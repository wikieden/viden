// @vitest-environment jsdom

import { describe, expect, test } from "vitest";

import {
  renderD13FleetWorkflow,
  type D13FleetWorkflowProjection,
} from "../src/screens/d13_fleet_workflow";

const PROJECTION: D13FleetWorkflowProjection = {
  workflows: [
    {
      dagId: "dag-1",
      goal: "ship the dash cancel",
      status: "active",
      createdAt: 1_700_000_000,
      updatedAt: 1_700_000_400,
      nodes: [
        {
          taskId: "task-1",
          title: "implement cancel window",
          objective: "widen the cancel window",
          role: "coder",
          dependsOn: [],
          requiredEvidence: ["replay-regression"],
          permissionPolicy: "propose_only",
          status: "running",
          progress: 40,
          blocked: false,
          blockers: [],
          laneIds: [],
        },
        {
          taskId: "task-2",
          title: "verify replay",
          objective: "re-run the replay regression",
          role: "tester",
          dependsOn: ["task-1"],
          requiredEvidence: [],
          permissionPolicy: "read_only",
          status: null,
          progress: null,
          blocked: true,
          laneIds: [],
          blockers: [
            {
              dependencyId: "dependency-1",
              dependsOnTaskId: "task-1",
              reason: "waits for the cancel window patch",
              auditId: "audit-1",
              updatedAt: 1_700_000_400,
            },
          ],
        },
      ],
    },
  ],
  handoffs: [],
};

function setup(projection: D13FleetWorkflowProjection = PROJECTION) {
  document.body.innerHTML = '<div id="host"></div>';
  const root = document.querySelector<HTMLElement>("#host")!;
  renderD13FleetWorkflow(root, projection, "en");
  return { root };
}

describe("D13 fleet workflow", () => {
  test("renders one column per Core workflow with its declared edges", () => {
    const { root } = setup();
    expect(root.querySelector("[data-d13-workflow='dag-1']")).not.toBeNull();
    expect(root.querySelectorAll("[data-d13-node]")).toHaveLength(2);
    expect(
      root.querySelector<HTMLElement>("[data-d13-node='task-2'] [data-d13-depends-on]")?.dataset
        .d13DependsOn,
    ).toBe("task-1");
  });

  test("states a planned node as not started rather than faking queued work", () => {
    const { root } = setup();
    const planned = root.querySelector<HTMLElement>(
      "[data-d13-node='task-2'] [data-d13-status]",
    );
    expect(planned?.dataset.d13Status).toBe("none");
    expect(planned?.textContent).toBe("not started");
    expect(
      root.querySelector<HTMLElement>("[data-d13-node='task-1'] [data-d13-status]")?.dataset
        .d13Status,
    ).toBe("running");
  });

  test("escalates only a node Core recorded a blocked dependency for", () => {
    const { root } = setup();
    const blocked = root.querySelectorAll("[data-d13-blocked='true']");
    expect(blocked).toHaveLength(1);
    expect((blocked[0] as HTMLElement).dataset.d13Node).toBe("task-2");
    expect(root.querySelector("[data-d13-blocker='dependency-1']")?.textContent).toContain(
      "waits for the cancel window patch",
    );
  });

  test("says there is no handoff instead of deriving one from the edges", () => {
    const { root } = setup();
    expect(root.querySelector("[data-d13-no-handoff]")).not.toBeNull();
    expect(root.querySelector("[data-d13-handoff]")).toBeNull();
  });
});
