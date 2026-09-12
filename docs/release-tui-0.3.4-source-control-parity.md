# Viden TUI 0.3.4 Source-Control And Conflict Parity

Chinese version: [release-tui-0.3.4-source-control-parity.zh-CN.md](release-tui-0.3.4-source-control-parity.zh-CN.md)

TUI parity minimum for the three `0.3.3` source-control capabilities:
`runtime.structured_diff` (GUI-CORE-012), `runtime.operator_git`
(GUI-CORE-020), and `runtime.conflict_content` (GUI-CORE-015). The contract is
[Frontend Integration Contract](frontend-integration-contract.md), sections
"Source Control And Diff UI Contract", "Operator Source-Control Actions", and
"Conflict content"; the design is
[0.3.3 Core Contract Increment](release-0.3.3-contract-design.md).

Core stays authoritative for every fact below. The TUI parses no diff text, runs
no `git`, and infers no outcome from output.

## Approval Decision Context

- When `ApprovalRequestView.decision_context` carries a diff, the approval
  overlay renders hunk rows in place of the `input_preview` line: a file row
  with the change kind and the real `+`/`-` counts, an `@@` header per hunk,
  and one row per line with the `old_line`/`new_line` numbers Git wrote.
- A row's absent side stays blank. A removed line has no new-file number, and
  the TUI never prints `0` there.
- `binary`, `omitted`, and `truncated` each get their own stated row, so "not
  shown" is never rendered as "nothing changed". `base_sha256` becomes a
  `computed against <8 chars>` note.
- Paths keep their distinctive tail; content lines are cut at the end with the
  registered `…` marker. The panel grows with the rows, bounded by a row cap so
  the approval actions stay on screen; the remainder is counted.
- Without `runtime.structured_diff` the overlay keeps the preview line and adds
  one row naming the missing capability.

## `/git` Operator Actions

- `/git` (alias `/source`) opens a selector-first picker, exactly as `/acp`
  does. Opening it sends no command.
- Rows: Stage all changes, Commit…, Push, Fetch. Commit opens a one-line
  message prompt; Esc returns to the rows and discards the draft. A blank
  message is refused locally and nothing is sent.
- Each row sends one `RunOperatorGitAction { owner, target, action }`. The
  target is the focused Lane, otherwise the workspace; the TUI never passes a
  path, and `Stage` carries no paths so Core stages every changed path.
- The command's `owner` equals its envelope owner, which is what Core's
  supervisor requires, and that owner is the actor the audit record names. For a
  Lane target it is the runtime owner Core published for that exact Lane.
- Every target Core has published no owner for is refused locally, before
  anything is sent, and `RuntimeOwner::default()` is never sent on
  `RunOperatorGitAction`. The rows stay listed and are labelled with the reason,
  and picking one states the refusal as a typed system entry. The GUI refuses
  the same two cases at `D1-OPERATOR-GIT-OWNER`, so the two clients name one
  gap rather than diverging:
  - a Lane whose runtime owner Core has not published — the entry names the
    Lane;
  - the workspace, for which Core publishes no workspace-scoped operator
    identity yet (GUI-CORE-027) — the entry cites that number. The picker's
    TARGET row keeps stating the workspace source facts Core did publish: the
    refusal is about the actor, not about the tree.
- Awaited events, in order: `CommandAccepted` (a receipt, never an outcome),
  then `ApprovalRequested` if the gate asks — handled by the normal approval
  overlay, which renders the staged diff as hunk rows — then
  `OperatorGitActionFinished`, and `WorkspaceSourceUpdated` behind it.
- `Completed` renders the resampled branch, ahead/behind and dirty, with the
  bounded output collapsed to a line count and its first line plus a truncated
  note. `Failed` renders the localized class, its recovery, and Core's
  `detail`. `CommandRejected` renders Core's reason verbatim as a refusal that
  happened before anything ran.
- One action is in flight at a time; a second is refused locally so "nothing was
  sent" stays true. A dismiss row stops watching a stranded correlation without
  claiming Core stopped.
- Without `runtime.operator_git` all four rows stay listed, disabled, and
  labelled with the capability name. The TUI does not fall back to a shell.

### Failure class to operator copy

| `OperatorGitFailureClass` | Message | Recovery |
| --- | --- | --- |
| `NothingToCommit` | nothing staged to commit | stage changes first |
| `NonFastForward` | the remote has commits this branch does not | fetch first, then push |
| `AuthenticationRequired` | the remote refused the credentials | configure the remote credentials |
| `RemoteUnreachable` | the remote could not be reached | check the network and the remote |
| `NoUpstream` | this branch has no upstream | push again with set upstream |
| `PathOutsideRepository` | a path left the target repository | choose a path inside the target |
| `Other` | git reported a failure | read the detail below |
| unrecognized variant | git reported a failure this build cannot classify | read the detail below |

`Other` is permanent, not a gap. An unrecognized variant keeps Core's real
`detail` instead of being squeezed into the nearest-looking class.

## Conflict Content

- A Decision Center bounce row appends `n files · m hunks` and the baseline kind
  (`revision`, `reviewed evidence`, `unknown`, or unrecognized) when
  `ConflictBounce.content` is present. A bounce with no content keeps exactly
  the reason-only row it had.
