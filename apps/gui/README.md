# Viden GUI

Chinese version: [README.zh-CN.md](README.zh-CN.md)

This directory is the GUI implementation track for Viden. The alpha evidence
gate selected Tauri, and `0.1.0-rc.3` certifies the canonical D1 cockpit
against the Core `0.3.5` same-state fixture. `0.1.0-beta.1` established the
single production
desktop bootstrap. The native launcher opens a frontend-safe `LocalCoreHost`
workspace and injects its `CoreClient` through `GuiCoreAdapter`. Once connected,
the app always presents the D1 cockpit shell. With no bound workspace, `Open
project` opens the native folder picker and rebinds through
`LocalCoreHost::open_workspace`; it never enters D11 or asks for a model. A
bound zero-Lane project renders the project cockpit with `New Lane`, which
opens the D1 New Lane popover for quick native or ACP lane startup. The exact
Core Lane receipt returns focus to D1.

## Run the desktop client locally

From `apps/gui`, install the pinned frontend dependencies and start the native
Tauri development window:

```bash
npm ci
npm run tauri -- dev
```

Vite and Tauri share the strict development endpoint
`http://localhost:1420`; the command fails instead of silently moving to a
different port. The native bootstrap binds a workspace only when
`VIDEN_GUI_WORKSPACE` is explicitly set. Otherwise the D1 welcome stays
available for native folder
selection; a real Core bootstrap failure renders explicit D6 disconnected state
rather than simulating success.

On macOS the native traffic-light controls stay available in an overlay title
bar, while the HTML shell supplies the dark draggable surface. The prototype
window border, rounded frame, and white native title strip are not rendered.

To build and open a self-contained macOS debug app that does not depend on the
Vite development server:

```bash
npm run tauri -- build --debug --bundles app
open ../../target/debug/bundle/macos/Viden.app
```

To bind an explicit project when launching the binary directly:

```bash
VIDEN_GUI_WORKSPACE=/absolute/project/path \
  ../../target/debug/bundle/macos/Viden.app/Contents/MacOS/viden-gui
```

Before Core bootstrap, the desktop host prepends existing standard user tool
directories such as `~/.local/bin`, Bun, Volta, asdf, mise/fnm, and Homebrew to
the inherited `PATH`. This keeps agent discovery and the later ACP spawn on the
same command path when the App is opened through Finder, without executing a
login shell or embedding a machine-specific absolute path.

## Frozen input

| Field | Value |
| --- | --- |
| GUI component version | `0.1.0-rc.4` |
| Minimum Core version | `0.3.5` |
| Supported frontend schemas | `[1]` |
| Common branch base | `3a7740ea72e58f4a22248a80f9e7324c49bb0f73` |
| Core final checkpoint | `f7fe1b31dfb237e4062209767a7051c2b2c68b93` |
| Core code checkpoint | `1cec82185bbe860d6b8536a63741bc01f1edf2f6` |
| Contract payload | `5bd2b80b0953f4194d082940a7b9164c7231ca2d` |
| Canonical D1 fixture | `d1-main-cockpit.json`, SHA-256 `05ac25909beaa84942a0468d2ae2d8058e7348bd0bd2b7bc86d6fd344fe69439` |
| Required Core capabilities | 15 frozen values plus additive extension capabilities, including `runtime.cockpit_context_v1` |
| Built-in locales | `en`, `zh-CN` |
| Appearance | 5 skins, 8 valid skin/mode pairs, 3 densities, 3 motion policies |

The active machine-readable manifest is
[release-manifest.toml](release-manifest.toml). Its immutable rc.4 snapshot is
[manifests/0.1.0-rc.4.toml](manifests/0.1.0-rc.4.toml); both files must
remain byte-equivalent for this release checkpoint. Earlier alpha, beta, rc.2,
and rc.3 snapshots remain historical evidence and are not rewritten. `rc.4`
adds the DiffReview and EvidenceView surfaces and the four `0.3.3` Core
capabilities; its `[evidence].root` deliberately still names the rc.3 tree,
because no `0.1.0-rc.4` acceptance tree was produced and naming one that does
not exist would be worse than naming the one that does.

## Design source order

Visual and interaction inventory starts from the accepted design hierarchy:

1. `docs/viden-design/Viden/index.html`
2. `docs/viden-design/Viden/GUI/Viden - 设计稿索引 (GUI).html`
3. `docs/viden-design/Viden/GUI/Viden - 组件库 (GUI).html`
4. `docs/viden-design/Viden/GUI/Viden - 桌面驾驶舱 (GUI).html` (D1)

D11 project configuration, D4 Lane creation, and D6 operational recovery are
subordinate screens under `docs/viden-design/Viden/GUI/pages/`. They inform the
operator loop but do not replace D1 as the desktop cockpit baseline.

The deterministic design revision also includes the registered component
semantics and the local sources actually consumed by these screens:
`docs/DESIGN-REF.md`, `GUI/gui-kit.css`, `GUI/gui-icons.jsx`,
`GUI/gui-titlebar.jsx`, `GUI/gui-statusbar.jsx`, `GUI/gui-inbox.jsx`, and
`GUI/gui-settings.jsx`. The manifest records the exact ordered list; archived
or mock sources are excluded.

## Core boundary

GUI code may depend on `viden-core` and GUI-owned framework/platform code only.
The allowed Core entry points are:

- `CoreClient`, `CoreTransport`, `StatefulCoreClient`, and `LocalCoreTransport`;
- `CoreHandshake`, schema/capability constants, command/event envelopes,
  snapshot, replay, transcript paging, and `RuntimeViewState`;
- frontend-neutral domain records re-exported by `viden-core`.

GUI must not import `viden_core::legacy`, `viden-runtime`, `viden-provider`,
`viden-tools`, `viden-permissions`, `viden-session`, `viden-workflows`,
`viden-context`, or config internals. Every mutation is sent as a
`RuntimeCommand`; visible success waits for `CommandAccepted` and the
subsequent ordered state events.

On the frontend side the same discipline has a single host seam:
`src/host/core_client.ts` defines the transport-neutral `GuiCoreClient`
interface (distinct from the Rust `CoreClient` above), and
`src/host/tauri_core_client.ts` is the only frontend module allowed to import
`@tauri-apps/*`. Screens and the shell consume the injected interface only, so
replacing the desktop host means supplying another `GuiCoreClient`
implementation, not editing screens.

## Inventory against Core `0.3.5`

| GUI area | Design intent | Core `0.3.5` status | GUI handling |
| --- | --- | --- | --- |
| Project open / D11 intake | native folder open plus project probe, provider health, config preview/confirm, and credential handles | `LocalCoreHost::open_workspace` provides trusted folder rebinding and `runtime.recent_work` answers `QueryRecentWork`; Core publishes no first-run intake signal, no repository-clone or project-scaffold command, and no concurrent multi-root supervision | Welcome uses the native folder picker and host rebind directly and lists Core's recent projects; the titlebar project picker adds the guarded switch. D11 stays an explicit in-project configuration flow and never owns folder open. Reachable at `?screen=d11` and from the project picker's `Configure this project…` row; the shell never redirects into it on its own |
| D4 lane creation | typed role, route, gate strength, mutation policy, target, budget, worktree preview, lane receipt | `PreviewStarterLane`/`CreateStarterLane`, Core-resolved preview, invalidation, approval, exact receipt, and `runtime.starter_lane_preview` advertisement are available | Task 8 renders the four-step reviewed flow; older Core handshakes still fail closed visibly with zero sends |
| D1 cockpit | no-project welcome center, zero-Lane project cockpit, activity/lane rails, streaming transcript/tool rows, Environment, Live Work, composer, evidence/context/cost facts | Stream/tool/approval/queue/task/lane/owner/evidence/context/cost/preferences/recent-work facts exist; diff/apply, stable audit timeline, actionable lane recovery, and concurrent multi-workspace supervision remain incomplete | No bound host renders Welcome; a bound empty project remains D1 and exposes `New Lane`; the Lane rail groups its Lanes under the one project Core supervises (`GUI-CORE-023`); live work renders from `RuntimeViewState` |
| Permission dock | scoped approve/deny, risk, target, expiry, default action, audit id | `ApprovalRequestView` and `RespondToApproval` exist | Usable through Core; GUI cannot execute tools directly |
| D2 decision center | one cross-Lane queue over gate approvals, lane asks, and contract confirmations, on one card skeleton of context, evidence, and action bar | `pending_approvals` with `RespondToApproval`, `review_requests` with `DecideReview`, and `contracts` with `ConfirmContract` exist; a structured approval diff and a pending-contract fact are missing | Reachable at `?screen=d2`; gate, contract, and review decisions all send Core commands, a review verdict carries an optional reviewer note and confirms only on the ordered `ReviewRequestUpdated` Core publishes, a review Core would refuse stays disabled under `D2-REVIEW-SETTLED` or `D2-NO-REVIEWER-ACTOR`, the approval diff stays unavailable under `GUI-CORE-012`, and the contract group is labelled decided history under `GUI-CORE-013` with both contract verdicts disabled-and-labelled by the same code, because Core refuses a second decision on a record it already holds |
| D10 lane monitor | one card per Lane across every project, with gate strength, status, progress, evidence, cost meterability, and an attention count | `lanes`, `lane_runtime_owners`, `tasks`, `agent_sessions`, `latest_evidence`, and `AgentLaneRecord.run_stats` exist; the ordered history comes from the audit timeline rather than the view state | Reachable at `?screen=d10`; read-only, gate strength comes from `AgentLaneRecord.gate_strength` rather than the agent label, an unbound Lane reports no project, a Lane with no Core task reports no progress, a cost-blind route (`AgentRoute::cost_meterability`) is marked and shows the bounded run facts Core recorded instead of an inferred cost — absent rather than zeroed when Core observed no run, and with an unknown exit code labelled as such — and the event ticker is one bounded newest-first page of Core's append-only audit timeline (`QueryAudit` -> `AuditPageLoaded`, limit 50, unscoped so it spans every project), rendering each record's stable id, raw dotted action key, owning project and Lane, and timestamp, with an absent `runtime.audit`, a pending read, a refusal, and an answered-but-empty timeline kept as four different lines |
| D12 integration gate | conflict banner, gate policy, bounce-to-origin-lane recovery timeline, the conflicting hunks, post-merge rollback, and no manual merge | `merge_gates`, `conflict_bounces`, `reverts`, `check_runs`, `AcceptMergeGate`, and `RejectMergeGate` exist, and `runtime.conflict_content` publishes `ConflictBounce.content` and `LaneConflictView.content` | Reachable at `?screen=d12`; `Accept and merge` and `Bounce to origin Lane` send their Core commands, each opening only when the rules `decide_merge_gate` enforces are met and naming its blocking code otherwise, the timeline and reverts are scoped to the selected gate, and each conflict record draws its rejected hunks as OURS beside THEIRS with the patch preimage as a third strip — never a merged result (`GUI-CORE-015` adopted) |
| D14 audit and timeline | who changed what, on which objects, with what outcome — plus a raw ordered event log for diagnosis | `RuntimeCommand::QueryAudit` -> `AuditPageLoaded` under `runtime.audit` exists, as does `CoreClient::replay` with `ReplayRequest`/`ReplayBatch` and `EventCursor`; `AuditPageLoaded` carries no command id and `AuditQuery` has no actor or time filter (`GUI-CORE-024`), and the view state carries no event log (`GUI-CORE-014`) | Reachable at `?screen=d14`, and from D2's decision detail and D12's revert rows scoped to the audit object Core linked. Two modes: **audit** (default) pages Core's append-only audit store newest-first with an acceptance-first correlation (a page counts only after Core accepted this exact `command_id`, a second concurrent read is refused locally, and rows come only from a confirming page), renders the dotted `action` key raw because it is Core's stable diff-able vocabulary, labels an actor or outcome this build cannot name as `unknown` rather than borrowing a known one, and prints each record's time as a fixed `YYYY-MM-DD HH:MM:SS UTC` clock in every locale, because an audit record is evidence compared across machines; **raw event replay (diagnostic)** keeps the replay-cursor log, where the row label is Core's own serde discriminant, an undecodable event still occupies a row, and a replay failure is shown instead of a shorter complete-looking trail. An absent `runtime.audit` opens D14 directly in raw mode, names the capability, and sends no audit command. Design filter chips, day grouping, the detail rail, rollup, and export are not shipped; the parts needing Core are `GUI-CORE-024` |
| D13 fleet and workflow | one board per workflow DAG with declared edges, node runtime status, blockers, and lane handoffs | `agent_dags` with `AgentDagTaskSpec`, `tasks`, `dependencies`, and `handoffs` exist | Reachable at `?screen=d13`; read-only, edges are the task specs' own dependency lists, a node reports status only when Core runs that task, a blocker appears only from a Core `DependencyState::Blocked` record, and a handoff is never derived from an edge |
| D6 recovery | connecting, disconnected, agent stopped, budget exhausted, gate queue clear, reconnect/restart/close actions | Runtime errors, CoreClient snapshot recovery, context budget facts, queue/gate facts, `RetryAgentSession`, and `StopLane` exist; no checkpoint is modelled at all | Task 10 renders operational Core-owned recovery states; the no-project `empty` state is handled by D1 Welcome Center; restart and close-Lane send their Core commands for the one unambiguous target Core published, inspect expands existing facts locally, and checkpoint remains visibly unavailable under `GUI-CORE-003` (`GUI-CORE-018`) |
| Locale and skin system | `en`/`zh-CN`, Aurora/Ice/Mono/Amber/Phosphor, dark/light constraints, density, motion | `RuntimeSnapshot.ui_preferences`, `SetUiPreferences`, `ResetUiPreferences`, and `UiPreferencesUpdated` exist with persistence and safe fallback diagnostics, advertised as `ui.preference_persistence` | The rail's Settings gear edits an unsaved draft and sends `SetUiPreferences`/`ResetUiPreferences`; rendered state changes only on the ordered `UiPreferencesUpdated`, and an absent capability opens the panel read-only |

