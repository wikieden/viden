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
| GUI component version | `0.1.0-rc.3` |
| Minimum Core version | `0.3.5` |
| Supported frontend schemas | `[1]` |
| Common branch base | `3a7740ea72e58f4a22248a80f9e7324c49bb0f73` |
| Core final checkpoint | `f7fe1b31dfb237e4062209767a7051c2b2c68b93` |
| Core code checkpoint | `17fa2071398d5eaf30045257163d57d22d99177b` |
| Contract payload | `5bd2b80b0953f4194d082940a7b9164c7231ca2d` |
| Canonical D1 fixture | `d1-main-cockpit.json`, SHA-256 `f96ba30cc6e80aa52cb15a2fd1f03c082487a3cd4779c25f61e42ee1548e1e3b` |
| Required Core capabilities | 15 frozen values plus additive extension capabilities, including `runtime.cockpit_context_v1` |
| Built-in locales | `en`, `zh-CN` |
| Appearance | 5 skins, 8 valid skin/mode pairs, 3 densities, 3 motion policies |

The active machine-readable manifest is
[release-manifest.toml](release-manifest.toml). Its immutable rc.3 snapshot is
[manifests/0.1.0-rc.3.toml](manifests/0.1.0-rc.3.toml); both files must
remain byte-equivalent for this release checkpoint. Earlier alpha, beta, and
rc.2 snapshots remain historical evidence and are not rewritten.

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
| Project open / D11 intake | native folder open plus project probe, provider health, config preview/confirm, and credential handles | `LocalCoreHost::open_workspace` provides trusted folder rebinding and `runtime.recent_work` answers `QueryRecentWork`; Core publishes no first-run intake signal, no repository-clone or project-scaffold command, and no concurrent multi-root supervision | Welcome uses the native folder picker and host rebind directly and lists Core's recent projects; the titlebar project picker adds the guarded switch. D11 stays an explicit in-project configuration flow and never owns folder open. Reachable at `?screen=d11` and from the agent menu's `Full setup`; the shell never redirects into it on its own |
| D4 lane creation | typed role, route, gate strength, mutation policy, target, budget, worktree preview, lane receipt | `PreviewStarterLane`/`CreateStarterLane`, Core-resolved preview, invalidation, approval, exact receipt, and `runtime.starter_lane_preview` advertisement are available | Task 8 renders the four-step reviewed flow; older Core handshakes still fail closed visibly with zero sends |
| D1 cockpit | no-project welcome center, zero-Lane project cockpit, activity/lane rails, streaming transcript/tool rows, Environment, Live Work, composer, evidence/context/cost facts | Stream/tool/approval/queue/task/lane/owner/evidence/context/cost/preferences/recent-work facts exist; diff/apply, stable audit timeline, actionable lane recovery, and concurrent multi-workspace supervision remain incomplete | No bound host renders Welcome; a bound empty project remains D1 and exposes `New Lane`; the Lane rail groups its Lanes under the one project Core supervises (`GUI-CORE-023`); live work renders from `RuntimeViewState` |
| Permission dock | scoped approve/deny, risk, target, expiry, default action, audit id | `ApprovalRequestView` and `RespondToApproval` exist | Usable through Core; GUI cannot execute tools directly |
| D2 decision center | one cross-Lane queue over gate approvals, lane asks, and contract confirmations, on one card skeleton of context, evidence, and action bar | `pending_approvals` with `RespondToApproval`, `review_requests` with `DecideReview`, and `contracts` with `ConfirmContract` exist; a structured approval diff and a pending-contract fact are missing | Reachable at `?screen=d2`; gate, contract, and review decisions all send Core commands, a review verdict carries an optional reviewer note and confirms only on the ordered `ReviewRequestUpdated` Core publishes, a review Core would refuse stays disabled under `D2-REVIEW-SETTLED` or `D2-NO-REVIEWER-ACTOR`, the approval diff stays unavailable under `GUI-CORE-012`, and the contract group is labelled decided history under `GUI-CORE-013` with both contract verdicts disabled-and-labelled by the same code, because Core refuses a second decision on a record it already holds |
| D10 lane monitor | one card per Lane across every project, with gate strength, status, progress, evidence, cost meterability, and an attention count | `lanes`, `lane_runtime_owners`, `tasks`, `agent_sessions`, `latest_evidence`, and `AgentLaneRecord.run_stats` exist; the ordered history comes from the audit timeline rather than the view state | Reachable at `?screen=d10`; read-only, gate strength comes from `AgentLaneRecord.gate_strength` rather than the agent label, an unbound Lane reports no project, a Lane with no Core task reports no progress, a cost-blind route (`AgentRoute::cost_meterability`) is marked and shows the bounded run facts Core recorded instead of an inferred cost — absent rather than zeroed when Core observed no run, and with an unknown exit code labelled as such — and the event ticker is one bounded newest-first page of Core's append-only audit timeline (`QueryAudit` -> `AuditPageLoaded`, limit 50, unscoped so it spans every project), rendering each record's stable id, raw dotted action key, owning project and Lane, and timestamp, with an absent `runtime.audit`, a pending read, a refusal, and an answered-but-empty timeline kept as four different lines |
| D12 integration gate | conflict banner, gate policy, bounce-to-origin-lane recovery timeline, post-merge rollback, and no manual merge | `merge_gates`, `conflict_bounces`, `reverts`, `check_runs`, `AcceptMergeGate`, and `RejectMergeGate` exist; no structured conflict content is published | Reachable at `?screen=d12`; `Accept and merge` and `Bounce to origin Lane` send their Core commands, each opening only when the rules `decide_merge_gate` enforces are met and naming its blocking code otherwise, the timeline and reverts are scoped to the selected gate, and the conflicting hunk is unavailable under `GUI-CORE-015` |
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

