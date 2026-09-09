use std::collections::BTreeMap;

use viden_types::{
    AgentNextAction, AgentTaskStatus, ApprovalResponse, AuditActor, AuditObjectRef, AuditOutcome,
    AuditRecord, ConflictBounce, ConflictBounceStatus, ContractDecision, ContractRecord,
    DependencyRecord, DependencyState, HandoffAcceptance, HandoffRecord, MergeGateDecision,
    MergeGateDecisionOutcome, MergeGateValidator, RevertRecord, ReviewRequestRecord,
    ReviewRequestStatus, RuntimeEvent, RuntimeEventKind, RuntimeOwner, fresh_id, now_timestamp,
    truncate_for_preview,
};

use crate::SessionEngine;

/// Builds one bounded audit fact for an accepted trust mutation.
///
/// `action` is a stable dotted key and `args` are stable tokens: the trust
/// summaries and reasons deliberately stay out, because they can carry user or
/// model prose while the audit timeline must stay translatable and diffable.
pub(crate) fn trust_audit_record(
    audit_id: &str,
    timestamp: u64,
    owner: &RuntimeOwner,
    action: &str,
    objects: Vec<AuditObjectRef>,
    args: &[(&str, &str)],
) -> Result<AuditRecord, String> {
    AuditRecord::sanitized(
        audit_id.to_string(),
        timestamp,
        owner.clone(),
        // Trust mutations reach the runtime through an operator-authorized
        // command, and the approval gate has already run at this point.
        AuditActor::Operator,
        action.to_string(),
        objects,
        // These sites run only after validation and permission succeed.
        AuditOutcome::Success,
        args.iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect::<BTreeMap<_, _>>(),
    )
}

impl SessionEngine {
    pub(crate) fn trust_mutation_permission_descriptor(
        &self,
        command: &viden_types::RuntimeCommand,
    ) -> Result<Option<(&'static str, String)>, String> {
        self.validate_trust_command(command)
    }

