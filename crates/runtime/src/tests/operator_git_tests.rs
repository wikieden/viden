//! Runtime behavior of `runtime.operator_git` (GUI-CORE-020).
//!
//! The flow is the contract, so the tests walk it in order: validate, gate,
//! audit before the effect, run the shared tool, classify. Each step has a
//! test because skipping it produces a specific lie a client would render as
//! fact — a clamped path stages a file nobody asked for, a missed plan-mode
//! check mutates during planning, a missing audit record leaves an
//! unauditable source-control change, and a rejection where an outcome
//! belongs tells an operator "denied" when git actually ran.
//!
//! Every git test here uses a temporary repository, and the push tests use a
//! *local bare* repository as the remote. A test that reached a network remote
//! would be a test of the network.

use std::fs;
use std::path::{Path, PathBuf};

use viden_types::{
    ApprovalResponse, AuditActor, AuditOutcome, ModelEvent, OperatorGitAction,
    OperatorGitFailureClass, OperatorGitOutcome, PermissionBehavior, PermissionLevel,
    PermissionRule, PermissionRuleSource, PermissionRuleValue, RuntimeCommand, RuntimeEvent,
    RuntimeEventKind, RuntimeOwner, SourceTarget, WorkMode,
};

use super::{SequenceProvider, temp_dir};
use crate::SessionEngine;
use crate::operator_git::classify_operator_git_failure;

fn git(cwd: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn init_git_repo(cwd: &Path) {
    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.email", "viden@example.com"],
        vec!["config", "user.name", "Viden"],
    ] {
        git(cwd, &args);
    }
}

fn owner() -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace_operator_git".to_string(),
        project_id: "project_operator_git".to_string(),
        ..Default::default()
    }
}

fn tool_rule(tool_name: &str, rule_behavior: PermissionBehavior) -> PermissionRule {
    PermissionRule {
        source: PermissionRuleSource::Session,
        rule_behavior,
        rule_value: PermissionRuleValue {
            tool_name: tool_name.to_string(),
            rule_content: None,
        },
    }
}

/// A workspace with one committed file and one uncommitted edit.
fn operator_workspace(name: &str) -> (PathBuf, SessionEngine) {
    let cwd = temp_dir(&format!("{name}_cwd"));
    let home = temp_dir(&format!("{name}_home"));
    init_git_repo(&cwd);
    fs::write(cwd.join("tracked.txt"), "one\n").unwrap();
    git(&cwd, &["add", "tracked.txt"]);
    git(&cwd, &["commit", "-m", "initial"]);
    fs::write(cwd.join("tracked.txt"), "one\ntwo\n").unwrap();
    let engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::<Vec<ModelEvent>>::new())),
        Some(home),
    )
    .unwrap();
    (cwd, engine)
}

/// Runs one action with the approval granted.
///
/// The default permission mode asks for every mutating tool, so a granting
/// approver is what puts these tests *past* the gate and onto the flow they
/// are about. The refusal paths each build their own approver so the denial
/// under test is explicit rather than inherited.
fn run_action(
    engine: &mut SessionEngine,
    command_id: &str,
    action: OperatorGitAction,
) -> Vec<RuntimeEvent> {
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    engine
        .handle_runtime_command(
            command_id,
            RuntimeCommand::RunOperatorGitAction {
                owner: owner(),
                target: SourceTarget::Workspace,
                action,
            },
            &mut approver,
        )
        .unwrap()
}

