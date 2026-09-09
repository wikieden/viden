use super::*;

#[test]
fn git_status_and_diff_work_in_repo() {
    let cwd = temp_dir("git_repo");
    git_init(&cwd);
    git_config_identity(&cwd);
    fs::write(cwd.join("demo.txt"), "hello\n").unwrap();
    git_add(&cwd, "demo.txt");
    git_commit(&cwd, "initial");
    fs::write(cwd.join("demo.txt"), "hello again\n").unwrap();

    let ctx = ToolExecutionContext::local(cwd.clone());
    let registry = ToolRegistry::builtin();

    let status_result = registry
        .execute(
            &ToolCall {
                id: "tool_git_status".into(),
                name: "git_status".into(),
                input: ToolInput::new(),
            },
            &ctx,
        )
        .unwrap();
    assert!(status_result.output.contains("demo.txt"));

    let diff_result = registry
        .execute(
            &ToolCall {
                id: "tool_git_diff".into(),
                name: "git_diff".into(),
                input: ToolInput::new(),
            },
            &ctx,
        )
        .unwrap();
    assert!(diff_result.output.contains("hello again"));
}

#[test]
fn git_branch_switch_and_commit_work() {
    let cwd = git_repo_with_tracked_file("git_branch");
    let ctx = ToolExecutionContext::local(cwd.clone());
    let registry = ToolRegistry::builtin();

    let mut switch_input = ToolInput::new();
    switch_input.insert("branch".into(), "feature/demo".into());
    switch_input.insert("create".into(), "true".into());
    registry
        .execute(
            &ToolCall {
                id: "tool_git_switch".into(),
                name: "git_switch".into(),
                input: switch_input,
            },
            &ctx,
        )
        .unwrap();

    fs::write(cwd.join("tracked.txt"), "second\n").unwrap();
    let mut commit_input = ToolInput::new();
    commit_input.insert("message".into(), "update tracked file".into());
    commit_input.insert("all".into(), "true".into());
    let result = registry
        .execute(
            &ToolCall {
                id: "tool_git_commit".into(),
                name: "git_commit".into(),
                input: commit_input,
            },
            &ctx,
        )
        .unwrap();
    assert!(
        result.output.contains("update tracked file") || result.output.contains("files changed")
    );
}

#[test]
fn git_add_stages_requested_paths() {
    let cwd = temp_dir("git_add");
    git_init(&cwd);
    fs::write(cwd.join("notes.txt"), "hello\n").unwrap();

    let ctx = ToolExecutionContext::local(cwd.clone());
    let registry = ToolRegistry::builtin();

    let mut add_input = ToolInput::new();
    add_input.insert("path".into(), "notes.txt".into());
    registry
        .execute(
            &ToolCall {
                id: "tool_git_add".into(),
                name: "git_add".into(),
                input: add_input,
            },
            &ctx,
        )
        .unwrap();

    let status = Command::new("git")
        .args(["status", "--short"])
        .current_dir(&cwd)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(stdout.contains("A  notes.txt"));
}

#[test]
fn git_push_pushes_current_branch_to_remote() {
    let remote = temp_dir("git_remote");
    let bare = Command::new("git")
        .arg("init")
        .arg("--bare")
        .current_dir(&remote)
        .status()
        .unwrap();
    assert!(bare.success());

    let cwd = git_repo_with_tracked_file("git_push");
    let origin = Command::new("git")
        .args(["remote", "add", "origin", remote.to_string_lossy().as_ref()])
        .current_dir(&cwd)
        .status()
        .unwrap();
    assert!(origin.success());

    let ctx = ToolExecutionContext::local(cwd.clone());
    let registry = ToolRegistry::builtin();
    let mut push_input = ToolInput::new();
    push_input.insert("set_upstream".into(), "true".into());
    let result = registry
        .execute(
            &ToolCall {
                id: "tool_git_push".into(),
                name: "git_push".into(),
                input: push_input,
            },
            &ctx,
        )
        .unwrap();
    assert!(
        result.output.contains("main")
            || result.output.contains("branch")
            || result.output.contains("up to date")
    );

    let remote_refs = Command::new("git")
        .args(["show-ref"])
        .current_dir(&remote)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&remote_refs.stdout);
    assert!(stdout.contains("refs/heads/main"));
}

