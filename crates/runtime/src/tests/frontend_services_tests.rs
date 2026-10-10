use super::*;
use crate::frontend_status::{
    MAX_COCKPIT_PATCH_BYTES, check_run_from_tool_result, runtime_service_health,
    sample_workspace_source, sample_workspace_source_with_git, workspace_changes_from_tool_result,
};
use crate::{EngineEvent, RuntimeSupervisor, SessionEngine};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use viden_config::CliOverrides;
use viden_lsp::{LspRuntime, LspServerRegistry};
use viden_types::{
    AgentDagTaskSpec, AgentRole, AgentTaskKind, AgentTaskStatus, ApprovalResponse, EventCursor,
    LocaleId, Message, RecentWorkQuery, ReplayRequest, Role, RuntimeCommand, RuntimeEvent,
    RuntimeEventEnvelope, RuntimeEventKind, RuntimeOwner, RuntimeServiceKind, RuntimeServiceStatus,
    RuntimeViewState, RuntimeWireEvent, SessionMetaEntry, StarterLanePreset,
    StarterLanePreviewInvalidationReason, StarterLaneRequest, ToolCall, ToolResult,
    TranscriptEntry, UiColorMode, UiDensity, UiMotion, UiPreferencePatch, UiPreferences, UiSkin,
    WorkMode, WorkspaceChangeKind, WorkspaceSourceStatus,
};

use viden_lanes::{
    LaneEffectExecutor, LaneEffectRequest, LaneEffectResult, LanePersistence,
    LocalLaneEffectExecutor,
};
use viden_workflows::lanes::LaneEvent;

#[test]
fn frontend_status_preserves_the_legacy_public_engine_event_surface() {
    fn legacy_event_kind(event: EngineEvent) -> &'static str {
        match event {
            EngineEvent::System(_) => "system",
            EngineEvent::Assistant(_) => "assistant",
            EngineEvent::ToolCall(_) => "tool_call",
            EngineEvent::ToolResult { .. } => "tool_result",
            EngineEvent::Command(_) => "command",
        }
    }

    assert_eq!(
        legacy_event_kind(EngineEvent::System("ready".to_string())),
        "system"
    );
}

#[test]
fn frontend_status_git_sampling_is_read_only_and_missing_git_is_not_clean() {
    let repo = starter_lane_repo("frontend_status_git_sampling");
    fs::write(repo.join("README.md"), "starter\nchanged\n").unwrap();
    for index in 0..75 {
        fs::write(repo.join(format!("untracked-{index}.txt")), "untracked\n").unwrap();
    }
    let before = run_git(&repo, &["status", "--porcelain=v1", "--branch"]);

    let source = sample_workspace_source(&repo);

    let after = run_git(&repo, &["status", "--porcelain=v1", "--branch"]);
    assert_eq!(after, before, "source inspection must not mutate Git state");
    assert_eq!(source.status, WorkspaceSourceStatus::Ready);
    assert_eq!(source.branch.as_deref(), Some("main"));
    assert!(source.worktree.is_some());
    assert!(source.dirty);
    assert!(source.added > 0);
    assert_eq!(source.behind, 0);

    let non_repo = temp_dir("frontend_status_missing_git");
    let unavailable = sample_workspace_source(&non_repo);
    assert_eq!(unavailable.status, WorkspaceSourceStatus::Unavailable);
    assert!(!unavailable.dirty);
}

#[cfg(unix)]
#[test]
fn frontend_status_git_sampling_is_time_and_memory_bounded_and_marks_truncation() {
    use std::os::unix::fs::PermissionsExt;

    let cwd = temp_dir("frontend_status_bounded_git");
    let fake_git = cwd.join("fake-git");
    fs::write(
        &fake_git,
        r#"#!/bin/sh
[ "$GIT_OPTIONAL_LOCKS" = "0" ] || exit 9
case "$*" in
  *"rev-parse --show-toplevel"*) printf '%s\n' "$PWD" ;;
  *"symbolic-ref --quiet --short HEAD"*) printf 'main\n' ;;
  *"status --porcelain=v1"*)
    (sleep 5) &
    i=0
    while [ "$i" -lt 100000 ]; do
      printf ' M file-%s\n' "$i"
      i=$((i + 1))
    done
    sleep 5
    ;;
  *"rev-list --left-right --count"*) printf '0 0\n' ;;
  *"diff --no-ext-diff --no-textconv --numstat"*) printf '1\t0\tREADME.md\n' ;;
  *) exit 8 ;;
esac
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_git).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_git, permissions).unwrap();

    let started = Instant::now();
    let source = sample_workspace_source_with_git(&cwd, &fake_git, Duration::from_millis(100));

    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(source.status, WorkspaceSourceStatus::Truncated);
    assert_eq!(source.added, 0, "partial totals must not be published");
    assert_eq!(source.deleted, 0, "partial totals must not be published");
}

#[cfg(unix)]
#[test]
fn frontend_status_repeated_git_timeouts_leave_no_descendants_or_blocked_pipes() {
    use std::os::unix::fs::PermissionsExt;

    let cwd = temp_dir("frontend_status_repeated_git_timeout");
    let fake_git = cwd.join("fake-git");
    let pid_log = cwd.join("descendant-pids");
    fs::write(
        &fake_git,
        format!(
            r#"#!/bin/sh
case "$*" in
  *"rev-parse --show-toplevel"*) printf '%s\n' "{cwd}" ;;
  *"symbolic-ref --quiet --short HEAD"*) printf 'main\n' ;;
  *"status --porcelain=v1"*)
    sleep 30 &
    printf '%s\n' "$!" >> "{pid_log}"
    wait
    ;;
  *) printf '0 0\n' ;;
esac
"#,
            cwd = cwd.display(),
            pid_log = pid_log.display(),
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_git).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_git, permissions).unwrap();

    // Under parallel test load a fast phase (rev-parse, symbolic-ref) can
    // itself hit the 1s timeout, so a fixed sample count is not guaranteed to
    // reach the blocking status phase. Resample until the fixture logged two
    // blocking-phase descendants, bounded to keep a real regression finite.
    let mut logged_pids = 0usize;
    for _ in 0..12 {
        let sample_started = Instant::now();
        let source = sample_workspace_source_with_git(&cwd, &fake_git, Duration::from_secs(1));
        assert_eq!(source.status, WorkspaceSourceStatus::Truncated);
        // A sample that waits for the 30s descendant holding stdout takes at
        // least that long; 20s separates it from load-induced slowness.
        assert!(
            sample_started.elapsed() < Duration::from_secs(20),
            "timed-out Git sampling appears to have waited for the sleeping descendant: {:?}",
            sample_started.elapsed()
        );
        logged_pids = fs::read_to_string(&pid_log)
            .map(|log| log.lines().count())
            .unwrap_or(0);
        if logged_pids >= 2 {
            break;
        }
    }
    assert!(
        logged_pids >= 2,
        "repeated timeout fixture did not reach the blocking Git phase often enough: \
         {logged_pids} blocking samples in 12 attempts"
    );

    let pids = fs::read_to_string(&pid_log)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let alive = pids
        .iter()
        .filter(|pid| {
            std::process::Command::new("kill")
                .args(["-0", pid.as_str()])
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        })
        .cloned()
        .collect::<Vec<_>>();
    for pid in &alive {
        let _ = std::process::Command::new("kill")
            .args(["-KILL", pid])
            .stderr(std::process::Stdio::null())
            .status();
    }
    assert!(
        alive.is_empty(),
        "timed-out Git sampling leaked descendant processes holding stdout: {alive:?}"
    );
}

#[cfg(unix)]
#[test]
fn frontend_status_git_totals_cover_all_rows_without_a_row_cap() {
    use std::os::unix::fs::PermissionsExt;

    let cwd = temp_dir("frontend_status_complete_git_totals");
    let fake_git = cwd.join("fake-git");
    // Every fake-git call leaves an empty marker named after its subcommand
    // once its output is written. That is the signal this test owns. SIGPIPE
    // is ignored so a reader that stops at the output cap cannot kill the
    // script before the marker lands; a missing marker therefore means the
    // sampler's deadline killed that call, and `Truncated` with every marker
    // present can only be the output cap or a row cap — what this test is
    // about.
    let finished = cwd.join("finished");
    fs::create_dir_all(&finished).unwrap();
    fs::write(
        &fake_git,
        format!(
            r#"#!/bin/sh
trap '' PIPE
[ "$GIT_OPTIONAL_LOCKS" = "0" ] || exit 9
case "$*" in
  *"rev-parse --show-toplevel"*) printf '%s\n' "$PWD"; call=rev-parse ;;
  *"symbolic-ref --quiet --short HEAD"*) printf 'main\n'; call=symbolic-ref ;;
  *"status --porcelain=v1"*) printf ' M README.md\n'; call=status ;;
  *"rev-list --left-right --count"*) printf '2 3\n'; call=rev-list ;;
  *"diff --no-ext-diff --no-textconv --numstat"*)
    i=0
    while [ "$i" -lt 75 ]; do
      if [ "$i" -eq 0 ]; then
        printf '1000001\t2\tfile-%s\n' "$i"
      else
        printf '1\t2\tfile-%s\n' "$i"
      fi
      i=$((i + 1))
    done
    call=diff
    ;;
  *) exit 8 ;;
esac
# An empty marker, never a printf: a printf after a failed stdout write
# would flush the stale stdout buffer into whatever it is redirected to.
: > "{finished}/$call"
"#,
            finished = finished.display(),
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_git).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_git, permissions).unwrap();

    // The per-call budget is a liveness ceiling, not a check: this test is
    // about row caps, and the markers above tell a deadline kill apart. On
    // macOS the first exec of a just-written executable can stall ~3 s
    // before it runs a line while the full suite runs. C19 measured the
    // fake-git child sitting at 0 CPU and exiting by itself at 3.01-3.02 s,
    // while `/bin/sh <a fresh file>` and a re-exec of the same script took
    // 3-6 ms. The old 3 s budget raced the operating system, not the sampler.
    let source = sample_workspace_source_with_git(&cwd, &fake_git, Duration::from_secs(30));

    let unfinished = ["rev-parse", "symbolic-ref", "status", "rev-list", "diff"]
        .into_iter()
        .filter(|call| !finished.join(call).exists())
        .collect::<Vec<_>>();
    assert!(
        unfinished.is_empty(),
        "fake-git calls {unfinished:?} never finished (the sampler stops at \
         the first one its deadline killed); that is not a row cap \
         (source: {source:?})"
    );
    assert_eq!(
        source.status,
        WorkspaceSourceStatus::Ready,
        "all five Git calls finished, so `Truncated` here is an output or row cap"
    );
    assert_eq!(source.ahead, 2);
    assert_eq!(source.behind, 3);
    assert_eq!(source.added, 1_000_075);
    assert_eq!(source.deleted, 150);
}

#[test]
fn frontend_status_service_health_never_invents_mcp_or_lsp_readiness() {
    let repo = temp_dir("frontend_status_service_health");
    fs::write(repo.join(".mcp.json"), r#"{"mcpServers":{"demo":{}}}"#).unwrap();
    let no_lsp = LspRuntime::new(LspServerRegistry::new(Vec::new()));

    let services = runtime_service_health(&repo, &no_lsp);

    assert!(services.iter().any(|service| {
        service.kind == RuntimeServiceKind::Mcp
            && service.status == RuntimeServiceStatus::Unavailable
    }));
    assert!(services.iter().any(|service| {
        service.kind == RuntimeServiceKind::Lsp
            && service.status == RuntimeServiceStatus::Unavailable
    }));
    assert!(services.iter().all(|service| {
        service.status != RuntimeServiceStatus::Connected
            && service.status != RuntimeServiceStatus::Ready
    }));

    let configured_but_not_running = LspRuntime::new(LspServerRegistry::default());
    let configured = runtime_service_health(&repo, &configured_but_not_running);
    assert!(configured.iter().any(|service| {
        service.kind == RuntimeServiceKind::Lsp
            && service.id == "rust-analyzer"
            && service.status == RuntimeServiceStatus::Offline
    }));

    let snapshot = viden_types::RuntimeSnapshot {
        cwd: repo,
        provider_family: "test".to_string(),
        model_label: "test".to_string(),
        work_mode: WorkMode::Build,
        permission_mode: viden_types::PermissionMode::Default,
        permission_level: viden_types::PermissionLevel::Ask,
        config_summary: String::new(),
        loaded_config_files: Vec::new(),
        startup_overrides: Vec::new(),
        ui_preferences: viden_types::ResolvedUiPreferences::default(),
    };
    let mut view = RuntimeViewState::new(snapshot);
    let mut first = services[0].clone();
    first.status = RuntimeServiceStatus::Connected;
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::RuntimeServiceHealthUpdated { service: first },
    ));
    view.apply_event(&RuntimeEvent::new(
        2,
        RuntimeEventKind::RuntimeServiceHealthUpdated {
            service: services[0].clone(),
        },
    ));
    assert_eq!(view.runtime_services.len(), 1);
    assert_eq!(
        view.runtime_services[0].status,
        RuntimeServiceStatus::Unavailable
    );
}

#[test]
fn frontend_status_structured_tool_results_are_owner_bound_and_bounded() {
    let owner = RuntimeOwner {
        workspace_id: "workspace-cockpit".to_string(),
        project_id: "project-cockpit".to_string(),
        lane_id: Some("lane-cockpit".to_string()),
        session_id: Some("session-cockpit".to_string()),
        ..RuntimeOwner::default()
    };
    let mut file_input = viden_types::ToolInput::new();
    file_input.insert("path".to_string(), "src/lib.rs".to_string());
    let file_call = ToolCall {
        id: "tool-file".to_string(),
        name: "edit_file".to_string(),
        input: file_input,
    };
    let file_result = ToolResult {
        tool_call_id: file_call.id.clone(),
        name: file_call.name.clone(),
        output: "rendered output must not become a patch".to_string(),
        diff: Some(format!(
            "--- before\n+++ after\n{}",
            "+bounded\n".repeat(20_000)
        )),
        success: true,
        exit_code: None,
    };

    let changes = workspace_changes_from_tool_result(&file_call, &file_result, &owner);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].owner, owner);
    assert_eq!(changes[0].path, "src/lib.rs");
    assert!(
        changes[0].patch.as_ref().unwrap().len() <= MAX_COCKPIT_PATCH_BYTES,
        "captured patch must have a documented byte cap"
    );
    assert!(
        !changes[0]
            .patch
            .as_ref()
            .unwrap()
            .contains("rendered output")
    );
    assert_eq!(
        changes[0].additions, 20_000,
        "counts must come from the complete structured diff before display truncation"
    );

    let mut write_input = viden_types::ToolInput::new();
    write_input.insert("path".to_string(), "existing.txt".to_string());
    let write_call = ToolCall {
        id: "tool-write-existing".to_string(),
        name: "write_file".to_string(),
        input: write_input,
    };
    let write_result = ToolResult {
        tool_call_id: write_call.id.clone(),
        name: write_call.name.clone(),
        output: String::new(),
        diff: Some("--- before\n+++ after\n+replacement\n".to_string()),
        success: true,
        exit_code: None,
    };
    let conservative = workspace_changes_from_tool_result(&write_call, &write_result, &owner);
    assert_eq!(conservative[0].kind, WorkspaceChangeKind::Modified);

    let mut check_input = viden_types::ToolInput::new();
    check_input.insert(
        "command".to_string(),
        "cargo test -p viden-runtime".to_string(),
    );
    let check_call = ToolCall {
        id: "tool-check".to_string(),
        name: "shell".to_string(),
        input: check_input,
    };
    let check_result = ToolResult {
        tool_call_id: check_call.id.clone(),
        name: check_call.name.clone(),
        output: "test result: ok. this display text must not override failure".to_string(),
        diff: None,
        success: false,
        exit_code: Some(101),
    };
    let check = check_run_from_tool_result(&check_call, &check_result, &owner)
        .expect("a real executed check command should produce a check fact");
    assert_eq!(check.owner, owner);
    assert_eq!(check.command, "cargo test -p viden-runtime");
    assert_eq!(check.status, viden_types::CheckRunStatus::Failed);
    assert_eq!(check.failing_location, None);
    assert!(!check.summary.contains("test result: ok"));

    let failed_file_result = ToolResult {
        success: false,
        diff: Some("invented".to_string()),
        ..file_result
    };
    assert!(workspace_changes_from_tool_result(&file_call, &failed_file_result, &owner).is_empty());
    let unrelated_call = ToolCall {
        id: "tool-read".to_string(),
        name: "read_file".to_string(),
        input: viden_types::ToolInput::new(),
    };
    assert!(
        check_run_from_tool_result(&unrelated_call, &check_result, &owner).is_none(),
        "rendered output alone must never synthesize a check"
    );
}

