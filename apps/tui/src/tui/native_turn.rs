//! The one liveness fact this client can hold for a built-in-provider turn.
//!
//! Core publishes no turn-liveness fact for the native path: that provider
//! opens no Agent session, starts no task, and emits its `AssistantDelta`s with
//! no session id, so nothing in [`RuntimeViewState`](viden_core::RuntimeViewState)
//! says "a native turn is running". Core also never *settles* the unscoped
//! `assistant_stream` for that path — the reducer clears it only on a terminal
//! Agent-session fact — so the finished reply stays in the view for the rest of
//! the session. Reading that residue as liveness is what stopped E1's composer
//! from ever submitting again (`docs/core-0.3-compatibility.md`, open follow-up
//! 6, and follow-up 3 for the missing Core fact).
//!
//! This slot replaces the residue with a bounded window over facts the client
//! already has: its own dispatched `command_id`, and the ordered events Core
//! published back. It is deliberately narrow:
//!
//! - It holds no authoritative record and settles nothing. It only says whether
//!   *this* client is still waiting for the turn it submitted.
//! - It tracks exactly one turn, the one the composer submitted for the
//!   session-scoped owner. A Lane's turn is not tracked here: a Lane publishes
//!   its own lifecycle, session and task facts, which are owner-scoped and do
//!   not need a client-side stand-in.
//! - It is not a cancellation and not a completion claim. When the window
//!   closes the client stops *assuming* the turn runs; whether Core is still
//!   working is Core's answer, and Core gives it — a second
//!   `SubmitUserInput` for a busy owner comes back as `CommandRejected` with
//!   the supervisor's own reason.
//!
//! ## The exact residue window, and why it closes where it does
//!
//! Opened: when the composer dispatches `SubmitUserInput`, before any receipt.
//!
//! Closed by, in whichever order Core publishes them:
//!
//! - `CommandRejected` for this command id — nothing ran;
//! - the first `SnapshotUpdated` after the turn was dispatched. The supervisor
//!   emits the native turn's terminal event batch through
//!   `runtime_events_for_streaming_output(.., include_runtime_state = true)`,
//!   whose prefix always begins with `SnapshotUpdated`
//!   (`crates/runtime/src/runtime_contract.rs`, `runtime_state_events`), while
//!   the intermediate approval-boundary batches carry no prefix at all. So one
//!   dispatched native turn yields exactly one `SnapshotUpdated`, at the head
//!   of the batch that ends it;
//! - a snapshot replacement, which discards the ordered stream this
//!   correlation was reading.
//!
//! `SnapshotUpdated` is not a turn-liveness fact — that is the Core gap — and
//! three operator commands publish one of their own: `SetWorkMode`,
//! `SetPermissionLevel`, and `SelectModel`. Changing mode, permission level or
//! model *while a native turn streams* therefore closes this window early. The
//! failure is bounded and stated rather than silent: the next prompt is
//! submitted instead of queued, and Core answers it with
//! `active runtime job … is already running`, which the transcript renders. The
//! opposite failure — a window that never closes — is the one this replaces,
//! and it is the one that cannot be recovered from inside a session.

use viden_core::{RuntimeEvent, RuntimeEventKind};

/// The composer's own in-flight native turn, or nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct NativeTurnSlot {
    in_flight: Option<String>,
}

impl NativeTurnSlot {
    /// Records that this client dispatched a native turn under `command_id`.
    ///
    /// A second dispatch replaces the first: Core admits one active job per
    /// owner, so two ids for one owner cannot both be live, and the newer one
    /// is the one whose answer is still coming.
    pub(super) fn begin(&mut self, command_id: impl Into<String>) {
        self.in_flight = Some(command_id.into());
    }

    /// Whether this client is still waiting for the turn it submitted.
    pub(super) fn is_in_flight(&self) -> bool {
        self.in_flight.is_some()
    }

    #[cfg(test)]
    pub(super) fn command_id(&self) -> Option<&str> {
        self.in_flight.as_deref()
    }

