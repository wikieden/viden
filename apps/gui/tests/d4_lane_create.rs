use std::sync::{Arc, Mutex};
use std::time::Duration;

use viden_core::{
    AgentLaneRecord, AgentRole, AgentRoute, ApprovalDecision, ApprovalDefaultAction,
    ApprovalRequestView, ApprovalRisk, ApprovalScope, ApprovalTarget, DataEgressPolicy,
    ExecutionTarget, GateStrength, LaneBudget, LaneStatus, MutationPolicy, RuntimeCommand,
    RuntimeEventKind, RuntimeOwner, RuntimeSnapshot, RuntimeViewState, StarterLanePreset,
    StarterLanePreview, StarterLanePreviewInvalidationReason, StarterLaneReceipt,
    StarterLaneRequest, WorkMode, local_core_handshake,
};
use viden_gui::{
    D4_STARTER_LANE_CAPABILITY, D4ApprovalIntent, D4Intent, D4LaneRequest, D4Preset, GuiCoreAdapter,
};

mod support;
use support::{TestCoreClient, TestOwner};

const D1_FIXTURE: &str = include_str!(
    "../../../crates/types/tests/fixtures/frontend-contract-v1/d1-vertical-slice.json"
);

#[derive(serde::Deserialize)]
struct SnapshotFixture {
    initial_snapshot: RuntimeSnapshot,
}

fn d4_view(mode: WorkMode) -> RuntimeViewState {
    let mut snapshot = serde_json::from_str::<SnapshotFixture>(D1_FIXTURE)
        .expect("runtime snapshot fixture")
        .initial_snapshot;
    snapshot.work_mode = mode;
    RuntimeViewState::new(snapshot)
}

fn owner(lane_id: &str) -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace-gui".into(),
        project_id: "project-gui".into(),
        lane_id: Some(lane_id.into()),
        session_id: Some("session-gui".into()),
        task_id: None,
        turn_id: Some("turn-gui".into()),
    }
}

fn test_owner(owner: &RuntimeOwner) -> TestOwner {
    TestOwner {
        workspace_id: owner.workspace_id.clone(),
        project_id: owner.project_id.clone(),
        lane_id: owner.lane_id.clone(),
        session_id: owner.session_id.clone(),
        task_id: owner.task_id.clone(),
        turn_id: owner.turn_id.clone(),
    }
}

fn request(lane_id: &str, preset: D4Preset) -> D4LaneRequest {
    D4LaneRequest {
        lane_id: lane_id.into(),
        preset,
        branch: None,
        worktree_path: None,
    }
}

fn core_request(request: &D4LaneRequest) -> StarterLaneRequest {
    StarterLaneRequest {
        lane_id: request.lane_id.clone(),
        preset: match request.preset {
            D4Preset::Coder => StarterLanePreset::Coder,
            D4Preset::Reviewer => StarterLanePreset::Reviewer,
            D4Preset::Tester => StarterLanePreset::Tester,
        },
        branch: request.branch.clone(),
        worktree_path: request.worktree_path.clone(),
    }
}

fn lane_for(request: &D4LaneRequest) -> AgentLaneRecord {
    let (role, gate_strength) = match request.preset {
        D4Preset::Coder => (AgentRole::Coder, GateStrength::Full),
        D4Preset::Reviewer => (AgentRole::Reviewer, GateStrength::Full),
        D4Preset::Tester => (AgentRole::Tester, GateStrength::Cooperative),
    };
    AgentLaneRecord {
        id: request.lane_id.clone(),
        task_id: None,
        role,
        route: AgentRoute::BuiltIn,
        gate_strength,
        mutation_policy: MutationPolicy::ProposeOnly,
        worktree: Some(format!("/workspace/.worktrees/{}", request.lane_id)),
        branch: Some(format!("codex/{}", request.lane_id)),
        target: ExecutionTarget::Local,
        data_egress: DataEgressPolicy::Deny,
        status: LaneStatus::Draft,
        budget: LaneBudget::default(),
        active_session_ids: Vec::new(),
        summary: format!("{} starter lane", role.as_str()),
        evidence: Vec::new(),
        run_stats: None,
    }
}

