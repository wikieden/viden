//! Agent session control from the Lane view
//! (`runtime.agent_session_control`, C14, plus the unchanged cancel and
//! retry).
//!
//! This module is pure. It answers which control a session in a given status
//! actually offers and what each one sends, and it correlates the one
//! in-flight control command against Core's own answer. It dispatches nothing
//! and decides nothing about a session.
//!
//! Three rules shape it:
//!
//! - **Status gates the key, and Core owns the gate.** A control that Core's
//!   contract says cannot apply to the session's current status is listed and
//!   labelled with that status rather than hidden, and it sends nothing. The
//!   status is read from the session record on the keypress, so a session that
//!   settled between render and keypress is never asked the wrong question.
//! - **A pause is never simulated locally.** There is no client-side input
//!   withholding here: an input this client sat on would be invisible to Core,
//!   to replay, and to every other client on the same session. `Pause` is
//!   Core's command and `held_input_count` is Core's count.
//! - **Switch model is not a command.** The contract composes it from a kill
//!   (or cancel) and a fresh `StartAgentSession` carrying the new `model` and
//!   the previous `load_session_id`; the row says so rather than offering a
//!   control Core does not publish.

use viden_core::{AgentSessionStatus, AgentSessionView, RuntimeCommand, RuntimeEvent};

use super::glyphs::GlyphSet;

/// The capability that publishes pause, resume, and kill.
pub(super) const SESSION_CONTROL_CAPABILITY: &str = "runtime.agent_session_control";

/// One control an Agent session row offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SessionControl {
    Pause,
    Resume,
    Kill,
    /// `CancelAgentSession`, unchanged and available without the extension.
    Stop,
    /// `RetryAgentSession`, unchanged; it accepts `Killed` exactly as it
    /// accepts `Failed`.
    Retry,
}

impl SessionControl {
    pub(super) const ALL: [Self; 5] = [
        Self::Pause,
        Self::Resume,
        Self::Kill,
        Self::Stop,
        Self::Retry,
    ];

    /// The key that sends this control from the Lane view.
    ///
    /// `r` stays Retry, which it has been since the ACP picker existed, so
    /// Resume takes `u`: silently re-pointing a key an operator already has in
    /// their fingers at a different command is how a resume becomes a restart.
    pub(super) const fn key(self) -> char {
        match self {
            Self::Pause => 'p',
            Self::Resume => 'u',
            Self::Kill => 'k',
            Self::Stop => 's',
            Self::Retry => 'r',
        }
    }

    pub(super) const fn label_key(self) -> &'static str {
        match self {
            Self::Pause => "session.control.pause",
            Self::Resume => "session.control.resume",
            Self::Kill => "session.control.kill",
            Self::Stop => "session.control.stop",
            Self::Retry => "session.control.retry",
        }
    }

    /// Whether this control needs `runtime.agent_session_control`.
    ///
    /// Stop and Retry predate it and stay live without it, which is exactly
    /// what keeps "this Core cannot pause" distinguishable from "this session
    /// cannot be stopped".
    pub(super) const fn needs_capability(self) -> bool {
        matches!(self, Self::Pause | Self::Resume | Self::Kill)
    }

    /// Whether Core's contract accepts this control from `status`.
    ///
    /// Every arm names the statuses Core's contract lists, so a status added
    /// later is a compile error here rather than a control quietly offered for
    /// a state this build has never reasoned about.
    pub(super) fn applies_to(self, status: AgentSessionStatus) -> bool {
        match self {
            // Accepted from `Starting`, `Running`, and `WaitingApproval`.
            Self::Pause => matches!(
                status,
                AgentSessionStatus::Starting
                    | AgentSessionStatus::Running
                    | AgentSessionStatus::WaitingApproval
            ),
            // Accepted from `Paused` only.
            Self::Resume => status == AgentSessionStatus::Paused,
            // Accepted from any non-terminal status, `Paused` included.
            Self::Kill | Self::Stop => !is_terminal(status),
            // `Killed` is retryable exactly as `Failed` is.
            Self::Retry => matches!(
                status,
                AgentSessionStatus::Failed
                    | AgentSessionStatus::Cancelled
                    | AgentSessionStatus::Killed
            ),
        }
    }

    pub(super) fn command(self, session_id: &str) -> RuntimeCommand {
        match self {
            Self::Pause => RuntimeCommand::PauseAgentSession {
                session_id: session_id.to_string(),
            },
            Self::Resume => RuntimeCommand::ResumeAgentSession {
                session_id: session_id.to_string(),
            },
            Self::Kill => RuntimeCommand::KillAgentSession {
                session_id: session_id.to_string(),
            },
            // The one control whose command takes a bare `String` rather than
            // a `SessionId`; unchanged from before this capability.
            Self::Stop => RuntimeCommand::CancelAgentSession {
                session_id: session_id.to_string(),
            },
            Self::Retry => RuntimeCommand::RetryAgentSession {
                session_id: session_id.to_string(),
            },
        }
    }
}

