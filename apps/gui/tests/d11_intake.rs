use std::sync::{Arc, Mutex};
use std::time::Duration;

use sha2::{Digest, Sha256};
use viden_core::{
    ApprovalDecision, ApprovalDefaultAction, ApprovalRequestView, ApprovalRisk, ApprovalScope,
    ApprovalTarget, CoreClientError, CredentialHandle, CredentialStatus, ProjectConfigPreview,
    ProjectConfigState, ProjectProbe, ProviderHealthView, RuntimeCommand, RuntimeErrorView,
    RuntimeEventKind, RuntimeSnapshot, RuntimeViewState,
};
use viden_gui::{D11Intent, GuiCoreAdapter};

mod support;
use support::TestCoreClient as D11Client;

const D1_FIXTURE: &str = include_str!(
    "../../../crates/types/tests/fixtures/frontend-contract-v1/d1-vertical-slice.json"
);

#[derive(serde::Deserialize)]
struct SnapshotFixture {
    initial_snapshot: RuntimeSnapshot,
}

fn accepted(command_id: &str, command: RuntimeCommand) -> RuntimeEventKind {
    RuntimeEventKind::CommandAccepted {
        command_id: command_id.into(),
        command,
    }
}

fn approval_request(
    id: &str,
    tool_name: &str,
    target_kind: &str,
    target_display: &str,
) -> ApprovalRequestView {
    approval_request_with_preview(id, tool_name, target_kind, target_display, target_display)
}

fn approval_request_with_preview(
    id: &str,
    tool_name: &str,
    target_kind: &str,
    target_display: &str,
    input_preview: &str,
) -> ApprovalRequestView {
    ApprovalRequestView {
        id: id.into(),
        tool_name: tool_name.into(),
        title: format!("Approve {target_display}"),
        message: "Review D11 action".into(),
        input_preview: input_preview.into(),
        is_mutating: true,
        reason: None,
        owner: Default::default(),
        risk: ApprovalRisk::Medium,
        target: ApprovalTarget {
            kind: target_kind.into(),
            display: target_display.into(),
            canonical_ref: None,
        },
        allowed_scopes: vec![ApprovalScope::Once],
        policy_reason_key: tool_name.into(),
        policy_reason_args: Default::default(),
        expires_at: 0,
        default_action: ApprovalDefaultAction::Deny,
        audit_id: format!("audit-{id}"),
        decision_context: None,
    }
}

fn preview(contents: &str, preview_id: &str) -> ProjectConfigPreview {
    ProjectConfigPreview {
        preview_id: preview_id.into(),
        relative_path: "viden.toml".into(),
        content_sha256: format!("{:x}", Sha256::digest(contents.as_bytes())),
        byte_len: contents.len() as u64,
        exact_contents: Some(contents.into()),
        base_content_sha256: None,
        project_name: Some(preview_id.into()),
        pack: Some("rust".into()),
        diagnostics: Vec::new(),
    }
}

fn d11_view() -> RuntimeViewState {
    let mut snapshot: RuntimeSnapshot = serde_json::from_str::<SnapshotFixture>(D1_FIXTURE)
        .expect("runtime snapshot fixture")
        .initial_snapshot;
    snapshot.cwd = "/workspace/demo".into();
    let mut view = RuntimeViewState::new(snapshot);
    view.project_probe = Some(ProjectProbe {
        root: "/workspace/demo".into(),
        is_git_repository: true,
        git_root: Some("/workspace/demo".into()),
        config_path: "/workspace/demo/viden.toml".into(),
        config_state: ProjectConfigState::Missing,
        project_name: Some("demo".into()),
        pack: Some("rust".into()),
        diagnostics: Vec::new(),
    });
    view.project_config_preview = Some(ProjectConfigPreview {
        preview_id: "preview-safe".into(),
        relative_path: "viden.toml".into(),
        content_sha256: "a".repeat(64),
        byte_len: 38,
        exact_contents: Some("[project]\nname = \"demo\"\npack = \"rust\"\n".into()),
        base_content_sha256: None,
        project_name: Some("demo".into()),
        pack: Some("rust".into()),
        diagnostics: Vec::new(),
    });
    view.credential_handles.push(CredentialHandle {
        provider_id: "deepseek".into(),
        backend_id: "keychain:deepseek-primary".into(),
        status: CredentialStatus::Locked,
    });
    view.provider = Some(ProviderHealthView {
        provider_id: "deepseek".into(),
        model: "deepseek-chat".into(),
        status: "credential_locked".into(),
        request_count: 0,
        error_count: 0,
        last_latency_ms: None,
        average_latency_ms: None,
        tokens_per_second: None,
        credential: view.credential_handles.first().cloned(),
    });
    view
}

