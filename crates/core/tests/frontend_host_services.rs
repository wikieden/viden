use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use viden_core::{
    CoreClient, LocalCoreHost, SecretBytes, WorkspaceOpenOverrides, WorkspaceOpenRequest,
};
use viden_session::SessionStore;
use viden_types::{
    FRONTEND_SCHEMA_V1, Message, PermissionMode, RecentWorkQuery, Role, RuntimeCommand,
    RuntimeCommandEnvelope, RuntimeOwner, TranscriptEntry,
};

static VIDEN_HOME_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn temp_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("viden-core-host-{label}-{unique}"));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn missing_temp_path(label: &str) -> PathBuf {
    let base = temp_dir(label);
    base.join("missing-home")
}

#[test]
fn local_host_opens_two_workspaces_without_state_bleed() {
    let home = temp_dir("home");
    let project_a = temp_dir("project-a");
    let project_b = temp_dir("project-b");
    let host = LocalCoreHost::with_session_home(home);

    let mut a = host
        .open_workspace(WorkspaceOpenRequest::new(project_a.clone()))
        .unwrap();
    let mut b = host
        .open_workspace(WorkspaceOpenRequest::new(project_b.clone()))
        .unwrap();

    assert_ne!(a.binding().canonical_root, b.binding().canonical_root);
    assert_ne!(a.binding().session_id, b.binding().session_id);
    assert_ne!(a.binding().stream_id, b.binding().stream_id);
    assert_eq!(
        a.client().snapshot().unwrap().view.snapshot.cwd,
        project_a.canonicalize().unwrap()
    );
    assert_eq!(
        b.client().snapshot().unwrap().view.snapshot.cwd,
        project_b.canonicalize().unwrap()
    );
}

#[test]
fn production_local_host_default_uses_one_shared_recent_work_home() {
    let _guard = VIDEN_HOME_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap();
    let home = temp_dir("default-shared-home");
    let project_a = temp_dir("default-shared-project-a");
    let project_b = temp_dir("default-shared-project-b");
    let previous_home = std::env::var_os("VIDEN_HOME");
    // SAFETY: this integration-test process serializes every VIDEN_HOME mutation
    // with the static lock and restores the previous value before returning.
    unsafe { std::env::set_var("VIDEN_HOME", &home) };

    let result = (|| {
        let host = LocalCoreHost::new();
        let a = host.open_workspace(WorkspaceOpenRequest::new(&project_a))?;
        let mut b = host.open_workspace(WorkspaceOpenRequest::new(&project_b))?;
        b.client().send(RuntimeCommandEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            client_id: "recent-work-test".to_string(),
            command_id: "query-recent-work".to_string(),
            owner: RuntimeOwner::default(),
            command: RuntimeCommand::QueryRecentWork {
                query: RecentWorkQuery { limit: 10 },
            },
        })?;
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(25));
            let snapshot = b.client().snapshot()?;
            if snapshot.view.recent_sessions.len() == 2 {
                let identities = snapshot
                    .view
                    .recent_sessions
                    .iter()
                    .map(|session| (session.canonical_root.clone(), session.session_id.clone()))
                    .collect::<Vec<_>>();
                assert!(
                    identities
                        .iter()
                        .any(|(_, id)| id == &a.binding().session_id)
                );
                assert!(
                    identities
                        .iter()
                        .any(|(_, id)| id == &b.binding().session_id)
                );
                return Ok::<(), Box<dyn std::error::Error>>(());
            }
        }
        panic!("default production host did not expose both project sessions");
    })();

    match previous_home {
        Some(value) => unsafe { std::env::set_var("VIDEN_HOME", value) },
        None => unsafe { std::env::remove_var("VIDEN_HOME") },
    }
    result.unwrap();
}

#[test]
fn local_host_rejects_missing_roots_and_files_before_bootstrap() {
    let home = temp_dir("reject-home");
    let project = temp_dir("reject-project");
    let file = project.join("not-a-directory.txt");
    std::fs::write(&file, "not a workspace").unwrap();
    let host = LocalCoreHost::with_session_home(home);

    assert!(
        host.open_workspace(WorkspaceOpenRequest::new(project.join("missing")))
            .is_err()
    );
    assert!(
        host.open_workspace(WorkspaceOpenRequest::new(file))
            .is_err()
    );
}

