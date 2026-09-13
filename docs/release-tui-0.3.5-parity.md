# Viden TUI 0.3.5 Parity — Plan Documents, Session Control, Approval Scopes, Starter-Lane Agent

Chinese version: [release-tui-0.3.5-parity.zh-CN.md](release-tui-0.3.5-parity.zh-CN.md)

TUI parity minimum for the four `0.3.5` Core capabilities: `runtime.plan_documents`
(C13), `runtime.agent_session_control` (C14), `runtime.approval_scopes` (C15,
GUI-CORE-019), and `runtime.starter_lane_agent` (C15, GUI-CORE-030). The contract
is [Frontend Integration Contract](frontend-integration-contract.md), sections
"Plan documents", "Session Control", "Approval Scopes", and "Starter-Lane Agent
Binding"; the design and its shipped amendments are
[0.3.5 Core Contract Increment](release-0.3.5-contract-design.md).

Core stays authoritative for every fact below. The TUI keeps no plan, pauses no
session by withholding input locally, remembers no grant, and starts no session
itself. Every refusal quoted to the operator is Core's own reason text.

## Plan Lens (`runtime.plan_documents`)

`/plan` opens an overlay over `RuntimeViewState.plan_documents`. The lens holds
no plan: the record is re-read from the view on every frame and again on every
keypress, so a revision that moved between render and keypress is decided on
what Core holds now, or refused by Core as stale.

Which plan is shown: a focused Lane narrows to the plans Core scoped to that
Lane, and only when Core published at least one for it; within the candidates
the *open* plan wins (`Drafting`, `Proposed`, `Accepted`) and ties break on
Core's own `updated_at` (`apps/tui/src/tui/plan_lens.rs:168`).

### What the rows show

| Row | Core fact |
| --- | --- |
| `<title> · <status> · rev <n>` | `title`, `status`, `revision` |
| `GOAL` | `goal` |
| `AUTHOR` | `proposed_by` — `assistant · session … · turn …` or `operator` |
| `DIGEST` | `content_sha256` |
| section headers and bodies | `sections`, one header per `PlanSectionKind` |
| `STEPS <n>` then one row per step | `steps` |
| `HANDOFF dag … · audit …` | `handoff`, absent until Core publishes one |

Step glyphs are the registered TUI status vocabulary
(`docs/viden-design/Viden/docs/DESIGN-REF.md`, "TUI 字形词表"): `◌` wait for
`Pending`, `▶` run for `InProgress`, `✓` done for `Completed`. Each step row
also carries its `role` and its `dependencies` (`· after step_1`).

### What each key sends

| Key | Command | Sent only when |
| --- | --- | --- |
| `a` | `AcceptPlanDocument { plan_id, revision, content_sha256 }` | `status == Proposed` |
| `r` | opens the reason line, then `RejectPlanDocument { …, reason }` | `status == Proposed` |
| `h` | `HandOffPlanDocument { plan_id, revision, content_sha256 }` | `status == Accepted` |
| `↑` `↓` | scroll only | always |
| `Esc` | closes the reason line, then the lens | always |

`revision` and `content_sha256` are copied out of the record read at the instant
of the keypress (`apps/tui/src/tui/app.rs:1477`,
`apps/tui/src/tui/plan_lens.rs:414`), never out of the row that was drawn. The
key mapping is `apps/tui/src/tui/app.rs:1402`; the status gate is
`apps/tui/src/tui/app.rs:1418`.

### Absent and disabled states

- **No plan proposed.** An empty `plan_documents` renders as one row saying so.
  Nothing here parses a plan out of assistant prose or renders an empty plan.
- **No capability.** The lens still opens and its single row names
  `runtime.plan_documents`, which keeps "this Core does not publish plans"
  distinguishable from "no plan was proposed".
- **A key the status refuses** is listed and labelled — `h Hand off ·
  unavailable in proposed` — and sends nothing at all: Core's own contract says
  it refuses, so it is not asked.
- **Hand off in Plan mode** keeps its key and is labelled `· refused in plan
  mode`. Only Core may say the mode refuses it, so the command is sent and the
  `plan_mode` refusal is rendered from Core's reason.
