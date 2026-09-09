// @vitest-environment jsdom

import { describe, expect, test, vi } from "vitest";

import {
  renderD2Decisions,
  type D2DecisionsProjection,
  type D2IntentResult,
} from "../src/screens/d2_decisions";

const PROJECTION: D2DecisionsProjection = {
  workMode: "build",
  permissionLevel: "standard",
  pendingTotal: 2,
  selectedId: "approval-1",
  groups: [
    {
      kind: "gate",
      unavailable: null,
      items: [
        {
          id: "approval-1",
          kind: "gate",
          title: "Approve fs_write",
          projectId: "project-viden",
          laneId: "lane-gate",
          sessionId: "session-lane-gate",
          taskId: "task-lane-gate",
          risk: "high",
          status: "pending",
          auditId: "audit-gate",
          updatedAt: null,
          expiresAt: 1_700_000_500,
        },
      ],
    },
    {
      kind: "review",
      unavailable: null,
      items: [
        {
          id: "review-1",
          kind: "review",
          title: "review-1",
          projectId: "project-viden",
          laneId: "lane-review",
          sessionId: null,
          taskId: "task-lane-review",
          risk: null,
          status: "pending",
          auditId: "audit-review",
          updatedAt: 1_700_000_200,
          expiresAt: null,
        },
      ],
    },
    {
      kind: "contract",
      unavailable: { key: "d2.contract.noPendingFact", code: "GUI-CORE-013" },
      items: [],
    },
  ],
  detail: {
    id: "approval-1",
    kind: "gate",
    title: "Approve fs_write",
    projectId: "project-viden",
    laneId: "lane-gate",
    taskId: "task-lane-gate",
    auditId: "audit-gate",
    // The runtime links no audit object for a tool approval, so this detail
    // offers no trail affordance.
    auditScope: null,
    policyReasonKey: "permission.requires_approval",
    blockedByPlan: false,
    context: {
      source: "approval_input_preview",
      text: "fs_write crates/types/src/runtime.rs",
      unavailable: { key: "d2.context.noStructuredDiff", code: "GUI-CORE-012" },
    },
    evidence: [
      {
        id: "evidence-1",
        kind: "check",
        summary: "viden-types check failed",
        path: "evidence/check.json",
        source: "lane-gate",
        timestamp: 1_700_000_150,
      },
    ],
    actions: [
      { kind: "once", available: true, sessionId: null, paths: [], code: null },
      { kind: "deny", available: true, sessionId: null, paths: [], code: null },
    ],
  },
};

function setup(projection: D2DecisionsProjection = PROJECTION) {
  document.body.innerHTML = '<div id="host"></div>';
  const root = document.querySelector<HTMLElement>("#host")!;
  const send = vi.fn(
    async (): Promise<D2IntentResult> => ({
      projection,
      pendingCommandId: "gui-d2-1",
      outcome: { state: "pending", reason: null },
    }),
  );
  const onViewAuditTrail = vi.fn();
  const controller = renderD2Decisions(root, projection, send, "en", onViewAuditTrail);
  return { root, send, controller, onViewAuditTrail };
}