#[test]
fn d11_projection_contains_only_authoritative_safe_facts() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = GuiCoreAdapter::new(Box::new(D11Client::new(d11_view(), sent)));
    adapter.connect().expect("connect D11 client");

    let projection = adapter.projection().d11_intake().expect("D11 projection");
    assert_eq!(projection.project.as_ref().unwrap().root, "/workspace/demo");
    assert_eq!(
        projection.project.as_ref().unwrap().mode.as_deref(),
        Some("rust")
    );
    assert_eq!(
        projection
            .preview
            .as_ref()
            .unwrap()
            .exact_contents
            .as_deref(),
        Some("[project]\nname = \"demo\"\npack = \"rust\"\n")
    );
    assert_eq!(
        projection.provider.as_ref().unwrap().status,
        "credential_locked"
    );
    assert_eq!(projection.credential_handles.len(), 1);
    assert_ne!(
        projection.credential_handles[0].masked_handle,
        "keychain:deepseek-primary"
    );
    assert!(projection.recent_work.available);
    assert_eq!(projection.recent_work.code, "core_command");
    assert!(!projection.credential_ingress.available);

    let wire = serde_json::to_string(&projection).expect("serialize D11 projection");
    assert!(!wire.contains("api_key"));
    assert!(!wire.contains("secret"));
}

#[test]
fn d11_recent_work_availability_follows_the_core_capability() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut client = D11Client::new(d11_view(), Arc::clone(&sent));
    client.capabilities.remove("runtime.recent_work");
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter
        .connect()
        .expect("base handshake remains compatible");

    let projection = adapter.projection().d11_intake().expect("D11 projection");
    assert!(!projection.recent_work.available);
    assert_eq!(projection.recent_work.code, "capability_missing");
    assert!(
        projection
            .recent_work
            .message
            .contains("runtime.recent_work")
    );
}

#[test]
fn d11_projection_exposes_pending_approval_and_rejection_facts() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut view = d11_view();
    view.pending_approvals.push(ApprovalRequestView {
        id: "approval-config-1".into(),
        tool_name: "workflow_project_config_confirm".into(),
        title: "Confirm project config".into(),
        message: "Review exact bytes".into(),
        input_preview: "viden.toml".into(),
        is_mutating: true,
        reason: None,
        owner: Default::default(),
        risk: ApprovalRisk::Medium,
        target: ApprovalTarget {
            kind: "file".into(),
            display: "viden.toml".into(),
            canonical_ref: None,
        },
        allowed_scopes: vec![ApprovalScope::Once],
        policy_reason_key: "project_config_confirm".into(),
        policy_reason_args: Default::default(),
        expires_at: 0,
        default_action: ApprovalDefaultAction::Deny,
        audit_id: "audit-config-1".into(),
        decision_context: None,
    });
    view.pending_approvals.push(ApprovalRequestView {
        id: "approval-lane-1".into(),
        tool_name: "lane_create".into(),
        title: "Create starter lane".into(),
        message: "Review starter lane".into(),
        input_preview: "lane-starter-coder".into(),
        is_mutating: true,
        reason: None,
        owner: Default::default(),
        risk: ApprovalRisk::Medium,
        target: ApprovalTarget {
            kind: "lane".into(),
            display: "lane-starter-coder".into(),
            canonical_ref: None,
        },
        allowed_scopes: vec![ApprovalScope::Once],
        policy_reason_key: "lane_create".into(),
        policy_reason_args: Default::default(),
        expires_at: 0,
        default_action: ApprovalDefaultAction::Deny,
        audit_id: "audit-lane-1".into(),
        decision_context: None,
    });
    view.errors.push(RuntimeErrorView {
        message: "command confirm-1 rejected: permission denied".into(),
        recoverable: true,
        hint: None,
    });
    let mut adapter = GuiCoreAdapter::new(Box::new(D11Client::new(view, sent)));
    adapter.connect().expect("connect D11 client");

    let projection = adapter.projection().d11_intake().unwrap();
    assert_eq!(projection.pending_approval.unwrap().id, "approval-lane-1");
    assert_eq!(
        projection.last_error.as_deref(),
        Some("command confirm-1 rejected: permission denied")
    );
}