- **An empty or over-bound rejection reason** is refused locally and nothing is
  sent: Core stores the reason and refuses an empty one, and the client refuses
  rather than truncating (`apps/tui/src/tui/app.rs:1444`).
- **A `Superseded` record** says so in place. A status this build cannot name
  renders as unknown, never as `Proposed` and never as decided.

## Session Control (`runtime.agent_session_control`)

The Lane view's `/acp` session rows gain the five controls. Every key is
intercepted before the input reducer, exactly as the retry key has been since
this picker existed (`apps/tui/src/tui/app.rs:500`).

| Key | Command | Accepted from |
| --- | --- | --- |
| `p` | `PauseAgentSession` | `Starting`, `Running`, `WaitingApproval` |
| `u` | `ResumeAgentSession` | `Paused` |
| `k` | `KillAgentSession` | any non-terminal status, `Paused` included |
| `s` | `CancelAgentSession` (unchanged) | any non-terminal status |
| `r` | `RetryAgentSession` (unchanged) | `Failed`, `Cancelled`, `Killed` |

`r` stays Retry, which it has been since the picker existed, so Resume takes
`u`: re-pointing a key an operator already has in their fingers is how a resume
becomes a restart. Every command is sent under the session's own Core-published
owner, the rule cancel already followed (`apps/tui/src/tui/app.rs:538`). The
status gate is `apps/tui/src/tui/session_control.rs:94`; the payloads are
`apps/tui/src/tui/session_control.rs:117`.

### What the rows show

`⏸ codex-acp · tidy the config loader · Paused · held 2`

- the registered status glyph (`apps/tui/src/tui/session_control.rs:163`): `◌`
  wait `Starting`, `▶` run `Running`, `◆` your-turn `WaitingApproval`, `✓` done
  `Completed`, **`⏸` gate `Paused`**, **`✗` fail `Failed` and `Killed`**, `⚠`
  warning `Cancelled`. `⏸` is the vocabulary's own pause mark and `✗` is what an
  operator-terminated session is; neither glyph is new.
- `· held <n>` from `control.held_input_count` — the only honest source for
  "typed but not sent";
- `· ended by kill` / `· ended by cancel` from `control.terminated_by` — the
  only thing that tells a kill from a cancel once both have settled.

Both are omitted when Core published no `control`: absence is absence, never
zero (`apps/tui/src/tui/session_control.rs:281`).

### Absent and disabled states

