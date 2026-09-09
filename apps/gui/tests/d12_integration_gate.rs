use std::sync::{Arc, Mutex};
use std::time::Duration;

use viden_core::{
    ConflictBounce, ConflictBounceStatus, EvidenceView, MergeGateStatus, RevertRecord,
    RuntimeCommand, RuntimeCommandEnvelope, RuntimeOwner, RuntimeSnapshot, RuntimeViewState,
};
use viden_gui::{D12Intent, GuiCoreAdapter, d12_action_code};

mod support;
use support::TestCoreClient;

const MERGE_GATE_FIXTURE: &str =
    include_str!("../../../crates/types/tests/fixtures/frontend-contract-v1/merge-gate.json");

fn owner(lane: &str) -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace-viden".to_string(),
        project_id: "project-boss-rush".to_string(),
        lane_id: Some(lane.to_string()),
        task_id: Some(format!("task-{lane}")),
        ..Default::default()
    }
}

/// One canonical evidence record shaped exactly as Core publishes it.
///
/// Acceptance needs the canonical reference: `decide_merge_gate` builds its
/// reviewed-evidence bindings from `canonical.source_hash`, so evidence
/// without one can never satisfy the gate.
///
/// Built through the wire form on purpose: `viden-core` re-exports
/// `EvidenceView` but not the canonical reference's own types, and the GUI
/// track must not reach around the facade into `viden-types`.
fn canonical_evidence(id: &str, task_id: &str, hash: &str) -> EvidenceView {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "kind": "test_result",
        "summary": "replay regression passed",
        "path": format!("artifacts/{id}.txt"),
        "source": "core",
        "canonical": {
            "item_id": format!("item-{id}"),
            "bundle_id": "bundle-1",
            "source_hash": hash,
            "producer": {
                "identity": "lane-3",
                "role": "coder",
                "task_id": task_id,
            },
            "permission_snapshot_id": "permission-1",
            "permission_scope": { "type": "task", "id": task_id },
            "evidence_scope": { "type": "task", "id": task_id },
            "verification": "verified",
            "quality": { "status": "pass" },
        },
        "timestamp": 1_700_000_070,
    }))
    .expect("canonical evidence fixture")
}

/// An open gate whose evidence Core has recorded canonically, owned by a real
/// Lane. This is the shape both decisions are actually allowed on.
fn acceptable_gate_view() -> RuntimeViewState {
    let mut view = gate_view();
    let task_id = view.merge_gates[0].task_id.clone();
    view.merge_gates[0].status = MergeGateStatus::NeedsChanges;
    view.merge_gates[0].owner = owner("lane-3");
    view.merge_gates[0].required_evidence = vec!["replay-regression".to_string()];
    view.merge_gates[0].evidence_ids = vec!["replay-regression".to_string()];
    view.latest_evidence.push(canonical_evidence(
        "replay-regression",
        &task_id,
        "a".repeat(64).as_str(),
    ));
    view
}

fn recording(view: RuntimeViewState) -> (GuiCoreAdapter, Arc<Mutex<Vec<RuntimeCommandEnvelope>>>) {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = GuiCoreAdapter::new(Box::new(TestCoreClient::new(view, Arc::clone(&sent))));
    adapter.connect().unwrap();
    (adapter, sent)
}

fn accept_code(adapter: &GuiCoreAdapter) -> Option<String> {
    adapter
        .d12_integration_gate()
        .unwrap()
        .detail
        .unwrap()
        .actions
        .iter()
        .find(|action| action.kind == "accept")
        .and_then(|action| action.code.map(str::to_string))
}

fn gate_view() -> RuntimeViewState {
    #[derive(serde::Deserialize)]
    struct Fixture {
        initial_snapshot: RuntimeSnapshot,
        events: Vec<viden_core::RuntimeEventEnvelope>,
    }
    let fixture: Fixture = serde_json::from_str(MERGE_GATE_FIXTURE).unwrap();
    let mut view = RuntimeViewState::new(fixture.initial_snapshot);
    for envelope in fixture.events {
        if let viden_core::RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
    }
    assert!(
        !view.merge_gates.is_empty(),
        "the merge-gate fixture must publish a gate"
    );
    view
}

