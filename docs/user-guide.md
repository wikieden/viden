# Viden User Guide

Chinese version: [user-guide.zh-CN.md](user-guide.zh-CN.md)

This guide describes the user-facing features that are available in the current
Viden development line.

## Mental Model

Viden has three layers:

- CLI runtime: loads config, selects a provider, records transcripts, runs tools,
  and applies permissions.
- Cockpit TUI: renders the conversation, current operation, approval prompts,
  workspace state, tasks, provider health, and side-screen controls.
- Operator surfaces: slash commands, agent lanes, side screens, tasks, memory,
  diagnostics, and release evidence.

All mutating actions should enter through the shared runtime path so they can be
recorded and permission-checked.

## Install And Start

Install with Homebrew:

```bash
brew install wikieden/tap/viden
viden --help
```

Run from a release archive:

```bash
viden --version
viden --provider fallback --model test-local
```

`viden` starts the main TUI by default. TUI 0.3.1 negotiates the Core 0.3.2
frontend-service extensions and requests a project probe, but a clean session remains
on the focused Welcome composer. `/setup` opens the Core-backed Setup selector;
`/lanes` opens the Core lane board. The full cockpit appears after a normal task
prompt or selection of a Core lane/session.

```text
/setup
/lanes
/setup provider
/connect
/models
/settings
```

Start the main cockpit with explicit startup overrides:

```bash
viden --provider deepseek --model deepseek-v4-flash
```

Use `--no-tui` when you need the legacy line REPL.

Start a side screen directly:

```bash
viden --tui-screen side-1 --provider deepseek --model deepseek-v4-flash
viden --tui-screen side-2 --provider deepseek --model deepseek-v4-flash
```

## Startup Flags

Common flags:

- `--provider <name>`: choose provider family.
- `--model <name>`: override model label.
- `--api-base <url>` and `--api-key <value>`: override provider connection.
- `--provider-plugin-dir <dir>`: add a dynamic provider plugin directory.
- `--permissions <level>`: set default permission level.
- `--session-home <dir>`: override transcript/index home.
- `--request-timeout <seconds>` and `--max-retries <n>`: tune provider HTTP
  behavior.
- `--config <path>`: load an explicit TOML config.
- `--resume [id|latest]`: resume a prior session at startup.
- `--tui`: start the main cockpit. This is the default.
- `--no-tui`: start the legacy line REPL.
- `--tui-screen <main|side-1|side-2>`: start a specific screen.
- `--tui-theme <aurora|aurora-light|ice|ice-light|mono|mono-light|amber|phosphor>`:
  select one of the eight registered palette profiles.

Preview flags for visual review:

- `--tui-preview`
- `--tui-preview-idle` for the first-launch welcome composer
- `--tui-preview-command-palette`
- `--tui-preview-live-turn`
- `--tui-preview-resize`
- `--tui-preview-cjk-input`
- `--tui-preview-lane`
- `--tui-preview-setup-wizard`
- `--tui-preview-provider-selector`
- `--tui-preview-provider-detail`
- `--tui-preview-model-selector`
- `--tui-preview-lane-selector`
- `--tui-preview-side`
- `--tui-preview-side-2`

Each preview also has an ANSI variant ending in `-ansi`.
`scripts/tui-previews.sh` writes to `target/tui-previews/0.3.1` by default;
`scripts/tui-regression.sh target/tui-regression/0.3.1` adds the structured
TUI 0.3.1 certification report without modifying accepted design references.

## Configuration

Viden loads:

1. platform config path;
2. `.viden/config.toml`;
3. environment variables;
4. CLI overrides.

Example:

```toml
provider = "deepseek"
model = "deepseek-v4-flash"
permission_mode = "auto_edit"
request_timeout_secs = 120
max_retries = 2

[providers.deepseek]
api_base = "https://api.deepseek.com"
api_key_env = "DEEPSEEK_API_KEY"
default_model = "deepseek-v4-flash"
models = ["deepseek-v4-flash", "deepseek-v4-pro"]
favorite_models = ["deepseek-v4-pro"]
```