#[test]
fn git_restore_reverts_worktree_file() {
    let cwd = git_repo_with_tracked_file("git_restore");
    fs::write(cwd.join("tracked.txt"), "second\n").unwrap();

    let ctx = ToolExecutionContext::local(cwd.clone());
    let registry = ToolRegistry::builtin();
    let mut restore_input = ToolInput::new();
    restore_input.insert("path".into(), "tracked.txt".into());
    registry
        .execute(
            &ToolCall {
                id: "tool_git_restore".into(),
                name: "git_restore".into(),
                input: restore_input,
            },
            &ctx,
        )
        .unwrap();

    let contents = fs::read_to_string(cwd.join("tracked.txt")).unwrap();
    assert_eq!(contents, "first\n");
}

#[test]
fn git_stash_push_list_and_pop_work() {
    let cwd = git_repo_with_tracked_file("git_stash");
    fs::write(cwd.join("tracked.txt"), "second\n").unwrap();

    let ctx = ToolExecutionContext::local(cwd.clone());
    let registry = ToolRegistry::builtin();

    let mut push_input = ToolInput::new();
    push_input.insert("message".into(), "save work".into());
    let push_result = registry
        .execute(
            &ToolCall {
                id: "tool_git_stash_push".into(),
                name: "git_stash_push".into(),
                input: push_input,
            },
            &ctx,
        )
        .unwrap();
    assert!(push_result.output.contains("save work") || push_result.output.contains("stash"));

    let list_result = registry
        .execute(
            &ToolCall {
                id: "tool_git_stash_list".into(),
                name: "git_stash_list".into(),
                input: ToolInput::new(),
            },
            &ctx,
        )
        .unwrap();
    assert!(list_result.output.contains("save work"));

    let pop_result = registry
        .execute(
            &ToolCall {
                id: "tool_git_stash_pop".into(),
                name: "git_stash_pop".into(),
                input: ToolInput::new(),
            },
            &ctx,
        )
        .unwrap();
    assert!(pop_result.output.contains("tracked.txt") || pop_result.output.contains("Dropped"));

    let contents = fs::read_to_string(cwd.join("tracked.txt")).unwrap();
    assert_eq!(contents, "second\n");
}

#[test]
fn git_worktree_add_list_and_remove_work() {
    let cwd = git_repo_with_tracked_file("git_worktree_repo");
    let worktree_path = cwd.parent().unwrap().join("viden_tools_worktree_checkout");
    if worktree_path.exists() {
        fs::remove_dir_all(&worktree_path).unwrap();
    }

    let ctx = ToolExecutionContext::local(cwd.clone());
    let registry = ToolRegistry::builtin();

    let mut add_input = ToolInput::new();
    add_input.insert("path".into(), worktree_path.to_string_lossy().to_string());
    add_input.insert("branch".into(), "feature/worktree".into());
    add_input.insert("create".into(), "true".into());
    registry
        .execute(
            &ToolCall {
                id: "tool_git_worktree_add".into(),
                name: "git_worktree_add".into(),
                input: add_input,
            },
            &ctx,
        )
        .unwrap();
    assert!(worktree_path.exists());

    let list_result = registry
        .execute(
            &ToolCall {
                id: "tool_git_worktree_list".into(),
                name: "git_worktree_list".into(),
                input: ToolInput::new(),
            },
            &ctx,
        )
        .unwrap();
    assert!(
        list_result
            .output
            .contains(worktree_path.to_string_lossy().as_ref())
    );

    let mut remove_input = ToolInput::new();
    remove_input.insert("path".into(), worktree_path.to_string_lossy().to_string());
    registry
        .execute(
            &ToolCall {
                id: "tool_git_worktree_remove".into(),
                name: "git_worktree_remove".into(),
                input: remove_input,
            },
            &ctx,
        )
        .unwrap();
    assert!(!worktree_path.exists());
}

