//! D16 Agent Board projections (`runtime.agent_session_control`, C14).
//!
//! The board is a centre-pane view over the Lane/session hierarchy. Four rules
//! shape the shapes below, and each exists because dropping it produces a
//! specific lie:
//!
//! 1. **A session is never detached from its Lane.** Sessions are grouped by
//!    their own `lane_id`, not by the Lane list: a Lane Core published no
//!    record for still gets a group carrying its id and saying the record is
//!    absent, because hiding the session would be worse than admitting the
//!    Lane row is thin. This is the functional design's rule (§4) and
//!    `D-PLANBOARD`'s.
//! 2. **Liveness is read from `status`, never from `active_session_ids`.** The
//!    C14 amendment is explicit: Core never prunes that list, not even for a
//!    cancel, so a client treating membership as "alive" would report every
//!    settled session as running.
//! 3. **A disabled control names the status that disabled it.** Core refuses a
//!    control the status does not allow with `session_status_<current>`; the
//!    board precomputes the same family so the operator reads the reason before
//!    pressing rather than after.
//! 4. **Switch model is two commands and is drawn as two.** The contract has no
//!    switch verb (`release-0.3.5-contract-design.md` §2): it is Kill or Cancel
//!    followed by `StartAgentSession { model, load_session_id }`. A single
//!    button would claim an atomicity Core does not have.

use serde::{Deserialize, Serialize};

/// The Core capability the whole screen rides.
pub const AGENT_SESSION_CONTROL_CAPABILITY: &str = "runtime.agent_session_control";

/// Core published no `runtime.agent_session_control`, so no control can be sent.
pub const D16_CAPABILITY_ABSENT_CODE: &str = "D16-NO-CAPABILITY";
/// This workspace has no Lane, so there is no session to supervise.
pub const D16_NO_LANE_CODE: &str = "D16-NO-LANE";
/// No session is selected, so the detail pane has nothing to act on.
pub const D16_NO_SESSION_CODE: &str = "D16-NO-SESSION";
/// The control has no runtime fact behind it at all; it is a register request.
pub const D16_NO_CORE_FACT_CODE: &str = "D16-NO-CORE-FACT";
/// The session is terminal, so nothing that acts on a live session applies.
pub const D16_SESSION_ENDED_CODE: &str = "D16-SESSION-ENDED";
/// Retry is offered on a terminal session that actually failed, not on a
/// completed one: Core accepts `Killed` as it accepts `Failed`, and a completed
/// session has nothing to retry.
pub const D16_SESSION_COMPLETED_CODE: &str = "D16-SESSION-COMPLETED";
/// Core published no model for this session, so a two-step switch has no
/// `load_session_id` target it could name honestly.
pub const D16_NO_MODEL_CODE: &str = "D16-NO-MODEL";

/// Prefix of the refusal family Core answers a badly-timed control with.
///
/// `crates/types/src/runtime.rs` documents `session_status_<current>`; the
/// board builds the same string so its label and Core's refusal are one
/// sentence rather than two that can drift.
pub const D16_STATUS_BLOCK_PREFIX: &str = "session_status_";

/// Operator control history for one session (`AgentSessionControlView`).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D16ControlProjection {
    pub paused_at: Option<u64>,
    pub resumed_at: Option<u64>,
    /// `cancelled` or `killed`; `None` while the session was never terminated
    /// by an operator command.
    pub terminated_by: Option<String>,
    /// Inputs held while paused, forwarded in order as each turn completes.
    pub held_input_count: u32,
}

/// One control in the session action row and whether it may be sent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D16ActionProjection {
    /// `pause` / `resume` / `kill` / `stop` / `retry` / `switch_model` /
    /// `downgrade_manual` / `ask_user`.
    pub kind: String,
    pub available: bool,
    /// Why not, in Core's own vocabulary where Core has one. `None` only when
    /// `available` is true.
    pub block: Option<String>,
    /// True for the two controls the design draws dashed: they have no runtime
    /// fact at all and are register requests rather than timing failures.
    pub request: bool,
}

