use std::{
    collections::{BTreeMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, mpsc},
    time::{Duration, Instant, SystemTime},
};

use crate::RuntimeEventSink;
use serde_json::{Value, json};
use viden_permissions::{PermissionContext, PermissionEngine};
use viden_plugin_api::{
    AgentPermissionProfile, AgentPluginCapability, AgentPluginDescriptor, AgentSource,
    AgentTransport,
};
use viden_plugin_host::builtin_agent_descriptors;
use viden_tools::{FilesystemCapability, LocalFilesystem, LocalProcess, ProcessCapability};
use viden_types::{
    AgentAuthState, AgentAvailability, AgentStartability, ApprovalResponse, MergeGateStatus,
    RuntimeEvent, RuntimeEventKind,
};

use super::acp::*;
use super::codex::*;
use super::glue::*;
use super::infra::*;
use std::cell::{Cell, RefCell};
use std::sync::{
    Mutex, MutexGuard, OnceLock,
    atomic::{AtomicU64, Ordering},
};
use viden_plugin_api::{AgentAuthMode, AgentCommandSpec, AgentProtocolVersion};

static TEMP_ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);
static SUBPROCESS_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn local_fs_capability() -> Arc<dyn FilesystemCapability> {
    Arc::new(LocalFilesystem)
}

fn local_process_capability() -> Arc<dyn ProcessCapability> {
    Arc::new(LocalProcess)
}

#[test]
fn codex_run_args_default_to_read_only_and_require_explicit_write() {
    let read_only =
        parse_codex_run_args(&["summarize".into(), "repo".into()]).expect("parse read-only task");
    assert!(!read_only.write);
    assert!(!read_only.app_server);
    assert_eq!(read_only.task, "summarize repo");

    let write = parse_codex_run_args(&["--write".into(), "edit".into(), "file".into()])
        .expect("parse write task");
    assert!(write.write);
    assert_eq!(write.task, "edit file");

    let app_server =
        parse_codex_run_args(&["--app-server".into(), "summarize".into(), "status".into()])
            .expect("parse app-server task");
    assert!(app_server.app_server);
    assert_eq!(app_server.task, "summarize status");

    let args = codex_run_command_args(Path::new("/repo"), "workspace-write", write.task);
    assert_eq!(
        args,
        vec![
            "exec",
            "--cd",
            "/repo",
            "--sandbox",
            "workspace-write",
            "edit file"
        ]
    );
}

#[test]
fn codex_probe_args_support_opt_in_write_turns() {
    assert_eq!(
        parse_codex_probe_args(&["--turn".into(), "summarize".into()])
            .expect("parse read-only turn"),
        CodexProbeMode::Turn {
            task: "summarize".to_string(),
            write: false,
        }
    );
    assert_eq!(
        parse_codex_probe_args(&["--turn-write".into(), "edit".into(), "file".into()])
            .expect("parse write turn"),
        CodexProbeMode::Turn {
            task: "edit file".to_string(),
            write: true,
        }
    );
}