#[test]
fn frontend_status_snapshot_and_replay_have_the_same_cockpit_hash() {
    let repo = starter_lane_repo("frontend_status_snapshot_replay");
    let home = temp_dir("frontend_status_snapshot_replay_home");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let engine = SessionEngine::new_with_home(&repo, provider, Some(home)).unwrap();
    let supervisor = RuntimeSupervisor::start(engine);

    let snapshot = supervisor.snapshot_envelope().unwrap();
    assert!(
        snapshot.cursor.sequence > 0,
        "snapshot sampling must be ordered through the runtime journal"
    );
    let replay = supervisor
        .replay_events(ReplayRequest {
            after: EventCursor {
                stream_id: snapshot.cursor.stream_id.clone(),
                sequence: 0,
            },
            limit: 100,
        })
        .unwrap();
    assert!(replay.events.iter().any(|envelope| {
        matches!(
            envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::WorkspaceSourceUpdated { .. },
                ..
            })
        )
    }));
    let mut replayed = RuntimeViewState::new(snapshot.view.snapshot.clone());
    for envelope in replay.events {
        if let RuntimeWireEvent::Known(event) = envelope.event {
            replayed.apply_event(&event);
        }
    }
    let live_cockpit = serde_json::to_vec(&(
        &snapshot.view.workspace_source,
        &snapshot.view.runtime_services,
        &snapshot.view.workspace_changes,
        &snapshot.view.check_runs,
    ))
    .unwrap();
    let replayed_cockpit = serde_json::to_vec(&(
        &replayed.workspace_source,
        &replayed.runtime_services,
        &replayed.workspace_changes,
        &replayed.check_runs,
    ))
    .unwrap();
    assert_eq!(
        Sha256::digest(live_cockpit),
        Sha256::digest(replayed_cockpit)
    );
}

#[test]
fn frontend_status_later_git_failure_replaces_stale_ready_source() {
    let repo = starter_lane_repo("frontend_status_source_replacement");
    let home = temp_dir("frontend_status_source_replacement_home");
    let engine =
        SessionEngine::new_with_home(&repo, Box::new(SequenceProvider::new(vec![])), Some(home))
            .unwrap();
    let supervisor = RuntimeSupervisor::start(engine);

    let ready = supervisor.snapshot_envelope().unwrap();
    assert_eq!(
        ready.view.workspace_source.as_ref().unwrap().status,
        WorkspaceSourceStatus::Ready
    );
    fs::rename(repo.join(".git"), repo.join(".git-offline")).unwrap();

    let unavailable = supervisor.snapshot_envelope().unwrap();

    assert!(unavailable.cursor.sequence > ready.cursor.sequence);
    assert_eq!(
        unavailable.view.workspace_source.as_ref().unwrap().status,
        WorkspaceSourceStatus::Unavailable
    );
    assert_eq!(
        unavailable.view.workspace_source.as_ref().unwrap().branch,
        None
    );
}

#[test]
fn frontend_status_supervisor_rejects_a_second_agent_binding_for_one_lane() {
    let cwd = temp_dir("frontend_status_unique_lane_agent");
    let home = temp_dir("frontend_status_unique_lane_agent_home");
    let engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(vec![vec![
            viden_types::ModelEvent::Done,
        ]])),
        Some(home),
    )
    .unwrap();
    let supervisor = RuntimeSupervisor::start(engine);
    let owner = RuntimeOwner {
        workspace_id: "workspace-agent-binding".to_string(),
        project_id: "project-agent-binding".to_string(),
        lane_id: Some("lane-agent-binding".to_string()),
        session_id: Some("session-native-agent".to_string()),
        ..RuntimeOwner::default()
    };

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd-native-agent",
            RuntimeCommand::SubmitUserInput {
                content: "bind the Lane to native Viden".to_string(),
            },
        )
        .unwrap();
    let first = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandAccepted { command_id, .. },
                    ..
                }) if command_id == "cmd-native-agent"
            )
        }) && events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::TaskUpdated { task },
                    ..
                }) if task.status == viden_types::AgentTaskStatus::Done
            )
        })
    });
    assert!(first.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::CommandAccepted { command_id, .. },
                ..
            }) if command_id == "cmd-native-agent"
        )
    }));

    supervisor
        .send_command_from_owner(
            owner,
            "cmd-second-agent",
            RuntimeCommand::StartAgentSession {
                request: viden_types::AgentSessionRequest {
                    lane_id: "lane-agent-binding".to_string(),
                    agent_id: "claude-acp".to_string(),
                    model: None,
                    load_session_id: None,
                    task: "must not replace the Lane agent".to_string(),
                },
            },
        )
        .unwrap();
    let rejected = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandRejected { command_id, reason },
                    ..
                }) if command_id == "cmd-second-agent"
                    && reason.starts_with("lane_already_bound_to_agent_session")
            )
        })
    });
    assert!(rejected.iter().all(|envelope| {
        !matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::CommandAccepted { command_id, .. },
                ..
            }) if command_id == "cmd-second-agent"
        )
    }));
    assert!(rejected.iter().all(|envelope| {
        !matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::AgentSessionStarted { session },
                ..
            }) if session.lane_id == "lane-agent-binding"
        )
    }));
}

#[test]
fn frontend_status_native_lane_binding_survives_core_restart() {
    let cwd = temp_dir("frontend_status_native_binding_restart");
    let home = temp_dir("frontend_status_native_binding_restart_home");
    let owner = RuntimeOwner {
        workspace_id: "workspace-agent-restart".to_string(),
        project_id: "project-agent-restart".to_string(),
        lane_id: Some("lane-agent-restart".to_string()),
        session_id: Some("session-native-restart".to_string()),
        ..RuntimeOwner::default()
    };
    let engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(vec![vec![
            viden_types::ModelEvent::Done,
        ]])),
        Some(home.clone()),
    )
    .unwrap();
    let first = RuntimeSupervisor::start(engine);
    first
        .send_command_from_owner(
            owner.clone(),
            "cmd-bind-before-restart",
            RuntimeCommand::SubmitUserInput {
                content: "bind native Viden".to_string(),
            },
        )
        .unwrap();
    collect_starter_envelopes_until(&first, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::TaskUpdated { task },
                    ..
                }) if task.status == viden_types::AgentTaskStatus::Done
            )
        })
    });
    drop(first);

    let restarted = RuntimeSupervisor::start(
        SessionEngine::new_with_home(&cwd, Box::new(SequenceProvider::new(vec![])), Some(home))
            .unwrap(),
    );
    restarted
        .send_command_from_owner(
            owner,
            "cmd-rebind-after-restart",
            RuntimeCommand::StartAgentSession {
                request: viden_types::AgentSessionRequest {
                    lane_id: "lane-agent-restart".to_string(),
                    agent_id: "codex-acp".to_string(),
                    model: None,
                    load_session_id: None,
                    task: "must remain rejected after restart".to_string(),
                },
            },
        )
        .unwrap();
    let events = collect_starter_envelopes_until(&restarted, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandRejected { command_id, reason },
                    ..
                }) if command_id == "cmd-rebind-after-restart"
                    && reason.starts_with("lane_already_bound_to_agent_session")
            )
        })
    });
    assert!(events.iter().all(|envelope| {
        !matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::CommandAccepted { command_id, .. },
                ..
            }) if command_id == "cmd-rebind-after-restart"
        )
    }));
}

#[test]
fn frontend_status_native_lane_binding_without_owner_session_uses_runtime_session() {
    let cwd = temp_dir("frontend_status_native_binding_runtime_session");
    let home = temp_dir("frontend_status_native_binding_runtime_session_home");
    let engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(vec![
            vec![viden_types::ModelEvent::Done],
            vec![viden_types::ModelEvent::Done],
        ])),
        Some(home),
    )
    .unwrap();
    let supervisor = RuntimeSupervisor::start(engine);
    let owner = RuntimeOwner {
        workspace_id: "workspace-native-session".to_string(),
        project_id: "project-native-session".to_string(),
        lane_id: Some("lane-native-session".to_string()),
        ..RuntimeOwner::default()
    };

    for (command_id, content) in [
        ("cmd-native-session-one", "first native turn"),
        ("cmd-native-session-two", "second native turn"),
    ] {
        supervisor
            .send_command_from_owner(
                owner.clone(),
                command_id,
                RuntimeCommand::SubmitUserInput {
                    content: content.to_string(),
                },
            )
            .unwrap();
        let events = collect_starter_envelopes_until(&supervisor, |events| {
            events.iter().any(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::TaskUpdated { task },
                        ..
                    }) if task.status == viden_types::AgentTaskStatus::Done
                )
            })
        });
        assert!(events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandAccepted {
                        command_id: accepted,
                        ..
                    },
                    ..
                }) if accepted == command_id
            )
        }));
    }
}

#[test]
fn frontend_status_worker_revalidates_a_binding_persisted_after_startup() {
    let cwd = temp_dir("frontend_status_binding_revalidation");
    let home = temp_dir("frontend_status_binding_revalidation_home");
    let engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(vec![vec![
            viden_types::ModelEvent::Done,
        ]])),
        Some(home),
    )
    .unwrap();
    let store = engine.workflow_store();
    let supervisor = RuntimeSupervisor::start(engine);
    store
        .bind_lane_agent_once(
            "lane-revalidation",
            "codex-acp",
            "session-external",
            viden_types::now_timestamp(),
        )
        .unwrap();
    let owner = RuntimeOwner {
        workspace_id: "workspace-revalidation".to_string(),
        project_id: "project-revalidation".to_string(),
        lane_id: Some("lane-revalidation".to_string()),
        session_id: Some("session-native".to_string()),
        ..RuntimeOwner::default()
    };

    supervisor
        .send_command_from_owner(
            owner,
            "cmd-revalidate-binding",
            RuntimeCommand::SubmitUserInput {
                content: "must revalidate durable ownership".to_string(),
            },
        )
        .unwrap();
    let events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandRejected { command_id, reason },
                    ..
                }) if command_id == "cmd-revalidate-binding"
                    && reason.starts_with("lane_already_bound_to_agent_session")
            )
        })
    });
    let reason = events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::CommandRejected { command_id, reason },
                ..
            }) if command_id == "cmd-revalidate-binding" => Some(reason),
            _ => None,
        })
        .unwrap();
    assert!(reason.contains("durable Lane-agent identity"));
    assert!(!reason.contains("active agent session"));
    assert!(events.iter().all(|envelope| {
        !matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::CommandAccepted { command_id, .. },
                ..
            }) if command_id == "cmd-revalidate-binding"
        )
    }));
}

#[test]
fn frontend_status_production_path_emits_only_executed_tool_facts_with_owner() {
    let cwd = temp_dir("frontend_status_production_tool_facts");
    let home = temp_dir("frontend_status_production_tool_facts_home");
    let mut file_input = viden_types::ToolInput::new();
    file_input.insert("path".to_string(), "generated.txt".to_string());
    file_input.insert("content".to_string(), "generated\n".to_string());
    let mut check_input = viden_types::ToolInput::new();
    check_input.insert("command".to_string(), "cargo check --help".to_string());
    let provider = Box::new(SequenceProvider::new(vec![
        vec![viden_types::ModelEvent::ToolCall(ToolCall {
            id: "tool-production-file".to_string(),
            name: "write_file".to_string(),
            input: file_input,
        })],
        vec![viden_types::ModelEvent::Done],
        vec![viden_types::ModelEvent::ToolCall(ToolCall {
            id: "tool-production-check".to_string(),
            name: "shell".to_string(),
            input: check_input,
        })],
        vec![viden_types::ModelEvent::Done],
    ]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let supervisor = RuntimeSupervisor::start(engine);
    let owner = RuntimeOwner {
        workspace_id: "workspace-production".to_string(),
        project_id: "project-production".to_string(),
        lane_id: Some("lane-production".to_string()),
        session_id: Some("session-production".to_string()),
        ..RuntimeOwner::default()
    };

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd-production-file",
            RuntimeCommand::SubmitUserInput {
                content: "create a file".to_string(),
            },
        )
        .unwrap();
    let change_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::WorkspaceChangeUpdated { .. },
                    ..
                })
            )
        }) && events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::TaskUpdated { task },
                    ..
                }) if task.status == viden_types::AgentTaskStatus::Done
            )
        })
    });
    let (change_owner, change) = change_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::WorkspaceChangeUpdated { change },
                ..
            }) => Some((&envelope.owner, change)),
            _ => None,
        })
        .unwrap();
    assert_eq!(change_owner, &owner);
    assert_eq!(&change.owner, &owner);
    assert_eq!(change.path, "generated.txt");
    assert_eq!(
        fs::read_to_string(cwd.join("generated.txt")).unwrap(),
        "generated\n"
    );
    let file_order = change_events
        .iter()
        .filter(|envelope| envelope.owner == owner)
        .filter_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ToolCallStarted { .. },
                ..
            }) => Some("started"),
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ToolCallFinished { .. },
                ..
            }) => Some("finished"),
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::WorkspaceChangeUpdated { .. },
                ..
            }) => Some("change"),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        file_order,
        vec!["started", "finished", "change"],
        "structured facts must follow their tool lifecycle in the ordered journal"
    );

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd-production-check",
            RuntimeCommand::SubmitUserInput {
                content: "run a check".to_string(),
            },
        )
        .unwrap();
    let check_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CheckRunUpdated { .. },
                    ..
                })
            )
        })
    });
    let (check_owner, check) = check_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::CheckRunUpdated { check },
                ..
            }) => Some((&envelope.owner, check)),
            _ => None,
        })
        .unwrap();
    assert_eq!(check_owner, &owner);
    assert_eq!(&check.owner, &owner);
    assert_eq!(check.command, "cargo check --help");
    assert_eq!(check.status, viden_types::CheckRunStatus::Passed);
    let check_order = check_events
        .iter()
        .filter(|envelope| envelope.owner == owner)
        .filter_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ToolCallStarted { .. },
                ..
            }) => Some("started"),
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ToolCallFinished { .. },
                ..
            }) => Some("finished"),
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::CheckRunUpdated { .. },
                ..
            }) => Some("check"),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(check_order, vec!["started", "finished", "check"]);
}

#[test]
fn frontend_status_later_provider_failure_keeps_ordered_tool_and_change_facts() {
    struct ToolThenFailProvider {
        call: ToolCall,
        request_count: usize,
    }

    impl viden_provider::ModelProvider for ToolThenFailProvider {
        fn provider_name(&self) -> &str {
            "tool-then-fail"
        }

        fn model(&self) -> &str {
            "test-model"
        }

        fn set_model(&mut self, _model: String) {}

        fn next_events(
            &mut self,
            _request: &viden_types::ModelRequest,
        ) -> Result<Vec<viden_types::ModelEvent>, String> {
            self.request_count += 1;
            if self.request_count == 1 {
                Ok(vec![viden_types::ModelEvent::ToolCall(self.call.clone())])
            } else {
                Err("provider failed after completed tool result".to_string())
            }
        }
    }

    let cwd = temp_dir("frontend_status_tool_then_provider_failure");
    let home = temp_dir("frontend_status_tool_then_provider_failure_home");
    let mut input = viden_types::ToolInput::new();
    input.insert("path".to_string(), "survives-failure.txt".to_string());
    input.insert("content".to_string(), "durable result\n".to_string());
    let provider = Box::new(ToolThenFailProvider {
        call: ToolCall {
            id: "tool-before-provider-failure".to_string(),
            name: "write_file".to_string(),
            input,
        },
        request_count: 0,
    });
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let supervisor = RuntimeSupervisor::start(engine);
    let owner = RuntimeOwner {
        workspace_id: "workspace-tool-failure".to_string(),
        project_id: "project-tool-failure".to_string(),
        lane_id: Some("lane-tool-failure".to_string()),
        session_id: Some("session-tool-failure".to_string()),
        ..RuntimeOwner::default()
    };

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd-tool-then-provider-failure",
            RuntimeCommand::SubmitUserInput {
                content: "write, then encounter provider failure".to_string(),
            },
        )
        .unwrap();
    let events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::Error { error },
                    ..
                }) if error.message.contains("provider failed after completed tool result")
            )
        })
    });
    let order = events
        .iter()
        .filter(|envelope| envelope.owner == owner)
        .filter_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ToolCallStarted { .. },
                ..
            }) => Some("started"),
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ToolCallFinished { .. },
                ..
            }) => Some("finished"),
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::WorkspaceChangeUpdated { .. },
                ..
            }) => Some("change"),
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::Error { .. },
                ..
            }) => Some("error"),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(order, vec!["started", "finished", "change", "error"]);
    assert_eq!(
        fs::read_to_string(cwd.join("survives-failure.txt")).unwrap(),
        "durable result\n"
    );
}

