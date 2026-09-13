use std::sync::{Arc, Mutex};

use viden_core::{
    AgentSessionStatus, AgentSessionView, AgentTaskStatus, LaneStatus, RuntimeCommand,
    RuntimeErrorView, RuntimeEvent, RuntimeEventKind, RuntimeOwner, RuntimeSnapshot,
    RuntimeViewState,
};
use viden_gui::{D6Intent, D6State, GuiCoreAdapter};

mod support;
use support::TestCoreClient;

const D1_FIXTURE: &str = include_str!(
    "../../../crates/types/tests/fixtures/frontend-contract-v1/d1-vertical-slice.json"
);

fn d1_view() -> RuntimeViewState {
    #[derive(serde::Deserialize)]
    struct Fixture {
        initial_snapshot: RuntimeSnapshot,
        events: Vec<viden_core::RuntimeEventEnvelope>,
    }
    let fixture: Fixture = serde_json::from_str(D1_FIXTURE).unwrap();
    let mut view = RuntimeViewState::new(fixture.initial_snapshot);
    for envelope in fixture.events {
        if let viden_core::RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
    }
    view
}

fn connected(view: RuntimeViewState) -> GuiCoreAdapter {
    connected_recording(view, Arc::new(Mutex::new(Vec::new())))
}

fn connected_recording(
    view: RuntimeViewState,
    sent: Arc<Mutex<Vec<viden_core::RuntimeCommandEnvelope>>>,
) -> GuiCoreAdapter {
    let mut adapter = GuiCoreAdapter::new(Box::new(TestCoreClient::new(view, sent)));
    adapter.connect().unwrap();
    adapter
}

/// Adds the Lane-bound ACP session D6 needs to name a restart target.
fn with_bound_session(
    view: &mut RuntimeViewState,
    lane_id: &str,
    session_id: &str,
    status: AgentSessionStatus,
) {
    view.agent_sessions.push(AgentSessionView {
        session_id: session_id.to_string(),
        lane_id: lane_id.to_string(),
        agent_id: "codex-acp".to_string(),
        model: None,
        status,
        owner: RuntimeOwner {
            lane_id: Some(lane_id.to_string()),
            session_id: Some(session_id.to_string()),
            ..RuntimeOwner::default()
        },
        task: "restore the failed turn".to_string(),
        diagnostic: None,
        output: None,
        control: None,
    });
}

fn action<'a>(
    projection: &'a viden_gui::D6RecoveryProjection,
    kind: &str,
) -> &'a viden_gui::D6ActionProjection {
    projection
        .actions
        .iter()
        .find(|action| action.kind == kind)
        .expect("D6 always projects every recovery action")
}