- An inspect row on the supervision overlay opens a read-only modal listing each
  file and, per rejected hunk, three labelled blocks: OURS at the file's own
  line numbers, THEIRS from the incoming hunk, and BASE, the patch preimage. A
  reason tag names Core's classification.
- The modal states that this is not a merge result and offers no resolution.
  Core computed no merge base.
- `base: None` renders as "the hunk carried no preimage"; `Some(vec![])` renders
  as "the patch expected an empty region". They are different facts.
- `omitted` files and `truncated` content are stated. `content: None` means Core
  has nothing to show, never that the conflict was empty.
- Lane conflicts carry the same summary on their transcript entry and the same
  modal body from `LaneConflictView.content`.

## Evidence Inspector

`runtime.evidence_reads` (GUI-CORE-025). The contract is
[Frontend Integration Contract](frontend-integration-contract.md), section
"Evidence archive reads"; it closes the dedicated evidence inspector deferred at
the `0.3.2` supervision checkpoint
([checkpoints](release-evidence/tui-supervision/checkpoints.md)).

- Two entry points. The supervision decision overlay gains an **Evidence…** read
  row, scoped to the record's own `owner` copied verbatim from Core — a gate's
  or a review's. A new `/evidence` system command opens the same inspector,
  scoped to the focused Lane by the id Core published with every other owner
  field left unset, so Core's prefix match treats them as wildcards; with no
  focused Lane the read is unscoped. Both rows are reads: they are appended
  after every Core decision, so neither renumbers one, and neither is blocked by
  or settles an in-flight supervision command.
- Rows never come from `RuntimeViewState.latest_evidence`. That projection is a
  recent window with no ordering rule, no cursor and no content; presenting it
  as the archive would claim a completeness Core never stated. The decisions
  overlay keeps its own evidence counts from that window, unchanged.
- The list is day-grouped in Core's order: the **undated** group first — an
  undated row is the oldest thing Core can honestly say about it — then UTC days
  ascending. A row is `HH:MM:SS`, a kind tag, the summary, and the Lane the owner
  names; an undated row renders `--:--:--` rather than a fabricated midnight.
- The footer states `LOADED n · more` or `LOADED n · archive complete`. The
  load-more row sends `next_after` back **verbatim** as `after`; the cursor is
  never parsed, constructed, or compared. Paging never widens the scope the
  operator opened.
- `f` cycles the kind filter through *all* plus the kinds Core actually
  returned, and re-queries from the first page rather than filtering rows in
  hand: Core cuts pages after filtering, so a locally filtered page could not say
  whether a matching row sits on a page this client never loaded. Grouping never
  hides: every row Core delivered gets a row.
- `Enter` on a row reads `ReadEvidenceContent` and opens the detail pane: a
  header with the kind and summary, the id and time, the owner, then `source`,
  `path`, the canonical item/bundle and the short `source_hash` when present, and
  the metadata **keys** as facts — a metadata value is free-form JSON and is
  never rendered as a typed fact. One content read is in flight at a time; a
  second is refused locally and nothing is sent, and content Core already
  published is re-rendered from the overlay's own cache instead of re-read.
- Content shapes: `Text` renders bounded scrollable rows beside the `sha256`
  Core verified them against, with the `truncated` note when the bound cut them;
  `Diff` renders through the same hunk producer the approval overlay uses, so one
  evidence patch and one workspace diff are the same rows; `Unavailable` renders
  the typed reason. `CommandRejected` renders Core's reason verbatim — a refusal
  is never an empty archive.
- `EvidenceRecorded` while the inspector is open marks the loaded page **stale**
  with a one-line banner; `r` reloads from the first page. Nothing re-reads
  behind the operator, because a background re-query would move the rows they are
  looking at.
- `Esc` unwinds the detail pane before the overlay. Closing drops the panel, the
  page, and the content cache, so a reopened inspector always re-queries rather
  than showing a page of unknown age.
- Without `runtime.evidence_reads` nothing is sent. The `/evidence` jump-index
  row stays listed, disabled, and labelled with the capability name; the
  supervision Evidence… row states the same gap as a local refusal; and typing
  `/evidence` states it as a typed system entry.

### Unavailable reason to operator copy

| `EvidenceUnavailableReason` | Rendered sentence |
| --- | --- |
| `SummaryOnly` | No canonical bytes: display-only evidence, never merge evidence. |
| `MissingCanonicalBytes` | No canonical bytes: the store no longer holds what this row names. |
| `HashMismatch` | canonical bytes failed verification — not shown |
| `Binary` | Canonical bytes verify but are not UTF-8: no text or diff shape to publish. |
| unrecognized variant | Core stated a reason this build cannot name. |

`HashMismatch` is deliberately not softened. Those are exactly the bytes a
reviewer must not be shown as canonical, and Core never serves them, so the row
says the verification failed rather than that the content is missing. An
unrecognized variant keeps its own sentence instead of borrowing the wording of
a reason it is not.

### Evidence Live Offline Check

Run against a copy of `.viden` minus `cache/` under a scratch workspace, never
the live one, with `--provider fallback --model test-local` in tmux.

`/evidence` with no focused Lane reads the whole archive, and the empty answer is
stated as an answer:

```
┌ EVIDENCE INSPECTOR ───────── Esc back · Enter open · f filter · r reload ┐
│ SCOPE whole archive · oldest first                                       │
│ FILTER every kind Core returns                                           │
│ No evidence in this scope.                                               │
│ LOADED 0 · archive complete                                              │
└──────────────────────────────────────────────────────────────────────────┘
```

The Evidence… row on a real dormant gate scopes the read to that record's own
Core-published owner. The scope row shows exactly the fields Core set, and the
unset ones are omitted rather than printed empty:

```
┌ SUPERVISION DECISION ─── Esc back · arrows/number select · Enter confirm ┐
│ ⏸ GATE gate-acp-session-019fb6ec-9f2e-72b1-a563-af76fc5561ca · Proposed  │
│ EVIDENCE 0 · VALIDATOR - · CONFLICT -                                    │
│ > 1 Accept merge gate                                                    │
│   2 Reject merge gate                                                    │
│   3 Evidence…                                                            │
│   4 Audit trail                                                          │
└──────────────────────────────────────────────────────────────────────────┘

┌ EVIDENCE INSPECTOR ───────── Esc back · Enter open · f filter · r reload ┐
│ SCOPE session=agent-session_1785480383543782000 task=acp-session-019fb6… │
│ FILTER every kind Core returns                                           │
│ No evidence in this scope.                                               │
│ LOADED 0 · archive complete                                              │
└──────────────────────────────────────────────────────────────────────────┘
```

The detail pane has no live capture. The archive Core rebuilds at open was empty
in that workspace, and no command the TUI can dispatch fills it: the only path
that writes `task_summary` into the archive is `RuntimeCommand::StartAgentTask`,
which the TUI never sends. Detail rendering is covered by the
`evidence-reads.json` replay and by the `main-evidence-detail` preview instead;
see Known Gaps.

## Live Offline Check

Run against a copy of `.viden` under a scratch workspace, never the live one,
with `--provider fallback --model test-local` in tmux. The two outcome captures
below were taken on a workspace target *before* the owner refusal landed; they
still show what a settled `OperatorGitActionFinished` renders, but a workspace
target no longer reaches Core at all. The picker block is the current rendering,
regenerated by `scripts/tui-previews.sh` into `main-git-picker.txt`:

```
┌ Source control ──────────────────────────────────────────────────────┐
│   Stage all changes · no workspace owner · GUI-CORE-027              │
│ > Commit… · no workspace owner · GUI-CORE-027                        │
│   Push · no workspace owner · GUI-CORE-027                           │
│   Fetch · no workspace owner · GUI-CORE-027                          │
│ TARGET  workspace · codex/v3-tui-client · ahead 2 behind 0 · dirty   │
└──────────────────────────────────────────────────────────────────────┘
```

```
SYSTEM
  Stage all changes completed · main · ahead 0 behind 0 · dirty ·
  OUTPUT 1 lines · git add --all completed AUDIT audit_...

SYSTEM
  Fetch failed · the remote could not be reached · check the network and
  the remote · fatal: 'origin' does not appear to be a git repository
  fatal: Could not read from remote repository. ... AUDIT audit_...
```

Both outcomes came from `OperatorGitActionFinished`; the failure class was
Core's `remote_unreachable`, and the two matching `source.fetch` audit records
(`phase: authorized` then `phase: failed`) confirm audit-before-effect.

## Parity Evidence

Shared fixtures under `crates/types/tests/fixtures/frontend-contract-v1/`
replay to the same business facts the GUI reads:

| Fixture | TUI test |
| --- | --- |
| `structured-diff.json` | `tui::modal::tests::structured_diff_fixture_replays_into_approval_hunk_rows` |
| `operator-git.json` | `tui::app::tests::operator_git_fixture_replays_into_typed_outcome_entries` |
| `conflict-content.json` | `tui::modal::tests::conflict_content_fixture_replays_into_summary_rows_and_a_three_sided_detail` |
| `evidence-reads.json` | `tui::evidence_panel::tests::evidence_reads_fixture_replays_into_rows_pages_content_and_a_refusal` |

Deterministic preview states, generated by `scripts/tui-previews.sh` and
exported by `scripts/tui-regression.sh`:

- `main-approval-hunks`: the approval overlay with decision-context hunks, an
  omitted file, and the base note;
- `main-git-picker`: the `/git` picker with the target and source row;
- `main-git-outcome`: one `Completed` and one `Failed` outcome entry;
- `main-conflict-detail`: the conflict modal with three sides, an omitted file,
  and the truncation note;
- `main-evidence-list`: the paged archive with the undated group first, two UTC
  day groups, the load-more row and the `LOADED 3 · more` footer;
- `main-evidence-detail`: one `patch` row's canonical bytes as hunk rows beside
  the sha256 Core verified them against.

## Composer Routing And The `/git` Target (T1c, 2026-09-10)

E1's release-evidence run drove one real task through this client and stopped
three times. All three were reproduced as failing tests before anything was
changed; the reproduction and the Core-side facts are
[Core 0.3 compatibility](core-0.3-compatibility.md), open follow-ups 5-7.

### The composer queues only when its own target is busy

`command_for_composer` routed on one unified "Core is busy" predicate, which was
true whenever Core's unscoped `assistant_stream` still held a finished built-in
turn's reply, whenever any Lane sat in `Draft`, and whenever `queued_inputs` was
non-empty. Core never drains the session queue, so the last one never went back
to false: after one fallback turn, or after creating one starter Lane, every
later prompt queued and none ran.

