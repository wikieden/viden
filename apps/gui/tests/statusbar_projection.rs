//! The cockpit statusbar is computed by the host from the confirmed Core
//! view. Every segment is a published fact; an absent fact projects as `None`
//! so the frontend renders an explicit placeholder, never an invented number.

use std::sync::{Arc, Mutex};

use viden_core::{
    RuntimeEvent, RuntimeEventEnvelope, RuntimeEventKind, RuntimeSnapshot, RuntimeViewState,
    RuntimeWireEvent,
};
use viden_gui::GuiCoreAdapter;

mod support;
use support::TestCoreClient;

const D1_FIXTURE: &str = include_str!(
    "../../../crates/types/tests/fixtures/frontend-contract-v1/d1-vertical-slice.json"
);

#[derive(serde::Deserialize)]
struct Fixture {
    initial_snapshot: RuntimeSnapshot,
    events: Vec<RuntimeEventEnvelope>,
}

fn d1_view() -> RuntimeViewState {
    let fixture: Fixture = serde_json::from_str(D1_FIXTURE).expect("parse D1 fixture");
    let mut view = RuntimeViewState::new(fixture.initial_snapshot);
    for envelope in fixture.events {
        if let RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
    }
    view
}

fn connected(view: RuntimeViewState) -> GuiCoreAdapter {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = GuiCoreAdapter::new(Box::new(TestCoreClient::new(view, sent)));
    adapter.connect().expect("connect statusbar client");
    adapter
}

#[test]
fn statusbar_projects_only_published_core_facts() {
    let adapter = connected(d1_view());
    let projection = adapter
        .d1_cockpit(Some("lane_d1_core"))
        .expect("D1 projection");
    let statusbar = &projection.statusbar;

    assert_eq!(statusbar.work_mode, "build");
    assert_eq!(statusbar.permission_level, "ask");

    // The fixture publishes a token-cost fact but no provider health and no
    // context budget: those segments stay absent instead of fabricated.
    let tokens = statusbar.tokens.as_ref().expect("token cost is published");
    assert_eq!(tokens.input_tokens, 100);
    assert_eq!(tokens.output_tokens, 25);
    assert!(statusbar.latency.is_none());
    assert!(statusbar.requests.is_none());
    assert!(statusbar.context.is_none());

    // One runtime error is the fixture's live diagnostic. The decision count
    // is zero: the fixture's approval was already resolved and its one open
    // merge gate is decided in D12, which this chip does not open (G7 made the
    // count the D2 queue's own total — see `pending_decision_count`).
    assert_eq!(statusbar.diagnostics_count, 1);
    assert_eq!(statusbar.pending_decision_count, 0);

    // The event segment is the replay-cursor stream position, not a counter.
    assert_eq!(statusbar.event_stream_position, 0);

    let lane = statusbar.lane.as_ref().expect("selected Lane is published");
    assert_eq!(lane.lane_id, "lane_d1_core");
    assert_eq!(lane.status, "running");
    assert_eq!(lane.progress, Some(40));
    // No agent session is bound to the Lane, so the agent stays unnamed.
    assert_eq!(lane.agent_id, None);
}

#[test]
fn statusbar_projects_provider_and_context_segments_when_core_publishes_them() {
    let mut view = d1_view();
    let provider: RuntimeEventKind = serde_json::from_value(serde_json::json!({
        "type": "provider_health_updated",
        "payload": {
            "provider": {
                "provider_id": "deepseek",
                "model": "deepseek-v4-flash",
                "status": "ready",
                "request_count": 128,
                "error_count": 2,
                "last_latency_ms": 840,
                "average_latency_ms": 1100,
                "tokens_per_second": 30
            }
        }
    }))
    .expect("typed provider event");
    let budget: RuntimeEventKind = serde_json::from_value(serde_json::json!({
        "type": "context_budget_exceeded",
        "payload": {
            "budget": {
                "budget_id": "ctx-main",
                "scope": { "type": "task", "id": "task_d1_core" },
                "soft_token_limit": 96000,
                "hard_token_limit": 128000,
                "used_tokens": 42100,
                "remaining_tokens": 85900,
                "exceeded": false,
                "updated_at": 7
            }
        }
    }))
    .expect("typed budget event");
    view.apply_event(&RuntimeEvent::new(90, provider));
    view.apply_event(&RuntimeEvent::new(91, budget));

    let adapter = connected(view);
    let statusbar = adapter
        .d1_cockpit(Some("lane_d1_core"))
        .expect("D1 projection")
        .statusbar;

    let latency = statusbar.latency.expect("latency is published");
    assert_eq!(latency.last_latency_ms, Some(840));
    assert_eq!(latency.average_latency_ms, Some(1100));
    let requests = statusbar.requests.expect("requests are published");
    assert_eq!(requests.request_count, 128);
    assert_eq!(requests.error_count, 2);
    let context = statusbar.context.expect("budget is published");
    assert_eq!(context.used_tokens, 42_100);
    assert_eq!(context.hard_token_limit, 128_000);
    assert!(!context.exceeded);
}

#[test]
fn statusbar_names_the_lane_agent_only_when_exactly_one_session_is_bound() {
    let mut view = d1_view();
    let session: RuntimeEventKind = serde_json::from_value(serde_json::json!({
        "type": "agent_session_started",
        "payload": {
            "session": {
                "session_id": "session_statusbar",
                "lane_id": "lane_d1_core",
                "agent_id": "codex-acp",
                "model": null,
                "status": "running",
                "owner": {
                    "workspace_id": "workspace_contract_v1",
                    "project_id": "project_viden",
                    "lane_id": "lane_d1_core",
                    "session_id": "session_statusbar",
                    "task_id": null,
                    "turn_id": null
                },
                "task": "statusbar coverage",
                "diagnostic": null,
                "output": null
            }
        }
    }))
    .expect("typed session event");
    view.apply_event(&RuntimeEvent::new(92, session));

    let adapter = connected(view);
    let statusbar = adapter
        .d1_cockpit(Some("lane_d1_core"))
        .expect("D1 projection")
        .statusbar;
    let lane = statusbar.lane.expect("selected Lane is published");
    assert_eq!(lane.agent_id.as_deref(), Some("codex-acp"));
}
