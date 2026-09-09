use std::collections::BTreeSet;

use viden_core::{
    AgentLaneRecord, ApprovalRequestView, ContextBundleRecord, CostLedgerTotals, EvidenceView,
    ProviderHealthView, RuntimeErrorView, RuntimeViewState,
};
use viden_types::{
    AgentDagRecord, AgentRoute, AgentTaskRecord, ApprovalDefaultAction, ApprovalScope,
    CapabilityId, ConflictBounceStatus, ContractDecision, DependencyState, HandoffAcceptance,
    LaneConflictView, LaneOutputView, LaneRecoveryView, MergeGateDecisionOutcome, MergeGateStatus,
    ReviewRequestStatus, RuntimeCommand, RuntimeOwner, TokenCostView, ToolCallView,
};

use super::ui_state::TuiUiState;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct CockpitProjection {
    pub(super) lanes: Vec<AgentLaneRecord>,
    pub(super) tasks: Vec<AgentTaskRecord>,
    pub(super) dags: Vec<AgentDagRecord>,
    pub(super) active_tools: Vec<ToolCallView>,
    pub(super) queued_inputs: Vec<viden_types::QueuedInputView>,
    pub(super) lane_outputs: Vec<LaneOutputView>,
    pub(super) lane_conflicts: Vec<LaneConflictView>,
    pub(super) lane_recoveries: Vec<LaneRecoveryView>,
    pub(super) evidence: Vec<EvidenceView>,
    pub(super) evidence_decisions: Vec<EvidenceDecisionProjection>,
    pub(super) merge_gates: Vec<MergeGateProjection>,
    pub(super) review_requests: Vec<ReviewRequestProjection>,
    pub(super) handoffs: Vec<HandoffProjection>,
    pub(super) contracts: Vec<ContractProjection>,
    pub(super) dependencies: Vec<DependencyProjection>,
    pub(super) conflict_bounces: Vec<ConflictBounceProjection>,
    pub(super) reverts: Vec<RevertProjection>,
    pub(super) context: Option<ContextBundleRecord>,
    pub(super) context_pressure: ContextPressure,
    pub(super) cost: CostLedgerTotals,
    pub(super) token_cost: Option<TokenCostView>,
    pub(super) cost_visibility: CostVisibility,
    pub(super) provider: Option<ProviderHealthView>,
    pub(super) approvals: Vec<ApprovalRequestView>,
    pub(super) approval_actions: Vec<ApprovalProjection>,
    pub(super) errors: Vec<RuntimeErrorView>,
    pub(super) recovery_actions: Vec<RecoveryActionProjection>,
    pub(super) owner_actions: Vec<OwnerActionProjection>,
    cancel_owners: Vec<(String, CancelOwnerProjection)>,
    pub(super) pending_command: Option<PendingCommandProjection>,
    pub(super) audit_ids: Vec<String>,
}

impl CockpitProjection {
    #[cfg(test)]
    pub(super) fn from(runtime: &RuntimeViewState, _ui: &TuiUiState) -> Self {
        Self::from_with_capabilities(runtime, _ui, &BTreeSet::new())
    }

    pub(super) fn from_with_capabilities(
        runtime: &RuntimeViewState,
        _ui: &TuiUiState,
        capabilities: &BTreeSet<CapabilityId>,
    ) -> Self {
        let merge_gates = runtime
            .merge_gates
            .iter()
            .map(MergeGateProjection::from)
            .collect::<Vec<_>>();
        Self {
            lanes: runtime.lanes.clone(),
            tasks: runtime.tasks.clone(),
            dags: runtime.agent_dags.clone(),
            active_tools: runtime.active_tool_calls.clone(),
            queued_inputs: runtime.queued_inputs.clone(),
            lane_outputs: runtime.lane_outputs.clone(),
            lane_conflicts: runtime.lane_conflicts.clone(),
            lane_recoveries: runtime.lane_recoveries.clone(),
            evidence: runtime.latest_evidence.clone(),
            evidence_decisions: evidence_decisions(&merge_gates),
            merge_gates,
            review_requests: runtime
                .review_requests
                .iter()
                .map(ReviewRequestProjection::from)
                .collect(),
            handoffs: runtime
                .handoffs
                .iter()
                .map(HandoffProjection::from)
                .collect(),
            contracts: runtime
                .contracts
                .iter()
                .map(ContractProjection::from)
                .collect(),
            dependencies: runtime
                .dependencies
                .iter()
                .map(DependencyProjection::from)
                .collect(),
            conflict_bounces: runtime
                .conflict_bounces
                .iter()
                .map(ConflictBounceProjection::from)
                .collect(),
            reverts: runtime.reverts.iter().map(RevertProjection::from).collect(),
            context: runtime.context.clone(),
            context_pressure: context_pressure(runtime),
            cost: runtime.cost_ledger.clone(),
            token_cost: runtime.token_cost.clone(),
            cost_visibility: cost_visibility(runtime),
            provider: runtime.provider.clone(),
            approvals: runtime.pending_approvals.clone(),
            approval_actions: runtime
                .pending_approvals
                .iter()
                .map(ApprovalProjection::from)
                .collect(),
            errors: runtime.errors.clone(),
            recovery_actions: recovery_actions(runtime),
            owner_actions: owner_actions(runtime),
            cancel_owners: runtime
                .lanes
                .iter()
                .filter(|lane| lane.is_active())
                .map(|lane| {
                    (
                        lane.id.clone(),
                        cancel_owner_for_lane(runtime, capabilities, &lane.id),
                    )
                })
                .collect(),
            pending_command: runtime.last_command.as_ref().map(|receipt| {
                PendingCommandProjection {
                    command_id: receipt.command_id.clone(),
                    command: receipt.command.clone(),
                    state: CommandFactState::PendingCoreFact,
                }
            }),
            audit_ids: audit_ids(runtime),
        }
    }