The predicate now splits along the question each caller asks:

- `state::composer_target_busy` answers routing and is owner-scoped, the same
  rule the GUI's composer applies. It reads an active tool call, a pending
  approval, an active task, or a live Agent session whose published owner is
  this input's target, plus this client's own in-flight native turn. An absent
  owner counts as the session scope: the frontend contract says an absent owner
  means the fact belongs to no Lane scope, and the built-in provider publishes
  every one of its facts that way, so treating them as someone else's work
  would start a second concurrent turn.
- `state::has_active_work` answers presentation — the status row's `ACTIVE`, the
  live-work strip, the exit confirmation, `Ctrl-C` — and is not owner-scoped, so
  a Lane running its own turn still reads as something happening.

H1's single-predicate rule survives the split as an implication rather than an
equality, and it holds by construction: presentation is computed *from* the
routing answer, so whenever the composer queues the status row says `ACTIVE`.
The two can describe two different turns; they can never describe one turn two
ways.

Lane lifecycle state is gone from both. `LaneStatus::is_active` is a lifecycle
predicate that counts `Draft`, `Attached` and `Detached`; the running subset is
`Queued`, `Starting`, `Running`, `WaitingApproval` and `NeedsInput`.
`queued_inputs` is gone from both, and the Core queue gap stays open and
recorded — this client simply stops treating a queue Core will not run as
evidence of a turn.

### The residue window for a native turn, stated exactly

**Closed 2026-09-12 by T2.** `runtime.turn_lifecycle` (C6) publishes
`TurnStarted`/`TurnFinished` for every native exit, so
`apps/tui/src/tui/native_turn.rs` is deleted and this client holds no liveness
window at all. What the window was, and the miss it accepted, is kept below
because the replacement is only legible against it; see
"Turn liveness is a Core fact" for what ships now.

Core publishes no turn-liveness fact for the built-in provider: no Agent
session, no task, and `AssistantDelta`s with no session id. It also settles the
unscoped stream only on a terminal Agent-session fact, so a finished reply stays
in the view for the rest of the session. `apps/tui/src/tui/native_turn.rs` holds
the window instead, and holds nothing authoritative:

- it **opens** when the composer dispatches `SubmitUserInput`;
- it **closes** on that command's `CommandRejected`, on the first
  `SnapshotUpdated` after dispatch — the head of the terminal batch
  `runtime_events_for_streaming_output` emits when a native turn returns, where
  the intermediate approval-boundary batches carry no such prefix — or on a
  snapshot replacement, which discards the stream the correlation was reading.

`SnapshotUpdated` is not a turn-liveness fact. Three operator commands publish
one of their own — `SetWorkMode`, `SetPermissionLevel`, `SelectModel` — so
changing work mode, permission level or model *while a native turn streams*
closes the window early. That miss is bounded and answered by Core rather than
guessed at: the next prompt is submitted, and the supervisor refuses a second
job for one owner with `active runtime job … is already running`, which the
transcript renders. The failure it replaces — a window that never closed — could
not be recovered from inside a session at all. Closing it properly needs the
Core turn-liveness fact of open follow-up 3.

### The selected Lane survives leaving its detail panel

`/git` is typed in the composer, and the Lane selection used to die with the
lane-detail panel the operator closed to get there, so every `/git` opened from
the composer targeted the workspace and was refused under GUI-CORE-027. A second
cause sat behind it: the ambient lane-detail panel out-rendered the interaction
panel, so even a surviving selection would have shown a lane inspector where the
picker's rows belong. A third surfaced only once the first two were fixed and
the flow could be walked live: a selected Lane with no session pulls the client
to the board lens, and the board, like `Setup`, `Decisions` and `Gallery`,
renders no composer at all (`render::render_frame`). A target that survives is
worth nothing if the surface it is a target *for* is off screen.

- `TuiUiState.lane_detail_open` is separate from `TuiUiState.focused_lane`. The
  first is what is on screen; the second is which Lane the next command names.
- The `Esc` unwind chain gained a rung: overlay -> lane detail -> Lane target ->
  insert. The first `Esc` puts the panel away and states that the Lane stays the
  target; the second clears it and states that too, so the documented way to
  drop the target is discoverable rather than hidden. `L:<lane>` on the status
  row is the target throughout.
- The interaction panel renders before the ambient lane detail. A selector the
  operator opened is never hidden by one they did not.
- The board lens is unwound by the same rung that closes the panel, and
  `reconcile_ui_state_with_runtime` stops pulling a Session lens back to the
  board once that panel is closed. Both moves are conditioned on
  `lane_detail_open`, so the board still wins while the operator is looking at
  the Lane, and a lens they asked for by name — `Setup`, `Decisions`,
  `Gallery` — is never unwound by this rung.
- The 027 refusal for the workspace target is unchanged.

The picker's TARGET row now names the Lane and states the source as unknown for
a Lane target. `RuntimeViewState.workspace_source` is a single workspace-scoped
view and Core publishes nothing per Lane, so printing the workspace's branch and
ahead/behind beside a Lane's name would attribute one tree's state to another.

### Live offline check, 2026-09-10

