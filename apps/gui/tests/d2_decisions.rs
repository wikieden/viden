use std::sync::{Arc, Mutex};

use viden_core::{
    ApprovalRequestView, ApprovalScope, ContractDecision, ContractRecord, EvidenceView,
    MergeGateRecord, MergeGateStatus, MergeGateValidator, ReviewRequestRecord, ReviewRequestStatus,
    ReviewVerdict, RuntimeCommand, RuntimeCommandEnvelope, RuntimeEventKind, RuntimeOwner,
    RuntimeSnapshot, RuntimeViewState,
};
use viden_gui::{
    D2_REVIEW_NO_ACTOR_CODE, D2_REVIEW_SETTLED_CODE, D2Intent, GuiCoreAdapter, PermissionChoice,
};

mod support;
use support::TestCoreClient;

const APPROVAL_FIXTURE: &str = include_str!(
    "../../../crates/types/tests/fixtures/frontend-contract-v1/approval-allow-deny.json"
);

fn owner(lane: &str) -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace-viden".to_string(),
        project_id: "project-viden".to_string(),
        lane_id: Some(lane.to_string()),
        session_id: Some(format!("session-{lane}")),
        task_id: Some(format!("task-{lane}")),
        turn_id: None,
    }
}

/// Builds a decision queue that carries one fact per D2 group so the
/// projection cannot pass by reusing a single fact family.
fn decision_view() -> (RuntimeViewState, ApprovalRequestView) {
    let fixture: serde_json::Value = serde_json::from_str(APPROVAL_FIXTURE).unwrap();
    let snapshot: RuntimeSnapshot =
        serde_json::from_value(fixture["initial_snapshot"].clone()).unwrap();
    let mut view = RuntimeViewState::new(snapshot);

    let mut approval: ApprovalRequestView = serde_json::from_value(
        fixture["events"][0]["event"]["kind"]["payload"]["approval"].clone(),
    )
    .unwrap();
    approval.owner = owner("lane-gate");
    approval.allowed_scopes = vec![
        ApprovalScope::Once,
        ApprovalScope::Session {
            session_id: "session-lane-gate".to_string(),
        },
    ];
    view.pending_approvals.push(approval.clone());

    view.contracts.push(ContractRecord {
        contract_id: "contract-feel-v1-1".to_string(),
        task_id: "task-lane-contract".to_string(),
        owner: owner("lane-contract"),
        summary: "feel param ranges v1.0 -> v1.1".to_string(),
        decision: ContractDecision::Confirmed,
        audit_id: "audit-contract".to_string(),
        updated_at: 1_700_000_100,
    });

    view.review_requests.push(ReviewRequestRecord {
        review_id: "review-jump-feel".to_string(),
        gate_id: "gate-integration".to_string(),
        task_id: "task-lane-review".to_string(),
        requester_lane_id: "lane-review".to_string(),
        reviewer_lane_id: "lane-owner".to_string(),
        owner: owner("lane-review"),
        evidence_ids: vec!["evidence-playtest".to_string()],
        evidence_bindings: Vec::new(),
        status: ReviewRequestStatus::Pending,
        feedback: None,
        audit_id: "audit-review".to_string(),
        updated_at: 1_700_000_200,
    });

    view.latest_evidence.push(EvidenceView {
        id: "evidence-playtest".to_string(),
        kind: "playtest".to_string(),
        summary: "jump feel validated".to_string(),
        path: Some("evidence/playtest.json".to_string()),
        source: Some("lane-review".to_string()),
        canonical: None,
        metadata: None,
        timestamp: Some(1_700_000_150),
        owner: None,
    });

    (view, approval)
}

fn connected(view: RuntimeViewState) -> (GuiCoreAdapter, Arc<Mutex<Vec<RuntimeCommandEnvelope>>>) {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = GuiCoreAdapter::new(Box::new(TestCoreClient::new(view, Arc::clone(&sent))));
    adapter.connect().unwrap();
    (adapter, sent)
}

