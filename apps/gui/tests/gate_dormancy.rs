//! Dormant merge gates across every GUI surface that counts or lists them.
//!
//! A gate still awaiting a decision whose Agent session has already finished
//! cannot get the evidence it waits for. Eight such gates, from sessions
//! terminal for weeks, rendered as first-class pending work: they buried live
//! gates at the top of D12, inflated the statusbar badge, and held D6 out of
//! its clear state.
//!
//! Every surface reads the same projection-level predicate, so these tests also
//! pin that there is one definition rather than four that can drift apart.
//! Dormancy is presentation: a dormant gate stays listed, stays selectable, and
//! keeps every action Core allows.

use std::sync::{Arc, Mutex};

use viden_core::{
    AgentSessionStatus, AgentSessionView, MergeGateRecord, MergeGateStatus, RuntimeOwner,
    RuntimeSnapshot, RuntimeViewState,
};
use viden_gui::{D6State, GuiCoreAdapter};

mod support;
use support::TestCoreClient;

const D1_FIXTURE: &str = include_str!(
    "../../../crates/types/tests/fixtures/frontend-contract-v1/d1-vertical-slice.json"
);
const APPROVAL_FIXTURE: &str = include_str!(
    "../../../crates/types/tests/fixtures/frontend-contract-v1/approval-allow-deny.json"
);

