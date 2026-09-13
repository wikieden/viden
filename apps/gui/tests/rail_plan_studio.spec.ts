// @vitest-environment jsdom

/**
 * The rail's Plan Studio slot (`G8`, `D-PLANBOARD`).
 *
 * The slot is `runtime.plan_documents`' only entry point on the rail, and it
 * follows the rail's own rules:
 *
 * - a destination the capability does not back is **disabled and labelled**,
 *   never hidden, and the capability is in the accessible name rather than only
 *   in a tooltip;
 * - the badge is `1` while a plan is `Proposed` for the selected owner — a
 *   decision awaiting the operator, which is what a rail badge means here and
 *   on D2 — and nothing otherwise;
 * - a count nobody took is **no badge**, not a zero.
 */

import { describe, expect, test, vi } from "vitest";

import { renderD1Cockpit, type D1Intent, type D1IntentResult } from "../src/screens/d1_cockpit";
import { IDLE_PLAN_STUDIO, type D15PlanStudioProjection } from "../src/models/plan";
import { D1_PROJECTION } from "./support/d1_projection";

function planProjection(
  status: string | null,
  laneId: string | null = null,
): D15PlanStudioProjection {
  return {
    ...IDLE_PLAN_STUDIO,
    capabilityAvailable: true,
    workMode: "plan",
    permissionMode: "plan",
    planModeActive: true,
    ownerLaneId: laneId,
    plan:
      status === null
        ? null
        : {
            planId: "plan_contract_v1",
            title: "Model the plan document and its decision",
            goal: "the operator can accept the plan and hand it to agents",
            status,
            revision: 2,
            contentSha256: "bcdf81ea",
            ownerLaneId: laneId,
            ownerSessionId: "session_plan_documents",
            sections: [],
            steps: [],
            stepsByRole: [],
            completedSteps: 0,
            proposedBy: { kind: "assistant", sessionId: null, turnId: "turn_plan_2" },
            createdAt: 1_700_007_000,
            updatedAt: 1_700_007_010,
            decidedAt: null,
            decisionAuditId: null,
            handoff: null,
          },
  };
}

function cockpit(plan?: {
  read: (laneId: string | null) => Promise<D15PlanStudioProjection>;
}) {
  document.body.innerHTML = '<div id="host"></div>';
  const root = document.querySelector<HTMLElement>("#host")!;
  const result: D1IntentResult = {
    projection: D1_PROJECTION,
    pendingCommandId: null,
    outcome: { state: "confirmed" as const, reason: null },
  };
  const controller = renderD1Cockpit(
    root,
    D1_PROJECTION,
    vi.fn(async (_intent: D1Intent) => result),
    vi.fn(async () => result),
    undefined,
    undefined,
    {
      poll: false,
      onNavigate: () => undefined,
      secondaryViews: { mount: () => undefined },
      plan,
    },
  );
  return { root, controller };
}

const slot = (root: HTMLElement) =>
  root.querySelector<HTMLButtonElement>('[data-rail-slot="d15"]')!;

