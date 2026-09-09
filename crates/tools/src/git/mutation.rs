use viden_types::{ToolInput, ToolSpec};

use crate::git::common::{
    collect_git_paths, current_git_branch, path_relative_to_repo, resolve_git_base,
    run_git_capture, run_git_capture_owned,
};
use crate::{BuiltinTool, ToolExecutionContext, ToolExecutionOutput};

pub(crate) struct GitSwitchTool;
pub(crate) struct GitCommitTool;
pub(crate) struct GitAddTool;
pub(crate) struct GitPushTool;
pub(crate) struct GitRestoreTool;
pub(crate) struct GitFetchTool;

impl BuiltinTool for GitSwitchTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "git_switch".to_string(),
            description: "Switch to or create a git branch".to_string(),
            is_mutating: true,
            input_schema_hint: "branch=name create=false path=optional/repo/root".to_string(),
        }
    }

    fn run(
        &self,
        ctx: &ToolExecutionContext,
        input: &ToolInput,
    ) -> Result<ToolExecutionOutput, String> {
        let repo = resolve_git_base(ctx, input)?;
        let branch = input
            .get("branch")
            .ok_or_else(|| "git_switch requires `branch`".to_string())?;
        let create = input
            .get("create")
            .map(|value| value == "true")
            .unwrap_or(false);
        let args: Vec<&str> = if create {
            vec!["switch", "-c", branch.as_str()]
        } else {
            vec!["switch", branch.as_str()]
        };
        let output = run_git_capture(&repo, &args)?;
        Ok(ToolExecutionOutput {
            output,
            diff: None,
            success: true,
            exit_code: None,
        })
    }
}

impl BuiltinTool for GitCommitTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "git_commit".to_string(),
            description: "Create a git commit".to_string(),
            is_mutating: true,
            input_schema_hint: "message='commit message' all=false path=optional/repo/root"
                .to_string(),
        }
    }

    fn run(
        &self,
        ctx: &ToolExecutionContext,
        input: &ToolInput,
    ) -> Result<ToolExecutionOutput, String> {
        let repo = resolve_git_base(ctx, input)?;
        let message = input
            .get("message")
            .ok_or_else(|| "git_commit requires `message`".to_string())?;
        let all = input
            .get("all")
            .map(|value| value == "true")
            .unwrap_or(false);
        let output = if all {
            run_git_capture(&repo, &["commit", "-am", message.as_str()])?
        } else {
            run_git_capture(&repo, &["commit", "-m", message.as_str()])?
        };
        Ok(ToolExecutionOutput {
            output,
            diff: None,
            success: true,
            exit_code: None,
        })
    }
}

impl BuiltinTool for GitAddTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "git_add".to_string(),
            description: "Stage files in git".to_string(),
            is_mutating: true,
            input_schema_hint: "path=file paths='a\\nb' all=false path=optional/repo/root"
                .to_string(),
        }
    }

    fn run(
        &self,
        ctx: &ToolExecutionContext,
        input: &ToolInput,
    ) -> Result<ToolExecutionOutput, String> {
        let repo = resolve_git_base(ctx, input)?;
        let all = input
            .get("all")
            .map(|value| value == "true")
            .unwrap_or(false);
        let mut args = vec!["add".to_string()];
        if all {
            args.push("--all".to_string());
        }
        for path in collect_git_paths(input) {
            args.push(path_relative_to_repo(&repo, &ctx.cwd, &path)?);
        }
        if args.len() == 1 {
            return Err("git_add requires at least one path or `all=true`".to_string());
        }
        let output = run_git_capture_owned(&repo, &args)?;
        Ok(ToolExecutionOutput {
            output,
            diff: None,
            success: true,
            exit_code: None,
        })
    }
}