#[test]
fn frontend_status_unknown_second_tool_keeps_completed_first_tool_prefix() {
    let cwd = temp_dir("frontend_status_unknown_second_tool");
    let home = temp_dir("frontend_status_unknown_second_tool_home");
    let provider = Box::new(SequenceProvider::new(vec![
        completed_write_then_unknown_tool("unknown-second-tool.txt"),
    ]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let supervisor = RuntimeSupervisor::start(engine);
    let owner = cockpit_test_owner("unknown-second-tool");

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd-unknown-second-tool",
            RuntimeCommand::SubmitUserInput {
                content: "complete one tool, then reject an unknown tool".to_string(),
            },
        )
        .unwrap();
    let events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::Error { error },
                    ..
                }) if error.message.contains("unknown tool")
            )
        })
    });

    assert_eq!(
        completed_tool_prefix_order(&events, &owner),
        vec!["started", "finished", "change", "error"]
    );
    assert_eq!(
        fs::read_to_string(cwd.join("unknown-second-tool.txt")).unwrap(),
        "completed before later failure\n"
    );
    assert_failed_turn_is_terminal(&events, &owner, 2);
    let view = reduce_owner_events(&events, &owner);
    let completed_tool_id = "tool-write-unknown-second-tool.txt";
    assert_eq!(
        view.tasks
            .iter()
            .find(|task| task.id == completed_tool_id)
            .map(|task| task.status),
        Some(AgentTaskStatus::Done),
        "the completed tool must retain its terminal outcome"
    );
    assert_eq!(
        events
            .iter()
            .filter(|envelope| {
                envelope.owner == owner
                    && matches!(
                        &envelope.event,
                        RuntimeWireEvent::Known(RuntimeEvent {
                            kind: RuntimeEventKind::TaskUpdated { task },
                            ..
                        }) if task.id == completed_tool_id
                            && matches!(
                                task.status,
                                AgentTaskStatus::Done
                                    | AgentTaskStatus::Failed
                                    | AgentTaskStatus::Cancelled
                            )
                    )
            })
            .count(),
        1,
        "the failure finalizer must not emit a duplicate terminal update"
    );
}

#[test]
fn frontend_status_start_agent_task_unknown_second_tool_keeps_completed_prefix() {
    let cwd = temp_dir("frontend_status_agent_task_unknown_second_tool");
    let home = temp_dir("frontend_status_agent_task_unknown_second_tool_home");
    let provider = Box::new(SequenceProvider::new(vec![
        completed_write_then_unknown_tool("agent-task-prefix.txt"),
    ]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::BypassPermissions)
        .unwrap();
    let supervisor = RuntimeSupervisor::start(engine);
    let owner = cockpit_test_owner("agent-task-prefix");
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd-agent-task-prefix-dag",
            RuntimeCommand::StartAgentDag {
                goal: "preserve completed tool facts".to_string(),
                tasks: vec![AgentDagTaskSpec {
                    task_id: "task-agent-prefix".to_string(),
                    role: AgentRole::Coder,
                    title: "Exercise ordered prefix".to_string(),
                    objective: "Complete one tool before a later tool error".to_string(),
                    dependencies: Vec::new(),
                    workspace: None,
                    file_scope: Vec::new(),
                    context_bundle_id: None,
                    required_evidence: Vec::new(),
                    permission_policy: "full_access".to_string(),
                }],
            },
        )
        .unwrap();
    let _ = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::AgentDagUpdated { .. },
                    ..
                })
            )
        })
    });

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd-agent-task-prefix-start",
            RuntimeCommand::StartAgentTask {
                task_id: "task-agent-prefix".to_string(),
            },
        )
        .unwrap();
    let events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::Error { error },
                    ..
                }) if error.message.contains("unknown tool")
            )
        })
    });

    assert_eq!(
        completed_tool_prefix_order(&events, &owner),
        vec!["started", "finished", "change", "error"]
    );
    assert_eq!(
        fs::read_to_string(cwd.join("agent-task-prefix.txt")).unwrap(),
        "completed before later failure\n"
    );
}

#[test]
fn frontend_status_tool_result_persistence_failure_keeps_completed_prefix() {
    let cwd = temp_dir("frontend_status_tool_persistence_failure");
    let home = temp_dir("frontend_status_tool_persistence_failure_home");
    let mut input = viden_types::ToolInput::new();
    input.insert("path".to_string(), "persist-prefix.txt".to_string());
    input.insert(
        "content".to_string(),
        "tool completed before persistence failed\n".to_string(),
    );
    let provider = Box::new(SequenceProvider::new(vec![vec![
        viden_types::ModelEvent::ToolCall(ToolCall {
            id: "tool-before-persistence-failure".to_string(),
            name: "write_file".to_string(),
            input,
        }),
    ]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::BypassPermissions)
        .unwrap();
    // User message, provider cost, tool call, assistant tool message, and
    // permission decision persist before the completed ToolResult append.
    engine.fail_after_transcript_appends_for_test(5);
    let supervisor = RuntimeSupervisor::start(engine);
    let owner = cockpit_test_owner("tool-persistence-failure");

    start_approval_command(
        &supervisor,
        &owner,
        ApprovalCommandPath::StartAgentTask,
        "tool-persistence-failure",
    );
    let events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::Error { error },
                    ..
                }) if error.message.contains("injected transcript append failure")
            )
        })
    });

    assert_eq!(
        completed_tool_prefix_order(&events, &owner),
        vec!["started", "finished", "change", "error"]
    );
    assert_eq!(
        fs::read_to_string(cwd.join("persist-prefix.txt")).unwrap(),
        "tool completed before persistence failed\n"
    );
    assert_failed_turn_is_terminal(&events, &owner, 2);
}

#[test]
fn frontend_status_assistant_tool_transcript_failure_terminalizes_turn_tasks() {
    let cwd = temp_dir("frontend_status_assistant_tool_transcript_failure");
    let home = temp_dir("frontend_status_assistant_tool_transcript_failure_home");
    let mut input = viden_types::ToolInput::new();
    input.insert("path".to_string(), "must-not-exist.txt".to_string());
    input.insert("content".to_string(), "not persisted\n".to_string());
    let provider = Box::new(SequenceProvider::new(vec![vec![
        viden_types::ModelEvent::ToolCall(ToolCall {
            id: "tool-assistant-transcript-failure".to_string(),
            name: "write_file".to_string(),
            input,
        }),
    ]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    // User message, provider cost, and ToolCall persist before the assistant
    // tool-call message append fails with the tool task in RunningTool.
    engine.fail_after_transcript_appends_for_test(3);
    let supervisor = RuntimeSupervisor::start(engine);
    let owner = cockpit_test_owner("assistant-tool-transcript-failure");

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd-assistant-tool-transcript-failure",
            RuntimeCommand::SubmitUserInput {
                content: "fail while recording the assistant tool call".to_string(),
            },
        )
        .unwrap();
    let events = collect_injected_transcript_failure(&supervisor);

    assert_eq!(completed_tool_prefix_order(&events, &owner), vec!["error"]);
    assert_failed_turn_is_terminal(&events, &owner, 2);
    assert!(!cwd.join("must-not-exist.txt").exists());
}

#[test]
fn frontend_status_cost_persistence_failure_terminalizes_thinking_provider() {
    let cwd = temp_dir("frontend_status_cost_persistence_failure");
    let home = temp_dir("frontend_status_cost_persistence_failure_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![
        viden_types::ModelEvent::AssistantText {
            content: "must not be published".to_string(),
        },
        viden_types::ModelEvent::Done,
    ]]));
    let engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    // The user message persists, then provider cost persistence fails while
    // the provider task is Thinking.
    engine.fail_after_transcript_appends_for_test(1);
    let supervisor = RuntimeSupervisor::start(engine);
    let owner = cockpit_test_owner("cost-persistence-failure");

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd-cost-persistence-failure",
            RuntimeCommand::SubmitUserInput {
                content: "fail while recording provider cost".to_string(),
            },
        )
        .unwrap();
    let events = collect_injected_transcript_failure(&supervisor);

    assert_failed_turn_is_terminal(&events, &owner, 1);
}

#[test]
fn frontend_status_text_persistence_failure_terminalizes_streaming_provider() {
    let cwd = temp_dir("frontend_status_text_persistence_failure");
    let home = temp_dir("frontend_status_text_persistence_failure_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![
        viden_types::ModelEvent::AssistantText {
            content: "must not be published".to_string(),
        },
        viden_types::ModelEvent::Done,
    ]]));
    let engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    // User message and provider cost persist before assistant text persistence
    // fails with the provider task in Streaming.
    engine.fail_after_transcript_appends_for_test(2);
    let supervisor = RuntimeSupervisor::start(engine);
    let owner = cockpit_test_owner("text-persistence-failure");

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd-text-persistence-failure",
            RuntimeCommand::SubmitUserInput {
                content: "fail while recording assistant text".to_string(),
            },
        )
        .unwrap();
    let events = collect_injected_transcript_failure(&supervisor);

    assert_failed_turn_is_terminal(&events, &owner, 1);
}

#[test]
fn frontend_status_turn_failure_does_not_terminalize_unrelated_active_task() {
    let cwd = temp_dir("frontend_status_turn_failure_task_scope");
    let home = temp_dir("frontend_status_turn_failure_task_scope_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![
        viden_types::ModelEvent::Done,
    ]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut unrelated = engine.provider_task(
        "unrelated concurrent work",
        AgentTaskStatus::Running,
        "running independently",
        50,
    );
    unrelated.id = "unrelated-active-provider".to_string();
    engine.upsert_agent_task(unrelated);
    engine.fail_after_transcript_appends_for_test(1);
    let supervisor = RuntimeSupervisor::start(engine);
    let owner = cockpit_test_owner("turn-failure-task-scope");

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd-turn-failure-task-scope",
            RuntimeCommand::SubmitUserInput {
                content: "fail without sweeping unrelated work".to_string(),
            },
        )
        .unwrap();
    let events = collect_injected_transcript_failure(&supervisor);
    let reduced = reduce_owner_events(&events, &owner);

    assert!(
        reduced
            .tasks
            .iter()
            .any(|task| task.id == "unrelated-active-provider" && task.is_active())
    );
    assert!(
        reduced.tasks.iter().any(|task| {
            task.id != "unrelated-active-provider"
                && task.kind == AgentTaskKind::Provider
                && task.status == AgentTaskStatus::Failed
        }),
        "the failed turn's provider task must still be terminalized"
    );
}

#[test]
fn frontend_status_submit_input_emits_one_live_approval_pair_once() {
    assert_supervised_approval_order(ApprovalCommandPath::SubmitUserInput, 1);
}

#[test]
fn frontend_status_submit_input_preserves_two_live_approval_boundaries() {
    assert_supervised_approval_order(ApprovalCommandPath::SubmitUserInput, 2);
}

#[test]
fn frontend_status_start_agent_task_emits_one_live_approval_pair_once() {
    assert_supervised_approval_order(ApprovalCommandPath::StartAgentTask, 1);
}

#[test]
fn frontend_status_start_agent_task_preserves_two_live_approval_boundaries() {
    assert_supervised_approval_order(ApprovalCommandPath::StartAgentTask, 2);
}

#[test]
fn frontend_status_permission_persistence_failure_has_no_orphan_tool_call() {
    let cwd = temp_dir("frontend_status_permission_persistence_failure");
    let home = temp_dir("frontend_status_permission_persistence_failure_home");
    let provider = approval_sequence_provider("permission-persistence", 1);
    let engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    // User message, provider cost, tool call, and assistant tool message
    // persist before the permission decision append.
    engine.fail_after_transcript_appends_for_test(4);
    let supervisor = RuntimeSupervisor::start(engine);
    let owner = cockpit_test_owner("permission-persistence");

    start_approval_command(
        &supervisor,
        &owner,
        ApprovalCommandPath::SubmitUserInput,
        "permission-persistence",
    );
    let mut events = collect_starter_envelopes_until(&supervisor, |events| {
        approval_request_id(events).is_some()
    });
    let request_id = approval_request_id(&events).unwrap();
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "approve-permission-persistence",
            RuntimeCommand::RespondToApproval {
                request_id,
                response: ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    events.extend(collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::Error { error },
                    ..
                }) if error.message.contains("injected transcript append failure")
            )
        })
    }));

    assert_eq!(
        approval_tool_order(&events, &owner),
        vec!["requested", "resolved", "error"]
    );
    assert_eq!(approval_event_counts(&events, &owner), (1, 1));
    assert_failed_turn_is_terminal(&events, &owner, 2);
    assert!(!cwd.join("permission-persistence-1.txt").exists());
}

#[derive(Debug, Clone, Copy)]
enum ApprovalCommandPath {
    SubmitUserInput,
    StartAgentTask,
}

fn assert_supervised_approval_order(path: ApprovalCommandPath, tool_count: usize) {
    let test_id = format!(
        "{}-{tool_count}",
        match path {
            ApprovalCommandPath::SubmitUserInput => "submit-approval",
            ApprovalCommandPath::StartAgentTask => "agent-approval",
        }
    );
    let cwd = temp_dir(&test_id);
    let home = temp_dir(&format!("{test_id}-home"));
    let provider = approval_sequence_provider(&test_id, tool_count);
    let engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let supervisor = RuntimeSupervisor::start(engine);
    let owner = cockpit_test_owner(&test_id);

    start_approval_command(&supervisor, &owner, path, &test_id);
    let mut events = collect_starter_envelopes_until(&supervisor, |events| {
        approval_request_id(events).is_some()
    });
    let mut request_id = approval_request_id(&events).unwrap();
    for approval_index in 0..tool_count {
        supervisor
            .send_command_from_owner(
                owner.clone(),
                format!("approve-{test_id}-{approval_index}"),
                RuntimeCommand::RespondToApproval {
                    request_id: request_id.clone(),
                    response: ApprovalResponse::allow_once(None),
                },
            )
            .unwrap();
        let final_approval = approval_index + 1 == tool_count;
        let next = collect_starter_envelopes_until(&supervisor, |batch| {
            if final_approval {
                tool_finished_count(batch, &owner) == 1
            } else {
                approval_request_id(batch).is_some()
            }
        });
        if !final_approval {
            request_id = approval_request_id(&next).unwrap();
        }
        events.extend(next);
    }

    let mut expected = Vec::with_capacity(tool_count * 4);
    for _ in 0..tool_count {
        expected.extend(["requested", "resolved", "started", "finished"]);
    }
    assert_eq!(approval_tool_order(&events, &owner), expected);
    assert_eq!(
        approval_event_counts(&events, &owner),
        (tool_count, tool_count)
    );
    assert!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .active_tool_calls
            .is_empty()
    );
}

fn approval_sequence_provider(test_id: &str, tool_count: usize) -> Box<dyn ModelProvider> {
    let tool_calls = (1..=tool_count)
        .map(|index| {
            let mut input = viden_types::ToolInput::new();
            input.insert("path".to_string(), format!("{test_id}-{index}.txt"));
            input.insert("content".to_string(), format!("approved tool {index}\n"));
            viden_types::ModelEvent::ToolCall(ToolCall {
                id: format!("tool-{test_id}-{index}"),
                name: "write_file".to_string(),
                input,
            })
        })
        .collect::<Vec<_>>();
    Box::new(SequenceProvider::new(vec![
        tool_calls,
        vec![viden_types::ModelEvent::Done],
    ]))
}