/// Whether the session has settled.
///
/// The four terminal statuses are named rather than derived from a negation,
/// so a status added to `AgentSessionStatus` later fails this match at compile
/// time instead of silently counting as alive and offering Kill for a session
/// that has already ended.
fn is_terminal(status: AgentSessionStatus) -> bool {
    matches!(
        status,
        AgentSessionStatus::Completed
            | AgentSessionStatus::Failed
            | AgentSessionStatus::Cancelled
            | AgentSessionStatus::Killed
    )
}

/// The registered glyph for one session status.
///
/// Every glyph is from the design package's TUI status vocabulary
/// (`docs/viden-design/Viden/docs/DESIGN-REF.md`, "TUI 字形词表"): a paused
/// session takes the gate glyph, which is the vocabulary's pause mark, and a
/// killed one takes the fail glyph, which is what an operator-terminated
/// session is.
pub(super) fn status_glyph(glyphs: GlyphSet, status: AgentSessionStatus) -> &'static str {
    match status {
        AgentSessionStatus::Starting => glyphs.wait,
        AgentSessionStatus::Running => glyphs.run,
        AgentSessionStatus::WaitingApproval => glyphs.your_turn,
        AgentSessionStatus::Completed => glyphs.done,
        AgentSessionStatus::Paused => glyphs.gate,
        AgentSessionStatus::Failed | AgentSessionStatus::Killed => glyphs.fail,
        AgentSessionStatus::Cancelled => glyphs.warning,
    }
}

/// One control command this client sent and has not seen answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PendingSessionControl {
    pub(super) command_id: String,
    pub(super) control: SessionControl,
    pub(super) session_id: String,
}

/// How Core answered one control command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SessionControlSettlement {
    /// A pre-effect refusal, carrying Core's own reason — the
    /// `session_status_<current>` family among them. Never composed here: only
    /// Core knows which status refused.
    Rejected {
        control: SessionControl,
        session_id: String,
        reason: String,
    },
    /// Core admitted the command. The session's own facts follow and are read
    /// from `RuntimeViewState`; this is a receipt, not an outcome.
    Accepted {
        control: SessionControl,
        session_id: String,
    },
}

/// TUI-local correlation for one in-flight session control command.
///
/// A separate slot from the supervision and source-control machines because a
/// pause and a merge-gate decision answer different questions and must not
/// block each other.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct SessionControlMachine {
    pending: Option<PendingSessionControl>,
}

impl SessionControlMachine {
    pub(super) fn begin(
        &mut self,
        command_id: impl Into<String>,
        control: SessionControl,
        session_id: impl Into<String>,
    ) {
        self.pending = Some(PendingSessionControl {
            command_id: command_id.into(),
            control,
            session_id: session_id.into(),
        });
    }

