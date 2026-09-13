//! D16 Agent Board projection tests (`runtime.agent_session_control`, C14).
//!
//! Every state is replayed from the canonical `agent-session-control.json`
//! fixture rather than hand-built, so the board is measured against the wire
//! truth Core froze. The fixture's twenty-four events walk the whole
//! capability — pause with a pending approval denied, two held inputs, resume,
//! kill, retry, and the two status refusals — and the prefix each test replays
//! is named in its own body.

use std::sync::{Arc, Mutex};

use viden_core::{RuntimeCommand, RuntimeCommandEnvelope, RuntimeSnapshot, RuntimeViewState};
use viden_gui::GuiCoreAdapter;

mod support;
use support::TestCoreClient;

const CONTROL_FIXTURE: &str = include_str!(
    "../../../crates/types/tests/fixtures/frontend-contract-v1/agent-session-control.json"
);
/// A Lane-bearing fixture, for the "a Lane with a record" half of the
/// hierarchy: `agent-session-control` publishes a session whose Lane Core
/// publishes no record for, which is the other half.
const LANE_FIXTURE: &str =
    include_str!("../../../crates/types/tests/fixtures/frontend-contract-v1/multi-lane.json");

#[derive(serde::Deserialize)]
struct Fixture {
    initial_snapshot: RuntimeSnapshot,
    events: Vec<viden_core::RuntimeEventEnvelope>,
}

fn replay(fixture: &str, events: usize) -> RuntimeViewState {
    let fixture: Fixture = serde_json::from_str(fixture).unwrap();
    let mut view = RuntimeViewState::new(fixture.initial_snapshot);
    for envelope in fixture.events.into_iter().take(events) {
        if let viden_core::RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
    }
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

fn connected_recording(
    view: RuntimeViewState,
) -> (GuiCoreAdapter, Arc<Mutex<Vec<RuntimeCommandEnvelope>>>) {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = GuiCoreAdapter::new(Box::new(TestCoreClient::new(view, sent.clone())));
    adapter.connect().unwrap();
    (adapter, sent)
}

/// Core's own capability set minus `runtime.agent_session_control`.
fn connected_without_capability(view: RuntimeViewState) -> GuiCoreAdapter {
    let mut client = TestCoreClient::new(view, Arc::new(Mutex::new(Vec::new())));
    client
        .capabilities
        .remove(viden_gui::AGENT_SESSION_CONTROL_CAPABILITY);
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().unwrap();
    adapter
}

fn action<'a>(
    session: &'a viden_gui::D16SessionProjection,
    kind: &str,
) -> &'a viden_gui::D16ActionProjection {
    session
        .actions
        .iter()
        .find(|item| item.kind == kind)
        .unwrap_or_else(|| panic!("the action row is missing `{kind}`"))
}

/// Through event index 2 (prefix 3): the session is running and its approval was denied. The
/// board reads role/task/status from `AgentSessionView` and nests the session
/// under the Lane its own `lane_id` names.
#[test]
fn d16_nests_every_session_under_the_lane_that_owns_it() {
    let view = replay(CONTROL_FIXTURE, 3);
    let projection = connected(view).d16_agent_board(None).expect("D16");

    assert!(projection.capability_available);
    assert_eq!(projection.total_lanes, 1);
    assert_eq!(projection.total_sessions, 1);
    let lane = &projection.lanes[0];
    assert_eq!(lane.lane_id, "lane_session_control");
    // This fixture publishes no Lane *record*, only sessions that name the
    // Lane. The group is drawn anyway and says the record is absent, because
    // hoisting the session out of its Lane would be the larger lie.
    assert!(!lane.published);
    assert!(lane.summary.is_none());
    let session = &lane.sessions[0];
    assert_eq!(session.session_id, "session_control");
    assert_eq!(session.agent_id, "codex-acp");
    assert_eq!(session.model.as_deref(), Some("gpt-5-codex"));
    assert_eq!(session.status, "waiting_approval");
    assert!(session.awaits_human);
    assert!(!session.terminal);
    assert_eq!(projection.awaiting_total, 1);
}

