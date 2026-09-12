use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use viden_core::{
    CORE_CLIENT_CAPABILITIES, CORE_CLIENT_VERSION, CORE_EXTENSION_CAPABILITIES, CheckRunStatus,
    CheckRunView, RuntimeServiceHealthView, WorkspaceChangeView, WorkspaceSourceView,
    frontend_capabilities, local_core_handshake,
};
use viden_types::{
    AgentAdapterSource, AgentAdapterView, AgentAuthState, AgentAvailability, AgentContentPart,
    AgentConversationRole, AgentDagRecord, AgentDagStatus, AgentDagTaskSpec, AgentLaneRecord,
    AgentNextAction, AgentRole, AgentRoute, AgentSessionStatus, AgentSessionView,
    AgentStartability, AgentTaskKind, AgentTaskRecord, AgentTaskStatus, ApprovalDecision,
    ApprovalDefaultAction, ApprovalRequestView, ApprovalResponse, ApprovalRisk, ApprovalScope,
    ApprovalTarget, AuditActor, AuditActorFilter, AuditObjectRef, AuditOutcome, AuditPage,
    AuditQuery, AuditRecord, CanonicalEvidenceReference, CapabilityId, ConflictBaseline,
    ConflictBounce, ConflictBounceStatus, ConflictContent, ConflictFile, ConflictHunk,
    ConflictHunkReason, ContextBudgetRecord, ContextBundleRecord, ContextOmittedSourceRecord,
    ContextScope, ContextSourceRecord, CostScope, CostUsageOutcome, CostUsageRecord,
    DecisionContext, DiffDocument, DiffFile, DiffHunk, DiffLine, DiffLineKind, EventCursor,
    EvidenceContent, EvidenceCursor, EvidencePage, EvidenceProducer, EvidenceQualityFacts,
    EvidenceQualityStatus, EvidenceQuery, EvidenceUnavailableReason, EvidenceVerificationState,
    EvidenceView, ExecutionTarget, FRONTEND_SCHEMA_V1, GateStrength, LaneBudget,
    LaneRuntimeOwnerBinding, LaneSidebarMode, LaneStatus, MAX_HIDDEN_STATUSBAR_SEGMENTS,
    MergeGateDecision, MergeGateDecisionOutcome, MergeGatePolicySnapshot, MergeGateRecord,
    MergeGateStatus, MergeGateType, MergeGateValidator, MutationPolicy, OperatorGitAction,
    OperatorGitFailureClass, OperatorGitOutcome, PermissionLevel, PermissionMode,
    ProjectConfigState, ProjectIdOrigin, ProjectProbe, QueuedInputView, RecentProjectSummary,
    RecentSessionSummary, ResolvedUiPreferences, ReviewRequestStatus, RuntimeCommand,
    RuntimeErrorView, RuntimeEvent, RuntimeEventEnvelope, RuntimeEventKind, RuntimeOwner,
    RuntimeSnapshot, RuntimeViewState, RuntimeWireEvent, SchemaVersion, SourceTarget,
    StarterLanePreview, StarterLanePreviewInvalidationReason, StarterLaneReceipt, TokenCostView,
    TokenUsage, TurnOutcome, TurnSource, TurnView, UiColorMode, UiDensity, UiLayoutPreferencePatch,
    UiLayoutPreferences, UiMotion, UiPreferenceDiagnostic, UiPreferences, UiSkin, WorkMode,
    WorkspaceChangeKind, WorkspaceDiffEntry, WorkspaceDiffPage, WorkspaceDiffQuery,
    WorkspaceDiffScope, WorkspaceEligibility, WorkspaceFileBody, WorkspaceFileContent,
    WorkspaceFileEntry, WorkspaceFileKind, WorkspaceFilePage, WorkspaceFileReadQuery,
    WorkspaceFileUnavailableReason, WorkspaceFilesQuery, WorkspaceRuntimeOwnerBinding,
};
use viden_types::{
    MAX_TRANSCRIPT_ROW_TEXT_BYTES, OwnedTranscriptRow, ToolCallView, TranscriptRowContent,
    TranscriptRowsPage, TranscriptRowsQuery,
};

const FIXTURE_DIR: &str = "tests/fixtures/frontend-contract-v1";
const REQUIRED_FIXTURES: [&str; 9] = [
    "stream-tool.json",
    "approval-allow-deny.json",
    "queued-follow-up.json",
    "dag-blocker.json",
    "multi-lane.json",
    "merge-gate.json",
    "context-pressure-cost-blind.json",
    "plan-denial.json",
    "d1-vertical-slice.json",
];

#[derive(Debug, Deserialize)]
struct FrontendContractFixture {
    fixture_id: String,
    schema_version: SchemaVersion,
    required_capabilities: Vec<CapabilityId>,
    initial_snapshot: RuntimeSnapshot,
    events: Vec<RuntimeEventEnvelope>,
    expected_final_cursor: EventCursor,
    expected_view_sha256: String,
}

#[derive(Debug, Serialize)]
struct FrontendContractFixtureOut {
    fixture_id: String,
    schema_version: SchemaVersion,
    required_capabilities: Vec<CapabilityId>,
    initial_snapshot: RuntimeSnapshot,
    events: Vec<RuntimeEventEnvelope>,
    expected_final_cursor: EventCursor,
    expected_view_sha256: String,
}

#[test]
fn frontend_contract_v1_corpus_replays_deterministically() {
    let root = fixture_root();
    let missing = REQUIRED_FIXTURES
        .iter()
        .filter(|name| !root.join(name).exists())
        .copied()
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "missing frontend-contract-v1 fixtures: {}",
        missing.join(", ")
    );

    for name in REQUIRED_FIXTURES {
        let fixture = read_fixture(&root, name);
        assert_fixture_identity(name, &fixture);
        assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
        assert_cursors_are_contiguous(name, &fixture);
        let (first_view, first_cursor, first_digest) = replay_fixture(&fixture);
        let (second_view, second_cursor, second_digest) = replay_fixture(&fixture);
        assert_eq!(first_cursor, fixture.expected_final_cursor, "{name}");
        assert_eq!(first_cursor, second_cursor, "{name}");
        assert_eq!(first_view, second_view, "{name}");
        assert_eq!(first_digest, second_digest, "{name}");
        assert_eq!(first_digest, fixture.expected_view_sha256, "{name}");
        assert_scenario_facts(name, &first_view);
    }
}

#[test]
fn frontend_contract_v1_capability_source_is_frozen_and_sorted() {
    let expected = [
        "runtime.agent_dag",
        "runtime.approvals",
        "runtime.commands",
        "runtime.context",
        "runtime.cost",
        "runtime.events",
        "runtime.evidence",
        "runtime.merge_gate",
        "runtime.queued_input",
        "runtime.replay",
        "runtime.snapshot",
        "runtime.transcript_page",
        "runtime.typed_lanes",
        "runtime.typed_tasks",
        "ui.preferences",
    ];
    assert_eq!(CORE_CLIENT_CAPABILITIES, expected);
    assert!(
        CORE_CLIENT_CAPABILITIES
            .windows(2)
            .all(|pair| pair[0] < pair[1])
    );
    let advertised = frontend_capabilities();
    assert!(
        CORE_EXTENSION_CAPABILITIES
            .windows(2)
            .all(|pair| pair[0] < pair[1])
    );
    assert_eq!(
        advertised.len(),
        expected.len() + CORE_EXTENSION_CAPABILITIES.len()
    );
    for capability in expected {
        assert!(advertised.contains(&CapabilityId(capability.to_string())));
    }
    assert!(advertised.contains(&CapabilityId("runtime.lane_lifecycle".to_string())));
    assert!(advertised.contains(&CapabilityId("runtime.project_onboarding".to_string())));
    assert!(advertised.contains(&CapabilityId("runtime.credential_handles".to_string())));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(extension_manifest.contains("base_component_version = \"0.3.0\""));
    assert!(extension_manifest.contains("candidate_component_version = \"0.3.6\""));
    assert!(extension_manifest.contains("compatibility = \"additive_capability_gated\""));
    assert!(extension_manifest.contains("[runtime_trust_loop]\ncommand_count = 8"));
    assert_eq!(CORE_CLIENT_VERSION, "0.3.6");
    assert_eq!(local_core_handshake().core_version, "0.3.6");
}

#[test]
fn frontend_host_capabilities_are_schema_one_core_0_3_6_and_additive() {
    let frozen_base = [
        "runtime.agent_dag",
        "runtime.approvals",
        "runtime.commands",
        "runtime.context",
        "runtime.cost",
        "runtime.events",
        "runtime.evidence",
        "runtime.merge_gate",
        "runtime.queued_input",
        "runtime.replay",
        "runtime.snapshot",
        "runtime.transcript_page",
        "runtime.typed_lanes",
        "runtime.typed_tasks",
        "ui.preferences",
    ];
    let extensions = [
        "core.workspace_host",
        "runtime.agent_adapters",
        "runtime.agent_conversation",
        "runtime.agent_permission_bridge",
        "runtime.agent_session_input",
        "runtime.agent_sessions",
        "runtime.audit",
        "runtime.cockpit_context_v1",
        // GUI-CORE-015. Additive like the rows around it; the frozen base list
        // is untouched, which is what keeps the nine base fixtures
        // byte-identical.
        "runtime.conflict_content",
        "runtime.credential_handles",
        "runtime.credential_staging",
        // GUI-CORE-028 and E1 defect 4, C7. An applied native or agent mutation
        // is archived as a `patch` row with canonical bytes, supervised turn
        // facts reach the durable projection, and an approval decision is an
        // audit row. Additive like the rows around it; the frozen base list is
        // untouched, which is what keeps the nine base fixtures byte-identical.
        "runtime.durable_work_evidence",
        // GUI-CORE-025, the fourth and last capability of the 0.3.3 contract
        // increment. Additive like the rows around it; the frozen base list is
        // untouched, which is what keeps the nine base fixtures byte-identical.
        "runtime.evidence_reads",
        "runtime.lane_lifecycle",
        "runtime.lane_owner_projection",
        // GUI-CORE-020. Additive like the rows below it; the frozen base list
        // is untouched, which is what keeps the nine base fixtures
        // byte-identical.
        "runtime.operator_git",
        "runtime.project_onboarding",
        "runtime.recent_work",
        "runtime.starter_lane_preview",
        // GUI-CORE-012. Additive like the row below it: the frozen base list
        // is untouched, which is what keeps the nine base fixtures
        // byte-identical.
        "runtime.structured_diff",
        // GUI-CORE-009, C8, and the last capability of the 0.3.4 contract
        // increment: 29 extensions is the milestone's final count. The typed
        // owner-scoped sibling of the frozen `runtime.transcript_page` above,
        // which stays exactly as it was — which is what keeps the nine base
        // fixtures byte-identical.
        "runtime.transcript_rows",
        "runtime.trust_loop",
        // The turn bracket, C6. Compatibility follow-ups 3 and 5: a native
        // turn had no terminal fact and the session queue was never drained,
        // so both clients guessed liveness from display residue.
        "runtime.turn_lifecycle",
        "runtime.workspace_eligibility",
        // One file's bytes behind the inventory, C9. The Files and Code tabs
        // need content, not just a path list, and a client must not read the
        // operator's tree itself.
        "runtime.workspace_file_reads",
        // GUI-CORE-022. The frozen base list above is unchanged, which is what
        // keeps the nine base fixtures byte-identical.
        "runtime.workspace_files",
        // GUI-CORE-027, the first capability of the 0.3.4 contract increment
        // (C5). The workspace-scoped operator identity a commit made with no
        // Lane selected is audited under.
        "runtime.workspace_owner",
        // The cockpit layout record, C5's second capability. It is a separate
        // record rather than a `UiPreferences` field precisely because
        // `ResolvedUiPreferences` rides every `RuntimeSnapshot`: a field there
        // would move all nine frozen base digests, and this one moves none.
        "ui.layout_preferences",
        "ui.preference_persistence",
    ];

    assert_eq!(FRONTEND_SCHEMA_V1, SchemaVersion(1));
    assert_eq!(CORE_CLIENT_VERSION, "0.3.6");
    assert_eq!(CORE_CLIENT_CAPABILITIES, frozen_base);
    assert_eq!(CORE_EXTENSION_CAPABILITIES, extensions);
    assert!(
        CORE_EXTENSION_CAPABILITIES
            .windows(2)
            .all(|pair| pair[0] < pair[1]),
        "extension capabilities must be sorted and unique"
    );
    let advertised = frontend_capabilities();
    assert_eq!(advertised.len(), frozen_base.len() + extensions.len());
    assert_eq!(
        local_core_handshake().active_schema_version,
        SchemaVersion(1)
    );

    let base_only = viden_types::CoreHandshake {
        core_version: "0.3.0".to_string(),
        supported_schema_versions: vec![FRONTEND_SCHEMA_V1],
        active_schema_version: FRONTEND_SCHEMA_V1,
        capabilities: frozen_base
            .into_iter()
            .map(|capability| CapabilityId(capability.to_string()))
            .collect(),
    };
    viden_core::validate_handshake(&base_only)
        .expect("missing optional extensions must not block a frozen-base client");

    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(extension_manifest.contains("candidate_component_version = \"0.3.6\""));
    assert!(extension_manifest.contains("schema_version = 1"));
    assert!(extension_manifest.contains("runtime.cockpit_context_v1"));
    assert!(!extension_manifest.contains("runtime.workspace_facts"));
    assert!(extension_manifest.contains("runtime.lane_owner_projection"));
    assert!(extension_manifest.contains(
        "extension_fixture_sha256 = \"96dd5fde9f1241eb50f9d8978cf478d0ac5d3327448dc6ccde9d0e5018ce1580\""
    ));
    assert!(extension_manifest.contains(
        "interaction_fixture_sha256 = \"a6f1c436a15f7c77a5410c3563d8c3f67c5a5a3864692de61db61623f93ed891\""
    ));
}

#[test]
fn frontend_contract_v1_exports_cockpit_fact_types() {
    assert!(std::any::type_name::<WorkspaceSourceView>().contains("WorkspaceSourceView"));
    assert!(std::any::type_name::<RuntimeServiceHealthView>().contains("RuntimeServiceHealthView"));
    assert!(std::any::type_name::<WorkspaceChangeView>().contains("WorkspaceChangeView"));
    assert!(std::any::type_name::<CheckRunView>().contains("CheckRunView"));
}

#[test]
fn frontend_host_capabilities_fixture_replays_known_facts_and_tolerates_future_events() {
    let name = "frontend-host-services.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read extension fixture bytes");
    assert_eq!(
        format!("{:x}", Sha256::digest(&fixture_bytes)),
        "96dd5fde9f1241eb50f9d8978cf478d0ac5d3327448dc6ccde9d0e5018ce1580"
    );
    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    let known_types = fixture
        .events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(event) => Some(match &event.kind {
                RuntimeEventKind::UiPreferencesUpdated { .. } => "ui_preferences_updated",
                RuntimeEventKind::RecentWorkLoaded { .. } => "recent_work_loaded",
                RuntimeEventKind::StarterLanePreviewed { .. } => "starter_lane_previewed",
                RuntimeEventKind::StarterLaneCreated { .. } => "starter_lane_created",
                RuntimeEventKind::StarterLanePreviewInvalidated { .. } => {
                    "starter_lane_preview_invalidated"
                }
                RuntimeEventKind::LaneRuntimeOwnerBound { .. } => "lane_runtime_owner_bound",
                other => panic!("extension fixture contains transient placeholder {other:?}"),
            }),
            RuntimeWireEvent::Unknown { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        known_types,
        [
            "ui_preferences_updated",
            "recent_work_loaded",
            "starter_lane_previewed",
            "starter_lane_created",
            "starter_lane_preview_invalidated",
            "lane_runtime_owner_bound",
        ]
    );
    assert!(fixture.events.iter().any(|envelope| matches!(
        envelope.event,
        RuntimeWireEvent::Unknown { ref event_type, .. }
            if event_type == "future_frontend_host_fact"
    )));

    let (first_view, first_cursor, first_digest) = replay_fixture(&fixture);
    let (second_view, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(first_view, second_view);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);
    assert_eq!(
        first_view.ui_preferences.locale,
        viden_types::LocaleId::ZhCn
    );
    assert_eq!(first_view.recent_projects.len(), 1);
    assert_eq!(first_view.recent_sessions.len(), 1);
    assert!(first_view.starter_lane_previews.is_empty());
    assert_eq!(first_view.starter_lane_receipts.len(), 1);
    assert_eq!(first_view.lane_runtime_owners.len(), 1);
}

#[test]
fn interaction_closed_loop_fixture_replays_identically_after_a_gap() {
    let name = "interaction-closed-loop.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read interaction fixture bytes");
    assert_eq!(
        format!("{:x}", Sha256::digest(&fixture_bytes)),
        "a6f1c436a15f7c77a5410c3563d8c3f67c5a5a3864692de61db61623f93ed891"
    );
    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);
    let event_types = fixture
        .events
        .iter()
        .map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(event) => match &event.kind {
                RuntimeEventKind::ProjectProbed { .. } => "project_open_no_lane",
                RuntimeEventKind::WorkspaceEligibilityUpdated { .. } => "workspace_eligible",
                RuntimeEventKind::StarterLanePreviewed { .. } => "starter_lane_previewed",
                RuntimeEventKind::StarterLaneCreated { .. } => "starter_lane_created",
                RuntimeEventKind::AgentAdaptersLoaded { .. } => "agent_adapters_loaded",
                RuntimeEventKind::AgentSessionStarted { .. } => "agent_session_started",
                RuntimeEventKind::AgentSessionCompleted { .. } => "agent_session_completed",
                RuntimeEventKind::AgentSessionInputAccepted { .. } => {
                    "agent_session_input_accepted"
                }
                RuntimeEventKind::ToolCallStarted { .. } => "tool_call_started",
                RuntimeEventKind::ToolCallFinished { .. } => "tool_call_finished",
                RuntimeEventKind::ApprovalRequested { .. } => "approval_requested",
                RuntimeEventKind::AgentSessionUpdated { .. } => "agent_session_updated",
                RuntimeEventKind::ApprovalResolved { .. } => "approval_resolved",
                RuntimeEventKind::EvidenceRecorded { .. } => "evidence_recorded",
                RuntimeEventKind::MergeGateUpdated { .. } => "merge_gate_updated",
                RuntimeEventKind::LaneConflictDetected { .. } => "apply_conflict",
                RuntimeEventKind::LaneRecoveryRequired { .. } => "recovery_required",
                ref other => panic!("unexpected interaction event {other:?}"),
            },
            RuntimeWireEvent::Unknown { event_type, .. } => event_type,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        event_types,
        [
            "project_open_no_lane",
            "workspace_eligible",
            "starter_lane_previewed",
            "starter_lane_created",
            "agent_adapters_loaded",
            "agent_session_started",
            "agent_session_completed",
            "agent_session_started",
            "tool_call_started",
            "tool_call_finished",
            "approval_requested",
            "agent_session_updated",
            "approval_resolved",
            "agent_session_updated",
            "evidence_recorded",
            "merge_gate_updated",
            "apply_conflict",
            "recovery_required",
            "agent_session_completed",
            "agent_session_input_accepted",
            "agent_session_started",
            "agent_session_completed",
        ]
    );

    let (full_view, full_cursor, full_digest) = replay_fixture(&fixture);
    let (reconnected_view, reconnected_cursor, reconnected_digest) =
        replay_fixture_after_gap(&fixture, 16);
    assert_eq!(full_view, reconnected_view);
    assert_eq!(full_cursor, reconnected_cursor);
    assert_eq!(full_digest, reconnected_digest);
    assert_eq!(full_digest, fixture.expected_view_sha256);

    assert_eq!(
        full_view.project_probe.as_ref().unwrap().config_state,
        ProjectConfigState::Missing
    );
    assert_eq!(full_view.starter_lane_receipts.len(), 1);
    assert!(full_view.starter_lane_previews.is_empty());
    assert!(
        full_view
            .workspace_eligibility
            .as_ref()
            .is_some_and(|eligibility| eligibility.can_create_lane)
    );
    assert_eq!(full_view.agent_adapters.len(), 4);
    assert!(full_view.agent_adapters.iter().any(|adapter| {
        adapter.route == AgentRoute::BuiltIn && adapter.availability == AgentAvailability::Available
    }));
    assert!(
        full_view
            .agent_adapters
            .iter()
            .any(|adapter| { adapter.route == AgentRoute::Acp && adapter.agent_id == "codex-acp" })
    );
    assert_eq!(full_view.agent_sessions.len(), 1);
    assert_eq!(full_view.agent_session_inputs.len(), 1);
    assert_eq!(
        full_view.agent_session_inputs[0].input_id,
        "agent-input-loop-follow-up"
    );
    assert!(
        full_view
            .agent_sessions
            .iter()
            .all(|session| session.status == AgentSessionStatus::Completed)
    );
    assert!(full_view.pending_approvals.is_empty());
    assert!(
        full_view
            .latest_evidence
            .iter()
            .any(|item| item.id == "evidence-loop-test")
    );
    assert!(full_view.merge_gates.iter().any(|gate| {
        gate.gate_id == "gate-loop-apply" && gate.status == MergeGateStatus::Accepted
    }));
    assert_eq!(full_view.lane_conflicts.len(), 1);
    assert_eq!(full_view.lane_recoveries.len(), 1);

    let release_manifest = include_str!("../release-manifest.toml");
    assert!(release_manifest.contains("component_version = \"0.3.6\""));
    assert!(release_manifest.contains("runtime.cockpit_context_v1"));
    assert!(!release_manifest.contains("runtime.workspace_facts"));
    assert!(release_manifest.contains(
        "contract_implementation_checkpoint = \"1cec82185bbe860d6b8536a63741bc01f1edf2f6\""
    ));

    // The manifest's fixture digests are pinned by *recomputation*, not by a
    // literal. A literal pin only has to be edited by hand once — or missed
    // once, which is what happened between `0.3.5` and this checkpoint, where
    // the interaction fixture was edited twice after its digest was recorded
    // and no gate noticed — for the pin to stop describing the shipped bytes.
    // Deriving both halves here means a moved fixture fails the checkpoint.
    assert!(
        release_manifest.contains(&format!(
            "payload_sha256 = \"{:x}\"",
            Sha256::digest(&fixture_bytes)
        )),
        "the release manifest must pin the interaction fixture's exact bytes"
    );
    assert!(
        release_manifest.contains(&format!("view_sha256 = \"{full_digest}\"")),
        "the release manifest must pin the interaction fixture's replayed view digest"
    );

    // The 0.3.3 contract increment's four extension fixtures are recorded in
    // the same checkpoint that advertises their capabilities, and by the same
    // derivation.
    let root = fixture_root();
    for name in [
        "structured-diff.json",
        "operator-git.json",
        "conflict-content.json",
        "evidence-reads.json",
    ] {
        let bytes = fs::read(root.join(name)).expect("read 0.3.3 extension fixture bytes");
        let payload = format!("{:x}", Sha256::digest(&bytes));
        let fixture = read_fixture(&root, name);
        let (_, cursor, digest) = replay_fixture(&fixture);
        assert!(
            release_manifest.contains(&format!("payload_sha256 = \"{payload}\"")),
            "{name} payload digest is not pinned in the Core release manifest"
        );
        assert!(
            release_manifest.contains(&format!("view_sha256 = \"{digest}\"")),
            "{name} view digest is not pinned in the Core release manifest"
        );
        assert!(
            release_manifest.contains(&format!("final_cursor = {}", cursor.sequence)),
            "{name} final cursor is not pinned in the Core release manifest"
        );
    }
}

/// GUI-CORE-011: canonical proof that a review verdict is readable as a
/// `ReviewRequestStatus` transition in the contract stream.
///
/// Registered as a post-freeze schema-1 extension fixture, so the frozen base
/// corpus of nine `0.3.0` fixtures keeps its byte and digest identity.
#[test]
fn review_decision_fixture_replays_the_review_verdict_transition() {
    let name = "review-decision.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read review decision fixture bytes");
    let fixture_sha256 = format!("{:x}", Sha256::digest(&fixture_bytes));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(
        extension_manifest.contains(&format!(
            "review_decision_fixture_sha256 = \"{fixture_sha256}\""
        )),
        "the extension manifest must register the exact review decision fixture bytes"
    );
    assert!(extension_manifest.contains("review_decision_fixture = \"review-decision.json\""));

    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    // The stream is exactly what a request followed by a verdict publishes.
    let statuses = fixture
        .events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(event) => match &event.kind {
                RuntimeEventKind::ReviewRequestUpdated { review } => Some(review.status),
                _ => None,
            },
            RuntimeWireEvent::Unknown { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        statuses,
        [ReviewRequestStatus::Pending, ReviewRequestStatus::Accepted],
        "the fixture must carry the Pending -> Accepted transition itself"
    );

    let (first_view, first_cursor, first_digest) = replay_fixture(&fixture);
    let (second_view, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(first_view, second_view);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);

    let review = first_view
        .review_requests
        .iter()
        .find(|review| review.review_id == "review_decision")
        .expect("the fixture must project its review request");
    assert_eq!(review.status, ReviewRequestStatus::Accepted);
    assert_eq!(
        review.feedback.as_deref(),
        Some("review.feedback.evidence_matches_request")
    );
    assert_eq!(review.audit_id, "audit_review_decided");

    let gate = first_view
        .merge_gates
        .iter()
        .find(|gate| gate.gate_id == "gate_review_decision")
        .expect("the fixture must project its merge gate");
    let validator = gate.validator.as_ref().expect("independent validator");
    assert_eq!(validator.review_request_id, review.review_id);
    assert_eq!(
        validator.validated_at,
        Some(1_700_000_102),
        "an accepted verdict stamps the validator it settled"
    );
    // The verdict settles the review only: deciding the gate stays a separate
    // permission-gated operator command.
    assert_eq!(gate.status, MergeGateStatus::CollectingEvidence);
}

#[test]
#[ignore = "manual review decision fixture refresh; normal tests validate committed JSON only"]
fn refresh_review_decision_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = review_decision_fixture();
    fs::write(
        root.join("review-decision.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

/// GUI-CORE-008: canonical proof that a published context budget is readable as
/// the selected Lane's own task scope.
///
/// Two Lanes run concurrently with distinct task-scoped budgets. A client that
/// took "the most recent budget", or matched a budget to a Lane by anything but
/// the typed scope, would attribute the wrong numbers to a Lane. That is the
/// guess the request refuses, so the fixture makes both budgets resolvable and
/// mutually exclusive.
///
/// Registered as a post-freeze schema-1 extension fixture, so the frozen base
/// corpus of nine `0.3.0` fixtures keeps its byte and digest identity.
#[test]
fn context_budgets_fixture_scopes_each_lane_budget_to_its_own_task() {
    let name = "context-budgets.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read context budget fixture bytes");
    let fixture_sha256 = format!("{:x}", Sha256::digest(&fixture_bytes));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(
        extension_manifest.contains(&format!(
            "context_budgets_fixture_sha256 = \"{fixture_sha256}\""
        )),
        "the extension manifest must register the exact context budget fixture bytes"
    );
    assert!(extension_manifest.contains("context_budgets_fixture = \"context-budgets.json\""));

    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    let (first_view, first_cursor, first_digest) = replay_fixture(&fixture);
    let (second_view, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(first_view, second_view);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);

    // Both Lanes are live at once, so "the latest budget" is never a safe pick.
    assert_eq!(first_view.context_budgets.len(), 2);
    assert_eq!(first_view.lane_runtime_owners.len(), 2);

    for (lane_id, task_id, budget_id, used, hard, exceeded) in [
        (
            "lane_context_alpha",
            "task_context_alpha",
            "ctxbudget-bundle_context_alpha",
            52_000_u64,
            80_000_u64,
            false,
        ),
        (
            "lane_context_beta",
            "task_context_beta",
            "ctxbudget-bundle_context_beta",
            41_000_u64,
            40_000_u64,
            true,
        ),
    ] {
        // The exact Core-bound owner is what names the Lane's task; the client
        // never derives a task id from display text or Lane ordering.
        let owner = first_view
            .lane_runtime_owners
            .iter()
            .find(|binding| binding.lane_id == lane_id)
            .map(|binding| &binding.owner)
            .expect("each fixture Lane publishes exactly one live owner binding");
        assert_eq!(owner.task_id.as_deref(), Some(task_id));

        let scope = ContextScope::Task(task_id.to_string());
        let mut scoped = first_view
            .context_budgets
            .iter()
            .filter(|budget| budget.scope == scope);
        let budget = scoped
            .next()
            .unwrap_or_else(|| panic!("{lane_id} task scope must resolve exactly one budget"));
        assert!(
            scoped.next().is_none(),
            "{lane_id} task scope must not resolve a second budget"
        );
        assert_eq!(budget.budget_id, budget_id);
        assert_eq!(budget.used_tokens, used);
        assert_eq!(budget.hard_token_limit, hard);
        assert_eq!(budget.exceeded, exceeded);
    }

    // The two scopes are disjoint, so neither Lane can resolve the other's
    // budget even though both were published on the same stream.
    let scopes = first_view
        .context_budgets
        .iter()
        .map(|budget| budget.scope.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        scopes,
        vec![
            ContextScope::Task("task_context_alpha".to_string()),
            ContextScope::Task("task_context_beta".to_string()),
        ]
    );
}

#[test]
#[ignore = "manual context budget fixture refresh; normal tests validate committed JSON only"]
fn refresh_context_budgets_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = context_budgets_fixture();
    fs::write(
        root.join("context-budgets.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

/// GUI-CORE-016: canonical proof that replaying ordered `AssistantDelta` chunks
/// reconstructs exactly the final Agent message.
///
/// The terminal marker for a streamed reply is the session-completion fact that
/// carries the same finished text. Reducing it must settle the turn without
/// appending a second copy of the reply, otherwise a client would render the
/// paragraph twice at the moment streaming ends.
#[test]
fn streamed_turn_fixture_reconstructs_exactly_the_final_message() {
    let name = "streamed-turn.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read streamed turn fixture bytes");
    let fixture_sha256 = format!("{:x}", Sha256::digest(&fixture_bytes));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(
        extension_manifest.contains(&format!(
            "streamed_turn_fixture_sha256 = \"{fixture_sha256}\""
        )),
        "the extension manifest must register the exact streamed turn fixture bytes"
    );
    assert!(extension_manifest.contains("streamed_turn_fixture = \"streamed-turn.json\""));

    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    let (first_view, first_cursor, first_digest) = replay_fixture(&fixture);
    let (second_view, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(first_view, second_view);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);

    // The stream itself carries ordered chunks scoped to one session and one
    // message id, followed by the terminal completion fact.
    let mut chunks = Vec::new();
    let mut terminal_output = None;
    for envelope in &fixture.events {
        let RuntimeWireEvent::Known(event) = &envelope.event else {
            continue;
        };
        match &event.kind {
            RuntimeEventKind::AssistantDelta {
                message_id,
                session_id,
                content,
                ..
            } => {
                assert_eq!(session_id.as_deref(), Some("session_streamed_turn"));
                assert_eq!(message_id, "message_streamed_turn_reply");
                chunks.push(content.clone());
            }
            RuntimeEventKind::AgentSessionCompleted { session } => {
                terminal_output = session.output.clone();
            }
            _ => {}
        }
    }
    assert!(chunks.len() >= 3, "a streamed turn must carry many chunks");
    let reconstructed = chunks.concat();

    let assistant_messages = first_view
        .agent_conversation
        .iter()
        .filter(|message| {
            message.session_id == "session_streamed_turn"
                && message.role == AgentConversationRole::Assistant
        })
        .collect::<Vec<_>>();
    assert_eq!(
        assistant_messages.len(),
        1,
        "the completion fact must settle the streamed message, not duplicate it"
    );
    let reply = assistant_messages[0];
    assert_eq!(reply.message_id, "message_streamed_turn_reply");
    assert_eq!(reply.content, reconstructed);
    assert_eq!(terminal_output.as_deref(), Some(reply.content.as_str()));

    // GUI-CORE-016, amended 2026-09-07: the unscoped assistant stream is the
    // in-flight surface. DURING the turn it holds the identical reply, so a
    // client that predates owner-scoped conversation still renders it live.
    let terminal_index = fixture
        .events
        .iter()
        .position(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(event)
                    if matches!(event.kind, RuntimeEventKind::AgentSessionCompleted { .. })
            )
        })
        .expect("the streamed turn fixture must carry a completion fact");
    let mut mid_turn_view = RuntimeViewState::new(fixture.initial_snapshot.clone());
    for envelope in fixture.events.iter().take(terminal_index) {
        if let RuntimeWireEvent::Known(event) = &envelope.event {
            mid_turn_view.apply_event(event);
        }
    }
    assert_eq!(
        mid_turn_view.assistant_stream, reconstructed,
        "the unscoped stream must hold the whole reply while the turn is in flight"
    );

    // AFTER settlement the stream is cleared: the reply is carried by the
    // completion fact and by the owner-scoped conversation asserted above.
    assert!(
        first_view.assistant_stream.is_empty(),
        "a settled turn must not leave its reply in the unscoped stream"
    );
}