#[test]
fn d2_projects_one_queue_from_gate_contract_and_review_facts() {
    let (view, approval) = decision_view();
    let (adapter, _) = connected(view);
    let projection = adapter.d2_decisions().expect("D2 projection");

    let gate = projection.group("gate").expect("gate group");
    let contract = projection.group("contract").expect("contract group");
    let review = projection.group("review").expect("review group");

    assert_eq!(gate.items.len(), 1, "one pending approval is one gate item");
    assert_eq!(gate.items[0].id, approval.id);
    assert_eq!(gate.items[0].lane_id.as_deref(), Some("lane-gate"));
    // The risk bucket is the fixture's Core fact, never a GUI heuristic.
    assert_eq!(gate.items[0].risk.as_deref(), Some("high"));

    assert_eq!(contract.items.len(), 1);
    assert_eq!(contract.items[0].id, "contract-feel-v1-1");
    assert_eq!(contract.items[0].lane_id.as_deref(), Some("lane-contract"));
    // ContractRecord carries a recorded decision and Core has no
    // "awaiting confirmation" contract fact, so the group must declare that
    // it is decided history rather than a human backlog.
    assert_eq!(contract.items[0].status, "confirmed");
    assert_eq!(
        contract.unavailable.as_ref().map(|entry| entry.code),
        Some("GUI-CORE-013")
    );

    assert_eq!(review.items.len(), 1);
    assert_eq!(review.items[0].id, "review-jump-feel");
    assert_eq!(review.items[0].lane_id.as_deref(), Some("lane-review"));
    assert_eq!(review.items[0].status, "pending");

    // The command bar count is the queue total of things actually awaiting a
    // human: pending approvals and pending reviews only.
    assert_eq!(projection.pending_total, 2);
}

#[test]
fn d2_selects_the_first_gate_item_and_renders_only_core_owned_context() {
    let (view, approval) = decision_view();
    let (adapter, _) = connected(view);
    let projection = adapter.d2_decisions().expect("D2 projection");
    let detail = projection.detail.expect("detail for the selected item");

    assert_eq!(detail.id, approval.id);
    assert_eq!(detail.kind, "gate");
    // The design shows a line-level diff. Core exposes only an opaque input
    // preview, so D2 renders the preview verbatim and declares the structured
    // diff unavailable instead of synthesizing diff rows.
    assert_eq!(detail.context.source, "approval_input_preview");
    assert_eq!(detail.context.text, approval.input_preview);
    assert!(
        detail.context.unavailable.is_some(),
        "structured diff must be declared unavailable, not invented"
    );
    assert_eq!(detail.audit_id, approval.audit_id);
}

#[test]
fn d2_gate_actions_come_from_core_allowed_scopes_and_send_respond_to_approval() {
    let (view, approval) = decision_view();
    let (mut adapter, sent) = connected(view);

    let projection = adapter.d2_decisions().expect("D2 projection");
    let detail = projection.detail.expect("detail");
    let kinds: Vec<&str> = detail
        .actions
        .iter()
        .filter(|action| action.available)
        .map(|action| action.kind.as_str())
        .collect();
    assert_eq!(kinds, vec!["once", "session", "deny"]);

    adapter
        .d2_send_intent(
            "gui-d2-test",
            D2Intent::RespondApproval {
                request_id: approval.id.clone(),
                choice: PermissionChoice::Once,
                feedback: None,
            },
        )
        .expect("respond to approval");

    let commands = sent.lock().unwrap();
    let last = commands.last().expect("one command was sent");
    assert!(
        matches!(
            &last.command,
            RuntimeCommand::RespondToApproval { request_id, .. } if request_id == &approval.id
        ),
        "gate decisions must travel as RespondToApproval, got {:?}",
        last.command
    );
}

#[test]
fn d2_contract_decision_sends_confirm_contract_with_the_core_owned_identity() {
    let (view, _) = decision_view();
    let (mut adapter, sent) = connected(view);

    adapter
        .d2_send_intent(
            "gui-d2-contract",
            D2Intent::DecideContract {
                contract_id: "contract-feel-v1-1".to_string(),
                accept: true,
            },
        )
        .expect("confirm contract");

    let commands = sent.lock().unwrap();
    let last = commands.last().expect("one command was sent");
    match &last.command {
        RuntimeCommand::ConfirmContract {
            contract_id,
            task_id,
            owner,
            decision,
            ..
        } => {
            assert_eq!(contract_id, "contract-feel-v1-1");
            // Identity is replayed from the Core record; the GUI never
            // reconstructs the owner from display strings.
            assert_eq!(task_id, "task-lane-contract");
            assert_eq!(owner.lane_id.as_deref(), Some("lane-contract"));
            assert_eq!(decision, &ContractDecision::Confirmed);
        }
        other => panic!("expected ConfirmContract, got {other:?}"),
    }
}