#[test]
fn codex_app_server_write_probe_requests_workspace_write_with_approval() {
    let cwd = Path::new("/tmp/viden-write-probe");
    let thread = codex_app_server_thread_start_request(cwd, true);
    let turn = codex_app_server_turn_start_request(cwd, "thread_1", "edit file", true);

    assert!(thread.contains(r#""approvalPolicy":"on-request""#));
    assert!(thread.contains(r#""sandbox":"workspace-write""#));
    assert!(turn.contains(r#""approvalPolicy":"on-request""#));
    assert!(turn.contains(r#""type":"workspaceWrite""#));
    assert!(turn.contains(r#""writableRoots":["/tmp/viden-write-probe"]"#));
    assert!(turn.contains(r#""networkAccess":false"#));
}

#[test]
fn codex_protocol_probe_reads_generated_schema_surface() {
    let root = temp_root("codex_schema_probe");
    fs::write(
        root.join("ClientRequest.json"),
        r#"{"enum":["thread/start","thread/resume","thread/read","review/start","turn/start","turn/interrupt"]}"#,
    )
    .expect("write client schema");
    fs::write(
        root.join("ServerNotification.json"),
        r#"{"enum":["thread/started","turn/started","turn/completed","item/commandExecution/outputDelta","item/fileChange/outputDelta","turn/diff/updated"]}"#,
    )
    .expect("write server notification schema");
    fs::write(
        root.join("ServerRequest.json"),
        r#"{"enum":["item/commandExecution/requestApproval","item/fileChange/requestApproval","item/permissions/requestApproval"]}"#,
    )
    .expect("write server request schema");

    let report = codex_protocol_probe_from_dir(&root).expect("schema probe");

    assert!(report.missing.is_empty());
    assert_eq!(
        report.available,
        vec![
            "thread lifecycle",
            "review",
            "turn control",
            "events",
            "evidence",
            "approvals"
        ]
    );
}

#[test]
fn acp_file_effects_flow_through_the_filesystem_capability() {
    // ACP client-requested reads and writes are model-driven effects and
    // must be OS-detached: swapping an in-memory capability must capture
    // them completely, leaving the real disk untouched.
    #[derive(Default)]
    struct MemoryFilesystem {
        files: Mutex<BTreeMap<PathBuf, String>>,
    }

    impl viden_tools::FilesystemCapability for MemoryFilesystem {
        fn is_dir(&self, _path: &Path) -> bool {
            false
        }

        fn read(&self, path: &Path) -> Result<Vec<u8>, String> {
            self.read_to_string(path).map(String::into_bytes)
        }

        fn read_to_string(&self, path: &Path) -> Result<String, String> {
            self.files
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .ok_or_else(|| format!("memory fs has no file at {}", path.display()))
        }

        fn create_dir_all(&self, _path: &Path) -> Result<(), String> {
            Ok(())
        }

        fn write(&self, path: &Path, contents: &str) -> Result<(), String> {
            self.files
                .lock()
                .unwrap()
                .insert(path.to_path_buf(), contents.to_string());
            Ok(())
        }
    }

    let root = temp_root("acp_memory_fs_effects");
    let target = root.join("captured.txt");
    let memory = Arc::new(MemoryFilesystem::default());
    let fs_capability: Arc<dyn viden_tools::FilesystemCapability> = memory.clone();
    let mut permission_engine = PermissionEngine::new(&root);
    let mut approver = |_prompt: viden_types::PermissionPrompt| ApprovalResponse::allow_once(None);

    let write_request = json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "fs/write_text_file",
        "params": {
            "path": target.display().to_string(),
            "content": "captured by memory fs"
        }
    });
    let response = acp_filesystem_client_request_response(
        &root,
        &mut permission_engine,
        &mut approver,
        &fs_capability,
        &write_request,
    )
    .expect("write request should be handled");
    assert!(response.contains(r#""result":{}"#), "got: {response}");
    assert_eq!(
        memory
            .files
            .lock()
            .unwrap()
            .get(&target)
            .map(String::as_str),
        Some("captured by memory fs"),
        "the ACP write must land in the swapped capability"
    );
    assert!(
        !target.exists(),
        "the ACP write must not touch the real filesystem"
    );

    let read_request = json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "fs/read_text_file",
        "params": {
            "path": target.display().to_string()
        }
    });
    let response = acp_filesystem_client_request_response(
        &root,
        &mut permission_engine,
        &mut approver,
        &fs_capability,
        &read_request,
    )
    .expect("read request should be handled");
    assert!(
        response.contains("captured by memory fs"),
        "the ACP read must come from the swapped capability, got: {response}"
    );
}

#[test]
fn acp_shell_command_uses_script_for_long_commands() {
    let long_command = format!("printf ok\n# {}", "x".repeat(40 * 1024));
    let plan = shell_command_plan(&long_command, false);

    assert_eq!(plan.program, "sh");
    assert!(plan.inline_args.is_empty());
    assert_eq!(plan.script_extension, Some("sh"));
    assert_eq!(
        plan.script_body.as_deref(),
        Some(format!("set -eu\n{long_command}\n").as_str())
    );
}

#[test]
fn acp_shell_command_keeps_short_commands_inline() {
    let plan = shell_command_plan("printf ok", false);

    assert_eq!(plan.program, "sh");
    assert_eq!(
        plan.inline_args,
        vec!["-lc".to_string(), "printf ok".to_string()]
    );
    assert!(plan.script_extension.is_none());
    assert!(plan.script_body.is_none());
}

#[test]
fn acp_shell_command_writes_long_command_script() {
    let cwd = temp_root("acp_shell_script");
    let long_command = format!("printf ok\n# {}", "x".repeat(40 * 1024));

    let _command = shell_command(&cwd, &long_command).expect("build shell command");

    let tmp_dir = cwd.join(".viden").join("tmp");
    let scripts = fs::read_dir(&tmp_dir)
        .expect("read tmp dir")
        .map(|entry| entry.expect("script entry").path())
        .collect::<Vec<_>>();
    assert_eq!(scripts.len(), 1);
    let script = fs::read_to_string(&scripts[0]).expect("read script");
    assert!(script.starts_with("set -eu\nprintf ok"));
    assert!(script.ends_with('\n'));
}

#[cfg(unix)]
#[test]
fn codex_app_server_initialize_probe_records_jsonl_evidence() {
    let _guard = subprocess_test_guard();
    let root = temp_root("codex_app_server_probe");
    let script = root.join("mock-codex-app-server.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "if [ \"$1\" != \"app-server\" ]; then exit 2; fi",
            "read _line",
            "printf '%s\\n' '{\"id\":1,\"result\":{\"userAgent\":\"Codex Desktop/mock (viden; test)\",\"codexHome\":\"/tmp/codex-home\",\"platformFamily\":\"unix\",\"platformOs\":\"macos\"}}'",
            "printf '%s\\n' '{\"method\":\"remoteControl/status/changed\",\"params\":{\"status\":\"disabled\"}}'",
            "sleep 1",
        ]
        .join("\n"),
    )
    .expect("write mock codex app-server script");
    make_executable(&script);

    let evidence =
        run_codex_app_server_probe(&root, &script.to_string_lossy(), CodexProbeMode::Initialize)
            .expect("probe succeeds");

    assert_eq!(evidence.user_agent, "Codex Desktop/mock (viden; test)");
    assert_eq!(evidence.codex_home, "/tmp/codex-home");
    assert_eq!(evidence.platform, "macos");
    assert_eq!(evidence.thread_id, None);
    assert_eq!(
        evidence.notifications,
        vec!["remoteControl/status/changed".to_string()]
    );
    let log = fs::read_to_string(&evidence.log_path).expect("read jsonl log");
    assert!(log.contains(r#""method\":\"initialize"#));
    assert!(log.contains("Codex Desktop/mock"));
}

#[cfg(unix)]
#[test]
fn codex_app_server_thread_probe_records_thread_evidence() {
    let _guard = subprocess_test_guard();
    let root = temp_root("codex_app_server_thread_probe");
    let script = root.join("mock-codex-thread.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "if [ \"$1\" != \"app-server\" ]; then exit 2; fi",
            "read init",
            "case \"$init\" in *'\"experimentalApi\":true'*) ;; *) exit 3 ;; esac",
            "printf '%s\\n' '{\"id\":1,\"result\":{\"userAgent\":\"Codex Desktop/mock\",\"codexHome\":\"/tmp/codex-home\",\"platformFamily\":\"unix\",\"platformOs\":\"macos\"}}'",
            "read thread",
            "case \"$thread\" in *'\"method\":\"thread/start\"'*) ;; *) exit 4 ;; esac",
            "printf '%s\\n' '{\"id\":2,\"result\":{\"thread\":{\"id\":\"thread_123\",\"sessionId\":\"thread_123\",\"turns\":[]},\"model\":\"gpt-test\"}}'",
            "printf '%s\\n' '{\"method\":\"thread/started\",\"params\":{\"thread\":{\"id\":\"thread_123\"}}}'",
            "sleep 1",
        ]
        .join("\n"),
    )
    .expect("write mock codex thread script");
    make_executable(&script);

    let evidence =
        run_codex_app_server_probe(&root, &script.to_string_lossy(), CodexProbeMode::Thread)
            .expect("probe succeeds");

    assert_eq!(evidence.thread_id, Some("thread_123".to_string()));
    assert_eq!(evidence.notifications, vec!["thread/started".to_string()]);
    let log = fs::read_to_string(&evidence.log_path).expect("read jsonl log");
    assert!(log.contains(r#"thread/start"#));
    assert!(log.contains("thread_123"));
}

#[cfg(unix)]
#[test]
fn codex_app_server_turn_probe_records_turn_evidence() {
    let _guard = subprocess_test_guard();
    let root = temp_root("codex_app_server_turn_probe");
    let script = root.join("mock-codex-turn.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "if [ \"$1\" != \"app-server\" ]; then exit 2; fi",
            "read init",
            "printf '%s\\n' '{\"id\":1,\"result\":{\"userAgent\":\"Codex Desktop/mock\",\"codexHome\":\"/tmp/codex-home\",\"platformFamily\":\"unix\",\"platformOs\":\"macos\"}}'",
            "read thread",
            "printf '%s\\n' '{\"id\":2,\"result\":{\"thread\":{\"id\":\"thread_456\",\"sessionId\":\"thread_456\",\"turns\":[]},\"model\":\"gpt-test\"}}'",
            "printf '%s\\n' '{\"method\":\"thread/started\",\"params\":{\"thread\":{\"id\":\"thread_456\"}}}'",
            "read turn",
            "case \"$turn\" in *'\"method\":\"turn/start\"'*'summarize status'*) ;; *) exit 4 ;; esac",
            "printf '%s\\n' '{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_456\",\"items\":[],\"itemsView\":\"complete\",\"status\":\"inProgress\",\"error\":null,\"startedAt\":1,\"completedAt\":null,\"durationMs\":null}}}'",
            "printf '%s\\n' '{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thread_456\",\"turn\":{\"id\":\"turn_456\",\"status\":\"inProgress\"}}}'",
            "printf '%s\\n' '{\"method\":\"item/agentMessage/delta\",\"params\":{\"threadId\":\"thread_456\",\"turnId\":\"turn_456\",\"delta\":\"ok\"}}'",
            "printf '%s\\n' '{\"method\":\"item/started\",\"params\":{\"threadId\":\"thread_456\",\"turnId\":\"turn_456\",\"item\":{\"type\":\"mcpToolCall\",\"id\":\"call_1\",\"server\":\"node_repl\",\"tool\":\"js\",\"status\":\"inProgress\",\"arguments\":{\"code\":\"await fs.writeFile(\\\\\"live.txt\\\\\", \\\\\"ok\\\\\")\"}}}}'",
            "printf '%s\\n' '{\"method\":\"item/completed\",\"params\":{\"threadId\":\"thread_456\",\"turnId\":\"turn_456\",\"item\":{\"type\":\"mcpToolCall\",\"id\":\"call_1\",\"server\":\"node_repl\",\"tool\":\"js\",\"status\":\"completed\",\"arguments\":{\"code\":\"await fs.writeFile(\\\\\"live.txt\\\\\", \\\\\"ok\\\\\")\"},\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"ok\"}]}}}}'",
            "printf '%s\\n' '{\"id\":9,\"method\":\"item/commandExecution/requestApproval\",\"params\":{\"threadId\":\"thread_456\",\"turnId\":\"turn_456\",\"itemId\":\"item_1\",\"startedAtMs\":1,\"command\":\"cargo test\",\"cwd\":\"/tmp\"}}'",
            "read approval",
            "case \"$approval\" in *'\"id\":9'*'\"decision\":\"decline\"'*) ;; *) exit 5 ;; esac",
            "printf '%s\\n' '{\"method\":\"item/completed\",\"params\":{\"threadId\":\"thread_456\",\"turnId\":\"turn_456\",\"item\":{\"type\":\"agentMessage\",\"text\":\"turn probe complete\"}}}'",
            "printf '%s\\n' '{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thread_456\",\"turn\":{\"id\":\"turn_456\",\"status\":\"completed\"}}}'",
            "sleep 1",
        ]
        .join("\n"),
    )
    .expect("write mock codex turn script");
    make_executable(&script);

    let evidence = run_codex_app_server_probe(
        &root,
        &script.to_string_lossy(),
        CodexProbeMode::Turn {
            task: "summarize status".to_string(),
            write: false,
        },
    )
    .expect("probe succeeds");

    assert_eq!(evidence.thread_id, Some("thread_456".to_string()));
    assert_eq!(evidence.turn_id, Some("turn_456".to_string()));
    assert_eq!(evidence.turn_status, Some("completed".to_string()));
    assert_eq!(
        evidence.final_message,
        Some("turn probe complete".to_string())
    );
    assert_eq!(
        evidence.approval_requests,
        vec!["item/commandExecution/requestApproval".to_string()]
    );
    assert!(
        evidence
            .notifications
            .contains(&"item/agentMessage/delta".to_string())
    );
    let log = fs::read_to_string(&evidence.log_path).expect("read jsonl log");
    assert!(log.contains(r#"turn/start"#));
    assert!(log.contains(r#"\"decision\":\"decline\""#));
    assert!(log.contains("summarize status"));
    assert!(log.contains("turn/completed"));

    let job_id = record_codex_app_server_turn_probe(&root, "summarize status", &evidence)
        .expect("record job");
    let status = render_codex_job_status(&root).expect("render job status");
    let result = render_codex_job_result(&root, Some(&job_id)).expect("render job result");
    assert!(status.contains(&job_id));
    assert!(status.contains("finished"));
    assert!(result.contains("thread_456"));
    assert!(result.contains("turn_456"));
    assert!(result.contains("resume: thread_456"));
    assert!(result.contains("message: turn probe complete"));
    assert!(result.contains("approvals: item/commandExecution/requestApproval"));
    assert!(result.contains("signals: mcp-tool-call, mcp-tool-completed, mcp-fs-write"));
}

#[test]
fn codex_app_server_signal_summary_reports_protocol_evidence() {
    let notifications = vec![
        "thread/started".to_string(),
        "item/commandExecution/outputDelta".to_string(),
        "item/fileChange/outputDelta".to_string(),
        "item/fileChange/patchUpdated".to_string(),
        "turn/diff/updated".to_string(),
        "fs/changed".to_string(),
        "item/mcpToolCall".to_string(),
        "item/mcpToolCall/completed".to_string(),
        "item/mcpToolCall/fs-write".to_string(),
        "error".to_string(),
    ];

    assert_eq!(
        codex_app_server_signal_summary(&notifications),
        "command-output, file-change, file-patch, diff-updated, fs-changed, mcp-tool-call, mcp-tool-completed, mcp-fs-write, app-server-error"
    );
    assert_eq!(codex_app_server_signal_summary(&[]), "none");
}

#[cfg(unix)]
#[test]
fn codex_app_server_job_records_async_status() {
    let _guard = subprocess_test_guard();
    let root = temp_root("codex_app_server_job");
    let script = root.join("mock-codex-job-app-server.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "if [ \"$1\" != \"app-server\" ]; then exit 2; fi",
            "read _init",
            "printf '%s\\n' '{\"id\":1,\"result\":{\"userAgent\":\"Codex Desktop/mock\",\"codexHome\":\"/tmp/codex-home\",\"platformFamily\":\"unix\",\"platformOs\":\"macos\"}}'",
            "read _thread",
            "printf '%s\\n' '{\"id\":2,\"result\":{\"thread\":{\"id\":\"thread_job\",\"sessionId\":\"thread_job\",\"turns\":[]},\"model\":\"gpt-test\"}}'",
            "printf '%s\\n' '{\"method\":\"thread/started\",\"params\":{\"thread\":{\"id\":\"thread_job\"}}}'",
            "read _turn",
            "printf '%s\\n' '{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_job\",\"items\":[],\"itemsView\":\"complete\",\"status\":\"inProgress\",\"error\":null,\"startedAt\":1,\"completedAt\":null,\"durationMs\":null}}}'",
            "printf '%s\\n' '{\"method\":\"item/completed\",\"params\":{\"threadId\":\"thread_job\",\"turnId\":\"turn_job\",\"item\":{\"type\":\"agentMessage\",\"text\":\"async job complete\"}}}'",
            "printf '%s\\n' '{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thread_job\",\"turn\":{\"id\":\"turn_job\",\"status\":\"completed\"}}}'",
            "sleep 1",
        ]
        .join("\n"),
    )
    .expect("write mock codex app-server job");
    make_executable(&script);

    let started = start_codex_app_server_job(
        &root,
        &script.to_string_lossy(),
        "summarize status".to_string(),
    )
    .expect("start app-server job");
    let id = started
        .lines()
        .find_map(|line| line.split('`').nth(1))
        .expect("job id in output")
        .to_string();

    wait_until(
        || {
            find_codex_job(&root, &id)
                .ok()
                .flatten()
                .is_some_and(|job| job.status == "finished")
        },
        Duration::from_secs(15),
    );

    let status = render_codex_job_status(&root).expect("render job status");
    let result = render_codex_job_result(&root, Some(&id)).expect("render job result");
    assert!(status.contains(&id));
    assert!(status.contains("finished"));
    assert!(result.contains("thread_job"));
    assert!(result.contains("turn_job"));
    assert!(result.contains("resume: thread_job"));
    assert!(result.contains("message: async job complete"));
    assert!(result.contains("signals: none"));
}

#[test]
fn acp_initialize_probe_records_jsonl_evidence() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_probe_ok");
    let script = root.join("mock-acp.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "read _line",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{\"loadSession\":true,\"promptCapabilities\":{\"image\":true}},\"agentInfo\":{\"name\":\"mock-acp\",\"version\":\"0.1.0\"},\"authMethods\":[{\"id\":\"api-key\",\"name\":\"API Key\"},{\"id\":\"browser\",\"name\":\"Browser Login\"}]}}'",
        ]
        .join("\n"),
    )
    .expect("write mock acp script");
    make_executable(&script);

    let evidence =
        run_acp_initialize_probe(&root, &script.to_string_lossy()).expect("probe succeeds");

    assert_eq!(evidence.protocol_version, "1");
    assert_eq!(evidence.agent_label, "mock-acp 0.1.0");
    assert_eq!(
        evidence.auth_methods,
        vec!["api-key (API Key)", "browser (Browser Login)"]
    );
    assert_eq!(evidence.auth_method_ids, vec!["api-key", "browser"]);
    assert!(evidence.capabilities.contains(&"loadSession".to_string()));
    assert!(
        evidence
            .capabilities
            .contains(&"promptCapabilities.image".to_string())
    );
    let log = fs::read_to_string(&evidence.log_path).expect("read jsonl log");
    assert!(log.contains(r#""method\":\"initialize"#));
    assert!(log.contains("mock-acp"));
}

#[test]
fn acp_auth_command_lists_methods_when_choice_required() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_auth_choose");
    let script = root.join("mock-acp-auth-choose.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "read _init",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{\"auth\":{\"logout\":{}}},\"agentInfo\":{\"name\":\"mock-auth\",\"version\":\"0.1.0\"},\"authMethods\":[{\"id\":\"api-key\",\"name\":\"API Key\"},{\"id\":\"browser\",\"name\":\"Browser Login\"}]}}'",
            "sleep 1",
        ]
        .join("\n"),
    )
    .expect("write mock acp auth choose script");
    make_executable(&script);
    let descriptor = mock_acp_descriptor("mock-acp-auth-choose", &script);

    let error = run_acp_authenticate_for_agent(&root, &descriptor, None)
        .expect_err("multiple methods require explicit choice");

    assert!(error.contains("choose an auth method"));
    assert!(error.contains("api-key (API Key)"));
    assert!(error.contains("browser (Browser Login)"));
}

#[test]
fn acp_auth_command_sends_authenticate_method() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_auth_method");
    let script = root.join("mock-acp-auth-method.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "read _init",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{\"auth\":{\"logout\":{}}},\"agentInfo\":{\"name\":\"mock-auth\",\"version\":\"0.1.0\"},\"authMethods\":[{\"id\":\"browser\",\"name\":\"Browser Login\"}]}}'",
            "read auth",
            "case \"$auth\" in *'\"method\":\"authenticate\"'*'\"methodId\":\"browser\"'*) ;; *) echo \"$auth\" >&2; exit 5 ;; esac",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"status\":\"ok\"}}'",
        ]
        .join("\n"),
    )
    .expect("write mock acp auth method script");
    make_executable(&script);
    let descriptor = mock_acp_descriptor("mock-acp-auth-method", &script);

    let evidence =
        run_acp_authenticate_for_agent(&root, &descriptor, Some("browser")).expect("auth succeeds");

    assert_eq!(evidence.method_id, "browser");
    assert_eq!(evidence.status, "ok");
    let log = fs::read_to_string(&evidence.log_path).expect("read auth log");
    assert!(log.contains("authenticate"));
    assert!(log.contains("browser"));
}

#[test]
fn acp_session_new_uses_mcp_server_array() {
    let request = acp_session_new_request(Path::new("/repo"));
    let value: Value = serde_json::from_str(&request).expect("valid session/new json");

    assert_eq!(
        value.get("method").and_then(Value::as_str),
        Some("session/new")
    );
    assert!(
        value
            .pointer("/params/mcpServers")
            .is_some_and(Value::is_array)
    );
}

#[test]
fn acp_run_args_parse_session_configuration() {
    let args = vec![
        "--async".to_string(),
        "--load-session".to_string(),
        "session_old".to_string(),
        "--mode".to_string(),
        "plan".to_string(),
        "--model".to_string(),
        "claude-sonnet".to_string(),
        "kiro-cli".to_string(),
        "continue".to_string(),
        "work".to_string(),
    ];

    let parsed = parse_acp_run_args(&args).expect("parse acp run args");

    assert!(parsed.async_job);
    assert_eq!(parsed.agent_id, "kiro-cli");
    assert_eq!(parsed.task, "continue work");
    assert_eq!(
        parsed.session.load_session_id.as_deref(),
        Some("session_old")
    );
    assert_eq!(parsed.session.mode_id.as_deref(), Some("plan"));
    assert_eq!(parsed.session.model_id.as_deref(), Some("claude-sonnet"));
}

#[test]
fn acp_session_load_uses_required_schema_fields() {
    let request = acp_session_load_request(Path::new("/repo"), "session_old");
    let value: Value = serde_json::from_str(&request).expect("valid session/load json");

    assert_eq!(
        value.get("method").and_then(Value::as_str),
        Some("session/load")
    );
    assert_eq!(
        value.pointer("/params/sessionId").and_then(Value::as_str),
        Some("session_old")
    );
    assert!(
        value
            .pointer("/params/mcpServers")
            .is_some_and(Value::is_array)
    );
}

#[test]
fn acp_session_configuration_requests_use_schema_shapes() {
    let set_mode = acp_session_set_mode_request("session_1", "plan", 2);
    let set_model = acp_session_set_model_request("session_1", "claude-sonnet", 3);
    let legacy_set_model = acp_legacy_session_set_model_request("session_1", "claude-sonnet", 4);
    let set_mode: Value = serde_json::from_str(&set_mode).expect("valid set_mode json");
    let set_model: Value = serde_json::from_str(&set_model).expect("valid set_model json");
    let legacy_set_model: Value =
        serde_json::from_str(&legacy_set_model).expect("valid legacy set_model json");

    assert_eq!(
        set_mode.get("method").and_then(Value::as_str),
        Some("session/set_mode")
    );
    assert_eq!(
        set_mode.pointer("/params/modeId").and_then(Value::as_str),
        Some("plan")
    );
    assert_eq!(
        set_model.get("method").and_then(Value::as_str),
        Some("session/set_config_option")
    );
    assert_eq!(
        set_model
            .pointer("/params/configId")
            .and_then(Value::as_str),
        Some("model")
    );
    assert_eq!(
        set_model.pointer("/params/value").and_then(Value::as_str),
        Some("claude-sonnet")
    );
    assert_eq!(
        legacy_set_model.get("method").and_then(Value::as_str),
        Some("session/set_model")
    );
    assert_eq!(
        legacy_set_model
            .pointer("/params/modelId")
            .and_then(Value::as_str),
        Some("claude-sonnet")
    );
}

#[test]
fn acp_session_prompt_uses_prompt_array() {
    let descriptor = mock_acp_descriptor("mock-codex-style", Path::new("mock"));
    let request = acp_session_prompt_request(&descriptor, "session_1", "hello", 2);
    let value: Value = serde_json::from_str(&request).expect("valid session/prompt json");

    assert_eq!(
        value.get("method").and_then(Value::as_str),
        Some("session/prompt")
    );
    assert!(value.pointer("/params/prompt").is_some_and(Value::is_array));
    assert!(value.pointer("/params/content").is_none());
}

#[test]
fn acp_response_reader_reports_closed_stdout_before_timeout() {
    let (_sender, receiver) = mpsc::channel::<std::io::Result<String>>();
    drop(_sender);
    let mut log_entries = Vec::new();

    let error = read_acp_response_line(&receiver, 1, &mut log_entries, Duration::from_millis(50))
        .expect_err("closed ACP stdout should not wait for timeout");

    assert!(error.contains("closed stdout before response id 1"));
}

#[test]
fn acp_kiro_session_prompt_uses_prompt_array() {
    let mut descriptor = mock_acp_descriptor("kiro-cli", Path::new("kiro-cli"));
    descriptor.source = AgentSource::LocalCommand;
    descriptor.command.command = "kiro-cli".to_string();
    descriptor.command.args = vec!["acp".to_string()];

    let request = acp_session_prompt_request(&descriptor, "session_1", "hello", 2);
    let value: Value = serde_json::from_str(&request).expect("valid Kiro session/prompt json");

    assert_eq!(
        value.get("method").and_then(Value::as_str),
        Some("session/prompt")
    );
    assert!(value.pointer("/params/prompt").is_some_and(Value::is_array));
    assert!(value.pointer("/params/content").is_none());
}

#[test]
fn acp_kiro_agent_env_adds_agent_selector_arg() {
    let _guard = subprocess_test_guard();
    let mut descriptor = mock_acp_descriptor("kiro-cli", Path::new("kiro-cli"));
    descriptor.source = AgentSource::LocalCommand;
    descriptor.command.command = "kiro-cli".to_string();
    descriptor.command.args = vec!["acp".to_string()];
    unsafe {
        env::set_var("VIDEN_KIRO_AGENT", "team-agent");
    }

    let args = acp_agent_command_args(&descriptor);

    unsafe {
        env::remove_var("VIDEN_KIRO_AGENT");
    }
    assert_eq!(args, vec!["acp", "--agent", "team-agent"]);
}

#[test]
fn acp_kiro_env_maps_official_acp_launch_options() {
    let _guard = subprocess_test_guard();
    let mut descriptor = mock_acp_descriptor("kiro-cli", Path::new("kiro-cli"));
    descriptor.source = AgentSource::LocalCommand;
    descriptor.command.command = "kiro-cli".to_string();
    descriptor.command.args = vec!["acp".to_string()];
    unsafe {
        env::set_var("VIDEN_KIRO_MODEL", "claude-sonnet-4");
        env::set_var("VIDEN_KIRO_EFFORT", "high");
        env::set_var(
            "VIDEN_KIRO_TRUST_TOOLS",
            "fs/read_text_file,terminal/create",
        );
        env::set_var("VIDEN_KIRO_AGENT_ENGINE", "v3");
    }

    let args = acp_agent_command_args(&descriptor);

    unsafe {
        env::remove_var("VIDEN_KIRO_MODEL");
        env::remove_var("VIDEN_KIRO_EFFORT");
        env::remove_var("VIDEN_KIRO_TRUST_TOOLS");
        env::remove_var("VIDEN_KIRO_AGENT_ENGINE");
    }
    assert_eq!(
        args,
        vec![
            "acp",
            "--model",
            "claude-sonnet-4",
            "--effort",
            "high",
            "--trust-tools",
            "fs/read_text_file,terminal/create",
            "--agent-engine",
            "v3"
        ]
    );
}

#[test]
fn acp_kiro_trust_all_tools_overrides_trust_tools() {
    let _guard = subprocess_test_guard();
    let mut descriptor = mock_acp_descriptor("kiro-cli", Path::new("kiro-cli"));
    descriptor.source = AgentSource::LocalCommand;
    descriptor.command.command = "kiro-cli".to_string();
    descriptor.command.args = vec!["acp".to_string()];
    unsafe {
        env::set_var("VIDEN_KIRO_TRUST_ALL_TOOLS", "true");
        env::set_var("VIDEN_KIRO_TRUST_TOOLS", "fs/read_text_file");
    }

    let args = acp_agent_command_args(&descriptor);

    unsafe {
        env::remove_var("VIDEN_KIRO_TRUST_ALL_TOOLS");
        env::remove_var("VIDEN_KIRO_TRUST_TOOLS");
    }
    assert_eq!(args, vec!["acp", "--trust-all-tools"]);
}

#[test]
fn acp_initialize_probe_uses_agent_descriptor_command_args() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_descriptor_probe_ok");
    let script = root.join("mock-acp-agent.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "if [ \"$1\" != \"acp\" ]; then exit 2; fi",
            "read _line",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{\"loadSession\":true,\"promptCapabilities\":{\"image\":true}},\"agentInfo\":{\"name\":\"mock-descriptor-acp\",\"version\":\"0.2.0\"}}}'",
        ]
        .join("\n"),
    )
    .expect("write mock acp agent script");
    make_executable(&script);
    let descriptor = AgentPluginDescriptor {
        agent_id: "mock-acp".to_string(),
        display_name: "Mock ACP".to_string(),
        version: "0.2.0".to_string(),
        transport: AgentTransport::Acp,
        source: AgentSource::LocalCommand,
        command: AgentCommandSpec {
            command: script.display().to_string(),
            args: vec!["acp".to_string()],
            env: vec![],
        },
        registry_package: None,
        protocol_versions: vec![AgentProtocolVersion::AcpV1],
        auth_modes: vec![AgentAuthMode::AgentNative],
        capabilities: vec![
            AgentPluginCapability::SessionPrompt,
            AgentPluginCapability::StreamingUpdates,
        ],
        permission_profile: AgentPermissionProfile::RuntimeGated,
        experimental_methods: vec![],
        config_schema_version: 1,
    };

    let evidence = run_acp_initialize_probe_for_agent(&root, &descriptor).expect("probe succeeds");

    assert_eq!(evidence.protocol_version, "1");
    assert_eq!(evidence.agent_label, "mock-descriptor-acp 0.2.0");
    let log = fs::read_to_string(&evidence.log_path).expect("read jsonl log");
    assert!(log.contains("mock-descriptor-acp"));

    let view = probe_typed_agent_adapter_descriptor(&root, &descriptor);
    assert_eq!(view.availability, AgentAvailability::Available);
    assert_eq!(view.auth_state, AgentAuthState::Ready);
    assert_eq!(view.startability, AgentStartability::Ready);
}

#[test]
fn acp_session_prompt_collects_streamed_updates() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_session_prompt");
    let script = root.join("mock-acp-session.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "read init",
            "case \"$init\" in *'\"method\":\"initialize\"'*) ;; *) exit 2 ;; esac",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{\"loadSession\":true},\"agentInfo\":{\"name\":\"mock-session-acp\",\"version\":\"0.3.0\"}}}'",
            "read new_session",
            "case \"$new_session\" in *'\"method\":\"session/new\"'*'\"cwd\"'*) ;; *) exit 3 ;; esac",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"sessionId\":\"session_123\"}}'",
            "read prompt",
            "case \"$prompt\" in *'\"method\":\"session/prompt\"'*'build a plan'*) ;; *) exit 4 ;; esac",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_123\",\"update\":{\"type\":\"AgentMessageChunk\",\"content\":\"Planning\"}}}'",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_123\",\"update\":{\"type\":\"ToolCall\",\"toolCallId\":\"tool_1\",\"title\":\"Read files\",\"status\":\"pending\"}}}'",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_123\",\"update\":{\"type\":\"ToolCallUpdate\",\"toolCallId\":\"tool_1\",\"status\":\"completed\",\"content\":\"README.md\"}}}'",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_123\",\"update\":{\"type\":\"TurnEnd\",\"status\":\"completed\"}}}'",
        ]
        .join("\n"),
    )
    .expect("write mock acp session script");
    make_executable(&script);
    let descriptor = AgentPluginDescriptor {
        agent_id: "mock-acp-session".to_string(),
        display_name: "Mock ACP Session".to_string(),
        version: "0.3.0".to_string(),
        transport: AgentTransport::Acp,
        source: AgentSource::LocalCommand,
        command: AgentCommandSpec {
            command: script.display().to_string(),
            args: vec![],
            env: vec![],
        },
        registry_package: None,
        protocol_versions: vec![AgentProtocolVersion::AcpV1],
        auth_modes: vec![AgentAuthMode::AgentNative],
        capabilities: vec![
            AgentPluginCapability::SessionPrompt,
            AgentPluginCapability::StreamingUpdates,
            AgentPluginCapability::ToolCalls,
        ],
        permission_profile: AgentPermissionProfile::RuntimeGated,
        experimental_methods: vec![],
        config_schema_version: 1,
    };

    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let evidence = run_acp_session_prompt_for_agent(
        &root,
        &descriptor,
        "build a plan",
        AcpSessionOptions::default(),
        &mut approver,
    )
    .unwrap();

    assert_eq!(evidence.session_id, "session_123");
    assert_eq!(evidence.final_status, "completed");
    assert_eq!(evidence.message, "Planning");
    assert_eq!(
        evidence.tool_calls,
        vec!["tool_1:pending:Read files", "tool_1:completed:README.md"]
    );
    let log = fs::read_to_string(&evidence.log_path).expect("read acp session log");
    assert!(log.contains("session/new"));
    assert!(log.contains("session/prompt"));
    assert!(log.contains("TurnEnd"));
}

#[test]
fn acp_session_prompt_can_load_and_configure_existing_session() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_session_load_configure");
    let script = root.join("mock-acp-load-configure.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "read _init",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{\"loadSession\":true,\"sessionCapabilities\":{\"setMode\":true}},\"agentInfo\":{\"name\":\"mock-load-configure\",\"version\":\"0.7.0\"}}}'",
            "read load",
            "case \"$load\" in *'\"method\":\"session/load\"'*'\"sessionId\":\"session_existing\"'*) ;; *) echo \"$load\" >&2; exit 5 ;; esac",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"sessionId\":\"session_existing\"}}'",
            "read mode",
            "case \"$mode\" in *'\"method\":\"session/set_mode\"'*'\"modeId\":\"plan\"'*) ;; *) echo \"$mode\" >&2; exit 6 ;; esac",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{}}'",
            "read model",
            "case \"$model\" in *'\"method\":\"session/set_config_option\"'*'\"configId\":\"model\"'*'\"value\":\"claude-sonnet\"'*) ;; *) echo \"$model\" >&2; exit 7 ;; esac",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"configOptions\":[]}}'",
            "read prompt",
            "case \"$prompt\" in *'\"method\":\"session/prompt\"'*) ;; *) echo \"$prompt\" >&2; exit 8 ;; esac",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_existing\",\"update\":{\"type\":\"AgentMessageChunk\",\"content\":\"configured\"}}}'",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":4,\"result\":{\"stopReason\":\"end_turn\"}}'",
        ]
        .join("\n"),
    )
    .expect("write mock acp load configure script");
    make_executable(&script);
    let descriptor = mock_acp_descriptor("mock-load-configure", &script);
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let session = AcpSessionOptions {
        load_session_id: Some("session_existing".to_string()),
        mode_id: Some("plan".to_string()),
        model_id: Some("claude-sonnet".to_string()),
    };

    let evidence =
        run_acp_session_prompt_for_agent(&root, &descriptor, "continue", session, &mut approver)
            .expect("configured acp session succeeds");

    assert_eq!(evidence.session_id, "session_existing");
    assert_eq!(evidence.final_status, "end_turn");
    assert_eq!(evidence.message, "configured");
    let log = fs::read_to_string(&evidence.log_path).expect("read acp session log");
    assert!(log.contains("session/load"));
    assert!(log.contains("session/set_mode"));
    assert!(log.contains("session/set_config_option"));
    assert!(log.contains("session/prompt"));
}

#[test]
fn acp_session_prompt_fails_when_set_mode_is_rejected() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_session_set_mode_error");
    let script = root.join("mock-acp-set-mode-error.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "read _init",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{\"sessionCapabilities\":{\"setMode\":true}},\"agentInfo\":{\"name\":\"mock-set-mode-error\",\"version\":\"0.7.0\"}}}'",
            "read _new",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"sessionId\":\"session_123\"}}'",
            "read mode",
            "case \"$mode\" in *'\"method\":\"session/set_mode\"'*'\"modeId\":\"plan\"'*) ;; *) echo \"$mode\" >&2; exit 5 ;; esac",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":2,\"error\":{\"code\":-32000,\"message\":\"mode unavailable\"}}'",
            "read maybe_prompt || exit 0",
            "echo \"unexpected prompt: $maybe_prompt\" >&2",
            "exit 9",
        ]
        .join("\n"),
    )
    .expect("write mock acp set-mode error script");
    make_executable(&script);
    let descriptor = mock_acp_descriptor("mock-set-mode-error", &script);
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let err = run_acp_session_prompt_for_agent(
        &root,
        &descriptor,
        "continue",
        AcpSessionOptions {
            load_session_id: None,
            mode_id: Some("plan".to_string()),
            model_id: None,
        },
        &mut approver,
    )
    .expect_err("set_mode errors should stop before prompting");

    assert!(err.contains("mode unavailable"), "{err}");
}

#[test]
fn acp_run_can_use_custom_command_descriptor_from_env() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_custom_command_run");
    let script = root.join("mock-custom-acp.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "read _init",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentInfo\":{\"name\":\"custom-acp\",\"version\":\"0.1.0\"}}}'",
            "read _new",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"sessionId\":\"session_custom\"}}'",
            "read prompt",
            "case \"$prompt\" in *'\"method\":\"session/prompt\"'*'hello custom'*) ;; *) echo \"$prompt\" >&2; exit 5 ;; esac",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_custom\",\"update\":{\"type\":\"AgentMessageChunk\",\"content\":\"custom ok\"}}}'",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_custom\",\"update\":{\"type\":\"TurnEnd\",\"status\":\"completed\"}}}'",
        ]
        .join("\n"),
    )
    .expect("write mock custom acp script");
    make_executable(&script);
    let descriptor = custom_acp_agent_descriptor(&script.display().to_string());
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let output = handle_acp_agent_run_command_with_agents(
        &root,
        &[descriptor],
        AcpRunArgs {
            async_job: false,
            agent_id: "custom-acp".to_string(),
            task: "hello custom".to_string(),
            session: AcpSessionOptions::default(),
        },
        &mut approver,
        PermissionContext::default(),
        None,
    )
    .expect("custom ACP descriptor should run");

    assert!(output.contains("agent: custom-acp"));
    assert!(output.contains("message: custom ok"));
}

#[test]
fn acp_session_prompt_accepts_codex_style_final_response() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_session_prompt_codex_style");
    let script = root.join("mock-acp-codex-style.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "read _init",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{\"loadSession\":true},\"agentInfo\":{\"name\":\"mock-codex-style\",\"version\":\"0.1.0\"}}}'",
            "read new_session",
            "case \"$new_session\" in *'\"mcpServers\":[]'*) ;; *) echo \"$new_session\" >&2; exit 3 ;; esac",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"sessionId\":\"session_codex\"}}'",
            "read prompt",
            "case \"$prompt\" in *'\"prompt\"'*'Reply'*) ;; *) echo \"$prompt\" >&2; exit 4 ;; esac",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_codex\",\"update\":{\"sessionUpdate\":\"agent_message_chunk\",\"content\":{\"type\":\"text\",\"text\":\"OK\"}}}}'",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"stopReason\":\"end_turn\",\"usage\":{\"totalTokens\":9,\"inputTokens\":7,\"outputTokens\":2}}}'",
        ]
        .join("\n"),
    )
    .expect("write mock codex-style acp script");
    make_executable(&script);
    let descriptor = mock_acp_descriptor("mock-acp-codex-style", &script);
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let evidence = run_acp_session_prompt_for_agent(
        &root,
        &descriptor,
        "Reply",
        AcpSessionOptions::default(),
        &mut approver,
    )
    .unwrap();

    assert_eq!(evidence.session_id, "session_codex");
    assert_eq!(evidence.final_status, "end_turn");
    assert_eq!(evidence.message, "OK");
    assert_eq!(
        evidence.usage_summary.as_deref(),
        Some("total=9 input=7 output=2")
    );
    assert_eq!(acp_session_job_status(&evidence), "finished");
}

#[test]
fn acp_session_prompt_accepts_kiro_notifications_and_tool_calls() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_session_prompt_kiro_style");
    let script = root.join("mock-acp-kiro-style.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "read _init",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{\"loadSession\":true,\"promptCapabilities\":{\"image\":true}},\"agentInfo\":{\"name\":\"kiro-cli\",\"version\":\"1.5.0\"}}}'",
            "read new_session",
            "case \"$new_session\" in *'\"mcpServers\":[]'*) ;; *) echo \"$new_session\" >&2; exit 3 ;; esac",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"sessionId\":\"session_kiro\"}}'",
            "read prompt",
            "case \"$prompt\" in *'\"prompt\"'*'Explain'*) ;; *) echo \"$prompt\" >&2; exit 4 ;; esac",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/notification\",\"params\":{\"sessionId\":\"session_kiro\",\"update\":{\"type\":\"AgentMessageChunk\",\"content\":\"Working\"}}}'",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/notification\",\"params\":{\"sessionId\":\"session_kiro\",\"update\":{\"type\":\"ToolCall\",\"toolCallId\":\"tool_1\",\"title\":\"Inspect project\",\"status\":\"pending\"}}}'",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/notification\",\"params\":{\"sessionId\":\"session_kiro\",\"update\":{\"type\":\"ToolCallUpdate\",\"toolCallId\":\"tool_1\",\"status\":\"completed\",\"content\":\"Cargo.toml\"}}}'",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/notification\",\"params\":{\"sessionId\":\"session_kiro\",\"update\":{\"type\":\"TurnEnd\",\"status\":\"completed\"}}}'",
        ]
        .join("\n"),
    )
    .expect("write mock kiro-style acp script");
    make_executable(&script);
    let mut descriptor = mock_acp_descriptor("kiro-cli", &script);
    descriptor.source = AgentSource::LocalCommand;
    descriptor.command.args = vec![];
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let evidence = run_acp_session_prompt_for_agent(
        &root,
        &descriptor,
        "Explain",
        AcpSessionOptions::default(),
        &mut approver,
    )
    .unwrap();

    assert_eq!(evidence.session_id, "session_kiro");
    assert_eq!(evidence.final_status, "completed");
    assert_eq!(evidence.message, "Working");
    assert_eq!(
        evidence.tool_calls,
        vec![
            "tool_1:pending:Inspect project",
            "tool_1:completed:Cargo.toml"
        ]
    );
    assert!(evidence.runtime_events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::AssistantDelta { content, .. } if content == "Working"
    )));
    assert!(evidence.runtime_events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::ToolCallStarted { tool_call_id, name, .. }
            if tool_call_id == "tool_1" && name == "Inspect project"
    )));
    assert!(evidence.runtime_events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::ToolCallFinished { tool_call_id, success, evidence, .. }
            if tool_call_id == "tool_1" && *success && evidence.is_some()
    )));
    assert!(evidence.runtime_events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::EvidenceRecorded { evidence }
            if evidence.kind == "tool_log" && evidence.summary.contains("Cargo.toml")
    )));
    assert!(evidence.runtime_events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::EvidenceRecorded { evidence }
            if evidence.kind == "acp_turn_end" && evidence.summary.contains("completed")
    )));
    assert!(evidence.runtime_events.iter().any(|event| matches!(
        &event.kind,
            RuntimeEventKind::MergeGateUpdated { gate }
            if gate.gate_id == "gate-acp-session-session_kiro"
                && gate.status == MergeGateStatus::CollectingEvidence
                && gate.decision.as_deref() == Some("missing_canonical")
                && gate.required_evidence == vec!["acp_turn_end".to_string()]
                && gate.evidence_ids.iter().any(|id| id.starts_with("acp-tool-tool_1"))
                && gate.evidence_ids.iter().any(|id| id.starts_with("acp-turn-end-session_kiro"))
    )));
}