fn d1_view() -> RuntimeViewState {
    #[derive(serde::Deserialize)]
    struct Fixture {
        initial_snapshot: RuntimeSnapshot,
        events: Vec<viden_core::RuntimeEventEnvelope>,
    }
    let fixture: Fixture = serde_json::from_str(D1_FIXTURE).expect("parse D1 fixture");
    let mut view = RuntimeViewState::new(fixture.initial_snapshot);
    for envelope in fixture.events {
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
    adapter.connect().expect("connect");
    adapter
}

/// An open gate owned by `session_id`, shaped as Core publishes one.
///
/// The gate id stays keyed on the ACP protocol handle exactly as Core keys it;
/// the owner session is the separate fact that makes the gate joinable to
/// `agent_sessions` at all.
fn open_gate(gate_id: &str, session_id: Option<&str>) -> MergeGateRecord {
    let mut gate: MergeGateRecord = serde_json::from_value(serde_json::json!({
        "gate_id": gate_id,
        "task_id": format!("task-{gate_id}"),
        "status": "proposed",
        "required_evidence": ["test_result"],
        "evidence_ids": []
    }))
    .expect("gate fixture");
    gate.owner.session_id = session_id.map(str::to_string);
    gate
}

fn session(session_id: &str, status: AgentSessionStatus) -> AgentSessionView {
    AgentSessionView {
        session_id: session_id.to_string(),
        lane_id: "lane_d1_core".to_string(),
        agent_id: "codex-acp".to_string(),
        model: None,
        status,
        owner: RuntimeOwner {
            lane_id: Some("lane_d1_core".to_string()),
            session_id: Some(session_id.to_string()),
            ..RuntimeOwner::default()
        },
        task: "merge the lane".to_string(),
        diagnostic: None,
        output: None,
    }
}

/// One gate on a live session and one on a finished session, with the dormant
/// gate deliberately first in Core's own order.
fn mixed_view() -> RuntimeViewState {
    let mut view = d1_view();
    view.pending_approvals.clear();
    view.merge_gates.clear();
    view.merge_gates
        .push(open_gate("gate-dormant", Some("session-done")));
    view.merge_gates
        .push(open_gate("gate-live", Some("session-live")));
    view.agent_sessions
        .push(session("session-done", AgentSessionStatus::Completed));
    view.agent_sessions
        .push(session("session-live", AgentSessionStatus::Running));
    view
}

// ---------------------------------------------------------------------------
// D12 gate list and default detail selection
// ---------------------------------------------------------------------------

#[test]
fn d12_lists_active_gates_before_dormant_ones_and_hides_none() {
    let projection = connected(mixed_view()).d12_integration_gate().expect("D12");

    let ids: Vec<&str> = projection
        .gates
        .iter()
        .map(|gate| gate.gate_id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["gate-live", "gate-dormant"],
        "active gates come first even though Core listed the dormant one first"
    );
    // Grouping, never hiding: both gates are still in the list.
    assert_eq!(projection.gates.len(), 2);
    assert!(!projection.gates[0].dormant);
    assert!(projection.gates[1].dormant);
}

#[test]
fn d12_opens_on_an_active_gate_rather_than_a_dormant_one() {
    let projection = connected(mixed_view()).d12_integration_gate().expect("D12");
    assert_eq!(projection.selected_gate_id.as_deref(), Some("gate-live"));
    assert_eq!(
        projection.detail.expect("detail").gate.gate_id,
        "gate-live",
        "the detail pane opens on work the operator can still act on"
    );
}

#[test]
fn d12_still_selects_a_dormant_gate_when_that_is_all_core_published() {
    let mut view = mixed_view();
    view.merge_gates
        .retain(|gate| gate.gate_id == "gate-dormant");
    let projection = connected(view).d12_integration_gate().expect("D12");

    // Dormancy is a fallback rule, not a filter: with nothing active the
    // dormant gate is still selected and still fully rendered.
    assert_eq!(projection.selected_gate_id.as_deref(), Some("gate-dormant"));
    assert!(projection.detail.is_some());
    assert!(projection.gates[0].dormant);
}

#[test]
fn d12_honours_an_explicit_selection_of_a_dormant_gate() {
    let projection = connected(mixed_view())
        .d12_integration_gate_for("gate-dormant")
        .expect("D12");
    assert_eq!(projection.selected_gate_id.as_deref(), Some("gate-dormant"));
    assert_eq!(
        projection.detail.expect("detail").gate.gate_id,
        "gate-dormant",
        "an operator who picks a dormant gate still gets its decision surface"
    );
}

/// The classification table. A decided gate is settled rather than dormant, and
/// a session this view has never seen is not a finished one.
#[test]
fn only_a_pending_gate_on_a_finished_session_is_dormant() {
    for terminal in [
        AgentSessionStatus::Completed,
        AgentSessionStatus::Failed,
        AgentSessionStatus::Cancelled,
    ] {
        let mut view = mixed_view();
        view.agent_sessions
            .retain(|entry| entry.session_id != "session-done");
        view.agent_sessions.push(session("session-done", terminal));
        let projection = connected(view).d12_integration_gate().expect("D12");
        let gate = projection
            .gates
            .iter()
            .find(|gate| gate.gate_id == "gate-dormant")
            .expect("gate");
        assert!(gate.dormant, "a pending gate on a {terminal:?} session");
    }

    for live in [
        AgentSessionStatus::Starting,
        AgentSessionStatus::Running,
        AgentSessionStatus::WaitingApproval,
    ] {
        let mut view = mixed_view();
        view.agent_sessions
            .retain(|entry| entry.session_id != "session-done");
        view.agent_sessions.push(session("session-done", live));
        let projection = connected(view).d12_integration_gate().expect("D12");
        assert!(
            projection.gates.iter().all(|gate| !gate.dormant),
            "a pending gate on a {live:?} session is actionable"
        );
    }

    // A closed gate is settled history, not dormant residue.
    let mut decided = mixed_view();
    decided
        .merge_gates
        .iter_mut()
        .find(|gate| gate.gate_id == "gate-dormant")
        .expect("gate")
        .status = MergeGateStatus::Merged;
    assert!(
        connected(decided)
            .d12_integration_gate()
            .expect("D12")
            .gates
            .iter()
            .all(|gate| !gate.dormant)
    );

    // An unknown session is not a finished one.
    let mut unknown = mixed_view();
    unknown
        .agent_sessions
        .retain(|entry| entry.session_id != "session-done");
    assert!(
        connected(unknown)
            .d12_integration_gate()
            .expect("D12")
            .gates
            .iter()
            .all(|gate| !gate.dormant)
    );
}

// ---------------------------------------------------------------------------
// D1 statusbar badge
// ---------------------------------------------------------------------------

#[test]
fn the_statusbar_badge_counts_only_gates_its_target_can_act_on() {
    let statusbar = connected(mixed_view())
        .d1_cockpit(Some("lane_d1_core"))
        .expect("D1")
        .statusbar;
    assert_eq!(
        statusbar.pending_gate_count, 1,
        "the dormant gate is listed in D12 but is not work waiting on the operator"
    );

    // With every gate dormant the badge is empty rather than advertising a
    // queue that leads to nothing actionable.
    let mut all_dormant = mixed_view();
    all_dormant
        .merge_gates
        .retain(|gate| gate.gate_id == "gate-dormant");
    assert_eq!(
        connected(all_dormant)
            .d1_cockpit(Some("lane_d1_core"))
            .expect("D1")
            .statusbar
            .pending_gate_count,
        0
    );
}

#[test]
fn the_statusbar_badge_still_counts_pending_approvals_alongside_live_gates() {
    // Approvals are untouched by gate dormancy: the badge is their sum with
    // the gates that are still actionable.
    let mut view = mixed_view();
    let fixture: serde_json::Value = serde_json::from_str(APPROVAL_FIXTURE).expect("fixture");
    let envelope: viden_core::RuntimeEventEnvelope =
        serde_json::from_value(fixture["events"][0].clone()).expect("approval event");
    if let viden_core::RuntimeWireEvent::Known(event) = envelope.event {
        view.apply_event(&event);
    }
    assert_eq!(view.pending_approvals.len(), 1, "one approval is pending");
    assert_eq!(
        connected(view)
            .d1_cockpit(Some("lane_d1_core"))
            .expect("D1")
            .statusbar
            .pending_gate_count,
        2,
        "one pending approval plus the one gate whose session is still live"
    );
}

// ---------------------------------------------------------------------------
// D6 gate-queue state
// ---------------------------------------------------------------------------

#[test]
fn d6_reaches_gate_queue_clear_when_only_dormant_gates_remain() {
    let mut clear = mixed_view();
    clear
        .merge_gates
        .retain(|gate| gate.gate_id == "gate-dormant");
    clear.errors.clear();
    clear.context_budgets.clear();
    clear
        .tasks
        .retain(|task| task.status != viden_core::AgentTaskStatus::Failed);
    assert_eq!(
        connected(clear).d6_recovery().state,
        D6State::GateQueueClear,
        "a gate abandoned by a finished session is not a decision still owed"
    );
}

#[test]
fn d6_stays_out_of_gate_queue_clear_while_a_live_gate_is_open() {
    let mut open = mixed_view();
    open.errors.clear();
    open.context_budgets.clear();
    open.tasks
        .retain(|task| task.status != viden_core::AgentTaskStatus::Failed);
    assert_ne!(connected(open).d6_recovery().state, D6State::GateQueueClear);
}