/// A published `ContractRecord` is always already decided — `ContractDecision`
/// has only `Confirmed` and `Rejected` — and Core rejects a second
/// `ConfirmContract` for an id it has recorded. The detail therefore offers no
/// live verdict: both actions stay visible, disabled, and named by the request
/// that would make a pending contract exist.
#[test]
fn d2_contract_detail_keeps_both_verdicts_visible_and_non_actionable() {
    let (view, _) = decision_view();
    let (adapter, _) = connected(view);
    let projection = adapter
        .d2_decisions_for("contract-feel-v1-1")
        .expect("contract detail");
    let detail = projection.detail.expect("detail");

    assert_eq!(detail.kind, "contract");
    let kinds: Vec<&str> = detail
        .actions
        .iter()
        .map(|action| action.kind.as_str())
        .collect();
    assert_eq!(
        kinds,
        vec!["confirm_contract", "reject_contract"],
        "both verdicts stay visible rather than being hidden"
    );
    for action in &detail.actions {
        assert!(
            !action.available,
            "{} must not be actionable against a decided record",
            action.kind
        );
        assert_eq!(action.code, Some("GUI-CORE-013"));
    }
}

#[test]
fn d2_review_items_expose_evidence_and_enable_the_decision_core_now_accepts() {
    let (view, _) = decision_view();
    let (adapter, _) = connected(view);
    let projection = adapter
        .d2_decisions_for("review-jump-feel")
        .expect("review detail");
    let detail = projection.detail.expect("detail");

    assert_eq!(detail.kind, "review");
    assert_eq!(detail.evidence.len(), 1);
    assert_eq!(detail.evidence[0].id, "evidence-playtest");
    assert_eq!(detail.evidence[0].summary, "jump feel validated");

    // GUI-CORE-011 is closed: Core publishes `DecideReview`, and the review is
    // Pending with a reviewer actor this client can derive, so both verdicts
    // are live rather than fail-closed.
    let kinds: Vec<&str> = detail
        .actions
        .iter()
        .filter(|action| action.available)
        .map(|action| action.kind.as_str())
        .collect();
    assert_eq!(kinds, vec!["accept_review", "reject_review"]);
    assert!(
        detail.actions.iter().all(|action| action.code.is_none()),
        "a live review decision names no blocking code"
    );
}

/// The reviewer identity Core stamped on the gate validator, and the actor the
/// GUI must reproduce when it is missing.
fn reviewer_actor() -> RuntimeOwner {
    let mut actor = owner("lane-review");
    actor.lane_id = Some("lane-owner".to_string());
    actor.session_id = None;
    actor.turn_id = None;
    actor
}

/// The gate the review was installed on, with Core's own validator record.
fn validator_gate(review_id: &str, validator_owner: RuntimeOwner) -> MergeGateRecord {
    MergeGateRecord {
        gate_id: "gate-integration".to_string(),
        task_id: "task-lane-review".to_string(),
        status: MergeGateStatus::NeedsChanges,
        required_evidence: Vec::new(),
        evidence_ids: vec!["evidence-playtest".to_string()],
        gate_type: Default::default(),
        owner: owner("lane-review"),
        validator: Some(MergeGateValidator {
            owner: validator_owner,
            review_request_id: review_id.to_string(),
            independent: true,
            validated_at: None,
        }),
        policy_snapshot: Default::default(),
        decision: None,
        conflict: None,
        applied_change_id: None,
        recovery_snapshot: None,
        audit_ids: Vec::new(),
        updated_at: Some(1_700_000_200),
    }
}

/// The `DecideReview` command Core echoes back on `CommandAccepted`.
fn decide_review(review_id: &str, accept: bool, actor: RuntimeOwner) -> RuntimeCommand {
    RuntimeCommand::DecideReview {
        review_id: review_id.to_string(),
        verdict: if accept {
            ReviewVerdict::Accepted
        } else {
            ReviewVerdict::Rejected
        },
        feedback: None,
        actor,
    }
}

/// The `ReviewRequestUpdated` fact Core emits from `decide_review`.
fn settled_review(review_id: &str, status: ReviewRequestStatus) -> ReviewRequestRecord {
    ReviewRequestRecord {
        review_id: review_id.to_string(),
        gate_id: "gate-integration".to_string(),
        task_id: "task-lane-review".to_string(),
        requester_lane_id: "lane-review".to_string(),
        reviewer_lane_id: "lane-owner".to_string(),
        owner: owner("lane-review"),
        evidence_ids: vec!["evidence-playtest".to_string()],
        evidence_bindings: Vec::new(),
        status,
        feedback: None,
        audit_id: "audit-review-decided".to_string(),
        updated_at: 1_700_000_300,
    }
}