#[test]
fn d11_sends_only_typed_onboarding_commands_and_uses_the_core_preview_for_confirm() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = GuiCoreAdapter::new(Box::new(D11Client::new(d11_view(), Arc::clone(&sent))));
    adapter.connect().expect("connect D11 client");

    adapter
        .send_d11_intent("probe-1", D11Intent::ProbeProject)
        .expect("send probe");
    adapter
        .send_d11_intent(
            "preview-1",
            D11Intent::PreviewProjectConfig {
                contents: "[project]\nname = \"demo\"\npack = \"rust\"\n".into(),
            },
        )
        .expect("send preview");
    adapter
        .send_d11_intent("confirm-1", D11Intent::ConfirmProjectConfig)
        .expect("send confirm");
    let sent = sent.lock().expect("sent command lock");
    assert!(matches!(sent[0].command, RuntimeCommand::ProbeProject));
    assert!(matches!(
        sent[1].command,
        RuntimeCommand::PreviewProjectConfig { .. }
    ));
    assert!(matches!(
        &sent[2].command,
        RuntimeCommand::ConfirmProjectConfig { preview_id, content_sha256 }
            if preview_id == "preview-safe" && content_sha256 == &"a".repeat(64)
    ));
    let wire = serde_json::to_string(&*sent).expect("serialize sent commands");
    assert!(!wire.contains("raw-secret-value"));
}

#[test]
fn credential_handle_intent_is_disabled_without_a_secure_ingress_receipt() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = GuiCoreAdapter::new(Box::new(D11Client::new(d11_view(), Arc::clone(&sent))));
    adapter.connect().expect("connect D11 client");

    let error = adapter
        .send_d11_intent(
            "credential-1",
            D11Intent::StoreCredentialHandle {
                provider_id: "deepseek".into(),
                backend_id: "keychain".into(),
                credential_request_id: "request_opaque_1".into(),
            },
        )
        .expect_err("credential storage must remain disabled");

    assert!(error.contains("GUI-CORE-026"));
    assert!(sent.lock().expect("sent command lock").is_empty());
}

#[test]
fn send_and_wait_returns_the_projection_after_the_core_event() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut view = d11_view();
    view.project_config_preview = None;
    let contents = "[project]\nname = \"event-confirmed\"\n";
    let preview = preview(contents, "preview-after-event");
    let client = D11Client::new(view, sent)
        .with_event(accepted(
            "preview-1",
            RuntimeCommand::PreviewProjectConfig {
                contents: contents.into(),
            },
        ))
        .with_event(RuntimeEventKind::ProjectConfigPreviewed { preview });
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D11 client");

    let result = adapter
        .send_d11_intent_and_wait(
            "preview-1",
            D11Intent::PreviewProjectConfig {
                contents: contents.into(),
            },
            Duration::ZERO,
        )
        .expect("send and wait for Core preview event");

    assert_eq!(
        result.projection.preview.unwrap().preview_id,
        "preview-after-event"
    );
    assert_eq!(result.pending_command_id, None);
}

#[test]
fn poll_completes_a_pending_command_when_the_matching_fact_arrives_late() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let contents = "[project]\nname = \"late\"\n";
    let client = D11Client::new(d11_view(), sent)
        .with_event(accepted(
            "preview-late",
            RuntimeCommand::PreviewProjectConfig {
                contents: contents.into(),
            },
        ))
        .with_gap()
        .with_event(RuntimeEventKind::ProjectConfigPreviewed {
            preview: preview(contents, "preview-late-fact"),
        });
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D11 client");

    let pending = adapter
        .send_d11_intent_and_wait(
            "preview-late",
            D11Intent::PreviewProjectConfig {
                contents: contents.into(),
            },
            Duration::ZERO,
        )
        .expect("send pending preview");
    assert_eq!(pending.pending_command_id.as_deref(), Some("preview-late"));

    let completed = adapter
        .poll_d11(Duration::ZERO)
        .expect("poll late Core fact");
    assert_eq!(completed.pending_command_id, None);
    assert_eq!(
        completed.projection.preview.unwrap().preview_id,
        "preview-late-fact"
    );
}

