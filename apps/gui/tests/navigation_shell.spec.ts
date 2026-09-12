// @vitest-environment jsdom

/**
 * The G3 navigation shell.
 *
 * `D-RAILNAV` accepts the activity rail as the cockpit's router, and the
 * `0.3.4` plan resolves the question it left open in favour of the D1
 * flagship's own behaviour: the secondary D-screens render *inside* the
 * cockpit chrome rather than replacing it. These tests pin the four halves of
 * that shell — one ordered router table, centre-pane views with a return path,
 * the `D-SIDEBAR` Lane sidebar modes, and the `D-STATUSBAR` config gear.
 */

import { beforeEach, describe, expect, test, vi } from "vitest";

import {
  D1_RAIL_ROUTES,
  renderActivityRail,
  type D1RailDestination,
} from "../src/components/activity_rail";
import {
  renderD1Cockpit,
  type D1CockpitProjection,
  type D1Controller,
  type D1IntentResult,
  type D1RenderOptions,
} from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

function projection(overrides: Partial<D1CockpitProjection> = {}): D1CockpitProjection {
  return { ...structuredClone(D1_PROJECTION), ...overrides };
}

function idle(next: D1CockpitProjection): D1IntentResult {
  return { projection: next, pendingCommandId: null, outcome: { state: "idle", reason: null } };
}

/** A secondary-view host that records what the cockpit asked it to mount. */
function secondaryHost(): {
  port: NonNullable<D1RenderOptions["secondaryViews"]>;
  mounted: Array<{ route: string; arg: string | null }>;
} {
  const mounted: Array<{ route: string; arg: string | null }> = [];
  return {
    mounted,
    port: {
      mount: (route, container, arg) => {
        mounted.push({ route, arg });
        const stage = document.createElement("section");
        stage.dataset.route = route;
        stage.textContent = `${route} screen`;
        container.replaceChildren(stage);
      },
    },
  };
}

function mount(
  options: D1RenderOptions = {},
  initial: D1CockpitProjection = projection(),
): { root: HTMLElement; controller: D1Controller } {
  document.body.innerHTML = '<main id="app"></main>';
  const root = document.querySelector<HTMLElement>("#app")!;
  const controller = renderD1Cockpit(
    root,
    initial,
    vi.fn(async () => idle(initial)),
    vi.fn(async () => idle(initial)),
    undefined,
    undefined,
    { poll: false, ...options },
  );
  return { root, controller };
}

const flush = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0));

