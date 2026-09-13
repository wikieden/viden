// @vitest-environment jsdom

import { beforeEach, describe, expect, test, vi } from "vitest";

import {
  D15_ALREADY_HANDED_OFF_CODE,
  D15_AMBIGUOUS_OWNER_CODE,
  D15_CAPABILITY_ABSENT_CODE,
  D15_NOT_ACCEPTED_CODE,
  D15_NO_PLAN_CODE,
  D15_PLAN_MODE_CODE,
  IDLE_PLAN_STUDIO,
  type D15Intent,
  type D15IntentResult,
  type D15Plan,
  type D15PlanStudioProjection,
} from "../src/models/plan";
import { renderD15PlanStudio } from "../src/screens/d15_plan_studio";

/**
 * The Plan View, measured against the canonical `plan-documents.json` values.
 *
 * Every literal below is the fixture's own — the plan id, the two revisions,
 * both digests, the two steps and their dependency edge, the DAG a handoff
 * produced, and Core's own `plan_revision_stale` sentence. Nothing here invents
 * a plan fact, because the whole point of the screen is that it renders none.
 */

const REV2_SHA = "bcdf81ea4a40a16a5c639734cd6e4fa064fef61d558b062d7abbd1b57b076a79";
const REV1_SHA = "062e22ca553ff5f82976fc2a85b7e065809fd1badacc4512594db1614b2ad316";
/** Core's own refusal, verbatim from the fixture's event 9. */
const STALE_REASON =
  "plan_revision_stale: plan `plan_contract_v1` is at revision 2 and the decision named revision 1\n" +
  "hint: re-read the plan from `plan_documents` and decide the revision it now carries";

function proposedPlan(): D15Plan {
  return {
    planId: "plan_contract_v1",
    title: "Model the plan document and its decision",
    goal: "the operator can accept the plan and hand it to agents",
    status: "proposed",
    revision: 2,
    contentSha256: REV2_SHA,
    ownerLaneId: null,
    ownerSessionId: "session_plan_documents",
    sections: [
      { kind: "requirements", body: "a plan is a Core fact with a decision, not transcript prose" },
      { kind: "acceptance_criteria", body: "the handoff names the DAG the steps became" },
    ],
    steps: [
      {
        id: "step_1",
        title: "model the record",
        status: "pending",
        role: "coder",
        fileScope: ["crates/types/src/plan.rs"],
        dependencies: [],
      },
      {
        id: "step_2",
        title: "review the contract",
        status: "pending",
        role: "reviewer",
        fileScope: ["crates/types/src/plan.rs"],
        dependencies: ["step_1"],
      },
    ],
    stepsByRole: [
      { role: "coder", steps: 1 },
      { role: "reviewer", steps: 1 },
    ],
    completedSteps: 0,
    proposedBy: { kind: "assistant", sessionId: "session_plan_documents", turnId: "turn_plan_2" },
    createdAt: 1_700_007_000,
    updatedAt: 1_700_007_010,
    decidedAt: null,
    decisionAuditId: null,
    handoff: null,
  };
}

function projection(
  overrides: Partial<D15PlanStudioProjection> = {},
  plan: D15Plan | null = proposedPlan(),
): D15PlanStudioProjection {
  const base: D15PlanStudioProjection = {
    capabilityAvailable: true,
    workMode: "plan",
    permissionMode: "plan",
    planModeActive: true,
    ownerLaneId: null,
    plan,
    ambiguousPlanIds: [],
    decision: {
      canDecide: plan?.status === "proposed",
      decideBlock: plan?.status === "proposed" ? null : D15_NO_PLAN_CODE,
      canHandOff: false,
      handOffBlock: plan ? D15_NOT_ACCEPTED_CODE : D15_NO_PLAN_CODE,
      rejectReasonMaxBytes: 2048,
    },
  };
  return { ...base, ...overrides };
}

function acceptedPlan(): D15Plan {
  return {
    ...proposedPlan(),
    status: "accepted",
    updatedAt: 1_700_007_020,
    decidedAt: 1_700_007_020,
    decisionAuditId: "audit_plan_accepted",
  };
}

