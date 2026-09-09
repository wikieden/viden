//! Runtime behavior of `runtime.structured_diff` (GUI-CORE-012).
//!
//! Three attachment sites and one read have to hold together:
//!
//! 1. an `edit_file` / `write_file` approval carries the change it *would*
//!    make, computed read-only — the file on disk must be byte-identical
//!    after the approval is built, because a preview that mutated would make
//!    a denial meaningless;
//! 2. a completed workspace change carries the same rows beside its `patch`
//!    text, so a base client and a structured client describe one change;
//! 3. `QueryWorkspaceDiff` passes the permission gate *before* it runs a
//!    single `git` process, and a refusal is a `CommandRejected` naming the
//!    read rather than an empty page a client would render as "nothing
//!    changed".

use std::fs;
use std::path::Path;

use sha2::Digest;

use viden_types::{
    ApprovalResponse, DiffLineKind, ModelEvent, PermissionBehavior, PermissionRule,
    PermissionRuleSource, PermissionRuleValue, RuntimeCommand, RuntimeEvent, RuntimeEventKind,
    SourceTarget, ToolCall, ToolInput, WorkMode, WorkspaceChangeKind, WorkspaceDiffPage,
    WorkspaceDiffQuery, WorkspaceDiffScope,
};

use super::{SequenceProvider, temp_dir};
use crate::SessionEngine;

fn init_git_repo(cwd: &Path) {
    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.email", "viden@example.com"],
        vec!["config", "user.name", "Viden"],
    ] {
        let status = std::process::Command::new("git")
            .args(&args)
            .current_dir(cwd)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }
}

