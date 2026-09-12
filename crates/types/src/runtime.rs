use crate::{
    AgentAdapterView, AgentContentPart, AgentConversationMessageView, AgentConversationRole,
    AgentDagRecord, AgentDagTaskSpec, AgentLaneId, AgentLaneRecord, AgentSessionInput,
    AgentSessionInputView, AgentSessionRequest, AgentSessionStatus, AgentSessionView, AgentTaskId,
    AgentTaskRecord, ApprovalDecision, ApprovalDefaultAction, ApprovalResponse, ApprovalRisk,
    ApprovalScope, ApprovalTarget, AuditPage, AuditQuery, CheckRunView, ConflictBounce,
    ContextBudgetRecord, ContextBundleRecord, ContextBundleSummaryRecord, ContextHandleRecord,
    ContextItemRecord, ContextQualityRecord, ContextReductionRecord, ContextRetrievalRecord,
    ContextScope, ContextViewRecord, ContractDecision, ContractRecord, CostLedgerTotals,
    CostUsageRecord, CredentialHandle, DecisionContext, DependencyRecord, DependencyState,
    EvidenceCanonicalizationRecord, EvidenceContent, EvidenceId, EvidencePage, EvidenceQuery,
    HandoffAcceptance, HandoffRecord, LaneStatus, MergeGateId, MergeGateRecord, MessageId,
    OperatorGitAction, OperatorGitOutcome, PermissionLevel, ProjectConfigPreview, ProjectProbe,
    ProviderCacheObservationRecord, RecentProjectSummary, RecentSessionSummary, RecentWorkQuery,
    ResolvedUiPreferences, RevertRecord, ReviewRequestRecord, ReviewVerdict,
    ReviewedEvidenceBinding, RuntimeOwner, RuntimeServiceHealthView, RuntimeSnapshot, SessionId,
    SourceTarget, StarterLanePreset, StarterLanePreview, StarterLanePreviewInvalidationReason,
    StarterLaneReceipt, StarterLaneRequest, ToolCallId, TranscriptPage, TranscriptPageRequest,
    UiLayoutPreferencePatch, UiLayoutPreferences, UiPreferenceDiagnostic, UiPreferencePatch,
    UiPreferences, WorkMode, WorkspaceChangeView, WorkspaceDiffPage, WorkspaceDiffQuery,
    WorkspaceEligibility, WorkspaceFilePage, WorkspaceFilesQuery, WorkspaceRuntimeOwnerBinding,
    WorkspaceSourceView, now_timestamp,
};
use std::collections::{BTreeMap, BTreeSet};

const RUNTIME_VIEW_COLLECTION_LIMIT: usize = 50;

