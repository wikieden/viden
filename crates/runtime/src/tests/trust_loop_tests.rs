use std::fs;
use std::time::{Duration, Instant};

use viden_context::{ContextEngine, ContextPutRequest};
use viden_types::{
    AgentDagTaskSpec, AgentRole, ApprovalResponse, CanonicalEvidenceReference, ConflictBaseline,
    ConflictBounceStatus, ConflictHunkReason, ContextContentKind, ContextScope, ContractDecision,
    DependencyState, EvidenceProducer, EvidenceQualityFacts, EvidenceQualityStatus,
    EvidenceVerificationState, HandoffAcceptance, MergeGateDecisionOutcome, MergeGateStatus,
    ReviewedEvidenceBinding, RuntimeCommand, RuntimeEvent, RuntimeEventKind, RuntimeOwner,
    RuntimeViewState,
};
use viden_workflows::stores::WorkflowStore;

use super::{SequenceProvider, temp_dir};
use crate::{RuntimeSupervisor, SessionEngine};

fn owner(lane_id: &str, task_id: &str) -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace-1".to_string(),
        project_id: "project-1".to_string(),
        lane_id: Some(lane_id.to_string()),
        session_id: Some(format!("session-{lane_id}")),
        task_id: Some(task_id.to_string()),
        turn_id: None,
    }
}

fn start_gate(
    engine: &mut SessionEngine,
    approver: &mut impl FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    task_id: &str,
    required_evidence: Vec<String>,
) -> Vec<RuntimeEvent> {
    engine
        .handle_runtime_command(
            format!("start-{task_id}"),
            RuntimeCommand::StartAgentDag {
                goal: format!("trust loop for {task_id}"),
                tasks: vec![AgentDagTaskSpec {
                    task_id: task_id.to_string(),
                    role: AgentRole::Coder,
                    title: "Trust-loop task".to_string(),
                    objective: "produce canonical evidence".to_string(),
                    dependencies: Vec::new(),
                    workspace: None,
                    file_scope: vec!["src".to_string()],
                    context_bundle_id: None,
                    required_evidence,
                    permission_policy: "scoped_mutation".to_string(),
                }],
            },
            approver,
        )
        .unwrap()
}

fn start_dependency_tasks(
    engine: &mut SessionEngine,
    approver: &mut impl FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
) {
    engine
        .handle_runtime_command(
            "start-dependency-tasks",
            RuntimeCommand::StartAgentDag {
                goal: "dynamic dependency aggregation".to_string(),
                tasks: ["task-a", "task-b", "task-c"]
                    .into_iter()
                    .map(|task_id| AgentDagTaskSpec {
                        task_id: task_id.to_string(),
                        role: AgentRole::Coder,
                        title: task_id.to_string(),
                        objective: format!("execute {task_id}"),
                        dependencies: Vec::new(),
                        workspace: None,
                        file_scope: vec!["src".to_string()],
                        context_bundle_id: None,
                        required_evidence: vec!["patch".to_string()],
                        permission_policy: "scoped_mutation".to_string(),
                    })
                    .collect(),
            },
            approver,
        )
        .unwrap();
}

#[test]
fn trust_loop_cross_lane_records_preserve_owner_and_dependency_state_through_replay() {
    let cwd = temp_dir("trust_loop_cross_lane_records_cwd");
    let home = temp_dir("trust_loop_cross_lane_records_home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home.clone()),
    )
    .unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let task_id = "task-cross-lane";
    let mut events = start_gate(
        &mut engine,
        &mut approver,
        task_id,
        vec!["patch".to_string()],
    );
    events.extend(start_gate(
        &mut engine,
        &mut approver,
        "task-contract",
        vec!["review".to_string()],
    ));

    let handoff_events = engine
        .handle_runtime_command(
            "handoff",
            RuntimeCommand::CreateHandoff {
                handoff_id: "handoff-1".to_string(),
                task_id: task_id.to_string(),
                from_lane_id: "lane-planner".to_string(),
                to_lane_id: "lane-coder".to_string(),
                owner: owner("lane-coder", task_id),
                summary: "planner hands accepted scope to coder".to_string(),
                acceptance: HandoffAcceptance::Accepted,
            },
            &mut approver,
        )
        .unwrap();
    assert!(handoff_events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MergeGateUpdated { gate }
            if gate.owner == owner("lane-coder", task_id)
    )));
    events.extend(handoff_events);
    let rejected_handoff = engine
        .handle_runtime_command(
            "handoff-rejected",
            RuntimeCommand::CreateHandoff {
                handoff_id: "handoff-rejected".to_string(),
                task_id: task_id.to_string(),
                from_lane_id: "lane-coder".to_string(),
                to_lane_id: "lane-docs".to_string(),
                owner: owner("lane-docs", task_id),
                summary: "docs lane rejected incomplete scope".to_string(),
                acceptance: HandoffAcceptance::Rejected,
            },
            &mut approver,
        )
        .unwrap();
    assert!(rejected_handoff.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::HandoffUpdated { handoff }
            if handoff.acceptance == HandoffAcceptance::Rejected
    )));
    assert!(rejected_handoff.iter().all(|event| !matches!(
        &event.kind,
        RuntimeEventKind::MergeGateUpdated { gate }
            if gate.owner == owner("lane-docs", task_id)
    )));
    events.extend(rejected_handoff);
    let review_binding = record_canonical_patch(
        &cwd,
        &mut engine,
        &mut approver,
        task_id,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );

    for (command_id, command) in [
        (
            "review",
            RuntimeCommand::RequestReview {
                review_id: "review-1".to_string(),
                gate_id: format!("gate-{task_id}"),
                requester_lane_id: "lane-coder".to_string(),
                reviewer_lane_id: "lane-reviewer".to_string(),
                owner: owner("lane-coder", task_id),
                evidence_ids: vec![review_binding.evidence_id],
            },
        ),
        (
            "contract",
            RuntimeCommand::ConfirmContract {
                contract_id: "contract-1".to_string(),
                task_id: task_id.to_string(),
                owner: owner("lane-reviewer", task_id),
                summary: "src scope and canonical patch evidence".to_string(),
                decision: ContractDecision::Confirmed,
            },
        ),
        (
            "block",
            RuntimeCommand::SetDependency {
                dependency_id: "dependency-1".to_string(),
                task_id: task_id.to_string(),
                depends_on_task_id: "task-contract".to_string(),
                owner: owner("lane-coder", task_id),
                state: DependencyState::Blocked,
                reason: "contract confirmation pending".to_string(),
            },
        ),
        (
            "unblock",
            RuntimeCommand::SetDependency {
                dependency_id: "dependency-1".to_string(),
                task_id: task_id.to_string(),
                depends_on_task_id: "task-contract".to_string(),
                owner: owner("lane-coder", task_id),
                state: DependencyState::Unblocked,
                reason: "contract confirmed".to_string(),
            },
        ),
    ] {
        events.extend(
            engine
                .handle_runtime_command(command_id, command, &mut approver)
                .unwrap(),
        );
    }

    let live = engine.runtime_view_state();
    assert_eq!(live.handoffs.len(), 2);
    assert_eq!(live.handoffs[0].acceptance, HandoffAcceptance::Accepted);
    assert_eq!(live.handoffs[0].owner, owner("lane-coder", task_id));
    assert_eq!(live.review_requests.len(), 1);
    assert_eq!(live.review_requests[0].reviewer_lane_id, "lane-reviewer");
    assert_eq!(live.contracts[0].decision, ContractDecision::Confirmed);
    assert_eq!(live.dependencies[0].state, DependencyState::Unblocked);
    assert!(live.handoffs[0].audit_id.starts_with("audit"));
    assert!(live.review_requests[0].audit_id.starts_with("audit"));

    let mut replayed = RuntimeViewState::new(live.snapshot.clone());
    for event in &events {
        replayed.apply_event(event);
    }
    assert_eq!(replayed.handoffs, live.handoffs);
    assert_eq!(replayed.review_requests, live.review_requests);
    assert_eq!(replayed.contracts, live.contracts);
    assert_eq!(replayed.dependencies, live.dependencies);
    assert_eq!(replayed.merge_gates, live.merge_gates);
}

