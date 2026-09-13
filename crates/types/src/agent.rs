use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    AgentLaneId, AgentNextAction, AgentTaskId, AgentTaskStatus, CapabilityId, EvidenceId,
    RuntimeOwner, SessionId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Planner,
    Coder,
    Reviewer,
    Tester,
    DocWriter,
    Researcher,
    ReleaseOperator,
}

impl AgentRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planner => "planner",
            Self::Coder => "coder",
            Self::Reviewer => "reviewer",
            Self::Tester => "tester",
            Self::DocWriter => "doc_writer",
            Self::Researcher => "researcher",
            Self::ReleaseOperator => "release_operator",
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        match input.trim().replace('-', "_").as_str() {
            "planner" => Some(Self::Planner),
            "coder" => Some(Self::Coder),
            "reviewer" => Some(Self::Reviewer),
            "tester" => Some(Self::Tester),
            "doc_writer" | "documenter" => Some(Self::DocWriter),
            "researcher" => Some(Self::Researcher),
            "release_operator" | "release" => Some(Self::ReleaseOperator),
            _ => None,
        }
    }
}

impl std::fmt::Display for AgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRoute {
    BuiltIn,
    Acp,
    Terminal,
    Tmux,
}

/// Whether Core can account for a route's model cost at all.
///
/// This is a contract fact, not a display hint: a `Blind` route must never be
/// shown with an inferred token or dollar figure, because Core observes no
/// provider call for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostMeterability {
    /// Core sees the provider exchange and can attribute tokens and money.
    Metered,
    /// Core sees only process facts; model cost is unobservable here.
    Blind,
}

impl AgentRoute {
    /// Whether Core can meter this route's model cost. Terminal and tmux lanes
    /// run arbitrary external processes whose token/dollar cost is invisible to
    /// Core, so their cost surface is exactly the bounded run facts in
    /// [`LaneRunStats`] — never a fabricated token estimate.
    pub fn cost_meterability(&self) -> CostMeterability {
        match self {
            Self::BuiltIn | Self::Acp => CostMeterability::Metered,
            Self::Terminal | Self::Tmux => CostMeterability::Blind,
        }
    }
}

/// Bounded, directly observed run facts for one lane.
///
/// These are the only quantities Core publishes for a cost-blind route. Every
/// field is measured from a local process or patch effect; none of them is
/// derived from a provider, a token count, or a price table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LaneRunStats {
    /// Accumulated wall time across completed runs, milliseconds.
    pub wall_time_ms: u64,
    /// Number of observed runtime starts.
    pub run_count: u64,
    /// Accumulated bytes of successfully applied unified diffs.
    pub diff_bytes: u64,
    /// Exit code of the most recent completed run. Best-effort: `None` when the
    /// process was force-killed, still running at observation, or ran under
    /// tmux (`kill-session` leaves no exit-code channel).
    pub last_exit_code: Option<i32>,
}

