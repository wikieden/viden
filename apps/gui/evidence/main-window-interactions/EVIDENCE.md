# Main-window interactions — visual evidence

Chinese version: [EVIDENCE.zh-CN.md](EVIDENCE.zh-CN.md)

Covers the main-window interaction work on `claude/gui-main-window-interactions`:
the D6 recovery actions (restart / close Lane, the presentation-only inspect
expansion, and the rejection alert), the composer work-mode / permission /
model selectors together with the cockpit statusbar, the Settings overlay
behind the activity rail's gear, D11 project intake, and the D12 merge-gate
accept / bounce action bar with its mandatory reason input.

Extended on `claude/gui-supervision-debts` with the D2 review verdict
(`RuntimeCommand::DecideReview`, closing GUI-CORE-011) and the D10 cost-blind
lane marker with the bounded run facts Core publishes for it.

Extended again on `claude/gui-d14-audit` with D14's two modes: the audit trail
over Core's `QueryAudit` -> `AuditPageLoaded` contract (`runtime.audit`), the
same trail scoped by the object D12's revert row navigates with, and the raw
event replay fallback a Core without `runtime.audit` leaves behind.

## What the harness is

[`qa.html`](qa.html) plus [`qa.ts`](qa.ts) render exactly one screenshot state
per `?state=` value. The page calls **only exported production render
functions** — `renderD1Cockpit`, `renderD6Recovery` (through the cockpit's own
work surface), `renderD11Intake`, `renderD12IntegrationGate`,
`renderD2Decisions`, `renderD10LaneMonitor`, `renderD14`, and the components they mount. Nothing here reimplements a control, a label, or a
layout, so a regression in `src/**` shows up in the capture instead of being
masked by a harness copy of the UI.

Rendering lives in `qa.ts` rather than an inline `<script>` so `tsc --noEmit`
typechecks the harness against the production render signatures.

### Determinism

- `Date.now` is frozen to `2026-01-01T00:00:00.000Z` before the first render.
  The work-status strip prints `now() - startedAt`, so an unfrozen clock would
  make two captures of the same state differ.
- `Math.random` is frozen to `0`.
- The cockpit is mounted with `poll: false`, so no timer ever refreshes a
  capture while the operator is framing it.
- Every host callback is a promise that never resolves, so no state can change
  after the harness settles. The single exception is `d6-error`, whose
  `sendD6Intent` rejects with one fixed sentence on purpose — that rejection
  *is* the state being captured.
- Each state finishes by setting `document.documentElement.dataset.captureReady`
  to its own name. Capture only after that attribute is present.

## Where the projections come from

| State group | Source | Kind |
| --- | --- | --- |
| `d1*`, `settings*`, `d6-*` | [`../../tests/support/d1_projection.ts`](../../tests/support/d1_projection.ts) | the shared D1 fixture the vitest suites mount |
| `d12-actions`, `d12-blocked`, `d12-conflict-none` | [`../gui-screen-restore/projections/d12.json`](../gui-screen-restore/projections/d12.json) | **generated** by `tests/capture_projections.rs` |
| `d12-conflict-content` | [`../gui-screen-restore/projections/d12-conflict.json`](../gui-screen-restore/projections/d12-conflict.json) | **generated** by the same test from the canonical `conflict-content.json` fixture |
| `d12-conflict-omitted` | [`../gui-screen-restore/projections/d12-conflict-omitted.json`](../gui-screen-restore/projections/d12-conflict-omitted.json) | **generated** by the same test — the bounded variant |
| `d2-review-*` | [`../gui-screen-restore/projections/d2-review.json`](../gui-screen-restore/projections/d2-review.json) | **generated** by `tests/capture_projections.rs` — the decision queue with the pending review selected |
| `d2-review-confirmed` | [`../gui-screen-restore/projections/d2-review-decided.json`](../gui-screen-restore/projections/d2-review-decided.json) | **generated** by the same test — the queue Core leaves behind after `decide_review` |
| `d10-blind*` | [`../gui-screen-restore/projections/d10.json`](../gui-screen-restore/projections/d10.json) | **generated** by `tests/capture_projections.rs` |
| `d14-audit*` | [`../gui-screen-restore/projections/d14-audit.json`](../gui-screen-restore/projections/d14-audit.json) | **generated** by the same test — the projection the production acceptance-first correlation produced from a Core `AuditPageLoaded` page |
| `d14-raw-fallback` | [`../gui-screen-restore/projections/d14-raw.json`](../gui-screen-restore/projections/d14-raw.json) | **generated** by the same test through `CoreClient::replay` |
| `d11`, `d11-recent` | mirrors the fixtures in `tests/d11_intake.spec.ts`, plus the hand-written `RecentWorkResult` below for the history panel | hand-written; D11 has no generated capture projection yet |
| `review*` | one `WorkspaceDiffLoaded` page: the same two entries the canonical extension fixture carries — a staged `modified` file with one real hunk and an `omitted` addition whose counts stay real — plus the same `truncated: true` page flag. `review-rejected` replaces the outcome with the fixture's own `CommandRejected` reason; `review-empty` keeps the page and empties `entries` | the `structured-diff.json` extension fixture and the page shape asserted in `tests/workspace_diff.rs` |
| `review-commit*`, `review-push-no-upstream`, `review-rejected-action` | the same page plus one `OperatorGitProjection`, each a delta on a Core that publishes `runtime.operator_git` and one owner this client may act as. The three answers are the canonical `operator-git.json` fixture's own three: an approved `Commit` that completed with `ahead: 2` and a clean tree, a `Push` that failed with `NoUpstream` and git's own sentence, and a `Stage` refused before anything ran with the fixture's `git_add` deny reason | the `operator-git.json` extension fixture and the projections asserted in `tests/operator_git.rs` |
| `approval-hunks` | the shared pending approval retargeted to `edit_file` and given the fixture's one-file `decision_context` and `base_sha256`. The tool is changed on purpose: `edit_file` is one of exactly three proposals Core attaches a context to, and a `shell` approval carrying hunks would be a screenshot of something Core never publishes | the `edit_file` approval in `structured-diff.json` and `an_approval_with_a_decision_context_projects_its_rows_and_base_hash` in `tests/decision_context.rs` |
| `lane-rail`, `project-picker`, `project-switch-confirm` | the shared D1 fixture plus a hand-written `RecentWorkResult` | hand-written; `frontend-contract-v1` has no canonical recent-work capture projection yet, so the shapes mirror `tests/recent_work.rs` and `tests/project_picker.spec.ts` |

The D12 projection is never hand-written. `tests/capture_projections.rs` runs
the real Rust projection over the canonical `frontend-contract-v1`
`merge-gate.json` fixture and serializes the result, so the captured pixels are
what Core facts actually produce. Regenerate after any projection change:

```bash
cargo test -p viden-gui --test capture_projections -- --ignored
```

W4 gave the D12 gate actions typed availability codes; the committed
`d12.json` now carries `missing_evidence` on accept and `no_actor` on reject,
which is what `d12-blocked` captures.

The same test now also emits `d2-review.json` (the queue with the pending
review selected, so the live accept/reject bar and the reviewer-note field come
from a real Core review record), `d2-review-decided.json` (the same queue with
that review settled — status `accepted`, Core's fresh decision audit id, the
reviewer note stored on the record), and records bounded run facts on the
`multi-lane.json` fixture's terminal lane, so `d10.json` carries a cost-blind
lane with run stats beside a metered lane that has none.

`d2-review-decided.json` exists because confirmation comes *from*
`ReviewRequestUpdated`: by the time the outcome is confirmed the review is
already settled, so a confirmed receipt sitting over a still-pending row would
be a state production can never reach. The harness therefore hands the intent
result the projection Core would actually have republished for that outcome.

### Deltas written on top of a source

Every value the harness adds is a delta on one of the sources above, and each
one carries an inline comment in `qa.ts` naming the fixture it mirrors.

