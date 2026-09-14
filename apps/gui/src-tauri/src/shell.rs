//! The shipped shell path, as plain functions over the desktop state.
//!
//! Every `#[tauri::command]` in `lib.rs` that a test needs to drive delegates
//! its whole body to a function here, and `spawn_core_event_pump` runs
//! [`pump_core_events_once`]. There is exactly one body per command, so a test
//! calling a function here calls what the webview's `invoke` calls: the same
//! `DesktopState` mutex, the same `GuiCoreAdapter`, the same lock discipline,
//! and — when the test spawns the pump — the same background drain competing
//! for that lock.
//!
//! Why this layer exists at all: H6's tests exercised the cockpit against a
//! mocked port and the adapter directly, and both passed while the shipped
//! shell dropped a stage and a follow-up on the floor (E3d defects 35 and 36).
//! The shell path is webview -> `main.ts` port -> `invoke` -> this function ->
//! adapter, woken only by the pump, and that is the path under test here.

use std::sync::Mutex;
use std::time::Duration;

use crate::{
    D1CockpitProjection, D1Intent, D1IntentResult, D2DecisionsProjection, GuiCoreAdapter,
    OperatorGitIntent, OperatorGitProjection, PermissionIntent, PermissionIntentResult,
};

/// The one piece of state the desktop host manages, and the one lock every
/// command and the pump contend for.
///
/// `pub` so an in-process harness can hold the same state a Tauri app manages
/// instead of building a parallel one that would not reproduce the contention.
pub struct DesktopState {
    pub(crate) adapter: Mutex<Option<GuiCoreAdapter>>,
}

impl DesktopState {
    pub fn new(adapter: Option<GuiCoreAdapter>) -> Self {
        Self {
            adapter: Mutex::new(adapter),
        }
    }
}

/// The wait every command gives Core's ordered answer before it returns.
pub const COMMAND_EVENT_TIMEOUT: Duration = Duration::from_millis(250);

const LOCK_UNAVAILABLE: &str = "GUI Core adapter lock is unavailable";
const NOT_CONNECTED: &str = "Core adapter is not connected";

/// Runs `body` against the bound adapter, or reports why there is none.
///
/// The lock is taken for the whole body, exactly as each Tauri command takes
/// it: a command and the pump never touch the adapter at the same time, and a
/// command that waits for Core holds the pump off for that wait.
fn with_adapter_mut<T>(
    state: &DesktopState,
    body: impl FnOnce(&mut GuiCoreAdapter) -> Result<T, String>,
) -> Result<T, String> {
    body(
        state
            .adapter
            .lock()
            .map_err(|_| LOCK_UNAVAILABLE.to_string())?
            .as_mut()
            .ok_or_else(|| NOT_CONNECTED.to_string())?,
    )
}

/// The read-only twin of [`with_adapter_mut`], for projections that send nothing.
fn with_adapter<T>(
    state: &DesktopState,
    body: impl FnOnce(&GuiCoreAdapter) -> Result<T, String>,
) -> Result<T, String> {
    body(
        state
            .adapter
            .lock()
            .map_err(|_| LOCK_UNAVAILABLE.to_string())?
            .as_ref()
            .ok_or_else(|| NOT_CONNECTED.to_string())?,
    )
}

/* ---- operator source control (GUI-CORE-020) ---- */

pub fn run_operator_git_action(
    state: &DesktopState,
    command_id: &str,
    lane_id: Option<&str>,
    action: OperatorGitIntent,
) -> Result<OperatorGitProjection, String> {
    with_adapter_mut(state, |adapter| {
        adapter.run_operator_git_action_and_wait(command_id, lane_id, action, COMMAND_EVENT_TIMEOUT)
    })
}

pub fn operator_git_poll(
    state: &DesktopState,
    lane_id: Option<&str>,
) -> Result<OperatorGitProjection, String> {
    with_adapter_mut(state, |adapter| {
        adapter.poll_operator_git(lane_id, COMMAND_EVENT_TIMEOUT)
    })
}

pub fn operator_git(
    state: &DesktopState,
    lane_id: Option<&str>,
) -> Result<OperatorGitProjection, String> {
    with_adapter(state, |adapter| Ok(adapter.operator_git(lane_id)))
}

/* ---- D1 ---- */

pub fn d1_cockpit(
    state: &DesktopState,
    selected_lane_id: Option<&str>,
) -> Option<D1CockpitProjection> {
    state
        .adapter
        .lock()
        .ok()?
        .as_ref()?
        .d1_cockpit(selected_lane_id)
}

pub fn d1_send_intent(
    state: &DesktopState,
    command_id: &str,
    intent: D1Intent,
) -> Result<D1IntentResult, String> {
    with_adapter_mut(state, |adapter| {
        adapter.send_d1_intent_and_wait(command_id, intent, COMMAND_EVENT_TIMEOUT)
    })
}

pub fn d1_abandon_probe(state: &DesktopState) -> Result<D1IntentResult, String> {
    with_adapter_mut(state, |adapter| {
        adapter.abandon_pending_probe(COMMAND_EVENT_TIMEOUT)
    })
}

pub fn d1_queue_follow_up(
    state: &DesktopState,
    command_id: &str,
    lane_id: &str,
    content: &str,
) -> Result<D1IntentResult, String> {
    with_adapter_mut(state, |adapter| {
        adapter.queue_follow_up_and_wait(command_id, lane_id, content, COMMAND_EVENT_TIMEOUT)
    })
}

pub fn d1_poll(
    state: &DesktopState,
    selected_lane_id: Option<&str>,
    wait_for_event: bool,
) -> Result<D1IntentResult, String> {
    let timeout = if wait_for_event {
        COMMAND_EVENT_TIMEOUT
    } else {
        Duration::ZERO
    };
    with_adapter_mut(state, |adapter| adapter.poll_d1(selected_lane_id, timeout))
}

/* ---- permission dock and D2 ---- */

pub fn permission_send_intent(
    state: &DesktopState,
    command_id: &str,
    intent: PermissionIntent,
) -> Result<PermissionIntentResult, String> {
    with_adapter_mut(state, |adapter| {
        adapter.send_permission_intent_and_wait(command_id, intent, COMMAND_EVENT_TIMEOUT)
    })
}

pub fn permission_poll(state: &DesktopState) -> Result<PermissionIntentResult, String> {
    with_adapter_mut(state, |adapter| adapter.poll_permission(Duration::ZERO))
}

pub fn d2_decisions(
    state: &DesktopState,
    selected_id: Option<&str>,
) -> Result<Option<D2DecisionsProjection>, String> {
    with_adapter(state, |adapter| {
        Ok(match selected_id {
            Some(id) => adapter.d2_decisions_for(id),
            None => adapter.d2_decisions(),
        })
    })
}

/* ---- the desktop event pump ---- */

/// One iteration of the desktop pump: drain ordered Core events off the UI
/// thread and report whether Core advanced.
///
/// `None` means no workspace is bound yet — the caller waits rather than
/// burning the CPU on an empty lock — and `Some(false)` means the drain came
/// back empty. `Err` means the lock is gone and the pump must stop.
pub fn pump_core_events_once(
    state: &DesktopState,
    event_timeout: Duration,
) -> Result<Option<bool>, ()> {
    let mut guard = state.adapter.lock().map_err(|_| ())?;
    match guard.as_mut() {
        Some(adapter) => Ok(Some(adapter.pump_events(event_timeout))),
        None => Ok(None),
    }
}
