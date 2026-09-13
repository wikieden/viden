//! D15 Plan Studio projections (`runtime.plan_documents`, C13).
//!
//! A plan is a Core-owned durable fact. This module projects exactly what Core
//! published for one owner and nothing else: the client never keeps a plan Core
//! does not know, never reconstructs a revision history the view state does not
//! carry, and never decides on a revision the operator did not read.
//!
//! Three rules shape the shapes below, and each exists because dropping it
//! produces a specific lie:
//!
//! 1. **Absence, emptiness and ambiguity are three answers.** A Core with no
//!    `runtime.plan_documents` capability, an owner with no plan, and an owner
//!    Core published several plans for are three different sentences; a single
//!    `Option<plan>` would collapse the last two into "no plan proposed".
//! 2. **A decision is pinned to what was rendered.** [`D15PlanProjection`]
//!    carries `revision` and `content_sha256`, and the intent carries them
//!    back. Re-resolving them at send time — the pattern D12 uses for a merge
//!    gate — would silently decide a revision the model wrote while the
//!    operator was reading the previous one, which is the exact failure
//!    `plan_revision_stale` exists to catch.
//! 3. **A blocked control names its block.** Every disabled decision carries a
//!    code, so the view states why rather than rendering a control that looks
//!    pressable and is not.

use serde::{Deserialize, Serialize};

/// The Core capability this screen reads.
pub const PLAN_DOCUMENTS_CAPABILITY: &str = "runtime.plan_documents";

