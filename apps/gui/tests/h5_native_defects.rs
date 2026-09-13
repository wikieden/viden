//! The host half of the E3b native defects (checkpoints.md, defects 22-24).
//!
//! Three host rules the native `0.3.5` run proved missing:
//!
//! 1. **One receive funnel settles every correlated command.** The desktop
//!    shell drains ordered Core events on a background pump and wakes the
//!    webview afterwards, so the drain that sees a D1 receipt is almost never
//!    the screen's own `poll_d1`. A pending the funnel does not observe is a
//!    command slot held for the life of the process (defect 22).
//! 2. **A refusal names its cause.** A submit the client cannot send is
//!    refused with the occupied pending's own kind and age, and the client
//!    only offers to stop waiting where the host would actually allow it —
//!    a pending mutation never (defect 23).
//! 3. **Cancel is Core's fact, not an identity's.** `Cancel active turn` is
//!    offered only where Core reports work in flight for the Lane, never
//!    because an owner binding exists (defect 24).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;
use viden_core::{
    FRONTEND_SCHEMA_V1, LaneRuntimeOwnerBinding, RuntimeEvent, RuntimeEventEnvelope,
    RuntimeEventKind, RuntimeOwner, RuntimeSnapshot, RuntimeViewState, RuntimeWireEvent,
    TurnSource, TurnView,
};
use viden_gui::{D1Intent, GuiCoreAdapter};

mod support;
use support::TestCoreClient;

const TIMEOUT: Duration = Duration::from_millis(10);
const LANE: &str = "lane_d1_core";

const D1_FIXTURE: &str = include_str!(
    "../../../crates/types/tests/fixtures/frontend-contract-v1/d1-vertical-slice.json"
);

#[derive(Deserialize)]
struct Fixture {
    initial_snapshot: RuntimeSnapshot,
    events: Vec<RuntimeEventEnvelope>,
}

fn lane_owner() -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace_contract_v1".into(),
        project_id: "project_viden".into(),
        lane_id: Some(LANE.into()),
        session_id: Some("session_lane_d1_core".into()),
        task_id: Some("task_d1_core".into()),
        // The residue defect 24 is about: a binding keeps the id of a turn
        // that already ended.
        turn_id: Some("turn_d1_core".into()),
    }
}

fn bound_view() -> RuntimeViewState {
    let fixture: Fixture = serde_json::from_str(D1_FIXTURE).expect("parse D1 fixture");
    let mut view = RuntimeViewState::new(fixture.initial_snapshot);
    for envelope in fixture.events {
        if let RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
    }
    view.lane_runtime_owners.push(LaneRuntimeOwnerBinding {
        lane_id: LANE.into(),
        owner: lane_owner(),
    });
    view
}