#[test]
fn trust_loop_dynamic_dependencies_reject_invalid_edges_before_permission() {
    let cwd = temp_dir("trust_loop_invalid_dependency_cwd");
    let home = temp_dir("trust_loop_invalid_dependency_home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    start_dependency_tasks(&mut engine, &mut allow);

    for (command_id, target, expected) in [
        ("missing-dependency", "task-missing", "does not exist"),
        ("self-dependency", "task-a", "cannot depend on itself"),
    ] {
        let mut approval_calls = 0;
        let events = engine
            .handle_runtime_command(
                command_id,
                RuntimeCommand::SetDependency {
                    dependency_id: command_id.to_string(),
                    task_id: "task-a".to_string(),
                    depends_on_task_id: target.to_string(),
                    owner: owner("lane-a", "task-a"),
                    state: DependencyState::Blocked,
                    reason: "invalid edge".to_string(),
                },
                &mut |_prompt| {
                    approval_calls += 1;
                    ApprovalResponse::allow_once(None)
                },
            )
            .unwrap();
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::CommandRejected { reason, .. } if reason.contains(expected)
        )));
        assert_eq!(approval_calls, 0);
    }

    engine
        .handle_runtime_command(
            "dependency-b-a",
            RuntimeCommand::SetDependency {
                dependency_id: "dependency-b-a".to_string(),
                task_id: "task-b".to_string(),
                depends_on_task_id: "task-a".to_string(),
                owner: owner("lane-b", "task-b"),
                state: DependencyState::Blocked,
                reason: "B waits for A".to_string(),
            },
            &mut allow,
        )
        .unwrap();
    let cycle = engine
        .handle_runtime_command(
            "dependency-a-b",
            RuntimeCommand::SetDependency {
                dependency_id: "dependency-a-b".to_string(),
                task_id: "task-a".to_string(),
                depends_on_task_id: "task-b".to_string(),
                owner: owner("lane-a", "task-a"),
                state: DependencyState::Blocked,
                reason: "A cannot wait for B".to_string(),
            },
            &mut allow,
        )
        .unwrap();
    assert!(cycle.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("dependency cycle")
    )));
}

#[test]
fn trust_loop_dynamic_dependencies_block_start_and_unblock_in_aggregate() {
    let cwd = temp_dir("trust_loop_dependency_aggregate_cwd");
    let home = temp_dir("trust_loop_dependency_aggregate_home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    start_dependency_tasks(&mut engine, &mut allow);
    for (id, target) in [("dependency-a-b", "task-b"), ("dependency-a-c", "task-c")] {
        engine
            .handle_runtime_command(
                format!("block-{id}"),
                RuntimeCommand::SetDependency {
                    dependency_id: id.to_string(),
                    task_id: "task-a".to_string(),
                    depends_on_task_id: target.to_string(),
                    owner: owner("lane-a", "task-a"),
                    state: DependencyState::Blocked,
                    reason: format!("wait for {target}"),
                },
                &mut allow,
            )
            .unwrap();
    }

    engine
        .handle_runtime_command(
            "start-blocked-a",
            RuntimeCommand::StartAgentTask {
                task_id: "task-a".to_string(),
            },
            &mut allow,
        )
        .unwrap();
    assert_eq!(
        engine
            .runtime_view_state()
            .tasks
            .iter()
            .find(|task| task.id == "task-a")
            .unwrap()
            .status,
        viden_types::AgentTaskStatus::Blocked
    );

    for (index, (id, target)) in [("dependency-a-b", "task-b"), ("dependency-a-c", "task-c")]
        .into_iter()
        .enumerate()
    {
        engine
            .handle_runtime_command(
                format!("unblock-{id}"),
                RuntimeCommand::SetDependency {
                    dependency_id: id.to_string(),
                    task_id: "task-a".to_string(),
                    depends_on_task_id: target.to_string(),
                    owner: owner("lane-a", "task-a"),
                    state: DependencyState::Unblocked,
                    reason: format!("{target} ready"),
                },
                &mut allow,
            )
            .unwrap();
        let status = engine
            .runtime_view_state()
            .tasks
            .iter()
            .find(|task| task.id == "task-a")
            .unwrap()
            .status;
        assert_eq!(
            status,
            if index == 0 {
                viden_types::AgentTaskStatus::Blocked
            } else {
                viden_types::AgentTaskStatus::Queued
            }
        );
    }
}

#[test]
fn trust_loop_summary_only_evidence_never_produces_typed_acceptance() {
    let cwd = temp_dir("trust_loop_summary_only_cwd");
    let home = temp_dir("trust_loop_summary_only_home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let task_id = "task-summary-only";
    start_gate(
        &mut engine,
        &mut approver,
        task_id,
        vec!["patch".to_string()],
    );

    let recorded = engine
        .handle_runtime_command(
            "summary-only",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: format!("gate-{task_id}"),
                evidence_id: Some("summary-only-patch".to_string()),
                kind: "patch".to_string(),
                summary: "diff looked good in chat".to_string(),
                path: None,
                source: Some("summary".to_string()),
                canonical: None,
            },
            &mut approver,
        )
        .unwrap();
    assert!(recorded.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MergeGateUpdated { gate }
            if gate.status == MergeGateStatus::CollectingEvidence
                && gate.decision.as_ref().is_some_and(|decision| {
                    decision.outcome == MergeGateDecisionOutcome::AwaitingEvidence
                })
    )));

    let accepted = engine
        .handle_runtime_command(
            "accept-summary-only",
            RuntimeCommand::AcceptMergeGate {
                gate_id: format!("gate-{task_id}"),
                actor: RuntimeOwner::default(),
                reviewed_evidence: Vec::new(),
                decision: Some("summary is sufficient".to_string()),
            },
            &mut approver,
        )
        .unwrap();
    assert!(accepted.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("missing_canonical")
    )));
    assert_ne!(
        engine.runtime_view_state().merge_gates[0]
            .decision
            .as_ref()
            .map(|decision| decision.outcome),
        Some(MergeGateDecisionOutcome::Accepted)
    );
}

#[test]
fn trust_loop_canonical_artifact_uses_core_permission_receipt_not_claimed_status() {
    let cwd = temp_dir("trust_loop_core_receipt_cwd");
    let home = temp_dir("trust_loop_core_receipt_home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    let task_id = "task-core-receipt";
    start_gate(&mut engine, &mut allow, task_id, vec!["patch".to_string()]);
    record_canonical_patch(
        &cwd,
        &mut engine,
        &mut allow,
        task_id,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );

    let canonical = engine.runtime_view_state().latest_evidence[0]
        .canonical
        .clone()
        .expect("artifact bytes should receive a canonical reference");
    assert!(
        canonical
            .permission_snapshot_id
            .as_deref()
            .is_some_and(|id| id.starts_with("permission-receipt"))
    );
    assert_eq!(canonical.verification, EvidenceVerificationState::Verified);
    assert_eq!(canonical.quality.status, EvidenceQualityStatus::Pass);
}

#[test]
fn trust_loop_merge_rejects_stale_canonical_bytes_before_permission() {
    let cwd = temp_dir("trust_loop_merge_preflight_cwd");
    let home = temp_dir("trust_loop_merge_preflight_home");
    fs::create_dir_all(cwd.join("src")).unwrap();
    fs::write(cwd.join("src/lib.rs"), "old\n").unwrap();
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    let task_id = "task-merge-preflight";
    start_gate(&mut engine, &mut allow, task_id, vec!["patch".to_string()]);
    let binding = record_canonical_patch(
        &cwd,
        &mut engine,
        &mut allow,
        task_id,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );
    let merge_actor = engine.runtime_view_state().merge_gates[0].owner.clone();
    engine
        .handle_runtime_command(
            "accept-merge-preflight",
            RuntimeCommand::AcceptMergeGate {
                gate_id: format!("gate-{task_id}"),
                actor: merge_actor.clone(),
                reviewed_evidence: vec![binding.clone()],
                decision: Some("accept exact patch".to_string()),
            },
            &mut allow,
        )
        .unwrap();

    let blob_path = cwd
        .join(".viden/context-engine/blobs")
        .join(&binding.source_hash[..2])
        .join(&binding.source_hash);
    fs::write(blob_path, b"tampered").unwrap();
    let mut approval_calls = 0;
    let events = engine
        .handle_runtime_command(
            "merge-stale-canonical",
            RuntimeCommand::MergeAgentPatch {
                gate_id: format!("gate-{task_id}"),
                actor: merge_actor,
                decision: Some("must not merge stale bytes".to_string()),
            },
            &mut |_prompt| {
                approval_calls += 1;
                ApprovalResponse::allow_once(None)
            },
        )
        .unwrap();

    assert!(
        events.iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::CommandRejected { reason, .. }
                if reason.contains("canonical") || reason.contains("hash")
        )),
        "unexpected events: {events:#?}"
    );
    assert_eq!(approval_calls, 0);
    assert_eq!(
        engine.runtime_view_state().merge_gates[0].status,
        MergeGateStatus::Accepted
    );
    assert_eq!(fs::read_to_string(cwd.join("src/lib.rs")).unwrap(), "old\n");
}