    /// Test-only: production code reads the settlement rather than the slot,
    /// because a control's outcome is Core's answer and not this client's
    /// memory of having asked.
    #[cfg(test)]
    pub(super) fn pending(&self) -> Option<&PendingSessionControl> {
        self.pending.as_ref()
    }

    /// Reconciles one ordered Core event against the in-flight control.
    ///
    /// Only the two command-correlated facts settle it. The session's own
    /// status changes are read from the view instead, because a status fact
    /// carries no command id and attributing one to this client's command
    /// would claim credit for whatever moved the session.
    pub(super) fn observe_event(
        &mut self,
        event: &RuntimeEvent,
    ) -> Option<SessionControlSettlement> {
        let pending = self.pending.as_ref()?;
        match &event.kind {
            viden_core::RuntimeEventKind::CommandRejected { command_id, reason }
                if command_id == &pending.command_id =>
            {
                let control = pending.control;
                let session_id = pending.session_id.clone();
                self.pending = None;
                Some(SessionControlSettlement::Rejected {
                    control,
                    session_id,
                    reason: reason.clone(),
                })
            }
            viden_core::RuntimeEventKind::CommandAccepted { command_id, .. }
            | viden_core::RuntimeEventKind::LaneCommandAccepted { command_id, .. }
                if command_id == &pending.command_id =>
            {
                let control = pending.control;
                let session_id = pending.session_id.clone();
                self.pending = None;
                Some(SessionControlSettlement::Accepted {
                    control,
                    session_id,
                })
            }
            _ => None,
        }
    }
}

/// The facts a session row shows beside its status.
///
/// `held_input_count` is the only honest source for "typed but not sent", and
/// `terminated_by` is the only thing that tells a cancel from a kill once both
/// have settled. Both are absent until Core publishes `control`, and absence
/// is rendered as absence rather than as zero.
pub(super) fn control_suffix(state: &super::state::TuiState, session: &AgentSessionView) -> String {
    let Some(control) = session.control.as_ref() else {
        return String::new();
    };
    let mut suffix = String::new();
    if control.held_input_count > 0 {
        suffix.push_str(&super::i18n::translate(
            state,
            "session.control.held",
            &[("count", &control.held_input_count.to_string())],
        ));
    }
    if let Some(termination) = control.terminated_by {
        let key = match termination {
            viden_types::AgentSessionTermination::Cancelled => "session.control.terminated.cancel",
            viden_types::AgentSessionTermination::Killed => "session.control.terminated.kill",
        };
        suffix.push_str(&super::i18n::text(state, key));
    }
    suffix
}

#[cfg(test)]
mod tests {
    use super::*;
    use viden_types::{AgentSessionControlView, AgentSessionTermination, CapabilityId};

    fn session(status: AgentSessionStatus) -> AgentSessionView {
        AgentSessionView {
            session_id: "session_1".to_string(),
            lane_id: "L1".to_string(),
            agent_id: "codex-acp".to_string(),
            model: None,
            status,
            owner: Default::default(),
            task: "refactor".to_string(),
            output: None,
            diagnostic: None,
            control: None,
        }
    }

    #[test]
    fn pause_resume_and_kill_follow_cores_own_status_gate() {
        for status in [
            AgentSessionStatus::Starting,
            AgentSessionStatus::Running,
            AgentSessionStatus::WaitingApproval,
        ] {
            assert!(SessionControl::Pause.applies_to(status), "{status:?}");
            assert!(!SessionControl::Resume.applies_to(status), "{status:?}");
            assert!(SessionControl::Kill.applies_to(status), "{status:?}");
        }
        assert!(SessionControl::Resume.applies_to(AgentSessionStatus::Paused));
        assert!(!SessionControl::Pause.applies_to(AgentSessionStatus::Paused));
        assert!(SessionControl::Kill.applies_to(AgentSessionStatus::Paused));
        for status in [
            AgentSessionStatus::Completed,
            AgentSessionStatus::Failed,
            AgentSessionStatus::Cancelled,
            AgentSessionStatus::Killed,
        ] {
            assert!(!SessionControl::Kill.applies_to(status), "{status:?}");
            assert!(!SessionControl::Pause.applies_to(status), "{status:?}");
        }
    }

