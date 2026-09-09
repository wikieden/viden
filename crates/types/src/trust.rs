use std::ops::Deref;

use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

use crate::{AgentLaneId, AgentTaskId, EvidenceId, MergeGateId, RuntimeOwner, now_timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffAcceptance {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffRecord {
    pub handoff_id: String,
    pub task_id: AgentTaskId,
    pub from_lane_id: AgentLaneId,
    pub to_lane_id: AgentLaneId,
    pub owner: RuntimeOwner,
    pub summary: String,
    pub acceptance: HandoffAcceptance,
    pub audit_id: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewRequestStatus {
    Pending,
    Accepted,
    Rejected,
}

/// The verdict an independent reviewer lane records on a pending review.
///
/// `ReviewRequestStatus` also carries `Pending`, which is a lifecycle state and
/// never a decision, so the command surface uses this narrower type: a review
/// decision can only settle a review, never reopen one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Accepted,
    Rejected,
}

impl ReviewVerdict {
    /// Stable audit/permission token. Kept separate from `Debug` so the audit
    /// timeline never depends on a derived formatting detail.
    pub fn as_token(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
        }
    }
}

impl From<ReviewVerdict> for ReviewRequestStatus {
    fn from(verdict: ReviewVerdict) -> Self {
        match verdict {
            ReviewVerdict::Accepted => Self::Accepted,
            ReviewVerdict::Rejected => Self::Rejected,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReviewedEvidenceBinding {
    pub evidence_id: EvidenceId,
    pub source_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewRequestRecord {
    pub review_id: String,
    pub gate_id: MergeGateId,
    pub task_id: AgentTaskId,
    pub requester_lane_id: AgentLaneId,
    pub reviewer_lane_id: AgentLaneId,
    pub owner: RuntimeOwner,
    pub evidence_ids: Vec<EvidenceId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_bindings: Vec<ReviewedEvidenceBinding>,
    pub status: ReviewRequestStatus,
    /// Reviewer prose attached to a recorded verdict. Additive and optional so
    /// schema-1 records written before review decisions still deserialize, and
    /// so a pending review never carries an empty note on the wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
    pub audit_id: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractDecision {
    Confirmed,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractRecord {
    pub contract_id: String,
    pub task_id: AgentTaskId,
    pub owner: RuntimeOwner,
    pub summary: String,
    pub decision: ContractDecision,
    pub audit_id: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyState {
    Blocked,
    Unblocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyRecord {
    pub dependency_id: String,
    pub task_id: AgentTaskId,
    pub depends_on_task_id: AgentTaskId,
    pub owner: RuntimeOwner,
    pub state: DependencyState,
    pub reason: String,
    pub audit_id: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeGateType {
    Patch,
    Review,
    Contract,
    Handoff,
    #[default]
    Artifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MergeGatePolicySnapshot {
    #[serde(default)]
    pub required_evidence: Vec<String>,
    #[serde(default)]
    pub permission_snapshot_id: Option<String>,
    #[serde(default)]
    pub requires_independent_validator: bool,
    #[serde(default)]
    pub captured_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeGateValidator {
    pub owner: RuntimeOwner,
    pub review_request_id: String,
    pub independent: bool,
    pub validated_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeGateDecisionOutcome {
    AwaitingEvidence,
    Accepted,
    Rejected,
    Conflict,
    Merged,
    Reverted,
    /// Read-only migration value for schema-1 records written before decisions
    /// became typed. New runtime commands never emit this outcome.
    Legacy,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MergeGateDecision {
    pub outcome: MergeGateDecisionOutcome,
    pub reason: String,
    #[serde(default)]
    pub owner: RuntimeOwner,
    #[serde(default)]
    pub evidence_ids: Vec<EvidenceId>,
    #[serde(default)]
    pub reviewed_evidence: Vec<ReviewedEvidenceBinding>,
    #[serde(default)]
    pub review_request_id: Option<String>,
    pub audit_id: String,
    pub decided_at: u64,
}

impl MergeGateDecision {
    /// Builds a typed decision that is being made right now.
    ///
    /// `decided_at` is stamped from the wall clock at call time, so callers must
    /// only use this when the decision is actually being taken; records replayed
    /// from persisted facts keep their original `decided_at` and are constructed
    /// through deserialization instead. `reviewed_evidence` and
    /// `review_request_id` start empty because review bindings are attached by a
    /// later review step, never by the deciding call site.
    pub fn decided_now(
        outcome: MergeGateDecisionOutcome,
        reason: String,
        owner: RuntimeOwner,
        evidence_ids: Vec<EvidenceId>,
        audit_id: String,
    ) -> Self {
        Self {
            outcome,
            reason,
            owner,
            evidence_ids,
            reviewed_evidence: Vec::new(),
            review_request_id: None,
            audit_id,
            decided_at: now_timestamp(),
        }
    }
}

impl Serialize for MergeGateDecision {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.outcome == MergeGateDecisionOutcome::Legacy {
            return serializer.serialize_str(&self.reason);
        }
        let mut state = serializer.serialize_struct("MergeGateDecision", 8)?;
        state.serialize_field("outcome", &self.outcome)?;
        state.serialize_field("reason", &self.reason)?;
        state.serialize_field("owner", &self.owner)?;
        state.serialize_field("evidence_ids", &self.evidence_ids)?;
        if !self.reviewed_evidence.is_empty() {
            state.serialize_field("reviewed_evidence", &self.reviewed_evidence)?;
        }
        if self.review_request_id.is_some() {
            state.serialize_field("review_request_id", &self.review_request_id)?;
        }
        state.serialize_field("audit_id", &self.audit_id)?;
        state.serialize_field("decided_at", &self.decided_at)?;
        state.end()
    }
}

impl Deref for MergeGateDecision {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.reason
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MergeGateDecisionWire {
    Typed(Box<MergeGateDecision>),
    Legacy(String),
}

pub(crate) fn deserialize_merge_gate_decision<'de, D>(
    deserializer: D,
) -> Result<Option<MergeGateDecision>, D::Error>
where
    D: Deserializer<'de>,
{
    let wire = Option::<MergeGateDecisionWire>::deserialize(deserializer)?;
    Ok(wire.map(|wire| match wire {
        MergeGateDecisionWire::Typed(decision) => *decision,
        MergeGateDecisionWire::Legacy(reason) => MergeGateDecision {
            outcome: MergeGateDecisionOutcome::Legacy,
            reason,
            owner: RuntimeOwner::default(),
            evidence_ids: Vec::new(),
            reviewed_evidence: Vec::new(),
            review_request_id: None,
            audit_id: "legacy".to_string(),
            decided_at: 0,
        },
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictBounceStatus {
    Pending,
    Revalidated,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictBounce {
    pub bounce_id: String,
    pub gate_id: MergeGateId,
    pub task_id: AgentTaskId,
    pub original_lane_id: AgentLaneId,
    pub owner: RuntimeOwner,
    pub reason: String,
    pub status: ConflictBounceStatus,
    #[serde(default)]
    pub evidence_ids: Vec<EvidenceId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub baseline_evidence: Vec<ReviewedEvidenceBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revalidation_evidence: Vec<ReviewedEvidenceBinding>,
    /// The lines the failed apply collided with
    /// (`runtime.conflict_content`, GUI-CORE-015).
    ///
    /// Additive since core-0.3.6, so a bounce published without it encodes to
    /// exactly the bytes it did before. `None` means no apply failure stands
    /// behind this bounce — an operator `BounceMergeConflict` carries a reason
    /// and nothing to show — never "the conflict had no content".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<crate::ConflictContent>,
    pub audit_id: String,
    pub created_at: u64,
    pub revalidated_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoverySnapshotReference {
    pub snapshot_id: String,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevertRecord {
    pub revert_id: String,
    pub gate_id: MergeGateId,
    pub applied_change_id: String,
    pub owner: RuntimeOwner,
    pub reason: String,
    #[serde(default)]
    pub restored_paths: Vec<String>,
    pub audit_id: String,
    pub reverted_at: u64,
}
