//! A streamed Agent reply must be visible while the turn is still running.
//!
//! This is the gap the GUI recorded as GUI-CORE-016. Core publishing ordered
//! chunk events is not enough on its own: the delta has to be scoped by the
//! session id the rest of the Agent facts use, or it grows a conversation the
//! cockpit never reads and the reply appears only when the turn ends.

use std::sync::{Arc, Mutex};

use viden_core::{
    AgentSessionStatus, AgentSessionView, LaneRuntimeOwnerBinding, RuntimeEvent, RuntimeEventKind,
    RuntimeOwner, RuntimeSnapshot, RuntimeViewState, RuntimeWireEvent,
};
use viden_gui::GuiCoreAdapter;

mod support;
use support::TestCoreClient;

const D1_FIXTURE: &str = include_str!(
    "../../../crates/types/tests/fixtures/frontend-contract-v1/d1-vertical-slice.json"
);

const LANE: &str = "lane_d1_core";
const SESSION: &str = "agent-session_17855";

fn owner() -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace_contract_v1".into(),
        project_id: "project_viden".into(),
        lane_id: Some(LANE.into()),
        session_id: Some(SESSION.into()),
        task_id: Some("task_d1_core".into()),
        turn_id: Some("turn_d1_core".into()),
    }
}

/// A cockpit view with one running Agent session on the selected Lane.
fn running_session_view() -> RuntimeViewState {
    let fixture: serde_json::Value = serde_json::from_str(D1_FIXTURE).expect("parse D1 fixture");
    let snapshot: RuntimeSnapshot =
        serde_json::from_value(fixture["initial_snapshot"].clone()).expect("typed snapshot");
    let mut view = RuntimeViewState::new(snapshot);
    for event in fixture["events"].as_array().expect("fixture events") {
        let envelope: viden_core::RuntimeEventEnvelope =
            serde_json::from_value(event.clone()).expect("typed envelope");
        if let RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
    }
    view.lane_runtime_owners.clear();
    view.apply_event(&RuntimeEvent::new(
        0,
        RuntimeEventKind::LaneRuntimeOwnerBound {
            binding: LaneRuntimeOwnerBinding {
                lane_id: LANE.into(),
                owner: owner(),
            },
        },
    ));
    view.apply_event(&RuntimeEvent::new(
        0,
        RuntimeEventKind::AgentSessionStarted {
            session: AgentSessionView {
                session_id: SESSION.into(),
                lane_id: LANE.into(),
                agent_id: "codex-acp".into(),
                model: None,
                status: AgentSessionStatus::Running,
                owner: owner(),
                task: "draw a cat".into(),
                diagnostic: None,
                output: None,
                control: None,
            },
        },
    ));
    view
}

fn delta(sequence: u64, session_id: &str, content: &str) -> RuntimeEvent {
    RuntimeEvent::new(
        sequence,
        RuntimeEventKind::AssistantDelta {
            message_id: format!("acp-message-{session_id}-turn-agent-input_1"),
            task_id: Some(format!("acp-session-{session_id}")),
            session_id: Some(session_id.to_string()),
            content: content.to_string(),
        },
    )
}

fn conversation(view: RuntimeViewState) -> Vec<(String, String)> {
    let mut adapter = GuiCoreAdapter::new(Box::new(TestCoreClient::new(
        view,
        Arc::new(Mutex::new(Vec::new())),
    )));
    adapter.connect().expect("connect");
    adapter
        .d1_cockpit(Some(LANE))
        .expect("cockpit projection")
        .agent_sessions
        .into_iter()
        .flat_map(|session| session.conversation)
        .map(|message| (message.role.to_string(), message.content))
        .collect()
}

#[test]
fn a_reply_is_readable_while_the_turn_is_still_running() {
    let mut view = running_session_view();
    for (sequence, chunk) in ["I will ", "draw ", "a cat."].iter().enumerate() {
        view.apply_event(&delta(sequence as u64 + 1, SESSION, chunk));
    }

    let rendered = conversation(view);
    assert!(
        rendered.contains(&("assistant".to_string(), "I will draw a cat.".to_string())),
        "the streamed reply must render before the session completes: {rendered:?}"
    );
    assert!(
        rendered
            .iter()
            .any(|(role, content)| role == "user" && content == "draw a cat"),
        "the prompt that started the turn must stay visible: {rendered:?}"
    );
}

#[test]
fn a_delta_scoped_to_another_session_never_joins_this_conversation() {
    let mut view = running_session_view();
    // The ACP protocol handle is not the session Core published. A delta
    // carrying it must not surface on this session's conversation.
    view.apply_event(&delta(1, "019fbc86-35da-7d33", "invisible"));

    let rendered = conversation(view);
    assert!(
        !rendered
            .iter()
            .any(|(_, content)| content.contains("invisible")),
        "a foreign session's delta leaked into the rendered conversation: {rendered:?}"
    );
}