#[test]
fn pending_poll_returns_intermediate_authoritative_provider_facts() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let contents = "[project]\nname = \"provider-update\"\n";
    let provider = ProviderHealthView {
        provider_id: "deepseek".into(),
        model: "deepseek-chat".into(),
        status: "offline".into(),
        request_count: 3,
        error_count: 1,
        last_latency_ms: None,
        average_latency_ms: None,
        tokens_per_second: None,
        credential: None,
    };
    let client = D11Client::new(d11_view(), sent)
        .with_event(accepted(
            "preview-provider",
            RuntimeCommand::PreviewProjectConfig {
                contents: contents.into(),
            },
        ))
        .with_event(RuntimeEventKind::ProviderHealthUpdated { provider })
        .with_gap();
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D11 client");

    let result = adapter
        .send_d11_intent_and_wait(
            "preview-provider",
            D11Intent::PreviewProjectConfig {
                contents: contents.into(),
            },
            Duration::ZERO,
        )
        .expect("send pending preview");

    assert_eq!(
        result.pending_command_id.as_deref(),
        Some("preview-provider")
    );
    assert_eq!(result.projection.provider.unwrap().status, "offline");
    assert_eq!(
        adapter
            .poll_d11(Duration::ZERO)
            .expect("poll retained pending target")
            .pending_command_id
            .as_deref(),
        Some("preview-provider")
    );
}

#[test]
fn approval_request_remains_visible_without_clearing_the_pending_target() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let hash = "a".repeat(64);
    let command = RuntimeCommand::ConfirmProjectConfig {
        preview_id: "preview-safe".into(),
        content_sha256: hash.clone(),
    };
    let approval = ApprovalRequestView {
        id: "approval-confirm-pending".into(),
        tool_name: "project_config_confirm".into(),
        title: "Confirm project config".into(),
        message: "Review exact bytes".into(),
        input_preview: format!("viden.toml sha256={hash}"),
        is_mutating: true,
        reason: None,
        owner: Default::default(),
        risk: ApprovalRisk::Medium,
        target: ApprovalTarget {
            kind: "file".into(),
            display: "viden.toml".into(),
            canonical_ref: None,
        },
        allowed_scopes: vec![ApprovalScope::Once],
        policy_reason_key: "project_config_confirm".into(),
        policy_reason_args: Default::default(),
        expires_at: 0,
        default_action: ApprovalDefaultAction::Deny,
        audit_id: "audit-confirm-pending".into(),
        decision_context: None,
    };
    let client = D11Client::new(d11_view(), sent)
        .with_event(accepted("confirm-pending", command))
        .with_event(RuntimeEventKind::ApprovalRequested { approval })
        .with_gap();
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D11 client");

    let result = adapter
        .send_d11_intent_and_wait(
            "confirm-pending",
            D11Intent::ConfirmProjectConfig,
            Duration::ZERO,
        )
        .expect("send confirmation awaiting approval");

    assert_eq!(
        result.pending_command_id.as_deref(),
        Some("confirm-pending")
    );
    assert_eq!(
        result.projection.pending_approval.unwrap().id,
        "approval-confirm-pending"
    );
}

#[test]
fn unrelated_approval_does_not_complete_or_retarget_the_pending_d11_command() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let command = RuntimeCommand::ConfirmProjectConfig {
        preview_id: "preview-safe".into(),
        content_sha256: "a".repeat(64),
    };
    let client = D11Client::new(d11_view(), sent)
        .with_event(accepted("confirm-pending", command))
        .with_event(RuntimeEventKind::ApprovalRequested {
            approval: approval_request("approval-other", "shell_exec", "command", "cargo test"),
        })
        .with_event(RuntimeEventKind::ApprovalResolved {
            request_id: "approval-other".into(),
            decision: ApprovalDecision::Deny,
            owner: Default::default(),
            audit_id: "audit-approval-other".into(),
        })
        .with_gap();
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D11 client");

    let result = adapter
        .send_d11_intent_and_wait(
            "confirm-pending",
            D11Intent::ConfirmProjectConfig,
            Duration::ZERO,
        )
        .expect("send confirmation with unrelated approval");

    assert_eq!(
        result.pending_command_id.as_deref(),
        Some("confirm-pending")
    );
    assert!(result.projection.pending_approval.is_none());
}