describe("the Plan Studio rail slot", () => {
  test("sits between the conversation and the Lanes, as D-PLANBOARD fixed the order", () => {
    const { root } = cockpit();
    const ids = Array.from(
      root.querySelectorAll<HTMLElement>("[data-rail-slot]"),
      (node) => node.dataset.railSlot,
    );
    expect(ids.slice(0, 3)).toEqual(["conversation", "d15", "lanes"]);
  });

  test("with no plan port the slot is disabled and names the capability", () => {
    const { root } = cockpit();
    expect(slot(root).disabled).toBe(true);
    expect(slot(root).getAttribute("aria-label")).toBe(
      "Plan Studio — unavailable: Core publishes no runtime.plan_documents",
    );
    // Disabled, never hidden: the operator can see the destination exists and
    // read why it cannot be opened.
    expect(slot(root).hidden).toBe(false);
  });

  test("a Core that publishes no plan documents keeps the slot disabled and named", async () => {
    const read = vi.fn(async () => IDLE_PLAN_STUDIO);
    const { root } = cockpit({ read });
    await vi.waitFor(() => expect(read).toHaveBeenCalledTimes(1));
    await vi.waitFor(() =>
      expect(slot(root).getAttribute("aria-label")).toContain("runtime.plan_documents"),
    );
    expect(slot(root).disabled).toBe(true);
    expect(slot(root).querySelector("[data-rail-badge]")).toBeNull();
  });

  test("a proposed plan badges the slot with one decision awaiting the operator", async () => {
    const read = vi.fn(async () => planProjection("proposed"));
    const { root } = cockpit({ read });
    await vi.waitFor(() =>
      expect(slot(root).querySelector("[data-rail-badge]")?.textContent).toBe("1"),
    );
    expect(slot(root).disabled).toBe(false);
    expect(slot(root).dataset.railRoute).toBe("d15");
  });

  test("an accepted plan is not a decision awaiting anyone, so it carries no badge", async () => {
    const read = vi.fn(async () => planProjection("accepted"));
    const { root } = cockpit({ read });
    await vi.waitFor(() => expect(read).toHaveBeenCalledTimes(1));
    await vi.waitFor(() => expect(slot(root).disabled).toBe(false));
    expect(slot(root).querySelector("[data-rail-badge]")).toBeNull();
  });

  test("an owner with no plan carries no badge, and the slot still opens", async () => {
    const read = vi.fn(async () => planProjection(null));
    const { root } = cockpit({ read });
    await vi.waitFor(() => expect(slot(root).disabled).toBe(false));
    expect(slot(root).querySelector("[data-rail-badge]")).toBeNull();
  });

  test("a host that cannot answer leaves the slot disabled without blaming Core", async () => {
    const read = vi.fn(async () => {
      throw new Error("the host is not connected");
    });
    const { root } = cockpit({ read });
    await vi.waitFor(() => expect(read).toHaveBeenCalledTimes(1));
    // The capability stays unresolved: a host failure is not a Core that lacks
    // the capability, so nothing claims one way or the other beyond "disabled".
    expect(slot(root).disabled).toBe(true);
  });

  test("selecting another Lane re-reads the plan for that owner", async () => {
    const seen: Array<string | null> = [];
    const read = vi.fn(async (laneId: string | null) => {
      seen.push(laneId);
      return planProjection("proposed", laneId);
    });
    const { root, controller } = cockpit({ read });
    // The cockpit mounts on the projection's own selected Lane, so the first
    // read is already owner-scoped — never unscoped and then corrected.
    await vi.waitFor(() => expect(seen).toEqual([D1_PROJECTION.selectedLaneId]));
    const other = D1_PROJECTION.lanes.find(
      (lane) => lane.id !== D1_PROJECTION.selectedLaneId,
    )?.id;
    const lane = other ?? "lane-other";
    controller.selectLane(lane);
    // A plan belongs to an owner: the held one is dropped rather than shown
    // under the Lane that was just selected.
    await vi.waitFor(() => expect(seen).toEqual([D1_PROJECTION.selectedLaneId, lane]));
    expect(slot(root).querySelector("[data-rail-badge]")?.textContent).toBe("1");
  });

  test("the slot routes to the centre view rather than a window route", async () => {
    const mount = vi.fn();
    document.body.innerHTML = '<div id="host"></div>';
    const root = document.querySelector<HTMLElement>("#host")!;
    const result: D1IntentResult = {
      projection: D1_PROJECTION,
      pendingCommandId: null,
      outcome: { state: "confirmed" as const, reason: null },
    };
    renderD1Cockpit(
      root,
      D1_PROJECTION,
      vi.fn(async (_intent: D1Intent) => result),
      vi.fn(async () => result),
      undefined,
      undefined,
      {
        poll: false,
        secondaryViews: { mount },
        plan: { read: async () => planProjection("proposed") },
      },
    );
    await vi.waitFor(() => expect(slot(root).disabled).toBe(false));
    slot(root).click();
    await vi.waitFor(() => expect(mount).toHaveBeenCalledTimes(1));
    expect(mount.mock.calls[0][0]).toBe("d15");
    // The chrome is retained: this is a view over the transcript, not a route.
    expect(root.querySelector("[data-cockpit-role='activity']")).not.toBeNull();
    expect(root.querySelector("[data-composer]")).not.toBeNull();
    expect(root.querySelector("[data-secondary-view='d15']")).not.toBeNull();
  });

  test("⌘⇧P toggles the same view, and stands down without the capability", async () => {
    const mount = vi.fn();
    document.body.innerHTML = '<div id="host"></div>';
    const root = document.querySelector<HTMLElement>("#host")!;
    const result: D1IntentResult = {
      projection: D1_PROJECTION,
      pendingCommandId: null,
      outcome: { state: "confirmed" as const, reason: null },
    };
    const controller = renderD1Cockpit(
      root,
      D1_PROJECTION,
      vi.fn(async (_intent: D1Intent) => result),
      vi.fn(async () => result),
      undefined,
      undefined,
      {
        poll: false,
        secondaryViews: { mount },
        plan: { read: async () => planProjection("proposed") },
      },
    );
    await vi.waitFor(() => expect(slot(root).disabled).toBe(false));
    const chord = () =>
      window.dispatchEvent(
        new KeyboardEvent("keydown", { key: "P", metaKey: true, shiftKey: true, bubbles: true }),
      );
    chord();
    await vi.waitFor(() => expect(mount).toHaveBeenCalledTimes(1));
    // A second press returns to the transcript, the way every view chord does.
    chord();
    expect(root.querySelector("[data-secondary-view='d15']")).toBeNull();
    controller.dispose();
  });
});