fn connected(view: RuntimeViewState) -> GuiCoreAdapter {
    let mut adapter = GuiCoreAdapter::new(Box::new(TestCoreClient::new(
        view,
        Arc::new(Mutex::new(Vec::new())),
    )));
    adapter.connect().unwrap();
    adapter
}

#[test]
fn d12_projects_each_core_merge_gate_with_its_policy() {
    let view = gate_view();
    let expected: Vec<String> = view
        .merge_gates
        .iter()
        .map(|gate| gate.gate_id.clone())
        .collect();
    let projection = connected(view).d12_integration_gate().expect("D12");

    assert_eq!(
        projection
            .gates
            .iter()
            .map(|gate| gate.gate_id.clone())
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        projection.selected_gate_id.as_deref(),
        expected.first().map(String::as_str)
    );
}

#[test]
fn d12_accept_stays_unavailable_until_every_required_evidence_id_is_present() {
    let mut view = gate_view();
    view.merge_gates[0].status = MergeGateStatus::CollectingEvidence;
    view.merge_gates[0].required_evidence = vec!["replay-regression".to_string()];
    view.merge_gates[0].evidence_ids.clear();

    let blocked = connected(view.clone()).d12_integration_gate().unwrap();
    let detail = blocked.detail.expect("detail");
    let accept = detail
        .actions
        .iter()
        .find(|action| action.kind == "accept")
        .expect("accept action");
    assert!(!accept.available, "a strong gate cannot be bypassed");
    assert_eq!(
        detail.missing_evidence,
        vec!["replay-regression".to_string()]
    );
    // The client offers no manual-merge escape hatch.
    assert!(
        detail
            .actions
            .iter()
            .all(|action| action.kind == "accept" || action.kind == "reject")
    );

    assert_eq!(
        accept_code(&connected(view.clone())),
        Some(d12_action_code::MISSING_EVIDENCE.to_string())
    );

    // Listing the id is not enough: Core builds the reviewed-evidence
    // bindings from the canonical reference, so a gate whose evidence has none
    // still cannot be accepted.
    view.merge_gates[0].evidence_ids = vec!["replay-regression".to_string()];
    let uncanonical = connected(view).d12_integration_gate().unwrap();
    let uncanonical_detail = uncanonical.detail.expect("detail");
    assert!(uncanonical_detail.missing_evidence.is_empty());
    let uncanonical_accept = uncanonical_detail
        .actions
        .iter()
        .find(|action| action.kind == "accept")
        .unwrap();
    assert!(!uncanonical_accept.available);
    assert_eq!(
        uncanonical_accept.code,
        Some(d12_action_code::EVIDENCE_NOT_CANONICAL)
    );

    let ready_view = acceptable_gate_view();
    let ready = connected(ready_view).d12_integration_gate().unwrap();
    let ready_detail = ready.detail.expect("detail");
    assert!(ready_detail.missing_evidence.is_empty());
    let ready_accept = ready_detail
        .actions
        .iter()
        .find(|action| action.kind == "accept")
        .unwrap();
    assert!(ready_accept.available);
    assert_eq!(ready_accept.code, None);
}

#[test]
fn d12_accept_stays_closed_while_the_policy_validator_is_missing() {
    let mut view = acceptable_gate_view();
    view.merge_gates[0]
        .policy_snapshot
        .requires_independent_validator = true;

    assert_eq!(
        accept_code(&connected(view)),
        Some(d12_action_code::VALIDATOR_REQUIRED.to_string())
    );
}

#[test]
fn d12_accept_stays_closed_while_a_conflict_bounce_is_pending() {
    let mut view = acceptable_gate_view();
    let gate_id = view.merge_gates[0].gate_id.clone();
    view.merge_gates[0].conflict = Some(ConflictBounce {
        bounce_id: "bounce-pending".to_string(),
        gate_id,
        task_id: view.merge_gates[0].task_id.clone(),
        original_lane_id: "lane-3".to_string(),
        owner: owner("lane-3"),
        reason: "dash.gd conflicts with the merged baseline".to_string(),
        status: ConflictBounceStatus::Pending,
        evidence_ids: vec![],
        baseline_evidence: vec![],
        revalidation_evidence: vec![],
        content: None,
        audit_id: "audit-bounce-pending".to_string(),
        created_at: 1_700_000_700,
        revalidated_at: None,
    });

    assert_eq!(
        accept_code(&connected(view)),
        Some(d12_action_code::CONFLICT_PENDING.to_string())
    );
}