impl BuiltinTool for GitPushTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "git_push".to_string(),
            description: "Push the current branch to a git remote".to_string(),
            is_mutating: true,
            input_schema_hint:
                "remote=origin branch=current set_upstream=false path=optional/repo/root"
                    .to_string(),
        }
    }

    fn run(
        &self,
        ctx: &ToolExecutionContext,
        input: &ToolInput,
    ) -> Result<ToolExecutionOutput, String> {
        let repo = resolve_git_base(ctx, input)?;
        let remote = input
            .get("remote")
            .cloned()
            .unwrap_or_else(|| "origin".to_string());
        let branch = input
            .get("branch")
            .cloned()
            .unwrap_or(current_git_branch(&repo)?);
        let set_upstream = input
            .get("set_upstream")
            .map(|value| value == "true")
            .unwrap_or(false);
        let mut args = vec!["push".to_string()];
        if set_upstream {
            args.push("--set-upstream".to_string());
        }
        args.push(remote);
        args.push(branch);
        let output = run_git_capture_owned(&repo, &args)?;
        Ok(ToolExecutionOutput {
            output,
            diff: None,
            success: true,
            exit_code: None,
        })
    }
}

impl BuiltinTool for GitRestoreTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "git_restore".to_string(),
            description: "Restore files from git HEAD or another source".to_string(),
            is_mutating: true,
            input_schema_hint:
                "path=file paths='a\\nb' staged=false worktree=true source=HEAD path=optional/repo/root"
                    .to_string(),
        }
    }

    fn run(
        &self,
        ctx: &ToolExecutionContext,
        input: &ToolInput,
    ) -> Result<ToolExecutionOutput, String> {
        let repo = resolve_git_base(ctx, input)?;
        let staged = input
            .get("staged")
            .map(|value| value == "true")
            .unwrap_or(false);
        let worktree = input
            .get("worktree")
            .map(|value| value != "false")
            .unwrap_or(true);
        if !staged && !worktree {
            return Err("git_restore requires `staged=true` or `worktree=true`".to_string());
        }
        let paths = collect_git_paths(input);
        if paths.is_empty() {
            return Err("git_restore requires at least one path".to_string());
        }
        let mut args = vec!["restore".to_string()];
        if staged {
            args.push("--staged".to_string());
        }
        if worktree && staged {
            args.push("--worktree".to_string());
        }
        if let Some(source) = input.get("source") {
            args.push("--source".to_string());
            args.push(source.clone());
        }
        args.push("--".to_string());
        for path in paths {
            args.push(path_relative_to_repo(&repo, &ctx.cwd, &path)?);
        }
        let output = run_git_capture_owned(&repo, &args)?;
        Ok(ToolExecutionOutput {
            output,
            diff: None,
            success: true,
            exit_code: None,
        })
    }
}

/// Updates remote-tracking refs without touching `HEAD` or the working tree.
///
/// `is_mutating: true` even though nothing under the operator's editor changes:
/// it writes `refs/remotes/`, contacts a remote, and must therefore be blocked
/// in plan mode and gated like every other write. Spelling it non-mutating
/// would file it under the engine's safe-read branch, which is how a "harmless"
/// tool becomes the one that runs when everything else is denied.
///
/// It exists as a tool rather than as operator-only code so an agent asked to
/// sync gets exactly the same effect under exactly the same permission name.
impl BuiltinTool for GitFetchTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "git_fetch".to_string(),
            description: "Fetch remote-tracking refs from a git remote".to_string(),
            is_mutating: true,
            input_schema_hint: "remote=origin path=optional/repo/root".to_string(),
        }
    }

    fn run(
        &self,
        ctx: &ToolExecutionContext,
        input: &ToolInput,
    ) -> Result<ToolExecutionOutput, String> {
        let repo = resolve_git_base(ctx, input)?;
        let remote = input
            .get("remote")
            .cloned()
            .unwrap_or_else(|| "origin".to_string());
        // A remote name that begins with `-` would be read by git as an
        // option, so it is refused rather than passed through: `--` does not
        // help here because the remote is the subcommand's own operand.
        if remote.starts_with('-') {
            return Err(format!("git_fetch remote `{remote}` cannot start with `-`"));
        }
        let output = run_git_capture_owned(&repo, &["fetch".to_string(), remote])?;
        Ok(ToolExecutionOutput {
            output,
            diff: None,
            success: true,
            exit_code: None,
        })
    }
}