fn rejection(events: &[RuntimeEvent], command_id: &str) -> String {
    events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::CommandRejected {
                command_id: id,
                reason,
            } if id == command_id => Some(reason.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a CommandRejected for {command_id}, got {events:?}"))
}

fn outcome(events: &[RuntimeEvent], command_id: &str) -> OperatorGitOutcome {
    events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::OperatorGitActionFinished {
                command_id: id,
                outcome,
                ..
            } if id == command_id => Some(outcome.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a finished action for {command_id}, got {events:?}"))
}

// --- 1. validate -------------------------------------------------------------

/// A path that leaves the target is refused before any process runs, and the
/// refusal names the command. Clamping it would stage a file nobody asked for;
/// answering with a clean outcome would say "nothing to do" about a request
/// that was never honored.
#[test]
fn a_stage_path_that_leaves_the_target_is_rejected_before_git_runs() {
    let (_cwd, mut engine) = operator_workspace("operator_git_escape");
    let events = run_action(
        &mut engine,
        "stage-escape",
        OperatorGitAction::Stage {
            paths: vec!["../outside.txt".to_string()],
        },
    );
    let reason = rejection(&events, "stage-escape");
    assert!(
        reason.contains("outside the repository"),
        "the refusal must name the condition: {reason}"
    );
    assert!(
        !events.iter().any(|event| matches!(
            event.kind,
            RuntimeEventKind::OperatorGitActionFinished { .. }
        )),
        "a pre-effect refusal must never also publish an outcome"
    );
}

/// An empty commit message is refused rather than passed to `git commit`,
/// which would open an editor in a non-interactive child and hang.
#[test]
fn an_empty_commit_message_is_rejected() {
    let (_cwd, mut engine) = operator_workspace("operator_git_empty_message");
    let events = run_action(
        &mut engine,
        "commit-empty",
        OperatorGitAction::Commit {
            message: "  \n".to_string(),
        },
    );
    assert!(rejection(&events, "commit-empty").contains("non-empty message"));
}

/// A Lane nobody has a record of is refused rather than answered from the
/// workspace root, which would apply one tree's action under another tree's
/// identity.
#[test]
fn an_unknown_lane_target_is_rejected() {
    let (_cwd, mut engine) = operator_workspace("operator_git_unknown_lane");
    let mut approver = |_prompt| ApprovalResponse::deny(None);
    let events = engine
        .handle_runtime_command(
            "lane-unknown",
            RuntimeCommand::RunOperatorGitAction {
                owner: owner(),
                target: SourceTarget::Lane {
                    lane_id: "lane_missing".to_string(),
                },
                action: OperatorGitAction::Stage { paths: Vec::new() },
            },
            &mut approver,
        )
        .unwrap();
    assert!(rejection(&events, "lane-unknown").contains("does not exist"));
}

// --- 2. gate -----------------------------------------------------------------

/// Plan mode refuses before any process spawns, through the permission
/// engine's own plan-mode branch rather than a call-site check that a future
/// path could forget.
#[test]
fn plan_mode_rejects_an_operator_action_before_the_effect() {
    let (cwd, mut engine) = operator_workspace("operator_git_plan");
    engine.set_work_mode(WorkMode::Plan).unwrap();
    let events = run_action(
        &mut engine,
        "stage-plan",
        OperatorGitAction::Stage { paths: Vec::new() },
    );
    let reason = rejection(&events, "stage-plan");
    assert!(reason.contains("plan mode"), "{reason}");
    let staged = std::process::Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(&cwd)
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&staged.stdout).trim().is_empty(),
        "plan mode must leave the index untouched"
    );
}

/// A deny rule on the *mapped agent spec* refuses the operator action. This is
/// the shared-vocabulary property: one `viden.toml` rule for `git_add` governs
/// an agent's stage and an operator's commit bar alike.
#[test]
fn a_deny_rule_on_the_mapped_agent_spec_refuses_the_operator_action() {
    let (_cwd, mut engine) = operator_workspace("operator_git_deny");
    engine.add_permission_rule_for_test(tool_rule("git_add", PermissionBehavior::Deny));
    let events = run_action(
        &mut engine,
        "stage-denied",
        OperatorGitAction::Stage { paths: Vec::new() },
    );
    let reason = rejection(&events, "stage-denied");
    assert!(reason.contains("git_add"), "{reason}");
    assert!(
        reason.contains("hint:"),
        "a refusal must say what to grant: {reason}"
    );
}