#[test]
fn d12_bounce_stays_closed_when_core_published_no_actor() {
    // `validate_reject_actor` refuses the default owner outright, so a gate
    // Core owns anonymously offers no bounce this client could send.
    let mut view = acceptable_gate_view();
    view.merge_gates[0].owner = RuntimeOwner::default();

    let detail = connected(view)
        .d12_integration_gate()
        .unwrap()
        .detail
        .unwrap();
    let reject = detail
        .actions
        .iter()
        .find(|action| action.kind == "reject")
        .unwrap();
    assert!(!reject.available);
    assert_eq!(reject.code, Some(d12_action_code::NO_ACTOR));
}

#[test]
fn d12_accept_sends_the_gate_owner_and_its_canonical_bindings() {
    let view = acceptable_gate_view();
    let gate_id = view.merge_gates[0].gate_id.clone();
    let (mut adapter, sent) = recording(view);

    adapter
        .send_d12_intent_and_wait(
            "command-accept",
            D12Intent::Accept {
                gate_id: gate_id.clone(),
                reviewed_evidence: None,
                decision: Some("evidence reviewed".to_string()),
            },
            Duration::ZERO,
        )
        .expect("accept is sent");

    let envelopes = sent.lock().unwrap();
    let [envelope] = envelopes.as_slice() else {
        panic!("exactly one command must reach Core");
    };
    assert_eq!(envelope.command_id, "command-accept");
    match &envelope.command {
        RuntimeCommand::AcceptMergeGate {
            gate_id: sent_gate,
            actor,
            reviewed_evidence,
            decision,
        } => {
            assert_eq!(sent_gate, &gate_id);
            // The actor is replayed from the gate Core published, never
            // rebuilt from the display text of the screen.
            assert_eq!(actor, &owner("lane-3"));
            assert_eq!(
                reviewed_evidence
                    .iter()
                    .map(|binding| binding.evidence_id.as_str())
                    .collect::<Vec<_>>(),
                vec!["replay-regression"]
            );
            assert_eq!(reviewed_evidence[0].source_hash, "a".repeat(64));
            assert_eq!(decision.as_deref(), Some("evidence reviewed"));
        }
        other => panic!("unexpected command {other:?}"),
    }
}

#[test]
fn d12_bounce_sends_the_operator_reason_and_refuses_an_empty_one() {
    let view = acceptable_gate_view();
    let gate_id = view.merge_gates[0].gate_id.clone();
    let (mut adapter, sent) = recording(view);

    let blank = adapter.send_d12_intent_and_wait(
        "command-blank",
        D12Intent::Bounce {
            gate_id: gate_id.clone(),
            reason: "   ".to_string(),
        },
        Duration::ZERO,
    );
    assert!(blank.is_err(), "an empty bounce reason never reaches Core");
    assert!(sent.lock().unwrap().is_empty());

    adapter
        .send_d12_intent_and_wait(
            "command-bounce",
            D12Intent::Bounce {
                gate_id: gate_id.clone(),
                reason: "rebase onto the merged baseline, keep the input buffer".to_string(),
            },
            Duration::ZERO,
        )
        .expect("bounce is sent");

    let envelopes = sent.lock().unwrap();
    let [envelope] = envelopes.as_slice() else {
        panic!("exactly one command must reach Core");
    };
    match &envelope.command {
        RuntimeCommand::RejectMergeGate {
            gate_id: sent_gate,
            actor,
            reason,
        } => {
            assert_eq!(sent_gate, &gate_id);
            assert_eq!(actor, &owner("lane-3"));
            assert_eq!(
                reason,
                "rebase onto the merged baseline, keep the input buffer"
            );
        }
        other => panic!("unexpected command {other:?}"),
    }
}