/// A Lane Core published a record for carries its own facts on the row.
#[test]
fn d16_reads_the_lane_row_from_the_published_lane_record() {
    let fixture: Fixture = serde_json::from_str(LANE_FIXTURE).unwrap();
    let mut view = RuntimeViewState::new(fixture.initial_snapshot);
    for envelope in fixture.events {
        if let viden_core::RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
    }
    assert!(!view.lanes.is_empty());
    let projection = connected(view).d16_agent_board(None).expect("D16");
    let lane = projection
        .lanes
        .iter()
        .find(|lane| lane.published)
        .expect("multi-lane publishes Lane records");
    assert!(lane.summary.is_some());
    assert!(lane.route.is_some());
    assert!(lane.gate_strength.is_some());
    assert!(lane.status.is_some());
}

/// Through event index 5 (prefix 6): paused. The control record travels on the facts of the
/// turn, so the board reads `paused_at` and the held-input count from Core
/// rather than counting anything itself.
#[test]
fn d16_reads_the_control_record_and_the_held_input_count() {
    let paused = connected(replay(CONTROL_FIXTURE, 6))
        .d16_agent_board(Some("session_control"))
        .expect("D16");
    let session = &paused.lanes[0].sessions[0];
    assert_eq!(session.status, "paused");
    let control = session
        .control
        .as_ref()
        .expect("a paused session is controlled");
    assert!(control.paused_at.is_some());
    assert_eq!(control.held_input_count, 0);

    // Through event index 11 (prefix 12): two inputs accepted while paused and held.
    let held = connected(replay(CONTROL_FIXTURE, 12))
        .d16_agent_board(Some("session_control"))
        .expect("D16");
    let session = &held.lanes[0].sessions[0];
    assert_eq!(session.status, "paused");
    assert_eq!(session.control.as_ref().unwrap().held_input_count, 2);
}

/// The action row is Core's acceptance rules, precomputed. A control the
/// status does not allow carries the exact `session_status_<current>` string
/// Core would have answered with, so the label read before pressing and the
/// refusal read after are one sentence.
#[test]
fn d16_disables_each_control_with_the_status_reason_core_would_answer() {
    let paused = connected(replay(CONTROL_FIXTURE, 6))
        .d16_agent_board(Some("session_control"))
        .unwrap();
    let session = &paused.lanes[0].sessions[0];
    assert!(!action(session, "pause").available);
    assert_eq!(
        action(session, "pause").block.as_deref(),
        Some("session_status_paused")
    );
    assert!(action(session, "resume").available);
    assert!(action(session, "kill").available);
    assert!(action(session, "stop").available);
    assert!(!action(session, "retry").available);

    let running = connected(replay(CONTROL_FIXTURE, 1))
        .d16_agent_board(Some("session_control"))
        .unwrap();
    let session = &running.lanes[0].sessions[0];
    assert!(action(session, "pause").available);
    assert!(!action(session, "resume").available);
    assert_eq!(
        action(session, "resume").block.as_deref(),
        Some("session_status_running")
    );
}

/// Through event index 19 (prefix 20): killed. `Killed` is terminal and Retry accepts it exactly
/// as it accepts `Failed`, which is the whole point of the new status.
#[test]
fn d16_offers_retry_on_a_killed_session_and_refuses_the_live_controls() {
    let killed = connected(replay(CONTROL_FIXTURE, 20))
        .d16_agent_board(Some("session_control"))
        .unwrap();
    let session = &killed.lanes[0].sessions[0];
    assert_eq!(session.status, "killed");
    assert!(session.terminal);
    assert_eq!(
        session.control.as_ref().unwrap().terminated_by.as_deref(),
        Some("killed")
    );
    assert!(action(session, "retry").available);
    assert!(!action(session, "pause").available);
    assert!(!action(session, "kill").available);
    assert!(!action(session, "stop").available);
    assert_eq!(
        action(session, "stop").block.as_deref(),
        Some(viden_gui::D16_SESSION_ENDED_CODE)
    );
    // The switch is a live-session action: there is nothing left to end.
    assert!(!action(session, "switch_model").available);
    assert_eq!(
        action(session, "switch_model").block.as_deref(),
        Some(viden_gui::D16_SESSION_ENDED_CODE)
    );
}

