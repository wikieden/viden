# Viden 0.3.4 Plan - Cockpit Alignment And Trusted Delivery Completion

Chinese version: [release-0.3.4-plan.zh-CN.md](release-0.3.4-plan.zh-CN.md)

`0.3.4` is the aggregate workspace milestone that follows the `0.3.3`
candidate. `PLAN.md` and `docs/parallel-development-plan.md` previously named
it "visual fidelity and production release gate"; this document re-scopes it
(see Scope Amendments) to one goal first: bring the shipped GUI cockpit level
with the accepted D1 design and every interaction and navigation the design
package already specifies, and finish the real-task delivery that `0.3.3`
left partial. The Core contract content lives in
[release-0.3.4-contract-design.md](release-0.3.4-contract-design.md).

Status: plan written 2026-09-12 from the design-versus-implementation gap
review of the same day. Nothing in this document is implemented. Every batch
is dispatched separately, reviewed adversarially in the main session, and
merged only on an explicit push.

## Baseline

`main` at `25072a0a` on 2026-09-12:

| Line | Version | State |
| --- | --- | --- |
| Core | `0.3.6` immutable checkpoint | 23 extension capabilities, schema `1` |
| TUI | `0.3.4` | parity minimum for the four `0.3.3` capabilities; composer routing and `/git` Lane target fixed (T1c) |
| GUI | `0.1.0-rc.4` | DiffReview and EvidenceView in-cockpit; D2/D4/D6/D10/D11/D12/D13/D14 as full-window screens |

The `0.3.3` real task reached an approved, typed, applied edit and stopped
before the commit: without a selected Lane no operator identity exists
(GUI-CORE-027), the durable evidence archive never sees supervisor-driven
work (GUI-CORE-028), a session-level queued follow-up is never executed, and
an approval's audit id has no durable row. The open register is 009, 013,
018, 019, 021, 023, 026, 027, 028.

## Gap Summary

Recorded from the 2026-09-12 review (design package D1 plus D2 to D14 versus
`apps/gui`); this plan is the response to it.

- **Navigation.** The design's rail routes to Conversation, Lane monitor,
  Diff review, Evidence, Pin lanes, Settings (Worktrees dropped, Diagnostics
  deferred, Inbox roadmap). The app's rail routes to Work, Lanes toggle,
  Integration gate, Decisions, Lane monitor, Fleet, Audit, Settings; the two
  registered families DiffReview and EvidenceView have no rail entry; D2,
  D10, D12, D13, D14 render as bare full-window screens that discard the
  titlebar, rail, lane rail, composer, and statusbar, with no return path.
- **Cockpit centre.** No Lane tab strip and no `Ctrl+Tab` Lane switching; no
  inline diff on tool blocks and no ordered user/assistant rows
  (GUI-CORE-009); no focus mode; the palette has no "New Lane" action and
  its file rows are inert; "Full setup" goes to D11 instead of the D4 Lane
  wizard; `Cmd+L`, `Cmd+G`, `Cmd+.` are unbound.
- **Context dock.** Five static key-value sections where the design has
  tabs (Environment, Files, Diff, Code, Docs), clickable Changes rows, a
  Commit-or-push row, PR status, a budget bar, and an inspector. Files and
  Diff need only capabilities that already exist.
- **Lane sidebar.** Toggle only; the design's pinned/floating modes with
  hover peek and persistence (`D-SIDEBAR`) are absent.
- **Core blockers.** 027, the session queue, 028 plus audit durability, 009,
  a native turn-liveness fact, a per-Lane source view, and an open-file
  command.

## Goals

1. **Rail-as-router, fully.** Every registered destination is reachable from
   the rail and renders inside the cockpit chrome, with a return path, exactly
   as `D-RAILNAV` and the D1 flagship show.
2. **Cockpit centre parity.** Lane tab strip, Lane switching, tool blocks
   with inline diff, palette Lane creation, the designed chords, focus mode.
3. **Context dock parity.** Tabs and sections backed by facts Core already
   publishes, then by the new file-read fact.
4. **Deliverable without a Lane.** A workspace-scoped operator identity so the
   commit and push work from the cockpit as designed.
5. **Truthful queue, evidence, and audit.** A queued follow-up runs; a native
   or ACP mutation lands in the durable evidence archive with canonical
   bytes; an approval decision is a durable audit row.
6. **Real task completed.** The `0.3.3` task rerun natively through the GUI
   from intake to an accepted push, with the archive and audit non-empty.

## Scope Amendments To The Controlling Plan

Dated 2026-09-12; recorded as notes in `docs/parallel-development-plan.md`.
The user may veto any of them.

- **The production release gate moves to `0.3.5`.** Packaging, notarization,
  Homebrew, and live-provider certification are not started until the cockpit
  is level with its design; certifying the current cockpit would certify the
  gaps above.