#[test]
fn d12_confirms_a_decision_only_on_the_gate_status_core_published() {
    let view = acceptable_gate_view();
    let gate_id = view.merge_gates[0].gate_id.clone();
    let mut accepted_gate = view.merge_gates[0].clone();
    accepted_gate.status = MergeGateStatus::Accepted;
    let mut wrong_gate = view.merge_gates[0].clone();
    wrong_gate.status = MergeGateStatus::Blocked;

    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = TestCoreClient::new(view, Arc::clone(&sent))
        .with_event(viden_core::RuntimeEventKind::CommandAccepted {
            command_id: "command-accept".to_string(),
            command: RuntimeCommand::AcceptMergeGate {
                gate_id: gate_id.clone(),
                actor: owner("lane-3"),
                reviewed_evidence: vec![],
                decision: None,
            },
        })
        // A transition this command did not ask for must not confirm it.
        .with_event(viden_core::RuntimeEventKind::MergeGateUpdated { gate: wrong_gate })
        .with_event(viden_core::RuntimeEventKind::MergeGateUpdated {
            gate: accepted_gate,
        });
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().unwrap();

    let result = adapter
        .send_d12_intent_and_wait(
            "command-accept",
            D12Intent::Accept {
                gate_id,
                reviewed_evidence: None,
                decision: None,
            },
            Duration::ZERO,
        )
        .expect("accept is sent");

    assert_eq!(result.outcome.state, "confirmed");
    assert_eq!(result.pending_command_id, None);
}

#[test]
fn d12_passes_a_core_rejection_reason_through_verbatim() {
    let view = acceptable_gate_view();
    let gate_id = view.merge_gates[0].gate_id.clone();

    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = TestCoreClient::new(view, Arc::clone(&sent)).with_event(
        viden_core::RuntimeEventKind::CommandRejected {
            command_id: "command-accept".to_string(),
            reason: "merge gate `gate_merge` requires an independent validator".to_string(),
        },
    );
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().unwrap();

    let result = adapter
        .send_d12_intent_and_wait(
            "command-accept",
            D12Intent::Accept {
                gate_id,
                reviewed_evidence: None,
                decision: None,
            },
            Duration::ZERO,
        )
        .expect("the send itself succeeds");

    assert_eq!(result.outcome.state, "rejected");
    assert_eq!(
        result.outcome.reason.as_deref(),
        Some("merge gate `gate_merge` requires an independent validator")
    );
    assert_eq!(result.pending_command_id, None);
}

#[test]
fn d12_refuses_a_decision_on_a_gate_that_vanished_or_closed_before_the_click() {
    let view = acceptable_gate_view();
    let gate_id = view.merge_gates[0].gate_id.clone();
    let (mut adapter, sent) = recording(view);

    let vanished = adapter.send_d12_intent_and_wait(
        "command-vanished",
        D12Intent::Bounce {
            gate_id: "gate-that-never-existed".to_string(),
            reason: "stale click".to_string(),
        },
        Duration::ZERO,
    );
    assert!(vanished.is_err());

    let mut closed_view = acceptable_gate_view();
    closed_view.merge_gates[0].status = MergeGateStatus::Merged;
    let (mut closed_adapter, closed_sent) = recording(closed_view);
    let closed = closed_adapter.send_d12_intent_and_wait(
        "command-closed",
        D12Intent::Accept {
            gate_id,
            reviewed_evidence: None,
            decision: None,
        },
        Duration::ZERO,
    );
    assert!(closed.is_err());

    // Fail-closed means nothing left the host.
    assert!(sent.lock().unwrap().is_empty());
    assert!(closed_sent.lock().unwrap().is_empty());
}