describe("the activity rail is one ordered router table", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("lists the design's destinations top to bottom, settings last", () => {
    expect(D1_RAIL_ROUTES.map((slot) => slot.id)).toEqual([
      "conversation",
      "lanes",
      "review",
      "evidence",
      "d2",
      "d10",
      "d12",
      "d13",
      "d14",
    ]);
    // Registered `GUI/gui-icons.jsx` glyphs only, one per slot.
    const icons = D1_RAIL_ROUTES.map((slot) => slot.icon);
    expect(new Set(icons).size).toBe(icons.length);
    expect(icons).toEqual([
      "chat",
      "lanes",
      "review",
      "evidence",
      "decide",
      "diagnostics",
      "worktree",
      "fleet",
      "brief",
    ]);
  });

  test("renders every slot in that order and closes with the settings gear", () => {
    const rail = renderActivityRail("en", {
      lanesAvailable: true,
      lanesOpen: false,
      onToggleLanes: () => undefined,
    });

    expect(
      Array.from(rail.querySelectorAll<HTMLElement>("button"), (button) =>
        button.dataset.railSlot ?? (button.dataset.settingsToggle ? "settings" : "?"),
      ),
    ).toEqual([
      "conversation",
      "lanes",
      "review",
      "evidence",
      "d2",
      "d10",
      "d12",
      "d13",
      "d14",
      "settings",
    ]);
  });

  test("marks the destination that is showing and leaves the others plain", () => {
    const rail = renderActivityRail("en", {
      lanesAvailable: true,
      lanesOpen: false,
      onToggleLanes: () => undefined,
      onOpenDestination: () => undefined,
      destinations: {
        d2: { available: true, current: true },
        d14: { available: true, current: false },
      },
    });

    expect(rail.querySelector('[data-rail-slot="d2"]')?.getAttribute("aria-current")).toBe(
      "page",
    );
    expect(rail.querySelector('[data-rail-slot="d14"]')?.getAttribute("aria-current")).toBeNull();
    // Conversation is only current while the transcript owns the centre pane.
    expect(
      rail.querySelector('[data-rail-slot="conversation"]')?.getAttribute("aria-current"),
    ).toBeNull();
  });

  test("a destination with no capability is disabled and says which one is missing", () => {
    const rail = renderActivityRail("en", {
      lanesAvailable: true,
      lanesOpen: false,
      onToggleLanes: () => undefined,
      onOpenDestination: () => undefined,
      destinations: {
        review: { available: false, current: false, note: "runtime.structured_diff" },
      },
    });

    const slot = rail.querySelector<HTMLButtonElement>('[data-rail-slot="review"]')!;
    expect(slot.disabled).toBe(true);
    expect(slot.getAttribute("aria-label")).toContain("runtime.structured_diff");
    expect(slot.title).toContain("runtime.structured_diff");
    // A disabled slot is not a route: nothing may claim it navigates.
    expect(slot.dataset.railRoute).toBeUndefined();
  });

  test("carries only counts Core published, on the Decisions slot", () => {
    const rail = renderActivityRail("en", {
      lanesAvailable: true,
      lanesOpen: false,
      onToggleLanes: () => undefined,
      onOpenDestination: () => undefined,
      destinations: {
        d2: { available: true, current: false, badge: 2 },
        d10: { available: true, current: false },
      },
    });

    expect(rail.querySelector('[data-rail-slot="d2"] [data-rail-badge]')?.textContent).toBe("2");
    expect(rail.querySelector('[data-rail-slot="d10"] [data-rail-badge]')).toBeNull();
  });
});