| State | Delta | Mirrors |
| --- | --- | --- |
| all `d1*` | `preferences.locale/skin/mode` follow the URL parameters | the cockpit reads its locale from the projection, not the document element |
| all `d1*` | a `ContextUsageProjection` on the context dock and the three statusbar fields the shared fixture leaves empty (`context`, `diagnosticsCount`, `pendingDecisionCount`, the last set to the generated D2 projection's own `pendingTotal` because since G7 the host derives both from one function) | the populated statusbar fixture in `tests/statusbar.spec.ts` |
| all `d1*` | `agentAdapters[0].models` | the adapter fixture in `tests/composer_controls.spec.ts` |
| `d6-actions`, `d6-error` | a stopped session whose `restart` carries a session id and `close_lane` a lane id | the `STOPPED` fixture in `tests/d6_recovery.spec.ts` |
| `d12-actions` | required evidence recorded, validator satisfied, both actions available with a `null` code | the `DECIDABLE` fixture in `tests/d12_integration_gate.spec.ts` |
| `d12-conflict-content` | a second rejected hunk with a different reason (`already_applied`) on Lane B's bounce, written as Core's own `ConflictContent` wire form, so the capture shows that each rejection carries its own classification and remedy. The preimage strips are opened, because a collapsed third side cannot prove the claim the pane makes | `d12_projects_the_hunks_two_sides_and_preimage_core_published_for_a_bounce` in `tests/d12_integration_gate.rs` |
| `d12-conflict-omitted` | the same bounce's content replaced with a `revision` baseline, one rendered file, one `omitted` file, and `truncated` set | `d12_keeps_an_omitted_file_and_a_truncated_payload_distinct_from_an_empty_conflict` in `tests/d12_integration_gate.rs` |
| `d12-conflict-none` | none; this is the untouched `merge-gate.json` projection, whose bounce Core published with no content at all | `d12_leaves_a_bounce_without_content_absent_rather_than_empty` in `tests/d12_integration_gate.rs` |
| `d11` | a probed `/workspace/demo` rust project with a credential-locked provider | the probed-project fixture in `tests/d11_intake.spec.ts` |
| `d2-review-*` | the intent result the host returns (`pending`, `confirmed`, or `rejected` with Core's own refusal sentence); the reviewer note is typed through the production input listener. `confirmed` swaps in the generated decided projection; `pending` and `rejected` keep the pending one, because Core has not answered yet in the first case and refused the command outright in the second | the outcome states asserted in `tests/d2_decisions.rs` and `tests/d2_decisions.spec.ts` |
| `d2-review-blocked` | both verdicts forced unavailable with `D2-NO-REVIEWER-ACTOR` | `d2_review_actions_fail_closed_with_a_local_code_when_no_actor_is_derivable` in `tests/d2_decisions.rs` |
| `d10-blind-unobserved` | `runStats` cleared on the blind lane | `d10_leaves_run_stats_absent_for_a_blind_lane_core_never_observed_running` in `tests/d10_lane_monitor.rs` |
| `d14-audit-scoped` | only `scope` set to the `revert:revert-1` object; the rows are the generated page untouched, because a capture must not invent records for a filter | `a_scoped_query_passes_the_exact_object_through_and_reports_the_scope` in `tests/d14_audit_trail.rs` |
| `d14-raw-fallback` | `capabilityAvailable` cleared with no rows and an idle outcome — absence, not emptiness | `audit_mode_is_unavailable_and_sends_nothing_without_the_core_capability` in `tests/d14_audit_trail.rs` |
| `d11`, `d11-recent` | the shared two-project `RecentWorkResult` handed to the screen's recent-work port | the loaded-rows fixture in `tests/d11_intake.spec.ts` |
| `palette` | one cross-Lane merge gate and one ask handed to `loadPaletteCrossLane` | the gate fixture in `tests/d12_integration_gate.spec.ts` and the single `liveWork.approvals` entry the D1 fixture already carries |
| `palette`, `palette-files`, `dock-files` | six real Viden paths in Core's lexicographic order with fixed byte sizes, handed to `loadWorkspaceFiles` with no prefix | the loaded-inventory fixture in `tests/command_palette.spec.ts` and the page shape asserted in `tests/workspace_files.rs` |
| `dock-files` | a second inventory page for the `apps/` prefix — the three real directories under it, with `complete: false`, because Core's page really does stop before that subtree does | the prefixed read asserted in `a_prefixed_read_sends_cores_own_prefix_and_keeps_the_page_clamp` in `tests/workspace_files.rs` |
| `dock-environment`, `dock-files`, `dock-diff` | the `REVIEW_PAGE` diff the review states already use, handed to the same `workspaceDiff` read pair — the dock and DiffReview render one page, so the capture uses one fixture | the page shape asserted in `tests/workspace_diff.rs` |
| `d10-ticker` | one four-record audit page over two interleaved projects, applied through the screen's own `applyEvents` | the canonical `audit-ordering.json` fixture and `audit_ordering_fixture_orders_two_projects_as_one_newest_first_timeline` in `crates/core/tests/frontend_contract_v1.rs` |
| `lane-rail`, `project-picker`, `project-switch-confirm` | a two-project `RecentWorkResult` whose timestamps are offsets from the frozen clock, so the rendered ages are stable. The open root is included on purpose — the picker must drop it from Recent rather than offer a switch to the project already open | the `RecentWorkLoaded` payloads asserted in `tests/recent_work.rs` |

The shared D1 fixture's `topbarSource.project` now carries `viden` rather than
`null`. That is the name Core publishes, and it is what the titlebar selector,
the rail's `.wsroot` group header, and the picker's "In workspace" row all
render; the path-fallback path keeps its own coverage in
`tests/cockpit_topbar.spec.ts` and `tests/lane_rail.spec.ts`, which override it
to `null` explicitly.

## How to run it

Start the dev server from the worktree that owns `apps/gui/**`:

```bash
npm --prefix apps/gui run dev -- --port 4173 --strictPort
```

Then open each URL in the authorized Browser runtime at a 1440x900 viewport,
wait for `data-capture-ready`, and capture. This mirrors
`tools/capture-d1-visual.sh`: the procedure standardizes URLs and dimensions
and does not invoke browser automation outside that runtime.

All URLs share the prefix
`http://localhost:4173/evidence/main-window-interactions/qa.html`.

| State | URL | What the capture must show |
| --- | --- | --- |
| `d1` | `…/qa.html?state=d1` | the full cockpit; the titlebar project selector with its branch and dirty marker beside the `↑/↓` and worktree chips; all nine statusbar segments carrying a fact plus the `⏸ 2 awaiting you` decision chip; the three composer selector pills |
| `d1-mode-menu` | `…/qa.html?state=d1-mode-menu` | the work-mode popover open over the composer, with the current mode marked selected |
| `d1-model-menu` | `…/qa.html?state=d1-model-menu` | the model popover open, showing both the provider group and the adapter group Core published |
| `palette` | `…/qa.html?state=palette` | the ⌘K command palette open over the cockpit from the titlebar toggle, with all four sections visible — Actions, Jump to (the cross-Lane gate and ask plus the Lane), Settings, and the Files section listing the workspace inventory Core published |
| `palette-files` | `…/qa.html?state=palette-files` | the same palette pre-scoped to `~`, framing the Core-published inventory alone: six paths in Core's lexicographic order, each with the entry kind Core reported and no path the client discovered itself (`GUI-CORE-022`) |
| `d10-ticker` | `…/qa.html?state=d10-ticker` | the D10 event ticker under the lane cards: one bounded newest-first page of Core's audit timeline, with two projects interleaved so the strip shows one order across projects rather than a per-project list, each row carrying Core's stable id, raw dotted action key, owner, and timestamp (`GUI-CORE-014`) |
| `review` | `…/qa.html?state=review` | DiffReview opened from the titlebar changes marker: the file tree with its `M`/`A` glyphs, the per-row stage toggle carrying Core's own `staged` fact, per-file counts and the header total; the unified body with Git's own `@@` header and per-side line numbers; the page truncation banner; `Split` visible and disabled; the live commit bar with `Commit` and `Commit & Push` disabled because no message is typed yet |
| `review-commit` | `…/qa.html?state=review-commit` | the commit bar acting: a message typed through the production input listener, all three actions enabled, and the titlebar sync chip a live `Push` |
| `review-commit-pending-approval` | `…/qa.html?state=review-commit-pending-approval` | the `Ask` path — Core published a `git` approval for the acting owner, so the bar says the permission dock owns the decision and every control, including the sync chip, is inert |
| `review-commit-completed` | `…/qa.html?state=review-commit-completed` | the success line built from Core's *resampled* source (`↑2 ↓0`, working tree clean), with git's transcript collapsed behind `Git output` |
| `review-push-no-upstream` | `…/qa.html?state=review-push-no-upstream` | a `Failed { NoUpstream }` outcome: the localized sentence for the class, git's own detail verbatim below it, and the `Push and set upstream` recovery the contract names — never a `Pull` |
| `review-rejected-action` | `…/qa.html?state=review-rejected-action` | a pre-effect `CommandRejected` for the same bar, Core's words unedited in a `role=alert`, with no failure line and no recovery |
| `review-omitted` | `…/qa.html?state=review-omitted` | the same view with the second entry selected, so the "rows not shown" note appears beside counts that stayed real (`omitted`) |
| `review-rejected` | `…/qa.html?state=review-rejected` | Core's refusal verbatim in a `role=alert`, with no file count in the header and no empty-tree sentence |
| `review-empty` | `…/qa.html?state=review-empty` | "No changes in the working tree" — the only state drawn that way, over a page Core actually answered |
| `approval-hunks` | `…/qa.html?state=approval-hunks` | the D1 permission dock rendering `decision_context` as hunk rows under the "Preview computed against" note, with `input_preview` above and the decision row pinned below (`GUI-CORE-012`) |
| `centre-lane-tabs` | `…/qa.html?state=centre-lane-tabs` | the `.tabstrip.lanebar` Lane tab strip over the transcript: one tab per Lane with its own recorded branch and bound agent, the current tab marked, the trailing `＋`, and the meta slot carrying the project, Core's published budget and the resolved work mode once |
| `centre-tool-diff` | `…/qa.html?state=centre-tool-diff` | the transcript's `.tool` blocks: a workspace change with its header disclosed over the shared hunk rows, and a failed check run with the command, the status chip, and the three `.testrow`s including Core's failing location |
| `centre-focus-mode` | `…/qa.html?state=centre-focus-mode` | focus mode entered from the titlebar control: two grid tracks, the pinned Lane column given back, the context dock behind its right-edge hot zone, and the `IFocus` control pressed |
| `dock-environment` | `…/qa.html?state=dock-environment` | the context dock's Environment panel: the six-tab strip with Environment / Files / Diff live and Terminal / Code / Docs struck through and named (Code is struck here because this state binds no file read; `consumer-file-read` is the frame where Core publishes one and the tab opens), then Changes with Core's `+3 −1` and one row per changed file, Local saying which tree it shows, the Commit-or-push route, the PR-status absence, and the `.envctx` budget bar. The environment facts section is collapsed in the capture — the panel is taller than 900px with it open, and collapsing it is what the design's `.envhd` is for |
| `dock-files` | `…/qa.html?state=dock-files` | the Files tab with `apps/` opened: three directories that exist only because `apps/` was asked for as its own Core page, Core's truncation sentence where its page stopped, and the inspector on a selected file with Open disabled and `runtime.workspace_file_reads` written out on screen |
| `dock-diff` | `…/qa.html?state=dock-diff` | the Diff tab: one entry per changed file, the first expanded over the shared `diff_rows` body with Git's own `@@` header and per-side numbers, the second left collapsed with its real `+1284 −0`, and the inspector's File / Diff / "Open in review" beside a disabled Stage and Revert |
| `dock-unavailable` | `…/qa.html?state=dock-unavailable` | the same panel on a host with no diff read and no operator git bound: Changes says no host is bound, Commit or push is disabled with its reason **on screen** rather than only in a tooltip, and PR status keeps its own absence sentence. Four absences, four sentences |
| `d4-from-popover` | `…/qa.html?state=d4-from-popover` | the D4 wizard entered from "Full setup…": the Lane named after the popover's task, the agent step listing Core's adapters with the popover's pick marked, and the sentence stating that the create command carries no agent binding |
| `lane-rail` | `…/qa.html?state=lane-rail` | `D-SIDEBAR` **pinned** mode: the sidebar as a real layout column at the design's 218px, pushing the work surface rather than covering it, with the activity rail's pin marked pressed. It shows the one `.wsroot` project group named `viden` with its `▾` collapse, its Lane count, the per-group `＋`, the Lane nested beneath it, and the `＋ Add project…` footer — and no second group and no "Global" section |
| `project-picker` | `…/qa.html?state=project-picker` | the picker open under the titlebar `▾` selector with all three columns visible at once: `Add directory…` enabled beside the two disabled rows naming `GUI-CORE-023`, the single "In workspace" row for the open project with its lane count, and one Recent row with its relative age |
| `project-switch-confirm` | `…/qa.html?state=project-switch-confirm` | the same picker after choosing the recent project, showing the inline confirmation: the target root, the replacement sentence naming `GUI-CORE-023`, the running-work counts, and Cancel beside Switch workspace |
| `permission-ask` | `…/qa.html?state=permission-ask` | the permission dock as Core published it, before any verdict: the command, the typed facts row, and the five actions with `Always`/`Edit` disabled under `GUI-CORE-003` — the baseline the redirect state is read against |
| `permission-deny-redirect` | `…/qa.html?state=permission-deny-redirect` | the same screen immediately after Deny: the composer prompt reads "Tell Codex what to do instead…" and holds the caret. `sendPermission` never resolves in the harness, which is the point — the prompt switches when the deny is *dispatched* and claims nothing about Core's answer |
| `settings` | `…/qa.html?state=settings` | the Settings overlay open over the cockpit with an unsaved draft, framed from the panel's own top: the Provider & Models card listing the provider group and the adapter group Core published with the current model marked, the Permissions card with all five Core levels — UI label, Core CLI identifier, one-line description — the current level marked, and the read-only working-directory row; Cancel and Save enabled below |
| `settings-unavailable` | `…/qa.html?state=settings-unavailable` | the same overlay with the absent `ui.preference_persistence` capability named, Save disabled, and the language/appearance controls read-only — while the Provider and Permissions rows stay **operable**, because they ride `SelectModel`/`SetPermissionLevel` rather than the preference capability |
| `d6-actions` | `…/qa.html?state=d6-actions` | the recovery surface with Restart agent and Close Lane enabled, and the inspect facts expanded |
| `d6-error` | `…/qa.html?state=d6-error` | the same surface after a refused restart, with Core's rejection rendered as an alert |
| `d12-actions` | `…/qa.html?state=d12-actions` | the merge gate with Accept available and the bounce reason input filled and enabled |
| `d12-blocked` | `…/qa.html?state=d12-blocked` | the same gate with Accept unavailable, naming `missing_evidence`, and the reason input disabled |
| `d12-conflict-content` | `…/qa.html?state=d12-conflict-content` | the bounce's conflict pane: the "two sides plus the patch preimage — not a merge result" statement, the `Read against` line naming the gate's reviewed evidence with its binding chip, then per hunk a reason chip with its remedy sentence, OURS beside THEIRS each numbered from Core's own start, and the opened BASE strip. Below it the Lane apply conflict for the same Lane, with its `revision` baseline. No merged text and no resolve control anywhere (`GUI-CORE-015`) |
| `d12-conflict-omitted` | `…/qa.html?state=d12-conflict-omitted` | the bounded payload: the truncation banner over the files, one file rendered, and `assets/atlas.png` kept as an entry saying its hunks are not shown — never "no conflict" |
| `d12-conflict-none` | `…/qa.html?state=d12-conflict-none` | a bounce Core published no content for, with the capability advertised: the pane says Core published none for this bounce and that an operator bounce carries none by contract, and does **not** name the capability, which is the other absence |
| `d11` | `…/qa.html?state=d11` | the project intake screen with the probed project and the provider warning |
| `d11-recent` | `…/qa.html?state=d11-recent` | the same intake screen scrolled to its Recent work panel, showing the Core `QueryRecentWork` rows — name, relative age, session count, canonical root — instead of the retired static unavailability sentence |
| `d2-review-pending` | `…/qa.html?state=d2-review-pending` | the pending review selected with Accept review / Reject review enabled, the typed reviewer note, and the receipt saying the verdict was sent and Core has not recorded it yet |
| `d2-review-confirmed` | `…/qa.html?state=d2-review-confirmed` | the same review after Core recorded the verdict, coherent end to end: the queue row reads `accepted`, the command-bar count drops to 1, the audit sink names the decision's own audit id, both verdicts are disabled under `D2-REVIEW-SETTLED`, the note is cleared and disabled, and the confirmed receipt sits below |
| `d2-review-rejected` | `…/qa.html?state=d2-review-rejected` | the same review after Core refused the verdict, with Core's own sentence rendered verbatim as an alert and the note preserved |
| `d2-review-blocked` | `…/qa.html?state=d2-review-blocked` | both verdicts disabled and named by `D2-NO-REVIEWER-ACTOR`, the reason spelled out once below the bar, and the reviewer note disabled — never enabled-and-inert and never silently hidden |
| `d2` | `…/qa.html?state=d2` | the queue as D2 opens: all three groups with their counts, the first gate approval selected, the contract group labelled decided history under `GUI-CORE-013`, and the approval's own diff row unavailable under `GUI-CORE-012` |
| `d2-contract` | `…/qa.html?state=d2-contract` | the contract record selected: Confirm and Reject **visible and disabled**, each naming `GUI-CORE-013`, with the reason spelled out once below the bar. Core has already decided every contract it publishes and refuses a second decision on an id it holds, so a live verdict here could only produce a refusal |
| `d4` | `…/qa.html?state=d4` | the reviewed starter-Lane wizard on its review step: route, gate strength, execution target, budget, worktree, and base revision exactly as Core's preview published them, with no picker or editor over any of them |
| `d13` | `…/qa.html?state=d13` | the fleet board: one Core workflow DAG with a blocked node naming the dependency record Core wrote as its blocker, and the handoff strip saying Core recorded none |
| `d10-blind` | `…/qa.html?state=d10-blind` | the cost-blind terminal lane with its `cost-blind route` marker and the four bounded run facts (wall time, runs, applied diff, last exit), beside a metered ACP lane carrying none of them |
| `d10-blind-unobserved` | `…/qa.html?state=d10-blind-unobserved` | the same blind lane before Core observed any run: the marker plus the sentence saying no run was observed, and no zeroed facts |
| `d14-audit` | `…/qa.html?state=d14-audit` | the mode toggle with `Audit trail` pressed beside `Raw event replay (diagnostic)`; three audit rows newest-first, each showing Core's raw dotted `action` key, the actor (with `codex-acp` on the agent row), the outcome (`denied` visibly distinct from `success`), the linked object chips, the bounded argument chips, and a readable `YYYY-MM-DD HH:MM:SS UTC` time with the zone spelled out; the load-older control, because Core's page is incomplete |
| `d14-audit-scoped` | `…/qa.html?state=d14-audit-scoped` | the same trail as D12's revert row opens it: the removable `Scoped to revert · revert-1` chip in the header, which re-queries unscoped when removed |
| `d14-raw-fallback` | `…/qa.html?state=d14-raw-fallback` | a Core without `runtime.audit`: raw mode pressed, the audit button disabled, the note naming the capability, and the replay rows below with the undecodable row kept and highlighted |
| `evidence` | `…/qa.html?state=evidence` | EvidenceView opened from the palette's `Open evidence` row: the kind chips with `task_summary` grouped last rather than hidden, the scope-stating search box, the `Undated` group **first** and two local-day groups after it in Core's own ascending order, and the selected `patch` row's canonical bytes rendered through the shared diff rows (`GUI-CORE-025`) |
| `evidence-text` | `…/qa.html?state=evidence-text` | the same list with a `test_result` row selected and the detail rail scrolled to its content: the bounded text, the sentence saying Core's 256 KiB bound cut it and that this does not mean the evidence was short, the `Verified against <sha256>` line, the linked chips, and `Open in review` disabled because the row is not a `patch` |
| `evidence-summary-only` | `…/qa.html?state=evidence-summary-only` | the display-only `task_summary` row: a report with no canonical fields and the note saying the entry names no canonical bytes, and `Unavailable { SummaryOnly }` rendered as "display-only evidence — Core holds no canonical bytes for it" |
| `evidence-unavailable` | `…/qa.html?state=evidence-unavailable` | the opposite absence on the `patch` row: `Unavailable { HashMismatch }` as "canonical bytes failed verification — not shown", in the error colour, with no body anywhere on the screen |
| `evidence-empty` | `…/qa.html?state=evidence-empty` | "No evidence in this scope." — the only state drawn that way, over a page Core actually answered — with `complete` stated as "Archive complete" rather than left to the absence of a `Load older` button |
| `evidence-rejected` | `…/qa.html?state=evidence-rejected` | Core's over-limit `kinds` refusal verbatim in a `role=alert`, its `hint:` line kept on its own line, with nothing loaded, no paging foot, and no empty-archive sentence |
| `nav-d2-in-cockpit` | `…/qa.html?state=nav-d2-in-cockpit` | the decision queue as a **centre-pane view**: the cockpit chrome unchanged around it — titlebar with the source block, activity rail with `Decisions` marked `aria-current` and carrying Core's own pending count as its badge, context dock, composer still addressed to the selected Lane, statusbar with its decision chip — plus the view's own `AUDIT`-style head and the Close control in the position DiffReview puts its own |
| `nav-d14-in-cockpit` | `…/qa.html?state=nav-d14-in-cockpit` | the audit trail in the same shell, which is where `D-AUDIT`'s one-way link from an evidence row or a D12 baseline chip now lands: the rail's `Audit timeline` slot marked current, the mode toggle and the three audit rows below the view head, and the conversation still one `Esc` away |
| `nav-sidebar-floating-peek` | `…/qa.html?state=nav-sidebar-floating-peek` | `D-SIDEBAR` **floating** mode — the decision's default — with the peek open through the keyboard path (the Lanes rail slot): the sidebar as an **overlay above a full-width transcript** rather than a layout column, the 12px hot zone with its `.edgehint` cue against the activity rail, and the rail's pin (above the settings gear) reading unpinned |
| `nav-statusbar-config` | `…/qa.html?state=nav-statusbar-config` | the `D-STATUSBAR` config gear open: the `.sbcfg` popover listing the six ambient segments with their checkboxes, the footer sentence saying identity and actionable items are always pinned, and `MODE`, `PERM`, `LANE`, and the decision chip visible on the bar behind it while absent from the list |
| `d10-actions` | `…/qa.html?state=d10-actions` | the Lane monitor's card action row in the cockpit: **Attach / Stop / Pause / Kill on both cards**, with Stop live on the Lane Core published an Agent session for and disabled on the Lane it published none for, and Pause and Kill disabled on both naming the commands `RuntimeCommand` does not carry (G6) |
| `d13-drill` | `…/qa.html?state=d13-drill` | both Lane-binding answers on one column: the node Core bound to a Lane carrying its visible `Open Lane lane_core ↗` line and `role="button"`, and a copy of the same generated node with the binding removed saying Core bound no Lane and offering no click (G6) |
| `d14-filtered` | `…/qa.html?state=d14-filtered` | the audit trail with the `agent` actor chip engaged: the actor chips are the values **this page** carries (`All actors`, `operator`, `agent` — no `system`, because no row on the page is one), the four time chips with `All time` pressed, the note saying the cut is the loaded page and naming `GUI-CORE-024`, the rollup reading `Outcomes on the loaded page, filtered · 1 of 3 · denied 1`, one remaining row, and `Export` disabled (G6) |
| `consumer-workspace-commit` | `…/qa.html?state=consumer-workspace-commit` | `C5`'s three client-visible facts in one frame: the Lane tab strip printing that Lane's **own** worktree branch with its own `↑2 ↓1` from `lane_sources`, the dock's Local section saying it is showing the **Lane** worktree with that tree's numbers, and Commit-or-push enabled because Core published a workspace owner to act as (`GUI-CORE-027`). The Environment facts section is collapsed for the reason `dock-environment` collapses it (G7) |
| `consumer-turn-live` | `…/qa.html?state=consumer-turn-live` | `C6`: the Live Work strip reading Core's turn rather than display residue — the source named `queued` (the queue drained, which is a different explanation for text appearing than a typed turn), the elapsed clock reading `2:00` from Core's `started_at` rather than `0:00` from this webview's mount, and the queue line saying `1 queued · runs after the current turn` (G7) |
| `consumer-evidence-patch` | `…/qa.html?state=consumer-evidence-patch` | `C7`: the archived `patch` row the runtime now records for a native tool edit, framed on its content — `verification: verified`, `canonical quality: pass`, Core's parsed diff rows for the hunk, the hash the bytes were verified against, and the canonical item in `LINKED`. The kind chips are deliberately untouched: Core applies `kinds` before it cuts the page, so pressing one while the harness answers a fixed page would show a filter the rows do not match (G7) |
| `consumer-transcript-rows` | `…/qa.html?state=consumer-transcript-rows` | `C8`: Core's ordered rows in the D1 transcript, scrolled to the top of the region so one frame carries the `Load older` control Core's cursor earned, the user row, the assistant row with `Over Core's 8 KiB row bound: this body is cut, not short.` and its `Open evidence` affordance, the `edit_file` tool call, its result with the same affordance, and the check run — with the permission row and its `Open audit trail` below the fold. The `transcript_user` / `transcript_assistant` unavailable rows are gone (`GUI-CORE-009`) (G7) |
| `consumer-file-read` | `…/qa.html?state=consumer-file-read` | `C9`: the dock inspector's Open, disabled in `dock-files`, sending one `ReadWorkspaceFile` — the tree with `AGENTS.md` selected, `lane-core worktree · 12288 bytes on disk · sha256 6d5e36fb44a9…` (the **whole** file's length and hash), the sentence saying Core's bound cut the body, the monospace body itself, and the `Code` tab no longer struck through in the strip (G7) |

`mode=dark|light` and `locale=en|zh-CN` are accepted on every state and resolve
through the shared `resolveTheme` path, so the harness never ships a second
palette. `mode=light` also selects the `ice` skin, matching how the design
pairs them. Suggested locale and skin proof:
`…/qa.html?state=d1&mode=light&locale=zh-CN`.

## Captured screenshots

Captured 2026-08-21 with headless Chrome
(`--headless --window-size=1440,900 --virtual-time-budget=6000`) against the
vite dev server on port 4173, then visually reviewed (6 of 11 sampled in
review; all 11 DOM-verified at build time).

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [d1-1440x900-dark-en.png](d1-1440x900-dark-en.png) | d1 | 1440x900 | dark | en |
| [d1-mode-menu-1440x900-dark-en.png](d1-mode-menu-1440x900-dark-en.png) | d1-mode-menu | 1440x900 | dark | en |
| [d1-model-menu-1440x900-dark-en.png](d1-model-menu-1440x900-dark-en.png) | d1-model-menu | 1440x900 | dark | en |
| [settings-1440x900-dark-en.png](settings-1440x900-dark-en.png) | settings | 1440x900 | dark | en |
| [settings-unavailable-1440x900-dark-en.png](settings-unavailable-1440x900-dark-en.png) | settings-unavailable | 1440x900 | dark | en |
| [d6-actions-1440x900-dark-en.png](d6-actions-1440x900-dark-en.png) | d6-actions | 1440x900 | dark | en |
| [d6-error-1440x900-dark-en.png](d6-error-1440x900-dark-en.png) | d6-error | 1440x900 | dark | en |
| [d12-actions-1440x900-dark-en.png](d12-actions-1440x900-dark-en.png) | d12-actions | 1440x900 | dark | en |
| [d12-blocked-1440x900-dark-en.png](d12-blocked-1440x900-dark-en.png) | d12-blocked | 1440x900 | dark | en |
| [d11-1440x900-dark-en.png](d11-1440x900-dark-en.png) | d11 | 1440x900 | dark | en |
| [d11-recent-1440x900-dark-en.png](d11-recent-1440x900-dark-en.png) | d11-recent | 1440x900 | dark | en |
| [d1-1440x900-light-zh-CN.png](d1-1440x900-light-zh-CN.png) | d1 | 1440x900 | light | zh-CN |

The eight topbar-bearing captures (`d1*`, `settings*`, `d6-*`) were recaptured
on 2026-08-21 after the titlebar git block landed and show the project
selector with its branch, the dirty dot, and both `.gitops` chips
(`↑1 ↓0`, `⎇ 1 worktree`); the standalone `d12*` screens carry no
cockpit titlebar, so their earlier captures remain valid. `d11-recent` was
first captured on 2026-08-29, when the history panel began rendering Core's
recent-work rows instead of the retired `GUI-CORE-007` sentence; a same-day
recapture of `d11` came out byte-identical because the panel sits below that
viewport's fold.

The command palette added a `.tbtbtn` toggle to `.tbtools`; the eight
topbar-bearing images plus the new `palette` state were recaptured on
2026-08-21 at 1440x900 and visually reviewed (the palette capture shows the
prefix legend, all four sections, kbd hints, and the disabled Files row).

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [palette-1440x900-dark-en.png](palette-1440x900-dark-en.png) | palette | 1440x900 | dark | en |

## Project picker and grouped rail captures

The nine topbar-bearing images (the titlebar selector gained the design's `▾`
and button chrome; the rail gained its `.wsroot` group header and
`＋ Add project…` footer) plus the three new states were recaptured on
2026-08-21 at 1440x900 and visually reviewed (all three picker columns with
both `GUI-CORE-023` disabled rows, the current-project and recent rows, and
the switch confirmation naming the replacement semantics and impact counts).

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [project-picker-1440x900-dark-en.png](project-picker-1440x900-dark-en.png) | project-picker | 1440x900 | dark | en |
| [lane-rail-1440x900-dark-en.png](lane-rail-1440x900-dark-en.png) | lane-rail | 1440x900 | dark | en |
| [project-switch-confirm-1440x900-dark-en.png](project-switch-confirm-1440x900-dark-en.png) | project-switch-confirm | 1440x900 | dark | en |

## Supervision-debt captures

Captured 2026-08-30 with headless Chrome
(`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`) against the vite dev server on port 4173, then
visually reviewed (all eight sampled in review).

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [d2-review-pending-1440x900-dark-en.png](d2-review-pending-1440x900-dark-en.png) | d2-review-pending | 1440x900 | dark | en |
| [d2-review-confirmed-1440x900-dark-en.png](d2-review-confirmed-1440x900-dark-en.png) | d2-review-confirmed | 1440x900 | dark | en |
| [d2-review-rejected-1440x900-dark-en.png](d2-review-rejected-1440x900-dark-en.png) | d2-review-rejected | 1440x900 | dark | en |
| [d2-review-blocked-1440x900-dark-en.png](d2-review-blocked-1440x900-dark-en.png) | d2-review-blocked | 1440x900 | dark | en |
| [d10-blind-1440x900-dark-en.png](d10-blind-1440x900-dark-en.png) | d10-blind | 1440x900 | dark | en |
| [d10-blind-unobserved-1440x900-dark-en.png](d10-blind-unobserved-1440x900-dark-en.png) | d10-blind-unobserved | 1440x900 | dark | en |
| [d2-review-confirmed-1440x900-light-zh-CN.png](d2-review-confirmed-1440x900-light-zh-CN.png) | d2-review-confirmed | 1440x900 | light | zh-CN |
| [d10-blind-1440x900-light-zh-CN.png](d10-blind-1440x900-light-zh-CN.png) | d10-blind | 1440x900 | light | zh-CN |

Both `d2-review-confirmed` captures were retaken on 2026-08-30 after the
harness began serving the generated decided projection for that outcome; the
earlier pair showed a confirmed receipt over a still-pending row, which is not a
state production can reach.

The two light/`zh-CN` captures are the locale and skin proof for both screens:
every added label — the reviewer-note caption, the three outcome sentences, the
cost-blind marker, and the four run-fact names — is translated, and neither
screen ships a palette of its own.

## Known limitation

The authorized Browser runtime renders and verifies these pages but cannot
write PNG files, so this directory holds the reproducible harness rather than
committed images until an operator captures them. `tools/capture-d1-visual.sh`
deliberately stops at the same boundary. The images listed above were captured
by an operator running headless Chrome directly with the flags recorded beside
each table.

## D14 audit captures

Captured 2026-08-31 with headless Chrome
(`--headless --disable-gpu --window-size=1440,900 --virtual-time-budget=6000`)
against the vite dev server on port 4177, then visually reviewed (all four
sampled in review).

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [d14-audit-1440x900-dark-en.png](d14-audit-1440x900-dark-en.png) | d14-audit | 1440x900 | dark | en |
| [d14-audit-scoped-1440x900-dark-en.png](d14-audit-scoped-1440x900-dark-en.png) | d14-audit-scoped | 1440x900 | dark | en |
| [d14-raw-fallback-1440x900-dark-en.png](d14-raw-fallback-1440x900-dark-en.png) | d14-raw-fallback | 1440x900 | dark | en |
| [d14-audit-1440x900-light-zh-CN.png](d14-audit-1440x900-light-zh-CN.png) | d14-audit | 1440x900 | light | zh-CN |

The light/`zh-CN` capture is the locale and skin proof: the mode toggle, the
screen title, the load-older control, and the scope-chip label are translated,
while every Core value in a row — the dotted `action` key, the object kinds and
ids, and the argument keys and values — stays exactly as Core published it.
That is the intended split: localizing the action vocabulary would destroy the
property that makes two audit timelines diffable.

The row timestamp sits on the same side of that split. It is rendered readable
(`2023-11-14 22:28:20 UTC`) rather than as the raw epoch second, but the format
is fixed and the zone is UTC in both locales: an audit record is evidence
compared across machines, so a locale-shifted clock would let two readers
disagree about one fact. The ISO value stays on the `<time datetime>` attribute
for machine reading. Raw replay mode deliberately keeps its epoch readout — its
`d14-raw-fallback` capture came back byte-identical after this change, which is
the proof that the diagnostic mode's presentation was not touched.

## Workspace inventory and event ticker captures

Captured 2026-09-02 with headless Chrome
(`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`) against the vite dev server on port 4188, then
visually reviewed (all four sampled in review).

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [palette-files-1440x900-dark-en.png](palette-files-1440x900-dark-en.png) | palette-files | 1440x900 | dark | en |
| [palette-files-1440x900-light-zh-CN.png](palette-files-1440x900-light-zh-CN.png) | palette-files | 1440x900 | light | zh-CN |
| [d10-ticker-1440x900-dark-en.png](d10-ticker-1440x900-dark-en.png) | d10-ticker | 1440x900 | dark | en |
| [d10-ticker-1440x900-light-zh-CN.png](d10-ticker-1440x900-light-zh-CN.png) | d10-ticker | 1440x900 | light | zh-CN |

These close `GUI-CORE-022` and `GUI-CORE-014` visually. The `palette-files`
captures replace the permanently disabled Files row with the inventory Core
published, in Core's own lexicographic order and with the entry kind Core
reported beside each path; no path in either image was discovered by the
client. The `d10-ticker` captures replace the `d10.events.noOrderedLog`
unavailable note with the audit timeline, and their two interleaved projects
are the visual form of the property `audit-ordering.json` proves.

The light/`zh-CN` captures are the locale split for both surfaces, and it is
the same split D14 already documents: the section headings, the entry-kind
column, and the ticker heading translate, while every Core value stays exactly
as Core published it — the workspace-relative paths, and the ticker's stable
audit ids and dotted `action` keys. Localizing an action vocabulary would
destroy the property that makes two timelines diffable, and localizing a path
would make it un-openable.

## Settings Core-command section captures

Captured 2026-09-07 with headless Chrome
(`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`) against the vite dev server on port 4199, then
visually reviewed (all three sampled in review).

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [settings-1440x900-dark-en.png](settings-1440x900-dark-en.png) | settings | 1440x900 | dark | en |
| [settings-unavailable-1440x900-dark-en.png](settings-unavailable-1440x900-dark-en.png) | settings-unavailable | 1440x900 | dark | en |
| [settings-1440x900-light-zh-CN.png](settings-1440x900-light-zh-CN.png) | settings | 1440x900 | light | zh-CN |

Both dark captures were retaken after the overlay gained its Provider & Models
and Permissions sections. Two things changed alongside them and are visible in
the images. The harness now anchors the `settings` capture to the panel's own
scroll top, because the remount after a draft focuses the drafted option and
would otherwise scroll the two new cards out of frame. And the panel's
`max-height` now subtracts its own bottom offset: the gear anchors the popover
near the bottom of the rail, so a panel sized against the full viewport height
started above the viewport top once it grew this tall, putting its heading and
close button out of reach.

`settings-unavailable` is the proof of the capability split. The language and
appearance controls are read-only under the named absent
`ui.preference_persistence`, while every Provider and Permissions row stays
operable in the same image — they ride `SelectModel` and `SetPermissionLevel`,
which that capability does not gate.

The light/`zh-CN` capture is the locale and skin proof for the added copy, and
it is the same split the audit timeline already documents: the section
headings, the level labels, the descriptions, and the working-directory caption
translate, while every Core value stays exactly as Core published it — the five
CLI identifiers (`ask`, `auto_edit`, `auto`, `read_only`, `full_access`), the
model names, the provider group labels, and the workspace root. Localizing a
CLI identifier would break the one property the chip exists for: matching this
panel against a Core log.
\n
## Permission deny-redirect captures

Captured 2026-09-07 with headless Chrome
(`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`) against the vite dev server on port 4199, then
visually reviewed (all three sampled in review).

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [permission-ask-1440x900-dark-en.png](permission-ask-1440x900-dark-en.png) | permission-ask | 1440x900 | dark | en |
| [permission-deny-redirect-1440x900-dark-en.png](permission-deny-redirect-1440x900-dark-en.png) | permission-deny-redirect | 1440x900 | dark | en |
| [permission-deny-redirect-1440x900-light-zh-CN.png](permission-deny-redirect-1440x900-light-zh-CN.png) | permission-deny-redirect | 1440x900 | light | zh-CN |

These are the design's post-deny composer (`Viden - 桌面驾驶舱 (GUI).html`,
the composer beside the permission dock). The pair is worth reading together:
the same screen before and after one click, with only the prompt and the caret
different. Nothing else moves, because nothing else may — the redirect is
presentation, `feedback` stays `null` (GUI-CORE-019), and the dock still shows
the ask because the harness's `sendPermission` never answers.

