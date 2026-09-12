// @vitest-environment jsdom

// The centre pane's Lane tab strip (`.tabstrip.lanebar` in
// `docs/viden-design/Viden/GUI/Viden - 桌面驾驶舱 (GUI).html`).
//
// The strip is the cockpit's answer to "which Lane am I talking to, and what
// else is open?". Every fact on it is a Core fact: the Lane's own recorded
// branch, the agent Core bound to it, the workspace budget Core published for
// the selected Lane, and the work mode Core resolved. Nothing is derived from
// the workspace when the Lane itself said nothing.
import { beforeEach, describe, expect, test, vi } from "vitest";

import { renderLaneTabs, type LaneTabsOptions } from "../src/components/lane_tabs";
import { renderD1Cockpit, type D1Intent, type D1IntentResult } from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

const SECOND_LANE = {
  id: "lane-review",
  role: "reviewer",
  status: "waiting_approval",
  summary: "Reviewing the patch",
  branch: "codex/lane-review",
};

const PROJECTION = {
  ...D1_PROJECTION,
  lanes: [...D1_PROJECTION.lanes, SECOND_LANE],
  agentSessions: [
    {
      sessionId: "session-review",
      laneId: "lane-review",
      agentId: "codex-acp",
      model: null,
      status: "running",
      task: "review",
      diagnostic: null,
    },
  ],
};

function tabs(overrides: Partial<LaneTabsOptions> = {}): HTMLElement {
  return renderLaneTabs({
    projection: PROJECTION,
    locale: "en",
    selectedLaneId: "lane-core",
    onSelectLane: vi.fn(),
    onCreateLane: vi.fn(),
    ...overrides,
  });
}

/**
 * `C5`'s per-Lane worktree source on the same strip.
 *
 * Two facts could not be shown before it: a Lane's *live* branch (the recorded
 * one is what Core wrote at creation) and that Lane's own ahead/behind. The
 * titlebar chips are the workspace root's position, so printing them here
 * would name one tree with another tree's numbers.
 */
describe("lane tab strip with C5 lane sources", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  const sampled = {
    status: "ready" as const,
    branch: "vd/retry-policy",
    worktree: "/workspace/viden/.worktrees/vd-retry-policy",
    ahead: 3,
    behind: 1,
    added: 4,
    deleted: 2,
    dirty: true,
  };

  test("a sampled worktree's branch wins over the recorded one and carries its position", () => {
    const strip = renderLaneTabs({
      projection: {
        ...PROJECTION,
        lanes: PROJECTION.lanes.map((lane) =>
          lane.id === "lane-review" ? { ...lane, source: sampled } : lane,
        ),
      },
      locale: "en",
      selectedLaneId: "lane-core",
      onSelectLane: vi.fn(),
      onCreateLane: vi.fn(),
    });
    const tab = strip.querySelector<HTMLElement>("[data-lane-tab='lane-review']")!;
    const branch = tab.querySelector<HTMLElement>("[data-lane-tab-branch]")!;
    expect(branch.textContent).toBe("vd/retry-policy");
    expect(branch.dataset.laneTabBranchSource).toBe("lane_sources");
    expect(tab.querySelector("[data-lane-tab-source]")?.textContent).toBe("↑3 ↓1 ●");
  });

  test("a Lane Core sampled nothing for keeps its recorded branch and no position", () => {
    const tab = tabs().querySelector<HTMLElement>("[data-lane-tab='lane-review']")!;
    const branch = tab.querySelector<HTMLElement>("[data-lane-tab-branch]")!;
    expect(branch.textContent).toBe("codex/lane-review");
    expect(branch.dataset.laneTabBranchSource).toBeUndefined();
    expect(tab.querySelector("[data-lane-tab-source]")).toBeNull();
  });

  test("an unavailable sample draws no position rather than zeroes", () => {
    const strip = renderLaneTabs({
      projection: {
        ...PROJECTION,
        lanes: PROJECTION.lanes.map((lane) =>
          lane.id === "lane-review"
            ? {
                ...lane,
                source: { ...sampled, status: "unavailable" as const, ahead: 0, behind: 0 },
              }
            : lane,
        ),
      },
      locale: "en",
      selectedLaneId: "lane-core",
      onSelectLane: vi.fn(),
      onCreateLane: vi.fn(),
    });
    const tab = strip.querySelector<HTMLElement>("[data-lane-tab='lane-review']")!;
    expect(tab.querySelector("[data-lane-tab-source]")).toBeNull();
  });
});

