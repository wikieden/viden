//! D10 Lane monitor projections.
//!
//! One card per Core Lane across every project, plus the counts the monitor
//! header shows. Every field is a published Core fact: gate strength, status,
//! project binding, and task progress are read from the contract, never
//! derived from an agent label, a branch name, or a worktree path.

use serde::Serialize;

use crate::d2::D2UnavailableProjection;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D10LaneProjection {
    pub id: String,
    /// Core project binding, or `None` when Core published no owner binding.
    pub project_id: Option<String>,
    pub summary: String,
    pub role: String,
    pub route: String,
    /// First-class lane fact (`full`/`cooperative`/`containment`).
    pub gate_strength: String,
    pub mutation_policy: String,
    pub status: String,
    /// True for the Core statuses that actually block on a human.
    pub awaits_human: bool,
    pub branch: Option<String>,
    pub worktree: Option<String>,
    /// Progress of the Core task this lane names; `None` when it names none.
    pub progress: Option<u8>,
    pub agents: Vec<D10AgentProjection>,
    pub evidence: Vec<D10EvidenceProjection>,
    pub token_limit: Option<u64>,
    pub cost_limit_micro_usd: Option<u64>,
    /// Whether Core can account for this lane's model cost at all
    /// (`metered`/`blind`), read from `AgentRoute::cost_meterability`. A blind
    /// route runs an external process Core never sees a provider exchange for,
    /// so the monitor must never show it an inferred token or dollar figure.
    pub cost_meterability: String,
    /// The bounded process facts Core recorded for a cost-blind lane. `None`
    /// means Core observed no run, which is deliberately not the same fact as a
    /// measured zero — the monitor renders absence as absence.
    ///
    /// Absent for a metered lane by design: its cost surface is the token and
    /// cost ledger, and these facts exist to replace a cost figure that does
    /// not exist rather than to add a second one beside it.
    pub run_stats: Option<D10RunStatsProjection>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D10RunStatsProjection {
    /// Accumulated wall time, humanized on the host so every frontend reads the
    /// same duration string.
    pub wall_time: String,
    /// The same quantity Core recorded, unrounded, so the humanized string is
    /// never the only copy of the fact.
    pub wall_time_ms: u64,
    pub run_count: u64,
    pub diff_bytes: u64,
    /// Exit code of the most recent completed run. `None` is Core's own
    /// best-effort absence (force-kill, still running, or a tmux session with
    /// no exit-code channel) and the frontend labels it unknown rather than
    /// showing a zero.
    pub last_exit_code: Option<i32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D10AgentProjection {
    pub session_id: String,
    pub agent_id: String,
    pub model: Option<String>,
    pub status: String,
    /// Core's own `AgentSessionStatus::is_terminal` answer
    /// (`runtime.agent_session_control`, C14).
    ///
    /// The monitor's Pause and Kill share D16's rules, and this is what lets
    /// the row name `session_status_<current>` instead of offering a control
    /// Core would refuse. Liveness is never read from the Lane's
    /// `active_session_ids`, which Core does not prune when a session settles.
    pub terminal: bool,
    /// True for the three statuses `PauseAgentSession` accepts.
    pub pausable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D10EvidenceProjection {
    pub id: String,
    pub kind: String,
    pub summary: String,
}

/// Page size for the D10 event ticker.
///
/// Core clamps to `1..=MAX_AUDIT_PAGE_SIZE`, so this is a readability choice,
/// not a protocol bound: the ticker is an ambient strip of recent activity, not
/// a browsable history — D14 is where an operator pages the full timeline.
pub const D10_EVENT_TICKER_LIMIT: u32 = 50;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D10LaneMonitorProjection {
    /// Whether Core's handshake published `runtime.agent_session_control`
    /// (C14). Without it the card's Pause and Kill stay disabled and name the
    /// capability, exactly as they did before the capability existed.
    pub session_control_available: bool,
    pub total_lanes: usize,
    /// Distinct Core project ids across bound lanes.
    pub total_projects: usize,
    pub awaiting_total: usize,
    pub lanes: Vec<D10LaneProjection>,
    /// Design affordances with no Core fact behind them.
    pub unavailable: Vec<D2UnavailableProjection>,
}