The shell reaches D11 at `?screen=d11` and from the agent menu's `Full setup`
action, which opens the full intake flow rather than the single-Lane D4 form.
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
an action behind it: routing slots open their restored screen, the Lane slot
toggles the Lane rail, and `Work` — the slot marked `aria-current` because it
is the screen already showing — returns focus to the composer. A slot with no
available action is disabled rather than enabled and inert.

Every routing slot is labelled — tooltip and accessible name alike — with the
screen it actually opens: Integration gate (D12), Decisions (D2), Audit
timeline (D14), Lane monitor (D10), and Fleet board (D13), each carrying the
registered `GUI/gui-icons.jsx` glyph closest to that screen. The rail
previously wore an editor's file-explorer vocabulary (Search, Source control,
Evidence, Diagnostics, Inbox) over those same routes, which is worse than a
missing label: the operator could only learn the real mapping by clicking, and
every tooltip and screen-reader announcement taught it wrong until they did.
The destinations are honest as of this change and the routing itself is
unchanged. What stays open is the *design* question rather than the labelling
one — the accepted rail has no slot for the decision queue, the audit trail,
the lane monitor, or the fleet board, and the design reaches several of them as
in-cockpit secondary views instead, so those four slots were a routing decision
awaiting design adjudication. That adjudication has since landed: the design
package's `docs/SPEC.md` decision `D-RAILNAV` (2026-09-08) accepts the rail as a
router to the standalone D-screens and keeps the flagship's in-page view
switching as design exploration, so this shipped routing is the accepted model
rather than an interim one.

`New Lane` opens one compact, anchored popover with the built-in Viden Agent
selected by default, discovered ACP Agents, the task draft, branded Agent
identity, Core-projected eligibility/probe diagnostics, and a
presentation-only isolation hint. `Full setup…` routes to the existing D4
compatibility flow instead of expanding the quick creator in place. Git
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
The `.gitops` block beside it holds two chips: `↑ahead ↓behind`, a `role=status`
element rather than a button because frontend-contract-v1 publishes no operator
git command (contract request `GUI-CORE-020`), and `⎇ N worktrees`, the block's
only control, which opens the D10 Lane monitor and is disabled when no
navigation handler is injected. `N` counts the distinct worktrees of the
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
bar's only interactive element — which navigates to the D2 decision queue.

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
facts are reachable by internal scrolling. Structured diff rows
(`GUI-CORE-012`), operator apply and commit (`GUI-CORE-020`), and checkpoint
capture and restore (rendered as `GUI-CORE-003`, contract request
`GUI-CORE-018`) remain explicit unavailable facts; D1 never fabricates a
successful placeholder. The `audit` row is gone: Core's audit timeline closed
that gap, and a dock row claiming otherwise would be a stale statement about
Core rather than a fact.

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

**The Lane rail** is the design's workspace explorer: one `.wsroot` group
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

**What waits for C2.** The commit bar the family draws is present, entirely
disabled, carries no handlers at all, and states `GUI-CORE-020` — operator git
actions need `runtime.operator_git`, which `frontend-contract-v1` does not
carry. Split is visible and disabled because only the unified body is built.
Conflict content is `GUI-CORE-015` and a separate batch.

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
`0.1.0-rc.2`/`0.1.0-rc.3` snapshots therefore names a path that no longer
exists. Those files are byte-frozen by
`rc_release_manifest_is_an_immutable_byte_equivalent_snapshot`, so the key
cannot be corrected in place; it is dropped when the `0.1.0-rc.4` manifest is
authored, and the module stays recoverable from Git history as the rc.3
record.