#[test]
fn different_sha_confirm_approval_does_not_retarget_the_pending_d11_command() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let pending_hash = "a".repeat(64);
    let unrelated_hash = "b".repeat(64);
    let command = RuntimeCommand::ConfirmProjectConfig {
        preview_id: "preview-safe".into(),
        content_sha256: pending_hash.clone(),
    };
    let client = D11Client::new(d11_view(), sent)
        .with_event(accepted("confirm-pending", command))
        .with_event(RuntimeEventKind::ApprovalRequested {
            approval: approval_request_with_preview(
                "approval-other-sha",
                "project_config_confirm",
                "file",
                "viden.toml",
                &format!("viden.toml sha256={unrelated_hash}"),
            ),
        })
        .with_gap();
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D11 client");

    let result = adapter
        .send_d11_intent_and_wait(
            "confirm-pending",
            D11Intent::ConfirmProjectConfig,
            Duration::ZERO,
        )
        .expect("send confirmation with same-tool different-sha approval");

    assert_eq!(
        result.pending_command_id.as_deref(),
        Some("confirm-pending")
    );
    assert!(result.projection.pending_approval.is_none());
}

#[test]
fn different_sha_confirm_approval_deny_or_expiry_does_not_clear_pending() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let pending_hash = "a".repeat(64);
    let unrelated_hash = "b".repeat(64);
    let command = RuntimeCommand::ConfirmProjectConfig {
        preview_id: "preview-safe".into(),
        content_sha256: pending_hash.clone(),
    };
    let client = D11Client::new(d11_view(), sent)
        .with_event(accepted("confirm-pending", command))
        .with_event(RuntimeEventKind::ApprovalRequested {
            approval: approval_request_with_preview(
                "approval-other-sha",
                "project_config_confirm",
                "file",
                "viden.toml",
                &format!("viden.toml sha256={unrelated_hash}"),
            ),
        })
        .with_event(RuntimeEventKind::ApprovalResolved {
            request_id: "approval-other-sha".into(),
            decision: ApprovalDecision::Deny,
            owner: Default::default(),
            audit_id: "audit-approval-other-sha".into(),
        })
        .with_gap();
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D11 client");

    let result = adapter
        .send_d11_intent_and_wait(
            "confirm-pending",
            D11Intent::ConfirmProjectConfig,
            Duration::ZERO,
        )
        .expect("different-sha denial must not clear active pending");

    assert_eq!(
        result.pending_command_id.as_deref(),
        Some("confirm-pending")
    );
    assert!(result.projection.pending_approval.is_none());
}

#[test]
fn malformed_confirm_approval_metadata_never_matches_by_substring() {
    let cases = [
        (
            "sha_suffix",
            format!("viden.toml sha256={}x", "a".repeat(64)),
        ),
        (
            "hash_prefix",
            format!("viden.toml sha256=x{}", "a".repeat(64)),
        ),
        (
            "free_text_suffix",
            format!("viden.toml {}x", "a".repeat(64)),
        ),
        (
            "free_text_prefix",
            format!("viden.toml x{}", "a".repeat(64)),
        ),
        (
            "wrong_field",
            format!("viden.toml old_sha256={}", "a".repeat(64)),
        ),
        ("uppercase", format!("viden.toml sha256={}", "A".repeat(64))),
        (
            "preview_id_suffix",
            format!(
                "viden.toml sha256={} preview_id=preview-safex",
                "a".repeat(64)
            ),
        ),
        (
            "preview_id_prefix",
            format!(
                "viden.toml sha256={} preview_id=xpreview-safe",
                "a".repeat(64)
            ),
        ),
    ];

    for (case, input_preview) in cases {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let command = RuntimeCommand::ConfirmProjectConfig {
            preview_id: "preview-safe".into(),
            content_sha256: "a".repeat(64),
        };
        let client = D11Client::new(d11_view(), sent)
            .with_event(accepted("confirm-pending", command))
            .with_event(RuntimeEventKind::ApprovalRequested {
                approval: approval_request_with_preview(
                    &format!("approval-{case}"),
                    "project_config_confirm",
                    "file",
                    "viden.toml",
                    &input_preview,
                ),
            })
            .with_gap();
        let mut adapter = GuiCoreAdapter::new(Box::new(client));
        adapter
            .connect()
            .unwrap_or_else(|error| panic!("connect D11 client for {case}: {error}"));

        let result = adapter
            .send_d11_intent_and_wait(
                "confirm-pending",
                D11Intent::ConfirmProjectConfig,
                Duration::ZERO,
            )
            .unwrap_or_else(|error| {
                panic!("send confirmation with malformed metadata {case}: {error}")
            });

        assert_eq!(
            result.pending_command_id.as_deref(),
            Some("confirm-pending"),
            "{case}"
        );
        assert!(
            result.projection.pending_approval.is_none(),
            "{case} must not expose a substring-matched approval"
        );
    }
}