#[test]
fn recovery_snapshot_readers_use_open_file_handle_source_guard() {
    let source = include_str!("../runtime_contract.rs");
    let helper_start = source
        .find("fn read_existing_private_file(")
        .expect("recovery reader helper exists");
    let helper_end = source[helper_start..]
        .find("fn evidence_by_id(")
        .map(|offset| helper_start + offset)
        .expect("helper guard end exists");
    let helper = &source[helper_start..helper_end];
    assert!(helper.contains("read_existing_private_file_openat"));
    assert!(helper.contains("libc::openat"));
    assert!(helper.contains("libc::O_NOFOLLOW"));
    assert!(helper.contains(".metadata()"));
    assert!(helper.contains(".read_to_end("));
    assert!(
        !helper.contains("fs::read("),
        "recovery/postimage readers must not re-open by path after metadata checks"
    );

    let stage_start = source
        .find("pub(crate) fn stage_rollback_paths")
        .expect("stage rollback function exists");
    let stage_end = source[stage_start..]
        .find("fn restore_transaction_files")
        .map(|offset| stage_start + offset)
        .expect("stage guard end exists");
    let stage = &source[stage_start..stage_end];
    assert!(stage.contains("read_existing_private_file(&root, path)"));
    assert!(
        !stage.contains("fs::read("),
        "rollback staging must capture bytes through the held file descriptor"
    );
}

#[test]
fn trust_loop_restart_revert_uses_durable_recovery_snapshot() {
    let cwd = temp_dir("trust_loop_restart_recovery_cwd");
    let home = temp_dir("trust_loop_restart_recovery_home");
    fs::create_dir_all(cwd.join("src")).unwrap();
    let private_preimage = "private-preimage-42\n";
    fs::write(cwd.join("src/lib.rs"), private_preimage).unwrap();
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home.clone()),
    )
    .unwrap();
    let session_id = engine.session_id().to_string();
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    let task_id = "task-restart-recovery";
    start_gate(&mut engine, &mut allow, task_id, vec!["patch".to_string()]);
    let binding = record_canonical_patch(
        &cwd,
        &mut engine,
        &mut allow,
        task_id,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-private-preimage-42\n+new\n",
    );
    let actor = engine.runtime_view_state().merge_gates[0].owner.clone();
    engine
        .handle_runtime_command(
            "accept-restart-recovery",
            RuntimeCommand::AcceptMergeGate {
                gate_id: format!("gate-{task_id}"),
                actor: actor.clone(),
                reviewed_evidence: vec![binding],
                decision: Some("accept durable patch".to_string()),
            },
            &mut allow,
        )
        .unwrap();
    engine
        .handle_runtime_command(
            "merge-restart-recovery",
            RuntimeCommand::MergeAgentPatch {
                gate_id: format!("gate-{task_id}"),
                actor: actor.clone(),
                decision: Some("merge with durable rollback".to_string()),
            },
            &mut allow,
        )
        .unwrap();
    let merged_gate = engine.runtime_view_state().merge_gates[0].clone();
    assert_eq!(merged_gate.status, MergeGateStatus::Merged);
    assert!(merged_gate.recovery_snapshot.is_some());
    assert_eq!(fs::read_to_string(cwd.join("src/lib.rs")).unwrap(), "new\n");
    let workflow_log = fs::read_to_string(
        WorkflowStore::new(&home, &cwd)
            .unwrap()
            .paths()
            .agent_log
            .clone(),
    )
    .unwrap();
    assert!(!workflow_log.contains(private_preimage));
    drop(engine);

    let mut resumed = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    resumed
        .process_input_with_approval(&format!("/resume {session_id}"), &mut allow)
        .unwrap();
    let reverted = resumed
        .handle_runtime_command(
            "revert-after-restart",
            RuntimeCommand::RevertAppliedChange {
                gate_id: format!("gate-{task_id}"),
                owner: owner("lane-origin", task_id),
                reason: "restart verification failed".to_string(),
            },
            &mut allow,
        )
        .unwrap();
    assert!(
        reverted.iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::MergeGateUpdated { gate }
                if gate.status == MergeGateStatus::Reverted
        )),
        "unexpected revert events: {reverted:#?}"
    );
    assert_eq!(
        fs::read_to_string(cwd.join("src/lib.rs")).unwrap(),
        private_preimage
    );
}

#[test]
fn trust_loop_request_review_requires_gate_owner_as_requester() {
    let cwd = temp_dir("trust_loop_request_review_owner_cwd");
    let home = temp_dir("trust_loop_request_review_owner_home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    let task_id = "task-review-owner";
    start_gate(&mut engine, &mut allow, task_id, vec!["patch".to_string()]);
    engine
        .handle_runtime_command(
            "handoff-review-owner",
            RuntimeCommand::CreateHandoff {
                handoff_id: "handoff-review-owner".to_string(),
                task_id: task_id.to_string(),
                from_lane_id: "lane-planner".to_string(),
                to_lane_id: "lane-origin".to_string(),
                owner: owner("lane-origin", task_id),
                summary: "origin owns request authority".to_string(),
                acceptance: HandoffAcceptance::Accepted,
            },
            &mut allow,
        )
        .unwrap();
    let binding = record_canonical_patch(
        &cwd,
        &mut engine,
        &mut allow,
        task_id,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );

    let mut approval_calls = 0;
    let reviewer_self_request = engine
        .handle_runtime_command(
            "reviewer-self-request",
            RuntimeCommand::RequestReview {
                review_id: "review-owner-bad".to_string(),
                gate_id: format!("gate-{task_id}"),
                requester_lane_id: "lane-origin".to_string(),
                reviewer_lane_id: "lane-reviewer".to_string(),
                owner: owner("lane-reviewer", task_id),
                evidence_ids: vec![binding.evidence_id.clone()],
            },
            &mut |_prompt| {
                approval_calls += 1;
                ApprovalResponse::allow_once(None)
            },
        )
        .unwrap();
    assert!(reviewer_self_request.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("request owner")
    )));
    assert_eq!(approval_calls, 0);
    assert!(engine.runtime_view_state().review_requests.is_empty());

    let accepted_request = engine
        .handle_runtime_command(
            "origin-review-request",
            RuntimeCommand::RequestReview {
                review_id: "review-owner-good".to_string(),
                gate_id: format!("gate-{task_id}"),
                requester_lane_id: "lane-origin".to_string(),
                reviewer_lane_id: "lane-reviewer".to_string(),
                owner: owner("lane-origin", task_id),
                evidence_ids: vec![binding.evidence_id],
            },
            &mut allow,
        )
        .unwrap();
    assert!(accepted_request.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::ReviewRequestUpdated { review }
            if review.owner == owner("lane-origin", task_id)
                && review.reviewer_lane_id == "lane-reviewer"
    )));
    let gate = &engine.runtime_view_state().merge_gates[0];
    assert_eq!(
        gate.validator
            .as_ref()
            .and_then(|validator| validator.owner.lane_id.as_deref()),
        Some("lane-reviewer")
    );
}