    pub(crate) fn validate_trust_command(
        &self,
        command: &viden_types::RuntimeCommand,
    ) -> Result<Option<(&'static str, String)>, String> {
        let descriptor = match command {
            viden_types::RuntimeCommand::CreateHandoff {
                handoff_id,
                task_id,
                from_lane_id,
                to_lane_id,
                owner,
                summary,
                ..
            } => {
                validate_trust_id("handoff_id", handoff_id)?;
                validate_trust_id("task_id", task_id)?;
                validate_trust_id("from_lane_id", from_lane_id)?;
                validate_trust_id("to_lane_id", to_lane_id)?;
                validate_owner(owner, Some(to_lane_id), task_id)?;
                if from_lane_id == to_lane_id {
                    return Err(
                        "handoff requires distinct source and destination lanes".to_string()
                    );
                }
                self.require_runtime_task(task_id)?;
                validate_trust_text("handoff summary", summary.clone(), 500)?;
                ensure_unique(
                    self.runtime_handoffs
                        .iter()
                        .any(|record| record.handoff_id == *handoff_id),
                    "handoff",
                    handoff_id,
                )?;
                (
                    "create_handoff",
                    format!("task={task_id} from={from_lane_id} to={to_lane_id}"),
                )
            }
            viden_types::RuntimeCommand::RequestReview {
                review_id,
                gate_id,
                requester_lane_id,
                reviewer_lane_id,
                owner,
                evidence_ids,
            } => {
                validate_trust_id("review_id", review_id)?;
                validate_trust_id("gate_id", gate_id)?;
                validate_trust_id("requester_lane_id", requester_lane_id)?;
                validate_trust_id("reviewer_lane_id", reviewer_lane_id)?;
                let gate_index = self.require_merge_gate_index(gate_id)?;
                validate_review_requester(
                    &self.runtime_merge_gates[gate_index],
                    requester_lane_id,
                    owner,
                )?;
                if requester_lane_id == reviewer_lane_id {
                    return Err("review requires an independent lane".to_string());
                }
                ensure_unique(
                    self.runtime_review_requests
                        .iter()
                        .any(|record| record.review_id == *review_id),
                    "review request",
                    review_id,
                )?;
                if evidence_ids.is_empty() {
                    return Err("review request requires canonical evidence bindings".to_string());
                }
                for evidence_id in evidence_ids {
                    validate_trust_id("evidence_id", evidence_id)?;
                    if !self
                        .runtime_evidence
                        .iter()
                        .any(|evidence| evidence.id == *evidence_id)
                    {
                        return Err(format!("review evidence `{evidence_id}` does not exist"));
                    }
                }
                self.validated_evidence_bindings_for_ids(gate_index, evidence_ids)?;
                (
                    "request_review",
                    format!("gate={gate_id} reviewer={reviewer_lane_id}"),
                )
            }
            viden_types::RuntimeCommand::DecideReview {
                review_id,
                verdict,
                feedback,
                actor,
            } => {
                self.preflight_decide_review(review_id, feedback.as_deref(), actor)?;
                (
                    "decide_review",
                    format!("review={review_id} verdict={}", verdict.as_token()),
                )
            }
            viden_types::RuntimeCommand::ConfirmContract {
                contract_id,
                task_id,
                owner,
                summary,
                ..
            } => {
                validate_trust_id("contract_id", contract_id)?;
                validate_trust_id("task_id", task_id)?;
                validate_owner(owner, owner.lane_id.as_deref(), task_id)?;
                self.require_runtime_task(task_id)?;
                ensure_unique(
                    self.runtime_contracts
                        .iter()
                        .any(|record| record.contract_id == *contract_id),
                    "contract",
                    contract_id,
                )?;
                validate_trust_text("contract summary", summary.clone(), 500)?;
                (
                    "confirm_contract",
                    format!("task={task_id} contract={contract_id}"),
                )
            }
            viden_types::RuntimeCommand::SetDependency {
                dependency_id,
                task_id,
                depends_on_task_id,
                owner,
                state,
                reason,
            } => {
                validate_trust_id("dependency_id", dependency_id)?;
                validate_trust_id("task_id", task_id)?;
                validate_trust_id("depends_on_task_id", depends_on_task_id)?;
                validate_owner(owner, owner.lane_id.as_deref(), task_id)?;
                self.validate_dependency_mutation(
                    dependency_id,
                    task_id,
                    depends_on_task_id,
                    *state,
                )?;
                validate_trust_text("dependency reason", reason.clone(), 240)?;
                (
                    "set_dependency",
                    format!("task={task_id} dependency={depends_on_task_id} state={state:?}"),
                )
            }
            viden_types::RuntimeCommand::AcceptMergeGate {
                gate_id,
                actor,
                reviewed_evidence,
                decision,
            } => {
                if let Some(decision) = decision {
                    validate_trust_text("merge gate decision", decision.clone(), 240)?;
                }
                self.preflight_accept_merge_gate(gate_id, actor, reviewed_evidence)?;
                ("accept_merge_gate", format!("gate={gate_id}"))
            }
            viden_types::RuntimeCommand::RejectMergeGate {
                gate_id,
                actor,
                reason,
            } => {
                validate_trust_text("merge gate decision", reason.clone(), 240)?;
                self.preflight_reject_merge_gate(gate_id, actor)?;
                ("reject_merge_gate", format!("gate={gate_id}"))
            }
            viden_types::RuntimeCommand::RecordAgentEvidence {
                gate_id,
                evidence_id,
                kind,
                canonical,
                ..
            } => {
                self.preflight_record_agent_evidence(
                    gate_id,
                    evidence_id.as_deref(),
                    kind,
                    canonical.as_ref(),
                )?;
                (
                    "record_agent_evidence",
                    format!("gate={gate_id} kind={}", kind.trim()),
                )
            }
            viden_types::RuntimeCommand::AcceptAgentArtifact {
                gate_id,
                evidence_id,
                actor,
                source_hash,
                decision,
            } => {
                if let Some(decision) = decision {
                    validate_trust_text("agent artifact decision", decision.clone(), 240)?;
                }
                self.preflight_accept_agent_artifact(gate_id, evidence_id, actor, source_hash)?;
                (
                    "accept_agent_artifact",
                    format!("gate={gate_id} evidence={evidence_id}"),
                )
            }
            viden_types::RuntimeCommand::RejectAgentArtifact {
                gate_id,
                evidence_id,
                actor,
                reason,
            } => {
                validate_trust_text("agent artifact rejection reason", reason.clone(), 240)?;
                self.preflight_reject_agent_artifact(gate_id, evidence_id, actor)?;
                (
                    "reject_agent_artifact",
                    format!("gate={gate_id} evidence={evidence_id}"),
                )
            }
            viden_types::RuntimeCommand::MergeAgentPatch {
                gate_id,
                actor,
                decision,
            } => {
                if let Some(decision) = decision {
                    validate_trust_text("patch merge decision", decision.clone(), 240)?;
                }
                self.preflight_merge_agent_patch(gate_id, actor)?;
                ("merge_agent_patch", format!("gate={gate_id}"))
            }
            viden_types::RuntimeCommand::RevalidateMergeConflict {
                gate_id,
                bounce_id,
                actor,
                evidence,
            } => {
                self.validate_conflict_revalidation(gate_id, bounce_id, actor, evidence)?;
                (
                    "revalidate_merge_conflict",
                    format!("gate={gate_id} bounce={bounce_id}"),
                )
            }
            viden_types::RuntimeCommand::BounceMergeConflict {
                gate_id,
                original_lane_id,
                owner,
                reason,
            } => {
                let gate_index = self.require_merge_gate_index(gate_id)?;
                validate_trust_id("original_lane_id", original_lane_id)?;
                validate_owner(
                    owner,
                    owner.lane_id.as_deref(),
                    &self.runtime_merge_gates[gate_index].task_id,
                )?;
                validate_trust_text("conflict reason", reason.clone(), 500)?;
                self.validate_conflict_bounce(gate_index, original_lane_id, owner)?;
                (
                    "bounce_merge_conflict",
                    format!("gate={gate_id} origin={original_lane_id}"),
                )
            }
            viden_types::RuntimeCommand::RevertAppliedChange {
                gate_id,
                owner,
                reason,
            } => {
                let gate_index = self.require_merge_gate_index(gate_id)?;
                validate_owner(
                    owner,
                    owner.lane_id.as_deref(),
                    &self.runtime_merge_gates[gate_index].task_id,
                )?;
                if self.runtime_merge_gates[gate_index].status
                    != viden_types::MergeGateStatus::Merged
                {
                    return Err(format!(
                        "merge gate `{gate_id}` has no applied change to revert"
                    ));
                }
                let change_id = self.runtime_merge_gates[gate_index]
                    .applied_change_id
                    .as_deref()
                    .ok_or_else(|| {
                        format!("merge gate `{gate_id}` is missing applied change identity")
                    })?;
                if self.runtime_merge_gates[gate_index]
                    .recovery_snapshot
                    .is_some()
                {
                    self.load_recovery_rollbacks_for_gate(gate_index)?;
                } else if !self.applied_change_rollbacks.contains_key(change_id) {
                    return Err(format!(
                        "applied change `{change_id}` has no recovery snapshot"
                    ));
                }
                validate_trust_text("revert reason", reason.clone(), 500)?;
                (
                    "revert_applied_change",
                    format!("gate={gate_id} change={change_id}"),
                )
            }
            _ => return Ok(None),
        };
        Ok(Some(descriptor))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_handoff<F>(
        &mut self,
        handoff_id: String,
        task_id: String,
        from_lane_id: String,
        to_lane_id: String,
        owner: RuntimeOwner,
        summary: String,
        acceptance: HandoffAcceptance,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        validate_trust_id("handoff_id", &handoff_id)?;
        validate_trust_id("task_id", &task_id)?;
        validate_trust_id("from_lane_id", &from_lane_id)?;
        validate_trust_id("to_lane_id", &to_lane_id)?;
        validate_owner(&owner, Some(&to_lane_id), &task_id)?;
        if from_lane_id == to_lane_id {
            return Err("handoff requires distinct source and destination lanes".to_string());
        }
        self.require_runtime_task(&task_id)?;
        let summary = validate_trust_text("handoff summary", summary, 500)?;
        ensure_unique(
            self.runtime_handoffs
                .iter()
                .any(|record| record.handoff_id == handoff_id),
            "handoff",
            &handoff_id,
        )?;
        self.require_trust_permission(
            "create_handoff",
            &format!("task={task_id} from={from_lane_id} to={to_lane_id}"),
            approver,
        )?;

        let now = now_timestamp();
        let audit_id = fresh_id("audit");
        self.append_trust_audit(&trust_audit_record(
            &audit_id,
            now,
            &owner,
            "handoff.created",
            vec![
                AuditObjectRef::new(AuditObjectRef::KIND_HANDOFF, &handoff_id),
                AuditObjectRef::new(AuditObjectRef::KIND_TASK, &task_id),
                AuditObjectRef::new(AuditObjectRef::KIND_LANE, &from_lane_id),
                AuditObjectRef::new(AuditObjectRef::KIND_LANE, &to_lane_id),
            ],
            &[(
                "acceptance",
                match acceptance {
                    HandoffAcceptance::Accepted => "accepted",
                    HandoffAcceptance::Rejected => "rejected",
                },
            )],
        )?)?;
        let handoff = HandoffRecord {
            handoff_id,
            task_id: task_id.clone(),
            from_lane_id,
            to_lane_id,
            owner: owner.clone(),
            summary,
            acceptance,
            audit_id: audit_id.clone(),
            updated_at: now,
        };
        self.runtime_handoffs.push(handoff.clone());
        let mut gate_update = None;
        if acceptance == HandoffAcceptance::Accepted
            && let Some(gate) = self
                .runtime_merge_gates
                .iter_mut()
                .find(|gate| gate.task_id == task_id)
        {
            gate.owner = owner;
            gate.audit_ids.push(audit_id);
            gate.updated_at = Some(now);
            gate_update = Some(gate.clone());
        }
        let mut events = vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::HandoffUpdated { handoff },
        )];
        if let Some(gate) = gate_update {
            events.push(RuntimeEvent::new(
                2,
                RuntimeEventKind::MergeGateUpdated { gate },
            ));
        }
        Ok(events)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn request_review<F>(
        &mut self,
        review_id: String,
        gate_id: String,
        requester_lane_id: String,
        reviewer_lane_id: String,
        owner: RuntimeOwner,
        evidence_ids: Vec<String>,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        validate_trust_id("review_id", &review_id)?;
        validate_trust_id("gate_id", &gate_id)?;
        validate_trust_id("requester_lane_id", &requester_lane_id)?;
        validate_trust_id("reviewer_lane_id", &reviewer_lane_id)?;
        if requester_lane_id == reviewer_lane_id {
            return Err("review requires an independent lane".to_string());
        }
        ensure_unique(
            self.runtime_review_requests
                .iter()
                .any(|record| record.review_id == review_id),
            "review request",
            &review_id,
        )?;
        let gate_index = self.require_merge_gate_index(&gate_id)?;
        let task_id = self.runtime_merge_gates[gate_index].task_id.clone();
        validate_review_requester(
            &self.runtime_merge_gates[gate_index],
            &requester_lane_id,
            &owner,
        )?;
        if evidence_ids.is_empty() {
            return Err("review request requires canonical evidence bindings".to_string());
        }
        for evidence_id in &evidence_ids {
            validate_trust_id("evidence_id", evidence_id)?;
            if !self
                .runtime_evidence
                .iter()
                .any(|evidence| evidence.id == *evidence_id)
            {
                return Err(format!("review evidence `{evidence_id}` does not exist"));
            }
        }
        let evidence_bindings =
            self.validated_evidence_bindings_for_ids(gate_index, &evidence_ids)?;
        self.require_trust_permission(
            "request_review",
            &format!("gate={gate_id} reviewer={reviewer_lane_id}"),
            approver,
        )?;

        let now = now_timestamp();
        let audit_id = fresh_id("audit");
        let decision_audit_id = fresh_id("audit");
        let mut review_objects = vec![
            AuditObjectRef::new(AuditObjectRef::KIND_REVIEW_REQUEST, &review_id),
            AuditObjectRef::new(AuditObjectRef::KIND_MERGE_GATE, &gate_id),
            AuditObjectRef::new(AuditObjectRef::KIND_TASK, &task_id),
            AuditObjectRef::new(AuditObjectRef::KIND_LANE, &requester_lane_id),
            AuditObjectRef::new(AuditObjectRef::KIND_LANE, &reviewer_lane_id),
        ];
        // Evidence links are bounded like every other audit field. When a
        // review binds more evidence than the object bound allows, the record
        // keeps the first references and `evidence_count` carries the total,
        // so the count is never silently wrong.
        let evidence_count = evidence_ids.len();
        let evidence_link_budget = viden_types::MAX_AUDIT_OBJECTS - review_objects.len();
        review_objects.extend(
            evidence_ids
                .iter()
                .take(evidence_link_budget)
                .map(|evidence_id| AuditObjectRef::new(AuditObjectRef::KIND_EVIDENCE, evidence_id)),
        );
        self.append_trust_audit(&trust_audit_record(
            &audit_id,
            now,
            &owner,
            "review.requested",
            review_objects,
            &[
                ("status", "pending"),
                ("evidence_count", &evidence_count.to_string()),
            ],
        )?)?;
        // The AwaitingEvidence stamp this site writes onto the gate is
        // bookkeeping during evidence collection, not an operator decision, so
        // it is deliberately not audited: `gate.decided` means the
        // permission-gated accept/reject only. `review.requested` above is the
        // operator action here.
        let review = ReviewRequestRecord {
            review_id: review_id.clone(),
            gate_id,
            task_id,
            requester_lane_id,
            reviewer_lane_id: reviewer_lane_id.clone(),
            owner: owner.clone(),
            evidence_ids,
            evidence_bindings,
            status: ReviewRequestStatus::Pending,
            feedback: None,
            audit_id: audit_id.clone(),
            updated_at: now,
        };
        self.runtime_review_requests.push(review.clone());
        let gate = &mut self.runtime_merge_gates[gate_index];
        gate.validator = Some(MergeGateValidator {
            owner: reviewer_owner_from_requester(&owner, &reviewer_lane_id),
            review_request_id: review_id,
            independent: true,
            validated_at: None,
        });
        gate.policy_snapshot.requires_independent_validator = true;
        gate.status = viden_types::MergeGateStatus::CollectingEvidence;
        gate.decision = Some(MergeGateDecision::decided_now(
            MergeGateDecisionOutcome::AwaitingEvidence,
            "independent_review_required".to_string(),
            gate.owner.clone(),
            gate.evidence_ids.clone(),
            decision_audit_id,
        ));
        gate.audit_ids.push(audit_id);
        gate.updated_at = Some(now);
        Ok(vec![
            RuntimeEvent::new(1, RuntimeEventKind::ReviewRequestUpdated { review }),
            RuntimeEvent::new(2, RuntimeEventKind::MergeGateUpdated { gate: gate.clone() }),
        ])
    }