The agent name is Core's published adapter `displayName`, which is why the
harness's pending projection carries an ACP session under the exact session id
Core's `laneAgent` fact already names: with no ACP conversation the prompt
falls back to the generic wording rather than guessing a name from an agent id.
Both paths are covered in `tests/permission_deny_redirect.spec.ts`.

The light/`zh-CN` capture is the locale and skin proof: the prompt translates
to 告诉 Codex 改做什么…, while `Codex` itself stays exactly as Core published
it. Translating an adapter's display name would make the redirect name an
agent the operator cannot find anywhere else in the cockpit.

## Activity-rail destination recapture

The rail's five routing slots were relabelled with the screens they actually
open and re-glyphed from the registered `GUI/gui-icons.jsx` set, so every
capture that shows the cockpit shell shows a different rail. All eighteen
cockpit-bearing images were recaptured 2026-09-07 with headless Chrome
(`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`) against the vite dev server on port 4199, then
visually reviewed (6 of 18 sampled in review; all 18 DOM-verified at build
time by `tests/activity_rail_destinations.spec.ts`).

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [d1-1440x900-dark-en.png](d1-1440x900-dark-en.png) | d1 | 1440x900 | dark | en |
| [d1-1440x900-light-zh-CN.png](d1-1440x900-light-zh-CN.png) | d1 | 1440x900 | light | zh-CN |
| [d1-mode-menu-1440x900-dark-en.png](d1-mode-menu-1440x900-dark-en.png) | d1-mode-menu | 1440x900 | dark | en |
| [d1-model-menu-1440x900-dark-en.png](d1-model-menu-1440x900-dark-en.png) | d1-model-menu | 1440x900 | dark | en |
| [palette-1440x900-dark-en.png](palette-1440x900-dark-en.png) | palette | 1440x900 | dark | en |
| [palette-files-1440x900-dark-en.png](palette-files-1440x900-dark-en.png) | palette-files | 1440x900 | dark | en |
| [palette-files-1440x900-light-zh-CN.png](palette-files-1440x900-light-zh-CN.png) | palette-files | 1440x900 | light | zh-CN |
| [lane-rail-1440x900-dark-en.png](lane-rail-1440x900-dark-en.png) | lane-rail | 1440x900 | dark | en |
| [project-picker-1440x900-dark-en.png](project-picker-1440x900-dark-en.png) | project-picker | 1440x900 | dark | en |
| [project-switch-confirm-1440x900-dark-en.png](project-switch-confirm-1440x900-dark-en.png) | project-switch-confirm | 1440x900 | dark | en |
| [settings-1440x900-dark-en.png](settings-1440x900-dark-en.png) | settings | 1440x900 | dark | en |
| [settings-1440x900-light-zh-CN.png](settings-1440x900-light-zh-CN.png) | settings | 1440x900 | light | zh-CN |
| [settings-unavailable-1440x900-dark-en.png](settings-unavailable-1440x900-dark-en.png) | settings-unavailable | 1440x900 | dark | en |
| [d6-actions-1440x900-dark-en.png](d6-actions-1440x900-dark-en.png) | d6-actions | 1440x900 | dark | en |
| [d6-error-1440x900-dark-en.png](d6-error-1440x900-dark-en.png) | d6-error | 1440x900 | dark | en |
| [permission-ask-1440x900-dark-en.png](permission-ask-1440x900-dark-en.png) | permission-ask | 1440x900 | dark | en |
| [permission-deny-redirect-1440x900-dark-en.png](permission-deny-redirect-1440x900-dark-en.png) | permission-deny-redirect | 1440x900 | dark | en |
| [permission-deny-redirect-1440x900-light-zh-CN.png](permission-deny-redirect-1440x900-light-zh-CN.png) | permission-deny-redirect | 1440x900 | light | zh-CN |

