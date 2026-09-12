use serde::{Deserialize, Serialize};

use crate::{D6RecoveryProjection, PermissionDockProjection, ResolvedPreferencesProjection};

pub const D1_OWNER_CAPABILITY: &str = "runtime.lane_owner_projection";

/// A selected Lane that carries more than one exact runtime owner binding.
///
/// Client-local, not a Core contract request, so the code deliberately does
/// not use the `GUI-CORE-` prefix the register in
/// `apps/gui/contract-requests.md` reserves for numbered requests. Core's own
/// reducer keeps at most one `LaneRuntimeOwnerBinding` per Lane, so a view
/// holding two is one this client refuses to render rather than a fact Core
/// still owes it; D1 enters the snapshot/replay recovery path instead of
/// choosing an owner.
pub const D1_OWNER_CARDINALITY_CODE: &str = "D1-OWNER-CARDINALITY";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct D1WorkspaceSourceProjection {
    pub status: &'static str,
    pub branch: Option<String>,
    pub worktree: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub added: u32,
    pub deleted: u32,
    pub dirty: bool,
}

/// Titlebar source-control facts for the cockpit's project selector and the
/// design's `.gitops` chips.
///
/// The whole struct is `Option` on the cockpit projection: `None` means Core
/// published no workspace source, or published one it could not sample, and
/// the titlebar omits the git block entirely (the design's `git:false` mode).
/// Rendering zeroes from an unavailable sample would read as "clean tree, in
/// sync", which is a fabricated fact.
///
/// Every field is read-only. frontend-contract-v1 carries no operator git
/// command, so nothing here backs a commit, push, or sync action
/// (`GUI-CORE-020`).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1TopbarSourceProjection {
    /// The project name Core published through its project probe. `None` when
    /// Core named none; the frontend then keeps showing the workspace path it
    /// already renders instead of deriving a name from it.
    pub project: Option<String>,
    pub branch: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub dirty: bool,
    pub status: &'static str,
    /// `true` when Core could only sample part of the workspace. The counts
    /// are still rendered, but behind a truncation marker so they are never
    /// read as complete.
    pub truncated: bool,
    /// Distinct worktrees across the project's active Lanes. Core publishes no
    /// git worktree inventory, so this counts the `worktree` names on Lane
    /// records — deduplicated, because the chip's label counts worktrees and
    /// two Lanes may share one.
    pub lane_worktree_count: u32,
}