#[test]
fn d12_scopes_the_recovery_timeline_to_the_selected_gate() {
    let mut view = gate_view();
    let gate_id = view.merge_gates[0].gate_id.clone();
    view.conflict_bounces.push(ConflictBounce {
        bounce_id: "bounce-1".to_string(),
        gate_id: gate_id.clone(),
        task_id: "task-lane-3".to_string(),
        original_lane_id: "lane-3".to_string(),
        owner: owner("lane-3"),
        reason: "src/player/dash.gd conflicts with the merged baseline".to_string(),
        status: ConflictBounceStatus::Revalidated,
        evidence_ids: vec![],
        baseline_evidence: vec![],
        revalidation_evidence: vec![],
        content: None,
        audit_id: "audit-bounce-1".to_string(),
        created_at: 1_700_000_700,
        revalidated_at: Some(1_700_000_800),
    });
    view.conflict_bounces.push(ConflictBounce {
        bounce_id: "bounce-other".to_string(),
        gate_id: "gate-unrelated".to_string(),
        task_id: "task-lane-9".to_string(),
        original_lane_id: "lane-9".to_string(),
        owner: owner("lane-9"),
        reason: "unrelated".to_string(),
        status: ConflictBounceStatus::Pending,
        evidence_ids: vec![],
        baseline_evidence: vec![],
        revalidation_evidence: vec![],
        content: None,
        audit_id: "audit-bounce-other".to_string(),
        created_at: 1_700_000_710,
        revalidated_at: None,
    });

    let detail = connected(view)
        .d12_integration_gate()
        .unwrap()
        .detail
        .expect("detail");
    assert_eq!(
        detail.bounces.len(),
        1,
        "another gate's bounce must not leak"
    );
    assert_eq!(detail.bounces[0].bounce_id, "bounce-1");
    assert_eq!(detail.bounces[0].original_lane_id, "lane-3");
    assert_eq!(detail.bounces[0].status, "revalidated");
}

#[test]
fn d12_surfaces_the_post_merge_revert_for_its_own_gate() {
    let mut view = gate_view();
    let gate_id = view.merge_gates[0].gate_id.clone();
    view.merge_gates[0].status = MergeGateStatus::Merged;
    view.reverts.push(RevertRecord {
        revert_id: "revert-1".to_string(),
        gate_id,
        applied_change_id: "change-1".to_string(),
        owner: owner("lane-3"),
        reason: "cancel window regressed".to_string(),
        restored_paths: vec!["src/player/dash.gd".to_string()],
        audit_id: "audit-revert-1".to_string(),
        reverted_at: 1_700_000_900,
    });

    let detail = connected(view)
        .d12_integration_gate()
        .unwrap()
        .detail
        .expect("detail");
    assert_eq!(detail.reverts.len(), 1);
    assert_eq!(detail.reverts[0].revert_id, "revert-1");
    assert_eq!(
        detail.reverts[0].restored_paths,
        vec!["src/player/dash.gd".to_string()]
    );
    assert_eq!(detail.reverts[0].audit_id, "audit-revert-1");
}

#[test]
fn d12_names_the_missing_capability_rather_than_the_closed_request() {
    // GUI-CORE-015 is closed, so the row must not cite it any more. What can
    // still be absent is the capability, and a Core without it publishes the
    // bounce reason and nothing else.
    let mut client = TestCoreClient::new(gate_view(), Arc::new(Mutex::new(Vec::new())));
    client
        .capabilities
        .remove(viden_gui::CONFLICT_CONTENT_CAPABILITY);
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().unwrap();
    let projection = adapter.d12_integration_gate().unwrap();

    assert!(!projection.conflict_content_available);
    let entry = projection
        .unavailable
        .iter()
        .find(|entry| entry.key == "d12.conflict.noStructuredHunk")
        .expect("an absent capability must be declared");
    assert_eq!(entry.code, viden_gui::CONFLICT_CONTENT_CAPABILITY);
    assert!(
        projection
            .unavailable
            .iter()
            .all(|entry| entry.code != "GUI-CORE-015"),
        "the closed request must not be cited"
    );

    // With the capability advertised the screen declares nothing unavailable:
    // a bounce that carries no content is the screen's own per-bounce
    // sentence, which is a different fact.
    let available = connected(gate_view()).d12_integration_gate().unwrap();
    assert!(available.conflict_content_available);
    assert!(available.unavailable.is_empty());
}