#[test]
fn acp_session_prompt_maps_diff_updates_to_patch_evidence() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_session_prompt_patch_evidence");
    let script = root.join("mock-acp-patch-evidence.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "read _init",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{\"loadSession\":true},\"agentInfo\":{\"name\":\"mock-acp-patch\",\"version\":\"0.1.0\"}}}'",
            "read _new_session",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"sessionId\":\"session_patch\"}}'",
            "read _prompt",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_patch\",\"update\":{\"type\":\"ToolCallUpdate\",\"toolCallId\":\"tool_patch\",\"status\":\"completed\",\"content\":\"generated patch\",\"diff\":\"diff --git a/src/lib.rs b/src/lib.rs\\n--- a/src/lib.rs\\n+++ b/src/lib.rs\\n@@ -1 +1 @@\\n-old\\n+new\\n\"}}}'",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_patch\",\"update\":{\"type\":\"TurnEnd\",\"status\":\"completed\"}}}'",
        ]
        .join("\n"),
    )
    .expect("write mock acp patch script");
    make_executable(&script);
    let descriptor = mock_acp_descriptor("mock-acp-patch", &script);
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let evidence = run_acp_session_prompt_for_agent(
        &root,
        &descriptor,
        "Generate a patch",
        AcpSessionOptions::default(),
        &mut approver,
    )
    .unwrap();

    let patch_id = evidence
        .runtime_events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::EvidenceRecorded { evidence } if evidence.kind == "patch" => {
                let metadata = evidence.metadata.as_ref()?;
                assert_eq!(
                    metadata.get("schema").and_then(Value::as_str),
                    Some("acp.patch.v1")
                );
                assert_eq!(
                    metadata.get("format").and_then(Value::as_str),
                    Some("unified_diff")
                );
                assert_eq!(metadata.get("fileCount").and_then(Value::as_u64), Some(1));
                assert_eq!(metadata.get("additions").and_then(Value::as_u64), Some(1));
                assert_eq!(metadata.get("deletions").and_then(Value::as_u64), Some(1));
                assert_eq!(
                    metadata.pointer("/files/0/path").and_then(Value::as_str),
                    Some("src/lib.rs")
                );
                assert_eq!(
                    metadata
                        .pointer("/origin/toolCallId")
                        .and_then(Value::as_str),
                    Some("tool_patch")
                );
                assert!(
                    metadata
                        .get("diff")
                        .and_then(Value::as_str)
                        .is_some_and(|diff| diff.contains("diff --git a/src/lib.rs b/src/lib.rs"))
                );
                assert_eq!(evidence.source.as_deref(), Some("acp:patch.v1"));
                assert!(evidence.summary.contains("ACP patch: 1 file(s), +1/-1"));
                Some(evidence.id.clone())
            }
            _ => None,
        });
    let patch_id = patch_id.expect("ACP diff update should record patch evidence");
    assert!(evidence.runtime_events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MergeGateUpdated { gate }
            if gate.gate_id == "gate-acp-session-session_patch"
                && gate.required_evidence == vec![
                    "patch".to_string(),
                    "acp_turn_end".to_string(),
                ]
                && gate.evidence_ids.iter().any(|id| id == &patch_id)
                && gate.status == MergeGateStatus::CollectingEvidence
                && gate.decision.as_deref() == Some("missing_canonical")
    )));
}