    /// Validates a review decision without mutating anything.
    ///
    /// Shared by the permission descriptor and the mutation so the classified
    /// action and the executed action can never disagree about who may decide.
    /// Returns the index of the pending review.
    pub(crate) fn preflight_decide_review(
        &self,
        review_id: &str,
        feedback: Option<&str>,
        actor: &RuntimeOwner,
    ) -> Result<usize, String> {
        validate_trust_id("review_id", review_id)?;
        let review_index = self
            .runtime_review_requests
            .iter()
            .position(|review| review.review_id == review_id)
            .ok_or_else(|| format!("review request `{review_id}` does not exist"))?;
        let review = &self.runtime_review_requests[review_index];
        if review.status != ReviewRequestStatus::Pending {
            return Err(format!("review `{review_id}` is already decided"));
        }
        validate_review_decider(review, actor)?;
        // Evidence-binding drift guard: the verdict may only settle the exact
        // canonical receipts the reviewer was asked about. Anything recorded on
        // the gate since the request invalidates the review instead of being
        // silently blessed by it.
        let gate_index = self.require_merge_gate_index(&review.gate_id)?;
        let current = self
            .validated_evidence_bindings_for_ids(gate_index, &review.evidence_ids)
            .map_err(|error| {
                format!("review evidence drifted since the request; re-request review: {error}")
            })?;
        if current != review.evidence_bindings {
            return Err("review evidence drifted since the request; re-request review".to_string());
        }
        if let Some(feedback) = feedback {
            validate_trust_text("review feedback", feedback.to_string(), 500)?;
        }
        Ok(review_index)
    }