- **Plan Studio and Agent Board move to `0.3.5`**, for the same reason they
  moved out of `0.3.3`: no registered D-screen and no contract.
- **Secondary screens become in-cockpit views.** D2, D10, D12, D13, and D14
  render in the centre pane with the full chrome retained, the way
  DiffReview and EvidenceView already do; `?screen=` deep links stay as
  entry points that open the cockpit on that view. This resolves the
  question the gap review left open in favour of the D1 flagship's own
  behaviour.
- **D5 gallery review is out.** The design marks it active, but Core has no
  gallery or variant model; it is opened as GUI-CORE-029 for `0.3.5`.
- **Unchanged from `D-RAILNAV`:** WorktreeBoard dropped; the embedded subagent
  tree, DiagnosticsView, DockSD, pop-out windows, D7, D8, D9, and Pip stay
  outside this milestone.

## Component Version Targets

| Line | Now | `0.3.4` target |
| --- | --- | --- |
| Core | `0.3.6` | `0.3.7` immutable checkpoint, schema `1`, six additive capabilities (23 to 29; amended 2026-09-12 from 28, see the contract design) |
| TUI | `0.3.4` | `0.3.5`, parity minimum for the new facts |
| GUI | `0.1.0-rc.4` | `0.1.0-rc.5`, cockpit level with D1 |

## Batches

Each batch is one worktree under `.worktrees/`, developed test-first, and
reviewed against its brief before merge. Core batches land before the client
batches that consume them; the contract design gives the exact shapes. G3 to
G6 need no Core change and start immediately, in parallel with Core.

| Batch | Owner | Content | Depends on |
| --- | --- | --- | --- |
| CD contract design | main session | `release-0.3.4-contract-design{,.zh-CN}.md`: exact shapes for C5 to C9 | this plan accepted |
| C5 `runtime.workspace_owner` + `ui.layout_preferences` | Core | Core mints `workspace_id`/`project_id` at open and publishes `WorkspaceRuntimeOwnerBound`; `RunOperatorGitAction` with `SourceTarget::Workspace` authorized and audited under it; `LaneSourceUpdated` per Lane worktree; a separate `UiLayoutPreferences` record (`lane_sidebar_mode`, hidden statusbar segments) because a `UiPreferences` field would move every base digest (amended 2026-09-12); two fixtures; closes GUI-CORE-027 | CD |
| C6 `runtime.turn_lifecycle` | Core | `TurnStarted`/`TurnFinished` for native and ACP turns with the owner; `assistant_stream` settled on `TurnFinished`; the session-level queue drained on `TurnFinished` with `InputDequeued`; fixture; closes E1 defect 1 and compatibility follow-up 3 | CD |
| C7 `runtime.durable_work_evidence` | Core | an applied native mutation and an ACP patch each produce an archived `patch` row with canonical ContextStore bytes and `source_hash`, persisted through the `runtime_projection` rows the archive rebuilds from; approval decisions written as durable audit rows; fixture; closes GUI-CORE-028 and E1 defect 4 | C6 |
| C8 `runtime.transcript_rows` | Core | owner-scoped ordered typed rows (user, assistant, tool call, tool result, check run) over the existing transcript page; fixture; closes GUI-CORE-009 | C6 |
| C9 `runtime.workspace_file_reads` | Core | `ReadWorkspaceFile { path, byte_limit }` to `WorkspaceFileLoaded { content: Text \| Binary \| Unavailable }` gated on the `read_file` tool spec; fixture | C5 |
| G3 navigation shell | GUI | rail set per design plus D2/D10/D12/D13/D14 as centre-pane views with chrome and a Close/`Esc` return; rail entries for DiffReview and EvidenceView; `Cmd+G` to Decisions; lane sidebar pinned/floating with hover peek, persisted through `lane_sidebar_mode` once C5 lands and held in memory before; statusbar config gear | none |
| G4 cockpit centre | GUI | Lane tab strip; `Ctrl+Tab`/`Ctrl+Shift+Tab`; palette "New Lane…" action and `Cmd+L`; "Full setup…" to D4 and the D4 wizard aligned to the design's four steps within what Core models (role, agent, skill pack, mutation policy; execution target local only); tool blocks with inline diff from `WorkspaceChangeView.diff` and check-run blocks; focus mode `Cmd+.` | none |
| G5 context dock | GUI | tabs Environment/Files/Diff; Environment sections Changes (rows open DiffReview on the file), Commit or push (routes to DiffReview and the sync chip), budget bar, Sources; Diff tab from `QueryWorkspaceDiff`; Files tab from `QueryWorkspaceFiles` (tree only until C9); MCP status shown only as unavailable with the reason | none |
| G6 secondary views | GUI | D10 Attach (select Lane) and Stop; D13 node drill to the Lane; D14 client-side actor/time filters and rollup; D2 shows the queue count in the rail badge | G3 |
| G7 consumers | GUI | commit bar and sync chip without a Lane (C5); composer queue truth and turn liveness (C6); EvidenceView archive rows and D14 approval rows (C7); ordered transcript rows (C8); Files tab content, palette file rows, Code tab (C9); register closes 009, 027, 028 | C5 to C9, G3 to G5 |
| T2 TUI parity | TUI | `/git` workspace target under the Core-published owner; native turn liveness replaces the client-side window; evidence inspector shows archived patches; regression baselines with causes | C5 to C7 |
| H2 hygiene | GUI, Core | E1 defects 6 (verified beside HashMismatch), 7 (process exit after palette Escape, reproduce first), 8 and 9 (Welcome layout and wording); D10 `eventsUnavailable` copy; strict apply binary drop | none |
| E2 release evidence | all | the `0.3.3` real task rerun natively through the GUI with the screen unlocked: intake, Lane, approved edit with hunks, check run, DiffReview commit with and without a Lane, push refused then accepted, non-empty archive and audit; bilingual checkpoints; Core `0.3.7` checkpoint and client versions recorded | everything above |

