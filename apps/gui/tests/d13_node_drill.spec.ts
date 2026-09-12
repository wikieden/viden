// @vitest-environment jsdom

import { describe, expect, test, vi } from "vitest";

import {
  renderD13FleetWorkflow,
  type D13FleetWorkflowProjection,
  type D13Node,
} from "../src/screens/d13_fleet_workflow";

/**
 * D13's node drill (`G6`).
 *
 * The design's fleet board treats a DAG node as the way into the work it
 * describes. Core does not publish a "node -> Lane" edge; it publishes a Lane's
 * own `task_id`, and the GUI projection joins on it. That makes the binding a
 * Core fact with three honest shapes — one Lane, none, several — and each gets
 * its own visible answer. Absence is never an error, and it is never a drill
 * that silently does nothing.
 */

function node(overrides: Partial<D13Node> = {}): D13Node {
  return {
    taskId: "task-1",
    title: "resolve blocker",
    objective: "recover dependency",
    role: "coder",
    dependsOn: [],
    requiredEvidence: [],
    permissionPolicy: "ask_before_mutation",
    status: "running",
    progress: 40,
    blocked: false,
    blockers: [],
    laneIds: ["lane-core"],
    ...overrides,
  };
}

function projectionOf(nodes: D13Node[]): D13FleetWorkflowProjection {
  return {
    workflows: [
      {
        dagId: "dag-1",
        goal: "freeze frontend contract v1",
        status: "running",
        createdAt: 1700000000,
        updatedAt: 1700000030,
        nodes,
      },
    ],
    handoffs: [],
  };
}

function setup(nodes: D13Node[], onOpenLane?: (laneId: string) => void) {
  document.body.innerHTML = '<div id="host"></div>';
  const root = document.querySelector<HTMLElement>("#host")!;
  renderD13FleetWorkflow(root, projectionOf(nodes), "en", onOpenLane);
  return root;
}

describe("D13 node drill", () => {
  test("clicking a node whose task Core bound to one Lane opens that Lane", () => {
    const onOpenLane = vi.fn();
    const root = setup([node()], onOpenLane);
    const card = root.querySelector<HTMLElement>("[data-d13-node='task-1']")!;

    expect(card.dataset.d13Drill).toBe("lane-core");
    expect(card.getAttribute("role")).toBe("button");
    expect(card.tabIndex).toBe(0);
    card.click();

    expect(onOpenLane).toHaveBeenCalledWith("lane-core");
  });

  test("Enter on a focused node does the same thing as the click", () => {
    const onOpenLane = vi.fn();
    const root = setup([node()], onOpenLane);
    const card = root.querySelector<HTMLElement>("[data-d13-node='task-1']")!;

    card.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    expect(onOpenLane).toHaveBeenCalledWith("lane-core");
  });

  test("a node with no Lane binding says why and does not navigate", () => {
    const onOpenLane = vi.fn();
    const root = setup([node({ laneIds: [] })], onOpenLane);
    const card = root.querySelector<HTMLElement>("[data-d13-node='task-1']")!;

    expect(card.dataset.d13Drill).toBe("none");
    expect(card.getAttribute("role")).toBeNull();
    expect(card.querySelector("[data-d13-drill-note]")?.textContent).toContain(
      "Core bound no Lane",
    );
    card.click();
    card.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    expect(onOpenLane).not.toHaveBeenCalled();
  });

  test("more than one bound Lane refuses to choose one", () => {
    const onOpenLane = vi.fn();
    const root = setup([node({ laneIds: ["lane-core", "lane-review"] })], onOpenLane);
    const card = root.querySelector<HTMLElement>("[data-d13-node='task-1']")!;

    expect(card.dataset.d13Drill).toBe("ambiguous");
    expect(card.querySelector("[data-d13-drill-note]")?.textContent).toContain(
      "more than one Lane",
    );
    card.click();
    expect(onOpenLane).not.toHaveBeenCalled();
  });

  test("without a host the node states that rather than looking clickable", () => {
    const root = setup([node()]);
    const card = root.querySelector<HTMLElement>("[data-d13-node='task-1']")!;

    expect(card.dataset.d13Drill).toBe("unavailable");
    expect(card.getAttribute("role")).toBeNull();
    expect(card.querySelector("[data-d13-drill-note]")?.textContent).toContain("No host is bound");
  });
});