    /// Stops waiting without claiming anything about Core.
    ///
    /// Used when a snapshot replacement discards the ordered stream this
    /// correlation was reading: the events that would have closed the window
    /// are not coming, and latching on a stream that no longer exists is the
    /// defect this slot exists to prevent.
    pub(super) fn reset(&mut self) {
        self.in_flight = None;
    }

    /// Reconciles one ordered Core event against the in-flight turn.
    ///
    /// Returns `true` when the window closed, so a caller may redraw.
    pub(super) fn observe_event(&mut self, event: &RuntimeEvent) -> bool {
        let Some(pending) = self.in_flight.as_deref() else {
            return false;
        };
        match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, .. } if command_id == pending => {
                self.in_flight = None;
                true
            }
            // A receipt is not an outcome; the turn is still in flight.
            RuntimeEventKind::CommandAccepted { .. } => false,
            RuntimeEventKind::SnapshotUpdated { .. } => {
                self.in_flight = None;
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use viden_core::{RuntimeCommand, RuntimeSnapshot, WorkMode};
    use viden_types::{PermissionLevel, PermissionMode};

    fn event(sequence: u64, kind: RuntimeEventKind) -> RuntimeEvent {
        RuntimeEvent {
            sequence,
            timestamp: Some(sequence),
            kind,
        }
    }

    fn snapshot_updated() -> RuntimeEvent {
        event(
            9,
            RuntimeEventKind::SnapshotUpdated {
                snapshot: RuntimeSnapshot {
                    cwd: std::path::PathBuf::from("/workspace"),
                    provider_family: "fallback".to_string(),
                    model_label: "test-local".to_string(),
                    work_mode: WorkMode::Build,
                    permission_mode: PermissionMode::Default,
                    permission_level: PermissionLevel::Ask,
                    config_summary: "fixture".to_string(),
                    loaded_config_files: Vec::new(),
                    startup_overrides: Vec::new(),
                    ui_preferences: Default::default(),
                },
            },
        )
    }

    #[test]
    fn acceptance_is_a_receipt_and_keeps_the_turn_in_flight() {
        let mut slot = NativeTurnSlot::default();
        slot.begin("tui-1");

        assert!(!slot.observe_event(&event(
            1,
            RuntimeEventKind::CommandAccepted {
                command_id: "tui-1".to_string(),
                command: RuntimeCommand::SubmitUserInput {
                    content: "explain the loader".to_string(),
                },
            },
        )));
        assert_eq!(slot.command_id(), Some("tui-1"));
    }

    #[test]
    fn the_terminal_batch_closes_the_window_so_stream_residue_cannot_latch() {
        let mut slot = NativeTurnSlot::default();
        slot.begin("tui-1");

        assert!(slot.observe_event(&snapshot_updated()));
        assert!(!slot.is_in_flight());
        // Idempotent: a later batch settles nothing that is already settled.
        assert!(!slot.observe_event(&snapshot_updated()));
    }

    #[test]
    fn a_refusal_of_this_command_closes_the_window_and_another_id_does_not() {
        let mut slot = NativeTurnSlot::default();
        slot.begin("tui-1");

        assert!(!slot.observe_event(&event(
            1,
            RuntimeEventKind::CommandRejected {
                command_id: "tui-2".to_string(),
                reason: "someone else's refusal".to_string(),
            },
        )));
        assert!(slot.is_in_flight());

        assert!(slot.observe_event(&event(
            2,
            RuntimeEventKind::CommandRejected {
                command_id: "tui-1".to_string(),
                reason: "active runtime job `tui-0` is already running".to_string(),
            },
        )));
        assert!(!slot.is_in_flight());
    }

    #[test]
    fn a_stream_replacement_stops_the_wait_rather_than_latching() {
        let mut slot = NativeTurnSlot::default();
        slot.begin("tui-1");

        slot.reset();

        assert!(!slot.is_in_flight());
    }
}
