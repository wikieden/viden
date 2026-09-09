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
    CommandAction, ConflictBounce, ConflictBounceStatus, ContextBudgetRecord, ContextBundleRecord,
    ContextOmittedSourceRecord, ContextScope, ContextSourceRecord, ContractDecision,
    ContractRecord, CoreHandshake, CostLedgerTotals, CostMeterability, CostUsageRecord,
    CredentialHandle, CredentialRequestId, CredentialStatus, DEFAULT_WORKSPACE_DIFF_BYTES,
    DataEgressPolicy, DecisionContext, DependencyRecord, DependencyState, DiffDocument, DiffFile,
    DiffHunk, DiffLine, DiffLineKind, EventCursor, EvidenceView, ExecutionTarget,
    FRONTEND_SCHEMA_V1, GapRecovery, GateStrength, HandoffAcceptance, HandoffRecord, LaneBudget,
    LaneConflictView, LaneRunStats, LaneRuntimeOwnerBinding, LaneStatus, LocaleId,
    MAX_WORKSPACE_DIFF_BYTES, MergeGatePolicySnapshot, MergeGateRecord, MergeGateStatus,
    MergeGateType, MergeGateValidator, MutationPolicy, PermissionLevel, PermissionMode,
    ProjectConfigPreview, ProjectConfigState, ProjectProbe, ProviderHealthView, QueuedInputView,
    RecentProjectSummary, RecentSessionSummary, RecentWorkQuery, ReplayBatch, ReplayRequest,
    ResolvedUiPreferences, RevertRecord, ReviewRequestRecord, ReviewRequestStatus, ReviewVerdict,
    ReviewedEvidenceBinding, RuntimeCommand, RuntimeCommandEnvelope, RuntimeErrorView,
    RuntimeEvent, RuntimeEventEnvelope, RuntimeEventKind, RuntimeOwner, RuntimeServiceHealthView,
    RuntimeServiceKind, RuntimeServiceStatus, RuntimeSnapshot, RuntimeSnapshotEnvelope,
    RuntimeViewState, RuntimeWireEvent, SchemaVersion, SourceTarget, StarterLanePreset,
    StarterLanePreview, StarterLanePreviewInvalidationReason, StarterLaneReceipt,
    StarterLaneRequest, TokenCostView, ToolCallView, TranscriptPage, TranscriptPageRequest,
    TranscriptRow, TranscriptRowId, TranscriptRowKind, TuiColorDepth, UiColorMode, UiDensity,
    UiMotion, UiPreferenceDiagnostic, UiPreferencePatch, UiPreferences, UiSkin, WorkMode,
    WorkspaceChangeKind, WorkspaceChangeView, WorkspaceDiffEntry, WorkspaceDiffPage,
    WorkspaceDiffQuery, WorkspaceDiffScope, WorkspaceEligibility, WorkspaceFileEntry,
    WorkspaceFileKind, WorkspaceFilePage, WorkspaceFilesQuery, WorkspaceSourceStatus,
    WorkspaceSourceView,
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
