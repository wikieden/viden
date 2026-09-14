//! The host half of the E3c native defects (checkpoints.md, defects 29-34).
//!
//! The rules under test are the ones the native `0.3.5` run proved missing on
//! the client side:
//!
//! 1. **An operator-git owner belongs to one Lane.** The commit bar resolves
//!    the owner of the Lane the cockpit has selected, and its
//!    `D1-OPERATOR-GIT-OWNER` refusal may name that Lane and no other
//!    (defect 30).
//! 2. **A submit behind a live turn is a queue, not a submit.** Core's own
//!    `QueueFollowUp` carries it and `InputQueued` confirms it, and it waits
//!    in its own pending rather than behind the unanswered submit that is
//!    holding the D1 slot (defect 29).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;
use viden_core::{
    FRONTEND_SCHEMA_V1, LaneRuntimeOwnerBinding, QueuedInputView, RuntimeCommand, RuntimeEvent,
    RuntimeEventEnvelope, RuntimeEventKind, RuntimeOwner, RuntimeSnapshot, RuntimeViewState,
    RuntimeWireEvent, TurnSource, TurnView,
};
use viden_gui::{D1Intent, GuiCoreAdapter};

mod support;
use support::TestCoreClient;

const TIMEOUT: Duration = Duration::from_millis(10);
/// The Lane the D1 fixture publishes, and the one E3c had selected.
const LANE_A: &str = "lane_d1_core";
/// A second Lane in the same workspace, the one E3c's refusal kept naming.
const LANE_B: &str = "lane_d1_review";

const D1_FIXTURE: &str = include_str!(
    "../../../crates/types/tests/fixtures/frontend-contract-v1/d1-vertical-slice.json"
);

#[derive(Deserialize)]
struct Fixture {
    initial_snapshot: RuntimeSnapshot,
    events: Vec<RuntimeEventEnvelope>,
}

fn owner_for(lane_id: &str, session_id: &str) -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace_contract_v1".into(),
        project_id: "project_viden".into(),
        lane_id: Some(lane_id.into()),
        session_id: Some(session_id.into()),
        task_id: Some("task_d1_core".into()),
        turn_id: None,
    }
}

fn fixture_view() -> RuntimeViewState {
    let fixture: Fixture = serde_json::from_str(D1_FIXTURE).expect("parse D1 fixture");
    let mut view = RuntimeViewState::new(fixture.initial_snapshot);
    for envelope in fixture.events {
        if let RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
    }
    view
}

/// The E3c workspace: two Lanes, and only the second one bound.
///
/// The asymmetry is what made the stale answer legible natively — the refusal
/// named a Lane that was not the selection, and the selection's own answer was
/// never asked for.
fn two_lane_view() -> RuntimeViewState {
    let mut view = fixture_view();
    view.lane_runtime_owners.clear();
    view.lane_runtime_owners.push(LaneRuntimeOwnerBinding {
        lane_id: LANE_B.into(),
        owner: owner_for(LANE_B, "session_lane_d1_review"),
    });
    view
}

fn connected(view: RuntimeViewState) -> GuiCoreAdapter {
    let mut adapter = GuiCoreAdapter::new(Box::new(TestCoreClient::new(
        view,
        Arc::new(Mutex::new(Vec::new())),
    )));
    adapter.connect().expect("connect the test client");
    adapter
}

