use std::sync::{Arc, Mutex};

use viden_core::{
    ApprovalRequestView, ApprovalScope, FRONTEND_SCHEMA_V1, RuntimeCommand, RuntimeEventKind,
    RuntimeSnapshot, RuntimeViewState, WorkMode,
};
use viden_gui::{GuiCoreAdapter, PermissionChoice, PermissionIntent};

mod support;
use support::{TestCoreClient, TestOwner};

const APPROVAL_FIXTURE: &str = include_str!(
    "../../../crates/types/tests/fixtures/frontend-contract-v1/approval-allow-deny.json"
);

fn approval_view(work_mode: WorkMode) -> (RuntimeViewState, ApprovalRequestView) {
    let fixture: serde_json::Value = serde_json::from_str(APPROVAL_FIXTURE).unwrap();
    let mut snapshot: RuntimeSnapshot =
        serde_json::from_value(fixture["initial_snapshot"].clone()).unwrap();
    snapshot.work_mode = work_mode;
    let mut view = RuntimeViewState::new(snapshot);
    let mut approval: ApprovalRequestView = serde_json::from_value(
        fixture["events"][0]["event"]["kind"]["payload"]["approval"].clone(),
    )
    .unwrap();
    approval.allowed_scopes = vec![
        ApprovalScope::Once,
        ApprovalScope::Session {
            session_id: "session_contract".into(),
        },
        ApprovalScope::RepoAllowlist {
            paths: vec!["/workspace/viden/apps/gui".into()],
        },
    ];
    view.pending_approvals.push(approval.clone());
    (view, approval)
}

fn connected(
    view: RuntimeViewState,
    sent: Arc<Mutex<Vec<viden_core::RuntimeCommandEnvelope>>>,
) -> GuiCoreAdapter {
    let mut adapter = GuiCoreAdapter::new(Box::new(TestCoreClient::new(view, sent)));
    adapter.connect().expect("connect permission fixture");
    adapter
}

#[test]
fn permission_dock_projects_every_exact_core_fact_and_only_typed_scopes() {
    let (view, approval) = approval_view(WorkMode::Build);
    let adapter = connected(view, Arc::new(Mutex::new(Vec::new())));

    let dock = adapter.permission_dock().expect("permission dock");
    let request = dock.request.expect("pending approval");
    assert_eq!(request.id, approval.id);
    assert_eq!(request.tool_name, "shell");
    assert_eq!(request.risk, "high");
    assert_eq!(request.target.kind, "repo_path");
    assert_eq!(request.target.display, "crates/core");
    assert_eq!(
        request.target.canonical_ref.as_deref(),
        Some("repo://crates/core")
    );
    assert_eq!(request.reason.as_deref(), Some("contract fixture coverage"));
    assert_eq!(request.input_preview, "cargo test");
    assert_eq!(request.expires_at, 1_700_003_600);
    assert_eq!(request.default_action, "deny");
    assert_eq!(request.audit_id, "audit_approval_allow");
    assert_eq!(
        request
            .actions
            .iter()
            .map(|action| (action.kind.as_str(), action.available))
            .collect::<Vec<_>>(),
        vec![
            ("once", true),
            ("session", true),
            ("repo_allowlist", true),
            ("always", false),
            ("edit", false),
            ("deny", true),
        ]
    );
    // This fixture's `allowed_scopes` carries no `Always` — it is a high-risk
    // request, which Core never offers a standing rule on — so the tier is
    // drawn with Core's own refusal name (`runtime.approval_scopes`, C15).
    assert_eq!(request.actions[3].code, Some("scope_not_offered"));
    // `shell` *is* on Core's editable list, but this fixture's approval does
    // not set `editable`, so Edit is withheld as Core's own decision rather
    // than as a client gap.
    assert!(!request.editable);
    assert_eq!(request.actions[4].code, Some("not_editable"));
    assert!(request.revised_from.is_none());
}