/// UI-independent command contract sent from a client surface into the runtime.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
// This is a stable serialized runtime protocol enum. Boxing large payloads
// would churn public construction semantics without changing wire format.
#[allow(clippy::large_enum_variant)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeCommand {
    QueryAgentAdapters,
    ProbeAgentAdapter {
        agent_id: String,
    },
    StartAgentSession {
        request: AgentSessionRequest,
    },
    SendAgentSessionInput {
        input: AgentSessionInput,
    },
    RetryAgentSession {
        session_id: SessionId,
    },
    CancelAgentSession {
        session_id: String,
    },
    ProbeProject,
    PreviewProjectConfig {
        contents: String,
    },
    ConfirmProjectConfig {
        preview_id: String,
        content_sha256: String,
    },
    StoreCredentialHandle {
        provider_id: String,
        backend_id: String,
        credential_request_id: String,
    },
    SetUiPreferences {
        patch: UiPreferencePatch,
    },
    ResetUiPreferences,
    /// Partial cockpit layout update (`ui.layout_preferences`).
    ///
    /// A separate command from `SetUiPreferences` because it writes a separate
    /// persisted table (`[ui.layout]`) and a separate published record. The
    /// answering `UiLayoutPreferencesUpdated` repeats the envelope's command
    /// id; a pre-write refusal is `CommandRejected` with that same id.
    SetUiLayoutPreferences {
        patch: UiLayoutPreferencePatch,
    },
    /// Drops the persisted `[ui.layout]` table and republishes the defaults.
    ResetUiLayoutPreferences,
    QueryRecentWork {
        query: RecentWorkQuery,
    },
    /// Read-only page of the append-only audit timeline. Like
    /// `QueryRecentWork` this never mutates and never prompts for permission,
    /// so it stays answerable in Plan mode.
    QueryAudit {
        query: AuditQuery,
    },
    /// Read-only page of the durable evidence archive
    /// (`runtime.evidence_reads`, GUI-CORE-025).
    ///
    /// Gate posture is `QueryAudit`'s, not `QueryWorkspaceFiles`': bounded and
    /// owner-scoped, never tool-gated. The evidence archive is Viden's own
    /// state — facts Core itself recorded — rather than the operator's working
    /// tree, so there is no workspace read to authorize and no tool whose
    /// `viden.toml` rule would mean anything here. It mutates nothing and
    /// prompts for nothing, so it stays answerable in Plan mode.
    QueryEvidence {
        query: EvidenceQuery,
    },
    /// Read-only canonical content behind one evidence row
    /// (`runtime.evidence_reads`, GUI-CORE-025).
    ///
    /// Same posture as `QueryEvidence`, and the same store: the answer is read
    /// only from the canonical ContextStore bytes the row's own
    /// `CanonicalEvidenceReference` names, verified against its `source_hash`
    /// first. Core never serves bytes it could not verify, so every other
    /// outcome is a typed `EvidenceContent::Unavailable` rather than content.
    ReadEvidenceContent {
        evidence_id: EvidenceId,
    },
    /// Read-only page of the workspace file inventory (GUI-CORE-022).
    ///
    /// Unlike `QueryAudit` this one *is* permission-gated: it reads the
    /// operator's working tree, so it goes through the same gate as every
    /// other workspace read. It still mutates nothing, so it stays answerable
    /// in Plan mode.
    QueryWorkspaceFiles {
        query: WorkspaceFilesQuery,
    },
    /// Read-only structured diff of the workspace or one Lane worktree
    /// (`runtime.structured_diff`, GUI-CORE-012).
    ///
    /// Permission-gated like `QueryWorkspaceFiles`, under the existing
    /// non-mutating `git_diff` tool with the resolved target root as the input
    /// path, so one `viden.toml` rule set governs an operator's diff read and
    /// an agent's `git_diff` call. It mutates nothing, so it stays answerable
    /// in Plan mode.
    QueryWorkspaceDiff {
        query: WorkspaceDiffQuery,
    },
    /// One operator source-control action against the workspace or one Lane
    /// worktree (`runtime.operator_git`, GUI-CORE-020).
    ///
    /// Unlike the read beside it this one mutates, so it takes the supervised
    /// path: validated, gated on the *mapped agent tool spec* (`git_add`,
    /// `git_restore`, `git_commit`, `git_push`, `git_fetch`) so one rule set
    /// governs operators and agents alike, audited before the effect, and
    /// refused before any process spawns in plan mode. The answering
    /// `OperatorGitActionFinished` repeats the command id.
    ///
    /// `owner` is required, like the trust-loop mutations: the supervisor
    /// rejects a command whose actor does not match its envelope owner, and
    /// the audit record this action appends needs a real owner rather than a
    /// default one, which would record an authorized mutation as belonging to
    /// nobody.
    RunOperatorGitAction {
        owner: RuntimeOwner,
        target: SourceTarget,
        action: OperatorGitAction,
    },
    PreviewStarterLane {
        request: StarterLaneRequest,
    },
    PreviewDefaultStarterLane {
        preset: StarterLanePreset,
    },
    CreateStarterLane {
        request: StarterLaneRequest,
        preview_id: String,
        content_sha256: String,
    },
    SubmitUserInput {
        content: String,
    },
    QueueFollowUp {
        content: String,
    },
    CancelActiveTurn,
    SetWorkMode {
        mode: WorkMode,
    },
    SetPermissionLevel {
        level: PermissionLevel,
    },
    RespondToApproval {
        request_id: String,
        response: ApprovalResponse,
    },
    ConfigureProvider {
        provider_id: String,
        api_key_env: Option<String>,
        endpoint: Option<String>,
        default_model: Option<String>,
    },
    SelectModel {
        provider_id: String,
        model: String,
    },
    ActivateModel {
        provider_id: String,
        model: String,
    },
    DeactivateModel {
        provider_id: String,
        model: String,
    },
    CreateLane {
        lane: AgentLaneRecord,
    },
    StartLane {
        lane_id: crate::AgentLaneId,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        output_log: Option<String>,
    },
    StopLane {
        lane_id: crate::AgentLaneId,
    },
    AttachLane {
        lane_id: crate::AgentLaneId,
    },
    DetachLane {
        lane_id: crate::AgentLaneId,
    },
    SendLaneInput {
        lane_id: crate::AgentLaneId,
        input: String,
    },
    AcceptLaneOutput {
        lane_id: crate::AgentLaneId,
        summary: String,
    },
    ReviseLaneOutput {
        lane_id: crate::AgentLaneId,
        feedback: String,
    },
    DiscardLaneOutput {
        lane_id: crate::AgentLaneId,
        reason: String,
    },
    ApplyLaneChanges {
        lane_id: crate::AgentLaneId,
        unified_diff: String,
    },
    ResolveLaneConflict {
        lane_id: crate::AgentLaneId,
        unified_diff: String,
    },
    ArchiveLane {
        lane_id: crate::AgentLaneId,
        summary: String,
    },
    CleanupLane {
        lane_id: crate::AgentLaneId,
        force: bool,
    },
    StartAgentDag {
        goal: String,
        tasks: Vec<AgentDagTaskSpec>,
    },
    StartAgentTask {
        task_id: AgentTaskId,
    },
    CancelAgentTask {
        task_id: AgentTaskId,
    },
    CreateHandoff {
        handoff_id: String,
        task_id: AgentTaskId,
        from_lane_id: crate::AgentLaneId,
        to_lane_id: crate::AgentLaneId,
        owner: RuntimeOwner,
        summary: String,
        acceptance: HandoffAcceptance,
    },
    RequestReview {
        review_id: String,
        gate_id: MergeGateId,
        requester_lane_id: crate::AgentLaneId,
        reviewer_lane_id: crate::AgentLaneId,
        owner: RuntimeOwner,
        evidence_ids: Vec<EvidenceId>,
    },
    /// Records the independent reviewer lane's verdict on a pending review.
    ///
    /// The gate decision commands stay separate: this settles the review fact
    /// only, and `AcceptMergeGate` still fails closed against a rejected one.
    DecideReview {
        review_id: String,
        verdict: ReviewVerdict,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        feedback: Option<String>,
        #[serde(default)]
        actor: RuntimeOwner,
    },
    ConfirmContract {
        contract_id: String,
        task_id: AgentTaskId,
        owner: RuntimeOwner,
        summary: String,
        decision: ContractDecision,
    },
    SetDependency {
        dependency_id: String,
        task_id: AgentTaskId,
        depends_on_task_id: AgentTaskId,
        owner: RuntimeOwner,
        state: DependencyState,
        reason: String,
    },
    AcceptMergeGate {
        gate_id: MergeGateId,
        #[serde(default)]
        actor: RuntimeOwner,
        #[serde(default)]
        reviewed_evidence: Vec<ReviewedEvidenceBinding>,
        decision: Option<String>,
    },
    RejectMergeGate {
        gate_id: MergeGateId,
        #[serde(default)]
        actor: RuntimeOwner,
        reason: String,
    },
    RecordAgentEvidence {
        gate_id: MergeGateId,
        evidence_id: Option<EvidenceId>,
        kind: String,
        summary: String,
        path: Option<String>,
        source: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        canonical: Option<CanonicalEvidenceReference>,
    },
    AcceptAgentArtifact {
        gate_id: MergeGateId,
        evidence_id: EvidenceId,
        #[serde(default)]
        actor: RuntimeOwner,
        #[serde(default)]
        source_hash: String,
        decision: Option<String>,
    },
    RejectAgentArtifact {
        gate_id: MergeGateId,
        evidence_id: EvidenceId,
        #[serde(default)]
        actor: RuntimeOwner,
        reason: String,
    },
    MergeAgentPatch {
        gate_id: MergeGateId,
        #[serde(default)]
        actor: RuntimeOwner,
        decision: Option<String>,
    },
    RevalidateMergeConflict {
        gate_id: MergeGateId,
        bounce_id: String,
        actor: RuntimeOwner,
        evidence: ReviewedEvidenceBinding,
    },
    BounceMergeConflict {
        gate_id: MergeGateId,
        original_lane_id: crate::AgentLaneId,
        owner: RuntimeOwner,
        reason: String,
    },
    RevertAppliedChange {
        gate_id: MergeGateId,
        owner: RuntimeOwner,
        reason: String,
    },
    RetrieveContext {
        handle_id: String,
        reason: String,
    },
    LoadTranscriptPage {
        request: TranscriptPageRequest,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommandAction {
    pub id: String,
    pub label: String,
    pub command: RuntimeCommand,
    pub enabled: bool,
    pub disabled_reason: Option<String>,
    pub shortcut: Option<String>,
    pub destructive: bool,
}

/// Exact live worker identity exposed to frontend clients for owner-scoped
/// controls. The owner is copied from the worker handle and is never inferred
/// from durable Lane or display state.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LaneRuntimeOwnerBinding {
    pub lane_id: AgentLaneId,
    pub owner: RuntimeOwner,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ApprovalRequestView {
    pub id: String,
    pub tool_name: String,
    pub title: String,
    pub message: String,
    pub input_preview: String,
    pub is_mutating: bool,
    pub reason: Option<String>,
    #[serde(default)]
    pub owner: RuntimeOwner,
    #[serde(default = "default_approval_risk")]
    pub risk: ApprovalRisk,
    #[serde(default = "default_approval_target")]
    pub target: ApprovalTarget,
    #[serde(default)]
    pub allowed_scopes: Vec<ApprovalScope>,
    #[serde(default)]
    pub policy_reason_key: String,
    #[serde(default)]
    pub policy_reason_args: BTreeMap<String, String>,
    #[serde(default)]
    pub expires_at: u64,
    #[serde(default = "default_approval_action")]
    pub default_action: ApprovalDefaultAction,
    #[serde(default)]
    pub audit_id: String,
    /// What Core knows about the change this approval would make
    /// (`runtime.structured_diff`, GUI-CORE-012).
    ///
    /// Additive and omitted when absent, so an approval with no context
    /// encodes to exactly the bytes it did before this field existed and the
    /// frozen fixture corpus is unchanged. `None` means Core computed no
    /// preview for this tool — never "this tool changes nothing".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_context: Option<DecisionContext>,
}

fn default_approval_risk() -> ApprovalRisk {
    ApprovalRisk::Medium
}

fn default_approval_target() -> ApprovalTarget {
    ApprovalTarget {
        kind: String::new(),
        display: String::new(),
        canonical_ref: None,
    }
}

fn default_approval_action() -> ApprovalDefaultAction {
    ApprovalDefaultAction::Deny
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EvidenceView {
    pub id: String,
    pub kind: String,
    pub summary: String,
    pub path: Option<String>,
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical: Option<CanonicalEvidenceReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub timestamp: Option<u64>,
    /// Runtime owner this evidence was recorded under.
    ///
    /// Additive since core-0.3.6 (GUI-CORE-010). `None` means Core did not
    /// know the owner at emission — never a default owner, and never an owner
    /// a client may infer from timing, ordering, or display text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<RuntimeOwner>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CanonicalEvidenceReference {
    pub item_id: String,
    pub bundle_id: String,
    pub source_hash: String,
    pub producer: EvidenceProducer,
    pub permission_snapshot_id: Option<String>,
    pub permission_scope: ContextScope,
    pub evidence_scope: ContextScope,
    pub verification: EvidenceVerificationState,
    pub quality: EvidenceQualityFacts,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EvidenceProducer {
    pub identity: String,
    pub role: String,
    pub task_id: AgentTaskId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceVerificationState {
    Unverified,
    Verified,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EvidenceQualityFacts {
    pub status: EvidenceQualityStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reason_codes: Vec<EvidenceCanonicalReasonCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceQualityStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceCanonicalStatus {
    Missing,
    Verified,
    Blocked,
    NeedsChanges,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceCanonicalReasonCode {
    MissingCanonical,
    MissingRequiredKind,
    MissingSource,
    HashMismatch,
    ScopeMismatch,
    MissingPermissionSnapshot,
    InvalidPermissionSnapshot,
    MissingProducer,
    QualityFailed,
    VerificationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EvidenceCanonicalStatusReport {
    pub status: EvidenceCanonicalStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reason_codes: Vec<EvidenceCanonicalReasonCode>,
}

pub fn canonical_evidence_status(evidence: &EvidenceView) -> EvidenceCanonicalStatus {
    match &evidence.canonical {
        None => EvidenceCanonicalStatus::Missing,
        Some(canonical) if canonical.quality.status == EvidenceQualityStatus::Fail => {
            EvidenceCanonicalStatus::NeedsChanges
        }
        Some(canonical) if canonical.verification == EvidenceVerificationState::Verified => {
            EvidenceCanonicalStatus::Verified
        }
        Some(_) => EvidenceCanonicalStatus::Blocked,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ToolCallView {
    pub tool_call_id: ToolCallId,
    pub name: String,
    pub input_preview: String,
    /// Runtime owner whose turn started this tool call.
    ///
    /// Additive since core-0.3.6 (GUI-CORE-010). Copied verbatim from
    /// [`RuntimeEventKind::ToolCallStarted`]; `None` means Core did not know
    /// the owner at emission — never a default owner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<RuntimeOwner>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProviderHealthView {
    pub provider_id: String,
    pub model: String,
    pub status: String,
    pub request_count: u64,
    pub error_count: u64,
    pub last_latency_ms: Option<u64>,
    pub average_latency_ms: Option<u64>,
    pub tokens_per_second: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<CredentialHandle>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TokenCostView {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost_micro_usd: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeErrorView {
    pub message: String,
    pub recoverable: bool,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeCommandReceipt {
    pub command_id: String,
    pub command: RuntimeCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QueuedInputView {
    pub id: String,
    pub content_preview: String,
    pub created_at: Option<u64>,
    /// Runtime owner of the session that queued this input.
    ///
    /// Additive since core-0.3.6 (GUI-CORE-010). `None` means Core did not
    /// know the owner at emission — never a default owner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<RuntimeOwner>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LaneOutputView {
    pub lane_id: crate::AgentLaneId,
    pub stream: String,
    pub content: String,
    pub timestamp: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LaneConflictView {
    pub lane_id: crate::AgentLaneId,
    pub summary: String,
    pub paths: Vec<String>,
    pub timestamp: Option<u64>,
    /// The lines the failed lane apply collided with
    /// (`runtime.conflict_content`, GUI-CORE-015).
    ///
    /// Additive since core-0.3.6. `None` means Core produced no content for
    /// this conflict — a producer that predates the field, or an apply whose
    /// failure carried no per-hunk detail — never "nothing collided".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<crate::ConflictContent>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LaneRecoveryView {
    pub lane_id: crate::AgentLaneId,
    pub reason: String,
    pub next_action: String,
    pub timestamp: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeEvent {
    pub sequence: u64,
    pub timestamp: Option<u64>,
    pub kind: RuntimeEventKind,
}

impl RuntimeEvent {
    pub fn new(sequence: u64, kind: RuntimeEventKind) -> Self {
        Self {
            sequence,
            timestamp: Some(now_timestamp()),
            kind,
        }
    }

    pub fn with_timestamp(sequence: u64, timestamp: Option<u64>, kind: RuntimeEventKind) -> Self {
        Self {
            sequence,
            timestamp,
            kind,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
// This is a stable serialized runtime protocol enum. Boxing large payloads
// would churn public construction semantics without changing wire format.
#[allow(clippy::large_enum_variant)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
// Evolution discipline: adding an event must never be a breaking change for a
// sibling crate. `non_exhaustive` forces every out-of-crate match to carry a
// wildcard arm, which is the compile-time twin of the runtime rule that an
// unknown event type deserializes to `RuntimeWireEvent::Unknown` and a
// persisted unknown event is quarantined rather than fatal.
#[non_exhaustive]
pub enum RuntimeEventKind {
    AgentAdaptersLoaded {
        adapters: Vec<AgentAdapterView>,
    },
    AgentAdapterProbed {
        adapter: AgentAdapterView,
    },
    AgentSessionStarted {
        session: AgentSessionView,
    },
    AgentSessionUpdated {
        session: AgentSessionView,
    },
    AgentSessionCompleted {
        session: AgentSessionView,
    },
    AgentSessionFailed {
        session: AgentSessionView,
    },
    AgentSessionInputAccepted {
        session_id: SessionId,
        input_id: String,
    },
    WorkspaceEligibilityUpdated {
        eligibility: WorkspaceEligibility,
    },
    ProjectProbed {
        probe: ProjectProbe,
    },
    ProjectConfigPreviewed {
        preview: ProjectConfigPreview,
    },
    ProjectConfigConfirmed {
        preview: ProjectConfigPreview,
    },
    CredentialHandleStored {
        handle: CredentialHandle,
    },
    UiPreferencesUpdated {
        resolved: ResolvedUiPreferences,
        persisted: Option<UiPreferences>,
        diagnostics: Vec<UiPreferenceDiagnostic>,
    },
    /// The current cockpit layout record (`ui.layout_preferences`).
    ///
    /// Published as the answer to `SetUiLayoutPreferences` and
    /// `ResetUiLayoutPreferences`, and again on the snapshot prefix so a
    /// reconnecting client learns the record without asking.
    UiLayoutPreferencesUpdated {
        /// The exact command this answers. Absent on the snapshot prefix,
        /// where no client asked: a fabricated id there would let a client
        /// settle a request it never sent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        command_id: Option<String>,
        preferences: UiLayoutPreferences,
        /// Whether the record reached the config file. `false` means Core
        /// applied it for this session but could not write it, and the reason
        /// is in `diagnostics` — the `ui.preference_persistence` precedent. A
        /// client must render that difference rather than report a saved
        /// preference that will not survive a restart.
        persisted: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        diagnostics: Vec<UiPreferenceDiagnostic>,
    },
    RecentWorkLoaded {
        projects: Vec<RecentProjectSummary>,
        sessions: Vec<RecentSessionSummary>,
        diagnostics: Vec<String>,
    },
    /// Answer to `QueryAudit`. The page is a query result, not runtime view
    /// state: the timeline is unbounded and paginated, so it is deliberately
    /// not folded into `RuntimeViewState`.
    AuditPageLoaded {
        /// The exact `QueryAudit` command id this page answers.
        ///
        /// Additive since core-0.3.6 (GUI-CORE-024): without it a client could
        /// only correlate a page to its own *acceptance*, so a second client
        /// reading the same Core concurrently could land its page in the
        /// window between our acceptance and Core's answer. `None` means the
        /// page came from a Core that predates the field, where that
        /// acceptance-gated correlation is still the best available; a client
        /// must never fabricate an id for it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        command_id: Option<String>,
        page: AuditPage,
    },
    /// Answer to `QueryEvidence` (`runtime.evidence_reads`, GUI-CORE-025).
    ///
    /// A query result like the audit page above and deliberately never folded
    /// into `RuntimeViewState`. Folding it in would be worse than useless
    /// here: the view already carries `latest_evidence`, the recent window, so
    /// merging an archive page into it would let a paged read overwrite the
    /// live projection with older rows and leave a client unable to tell the
    /// two apart.
    EvidencePageLoaded {
        /// The exact `QueryEvidence` command id this page answers. Required,
        /// like `WorkspaceFilesLoaded`: this event is new, so a client never
        /// falls back to attributing a page to its own acceptance.
        command_id: String,
        page: EvidencePage,
    },
    /// Answer to `ReadEvidenceContent` (`runtime.evidence_reads`,
    /// GUI-CORE-025).
    ///
    /// `evidence_id` is echoed beside the command id so a client that expanded
    /// several rows attributes each answer without holding its own request
    /// table. `content` is verified canonical bytes or a typed unavailable
    /// reason; there is no variant for bytes Core could not verify.
    EvidenceContentLoaded {
        command_id: String,
        evidence_id: EvidenceId,
        content: EvidenceContent,
    },
    /// Answer to `QueryWorkspaceFiles`. Like an audit page this is a query
    /// result, not runtime view state: the inventory is unbounded and
    /// paginated, so it is deliberately not folded into `RuntimeViewState`.
    WorkspaceFilesLoaded {
        /// The exact `QueryWorkspaceFiles` command id this page answers.
        ///
        /// Required, not optional. `AuditPageLoaded` shipped this field as an
        /// addition and therefore carries a permanent `None` case
        /// (GUI-CORE-024); this event is new, so the correlation is mandatory
        /// from its first byte and a client never has to fall back to
        /// attributing a page to whatever read it happened to have in flight.
        command_id: String,
        page: WorkspaceFilePage,
    },
    /// Answer to `QueryWorkspaceDiff`. A query result like the two pages
    /// above: bounded, re-read on demand, and deliberately never folded into
    /// `RuntimeViewState`, so publishing one moves no snapshot digest.
    WorkspaceDiffLoaded {
        /// The exact `QueryWorkspaceDiff` command id this page answers.
        /// Required, like `WorkspaceFilesLoaded`: this event is new, so a
        /// client never falls back to attributing a page to its own
        /// acceptance.
        command_id: String,
        page: WorkspaceDiffPage,
    },
    /// The settled answer to a `RunOperatorGitAction`
    /// (`runtime.operator_git`, GUI-CORE-020).
    ///
    /// Published once the permission gate granted the action and the effect
    /// was attempted, whether it succeeded or not: a failure *after* a granted
    /// permission is a `Failed` outcome here, never a `CommandRejected`, which
    /// is reserved for refusals that happened before anything ran. The
    /// `WorkspaceSourceUpdated` that follows carries the resampled source for
    /// clients that only track the chip.
    OperatorGitActionFinished {
        /// The exact `RunOperatorGitAction` command id this answers.
        command_id: String,
        /// Echoed so a client that fanned out several actions can attribute
        /// this one without holding its own request table.
        target: SourceTarget,
        action: OperatorGitAction,
        outcome: OperatorGitOutcome,
        /// The audit record appended *before* the effect. A reader joins on it
        /// to find the authorization and, for a failure, the completion record
        /// that names it.
        audit_id: String,
    },
    WorkspaceSourceUpdated {
        source: WorkspaceSourceView,
    },
    /// The workspace-scoped operator identity Core minted at open
    /// (`runtime.workspace_owner`, GUI-CORE-027).
    ///
    /// Emitted once per open as the first fact after `SnapshotUpdated`, so a
    /// snapshot replay carries it, and again whenever the workspace is
    /// rebound. Its absence is a real answer — "this Core published no
    /// workspace identity" — and a client must render the workspace-target
    /// commit bar as unavailable rather than substitute
    /// `RuntimeOwner::default()`, which names nobody.
    WorkspaceRuntimeOwnerBound {
        binding: WorkspaceRuntimeOwnerBinding,
    },
    /// One Lane worktree's own source-control facts
    /// (`runtime.workspace_owner`).
    ///
    /// `WorkspaceSourceUpdated` keeps its meaning — the workspace root only —
    /// so a Lane's branch and ahead/behind arrive here instead of overwriting
    /// the workspace chip with another tree's numbers. A Lane with no worktree
    /// of its own is the workspace and therefore has no row at all; an empty
    /// map means Core sampled no Lane, never that every Lane is clean.
    LaneSourceUpdated {
        lane_id: crate::AgentLaneId,
        source: WorkspaceSourceView,
    },
    RuntimeServiceHealthUpdated {
        service: RuntimeServiceHealthView,
    },
    WorkspaceChangeUpdated {
        change: WorkspaceChangeView,
    },
    CheckRunUpdated {
        check: CheckRunView,
    },
    StarterLanePreviewed {
        preview: StarterLanePreview,
    },
    StarterLaneCreated {
        receipt: StarterLaneReceipt,
    },
    StarterLanePreviewInvalidated {
        owner: RuntimeOwner,
        preview_id: String,
        reason: StarterLanePreviewInvalidationReason,
    },
    SnapshotUpdated {
        snapshot: RuntimeSnapshot,
    },
    AssistantDelta {
        message_id: MessageId,
        task_id: Option<AgentTaskId>,
        /// Session this delta belongs to, when Core can scope it to one.
        ///
        /// Additive since `core-v0.3.6`: an older producer omits it and the
        /// delta stays in the unscoped assistant stream, exactly as before.
        /// A scoped delta additionally grows the owner-scoped conversation so
        /// a client can render a reply while it is still being produced.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_id: Option<SessionId>,
        content: String,
    },
    /// A typed non-text piece of an Agent message.
    ///
    /// Additive since `core-v0.3.6`. Text arrives as `AssistantDelta`; this
    /// carries what text cannot represent, so an image or file an Agent
    /// returned reaches the client instead of being dropped.
    AgentMessagePart {
        session_id: SessionId,
        message_id: MessageId,
        part: AgentContentPart,
    },
    ToolCallStarted {
        tool_call_id: ToolCallId,
        name: String,
        input_preview: String,
        /// Runtime owner whose turn started this call.
        ///
        /// Additive since core-0.3.6 (GUI-CORE-010). The reducer folds a
        /// `ToolCallView` out of these fields, so the owner has to travel on
        /// the event: `RuntimeViewState::apply_event` never sees the envelope.
        /// `None` means Core did not know the owner at emission.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        owner: Option<RuntimeOwner>,
    },
    ToolCallFinished {
        tool_call_id: ToolCallId,
        name: String,
        success: bool,
        exit_code: Option<i32>,
        evidence: Option<EvidenceView>,
    },
    ApprovalRequested {
        approval: ApprovalRequestView,
    },
    ApprovalResolved {
        request_id: String,
        decision: ApprovalDecision,
        owner: RuntimeOwner,
        audit_id: String,
    },
    CommandAccepted {
        command_id: String,
        command: RuntimeCommand,
    },
    LaneCommandAccepted {
        command_id: String,
        command: RuntimeCommand,
    },
    CommandRejected {
        command_id: String,
        reason: String,
    },
    TranscriptPageLoaded {
        page: Box<TranscriptPage>,
    },
    InputQueued {
        input: QueuedInputView,
    },
    InputDequeued {
        input_id: String,
    },
    TaskUpdated {
        task: AgentTaskRecord,
    },
    AgentDagUpdated {
        dag: AgentDagRecord,
    },
    LaneUpdated {
        lane: AgentLaneRecord,
    },
    LaneRuntimeOwnerBound {
        binding: LaneRuntimeOwnerBinding,
    },
    LaneOutputAppended {
        lane_id: crate::AgentLaneId,
        stream: String,
        content: String,
    },
    LaneConflictDetected {
        lane_id: crate::AgentLaneId,
        summary: String,
        paths: Vec<String>,
        /// Additive since core-0.3.6 (`runtime.conflict_content`,
        /// GUI-CORE-015). A payload written before the field deserializes to
        /// `None` rather than failing the stream.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<crate::ConflictContent>,
    },
    LaneRecoveryRequired {
        lane_id: crate::AgentLaneId,
        reason: String,
        next_action: String,
    },
    EvidenceRecorded {
        evidence: EvidenceView,
    },
    ContextUpdated {
        context: ContextBundleRecord,
    },
    ContextBundleBuilt {
        bundle_id: String,
        scope: ContextScope,
        handle_ids: Vec<String>,
        estimated_tokens: u64,
    },
    ContextItemStored {
        item: ContextItemRecord,
    },
    ContextViewDerived {
        view: ContextViewRecord,
        handle: ContextHandleRecord,
    },
    ContextReductionRecorded {
        reduction: ContextReductionRecord,
    },
    ContextRetrieved {
        retrieval: ContextRetrievalRecord,
    },
    ContextBudgetExceeded {
        budget: ContextBudgetRecord,
    },
    ContextQualityFailed {
        quality: ContextQualityRecord,
    },
    CostUsageRecorded {
        cost: CostUsageRecord,
    },
    ProviderCacheObserved {
        provider_id: String,
        model: String,
        cached_input_tokens: u64,
        cache_hit_microunits: u64,
    },
    EvidenceCanonicalized {
        evidence_id: EvidenceId,
        item_id: String,
        content_sha256: String,
    },
    MergeGateUpdated {
        gate: MergeGateRecord,
    },
    HandoffUpdated {
        handoff: HandoffRecord,
    },
    ReviewRequestUpdated {
        review: ReviewRequestRecord,
    },
    ContractUpdated {
        contract: ContractRecord,
    },
    DependencyUpdated {
        dependency: DependencyRecord,
    },
    MergeConflictBounced {
        conflict: ConflictBounce,
    },
    RevertRecorded {
        revert: RevertRecord,
    },
    ProviderHealthUpdated {
        provider: ProviderHealthView,
    },
    TokenCostUpdated {
        cost: TokenCostView,
    },
    Error {
        error: RuntimeErrorView,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeViewState {
    pub snapshot: RuntimeSnapshot,
    // The live reducer mirrors this projection for in-process consumers. The
    // frozen v1 wire view continues to carry the same fact in `snapshot`.
    #[serde(skip)]
    pub ui_preferences: ResolvedUiPreferences,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_adapters: Vec<AgentAdapterView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_sessions: Vec<AgentSessionView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_session_inputs: Vec<AgentSessionInputView>,
    /// Ordered user/assistant messages reduced from typed Agent session events.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_conversation: Vec<AgentConversationMessageView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_projects: Vec<RecentProjectSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_sessions: Vec<RecentSessionSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_work_diagnostics: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_source: Option<WorkspaceSourceView>,
    /// The workspace-scoped operator identity Core published, if any
    /// (`runtime.workspace_owner`). Absent means Core published none: a
    /// client gates its workspace-target commit bar on this being present and
    /// never fabricates one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_owner: Option<RuntimeOwner>,
    /// Per-Lane source-control facts, keyed by Lane id. A Lane with no
    /// worktree of its own has no row, because its source *is*
    /// `workspace_source`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub lane_sources: BTreeMap<AgentLaneId, WorkspaceSourceView>,
    /// The persisted cockpit layout record (`ui.layout_preferences`). Absent
    /// until Core publishes one, which is what keeps every frozen base fixture
    /// digest where it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_preferences: Option<UiLayoutPreferences>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_services: Vec<RuntimeServiceHealthView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workspace_changes: Vec<WorkspaceChangeView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub check_runs: Vec<CheckRunView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub starter_lane_previews: Vec<StarterLanePreview>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub starter_lane_receipts: Vec<StarterLaneReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_probe: Option<ProjectProbe>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_eligibility: Option<WorkspaceEligibility>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_config_preview: Option<ProjectConfigPreview>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed_project_config: Option<ProjectConfigPreview>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credential_handles: Vec<CredentialHandle>,
    pub pending_approvals: Vec<ApprovalRequestView>,
    pub queued_inputs: Vec<QueuedInputView>,
    pub active_tool_calls: Vec<ToolCallView>,
    pub tasks: Vec<AgentTaskRecord>,
    pub agent_dags: Vec<AgentDagRecord>,
    pub lanes: Vec<AgentLaneRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lane_runtime_owners: Vec<LaneRuntimeOwnerBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lane_outputs: Vec<LaneOutputView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lane_conflicts: Vec<LaneConflictView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lane_recoveries: Vec<LaneRecoveryView>,
    pub latest_evidence: Vec<EvidenceView>,
    pub assistant_stream: String,
    pub context: Option<ContextBundleRecord>,
    #[serde(default)]
    pub context_bundles: Vec<ContextBundleSummaryRecord>,
    #[serde(default)]
    pub context_handles: Vec<ContextHandleRecord>,
    #[serde(default)]
    pub context_items: Vec<ContextItemRecord>,
    #[serde(default)]
    pub context_views: Vec<ContextViewRecord>,
    #[serde(default)]
    pub context_reductions: Vec<ContextReductionRecord>,
    #[serde(default)]
    pub context_retrievals: Vec<ContextRetrievalRecord>,
    #[serde(default)]
    pub context_budgets: Vec<ContextBudgetRecord>,
    #[serde(default)]
    pub context_quality: Vec<ContextQualityRecord>,
    #[serde(default)]
    pub cost_usage: Vec<CostUsageRecord>,
    #[serde(default)]
    pub cost_ledger: CostLedgerTotals,
    #[serde(default, skip)]
    seen_cost_usage_ids: BTreeSet<String>,
    #[serde(default)]
    pub provider_cache_observations: Vec<ProviderCacheObservationRecord>,
    #[serde(default)]
    pub canonical_evidence: Vec<EvidenceCanonicalizationRecord>,
    pub provider: Option<ProviderHealthView>,
    pub token_cost: Option<TokenCostView>,
    pub merge_gates: Vec<MergeGateRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handoffs: Vec<HandoffRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub review_requests: Vec<ReviewRequestRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contracts: Vec<ContractRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<DependencyRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflict_bounces: Vec<ConflictBounce>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reverts: Vec<RevertRecord>,
    pub errors: Vec<RuntimeErrorView>,
    pub last_command: Option<RuntimeCommandReceipt>,
}

impl RuntimeViewState {
    pub fn new(snapshot: RuntimeSnapshot) -> Self {
        let ui_preferences = snapshot.ui_preferences.clone();
        Self {
            snapshot,
            ui_preferences,
            agent_adapters: Vec::new(),
            agent_sessions: Vec::new(),
            agent_session_inputs: Vec::new(),
            agent_conversation: Vec::new(),
            recent_projects: Vec::new(),
            recent_sessions: Vec::new(),
            recent_work_diagnostics: Vec::new(),
            workspace_source: None,
            workspace_owner: None,
            lane_sources: BTreeMap::new(),
            layout_preferences: None,
            runtime_services: Vec::new(),
            workspace_changes: Vec::new(),
            check_runs: Vec::new(),
            starter_lane_previews: Vec::new(),
            starter_lane_receipts: Vec::new(),
            project_probe: None,
            workspace_eligibility: None,
            project_config_preview: None,
            confirmed_project_config: None,
            credential_handles: Vec::new(),
            pending_approvals: Vec::new(),
            queued_inputs: Vec::new(),
            active_tool_calls: Vec::new(),
            tasks: Vec::new(),
            agent_dags: Vec::new(),
            lanes: Vec::new(),
            lane_runtime_owners: Vec::new(),
            lane_outputs: Vec::new(),
            lane_conflicts: Vec::new(),
            lane_recoveries: Vec::new(),
            latest_evidence: Vec::new(),
            assistant_stream: String::new(),
            context: None,
            context_bundles: Vec::new(),
            context_handles: Vec::new(),
            context_items: Vec::new(),
            context_views: Vec::new(),
            context_reductions: Vec::new(),
            context_retrievals: Vec::new(),
            context_budgets: Vec::new(),
            context_quality: Vec::new(),
            cost_usage: Vec::new(),
            cost_ledger: CostLedgerTotals::default(),
            seen_cost_usage_ids: BTreeSet::new(),
            provider_cache_observations: Vec::new(),
            canonical_evidence: Vec::new(),
            provider: None,
            token_cost: None,
            merge_gates: Vec::new(),
            handoffs: Vec::new(),
            review_requests: Vec::new(),
            contracts: Vec::new(),
            dependencies: Vec::new(),
            conflict_bounces: Vec::new(),
            reverts: Vec::new(),
            errors: Vec::new(),
            last_command: None,
        }
    }

    pub fn apply_event(&mut self, event: &RuntimeEvent) {
        match &event.kind {
            RuntimeEventKind::AgentAdaptersLoaded { adapters } => {
                self.agent_adapters = adapters.clone();
                cap_vec(&mut self.agent_adapters);
            }
            RuntimeEventKind::AgentAdapterProbed { adapter } => {
                upsert_by_id(&mut self.agent_adapters, adapter.clone(), |existing| {
                    existing.agent_id == adapter.agent_id
                });
                cap_vec(&mut self.agent_adapters);
            }
            RuntimeEventKind::AgentSessionStarted { session }
            | RuntimeEventKind::AgentSessionUpdated { session }
            | RuntimeEventKind::AgentSessionCompleted { session }
            | RuntimeEventKind::AgentSessionFailed { session } => {
                // A terminal turn settles the unscoped assistant stream.
                //
                // That stream is the in-flight display surface only. Once a
                // turn ends its reply is carried by the terminal fact's
                // `session.output` and by the owner-scoped
                // `agent_conversation`, so retaining the text here only grows
                // an unattributable blob across turns, sessions, and replays.
                //
                // Concurrency caveat: the stream carries no session identity,
                // so with concurrent sessions it was already interleaved text
                // from every producer. Clearing on any terminal event is
                // therefore no less attributable than keeping it.
                //
                // This runs outside the lane-owner guard below because the
                // unscoped stream is not part of that per-lane session
                // projection: a delta reached it without any ownership check,
                // so its settlement must not require one either.
                let settles_turn =
                    matches!(
                        event.kind,
                        RuntimeEventKind::AgentSessionCompleted { .. }
                            | RuntimeEventKind::AgentSessionFailed { .. }
                    ) || (matches!(event.kind, RuntimeEventKind::AgentSessionUpdated { .. })
                        && matches!(
                            session.status,
                            AgentSessionStatus::Completed
                                | AgentSessionStatus::Failed
                                | AgentSessionStatus::Cancelled
                        ));
                if settles_turn {
                    self.assistant_stream.clear();
                }
                let existing_session_owner_matches = self
                    .agent_sessions
                    .iter()
                    .find(|existing| existing.session_id == session.session_id)
                    .is_some_and(|existing| existing.owner == session.owner);
                let session_is_new = self
                    .agent_sessions
                    .iter()
                    .all(|existing| existing.session_id != session.session_id);
                let lane_is_unbound = self
                    .agent_sessions
                    .iter()
                    .all(|existing| existing.lane_id != session.lane_id);
                // A Lane's first session identity is authoritative during replay.
                // Later sessions cannot silently join or replace that projection.
                if existing_session_owner_matches || (session_is_new && lane_is_unbound) {
                    if matches!(event.kind, RuntimeEventKind::AgentSessionStarted { .. }) {
                        let accepted_inputs = self
                            .agent_session_inputs
                            .iter()
                            .filter(|input| input.session_id == session.session_id)
                            .count();
                        let user_messages = self
                            .agent_conversation
                            .iter()
                            .filter(|message| {
                                message.session_id == session.session_id
                                    && message.role == AgentConversationRole::User
                            })
                            .count();
                        // Initial task + accepted follow-ups define user-message count.
                        // Retry events carry the same task without an accepted input.
                        if user_messages < accepted_inputs.saturating_add(1) {
                            append_agent_conversation_message(
                                &mut self.agent_conversation,
                                &session.session_id,
                                AgentConversationRole::User,
                                &session.task,
                            );
                        }
                    }
                    if matches!(event.kind, RuntimeEventKind::AgentSessionCompleted { .. })
                        && let Some(output) = session.output.as_deref()
                        && !output.trim().is_empty()
                        && !self
                            .agent_conversation
                            .iter()
                            .rev()
                            .find(|message| message.session_id == session.session_id)
                            .is_some_and(|message| {
                                message.role == AgentConversationRole::Assistant
                                    && message.content == output
                            })
                    {
                        append_agent_conversation_message(
                            &mut self.agent_conversation,
                            &session.session_id,
                            AgentConversationRole::Assistant,
                            output,
                        );
                    }
                    upsert_by_id(&mut self.agent_sessions, session.clone(), |existing| {
                        existing.session_id == session.session_id
                    });
                    cap_vec(&mut self.agent_sessions);
                    // Lane creation binds its worker before an Agent session exists.
                    // Promote that provisional identity exactly once so the first
                    // Core-published session becomes the Lane's routable owner.
                    if session.owner.lane_id.as_ref() == Some(&session.lane_id)
                        && session.owner.session_id.as_ref() == Some(&session.session_id)
                        && let Some(binding) = self.lane_runtime_owners.iter_mut().find(|binding| {
                            binding.lane_id == session.lane_id
                                && binding.owner.session_id.is_none()
                                && binding.owner.workspace_id == session.owner.workspace_id
                                && binding.owner.project_id == session.owner.project_id
                                && binding.owner.lane_id == session.owner.lane_id
                        })
                    {
                        binding.owner = session.owner.clone();
                    }
                }
            }
            RuntimeEventKind::AgentSessionInputAccepted {
                session_id,
                input_id,
            } => {
                upsert_by_id(
                    &mut self.agent_session_inputs,
                    AgentSessionInputView {
                        session_id: session_id.clone(),
                        input_id: input_id.clone(),
                    },
                    |existing| existing.input_id == *input_id,
                );
                cap_vec(&mut self.agent_session_inputs);
            }
            RuntimeEventKind::WorkspaceEligibilityUpdated { eligibility } => {
                self.workspace_eligibility = Some(eligibility.clone());
            }
            RuntimeEventKind::ProjectProbed { probe } => {
                self.project_probe = Some(probe.clone());
            }
            RuntimeEventKind::ProjectConfigPreviewed { preview } => {
                self.project_config_preview = Some(preview.clone());
            }
            RuntimeEventKind::ProjectConfigConfirmed { preview } => {
                self.confirmed_project_config = Some(preview.clone());
                self.project_config_preview = None;
            }
            RuntimeEventKind::CredentialHandleStored { handle } => {
                upsert_by_id(&mut self.credential_handles, handle.clone(), |existing| {
                    existing.provider_id == handle.provider_id
                        && existing.backend_id == handle.backend_id
                });
            }
            RuntimeEventKind::UiPreferencesUpdated { resolved, .. } => {
                self.ui_preferences = resolved.clone();
                self.snapshot.ui_preferences = resolved.clone();
            }
            // Deliberately does not touch `ui_preferences` or
            // `snapshot.ui_preferences`: the layout record is a separate fact
            // precisely so the resolved preference profile every base fixture
            // serializes stays where it is.
            RuntimeEventKind::UiLayoutPreferencesUpdated { preferences, .. } => {
                self.layout_preferences = Some(preferences.clone());
            }
            RuntimeEventKind::RecentWorkLoaded {
                projects,
                sessions,
                diagnostics,
            } => {
                self.recent_projects = projects.clone();
                self.recent_sessions = sessions.clone();
                self.recent_work_diagnostics = diagnostics.clone();
            }
            // A loaded audit page answers one paginated query; folding it into
            // the capped view collections would silently truncate the
            // timeline, so the view state deliberately ignores it.
            RuntimeEventKind::AuditPageLoaded { .. } => {}
            // Same rule as an audit page, with a sharper reason: the view
            // already carries `latest_evidence`, the recent window. Folding an
            // archive page into it would let a paged read of old rows
            // overwrite the live projection, and a client would have no way to
            // tell which of the two it was looking at.
            RuntimeEventKind::EvidencePageLoaded { .. } => {}
            // The bytes behind one row, answered on demand and re-asked
            // whenever the row is reopened. Storing them would keep an
            // arbitrary blob alive in view state and in every snapshot after
            // it, for a value that is only ever rendered once.
            RuntimeEventKind::EvidenceContentLoaded { .. } => {}
            // Same rule as an audit page: one paginated query result. Folding
            // an inventory page into the capped view collections would
            // silently truncate the tree a client is paging through.
            RuntimeEventKind::WorkspaceFilesLoaded { .. } => {}
            // Same rule again: a bounded diff read answered on demand. It is
            // re-asked whenever a reviewer opens a file, so folding it into
            // view state would keep a stale diff alive after the tree moved.
            RuntimeEventKind::WorkspaceDiffLoaded { .. } => {}
            // A settled operator action is an answer to one command, not a
            // fact about the workspace. The workspace fact it produced arrives
            // as the `WorkspaceSourceUpdated` below and *is* reduced; folding
            // the outcome in as well would give a client two sources for one
            // truth and let a stale action outcome outlive the tree it
            // described.
            RuntimeEventKind::OperatorGitActionFinished { .. } => {}
            RuntimeEventKind::WorkspaceSourceUpdated { source } => {
                self.workspace_source = Some(source.clone());
            }
            RuntimeEventKind::WorkspaceRuntimeOwnerBound { binding } => {
                // A payload that does not describe the workspace scope is
                // untrusted protocol input. Storing it would hand a client an
                // authority Core never published — the one failure a
                // workspace-scoped actor exists to prevent — so an invalid
                // binding leaves the projection exactly as it was.
                if binding.validate().is_ok() {
                    self.workspace_owner = Some(binding.owner.clone());
                }
            }
            RuntimeEventKind::LaneSourceUpdated { lane_id, source } => {
                self.lane_sources.insert(lane_id.clone(), source.clone());
                // Keyed by Lane, so the map is bounded by the Lane count the
                // rest of the view already caps rather than by event volume.
                if self.lane_sources.len() > RUNTIME_VIEW_COLLECTION_LIMIT
                    && let Some(oldest) = self.lane_sources.keys().next().cloned()
                    && oldest != *lane_id
                {
                    self.lane_sources.remove(&oldest);
                }
            }
            RuntimeEventKind::RuntimeServiceHealthUpdated { service } => {
                upsert_by_id(&mut self.runtime_services, service.clone(), |existing| {
                    existing.kind == service.kind && existing.id == service.id
                });
                cap_vec(&mut self.runtime_services);
            }
            RuntimeEventKind::WorkspaceChangeUpdated { change } => {
                upsert_by_id(&mut self.workspace_changes, change.clone(), |existing| {
                    existing.owner == change.owner && existing.id == change.id
                });
                cap_vec(&mut self.workspace_changes);
            }
            RuntimeEventKind::CheckRunUpdated { check } => {
                upsert_by_id(&mut self.check_runs, check.clone(), |existing| {
                    existing.owner == check.owner && existing.id == check.id
                });
                cap_vec(&mut self.check_runs);
            }
            RuntimeEventKind::StarterLanePreviewed { preview } => {
                upsert_by_id(
                    &mut self.starter_lane_previews,
                    preview.clone(),
                    |existing| {
                        existing.owner == preview.owner && existing.preview_id == preview.preview_id
                    },
                );
                cap_vec(&mut self.starter_lane_previews);
            }
            RuntimeEventKind::StarterLaneCreated { receipt } => {
                self.starter_lane_previews.retain(|preview| {
                    preview.preview_id != receipt.preview_id || preview.owner != receipt.owner
                });
                upsert_by_id(
                    &mut self.starter_lane_receipts,
                    receipt.clone(),
                    |existing| {
                        existing.owner == receipt.owner && existing.preview_id == receipt.preview_id
                    },
                );
                cap_vec(&mut self.starter_lane_receipts);
                upsert_by_id(&mut self.lanes, receipt.lane.clone(), |existing| {
                    existing.id == receipt.lane.id
                });
            }
            RuntimeEventKind::StarterLanePreviewInvalidated {
                owner, preview_id, ..
            } => {
                self.starter_lane_previews
                    .retain(|preview| preview.owner != *owner || preview.preview_id != *preview_id);
            }
            RuntimeEventKind::SnapshotUpdated { snapshot } => {
                self.snapshot = snapshot.clone();
                self.ui_preferences = snapshot.ui_preferences.clone();
            }
            RuntimeEventKind::AssistantDelta {
                message_id,
                session_id,
                content,
                ..
            } => {
                self.assistant_stream.push_str(content);
                // A scoped delta grows exactly one conversation message per
                // turn, keyed by the message id the producer keeps stable.
                // Without a session id the delta cannot be attributed to a
                // conversation, so it stays in the unscoped stream only.
                if let Some(session_id) = session_id {
                    grow_agent_conversation_message(
                        &mut self.agent_conversation,
                        session_id,
                        message_id,
                        content,
                    );
                }
            }
            RuntimeEventKind::AgentMessagePart {
                session_id,
                message_id,
                part,
            } => {
                // Parts attach to a message Core already published. An orphan
                // part is dropped rather than conjuring a message that never
                // had text.
                if let Some(message) = self.agent_conversation.iter_mut().find(|message| {
                    message.message_id == *message_id && message.session_id == *session_id
                }) {
                    message.parts.push(part.clone());
                }
            }
            RuntimeEventKind::ToolCallStarted {
                tool_call_id,
                name,
                input_preview,
                owner,
            } => upsert_by_id(
                &mut self.active_tool_calls,
                ToolCallView {
                    tool_call_id: tool_call_id.clone(),
                    name: name.clone(),
                    input_preview: input_preview.clone(),
                    owner: owner.clone(),
                },
                |tool| tool.tool_call_id == *tool_call_id,
            ),
            RuntimeEventKind::ToolCallFinished {
                tool_call_id,
                evidence,
                ..
            } => {
                self.active_tool_calls
                    .retain(|tool| tool.tool_call_id != *tool_call_id);
                if let Some(evidence) = evidence {
                    upsert_by_id(&mut self.latest_evidence, evidence.clone(), |existing| {
                        existing.id == evidence.id
                    });
                }
            }
            RuntimeEventKind::ApprovalRequested { approval } => {
                upsert_by_id(&mut self.pending_approvals, approval.clone(), |existing| {
                    existing.id == approval.id
                })
            }
            RuntimeEventKind::ApprovalResolved { request_id, .. } => {
                self.pending_approvals
                    .retain(|approval| approval.id != *request_id);
            }
            RuntimeEventKind::CommandAccepted {
                command_id,
                command,
            }
            | RuntimeEventKind::LaneCommandAccepted {
                command_id,
                command,
            } => {
                self.last_command = Some(RuntimeCommandReceipt {
                    command_id: command_id.clone(),
                    command: command.clone(),
                });
            }
            RuntimeEventKind::CommandRejected { command_id, reason } => {
                self.last_command = None;
                self.errors.push(RuntimeErrorView {
                    message: format!("command {command_id} rejected: {reason}"),
                    recoverable: true,
                    hint: None,
                });
            }
            RuntimeEventKind::TranscriptPageLoaded { .. } => {
                // Transcript pages are transient fetch results. They do not
                // mutate the long-lived runtime projection.
            }
            RuntimeEventKind::InputQueued { input } => {
                upsert_by_id(&mut self.queued_inputs, input.clone(), |existing| {
                    existing.id == input.id
                });
            }
            RuntimeEventKind::InputDequeued { input_id } => {
                self.queued_inputs.retain(|input| input.id != *input_id);
            }
            RuntimeEventKind::TaskUpdated { task } => {
                upsert_by_id(&mut self.tasks, task.clone(), |existing| {
                    existing.id == task.id
                });
            }
            RuntimeEventKind::AgentDagUpdated { dag } => {
                upsert_by_id(&mut self.agent_dags, dag.clone(), |existing| {
                    existing.dag_id == dag.dag_id
                });
            }
            RuntimeEventKind::LaneUpdated { lane } => {
                upsert_by_id(&mut self.lanes, lane.clone(), |existing| {
                    existing.id == lane.id
                });
                if matches!(
                    lane.status,
                    LaneStatus::Done
                        | LaneStatus::Failed
                        | LaneStatus::Cancelled
                        | LaneStatus::Archived
                ) {
                    self.lane_runtime_owners
                        .retain(|binding| binding.lane_id != lane.id);
                }
            }
            RuntimeEventKind::LaneRuntimeOwnerBound { binding } => {
                // A mismatched payload is untrusted protocol input. Never
                // normalize it into an authority the Core did not publish.
                // A Lane's first valid execution owner is authoritative for
                // the view lifetime. Exact replay is already idempotent, and
                // a later owner/session cannot replace the existing binding.
                let lane_is_terminal = self
                    .lanes
                    .iter()
                    .find(|lane| lane.id == binding.lane_id)
                    .is_some_and(|lane| {
                        matches!(
                            lane.status,
                            LaneStatus::Done
                                | LaneStatus::Failed
                                | LaneStatus::Cancelled
                                | LaneStatus::Archived
                        )
                    });
                if binding.owner.lane_id.as_ref() == Some(&binding.lane_id)
                    && !lane_is_terminal
                    && self
                        .lane_runtime_owners
                        .iter()
                        .all(|existing| existing.lane_id != binding.lane_id)
                {
                    self.lane_runtime_owners.push(binding.clone());
                }
            }
            RuntimeEventKind::LaneOutputAppended {
                lane_id,
                stream,
                content,
            } => {
                self.lane_outputs.push(LaneOutputView {
                    lane_id: lane_id.clone(),
                    stream: stream.clone(),
                    content: content.clone(),
                    timestamp: event.timestamp,
                });
                cap_vec(&mut self.lane_outputs);
            }
            RuntimeEventKind::LaneConflictDetected {
                lane_id,
                summary,
                paths,
                content,
            } => {
                upsert_by_id(
                    &mut self.lane_conflicts,
                    LaneConflictView {
                        lane_id: lane_id.clone(),
                        summary: summary.clone(),
                        paths: paths.clone(),
                        timestamp: event.timestamp,
                        // Copied verbatim, absence included: the view states
                        // what this event carried, never what an earlier one
                        // did, so a client cannot read stale lines as current.
                        content: content.clone(),
                    },
                    |existing| existing.lane_id == *lane_id,
                );
                cap_vec(&mut self.lane_conflicts);
            }
            RuntimeEventKind::LaneRecoveryRequired {
                lane_id,
                reason,
                next_action,
            } => {
                upsert_by_id(
                    &mut self.lane_recoveries,
                    LaneRecoveryView {
                        lane_id: lane_id.clone(),
                        reason: reason.clone(),
                        next_action: next_action.clone(),
                        timestamp: event.timestamp,
                    },
                    |existing| existing.lane_id == *lane_id,
                );
                cap_vec(&mut self.lane_recoveries);
            }
            RuntimeEventKind::EvidenceRecorded { evidence } => {
                upsert_by_id(&mut self.latest_evidence, evidence.clone(), |existing| {
                    existing.id == evidence.id
                });
            }
            RuntimeEventKind::ContextUpdated { context } => {
                self.context = Some(context.clone());
            }
            RuntimeEventKind::ContextBundleBuilt {
                bundle_id,
                scope,
                handle_ids,
                estimated_tokens,
            } => {
                upsert_by_id(
                    &mut self.context_bundles,
                    ContextBundleSummaryRecord {
                        bundle_id: bundle_id.clone(),
                        scope: scope.clone(),
                        handle_ids: handle_ids.clone(),
                        estimated_tokens: *estimated_tokens,
                    },
                    |existing| existing.bundle_id == *bundle_id,
                );
                cap_vec(&mut self.context_bundles);
            }
            RuntimeEventKind::ContextItemStored { item } => {
                upsert_by_id(&mut self.context_items, item.clone(), |existing| {
                    existing.item_id == item.item_id
                });
                cap_vec(&mut self.context_items);
            }
            RuntimeEventKind::ContextViewDerived { view, handle } => {
                upsert_by_id(&mut self.context_views, view.clone(), |existing| {
                    existing.view_id == view.view_id
                });
                upsert_by_id(&mut self.context_handles, handle.clone(), |existing| {
                    existing.handle_id == handle.handle_id
                });
                cap_vec(&mut self.context_views);
                cap_vec(&mut self.context_handles);
            }
            RuntimeEventKind::ContextReductionRecorded { reduction } => {
                let reduction = sanitize_context_reduction_record(reduction);
                let reduction_id = reduction.reduction_id.clone();
                upsert_by_id(&mut self.context_reductions, reduction, |existing| {
                    existing.reduction_id == reduction_id
                });
                cap_vec(&mut self.context_reductions);
            }
            RuntimeEventKind::ContextRetrieved { retrieval } => {
                self.context_retrievals.push(retrieval.clone());
                cap_vec(&mut self.context_retrievals);
            }
            RuntimeEventKind::ContextBudgetExceeded { budget } => {
                upsert_by_id(&mut self.context_budgets, budget.clone(), |existing| {
                    existing.budget_id == budget.budget_id
                });
                cap_vec(&mut self.context_budgets);
            }
            RuntimeEventKind::ContextQualityFailed { quality } => {
                upsert_by_id(&mut self.context_quality, quality.clone(), |existing| {
                    existing.quality_id == quality.quality_id
                });
                cap_vec(&mut self.context_quality);
            }
            RuntimeEventKind::CostUsageRecorded { cost } => {
                if self.seen_cost_usage_ids.insert(cost.usage_id.clone()) {
                    self.cost_ledger.record(cost);
                    self.cost_usage.push(cost.clone());
                    if self
                        .cost_usage
                        .iter()
                        .any(|record| record.actual_cost.is_none())
                    {
                        self.cost_ledger.actual_cost = None;
                        self.cost_ledger.total_actual_cost_micro_usd = None;
                    }
                }
                cap_vec(&mut self.cost_usage);
            }
            RuntimeEventKind::ProviderCacheObserved {
                provider_id,
                model,
                cached_input_tokens,
                cache_hit_microunits,
            } => {
                self.provider_cache_observations
                    .push(ProviderCacheObservationRecord {
                        provider_id: provider_id.clone(),
                        model: model.clone(),
                        cached_input_tokens: *cached_input_tokens,
                        cache_hit_microunits: *cache_hit_microunits,
                    });
                cap_vec(&mut self.provider_cache_observations);
            }
            RuntimeEventKind::EvidenceCanonicalized {
                evidence_id,
                item_id,
                content_sha256,
            } => {
                upsert_by_id(
                    &mut self.canonical_evidence,
                    EvidenceCanonicalizationRecord {
                        evidence_id: evidence_id.clone(),
                        item_id: item_id.clone(),
                        content_sha256: content_sha256.clone(),
                    },
                    |existing| existing.evidence_id == *evidence_id,
                );
                cap_vec(&mut self.canonical_evidence);
            }
            RuntimeEventKind::MergeGateUpdated { gate } => {
                upsert_by_id(&mut self.merge_gates, gate.clone(), |existing| {
                    existing.gate_id == gate.gate_id
                });
            }
            RuntimeEventKind::HandoffUpdated { handoff } => {
                upsert_by_id(&mut self.handoffs, handoff.clone(), |existing| {
                    existing.handoff_id == handoff.handoff_id
                });
            }
            RuntimeEventKind::ReviewRequestUpdated { review } => {
                upsert_by_id(&mut self.review_requests, review.clone(), |existing| {
                    existing.review_id == review.review_id
                });
            }
            RuntimeEventKind::ContractUpdated { contract } => {
                upsert_by_id(&mut self.contracts, contract.clone(), |existing| {
                    existing.contract_id == contract.contract_id
                });
            }
            RuntimeEventKind::DependencyUpdated { dependency } => {
                upsert_by_id(&mut self.dependencies, dependency.clone(), |existing| {
                    existing.dependency_id == dependency.dependency_id
                });
            }
            RuntimeEventKind::MergeConflictBounced { conflict } => {
                upsert_by_id(&mut self.conflict_bounces, conflict.clone(), |existing| {
                    existing.bounce_id == conflict.bounce_id
                });
            }
            RuntimeEventKind::RevertRecorded { revert } => {
                upsert_by_id(&mut self.reverts, revert.clone(), |existing| {
                    existing.revert_id == revert.revert_id
                });
            }
            RuntimeEventKind::ProviderHealthUpdated { provider } => {
                self.provider = Some(provider.clone());
            }
            RuntimeEventKind::TokenCostUpdated { cost } => {
                self.token_cost = Some(cost.clone());
            }
            RuntimeEventKind::Error { error } => {
                self.errors.push(error.clone());
            }
        }
    }
}

/// Appends a streamed delta to the assistant message it belongs to, creating
/// that message on the first chunk.
///
/// The producer keeps `message_id` stable for one turn, so a growing reply is
/// one message rather than one message per chunk. Ordering is the event order
/// Core published; nothing here reorders or merges by content.
fn grow_agent_conversation_message(
    messages: &mut Vec<AgentConversationMessageView>,
    session_id: &str,
    message_id: &str,
    delta: &str,
) {
    if let Some(existing) = messages
        .iter_mut()
        .find(|message| message.message_id == message_id && message.session_id == session_id)
    {
        existing.content.push_str(delta);
        return;
    }
    messages.push(AgentConversationMessageView {
        message_id: message_id.to_string(),
        session_id: session_id.to_string(),
        role: AgentConversationRole::Assistant,
        content: delta.to_string(),
        // A streamed reply starts as text; non-text parts arrive with the
        // completed message from the adapter.
        parts: Vec::new(),
    });
    cap_vec(messages);
}

fn append_agent_conversation_message(
    messages: &mut Vec<AgentConversationMessageView>,
    session_id: &str,
    role: AgentConversationRole,
    content: &str,
) {
    let prefix = format!("{session_id}-message-");
    let ordinal = messages
        .iter()
        .filter(|message| message.session_id == session_id)
        .filter_map(|message| message.message_id.strip_prefix(&prefix))
        .filter_map(|ordinal| ordinal.parse::<usize>().ok())
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    messages.push(AgentConversationMessageView {
        message_id: format!("{session_id}-message-{ordinal}"),
        session_id: session_id.to_string(),
        role,
        content: content.to_string(),
        parts: Vec::new(),
    });
    cap_vec(messages);
}

fn cap_vec<T>(items: &mut Vec<T>) {
    let excess = items.len().saturating_sub(RUNTIME_VIEW_COLLECTION_LIMIT);
    if excess > 0 {
        items.drain(0..excess);
    }
}

fn sanitize_context_reduction_record(reduction: &ContextReductionRecord) -> ContextReductionRecord {
    ContextReductionRecord {
        reduction_id: sanitize_runtime_atom(&reduction.reduction_id, 96),
        item_id: sanitize_runtime_atom(&reduction.item_id, 96),
        view_id: reduction
            .view_id
            .as_deref()
            .map(|view_id| sanitize_runtime_atom(view_id, 96)),
        reducer_id: sanitize_runtime_atom(&reduction.reducer_id, 80),
        reducer_version: sanitize_runtime_atom(&reduction.reducer_version, 80),
        status: sanitize_runtime_atom(&reduction.status, 80),
        reason: reduction
            .reason
            .as_deref()
            .map(|reason| sanitize_runtime_text(reason, 160)),
        fallback: reduction.fallback,
        host_latency_ms: reduction.host_latency_ms.min(60_000),
        created_at: reduction.created_at,
    }
}

fn sanitize_runtime_atom(value: &str, max_chars: usize) -> String {
    sanitize_runtime_text(value, max_chars)
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn sanitize_runtime_text(value: &str, max_chars: usize) -> String {
    value
        .replace("/Users/", "_Users_")
        .replace("\\Users\\", "_Users_")
        .replace("sk-", "sk_redacted_")
        .replace("storage_path", "storage_ref")
        .replace("credential", "cred_ref")
        .replace("password", "password_ref")
        .chars()
        .filter(|ch| ch.is_ascii_graphic() || ch.is_ascii_whitespace())
        .take(max_chars)
        .collect()
}

fn upsert_by_id<T, F>(items: &mut Vec<T>, item: T, matches: F)
where
    F: Fn(&T) -> bool,
{
    if let Some(existing) = items.iter_mut().find(|existing| matches(existing)) {
        *existing = item;
    } else {
        items.push(item);
    }
}
