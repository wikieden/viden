use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum D6ConnectionState {
    Disconnected,
    Connecting,
    Live,
    Recovering,
    Incompatible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum D6State {
    Live,
    Empty,
    Connecting,
    Disconnected,
    ProviderError,
    AgentStopped,
    ContextOverflow,
    GateQueueClear,
    IncompatibleSchema,
    MissingFeatureCapability,
    EventGap,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D6ActionProjection {
    pub kind: &'static str,
    pub available: bool,
    pub code: &'static str,
    /// Exact Core ACP session this action acts on, when Core published one.
    ///
    /// Only `restart` names a session. The intent replays this id instead of
    /// rebuilding an identity from display text, so a recovery action can
    /// never be routed at a session Core did not publish.
    pub session_id: Option<String>,
    /// Exact Core Lane this action acts on, when Core published one.
    ///
    /// `restart` names the Lane that owns the session; `close_lane` names the
    /// Lane it stops.
    pub lane_id: Option<String>,
}

impl D6ActionProjection {
    /// A recovery action with no Core target to name.
    pub(crate) fn untargeted(kind: &'static str, available: bool, code: &'static str) -> Self {
        Self {
            kind,
            available,
            code,
            session_id: None,
            lane_id: None,
        }
    }
}

/// One D6 recovery action the operator asked Core to perform.
///
/// Every variant maps to exactly one existing `RuntimeCommand`; D6 owns no
/// private recovery path. `inspect` is absent on purpose: it expands facts the
/// projection already carries and never reaches Core.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum D6Intent {
    /// Re-runs a failed or cancelled ACP session (`RuntimeCommand::RetryAgentSession`).
    Restart { session_id: String },
    /// Stops an active Lane (`RuntimeCommand::StopLane`).
    CloseLane { lane_id: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D6IntentResult {
    pub projection: D6RecoveryProjection,
    /// Set while Core has not yet published the receipt for this command.
    pub pending_command_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D6RecoveryProjection {
    pub connection: D6ConnectionState,
    pub state: D6State,
    pub detail: Option<String>,
    pub hint: Option<String>,
    pub recoverable: bool,
    pub business_success_blocked: bool,
    pub used_tokens: Option<u64>,
    pub hard_token_limit: Option<u64>,
    pub missing_capabilities: Vec<String>,
    pub actions: Vec<D6ActionProjection>,
    /// The host's own bounded reconnect, or `None` when the connection is live
    /// or nothing has been attempted (E3d defect 37).
    ///
    /// Core publishes no reconnect command — that gap is `GUI-CORE-003`, and
    /// the recovery surface's `Reconnect` control stays disabled and labelled
    /// with it. What this carries is the *host's* retry of the handshake and
    /// snapshot it already owns: how many attempts have run, how long until the
    /// next one, and whether it has stopped trying. Without it the cockpit
    /// renders `Establishing the versioned Core connection.` with no end, which
    /// is what E3d sat in for the rest of that session.
    pub reconnect: Option<D6ReconnectProjection>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D6ReconnectProjection {
    pub attempts: u32,
    pub max_attempts: u32,
    /// Milliseconds until the next attempt, or `None` when one is running or
    /// the host has given up. The surface renders a countdown from it rather
    /// than a spinner.
    pub next_attempt_in_ms: Option<u64>,
    pub gave_up: bool,
    /// The last transport failure, verbatim, so the operator is told what the
    /// host saw instead of only that something is wrong.
    pub last_error: Option<String>,
}
