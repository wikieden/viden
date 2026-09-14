//! The host half of the E3c native defects (checkpoints.md, defects 29-34).
//!
//! The rules under test are the ones the native `0.3.5` run proved missing on
//! the client side:
//!
//! 1. **An operator-git owner belongs to one Lane.** The commit bar resolves
//!    the owner of the Lane the cockpit has selected, and its
//!    `D1-OPERATOR-GIT-OWNER` refusal may name that Lane and no other
//!    (defect 30).

use std::sync::{Arc, Mutex};

use serde::Deserialize;
use viden_core::{
    LaneRuntimeOwnerBinding, RuntimeEventEnvelope, RuntimeOwner, RuntimeSnapshot, RuntimeViewState,
    RuntimeWireEvent,
};
use viden_gui::GuiCoreAdapter;

mod support;
use support::TestCoreClient;

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