/// Context pressure for the selected Lane's own task.
///
/// Resolved through the typed `ContextScope::Task` named by the exact runtime
/// owner Core bound to that Lane, so it is never another Lane's budget and
/// never a client-side estimate. Absent when the Lane has no exact owner, no
/// task, or no budget published in that scope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1ContextUsageProjection {
    pub budget_id: String,
    pub used_tokens: u64,
    pub soft_token_limit: u64,
    pub hard_token_limit: u64,
    pub remaining_tokens: u64,
    pub exceeded: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1LaneAgentProjection {
    pub lane_id: String,
    pub workspace_id: String,
    pub project_id: String,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub turn_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1ProviderHealthProjection {
    pub provider_id: String,
    pub model: String,
    pub status: String,
    pub request_count: u64,
    pub error_count: u64,
    pub last_latency_ms: Option<u64>,
    pub average_latency_ms: Option<u64>,
    pub tokens_per_second: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1RuntimeServiceProjection {
    pub id: String,
    pub kind: &'static str,
    pub label: String,
    pub status: &'static str,
    pub detail_key: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1ChecklistItemProjection {
    pub id: String,
    pub kind: &'static str,
    pub label: String,
    pub status: &'static str,
    pub command: Option<String>,
    pub path: Option<String>,
    pub summary: Option<String>,
    pub patch: Option<String>,
    /// The same change as typed rows, when Core published them
    /// (`runtime.structured_diff`). `patch` stays beside it: the two are two
    /// views of one Core computation, never two independent ones.
    pub diff: Option<crate::diff_review::DiffDocumentProjection>,
    pub failing_location: Option<String>,
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1ContextDockProjection {
    pub source: Option<D1WorkspaceSourceProjection>,
    /// The selected Lane's own worktree source (`runtime.workspace_owner`,
    /// `C5`'s `lane_sources`).
    ///
    /// A separate field from `source`, not a replacement for it: Core samples
    /// `source` from the workspace root and a Lane's worktree is a different
    /// tree. `None` means Core published no source for this Lane — a Lane that
    /// works directly in the workspace, one that is no longer active, or a
    /// Core build without `C5` — and is never filled in from the workspace,
    /// which would print one tree's branch under another tree's name.
    pub lane_source: Option<D1WorkspaceSourceProjection>,
    pub context: Option<D1ContextUsageProjection>,
    pub lane_agent: Option<D1LaneAgentProjection>,
    pub provider: Option<D1ProviderHealthProjection>,
    pub services: Vec<D1RuntimeServiceProjection>,
    pub checklist: Vec<D1ChecklistItemProjection>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum D1Intent {
    Submit {
        lane_id: String,
        content: String,
    },
    Cancel {
        lane_id: String,
    },
    QueryAgentAdapters,
    ProbeAgentAdapter {
        agent_id: String,
    },
    PreviewDefaultLane {
        preset: String,
    },
    CreateStarterLane {
        lane_id: String,
        preset: String,
        branch: Option<String>,
        preview_id: String,
        content_sha256: String,
    },
    StartAgentSession {
        lane_id: String,
        agent_id: String,
        model: Option<String>,
        task: String,
    },
    SendAgentSessionInput {
        lane_id: String,
        session_id: String,
        content: String,
    },
    RetryAgentSession {
        lane_id: String,
        session_id: String,
    },
    CancelAgentSession {
        lane_id: String,
        session_id: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct D1OutcomeProjection {
    pub state: &'static str,
    pub reason: Option<String>,
}

impl D1OutcomeProjection {
    pub(crate) fn idle() -> Self {
        Self {
            state: "idle",
            reason: None,
        }
    }

    pub(crate) fn pending() -> Self {
        Self {
            state: "pending",
            reason: None,
        }
    }

    pub(crate) fn confirmed() -> Self {
        Self {
            state: "confirmed",
            reason: None,
        }
    }

    pub(crate) fn rejected(reason: String) -> Self {
        Self {
            state: "rejected",
            reason: Some(reason),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1LaneProjection {
    pub id: String,
    pub role: String,
    pub status: String,
    pub summary: String,
    pub branch: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1EnvironmentProjection {
    pub cwd: String,
    pub provider_id: String,
    pub model: String,
    pub work_mode: String,
    pub permission_level: String,
    pub token_total: u64,
    pub cost_micro_usd: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct D1TaskProjection {
    pub id: String,
    pub title: String,
    pub status: String,
    pub progress: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1ToolProjection {
    pub id: String,
    pub name: String,
    pub input_preview: String,
    pub state: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct D1ApprovalProjection {
    pub id: String,
    pub title: String,
    pub risk: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1QueuedInputProjection {
    pub id: String,
    pub content_preview: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct D1EvidenceProjection {
    pub id: String,
    pub kind: String,
    pub summary: String,
    pub path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1LiveWorkProjection {
    pub tasks: Vec<D1TaskProjection>,
    pub tools: Vec<D1ToolProjection>,
    pub approvals: Vec<D1ApprovalProjection>,
    pub queued_inputs: Vec<D1QueuedInputProjection>,
    pub evidence: Vec<D1EvidenceProjection>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct D1TranscriptRowProjection {
    pub id: String,
    pub kind: &'static str,
    pub content: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1WorkspaceEligibilityProjection {
    pub is_git_repository: bool,
    pub has_head: bool,
    pub can_create_lane: bool,
    pub diagnostic: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1StarterLanePreviewProjection {
    pub preview_id: String,
    pub content_sha256: String,
    pub lane_id: String,
    pub branch: Option<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1StarterLaneReceiptProjection {
    pub preview_id: String,
    pub lane_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1AgentAdapterProjection {
    pub agent_id: String,
    pub display_name: String,
    pub startability: String,
    pub diagnostics: Vec<String>,
    /// Model options Core published for this adapter. Empty means Core
    /// published none; the client never fabricates a model list.
    pub models: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1AgentSessionProjection {
    pub session_id: String,
    pub lane_id: String,
    pub agent_id: String,
    pub model: Option<String>,
    pub status: String,
    pub task: String,
    pub diagnostic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    pub conversation: Vec<D1AgentConversationMessageProjection>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1AgentConversationMessageProjection {
    pub message_id: String,
    pub role: &'static str,
    pub content: String,
    /// Typed content Core published alongside the text. Empty when Core
    /// published none; the client never synthesizes a part.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<D1ContentPartProjection>,
}

/// One renderable piece of an Agent message.
///
/// A part kind this build cannot render is still projected, so the operator
/// sees that content exists rather than silently losing it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1ContentPartProjection {
    pub kind: String,
    pub media_type: Option<String>,
    pub reference: Option<String>,
    pub text: Option<String>,
    pub label: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1AgentSessionInputProjection {
    pub session_id: String,
    pub input_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1CostUsageProjection {
    pub usage_id: String,
    pub attempt_index: u32,
    pub total_tokens: u64,
    pub actual_cost_micro_usd: Option<u64>,
    pub outcome: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1CursorProjection {
    pub stream_id: String,
    pub sequence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1ComposerProjection {
    pub editable: bool,
    pub busy: bool,
    pub can_cancel: bool,
    pub can_submit_immediately: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct D1UnavailableFeatureProjection {
    pub id: &'static str,
    pub available: bool,
    pub code: &'static str,
    pub message: &'static str,
}

/// Context usage for the statusbar. This is the most recently updated budget
/// Core published anywhere in the workspace, never a per-Lane estimate. The
/// Lane-scoped number lives in the Context Dock's
/// [`D1ContextUsageProjection`], which resolves the selected Lane's own task
/// scope; this segment stays a coarse workspace-level indicator.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1StatusbarContextProjection {
    pub used_tokens: u64,
    pub hard_token_limit: u64,
    pub exceeded: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1StatusbarLaneProjection {
    pub lane_id: String,
    /// The agent behind the Lane's sole Core agent session. `None` when Core
    /// publishes zero or more than one session for the Lane (fail closed on
    /// ambiguous identity).
    pub agent_id: Option<String>,
    pub status: String,
    /// Progress of the Lane's own task record, when Core links one.
    pub progress: Option<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1StatusbarLatencyProjection {
    pub last_latency_ms: Option<u64>,
    pub average_latency_ms: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1StatusbarTokensProjection {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1StatusbarRequestsProjection {
    pub request_count: u64,
    pub error_count: u64,
}

/// Window-level statusbar facts, computed by the host from the confirmed
/// Core view so the frontend renders segments without a second reducer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1StatusbarProjection {
    pub work_mode: String,
    pub permission_level: String,
    pub context: Option<D1StatusbarContextProjection>,
    /// Ordered event-stream position of the confirmed snapshot (the adapter's
    /// replay cursor sequence). frontend-contract-v1 publishes no event
    /// counter, so this is a stream position, never a count.
    pub event_stream_position: u64,
    pub lane: Option<D1StatusbarLaneProjection>,
    pub latency: Option<D1StatusbarLatencyProjection>,
    pub tokens: Option<D1StatusbarTokensProjection>,
    /// Number of runtime errors Core currently publishes.
    pub diagnostics_count: u64,
    pub requests: Option<D1StatusbarRequestsProjection>,
    /// Pending approvals plus open merge gates awaiting a human.
    pub pending_gate_count: u64,
}

/// One composer-control mutation. Values arrive as the CLI names Core itself
/// publishes in the snapshot; unknown names are rejected before a command is
/// built. The GUI never applies the mode/permission coupling rule locally —
/// it sends the command and renders whatever snapshot Core publishes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ComposerControlIntent {
    SetWorkMode { mode: String },
    SetPermissionLevel { level: String },
    SelectModel { provider_id: String, model: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1CockpitProjection {
    pub preferences: ResolvedPreferencesProjection,
    pub selected_lane_id: Option<String>,
    pub topbar_source: Option<D1TopbarSourceProjection>,
    pub context_dock: D1ContextDockProjection,
    pub lanes: Vec<D1LaneProjection>,
    pub environment: D1EnvironmentProjection,
    pub live_work: D1LiveWorkProjection,
    pub transcript: Vec<D1TranscriptRowProjection>,
    pub workspace_eligibility: Option<D1WorkspaceEligibilityProjection>,
    pub starter_lane_previews: Vec<D1StarterLanePreviewProjection>,
    pub starter_lane_receipts: Vec<D1StarterLaneReceiptProjection>,
    pub agent_adapters: Vec<D1AgentAdapterProjection>,
    pub agent_sessions: Vec<D1AgentSessionProjection>,
    pub agent_session_inputs: Vec<D1AgentSessionInputProjection>,
    pub cost_usage: Vec<D1CostUsageProjection>,
    pub replay_cursor: D1CursorProjection,
    pub composer: D1ComposerProjection,
    pub statusbar: D1StatusbarProjection,
    pub permission_dock: PermissionDockProjection,
    pub recovery: D6RecoveryProjection,
    pub unavailable_features: Vec<D1UnavailableFeatureProjection>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D1IntentResult {
    pub projection: D1CockpitProjection,
    pub pending_command_id: Option<String>,
    pub outcome: D1OutcomeProjection,
}

/// The capability gaps D1 declares, each naming the open register entry in
/// `apps/gui/contract-requests.md` that closes it.
///
/// A row here is a claim about Core, so it is removed the moment the fact
/// arrives rather than left standing as a stale sentence, and it never cites a
/// code the register does not carry.
pub(crate) fn unavailable_features(
    structured_diff: bool,
    operator_git: bool,
) -> Vec<D1UnavailableFeatureProjection> {
    let mut features = Vec::new();
    // `diff` was unconditional until Core published `runtime.structured_diff`.
    // The row is a claim about Core — "Core publishes an opaque patch string"
    // — so it is dropped the moment that stops being true, and it stays for a
    // Core that really does publish only the patch string.
    if !structured_diff {
        features.push(D1UnavailableFeatureProjection {
            id: "diff",
            available: false,
            code: "GUI-CORE-012",
            message: "Structured diff rows are unavailable; Core publishes an opaque patch string.",
        });
    }
    // `apply` was unconditional under GUI-CORE-020 until Core published
    // `runtime.operator_git`. Keeping it after C2 landed would have been a
    // stale claim about Core: the DiffReview commit bar and the titlebar sync
    // control now reach real `RunOperatorGitAction` commands, and the real
    // state is what those controls render. The row survives only for a Core
    // build that genuinely publishes no operator source-control actions, and
    // it then names the capability rather than implying the register entry is
    // still open.
    if !operator_git {
        features.push(D1UnavailableFeatureProjection {
            id: "apply",
            available: false,
            code: "GUI-CORE-020",
            message: "Operator source-control actions are unavailable; Core publishes no \
                      `runtime.operator_git`.",
        });
    }
    features.extend([
        // `audit` was here under GUI-CORE-004 until Core published the
        // append-only audit timeline (`QueryAudit` -> `AuditPageLoaded`,
        // capability `runtime.audit`), which closed GUI-CORE-014 and
        // GUI-CORE-024. The timeline's registered hosts are the D10 ticker and
        // D14; D1 declaring it missing was a claim about Core that had stopped
        // being true.
        D1UnavailableFeatureProjection {
            id: "recovery",
            available: false,
            // The fail-closed code the register pins for a recovery action
            // schema 1 does not model. Restart and close-Lane now reach real
            // Core commands, so only checkpoint capture and restore are left
            // (contract request GUI-CORE-018).
            code: "GUI-CORE-003",
            message: "Checkpoint capture and restore are unavailable.",
        },
        D1UnavailableFeatureProjection {
            id: "transcript_user",
            available: false,
            code: "GUI-CORE-009",
            message: "Typed user prompt rows are unavailable.",
        },
        D1UnavailableFeatureProjection {
            id: "transcript_assistant",
            available: false,
            code: "GUI-CORE-009",
            message: "Owner-scoped assistant rows are unavailable.",
        },
        // `live_work_scope` was here under GUI-CORE-010 until Core published a
        // `RuntimeOwner` on each live-work fact. D1 now scopes tasks, tool
        // calls, queued inputs, and evidence by exact owner equality; a fact
        // with no owner is still omitted from Lane scope, which is a property
        // of that fact rather than a missing capability.
    ]);
    features
}