#[test]
fn acp_smoke_gate_reports_pass_and_blocked_auth() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_smoke_gate");
    let ok = root.join("mock-acp-ok.sh");
    fs::write(
        &ok,
        [
            "#!/bin/sh",
            "read _init",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{},\"agentInfo\":{\"name\":\"mock-ok\",\"version\":\"0.1.0\"},\"authMethods\":[]}}'",
        ]
        .join("\n"),
    )
    .expect("write ok smoke script");
    make_executable(&ok);
    let blocked = root.join("mock-acp-blocked.sh");
    fs::write(
        &blocked,
        [
            "#!/bin/sh",
            "echo 'error: You are not logged in, please log in' >&2",
            "exit 3",
        ]
        .join("\n"),
    )
    .expect("write blocked smoke script");
    make_executable(&blocked);
    let agents = vec![
        mock_acp_descriptor("mock-ok", &ok),
        mock_acp_descriptor("mock-blocked", &blocked),
    ];
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let report = run_acp_smoke_gate_for_agents(&root, &agents, false, &mut approver).unwrap_err();

    assert!(report.contains("PASS mock-ok"));
    assert!(report.contains("BLOCKED mock-blocked"));
    assert!(report.contains("summary: 0 failed, 1 blocked-auth"));
}

#[test]
fn acp_smoke_gate_classifies_timeout_as_failure_not_auth_block() {
    let descriptor = mock_acp_descriptor("kiro-cli", Path::new("kiro-cli"));

    assert_eq!(
        classify_acp_smoke_error(
            &descriptor,
            "ACP session/prompt timed out before TurnEnd or final response after 120s",
        ),
        "timeout"
    );
    assert_eq!(
        classify_acp_smoke_error(&descriptor, "You are not logged in, please log in"),
        "blocked-auth"
    );
}

#[test]
fn acp_session_prompt_timeout_is_agent_aware_and_env_overridable() {
    let _guard = subprocess_test_guard();
    let mut kiro = mock_acp_descriptor("kiro-cli", Path::new("kiro-cli"));
    kiro.command.command = "kiro-cli".to_string();
    let codex = mock_acp_descriptor("codex-acp", Path::new("npx"));

    unsafe {
        env::remove_var("VIDEN_ACP_SESSION_TIMEOUT_SECS");
    }
    assert_eq!(
        acp_session_prompt_timeout(&kiro),
        Duration::from_secs(DEFAULT_KIRO_ACP_SESSION_TIMEOUT_SECS)
    );
    assert_eq!(
        acp_session_prompt_timeout(&codex),
        Duration::from_secs(DEFAULT_LOCAL_ACP_SESSION_TIMEOUT_SECS)
    );

    unsafe {
        env::set_var("VIDEN_ACP_SESSION_TIMEOUT_SECS", "7");
    }
    assert_eq!(acp_session_prompt_timeout(&kiro), Duration::from_secs(7));
    unsafe {
        env::remove_var("VIDEN_ACP_SESSION_TIMEOUT_SECS");
    }
}

#[test]
fn acp_session_permission_request_routes_through_approver() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_session_permission");
    let script = root.join("mock-acp-permission.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "read _init",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{},\"agentInfo\":{\"name\":\"mock-permission-acp\",\"version\":\"0.4.0\"}}}'",
            "read _new_session",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"sessionId\":\"session_perm\"}}'",
            "read _prompt",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"session/request_permission\",\"params\":{\"sessionId\":\"session_perm\",\"toolCall\":{\"toolCallId\":\"tool_2\",\"title\":\"Edit file\",\"kind\":\"edit\"},\"options\":[{\"optionId\":\"deny\",\"kind\":\"reject_once\",\"name\":\"Deny\"},{\"optionId\":\"allow\",\"kind\":\"allow_once\",\"name\":\"Allow\"}]}}'",
            "read approval",
            "case \"$approval\" in *'\"id\":9'*'\"optionId\":\"allow\"'*) ;; *) exit 5 ;; esac",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_perm\",\"update\":{\"type\":\"ToolCallUpdate\",\"toolCallId\":\"tool_2\",\"status\":\"completed\",\"content\":\"approved\"}}}'",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_perm\",\"update\":{\"type\":\"TurnEnd\",\"status\":\"completed\"}}}'",
        ]
        .join("\n"),
    )
    .expect("write mock acp permission script");
    make_executable(&script);
    let descriptor = AgentPluginDescriptor {
        agent_id: "mock-acp-permission".to_string(),
        display_name: "Mock ACP Permission".to_string(),
        version: "0.4.0".to_string(),
        transport: AgentTransport::Acp,
        source: AgentSource::LocalCommand,
        command: AgentCommandSpec {
            command: script.display().to_string(),
            args: vec![],
            env: vec![],
        },
        registry_package: None,
        protocol_versions: vec![AgentProtocolVersion::AcpV1],
        auth_modes: vec![AgentAuthMode::AgentNative],
        capabilities: vec![
            AgentPluginCapability::SessionPrompt,
            AgentPluginCapability::StreamingUpdates,
            AgentPluginCapability::ToolCalls,
        ],
        permission_profile: AgentPermissionProfile::RuntimeGated,
        experimental_methods: vec![],
        config_schema_version: 1,
    };
    let approvals = Cell::new(0usize);
    let prompts = RefCell::new(Vec::new());
    let mut approver = |prompt: viden_types::PermissionPrompt| {
        approvals.set(approvals.get() + 1);
        prompts.borrow_mut().push(prompt);
        ApprovalResponse::allow_once(Some("ok".to_string()))
    };

    let evidence = run_acp_session_prompt_for_agent(
        &root,
        &descriptor,
        "edit the file",
        AcpSessionOptions::default(),
        &mut approver,
    )
    .unwrap();

    assert_eq!(approvals.get(), 1);
    assert_eq!(prompts.borrow()[0].tool_name, "acp:tool_2");
    assert!(prompts.borrow()[0].message.contains("Edit file"));
    assert_eq!(evidence.tool_calls, vec!["tool_2:completed:approved"]);
    let log = fs::read_to_string(&evidence.log_path).expect("read acp session log");
    assert!(log.contains("session/request_permission"));
    assert!(log.contains(r#"optionId\":\"allow"#));
}

#[test]
fn first_party_acp_permission_and_tool_update_fixtures_project_consistently() {
    let fixtures = [
        include_str!("../tests/fixtures/acp-v1/claude-acp.json"),
        include_str!("../tests/fixtures/acp-v1/codex-acp.json"),
        include_str!("../tests/fixtures/acp-v1/kiro-acp.json"),
    ];
    let known = builtin_agent_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.agent_id)
        .collect::<HashSet<_>>();

    for raw in fixtures {
        let fixture: Value = serde_json::from_str(raw).expect("valid ACP fixture");
        let agent_id = fixture["agent_id"].as_str().expect("fixture agent id");
        assert!(
            known.contains(agent_id),
            "unknown fixture adapter {agent_id}"
        );
        let permission = &fixture["permission_request"];
        let prompt = acp_permission_prompt(permission);
        assert!(prompt.tool_name.starts_with("acp:"), "{agent_id}");
        assert!(!prompt.message.trim().is_empty(), "{agent_id}");
        let allow: Value = serde_json::from_str(&acp_permission_response(permission, true))
            .expect("allow response");
        let deny: Value = serde_json::from_str(&acp_permission_response(permission, false))
            .expect("deny response");
        assert_ne!(
            allow.pointer("/result/outcome/optionId"),
            deny.pointer("/result/outcome/optionId"),
            "{agent_id} must map distinct allow and deny options"
        );

        let mut events = Vec::new();
        let mut sequence = 1;
        let mut evidence_ids = Vec::new();
        append_acp_update_runtime_events(
            &mut events,
            &mut sequence,
            agent_id,
            &format!("acp-message-{agent_id}-turn-1"),
            &mut evidence_ids,
            None,
            None,
            &fixture["tool_update"],
        );
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::ToolCallFinished { success: true, .. }
        )));
        assert!(
            events
                .iter()
                .any(|event| matches!(event.kind, RuntimeEventKind::EvidenceRecorded { .. }))
        );
    }
}