#[test]
fn d6_projects_empty_provider_agent_context_and_queue_clear_from_core_facts() {
    let mut empty = d1_view();
    empty.lanes.clear();
    empty.tasks.clear();
    empty.pending_approvals.clear();
    assert_eq!(connected(empty).d6_recovery().state, D6State::Empty);

    let mut provider = d1_view();
    let provider_error: RuntimeEventKind = serde_json::from_value(serde_json::json!({
        "type": "provider_health_updated",
        "payload": {
            "provider": {
                "provider_id": "deepseek",
                "model": "deepseek-v4-flash",
                "status": "error",
                "request_count": 2,
                "error_count": 1,
                "last_latency_ms": 300,
                "average_latency_ms": 240,
                "tokens_per_second": 20
            }
        }
    }))
    .unwrap();
    provider.apply_event(&RuntimeEvent::new(98, provider_error));
    provider.errors.push(RuntimeErrorView {
        message: "provider unavailable".into(),
        recoverable: true,
        hint: Some("select another provider".into()),
    });
    let projection = connected(provider).d6_recovery();
    assert_eq!(projection.state, D6State::ProviderError);
    assert_eq!(projection.detail.as_deref(), Some("provider unavailable"));

    let mut stopped = d1_view();
    stopped.tasks[0].status = AgentTaskStatus::Failed;
    let projection = connected(stopped).d6_recovery();
    assert_eq!(projection.state, D6State::AgentStopped);
    // A failed task is not a retryable ACP session, so restart stays closed
    // even though Core reports stopped execution.
    assert!(!action(&projection, "restart").available);
    assert!(!action(&projection, "checkpoint").available);
    assert!(
        projection
            .actions
            .iter()
            .any(|action| action.code == "GUI-CORE-003")
    );

    let mut overflow = d1_view();
    let budget: RuntimeEventKind = serde_json::from_value(serde_json::json!({
        "type": "context_budget_exceeded",
        "payload": {
            "budget": {
                "budget_id": "ctx-hard",
                "scope": { "type": "task", "id": "task-core" },
                "soft_token_limit": 8000,
                "hard_token_limit": 10000,
                "used_tokens": 10001,
                "remaining_tokens": 0,
                "exceeded": true,
                "updated_at": 1
            }
        }
    }))
    .unwrap();
    overflow.apply_event(&RuntimeEvent::new(99, budget));
    let projection = connected(overflow).d6_recovery();
    assert_eq!(projection.state, D6State::ContextOverflow);
    assert_eq!(projection.used_tokens, Some(10_001));
    assert_eq!(projection.hard_token_limit, Some(10_000));

    let mut clear = d1_view();
    clear.pending_approvals.clear();
    clear.merge_gates.clear();
    clear.errors.clear();
    clear.context_budgets.clear();
    clear
        .tasks
        .retain(|task| task.status != AgentTaskStatus::Failed);
    assert_eq!(
        connected(clear).d6_recovery().state,
        D6State::GateQueueClear
    );

    let mut open_gate = d1_view();
    open_gate.pending_approvals.clear();
    open_gate.merge_gates.push(
        serde_json::from_value(serde_json::json!({
            "gate_id": "gate-open",
            "task_id": "task-core",
            "status": "collecting_evidence",
            "required_evidence": ["test_result"],
            "evidence_ids": []
        }))
        .unwrap(),
    );
    assert_ne!(
        connected(open_gate).d6_recovery().state,
        D6State::GateQueueClear
    );
}

#[test]
fn d6_checkpoint_stays_fail_closed_because_schema_one_has_no_checkpoint_capability() {
    let mut stopped = d1_view();
    stopped.tasks[0].status = AgentTaskStatus::Failed;
    with_bound_session(
        &mut stopped,
        "lane_d1_core",
        "session_lane_d1_core",
        AgentSessionStatus::Failed,
    );
    let projection = connected(stopped).d6_recovery();
    let checkpoint = action(&projection, "checkpoint");
    assert!(!checkpoint.available);
    assert_eq!(checkpoint.code, "GUI-CORE-003");
    assert_eq!(checkpoint.session_id, None);
    assert_eq!(checkpoint.lane_id, None);
}

#[test]
fn d6_restart_names_the_exact_lane_bound_session_core_reports_failed() {
    let mut failed = d1_view();
    with_bound_session(
        &mut failed,
        "lane_d1_core",
        "session_lane_d1_core",
        AgentSessionStatus::Failed,
    );
    let projection = connected(failed).d6_recovery();
    let restart = action(&projection, "restart");
    assert!(restart.available);
    assert_eq!(restart.session_id.as_deref(), Some("session_lane_d1_core"));
    assert_eq!(restart.lane_id.as_deref(), Some("lane_d1_core"));

    // A running session is not retryable, matching D1's own retry rule.
    let mut running = d1_view();
    with_bound_session(
        &mut running,
        "lane_d1_core",
        "session_lane_d1_core",
        AgentSessionStatus::Running,
    );
    let projection = connected(running).d6_recovery();
    assert!(!action(&projection, "restart").available);
    assert_eq!(action(&projection, "restart").session_id, None);

    // Two retryable sessions are an ambiguous target; D6 must not choose one.
    let mut ambiguous = d1_view();
    with_bound_session(
        &mut ambiguous,
        "lane_d1_core",
        "session_lane_d1_core",
        AgentSessionStatus::Failed,
    );
    with_bound_session(
        &mut ambiguous,
        "lane_d1_core",
        "session_second",
        AgentSessionStatus::Cancelled,
    );
    let projection = connected(ambiguous).d6_recovery();
    assert!(!action(&projection, "restart").available);
}