/// An `Ask` on a commit reaches the approver carrying the *staged* rows, which
/// are exactly what the commit will contain. Approving it lets the commit
/// through; there is no second gate to slip past.
#[test]
fn a_commit_ask_carries_the_staged_diff_and_a_grant_lets_it_through() {
    let (cwd, mut engine) = operator_workspace("operator_git_commit_ask");
    git(&cwd, &["add", "tracked.txt"]);
    engine.add_permission_rule_for_test(tool_rule("git_commit", PermissionBehavior::Ask));

    let mut seen = None;
    let mut approver = |prompt: viden_types::PermissionPrompt| {
        seen = Some(prompt);
        ApprovalResponse::allow_once(None)
    };
    let events = engine
        .handle_runtime_command(
            "commit-ask",
            RuntimeCommand::RunOperatorGitAction {
                owner: owner(),
                target: SourceTarget::Workspace,
                action: OperatorGitAction::Commit {
                    message: "fix: stage and commit".to_string(),
                },
            },
            &mut approver,
        )
        .unwrap();

    let prompt = seen.expect("a commit under an ask rule must prompt");
    assert_eq!(prompt.tool_name, "git_commit");
    let document = prompt
        .decision_context
        .expect("a commit approval must show what is being committed")
        .diff
        .expect("the staged diff");
    assert_eq!(
        document
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        vec!["tracked.txt"],
        "the context must describe the index, not the working tree"
    );

    assert!(matches!(
        outcome(&events, "commit-ask"),
        OperatorGitOutcome::Completed { .. }
    ));
}

/// A denied approval is a pre-effect refusal, not a failed outcome: nothing
/// ran, so there is nothing to report as having failed.
#[test]
fn a_denied_commit_approval_is_a_rejection_and_not_a_failed_outcome() {
    let (cwd, mut engine) = operator_workspace("operator_git_commit_deny");
    git(&cwd, &["add", "tracked.txt"]);
    engine.add_permission_rule_for_test(tool_rule("git_commit", PermissionBehavior::Ask));
    let mut approver = |_prompt| ApprovalResponse::deny(Some("not now".to_string()));
    let events = engine
        .handle_runtime_command(
            "commit-denied",
            RuntimeCommand::RunOperatorGitAction {
                owner: owner(),
                target: SourceTarget::Workspace,
                action: OperatorGitAction::Commit {
                    message: "fix: denied".to_string(),
                },
            },
            &mut approver,
        )
        .unwrap();
    assert!(rejection(&events, "commit-denied").contains("git_commit"));
    let log = std::process::Command::new("git")
        .args(["log", "--oneline"])
        .current_dir(&cwd)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&log.stdout).lines().count(),
        1,
        "a denied approval must leave HEAD where it was"
    );
}

// --- 3. audit before effect --------------------------------------------------

/// The authorization record lands before the effect and the finished event
/// names it, so an audit reader can always find the record that preceded a
/// source-control change.
#[test]
fn a_completed_action_audits_the_authorization_before_the_effect_and_names_it() {
    let (_cwd, mut engine) = operator_workspace("operator_git_audit");
    let events = run_action(
        &mut engine,
        "stage-audited",
        OperatorGitAction::Stage { paths: Vec::new() },
    );
    let audit_id = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::OperatorGitActionFinished { audit_id, .. } => Some(audit_id.clone()),
            _ => None,
        })
        .expect("a finished action");

    let records = super::audit_runtime_tests::all_audit_records(&engine);
    let authorized = records
        .iter()
        .find(|record| record.audit_id == audit_id)
        .expect("the event's audit id must name a real record");
    assert_eq!(authorized.action, "source.stage");
    assert_eq!(authorized.actor, AuditActor::Operator);
    assert_eq!(
        authorized.args.get("phase").map(String::as_str),
        Some("authorized")
    );
    assert_eq!(
        authorized.args.get("tool").map(String::as_str),
        Some("git_add")
    );
    assert!(
        authorized
            .objects
            .iter()
            .any(|object| object.kind == "source"),
        "an operator action is audited against its source target"
    );

    // The append-only log cannot amend the authorization, so the outcome
    // arrives as a second record that names it.
    let completion = records
        .iter()
        .find(|record| record.args.get("attempt") == Some(&audit_id))
        .expect("a completion record must name the attempt it settles");
    assert_eq!(completion.outcome, AuditOutcome::Success);
    assert_eq!(
        completion.args.get("phase").map(String::as_str),
        Some("completed")
    );
}

