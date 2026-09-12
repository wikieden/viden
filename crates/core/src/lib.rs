//! Viden core facade.
//!
//! This crate is the stable import boundary for runtime clients. It re-exports
//! the core runtime and contract types without introducing any TUI or GUI
//! dependency.

mod client;
mod compatibility;
mod host;
mod local_transport;

pub use client::{CoreClient, CoreClientError, CoreTransport, StatefulCoreClient};
pub use compatibility::{
    COCKPIT_CONTEXT_CAPABILITY, CORE_CLIENT_CAPABILITIES, CORE_CLIENT_VERSION,
    CORE_EXTENSION_CAPABILITIES, frontend_capabilities, local_core_handshake, validate_handshake,
    validate_schema_version,
};
pub use host::{
    BoundCoreClient, CoreHostError, LocalCoreHost, SecretBytes, WorkspaceBinding,
    WorkspaceOpenOverrides, WorkspaceOpenRequest,
};
pub use local_transport::LocalCoreTransport;
pub use viden_types::{
    AgentAdapterSource, AgentAdapterView, AgentAuthState, AgentAvailability, AgentContentPart,
    AgentConversationMessageView, AgentConversationRole, AgentDagRecord, AgentDagStatus,
    AgentDagTaskSpec, AgentLaneRecord, AgentRole, AgentRoute, AgentSessionInput,
    AgentSessionInputView, AgentSessionRequest, AgentSessionStatus, AgentSessionView,
    AgentStartability, AgentTaskKind, AgentTaskRecord, AgentTaskStatus, ApprovalDecision,
    ApprovalDefaultAction, ApprovalRequestView, ApprovalResponse, ApprovalRisk, ApprovalScope,
    ApprovalTarget, AuditActor, AuditActorFilter, AuditCursor, AuditId, AuditObjectRef,
    AuditOutcome, AuditPage, AuditQuery, AuditRecord, CapabilityId, CheckRunStatus, CheckRunView,
    CommandAction, ConflictBaseline, ConflictBounce, ConflictBounceStatus, ConflictContent,
    ConflictFile, ConflictHunk, ConflictHunkReason, ContextBudgetRecord, ContextBundleRecord,
    ContextOmittedSourceRecord, ContextScope, ContextSourceRecord, ContractDecision,
    ContractRecord, CoreHandshake, CostLedgerTotals, CostMeterability, CostUsageRecord,
    CredentialHandle, CredentialRequestId, CredentialStatus, DEFAULT_EVIDENCE_PAGE_SIZE,
    DEFAULT_TRANSCRIPT_ROWS_PAGE, DEFAULT_WORKSPACE_DIFF_BYTES, DEFAULT_WORKSPACE_FILE_BYTES,
    DataEgressPolicy, DecisionContext, DependencyRecord, DependencyState, DiffDocument, DiffFile,
    DiffHunk, DiffLine, DiffLineKind, EventCursor, EvidenceContent, EvidenceCursor, EvidencePage,
    EvidenceQualityStatus, EvidenceQuery, EvidenceUnavailableReason, EvidenceVerificationState,
    EvidenceView, ExecutionTarget, FRONTEND_SCHEMA_V1, GapRecovery, GateStrength,
    HandoffAcceptance, HandoffRecord, LaneBudget, LaneConflictView, LaneRunStats,
    LaneRuntimeOwnerBinding, LaneSidebarMode, LaneStatus, LocaleId, MAX_CONFLICT_CONTENT_BYTES,
    MAX_EVIDENCE_CONTENT_BYTES, MAX_EVIDENCE_PAGE_SIZE, MAX_EVIDENCE_QUERY_KINDS,
    MAX_HIDDEN_STATUSBAR_SEGMENT_BYTES, MAX_HIDDEN_STATUSBAR_SEGMENTS,
    MAX_OPERATOR_COMMIT_MESSAGE_BYTES, MAX_OPERATOR_GIT_OUTPUT_BYTES,
    MAX_TRANSCRIPT_ROW_TEXT_BYTES, MAX_TRANSCRIPT_ROWS_PAGE, MAX_TURN_FAILURE_REASON_CHARS,
    MAX_WORKSPACE_DIFF_BYTES, MAX_WORKSPACE_FILE_BYTES, MergeGatePolicySnapshot, MergeGateRecord,
    MergeGateStatus, MergeGateType, MergeGateValidator, MutationPolicy, OperatorGitAction,
    OperatorGitFailureClass, OperatorGitOutcome, OwnedTranscriptRow, PROJECT_ID_PREFIX,
    PermissionLevel, PermissionMode, ProjectConfigPreview, ProjectConfigState, ProjectIdOrigin,
    ProjectProbe, ProviderHealthView, QueuedInputView, RecentProjectSummary, RecentSessionSummary,
    RecentWorkQuery, ReplayBatch, ReplayRequest, ResolvedUiPreferences, RevertRecord,
    ReviewRequestRecord, ReviewRequestStatus, ReviewVerdict, ReviewedEvidenceBinding,
    RuntimeCommand, RuntimeCommandEnvelope, RuntimeErrorView, RuntimeEvent, RuntimeEventEnvelope,
    RuntimeEventKind, RuntimeOwner, RuntimeServiceHealthView, RuntimeServiceKind,
    RuntimeServiceStatus, RuntimeSnapshot, RuntimeSnapshotEnvelope, RuntimeViewState,
    RuntimeWireEvent, SchemaVersion, SourceTarget, StarterLanePreset, StarterLanePreview,
    StarterLanePreviewInvalidationReason, StarterLaneReceipt, StarterLaneRequest, TokenCostView,
    ToolCallView, TranscriptPage, TranscriptPageRequest, TranscriptRow, TranscriptRowContent,
    TranscriptRowId, TranscriptRowKind, TranscriptRowsPage, TranscriptRowsQuery, TuiColorDepth,
    TurnOutcome, TurnSource, TurnView, UiColorMode, UiDensity, UiLayoutPreferencePatch,
    UiLayoutPreferences, UiMotion, UiPreferenceDiagnostic, UiPreferencePatch, UiPreferences,
    UiSkin, WORKSPACE_ID_DIGEST_CHARS, WORKSPACE_ID_PREFIX, WorkMode, WorkspaceChangeKind,
    WorkspaceChangeView, WorkspaceDiffEntry, WorkspaceDiffPage, WorkspaceDiffQuery,
    WorkspaceDiffScope, WorkspaceEligibility, WorkspaceFileBody, WorkspaceFileContent,
    WorkspaceFileEntry, WorkspaceFileKind, WorkspaceFilePage, WorkspaceFileReadQuery,
    WorkspaceFileUnavailableReason, WorkspaceFilesQuery, WorkspaceRuntimeOwnerBinding,
    WorkspaceSourceStatus, WorkspaceSourceView, workspace_owner_authorizes,
};