#[test]
fn acp_session_permission_denial_selects_reject_option() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_session_permission_denied");
    let script = root.join("mock-acp-permission-denied.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "read _init",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{},\"agentInfo\":{\"name\":\"mock-deny-acp\",\"version\":\"0.4.0\"}}}'",
            "read _new_session",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"sessionId\":\"session_deny\"}}'",
            "read _prompt",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":10,\"method\":\"session/request_permission\",\"params\":{\"sessionId\":\"session_deny\",\"toolCall\":{\"toolCallId\":\"tool_3\",\"title\":\"Run command\",\"kind\":\"terminal\"},\"options\":[{\"optionId\":\"approve\",\"kind\":\"allow_once\",\"name\":\"Allow\"},{\"optionId\":\"reject\",\"kind\":\"reject_once\",\"name\":\"Reject\"}]}}'",
            "read approval",
            "case \"$approval\" in *'\"id\":10'*'\"optionId\":\"reject\"'*) ;; *) exit 5 ;; esac",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_deny\",\"update\":{\"type\":\"TurnEnd\",\"status\":\"completed\"}}}'",
        ]
        .join("\n"),
    )
    .expect("write mock acp permission denied script");
    make_executable(&script);
    let descriptor = AgentPluginDescriptor {
        agent_id: "mock-acp-deny".to_string(),
        display_name: "Mock ACP Deny".to_string(),
        version: "0.4.0".to_string(),
        transport: AgentTransport::Acp,
        source: AgentSource::LocalCommand,
        command: AgentCommandSpec {
            command: script.display().to_string(),
            args: vec![],
            env: vec![],
        },
        registry_package: None,
        protocol_versions: vec![AgentProtocolVersion::AcpV1],
        auth_modes: vec![AgentAuthMode::AgentNative],
        capabilities: vec![AgentPluginCapability::SessionPrompt],
        permission_profile: AgentPermissionProfile::RuntimeGated,
        experimental_methods: vec![],
        config_schema_version: 1,
    };
    let mut approver =
        |_prompt: viden_types::PermissionPrompt| ApprovalResponse::deny(Some("no".to_string()));

    let evidence = run_acp_session_prompt_for_agent(
        &root,
        &descriptor,
        "run a command",
        AcpSessionOptions::default(),
        &mut approver,
    )
    .unwrap();

    assert_eq!(evidence.final_status, "completed");
    let log = fs::read_to_string(&evidence.log_path).expect("read acp session log");
    assert!(log.contains(r#"optionId\":\"reject"#));
}

#[test]
fn acp_session_handles_permission_gated_file_read_requests() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_session_file_request_read");
    let target = root.join("notes.txt");
    fs::write(&target, "hello from acp\nsecond line\n").expect("write target file");
    let script = root.join("mock-acp-file-request.sh");
    fs::write(
        &script,
        vec![
            "#!/bin/sh".to_string(),
            "read _init".to_string(),
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{},\"agentInfo\":{\"name\":\"mock-file-acp\",\"version\":\"0.5.0\"}}}'".to_string(),
            "read _new_session".to_string(),
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"sessionId\":\"session_file\"}}'".to_string(),
            "read _prompt".to_string(),
            format!(
                "printf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"id\":17,\"method\":\"fs/read_text_file\",\"params\":{{\"path\":\"{}\",\"startLine\":1,\"limit\":1}}}}'",
                target.display()
            ),
            "read file_response".to_string(),
            "case \"$file_response\" in".to_string(),
            "  *'\"content\":\"hello from acp\"'*) ;;".to_string(),
            "  *) echo \"$file_response\" >&2; exit 3 ;;".to_string(),
            "esac".to_string(),
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_file\",\"update\":{\"type\":\"TurnEnd\",\"status\":\"completed\"}}}'".to_string(),
        ]
        .join("\n"),
    )
    .expect("write mock acp file request script");
    make_executable(&script);
    let descriptor = mock_acp_descriptor("mock-acp-file-request", &script);
    let mut approver = |_prompt: viden_types::PermissionPrompt| ApprovalResponse::allow_once(None);

    let evidence = run_acp_session_prompt_for_agent(
        &root,
        &descriptor,
        "read a file",
        AcpSessionOptions::default(),
        &mut approver,
    )
    .expect("session should continue after file read request");

    assert_eq!(evidence.final_status, "completed");
    let log = fs::read_to_string(&evidence.log_path).expect("read acp session log");
    assert!(log.contains("fs/read_text_file"));
    assert!(log.contains("hello from acp"));
}

#[test]
fn acp_session_file_write_requests_require_approval() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_session_file_request_write");
    let target = root.join("written.txt");
    let script = root.join("mock-acp-file-write.sh");
    fs::write(
        &script,
        vec![
            "#!/bin/sh".to_string(),
            "read _init".to_string(),
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{},\"agentInfo\":{\"name\":\"mock-file-write-acp\",\"version\":\"0.5.0\"}}}'".to_string(),
            "read _new_session".to_string(),
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"sessionId\":\"session_file_write\"}}'".to_string(),
            "read _prompt".to_string(),
            format!(
                "printf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"id\":18,\"method\":\"fs/write_text_file\",\"params\":{{\"path\":\"{}\",\"content\":\"written by acp\"}}}}'",
                target.display()
            ),
            "read file_response".to_string(),
            "case \"$file_response\" in".to_string(),
            "  *'\"result\":{}'*) ;;".to_string(),
            "  *) echo \"$file_response\" >&2; exit 3 ;;".to_string(),
            "esac".to_string(),
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_file_write\",\"update\":{\"type\":\"TurnEnd\",\"status\":\"completed\"}}}'".to_string(),
        ]
        .join("\n"),
    )
    .expect("write mock acp file write script");
    make_executable(&script);
    let descriptor = mock_acp_descriptor("mock-acp-file-write", &script);
    let approvals = Cell::new(0usize);
    let mut approver = |prompt: viden_types::PermissionPrompt| {
        approvals.set(approvals.get() + 1);
        assert_eq!(prompt.tool_name, "write_file");
        assert!(prompt.input_preview.contains("written.txt"));
        ApprovalResponse::allow_once(None)
    };

    let evidence = run_acp_session_prompt_for_agent(
        &root,
        &descriptor,
        "write a file",
        AcpSessionOptions::default(),
        &mut approver,
    )
    .expect("session should continue after approved file write request");

    assert_eq!(approvals.get(), 1);
    assert_eq!(evidence.final_status, "completed");
    assert_eq!(fs::read_to_string(&target).unwrap(), "written by acp");
}

