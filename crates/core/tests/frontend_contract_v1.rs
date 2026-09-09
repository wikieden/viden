use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use viden_core::{
    CORE_CLIENT_CAPABILITIES, CORE_CLIENT_VERSION, CORE_EXTENSION_CAPABILITIES, CheckRunView,
    RuntimeServiceHealthView, WorkspaceChangeView, WorkspaceSourceView, frontend_capabilities,
    local_core_handshake,
};
use viden_types::{
    AgentAdapterSource, AgentAdapterView, AgentAuthState, AgentAvailability, AgentContentPart,
    AgentConversationRole, AgentDagRecord, AgentDagStatus, AgentDagTaskSpec, AgentLaneRecord,
    AgentNextAction, AgentRole, AgentRoute, AgentSessionStatus, AgentSessionView,
    AgentStartability, AgentTaskKind, AgentTaskRecord, AgentTaskStatus, ApprovalDecision,
    ApprovalDefaultAction, ApprovalRequestView, ApprovalResponse, ApprovalRisk, ApprovalScope,
    ApprovalTarget, AuditActor, AuditActorFilter, AuditObjectRef, AuditOutcome, AuditPage,
    AuditQuery, AuditRecord, CapabilityId, ContextBudgetRecord, ContextBundleRecord,
    ContextOmittedSourceRecord, ContextScope, ContextSourceRecord, CostScope, CostUsageOutcome,
    CostUsageRecord, EventCursor, EvidenceView, ExecutionTarget, FRONTEND_SCHEMA_V1, GateStrength,
    LaneBudget, LaneRuntimeOwnerBinding, LaneStatus, MergeGateDecision, MergeGateDecisionOutcome,
    MergeGatePolicySnapshot, MergeGateRecord, MergeGateStatus, MergeGateType, MergeGateValidator,
    MutationPolicy, PermissionLevel, PermissionMode, ProjectConfigState, ProjectProbe,
    QueuedInputView, RecentProjectSummary, RecentSessionSummary, ResolvedUiPreferences,
    ReviewRequestStatus, RuntimeCommand, RuntimeErrorView, RuntimeEvent, RuntimeEventEnvelope,
    RuntimeEventKind, RuntimeOwner, RuntimeSnapshot, RuntimeViewState, RuntimeWireEvent,
    SchemaVersion, StarterLanePreview, StarterLanePreviewInvalidationReason, StarterLaneReceipt,
    TokenCostView, TokenUsage, UiColorMode, UiDensity, UiMotion, UiPreferences, UiSkin, WorkMode,
    WorkspaceEligibility, WorkspaceFileEntry, WorkspaceFileKind, WorkspaceFilePage,
    WorkspaceFilesQuery,
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
    assert!(extension_manifest.contains("candidate_component_version = \"0.3.5\""));
    assert!(extension_manifest.contains("compatibility = \"additive_capability_gated\""));
    assert!(extension_manifest.contains("[runtime_trust_loop]\ncommand_count = 8"));
    assert_eq!(CORE_CLIENT_VERSION, "0.3.5");
    assert_eq!(local_core_handshake().core_version, "0.3.5");
}

#[test]
fn frontend_host_capabilities_are_schema_one_core_0_3_5_and_additive() {
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
        "runtime.credential_handles",
        "runtime.credential_staging",
        "runtime.lane_lifecycle",
        "runtime.lane_owner_projection",
        "runtime.project_onboarding",
        "runtime.recent_work",
        "runtime.starter_lane_preview",
        "runtime.trust_loop",
        "runtime.workspace_eligibility",
        // GUI-CORE-022. The frozen base list above is unchanged, which is what
        // keeps the nine base fixtures byte-identical.
        "runtime.workspace_files",
        "ui.preference_persistence",
    ];

    assert_eq!(FRONTEND_SCHEMA_V1, SchemaVersion(1));
    assert_eq!(CORE_CLIENT_VERSION, "0.3.5");
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
    assert!(extension_manifest.contains("candidate_component_version = \"0.3.5\""));
    assert!(extension_manifest.contains("schema_version = 1"));
    assert!(extension_manifest.contains("runtime.cockpit_context_v1"));
    assert!(!extension_manifest.contains("runtime.workspace_facts"));
    assert!(extension_manifest.contains("runtime.lane_owner_projection"));
    assert!(extension_manifest.contains(
        "extension_fixture_sha256 = \"96dd5fde9f1241eb50f9d8978cf478d0ac5d3327448dc6ccde9d0e5018ce1580\""
    ));
    assert!(extension_manifest.contains(
        "interaction_fixture_sha256 = \"78e8993fa455149d05744d15e70bc4c2072f3d4726bf76026203826f500204a5\""
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
    assert!(release_manifest.contains("component_version = \"0.3.5\""));
    assert!(release_manifest.contains("runtime.cockpit_context_v1"));
    assert!(!release_manifest.contains("runtime.workspace_facts"));
    assert!(release_manifest.contains(
        "contract_implementation_checkpoint = \"17fa2071398d5eaf30045257163d57d22d99177b\""
    ));
    assert!(release_manifest.contains(
        "payload_sha256 = \"78e8993fa455149d05744d15e70bc4c2072f3d4726bf76026203826f500204a5\""
    ));
    assert!(release_manifest.contains(
        "view_sha256 = \"46db05abaaae36cf37cb7ffa0493a4ef8c158a2d5b4ffeef08d01dbf8e284ed0\""
    ));
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