describe("secondary screens are centre-pane views with the chrome retained", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("mounting D2 keeps the titlebar, rails, composer and statusbar", async () => {
    const host = secondaryHost();
    const { root, controller } = mount({ secondaryViews: host.port });

    root.querySelector<HTMLButtonElement>('[data-rail-slot="d2"]')!.click();
    await flush();

    expect(host.mounted).toEqual([{ route: "d2", arg: null }]);
    expect(root.querySelector('[data-secondary-view="d2"] [data-route="d2"]')).not.toBeNull();
    for (const landmark of [
      "topbar",
      "activity-rail",
      "lane-rail",
      "context-dock",
      "statusbar",
    ]) {
      expect(
        root.querySelector(`[data-shell-landmark="${landmark}"]`),
        landmark,
      ).not.toBeNull();
    }
    // The composer stays addressed to the selected Lane while the view is open.
    expect(root.querySelector("[data-composer]")).not.toBeNull();
    expect(root.querySelector('#d1-lane-rail [aria-current="true"]')?.textContent).toContain(
      "lane-core",
    );
    controller.dispose();
  });

  test("the centre view is named on the frame and the rail marks it", async () => {
    const host = secondaryHost();
    const { root, controller } = mount({ secondaryViews: host.port });
    const frame = (): HTMLElement => root.querySelector<HTMLElement>('[data-screen="d1-cockpit"]')!;

    expect(frame().dataset.centerView).toBe("transcript");
    controller.openCenterView("d14", "evidence:ev-1");
    await flush();

    expect(frame().dataset.centerView).toBe("d14");
    expect(host.mounted).toEqual([{ route: "d14", arg: "evidence:ev-1" }]);
    expect(
      root.querySelector('[data-rail-slot="d14"]')?.getAttribute("aria-current"),
    ).toBe("page");
    controller.dispose();
  });

  test("the Close control and a second press of the same rail slot both return", async () => {
    const host = secondaryHost();
    const { root, controller } = mount({ secondaryViews: host.port });
    const frame = (): HTMLElement => root.querySelector<HTMLElement>('[data-screen="d1-cockpit"]')!;

    root.querySelector<HTMLButtonElement>('[data-rail-slot="d13"]')!.click();
    await flush();
    root.querySelector<HTMLButtonElement>("[data-secondary-close]")!.click();
    await flush();
    expect(frame().dataset.centerView).toBe("transcript");
    expect(root.querySelector(".d1-transcript")).not.toBeNull();

    root.querySelector<HTMLButtonElement>('[data-rail-slot="d13"]')!.click();
    await flush();
    expect(frame().dataset.centerView).toBe("d13");
    root.querySelector<HTMLButtonElement>('[data-rail-slot="d13"]')!.click();
    await flush();
    expect(frame().dataset.centerView).toBe("transcript");
    controller.dispose();
  });

  test("a host rejection states Core's words and never leaves a blank pane", async () => {
    const { root, controller } = mount({
      secondaryViews: {
        mount: async () => {
          throw new Error("Core refused the decision read");
        },
      },
    });

    controller.openCenterView("d2");
    await flush();
    await flush();

    const alert = root.querySelector<HTMLElement>("[data-secondary-error]");
    expect(alert?.getAttribute("role")).toBe("alert");
    expect(alert?.textContent).toContain("Core refused the decision read");
    controller.dispose();
  });

  test("without a secondary host the rail falls back to the shell's own route", () => {
    const onNavigate = vi.fn();
    const { root, controller } = mount({ onNavigate });

    root.querySelector<HTMLButtonElement>('[data-rail-slot="d12"]')!.click();
    expect(onNavigate).toHaveBeenCalledExactlyOnceWith("d12");
    controller.dispose();
  });

  test("the statusbar gate segment lands on the in-cockpit decision queue", async () => {
    const host = secondaryHost();
    const onNavigate = vi.fn();
    const { root, controller } = mount(
      { secondaryViews: host.port, onNavigate },
      projection({
        statusbar: { ...D1_PROJECTION.statusbar, pendingGateCount: 2 },
      }),
    );

    root.querySelector<HTMLButtonElement>("[data-sb-gate]")!.click();
    await flush();

    expect(host.mounted).toEqual([{ route: "d2", arg: null }]);
    expect(onNavigate).not.toHaveBeenCalled();
    controller.dispose();
  });
});

describe("the return path", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  const escape = (): void => {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  };

  test("Esc returns to the transcript when the view owns the centre pane", async () => {
    const host = secondaryHost();
    const { root, controller } = mount({ secondaryViews: host.port });
    const frame = (): HTMLElement => root.querySelector<HTMLElement>('[data-screen="d1-cockpit"]')!;

    controller.openCenterView("d10");
    await flush();
    escape();
    await flush();

    expect(frame().dataset.centerView).toBe("transcript");
    controller.dispose();
  });

  test("Esc stays with the cancel-turn binding while a turn is cancellable", async () => {
    // The Live Work strip — and with it the cancel affordance Esc is named
    // after — lives inside the transcript, so this is the view where the two
    // bindings could contend. The cancel still wins, and nothing else fires.
    const host = secondaryHost();
    const busy = projection({
      composer: {
        editable: true,
        busy: true,
        canCancel: true,
        canSubmitImmediately: false,
      },
    });
    const send = vi.fn(async () => idle(busy));
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const controller = renderD1Cockpit(
      root,
      busy,
      send,
      vi.fn(async () => idle(busy)),
      undefined,
      undefined,
      { poll: false, secondaryViews: host.port },
    );
    const frame = (): HTMLElement => root.querySelector<HTMLElement>('[data-screen="d1-cockpit"]')!;

    expect(root.querySelector("[data-work-cancel]")).not.toBeNull();
    escape();
    await flush();

    expect(send).toHaveBeenCalledWith({ type: "cancel", laneId: "lane-core" });
    expect(frame().dataset.centerView).toBe("transcript");
    controller.dispose();
  });

  test("Esc does not close the view while the command palette owns it", async () => {
    const host = secondaryHost();
    const { root, controller } = mount({ secondaryViews: host.port });
    const frame = (): HTMLElement => root.querySelector<HTMLElement>('[data-screen="d1-cockpit"]')!;

    controller.openCenterView("d13");
    await flush();
    root.querySelector<HTMLButtonElement>("[data-command-palette-toggle]")!.click();
    await flush();
    expect(document.querySelector("[data-command-palette]")).not.toBeNull();

    escape();
    await flush();
    expect(frame().dataset.centerView).toBe("d13");
    controller.dispose();
  });

  test("⌘G opens the decision queue", async () => {
    const host = secondaryHost();
    const { root, controller } = mount({ secondaryViews: host.port });
    const frame = (): HTMLElement => root.querySelector<HTMLElement>('[data-screen="d1-cockpit"]')!;

    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "g", metaKey: true, bubbles: true }),
    );
    await flush();

    expect(frame().dataset.centerView).toBe("d2");
    expect(host.mounted).toEqual([{ route: "d2", arg: null }]);
    controller.dispose();
  });
});