describe("D2 decision center", () => {
  test("renders one queue over every Core decision family", () => {
    const { root } = setup();
    expect(root.querySelector("[data-d2-group='gate']")).not.toBeNull();
    expect(root.querySelector("[data-d2-group='review']")).not.toBeNull();
    expect(root.querySelector("[data-d2-group='contract']")).not.toBeNull();
    expect(root.querySelector("[data-d2-pending-total]")?.textContent).toContain("2");
  });

  test("declares Core contract gaps instead of hiding them", () => {
    const { root } = setup();
    const codes = [...root.querySelectorAll("[data-d2-unavailable]")].map(
      (node) => (node as HTMLElement).dataset.d2Unavailable,
    );
    expect(codes).toContain("GUI-CORE-013");
    expect(codes).toContain("GUI-CORE-012");
  });

  test("keeps a decided contract's verdicts visible but non-actionable", () => {
    // Every `ContractRecord` schema 1 publishes is already decided, and Core
    // refuses a second `ConfirmContract` for an id it has recorded. An enabled
    // Confirm or Reject here could only ever produce a refusal.
    const decided: D2DecisionsProjection = {
      ...PROJECTION,
      selectedId: "contract-1",
      detail: {
        id: "contract-1",
        kind: "contract",
        title: "feel-v1 contract",
        projectId: "project-viden",
        laneId: "lane-contract",
        taskId: "task-lane-contract",
        auditId: "audit-contract",
        auditScope: { kind: "contract", id: "contract-1" },
        policyReasonKey: null,
        blockedByPlan: false,
        context: {
          source: "contract_summary",
          text: "feel-v1 contract",
          unavailable: null,
        },
        evidence: [],
        actions: [
          {
            kind: "confirm_contract",
            available: false,
            sessionId: null,
            paths: [],
            code: "GUI-CORE-013",
          },
          {
            kind: "reject_contract",
            available: false,
            sessionId: null,
            paths: [],
            code: "GUI-CORE-013",
          },
        ],
      },
    };
    const { root, send } = setup(decided);

    const buttons = [...root.querySelectorAll<HTMLButtonElement>("[data-d2-action]")];
    expect(buttons.map((button) => button.dataset.d2Action)).toEqual([
      "confirm_contract",
      "reject_contract",
    ]);
    for (const button of buttons) {
      // Disabled and labelled, never hidden and never enabled-and-inert.
      expect(button.disabled).toBe(true);
      expect(button.dataset.d2ActionCode).toBe("GUI-CORE-013");
      expect(button.textContent).toContain("GUI-CORE-013");
      button.click();
    }
    expect(send).not.toHaveBeenCalled();

    // The reason is spelled out once below the bar, in words rather than as a
    // bare code.
    const notes = [...root.querySelectorAll<HTMLElement>("[data-d2-unavailable='GUI-CORE-013']")];
    expect(notes.some((note) => note.textContent?.includes("already recorded"))).toBe(true);
  });

  test("renders the Core input preview verbatim and never a synthesized diff", () => {
    const { root } = setup();
    const pane = root.querySelector<HTMLElement>("[data-d2-context]");
    expect(pane?.dataset.d2Context).toBe("approval_input_preview");
    expect(pane?.querySelector(".d2-context-body")?.textContent).toBe(
      "fs_write crates/types/src/runtime.rs",
    );
    expect(root.querySelector(".d2-diff-row")).toBeNull();
  });

  test("a gate action sends the approval intent for the selected request", () => {
    const { root, send } = setup();
    root.querySelector<HTMLButtonElement>("[data-d2-action='once']")!.click();
    expect(send).toHaveBeenCalledWith({
      type: "respond_approval",
      requestId: "approval-1",
      choice: "once",
      feedback: null,
    });
  });

  test("filtering narrows the queue without dropping the decision detail", () => {
    const { root } = setup();
    root.querySelector<HTMLButtonElement>("[data-d2-filter='review']")!.click();
    expect(root.querySelector("[data-d2-group='gate']")).toBeNull();
    expect(root.querySelector("[data-d2-group='review']")).not.toBeNull();
    expect(root.querySelector("[data-d2-detail='approval-1']")).not.toBeNull();
  });

  test("unavailable actions stay visible, disabled, and named by their code", () => {
    const { root } = setup(reviewProjection({ available: false, code: "D2-NO-REVIEWER-ACTOR" }));
    const button = root.querySelector<HTMLButtonElement>("[data-d2-action='accept_review']");
    expect(button?.disabled).toBe(true);
    expect(button?.textContent).toContain("D2-NO-REVIEWER-ACTOR");
    // The bare code is spelled out once, so a blocked action is never silent.
    expect(root.querySelector("[data-d2-unavailable='D2-NO-REVIEWER-ACTOR']")?.textContent).toContain(
      "independent reviewer identity",
    );
    // The reviewer note cannot be typed into a decision that cannot be sent.
    expect(root.querySelector<HTMLTextAreaElement>("[data-d2-review-feedback]")?.disabled).toBe(
      true,
    );
  });
});

/// A selected pending review whose two verdicts share one availability state.
function reviewProjection(
  action: { available: boolean; code: string | null },
): D2DecisionsProjection {
  return {
    ...PROJECTION,
    selectedId: "review-1",
    detail: {
      ...PROJECTION.detail!,
      id: "review-1",
      kind: "review",
      title: "review-1",
      laneId: "lane-review",
      taskId: "task-lane-review",
      auditId: "audit-review",
      auditScope: { kind: "merge_gate", id: "gate-integration" },
      policyReasonKey: null,
      context: { source: "review_request", text: "gate-integration", unavailable: null },
      actions: [
        { kind: "accept_review", sessionId: null, paths: [], ...action },
        { kind: "reject_review", sessionId: null, paths: [], ...action },
      ],
    },
  };
}