/// A Lane action is audited against the Lane as well as the source, so a
/// reader can ask what a Lane did to its worktree without joining on paths.
#[test]
fn a_lane_action_audits_against_the_lane_and_the_source() {
    let objects = crate::operator_git::operator_git_audit_objects(&SourceTarget::Lane {
        lane_id: "lane_alpha".to_string(),
    });
    let kinds = objects
        .iter()
        .map(|object| (object.kind.as_str(), object.id.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![("source", "lane_alpha"), ("lane", "lane_alpha")]
    );
}

// --- 4. effect and outcome ---------------------------------------------------

/// The happy path: the shared tool runs, the outcome is typed, and a resampled
/// source follows so a client's chip reflects what the action did.
#[test]
fn a_completed_stage_publishes_a_typed_outcome_and_a_resampled_source() {
    let (cwd, mut engine) = operator_workspace("operator_git_stage_ok");
    let events = run_action(
        &mut engine,
        "stage-ok",
        OperatorGitAction::Stage { paths: Vec::new() },
    );
    match outcome(&events, "stage-ok") {
        OperatorGitOutcome::Completed { truncated, .. } => assert!(!truncated),
        other => panic!("expected a completed stage, got {other:?}"),
    }
    let staged = std::process::Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(&cwd)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&staged.stdout).trim(),
        "tracked.txt"
    );
    // The source fact follows the outcome, in that order: a client that only
    // reduces `WorkspaceSourceUpdated` still sees the post-effect tree.
    let positions = events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match &event.kind {
            RuntimeEventKind::OperatorGitActionFinished { .. } => Some(("finished", index)),
            RuntimeEventKind::WorkspaceSourceUpdated { .. } => Some(("source", index)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(positions.len(), 2);
    assert_eq!(positions[0].0, "finished");
    assert_eq!(positions[1].0, "source");
}

/// Unstaging leaves the working tree alone. `git_restore` with both `staged`
/// and `worktree` set would discard the operator's edits, which is the one
/// thing an "unstage" must never do.
#[test]
fn an_unstage_clears_the_index_without_touching_the_working_tree() {
    let (cwd, mut engine) = operator_workspace("operator_git_unstage");
    git(&cwd, &["add", "tracked.txt"]);
    let events = run_action(
        &mut engine,
        "unstage-ok",
        OperatorGitAction::Unstage { paths: Vec::new() },
    );
    assert!(matches!(
        outcome(&events, "unstage-ok"),
        OperatorGitOutcome::Completed { .. }
    ));
    let staged = std::process::Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(&cwd)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&staged.stdout).trim().is_empty());
    assert_eq!(
        fs::read_to_string(cwd.join("tracked.txt")).unwrap(),
        "one\ntwo\n",
        "an unstage must never discard working-tree content"
    );
}