fn envelope(sequence: u64, owner: RuntimeOwner, kind: RuntimeEventKind) -> RuntimeEventEnvelope {
    RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner,
        cursor: viden_core::EventCursor {
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

/* ---- defect 30: the owner the commit bar resolves is the selection's ---- */

#[test]
fn the_operator_git_owner_is_resolved_against_the_selected_lane_alone() {
    let adapter = connected(two_lane_view());

    let unbound = adapter.operator_git(Some(LANE_A));
    assert!(
        !unbound.owner_available,
        "Core published no binding for `{LANE_A}`, so the bar may not act for it"
    );
    let reason = unbound
        .owner_unavailable_reason
        .as_deref()
        .expect("an unavailable owner always carries its reason");
    assert!(
        reason.contains(LANE_A),
        "the refusal names the Lane it was asked about: {reason}"
    );
    assert!(
        !reason.contains(LANE_B),
        "the refusal must never name a Lane that was not asked about: {reason}"
    );

    // The selection switch, which is the whole of defect 30: the same adapter,
    // a different Lane, a different answer.
    let bound = adapter.operator_git(Some(LANE_B));
    assert!(
        bound.owner_available,
        "Core published exactly one binding for `{LANE_B}`"
    );
    assert_eq!(bound.owner_unavailable_reason, None);
}

/* ---- defect 29: a follow-up queues in its own pending ---- */

#[test]
fn a_follow_up_queues_while_the_submit_that_started_the_turn_is_still_unanswered() {
    let lane_owner = owner_for(LANE_B, "session_lane_d1_review");
    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = TestCoreClient::new(two_lane_view(), Arc::clone(&sent))
        // Nothing for the submit's own drain to find: it leaves and Core does
        // not answer it, which is the whole of the state under test.
        .with_gap()
        // Core starts the turn and then says nothing more about the submit —
        // a turn waiting on an approval holds `SubmitUserInput` open for as
        // long as the operator takes to decide, which is the state E3c was in.
        .with_envelope(envelope(
            30,
            lane_owner.clone(),
            RuntimeEventKind::TurnStarted {
                turn: TurnView {
                    turn_id: "turn_d1_review".into(),
                    owner: lane_owner.clone(),
                    source: TurnSource::UserInput,
                    started_at: 1_700_000_500,
                },
            },
        ))
        .with_envelope(envelope(
            31,
            lane_owner.clone(),
            RuntimeEventKind::CommandAccepted {
                command_id: "gui-h6-queue".to_string(),
                command: RuntimeCommand::QueueFollowUp {
                    content: "Summarise what you just changed".to_string(),
                },
            },
        ))
        .with_envelope(envelope(
            32,
            lane_owner.clone(),
            RuntimeEventKind::InputQueued {
                input: QueuedInputView {
                    id: "queued_h6".into(),
                    content_preview: "Summarise what you just changed".into(),
                    created_at: Some(1_700_000_600),
                    owner: Some(lane_owner.clone()),
                },
            },
        ));
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect the test client");

    let submitted = adapter
        .send_d1_intent_and_wait(
            "gui-h6-submit",
            D1Intent::Submit {
                lane_id: LANE_B.to_string(),
                content: "tool edit_file path=README.md".to_string(),
            },
            // The submit's own drain finds the gap and stops there, which is
            // what "Core has not answered it" means on this transport.
            Duration::ZERO,
        )
        .expect("the first submit is sent");
    assert_eq!(
        submitted.pending_command_id.as_deref(),
        Some("gui-h6-submit"),
        "the submit is unanswered, which is the state the follow-up has to survive"
    );

    // The route the cockpit used to take, kept here as the record of what it
    // did: a follow-up sent through the single D1 slot is refused by the
    // submit that is holding it, which is exactly what E3c's step 15 saw.
    let refused = adapter
        .send_d1_intent_and_wait(
            "gui-h6-follow-up-through-the-slot",
            D1Intent::Submit {
                lane_id: LANE_B.to_string(),
                content: "Summarise what you just changed".to_string(),
            },
            Duration::ZERO,
        )
        .expect_err("the one D1 slot is occupied");
    assert!(
        refused.contains("gui-h6-submit"),
        "the refusal names the submit that holds the slot: {refused}"
    );

    let queued = adapter
        .queue_follow_up_and_wait(
            "gui-h6-queue",
            LANE_B,
            "Summarise what you just changed",
            TIMEOUT,
        )
        .expect("the follow-up is queued behind the live turn rather than refused");

    let commands = sent.lock().expect("read the sent commands");
    assert!(
        matches!(
            commands.last().map(|envelope| &envelope.command),
            Some(RuntimeCommand::QueueFollowUp { content })
                if content == "Summarise what you just changed"
        ),
        "the command Core receives is `QueueFollowUp`, not a second `SubmitUserInput`: {:?}",
        commands.last().map(|envelope| &envelope.command)
    );
    drop(commands);

    assert_eq!(
        queued.outcome.state, "confirmed",
        "Core's own `InputQueued` is what confirms it"
    );
    assert_eq!(
        queued.pending_command_id, None,
        "the queue settled in its own pending"
    );

    let still_open = adapter
        .poll_d1(Some(LANE_B), TIMEOUT)
        .expect("the D1 slot is read separately");
    assert_eq!(
        still_open.pending_command_id.as_deref(),
        Some("gui-h6-submit"),
        "settling the queue must never free the submit's slot: they are two commands"
    );
}