/// One session under its Lane.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D16SessionProjection {
    pub session_id: String,
    pub lane_id: String,
    pub agent_id: String,
    /// `None` when Core published no model for this session — absence, never
    /// a default model name.
    pub model: Option<String>,
    pub task: String,
    /// Core's own snake_case status, including the two C14 additions.
    pub status: String,
    pub diagnostic: Option<String>,
    /// Absent until the session is first paused, resumed, or terminated.
    pub control: Option<D16ControlProjection>,
    /// Core's own `AgentSessionStatus::is_terminal` answer, projected so the
    /// view never re-decides which statuses have settled.
    pub terminal: bool,
    /// True only for `WaitingApproval`, which is the one status that blocks on
    /// a human. This is what the header count and the rail badge count.
    pub awaits_human: bool,
    pub actions: Vec<D16ActionProjection>,
}

/// One Lane row, with every session Core attributes to it nested under it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D16LaneProjection {
    pub lane_id: String,
    /// True when Core published an `AgentLaneRecord` for this id. False means
    /// the sessions named a Lane the view state does not carry: the group is
    /// still drawn, because detaching the sessions would be the larger lie.
    pub published: bool,
    pub summary: Option<String>,
    pub role: Option<String>,
    pub route: Option<String>,
    pub gate_strength: Option<String>,
    pub status: Option<String>,
    pub branch: Option<String>,
    pub worktree: Option<String>,
    pub sessions: Vec<D16SessionProjection>,
    /// Sessions under this Lane blocking on a human.
    pub awaiting: usize,
}

/// One published evidence row owned by the selected session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D16EvidenceProjection {
    pub id: String,
    pub kind: String,
    pub summary: String,
}

/// One Agent conversation message Core reduced for this session.
///
/// Separate from the audit rows rather than merged with them: the messages
/// carry no timestamp, so interleaving the two lists would invent an order
/// Core never published.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D16ConversationProjection {
    pub message_id: String,
    /// `user` / `assistant`, Core's own words.
    pub role: String,
    pub content: String,
}

/// The two explicit steps a model switch really is.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D16SwitchModelProjection {
    /// The session the second step would load, so the new session keeps the
    /// transcript. Always the selected session's own id.
    pub load_session_id: String,
    /// The model Core currently reports, which the second step's field starts
    /// from. `None` when Core published none.
    pub current_model: Option<String>,
    pub agent_id: String,
}

/// The right-hand detail pane for one selected session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D16DetailProjection {
    pub session_id: String,
    pub lane_id: String,
    pub conversation: Vec<D16ConversationProjection>,
    pub evidence: Vec<D16EvidenceProjection>,
    pub switch_model: Option<D16SwitchModelProjection>,
    /// Why a switch cannot be offered, when it cannot.
    pub switch_model_block: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D16AgentBoardProjection {
    /// Whether Core's handshake published `runtime.agent_session_control`.
    ///
    /// The *board* is still drawn without it — Lanes and sessions are ordinary
    /// `0.3.0` facts — but every control renders disabled and labelled, which
    /// is the `0.3.4` rule for a missing capability.
    pub capability_available: bool,
    pub lanes: Vec<D16LaneProjection>,
    pub total_lanes: usize,
    pub total_sessions: usize,
    pub awaiting_total: usize,
    /// The session the detail pane is showing. `None` when the board has no
    /// session at all, or when the shell's selection names one Core no longer
    /// publishes — in which case the board falls back to the first session that
    /// blocks on a human, then to the first session, and never to a row Core
    /// does not hold.
    pub selected_session_id: Option<String>,
    pub detail: Option<D16DetailProjection>,
}

/// One control the board sends.
///
/// `SwitchModel` is deliberately absent: the two-step flow sends `Kill` (or
/// `Stop`) and then `Start`, each as its own intent with its own command id, so
/// the operator's two decisions stay two facts in the audit log.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum D16Intent {
    Pause {
        session_id: String,
    },
    Resume {
        session_id: String,
    },
    Kill {
        session_id: String,
    },
    Stop {
        session_id: String,
    },
    Retry {
        session_id: String,
    },
    /// Step 2 of the model switch: start again on `model`, loading the ended
    /// session so the new one keeps the transcript.
    Start {
        session_id: String,
        model: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D16IntentResult {
    pub projection: D16AgentBoardProjection,
    pub pending_command_id: Option<String>,
    pub outcome: crate::D1OutcomeProjection,
}
