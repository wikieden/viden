//! The workspace-scoped operator identity (`runtime.workspace_owner`,
//! GUI-CORE-027).
//!
//! Before this capability there was exactly one way to obtain a real
//! `RuntimeOwner`: run inside a Lane. A commit made from the cockpit with no
//! Lane selected therefore had no actor, and both clients refused it locally
//! rather than file an authorized source-control change under
//! `RuntimeOwner::default()`, which names nobody.
//!
//! What these tests hold in place:
//!
//! 1. the workspace id is derived from the canonical root and stable across
//!    opens of the same directory, while the project id is durable and
//!    survives a move;
//! 2. the binding is the first fact after `SnapshotUpdated`, so a snapshot
//!    replay carries it;
//! 3. a workspace-target operator git action under the bound owner runs end to
//!    end and is audited under that owner, while an empty owner is refused by
//!    command id and never spawns `git`;
//! 4. a Lane's own source arrives as `LaneSourceUpdated` and never overwrites
//!    the workspace chip with another tree's numbers.

use std::fs;
use std::path::PathBuf;

use viden_types::{
    ApprovalResponse, OperatorGitAction, ProjectIdOrigin, RuntimeCommand, RuntimeEvent,
    RuntimeEventKind, RuntimeOwner, SourceTarget, WORKSPACE_ID_DIGEST_CHARS, WORKSPACE_ID_PREFIX,
};

use super::{SequenceProvider, temp_dir};
use crate::{SessionEngine, mint_workspace_owner_binding};

fn git(cwd: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed in {cwd:?}");
}

fn init_repo(cwd: &std::path::Path) {
    git(cwd, &["init", "--initial-branch=main"]);
    git(cwd, &["config", "user.email", "core@example.invalid"]);
    git(cwd, &["config", "user.name", "Core Test"]);
    git(cwd, &["config", "commit.gpgsign", "false"]);
}

/// A workspace with one committed file, one uncommitted edit, and a bound
/// workspace owner.
fn bound_workspace(name: &str) -> (PathBuf, SessionEngine, RuntimeOwner) {
    let cwd = temp_dir(&format!("{name}_cwd"));
    let home = temp_dir(&format!("{name}_home"));
    init_repo(&cwd);
    fs::write(cwd.join("tracked.txt"), "one\n").unwrap();
    git(&cwd, &["add", "tracked.txt"]);
    git(&cwd, &["commit", "-m", "initial"]);
    fs::write(cwd.join("tracked.txt"), "one\ntwo\n").unwrap();
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(
            Vec::<Vec<viden_types::ModelEvent>>::new(),
        )),
        Some(home),
    )
    .unwrap();
    let binding = mint_workspace_owner_binding(&cwd).unwrap();
    let owner = binding.owner.clone();
    engine.bind_workspace_owner(binding);
    (cwd, engine, owner)
}

/// The workspace id names a location: the same canonical root always mints the
/// same id, and a different root mints a different one. The project id is the
/// durable half and is read back rather than re-minted.
#[test]
fn minting_is_stable_for_a_root_and_distinct_between_roots() {
    let first = temp_dir("workspace_owner_mint_first");
    let second = temp_dir("workspace_owner_mint_second");

    let a = mint_workspace_owner_binding(&first).unwrap();
    let b = mint_workspace_owner_binding(&first).unwrap();
    let c = mint_workspace_owner_binding(&second).unwrap();

    a.validate().unwrap();
    assert!(a.owner.workspace_id.starts_with(WORKSPACE_ID_PREFIX));
    assert_eq!(
        a.owner.workspace_id.len(),
        WORKSPACE_ID_PREFIX.len() + WORKSPACE_ID_DIGEST_CHARS
    );
    assert!(
        a.owner.workspace_id[WORKSPACE_ID_PREFIX.len()..]
            .chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
    );
    assert_eq!(a.owner.workspace_id, b.owner.workspace_id);
    assert_ne!(a.owner.workspace_id, c.owner.workspace_id);

    assert_eq!(a.project_id_origin, ProjectIdOrigin::Minted);
    assert_eq!(b.project_id_origin, ProjectIdOrigin::Existing);
    assert_eq!(a.owner.project_id, b.owner.project_id);
    assert_ne!(a.owner.project_id, c.owner.project_id);

    // The workspace scope carries no Lane, session, task, or turn.
    assert_eq!(a.owner.lane_id, None);
    assert_eq!(a.owner.session_id, None);
    assert_eq!(a.owner.task_id, None);
    assert_eq!(a.owner.turn_id, None);
}