    pub(crate) fn decide_review<F>(
        &mut self,
        review_id: String,
        verdict: viden_types::ReviewVerdict,
        feedback: Option<String>,
        actor: RuntimeOwner,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        let review_index = self.preflight_decide_review(&review_id, feedback.as_deref(), &actor)?;
        let feedback = feedback
            .map(|feedback| validate_trust_text("review feedback", feedback, 500))
            .transpose()?;
        self.require_trust_permission(
            "decide_review",
            &format!("review={review_id} verdict={}", verdict.as_token()),
            approver,
        )?;

        let now = now_timestamp();
        let audit_id = fresh_id("audit");
        let review = &self.runtime_review_requests[review_index];
        let gate_id = review.gate_id.clone();
        let task_id = review.task_id.clone();
        let requester_lane_id = review.requester_lane_id.clone();
        let reviewer_lane_id = review.reviewer_lane_id.clone();
        let gate_index = self.require_merge_gate_index(&gate_id)?;
        // Audit before mutation, the same rule as every other trust site: the
        // reviewer verdict is what later authorizes (or blocks) the merge gate,
        // so a failed append must fail the decision instead of leaving an
        // unauditable settlement. Reviewer prose never enters the audit args.
        self.append_trust_audit(&trust_audit_record(
            &audit_id,
            now,
            &actor,
            "review.decided",
            vec![
                AuditObjectRef::new(AuditObjectRef::KIND_REVIEW_REQUEST, &review_id),
                AuditObjectRef::new(AuditObjectRef::KIND_MERGE_GATE, &gate_id),
                AuditObjectRef::new(AuditObjectRef::KIND_TASK, &task_id),
                AuditObjectRef::new(AuditObjectRef::KIND_LANE, &requester_lane_id),
                AuditObjectRef::new(AuditObjectRef::KIND_LANE, &reviewer_lane_id),
            ],
            &[("verdict", verdict.as_token())],
        )?)?;

        let review = &mut self.runtime_review_requests[review_index];
        review.status = ReviewRequestStatus::from(verdict);
        review.feedback = feedback;
        review.audit_id = audit_id.clone();
        review.updated_at = now;
        let review = review.clone();

        let gate = &mut self.runtime_merge_gates[gate_index];
        // Only the validator this review was installed for is stamped: a gate
        // whose validator moved on to another review must not inherit this
        // verdict's independent validation.
        if verdict == viden_types::ReviewVerdict::Accepted
            && let Some(validator) = gate.validator.as_mut()
            && validator.review_request_id == review_id
        {
            validator.validated_at = Some(now);
        }
        // A rejection leaves the gate status to the operator's own gate
        // decision, but the gate still joins this audit fact either way.
        gate.audit_ids.push(audit_id);
        gate.updated_at = Some(now);
        let gate = gate.clone();

        Ok(vec![
            RuntimeEvent::new(1, RuntimeEventKind::ReviewRequestUpdated { review }),
            RuntimeEvent::new(2, RuntimeEventKind::MergeGateUpdated { gate }),
        ])
    }

