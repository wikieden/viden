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

## Known Gaps

- The conflict modal is reached from the supervision overlay's inspect row,
  which exists for merge-gate bounces. A Lane conflict states its counts on its
  transcript entry; the TUI has no pickable Lane-conflict row to open the modal
  from, and adding one is deferred with the rest of the Lane decision surface.
- The ambient pinned approval panel keeps the `input_preview` line. Hunk rows
  live in the approval overlay, which is where an approval is decided; the
  pinned panel is a fixed-height summary and growing it would push the pinned
  actions off a short terminal.
- The evidence inspector has no live capture of its detail pane. The archive
  Core rebuilds at open was empty in the scratch workspace, and no command the
  TUI dispatches fills it: `RuntimeCommand::StartAgentTask` is the only path
  that writes `task_summary` evidence into the archive, and the TUI never sends
  it. In the same session `latest_evidence` reached 1 while `QueryEvidence`
  answered an empty complete archive — the recent window and the archive are
  different projections, which is exactly why this client never derives archive
  rows from the window. Whether Core should also fold that recorded fact into
  the archive is a Core question, not a client change.
- The inspector offers one kind filter at a time. Core's `kinds` is an OR list,
  but the overlay has one cycling control, so sending several would claim a
  selection the operator never made. Owner filters below the opened scope, and
  a time range, have no control either.
- No row-level action inside the inspector: no "open in review" into a diff
  surface, no copy of an evidence id, no jump to the gate that requires it. The
  overlay browses and reads; it decides nothing.
- `QueryWorkspaceDiff` / `WorkspaceDiffLoaded` has no TUI reader yet: the TUI
  renders the diff Core attaches to an approval, not an operator diff pane.
- Core publishes no workspace-scoped operator identity (GUI-CORE-027), so the
  `/git` rows are actionable only for a Lane whose runtime owner Core has
  published. Every other target is refused locally with that reason stated. The
  TUI does not substitute a default owner, because the audit record such an
  action writes would then name nobody; the GUI refuses identically at
  `D1-OPERATOR-GIT-OWNER`. Lifting this needs a Core fact, not a client change.