/// The binding is published as the first fact after `SnapshotUpdated`, so a
/// client that only replays a snapshot still learns the workspace identity.
#[test]
fn the_binding_is_the_first_fact_after_the_snapshot() {
    let (_cwd, engine, owner) = bound_workspace("workspace_owner_prefix");

    let events = engine.runtime_events_for_engine_events(&[]);

    assert!(matches!(
        events[0].kind,
        RuntimeEventKind::SnapshotUpdated { .. }
    ));
    let RuntimeEventKind::WorkspaceRuntimeOwnerBound { binding } = &events[1].kind else {
        panic!(
            "the workspace owner binding must follow the snapshot, got {:?}",
            events[1].kind
        );
    };
    assert_eq!(binding.owner, owner);
    assert_eq!(engine.workspace_owner(), Some(&owner));
    assert_eq!(engine.runtime_view_state().workspace_owner, Some(owner));
}

/// An engine with no bound owner publishes no binding at all. Absence is a
/// real answer — "this Core published no workspace identity" — and a client
/// gates its workspace commit bar on it.
#[test]
fn an_unbound_engine_publishes_no_workspace_owner() {
    let cwd = temp_dir("workspace_owner_unbound_cwd");
    let home = temp_dir("workspace_owner_unbound_home");
    let engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(
            Vec::<Vec<viden_types::ModelEvent>>::new(),
        )),
        Some(home),
    )
    .unwrap();

    assert_eq!(engine.workspace_owner(), None);
    assert!(
        !engine
            .runtime_events_for_engine_events(&[])
            .iter()
            .any(|event| matches!(
                event.kind,
                RuntimeEventKind::WorkspaceRuntimeOwnerBound { .. }
            ))
    );
    assert!(engine.runtime_view_state().workspace_owner.is_none());
}

fn run_workspace_git(
    engine: &mut SessionEngine,
    command_id: &str,
    owner: RuntimeOwner,
    action: OperatorGitAction,
) -> Vec<RuntimeEvent> {
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    engine
        .handle_runtime_command(
            command_id,
            RuntimeCommand::RunOperatorGitAction {
                owner,
                target: SourceTarget::Workspace,
                action,
            },
            &mut approver,
        )
        .unwrap()
}

/// The whole point of the capability: a workspace-target stage and commit run
/// end to end under the published owner, and the audit record names that
/// owner rather than nobody.
#[test]
fn a_workspace_target_action_runs_and_is_audited_under_the_bound_owner() {
    let (cwd, mut engine, owner) = bound_workspace("workspace_owner_commit");

    let staged = run_workspace_git(
        &mut engine,
        "ws-stage",
        owner.clone(),
        OperatorGitAction::Stage { paths: Vec::new() },
    );
    let audit_id = staged
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::OperatorGitActionFinished {
                command_id,
                outcome,
                audit_id,
                ..
            } if command_id == "ws-stage" => {
                assert!(
                    matches!(outcome, viden_types::OperatorGitOutcome::Completed { .. }),
                    "the stage must complete, got {outcome:?}"
                );
                Some(audit_id.clone())
            }
            _ => None,
        })
        .expect("a workspace-target stage settles under the bound owner");

    let audit_events = engine
        .query_audit(
            "ws-audit-read",
            viden_types::AuditQuery {
                limit: 50,
                ..viden_types::AuditQuery::default()
            },
        )
        .expect("the audit timeline is readable");
    let page = audit_events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::AuditPageLoaded { page, .. } => Some(page.clone()),
            _ => None,
        })
        .expect("the audit read answers with a page");
    let record = page
        .records
        .iter()
        .find(|record| record.audit_id == audit_id)
        .expect("the authorization record is in the timeline");
    assert_eq!(record.owner, owner);
    assert_eq!(record.action, "source.stage");

    let committed = run_workspace_git(
        &mut engine,
        "ws-commit",
        owner,
        OperatorGitAction::Commit {
            message: "feat(core): commit without a Lane".to_string(),
        },
    );
    assert!(committed.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::OperatorGitActionFinished { outcome, .. }
            if matches!(outcome, viden_types::OperatorGitOutcome::Completed { .. })
    )));
    let log = std::process::Command::new("git")
        .args(["log", "-1", "--pretty=%s"])
        .current_dir(&cwd)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&log.stdout).trim(),
        "feat(core): commit without a Lane"
    );
}