describe("D2 review decisions", () => {
  function reviewSetup(
    outcome: D2IntentResult["outcome"] = { state: "confirmed", reason: null },
    action: { available: boolean; code: string | null } = { available: true, code: null },
  ) {
    const projection = reviewProjection(action);
    document.body.innerHTML = '<div id="host"></div>';
    const root = document.querySelector<HTMLElement>("#host")!;
    const send = vi.fn(
      async (): Promise<D2IntentResult> => ({
        projection,
        pendingCommandId: outcome.state === "pending" ? "gui-d2-1" : null,
        outcome,
      }),
    );
    renderD2Decisions(root, projection, send, "en");
    return { root, send };
  }

  test("accepting a pending review sends DecideReview with no note", () => {
    const { root, send } = reviewSetup();
    root.querySelector<HTMLButtonElement>("[data-d2-action='accept_review']")!.click();
    expect(send).toHaveBeenCalledWith({
      type: "decide_review",
      reviewId: "review-1",
      accept: true,
      feedback: null,
    });
  });

  test("rejecting carries the reviewer note the operator typed", () => {
    const { root, send } = reviewSetup();
    const note = root.querySelector<HTMLTextAreaElement>("[data-d2-review-feedback]")!;
    note.value = "  jump arc still overshoots  ";
    note.dispatchEvent(new Event("input", { bubbles: true }));
    root.querySelector<HTMLButtonElement>("[data-d2-action='reject_review']")!.click();
    expect(send).toHaveBeenCalledWith({
      type: "decide_review",
      reviewId: "review-1",
      accept: false,
      feedback: "jump arc still overshoots",
    });
  });

  test("refuses an over-limit note locally instead of truncating it", () => {
    const { root, send } = reviewSetup();
    const note = root.querySelector<HTMLTextAreaElement>("[data-d2-review-feedback]")!;
    note.value = "x".repeat(501);
    note.dispatchEvent(new Event("input", { bubbles: true }));
    root.querySelector<HTMLButtonElement>("[data-d2-action='accept_review']")!.click();
    expect(send).not.toHaveBeenCalled();
    const outcome = root.querySelector<HTMLElement>("[data-d2-outcome]");
    expect(outcome?.dataset.d2Outcome).toBe("rejected");
    expect(outcome?.textContent).toContain("500");
  });

  test("renders the Core receipt for a confirmed verdict", async () => {
    const { root } = reviewSetup();
    root.querySelector<HTMLButtonElement>("[data-d2-action='accept_review']")!.click();
    await vi.waitFor(() =>
      expect(root.querySelector<HTMLElement>("[data-d2-outcome]")?.dataset.d2Outcome).toBe(
        "confirmed",
      ),
    );
  });

  test("shows a verdict as pending until Core records it", async () => {
    const { root } = reviewSetup({ state: "pending", reason: null });
    root.querySelector<HTMLButtonElement>("[data-d2-action='accept_review']")!.click();
    await vi.waitFor(() =>
      expect(root.querySelector<HTMLElement>("[data-d2-outcome]")?.dataset.d2Outcome).toBe(
        "pending",
      ),
    );
  });

  test("passes a Core refusal through verbatim", async () => {
    const reason = "review decision requires the independent reviewer lane";
    const { root } = reviewSetup({ state: "rejected", reason });
    root.querySelector<HTMLButtonElement>("[data-d2-action='accept_review']")!.click();
    await vi.waitFor(() =>
      expect(root.querySelector("[data-d2-outcome]")?.textContent).toContain(reason),
    );
  });
});

describe("D2 audit trail navigation", () => {
  test("a tool approval offers no trail, because Core links no object for it", () => {
    const { root } = setup();
    expect(root.querySelector("[data-d2-audit-trail]")).toBeNull();
  });

  test("a review decision navigates to its parent gate's audit trail", () => {
    const { root, onViewAuditTrail } = setup(
      reviewProjection({ available: true, code: null }),
    );
    const trail = root.querySelector<HTMLButtonElement>("[data-d2-audit-trail]");
    expect(trail?.dataset.d2AuditTrail).toBe("merge_gate:gate-integration");
    trail!.click();
    expect(onViewAuditTrail).toHaveBeenCalledWith({
      kind: "merge_gate",
      id: "gate-integration",
    });
  });
});