fn preview_for(request: &D4LaneRequest, owner: RuntimeOwner) -> StarterLanePreview {
    let lane = lane_for(request);
    StarterLanePreview {
        preview_id: format!("preview-{}", request.lane_id),
        content_sha256: "a".repeat(64),
        owner,
        branch: lane.branch.clone().expect("branch"),
        worktree_path: lane.worktree.clone().expect("worktree"),
        base_revision: "b".repeat(40),
        diagnostics: Vec::new(),
        lane,
    }
}

fn receipt_for(preview: &StarterLanePreview) -> StarterLaneReceipt {
    let mut lane = preview.lane.clone();
    lane.status = LaneStatus::Running;
    StarterLaneReceipt {
        preview_id: preview.preview_id.clone(),
        content_sha256: preview.content_sha256.clone(),
        lane,
        branch: preview.branch.clone(),
        worktree_path: preview.worktree_path.clone(),
        base_revision: preview.base_revision.clone(),
        owner: preview.owner.clone(),
    }
}

fn accepted(command_id: &str, command: RuntimeCommand) -> RuntimeEventKind {
    RuntimeEventKind::CommandAccepted {
        command_id: command_id.into(),
        command,
    }
}

fn lane_accepted(command_id: &str, command: RuntimeCommand) -> RuntimeEventKind {
    RuntimeEventKind::LaneCommandAccepted {
        command_id: command_id.into(),
        command,
    }
}

fn approval(owner: RuntimeOwner, lane_id: &str) -> ApprovalRequestView {
    ApprovalRequestView {
        id: format!("approval-{lane_id}"),
        tool_name: "lane_create".into(),
        title: format!("Create {lane_id}"),
        message: "Review exact starter Lane".into(),
        input_preview: lane_id.into(),
        is_mutating: true,
        reason: None,
        owner,
        risk: ApprovalRisk::Medium,
        target: ApprovalTarget {
            kind: "lane".into(),
            display: lane_id.into(),
            canonical_ref: None,
        },
        allowed_scopes: vec![ApprovalScope::Once],
        policy_reason_key: "lane_create".into(),
        policy_reason_args: Default::default(),
        expires_at: 0,
        default_action: ApprovalDefaultAction::Deny,
        audit_id: format!("audit-{lane_id}"),
        decision_context: None,
    }
}

fn enabled_client(
    view: RuntimeViewState,
    sent: Arc<Mutex<Vec<viden_core::RuntimeCommandEnvelope>>>,
) -> TestCoreClient {
    let mut client = TestCoreClient::new(view, sent).with_stream_id("d4");
    client
        .capabilities
        .insert(D4_STARTER_LANE_CAPABILITY.to_string());
    client
}

#[test]
fn missing_capability_is_visible_and_blocks_transport() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut client = TestCoreClient::new(d4_view(WorkMode::Build), Arc::clone(&sent));
    client.capabilities.remove(D4_STARTER_LANE_CAPABILITY);
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect GUI fake");

    let projection = adapter.d4_lane_create().expect("D4 projection");
    assert!(!projection.availability.available);
    assert_eq!(
        projection.availability.capability,
        D4_STARTER_LANE_CAPABILITY
    );

    let error = adapter
        .send_d4_intent_and_wait(
            "preview-disabled",
            D4Intent::Preview {
                request: request("starter-coder", D4Preset::Coder),
            },
            Duration::ZERO,
        )
        .expect_err("missing capability must fail closed");
    assert!(error.contains(D4_STARTER_LANE_CAPABILITY));
    assert!(sent.lock().expect("sent lock").is_empty());
}

#[test]
fn core_0_3_2_handshake_advertises_typed_d4_lane_creation() {
    assert!(
        local_core_handshake()
            .capabilities
            .iter()
            .any(|capability| capability.0 == D4_STARTER_LANE_CAPABILITY)
    );
}