#[test]
fn trust_loop_request_review_rejects_cross_workspace_same_lane_owner() {
    let cwd = temp_dir("trust_loop_request_review_cross_workspace_cwd");
    let home = temp_dir("trust_loop_request_review_cross_workspace_home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    let task_id = "task-review-scope";
    start_gate(&mut engine, &mut allow, task_id, vec!["patch".to_string()]);
    engine
        .handle_runtime_command(
            "handoff-review-scope",
            RuntimeCommand::CreateHandoff {
                handoff_id: "handoff-review-scope".to_string(),
                task_id: task_id.to_string(),
                from_lane_id: "lane-planner".to_string(),
                to_lane_id: "lane-origin".to_string(),
                owner: owner("lane-origin", task_id),
                summary: "origin owns full scoped review request".to_string(),
                acceptance: HandoffAcceptance::Accepted,
            },
            &mut allow,
        )
        .unwrap();
    let binding = record_canonical_patch(
        &cwd,
        &mut engine,
        &mut allow,
        task_id,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );
    let mut cross_workspace_owner = owner("lane-origin", task_id);
    cross_workspace_owner.workspace_id = "workspace-2".to_string();

    let mut approval_calls = 0;
    let rejected = engine
        .handle_runtime_command(
            "cross-workspace-review-request",
            RuntimeCommand::RequestReview {
                review_id: "review-cross-workspace".to_string(),
                gate_id: format!("gate-{task_id}"),
                requester_lane_id: "lane-origin".to_string(),
                reviewer_lane_id: "lane-reviewer".to_string(),
                owner: cross_workspace_owner,
                evidence_ids: vec![binding.evidence_id],
            },
            &mut |_prompt| {
                approval_calls += 1;
                ApprovalResponse::allow_once(None)
            },
        )
        .unwrap();

    assert!(rejected.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("complete merge gate owner scope")
    )));
    assert_eq!(approval_calls, 0);
    assert!(engine.runtime_view_state().review_requests.is_empty());
}

#[test]
fn trust_loop_reject_gate_requires_authorized_actor_and_records_actor() {
    let cwd = temp_dir("trust_loop_reject_gate_actor_cwd");
    let home = temp_dir("trust_loop_reject_gate_actor_home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    let task_id = "task-reject-gate-actor";
    start_gate(&mut engine, &mut allow, task_id, vec!["patch".to_string()]);
    engine
        .handle_runtime_command(
            "handoff-reject-gate-actor",
            RuntimeCommand::CreateHandoff {
                handoff_id: "handoff-reject-gate-actor".to_string(),
                task_id: task_id.to_string(),
                from_lane_id: "lane-planner".to_string(),
                to_lane_id: "lane-origin".to_string(),
                owner: owner("lane-origin", task_id),
                summary: "origin owns rejection authority".to_string(),
                acceptance: HandoffAcceptance::Accepted,
            },
            &mut allow,
        )
        .unwrap();

    let mut approval_calls = 0;
    let attacker = engine
        .handle_runtime_command(
            "reject-gate-attacker",
            RuntimeCommand::RejectMergeGate {
                gate_id: format!("gate-{task_id}"),
                actor: owner("lane-attacker", task_id),
                reason: "attacker cannot reject".to_string(),
            },
            &mut |_prompt| {
                approval_calls += 1;
                ApprovalResponse::allow_once(None)
            },
        )
        .unwrap();
    assert!(attacker.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("not authorized")
    )));
    assert_eq!(approval_calls, 0);

    let accepted = engine
        .handle_runtime_command(
            "reject-gate-origin",
            RuntimeCommand::RejectMergeGate {
                gate_id: format!("gate-{task_id}"),
                actor: owner("lane-origin", task_id),
                reason: "needs focused verification".to_string(),
            },
            &mut allow,
        )
        .unwrap();
    assert!(accepted.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MergeGateUpdated { gate }
            if gate.status == MergeGateStatus::NeedsChanges
                && gate.decision.as_ref().is_some_and(|decision| {
                    decision.outcome == MergeGateDecisionOutcome::Rejected
                        && decision.owner == owner("lane-origin", task_id)
                })
    )));
}

#[test]
fn trust_loop_dependency_id_cannot_be_rebound_to_different_endpoints() {
    let cwd = temp_dir("trust_loop_dependency_rebind_cwd");
    let home = temp_dir("trust_loop_dependency_rebind_home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    start_dependency_tasks(&mut engine, &mut allow);
    engine
        .handle_runtime_command(
            "block-a-on-b",
            RuntimeCommand::SetDependency {
                dependency_id: "dependency-fixed-edge".to_string(),
                task_id: "task-a".to_string(),
                depends_on_task_id: "task-b".to_string(),
                owner: owner("lane-a", "task-a"),
                state: DependencyState::Blocked,
                reason: "task b must finish first".to_string(),
            },
            &mut allow,
        )
        .unwrap();

    let mut approval_calls = 0;
    let rebound = engine
        .handle_runtime_command(
            "rebind-a-on-c",
            RuntimeCommand::SetDependency {
                dependency_id: "dependency-fixed-edge".to_string(),
                task_id: "task-a".to_string(),
                depends_on_task_id: "task-c".to_string(),
                owner: owner("lane-a", "task-a"),
                state: DependencyState::Blocked,
                reason: "same id cannot point at c".to_string(),
            },
            &mut |_prompt| {
                approval_calls += 1;
                ApprovalResponse::allow_once(None)
            },
        )
        .unwrap();
    assert!(rebound.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("dependency id") && reason.contains("endpoints")
    )));
    assert_eq!(approval_calls, 0);
    let view = engine.runtime_view_state();
    assert_eq!(view.dependencies.len(), 1);
    assert_eq!(view.dependencies[0].depends_on_task_id, "task-b");

    let mut approval_calls = 0;
    let unblocked_rebound = engine
        .handle_runtime_command(
            "unblock-rebind-a-on-c",
            RuntimeCommand::SetDependency {
                dependency_id: "dependency-fixed-edge".to_string(),
                task_id: "task-a".to_string(),
                depends_on_task_id: "task-c".to_string(),
                owner: owner("lane-a", "task-a"),
                state: DependencyState::Unblocked,
                reason: "same id cannot unblock a different edge".to_string(),
            },
            &mut |_prompt| {
                approval_calls += 1;
                ApprovalResponse::allow_once(None)
            },
        )
        .unwrap();
    assert!(unblocked_rebound.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("dependency id") && reason.contains("endpoints")
    )));
    assert_eq!(approval_calls, 0);
    let view = engine.runtime_view_state();
    assert_eq!(view.dependencies.len(), 1);
    assert_eq!(view.dependencies[0].depends_on_task_id, "task-b");
}

#[test]
fn trust_loop_bounce_requires_gate_owner_and_valid_canonical_baseline() {
    let cwd = temp_dir("trust_loop_bounce_preflight_cwd");
    let home = temp_dir("trust_loop_bounce_preflight_home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    let task_id = "task-bounce-preflight";
    start_gate(&mut engine, &mut allow, task_id, vec!["patch".to_string()]);
    engine
        .handle_runtime_command(
            "handoff-bounce-preflight",
            RuntimeCommand::CreateHandoff {
                handoff_id: "handoff-bounce-preflight".to_string(),
                task_id: task_id.to_string(),
                from_lane_id: "lane-planner".to_string(),
                to_lane_id: "lane-origin".to_string(),
                owner: owner("lane-origin", task_id),
                summary: "origin owns bounce".to_string(),
                acceptance: HandoffAcceptance::Accepted,
            },
            &mut allow,
        )
        .unwrap();
    let binding = record_canonical_patch(
        &cwd,
        &mut engine,
        &mut allow,
        task_id,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );

    let mut approval_calls = 0;
    let wrong_origin = engine
        .handle_runtime_command(
            "wrong-origin-bounce",
            RuntimeCommand::BounceMergeConflict {
                gate_id: format!("gate-{task_id}"),
                original_lane_id: "lane-intruder".to_string(),
                owner: owner("lane-origin", task_id),
                reason: "wrong origin must fail".to_string(),
            },
            &mut |_prompt| {
                approval_calls += 1;
                ApprovalResponse::allow_once(None)
            },
        )
        .unwrap();
    assert!(wrong_origin.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("origin lane")
    )));
    assert_eq!(approval_calls, 0);

    engine
        .runtime_evidence
        .retain(|evidence| evidence.id != binding.evidence_id);
    let invalid_baseline = engine
        .handle_runtime_command(
            "invalid-baseline-bounce",
            RuntimeCommand::BounceMergeConflict {
                gate_id: format!("gate-{task_id}"),
                original_lane_id: "lane-origin".to_string(),
                owner: owner("lane-origin", task_id),
                reason: "baseline evidence vanished".to_string(),
            },
            &mut |_prompt| {
                approval_calls += 1;
                ApprovalResponse::allow_once(None)
            },
        )
        .unwrap();
    assert!(invalid_baseline.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("canonical evidence")
                || reason.contains("does not exist")
    )));
    assert_eq!(approval_calls, 0);
    assert!(engine.runtime_view_state().conflict_bounces.is_empty());
}