    pub(crate) fn confirm_contract<F>(
        &mut self,
        contract_id: String,
        task_id: String,
        owner: RuntimeOwner,
        summary: String,
        decision: ContractDecision,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        validate_trust_id("contract_id", &contract_id)?;
        validate_trust_id("task_id", &task_id)?;
        validate_owner(&owner, owner.lane_id.as_deref(), &task_id)?;
        self.require_runtime_task(&task_id)?;
        ensure_unique(
            self.runtime_contracts
                .iter()
                .any(|record| record.contract_id == contract_id),
            "contract",
            &contract_id,
        )?;
        let summary = validate_trust_text("contract summary", summary, 500)?;
        self.require_trust_permission(
            "confirm_contract",
            &format!("task={task_id} contract={contract_id}"),
            approver,
        )?;
        let now = now_timestamp();
        let audit_id = fresh_id("audit");
        self.append_trust_audit(&trust_audit_record(
            &audit_id,
            now,
            &owner,
            "contract.confirmed",
            vec![
                AuditObjectRef::new(AuditObjectRef::KIND_CONTRACT, &contract_id),
                AuditObjectRef::new(AuditObjectRef::KIND_TASK, &task_id),
            ],
            &[(
                "decision",
                match decision {
                    ContractDecision::Confirmed => "confirmed",
                    ContractDecision::Rejected => "rejected",
                },
            )],
        )?)?;
        let contract = ContractRecord {
            contract_id,
            task_id,
            owner,
            summary,
            decision,
            audit_id,
            updated_at: now,
        };
        self.runtime_contracts.push(contract.clone());
        Ok(vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::ContractUpdated { contract },
        )])
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn set_dependency<F>(
        &mut self,
        dependency_id: String,
        task_id: String,
        depends_on_task_id: String,
        owner: RuntimeOwner,
        state: DependencyState,
        reason: String,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        validate_trust_id("dependency_id", &dependency_id)?;
        validate_trust_id("task_id", &task_id)?;
        validate_trust_id("depends_on_task_id", &depends_on_task_id)?;
        validate_owner(&owner, owner.lane_id.as_deref(), &task_id)?;
        self.validate_dependency_mutation(&dependency_id, &task_id, &depends_on_task_id, state)?;
        let reason = validate_trust_text("dependency reason", reason, 240)?;
        self.require_trust_permission(
            "set_dependency",
            &format!("task={task_id} dependency={depends_on_task_id} state={state:?}"),
            approver,
        )?;

        let now = now_timestamp();
        let audit_id = fresh_id("audit");
        self.append_trust_audit(&trust_audit_record(
            &audit_id,
            now,
            &owner,
            "dependency.set",
            vec![
                AuditObjectRef::new(AuditObjectRef::KIND_DEPENDENCY, &dependency_id),
                AuditObjectRef::new(AuditObjectRef::KIND_TASK, &task_id),
                AuditObjectRef::new(AuditObjectRef::KIND_TASK, &depends_on_task_id),
            ],
            &[(
                "state",
                match state {
                    DependencyState::Blocked => "blocked",
                    DependencyState::Unblocked => "unblocked",
                },
            )],
        )?)?;
        let dependency = DependencyRecord {
            dependency_id,
            task_id: task_id.clone(),
            depends_on_task_id,
            owner,
            state,
            reason: reason.clone(),
            audit_id,
            updated_at: now,
        };
        upsert_dependency(&mut self.runtime_dependencies, dependency.clone());
        let mut events = vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::DependencyUpdated { dependency },
        )];
        if let Some(mut task) = self
            .runtime_tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
        {
            if let Some(blocker) = self.blocking_dependency_for_task(&task_id) {
                task.status = AgentTaskStatus::Blocked;
                task.activity = format!("dependency blocked: {blocker}");
            } else if task.status == AgentTaskStatus::Blocked {
                task.status = AgentTaskStatus::Queued;
                task.activity = format!("dependency unblocked: {reason}");
            }
            task.updated_at = Some(now.saturating_mul(1000));
            self.upsert_agent_task(task.clone());
            events.push(RuntimeEvent::new(2, RuntimeEventKind::TaskUpdated { task }));
        }
        Ok(events)
    }

    fn validate_dependency_mutation(
        &self,
        dependency_id: &str,
        task_id: &str,
        depends_on_task_id: &str,
        state: DependencyState,
    ) -> Result<(), String> {
        self.require_runtime_task(task_id)?;
        self.require_runtime_task(depends_on_task_id)
            .map_err(|_| format!("dependency task `{depends_on_task_id}` does not exist"))?;
        if task_id == depends_on_task_id {
            return Err(format!("agent task `{task_id}` cannot depend on itself"));
        }
        let mut edges = std::collections::BTreeMap::<String, Vec<String>>::new();
        for dag in &self.runtime_agent_dags {
            for task in &dag.tasks {
                edges
                    .entry(task.task_id.clone())
                    .or_default()
                    .extend(task.dependencies.iter().cloned());
            }
        }
        for dependency in &self.runtime_dependencies {
            if dependency.dependency_id == dependency_id
                && (dependency.task_id != task_id
                    || dependency.depends_on_task_id != depends_on_task_id)
            {
                return Err(format!(
                    "dependency id `{dependency_id}` is already bound to different endpoints"
                ));
            }
            if dependency.dependency_id != dependency_id
                && dependency.state == DependencyState::Blocked
            {
                edges
                    .entry(dependency.task_id.clone())
                    .or_default()
                    .push(dependency.depends_on_task_id.clone());
            }
        }
        if state == DependencyState::Unblocked {
            return Ok(());
        }
        edges
            .entry(task_id.to_string())
            .or_default()
            .push(depends_on_task_id.to_string());
        if dependency_path_exists(&edges, depends_on_task_id, task_id) {
            return Err(format!(
                "dependency cycle would connect `{task_id}` and `{depends_on_task_id}`"
            ));
        }
        Ok(())
    }

    pub(crate) fn blocking_dependency_for_task(&self, task_id: &str) -> Option<String> {
        let static_blocker = self.runtime_agent_dags.iter().find_map(|dag| {
            let spec = dag.tasks.iter().find(|task| task.task_id == task_id)?;
            spec.dependencies.iter().find_map(|dependency| {
                let task = self
                    .runtime_tasks
                    .iter()
                    .find(|task| task.id == *dependency)?;
                (!matches!(
                    task.status,
                    AgentTaskStatus::Done | AgentTaskStatus::Applied | AgentTaskStatus::Archived
                ))
                .then(|| dependency.clone())
            })
        });
        static_blocker.or_else(|| {
            self.runtime_dependencies
                .iter()
                .find(|dependency| {
                    dependency.task_id == task_id && dependency.state == DependencyState::Blocked
                })
                .map(|dependency| dependency.depends_on_task_id.clone())
        })
    }

    pub(crate) fn bounce_merge_conflict<F>(
        &mut self,
        gate_id: String,
        original_lane_id: String,
        owner: RuntimeOwner,
        reason: String,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        validate_trust_id("gate_id", &gate_id)?;
        validate_trust_id("original_lane_id", &original_lane_id)?;
        let gate_index = self.require_merge_gate_index(&gate_id)?;
        let task_id = self.runtime_merge_gates[gate_index].task_id.clone();
        validate_owner(&owner, owner.lane_id.as_deref(), &task_id)?;
        let reason = validate_trust_text("conflict reason", reason, 500)?;
        self.validate_conflict_bounce(gate_index, &original_lane_id, &owner)?;
        self.require_trust_permission(
            "bounce_merge_conflict",
            &format!("gate={gate_id} origin={original_lane_id}"),
            approver,
        )?;
        self.record_conflict_bounce(gate_index, original_lane_id, owner, reason)
    }

    pub(crate) fn validate_conflict_bounce(
        &self,
        gate_index: usize,
        original_lane_id: &str,
        owner: &RuntimeOwner,
    ) -> Result<Vec<viden_types::ReviewedEvidenceBinding>, String> {
        let gate = &self.runtime_merge_gates[gate_index];
        if gate.owner.lane_id.as_deref() != Some(original_lane_id) || owner != &gate.owner {
            return Err("merge conflict bounce must target the gate owner origin lane".to_string());
        }
        self.validated_evidence_bindings_for_ids(gate_index, &gate.evidence_ids)
            .map_err(|error| {
                format!("merge conflict bounce requires valid canonical evidence baseline: {error}")
            })
    }

    pub(crate) fn record_conflict_bounce(
        &mut self,
        gate_index: usize,
        original_lane_id: String,
        owner: RuntimeOwner,
        reason: String,
    ) -> Result<Vec<RuntimeEvent>, String> {
        let now = now_timestamp();
        let gate_id = self.runtime_merge_gates[gate_index].gate_id.clone();
        let task_id = self.runtime_merge_gates[gate_index].task_id.clone();
        let baseline_evidence =
            self.validate_conflict_bounce(gate_index, &original_lane_id, &owner)?;
        let audit_id = fresh_id("audit");
        let bounce_id = fresh_id("conflict");
        self.append_trust_audit(&trust_audit_record(
            &audit_id,
            now,
            &owner,
            "conflict.bounced",
            vec![
                AuditObjectRef::new(AuditObjectRef::KIND_MERGE_GATE, &gate_id),
                AuditObjectRef::new(AuditObjectRef::KIND_TASK, &task_id),
                AuditObjectRef::new(AuditObjectRef::KIND_LANE, &original_lane_id),
            ],
            &[("status", "pending"), ("bounce_id", &bounce_id)],
        )?)?;
        let conflict = ConflictBounce {
            bounce_id,
            gate_id,
            task_id: task_id.clone(),
            original_lane_id,
            owner: owner.clone(),
            reason: reason.clone(),
            status: ConflictBounceStatus::Pending,
            evidence_ids: self.runtime_merge_gates[gate_index].evidence_ids.clone(),
            baseline_evidence,
            revalidation_evidence: Vec::new(),
            content: None,
            audit_id: audit_id.clone(),
            created_at: now,
            revalidated_at: None,
        };
        self.runtime_conflict_bounces.push(conflict.clone());
        let gate = &mut self.runtime_merge_gates[gate_index];
        gate.status = viden_types::MergeGateStatus::NeedsChanges;
        gate.conflict = Some(conflict.clone());
        gate.decision = Some(MergeGateDecision::decided_now(
            MergeGateDecisionOutcome::Conflict,
            reason.clone(),
            owner,
            gate.evidence_ids.clone(),
            audit_id.clone(),
        ));
        gate.audit_ids.push(audit_id);
        gate.updated_at = Some(now);
        let gate = gate.clone();
        let mut events = vec![
            RuntimeEvent::new(
                1,
                RuntimeEventKind::MergeConflictBounced {
                    conflict: conflict.clone(),
                },
            ),
            RuntimeEvent::new(2, RuntimeEventKind::MergeGateUpdated { gate }),
        ];
        if let Some(mut task) = self
            .runtime_tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
        {
            task.status = AgentTaskStatus::NeedsInput;
            task.activity = reason;
            task.next_action = Some(AgentNextAction {
                label: "resolve merge conflict".to_string(),
                command: Some(format!("/lane attach {}", conflict.original_lane_id)),
                reason: Some("merge conflict returned to the originating lane".to_string()),
            });
            task.updated_at = Some(now.saturating_mul(1000));
            self.upsert_agent_task(task.clone());
            events.push(RuntimeEvent::new(3, RuntimeEventKind::TaskUpdated { task }));
        }
        Ok(events)
    }

    pub(crate) fn revalidate_merge_conflict<F>(
        &mut self,
        gate_id: String,
        bounce_id: String,
        actor: RuntimeOwner,
        evidence: viden_types::ReviewedEvidenceBinding,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        let (gate_index, conflict) =
            self.validate_conflict_revalidation(&gate_id, &bounce_id, &actor, &evidence)?;
        self.require_trust_permission(
            "revalidate_merge_conflict",
            &format!("gate={gate_id} bounce={bounce_id}"),
            approver,
        )?;

        let now = now_timestamp();
        let audit_id = fresh_id("audit");
        // Audit before mutation: revalidation is the origin lane's
        // permission-gated claim that the conflict is resolved.
        let task_id = self.runtime_merge_gates[gate_index].task_id.clone();
        self.append_trust_audit(&trust_audit_record(
            &audit_id,
            now,
            &actor,
            "conflict.revalidated",
            vec![
                AuditObjectRef::new(AuditObjectRef::KIND_MERGE_GATE, &gate_id),
                AuditObjectRef::new(AuditObjectRef::KIND_TASK, &task_id),
                AuditObjectRef::new(AuditObjectRef::KIND_CONFLICT, &bounce_id),
                AuditObjectRef::new(AuditObjectRef::KIND_EVIDENCE, &evidence.evidence_id),
            ],
            &[("status", "revalidated")],
        )?)?;
        let conflict = &mut self.runtime_conflict_bounces[conflict];
        conflict.status = ConflictBounceStatus::Revalidated;
        conflict.revalidated_at = Some(now);
        conflict.revalidation_evidence = vec![evidence];
        let conflict = conflict.clone();
        let gate = &mut self.runtime_merge_gates[gate_index];
        gate.conflict = Some(conflict.clone());
        gate.status = viden_types::MergeGateStatus::CollectingEvidence;
        gate.decision = Some(MergeGateDecision::decided_now(
            MergeGateDecisionOutcome::AwaitingEvidence,
            "revalidated_conflict_requires_review".to_string(),
            actor,
            gate.evidence_ids.clone(),
            audit_id,
        ));
        gate.updated_at = Some(now);
        Ok(vec![
            RuntimeEvent::new(
                1,
                RuntimeEventKind::MergeConflictBounced {
                    conflict: conflict.clone(),
                },
            ),
            RuntimeEvent::new(2, RuntimeEventKind::MergeGateUpdated { gate: gate.clone() }),
        ])
    }

    fn validate_conflict_revalidation(
        &self,
        gate_id: &str,
        bounce_id: &str,
        actor: &RuntimeOwner,
        evidence: &viden_types::ReviewedEvidenceBinding,
    ) -> Result<(usize, usize), String> {
        validate_trust_id("gate_id", gate_id)?;
        validate_trust_id("bounce_id", bounce_id)?;
        validate_trust_id("evidence_id", &evidence.evidence_id)?;
        let gate_index = self.require_merge_gate_index(gate_id)?;
        let gate = &self.runtime_merge_gates[gate_index];
        validate_owner(actor, actor.lane_id.as_deref(), &gate.task_id)?;
        let gate_conflict = gate
            .conflict
            .as_ref()
            .ok_or_else(|| format!("merge gate `{gate_id}` has no conflict to revalidate"))?;
        if gate_conflict.bounce_id != bounce_id {
            return Err("merge conflict bounce identity mismatch".to_string());
        }
        if gate_conflict.status != ConflictBounceStatus::Pending {
            return Err("merge conflict is not pending revalidation".to_string());
        }
        if actor.lane_id.as_deref() != Some(gate_conflict.original_lane_id.as_str())
            || actor != &gate_conflict.owner
        {
            return Err("merge conflict revalidation must come from the origin lane".to_string());
        }
        let current = self.validated_evidence_bindings_for_ids(
            gate_index,
            std::slice::from_ref(&evidence.evidence_id),
        )?;
        if current.as_slice() != std::slice::from_ref(evidence) {
            return Err("merge conflict evidence hash does not match canonical bytes".to_string());
        }
        if gate_conflict
            .baseline_evidence
            .iter()
            .any(|baseline| baseline.source_hash == evidence.source_hash)
        {
            return Err(
                "merge conflict revalidation requires a changed canonical receipt".to_string(),
            );
        }
        let stored = self
            .runtime_evidence
            .iter()
            .find(|stored| stored.id == evidence.evidence_id)
            .and_then(|stored| stored.canonical.as_ref())
            .ok_or_else(|| "merge conflict evidence is not canonical".to_string())?;
        if stored.producer.identity != gate_conflict.original_lane_id {
            return Err("merge conflict evidence producer is not the origin lane".to_string());
        }
        let conflict_index = self
            .runtime_conflict_bounces
            .iter()
            .position(|conflict| conflict.bounce_id == bounce_id)
            .ok_or_else(|| "merge conflict bounce fact does not exist".to_string())?;
        Ok((gate_index, conflict_index))
    }

    pub(crate) fn mark_conflict_resolved(&mut self, gate_index: usize) -> Option<ConflictBounce> {
        let conflict = self.runtime_merge_gates[gate_index].conflict.as_mut()?;
        conflict.status = ConflictBounceStatus::Resolved;
        let conflict = conflict.clone();
        upsert_conflict(&mut self.runtime_conflict_bounces, conflict.clone());
        Some(conflict)
    }

    pub(crate) fn revert_applied_change<F>(
        &mut self,
        gate_id: String,
        owner: RuntimeOwner,
        reason: String,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        validate_trust_id("gate_id", &gate_id)?;
        let gate_index = self.require_merge_gate_index(&gate_id)?;
        let task_id = self.runtime_merge_gates[gate_index].task_id.clone();
        validate_owner(&owner, owner.lane_id.as_deref(), &task_id)?;
        if self.runtime_merge_gates[gate_index].status != viden_types::MergeGateStatus::Merged {
            return Err(format!(
                "merge gate `{gate_id}` has no applied change to revert"
            ));
        }
        let applied_change_id = self.runtime_merge_gates[gate_index]
            .applied_change_id
            .clone()
            .ok_or_else(|| format!("merge gate `{gate_id}` is missing applied change identity"))?;
        let original = if self.runtime_merge_gates[gate_index]
            .recovery_snapshot
            .is_some()
        {
            self.load_recovery_rollbacks_for_gate(gate_index)?
        } else {
            self.applied_change_rollbacks
                .get(&applied_change_id)
                .cloned()
                .ok_or_else(|| {
                    format!("applied change `{applied_change_id}` has no recovery snapshot")
                })?
        };
        let reason = validate_trust_text("revert reason", reason, 500)?;
        self.require_trust_permission(
            "revert_applied_change",
            &format!("gate={gate_id} change={applied_change_id}"),
            approver,
        )?;

        let paths = original
            .iter()
            .map(|rollback| rollback.path.clone())
            .collect::<Vec<_>>();
        let now = now_timestamp();
        let audit_id = fresh_id("audit");
        let restored_paths = paths
            .iter()
            .filter_map(|path| path.strip_prefix(&self.cwd).ok())
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect::<Vec<_>>();
        let revert_id = fresh_id("revert");
        // Audit strictly before any byte touches the disk.
        //
        // Deliberate tradeoff: if restoration fails after this append, the
        // audit record is orphaned (an audit fact with no matching
        // RevertRecord), which is detectable evidence that a revert was
        // attempted. The reverse order — restore first, audit second — would
        // instead leave files already reverted with no audit fact, no
        // RevertRecord, and the gate still Merged: a silent, untraceable disk
        // mutation. An orphaned record beats an invisible change.
        self.append_trust_audit(&trust_audit_record(
            &audit_id,
            now,
            &owner,
            "change.reverted",
            vec![
                AuditObjectRef::new(AuditObjectRef::KIND_REVERT, &revert_id),
                AuditObjectRef::new(AuditObjectRef::KIND_APPLIED_CHANGE, &applied_change_id),
                AuditObjectRef::new(AuditObjectRef::KIND_MERGE_GATE, &gate_id),
                AuditObjectRef::new(AuditObjectRef::KIND_TASK, &task_id),
            ],
            &[("restored_path_count", &restored_paths.len().to_string())],
        )?)?;
        self.stage_rollback_paths(&paths)?;
        self.persist_merge_gate_precommit(gate_index, "audit-revert-precommit")?;
        self.restore_file_rollbacks(&original)?;

        let revert = RevertRecord {
            revert_id,
            gate_id,
            applied_change_id: applied_change_id.clone(),
            owner: owner.clone(),
            reason: reason.clone(),
            restored_paths,
            audit_id: audit_id.clone(),
            reverted_at: now,
        };
        self.runtime_reverts.push(revert.clone());
        self.applied_change_rollbacks.remove(&applied_change_id);
        let gate = &mut self.runtime_merge_gates[gate_index];
        gate.status = viden_types::MergeGateStatus::Reverted;
        gate.decision = Some(MergeGateDecision::decided_now(
            MergeGateDecisionOutcome::Reverted,
            reason.clone(),
            owner,
            gate.evidence_ids.clone(),
            audit_id.clone(),
        ));
        gate.audit_ids.push(audit_id);
        gate.updated_at = Some(now);
        let gate = gate.clone();

        let mut events = vec![
            RuntimeEvent::new(1, RuntimeEventKind::RevertRecorded { revert }),
            RuntimeEvent::new(2, RuntimeEventKind::MergeGateUpdated { gate }),
        ];
        if let Some(mut task) = self
            .runtime_tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
        {
            task.status = AgentTaskStatus::NeedsInput;
            task.activity = format!("applied change reverted: {reason}");
            task.next_action = Some(AgentNextAction {
                label: "revise reverted change".to_string(),
                command: Some(format!("/agent start {task_id}")),
                reason: Some("the applied change was reverted by an audited command".to_string()),
            });
            task.updated_at = Some(now.saturating_mul(1000));
            self.upsert_agent_task(task.clone());
            events.push(RuntimeEvent::new(3, RuntimeEventKind::TaskUpdated { task }));
        }
        Ok(events)
    }

    /// Persists one trust audit fact, fail-closed.
    ///
    /// A failed append fails the whole trust action. Audit persistence is part
    /// of the mutation's contract, the same way permission is checked before
    /// mutation: publishing a trust event whose audit record never landed
    /// would leave an unauditable change in the timeline.
    pub(crate) fn append_trust_audit(&self, record: &AuditRecord) -> Result<(), String> {
        if self.should_fail_workflow_append_for_test() {
            return Err("injected workflow append failure".to_string());
        }
        self.workflows.append_audit_record(record)
    }

    pub(crate) fn require_trust_permission<F>(
        &mut self,
        action: &str,
        preview: &str,
        approver: &mut F,
    ) -> Result<(), String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        self.require_trust_permission_with_context(action, preview, None, approver)
    }

    /// The same trust-loop gate, carrying the multi-file change the operator
    /// is approving (`runtime.structured_diff`, GUI-CORE-012).
    pub(crate) fn require_trust_permission_with_context<F>(
        &mut self,
        action: &str,
        preview: &str,
        decision_context: Option<viden_types::DecisionContext>,
        approver: &mut F,
    ) -> Result<(), String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        if let Some(denial) = self.ensure_workflow_permission_with_context(
            action,
            preview,
            decision_context,
            approver,
        )? {
            return Err(denial);
        }
        Ok(())
    }

    fn require_runtime_task(&self, task_id: &str) -> Result<(), String> {
        if self.runtime_tasks.iter().any(|task| task.id == task_id) {
            Ok(())
        } else {
            Err(format!("agent task `{task_id}` does not exist"))
        }
    }

    pub(crate) fn require_merge_gate_index(&self, gate_id: &str) -> Result<usize, String> {
        self.runtime_merge_gates
            .iter()
            .position(|gate| gate.gate_id == gate_id)
            .ok_or_else(|| format!("merge gate `{gate_id}` does not exist"))
    }
}