#[test]
fn d4_tauri_wire_uses_camel_case_fields_and_closed_intents() {
    let intent = serde_json::from_value::<D4Intent>(serde_json::json!({
        "type": "respond_to_approval",
        "requestId": "approval-wire",
        "decision": "deny"
    }))
    .expect("deserialize D4 webview intent");
    assert!(matches!(
        intent,
        D4Intent::RespondToApproval {
            request_id,
            decision: D4ApprovalIntent::Deny
        } if request_id == "approval-wire"
    ));

    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = GuiCoreAdapter::new(Box::new(TestCoreClient::new(
        d4_view(WorkMode::Build),
        sent,
    )));
    adapter.connect().expect("connect GUI fake");
    let wire = serde_json::to_value(adapter.d4_lane_create().expect("D4 projection"))
        .expect("serialize D4 projection");
    assert!(wire.get("workMode").is_some());
    assert!(wire["outcome"].get("requiresRepreview").is_some());
    assert!(wire.get("navigationLaneId").is_some());
}

#[test]
fn plan_mode_allows_preview_but_disables_create() {
    let request = request("starter-plan", D4Preset::Coder);
    let owner = owner(&request.lane_id);
    let preview = preview_for(&request, owner.clone());
    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = enabled_client(d4_view(WorkMode::Plan), Arc::clone(&sent))
        .with_owned_event(
            test_owner(&owner),
            accepted(
                "preview-plan",
                RuntimeCommand::PreviewStarterLane {
                    request: core_request(&request),
                },
            ),
        )
        .with_owned_event(
            test_owner(&owner),
            RuntimeEventKind::StarterLanePreviewed {
                preview: preview.clone(),
            },
        );
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D4 fake");

    let result = adapter
        .send_d4_intent_and_wait(
            "preview-plan",
            D4Intent::Preview {
                request: request.clone(),
            },
            Duration::ZERO,
        )
        .expect("Plan preview is read-only");
    assert_eq!(
        result.projection.preview.as_ref().unwrap().preview_id,
        preview.preview_id
    );
    assert!(!result.projection.can_create);
    assert_eq!(result.projection.work_mode, "plan");

    let error = adapter
        .send_d4_intent_and_wait("create-plan", D4Intent::Create { request }, Duration::ZERO)
        .expect_err("Plan create must be disabled before transport");
    assert!(error.contains("Plan"));
    assert_eq!(sent.lock().expect("sent lock").len(), 1);
}

