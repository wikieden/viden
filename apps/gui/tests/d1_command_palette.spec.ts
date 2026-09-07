// @vitest-environment jsdom

/**
 * The command palette inside the cockpit.
 *
 * The palette is mounted from D1 — the level the design mounts it at — because
 * every action it offers is a cockpit action: selecting a Lane, focusing the
 * composer, opening the Settings overlay, or handing a route plus its exact
 * Core id to the shell. These tests pin the shortcuts, the in-cockpit
 * selection path, the fail-soft cross-Lane read, and the Escape boundary
 * against the cockpit's own cancel-turn binding.
 */

import { afterEach, describe, expect, test, vi } from "vitest";

import {
  renderD1Cockpit,
  type D1CockpitProjection,
  type D1Controller,
  type D1IntentResult,
} from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

let controller: D1Controller | null = null;

afterEach(() => {
  controller?.dispose();
  controller = null;
  document.body.replaceChildren();
});

interface MountOptions {
  projection?: D1CockpitProjection;
  onNavigate?: (route: string, arg?: string) => void;
  send?: (intent: unknown) => Promise<D1IntentResult>;
  loadPaletteCrossLane?: () => Promise<{
    gates: Array<{ gateId: string; taskId: string; status: string; dormant: boolean }>;
    asks: Array<{ id: string; title: string; kind: string; laneId: string | null }>;
  }>;
  preferences?: boolean;
}

function idle(projection: D1CockpitProjection): D1IntentResult {
  return { projection, pendingCommandId: null, outcome: { state: "idle", reason: null } };
}

function mount(options: MountOptions = {}): HTMLElement {
  const root = document.createElement("div");
  document.body.replaceChildren(root);
  const projection = options.projection ?? structuredClone(D1_PROJECTION);
  controller = renderD1Cockpit(
    root,
    projection,
    (options.send ?? (async () => idle(projection))) as never,
    async () => idle(projection),
    undefined,
    undefined,
    {
      poll: false,
      onNavigate: options.onNavigate ?? (() => undefined),
      loadPaletteCrossLane: options.loadPaletteCrossLane,
      preferences: options.preferences
        ? {
            isAvailable: async () => true,
            save: async () => ({ status: "unavailable", capability: "x" }) as never,
            restore: async () => ({ status: "unavailable", capability: "x" }) as never,
          }
        : undefined,
    },
  );
  return root;
}

function palette(): HTMLElement | null {
  return document.querySelector<HTMLElement>("[data-command-palette]");
}

function toggle(): HTMLButtonElement {
  return document.querySelector<HTMLButtonElement>("[data-command-palette-toggle]")!;
}

function input(): HTMLInputElement {
  return document.querySelector<HTMLInputElement>("[data-palette-input]")!;
}

function rows(): HTMLElement[] {
  return Array.from(document.querySelectorAll<HTMLElement>("[data-palette-row]"));
}

function type(value: string): void {
  const field = input();
  field.value = value;
  field.dispatchEvent(new Event("input", { bubbles: true }));
}

function shortcut(key: string, modifiers: Partial<KeyboardEventInit> = {}): void {
  window.dispatchEvent(
    new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...modifiers }),
  );
}

const tick = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0));