#[test]
fn bounded_confirm_approval_metadata_accepts_exact_sha_and_preview_id_tokens() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let command = RuntimeCommand::ConfirmProjectConfig {
        preview_id: "preview-safe".into(),
        content_sha256: "a".repeat(64),
    };
    let client = D11Client::new(d11_view(), sent)
        .with_event(accepted("confirm-pending", command))
        .with_event(RuntimeEventKind::ApprovalRequested {
            approval: approval_request_with_preview(
                "approval-exact-fields",
                "project_config_confirm",
                "file",
                "viden.toml",
                &format!(
                    "viden.toml sha256={} preview_id=preview-safe",
                    "a".repeat(64)
                ),
            ),
        })
        .with_gap();
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D11 client");

    let result = adapter
        .send_d11_intent_and_wait(
            "confirm-pending",
            D11Intent::ConfirmProjectConfig,
            Duration::ZERO,
        )
        .expect("send confirmation with exact bounded metadata");

    assert_eq!(
        result.projection.pending_approval.unwrap().id,
        "approval-exact-fields"
    );
}

#[test]
fn later_unrelated_d11_approval_does_not_hide_the_active_pending_approval() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let pending_hash = "a".repeat(64);
    let other_hash = "b".repeat(64);
    let command = RuntimeCommand::ConfirmProjectConfig {
        preview_id: "preview-safe".into(),
        content_sha256: pending_hash.clone(),
    };
    let client = D11Client::new(d11_view(), sent)
        .with_event(accepted("confirm-pending", command))
        .with_event(RuntimeEventKind::ApprovalRequested {
            approval: approval_request_with_preview(
                "approval-active",
                "project_config_confirm",
                "file",
                "viden.toml",
                &format!("viden.toml sha256={pending_hash}"),
            ),
        })
        .with_event(RuntimeEventKind::ApprovalRequested {
            approval: approval_request_with_preview(
                "approval-later-other",
                "project_config_confirm",
                "file",
                "viden.toml",
                &format!("viden.toml sha256={other_hash}"),
            ),
        })
        .with_gap();
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D11 client");

    let result = adapter
        .send_d11_intent_and_wait(
            "confirm-pending",
            D11Intent::ConfirmProjectConfig,
            Duration::ZERO,
        )
        .expect("later unrelated D11 approval must not hide active approval");

    assert_eq!(
        result.pending_command_id.as_deref(),
        Some("confirm-pending")
    );
    assert_eq!(
        result.projection.pending_approval.unwrap().id,
        "approval-active"
    );
}

#[test]
fn approval_allow_continues_waiting_until_the_matching_business_fact_arrives() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let command = RuntimeCommand::ConfirmProjectConfig {
        preview_id: "preview-safe".into(),
        content_sha256: "a".repeat(64),
    };
    let client = D11Client::new(d11_view(), sent)
        .with_event(accepted("confirm-await-fact", command))
        .with_event(RuntimeEventKind::ApprovalRequested {
            approval: approval_request_with_preview(
                "approval-confirm",
                "project_config_confirm",
                "file",
                "viden.toml",
                &format!("viden.toml sha256={}", "a".repeat(64)),
            ),
        })
        .with_event(RuntimeEventKind::ApprovalResolved {
            request_id: "approval-confirm".into(),
            decision: ApprovalDecision::Allow {
                scope: ApprovalScope::Once,
            },
            owner: Default::default(),
            audit_id: "audit-approval-confirm".into(),
        })
        .with_gap()
        .with_event(RuntimeEventKind::ProjectConfigConfirmed {
            preview: ProjectConfigPreview {
                preview_id: "preview-safe".into(),
                relative_path: "viden.toml".into(),
                content_sha256: "a".repeat(64),
                byte_len: 38,
                exact_contents: Some("[project]\nname = \"demo\"\npack = \"rust\"\n".into()),
                base_content_sha256: None,
                project_name: Some("demo".into()),
                pack: Some("rust".into()),
                diagnostics: Vec::new(),
            },
        });
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D11 client");

    let waiting = adapter
        .send_d11_intent_and_wait(
            "confirm-await-fact",
            D11Intent::ConfirmProjectConfig,
            Duration::ZERO,
        )
        .expect("approval allow is not enough for D11 success");
    assert_eq!(
        waiting.pending_command_id.as_deref(),
        Some("confirm-await-fact")
    );

    let completed = adapter
        .poll_d11(Duration::ZERO)
        .expect("late confirmation fact completes pending D11 command");
    assert_eq!(completed.pending_command_id, None);
    assert!(completed.projection.confirmed_config.is_some());
}

