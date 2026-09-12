use crate::{
    AgentLaneRecord, LaneSidebarMode, LocaleId, MAX_HIDDEN_STATUSBAR_SEGMENT_BYTES,
    MAX_HIDDEN_STATUSBAR_SEGMENTS, RuntimeOwner, UiColorMode, UiDensity, UiMotion, UiSkin,
};

/// Partial personal UI preference update sent by a frontend.
///
/// Every field is a closed enum so serialized commands cannot carry free-form
/// theme names or secrets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UiPreferencePatch {
    pub locale: Option<LocaleId>,
    pub skin: Option<UiSkin>,
    pub mode: Option<UiColorMode>,
    pub density: Option<UiDensity>,
    pub motion: Option<UiMotion>,
}

/// Partial cockpit layout update sent by a frontend (`ui.layout_preferences`).
///
/// `None` on a field means "leave it as it is", never "reset it": a reset is
/// its own command, so a client that omits a field cannot accidentally erase a
/// preference it never meant to touch.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UiLayoutPreferencePatch {
    #[serde(default)]
    pub lane_sidebar_mode: Option<LaneSidebarMode>,
    #[serde(default)]
    pub hidden_statusbar_segments: Option<Vec<String>>,
}

impl UiLayoutPreferencePatch {
    /// Rejects a patch that cannot mean what it says, before anything is
    /// written.
    ///
    /// Every rejection here becomes a `CommandRejected`. The bound is a
    /// refusal rather than a clamp because the two answers are different
    /// facts: a clamped list silently keeps showing a segment the operator
    /// asked to hide, and nothing on the stream would say so.
    pub fn validate(&self) -> Result<(), String> {
        let Some(segments) = &self.hidden_statusbar_segments else {
            return Ok(());
        };
        if segments.len() > MAX_HIDDEN_STATUSBAR_SEGMENTS {
            return Err(format!(
                "hidden statusbar segments exceed the bound of {MAX_HIDDEN_STATUSBAR_SEGMENTS}"
            ));
        }
        for segment in segments {
            if segment.trim().is_empty() {
                return Err("a hidden statusbar segment name cannot be empty".to_string());
            }
            if segment.len() > MAX_HIDDEN_STATUSBAR_SEGMENT_BYTES {
                return Err(format!(
                    "hidden statusbar segment `{segment}` exceeds \
                     {MAX_HIDDEN_STATUSBAR_SEGMENT_BYTES} bytes"
                ));
            }
            // A control character in a segment name would reach a terminal
            // statusbar as an escape sequence. Core cannot validate the
            // client's vocabulary, but it can refuse a name no vocabulary
            // contains.
            if segment.chars().any(char::is_control) {
                return Err(
                    "a hidden statusbar segment name cannot contain control characters".to_string(),
                );
            }
        }
        Ok(())
    }
}

/// Bounded cross-project session inventory requested by a frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecentWorkQuery {
    pub limit: u16,
}

/// Project facts safe to expose outside the session persistence boundary.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecentProjectSummary {
    pub canonical_root: String,
    pub display_name: String,
    pub last_updated_at: u64,
    pub latest_session_id: Option<String>,
}

/// Session facts safe to expose outside the session persistence boundary.
///
/// This intentionally omits transcript locations, titles, previews, arbitrary
/// metadata values, and command/tool/message bodies.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecentSessionSummary {
    pub canonical_root: String,
    pub session_id: String,
    pub created_at: u64,
    pub last_updated_at: u64,
    pub message_count: u64,
    pub tool_call_count: u64,
    pub command_count: u64,
}

/// Whether source-control fields are complete enough for clients to use.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceSourceStatus {
    #[default]
    Ready,
    Unavailable,
    Truncated,
}

/// Source-control facts for the current workspace, independent of any client layout.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceSourceView {
    /// Legacy schema-v1 payloads predate explicit availability and were complete.
    #[serde(default)]
    pub status: WorkspaceSourceStatus,
    pub branch: Option<String>,
    pub worktree: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub added: u32,
    pub deleted: u32,
    pub dirty: bool,
}

/// Closed service families reported by the runtime health projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeServiceKind {
    Mcp,
    Lsp,
}

/// Runtime-owned health classifications; clients must not infer availability from these facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeServiceStatus {
    Connected,
    Ready,
    Degraded,
    Offline,
    Unavailable,
}

/// One service-health fact identified by its service family and stable id.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeServiceHealthView {
    pub id: String,
    pub kind: RuntimeServiceKind,
    pub label: String,
    pub status: RuntimeServiceStatus,
    pub detail_key: Option<String>,
}

/// Closed workspace change classifications shared by terminal and desktop clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
}

/// Owner-bound source change fact. The runtime envelope repeats this owner for validation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceChangeView {
    pub id: String,
    pub owner: RuntimeOwner,
    pub path: String,
    pub kind: WorkspaceChangeKind,
    pub patch: Option<String>,
    pub additions: u32,
    pub deletions: u32,
    /// The same change as typed rows (`runtime.structured_diff`). `patch`
    /// stays beside it for base clients: this field is additive and omitted
    /// when absent, so a change published without it encodes to exactly the
    /// bytes it did before. `None` means Core produced no structured diff for
    /// this change — never "the change is empty".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<crate::DiffDocument>,
}

/// Closed execution states for a runtime-owned check result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckRunStatus {
    Queued,
    Running,
    Passed,
    Failed,
    Cancelled,
}

/// Owner-bound check fact. Presentation labels and localized copy remain frontend concerns.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CheckRunView {
    pub id: String,
    pub owner: RuntimeOwner,
    pub label: String,
    pub command: String,
    pub status: CheckRunStatus,
    pub summary: String,
    pub failing_location: Option<String>,
}

/// Closed starter templates offered by first-run and Lane-creation clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StarterLanePreset {
    Coder,
    Reviewer,
    Tester,
}

/// User-controlled inputs whose resolved result must be reviewed before creation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StarterLaneRequest {
    pub lane_id: String,
    pub preset: StarterLanePreset,
    pub branch: Option<String>,
    pub worktree_path: Option<String>,
}

/// Read-only resolution of a starter Lane request against one repository revision.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StarterLanePreview {
    pub preview_id: String,
    pub content_sha256: String,
    pub owner: RuntimeOwner,
    pub lane: AgentLaneRecord,
    pub branch: String,
    pub worktree_path: String,
    pub base_revision: String,
    pub diagnostics: Vec<String>,
}

/// Owner-bound proof that the reviewed Lane was durably created.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StarterLaneReceipt {
    pub preview_id: String,
    pub content_sha256: String,
    pub lane: AgentLaneRecord,
    pub branch: String,
    pub worktree_path: String,
    pub base_revision: String,
    pub owner: RuntimeOwner,
}

/// Stable reasons why a reviewed preview stopped being creatable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StarterLanePreviewInvalidationReason {
    PlanModeDenied,
    RequestChanged,
    HashMismatch,
    BaseRevisionChanged,
    WorktreeUnavailable,
    BranchUnavailable,
    LaneAlreadyRegistered,
    PermissionDenied,
    EffectFailed,
}