function handedOffPlan(): D15Plan {
  return {
    ...acceptedPlan(),
    status: "handed_off",
    updatedAt: 1_700_007_030,
    handoff: {
      dagId: "dag_plan_contract_v1",
      handedOffAt: 1_700_007_030,
      auditId: "audit_plan_handed_off",
    },
  };
}

function idleResult(next: D15PlanStudioProjection): D15IntentResult {
  return { projection: next, pendingCommandId: null, outcome: { state: "confirmed", reason: null } };
}

let root: HTMLElement;

beforeEach(() => {
  document.body.innerHTML = "";
  root = document.createElement("div");
  document.body.append(root);
});

const action = (marker: string) =>
  root.querySelector<HTMLButtonElement>(`[data-d15-action="${marker}"]`)!;

describe("D15 Plan Studio — reading the plan Core published", () => {
  test("draws the sections in the projection's order and never reorders them locally", () => {
    renderD15PlanStudio(root, projection(), "en");
    const kinds = Array.from(
      root.querySelectorAll<HTMLElement>("[data-d15-section]"),
      (node) => node.dataset.d15Section,
    );
    expect(kinds).toEqual(["requirements", "acceptance_criteria"]);
  });

  test("renders each step with its status, role, file scope, and dependency edge", () => {
    renderD15PlanStudio(root, projection(), "en");
    const steps = root.querySelectorAll<HTMLElement>("[data-d15-step]");
    expect(steps).toHaveLength(2);
    expect(steps[0].dataset.d15StepStatus).toBe("pending");
    expect(steps[0].textContent).toContain("coder");
    expect(steps[0].textContent).toContain("crates/types/src/plan.rs");
    // The edge is Core's own `dependencies` list, printed as the design's
    // "after step_n" rather than inferred from the order of the rows.
    expect(steps[1].dataset.d15Dependencies).toBe("step_1");
    expect(steps[1].textContent).toContain("after step_1");
  });

  test("the record rail prints the pin Core holds: revision, digest, and author", () => {
    renderD15PlanStudio(root, projection(), "en");
    const rail = root.querySelector<HTMLElement>("[data-d15-rail]")!;
    expect(rail.querySelector('[data-d15-record="plan"]')!.textContent).toContain(
      "plan_contract_v1",
    );
    expect(rail.querySelector('[data-d15-record="revision"]')!.textContent).toContain("2");
    expect(rail.querySelector('[data-d15-record="contentSha"]')!.textContent).toContain("bcdf81ea");
    expect(rail.querySelector('[data-d15-record="proposedBy"]')!.textContent).toContain("assistant");
    // Nothing is decided or handed off yet, and both say so as absence.
    expect(rail.querySelector('[data-d15-record="decidedAt"]')!.textContent).toContain("—");
    expect(rail.querySelector('[data-d15-record="handoff"]')!.textContent).toContain("—");
  });

  test("states one supersede relation from the record's own revision, never a history", () => {
    renderD15PlanStudio(root, projection(), "en");
    const rail = root.querySelector<HTMLElement>("[data-d15-rail]")!;
    expect(rail.querySelector('[data-d15-record="revisionCurrent"]')).not.toBeNull();
    expect(rail.querySelector('[data-d15-record="revisionSuperseded"]')!.textContent).toContain(
      "rev 1",
    );
    // Exactly two revision rows: the record carries the latest, and the earlier
    // one is stated only as the relation the revision number itself implies.
    expect(rail.querySelectorAll('[data-d15-record^="revision"]')).toHaveLength(3);
    expect(rail.querySelector('[data-d15-note="revision-note"]')!.textContent).toContain(
      "latest revision only",
    );
  });

  test("a first revision states no supersede relation at all", () => {
    const first: D15Plan = { ...proposedPlan(), revision: 1, contentSha256: REV1_SHA };
    renderD15PlanStudio(root, projection({}, first), "en");
    const rail = root.querySelector<HTMLElement>("[data-d15-rail]")!;
    expect(rail.querySelector('[data-d15-record="revisionSuperseded"]')).toBeNull();
  });
});