Open requests are recorded in [contract-requests.md](contract-requests.md) and
[contract-requests.zh-CN.md](contract-requests.zh-CN.md). GUI must not close
those gaps with private reducers or direct runtime access.

The remaining open requests block only the production screens named in their
rows. They do not block the framework-neutral, fixture-only Tasks 2-3 or their
evidence; no spike result authorizes production mutation or persistence.

## D11 project intake

Task 7 implements the D11 subordinate project-configuration flow after the fixed Core
`0.3.2` integration checkpoint. `GuiCoreAdapter` gates project onboarding,
credential-handle intents on their advertised extension capabilities. Probe,
preview, and confirm remain distinct Core commands; D11 starter selection is a
local ordered review queue and never sends legacy `CreateLane`. A read-only
`d11_poll` command keeps receiving late facts after
the initial bounded wait, while the adapter serializes pending intake commands
and matches preview hashes, confirmation ids/hashes, Lane ids, and the active
approval request id before clearing pending state. Project-config approvals are
bound to the exact Core metadata token `sha256=<64 lowercase hex>` before the
GUI accepts their request id; if a bounded `preview_id=` token is present, it
must also equal the pending preview id. Hash substrings, non-lowercase hashes,
free text, and non-`sha256` fields cannot retarget or clear the pending command.
Allow decisions keep waiting
for the matching business fact; deny/expiry decisions clear pending and keep
draining the following Core error projection. Transient poll failures keep the
local draft and pending identity, then retry with bounded backoff. Intermediate
Core projection changes remain visible during that wait. Cancel clears only the
in-memory navigation state, performs no Core mutation, and returns to D1.
Welcome never enters this flow: folder selection and host rebinding complete first.

The shell reaches D11 at `?screen=d11` and from the project picker's
`Configure this project…` row beside the open project. It used to hang off the
New Lane popover's `Full setup…`; the `0.3.4` cockpit-centre batch moved that
action to the D4 Lane wizard it belongs to, because asking an operator to
configure the whole project in order to make one Lane is the wrong question.
D11 configures a *project* — probe, `viden.toml`, credentials, starter Lanes —
so the project surface is where it is entered from.
`d11_poll` is both the entry read and the wait, so re-entering resumes a command
still awaiting its Core receipt instead of restarting it, and the starter-Lane
seeds D11 collects are handed to D4, which owns the preview/confirm receipt loop.
There is no automatic redirect into D11: Core publishes no first-run intake fact,
so the client would have to invent one.

The standalone host drains ordered command events before refreshing its
authoritative snapshot, so acceptance cannot hide the later probe, preview, or
confirmation fact. A new-project draft includes the required `name` and `pack`
fields. When confirmation requires Core approval, D11 embeds the same typed
Permission Dock used by D1; `Allow once` or `Deny` remains an explicit Core
command, never a GUI-side bypass.

## D4 reviewed starter Lane creation

Task 8 opens D4 from the project cockpit's `New Lane` action and reviews one
seed at a time. Cancel or Skip before creation returns to D1 without a mutation;
after creation is sent, the screen exposes
the exact Core approval allow/deny actions and no fake cancel command. Each
complete `StarterLaneCreated.receipt` advances the queue, and the final receipt
emits a typed D1 navigation request focused on the last created Lane.

### The four steps

The wizard's steps are the design's own (`GUI/pages/Viden - D4 Lane创建流程
(GUI).html` `STEPS`), and each renders only its own fields rather than four
headings over one form:

| # | Step | What it renders | What Core does not model |
| --- | --- | --- | --- |
| 1 | Role & workstation | the three `StarterLanePreset` roles, the Lane name and branch, and Core's resolved worktree and base revision read-only | the design's seven-role domain pack (`D-ROLES`) is not on this contract, so the three Core names are the three offered |
| 2 | Choose agent | Core's published adapters, read-only, with the New Lane popover's pick marked, plus the resolved route | `StarterLaneRequest` carries **no agent binding**, so creating the Lane does not start the Agent session; the step says so rather than offering a selection that cannot reach Core — a contract request this batch drafted rather than numbered, because the register is frozen for `0.3.4` |
| 3 | Skill pack | the step, stated unavailable with the reason | `frontend-contract-v1` publishes no skill, pack, or context-injection fact anywhere, and the request has no field for one — the same drafted request |
| 4 | Gates & target | Core's own resolution: route, gate strength, target, budget, worktree, base revision, and mutation policy | the request carries no execution target and the preview always resolves `local`; remote targets are designed in D9 and are not on this contract, which the step names instead of drawing a picker |

The task the popover carried is shown on step 1 with the note that it is **not**
part of the create command: `StarterLaneRequest` carries the Lane id, preset,
branch and worktree only, so the task travels as the operator's first message
once the Lane is open — which is exactly what the compact New Lane creator
does (`create_starter_lane`, then `submit` / `start_agent_session`).

The adapter sends `PreviewStarterLane`, retains the exact original request,
and accepts only a same-owner `StarterLanePreviewed` fact. Branch, worktree,
base revision, route, gate, target, mutation policy, and budget are read-only
Core facts. Build mode may create only the unchanged reviewed request; Plan
mode may preview but cannot create. `CommandAccepted` and `LaneUpdated` remain
intermediate facts and never navigate. Request changes, rejection, approval
denial, and typed preview invalidation preserve the webview draft and require
a new preview. Only a full owner/id/hash/Lane/branch/worktree/base/config match
on `StarterLaneCreated` authorizes queue advancement.

Core `0.3.5` advertises the exact additive capabilities
`runtime.starter_lane_preview` and `runtime.cockpit_context_v1`, so the
production D4 path and D1 Context Dock can use the reviewed typed flow.
Connections to older or partial Core handshakes still show the gate and send
nothing; `runtime.lane_lifecycle` is deliberately not accepted as a substitute.

Deterministic browser evidence for rc.3 is retained under
`evidence/0.1.0-rc.3/`. The complete eight-pair theme matrix, all three
density values, both catalogs, and reduced-motion behavior remain automated
tests.

The config rail renders only Core's exact reviewed `viden.toml` contents, and
confirmation copies the preview id and SHA from the current Core projection.
Credential rows contain masked handles only. Because no frontend-safe platform
credential staging channel exists, raw credential entry and the webview
`StoreCredentialHandle` path are disabled with `GUI-CORE-026`. D11's history
panel renders the same Core `QueryRecentWork` inventory as the Welcome centre
and the project picker described below, gated on the `runtime.recent_work`
capability (the old `GUI-CORE-007` stopgap is retired); none of these surfaces
scan local storage, JSONL, or SQLite. Project switching now uses the Core-owned
`LocalCoreHost::open_workspace` boundary; secure raw credential staging remains
the outstanding `GUI-CORE-026` part.

## D1 streaming cockpit