#[test]
#[ignore = "manual streamed turn fixture refresh; normal tests validate committed JSON only"]
fn refresh_streamed_turn_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = streamed_turn_fixture();
    fs::write(
        root.join("streamed-turn.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

/// GUI-CORE-017: canonical proof that an ACP turn returning an image alongside
/// text reaches the client as typed content parts.
///
/// Parts attach to the message they belong to, so a second message in the same
/// session stays part-free. A part kind this build does not model round-trips
/// verbatim instead of being dropped into prose that claims content exists.
#[test]
fn message_parts_fixture_attaches_typed_parts_to_their_own_message() {
    let name = "message-parts.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read message parts fixture bytes");
    let fixture_sha256 = format!("{:x}", Sha256::digest(&fixture_bytes));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(
        extension_manifest.contains(&format!(
            "message_parts_fixture_sha256 = \"{fixture_sha256}\""
        )),
        "the extension manifest must register the exact message parts fixture bytes"
    );
    assert!(extension_manifest.contains("message_parts_fixture = \"message-parts.json\""));

    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    let (first_view, first_cursor, first_digest) = replay_fixture(&fixture);
    let (second_view, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(first_view, second_view);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);

    let message = |message_id: &str| {
        first_view
            .agent_conversation
            .iter()
            .find(|message| {
                message.session_id == "session_message_parts" && message.message_id == message_id
            })
            .unwrap_or_else(|| panic!("the fixture must project {message_id}"))
    };
    let reply = message("message_parts_reply");
    let follow_up = message("message_parts_follow_up");

    // Both parts landed on the message their event named, never on the later
    // message that happens to share the session.
    assert_eq!(reply.parts.len(), 2);
    assert!(
        follow_up.parts.is_empty(),
        "a part must not drift onto another message in the same session"
    );

    let AgentContentPart::Image {
        media_type,
        reference,
        alt,
    } = &reply.parts[0]
    else {
        panic!("the first part must be the returned image");
    };
    assert_eq!(media_type, "image/png");
    // An immutable content reference into the Agent parts directory, named by
    // the content digest. Bytes never travel on the wire.
    assert_eq!(
        reference,
        &format!(".viden/agents/parts/{}.png", "7c".repeat(32))
    );
    assert_eq!(alt.as_deref(), Some("message.part.coverage_chart"));

    // An unmodeled kind keeps its exact published object, so re-encoding the
    // projected part is byte-identical to what Core sent.
    let unknown = &reply.parts[1];
    let AgentContentPart::Unknown { kind, payload } = unknown else {
        panic!("the second part must stay an unmodeled kind");
    };
    assert_eq!(kind, "audio");
    let encoded = serde_json::to_value(unknown).expect("re-encode the unmodeled part");
    assert_eq!(&encoded, payload);
    let decoded: AgentContentPart =
        serde_json::from_value(encoded).expect("re-decode the unmodeled part");
    assert_eq!(&decoded, unknown);

    // The text surface stays the compatibility contract: a client that predates
    // parts still renders the same prose Core published.
    assert_eq!(reply.content, "Rendered the coverage chart.");
}

#[test]
#[ignore = "manual message parts fixture refresh; normal tests validate committed JSON only"]
fn refresh_message_parts_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = message_parts_fixture();
    fs::write(
        root.join("message-parts.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

/// GUI-CORE-024: canonical proof that an audit page names the read it answers
/// and that a server-side filter is applied before the page is cut.
///
/// The two concurrent reads are answered out of order, so nothing but the
/// command id can attribute a page. The filtered read comes back `complete`
/// while strictly older non-agent records are visible on the unfiltered pages —
/// a client filtering a page it already held could not have established that.
#[test]
fn audit_reads_fixture_attributes_each_page_and_filters_before_paging() {
    let name = "audit-reads.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read audit reads fixture bytes");
    let fixture_sha256 = format!("{:x}", Sha256::digest(&fixture_bytes));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(
        extension_manifest.contains(&format!(
            "audit_reads_fixture_sha256 = \"{fixture_sha256}\""
        )),
        "the extension manifest must register the exact audit reads fixture bytes"
    );
    assert!(extension_manifest.contains("audit_reads_fixture = \"audit-reads.json\""));

    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    let (_, first_cursor, first_digest) = replay_fixture(&fixture);
    let (_, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);
    let known = |envelope: &RuntimeEventEnvelope| match &envelope.event {
        RuntimeWireEvent::Known(event) => event.kind.clone(),
        RuntimeWireEvent::Unknown { event_type, .. } => {
            panic!("audit fixture events must all be known, got {event_type}")
        }
    };
    let kinds = fixture.events.iter().map(known).collect::<Vec<_>>();

    // An audit page is a query result, not view state: reducing every page in
    // the fixture must leave the view exactly as the snapshot published it, so
    // a client can never read a page as runtime truth.
    let mut pages_only = RuntimeViewState::new(fixture.initial_snapshot.clone());
    for kind in &kinds {
        if matches!(kind, RuntimeEventKind::AuditPageLoaded { .. }) {
            pages_only.apply_event(&RuntimeEvent::new(1, kind.clone()));
        }
    }
    assert_eq!(
        canonical_view_sha256(&pages_only),
        canonical_view_sha256(&RuntimeViewState::new(fixture.initial_snapshot.clone())),
        "an audit page must never fold into RuntimeViewState"
    );

    let accepted_ids = kinds
        .iter()
        .filter_map(|kind| match kind {
            RuntimeEventKind::CommandAccepted {
                command_id,
                command: RuntimeCommand::QueryAudit { .. },
            } => Some(command_id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let pages = kinds
        .iter()
        .filter_map(|kind| match kind {
            RuntimeEventKind::AuditPageLoaded { command_id, page } => {
                Some((command_id.clone(), page.clone()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(accepted_ids.len(), 3);
    assert_eq!(pages.len(), 3);

    // Both concurrent reads were accepted before either was answered, and the
    // pages came back in the opposite order, so arrival order attributes them
    // wrongly and only the published command id gets them right.
    let first_page_position = kinds
        .iter()
        .position(|kind| matches!(kind, RuntimeEventKind::AuditPageLoaded { .. }))
        .expect("the fixture must publish a page");
    assert!(
        first_page_position > 1,
        "both concurrent reads must be accepted before either is answered"
    );
    assert_eq!(pages[0].0.as_deref(), Some("audit_read_second"));
    assert_eq!(pages[1].0.as_deref(), Some("audit_read_first"));
    for (command_id, _) in &pages {
        let command_id = command_id.as_deref().expect("every page names its read");
        assert!(
            accepted_ids.iter().any(|accepted| accepted == command_id),
            "a page must name a read Core actually accepted, got {command_id}"
        );
    }

    // The two reads' pages are genuinely different answers, so attributing one
    // to the other read would have been a visible error, not a harmless swap.
    let (_, second_read_page) = &pages[0];
    let (_, first_read_page) = &pages[1];
    assert_eq!(second_read_page.records.len(), 3);
    assert!(second_read_page.complete);
    assert_eq!(first_read_page.records.len(), 2);
    assert!(!first_read_page.complete);
    assert_eq!(
        first_read_page.next_before,
        Some(second_read_page.records[1].cursor()),
        "the incomplete page's cursor names the record it stopped at"
    );

    // The filtered read: Core applied the actor filter before cutting the page,
    // so `complete` is the agent timeline's completeness even though the
    // unfiltered pages above carry strictly older operator and system records.
    let filtered_query = kinds
        .iter()
        .find_map(|kind| match kind {
            RuntimeEventKind::CommandAccepted {
                command_id,
                command: RuntimeCommand::QueryAudit { query },
            } if command_id == "audit_read_agents" => Some(query.clone()),
            _ => None,
        })
        .expect("the fixture must accept a filtered read");
    assert_eq!(filtered_query.actor, Some(AuditActorFilter::AnyAgent));
    let (_, filtered_page) = &pages[2];
    assert_eq!(pages[2].0.as_deref(), Some("audit_read_agents"));
    assert!(
        filtered_page
            .records
            .iter()
            .all(|record| matches!(record.actor, AuditActor::Agent { .. })),
        "a filtered page must contain only records the filter kept"
    );
    assert!(filtered_page.complete);
    assert_eq!(filtered_page.next_before, None);
    let oldest_filtered = filtered_page
        .records
        .last()
        .expect("the filtered page must not be empty")
        .timestamp;
    assert!(
        second_read_page
            .records
            .iter()
            .any(|record| record.timestamp < oldest_filtered),
        "older unfiltered records must exist, so `complete` can only be the filtered timeline"
    );
}

#[test]
#[ignore = "manual audit reads fixture refresh; normal tests validate committed JSON only"]
fn refresh_audit_reads_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = audit_reads_fixture();
    fs::write(
        root.join("audit-reads.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

/// GUI-CORE-022: canonical proof that a workspace inventory page names the read
/// it answers, that its entries are ordered, and that a workspace with no read
/// leaves a client with no file list at all.
///
/// Two concurrent reads on the same project are answered in the opposite order,
/// so nothing but the command id can attribute a page — and unlike an audit
/// page the id is required, so there is no fallback case to exercise. A second
/// project publishes an ordinary workspace stream with no inventory page: that
/// is the "without one" half the contract request asks for, and the honest
/// answer there is no file list, never an empty one.
#[test]
fn workspace_files_fixture_attributes_each_page_and_leaves_an_unread_project_listless() {
    let name = "workspace-files.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read workspace files fixture bytes");
    let fixture_sha256 = format!("{:x}", Sha256::digest(&fixture_bytes));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(
        extension_manifest.contains(&format!(
            "workspace_files_fixture_sha256 = \"{fixture_sha256}\""
        )),
        "the extension manifest must register the exact workspace files fixture bytes"
    );
    assert!(extension_manifest.contains("workspace_files_fixture = \"workspace-files.json\""));

    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    let (_, first_cursor, first_digest) = replay_fixture(&fixture);
    let (_, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);

    let known = |envelope: &RuntimeEventEnvelope| match &envelope.event {
        RuntimeWireEvent::Known(event) => event.kind.clone(),
        RuntimeWireEvent::Unknown { event_type, .. } => panic!(
            "workspace files fixture events must all be known, got {event_type} — a quarantined \
             inventory page reads as an empty workspace"
        ),
    };
    let kinds = fixture.events.iter().map(known).collect::<Vec<_>>();

    // An inventory page is a query result, not view state: reducing every page
    // in the fixture must leave the view exactly as the snapshot published it,
    // so a client can never read a page as runtime truth.
    let mut pages_only = RuntimeViewState::new(fixture.initial_snapshot.clone());
    for kind in &kinds {
        if matches!(kind, RuntimeEventKind::WorkspaceFilesLoaded { .. }) {
            pages_only.apply_event(&RuntimeEvent::new(1, kind.clone()));
        }
    }
    assert_eq!(
        canonical_view_sha256(&pages_only),
        canonical_view_sha256(&RuntimeViewState::new(fixture.initial_snapshot.clone())),
        "a workspace inventory page must never fold into RuntimeViewState"
    );

    let accepted_ids = kinds
        .iter()
        .filter_map(|kind| match kind {
            RuntimeEventKind::CommandAccepted {
                command_id,
                command: RuntimeCommand::QueryWorkspaceFiles { .. },
            } => Some(command_id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let pages = kinds
        .iter()
        .filter_map(|kind| match kind {
            RuntimeEventKind::WorkspaceFilesLoaded { command_id, page } => {
                Some((command_id.clone(), page.clone()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(accepted_ids.len(), 2);
    assert_eq!(pages.len(), 2);

    // Both reads were accepted before either was answered, and the pages come
    // back in the opposite order, so arrival order attributes them wrongly and
    // only the published command id gets them right.
    let first_page_position = kinds
        .iter()
        .position(|kind| matches!(kind, RuntimeEventKind::WorkspaceFilesLoaded { .. }))
        .expect("the fixture must publish a page");
    assert!(
        first_page_position > 1,
        "both concurrent reads must be accepted before either is answered"
    );
    assert_eq!(pages[0].0, "workspace_files_second");
    assert_eq!(pages[1].0, "workspace_files_first");
    for (command_id, _) in &pages {
        assert!(
            accepted_ids.iter().any(|accepted| accepted == command_id),
            "a page must name a read Core actually accepted, got {command_id}"
        );
    }

    // The two reads' pages are genuinely different answers, so attributing one
    // to the other read would have been a visible error, not a harmless swap.
    let (_, scoped_page) = &pages[0];
    let (_, root_page) = &pages[1];
    assert_ne!(scoped_page, root_page);

    // Every page is ordered, and the incomplete one names the entry it stopped
    // at so the next read resumes exclusively after it.
    for (command_id, page) in &pages {
        let paths = page
            .entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(paths, sorted, "page {command_id} must be lexicographic");
        for entry in &page.entries {
            assert!(
                !entry.path.starts_with('/') && !entry.path.contains(".."),
                "a published path must stay workspace-relative, got {}",
                entry.path
            );
            if entry.kind == WorkspaceFileKind::Dir {
                assert_eq!(
                    entry.size_bytes, None,
                    "a directory must publish no byte size"
                );
            }
        }
    }
    assert!(!root_page.complete);
    assert_eq!(
        root_page.next_after.as_deref(),
        root_page.entries.last().map(|entry| entry.path.as_str()),
        "the incomplete page's cursor names the entry it stopped at"
    );
    assert!(scoped_page.complete);
    assert_eq!(scoped_page.next_after, None);

    // The scoped read applied its prefix before the page was cut, so
    // `complete` is the subtree's completeness even though the root page above
    // is still incomplete.
    let scoped_query = kinds
        .iter()
        .find_map(|kind| match kind {
            RuntimeEventKind::CommandAccepted {
                command_id,
                command: RuntimeCommand::QueryWorkspaceFiles { query },
            } if command_id == "workspace_files_second" => Some(query.clone()),
            _ => None,
        })
        .expect("the fixture must accept the scoped read");
    let prefix = scoped_query
        .prefix
        .as_deref()
        .expect("the scoped read must carry a prefix");
    assert!(
        scoped_page
            .entries
            .iter()
            .all(|entry| entry.path.starts_with(prefix)),
        "a scoped page must contain only paths the prefix kept"
    );

    // The second project: an ordinary workspace stream with no inventory read
    // and no inventory page. A client scoped to it has no file list at all,
    // which is the honest answer — never an empty one it could render as "this
    // project has no files".
    let unread_project = "project_viden_docs";
    let unread_owner_events = fixture
        .events
        .iter()
        .filter(|envelope| envelope.owner.project_id == unread_project)
        .collect::<Vec<_>>();
    assert!(
        !unread_owner_events.is_empty(),
        "the fixture must publish a second workspace stream"
    );
    assert!(
        unread_owner_events.iter().all(|envelope| !matches!(
            &envelope.event,
            RuntimeWireEvent::Known(event)
                if matches!(
                    event.kind,
                    RuntimeEventKind::WorkspaceFilesLoaded { .. }
                        | RuntimeEventKind::CommandAccepted {
                            command: RuntimeCommand::QueryWorkspaceFiles { .. },
                            ..
                        }
                )
        )),
        "the unread project must publish neither an inventory read nor a page"
    );
}

#[test]
#[ignore = "manual workspace files fixture refresh; normal tests validate committed JSON only"]
fn refresh_workspace_files_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = workspace_files_fixture();
    fs::write(
        root.join("workspace-files.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

/// GUI-CORE-012: canonical proof that Core publishes diffs as rows rather than
/// as text a client has to parse, at every site the 0.3.3 contract design
/// names.
///
/// One `edit_file` approval whose `decision_context` holds a single file and a
/// single hunk with its `base_sha256`; one `MergeAgentPatch` approval carrying
/// the two-file change the trust loop would apply; one `WorkspaceDiffLoaded`
/// page whose two entries prove that a staged path and a bounded one are
/// distinguishable; and one refused read answered by `CommandRejected` with
/// the same command id. The refusal is in the fixture on purpose: an empty
/// page and a refused page are different facts, and this is the corpus that
/// says so.
#[test]
fn structured_diff_fixture_carries_decision_context_pages_and_a_refusal() {
    let name = "structured-diff.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read structured diff fixture bytes");
    let fixture_sha256 = format!("{:x}", Sha256::digest(&fixture_bytes));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(
        extension_manifest.contains(&format!(
            "structured_diff_fixture_sha256 = \"{fixture_sha256}\""
        )),
        "the extension manifest must register the exact structured diff fixture bytes"
    );
    assert!(extension_manifest.contains("structured_diff_fixture = \"structured-diff.json\""));

    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    let (_, first_cursor, first_digest) = replay_fixture(&fixture);
    let (_, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);
    assert!(
        extension_manifest.contains(&format!("structured_diff_view_sha256 = \"{first_digest}\"")),
        "the extension manifest must register the replayed view digest"
    );

    let kinds = fixture
        .events
        .iter()
        .map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(event) => event.kind.clone(),
            RuntimeWireEvent::Unknown { event_type, .. } => panic!(
                "structured diff fixture events must all be known, got {event_type} — a \
                 quarantined diff page reads to a reviewer as `nothing changed`"
            ),
        })
        .collect::<Vec<_>>();

    // A diff page is a query answer, not view state: reducing every page must
    // leave the view exactly as the snapshot published it.
    let mut pages_only = RuntimeViewState::new(fixture.initial_snapshot.clone());
    for kind in &kinds {
        if matches!(kind, RuntimeEventKind::WorkspaceDiffLoaded { .. }) {
            pages_only.apply_event(&RuntimeEvent::new(1, kind.clone()));
        }
    }
    assert_eq!(
        canonical_view_sha256(&pages_only),
        canonical_view_sha256(&RuntimeViewState::new(fixture.initial_snapshot.clone())),
        "a workspace diff page must never fold into RuntimeViewState"
    );

    let approvals = kinds
        .iter()
        .filter_map(|kind| match kind {
            RuntimeEventKind::ApprovalRequested { approval } => Some(approval.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(approvals.len(), 2);

    // The single-file case: one file, one hunk, and the hash of the bytes the
    // preview was computed against, which is what lets a client detect the
    // file moving between preview and execution.
    let edit = &approvals[0];
    assert_eq!(edit.tool_name, "edit_file");
    let edit_context = edit
        .decision_context
        .as_ref()
        .expect("an edit_file approval carries its decision context");
    assert_eq!(edit_context.base_sha256.as_ref().map(String::len), Some(64));
    let edit_diff = edit_context.diff.as_ref().expect("the previewed rows");
    assert_eq!(edit_diff.files.len(), 1);
    assert_eq!(edit_diff.files[0].hunks.len(), 1);
    assert!(!edit_diff.truncated);
    for line in &edit_diff.files[0].hunks[0].lines {
        match line.kind {
            DiffLineKind::Added => assert_eq!(line.old_line, None),
            DiffLineKind::Removed => assert_eq!(line.new_line, None),
            _ => assert!(line.old_line.is_some() && line.new_line.is_some()),
        }
    }

    // The multi-file case GUI-CORE-012 asked for. No `base_sha256`: one hash
    // cannot describe several files, and naming one would invite a client to
    // check the wrong one.
    let merge = &approvals[1];
    assert_eq!(merge.tool_name, "workflow_merge_agent_patch");
    let merge_context = merge
        .decision_context
        .as_ref()
        .expect("a patch merge approval carries the change it applies");
    assert_eq!(merge_context.base_sha256, None);
    let merge_diff = merge_context.diff.as_ref().expect("the patch rows");
    assert_eq!(merge_diff.files.len(), 2);

    let pages = kinds
        .iter()
        .filter_map(|kind| match kind {
            RuntimeEventKind::WorkspaceDiffLoaded { command_id, page } => {
                Some((command_id.clone(), page.clone()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(pages.len(), 1);
    let (page_command_id, page) = &pages[0];
    assert!(
        kinds.iter().any(|kind| matches!(
            kind,
            RuntimeEventKind::CommandAccepted { command_id, command: RuntimeCommand::QueryWorkspaceDiff { .. } }
                if command_id == page_command_id
        )),
        "a page must name a read Core actually accepted"
    );
    assert_eq!(page.entries.len(), 2);
    let staged = &page.entries[0];
    assert!(staged.staged);
    assert_eq!(staged.index, Some(WorkspaceChangeKind::Modified));
    assert!(!staged.diff.as_ref().expect("a staged entry's rows").omitted);
    // The bounded entry keeps real counts with no rows, so "not shown" can
    // never be read as "unchanged".
    let bounded = page.entries[1]
        .diff
        .as_ref()
        .expect("a bounded entry still names its file");
    assert!(bounded.omitted);
    assert!(bounded.hunks.is_empty());
    assert!(bounded.additions > 0 || bounded.deletions > 0);
    assert!(page.truncated);

    // The refusal: same command id as the read it answers, no page beside it.
    let refused = kinds
        .iter()
        .find_map(|kind| match kind {
            RuntimeEventKind::CommandRejected { command_id, reason } => {
                Some((command_id.clone(), reason.clone()))
            }
            _ => None,
        })
        .expect("the fixture must carry a refused read");
    assert!(
        kinds.iter().any(|kind| matches!(
            kind,
            RuntimeEventKind::CommandAccepted { command_id, command: RuntimeCommand::QueryWorkspaceDiff { .. } }
                if *command_id == refused.0
        )),
        "the refusal must name the exact read it answers"
    );
    assert_ne!(refused.0, *page_command_id);
    assert!(refused.1.contains("git_diff"));
    assert!(refused.1.contains("hint:"));
    assert!(
        pages.iter().all(|(command_id, _)| *command_id != refused.0),
        "a refused read must never also be answered with a page"
    );
}

#[test]
#[ignore = "manual structured diff fixture refresh; normal tests validate committed JSON only"]
fn refresh_structured_diff_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = structured_diff_fixture();
    fs::write(
        root.join("structured-diff.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

/// GUI-CORE-025: canonical proof that the evidence archive is pageable, that a
/// filter narrows what `complete` describes, that content is typed even when
/// there is none, and that a refusal is never an empty page.
///
/// The four failure modes this guards against all render identically in a
/// naive client — as "no evidence" — and each is a different fact: a cut page
/// has more rows behind a cursor, a filtered page is complete for its filter
/// only, a summary-only row exists and has no canonical bytes, and a refused
/// query was never answered at all.
#[test]
fn evidence_reads_fixture_pages_the_archive_and_types_every_absence() {
    let name = "evidence-reads.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read evidence reads fixture bytes");
    let fixture_sha256 = format!("{:x}", Sha256::digest(&fixture_bytes));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(
        extension_manifest.contains(&format!(
            "evidence_reads_fixture_sha256 = \"{fixture_sha256}\""
        )),
        "the extension manifest must register the exact evidence reads fixture bytes"
    );
    assert!(extension_manifest.contains("evidence_reads_fixture = \"evidence-reads.json\""));

    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    let (_, first_cursor, first_digest) = replay_fixture(&fixture);
    let (_, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);
    assert!(
        extension_manifest.contains(&format!("evidence_reads_view_sha256 = \"{first_digest}\"")),
        "the extension manifest must register the replayed view digest"
    );

    let kinds = fixture
        .events
        .iter()
        .map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(event) => event.kind.clone(),
            RuntimeWireEvent::Unknown { event_type, .. } => panic!(
                "evidence reads fixture events must all be known, got {event_type} — a \
                 quarantined page reads to an operator as `no evidence was recorded`"
            ),
        })
        .collect::<Vec<_>>();

    // Both answers are query results: reducing every one of them must leave
    // the view exactly as the snapshot published it. `latest_evidence` in
    // particular must stay empty, because an archive page overwriting the
    // recent window is the one confusion this capability must not create.
    let mut answers_only = RuntimeViewState::new(fixture.initial_snapshot.clone());
    for kind in &kinds {
        if matches!(
            kind,
            RuntimeEventKind::EvidencePageLoaded { .. }
                | RuntimeEventKind::EvidenceContentLoaded { .. }
        ) {
            answers_only.apply_event(&RuntimeEvent::new(1, kind.clone()));
        }
    }
    assert!(answers_only.latest_evidence.is_empty());
    assert_eq!(
        canonical_view_sha256(&answers_only),
        canonical_view_sha256(&RuntimeViewState::new(fixture.initial_snapshot.clone())),
        "an evidence read answer must never fold into RuntimeViewState"
    );

    let pages = kinds
        .iter()
        .filter_map(|kind| match kind {
            RuntimeEventKind::EvidencePageLoaded { command_id, page } => {
                Some((command_id.clone(), page.clone()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(pages.len(), 3, "two archive pages plus one filtered page");

    // The two archive pages tile: page one is incomplete and names a cursor,
    // page two resumes from exactly that cursor and completes, and no row
    // appears on both.
    let (first_id, first_page) = &pages[0];
    let (second_id, second_page) = &pages[1];
    assert_eq!(first_id, "evidence_read_first");
    assert_eq!(second_id, "evidence_read_second");
    assert!(!first_page.complete);
    let resume = first_page
        .next_after
        .as_deref()
        .expect("an incomplete page names where to resume");
    assert!(second_page.complete);
    assert_eq!(second_page.next_after, None);
    let resumed_from = kinds
        .iter()
        .find_map(|kind| match kind {
            RuntimeEventKind::CommandAccepted {
                command_id,
                command: RuntimeCommand::QueryEvidence { query },
            } if command_id == "evidence_read_second" => query.after.clone(),
            _ => None,
        })
        .expect("the second read carries the cursor it resumed from");
    assert_eq!(
        resumed_from, resume,
        "page two resumes from page one's cursor verbatim, never a reconstructed one"
    );
    let paged = first_page
        .entries
        .iter()
        .chain(second_page.entries.iter())
        .map(|entry| (entry.timestamp, entry.id.clone()))
        .collect::<Vec<_>>();
    let mut ordered = paged.clone();
    ordered.sort();
    ordered.dedup();
    assert_eq!(
        paged, ordered,
        "the archive is ascending on (timestamp, id) and no row is repeated across pages"
    );
    assert_eq!(paged.len(), 3);

    // The filtered page is complete for `patch` while the unfiltered archive
    // above was not. A client that read `complete` as a fact about the archive
    // would stop paging after one filtered read.
    let (filtered_id, filtered_page) = &pages[2];
    assert_eq!(filtered_id, "evidence_read_patches");
    assert!(filtered_page.complete);
    assert!(
        filtered_page
            .entries
            .iter()
            .all(|entry| entry.kind == "patch")
    );
    assert!(
        filtered_page.entries.len() < paged.len(),
        "the filter kept fewer rows than the archive holds, which is what makes \
         `complete` a claim about the filter"
    );

    // Content: three reads, three different typed answers, none of them empty.
    let contents = kinds
        .iter()
        .filter_map(|kind| match kind {
            RuntimeEventKind::EvidenceContentLoaded {
                evidence_id,
                content,
                ..
            } => Some((evidence_id.clone(), content.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(contents.len(), 3);
    let text = contents
        .iter()
        .find(|(id, _)| id == "evidence_bravo_tests")
        .map(|(_, content)| content.clone())
        .expect("the test_result row answers with text");
    let EvidenceContent::Text {
        truncated, sha256, ..
    } = text
    else {
        panic!("a non-patch canonical row answers with bounded text");
    };
    assert!(!truncated);
    assert_eq!(
        sha256.len(),
        64,
        "the answer names the hash the bytes were verified against"
    );
    let patch = contents
        .iter()
        .find(|(id, _)| id == "evidence_alpha_patch")
        .map(|(_, content)| content.clone())
        .expect("the patch row answers with a diff");
    let EvidenceContent::Diff { document, .. } = patch else {
        panic!("a `patch` row answers with parsed diff rows, not raw text");
    };
    assert_eq!(document.files.len(), 1);
    assert_eq!(document.files[0].hunks.len(), 1);
    assert!(!document.truncated);
    assert!(matches!(
        contents
            .iter()
            .find(|(id, _)| id == "evidence_charlie_summary")
            .map(|(_, content)| content.clone()),
        Some(EvidenceContent::Unavailable {
            reason: EvidenceUnavailableReason::SummaryOnly
        }),
    ));

    // The refusal answers the read that asked and publishes no page. An empty
    // page here would be indistinguishable from an empty archive.
    assert!(kinds.iter().any(|kind| matches!(
        kind,
        RuntimeEventKind::CommandRejected { command_id, reason }
            if command_id == "evidence_read_overlimit" && reason.contains("32 entry bound")
    )));
    assert!(
        pages
            .iter()
            .all(|(command_id, _)| command_id != "evidence_read_overlimit"),
        "a refused read must publish no page at all"
    );
}

#[test]
#[ignore = "manual evidence reads fixture refresh; normal tests validate committed JSON only"]
fn refresh_evidence_reads_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = evidence_reads_fixture();
    fs::write(
        root.join("evidence-reads.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

/// GUI-CORE-020: canonical proof that an operator source-control action is
/// answered by a typed outcome, that a refusal and a failure are different
/// facts, and that neither is ever a silent success.
///
/// A `Stage` is refused by policy and answered by `CommandRejected`; a
/// `Commit` goes through the ask path with the staged rows attached, resolves,
/// completes, and is followed by a resampled source showing `ahead` moved and
/// the tree clean; a `Push` finishes with a `Failed { NoUpstream }` outcome
/// rather than a rejection, because the gate said yes and the attempt was
/// audited. A client that rendered the refusal and the failure the same way
/// would send an operator to their permission rules for a tracking problem.
#[test]
fn operator_git_fixture_separates_a_refusal_a_completion_and_a_failure() {
    let name = "operator-git.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read operator git fixture bytes");
    let fixture_sha256 = format!("{:x}", Sha256::digest(&fixture_bytes));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(
        extension_manifest.contains(&format!(
            "operator_git_fixture_sha256 = \"{fixture_sha256}\""
        )),
        "the extension manifest must register the exact operator git fixture bytes"
    );
    assert!(extension_manifest.contains("operator_git_fixture = \"operator-git.json\""));

    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    let (view, first_cursor, first_digest) = replay_fixture(&fixture);
    let (_, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);
    assert!(
        extension_manifest.contains(&format!("operator_git_view_sha256 = \"{first_digest}\"")),
        "the extension manifest must register the replayed view digest"
    );

    let kinds = fixture
        .events
        .iter()
        .map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(event) => event.kind.clone(),
            RuntimeWireEvent::Unknown { event_type, .. } => panic!(
                "operator git fixture events must all be known, got {event_type} — a client whose \
                 push answer was quarantined reads the silence as success"
            ),
        })
        .collect::<Vec<_>>();

    // The finished event is a settled answer to one command, not view state.
    // The `WorkspaceSourceUpdated` beside it is what a client's chip reads, and
    // that one *is* reduced — which is why the final view carries the source.
    let mut finished_only = RuntimeViewState::new(fixture.initial_snapshot.clone());
    for kind in &kinds {
        if matches!(kind, RuntimeEventKind::OperatorGitActionFinished { .. }) {
            finished_only.apply_event(&RuntimeEvent::new(1, kind.clone()));
        }
    }
    assert_eq!(
        canonical_view_sha256(&finished_only),
        canonical_view_sha256(&RuntimeViewState::new(fixture.initial_snapshot.clone())),
        "a finished operator action must never fold into RuntimeViewState"
    );
    let source = view
        .workspace_source
        .as_ref()
        .expect("the resampled source must reach the view");
    assert_eq!(source.ahead, 2, "the completed commit moved the branch");
    assert!(!source.dirty, "the completed commit cleaned the tree");

    let refused = kinds
        .iter()
        .find_map(|kind| match kind {
            RuntimeEventKind::CommandRejected { command_id, reason } => {
                Some((command_id.clone(), reason.clone()))
            }
            _ => None,
        })
        .expect("the fixture must carry a refused action");
    assert!(refused.1.contains("git_add"), "{}", refused.1);
    assert!(refused.1.contains("hint:"));

    let settled = kinds
        .iter()
        .filter_map(|kind| match kind {
            RuntimeEventKind::OperatorGitActionFinished {
                command_id,
                outcome,
                audit_id,
                ..
            } => Some((command_id.clone(), outcome.clone(), audit_id.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(settled.len(), 2);
    assert!(
        settled
            .iter()
            .all(|(command_id, _, audit_id)| *command_id != refused.0 && !audit_id.is_empty()),
        "a refused action is never also settled, and every settled action names its audit record"
    );
    assert!(matches!(
        settled[0].1,
        OperatorGitOutcome::Completed {
            truncated: false,
            ..
        }
    ));
    assert!(
        matches!(
            settled[1].1,
            OperatorGitOutcome::Failed {
                class: OperatorGitFailureClass::NoUpstream,
                ..
            }
        ),
        "a push that could not be tracked is a failed outcome, not a rejection"
    );

    // The commit approval shows what is being committed, and is ranked and
    // grouped as source control rather than as one anonymous tool call.
    let approval = kinds
        .iter()
        .find_map(|kind| match kind {
            RuntimeEventKind::ApprovalRequested { approval } => Some(approval.clone()),
            _ => None,
        })
        .expect("the fixture must carry the commit approval");
    assert_eq!(approval.tool_name, "git_commit");
    assert_eq!(approval.target.kind, "git");
    assert_eq!(approval.risk, ApprovalRisk::Medium);
    assert!(
        approval
            .decision_context
            .and_then(|context| context.diff)
            .is_some_and(|diff| !diff.files.is_empty()),
        "a commit approval must show the staged rows it would turn into a commit"
    );
}

/// GUI-CORE-015: canonical proof that a conflict is published as lines with a
/// named baseline, and that the two attach sites answer with the same shape.
///
/// The merge bounce and the Lane conflict both carry `ours`, `theirs`, and the
/// patch preimage, each separately addressable. Their baselines differ by kind
/// on purpose — a gate's baseline is its canonical reviewed evidence, a Lane's
/// is the revision its file was read at — and neither is invented. Nothing in
/// the payload is a merged result: Core computed no merge base, and a client
/// that presented these three sides as a resolvable three-way merge would
/// offer an auto-resolution Core never produced.
#[test]
fn conflict_content_fixture_publishes_two_sides_and_a_preimage() {
    let name = "conflict-content.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read conflict content fixture bytes");
    let fixture_sha256 = format!("{:x}", Sha256::digest(&fixture_bytes));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(
        extension_manifest.contains(&format!(
            "conflict_content_fixture_sha256 = \"{fixture_sha256}\""
        )),
        "the extension manifest must register the exact conflict content fixture bytes"
    );
    assert!(extension_manifest.contains("conflict_content_fixture = \"conflict-content.json\""));

    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    let (view, first_cursor, first_digest) = replay_fixture(&fixture);
    let (_, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);
    assert!(
        extension_manifest.contains(&format!(
            "conflict_content_view_sha256 = \"{first_digest}\""
        )),
        "the extension manifest must register the replayed view digest"
    );

    for envelope in &fixture.events {
        if let RuntimeWireEvent::Unknown { event_type, .. } = &envelope.event {
            panic!(
                "conflict content fixture events must all be known, got {event_type} — a client \
                 whose conflict was quarantined is told a merge stalled for no reason"
            );
        }
    }

    // Lane A merged; Lane B did not, and its gate carries the bounce.
    assert!(
        view.merge_gates
            .iter()
            .any(|gate| gate.gate_id == "gate_conflict_a" && gate.status == MergeGateStatus::Merged)
    );
    let bounce = view
        .merge_gates
        .iter()
        .find(|gate| gate.gate_id == "gate_conflict_b")
        .and_then(|gate| gate.conflict.as_ref())
        .expect("the refused merge must leave its bounce on the gate");
    let merge_content = bounce
        .content
        .as_ref()
        .expect("a bounce raised by a failed apply carries its lines");
    assert_eq!(
        merge_content.baseline,
        ConflictBaseline::Evidence {
            bindings: bounce.baseline_evidence.clone()
        },
        "a gate's baseline is the canonical evidence it was reviewed against"
    );

    let lane_conflict = view
        .lane_conflicts
        .iter()
        .find(|conflict| conflict.lane_id == "lane_conflict_b")
        .expect("the lane apply conflict must reach the view");
    let lane_content = lane_conflict
        .content
        .as_ref()
        .expect("a refused lane apply carries its lines");
    assert!(
        matches!(lane_content.baseline, ConflictBaseline::Revision { ref sha } if sha.len() == 40),
        "a Lane has no reviewed baseline, so it names the revision it read"
    );

    // Both sites answer with the same shape: two sides plus the preimage, each
    // separately positioned, and none of them a merged result.
    for content in [merge_content, lane_content] {
        assert!(!content.truncated);
        assert_eq!(content.files.len(), 1);
        assert!(!content.files[0].omitted);
        assert_eq!(content.files[0].hunks.len(), 1);
        let hunk = &content.files[0].hunks[0];
        assert_eq!(hunk.reason, ConflictHunkReason::ContextMismatch);
        assert_eq!(hunk.ours_start, 42);
        assert_eq!(hunk.theirs_start, 42);
        let base = hunk.base.as_ref().expect("the patch preimage is shown");
        assert_ne!(&hunk.ours, &hunk.theirs);
        assert_ne!(&hunk.ours, base);
        assert_ne!(&hunk.theirs, base);
    }
}

#[test]
#[ignore = "manual conflict content fixture refresh; normal tests validate committed JSON only"]
fn refresh_conflict_content_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = conflict_content_fixture();
    fs::write(
        root.join("conflict-content.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

#[test]
#[ignore = "manual operator git fixture refresh; normal tests validate committed JSON only"]
fn refresh_operator_git_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = operator_git_fixture();
    fs::write(
        root.join("operator-git.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

/// GUI-CORE-014: canonical proof that the audit timeline is ordered globally
/// across projects, not per project.
///
/// D10's ticker is one bounded newest-first page spanning every project in the
/// workspace, so the ordering it renders has to be a total order over the whole
/// timeline. The existing `audit-reads` fixture cannot prove that: all three of
/// its records carry the same `project_id`. Here two projects interleave, and
/// one pair of records shares a timestamp across the project boundary, so the
/// `audit_id` tiebreak is exercised where it matters — a client that grouped by
/// project, or that fell back to arrival order on a tie, would produce a
/// visibly different ticker.
#[test]
fn audit_ordering_fixture_orders_two_projects_as_one_newest_first_timeline() {
    let name = "audit-ordering.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read audit ordering fixture bytes");
    let fixture_sha256 = format!("{:x}", Sha256::digest(&fixture_bytes));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(
        extension_manifest.contains(&format!(
            "audit_ordering_fixture_sha256 = \"{fixture_sha256}\""
        )),
        "the extension manifest must register the exact audit ordering fixture bytes"
    );
    assert!(extension_manifest.contains("audit_ordering_fixture = \"audit-ordering.json\""));

    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    let (_, first_cursor, first_digest) = replay_fixture(&fixture);
    let (_, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);

    let page = fixture
        .events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(event) => match &event.kind {
                RuntimeEventKind::AuditPageLoaded { page, .. } => Some(page.clone()),
                _ => None,
            },
            RuntimeWireEvent::Unknown { .. } => None,
        })
        .expect("the fixture must publish one audit page");

    // A total order over the whole timeline: strictly descending on
    // `(timestamp, audit_id)`, which is exactly `AuditRecord::cursor()`.
    let cursors = page
        .records
        .iter()
        .map(AuditRecord::cursor)
        .collect::<Vec<_>>();
    assert!(
        cursors.windows(2).all(|pair| pair[0] > pair[1]),
        "the page must be strictly newest-first by (timestamp, audit_id), got {cursors:?}"
    );

    let projects = page
        .records
        .iter()
        .map(|record| record.owner.project_id.clone())
        .collect::<Vec<_>>();
    let distinct = projects.iter().collect::<BTreeSet<_>>();
    assert_eq!(
        distinct.len(),
        2,
        "the ordering proof needs exactly two projects, got {projects:?}"
    );
    // Genuinely interleaved: a client that grouped the timeline by project
    // would produce a different sequence, so this ordering can only be global.
    assert!(
        projects.windows(2).any(|pair| pair[0] != pair[1])
            && projects
                .windows(2)
                .filter(|pair| pair[0] != pair[1])
                .count()
                > 1,
        "the two projects must interleave rather than sit in blocks, got {projects:?}"
    );

    // The cross-project tie: two records share a timestamp, so only the
    // `audit_id` tiebreak orders them — and it does so across the project
    // boundary, not within one project's own list.
    let tie = page
        .records
        .windows(2)
        .find(|pair| pair[0].timestamp == pair[1].timestamp)
        .expect("the fixture must contain a same-timestamp pair");
    assert_ne!(
        tie[0].owner.project_id, tie[1].owner.project_id,
        "the tie must span two projects, which is what makes the tiebreak global"
    );
    assert!(
        tie[0].audit_id > tie[1].audit_id,
        "a timestamp tie is broken by the descending audit id"
    );

    // Every record still carries the stable id, dotted action key, owner, and
    // timestamp the ticker renders — the close criteria's own field list.
    for record in &page.records {
        assert!(!record.audit_id.is_empty());
        assert!(record.action.contains('.'));
        assert!(!record.owner.project_id.is_empty());
        assert!(record.timestamp > 0);
    }
    assert!(page.complete);
    assert_eq!(page.next_before, None);
}

#[test]
#[ignore = "manual audit ordering fixture refresh; normal tests validate committed JSON only"]
fn refresh_audit_ordering_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = audit_ordering_fixture();
    fs::write(
        root.join("audit-ordering.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

/// GUI-CORE-010: canonical proof that every live-work fact names the owner it
/// belongs to, and that selecting one Lane's exact owner projects that Lane's
/// facts and no others.
///
/// The two Lanes are live at the same time and their facts are interleaved, so
/// neither ordering nor recency can stand in for ownership. The ownerless group
/// proves the honest case: Core did not know an owner, so the fact belongs to
/// no Lane scope while staying visible workspace-wide.
#[test]
fn owner_scoped_live_work_fixture_projects_each_fact_to_its_own_owner() {
    let name = "owner-scoped-live-work.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read owner scoped live work fixture");
    let fixture_sha256 = format!("{:x}", Sha256::digest(&fixture_bytes));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(
        extension_manifest.contains(&format!(
            "owner_scoped_live_work_fixture_sha256 = \"{fixture_sha256}\""
        )),
        "the extension manifest must register the exact owner scoped live work fixture bytes"
    );
    assert!(
        extension_manifest
            .contains("owner_scoped_live_work_fixture = \"owner-scoped-live-work.json\"")
    );

    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    let (first_view, first_cursor, first_digest) = replay_fixture(&fixture);
    let (second_view, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(first_view, second_view);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);

    // Both Lanes are live at once, so "the latest fact" is never a safe pick.
    assert_eq!(first_view.lane_runtime_owners.len(), 2);
    assert_eq!(first_view.tasks.len(), 3);
    assert_eq!(first_view.active_tool_calls.len(), 3);
    assert_eq!(first_view.queued_inputs.len(), 3);
    assert_eq!(first_view.latest_evidence.len(), 3);

    for (lane_id, suffix, other) in [
        ("lane_live_alpha", "alpha", "beta"),
        ("lane_live_beta", "beta", "alpha"),
    ] {
        // The selected owner is the exact Core-bound one, the same identity the
        // context dock resolves a Lane's task scope with.
        let selected = first_view
            .lane_runtime_owners
            .iter()
            .find(|binding| binding.lane_id == lane_id)
            .map(|binding| binding.owner.clone())
            .expect("each fixture Lane publishes exactly one live owner binding");

        let scoped_tasks = first_view
            .tasks
            .iter()
            .filter(|task| task.owner.as_ref() == Some(&selected))
            .map(|task| task.id.as_str())
            .collect::<Vec<_>>();
        let scoped_tools = first_view
            .active_tool_calls
            .iter()
            .filter(|tool| tool.owner.as_ref() == Some(&selected))
            .map(|tool| tool.tool_call_id.as_str())
            .collect::<Vec<_>>();
        let scoped_inputs = first_view
            .queued_inputs
            .iter()
            .filter(|input| input.owner.as_ref() == Some(&selected))
            .map(|input| input.id.as_str())
            .collect::<Vec<_>>();
        let scoped_evidence = first_view
            .latest_evidence
            .iter()
            .filter(|evidence| evidence.owner.as_ref() == Some(&selected))
            .map(|evidence| evidence.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(scoped_tasks, vec![format!("task_live_{suffix}")]);
        assert_eq!(scoped_tools, vec![format!("tool_live_{suffix}")]);
        assert_eq!(scoped_inputs, vec![format!("queued_live_{suffix}")]);
        assert_eq!(scoped_evidence, vec![format!("evidence_live_{suffix}")]);

        // Neither the other Lane's facts nor the ownerless ones can leak in.
        for excluded in [other, "unowned"] {
            assert!(!scoped_tasks.contains(&format!("task_live_{excluded}").as_str()));
            assert!(!scoped_tools.contains(&format!("tool_live_{excluded}").as_str()));
            assert!(!scoped_inputs.contains(&format!("queued_live_{excluded}").as_str()));
            assert!(!scoped_evidence.contains(&format!("evidence_live_{excluded}").as_str()));
        }
    }

    // The ownerless group is published and visible; absence of an owner is a
    // stated fact, not a fact Core dropped.
    assert!(
        first_view
            .tasks
            .iter()
            .any(|task| task.id == "task_live_unowned" && task.owner.is_none())
    );
    assert!(
        first_view
            .active_tool_calls
            .iter()
            .any(|tool| tool.tool_call_id == "tool_live_unowned" && tool.owner.is_none())
    );
    assert!(
        first_view
            .queued_inputs
            .iter()
            .any(|input| input.id == "queued_live_unowned" && input.owner.is_none())
    );
    assert!(
        first_view
            .latest_evidence
            .iter()
            .any(|evidence| evidence.id == "evidence_live_unowned" && evidence.owner.is_none())
    );
}

#[test]
#[ignore = "manual owner scoped live work fixture refresh; normal tests validate committed JSON only"]
fn refresh_owner_scoped_live_work_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = owner_scoped_live_work_fixture();
    fs::write(
        root.join("owner-scoped-live-work.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

#[test]
fn frontend_contract_v1_migrations_are_idempotent_before_fixture_replay() {
    let legacy_lanes = parse_legacy_lanes_tsv(include_str!(
        "../../types/tests/fixtures/frontend-contract-v1/legacy-lanes.tsv"
    ));
    let typed_lanes: Vec<AgentLaneRecord> = serde_json::from_str(include_str!(
        "../../types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
    ))
    .expect("typed lane fixture should parse");
    assert_eq!(legacy_lanes, typed_lanes);
    let reparsed: Vec<AgentLaneRecord> =
        serde_json::from_value(serde_json::to_value(&legacy_lanes).unwrap()).unwrap();
    assert_eq!(reparsed, typed_lanes);

    let legacy_cost = r#"{
        "provider_id": "fixture-provider",
        "model": "fixture-model",
        "scope": {"type": "task", "id": "task_migration"},
        "input_tokens": 10,
        "output_tokens": 5,
        "cached_input_tokens": 2,
        "total_tokens": 15,
        "estimated_cost_micro_usd": 42,
        "actual_cost_micro_usd": null,
        "request_id": "request_migration",
        "attempt_index": 1,
        "outcome": "success",
        "recorded_at": 1700000000
    }"#;
    let first_cost: CostUsageRecord = serde_json::from_str(legacy_cost).unwrap();
    let second_cost: CostUsageRecord =
        serde_json::from_value(serde_json::to_value(&first_cost).unwrap()).unwrap();
    assert_eq!(first_cost, second_cost);
    assert_eq!(first_cost.actual_cost, None);

    let legacy_approval = r#"{"approved": false, "feedback": "deny mutation"}"#;
    let first_approval: ApprovalResponse = serde_json::from_str(legacy_approval).unwrap();
    let second_approval: ApprovalResponse =
        serde_json::from_value(serde_json::to_value(&first_approval).unwrap()).unwrap();
    assert_eq!(first_approval, second_approval);
    assert!(matches!(second_approval.decision, ApprovalDecision::Deny));
}

#[test]
#[ignore = "manual fixture refresh; normal tests validate committed JSON only"]
fn refresh_frontend_contract_v1_fixtures() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("typed-lanes.json"),
        serde_json::to_string_pretty(&typed_lanes_fixture()).unwrap() + "\n",
    )
    .unwrap();
    for fixture in build_fixtures() {
        let path = root.join(format!("{}.json", fixture.fixture_id));
        fs::write(path, serde_json::to_string_pretty(&fixture).unwrap() + "\n").unwrap();
    }
}

#[test]
#[ignore = "manual extension fixture refresh; normal tests validate committed JSON only"]
fn refresh_frontend_host_services_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = frontend_host_services_fixture();
    fs::write(
        root.join("frontend-host-services.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

#[test]
#[ignore = "manual interaction fixture refresh; normal tests validate committed JSON only"]
fn refresh_interaction_closed_loop_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = interaction_closed_loop_fixture();
    fs::write(
        root.join("interaction-closed-loop.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate should have workspace sibling crates")
        .join("types")
        .join(FIXTURE_DIR)
}

fn read_fixture(root: &Path, name: &str) -> FrontendContractFixture {
    let path = root.join(name);
    let raw = fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "failed to read frontend-contract-v1 fixture {}: {err}",
            path.display()
        )
    });
    assert_no_local_paths_or_secrets(name, &raw);
    serde_json::from_str(&raw).unwrap_or_else(|err| {
        panic!(
            "failed to parse frontend-contract-v1 fixture {}: {err}",
            path.display()
        )
    })
}

fn assert_fixture_identity(name: &str, fixture: &FrontendContractFixture) {
    assert_eq!(fixture.schema_version, FRONTEND_SCHEMA_V1, "{name}");
    assert_eq!(
        format!("{}.json", fixture.fixture_id),
        name,
        "{name} fixture_id must match file name"
    );
    assert_eq!(fixture.expected_view_sha256.len(), 64, "{name}");
    assert!(
        fixture
            .expected_view_sha256
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()),
        "{name} digest must be lowercase hex"
    );
}

fn assert_capabilities_are_sorted_unique_and_advertised(
    name: &str,
    fixture: &FrontendContractFixture,
) {
    assert!(
        fixture
            .required_capabilities
            .windows(2)
            .all(|pair| pair[0] < pair[1]),
        "{name} required capabilities must be sorted and unique"
    );
    let advertised = frontend_capabilities();
    for capability in &fixture.required_capabilities {
        assert!(
            advertised.contains(capability),
            "{name} requires unadvertised capability {}",
            capability.0
        );
    }
}

fn assert_cursors_are_contiguous(name: &str, fixture: &FrontendContractFixture) {
    assert!(!fixture.events.is_empty(), "{name} must be non-empty");
    let stream_id = format!("fixture:{}", fixture.fixture_id);
    for (index, envelope) in fixture.events.iter().enumerate() {
        let expected_sequence = index as u64 + 1;
        assert_eq!(envelope.schema_version, FRONTEND_SCHEMA_V1, "{name}");
        assert_eq!(envelope.cursor.stream_id, stream_id, "{name}");
        assert_eq!(envelope.cursor.sequence, expected_sequence, "{name}");
        if let RuntimeWireEvent::Known(event) = &envelope.event {
            assert_eq!(event.sequence, expected_sequence, "{name}");
        }
    }
    assert_eq!(
        fixture.expected_final_cursor,
        fixture.events.last().unwrap().cursor,
        "{name}"
    );
}

fn assert_no_local_paths_or_secrets(name: &str, raw: &str) {
    let forbidden = ["/Users/", "\\Users\\", "sk-", "OPENAI_API_KEY", "password"];
    for token in forbidden {
        assert!(
            !raw.contains(token),
            "{name} contains local path or secret marker `{token}`"
        );
    }
}

fn replay_fixture(fixture: &FrontendContractFixture) -> (RuntimeViewState, EventCursor, String) {
    let mut view = RuntimeViewState::new(fixture.initial_snapshot.clone());
    let mut cursor = EventCursor {
        stream_id: format!("fixture:{}", fixture.fixture_id),
        sequence: 0,
    };
    for envelope in &fixture.events {
        if let RuntimeWireEvent::Known(event) = &envelope.event {
            view.apply_event(event);
        }
        cursor = envelope.cursor.clone();
    }
    let digest = canonical_view_sha256(&view);
    (view, cursor, digest)
}

fn replay_fixture_after_gap(
    fixture: &FrontendContractFixture,
    delivered_before_gap: usize,
) -> (RuntimeViewState, EventCursor, String) {
    assert!(delivered_before_gap < fixture.events.len());
    let mut view = RuntimeViewState::new(fixture.initial_snapshot.clone());
    let mut cursor = EventCursor {
        stream_id: format!("fixture:{}", fixture.fixture_id),
        sequence: 0,
    };
    for envelope in fixture.events.iter().take(delivered_before_gap) {
        if let RuntimeWireEvent::Known(event) = &envelope.event {
            view.apply_event(event);
        }
        cursor = envelope.cursor.clone();
    }
    let gap = &fixture.events[delivered_before_gap + 1];
    assert!(gap.cursor.sequence > cursor.sequence + 1);

    // Reconnect asks Core for the missing contiguous batch and reduces those
    // ordered facts before accepting the already observed post-gap event.
    for envelope in fixture.events.iter().skip(delivered_before_gap) {
        assert_eq!(envelope.cursor.sequence, cursor.sequence + 1);
        if let RuntimeWireEvent::Known(event) = &envelope.event {
            view.apply_event(event);
        }
        cursor = envelope.cursor.clone();
    }
    let digest = canonical_view_sha256(&view);
    (view, cursor, digest)
}

fn canonical_view_sha256(view: &RuntimeViewState) -> String {
    let value = serde_json::to_value(view).expect("runtime view must serialize");
    let sorted = sort_json(value);
    let bytes = serde_json::to_vec(&sorted).expect("canonical json must serialize");
    format!("{:x}", Sha256::digest(bytes))
}

fn sort_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let sorted = map
                .into_iter()
                .map(|(key, value)| (key, sort_json(value)))
                .collect::<BTreeMap<_, _>>();
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(sort_json).collect())
        }
        other => other,
    }
}

fn assert_scenario_facts(name: &str, view: &RuntimeViewState) {
    match name {
        "stream-tool.json" => {
            assert_eq!(view.assistant_stream, "Checking repository");
            assert!(view.active_tool_calls.is_empty());
            assert!(
                view.latest_evidence
                    .iter()
                    .any(|evidence| evidence.id == "ev_tool_ok")
            );
        }
        "approval-allow-deny.json" => {
            assert!(view.pending_approvals.is_empty());
            assert_eq!(view.errors.len(), 1);
            assert!(view.errors[0].message.contains("denied"));
        }
        "queued-follow-up.json" => {
            assert!(view.queued_inputs.is_empty());
            assert_eq!(view.assistant_stream, "working");
        }
        "dag-blocker.json" => {
            assert!(
                view.agent_dags
                    .iter()
                    .any(|dag| dag.status == AgentDagStatus::Blocked)
            );
            assert!(
                view.tasks
                    .iter()
                    .any(|task| task.status == AgentTaskStatus::Blocked)
            );
            assert!(view.tasks.iter().any(|task| {
                task.next_action
                    .as_ref()
                    .is_some_and(|action| action.label == "Retry blocker")
            }));
        }
        "multi-lane.json" => {
            assert_eq!(view.lanes.len(), 2);
            assert!(view.lanes.iter().any(|lane| lane.role == AgentRole::Coder));
            assert!(
                view.lanes
                    .iter()
                    .any(|lane| lane.role == AgentRole::Reviewer)
            );
        }
        "merge-gate.json" => {
            assert!(
                view.latest_evidence
                    .iter()
                    .any(|evidence| evidence.kind == "review")
            );
            assert!(
                view.merge_gates
                    .iter()
                    .any(|gate| gate.status == MergeGateStatus::Accepted)
            );
        }
        "context-pressure-cost-blind.json" => {
            let context = view.context.as_ref().expect("context must be projected");
            assert_eq!(context.pressure_percent(), 92);
            assert_eq!(view.cost_ledger.total_actual_cost_micro_usd, None);
        }
        "plan-denial.json" => {
            assert_eq!(view.snapshot.work_mode, WorkMode::Plan);
            assert!(
                view.errors
                    .iter()
                    .any(|error| error.message.contains("Plan mode"))
            );
            assert!(view.latest_evidence.is_empty());
        }
        "d1-vertical-slice.json" => {
            assert_eq!(view.assistant_stream, "D1 cockpit state");
            assert!(view.lanes.iter().any(|lane| lane.id == "lane_d1_core"));
            assert!(view.pending_approvals.is_empty());
            assert!(
                view.merge_gates
                    .iter()
                    .any(|gate| gate.gate_id == "gate_d1")
            );
            assert_eq!(view.token_cost.as_ref().unwrap().cost_micro_usd, None);
            assert_d1_preferences_are_snapshot_capabilities_not_view_fields(view);
        }
        other => panic!("unhandled fixture {other}"),
    }
}

fn assert_d1_preferences_are_snapshot_capabilities_not_view_fields(view: &RuntimeViewState) {
    assert_eq!(
        view.snapshot.ui_preferences,
        ResolvedUiPreferences {
            locale: viden_types::LocaleId::ZhCn,
            skin: UiSkin::Aurora,
            mode: UiColorMode::Dark,
            density: UiDensity::Regular,
            motion: UiMotion::Reduced,
            diagnostics: Vec::new(),
        }
    );
    let config_summary: serde_json::Value =
        serde_json::from_str(&view.snapshot.config_summary).unwrap();
    assert_eq!(
        config_summary["ui"]["effective"],
        serde_json::json!({
            "locale": "zh-CN",
            "skin": "aurora",
            "mode": "dark",
            "density": "regular",
            "motion": "reduced"
        })
    );
    assert_eq!(
        config_summary["design_entry"]["hierarchy"],
        serde_json::json!([
            "docs/viden-design/Viden/index.html",
            "client design index",
            "component library",
            "TUI unified prototype or GUI D1 desktop cockpit"
        ])
    );
    assert_eq!(
        config_summary["design_entry"]["d11"],
        "onboarding subordinate"
    );
}

fn parse_legacy_lanes_tsv(raw: &str) -> Vec<AgentLaneRecord> {
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let parts = line.split('\t').collect::<Vec<_>>();
            assert_eq!(parts.len(), 7, "legacy lane row should have seven columns");
            let stable_id = parts[0].trim_start_matches("L-").replace('-', "_");
            let legacy = serde_json::json!({
                "id": parts[0],
                "task_id": format!("task_{stable_id}"),
                "agent": parts[1],
                "screen": parts[2],
                "transport": parts[4],
                "status": parts[3],
                "summary": parts[6],
                "evidence": [format!("evidence_{stable_id}")],
            });
            serde_json::from_value(legacy).unwrap()
        })
        .collect()
}

fn build_fixtures() -> Vec<FrontendContractFixtureOut> {
    vec![
        fixture(
            "stream-tool",
            &["runtime.events", "runtime.evidence", "runtime.snapshot"],
            snapshot(WorkMode::Build),
            envelopes(
                "stream-tool",
                vec![
                    RuntimeEventKindExt::Assistant("msg_stream", "Checking "),
                    RuntimeEventKindExt::ToolStarted("tool_rg", "rg", "rg TODO"),
                    RuntimeEventKindExt::ToolFinished(
                        "tool_rg",
                        "rg",
                        true,
                        Some(evidence("ev_tool_ok", "tool_log", "rg completed")),
                    ),
                    RuntimeEventKindExt::Assistant("msg_stream", "repository"),
                ],
            ),
        ),
        fixture(
            "approval-allow-deny",
            &[
                "runtime.approvals",
                "runtime.commands",
                "runtime.events",
                "runtime.snapshot",
            ],
            snapshot(WorkMode::Build),
            envelopes(
                "approval-allow-deny",
                vec![
                    RuntimeEventKindExt::ApprovalRequested("approval_allow", "Edit docs", true),
                    RuntimeEventKindExt::ApprovalResolved("approval_allow", true),
                    RuntimeEventKindExt::ApprovalRequested("approval_deny", "Delete file", true),
                    RuntimeEventKindExt::ApprovalResolved("approval_deny", false),
                    RuntimeEventKindExt::Error("approval denied before effect execution", true),
                ],
            ),
        ),
        fixture(
            "queued-follow-up",
            &["runtime.events", "runtime.queued_input", "runtime.snapshot"],
            snapshot(WorkMode::Build),
            envelopes(
                "queued-follow-up",
                vec![
                    RuntimeEventKindExt::Assistant("msg_queue", "working"),
                    RuntimeEventKindExt::InputQueued("input_continue", "continue with tests"),
                    RuntimeEventKindExt::InputDequeued("input_continue"),
                ],
            ),
        ),
        fixture(
            "dag-blocker",
            &[
                "runtime.agent_dag",
                "runtime.events",
                "runtime.snapshot",
                "runtime.typed_tasks",
            ],
            snapshot(WorkMode::Build),
            envelopes(
                "dag-blocker",
                vec![
                    RuntimeEventKindExt::Dag("dag_blocker", AgentDagStatus::Blocked),
                    RuntimeEventKindExt::Task(
                        "task_blocked",
                        AgentRole::Coder,
                        AgentTaskStatus::Blocked,
                        "dependency missing",
                        Some("Retry blocker"),
                    ),
                    RuntimeEventKindExt::Error("blocked by task_dependency_missing", true),
                ],
            ),
        ),
        fixture(
            "multi-lane",
            &[
                "runtime.events",
                "runtime.snapshot",
                "runtime.typed_lanes",
                "runtime.typed_tasks",
            ],
            snapshot(WorkMode::Build),
            envelopes(
                "multi-lane",
                vec![
                    RuntimeEventKindExt::Lane(
                        "lane_core",
                        "task_core",
                        AgentRole::Coder,
                        AgentRoute::Terminal,
                        LaneStatus::Running,
                        ExecutionTarget::Local,
                    ),
                    RuntimeEventKindExt::Lane(
                        "lane_review",
                        "task_review",
                        AgentRole::Reviewer,
                        AgentRoute::Acp,
                        LaneStatus::WaitingApproval,
                        ExecutionTarget::Ssh {
                            host: "review.example.test".to_string(),
                        },
                    ),
                    RuntimeEventKindExt::Task(
                        "task_core",
                        AgentRole::Coder,
                        AgentTaskStatus::Running,
                        "core coding",
                        None,
                    ),
                    RuntimeEventKindExt::Task(
                        "task_review",
                        AgentRole::Reviewer,
                        AgentTaskStatus::WaitingApproval,
                        "review queue",
                        None,
                    ),
                ],
            ),
        ),
        fixture(
            "merge-gate",
            &[
                "runtime.events",
                "runtime.evidence",
                "runtime.merge_gate",
                "runtime.snapshot",
            ],
            snapshot(WorkMode::Build),
            envelopes(
                "merge-gate",
                vec![
                    RuntimeEventKindExt::Evidence(evidence("ev_patch", "patch", "patch applied")),
                    RuntimeEventKindExt::Evidence(evidence(
                        "ev_test",
                        "test_result",
                        "tests passed",
                    )),
                    RuntimeEventKindExt::Evidence(evidence("ev_review", "review", "review passed")),
                    RuntimeEventKindExt::MergeGate(
                        "gate_merge",
                        "task_merge",
                        MergeGateStatus::Accepted,
                        vec!["ev_patch", "ev_test", "ev_review"],
                    ),
                ],
            ),
        ),
        fixture(
            "context-pressure-cost-blind",
            &[
                "runtime.context",
                "runtime.cost",
                "runtime.events",
                "runtime.snapshot",
            ],
            snapshot(WorkMode::Build),
            envelopes(
                "context-pressure-cost-blind",
                vec![
                    RuntimeEventKindExt::ContextPressure,
                    RuntimeEventKindExt::CostUnknown,
                ],
            ),
        ),
        fixture(
            "plan-denial",
            &["runtime.commands", "runtime.events", "runtime.snapshot"],
            snapshot(WorkMode::Plan),
            envelopes(
                "plan-denial",
                vec![
                    RuntimeEventKindExt::CommandRejected(
                        "cmd_mutate",
                        "Plan mode blocks file/shell/Git/workflow/memory/task mutations before execution",
                    ),
                    RuntimeEventKindExt::Error("Plan mode denied mutation before execution", true),
                ],
            ),
        ),
        fixture(
            "d1-vertical-slice",
            &[
                "runtime.agent_dag",
                "runtime.approvals",
                "runtime.context",
                "runtime.cost",
                "runtime.events",
                "runtime.evidence",
                "runtime.merge_gate",
                "runtime.typed_lanes",
                "runtime.typed_tasks",
                "ui.preferences",
            ],
            snapshot(WorkMode::Build),
            envelopes(
                "d1-vertical-slice",
                vec![
                    RuntimeEventKindExt::Assistant("msg_d1", "D1 cockpit state"),
                    RuntimeEventKindExt::ToolStarted(
                        "tool_d1",
                        "cargo",
                        "cargo test -p viden-core",
                    ),
                    RuntimeEventKindExt::ToolFinished(
                        "tool_d1",
                        "cargo",
                        true,
                        Some(evidence("ev_d1_test", "test_result", "core tests passed")),
                    ),
                    RuntimeEventKindExt::Lane(
                        "lane_d1_core",
                        "task_d1_core",
                        AgentRole::Coder,
                        AgentRoute::Terminal,
                        LaneStatus::Running,
                        ExecutionTarget::Local,
                    ),
                    RuntimeEventKindExt::Task(
                        "task_d1_core",
                        AgentRole::Coder,
                        AgentTaskStatus::Running,
                        "Core contract freeze",
                        Some("Run parity fixtures"),
                    ),
                    RuntimeEventKindExt::ApprovalRequested(
                        "approval_d1",
                        "Allow fixture write",
                        true,
                    ),
                    RuntimeEventKindExt::ApprovalResolved("approval_d1", true),
                    RuntimeEventKindExt::Evidence(evidence(
                        "ev_d1_review",
                        "review",
                        "contract review passed",
                    )),
                    RuntimeEventKindExt::MergeGate(
                        "gate_d1",
                        "task_d1_core",
                        MergeGateStatus::CollectingEvidence,
                        vec!["ev_d1_test", "ev_d1_review"],
                    ),
                    RuntimeEventKindExt::ContextPressure,
                    RuntimeEventKindExt::CostUnknown,
                    RuntimeEventKindExt::Error(
                        "recovered from missing optional GUI panel state",
                        true,
                    ),
                    RuntimeEventKindExt::TokenCostUnknown,
                ],
            ),
        ),
    ]
}

/// Canonical proof of the `ReviewRequestStatus` transition a review decision
/// publishes (GUI-CORE-011).
///
/// The stream is exactly what `DecideReview` emits: the settled review fact
/// followed by the gate whose independent validator the accepted verdict
/// stamped. Frontends read the transition from these facts, never from the
/// gate decision text.
fn review_decision_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "review-decision";
    let requester = RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden".to_string(),
        lane_id: Some("lane_review_origin".to_string()),
        session_id: Some("session_review_origin".to_string()),
        task_id: Some("task_review_decision".to_string()),
        turn_id: None,
    };
    let reviewer = RuntimeOwner {
        lane_id: Some("lane_review_independent".to_string()),
        session_id: None,
        ..requester.clone()
    };
    let patch = evidence("ev_review_patch", "patch", "canonical patch under review");
    let binding = viden_types::ReviewedEvidenceBinding {
        evidence_id: patch.id.clone(),
        source_hash: "a1".repeat(32),
    };
    let pending_review = viden_types::ReviewRequestRecord {
        review_id: "review_decision".to_string(),
        gate_id: "gate_review_decision".to_string(),
        task_id: "task_review_decision".to_string(),
        requester_lane_id: "lane_review_origin".to_string(),
        reviewer_lane_id: "lane_review_independent".to_string(),
        owner: requester.clone(),
        evidence_ids: vec![patch.id.clone()],
        evidence_bindings: vec![binding.clone()],
        status: viden_types::ReviewRequestStatus::Pending,
        feedback: None,
        audit_id: "audit_review_requested".to_string(),
        updated_at: 1_700_000_101,
    };
    let decided_review = viden_types::ReviewRequestRecord {
        status: viden_types::ReviewRequestStatus::Accepted,
        feedback: Some("review.feedback.evidence_matches_request".to_string()),
        audit_id: "audit_review_decided".to_string(),
        updated_at: 1_700_000_102,
        ..pending_review.clone()
    };
    let validator = MergeGateValidator {
        owner: reviewer,
        review_request_id: pending_review.review_id.clone(),
        independent: true,
        validated_at: None,
    };
    let collecting_gate = MergeGateRecord {
        gate_id: "gate_review_decision".to_string(),
        task_id: "task_review_decision".to_string(),
        status: MergeGateStatus::CollectingEvidence,
        required_evidence: vec!["patch".to_string()],
        evidence_ids: vec![patch.id.clone()],
        gate_type: MergeGateType::Review,
        owner: requester.clone(),
        validator: Some(validator.clone()),
        policy_snapshot: MergeGatePolicySnapshot {
            required_evidence: vec!["patch".to_string()],
            permission_snapshot_id: Some("permission_review_decision".to_string()),
            requires_independent_validator: true,
            captured_at: Some(1_700_000_101),
        },
        decision: Some(MergeGateDecision {
            outcome: MergeGateDecisionOutcome::AwaitingEvidence,
            reason: "independent_review_required".to_string(),
            owner: requester,
            evidence_ids: vec![patch.id.clone()],
            reviewed_evidence: vec![binding],
            review_request_id: Some(pending_review.review_id.clone()),
            audit_id: "audit_review_requested".to_string(),
            decided_at: 1_700_000_101,
        }),
        conflict: None,
        applied_change_id: None,
        recovery_snapshot: None,
        audit_ids: vec!["audit_review_requested".to_string()],
        updated_at: Some(1_700_000_101),
    };
    let validated_gate = MergeGateRecord {
        validator: Some(MergeGateValidator {
            validated_at: Some(1_700_000_102),
            ..validator
        }),
        audit_ids: vec![
            "audit_review_requested".to_string(),
            "audit_review_decided".to_string(),
        ],
        updated_at: Some(1_700_000_102),
        ..collecting_gate.clone()
    };

    let kinds = vec![
        RuntimeEventKind::EvidenceRecorded { evidence: patch },
        RuntimeEventKind::MergeGateUpdated {
            gate: collecting_gate,
        },
        RuntimeEventKind::ReviewRequestUpdated {
            review: pending_review,
        },
        RuntimeEventKind::ReviewRequestUpdated {
            review: decided_review,
        },
        RuntimeEventKind::MergeGateUpdated {
            gate: validated_gate,
        },
    ];
    let events = kinds
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            let sequence = index as u64 + 1;
            RuntimeEventEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                owner: RuntimeOwner {
                    workspace_id: "workspace_contract_v1".to_string(),
                    project_id: "project_viden".to_string(),
                    lane_id: None,
                    session_id: Some(format!("session_{fixture_id}")),
                    task_id: None,
                    turn_id: Some(format!("turn_{fixture_id}")),
                },
                cursor: EventCursor {
                    stream_id: format!("fixture:{fixture_id}"),
                    sequence,
                },
                event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
                    sequence,
                    Some(1_700_000_100 + sequence),
                    kind,
                )),
            }
        })
        .collect();

    fixture(
        fixture_id,
        &[
            "runtime.events",
            "runtime.evidence",
            "runtime.merge_gate",
            "runtime.snapshot",
            "runtime.trust_loop",
        ],
        snapshot(WorkMode::Build),
        events,
    )
}

/// Canonical D1 proof that two concurrent Lanes carry distinct task-scoped
/// context budgets (GUI-CORE-008).
///
/// The stream is exactly what two Lanes under context pressure publish: each
/// Lane's typed record, the exact runtime owner Core bound to it, and the
/// budget for that owner's task. `ContextBudgetExceeded` is the production
/// carrier for both soft pressure (`exceeded: false`) and a breached hard
/// limit, so one Lane of each is present.
fn context_budgets_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "context-budgets";
    let lane_owner = |lane: &str, task: &str, session: &str| RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden".to_string(),
        lane_id: Some(lane.to_string()),
        session_id: Some(session.to_string()),
        task_id: Some(task.to_string()),
        turn_id: None,
    };
    let lane = |id: &str, task: &str, session: &str, summary: &str| AgentLaneRecord {
        id: id.to_string(),
        task_id: Some(task.to_string()),
        role: AgentRole::Coder,
        route: AgentRoute::BuiltIn,
        gate_strength: GateStrength::Full,
        mutation_policy: MutationPolicy::ProposeOnly,
        worktree: Some(format!("workspace/.worktrees/{id}")),
        branch: Some(format!("codex/{id}")),
        target: ExecutionTarget::Local,
        data_egress: viden_types::DataEgressPolicy::Deny,
        status: LaneStatus::Running,
        budget: LaneBudget::default(),
        active_session_ids: vec![session.to_string()],
        summary: summary.to_string(),
        evidence: Vec::new(),
        run_stats: None,
    };
    let budget = |bundle: &str,
                  task: &str,
                  soft: u64,
                  hard: u64,
                  used: u64,
                  updated_at: u64|
     -> ContextBudgetRecord {
        ContextBudgetRecord {
            budget_id: format!("ctxbudget-{bundle}"),
            scope: ContextScope::Task(task.to_string()),
            soft_token_limit: soft,
            hard_token_limit: hard,
            used_tokens: used,
            remaining_tokens: hard.saturating_sub(used),
            exceeded: used > hard,
            updated_at: Some(updated_at),
        }
    };

    let alpha_owner = lane_owner(
        "lane_context_alpha",
        "task_context_alpha",
        "session_context_alpha",
    );
    let beta_owner = lane_owner(
        "lane_context_beta",
        "task_context_beta",
        "session_context_beta",
    );

    let kinds = vec![
        RuntimeEventKind::LaneUpdated {
            lane: lane(
                "lane_context_alpha",
                "task_context_alpha",
                "session_context_alpha",
                "lane.context.alpha.running",
            ),
        },
        RuntimeEventKind::LaneRuntimeOwnerBound {
            binding: LaneRuntimeOwnerBinding {
                lane_id: "lane_context_alpha".to_string(),
                owner: alpha_owner.clone(),
            },
        },
        RuntimeEventKind::LaneUpdated {
            lane: lane(
                "lane_context_beta",
                "task_context_beta",
                "session_context_beta",
                "lane.context.beta.running",
            ),
        },
        RuntimeEventKind::LaneRuntimeOwnerBound {
            binding: LaneRuntimeOwnerBinding {
                lane_id: "lane_context_beta".to_string(),
                owner: beta_owner.clone(),
            },
        },
        RuntimeEventKind::ContextBudgetExceeded {
            budget: budget(
                "bundle_context_alpha",
                "task_context_alpha",
                48_000,
                80_000,
                52_000,
                1_700_000_201,
            ),
        },
        RuntimeEventKind::ContextBudgetExceeded {
            budget: budget(
                "bundle_context_beta",
                "task_context_beta",
                24_000,
                40_000,
                41_000,
                1_700_000_202,
            ),
        },
    ];
    let owners = [
        alpha_owner.clone(),
        alpha_owner,
        beta_owner.clone(),
        beta_owner.clone(),
        // The budget facts belong to the same owners that were just bound; a
        // Lane never publishes a budget under another Lane's owner.
        lane_owner(
            "lane_context_alpha",
            "task_context_alpha",
            "session_context_alpha",
        ),
        beta_owner,
    ];
    let events = kinds
        .into_iter()
        .zip(owners)
        .enumerate()
        .map(|(index, (kind, owner))| {
            let sequence = index as u64 + 1;
            RuntimeEventEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                owner,
                cursor: EventCursor {
                    stream_id: format!("fixture:{fixture_id}"),
                    sequence,
                },
                event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
                    sequence,
                    Some(1_700_000_200 + sequence),
                    kind,
                )),
            }
        })
        .collect();

    fixture(
        fixture_id,
        &[
            "runtime.context",
            "runtime.events",
            "runtime.lane_lifecycle",
            "runtime.lane_owner_projection",
            "runtime.snapshot",
            "runtime.typed_lanes",
        ],
        snapshot(WorkMode::Build),
        events,
    )
}

/// Canonical proof that ordered `AssistantDelta` chunks reconstruct exactly the
/// final Agent message (GUI-CORE-016).
///
/// The producer keeps one message id for the whole prompt turn, so the reply
/// grows as a single owner-scoped conversation message. The terminal marker is
/// the completion fact carrying the same finished text; reducing it settles the
/// turn without appending a duplicate paragraph.
fn streamed_turn_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "streamed-turn";
    let owner = RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden".to_string(),
        lane_id: Some("lane_streamed_turn".to_string()),
        session_id: Some("session_streamed_turn".to_string()),
        task_id: Some("task_streamed_turn".to_string()),
        turn_id: Some("turn_streamed_turn".to_string()),
    };
    let chunks = [
        "Read the reducer, ",
        "found the duplicate branch, ",
        "and covered it with a replay test.",
    ];
    let reply = chunks.concat();
    let session = AgentSessionView {
        session_id: "session_streamed_turn".to_string(),
        lane_id: "lane_streamed_turn".to_string(),
        agent_id: "codex-acp".to_string(),
        model: Some("gpt-5".to_string()),
        status: AgentSessionStatus::Running,
        owner: owner.clone(),
        task: "task.streamed_turn.investigate".to_string(),
        diagnostic: None,
        output: None,
    };

    let mut kinds = vec![RuntimeEventKind::AgentSessionStarted {
        session: session.clone(),
    }];
    kinds.extend(chunks.iter().map(|chunk| RuntimeEventKind::AssistantDelta {
        message_id: "message_streamed_turn_reply".to_string(),
        task_id: Some("task_streamed_turn".to_string()),
        session_id: Some("session_streamed_turn".to_string()),
        content: (*chunk).to_string(),
    }));
    kinds.push(RuntimeEventKind::AgentSessionCompleted {
        session: AgentSessionView {
            status: AgentSessionStatus::Completed,
            output: Some(reply),
            ..session
        },
    });

    fixture(
        fixture_id,
        &[
            "runtime.agent_conversation",
            "runtime.agent_sessions",
            "runtime.events",
            "runtime.snapshot",
        ],
        snapshot(WorkMode::Build),
        owned_envelopes(fixture_id, owner, kinds, 1_700_000_300),
    )
}

/// Canonical proof that an ACP turn returning an image alongside text publishes
/// typed content parts (GUI-CORE-017).
///
/// The image travels as an immutable reference into the Agent parts directory,
/// named by the content digest; inline bytes never reach the wire. A second
/// message in the same session proves that a part attaches to the message its
/// event named, and an unmodeled kind is preserved verbatim.
fn message_parts_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "message-parts";
    let owner = RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden".to_string(),
        lane_id: Some("lane_message_parts".to_string()),
        session_id: Some("session_message_parts".to_string()),
        task_id: Some("task_message_parts".to_string()),
        turn_id: Some("turn_message_parts".to_string()),
    };
    let session = AgentSessionView {
        session_id: "session_message_parts".to_string(),
        lane_id: "lane_message_parts".to_string(),
        agent_id: "claude-acp".to_string(),
        model: Some("claude-sonnet".to_string()),
        status: AgentSessionStatus::Running,
        owner: owner.clone(),
        task: "task.message_parts.render_chart".to_string(),
        diagnostic: None,
        output: None,
    };
    let follow_up = "Nothing else changed.";
    let delta = |message_id: &str, content: &str| RuntimeEventKind::AssistantDelta {
        message_id: message_id.to_string(),
        task_id: Some("task_message_parts".to_string()),
        session_id: Some("session_message_parts".to_string()),
        content: content.to_string(),
    };
    let part = |part: AgentContentPart| RuntimeEventKind::AgentMessagePart {
        session_id: "session_message_parts".to_string(),
        message_id: "message_parts_reply".to_string(),
        part,
    };

    let kinds = vec![
        RuntimeEventKind::AgentSessionStarted {
            session: session.clone(),
        },
        delta("message_parts_reply", "Rendered the coverage chart."),
        part(AgentContentPart::Image {
            media_type: "image/png".to_string(),
            reference: format!(".viden/agents/parts/{}.png", "7c".repeat(32)),
            alt: Some("message.part.coverage_chart".to_string()),
        }),
        // A kind this build does not model. Core keeps the exact published
        // object so a newer producer never loses content on an older client.
        part(AgentContentPart::Unknown {
            kind: "audio".to_string(),
            payload: serde_json::json!({
                "type": "audio",
                "mediaType": "audio/wav",
                "reference": format!(".viden/agents/parts/{}.wav", "3d".repeat(32)),
            }),
        }),
        delta("message_parts_follow_up", follow_up),
        RuntimeEventKind::AgentSessionCompleted {
            session: AgentSessionView {
                status: AgentSessionStatus::Completed,
                output: Some(follow_up.to_string()),
                ..session
            },
        },
    ];

    fixture(
        fixture_id,
        &[
            "runtime.agent_conversation",
            "runtime.agent_sessions",
            "runtime.events",
            "runtime.snapshot",
        ],
        snapshot(WorkMode::Build),
        owned_envelopes(fixture_id, owner, kinds, 1_700_000_400),
    )
}

/// Canonical proof that an audit page is attributable to the exact read that
/// asked for it, and that a server-side filter is applied before paging
/// (GUI-CORE-024).
///
/// Two reads are accepted before either is answered, and the pages come back in
/// the opposite order, so arrival order cannot stand in for correlation: only
/// the command id on each page tells them apart. A third read filters to agent
/// actors and comes back `complete` while strictly older operator and system
/// records are still visible on the unfiltered pages — the fact a client-side
/// filter could never establish.
fn audit_reads_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "audit-reads";
    let owner = RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden".to_string(),
        lane_id: Some("lane_audit_reads".to_string()),
        session_id: Some("session_audit_reads".to_string()),
        task_id: Some("task_audit_reads".to_string()),
        turn_id: Some("turn_audit_reads".to_string()),
    };
    let audit_record = |audit_id: &str, timestamp: u64, actor: AuditActor, action: &str| {
        AuditRecord::sanitized(
            audit_id.to_string(),
            timestamp,
            owner.clone(),
            actor,
            action.to_string(),
            vec![AuditObjectRef::new(
                AuditObjectRef::KIND_LANE,
                "lane_audit_reads",
            )],
            AuditOutcome::Success,
            BTreeMap::from([("outcome".to_string(), "accepted".to_string())]),
        )
        .expect("fixture audit records must satisfy the sanitization bounds")
    };
    let operator_gate = audit_record(
        "audit_operator_gate",
        1_700_000_100,
        AuditActor::Operator,
        "gate.decided",
    );
    let system_probe = audit_record(
        "audit_system_probe",
        1_700_000_200,
        AuditActor::System,
        "project.probed",
    );
    let agent_handoff = audit_record(
        "audit_agent_handoff",
        1_700_000_300,
        AuditActor::Agent {
            agent_id: "lane_audit_reads_coder".to_string(),
        },
        "handoff.created",
    );

    let unfiltered = |limit: u32| AuditQuery {
        limit,
        ..AuditQuery::default()
    };
    let accepted = |command_id: &str, query: AuditQuery| RuntimeEventKind::CommandAccepted {
        command_id: command_id.to_string(),
        command: RuntimeCommand::QueryAudit { query },
    };
    let loaded = |command_id: &str, page: AuditPage| RuntimeEventKind::AuditPageLoaded {
        command_id: Some(command_id.to_string()),
        page,
    };
    let agent_filter = AuditQuery {
        actor: Some(AuditActorFilter::AnyAgent),
        limit: 2,
        ..AuditQuery::default()
    };

    let kinds = vec![
        // Both reads are outstanding before either is answered.
        accepted("audit_read_first", unfiltered(2)),
        accepted("audit_read_second", unfiltered(3)),
        // The second read is answered first, so a client correlating by
        // arrival order would attribute this page to the first read.
        loaded(
            "audit_read_second",
            AuditPage {
                records: vec![
                    agent_handoff.clone(),
                    system_probe.clone(),
                    operator_gate.clone(),
                ],
                next_before: None,
                complete: true,
            },
        ),
        loaded(
            "audit_read_first",
            AuditPage {
                records: vec![agent_handoff.clone(), system_probe.clone()],
                next_before: Some(system_probe.cursor()),
                complete: false,
            },
        ),
        // The filtered read: `complete` describes the agent timeline, not the
        // project timeline the two reads above just published in full.
        accepted("audit_read_agents", agent_filter),
        loaded(
            "audit_read_agents",
            AuditPage {
                records: vec![agent_handoff],
                next_before: None,
                complete: true,
            },
        ),
    ];

    fixture(
        fixture_id,
        &[
            "runtime.audit",
            "runtime.commands",
            "runtime.events",
            "runtime.snapshot",
        ],
        snapshot(WorkMode::Plan),
        owned_envelopes(fixture_id, owner, kinds, 1_700_000_500),
    )
}

/// Canonical proof that the audit timeline is one newest-first order across
/// projects rather than a per-project list (GUI-CORE-014).
///
/// D10's ticker is a bounded page over every project in the workspace, so the
/// order has to be total. Two projects interleave here, and one pair of records
/// shares a timestamp across the project boundary so the `audit_id` tiebreak is
/// exercised exactly where a per-project ordering would diverge. This is a
/// separate fixture rather than an edit to `audit-reads`, whose three records
/// all sit in one project and whose digest is already registered.
fn audit_ordering_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "audit-ordering";
    // The read is workspace-scoped: the ticker spans projects, so the query
    // owner names no single project.
    let read_owner = RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: String::new(),
        lane_id: None,
        session_id: None,
        task_id: None,
        turn_id: None,
    };
    let record = |audit_id: &str, timestamp: u64, project: &str, lane: &str, action: &str| {
        AuditRecord::sanitized(
            audit_id.to_string(),
            timestamp,
            RuntimeOwner {
                workspace_id: "workspace_contract_v1".to_string(),
                project_id: project.to_string(),
                lane_id: Some(lane.to_string()),
                session_id: None,
                task_id: None,
                turn_id: None,
            },
            AuditActor::Operator,
            action.to_string(),
            vec![AuditObjectRef::new(AuditObjectRef::KIND_LANE, lane)],
            AuditOutcome::Success,
            BTreeMap::from([("outcome".to_string(), "accepted".to_string())]),
        )
        .expect("fixture audit records must satisfy the sanitization bounds")
    };
    // Newest first by `(timestamp, audit_id)`. The first two share a timestamp
    // and sit in different projects, so only the descending audit id separates
    // them — and it does so across the project boundary.
    let ordered = vec![
        record(
            "audit_delta_review",
            1_700_000_200,
            "project_viden_docs",
            "lane_docs_writer",
            "review.decided",
        ),
        record(
            "audit_charlie_revert",
            1_700_000_200,
            "project_viden",
            "lane_core_runtime",
            "change.reverted",
        ),
        record(
            "audit_bravo_handoff",
            1_700_000_150,
            "project_viden_docs",
            "lane_docs_writer",
            "handoff.created",
        ),
        record(
            "audit_alpha_gate",
            1_700_000_100,
            "project_viden",
            "lane_core_runtime",
            "gate.decided",
        ),
    ];

    let kinds = vec![
        RuntimeEventKind::CommandAccepted {
            command_id: "audit_ticker_read".to_string(),
            command: RuntimeCommand::QueryAudit {
                query: AuditQuery {
                    // The ticker asks for one bounded page over the whole
                    // workspace: no project filter, so nothing is scoped away
                    // before the ordering is established.
                    limit: 50,
                    ..AuditQuery::default()
                },
            },
        },
        RuntimeEventKind::AuditPageLoaded {
            command_id: Some("audit_ticker_read".to_string()),
            page: AuditPage {
                records: ordered,
                next_before: None,
                complete: true,
            },
        },
    ];

    fixture(
        fixture_id,
        &[
            "runtime.audit",
            "runtime.commands",
            "runtime.events",
            "runtime.snapshot",
        ],
        snapshot(WorkMode::Build),
        owned_envelopes(fixture_id, read_owner, kinds, 1_700_000_800),
    )
}

/// Canonical proof of the structured diff contract at each site the 0.3.3
/// design names (GUI-CORE-012).
///
/// The scenario is deliberately not a happy path: one read is answered and one
/// is refused, and the answered page carries one entry with rows and one the
/// byte bound stripped. A client that treated an empty page, a bounded entry,
/// and a refusal as the same thing would render all three as "nothing
/// changed", which is the failure this capability exists to prevent.
fn structured_diff_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "structured-diff";
    let owner = RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden".to_string(),
        lane_id: Some("lane_structured_diff".to_string()),
        session_id: Some("session_structured_diff".to_string()),
        task_id: Some("task_structured_diff".to_string()),
        turn_id: Some("turn_structured_diff".to_string()),
    };
    let line = |kind: DiffLineKind, content: &str, old_line, new_line| DiffLine {
        kind,
        content: content.to_string(),
        old_line,
        new_line,
    };
    // The single-file preview: what `edit_file` would do to one file, computed
    // read-only against the bytes `base_sha256` names.
    let edit_document = DiffDocument {
        files: vec![DiffFile {
            path: "crates/types/src/diff.rs".to_string(),
            old_path: None,
            kind: WorkspaceChangeKind::Modified,
            binary: false,
            omitted: false,
            additions: 1,
            deletions: 1,
            hunks: vec![DiffHunk {
                old_start: 42,
                old_lines: 3,
                new_start: 42,
                new_lines: 3,
                header: Some("pub struct DiffDocument {".to_string()),
                lines: vec![
                    line(
                        DiffLineKind::Context,
                        "    pub files: Vec<DiffFile>,",
                        Some(42),
                        Some(42),
                    ),
                    line(
                        DiffLineKind::Removed,
                        "    pub truncated: bool,",
                        Some(43),
                        None,
                    ),
                    line(
                        DiffLineKind::Added,
                        "    pub truncated: bool, // bounded",
                        None,
                        Some(43),
                    ),
                    line(
                        DiffLineKind::Context,
                        "    pub byte_limit: u32,",
                        Some(44),
                        Some(44),
                    ),
                ],
            }],
        }],
        truncated: false,
        byte_limit: 65_536,
    };
    // The multi-file case: the trust loop's canonical patch, with no single
    // preimage to hash.
    let merge_document = DiffDocument {
        files: vec![
            DiffFile {
                path: "crates/runtime/src/frontend_services.rs".to_string(),
                old_path: None,
                kind: WorkspaceChangeKind::Modified,
                binary: false,
                omitted: false,
                additions: 1,
                deletions: 0,
                hunks: vec![DiffHunk {
                    old_start: 10,
                    old_lines: 1,
                    new_start: 10,
                    new_lines: 2,
                    header: None,
                    lines: vec![
                        line(
                            DiffLineKind::Context,
                            "use crate::SessionEngine;",
                            Some(10),
                            Some(10),
                        ),
                        line(
                            DiffLineKind::Added,
                            "use viden_tools::render_diff;",
                            None,
                            Some(11),
                        ),
                    ],
                }],
            },
            DiffFile {
                path: "crates/runtime/src/decision_context.rs".to_string(),
                old_path: None,
                kind: WorkspaceChangeKind::Added,
                binary: false,
                omitted: false,
                additions: 2,
                deletions: 0,
                hunks: vec![DiffHunk {
                    old_start: 0,
                    old_lines: 0,
                    new_start: 1,
                    new_lines: 2,
                    header: None,
                    lines: vec![
                        line(
                            DiffLineKind::Added,
                            "//! Approval decision context.",
                            None,
                            Some(1),
                        ),
                        line(
                            DiffLineKind::Added,
                            "pub(crate) fn tool_decision_context() {}",
                            None,
                            Some(2),
                        ),
                    ],
                }],
            },
        ],
        truncated: false,
        byte_limit: 65_536,
    };

    let approval =
        |id: &str, tool_name: &str, input_preview: &str, decision_context: DecisionContext| {
            ApprovalRequestView {
                id: id.to_string(),
                tool_name: tool_name.to_string(),
                title: format!("Approve {tool_name}"),
                message: format!("{tool_name} requires approval"),
                input_preview: input_preview.to_string(),
                is_mutating: true,
                reason: Some(format!("{tool_name} requires approval")),
                owner: owner.clone(),
                risk: ApprovalRisk::Medium,
                target: ApprovalTarget {
                    kind: tool_name.to_string(),
                    display: input_preview.to_string(),
                    canonical_ref: None,
                },
                allowed_scopes: vec![ApprovalScope::Once],
                policy_reason_key: "permission.requires_approval".to_string(),
                policy_reason_args: BTreeMap::new(),
                expires_at: 1_700_001_100,
                default_action: ApprovalDefaultAction::Deny,
                audit_id: format!("audit_{id}"),
                decision_context: Some(decision_context),
            }
        };

    let source = WorkspaceSourceView {
        status: viden_types::WorkspaceSourceStatus::Ready,
        branch: Some("codex/v3-core-runtime".to_string()),
        worktree: Some("workspace/viden".to_string()),
        ahead: 1,
        behind: 0,
        added: 2,
        deleted: 0,
        dirty: true,
    };
    let page = WorkspaceDiffPage {
        target: SourceTarget::Workspace,
        source,
        entries: vec![
            WorkspaceDiffEntry {
                path: "crates/types/src/diff.rs".to_string(),
                index: Some(WorkspaceChangeKind::Modified),
                worktree: None,
                staged: true,
                diff: Some(edit_document.files[0].clone()),
            },
            // Dropped by the bound, and still visible with real counts: a
            // reviewer must be able to tell "not shown" from "unchanged".
            WorkspaceDiffEntry {
                path: "crates/types/tests/fixtures/frontend-contract-v1/structured-diff.json"
                    .to_string(),
                index: None,
                worktree: Some(WorkspaceChangeKind::Added),
                staged: false,
                diff: Some(DiffFile {
                    path: "crates/types/tests/fixtures/frontend-contract-v1/structured-diff.json"
                        .to_string(),
                    old_path: None,
                    kind: WorkspaceChangeKind::Added,
                    binary: false,
                    omitted: true,
                    additions: 1_284,
                    deletions: 0,
                    hunks: Vec::new(),
                }),
            },
        ],
        truncated: true,
    };

    let kinds = vec![
        // Both reads are accepted before either is settled, so neither the
        // page nor the refusal can be attributed by arrival order.
        RuntimeEventKind::CommandAccepted {
            command_id: "structured_diff_read".to_string(),
            command: RuntimeCommand::QueryWorkspaceDiff {
                query: WorkspaceDiffQuery {
                    target: SourceTarget::Workspace,
                    scope: WorkspaceDiffScope::Both,
                    paths: vec!["crates/types".to_string()],
                    byte_limit: Some(4_096),
                },
            },
        },
        RuntimeEventKind::CommandAccepted {
            command_id: "structured_diff_refused".to_string(),
            command: RuntimeCommand::QueryWorkspaceDiff {
                query: WorkspaceDiffQuery {
                    target: SourceTarget::Lane {
                        lane_id: "lane_structured_diff".to_string(),
                    },
                    scope: WorkspaceDiffScope::Worktree,
                    paths: Vec::new(),
                    byte_limit: None,
                },
            },
        },
        RuntimeEventKind::ApprovalRequested {
            approval: approval(
                "approval_structured_edit",
                "edit_file",
                "path: crates/types/src/diff.rs",
                DecisionContext {
                    diff: Some(edit_document),
                    base_sha256: Some(
                        "3f79bb7b435b05321651daefd374cdc681dc06faa65e374e38337b88ca046dea"
                            .to_string(),
                    ),
                },
            ),
        },
        RuntimeEventKind::ApprovalRequested {
            approval: approval(
                "approval_structured_merge",
                "workflow_merge_agent_patch",
                "action: merge_agent_patch",
                DecisionContext {
                    diff: Some(merge_document),
                    base_sha256: None,
                },
            ),
        },
        RuntimeEventKind::WorkspaceDiffLoaded {
            command_id: "structured_diff_read".to_string(),
            page,
        },
        // The refusal answers the *other* read, names the gate, and carries
        // the actionable hint folded into the reason.
        RuntimeEventKind::CommandRejected {
            command_id: "structured_diff_refused".to_string(),
            reason: "permission denied\ntool: git_diff\nreason: DenyRule\nmessage: git_diff is \
                     denied by a workspace rule\nhint: grant the `git_diff` permission to read \
                     structured workspace changes"
                .to_string(),
        },
    ];

    fixture(
        fixture_id,
        &[
            "runtime.approvals",
            "runtime.commands",
            "runtime.events",
            "runtime.snapshot",
            "runtime.structured_diff",
        ],
        snapshot(WorkMode::Build),
        owned_envelopes(fixture_id, owner, kinds, 1_700_001_000),
    )
}

/// GUI-CORE-020: an operator source-control action refused, one completed, one
/// failed.
///
/// The scenario is deliberately not three happy paths. A policy refusal, a
/// granted commit, and a push that could not be tracked are three different
/// facts, and a client that rendered them alike would tell an operator "denied"
/// about a tracking problem and "done" about a push that never left the
/// machine. Outputs are fixed strings with no machine path in them, so the
/// bytes are identical on every machine that regenerates this fixture.
/// Canonical proof of the evidence archive read contract (GUI-CORE-025).
///
/// Deliberately not a happy path. Two pages tile one three-row archive through
/// an opaque cursor; a kind-filtered page is `complete` while the unfiltered
/// archive is not; one content read answers text, one answers a parsed diff,
/// one answers `Unavailable { SummaryOnly }` for display-only evidence; and an
/// over-limit query is refused by `CommandRejected` rather than answered with
/// an empty page. A client that treated a cut page, a filtered page, a
/// summary-only row, and a refusal as the same thing would render all four as
/// "no evidence", which is the fabricated absence this capability exists to
/// prevent.
fn evidence_reads_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "evidence-reads";
    let owner = RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden".to_string(),
        lane_id: Some("lane_evidence_reads".to_string()),
        session_id: Some("session_evidence_reads".to_string()),
        task_id: Some("task_evidence_reads".to_string()),
        turn_id: Some("turn_evidence_reads".to_string()),
    };
    let canonical = |item: &str, hash_seed: char| CanonicalEvidenceReference {
        item_id: item.to_string(),
        bundle_id: "bundle_evidence_reads".to_string(),
        source_hash: std::iter::repeat_n(hash_seed, 64).collect::<String>(),
        producer: EvidenceProducer {
            identity: "lane_evidence_reads".to_string(),
            role: "coder".to_string(),
            task_id: "task_evidence_reads".to_string(),
        },
        permission_snapshot_id: Some("permission-receipt-evidence-reads".to_string()),
        permission_scope: ContextScope::Task("task_evidence_reads".to_string()),
        evidence_scope: ContextScope::Task("task_evidence_reads".to_string()),
        verification: EvidenceVerificationState::Verified,
        quality: EvidenceQualityFacts {
            status: EvidenceQualityStatus::Pass,
            reason_codes: Vec::new(),
        },
    };
    let row = |id: &str,
               kind: &str,
               summary: &str,
               timestamp: u64,
               canonical: Option<CanonicalEvidenceReference>| EvidenceView {
        id: id.to_string(),
        kind: kind.to_string(),
        summary: summary.to_string(),
        path: None,
        source: Some("lane_evidence_reads".to_string()),
        canonical,
        metadata: None,
        timestamp: Some(timestamp),
        owner: Some(owner.clone()),
    };

    // Oldest first, which is the order the pages must publish.
    let patch = row(
        "evidence_alpha_patch",
        "patch",
        "canonical patch for the diff module",
        1_700_000_100,
        Some(canonical("item_evidence_alpha", 'a')),
    );
    let tests = row(
        "evidence_bravo_tests",
        "test_result",
        "workspace suite passed",
        1_700_000_200,
        Some(canonical("item_evidence_bravo", 'b')),
    );
    // Display-only: provider prose with no canonical reference, which the gate
    // already refuses as merge evidence and the read path answers as
    // `SummaryOnly` rather than as content.
    let summary = row(
        "evidence_charlie_summary",
        "task_summary",
        "the model's own account of the turn",
        1_700_000_300,
        None,
    );

    let cursor_after_tests = EvidenceCursor {
        timestamp: tests.timestamp,
        id: tests.id.clone(),
    }
    .encode();

    let accepted_query =
        |command_id: &str, query: EvidenceQuery| RuntimeEventKind::CommandAccepted {
            command_id: command_id.to_string(),
            command: RuntimeCommand::QueryEvidence { query },
        };
    let loaded = |command_id: &str, page: EvidencePage| RuntimeEventKind::EvidencePageLoaded {
        command_id: command_id.to_string(),
        page,
    };
    let accepted_content =
        |command_id: &str, evidence_id: &str| RuntimeEventKind::CommandAccepted {
            command_id: command_id.to_string(),
            command: RuntimeCommand::ReadEvidenceContent {
                evidence_id: evidence_id.to_string(),
            },
        };
    let content = |command_id: &str, evidence_id: &str, content: EvidenceContent| {
        RuntimeEventKind::EvidenceContentLoaded {
            command_id: command_id.to_string(),
            evidence_id: evidence_id.to_string(),
            content,
        }
    };

    let kinds = vec![
        // Page one of two: limit 2 over three rows, so it is not complete and
        // names where to resume.
        accepted_query(
            "evidence_read_first",
            EvidenceQuery {
                limit: 2,
                ..EvidenceQuery::default()
            },
        ),
        loaded(
            "evidence_read_first",
            EvidencePage {
                entries: vec![patch.clone(), tests.clone()],
                complete: false,
                next_after: Some(cursor_after_tests.clone()),
            },
        ),
        // Page two resumes from the opaque cursor page one published, verbatim.
        accepted_query(
            "evidence_read_second",
            EvidenceQuery {
                limit: 2,
                after: Some(cursor_after_tests),
                ..EvidenceQuery::default()
            },
        ),
        loaded(
            "evidence_read_second",
            EvidencePage {
                entries: vec![summary.clone()],
                complete: true,
                next_after: None,
            },
        ),
        // The filtered read is `complete` for the `patch` archive even though
        // two rows the unfiltered pages just published are older than nothing
        // it returned: `complete` describes the filter, not the archive.
        accepted_query(
            "evidence_read_patches",
            EvidenceQuery {
                kinds: vec!["patch".to_string()],
                limit: 2,
                ..EvidenceQuery::default()
            },
        ),
        loaded(
            "evidence_read_patches",
            EvidencePage {
                entries: vec![patch.clone()],
                complete: true,
                next_after: None,
            },
        ),
        // Content: bounded text for a non-patch row.
        accepted_content("evidence_content_tests", "evidence_bravo_tests"),
        content(
            "evidence_content_tests",
            "evidence_bravo_tests",
            EvidenceContent::Text {
                text: "running 3 tests\ntest result: ok. 3 passed; 0 failed\n".to_string(),
                truncated: false,
                sha256: std::iter::repeat_n('b', 64).collect::<String>(),
            },
        ),
        // Content: a `patch` row answers diff rows through the same parser the
        // structured diff capability uses, so "Open in review" renders one
        // shape rather than two.
        accepted_content("evidence_content_patch", "evidence_alpha_patch"),
        content(
            "evidence_content_patch",
            "evidence_alpha_patch",
            EvidenceContent::Diff {
                document: DiffDocument {
                    files: vec![DiffFile {
                        path: "crates/types/src/evidence_reads.rs".to_string(),
                        old_path: None,
                        kind: WorkspaceChangeKind::Modified,
                        binary: false,
                        omitted: false,
                        additions: 1,
                        deletions: 1,
                        hunks: vec![DiffHunk {
                            old_start: 12,
                            old_lines: 3,
                            new_start: 12,
                            new_lines: 3,
                            header: Some("impl EvidenceQuery".to_string()),
                            lines: vec![
                                DiffLine {
                                    kind: DiffLineKind::Context,
                                    content: "    pub fn clamped_limit(&self) -> usize {"
                                        .to_string(),
                                    old_line: Some(12),
                                    new_line: Some(12),
                                },
                                DiffLine {
                                    kind: DiffLineKind::Removed,
                                    content: "        self.limit as usize".to_string(),
                                    old_line: Some(13),
                                    new_line: None,
                                },
                                DiffLine {
                                    kind: DiffLineKind::Added,
                                    content: "        self.limit.clamp(1, 200) as usize"
                                        .to_string(),
                                    old_line: None,
                                    new_line: Some(13),
                                },
                                DiffLine {
                                    kind: DiffLineKind::Context,
                                    content: "    }".to_string(),
                                    old_line: Some(14),
                                    new_line: Some(14),
                                },
                            ],
                        }],
                    }],
                    truncated: false,
                    byte_limit: 256 * 1024,
                },
                sha256: std::iter::repeat_n('a', 64).collect::<String>(),
            },
        ),
        // Content: display-only evidence has no canonical bytes at all, which
        // is a stated fact rather than an empty body.
        accepted_content("evidence_content_summary", "evidence_charlie_summary"),
        content(
            "evidence_content_summary",
            "evidence_charlie_summary",
            EvidenceContent::Unavailable {
                reason: EvidenceUnavailableReason::SummaryOnly,
            },
        ),
        // The refusal: an over-limit `kinds` filter. `limit` itself is clamped
        // rather than refused, so this is the shape a client actually meets,
        // and it arrives as `CommandRejected` naming this exact read instead of
        // as an empty page a client would render as an empty archive.
        accepted_query(
            "evidence_read_overlimit",
            EvidenceQuery {
                kinds: (0..33).map(|index| format!("kind_{index}")).collect(),
                limit: 2,
                ..EvidenceQuery::default()
            },
        ),
        RuntimeEventKind::CommandRejected {
            command_id: "evidence_read_overlimit".to_string(),
            reason: "evidence query kinds exceed the 32 entry bound: 33 requested\nhint: ask for \
                     fewer kinds, or drop the filter and page the archive"
                .to_string(),
        },
    ];

    fixture(
        fixture_id,
        &[
            "runtime.commands",
            "runtime.events",
            "runtime.evidence_reads",
            "runtime.snapshot",
        ],
        snapshot(WorkMode::Build),
        owned_envelopes(fixture_id, owner, kinds, 1_700_000_900),
    )
}

fn operator_git_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "operator-git";
    let owner = RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden".to_string(),
        lane_id: Some("lane_operator_git".to_string()),
        session_id: Some("session_operator_git".to_string()),
        task_id: Some("task_operator_git".to_string()),
        turn_id: Some("turn_operator_git".to_string()),
    };

    // What the commit approval shows: the *index*, which is exactly the
    // content the commit will contain. No `base_sha256`, because one hash
    // cannot describe a multi-file staged change.
    let staged = DiffDocument {
        files: vec![DiffFile {
            path: "crates/types/src/source_control.rs".to_string(),
            old_path: None,
            kind: WorkspaceChangeKind::Added,
            binary: false,
            omitted: false,
            additions: 2,
            deletions: 0,
            hunks: vec![DiffHunk {
                old_start: 0,
                old_lines: 0,
                new_start: 1,
                new_lines: 2,
                header: None,
                lines: vec![
                    DiffLine {
                        kind: DiffLineKind::Added,
                        content: "//! Operator source-control actions.".to_string(),
                        old_line: None,
                        new_line: Some(1),
                    },
                    DiffLine {
                        kind: DiffLineKind::Added,
                        content: "pub enum OperatorGitAction {}".to_string(),
                        old_line: None,
                        new_line: Some(2),
                    },
                ],
            }],
        }],
        truncated: false,
        byte_limit: 65_536,
    };

    let before_commit = WorkspaceSourceView {
        status: viden_types::WorkspaceSourceStatus::Ready,
        branch: Some("codex/v3-core-runtime".to_string()),
        worktree: Some("workspace/viden".to_string()),
        ahead: 1,
        behind: 0,
        added: 2,
        deleted: 0,
        dirty: true,
    };
    // Resampled *after* the commit: the branch moved and the tree is clean.
    // A client reading these numbers is reading what the action did, not what
    // it was predicted to do.
    let after_commit = WorkspaceSourceView {
        ahead: 2,
        added: 0,
        dirty: false,
        ..before_commit.clone()
    };

    let kinds = vec![
        // All three commands are accepted before any is settled, so nothing can
        // be attributed by arrival order.
        RuntimeEventKind::CommandAccepted {
            command_id: "operator_git_stage_refused".to_string(),
            command: RuntimeCommand::RunOperatorGitAction {
                owner: owner.clone(),
                target: SourceTarget::Workspace,
                action: OperatorGitAction::Stage {
                    paths: vec!["crates/types/src/source_control.rs".to_string()],
                },
            },
        },
        RuntimeEventKind::CommandAccepted {
            command_id: "operator_git_commit".to_string(),
            command: RuntimeCommand::RunOperatorGitAction {
                owner: owner.clone(),
                target: SourceTarget::Workspace,
                action: OperatorGitAction::Commit {
                    message: "feat(types): add operator git actions".to_string(),
                },
            },
        },
        RuntimeEventKind::CommandAccepted {
            command_id: "operator_git_push".to_string(),
            command: RuntimeCommand::RunOperatorGitAction {
                owner: owner.clone(),
                target: SourceTarget::Lane {
                    lane_id: "lane_operator_git".to_string(),
                },
                action: OperatorGitAction::Push {
                    remote: Some("origin".to_string()),
                    set_upstream: false,
                },
            },
        },
        // The refusal happened before anything ran, names the mapped agent
        // spec the operator's rules govern, and carries the actionable hint.
        RuntimeEventKind::CommandRejected {
            command_id: "operator_git_stage_refused".to_string(),
            reason: "permission denied\ntool: git_add\nreason: DenyRule\nmessage: git_add is \
                     denied by a workspace rule\nhint: grant the `git_add` permission to run \
                     source-control actions from this client"
                .to_string(),
        },
        RuntimeEventKind::ApprovalRequested {
            approval: ApprovalRequestView {
                id: "approval_operator_git_commit".to_string(),
                tool_name: "git_commit".to_string(),
                title: "Approve git_commit".to_string(),
                message: "git_commit requires approval".to_string(),
                input_preview: "  message: feat(types): add operator git actions".to_string(),
                is_mutating: true,
                reason: Some("git_commit requires approval".to_string()),
                owner: owner.clone(),
                // Reversibility, not size: a commit moves HEAD but can still be
                // amended locally, which is what separates it from the push.
                risk: ApprovalRisk::Medium,
                target: ApprovalTarget {
                    kind: "git".to_string(),
                    display: "git_commit (workspace)".to_string(),
                    canonical_ref: Some("workspace".to_string()),
                },
                allowed_scopes: vec![ApprovalScope::Once],
                policy_reason_key: "permission.requires_approval".to_string(),
                policy_reason_args: BTreeMap::new(),
                expires_at: 1_700_002_100,
                default_action: ApprovalDefaultAction::Deny,
                audit_id: "audit_operator_git_commit".to_string(),
                decision_context: Some(DecisionContext {
                    diff: Some(staged),
                    base_sha256: None,
                }),
            },
        },
        RuntimeEventKind::ApprovalResolved {
            request_id: "approval_operator_git_commit".to_string(),
            decision: ApprovalDecision::Allow {
                scope: ApprovalScope::Once,
            },
            owner: owner.clone(),
            audit_id: "audit_operator_git_commit".to_string(),
        },
        RuntimeEventKind::OperatorGitActionFinished {
            command_id: "operator_git_commit".to_string(),
            target: SourceTarget::Workspace,
            action: OperatorGitAction::Commit {
                message: "feat(types): add operator git actions".to_string(),
            },
            outcome: OperatorGitOutcome::Completed {
                output: "[codex/v3-core-runtime 1a2b3c4] feat(types): add operator git actions\n \
                         1 file changed, 2 insertions(+)"
                    .to_string(),
                truncated: false,
                source: after_commit.clone(),
            },
            audit_id: "audit_operator_git_commit".to_string(),
        },
        RuntimeEventKind::WorkspaceSourceUpdated {
            source: after_commit.clone(),
        },
        // The push was permitted and then could not be honored, so it settles
        // as a *failure* rather than a rejection. The class is what tells the
        // client to offer `set_upstream`; the detail is for a human to read.
        RuntimeEventKind::OperatorGitActionFinished {
            command_id: "operator_git_push".to_string(),
            target: SourceTarget::Lane {
                lane_id: "lane_operator_git".to_string(),
            },
            action: OperatorGitAction::Push {
                remote: Some("origin".to_string()),
                set_upstream: false,
            },
            outcome: OperatorGitOutcome::Failed {
                class: OperatorGitFailureClass::NoUpstream,
                detail: "the current branch has no upstream branch; push with set_upstream to \
                         create one"
                    .to_string(),
            },
            audit_id: "audit_operator_git_push".to_string(),
        },
        // The source is resampled after a failure too: a rejected push leaves
        // facts worth reading, and guessing which ones it preserved would be
        // the inference this contract forbids.
        RuntimeEventKind::WorkspaceSourceUpdated {
            source: after_commit,
        },
    ];

    fixture(
        fixture_id,
        &[
            "runtime.approvals",
            "runtime.commands",
            "runtime.events",
            "runtime.operator_git",
            "runtime.snapshot",
        ],
        snapshot(WorkMode::Build),
        owned_envelopes(fixture_id, owner, kinds, 1_700_002_000),
    )
}

/// GUI-CORE-015: a merge that lands, a merge that collides, and a Lane apply
/// that collides — with the lines, not a sentence.
///
/// Two Lanes touch the same file. Lane A's patch merges. Lane B's patch was
/// written against the text Lane A replaced, so its apply is refused and the
/// bounce carries the collision: what the file holds now, what Lane B's patch
/// carried, and the preimage Lane B expected. Those three sides are the whole
/// contract — Core computes no merge base and resolves nothing, so a client
/// must render them as two sides plus a preimage and never as a three-way
/// merge with a resolvable result.
///
/// The two baselines are deliberately different kinds. A merge gate's baseline
/// *is* its canonical reviewed evidence, so the bounce says `Evidence`; a Lane
/// has no reviewed baseline, so its conflict says `Revision` — the commit the
/// worktree was read at. A single "baseline" string would have forced both
/// into a shape that is a lie for one of them.
///
/// Every value is fixed and no machine path appears, so the bytes are
/// identical on every machine that regenerates this fixture.
fn conflict_content_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "conflict-content";
    let owner = RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden".to_string(),
        lane_id: Some("lane_conflict_b".to_string()),
        session_id: Some("session_conflict_content".to_string()),
        task_id: Some("task_conflict_b".to_string()),
        turn_id: Some("turn_conflict_content".to_string()),
    };
    let lane_a_owner = RuntimeOwner {
        lane_id: Some("lane_conflict_a".to_string()),
        task_id: Some("task_conflict_a".to_string()),
        ..owner.clone()
    };
    let baseline_binding = viden_types::ReviewedEvidenceBinding {
        evidence_id: "ev_conflict_patch_b".to_string(),
        source_hash: "cf".repeat(32),
    };

    let merged_gate = MergeGateRecord {
        gate_id: "gate_conflict_a".to_string(),
        task_id: "task_conflict_a".to_string(),
        status: MergeGateStatus::Merged,
        required_evidence: vec!["patch".to_string()],
        evidence_ids: vec!["ev_conflict_patch_a".to_string()],
        gate_type: MergeGateType::Patch,
        owner: lane_a_owner.clone(),
        validator: None,
        policy_snapshot: MergeGatePolicySnapshot {
            required_evidence: vec!["patch".to_string()],
            permission_snapshot_id: Some("permission_conflict_content".to_string()),
            requires_independent_validator: false,
            captured_at: Some(1_700_003_000),
        },
        decision: Some(MergeGateDecision {
            outcome: MergeGateDecisionOutcome::Merged,
            reason: "apply reviewed patch".to_string(),
            owner: lane_a_owner,
            evidence_ids: vec!["ev_conflict_patch_a".to_string()],
            reviewed_evidence: Vec::new(),
            review_request_id: None,
            audit_id: "audit_conflict_merge_a".to_string(),
            decided_at: 1_700_003_001,
        }),
        conflict: None,
        applied_change_id: Some("change_conflict_a".to_string()),
        recovery_snapshot: None,
        audit_ids: vec!["audit_conflict_merge_a".to_string()],
        updated_at: Some(1_700_003_001),
    };

    // What Lane B's apply collided with. One file, one hunk: the hunks after a
    // refusal were never attempted, so listing them would report a collision
    // nothing observed.
    let bounce_content = ConflictContent {
        // The gate holds canonical reviewed evidence, and that is what the
        // reviewer accepted the patch against.
        baseline: ConflictBaseline::Evidence {
            bindings: vec![baseline_binding.clone()],
        },
        files: vec![ConflictFile {
            path: "crates/runtime/src/trust_loop.rs".to_string(),
            hunks: vec![ConflictHunk {
                ours_start: 42,
                ours: vec!["    let bounce = record_conflict_bounce(gate)?;\n".to_string()],
                theirs_start: 42,
                theirs: vec!["    let bounce = bounce_with_reason(gate, reason)?;\n".to_string()],
                base: Some(vec!["    let bounce = record_bounce(gate)?;\n".to_string()]),
                reason: ConflictHunkReason::ContextMismatch,
            }],
            omitted: false,
        }],
        truncated: false,
    };
    let bounce = ConflictBounce {
        bounce_id: "bounce_conflict_b".to_string(),
        gate_id: "gate_conflict_b".to_string(),
        task_id: "task_conflict_b".to_string(),
        original_lane_id: "lane_conflict_b".to_string(),
        owner: owner.clone(),
        reason: "patch conflict: expected hunk context was not found".to_string(),
        status: ConflictBounceStatus::Pending,
        evidence_ids: vec!["ev_conflict_patch_b".to_string()],
        baseline_evidence: vec![baseline_binding],
        revalidation_evidence: Vec::new(),
        content: Some(bounce_content),
        audit_id: "audit_conflict_bounce_b".to_string(),
        created_at: 1_700_003_002,
        revalidated_at: None,
    };
    let bounced_gate = MergeGateRecord {
        gate_id: "gate_conflict_b".to_string(),
        task_id: "task_conflict_b".to_string(),
        status: MergeGateStatus::NeedsChanges,
        required_evidence: vec!["patch".to_string()],
        evidence_ids: vec!["ev_conflict_patch_b".to_string()],
        gate_type: MergeGateType::Patch,
        owner: owner.clone(),
        validator: None,
        policy_snapshot: MergeGatePolicySnapshot {
            required_evidence: vec!["patch".to_string()],
            permission_snapshot_id: Some("permission_conflict_content".to_string()),
            requires_independent_validator: false,
            captured_at: Some(1_700_003_000),
        },
        decision: Some(MergeGateDecision {
            outcome: MergeGateDecisionOutcome::Conflict,
            reason: "patch conflict: expected hunk context was not found".to_string(),
            owner: owner.clone(),
            evidence_ids: vec!["ev_conflict_patch_b".to_string()],
            reviewed_evidence: Vec::new(),
            review_request_id: None,
            audit_id: "audit_conflict_bounce_b".to_string(),
            decided_at: 1_700_003_002,
        }),
        conflict: Some(bounce.clone()),
        applied_change_id: None,
        recovery_snapshot: None,
        audit_ids: vec!["audit_conflict_bounce_b".to_string()],
        updated_at: Some(1_700_003_002),
    };

    // The Lane apply path. A Lane has no reviewed baseline, so the honest
    // answer is the commit its worktree was read at.
    let lane_content = ConflictContent {
        baseline: ConflictBaseline::Revision {
            sha: "9f".repeat(20),
        },
        files: vec![ConflictFile {
            path: "crates/runtime/src/trust_loop.rs".to_string(),
            hunks: vec![ConflictHunk {
                ours_start: 42,
                ours: vec!["    let bounce = record_conflict_bounce(gate)?;\n".to_string()],
                theirs_start: 42,
                theirs: vec!["    let bounce = lane_bounce(gate)?;\n".to_string()],
                base: Some(vec!["    let bounce = record_bounce(gate)?;\n".to_string()]),
                reason: ConflictHunkReason::ContextMismatch,
            }],
            omitted: false,
        }],
        truncated: false,
    };

    let kinds = vec![
        RuntimeEventKind::MergeGateUpdated { gate: merged_gate },
        RuntimeEventKind::MergeConflictBounced {
            conflict: bounce.clone(),
        },
        RuntimeEventKind::MergeGateUpdated { gate: bounced_gate },
        RuntimeEventKind::LaneConflictDetected {
            lane_id: "lane_conflict_b".to_string(),
            summary: "patch conflict: expected hunk context was not found".to_string(),
            paths: vec!["crates/runtime/src/trust_loop.rs".to_string()],
            content: Some(lane_content),
        },
    ];

    fixture(
        fixture_id,
        &[
            "runtime.conflict_content",
            "runtime.events",
            "runtime.merge_gate",
            "runtime.snapshot",
            "runtime.typed_lanes",
        ],
        snapshot(WorkMode::Build),
        owned_envelopes(fixture_id, owner, kinds, 1_700_003_000),
    )
}

/// Canonical proof that a workspace inventory page is attributable to the exact
/// read that asked for it, that its entries are ordered, and that a project
/// nobody read leaves a client with no file list (GUI-CORE-022).
///
/// Two reads are accepted before either is answered and the pages come back in
/// the opposite order, so arrival order cannot stand in for correlation. Unlike
/// `AuditPageLoaded` the command id is required here, so there is no
/// uncorrelated page to model. A second project publishes lane facts and no
/// inventory at all: the "project without one" the request asks for, where the
/// only honest client state is "no list", never an empty list.
fn workspace_files_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "workspace-files";
    let read_owner = RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden".to_string(),
        lane_id: None,
        session_id: Some("session_workspace_files".to_string()),
        task_id: None,
        turn_id: Some("turn_workspace_files".to_string()),
    };
    // A second attached project in the same workspace. Its stream carries real
    // lane facts, so "no file list" here is the absence of a read rather than
    // the absence of a project.
    let unread_owner = RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden_docs".to_string(),
        lane_id: Some("lane_workspace_files_docs".to_string()),
        session_id: Some("session_workspace_files_docs".to_string()),
        task_id: None,
        turn_id: None,
    };
    let file = |path: &str, size: u64| WorkspaceFileEntry {
        path: path.to_string(),
        kind: WorkspaceFileKind::File,
        size_bytes: Some(size),
    };
    let dir = |path: &str| WorkspaceFileEntry {
        path: path.to_string(),
        kind: WorkspaceFileKind::Dir,
        size_bytes: None,
    };
    let accepted =
        |command_id: &str, query: WorkspaceFilesQuery| RuntimeEventKind::CommandAccepted {
            command_id: command_id.to_string(),
            command: RuntimeCommand::QueryWorkspaceFiles { query },
        };
    let loaded =
        |command_id: &str, page: WorkspaceFilePage| RuntimeEventKind::WorkspaceFilesLoaded {
            command_id: command_id.to_string(),
            page,
        };
    // The first page stops mid-tree: `next_after` names the entry it stopped
    // at, and the cursor is exclusive so the next read resumes strictly after.
    let root_page = WorkspaceFilePage {
        entries: vec![
            file("AGENTS.md", 4_096),
            file("README.md", 2_048),
            dir("crates"),
            file("crates/core/src/lib.rs", 8_192),
        ],
        next_after: Some("crates/core/src/lib.rs".to_string()),
        complete: false,
    };
    // The scoped read: Core applied the prefix before cutting the page, so
    // `complete` describes the `crates/types` subtree even though the root page
    // above is still incomplete.
    let scoped_page = WorkspaceFilePage {
        entries: vec![
            dir("crates/types"),
            file("crates/types/src/audit.rs", 16_384),
            file("crates/types/src/lib.rs", 32_768),
        ],
        next_after: None,
        complete: true,
    };
    let docs_lane = AgentLaneRecord {
        id: "lane_workspace_files_docs".to_string(),
        task_id: None,
        role: AgentRole::Reviewer,
        route: AgentRoute::BuiltIn,
        gate_strength: GateStrength::Full,
        mutation_policy: MutationPolicy::ProposeOnly,
        worktree: Some("workspace/.worktrees/lane_workspace_files_docs".to_string()),
        branch: Some("codex/lane_workspace_files_docs".to_string()),
        target: ExecutionTarget::Local,
        data_egress: viden_types::DataEgressPolicy::Deny,
        status: LaneStatus::Running,
        budget: LaneBudget::default(),
        active_session_ids: vec!["session_workspace_files_docs".to_string()],
        summary: "lane.workspace_files_docs.running".to_string(),
        evidence: Vec::new(),
        run_stats: None,
    };

    let owned = vec![
        // Both reads are outstanding before either is answered.
        (
            read_owner.clone(),
            accepted(
                "workspace_files_first",
                WorkspaceFilesQuery {
                    prefix: None,
                    limit: Some(4),
                    after: None,
                },
            ),
        ),
        (
            read_owner.clone(),
            accepted(
                "workspace_files_second",
                WorkspaceFilesQuery {
                    prefix: Some("crates/types".to_string()),
                    limit: Some(50),
                    after: None,
                },
            ),
        ),
        // The second read is answered first, so a client correlating by arrival
        // order would attribute this page to the first read.
        (
            read_owner.clone(),
            loaded("workspace_files_second", scoped_page),
        ),
        (read_owner, loaded("workspace_files_first", root_page)),
        // The unread project's own stream: a real lane fact and no inventory.
        (
            unread_owner,
            RuntimeEventKind::LaneUpdated { lane: docs_lane },
        ),
    ];

    let events = owned
        .into_iter()
        .enumerate()
        .map(|(index, (owner, kind))| {
            let sequence = index as u64 + 1;
            RuntimeEventEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                owner,
                cursor: EventCursor {
                    stream_id: format!("fixture:{fixture_id}"),
                    sequence,
                },
                event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
                    sequence,
                    Some(1_700_000_700 + sequence),
                    kind,
                )),
            }
        })
        .collect::<Vec<_>>();

    fixture(
        fixture_id,
        &[
            "runtime.commands",
            "runtime.events",
            "runtime.snapshot",
            "runtime.typed_lanes",
            "runtime.workspace_files",
        ],
        snapshot(WorkMode::Build),
        events,
    )
}