/// A commit with nothing staged is a *failure after a granted permission*, not
/// a rejection: git ran and refused. Telling the operator "denied" would send
/// them to their permission rules for a problem in their index.
#[test]
fn a_commit_with_nothing_staged_fails_rather_than_being_rejected() {
    let (_cwd, mut engine) = operator_workspace("operator_git_nothing");
    let events = run_action(
        &mut engine,
        "commit-nothing",
        OperatorGitAction::Commit {
            message: "fix: nothing staged".to_string(),
        },
    );
    match outcome(&events, "commit-nothing") {
        OperatorGitOutcome::Failed { class, detail } => {
            assert_eq!(class, OperatorGitFailureClass::NothingToCommit);
            assert!(!detail.is_empty(), "the real git text is preserved");
        }
        other => panic!("expected a failed commit, got {other:?}"),
    }
    assert!(
        !events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::CommandRejected { .. })),
        "a failure after a granted permission is never a rejection"
    );
}

/// The audit timeline records a failed effect as failed, so a reader cannot
/// mistake an authorized attempt for a completed change.
#[test]
fn a_failed_effect_appends_a_failed_completion_record() {
    let (_cwd, mut engine) = operator_workspace("operator_git_failed_audit");
    let events = run_action(
        &mut engine,
        "commit-failed-audit",
        OperatorGitAction::Commit {
            message: "fix: nothing staged".to_string(),
        },
    );
    let audit_id = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::OperatorGitActionFinished { audit_id, .. } => Some(audit_id.clone()),
            _ => None,
        })
        .expect("a finished action");
    let records = super::audit_runtime_tests::all_audit_records(&engine);
    let completion = records
        .iter()
        .find(|record| record.args.get("attempt") == Some(&audit_id))
        .expect("a completion record");
    assert_eq!(completion.outcome, AuditOutcome::Failed);
    assert_eq!(
        completion.args.get("class").map(String::as_str),
        Some("nothing_to_commit")
    );
}

/// A push to a local bare remote completes and the resampled source shows the
/// branch is no longer ahead. The remote is a bare repository on disk: a test
/// that reached the network would be a test of the network.
#[test]
fn a_push_to_a_local_bare_remote_completes_and_resamples_the_source() {
    let (cwd, mut engine) = operator_workspace("operator_git_push_ok");
    let remote = temp_dir("operator_git_push_remote");
    git(&remote, &["init", "--bare", "-b", "main"]);
    git(
        &cwd,
        &["remote", "add", "origin", &remote.display().to_string()],
    );
    git(&cwd, &["add", "tracked.txt"]);
    git(&cwd, &["commit", "-m", "second"]);
    git(&cwd, &["push", "--set-upstream", "origin", "main"]);
    fs::write(cwd.join("tracked.txt"), "one\ntwo\nthree\n").unwrap();
    git(&cwd, &["commit", "-am", "third"]);

    let events = run_action(
        &mut engine,
        "push-ok",
        OperatorGitAction::Push {
            remote: Some("origin".to_string()),
            set_upstream: false,
        },
    );
    match outcome(&events, "push-ok") {
        OperatorGitOutcome::Completed { source, .. } => {
            assert_eq!(source.ahead, 0, "a completed push clears the ahead count");
            assert!(!source.dirty);
        }
        other => panic!("expected a completed push, got {other:?}"),
    }
}

/// A push from a branch with no upstream fails with the class that tells a
/// client to offer `set_upstream`, and does not run.
///
/// `git push origin work` would *succeed* here and create an untracked remote
/// branch, after which ahead/behind is unknowable and the source chip would
/// read "in sync" forever. Refusing it as a typed failure is what keeps the
/// chip honest, and `NoUpstream` is what tells the client which button to
/// offer next.
#[test]
fn a_push_without_an_upstream_fails_with_no_upstream() {
    let (cwd, mut engine) = operator_workspace("operator_git_push_no_upstream");
    let remote = temp_dir("operator_git_no_upstream_remote");
    git(&remote, &["init", "--bare", "-b", "main"]);
    git(
        &cwd,
        &["remote", "add", "origin", &remote.display().to_string()],
    );
    git(&cwd, &["switch", "-c", "work"]);
    git(&cwd, &["add", "tracked.txt"]);
    git(&cwd, &["commit", "-m", "work"]);

    let events = run_action(
        &mut engine,
        "push-no-upstream",
        OperatorGitAction::Push {
            remote: Some("origin".to_string()),
            set_upstream: false,
        },
    );
    match outcome(&events, "push-no-upstream") {
        OperatorGitOutcome::Failed { class, .. } => {
            assert_eq!(class, OperatorGitFailureClass::NoUpstream);
        }
        other => panic!("expected a failed push, got {other:?}"),
    }
    let remote_branches = std::process::Command::new("git")
        .args(["branch", "--all"])
        .current_dir(&remote)
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&remote_branches.stdout).contains("work"),
        "the refused push must not have created an untracked remote branch"
    );
}