#[test]
fn reviewed_create_waits_through_approval_and_lane_fact_for_exact_receipt() {
    let request = request("starter-review", D4Preset::Reviewer);
    let owner = owner(&request.lane_id);
    let preview = preview_for(&request, owner.clone());
    let receipt = receipt_for(&preview);
    let approval = approval(owner.clone(), &request.lane_id);
    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = enabled_client(d4_view(WorkMode::Build), Arc::clone(&sent))
        .with_owned_event(
            test_owner(&owner),
            accepted(
                "preview-reviewer",
                RuntimeCommand::PreviewStarterLane {
                    request: core_request(&request),
                },
            ),
        )
        .with_owned_event(
            test_owner(&owner),
            RuntimeEventKind::StarterLanePreviewed {
                preview: preview.clone(),
            },
        )
        .with_owned_event(
            test_owner(&owner),
            lane_accepted(
                "create-reviewer",
                RuntimeCommand::CreateStarterLane {
                    request: core_request(&request),
                    preview_id: preview.preview_id.clone(),
                    content_sha256: preview.content_sha256.clone(),
                },
            ),
        )
        .with_owned_event(
            test_owner(&owner),
            RuntimeEventKind::ApprovalRequested {
                approval: approval.clone(),
            },
        )
        .with_gap()
        .with_owned_event(
            test_owner(&owner),
            accepted(
                "approve-reviewer",
                RuntimeCommand::RespondToApproval {
                    request_id: approval.id.clone(),
                    response: viden_core::ApprovalResponse::allow_once(None),
                },
            ),
        )
        .with_owned_event(
            test_owner(&owner),
            RuntimeEventKind::ApprovalResolved {
                request_id: approval.id.clone(),
                decision: ApprovalDecision::Allow {
                    scope: ApprovalScope::Once,
                },
                owner: owner.clone(),
                audit_id: approval.audit_id.clone(),
            },
        )
        .with_owned_event(
            test_owner(&owner),
            RuntimeEventKind::LaneUpdated {
                lane: receipt.lane.clone(),
            },
        )
        .with_gap()
        .with_owned_event(
            test_owner(&owner),
            RuntimeEventKind::StarterLaneCreated {
                receipt: receipt.clone(),
            },
        );
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D4 fake");

    let preview_result = adapter
        .send_d4_intent_and_wait(
            "preview-reviewer",
            D4Intent::Preview {
                request: request.clone(),
            },
            Duration::ZERO,
        )
        .expect("review preview");
    let resolved = preview_result
        .projection
        .preview
        .expect("resolved Core preview");
    assert_eq!(resolved.lane.role, "reviewer");
    assert_eq!(resolved.lane.route, "built_in");
    assert_eq!(resolved.lane.gate_strength, "full");
    assert_eq!(resolved.lane.mutation_policy, "propose_only");
    assert_eq!(resolved.lane.target, "local");
    assert_eq!(resolved.lane.budget.token_limit, None);

    let create_result = adapter
        .send_d4_intent_and_wait(
            "create-reviewer",
            D4Intent::Create {
                request: request.clone(),
            },
            Duration::ZERO,
        )
        .expect("send exact reviewed create");
    assert_eq!(
        create_result.pending_intent.as_deref(),
        Some("create_starter_lane")
    );
    assert_eq!(
        create_result
            .projection
            .pending_approval
            .as_ref()
            .unwrap()
            .id,
        approval.id
    );
    assert_eq!(create_result.projection.navigation_lane_id, None);

    let allow_result = adapter
        .send_d4_intent_and_wait(
            "approve-reviewer",
            D4Intent::RespondToApproval {
                request_id: approval.id.clone(),
                decision: D4ApprovalIntent::AllowOnce,
            },
            Duration::ZERO,
        )
        .expect("allow reviewed create");
    assert_eq!(
        allow_result.pending_intent.as_deref(),
        Some("create_starter_lane")
    );
    assert_eq!(allow_result.projection.navigation_lane_id, None);
    assert_eq!(allow_result.projection.outcome.state, "waiting_for_receipt");

    let completed = adapter
        .poll_d4(Duration::ZERO)
        .expect("receive exact receipt");
    assert_eq!(completed.pending_command_id, None);
    assert_eq!(
        completed.projection.navigation_lane_id.as_deref(),
        Some("starter-review")
    );
    assert_eq!(
        completed.projection.receipt.as_ref().unwrap().preview_id,
        preview.preview_id
    );

    let commands = sent.lock().expect("sent lock");
    assert!(matches!(
        commands[0].command,
        RuntimeCommand::PreviewStarterLane { .. }
    ));
    assert!(matches!(
        commands[1].command,
        RuntimeCommand::CreateStarterLane { .. }
    ));
    assert!(matches!(
        commands[2].command,
        RuntimeCommand::RespondToApproval { .. }
    ));
    assert_eq!(commands[0].owner.lane_id, Some("starter-review".into()));
    assert!(commands[0].owner.workspace_id.is_empty());
    assert!(commands[1..].iter().all(|envelope| envelope.owner == owner));
}

#[test]
fn changed_request_is_rejected_locally_and_requires_a_new_preview() {
    let original = request("starter-changed", D4Preset::Coder);
    let owner = owner(&original.lane_id);
    let preview = preview_for(&original, owner.clone());
    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = enabled_client(d4_view(WorkMode::Build), Arc::clone(&sent))
        .with_owned_event(
            test_owner(&owner),
            accepted(
                "preview-changed",
                RuntimeCommand::PreviewStarterLane {
                    request: core_request(&original),
                },
            ),
        )
        .with_owned_event(
            test_owner(&owner),
            RuntimeEventKind::StarterLanePreviewed { preview },
        );
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D4 fake");
    adapter
        .send_d4_intent_and_wait(
            "preview-changed",
            D4Intent::Preview {
                request: original.clone(),
            },
            Duration::ZERO,
        )
        .expect("preview exact request");

    let mut changed = original;
    changed.branch = Some("codex/different".into());
    let error = adapter
        .send_d4_intent_and_wait(
            "create-changed",
            D4Intent::Create { request: changed },
            Duration::ZERO,
        )
        .expect_err("changed request must not consume reviewed preview");
    assert!(error.contains("re-preview"));
    assert_eq!(sent.lock().expect("sent lock").len(), 1);
}