#[test]
fn acp_filesystem_bridge_denies_out_of_scope_reads() {
    let root = temp_root("acp_file_out_of_scope");
    let engine = PermissionEngine::new(&root);
    let response = acp_read_text_file_response(
        &root,
        &engine,
        &local_fs_capability(),
        &json!({
            "jsonrpc": "2.0",
            "id": 19,
            "method": "fs/read_text_file",
            "params": {"path": "/tmp/outside-viden.txt"}
        }),
    );

    assert!(response.contains(r#""id":19"#));
    assert!(response.contains("Path is outside the allowed working directory scope"));
}

#[test]
fn acp_session_terminal_requests_run_through_permission_bridge() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_session_terminal_request");
    let script = root.join("mock-acp-terminal.sh");
    fs::write(
        &script,
        vec![
            "#!/bin/sh".to_string(),
            "read _init".to_string(),
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{},\"agentInfo\":{\"name\":\"mock-terminal-acp\",\"version\":\"0.6.0\"}}}'".to_string(),
            "read _new_session".to_string(),
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"sessionId\":\"session_terminal\"}}'".to_string(),
            "read _prompt".to_string(),
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":42,\"method\":\"terminal/create\",\"params\":{\"sessionId\":\"session_terminal\",\"command\":\"printf\",\"args\":[\"terminal-ok\"]}}'".to_string(),
            "read create_response".to_string(),
            "case \"$create_response\" in".to_string(),
            "  *'\"terminalId\":\"acp-terminal-1\"'*) ;;".to_string(),
            "  *) echo \"$create_response\" >&2; exit 3 ;;".to_string(),
            "esac".to_string(),
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":43,\"method\":\"terminal/output\",\"params\":{\"sessionId\":\"session_terminal\",\"terminalId\":\"acp-terminal-1\"}}'".to_string(),
            "read output_response".to_string(),
            "case \"$output_response\" in".to_string(),
            "  *'terminal-ok'*) ;;".to_string(),
            "  *) echo \"$output_response\" >&2; exit 4 ;;".to_string(),
            "esac".to_string(),
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":44,\"method\":\"terminal/wait_for_exit\",\"params\":{\"sessionId\":\"session_terminal\",\"terminalId\":\"acp-terminal-1\"}}'".to_string(),
            "read wait_response".to_string(),
            "case \"$wait_response\" in".to_string(),
            "  *'\"exitCode\":0'*) ;;".to_string(),
            "  *) echo \"$wait_response\" >&2; exit 5 ;;".to_string(),
            "esac".to_string(),
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":45,\"method\":\"terminal/release\",\"params\":{\"sessionId\":\"session_terminal\",\"terminalId\":\"acp-terminal-1\"}}'".to_string(),
            "read release_response".to_string(),
            "case \"$release_response\" in".to_string(),
            "  *'\"result\":{}'*) ;;".to_string(),
            "  *) echo \"$release_response\" >&2; exit 6 ;;".to_string(),
            "esac".to_string(),
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_terminal\",\"update\":{\"type\":\"TurnEnd\",\"status\":\"completed\"}}}'".to_string(),
        ]
        .join("\n"),
    )
    .expect("write mock acp terminal script");
    make_executable(&script);
    let descriptor = mock_acp_descriptor("mock-acp-terminal", &script);
    let approvals = Cell::new(0usize);
    let mut approver = |prompt: viden_types::PermissionPrompt| {
        approvals.set(approvals.get() + 1);
        assert_eq!(prompt.tool_name, "shell");
        assert!(prompt.input_preview.contains("printf terminal-ok"));
        ApprovalResponse::allow_once(None)
    };

    let evidence = run_acp_session_prompt_for_agent(
        &root,
        &descriptor,
        "run a terminal command",
        AcpSessionOptions::default(),
        &mut approver,
    )
    .expect("session should continue after terminal requests");

    assert_eq!(approvals.get(), 1);
    assert_eq!(evidence.final_status, "completed");
    let log = fs::read_to_string(&evidence.log_path).expect("read acp session log");
    assert!(log.contains("terminal/create"));
    assert!(log.contains("terminal-ok"));
}

#[test]
fn acp_terminal_bridge_supports_long_running_output_polling() {
    let root = temp_root("acp_terminal_long_running");
    let mut engine = PermissionEngine::new(&root);
    let mut terminals = AcpTerminalStore::default();
    let mut approver = |_prompt: viden_types::PermissionPrompt| ApprovalResponse::allow_once(None);

    let started = Instant::now();
    let create = acp_terminal_create_response(
        &root,
        &mut engine,
        &mut approver,
        &local_process_capability(),
        &mut terminals,
        &json!({
            "jsonrpc": "2.0",
            "id": 47,
            "method": "terminal/create",
            "params": {
                "sessionId": "session_terminal",
                "command": "sh",
                "args": ["-c", "printf started; sleep 1; printf done"]
            }
        }),
    );
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "terminal/create should return before the command exits"
    );
    assert!(create.contains(r#""terminalId":"acp-terminal-1""#));

    std::thread::sleep(Duration::from_millis(100));
    let output = acp_terminal_output_response(
        &mut terminals,
        &json!({
            "jsonrpc": "2.0",
            "id": 48,
            "method": "terminal/output",
            "params": {
                "sessionId": "session_terminal",
                "terminalId": "acp-terminal-1"
            }
        }),
    );
    assert!(output.contains("started"));
    assert!(output.contains(r#""exitCode":null"#));

    let wait = acp_terminal_wait_for_exit_response(
        &mut terminals,
        &json!({
            "jsonrpc": "2.0",
            "id": 49,
            "method": "terminal/wait_for_exit",
            "params": {
                "sessionId": "session_terminal",
                "terminalId": "acp-terminal-1"
            }
        }),
    );
    assert!(wait.contains(r#""exitCode":0"#));

    let final_output = acp_terminal_output_response(
        &mut terminals,
        &json!({
            "jsonrpc": "2.0",
            "id": 50,
            "method": "terminal/output",
            "params": {
                "sessionId": "session_terminal",
                "terminalId": "acp-terminal-1"
            }
        }),
    );
    assert!(final_output.contains("started"));
    assert!(final_output.contains("done"));
}

#[test]
fn acp_terminal_bridge_can_kill_long_running_processes() {
    let root = temp_root("acp_terminal_kill_long_running");
    let mut engine = PermissionEngine::new(&root);
    let mut terminals = AcpTerminalStore::default();
    let mut approver = |_prompt: viden_types::PermissionPrompt| ApprovalResponse::allow_once(None);

    let create = acp_terminal_create_response(
        &root,
        &mut engine,
        &mut approver,
        &local_process_capability(),
        &mut terminals,
        &json!({
            "jsonrpc": "2.0",
            "id": 51,
            "method": "terminal/create",
            "params": {
                "sessionId": "session_terminal",
                "command": "sh",
                "args": ["-c", "printf started; sleep 5; printf never"]
            }
        }),
    );
    assert!(create.contains(r#""terminalId":"acp-terminal-1""#));

    std::thread::sleep(Duration::from_millis(100));
    let kill = acp_terminal_kill_response(
        &mut terminals,
        &json!({
            "jsonrpc": "2.0",
            "id": 52,
            "method": "terminal/kill",
            "params": {
                "sessionId": "session_terminal",
                "terminalId": "acp-terminal-1"
            }
        }),
    );
    assert!(kill.contains(r#""result":{}"#));

    let wait = acp_terminal_wait_for_exit_response(
        &mut terminals,
        &json!({
            "jsonrpc": "2.0",
            "id": 53,
            "method": "terminal/wait_for_exit",
            "params": {
                "sessionId": "session_terminal",
                "terminalId": "acp-terminal-1"
            }
        }),
    );
    assert!(wait.contains(r#""signal":"killed""#));

    let output = acp_terminal_output_response(
        &mut terminals,
        &json!({
            "jsonrpc": "2.0",
            "id": 54,
            "method": "terminal/output",
            "params": {
                "sessionId": "session_terminal",
                "terminalId": "acp-terminal-1"
            }
        }),
    );
    assert!(output.contains("started"));
    assert!(!output.contains("never"));
}

#[test]
fn acp_terminal_bridge_supports_stdin_input() {
    let root = temp_root("acp_terminal_stdin_input");
    let mut engine = PermissionEngine::new(&root);
    let mut terminals = AcpTerminalStore::default();
    let mut approver = |_prompt: viden_types::PermissionPrompt| ApprovalResponse::allow_once(None);

    let create = acp_terminal_create_response(
        &root,
        &mut engine,
        &mut approver,
        &local_process_capability(),
        &mut terminals,
        &json!({
            "jsonrpc": "2.0",
            "id": 55,
            "method": "terminal/create",
            "params": {
                "sessionId": "session_terminal",
                "command": "sh",
                "args": ["-c", "read line; printf 'got:%s' \"$line\""]
            }
        }),
    );
    assert!(create.contains(r#""terminalId":"acp-terminal-1""#));

    let input = acp_terminal_input_response(
        &mut terminals,
        &json!({
            "jsonrpc": "2.0",
            "id": 56,
            "method": "terminal/input",
            "params": {
                "sessionId": "session_terminal",
                "terminalId": "acp-terminal-1",
                "input": "hello\n"
            }
        }),
    );
    assert!(input.contains(r#""bytesWritten":6"#));

    let wait = acp_terminal_wait_for_exit_response(
        &mut terminals,
        &json!({
            "jsonrpc": "2.0",
            "id": 57,
            "method": "terminal/wait_for_exit",
            "params": {
                "sessionId": "session_terminal",
                "terminalId": "acp-terminal-1"
            }
        }),
    );
    assert!(wait.contains(r#""exitCode":0"#));

    let output = acp_terminal_output_response(
        &mut terminals,
        &json!({
            "jsonrpc": "2.0",
            "id": 58,
            "method": "terminal/output",
            "params": {
                "sessionId": "session_terminal",
                "terminalId": "acp-terminal-1"
            }
        }),
    );
    assert!(output.contains("got:hello"));
}

#[test]
fn acp_terminal_bridge_respects_plan_mode() {
    let root = temp_root("acp_terminal_plan_mode");
    let mut engine = PermissionEngine::new(&root);
    let context = PermissionContext {
        mode: viden_types::PermissionMode::Plan,
        ..Default::default()
    };
    engine.restore_context(context);
    let mut terminals = AcpTerminalStore::default();
    let approvals = Cell::new(0usize);
    let mut approver = |_prompt: viden_types::PermissionPrompt| {
        approvals.set(approvals.get() + 1);
        ApprovalResponse::allow_once(None)
    };

    let response = acp_terminal_create_response(
        &root,
        &mut engine,
        &mut approver,
        &local_process_capability(),
        &mut terminals,
        &json!({
            "jsonrpc": "2.0",
            "id": 46,
            "method": "terminal/create",
            "params": {
                "sessionId": "session_terminal",
                "command": "printf",
                "args": ["blocked"]
            }
        }),
    );

    assert_eq!(approvals.get(), 0);
    assert!(terminals.records.is_empty());
    assert!(response.contains(r#""id":46"#));
    assert!(response.contains("blocked while plan mode is active"));
}

#[test]
fn acp_agent_handshake_timeout_allows_registry_cold_start() {
    let mut registry = mock_acp_descriptor("mock-registry-acp", Path::new("npx"));
    registry.source = AgentSource::Registry;
    let mut local = mock_acp_descriptor("mock-local-acp", Path::new("kiro-cli"));
    local.source = AgentSource::LocalCommand;

    assert_eq!(
        acp_agent_handshake_timeout(&registry),
        Duration::from_secs(DEFAULT_REGISTRY_ACP_HANDSHAKE_TIMEOUT_SECS)
    );
    assert_eq!(
        acp_agent_handshake_timeout(&local),
        Duration::from_secs(DEFAULT_LOCAL_ACP_HANDSHAKE_TIMEOUT_SECS)
    );
}

#[test]
fn acp_registry_agent_uses_version_scoped_npm_cache() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_registry_npm_cache");
    let script = root.join("mock-registry-acp.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "case \"$npm_config_cache\" in",
            "  */.viden/cache/npm/mock-registry-cache/test) ;;",
            "  *) echo \"unexpected npm_config_cache=$npm_config_cache\" >&2; exit 7 ;;",
            "esac",
            "test \"$NPM_CONFIG_CACHE\" = \"$npm_config_cache\" || exit 8",
            "test \"$npm_config_audit\" = \"false\" || exit 9",
            "test \"$npm_config_fund\" = \"false\" || exit 10",
            "read _line",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{},\"agentInfo\":{\"name\":\"mock-registry-cache\",\"version\":\"0.1.0\"}}}'",
        ]
        .join("\n"),
    )
    .expect("write mock registry acp script");
    make_executable(&script);
    let mut descriptor = mock_acp_descriptor("mock-registry-cache", &script);
    descriptor.source = AgentSource::Registry;

    let evidence =
        run_acp_initialize_probe_for_agent(&root, &descriptor).expect("registry probe succeeds");

    assert_eq!(evidence.agent_label, "mock-registry-cache 0.1.0");
    assert!(
        root.join(".viden/cache/npm/mock-registry-cache/test")
            .is_dir()
    );
}

#[test]
fn acp_initialize_probe_records_stderr_on_agent_exit() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_probe_stderr");
    let script = root.join("mock-acp-stderr.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "echo 'Auth: Not authenticated. Please run login' >&2",
            "exit 3",
        ]
        .join("\n"),
    )
    .expect("write mock acp stderr script");
    make_executable(&script);
    let descriptor = mock_acp_descriptor("mock-acp-stderr", &script);

    let error = run_acp_initialize_probe_for_agent(&root, &descriptor)
        .expect_err("probe should fail with stderr");

    assert!(error.contains("ACP command closed stdout without response"));
    assert!(error.contains("Auth: Not authenticated"));
    let log_path = error
        .split("log ")
        .last()
        .expect("log path in error")
        .trim();
    let log = fs::read_to_string(log_path).expect("read probe log");
    assert!(log.contains(r#""direction":"stderr"#));
    assert!(log.contains("Auth: Not authenticated"));
}

#[cfg(unix)]
#[test]
fn acp_child_cleanup_timeout_is_bounded() {
    let _guard = subprocess_test_guard();
    let mut child = Command::new("sh")
        .arg("-c")
        .arg("trap '' TERM; sleep 5")
        .spawn()
        .expect("spawn sleep child");

    let exited = wait_child_timeout(&mut child, Duration::from_millis(100));

    assert!(!exited);
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn acp_async_job_records_status_and_result() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_async_job");
    let script = root.join("mock-acp-async.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "read _init",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{},\"agentInfo\":{\"name\":\"mock-async-acp\",\"version\":\"0.5.0\"}}}'",
            "read _new_session",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"sessionId\":\"session_async\"}}'",
            "read _prompt",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_async\",\"update\":{\"type\":\"AgentMessageChunk\",\"content\":\"async done\"}}}'",
            "sleep 3",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_async\",\"update\":{\"type\":\"TurnEnd\",\"status\":\"completed\"}}}'",
        ]
        .join("\n"),
    )
    .expect("write mock acp async script");
    make_executable(&script);
    let descriptor = mock_acp_descriptor("mock-acp-async", &script);

    let started = start_acp_session_job(
        &root,
        &descriptor,
        "finish quickly".to_string(),
        AcpSessionOptions::default(),
        None,
    )
    .expect("start acp job");
    let id = started
        .lines()
        .find_map(|line| line.split('`').nth(1))
        .expect("job id in output")
        .to_string();
    let runtime_events_path = acp_job_runtime_events_path(&root, &id);

    let live_deadline = Instant::now() + Duration::from_secs(2);
    let mut saw_live_event_while_running = false;
    while Instant::now() < live_deadline {
        let running = find_codex_job(&root, &id)
            .ok()
            .flatten()
            .is_some_and(|job| job.status == "running");
        let runtime_events = read_acp_runtime_events(&runtime_events_path);
        let has_assistant_event = runtime_events.iter().any(|event| {
            matches!(
                &event.kind,
                RuntimeEventKind::AssistantDelta { content, .. } if content == "async done"
            )
        });
        let has_turn_end_evidence = runtime_events.iter().any(|event| {
            matches!(
                &event.kind,
                RuntimeEventKind::EvidenceRecorded { evidence } if evidence.kind == "acp_turn_end"
            )
        });
        if running && has_assistant_event {
            assert!(!has_turn_end_evidence);
            saw_live_event_while_running = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        saw_live_event_while_running,
        "async ACP job should persist assistant runtime events while still running"
    );
    let live_runtime_events =
        fs::read_to_string(&runtime_events_path).expect("read live ACP runtime events");
    assert!(live_runtime_events.contains("async done"));
    let parsed_live_runtime_events = read_acp_runtime_events(&runtime_events_path);
    assert!(!parsed_live_runtime_events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::EvidenceRecorded { evidence } if evidence.kind == "acp_turn_end"
    )));

    wait_until(
        || {
            find_codex_job(&root, &id)
                .ok()
                .flatten()
                .is_some_and(|job| job.status == "finished")
        },
        Duration::from_secs(10),
    );

    let status = render_codex_job_status(&root).expect("render job status");
    let result = render_codex_job_result(&root, Some(&id)).expect("render job result");
    assert!(status.contains("acp-session"));
    assert!(status.contains("finished"));
    assert!(status.contains("session: session_async"));
    assert!(!status.contains("codex resume session_async"));
    assert!(result.contains("session_async"));
    assert!(result.contains("session: session_async"));
    assert!(!result.contains("codex resume session_async"));
    assert!(result.contains("async done"));

    let runtime_events_log =
        fs::read_to_string(&runtime_events_path).expect("read ACP runtime events");
    assert!(runtime_events_log.contains("assistant_delta"));
    assert!(runtime_events_log.contains("acp_turn_end"));
    let runtime_events = tracked_agent_job_runtime_events(&root);
    assert!(runtime_events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::AssistantDelta { content, .. } if content == "async done"
    )));
    assert!(runtime_events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::EvidenceRecorded { evidence }
            if evidence.kind == "acp_turn_end"
                && evidence.summary.contains("completed")
    )));
    assert!(runtime_events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MergeGateUpdated { gate }
            if gate.gate_id == "gate-acp-session-session_async"
                && gate.status == MergeGateStatus::CollectingEvidence
                && gate.decision.as_deref() == Some("missing_canonical")
                && gate.evidence_ids.iter().any(|id| id.starts_with("acp-turn-end-session_async"))
    )));
}

/// Guard that installs a mock ACP command for the `custom-acp` descriptor.
///
/// The typed session path resolves its agent through `acp_agent_descriptors`,
/// so a test agent has to arrive the same way a user's custom agent does.
/// Held together with `subprocess_test_guard`, which serializes every test
/// that exchanges lines with a mock agent process.
struct CustomAcpAgentGuard;

impl CustomAcpAgentGuard {
    fn install(script: &Path) -> Self {
        // SAFETY: serialized by `subprocess_test_guard`, and removed on drop.
        unsafe { env::set_var("VIDEN_AGENT_ACP_COMMAND", script.display().to_string()) };
        Self
    }
}

impl Drop for CustomAcpAgentGuard {
    fn drop(&mut self) {
        // SAFETY: serialized by `subprocess_test_guard`.
        unsafe { env::remove_var("VIDEN_AGENT_ACP_COMMAND") };
    }
}

fn mock_typed_session_script(root: &Path, name: &str, session: &str) -> PathBuf {
    let script = root.join(name);
    fs::write(
        &script,
        [
            "#!/bin/sh".to_string(),
            "read _init".to_string(),
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{},\"agentInfo\":{\"name\":\"mock-typed-acp\",\"version\":\"0.5.0\"}}}'".to_string(),
            "read _new_session".to_string(),
            format!(
                "printf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"sessionId\":\"{session}\"}}}}'"
            ),
            "read _prompt".to_string(),
            format!(
                "printf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{\"sessionId\":\"{session}\",\"update\":{{\"type\":\"AgentMessageChunk\",\"content\":\"first \"}}}}}}'"
            ),
            format!(
                "printf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{\"sessionId\":\"{session}\",\"update\":{{\"type\":\"AgentMessageChunk\",\"content\":\"second\"}}}}}}'"
            ),
            format!(
                "printf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{\"sessionId\":\"{session}\",\"update\":{{\"type\":\"TurnEnd\",\"status\":\"completed\"}}}}}}'"
            ),
        ]
        .join("\n"),
    )
    .expect("write mock typed acp script");
    make_executable(&script);
    script
}

fn typed_session_owner(session_id: &str, lane_id: &str) -> viden_types::RuntimeOwner {
    viden_types::RuntimeOwner {
        workspace_id: "workspace-test".to_string(),
        project_id: "project-test".to_string(),
        lane_id: Some(lane_id.to_string()),
        session_id: Some(session_id.to_string()),
        task_id: None,
        turn_id: None,
    }
}

/// Exactly one writer persists any given ACP runtime event.
///
/// The runner owns every event it streams while `runtime_event_log_path` is
/// set; the monitor owns only the terminal session fact. A second writer would
/// not show up live — the live sink only ever sees each event once — but it
/// doubles the durable file that snapshot assembly and replay rebuild from.
#[test]
fn typed_agent_session_persists_each_runtime_event_once() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_typed_single_write");
    let script = mock_typed_session_script(&root, "mock-acp-typed.sh", "session_typed");
    let _agent_guard = CustomAcpAgentGuard::install(&script);

    let session_id = "agent-session_typed_once".to_string();
    let lane_id = "lane-typed-once".to_string();
    let sink: RuntimeEventSink = Arc::new(|_events| {});
    let approver: AgentSessionApprover = Box::new(|_prompt: viden_types::PermissionPrompt| {
        ApprovalResponse::allow_once(Some("test".into()))
    });

    start_typed_agent_session(
        &root,
        session_id.clone(),
        viden_types::AgentSessionRequest {
            lane_id: lane_id.clone(),
            agent_id: "custom-acp".to_string(),
            model: None,
            load_session_id: None,
            task: "say something".to_string(),
        },
        typed_session_owner(&session_id, &lane_id),
        sink,
        approver,
    )
    .expect("start typed agent session");

    let runtime_events_path = acp_job_runtime_events_path(&root, &session_id);
    wait_until(
        || {
            find_codex_job(&root, &session_id)
                .ok()
                .flatten()
                .is_some_and(|job| !matches!(job.status.as_str(), "running" | "waiting_approval"))
        },
        Duration::from_secs(20),
    );
    // The terminal event is appended after the job record, so wait for it too.
    wait_until(
        || {
            read_acp_runtime_events(&runtime_events_path)
                .iter()
                .any(|event| {
                    matches!(
                        &event.kind,
                        RuntimeEventKind::AgentSessionCompleted { .. }
                            | RuntimeEventKind::AgentSessionFailed { .. }
                            | RuntimeEventKind::AgentSessionUpdated { .. }
                    )
                })
        },
        Duration::from_secs(10),
    );

    let raw = fs::read_to_string(&runtime_events_path).expect("read typed runtime events");
    let mut seen = HashSet::new();
    let mut duplicated = Vec::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let value = serde_json::from_str::<Value>(line).expect("runtime event line is json");
        let key = (
            value
                .get("sequence")
                .and_then(Value::as_u64)
                .expect("sequence"),
            value
                .pointer("/kind/type")
                .and_then(Value::as_str)
                .expect("kind type")
                .to_string(),
        );
        if !seen.insert(key.clone()) {
            duplicated.push(key);
        }
    }
    assert!(
        duplicated.is_empty(),
        "each (sequence, kind) must be persisted exactly once, duplicated: {duplicated:?}\n{raw}"
    );
    let assistant_deltas = read_acp_runtime_events(&runtime_events_path)
        .into_iter()
        .filter(|event| matches!(&event.kind, RuntimeEventKind::AssistantDelta { .. }))
        .count();
    assert_eq!(
        assistant_deltas, 2,
        "the two streamed chunks must be persisted once each\n{raw}"
    );
}

/// One turn publishes one merge gate, under one id.
///
/// The opening `Proposed` fact used to be keyed on the ACP protocol handle
/// while every later update of the same gate was keyed on the Agent session
/// Core published, so a supervised turn produced two gate records: one stuck at
/// `Proposed` under an id that joins to nothing in `RuntimeViewState`, and one
/// collecting the evidence. Both are now keyed on the published session.
/// Legacy logs keep their original ids and bind through the owner backfill —
/// see `replay_binds_legacy_gates_to_the_session_artifact_that_produced_them`.
#[test]
fn a_typed_agent_turn_keys_every_merge_gate_fact_on_the_published_session() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_typed_gate_key");
    let script =
        mock_typed_session_script(&root, "mock-acp-gate-key.sh", "session_protocol_handle");
    let _agent_guard = CustomAcpAgentGuard::install(&script);

    let session_id = "agent-session_typed_gate_key".to_string();
    let lane_id = "lane-typed-gate-key".to_string();
    let sink: RuntimeEventSink = Arc::new(|_events| {});
    let approver: AgentSessionApprover = Box::new(|_prompt: viden_types::PermissionPrompt| {
        ApprovalResponse::allow_once(Some("test".into()))
    });

    start_typed_agent_session(
        &root,
        session_id.clone(),
        viden_types::AgentSessionRequest {
            lane_id: lane_id.clone(),
            agent_id: "custom-acp".to_string(),
            model: None,
            load_session_id: None,
            task: "say something".to_string(),
        },
        typed_session_owner(&session_id, &lane_id),
        sink,
        approver,
    )
    .expect("start typed agent session");

    let runtime_events_path = acp_job_runtime_events_path(&root, &session_id);
    wait_until(
        || {
            read_acp_runtime_events(&runtime_events_path)
                .iter()
                .filter(|event| matches!(&event.kind, RuntimeEventKind::MergeGateUpdated { .. }))
                .count()
                >= 2
        },
        Duration::from_secs(20),
    );

    let gates = read_acp_runtime_events(&runtime_events_path)
        .into_iter()
        .filter_map(|event| match event.kind {
            RuntimeEventKind::MergeGateUpdated { gate } => Some(gate),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        gates.len() >= 2,
        "the turn must publish the proposed gate and at least one update"
    );
    let expected_gate_id = format!("gate-acp-session-{session_id}");
    for gate in &gates {
        assert_eq!(
            gate.gate_id, expected_gate_id,
            "every gate fact of one turn shares the published session key"
        );
        assert_eq!(gate.task_id, format!("acp-session-{session_id}"));
        assert_eq!(gate.owner.session_id.as_deref(), Some(session_id.as_str()));
    }
    assert!(
        gates
            .iter()
            .all(|gate| !gate.gate_id.contains("session_protocol_handle")),
        "no gate fact may still be keyed on the ACP protocol handle"
    );
    assert!(
        gates
            .iter()
            .any(|gate| gate.status == MergeGateStatus::Proposed),
        "the opening proposed fact must be among them"
    );
}

fn acp_runtime_event_fixture_line(event: &RuntimeEvent) -> String {
    serde_json::to_string(event).expect("encode fixture runtime event")
}

/// Legacy files written before the single-writer fix hold each turn's event
/// block twice; the reader heals them without rewriting the artifact.
#[test]
fn read_acp_runtime_events_heals_a_duplicated_completion_block() {
    let root = temp_root("acp_events_heal");
    let path = root.join("agent-input_1.runtime-events.jsonl");
    // The exact shape observed in doubled files: the runner's streamed block,
    // then the same block again from the monitor's completion re-append.
    let block = [
        RuntimeEvent::new(
            1,
            RuntimeEventKind::MergeGateUpdated {
                gate: acp_session_merge_gate("session_heal", None, MergeGateStatus::Proposed, &[]),
            },
        ),
        RuntimeEvent::new(
            2,
            RuntimeEventKind::AssistantDelta {
                message_id: "acp-message-heal".to_string(),
                task_id: None,
                session_id: Some("session_heal".to_string()),
                content: "hello ".to_string(),
            },
        ),
        RuntimeEvent::new(
            3,
            RuntimeEventKind::AssistantDelta {
                message_id: "acp-message-heal".to_string(),
                task_id: None,
                session_id: Some("session_heal".to_string()),
                content: "world".to_string(),
            },
        ),
    ];
    let mut content = String::new();
    for event in block.iter().chain(block.iter()) {
        content.push_str(&acp_runtime_event_fixture_line(event));
        content.push('\n');
    }
    fs::write(&path, &content).expect("write doubled fixture");

    let events = read_acp_runtime_events(&path);

    assert_eq!(
        events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2, 3],
        "a byte-identical repeat of an already-read line is the completion re-append"
    );
    let text: String = events
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::AssistantDelta { content, .. } => Some(content.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "hello world");
}

/// Two genuinely distinct events never serialize identically, because the
/// runner's sequence increments per event. A repeated chunk keeps both copies.
#[test]
fn read_acp_runtime_events_keeps_repeated_content_at_distinct_sequences() {
    let root = temp_root("acp_events_repeat");
    let path = root.join("agent-input_2.runtime-events.jsonl");
    let repeated = |sequence: u64| {
        RuntimeEvent::new(
            sequence,
            RuntimeEventKind::AssistantDelta {
                message_id: "acp-message-repeat".to_string(),
                task_id: None,
                session_id: Some("session_repeat".to_string()),
                content: "tick".to_string(),
            },
        )
    };
    let mut content = String::new();
    for event in [repeated(1), repeated(2), repeated(3)] {
        content.push_str(&acp_runtime_event_fixture_line(&event));
        content.push('\n');
    }
    fs::write(&path, &content).expect("write repeated-content fixture");

    let events = read_acp_runtime_events(&path);

    assert_eq!(
        events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2, 3],
        "identical content at distinct sequences is legitimate streamed output"
    );
}

#[test]
fn acp_async_job_pushes_runtime_events_to_live_sink() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_async_live_sink");
    let script = root.join("mock-acp-live-sink.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "read _init",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{},\"agentInfo\":{\"name\":\"mock-live-acp\",\"version\":\"0.5.0\"}}}'",
            "read _new_session",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"sessionId\":\"session_live\"}}'",
            "read _prompt",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_live\",\"update\":{\"type\":\"AgentMessageChunk\",\"content\":\"live sink delta\"}}}'",
            "sleep 2",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"session_live\",\"update\":{\"type\":\"TurnEnd\",\"status\":\"completed\"}}}'",
        ]
        .join("\n"),
    )
    .expect("write mock acp live sink script");
    make_executable(&script);
    let descriptor = mock_acp_descriptor("mock-acp-live-sink", &script);
    let (sender, receiver) = mpsc::channel();
    let live_sink: RuntimeEventSink = Arc::new(move |events| {
        for event in events {
            let _ = sender.send(event);
        }
    });

    let started = start_acp_session_job(
        &root,
        &descriptor,
        "stream while running".to_string(),
        AcpSessionOptions::default(),
        Some(live_sink),
    )
    .expect("start acp job");
    let id = started
        .lines()
        .find_map(|line| line.split('`').nth(1))
        .expect("job id in output")
        .to_string();

    let live_events = wait_for_channel_events(&receiver, Duration::from_secs(10), |events| {
        let has_proposed_gate = events.iter().any(|event| {
            matches!(
                &event.kind,
                RuntimeEventKind::MergeGateUpdated { gate }
                    if gate.gate_id == "gate-acp-session-session_live"
                        && gate.status == MergeGateStatus::Proposed
            )
        });
        let has_assistant_delta = events.iter().any(|event| {
            matches!(
                &event.kind,
                RuntimeEventKind::AssistantDelta { content, .. } if content == "live sink delta"
            )
        });
        has_proposed_gate && has_assistant_delta
    });
    assert!(live_events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::MergeGateUpdated { gate }
                if gate.gate_id == "gate-acp-session-session_live"
                    && gate.status == MergeGateStatus::Proposed
        )
    }));
    let job = find_codex_job(&root, &id)
        .expect("find job")
        .expect("job exists");
    assert_eq!(job.status, "running");
}

#[test]
fn acp_async_job_can_be_cancelled_by_pid() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_async_cancel");
    let script = root.join("mock-acp-cancel.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "trap 'exit 0' TERM",
            "read _init",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{},\"agentInfo\":{\"name\":\"mock-cancel-acp\",\"version\":\"0.5.0\"}}}'",
            "read _new_session",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"sessionId\":\"session_cancel\"}}'",
            "read _prompt",
            "sleep 20",
        ]
        .join("\n"),
    )
    .expect("write mock acp cancel script");
    make_executable(&script);
    let descriptor = mock_acp_descriptor("mock-acp-cancel", &script);

    let started = start_acp_session_job(
        &root,
        &descriptor,
        "wait".to_string(),
        AcpSessionOptions::default(),
        None,
    )
    .expect("start acp job");
    let id = started
        .lines()
        .find_map(|line| line.split('`').nth(1))
        .expect("job id in output")
        .to_string();
    wait_until(
        || {
            find_codex_job(&root, &id)
                .ok()
                .flatten()
                .is_some_and(|job| job.pid.is_some() && job.status == "running")
        },
        Duration::from_secs(10),
    );

    let cancelled = cancel_codex_job(&root, Some(&id)).expect("cancel acp job");

    assert!(cancelled.contains("Cancelled"));
    let job = find_codex_job(&root, &id)
        .expect("find job")
        .expect("job exists");
    assert_eq!(job.status, "cancelled");
    wait_until(
        || {
            fs::read_to_string(&job.result_path)
                .is_ok_and(|result| result.contains("# ACP session result"))
        },
        Duration::from_secs(5),
    );
    let result = fs::read_to_string(&job.result_path).expect("read cancellation result");
    assert!(result.contains("# ACP session result"));
    assert!(result.contains("status: cancelled"));
    assert!(result.contains("session: session_cancel"));
    assert!(result.contains("tool_calls: none"));
    wait_until(
        || fs::read_to_string(&job.log_path).is_ok_and(|log| log.contains("session/cancel")),
        Duration::from_secs(5),
    );
    let log = fs::read_to_string(&job.log_path).expect("read cancellation log");
    assert!(log.contains("session/cancel"));
}