/// Canonical proof that live-work facts carry the runtime owner they belong to
/// and that a selected-owner projection sees only its own (GUI-CORE-010).
///
/// Two Lanes run at once, each with its own task, active tool call, queued
/// input, and evidence record. A fourth group of the same four fact kinds is
/// published without an owner, because Core did not know one at emission; those
/// stay visible workspace-wide and belong to neither Lane. Nothing in the
/// stream lets a client tell the groups apart by ordering, timing, or label —
/// only the published owner does.
fn owner_scoped_live_work_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "owner-scoped-live-work";
    let lane_owner = |lane: &str, task: &str, session: &str, turn: &str| RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden".to_string(),
        lane_id: Some(lane.to_string()),
        session_id: Some(session.to_string()),
        task_id: Some(task.to_string()),
        turn_id: Some(turn.to_string()),
    };
    // The envelope owner for facts Core published with no owner of their own:
    // the workspace-scoped runtime owner, bound to no Lane.
    let workspace_owner = RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden".to_string(),
        lane_id: None,
        session_id: None,
        task_id: None,
        turn_id: None,
    };
    let lane = |id: &str, task: &str, session: &str| AgentLaneRecord {
        id: id.to_string(),
        task_id: Some(task.to_string()),
        role: AgentRole::Coder,
        route: AgentRoute::Acp,
        gate_strength: GateStrength::Full,
        mutation_policy: MutationPolicy::ProposeOnly,
        worktree: Some(format!("workspace/.worktrees/{id}")),
        branch: Some(format!("codex/{id}")),
        target: ExecutionTarget::Local,
        data_egress: viden_types::DataEgressPolicy::Deny,
        status: LaneStatus::Running,
        budget: LaneBudget::default(),
        active_session_ids: vec![session.to_string()],
        summary: format!("lane.{id}.running"),
        evidence: Vec::new(),
        run_stats: None,
    };
    let owned_task = |id: &str, owner: Option<&RuntimeOwner>| AgentTaskRecord {
        id: id.to_string(),
        parent_id: None,
        role: AgentRole::Coder,
        kind: AgentTaskKind::Job,
        route: AgentRoute::Acp,
        title: format!("{id} title"),
        status: AgentTaskStatus::RunningTool,
        activity: "running an agent job".to_string(),
        summary: format!("{id} summary"),
        progress: 40,
        started_at: Some(1_700_000_600),
        updated_at: Some(1_700_000_640),
        workspace: None,
        evidence: Vec::new(),
        permissions: vec!["ask".to_string()],
        decision: None,
        result: None,
        resume_handle: None,
        pid: None,
        next_action: None,
        owner: owner.cloned(),
    };
    let owned_evidence = |id: &str, owner: Option<&RuntimeOwner>| EvidenceView {
        id: id.to_string(),
        kind: "tool_log".to_string(),
        summary: format!("{id} summary"),
        path: None,
        source: Some("acp".to_string()),
        canonical: None,
        metadata: None,
        timestamp: Some(1_700_000_660),
        owner: owner.cloned(),
    };
    let owned_input = |id: &str, owner: Option<&RuntimeOwner>| QueuedInputView {
        id: id.to_string(),
        content_preview: format!("{id} preview"),
        created_at: Some(1_700_000_650),
        owner: owner.cloned(),
    };
    let tool_call = |id: &str, owner: Option<&RuntimeOwner>| RuntimeEventKind::ToolCallStarted {
        tool_call_id: id.to_string(),
        name: "shell".to_string(),
        input_preview: "cargo test".to_string(),
        owner: owner.cloned(),
    };

    let alpha = lane_owner(
        "lane_live_alpha",
        "task_live_alpha",
        "session_live_alpha",
        "turn_live_alpha",
    );
    let beta = lane_owner(
        "lane_live_beta",
        "task_live_beta",
        "session_live_beta",
        "turn_live_beta",
    );

    let mut owned_events: Vec<(RuntimeEventKind, RuntimeOwner)> = vec![
        (
            RuntimeEventKind::LaneUpdated {
                lane: lane("lane_live_alpha", "task_live_alpha", "session_live_alpha"),
            },
            alpha.clone(),
        ),
        (
            RuntimeEventKind::LaneRuntimeOwnerBound {
                binding: LaneRuntimeOwnerBinding {
                    lane_id: "lane_live_alpha".to_string(),
                    owner: alpha.clone(),
                },
            },
            alpha.clone(),
        ),
        (
            RuntimeEventKind::LaneUpdated {
                lane: lane("lane_live_beta", "task_live_beta", "session_live_beta"),
            },
            beta.clone(),
        ),
        (
            RuntimeEventKind::LaneRuntimeOwnerBound {
                binding: LaneRuntimeOwnerBinding {
                    lane_id: "lane_live_beta".to_string(),
                    owner: beta.clone(),
                },
            },
            beta.clone(),
        ),
    ];
    // Interleave the two Lanes' live work so arrival order carries no grouping
    // a client could mistake for ownership.
    for (suffix, owner) in [("alpha", &alpha), ("beta", &beta)] {
        owned_events.extend([
            (
                RuntimeEventKind::TaskUpdated {
                    task: owned_task(&format!("task_live_{suffix}"), Some(owner)),
                },
                owner.clone(),
            ),
            (
                tool_call(&format!("tool_live_{suffix}"), Some(owner)),
                owner.clone(),
            ),
            (
                RuntimeEventKind::InputQueued {
                    input: owned_input(&format!("queued_live_{suffix}"), Some(owner)),
                },
                owner.clone(),
            ),
            (
                RuntimeEventKind::EvidenceRecorded {
                    evidence: owned_evidence(&format!("evidence_live_{suffix}"), Some(owner)),
                },
                owner.clone(),
            ),
        ]);
    }
    // The same four fact kinds with no owner Core could name.
    owned_events.extend([
        (
            RuntimeEventKind::TaskUpdated {
                task: owned_task("task_live_unowned", None),
            },
            workspace_owner.clone(),
        ),
        (
            tool_call("tool_live_unowned", None),
            workspace_owner.clone(),
        ),
        (
            RuntimeEventKind::InputQueued {
                input: owned_input("queued_live_unowned", None),
            },
            workspace_owner.clone(),
        ),
        (
            RuntimeEventKind::EvidenceRecorded {
                evidence: owned_evidence("evidence_live_unowned", None),
            },
            workspace_owner,
        ),
    ]);

    let events = owned_events
        .into_iter()
        .enumerate()
        .map(|(index, (kind, owner))| {
            let sequence = index as u64 + 1;
            RuntimeEventEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                owner,
                cursor: EventCursor {
                    stream_id: format!("fixture:{fixture_id}"),
                    sequence,
                },
                event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
                    sequence,
                    Some(1_700_000_600 + sequence),
                    kind,
                )),
            }
        })
        .collect();

    fixture(
        fixture_id,
        &[
            "runtime.evidence",
            "runtime.events",
            "runtime.lane_lifecycle",
            "runtime.lane_owner_projection",
            "runtime.queued_input",
            "runtime.snapshot",
            "runtime.typed_lanes",
            "runtime.typed_tasks",
        ],
        snapshot(WorkMode::Build),
        events,
    )
}