fn start_approval_command(
    supervisor: &RuntimeSupervisor,
    owner: &RuntimeOwner,
    path: ApprovalCommandPath,
    test_id: &str,
) {
    match path {
        ApprovalCommandPath::SubmitUserInput => supervisor
            .send_command_from_owner(
                owner.clone(),
                format!("submit-{test_id}"),
                RuntimeCommand::SubmitUserInput {
                    content: "run approval-gated tools".to_string(),
                },
            )
            .unwrap(),
        ApprovalCommandPath::StartAgentTask => {
            let task_id = format!("task-{test_id}");
            supervisor
                .send_command_from_owner(
                    owner.clone(),
                    format!("dag-{test_id}"),
                    RuntimeCommand::StartAgentDag {
                        goal: "run approval-gated tools".to_string(),
                        tasks: vec![AgentDagTaskSpec {
                            task_id: task_id.clone(),
                            role: AgentRole::Coder,
                            title: "Approval ordering".to_string(),
                            objective: "Run approval-gated tools in order".to_string(),
                            dependencies: Vec::new(),
                            workspace: None,
                            file_scope: Vec::new(),
                            context_bundle_id: None,
                            required_evidence: Vec::new(),
                            permission_policy: "full_access".to_string(),
                        }],
                    },
                )
                .unwrap();
            let _ = collect_starter_envelopes_until(supervisor, |events| {
                events.iter().any(|envelope| {
                    matches!(
                        &envelope.event,
                        RuntimeWireEvent::Known(RuntimeEvent {
                            kind: RuntimeEventKind::AgentDagUpdated { .. },
                            ..
                        })
                    )
                })
            });
            supervisor
                .send_command_from_owner(
                    owner.clone(),
                    format!("start-{test_id}"),
                    RuntimeCommand::StartAgentTask { task_id },
                )
                .unwrap();
        }
    }
}

fn collect_injected_transcript_failure(
    supervisor: &RuntimeSupervisor,
) -> Vec<RuntimeEventEnvelope> {
    collect_starter_envelopes_until(supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::Error { error },
                    ..
                }) if error.message.contains("injected transcript append failure")
            )
        })
    })
}

fn assert_failed_turn_is_terminal(
    events: &[RuntimeEventEnvelope],
    owner: &RuntimeOwner,
    expected_terminal_task_count: usize,
) {
    let error_index = events
        .iter()
        .position(|envelope| {
            &envelope.owner == owner
                && matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::Error { .. },
                        ..
                    })
                )
        })
        .expect("turn failure emits Error");
    let terminal_task_indices = events
        .iter()
        .enumerate()
        .filter_map(|(index, envelope)| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::TaskUpdated { task },
                ..
            }) if &envelope.owner == owner
                && matches!(
                    task.kind,
                    AgentTaskKind::Provider | AgentTaskKind::Tool | AgentTaskKind::Shell
                )
                && !task.is_active() =>
            {
                Some(index)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        terminal_task_indices.len() >= expected_terminal_task_count
            && terminal_task_indices
                .iter()
                .all(|index| *index < error_index),
        "all turn-owned provider/tool terminal facts must precede Error"
    );

    let reduced = reduce_owner_events(events, owner);
    assert!(
        reduced.active_tool_calls.is_empty(),
        "failed turn must not leave an active tool call"
    );
    assert!(
        reduced.tasks.iter().all(|task| {
            !matches!(
                task.kind,
                AgentTaskKind::Provider | AgentTaskKind::Tool | AgentTaskKind::Shell
            ) || !task.is_active()
        }),
        "failed turn must not leave an active provider/tool task"
    );
}

fn reduce_owner_events(events: &[RuntimeEventEnvelope], owner: &RuntimeOwner) -> RuntimeViewState {
    let snapshot = events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::SnapshotUpdated { snapshot },
                ..
            }) if &envelope.owner == owner => Some(snapshot.clone()),
            _ => None,
        })
        .expect("failed turn emits a runtime snapshot");
    let mut reduced = RuntimeViewState::new(snapshot);
    for envelope in events.iter().filter(|envelope| &envelope.owner == owner) {
        if let RuntimeWireEvent::Known(event) = &envelope.event {
            reduced.apply_event(event);
        }
    }
    reduced
}

fn approval_request_id(events: &[RuntimeEventEnvelope]) -> Option<String> {
    events.iter().find_map(|envelope| match &envelope.event {
        RuntimeWireEvent::Known(RuntimeEvent {
            kind: RuntimeEventKind::ApprovalRequested { approval },
            ..
        }) => Some(approval.id.clone()),
        _ => None,
    })
}

fn approval_event_counts(events: &[RuntimeEventEnvelope], owner: &RuntimeOwner) -> (usize, usize) {
    events
        .iter()
        .filter(|envelope| &envelope.owner == owner)
        .fold((0, 0), |(requested, resolved), envelope| {
            match &envelope.event {
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { .. },
                    ..
                }) => (requested + 1, resolved),
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalResolved { .. },
                    ..
                }) => (requested, resolved + 1),
                _ => (requested, resolved),
            }
        })
}

fn tool_finished_count(events: &[RuntimeEventEnvelope], owner: &RuntimeOwner) -> usize {
    events
        .iter()
        .filter(|envelope| &envelope.owner == owner)
        .filter(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ToolCallFinished { .. },
                    ..
                })
            )
        })
        .count()
}

fn approval_tool_order(events: &[RuntimeEventEnvelope], owner: &RuntimeOwner) -> Vec<&'static str> {
    events
        .iter()
        .filter(|envelope| &envelope.owner == owner)
        .filter_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { .. },
                ..
            }) => Some("requested"),
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalResolved { .. },
                ..
            }) => Some("resolved"),
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ToolCallStarted { .. },
                ..
            }) => Some("started"),
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ToolCallFinished { .. },
                ..
            }) => Some("finished"),
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::Error { .. },
                ..
            }) => Some("error"),
            _ => None,
        })
        .collect()
}

fn completed_write_then_unknown_tool(path: &str) -> Vec<viden_types::ModelEvent> {
    let mut write_input = viden_types::ToolInput::new();
    write_input.insert("path".to_string(), path.to_string());
    write_input.insert(
        "content".to_string(),
        "completed before later failure\n".to_string(),
    );
    vec![
        viden_types::ModelEvent::ToolCall(ToolCall {
            id: format!("write-{path}"),
            name: "write_file".to_string(),
            input: write_input,
        }),
        viden_types::ModelEvent::ToolCall(ToolCall {
            id: format!("unknown-{path}"),
            name: "unknown_tool".to_string(),
            input: viden_types::ToolInput::new(),
        }),
    ]
}

fn cockpit_test_owner(id: &str) -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: format!("workspace-{id}"),
        project_id: format!("project-{id}"),
        lane_id: Some(format!("lane-{id}")),
        session_id: Some(format!("session-{id}")),
        ..RuntimeOwner::default()
    }
}

fn completed_tool_prefix_order(
    events: &[RuntimeEventEnvelope],
    owner: &RuntimeOwner,
) -> Vec<&'static str> {
    events
        .iter()
        .filter(|envelope| &envelope.owner == owner)
        .filter_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ToolCallStarted { .. },
                ..
            }) => Some("started"),
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ToolCallFinished { .. },
                ..
            }) => Some("finished"),
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::WorkspaceChangeUpdated { .. },
                ..
            }) => Some("change"),
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::Error { .. },
                ..
            }) => Some("error"),
            _ => None,
        })
        .collect()
}

fn append_recent_runtime_fixture(home: &Path, root: &Path, session_id: &str, timestamp: u64) {
    let root = root.canonicalize().unwrap();
    let store =
        viden_session::SessionStore::new_with_home(home, &root, Some(session_id.to_string()))
            .unwrap();
    store
        .append_entries_atomic(&[
            TranscriptEntry::SessionMeta {
                entry: SessionMetaEntry {
                    timestamp,
                    key: "canonical_root".to_string(),
                    value: root.display().to_string(),
                },
            },
            TranscriptEntry::SessionMeta {
                entry: SessionMetaEntry {
                    timestamp,
                    key: "session_created_at".to_string(),
                    value: timestamp.to_string(),
                },
            },
            TranscriptEntry::Message {
                message: Message {
                    id: format!("message-{session_id}"),
                    role: Role::User,
                    content: "sk-secret-runtime-body".to_string(),
                    timestamp: timestamp + 1,
                    tool_name: None,
                    tool_call_id: None,
                },
            },
        ])
        .unwrap();
}

#[derive(Default)]
struct StarterLaneEffects {
    calls: Mutex<Vec<String>>,
}

impl LaneEffectExecutor for StarterLaneEffects {
    fn execute(&self, request: LaneEffectRequest) -> Result<LaneEffectResult, String> {
        let LaneEffectRequest::Create { lane, .. } = request else {
            panic!("starter lane test received a non-create effect");
        };
        self.calls.lock().unwrap().push(lane.id);
        Ok(LaneEffectResult::success("starter lane created"))
    }
}

impl StarterLaneEffects {
    /// The Lane ids whose create effect actually ran.
    fn created_lane_ids(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

fn starter_lane_repo(name: &str) -> PathBuf {
    let repo = temp_dir(name);
    run_git(&repo, &["init", "-b", "main"]);
    run_git(&repo, &["config", "user.email", "viden@example.com"]);
    run_git(&repo, &["config", "user.name", "Viden Test"]);
    fs::write(repo.join("README.md"), "starter\n").unwrap();
    run_git(&repo, &["add", "README.md"]);
    run_git(&repo, &["commit", "-m", "initial"]);
    repo
}

fn run_git(repo: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn starter_request(lane_id: &str, preset: StarterLanePreset) -> StarterLaneRequest {
    StarterLaneRequest {
        lane_id: lane_id.to_string(),
        preset,
        branch: None,
        worktree_path: None,
        agent: None,
    }
}

fn starter_owner(lane_id: &str) -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace-starter".to_string(),
        project_id: "project-starter".to_string(),
        lane_id: Some(lane_id.to_string()),
        session_id: Some(format!("session-{lane_id}")),
        task_id: None,
        turn_id: Some(format!("turn-{lane_id}")),
    }
}

fn starter_supervisor(
    repo: &Path,
    home: PathBuf,
    effects: Arc<StarterLaneEffects>,
) -> RuntimeSupervisor {
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(repo, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects as Arc<dyn LaneEffectExecutor>,
    )
}

fn collect_starter_envelopes_until(
    supervisor: &RuntimeSupervisor,
    done: impl Fn(&[RuntimeEventEnvelope]) -> bool,
) -> Vec<RuntimeEventEnvelope> {
    let started = Instant::now();
    let mut events = Vec::new();
    while started.elapsed() < Duration::from_secs(10) {
        if let Some(event) = supervisor.recv_event_envelope_timeout(Duration::from_millis(50)) {
            events.push(event);
            if done(&events) {
                return events;
            }
        }
    }
    panic!("timed out waiting for starter lane events: {events:#?}");
}

#[test]
fn starter_lane_preview_resolves_presets_without_git_workflow_or_effect_mutation() {
    let repo = starter_lane_repo("starter_lane_preview_repo");
    let home = temp_dir("starter_lane_preview_home");
    let effects = Arc::new(StarterLaneEffects::default());
    let supervisor = starter_supervisor(&repo, home.clone(), Arc::clone(&effects));
    let base_revision = run_git(&repo, &["rev-parse", "HEAD"]);

    for (preset, role, lane_id) in [
        (StarterLanePreset::Coder, AgentRole::Coder, "starter-coder"),
        (
            StarterLanePreset::Reviewer,
            AgentRole::Reviewer,
            "starter-reviewer",
        ),
        (
            StarterLanePreset::Tester,
            AgentRole::Tester,
            "starter-tester",
        ),
    ] {
        let owner = starter_owner(lane_id);
        supervisor
            .send_command_from_owner(
                owner.clone(),
                format!("preview-{lane_id}"),
                RuntimeCommand::PreviewStarterLane {
                    request: starter_request(lane_id, preset),
                },
            )
            .unwrap();
        let events = collect_starter_envelopes_until(&supervisor, |events| {
            events.iter().any(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::StarterLanePreviewed { preview },
                        ..
                    }) if preview.lane.id == lane_id
                )
            })
        });
        let (event_owner, preview) = events
            .iter()
            .find_map(|envelope| match &envelope.event {
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewed { preview },
                    ..
                }) if preview.lane.id == lane_id => Some((&envelope.owner, preview)),
                _ => None,
            })
            .unwrap();
        assert_eq!(event_owner, &owner);
        assert_eq!(&preview.owner, &owner);
        assert_eq!(preview.lane.role, role);
        assert_eq!(preview.branch, format!("codex/{lane_id}"));
        assert_eq!(
            preview.worktree_path,
            repo.canonicalize()
                .unwrap()
                .join(".worktrees")
                .join(lane_id)
                .display()
                .to_string()
        );
        assert_eq!(preview.base_revision, base_revision);
        assert_eq!(preview.content_sha256.len(), 64);
        assert!(!preview.preview_id.is_empty());
        assert!(preview.diagnostics.is_empty());
        assert!(!Path::new(&preview.worktree_path).exists());
    }

    assert!(effects.calls.lock().unwrap().is_empty());
    assert!(
        viden_workflows::stores::WorkflowStore::new(home, &repo)
            .unwrap()
            .load_lane_events()
            .unwrap()
            .is_empty()
    );
    assert_eq!(run_git(&repo, &["branch", "--list", "codex/*"]), "");
}

#[test]
fn default_starter_lane_preview_generates_unique_core_owned_identity() {
    let repo = starter_lane_repo("default_starter_lane_preview_repo");
    let home = temp_dir("default_starter_lane_preview_home");
    let effects = Arc::new(StarterLaneEffects::default());
    let supervisor = starter_supervisor(&repo, home, effects);
    let owner = RuntimeOwner {
        workspace_id: "workspace-starter".to_string(),
        project_id: "project-starter".to_string(),
        ..RuntimeOwner::default()
    };

    let mut lane_ids = Vec::new();
    for command_id in ["default-preview-1", "default-preview-2"] {
        supervisor
            .send_command_from_owner(
                owner.clone(),
                command_id,
                RuntimeCommand::PreviewDefaultStarterLane {
                    preset: StarterLanePreset::Coder,
                },
            )
            .unwrap();
        let events = collect_starter_envelopes_until(&supervisor, |events| {
            events.iter().any(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::StarterLanePreviewed { .. },
                        ..
                    })
                )
            })
        });
        let preview = events
            .iter()
            .find_map(|envelope| match &envelope.event {
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewed { preview },
                    ..
                }) => Some(preview),
                _ => None,
            })
            .unwrap();
        assert!(preview.lane.id.starts_with("lane_"));
        assert_eq!(preview.owner.lane_id.as_ref(), Some(&preview.lane.id));
        assert_eq!(preview.branch, format!("viden/{}", preview.lane.id));
        lane_ids.push(preview.lane.id.clone());
    }
    assert_ne!(lane_ids[0], lane_ids[1]);
}

#[test]
fn default_starter_lane_creates_in_non_git_workspace_without_worktree() {
    let repo = temp_dir("default_starter_lane_non_git_repo");
    let home = temp_dir("default_starter_lane_non_git_home");
    let effects = Arc::new(StarterLaneEffects::default());
    let supervisor = starter_supervisor(&repo, home, Arc::clone(&effects));
    let workspace_owner = RuntimeOwner {
        workspace_id: "workspace-starter".to_string(),
        project_id: "project-starter".to_string(),
        ..RuntimeOwner::default()
    };

    supervisor
        .send_command_from_owner(
            workspace_owner,
            "default-preview-non-git",
            RuntimeCommand::PreviewDefaultStarterLane {
                preset: StarterLanePreset::Coder,
            },
        )
        .unwrap();
    let preview_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewed { .. },
                    ..
                })
            )
        })
    });
    let eligibility = preview_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::WorkspaceEligibilityUpdated { eligibility },
                ..
            }) => Some(eligibility),
            _ => None,
        })
        .unwrap();
    assert!(!eligibility.is_git_repository);
    assert!(!eligibility.has_head);
    assert!(eligibility.can_create_lane);
    assert_eq!(eligibility.diagnostic, None);
    let preview = preview_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewed { preview },
                ..
            }) => Some(preview.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(preview.lane.branch, None);
    assert_eq!(preview.lane.worktree, None);
    assert_eq!(preview.branch, "");
    assert_eq!(
        preview.worktree_path,
        repo.canonicalize().unwrap().display().to_string()
    );
    assert!(preview.base_revision.starts_with("workspace:"));
    assert!(preview.diagnostics.is_empty());

    let request = StarterLaneRequest {
        lane_id: preview.lane.id.clone(),
        preset: StarterLanePreset::Coder,
        branch: None,
        worktree_path: None,
        agent: None,
    };
    supervisor
        .send_command_from_owner(
            preview.owner.clone(),
            "default-create-non-git",
            RuntimeCommand::CreateStarterLane {
                request,
                preview_id: preview.preview_id.clone(),
                content_sha256: preview.content_sha256.clone(),
            },
        )
        .unwrap();
    let approval_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { approval },
                    ..
                }) if approval.tool_name == "lane_create"
            )
        })
    });
    let approval_id = approval_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(approval.id.clone()),
            _ => None,
        })
        .unwrap();
    supervisor
        .send_command_from_owner(
            preview.owner.clone(),
            "default-approve-non-git",
            RuntimeCommand::RespondToApproval {
                request_id: approval_id,
                response: ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    let created_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLaneCreated { .. },
                    ..
                })
            )
        })
    });
    let receipt = created_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLaneCreated { receipt },
                ..
            }) => Some(receipt),
            _ => None,
        })
        .unwrap();
    assert_eq!(receipt.lane.branch, None);
    assert_eq!(receipt.lane.worktree, None);
    assert_eq!(receipt.worktree_path, preview.worktree_path);
    assert_eq!(effects.calls.lock().unwrap().as_slice(), [preview.lane.id]);
}