Two glyphs changed shape in every one of them: the fourth slot is now the
`decide` checkmark for the decision queue rather than the `review` book, and
the seventh is the `fleet` node graph for the fleet board rather than the
`inbox` tray. The labels themselves are tooltip and accessible-name text, so
they do not appear in a still capture — the coherence between each slot's route
and its name is asserted in `tests/activity_rail_destinations.spec.ts` instead,
which is the right place for it.

## `0.3.3` H1 hygiene captures

The `0.3.3` design-gap review found four states the harness could render but
had never captured, and two captures that had gone stale. Captured 2026-09-09
with headless Chrome (`--headless --disable-gpu --hide-scrollbars
--window-size=1440,900 --virtual-time-budget=6000`) against the vite dev server
on port 4173, then visually reviewed (all eight sampled in review).

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [d2-1440x900-dark-en.png](d2-1440x900-dark-en.png) | d2 | 1440x900 | dark | en |
| [d2-contract-1440x900-dark-en.png](d2-contract-1440x900-dark-en.png) | d2-contract | 1440x900 | dark | en |
| [d2-contract-1440x900-light-zh-CN.png](d2-contract-1440x900-light-zh-CN.png) | d2-contract | 1440x900 | light | zh-CN |
| [d4-1440x900-dark-en.png](d4-1440x900-dark-en.png) | d4 | 1440x900 | dark | en |
| [d13-1440x900-dark-en.png](d13-1440x900-dark-en.png) | d13 | 1440x900 | dark | en |
| [d10-blind-1440x900-dark-en.png](d10-blind-1440x900-dark-en.png) | d10-blind | 1440x900 | dark | en |
| [d10-blind-1440x900-light-zh-CN.png](d10-blind-1440x900-light-zh-CN.png) | d10-blind | 1440x900 | light | zh-CN |
| [d10-blind-unobserved-1440x900-dark-en.png](d10-blind-unobserved-1440x900-dark-en.png) | d10-blind-unobserved | 1440x900 | dark | en |

`d2`, `d2-contract`, `d4`, and `d13` are new harness states. `d2` and `d13`
render generated projections (`d2.json`, `d13.json`) unchanged; `d2-contract`
is one delta on `d2.json` — the contract record selected, with the availability
`tests/d2_decisions.rs` asserts for it; `d4` is hand-written from
`tests/d4_lane_create.spec.ts`, because D4's reviewed preview lives in the
adapter's own slot rather than in `RuntimeViewState` and
`tests/capture_projections.rs` has nothing to project it from.

The two `d10-blind*` images are **recaptures**, and the reason is the point:
the committed pair still carried the footer *"Core publishes no ordered event
log in the view state, so the event stream is unavailable · GUI-CORE-014"*.
That request closed when Core published the audit timeline, `d10.json` now
carries `unavailable: []`, and the strip reads "Core publishes no audit
timeline, so the event stream is unavailable" — because these two states apply
no audit page, which `d10-ticker` does. The old images were evidence of a
screen that no longer exists.

The `d2-contract` pair is the bilingual proof for the same change: the English
capture reads "Core already recorded this contract's decision, and publishes no
pending contract to confirm.", the Chinese one
"Core 已记录该契约的裁决，且不发布任何待确认契约。", and the code
`GUI-CORE-013` stays untranslated in both, because a reason code is an
identifier rather than prose.

## DiffReview and approval-hunk captures

Captured 2026-09-09 with headless Chrome
(`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`) against the vite dev server on port 4211, then
visually reviewed (all six sampled in review).

The five `review*` images were recaptured on 2026-09-09 against port 4173 after
the commit bar became live: the message box is a real `<input>`, each file row
carries its stage/unstage toggle, and the disabled `Commit` drops its solid
green fill, because a solid primary at 55% opacity still reads as "press me".

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [review-1440x900-dark-en.png](review-1440x900-dark-en.png) | review | 1440x900 | dark | en |
| [review-1440x900-light-zh-CN.png](review-1440x900-light-zh-CN.png) | review | 1440x900 | light | zh-CN |
| [review-omitted-1440x900-dark-en.png](review-omitted-1440x900-dark-en.png) | review-omitted | 1440x900 | dark | en |
| [review-rejected-1440x900-dark-en.png](review-rejected-1440x900-dark-en.png) | review-rejected | 1440x900 | dark | en |
| [review-empty-1440x900-dark-en.png](review-empty-1440x900-dark-en.png) | review-empty | 1440x900 | dark | en |
| [approval-hunks-1440x900-dark-en.png](approval-hunks-1440x900-dark-en.png) | approval-hunks | 1440x900 | dark | en |

These are the registered DiffReview family (`docs/DESIGN-REF.md`, "D1 次级视图",
`D-RAILNAV ①`) and the D1 permission dock rendering Core's decision context.
All five review states are opened the way an operator opens them — through the
titlebar's changes marker — rather than by mounting the screen directly, so the
entry point is under test in every image.

The set is chosen so that each of the honesty rules the source-control contract
states has exactly one image that can falsify it:

| Image | The rule it proves |
| --- | --- |
| `review` | a `truncated` page carries its banner; the file tree shows the `M`/`A` glyphs, Core's own staged `✓`, and per-file counts; the commit bar is fully disabled and names `GUI-CORE-020`; `Split` is visible and disabled |
| `review-omitted` | the second entry selected, so its "Rows not shown (1284 additions, 0 deletions) — over the byte bound" note is framed beside counts that stayed real. This is the one rule a screenshot of the first file cannot show |
| `review-rejected` | Core's refusal rendered verbatim, hint included, with **no** file count in the header — "0 files · +0 −0" would be a number Core never gave, and would read as a clean tree |
| `review-empty` | the only state drawn as "No changes in the working tree": a page Core answered, with zero entries |
| `approval-hunks` | the dock rendering `decision_context` as hunk rows under "Preview computed against 3f79bb7b", with `input_preview` still above them and the decision row pinned below them |

`review-rejected` and `review-empty` are worth reading as a pair: the two
images differ in exactly the way the contract says they must. One shows Core's
own words and no counts, the other shows a count of zero and the empty-tree
sentence. A client that collapsed them would produce the same picture twice.

The light/`zh-CN` capture is the locale and skin proof for the added copy: the
headings, the segmented control, the truncation banner, the commit-bar note,
and the counts translate, while `crates/types/src/diff.rs`, the `@@` header,
the diff content, the branch name, and the code `GUI-CORE-020` stay exactly as
they are. A translated `@@` header or reason code would break the one property
a diff pane exists for — matching what a reviewer sees in a terminal.

Two things the images make visible that are worth stating rather than leaving
to the eye. The dock's action row is pinned and its context scrolls, which is a
change this batch made: before Core published a decision context the dock
always fitted its host, and letting an unbounded preview scroll the whole dock
would push Approve and Deny out of sight behind the very rows they answer
(`permission-ask` is unchanged and is the before image). And a basename longer
than the 236px file tree still ellipsizes; the split only guarantees that the
directory half disappears first, and the full path stays in the row's title and
in the pane header.

## DiffReview action captures

Captured 2026-09-09 with headless Chrome
(`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`) against the vite dev server on port 4173, then
visually reviewed (all six sampled in review).

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [review-commit-1440x900-dark-en.png](review-commit-1440x900-dark-en.png) | review-commit | 1440x900 | dark | en |
| [review-commit-pending-approval-1440x900-dark-en.png](review-commit-pending-approval-1440x900-dark-en.png) | review-commit-pending-approval | 1440x900 | dark | en |
| [review-commit-completed-1440x900-dark-en.png](review-commit-completed-1440x900-dark-en.png) | review-commit-completed | 1440x900 | dark | en |
| [review-push-no-upstream-1440x900-dark-en.png](review-push-no-upstream-1440x900-dark-en.png) | review-push-no-upstream | 1440x900 | dark | en |
| [review-rejected-action-1440x900-dark-en.png](review-rejected-action-1440x900-dark-en.png) | review-rejected-action | 1440x900 | dark | en |
| [review-commit-completed-1440x900-light-zh-CN.png](review-commit-completed-1440x900-light-zh-CN.png) | review-commit-completed | 1440x900 | light | zh-CN |

This is the action half of the same registered family
(`runtime.operator_git`, GUI-CORE-020). Each image is chosen so exactly one
contract rule can falsify it:

| Image | The rule it proves |
| --- | --- |
| `review-commit` | the bar acts: a typed message enables `Commit` and `Commit & Push`, `Stage all` needs no message, every file row carries its stage toggle, and the titlebar chip is a live `Push` |
| `review-commit-pending-approval` | an `Ask` belongs to the permission dock. The bar names the action it is waiting on and every control — including the sync chip — is disabled rather than hidden |
| `review-commit-completed` | the success line is built from the outcome's resampled `source` (`↑2 ↓0`, working tree clean) and **not** from git's transcript, which is collapsed behind `Git output` |
| `review-push-no-upstream` | a failure *after* the gate is an outcome, not a denial: the localized sentence for Core's `NoUpstream` class, git's own detail verbatim, and the one recovery the contract names |
| `review-rejected-action` | a refusal *before* the gate is Core's own reason, verbatim, in a `role=alert` — with no failure sentence and no recovery beside it |

`review-push-no-upstream` and `review-rejected-action` are the pair worth
reading together, and they are the reason both exist. Both are red; neither is
the other. One says git ran and rejected the push, and offers the next git
command. The other says nothing ran and names the `viden.toml` rule that
stopped it. A client that drew them alike would send an operator to their
permission file about a problem in their own branch.

Two absences are deliberate and visible. No image offers a `Pull`: the contract
excludes it for `0.3.3` because it moves `HEAD` and can create conflicts that
belong to the Lane conflict machinery, and every sync tooltip says so in both
languages. And `review-push-no-upstream` offers `Push and set upstream` rather
than a bare retry, because Core refuses an untracked push outright — after
`git push <remote> <branch>` succeeded, ahead/behind would be unknowable and
the chip would read "in sync" forever.

The light/`zh-CN` capture is the locale and skin proof for the action copy: the
completion sentence, the working-tree state, the `Git 输出` disclosure, and all
three button labels translate, while the branch name, the resampled counts, and
git's own output stay exactly as Core published them.
## D12 structured conflict content

Captured 2026-09-09 with the same headless Chrome procedure
(`--headless --window-size=1440,900 --virtual-time-budget=6000`) against the
vite dev server on port 4173, then visually reviewed. These are the first
images of `runtime.conflict_content` (GUI-CORE-015) in the client.

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [d12-conflict-content-1440x900-dark-en.png](d12-conflict-content-1440x900-dark-en.png) | d12-conflict-content | 1440x900 | dark | en |
| [d12-conflict-omitted-1440x900-dark-en.png](d12-conflict-omitted-1440x900-dark-en.png) | d12-conflict-omitted | 1440x900 | dark | en |
| [d12-conflict-none-1440x900-dark-en.png](d12-conflict-none-1440x900-dark-en.png) | d12-conflict-none | 1440x900 | dark | en |
| [d12-conflict-content-1440x900-light-zh-CN.png](d12-conflict-content-1440x900-light-zh-CN.png) | d12-conflict-content | 1440x900 | light | zh-CN |

Each image exists to make one honesty rule falsifiable:

| Image | The rule it proves |
| --- | --- |
| `d12-conflict-content` | the pane draws **two sides plus the patch preimage and never a merge result**: OURS and THEIRS side by side at Core's own `ours_start` / `theirs_start`, the preimage as its own labelled third strip, the statement spelled out above the rows, and no merged text and no resolve control on the screen at all. Both hunks carry their own reason chip and remedy, so a reader can see that the classification is per hunk rather than per file. The baseline is the gate's reviewed evidence with its binding chip, which is the merge path's answer — not a bare commit |
| `d12-conflict-omitted` | `omitted` and `truncated` stay visible: the banner sits over the file list and `assets/atlas.png` keeps its entry saying its hunks are not shown. A reviewer must always be able to tell "not shown" from "this file was fine", and the same image carries a `revision` baseline so both baseline kinds appear in the set |
| `d12-conflict-none` | the two absences are different sentences. Here the capability *is* advertised and the record simply carries no content, so the pane says Core published none for this bounce and names the operator-bounce contract — it does not name `runtime.conflict_content`, which is what the screen's unavailable row says when the capability itself is missing |

The light/`zh-CN` capture is the locale proof for the added copy. The section
headings, the not-a-merge statement, the baseline line, the reason chips and
their remedies, and the OURS/THEIRS/BASE labels translate; the file path, the
conflicting source lines, the evidence id, the source hash, and Core's own
Lane and gate ids stay exactly as Core published them. A translated source line
would break the one property a conflict pane exists for.

The reason vocabulary is Core's `ConflictHunkReason`, rendered from the exact
discriminant so a reason this build does not name reaches the screen raw
instead of borrowing a known one:

| `ConflictHunkReason` | Label | What the operator can do |
| --- | --- | --- |
| `context_mismatch` | Context mismatch | the origin Lane must re-derive its patch against the current content; nothing can be applied here |
| `already_applied` | Already applied | the file already holds the hunk's new side at that range, so there is nothing left to apply |
| `file_missing` | File missing | the patch changes a file that is not in the target tree — a file-level decision |
| `file_deleted` | File would survive deletion | the deletion's preimage does not cover the whole file, so it would be left behind — a file-level decision |
| `binary` | Binary | Core reported non-text content, so there are no lines to match |
| anything else | `Reason <tag>` | shown exactly as Core published it |

## EvidenceView archive captures

Captured 2026-09-10 with the same headless Chrome procedure
(`--headless --window-size=1440,900 --virtual-time-budget=6000`) against the
vite dev server on port 4173, then visually reviewed. These are the first
images of `runtime.evidence_reads` (GUI-CORE-025) in the client.

The rows and content answers are inline fixtures in `qa.ts` rather than a
generated projection: this capability's projections are query answers, not
runtime facts, so `RuntimeProjection` never holds one and
`tests/capture_projections.rs` has nothing to emit for it. The rows are written
in Core's own ascending `(timestamp, id)` order with the undated row first,
which is the order Core would have delivered them in; nothing in the client
sorts.

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [evidence-1440x900-dark-en.png](evidence-1440x900-dark-en.png) | evidence | 1440x900 | dark | en |
| [evidence-text-1440x900-dark-en.png](evidence-text-1440x900-dark-en.png) | evidence-text | 1440x900 | dark | en |
| [evidence-summary-only-1440x900-dark-en.png](evidence-summary-only-1440x900-dark-en.png) | evidence-summary-only | 1440x900 | dark | en |
| [evidence-unavailable-1440x900-dark-en.png](evidence-unavailable-1440x900-dark-en.png) | evidence-unavailable | 1440x900 | dark | en |
| [evidence-empty-1440x900-dark-en.png](evidence-empty-1440x900-dark-en.png) | evidence-empty | 1440x900 | dark | en |
| [evidence-rejected-1440x900-dark-en.png](evidence-rejected-1440x900-dark-en.png) | evidence-rejected | 1440x900 | dark | en |
| [evidence-1440x900-light-zh-CN.png](evidence-1440x900-light-zh-CN.png) | evidence | 1440x900 | light | zh-CN |

Each image exists to make one honesty rule falsifiable:

| Image | The rule it proves |
| --- | --- |
| `evidence` | this is the **archive**, not `latest_evidence`. The `Undated` group leads because that is where Core's ordering puts it, the two dated groups follow ascending, and the `task_summary` chip sits after the five first-class kinds instead of being dropped — a chip bar that hid a kind would hide rows the archive holds. `Load older` is offered because Core's page is incomplete, and the search box's own label says it filters the loaded rows because Core publishes no evidence search |
| `evidence-text` | a bound is stated, never implied: the truncation sentence spells out that Core's 256 KiB bound cut the content and that this does **not** mean the evidence was short, and the `Verified against <sha256>` line lets a reader join what is on screen to the row's canonical reference. `Open in review` is visible and disabled on a non-`patch` row rather than hidden |
| `evidence-summary-only` | "no canonical reference" is its own fact. The report carries no canonical item, bundle, hash, or producer and says the entry names no canonical bytes, and the content block renders `Unavailable { SummaryOnly }` as a sentence rather than an empty body |
| `evidence-unavailable` | `HashMismatch` is the **opposite** fact from `SummaryOnly`, and the pair of captures proves the two are not folded together. Here Core has the bytes and refuses to serve them, so the pane says verification failed and shows no body at all — the one thing a reviewer must never be shown is content that failed its own hash |
| `evidence-empty` | four absences stay four sentences. This is the only one drawn as "no evidence in this scope", and it is drawn over a page Core actually answered; `complete` is stated in words rather than left to a missing button |
| `evidence-rejected` | a refusal is never an empty page. Core's own reason is rendered unedited in a `role=alert`, its `hint:` line preserved on its own line, and the list stays unloaded — no rows, no paging foot, and no empty-archive sentence |

The light/`zh-CN` capture is the locale proof for the added copy. The kind
chips, the `Undated` label, the report field names, the metadata note, the
content sentences, and both footer actions translate; the evidence ids, the
Core-published summaries, the paths, the source hash, the producer, the raw
`task_summary` kind Core has no localized name for, and the `YYYY-MM-DD` day
keys stay exactly as Core published them.

One determinism caveat specific to this family: day grouping and the row time
are **local**, by design — an operator reads an archive in the day they are in,
unlike an audit record, which is fixed to UTC because it is compared across
machines. `Date.now` is frozen for these captures but the zone is not, so the
day headings and times in the images are the capture host's zone. The fixture's
two seconds are `2023-11-14 22:15:00 UTC` and 24 hours later; a capture taken
in another zone will show different day keys for the same rows, and that is the
grouping rule working rather than drift.

The unavailable vocabulary is Core's `EvidenceUnavailableReason`, switched on
by discriminant so a reason this build does not model reaches the screen as
itself rather than borrowing a known one:

| `EvidenceUnavailableReason` | What the detail rail says |
| --- | --- |
| `summary_only` | display-only evidence — Core holds no canonical bytes for it |
| `missing_canonical_bytes` | Core names canonical bytes the store no longer holds |
| `hash_mismatch` | canonical bytes failed verification — not shown |
| `binary` | the canonical bytes verify and are not text, so there is no body to show |
| anything else | Core named a reason this build does not model (`<reason>`) |

### Recapture 2026-09-10 (E1)

All seven images above were retaken on 2026-09-10 with the identical procedure
(`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000` against the vite dev server on port 4173) and
visually reviewed again. Cause: batch F1 re-exported `EvidenceVerificationState`
and `EvidenceQualityStatus` on the `viden-core` facade and the detail rail's
report gained two rows for them, so the captures taken before F1 were one
revision behind the surface they document.

Four of the seven moved and three did not, which is itself the fact worth
recording:

| File | Moved | Why |
| --- | --- | --- |
| `evidence-1440x900-dark-en.png` | yes | the selected `patch` row carries both verdicts, so `verification` and `canonical quality` are now in its report |
| `evidence-text-1440x900-dark-en.png` | yes | same, on the selected `test_result` row |
| `evidence-unavailable-1440x900-dark-en.png` | yes | same row as `evidence`, with the content answer replaced |
| `evidence-1440x900-light-zh-CN.png` | yes | the two row labels translate, so the locale proof had to move with them |
| `evidence-summary-only-1440x900-dark-en.png` | no | the `task_summary` row carries neither verdict, so no row was added; its report is unchanged byte for byte |
| `evidence-empty-1440x900-dark-en.png` | no | no row is selected, so there is no report to add to |
| `evidence-rejected-1440x900-dark-en.png` | no | the query was refused, so nothing is loaded and nothing is selected |

One thing the `evidence-unavailable` capture makes visible and does not yet
explain in words: the report can read `verification verified` / `canonical
quality pass` while the content block below it reads "canonical bytes failed
verification — not shown". Those are two different facts — Core's recorded
verdicts on the *evidence record*, and the *content read's* own hash check
against `source_hash` — and the screen currently states neither relationship.
Recorded as a follow-up in `docs/core-0.3-compatibility.md`, not fixed here.

## Navigation shell captures (G3)

Captured 2026-09-12 with the same headless Chrome procedure
(`--headless --window-size=1440,900 --virtual-time-budget=6000`) against the
vite dev server on port 4173, then visually reviewed. These are the first
images of the rail-as-router shell: `D-RAILNAV` with the five secondary
D-screens rendered inside the cockpit, `D-SIDEBAR`'s floating mode, and
`D-STATUSBAR`'s config gear.

The D2 and D14 projections are the **generated** ones the standalone captures
already used (`../gui-screen-restore/projections/d2.json` and
`d14-audit.json` / `d14-raw.json`), handed to the same production renderers
through the cockpit's `secondaryViews` seam. Nothing about the screens changed
for these captures — only the container they are mounted into — which is the
property the images exist to show.

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [nav-d2-in-cockpit-1440x900-dark-en.png](nav-d2-in-cockpit-1440x900-dark-en.png) | nav-d2-in-cockpit | 1440x900 | dark | en |
| [nav-d2-in-cockpit-1440x900-light-zh-CN.png](nav-d2-in-cockpit-1440x900-light-zh-CN.png) | nav-d2-in-cockpit | 1440x900 | light | zh-CN |
| [nav-d14-in-cockpit-1440x900-dark-en.png](nav-d14-in-cockpit-1440x900-dark-en.png) | nav-d14-in-cockpit | 1440x900 | dark | en |
| [nav-sidebar-floating-peek-1440x900-dark-en.png](nav-sidebar-floating-peek-1440x900-dark-en.png) | nav-sidebar-floating-peek | 1440x900 | dark | en |
| [nav-statusbar-config-1440x900-dark-en.png](nav-statusbar-config-1440x900-dark-en.png) | nav-statusbar-config | 1440x900 | dark | en |

Each image exists to make one claim falsifiable:

| Image | The claim it proves |
| --- | --- |
| `nav-d2-in-cockpit` | the chrome **survives** the switch. Before G3 this screen replaced the window; here the titlebar, the activity rail, the context dock, the composer, and the statusbar are all still on screen beside it, and the composer still names the selected Lane. The rail marks `Decisions` current and carries `2` — Core's own `pendingDecisionCount`, the one published number the rail is allowed to show, and since G7 the same number the statusbar's `⏸` segment and the queue's own `2 awaiting you` header print |
| `nav-d2-in-cockpit` (light/zh-CN) | the locale proof for the added copy: the view head, the Close control's accessible name, and the rail's new slot names translate, while Core's ids, the raw action keys, and the capability names stay exactly as Core published them |
| `nav-d14-in-cockpit` | the return path has somewhere to return **to**. `D-AUDIT`'s link runs one way — audit rows link evidence, not the reverse — so an operator who follows it from an evidence row or a D12 chip used to lose the conversation; here the trail renders over it and `Esc` or the Close control brings the transcript back |
| `nav-sidebar-floating-peek` | `D-SIDEBAR`'s floating mode is an overlay, not a second layout. The transcript keeps its full width behind the peeked sidebar (the grid stays three tracks), the 12px hot zone with its `.edgehint` sits against the activity rail, and the rail's pin above the gear reads unpinned — the same component, a different host. Read it against `lane-rail`, which is the pinned half of the same decision |
| `nav-statusbar-config` | `D-STATUSBAR` splits the bar by **actionability**, not urgency. The popover offers exactly the six ambient segments; `MODE`, `PERM`, `LANE`, and the decision chip are on the bar behind it and are not in the list, because a control that can be switched off is a control the operator cannot reach when it matters (`O-B6`) |