fn git(cwd: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn git_diff_rule(rule_behavior: PermissionBehavior) -> PermissionRule {
    PermissionRule {
        source: PermissionRuleSource::Session,
        rule_behavior,
        rule_value: PermissionRuleValue {
            tool_name: "git_diff".to_string(),
            rule_content: None,
        },
    }
}

fn approval_from(events: &[RuntimeEvent]) -> viden_types::ApprovalRequestView {
    events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::ApprovalRequested { approval } => Some(approval.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected an ApprovalRequested, got {events:?}"))
}

fn tool_turn(cwd: &Path, home: &Path, call: ToolCall) -> (SessionEngine, Vec<RuntimeEvent>) {
    let provider = Box::new(SequenceProvider::new(vec![
        vec![ModelEvent::ToolCall(call)],
        vec![ModelEvent::AssistantText {
            content: "done".to_string(),
        }],
    ]));
    let mut engine = SessionEngine::new_with_home(cwd, provider, Some(home.to_path_buf())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::deny(Some("previewed only".to_string()));
    let events = engine
        .handle_runtime_command(
            "turn-1",
            RuntimeCommand::SubmitUserInput {
                content: "change the file".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    (engine, events)
}

/// The approval for an `edit_file` carries the change it would make, and the
/// file on disk is untouched. A preview that wrote would turn a denial into a
/// mutation that already happened.
#[test]
fn an_edit_file_approval_previews_the_change_without_touching_the_file() {
    let home = temp_dir("structured_diff_edit_home");
    let cwd = temp_dir("structured_diff_edit_cwd");
    let target = cwd.join("src.txt");
    let before = "alpha\nbeta\ngamma\n";
    fs::write(&target, before).unwrap();
    let before_bytes = fs::read(&target).unwrap();

    let mut input = ToolInput::new();
    input.insert("path".to_string(), "src.txt".to_string());
    input.insert("old".to_string(), "beta".to_string());
    input.insert("new".to_string(), "BETA".to_string());
    let (_engine, events) = tool_turn(
        &cwd,
        &home,
        ToolCall {
            id: "tool_edit".to_string(),
            name: "edit_file".to_string(),
            input,
        },
    );

    assert_eq!(
        fs::read(&target).unwrap(),
        before_bytes,
        "building an approval must not write a single byte"
    );

    let approval = approval_from(&events);
    let context = approval
        .decision_context
        .expect("an edit_file approval must carry its decision context");
    let base = context
        .base_sha256
        .expect("a diff against an existing file must name the bytes it read");
    assert_eq!(base.len(), 64);
    assert_eq!(
        base,
        format!("{:x}", sha2::Sha256::digest(before.as_bytes())),
        "base_sha256 must hash exactly the bytes Core read"
    );

    let document = context.diff.expect("the context must carry a diff");
    assert!(!document.truncated);
    assert_eq!(document.files.len(), 1);
    let file = &document.files[0];
    assert_eq!(
        file.path, "src.txt",
        "the published path is the workspace-relative one Core resolved, never an absolute path"
    );
    assert_eq!(file.kind, WorkspaceChangeKind::Modified);
    assert_eq!((file.additions, file.deletions), (1, 1));
    let rows = file
        .hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .map(|line| (line.kind, line.content.as_str()))
        .collect::<Vec<_>>();
    assert!(rows.contains(&(DiffLineKind::Removed, "beta")));
    assert!(rows.contains(&(DiffLineKind::Added, "BETA")));
}

/// A `write_file` to a path that does not exist yet is an addition against an
/// empty base, and Core read no bytes, so it names no base hash. Publishing
/// the hash of nothing would let a client believe it had a real preimage.
#[test]
fn a_write_file_approval_for_a_missing_file_is_an_addition_with_no_base_hash() {
    let home = temp_dir("structured_diff_write_home");
    let cwd = temp_dir("structured_diff_write_cwd");

    let mut input = ToolInput::new();
    input.insert("path".to_string(), "created.txt".to_string());
    input.insert("content".to_string(), "first\nsecond\n".to_string());
    let (_engine, events) = tool_turn(
        &cwd,
        &home,
        ToolCall {
            id: "tool_write".to_string(),
            name: "write_file".to_string(),
            input,
        },
    );

    assert!(
        !cwd.join("created.txt").exists(),
        "an approval preview must not create the file it previews"
    );
    let context = approval_from(&events)
        .decision_context
        .expect("a write_file approval must carry its decision context");
    assert_eq!(
        context.base_sha256, None,
        "there is no preimage to hash for a file Core could not read"
    );
    let file = &context.diff.expect("a diff").files[0];
    assert_eq!(file.path, "created.txt");
    assert_eq!(file.kind, WorkspaceChangeKind::Added);
    assert_eq!((file.additions, file.deletions), (2, 0));
}

/// A tool Core cannot preview attaches no context at all. `None` means "Core
/// did not compute one", which a client renders as the plain input preview —
/// never as an empty diff, which reads as "this changes nothing".
#[test]
fn a_shell_approval_carries_no_decision_context() {
    let home = temp_dir("structured_diff_shell_home");
    let cwd = temp_dir("structured_diff_shell_cwd");
    let mut input = ToolInput::new();
    input.insert("command".to_string(), "echo hi".to_string());
    let (_engine, events) = tool_turn(
        &cwd,
        &home,
        ToolCall {
            id: "tool_shell".to_string(),
            name: "shell".to_string(),
            input,
        },
    );
    assert_eq!(approval_from(&events).decision_context, None);
}

/// The completed change carries the same rows its `patch` text carries, so a
/// base client reading `patch` and a structured client reading `diff` describe
/// one change rather than two.
#[test]
fn a_completed_workspace_change_carries_the_same_rows_as_its_patch() {
    let home = temp_dir("structured_diff_change_home");
    let cwd = temp_dir("structured_diff_change_cwd");
    fs::write(cwd.join("src.txt"), "alpha\nbeta\n").unwrap();

    let mut input = ToolInput::new();
    input.insert("path".to_string(), "src.txt".to_string());
    input.insert("old".to_string(), "beta".to_string());
    input.insert("new".to_string(), "BETA".to_string());
    let provider = Box::new(SequenceProvider::new(vec![
        vec![ModelEvent::ToolCall(ToolCall {
            id: "tool_edit".to_string(),
            name: "edit_file".to_string(),
            input,
        })],
        vec![ModelEvent::AssistantText {
            content: "done".to_string(),
        }],
    ]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let events = engine
        .handle_runtime_command(
            "turn-1",
            RuntimeCommand::SubmitUserInput {
                content: "edit it".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    let change = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::WorkspaceChangeUpdated { change } => Some(change.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a WorkspaceChangeUpdated, got {events:?}"));
    assert!(change.patch.is_some(), "`patch` stays for base clients");
    let document = change
        .diff
        .expect("a workspace change must carry its structured rows");
    assert_eq!(document.files.len(), 1);
    let file = &document.files[0];
    assert_eq!(file.path, change.path);
    assert_eq!(file.kind, change.kind);
    assert_eq!(
        (file.additions, file.deletions),
        (change.additions, change.deletions),
        "the structured counts must equal the counts published beside them"
    );
}

fn query_diff(
    engine: &mut SessionEngine,
    command_id: &str,
    query: WorkspaceDiffQuery,
) -> Vec<RuntimeEvent> {
    let mut denier = |_prompt| panic!("a read-only diff query must not request approval");
    engine
        .handle_runtime_command(
            command_id,
            RuntimeCommand::QueryWorkspaceDiff { query },
            &mut denier,
        )
        .unwrap()
}

fn diff_page(events: &[RuntimeEvent], expected_command_id: &str) -> WorkspaceDiffPage {
    let loaded = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::WorkspaceDiffLoaded { command_id, page } => {
                Some((command_id.clone(), page.clone()))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a WorkspaceDiffLoaded, got {events:?}"));
    assert_eq!(
        loaded.0, expected_command_id,
        "a page must name the exact read it answers"
    );
    loaded.1
}

fn rejection(events: &[RuntimeEvent], expected_command_id: &str) -> String {
    let (command_id, reason) = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, reason } => {
                Some((command_id.clone(), reason.clone()))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a CommandRejected, got {events:?}"));
    assert_eq!(command_id, expected_command_id);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::WorkspaceDiffLoaded { .. })),
        "a refusal must never be accompanied by a page"
    );
    reason
}

fn diff_workspace(name: &str) -> (std::path::PathBuf, SessionEngine) {
    let cwd = temp_dir(&format!("{name}_cwd"));
    let home = temp_dir(&format!("{name}_home"));
    init_git_repo(&cwd);
    fs::write(cwd.join("tracked.txt"), "one\ntwo\nthree\n").unwrap();
    fs::write(cwd.join("staged.txt"), "kept\n").unwrap();
    git(&cwd, &["add", "tracked.txt", "staged.txt"]);
    git(&cwd, &["commit", "-m", "initial"]);
    // One unstaged edit, one staged edit, one untracked file, and one file
    // that exists only inside an excluded state directory.
    fs::write(cwd.join("tracked.txt"), "one\nTWO\nthree\n").unwrap();
    fs::write(cwd.join("staged.txt"), "kept\nadded\n").unwrap();
    git(&cwd, &["add", "staged.txt"]);
    fs::write(cwd.join("untracked.txt"), "brand new\n").unwrap();
    fs::create_dir_all(cwd.join(".viden")).unwrap();
    fs::write(cwd.join(".viden/state.json"), "{}").unwrap();
    let engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    (cwd, engine)
}

#[test]
fn a_workspace_diff_read_publishes_worktree_rows_staged_flags_and_untracked_content() {
    let (_cwd, mut engine) = diff_workspace("structured_diff_read");
    let events = query_diff(
        &mut engine,
        "diff-read-1",
        WorkspaceDiffQuery {
            scope: WorkspaceDiffScope::Both,
            ..WorkspaceDiffQuery::default()
        },
    );
    assert!(matches!(
        &events[0].kind,
        RuntimeEventKind::CommandAccepted { command_id, .. } if command_id == "diff-read-1"
    ));
    let page = diff_page(&events, "diff-read-1");
    assert_eq!(page.target, SourceTarget::Workspace);
    assert_eq!(page.source.branch.as_deref(), Some("main"));

    let paths = page
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted, "entries must be lexicographic");
    assert_eq!(
        paths,
        vec![
            "staged.txt".to_string(),
            "tracked.txt".to_string(),
            "untracked.txt".to_string()
        ],
        "state directories never appear, whatever .gitignore says"
    );

    let staged = &page.entries[0];
    assert!(staged.staged, "a path with index content must say so");
    assert_eq!(staged.index, Some(WorkspaceChangeKind::Modified));
    let staged_rows = staged
        .diff
        .as_ref()
        .expect("a staged path must carry its cached diff")
        .additions;
    assert_eq!(staged_rows, 1);

    let worktree = &page.entries[1];
    assert!(!worktree.staged);
    assert_eq!(worktree.worktree, Some(WorkspaceChangeKind::Modified));
    let worktree_file = worktree.diff.as_ref().expect("an unstaged diff");
    assert_eq!((worktree_file.additions, worktree_file.deletions), (1, 1));

    // An untracked file has no `git diff` output at all, so Core publishes its
    // whole content as an addition. Leaving it out would tell a reviewer the
    // file does not exist.
    let untracked = &page.entries[2];
    assert_eq!(untracked.worktree, Some(WorkspaceChangeKind::Untracked));
    let untracked_file = untracked.diff.as_ref().expect("an untracked file's rows");
    assert_eq!(untracked_file.kind, WorkspaceChangeKind::Added);
    assert_eq!(untracked_file.additions, 1);
    assert_eq!(untracked_file.hunks[0].lines[0].content, "brand new");
}

/// The path filter selects entries; it never invents them.
#[test]
fn a_workspace_diff_read_honors_its_path_filter_and_rejects_an_escaping_path() {
    let (_cwd, mut engine) = diff_workspace("structured_diff_paths");
    let page = diff_page(
        &query_diff(
            &mut engine,
            "diff-read-paths",
            WorkspaceDiffQuery {
                scope: WorkspaceDiffScope::Both,
                paths: vec!["tracked.txt".to_string()],
                ..WorkspaceDiffQuery::default()
            },
        ),
        "diff-read-paths",
    );
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].path, "tracked.txt");

    let reason = rejection(
        &query_diff(
            &mut engine,
            "diff-read-escape",
            WorkspaceDiffQuery {
                paths: vec!["../secrets".to_string()],
                ..WorkspaceDiffQuery::default()
            },
        ),
        "diff-read-escape",
    );
    assert!(reason.contains("leaves the target"), "got {reason}");
}

/// A denial is a `CommandRejected` naming this read and carrying the hint. An
/// empty page would render as "nothing changed" and a bare `Error` carries no
/// command id at all, so a client with a read outstanding could mistake an
/// unrelated failure for its own refusal.
#[test]
fn a_denied_workspace_diff_read_is_rejected_with_its_command_id_and_a_hint() {
    let (_cwd, mut engine) = diff_workspace("structured_diff_denied");
    engine.add_permission_rule_for_test(git_diff_rule(PermissionBehavior::Deny));
    let reason = rejection(
        &query_diff(
            &mut engine,
            "diff-read-denied",
            WorkspaceDiffQuery::default(),
        ),
        "diff-read-denied",
    );
    assert!(reason.contains("git_diff"), "got {reason}");
    assert!(
        reason.contains("hint:"),
        "the actionable hint must survive into the reason, got {reason}"
    );
}

/// An unresolved ask is refused the same way: this read answers a keystroke,
/// not an interactive turn, so it is decided non-interactively rather than
/// blocking a client behind an approval prompt.
#[test]
fn an_asking_workspace_diff_read_is_refused_rather_than_prompting() {
    let (_cwd, mut engine) = diff_workspace("structured_diff_ask");
    engine.add_permission_rule_for_test(git_diff_rule(PermissionBehavior::Ask));
    let reason = rejection(
        &query_diff(&mut engine, "diff-read-ask", WorkspaceDiffQuery::default()),
        "diff-read-ask",
    );
    assert!(reason.contains("git_diff"), "got {reason}");
}

/// Plan mode blocks mutation, not reading. `git_diff` mutates nothing, so the
/// read still answers through the engine's safe-read branch.
#[test]
fn a_workspace_diff_read_still_answers_in_plan_mode() {
    let (_cwd, mut engine) = diff_workspace("structured_diff_plan");
    engine.set_work_mode(WorkMode::Plan).unwrap();
    let page = diff_page(
        &query_diff(&mut engine, "diff-read-plan", WorkspaceDiffQuery::default()),
        "diff-read-plan",
    );
    assert!(!page.entries.is_empty());
}

/// A Lane target is resolved by Core from its own records. An id no Lane
/// carries is a rejection, never a silent fall back to the workspace root —
/// which would answer a question about one tree with facts from another.
#[test]
fn a_workspace_diff_read_rejects_an_unknown_lane_target() {
    let (_cwd, mut engine) = diff_workspace("structured_diff_lane");
    let reason = rejection(
        &query_diff(
            &mut engine,
            "diff-read-lane",
            WorkspaceDiffQuery {
                target: SourceTarget::Lane {
                    lane_id: "lane_missing".to_string(),
                },
                ..WorkspaceDiffQuery::default()
            },
        ),
        "diff-read-lane",
    );
    assert!(reason.contains("lane_missing"), "got {reason}");
}

/// The byte bound degrades rows, never entries: a bounded read still lists
/// every changed path and says the page is partial.
#[test]
fn a_bounded_workspace_diff_read_omits_rows_and_says_so() {
    let (_cwd, mut engine) = diff_workspace("structured_diff_bound");
    let page = diff_page(
        &query_diff(
            &mut engine,
            "diff-read-bound",
            WorkspaceDiffQuery {
                scope: WorkspaceDiffScope::Both,
                byte_limit: Some(1),
                ..WorkspaceDiffQuery::default()
            },
        ),
        "diff-read-bound",
    );
    assert!(page.truncated, "the page must admit it is partial");
    assert_eq!(page.entries.len(), 3, "every changed path stays listed");
    assert!(
        page.entries
            .iter()
            .filter_map(|entry| entry.diff.as_ref())
            .any(|file| file.omitted && file.additions > 0),
        "an omitted file keeps its real counts, got {:?}",
        page.entries
    );
}

/// A diff page is a query answer, not runtime truth: reducing one must leave
/// `RuntimeViewState` exactly as it was, so no snapshot digest moves.
#[test]
fn a_workspace_diff_page_never_folds_into_the_runtime_view() {
    let (_cwd, mut engine) = diff_workspace("structured_diff_view");
    let events = query_diff(&mut engine, "diff-read-view", WorkspaceDiffQuery::default());
    let mut view = viden_types::RuntimeViewState::new(engine.runtime_snapshot());
    let baseline = serde_json::to_string(&view).unwrap();
    for event in &events {
        if matches!(event.kind, RuntimeEventKind::WorkspaceDiffLoaded { .. }) {
            view.apply_event(event);
        }
    }
    assert_eq!(serde_json::to_string(&view).unwrap(), baseline);
}

/// The trust loop's patch merge is the multi-file case GUI-CORE-012 asked
/// for: the operator approving it sees every file the canonical patch
/// touches, not a gate id. There is no single preimage across several files,
/// so the context names no `base_sha256` — one hash cannot describe them all,
/// and publishing one would invite a client to check the wrong file.
#[test]
fn a_merge_agent_patch_approval_carries_the_multi_file_change() {
    use std::sync::{Arc, Mutex};

    use viden_types::{HandoffAcceptance, PermissionPrompt};

    use super::audit_runtime_tests::{owner, record_canonical_patch, start_gate};

    let cwd = temp_dir("structured_diff_merge_cwd");
    let home = temp_dir("structured_diff_merge_home");
    fs::create_dir_all(cwd.join("src")).unwrap();
    fs::write(cwd.join("src/lib.rs"), "old\n").unwrap();
    fs::write(cwd.join("src/other.rs"), "kept\n").unwrap();
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();

    let prompts: Arc<Mutex<Vec<PermissionPrompt>>> = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&prompts);
    let mut approver = move |prompt: PermissionPrompt| {
        recorded.lock().unwrap().push(prompt);
        ApprovalResponse::allow_once(None)
    };

    let task_id = "task-structured-merge";
    start_gate(
        &mut engine,
        &mut approver,
        task_id,
        vec!["patch".to_string()],
    );
    engine
        .handle_runtime_command(
            "handoff-merge",
            RuntimeCommand::CreateHandoff {
                handoff_id: "handoff-merge".to_string(),
                task_id: task_id.to_string(),
                from_lane_id: "lane-planner".to_string(),
                to_lane_id: "lane-origin".to_string(),
                owner: owner("lane-origin", task_id),
                summary: "origin owns the patch".to_string(),
                acceptance: HandoffAcceptance::Accepted,
            },
            &mut approver,
        )
        .unwrap();
    let patch = b"diff --git a/src/lib.rs b/src/lib.rs\n\
                  --- a/src/lib.rs\n\
                  +++ b/src/lib.rs\n\
                  @@ -1 +1 @@\n\
                  -old\n\
                  +merged\n\
                  diff --git a/src/other.rs b/src/other.rs\n\
                  --- a/src/other.rs\n\
                  +++ b/src/other.rs\n\
                  @@ -1 +1,2 @@\n\
                  \x20kept\n\
                  +appended\n";
    let binding = record_canonical_patch(&cwd, &mut engine, &mut approver, task_id, patch);
    engine
        .handle_runtime_command(
            "review-merge",
            RuntimeCommand::RequestReview {
                review_id: "review-merge".to_string(),
                gate_id: format!("gate-{task_id}"),
                requester_lane_id: "lane-origin".to_string(),
                reviewer_lane_id: "lane-reviewer".to_string(),
                owner: owner("lane-origin", task_id),
                evidence_ids: vec![binding.evidence_id.clone()],
            },
            &mut approver,
        )
        .unwrap();
    engine
        .handle_runtime_command(
            "accept-merge",
            RuntimeCommand::AcceptMergeGate {
                gate_id: format!("gate-{task_id}"),
                actor: owner("lane-reviewer", task_id),
                reviewed_evidence: vec![binding],
                decision: Some("reviewer accepted the patch".to_string()),
            },
            &mut approver,
        )
        .unwrap();
    prompts.lock().unwrap().clear();
    engine
        .handle_runtime_command(
            "merge-patch",
            RuntimeCommand::MergeAgentPatch {
                gate_id: format!("gate-{task_id}"),
                actor: owner("lane-origin", task_id),
                decision: Some("apply the reviewed patch".to_string()),
            },
            &mut approver,
        )
        .unwrap();

    let prompt = prompts
        .lock()
        .unwrap()
        .iter()
        .find(|prompt| prompt.tool_name == "workflow_merge_agent_patch")
        .cloned()
        .expect("the patch merge must reach the operator gate");
    let context = prompt
        .decision_context
        .expect("a patch merge approval must carry the change it applies");
    assert_eq!(
        context.base_sha256, None,
        "a multi-file patch has no single preimage to hash"
    );
    let document = context.diff.expect("the patch rows");
    let files = document
        .files
        .iter()
        .map(|file| (file.path.clone(), file.additions, file.deletions))
        .collect::<Vec<_>>();
    assert_eq!(
        files,
        vec![
            ("src/lib.rs".to_string(), 1, 1),
            ("src/other.rs".to_string(), 1, 0),
        ],
        "every file the canonical patch touches must be visible before the apply"
    );
}