describe("D15 Plan Studio — the three honest states", () => {
  test("no plan proposed is absence, with nothing to decide and nothing sent", () => {
    const send = vi.fn();
    renderD15PlanStudio(
      root,
      projection(
        {
          decision: {
            canDecide: false,
            decideBlock: D15_NO_PLAN_CODE,
            canHandOff: false,
            handOffBlock: D15_NO_PLAN_CODE,
            rejectReasonMaxBytes: 2048,
          },
        },
        null,
      ),
      "en",
      { send },
    );
    expect(root.querySelector<HTMLElement>("[data-d15-empty]")!.dataset.d15Empty).toBe("no-plan");
    expect(root.textContent).toContain("No plan proposed");
    // Plan mode is stated as what it blocks, which is why no plan exists yet.
    expect(root.textContent).toContain("Plan mode blocks file, shell, Git");
    expect(action("accept").disabled).toBe(true);
    expect(action("accept").getAttribute("aria-label")).toContain("nothing to decide");
    expect(send).not.toHaveBeenCalled();
  });

  test("a stale refusal shows Core's own words and offers the re-read it prescribes", async () => {
    const send = vi.fn(
      async (): Promise<D15IntentResult> => ({
        projection: projection(),
        pendingCommandId: null,
        outcome: { state: "rejected", reason: STALE_REASON },
      }),
    );
    const reload = vi.fn(async () => projection());
    renderD15PlanStudio(root, projection(), "en", { send, reload });
    action("accept").click();
    await vi.waitFor(() => expect(root.querySelector("[data-d15-refusal]")).not.toBeNull());

    const refusal = root.querySelector<HTMLElement>("[data-d15-refusal]")!;
    expect(refusal.dataset.d15Refusal).toBe("plan_revision_stale");
    expect(refusal.getAttribute("role")).toBe("alert");
    // Core's sentence, verbatim — not a client paraphrase of it.
    expect(refusal.textContent).toContain("is at revision 2 and the decision named revision 1");
    root.querySelector<HTMLButtonElement>("[data-d15-reload]")!.click();
    await vi.waitFor(() => expect(reload).toHaveBeenCalledTimes(1));
  });

  test("plan mode disables the handoff, names it, and states the refusal up front", () => {
    renderD15PlanStudio(
      root,
      projection(
        {
          decision: {
            canDecide: false,
            decideBlock: "D15-PLAN-DECIDED",
            canHandOff: false,
            handOffBlock: D15_PLAN_MODE_CODE,
            rejectReasonMaxBytes: 2048,
          },
        },
        acceptedPlan(),
      ),
      "en",
      { send: vi.fn() },
    );
    const handoff = action("handoff");
    expect(handoff.disabled).toBe(true);
    expect(handoff.dataset.d15Block).toBe(D15_PLAN_MODE_CODE);
    // Labelled, not hidden, and the reason is in the accessible name.
    expect(handoff.getAttribute("aria-label")).toContain("plan mode is active");
    expect(
      root.querySelector<HTMLElement>(`[data-d15-refusal="${D15_PLAN_MODE_CODE}"]`),
    ).not.toBeNull();
  });
});