The cockpit-bearing captures that predate G3 were re-taken in the same batch;
see the recapture section below.

## Cockpit recapture after the navigation shell (2026-09-12)

Every cockpit-bearing image in this directory was re-taken on **2026-09-12**
through the qa harness (`evidence/main-window-interactions/qa.ts`) against the
vite dev server on port 4173, with the same headless Chrome procedure
(`--headless --window-size=1440,900 --virtual-time-budget=6000`,
`data-capture-ready` gate), and each one was opened and read before it was
committed.

The reason is not cosmetic. G3 changed two pieces of chrome that appear in
every single one of these frames:

- the **activity rail** became the router of `D-RAILNAV`. It gained five
  destination slots (`Decisions`, `Lane monitor`, `Integration gate`,
  `Fleet board`, `Audit timeline`) between the existing Review/Evidence pair
  and the spacer, and a **pin** for the Lane sidebar directly above the
  settings gear;
- the **statusbar** gained the `D-STATUSBAR` config gear at its leading edge.

So every image taken before this batch showed a rail and a statusbar that no
longer exist. All 42 files moved; none came out byte-identical. The generated
projections behind them did not change — only the shell around them — which is
what makes this a recapture and not a new claim.

`evidence/0.1.0-rc.3/` is a historical release bundle and was deliberately left
untouched.

| File | Recaptured | What moved in it |
| --- | --- | --- |
| `approval-hunks-1440x900-dark-en.png` | 2026-09-12 | rail slots + pin, statusbar gear |
| `d1-1440x900-dark-en.png` | 2026-09-12 | rail slots + pin, statusbar gear; this is the reference frame for the new rail order |
| `d1-1440x900-light-zh-CN.png` | 2026-09-12 | same, in the light skin and zh-CN, where the new slot names and the pin's title translate |
| `d1-mode-menu-1440x900-dark-en.png` | 2026-09-12 | rail slots + pin, statusbar gear; the mode menu itself is unchanged |
| `d1-model-menu-1440x900-dark-en.png` | 2026-09-12 | rail slots + pin, statusbar gear; the model menu itself is unchanged |
| `d6-actions-1440x900-dark-en.png` | 2026-09-12 | rail slots + pin, statusbar gear; the agent-stopped surface is unchanged |
| `d6-error-1440x900-dark-en.png` | 2026-09-12 | same, with Core's restart refusal still below the actions |
| `evidence-1440x900-dark-en.png` | 2026-09-12 | rail slots + pin, statusbar gear; `Evidence` is now a router destination and is marked current |
| `evidence-1440x900-light-zh-CN.png` | 2026-09-12 | same, light/zh-CN |
| `evidence-empty-1440x900-dark-en.png` | 2026-09-12 | same; the empty archive answer is unchanged |
| `evidence-rejected-1440x900-dark-en.png` | 2026-09-12 | same; Core's refusal text is unchanged |
| `evidence-summary-only-1440x900-dark-en.png` | 2026-09-12 | same; the display-only report is unchanged |
| `evidence-text-1440x900-dark-en.png` | 2026-09-12 | same; the cut-content notice is unchanged |
| `evidence-unavailable-1440x900-dark-en.png` | 2026-09-12 | same; the failed-verification content answer is unchanged |
| `lane-rail-1440x900-dark-en.png` | 2026-09-12 | **behaviour change, not only chrome**: this state now mounts `laneSidebarMode: "pinned"`, so the Lane sidebar is a real fourth grid column (`52px 218px …`) that pushes the work surface right, instead of the overlay it used to be. The rail pin above the gear reads pressed |
| `nav-d14-in-cockpit-1440x900-dark-en.png` | 2026-09-12 | re-taken after the D-SIDEBAR fix so the rail matches the rest of the batch |
| `nav-d2-in-cockpit-1440x900-dark-en.png` | 2026-09-12 | same |
| `nav-d2-in-cockpit-1440x900-light-zh-CN.png` | 2026-09-12 | same |
| `nav-sidebar-floating-peek-1440x900-dark-en.png` | 2026-09-12 | **re-taken against the corrected default**: the state no longer passes a mode at all, so it proves `floating` is what the cockpit does with no caller preference |
| `nav-statusbar-config-1440x900-dark-en.png` | 2026-09-12 | **replaces a broken file** — see the note below |
| `palette-1440x900-dark-en.png` | 2026-09-12 | rail slots + pin, statusbar gear behind the scrim; the palette's own action list already named D2/D4/D10/D11/D12/D13/D14 and is unchanged |
| `palette-files-1440x900-dark-en.png` | 2026-09-12 | same |
| `palette-files-1440x900-light-zh-CN.png` | 2026-09-12 | same, light/zh-CN |
| `permission-ask-1440x900-dark-en.png` | 2026-09-12 | rail slots + pin, statusbar gear; the permission dock is unchanged |
| `permission-deny-redirect-1440x900-dark-en.png` | 2026-09-12 | same; the redirected composer placeholder is unchanged |
| `permission-deny-redirect-1440x900-light-zh-CN.png` | 2026-09-12 | same, light/zh-CN |
| `project-picker-1440x900-dark-en.png` | 2026-09-12 | rail slots + pin, statusbar gear; the picker popover is unchanged |
| `project-switch-confirm-1440x900-dark-en.png` | 2026-09-12 | same; the switch confirmation is unchanged |
| `review-1440x900-dark-en.png` | 2026-09-12 | rail slots + pin, statusbar gear; `Review` is now a router destination and is marked current |
| `review-1440x900-light-zh-CN.png` | 2026-09-12 | same, light/zh-CN |
| `review-commit-1440x900-dark-en.png` | 2026-09-12 | same; the enabled Commit / Commit & Push pair is unchanged |
| `review-commit-completed-1440x900-dark-en.png` | 2026-09-12 | same; the completion line and Git output disclosure are unchanged |
| `review-commit-completed-1440x900-light-zh-CN.png` | 2026-09-12 | same, light/zh-CN |
| `review-commit-pending-approval-1440x900-dark-en.png` | 2026-09-12 | same; the pending-approval line is unchanged |
| `review-empty-1440x900-dark-en.png` | 2026-09-12 | same; the clean-tree answer is unchanged |
| `review-omitted-1440x900-dark-en.png` | 2026-09-12 | same; the over-the-bound row notice is unchanged |
| `review-push-no-upstream-1440x900-dark-en.png` | 2026-09-12 | same; the no-upstream explanation and its single recovery action are unchanged |
| `review-rejected-1440x900-dark-en.png` | 2026-09-12 | same; Core's `git_diff` denial is unchanged |
| `review-rejected-action-1440x900-dark-en.png` | 2026-09-12 | same; Core's `git_add` denial is unchanged |
| `settings-1440x900-dark-en.png` | 2026-09-12 | rail slots + pin behind the dialog, statusbar gear; the Settings dialog is unchanged |
| `settings-1440x900-light-zh-CN.png` | 2026-09-12 | same, light/zh-CN |
| `settings-unavailable-1440x900-dark-en.png` | 2026-09-12 | same; the `ui.preference_persistence` notice is unchanged |

## Cockpit centre captures (G4)

