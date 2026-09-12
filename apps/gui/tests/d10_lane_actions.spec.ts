// @vitest-environment jsdom

import { describe, expect, test, vi } from "vitest";

import {
  renderD10LaneMonitor,
  type D10Lane,
  type D10LaneActions,
  type D10LaneMonitorProjection,
} from "../src/screens/d10_lane_monitor";

/**
 * D10's card action row (`G6`).
 *
 * The design draws Attach / Pause / Kill on every Lane card. This contract has
 * a Lane selection (not a Core command at all) and `CancelAgentSession`; it has
 * no pause and no kill. The tests below pin the three properties that keeps
 * honest: the two real controls do exactly what they say, the two missing ones
 * stay visible and disabled with the gap named, and a Stop outcome is Core's
 * own answer rather than an assumed success.
 */

function lane(overrides: Partial<D10Lane> = {}): D10Lane {
  return {
    id: "lane-1",
    projectId: "project-boss-rush",
    summary: "gameplay · jump feel",
    role: "coder",
    route: "acp",
    gateStrength: "cooperative",
    mutationPolicy: "propose_only",
    status: "running",
    awaitsHuman: false,
    branch: "codex/lane-1",
    worktree: ".worktrees/lane-1",
    progress: 64,
    agents: [
      { sessionId: "session-1", agentId: "codex", model: "gpt-5-codex", status: "running" },
    ],
    evidence: [],
    tokenLimit: null,
    costLimitMicroUsd: null,
    costMeterability: "metered",
    runStats: null,
    ...overrides,
  };
}

function projectionOf(lanes: D10Lane[]): D10LaneMonitorProjection {
  return {
    totalLanes: lanes.length,
    totalProjects: 1,
    awaitingTotal: 0,
    lanes,
    unavailable: [],
  };
}

function setup(lanes: D10Lane[], actions?: D10LaneActions) {
  document.body.innerHTML = '<div id="host"></div>';
  const root = document.querySelector<HTMLElement>("#host")!;
  renderD10LaneMonitor(root, projectionOf(lanes), "en", undefined, null, actions);
  return root;
}

const flush = async (): Promise<void> => {
  await Promise.resolve();
  await Promise.resolve();
};

describe("D10 lane card actions", () => {
  test("Attach selects the Lane through the host rather than sending a command", () => {
    const attach = vi.fn();
    const stop = vi.fn();
    const root = setup([lane()], { attach, stop });

    root.querySelector<HTMLButtonElement>("[data-d10-lane='lane-1'] [data-d10-action='attach']")!
      .click();

    expect(attach).toHaveBeenCalledWith("lane-1");
    expect(stop).not.toHaveBeenCalled();
  });

  test("Stop sends the Lane's own Agent session id and reports Core's acceptance", async () => {
    const stop = vi.fn(async () => ({ state: "confirmed", reason: null }));
    const root = setup([lane()], { attach: vi.fn(), stop });
    const button = root.querySelector<HTMLButtonElement>(
      "[data-d10-lane='lane-1'] [data-d10-action='stop']",
    )!;

    expect(button.disabled).toBe(false);
    expect(button.dataset.d10StopSession).toBe("session-1");
    button.click();
    await flush();

    expect(stop).toHaveBeenCalledWith("lane-1", "session-1");
    const outcome = root.querySelector<HTMLElement>("[data-d10-stop-outcome]")!;
    expect(outcome.dataset.d10StopOutcome).toBe("confirmed");
    expect(outcome.textContent).toContain("Core accepted the stop command.");
  });

  test("a refused Stop shows Core's own reason, never a success", async () => {
    const stop = vi.fn(async () => ({
      state: "rejected",
      reason: "agent session `session-1` is not cancellable",
    }));
    const root = setup([lane()], { attach: vi.fn(), stop });

    root.querySelector<HTMLButtonElement>("[data-d10-lane='lane-1'] [data-d10-action='stop']")!
      .click();
    await flush();

    const outcome = root.querySelector<HTMLElement>("[data-d10-stop-outcome]")!;
    expect(outcome.dataset.d10StopOutcome).toBe("rejected");
    expect(outcome.textContent).toContain("is not cancellable");
  });

  test("a Lane with no Agent session disables Stop and names the reason", () => {
    const root = setup([lane({ agents: [] })], { attach: vi.fn(), stop: vi.fn() });
    const button = root.querySelector<HTMLButtonElement>(
      "[data-d10-lane='lane-1'] [data-d10-action='stop']",
    )!;

    expect(button.disabled).toBe(true);
    expect(button.getAttribute("aria-label")).toContain("Core published no Agent session");
    expect(button.dataset.d10StopSession).toBeUndefined();
  });

  test("more than one published session refuses to choose one", () => {
    const root = setup(
      [
        lane({
          agents: [
            { sessionId: "session-1", agentId: "codex", model: null, status: "running" },
            { sessionId: "session-2", agentId: "claude", model: null, status: "running" },
          ],
        }),
      ],
      { attach: vi.fn(), stop: vi.fn() },
    );
    const button = root.querySelector<HTMLButtonElement>("[data-d10-action='stop']")!;

    expect(button.disabled).toBe(true);
    expect(button.getAttribute("aria-label")).toContain("more than one Agent session");
  });

  test("Pause and Kill stay visible, disabled, and name the missing Core command", () => {
    const root = setup([lane()], { attach: vi.fn(), stop: vi.fn() });

    const pause = root.querySelector<HTMLButtonElement>("[data-d10-action='pause']")!;
    const kill = root.querySelector<HTMLButtonElement>("[data-d10-action='kill']")!;
    expect(pause.disabled).toBe(true);
    expect(kill.disabled).toBe(true);
    expect(pause.getAttribute("aria-label")).toContain("no pause command");
    expect(kill.getAttribute("aria-label")).toContain("no kill command");
  });

  test("without a host every action is disabled rather than enabled and inert", () => {
    const root = setup([lane()]);

    for (const kind of ["attach", "stop", "pause", "kill"]) {
      const button = root.querySelector<HTMLButtonElement>(`[data-d10-action='${kind}']`)!;
      expect(button, kind).not.toBeNull();
      expect(button.disabled, kind).toBe(true);
    }
    expect(
      root.querySelector<HTMLButtonElement>("[data-d10-action='attach']")!.getAttribute("aria-label"),
    ).toContain("No host is bound");
  });
});
