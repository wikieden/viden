use std::sync::{Arc, Mutex};
use std::time::Duration;

use viden_core::{CoreClientError, RuntimeSnapshot, RuntimeViewState};
use viden_gui::{D6ConnectionState, D6State, GuiCoreAdapter};

mod support;
use support::TestCoreClient;

const D1_FIXTURE: &str = include_str!(
    "../../../crates/types/tests/fixtures/frontend-contract-v1/d1-vertical-slice.json"
);

fn view() -> RuntimeViewState {
    let fixture: serde_json::Value = serde_json::from_str(D1_FIXTURE).unwrap();
    RuntimeViewState::new(
        serde_json::from_value::<RuntimeSnapshot>(fixture["initial_snapshot"].clone()).unwrap(),
    )
}

#[test]
fn event_gap_blocks_business_success_until_snapshot_recovery_is_published() {
    let client = TestCoreClient::new(view(), Arc::new(Mutex::new(Vec::new()))).with_recv_error(
        CoreClientError::SnapshotRequired {
            reason_code: "event_gap".into(),
        },
    );
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().unwrap();

    assert!(adapter.pump(Duration::ZERO).is_err());
    let recovering = adapter.d6_recovery();
    assert_eq!(recovering.connection, D6ConnectionState::Recovering);
    assert_eq!(recovering.state, D6State::EventGap);
    assert!(recovering.business_success_blocked);

    adapter
        .recover()
        .expect("validated Core snapshot completes recovery");
    let live = adapter.d6_recovery();
    assert_eq!(live.connection, D6ConnectionState::Live);
    assert!(!live.business_success_blocked);
}

#[test]
fn incompatible_schema_and_transport_disconnect_remain_explicit_non_live_states() {
    let incompatible = TestCoreClient::new(view(), Arc::new(Mutex::new(Vec::new())))
        .with_recv_error(CoreClientError::Compatibility(
            "schema 9 is unsupported".into(),
        ));
    let mut adapter = GuiCoreAdapter::new(Box::new(incompatible));
    adapter.connect().unwrap();
    assert!(adapter.pump(Duration::ZERO).is_err());
    assert_eq!(adapter.d6_recovery().state, D6State::IncompatibleSchema);

    let disconnected = TestCoreClient::new(view(), Arc::new(Mutex::new(Vec::new())))
        .with_recv_error(CoreClientError::Transport("bridge dropped".into()));
    let mut adapter = GuiCoreAdapter::new(Box::new(disconnected));
    adapter.connect().unwrap();
    assert!(adapter.pump(Duration::ZERO).is_err());
    assert_eq!(adapter.d6_recovery().state, D6State::Disconnected);
    assert!(adapter.d6_recovery().business_success_blocked);
}

/*
 * H2 hygiene — E1 defect 7, the adapter-drop seam.
 *
 * The run saw the cockpit fall back to `Core connection pending` and the
 * process disappear about ten seconds later, with no crash report. The claim
 * under test is the one the report implies and the code must not contain: that
 * losing the transport costs the operator the session.
 *
 * At this seam a dropped transport is a *state change*, not a teardown. The
 * adapter keeps the last view Core published, classifies the drop, and offers
 * the reconnect the D6 contract already models; nothing here — and nothing
 * above it in `apps/gui/src-tauri/src/lib.rs` — terminates anything. The event
 * pump's only escape is a poisoned lock, and it ends its own thread; the
 * process outlives it.
 */
#[test]
fn a_dropped_transport_keeps_the_published_view_and_offers_a_reconnect() {
    let dropped = TestCoreClient::new(view(), Arc::new(Mutex::new(Vec::new())))
        .with_recv_error(CoreClientError::Transport("bridge dropped".into()));
    let mut adapter = GuiCoreAdapter::new(Box::new(dropped));
    adapter.connect().unwrap();
    let bound = adapter
        .d1_cockpit(None)
        .expect("a connected adapter publishes the cockpit");

    // The drop, then fifteen more drains: a client that polls every 250ms for
    // the ten seconds the run measured must not find a different answer, and
    // must not panic on any of them.
    for _ in 0..16 {
        adapter.pump_events(Duration::ZERO);
    }

    let after = adapter
        .d1_cockpit(None)
        .expect("the last view Core published survives the drop");
    // The facts are the ones Core published, unchanged: the client neither
    // invents newer state nor throws the old away.
    assert_eq!(after.lanes.len(), bound.lanes.len());
    assert_eq!(after.transcript.len(), bound.transcript.len());

    let recovery = adapter.d6_recovery();
    assert_eq!(recovery.connection, D6ConnectionState::Disconnected);
    assert_eq!(recovery.state, D6State::Disconnected);
    // Business success is blocked and a reconnect is offered: the two halves of
    // "keep the session, stop trusting it".
    assert!(recovery.business_success_blocked);
    assert!(
        recovery
            .actions
            .iter()
            .any(|action| action.kind == "reconnect" && action.available),
        "a disconnected adapter must offer the reconnect the D6 contract models: {:?}",
        recovery.actions
    );
}