describe("lane tab strip", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("renders one tab per projected Lane with its own branch and bound agent", () => {
    const strip = tabs();

    const rows = Array.from(strip.querySelectorAll<HTMLElement>("[data-lane-tab]"));
    expect(rows.map((row) => row.dataset.laneTab)).toEqual(["lane-core", "lane-review"]);
    expect(rows[0]!.getAttribute("aria-selected")).toBe("true");
    expect(rows[1]!.getAttribute("aria-selected")).toBe("false");
    // Lane name and the Lane's *own* recorded branch.
    expect(rows[0]!.querySelector(".lnm")?.textContent).toBe("Streaming cockpit");
    expect(rows[0]!.querySelector(".lbr")?.textContent).toBe("codex/lane-core");
    expect(rows[1]!.querySelector(".lbr")?.textContent).toBe("codex/lane-review");
    // The agent chip is Core's binding; a Lane with no Agent session runs on
    // the built-in runtime, which the rail already names the same way.
    expect(rows[0]!.querySelector("[data-lane-tab-agent]")?.textContent).toBe("Viden Agent");
    expect(rows[1]!.querySelector("[data-lane-tab-agent]")?.textContent).toBe("codex-acp");
  });

  test("omits a branch the Lane did not record rather than borrowing the workspace's", () => {
    const strip = tabs({
      projection: {
        ...PROJECTION,
        lanes: [{ ...PROJECTION.lanes[0]!, branch: null }],
      },
    });

    const row = strip.querySelector<HTMLElement>("[data-lane-tab]")!;
    expect(row.querySelector(".lbr")).toBeNull();
    // The workspace really is on a branch; it must not leak onto the Lane.
    expect(row.textContent).not.toContain("codex/lane-core");
  });

  test("carries the project, the published budget and the work mode in the meta slot", () => {
    const strip = tabs({
      projection: {
        ...PROJECTION,
        contextDock: {
          ...PROJECTION.contextDock,
          context: {
            budgetId: "budget-1",
            usedTokens: 128_000,
            softTokenLimit: 160_000,
            hardTokenLimit: 200_000,
            remainingTokens: 72_000,
            exceeded: false,
          },
        },
      },
    });

    const meta = strip.querySelector<HTMLElement>("[data-lane-tab-meta]")!;
    expect(meta.querySelector("[data-lane-tab-project]")?.textContent).toBe("viden");
    expect(meta.querySelector("[data-lane-tab-context]")?.textContent).toBe("128k");
    expect(meta.querySelector("[data-lane-tab-mode]")?.textContent).toBe("Build");
  });

  test("leaves the context slot out when Core published no budget", () => {
    const meta = tabs().querySelector<HTMLElement>("[data-lane-tab-meta]")!;
    expect(meta.querySelector("[data-lane-tab-context]")).toBeNull();
    expect(meta.querySelector("[data-lane-tab-mode]")?.textContent).toBe("Build");
  });

  test("keeps the strip and its create affordance with no Lanes at all", () => {
    const strip = tabs({
      projection: { ...PROJECTION, lanes: [], agentSessions: [] },
      selectedLaneId: null,
    });

    expect(strip.querySelectorAll("[data-lane-tab]")).toHaveLength(0);
    expect(strip.querySelector("[data-lane-tab-empty]")?.textContent).toBe(
      "No Lanes yet",
    );
    expect(strip.querySelector("[data-lane-tab-add]")).not.toBeNull();
  });

  test("selecting a tab and pressing the add affordance reach the cockpit handlers", () => {
    const onSelectLane = vi.fn();
    const onCreateLane = vi.fn();
    const strip = tabs({ onSelectLane, onCreateLane });

    strip.querySelector<HTMLButtonElement>('[data-lane-tab="lane-review"]')!.click();
    expect(onSelectLane).toHaveBeenCalledWith("lane-review");
    strip.querySelector<HTMLButtonElement>("[data-lane-tab-add]")!.click();
    expect(onCreateLane).toHaveBeenCalledTimes(1);
  });
});

function setup(projection = PROJECTION) {
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
    // A bound secondary host, so `openCenterView` really switches the pane
    // instead of falling through to the shell's window route.
    secondaryViews: {
      mount: (route, container) => {
        container.textContent = route;
      },
    },
  });
  return { root, controller, send };
}

/// The Lane the strip marks current. The cockpit owns the selection locally
/// until Core republishes for it, so the strip — not the projection — is what
/// a switching test must read.
function currentTab(root: HTMLElement): string | undefined {
  return root.querySelector<HTMLElement>('[data-lane-tab][aria-selected="true"]')?.dataset.laneTab;
}

describe("lane switching in the cockpit", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("mounts the strip above the transcript and selects through the Lane rail path", () => {
    const { root, controller } = setup();

    const strip = root.querySelector<HTMLElement>("[data-lane-tabs]")!;
    expect(strip.previousElementSibling).toBeNull();
    expect(strip.nextElementSibling?.classList.contains("d1-transcript")).toBe(true);

    root.querySelector<HTMLButtonElement>('[data-lane-tab="lane-review"]')!.click();
    expect(currentTab(root)).toBe("lane-review");
    // The rail and the strip are two views of one selection.
    expect(
      root.querySelector('[data-lane-id="lane-review"]')?.getAttribute("aria-current"),
    ).toBe("true");
    controller.dispose();
  });

  test("Ctrl+Tab and Ctrl+Shift+Tab cycle the projected Lanes", () => {
    const { root, controller } = setup();

    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Tab", ctrlKey: true, bubbles: true }),
    );
    expect(currentTab(root)).toBe("lane-review");

    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Tab", ctrlKey: true, bubbles: true }),
    );
    expect(currentTab(root)).toBe("lane-core");

    window.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: "Tab",
        ctrlKey: true,
        shiftKey: true,
        bubbles: true,
      }),
    );
    expect(currentTab(root)).toBe("lane-review");
    controller.dispose();
  });

  test("the cycle stands down while an overlay owns the keyboard", () => {
    const { root, controller } = setup();

    root.querySelector<HTMLButtonElement>("[data-command-palette-toggle]")!.click();
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Tab", ctrlKey: true, bubbles: true }),
    );
    expect(currentTab(root)).toBe("lane-core");
    controller.dispose();
  });

  test("the strip is absent while a centre view owns the pane", () => {
    const { root, controller } = setup();

    controller.openCenterView("d2");
    expect(root.querySelector("[data-lane-tabs]")).toBeNull();
    controller.closeCenterView();
    expect(root.querySelector("[data-lane-tabs]")).not.toBeNull();
    controller.dispose();
  });
});
