//! D12 integration gate projections.
//!
//! The integration gate is the failure path of the acceptance loop: two Lanes
//! touched the same place and the merge conflicts. The client never offers a
//! manual merge. It shows the Core gate, the bounce timeline back to the
//! origin Lane, and any post-merge revert, and it keeps `accept` closed until
//! every evidence id the gate policy requires is present.

use serde::{Deserialize, Serialize};

use crate::d1::D1OutcomeProjection;
use crate::d2::D2UnavailableProjection;

/// Why Core would refuse the action the button offers.
///
/// Each code names one rule `RuntimeContract::decide_merge_gate` enforces, so
/// a disabled control can say what is actually blocking it instead of going
/// dark. The codes are stable discriminants; the frontend owns the sentence.
pub mod d12_action_code {
    /// The gate already reached a terminal status.
    pub const GATE_CLOSED: &str = "gate_closed";
    /// The gate policy still lists evidence Core has not recorded.
    pub const MISSING_EVIDENCE: &str = "missing_evidence";
    /// A listed evidence id has no canonical reference, so Core can neither
    /// verify it nor build the reviewed-evidence binding acceptance requires.
    pub const EVIDENCE_NOT_CANONICAL: &str = "evidence_not_canonical";
    /// The policy demands an independent validator the gate does not have.
    pub const VALIDATOR_REQUIRED: &str = "validator_required";
    /// A conflict bounce is still pending origin-Lane revalidation.
    pub const CONFLICT_PENDING: &str = "conflict_pending";
    /// The validator's review request is missing or already decided.
    pub const REVIEW_NOT_PENDING: &str = "review_not_pending";
    /// Core published no owner this client may act as.
    pub const NO_ACTOR: &str = "no_actor";
}

/// One merge-gate decision the operator asked Core to make.
///
/// Both variants map to exactly one existing `RuntimeCommand`
/// (`AcceptMergeGate` / `RejectMergeGate`). D12 owns no private merge path and
/// never resolves a conflict itself: rejection bounces the work back to the
/// origin Lane with the reason the operator typed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum D12Intent {
    /// `RuntimeCommand::AcceptMergeGate`.
    ///
    /// `reviewed_evidence` is optional: when the client omits it the adapter
    /// derives the exact bindings Core will compare against from the current
    /// view, which is the only way the client can be sure they match.
    Accept {
        gate_id: String,
        #[serde(default)]
        reviewed_evidence: Option<Vec<D12ReviewedEvidenceInput>>,
        #[serde(default)]
        decision: Option<String>,
    },
    /// `RuntimeCommand::RejectMergeGate`. The reason is mandatory: Core stores
    /// it as the gate decision and the origin Lane's agent works from it.
    Bounce { gate_id: String, reason: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D12ReviewedEvidenceInput {
    pub evidence_id: String,
    pub source_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D12IntentResult {
    pub projection: D12IntegrationGateProjection,
    /// Set while Core has not yet published the receipt for this command.
    pub pending_command_id: Option<String>,
    /// `idle` / `pending` / `confirmed` / `rejected`, with Core's own reason.
    pub outcome: D1OutcomeProjection,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D12GateProjection {
    pub gate_id: String,
    pub task_id: String,
    pub status: String,
    pub gate_type: String,
    pub project_id: String,
    pub lane_id: Option<String>,
    pub requires_independent_validator: bool,
    pub has_validator: bool,
    pub required_evidence: Vec<String>,
    pub evidence_ids: Vec<String>,
    /// The gate awaits a decision its Agent session can no longer feed: the
    /// session is Completed, Failed, or Cancelled. Presentation only — the gate
    /// stays listed, selectable, and decidable; the flag exists so the screen
    /// can group it below live work instead of burying live work under it.
    pub dormant: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D12BounceProjection {
    pub bounce_id: String,
    pub original_lane_id: String,
    pub task_id: String,
    pub reason: String,
    pub status: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D12RevertProjection {
    pub revert_id: String,
    pub applied_change_id: String,
    pub reason: String,
    pub restored_paths: Vec<String>,
    pub audit_id: String,
    /// The audit object `change.reverted` links for this revert, so the row can
    /// open the revert's own trail. `AuditQuery` filters by object, never by
    /// audit id.
    pub audit_scope: Option<crate::D14AuditScopeProjection>,
    pub reverted_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D12CheckProjection {
    pub id: String,
    pub name: String,
    pub status: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D12ActionProjection {
    pub kind: String,
    pub available: bool,
    pub code: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D12GateDetailProjection {
    pub gate: D12GateProjection,
    /// Evidence ids the policy requires that Core has not recorded yet.
    pub missing_evidence: Vec<String>,
    pub bounces: Vec<D12BounceProjection>,
    pub reverts: Vec<D12RevertProjection>,
    pub checks: Vec<D12CheckProjection>,
    pub actions: Vec<D12ActionProjection>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D12IntegrationGateProjection {
    pub gates: Vec<D12GateProjection>,
    pub selected_gate_id: Option<String>,
    pub detail: Option<D12GateDetailProjection>,
    pub unavailable: Vec<D2UnavailableProjection>,
}