/// Wraps ordered event kinds published by one owner into contiguous envelopes.
fn owned_envelopes(
    fixture_id: &str,
    owner: RuntimeOwner,
    kinds: Vec<RuntimeEventKind>,
    base_timestamp: u64,
) -> Vec<RuntimeEventEnvelope> {
    kinds
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            let sequence = index as u64 + 1;
            RuntimeEventEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                owner: owner.clone(),
                cursor: EventCursor {
                    stream_id: format!("fixture:{fixture_id}"),
                    sequence,
                },
                event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
                    sequence,
                    Some(base_timestamp + sequence),
                    kind,
                )),
            }
        })
        .collect()
}

fn frontend_host_services_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "frontend-host-services";
    let owner = RuntimeOwner {
        workspace_id: "workspace-host-fixture".to_string(),
        project_id: "project-host-fixture".to_string(),
        lane_id: Some("lane-host-fixture".to_string()),
        session_id: Some("session-host-fixture".to_string()),
        task_id: Some("task_host_fixture".to_string()),
        turn_id: Some("turn-host-fixture".to_string()),
    };
    let lane = AgentLaneRecord {
        id: "lane-host-fixture".to_string(),
        task_id: Some("task_host_fixture".to_string()),
        role: AgentRole::Coder,
        route: AgentRoute::BuiltIn,
        gate_strength: GateStrength::Full,
        mutation_policy: MutationPolicy::ProposeOnly,
        worktree: Some("workspace/.worktrees/lane-host-fixture".to_string()),
        branch: Some("codex/lane-host-fixture".to_string()),
        target: ExecutionTarget::Local,
        data_egress: viden_types::DataEgressPolicy::Deny,
        status: LaneStatus::Running,
        budget: LaneBudget::default(),
        active_session_ids: vec!["session-host-fixture".to_string()],
        summary: "reviewed starter Lane".to_string(),
        evidence: Vec::new(),
        run_stats: None,
    };
    let preview = StarterLanePreview {
        preview_id: "preview-host-fixture".to_string(),
        content_sha256: "ab".repeat(32),
        owner: owner.clone(),
        lane: lane.clone(),
        branch: "codex/lane-host-fixture".to_string(),
        worktree_path: "workspace/.worktrees/lane-host-fixture".to_string(),
        base_revision: "cd".repeat(20),
        diagnostics: Vec::new(),
    };
    let kinds = vec![
        RuntimeEventKind::UiPreferencesUpdated {
            resolved: ResolvedUiPreferences {
                locale: viden_types::LocaleId::ZhCn,
                skin: UiSkin::Ice,
                mode: UiColorMode::Dark,
                density: UiDensity::Compact,
                motion: UiMotion::Reduced,
                diagnostics: Vec::new(),
            },
            persisted: Some(UiPreferences {
                locale: viden_types::LocaleId::ZhCn,
                skin: UiSkin::Ice,
                mode: UiColorMode::Dark,
                density: UiDensity::Compact,
                motion: UiMotion::Reduced,
            }),
            diagnostics: Vec::new(),
        },
        RuntimeEventKind::RecentWorkLoaded {
            projects: vec![RecentProjectSummary {
                canonical_root: "workspace/project".to_string(),
                display_name: "project".to_string(),
                last_updated_at: 1_700_000_020,
                latest_session_id: Some("session-host-fixture".to_string()),
            }],
            sessions: vec![RecentSessionSummary {
                canonical_root: "workspace/project".to_string(),
                session_id: "session-host-fixture".to_string(),
                created_at: 1_700_000_010,
                last_updated_at: 1_700_000_020,
                message_count: 2,
                tool_call_count: 1,
                command_count: 1,
            }],
            diagnostics: Vec::new(),
        },
        RuntimeEventKind::StarterLanePreviewed {
            preview: preview.clone(),
        },
        RuntimeEventKind::StarterLaneCreated {
            receipt: StarterLaneReceipt {
                preview_id: preview.preview_id.clone(),
                content_sha256: preview.content_sha256.clone(),
                lane,
                branch: preview.branch.clone(),
                worktree_path: preview.worktree_path.clone(),
                base_revision: preview.base_revision.clone(),
                owner: owner.clone(),
            },
        },
        RuntimeEventKind::StarterLanePreviewInvalidated {
            owner: owner.clone(),
            preview_id: preview.preview_id,
            reason: StarterLanePreviewInvalidationReason::BaseRevisionChanged,
        },
        RuntimeEventKind::LaneRuntimeOwnerBound {
            binding: LaneRuntimeOwnerBinding {
                lane_id: "lane-host-fixture".to_string(),
                owner: owner.clone(),
            },
        },
    ];
    let mut events = kinds
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            let sequence = index as u64 + 1;
            RuntimeEventEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                owner: owner.clone(),
                cursor: EventCursor {
                    stream_id: format!("fixture:{fixture_id}"),
                    sequence,
                },
                event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
                    sequence,
                    Some(1_700_000_000 + sequence),
                    kind,
                )),
            }
        })
        .collect::<Vec<_>>();
    events.push(RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner,
        cursor: EventCursor {
            stream_id: format!("fixture:{fixture_id}"),
            sequence: 7,
        },
        event: RuntimeWireEvent::Unknown {
            event_type: "future_frontend_host_fact".to_string(),
            payload: serde_json::json!({"optional": true}),
        },
    });
    fixture(
        fixture_id,
        &[
            "core.workspace_host",
            "runtime.credential_staging",
            "runtime.lane_owner_projection",
            "runtime.recent_work",
            "runtime.starter_lane_preview",
            "ui.preference_persistence",
        ],
        snapshot(WorkMode::Build),
        events,
    )
}

