//! The host half of the E3a native defects (checkpoints.md, defects 14-17).
//!
//! Three host rules, each of which the native run proved missing:
//!
//! 1. **One drain, one reconciliation.** Several surfaces drain the same
//!    ordered stream. Whichever one happens to consume an event must reconcile
//!    *every* correlated command this adapter holds, or the surface that was
//!    waiting for it waits forever — which is what left the commit bar saying
//!    "waiting for the permission decision" beside a completed commit.
//! 2. **A decision is not a verdict.** D2's approve sends
//!    `RespondToApproval`; only Core's `ApprovalResolved` settles it, and only
//!    the projection refreshed after it can show the next pending approval.
//! 3. **A probe is a read, so its wait may be abandoned.** Core answers one
//!    command at a time and gives an ACP handshake up to 90 s. Abandoning an
//!    unanswered `ProbeAgentAdapter` frees the slot without claiming anything
//!    about Core; abandoning anything that mutates would not be allowed.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use viden_core::{
    ApprovalDecision, ApprovalRequestView, ApprovalScope, EventCursor, FRONTEND_SCHEMA_V1,
    LaneRuntimeOwnerBinding, OperatorGitAction, OperatorGitOutcome, RuntimeEvent,
    RuntimeEventEnvelope, RuntimeEventKind, RuntimeOwner, RuntimeSnapshot, RuntimeViewState,
    RuntimeWireEvent, SourceTarget, WorkspaceSourceStatus, WorkspaceSourceView,
};
use viden_gui::{D1Intent, D2Intent, GuiCoreAdapter, OperatorGitIntent, PermissionChoice};

mod support;
use support::TestCoreClient;

const TIMEOUT: Duration = Duration::from_millis(10);
const LANE: &str = "lane-alpha";

fn multi_lane_view() -> RuntimeViewState {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../crates/types/tests/fixtures/frontend-contract-v1/multi-lane.json"
    ))
    .expect("fixture json");
    let snapshot: RuntimeSnapshot =
        serde_json::from_value(fixture["initial_snapshot"].clone()).expect("fixture snapshot");
    RuntimeViewState::new(snapshot)
}

fn lane_owner() -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden".to_string(),
        lane_id: Some(LANE.to_string()),
        session_id: Some("session-alpha".to_string()),
        task_id: Some("task-alpha".to_string()),
        turn_id: None,
    }
}

fn view_with_lane_owner() -> RuntimeViewState {
    let mut view = multi_lane_view();
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::LaneRuntimeOwnerBound {
            binding: LaneRuntimeOwnerBinding {
                lane_id: LANE.to_string(),
                owner: lane_owner(),
            },
        },
    ));
    view
}

fn envelope(sequence: u64, kind: RuntimeEventKind) -> RuntimeEventEnvelope {
    RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: RuntimeOwner::default(),
        cursor: EventCursor {
            stream_id: "gui-test".to_string(),
            sequence,
        },
        event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
            sequence,
            Some(1_700_000_000 + sequence),
            kind,
        )),
    }
}

fn source() -> WorkspaceSourceView {
    WorkspaceSourceView {
        status: WorkspaceSourceStatus::Ready,
        branch: Some("main".to_string()),
        worktree: Some("/workspace/viden".to_string()),
        ahead: 0,
        behind: 0,
        added: 0,
        deleted: 0,
        dirty: false,
    }
}

/* ---- defect 17: one drain reconciles every correlated command ---- */

#[test]
fn a_finished_action_drained_by_the_cockpit_poll_still_settles_the_commit_bar() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = TestCoreClient::new(view_with_lane_owner(), Arc::clone(&sent))
        // Nothing for the action's own drain to find: Core answers only after
        // the cockpit's 250 ms poll has already taken the stream.
        .with_gap()
        .with_envelope(envelope(
            10,
            RuntimeEventKind::OperatorGitActionFinished {
                command_id: "gui-git-commit".to_string(),
                target: SourceTarget::Lane {
                    lane_id: LANE.to_string(),
                },
                action: OperatorGitAction::Commit {
                    message: "Edit the fixture README heading".to_string(),
                },
                outcome: OperatorGitOutcome::Completed {
                    output: "[main a5d3d53] Edit the fixture README heading".to_string(),
                    truncated: false,
                    source: source(),
                },
                audit_id: "audit_operator_git_commit".to_string(),
            },
        ));
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().unwrap();

    // The commit leaves with no answer yet, so the bar is legitimately pending.
    let pending = adapter
        .run_operator_git_action_and_wait(
            "gui-git-commit",
            Some(LANE),
            OperatorGitIntent::Commit {
                message: "Edit the fixture README heading".to_string(),
            },
            Duration::ZERO,
        )
        .expect("the commit is sent");
    assert_eq!(pending.outcome.state, "pending");

    // The cockpit's own poll is the next thing to touch the stream.
    adapter.poll_d1(Some(LANE), TIMEOUT).expect("the D1 poll");

    let settled = adapter.operator_git(Some(LANE));
    assert_eq!(
        settled.outcome.state, "confirmed",
        "an OperatorGitActionFinished consumed by another drain must still settle the action"
    );
    assert_eq!(settled.pending_command_id, None);
    assert_eq!(
        settled.result.as_ref().map(|result| result.kind),
        Some("completed")
    );
}

/* ---- defect 16: the Decision Center's own verdict ---- */