#[test]
fn d2_review_actor_replays_the_validator_core_recorded_for_this_review() {
    let (mut view, _) = decision_view();
    // Core stores the reviewer owner on the gate validator when the review is
    // requested; that record, not a reconstruction, is the preferred actor.
    let mut stamped = reviewer_actor();
    stamped.task_id = Some("task-lane-review".to_string());
    view.merge_gates
        .push(validator_gate("review-jump-feel", stamped.clone()));
    let (mut adapter, sent) = connected(view);

    adapter
        .d2_send_intent(
            "gui-d2-review-validator",
            D2Intent::DecideReview {
                review_id: "review-jump-feel".to_string(),
                accept: true,
                feedback: None,
            },
        )
        .expect("send the review verdict");

    let commands = sent.lock().unwrap();
    let last = commands.last().expect("one command was sent");
    match &last.command {
        RuntimeCommand::DecideReview { actor, .. } => assert_eq!(actor, &stamped),
        other => panic!("expected DecideReview, got {other:?}"),
    }
    assert_eq!(last.owner, stamped);
}

#[test]
fn d2_review_accept_sends_decide_review_and_confirms_only_on_the_core_review_fact() {
    let (view, _) = decision_view();
    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = TestCoreClient::new(view, Arc::clone(&sent))
        .with_event(RuntimeEventKind::CommandAccepted {
            command_id: "gui-d2-review-accept".to_string(),
            command: decide_review("review-jump-feel", true, reviewer_actor()),
        })
        .with_event(RuntimeEventKind::ReviewRequestUpdated {
            review: settled_review("review-jump-feel", ReviewRequestStatus::Accepted),
        })
        // Core emits the validator stamp right after the review fact. It must
        // be tolerated, never mistaken for the confirmation itself.
        .with_event(RuntimeEventKind::MergeGateUpdated {
            gate: validator_gate("review-jump-feel", reviewer_actor()),
        });
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().unwrap();

    let result = adapter
        .d2_send_intent(
            "gui-d2-review-accept",
            D2Intent::DecideReview {
                review_id: "review-jump-feel".to_string(),
                accept: true,
                feedback: None,
            },
        )
        .expect("send the review verdict");

    assert_eq!(result.outcome.state, "confirmed");
    assert_eq!(result.pending_command_id, None);

    let commands = sent.lock().unwrap();
    let last = commands.last().expect("one command was sent");
    match &last.command {
        RuntimeCommand::DecideReview {
            review_id,
            verdict,
            feedback,
            actor,
        } => {
            assert_eq!(review_id, "review-jump-feel");
            // Core's own wire token for the verdict; the GUI never invents one.
            assert_eq!(
                serde_json::to_value(verdict).unwrap(),
                serde_json::json!("accepted")
            );
            assert_eq!(feedback, &None);
            // `validate_review_decider` demands the independent reviewer lane.
            assert_eq!(actor.lane_id.as_deref(), Some("lane-owner"));
            assert_eq!(actor.task_id.as_deref(), Some("task-lane-review"));
            assert_eq!(actor.session_id, None);
            assert_eq!(actor.turn_id, None);
        }
        other => panic!("expected DecideReview, got {other:?}"),
    }
}

#[test]
fn d2_review_reject_carries_the_reviewer_feedback_core_stores() {
    let (view, _) = decision_view();
    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = TestCoreClient::new(view, Arc::clone(&sent))
        .with_event(RuntimeEventKind::CommandAccepted {
            command_id: "gui-d2-review-reject".to_string(),
            command: decide_review("review-jump-feel", false, reviewer_actor()),
        })
        .with_event(RuntimeEventKind::ReviewRequestUpdated {
            review: settled_review("review-jump-feel", ReviewRequestStatus::Rejected),
        });
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().unwrap();

    let result = adapter
        .d2_send_intent(
            "gui-d2-review-reject",
            D2Intent::DecideReview {
                review_id: "review-jump-feel".to_string(),
                accept: false,
                // Surrounding whitespace is trimmed the way Core trims before
                // it validates, so a padded note is never sent as-is.
                feedback: Some("  jump arc still overshoots  ".to_string()),
            },
        )
        .expect("send the review verdict");

    assert_eq!(result.outcome.state, "confirmed");

    let commands = sent.lock().unwrap();
    match &commands.last().expect("one command was sent").command {
        RuntimeCommand::DecideReview {
            verdict, feedback, ..
        } => {
            assert_eq!(
                serde_json::to_value(verdict).unwrap(),
                serde_json::json!("rejected")
            );
            assert_eq!(feedback.as_deref(), Some("jump arc still overshoots"));
        }
        other => panic!("expected DecideReview, got {other:?}"),
    }
}