describe("cockpit command palette", () => {
  test("the titlebar carries the design's palette toggle", () => {
    mount();
    const button = toggle();
    expect(button.className).toContain("tbtbtn");
    expect(button.title).toContain("⌘K");
    expect(button.getAttribute("aria-haspopup")).toBe("dialog");
    expect(button.getAttribute("aria-expanded")).toBe("false");
  });

  test("the toggle opens and closes the overlay", async () => {
    mount();
    toggle().click();
    await tick();
    expect(palette()).not.toBeNull();
    expect(toggle().getAttribute("aria-expanded")).toBe("true");
    toggle().click();
    expect(palette()).toBeNull();
  });

  test("Cmd+K and Ctrl+K both toggle the palette", async () => {
    mount();
    shortcut("k", { metaKey: true });
    await tick();
    expect(palette()).not.toBeNull();
    shortcut("k", { metaKey: true });
    expect(palette()).toBeNull();
    shortcut("k", { ctrlKey: true });
    await tick();
    expect(palette()).not.toBeNull();
  });

  test("an unmodified k never opens the palette", async () => {
    mount();
    shortcut("k");
    await tick();
    expect(palette()).toBeNull();
  });

  test("Ctrl+P opens the palette pre-scoped to commands", async () => {
    mount();
    shortcut("p", { ctrlKey: true });
    await tick();
    expect(input().value).toBe(">");
    expect(rows().every((row) => (row.dataset.paletteItemId ?? "").startsWith("action:"))).toBe(
      true,
    );
  });

  test("typing in the composer does not swallow the shortcut", async () => {
    const root = mount();
    const composer = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
    composer.focus();
    shortcut("k", { metaKey: true });
    await tick();
    expect(palette()).not.toBeNull();
    // The overlay is modal: opening it over a focused composer must take the
    // caret, or the operator's next keystroke lands in the transcript input.
    expect(document.activeElement).toBe(input());
  });

  test("a Core refresh while the palette is open does not steal the caret back", async () => {
    const projection = structuredClone(D1_PROJECTION);
    const root = mount({ projection });
    root.querySelector<HTMLTextAreaElement>("[data-composer]")!.focus();
    shortcut("k", { metaKey: true });
    await tick();
    type(":lane");
    const refreshed = structuredClone(D1_PROJECTION);
    refreshed.statusbar.eventStreamPosition = 99;
    controller!.applyProjection(refreshed);
    expect(document.activeElement).toBe(input());
    expect(input().value).toBe(":lane");
  });

  test("a Lane row selects the Lane in place and returns focus to the composer", async () => {
    const twoLanes = structuredClone(D1_PROJECTION);
    twoLanes.lanes.push({
      id: "lane-docs",
      role: "reviewer",
      status: "idle",
      summary: "Docs pass",
      branch: "codex/lane-docs",
    });
    const root = mount({ projection: twoLanes });
    shortcut("k", { metaKey: true });
    await tick();
    type(":lane-docs");
    expect(rows()).toHaveLength(1);
    rows()[0]!.click();
    await tick();
    expect(palette()).toBeNull();
    expect(
      root.querySelector('[data-lane-id="lane-docs"]')?.getAttribute("aria-current"),
    ).toBe("true");
    expect(document.activeElement).toBe(root.querySelector("[data-composer]"));
  });

  test("a gate row hands D12 the exact gate id", async () => {
    const onNavigate = vi.fn();
    mount({
      onNavigate,
      loadPaletteCrossLane: async () => ({
        gates: [{ gateId: "gate-7", taskId: "task-core", status: "blocked", dormant: false }],
        asks: [],
      }),
    });
    shortcut("k", { metaKey: true });
    await tick();
    await tick();
    type("#gate-7");
    rows()[0]!.click();
    expect(onNavigate).toHaveBeenCalledExactlyOnceWith("d12", "gate-7");
  });

  test("a failed cross-Lane read degrades the section without blocking the palette", async () => {
    mount({
      loadPaletteCrossLane: async () => {
        throw new Error("Core did not answer the decision read");
      },
    });
    shortcut("k", { metaKey: true });
    await tick();
    await tick();
    expect(palette()).not.toBeNull();
    const note = rows().find((row) => row.dataset.paletteItemId === "jump:cross-lane-unavailable");
    expect(note?.getAttribute("aria-disabled")).toBe("true");
    expect(note?.textContent).toContain("Core did not answer the decision read");
    // Lanes from the projection the cockpit already holds still resolve.
    type(":lane-core");
    expect(rows()).toHaveLength(1);
  });

  test("without a cross-Lane reader the section states that it is unavailable", async () => {
    mount();
    shortcut("k", { metaKey: true });
    await tick();
    expect(
      rows().some((row) => row.dataset.paletteItemId === "jump:cross-lane-unavailable"),
    ).toBe(true);
  });

  test("Escape closes the palette and never cancels the running turn", async () => {
    const send = vi.fn(async () => idle(structuredClone(D1_PROJECTION)));
    const root = mount({ send: send as never });
    // The cockpit's Escape binding is live: the strip is offering a cancel.
    expect(root.querySelector("[data-work-cancel]")).not.toBeNull();
    shortcut("k", { metaKey: true });
    await tick();
    input().dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }),
    );
    await tick();
    expect(palette()).toBeNull();
    expect(send).not.toHaveBeenCalled();
    // With the palette closed, Escape reaches the cancel binding again.
    shortcut("Escape");
    await tick();
    expect(send).toHaveBeenCalledOnce();
  });

  test("closing returns focus to the titlebar toggle", async () => {
    mount();
    toggle().click();
    await tick();
    input().dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }),
    );
    expect(document.activeElement).toBe(toggle());
  });

  test("the settings row opens the Settings overlay", async () => {
    mount({ preferences: true });
    shortcut("k", { metaKey: true });
    await tick();
    type("open settings");
    rows()[0]!.click();
    await tick();
    await tick();
    expect(document.querySelector("[data-settings-panel]")).not.toBeNull();
  });

  test("disposing the cockpit removes the palette and its shortcut", async () => {
    mount();
    shortcut("k", { metaKey: true });
    await tick();
    expect(palette()).not.toBeNull();
    controller?.dispose();
    controller = null;
    expect(palette()).toBeNull();
    shortcut("k", { metaKey: true });
    await tick();
    expect(palette()).toBeNull();
  });
});
