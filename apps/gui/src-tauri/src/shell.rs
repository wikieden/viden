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

use std::sync::{Mutex, TryLockError};
use std::thread;
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

/// How long one pump round may hold the adapter lock for its drain.
///
/// E3d defect 35. Every Tauri command runs on a thread that is not the pump's,
/// and both take this one mutex. The pump used to hold it across a 250 ms
/// blocking receive and re-acquire it the instant it let go, and
/// `std::sync::Mutex` promises no fairness — so the operator's own gesture
/// could queue behind a background drain indefinitely. `Stage all` pressing
/// cleanly and nothing happening is what that looks like from the outside: the
/// `invoke` never reached the adapter, so nothing was ever sent.
///
/// Short enough that a command rarely waits for a whole round, and still an
/// order of magnitude better than the 250 ms wake latency it replaces.
pub const PUMP_DRAIN: Duration = Duration::from_millis(20);

/// The gap [`pump_core_events_once`] leaves the lock free after every round.
///
/// The guarantee belongs to the function rather than to its callers: a loop
/// that simply calls it in a `while` cannot starve a command, because the
/// function itself does not return until the lock has been unheld for this
/// long. A caller that could skip the gap would be a caller that could
/// reintroduce defect 35.
pub const PUMP_YIELD: Duration = Duration::from_millis(5);

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

/// Drains ordered Core events while a follow-up is still queued.
///
/// E3d defect 36. A `QueueFollowUp` the client has *sent* is not a follow-up
/// Core has *queued*: only Core's own `InputQueued` says that, and without a
/// route back to the queue's own pending the webview had no way to learn the
/// difference. The result carries the queue's outcome, never the submit's.
pub fn d1_queue_follow_up_poll(
    state: &DesktopState,
    lane_id: &str,
) -> Result<D1IntentResult, String> {
    with_adapter_mut(state, |adapter| {
        adapter.poll_queue_follow_up(lane_id, Duration::ZERO)
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

/* ---- D6 recovery ---- */

/// Re-reads Core's snapshot and returns the recovery surface, as the D6
/// `Reconnect` control does.
pub fn d6_recover(state: &DesktopState) -> Result<crate::D6RecoveryProjection, String> {
    with_adapter_mut(state, |adapter| {
        adapter.recover().map_err(|error| error.to_string())?;
        Ok(adapter.d6_recovery())
    })
}

/// The recovery surface with no Core traffic, including the host's own bounded
/// reconnect (E3d defect 37).
///
/// The cockpit reads this on every ordered wake, which is what makes the
/// countdown move without anyone pressing anything.
pub fn d6_recovery(state: &DesktopState) -> Result<crate::D6RecoveryProjection, String> {
    with_adapter(state, |adapter| Ok(adapter.d6_recovery()))
}

/* ---- the desktop event pump ---- */

/// One round of the desktop pump: drain ordered Core events off the UI thread
/// and report whether Core advanced.
///
/// `None` means no workspace is bound yet — the caller waits rather than
/// burning the CPU on an empty lock — and `Some(false)` means the drain came
/// back empty, was skipped because a command held the lock, or both. `Err`
/// means the lock is poisoned and the pump must stop.
///
/// Two rules keep the operator's own command ahead of this background drain
/// (E3d defect 35), and both live here rather than in the caller:
///
/// * the lock is *tried*, never waited for — a command holding it is
///   answering a gesture, and the events it does not drain this round are
///   still queued for the next one;
/// * the round does not return until the lock has been released for
///   [`PUMP_YIELD`], so a command blocked on it has an unlocked mutex to
///   acquire before the pump asks again.
///
/// It is also where a dropped connection is retried, because the pump is the
/// only thread that runs without an operator behind it (E3d defect 37).
pub fn pump_core_events_once(
    state: &DesktopState,
    event_timeout: Duration,
) -> Result<Option<bool>, ()> {
    let round = {
        let mut guard = match state.adapter.try_lock() {
            Ok(guard) => guard,
            // A command is in the adapter. Nothing is lost: Core's events stay
            // queued, and this round simply does not take them.
            Err(TryLockError::WouldBlock) => return Ok(Some(false)),
            Err(TryLockError::Poisoned(_)) => return Err(()),
        };
        match guard.as_mut() {
            Some(adapter) => {
                // The pump is the only thread that runs on its own, so a
                // connection nobody is holding open is its to get back
                // (E3d defect 37). Cheap when the connection is live: it
                // returns before doing anything.
                let reconnected = adapter.maintain_connection();
                Some(adapter.pump_events(event_timeout) || reconnected)
            }
            None => None,
        }
    };
    // Outside the guard, unconditionally: this is the window the command
    // thread needs, and a round that skipped the drain still has to leave it.
    thread::sleep(PUMP_YIELD);
    Ok(round)
}