/// An owner that names no workspace is refused by command id, before any
/// process spawns, and the refusal names GUI-CORE-027 so an operator reading
/// it knows which capability is missing rather than only that they were
/// refused.
#[test]
fn an_empty_owner_is_refused_and_names_the_contract_request() {
    let (cwd, mut engine, _owner) = bound_workspace("workspace_owner_empty");
    let before = fs::read_to_string(cwd.join("tracked.txt")).unwrap();

    let events = run_workspace_git(
        &mut engine,
        "ws-anonymous",
        RuntimeOwner::default(),
        OperatorGitAction::Stage { paths: Vec::new() },
    );

    let reason = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, reason }
                if command_id == "ws-anonymous" =>
            {
                Some(reason.clone())
            }
            _ => None,
        })
        .expect("an anonymous workspace action is refused by command id");
    assert!(reason.contains("GUI-CORE-027"), "reason was `{reason}`");
    assert!(
        !events.iter().any(|event| matches!(
            event.kind,
            RuntimeEventKind::OperatorGitActionFinished { .. }
        )),
        "a refused action settles nothing"
    );
    let staged = std::process::Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(&cwd)
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&staged.stdout).trim().is_empty(),
        "nothing was staged"
    );
    assert_eq!(fs::read_to_string(cwd.join("tracked.txt")).unwrap(), before);
}

/// An owner naming another workspace is refused too. Accepting it would file
/// this workspace's mutation under an identity that belongs to a different
/// tree.
#[test]
fn an_owner_from_another_workspace_is_refused() {
    let (_cwd, mut engine, owner) = bound_workspace("workspace_owner_foreign");
    let foreign = RuntimeOwner {
        workspace_id: format!("{}deadbeefdeadbeef", WORKSPACE_ID_PREFIX),
        ..owner
    };

    let events = run_workspace_git(
        &mut engine,
        "ws-foreign",
        foreign,
        OperatorGitAction::Stage { paths: Vec::new() },
    );

    let reason = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, reason }
                if command_id == "ws-foreign" =>
            {
                Some(reason.clone())
            }
            _ => None,
        })
        .expect("a foreign workspace owner is refused by command id");
    assert!(reason.contains("GUI-CORE-027"), "reason was `{reason}`");
}

/// A Lane-target action publishes the Lane's own source rather than
/// overwriting `workspace_source` with the Lane worktree's branch and counts.
/// The workspace chip and the Lane chip answer different questions.
#[test]
fn a_lane_target_action_publishes_the_lane_source_not_the_workspace_source() {
    let (cwd, mut engine, owner) = bound_workspace("workspace_owner_lane_source");
    let worktree = cwd.join(".worktrees").join("lane-a");
    git(
        &cwd,
        &[
            "worktree",
            "add",
            "-b",
            "codex/lane-a",
            worktree.to_str().unwrap(),
        ],
    );
    fs::write(worktree.join("tracked.txt"), "one\nlane\n").unwrap();
    let lane_id = "lane_source_a".to_string();
    engine
        .workflow_store()
        .append_lane_event(&viden_workflows::lanes::LaneEvent::created(
            "lane_event_source_a",
            viden_types::AgentLaneRecord {
                id: lane_id.clone(),
                task_id: None,
                role: viden_types::AgentRole::Coder,
                route: viden_types::AgentRoute::BuiltIn,
                gate_strength: viden_types::GateStrength::Full,
                mutation_policy: viden_types::MutationPolicy::ProposeOnly,
                worktree: Some(worktree.to_string_lossy().to_string()),
                branch: Some("codex/lane-a".to_string()),
                target: viden_types::ExecutionTarget::Local,
                data_egress: viden_types::DataEgressPolicy::Deny,
                status: viden_types::LaneStatus::Running,
                budget: viden_types::LaneBudget::default(),
                active_session_ids: Vec::new(),
                summary: "lane source fixture".to_string(),
                evidence: Vec::new(),
                run_stats: None,
            },
            1_700_000_000,
            None,
        ))
        .unwrap();

    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let events = engine
        .handle_runtime_command(
            "lane-stage",
            RuntimeCommand::RunOperatorGitAction {
                owner,
                target: SourceTarget::Lane {
                    lane_id: lane_id.clone(),
                },
                action: OperatorGitAction::Stage { paths: Vec::new() },
            },
            &mut approver,
        )
        .unwrap();

    let source = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::LaneSourceUpdated {
                lane_id: id,
                source,
            } if *id == lane_id => Some(source.clone()),
            _ => None,
        })
        .expect("a Lane-target action publishes that Lane's source");
    assert_eq!(source.branch.as_deref(), Some("codex/lane-a"));
    assert!(
        !events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::WorkspaceSourceUpdated { .. })),
        "a Lane action must not republish the workspace source"
    );
}