fn interaction_closed_loop_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "interaction-closed-loop";
    let acp_owner = RuntimeOwner {
        workspace_id: "workspace-loop".to_string(),
        project_id: "project-loop".to_string(),
        lane_id: Some("lane-loop-coder".to_string()),
        session_id: Some("session-loop-acp".to_string()),
        task_id: Some("task_loop".to_string()),
        turn_id: Some("turn-loop-acp".to_string()),
    };
    let built_in_owner = RuntimeOwner {
        session_id: Some("session-loop-built-in".to_string()),
        turn_id: Some("turn-loop-built-in".to_string()),
        ..acp_owner.clone()
    };
    let lane = AgentLaneRecord {
        id: "lane-loop-coder".to_string(),
        task_id: Some("task_loop".to_string()),
        role: AgentRole::Coder,
        route: AgentRoute::Acp,
        gate_strength: GateStrength::Cooperative,
        mutation_policy: MutationPolicy::ProposeOnly,
        worktree: Some("workspace/.worktrees/lane-loop-coder".to_string()),
        branch: Some("codex/lane-loop-coder".to_string()),
        target: ExecutionTarget::Local,
        data_egress: viden_types::DataEgressPolicy::Deny,
        status: LaneStatus::Running,
        budget: LaneBudget {
            token_limit: Some(24_000),
            cost_limit_micro_usd: Some(750_000),
            wall_time_limit_secs: Some(2_400),
        },
        active_session_ids: vec![
            "session-loop-built-in".to_string(),
            "session-loop-acp".to_string(),
        ],
        summary: "lane.loop.running".to_string(),
        evidence: Vec::new(),
        run_stats: None,
    };
    let preview = StarterLanePreview {
        preview_id: "preview-loop-coder".to_string(),
        content_sha256: "12".repeat(32),
        owner: acp_owner.clone(),
        lane: lane.clone(),
        branch: "codex/lane-loop-coder".to_string(),
        worktree_path: "workspace/.worktrees/lane-loop-coder".to_string(),
        base_revision: "34".repeat(20),
        diagnostics: Vec::new(),
    };
    let adapters = vec![
        AgentAdapterView {
            agent_id: "viden-built-in".to_string(),
            display_name: "Viden Built-in".to_string(),
            route: AgentRoute::BuiltIn,
            source: AgentAdapterSource::BuiltIn,
            availability: AgentAvailability::Available,
            auth_state: AgentAuthState::Ready,
            startability: AgentStartability::Ready,
            capabilities: vec![CapabilityId("agent.session.prompt".to_string())],
            models: vec!["workspace-default".to_string()],
            diagnostics: Vec::new(),
        },
        AgentAdapterView {
            agent_id: "claude-acp".to_string(),
            display_name: "Claude ACP".to_string(),
            route: AgentRoute::Acp,
            source: AgentAdapterSource::Registry,
            availability: AgentAvailability::Available,
            auth_state: AgentAuthState::Ready,
            startability: AgentStartability::Ready,
            capabilities: vec![CapabilityId("agent.session.prompt".to_string())],
            models: Vec::new(),
            diagnostics: Vec::new(),
        },
        AgentAdapterView {
            agent_id: "codex-acp".to_string(),
            display_name: "Codex ACP".to_string(),
            route: AgentRoute::Acp,
            source: AgentAdapterSource::Registry,
            availability: AgentAvailability::Available,
            auth_state: AgentAuthState::Ready,
            startability: AgentStartability::Ready,
            capabilities: vec![
                CapabilityId("agent.permission.request".to_string()),
                CapabilityId("agent.session.cancel".to_string()),
                CapabilityId("agent.session.prompt".to_string()),
            ],
            models: vec!["gpt-5".to_string()],
            diagnostics: Vec::new(),
        },
        AgentAdapterView {
            agent_id: "kiro-cli".to_string(),
            display_name: "Kiro CLI".to_string(),
            route: AgentRoute::Acp,
            source: AgentAdapterSource::LocalCommand,
            availability: AgentAvailability::NeedsAuth,
            auth_state: AgentAuthState::LoggedOut,
            startability: AgentStartability::AuthenticationRequired,
            capabilities: vec![CapabilityId("agent.session.prompt".to_string())],
            models: Vec::new(),
            diagnostics: vec!["agent.auth.required".to_string()],
        },
    ];
    let built_in_session = AgentSessionView {
        session_id: "session-loop-built-in".to_string(),
        lane_id: lane.id.clone(),
        agent_id: "viden-built-in".to_string(),
        model: Some("workspace-default".to_string()),
        status: AgentSessionStatus::Starting,
        owner: built_in_owner.clone(),
        task: "task.loop.preflight".to_string(),
        diagnostic: None,
        output: None,
    };
    let acp_session = AgentSessionView {
        session_id: "session-loop-acp".to_string(),
        lane_id: lane.id.clone(),
        agent_id: "codex-acp".to_string(),
        model: Some("gpt-5".to_string()),
        status: AgentSessionStatus::Starting,
        owner: acp_owner.clone(),
        task: "task.loop.implement".to_string(),
        diagnostic: None,
        output: None,
    };
    let mut approval = approval("approval-loop-tool", "approval.tool.execute", true);
    approval.owner = acp_owner.clone();
    approval.policy_reason_key = "approval.agent_tool.mutation".to_string();
    approval.policy_reason_args = BTreeMap::from([
        ("agent_id".to_string(), "codex-acp".to_string()),
        ("lane_id".to_string(), lane.id.clone()),
    ]);
    let loop_evidence = evidence("evidence-loop-test", "test_result", "evidence.test.passed");
    let gate = MergeGateRecord {
        gate_id: "gate-loop-apply".to_string(),
        task_id: "task_loop".to_string(),
        status: MergeGateStatus::Accepted,
        required_evidence: vec!["test_result".to_string()],
        evidence_ids: vec![loop_evidence.id.clone()],
        gate_type: MergeGateType::Artifact,
        owner: acp_owner.clone(),
        validator: None,
        policy_snapshot: MergeGatePolicySnapshot {
            required_evidence: vec!["test_result".to_string()],
            permission_snapshot_id: Some("permission-loop-1".to_string()),
            requires_independent_validator: false,
            captured_at: Some(1_700_100_014),
        },
        decision: Some(MergeGateDecision {
            outcome: MergeGateDecisionOutcome::Accepted,
            reason: "gate.evidence.satisfied".to_string(),
            owner: acp_owner.clone(),
            evidence_ids: vec![loop_evidence.id.clone()],
            reviewed_evidence: Vec::new(),
            review_request_id: None,
            audit_id: "audit-loop-gate".to_string(),
            decided_at: 1_700_100_014,
        }),
        conflict: None,
        applied_change_id: None,
        recovery_snapshot: None,
        audit_ids: vec!["audit-loop-gate".to_string()],
        updated_at: Some(1_700_100_014),
    };
    let events_with_owners = vec![
        (
            acp_owner.clone(),
            RuntimeEventKind::ProjectProbed {
                probe: ProjectProbe {
                    root: "workspace/project".to_string(),
                    is_git_repository: true,
                    git_root: Some("workspace/project".to_string()),
                    config_path: "workspace/project/viden.toml".to_string(),
                    config_state: ProjectConfigState::Missing,
                    project_name: Some("project".to_string()),
                    pack: None,
                    diagnostics: Vec::new(),
                },
            },
        ),
        (
            acp_owner.clone(),
            RuntimeEventKind::WorkspaceEligibilityUpdated {
                eligibility: WorkspaceEligibility {
                    is_git_repository: true,
                    has_head: true,
                    can_create_lane: true,
                    diagnostic: None,
                },
            },
        ),
        (
            acp_owner.clone(),
            RuntimeEventKind::StarterLanePreviewed {
                preview: preview.clone(),
            },
        ),
        (
            acp_owner.clone(),
            RuntimeEventKind::StarterLaneCreated {
                receipt: StarterLaneReceipt {
                    preview_id: preview.preview_id.clone(),
                    content_sha256: preview.content_sha256.clone(),
                    lane,
                    branch: preview.branch.clone(),
                    worktree_path: preview.worktree_path.clone(),
                    base_revision: preview.base_revision.clone(),
                    owner: acp_owner.clone(),
                },
            },
        ),
        (
            acp_owner.clone(),
            RuntimeEventKind::AgentAdaptersLoaded { adapters },
        ),
        (
            built_in_owner.clone(),
            RuntimeEventKind::AgentSessionStarted {
                session: built_in_session.clone(),
            },
        ),
        (
            built_in_owner,
            RuntimeEventKind::AgentSessionCompleted {
                session: AgentSessionView {
                    status: AgentSessionStatus::Completed,
                    ..built_in_session
                },
            },
        ),
        (
            acp_owner.clone(),
            RuntimeEventKind::AgentSessionStarted {
                session: acp_session.clone(),
            },
        ),
        (
            acp_owner.clone(),
            RuntimeEventKind::ToolCallStarted {
                tool_call_id: "tool-loop-test".to_string(),
                name: "shell".to_string(),
                input_preview: "command.test.core".to_string(),
                owner: None,
            },
        ),
        (
            acp_owner.clone(),
            RuntimeEventKind::ToolCallFinished {
                tool_call_id: "tool-loop-test".to_string(),
                name: "shell".to_string(),
                success: true,
                exit_code: Some(0),
                evidence: None,
            },
        ),
        (
            acp_owner.clone(),
            RuntimeEventKind::ApprovalRequested { approval },
        ),
        (
            acp_owner.clone(),
            RuntimeEventKind::AgentSessionUpdated {
                session: AgentSessionView {
                    status: AgentSessionStatus::WaitingApproval,
                    ..acp_session.clone()
                },
            },
        ),
        (
            acp_owner.clone(),
            RuntimeEventKind::ApprovalResolved {
                request_id: "approval-loop-tool".to_string(),
                decision: ApprovalDecision::Allow {
                    scope: ApprovalScope::Once,
                },
                owner: acp_owner.clone(),
                audit_id: "audit-approval-loop-tool".to_string(),
            },
        ),
        (
            acp_owner.clone(),
            RuntimeEventKind::AgentSessionUpdated {
                session: AgentSessionView {
                    status: AgentSessionStatus::Running,
                    ..acp_session.clone()
                },
            },
        ),
        (
            acp_owner.clone(),
            RuntimeEventKind::EvidenceRecorded {
                evidence: loop_evidence,
            },
        ),
        (
            acp_owner.clone(),
            RuntimeEventKind::MergeGateUpdated { gate },
        ),
        (
            acp_owner.clone(),
            RuntimeEventKind::LaneConflictDetected {
                lane_id: "lane-loop-coder".to_string(),
                summary: "conflict.apply.non_fast_forward".to_string(),
                paths: vec!["src/lib.rs".to_string()],
                content: None,
            },
        ),
        (
            acp_owner.clone(),
            RuntimeEventKind::LaneRecoveryRequired {
                lane_id: "lane-loop-coder".to_string(),
                reason: "recovery.apply_conflict".to_string(),
                next_action: "action.revalidate_merge_conflict".to_string(),
            },
        ),
        (
            acp_owner.clone(),
            RuntimeEventKind::AgentSessionCompleted {
                session: AgentSessionView {
                    status: AgentSessionStatus::Completed,
                    ..acp_session.clone()
                },
            },
        ),
        (
            acp_owner.clone(),
            RuntimeEventKind::AgentSessionInputAccepted {
                session_id: acp_session.session_id.clone(),
                input_id: "agent-input-loop-follow-up".to_string(),
            },
        ),
        (
            acp_owner.clone(),
            RuntimeEventKind::AgentSessionStarted {
                session: AgentSessionView {
                    status: AgentSessionStatus::Starting,
                    task: "task.loop.follow_up".to_string(),
                    ..acp_session.clone()
                },
            },
        ),
        (
            acp_owner,
            RuntimeEventKind::AgentSessionCompleted {
                session: AgentSessionView {
                    status: AgentSessionStatus::Completed,
                    task: "task.loop.follow_up".to_string(),
                    ..acp_session
                },
            },
        ),
    ];
    let events = events_with_owners
        .into_iter()
        .enumerate()
        .map(|(index, (owner, kind))| {
            let sequence = index as u64 + 1;
            RuntimeEventEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                owner,
                cursor: EventCursor {
                    stream_id: format!("fixture:{fixture_id}"),
                    sequence,
                },
                event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
                    sequence,
                    Some(1_700_100_000 + sequence),
                    kind,
                )),
            }
        })
        .collect();

    fixture(
        fixture_id,
        &[
            "core.workspace_host",
            "runtime.agent_adapters",
            "runtime.agent_conversation",
            "runtime.agent_permission_bridge",
            "runtime.agent_session_input",
            "runtime.agent_sessions",
            "runtime.approvals",
            "runtime.events",
            "runtime.evidence",
            "runtime.lane_lifecycle",
            "runtime.merge_gate",
            "runtime.project_onboarding",
            "runtime.replay",
            "runtime.snapshot",
            "runtime.starter_lane_preview",
            "runtime.typed_lanes",
            "runtime.workspace_eligibility",
        ],
        snapshot(WorkMode::Build),
        events,
    )
}