#[test]
fn d2_review_empty_feedback_is_absence_rather_than_an_empty_note() {
    let (view, _) = decision_view();
    let (mut adapter, sent) = connected(view);

    adapter
        .d2_send_intent(
            "gui-d2-review-blank",
            D2Intent::DecideReview {
                review_id: "review-jump-feel".to_string(),
                accept: true,
                feedback: Some("   ".to_string()),
            },
        )
        .expect("send the review verdict");

    let commands = sent.lock().unwrap();
    match &commands.last().expect("one command was sent").command {
        RuntimeCommand::DecideReview { feedback, .. } => assert_eq!(feedback, &None),
        other => panic!("expected DecideReview, got {other:?}"),
    }
}

#[test]
fn d2_refuses_review_feedback_over_the_core_limit_instead_of_truncating_it() {
    let (view, _) = decision_view();
    let (mut adapter, sent) = connected(view);

    let error = adapter
        .d2_send_intent(
            "gui-d2-review-long",
            D2Intent::DecideReview {
                review_id: "review-jump-feel".to_string(),
                accept: false,
                feedback: Some("x".repeat(501)),
            },
        )
        .expect_err("over-limit feedback is refused locally");
    assert!(
        error.contains("500"),
        "the refusal must name Core's own limit, got {error}"
    );
    assert!(
        sent.lock().unwrap().is_empty(),
        "nothing may reach Core when the local check refuses"
    );
}

#[test]
fn d2_review_is_not_confirmed_by_an_update_naming_another_review() {
    let (mut view, _) = decision_view();
    let mut other = settled_review("review-other", ReviewRequestStatus::Pending);
    other.audit_id = "audit-review-other".to_string();
    view.review_requests.push(other);
    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = TestCoreClient::new(view, Arc::clone(&sent))
        .with_event(RuntimeEventKind::CommandAccepted {
            command_id: "gui-d2-review-mismatch".to_string(),
            command: decide_review("review-jump-feel", true, reviewer_actor()),
        })
        .with_event(RuntimeEventKind::ReviewRequestUpdated {
            review: settled_review("review-other", ReviewRequestStatus::Accepted),
        });
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().unwrap();

    let result = adapter
        .d2_send_intent(
            "gui-d2-review-mismatch",
            D2Intent::DecideReview {
                review_id: "review-jump-feel".to_string(),
                accept: true,
                feedback: None,
            },
        )
        .expect("send the review verdict");

    assert_eq!(result.outcome.state, "pending");
    assert_eq!(
        result.pending_command_id.as_deref(),
        Some("gui-d2-review-mismatch")
    );
}

#[test]
fn d2_review_passes_a_core_rejection_reason_through_verbatim() {
    let (view, _) = decision_view();
    let reason = "review decision requires the independent reviewer lane";
    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = TestCoreClient::new(view, Arc::clone(&sent)).with_event(
        RuntimeEventKind::CommandRejected {
            command_id: "gui-d2-review-refused".to_string(),
            reason: reason.to_string(),
        },
    );
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().unwrap();

    let result = adapter
        .d2_send_intent(
            "gui-d2-review-refused",
            D2Intent::DecideReview {
                review_id: "review-jump-feel".to_string(),
                accept: true,
                feedback: None,
            },
        )
        .expect("send the review verdict");

    assert_eq!(result.outcome.state, "rejected");
    assert_eq!(result.outcome.reason.as_deref(), Some(reason));
    assert_eq!(result.pending_command_id, None);
}