#[test]
fn trust_loop_accept_requires_validator_actor_and_exact_reviewed_hashes() {
    let cwd = temp_dir("trust_loop_exact_reviewer_binding_cwd");
    let home = temp_dir("trust_loop_exact_reviewer_binding_home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    let task_id = "task-exact-review";
    start_gate(&mut engine, &mut allow, task_id, vec!["patch".to_string()]);
    engine
        .handle_runtime_command(
            "handoff-exact-review",
            RuntimeCommand::CreateHandoff {
                handoff_id: "handoff-exact-review".to_string(),
                task_id: task_id.to_string(),
                from_lane_id: "lane-planner".to_string(),
                to_lane_id: "lane-origin".to_string(),
                owner: owner("lane-origin", task_id),
                summary: "origin owns reviewed patch".to_string(),
                acceptance: HandoffAcceptance::Accepted,
            },
            &mut allow,
        )
        .unwrap();
    let first = record_canonical_patch(
        &cwd,
        &mut engine,
        &mut allow,
        task_id,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+first\n",
    );
    engine
        .handle_runtime_command(
            "request-exact-review",
            RuntimeCommand::RequestReview {
                review_id: "review-exact".to_string(),
                gate_id: format!("gate-{task_id}"),
                requester_lane_id: "lane-origin".to_string(),
                reviewer_lane_id: "lane-reviewer".to_string(),
                owner: owner("lane-origin", task_id),
                evidence_ids: vec![first.evidence_id.clone()],
            },
            &mut allow,
        )
        .unwrap();

    let mut approval_calls = 0;
    let wrong_actor = engine
        .handle_runtime_command(
            "wrong-reviewer",
            RuntimeCommand::AcceptMergeGate {
                gate_id: format!("gate-{task_id}"),
                actor: owner("lane-intruder", task_id),
                reviewed_evidence: vec![first.clone()],
                decision: Some("intruder cannot approve".to_string()),
            },
            &mut |_prompt| {
                approval_calls += 1;
                ApprovalResponse::allow_once(None)
            },
        )
        .unwrap();
    assert!(wrong_actor.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("validator owner")
    )));
    assert_eq!(approval_calls, 0);

    let second = record_canonical_patch(
        &cwd,
        &mut engine,
        &mut allow,
        task_id,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+second\n",
    );
    assert_ne!(first.source_hash, second.source_hash);
    let drifted = engine
        .handle_runtime_command(
            "drifted-review",
            RuntimeCommand::AcceptMergeGate {
                gate_id: format!("gate-{task_id}"),
                actor: owner("lane-reviewer", task_id),
                reviewed_evidence: vec![first],
                decision: Some("old reviewed bytes cannot approve new patch".to_string()),
            },
            &mut allow,
        )
        .unwrap();
    assert!(drifted.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("reviewed evidence changed")
    )));
    let view = engine.runtime_view_state();
    assert_eq!(
        view.review_requests[0].status,
        viden_types::ReviewRequestStatus::Pending
    );
    assert_eq!(
        view.review_requests[0].evidence_ids,
        vec![second.evidence_id]
    );
    assert_ne!(view.merge_gates[0].status, MergeGateStatus::Accepted);
}

#[test]
fn trust_loop_artifact_shortcut_cannot_bypass_independent_review_policy() {
    let cwd = temp_dir("trust_loop_artifact_policy_cwd");
    let home = temp_dir("trust_loop_artifact_policy_home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    let task_id = "task-artifact-policy";
    start_gate(&mut engine, &mut allow, task_id, vec!["patch".to_string()]);
    engine
        .handle_runtime_command(
            "handoff-artifact-policy",
            RuntimeCommand::CreateHandoff {
                handoff_id: "handoff-artifact-policy".to_string(),
                task_id: task_id.to_string(),
                from_lane_id: "lane-planner".to_string(),
                to_lane_id: "lane-origin".to_string(),
                owner: owner("lane-origin", task_id),
                summary: "origin owns artifact".to_string(),
                acceptance: HandoffAcceptance::Accepted,
            },
            &mut allow,
        )
        .unwrap();
    let binding = record_canonical_patch(
        &cwd,
        &mut engine,
        &mut allow,
        task_id,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );
    engine
        .handle_runtime_command(
            "request-artifact-review",
            RuntimeCommand::RequestReview {
                review_id: "review-artifact-policy".to_string(),
                gate_id: format!("gate-{task_id}"),
                requester_lane_id: "lane-origin".to_string(),
                reviewer_lane_id: "lane-reviewer".to_string(),
                owner: owner("lane-origin", task_id),
                evidence_ids: vec![binding.evidence_id.clone()],
            },
            &mut allow,
        )
        .unwrap();

    let mut approval_calls = 0;
    let events = engine
        .handle_runtime_command(
            "artifact-shortcut",
            RuntimeCommand::AcceptAgentArtifact {
                gate_id: format!("gate-{task_id}"),
                evidence_id: binding.evidence_id,
                actor: owner("lane-reviewer", task_id),
                source_hash: binding.source_hash,
                decision: Some("legacy artifact shortcut".to_string()),
            },
            &mut |_prompt| {
                approval_calls += 1;
                ApprovalResponse::allow_once(None)
            },
        )
        .unwrap();
    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("independent review")
    )));
    assert_eq!(approval_calls, 0);
    assert_ne!(
        engine.runtime_view_state().merge_gates[0].status,
        MergeGateStatus::Accepted
    );
}

#[test]
fn trust_loop_reject_agent_artifact_requires_gate_bound_evidence_before_permission() {
    let cwd = temp_dir("trust_loop_reject_artifact_preflight_cwd");
    let home = temp_dir("trust_loop_reject_artifact_preflight_home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    start_gate(
        &mut engine,
        &mut allow,
        "task-reject-artifact-a",
        vec!["patch".to_string()],
    );
    start_gate(
        &mut engine,
        &mut allow,
        "task-reject-artifact-b",
        vec!["patch".to_string()],
    );
    let other_binding = record_canonical_patch(
        &cwd,
        &mut engine,
        &mut allow,
        "task-reject-artifact-b",
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+other\n",
    );

    let mut approval_calls = 0;
    let rejected = engine
        .handle_runtime_command(
            "reject-cross-gate-artifact",
            RuntimeCommand::RejectAgentArtifact {
                gate_id: "gate-task-reject-artifact-a".to_string(),
                evidence_id: other_binding.evidence_id,
                actor: owner("lane-origin", "task-reject-artifact-a"),
                reason: "cannot reject another gate evidence".to_string(),
            },
            &mut |_prompt| {
                approval_calls += 1;
                ApprovalResponse::allow_once(None)
            },
        )
        .unwrap();

    assert!(rejected.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("gate evidence")
    )));
    assert_eq!(approval_calls, 0);
}