fn fixture(
    fixture_id: &str,
    required_capabilities: &[&str],
    initial_snapshot: RuntimeSnapshot,
    events: Vec<RuntimeEventEnvelope>,
) -> FrontendContractFixtureOut {
    let mut view = RuntimeViewState::new(initial_snapshot.clone());
    for envelope in &events {
        if let RuntimeWireEvent::Known(event) = &envelope.event {
            view.apply_event(event);
        }
    }
    let expected_final_cursor = events.last().unwrap().cursor.clone();
    let mut required_capabilities = required_capabilities
        .iter()
        .map(|capability| CapabilityId((*capability).to_string()))
        .collect::<Vec<_>>();
    required_capabilities.sort();
    FrontendContractFixtureOut {
        fixture_id: fixture_id.to_string(),
        schema_version: FRONTEND_SCHEMA_V1,
        required_capabilities,
        initial_snapshot,
        events,
        expected_final_cursor,
        expected_view_sha256: canonical_view_sha256(&view),
    }
}

fn envelopes(fixture_id: &str, kinds: Vec<RuntimeEventKindExt>) -> Vec<RuntimeEventEnvelope> {
    kinds
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            let sequence = index as u64 + 1;
            RuntimeEventEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                owner: RuntimeOwner {
                    workspace_id: "workspace_contract_v1".to_string(),
                    project_id: "project_viden".to_string(),
                    lane_id: None,
                    session_id: Some(format!("session_{fixture_id}")),
                    task_id: None,
                    turn_id: Some(format!("turn_{fixture_id}")),
                },
                cursor: EventCursor {
                    stream_id: format!("fixture:{fixture_id}"),
                    sequence,
                },
                event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
                    sequence,
                    Some(1_700_000_000 + sequence),
                    to_runtime_event_kind(kind),
                )),
            }
        })
        .collect()
}

enum RuntimeEventKindExt {
    Assistant(&'static str, &'static str),
    ToolStarted(&'static str, &'static str, &'static str),
    ToolFinished(&'static str, &'static str, bool, Option<EvidenceView>),
    ApprovalRequested(&'static str, &'static str, bool),
    ApprovalResolved(&'static str, bool),
    InputQueued(&'static str, &'static str),
    InputDequeued(&'static str),
    Dag(&'static str, AgentDagStatus),
    Task(
        &'static str,
        AgentRole,
        AgentTaskStatus,
        &'static str,
        Option<&'static str>,
    ),
    Lane(
        &'static str,
        &'static str,
        AgentRole,
        AgentRoute,
        LaneStatus,
        ExecutionTarget,
    ),
    Evidence(EvidenceView),
    MergeGate(
        &'static str,
        &'static str,
        MergeGateStatus,
        Vec<&'static str>,
    ),
    ContextPressure,
    CostUnknown,
    TokenCostUnknown,
    CommandRejected(&'static str, &'static str),
    Error(&'static str, bool),
}

fn to_runtime_event_kind(kind: RuntimeEventKindExt) -> viden_types::RuntimeEventKind {
    match kind {
        RuntimeEventKindExt::Assistant(message_id, content) => {
            viden_types::RuntimeEventKind::AssistantDelta {
                message_id: message_id.to_string(),
                task_id: None,
                session_id: None,
                content: content.to_string(),
            }
        }
        RuntimeEventKindExt::ToolStarted(tool_call_id, name, input_preview) => {
            viden_types::RuntimeEventKind::ToolCallStarted {
                tool_call_id: tool_call_id.to_string(),
                name: name.to_string(),
                input_preview: input_preview.to_string(),
                owner: None,
            }
        }
        RuntimeEventKindExt::ToolFinished(tool_call_id, name, success, evidence) => {
            viden_types::RuntimeEventKind::ToolCallFinished {
                tool_call_id: tool_call_id.to_string(),
                name: name.to_string(),
                success,
                exit_code: Some(if success { 0 } else { 1 }),
                evidence,
            }
        }
        RuntimeEventKindExt::ApprovalRequested(id, title, is_mutating) => {
            viden_types::RuntimeEventKind::ApprovalRequested {
                approval: approval(id, title, is_mutating),
            }
        }
        RuntimeEventKindExt::ApprovalResolved(request_id, allow) => {
            viden_types::RuntimeEventKind::ApprovalResolved {
                request_id: request_id.to_string(),
                decision: if allow {
                    ApprovalDecision::Allow {
                        scope: ApprovalScope::Once,
                    }
                } else {
                    ApprovalDecision::Deny
                },
                owner: RuntimeOwner::default(),
                audit_id: format!("audit_{request_id}"),
            }
        }
        RuntimeEventKindExt::InputQueued(id, content_preview) => {
            viden_types::RuntimeEventKind::InputQueued {
                input: QueuedInputView {
                    id: id.to_string(),
                    content_preview: content_preview.to_string(),
                    created_at: Some(1_700_000_010),
                    owner: None,
                },
            }
        }
        RuntimeEventKindExt::InputDequeued(input_id) => {
            viden_types::RuntimeEventKind::InputDequeued {
                input_id: input_id.to_string(),
            }
        }
        RuntimeEventKindExt::Dag(dag_id, status) => {
            viden_types::RuntimeEventKind::AgentDagUpdated {
                dag: AgentDagRecord {
                    dag_id: dag_id.to_string(),
                    goal: "freeze frontend contract v1".to_string(),
                    status,
                    tasks: vec![AgentDagTaskSpec {
                        task_id: "task_blocked".to_string(),
                        role: AgentRole::Coder,
                        title: "resolve blocker".to_string(),
                        objective: "recover dependency".to_string(),
                        dependencies: vec!["task_dependency".to_string()],
                        workspace: Some(".worktrees/v3-core-runtime".to_string()),
                        file_scope: vec!["crates/core".to_string()],
                        context_bundle_id: Some("ctx_bundle_contract".to_string()),
                        required_evidence: vec!["test_result".to_string()],
                        permission_policy: "ask_before_mutation".to_string(),
                    }],
                    created_at: Some(1_700_000_000),
                    updated_at: Some(1_700_000_030),
                },
            }
        }
        RuntimeEventKindExt::Task(id, role, status, activity, next_action) => {
            viden_types::RuntimeEventKind::TaskUpdated {
                task: task(id, role, status, activity, next_action),
            }
        }
        RuntimeEventKindExt::Lane(id, task_id, role, route, status, target) => {
            viden_types::RuntimeEventKind::LaneUpdated {
                lane: AgentLaneRecord {
                    id: id.to_string(),
                    task_id: Some(task_id.to_string()),
                    role,
                    route,
                    gate_strength: match route {
                        AgentRoute::BuiltIn => GateStrength::Full,
                        AgentRoute::Acp => GateStrength::Cooperative,
                        AgentRoute::Terminal | AgentRoute::Tmux => GateStrength::Containment,
                    },
                    mutation_policy: MutationPolicy::ProposeOnly,
                    worktree: Some(format!(".worktrees/{id}")),
                    branch: Some(format!("codex/{id}")),
                    target,
                    data_egress: viden_types::DataEgressPolicy::AllowListed {
                        domains: vec!["docs.example.test".to_string()],
                    },
                    status,
                    budget: LaneBudget {
                        token_limit: Some(16_000),
                        cost_limit_micro_usd: Some(500_000),
                        wall_time_limit_secs: Some(1_800),
                    },
                    active_session_ids: vec![format!("session_{id}")],
                    summary: format!("{id} active"),
                    evidence: vec![format!("evidence_{id}")],
                    run_stats: None,
                },
            }
        }
        RuntimeEventKindExt::Evidence(evidence) => {
            viden_types::RuntimeEventKind::EvidenceRecorded { evidence }
        }
        RuntimeEventKindExt::MergeGate(gate_id, task_id, status, evidence_ids) => {
            viden_types::RuntimeEventKind::MergeGateUpdated {
                gate: MergeGateRecord {
                    gate_id: gate_id.to_string(),
                    task_id: task_id.to_string(),
                    status,
                    required_evidence: vec![
                        "patch".to_string(),
                        "test_result".to_string(),
                        "review".to_string(),
                    ],
                    evidence_ids: evidence_ids.into_iter().map(str::to_string).collect(),
                    gate_type: MergeGateType::Artifact,
                    owner: RuntimeOwner::default(),
                    validator: None,
                    policy_snapshot: MergeGatePolicySnapshot::default(),
                    decision: Some(MergeGateDecision {
                        outcome: MergeGateDecisionOutcome::Legacy,
                        reason: "core facts satisfied gate".to_string(),
                        owner: RuntimeOwner::default(),
                        evidence_ids: Vec::new(),
                        reviewed_evidence: Vec::new(),
                        review_request_id: None,
                        audit_id: "legacy".to_string(),
                        decided_at: 0,
                    }),
                    conflict: None,
                    applied_change_id: None,
                    recovery_snapshot: None,
                    audit_ids: Vec::new(),
                    updated_at: Some(1_700_000_050),
                },
            }
        }
        RuntimeEventKindExt::ContextPressure => viden_types::RuntimeEventKind::ContextUpdated {
            context: ContextBundleRecord {
                bundle_id: "ctx_bundle_pressure".to_string(),
                task_id: "task_context".to_string(),
                policy: "pressure-aware".to_string(),
                sources: vec![ContextSourceRecord {
                    name: "contract-spec".to_string(),
                    kind: "doc".to_string(),
                    priority: 1,
                    estimated_tokens: 23_000,
                    summary: "frontend contract spec".to_string(),
                    include_reason: "required by contract freeze".to_string(),
                    handle_id: Some("ctxh_contract_spec".to_string()),
                    item_id: Some("ctxi_contract_spec".to_string()),
                    view_id: Some("ctxv_contract_spec".to_string()),
                    content_sha256: Some("a".repeat(64)),
                    view_sha256: Some("b".repeat(64)),
                    quality_id: Some("ctxq_contract_spec".to_string()),
                }],
                omitted_sources: vec![ContextOmittedSourceRecord {
                    name: "large-history".to_string(),
                    kind: "transcript".to_string(),
                    estimated_tokens: 9_000,
                    reason: "hard_token_limit".to_string(),
                }],
                estimated_tokens: 92_000,
                largest_sources: vec!["contract-spec".to_string()],
                compaction_notes: vec!["omitted large-history".to_string()],
                soft_token_budget: 80_000,
                hard_token_limit: 100_000,
            },
        },
        RuntimeEventKindExt::CostUnknown => viden_types::RuntimeEventKind::CostUsageRecorded {
            cost: CostUsageRecord {
                usage_id: "usage_unknown_actual".to_string(),
                provider_id: "deepseek".to_string(),
                model: "deepseek-v4-flash".to_string(),
                scopes: vec![CostScope::AgentTask("task_context".to_string())],
                tokens: TokenUsage {
                    input_tokens: Some(100),
                    output_tokens: Some(25),
                    cached_input_tokens: Some(10),
                    retrieval_tokens: Some(5),
                    total_tokens: Some(125),
                },
                estimate: None,
                actual_cost: None,
                attempt_index: 1,
                outcome: CostUsageOutcome::Success,
                recorded_at: Some(1_700_000_060),
            },
        },
        RuntimeEventKindExt::TokenCostUnknown => viden_types::RuntimeEventKind::TokenCostUpdated {
            cost: TokenCostView {
                input_tokens: 100,
                output_tokens: 25,
                total_tokens: 125,
                cost_micro_usd: None,
            },
        },
        RuntimeEventKindExt::CommandRejected(command_id, reason) => {
            viden_types::RuntimeEventKind::CommandRejected {
                command_id: command_id.to_string(),
                reason: reason.to_string(),
            }
        }
        RuntimeEventKindExt::Error(message, recoverable) => viden_types::RuntimeEventKind::Error {
            error: RuntimeErrorView {
                message: message.to_string(),
                recoverable,
                hint: Some("retry after resolving contract request".to_string()),
            },
        },
    }
}

fn task(
    id: &str,
    role: AgentRole,
    status: AgentTaskStatus,
    activity: &str,
    next_action: Option<&str>,
) -> AgentTaskRecord {
    AgentTaskRecord {
        id: id.to_string(),
        parent_id: None,
        role,
        kind: AgentTaskKind::Agent,
        route: AgentRoute::Terminal,
        title: format!("{id} title"),
        status,
        activity: activity.to_string(),
        summary: format!("{id} summary"),
        progress: if status == AgentTaskStatus::Done {
            100
        } else {
            40
        },
        started_at: Some(1_700_000_000),
        updated_at: Some(1_700_000_040),
        workspace: Some(".worktrees/v3-core-runtime".to_string()),
        evidence: vec![format!("evidence_{id}")],
        permissions: vec!["ask".to_string()],
        decision: None,
        result: None,
        resume_handle: Some(format!("resume_{id}")),
        pid: None,
        next_action: next_action.map(|label| AgentNextAction {
            label: label.to_string(),
            command: Some("retry".to_string()),
            reason: Some("blocked dependency recovered".to_string()),
        }),
        owner: None,
    }
}

fn approval(id: &str, title: &str, is_mutating: bool) -> ApprovalRequestView {
    ApprovalRequestView {
        id: id.to_string(),
        tool_name: "shell".to_string(),
        title: title.to_string(),
        message: "Core requests scoped permission".to_string(),
        input_preview: "cargo test".to_string(),
        is_mutating,
        reason: Some("contract fixture coverage".to_string()),
        owner: RuntimeOwner {
            workspace_id: "workspace_contract_v1".to_string(),
            project_id: "project_viden".to_string(),
            lane_id: Some("lane_core".to_string()),
            session_id: Some("session_contract".to_string()),
            task_id: Some("task_contract".to_string()),
            turn_id: Some("turn_contract".to_string()),
        },
        risk: ApprovalRisk::High,
        target: ApprovalTarget {
            kind: "repo_path".to_string(),
            display: "crates/core".to_string(),
            canonical_ref: Some("repo://crates/core".to_string()),
        },
        allowed_scopes: vec![ApprovalScope::Once],
        policy_reason_key: "approval.contract_fixture".to_string(),
        policy_reason_args: BTreeMap::new(),
        expires_at: 1_700_003_600,
        default_action: ApprovalDefaultAction::Deny,
        audit_id: format!("audit_{id}"),
        decision_context: None,
    }
}

fn evidence(id: &str, kind: &str, summary: &str) -> EvidenceView {
    EvidenceView {
        id: id.to_string(),
        kind: kind.to_string(),
        summary: summary.to_string(),
        path: Some(format!("artifacts/{id}.txt")),
        source: Some("core".to_string()),
        canonical: None,
        metadata: None,
        timestamp: Some(1_700_000_070),
        owner: None,
    }
}

fn snapshot(work_mode: WorkMode) -> RuntimeSnapshot {
    RuntimeSnapshot {
        cwd: PathBuf::from("workspace/viden"),
        provider_family: "deepseek".to_string(),
        model_label: "deepseek-v4-flash".to_string(),
        work_mode,
        permission_mode: PermissionMode::Default,
        permission_level: PermissionLevel::Ask,
        config_summary: serde_json::json!({
            "ui": {
                "locales": ["en", "zh-CN"],
                "skin_mode_pairs": [
                    ["aurora", "dark"],
                    ["aurora", "light"],
                    ["ice", "dark"],
                    ["ice", "light"],
                    ["mono", "dark"],
                    ["mono", "light"],
                    ["amber", "dark"],
                    ["phosphor", "dark"]
                ],
                "density": ["compact", "regular", "comfy"],
                "motion": ["system", "reduced", "full"],
                "effective": UiPreferences {
                    locale: viden_types::LocaleId::ZhCn,
                    skin: UiSkin::Aurora,
                    mode: UiColorMode::Dark,
                    density: UiDensity::Regular,
                    motion: UiMotion::Reduced,
                }
            },
            "design_entry": {
                "hierarchy": [
                    "docs/viden-design/Viden/index.html",
                    "client design index",
                    "component library",
                    "TUI unified prototype or GUI D1 desktop cockpit"
                ],
                "tui": "TUI index -> component library -> unified prototype",
                "gui": "GUI index -> component library -> D1 desktop cockpit",
                "d11": "onboarding subordinate"
            }
        })
        .to_string(),
        loaded_config_files: vec![PathBuf::from("config/viden.toml")],
        startup_overrides: vec!["--provider=deepseek".to_string()],
        ui_preferences: ResolvedUiPreferences {
            locale: viden_types::LocaleId::ZhCn,
            skin: UiSkin::Aurora,
            mode: UiColorMode::Dark,
            density: UiDensity::Regular,
            motion: UiMotion::Reduced,
            diagnostics: Vec::new(),
        },
    }
}

fn typed_lanes_fixture() -> Vec<AgentLaneRecord> {
    parse_legacy_lanes_tsv(include_str!(
        "../../types/tests/fixtures/frontend-contract-v1/legacy-lanes.tsv"
    ))
}

/// GUI-CORE-027: the workspace has an operator identity, and everything scoped
/// by it carries the same two ids.
///
/// The failure this guards against is the one the `0.3.3` real task hit: a
/// commit made with no Lane selected had no actor, so it was refused. Here the
/// binding arrives first, a Lane created afterwards carries the same workspace
/// and project ids, a workspace-target commit settles under that owner with an
/// audit id, and the Lane's own source arrives as its own row rather than as
/// the workspace source.
#[test]
fn workspace_owner_fixture_scopes_every_fact_to_one_published_identity() {
    let name = "workspace-owner.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read workspace owner fixture bytes");
    let fixture_sha256 = format!("{:x}", Sha256::digest(&fixture_bytes));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(
        extension_manifest.contains(&format!(
            "workspace_owner_fixture_sha256 = \"{fixture_sha256}\""
        )),
        "the extension manifest must register the exact workspace owner fixture bytes"
    );
    assert!(extension_manifest.contains("workspace_owner_fixture = \"workspace-owner.json\""));

    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    let (view, first_cursor, first_digest) = replay_fixture(&fixture);
    let (second_view, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(view, second_view);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);
    assert!(
        extension_manifest.contains(&format!("workspace_owner_view_sha256 = \"{first_digest}\"")),
        "the extension manifest must register the replayed view digest"
    );

    // The published identity is the workspace scope: two ids and nothing else.
    let owner = view
        .workspace_owner
        .clone()
        .expect("the fixture publishes a workspace owner");
    assert!(!owner.workspace_id.is_empty());
    assert!(!owner.project_id.is_empty());
    assert_eq!(owner.lane_id, None);
    assert_eq!(owner.session_id, None);
    assert_eq!(owner.task_id, None);
    assert_eq!(owner.turn_id, None);

    // The binding says the project id was minted on this open, which is a
    // different fact from "this project already had one".
    let binding = fixture
        .events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::WorkspaceRuntimeOwnerBound { binding },
                ..
            }) => Some(binding.clone()),
            _ => None,
        })
        .expect("the fixture carries the binding");
    assert_eq!(binding.project_id_origin, ProjectIdOrigin::Minted);
    binding.validate().expect("the binding describes a scope");

    // A Lane created after the binding inherits both ids, so the Lane owner
    // and the workspace owner agree about which workspace they are in.
    let lane_binding = view
        .lane_runtime_owners
        .first()
        .expect("the fixture creates one Lane");
    assert_eq!(lane_binding.owner.workspace_id, owner.workspace_id);
    assert_eq!(lane_binding.owner.project_id, owner.project_id);
    assert_eq!(
        lane_binding.owner.lane_id.as_deref(),
        Some(lane_binding.lane_id.as_str())
    );

    // The workspace-target commit settled under the workspace owner, and the
    // audit row that authorized it names that same owner rather than nobody.
    let (audit_id, commit_owner) = fixture
        .events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind:
                    RuntimeEventKind::OperatorGitActionFinished {
                        target: SourceTarget::Workspace,
                        audit_id,
                        ..
                    },
                ..
            }) => Some((audit_id.clone(), envelope.owner.clone())),
            _ => None,
        })
        .expect("the fixture settles a workspace-target action");
    assert_eq!(commit_owner, owner);
    let audited = fixture
        .events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::AuditPageLoaded { page, .. },
                ..
            }) => page
                .records
                .iter()
                .find(|record| record.audit_id == audit_id)
                .cloned(),
            _ => None,
        })
        .expect("the authorization record is readable");
    assert_eq!(audited.owner, owner);
    assert_eq!(audited.action, "source.commit");

    // The Lane's own source is a Lane row. The workspace chip still describes
    // the workspace, which is the whole reason the row exists.
    let lane_source = view
        .lane_sources
        .get(&lane_binding.lane_id)
        .expect("the Lane has its own source row");
    assert_eq!(lane_source.branch.as_deref(), Some("codex/lane-workspace"));
    assert_ne!(
        view.workspace_source
            .as_ref()
            .and_then(|source| source.branch.as_deref()),
        lane_source.branch.as_deref(),
        "the workspace chip must not carry the Lane worktree's branch"
    );
}

#[test]
#[ignore = "manual workspace owner fixture refresh; normal tests validate committed JSON only"]
fn refresh_workspace_owner_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = workspace_owner_fixture();
    fs::write(
        root.join("workspace-owner.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

/// The cockpit layout record is stored, reset, and republished without ever
/// touching the appearance profile every frozen base fixture serializes.
///
/// Four facts a naive client would render identically and which are not the
/// same: a stored record, a record Core applied but could not write, a record
/// reset to the defaults, and a patch refused before anything was written.
#[test]
fn ui_layout_preferences_fixture_separates_stored_unpersisted_reset_and_refused() {
    let name = "ui-layout-preferences.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read layout preference fixture bytes");
    let fixture_sha256 = format!("{:x}", Sha256::digest(&fixture_bytes));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(
        extension_manifest.contains(&format!(
            "ui_layout_preferences_fixture_sha256 = \"{fixture_sha256}\""
        )),
        "the extension manifest must register the exact layout preference fixture bytes"
    );
    assert!(
        extension_manifest
            .contains("ui_layout_preferences_fixture = \"ui-layout-preferences.json\"")
    );

    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    let (view, first_cursor, first_digest) = replay_fixture(&fixture);
    let (second_view, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(view, second_view);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);
    assert!(
        extension_manifest.contains(&format!(
            "ui_layout_preferences_view_sha256 = \"{first_digest}\""
        )),
        "the extension manifest must register the replayed view digest"
    );

    // The reset is the last record, so the view ends on the defaults — which
    // `D-SIDEBAR` makes floating, not pinned.
    assert_eq!(
        view.layout_preferences,
        Some(UiLayoutPreferences::default())
    );
    assert_eq!(
        view.layout_preferences
            .as_ref()
            .map(|preferences| preferences.lane_sidebar_mode),
        Some(LaneSidebarMode::Floating)
    );
    // The separate record is the entire point: the resolved appearance profile
    // is exactly what the snapshot started with.
    assert_eq!(view.ui_preferences, fixture.initial_snapshot.ui_preferences);
    assert_eq!(
        view.snapshot.ui_preferences,
        fixture.initial_snapshot.ui_preferences
    );

    let records = fixture
        .events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind:
                    RuntimeEventKind::UiLayoutPreferencesUpdated {
                        command_id,
                        preferences,
                        persisted,
                        ..
                    },
                ..
            }) => Some((command_id.clone(), preferences.clone(), *persisted)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 3);

    // The snapshot prefix's copy carries no command id, because nobody asked,
    // and it starts from the floating default.
    assert_eq!(records[0].0, None);
    assert_eq!(records[0].1, UiLayoutPreferences::default());
    assert_eq!(records[0].1.lane_sidebar_mode, LaneSidebarMode::Floating);

    // A stored record: pinned — the *non-default* mode, so the fixture proves
    // a stored choice rather than repeating the default — with an unknown
    // segment kept verbatim. Core does not own the client's statusbar
    // vocabulary, so a name it cannot recognize is still the operator's
    // choice.
    assert_eq!(records[1].0.as_deref(), Some("layout_set_pinned"));
    assert_eq!(records[1].1.lane_sidebar_mode, LaneSidebarMode::Pinned);
    assert_eq!(
        records[1].1.hidden_statusbar_segments,
        vec!["cost".to_string(), "a-future-client-segment".to_string()]
    );
    assert!(!records[1].2, "this record did not reach the config file");

    // The reset lands back on the floating default and *is* persisted, which
    // is a different fact from the unpersisted record above.
    assert_eq!(records[2].0.as_deref(), Some("layout_reset"));
    assert_eq!(records[2].1, UiLayoutPreferences::default());
    assert_eq!(records[2].1.lane_sidebar_mode, LaneSidebarMode::Floating);
    assert!(records[2].2);
    assert_ne!(
        records[1].1.lane_sidebar_mode, records[2].1.lane_sidebar_mode,
        "the set and the reset must land on different modes, or the sequence \
         proves nothing about either"
    );

    // An over-bound list is refused before anything is written, by command id,
    // and never answered with a record a client would render as stored.
    let refusal = fixture
        .events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::CommandRejected { command_id, reason },
                ..
            }) if command_id == "layout_set_over_bound" => Some(reason.clone()),
            _ => None,
        })
        .expect("the over-bound patch is refused by command id");
    assert!(refusal.contains(&MAX_HIDDEN_STATUSBAR_SEGMENTS.to_string()));
}

