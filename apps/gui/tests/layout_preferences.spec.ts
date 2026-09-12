// @vitest-environment jsdom

/**
 * The cockpit's side of the Core layout record (`ui.layout_preferences`, C5).
 *
 * G3 shipped the Lane sidebar mode and the statusbar's ambient set as
 * in-memory presentation state, with the seam written down because the
 * frontend contract makes Core the single preference authority. These tests
 * pin the four rules that closing that seam has to keep:
 *
 * 1. **Core's record is what is drawn.** A published `pinned` mode renders the
 *    pinned column even though the caller passed the design default, and the
 *    cockpit holds no copy: the next read wins.
 * 2. **A pin writes a patch, and only the axis that changed.** The rendered
 *    mode moves on Core's answer, not on the click.
 * 3. **`persisted: false` is visible where the change was made.** An applied
 *    but unwritten record says so on the pin and in the config popover, rather
 *    than coming back rearranged after a restart.
 * 4. **An absent capability keeps the session-local toggle and names itself.**
 *    Never a client-side write, and never a dead control.
 */

import { describe, expect, test, vi } from "vitest";

import { renderD1Cockpit, type D1Intent, type D1IntentResult } from "../src/screens/d1_cockpit";
import {
  IDLE_LAYOUT_PREFERENCES,
  type LayoutPreferencePatch,
  type LayoutPreferencesProjection,
} from "../src/models/layout_preferences";
import { D1_PROJECTION } from "./support/d1_projection";

function record(
  overrides: Partial<LayoutPreferencesProjection> = {},
): LayoutPreferencesProjection {
  return {
    ...IDLE_LAYOUT_PREFERENCES,
    capabilityAvailable: true,
    laneSidebarMode: "floating",
    ...overrides,
  };
}

interface Harness {
  root: HTMLElement;
  patches: LayoutPreferencePatch[];
  reads: number;
}

function mount(
  answers: LayoutPreferencesProjection[],
  onSet?: (patch: LayoutPreferencePatch) => LayoutPreferencesProjection,
): Harness {
  document.body.innerHTML = '<div id="host"></div>';
  const root = document.querySelector<HTMLElement>("#host")!;
  const result: D1IntentResult = {
    projection: D1_PROJECTION,
    pendingCommandId: null,
    outcome: { state: "confirmed" as const, reason: null },
  };
  const harness: Harness = { root, patches: [], reads: 0 };
  let current = answers[0] ?? IDLE_LAYOUT_PREFERENCES;
  renderD1Cockpit(
    root,
    D1_PROJECTION,
    vi.fn(async (_intent: D1Intent) => result),
    vi.fn(async () => result),
    undefined,
    undefined,
    {
      poll: false,
      onNavigate: () => undefined,
      layout: {
        read: async () => {
          const answer = answers[Math.min(harness.reads, answers.length - 1)] ?? current;
          harness.reads += 1;
          current = answer;
          return answer;
        },
        set: async (patch) => {
          harness.patches.push(patch);
          current = onSet ? onSet(patch) : current;
          return current;
        },
      },
    },
  );
  return harness;
}

async function settle(hops = 8): Promise<void> {
  for (let hop = 0; hop < hops; hop += 1) await Promise.resolve();
}

describe("the Core-owned cockpit layout record", () => {
  test("renders the published mode rather than the caller's default", async () => {
    const harness = mount([record({ laneSidebarMode: "pinned" })]);
    await settle();
    expect(
      harness.root.querySelector<HTMLElement>("[data-lane-sidebar-pin]")?.dataset.laneSidebarPin,
    ).toBe("pinned");
    expect(harness.reads).toBeGreaterThan(0);
  });

  test("hidden segments come from the record, and unknown names are left in it", async () => {
    const harness = mount([
      record({ hiddenStatusbarSegments: ["latency", "req", "a-newer-client-segment"] }),
    ]);
    await settle();
    expect(harness.root.querySelector("[data-sb-segment='latency']")).toBeNull();
    expect(harness.root.querySelector("[data-sb-segment='req']")).toBeNull();
    expect(harness.root.querySelector("[data-sb-segment='events']")).not.toBeNull();

    // Hiding one more must carry the name this build does not know through
    // untouched, or the next write would unhide a newer client's segment.
    harness.root.querySelector<HTMLButtonElement>("[data-sb-config-toggle]")?.click();
    await settle();
    harness.root.querySelector<HTMLButtonElement>("[data-sb-config-item='diag']")?.click();
    await settle();
    expect(harness.patches).toHaveLength(1);
    expect(harness.patches[0].hiddenStatusbarSegments).toEqual([
      "latency",
      "diag",
      "req",
      "a-newer-client-segment",
    ]);
    expect(harness.patches[0].laneSidebarMode).toBeUndefined();
  });

  test("the pin sends one axis and adopts Core's answer", async () => {
    const harness = mount([record({ laneSidebarMode: "floating" })], (patch) =>
      record({ laneSidebarMode: patch.laneSidebarMode ?? "floating", persisted: true }),
    );
    await settle();
    harness.root.querySelector<HTMLButtonElement>("[data-lane-sidebar-pin]")?.click();
    await settle();
    expect(harness.patches).toEqual([{ laneSidebarMode: "pinned" }]);
    expect(
      harness.root.querySelector<HTMLElement>("[data-lane-sidebar-pin]")?.dataset.laneSidebarPin,
    ).toBe("pinned");
  });

  test("a record Core could not write says so on the pin and in the popover", async () => {
    const harness = mount([record()], () =>
      record({ laneSidebarMode: "pinned", persisted: false }),
    );
    await settle();
    harness.root.querySelector<HTMLButtonElement>("[data-lane-sidebar-pin]")?.click();
    await settle();
    const pin = harness.root.querySelector<HTMLElement>("[data-lane-sidebar-pin]")!;
    expect(pin.dataset.laneSidebarNote).toBe("true");
    expect(pin.title).toContain("could not write it");

    harness.root.querySelector<HTMLButtonElement>("[data-sb-config-toggle]")?.click();
    await settle();
    expect(
      harness.root.querySelector<HTMLElement>("[data-sb-config-note]")?.textContent,
    ).toContain("could not write it");
  });

  test("an unmodelled mode falls back to the design default rather than a guess", async () => {
    const harness = mount([record({ laneSidebarMode: "unknown" })]);
    await settle();
    expect(
      harness.root.querySelector<HTMLElement>("[data-lane-sidebar-pin]")?.dataset.laneSidebarPin,
    ).toBe("floating");
  });

  test("without the capability the toggle stays session-local and names it", async () => {
    const harness = mount([IDLE_LAYOUT_PREFERENCES]);
    await settle();
    harness.root.querySelector<HTMLButtonElement>("[data-lane-sidebar-pin]")?.click();
    await settle();
    expect(harness.patches).toHaveLength(0);
    const pin = harness.root.querySelector<HTMLElement>("[data-lane-sidebar-pin]")!;
    expect(pin.dataset.laneSidebarPin).toBe("pinned");
    expect(pin.title).toContain("ui.layout_preferences");
  });
});