    #[test]
    fn retry_accepts_killed_exactly_as_it_accepts_failed() {
        assert!(SessionControl::Retry.applies_to(AgentSessionStatus::Killed));
        assert!(SessionControl::Retry.applies_to(AgentSessionStatus::Failed));
        assert!(!SessionControl::Retry.applies_to(AgentSessionStatus::Running));
    }

    #[test]
    fn every_control_sends_the_frozen_core_command_for_the_session() {
        assert_eq!(
            SessionControl::Pause.command("session_1"),
            RuntimeCommand::PauseAgentSession {
                session_id: "session_1".to_string()
            }
        );
        assert_eq!(
            SessionControl::Resume.command("session_1"),
            RuntimeCommand::ResumeAgentSession {
                session_id: "session_1".to_string()
            }
        );
        assert_eq!(
            SessionControl::Kill.command("session_1"),
            RuntimeCommand::KillAgentSession {
                session_id: "session_1".to_string()
            }
        );
        assert_eq!(
            SessionControl::Stop.command("session_1"),
            RuntimeCommand::CancelAgentSession {
                session_id: "session_1".to_string()
            }
        );
    }

    #[test]
    fn stop_and_retry_stay_live_without_the_extension() {
        assert!(!SessionControl::Stop.needs_capability());
        assert!(!SessionControl::Retry.needs_capability());
        assert!(SessionControl::Pause.needs_capability());
        assert!(SessionControl::Resume.needs_capability());
        assert!(SessionControl::Kill.needs_capability());
    }

    #[test]
    fn paused_and_killed_take_registered_glyphs_from_the_design_vocabulary() {
        let glyphs = GlyphSet::new(true);

        assert_eq!(status_glyph(glyphs, AgentSessionStatus::Paused), "⏸");
        assert_eq!(status_glyph(glyphs, AgentSessionStatus::Killed), "✗");
        assert_eq!(status_glyph(glyphs, AgentSessionStatus::Running), "▶");
    }

    #[test]
    fn held_inputs_and_the_termination_are_shown_and_absence_is_not_zero() {
        let mut state = super::super::state::TuiState::default();
        state
            .capabilities
            .insert(CapabilityId(SESSION_CONTROL_CAPABILITY.to_string()));
        let mut paused = session(AgentSessionStatus::Paused);
        assert_eq!(control_suffix(&state, &paused), "");

        paused.control = Some(AgentSessionControlView {
            paused_at: Some(1),
            resumed_at: None,
            terminated_by: None,
            held_input_count: 2,
        });
        assert!(control_suffix(&state, &paused).contains('2'));

        let mut killed = session(AgentSessionStatus::Killed);
        killed.control = Some(AgentSessionControlView {
            paused_at: Some(1),
            resumed_at: Some(2),
            terminated_by: Some(AgentSessionTermination::Killed),
            held_input_count: 0,
        });
        let suffix = control_suffix(&state, &killed);
        assert!(suffix.contains("ended by kill"), "{suffix}");
        assert!(!suffix.contains('0'), "absence is not zero: {suffix}");
    }

    #[test]
    fn a_status_refusal_carries_cores_own_reason() {
        let mut machine = SessionControlMachine::default();
        machine.begin("tui-3", SessionControl::Resume, "session_1");

        let settlement = machine.observe_event(&RuntimeEvent {
            kind: viden_core::RuntimeEventKind::CommandRejected {
                command_id: "tui-3".to_string(),
                reason: "session_status_running".to_string(),
            },
            sequence: 4,
            timestamp: Some(0),
        });

        assert_eq!(
            settlement,
            Some(SessionControlSettlement::Rejected {
                control: SessionControl::Resume,
                session_id: "session_1".to_string(),
                reason: "session_status_running".to_string(),
            })
        );
        assert!(machine.pending().is_none());
    }
}