#[test]
fn trust_loop_conflict_revalidation_requires_origin_lane_and_changed_receipt() {
    let cwd = temp_dir("trust_loop_origin_revalidation_cwd");
    let home = temp_dir("trust_loop_origin_revalidation_home");
    fs::create_dir_all(cwd.join("src")).unwrap();
    fs::write(cwd.join("src/lib.rs"), "current\n").unwrap();
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    let task_id = "task-origin-revalidation";
    start_gate(&mut engine, &mut allow, task_id, vec!["patch".to_string()]);
    engine
        .handle_runtime_command(
            "handoff-origin-revalidation",
            RuntimeCommand::CreateHandoff {
                handoff_id: "handoff-origin-revalidation".to_string(),
                task_id: task_id.to_string(),
                from_lane_id: "lane-planner".to_string(),
                to_lane_id: "lane-origin".to_string(),
                owner: owner("lane-origin", task_id),
                summary: "origin owns recovery".to_string(),
                acceptance: HandoffAcceptance::Accepted,
            },
            &mut allow,
        )
        .unwrap();
    let original = record_canonical_patch(
        &cwd,
        &mut engine,
        &mut allow,
        task_id,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+merged\n",
    );
    engine
        .handle_runtime_command(
            "request-origin-review",
            RuntimeCommand::RequestReview {
                review_id: "review-origin-1".to_string(),
                gate_id: format!("gate-{task_id}"),
                requester_lane_id: "lane-origin".to_string(),
                reviewer_lane_id: "lane-reviewer".to_string(),
                owner: owner("lane-origin", task_id),
                evidence_ids: vec![original.evidence_id.clone()],
            },
            &mut allow,
        )
        .unwrap();
    engine
        .handle_runtime_command(
            "accept-origin-review",
            RuntimeCommand::AcceptMergeGate {
                gate_id: format!("gate-{task_id}"),
                actor: owner("lane-reviewer", task_id),
                reviewed_evidence: vec![original.clone()],
                decision: Some("review exact original patch".to_string()),
            },
            &mut allow,
        )
        .unwrap();
    engine
        .handle_runtime_command(
            "merge-origin-conflict",
            RuntimeCommand::MergeAgentPatch {
                gate_id: format!("gate-{task_id}"),
                actor: owner("lane-origin", task_id),
                decision: Some("apply reviewed original patch".to_string()),
            },
            &mut allow,
        )
        .unwrap();
    let pending = engine.runtime_view_state().merge_gates[0]
        .conflict
        .clone()
        .expect("merge mismatch must create a conflict bounce");
    assert_eq!(pending.status, ConflictBounceStatus::Pending);

    let immediate = engine
        .handle_runtime_command(
            "accept-pending-conflict",
            RuntimeCommand::AcceptMergeGate {
                gate_id: format!("gate-{task_id}"),
                actor: owner("lane-reviewer", task_id),
                reviewed_evidence: vec![original.clone()],
                decision: Some("cannot accept pending conflict".to_string()),
            },
            &mut allow,
        )
        .unwrap();
    assert!(immediate.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("pending conflict")
    )));

    let unchanged = record_canonical_patch(
        &cwd,
        &mut engine,
        &mut allow,
        task_id,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+merged\n",
    );
    assert_eq!(unchanged.source_hash, original.source_hash);
    assert_eq!(
        engine.runtime_view_state().merge_gates[0]
            .conflict
            .as_ref()
            .map(|conflict| conflict.status),
        Some(ConflictBounceStatus::Pending)
    );
    let unchanged_revalidation = engine
        .handle_runtime_command(
            "unchanged-revalidation",
            RuntimeCommand::RevalidateMergeConflict {
                gate_id: format!("gate-{task_id}"),
                bounce_id: pending.bounce_id.clone(),
                actor: owner("lane-origin", task_id),
                evidence: unchanged,
            },
            &mut allow,
        )
        .unwrap();
    assert!(unchanged_revalidation.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("changed canonical receipt")
    )));

    let changed = record_canonical_patch(
        &cwd,
        &mut engine,
        &mut allow,
        task_id,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-current\n+merged\n",
    );
    assert_ne!(changed.source_hash, original.source_hash);
    let wrong_lane = engine
        .handle_runtime_command(
            "wrong-lane-revalidation",
            RuntimeCommand::RevalidateMergeConflict {
                gate_id: format!("gate-{task_id}"),
                bounce_id: pending.bounce_id.clone(),
                actor: owner("lane-intruder", task_id),
                evidence: changed.clone(),
            },
            &mut allow,
        )
        .unwrap();
    assert!(wrong_lane.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("origin lane")
    )));
    let revalidated = engine
        .handle_runtime_command(
            "origin-revalidation",
            RuntimeCommand::RevalidateMergeConflict {
                gate_id: format!("gate-{task_id}"),
                bounce_id: pending.bounce_id,
                actor: owner("lane-origin", task_id),
                evidence: changed.clone(),
            },
            &mut allow,
        )
        .unwrap();
    assert!(revalidated.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MergeConflictBounced { conflict }
            if conflict.status == ConflictBounceStatus::Revalidated
                && conflict.revalidation_evidence == vec![changed.clone()]
    )));
}

#[test]
fn trust_loop_conflict_bounces_to_origin_then_revalidates_merges_and_reverts() {
    let cwd = temp_dir("trust_loop_conflict_revert_cwd");
    let home = temp_dir("trust_loop_conflict_revert_home");
    fs::create_dir_all(cwd.join("src")).unwrap();
    fs::write(cwd.join("src/lib.rs"), "current\n").unwrap();
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let task_id = "task-conflict";
    start_gate(
        &mut engine,
        &mut approver,
        task_id,
        vec!["patch".to_string()],
    );
    engine
        .handle_runtime_command(
            "handoff-conflict",
            RuntimeCommand::CreateHandoff {
                handoff_id: "handoff-conflict".to_string(),
                task_id: task_id.to_string(),
                from_lane_id: "lane-planner".to_string(),
                to_lane_id: "lane-origin".to_string(),
                owner: owner("lane-origin", task_id),
                summary: "origin owns the patch".to_string(),
                acceptance: HandoffAcceptance::Accepted,
            },
            &mut approver,
        )
        .unwrap();
    let patch = b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+merged\n";
    let original = record_canonical_patch(&cwd, &mut engine, &mut approver, task_id, patch);
    engine
        .handle_runtime_command(
            "review-conflict",
            RuntimeCommand::RequestReview {
                review_id: "review-conflict".to_string(),
                gate_id: format!("gate-{task_id}"),
                requester_lane_id: "lane-origin".to_string(),
                reviewer_lane_id: "lane-reviewer".to_string(),
                owner: owner("lane-origin", task_id),
                evidence_ids: vec![original.evidence_id.clone()],
            },
            &mut approver,
        )
        .unwrap();
    let pending_review = engine.runtime_view_state().merge_gates[0].clone();
    assert_eq!(pending_review.status, MergeGateStatus::CollectingEvidence);
    assert!(pending_review.decision.as_ref().is_some_and(|decision| {
        decision.outcome == MergeGateDecisionOutcome::AwaitingEvidence
            && decision.reason == "independent_review_required"
    }));
    engine
        .handle_runtime_command(
            "accept-before-conflict",
            RuntimeCommand::AcceptMergeGate {
                gate_id: format!("gate-{task_id}"),
                actor: owner("lane-reviewer", task_id),
                reviewed_evidence: vec![original.clone()],
                decision: Some("independent reviewer accepted initial patch".to_string()),
            },
            &mut approver,
        )
        .unwrap();

    let conflicted = engine
        .handle_runtime_command(
            "merge-conflict",
            RuntimeCommand::MergeAgentPatch {
                gate_id: format!("gate-{task_id}"),
                actor: owner("lane-origin", task_id),
                decision: Some("apply reviewed patch".to_string()),
            },
            &mut approver,
        )
        .unwrap();
    assert!(conflicted.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MergeConflictBounced { conflict }
            if conflict.original_lane_id == "lane-origin"
                && conflict.status == ConflictBounceStatus::Pending
    )));
    assert_eq!(
        fs::read_to_string(cwd.join("src/lib.rs")).unwrap(),
        "current\n"
    );
    let pending = engine.runtime_view_state().merge_gates[0]
        .conflict
        .clone()
        .expect("conflict bounce should be durable");

    fs::write(cwd.join("src/lib.rs"), "old\n").unwrap();
    let revised_patch = b"diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+merged\n";
    let revised = record_canonical_patch(&cwd, &mut engine, &mut approver, task_id, revised_patch);
    engine
        .handle_runtime_command(
            "revalidate-conflict",
            RuntimeCommand::RevalidateMergeConflict {
                gate_id: format!("gate-{task_id}"),
                bounce_id: pending.bounce_id,
                actor: owner("lane-origin", task_id),
                evidence: revised.clone(),
            },
            &mut approver,
        )
        .unwrap();
    let gate = engine.runtime_view_state().merge_gates[0].clone();
    assert_eq!(
        gate.conflict.as_ref().map(|conflict| conflict.status),
        Some(ConflictBounceStatus::Revalidated)
    );
    engine
        .handle_runtime_command(
            "review-revalidated-conflict",
            RuntimeCommand::RequestReview {
                review_id: "review-conflict-2".to_string(),
                gate_id: format!("gate-{task_id}"),
                requester_lane_id: "lane-origin".to_string(),
                reviewer_lane_id: "lane-reviewer".to_string(),
                owner: owner("lane-origin", task_id),
                evidence_ids: vec![revised.evidence_id.clone()],
            },
            &mut approver,
        )
        .unwrap();

    let accepted = engine
        .handle_runtime_command(
            "accept-revalidated",
            RuntimeCommand::AcceptMergeGate {
                gate_id: format!("gate-{task_id}"),
                actor: owner("lane-reviewer", task_id),
                reviewed_evidence: vec![revised],
                decision: Some("independent reviewer accepted revalidation".to_string()),
            },
            &mut approver,
        )
        .unwrap();
    assert!(accepted.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::ReviewRequestUpdated { review }
            if review.review_id == "review-conflict-2"
                && review.status == viden_types::ReviewRequestStatus::Accepted
                && review.evidence_ids.iter().any(|id| id == "evidence-task-conflict")
    )));
    assert_eq!(
        engine.runtime_view_state().review_requests[1].status,
        viden_types::ReviewRequestStatus::Accepted
    );
    let merged = engine
        .handle_runtime_command(
            "merge-revalidated",
            RuntimeCommand::MergeAgentPatch {
                gate_id: format!("gate-{task_id}"),
                actor: owner("lane-origin", task_id),
                decision: Some("apply revalidated patch".to_string()),
            },
            &mut approver,
        )
        .unwrap();
    assert!(merged.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MergeGateUpdated { gate }
            if gate.status == MergeGateStatus::Merged
                && gate.applied_change_id.is_some()
                && gate.decision.as_ref().is_some_and(|decision| {
                    decision.outcome == MergeGateDecisionOutcome::Merged
                })
    )));
    assert_eq!(
        engine.runtime_view_state().conflict_bounces[0].status,
        ConflictBounceStatus::Resolved
    );
    assert_eq!(
        fs::read_to_string(cwd.join("src/lib.rs")).unwrap(),
        "merged\n"
    );

    let reverted = engine
        .handle_runtime_command(
            "revert-applied",
            RuntimeCommand::RevertAppliedChange {
                gate_id: format!("gate-{task_id}"),
                owner: owner("lane-reviewer", task_id),
                reason: "post-merge verification failed".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    assert!(reverted.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::RevertRecorded { revert }
            if revert.owner == owner("lane-reviewer", task_id)
                && revert.audit_id.starts_with("audit")
    )));
    assert_eq!(fs::read_to_string(cwd.join("src/lib.rs")).unwrap(), "old\n");
    assert_eq!(
        engine.runtime_view_state().merge_gates[0].status,
        MergeGateStatus::Reverted
    );
}