/// The exact native GUI creation path (E3b defect 22): the `⌘L` popover's
/// `PreviewDefaultStarterLane` in a real Git workspace, the real
/// `LocalLaneEffectExecutor`, and a `lane_create` approval decided with
/// allow-once. The receipt the host's pending creation settles on must arrive
/// after the Lane's own `LaneUpdated`, and no invalidation may precede it.
#[test]
fn default_starter_lane_in_real_git_workspace_settles_its_creation_receipt() {
    let repo = starter_lane_repo("default_starter_lane_real_git_repo");
    let home = temp_dir("default_starter_lane_real_git_home");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let engine = SessionEngine::new_with_home(&repo, provider, Some(home)).unwrap();
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        Arc::new(LocalLaneEffectExecutor::default()) as Arc<dyn LaneEffectExecutor>,
    );
    let workspace_owner = RuntimeOwner {
        workspace_id: "workspace-starter".to_string(),
        project_id: "project-starter".to_string(),
        ..RuntimeOwner::default()
    };

    supervisor
        .send_command_from_owner(
            workspace_owner,
            "gui-d1-preview-default",
            RuntimeCommand::PreviewDefaultStarterLane {
                preset: StarterLanePreset::Coder,
            },
        )
        .unwrap();
    let preview_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewed { .. },
                    ..
                })
            )
        })
    });
    let preview = preview_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewed { preview },
                ..
            }) => Some(preview.clone()),
            _ => None,
        })
        .unwrap();
    assert!(
        preview.lane.branch.is_some(),
        "real Git workspace previews a branch"
    );

    // Byte-identical to what `GuiCoreAdapter` rebuilds from the published
    // projection before it sends `CreateStarterLane` (adapter.rs:4726).
    let request = StarterLaneRequest {
        lane_id: preview.lane.id.clone(),
        preset: StarterLanePreset::Coder,
        branch: preview.lane.branch.clone(),
        worktree_path: None,
        agent: None,
    };
    supervisor
        .send_command_from_owner(
            preview.owner.clone(),
            "gui-d1-create-default",
            RuntimeCommand::CreateStarterLane {
                request,
                preview_id: preview.preview_id.clone(),
                content_sha256: preview.content_sha256.clone(),
            },
        )
        .unwrap();
    let approval_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { approval },
                    ..
                }) if approval.tool_name == "lane_create"
            )
        })
    });
    let approval_id = approval_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(approval.id.clone()),
            _ => None,
        })
        .unwrap();
    supervisor
        .send_command_from_owner(
            preview.owner.clone(),
            "gui-d1-approve-default",
            RuntimeCommand::RespondToApproval {
                request_id: approval_id,
                response: ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();

    let mut after_approval = Vec::new();
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(10) {
        let Some(envelope) = supervisor.recv_event_envelope_timeout(Duration::from_millis(50))
        else {
            continue;
        };
        let created = matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLaneCreated { .. },
                ..
            })
        );
        after_approval.push(envelope);
        if created {
            break;
        }
    }
    let names: Vec<&str> = after_approval
        .iter()
        .map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(event) => match &event.kind {
                RuntimeEventKind::LaneUpdated { .. } => "LaneUpdated",
                RuntimeEventKind::LaneOutputAppended { .. } => "LaneOutputAppended",
                RuntimeEventKind::LaneRuntimeOwnerBound { .. } => "LaneRuntimeOwnerBound",
                RuntimeEventKind::StarterLaneCreated { .. } => "StarterLaneCreated",
                RuntimeEventKind::StarterLanePreviewInvalidated { .. } => {
                    "StarterLanePreviewInvalidated"
                }
                RuntimeEventKind::CommandRejected { .. } => "CommandRejected",
                RuntimeEventKind::Error { .. } => "Error",
                RuntimeEventKind::LaneRecoveryRequired { .. } => "LaneRecoveryRequired",
                RuntimeEventKind::ApprovalResolved { .. } => "ApprovalResolved",
                RuntimeEventKind::SnapshotUpdated { .. } => "SnapshotUpdated",
                _ => "other",
            },
            RuntimeWireEvent::Unknown { .. } => "unknown",
        })
        .collect();

    let lane_updated_at = names.iter().position(|name| *name == "LaneUpdated");
    let created_at = names
        .iter()
        .position(|name| *name == "StarterLaneCreated")
        .unwrap_or_else(|| {
            panic!(
                "no StarterLaneCreated receipt after the allow-once; \
                 the host's creation pending can never settle. ordered stream: {names:?}"
            )
        });
    let lane_updated_at = lane_updated_at.expect("the created Lane publishes LaneUpdated");
    assert!(
        lane_updated_at < created_at,
        "the receipt must follow the Lane fact: {names:?}"
    );
    assert!(
        !names[..created_at].contains(&"StarterLanePreviewInvalidated"),
        "a created Lane must never be preceded by an invalidation: {names:?}"
    );

    let receipt = after_approval
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLaneCreated { receipt },
                ..
            }) => Some(receipt.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(receipt.preview_id, preview.preview_id);
    assert_eq!(receipt.content_sha256, preview.content_sha256);
    assert_eq!(receipt.owner, preview.owner);
    assert_eq!(receipt.lane.id, preview.lane.id);
    // The popover's TASK field never reaches Core on this path: the default
    // preview carries no agent binding, so no session is started with it.
    assert!(preview.agent.is_none());
    assert!(receipt.session.is_none());
}

#[test]
fn starter_lane_create_waits_for_permission_and_emits_exact_owner_receipt() {
    let repo = starter_lane_repo("starter_lane_create_repo");
    let home = temp_dir("starter_lane_create_home");
    let effects = Arc::new(StarterLaneEffects::default());
    let supervisor = starter_supervisor(&repo, home, Arc::clone(&effects));
    let request = starter_request("starter-create", StarterLanePreset::Coder);
    let owner = starter_owner(&request.lane_id);
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "starter-preview",
            RuntimeCommand::PreviewStarterLane {
                request: request.clone(),
            },
        )
        .unwrap();
    let preview_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewed { .. },
                    ..
                })
            )
        })
    });
    let preview = preview_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewed { preview },
                ..
            }) => Some(preview.clone()),
            _ => None,
        })
        .unwrap();

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "starter-create",
            RuntimeCommand::CreateStarterLane {
                request,
                preview_id: preview.preview_id.clone(),
                content_sha256: preview.content_sha256.clone(),
            },
        )
        .unwrap();
    let approval_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { approval },
                    ..
                }) if approval.tool_name == "lane_create"
            )
        })
    });
    assert!(effects.calls.lock().unwrap().is_empty());
    let approval_id = approval_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(approval.id.clone()),
            _ => None,
        })
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "starter-approve",
            RuntimeCommand::RespondToApproval {
                request_id: approval_id,
                response: ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    let created_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLaneCreated { .. },
                    ..
                })
            )
        })
    });
    let (event_owner, receipt) = created_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLaneCreated { receipt },
                ..
            }) => Some((&envelope.owner, receipt)),
            _ => None,
        })
        .unwrap();
    assert_eq!(event_owner, &owner);
    assert_eq!(receipt.owner, owner);
    assert_eq!(receipt.preview_id, preview.preview_id);
    assert_eq!(receipt.content_sha256, preview.content_sha256);
    assert_eq!(receipt.base_revision, preview.base_revision);
    assert_eq!(receipt.lane.id, "starter-create");
    assert_eq!(
        receipt.lane.active_session_ids,
        vec!["session-starter-create"]
    );
    let completed = supervisor.snapshot_envelope().unwrap();
    assert!(completed.view.starter_lane_previews.is_empty());
    assert_eq!(completed.view.starter_lane_receipts, vec![receipt.clone()]);
    assert_eq!(effects.calls.lock().unwrap().as_slice(), ["starter-create"]);
}

#[test]
fn starter_lane_pending_creation_cannot_be_replaced_by_a_second_preview() {
    let repo = starter_lane_repo("starter_lane_pending_duplicate_repo");
    let home = temp_dir("starter_lane_pending_duplicate_home");
    let effects = Arc::new(StarterLaneEffects::default());
    let supervisor = starter_supervisor(&repo, home, Arc::clone(&effects));
    let request = starter_request("starter-pending", StarterLanePreset::Coder);
    let owner = starter_owner(&request.lane_id);

    let preview = |command_id: &str| {
        supervisor
            .send_command_from_owner(
                owner.clone(),
                command_id,
                RuntimeCommand::PreviewStarterLane {
                    request: request.clone(),
                },
            )
            .unwrap();
        collect_starter_envelopes_until(&supervisor, |events| {
            events.iter().any(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::StarterLanePreviewed { .. },
                        ..
                    })
                )
            })
        })
        .into_iter()
        .find_map(|envelope| match envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewed { preview },
                ..
            }) => Some(preview),
            _ => None,
        })
        .unwrap()
    };
    let first = preview("preview-pending-first");
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "create-pending-first",
            RuntimeCommand::CreateStarterLane {
                request: request.clone(),
                preview_id: first.preview_id.clone(),
                content_sha256: first.content_sha256.clone(),
            },
        )
        .unwrap();
    let approval_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { .. },
                    ..
                })
            )
        })
    });
    let approval_id = approval_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(approval.id.clone()),
            _ => None,
        })
        .unwrap();

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "create-pending-legacy",
            RuntimeCommand::CreateLane {
                lane: first.lane.clone(),
            },
        )
        .unwrap();
    let legacy_rejected = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandRejected { command_id, .. },
                    ..
                }) if command_id == "create-pending-legacy"
            )
        })
    });
    assert!(!legacy_rejected.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewInvalidated { preview_id, .. },
                ..
            }) if preview_id == &first.preview_id
        )
    }));
    assert!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .starter_lane_previews
            .iter()
            .any(|preview| preview.preview_id == first.preview_id)
    );
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "start-pending-early",
            RuntimeCommand::StartLane {
                lane_id: request.lane_id.clone(),
                command: "worker".to_string(),
                args: Vec::new(),
                env: Vec::new(),
                output_log: None,
            },
        )
        .unwrap();
    let start_rejected = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandRejected { command_id, .. },
                    ..
                }) if command_id == "start-pending-early"
            )
        })
    });
    assert!(!start_rejected.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewInvalidated { preview_id, .. },
                ..
            }) if preview_id == &first.preview_id
        )
    }));
    assert!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .starter_lane_previews
            .iter()
            .any(|preview| preview.preview_id == first.preview_id)
    );

    let second = preview("preview-pending-second");
    assert_ne!(first.preview_id, second.preview_id);
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "create-pending-second",
            RuntimeCommand::CreateStarterLane {
                request,
                preview_id: second.preview_id.clone(),
                content_sha256: second.content_sha256,
            },
        )
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "approve-pending-first",
            RuntimeCommand::RespondToApproval {
                request_id: approval_id,
                response: ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    let completed = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLaneCreated { .. },
                    ..
                })
            )
        })
    });
    assert!(completed.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::CommandRejected { command_id, .. },
                ..
            }) if command_id == "create-pending-second"
        )
    }));
    assert!(completed.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewInvalidated {
                    preview_id,
                    reason: StarterLanePreviewInvalidationReason::LaneAlreadyRegistered,
                    ..
                },
                ..
            }) if preview_id == &second.preview_id
        )
    }));
    assert!(completed.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLaneCreated { receipt },
                ..
            }) if receipt.preview_id == first.preview_id && receipt.owner == owner
        )
    }));
    assert_eq!(
        effects.calls.lock().unwrap().as_slice(),
        ["starter-pending"]
    );
}

#[test]
fn starter_lane_pending_creation_cancel_is_a_clean_denial_without_recovery_or_effect() {
    let repo = starter_lane_repo("starter_lane_pending_cancel_repo");
    let home = temp_dir("starter_lane_pending_cancel_home");
    let effects = Arc::new(StarterLaneEffects::default());
    let supervisor = starter_supervisor(&repo, home.clone(), Arc::clone(&effects));
    let request = starter_request("starter-cancel", StarterLanePreset::Tester);
    let owner = starter_owner(&request.lane_id);
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "preview-cancel",
            RuntimeCommand::PreviewStarterLane {
                request: request.clone(),
            },
        )
        .unwrap();
    let preview_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewed { .. },
                    ..
                })
            )
        })
    });
    let preview = preview_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewed { preview },
                ..
            }) => Some(preview.clone()),
            _ => None,
        })
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "create-cancel",
            RuntimeCommand::CreateStarterLane {
                request,
                preview_id: preview.preview_id.clone(),
                content_sha256: preview.content_sha256,
            },
        )
        .unwrap();
    collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { .. },
                    ..
                })
            )
        })
    });
    supervisor
        .send_command_from_owner(
            owner,
            "cancel-pending-starter",
            RuntimeCommand::CancelActiveTurn,
        )
        .unwrap();
    let cancelled = collect_starter_envelopes_until(&supervisor, |events| {
        let invalidated = events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewInvalidated {
                        preview_id,
                        reason: StarterLanePreviewInvalidationReason::PermissionDenied,
                        ..
                    },
                    ..
                }) if preview_id == &preview.preview_id
            )
        });
        let accepted = events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandAccepted {
                        command_id,
                        command: RuntimeCommand::CancelActiveTurn,
                    },
                    ..
                }) if command_id == "cancel-pending-starter"
            )
        });
        invalidated && accepted
    });
    assert!(cancelled.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalResolved {
                    decision: viden_types::ApprovalDecision::Deny,
                    ..
                },
                ..
            })
        )
    }));
    assert!(!cancelled.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::LaneUpdated { .. }
                    | RuntimeEventKind::LaneRecoveryRequired { .. }
                    | RuntimeEventKind::Error { .. }
                    | RuntimeEventKind::StarterLaneCreated { .. },
                ..
            })
        )
    }));
    assert!(effects.calls.lock().unwrap().is_empty());
    assert!(
        viden_workflows::stores::WorkflowStore::new(home, &repo)
            .unwrap()
            .load_lane_events()
            .unwrap()
            .is_empty()
    );
    assert!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .starter_lane_previews
            .is_empty()
    );
}

