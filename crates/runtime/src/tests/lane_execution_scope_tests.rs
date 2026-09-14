//! Where a Lane-owned turn's tools run, and who owns a Lane after a restart
//! (C17, E3c defects 28 and 34).
//!
//! Two claims, each of which the `0.3.5` native run found false:
//!
//! 1. A tool called inside a turn whose owner carries a `lane_id` resolves its
//!    paths and runs its shells against that Lane's worktree, never against the
//!    session's workspace root. A Lane that has no worktree keeps the root,
//!    because that *is* its execution scope.
//! 2. A Lane restored from disk after a restart has exactly one Core execution
//!    owner again, naming the new process's session, so a Lane-owned submit is
//!    accepted instead of being refused forever.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use viden_types::{
    AgentLaneRecord, AgentRole, AgentRoute, ApprovalResponse, DataEgressPolicy, ExecutionTarget,
    GateStrength, LaneBudget, LaneStatus, ModelEvent, MutationPolicy, RuntimeCommand, RuntimeEvent,
    RuntimeEventKind, RuntimeOwner, ToolCall, ToolInput,
};

use super::{SequenceProvider, temp_dir};
use crate::{RuntimeSupervisor, SessionEngine};

fn run_git(repo: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} in {} failed: {}",
        repo.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A real repository with one commit, which is what a Lane worktree needs.
fn committed_repo(name: &str) -> PathBuf {
    let repo = temp_dir(name);
    run_git(&repo, &["init", "-b", "main"]);
    run_git(&repo, &["config", "user.email", "viden@example.com"]);
    run_git(&repo, &["config", "user.name", "Viden Test"]);
    fs::write(repo.join("README.md"), "root readme\n").unwrap();
    run_git(&repo, &["add", "README.md"]);
    run_git(&repo, &["commit", "-m", "initial"]);
    repo
}

fn lane_record(id: &str, worktree: Option<&str>) -> AgentLaneRecord {
    AgentLaneRecord {
        id: id.to_string(),
        task_id: None,
        role: AgentRole::Coder,
        route: AgentRoute::BuiltIn,
        gate_strength: GateStrength::Containment,
        mutation_policy: MutationPolicy::ProposeOnly,
        worktree: worktree.map(str::to_string),
        branch: worktree.map(|_| format!("codex/{id}")),
        target: ExecutionTarget::Local,
        data_egress: DataEgressPolicy::Deny,
        status: LaneStatus::Draft,
        budget: LaneBudget::default(),
        active_session_ids: Vec::new(),
        summary: "ready".to_string(),
        evidence: Vec::new(),
        run_stats: None,
    }
}

fn edit_call(id: &str, path: &str, old: &str, new: &str) -> ToolCall {
    let mut input = ToolInput::new();
    input.insert("path".to_string(), path.to_string());
    input.insert("old".to_string(), old.to_string());
    input.insert("new".to_string(), new.to_string());
    ToolCall {
        id: id.to_string(),
        name: "edit_file".to_string(),
        input,
    }
}

fn shell_call(id: &str, command: &str) -> ToolCall {
    let mut input = ToolInput::new();
    input.insert("command".to_string(), command.to_string());
    ToolCall {
        id: id.to_string(),
        name: "shell".to_string(),
        input,
    }
}

fn collect_until(
    supervisor: &RuntimeSupervisor,
    what: &str,
    done: impl Fn(&[RuntimeEvent]) -> bool,
) -> Vec<RuntimeEvent> {
    let started = std::time::Instant::now();
    let mut events = Vec::new();
    while started.elapsed() < Duration::from_secs(30) {
        if let Some(event) = supervisor.recv_event_timeout(Duration::from_millis(50)) {
            events.push(event);
            if done(&events) {
                return events;
            }
        }
    }
    panic!("timed out waiting for {what}; collected: {events:#?}");
}

