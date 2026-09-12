# viden-tools

## Purpose

`viden-tools` owns built-in local tools and execution adapters.

## Does Not Own

- Permission decisions.
- Model planning.
- Transcript or workflow state.
- Merge-gate orchestration or workflow persistence.

## Public Surface

- `BuiltinTool`
- `ToolRegistry`
- Built-ins for shell, files, glob, grep, web, and Git.
- Lane effect adapters for Git worktrees, local process groups, typed tmux/PTY
  terminal backends, and checked patch application.

## Invariants

- Mutating tools must be marked mutating in `ToolSpec`.
- Outputs must become serializable `ToolResult` values.
- Shell stays platform-aware: POSIX on Unix, PowerShell on Windows.
- Local lane processes never leave stdout or stderr in unread pipes: callers
  choose a durable combined log, otherwise output is explicitly discarded.
- `TerminalBackend` keeps typed tmux and PTY launch/input/stop semantics apart
  from plain `ProcessBackend` child-process effects.
- Patch adapters prepare every create, write, and delete before touching the
  filesystem. Standard `/dev/null` new-file and deleted-file diffs therefore
  participate in the same runtime rollback transaction.
- A file the strict apply cannot write is a stated outcome, never a silent
  absence. A binary section — what Git reports as `Binary files … differ` or
  `GIT binary patch` — is named in `PatchApplyOutcome.conflicts` by both
  `check` and `apply` while the text files in the same patch still apply, and
  `conflict_content` publishes it as one row-less hunk with
  `ConflictHunkReason::Binary` and no preimage.
- Git worktree tools delegate to the same lane worktree adapter used by Core
  lane orchestration.

## Reference Alignment

Reflects `.ref` `Tool.ts` and tool registry behavior using Rust traits and local adapters.

## Test

```bash
cargo test -p viden-tools
```