describe("D15 Plan Studio — the decision row", () => {
  test("Accept sends the revision and digest this render drew", async () => {
    const sent: D15Intent[] = [];
    const send = vi.fn(async (intent: D15Intent) => {
      sent.push(intent);
      return idleResult(projection({}, acceptedPlan()));
    });
    renderD15PlanStudio(root, projection(), "en", { send });
    action("accept").click();
    await vi.waitFor(() => expect(send).toHaveBeenCalledTimes(1));
    expect(sent[0]).toEqual({
      type: "accept",
      planId: "plan_contract_v1",
      revision: 2,
      contentSha256: REV2_SHA,
    });
  });

  test("Hand off sends the same pin once Core reports build mode", async () => {
    const sent: D15Intent[] = [];
    const send = vi.fn(async (intent: D15Intent) => {
      sent.push(intent);
      return idleResult(projection({}, handedOffPlan()));
    });
    renderD15PlanStudio(
      root,
      projection(
        {
          workMode: "build",
          permissionMode: "default",
          planModeActive: false,
          decision: {
            canDecide: false,
            decideBlock: "D15-PLAN-DECIDED",
            canHandOff: true,
            handOffBlock: null,
            rejectReasonMaxBytes: 2048,
          },
        },
        acceptedPlan(),
      ),
      "en",
      { send },
    );
    expect(action("handoff").disabled).toBe(false);
    action("handoff").click();
    await vi.waitFor(() => expect(send).toHaveBeenCalledTimes(1));
    expect(sent[0]).toEqual({
      type: "hand_off",
      planId: "plan_contract_v1",
      revision: 2,
      contentSha256: REV2_SHA,
    });
  });

  test("Reject opens a reason the decision carries, and sends it with the pin", async () => {
    const sent: D15Intent[] = [];
    const send = vi.fn(async (intent: D15Intent) => {
      sent.push(intent);
      return idleResult(projection());
    });
    renderD15PlanStudio(root, projection(), "en", { send });
    action("reject").click();

    const field = root.querySelector<HTMLTextAreaElement>("[data-d15-reason-field]")!;
    // No `maxlength`: a bound that truncates would record a sentence the
    // operator did not write.
    expect(field.hasAttribute("maxlength")).toBe(false);
    const confirm = root.querySelector<HTMLButtonElement>("[data-d15-reject-confirm]")!;
    expect(confirm.disabled).toBe(true);
    field.value = "Risks misses the two exhaustive matches in the TUI client.";
    field.dispatchEvent(new Event("input"));
    expect(confirm.disabled).toBe(false);
    confirm.click();
    await vi.waitFor(() => expect(send).toHaveBeenCalledTimes(1));
    expect(sent[0]).toEqual({
      type: "reject",
      planId: "plan_contract_v1",
      revision: 2,
      contentSha256: REV2_SHA,
      reason: "Risks misses the two exhaustive matches in the TUI client.",
    });
  });

  test("a reason over Core's byte bound refuses the send and counts UTF-8 bytes", () => {
    renderD15PlanStudio(root, projection(), "en", { send: vi.fn() });
    action("reject").click();
    const field = root.querySelector<HTMLTextAreaElement>("[data-d15-reason-field]")!;
    const confirm = root.querySelector<HTMLButtonElement>("[data-d15-reject-confirm]")!;
    // 1000 CJK characters are 3000 UTF-8 bytes and 1000 UTF-16 units: counting
    // the wrong one would let an over-bound reason through.
    field.value = "计".repeat(1000);
    field.dispatchEvent(new Event("input"));
    const counter = root.querySelector<HTMLElement>("[data-d15-reason-count]")!;
    expect(counter.textContent).toContain("3000 / 2048");
    expect(counter.dataset.d15ReasonOver).toBe("true");
    expect(confirm.disabled).toBe(true);
  });

  test("a pending command is stated by its id rather than looking settled", async () => {
    let resolve: ((result: D15IntentResult) => void) | null = null;
    const send = vi.fn(
      () =>
        new Promise<D15IntentResult>((done) => {
          resolve = done;
        }),
    );
    renderD15PlanStudio(root, projection(), "en", { send });
    action("accept").click();
    await vi.waitFor(() =>
      expect(root.querySelector('[data-d15-note="outcome-pending"]')).not.toBeNull(),
    );
    // While one command is in flight every control is held: a second decision
    // could only race Core's own refusal.
    expect(action("accept").disabled).toBe(true);
    resolve!({
      projection: projection({}, acceptedPlan()),
      pendingCommandId: "gui-plan-1",
      outcome: { state: "pending", reason: null },
    });
    await vi.waitFor(() =>
      expect(root.querySelector('[data-d15-note="outcome-pending"]')!.textContent).toContain(
        "gui-plan-1",
      ),
    );
  });
});