/// Every live Lane with a worktree of its own gets a source row on the status
/// sampling that feeds the snapshot, and a direct-workspace Lane gets none:
/// its source *is* the workspace source, and inventing a duplicate row would
/// give a client two answers for one tree.
#[test]
fn the_status_sampling_publishes_a_row_per_live_lane_worktree() {
    let (cwd, engine, _owner) = bound_workspace("workspace_owner_lane_rows");
    let worktree = cwd.join(".worktrees").join("lane-rows");
    git(
        &cwd,
        &[
            "worktree",
            "add",
            "-b",
            "codex/lane-rows",
            worktree.to_str().unwrap(),
        ],
    );
    register_lane(
        &engine,
        "lane_with_worktree",
        Some(worktree.to_string_lossy().to_string()),
        viden_types::LaneStatus::Running,
    );
    register_lane(
        &engine,
        "lane_direct_workspace",
        None,
        viden_types::LaneStatus::Running,
    );
    register_lane(
        &engine,
        "lane_archived",
        Some(worktree.to_string_lossy().to_string()),
        viden_types::LaneStatus::Archived,
    );

    let rows = engine
        .frontend_status_lifecycle_events()
        .into_iter()
        .filter_map(|event| match event.kind {
            RuntimeEventKind::LaneSourceUpdated { lane_id, source } => Some((lane_id, source)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
        vec!["lane_with_worktree"]
    );
    assert_eq!(rows[0].1.branch.as_deref(), Some("codex/lane-rows"));
}

/// A command that carries no actor of its own is routed under the workspace
/// identity, so a Lane created after open carries the workspace and project
/// ids in its binding rather than a pair of empty strings.
#[test]
fn an_ownerless_command_is_routed_under_the_bound_workspace_identity() {
    let (_cwd, engine, owner) = bound_workspace("workspace_owner_stamp");

    let stamped = crate::runtime_supervisor::stamp_workspace_identity(
        &engine,
        RuntimeOwner {
            lane_id: Some("lane_new".to_string()),
            ..RuntimeOwner::default()
        },
        &RuntimeCommand::ProbeProject,
    );

    assert_eq!(stamped.workspace_id, owner.workspace_id);
    assert_eq!(stamped.project_id, owner.project_id);
    assert_eq!(stamped.lane_id.as_deref(), Some("lane_new"));
}

/// An owner a client already named is never rewritten, and a command that
/// carries its own actor is never stamped: the supervisor validates that the
/// actor equals the envelope owner, so silently changing one of the two would
/// either break that check or launder a mismatch past it.
#[test]
fn stamping_never_rewrites_a_named_owner_or_an_actor_bearing_command() {
    let (_cwd, engine, owner) = bound_workspace("workspace_owner_no_rewrite");
    let named = RuntimeOwner {
        workspace_id: "ws_client_named".to_string(),
        project_id: "prj_client_named".to_string(),
        ..RuntimeOwner::default()
    };

    assert_eq!(
        crate::runtime_supervisor::stamp_workspace_identity(
            &engine,
            named.clone(),
            &RuntimeCommand::ProbeProject
        ),
        named
    );
    assert_eq!(
        crate::runtime_supervisor::stamp_workspace_identity(
            &engine,
            RuntimeOwner::default(),
            &RuntimeCommand::RunOperatorGitAction {
                owner: owner.clone(),
                target: SourceTarget::Workspace,
                action: OperatorGitAction::Fetch { remote: None },
            },
        ),
        RuntimeOwner::default()
    );
}

fn register_lane(
    engine: &SessionEngine,
    lane_id: &str,
    worktree: Option<String>,
    status: viden_types::LaneStatus,
) {
    engine
        .workflow_store()
        .append_lane_event(&viden_workflows::lanes::LaneEvent::created(
            format!("lane_event_{lane_id}"),
            viden_types::AgentLaneRecord {
                id: lane_id.to_string(),
                task_id: None,
                role: viden_types::AgentRole::Coder,
                route: viden_types::AgentRoute::BuiltIn,
                gate_strength: viden_types::GateStrength::Full,
                mutation_policy: viden_types::MutationPolicy::ProposeOnly,
                worktree,
                branch: None,
                target: viden_types::ExecutionTarget::Local,
                data_egress: viden_types::DataEgressPolicy::Deny,
                status,
                budget: viden_types::LaneBudget::default(),
                active_session_ids: Vec::new(),
                summary: "lane source fixture".to_string(),
                evidence: Vec::new(),
                run_stats: None,
            },
            1_700_000_000,
            None,
        ))
        .unwrap();
}