#[test]
#[ignore = "manual layout preference fixture refresh; normal tests validate committed JSON only"]
fn refresh_ui_layout_preferences_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = ui_layout_preferences_fixture();
    fs::write(
        root.join("ui-layout-preferences.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

/// GUI-CORE-027, as bytes.
///
/// Every value is fixed and no machine path appears, so the fixture is
/// identical on every machine that regenerates it. The workspace and project
/// ids are the shapes Core mints — `ws_` plus 16 hex characters, `prj_` plus a
/// token — rather than real ones, because a real id names a real directory.
fn workspace_owner_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "workspace-owner";
    let workspace_owner = RuntimeOwner {
        workspace_id: "ws_4f3c1a09b8d27e65".to_string(),
        project_id: "prj_contract_v1_workspace".to_string(),
        lane_id: None,
        session_id: None,
        task_id: None,
        turn_id: None,
    };
    // A Lane created after the binding: its owner starts from the workspace
    // identity, so both ids are the same and only the Lane fields are added.
    let lane_owner = RuntimeOwner {
        lane_id: Some("lane_workspace_owner".to_string()),
        session_id: Some("session_workspace_owner".to_string()),
        task_id: Some("task_workspace_owner".to_string()),
        turn_id: Some("turn_workspace_owner".to_string()),
        ..workspace_owner.clone()
    };
    let lane = AgentLaneRecord {
        id: "lane_workspace_owner".to_string(),
        task_id: Some("task_workspace_owner".to_string()),
        role: AgentRole::Coder,
        route: AgentRoute::BuiltIn,
        gate_strength: GateStrength::Full,
        mutation_policy: MutationPolicy::ProposeOnly,
        worktree: Some("workspace/.worktrees/lane-workspace".to_string()),
        branch: Some("codex/lane-workspace".to_string()),
        target: ExecutionTarget::Local,
        data_egress: viden_types::DataEgressPolicy::Deny,
        status: LaneStatus::Running,
        budget: LaneBudget::default(),
        active_session_ids: vec!["session_workspace_owner".to_string()],
        summary: "Lane created under the published workspace identity".to_string(),
        evidence: Vec::new(),
        run_stats: None,
    };
    let workspace_source = WorkspaceSourceView {
        status: viden_types::WorkspaceSourceStatus::Ready,
        branch: Some("main".to_string()),
        worktree: Some("workspace/viden".to_string()),
        ahead: 1,
        behind: 0,
        added: 0,
        deleted: 0,
        dirty: false,
    };
    // The Lane worktree's own facts: a different branch and a dirty tree. If
    // this rode `WorkspaceSourceUpdated` the workspace chip would claim the
    // Lane's branch, which is the confusion the Lane row exists to end.
    let lane_source = WorkspaceSourceView {
        branch: Some("codex/lane-workspace".to_string()),
        worktree: Some("workspace/.worktrees/lane-workspace".to_string()),
        ahead: 0,
        added: 3,
        deleted: 1,
        dirty: true,
        ..workspace_source.clone()
    };
    let commit_audit = AuditRecord::sanitized(
        "audit_workspace_commit".to_string(),
        1_700_004_100,
        workspace_owner.clone(),
        AuditActor::Operator,
        "source.commit".to_string(),
        vec![AuditObjectRef {
            kind: AuditObjectRef::KIND_SOURCE.to_string(),
            id: "workspace".to_string(),
        }],
        AuditOutcome::Success,
        [
            ("phase".to_string(), "authorized".to_string()),
            ("tool".to_string(), "git_commit".to_string()),
        ]
        .into_iter()
        .collect(),
    )
    .expect("the audit record is well formed");

    let owned_events: Vec<(RuntimeEventKind, RuntimeOwner)> = vec![
        // The binding is the first fact after the snapshot, so a client that
        // only replays a snapshot still learns which owner to send.
        (
            RuntimeEventKind::WorkspaceRuntimeOwnerBound {
                binding: WorkspaceRuntimeOwnerBinding {
                    canonical_root: "workspace/viden".to_string(),
                    owner: workspace_owner.clone(),
                    project_id_origin: ProjectIdOrigin::Minted,
                },
            },
            workspace_owner.clone(),
        ),
        (
            RuntimeEventKind::WorkspaceSourceUpdated {
                source: workspace_source.clone(),
            },
            workspace_owner.clone(),
        ),
        (
            RuntimeEventKind::LaneUpdated { lane: lane.clone() },
            lane_owner.clone(),
        ),
        (
            RuntimeEventKind::LaneRuntimeOwnerBound {
                binding: LaneRuntimeOwnerBinding {
                    lane_id: "lane_workspace_owner".to_string(),
                    owner: lane_owner.clone(),
                },
            },
            lane_owner.clone(),
        ),
        // The commit that had nowhere to go before this capability.
        (
            RuntimeEventKind::CommandAccepted {
                command_id: "workspace_commit".to_string(),
                command: RuntimeCommand::RunOperatorGitAction {
                    owner: workspace_owner.clone(),
                    target: SourceTarget::Workspace,
                    action: OperatorGitAction::Commit {
                        message: "feat(core): commit without a Lane".to_string(),
                    },
                },
            },
            workspace_owner.clone(),
        ),
        (
            RuntimeEventKind::OperatorGitActionFinished {
                command_id: "workspace_commit".to_string(),
                target: SourceTarget::Workspace,
                action: OperatorGitAction::Commit {
                    message: "feat(core): commit without a Lane".to_string(),
                },
                outcome: OperatorGitOutcome::Completed {
                    output: "[main 9f8e7d6] feat(core): commit without a Lane\n 1 file \
                             changed, 4 insertions(+)"
                        .to_string(),
                    truncated: false,
                    source: WorkspaceSourceView {
                        ahead: 2,
                        ..workspace_source.clone()
                    },
                },
                audit_id: "audit_workspace_commit".to_string(),
            },
            workspace_owner.clone(),
        ),
        (
            RuntimeEventKind::WorkspaceSourceUpdated {
                source: WorkspaceSourceView {
                    ahead: 2,
                    ..workspace_source
                },
            },
            workspace_owner.clone(),
        ),
        // The authorization is readable: the audit row names the workspace
        // owner rather than nobody, which is what GUI-CORE-027 was about.
        (
            RuntimeEventKind::AuditPageLoaded {
                command_id: Some("workspace_audit_read".to_string()),
                page: AuditPage {
                    records: vec![commit_audit],
                    next_before: None,
                    complete: true,
                },
            },
            workspace_owner.clone(),
        ),
        // A Lane-target stage settles and publishes *that Lane's* source.
        (
            RuntimeEventKind::OperatorGitActionFinished {
                command_id: "lane_stage".to_string(),
                target: SourceTarget::Lane {
                    lane_id: "lane_workspace_owner".to_string(),
                },
                action: OperatorGitAction::Stage { paths: Vec::new() },
                outcome: OperatorGitOutcome::Completed {
                    output: String::new(),
                    truncated: false,
                    source: lane_source.clone(),
                },
                audit_id: "audit_lane_stage".to_string(),
            },
            lane_owner.clone(),
        ),
        (
            RuntimeEventKind::LaneSourceUpdated {
                lane_id: "lane_workspace_owner".to_string(),
                source: lane_source,
            },
            lane_owner,
        ),
    ];

    fixture(
        fixture_id,
        &[
            "runtime.approvals",
            "runtime.audit",
            "runtime.commands",
            "runtime.events",
            "runtime.lane_owner_projection",
            "runtime.operator_git",
            "runtime.snapshot",
            "runtime.typed_lanes",
            "runtime.workspace_owner",
        ],
        snapshot(WorkMode::Build),
        owned_envelopes_per_event(fixture_id, owned_events, 1_700_004_000),
    )
}

/// `ui.layout_preferences`, as bytes.
fn ui_layout_preferences_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "ui-layout-preferences";
    let owner = RuntimeOwner {
        workspace_id: "ws_4f3c1a09b8d27e65".to_string(),
        project_id: "prj_contract_v1_workspace".to_string(),
        lane_id: None,
        session_id: None,
        task_id: None,
        turn_id: None,
    };
    // Pinned is the *non-default* mode — `D-SIDEBAR` makes floating the
    // default — so the stored record here is distinguishable from an absent
    // one. A fixture that stored the default would prove nothing about
    // persistence.
    let pinned = UiLayoutPreferences {
        lane_sidebar_mode: LaneSidebarMode::Pinned,
        // An unknown segment name, kept verbatim: the statusbar vocabulary
        // belongs to the client, and a Core that stored only the names it knew
        // would quietly unhide everything a newer client hid.
        hidden_statusbar_segments: vec!["cost".to_string(), "a-future-client-segment".to_string()],
    };

    let kinds = vec![
        // The snapshot prefix's copy: the defaults — floating, per
        // `D-SIDEBAR` — and no command id, because nobody asked for it.
        RuntimeEventKind::UiLayoutPreferencesUpdated {
            command_id: None,
            preferences: UiLayoutPreferences::default(),
            persisted: true,
            diagnostics: Vec::new(),
        },
        RuntimeEventKind::CommandAccepted {
            command_id: "layout_set_pinned".to_string(),
            command: RuntimeCommand::SetUiLayoutPreferences {
                patch: UiLayoutPreferencePatch {
                    lane_sidebar_mode: Some(LaneSidebarMode::Pinned),
                    hidden_statusbar_segments: Some(vec![
                        "cost".to_string(),
                        "a-future-client-segment".to_string(),
                    ]),
                },
            },
        },
        // Applied for this session, but the config file could not be written.
        // `persisted: false` plus the reason is the whole difference between
        // "saved" and "saved until you restart".
        RuntimeEventKind::UiLayoutPreferencesUpdated {
            command_id: Some("layout_set_pinned".to_string()),
            preferences: pinned,
            persisted: false,
            diagnostics: vec![UiPreferenceDiagnostic::new(
                "ui.layout.not_persisted",
                "ui.layout",
                "ui.layout",
                Some("Failed to create config temp: read-only file system".to_string()),
            )],
        },
        // Refused before anything was written, by command id. An empty success
        // here would tell a client its preference was stored when it was not.
        RuntimeEventKind::CommandRejected {
            command_id: "layout_set_over_bound".to_string(),
            reason: "hidden statusbar segments exceed the bound of 16".to_string(),
        },
        RuntimeEventKind::CommandAccepted {
            command_id: "layout_reset".to_string(),
            command: RuntimeCommand::ResetUiLayoutPreferences,
        },
        RuntimeEventKind::UiLayoutPreferencesUpdated {
            command_id: Some("layout_reset".to_string()),
            preferences: UiLayoutPreferences::default(),
            persisted: true,
            diagnostics: Vec::new(),
        },
    ];

    fixture(
        fixture_id,
        &[
            "runtime.commands",
            "runtime.events",
            "runtime.snapshot",
            "ui.layout_preferences",
            "ui.preferences",
        ],
        snapshot(WorkMode::Build),
        owned_envelopes(fixture_id, owner, kinds, 1_700_004_200),
    )
}

/// Envelopes whose owner varies per event.
///
/// `owned_envelopes` fixes one owner for a whole fixture, which cannot express
/// a workspace-scoped fact standing beside a Lane-scoped one — the exact pair
/// `runtime.workspace_owner` exists to distinguish.
fn owned_envelopes_per_event(
    fixture_id: &str,
    events: Vec<(RuntimeEventKind, RuntimeOwner)>,
    base_timestamp: u64,
) -> Vec<RuntimeEventEnvelope> {
    events
        .into_iter()
        .enumerate()
        .map(|(index, (kind, owner))| {
            let sequence = index as u64 + 1;
            RuntimeEventEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                owner,
                cursor: EventCursor {
                    stream_id: format!("fixture:{fixture_id}"),
                    sequence,
                },
                event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
                    sequence,
                    Some(base_timestamp + sequence),
                    kind,
                )),
            }
        })
        .collect()
}

/// The turn bracket, and the queue that hangs off it (`runtime.turn_lifecycle`).
///
/// Four facts a client would otherwise have to guess at, and which this fixture
/// separates: a turn is running; a turn ended and its reply is settled; a
/// queued prompt left the queue and became a turn of its own; and a turn that
/// was cancelled released nothing behind it.
#[test]
fn turn_lifecycle_fixture_brackets_every_turn_and_drains_only_behind_a_completion() {
    let name = "turn-lifecycle.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read turn lifecycle fixture bytes");
    let fixture_sha256 = format!("{:x}", Sha256::digest(&fixture_bytes));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(
        extension_manifest.contains(&format!(
            "turn_lifecycle_fixture_sha256 = \"{fixture_sha256}\""
        )),
        "the extension manifest must register the exact turn lifecycle fixture bytes"
    );
    assert!(extension_manifest.contains("turn_lifecycle_fixture = \"turn-lifecycle.json\""));

    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    let (view, first_cursor, first_digest) = replay_fixture(&fixture);
    let (second_view, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(view, second_view);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);
    assert!(
        extension_manifest.contains(&format!("turn_lifecycle_view_sha256 = \"{first_digest}\"")),
        "the extension manifest must register the replayed view digest"
    );

    // Five turns, each opened once and closed once, and none of them left
    // running at the end.
    let opened = fixture
        .events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::TurnStarted { turn },
                ..
            }) => Some(turn.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let closed = fixture
        .events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind:
                    RuntimeEventKind::TurnFinished {
                        turn_id, outcome, ..
                    },
                ..
            }) => Some((turn_id.clone(), outcome.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(opened.len(), 5);
    assert_eq!(closed.len(), 5);
    for (turn, (closed_id, _)) in opened.iter().zip(closed.iter()) {
        assert_eq!(&turn.turn_id, closed_id, "a turn closes under its own id");
        assert_eq!(turn.owner.lane_id, None, "the composer turn names no Lane");
        assert_eq!(turn.owner.turn_id.as_deref(), Some(turn.turn_id.as_str()));
    }
    assert!(
        view.active_turns.is_empty(),
        "every turn in this fixture ended, so nothing is running"
    );

    // The reply is settled by the turn's end rather than left as residue: the
    // deltas of the first turn are gone from the unscoped stream.
    assert!(
        fixture.events.iter().any(|envelope| matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::AssistantDelta { .. },
                ..
            })
        )),
        "the fixture streams a reply"
    );
    assert!(view.assistant_stream.is_empty());

    // Two prompts queued while the second turn ran are drained behind its
    // completion, oldest first, each announced before the turn it started and
    // each naming the queue entry it came from.
    let dequeued = fixture
        .events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::InputDequeued { input_id },
                ..
            }) => Some(input_id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(dequeued, vec!["queued_first", "queued_second"]);
    assert_eq!(
        opened[2].source,
        TurnSource::QueuedInput {
            input_id: "queued_first".to_string()
        }
    );
    assert_eq!(
        opened[3].source,
        TurnSource::QueuedInput {
            input_id: "queued_second".to_string()
        }
    );
    let announced = fixture
        .events
        .iter()
        .position(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::InputDequeued { input_id },
                    ..
                }) if input_id == "queued_first"
            )
        })
        .expect("the first drain is announced");
    let ran = fixture
        .events
        .iter()
        .position(|envelope| matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::TurnStarted { turn },
                ..
            }) if turn.source == TurnSource::QueuedInput { input_id: "queued_first".to_string() }
        ))
        .expect("the first drained turn starts");
    assert!(
        announced < ran,
        "a client is never shown a turn quoting a queue entry it has not been told left the queue"
    );

    // The cancelled fifth turn releases nothing: the prompt queued before it
    // is still queued, and the snapshot prefix would re-list it.
    assert_eq!(closed[4].1, TurnOutcome::Cancelled);
    assert_eq!(view.queued_inputs.len(), 1);
    assert_eq!(view.queued_inputs[0].id, "queued_after_cancel");
    assert!(
        fixture
            .events
            .iter()
            .skip(announced)
            .all(|envelope| !matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::InputDequeued { input_id },
                    ..
                }) if input_id == "queued_after_cancel"
            )),
        "nothing runs behind a cancelled turn"
    );
}

#[test]
#[ignore = "manual turn lifecycle fixture refresh; normal tests validate committed JSON only"]
fn refresh_turn_lifecycle_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = turn_lifecycle_fixture();
    fs::write(
        root.join("turn-lifecycle.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

/// `runtime.turn_lifecycle`, as bytes.
///
/// Five turns over one session-scoped owner: a typed turn whose reply settles
/// on its own end, a second typed turn that two prompts are queued behind and
/// which drains both on completing, and a fifth that is cancelled and therefore
/// leaves the prompt queued behind it exactly where it is.
fn turn_lifecycle_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "turn-lifecycle";
    let owner = RuntimeOwner {
        workspace_id: "ws_4f3c1a09b8d27e65".to_string(),
        project_id: "prj_contract_v1_workspace".to_string(),
        lane_id: None,
        session_id: None,
        task_id: None,
        turn_id: None,
    };
    let turn_owner = |turn_id: &str| RuntimeOwner {
        turn_id: Some(turn_id.to_string()),
        ..owner.clone()
    };
    let started = |turn_id: &str, source: TurnSource, at: u64| {
        (
            RuntimeEventKind::TurnStarted {
                turn: TurnView {
                    turn_id: turn_id.to_string(),
                    owner: turn_owner(turn_id),
                    source,
                    started_at: at,
                },
            },
            turn_owner(turn_id),
        )
    };
    let finished = |turn_id: &str, outcome: TurnOutcome, at: u64| {
        (
            RuntimeEventKind::TurnFinished {
                turn_id: turn_id.to_string(),
                owner: turn_owner(turn_id),
                outcome,
                finished_at: at,
            },
            turn_owner(turn_id),
        )
    };
    let submit = |command_id: &str, content: &str| {
        (
            RuntimeEventKind::CommandAccepted {
                command_id: command_id.to_string(),
                command: RuntimeCommand::SubmitUserInput {
                    content: content.to_string(),
                },
            },
            owner.clone(),
        )
    };
    let queued = |input_id: &str, content: &str, at: u64| {
        (
            RuntimeEventKind::InputQueued {
                input: QueuedInputView {
                    id: input_id.to_string(),
                    content_preview: content.to_string(),
                    created_at: Some(at),
                    owner: Some(owner.clone()),
                },
            },
            owner.clone(),
        )
    };
    let dequeued = |input_id: &str| {
        (
            RuntimeEventKind::InputDequeued {
                input_id: input_id.to_string(),
            },
            owner.clone(),
        )
    };

    let owned_events: Vec<(RuntimeEventKind, RuntimeOwner)> = vec![
        // A typed turn, its streamed reply, and its end. The end is what
        // settles the reply; before this capability the stream simply kept it.
        submit("submit_first", "describe the change"),
        started("turn_first", TurnSource::UserInput, 1_700_005_002),
        (
            RuntimeEventKind::AssistantDelta {
                message_id: "message_first".to_string(),
                task_id: None,
                session_id: None,
                content: "Applied the ".to_string(),
            },
            owner.clone(),
        ),
        (
            RuntimeEventKind::AssistantDelta {
                message_id: "message_first".to_string(),
                task_id: None,
                session_id: None,
                content: "requested change.".to_string(),
            },
            owner.clone(),
        ),
        finished("turn_first", TurnOutcome::Completed, 1_700_005_005),
        // A second turn, with two prompts queued behind it while it runs.
        submit("submit_second", "now run the checks"),
        started("turn_second", TurnSource::UserInput, 1_700_005_007),
        queued("queued_first", "then commit it", 1_700_005_008),
        queued("queued_second", "then push it", 1_700_005_009),
        finished("turn_second", TurnOutcome::Completed, 1_700_005_010),
        // The drain: announced, then run, oldest first, each turn naming the
        // queue entry it came from so text nobody just typed is explicable.
        dequeued("queued_first"),
        started(
            "turn_queued_first",
            TurnSource::QueuedInput {
                input_id: "queued_first".to_string(),
            },
            1_700_005_012,
        ),
        finished("turn_queued_first", TurnOutcome::Completed, 1_700_005_013),
        dequeued("queued_second"),
        started(
            "turn_queued_second",
            TurnSource::QueuedInput {
                input_id: "queued_second".to_string(),
            },
            1_700_005_015,
        ),
        finished("turn_queued_second", TurnOutcome::Completed, 1_700_005_016),
        // A prompt queued behind a turn the operator then cancels. The
        // cancellation is a decision about that turn, not an instruction to
        // run what is waiting, so the queue stays exactly where it is.
        queued(
            "queued_after_cancel",
            "and open a pull request",
            1_700_005_017,
        ),
        submit("submit_third", "re-run the failing check"),
        started("turn_third", TurnSource::UserInput, 1_700_005_019),
        (
            RuntimeEventKind::CommandAccepted {
                command_id: "cancel_third".to_string(),
                command: RuntimeCommand::CancelActiveTurn,
            },
            owner.clone(),
        ),
        finished("turn_third", TurnOutcome::Cancelled, 1_700_005_021),
    ];

    fixture(
        fixture_id,
        &[
            "runtime.commands",
            "runtime.events",
            "runtime.queued_input",
            "runtime.snapshot",
            "runtime.turn_lifecycle",
        ],
        snapshot(WorkMode::Build),
        owned_envelopes_per_event(fixture_id, owned_events, 1_700_005_000),
    )
}

/// Canonical proof of the single-file read contract
/// (`runtime.workspace_file_reads`, C9).
///
/// Deliberately not a happy path. Two reads are outstanding at once and the
/// second is answered first, so a client correlating by arrival order
/// misattributes both. One answer is whole text, one is text the bound cut on a
/// character boundary, one is binary with no payload, one is a path that is not
/// there, and one is a directory — five different things a naive client would
/// render as the same empty editor. Two more reads are refused outright: a path
/// that leaves the target, and a `read_file` deny rule. A client that treated a
/// truncated body, a binary body, a missing path, a directory, and a refusal
/// alike would show an operator an empty file five times over, which is the
/// fabricated content this capability exists to prevent.
#[test]
fn workspace_file_reads_fixture_types_every_body_and_refuses_rather_than_faking_absence() {
    let name = "workspace-file-reads.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read workspace file reads fixture bytes");
    let fixture_sha256 = format!("{:x}", Sha256::digest(&fixture_bytes));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(
        extension_manifest.contains(&format!(
            "workspace_file_reads_fixture_sha256 = \"{fixture_sha256}\""
        )),
        "the extension manifest must register the exact workspace file reads fixture bytes"
    );
    assert!(
        extension_manifest.contains("workspace_file_reads_fixture = \"workspace-file-reads.json\"")
    );

    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    let (view, first_cursor, first_digest) = replay_fixture(&fixture);
    let (second_view, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(view, second_view);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);
    assert!(
        extension_manifest.contains(&format!(
            "workspace_file_reads_view_sha256 = \"{first_digest}\""
        )),
        "the extension manifest must register the replayed view digest"
    );

    let answers = fixture
        .events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::WorkspaceFileLoaded { command_id, file },
                ..
            }) => Some((command_id.clone(), file.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(answers.len(), 5);

    // Both reads are outstanding before either is answered, and the second is
    // answered first: a client correlating by arrival order attributes the
    // truncated body to the read that asked for the whole file.
    let accepted_order = fixture
        .events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind:
                    RuntimeEventKind::CommandAccepted {
                        command_id,
                        command: RuntimeCommand::ReadWorkspaceFile { .. },
                    },
                ..
            }) => Some(command_id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(accepted_order[0], "file_read_text");
    assert_eq!(accepted_order[1], "file_read_truncated");
    assert_eq!(answers[0].0, "file_read_truncated");
    assert_eq!(answers[1].0, "file_read_text");

    // Whole text: not truncated, and its digest covers the bytes it carries.
    let whole = &answers[1].1;
    let WorkspaceFileBody::Text { text, truncated } = &whole.content else {
        panic!("the first read answers text, got {:?}", whole.content);
    };
    assert!(!truncated);
    assert_eq!(whole.size, Some(text.len() as u64));
    assert_eq!(
        whole.sha256.as_deref(),
        Some(format!("{:x}", Sha256::digest(text.as_bytes())).as_str()),
    );

    // The cut body: the text is shorter than the file, the cut landed on a
    // character boundary, and `size`/`sha256` still describe the whole file —
    // which is what lets a client tell a truncated view of one revision from a
    // full view of another.
    let cut = &answers[0].1;
    let WorkspaceFileBody::Text { text, truncated } = &cut.content else {
        panic!("the second read answers text, got {:?}", cut.content);
    };
    assert!(truncated);
    assert!(cut.size.unwrap() > text.len() as u64);
    assert!(text.is_char_boundary(text.len()));
    assert!(
        !text.ends_with('\u{fffd}'),
        "a cut never yields replacements"
    );

    // Binary carries no payload, and still identifies the bytes.
    let binary = &answers[2].1;
    assert_eq!(binary.content, WorkspaceFileBody::Binary);
    assert!(binary.size.is_some() && binary.sha256.is_some());

    // The two absences are typed, and neither pretends to a size or a hash.
    for (index, reason) in [
        (3, WorkspaceFileUnavailableReason::NotFound),
        (4, WorkspaceFileUnavailableReason::Directory),
    ] {
        let absent = &answers[index].1;
        assert_eq!(
            absent.content,
            WorkspaceFileBody::Unavailable { reason },
            "absence is typed so a client's affordance can differ per case"
        );
        assert_eq!(
            (absent.size, absent.sha256.clone()),
            (None, None),
            "0 and an empty digest would render as a real empty file"
        );
    }

    // The two refusals answer their own reads and publish no body at all.
    let refusals = fixture
        .events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::CommandRejected { command_id, reason },
                ..
            }) => Some((command_id.clone(), reason.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(refusals.len(), 2);
    assert_eq!(refusals[0].0, "file_read_escape");
    assert!(
        refusals[0].1.contains("leaves the target"),
        "an escaping path is refused, never answered as a missing file"
    );
    assert!(
        !refusals[0].1.contains("not_found"),
        "a refusal must not masquerade as an absence"
    );
    assert_eq!(refusals[1].0, "file_read_denied");
    assert!(refusals[1].1.contains("read_file"));
    assert!(
        refusals[1].1.contains("grant the `read_file` permission"),
        "the refusal keeps the actionable grant hint"
    );

    // A file read is a query answer, never view state: applying every answer
    // in this fixture to a fresh view leaves it byte-identical, so publishing
    // one moves no snapshot digest and no frozen base fixture.
    let untouched = RuntimeViewState::new(fixture.initial_snapshot.clone());
    let mut only_answers = RuntimeViewState::new(fixture.initial_snapshot.clone());
    for envelope in &fixture.events {
        if let RuntimeWireEvent::Known(event) = &envelope.event
            && matches!(event.kind, RuntimeEventKind::WorkspaceFileLoaded { .. })
        {
            only_answers.apply_event(event);
        }
    }
    assert_eq!(
        canonical_view_sha256(&only_answers),
        canonical_view_sha256(&untouched),
        "no file read may reduce into RuntimeViewState"
    );
}

#[test]
#[ignore = "manual workspace file read fixture refresh; normal tests validate committed JSON only"]
fn refresh_workspace_file_reads_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = workspace_file_reads_fixture();
    fs::write(
        root.join("workspace-file-reads.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

/// `runtime.workspace_file_reads`, as bytes.
///
/// Seven reads over one session-scoped owner: whole text, text the bound cut on
/// a character boundary, binary, a path that is not there, a directory, a path
/// that leaves the target, and a `read_file` deny rule. The contents are fixed
/// strings with no machine path in them, so the bytes are identical on every
/// machine that regenerates this fixture.
fn workspace_file_reads_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "workspace-file-reads";
    let owner = RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden".to_string(),
        lane_id: None,
        session_id: Some("session_workspace_file_reads".to_string()),
        task_id: None,
        turn_id: Some("turn_workspace_file_reads".to_string()),
    };
    let digest = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
    let accepted =
        |command_id: &str, query: WorkspaceFileReadQuery| RuntimeEventKind::CommandAccepted {
            command_id: command_id.to_string(),
            command: RuntimeCommand::ReadWorkspaceFile { query },
        };
    let loaded =
        |command_id: &str, file: WorkspaceFileContent| RuntimeEventKind::WorkspaceFileLoaded {
            command_id: command_id.to_string(),
            file,
        };
    let unavailable = |path: &str, reason: WorkspaceFileUnavailableReason| WorkspaceFileContent {
        path: path.to_string(),
        size: None,
        sha256: None,
        content: WorkspaceFileBody::Unavailable { reason },
    };

    let whole_text = "pub const MAX_WORKSPACE_FILE_BYTES: u32 = 1024 * 1024;\n".to_string();
    // The bound below lands on the first byte of the two-byte `é`, so the cut
    // has to fall back to the character boundary before it.
    let long_text =
        "The bound cuts on a character boundary, never inside the caf\u{00e9} sign.".to_string();
    let boundary = long_text
        .find('\u{00e9}')
        .expect("the fixture text carries the multi-byte char");
    let cut_limit = boundary as u32 + 1;
    // A PNG signature: the leading NUL-free bytes still fail the decode, and
    // byte 0x89 is what a client must never be shown as text.
    let binary_bytes: Vec<u8> = vec![
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d,
    ];

    let kinds = vec![
        // Both reads are sent before either is answered.
        accepted(
            "file_read_text",
            WorkspaceFileReadQuery {
                target: SourceTarget::Workspace,
                path: "crates/types/src/workspace_files.rs".to_string(),
                byte_limit: None,
            },
        ),
        accepted(
            "file_read_truncated",
            WorkspaceFileReadQuery {
                target: SourceTarget::Workspace,
                path: "docs/frontend-integration-contract.md".to_string(),
                byte_limit: Some(cut_limit),
            },
        ),
        // The second read is answered first, so a client correlating by
        // arrival order attributes the cut body to the whole-file read.
        loaded(
            "file_read_truncated",
            WorkspaceFileContent {
                path: "docs/frontend-integration-contract.md".to_string(),
                size: Some(long_text.len() as u64),
                sha256: Some(digest(long_text.as_bytes())),
                content: WorkspaceFileBody::Text {
                    text: long_text[..boundary].to_string(),
                    truncated: true,
                },
            },
        ),
        loaded(
            "file_read_text",
            WorkspaceFileContent {
                path: "crates/types/src/workspace_files.rs".to_string(),
                size: Some(whole_text.len() as u64),
                sha256: Some(digest(whole_text.as_bytes())),
                content: WorkspaceFileBody::Text {
                    text: whole_text.clone(),
                    truncated: false,
                },
            },
        ),
        // A Lane target: Core resolved the worktree from its own records and
        // the client passed no path.
        accepted(
            "file_read_binary",
            WorkspaceFileReadQuery {
                target: SourceTarget::Lane {
                    lane_id: "lane_workspace_file_reads".to_string(),
                },
                path: "docs/viden-design/Viden/assets/cockpit.png".to_string(),
                byte_limit: None,
            },
        ),
        loaded(
            "file_read_binary",
            WorkspaceFileContent {
                path: "docs/viden-design/Viden/assets/cockpit.png".to_string(),
                size: Some(binary_bytes.len() as u64),
                sha256: Some(digest(&binary_bytes)),
                content: WorkspaceFileBody::Binary,
            },
        ),
        // Absent and directory: two facts about the tree, not two failures.
        accepted(
            "file_read_missing",
            WorkspaceFileReadQuery {
                target: SourceTarget::Workspace,
                path: "crates/types/src/removed.rs".to_string(),
                byte_limit: None,
            },
        ),
        loaded(
            "file_read_missing",
            unavailable(
                "crates/types/src/removed.rs",
                WorkspaceFileUnavailableReason::NotFound,
            ),
        ),
        accepted(
            "file_read_directory",
            WorkspaceFileReadQuery {
                target: SourceTarget::Workspace,
                path: "crates/types/src".to_string(),
                byte_limit: None,
            },
        ),
        loaded(
            "file_read_directory",
            unavailable(
                "crates/types/src",
                WorkspaceFileUnavailableReason::Directory,
            ),
        ),
        // A path that leaves the target. Refused, and deliberately not
        // answered as `not_found`: "you may not ask that" and "your tree does
        // not contain it" are different facts.
        accepted(
            "file_read_escape",
            WorkspaceFileReadQuery {
                target: SourceTarget::Workspace,
                path: "crates/../../etc/passwd".to_string(),
                byte_limit: None,
            },
        ),
        RuntimeEventKind::CommandRejected {
            command_id: "file_read_escape".to_string(),
            reason: "workspace file path `crates/../../etc/passwd` leaves the target".to_string(),
        },
        // A `read_file` deny rule. The refusal names the gate and folds the
        // actionable hint into the reason, because `CommandRejected` has no
        // `hint` field and a refusal an operator cannot act on is worse.
        accepted(
            "file_read_denied",
            WorkspaceFileReadQuery {
                target: SourceTarget::Workspace,
                path: "viden.toml".to_string(),
                byte_limit: None,
            },
        ),
        RuntimeEventKind::CommandRejected {
            command_id: "file_read_denied".to_string(),
            reason: "Permission decision:\nSummary: decision=deny\n\nDetails\ntool: read_file\n\
                     reason: RuleDeny\nmessage: Denied by permission rule for read_file\n\
                     hint: grant the `read_file` permission to open workspace files from this \
                     client"
                .to_string(),
        },
    ];

    fixture(
        fixture_id,
        &[
            "runtime.commands",
            "runtime.events",
            "runtime.snapshot",
            "runtime.workspace_file_reads",
        ],
        snapshot(WorkMode::Build),
        owned_envelopes(fixture_id, owner, kinds, 1_700_006_000),
    )
}

/// Durable work evidence: the archive an applied mutation actually leaves
/// behind (`runtime.durable_work_evidence`).
///
/// The `0.3.3` real task stopped here. An operator approved a typed edit, the
/// edit applied, and then there was nothing to review: the archive was empty and
/// the audit id the approval had shown named no row. This fixture is the whole
/// loop that fact-checks itself — the decision, its durable row, the archived
/// patch with canonical bytes, the page that finds it, the content that verifies
/// against the hash the row published, and an agent-reported patch the runtime
/// completes on ingestion.
#[test]
fn durable_work_evidence_fixture_closes_the_loop_from_approval_to_verified_bytes() {
    let name = "durable-work-evidence.json";
    let root = fixture_root();
    let fixture_bytes =
        fs::read(root.join(name)).expect("read durable work evidence fixture bytes");
    let fixture_sha256 = format!("{:x}", Sha256::digest(&fixture_bytes));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(
        extension_manifest.contains(&format!(
            "durable_work_evidence_fixture_sha256 = \"{fixture_sha256}\""
        )),
        "the extension manifest must register the exact durable work evidence fixture bytes"
    );
    assert!(
        extension_manifest
            .contains("durable_work_evidence_fixture = \"durable-work-evidence.json\"")
    );

    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    let (view, first_cursor, first_digest) = replay_fixture(&fixture);
    let (second_view, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(view, second_view);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);
    assert!(
        extension_manifest.contains(&format!(
            "durable_work_evidence_view_sha256 = \"{first_digest}\""
        )),
        "the extension manifest must register the replayed view digest"
    );

    let evidence_rows = fixture
        .events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::EvidenceRecorded { evidence },
                ..
            }) if evidence.kind == "patch" => Some(evidence.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        evidence_rows.len(),
        2,
        "one native patch and one agent patch"
    );
    let native = &evidence_rows[0];
    let agent = &evidence_rows[1];

    // Every archived patch names bytes Core can serve. A row without a
    // canonical reference is display-only evidence, and the merge gate already
    // refuses that; this capability exists so an applied mutation is never it.
    let native_canonical = native
        .canonical
        .as_ref()
        .expect("the native patch carries canonical bytes");
    let agent_canonical = agent
        .canonical
        .as_ref()
        .expect("the runtime completed the agent patch the adapter could not");
    assert_eq!(native_canonical.producer.identity, "native");
    assert_eq!(agent_canonical.producer.identity, "agent");

    // The native producer names the Lane's task, which is what lets its row
    // satisfy that Lane's `patch` merge gate. A session-scoped turn would name
    // its turn here and could never satisfy one.
    let native_owner = native.owner.as_ref().expect("the row names its owner");
    assert_eq!(
        Some(native_canonical.producer.task_id.as_str()),
        native_owner.task_id.as_deref()
    );
    assert!(native_owner.turn_id.is_some(), "the row names its turn");

    // An operator receipt is a fact about the native path only. The adapter
    // patch carries none, because the ACP permission bridge mints no audit id
    // it could name, and inventing one would make it look approved.
    let approval_audit_id = native_canonical
        .permission_snapshot_id
        .clone()
        .expect("the native patch names the approval that allowed it");
    assert_eq!(agent_canonical.permission_snapshot_id, None);

    // That receipt resolves: the id on `ApprovalResolved` is the id of a row
    // `QueryAudit` returns. Before this capability it named nothing at all.
    assert!(fixture.events.iter().any(|envelope| matches!(
        &envelope.event,
        RuntimeWireEvent::Known(RuntimeEvent {
            kind: RuntimeEventKind::ApprovalResolved { audit_id, .. },
            ..
        }) if audit_id == &approval_audit_id
    )));
    let audited = fixture
        .events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::AuditPageLoaded { page, .. },
                ..
            }) => Some(page.clone()),
            _ => None,
        })
        .expect("the audit read is answered");
    let record = audited
        .records
        .iter()
        .find(|record| record.audit_id == approval_audit_id)
        .expect("the approval decision is a durable audit row");
    assert_eq!(record.action, "approval.allow_once");
    assert_eq!(record.actor, AuditActor::Operator);
    assert!(
        record
            .objects
            .iter()
            .any(|object| object.kind == "tool" && object.id == "edit_file"),
        "the row names what was allowed"
    );

    // Both canonicalizations are announced, each after the row it completes.
    let canonicalized = fixture
        .events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind:
                    RuntimeEventKind::EvidenceCanonicalized {
                        evidence_id,
                        content_sha256,
                        ..
                    },
                ..
            }) => Some((evidence_id.clone(), content_sha256.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        canonicalized,
        vec![
            (native.id.clone(), native_canonical.source_hash.clone()),
            (agent.id.clone(), agent_canonical.source_hash.clone()),
        ]
    );

    // The read path serves the native patch as a diff whose hash is the one the
    // row published, which is the verification a reviewer is relying on.
    let content = fixture
        .events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::EvidenceContentLoaded { content, .. },
                ..
            }) => Some(content.clone()),
            _ => None,
        })
        .expect("the content read is answered");
    let EvidenceContent::Diff { sha256, document } = content else {
        panic!("a patch row's canonical bytes are served as a diff");
    };
    assert_eq!(sha256, native_canonical.source_hash);
    assert_eq!(document.files.len(), 1);

    // The live cockpit change is published before the archived row that
    // describes it, never after: a reviewable artifact must not arrive before
    // the change it belongs to.
    let change = fixture
        .events
        .iter()
        .position(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::WorkspaceChangeUpdated { .. },
                    ..
                })
            )
        })
        .expect("the live workspace change is published");
    let archived = fixture
        .events
        .iter()
        .position(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::EvidenceRecorded { evidence },
                    ..
                }) if evidence.id == native.id
            )
        })
        .expect("the archived row is published");
    assert!(change < archived);
}