/// Core refuses `HandOffPlanDocument` in plan mode with this reason family.
pub const D15_PLAN_MODE_CODE: &str = "plan_mode";
/// Core refuses a pinned decision whose revision or digest moved.
pub const D15_REVISION_STALE_CODE: &str = "plan_revision_stale";
/// No plan exists for this owner, so nothing can be decided.
pub const D15_NO_PLAN_CODE: &str = "D15-NO-PLAN";
/// Core's handshake published no `runtime.plan_documents`.
pub const D15_CAPABILITY_ABSENT_CODE: &str = "D15-NO-CAPABILITY";
/// The plan carries a settled decision; Core refuses a second one.
pub const D15_ALREADY_DECIDED_CODE: &str = "D15-PLAN-DECIDED";
/// Only an `Accepted` plan can be handed off.
pub const D15_NOT_ACCEPTED_CODE: &str = "D15-PLAN-NOT-ACCEPTED";
/// The plan already became a DAG.
pub const D15_ALREADY_HANDED_OFF_CODE: &str = "D15-PLAN-HANDED-OFF";
/// Core published more than one plan for this owner; the client will not choose.
pub const D15_AMBIGUOUS_OWNER_CODE: &str = "D15-PLAN-AMBIGUOUS";
/// An accepted plan with no steps has no work to spawn.
pub const D15_NO_STEPS_CODE: &str = "D15-PLAN-NO-STEPS";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D15PlanSectionProjection {
    /// Core's own snake_case kind, unlocalized: it is the contract's stable
    /// vocabulary and the view labels it rather than renaming it.
    pub kind: String,
    pub body: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D15PlanStepProjection {
    pub id: String,
    pub title: String,
    /// `pending` / `in_progress` / `completed`, Core's own words.
    pub status: String,
    /// `None` when the author named none. Core defaults it to `coder` at
    /// handoff; the view says the role is unset rather than printing the
    /// default as though the author had written it.
    pub role: Option<String>,
    pub file_scope: Vec<String>,
    /// Sibling step ids this step runs after, in Core's own order.
    pub dependencies: Vec<String>,
}

/// One role and how many steps carry it, for the record rail's chips.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D15PlanRoleCountProjection {
    pub role: String,
    pub steps: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D15PlanAuthorProjection {
    /// `assistant` or `operator`; `unknown` for an author this build cannot
    /// name — `PlanAuthor` is `#[non_exhaustive]`, so a newer Core will publish
    /// one, and nearest-matching it to `operator` would credit a person for
    /// what a model wrote.
    pub kind: String,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D15PlanHandoffProjection {
    pub dag_id: String,
    pub handed_off_at: u64,
    pub audit_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D15PlanProjection {
    pub plan_id: String,
    pub title: String,
    pub goal: String,
    /// `drafting` / `proposed` / `accepted` / `rejected` / `handed_off` /
    /// `superseded`, or `unknown` for a status this build cannot name.
    pub status: String,
    pub revision: u32,
    pub content_sha256: String,
    /// The Lane this plan belongs to, or `None` for a session-scoped plan.
    pub owner_lane_id: Option<String>,
    pub owner_session_id: Option<String>,
    pub sections: Vec<D15PlanSectionProjection>,
    pub steps: Vec<D15PlanStepProjection>,
    pub steps_by_role: Vec<D15PlanRoleCountProjection>,
    pub completed_steps: usize,
    pub proposed_by: D15PlanAuthorProjection,
    pub created_at: u64,
    pub updated_at: u64,
    pub decided_at: Option<u64>,
    pub decision_audit_id: Option<String>,
    pub handoff: Option<D15PlanHandoffProjection>,
}

/// What the decision row may send, and why it may not.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D15DecisionProjection {
    pub can_decide: bool,
    /// The code the view names when `can_decide` is false.
    pub decide_block: Option<&'static str>,
    pub can_hand_off: bool,
    pub hand_off_block: Option<&'static str>,
    /// Core's own `MAX_PLAN_REJECT_REASON_BYTES`, carried on the projection
    /// rather than mirrored here (GUI-CORE-034, closed 2026-09-13).
    ///
    /// This module is a presentation shape and may not hold Core types, so the
    /// value is filled in by the projection from the Core facade's own
    /// constant, and the adapter checks against the same one when it refuses an
    /// over-bound reason. A literal here was exactly the drift the
    /// shared-constant rule exists to prevent: a Core that raised the bound
    /// would have left this client refusing reasons Core accepts.
    pub reject_reason_max_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D15PlanStudioProjection {
    /// Whether Core's handshake published `runtime.plan_documents`.
    pub capability_available: bool,
    /// Core's own work mode (`plan` / `build` / …), as published.
    pub work_mode: String,
    /// Core's own permission mode, as published.
    pub permission_mode: String,
    /// True when Core would refuse `HandOffPlanDocument` with `plan_mode`.
    ///
    /// Core checks its *permission* mode, and `set_work_mode` couples the two
    /// (`WorkMode::Plan | Review | Explore` forces `PermissionMode::Plan`), so
    /// either fact saying plan mode is enough to know the mutation is refused.
    /// Fail-closed on either: the cost of predicting a refusal that would not
    /// have happened is a labelled control, and the cost of the opposite is a
    /// button that looks live and is not.
    pub plan_mode_active: bool,
    /// The owner this view is scoped to: the selected Lane, or the session when
    /// no Lane is selected.
    pub owner_lane_id: Option<String>,
    /// The plan Core holds for that owner, or `None` when it holds none.
    ///
    /// Always `None` while `capability_available` is false: a Core that did
    /// not advertise the capability has not promised these records mean what
    /// this client would render them as, and showing one beside "plan
    /// documents are unavailable" is two claims about one Core that cannot
    /// both be true.
    pub plan: Option<D15PlanProjection>,
    /// Every plan id Core published for this owner when there is more than one
    /// open candidate. Non-empty means the client refused to choose.
    pub ambiguous_plan_ids: Vec<String>,
    pub decision: D15DecisionProjection,
}

/// One decision the Plan View sends.
///
/// `revision` and `content_sha256` travel from the render, not from a re-read:
/// they are the pin, and a host that resolved them fresh would turn every stale
/// decision into a silent accept of a revision nobody read.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum D15Intent {
    Accept {
        plan_id: String,
        revision: u32,
        content_sha256: String,
    },
    Reject {
        plan_id: String,
        revision: u32,
        content_sha256: String,
        reason: String,
    },
    HandOff {
        plan_id: String,
        revision: u32,
        content_sha256: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D15IntentResult {
    pub projection: D15PlanStudioProjection,
    pub pending_command_id: Option<String>,
    pub outcome: crate::D1OutcomeProjection,
}