Run in tmux (120x40) against a scratch copy of `.viden` — `agents/`,
`context-engine/`, `projects/`, `workflows/`, `index.sqlite3` and `lanes.tsv`,
with `cache/` excluded — over a temporary Git repository, `--provider fallback
--model test-local`. The repository's own `.viden/` was not opened. The sequence
is E1's, continued past the point where it stalled: one fallback turn, `New
Lane`, approve, `Esc` out of the detail panel, a second composer submit, `/git`,
then `Esc` again.

Every block below is a `tmux capture-pane` frame of that run, trimmed to the
rows described and with the scratch path shortened. The lane id is the one Core
minted in that run.

**1. After one completed fallback turn.** The composer offers `Send` and the
status row says `IDLE`. Before T1c this is where both went permanently to
`Queue` / `ACTIVE`, because Core's unscoped stream still held the reply.

```
│ MODE [Build]  PERM [Ask]      ACTIONS: [^J Send] [^K Clr] [^R Regenerate] [^N New Task] [? Help] │
 P:t1c-live L:- M:Build INSERT · IDLE · EVENTS 8 · TOKENS 0 · PROVIDER healthy
```

**2. After `New Lane` and `Allow once`.** The Lane is real — Core created the
branch `viden/lane_1789024399248187000` and its worktree, both confirmed out of
band with `git branch` and `git worktree list`. It sits in `Draft`, and the
status row says `IDLE`, not `ACTIVE`: a created Lane is not a running turn.

```
┌ LANE DETAIL ─────────────────────────────────── lane_1789024399248187000 ┐
│ lane_1789024399248187000 coder                                           │
│ ROUTE main→side-1                                                        │
│ STATE  Draft                                                             │
 P:t1c-live L:lane_178902439 M:Build NORMAL · IDLE · EVENTS 10 · TOKENS 0 · PROVIDER healthy
```

**3. `Esc` out of the lane detail.** The panel closes, the cockpit and its
composer come back, `L:lane_178902439` survives, and the transcript states what
happened and how to undo it.

```
 VIDEN / COCKPIT  fallback  test-local  healthy
 /private/tmp/…  lane lane_1789024399248187000  session -  approvals 0
...
│ MODE [Build]  PERM [Ask]      ACTIONS: [^J Send] [^K Clr] [^R Regenerate] [^N New Task] [? Help] │
 P:t1c-live L:lane_178902439 M:Build NORMAL · IDLE · EVENTS 10 · TOKENS 0 · PROVIDER healthy
```

```
SYSTEM
  Closed the lane detail. The Lane stays the target for /git; Esc again clears it.
```

**4. A composer submit with the Lane in existence.** Accepted and answered — a
`USER` row and a reply, no `queued …` row anywhere in the session. Under the old
predicate this is E1's second stall.

```
USER
  where does the config loader live

USER
  add a config loader note

SYSTEM
  Closed the lane detail. The Lane stays the target for /git; Esc again clears it.

USER
  third prompt after the Lane exists
```

**5. `/git` typed in the composer.** The picker names the Lane and all four rows
are pickable: Core published a runtime owner for this Lane, so
`runtime.operator_git` is reachable from this client. No commit was run; the
point of the capture is the target and the enabled rows.

```
┌ Source control ──────────────────────────────────────────────────────┐
│ > Stage all changes                                                  │
│   Commit…                                                            │
│   Push                                                               │
│   Fetch                                                              │
│ TARGET  lane_1789024399248187000 · source unknown                    │
└──────────────────────────────────────────────────────────────────────┘
```

**6. The documented way to drop the target.** One more `Esc` clears it and says
so; `L:` returns to `-`.

```
SYSTEM
  Cleared the Lane target lane_1789024399248187000. /git now names the workspace.

 P:t1c-live L:- M:Build INSERT · IDLE · EVENTS 13 · TOKENS 0 · PROVIDER healthy
