// @vitest-environment jsdom

import { describe, expect, test, vi } from "vitest";

import {
  renderD10LaneMonitor,
  type D10Lane,
  type D10LaneActions,
  type D10LaneMonitorProjection,
} from "../src/screens/d10_lane_monitor";

/**
 * D10's card action row (`G6`, extended by `G9`).
 *
 * The design draws Attach / Pause / Kill on every Lane card. Attach is a Lane
 * selection and not a Core command at all; Stop is `CancelAgentSession`; and
 * since `runtime.agent_session_control` (C14) Pause and Kill are Core commands
 * too, routed through the same host handler the Agent Board uses. The tests
 * below pin what that keeps honest: each control does exactly what it says, a
 * control Core would refuse is disabled with Core's own reason rather than
 * offered, and an outcome is Core's answer rather than an assumed success.
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
      { sessionId: "session-1", agentId: "codex", model: "gpt-5-codex", status: "running", terminal: false, pausable: true },
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
    sessionControlAvailable: true,
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
            { sessionId: "session-1", agentId: "codex", model: null, status: "running", terminal: false, pausable: true },
            { sessionId: "session-2", agentId: "claude", model: null, status: "running", terminal: false, pausable: true },
          ],
        }),
      ],
      { attach: vi.fn(), stop: vi.fn() },
    );
    const button = root.querySelector<HTMLButtonElement>("[data-d10-action='stop']")!;

    expect(button.disabled).toBe(true);
    expect(button.getAttribute("aria-label")).toContain("more than one Agent session");
  });

  test("Pause and Kill send Core's own commands for the Lane's session", async () => {
    const pause = vi.fn(async () => ({ state: "confirmed", reason: null }));
    const kill = vi.fn(async () => ({ state: "confirmed", reason: null }));
    const root = setup([lane()], { attach: vi.fn(), stop: vi.fn(), pause, kill });

    const pauseButton = root.querySelector<HTMLButtonElement>("[data-d10-action='pause']")!;
    expect(pauseButton.disabled).toBe(false);
    expect(pauseButton.dataset.d10ControlSession).toBe("session-1");
    pauseButton.click();
    await flush();
    expect(pause).toHaveBeenCalledWith("lane-1", "session-1");

    root.querySelector<HTMLButtonElement>("[data-d10-action='kill']")!.click();
    await flush();
    expect(kill).toHaveBeenCalledWith("lane-1", "session-1");
  });

  test("names the capability when Core publishes no session control", () => {
    document.body.innerHTML = '<div id="host"></div>';
    const root = document.querySelector<HTMLElement>("#host")!;
    renderD10LaneMonitor(
      root,
      { ...projectionOf([lane()]), sessionControlAvailable: false },
      "en",
      undefined,
      null,
      { attach: vi.fn(), stop: vi.fn(), pause: vi.fn(), kill: vi.fn() },
    );

    for (const kind of ["pause", "kill"]) {
      const button = root.querySelector<HTMLButtonElement>(`[data-d10-action='${kind}']`)!;
      expect(button.disabled, kind).toBe(true);
      expect(button.getAttribute("aria-label"), kind).toContain(
        "runtime.agent_session_control",
      );
    }
    // Stop predates the capability and must not be retired by its absence.
    expect(
      root.querySelector<HTMLButtonElement>("[data-d10-action='stop']")!.disabled,
    ).toBe(false);
  });

  test("refuses a control the session's status does not allow, in Core's words", () => {
    const paused = lane({
      agents: [
        {
          sessionId: "session-1",
          agentId: "codex",
          model: "gpt-5-codex",
          status: "paused",
          terminal: false,
          pausable: false,
        },
      ],
    });
    const root = setup([paused], {
      attach: vi.fn(),
      stop: vi.fn(),
      pause: vi.fn(),
      kill: vi.fn(),
    });

    const pauseButton = root.querySelector<HTMLButtonElement>("[data-d10-action='pause']")!;
    expect(pauseButton.disabled).toBe(true);
    expect(pauseButton.getAttribute("aria-label")).toContain("the session is paused");
    // Kill accepts a paused session, so it stays live.
    expect(
      root.querySelector<HTMLButtonElement>("[data-d10-action='kill']")!.disabled,
    ).toBe(false);
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