#[test]
fn exact_once_session_repo_and_deny_responses_send_only_respond_to_approval() {
    for (choice, expected_scope) in [
        (PermissionChoice::Once, Some("once")),
        (PermissionChoice::Session, Some("session")),
        (PermissionChoice::RepoAllowlist, Some("repo_allowlist")),
        (PermissionChoice::Deny, None),
    ] {
        let (view, approval) = approval_view(WorkMode::Build);
        let sent = Arc::new(Mutex::new(Vec::new()));
        let client = TestCoreClient::new(view, Arc::clone(&sent)).with_owned_event(
            TestOwner {
                workspace_id: approval.owner.workspace_id.clone(),
                project_id: approval.owner.project_id.clone(),
                lane_id: approval.owner.lane_id.clone(),
                session_id: approval.owner.session_id.clone(),
                task_id: approval.owner.task_id.clone(),
                turn_id: approval.owner.turn_id.clone(),
            },
            RuntimeEventKind::ApprovalResolved {
                request_id: approval.id.clone(),
                decision: match expected_scope {
                    Some("once") => viden_core::ApprovalDecision::Allow {
                        scope: ApprovalScope::Once,
                    },
                    Some("session") => viden_core::ApprovalDecision::Allow {
                        scope: ApprovalScope::Session {
                            session_id: "session_contract".into(),
                        },
                    },
                    Some("repo_allowlist") => viden_core::ApprovalDecision::Allow {
                        scope: ApprovalScope::RepoAllowlist {
                            paths: vec!["/workspace/viden/apps/gui".into()],
                        },
                    },
                    _ => viden_core::ApprovalDecision::Deny,
                },
                owner: approval.owner.clone(),
                audit_id: approval.audit_id.clone(),
            },
        );
        let mut adapter = GuiCoreAdapter::new(Box::new(client));
        adapter.connect().unwrap();
        let result = adapter
            .send_permission_intent_and_wait(
                "permission-command",
                PermissionIntent::Respond {
                    request_id: approval.id.clone(),
                    choice,
                    feedback: None,
                    revised_input: None,
                },
                std::time::Duration::ZERO,
            )
            .expect("send exact approval response");
        assert_eq!(result.outcome.state, "confirmed");
        let sent = sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].schema_version, FRONTEND_SCHEMA_V1);
        assert_eq!(sent[0].owner, approval.owner);
        assert!(matches!(
            sent[0].command,
            RuntimeCommand::RespondToApproval { .. }
        ));
    }
}

#[test]
fn plan_mutation_and_untyped_design_actions_send_nothing() {
    let (view, approval) = approval_view(WorkMode::Plan);
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = connected(view, Arc::clone(&sent));
    let request = adapter
        .permission_dock()
        .unwrap()
        .request
        .expect("Plan approval request remains visible");
    assert!(request.actions.iter().all(|action| !action.available));

    for choice in [
        PermissionChoice::Once,
        PermissionChoice::Session,
        PermissionChoice::RepoAllowlist,
        PermissionChoice::Always,
        PermissionChoice::Edit,
        PermissionChoice::Deny,
    ] {
        let error = adapter
            .send_permission_intent(
                "blocked",
                PermissionIntent::Respond {
                    request_id: approval.id.clone(),
                    choice,
                    feedback: None,
                    revised_input: None,
                },
            )
            .expect_err("Plan mutation or untyped design action must fail closed");
        assert!(error.contains("Plan") || error.contains("GUI-CORE-003"));
    }
    assert!(sent.lock().unwrap().is_empty());
}

#[test]
fn d1_poll_observes_the_exact_permission_resolution_it_consumes() {
    let (view, approval) = approval_view(WorkMode::Build);
    let sent = Arc::new(Mutex::new(Vec::new()));
    let owner = TestOwner {
        workspace_id: approval.owner.workspace_id.clone(),
        project_id: approval.owner.project_id.clone(),
        lane_id: approval.owner.lane_id.clone(),
        session_id: approval.owner.session_id.clone(),
        task_id: approval.owner.task_id.clone(),
        turn_id: approval.owner.turn_id.clone(),
    };
    let client = TestCoreClient::new(view, Arc::clone(&sent))
        .with_gap()
        .with_owned_event(
            owner,
            RuntimeEventKind::ApprovalResolved {
                request_id: approval.id.clone(),
                decision: viden_core::ApprovalDecision::Deny,
                owner: approval.owner.clone(),
                audit_id: approval.audit_id.clone(),
            },
        );
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().unwrap();
    let pending = adapter
        .send_permission_intent_and_wait(
            "permission-command",
            PermissionIntent::Respond {
                request_id: approval.id.clone(),
                choice: PermissionChoice::Deny,
                feedback: None,
                revised_input: None,
            },
            std::time::Duration::ZERO,
        )
        .unwrap();
    assert_eq!(pending.outcome.state, "pending");

    adapter.poll_d1(None, std::time::Duration::ZERO).unwrap();
    let observed = adapter.poll_permission(std::time::Duration::ZERO).unwrap();
    assert_eq!(observed.outcome.state, "confirmed");
    assert!(observed.pending_command_id.is_none());
}