/// Retry is not offered on a completed session: there is nothing to retry, and
/// the row says which of the two terminal reasons applies.
#[test]
fn d16_names_completion_rather_than_a_status_refusal_on_retry() {
    // Prefix 17: event index 16 completed the session, and index 17 is the
    // retry that restarts it, so this prefix is the completed state exactly.
    let completed = connected(replay(CONTROL_FIXTURE, 17))
        .d16_agent_board(Some("session_control"))
        .unwrap();
    let session = &completed.lanes[0].sessions[0];
    assert_eq!(session.status, "completed");
    assert!(!action(session, "retry").available);
    assert_eq!(
        action(session, "retry").block.as_deref(),
        Some(viden_gui::D16_SESSION_COMPLETED_CODE)
    );
}

/// The two controls with no Core fact are drawn as requests, not as timing
/// failures: `D-PLANBOARD` forbids drawing them as usable and the projection
/// is where that is decided.
#[test]
fn d16_marks_downgrade_and_ask_user_as_requests_with_no_core_fact() {
    let projection = connected(replay(CONTROL_FIXTURE, 3))
        .d16_agent_board(None)
        .unwrap();
    let session = &projection.lanes[0].sessions[0];
    for kind in ["downgrade_manual", "ask_user"] {
        let item = action(session, kind);
        assert!(!item.available);
        assert!(item.request);
        assert_eq!(
            item.block.as_deref(),
            Some(viden_gui::D16_NO_CORE_FACT_CODE)
        );
    }
}

/// Without the capability the board still reads — Lanes and sessions are
/// ordinary `0.3.0` facts — and every capability-backed control is disabled and
/// names the capability. Stop and Retry predate C14 and stay live.
#[test]
fn d16_disables_the_capability_controls_and_keeps_the_board_readable() {
    let projection = connected_without_capability(replay(CONTROL_FIXTURE, 3))
        .d16_agent_board(None)
        .unwrap();
    assert!(!projection.capability_available);
    assert_eq!(projection.total_sessions, 1);
    let session = &projection.lanes[0].sessions[0];
    for kind in ["pause", "resume", "kill"] {
        assert!(!action(session, kind).available);
        assert_eq!(
            action(session, kind).block.as_deref(),
            Some(viden_gui::D16_CAPABILITY_ABSENT_CODE),
            "{kind} must name the capability"
        );
    }
    assert!(action(session, "stop").available);
}

/// A selection naming a session Core no longer publishes falls back to the row
/// that blocks on a human, never to a session that is gone.
#[test]
fn d16_resolves_a_stale_selection_against_what_core_publishes() {
    let projection = connected(replay(CONTROL_FIXTURE, 3))
        .d16_agent_board(Some("session_that_left"))
        .unwrap();
    assert_eq!(
        projection.selected_session_id.as_deref(),
        Some("session_control")
    );
    let detail = projection
        .detail
        .expect("a resolved selection has a detail");
    assert_eq!(detail.session_id, "session_control");
    assert_eq!(detail.lane_id, "lane_session_control");
}

/// The switch panel is the two steps it really is: it names the session the
/// second step loads, and the projection offers no single switch command.
#[test]
fn d16_describes_switch_model_as_a_two_step_flow() {
    let projection = connected(replay(CONTROL_FIXTURE, 1))
        .d16_agent_board(Some("session_control"))
        .unwrap();
    let detail = projection.detail.expect("detail");
    let switch = detail.switch_model.expect("a live session can switch");
    assert_eq!(switch.load_session_id, "session_control");
    assert_eq!(switch.current_model.as_deref(), Some("gpt-5-codex"));
    assert_eq!(switch.agent_id, "codex-acp");
    assert!(detail.switch_model_block.is_none());
}