#[test]
fn trust_loop_revert_audit_failure_restores_applied_bytes_and_facts() {
    let cwd = temp_dir("trust_loop_revert_audit_failure_cwd");
    let home = temp_dir("trust_loop_revert_audit_failure_home");
    fs::create_dir_all(cwd.join("src")).unwrap();
    fs::write(cwd.join("src/lib.rs"), "old\n").unwrap();
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home.clone()),
    )
    .unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let task_id = "task-revert-failure";
    start_gate(
        &mut engine,
        &mut approver,
        task_id,
        vec!["patch".to_string()],
    );
    let patch = b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+merged\n";
    let binding = record_canonical_patch(&cwd, &mut engine, &mut approver, task_id, patch);
    let actor = engine.runtime_view_state().merge_gates[0].owner.clone();
    engine
        .handle_runtime_command(
            "accept-before-revert-failure",
            RuntimeCommand::AcceptMergeGate {
                gate_id: format!("gate-{task_id}"),
                actor: actor.clone(),
                reviewed_evidence: vec![binding],
                decision: Some("accept patch before revert test".to_string()),
            },
            &mut approver,
        )
        .unwrap();
    engine
        .handle_runtime_command(
            "merge-before-revert-failure",
            RuntimeCommand::MergeAgentPatch {
                gate_id: format!("gate-{task_id}"),
                actor,
                decision: Some("merge before revert".to_string()),
            },
            &mut approver,
        )
        .unwrap();
    let before = engine.runtime_view_state();
    // The audit fact and the revert intent precommit both succeed, then the
    // final durable fact fails after bytes have changed so compensation must
    // restore both domains. (Append 0 is the `change.reverted` audit record,
    // which is written before any byte moves; append 1 is the precommit.)
    engine.fail_after_workflow_appends_for_test(2);

    let rejected = engine
        .handle_runtime_command(
            "revert-audit-failure",
            RuntimeCommand::RevertAppliedChange {
                gate_id: format!("gate-{task_id}"),
                owner: owner("lane-reviewer", task_id),
                reason: "force workflow failure".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    assert!(rejected.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("injected workflow append failure")
    )));
    assert!(rejected.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::Error { error }
            if error.recoverable
                && error.hint.as_deref().is_some_and(|hint| hint.contains("restored"))
    )));
    assert_eq!(
        fs::read_to_string(cwd.join("src/lib.rs")).unwrap(),
        "merged\n"
    );
    assert_eq!(engine.runtime_view_state().merge_gates, before.merge_gates);
    assert_eq!(engine.runtime_view_state().reverts, before.reverts);
    let workflow_events = WorkflowStore::new(home, &cwd)
        .unwrap()
        .load_agent_events()
        .unwrap();
    assert!(workflow_events.iter().any(|event| {
        event.event_type == "runtime_projection_batch"
            && event
                .payload
                .get("runtime_events_json")
                .is_some_and(|json| json.contains("audit-revert-precommit"))
    }));
}

#[test]
fn trust_loop_supervisor_resumes_approved_mutation_with_the_original_owner() {
    let cwd = temp_dir("trust_loop_supervisor_approval_cwd");
    let home = temp_dir("trust_loop_supervisor_approval_home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let task_id = "task-supervised-trust";
    start_gate(
        &mut engine,
        &mut |_prompt| ApprovalResponse::allow_once(None),
        task_id,
        vec!["review".to_string()],
    );
    let supervisor = RuntimeSupervisor::start(engine);
    let command_owner = owner("lane-reviewer", task_id);
    supervisor
        .send_command_from_owner(
            command_owner.clone(),
            "record-supervised-review",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: format!("gate-{task_id}"),
                evidence_id: Some("evidence-supervised-review".to_string()),
                kind: "review".to_string(),
                summary: "review completed with canonical evidence pending".to_string(),
                path: None,
                source: Some("lane-reviewer".to_string()),
                canonical: None,
            },
        )
        .unwrap();
    let requested = collect_supervisor_events(&supervisor, |events| {
        events.iter().any(|event| {
            matches!(
                &event.event,
                viden_types::RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { .. },
                    ..
                })
            )
        })
    });
    let request_id = requested
        .iter()
        .find_map(|event| match &event.event {
            viden_types::RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) if approval.owner == command_owner => Some(approval.id.clone()),
            _ => None,
        })
        .expect("trust mutation should request approval for the submitting owner");

    supervisor
        .send_command_from_owner(
            command_owner.clone(),
            "approve-supervised-review",
            RuntimeCommand::RespondToApproval {
                request_id,
                response: ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    let resumed = collect_supervisor_events(&supervisor, |events| {
        events.iter().any(|event| {
            matches!(
                &event.event,
                viden_types::RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::EvidenceRecorded { evidence },
                    ..
                }) if evidence.id == "evidence-supervised-review"
            )
        })
    });
    assert!(resumed.iter().any(|event| {
        event.owner == command_owner
            && matches!(
                &event.event,
                viden_types::RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::EvidenceRecorded { evidence },
                    ..
                }) if evidence.id == "evidence-supervised-review"
            )
    }));

    supervisor
        .send_command_from_owner(
            command_owner.clone(),
            "deny-supervised-contract",
            RuntimeCommand::ConfirmContract {
                contract_id: "contract-supervised-denied".to_string(),
                task_id: task_id.to_string(),
                owner: command_owner.clone(),
                summary: "this contract must not survive denial".to_string(),
                decision: ContractDecision::Confirmed,
            },
        )
        .unwrap();
    let requested = collect_supervisor_events(&supervisor, |events| {
        events.iter().any(|event| {
            matches!(
                &event.event,
                viden_types::RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { .. },
                    ..
                })
            )
        })
    });
    let request_id = requested
        .iter()
        .find_map(|event| match &event.event {
            viden_types::RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(approval.id.clone()),
            _ => None,
        })
        .expect("contract mutation should request approval");
    supervisor
        .send_command_from_owner(
            command_owner,
            "reject-supervised-contract",
            RuntimeCommand::RespondToApproval {
                request_id,
                response: ApprovalResponse::deny(Some("contract rejected".to_string())),
            },
        )
        .unwrap();
    collect_supervisor_events(&supervisor, |events| {
        events.iter().any(|event| {
            matches!(
                &event.event,
                viden_types::RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalResolved {
                        decision: viden_types::ApprovalDecision::Deny,
                        ..
                    },
                    ..
                })
            )
        })
    });
    assert!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .contracts
            .is_empty()
    );
}