#[test]
fn cancellation_before_process_start_keeps_durable_job_nonterminal() {
    let root = temp_root("acp_cancel_before_pid");
    let id = "agent-session-before-pid";
    let record = CodexJobRecord {
        id: id.to_string(),
        kind: "acp-session".to_string(),
        status: "running".to_string(),
        pid: None,
        command: "mock-acp".to_string(),
        task: "wait for process startup".to_string(),
        log_path: codex_job_artifact_path(&root, id, "jsonl"),
        result_path: codex_job_artifact_path(&root, id, "result.md"),
        baseline_path: codex_job_artifact_path(&root, id, "baseline.status"),
        updated_at: timestamp_millis(),
        agent: None,
    };
    append_codex_job_record(&root, "started", &record).expect("record pending job");

    let error = cancel_codex_job(&root, Some(id)).expect_err("termination is not confirmed");
    let persisted = find_codex_job(&root, id)
        .expect("read pending job")
        .expect("pending job exists");

    assert!(error.contains("termination is not confirmed"));
    assert_eq!(persisted.status, "running");
    assert!(acp_job_cancel_path(&root, id).exists());
}

#[test]
fn acp_async_job_sends_session_cancel_when_agent_supports_it() {
    let _guard = subprocess_test_guard();
    // Keep the cooperative-cancel window open under parallel test load: if
    // the production grace expired, the process-termination fallback would
    // legitimately erase the session/cancel evidence this test asserts.
    let _grace = widen_acp_session_cancel_grace_for_test(30_000);
    let root = temp_root("acp_async_session_cancel");
    let script = root.join("mock-acp-session-cancel.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "read _init",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{\"sessionCancel\":true},\"agentInfo\":{\"name\":\"mock-session-cancel-acp\",\"version\":\"0.6.0\"}}}'",
            "read _new_session",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"sessionId\":\"session_explicit_cancel\"}}'",
            "read _prompt",
            "while IFS= read -r line; do",
            "  case \"$line\" in",
            "    *'\"method\":\"session/cancel\"'*)",
            "      printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"status\":\"cancelled\"}}'",
            "      printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"method\":\"session/notification\",\"params\":{\"sessionId\":\"session_explicit_cancel\",\"update\":{\"type\":\"TurnEnd\",\"status\":\"cancelled\"}}}'",
            "      exit 0",
            "      ;;",
            "  esac",
            "done",
        ]
        .join("\n"),
    )
    .expect("write mock acp session cancel script");
    make_executable(&script);
    let descriptor = mock_acp_descriptor("mock-acp-session-cancel", &script);

    let started = start_acp_session_job(
        &root,
        &descriptor,
        "wait".to_string(),
        AcpSessionOptions::default(),
        None,
    )
    .expect("start acp job");
    let id = started
        .lines()
        .find_map(|line| line.split('`').nth(1))
        .expect("job id in output")
        .to_string();
    wait_until(
        || {
            find_codex_job(&root, &id)
                .ok()
                .flatten()
                .is_some_and(|job| job.pid.is_some() && job.status == "running")
        },
        Duration::from_secs(10),
    );

    let cancelled = cancel_codex_job(&root, Some(&id)).expect("cancel acp job");

    assert!(cancelled.contains("Cancelled"));
    wait_until(
        || {
            find_codex_job(&root, &id)
                .ok()
                .flatten()
                .is_some_and(|job| job.status == "cancelled")
        },
        Duration::from_secs(10),
    );
    let job = find_codex_job(&root, &id)
        .expect("find job")
        .expect("job exists");
    // The session worker flushes the log and the monitor publishes the
    // rich result asynchronously after cancellation is confirmed.
    wait_until(
        || fs::read_to_string(&job.log_path).is_ok_and(|log| log.contains("session/cancel")),
        Duration::from_secs(10),
    );
    let log = fs::read_to_string(&job.log_path).expect("read cancellation log");
    assert!(log.contains("session/cancel"));
    wait_until(
        || {
            fs::read_to_string(&job.result_path).is_ok_and(|result| {
                result.contains("status: cancelled") && result.contains("session_explicit_cancel")
            })
        },
        Duration::from_secs(10),
    );
    let result = fs::read_to_string(&job.result_path).expect("read cancellation result");
    assert!(result.contains("status: cancelled"));
    assert!(result.contains("session_explicit_cancel"));
}