/// Pause sends Core's own command under the owner Core published on the
/// session, and nothing else.
#[test]
fn d16_sends_pause_under_the_owner_core_published() {
    let view = replay(CONTROL_FIXTURE, 1);
    let owner = view.agent_sessions[0].owner.clone();
    let (mut adapter, sent) = connected_recording(view);
    adapter
        .send_d16_intent_and_wait(
            "gui-board-pause-1",
            viden_gui::D16Intent::Pause {
                session_id: "session_control".to_string(),
            },
            std::time::Duration::ZERO,
        )
        .expect("pause is sent");
    let sent = sent.lock().unwrap();
    let envelope = sent.last().expect("one command");
    assert_eq!(envelope.command_id, "gui-board-pause-1");
    assert_eq!(envelope.owner, owner);
    assert!(matches!(
        &envelope.command,
        RuntimeCommand::PauseAgentSession { session_id } if session_id == "session_control"
    ));
}

/// Step 2 of the switch is `StartAgentSession` with `load_session_id` set to
/// the ended session, so the new session keeps the transcript.
#[test]
fn d16_starts_the_replacement_session_loading_the_old_one() {
    let view = replay(CONTROL_FIXTURE, 1);
    let (mut adapter, sent) = connected_recording(view);
    adapter
        .send_d16_intent_and_wait(
            "gui-board-start-1",
            viden_gui::D16Intent::Start {
                session_id: "session_control".to_string(),
                model: Some("claude-sonnet-4.6".to_string()),
            },
            std::time::Duration::ZERO,
        )
        .expect("start is sent");
    let sent = sent.lock().unwrap();
    let RuntimeCommand::StartAgentSession { request } = &sent.last().unwrap().command else {
        panic!("step 2 is StartAgentSession");
    };
    assert_eq!(request.load_session_id.as_deref(), Some("session_control"));
    assert_eq!(request.model.as_deref(), Some("claude-sonnet-4.6"));
    assert_eq!(request.lane_id, "lane_session_control");
    assert_eq!(request.agent_id, "codex-acp");
}

/// A control for a session Core does not publish is refused locally rather
/// than sent as a command Core would have to reject.
#[test]
fn d16_refuses_a_control_on_a_session_core_does_not_publish() {
    let mut adapter = connected(replay(CONTROL_FIXTURE, 1));
    let error = adapter
        .send_d16_intent_and_wait(
            "gui-board-kill-1",
            viden_gui::D16Intent::Kill {
                session_id: "session_missing".to_string(),
            },
            std::time::Duration::ZERO,
        )
        .expect_err("an unpublished session is refused locally");
    assert!(error.contains("session_missing"));
}

/// Pause, Resume and Kill need the capability; Stop and Retry predate it and
/// must not be retired by its absence.
#[test]
fn d16_sends_no_capability_control_without_the_capability() {
    let mut adapter = connected_without_capability(replay(CONTROL_FIXTURE, 1));
    let error = adapter
        .send_d16_intent_and_wait(
            "gui-board-pause-2",
            viden_gui::D16Intent::Pause {
                session_id: "session_control".to_string(),
            },
            std::time::Duration::ZERO,
        )
        .expect_err("no capability means nothing is sent");
    assert!(error.contains(viden_gui::AGENT_SESSION_CONTROL_CAPABILITY));

    adapter
        .send_d16_intent_and_wait(
            "gui-board-stop-1",
            viden_gui::D16Intent::Stop {
                session_id: "session_control".to_string(),
            },
            std::time::Duration::ZERO,
        )
        .expect("cancel has worked since 0.3.0 and is not gated on C14");
}

/// A workspace with no Lane and no session is absence, not an empty board with
/// a placeholder agent.
#[test]
fn d16_reports_no_lane_rather_than_an_invented_row() {
    let fixture: Fixture = serde_json::from_str(CONTROL_FIXTURE).unwrap();
    let view = RuntimeViewState::new(fixture.initial_snapshot);
    let projection = connected(view).d16_agent_board(None).expect("D16");
    assert!(projection.lanes.is_empty());
    assert_eq!(projection.total_sessions, 0);
    assert!(projection.selected_session_id.is_none());
    assert!(projection.detail.is_none());
}