#[test]
fn stale_or_denied_preview_preserves_no_navigation_and_requires_repreview() {
    for reason in [
        StarterLanePreviewInvalidationReason::BaseRevisionChanged,
        StarterLanePreviewInvalidationReason::PermissionDenied,
    ] {
        let request = request("starter-invalid", D4Preset::Tester);
        let owner = owner(&request.lane_id);
        let preview = preview_for(&request, owner.clone());
        let sent = Arc::new(Mutex::new(Vec::new()));
        let client = enabled_client(d4_view(WorkMode::Build), sent)
            .with_owned_event(
                test_owner(&owner),
                accepted(
                    "preview-invalid",
                    RuntimeCommand::PreviewStarterLane {
                        request: core_request(&request),
                    },
                ),
            )
            .with_owned_event(
                test_owner(&owner),
                RuntimeEventKind::StarterLanePreviewed {
                    preview: preview.clone(),
                },
            )
            .with_owned_event(
                test_owner(&owner),
                lane_accepted(
                    "create-invalid",
                    RuntimeCommand::CreateStarterLane {
                        request: core_request(&request),
                        preview_id: preview.preview_id.clone(),
                        content_sha256: preview.content_sha256.clone(),
                    },
                ),
            )
            .with_owned_event(
                test_owner(&owner),
                RuntimeEventKind::StarterLanePreviewInvalidated {
                    owner: owner.clone(),
                    preview_id: preview.preview_id.clone(),
                    reason,
                },
            );
        let mut adapter = GuiCoreAdapter::new(Box::new(client));
        adapter.connect().expect("connect D4 fake");
        adapter
            .send_d4_intent_and_wait(
                "preview-invalid",
                D4Intent::Preview {
                    request: request.clone(),
                },
                Duration::ZERO,
            )
            .expect("preview request");
        let result = adapter
            .send_d4_intent_and_wait(
                "create-invalid",
                D4Intent::Create { request },
                Duration::ZERO,
            )
            .expect("Core invalidation is a typed terminal result");

        assert!(result.projection.preview.is_none());
        assert!(result.projection.outcome.requires_repreview);
        assert_eq!(result.projection.navigation_lane_id, None);
    }
}

#[test]
fn mismatched_preview_or_receipt_owner_never_completes_pending_command() {
    let request = request("starter-owner", D4Preset::Coder);
    let expected_owner = owner(&request.lane_id);
    let mut wrong_owner = expected_owner.clone();
    wrong_owner.project_id = "project-other".into();
    let wrong_preview = preview_for(&request, wrong_owner.clone());
    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = enabled_client(d4_view(WorkMode::Build), sent)
        .with_owned_event(
            test_owner(&expected_owner),
            accepted(
                "preview-owner",
                RuntimeCommand::PreviewStarterLane {
                    request: core_request(&request),
                },
            ),
        )
        .with_owned_event(
            test_owner(&expected_owner),
            RuntimeEventKind::StarterLanePreviewed {
                preview: wrong_preview,
            },
        )
        .with_gap();
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D4 fake");

    let result = adapter
        .send_d4_intent_and_wait(
            "preview-owner",
            D4Intent::Preview { request },
            Duration::ZERO,
        )
        .expect("mismatched fact remains pending rather than trusted");
    assert_eq!(result.pending_command_id.as_deref(), Some("preview-owner"));
    assert!(result.projection.preview.is_none());
    assert_eq!(result.projection.navigation_lane_id, None);
}