fn validate_owner(
    owner: &RuntimeOwner,
    lane_id: Option<&str>,
    task_id: &str,
) -> Result<(), String> {
    validate_trust_id("owner.workspace_id", &owner.workspace_id)
        .map_err(|_| "trust-loop owner requires valid workspace identity".to_string())?;
    validate_trust_id("owner.project_id", &owner.project_id)
        .map_err(|_| "trust-loop owner requires valid project identity".to_string())?;
    for (name, value) in [
        ("owner.lane_id", owner.lane_id.as_deref()),
        ("owner.session_id", owner.session_id.as_deref()),
        ("owner.task_id", owner.task_id.as_deref()),
        ("owner.turn_id", owner.turn_id.as_deref()),
    ] {
        if let Some(value) = value {
            validate_trust_id(name, value)?;
        }
    }
    if owner.task_id.as_deref() != Some(task_id) {
        return Err("trust-loop owner task identity mismatch".to_string());
    }
    if let Some(lane_id) = lane_id
        && owner.lane_id.as_deref() != Some(lane_id)
    {
        return Err("trust-loop owner lane identity mismatch".to_string());
    }
    Ok(())
}

fn validate_review_requester(
    gate: &viden_types::MergeGateRecord,
    requester_lane_id: &str,
    owner: &RuntimeOwner,
) -> Result<(), String> {
    if let Some(owner_lane_id) = gate.owner.lane_id.as_deref()
        && owner_lane_id != requester_lane_id
    {
        return Err("review requester does not own the merge gate".to_string());
    }
    if &gate.owner != owner {
        return Err(
            "review request owner must match the complete merge gate owner scope".to_string(),
        );
    }
    Ok(())
}