describe("rail destinations reflect the capabilities the cockpit actually has", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("review and evidence are disabled and labelled without their capability", async () => {
    const { root, controller } = mount({
      workspaceDiff: {
        read: async () => ({
          outcome: { state: "idle" as const, reason: null },
          targetLaneId: null,
          source: null,
          entries: [],
          truncated: false,
          loaded: false,
          pendingCommandId: null,
          capabilityAvailable: false,
          stale: false,
        }),
        query: async () => {
          throw new Error("not reached");
        },
      },
    });
    await flush();

    const review = root.querySelector<HTMLButtonElement>('[data-rail-slot="review"]')!;
    expect(review.disabled).toBe(true);
    expect(review.getAttribute("aria-label")).toContain("runtime.structured_diff");

    const evidence = root.querySelector<HTMLButtonElement>('[data-rail-slot="evidence"]')!;
    expect(evidence.disabled).toBe(true);
    expect(evidence.getAttribute("aria-label")).toContain("runtime.evidence_reads");
    controller.dispose();
  });

  test("the audit slot states the raw-replay fallback rather than a capability", () => {
    const host = secondaryHost();
    const { root, controller } = mount({ secondaryViews: host.port });
    const audit = root.querySelector<HTMLButtonElement>('[data-rail-slot="d14"]')!;

    expect(audit.disabled).toBe(false);
    expect(audit.title).toContain("raw event replay");
    controller.dispose();
  });

  test("a rail destination that opens a centre view carries the review state too", async () => {
    const diff = {
      outcome: { state: "idle" as const, reason: null },
      targetLaneId: null,
      source: null,
      entries: [],
      truncated: false,
      loaded: true,
      pendingCommandId: null,
      capabilityAvailable: true,
      stale: false,
    };
    const { root, controller } = mount({
      workspaceDiff: { read: async () => diff, query: async () => diff },
    });
    await flush();

    const review = root.querySelector<HTMLButtonElement>('[data-rail-slot="review"]')!;
    expect(review.disabled).toBe(false);
    review.click();
    await flush();

    expect(
      root.querySelector<HTMLElement>('[data-screen="d1-cockpit"]')!.dataset.centerView,
    ).toBe("review");
    expect(root.querySelector('[data-rail-slot="review"]')?.getAttribute("aria-current")).toBe(
      "page",
    );
    controller.dispose();
  });
});

describe("the rail destination type stays closed over the router table", () => {
  test("every destination slot names a centre view", () => {
    const destinations: D1RailDestination[] = D1_RAIL_ROUTES.flatMap((slot) =>
      slot.kind === "destination" && slot.destination ? [slot.destination] : [],
    );
    expect(destinations).toEqual(["review", "evidence", "d2", "d10", "d12", "d13", "d14"]);
  });
});