/// Temporary compatibility imports for the pre-v3 TUI bootstrap.
///
/// Frontend clients must use [`CoreClient`] and must not import this module.
/// It can be removed after the legacy TUI has migrated to the frozen contract.
#[deprecated(note = "use CoreClient and protocol/view contracts from viden-core")]
pub mod legacy {
    pub use viden_provider::{
        ModelProvider, ModelRequestControl, ProviderAuthMode, ProviderDescriptor,
    };
    pub use viden_runtime::{EngineEvent, ProviderTelemetry, RuntimeSupervisor, SessionEngine};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facade_exports_runtime_contract_types() {
        assert!(std::any::type_name::<RuntimeEvent>().contains("RuntimeEvent"));
        assert!(std::any::type_name::<RuntimeCommand>().contains("RuntimeCommand"));
        assert!(std::any::type_name::<RuntimeViewState>().contains("RuntimeViewState"));
        assert!(
            std::any::type_name::<StatefulCoreClient<LocalCoreTransport>>()
                .contains("StatefulCoreClient")
        );
        assert!(std::any::type_name::<ApprovalRequestView>().contains("ApprovalRequestView"));
        assert!(std::any::type_name::<EvidenceView>().contains("EvidenceView"));
        assert!(std::any::type_name::<QueuedInputView>().contains("QueuedInputView"));
        assert!(std::any::type_name::<TurnView>().contains("TurnView"));
        assert!(std::any::type_name::<AgentLaneRecord>().contains("AgentLaneRecord"));
        assert!(
            std::any::type_name::<LaneRuntimeOwnerBinding>().contains("LaneRuntimeOwnerBinding")
        );
        assert!(std::any::type_name::<AgentTaskStatus>().contains("AgentTaskStatus"));
        assert!(std::any::type_name::<MergeGateStatus>().contains("MergeGateStatus"));
        assert!(std::any::type_name::<ResolvedUiPreferences>().contains("ResolvedUiPreferences"));
        assert!(std::any::type_name::<UiPreferencePatch>().contains("UiPreferencePatch"));
        assert!(std::any::type_name::<UiSkin>().contains("UiSkin"));
        assert!(std::any::type_name::<WorkMode>().contains("WorkMode"));
        assert!(std::any::type_name::<ProjectProbe>().contains("ProjectProbe"));
        assert!(std::any::type_name::<ProjectConfigPreview>().contains("ProjectConfigPreview"));
        assert!(std::any::type_name::<CredentialHandle>().contains("CredentialHandle"));
        assert!(std::any::type_name::<RecentWorkQuery>().contains("RecentWorkQuery"));
        assert!(std::any::type_name::<AuditQuery>().contains("AuditQuery"));
        assert!(std::any::type_name::<AuditPage>().contains("AuditPage"));
        assert!(std::any::type_name::<AuditRecord>().contains("AuditRecord"));
        assert!(std::any::type_name::<AuditObjectRef>().contains("AuditObjectRef"));
        // GUI-CORE-022: the typed workspace inventory. A frontend must be able
        // to render a file list from Core facts instead of walking the tree
        // itself, which is outside the client boundary and bypasses the
        // permission gate every other path read goes through.
        assert!(std::any::type_name::<WorkspaceFilesQuery>().contains("WorkspaceFilesQuery"));
        assert!(std::any::type_name::<WorkspaceFilePage>().contains("WorkspaceFilePage"));
        assert!(std::any::type_name::<WorkspaceFileEntry>().contains("WorkspaceFileEntry"));
        assert!(std::any::type_name::<WorkspaceFileKind>().contains("WorkspaceFileKind"));
        assert!(CORE_EXTENSION_CAPABILITIES.contains(&"runtime.workspace_files"));
        // C9: one file's bytes behind the inventory. `WorkspaceFileBody` is
        // what lets a client tell text from binary from absent instead of
        // rendering an empty editor over all three.
        assert!(std::any::type_name::<WorkspaceFileReadQuery>().contains("WorkspaceFileReadQuery"));
        assert!(std::any::type_name::<WorkspaceFileContent>().contains("WorkspaceFileContent"));
        assert!(std::any::type_name::<WorkspaceFileBody>().contains("WorkspaceFileBody"));
        assert!(
            std::any::type_name::<WorkspaceFileUnavailableReason>()
                .contains("WorkspaceFileUnavailableReason")
        );
        assert_eq!(DEFAULT_WORKSPACE_FILE_BYTES, 256 * 1024);
        assert_eq!(MAX_WORKSPACE_FILE_BYTES, 1024 * 1024);
        assert!(CORE_EXTENSION_CAPABILITIES.contains(&"runtime.workspace_file_reads"));
        // C8: the typed owner-scoped transcript rows beside the untouched
        // `runtime.transcript_page`. `TranscriptRowContent` is what lets a
        // client render ordered user/assistant/tool rows without pattern
        // matching on persisted entry shapes it has no contract for.
        assert!(std::any::type_name::<TranscriptRowsQuery>().contains("TranscriptRowsQuery"));
        assert!(std::any::type_name::<TranscriptRowsPage>().contains("TranscriptRowsPage"));
        assert!(std::any::type_name::<TranscriptRowContent>().contains("TranscriptRowContent"));
        assert!(std::any::type_name::<OwnedTranscriptRow>().contains("OwnedTranscriptRow"));
        assert_eq!(DEFAULT_TRANSCRIPT_ROWS_PAGE, 50);
        assert_eq!(MAX_TRANSCRIPT_ROWS_PAGE, 200);
        assert_eq!(MAX_TRANSCRIPT_ROW_TEXT_BYTES, 8 * 1024);
        assert!(CORE_EXTENSION_CAPABILITIES.contains(&"runtime.transcript_rows"));
        // GUI-CORE-012: the typed diff substrate. A frontend must render hunk
        // rows from Core facts instead of parsing unified diff text itself,
        // which would put a second parser and a second definition of "hunk"
        // outside the Core boundary.
        assert!(std::any::type_name::<DiffDocument>().contains("DiffDocument"));
        assert!(std::any::type_name::<DiffFile>().contains("DiffFile"));
        assert!(std::any::type_name::<DiffHunk>().contains("DiffHunk"));
        assert!(std::any::type_name::<DiffLine>().contains("DiffLine"));
        assert!(std::any::type_name::<DiffLineKind>().contains("DiffLineKind"));
        assert!(std::any::type_name::<DecisionContext>().contains("DecisionContext"));
        assert!(std::any::type_name::<SourceTarget>().contains("SourceTarget"));
        assert!(std::any::type_name::<WorkspaceDiffQuery>().contains("WorkspaceDiffQuery"));
        assert!(std::any::type_name::<WorkspaceDiffScope>().contains("WorkspaceDiffScope"));
        assert!(std::any::type_name::<WorkspaceDiffPage>().contains("WorkspaceDiffPage"));
        assert!(std::any::type_name::<WorkspaceDiffEntry>().contains("WorkspaceDiffEntry"));
        assert_eq!(DEFAULT_WORKSPACE_DIFF_BYTES, 256 * 1024);
        assert_eq!(MAX_WORKSPACE_DIFF_BYTES, 1024 * 1024);
        assert!(CORE_EXTENSION_CAPABILITIES.contains(&"runtime.structured_diff"));
        // GUI-CORE-020: the typed operator source-control action. A frontend
        // must send a typed action and read a typed outcome instead of driving
        // git itself or parsing output text, which would put a second git
        // implementation and a second failure taxonomy outside Core.
        assert!(std::any::type_name::<OperatorGitAction>().contains("OperatorGitAction"));
        assert!(std::any::type_name::<OperatorGitOutcome>().contains("OperatorGitOutcome"));
        assert!(
            std::any::type_name::<OperatorGitFailureClass>().contains("OperatorGitFailureClass")
        );
        assert_eq!(MAX_OPERATOR_GIT_OUTPUT_BYTES, 8 * 1024);
        assert_eq!(MAX_OPERATOR_COMMIT_MESSAGE_BYTES, 4 * 1024);
        assert!(CORE_EXTENSION_CAPABILITIES.contains(&"runtime.operator_git"));
        // GUI-CORE-025: the typed evidence archive read. A frontend must page
        // the archive and read canonical content through Core instead of
        // deriving either from `latest_evidence`, which is a recent-window
        // projection with no ordering, no cursor, and no content, or reading
        // the ContextStore itself, which is outside the client boundary and
        // bypasses the hash verification every canonical read goes through.
        assert!(std::any::type_name::<EvidenceQuery>().contains("EvidenceQuery"));
        assert!(std::any::type_name::<EvidencePage>().contains("EvidencePage"));
        assert!(std::any::type_name::<EvidenceCursor>().contains("EvidenceCursor"));
        assert!(std::any::type_name::<EvidenceContent>().contains("EvidenceContent"));
        assert!(
            std::any::type_name::<EvidenceUnavailableReason>()
                .contains("EvidenceUnavailableReason")
        );
        assert_eq!(DEFAULT_EVIDENCE_PAGE_SIZE, 50);
        assert_eq!(MAX_EVIDENCE_PAGE_SIZE, 200);
        assert_eq!(MAX_EVIDENCE_QUERY_KINDS, 32);
        assert_eq!(MAX_EVIDENCE_CONTENT_BYTES, 256 * 1024);
        assert!(CORE_EXTENSION_CAPABILITIES.contains(&"runtime.evidence_reads"));
        // GUI-CORE-028 and E1 defect 4: the archive those reads page is
        // actually written. The reads above shipped in 0.3.3 over an archive
        // that no applied mutation ever reached, so a client could page it
        // correctly and always find it empty. Gated separately because "this
        // Core archives applied work" and "this Core can answer an archive
        // read" are different facts a client has to be able to tell apart.
        assert!(CORE_EXTENSION_CAPABILITIES.contains(&"runtime.durable_work_evidence"));
        // The canonical reference's own verdicts. `EvidenceView.canonical`
        // carries Core's verification and quality state, so a client that
        // cannot name these two enums can render the reference and not what
        // Core concluded about it — and would have to invent a second
        // vocabulary for a verdict Core already owns.
        assert!(
            std::any::type_name::<EvidenceVerificationState>()
                .contains("EvidenceVerificationState")
        );
        assert!(std::any::type_name::<EvidenceQualityStatus>().contains("EvidenceQualityStatus"));
        assert_eq!(
            EvidenceVerificationState::Verified,
            EvidenceVerificationState::Verified
        );
        assert_eq!(EvidenceQualityStatus::Pass, EvidenceQualityStatus::Pass);
        // GUI-CORE-015: the typed conflict lines. `ConflictBounce` and
        // `LaneConflictView` were already exported, but the family they carry
        // was not, so a client could hold the record and not read the hunks
        // inside it without a second `viden-*` dependency or a private decoder
        // of Core's wire encoding. Both are outside the client boundary.
        assert!(std::any::type_name::<ConflictContent>().contains("ConflictContent"));
        assert!(std::any::type_name::<ConflictBaseline>().contains("ConflictBaseline"));
        assert!(std::any::type_name::<ConflictFile>().contains("ConflictFile"));
        assert!(std::any::type_name::<ConflictHunk>().contains("ConflictHunk"));
        assert!(std::any::type_name::<ConflictHunkReason>().contains("ConflictHunkReason"));
        assert_eq!(MAX_CONFLICT_CONTENT_BYTES, 256 * 1024);
        assert!(CORE_EXTENSION_CAPABILITIES.contains(&"runtime.conflict_content"));
        // GUI-CORE-008: the typed context budget and its scope. A frontend must
        // be able to prove that a budget belongs to the selected Lane's task
        // instead of reconstructing a private serialization of the scope shape.
        assert!(std::any::type_name::<ContextBudgetRecord>().contains("ContextBudgetRecord"));
        assert!(std::any::type_name::<ContextScope>().contains("ContextScope"));
        assert_eq!(
            ContextScope::Task("task_facade".to_string()),
            ContextScope::Task("task_facade".to_string())
        );
        assert!(matches!(
            ContextBudgetRecord {
                budget_id: "ctxbudget-facade".to_string(),
                scope: ContextScope::Task("task_facade".to_string()),
                soft_token_limit: 2,
                hard_token_limit: 4,
                used_tokens: 3,
                remaining_tokens: 1,
                exceeded: false,
                updated_at: None,
            }
            .scope,
            ContextScope::Task(_)
        ));
        // Supervision and lane-cost vocabularies a frontend needs to build a
        // typed command or read a typed fact without reaching past this facade
        // into `viden-types`.
        assert!(std::any::type_name::<ReviewVerdict>().contains("ReviewVerdict"));
        assert!(std::any::type_name::<CostMeterability>().contains("CostMeterability"));
        assert!(std::any::type_name::<LaneRunStats>().contains("LaneRunStats"));
        assert!(std::any::type_name::<RecentProjectSummary>().contains("RecentProjectSummary"));
        assert!(std::any::type_name::<RecentSessionSummary>().contains("RecentSessionSummary"));
        assert!(std::any::type_name::<AgentAdapterView>().contains("AgentAdapterView"));
        assert!(std::any::type_name::<AgentSessionRequest>().contains("AgentSessionRequest"));
        assert!(std::any::type_name::<AgentSessionInput>().contains("AgentSessionInput"));
        assert!(std::any::type_name::<AgentSessionInputView>().contains("AgentSessionInputView"));
        assert!(std::any::type_name::<AgentSessionView>().contains("AgentSessionView"));
        assert!(std::any::type_name::<AgentStartability>().contains("AgentStartability"));
        assert!(std::any::type_name::<WorkspaceEligibility>().contains("WorkspaceEligibility"));
        let capabilities = frontend_capabilities();
        for capability in [
            "runtime.agent_adapters",
            "runtime.agent_permission_bridge",
            "runtime.agent_session_input",
            "runtime.agent_sessions",
            "runtime.workspace_eligibility",
        ] {
            assert!(
                capabilities.contains(&CapabilityId(capability.to_string())),
                "missing negotiated capability {capability}"
            );
        }
    }