#[test]
fn d12_scopes_a_revert_row_to_the_audit_object_core_actually_linked() {
    let mut view = gate_view();
    view.reverts.push(RevertRecord {
        revert_id: "revert-1".to_string(),
        gate_id: view.merge_gates[0].gate_id.clone(),
        applied_change_id: "change-1".to_string(),
        owner: owner("lane-3"),
        reason: "cancel window regressed".to_string(),
        restored_paths: vec!["src/player/dash.gd".to_string()],
        audit_id: "audit-revert-1".to_string(),
        reverted_at: 1_700_000_900,
    });

    let detail = connected(view)
        .d12_integration_gate()
        .unwrap()
        .detail
        .expect("detail");
    // `change.reverted` links `revert:<revert_id>` (crates/runtime/src/
    // trust_loop.rs), so the row scopes by that object and not by its audit id,
    // which `AuditQuery` cannot filter on at all.
    let scope = detail.reverts[0]
        .audit_scope
        .as_ref()
        .expect("a revert row must offer its audit trail");
    assert_eq!(scope.kind, "revert");
    assert_eq!(scope.id, "revert-1");
}

/* ------------------------------------------------------------------ */
/* Structured conflict content (`runtime.conflict_content`)            */
/* ------------------------------------------------------------------ */

const CONFLICT_CONTENT_FIXTURE: &str =
    include_str!("../../../crates/types/tests/fixtures/frontend-contract-v1/conflict-content.json");

/// The canonical `conflict-content.json` view: Lane A merges, Lane B's patch
/// is refused with one hunk, and a Lane apply conflict carries the same shape.
fn conflict_view() -> RuntimeViewState {
    #[derive(serde::Deserialize)]
    struct Fixture {
        initial_snapshot: RuntimeSnapshot,
        events: Vec<viden_core::RuntimeEventEnvelope>,
    }
    let fixture: Fixture = serde_json::from_str(CONFLICT_CONTENT_FIXTURE).unwrap();
    let mut view = RuntimeViewState::new(fixture.initial_snapshot);
    for envelope in fixture.events {
        if let viden_core::RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
    }
    view
}

#[test]
fn d12_projects_the_hunks_two_sides_and_preimage_core_published_for_a_bounce() {
    let detail = connected(conflict_view())
        .d12_integration_gate_for("gate_conflict_b")
        .unwrap()
        .detail
        .expect("the conflicting gate has a detail");

    let content = detail.bounces[0]
        .content
        .as_ref()
        .expect("the fixture's bounce carries structured content");
    assert!(!content.truncated);

    // The merge path's baseline is the gate's canonical evidence bindings,
    // not a bare commit, and each binding routes to its own audit object.
    assert_eq!(content.baseline.kind, "evidence");
    assert!(content.baseline.sha.is_none());
    let binding = &content.baseline.bindings[0];
    assert_eq!(binding.evidence_id, "ev_conflict_patch_b");
    assert_eq!(binding.source_hash, "cf".repeat(32));
    assert_eq!(binding.short_hash, "cfcfcfcfcfcf");
    assert_eq!(binding.audit_scope.kind, "evidence");
    assert_eq!(binding.audit_scope.id, "ev_conflict_patch_b");

    let file = &content.files[0];
    assert_eq!(file.path, "crates/runtime/src/trust_loop.rs");
    assert!(!file.omitted);

    // Two sides plus the patch preimage. Nothing here is a merge result.
    let hunk = &file.hunks[0];
    assert_eq!(hunk.ours_start, 42);
    assert_eq!(
        hunk.ours,
        vec!["    let bounce = record_conflict_bounce(gate)?;\n".to_string()]
    );
    assert_eq!(hunk.theirs_start, 42);
    assert_eq!(
        hunk.theirs,
        vec!["    let bounce = bounce_with_reason(gate, reason)?;\n".to_string()]
    );
    assert_eq!(
        hunk.base,
        Some(vec!["    let bounce = record_bounce(gate)?;\n".to_string()])
    );
    assert_eq!(hunk.reason, "context_mismatch");
}