#[test]
fn d2_refuses_a_second_review_decision_while_one_is_still_pending() {
    let (view, _) = decision_view();
    let (mut adapter, _) = connected(view);

    adapter
        .d2_send_intent(
            "gui-d2-review-first",
            D2Intent::DecideReview {
                review_id: "review-jump-feel".to_string(),
                accept: true,
                feedback: None,
            },
        )
        .expect("first verdict is sent");

    let error = adapter
        .d2_send_intent(
            "gui-d2-review-second",
            D2Intent::DecideReview {
                review_id: "review-jump-feel".to_string(),
                accept: false,
                feedback: None,
            },
        )
        .expect_err("a second verdict must be refused while one is unanswered");
    assert!(
        error.contains("gui-d2-review-first"),
        "the refusal must name the command still in flight, got {error}"
    );
}

#[test]
fn d2_review_actions_fail_closed_with_a_local_code_when_no_actor_is_derivable() {
    let (mut view, _) = decision_view();
    // Core published the review without an owner and without a gate validator,
    // so there is no identity `validate_review_decider` would accept.
    view.review_requests[0].owner = RuntimeOwner::default();
    let (mut adapter, sent) = connected(view);

    let projection = adapter
        .d2_decisions_for("review-jump-feel")
        .expect("review detail");
    let detail = projection.detail.expect("detail");
    assert!(detail.actions.iter().all(|action| !action.available));
    assert!(
        detail
            .actions
            .iter()
            .all(|action| action.code == Some(D2_REVIEW_NO_ACTOR_CODE)),
        "a review with no derivable actor names the local reason, not GUI-CORE-011"
    );

    let error = adapter
        .d2_send_intent(
            "gui-d2-review-no-actor",
            D2Intent::DecideReview {
                review_id: "review-jump-feel".to_string(),
                accept: true,
                feedback: None,
            },
        )
        .expect_err("an undecidable review must be refused before the wire");
    assert!(error.contains("reviewer"), "got {error}");
    assert!(sent.lock().unwrap().is_empty());
}

#[test]
fn d2_review_actions_fail_closed_once_core_has_settled_the_review() {
    let (mut view, _) = decision_view();
    view.review_requests[0].status = ReviewRequestStatus::Accepted;
    let (mut adapter, sent) = connected(view);

    let projection = adapter
        .d2_decisions_for("review-jump-feel")
        .expect("review detail");
    let detail = projection.detail.expect("detail");
    assert!(
        detail
            .actions
            .iter()
            .all(|action| !action.available && action.code == Some(D2_REVIEW_SETTLED_CODE)),
        "a decided review can never be re-decided"
    );

    adapter
        .d2_send_intent(
            "gui-d2-review-settled",
            D2Intent::DecideReview {
                review_id: "review-jump-feel".to_string(),
                accept: false,
                feedback: None,
            },
        )
        .expect_err("a settled review must be refused before the wire");
    assert!(sent.lock().unwrap().is_empty());
}

#[test]
fn d2_scopes_a_review_decision_to_its_parent_gate() {
    let (view, _) = decision_view();
    let (adapter, _) = connected(view);
    let detail = adapter
        .d2_decisions_for("review-jump-feel")
        .expect("review projection")
        .detail
        .expect("review detail");
    // Every trust record in the chain links the gate (crates/runtime/src/
    // trust_loop.rs), so scoping by the parent gate is what makes the audit
    // view the decision chain rather than one isolated record.
    let scope = detail.audit_scope.expect("a review offers its audit trail");
    assert_eq!(scope.kind, "merge_gate");
    assert_eq!(scope.id, "gate-integration");
}

#[test]
fn d2_scopes_a_contract_decision_to_the_contract_object() {
    let (view, _) = decision_view();
    let (adapter, _) = connected(view);
    let detail = adapter
        .d2_decisions_for("contract-feel-v1-1")
        .expect("contract projection")
        .detail
        .expect("contract detail");
    let scope = detail
        .audit_scope
        .expect("a contract offers its audit trail");
    assert_eq!(scope.kind, "contract");
    assert_eq!(scope.id, "contract-feel-v1-1");
}

#[test]
fn d2_offers_no_audit_scope_for_a_tool_approval_core_links_no_object_for() {
    let (view, approval) = decision_view();
    let (adapter, _) = connected(view);
    let detail = adapter
        .d2_decisions_for(&approval.id)
        .expect("approval projection")
        .detail
        .expect("approval detail");
    // C7 appends the durable row when the decision is *applied*, keyed by the
    // permission object. This item is still pending, so no row exists yet and
    // an affordance here would open an empty timeline and claim it was the
    // decision's trail. The trail is offered where the decision exists: the
    // permission rows in the dock and in the ordered transcript.
    assert_eq!(detail.audit_scope, None);
}