/// The same push with `set_upstream` completes, and the branch is tracked
/// afterwards, so the source chip can report ahead/behind truthfully.
#[test]
fn a_push_with_set_upstream_creates_the_tracking_branch() {
    let (cwd, mut engine) = operator_workspace("operator_git_push_set_upstream");
    let remote = temp_dir("operator_git_set_upstream_remote");
    git(&remote, &["init", "--bare", "-b", "main"]);
    git(
        &cwd,
        &["remote", "add", "origin", &remote.display().to_string()],
    );
    git(&cwd, &["switch", "-c", "work"]);
    git(&cwd, &["add", "tracked.txt"]);
    git(&cwd, &["commit", "-m", "work"]);

    let events = run_action(
        &mut engine,
        "push-set-upstream",
        OperatorGitAction::Push {
            remote: Some("origin".to_string()),
            set_upstream: true,
        },
    );
    match outcome(&events, "push-set-upstream") {
        OperatorGitOutcome::Completed { source, .. } => assert_eq!(source.ahead, 0),
        other => panic!("expected a completed push, got {other:?}"),
    }
    let upstream = std::process::Command::new("git")
        .args([
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ])
        .current_dir(&cwd)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&upstream.stdout).trim(),
        "origin/work"
    );
}

// --- 5. classification -------------------------------------------------------

/// The classifier's table. One function turns git's stderr into one class so
/// no client ever parses output text; every line here is a message git really
/// writes on the porcelain paths Viden drives.
#[test]
fn the_failure_classifier_maps_each_git_message_to_its_class() {
    let cases: &[(&str, OperatorGitFailureClass)] = &[
        (
            "nothing to commit, working tree clean",
            OperatorGitFailureClass::NothingToCommit,
        ),
        (
            "no changes added to commit (use \"git add\" and/or \"git commit -a\")",
            OperatorGitFailureClass::NothingToCommit,
        ),
        (
            " ! [rejected]        main -> main (fetch first)",
            OperatorGitFailureClass::NonFastForward,
        ),
        (
            "Updates were rejected because the tip of your current branch is behind",
            OperatorGitFailureClass::NonFastForward,
        ),
        (
            "fatal: Authentication failed for 'https://example.com/repo.git/'",
            OperatorGitFailureClass::AuthenticationRequired,
        ),
        (
            "fatal: could not read Username for 'https://example.com': terminal prompts disabled",
            OperatorGitFailureClass::AuthenticationRequired,
        ),
        (
            "git@example.com: Permission denied (publickey).",
            OperatorGitFailureClass::AuthenticationRequired,
        ),
        (
            "fatal: unable to access 'https://example.invalid/': Could not resolve host: example.invalid",
            OperatorGitFailureClass::RemoteUnreachable,
        ),
        (
            "ssh: connect to host example.invalid port 22: Connection refused",
            OperatorGitFailureClass::RemoteUnreachable,
        ),
        (
            "fatal: The current branch work has no upstream branch.",
            OperatorGitFailureClass::NoUpstream,
        ),
        (
            // An unconfigured remote *name* is read by git as a URL, so the
            // message is about the far end, not about tracking. Classifying it
            // as `NoUpstream` would tell a client to offer "set upstream" for
            // a remote that does not exist.
            "fatal: 'nowhere' does not appear to be a git repository",
            OperatorGitFailureClass::RemoteUnreachable,
        ),
        (
            "fatal: /etc/passwd: '/etc/passwd' is outside repository at '/work'",
            OperatorGitFailureClass::PathOutsideRepository,
        ),
        (
            "error: something nobody has seen before",
            OperatorGitFailureClass::Other,
        ),
        ("", OperatorGitFailureClass::Other),
    ];
    for (stderr, expected) in cases {
        assert_eq!(
            classify_operator_git_failure(stderr),
            *expected,
            "classifying {stderr:?}"
        );
    }
}

