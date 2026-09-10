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
| all `d1*` | a `ContextUsageProjection` on the context dock and the three statusbar fields the shared fixture leaves empty (`context`, `diagnosticsCount`, `pendingGateCount`) | the populated statusbar fixture in `tests/statusbar.spec.ts` |
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
| `palette`, `palette-files` | six real Viden paths in Core's lexicographic order with fixed byte sizes, handed to `loadPaletteFiles` | the loaded-inventory fixture in `tests/command_palette.spec.ts` and the page shape asserted in `tests/workspace_files.rs` |
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
| `d1` | `…/qa.html?state=d1` | the full cockpit; the titlebar project selector with its branch and dirty marker beside the `↑/↓` and worktree chips; all nine statusbar segments carrying a fact plus the pending-gate chip; the three composer selector pills |
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
| `lane-rail` | `…/qa.html?state=lane-rail` | the rail pinned open (it auto-hides), showing the one `.wsroot` project group named `viden` with its `▾` collapse, its Lane count, the per-group `＋`, the Lane nested beneath it, and the `＋ Add project…` footer — and no second group and no "Global" section |
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