#[test]
fn approval_deny_clears_pending_and_drains_the_following_rejection_error() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let command = RuntimeCommand::ConfirmProjectConfig {
        preview_id: "preview-safe".into(),
        content_sha256: "a".repeat(64),
    };
    let client = D11Client::new(d11_view(), sent)
        .with_event(accepted("confirm-denied", command))
        .with_event(RuntimeEventKind::ApprovalRequested {
            approval: approval_request_with_preview(
                "approval-denied",
                "project_config_confirm",
                "file",
                "viden.toml",
                &format!("viden.toml sha256={}", "a".repeat(64)),
            ),
        })
        .with_event(RuntimeEventKind::ApprovalResolved {
            request_id: "approval-denied".into(),
            decision: ApprovalDecision::Deny,
            owner: Default::default(),
            audit_id: "audit-approval-denied".into(),
        })
        .with_event(RuntimeEventKind::CommandRejected {
            command_id: "confirm-denied".into(),
            reason: "permission denied".into(),
        });
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D11 client");

    let denied = adapter
        .send_d11_intent_and_wait(
            "confirm-denied",
            D11Intent::ConfirmProjectConfig,
            Duration::ZERO,
        )
        .expect("approval denial is represented as Core state");

    assert_eq!(denied.pending_command_id, None);
    assert_eq!(
        denied.projection.last_error.as_deref(),
        Some("command confirm-denied rejected: permission denied")
    );
}

#[test]
fn matching_approval_deny_clears_pending_even_before_rejection_arrives() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let command = RuntimeCommand::ConfirmProjectConfig {
        preview_id: "preview-safe".into(),
        content_sha256: "a".repeat(64),
    };
    let client = D11Client::new(d11_view(), sent)
        .with_event(accepted("confirm-denied-no-reject", command))
        .with_event(RuntimeEventKind::ApprovalRequested {
            approval: approval_request_with_preview(
                "approval-denied-no-reject",
                "project_config_confirm",
                "file",
                "viden.toml",
                &format!("viden.toml sha256={}", "a".repeat(64)),
            ),
        })
        .with_event(RuntimeEventKind::ApprovalResolved {
            request_id: "approval-denied-no-reject".into(),
            decision: ApprovalDecision::Deny,
            owner: Default::default(),
            audit_id: "audit-approval-denied-no-reject".into(),
        })
        .with_gap();
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D11 client");

    let denied = adapter
        .send_d11_intent_and_wait(
            "confirm-denied-no-reject",
            D11Intent::ConfirmProjectConfig,
            Duration::ZERO,
        )
        .expect("matching denial ends the pending D11 command");

    assert_eq!(denied.pending_command_id, None);
}

#[test]
fn stale_preview_fact_cannot_complete_the_next_serialized_request() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let contents_a = "[project]\nname = \"a\"\n";
    let contents_b = "[project]\nname = \"b\"\n";
    let client = D11Client::new(d11_view(), sent)
        .with_event(accepted(
            "preview-a",
            RuntimeCommand::PreviewProjectConfig {
                contents: contents_a.into(),
            },
        ))
        .with_event(RuntimeEventKind::ProjectConfigPreviewed {
            preview: preview(contents_a, "fact-a"),
        })
        .with_event(accepted(
            "preview-b",
            RuntimeCommand::PreviewProjectConfig {
                contents: contents_b.into(),
            },
        ))
        .with_event(RuntimeEventKind::ProjectConfigPreviewed {
            preview: preview(contents_a, "stale-a"),
        })
        .with_event(RuntimeEventKind::ProjectConfigPreviewed {
            preview: preview(contents_b, "fact-b"),
        });
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D11 client");

    let first = adapter
        .send_d11_intent_and_wait(
            "preview-a",
            D11Intent::PreviewProjectConfig {
                contents: contents_a.into(),
            },
            Duration::ZERO,
        )
        .expect("complete preview A");
    assert_eq!(first.projection.preview.unwrap().preview_id, "fact-a");

    let second = adapter
        .send_d11_intent_and_wait(
            "preview-b",
            D11Intent::PreviewProjectConfig {
                contents: contents_b.into(),
            },
            Duration::ZERO,
        )
        .expect("complete preview B");
    assert_eq!(second.pending_command_id, None);
    assert_eq!(second.projection.preview.unwrap().preview_id, "fact-b");
}