/// Answers the next approval whose tool matches, and returns the request it
/// answered so the caller can assert on what the operator was shown.
fn allow_next_approval(
    supervisor: &RuntimeSupervisor,
    tool_name: &str,
) -> viden_types::ApprovalRequestView {
    let events = collect_until(
        supervisor,
        &format!("an approval for `{tool_name}`"),
        |events| {
            events.iter().any(|event| {
                matches!(&event.kind, RuntimeEventKind::ApprovalRequested { approval }
                if approval.tool_name == tool_name)
            })
        },
    );
    let approval = events
        .iter()
        .rev()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::ApprovalRequested { approval } if approval.tool_name == tool_name => {
                Some(approval.clone())
            }
            _ => None,
        })
        .expect("the approval this wait returned on");
    supervisor
        .send_command_from_owner(
            approval.owner.clone(),
            format!("allow-{}", approval.id),
            RuntimeCommand::RespondToApproval {
                request_id: approval.id.clone(),
                response: ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    approval
}

/// The output Core published for the last completed call of one tool.
///
/// Matched on the tool name rather than the call id: the projection that turns
/// engine events into `ToolCallFinished` synthesizes its own id, so the id the
/// provider chose is not what arrives on the wire.
fn tool_output(events: &[RuntimeEvent], tool_name: &str) -> String {
    events
        .iter()
        .rev()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::ToolCallFinished { name, evidence, .. } if name == tool_name => {
                evidence.as_ref().map(|evidence| evidence.summary.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no completed `{tool_name}` call in {events:#?}"))
}

/// Creates a Lane through the real effect executor, waiting for the create
/// receipt the Lane worker publishes once the worktree is on disk.
fn create_lane(supervisor: &RuntimeSupervisor, owner: &RuntimeOwner, lane: AgentLaneRecord) {
    let lane_id = lane.id.clone();
    supervisor
        .send_command_from_owner(
            owner.clone(),
            format!("create-{lane_id}"),
            RuntimeCommand::CreateLane { lane },
        )
        .unwrap();
    allow_next_approval(supervisor, "lane_create");
    collect_until(supervisor, "the Lane create receipt", |events| {
        events.iter().any(|event| {
            matches!(
                &event.kind,
                RuntimeEventKind::LaneOutputAppended { lane_id: id, stream, .. }
                    if *id == lane_id && stream == "receipt"
            )
        })
    });
}

/// Defect 28. The Lane-owned turn's `edit_file` writes the Lane worktree's
/// file, the approval preview names that file, and a `shell` in the same Lane
/// runs there too. Before the fix all three used the workspace root, so the
/// product could not dirty the worktree it had just created.
#[test]
fn a_lane_owned_turns_tools_run_in_the_lane_worktree() {
    let repo = committed_repo("lane_tools_worktree_repo");
    let home = temp_dir("lane_tools_worktree_home");
    let lane_id = "lane-worktree-scope";

    let provider = Box::new(SequenceProvider::new(vec![
        vec![ModelEvent::ToolCall(edit_call(
            "tool_lane_edit",
            "README.md",
            "root readme",
            "lane readme",
        ))],
        vec![ModelEvent::AssistantText {
            content: "edited".to_string(),
        }],
        vec![ModelEvent::ToolCall(shell_call("tool_lane_pwd", "pwd"))],
        vec![ModelEvent::AssistantText {
            content: "printed".to_string(),
        }],
    ]));
    let engine = SessionEngine::new_with_home(&repo, provider, Some(home)).unwrap();
    let session_id = engine.session_id().to_string();
    let supervisor = RuntimeSupervisor::start(engine);

    let lane_owner = RuntimeOwner {
        workspace_id: "workspace-lane-scope".to_string(),
        project_id: "project-lane-scope".to_string(),
        lane_id: Some(lane_id.to_string()),
        session_id: Some(session_id.clone()),
        task_id: None,
        turn_id: None,
    };
    create_lane(
        &supervisor,
        &lane_owner,
        lane_record(lane_id, Some(&format!(".worktrees/{lane_id}"))),
    );
    let worktree = repo.join(".worktrees").join(lane_id);
    assert!(
        worktree.join("README.md").is_file(),
        "the real effect executor created the worktree at {}",
        worktree.display()
    );
    // The two copies are made to differ so the preview's `base_sha256` can
    // only match one of them: identical trees would prove nothing about which
    // file Core read.
    fs::write(repo.join("README.md"), "root readme\nroot only\n").unwrap();
    let worktree_base_sha = sha256_of(&worktree.join("README.md"));

    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "lane-edit",
            RuntimeCommand::SubmitUserInput {
                content: "edit the readme".to_string(),
            },
        )
        .unwrap();
    let approval = allow_next_approval(&supervisor, "edit_file");
    let events = collect_until(&supervisor, "the Lane turn to finish", |events| {
        events
            .iter()
            .any(|event| matches!(&event.kind, RuntimeEventKind::TurnFinished { .. }))
    });

    // The file the Lane owns changed; the file the workspace root owns did not.
    assert_eq!(
        fs::read_to_string(worktree.join("README.md")).unwrap(),
        "lane readme\n",
        "the Lane worktree's README carries the edit"
    );
    assert_eq!(
        fs::read_to_string(repo.join("README.md")).unwrap(),
        "root readme\nroot only\n",
        "the workspace root's README is untouched by a Lane-owned turn"
    );

    // What the tool reported, and what the operator was shown before allowing
    // it, both name the worktree.
    let output = tool_output(&events, "edit_file");
    assert!(
        output.contains(&worktree.join("README.md").display().to_string()),
        "the tool's reported path is under the Lane worktree, got: {output}"
    );
    let preview_paths = approval
        .decision_context
        .as_ref()
        .and_then(|context| context.diff.as_ref())
        .map(|diff| {
            diff.files
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert_eq!(
        preview_paths,
        vec!["README.md".to_string()],
        "the approval preview names the worktree file it resolved"
    );
    let base = approval
        .decision_context
        .as_ref()
        .and_then(|context| context.base_sha256.clone())
        .expect("the preview hashed the file it read");
    assert_eq!(
        base, worktree_base_sha,
        "the preview hashed the Lane worktree's file, not the workspace root's"
    );

    // The same Lane, a second turn, a shell: the working directory it prints is
    // the worktree.
    supervisor
        .send_command_from_owner(
            lane_owner,
            "lane-pwd",
            RuntimeCommand::SubmitUserInput {
                content: "print the working directory".to_string(),
            },
        )
        .unwrap();
    allow_next_approval(&supervisor, "shell");
    let shell_events = collect_until(&supervisor, "the shell turn to finish", |events| {
        events
            .iter()
            .any(|event| matches!(&event.kind, RuntimeEventKind::TurnFinished { .. }))
    });
    let printed = tool_output(&shell_events, "shell");
    let expected = worktree.canonicalize().unwrap();
    assert_eq!(
        PathBuf::from(printed.trim()).canonicalize().ok(),
        Some(expected),
        "`pwd` in a Lane-owned turn prints the Lane worktree"
    );
}

/// The same repository, a Lane with no worktree. Its execution scope *is* the
/// workspace root, so nothing moves — the fix must not redirect a Lane that
/// never had a worktree to redirect to.
#[test]
fn a_lane_without_a_worktree_keeps_the_workspace_root() {
    let repo = temp_dir("lane_tools_no_worktree_repo");
    let home = temp_dir("lane_tools_no_worktree_home");
    fs::write(repo.join("README.md"), "root readme\n").unwrap();
    let lane_id = "lane-no-worktree";

    let provider = Box::new(SequenceProvider::new(vec![
        vec![ModelEvent::ToolCall(edit_call(
            "tool_rootless_edit",
            "README.md",
            "root readme",
            "still root",
        ))],
        vec![ModelEvent::AssistantText {
            content: "edited".to_string(),
        }],
    ]));
    let engine = SessionEngine::new_with_home(&repo, provider, Some(home)).unwrap();
    let session_id = engine.session_id().to_string();
    let supervisor = RuntimeSupervisor::start(engine);

    let lane_owner = RuntimeOwner {
        workspace_id: "workspace-rootless".to_string(),
        project_id: "project-rootless".to_string(),
        lane_id: Some(lane_id.to_string()),
        session_id: Some(session_id),
        task_id: None,
        turn_id: None,
    };
    create_lane(&supervisor, &lane_owner, lane_record(lane_id, None));

    supervisor
        .send_command_from_owner(
            lane_owner,
            "rootless-edit",
            RuntimeCommand::SubmitUserInput {
                content: "edit the readme".to_string(),
            },
        )
        .unwrap();
    allow_next_approval(&supervisor, "edit_file");
    let events = collect_until(&supervisor, "the rootless Lane turn to finish", |events| {
        events
            .iter()
            .any(|event| matches!(&event.kind, RuntimeEventKind::TurnFinished { .. }))
    });

    assert_eq!(
        fs::read_to_string(repo.join("README.md")).unwrap(),
        "still root\n",
        "a Lane with no worktree still runs in the workspace root"
    );
    let output = tool_output(&events, "edit_file");
    assert!(
        output.contains(&repo.join("README.md").display().to_string()),
        "the reported path is the workspace root's file, got: {output}"
    );
}

fn sha256_of(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}

/// Defect 34. A Lane created before a restart must have exactly one Core
/// execution owner after it, naming the *new* process's session, and a
/// Lane-owned submit against that owner must become a turn.
///
/// Before the fix the restored Lane had no binding at all — the snapshot
/// prefix republishes `LaneUpdated` for every durable Lane and deliberately
/// publishes no owner — so every client predicate that requires exactly one
/// owner found none, and the durable `lane-agent-bindings` row still named the
/// dead session, so the submit was refused as already bound.
#[test]
fn a_restored_lane_regains_one_execution_owner_naming_the_new_session() {
    let repo = committed_repo("lane_restore_owner_repo");
    let home = temp_dir("lane_restore_owner_home");
    let lane_id = "lane-restore-owner";

    // First process: create the Lane and run one Lane-owned turn, which is
    // what writes the durable Lane-agent binding under this session.
    let first_session = {
        let provider = Box::new(SequenceProvider::new(vec![vec![
            ModelEvent::AssistantText {
                content: "first".to_string(),
            },
        ]]));
        let engine = SessionEngine::new_with_home(&repo, provider, Some(home.clone())).unwrap();
        let session_id = engine.session_id().to_string();
        let supervisor = RuntimeSupervisor::start(engine);
        let owner = RuntimeOwner {
            workspace_id: "workspace-restore".to_string(),
            project_id: "project-restore".to_string(),
            lane_id: Some(lane_id.to_string()),
            session_id: Some(session_id.clone()),
            task_id: None,
            turn_id: None,
        };
        create_lane(
            &supervisor,
            &owner,
            lane_record(lane_id, Some(&format!(".worktrees/{lane_id}"))),
        );
        supervisor
            .send_command_from_owner(
                owner,
                "first-turn",
                RuntimeCommand::SubmitUserInput {
                    content: "hello".to_string(),
                },
            )
            .unwrap();
        collect_until(&supervisor, "the first process's turn", |events| {
            events
                .iter()
                .any(|event| matches!(&event.kind, RuntimeEventKind::TurnFinished { .. }))
        });
        drop(supervisor);
        session_id
    };

    // Second process: the same VIDEN_HOME, a new session.
    let provider = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::AssistantText {
            content: "second".to_string(),
        },
    ]]));
    let engine = SessionEngine::new_with_home(&repo, provider, Some(home)).unwrap();
    let second_session = engine.session_id().to_string();
    assert_ne!(
        first_session, second_session,
        "the restart mints a new session, which is the whole premise"
    );
    let supervisor = RuntimeSupervisor::start(engine);

    let view = supervisor.snapshot_envelope().unwrap().view;
    assert!(
        view.lanes.iter().any(|lane| lane.id == lane_id),
        "the Lane row is restored"
    );
    let bindings = view
        .lane_runtime_owners
        .iter()
        .filter(|binding| binding.lane_id == lane_id)
        .collect::<Vec<_>>();
    assert_eq!(
        bindings.len(),
        1,
        "a restored Lane has exactly one Core execution owner, got {:?}",
        view.lane_runtime_owners
    );
    let restored_owner = bindings[0].owner.clone();
    assert_eq!(
        restored_owner.lane_id.as_deref(),
        Some(lane_id),
        "the binding's owner names its own Lane"
    );
    assert_eq!(
        restored_owner.session_id.as_deref(),
        Some(second_session.as_str()),
        "the restored owner names the new process's session, not the dead one"
    );

    // And the owner a client derives from that binding can actually work.
    supervisor
        .send_command_from_owner(
            restored_owner,
            "restored-turn",
            RuntimeCommand::SubmitUserInput {
                content: "work after the restart".to_string(),
            },
        )
        .unwrap();
    let events = collect_until(&supervisor, "the restored Lane's turn", |events| {
        events.iter().any(|event| {
            matches!(
                &event.kind,
                RuntimeEventKind::TurnFinished { .. } | RuntimeEventKind::CommandRejected { .. }
            )
        })
    });
    let rejection = events.iter().find_map(|event| match &event.kind {
        RuntimeEventKind::CommandRejected { reason, .. } => Some(reason.clone()),
        _ => None,
    });
    assert_eq!(
        rejection, None,
        "a Lane-owned submit against the restored owner is accepted"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(&event.kind, RuntimeEventKind::TurnStarted { .. })),
        "the accepted submit became a turn"
    );
}