/// Where an adapter definition comes from. This public view deliberately does
/// not expose plugin-host command lines or environment references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentAdapterSource {
    BuiltIn,
    Registry,
    LocalCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentAvailability {
    Available,
    NeedsInstall,
    NeedsAuth,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentAuthState {
    Unknown,
    Ready,
    LoggedOut,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStartability {
    Ready,
    /// An agent whose startability has not been probed yet is the safe default:
    /// the client must not offer to start what Core has not confirmed.
    #[default]
    ProbeRequired,
    InstallRequired,
    AuthenticationRequired,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionStatus {
    Starting,
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Cancelled,
    /// Alive, with no prompt in flight and no input forwarded until resumed
    /// (`runtime.agent_session_control`).
    ///
    /// Neither ACP nor the Codex app-server has a pause verb, so this is a
    /// Core-side state over the session's input path: the in-flight prompt was
    /// cancelled, but the resident session and its resume handle are kept.
    Paused,
    /// Terminated without a cancel grace; terminal, and retryable like
    /// [`AgentSessionStatus::Failed`].
    Killed,
}

impl AgentSessionStatus {
    /// Whether no further work runs under this status without an explicit
    /// restart.
    ///
    /// The honesty rule for the two 0.3.5 statuses lives here so no exhaustive
    /// match has to re-decide it: a paused session is alive and still holds its
    /// adapter, and a killed one is settled exactly as a cancelled one is.
    pub fn is_terminal(self) -> bool {
        match self {
            Self::Completed | Self::Failed | Self::Cancelled | Self::Killed => true,
            Self::Starting | Self::Running | Self::WaitingApproval | Self::Paused => false,
        }
    }
}

/// Why a terminal Agent session stopped (`runtime.agent_session_control`).
///
/// Cancel and kill both end a session, but they are not the same operator act:
/// cancel gave the adapter its cooperative grace, kill did not. A client that
/// cannot tell them apart cannot explain what happened to the work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionTermination {
    Cancelled,
    Killed,
}

/// The control history of one Agent session.
///
/// Published only once a session has actually been paused, resumed, or
/// terminated by the operator, so a session that was never controlled still
/// encodes to the bytes it did before this capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AgentSessionControlView {
    pub paused_at: Option<u64>,
    pub resumed_at: Option<u64>,
    pub terminated_by: Option<AgentSessionTermination>,
    /// Inputs accepted while paused and held; forwarded in order on resume,
    /// discarded and named in the diagnostic on kill.
    pub held_input_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentAdapterView {
    pub agent_id: String,
    pub display_name: String,
    pub route: AgentRoute,
    pub source: AgentAdapterSource,
    pub availability: AgentAvailability,
    pub auth_state: AgentAuthState,
    /// Core-owned readiness classification. Legacy adapter records default to
    /// probe-required so a frontend can never infer permission to start.
    #[serde(default, skip_serializing_if = "is_probe_required")]
    pub startability: AgentStartability,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<CapabilityId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
}

fn is_probe_required(value: &AgentStartability) -> bool {
    *value == AgentStartability::ProbeRequired
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSessionRequest {
    pub lane_id: AgentLaneId,
    pub agent_id: String,
    pub model: Option<String>,
    pub load_session_id: Option<String>,
    pub task: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSessionInput {
    pub session_id: SessionId,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSessionInputView {
    pub session_id: SessionId,
    pub input_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentConversationRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentConversationMessageView {
    pub message_id: String,
    pub session_id: SessionId,
    pub role: AgentConversationRole,
    /// Concatenated text of the message.
    ///
    /// Kept as the compatibility surface: a client that predates content parts
    /// still renders the full text, and a producer that predates them still
    /// decodes with an empty `parts`.
    pub content: String,
    /// Typed content parts in the order the Agent produced them.
    ///
    /// Additive since `core-v0.3.6`. Non-text content (an image an Agent
    /// returned, a file it attached) has no representation in `content`, so
    /// without this a client can only show prose claiming the content exists.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<AgentContentPart>,
}

/// One typed piece of an Agent message.
///
/// A part kind this build does not know is preserved verbatim rather than
/// dropped, so a newer Core never silently loses content on an older client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentContentPart {
    Text {
        text: String,
    },
    Image {
        media_type: String,
        /// Immutable reference the client resolves; never inline bytes.
        reference: String,
        alt: Option<String>,
    },
    File {
        media_type: String,
        reference: String,
        name: Option<String>,
    },
    Unknown {
        kind: String,
        /// The original object, so re-serializing is lossless.
        payload: serde_json::Value,
    },
}

impl Serialize for AgentContentPart {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        match self {
            Self::Text { text } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "text")?;
                map.serialize_entry("text", text)?;
                map.end()
            }
            Self::Image {
                media_type,
                reference,
                alt,
            } => {
                let mut map = serializer.serialize_map(Some(4))?;
                map.serialize_entry("type", "image")?;
                map.serialize_entry("mediaType", media_type)?;
                map.serialize_entry("reference", reference)?;
                if let Some(alt) = alt {
                    map.serialize_entry("alt", alt)?;
                }
                map.end()
            }
            Self::File {
                media_type,
                reference,
                name,
            } => {
                let mut map = serializer.serialize_map(Some(4))?;
                map.serialize_entry("type", "file")?;
                map.serialize_entry("mediaType", media_type)?;
                map.serialize_entry("reference", reference)?;
                if let Some(name) = name {
                    map.serialize_entry("name", name)?;
                }
                map.end()
            }
            // Round-trips the exact object Core published.
            Self::Unknown { payload, .. } => payload.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for AgentContentPart {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let kind = value
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let string_at = |keys: &[&str]| -> Option<String> {
            keys.iter()
                .find_map(|key| value.get(*key))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };
        Ok(match kind.as_str() {
            "text" => Self::Text {
                text: string_at(&["text"]).unwrap_or_default(),
            },
            "image" => Self::Image {
                media_type: string_at(&["mediaType", "media_type"]).unwrap_or_default(),
                reference: string_at(&["reference", "uri"]).unwrap_or_default(),
                alt: string_at(&["alt"]),
            },
            "file" => Self::File {
                media_type: string_at(&["mediaType", "media_type"]).unwrap_or_default(),
                reference: string_at(&["reference", "uri"]).unwrap_or_default(),
                name: string_at(&["name"]),
            },
            _ => Self::Unknown {
                kind,
                payload: value,
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSessionView {
    pub session_id: SessionId,
    pub lane_id: AgentLaneId,
    pub agent_id: String,
    pub model: Option<String>,
    pub status: AgentSessionStatus,
    pub owner: RuntimeOwner,
    pub task: String,
    pub diagnostic: Option<String>,
    /// Latest completed ACP assistant response for this exact owner session.
    /// Older frontend-contract-v1 snapshots omit it and decode as unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Operator control history (`runtime.agent_session_control`).
    ///
    /// Absent until the session is first paused, resumed, or terminated, so an
    /// older snapshot and an uncontrolled session encode identically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<AgentSessionControlView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateStrength {
    Full,
    Cooperative,
    Containment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationPolicy {
    Autonomous,
    ProposeOnly,
    ReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneStatus {
    Draft,
    Queued,
    Starting,
    Running,
    WaitingApproval,
    NeedsInput,
    #[serde(alias = "apply_conflict")]
    Blocked,
    Attached,
    #[serde(alias = "stopped")]
    Detached,
    #[serde(alias = "completed", alias = "accepted", alias = "applied")]
    Done,
    Failed,
    #[serde(alias = "discarded", alias = "canceled")]
    Cancelled,
    Archived,
}

impl LaneStatus {
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Draft
                | Self::Queued
                | Self::Starting
                | Self::Running
                | Self::WaitingApproval
                | Self::NeedsInput
                | Self::Blocked
                | Self::Attached
                | Self::Detached
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskKind {
    Provider,
    Tool,
    Shell,
    Test,
    Job,
    Agent,
}

impl std::fmt::Display for AgentTaskKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Provider => "provider",
            Self::Tool => "tool",
            Self::Shell => "shell",
            Self::Test => "test",
            Self::Job => "job",
            Self::Agent => "agent",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionTarget {
    Local,
    Ssh { host: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataEgressPolicy {
    Deny,
    AllowProvider,
    AllowListed { domains: Vec<String> },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaneBudget {
    pub token_limit: Option<u64>,
    pub cost_limit_micro_usd: Option<u64>,
    pub wall_time_limit_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTaskRecord {
    pub id: AgentTaskId,
    pub parent_id: Option<AgentTaskId>,
    pub role: AgentRole,
    pub kind: AgentTaskKind,
    pub route: AgentRoute,
    pub title: String,
    pub status: AgentTaskStatus,
    pub activity: String,
    pub summary: String,
    pub progress: u8,
    pub started_at: Option<u64>,
    pub updated_at: Option<u64>,
    pub workspace: Option<String>,
    pub evidence: Vec<String>,
    pub permissions: Vec<String>,
    pub decision: Option<String>,
    pub result: Option<String>,
    pub resume_handle: Option<String>,
    pub pid: Option<u32>,
    pub next_action: Option<AgentNextAction>,
    /// Runtime owner this task belongs to.
    ///
    /// Additive since core-0.3.6 (GUI-CORE-010). `None` means Core did not
    /// know the owner at emission — never a default owner, and never an owner
    /// a client may infer from timing, ordering, or the task's own label.
    pub owner: Option<crate::RuntimeOwner>,
}

impl AgentTaskRecord {
    pub fn status_kind(&self) -> AgentTaskStatus {
        self.status
    }

    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }

    pub fn priority(&self) -> u8 {
        self.status.priority()
    }
}

impl Serialize for AgentTaskRecord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        AgentTaskRecordWireRef::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AgentTaskRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = AgentTaskRecordWire::deserialize(deserializer)?;
        let kind = legacy_task_kind(&wire.kind).map_err(serde::de::Error::custom)?;
        let role = match wire.role {
            Some(role) => role,
            None => {
                legacy_task_role(wire.agent.as_deref(), kind).map_err(serde::de::Error::custom)?
            }
        };
        let route = match wire.route {
            Some(route) => route,
            None => legacy_task_route(wire.transport.as_deref(), kind)
                .map_err(serde::de::Error::custom)?,
        };
        Ok(Self {
            id: wire.id,
            parent_id: wire.parent_id,
            role,
            kind,
            route,
            title: wire.title,
            status: wire.status,
            activity: wire.activity,
            summary: wire.summary,
            progress: wire.progress,
            started_at: wire.started_at,
            updated_at: wire.updated_at,
            workspace: wire.workspace,
            evidence: wire.evidence,
            permissions: wire.permissions,
            decision: wire.decision,
            result: wire.result,
            resume_handle: wire.resume_handle,
            pid: wire.pid,
            next_action: wire.next_action,
            owner: wire.owner,
        })
    }
}

#[derive(Serialize)]
struct AgentTaskRecordWireRef<'a> {
    id: &'a AgentTaskId,
    parent_id: &'a Option<AgentTaskId>,
    #[serde(rename = "agent")]
    role: AgentRole,
    kind: AgentTaskKind,
    #[serde(rename = "transport")]
    route: AgentRoute,
    title: &'a str,
    status: AgentTaskStatus,
    activity: &'a str,
    summary: &'a str,
    progress: u8,
    started_at: &'a Option<u64>,
    updated_at: &'a Option<u64>,
    workspace: &'a Option<String>,
    evidence: &'a Vec<String>,
    permissions: &'a Vec<String>,
    decision: &'a Option<String>,
    result: &'a Option<String>,
    resume_handle: &'a Option<String>,
    pid: &'a Option<u32>,
    next_action: &'a Option<AgentNextAction>,
    /// Omitted entirely when absent, so a record with no known owner encodes
    /// to exactly the bytes it did before the field existed.
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: &'a Option<crate::RuntimeOwner>,
}

impl<'a> From<&'a AgentTaskRecord> for AgentTaskRecordWireRef<'a> {
    fn from(task: &'a AgentTaskRecord) -> Self {
        Self {
            id: &task.id,
            parent_id: &task.parent_id,
            role: task.role,
            kind: task.kind,
            route: task.route,
            title: &task.title,
            status: task.status,
            activity: &task.activity,
            summary: &task.summary,
            progress: task.progress,
            started_at: &task.started_at,
            updated_at: &task.updated_at,
            workspace: &task.workspace,
            evidence: &task.evidence,
            permissions: &task.permissions,
            decision: &task.decision,
            result: &task.result,
            resume_handle: &task.resume_handle,
            pid: &task.pid,
            next_action: &task.next_action,
            owner: &task.owner,
        }
    }
}

#[derive(Deserialize)]
struct AgentTaskRecordWire {
    id: AgentTaskId,
    parent_id: Option<AgentTaskId>,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    role: Option<AgentRole>,
    kind: String,
    #[serde(default)]
    transport: Option<String>,
    #[serde(default)]
    route: Option<AgentRoute>,
    title: String,
    status: AgentTaskStatus,
    activity: String,
    summary: String,
    progress: u8,
    started_at: Option<u64>,
    updated_at: Option<u64>,
    workspace: Option<String>,
    evidence: Vec<String>,
    permissions: Vec<String>,
    decision: Option<String>,
    result: Option<String>,
    resume_handle: Option<String>,
    pid: Option<u32>,
    next_action: Option<AgentNextAction>,
    /// Absent in every record written before core-0.3.6; absence stays absence
    /// rather than becoming a default owner.
    #[serde(default)]
    owner: Option<crate::RuntimeOwner>,
}

// Legacy task names identify implementations, not roles. The migration keeps
// that ambiguity at the input boundary so reducers only see the typed role.
fn legacy_task_role(value: Option<&str>, kind: AgentTaskKind) -> Result<AgentRole, String> {
    let value = value.ok_or_else(|| "legacy task is missing agent/role".to_string())?;
    if let Some(role) = AgentRole::parse(value) {
        return Ok(role);
    }
    match value.trim() {
        "viden" | "codex" | "claude" | "shell" | "git" | "acp" => {
            if kind == AgentTaskKind::Test {
                Ok(AgentRole::Tester)
            } else {
                Ok(AgentRole::Coder)
            }
        }
        "external" => {
            Err("legacy task agent `external` is not a role; use route/source".to_string())
        }
        other => Err(format!("unknown legacy task agent `{other}`")),
    }
}

fn legacy_task_route(value: Option<&str>, kind: AgentTaskKind) -> Result<AgentRoute, String> {
    let value = value.ok_or_else(|| "legacy task is missing transport/route".to_string())?;
    let normalized = value.trim().to_ascii_lowercase();
    // A v0 provider task stored its provider identifier in `transport`; only
    // provider tasks may treat arbitrary identifiers as the built-in route.
    if kind == AgentTaskKind::Provider || matches!(normalized.as_str(), "runtime" | "core") {
        return Ok(AgentRoute::BuiltIn);
    }
    match normalized.as_str() {
        "built_in" => Ok(AgentRoute::BuiltIn),
        "acp" | "acp-session" => Ok(AgentRoute::Acp),
        "tmux" => Ok(AgentRoute::Tmux),
        "shell" | "local" | "pty" | "terminal" | "app-server" => Ok(AgentRoute::Terminal),
        value if value.starts_with("codex") || value.starts_with("claude") => {
            Ok(AgentRoute::Terminal)
        }
        other => Err(format!("unknown legacy task transport `{other}`")),
    }
}

// `runtime` and `lane` were v0 projection labels. They migrate to the single
// v1 supervised-agent kind; unrecognized labels still fail at the input edge.
fn legacy_task_kind(value: &str) -> Result<AgentTaskKind, String> {
    match value.trim() {
        "provider" => Ok(AgentTaskKind::Provider),
        "tool" => Ok(AgentTaskKind::Tool),
        "shell" => Ok(AgentTaskKind::Shell),
        "test" => Ok(AgentTaskKind::Test),
        "job" => Ok(AgentTaskKind::Job),
        "agent" | "runtime" | "lane" => Ok(AgentTaskKind::Agent),
        other => Err(format!("unknown task kind `{other}`")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLaneRecord {
    pub id: AgentLaneId,
    pub task_id: Option<AgentTaskId>,
    pub role: AgentRole,
    pub route: AgentRoute,
    pub gate_strength: GateStrength,
    pub mutation_policy: MutationPolicy,
    pub worktree: Option<String>,
    pub branch: Option<String>,
    pub target: ExecutionTarget,
    pub data_egress: DataEgressPolicy,
    pub status: LaneStatus,
    pub budget: LaneBudget,
    pub active_session_ids: Vec<SessionId>,
    pub summary: String,
    pub evidence: Vec<EvidenceId>,
    /// Bounded run facts accumulated by the lane reducer. `None` means the lane
    /// has never been observed running, which is deliberately distinct from
    /// `Some(LaneRunStats::default())` ("ran and measured zero"). Absence is
    /// also what keeps the frozen frontend-contract-v1 lane bytes unchanged.
    pub run_stats: Option<LaneRunStats>,
}

impl AgentLaneRecord {
    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }
}

impl Serialize for AgentLaneRecord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        AgentLaneRecordWireRef::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AgentLaneRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        if value.get("role").is_some() || value.get("route").is_some() {
            let typed =
                AgentLaneRecordWire::deserialize(value).map_err(serde::de::Error::custom)?;
            return Ok(typed.into());
        }
        let legacy = LegacyAgentLaneRecord::deserialize(value).map_err(serde::de::Error::custom)?;
        legacy.try_into().map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize)]
struct AgentLaneRecordWireRef<'a> {
    id: &'a AgentLaneId,
    task_id: &'a Option<AgentTaskId>,
    role: AgentRole,
    route: AgentRoute,
    gate_strength: GateStrength,
    mutation_policy: MutationPolicy,
    worktree: &'a Option<String>,
    branch: &'a Option<String>,
    target: &'a ExecutionTarget,
    data_egress: &'a DataEgressPolicy,
    status: LaneStatus,
    budget: &'a LaneBudget,
    active_session_ids: &'a Vec<SessionId>,
    summary: &'a str,
    evidence: &'a Vec<EvidenceId>,
    // Additive post-freeze field. Omitting it when absent keeps every recorded
    // frontend-contract-v1 fixture byte and digest identical.
    #[serde(skip_serializing_if = "Option::is_none")]
    run_stats: &'a Option<LaneRunStats>,
}

impl<'a> From<&'a AgentLaneRecord> for AgentLaneRecordWireRef<'a> {
    fn from(lane: &'a AgentLaneRecord) -> Self {
        Self {
            id: &lane.id,
            task_id: &lane.task_id,
            role: lane.role,
            route: lane.route,
            gate_strength: lane.gate_strength,
            mutation_policy: lane.mutation_policy,
            worktree: &lane.worktree,
            branch: &lane.branch,
            target: &lane.target,
            data_egress: &lane.data_egress,
            status: lane.status,
            budget: &lane.budget,
            active_session_ids: &lane.active_session_ids,
            summary: &lane.summary,
            evidence: &lane.evidence,
            run_stats: &lane.run_stats,
        }
    }
}

#[derive(Deserialize)]
struct AgentLaneRecordWire {
    id: AgentLaneId,
    task_id: Option<AgentTaskId>,
    role: AgentRole,
    route: AgentRoute,
    gate_strength: GateStrength,
    mutation_policy: MutationPolicy,
    worktree: Option<String>,
    branch: Option<String>,
    target: ExecutionTarget,
    data_egress: DataEgressPolicy,
    status: LaneStatus,
    budget: LaneBudget,
    active_session_ids: Vec<SessionId>,
    summary: String,
    evidence: Vec<EvidenceId>,
    // Older writers never emitted this field; they decode as unobserved.
    #[serde(default)]
    run_stats: Option<LaneRunStats>,
}

impl From<AgentLaneRecordWire> for AgentLaneRecord {
    fn from(lane: AgentLaneRecordWire) -> Self {
        Self {
            id: lane.id,
            task_id: lane.task_id,
            role: lane.role,
            route: lane.route,
            gate_strength: lane.gate_strength,
            mutation_policy: lane.mutation_policy,
            worktree: lane.worktree,
            branch: lane.branch,
            target: lane.target,
            data_egress: lane.data_egress,
            status: lane.status,
            budget: lane.budget,
            active_session_ids: lane.active_session_ids,
            summary: lane.summary,
            evidence: lane.evidence,
            run_stats: lane.run_stats,
        }
    }
}

#[derive(Deserialize)]
struct LegacyAgentLaneRecord {
    id: AgentLaneId,
    task_id: AgentTaskId,
    agent: String,
    #[allow(dead_code)]
    screen: String,
    transport: String,
    status: String,
    summary: String,
    evidence: Vec<EvidenceId>,
}

impl TryFrom<LegacyAgentLaneRecord> for AgentLaneRecord {
    type Error = String;

    fn try_from(lane: LegacyAgentLaneRecord) -> Result<Self, Self::Error> {
        let route = legacy_lane_route(&lane.transport)?;
        Ok(Self {
            id: lane.id,
            task_id: Some(lane.task_id),
            role: legacy_lane_role(&lane.agent)?,
            route,
            gate_strength: default_gate_strength(route),
            mutation_policy: MutationPolicy::ProposeOnly,
            worktree: None,
            branch: None,
            target: ExecutionTarget::Local,
            data_egress: DataEgressPolicy::Deny,
            status: legacy_lane_status(&lane.status)?,
            budget: LaneBudget::default(),
            active_session_ids: Vec::new(),
            summary: lane.summary,
            evidence: lane.evidence,
            // A v0 record carries no observations; it is unobserved, not zero.
            run_stats: None,
        })
    }
}

pub fn legacy_lane_role(value: &str) -> Result<AgentRole, String> {
    if let Some(role) = AgentRole::parse(value) {
        return Ok(role);
    }
    match value.trim() {
        "codex" | "claude" | "shell" | "git" | "acp" | "tmux" | "viden" => Ok(AgentRole::Coder),
        "external" => {
            Err("legacy lane agent `external` is not a role; use route/source".to_string())
        }
        other => Err(format!("unknown legacy lane agent `{other}`")),
    }
}

pub fn legacy_lane_route(value: &str) -> Result<AgentRoute, String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "built_in" | "runtime" | "core" => Ok(AgentRoute::BuiltIn),
        "acp" | "acp-session" => Ok(AgentRoute::Acp),
        "tmux" => Ok(AgentRoute::Tmux),
        "shell" | "local" | "pty" | "terminal" | "app-server" | "main" => Ok(AgentRoute::Terminal),
        value if value.starts_with("tmux") => Ok(AgentRoute::Tmux),
        value if value.starts_with("codex") || value.starts_with("claude") => {
            Ok(AgentRoute::Terminal)
        }
        other => Err(format!("unknown legacy lane transport `{other}`")),
    }
}

fn legacy_lane_status(value: &str) -> Result<LaneStatus, String> {
    if let Ok(status) = serde_json::from_value(serde_json::Value::String(value.to_string())) {
        return Ok(status);
    }
    match value.trim() {
        "thinking" => Ok(LaneStatus::Starting),
        "streaming" | "editing" | "running_tool" | "testing" => Ok(LaneStatus::Running),
        // v0's reviewing state meant the lane was waiting for an operator decision.
        "reviewing" => Ok(LaneStatus::NeedsInput),
        "manual" | "approval" => Ok(LaneStatus::WaitingApproval),
        other => Err(format!("unknown legacy lane status `{other}`")),
    }
}

pub fn default_gate_strength(route: AgentRoute) -> GateStrength {
    match route {
        AgentRoute::BuiltIn => GateStrength::Full,
        AgentRoute::Acp => GateStrength::Cooperative,
        AgentRoute::Terminal | AgentRoute::Tmux => GateStrength::Containment,
    }
}