    pub(super) fn supervision_counts(&self) -> SupervisionCounts {
        SupervisionCounts {
            merge_gates: self.merge_gates.len(),
            review_requests: self.review_requests.len(),
            handoffs: self.handoffs.len(),
            contracts: self.contracts.len(),
            dependencies: self.dependencies.len(),
            conflict_bounces: self.conflict_bounces.len(),
            reverts: self.reverts.len(),
        }
    }

    pub(super) fn cancel_owner_for_lane(&self, lane_id: &str) -> CancelOwnerProjection {
        self.cancel_owners
            .iter()
            .find(|(candidate, _)| candidate == lane_id)
            .map(|(_, projection)| projection.clone())
            .unwrap_or(CancelOwnerProjection::Unavailable(
                CancelUnavailableReason::LaneNotActive,
            ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CancelOwnerProjection {
    Available(RuntimeOwner),
    Unavailable(CancelUnavailableReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CancelUnavailableReason {
    MissingCapability,
    CoreOwnerRequired,
    OwnerLaneMismatch,
    AmbiguousOwner,
    LaneNotActive,
}

fn cancel_owner_for_lane(
    runtime: &RuntimeViewState,
    capabilities: &BTreeSet<CapabilityId>,
    lane_id: &str,
) -> CancelOwnerProjection {
    if !capabilities.contains(&CapabilityId("runtime.lane_owner_projection".to_string())) {
        return CancelOwnerProjection::Unavailable(CancelUnavailableReason::MissingCapability);
    }
    let bindings = runtime
        .lane_runtime_owners
        .iter()
        .filter(|binding| binding.lane_id == lane_id)
        .collect::<Vec<_>>();
    if bindings.is_empty() {
        CancelOwnerProjection::Unavailable(CancelUnavailableReason::CoreOwnerRequired)
    } else if bindings.len() > 1 {
        CancelOwnerProjection::Unavailable(CancelUnavailableReason::AmbiguousOwner)
    } else if bindings[0].owner.lane_id.as_deref() == Some(lane_id) {
        CancelOwnerProjection::Available(bindings[0].owner.clone())
    } else {
        CancelOwnerProjection::Unavailable(CancelUnavailableReason::OwnerLaneMismatch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum ContextPressure {
    #[default]
    Unavailable,
    Nominal,
    Elevated,
    PressureCritical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum CostVisibility {
    #[default]
    Unavailable,
    Metered,
    BlindUnmetered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ApprovalChoice {
    AllowOnce,
    AllowSession,
    AddRepoAllowlist,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ApprovalExpiry {
    Active,
    ExpiredAwaitingCore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ApprovalProjection {
    pub(super) request_id: String,
    pub(super) owner: RuntimeOwner,
    pub(super) choices: Vec<ApprovalChoice>,
    pub(super) expires_at: u64,
    pub(super) expiry: ApprovalExpiry,
    pub(super) default_action: ApprovalDefaultAction,
    pub(super) audit_id: String,
}

impl ApprovalProjection {
    fn from(approval: &ApprovalRequestView) -> Self {
        let mut choices = approval
            .allowed_scopes
            .iter()
            .map(|scope| match scope {
                ApprovalScope::Once => ApprovalChoice::AllowOnce,
                ApprovalScope::Session { .. } => ApprovalChoice::AllowSession,
                ApprovalScope::RepoAllowlist { .. } => ApprovalChoice::AddRepoAllowlist,
            })
            .collect::<Vec<_>>();
        choices.push(ApprovalChoice::Deny);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let expiry = if approval.expires_at > 0 && approval.expires_at <= now {
            ApprovalExpiry::ExpiredAwaitingCore
        } else {
            ApprovalExpiry::Active
        };
        Self {
            request_id: approval.id.clone(),
            owner: approval.owner.clone(),
            choices,
            expires_at: approval.expires_at,
            expiry,
            default_action: approval.default_action,
            audit_id: approval.audit_id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MergeGateProjection {
    pub(super) gate_id: String,
    pub(super) task_id: String,
    pub(super) status: MergeGateStatus,
    pub(super) decision: Option<MergeGateDecisionOutcome>,
    pub(super) evidence_ids: Vec<String>,
    pub(super) audit_ids: Vec<String>,
    pub(super) conflict_id: Option<String>,
}

impl From<&viden_types::MergeGateRecord> for MergeGateProjection {
    fn from(gate: &viden_types::MergeGateRecord) -> Self {
        let mut audit_ids = gate.audit_ids.clone();
        if let Some(decision) = gate.decision.as_ref() {
            audit_ids.push(decision.audit_id.clone());
        }
        Self {
            gate_id: gate.gate_id.clone(),
            task_id: gate.task_id.clone(),
            status: gate.status,
            decision: gate.decision.as_ref().map(|decision| decision.outcome),
            evidence_ids: gate.evidence_ids.clone(),
            audit_ids,
            conflict_id: gate
                .conflict
                .as_ref()
                .map(|conflict| conflict.bounce_id.clone()),
        }
    }
}

/// Compact row facts for one review request.
///
/// Deliberately not a clone of `ReviewRequestRecord`: a row renders identity,
/// the two lane parties, the settled status, and freshness. Evidence bindings,
/// owners, and reviewer prose stay in Core's record and are re-read from
/// `RuntimeViewState` when a detail surface needs them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReviewRequestProjection {
    pub(super) review_id: String,
    pub(super) gate_id: String,
    pub(super) requester_lane_id: String,
    pub(super) reviewer_lane_id: String,
    pub(super) status: ReviewRequestStatus,
    pub(super) updated_at: u64,
}

impl From<&viden_types::ReviewRequestRecord> for ReviewRequestProjection {
    fn from(review: &viden_types::ReviewRequestRecord) -> Self {
        Self {
            review_id: review.review_id.clone(),
            gate_id: review.gate_id.clone(),
            requester_lane_id: review.requester_lane_id.clone(),
            reviewer_lane_id: review.reviewer_lane_id.clone(),
            status: review.status,
            updated_at: review.updated_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HandoffProjection {
    pub(super) handoff_id: String,
    pub(super) task_id: String,
    pub(super) from_lane_id: String,
    pub(super) to_lane_id: String,
    pub(super) acceptance: HandoffAcceptance,
    pub(super) updated_at: u64,
}

impl From<&viden_types::HandoffRecord> for HandoffProjection {
    fn from(handoff: &viden_types::HandoffRecord) -> Self {
        Self {
            handoff_id: handoff.handoff_id.clone(),
            task_id: handoff.task_id.clone(),
            from_lane_id: handoff.from_lane_id.clone(),
            to_lane_id: handoff.to_lane_id.clone(),
            acceptance: handoff.acceptance,
            updated_at: handoff.updated_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ContractProjection {
    pub(super) contract_id: String,
    pub(super) task_id: String,
    pub(super) decision: ContractDecision,
    pub(super) updated_at: u64,
}

impl From<&viden_types::ContractRecord> for ContractProjection {
    fn from(contract: &viden_types::ContractRecord) -> Self {
        Self {
            contract_id: contract.contract_id.clone(),
            task_id: contract.task_id.clone(),
            decision: contract.decision,
            updated_at: contract.updated_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DependencyProjection {
    pub(super) dependency_id: String,
    pub(super) task_id: String,
    pub(super) depends_on_task_id: String,
    pub(super) state: DependencyState,
    pub(super) updated_at: u64,
}

impl From<&viden_types::DependencyRecord> for DependencyProjection {
    fn from(dependency: &viden_types::DependencyRecord) -> Self {
        Self {
            dependency_id: dependency.dependency_id.clone(),
            task_id: dependency.task_id.clone(),
            depends_on_task_id: dependency.depends_on_task_id.clone(),
            state: dependency.state,
            updated_at: dependency.updated_at,
        }
    }
}

/// Compact row facts for one bounced merge conflict. `revalidated_at` stays
/// optional because absence is a fact: a pending bounce has not been
/// revalidated, and the row must not render a zero timestamp for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ConflictBounceProjection {
    pub(super) bounce_id: String,
    pub(super) gate_id: String,
    pub(super) original_lane_id: String,
    pub(super) status: ConflictBounceStatus,
    pub(super) created_at: u64,
    pub(super) revalidated_at: Option<u64>,
}

impl From<&viden_types::ConflictBounce> for ConflictBounceProjection {
    fn from(conflict: &viden_types::ConflictBounce) -> Self {
        Self {
            bounce_id: conflict.bounce_id.clone(),
            gate_id: conflict.gate_id.clone(),
            original_lane_id: conflict.original_lane_id.clone(),
            status: conflict.status,
            created_at: conflict.created_at,
            revalidated_at: conflict.revalidated_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RevertProjection {
    pub(super) revert_id: String,
    pub(super) gate_id: String,
    pub(super) applied_change_id: String,
    pub(super) restored_path_count: usize,
    pub(super) reverted_at: u64,
}

impl From<&viden_types::RevertRecord> for RevertProjection {
    fn from(revert: &viden_types::RevertRecord) -> Self {
        Self {
            revert_id: revert.revert_id.clone(),
            gate_id: revert.gate_id.clone(),
            applied_change_id: revert.applied_change_id.clone(),
            restored_path_count: revert.restored_paths.len(),
            reverted_at: revert.reverted_at,
        }
    }
}

/// Compact supervision inbox counts.
///
/// The status bar and right rail historically counted merge gates only. These
/// counts extend that inbox to the rest of the Core-owned supervision surface
/// without any frontend aggregation rule beyond "how many records did Core
/// publish".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct SupervisionCounts {
    pub(super) merge_gates: usize,
    pub(super) review_requests: usize,
    pub(super) handoffs: usize,
    pub(super) contracts: usize,
    pub(super) dependencies: usize,
    pub(super) conflict_bounces: usize,
    pub(super) reverts: usize,
}

impl SupervisionCounts {
    /// Whether Core published any supervision record beyond merge gates. Rows
    /// that would otherwise render all-zero stay hidden.
    pub(super) fn has_non_gate_records(self) -> bool {
        self.review_requests
            + self.handoffs
            + self.contracts
            + self.dependencies
            + self.conflict_bounces
            + self.reverts
            > 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EvidenceDecision {
    EvidenceAccepted,
    EvidenceRejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EvidenceDecisionProjection {
    pub(super) evidence_id: String,
    pub(super) gate_id: String,
    pub(super) decision: EvidenceDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecoveryActionKind {
    LaneRecovery,
    RecoverableError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RecoveryActionProjection {
    pub(super) kind: RecoveryActionKind,
    pub(super) lane_id: Option<String>,
    pub(super) reason: String,
    pub(super) action: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OwnerActionKind {
    OwnerScopedCancelUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OwnerActionUnavailableReason {
    CoreOwnerRequired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OwnerActionProjection {
    pub(super) kind: OwnerActionKind,
    pub(super) target_lane_id: String,
    pub(super) reason: OwnerActionUnavailableReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommandFactState {
    PendingCoreFact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PendingCommandProjection {
    pub(super) command_id: String,
    pub(super) command: RuntimeCommand,
    pub(super) state: CommandFactState,
}

fn evidence_decisions(gates: &[MergeGateProjection]) -> Vec<EvidenceDecisionProjection> {
    gates
        .iter()
        .flat_map(|gate| {
            let decision = match gate.decision {
                Some(MergeGateDecisionOutcome::Accepted | MergeGateDecisionOutcome::Merged) => {
                    Some(EvidenceDecision::EvidenceAccepted)
                }
                Some(MergeGateDecisionOutcome::Rejected) => {
                    Some(EvidenceDecision::EvidenceRejected)
                }
                _ => None,
            };
            gate.evidence_ids.iter().filter_map(move |evidence_id| {
                decision.map(|decision| EvidenceDecisionProjection {
                    evidence_id: evidence_id.clone(),
                    gate_id: gate.gate_id.clone(),
                    decision,
                })
            })
        })
        .collect()
}

fn context_pressure(runtime: &RuntimeViewState) -> ContextPressure {
    let Some(context) = runtime.context.as_ref() else {
        return ContextPressure::Unavailable;
    };
    match context.pressure_percent() {
        90.. => ContextPressure::PressureCritical,
        75..=89 => ContextPressure::Elevated,
        _ => ContextPressure::Nominal,
    }
}

fn cost_visibility(runtime: &RuntimeViewState) -> CostVisibility {
    let containment_blind = runtime.lanes.iter().any(|lane| {
        matches!(lane.route, AgentRoute::Terminal | AgentRoute::Tmux)
            && lane.gate_strength == viden_types::GateStrength::Containment
    });
    let actual_blind = runtime
        .cost_usage
        .iter()
        .any(|usage| usage.actual_cost.is_none())
        || (runtime.cost_ledger.total_tokens > 0 && runtime.cost_ledger.actual_cost.is_none())
        || runtime
            .token_cost
            .as_ref()
            .is_some_and(|cost| cost.cost_micro_usd.is_none());
    if containment_blind || actual_blind {
        CostVisibility::BlindUnmetered
    } else if !runtime.cost_usage.is_empty()
        || runtime.cost_ledger.actual_cost.is_some()
        || runtime
            .token_cost
            .as_ref()
            .is_some_and(|cost| cost.cost_micro_usd.is_some())
    {
        CostVisibility::Metered
    } else {
        CostVisibility::Unavailable
    }
}

fn recovery_actions(runtime: &RuntimeViewState) -> Vec<RecoveryActionProjection> {
    runtime
        .lane_recoveries
        .iter()
        .map(|recovery| RecoveryActionProjection {
            kind: RecoveryActionKind::LaneRecovery,
            lane_id: Some(recovery.lane_id.clone()),
            reason: recovery.reason.clone(),
            action: recovery.next_action.clone(),
        })
        .chain(
            runtime
                .errors
                .iter()
                .filter(|error| error.recoverable)
                .map(|error| RecoveryActionProjection {
                    kind: RecoveryActionKind::RecoverableError,
                    lane_id: None,
                    reason: error.message.clone(),
                    action: error.hint.clone().unwrap_or_else(|| "retry".to_string()),
                }),
        )
        .collect()
}

fn owner_actions(runtime: &RuntimeViewState) -> Vec<OwnerActionProjection> {
    runtime
        .lanes
        .iter()
        .filter(|lane| lane.is_active())
        .map(|lane| OwnerActionProjection {
            kind: OwnerActionKind::OwnerScopedCancelUnavailable,
            // Lane identity is the only owner-related fact present on this
            // projection. Dispatch resolves the authoritative RuntimeOwner
            // through CoreClient instead of manufacturing one from paths.
            target_lane_id: lane.id.clone(),
            reason: OwnerActionUnavailableReason::CoreOwnerRequired,
        })
        .collect()
}

fn audit_ids(runtime: &RuntimeViewState) -> Vec<String> {
    let mut ids = runtime
        .pending_approvals
        .iter()
        .map(|approval| approval.audit_id.clone())
        .chain(
            runtime
                .handoffs
                .iter()
                .map(|record| record.audit_id.clone()),
        )
        .chain(
            runtime
                .review_requests
                .iter()
                .map(|record| record.audit_id.clone()),
        )
        .chain(
            runtime
                .contracts
                .iter()
                .map(|record| record.audit_id.clone()),
        )
        .chain(
            runtime
                .dependencies
                .iter()
                .map(|record| record.audit_id.clone()),
        )
        .chain(
            runtime
                .conflict_bounces
                .iter()
                .map(|record| record.audit_id.clone()),
        )
        .chain(runtime.reverts.iter().map(|record| record.audit_id.clone()))
        .collect::<Vec<_>>();
    for gate in &runtime.merge_gates {
        ids.extend(gate.audit_ids.iter().cloned());
        if let Some(decision) = gate.decision.as_ref() {
            ids.push(decision.audit_id.clone());
        }
    }
    ids.retain(|id| !id.is_empty());
    ids.sort();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, path::PathBuf};

    use viden_core::{
        AgentLaneRecord, AgentRole, AgentRoute, ApprovalDefaultAction, ApprovalRisk, ApprovalScope,
        ApprovalTarget, DataEgressPolicy, ExecutionTarget, GateStrength, LaneBudget, LaneStatus,
        MutationPolicy, PermissionLevel, PermissionMode, ProviderHealthView, RuntimeCommand,
        RuntimeErrorView, RuntimeEventEnvelope, RuntimeSnapshot, RuntimeViewState,
        RuntimeWireEvent, WorkMode,
    };
    use viden_types::{CapabilityId, LaneRuntimeOwnerBinding, RuntimeCommandReceipt};
    use viden_types::{
        MergeGateDecision, MergeGateDecisionOutcome, MergeGateStatus, ReviewedEvidenceBinding,
        RuntimeOwner,
    };

    use super::*;

    #[test]
    fn lane_runtime_owner_projection_requires_one_exact_live_binding() {
        let mut runtime = RuntimeViewState::new(runtime_snapshot());
        runtime.lanes = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        let owner = RuntimeOwner {
            workspace_id: "workspace".to_string(),
            project_id: "project".to_string(),
            lane_id: Some("L-start".to_string()),
            session_id: Some("session-start".to_string()),
            task_id: Some("task_start".to_string()),
            turn_id: Some("turn-start".to_string()),
        };
        runtime.lane_runtime_owners = vec![LaneRuntimeOwnerBinding {
            lane_id: "L-start".to_string(),
            owner: owner.clone(),
        }];
        let capabilities =
            BTreeSet::from([CapabilityId("runtime.lane_owner_projection".to_string())]);

        let projection = CockpitProjection::from_with_capabilities(
            &runtime,
            &TuiUiState::default(),
            &capabilities,
        );
        assert_eq!(
            projection.cancel_owner_for_lane("L-start"),
            CancelOwnerProjection::Available(owner.clone())
        );
        assert_eq!(
            projection.cancel_owner_for_lane("L-conflict"),
            CancelOwnerProjection::Unavailable(CancelUnavailableReason::CoreOwnerRequired)
        );

        let missing_capability = CockpitProjection::from_with_capabilities(
            &runtime,
            &TuiUiState::default(),
            &BTreeSet::new(),
        );
        assert_eq!(
            missing_capability.cancel_owner_for_lane("L-start"),
            CancelOwnerProjection::Unavailable(CancelUnavailableReason::MissingCapability)
        );

        let mut mismatched = runtime.clone();
        mismatched.lane_runtime_owners[0].owner.lane_id = Some("other-lane".to_string());
        let projection = CockpitProjection::from_with_capabilities(
            &mismatched,
            &TuiUiState::default(),
            &capabilities,
        );
        assert_eq!(
            projection.cancel_owner_for_lane("L-start"),
            CancelOwnerProjection::Unavailable(CancelUnavailableReason::OwnerLaneMismatch)
        );

        let mut ambiguous = runtime.clone();
        ambiguous.lane_runtime_owners.push(LaneRuntimeOwnerBinding {
            lane_id: "L-start".to_string(),
            owner,
        });
        let projection = CockpitProjection::from_with_capabilities(
            &ambiguous,
            &TuiUiState::default(),
            &capabilities,
        );
        assert_eq!(
            projection.cancel_owner_for_lane("L-start"),
            CancelOwnerProjection::Unavailable(CancelUnavailableReason::AmbiguousOwner)
        );
    }

    #[test]
    fn structured_runtime_fields_are_the_only_cockpit_fact_source() {
        let mut runtime = RuntimeViewState::new(runtime_snapshot());
        runtime.lanes.push(AgentLaneRecord {
            id: "lane-1".to_string(),
            task_id: Some("task-1".to_string()),
            role: AgentRole::Coder,
            route: AgentRoute::BuiltIn,
            gate_strength: GateStrength::Full,
            mutation_policy: MutationPolicy::ProposeOnly,
            worktree: None,
            branch: None,
            target: ExecutionTarget::Local,
            data_egress: DataEgressPolicy::Deny,
            status: LaneStatus::WaitingApproval,
            budget: LaneBudget::default(),
            active_session_ids: vec!["session-1".to_string()],
            summary: "structured lane".to_string(),
            evidence: vec!["evidence-1".to_string()],
            run_stats: None,
        });
        runtime.latest_evidence.push(EvidenceView {
            id: "evidence-1".to_string(),
            kind: "test".to_string(),
            summary: "structured evidence".to_string(),
            path: None,
            source: Some("core".to_string()),
            canonical: None,
            metadata: None,
            timestamp: Some(1),
            owner: None,
        });
        runtime.context = Some(ContextBundleRecord {
            bundle_id: "bundle-1".to_string(),
            task_id: "task-1".to_string(),
            policy: "structured-policy".to_string(),
            sources: Vec::new(),
            omitted_sources: Vec::new(),
            estimated_tokens: 700,
            largest_sources: Vec::new(),
            compaction_notes: Vec::new(),
            soft_token_budget: 800,
            hard_token_limit: 1_000,
        });
        runtime.cost_ledger.total_tokens = 321;
        runtime.provider = Some(ProviderHealthView {
            provider_id: "provider-1".to_string(),
            model: "model-1".to_string(),
            status: "healthy".to_string(),
            request_count: 3,
            error_count: 0,
            last_latency_ms: Some(12),
            average_latency_ms: Some(10),
            tokens_per_second: Some(50),
            credential: None,
        });
        runtime.pending_approvals.push(ApprovalRequestView {
            id: "approval-1".to_string(),
            tool_name: "shell".to_string(),
            title: "Run command".to_string(),
            message: "structured approval".to_string(),
            input_preview: "cargo test".to_string(),
            is_mutating: true,
            reason: Some("test".to_string()),
            owner: Default::default(),
            risk: ApprovalRisk::Medium,
            target: ApprovalTarget {
                kind: "command".to_string(),
                display: "cargo test".to_string(),
                canonical_ref: None,
            },
            allowed_scopes: Vec::new(),
            policy_reason_key: "policy.test".to_string(),
            policy_reason_args: Default::default(),
            expires_at: 0,
            default_action: ApprovalDefaultAction::Deny,
            audit_id: "audit-1".to_string(),
            decision_context: None,
        });
        runtime.errors.push(RuntimeErrorView {
            message: "structured error".to_string(),
            recoverable: true,
            hint: Some("retry".to_string()),
        });
        let ui = TuiUiState {
            entries: vec![super::super::ui_state::TuiEntry {
                label: "assistant".to_string(),
                body: "fake lane done; zero cost; provider failed".to_string(),
            }],
            ..TuiUiState::default()
        };

        let projection = CockpitProjection::from(&runtime, &ui);

        assert_eq!(projection.lanes[0].status, LaneStatus::WaitingApproval);
        assert_eq!(projection.evidence[0].summary, "structured evidence");
        assert_eq!(projection.context.as_ref().unwrap().estimated_tokens, 700);
        assert_eq!(projection.cost.total_tokens, 321);
        assert_eq!(projection.provider.as_ref().unwrap().status, "healthy");
        assert_eq!(projection.approvals[0].id, "approval-1");
        assert_eq!(projection.errors[0].message, "structured error");
    }

    #[test]
    fn changing_transcript_copy_does_not_change_cockpit_facts() {
        let mut runtime = RuntimeViewState::new(runtime_snapshot());
        runtime.provider = Some(ProviderHealthView {
            provider_id: "provider-1".to_string(),
            model: "model-1".to_string(),
            status: "healthy".to_string(),
            request_count: 1,
            error_count: 0,
            last_latency_ms: None,
            average_latency_ms: None,
            tokens_per_second: None,
            credential: None,
        });
        let first = CockpitProjection::from(
            &runtime,
            &TuiUiState {
                entries: vec![super::super::ui_state::TuiEntry {
                    label: "assistant".to_string(),
                    body: "provider failed".to_string(),
                }],
                ..TuiUiState::default()
            },
        );
        let second = CockpitProjection::from(
            &runtime,
            &TuiUiState {
                entries: vec![super::super::ui_state::TuiEntry {
                    label: "assistant".to_string(),
                    body: "provider healthy".to_string(),
                }],
                ..TuiUiState::default()
            },
        );

        assert_eq!(first, second);
        assert_eq!(first.provider.unwrap().status, "healthy");
    }

    #[test]
    fn supervision_records_project_compact_rows_and_inbox_counts() {
        let mut runtime = RuntimeViewState::new(runtime_snapshot());
        runtime
            .review_requests
            .push(viden_types::ReviewRequestRecord {
                review_id: "review-1".to_string(),
                gate_id: "gate-1".to_string(),
                task_id: "task-1".to_string(),
                requester_lane_id: "lane-a".to_string(),
                reviewer_lane_id: "lane-b".to_string(),
                owner: RuntimeOwner::default(),
                evidence_ids: vec!["ev-1".to_string()],
                evidence_bindings: Vec::new(),
                status: viden_types::ReviewRequestStatus::Rejected,
                feedback: Some("needs a regression test".to_string()),
                audit_id: "audit-review".to_string(),
                updated_at: 11,
            });
        runtime.handoffs.push(viden_types::HandoffRecord {
            handoff_id: "handoff-1".to_string(),
            task_id: "task-1".to_string(),
            from_lane_id: "lane-a".to_string(),
            to_lane_id: "lane-b".to_string(),
            owner: RuntimeOwner::default(),
            summary: "ready for review".to_string(),
            acceptance: viden_types::HandoffAcceptance::Accepted,
            audit_id: "audit-handoff".to_string(),
            updated_at: 12,
        });
        runtime.contracts.push(viden_types::ContractRecord {
            contract_id: "contract-1".to_string(),
            task_id: "task-1".to_string(),
            owner: RuntimeOwner::default(),
            summary: "frontend contract v1".to_string(),
            decision: viden_types::ContractDecision::Confirmed,
            audit_id: "audit-contract".to_string(),
            updated_at: 13,
        });
        runtime.dependencies.push(viden_types::DependencyRecord {
            dependency_id: "dep-1".to_string(),
            task_id: "task-1".to_string(),
            depends_on_task_id: "task-0".to_string(),
            owner: RuntimeOwner::default(),
            state: viden_types::DependencyState::Blocked,
            reason: "upstream pending".to_string(),
            audit_id: "audit-dep".to_string(),
            updated_at: 14,
        });
        runtime.conflict_bounces.push(viden_types::ConflictBounce {
            bounce_id: "bounce-1".to_string(),
            gate_id: "gate-1".to_string(),
            task_id: "task-1".to_string(),
            original_lane_id: "lane-a".to_string(),
            owner: RuntimeOwner::default(),
            reason: "base moved".to_string(),
            status: viden_types::ConflictBounceStatus::Pending,
            evidence_ids: Vec::new(),
            baseline_evidence: Vec::new(),
            revalidation_evidence: Vec::new(),
            content: None,
            audit_id: "audit-bounce".to_string(),
            created_at: 15,
            revalidated_at: None,
        });
        runtime.reverts.push(viden_types::RevertRecord {
            revert_id: "revert-1".to_string(),
            gate_id: "gate-1".to_string(),
            applied_change_id: "change-1".to_string(),
            owner: RuntimeOwner::default(),
            reason: "regression in main".to_string(),
            restored_paths: vec!["crates/core/src/lib.rs".to_string()],
            audit_id: "audit-revert".to_string(),
            reverted_at: 16,
        });

        let projection = CockpitProjection::from(&runtime, &TuiUiState::default());

        assert_eq!(
            projection.review_requests,
            vec![ReviewRequestProjection {
                review_id: "review-1".to_string(),
                gate_id: "gate-1".to_string(),
                requester_lane_id: "lane-a".to_string(),
                reviewer_lane_id: "lane-b".to_string(),
                status: viden_types::ReviewRequestStatus::Rejected,
                updated_at: 11,
            }]
        );
        assert_eq!(projection.handoffs[0].to_lane_id, "lane-b");
        assert_eq!(
            projection.handoffs[0].acceptance,
            viden_types::HandoffAcceptance::Accepted
        );
        assert_eq!(
            projection.contracts[0].decision,
            viden_types::ContractDecision::Confirmed
        );
        assert_eq!(
            projection.dependencies[0].depends_on_task_id,
            "task-0".to_string()
        );
        assert_eq!(projection.conflict_bounces[0].revalidated_at, None);
        assert_eq!(projection.reverts[0].restored_path_count, 1);

        let counts = projection.supervision_counts();
        assert_eq!(
            counts,
            SupervisionCounts {
                merge_gates: 0,
                review_requests: 1,
                handoffs: 1,
                contracts: 1,
                dependencies: 1,
                conflict_bounces: 1,
                reverts: 1,
            }
        );
        assert!(counts.has_non_gate_records());
        assert!(
            !CockpitProjection::from(
                &RuntimeViewState::new(runtime_snapshot()),
                &TuiUiState::default()
            )
            .supervision_counts()
            .has_non_gate_records()
        );
        // audit_ids() already spanned these records and must keep doing so.
        for audit_id in [
            "audit-review",
            "audit-handoff",
            "audit-contract",
            "audit-dep",
            "audit-bounce",
            "audit-revert",
        ] {
            assert!(
                projection.audit_ids.contains(&audit_id.to_string()),
                "audit id {audit_id} left the projection"
            );
        }
    }

    #[test]
    fn local_supervision_fixture_matrix() {
        let mut stream = runtime_from_fixture(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/stream-tool.json"
        ));
        let mut approval = runtime_from_fixture_prefix(
            include_str!(
                "../../../../crates/types/tests/fixtures/frontend-contract-v1/approval-allow-deny.json"
            ),
            1,
        );
        approval.pending_approvals[0].allowed_scopes = vec![
            ApprovalScope::Once,
            ApprovalScope::Session {
                session_id: "session-approval".to_string(),
            },
            ApprovalScope::RepoAllowlist {
                paths: vec!["crates/core".to_string()],
            },
        ];
        approval.pending_approvals[0].expires_at = 1;

        let dag = runtime_from_fixture(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/dag-blocker.json"
        ));
        let lanes = runtime_from_fixture(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/multi-lane.json"
        ));
        let mut merge = runtime_from_fixture(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/merge-gate.json"
        ));
        let template = merge.merge_gates[0].clone();
        let gate = |suffix: &str,
                    status: MergeGateStatus,
                    outcome: MergeGateDecisionOutcome,
                    reason: &str| {
            let mut gate = template.clone();
            gate.gate_id = format!("gate_{suffix}");
            gate.status = status;
            gate.decision = Some(MergeGateDecision {
                outcome,
                reason: reason.to_string(),
                owner: RuntimeOwner::default(),
                evidence_ids: gate.evidence_ids.clone(),
                reviewed_evidence: gate
                    .evidence_ids
                    .iter()
                    .map(|evidence_id| ReviewedEvidenceBinding {
                        evidence_id: evidence_id.clone(),
                        source_hash: format!("hash-{evidence_id}"),
                    })
                    .collect(),
                review_request_id: Some(format!("review_{suffix}")),
                audit_id: format!("audit_{suffix}"),
                decided_at: 2,
            });
            gate
        };
        merge.merge_gates = vec![
            template.clone(),
            gate(
                "collecting",
                MergeGateStatus::CollectingEvidence,
                MergeGateDecisionOutcome::AwaitingEvidence,
                "awaiting evidence",
            ),
            gate(
                "accepted",
                MergeGateStatus::Accepted,
                MergeGateDecisionOutcome::Accepted,
                "evidence accepted",
            ),
            gate(
                "needs_changes",
                MergeGateStatus::NeedsChanges,
                MergeGateDecisionOutcome::Rejected,
                "evidence rejected",
            ),
            gate(
                "conflict",
                MergeGateStatus::Blocked,
                MergeGateDecisionOutcome::Conflict,
                "merge conflict",
            ),
        ];
        let mut context = runtime_from_fixture(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/context-pressure-cost-blind.json"
        ));
        context.lanes.push(lanes.lanes[0].clone());
        let mut recovery = runtime_from_fixture(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/d1-vertical-slice.json"
        ));
        recovery.errors.extend([
            RuntimeErrorView {
                message: "provider unavailable".to_string(),
                recoverable: true,
                hint: Some("retry provider".to_string()),
            },
            RuntimeErrorView {
                message: "reconnect required after sequence gap".to_string(),
                recoverable: true,
                hint: Some("replay from cursor 41".to_string()),
            },
        ]);
        recovery.last_command = Some(RuntimeCommandReceipt {
            command_id: "command-pending-gate".to_string(),
            command: RuntimeCommand::MergeAgentPatch {
                gate_id: "gate_d1".to_string(),
                actor: Default::default(),
                decision: Some("apply reviewed patch".to_string()),
            },
        });

        // Keep the fixture matrix black-boxed at this boundary: every needle
        // is a typed Core fact or a fail-closed presentation classification.
        // Transcript copy, terminal output, and process exit strings are never
        // input to this projection.
        let cases = [
            (
                "streaming + tool",
                format!(
                    "{:?}",
                    CockpitProjection::from(&stream, &TuiUiState::default())
                ),
                // The streamed reply itself is deliberately absent: the
                // projection no longer carries a second copy of Core's
                // `assistant_stream`, which the transcript renders straight
                // from `RuntimeViewState`. What must survive here is the typed
                // evidence the same turn produced.
                vec!["ev_tool_ok", "tool_log", "rg completed"],
            ),
            (
                "four-choice approval + auto deny",
                format!(
                    "{:?}",
                    CockpitProjection::from(&approval, &TuiUiState::default())
                ),
                vec![
                    "approval_allow",
                    "Once",
                    "Session",
                    "RepoAllowlist",
                    "Deny",
                    "ExpiredAwaitingCore",
                ],
            ),
            (
                "DAG blocker + retry",
                format!(
                    "{:?}",
                    CockpitProjection::from(&dag, &TuiUiState::default())
                ),
                vec![
                    "dag_blocker",
                    "task_dependency",
                    "task_blocked",
                    "Retry blocker",
                ],
            ),
            (
                "lane switch + owner cancel",
                format!(
                    "{:?}",
                    CockpitProjection::from(&lanes, &TuiUiState::default())
                ),
                vec![
                    "lane_core",
                    "lane_review",
                    "session_lane_core",
                    "OwnerScopedCancelUnavailable",
                    "CoreOwnerRequired",
                ],
            ),
            (
                "evidence + merge gate states",
                format!(
                    "{:?}",
                    CockpitProjection::from(&merge, &TuiUiState::default())
                ),
                vec![
                    "ev_patch",
                    "gate_merge",
                    "CollectingEvidence",
                    "Accepted",
                    "NeedsChanges",
                    "Conflict",
                    "EvidenceAccepted",
                    "EvidenceRejected",
                ],
            ),
            (
                "context pressure + blind cost",
                format!(
                    "{:?}",
                    CockpitProjection::from(&context, &TuiUiState::default())
                ),
                vec!["ctx_bundle_pressure", "PressureCritical", "BlindUnmetered"],
            ),
            (
                "provider recovery + reconnect replay + pending command",
                format!(
                    "{:?}",
                    CockpitProjection::from(&recovery, &TuiUiState::default())
                ),
                vec![
                    "provider unavailable",
                    "retry provider",
                    "reconnect required after sequence gap",
                    "replay from cursor 41",
                    "command-pending-gate",
                    "PendingCoreFact",
                ],
            ),
        ];

        let missing = cases
            .iter()
            .flat_map(|(name, rendered, needles)| {
                needles
                    .iter()
                    .filter(move |needle| !rendered.contains(*needle))
                    .map(move |needle| format!("{name}: {needle}"))
            })
            .collect::<Vec<_>>();

        assert!(
            missing.is_empty(),
            "local supervision projection is missing:\n{}",
            missing.join("\n")
        );
        // Prevent accidental mutation of fixture views while adding the matrix.
        stream.assistant_stream.clear();
        assert!(stream.assistant_stream.is_empty());
    }

    fn runtime_from_fixture(contents: &str) -> RuntimeViewState {
        runtime_from_fixture_prefix(contents, usize::MAX)
    }

    fn runtime_from_fixture_prefix(contents: &str, event_limit: usize) -> RuntimeViewState {
        #[derive(serde::Deserialize)]
        struct Fixture {
            initial_snapshot: RuntimeSnapshot,
            events: Vec<RuntimeEventEnvelope>,
        }

        let fixture: Fixture = serde_json::from_str(contents).expect("frontend fixture");
        let mut runtime = RuntimeViewState::new(fixture.initial_snapshot);
        for envelope in fixture.events.iter().take(event_limit) {
            if let RuntimeWireEvent::Known(event) = &envelope.event {
                runtime.apply_event(event);
            }
        }
        runtime
    }

    fn runtime_snapshot() -> RuntimeSnapshot {
        RuntimeSnapshot {
            cwd: PathBuf::from("/workspace"),
            provider_family: "provider-1".to_string(),
            model_label: "model-1".to_string(),
            work_mode: WorkMode::Build,
            permission_mode: PermissionMode::Default,
            permission_level: PermissionLevel::Ask,
            config_summary: "fixture".to_string(),
            loaded_config_files: Vec::new(),
            startup_overrides: Vec::new(),
            ui_preferences: Default::default(),
        }
    }
}