#[test]
fn starter_lane_create_rejects_changed_request_stale_base_and_duplicate_worktree_without_effects() {
    for scenario in ["changed", "stale", "duplicate", "branch"] {
        let repo = starter_lane_repo(&format!("starter_lane_reject_{scenario}_repo"));
        let home = temp_dir(&format!("starter_lane_reject_{scenario}_home"));
        let effects = Arc::new(StarterLaneEffects::default());
        let supervisor = starter_supervisor(&repo, home.clone(), Arc::clone(&effects));
        let lane_id = format!("starter-{scenario}");
        let request = starter_request(&lane_id, StarterLanePreset::Coder);
        let owner = starter_owner(&lane_id);
        supervisor
            .send_command_from_owner(
                owner.clone(),
                format!("preview-{scenario}"),
                RuntimeCommand::PreviewStarterLane {
                    request: request.clone(),
                },
            )
            .unwrap();
        let preview_events = collect_starter_envelopes_until(&supervisor, |events| {
            events.iter().any(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::StarterLanePreviewed { .. },
                        ..
                    })
                )
            })
        });
        let preview = preview_events
            .iter()
            .find_map(|envelope| match &envelope.event {
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewed { preview },
                    ..
                }) => Some(preview.clone()),
                _ => None,
            })
            .unwrap();

        let create_request = match scenario {
            "changed" => StarterLaneRequest {
                branch: Some(format!("codex/{lane_id}-changed")),
                ..request
            },
            "stale" => {
                fs::write(repo.join("README.md"), "changed\n").unwrap();
                run_git(&repo, &["add", "README.md"]);
                run_git(&repo, &["commit", "-m", "advance base"]);
                request
            }
            "duplicate" => {
                fs::create_dir_all(&preview.worktree_path).unwrap();
                request
            }
            "branch" => {
                run_git(&repo, &["branch", &preview.branch]);
                request
            }
            _ => unreachable!(),
        };
        supervisor
            .send_command_from_owner(
                owner,
                format!("create-{scenario}"),
                RuntimeCommand::CreateStarterLane {
                    request: create_request,
                    preview_id: preview.preview_id.clone(),
                    content_sha256: preview.content_sha256.clone(),
                },
            )
            .unwrap();
        let expected_invalidation = match scenario {
            "changed" => StarterLanePreviewInvalidationReason::RequestChanged,
            "stale" => StarterLanePreviewInvalidationReason::BaseRevisionChanged,
            "duplicate" => StarterLanePreviewInvalidationReason::WorktreeUnavailable,
            "branch" => StarterLanePreviewInvalidationReason::BranchUnavailable,
            _ => unreachable!(),
        };
        let rejected = collect_starter_envelopes_until(&supervisor, |events| {
            let rejected = events.iter().any(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::CommandRejected { .. },
                        ..
                    })
                )
            });
            let invalidated = events.iter().any(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::StarterLanePreviewInvalidated { reason, .. },
                        ..
                    }) if *reason == expected_invalidation
                )
            });
            rejected && invalidated
        });
        assert!(rejected.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandRejected { reason, .. },
                    ..
                }) if reason.contains(scenario)
                    || (scenario == "stale" && reason.contains("base revision"))
                    || (scenario == "duplicate" && reason.contains("worktree"))
                    || (scenario == "branch" && reason.contains("branch"))
            )
        }));
        assert!(effects.calls.lock().unwrap().is_empty());
        assert!(
            viden_workflows::stores::WorkflowStore::new(home, &repo)
                .unwrap()
                .load_lane_events()
                .unwrap()
                .is_empty()
        );
        assert!(
            supervisor
                .snapshot_envelope()
                .unwrap()
                .view
                .starter_lane_previews
                .is_empty()
        );
        assert!(!rejected.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { .. }
                        | RuntimeEventKind::StarterLaneCreated { .. },
                    ..
                })
            )
        }));
    }
}

#[test]
fn starter_lane_create_plan_mode_denies_before_approval_and_consumes_preview() {
    let repo = starter_lane_repo("starter_lane_plan_repo");
    let home = temp_dir("starter_lane_plan_home");
    let effects = Arc::new(StarterLaneEffects::default());
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&repo, provider, Some(home)).unwrap();
    engine.set_work_mode(WorkMode::Plan).unwrap();
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let request = starter_request("starter-plan", StarterLanePreset::Tester);
    let owner = starter_owner(&request.lane_id);
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "preview-plan",
            RuntimeCommand::PreviewStarterLane {
                request: request.clone(),
            },
        )
        .unwrap();
    let preview_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewed { .. },
                    ..
                })
            )
        })
    });
    let preview = preview_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewed { preview },
                ..
            }) => Some(preview.clone()),
            _ => None,
        })
        .unwrap();

    supervisor
        .send_command_from_owner(
            owner,
            "create-plan",
            RuntimeCommand::CreateStarterLane {
                request,
                preview_id: preview.preview_id,
                content_sha256: preview.content_sha256,
            },
        )
        .unwrap();
    let rejected = collect_starter_envelopes_until(&supervisor, |events| {
        let rejected = events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandRejected { .. },
                    ..
                })
            )
        });
        let invalidated = events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewInvalidated {
                        reason: StarterLanePreviewInvalidationReason::PlanModeDenied,
                        ..
                    },
                    ..
                })
            )
        });
        rejected && invalidated
    });
    assert!(rejected.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::CommandRejected { reason, .. },
                ..
            }) if reason.contains("Plan mode")
        )
    }));
    assert!(effects.calls.lock().unwrap().is_empty());
    assert!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .starter_lane_previews
            .is_empty()
    );
}

#[test]
fn starter_lane_create_rejects_unknown_hash_and_wrong_owner_as_one_time_preview_failures() {
    for scenario in ["unknown", "hash", "owner"] {
        let repo = starter_lane_repo(&format!("starter_lane_identity_{scenario}_repo"));
        let home = temp_dir(&format!("starter_lane_identity_{scenario}_home"));
        let effects = Arc::new(StarterLaneEffects::default());
        let supervisor = starter_supervisor(&repo, home, Arc::clone(&effects));
        let lane_id = format!("starter-{scenario}");
        let request = starter_request(&lane_id, StarterLanePreset::Reviewer);
        let owner = starter_owner(&lane_id);
        supervisor
            .send_command_from_owner(
                owner.clone(),
                format!("preview-{scenario}"),
                RuntimeCommand::PreviewStarterLane {
                    request: request.clone(),
                },
            )
            .unwrap();
        let preview_events = collect_starter_envelopes_until(&supervisor, |events| {
            events.iter().any(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::StarterLanePreviewed { .. },
                        ..
                    })
                )
            })
        });
        let preview = preview_events
            .iter()
            .find_map(|envelope| match &envelope.event {
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewed { preview },
                    ..
                }) => Some(preview.clone()),
                _ => None,
            })
            .unwrap();
        let (attempt_owner, preview_id, hash) = match scenario {
            "unknown" => (
                owner.clone(),
                "missing-preview".to_string(),
                preview.content_sha256.clone(),
            ),
            "hash" => (owner.clone(), preview.preview_id.clone(), "00".repeat(32)),
            "owner" => {
                let mut wrong = owner.clone();
                wrong.workspace_id = "wrong-workspace".to_string();
                (
                    wrong,
                    preview.preview_id.clone(),
                    preview.content_sha256.clone(),
                )
            }
            _ => unreachable!(),
        };
        supervisor
            .send_command_from_owner(
                attempt_owner,
                format!("create-{scenario}"),
                RuntimeCommand::CreateStarterLane {
                    request: request.clone(),
                    preview_id,
                    content_sha256: hash,
                },
            )
            .unwrap();
        let rejected = collect_starter_envelopes_until(&supervisor, |events| {
            events.iter().any(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::CommandRejected { .. },
                        ..
                    })
                )
            })
        });
        assert!(rejected.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandRejected { reason, .. },
                    ..
                }) if reason.contains(scenario) || (scenario == "unknown" && reason.contains("preview"))
            )
        }));
        assert!(effects.calls.lock().unwrap().is_empty());
        let snapshot = supervisor.snapshot_envelope().unwrap();
        assert_eq!(
            snapshot.view.starter_lane_previews.len(),
            usize::from(scenario != "hash")
        );
        assert_eq!(
            rejected.iter().any(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::StarterLanePreviewInvalidated {
                            reason: StarterLanePreviewInvalidationReason::HashMismatch,
                            ..
                        },
                        ..
                    })
                )
            }),
            scenario == "hash"
        );

        if scenario == "owner" {
            supervisor
                .send_command_from_owner(
                    owner.clone(),
                    "create-owner-after-mismatch",
                    RuntimeCommand::CreateStarterLane {
                        request,
                        preview_id: preview.preview_id,
                        content_sha256: preview.content_sha256,
                    },
                )
                .unwrap();
            let approval = collect_starter_envelopes_until(&supervisor, |events| {
                events.iter().any(|envelope| {
                    matches!(
                        &envelope.event,
                        RuntimeWireEvent::Known(RuntimeEvent {
                            kind: RuntimeEventKind::ApprovalRequested { .. },
                            ..
                        })
                    )
                })
            });
            let approval_id = approval
                .iter()
                .find_map(|envelope| match &envelope.event {
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::ApprovalRequested { approval },
                        ..
                    }) => Some(approval.id.clone()),
                    _ => None,
                })
                .unwrap();
            supervisor
                .send_command_from_owner(
                    owner,
                    "deny-owner-after-mismatch",
                    RuntimeCommand::RespondToApproval {
                        request_id: approval_id,
                        response: ApprovalResponse::deny(None),
                    },
                )
                .unwrap();
            collect_starter_envelopes_until(&supervisor, |events| {
                events.iter().any(|envelope| {
                    matches!(
                        &envelope.event,
                        RuntimeWireEvent::Known(RuntimeEvent {
                            kind: RuntimeEventKind::StarterLanePreviewInvalidated {
                                reason: StarterLanePreviewInvalidationReason::PermissionDenied,
                                ..
                            },
                            ..
                        })
                    )
                })
            });
        }
    }
}

#[test]
fn starter_lane_previews_are_owner_scoped_interleaved_and_replayable_in_process() {
    let repo = starter_lane_repo("starter_lane_multi_owner_repo");
    let home = temp_dir("starter_lane_multi_owner_home");
    let effects = Arc::new(StarterLaneEffects::default());
    let supervisor = starter_supervisor(&repo, home, Arc::clone(&effects));
    let initial_cursor = supervisor.snapshot_envelope().unwrap().cursor;
    let request_a = starter_request("starter-owner-a", StarterLanePreset::Coder);
    let request_b = starter_request("starter-owner-b", StarterLanePreset::Reviewer);
    let owner_a = starter_owner(&request_a.lane_id);
    let owner_b = starter_owner(&request_b.lane_id);
    for (command_id, owner, request) in [
        ("preview-owner-a", owner_a.clone(), request_a.clone()),
        ("preview-owner-b", owner_b.clone(), request_b.clone()),
    ] {
        supervisor
            .send_command_from_owner(
                owner,
                command_id,
                RuntimeCommand::PreviewStarterLane { request },
            )
            .unwrap();
    }
    let preview_events = collect_starter_envelopes_until(&supervisor, |events| {
        events
            .iter()
            .filter(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::StarterLanePreviewed { .. },
                        ..
                    })
                )
            })
            .count()
            == 2
    });
    let preview_a = preview_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewed { preview },
                ..
            }) if preview.owner == owner_a => Some(preview.clone()),
            _ => None,
        })
        .unwrap();
    let preview_b = preview_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewed { preview },
                ..
            }) if preview.owner == owner_b => Some(preview.clone()),
            _ => None,
        })
        .unwrap();
    let preview_a_id = preview_a.preview_id.clone();
    let preview_b_id = preview_b.preview_id.clone();
    let snapshot = supervisor.snapshot_envelope().unwrap();
    assert_eq!(snapshot.view.starter_lane_previews.len(), 2);
    let serialized_snapshot = serde_json::to_string(&snapshot.view).unwrap();
    assert!(serialized_snapshot.contains("starter_lane_previews"));
    assert!(serialized_snapshot.contains(&preview_a.preview_id));
    assert!(serialized_snapshot.contains(&preview_b.preview_id));
    let preview_replay = supervisor
        .replay_events(ReplayRequest {
            after: initial_cursor.clone(),
            limit: 100,
        })
        .unwrap();
    assert!(preview_replay.events.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewed { preview },
                ..
            }) if preview.owner == owner_a
        )
    }));
    let replay_json = serde_json::to_string(&preview_replay).unwrap();
    assert!(replay_json.contains(&preview_a.preview_id));
    assert!(replay_json.contains(&preview_b.preview_id));

    supervisor
        .send_command_from_owner(
            owner_a.clone(),
            "reject-owner-a",
            RuntimeCommand::CreateStarterLane {
                request: request_a,
                preview_id: preview_a.preview_id,
                content_sha256: "00".repeat(32),
            },
        )
        .unwrap();
    let rejected_a = collect_starter_envelopes_until(&supervisor, |events| {
        let rejected = events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::CommandRejected { command_id, .. }, .. }) if command_id == "reject-owner-a"));
        let invalidated = events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::StarterLanePreviewInvalidated { owner, preview_id, reason: StarterLanePreviewInvalidationReason::HashMismatch }, .. }) if owner == &owner_a && preview_id == &preview_a_id));
        rejected && invalidated
    });
    assert!(rejected_a.iter().all(|envelope| {
        !matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::StarterLanePreviewInvalidated { owner, preview_id, .. }, .. }) if owner == &owner_b || preview_id == &preview_b_id)
    }));
    let after_a = supervisor.snapshot_envelope().unwrap();
    assert_eq!(after_a.view.starter_lane_previews.len(), 1);
    assert_eq!(after_a.view.starter_lane_previews[0].owner, owner_b);

    supervisor
        .send_command_from_owner(
            owner_b.clone(),
            "create-owner-b",
            RuntimeCommand::CreateStarterLane {
                request: request_b,
                preview_id: preview_b.preview_id,
                content_sha256: preview_b.content_sha256,
            },
        )
        .unwrap();
    let approval_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            envelope.owner == owner_b
                && matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::ApprovalRequested { .. },
                        ..
                    })
                )
        })
    });
    let approval_id = approval_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) if envelope.owner == owner_b => Some(approval.id.clone()),
            _ => None,
        })
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner_b.clone(),
            "approve-owner-b",
            RuntimeCommand::RespondToApproval {
                request_id: approval_id,
                response: ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    let created = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| envelope.owner == owner_b && matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::StarterLaneCreated { receipt }, .. }) if receipt.owner == owner_b))
    });
    assert!(created.iter().any(|envelope| envelope.owner == owner_b && matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::StarterLaneCreated { receipt }, .. }) if receipt.lane.id == "starter-owner-b")));
    let completed_replay = supervisor
        .replay_events(ReplayRequest {
            after: initial_cursor,
            limit: 100,
        })
        .unwrap();
    assert!(completed_replay.events.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLaneCreated { receipt },
                ..
            }) if receipt.owner == owner_b
        )
    }));
    assert!(completed_replay.events.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewInvalidated {
                    owner,
                    preview_id,
                    reason: StarterLanePreviewInvalidationReason::HashMismatch,
                },
                ..
            }) if owner == &owner_a && preview_id == &preview_a_id
        )
    }));
    let completed_snapshot = supervisor.snapshot_envelope().unwrap();
    assert!(completed_snapshot.view.starter_lane_previews.is_empty());
    assert_eq!(completed_snapshot.view.starter_lane_receipts.len(), 1);
    assert_eq!(
        completed_snapshot.view.starter_lane_receipts[0].preview_id,
        preview_b_id
    );
    assert_eq!(
        effects.calls.lock().unwrap().as_slice(),
        ["starter-owner-b"]
    );
}

#[test]
fn starter_lane_create_rechecks_base_after_permission_before_effect() {
    let repo = starter_lane_repo("starter_lane_toctou_repo");
    let home = temp_dir("starter_lane_toctou_home");
    let effects = Arc::new(StarterLaneEffects::default());
    let supervisor = starter_supervisor(&repo, home, Arc::clone(&effects));
    let request = starter_request("starter-toctou", StarterLanePreset::Coder);
    let owner = starter_owner(&request.lane_id);
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "preview-toctou",
            RuntimeCommand::PreviewStarterLane {
                request: request.clone(),
            },
        )
        .unwrap();
    let preview_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewed { .. },
                    ..
                })
            )
        })
    });
    let preview = preview_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewed { preview },
                ..
            }) => Some(preview.clone()),
            _ => None,
        })
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "create-toctou",
            RuntimeCommand::CreateStarterLane {
                request,
                preview_id: preview.preview_id,
                content_sha256: preview.content_sha256,
            },
        )
        .unwrap();
    let approval_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { .. },
                    ..
                })
            )
        })
    });
    let approval_id = approval_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(approval.id.clone()),
            _ => None,
        })
        .unwrap();
    fs::write(repo.join("README.md"), "advanced during approval\n").unwrap();
    run_git(&repo, &["add", "README.md"]);
    run_git(&repo, &["commit", "-m", "advance while pending"]);
    supervisor
        .send_command_from_owner(
            owner,
            "approve-toctou",
            RuntimeCommand::RespondToApproval {
                request_id: approval_id,
                response: ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    let failed = collect_starter_envelopes_until(&supervisor, |events| {
        let failed = events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::Error { error }, .. }) if error.message.contains("base revision changed")));
        let invalidated = events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewInvalidated {
                        reason: StarterLanePreviewInvalidationReason::EffectFailed,
                        ..
                    },
                    ..
                })
            )
        });
        failed && invalidated
    });
    assert!(failed.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::Error { error }, .. }) if error.message.contains("approval was pending"))));
    assert!(effects.calls.lock().unwrap().is_empty());
    assert!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .starter_lane_previews
            .is_empty()
    );
    assert!(!failed.iter().any(|envelope| matches!(
        &envelope.event,
        RuntimeWireEvent::Known(RuntimeEvent {
            kind: RuntimeEventKind::StarterLaneCreated { .. },
            ..
        })
    )));
}