```

**7. The workspace target is unchanged.** `/git` with no Lane selected still
renders all four rows refused under GUI-CORE-027, beside the workspace's own
source facts — which it does have.

```
┌ Source control ──────────────────────────────────────────────────────┐
│ > Stage all changes · no workspace owner · GUI-CORE-027              │
│   Commit… · no workspace owner · GUI-CORE-027                        │
│   Push · no workspace owner · GUI-CORE-027                           │
│   Fetch · no workspace owner · GUI-CORE-027                          │
│ TARGET  workspace · main · ahead 0 behind 0 · dirty                  │
└──────────────────────────────────────────────────────────────────────┘
```

Three honest notes about the same run. Core's `assistant_stream` visibly
concatenates every reply, because Core still never settles it for the built-in
path — that is open follow-up 3, unchanged here, and it is now only a display
artefact rather than a routing input. The scratch `.viden` copy carries eight
`Proposed` gates from the sessions it was copied from, which the supervision
strip lists throughout; they belong to the copied state, not to this run. And
one `USER` row reads `i/git`, because the harness sent an `i` while the composer
was already in Insert mode; that is the operator of the harness, not the client.

## TUI Parity For The 0.3.4 Core Increments (T2, 2026-09-12)

Batch T2 of [the 0.3.4 plan](release-0.3.4-plan.md) adopts the Core increments
C5 to C7 and records the parity minimum for C9. The shapes below are the ones
that shipped, which differ from
[the contract design](release-0.3.4-contract-design.md) body in the ways its own
"Amendments From Implementation" section records; the fixtures
`workspace-owner.json`, `turn-lifecycle.json`, and `durable-work-evidence.json`
are the wire truth this client was written against, and three of the tests below
replay them directly rather than hand-building rows.

| Capability | TUI parity in T2 |
| --- | --- |
| `runtime.turn_lifecycle` | adopted: the busy predicate and the queue copy |
| `runtime.workspace_owner` | adopted: `/git` workspace target, per-Lane source row |
| `runtime.durable_work_evidence` | adopted: archived `patch` rows and the approval audit row |
| `ui.layout_preferences` | no parity minimum; the TUI renders no lane sidebar |
| `runtime.workspace_file_reads` | no parity minimum; the TUI registers no file viewer |

### Turn liveness is a Core fact

The predicate as shipped, per composer scope:

- `state::composer_target(state)`
  (`apps/tui/src/tui/state.rs:311`) resolves which owner scope the next
  submission names. A focused Agent conversation names that session's Lane;
  everything else is the session scope, with `lane_id: None`, which is the
  driver's own envelope owner. One resolver, so routing and the send path
  cannot disagree about whose turn they are asking about.
- `state::turn_is_active_for` (`apps/tui/src/tui/state.rs:340`) matches
  `RuntimeViewState.active_turns` on that scope and never on `turn_id`:
  `turn_id` is a fresh per-turn value that exists so an audit row has something
  to join on, and the contract says a client matches the scope.
- `state::composer_target_busy` (`apps/tui/src/tui/state.rs:396`) answers
  routing — queue versus submit, read by `app::command_for_composer`
  (`apps/tui/src/tui/app.rs:2632`). For the **session scope** it is a
  Core-published session turn, an active tool call, a pending approval, an
  active task, or a live Agent session whose published owner is that scope. For
  a **Lane's conversation** it is a Core-published turn for that Lane, or that
  Lane's own Agent session running.
- `state::has_active_work` (`apps/tui/src/tui/state.rs:440`) answers
  presentation — the status row's `ACTIVE`, the live-work strip, the exit
  confirmation, `Ctrl-C` — and is not owner-scoped: it starts from the routing
  answer and adds any turn, tool call, approval, task, running Lane, or live
  Agent session. H1's single-predicate rule therefore still holds as an
  implication by construction.

Four facts neither predicate reads, each for its own reason:

- `assistant_stream`, Core's unscoped stream. Core now clears it on the
  session-scoped `TurnFinished`, and this client does not read it either way.
- this client's own dispatched command id. That was T1c's window, and it closed
  on the first `SnapshotUpdated` after dispatch — not a turn-liveness fact,
  since `SetWorkMode`, `SetPermissionLevel`, and `SelectModel` publish one of
  their own. `apps/tui/src/tui/native_turn.rs` is deleted.
- `queued_inputs`. A queue is work Core has not started; the drain is announced
  by `InputDequeued` and the turn it starts by `TurnStarted`.
- Lane lifecycle state, including `Draft`. Unchanged from T1c.

An empty `active_turns` is a real answer, including right after a restart:
Core never resumes a turn across one and persists neither fact. Without the
capability the list is always empty, so this client submits and Core answers
with its own `CommandRejected` — a Core answer rather than a client guess,
which is the direction the contract requires.

The queue copy states which promise Core is making
(`apps/tui/src/tui/composer.rs:93`). Core drains the session queue only behind
a **completed** session-scoped turn, so while such a turn runs the composer
reads `N queued; runs after the current turn`, and with no session turn running
— after a failure, a cancellation, or a Lane's turn, which never arms the drain
— it reads `N queued; waits for the next completed turn`. Saying "runs after
the current turn" in the second case would promise execution Core has already
decided against.

E1 defect 2's two scenarios are pinned as tests. At the predicate level:
`state::tests::the_composer_is_idle_after_one_completed_fallback_turn` and
`state::tests::a_draft_lane_and_a_session_queue_are_not_active_work`. At the
routing level, `app::tests::core_turn_brackets_decide_whether_the_next_prompt_
queues_or_submits` pushes `TurnStarted` and then `TurnFinished` through the real
reducer, so what is asserted is that the bracket decides, not a flag.

### The `/git` workspace target under the Core-published owner

`operator_git::operator_git_owner` (`apps/tui/src/tui/operator_git.rs:235`) is
the single resolver the picker rows and the send path both read, so a row is
never offered as pickable and then refused after the operator picked it.

- **Workspace target.** The owner Core published in
  `RuntimeViewState.workspace_owner`, copied verbatim into both places Core
  compares: the command's own `owner` field and the envelope's. The client does
  not recompute the `ws_` digest from the root path and does not substitute
  `RuntimeOwner::default()` — Core authorizes the action by the workspace and
  project ids *it* published, and an id this client derived is not one of them.
- **Absence.** `workspace_owner` absent means this Core published none. The
  four rows stay listed, disabled, and labelled `Core published no workspace
  owner`, and nothing is sent. The label no longer quotes GUI-CORE-027: the
  register entry is closed on the Core side, and a client that kept citing it
  would be reporting a Core gap that no longer exists.
- **Lane target.** Unchanged from T1c: the runtime owner Core published for
  that exact Lane.
- **An unmodelled target.** `SourceTarget` is `#[non_exhaustive]`, so a target
  a newer Core names is refused and called unknown rather than resolved as the
  nearest-looking one.