#[test]
fn send_failure_restores_d11_controls_without_registering_pending_state() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = D11Client::new(d11_view(), Arc::clone(&sent))
        .with_send_error(CoreClientError::Transport("send failed".into()));
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D11 client");

    let error = adapter
        .send_d11_intent_and_wait("probe-send-fails", D11Intent::ProbeProject, Duration::ZERO)
        .expect_err("send failure returns before pending registration");
    assert!(error.contains("send failed"));
    assert!(sent.lock().expect("sent command lock").is_empty());

    let next = adapter
        .send_d11_intent_and_wait("probe-retry", D11Intent::ProbeProject, Duration::ZERO)
        .expect("retry is allowed because no stale pending state was registered");
    assert_eq!(next.pending_command_id.as_deref(), Some("probe-retry"));
}

#[test]
fn transient_poll_error_keeps_pending_identity_for_recovery() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let contents = "[project]\nname = \"recover\"\n";
    let client = D11Client::new(d11_view(), sent)
        .with_event(accepted(
            "preview-recover",
            RuntimeCommand::PreviewProjectConfig {
                contents: contents.into(),
            },
        ))
        .with_gap()
        .with_recv_error(CoreClientError::Transport("temporary poll failure".into()))
        .with_event(RuntimeEventKind::ProjectConfigPreviewed {
            preview: preview(contents, "preview-after-recover"),
        });
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D11 client");

    let pending = adapter
        .send_d11_intent_and_wait(
            "preview-recover",
            D11Intent::PreviewProjectConfig {
                contents: contents.into(),
            },
            Duration::ZERO,
        )
        .expect("send pending preview");
    assert_eq!(
        pending.pending_command_id.as_deref(),
        Some("preview-recover")
    );

    let error = adapter
        .poll_d11(Duration::ZERO)
        .expect_err("transient poll error is reported");
    assert!(error.contains("temporary poll failure"));

    let recovered = adapter
        .poll_d11(Duration::ZERO)
        .expect("late fact after transient poll failure completes request");
    assert_eq!(recovered.pending_command_id, None);
    assert_eq!(
        recovered.projection.preview.unwrap().preview_id,
        "preview-after-recover"
    );
}

#[test]
fn a_second_d11_command_is_rejected_while_the_first_is_pending() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = GuiCoreAdapter::new(Box::new(D11Client::new(d11_view(), sent)));
    adapter.connect().expect("connect D11 client");

    adapter
        .send_d11_intent_and_wait("probe-a", D11Intent::ProbeProject, Duration::ZERO)
        .expect("send pending probe A");
    let error = adapter
        .send_d11_intent_and_wait("probe-b", D11Intent::ProbeProject, Duration::ZERO)
        .expect_err("serialize D11 commands");

    assert!(error.contains("probe-a"));
}

#[test]
fn send_and_wait_reports_pending_when_core_publishes_no_event() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = GuiCoreAdapter::new(Box::new(D11Client::new(d11_view(), sent)));
    adapter.connect().expect("connect D11 client");

    let result = adapter
        .send_d11_intent_and_wait("probe-pending", D11Intent::ProbeProject, Duration::ZERO)
        .expect("send probe without a Core fact");

    assert_eq!(result.pending_command_id.as_deref(), Some("probe-pending"));
}

#[test]
fn send_and_wait_keeps_command_pending_after_acceptance_only() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let accepted = RuntimeEventKind::CommandAccepted {
        command_id: "preview-accepted".into(),
        command: RuntimeCommand::PreviewProjectConfig {
            contents: "[project]\nname = \"accepted-only\"\n".into(),
        },
    };
    let client = D11Client::new(d11_view(), sent).with_event(accepted);
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D11 client");

    let result = adapter
        .send_d11_intent_and_wait(
            "preview-accepted",
            D11Intent::PreviewProjectConfig {
                contents: "[project]\nname = \"accepted-only\"\n".into(),
            },
            Duration::ZERO,
        )
        .expect("send preview with acceptance only");

    assert_eq!(
        result.pending_command_id.as_deref(),
        Some("preview-accepted")
    );
}

#[test]
fn missing_extension_capability_rejects_the_command_before_transport() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut client = D11Client::new(d11_view(), Arc::clone(&sent));
    client.capabilities.remove("runtime.project_onboarding");
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter
        .connect()
        .expect("base handshake remains compatible");

    let error = adapter
        .send_d11_intent("probe-1", D11Intent::ProbeProject)
        .expect_err("missing extension must reject");
    assert!(error.to_string().contains("runtime.project_onboarding"));
    assert!(sent.lock().expect("sent command lock").is_empty());
}