#[test]
fn starter_lane_preview_rejects_parent_absolute_and_symlink_escape_paths() {
    let repo = starter_lane_repo("starter_lane_path_scope_repo");
    let home = temp_dir("starter_lane_path_scope_home");
    let outside = temp_dir("starter_lane_path_scope_outside");
    let effects = Arc::new(StarterLaneEffects::default());
    let supervisor = starter_supervisor(&repo, home, Arc::clone(&effects));
    let symlink_parent = repo.join("linked");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &symlink_parent).unwrap();

    let mut paths = vec![
        "../escape".to_string(),
        outside.join("absolute-escape").display().to_string(),
    ];
    #[cfg(unix)]
    paths.push("linked/escape".to_string());
    for (index, worktree_path) in paths.into_iter().enumerate() {
        let lane_id = format!("starter-escape-{index}");
        let owner = starter_owner(&lane_id);
        supervisor
            .send_command_from_owner(
                owner,
                format!("preview-escape-{index}"),
                RuntimeCommand::PreviewStarterLane {
                    request: StarterLaneRequest {
                        worktree_path: Some(worktree_path),
                        ..starter_request(&lane_id, StarterLanePreset::Tester)
                    },
                },
            )
            .unwrap();
        let rejected = collect_starter_envelopes_until(&supervisor, |events| {
            events.iter().any(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::CommandRejected { .. },
                        ..
                    })
                )
            })
        });
        assert!(!rejected.iter().any(|envelope| matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewed { .. },
                ..
            })
        )));
    }
    let invalid_branch_lane = "starter-invalid-branch";
    supervisor
        .send_command_from_owner(
            starter_owner(invalid_branch_lane),
            "preview-invalid-branch",
            RuntimeCommand::PreviewStarterLane {
                request: StarterLaneRequest {
                    branch: Some("invalid branch".to_string()),
                    ..starter_request(invalid_branch_lane, StarterLanePreset::Coder)
                },
            },
        )
        .unwrap();
    let invalid_branch = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::CommandRejected { reason, .. }, .. }) if reason.contains("branch")))
    });
    assert!(!invalid_branch.iter().any(|envelope| matches!(
        &envelope.event,
        RuntimeWireEvent::Known(RuntimeEvent {
            kind: RuntimeEventKind::StarterLanePreviewed { .. },
            ..
        })
    )));
    assert!(effects.calls.lock().unwrap().is_empty());
}

#[test]
fn starter_lane_denied_create_consumes_only_its_preview_without_any_fact_or_effect() {
    let repo = starter_lane_repo("starter_lane_denied_repo");
    let home = temp_dir("starter_lane_denied_home");
    let effects = Arc::new(StarterLaneEffects::default());
    let supervisor = starter_supervisor(&repo, home.clone(), Arc::clone(&effects));
    let request = starter_request("starter-denied", StarterLanePreset::Tester);
    let owner = starter_owner(&request.lane_id);
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "preview-denied",
            RuntimeCommand::PreviewStarterLane {
                request: request.clone(),
            },
        )
        .unwrap();
    let preview_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewed { .. },
                    ..
                })
            )
        })
    });
    let preview = preview_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewed { preview },
                ..
            }) => Some(preview.clone()),
            _ => None,
        })
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "create-denied",
            RuntimeCommand::CreateStarterLane {
                request: request.clone(),
                preview_id: preview.preview_id.clone(),
                content_sha256: preview.content_sha256.clone(),
            },
        )
        .unwrap();
    let approval_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { .. },
                    ..
                })
            )
        })
    });
    let approval_id = approval_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(approval.id.clone()),
            _ => None,
        })
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "deny-starter",
            RuntimeCommand::RespondToApproval {
                request_id: approval_id,
                response: ApprovalResponse::deny(None),
            },
        )
        .unwrap();
    let denied = collect_starter_envelopes_until(&supervisor, |events| {
        let rejected = events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::CommandRejected { command_id, .. }, .. }) if command_id == "create-denied"));
        let invalidated = events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::StarterLanePreviewInvalidated { owner: event_owner, preview_id, reason: StarterLanePreviewInvalidationReason::PermissionDenied }, .. }) if event_owner == &owner && preview_id == &preview.preview_id));
        rejected && invalidated
    });
    assert!(denied.iter().any(|envelope| matches!(
        &envelope.event,
        RuntimeWireEvent::Known(RuntimeEvent {
            kind: RuntimeEventKind::StarterLanePreviewInvalidated {
                reason: StarterLanePreviewInvalidationReason::PermissionDenied,
                ..
            },
            ..
        })
    )));
    assert!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .starter_lane_previews
            .is_empty()
    );
    supervisor
        .send_command_from_owner(
            owner,
            "reuse-denied",
            RuntimeCommand::CreateStarterLane {
                request,
                preview_id: preview.preview_id,
                content_sha256: preview.content_sha256,
            },
        )
        .unwrap();
    let reused = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::CommandRejected { command_id, reason }, .. }) if command_id == "reuse-denied" && reason.contains("unknown")))
    });
    assert!(effects.calls.lock().unwrap().is_empty());
    assert!(!Path::new(&preview.worktree_path).exists());
    assert_eq!(run_git(&repo, &["branch", "--list", &preview.branch]), "");
    assert!(!reused.iter().any(|envelope| matches!(
        &envelope.event,
        RuntimeWireEvent::Known(RuntimeEvent {
            kind: RuntimeEventKind::StarterLaneCreated { .. },
            ..
        })
    )));
    assert!(
        viden_workflows::stores::WorkflowStore::new(home, &repo)
            .unwrap()
            .load_lane_events()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn starter_lane_preview_is_process_local_and_invalid_after_supervisor_restart() {
    let repo = starter_lane_repo("starter_lane_restart_repo");
    let home = temp_dir("starter_lane_restart_home");
    let effects = Arc::new(StarterLaneEffects::default());
    let request = starter_request("starter-restart", StarterLanePreset::Coder);
    let owner = starter_owner(&request.lane_id);
    let supervisor = starter_supervisor(&repo, home.clone(), Arc::clone(&effects));
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "preview-restart",
            RuntimeCommand::PreviewStarterLane {
                request: request.clone(),
            },
        )
        .unwrap();
    let preview_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewed { .. },
                    ..
                })
            )
        })
    });
    let preview = preview_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewed { preview },
                ..
            }) => Some(preview.clone()),
            _ => None,
        })
        .unwrap();
    drop(supervisor);

    let restarted = starter_supervisor(&repo, home, Arc::clone(&effects));
    restarted
        .send_command_from_owner(
            owner,
            "create-restart",
            RuntimeCommand::CreateStarterLane {
                request,
                preview_id: preview.preview_id,
                content_sha256: preview.content_sha256,
            },
        )
        .unwrap();
    let rejected = collect_starter_envelopes_until(&restarted, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::CommandRejected { reason, .. }, .. }) if reason.contains("unknown")))
    });
    assert!(rejected.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::CommandRejected { reason, .. }, .. }) if reason.contains("preview"))));
    assert!(effects.calls.lock().unwrap().is_empty());
}

struct AlwaysFailLanePersistence;

impl LanePersistence for AlwaysFailLanePersistence {
    fn append(&self, _event: &LaneEvent) -> Result<(), String> {
        Err("injected starter lane append failure".to_string())
    }

    fn load_lanes(&self) -> Result<BTreeMap<String, viden_types::AgentLaneRecord>, String> {
        Ok(BTreeMap::new())
    }
}

#[test]
fn starter_lane_real_git_compensation_removes_worktree_and_new_branch_on_append_failure() {
    let repo = starter_lane_repo("starter_lane_compensation_repo");
    let home = temp_dir("starter_lane_compensation_home");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&repo, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let supervisor = RuntimeSupervisor::start_with_lane_effects_and_persistence_for_test(
        engine,
        Arc::new(LocalLaneEffectExecutor::default()) as Arc<dyn LaneEffectExecutor>,
        Arc::new(AlwaysFailLanePersistence),
    );
    let request = starter_request("starter-compensate", StarterLanePreset::Coder);
    let owner = starter_owner(&request.lane_id);
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "preview-compensate",
            RuntimeCommand::PreviewStarterLane {
                request: request.clone(),
            },
        )
        .unwrap();
    let preview_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewed { .. },
                    ..
                })
            )
        })
    });
    let preview = preview_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewed { preview },
                ..
            }) => Some(preview.clone()),
            _ => None,
        })
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "create-compensate",
            RuntimeCommand::CreateStarterLane {
                request,
                preview_id: preview.preview_id,
                content_sha256: preview.content_sha256,
            },
        )
        .unwrap();
    let approval_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { .. },
                    ..
                })
            )
        })
    });
    let approval_id = approval_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(approval.id.clone()),
            _ => None,
        })
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner,
            "approve-compensate",
            RuntimeCommand::RespondToApproval {
                request_id: approval_id,
                response: ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    let failed = collect_starter_envelopes_until(&supervisor, |events| {
        let recovery = events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneRecoveryRequired { .. },
                    ..
                })
            )
        });
        let invalidated = events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewInvalidated {
                        reason: StarterLanePreviewInvalidationReason::EffectFailed,
                        ..
                    },
                    ..
                })
            )
        });
        recovery && invalidated
    });
    assert!(!Path::new(&preview.worktree_path).exists());
    assert_eq!(run_git(&repo, &["branch", "--list", &preview.branch]), "");
    assert!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .starter_lane_previews
            .is_empty()
    );
    assert!(!failed.iter().any(|envelope| matches!(
        &envelope.event,
        RuntimeWireEvent::Known(RuntimeEvent {
            kind: RuntimeEventKind::StarterLaneCreated { .. },
            ..
        })
    )));
}

#[test]
fn recent_work_command_is_read_only_in_plan_and_emits_exactly_accepted_then_loaded() {
    let cwd = temp_dir("recent_work_runtime_cwd");
    let other = temp_dir("recent_work_runtime_other");
    let home = temp_dir("recent_work_runtime_home");
    append_recent_runtime_fixture(&home, &other, "recent-runtime", 10);
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    engine.set_work_mode(WorkMode::Plan).unwrap();
    let mut approver = |_prompt| panic!("read-only recent work must not request approval");

    let events = engine
        .handle_runtime_command(
            "recent-work",
            RuntimeCommand::QueryRecentWork {
                query: RecentWorkQuery { limit: 10 },
            },
            &mut approver,
        )
        .unwrap();

    assert_eq!(events.len(), 2);
    assert!(matches!(
        &events[0].kind,
        RuntimeEventKind::CommandAccepted { command_id, .. } if command_id == "recent-work"
    ));
    assert!(matches!(
        &events[1].kind,
        RuntimeEventKind::RecentWorkLoaded { sessions, .. }
            if sessions.len() == 1 && sessions[0].session_id == "recent-runtime"
    ));
    assert!(
        !serde_json::to_string(&events)
            .unwrap()
            .contains("sk-secret-runtime-body")
    );
    assert!(
        engine
            .workflow_store()
            .load_agent_events()
            .unwrap()
            .is_empty()
    );
    assert!(
        !fs::read_to_string(engine.store.transcript_path())
            .unwrap()
            .contains("recent_work_loaded")
    );
}

#[test]
fn recent_work_new_transcript_starts_with_canonical_root_and_stable_timestamp_metadata() {
    let cwd = temp_dir("recent_work_initial_metadata_cwd")
        .canonicalize()
        .unwrap();
    let home = temp_dir("recent_work_initial_metadata_home");
    let engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();

    let entries = engine.store.load_entries().unwrap();
    let root = entries.iter().find_map(|entry| match entry {
        TranscriptEntry::SessionMeta { entry } if entry.key == "canonical_root" => {
            Some((entry.timestamp, entry.value.clone()))
        }
        _ => None,
    });
    let created = entries.iter().find_map(|entry| match entry {
        TranscriptEntry::SessionMeta { entry } if entry.key == "session_created_at" => {
            Some((entry.timestamp, entry.value.clone()))
        }
        _ => None,
    });

    assert_eq!(root.as_ref().map(|(_, value)| value.as_str()), cwd.to_str());
    assert_eq!(
        created
            .as_ref()
            .and_then(|(_, value)| value.parse::<u64>().ok()),
        created.as_ref().map(|(timestamp, _)| *timestamp)
    );
    let raw = fs::read_to_string(engine.store.transcript_path()).unwrap();
    assert!(raw.starts_with("{\"type\":\"runtime_event_batch_begin\""));
}

#[test]
fn recent_work_supervisor_snapshot_and_replay_restore_the_last_loaded_result() {
    let cwd = temp_dir("recent_work_supervisor_cwd")
        .canonicalize()
        .unwrap();
    let other = temp_dir("recent_work_supervisor_other")
        .canonicalize()
        .unwrap();
    let home = temp_dir("recent_work_supervisor_home");
    append_recent_runtime_fixture(&home, &other, "other-session", 10);
    let engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let supervisor = RuntimeSupervisor::start(engine);
    supervisor
        .send_command(
            "recent-work-supervisor",
            RuntimeCommand::QueryRecentWork {
                query: RecentWorkQuery { limit: 10 },
            },
        )
        .unwrap();

    let events = collect_until(&supervisor, |events| {
        events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::RecentWorkLoaded { .. }))
    });
    let loaded = events
        .iter()
        .find(|event| matches!(event.kind, RuntimeEventKind::RecentWorkLoaded { .. }))
        .unwrap();
    let snapshot = supervisor.snapshot_envelope().unwrap();
    assert_eq!(snapshot.view.recent_sessions.len(), 2);
    assert!(
        snapshot
            .view
            .recent_sessions
            .iter()
            .any(|session| session.session_id == "other-session")
    );

    let replay = supervisor
        .replay_events(ReplayRequest {
            after: EventCursor {
                stream_id: snapshot.cursor.stream_id.clone(),
                sequence: 0,
            },
            limit: 10,
        })
        .unwrap();
    assert!(replay.events.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(event)
                if event.sequence == loaded.sequence
                    && matches!(event.kind, RuntimeEventKind::RecentWorkLoaded { .. })
        )
    }));
}

fn preference_engine(slug: &str) -> (SessionEngine, PathBuf, PathBuf) {
    let cwd = temp_dir(slug);
    let config_path = cwd.join("user-config.toml");
    let home = cwd.join("session-home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    engine.set_ui_preference_context(
        None,
        Some(config_path.clone()),
        UiPreferences::client_default(),
    );
    (engine, cwd, config_path)
}

#[test]
fn ui_preferences_command_set_emits_fact_and_reducer_syncs_view_and_snapshot() {
    let (mut engine, _cwd, config_path) = preference_engine("ui_preferences_set");
    let before = engine.runtime_view_state();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let events = engine
        .handle_runtime_command(
            "set-ui",
            RuntimeCommand::SetUiPreferences {
                patch: UiPreferencePatch {
                    locale: Some(LocaleId::ZhCn),
                    skin: Some(UiSkin::Ice),
                    mode: Some(UiColorMode::Light),
                    density: Some(UiDensity::Compact),
                    motion: Some(UiMotion::Reduced),
                },
            },
            &mut approver,
        )
        .unwrap();

    let fact = events
        .iter()
        .find(|event| matches!(event.kind, RuntimeEventKind::UiPreferencesUpdated { .. }))
        .expect("successful preference command emits a fact");
    let mut replayed = before;
    replayed.apply_event(fact);
    assert_eq!(replayed.ui_preferences.locale, LocaleId::ZhCn);
    assert_eq!(replayed.ui_preferences, replayed.snapshot.ui_preferences);
    assert_eq!(
        engine.runtime_snapshot().ui_preferences,
        replayed.ui_preferences
    );
    assert!(fs::read_to_string(config_path).unwrap().contains("zh-CN"));
}