The picker's TARGET row (`apps/tui/src/tui/modal.rs:1083`) reads each target's
own source: `lane_sources[lane]` for a Lane, `workspace_source` for the
workspace. That split is what `LaneSourceUpdated` is for — before it a
Lane-target action put one tree's branch and ahead/behind into the workspace's
chip. A Lane with no row still says `source unknown`, because a Lane without a
row is either a direct-workspace Lane or one Core has not sampled, and printing
the workspace's numbers beside a Lane's name would attribute one tree's state
to another.

### The evidence inspector shows archived patches

`runtime.durable_work_evidence` changed which facts reach the archive, not the
read contract, so the inspector's paging, cursor handling, and content states
are unchanged. What is new is the detail pane's canonical half
(`apps/tui/src/tui/evidence_panel.rs:693`), stated as four separate rows
because a reader of an archived patch has four separate questions:

- `CANONICAL item … · bundle …` — where the bytes are;
- `PRODUCER <identity> · <role> · task <task>` — `producer.task_id` is exactly
  what a merge gate checks, so this row is what explains a `MissingProducer`
  refusal to someone who can see the evidence exists;
- `APPROVAL audit <id>`, or `no operator approval receipt; a rule or an adapter
  allowed this` — an absent `permission_snapshot_id` means no operator approval
  allowed that call, and rendering nothing there would read as approved;
- `RECORD Core verified the canonical reference: <state>` — the verdict Core
  recorded on the *record*, which is a different fact from the content read's
  own hash check against `source_hash`. Keeping them apart is what stops a
  `verified` record from standing beside an unexplained `HashMismatch`, which
  is the shape E1 defect 9 records on the GUI side.

`canonical: None` on a `patch` row renders as `CANONICAL none`, with the row's
own summary carrying the reason — a diff over `MAX_EVIDENCE_CONTENT_BYTES`, or
a store write that failed. A blank there read as display-only evidence, which a
merge gate treats differently.

Content states are unchanged and complete: `Diff` renders through the same hunk
producer the approval overlay uses, `Text` states the verified hash and whether
the byte bound cut it, and `SummaryOnly`, `MissingCanonicalBytes`,
`HashMismatch`, and `Binary` each keep their own sentence
(`evidence_panel::unavailable_reason_key`).

The audit lens needed no new code: the dotted `action` key is Core's stable
vocabulary and is rendered raw, which is why `approval.allow_once` arrives
readable without a catalog entry per decision. The change is the proof —
`audit_panel::tests::the_durable_work_evidence_fixture_replays_the_approval_
decision_into_the_audit_lens` replays the fixture's audit page and asserts the
row names the approval request and the tool the decision released. Before C7
that id was a live correlation id written nowhere, which is E1 defect 4.

### The two capabilities with no TUI parity minimum

- `ui.layout_preferences` persists the Lane sidebar mode and the hidden ambient
  statusbar segments. The TUI renders **no lane sidebar**: its Lane surfaces are
  the side screens, the lane-detail panel, and the `/lane` selector, none of
  which is a pinned-or-floating sidebar. Nothing in `apps/tui/**` reads,
  persists, or mirrors `lane_sidebar_mode`, and nothing writes
  `SetUiLayoutPreferences` or `ResetUiLayoutPreferences`; the statusbar's
  segments are not operator-hideable here either. A TUI that grew a hideable
  ambient segment would consume the existing record rather than add a second
  preference model.
- `runtime.workspace_file_reads` answers what is in one workspace file. The TUI
  registers no file viewer and sends no `ReadWorkspaceFile`, so there is
  nothing to render it in; the file *inventory* reader
  (`apps/tui/src/tui/workspace_files.rs`) is unchanged.

Both rows are recorded as such in
[Core 0.3 compatibility](core-0.3-compatibility.md).

### Regression frames changed by T2, with causes

`scripts/tui-regression.sh` was run on the batch's own base
(`claude/int-0.3.4` at `e262fbc7`) and again on the finished branch, and the
generated frames were compared file by file. Exactly two changed, each with its
`.ansi` and `.svg` renderings; every other frame, line count, width, and chip
balance is byte-identical.

| Frame | Cause |
| --- | --- |
| `main-git-picker` | The preview now publishes `RuntimeViewState.workspace_owner`, which is what `runtime.workspace_owner` (C5) adds, so the four rows render pickable instead of suffixed with `· no workspace owner · GUI-CORE-027`. |
| `main-evidence-detail` | The preview's `patch` row now carries the `CanonicalEvidenceReference` an archived native patch actually has since `runtime.durable_work_evidence` (C7), so the detail gains five rows — `CANONICAL`, `HASH`, `PRODUCER`, `APPROVAL`, `RECORD` — in place of five blank rows below the diff. The diff hunks themselves are unchanged and still fit the panel. |

`scripts/tui-previews.sh` is the first step of the regression script, so both
frames were regenerated by the same run; no preview was regenerated by hand.

### Live offline check, 2026-09-12

Run in tmux (140x40) with `--provider fallback --model test-local` against a
scratch Git repository created for the run under this session's scratchpad, not
against this repository: the workspace was `git init`-ed with two files and one
commit, and Core created its own `.viden/` and Lane worktree inside it. This
repository's `.viden/` was neither opened nor modified.