#[test]
fn local_host_resumes_exact_session_without_returning_a_fresh_binding() {
    let home = temp_dir("resume-home");
    let project = temp_dir("resume-project");
    let session_id = seed_session(&home, &project, "session_exact_resume", "existing work");
    let host = LocalCoreHost::with_session_home(home);

    let binding = host
        .open_workspace(
            WorkspaceOpenRequest::new(project).with_resume_session_id(session_id.clone()),
        )
        .unwrap()
        .binding()
        .clone();

    assert_eq!(binding.session_id, session_id);
}

#[test]
fn local_host_rejects_missing_resume_without_returning_a_fresh_binding() {
    let home = temp_dir("missing-resume-home");
    let project = temp_dir("missing-resume-project");
    seed_session(&home, &project, "session_existing", "existing work");
    let host = LocalCoreHost::with_session_home(home);

    let error = match host.open_workspace(
        WorkspaceOpenRequest::new(project).with_resume_session_id("session_missing"),
    ) {
        Ok(_) => panic!("missing resume must not return a fresh binding"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("session_missing"));
    assert!(error.to_string().contains("not found"));
}

#[test]
fn local_host_rejects_ambiguous_resume_without_returning_a_fresh_binding() {
    let home = temp_dir("ambiguous-resume-home");
    let project = temp_dir("ambiguous-resume-project");
    seed_session(&home, &project, "session_ambiguous_a", "existing work a");
    seed_session(&home, &project, "session_ambiguous_b", "existing work b");
    let host = LocalCoreHost::with_session_home(home);

    let error = match host.open_workspace(
        WorkspaceOpenRequest::new(project).with_resume_session_id("session_ambiguous"),
    ) {
        Ok(_) => panic!("ambiguous resume must not return a fresh binding"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("session_ambiguous"));
    assert!(error.to_string().contains("ambiguous"));
}

#[test]
fn local_host_missing_resume_does_not_create_pristine_session_home() {
    let home = missing_temp_path("pristine-missing-resume-home");
    let project = temp_dir("pristine-missing-resume-project");
    let host = LocalCoreHost::with_session_home(home.clone());

    let error = match host.open_workspace(
        WorkspaceOpenRequest::new(project).with_resume_session_id("session_missing"),
    ) {
        Ok(_) => panic!("missing resume must not return a fresh binding"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("session_missing"));
    assert!(!home.exists(), "missing resume lookup must not create home");
}

#[test]
fn local_host_missing_resume_preserves_existing_empty_session_home() {
    let home = temp_dir("empty-missing-resume-home");
    let project = temp_dir("empty-missing-resume-project");
    let sentinel = home.join("sentinel.txt");
    std::fs::write(&sentinel, "unchanged").unwrap();
    let before = std::fs::metadata(&sentinel).unwrap();
    let host = LocalCoreHost::with_session_home(home.clone());

    let error = match host.open_workspace(
        WorkspaceOpenRequest::new(project).with_resume_session_id("session_missing"),
    ) {
        Ok(_) => panic!("missing resume must not return a fresh binding"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("session_missing"));
    assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "unchanged");
    assert_eq!(std::fs::metadata(&sentinel).unwrap().len(), before.len());
    #[cfg(unix)]
    assert_eq!(
        std::fs::metadata(&sentinel).unwrap().permissions().mode(),
        before.permissions().mode()
    );
    assert_eq!(
        std::fs::metadata(&sentinel)
            .unwrap()
            .permissions()
            .readonly(),
        before.permissions().readonly()
    );
    assert!(!home.join("projects").exists());
    assert!(!home.join("index.sqlite3").exists());
}

#[test]
fn workspace_open_request_debug_never_accepts_or_prints_raw_api_keys() {
    let request = WorkspaceOpenRequest::new(temp_dir("debug-project")).with_overrides(
        WorkspaceOpenOverrides {
            provider: Some("deepseek".to_string()),
            model: Some("deepseek-v4-flash".to_string()),
            permission_mode: Some(PermissionMode::Plan),
            ..WorkspaceOpenOverrides::default()
        },
    );

    let rendered = format!("{request:?}");
    assert!(!rendered.contains("sk-test-secret"));
    assert!(!rendered.contains("api_key"));

    let core_lib =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
            .unwrap();
    assert!(!core_lib.contains("pub use viden_config::CliOverrides"));
}

#[test]
fn production_staged_secret_uses_bound_client_and_unavailable_sink_is_one_use_and_redacted() {
    let home = temp_dir("credential-home");
    let project = temp_dir("credential-project");
    let host = LocalCoreHost::with_session_home(home.clone());
    let mut client = host
        .open_workspace(
            WorkspaceOpenRequest::new(project).with_overrides(WorkspaceOpenOverrides {
                permission_mode: Some(PermissionMode::DontAsk),
                ..WorkspaceOpenOverrides::default()
            }),
        )
        .unwrap();

    let request = client
        .stage_credential(
            "sequence",
            "test-keychain:item-1",
            SecretBytes::new(b"sk-test".to_vec()),
        )
        .unwrap();

    assert!(!format!("{request:?}").contains("sk-test"));
    assert!(!serde_json::to_string(&request).unwrap().contains("sk-test"));
    let command = store_handle_command(
        "store-unavailable",
        "sequence",
        "test-keychain:item-1",
        request.id().to_string(),
    );
    assert!(!serde_json::to_string(&command).unwrap().contains("sk-test"));
    client.client().send(command).unwrap();
    let unavailable = snapshot_until_rejected(client.client(), "store-unavailable");
    assert!(unavailable.contains("credential platform sink unavailable"));
    assert!(!unavailable.contains("sk-test"));

    client
        .client()
        .send(store_handle_command(
            "store-replay",
            "sequence",
            "test-keychain:item-1",
            request.id().to_string(),
        ))
        .unwrap();
    let replay = snapshot_until_rejected(client.client(), "store-replay");
    assert!(replay.contains("credential request"));
    assert!(!read_all_jsonl(&home).contains("sk-test"));
}

#[test]
fn production_core_host_api_exposes_no_test_sink_or_arbitrary_binding_staging() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let core_host = std::fs::read_to_string(manifest_dir.join("src/host.rs")).unwrap();
    let core_lib = std::fs::read_to_string(manifest_dir.join("src/lib.rs")).unwrap();

    assert!(!core_host.contains("pub fn for_test"));
    assert!(!core_host.contains("pub fn stage_credential_for_binding"));
    assert!(!core_host.contains("pub fn with_credential_capacity_for_test"));
    assert!(!core_host.contains("pub fn with_credential_clock_for_test"));
    assert!(!core_host.contains("pub fn fail_next_credential_sink_for_test"));
    assert!(!core_lib.contains("stage_credential_for_binding"));
    assert!(core_host.contains(
        "pub fn stage_credential(
        &self,"
    ));
    for trait_name in ["Clone", "Debug", "serde::Serialize", "serde::Deserialize"] {
        assert!(!core_host.contains(&format!("SecretBytes, {trait_name}")));
    }
}

fn seed_session(home: &std::path::Path, project: &std::path::Path, id: &str, text: &str) -> String {
    let store =
        SessionStore::new_with_home(home, project.canonicalize().unwrap(), Some(id.to_string()))
            .unwrap();
    store
        .append_entry(&TranscriptEntry::Message {
            message: Message::new(Role::User, text.to_string()),
        })
        .unwrap();
    store.session_id().to_string()
}

fn store_handle_command(
    command_id: &str,
    provider_id: &str,
    backend_id: &str,
    credential_request_id: String,
) -> RuntimeCommandEnvelope {
    RuntimeCommandEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        client_id: "frontend-host-test".to_string(),
        command_id: command_id.to_string(),
        owner: RuntimeOwner::default(),
        command: RuntimeCommand::StoreCredentialHandle {
            provider_id: provider_id.to_string(),
            backend_id: backend_id.to_string(),
            credential_request_id,
        },
    }
}

fn snapshot_until_rejected(client: &mut impl CoreClient, command_id: &str) -> String {
    let mut seen = Vec::new();
    for _ in 0..16 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let snapshot = client.snapshot().expect("snapshot");
        seen.extend(
            snapshot
                .view
                .errors
                .iter()
                .map(|error| error.message.clone()),
        );
        if let Some(error) = snapshot.view.errors.iter().find(|error| {
            error
                .message
                .contains(&format!("command {command_id} rejected:"))
        }) {
            return error.message.clone();
        }
    }
    panic!("missing rejection for {command_id}; seen {seen:?}");
}