fn view_with_live_turn() -> RuntimeViewState {
    let mut view = bound_view();
    view.apply_event(&RuntimeEvent::new(
        900,
        RuntimeEventKind::TurnStarted {
            turn: TurnView {
                turn_id: "turn_d1_core".into(),
                owner: lane_owner(),
                source: TurnSource::UserInput,
                started_at: 1_700_000_500,
            },
        },
    ));
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

/* ---- defect 22: the pump settles what it drains ---- */

#[test]
fn a_receipt_drained_by_the_desktop_pump_still_frees_the_d1_command_slot() {
    let client = TestCoreClient::new(bound_view(), Arc::new(Mutex::new(Vec::new())))
        // Nothing for the send's own drain to find: the background pump is
        // the thread sitting in the bounded receive.
        .with_gap()
        .with_envelope(envelope(
            20,
            RuntimeOwner::default(),
            RuntimeEventKind::CommandAccepted {
                command_id: "gui-d1-mode".to_string(),
                command: viden_core::RuntimeCommand::SetWorkMode {
                    mode: viden_core::WorkMode::Build,
                },
            },
        ))
        .with_envelope(envelope(
            21,
            RuntimeOwner::default(),
            RuntimeEventKind::SnapshotUpdated {
                snapshot: bound_view().snapshot.clone(),
            },
        ));
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect the test client");

    let pending = adapter
        .send_composer_control_and_wait(
            "gui-d1-mode",
            viden_gui::ComposerControlIntent::SetWorkMode {
                mode: "build".to_string(),
            },
            Some(LANE),
            Duration::ZERO,
        )
        .expect("the control is sent");
    assert_eq!(pending.outcome.state, "pending");
    assert_eq!(pending.pending_command_id.as_deref(), Some("gui-d1-mode"));

    // The desktop shell's background pump is the next thing to touch the
    // stream, and it has no screen to answer.
    while adapter.pump_events(Duration::ZERO) {}

    let settled = adapter.poll_d1(Some(LANE), TIMEOUT).expect("the D1 poll");
    assert_eq!(
        settled.pending_command_id, None,
        "a receipt consumed by the pump must still settle the D1 command"
    );
    assert_eq!(settled.outcome.state, "confirmed");
}

/* ---- defect 23: a refused submit names what is holding the slot ---- */

#[test]
fn an_occupied_command_slot_publishes_its_kind_age_and_whether_it_may_be_abandoned() {
    let mut adapter = connected(bound_view());

    // A read. The host abandons exactly this and `QueryAgentAdapters`.
    let probing = adapter
        .send_d1_intent_and_wait(
            "gui-d1-probe",
            D1Intent::ProbeAgentAdapter {
                agent_id: "codex-acp".to_string(),
            },
            Duration::ZERO,
        )
        .expect("the probe is sent");
    let pending = probing
        .pending_command
        .as_ref()
        .expect("an occupied slot publishes its own pending record");
    assert_eq!(pending.command_id, "gui-d1-probe");
    assert_eq!(pending.kind, "probe_agent_adapter");
    assert!(
        pending.abandonable,
        "an adapter discovery read mutates nothing, so its wait may be abandoned"
    );
    assert!(pending.age_ms < 60_000);

    adapter
        .abandon_pending_probe(Duration::ZERO)
        .expect("abandon the discovery read");

    // A mutation. Its answer is the only record of whether its effect ran, so
    // the client must never offer to stop waiting for it.
    let submitting = adapter
        .send_d1_intent_and_wait(
            "gui-d1-submit",
            D1Intent::Submit {
                lane_id: LANE.to_string(),
                content: "edit the fixture README".to_string(),
            },
            Duration::ZERO,
        )
        .expect("the submit is sent");
    let pending = submitting
        .pending_command
        .as_ref()
        .expect("an occupied slot publishes its own pending record");
    assert_eq!(pending.kind, "submit_user_input");
    assert!(
        !pending.abandonable,
        "a pending mutation is never abandoned client-side"
    );
}

#[test]
fn a_free_command_slot_publishes_no_pending_record_to_name() {
    let adapter = connected(bound_view());
    let mut adapter = adapter;
    let idle = adapter.poll_d1(Some(LANE), Duration::ZERO).expect("poll");

    assert_eq!(idle.pending_command_id, None);
    assert!(
        idle.pending_command.is_none(),
        "a free slot must publish nothing for a refusal to name"
    );
}

/* ---- defect 24: Cancel is offered on Core's active-turn fact ---- */

#[test]
fn cancel_is_not_offered_for_a_bound_lane_core_is_running_no_turn_for() {
    let projection = connected(bound_view())
        .d1_cockpit(Some(LANE))
        .expect("D1 projection");

    assert!(
        !projection.composer.busy,
        "the fixture publishes no active turn for this Lane"
    );
    assert!(
        !projection.composer.can_cancel,
        "an owner binding is an identity, not work: Cancel must not claim a turn that does not exist"
    );
}

#[test]
fn cancel_is_offered_once_core_reports_an_active_turn_for_the_lane() {
    let projection = connected(view_with_live_turn())
        .d1_cockpit(Some(LANE))
        .expect("D1 projection");

    assert!(projection.composer.busy);
    assert!(
        projection.composer.can_cancel,
        "Core's own `active_turns` entry is what makes the turn cancellable"
    );
}

/* ---- the same funnel hole for Plan Studio and the Agent Board ---- */

const PLAN_FIXTURE: &str =
    include_str!("../../../crates/types/tests/fixtures/frontend-contract-v1/plan-documents.json");
const CONTROL_FIXTURE: &str = include_str!(
    "../../../crates/types/tests/fixtures/frontend-contract-v1/agent-session-control.json"
);

#[derive(Deserialize)]
struct EnvelopeFixture {
    initial_snapshot: RuntimeSnapshot,
    events: Vec<RuntimeEventEnvelope>,
}

/// Replays the first `events` envelopes of a fixture through Core's own
/// reducer and hands back the remaining envelopes, unmodified.
///
/// The tail is what the pump is then made to drain, so these tests feed Core's
/// own frozen bytes — including its own `command_id`s — rather than a
/// hand-built receipt the screen might happen to accept.
fn split_fixture(fixture: &str, applied: usize) -> (RuntimeViewState, Vec<RuntimeEventEnvelope>) {
    let fixture: EnvelopeFixture = serde_json::from_str(fixture).expect("fixture json");
    let mut view = RuntimeViewState::new(fixture.initial_snapshot);
    let mut rest = Vec::new();
    for (index, envelope) in fixture.events.into_iter().enumerate() {
        if index < applied {
            if let RuntimeWireEvent::Known(event) = &envelope.event {
                view.apply_event(event);
            }
        } else {
            rest.push(envelope);
        }
    }
    (view, rest)
}

#[test]
fn a_plan_receipt_drained_by_the_desktop_pump_still_frees_the_d15_command_slot() {
    // Through event 6: `update_plan` has proposed revision 2 and nothing has
    // been decided, which is the state the operator presses Accept from.
    let (view, rest) = split_fixture(PLAN_FIXTURE, 7);
    let plan = view.plan_documents[0].clone();
    let mut client = TestCoreClient::new(view, Arc::new(Mutex::new(Vec::new())))
        // Nothing for the send's own drain to find: the background pump is the
        // thread sitting in the bounded receive.
        .with_gap();
    // Events 9, 10 and 11 — Core's acceptance of `plan_accept` and the
    // `PlanDocumentDecided` fact that answers it.
    for envelope in rest.into_iter().skip(2).take(3) {
        client = client.with_envelope(envelope);
    }
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect the test client");

    let pending = adapter
        .send_d15_intent_and_wait(
            "plan_accept",
            viden_gui::D15Intent::Accept {
                plan_id: plan.plan_id.0.clone(),
                revision: plan.revision,
                content_sha256: plan.content_sha256.clone(),
            },
            Duration::ZERO,
        )
        .expect("the plan decision is sent");
    assert_eq!(pending.outcome.state, "pending");
    assert_eq!(pending.pending_command_id.as_deref(), Some("plan_accept"));

    while adapter.pump_events(Duration::ZERO) {}

    let settled = adapter.poll_d15(None, TIMEOUT).expect("the D15 poll");
    assert_eq!(
        settled.pending_command_id, None,
        "a plan receipt consumed by the pump must still free the D15 slot"
    );
    assert_eq!(
        settled.outcome.state, "confirmed",
        "and the screen's next poll must still be able to read the outcome"
    );
}

#[test]
fn a_session_receipt_drained_by_the_desktop_pump_still_frees_the_d16_command_slot() {
    // Through event 3: the session is live with its approval already denied,
    // which is the state the operator presses Pause from.
    let (view, rest) = split_fixture(CONTROL_FIXTURE, 4);
    let mut client = TestCoreClient::new(view, Arc::new(Mutex::new(Vec::new()))).with_gap();
    // Events 4 and 5 — Core's acceptance of `cmd_session_pause` and the paused
    // session fact that answers it.
    for envelope in rest.into_iter().take(2) {
        client = client.with_envelope(envelope);
    }
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect the test client");

    let pending = adapter
        .send_d16_intent_and_wait(
            "cmd_session_pause",
            viden_gui::D16Intent::Pause {
                session_id: "session_control".to_string(),
            },
            Duration::ZERO,
        )
        .expect("the session control is sent");
    assert_eq!(pending.outcome.state, "pending");
    assert_eq!(
        pending.pending_command_id.as_deref(),
        Some("cmd_session_pause")
    );

    while adapter.pump_events(Duration::ZERO) {}

    let settled = adapter.poll_d16(None, TIMEOUT).expect("the D16 poll");
    assert_eq!(
        settled.pending_command_id, None,
        "a session receipt consumed by the pump must still free the D16 slot"
    );
    assert_eq!(
        settled.outcome.state, "confirmed",
        "and the screen's next poll must still be able to read the outcome"
    );
}
