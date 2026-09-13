/**
 * D15 Plan Studio projection types (`runtime.plan_documents`, C13).
 *
 * Mirrors `apps/gui/src-tauri/src/plan.rs` one-to-one. Nothing here derives a
 * plan fact: the client renders what Core published for one owner and echoes
 * the revision and digest back on a decision, which is what makes a stale
 * decision refusable rather than silently applied.
 */

/** Codes a blocked decision control carries, defined by the Rust projection. */
export const D15_PLAN_MODE_CODE = "plan_mode";
export const D15_REVISION_STALE_CODE = "plan_revision_stale";
export const D15_NO_PLAN_CODE = "D15-NO-PLAN";
export const D15_CAPABILITY_ABSENT_CODE = "D15-NO-CAPABILITY";
export const D15_ALREADY_DECIDED_CODE = "D15-PLAN-DECIDED";
export const D15_NOT_ACCEPTED_CODE = "D15-PLAN-NOT-ACCEPTED";
export const D15_ALREADY_HANDED_OFF_CODE = "D15-PLAN-HANDED-OFF";
export const D15_AMBIGUOUS_OWNER_CODE = "D15-PLAN-AMBIGUOUS";
export const D15_NO_STEPS_CODE = "D15-PLAN-NO-STEPS";

/** The capability the whole screen rides. */
export const PLAN_DOCUMENTS_CAPABILITY = "runtime.plan_documents";

export interface D15PlanSection {
  /** Core's own snake_case kind; the view labels it, never renames it. */
  kind: string;
  body: string;
}

export interface D15PlanStep {
  id: string;
  title: string;
  /** `pending` | `in_progress` | `completed`, in Core's words. */
  status: string;
  /** `null` when the author named none; Core defaults it at handoff. */
  role: string | null;
  fileScope: string[];
  dependencies: string[];
}

export interface D15PlanRoleCount {
  role: string;
  steps: number;
}

export interface D15PlanAuthor {
  /** `assistant` | `operator` | `unknown`. */
  kind: string;
  sessionId: string | null;
  turnId: string | null;
}

export interface D15PlanHandoff {
  dagId: string;
  handedOffAt: number;
  auditId: string;
}

export interface D15Plan {
  planId: string;
  title: string;
  goal: string;
  status: string;
  revision: number;
  contentSha256: string;
  ownerLaneId: string | null;
  ownerSessionId: string | null;
  sections: D15PlanSection[];
  steps: D15PlanStep[];
  stepsByRole: D15PlanRoleCount[];
  completedSteps: number;
  proposedBy: D15PlanAuthor;
  createdAt: number;
  updatedAt: number;
  decidedAt: number | null;
  decisionAuditId: string | null;
  handoff: D15PlanHandoff | null;
}

export interface D15Decision {
  canDecide: boolean;
  decideBlock: string | null;
  canHandOff: boolean;
  handOffBlock: string | null;
  rejectReasonMaxBytes: number;
}

export interface D15PlanStudioProjection {
  capabilityAvailable: boolean;
  workMode: string;
  permissionMode: string;
  planModeActive: boolean;
  ownerLaneId: string | null;
  plan: D15Plan | null;
  ambiguousPlanIds: string[];
  decision: D15Decision;
}

/** One decision the view sends. The pin travels with it, unchanged. */
export type D15Intent =
  | { type: "accept"; planId: string; revision: number; contentSha256: string }
  | {
      type: "reject";
      planId: string;
      revision: number;
      contentSha256: string;
      reason: string;
    }
  | { type: "hand_off"; planId: string; revision: number; contentSha256: string };

export interface D15IntentResult {
  projection: D15PlanStudioProjection;
  pendingCommandId: string | null;
  outcome: {
    state: "idle" | "pending" | "confirmed" | "rejected";
    reason: string | null;
  };
}

/**
 * The pre-Core frame: a capability nobody has answered for yet.
 *
 * `capabilityAvailable` is deliberately `false` rather than `true`: a client
 * that assumed the capability before the handshake would offer a decision row
 * over a Core that publishes no plans at all.
 */
export const IDLE_PLAN_STUDIO: D15PlanStudioProjection = {
  capabilityAvailable: false,
  workMode: "unknown",
  permissionMode: "unknown",
  planModeActive: false,
  ownerLaneId: null,
  plan: null,
  ambiguousPlanIds: [],
  decision: {
    canDecide: false,
    decideBlock: D15_CAPABILITY_ABSENT_CODE,
    canHandOff: false,
    handOffBlock: D15_CAPABILITY_ABSENT_CODE,
    rejectReasonMaxBytes: 2048,
  },
};

/**
 * Byte length of a rejection reason, measured the way Core measures it.
 *
 * `String.length` counts UTF-16 units, and Core bounds UTF-8 bytes; a CJK
 * reason is three bytes per character there and one unit here, so counting the
 * wrong one would let an over-bound reason through and report an under-bound
 * one as too long.
 */
export function rejectReasonBytes(reason: string): number {
  return new TextEncoder().encode(reason).length;
}