What the harness could show:

- **One completed fallback turn leaves the composer submitting.** After the
  welcome prompt, the approved `lane_create`, and `Esc` out of the lane detail,
  the composer offered `[^J Send]` and the status row said `IDLE`.
- **A created Lane is not a running turn.** The Lane sat in `Draft` with
  `L:lane_1789212279910316000` on the status row and `IDLE` beside it.
- **Two further composer prompts both ran.** Both appear as `USER` rows with no
  `queued …` row anywhere in the session, which is E1 defect 2's two scenarios
  walked end to end.
- **A Lane target states its own source.** `/git` with the Lane selected showed
  the four rows pickable and `TARGET lane_1789212279910316000 ·
  viden/lane_1789212279910316000 · …`, read from `lane_sources`. Before C5 this
  row said `source unknown` for every Lane, which is what the T1c section above
  records as an honesty consequence; it is now a published fact.
- **The documented way to drop the target still works.** One more `Esc`
  cleared it, stated so, and `L:` returned to `-`.
- **The evidence inspector answers an empty complete archive**, which is
  correct for this run: the fallback provider applied no file mutation, and
  `runtime.durable_work_evidence` archives a `patch` row for an applied
  mutation. An empty archive here is not the E1 defect-5 emptiness.

What the harness could not show:

- **The workspace `/git` rows enabled.** `/git` with no Lane selected still
  rendered all four rows disabled with `Core published no workspace owner`,
  because on the path a `viden` binary actually takes
  `RuntimeViewState.workspace_owner` is absent. That is a Core-side gap, not a
  client one, and it is recorded as such below and in
  [Core 0.3 compatibility](core-0.3-compatibility.md): the only production
  caller of `SessionEngine::bind_workspace_owner` is
  `LocalCoreHost::open_workspace`, which the GUI uses and `apps/cli` does not —
  the CLI bootstraps the engine and supervisor directly. The client half is
  proven instead by
  `app::tests::a_published_workspace_owner_enables_the_rows_and_is_sent_verbatim`
  and by the regenerated `main-git-picker` frame.
- **An archived patch in a TUI session.** Filling the archive needs an applied
  `write_file`/`edit_file`, which the fallback provider does not produce; E1
  reached one only through an approved tool call driven by a real provider. The
  detail pane's evidence here is the fixture replay plus the preview frame.
- **A drained session queue.** Nothing in the run queued, because nothing was
  busy long enough to queue behind; the queue copy's two states are covered by
  `composer::tests::composer_invites_next_prompt_during_active_turn`.

Two honest notes about the harness rather than the client. Two `USER` rows read
`try point live` and `i/evidence`, because the driver script sent an `i` while
the composer was already in Insert mode and lost the first characters of the
line — that is the operator of the harness, not the client. And the run's first
prompt went through the welcome flow, which creates a starter Lane and asks for
`lane_create`; the approval was allowed from the Decisions surface, which is
where a pinned approval's keys live.

## Known Gaps

- The conflict modal is reached from the supervision overlay's inspect row,
  which exists for merge-gate bounces. A Lane conflict states its counts on its
  transcript entry; the TUI has no pickable Lane-conflict row to open the modal
  from, and adding one is deferred with the rest of the Lane decision surface.
- The ambient pinned approval panel keeps the `input_preview` line. Hunk rows
  live in the approval overlay, which is where an approval is decided; the
  pinned panel is a fixed-height summary and growing it would push the pinned
  actions off a short terminal.
- The evidence inspector's detail pane still has no *live* capture. The gap has
  moved: `runtime.durable_work_evidence` (C7) makes an applied native mutation
  archive a `patch` row, so the archive is no longer structurally empty for a
  TUI session, but the capture of one is E2's to take. T2's evidence for the
  pane is the generated preview frame plus the two fixture replays named above.
  The recent window and the archive remain different projections, which is why
  this client still never derives archive rows from `latest_evidence`.
- The inspector offers one kind filter at a time. Core's `kinds` is an OR list,
  but the overlay has one cycling control, so sending several would claim a
  selection the operator never made. Owner filters below the opened scope, and
  a time range, have no control either.
- No row-level action inside the inspector: no "open in review" into a diff
  surface, no copy of an evidence id, no jump to the gate that requires it. The
  overlay browses and reads; it decides nothing.
- `QueryWorkspaceDiff` / `WorkspaceDiffLoaded` has no TUI reader yet: the TUI
  renders the diff Core attaches to an approval, not an operator diff pane.
- **Closed 2026-09-12 by C5 and T2, with one Core-side gap left.** Core mints a
  workspace-scoped operator identity, and this client sends the workspace
  target under it, so the `/git` rows are actionable for the workspace as well
  as for a Lane whose runtime owner Core has published. A target with no
  published owner is still refused locally, now naming the missing fact rather
  than GUI-CORE-027. The gap: `apps/cli` bootstraps the engine and supervisor
  directly rather than through `LocalCoreHost::open_workspace`, which is the
  only production caller of `SessionEngine::bind_workspace_owner`, so a `viden`
  TUI session still sees `workspace_owner` absent and shows the refusal. The
  GUI is unaffected because its adapter opens through the host. Closing it is a
  Core or CLI change, not a client one.