/// Only the independent reviewer lane may settle a review.
///
/// The requester owns the gate and the evidence, so letting it record its own
/// verdict would collapse the two-lane trust loop into self-approval.
fn validate_review_decider(
    review: &ReviewRequestRecord,
    actor: &RuntimeOwner,
) -> Result<(), String> {
    if actor == &RuntimeOwner::default() {
        return Err("review decision requires an actor".to_string());
    }
    if actor.lane_id.as_deref() != Some(review.reviewer_lane_id.as_str()) {
        return Err("review decision requires the independent reviewer lane".to_string());
    }
    validate_owner(actor, Some(&review.reviewer_lane_id), &review.task_id)?;
    if actor.workspace_id != review.owner.workspace_id
        || actor.project_id != review.owner.project_id
    {
        return Err("review decision actor does not match the review workspace scope".to_string());
    }
    Ok(())
}

fn reviewer_owner_from_requester(requester: &RuntimeOwner, reviewer_lane_id: &str) -> RuntimeOwner {
    let mut reviewer = requester.clone();
    reviewer.lane_id = Some(reviewer_lane_id.to_string());
    // The request command is authorized by the gate owner/requester; the
    // validator owner stores the independent reviewer lane and intentionally
    // leaves session identity unclaimed until that reviewer accepts.
    reviewer.session_id = None;
    reviewer.turn_id = None;
    reviewer
}