Task 9 makes D1 the persistent application shell and canonical work surface.
It renders first with D6 connecting/disconnected state or the host-owned
no-project welcome. A bound project with no Lanes remains D1, while D4 is the
project-only creation workflow and D11 is explicit configuration. The activity
rail, Lane rail, Environment, Live Work,
transcript/tool rows, queue state, evidence, and composer are transport-safe
projections of Core's latest `RuntimeViewState`.
The webview owns only focus, draft, layout, bounded-row, and scroll-anchor
state. It does not parse display strings, persist a second workspace model, or
claim command acceptance as business success. Ordered Core refreshes update the
activity and Lane rails in place so their hover roots remain mounted while
volatile Lane/Agent status changes; this prevents the floating sidebar from
flashing without hiding fresh Core facts. Every enabled activity-rail slot has
an action behind it, and a slot with no available action is disabled rather
than enabled and inert. The rail itself is the cockpit's router; see
[Navigation shell](#navigation-shell) below for the table, the centre views,
the return path, and the Lane sidebar's two modes.

### Centre pane

The centre pane is the design's `.center` column: the `.tabstrip.lanebar` Lane
tab strip over the scrolling transcript. The strip carries **one tab per Lane
the cockpit projection lists** — status dot, Lane id, Lane name, and the Lane's
own recorded branch — plus the agent Core bound to it, a trailing `＋` that
opens the New Lane popover, and the design's `.tabmeta` slot with the project,
the context budget Core published, and the resolved work mode.

Three absences are deliberate. A Lane that recorded no branch shows none: the
workspace's branch is a different fact and never stands in for a Lane's (C5's
`lane_sources` is the seam that will carry a per-Lane worktree source). The
budget and the mode live in the trailing meta rather than on each tab, because
the cockpit projection is Lane-scoped — `contextDock.context` and
`statusbar.workMode` describe the Lane the read was made for, and printing
either against another Lane would be a number with the wrong name on it. A Lane
with no Agent session is named for the built-in runtime, exactly as the Lane
rail names it.

With no Lanes at all the strip stays, carrying the rail's own "No Lanes yet"
sentence and the `＋`. A centre *view* — DiffReview, EvidenceView, or one of the
five D-screens — replaces the whole column, which is the flagship's own switch,
so those views draw no strip and state their own scope in their head instead.

Clicking a tab selects the Lane through `selectLane`, the same path the Lane
rail and the palette use. `⌃⇥` / `⌃⇧⇥` cycle the projected Lanes, which is the
pair the design's keyboard registry binds to "Next / previous lane"; they stand
down while an IME composition or any overlay owns the keyboard, because moving
the conversation out from under a decision the operator is being asked to make
is the one thing the chord must never do.

**Tool blocks.** A workspace change and a check run render as the design's
`.tool` block — a `.th` header over a `.tb` body. The change's body is the
shared hunk renderer (`diff_rows`), the same rows DiffReview, the permission
dock and D2 draw, and it is **collapsed by default**: a transcript is read top
to bottom and a long hunk in the middle of it buries the conversation. A check
run is not collapsed; its body is the design's three `.testrow`s — status, the
failing location when Core reported one, and the result — and the failing line
is why the block is on screen.

The header names what Core named. Core publishes no producing tool on a
workspace change, so the name slot carries the change kind — unless the pending
approval's own `decisionContext` diff covers exactly this path, which is the
one case where the design's `write_file` / `edit_file` is a Core fact rather
than a guess (ordered typed tool-call rows remain `GUI-CORE-009`). The gate
chip appears on that same evidence and nowhere else: an approval Core attached
no context to gates nothing here. A change with no rows keeps the patch-or-
unavailable body it has always had.

**Focus mode (`⌘.` / `⌃.`).** `D-SIDEBAR`'s override: "focus 专注模式覆盖此偏好
→ 两侧强制 hover 浮窗(退出恢复)". Both side panels move behind their own
registered `.edgewrap` hot zones — the Lane sidebar's on the left, the context
dock's on the right, each with the same ~700 ms peek delay — and the transcript
takes the width. The activity rail stays, because it is the cockpit's router
and a router that disappears is a dead end.

The flag *shadows* `laneSidebarMode` rather than writing it, which is what
makes "退出恢复" free: leaving restores the pinned column the operator chose
with no saved copy. It is in-memory state like the sidebar mode and the
statusbar's ambient set, but unlike those two it is not a preference — it is a
posture held for a few minutes — so it is deliberately **not** part of C5's
`UiLayoutPreferences`. The titlebar carries the design's `IFocus` control for
it, in the `.tbtools` position the design draws, with `aria-pressed` reporting
the state.

`New Lane` opens one compact, anchored popover with the built-in Viden Agent
selected by default, discovered ACP Agents, the task draft, branded Agent
identity, Core-projected eligibility/probe diagnostics, and a
presentation-only isolation hint. `Full setup…` opens the D4 Lane wizard on
the popover's own draft — the task names the Lane and its branch with the same
`vd/<slug>` the popover previews, and the chosen agent is marked on the
wizard's agent step. Cancelling the wizard returns to the cockpit with the
popover reopened on exactly what the operator had typed, so a detour through
the full form never costs them the draft. Git
workspaces preview the derived branch/worktree; non-Git directories explicitly
state that the Lane runs in the opened workspace without creating either. Agent
selection stays inside the popover, the task textarea receives focus, and Create
Lane is disabled until the task is non-empty. Core or Agent discovery redraws
are deferred while that textarea owns an IME composition, so macOS candidate
input is not detached mid-composition. Create dispatches the
existing ordered path: `preview_default_lane`, `create_starter_lane`, then
native `submit` or ACP `start_agent_session` only after the exact Core Lane is
projected. Transport or Core rejection preserves the draft and uses the typed
D1 rejection surface. ACP discovery runs automatically once per cockpit
lifetime; reopening the popover reuses that result. A failed discovery exits
the busy state, displays the exact diagnostic in the popover, and retries only
after the operator chooses `Retry ACP check`. ACP startup rejection remains
visible on the typed D1 rejection surface after its Lane is created.

The composer remains editable while an assistant stream, tool, task, approval,
or queued input is active. Enter sends `QueueFollowUp` in that state and
`SubmitUserInput` when idle; Shift+Enter preserves multiline input and CJK IME
composition never submits early. Streaming Core redraws likewise keep the
focused textarea mounted until `compositionend`. Both commands use an exact
Core-published Lane owner from a live owner binding or the D4 receipt. Cancel is stricter: it
is visible and transport-enabled only when the selected Lane is active,
`runtime.lane_owner_projection` is advertised, and exactly one matching
`lane_runtime_owners` binding supplies the complete owner. Missing, mismatched,
or ambiguous owners fail closed with zero sends.
If another ordered Core command or Agent probe still owns the client command
slot, one composer submission waits visibly as `Queue follow-up`, disables a
duplicate Send, and dispatches as soon as that slot is released; input is never
silently dropped.
After a desktop restart, the sole terminal ACP session restored by Core may
accept a follow-up through its exact durable session owner even though the
process-local Lane binding has expired. Before the continuation starts, Core
publishes a fresh `LaneRuntimeOwnerBound` fact for that same durable owner, so
the Lane remains visible as busy while the ACP response is in flight instead
of temporarily falling into Agent Stopped. Duplicate sessions, owner mismatch,
and non-ACP restoration still fail closed.
Within one app/Core lifetime, completed ACP turns keep their healthy process and
remote session resident. The next Send therefore goes straight to
`session/prompt`; a dead or incompatible resident connection falls back to the
persisted `session/load` path before prompt delivery. The GUI does not own this
cache and continues to render only ordered Core facts. Immediate Starting/busy
feedback measures Core dispatch, while time to first assistant content also
includes agent startup (for cold turns), context work, and model inference.
Core caps the resident pool at eight sessions and expires connections after 15
idle minutes; a later Send transparently uses the persisted reload path. Core
shutdown also retires the workspace's entire resident pool.
Cancelling an active built-in model turn now stops that turn before considering
Lane lifecycle cancellation, so the Lane and its exact owner remain routable.
A legacy terminal native Lane without an owner renders a disabled composer and
Send action instead of a clickable no-op.

The composer meta row carries three popover selectors: work mode (Plan, Build,
Review, Explore), permission level (Ask, Auto Edit, Auto, Read Only, Full
Access), and model (grouped by the active provider and every adapter model
list Core published; the current pair is highlighted, and nothing is invented
when Core published no options). Selecting an option dispatches
`SetWorkMode`, `SetPermissionLevel`, or `SelectModel` through the host
commands `set_work_mode`, `set_permission_level`, and `select_model`, which
share the ordered D1 pending pipeline and resolve with the refreshed
projection. The selectors never apply Core's mode/permission coupling rule
locally: both pills re-render from the snapshot Core republished, so choosing
Plan visibly flips the permission pill to Read Only exactly when Core says so.
While a control call is in flight the row is `aria-busy` and the pills are
disabled; they are also disabled while the composer is not editable or no
workspace is open. A Core rejection or transport failure renders on the typed
D1 rejection surface (`role=alert`). Popovers follow the agent-menu
conventions: Escape closes and returns focus to the pill, an outside click
closes, and arrow keys move option focus.

The activity rail closes with the prototype's Settings gear below the spacer.
It opens the Settings overlay for provider/model, permissions, language, skin,
mode, density, and motion, built from the registered design component
`GUI/gui-settings.jsx` with shared tokens only.

The overlay's first two sections are a **second surface onto the composer
pills, not a second model**. The Permissions section renders the permission
levels from the same `PERMISSION_LEVELS` enumeration the pill uses — each row
carrying the UI label, the Core CLI identifier verbatim, and a one-line
description — reads the current level from the same `statusbar.permissionLevel`
fact, and dispatches the same `SetPermissionLevel` intent. The Provider &
Models section lists exactly what `modelGroups()` returns for the pill (the
active provider group plus each adapter group Core published), marks Core's
current selection, and dispatches the same `SelectModel` intent. Both are
disabled — never hidden — while the composer is not editable, no workspace is
open, or a command holds Core's one-command-at-a-time D1 slot; that gating is
independent of `ui.preference_persistence`, which governs only the draft-and-
save half below. A read-only working-directory row states the workspace root
Core opened.

Three elements the design draws in those sections are deliberately absent
rather than faked: the add-provider action and per-provider API-key chips
(credentials remain GUI-CORE-026 territory), the Requests card
(`request_timeout_secs`, `max_retries`, `provider_plugin_dirs` have no Core
command in `frontend-contract-v1`), and the permission Rules preview box plus
the editable additional-working-directories field (Core publishes neither a
rule table nor a scope command). Each absence carries a code comment naming
the design element it corresponds to. An undrawn element is not a lie; a drawn
control that cannot reach Core would be.

The language and appearance controls keep their existing contract. Every one
of them edits an unsaved GUI-local draft: Save stays
disabled until an axis is drafted, and only the axes the operator actually
selected enter the patch, so an untouched axis keeps whatever Core resolves.
Save sends `SetUiPreferences` and Restore defaults sends `ResetUiPreferences`
through the host commands `preferences_save`, `preferences_restore`, and
`preferences_poll`.

Confirmation is the ordered `UiPreferencesUpdated` fact and nothing else: a
republished snapshot is not a persistence receipt, a save confirms only when
the persisted `[ui]` table carries every value the patch asked for, and a
restore confirms when that table is gone while the resolved fallback still
renders. On confirmation the panel adopts Core's resolution — including axes
Core coupled in that the click never asked for — and applies it to the live
theme and document language. The skin/mode pair is not pre-validated in the
client, so an unsupported pair such as Amber with Light stays selectable and
comes back as Core's own rejection in a `role=alert` line beside Core's
diagnostics; a Plan/Review/Explore denial arrives the same way, with the
config bytes unchanged. While a command is in flight the panel is `aria-busy`
and every control is disabled. When Core's handshake does not advertise
`ui.preference_persistence` the gear still opens the panel, read-only, naming
that capability — never a hidden entry and never an enabled-and-inert control.
The overlay follows the agent-menu conventions: Escape closes and returns
focus to the gear, an outside click closes, and arrow keys move option focus.

The cockpit titlebar carries the workspace's source-control facts, computed by
the host from Core's `workspace_source` sample and projected as `topbarSource`.
The project selector shows the project name Core published — or the workspace
path when Core named none, never a name derived from the path — followed by
`⎇ <branch>` and a dirty marker when the sample reports uncommitted changes.
The `.gitops` block beside it holds two chips: `↑ahead ↓behind`, which is an
operator control since `runtime.operator_git` (see the DiffReview section for
its push/fetch rule, and note it stays a `role=status` readout on a host that
carries no action port), and `⎇ N worktrees`, which opens the D10 Lane monitor
and is disabled when no navigation handler is injected. `N` counts the distinct worktrees of the
project's active Lanes: Core publishes no git worktree inventory, and two Lanes
sharing a worktree are one worktree. When Core publishes no workspace source,
or reports it unavailable, the whole block is omitted rather than rendering
zeroes that would read as a clean, in-sync tree; a truncated sample keeps its
published counts behind a truncation marker. The design's `▾` project picker is
deliberately absent until the multi-project rail exists — the GUI ships no
enabled-and-inert control.

The cockpit statusbar renders the host-computed statusbar projection as
terminal-vocabulary segments: `MODE`, `PERM`, `CONTEXT` (the most recent
workspace budget), `EVENTS` (the replay-cursor stream position, titled as a
position because frontend-contract-v1 publishes no event counter), `LANE`
(selected Lane, its sole bound agent, status, and task progress), `LATENCY`,
`TOKENS` (input up, output down), `DIAG` (runtime error count), and `REQ`
(provider request/error counts). A segment whose Core fact is absent renders
an explicit em-dash rather than a fabricated number. When approvals or open
merge gates are waiting, the right edge shows the pending-gate segment — the
bar's only actionable element — which opens the in-cockpit D2 decision queue. A
gear at the leading edge opens the `D-STATUSBAR` config popover over the six
ambient segments; see [Navigation shell](#navigation-shell).

The transcript retains at most 240 rows. Leaving the latest edge sets
`follow_latest=false`, preserves the current anchor, and increments a visible
new-output count instead of forcing a scroll. Rust and webview tests cover
10,000-event bursts, 50,000 rows, resize/idle reads, CJK composition,
multiline paste/undo, keyboard traversal, ARIA regions, and visible focus.
Browser-controlled rc.3 evidence under `evidence/0.1.0-rc.3/` includes
Aurora dark/regular English, Ice light/regular English, Aurora dark/regular
Chinese, compact density, responsive drawer states, and an independent
same-state design reference populated from `d1-main-cockpit.json`. It also
includes a supplemental Context Dock bottom-state capture that proves lower
facts are reachable by internal scrolling. Checkpoint capture and restore
(rendered as `GUI-CORE-003`, contract request `GUI-CORE-018`) remains an
explicit unavailable fact; D1 never fabricates a successful placeholder. The
`audit`, `diff`, and `apply` rows are gone: Core's audit timeline,
`runtime.structured_diff`, and `runtime.operator_git` closed those gaps, and a
row claiming otherwise would be a stale statement about Core rather than a
fact. `diff` and `apply` still appear against a Core build that really
publishes neither capability, and then name the capability itself.

## Navigation shell

The activity rail is the cockpit's router (`D-RAILNAV`), and every destination
it names renders **inside** the cockpit chrome. The `0.3.4` plan resolved the
question `D-RAILNAV` left open in favour of the D1 flagship's own behaviour:
D2, D10, D12, D13, and D14 were full-window screens that discarded the
titlebar, both rails, the context dock, the composer, and the statusbar, and
left no way back except a browser-style route; they are now views over the
transcript, exactly as DiffReview and EvidenceView already were.

**One router table.** `src/components/activity_rail.ts` exports
`D1_RAIL_ROUTES`, a single ordered array that is both the rail's render order
and its routing map. It replaced a slot list plus a separate route map keyed by
message id, which could drift apart; one table makes "every registered
destination is reachable from the rail" checkable rather than asserted.

| # | Slot | Opens | Glyph | Availability |
| --- | --- | --- | --- | --- |
| 1 | Conversation | the transcript; focuses the composer | `chat` | a focusable composer |
| 2 | Lanes | toggles the Lane sidebar (see below) | `lanes` | a bound workspace |
| 3 | Diff review | centre view `review` | `review` | `runtime.structured_diff` |
| 4 | Evidence | centre view `evidence` | `evidence` | `runtime.evidence_reads` |
| 5 | Decisions | centre view `d2` | `decide` | a bound host |
| 6 | Lane monitor | centre view `d10` | `diagnostics` | a bound host |
| 7 | Integration gate | centre view `d12` | `worktree` | a bound host |
| 8 | Fleet | centre view `d13` | `fleet` | a bound host |
| 9 | Audit | centre view `d14` | `brief` | a bound host |
| — | Settings | the Settings overlay, below the spacer | `settings` | a bound host |

Every glyph is a registered `GUI/gui-icons.jsx` path; no slot invents art.
Exactly one slot is marked `aria-current="page"`, and the table decides which:
the Conversation slot is current precisely when no destination is, so the rail
can never mark two at once.

**Absence is named, never hidden.** A destination whose Core capability is
missing is disabled *and* labelled with the capability, in the accessible name
rather than only in a tooltip — `Diff review — unavailable: Core publishes no
runtime.structured_diff`. Audit is the one destination that is never blocked:
without `runtime.audit` D14 opens in raw event replay, which is a different
view of the same question, so the slot says so up front instead of letting the
operator discover it after the click. The only badge is the Decisions slot's
pending count, which is `statusbar.pendingGateCount` — a number Core already
publishes. No other slot carries one, because no other slot has a Core-published
count, and a zero would read as "nothing waiting" when the truth is "nobody
counted".

**Centre views.** `centerView` is `transcript | review | evidence | d2 | d10 |
d12 | d13 | d14`, and the cockpit publishes it as `data-center-view` on its own
frame; `root.dataset.route` stays `d1`, because the centre view is a fact about
the cockpit rather than a route. The five D-screens are mounted through one
seam, `D1RenderOptions.secondaryViews`: the shell owns *what* goes in (each
screen is a Core read that belongs to the client boundary) and the cockpit owns
*where* — the centre pane, the chrome around it, the Close control, and `Esc`.
The screen renderers are untouched; they take a container and fill it, and only
the container moved from the window root into D1's centre pane. That is what
keeps their existing Rust projections, `onNavigate` semantics, and vitest
coverage passing. The host node is kept across ordered Core refreshes, so a
mounted screen holds its own selection, filter, and mode instead of being
rebuilt — and re-read from Core — on every wake. A rejected read renders Core's
own words in a `role=alert` in place of the screen; the pane is never left
blank, because blank reads as "nothing here" rather than "Core would not
answer".

**Deep links.** `?screen=d2|d10|d12|d13|d14` opens the cockpit and then
switches its centre pane, so a link produces the same chrome, the same selected
Lane, and the same return path the rail does. `?screen=d4` and `?screen=d11`
stay full-window flows: they really do replace the cockpit. Welcome still wins
when no workspace is bound — the deep link is only read once Core answered with
a cockpit projection.

**Every cross-view link lands in the cockpit.** The palette's `#` rows (a merge
gate to D12, an ask to D2), the statusbar's pending-gate segment, the
titlebar's worktrees chip, D2's and D12's audit-trail links, EvidenceView's
"Open audit trail" footer, and D10's card action to the decision centre all go
through one function — the cockpit's `navigate` — which switches the centre
view when it can host the destination and falls through to the shell's window
route when it cannot. The scope each of them carries is preserved: D14's
`kind:id` audit scope still goes through `parseAuditScope`, and D12's gate id
and D2's decision id are still Core's own ids, re-read by the screen before it
renders.

**Return path.** Every non-transcript view carries a Close control in the
position DiffReview puts its own — the trailing end of the view's head — and
pressing the same rail slot again returns to the transcript. `⌘G` / `⌃G` opens
Decisions and toggles the same way, joining `⌘R` (Diff review) and `⌘E`
(Evidence). `Esc` is handled in exactly one place, with an explicit priority
order, because the order *is* the contract:

1. an IME composition owns the key outright;
2. an open overlay owns it — the settings panel, the command palette, a
   composer-control popover, the New Lane popover, the project picker, or the
   permission dock. A decision the operator is being asked to make outranks any
   navigation;
3. the floating Lane sidebar's peek, the most transient thing on screen and the
   one `D-SIDEBAR` binds `Esc` to;
4. the composer's cancel-turn binding, which stops real work rather than moving
   a view. Its affordance — the Live Work strip's Cancel — lives inside the
   transcript, so in practice the two never contend;
5. the centre view's return path, and only then.

**Lane sidebar modes (`D-SIDEBAR`).** `floating` is the decision's default and
the cockpit's: the sidebar gives its horizontal space to the transcript and
lives behind the design's 12 px hot zone with its `.edgehint` cue against the
activity rail. A pointer entering the strip peeks it open; leaving hides it
after the decision's ~700 ms delay. Selecting a Lane hides it immediately — it
has done its job — and so does `Esc`. The keyboard path is the Lanes rail slot,
which toggles the peek, because a 12 px strip is not a pointer target everyone
can hit.

`pinned` is a **real layout column**, not the same overlay with a
flag: the cockpit body grows a fourth grid track at the design's default width,
`--rail-left` (218 px), and the rail stops being absolutely positioned. In that
mode the Lanes slot toggles the column itself, which is the only way to give
that width back without leaving the mode, and `Esc` leaves it alone — `Esc`
belongs to the transient peek. At 1100 px or below the shell already collapses
to two tracks, so pinned falls back to the overlay there rather than taking
width the transcript does not have.

The single toggle entry is the pin button at the bottom of the activity rail,
directly above the Settings gear, exactly where `D-SIDEBAR` puts it; its
`aria-pressed` reports the current mode rather than the verb it performs. There
is deliberately no second entry: the sidebar-header `.pinbtn` the design once
drew was never rendered and its CSS was deleted on 2026-07-02. The rail
component is the same node in both modes — only its host changes, which is the
decision's own rule.

The token's documented 176–360 px drag is **not** implemented; the pinned
column is fixed at 218 px for this batch.

**Statusbar config gear (`D-STATUSBAR`).** A gear at the leading edge of the bar
opens the design's `.sbcfg` popover listing the six *ambient* segments —
`CONTEXT`, `EVENTS`, `LATENCY`, `TOKENS`, `DIAG`, `REQ` — with a checkbox each.
The pinned half is not listed and cannot be switched off: `MODE`, `PERM`,
`LANE`, and the pending-gate chip are identity and action, and `D-STATUSBAR`
splits the bar by actionability rather than urgency. A bar mounted without the
config port renders no gear at all, rather than a gear that does nothing.

**Two in-memory seams.** The Lane sidebar mode and the statusbar's ambient
visibility are presentation state held in memory for this batch and
deliberately **not** persisted. The frontend contract makes Core the single
preference authority, so the design prototype's `localStorage` keys
(`vd-leftmode`, `vd-leftw`) would be the second preference model the contract
forbids. Core batch `C5` adds `UiLayoutPreferences.lane_sidebar_mode`; G7 then
reads it through the resolved preference projection and writes it back, and
`D1RenderOptions.laneSidebarMode` becomes Core's value rather than a caller
default. The pinned column's width — the token's 176–360 drag — is the same
record's second field. Both seams carry that note in the code.

## Projects, recent work, and the grouped rail

Core supervises **one** workspace at a time.
`LocalCoreHost::open_workspace` builds a new `RuntimeSupervisor` per call and
the desktop host swaps its single adapter slot, so a successful open *replaces*
the current workspace: dropping the previous supervisor joins its worker and
shuts down every resident ACP session. Everything below follows from that fact.

**Recent work** is a Core read, not a client scan. `queryRecentWork` sends
`QueryRecentWork` and treats only the ordered `RecentWorkLoaded` fact as the
answer — the `CommandAccepted` that precedes it is matched by command id *and*
variant, a republished snapshot never confirms it, and an inventory answer that
arrives before this command's acceptance belongs to another reader. Core owns
the scan of the shared session home, the `1..=100` clamp, the whitelist DTOs,
and the ordering; the GUI re-serializes what arrives without re-sorting it and
renders Core's diagnostics verbatim. Four states stay distinct rather than
collapsing into one empty list: an absent `runtime.recent_work` capability, a
Core rejection, a read Core accepted but has not answered, and a genuinely
empty inventory.

**Welcome** lists those projects with a relative age and the session count from
the same bounded fact. Welcome renders only when no workspace is bound, so a
recent row opens directly — there is nothing to replace.

**The project picker** opens from the titlebar `.projsel` (which gains the
design's `▾` and button semantics only where the picker can actually open) and
from the rail's `＋ Add project…` footer. It draws the design's three columns:

| Column | Contents |
| --- | --- |
| Add | `Add directory…` runs the native chooser, then the switch confirmation. `Clone repo…` and `New empty project` are visible and **disabled**, naming `GUI-CORE-023` |
| In workspace | exactly one row — the open project, marked current and non-actionable, with its Lane count |
| Recent | Core's recent projects minus the open root; each opens the switch confirmation |

**Every switch is confirmed inline** — never through a browser `confirm()`.
The step names the target root, states that Viden supervises one workspace so
this replaces the current one (`GUI-CORE-023`), and counts the running Lanes
and Agent sessions the replacement tears down. An idle workspace still
confirms, with a milder sentence: nothing is interrupted, but the session is
still closed and rebuilt. Escape inside the confirmation backs out to the
columns rather than also dismissing the popover; Escape at the columns closes
it and hands focus back to whichever anchor opened it, resolving the *live*
anchor because a Core refresh rebuilds both the titlebar and the rail.

**The Lane rail** has two `D-SIDEBAR` modes (see
[Navigation shell](#navigation-shell)) and is, in both, the design's workspace
explorer: one `.wsroot` group
header carrying the project name Core published (or the workspace path when it
published none), a `▸`/`▾` collapse whose state is GUI-local and survives
ordered Core refreshes, the per-group `＋` — the same Lane creation action, now
with an accessible name behind the design's bare glyph — and the Lanes nested
beneath it. The design draws several project groups and a cross-project
"Global" section; both are mock data, so the rail renders exactly **one** group
with no fabricated siblings. That is the visible half of `GUI-CORE-023`.

## Command palette

The titlebar's palette button and **⌘K** (⌃K off macOS) open the cockpit
command palette drawn in `Viden - 桌面驾驶舱 (GUI).html` (`scrim top` /
`palette` / `palin` / `palsec` / `palrow`). **⌃P** opens the same overlay
pre-scoped to `>`, matching the composer caption the design draws
(`⌘K palette · ⌃P commands`).

The query grammar and the fuzzy scorer are a deliberate port of the TUI jump
index (`apps/tui/src/tui/jump.rs`), so the selector language is one language
across frontends:

| Sigil | Scope |
| --- | --- |
| `:` | lanes |
| `@` | Agent sessions |
| `#` | merge gates and asks |
| `>` | commands (the Actions and Settings sections) |
| `~` | files |
| _none_ | every kind |

An unsigilled query matches by subsequence over each row's title, context, and
keywords using the same position-plus-adjacency score the TUI computes. Rows are
not re-ranked under the cursor: the design's section order (Actions, Jump to,
Settings, Files) is preserved, exactly as the TUI preserves its group order.

The Actions section carries the design's own Lane-creation row, "Delegate task
to new worktree… ⌘L" — creating a Lane *is* delegating a task to a fresh
worktree, which is why the design names it that way. It opens the same New Lane
popover the rail's and the tab strip's `＋` open, so there is one creation
surface rather than two. When Core says the workspace cannot carry a Lane the
row is disabled and carries Core's own diagnostic; a Core that published no
workspace eligibility at all gets its own sentence, because that is not a
refusal and must not be worded as one.

Selecting a Lane or an Agent session selects it **in the cockpit** through the
same path the Lane rail uses, then focuses the composer; the palette never
navigates away for something the current screen already owns. A merge gate opens
D12 and an ask opens D2, each carrying the exact Core id, and the target screen
still re-reads its own Core projection before it renders.

Cross-Lane gates and asks are not in the Lane-scoped D1 projection, so the shell
reads `d2_decisions` and `d12_integration_gate` — projections that already exist
— when the palette opens. The read is eager, so `#` is answerable the moment it
is typed, and fail-soft: a rejection degrades that one section to a note
carrying Core's own words while lanes, sessions, and actions keep working. The
`Files` section lists the workspace inventory Core published
(`QueryWorkspaceFiles` -> `WorkspaceFilesLoaded` under
`runtime.workspace_files`, GUI-CORE-022), read the same eager fail-soft way and
rendered in Core's own lexicographic order. The client walks nothing. A Core
without the capability keeps the permanently disabled row naming the request
and sends no command; a read still in flight, a refusal carrying Core's own
sentence, and an answered read over an empty workspace each get their own row,
so an empty list never stands in for any of them. The TUI's jump index mirrors
all four cases.

### Chords

Every window-level chord the cockpit binds, in one place. `⌘` is `⌃` off
macOS, except where the table says otherwise. Each stands down while an IME
composition owns the key, and all but `⌘K` / `⌃P` stand down while a modal
overlay owns focus.

| Chord | Opens / does | Stands down when |
| --- | --- | --- |
| `⌘K` | the command palette (toggles) | a settings panel, New Lane popover, or control popover has focus |
| `⌃P` | the palette pre-scoped to `>` | same |
| `⌘L` | the New Lane popover | Core says the workspace cannot carry a Lane — the same condition the palette row fails closed on, with Core's own sentence |
| `⌘R` | DiffReview (toggles) | no host is bound, or Core published no `runtime.structured_diff` |
| `⌘E` | EvidenceView (toggles) | no host is bound, or Core published no `runtime.evidence_reads` |
| `⌘F` | focuses the evidence search box | EvidenceView does not own the centre pane |
| `⌘G` | the Decisions queue (toggles) | no router and no secondary host is bound |
| `⌘.` | focus mode (toggles) | an overlay owns focus |
| `⌃⇥` / `⌃⇧⇥` | next / previous Lane | an overlay owns focus, or there is nothing to switch to |
| `⌘O` | the folder picker | only bound on the no-project Welcome |
| `Esc` | the priority list below | — |

`Esc` is handled in exactly one place, and the order *is* the contract:

1. an IME composition owns the key outright;
2. an open overlay owns it — the settings panel, the command palette, a
   composer-control popover, the New Lane popover, the project picker, or the
   permission dock. A decision the operator is being asked to make outranks any
   navigation;
3. the floating Lane sidebar's peek, the most transient thing on screen and the
   one `D-SIDEBAR` binds `Esc` to;
4. the composer's cancel-turn binding, which stops real work rather than moving
   a view;
5. the centre view's return path;
6. focus mode, last of all — it hides no decision and interrupts no work, so
   every surface above owns the key first.

### Keybinding divergence from the TUI

The GUI follows its own design here, and it is the inverse of the terminal
client:

| Chord | GUI | TUI (`apps/tui/src/tui/keymap.rs`) |
| --- | --- | --- |
| ⌘K / Ctrl+K | open the palette | command palette |
| Ctrl+P | open the palette scoped to `>` | jump index |

This is deliberate, not drift. ⌘K is the desktop convention the cockpit design
commits to in its titlebar tooltip and composer caption, and the GUI palette is
a single surface that already contains both halves the TUI splits across two
chords — so binding ⌃P to the command scope of that one surface keeps the
design's caption honest while preserving the muscle memory of "⌃P means
commands". The two clients' *query* grammars remain identical, which is the
parity that matters when an operator moves between them.

Escape inside the palette closes it and stops there. The cockpit binds a
window-level Escape to "cancel the running turn", so the overlay consumes its
own dismissal rather than cancelling Core work on the way out. The overlay is
`role="dialog"` / `aria-modal`, the input is a labelled `combobox` owning a
`listbox`, the highlighted row is its `aria-activedescendant`, and focus returns
to the titlebar toggle — re-resolved on close, because a Core refresh may have
rebuilt the titlebar while the palette was open.

## Permission dock and D6 recovery

Task 10 places the canonical `.gperm.dock` immediately above the D1 composer.
It renders the exact Core approval risk, target, allowed scopes, reason, input
preview, expiry, default action, and audit id. Once, Session, repository
allowlist, and Deny map only to `RespondToApproval`; Always and Edit remain
disabled as `GUI-CORE-003` (contract request `GUI-CORE-019`), which also keeps
the design's `Shift+A` chord on `repo_allowlist` rather than on a dead action. Plan-mode mutation responses fail closed before
transport. Command acceptance is not success: pending clears only after the
matching ordered `ApprovalResolved` owner/request/audit fact.

Deny additionally redirects the composer, following the design's own post-deny
state: the prompt becomes "Tell <agent> what to do instead…" and takes the
caret, so the operator corrects the agent where they were already looking. The
agent is named from Core's published adapter `displayName` when the focused
conversation is an ACP session, and generically otherwise — never from a name
guessed out of an agent id. This is presentation and nothing more. It is
applied when the deny is *dispatched* rather than when Core answers, because it
claims nothing about the answer, and `feedback` stays `null`: schema 1 carries
no feedback field (`GUI-CORE-019`), so the correction travels as the operator's
next ordinary message rather than as a fabricated field. The redirect retires
on the next submit or when Core publishes a *different* request id; a refresh
still carrying the ask that was just denied leaves it alone, because that is
the ordinary state between the command and Core's answer.

D6 is a subordinate central work surface inside D1, never a second cockpit
shell. Empty, connection, provider, stopped-agent, context-overflow,
capability, incompatible-schema, queue-clear, and event-gap states come only
from Core projection or CoreClient errors. Event-gap reconnect uses the
CoreClient snapshot path and remains busy until a validated live snapshot is
published. Restart sends `RetryAgentSession` for the one Lane-bound ACP session
Core reports as failed or cancelled, and close-Lane sends `StopLane` for the
one active Lane Core published; both fail closed when the target is ambiguous,
because D6 carries no Lane selection. Inspect is a local toggle over the facts
already in the projection and reaches no Core command. Checkpoint stays visible
but disabled under `GUI-CORE-003` (contract request `GUI-CORE-018`); the GUI
never fabricates recovery receipts.

## DiffReview

`⌘R` (`⌃R` off macOS), the titlebar's changes marker, and the palette's
`Open review` all open the registered `.review > .filetree + .diffpane` family
in D1's centre pane. It is a **view inside the cockpit, not a route**: per the
design decision `D-RAILNAV` the activity rail stays a router to the standalone
D-screens, DiffReview is one of the secondary surfaces that adjudication
registered, and closing it returns to the transcript rather than navigating.
The CSS comes from `GUI/gui-kit.css`, where the `D0` promotion mirrored the
family for exactly this second consumer; hand-copying D1's inline rules is
drift by the package's own rule.

**Data path.** One `QueryWorkspaceDiff` -> `WorkspaceDiffLoaded` through the
CoreClient seam, correlated the way `QueryWorkspaceFiles` is: one read in
flight, the exact `command_id` on both the page and a `CommandRejected`, and no
acceptance-gated fallback, because the page's id is a required field. The
default query is the whole target, `scope: Both`, no path filter, and Core's
own byte bound. The target follows the cockpit's Lane selection — a selected
Lane reviews that Lane's worktree — and Core resolves the worktree from the
Lane id, because a client never passes a path. Core owns the permission gate
(the non-mutating `git_diff` tool, so the read stays answerable in Plan mode),
the `git status`/`git diff` runs, the bound, and the ordering. The client runs
no git and parses no diff text.

**Re-query rule.** Refresh re-reads on demand. Beyond that, while the view is
open every ordered Core wake reads the host's no-traffic projection; the
adapter counts the two facts that invalidate a diff — `WorkspaceSourceUpdated`
and `WorkspaceChangeUpdated` — inside its single receive funnel, so a fact
drained by an unrelated screen's poll still reaches the open review. A page
invalidated since it was read shows its banner immediately and re-reads after
a 400 ms debounce, so a burst of agent writes costs one bounded query rather
than one per event. The rows stay on screen throughout: "re-read due" must
never blank the only facts the operator has. A closed review checks nothing.
The revision is captured when the command leaves rather than when the page
lands, which fails safe towards one extra read instead of a silently outdated
review pane.

**Honesty rules**, each with its own sentence and its own test:

| Core fact | What the view renders |
| --- | --- |
| `DiffFile.omitted` | "Rows not shown (n additions, m deletions) — over the byte bound", with the real counts |
| `DiffFile.binary` | "Binary, no rows" |
| `WorkspaceDiffEntry.diff: None` | "Core produced no diff for this file. This does not mean it is unchanged." |
| `WorkspaceDiffPage.truncated` | a page banner stating the bound dropped at least one file's rows |
| a loaded page with zero entries | "No changes in the working tree" — the only state drawn that way |
| a read with no answer yet | "Reading the workspace diff from Core…" |
| `CommandRejected` | Core's refusal text verbatim in a `role=alert` |
| no `runtime.structured_diff` | the capability named, and explicitly not a clean tree |

Header totals are summed over the entries Core published rows for; when any
entry carries no diff the totals are marked partial rather than quietly
under-reporting, and before a page arrives no total is shown at all. File rows
carry the `WorkspaceChangeKind` glyph (M/A/D/R/?), Core's own `staged` mark,
and per-file counts. A long path keeps its basename: the directory half gives
way a hundred times faster, and the full path is the row's title.

**Action side** (`runtime.operator_git`, GUI-CORE-020). The commit bar acts and
so does the titlebar sync chip. Every button sends one `RunOperatorGitAction`
through the CoreClient seam with a client-chosen `command_id`, and only an
ordered event naming that id settles it. The client runs no git, never falls
back to a shell command, and never reads a fact out of `output`.

- **Bar.** A commit message box bounded at Core's 4 KiB — counted in UTF-8
  bytes, the unit Core measures, so a Chinese message cannot slip past a bound
  Core then refuses — plus `Stage all` (`Stage { paths: [] }`), `Commit`, and
  `Commit & Push`. Each file row carries a stage/unstage toggle whose direction
  follows Core's own `staged` flag; the client never re-derives it from the
  index classification Core already derived it from.
- **"Commit and push" is two commands.** The push is sent only after the commit
  reports `Completed` — Core's typed answer, never a reading of git's output. A
  commit that failed or was refused stops the pair and the bar says the push
  was not sent, because a silently dropped second half would leave the operator
  believing the branch was published.
- **One action at a time**, correlated strictly by `command_id`. The bar is one
  surface with one message box; a second action in flight could only race the
  first for the same index.
- **The actor is Core's.** `RunOperatorGitAction` requires an owner matching its
  envelope owner, and the audit record needs a real one, so an action is sent
  only with the exact `RuntimeOwner` Core bound to the acting Lane. Without one
  the host refuses locally under `D1-OPERATOR-GIT-OWNER` rather than sending a
  default owner, which would record an authorized mutation as belonging to
  nobody. `target` is the same `SourceTarget` the open review is reading.
- **Re-read, never patch.** After an action settles, the `WorkspaceSourceUpdated`
  Core publishes behind it invalidates the page, and the review re-reads through
  the same debounced staleness path a Core-side write goes through.

**Action honesty rules**, each with its own sentence and its own test:

| Core fact | What the view renders |
| --- | --- |
| `CommandRejected` | Core's reason verbatim in a `role=alert`. A refusal happened **before** anything ran |
| `Failed { class, detail }` | the localized sentence for the class plus git's `detail` verbatim. The effect **was** attempted and audited; this is never drawn as a denial |
| `Completed` | a line built from the outcome's resampled `source` — branch, ahead, behind, clean or dirty — never from the output text |
| `Completed.output` | collapsed behind a `Git output` disclosure, with its own note when `truncated` |
| an `ApprovalRequested` with `target.kind = "git"` | the permission dock owns the decision; the bar names the action it is waiting on and stays inert |
| no `runtime.operator_git` | every control visible, disabled, and naming the capability — not `GUI-CORE-020`, which is closed |
| an in-flight action, or `workspace_source.status != Ready` | the sync chip disabled and labelled, with the direction it would have taken still named |

The recovery offered is the one the contract names for the class, and only
that one: fetch for `NonFastForward`, `Push and set upstream` for `NoUpstream`,
and **nothing** for `AuthenticationRequired`, where a retry button would only
be a retry loop against a credential this client cannot supply. An
unrecognized class keeps its own sentence and Core's `detail` rather than
borrowing the nearest-looking one, which would offer the wrong next step.

**The titlebar sync chip** is a control: `Push` when Core's resampled counts
say there is local work, `Fetch` in every other state — behind, or clean. It
never offers a `Pull`, and its tooltip says why: the contract excludes pull,
merge, rebase, `commit --amend`, reset, force push, branch delete, switch, and
stash for `0.3.3`, each for a reason recorded in the frontend integration
contract. Split is still visible and disabled because only the unified body is
built. Conflict content is `GUI-CORE-015` and a separate batch.

**Approval decision context.** The same row renderer draws
`ApprovalRequestView.decision_context` in the D1 permission dock and the D2
decision detail, and `WorkspaceChangeView.diff` on D1's changed-file cards.
`base_sha256` renders as "Preview computed against <8 chars>" — the preimage
the preview was computed against, not a guarantee the file has not moved,
because execution runs the proposed tool input against whatever the file holds
then and Core does not re-check first. The unavailable markers are treated as
claims about Core: D1's `diff` row goes when Core advertises the capability,
D2's marker goes per decision and only where Core attached a context. `shell`
and the `git_*` family legitimately carry none, and there `input_preview`
keeps the exact wording it always had. The dock's action row is pinned while
the context scrolls, so a long preview can never push Approve and Deny behind
the rows they answer.

## D12 merge-gate decisions

`Accept and merge` and `Bounce to origin Lane` are the only two mutations D12
offers; there is no manual-merge escape hatch and the client resolves no
conflict itself. Both travel as their Core command — `AcceptMergeGate` and
`RejectMergeGate` — and both are derived from the rules
`RuntimeContract::decide_merge_gate` and `validate_reject_actor` actually
enforce:

- acceptance needs every required evidence kind verified, an independent
  validator when the gate policy demands one, no conflict bounce still pending
  origin-Lane revalidation, an actor matching the validator's Lane (or the gate
  owner when Core recorded no validator), and reviewed-evidence bindings equal
  to what Core recorded;
- rejection refuses the default owner outright, otherwise admits the
  validator's Lane or the gate owner, and requires a non-empty reason, which
  Core stores as the gate decision and the origin Lane's agent works from;
- `MergeGateUpdated` carrying the requested status is the business fact that
  confirms either decision. Command acceptance is not the decision.

Availability is derived fail-closed: every condition above is *necessary* for
Core to accept the command, never sufficient. Core keeps facts
`frontend-contract-v1` does not carry — canonical context items, permission
snapshots, evidence quality — so a command this projection allows may still be
refused, and the refusal is rendered verbatim in a `role=alert` rather than
pre-empted by a GUI-private gate model. A closed control names its blocking
code (`missing_evidence`, `evidence_not_canonical`, `validator_required`,
`conflict_pending`, `review_not_pending`, `no_actor`, `gate_closed`) instead of
going dark.

The host re-resolves the gate against the current Core view before the command
leaves it and replays the actor and evidence bindings from Core's own records,
so a gate that vanished or closed between render and click fails locally and no
runtime identity or evidence hash is ever rebuilt from display text.

## D12 conflict content

`runtime.conflict_content` (Core `0.3.6`, GUI-CORE-015) attaches an optional
`ConflictContent` to `ConflictBounce` and to `LaneConflictView`. D12 renders it
under the record that carries it: each bounce in the recovery timeline draws
its own pane, and the Lane apply conflicts Core recorded for the Lanes this
gate involves — its owning Lane and every origin Lane a bounce names — are
listed separately, because they are a different Core record with a different
producer rather than a step in this gate's recovery.

What the pane draws, and what it must never draw:

- **Two sides plus the patch preimage. Not a three-way merge.** OURS is Core's
  read-only read of the Lane's current file at the hunk's declared old range,
  numbered from `ours_start`; THEIRS is the incoming patch's new side, numbered
  from `theirs_start`; BASE is that hunk's own preimage, in a collapsible third
  strip. Core computes no merge base and resolves nothing, so the client shows
  no merged text and offers no resolve control — the statement is printed with
  every pane, because the two-column layout is exactly what invites the wrong
  reading. This is the same rule the two-mutation gate rests on: a hunk
  resolved inside the gate would be code that never passed the Lane's own
  gates.
- **The baseline is named, never assumed.** `Evidence { bindings }` is the
  merge path's answer and renders each binding as a chip that opens that
  evidence object's own audit trail, the route D12's revert rows already take;
  `Revision { sha }` renders the short sha with the full value in the row's
  title; `Unknown` renders as unknown, never silently as `HEAD`. A baseline
  kind this build does not name renders as itself.
- **Every rejection carries its classification and its remedy.** The reason
  chip comes from Core's `ConflictHunkReason` and is switched on, never parsed
  out of a message: a context mismatch says the origin Lane has to re-derive
  its patch, an already-applied hunk says there is nothing left to apply, and a
  missing or surviving file says the decision is file-level. An unmodelled
  reason is shown raw rather than folded into a known one.
- **Four absences stay four sentences.** A missing `runtime.conflict_content`
  names the capability in the screen's unavailable row; a record Core published
  no content for says so, and says that an operator `BounceMergeConflict` is a
  human judgement with no failed apply behind it and carries none by contract;
  an `omitted` file keeps its entry and says its hunks are over Core's byte
  bound; a `truncated` payload carries a banner. None of them may read as "no
  conflict".

The rows are the registered `.diffbody > .dl` family from `gui-kit.css`, so a
conflict line and a diff line are the same object on screen. The ours/theirs
tinting follows the D12 design page and deliberately does not reuse
`.dl.add` / `.dl.del`: a conflict side is neither an addition nor a deletion,
and borrowing those classes would claim a classification Core never made.

One implementation note worth stating. `viden-core` re-exports
`ConflictBounce` and `LaneConflictView` but not the `ConflictContent` family
they carry, and the GUI may hold no second `viden-*` dependency, so
`RuntimeProjection` reads the value through Core's own canonical serde
encoding rather than a second parser. A Core-side re-export would remove that
hop; it is recorded against GUI-CORE-015.

## EvidenceView

`⌘E` (`⌃E` off macOS) and the palette's `Open evidence` open the registered
`.evwrap > .evmain(.evbar + .evscroll) + .evdet` family in D1's centre pane.
Like DiffReview it is a **view inside the cockpit, not a route**: `D-RAILNAV`
keeps the activity rail a router to the standalone D-screens and registers the
secondary surfaces as views over the transcript, so closing returns to the
transcript rather than navigating. The CSS comes from `GUI/gui-kit.css`, where
the `D0` promotion mirrored the family for a second consumer.

`E` and `⌘F` were both unbound in this shell — the cockpit's only chords are
`⌘K`/`⌃P`, `⌘O`, and `⌘R` — so `⌘E` toggles the view and `⌘F` focuses the
search box while it owns the centre pane. The D1 Live Work strip is *not* an
entry point: it is one merged `role=status` line over tasks, tools, approvals,
queued input, and `latest_evidence`, and turning the recent-window projection
into a door to the archive is exactly the conflation this view exists to end.

**This is the archive, not `latest_evidence`.** `RuntimeViewState.latest_evidence`
is a recent-window projection — an upsert-by-id list of whatever the current
stream published, with no ordering rule, no cursor, and no content. The two are
never merged and never render in the same list. Everything below comes from
`runtime.evidence_reads` (Core `0.3.6`, GUI-CORE-025).

**Data path.** One `QueryEvidence` -> `EvidencePageLoaded` through the
CoreClient seam, correlated as `QueryWorkspaceDiff` is: one read in flight, the
exact `command_id` on both the page and a `CommandRejected`, and no
acceptance-gated fallback because the page's id is a required field. The first
page is `limit: 50`; the `owner` scope is the exact `RuntimeOwner` Core bound to
the selected Lane, narrowed to workspace/project/Lane and deliberately *not*
carrying Core's turn binding, which would answer "this turn's evidence" for a
question about the Lane. With a Lane selected and no exact Core owner the host
refuses locally under `D1-EVIDENCE-NO-OWNER` rather than reading unscoped,
because an unscoped answer under a Lane's name would show every Lane's evidence
as that Lane's. No selection reads the whole archive. The gate posture is
`QueryAudit`'s, not `QueryWorkspaceFiles`': bounded and owner-scoped, never
tool-gated, so both reads stay answerable in Plan mode.

**Paging.** "Load older" sends one more `QueryEvidence` carrying Core's own
`next_after` verbatim as `after`. The client never parses, constructs, or
compares a cursor, and the control is disabled when Core published none rather
than letting the client build one. `complete` is stated — "archive complete" —
instead of being left to the absence of a button, and because Core filters
before it cuts the page, `complete` describes the *filtered* archive.

**Filter and search are different things, and the view says so.** The kind
chips are `EvidenceQuery.kinds`, so a chip change is a new Core query: a client
narrowing the page it already holds could not tell whether a match sits on a
page it never loaded. The chip vocabulary is the five first-class kinds
(`patch`, `test_result`, `review`, `doc_update`, `release_artifact`) followed by
every other kind the loaded rows actually carry — grouped last, never hidden.
The search box is the opposite: Core exposes no evidence search, so it is a
case-insensitive substring over the rows already loaded, and its tooltip and
`aria-label` name that scope with the loaded row count. A search that hides
every row gets its own sentence, distinct from "no evidence".

**Grouping.** Rows arrive ascending on `(timestamp, id)` and are appended in
arrival order; nothing is sorted client-side. Day grouping is presentation over
Core's order — consecutive rows sharing a local day form one group — so the
undated group leads, which is where Core's ordering already puts it. An undated
row's time column is a dash, never a fabricated clock.

**Re-query rule: mark stale, never reload.** The adapter counts
`EvidenceRecorded` inside its single receive funnel, so a recording drained by
an unrelated screen's poll still reaches an open view; the count is compared
against the revision the loaded pages were read at. A stale list keeps its rows
and gains a banner with a Refresh. Unlike DiffReview it never re-reads on its
own: a diff pane holds one page of one tree, but an evidence list holds however
many pages the operator paged through by hand, and reloading would discard that
and move the rows they were reading.

**Content.** Selecting a row sends one `ReadEvidenceContent` — one in flight,
and the answer is cached per id for the view's lifetime, so re-selecting a row
costs no second read. The cache is dropped when the view closes, because a body
that outlived the view could be rendered against an archive that has since
moved. Answers are filed under the `evidence_id` **Core echoed**, never the id
the client asked with, and while the selected row is not the row an answer
belongs to the block says "reading" rather than showing another row's bytes.
Core reads only the canonical ContextStore bytes the row's own reference names
and verifies them against its `source_hash` first, so the client opens no store
and renders no unverified bytes.

**Content honesty rules**, each with its own sentence and its own test:

| Core fact | What the detail rail renders |
| --- | --- |
| `Text { truncated: true }` | the body plus "over Core's 256 KiB bound: the content is cut", explicitly not "the evidence was short" |
| `Text` / `Diff` `sha256` | "Verified against \<hash\>", so the reader can join what is on screen to the row's canonical reference |
| `Diff` | the shared `diff_rows` renderer DiffReview and the approval surfaces use, because Core parsed the patch with the same producer |
| `Unavailable { SummaryOnly }` | "display-only evidence — Core holds no canonical bytes for it" |
| `Unavailable { MissingCanonicalBytes }` | "Core names canonical bytes the store no longer holds" |
| `Unavailable { HashMismatch }` | "canonical bytes failed verification — not shown" |
| `Unavailable { Binary }` | "the canonical bytes verify and are not text, so there is no body to show" |
| an unmodelled reason or content shape | stated as unnamed, never folded into a known one — both enums are `#[non_exhaustive]` |
| `CommandRejected` | Core's refusal text verbatim in a `role=alert` |

**List honesty rules.** A read with no answer yet says "reading"; a loaded page
with zero entries is the only state drawn as "no evidence in this scope"; a
refusal renders Core's words and loads nothing; and an absent
`runtime.evidence_reads` names the capability, states that this is not an empty
archive, and keeps both entry points visible, disabled, and labelled with it —
never silently hidden. The palette row and the `⌘E` chord read one no-traffic
projection at mount for that answer and send no command.

**Detail rail.** The header carries the kind glyph, the localized kind, Core's
summary, the owning Lane, the timestamp, and the evidence id. The report states
`source`, `path`, `owner lane`, and — when the row names one — the canonical
item, bundle, `source hash`, and producer; a field Core did not record says so
rather than showing a blank. `metadata` is flattened one level and rendered as
facts under a note saying nothing is interpreted: Core's keys are free-form
JSON, and a client that decided what a key *meant* would be inventing a
vocabulary. Nothing infers a row's content, outcome, or verification from its
`summary`. Linked chips name the Core objects the row carries — the canonical
item, the path, the owning task — and are facts rather than navigation.

**Footer.** "Open in review" is offered for a `patch` row and switches the
centre pane to DiffReview through the same `centerView` switch `⌘R` uses; it
states that DiffReview reads the *current working tree* rather than this
recorded patch, and it is disabled with its own sentence for a non-`patch` row,
an unbound host, and a Core without `runtime.structured_diff`. "Open audit
trail" opens D14 scoped to the `evidence` audit object, because `D-AUDIT`'s
link runs one way — audit rows link evidence, not the reverse — so the row is
never given an audit id it does not have.

One implementation note. `viden-core` re-exports `EvidenceView` but not
`EvidenceVerificationState` or `EvidenceQualityStatus`, so the report states the
row's canonical reference, producer, and hash and does not name Core's
verification or quality state for it. It is recorded against GUI-CORE-025.

## Production bootstrap

`src-tauri` is the only GUI member of the root Rust workspace and declares its
own `0.1.0-rc.3` version. `GuiCoreAdapter`, its D4 adapter extension, and
`RuntimeProjection` are the only production boundary modules that hold Core
contracts. `GuiPreferences`,
`WorkspaceSelection`, `ComposerDraft`, and `TranscriptViewport` are
presentation-only state. Closing the window drops the injected client without
sending a mutation.

## Locale and appearance

The production webview requests a transport-safe projection of
`RuntimeViewState.snapshot.ui_preferences`. That resolved Core state alone sets
the document language, skin, effective mode, density, and motion attributes.
The built-in `en` and `zh-CN` catalogs have checked key and placeholder parity;
shortcuts, paths, and code remain literal.

The eight accepted appearance pairs are Aurora, Ice, and Mono in dark or light,
plus dark-only Amber and Phosphor. Invalid or corrupt values use a deterministic
safe fallback and retain diagnostics. The Tauri CSS adapter imports
`docs/viden-design/Viden/tokens.css` directly. Run
`tools/check-generated-tokens.sh` to validate its SHA-256, semantic roles,
theme/density matrices, adapter import, and generated metadata. Production GUI
source contains no copied token values.

Preference controls keep an unsaved in-memory draft. Save and restore use
`SetUiPreferences` or `ResetUiPreferences`: the GUI does not use browser
storage, files, config, or a private preference authority, and rendered state
changes only after `UiPreferencesUpdated` supplies a new resolved projection.
Availability comes from the handshake capability `ui.preference_persistence`
(`preferences_available`); the client defines no finer-grained preference
capability of its own.

The default native binary launches without a bound workspace unless
`VIDEN_GUI_WORKSPACE` is explicit. D1 Welcome opens a native folder chooser,
and `LocalCoreHost` constructs and owns the runtime behind the injected
frontend-safe `CoreClient`. A real host/bootstrap failure renders D6
disconnected; it does not enter D11. The D11 adapter still requires an injected
frontend-safe `CoreClient`; it must
not construct `SessionEngine`/`RuntimeSupervisor` or add a private reducer.
Task 6 now owns the resolved locale/appearance projection and unsaved draft
contract; Task 7 owns D11, and Task 9 owns D1.

## rc.3 visual, metadata, and bundle gate

Task 11 added a framework-neutral component gallery
(`src/screens/component_gallery.ts`) and a deterministic pairwise case
inventory/DOM contract for D1, D11, D4, D6, and the gallery. It enumerates
both locales, every valid skin/mode pair, all densities, system/reduced motion,
and desktop, narrow, and scaled-font requirements. The reviewed visual evidence
is the representative desktop gate plus exact-size D1 same-state QA; gallery,
narrow, and scaled-font captures remain explicitly partial. D1 pass/fail visual
QA compares an independent canonical-state design reference against the
production canonical capture. The older accepted desktop cockpit screenshot is
kept only as historical visual lineage.

Machine-readable accessibility, bounded local performance records,
Browser-controlled same-state screenshots, side-by-side QA, exact methods, and
explicit native audit/profile skips are under
[evidence/0.1.0-rc.3](evidence/0.1.0-rc.3/README.md). The active manifest and
immutable rc.3 snapshot record the same evidence paths and remain
byte-equivalent. The macOS `.app` bundle is a local build artifact only; it is
not installed, signed, notarized, published, tagged, or released.

The gallery module itself was deleted on `claude/hygiene-h1`: nothing ever
imported it, no qa state or script rendered it, and its stylesheet never
reached a bundle, so it produced no capture and could not regress. The
`[evidence] component_gallery` key in `release-manifest.toml` and in the
`0.1.0-rc.2`/`0.1.0-rc.3` snapshots therefore named a path that no longer
exists. Those two snapshots are byte-frozen by
`rc_release_manifest_is_an_immutable_byte_equivalent_snapshot`, so the key
could not be corrected in place. It is dropped from the active manifest and its
`0.1.0-rc.4` snapshot, authored for this release step; the frozen rc.2 and rc.3
snapshots keep it as the record of what those versions claimed, and the module
stays recoverable from Git history.

## Lane monitor actions

D10's cards carry the design's action row. Two of its four controls exist on
`frontend-contract-v1` and two do not, and the row is built so a reader can
tell which is which without clicking.

| Control | What it is |
| --- | --- |
| **Attach** | Not a Core command. It calls the cockpit's own `selectLane` — the path the Lane rail, the Lane tab strip and `⌃⇥` share — and then the router's `conversation` route, so the composer ends up addressing that Lane with the transcript back in the centre pane. |
| **Stop** | `RuntimeCommand::CancelAgentSession` for the Lane's own Agent session, sent under a `gui-d10-stop-…` command id. The outcome shown is the answering event's: accepted, or Core's own refusal text. |
| **Pause** | No Core command exists. Rendered disabled, naming that. |
| **Kill** | No Core command exists. Rendered disabled, naming that, and saying that Stop cancels through `CancelAgentSession` rather than implying Stop is the same verb. |

Stop's target is resolved fail-closed from the Lane's published sessions, the
same way the cockpit resolves its owner binding: exactly one session is a
target, zero says there is nothing to stop, and more than one says the client
will not choose. Without a bound host every control in the row is disabled and
says so, rather than being hidden or left enabled and inert.

`RuntimeCommand` carries no pause and no kill for an Agent session
(`crates/types/src/runtime.rs`); the two disabled controls are the honest
rendering of that, not a placeholder. The event ticker's own
`eventsUnavailable` copy is unchanged here — it belongs to the hygiene batch.

## Fleet drill

A D13 node opens the Lane its task is bound to. Core publishes no
node-to-Lane edge, so the GUI projection joins on the binding Core does
publish — `AgentLaneRecord::task_id` — and carries the result per node as
`laneIds`, deliberately as a list rather than an `Option`:

| `laneIds` | What the node does |
| --- | --- |
| exactly one | `role="button"`, focusable, click or `Enter` hands off through the cockpit's `selectLane`; a visible `Open Lane <id> ↗` line names the destination before the click |
| empty | states that Core bound no Lane to this task; not focusable, not clickable |
| more than one | states that Core bound several and lists them; the client does not choose |

Absence is not an error and not a dead control: a node that cannot open a Lane
never renders as one that can. Without a bound host every node states that
instead. `Space` is deliberately not bound — on a non-button element it is the
webview's own scroll, and taking it costs more than the chord is worth.

## Audit filters

D14 offers the design's actor and time chips over **the page the client is
holding**, and every label on the bar says so. `AuditQuery` carries
`project_id`, `lane_id`, `object` and the `before` cursor — no actor filter and
no time range (GUI-CORE-024) — so a server-side filter is not available, and a
client-side one that stayed quiet about its scope would misreport "no records
for this actor" whenever the matching record sits on a page nobody loaded.

- **Actor chips** are the `actorKind` values the loaded page actually carries,
  plus a neutral `All actors`. A kind that is legal on the contract but absent
  from the page is not offered: a chip that could only ever filter to nothing
  misstates what is loaded. The values render as Core's own words, unlocalized,
  for the same reason the `action` key does.
- **Time chips** are `All time`, `Today (UTC)`, `Last 24 hours` and
  `Last 7 days`, cut on the rows' own Core timestamps. `Today` is the UTC
  calendar day rather than a rolling day, because the rows print a UTC clock
  and two readers comparing evidence must mean the same day by it.
- **The rollup** counts outcomes for what is drawn beneath it and its caption
  always names the loaded page — `Outcomes on the loaded page · N`, or
  `…, filtered · N of M`. It is never a total, because the audit store is
  larger than any page.
- **Filtering to empty** says no row on the loaded page matches. That is a
  different sentence from Core recorded no audit entry for this view, and the
  two never share one.
- **Export** is registered in the design's filter bar with no command behind
  it, so it renders disabled naming GUI-CORE-024.

The filters are held for the mount only and are never persisted: Core is the
single preference authority, and a remembered filter is also how an operator
returns to a trail that is quietly hiding half of itself. Raw replay mode is
untouched — it is the diagnostic event log, and its value is the Core stream
position rather than an actor or a wall clock.

## Decision queue badge

The activity rail's D2 slot carries the count Core publishes as
`D1StatusbarProjection.pendingGateCount`, which is the same number the
statusbar's `⏸` segment prints. The field is `number | null`, and the three
states are three different renderings:

| Value | Rail | Statusbar |
| --- | --- | --- |
| above zero | the design's `.badge` with Core's number | the `⏸` segment |
| `0` | no badge | no segment |
| `null` | no badge, and the slot's name says Core published no decision count | no segment |

`null` is what the shell's pre-connection placeholder projection carries. It
used to be `0`, which rendered as an empty decision queue before anything had
been counted; absence and zero are different facts and are now drawn
differently.

One caveat, recorded rather than hidden: the D2 view's own `pendingTotal` is a
*different* sum — approvals plus pending reviews, where the badge is approvals
plus open non-dormant merge gates — so the badge and the view's header can
legitimately differ. Reconciling them is a projection question for the batch
that owns those counts; counting a second time in the rail would only make the
rail disagree with the statusbar as well.
