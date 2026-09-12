//! The client side of `runtime.turn_lifecycle` (C6).
//!
//! Before this capability the cockpit read liveness from display residue: the
//! composer was busy when the Lane's owner binding happened to carry a
//! `turn_id`. That is a fact about an owner, not about work, and it is why the
//! composer kept queueing after a turn had ended. Four rules replace it:
//!
//! 1. **Busy is an `active_turns` entry for this composer's target.** A Lane
//!    composer is busy for a turn whose owner names that Lane (or a running
//!    Agent session); the session-scoped composer is busy for a turn whose
//!    owner names no Lane. Nothing is inferred from an owner's `turn_id`.
//! 2. **A turn names its own source.** Typed, queued, and agent are three
//!    different explanations for text appearing in the transcript, and the
//!    strip says which.
//! 3. **Elapsed comes from Core's `started_at`.** The client-observed clock
//!    could only ever measure since this webview noticed.
//! 4. **A failed turn's reason is kept.** `TurnFinished { Failed }` removes the
//!    turn from `active_turns`, so the reason is captured from the event or it
//!    is lost — and a composer that went quiet with no sentence is the state
//!    the operator cannot act on.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use viden_core::{
    EventCursor, FRONTEND_SCHEMA_V1, LaneRuntimeOwnerBinding, RuntimeCommandEnvelope, RuntimeEvent,
    RuntimeEventEnvelope, RuntimeEventKind, RuntimeOwner, RuntimeSnapshot, RuntimeViewState,
    RuntimeWireEvent, TurnOutcome, TurnSource, TurnView,
};
use viden_gui::GuiCoreAdapter;

mod support;
use support::TestCoreClient;

const TIMEOUT: Duration = Duration::from_millis(10);
const LANE: &str = "lane_core";

fn view() -> RuntimeViewState {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../crates/types/tests/fixtures/frontend-contract-v1/multi-lane.json"
    ))
    .expect("fixture json");
    let snapshot: RuntimeSnapshot =
        serde_json::from_value(fixture["initial_snapshot"].clone()).expect("fixture snapshot");
    let mut view = RuntimeViewState::new(snapshot);
    let envelopes: Vec<RuntimeEventEnvelope> =
        serde_json::from_value(fixture["events"].clone()).expect("fixture events");
    for envelope in envelopes {
        if let RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
    }
    // The cockpit only has a composer once Core bound an exact owner to the
    // selected Lane, so the binding is part of the baseline rather than of
    // any one test.
    view.lane_runtime_owners.push(LaneRuntimeOwnerBinding {
        lane_id: LANE.to_string(),
        owner: lane_owner(None),
    });
    view
}

/// The Lane's exact owner. `turn_id` is a parameter on purpose: the residue
/// predicate this batch retires read exactly that field.
fn lane_owner(turn_id: Option<&str>) -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden".to_string(),
        lane_id: Some(LANE.to_string()),
        session_id: Some("session_multi-lane".to_string()),
        task_id: Some("task_core".to_string()),
        turn_id: turn_id.map(str::to_string),
    }
}

fn session_owner() -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden".to_string(),
        lane_id: None,
        session_id: Some("session_multi-lane".to_string()),
        task_id: None,
        turn_id: None,
    }
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

fn started(
    sequence: u64,
    turn_id: &str,
    owner: RuntimeOwner,
    source: TurnSource,
) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::TurnStarted {
            turn: TurnView {
                turn_id: turn_id.to_string(),
                owner,
                source,
                started_at: 1_700_000_500,
            },
        },
    )
}

fn finished(
    sequence: u64,
    turn_id: &str,
    owner: RuntimeOwner,
    outcome: TurnOutcome,
) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::TurnFinished {
            turn_id: turn_id.to_string(),
            owner,
            outcome,
            finished_at: 1_700_000_900,
        },
    )
}

fn connected(view: RuntimeViewState, events: Vec<RuntimeEventEnvelope>) -> GuiCoreAdapter {
    let sent = Arc::new(Mutex::new(Vec::<RuntimeCommandEnvelope>::new()));
    let mut client = TestCoreClient::new(view, sent);
    for event in events {
        client = client.with_envelope(event);
    }
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect");
    adapter
}

#[test]
fn a_lane_composer_is_busy_for_a_turn_core_scoped_to_that_lane() {
    let mut adapter = connected(
        view(),
        vec![started(
            1,
            "turn-lane-1",
            lane_owner(Some("turn-lane-1")),
            TurnSource::UserInput,
        )],
    );
    adapter.poll_d1(Some(LANE), TIMEOUT).expect("poll");
    let projection = adapter.d1_cockpit(Some(LANE)).expect("D1");
    assert!(projection.composer.busy, "Core says a turn is running");
    assert!(!projection.composer.can_submit_immediately);

    let turn = projection
        .active_turns
        .iter()
        .find(|turn| turn.turn_id == "turn-lane-1")
        .expect("the turn is projected");
    assert_eq!(turn.lane_id.as_deref(), Some(LANE));
    assert_eq!(turn.source, "user_input");
    assert_eq!(turn.started_at, 1_700_000_500);
}