#[test]
fn d12_projects_lane_apply_conflicts_with_the_same_content_and_a_revision_baseline() {
    let detail = connected(conflict_view())
        .d12_integration_gate_for("gate_conflict_b")
        .unwrap()
        .detail
        .expect("detail");

    // The Lane apply path is keyed by Lane; this gate's own Lane is the link,
    // never a client-invented association.
    let conflict = detail
        .lane_conflicts
        .iter()
        .find(|conflict| conflict.lane_id == "lane_conflict_b")
        .expect("the fixture publishes a Lane apply conflict for this Lane");
    assert_eq!(
        conflict.paths,
        vec!["crates/runtime/src/trust_loop.rs".to_string()]
    );
    let content = conflict
        .content
        .as_ref()
        .expect("the Lane apply conflict carries content");
    // The apply path names a revision rather than evidence bindings.
    assert_eq!(content.baseline.kind, "revision");
    assert_eq!(content.baseline.sha.as_deref(), Some(&"9f".repeat(20)[..]));
    assert_eq!(content.baseline.short_sha.as_deref(), Some("9f9f9f9f9f9f"));
    assert!(content.baseline.bindings.is_empty());
    assert_eq!(content.files[0].hunks[0].reason, "context_mismatch");
}

#[test]
fn d12_keeps_an_omitted_file_and_a_truncated_payload_distinct_from_an_empty_conflict() {
    let mut view = gate_view();
    let gate_id = view.merge_gates[0].gate_id.clone();
    // Built through Core's own wire form: the facade re-exports `ConflictBounce`
    // but not the `ConflictContent` family it carries, so the fixture is the
    // exact JSON Core publishes rather than a client-side literal.
    view.conflict_bounces.push(
        serde_json::from_value(serde_json::json!({
            "bounce_id": "bounce-omitted",
            "gate_id": gate_id,
            "task_id": "task-lane-3",
            "original_lane_id": "lane-3",
            "owner": { "workspace_id": "workspace-viden", "project_id": "project-boss-rush" },
            "reason": "patch conflict",
            "status": "pending",
            "evidence_ids": [],
            "baseline_evidence": [],
            "revalidation_evidence": [],
            "content": {
                "baseline": "unknown",
                "files": [{
                    "path": "assets/atlas.png",
                    "hunks": [],
                    "omitted": true
                }],
                "truncated": true
            },
            "audit_id": "audit-bounce-omitted",
            "created_at": 1_700_000_700,
            "revalidated_at": null
        }))
        .expect("Core's own conflict-bounce encoding"),
    );

    let detail = connected(view)
        .d12_integration_gate()
        .unwrap()
        .detail
        .expect("detail");
    let content = detail.bounces[0].content.as_ref().expect("content");
    assert!(content.truncated);
    // `Unknown` is a real Core answer — Core held no baseline it could name —
    // and must never be silently rendered as `HEAD`.
    assert_eq!(content.baseline.kind, "unknown");
    assert!(content.baseline.sha.is_none());
    assert!(content.files[0].omitted);
    assert!(content.files[0].hunks.is_empty());
}

#[test]
fn d12_leaves_a_bounce_without_content_absent_rather_than_empty() {
    let mut view = gate_view();
    view.conflict_bounces.push(ConflictBounce {
        bounce_id: "bounce-operator".to_string(),
        gate_id: view.merge_gates[0].gate_id.clone(),
        task_id: "task-lane-3".to_string(),
        original_lane_id: "lane-3".to_string(),
        owner: owner("lane-3"),
        reason: "cancel window needs a rethink".to_string(),
        status: ConflictBounceStatus::Pending,
        evidence_ids: Vec::new(),
        baseline_evidence: Vec::new(),
        revalidation_evidence: Vec::new(),
        content: None,
        audit_id: "audit-bounce-operator".to_string(),
        created_at: 1_700_000_700,
        revalidated_at: None,
    });

    let detail = connected(view)
        .d12_integration_gate()
        .unwrap()
        .detail
        .expect("detail");
    // An operator bounce is a human judgement with no failed apply behind it,
    // so Core publishes no content. Absent stays absent: it is not an empty
    // conflict and not a missing capability.
    assert!(detail.bounces[0].content.is_none());
    assert_eq!(detail.bounces[0].reason, "cancel window needs a rethink");
}