#[test]
#[ignore = "manual durable work evidence fixture refresh; normal tests validate committed JSON only"]
fn refresh_durable_work_evidence_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = durable_work_evidence_fixture();
    fs::write(
        root.join("durable-work-evidence.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

/// `runtime.durable_work_evidence`, as bytes.
///
/// One Lane turn that edits a file behind an approval the operator allows, then
/// the three reads that prove the work is reviewable afterwards, then an agent
/// adapter's own patch fact being completed by the runtime's ingestion of it.
fn durable_work_evidence_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "durable-work-evidence";
    let owner = RuntimeOwner {
        workspace_id: "ws_4f3c1a09b8d27e65".to_string(),
        project_id: "prj_contract_v1_workspace".to_string(),
        lane_id: Some("lane_durable_work".to_string()),
        session_id: Some("session_durable_work".to_string()),
        task_id: Some("task_durable_work".to_string()),
        turn_id: None,
    };
    let turn_owner = RuntimeOwner {
        turn_id: Some("turn_durable_work".to_string()),
        ..owner.clone()
    };
    let agent_owner = RuntimeOwner {
        lane_id: Some("lane_durable_work_agent".to_string()),
        session_id: Some("session_durable_work_agent".to_string()),
        task_id: Some("acp-session-durable".to_string()),
        turn_id: Some("turn_durable_work_agent".to_string()),
        ..owner.clone()
    };

    let native_diff = "--- a/crates/types/src/evidence_reads.rs\n+++ \
                       b/crates/types/src/evidence_reads.rs\n@@ -12,3 +12,3 @@ impl \
                       EvidenceQuery\n     pub fn clamped_limit(&self) -> usize {\n-        \
                       self.limit as usize\n+        self.limit.clamp(1, 200) as \
                       usize\n     }\n";
    let agent_diff = "--- a/docs/core-0.3-compatibility.md\n+++ \
                      b/docs/core-0.3-compatibility.md\n@@ -4,2 +4,2 @@\n-Capabilities: \
                      26\n+Capabilities: 27\n";
    let native_hash = format!("{:x}", Sha256::digest(native_diff.as_bytes()));
    let agent_hash = format!("{:x}", Sha256::digest(agent_diff.as_bytes()));
    let approval_audit_id = "audit_durable_work_approval";
    let approval_request_id = "approval_durable_work";

    let native_canonical = CanonicalEvidenceReference {
        item_id: "ctxi_durable_work_native".to_string(),
        bundle_id: "bundle_durable_work".to_string(),
        source_hash: native_hash.clone(),
        producer: EvidenceProducer {
            identity: "native".to_string(),
            role: "coder".to_string(),
            task_id: "task_durable_work".to_string(),
        },
        permission_snapshot_id: Some(approval_audit_id.to_string()),
        permission_scope: ContextScope::Task("task_durable_work".to_string()),
        evidence_scope: ContextScope::Task("task_durable_work".to_string()),
        verification: EvidenceVerificationState::Verified,
        quality: EvidenceQualityFacts {
            status: EvidenceQualityStatus::Pass,
            reason_codes: Vec::new(),
        },
    };
    let native_row = EvidenceView {
        id: "patch-tool_durable_work_edit".to_string(),
        kind: "patch".to_string(),
        summary: "native lane lane_durable_work edit: crates/types/src/evidence_reads.rs (+1/-1)"
            .to_string(),
        path: Some("crates/types/src/evidence_reads.rs".to_string()),
        source: Some("native".to_string()),
        canonical: Some(native_canonical.clone()),
        metadata: None,
        timestamp: Some(1_700_006_010),
        owner: Some(turn_owner.clone()),
    };
    let agent_row = EvidenceView {
        id: "acp-patch-tool_durable_work_agent-7".to_string(),
        kind: "patch".to_string(),
        summary: "ACP patch: 1 file(s), +1/-1, first docs/core-0.3-compatibility.md".to_string(),
        path: Some("docs/core-0.3-compatibility.md".to_string()),
        source: Some("acp:patch.v1".to_string()),
        canonical: Some(CanonicalEvidenceReference {
            item_id: "ctxi_durable_work_agent".to_string(),
            // No context bundle backs an adapter patch, so the store handle
            // stands in rather than a bundle id Core would have to invent.
            bundle_id: "ctxi_durable_work_agent".to_string(),
            source_hash: agent_hash.clone(),
            producer: EvidenceProducer {
                identity: "agent".to_string(),
                role: "coder".to_string(),
                task_id: "acp-session-durable".to_string(),
            },
            permission_snapshot_id: None,
            permission_scope: ContextScope::Task("acp-session-durable".to_string()),
            evidence_scope: ContextScope::Task("acp-session-durable".to_string()),
            verification: EvidenceVerificationState::Verified,
            quality: EvidenceQualityFacts {
                status: EvidenceQualityStatus::Pass,
                reason_codes: Vec::new(),
            },
        }),
        // The adapter's own metadata is kept verbatim: the runtime completes
        // the row, it does not rewrite what the adapter reported.
        metadata: Some(serde_json::json!({
            "schema": "acp.patch.v1",
            "format": "unified_diff",
            "fileCount": 1,
            "additions": 1,
            "deletions": 1,
            "diff": agent_diff,
        })),
        timestamp: Some(1_700_006_040),
        owner: Some(agent_owner.clone()),
    };

    let audit_row = AuditRecord::sanitized(
        approval_audit_id.to_string(),
        1_700_006_006,
        owner.clone(),
        AuditActor::Operator,
        "approval.allow_once".to_string(),
        vec![
            AuditObjectRef::new(AuditObjectRef::KIND_PERMISSION, approval_request_id),
            AuditObjectRef::new("tool", "edit_file"),
            AuditObjectRef::new("job", "cmd_durable_work_submit"),
        ],
        AuditOutcome::Success,
        BTreeMap::from([("scope".to_string(), "allow_once".to_string())]),
    )
    .expect("fixture audit records must satisfy the sanitization bounds");

    let owned_events: Vec<(RuntimeEventKind, RuntimeOwner)> = vec![
        (
            RuntimeEventKind::CommandAccepted {
                command_id: "cmd_durable_work_submit".to_string(),
                command: RuntimeCommand::SubmitUserInput {
                    content: "tighten the evidence page bound".to_string(),
                },
            },
            owner.clone(),
        ),
        (
            RuntimeEventKind::TurnStarted {
                turn: TurnView {
                    turn_id: "turn_durable_work".to_string(),
                    owner: turn_owner.clone(),
                    source: TurnSource::UserInput,
                    started_at: 1_700_006_002,
                },
            },
            turn_owner.clone(),
        ),
        // The mutation is asked for, and the request already names the audit id
        // the decision will be written under.
        (
            RuntimeEventKind::ApprovalRequested {
                approval: ApprovalRequestView {
                    id: approval_request_id.to_string(),
                    tool_name: "edit_file".to_string(),
                    title: "Approve edit_file".to_string(),
                    message: "edit crates/types/src/evidence_reads.rs".to_string(),
                    input_preview: "path=crates/types/src/evidence_reads.rs".to_string(),
                    is_mutating: true,
                    reason: Some("edit crates/types/src/evidence_reads.rs".to_string()),
                    owner: owner.clone(),
                    risk: ApprovalRisk::Medium,
                    target: ApprovalTarget {
                        kind: "edit_file".to_string(),
                        display: "crates/types/src/evidence_reads.rs".to_string(),
                        canonical_ref: Some("crates/types/src/evidence_reads.rs".to_string()),
                    },
                    allowed_scopes: vec![ApprovalScope::Once],
                    policy_reason_key: "permission.requires_approval".to_string(),
                    policy_reason_args: BTreeMap::new(),
                    expires_at: 1_700_006_300,
                    default_action: ApprovalDefaultAction::Deny,
                    audit_id: approval_audit_id.to_string(),
                    decision_context: None,
                },
            },
            owner.clone(),
        ),
        (
            RuntimeEventKind::CommandAccepted {
                command_id: "cmd_durable_work_approve".to_string(),
                command: RuntimeCommand::RespondToApproval {
                    request_id: approval_request_id.to_string(),
                    response: ApprovalResponse::allow_once(None),
                },
            },
            owner.clone(),
        ),
        (
            RuntimeEventKind::ApprovalResolved {
                request_id: approval_request_id.to_string(),
                decision: ApprovalDecision::Allow {
                    scope: ApprovalScope::Once,
                },
                owner: owner.clone(),
                audit_id: approval_audit_id.to_string(),
            },
            owner.clone(),
        ),
        // The decision is durable before anything reads it back.
        (
            RuntimeEventKind::CommandAccepted {
                command_id: "cmd_durable_work_audit".to_string(),
                command: RuntimeCommand::QueryAudit {
                    query: AuditQuery {
                        limit: 5,
                        ..AuditQuery::default()
                    },
                },
            },
            owner.clone(),
        ),
        (
            RuntimeEventKind::AuditPageLoaded {
                command_id: Some("cmd_durable_work_audit".to_string()),
                page: AuditPage {
                    records: vec![audit_row],
                    next_before: None,
                    complete: true,
                },
            },
            owner.clone(),
        ),
        // The applied mutation: the cockpit's live view of the tree first, the
        // archived artifact that describes the same change after it.
        (
            RuntimeEventKind::WorkspaceChangeUpdated {
                change: WorkspaceChangeView {
                    id: "tool_durable_work_edit:crates/types/src/evidence_reads.rs".to_string(),
                    owner: turn_owner.clone(),
                    path: "crates/types/src/evidence_reads.rs".to_string(),
                    kind: WorkspaceChangeKind::Modified,
                    patch: Some(native_diff.to_string()),
                    additions: 1,
                    deletions: 1,
                    diff: None,
                },
            },
            turn_owner.clone(),
        ),
        (
            RuntimeEventKind::EvidenceRecorded {
                evidence: native_row.clone(),
            },
            turn_owner.clone(),
        ),
        (
            RuntimeEventKind::EvidenceCanonicalized {
                evidence_id: native_row.id.clone(),
                item_id: native_canonical.item_id.clone(),
                content_sha256: native_hash.clone(),
            },
            turn_owner.clone(),
        ),
        (
            RuntimeEventKind::TurnFinished {
                turn_id: "turn_durable_work".to_string(),
                owner: turn_owner.clone(),
                outcome: TurnOutcome::Completed,
                finished_at: 1_700_006_020,
            },
            turn_owner.clone(),
        ),
        // The archive answers after the turn, which is the read that was empty
        // before this capability existed.
        (
            RuntimeEventKind::CommandAccepted {
                command_id: "cmd_durable_work_page".to_string(),
                command: RuntimeCommand::QueryEvidence {
                    query: EvidenceQuery {
                        kinds: vec!["patch".to_string()],
                        limit: 10,
                        ..EvidenceQuery::default()
                    },
                },
            },
            owner.clone(),
        ),
        (
            RuntimeEventKind::EvidencePageLoaded {
                command_id: "cmd_durable_work_page".to_string(),
                page: EvidencePage {
                    entries: vec![native_row.clone()],
                    complete: true,
                    next_after: None,
                },
            },
            owner.clone(),
        ),
        (
            RuntimeEventKind::CommandAccepted {
                command_id: "cmd_durable_work_content".to_string(),
                command: RuntimeCommand::ReadEvidenceContent {
                    evidence_id: native_row.id.clone(),
                },
            },
            owner.clone(),
        ),
        (
            RuntimeEventKind::EvidenceContentLoaded {
                command_id: "cmd_durable_work_content".to_string(),
                evidence_id: native_row.id.clone(),
                content: EvidenceContent::Diff {
                    document: DiffDocument {
                        files: vec![DiffFile {
                            path: "crates/types/src/evidence_reads.rs".to_string(),
                            old_path: None,
                            kind: WorkspaceChangeKind::Modified,
                            binary: false,
                            omitted: false,
                            additions: 1,
                            deletions: 1,
                            hunks: vec![DiffHunk {
                                old_start: 12,
                                old_lines: 3,
                                new_start: 12,
                                new_lines: 3,
                                header: Some("impl EvidenceQuery".to_string()),
                                lines: vec![
                                    DiffLine {
                                        kind: DiffLineKind::Context,
                                        content: "    pub fn clamped_limit(&self) -> usize {"
                                            .to_string(),
                                        old_line: Some(12),
                                        new_line: Some(12),
                                    },
                                    DiffLine {
                                        kind: DiffLineKind::Removed,
                                        content: "        self.limit as usize".to_string(),
                                        old_line: Some(13),
                                        new_line: None,
                                    },
                                    DiffLine {
                                        kind: DiffLineKind::Added,
                                        content: "        self.limit.clamp(1, 200) as usize"
                                            .to_string(),
                                        old_line: None,
                                        new_line: Some(13),
                                    },
                                    DiffLine {
                                        kind: DiffLineKind::Context,
                                        content: "    }".to_string(),
                                        old_line: Some(14),
                                        new_line: Some(14),
                                    },
                                ],
                            }],
                        }],
                        truncated: false,
                        byte_limit: 256 * 1024,
                    },
                    sha256: native_hash,
                },
            },
            owner.clone(),
        ),
        // The adapter's patch, as the runtime publishes it after storing the
        // bytes the adapter carried in `metadata` and could not store itself.
        (
            RuntimeEventKind::EvidenceRecorded {
                evidence: agent_row.clone(),
            },
            agent_owner.clone(),
        ),
        (
            RuntimeEventKind::EvidenceCanonicalized {
                evidence_id: agent_row.id.clone(),
                item_id: "ctxi_durable_work_agent".to_string(),
                content_sha256: agent_hash,
            },
            agent_owner,
        ),
    ];

    fixture(
        fixture_id,
        &[
            "runtime.audit",
            "runtime.cockpit_context_v1",
            "runtime.commands",
            "runtime.durable_work_evidence",
            "runtime.events",
            "runtime.evidence_reads",
            "runtime.snapshot",
            "runtime.turn_lifecycle",
        ],
        snapshot(WorkMode::Build),
        owned_envelopes_per_event(fixture_id, owned_events, 1_700_006_000),
    )
}

/// Canonical proof of the owner-scoped transcript rows contract
/// (`runtime.transcript_rows`, C8, closes GUI-CORE-009).
///
/// Deliberately not a happy path. Three reads are outstanding at once over one
/// durable transcript, and the second is answered first, so a client
/// correlating by arrival order attributes one Lane's conversation to the other.
/// Two Lanes and the session each get their own page and no row appears on more
/// than one of them, which is the isolation the base `runtime.transcript_page`
/// cannot express at all. One Lane's page is cut by the limit and its `older`
/// cursor is passed straight back to reach the page above it, tiling without
/// repeating the boundary row. One assistant body is over the byte bound, says
/// so, and names the canonical evidence row that still holds it whole. And one
/// read is refused outright for a cursor this build did not issue, because "this
/// query was malformed" and "nothing was said" must never render the same — the
/// fabricated absence this capability exists to end.
#[test]
fn transcript_rows_fixture_scopes_three_owners_and_tiles_without_repeating_a_row() {
    let name = "transcript-rows.json";
    let root = fixture_root();
    let fixture_bytes = fs::read(root.join(name)).expect("read transcript rows fixture bytes");
    let fixture_sha256 = format!("{:x}", Sha256::digest(&fixture_bytes));
    let extension_manifest = include_str!("../frontend-contract-extensions.toml");
    assert!(
        extension_manifest.contains(&format!(
            "transcript_rows_fixture_sha256 = \"{fixture_sha256}\""
        )),
        "the extension manifest must register the exact transcript rows fixture bytes"
    );
    assert!(extension_manifest.contains("transcript_rows_fixture = \"transcript-rows.json\""));

    let fixture = read_fixture(&root, name);
    assert_fixture_identity(name, &fixture);
    assert_capabilities_are_sorted_unique_and_advertised(name, &fixture);
    assert_cursors_are_contiguous(name, &fixture);

    let (view, first_cursor, first_digest) = replay_fixture(&fixture);
    let (second_view, second_cursor, second_digest) = replay_fixture(&fixture);
    assert_eq!(view, second_view);
    assert_eq!(first_cursor, second_cursor);
    assert_eq!(first_digest, second_digest);
    assert_eq!(first_cursor, fixture.expected_final_cursor);
    assert_eq!(first_digest, fixture.expected_view_sha256);
    assert!(
        extension_manifest.contains(&format!("transcript_rows_view_sha256 = \"{first_digest}\"")),
        "the extension manifest must register the replayed view digest"
    );

    let pages = fixture
        .events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::TranscriptRowsLoaded { command_id, page },
                ..
            }) => Some((command_id.clone(), page.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(pages.len(), 4, "three scoped reads plus one older page");

    // Arrival order is not request order: a client that attributed the first
    // answer to the first read would render Lane B's conversation under Lane A.
    let accepted_order = fixture
        .events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind:
                    RuntimeEventKind::CommandAccepted {
                        command_id,
                        command: RuntimeCommand::QueryTranscriptRows { .. },
                    },
                ..
            }) => Some(command_id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(accepted_order[0], "rows_lane_a");
    assert_eq!(pages[0].0, "rows_lane_b");

    let page_for = |command_id: &str| {
        pages
            .iter()
            .find(|(answered, _)| answered == command_id)
            .map(|(_, page)| page.clone())
            .unwrap_or_else(|| panic!("no page answered `{command_id}`"))
    };

    // Owner isolation, proved by the rows themselves rather than by the query:
    // every row of every page names the Lane or the session it was read for,
    // and the three row sets are disjoint.
    let lane_a = page_for("rows_lane_a");
    let lane_a_older = page_for("rows_lane_a_older");
    let lane_b = page_for("rows_lane_b");
    let session = page_for("rows_session");
    for row in lane_a.rows.iter().chain(lane_a_older.rows.iter()) {
        assert_eq!(row.owner.lane_id.as_deref(), Some("lane_transcript_rows_a"));
    }
    for row in &lane_b.rows {
        assert_eq!(row.owner.lane_id.as_deref(), Some("lane_transcript_rows_b"));
    }
    for row in &session.rows {
        assert_eq!(
            row.owner.lane_id, None,
            "a session-scoped row must never carry a Lane"
        );
        assert_eq!(
            row.owner.session_id.as_deref(),
            Some("session_transcript_rows")
        );
    }
    let ids = lane_a
        .rows
        .iter()
        .chain(lane_a_older.rows.iter())
        .chain(lane_b.rows.iter())
        .chain(session.rows.iter())
        .map(|row| row.id.clone())
        .collect::<Vec<_>>();
    let unique = ids.iter().cloned().collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), unique.len(), "no row may appear on two pages");

    // The page boundary and the cursor round-trip: the cut page names an
    // `older` cursor, the next read passes it back verbatim, and the two pages
    // tile the Lane's transcript with the boundary row on exactly one of them.
    assert!(!lane_a.complete, "a cut page is not complete");
    let older = lane_a
        .older
        .clone()
        .expect("an incomplete page carries a cursor");
    let resumed = fixture
        .events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind:
                    RuntimeEventKind::CommandAccepted {
                        command_id,
                        command: RuntimeCommand::QueryTranscriptRows { query },
                    },
                ..
            }) if command_id == "rows_lane_a_older" => Some(query.before.clone()),
            _ => None,
        })
        .expect("the older read is accepted");
    assert_eq!(
        resumed.as_deref(),
        Some(older.as_str()),
        "the resumed read passes the cursor back verbatim"
    );
    assert!(lane_a_older.complete, "no older row matches the scope");
    assert!(
        lane_a_older.older.is_none(),
        "a complete page has no cursor"
    );
    let boundary = lane_a.rows.first().expect("a cut page has rows");
    assert!(
        lane_a_older
            .rows
            .iter()
            .all(|row| row.sequence < boundary.sequence),
        "the cursor is exclusive, so the boundary row is on exactly one page"
    );

    // Every content variant appears once, so a client cannot pass this fixture
    // while rendering only prose.
    let variants = lane_a
        .rows
        .iter()
        .chain(lane_a_older.rows.iter())
        .map(|row| match &row.content {
            TranscriptRowContent::User { .. } => "user",
            TranscriptRowContent::Assistant { .. } => "assistant",
            TranscriptRowContent::ToolCall { .. } => "tool_call",
            TranscriptRowContent::ToolResult { .. } => "tool_result",
            TranscriptRowContent::CheckRun { .. } => "check_run",
            TranscriptRowContent::Permission { .. } => "permission",
            _ => "unknown",
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        variants,
        BTreeSet::from([
            "user",
            "assistant",
            "tool_call",
            "tool_result",
            "check_run",
            "permission",
        ]),
        "the fixture must carry every row variant"
    );

    // The truncated assistant body: cut at the bound, flagged, and naming the
    // canonical row that holds it whole. A client that rendered it as the whole
    // reply would be showing a reviewer a partial answer as complete.
    let truncated = lane_a_older
        .rows
        .iter()
        .find_map(|row| match &row.content {
            TranscriptRowContent::Assistant {
                text,
                truncated,
                evidence_id,
            } => Some((text.clone(), *truncated, evidence_id.clone())),
            _ => None,
        })
        .expect("the fixture carries a cut assistant body");
    assert!(truncated.1, "the bound cut the body and says so");
    assert_eq!(
        truncated.0.len(),
        MAX_TRANSCRIPT_ROW_TEXT_BYTES as usize,
        "the published prefix is exactly the bound"
    );
    assert_eq!(
        truncated.2.as_deref(),
        Some("assistant-body-transcript-rows"),
        "a cut body names the canonical row that still holds it"
    );

    // The tool result names C7's archived patch row for its own tool call, so a
    // reviewer reaches the bytes from the transcript rather than from a guess.
    assert!(lane_a.rows.iter().any(|row| matches!(
        &row.content,
        TranscriptRowContent::ToolResult { tool_call_id, evidence_id, .. }
            if tool_call_id == "call_transcript_rows_edit"
                && evidence_id.as_deref() == Some("patch-call_transcript_rows_edit")
    )));

    // A scoped allow's payload is not in the durable audit row, so the
    // permission row admits it does not know the decision rather than
    // publishing a bare `allow_once` for a standing grant.
    let permission = lane_a
        .rows
        .iter()
        .find_map(|row| match &row.content {
            TranscriptRowContent::Permission {
                request_id,
                decision,
                audit_id,
            } => Some((request_id.clone(), decision.clone(), audit_id.clone())),
            _ => None,
        })
        .expect("the fixture carries a permission row");
    assert_eq!(permission.0, "approval_transcript_rows");
    assert_eq!(permission.2, "audit_transcript_rows");
    assert_eq!(
        permission.1,
        Some(ApprovalDecision::Allow {
            scope: ApprovalScope::Once
        })
    );

    // The refusal. A cursor this build did not issue is a `command_rejected`
    // naming the exact read, never an empty page.
    let refusal = fixture
        .events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::CommandRejected { command_id, reason },
                ..
            }) => Some((command_id.clone(), reason.clone())),
            _ => None,
        })
        .expect("the fixture carries the cursor refusal");
    assert_eq!(refusal.0, "rows_bad_cursor");
    assert!(
        refusal.1.contains("transcript row cursor `page-2`"),
        "the refusal names the cursor it refused"
    );
    assert!(
        refusal.1.contains("pass back the `older` cursor"),
        "the refusal keeps the actionable hint"
    );
    assert!(
        !pages
            .iter()
            .any(|(command_id, _)| command_id == "rows_bad_cursor"),
        "a refused read must not also publish a page"
    );

    // A transcript rows page is a query answer, never view state: applying
    // every page in this fixture to a fresh view leaves it byte-identical, so
    // publishing one moves no snapshot digest and no frozen base fixture.
    let untouched = RuntimeViewState::new(fixture.initial_snapshot.clone());
    let mut only_pages = RuntimeViewState::new(fixture.initial_snapshot.clone());
    for envelope in &fixture.events {
        if let RuntimeWireEvent::Known(event) = &envelope.event
            && matches!(event.kind, RuntimeEventKind::TranscriptRowsLoaded { .. })
        {
            only_pages.apply_event(event);
        }
    }
    assert_eq!(
        canonical_view_sha256(&only_pages),
        canonical_view_sha256(&untouched),
        "no transcript rows page may reduce into RuntimeViewState"
    );
}

#[test]
#[ignore = "manual transcript rows fixture refresh; normal tests validate committed JSON only"]
fn refresh_transcript_rows_extension_fixture() {
    let root = fixture_root();
    fs::create_dir_all(&root).unwrap();
    let fixture = transcript_rows_fixture();
    fs::write(
        root.join("transcript-rows.json"),
        serde_json::to_string_pretty(&fixture).unwrap() + "\n",
    )
    .unwrap();
}

/// `runtime.transcript_rows`, as bytes.
///
/// One durable transcript holding three owners' work — two Lanes and the
/// session composer, which is what the supervised input path really produces,
/// since a Lane's native turn runs through the same engine as the composer.
/// Three reads are outstanding at once and answered out of order; one is cut by
/// its limit and resumed through its own `older` cursor; one is refused for a
/// cursor this build did not issue.
///
/// The row sequences are Core's own: a transcript row at entry ordinal `o` gets
/// `2 * o + 1` and an audit-derived permission row that belongs after the first
/// `a` entries gets `2 * a`, which is what keeps positions stable under append.
/// Every string is fixed with no machine path in it, so the bytes are identical
/// on every machine that regenerates this fixture.
fn transcript_rows_fixture() -> FrontendContractFixtureOut {
    let fixture_id = "transcript-rows";
    let workspace_id = "workspace_contract_v1";
    let project_id = "project_viden";
    // The reading client. A transcript rows read is answered to whoever asked,
    // so the envelope owner is the reader's and the row owners are the work's.
    let reader = RuntimeOwner {
        workspace_id: workspace_id.to_string(),
        project_id: project_id.to_string(),
        lane_id: None,
        session_id: Some("session_transcript_rows".to_string()),
        task_id: None,
        turn_id: Some("turn_transcript_rows".to_string()),
    };
    let work_owner = |lane: Option<&str>, session: &str, turn: &str| RuntimeOwner {
        workspace_id: workspace_id.to_string(),
        project_id: project_id.to_string(),
        lane_id: lane.map(ToString::to_string),
        session_id: Some(session.to_string()),
        task_id: None,
        turn_id: Some(turn.to_string()),
    };
    let read_scope = |lane: Option<&str>, session: Option<&str>| RuntimeOwner {
        workspace_id: workspace_id.to_string(),
        project_id: project_id.to_string(),
        lane_id: lane.map(ToString::to_string),
        session_id: session.map(ToString::to_string),
        task_id: None,
        turn_id: None,
    };
    let lane_a_owner = work_owner(
        Some("lane_transcript_rows_a"),
        "session_lane_a",
        "turn_lane_a",
    );
    let lane_b_owner = work_owner(
        Some("lane_transcript_rows_b"),
        "session_lane_b",
        "turn_lane_b",
    );
    let session_owner = work_owner(None, "session_transcript_rows", "turn_session");

    let row = |id: &str, owner: &RuntimeOwner, sequence: u64, content: TranscriptRowContent| {
        OwnedTranscriptRow {
            id: id.to_string(),
            owner: owner.clone(),
            sequence,
            timestamp: Some(1_700_005_900 + sequence),
            content,
        }
    };

    // A real reply over the byte bound, cut exactly as Core cuts it. The bytes
    // are in the fixture rather than a short stand-in with `truncated = true`,
    // because a flag on a body the bound could not have cut would be a state
    // Core never produces.
    let whole_reply = {
        let sentence = "The archive keeps the whole reply; the row keeps its first 8 KiB. ";
        let mut body = String::new();
        while body.len() <= MAX_TRANSCRIPT_ROW_TEXT_BYTES as usize {
            body.push_str(sentence);
        }
        body
    };
    let cut_reply = whole_reply[..MAX_TRANSCRIPT_ROW_TEXT_BYTES as usize].to_string();

    // Lane A's transcript: six rows across one turn, with the approval that
    // allowed the edit sitting between the tool call and its result.
    let lane_a_oldest = vec![
        row(
            "session_lane_a:0",
            &lane_a_owner,
            1,
            TranscriptRowContent::User {
                text: "rename the parser entry point".to_string(),
                truncated: false,
            },
        ),
        row(
            "session_lane_a:1",
            &lane_a_owner,
            3,
            TranscriptRowContent::Assistant {
                text: cut_reply,
                truncated: true,
                evidence_id: Some("assistant-body-transcript-rows".to_string()),
            },
        ),
        row(
            "session_lane_a:2",
            &lane_a_owner,
            5,
            TranscriptRowContent::ToolCall {
                call: ToolCallView {
                    tool_call_id: "call_transcript_rows_edit".to_string(),
                    name: "edit_file".to_string(),
                    input_preview: "path=crates/tools/src/parser.rs".to_string(),
                    owner: Some(lane_a_owner.clone()),
                },
            },
        ),
    ];
    let lane_a_newest = vec![
        row(
            "approval:audit_transcript_rows",
            &lane_a_owner,
            6,
            TranscriptRowContent::Permission {
                request_id: "approval_transcript_rows".to_string(),
                decision: Some(ApprovalDecision::Allow {
                    scope: ApprovalScope::Once,
                }),
                audit_id: "audit_transcript_rows".to_string(),
            },
        ),
        row(
            "session_lane_a:3",
            &lane_a_owner,
            7,
            TranscriptRowContent::ToolResult {
                tool_call_id: "call_transcript_rows_edit".to_string(),
                success: true,
                summary: "1 file changed, 4 insertions(+), 2 deletions(-)".to_string(),
                evidence_id: Some("patch-call_transcript_rows_edit".to_string()),
            },
        ),
        row(
            "session_lane_a:4",
            &lane_a_owner,
            9,
            TranscriptRowContent::CheckRun {
                check: CheckRunView {
                    id: "call_transcript_rows_check".to_string(),
                    owner: lane_a_owner.clone(),
                    label: "cargo test -p viden-tools".to_string(),
                    command: "cargo test -p viden-tools".to_string(),
                    status: CheckRunStatus::Passed,
                    summary: "passed".to_string(),
                    failing_location: None,
                },
            },
        ),
    ];
    // Exclusive, so the boundary row lands on exactly one of the two pages.
    let older_cursor = "s:6:approval:audit_transcript_rows".to_string();

    let lane_b_rows = vec![
        row(
            "session_lane_b:0",
            &lane_b_owner,
            1,
            TranscriptRowContent::User {
                text: "write the migration note".to_string(),
                truncated: false,
            },
        ),
        row(
            "session_lane_b:1",
            &lane_b_owner,
            3,
            TranscriptRowContent::Assistant {
                text: "Drafted docs/migration.md.".to_string(),
                truncated: false,
                evidence_id: None,
            },
        ),
    ];
    let session_rows = vec![
        row(
            "session_transcript_rows:0",
            &session_owner,
            1,
            TranscriptRowContent::User {
                text: "what changed in the two Lanes?".to_string(),
                truncated: false,
            },
        ),
        row(
            "session_transcript_rows:1",
            &session_owner,
            3,
            TranscriptRowContent::Assistant {
                text: "One renamed the parser entry point; one drafted the migration note."
                    .to_string(),
                truncated: false,
                evidence_id: None,
            },
        ),
    ];

    let accepted =
        |command_id: &str, query: TranscriptRowsQuery| RuntimeEventKind::CommandAccepted {
            command_id: command_id.to_string(),
            command: RuntimeCommand::QueryTranscriptRows { query },
        };
    let loaded =
        |command_id: &str, page: TranscriptRowsPage| RuntimeEventKind::TranscriptRowsLoaded {
            command_id: command_id.to_string(),
            page,
        };

    let kinds = vec![
        // All three reads are sent before any is answered.
        accepted(
            "rows_lane_a",
            TranscriptRowsQuery {
                owner: read_scope(Some("lane_transcript_rows_a"), None),
                before: None,
                limit: Some(3),
            },
        ),
        accepted(
            "rows_lane_b",
            TranscriptRowsQuery {
                owner: read_scope(Some("lane_transcript_rows_b"), None),
                before: None,
                limit: None,
            },
        ),
        accepted(
            "rows_session",
            TranscriptRowsQuery {
                owner: read_scope(None, Some("session_transcript_rows")),
                before: None,
                limit: None,
            },
        ),
        // Answered out of request order, so a client correlating by arrival
        // renders Lane B's conversation under Lane A.
        loaded(
            "rows_lane_b",
            TranscriptRowsPage {
                rows: lane_b_rows,
                older: None,
                complete: true,
            },
        ),
        loaded(
            "rows_lane_a",
            TranscriptRowsPage {
                rows: lane_a_newest,
                older: Some(older_cursor.clone()),
                complete: false,
            },
        ),
        loaded(
            "rows_session",
            TranscriptRowsPage {
                rows: session_rows,
                older: None,
                complete: true,
            },
        ),
        // The cursor round-trip: passed back verbatim to reach the page above.
        accepted(
            "rows_lane_a_older",
            TranscriptRowsQuery {
                owner: read_scope(Some("lane_transcript_rows_a"), None),
                before: Some(older_cursor),
                limit: Some(3),
            },
        ),
        loaded(
            "rows_lane_a_older",
            TranscriptRowsPage {
                rows: lane_a_oldest,
                older: None,
                complete: true,
            },
        ),
        // A cursor this build did not issue. Refused, and deliberately not
        // answered with an empty page: "malformed" and "nothing was said" are
        // different facts and a client must never render the second for the
        // first.
        accepted(
            "rows_bad_cursor",
            TranscriptRowsQuery {
                owner: read_scope(Some("lane_transcript_rows_a"), None),
                before: Some("page-2".to_string()),
                limit: None,
            },
        ),
        RuntimeEventKind::CommandRejected {
            command_id: "rows_bad_cursor".to_string(),
            reason: "transcript row cursor `page-2` is not a cursor this build issued\n\
                     hint: pass back the `older` cursor from the previous page verbatim, or omit \
                     it to read the newest page"
                .to_string(),
        },
    ];

    fixture(
        fixture_id,
        &[
            "runtime.commands",
            "runtime.events",
            "runtime.snapshot",
            "runtime.transcript_rows",
        ],
        snapshot(WorkMode::Build),
        owned_envelopes(fixture_id, reader, kinds, 1_700_006_000),
    )
}