#[test]
fn a_finished_turn_leaves_the_composer_idle_even_with_an_owner_carrying_a_turn_id() {
    // The retired predicate read `owner.turn_id`, which a binding keeps after
    // the work ends. The view's binding carries one here on purpose.
    let mut view = view();
    view.lane_runtime_owners.clear();
    view.lane_runtime_owners.push(LaneRuntimeOwnerBinding {
        lane_id: LANE.to_string(),
        owner: lane_owner(Some("turn-lane-1")),
    });
    let mut adapter = connected(
        view,
        vec![
            started(
                1,
                "turn-lane-1",
                lane_owner(Some("turn-lane-1")),
                TurnSource::UserInput,
            ),
            finished(
                2,
                "turn-lane-1",
                lane_owner(Some("turn-lane-1")),
                TurnOutcome::Completed,
            ),
        ],
    );
    adapter.poll_d1(Some(LANE), TIMEOUT).expect("poll");
    adapter.poll_d1(Some(LANE), TIMEOUT).expect("poll");
    let projection = adapter.d1_cockpit(Some(LANE)).expect("D1");
    assert!(
        projection.active_turns.is_empty(),
        "Core removed the turn it finished"
    );
    assert!(
        !projection.composer.busy,
        "liveness is Core's fact, not the owner's residue"
    );
}

#[test]
fn a_session_scoped_turn_never_makes_a_lane_composer_busy() {
    let mut adapter = connected(
        view(),
        vec![started(
            1,
            "turn-session-1",
            session_owner(),
            TurnSource::QueuedInput {
                input_id: "input-1".to_string(),
            },
        )],
    );
    adapter.poll_d1(Some(LANE), TIMEOUT).expect("poll");
    let projection = adapter.d1_cockpit(Some(LANE)).expect("D1");
    assert!(
        !projection.composer.busy,
        "a session-scoped turn is not this Lane's work"
    );
    let turn = projection
        .active_turns
        .first()
        .expect("the turn is still projected, because the strip names it");
    assert_eq!(turn.lane_id, None);
    assert_eq!(turn.source, "queued_input");
    assert_eq!(turn.source_input_id.as_deref(), Some("input-1"));
}

#[test]
fn a_failed_turn_keeps_cores_reason_after_the_turn_is_gone() {
    let mut adapter = connected(
        view(),
        vec![
            started(
                1,
                "turn-lane-2",
                lane_owner(Some("turn-lane-2")),
                TurnSource::UserInput,
            ),
            finished(
                2,
                "turn-lane-2",
                lane_owner(Some("turn-lane-2")),
                TurnOutcome::failed("the provider refused the request"),
            ),
        ],
    );
    adapter.poll_d1(Some(LANE), TIMEOUT).expect("poll");
    adapter.poll_d1(Some(LANE), TIMEOUT).expect("poll");
    let projection = adapter.d1_cockpit(Some(LANE)).expect("D1");
    let failure = projection
        .turn_failure
        .as_ref()
        .expect("the reason survives the turn");
    assert_eq!(failure.turn_id, "turn-lane-2");
    assert_eq!(failure.lane_id.as_deref(), Some(LANE));
    assert_eq!(failure.outcome, "failed");
    assert_eq!(
        failure.reason.as_deref(),
        Some("the provider refused the request")
    );

    // A cancellation is a decision, not an error: it is recorded as its own
    // outcome with no reason, because Core publishes none for it.
    let mut adapter = connected(
        view(),
        vec![
            started(
                1,
                "turn-lane-3",
                lane_owner(Some("turn-lane-3")),
                TurnSource::UserInput,
            ),
            finished(
                2,
                "turn-lane-3",
                lane_owner(Some("turn-lane-3")),
                TurnOutcome::Cancelled,
            ),
        ],
    );
    adapter.poll_d1(Some(LANE), TIMEOUT).expect("poll");
    adapter.poll_d1(Some(LANE), TIMEOUT).expect("poll");
    let projection = adapter.d1_cockpit(Some(LANE)).expect("D1");
    let cancelled = projection.turn_failure.as_ref().expect("recorded");
    assert_eq!(cancelled.outcome, "cancelled");
    assert_eq!(cancelled.reason, None);
}

#[test]
fn a_completed_turn_records_no_failure() {
    let mut adapter = connected(
        view(),
        vec![
            started(
                1,
                "turn-lane-4",
                lane_owner(Some("turn-lane-4")),
                TurnSource::UserInput,
            ),
            finished(
                2,
                "turn-lane-4",
                lane_owner(Some("turn-lane-4")),
                TurnOutcome::Completed,
            ),
        ],
    );
    adapter.poll_d1(Some(LANE), TIMEOUT).expect("poll");
    adapter.poll_d1(Some(LANE), TIMEOUT).expect("poll");
    assert!(
        adapter
            .d1_cockpit(Some(LANE))
            .expect("D1")
            .turn_failure
            .is_none(),
        "a completed turn is not an error row"
    );
}