/// A non-fast-forward push also mentions the upstream, so the ordering inside
/// the classifier matters: the rejection wins, because "fetch first" and
/// "set an upstream" are different recoveries.
#[test]
fn a_rejected_push_classifies_as_non_fast_forward_and_not_as_no_upstream() {
    let stderr = " ! [rejected]        main -> main (non-fast-forward)\n\
                  hint: Updates were rejected because the tip of your current branch is behind\n\
                  hint: its remote counterpart. If you want to integrate the remote changes,\n\
                  hint: use 'git pull' before pushing again.";
    assert_eq!(
        classify_operator_git_failure(stderr),
        OperatorGitFailureClass::NonFastForward
    );
}

// --- 6. supervised approval shape -------------------------------------------

/// The supervised path prepares under the *same* spec the engine gates under.
/// If the two disagreed, an action could reach the approval dock and then run
/// without one, or the reverse.
#[test]
fn the_supervised_preparation_uses_the_mapped_agent_spec() {
    let (_cwd, mut engine) = operator_workspace("operator_git_supervised");
    engine.add_permission_rule_for_test(tool_rule("git_push", PermissionBehavior::Ask));
    let prepared = engine
        .prepare_project_mutation_for_supervisor(
            &owner(),
            &RuntimeCommand::RunOperatorGitAction {
                owner: owner(),
                target: SourceTarget::Workspace,
                action: OperatorGitAction::Push {
                    remote: None,
                    set_upstream: true,
                },
            },
        )
        .unwrap();
    match prepared {
        crate::project_runtime::SupervisorProjectMutationPreparation::Pending(prompt) => {
            assert_eq!(prompt.tool_name, "git_push");
            assert!(
                prompt.decision_context.is_none(),
                "a push has no local content to preview"
            );
        }
        other => panic!("expected a pending approval, got {other:?}"),
    }
}

/// A command whose actor does not match its envelope owner is refused: the
/// approval queue is owner-scoped, and an unmatched actor would let one owner
/// queue a mutation under another's identity.
#[test]
fn an_actor_that_does_not_match_the_envelope_owner_is_refused() {
    let (_cwd, engine) = operator_workspace("operator_git_owner_mismatch");
    let mut other = owner();
    other.project_id = "project_somewhere_else".to_string();
    let error = engine
        .prepare_project_mutation_for_supervisor(
            &other,
            &RuntimeCommand::RunOperatorGitAction {
                owner: owner(),
                target: SourceTarget::Workspace,
                action: OperatorGitAction::Fetch { remote: None },
            },
        )
        .unwrap_err();
    assert!(error.contains("does not match envelope owner"), "{error}");
}

/// The permission level a client renders is untouched by this path; the
/// capability adds a command, not a mode.
#[test]
fn an_operator_action_does_not_change_the_permission_level() {
    let (_cwd, mut engine) = operator_workspace("operator_git_level");
    let before = engine.permission_level();
    run_action(
        &mut engine,
        "stage-level",
        OperatorGitAction::Stage { paths: Vec::new() },
    );
    assert_eq!(engine.permission_level(), before);
    assert_eq!(before, PermissionLevel::Ask);
}