describe("D15 Plan Studio — the handoff and its link", () => {
  test("a handed-off plan names the DAG, the audit row, and opens the fleet board", () => {
    const onOpenDag = vi.fn();
    renderD15PlanStudio(
      root,
      projection(
        {
          workMode: "build",
          permissionMode: "default",
          planModeActive: false,
          decision: {
            canDecide: false,
            decideBlock: "D15-PLAN-DECIDED",
            canHandOff: false,
            handOffBlock: D15_ALREADY_HANDED_OFF_CODE,
            rejectReasonMaxBytes: 2048,
          },
        },
        handedOffPlan(),
      ),
      "en",
      { send: vi.fn(), onOpenDag },
    );
    const line = root.querySelector<HTMLElement>("[data-d15-handoff]")!;
    expect(line.dataset.d15Handoff).toBe("dag_plan_contract_v1");
    expect(line.textContent).toContain("audit_plan_handed_off");
    root.querySelector<HTMLButtonElement>("[data-d15-open-dag]")!.click();
    expect(onOpenDag).toHaveBeenCalledWith("dag_plan_contract_v1");
    expect(action("handoff").dataset.d15Block).toBe(D15_ALREADY_HANDED_OFF_CODE);
  });

  test("with no host to navigate with, the link states that rather than looking live", () => {
    renderD15PlanStudio(
      root,
      projection({ planModeActive: false }, handedOffPlan()),
      "en",
      { send: vi.fn() },
    );
    expect(root.querySelector("[data-d15-open-dag]")).toBeNull();
    expect(root.querySelector('[data-d15-note="handoff-open"]')!.textContent).toContain(
      "No host is bound",
    );
  });
});

describe("D15 Plan Studio — absence and ambiguity", () => {
  test("an absent capability names it and claims no plan", () => {
    renderD15PlanStudio(root, IDLE_PLAN_STUDIO, "en", { send: vi.fn() });
    expect(root.querySelector<HTMLElement>("[data-d15-empty]")!.dataset.d15Empty).toBe(
      "capability",
    );
    expect(root.textContent).toContain("runtime.plan_documents");
    // No record rail either: an empty one beside "unavailable" would read as a
    // plan Core is withholding rather than one it never published.
    expect(root.querySelector("[data-d15-rail]")).toBeNull();
    expect(action("accept").disabled).toBe(true);
    expect(action("accept").dataset.d15Block).toBe(D15_CAPABILITY_ABSENT_CODE);
  });

  test("two open plans for one owner are named, and neither is chosen", () => {
    renderD15PlanStudio(
      root,
      projection(
        {
          ambiguousPlanIds: ["plan_contract_v1", "plan_second"],
          decision: {
            canDecide: false,
            decideBlock: D15_AMBIGUOUS_OWNER_CODE,
            canHandOff: false,
            handOffBlock: D15_AMBIGUOUS_OWNER_CODE,
            rejectReasonMaxBytes: 2048,
          },
        },
        null,
      ),
      "en",
      { send: vi.fn() },
    );
    expect(root.querySelector<HTMLElement>("[data-d15-empty]")!.dataset.d15Empty).toBe("ambiguous");
    expect(root.querySelector('[data-d15-note="ambiguous-ids"]')!.textContent).toContain(
      "plan_second",
    );
    expect(action("accept").disabled).toBe(true);
  });
});

describe("D15 Plan Studio — bilingual copy", () => {
  test("renders the same facts in Chinese without borrowing an English sentence", () => {
    renderD15PlanStudio(root, projection(), "zh-CN", { send: vi.fn() });
    expect(root.querySelector("[data-d15-title]")!.textContent).toContain("Model the plan document");
    expect(root.textContent).toContain("接受");
    expect(root.textContent).toContain("附理由驳回");
    // Core's own vocabulary stays unlocalized: the step id, the digest, and the
    // section kinds are the contract's words, not display text.
    expect(root.querySelector('[data-d15-step="step_1"]')).not.toBeNull();
  });
});