Dispatch order: CD first (main session). Then C5 in parallel with G3 and G4;
C6 in parallel with G5; C7 and C9 in parallel with G6; C8; then G7, T2, with
H2 interleaved wherever a reviewer is idle; E2 last. Local integration
branch `claude/int-0.3.4`; `main` moves only on an explicit push.

## Gates

Per batch, the smallest relevant check first, then the branch gates from
`docs/parallel-development-plan.md`. The commands are those of the `0.3.3`
plan; the additions are:

- the nine frozen `frontend-contract-v1` base fixtures keep their exact bytes;
  the `UiPreferences` field is additive and, if any base digest moves, ships
  as a separate preference record instead;
- the capability count moves from 23 to 29 in `scripts/tui-regression.sh`
  and `apps/tui/src/tui/client.rs` with tracking comments;
- every in-cockpit view has a qa harness state, a PNG in both themes, and an
  `EVIDENCE.md` row; every rail destination has a test proving the chrome
  survives the switch and `Esc` returns;
- the plugin-host crate's `context_reducer_process_*` tests are run serially
  (`--test-threads=1`); they flake under any parallel run on this host.

## Exit Criteria

`0.3.4` is complete only when all of the following hold on `main`:

- Core `0.3.7` is recorded as an immutable checkpoint with the 29-capability
  set, six new extension fixtures, and unchanged base fixture bytes.
- Every rail destination in the design that is not dropped, deferred, or
  roadmap renders inside the cockpit chrome with a return path, and the two
  `0.3.3` families have rail entries.
- The cockpit centre has the Lane tab strip, Lane switching, palette Lane
  creation, `Cmd+L`/`Cmd+G`/`Cmd+.`, and inline tool diffs; the context dock
  has Environment, Files, and Diff tabs backed by Core facts.
- A commit and a push complete from the cockpit with no Lane selected, under
  a Core-published workspace owner.
- A queued follow-up runs; an applied native edit appears in EvidenceView as
  a `patch` row with verified content; the approval that allowed it appears
  in D14.
- The register closes 009, 027, and 028 with dates; 013, 018, 019, 021, 023,
  026, and 029 carry `0.3.5` notes.
- The E2 evidence document exists in both languages with native GUI
  captures of every step, and the compatibility follow-ups are closed or
  re-dated.

Explicitly not part of completion: packaging, notarization, Homebrew, live
provider certification, Plan Studio, Agent Board, D5, D7, D8, D9, DockSD,
Diagnostics, pop-out windows, multi-workspace supervision (023), forge
status (021).

## Risks

- **Chrome-retaining views change the D-screen tests.** D2/D10/D12/D13/D14
  each have projection and vitest coverage written for full-window
  rendering. G3 must keep those tests by hosting the existing screen renderers
  inside the centre pane rather than rewriting them.
- **Workspace owner minting touches every non-Lane command.** C5 changes what
  `RuntimeOwner::default()` means on the wire; the contract design must show
  that the nine base fixtures, which carry empty ids, still replay
  byte-identically.
- **Turn lifecycle for the native path is new Core state.** C6 is the first
  time the supervisor publishes a terminal fact for a built-in turn; the
  design must state what happens on a crashed or cancelled turn so the
  queue never drains into a dead session.
- **Durable evidence adds live events to native turns.** C7's new
  `EvidenceRecorded` on a native mutation must be checked against the frozen
  fixtures; they are static JSON, so only regeneration moves them, and the
  check is that none is needed.
- **Native GUI runs need an unlocked screen.** E2 and any G-batch live check
  stop, rather than send keystrokes, when the host session is locked.
- **Plugin-host flake.** Known and unrelated; serial rerun is the rule, not a
  pass.