Captured 2026-09-12 with the same headless Chrome procedure
(`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`) against the vite dev server on **port 4175**
(4173 is another worktree's), then opened and read one by one. These are the
first images of the cockpit centre level with its design: the
`.tabstrip.lanebar` Lane tab strip, the `.tool` transcript blocks, focus mode,
and the D4 wizard entered from the New Lane popover.

Two new projections back them, both deltas on the shared D1 fixture and both
described in `qa.ts` beside the fixture they mirror:

- `d1TwoLanes()` adds a second Lane with its **own** recorded branch plus an
  ACP session bound to it, so the strip shows a built-in Lane and an
  agent-bound one side by side. Without a second Lane a tab strip proves
  nothing.
- `d1ToolBlocks()` replaces the checklist with one workspace change carrying
  the same one-file, one-hunk page the review states use
  (`REVIEW_PAGE.entries[0].diff`) and one failed check run with the
  `file:line` location the canonical `d1-main-cockpit.json` state records.

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [centre-lane-tabs-1440x900-dark-en.png](centre-lane-tabs-1440x900-dark-en.png) | centre-lane-tabs | 1440x900 | dark | en |
| [centre-lane-tabs-1440x900-light-zh-CN.png](centre-lane-tabs-1440x900-light-zh-CN.png) | centre-lane-tabs | 1440x900 | light | zh-CN |
| [centre-tool-diff-1440x900-dark-en.png](centre-tool-diff-1440x900-dark-en.png) | centre-tool-diff | 1440x900 | dark | en |
| [centre-focus-mode-1440x900-dark-en.png](centre-focus-mode-1440x900-dark-en.png) | centre-focus-mode | 1440x900 | dark | en |
| [d4-from-popover-1440x900-dark-en.png](d4-from-popover-1440x900-dark-en.png) | d4-from-popover | 1440x900 | dark | en |

Each image exists to make one claim falsifiable:

| Image | The claim it proves |
| --- | --- |
| `centre-lane-tabs` | the strip is **per Lane and per Core fact**. Two tabs, the selected one marked with the design's accent rule, each carrying that Lane's own recorded branch — `codex/lane-core` and `vd/retry-policy`, which are different, so neither can be the workspace's — and the agent Core bound to it (`Viden Agent` for the built-in Lane, `codex-acp` for the ACP one). The trailing `.tabmeta` carries the project, `42.1k` (Core's published budget) and `Build` (Core's resolved mode) **once**, not per tab, because the projection is Lane-scoped |
| `centre-lane-tabs` (light/zh-CN) | the locale proof for the added copy: the strip's accessible names, the create affordance's label and the mode word translate, while Lane ids, branch names and agent ids stay exactly as Core published them |
| `centre-tool-diff` | the transcript carries the evidence inline. The change block shows the design's header — disclosure caret, the change kind Core published, the path, `+1 −1` — over the shared hunk renderer with Git's own `@@` header and per-side line numbers; the check block shows the command, the `Failed` chip, and the three `.testrow`s including the failing location Core reported. The diff block is opened here on purpose: collapsed is its default, and the capture has to show what the disclosure reveals |
| `centre-focus-mode` | `D-SIDEBAR`'s override really is **两侧强制 hover 浮窗**. The body is two tracks — activity rail and transcript — the pinned Lane column this state started in is gone, the context dock is off the grid behind its right-edge hot zone, and the titlebar's `IFocus` control reads pressed. Read it against `lane-rail`, which is the same cockpit with the column present |
| `d4-from-popover` | "Full setup…" keeps the draft and states the gap. The Lane is named `refactor-the-config-loader` after the task the popover carried, the agent step lists Core's adapters with `Codex` marked as the popover's pick, and the step says in its own words that `StarterLaneRequest` carries no agent binding — so the wizard never implies it will start the session |

Every cockpit-bearing capture in this directory was re-taken in the same batch;
see the recapture section below.

### Cockpit recapture after the centre batch (2026-09-12, G4)

The Lane tab strip is in every transcript-bearing frame and the `.tool` blocks
are in every frame with a checklist, so the same **42** cockpit-bearing images
G3 re-took were re-taken again, plus `d4-1440x900-dark-en.png` because the
wizard's steps changed — 43 files. All were captured against the same live
server and every one of them was opened and read before committing.

One layout fix moved with them and is worth naming, because it was found by a
capture rather than by a test. `gui-kit.css`'s `.frame` is a flex column and
ties on specificity with `.d1-frame`'s `display: grid` while sitting later in
the cascade, so the shell has always been laid out as flex; the body only
reached the status bar because the context dock's natural height happened to
get there. Focus mode takes the dock out of the flow, and the first
`centre-focus-mode` capture showed the shell stopping 170px short of the
status bar with bare page behind it. `.d1-body` now claims the remaining space
explicitly (`flex: 1 1 auto`), which is what the frame's rows always intended
and which measures identically in every non-focus state.

The 43 re-taken files, in one list so the claim is checkable:

- `approval-hunks-1440x900-dark-en.png`
- `d1-1440x900-dark-en.png`
- `d1-1440x900-light-zh-CN.png`
- `d1-mode-menu-1440x900-dark-en.png`
- `d1-model-menu-1440x900-dark-en.png`
- `d4-1440x900-dark-en.png`
- `d6-actions-1440x900-dark-en.png`
- `d6-error-1440x900-dark-en.png`
- `evidence-1440x900-dark-en.png`
- `evidence-1440x900-light-zh-CN.png`
- `evidence-empty-1440x900-dark-en.png`
- `evidence-rejected-1440x900-dark-en.png`
- `evidence-summary-only-1440x900-dark-en.png`
- `evidence-text-1440x900-dark-en.png`
- `evidence-unavailable-1440x900-dark-en.png`
- `lane-rail-1440x900-dark-en.png`
- `nav-d14-in-cockpit-1440x900-dark-en.png`
- `nav-d2-in-cockpit-1440x900-dark-en.png`
- `nav-d2-in-cockpit-1440x900-light-zh-CN.png`
- `nav-sidebar-floating-peek-1440x900-dark-en.png`
- `nav-statusbar-config-1440x900-dark-en.png`
- `palette-1440x900-dark-en.png`
- `palette-files-1440x900-dark-en.png`
- `palette-files-1440x900-light-zh-CN.png`
- `permission-ask-1440x900-dark-en.png`
- `permission-deny-redirect-1440x900-dark-en.png`
- `permission-deny-redirect-1440x900-light-zh-CN.png`
- `project-picker-1440x900-dark-en.png`
- `project-switch-confirm-1440x900-dark-en.png`
- `review-1440x900-dark-en.png`
- `review-1440x900-light-zh-CN.png`
- `review-commit-1440x900-dark-en.png`
- `review-commit-completed-1440x900-dark-en.png`
- `review-commit-completed-1440x900-light-zh-CN.png`
- `review-commit-pending-approval-1440x900-dark-en.png`
- `review-empty-1440x900-dark-en.png`
- `review-omitted-1440x900-dark-en.png`
- `review-push-no-upstream-1440x900-dark-en.png`
- `review-rejected-1440x900-dark-en.png`
- `review-rejected-action-1440x900-dark-en.png`
- `settings-1440x900-dark-en.png`
- `settings-1440x900-light-zh-CN.png`
- `settings-unavailable-1440x900-dark-en.png`

The five new files are listed in the table above.

## Context dock captures (G5)

Captured 2026-09-12 with the same headless Chrome procedure
(`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`) against the vite dev server on **port 4176**
(4173 and 4175 belong to other worktrees), then opened and read one by one.
These are the first images of the dock as the design draws it: the six-tab
`.docktabs` strip, the Environment panel's `.envsec` sections, the Files tree,
and the Diff panel with the shared hunk rows.

Two fixtures back them, both beside the ones they mirror in `qa.ts`:

- `PALETTE_FILES_APPS` is a second `QueryWorkspaceFiles` page, for the `apps/`
  prefix, answered only when that directory is opened. Without a second page
  the Files tab could be a client-side filter of the first, which is exactly
  what the client boundary forbids.
- the diff states reuse `REVIEW_PAGE`, the page the review captures already
  use, because the dock and DiffReview really do render one page per target.

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [dock-environment-1440x900-dark-en.png](dock-environment-1440x900-dark-en.png) | dock-environment | 1440x900 | dark | en |
| [dock-environment-1440x900-light-zh-CN.png](dock-environment-1440x900-light-zh-CN.png) | dock-environment | 1440x900 | light | zh-CN |
| [dock-files-1440x900-dark-en.png](dock-files-1440x900-dark-en.png) | dock-files | 1440x900 | dark | en |
| [dock-diff-1440x900-dark-en.png](dock-diff-1440x900-dark-en.png) | dock-diff | 1440x900 | dark | en |
| [dock-unavailable-1440x900-dark-en.png](dock-unavailable-1440x900-dark-en.png) | dock-unavailable | 1440x900 | dark | en |

Each image exists to make one claim falsifiable:

| Image | The claim it proves |
| --- | --- |
| `dock-environment` | the dock is **tabbed and honest about all six tabs**. Environment / Files / Diff are live; Terminal / Code / Docs are struck through and carry their reason in the title — Code because this state binds no file read, which is a fact about the host rather than a permanent gap (see `consumer-file-read`). Below them Changes shows Core's `+3 -1` over two real file rows with per-file counts, Core's byte-bound sentence, Local naming the workspace root it is showing, the Commit-or-push route, the PR-status absence, and the `.envctx` bar at `42.1k / 128k` with `33 per cent used` and the spend Core published |
| `dock-environment` (light/zh-CN) | the locale proof for every string this batch added, tab labels included: `环境 / 文件 / 终端 / 源码 / 对比 / 文档`, `变更`, `本地`, `提交或推送`, `PR 状态`, `上下文`. Paths, branch names and token counts stay exactly as Core published them |
| `dock-files` | the tree is **one Core page per directory**. `apps/` is open and shows `cli`, `gui` and `tui` — rows that exist only because that prefix was asked for — and Core's `complete: false` renders as "the page ended before this tree did". The selected file fills the inspector, whose Open is disabled with `runtime.workspace_file_reads` written out on screen rather than hidden in a tooltip |
| `dock-diff` | the panel is a **file list first**. Two entries, the second still collapsed with its real `+1284 -0`, the first expanded over the shared `diff_rows` body with Git's own `@@` header and per-side line numbers, and the inspector's File / Diff / "Open in review" beside a Stage and Revert that are disabled because Core publishes no per-file command for either |
| `dock-unavailable` | four absences, four sentences. Changes says no host is bound; Commit or push is disabled **and** says why on screen; PR status keeps its own absence sentence; the Local section still renders, because the workspace source is a fact Core did publish |

One state reads oddly on purpose and is worth naming: `review-empty`'s dock
shows `Lines +3 -1` above "No changes". Both are Core's, from two different
reads — the workspace source sample and the structured diff page — and the
section labels them separately rather than reconciling them. A client that
silently zeroed the counts to match the page would be inventing agreement
Core never published.

### Cockpit recapture after the context dock (2026-09-12, G5)

The dock is in every cockpit-bearing frame, so all **46** of them were
re-taken against the same live server and every one was opened and read before
committing. The standalone D-screen families (`d2-*`, `d4-*`, `d10-*`,
`d11-*`, `d12-*`, `d13-*`, `d14-*`) are full-window renderers with no dock, so
nothing in this batch could have moved them and they were left alone. Every
output was screened by file size for the Chrome error page's signature
(~28 KB against a real frame's 110-210 KB) — no suspects.

The 46 re-taken files, in one list so the claim is checkable:

- `approval-hunks-1440x900-dark-en.png`
- `centre-focus-mode-1440x900-dark-en.png`
- `centre-lane-tabs-1440x900-dark-en.png`
- `centre-lane-tabs-1440x900-light-zh-CN.png`
- `centre-tool-diff-1440x900-dark-en.png`
- `d1-1440x900-dark-en.png`
- `d1-1440x900-light-zh-CN.png`
- `d1-mode-menu-1440x900-dark-en.png`
- `d1-model-menu-1440x900-dark-en.png`
- `d6-actions-1440x900-dark-en.png`
- `d6-error-1440x900-dark-en.png`
- `evidence-1440x900-dark-en.png`
- `evidence-1440x900-light-zh-CN.png`
- `evidence-empty-1440x900-dark-en.png`
- `evidence-rejected-1440x900-dark-en.png`
- `evidence-summary-only-1440x900-dark-en.png`
- `evidence-text-1440x900-dark-en.png`
- `evidence-unavailable-1440x900-dark-en.png`
- `lane-rail-1440x900-dark-en.png`
- `nav-d14-in-cockpit-1440x900-dark-en.png`
- `nav-d2-in-cockpit-1440x900-dark-en.png`
- `nav-d2-in-cockpit-1440x900-light-zh-CN.png`
- `nav-sidebar-floating-peek-1440x900-dark-en.png`
- `nav-statusbar-config-1440x900-dark-en.png`
- `palette-1440x900-dark-en.png`
- `palette-files-1440x900-dark-en.png`
- `palette-files-1440x900-light-zh-CN.png`
- `permission-ask-1440x900-dark-en.png`
- `permission-deny-redirect-1440x900-dark-en.png`
- `permission-deny-redirect-1440x900-light-zh-CN.png`
- `project-picker-1440x900-dark-en.png`
- `project-switch-confirm-1440x900-dark-en.png`
- `review-1440x900-dark-en.png`
- `review-1440x900-light-zh-CN.png`
- `review-commit-1440x900-dark-en.png`
- `review-commit-completed-1440x900-dark-en.png`
- `review-commit-completed-1440x900-light-zh-CN.png`
- `review-commit-pending-approval-1440x900-dark-en.png`
- `review-empty-1440x900-dark-en.png`
- `review-omitted-1440x900-dark-en.png`
- `review-push-no-upstream-1440x900-dark-en.png`
- `review-rejected-1440x900-dark-en.png`
- `review-rejected-action-1440x900-dark-en.png`
- `settings-1440x900-dark-en.png`
- `settings-1440x900-light-zh-CN.png`
- `settings-unavailable-1440x900-dark-en.png`

The five new files are listed in the table above.

### The `nav-statusbar-config` file, stated plainly

The first commit of this batch shipped a
`nav-statusbar-config-1440x900-dark-en.png` that was reviewed and correct, but
the working tree then received a second, **broken** write of the same path: a
recapture run that started after the dev server had already been stopped, so
Chrome rendered its own "This site can't be reached / ERR_CONNECTION_REFUSED"
page and the driver saved that as the evidence. It landed after the last
working-tree check of the previous round, so it was never looked at. The file
in this commit was taken with vite up and read before committing; it shows the
`.sbcfg` popover over the six ambient segments, which is what the row above
claims. The 2026-09-12 run guarded against a repeat: the dev server was
verified live before the batch started, and every output was screened by file
size afterwards for the error page's signature (~28 KB against a real frame's
100-200 KB) — no suspects. That guard lives in the throwaway capture driver,
not in the repository; the durable protection is still the rule that every PNG
listed in a report has been opened and read.

The 27 standalone screen captures in this directory (`d2-*`, `d4-*`, `d10-*`,
`d11-*`, `d12-*`, `d13-*`, `d14-*`) are **not** cockpit-bearing — they are the
full-window renderers with no rail and no statusbar — so nothing in G3 could
have moved them and they were left alone.

## Secondary-view captures (G6)

Captured 2026-09-12 with headless Chrome
(`--disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`) against the vite dev server on port 4177 — 4173,
4175 and 4176 were checked with `lsof` first — then visually reviewed (all
five opened and read).

Two of the five (`d10-actions`, `d14-filtered` dark/en) were taken with the
old `--headless`; the other three with `--headless=new`, because the old mode
started hanging indefinitely while a second Chrome was capturing on this host
and a hung driver is how a broken file gets committed unseen. Every other flag
is identical and the rendering is the same page; the mode is recorded here
only so the method matches what was run.

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [d10-actions-1440x900-dark-en.png](d10-actions-1440x900-dark-en.png) | d10-actions | 1440x900 | dark | en |
| [d13-drill-1440x900-dark-en.png](d13-drill-1440x900-dark-en.png) | d13-drill | 1440x900 | dark | en |
| [d14-filtered-1440x900-dark-en.png](d14-filtered-1440x900-dark-en.png) | d14-filtered | 1440x900 | dark | en |
| [d14-filtered-1440x900-light-zh-CN.png](d14-filtered-1440x900-light-zh-CN.png) | d14-filtered | 1440x900 | light | zh-CN |

| Image | The claim it proves |
| --- | --- |
| `d10-actions` | the action row is **four controls on every card**, and the two that cannot act say so. `lane_core` (terminal route, no published session) shows Attach live and Stop dimmed; `lane_review` (ACP route, one published session) shows both live. Pause and Kill are dimmed on both cards — visible, in the positions the design draws them, and carrying the sentence naming the command `RuntimeCommand` does not have. Read it against `d10-blind`, which is the same two cards before the row existed |
| `d13-drill` | both Lane-binding answers in one frame. The upper node carries `Open Lane lane_core ↗` and is the control; the lower one — the same generated node with its binding removed — carries "Core bound no Lane to this task, so there is nothing to open." and no affordance at all. The difference is in the frame rather than in a hover state, which is the point of the visible cue |
| `d14-filtered` | the whole loaded-page claim at once: the actor chips are `All actors / operator / agent` — no `system` chip, because no row on this page is one — the four time chips sit behind `All time`, the note under the bar states that the cut is the loaded page and names `GUI-CORE-024`, the rollup reads `Outcomes on the loaded page, filtered · 1 of 3` with `denied 1`, exactly one row survives the filter, and `Export` is dimmed at the end of the bar |
| `d14-filtered` (light/zh-CN) | the locale and skin proof for the added copy: the chip labels, the loaded-page note, the rollup caption and the export control translate, while every Core value in the surviving row — the dotted `evidence.rejected` key, the actor word `agent`, the agent id `codex-acp`, the outcome `denied`, the object and argument chips — stays exactly as Core published it, and the timestamp keeps its fixed `UTC` format in both locales |

Two facts in this batch have no capture and are covered by vitest instead,
stated here rather than implied:

- **the absent decision count.** `pendingDecisionCount: null` renders as *no*
  badge and no statusbar segment, with the reason on the slot's accessible
  name. A screenshot of an absence is indistinguishable from a screenshot of a
  zero, so the distinction is pinned by
  [`../../tests/rail_decision_badge.spec.ts`](../../tests/rail_decision_badge.spec.ts)
  instead of by a PNG.
- **a settled Stop.** The harness's host callbacks never resolve on purpose,
  so `d10-actions` shows the row before any command is sent. The pending line,
  Core's acceptance, and Core's verbatim refusal are pinned by
  [`../../tests/d10_lane_actions.spec.ts`](../../tests/d10_lane_actions.spec.ts)
  and the command-id correlation by
  [`../../tests/lane_handoff.spec.ts`](../../tests/lane_handoff.spec.ts).

`d14-filtered`'s rows are the generated page's own 2023 timestamps, so the
three narrow time ranges would empty it. The capture therefore shows the time
chips present with `All time` pressed rather than a range engaged; no newer
timestamp was invented to make a prettier frame. The range rule itself is
covered by `filterAuditRows` in
[`../../tests/d14_audit_filters.spec.ts`](../../tests/d14_audit_filters.spec.ts),
which pins `today`, `24h` and `7d` against a frozen clock.

The `d13-drill` and `d10-actions` states are cockpit-bearing: both screens are
centre-pane views since G3, and the captures show the titlebar, activity rail,
Lane sidebar, context dock, composer and statusbar unchanged around them, with
the view's own Close control in the position DiffReview puts its own.

## H2 hygiene captures

Appended by batch H2 (2026-09-12) for the three states the hygiene fixes added.
Each was taken against the production render functions through `qa.ts`, at
`1440x900` dark/en unless the row says otherwise, and each was opened and read
before it was committed.

| State | URL | What it must show |
| --- | --- | --- |
| `evidence-hash-mismatch` | `…/qa.html?state=evidence-hash-mismatch` | E1 defect 6. The `patch` row whose archive record says `verified` and whose content answer is `Unavailable { HashMismatch }`. The report's `verification` row keeps the recorded word `verified` and carries the mismatch treatment — the error colour plus a strike-through, so the state is not colour alone — and the sentence under it relates the two facts: what the archive recorded, and what Core got when it read the bytes just now. The content note below still says verification failed and shows no body. Framed on the report so both are in one frame. |
| `welcome-fill` | `…/qa.html?state=welcome-fill` | E1 defects 8 and 9 together. The unbound first-run centre pane: the activity rail, the welcome column and the status bar all reach the bottom of the window, and the Recent note reads `No project open` / `Recent projects appear once a project is open.` with the host's own `Error: Core adapter is not connected` kept beneath it as the dimmed diagnostic. |
| `welcome-fill` at `1440x640` | same | the same layout at the second height the E1 run measured, where the old chain stopped at roughly the same 525 px regardless of window height. Nothing is cut and the status bar is still at the bottom. |
| `d10-events-not-read` | `…/qa.html?state=d10-events-not-read` | compatibility follow-up 2. The Lane monitor as an in-cockpit view — full chrome, Close control — with the event stream saying `The Core audit timeline has not been read yet.` This state issues no audit read, and the sentence claims nothing about whether Core offers a timeline; that claim belongs to the capability-absent state, which names `runtime.audit`. |

| File | State | Size | Mode | Locale |
| --- | --- | --- | --- | --- |
| [evidence-hash-mismatch-1440x900-dark-en.png](evidence-hash-mismatch-1440x900-dark-en.png) | evidence-hash-mismatch | 1440x900 | dark | en |
| [welcome-fill-1440x900-dark-en.png](welcome-fill-1440x900-dark-en.png) | welcome-fill | 1440x900 | dark | en |
| [welcome-fill-1440x640-dark-en.png](welcome-fill-1440x640-dark-en.png) | welcome-fill | 1440x640 | dark | en |
| [d10-events-not-read-1440x900-dark-en.png](d10-events-not-read-1440x900-dark-en.png) | d10-events-not-read | 1440x900 | dark | en |

No existing capture was retaken by H2. The `evidence-unavailable` capture above
is the *same* Core answer as `evidence-hash-mismatch` and is deliberately kept:
it frames the content note, this one frames the report, and the pair is what
shows that the two facts are now related rather than merely both present.

## G7 consumer captures and the integrated recapture

Batch G7 (2026-09-12) consumed the five `0.3.4` Core capabilities, and the
recapture below exists because of how G5 and G6 were built: the dock tabs (G5)
and the D-view states plus the decision badge (G6) were captured on **separate
branches**, so every G6 frame showed the pre-G5 dock and every G5 frame lacked
G6's badge. Both sets were also taken before the counts they print were
unified. Every **cockpit-bearing** frame in this directory was therefore
retaken on the integrated tree, in one run, and viewed one by one before it was
committed: 65 images, the 59 existing cockpit frames plus the 6 new
`consumer-*` ones.

Captured 2026-09-12 with headless Chrome
(`--headless=new --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`) against the vite dev server on port 4179, six
captures in flight at a time; every image in the table below was then opened
and read. `welcome-fill` keeps its second height (`1440x640`), captured the
same way.

The 28 frames **not** in this run are the standalone-screen states — `d2*`,
`d4*`, `d10-blind*`, `d10-ticker`, `d11*`, `d12*`, `d13`, `d14-audit*`,
`d14-raw-fallback` — which render one D-screen with no cockpit chrome around it
(`renderD2Decisions`, `renderD4LaneCreate`, and so on). Nothing G5, G6 or G7
changed reaches them: they carry no titlebar, no activity rail, no dock and no
statusbar, so their committed images remain exactly as valid as the day they
were framed. Cockpit-bearing means the state mounts the cockpit —
`mountCockpit`, `openReview`, `openEvidence`, or `renderD1Cockpit` directly —
which is also how the recapture list was derived rather than by eye.

**What the recapture changes on every cockpit frame.** The statusbar's decision
segment and the rail's D2 badge now print the one count
[the README defines](../../README.md#decision-queue-badge) — `⏸ 2 awaiting you`,
the generated D2 projection's own total — where the G5/G6 frames printed the
gate count beside a different queue total. Frames that carry the dock also carry the G5
tabs, and frames that carry a D-view carry G6's own chrome.

**Two frames changed for a reason of their own:**

- `dock-unavailable` was byte-identical to `d1` in the committed set, because
  every absence sentence it claims to show sat below the dock's fold with the
  Environment facts open. The state now collapses that section, the way
  `dock-environment` does, and the frame shows the sentences it was always
  supposed to prove.
- `d2-rail-badge` is **deleted with its rows**. Its whole claim was the
  divergence — a badge reading `7` beside a queue header reading `2` — and G7
  closed that by deriving both from `pending_decision_count`. Keeping the state
  would have meant fabricating a statusbar count the D2 projection contradicts,
  which is the one thing the state was built to expose. `nav-d2-in-cockpit` now
  carries the three-way agreement in one frame, and
  [`../../tests/rail_decision_badge.spec.ts`](../../tests/rail_decision_badge.spec.ts)
  and [`../../tests/gate_dormancy.rs`](../../tests/gate_dormancy.rs) keep the
  badge-reads-the-projection and the one-count claims under test.

The five new states are the `consumer-*` rows in the state table above.
`consumer-transcript-rows` also carries the batch's locale and skin proof:
every label G7 added — `Load older`, the 8 KiB row-bound sentence, `Open
evidence`, `tool result`, the check-run status words, `Conversation complete.`
— is translated, while every Core value in a row (the prose, the tool name, the
path, the decision word, the audit id) stays exactly as Core published it.

Two G7 facts have no capture and are pinned by tests instead, stated here
rather than implied:

- **a refused file read.** Core's `CommandRejected` for a path the permission
  gate denies, and the local refusal for a path that leaves the target, are
  sentences rather than states of the tree, and a screenshot of one is
  indistinguishable from a screenshot of the other's wording. Both are pinned
  by [`../../tests/workspace_file_read.rs`](../../tests/workspace_file_read.rs)
  and
  [`../../tests/workspace_file_read.spec.ts`](../../tests/workspace_file_read.spec.ts).
- **a layout record Core could not write.** `persisted: false` adds one
  sentence to the rail pin and to the statusbar popover; the harness has no
  state for it, and it is pinned by
  [`../../tests/layout_preferences.spec.ts`](../../tests/layout_preferences.spec.ts).

| File | State | Viewport | Mode | Locale |
| --- | --- | --- | --- | --- |
| [approval-hunks-1440x900-dark-en.png](approval-hunks-1440x900-dark-en.png) | approval-hunks | 1440x900 | dark | en |
| [centre-focus-mode-1440x900-dark-en.png](centre-focus-mode-1440x900-dark-en.png) | centre-focus-mode | 1440x900 | dark | en |
| [centre-lane-tabs-1440x900-dark-en.png](centre-lane-tabs-1440x900-dark-en.png) | centre-lane-tabs | 1440x900 | dark | en |
| [centre-lane-tabs-1440x900-light-zh-CN.png](centre-lane-tabs-1440x900-light-zh-CN.png) | centre-lane-tabs | 1440x900 | light | zh-CN |
| [centre-tool-diff-1440x900-dark-en.png](centre-tool-diff-1440x900-dark-en.png) | centre-tool-diff | 1440x900 | dark | en |
| [consumer-evidence-patch-1440x900-dark-en.png](consumer-evidence-patch-1440x900-dark-en.png) | consumer-evidence-patch | 1440x900 | dark | en |
| [consumer-file-read-1440x900-dark-en.png](consumer-file-read-1440x900-dark-en.png) | consumer-file-read | 1440x900 | dark | en |
| [consumer-transcript-rows-1440x900-dark-en.png](consumer-transcript-rows-1440x900-dark-en.png) | consumer-transcript-rows | 1440x900 | dark | en |
| [consumer-transcript-rows-1440x900-light-zh-CN.png](consumer-transcript-rows-1440x900-light-zh-CN.png) | consumer-transcript-rows | 1440x900 | light | zh-CN |
| [consumer-turn-live-1440x900-dark-en.png](consumer-turn-live-1440x900-dark-en.png) | consumer-turn-live | 1440x900 | dark | en |
| [consumer-workspace-commit-1440x900-dark-en.png](consumer-workspace-commit-1440x900-dark-en.png) | consumer-workspace-commit | 1440x900 | dark | en |
| [d1-1440x900-dark-en.png](d1-1440x900-dark-en.png) | d1 | 1440x900 | dark | en |
| [d1-1440x900-light-zh-CN.png](d1-1440x900-light-zh-CN.png) | d1 | 1440x900 | light | zh-CN |
| [d1-mode-menu-1440x900-dark-en.png](d1-mode-menu-1440x900-dark-en.png) | d1-mode-menu | 1440x900 | dark | en |
| [d1-model-menu-1440x900-dark-en.png](d1-model-menu-1440x900-dark-en.png) | d1-model-menu | 1440x900 | dark | en |
| [d10-actions-1440x900-dark-en.png](d10-actions-1440x900-dark-en.png) | d10-actions | 1440x900 | dark | en |
| [d10-events-not-read-1440x900-dark-en.png](d10-events-not-read-1440x900-dark-en.png) | d10-events-not-read | 1440x900 | dark | en |
| [d13-drill-1440x900-dark-en.png](d13-drill-1440x900-dark-en.png) | d13-drill | 1440x900 | dark | en |
| [d14-filtered-1440x900-dark-en.png](d14-filtered-1440x900-dark-en.png) | d14-filtered | 1440x900 | dark | en |
| [d14-filtered-1440x900-light-zh-CN.png](d14-filtered-1440x900-light-zh-CN.png) | d14-filtered | 1440x900 | light | zh-CN |
| [d6-actions-1440x900-dark-en.png](d6-actions-1440x900-dark-en.png) | d6-actions | 1440x900 | dark | en |
| [d6-error-1440x900-dark-en.png](d6-error-1440x900-dark-en.png) | d6-error | 1440x900 | dark | en |
| [dock-diff-1440x900-dark-en.png](dock-diff-1440x900-dark-en.png) | dock-diff | 1440x900 | dark | en |
| [dock-environment-1440x900-dark-en.png](dock-environment-1440x900-dark-en.png) | dock-environment | 1440x900 | dark | en |
| [dock-environment-1440x900-light-zh-CN.png](dock-environment-1440x900-light-zh-CN.png) | dock-environment | 1440x900 | light | zh-CN |
| [dock-files-1440x900-dark-en.png](dock-files-1440x900-dark-en.png) | dock-files | 1440x900 | dark | en |
| [dock-unavailable-1440x900-dark-en.png](dock-unavailable-1440x900-dark-en.png) | dock-unavailable | 1440x900 | dark | en |
| [evidence-1440x900-dark-en.png](evidence-1440x900-dark-en.png) | evidence | 1440x900 | dark | en |
| [evidence-1440x900-light-zh-CN.png](evidence-1440x900-light-zh-CN.png) | evidence | 1440x900 | light | zh-CN |
| [evidence-empty-1440x900-dark-en.png](evidence-empty-1440x900-dark-en.png) | evidence-empty | 1440x900 | dark | en |
| [evidence-hash-mismatch-1440x900-dark-en.png](evidence-hash-mismatch-1440x900-dark-en.png) | evidence-hash-mismatch | 1440x900 | dark | en |
| [evidence-rejected-1440x900-dark-en.png](evidence-rejected-1440x900-dark-en.png) | evidence-rejected | 1440x900 | dark | en |
| [evidence-summary-only-1440x900-dark-en.png](evidence-summary-only-1440x900-dark-en.png) | evidence-summary-only | 1440x900 | dark | en |
| [evidence-text-1440x900-dark-en.png](evidence-text-1440x900-dark-en.png) | evidence-text | 1440x900 | dark | en |
| [evidence-unavailable-1440x900-dark-en.png](evidence-unavailable-1440x900-dark-en.png) | evidence-unavailable | 1440x900 | dark | en |
| [lane-rail-1440x900-dark-en.png](lane-rail-1440x900-dark-en.png) | lane-rail | 1440x900 | dark | en |
| [nav-d14-in-cockpit-1440x900-dark-en.png](nav-d14-in-cockpit-1440x900-dark-en.png) | nav-d14-in-cockpit | 1440x900 | dark | en |
| [nav-d2-in-cockpit-1440x900-dark-en.png](nav-d2-in-cockpit-1440x900-dark-en.png) | nav-d2-in-cockpit | 1440x900 | dark | en |
| [nav-d2-in-cockpit-1440x900-light-zh-CN.png](nav-d2-in-cockpit-1440x900-light-zh-CN.png) | nav-d2-in-cockpit | 1440x900 | light | zh-CN |
| [nav-sidebar-floating-peek-1440x900-dark-en.png](nav-sidebar-floating-peek-1440x900-dark-en.png) | nav-sidebar-floating-peek | 1440x900 | dark | en |
| [nav-statusbar-config-1440x900-dark-en.png](nav-statusbar-config-1440x900-dark-en.png) | nav-statusbar-config | 1440x900 | dark | en |
| [palette-1440x900-dark-en.png](palette-1440x900-dark-en.png) | palette | 1440x900 | dark | en |
| [palette-files-1440x900-dark-en.png](palette-files-1440x900-dark-en.png) | palette-files | 1440x900 | dark | en |
| [palette-files-1440x900-light-zh-CN.png](palette-files-1440x900-light-zh-CN.png) | palette-files | 1440x900 | light | zh-CN |
| [permission-ask-1440x900-dark-en.png](permission-ask-1440x900-dark-en.png) | permission-ask | 1440x900 | dark | en |
| [permission-deny-redirect-1440x900-dark-en.png](permission-deny-redirect-1440x900-dark-en.png) | permission-deny-redirect | 1440x900 | dark | en |
| [permission-deny-redirect-1440x900-light-zh-CN.png](permission-deny-redirect-1440x900-light-zh-CN.png) | permission-deny-redirect | 1440x900 | light | zh-CN |
| [project-picker-1440x900-dark-en.png](project-picker-1440x900-dark-en.png) | project-picker | 1440x900 | dark | en |
| [project-switch-confirm-1440x900-dark-en.png](project-switch-confirm-1440x900-dark-en.png) | project-switch-confirm | 1440x900 | dark | en |
| [review-1440x900-dark-en.png](review-1440x900-dark-en.png) | review | 1440x900 | dark | en |
| [review-1440x900-light-zh-CN.png](review-1440x900-light-zh-CN.png) | review | 1440x900 | light | zh-CN |
| [review-commit-1440x900-dark-en.png](review-commit-1440x900-dark-en.png) | review-commit | 1440x900 | dark | en |
| [review-commit-completed-1440x900-dark-en.png](review-commit-completed-1440x900-dark-en.png) | review-commit-completed | 1440x900 | dark | en |
| [review-commit-completed-1440x900-light-zh-CN.png](review-commit-completed-1440x900-light-zh-CN.png) | review-commit-completed | 1440x900 | light | zh-CN |
| [review-commit-pending-approval-1440x900-dark-en.png](review-commit-pending-approval-1440x900-dark-en.png) | review-commit-pending-approval | 1440x900 | dark | en |
| [review-empty-1440x900-dark-en.png](review-empty-1440x900-dark-en.png) | review-empty | 1440x900 | dark | en |
| [review-omitted-1440x900-dark-en.png](review-omitted-1440x900-dark-en.png) | review-omitted | 1440x900 | dark | en |
| [review-push-no-upstream-1440x900-dark-en.png](review-push-no-upstream-1440x900-dark-en.png) | review-push-no-upstream | 1440x900 | dark | en |
| [review-rejected-1440x900-dark-en.png](review-rejected-1440x900-dark-en.png) | review-rejected | 1440x900 | dark | en |
| [review-rejected-action-1440x900-dark-en.png](review-rejected-action-1440x900-dark-en.png) | review-rejected-action | 1440x900 | dark | en |
| [settings-1440x900-dark-en.png](settings-1440x900-dark-en.png) | settings | 1440x900 | dark | en |
| [settings-1440x900-light-zh-CN.png](settings-1440x900-light-zh-CN.png) | settings | 1440x900 | light | zh-CN |
| [settings-unavailable-1440x900-dark-en.png](settings-unavailable-1440x900-dark-en.png) | settings-unavailable | 1440x900 | dark | en |
| [welcome-fill-1440x640-dark-en.png](welcome-fill-1440x640-dark-en.png) | welcome-fill | 1440x640 | dark | en |
| [welcome-fill-1440x900-dark-en.png](welcome-fill-1440x900-dark-en.png) | welcome-fill | 1440x900 | dark | en |

Every row above was captured in this run and viewed. The earlier capture tables
in this document record when each state was *first* framed and what it was
framed for; for a cockpit-bearing state, the image they link is the one in this
table.