#[test]
fn ui_preferences_command_invalid_profile_is_rejected_before_approval() {
    let (mut engine, cwd, config_path) = preference_engine("ui_preferences_invalid");
    let original = b"[ui]\nskin = \"amber\"\nmode = \"dark\"\n";
    fs::write(&config_path, original).unwrap();
    let approvals = AtomicUsize::new(0);
    let mut approver = |_prompt| {
        approvals.fetch_add(1, Ordering::SeqCst);
        ApprovalResponse::allow_once(None)
    };
    let events = engine
        .handle_runtime_command(
            "invalid-ui",
            RuntimeCommand::SetUiPreferences {
                patch: UiPreferencePatch {
                    mode: Some(UiColorMode::Light),
                    ..UiPreferencePatch::default()
                },
            },
            &mut approver,
        )
        .unwrap();

    assert_eq!(approvals.load(Ordering::SeqCst), 0);
    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("ui.invalid_skin_mode_pair")
    )));
    assert_eq!(fs::read(&config_path).unwrap(), original);
    assert!(ui_temp_files(&cwd).is_empty());
}

#[test]
fn ui_preferences_command_cli_override_wins_after_persisting_user_profile() {
    let (mut engine, _cwd, config_path) = preference_engine("ui_preferences_cli_wins");
    let cli = UiPreferences {
        locale: LocaleId::En,
        skin: UiSkin::Mono,
        mode: UiColorMode::Dark,
        density: UiDensity::Regular,
        motion: UiMotion::Full,
    };
    engine.set_ui_preference_context(
        Some(cli),
        Some(config_path),
        UiPreferences::client_default(),
    );
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let events = engine
        .handle_runtime_command(
            "cli-wins",
            RuntimeCommand::SetUiPreferences {
                patch: UiPreferencePatch {
                    locale: Some(LocaleId::ZhCn),
                    skin: Some(UiSkin::Ice),
                    mode: Some(UiColorMode::Light),
                    density: Some(UiDensity::Compact),
                    motion: Some(UiMotion::Reduced),
                },
            },
            &mut approver,
        )
        .unwrap();

    let (resolved, persisted) = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::UiPreferencesUpdated {
                resolved,
                persisted,
                ..
            } => Some((resolved, persisted)),
            _ => None,
        })
        .unwrap();
    assert_eq!(resolved.locale, LocaleId::En);
    assert_eq!(resolved.skin, UiSkin::Mono);
    assert_eq!(persisted.as_ref().unwrap().locale, LocaleId::ZhCn);
}

#[test]
fn ui_preferences_command_bootstrap_retains_only_safe_reresolution_context() {
    let cwd = temp_dir("ui_preferences_bootstrap_context");
    let config_path = cwd.join("user-config.toml");
    let cli = UiPreferences {
        locale: LocaleId::En,
        skin: UiSkin::Mono,
        mode: UiColorMode::Dark,
        density: UiDensity::Regular,
        motion: UiMotion::Full,
    };
    let bootstrap = crate::bootstrap_runtime(crate::RuntimeBootstrapRequest::new(
        &cwd,
        CliOverrides {
            provider: Some("fallback".to_string()),
            model: Some("test-local".to_string()),
            api_key: Some("must-not-be-retained-for-ui-reresolution".to_string()),
            session_home: Some(cwd.join("session-home")),
            config_path: Some(config_path),
            ui: Some(cli),
            ..CliOverrides::default()
        },
    ))
    .unwrap();
    let mut engine = bootstrap.engine;
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let events = engine
        .handle_runtime_command(
            "bootstrap-ui",
            RuntimeCommand::SetUiPreferences {
                patch: UiPreferencePatch {
                    locale: Some(LocaleId::ZhCn),
                    skin: Some(UiSkin::Ice),
                    mode: Some(UiColorMode::Light),
                    ..UiPreferencePatch::default()
                },
            },
            &mut approver,
        )
        .unwrap();

    let serialized = serde_json::to_string(&events).unwrap();
    assert!(!serialized.contains("must-not-be-retained"));
    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::UiPreferencesUpdated { resolved, .. }
            if resolved.locale == LocaleId::En && resolved.skin == UiSkin::Mono
    )));
}

#[test]
fn ui_preferences_command_plan_mode_preserves_bytes_mtime_and_temp_state() {
    let (mut engine, cwd, config_path) = preference_engine("ui_preferences_plan");
    let original = b"[ui]\nskin = \"ice\"\nmode = \"dark\"\n";
    fs::write(&config_path, original).unwrap();
    let before_modified = fs::metadata(&config_path).unwrap().modified().unwrap();
    engine.set_work_mode(WorkMode::Plan).unwrap();
    let mut approver = |_prompt| panic!("Plan mode must deny before approval");
    let events = engine
        .handle_runtime_command("plan-ui", RuntimeCommand::ResetUiPreferences, &mut approver)
        .unwrap();

    assert!(
        events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::CommandRejected { .. }))
    );
    assert_eq!(fs::read(&config_path).unwrap(), original);
    assert_eq!(
        fs::metadata(&config_path).unwrap().modified().unwrap(),
        before_modified
    );
    assert!(ui_temp_files(&cwd).is_empty());
}

#[test]
fn ui_preferences_command_reset_removes_ui_and_emits_non_durable_projection_fact() {
    let (mut engine, _cwd, config_path) = preference_engine("ui_preferences_reset");
    fs::write(
        &config_path,
        "custom = 7\n[ui]\nskin = \"ice\"\nmode = \"dark\"\nfuture = \"gone\"\n",
    )
    .unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let events = engine
        .handle_runtime_command(
            "reset-ui",
            RuntimeCommand::ResetUiPreferences,
            &mut approver,
        )
        .unwrap();

    let persisted = events.iter().find_map(|event| match &event.kind {
        RuntimeEventKind::UiPreferencesUpdated { persisted, .. } => Some(persisted),
        _ => None,
    });
    assert_eq!(persisted, Some(&None));
    let value = fs::read_to_string(&config_path).unwrap();
    assert!(!value.contains("[ui]"));
    assert!(value.contains("custom = 7"));
    assert!(
        engine
            .workflow_store()
            .load_agent_events()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn ui_preferences_command_supervisor_routes_mutation_through_approval() {
    let (engine, _cwd, _config_path) = preference_engine("ui_preferences_supervisor");
    let supervisor = RuntimeSupervisor::start(engine);
    supervisor
        .send_command(
            "supervised-ui",
            RuntimeCommand::SetUiPreferences {
                patch: UiPreferencePatch {
                    skin: Some(UiSkin::Ice),
                    mode: Some(UiColorMode::Light),
                    ..UiPreferencePatch::default()
                },
            },
        )
        .unwrap();

    let events = collect_until(&supervisor, |events| {
        events.iter().any(|event| {
            matches!(
                event.kind,
                RuntimeEventKind::ApprovalRequested { .. }
                    | RuntimeEventKind::CommandRejected { .. }
            )
        })
    });
    assert!(
        events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::ApprovalRequested { .. }))
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::CommandRejected { .. }))
    );
}

#[test]
fn ui_preferences_command_supervisor_success_updates_live_snapshot_and_replay() {
    let (engine, _cwd, config_path) = preference_engine("ui_preferences_supervisor_success");
    let supervisor = RuntimeSupervisor::start(engine);
    supervisor
        .send_command(
            "supervised-ui-success",
            RuntimeCommand::SetUiPreferences {
                patch: UiPreferencePatch {
                    locale: Some(LocaleId::ZhCn),
                    skin: Some(UiSkin::Mono),
                    mode: Some(UiColorMode::Light),
                    ..UiPreferencePatch::default()
                },
            },
        )
        .unwrap();
    let pending = collect_until(&supervisor, |events| {
        events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::ApprovalRequested { .. }))
    });
    let request_id = pending
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::ApprovalRequested { approval } => Some(approval.id.clone()),
            _ => None,
        })
        .unwrap();
    supervisor
        .send_command(
            "approve-ui",
            RuntimeCommand::RespondToApproval {
                request_id,
                response: ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();

    let success = collect_until(&supervisor, |events| {
        events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::UiPreferencesUpdated { .. }))
    });
    assert!(success.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::UiPreferencesUpdated { resolved, .. }
            if resolved.locale == LocaleId::ZhCn && resolved.mode == UiColorMode::Light
    )));
    let snapshot = supervisor.snapshot_envelope().unwrap();
    assert_eq!(
        snapshot.view.ui_preferences,
        snapshot.view.snapshot.ui_preferences
    );
    assert_eq!(snapshot.view.ui_preferences.locale, LocaleId::ZhCn);
    assert!(fs::read_to_string(config_path).unwrap().contains("zh-CN"));
}

#[test]
fn ui_preferences_command_supervisor_rejects_invalid_profile_without_approval() {
    let (engine, cwd, config_path) = preference_engine("ui_preferences_supervisor_invalid");
    let original = b"[ui]\nskin = \"amber\"\nmode = \"dark\"\n";
    fs::write(&config_path, original).unwrap();
    let supervisor = RuntimeSupervisor::start(engine);
    supervisor
        .send_command(
            "supervised-invalid-ui",
            RuntimeCommand::SetUiPreferences {
                patch: UiPreferencePatch {
                    mode: Some(UiColorMode::Light),
                    ..UiPreferencePatch::default()
                },
            },
        )
        .unwrap();

    let events = collect_until(&supervisor, |events| {
        events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::CommandRejected { .. }))
    });
    assert!(
        !events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::ApprovalRequested { .. }))
    );
    assert_eq!(fs::read(&config_path).unwrap(), original);
    assert!(ui_temp_files(&cwd).is_empty());
}

fn collect_until(
    supervisor: &RuntimeSupervisor,
    done: impl Fn(&[viden_types::RuntimeEvent]) -> bool,
) -> Vec<viden_types::RuntimeEvent> {
    let started = Instant::now();
    let mut events = Vec::new();
    while started.elapsed() < Duration::from_secs(10) {
        if let Some(event) = supervisor.recv_event_timeout(Duration::from_millis(50)) {
            events.push(event);
            if done(&events) {
                return events;
            }
        }
    }
    panic!("timed out waiting for UI preference supervisor events: {events:#?}");
}

fn ui_temp_files(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(".ui-") && name.ends_with(".tmp"))
        })
        .collect()
}

// --- `runtime.starter_lane_agent` (GUI-CORE-030) -----------------------------

/// The operator's Agent pick travels with the request, changes the preview
/// hash, and lands on the receipt — and a pick this Core cannot honor leaves
/// the Lane standing.
///
/// The two halves are deliberately one test: "the binding reached Core" and
/// "the Lane survived a refused session" are the same guarantee seen from
/// either side, and splitting them would let one pass while the other
/// regressed.
#[test]
fn a_starter_lane_binding_reaches_the_receipt_and_never_unwinds_the_lane() {
    let repo = starter_lane_repo("starter_lane_agent_repo");
    let home = temp_dir("starter_lane_agent_home");
    let effects = Arc::new(StarterLaneEffects::default());
    let supervisor = starter_supervisor(&repo, home, Arc::clone(&effects));

    let binding = viden_types::StarterLaneAgentBinding {
        // No adapter of this name is registered in the test process, so the
        // session start is refused and the receipt says so.
        agent_id: "agent-that-is-not-installed".to_string(),
        model: Some("some-model".to_string()),
        task: Some("wire the binding".to_string()),
    };
    let lane_id = "lane-agent-binding";
    let owner = starter_owner(lane_id);
    let bound_request = StarterLaneRequest {
        agent: Some(binding.clone()),
        ..starter_request(lane_id, StarterLanePreset::Coder)
    };

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "agent-binding-preview",
            RuntimeCommand::PreviewStarterLane {
                request: bound_request.clone(),
            },
        )
        .unwrap();
    let events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewed { .. },
                    ..
                })
            )
        })
    });
    let preview = events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewed { preview },
                ..
            }) => Some(preview.clone()),
            _ => None,
        })
        .unwrap();

    // Echoed, not dropped, and named in a diagnostic rather than refused.
    assert_eq!(preview.agent.as_ref(), Some(&binding));
    assert!(
        preview.diagnostics.iter().any(
            |diagnostic| diagnostic == "starter_lane.agent_unknown:agent-that-is-not-installed"
        ),
        "an unknown adapter is a diagnostic, not a rejection: {:?}",
        preview.diagnostics
    );

    // Create against the bound preview. The Lane is created and the receipt
    // carries the refusal rather than the create failing.
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "agent-binding-create",
            RuntimeCommand::CreateStarterLane {
                request: bound_request,
                preview_id: preview.preview_id.clone(),
                content_sha256: preview.content_sha256.clone(),
            },
        )
        .unwrap();
    // The Lane effect itself is gated like every other Lane mutation; the
    // binding changes nothing about that.
    let approval_events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { approval },
                    ..
                }) if approval.tool_name == "lane_create"
            )
        })
    });
    let approval_id = approval_events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(approval.id.clone()),
            _ => None,
        })
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "agent-binding-approve",
            RuntimeCommand::RespondToApproval {
                request_id: approval_id,
                response: ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    let created = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLaneCreated { .. },
                    ..
                })
            )
        })
    });
    let receipt = created
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLaneCreated { receipt },
                ..
            }) => Some(receipt.clone()),
            _ => None,
        })
        .unwrap();

    assert_eq!(receipt.preview_id, preview.preview_id);
    match receipt.session.as_ref() {
        Some(viden_types::StarterLaneSessionOutcome::Refused { reason }) => {
            assert!(
                reason.contains("agent-that-is-not-installed"),
                "the refusal names the adapter: {reason}"
            );
        }
        other => panic!("an unknown adapter must be refused, got {other:?}"),
    }
    assert_eq!(receipt.lane.id, lane_id, "the Lane exists anyway");
    assert!(
        effects.created_lane_ids().contains(&lane_id.to_string()),
        "the Lane effect ran despite the refused session"
    );
}

/// A request with no binding is the flow that already existed, byte for byte.
#[test]
fn a_starter_lane_request_without_a_binding_encodes_exactly_as_before() {
    let request = starter_request("lane-no-agent", StarterLanePreset::Coder);

    let encoded = serde_json::to_string(&request).unwrap();

    assert!(request.agent.is_none());
    assert!(
        !encoded.contains("\"agent\""),
        "the field must be omitted entirely: {encoded}"
    );
    assert_eq!(
        encoded,
        "{\"lane_id\":\"lane-no-agent\",\"preset\":\"coder\",\"branch\":null,\"worktree_path\":null}"
    );
}

/// The preview hash covers the binding, so changing the pick after review
/// invalidates the preview exactly as changing the branch does.
///
/// Without this, an operator could review a Lane bound to one Agent and create
/// it bound to another — or to none — and the receipt would claim the reviewed
/// preview produced it.
#[test]
fn changing_the_agent_pick_after_review_invalidates_the_starter_lane_preview() {
    let repo = starter_lane_repo("starter_lane_agent_changed_repo");
    let home = temp_dir("starter_lane_agent_changed_home");
    let effects = Arc::new(StarterLaneEffects::default());
    let supervisor = starter_supervisor(&repo, home, Arc::clone(&effects));

    let lane_id = "lane-agent-changed";
    let owner = starter_owner(lane_id);
    let reviewed = StarterLaneRequest {
        agent: Some(viden_types::StarterLaneAgentBinding {
            agent_id: "claude-code".to_string(),
            model: None,
            task: None,
        }),
        ..starter_request(lane_id, StarterLanePreset::Coder)
    };

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "agent-changed-preview",
            RuntimeCommand::PreviewStarterLane {
                request: reviewed.clone(),
            },
        )
        .unwrap();
    let events = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewed { .. },
                    ..
                })
            )
        })
    });
    let preview = events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::StarterLanePreviewed { preview },
                ..
            }) => Some(preview.clone()),
            _ => None,
        })
        .unwrap();

    // Same lane, same branch, same worktree — only the pick is gone.
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "agent-changed-create",
            RuntimeCommand::CreateStarterLane {
                request: starter_request(lane_id, StarterLanePreset::Coder),
                preview_id: preview.preview_id.clone(),
                content_sha256: preview.content_sha256.clone(),
            },
        )
        .unwrap();
    let settled = collect_starter_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewInvalidated { .. },
                    ..
                })
            )
        })
    });

    assert!(settled.iter().any(|envelope| matches!(
        &envelope.event,
        RuntimeWireEvent::Known(RuntimeEvent {
            kind: RuntimeEventKind::StarterLanePreviewInvalidated { reason, preview_id, .. },
            ..
        }) if *reason == StarterLanePreviewInvalidationReason::RequestChanged
            && preview_id == &preview.preview_id
    )));
    assert!(
        effects.created_lane_ids().is_empty(),
        "an invalidated preview must not create a Lane"
    );
}