- A control the session's current status refuses is labelled `· unavailable in
  <status>` and sends nothing.
- Without the extension, Pause, Resume and Kill are labelled with the missing
  capability and Stop and Retry stay live, which keeps "this Core cannot pause"
  distinguishable from "this session cannot be stopped".
- A `session_status_<current>` refusal reaches the transcript as Core's own
  reason (`apps/tui/src/tui/app.rs:588`), correlated on this client's own
  command id in a slot separate from the supervision and `/git` machines.
- **Switch model** is stated, not offered: a second inert row reads `Switch
  model = k Kill + new session: model, load_session_id`. Core publishes no
  switch command and the contract composes it from two steps.

## Approval Scopes (`runtime.approval_scopes`, GUI-CORE-019)

The `Always → AllowOnce` projection is retired
(`apps/tui/src/tui/projection.rs`): telling an operator they were allowing one
call while Core was offering a rule that outlives every session and every
workspace was the opposite of an informed decision.

| Key | Decision | Sent only when |
| --- | --- | --- |
| `1` / `y` | `Allow { Once }` | `allowed_scopes` carries `once` |
| `2` | `Allow { Session }` | `allowed_scopes` carries `session` |
| `3` | `Allow { RepoAllowlist }` | `allowed_scopes` carries `repo_allowlist` |
| `a` | `Allow { Always }` | `allowed_scopes` carries `always` |
| `4` / `n` | `Deny` | always |
| `e` | opens the revision field, then `Edit { revised_input }` | `editable` |

`a` is `apps/tui/src/tui/input.rs:130`; `e` is
`apps/tui/src/tui/input.rs:135` and `apps/tui/src/tui/input.rs:165`. The
revision is decoded with the contract's own `ToolInput` encoding —
`viden_types::decode_tool_input`, `key=value` tab-separated —
(`apps/tui/src/tui/input.rs:219`), never parsed out of the rendered
`input_preview`.

`/approvals` lists `RuntimeViewState.standing_approvals` with each rule's tool,
content, granting workspace and audit row; `Enter` revokes the selected
`rule_id` through `RevokeStandingApproval`
(`apps/tui/src/tui/approval_scopes.rs:33`, `apps/tui/src/tui/app.rs:1267`). The
list is Core's field, never this client's memory of grants it witnessed.

### Absent and disabled states

- A scope Core did not offer is shown disabled with ` · unavailable` and its key
  sends nothing. `Always` is not offered for a high-risk request or in plan
  mode, and that is the disabled row an operator sees.
- A request Core marks not editable shows `e Edit input · Core marks this input
  not editable` and the key does nothing.
- A revision that decodes to nothing is refused locally: `A revision is
  key=value pairs. Nothing was sent.`
- A second request produced by an edit carries `REVISES <request_id>` from
  `revised_from`, so the operator replaces the row they were looking at rather
  than seeing two unrelated prompts.
- An empty `standing_approvals` renders as absence; without the extension the
  list names the missing capability.

### Known gap — the revision field cannot be pre-filled

`ApprovalRequestView` carries no typed `ToolInput`. `input_preview` is a
rendered `key: value` dump built for a human reader
(`crates/permissions/src/lib.rs:435`), and `candidate_paths` exists in that same
module precisely so approval UIs do not scrape display text. A client therefore
cannot show "the editable input fields" and cannot pre-fill a revision without
scraping.

What shipped: the field opens empty and its prompt states the gap — `Revision ·
key=value, tab-separated · Enter sends · Esc discards · Core publishes no typed
input to pre-fill`. The operator types the revision, and everything downstream —
the re-decision, the second request, the `revision_unchanged` refusal — is
Core's, unchanged. Filed as a Core contract request; see "Core Contract
Requests" below.

## Starter-Lane Agent Binding (`runtime.starter_lane_agent`, GUI-CORE-030)

Lane creation is two steps. The task step hands over to an Agent pick built from
`RuntimeViewState.agent_adapters`; nothing is sent until the pick is confirmed,
and `Esc` unwinds the pick back to the task rather than closing the panel.

| Row | Binding sent |
| --- | --- |
| `No Agent · create the Lane, start a session later` (always first) | `agent: None` — the two-step flow, byte-identical to what this client sent before the field existed |
| `<agent_id> · adapter default model · <startability>` | `agent: { agent_id, model: None, task: None }` |
| `<agent_id> · <model> · <startability>` | `agent: { agent_id, model: Some(model), task: None }` |

One row per adapter plus one per model that adapter published
(`apps/tui/src/tui/starter_lane_agent.rs:76`), so picking a model never means
naming one this client could not verify exists. `task` is left unset: Core
defaults it to the preset's Lane summary, and the composer text is the first
turn, which the creation flow submits separately
(`apps/tui/src/tui/starter_lane_agent.rs:55`).

### What is echoed and what is rendered

- The **preview's** `agent` is rendered from Core's echo, not from this client's
  memory of the pick, together with every preview diagnostic — so an `agent_id`
  Core does not know appears as `starter_lane.agent_unknown:<id>` before the
  receipt arrives.
- The **receipt's** `session` outcome is stated: `Started` names the session id;
  `Refused` names Core's reason beside a Lane that exists, because Lane creation
  never fails because a session did not start.
- Without the extension the pick list is the two-step flow alone and the row
  names the missing capability: a binding this Core would drop is worse than no
  binding, because the operator would believe a session was starting.

## Regression Frames

Four frames were added to `scripts/tui-previews.sh` and
`scripts/tui-regression.sh`, each asserted on a Core fact rather than on a
layout:

| Frame | Asserted content |
| --- | --- |
| `main-plan-lens` | `rev 2`, `a Accept`, `step_1`, `Hand off · unavailable in proposed` |
| `main-session-control` | `held 2`, `ended by kill`, `p Pause`, `u Resume`, `k Kill`, `load_session_id` |
| `main-approval-scopes` | `a Allow always`, `e Edit input`, `REVISES` |
| `main-lane-agent` | `No Agent`, `claude-sonnet` |

### Baselines re-pinned by T4, with causes

| Baseline | Cause |
| --- | --- |
| `jump::tests::index_groups_typed_runtime_facts_in_stable_order_and_keeps_file_unavailable` | 16 → 18 command rows: `/plan` and `/approvals` joined the registry |
| `modal::tests::global_jump_windows_rows_to_keep_selected_item_visible` | selected row 14 → 16, same cause |
| `modal::tests::global_jump_window_keeps_default_disabled_tail_selected` | disabled FILES tail 16 → 18, same cause |
| `preview::tests::previews_use_stable_demo_workspace_snapshot` | the palette's eight-row window now ends at `/decisions`, so it asserts `/approvals` and `/plan` instead of `/evidence`, which is one row past it |
| `modal::tests::acp_picker_lists_lane_sessions_before_truthful_adapter_rows` | the adapter row moves from index 1 to 3; the two inert control rows are appended *after* the sessions, so no session row's index shifts |
| `app::tests::native_lane_task_waits_for_preview_and_receipt_before_submitting` and `…_preserves_direct_workspace_preview_without_fake_branch` | Lane creation gained the Agent step; both now assert nothing is sent until the pick is confirmed |
| approval overlay height 14 → 18 (`modal.rs`) | without it the new standing-scope and revision rows pushed `Deny` and `Edit` off the bottom of the panel |
| `apps/tui/release-manifest.toml` `catalog_en` / `catalog_zh_cn` | both catalogs gained the plan, session-control, approval-scope and starter-Lane keys in English and Simplified Chinese |

`scripts/tui-regression.sh` passed on the finished branch: 24 text frames, the
`.ansi` renderings, and 24 `.svg` artifacts, plus the 22 certification tests and
the boundary scan.

## Live Offline Check, 2026-09-13

Run in tmux (140x40) with `--provider fallback --model test-local` against a
scratch Git repository created for this run under the session scratchpad, with
`VIDEN_HOME` pointed at a scratch directory beside it. The engine home resolved
to the scratch repository's own `.viden/` (`SessionStore::new` prefers
`<cwd>/.viden`), so the standing-approval file is
`<scratch repo>/.viden/settings.toml`. This repository's `.viden/` and
`~/.viden/` were neither read nor written — both are unmodified. The fixture is
one commit (`4f78a0c Add the fixture README`) on `main`.

The plan was driven through the real tool path: the fallback provider forwards
`tool <name> key=value …` as a model tool call, so `update_plan` was executed by
the registry and folded into a plan by the runtime exactly as a planner's call
would be.

| # | Step | Pane text |
| --- | --- | --- |
| 1 | `tool update_plan title=Split-the-supervisor plan=[…3 steps…]` | `EVIDENCE  Recorded the plan `Split-the-supervisor`: 3 step(s) (1 pending, 1 in progress, 1 completed) and 0 section(s). The operator decides it; it is not work until they hand it off.` |
| 2 | `/plan` | `Split-the-supervisor · proposed · rev 1` · `DIGEST d71340819cc71fa0…` · `✓ step_1  Extract-the-lane-seam` · `▶ step_2  Move-the-supervisor-arms` · `◌ step_3  Re-run-the-frozen-digests` · `a Accept` · `r Reject…` · `h Hand off · unavailable in proposed` |
| 3 | `a` | `Split-the-supervisor · accepted · rev 1` · `a Accept · unavailable in accepted` · `r Reject… · unavailable in accepted` · `h Hand off` — the decision rows flipped on Core's own status change |
| 4 | `tool shell command=pwd` | `APPROVAL approval_1789288934582889000 · Active · AUDIT audit_1789288934582893000` |
| 5 | `Ctrl-G`, `Enter` | `Approve shell · Medium` · `1 Allow once` · `2 Allow for session · unavailable` · `3 Add repo allowlist · unavailable` · `a Allow always` · `4 Deny` · `e Edit input` |
| 6 | `a` | The request resolved. `<scratch repo>/.viden/settings.toml` now holds `[[permissions.standing]] … rule_content = "command=pwd" … tool_name = "shell"`, and the audit log holds `audit_1789288934582893000 approval.always`. |
| 7 | Client restarted, `tool shell command=pwd` again | Ran with **no approval at all**: `EVIDENCE  /…/t4-smoke/repo`. The rule loaded at bootstrap in a new process, which is the whole difference from `session` and `repo_allowlist`. |
| 8 | `/approvals` | `> shell  command=pwd · granted in ws_dd36031d32c89aa8 · audit audit_…` |
| 9 | `Enter` (revoke) | `No standing approvals. Core holds no rule that outlives this session.`; the file reads `[permissions]` / `standing = []`; audit row `approval.revoke`. |
| 10 | `tool shell command=pwd` once more | `APPROVAL approval_1789289107439888000 · Active` — the request returned. |
| 11 | `Ctrl-G`, `Enter`, `e`, `command=id` | `Revision · key=value, tab-separated · Enter sends · Esc discards · Core …` · `› command=id` |
| 12 | `Enter` | Core re-decided the revision and published a **second** request: `TARGET command: id`, new id `approval_1789289132946687000`, fresh `AUDIT audit_1789289132946687000`, and `REVISES approval_1789289107439888000`. |
| 13 | `1` | The revised input ran; audit row `approval.allow_once`. |

Audit rows written by the run, read out of band from the scratch tree:

```
audit_1789288863870522000 plan.accepted
audit_1789288934582893000 approval.always
audit_1789289094067974000 approval.revoke
audit_1789289107439893000 approval.edit
audit_1789289132946687000 approval.allow_once
```

One harness note, not a client one: one `USER` row reads `itool read_file
path=README.md`, because the driver script sent an `i` while the composer was
already in Insert mode. It is the same artifact T2, T3 and E2 recorded.

## Not Covered By The Live Run

- **Pause, resume and kill were not exercised live.** They need a running ACP or
  Codex app-server session, which the offline fallback provider cannot start;
  the fallback path has no Agent session to control. The commands, the status
  gate, the glyphs and the `session_status_<current>` refusal are covered by the
  crate suite (`session_control.rs` and `app.rs` tests) against the shared
  `agent-session-control.json` fixture's own statuses, and the frame
  `main-session-control` renders both a paused and a killed session.
- **The starter-Lane binding was not exercised live** for the same reason: no
  adapter is startable offline, so a bound session would be refused for an
  environmental reason rather than a contract one. The binding on the request,
  the preview echo and both receipt outcomes are covered against the shared
  fixture in `app.rs`.
- **Hand off** was not sent live: the accepted plan's handoff spawns an agent
  DAG, which is work rather than evidence, and Plan mode was not active in this
  run so the `plan_mode` refusal had nothing to refuse.

## Core Contract Requests

1. **`ApprovalRequestView` needs the typed `ToolInput` for an `editable`
   request.** Core already holds it — `PermissionPrompt.input`, added by C15 for
   the byte-identical `revision_unchanged` check — but does not publish it. The
   only thing on the wire is the rendered `input_preview`, and the contract's own
   rule is that clients do not scrape display text. Until it is published no
   client can render "the editable input fields" or pre-fill a revision; the TUI
   opens an empty field and says so.
2. **`viden_core` does not re-export `StandingApprovalView`,
   `StarterLaneAgentBinding`, or `StarterLaneSessionOutcome`.** Every other type
   in these four capabilities is re-exported from the facade. The TUI reaches
   them through its direct `viden-types` dependency, which works but means three
   `0.3.5` types are absent from the crate a frontend is supposed to consume.

Neither blocks this batch.