fn collect_supervisor_events(
    supervisor: &RuntimeSupervisor,
    done: impl Fn(&[viden_types::RuntimeEventEnvelope]) -> bool,
) -> Vec<viden_types::RuntimeEventEnvelope> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut events = Vec::new();
    while Instant::now() < deadline {
        if let Some(event) = supervisor.recv_event_envelope_timeout(Duration::from_millis(25)) {
            events.push(event);
            if done(&events) {
                return events;
            }
        }
    }
    panic!("timed out waiting for supervised trust-loop events: {events:#?}");
}

fn record_canonical_patch(
    cwd: &std::path::Path,
    engine: &mut SessionEngine,
    approver: &mut impl FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    task_id: &str,
    patch: &[u8],
) -> ReviewedEvidenceBinding {
    let evidence_id = format!("evidence-{task_id}");
    let bundle_id = format!("bundle-{task_id}");
    let mut store = ContextEngine::open(cwd.join(".viden/context-engine")).unwrap();
    let stored = store
        .store(ContextPutRequest {
            scope: ContextScope::Task(task_id.to_string()),
            kind: ContextContentKind::Diff,
            content: patch,
            evidence_id: Some(evidence_id.clone()),
        })
        .unwrap();
    let canonical = CanonicalEvidenceReference {
        item_id: stored.item.item_id.clone(),
        bundle_id: bundle_id.clone(),
        source_hash: stored.item.content_sha256.clone(),
        producer: EvidenceProducer {
            identity: "lane-origin".to_string(),
            role: "coder".to_string(),
            task_id: task_id.to_string(),
        },
        permission_snapshot_id: Some(format!("permission-{task_id}")),
        permission_scope: ContextScope::Task(task_id.to_string()),
        evidence_scope: ContextScope::Task(task_id.to_string()),
        verification: EvidenceVerificationState::Verified,
        quality: EvidenceQualityFacts {
            status: EvidenceQualityStatus::Pass,
            reason_codes: Vec::new(),
        },
    };
    let binding = ReviewedEvidenceBinding {
        evidence_id: evidence_id.clone(),
        source_hash: canonical.source_hash.clone(),
    };
    engine.set_merge_gate_context_facts_for_test(&bundle_id, stored.item);
    let events = engine
        .handle_runtime_command(
            format!("record-{evidence_id}"),
            RuntimeCommand::RecordAgentEvidence {
                gate_id: format!("gate-{task_id}"),
                evidence_id: Some(evidence_id),
                kind: "patch".to_string(),
                summary: "canonical patch".to_string(),
                path: None,
                source: Some("lane-origin".to_string()),
                canonical: Some(canonical),
            },
            approver,
        )
        .unwrap();
    assert!(
        events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::MergeGateUpdated { .. }))
    );
    binding
}

/// `runtime.conflict_content` on the merge path: a bounce raised by a failed
/// apply carries the lines that collided, and its baseline is the gate's
/// canonical evidence rather than a revision, because a gate's baseline *is*
/// its bindings.
#[test]
fn a_failed_merge_bounce_carries_the_lines_that_collided() {
    let cwd = temp_dir("merge_conflict_content_cwd");
    let home = temp_dir("merge_conflict_content_home");
    fs::create_dir_all(cwd.join("src")).unwrap();
    fs::write(cwd.join("src/lib.rs"), "current\n").unwrap();
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    let task_id = "task-conflict-content";
    start_gate(&mut engine, &mut allow, task_id, vec!["patch".to_string()]);
    engine
        .handle_runtime_command(
            "handoff-conflict-content",
            RuntimeCommand::CreateHandoff {
                handoff_id: "handoff-conflict-content".to_string(),
                task_id: task_id.to_string(),
                from_lane_id: "lane-planner".to_string(),
                to_lane_id: "lane-origin".to_string(),
                owner: owner("lane-origin", task_id),
                summary: "origin owns the merge".to_string(),
                acceptance: HandoffAcceptance::Accepted,
            },
            &mut allow,
        )
        .unwrap();
    let binding = record_canonical_patch(
        &cwd,
        &mut engine,
        &mut allow,
        task_id,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+merged\n",
    );
    engine
        .handle_runtime_command(
            "request-conflict-content-review",
            RuntimeCommand::RequestReview {
                review_id: "review-conflict-content".to_string(),
                gate_id: format!("gate-{task_id}"),
                requester_lane_id: "lane-origin".to_string(),
                reviewer_lane_id: "lane-reviewer".to_string(),
                owner: owner("lane-origin", task_id),
                evidence_ids: vec![binding.evidence_id.clone()],
            },
            &mut allow,
        )
        .unwrap();
    engine
        .handle_runtime_command(
            "accept-conflict-content",
            RuntimeCommand::AcceptMergeGate {
                gate_id: format!("gate-{task_id}"),
                actor: owner("lane-reviewer", task_id),
                reviewed_evidence: vec![binding.clone()],
                decision: Some("reviewed".to_string()),
            },
            &mut allow,
        )
        .unwrap();

    let events = engine
        .handle_runtime_command(
            "merge-conflict-content",
            RuntimeCommand::MergeAgentPatch {
                gate_id: format!("gate-{task_id}"),
                actor: owner("lane-origin", task_id),
                decision: Some("apply reviewed patch".to_string()),
            },
            &mut allow,
        )
        .unwrap();

    let bounced = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::MergeConflictBounced { conflict } => Some(conflict),
            _ => None,
        })
        .expect("a failed apply must bounce the conflict");
    let content = bounced
        .content
        .as_ref()
        .expect("a bounce raised by a failed apply must carry its content");
    assert_eq!(
        content.baseline,
        ConflictBaseline::Evidence {
            bindings: bounced.baseline_evidence.clone()
        },
        "the gate's canonical bindings are its baseline"
    );
    assert!(!content.truncated);
    assert_eq!(content.files.len(), 1);
    assert_eq!(content.files[0].path, "src/lib.rs");
    let hunk = &content.files[0].hunks[0];
    assert_eq!(hunk.reason, ConflictHunkReason::ContextMismatch);
    assert_eq!(hunk.ours, vec!["current\n"]);
    assert_eq!(hunk.theirs, vec!["merged\n"]);
    assert_eq!(hunk.base.as_deref(), Some(["old\n".to_string()].as_slice()));

    // The view carries the same content, so a client reads it from
    // `RuntimeViewState` rather than holding the event.
    let view = engine.runtime_view_state();
    assert_eq!(
        view.merge_gates[0]
            .conflict
            .as_ref()
            .and_then(|conflict| conflict.content.as_ref()),
        Some(content)
    );
}

/// An operator bounce is a human judgement with a reason, not a failed apply.
/// There is no rejected hunk behind it, so it publishes no content rather than
/// an empty one.
#[test]
fn an_operator_bounce_publishes_no_content() {
    let cwd = temp_dir("operator_bounce_content_cwd");
    let home = temp_dir("operator_bounce_content_home");
    fs::create_dir_all(cwd.join("src")).unwrap();
    fs::write(cwd.join("src/lib.rs"), "old\n").unwrap();
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    let task_id = "task-operator-bounce-content";
    start_gate(&mut engine, &mut allow, task_id, vec!["patch".to_string()]);
    engine
        .handle_runtime_command(
            "handoff-operator-bounce-content",
            RuntimeCommand::CreateHandoff {
                handoff_id: "handoff-operator-bounce-content".to_string(),
                task_id: task_id.to_string(),
                from_lane_id: "lane-planner".to_string(),
                to_lane_id: "lane-origin".to_string(),
                owner: owner("lane-origin", task_id),
                summary: "origin owns the merge".to_string(),
                acceptance: HandoffAcceptance::Accepted,
            },
            &mut allow,
        )
        .unwrap();
    record_canonical_patch(
        &cwd,
        &mut engine,
        &mut allow,
        task_id,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );

    let events = engine
        .handle_runtime_command(
            "operator-bounce-content",
            RuntimeCommand::BounceMergeConflict {
                gate_id: format!("gate-{task_id}"),
                original_lane_id: "lane-origin".to_string(),
                owner: owner("lane-origin", task_id),
                reason: "the approach is wrong, not the lines".to_string(),
            },
            &mut allow,
        )
        .unwrap();

    let bounced = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::MergeConflictBounced { conflict } => Some(conflict),
            _ => None,
        })
        .expect("the operator bounce must be recorded");
    assert!(bounced.content.is_none());
}
