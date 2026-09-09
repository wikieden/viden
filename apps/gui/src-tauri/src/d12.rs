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
    /// The lines the failed merge collided with
    /// (`runtime.conflict_content`, GUI-CORE-015).
    ///
    /// `None` means Core published nothing for this bounce — an operator
    /// `BounceMergeConflict` has a reason and no failed apply behind it, so it
    /// carries none by contract — and never that the conflict was empty.
    pub content: Option<D12ConflictContentProjection>,
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
    /// Lane apply conflicts Core recorded for the Lanes this gate involves:
    /// the gate's own Lane and every bounce's origin Lane.
    pub lane_conflicts: Vec<D12LaneConflictProjection>,
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
    /// Core advertised `runtime.conflict_content`. False means the client must
    /// keep rendering the reason text alone and must not claim there was
    /// nothing to show.
    pub conflict_content_available: bool,
    pub unavailable: Vec<D2UnavailableProjection>,
}

/* ------------------------------------------------------------------ */
/* Structured conflict content (`runtime.conflict_content`, GUI-CORE-015) */
/* ------------------------------------------------------------------ */

/// The `frontend-contract-v1` extension that carries conflict lines.
///
/// Without it Core publishes the bounce `reason` and the Lane conflict
/// `summary` and nothing else, which is a different fact from "this bounce had
/// no content": the screen names the capability instead of the per-bounce
/// sentence.
pub const CONFLICT_CONTENT_CAPABILITY: &str = "runtime.conflict_content";

/// One reviewed-evidence binding a conflict baseline names.
///
/// The chip routes the same way D12's revert rows already do: through the
/// audit object Core links, never through the bare id, because `AuditQuery`
/// filters by object.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D12ConflictEvidenceProjection {
    pub evidence_id: String,
    pub source_hash: String,
    /// Short display form of `source_hash`; the full hash stays available.
    pub short_hash: String,
    pub audit_scope: crate::D14AuditScopeProjection,
}

/// What the `ours` side was read against.
///
/// `kind` is Core's own serde tag (`revision` / `evidence` / `unknown`), kept
/// as a string because `ConflictBaseline` is `#[non_exhaustive]`: a later
/// baseline kind must reach the screen as its real name rather than collapse
/// into one of today's three.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D12ConflictBaselineProjection {
    pub kind: String,
    /// Present only for `revision`.
    pub sha: Option<String>,
    /// Short display form of `sha`.
    pub short_sha: Option<String>,
    /// Present only for `evidence`.
    pub bindings: Vec<D12ConflictEvidenceProjection>,
}

/// One hunk the strict apply refused: two sides plus the patch preimage.
///
/// Never a merge result. Core computes no merge base and resolves nothing, so
/// there is no merged text to project and no resolve action to offer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D12ConflictHunkProjection {
    pub ours_start: u32,
    pub ours: Vec<String>,
    pub theirs_start: u32,
    pub theirs: Vec<String>,
    /// `None` means the hunk had no preimage at all (a binary file); an empty
    /// vector means it expected an empty region, which is what a creation hunk
    /// expects. The two are different facts and stay encoded differently.
    pub base: Option<Vec<String>>,
    /// Core's own `ConflictHunkReason` tag. `#[non_exhaustive]`, so an unnamed
    /// reason reaches the screen raw instead of being folded into a known one.
    pub reason: String,
}

/// One file the apply refused.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D12ConflictFileProjection {
    pub path: String,
    pub hunks: Vec<D12ConflictHunkProjection>,
    /// The file conflicted but its lines were dropped by Core's byte bound.
    /// Rendered as "not shown", never as "no conflict here".
    pub omitted: bool,
}

/// What a failed apply collided with, as lines rather than prose.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D12ConflictContentProjection {
    pub baseline: D12ConflictBaselineProjection,
    pub files: Vec<D12ConflictFileProjection>,
    /// At least one file's hunks were dropped by Core's byte bound.
    pub truncated: bool,
}

/// One Lane apply conflict Core recorded for a Lane this gate involves.
///
/// `LaneConflictView` rides the Lane apply path rather than the merge path, so
/// it is a separate list: the gate's own bounces stay the recovery timeline,
/// and these are the collisions the Lane hit while applying.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D12LaneConflictProjection {
    pub lane_id: String,
    pub summary: String,
    pub paths: Vec<String>,
    pub timestamp: Option<u64>,
    pub content: Option<D12ConflictContentProjection>,
}