#[test]
fn acp_initialize_probe_reports_timeout_with_log() {
    let _guard = subprocess_test_guard();
    let root = temp_root("acp_probe_timeout");
    let script = root.join("silent-acp.sh");
    fs::write(&script, "#!/bin/sh\nsleep 10\n").expect("write silent acp script");
    make_executable(&script);

    let error = run_acp_initialize_probe(&root, &script.to_string_lossy())
        .expect_err("probe should time out");

    assert!(error.contains("timed out"));
    assert!(error.contains(".viden/agents/acp-doctor-"));
}

#[cfg(unix)]
#[test]
fn codex_diagnostics_reports_app_server_auth_and_job_store() {
    let root = temp_root("codex_doctor_ok");
    let script = root.join("mock-codex.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "if [ \"$1\" = \"--version\" ]; then",
            "  echo 'codex-cli 9.9.9'",
            "elif [ \"$1\" = \"app-server\" ] && [ \"$2\" = \"--help\" ]; then",
            "  echo 'Usage: codex app-server [OPTIONS]'",
            "elif [ \"$1\" = \"login\" ] && [ \"$2\" = \"status\" ]; then",
            "  echo 'Logged in using ChatGPT'",
            "else",
            "  echo unexpected \"$@\" >&2",
            "  exit 2",
            "fi",
        ]
        .join("\n"),
    )
    .expect("write mock codex script");
    make_executable(&script);

    let report = codex_diagnostics(&root, &script.to_string_lossy());

    let CodexDiagnosticReport::Ready(report) = report else {
        panic!("expected ready Codex report");
    };
    assert_eq!(report.version, "codex-cli 9.9.9");
    assert_eq!(report.app_server, "ok (codex app-server)");
    assert_eq!(report.auth, "Logged in using ChatGPT");
    assert!(report.job_store.ends_with(".viden/agents/codex-jobs.jsonl"));
}

#[test]
fn codex_diagnostics_reports_missing_command() {
    let root = temp_root("codex_doctor_missing");

    let report = codex_diagnostics(&root, "viden-definitely-missing-codex");

    let CodexDiagnosticReport::Unavailable(reason) = report else {
        panic!("expected unavailable Codex report");
    };
    assert!(reason.contains("failed to launch"));
}

#[cfg(unix)]
#[test]
fn codex_job_lifecycle_records_result_artifacts() {
    let _guard = subprocess_test_guard();
    let root = temp_root("codex_job_lifecycle");
    let script = root.join("mock-codex-job.sh");
    fs::write(
        &script,
        [
            "#!/bin/sh",
            "out=''",
            "while [ \"$#\" -gt 0 ]; do",
            "  if [ \"$1\" = \"-o\" ]; then",
            "    shift",
            "    out=\"$1\"",
            "  fi",
            "  shift || true",
            "done",
            "echo 'mock codex log'",
            "if [ -n \"$out\" ]; then",
            "  mkdir -p src",
            "  echo 'pub fn generated() {}' > src/generated.rs",
            "  echo 'mock codex result' > \"$out\"",
            "  echo 'Session ID: ses_test_123' >> \"$out\"",
            "  echo 'Changed files: src/generated.rs' >> \"$out\"",
            "fi",
        ]
        .join("\n"),
    )
    .expect("write mock codex job");
    make_executable(&script);

    let started = start_codex_job(
        &root,
        &script.to_string_lossy(),
        CodexJobKind::Run,
        "hello from test".to_string(),
        vec!["exec".to_string(), "hello from test".to_string()],
    )
    .expect("start codex job");
    let id = started
        .lines()
        .find_map(|line| line.split('`').nth(1))
        .expect("job id in output")
        .to_string();

    wait_until(
        || {
            find_codex_job(&root, &id)
                .ok()
                .flatten()
                .is_some_and(|job| job.status == "finished")
        },
        Duration::from_secs(15),
    );

    let status = render_codex_job_status(&root).expect("render job status");
    let result = render_codex_job_result(&root, Some(&id)).expect("render job result");

    assert!(status.contains(&id));
    assert!(status.contains("finished"));
    assert!(status.contains("resume: codex resume ses_test_123"));
    assert!(status.contains("files: src/generated.rs"));
    assert!(result.contains("mock codex result"));
    assert!(result.contains("resume: codex resume ses_test_123"));
    assert!(result.contains("files: src/generated.rs"));
}

#[cfg(unix)]
#[test]
fn codex_job_cancel_records_cancelled_status() {
    let _guard = subprocess_test_guard();
    let root = temp_root("codex_job_cancel");
    let script = root.join("slow-codex-job.sh");
    fs::write(&script, "#!/bin/sh\nsleep 5\n").expect("write slow codex job");
    make_executable(&script);

    let started = start_codex_job(
        &root,
        &script.to_string_lossy(),
        CodexJobKind::Review,
        "slow review".to_string(),
        vec!["review".to_string()],
    )
    .expect("start codex job");
    let id = started
        .lines()
        .find_map(|line| line.split('`').nth(1))
        .expect("job id in output")
        .to_string();

    let output = cancel_codex_job(&root, Some(&id)).expect("cancel job");
    let job = find_codex_job(&root, &id)
        .expect("read job")
        .expect("job exists");

    assert!(output.contains("Cancelled Codex job"));
    assert_eq!(job.status, "cancelled");
}

fn wait_until(predicate: impl Fn() -> bool, timeout: Duration) {
    let start = SystemTime::now();
    while start.elapsed().unwrap_or_default() < timeout {
        if predicate() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
#[should_panic(expected = "timed out waiting for runtime event condition")]
fn channel_event_wait_fails_at_unmet_condition_boundary() {
    let (_sender, receiver) = mpsc::channel::<RuntimeEvent>();

    let _ = wait_for_channel_events(&receiver, Duration::ZERO, |_| false);
}

fn wait_for_channel_events(
    receiver: &mpsc::Receiver<RuntimeEvent>,
    timeout: Duration,
    predicate: impl Fn(&[RuntimeEvent]) -> bool,
) -> Vec<RuntimeEvent> {
    let deadline = Instant::now() + timeout;
    let mut events = Vec::new();
    while Instant::now() < deadline {
        if predicate(&events) {
            return events;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(remaining.min(Duration::from_millis(20))) {
            Ok(event) => events.push(event),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    assert!(
        predicate(&events),
        "timed out waiting for runtime event condition; observed events: {events:#?}"
    );
    events
}

struct AcpCancelGraceOverrideGuard;

impl Drop for AcpCancelGraceOverrideGuard {
    fn drop(&mut self) {
        ACP_SESSION_CANCEL_GRACE_OVERRIDE_MS.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

// Hold alongside `subprocess_test_guard` so the widened window is cleared
// before any test that exercises the process-termination fallback runs.
fn widen_acp_session_cancel_grace_for_test(ms: u64) -> AcpCancelGraceOverrideGuard {
    ACP_SESSION_CANCEL_GRACE_OVERRIDE_MS.store(ms, std::sync::atomic::Ordering::Relaxed);
    AcpCancelGraceOverrideGuard
}

fn subprocess_test_guard() -> MutexGuard<'static, ()> {
    // Mock app-server and Codex job tests exchange lines with subprocesses;
    // serialize them so default parallel test runs do not starve timeout paths.
    SUBPROCESS_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn temp_root(name: &str) -> PathBuf {
    let root = env::temp_dir().join(format!(
        "viden-runtime-{name}-{}-{}-{}",
        std::process::id(),
        timestamp_millis(),
        TEMP_ROOT_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).expect("create temp root");
    root
}

fn mock_acp_descriptor(agent_id: &str, script: &Path) -> AgentPluginDescriptor {
    AgentPluginDescriptor {
        agent_id: agent_id.to_string(),
        display_name: "Mock ACP".to_string(),
        version: "test".to_string(),
        transport: AgentTransport::Acp,
        source: AgentSource::LocalCommand,
        command: AgentCommandSpec {
            command: script.display().to_string(),
            args: vec![],
            env: vec![],
        },
        registry_package: None,
        protocol_versions: vec![AgentProtocolVersion::AcpV1],
        auth_modes: vec![AgentAuthMode::AgentNative],
        capabilities: vec![
            AgentPluginCapability::SessionPrompt,
            AgentPluginCapability::StreamingUpdates,
        ],
        permission_profile: AgentPermissionProfile::RuntimeGated,
        experimental_methods: vec![],
        config_schema_version: 1,
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("chmod");
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

/// A gate must name the Agent session Core published, not only the ACP
/// protocol handle its id is keyed on. Without this a frontend cannot tell
/// whether a pending gate belongs to a session that is still running.
#[test]
fn an_acp_merge_gate_carries_the_published_owner_session() {
    let owner = viden_types::RuntimeOwner {
        workspace_id: "workspace-viden".to_string(),
        project_id: "project-viden".to_string(),
        lane_id: Some("lane_1".to_string()),
        session_id: Some("agent-session_1".to_string()),
        ..Default::default()
    };
    let gate = acp_session_merge_gate(
        "019fbc5a-1e63-7e41-8df7-81a43556838a",
        Some(&owner),
        MergeGateStatus::Proposed,
        &[],
    );
    // The gate's own identity is unchanged: it stays keyed on the ACP
    // protocol session so already-persisted gates keep the same id.
    assert_eq!(
        gate.gate_id,
        "gate-acp-session-019fbc5a-1e63-7e41-8df7-81a43556838a"
    );
    assert_eq!(
        gate.task_id,
        "acp-session-019fbc5a-1e63-7e41-8df7-81a43556838a"
    );
    // The owner is what makes the gate joinable to `agent_sessions`.
    assert_eq!(gate.owner.session_id.as_deref(), Some("agent-session_1"));
    assert_eq!(gate.owner.lane_id.as_deref(), Some("lane_1"));
    assert_eq!(gate.owner.workspace_id, "workspace-viden");
    assert_eq!(gate.owner.project_id, "project-viden");
}

/// Without a published owner the gate stays exactly the bytes it was, so a
/// gate emitted where Core knows no owner never invents one.
#[test]
fn an_acp_merge_gate_without_an_owner_invents_none() {
    let gate = acp_session_merge_gate("session_x", None, MergeGateStatus::Proposed, &[]);
    assert_eq!(gate.owner.session_id, None);
    assert_eq!(gate.owner.lane_id, None);
    assert_eq!(gate.owner.task_id.as_deref(), Some("acp-session-session_x"));
}

/// Logs written before gates carried their owner still replay into joinable
/// gates: the session artifact that produced the gate names the session, and a
/// later follow-up turn of the same gate inherits that binding.
#[test]
fn replay_binds_legacy_gates_to_the_session_artifact_that_produced_them() {
    let root = temp_root("acp_gate_owner_backfill");
    let agents = root.join(".viden").join("agents");
    fs::create_dir_all(&agents).expect("create agents dir");

    // A gate written by the session's own first turn, with no owner at all.
    let legacy_gate = acp_session_merge_gate("019fb746", None, MergeGateStatus::Proposed, &[]);
    let first_turn = [RuntimeEvent::new(
        1,
        RuntimeEventKind::MergeGateUpdated {
            gate: legacy_gate.clone(),
        },
    )];
    write_acp_runtime_events(
        &agents.join("agent-session_1785486260041818000.runtime-events.jsonl"),
        &first_turn,
    )
    .expect("write first turn log");

    // The same gate re-emitted by a later follow-up turn, whose artifact id is
    // an input id and therefore names no session by itself.
    write_acp_runtime_events(
        &agents.join("agent-input_1785487371561133000.runtime-events.jsonl"),
        &first_turn,
    )
    .expect("write follow-up turn log");

    let events = tracked_agent_job_runtime_events(&root);
    let gates = events
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::MergeGateUpdated { gate } => Some(gate),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(gates.len(), 2, "both turns replay their gate");
    for gate in gates {
        assert_eq!(
            gate.gate_id, "gate-acp-session-019fb746",
            "the gate id must not change while backfilling its owner"
        );
        assert_eq!(
            gate.owner.session_id.as_deref(),
            Some("agent-session_1785486260041818000"),
            "a legacy gate must be bound to the session artifact that produced it"
        );
    }
}