fn read_all_jsonl(root: &std::path::Path) -> String {
    fn visit(path: &std::path::Path, out: &mut String) {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit(&path, out);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "jsonl")
                && let Ok(contents) = std::fs::read_to_string(&path)
            {
                out.push_str(&contents);
            }
        }
    }
    let mut out = String::new();
    visit(root, &mut out);
    out
}

#[test]
fn fixture_frontend_host_services_is_registered_and_fully_redacted() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("types/tests/fixtures/frontend-contract-v1/frontend-host-services.json");
    let raw = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|error| panic!("read {}: {error}", fixture.display()));
    let value: serde_json::Value = serde_json::from_str(&raw).expect("valid fixture json");
    assert_eq!(value["schema_version"], FRONTEND_SCHEMA_V1.0);
    assert_eq!(value["fixture_id"], "frontend-host-services");
    for forbidden in [
        "/Users/",
        "\\Users\\",
        "sk-",
        "OPENAI_API_KEY",
        "password",
        "credential_request_id",
        "backend_id",
        "transcript_path",
    ] {
        assert!(
            !raw.contains(forbidden),
            "fixture contains forbidden private marker `{forbidden}`"
        );
    }
}

// --- runtime.workspace_owner (C5, GUI-CORE-027) ------------------------------

/// Opening a workspace mints its identity and publishes it: the binding the
/// host hands back carries both ids, `.viden/project.toml` holds the durable
/// half, and the snapshot a client reads carries the owner its
/// workspace-target commands must send.
#[test]
fn opening_a_workspace_mints_and_publishes_its_operator_identity() {
    let home = temp_dir("workspace-owner-home");
    let project = temp_dir("workspace-owner-project");
    let host = LocalCoreHost::with_session_home(home);

    let mut opened = host
        .open_workspace(WorkspaceOpenRequest::new(project.clone()))
        .unwrap();

    let binding = opened.binding().clone();
    assert!(binding.workspace_id.starts_with("ws_"));
    assert!(binding.project_id.starts_with("prj_"));
    assert!(project.join(".viden").join("project.toml").is_file());

    let snapshot = opened.client().snapshot().unwrap();
    let owner = snapshot
        .view
        .workspace_owner
        .clone()
        .expect("an opened workspace publishes its owner");
    assert_eq!(owner.workspace_id, binding.workspace_id);
    assert_eq!(owner.project_id, binding.project_id);
    assert_eq!(owner.lane_id, None);
    assert_eq!(owner.session_id, None);
    assert_eq!(owner.task_id, None);
    assert_eq!(owner.turn_id, None);
}

/// The workspace id names a location and the project id names the project.
/// Reopening the same directory reproduces both; a different directory
/// reproduces neither.
#[test]
fn reopening_a_workspace_reproduces_both_ids_and_a_new_one_shares_none() {
    let home = temp_dir("workspace-owner-stable-home");
    let project = temp_dir("workspace-owner-stable-project");
    let other = temp_dir("workspace-owner-other-project");
    let host = LocalCoreHost::with_session_home(home);

    let first = host
        .open_workspace(WorkspaceOpenRequest::new(project.clone()))
        .unwrap()
        .binding()
        .clone();
    let second = host
        .open_workspace(WorkspaceOpenRequest::new(project))
        .unwrap()
        .binding()
        .clone();
    let elsewhere = host
        .open_workspace(WorkspaceOpenRequest::new(other))
        .unwrap()
        .binding()
        .clone();

    assert_eq!(first.workspace_id, second.workspace_id);
    assert_eq!(first.project_id, second.project_id);
    assert_ne!(first.workspace_id, elsewhere.workspace_id);
    assert_ne!(first.project_id, elsewhere.project_id);
}