    #[test]
    fn facade_exports_starter_lane_frontend_types() {
        let capability = CapabilityId("runtime.lane_lifecycle".to_string());
        let owner = RuntimeOwner {
            workspace_id: "workspace-test".to_string(),
            project_id: "project-test".to_string(),
            ..RuntimeOwner::default()
        };
        let handshake = CoreHandshake {
            core_version: "test-core".to_string(),
            supported_schema_versions: vec![FRONTEND_SCHEMA_V1],
            active_schema_version: FRONTEND_SCHEMA_V1,
            capabilities: std::collections::BTreeSet::from([capability]),
        };
        let request = StarterLaneRequest {
            lane_id: "lane-coder".to_string(),
            preset: StarterLanePreset::Coder,
            branch: Some("codex/lane-coder".to_string()),
            worktree_path: Some(".worktrees/lane-coder".to_string()),
        };
        let command = RuntimeCommand::PreviewStarterLane {
            request: request.clone(),
        };

        assert_eq!(handshake.active_schema_version, FRONTEND_SCHEMA_V1);
        assert_eq!(owner.workspace_id, "workspace-test");
        assert_eq!(command, RuntimeCommand::PreviewStarterLane { request });
        assert!(std::any::type_name::<StarterLanePreview>().contains("StarterLanePreview"));
        assert!(std::any::type_name::<StarterLaneReceipt>().contains("StarterLaneReceipt"));
        assert!(
            std::any::type_name::<StarterLanePreviewInvalidationReason>()
                .contains("StarterLanePreviewInvalidationReason")
        );
    }
}
