// @vitest-environment jsdom

/**
 * The Lane hand-off from a centre view (`G6`).
 *
 * D10's `Attach` and D13's node drill are the same act: put the operator's
 * conversation on that Lane. They go through the cockpit's one selection path
 * and then through its own `conversation` route, so a hand-off from either
 * screen leaves the shell in exactly the state a Lane rail click would.
 *
 * These specs drive `hydrateShellFromCore` through an injected CoreClient, so
 * they prove the seam between the screens and the cockpit, not a host detail.
 */

import { beforeEach, describe, expect, test, vi } from "vitest";

import { hydrateShellFromCore } from "../src/main";
import type { D10LaneMonitorProjection } from "../src/screens/d10_lane_monitor";
import type { D13FleetWorkflowProjection } from "../src/screens/d13_fleet_workflow";
import { fakeCoreClient } from "./support/fake_core_client";
import { D1_PROJECTION } from "./support/d1_projection";

/** The cockpit with a second Lane, so a selection change is observable. */
const TWO_LANES = {
  ...D1_PROJECTION,
  lanes: [
    ...D1_PROJECTION.lanes,
    {
      id: "lane-review",
      role: "reviewer",
      status: "running",
      summary: "Retry policy",
      branch: "vd/retry-policy",
    },
  ],
};

const D10_PROJECTION: D10LaneMonitorProjection = {
  totalLanes: 1,
  totalProjects: 1,
  awaitingTotal: 0,
  lanes: [
    {
      id: "lane-review",
      projectId: "project-viden",
      summary: "Retry policy",
      role: "reviewer",
      route: "acp",
      gateStrength: "cooperative",
      mutationPolicy: "propose_only",
      status: "running",
      awaitsHuman: false,
      branch: "vd/retry-policy",
      worktree: ".worktrees/lane-review",
      progress: 40,
      agents: [
        { sessionId: "session-review", agentId: "codex", model: null, status: "running" },
      ],
      evidence: [],
      tokenLimit: null,
      costLimitMicroUsd: null,
      costMeterability: "metered",
      runStats: null,
    },
  ],
  unavailable: [],
};

const D13_PROJECTION: D13FleetWorkflowProjection = {
  workflows: [
    {
      dagId: "dag-1",
      goal: "freeze the contract",
      status: "active",
      createdAt: 1_700_000_000,
      updatedAt: 1_700_000_400,
      nodes: [
        {
          taskId: "task-review",
          title: "verify replay",
          objective: "re-run the replay regression",
          role: "tester",
          dependsOn: [],
          requiredEvidence: [],
          permissionPolicy: "read_only",
          status: "running",
          progress: 40,
          blocked: false,
          blockers: [],
          laneIds: ["lane-review"],
        },
      ],
    },
  ],
  handoffs: [],
};

function root(): HTMLElement {
  const element = document.querySelector<HTMLElement>("#app");
  if (!element) throw new Error("test root is missing");
  return element;
}

function frame(): HTMLElement {
  const element = root().querySelector<HTMLElement>('[data-screen="d1-cockpit"]');
  if (!element) throw new Error("the cockpit frame is missing");
  return element;
}

function selectedLaneIds(): string[] {
  return Array.from(
    root().querySelectorAll<HTMLElement>('[data-lane-id][aria-current="true"]'),
    (lane) => lane.dataset.laneId ?? "",
  );
}

describe("Lane hand-off from a centre view", () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="app"></main>';
    window.history.replaceState({}, "", "/");
  });

  test("D10 Attach selects the Lane and returns the centre pane to the conversation", async () => {
    window.history.replaceState({}, "", "/?screen=d10");
    const client = fakeCoreClient({
      d1Cockpit: async () => TWO_LANES,
      d10LaneMonitor: async () => D10_PROJECTION,
      d10Events: async () => {
        throw new Error("the ticker read is not part of this route");
      },
    });

    await hydrateShellFromCore(root(), client);
    expect(frame().dataset.centerView).toBe("d10");
    expect(selectedLaneIds()).toEqual(["lane-core"]);

    root()
      .querySelector<HTMLButtonElement>(
        "[data-d10-lane='lane-review'] [data-d10-action='attach']",
      )!
      .click();

    await vi.waitFor(() => expect(frame().dataset.centerView).toBe("transcript"));
    expect(selectedLaneIds()).toEqual(["lane-review"]);
  });

  test("D10 Stop sends CancelAgentSession for that Lane's session and reports Core's answer", async () => {
    window.history.replaceState({}, "", "/?screen=d10");
    const d1SendIntent = vi.fn(async () => ({
      projection: TWO_LANES,
      pendingCommandId: null,
      outcome: { state: "confirmed" as const, reason: null },
    }));
    const client = fakeCoreClient({
      d1Cockpit: async () => TWO_LANES,
      d10LaneMonitor: async () => D10_PROJECTION,
      d10Events: async () => {
        throw new Error("the ticker read is not part of this route");
      },
      d1SendIntent,
    });

    await hydrateShellFromCore(root(), client);
    root()
      .querySelector<HTMLButtonElement>("[data-d10-lane='lane-review'] [data-d10-action='stop']")!
      .click();

    await vi.waitFor(() => expect(d1SendIntent).toHaveBeenCalled());
    const [commandId, intent] = d1SendIntent.mock.calls[0] as unknown as [string, unknown];
    expect(commandId).toMatch(/^gui-d10-stop-/);
    expect(intent).toEqual({
      type: "cancel_agent_session",
      laneId: "lane-review",
      sessionId: "session-review",
    });
    // The centre pane stays on the monitor: Stop supervises a Lane, it does
    // not move the operator's conversation onto it.
    expect(frame().dataset.centerView).toBe("d10");
    await vi.waitFor(() =>
      expect(root().querySelector("[data-d10-stop-outcome]")?.textContent).toContain(
        "Core accepted",
      ),
    );
  });

  test("a D13 node bound to a Lane drills into the same hand-off", async () => {
    window.history.replaceState({}, "", "/?screen=d13");
    const client = fakeCoreClient({
      d1Cockpit: async () => TWO_LANES,
      d13FleetWorkflow: async () => D13_PROJECTION,
    });

    await hydrateShellFromCore(root(), client);
    expect(frame().dataset.centerView).toBe("d13");

    root().querySelector<HTMLElement>("[data-d13-node='task-review']")!.click();

    await vi.waitFor(() => expect(frame().dataset.centerView).toBe("transcript"));
    expect(selectedLaneIds()).toEqual(["lane-review"]);
  });
});