fn approval_view() -> (RuntimeViewState, ApprovalRequestView, ApprovalRequestView) {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../crates/types/tests/fixtures/frontend-contract-v1/approval-allow-deny.json"
    ))
    .expect("fixture json");
    let snapshot: RuntimeSnapshot =
        serde_json::from_value(fixture["initial_snapshot"].clone()).expect("fixture snapshot");
    let mut view = RuntimeViewState::new(snapshot);

    let mut first: ApprovalRequestView = serde_json::from_value(
        fixture["events"][0]["event"]["kind"]["payload"]["approval"].clone(),
    )
    .expect("fixture approval");
    first.owner = lane_owner();
    first.allowed_scopes = vec![ApprovalScope::Once];
    first.id = "approval_git_add".to_string();
    first.tool_name = "git_add".to_string();
    first.title = "Approve git_add".to_string();
    first.audit_id = "audit_git_add".to_string();

    let mut second = first.clone();
    second.id = "approval_git_commit".to_string();
    second.tool_name = "git_commit".to_string();
    second.title = "Approve git_commit".to_string();
    second.audit_id = "audit_git_commit".to_string();

    view.pending_approvals.push(first.clone());
    (view, first, second)
}

#[test]
fn d2_approve_waits_for_the_verdict_and_then_lists_the_next_approval() {
    let (view, first, second) = approval_view();
    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = TestCoreClient::new(view, Arc::clone(&sent))
        .with_envelope(envelope(
            20,
            RuntimeEventKind::ApprovalResolved {
                request_id: first.id.clone(),
                decision: ApprovalDecision::Allow {
                    scope: ApprovalScope::Once,
                },
                owner: first.owner.clone(),
                audit_id: first.audit_id.clone(),
            },
        ))
        // The next gate Core raises for the same task, exactly as the native
        // run's `git_commit` followed its `git_add`.
        .with_envelope(envelope(
            21,
            RuntimeEventKind::ApprovalRequested {
                approval: second.clone(),
            },
        ));
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().unwrap();

    let result = adapter
        .d2_send_intent_and_wait(
            "gui-d2-approve",
            D2Intent::RespondApproval {
                request_id: first.id.clone(),
                choice: PermissionChoice::Once,
                feedback: None,
            },
            TIMEOUT,
        )
        .expect("the approval decision is sent");

    assert_eq!(
        result.outcome.state, "confirmed",
        "only Core's ApprovalResolved settles a decision"
    );
    assert_eq!(result.pending_command_id, None);

    let gate = result.projection.group("gate").expect("gate group");
    let ids: Vec<&str> = gate.items.iter().map(|item| item.id.as_str()).collect();
    assert!(
        !ids.contains(&first.id.as_str()),
        "the decided approval must leave the queue, not stay listed as pending"
    );
    assert_eq!(result.projection.pending_total, 0);

    // The next gate arrives on the same ordered stream a moment later, as
    // `git_commit` did after `git_add`. Any drain may carry it; the queue must
    // then list it rather than keeping the decided card.
    adapter.poll_d1(None, TIMEOUT).expect("the cockpit poll");
    let next = adapter.d2_decisions().expect("D2 projection");
    let next_ids: Vec<&str> = next
        .group("gate")
        .expect("gate group")
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    assert_eq!(
        next_ids,
        vec![second.id.as_str()],
        "the next approval Core raised must be listed and actionable"
    );
    assert_eq!(next.pending_total, 1);
}

/* ---- defects 14 and 15: abandoning an unanswered probe ---- */

#[test]
fn an_unanswered_probe_can_be_abandoned_so_the_next_command_is_accepted() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    // Core never answers: the host has no ACP binary and the handshake runs
    // out on Core's own single command worker.
    let client = TestCoreClient::new(multi_lane_view(), Arc::clone(&sent));
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().unwrap();

    let probe = adapter
        .send_d1_intent_and_wait(
            "gui-d1-probe",
            D1Intent::ProbeAgentAdapter {
                agent_id: "codex-acp".to_string(),
            },
            Duration::ZERO,
        )
        .expect("the probe is sent");
    assert_eq!(probe.pending_command_id.as_deref(), Some("gui-d1-probe"));

    // Every other D1 command is refused while it stands — which is exactly
    // what closed Lane creation in the shipped build.
    let blocked = adapter.send_d1_intent_and_wait(
        "gui-d1-preview",
        D1Intent::PreviewDefaultLane {
            preset: "coder".to_string(),
        },
        Duration::ZERO,
    );
    assert!(blocked.is_err(), "the one command slot is occupied");

    let released = adapter
        .abandon_pending_probe(Duration::ZERO)
        .expect("the probe is abandoned");
    assert_eq!(released.pending_command_id, None);
    assert_eq!(released.outcome.state, "idle");

    adapter
        .send_d1_intent_and_wait(
            "gui-d1-preview",
            D1Intent::PreviewDefaultLane {
                preset: "coder".to_string(),
            },
            Duration::ZERO,
        )
        .expect("the slot is free again");
}

#[test]
fn abandoning_refuses_to_drop_anything_but_a_probe() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = TestCoreClient::new(view_with_lane_owner(), Arc::clone(&sent));
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().unwrap();

    adapter
        .send_d1_intent_and_wait(
            "gui-d1-submit",
            D1Intent::Submit {
                lane_id: LANE.to_string(),
                content: "Edit the fixture README heading".to_string(),
            },
            Duration::ZERO,
        )
        .expect("the submit is sent");

    let result = adapter.abandon_pending_probe(Duration::ZERO);
    assert!(
        result.is_err(),
        "a command with an effect is never abandoned: its answer is the only record of what ran"
    );
    let still_pending = adapter.poll_d1(Some(LANE), Duration::ZERO).expect("poll");
    assert_eq!(
        still_pending.pending_command_id.as_deref(),
        Some("gui-d1-submit")
    );
}