#[test]
fn git_worktree_tools_reject_path_traversal_through_lane_effects() {
    let cwd = git_repo_with_tracked_file("git_worktree_path_traversal_repo");
    let ctx = ToolExecutionContext::local(cwd.clone());
    let registry = ToolRegistry::builtin();

    let mut add_input = ToolInput::new();
    add_input.insert("path".into(), "../escape".into());
    add_input.insert("branch".into(), "feature/escape".into());
    add_input.insert("create".into(), "true".into());
    let err = registry
        .execute(
            &ToolCall {
                id: "tool_git_worktree_add_escape".into(),
                name: "git_worktree_add".into(),
                input: add_input,
            },
            &ctx,
        )
        .unwrap_err();
    assert!(err.contains("unsafe worktree path"));

    let mut remove_input = ToolInput::new();
    remove_input.insert("path".into(), "../escape".into());
    let err = registry
        .execute(
            &ToolCall {
                id: "tool_git_worktree_remove_escape".into(),
                name: "git_worktree_remove".into(),
                input: remove_input,
            },
            &ctx,
        )
        .unwrap_err();
    assert!(err.contains("unsafe worktree path"));
}

/// `git_fetch` exists so an operator's sync control and an agent's fetch are
/// the same tool under the same permission name (`runtime.operator_git`,
/// GUI-CORE-020).
///
/// The remote here is a local bare repository. A test that reached a network
/// remote would be a test of the network, and would fail closed on a machine
/// with no credentials for reasons that have nothing to do with this tool.
#[test]
fn git_fetch_updates_remote_refs_from_a_local_bare_remote() {
    let remote = temp_dir("git_fetch_remote");
    let init = Command::new("git")
        .args(["init", "--bare", "-b", "main"])
        .current_dir(&remote)
        .status()
        .unwrap();
    assert!(init.success());

    let cwd = git_repo_with_tracked_file("git_fetch_clone");
    let add_remote = Command::new("git")
        .args(["remote", "add", "origin"])
        .arg(&remote)
        .current_dir(&cwd)
        .status()
        .unwrap();
    assert!(add_remote.success());
    let push = Command::new("git")
        .args(["push", "--set-upstream", "origin", "main"])
        .current_dir(&cwd)
        .status()
        .unwrap();
    assert!(push.success());

    let ctx = ToolExecutionContext::local(cwd.clone());
    let registry = ToolRegistry::builtin();
    let spec = registry.spec("git_fetch").expect("git_fetch is registered");
    // It writes refs under `refs/remotes/`, so it is a mutation and the
    // permission gate must treat it as one. A non-mutating spelling would let
    // it run in plan mode.
    assert!(spec.is_mutating);

    let mut input = ToolInput::new();
    input.insert("remote".into(), "origin".into());
    let result = registry
        .execute(
            &ToolCall {
                id: "tool_git_fetch".into(),
                name: "git_fetch".into(),
                input,
            },
            &ctx,
        )
        .unwrap();
    assert!(result.success);

    let refs = Command::new("git")
        .args(["rev-parse", "refs/remotes/origin/main"])
        .current_dir(&cwd)
        .output()
        .unwrap();
    assert!(refs.status.success(), "the fetch must land a remote ref");
}

/// A fetch from a remote that is not configured fails loudly rather than
/// reporting a clean no-op, which an operator would read as "already in sync".
#[test]
fn git_fetch_reports_an_unknown_remote_as_a_failure() {
    let cwd = git_repo_with_tracked_file("git_fetch_unknown");
    let ctx = ToolExecutionContext::local(cwd);
    let registry = ToolRegistry::builtin();
    let mut input = ToolInput::new();
    input.insert("remote".into(), "nowhere".into());
    let error = registry
        .execute(
            &ToolCall {
                id: "tool_git_fetch_unknown".into(),
                name: "git_fetch".into(),
                input,
            },
            &ctx,
        )
        .unwrap_err();
    assert!(!error.is_empty());
}