#[test]
fn d6_close_lane_names_the_exact_active_lane_and_fails_closed_when_ambiguous() {
    let projection = connected(d1_view()).d6_recovery();
    let close_lane = action(&projection, "close_lane");
    assert!(close_lane.available);
    assert_eq!(close_lane.lane_id.as_deref(), Some("lane_d1_core"));

    let mut none_active = d1_view();
    for lane in &mut none_active.lanes {
        lane.status = LaneStatus::Done;
    }
    let projection = connected(none_active).d6_recovery();
    assert!(!action(&projection, "close_lane").available);
    assert_eq!(action(&projection, "close_lane").lane_id, None);

    let mut ambiguous = d1_view();
    let mut second = ambiguous.lanes[0].clone();
    second.id = "lane_d1_second".to_string();
    ambiguous.lanes.push(second);
    let projection = connected(ambiguous).d6_recovery();
    assert!(!action(&projection, "close_lane").available);
}

#[test]
fn d6_send_intent_dispatches_the_core_lane_and_session_commands() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut failed = d1_view();
    with_bound_session(
        &mut failed,
        "lane_d1_core",
        "session_lane_d1_core",
        AgentSessionStatus::Failed,
    );
    let mut adapter = connected_recording(failed, Arc::clone(&sent));

    let restart = adapter
        .send_d6_intent_and_wait(
            "gui-d6-restart",
            D6Intent::Restart {
                session_id: "session_lane_d1_core".to_string(),
            },
            std::time::Duration::ZERO,
        )
        .expect("restart dispatches the Core retry command");
    assert_eq!(
        restart.pending_command_id.as_deref(),
        Some("gui-d6-restart")
    );
    assert!(
        restart
            .projection
            .actions
            .iter()
            .any(|a| a.kind == "restart")
    );

    adapter
        .send_d6_intent_and_wait(
            "gui-d6-close",
            D6Intent::CloseLane {
                lane_id: "lane_d1_core".to_string(),
            },
            std::time::Duration::ZERO,
        )
        .expect("close lane dispatches the Core stop command");

    let commands = sent.lock().expect("sent commands");
    let kinds = commands
        .iter()
        .map(|envelope| envelope.command.clone())
        .collect::<Vec<_>>();
    assert!(matches!(
        kinds.first(),
        Some(RuntimeCommand::RetryAgentSession { session_id }) if session_id == "session_lane_d1_core"
    ));
    assert!(matches!(
        kinds.get(1),
        Some(RuntimeCommand::StopLane { lane_id }) if lane_id == "lane_d1_core"
    ));
    assert_eq!(
        commands[0].owner.lane_id.as_deref(),
        Some("lane_d1_core"),
        "every D6 command replays the exact Core owner"
    );
}

#[test]
fn d6_send_intent_refuses_a_target_core_has_not_published() {
    let mut adapter = connected(d1_view());
    let error = adapter
        .send_d6_intent_and_wait(
            "gui-d6-restart",
            D6Intent::Restart {
                session_id: "session_unknown".to_string(),
            },
            std::time::Duration::ZERO,
        )
        .expect_err("an unpublished session is not a restart target");
    assert!(error.contains("session_unknown"), "{error}");

    let error = adapter
        .send_d6_intent_and_wait(
            "gui-d6-close",
            D6Intent::CloseLane {
                lane_id: "lane_unknown".to_string(),
            },
            std::time::Duration::ZERO,
        )
        .expect_err("an unpublished Lane is not a close target");
    assert!(error.contains("lane_unknown"), "{error}");
}