The stable TUI 0.3.0 project-onboarding path is Core-backed. Setup renders the
Core project probe and keeps a local, secret-free D11-shaped draft containing
`project.name` and `project.pack`. Preview sends the draft's exact bytes through
`PreviewProjectConfig`. Confirm remains unavailable until Core returns a valid
preview matching the current draft, and only `ProjectConfigConfirmed` marks the
configuration complete. The TUI does not scan the project, write configuration,
or infer success locally.

`/connect` and `/provider` show supplier metadata; `/models` and `/model` show
configured model choices. TUI 0.3.0 has no trusted frontend secret-ingress
method, so these panels never collect credential bytes or serialize a
`/provider key` command. Provider detail displays active Core health and only a
masked credential-handle summary. If Core has no safe handle or ingress, it
shows `TRUSTED INGRESS unavailable` and remains read-only. Core 0.3.2 also does
not expose global session enumeration; `/lanes` selects only session ids
advertised on each Core lane.

TUI 0.3.1 opens `/settings` as a selector-first UI preference surface. It
offers locale (`system | en | zh-CN`), five skins, system/dark/light mode,
compact/regular/comfy density, system/reduced/full motion, terminal color
depth, Apply, and Reset. Amber and Phosphor are dark-only; their light choices
remain visible but disabled with an explanation. Every row shows the current
selection and its effect.

Stable Apply sends only a typed `UiPreferencePatch`; Reset sends
`ResetUiPreferences`. `CommandAccepted` means pending, not saved. The selector
reports success only after the matching `UiPreferencesUpdated` returns Core's
resolved value, persisted value, and diagnostics. A rejection keeps the draft
and shows Core's reason. CLI and user-config precedence is displayed from Core
diagnostics and is never re-resolved in the TUI. The same UI preference command
path remains available in Plan mode because Core classifies this presentation
mutation separately from project, file, shell, Git, workflow, and memory
effects.

Color depth is a clearly labeled, unsaved session-only terminal preview because
schema 1 has no persisted color-depth field. It never writes a TUI config or
local preference store. If `ui.preference_persistence` is absent, Settings is
still visible as unavailable, and Apply/Reset send no command.

Typed commands still use compact completion while you are editing, so a large
selector does not steal the composer before you press Enter. Direct commands
such as `/settings provider <provider-id> ...`, `/models <provider-id> <model>`,
and `/model <model>` remain available for scripts and advanced users. Provider
failures are classified
into recovery classes such as missing key, auth, rate limit, timeout, context
overflow, compatibility, and model unavailable; the recovery prompt includes
concrete commands to open doctor, switch model/provider, retry later, or use
fallback. When a provider rejects a request as too large, Viden records a
compaction note, retries once with a smaller provider request view, and keeps the
full local transcript intact for audit.

```text
/settings
/setup
/connect
/provider
/model <model>
/models
/settings provider <provider-id> [model]
/settings provider <provider-id> key-env <ENV_NAME>
/settings provider <provider-id> endpoint <url>
/settings provider <provider-id> default-model <model>
/settings provider <provider-id> enable-model <model>
/settings provider <provider-id> favorite-model <model>
/settings provider <provider-id> models <model> [model...]
/settings permissions <level>
/settings theme <name>
/settings save
```

Useful environment variables:

- `VIDEN_PROVIDER`, `VIDEN_MODEL`
- `VIDEN_API_BASE`, `VIDEN_API_KEY`
- `VIDEN_PROVIDER_PLUGIN_DIRS`
- `VIDEN_PERMISSION_MODE`, `VIDEN_SESSION_HOME`
- `VIDEN_REQUEST_TIMEOUT_SECS`, `VIDEN_MAX_RETRIES`
- `VIDEN_CONFIG`
- `VIDEN_SCREEN_LAUNCH_TEMPLATE`
- `VIDEN_LANE_CODEX_TEMPLATE`, `VIDEN_LANE_CLAUDE_TEMPLATE`
- `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `DEEPSEEK_API_KEY`,
  `DEEPSEEK_API_BASE`

## Providers

Use `/provider list` to inspect the runtime registry. The current built-in
registry includes:

- `anthropic`
- `deepseek`
- `deepseek-anthropic`
- `dashscope-coding-plan`
- `dashscope-coding-plan-anthropic`
- `dashscope-tokenplan`
- `dashscope-tokenplan-anthropic`
- `fallback`
- `groq`
- `kimi`
- `mistral`
- `ollama`
- `openai`
- `openai-compatible`
- `openrouter`
- `qwen`
- `together`
- `volcengine`
- `zhipu`

Provider commands:

```text
/provider
/connect
/provider <provider-id> [model]
/provider list
/provider doctor [provider-id]
/provider reload
/provider use <provider-id> [model]
/model [name]
/models
/models <provider-id> <model>
/settings
/settings permissions [mode]
/settings theme [name]
/setup
```

`fallback` is useful for offline smoke tests. It does not call a remote model.

Live provider smoke tests use the same runtime path as a normal non-TUI request
and store transcript evidence:

```bash
scripts/provider-live-smoke.sh --provider deepseek --model deepseek-v4-flash
scripts/provider-live-smoke.sh --provider dashscope-coding-plan --model qwen3.6-plus
scripts/provider-live-smoke.sh --provider dashscope-tokenplan --model qwen3.6-plus
scripts/deepseek-dev-scenario-smoke.sh --model deepseek-v4-flash
```

`dashscope-coding-plan` uses `DASHSCOPE_CODING_PLAN_API_KEY`.
`dashscope-tokenplan` uses `DASHSCOPE_API_KEY`.
The DeepSeek development scenario is billable. It writes `usage.json` and
`summary.md` with input/output/total token counts and an estimated CNY cost
using the configured model price.

## TUI Screens

Main cockpit:

- transcript and tool result stream;
- operation center for current activity;
- approval modal;
- workspace snapshot;
- active tasks;
- diagnostics;
- provider health;
- recent files;
- composer and status bar.

Side screens:

- `side-1`: agent lane monitor with lane state, attach hints, output tail, and
  artifacts.
- `side-2`: ops/evidence monitor with recent test, diff, provider, extension,
  and task evidence.

Open side screens from the TUI:

```text
/screen side-1
/screen side-2
/screen list
/screen close side-1
```

Route side screens through a terminal app, tmux, or monitor placement script by
setting `VIDEN_SCREEN_SIDE_1_LAUNCH_TEMPLATE`,
`VIDEN_SCREEN_SIDE_2_LAUNCH_TEMPLATE`, or `VIDEN_SCREEN_LAUNCH_TEMPLATE`.

## TUI Controls

TUI 0.3.1 uses explicit Normal, Insert, and Overlay ownership. The status bar
always shows the active mode.

- Normal: press `i` to enter Insert. Context keys act on the selected runtime
  fact instead of inserting text.
- Insert: printable keys edit the grapheme-safe multiline composer. `Enter`
  submits when idle and queues a follow-up during active work. `Shift-Enter` or
  `Alt-Enter` inserts a newline; `Enter` inside an open triple-backtick fence
  also inserts a newline. Bracketed paste preserves newlines and never sends.
- Overlay: arrows or `j`/`k` move focus, typing filters, and `Enter` applies the
  focused action. `Esc` unwinds overlay, then selection, then Insert while
  preserving the draft.
- Global chords work in every mode: `Ctrl-L` lane, `Ctrl-S` session, `Ctrl-T`
  new session, `Ctrl-K` command palette, `Ctrl-B` board, `Ctrl-G` decisions,
  and `?` context help.
- `Ctrl-C` requests cancellation only for the exact active Core owner. It does
  not deny an approval or exit directly. Two idle presses open exit
  confirmation only when Core reports no current work.
- Streaming, tool execution, and pinned approvals never lock the composer.
  Approval shortcuts belong to an explicitly focused approval: arrows or
  `Tab` reach Deny, Diff, and Approve; typed scopes are sent with the stable
  Core request id and owner.
- `PageUp` / `PageDown` browse transcript history; `Ctrl-Home` jumps to the
  oldest visible history and `Ctrl-End` returns to the live tail. CJK and
  grapheme cursor placement use terminal cell width.
- Mouse capture is off by default so native text selection keeps working. TUI
  0.3.1 has complete keyboard paths but does not yet expose the optional
  `mouse_capture=true` opt-in described by the design.

## Slash Commands

Runtime:

```text
/help
/status
/config
/doctor
/mode [build|plan]
/permissions [level]
/plan [on|off]
/test <command>
```

`/plan` is an immediate TUI command: it switches to Plan mode and returns the
composer to normal input without starting a provider turn. On the first-launch
welcome screen, `/plan` keeps the welcome composer visible until a real task
prompt starts the session.

Plan mode is for planning product requirements, architecture, implementation
approach, test strategy, and development steps. It may read and inspect the
project, but it does not write code, modify files, run mutating shell/Git/workflow
actions, or persist the plan. Plan output stays in the transcript until you
confirm implementation and switch back to Build with `ask`, `auto_edit`,
`read_only`, or `full_access`.

Sessions:

```text
/sessions
/resume latest
/resume #<index>
/resume <session-id-prefix>
/diff
```

Repository and web:

```text
/git status
/git diff [path]
/git branch
/git add [--all|-A] <path...>
/git restore [--staged] [--source <ref>] <path...>
/git switch <branch> [--create]
/git commit [--all] <message>
/git push [branch] | [remote branch] [--set-upstream|-u]
/git stash <list|push|pop|drop>
/git worktree <list|add|remove>
/web search <query> [--limit <n>] [--site <domain>]
/web fetch <url> [--max-bytes <n>] [--raw]
```

Code intelligence:

```text
/lsp status
/lsp diagnostics <path>
/lsp symbols <path>
/lsp references <path> <line> <character>
```

Tasks and memory:

```text
/tasks
/task add <title>
/task view <task-id>
/task update <task-id> <title>
/task status <task-id> <todo|in_progress|blocked|done|archived>
/task link <task-id> <depends-on-id>
/task block <task-id> <reason>
/task unblock <task-id>
/task archive <task-id>
/task restore <task-id>
/task resume-context
/brief <task goal>
/spec <task goal>
/brief show
/brief clear
/brief steering init
/brief steering show
/memory project
/memory session
/memory suggest <content>
/memory confirm <memory-id>
/memory reject <memory-id>
/memory prune <memory-id>
/memory add <content>
/memory export
/context
```

Agents and lanes:

```text
/agent list
/agent doctor [id]
/agent run codex [--write|--app-server] <task>
/agent review codex [--base <ref>] [prompt]
/agent challenge codex [prompt]
/agent status
/agent result <id>
/agent cancel <id>
/agent logs <id>
/lane codex <task>
/lane codex-review <task>
/lane claude <task>
/lane run <command>
/lane ask <tool> <task>
/lane inspect <id>
/lane timeline <id>
/lane diff <id>
/lane artifacts <id>
/lane stop <id>
/lane retry <id>
/lane attach <id>
/lane tmux <id>
/lane pty <id>
/lane send <id> <input>
/lane detach <id>
/lane accept <id>
/lane revise <id>
/lane discard <id>
/lane apply <id>
/lane resolve <id>
/lane archive <id>
/lane cleanup <id>
```

`/agent doctor [id]` reports each adapter capability record: readiness,
mutation mode, evidence mode, config source, and known limits. Use it before
trusting a delegated Codex/Claude/template/tmux/ACP lane.

Inside the TUI, `/lane` opens a centered action selector. It lists lane launch
commands and adds id-specific inspect, timeline, diff, and artifacts actions for
active lanes, so you can operate lanes without memorizing their ids.

`/lane codex-review <task>` is the P0 read-only Codex trust-loop path. It writes
an envelope, launches `codex review --uncommitted` when Codex is available (or a
`VIDEN_LANE_CODEX_REVIEW_TEMPLATE` override when configured), and stores the
result in the same log/timeline/evidence model as other lanes.

`/lane tmux <id>` now preflights the default tmux/Claude path before marking a
lane attached. Missing `tmux` or `claude` produces a setup-needed timeline event
instead of a false attached state; custom templates can be supplied with
`VIDEN_LANE_TMUX_TEMPLATE` and `VIDEN_LANE_TMUX_COMMAND_TEMPLATE`.

`/lane inspect <id>` reports the lane status, command, exit code, log, artifacts,
envelope, timeline, decision, next action, and changed files. `/lane timeline
<id>` prints the ordered event stream for review/apply/debug evidence.

Extensions:

```text
/extensions list
/extensions doctor
/mcp list
/mcp doctor
/skills list
/skills list --all
```

## Built-In Tools

Model tool calls and fallback `tool ...` syntax can use:

- `read_file`
- `write_file`
- `edit_file`
- `glob`
- `grep`
- `shell`
- `web_search`
- `web_fetch`
- `git_status`
- `git_diff`
- `git_branch`
- `git_switch`
- `git_add`
- `git_restore`
- `git_commit`
- `git_push`
- `git_fetch`
- `git_stash_list`
- `git_stash_push`
- `git_stash_pop`
- `git_stash_drop`
- `git_worktree_list`
- `git_worktree_add`
- `git_worktree_remove`
- `lsp_diagnostics`
- `lsp_symbols`
- `lsp_references`

Example fallback tool syntax:

```text
tool read_file path=Cargo.toml
tool grep pattern=SessionEngine path=viden-runtime/src
```

## Modes And Permissions

Viden separates work intent from trust level:

- Work Mode: `build` for implementation, `plan` for requirements,
  architecture, implementation approach, test strategy, and development plans.
- Permission Level: `ask`, `auto_edit`, `read_only`, or `full_access`.

`/plan` is a shortcut to Plan work mode with Read Only permission level. It
does not write code. `/permissions` changes the trust boundary only; it does not
change provider/model or the work mode.

Compatibility aliases such as `default`, `acceptEdits`, `bypassPermissions`,
and `dontAsk` are still accepted by the parser for older configs and scripts,
but new docs and UI use the canonical permission-level names.

The permission path covers file mutation, shell, Git, workflow task/memory
changes, and write-capable delegated Codex jobs.

## Sessions, Tasks, And Memory

Sessions are stored as JSONL transcripts with a rebuildable SQLite index. Use
`/sessions` and `/resume` to continue previous work.

Tasks and memory are stored as workflow events. Assistant-suggested project
memory must be confirmed before it becomes active project memory.

`/task resume-context` combines task and memory state into a resumable project
context snapshot.

`/brief <task goal>` creates a lightweight active task brief under
`.viden/briefs/active.md`; `/spec` is an alias. When an active brief exists,
provider ContextBundles, lane envelopes, and side-2 ops can reference it.
`/brief steering init` creates minimal `.viden/steering/` templates for
project conventions, architecture, and workflows.

`/context` shows the latest provider ContextBundle, including the v1 policy,
source priorities, token estimates, omitted sources, and compaction notes. It is
read-only and does not expose raw secret values.

## Agent Lanes

Agent lanes let Viden supervise external tools without pretending they are
native model calls. Current adapters include:

- Codex CLI / app-server entrypoints.
- Claude Code via command template.
- Custom template agents.
- Shell lanes.
- Tmux lane attachment.
- Embedded PTY lane.
- Experimental ACP command surface.

Lane artifacts are written under `.viden/lanes/` so the main TUI and side
screens can show next actions, output tails, decisions, apply results, and
conflicts.

## Extension Boundaries

Viden can discover extension surfaces today:

- provider plugin directories;
- agent adapters;
- MCP config files;
- local skills under project, user, and legacy skill roots.

Current boundary: MCP-backed tools are visible but not yet wired into the
mutation permission path. Skills are task recipes, not direct tools.

## Release Evidence

For `0.1.16`, the release was verified with:

- clippy-as-gate;
- workspace tests;
- deterministic TUI screenshots;
- fallback CLI smoke;
- Codex app-server protocol fixture;
- app-server write guard;
- lane operator-loop smoke;
- GitHub release asset validation;
- Homebrew formula validation.

See [0.1.16 Status](release-0.1.16-status.md) for evidence paths and release
asset names.

## Feedback

Open a GitHub issue with:

- Viden version and install method;
- OS and terminal app;
- provider/model;
- command and reproduction steps;
- screenshots or logs with secrets redacted.