fn validate_trust_id(name: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value.is_ascii()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
        || value.contains("..")
        || value.starts_with(['.', ':', '-', '_'])
        || value.ends_with(['.', ':', '-', '_'])
    {
        return Err(format!("invalid trust-loop {name}"));
    }
    Ok(())
}

fn validate_trust_text(name: &str, value: String, max_chars: usize) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(format!(
            "{name} cannot be empty or contain control characters"
        ));
    }
    Ok(truncate_for_preview(value, max_chars))
}

fn ensure_unique(exists: bool, kind: &str, id: &str) -> Result<(), String> {
    if exists {
        Err(format!("{kind} `{id}` already exists"))
    } else {
        Ok(())
    }
}

fn dependency_path_exists(
    edges: &std::collections::BTreeMap<String, Vec<String>>,
    start: &str,
    target: &str,
) -> bool {
    let mut pending = vec![start];
    let mut visited = std::collections::BTreeSet::new();
    while let Some(node) = pending.pop() {
        if node == target {
            return true;
        }
        if !visited.insert(node) {
            continue;
        }
        if let Some(dependencies) = edges.get(node) {
            pending.extend(dependencies.iter().map(String::as_str));
        }
    }
    false
}

fn upsert_dependency(records: &mut Vec<DependencyRecord>, record: DependencyRecord) {
    if let Some(existing) = records
        .iter_mut()
        .find(|existing| existing.dependency_id == record.dependency_id)
    {
        *existing = record;
    } else {
        records.push(record);
    }
}

fn upsert_conflict(records: &mut Vec<ConflictBounce>, record: ConflictBounce) {
    if let Some(existing) = records
        .iter_mut()
        .find(|existing| existing.bounce_id == record.bounce_id)
    {
        *existing = record;
    } else {
        records.push(record);
    }
}
