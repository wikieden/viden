// @vitest-environment jsdom

// Creating a Lane from the palette and the `⌘L` chord, and routing the New
// Lane popover's "Full setup…" to the D4 wizard.
//
// The design binds `⌘L` to "New lane / delegate" in its own keyboard registry
// (`GUI/gui-settings.jsx` `SecKeyboard`) and draws the palette row as
// "Delegate task to new worktree… ⌘L". Both reach the same popover the rail's
// `＋` opens; none of them is a second creation path.
import { beforeEach, describe, expect, test, vi } from "vitest";

import { renderD1Cockpit, type D1Intent, type D1IntentResult } from "../src/screens/d1_cockpit";
import type { D1RenderOptions } from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

function setup(
  projection = D1_PROJECTION,
  options: Partial<D1RenderOptions> = {},
) {
  const root = document.createElement("div");
  document.body.append(root);
  const send = vi.fn(
    async (_intent: D1Intent): Promise<D1IntentResult> => ({
      projection,
      pendingCommandId: null,
      outcome: { state: "idle", reason: null },
    }),
  );
  const poll = vi.fn(
    async (): Promise<D1IntentResult> => ({
      projection,
      pendingCommandId: null,
      outcome: { state: "idle", reason: null },
    }),
  );
  const controller = renderD1Cockpit(root, projection, send, poll, undefined, undefined, {
    poll: false,
    showWelcome: false,
    ...options,
  });
  return { root, controller };
}

function paletteRow(id: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-palette-row][data-palette-item-id="${id}"]`);
}

describe("palette New Lane action", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("offers the design's row under the command scope and opens the popover", async () => {
    const { root, controller } = setup();

    root.querySelector<HTMLButtonElement>("[data-command-palette-toggle]")!.click();
    const input = document.querySelector<HTMLInputElement>("[data-palette-input]")!;
    input.value = ">";
    input.dispatchEvent(new Event("input", { bubbles: true }));

    const row = paletteRow("action:new-lane")!;
    expect(row).not.toBeNull();
    expect(row.textContent).toContain("Delegate task to new worktree…");
    expect(row.textContent).toContain("⌘L");
    expect(row.getAttribute("aria-disabled")).toBeNull();

    row.click();
    await vi.waitFor(() =>
      expect(root.querySelector("[data-new-lane-popover]")).not.toBeNull(),
    );
    // The palette closes behind it: one overlay at a time.
    expect(root.querySelector("[data-command-palette]")).toBeNull();
    controller.dispose();
  });

  test("states Core's own reason instead of offering a creation that fails", () => {
    const { root, controller } = setup({
      ...D1_PROJECTION,
      workspaceEligibility: {
        isGitRepository: false,
        hasHead: false,
        canCreateLane: false,
        diagnostic: "the workspace is not a Git repository",
      },
    });

    root.querySelector<HTMLButtonElement>("[data-command-palette-toggle]")!.click();
    const row = paletteRow("action:new-lane")!;
    expect(row.getAttribute("aria-disabled")).toBe("true");
    expect(row.textContent).toContain("the workspace is not a Git repository");
    controller.dispose();
  });
});

describe("the ⌘L chord", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("opens the New Lane popover from the cockpit", async () => {
    const { root, controller } = setup();

    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "l", metaKey: true, bubbles: true }),
    );
    await vi.waitFor(() =>
      expect(root.querySelector("[data-new-lane-popover]")).not.toBeNull(),
    );
    controller.dispose();
  });

  test("stands down when Core says this workspace cannot carry a Lane", () => {
    const { root, controller } = setup({
      ...D1_PROJECTION,
      workspaceEligibility: {
        isGitRepository: true,
        hasHead: false,
        canCreateLane: false,
        diagnostic: "the repository has no HEAD",
      },
    });

    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "l", ctrlKey: true, bubbles: true }),
    );
    expect(root.querySelector("[data-new-lane-popover]")).toBeNull();
    controller.dispose();
  });
});

describe("Full setup", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("hands the popover draft to the caller instead of discarding it", async () => {
    const onFullSetup = vi.fn();
    const { root, controller } = setup(D1_PROJECTION, { onFullSetup });

    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "l", metaKey: true, bubbles: true }),
    );
    await vi.waitFor(() =>
      expect(root.querySelector('[data-new-lane-popover][aria-busy="false"]')).not.toBeNull(),
    );
    const task = root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;
    task.value = "Refactor the config loader";
    task.dispatchEvent(new InputEvent("input", { bubbles: true }));
    root.querySelector<HTMLButtonElement>('[data-agent-id="codex-acp"]')!.click();
    root.querySelector<HTMLButtonElement>("[data-full-setup]")!.click();

    expect(onFullSetup).toHaveBeenCalledWith({
      agentId: "codex-acp",
      task: "Refactor the config loader",
    });
    controller.dispose();
  });

  test("reopens the popover on the draft the caller returns with", async () => {
    const { root, controller } = setup(D1_PROJECTION, {
      newLaneDraft: { agentId: "codex-acp", task: "Refactor the config loader" },
    });

    await vi.waitFor(() =>
      expect(root.querySelector('[data-new-lane-popover][aria-busy="false"]')).not.toBeNull(),
    );
    const popover = root.querySelector<HTMLElement>("[data-new-lane-popover]")!;
    expect(popover).not.toBeNull();
    expect(popover.querySelector<HTMLTextAreaElement>("[data-lane-task]")!.value).toBe(
      "Refactor the config loader",
    );
    expect(
      popover
        .querySelector<HTMLButtonElement>('[data-agent-id="codex-acp"]')!
        .getAttribute("aria-checked"),
    ).toBe("true");
    controller.dispose();
  });
});
