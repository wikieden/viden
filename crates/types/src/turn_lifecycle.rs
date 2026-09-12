//! Turn liveness for every execution path (`runtime.turn_lifecycle`).
//!
//! Before this capability Core published a terminal fact for exactly one kind
//! of turn: an Agent session, which ends in `AgentSessionCompleted`,
//! `AgentSessionFailed`, or a cancelled `AgentSessionUpdated`. A built-in
//! native turn had none. Clients therefore guessed at liveness from display
//! residue — the TUI from `assistant_stream` still holding a finished turn's
//! text, the GUI from `turn_id` heuristics — and two things followed from that
//! guess: a composer that never stopped queueing, and a session-level queue
//! that was published and never drained.
//!
//! Three rules shape the facts below:
//!
//! 1. **A turn is bracketed, not inferred.** [`TurnStarted`] and
//!    [`TurnFinished`] are a matched pair on every exit — completion, failure,
//!    and cancellation alike. A client reads liveness from
//!    `RuntimeViewState.active_turns` and never from the presence of text.
//!
//!    [`TurnStarted`]: crate::RuntimeEventKind::TurnStarted
//!    [`TurnFinished`]: crate::RuntimeEventKind::TurnFinished
//! 2. **The outcome is named, because the three ends are not the same.** A
//!    completed turn drains the queue behind it; a failed or cancelled one
//!    leaves the queue exactly where it is, so nothing runs behind a turn that
//!    did not finish its work. A client that only knew "the turn ended" could
//!    not render that difference, and an operator would watch a queue behind a
//!    failure and not know whether it was about to run.
//! 3. **A turn names its own origin.** [`TurnSource`] separates a prompt the
//!    operator just typed from one their earlier `QueueFollowUp` put in line
//!    and from an Agent session run. The second is the case a client cannot
//!    otherwise explain: text appears in the transcript that nobody typed just
//!    now, and without the source that reads as Core inventing work.

use serde::{Deserialize, Serialize};

use crate::{RuntimeOwner, SessionId};

/// Character bound for the reason on a failed turn.
///
/// A failure reason is a projection string, not a log line: it is carried on
/// the wire, reduced into client state, and rendered beside a composer. The
/// bound exists so a provider error carrying a whole request body cannot grow
/// the event stream, and the same sanitizer every other projection string uses
/// runs first, so a home path or a key-shaped token never reaches a client
/// through this field.
pub const MAX_TURN_FAILURE_REASON_CHARS: usize = 500;

/// One turn Core is running right now.
///
/// `owner` is the full owner of the work: `lane_id: None` for the
/// session-scoped composer turn, and the Agent session's own owner for an ACP
/// run. A client matches its composer against this owner rather than against
/// the shape of the turn, because the same fact serves both paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnView {
    pub turn_id: String,
    pub owner: RuntimeOwner,
    pub source: TurnSource,
    pub started_at: u64,
}

/// Where a turn came from.
///
/// `#[non_exhaustive]`: a future source — a scheduled turn, a resumed one —
/// must not break a client's match arms, and a client that cannot name the
/// source still knows a turn is live.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum TurnSource {
    /// A prompt the operator submitted through `SubmitUserInput`.
    UserInput,
    /// A prompt an earlier `QueueFollowUp` put in line, now drained by Core
    /// after a completed turn.
    ///
    /// `input_id` is the exact id the `InputQueued` and `InputDequeued` facts
    /// carry, so a client can tell which of several queued prompts is running
    /// rather than guess from the text.
    QueuedInput { input_id: String },
    /// One Agent session run.
    AgentSession { session_id: SessionId },
}

/// How a turn ended.
///
/// `#[non_exhaustive]` for the same reason as [`TurnSource`]. A client that
/// meets an unmodeled outcome must fall back to "ended, cause unknown" rather
/// than to "completed": the queue-drain rule below hangs on this value, and a
/// client that read an unknown outcome as success would tell an operator their
/// queue is about to run when Core has already decided it is not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum TurnOutcome {
    /// The turn ran to the end of its work. Only this outcome drains the
    /// session queue.
    Completed,
    /// The turn stopped on an error. Completed tool facts published before the
    /// stop stand; the queue behind it is kept.
    Failed { reason: String },
    /// The owner stopped the turn through `CancelActiveTurn`. The queue behind
    /// it is kept, because a cancellation is a decision about this turn and
    /// not an instruction to run the next one.
    Cancelled,
}

impl TurnOutcome {
    /// The failure outcome, with the reason sanitized and bounded.
    ///
    /// Always use this rather than constructing `Failed` directly: the reason
    /// reaching this constructor is a provider, tool, or engine error string,
    /// which is exactly the kind of text that carries absolute paths and
    /// secret-shaped tokens.
    pub fn failed(reason: impl AsRef<str>) -> Self {
        Self::Failed {
            reason: crate::runtime::sanitize_runtime_text(
                reason.as_ref(),
                MAX_TURN_FAILURE_REASON_CHARS,
            ),
        }
    }

    /// Whether this outcome permits the session queue to drain behind the
    /// turn.
    ///
    /// Named rather than inlined as a `matches!` at each producer, because
    /// there are three producers and one rule: nothing runs behind a turn that
    /// did not complete.
    pub fn drains_queue(&self) -> bool {
        matches!(self, Self::Completed)
    }
}
