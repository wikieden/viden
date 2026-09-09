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

Deterministic preview states, generated by `scripts/tui-previews.sh` and
exported by `scripts/tui-regression.sh`:

- `main-approval-hunks`: the approval overlay with decision-context hunks, an
  omitted file, and the base note;
- `main-git-picker`: the `/git` picker with the target and source row;
- `main-git-outcome`: one `Completed` and one `Failed` outcome entry;
- `main-conflict-detail`: the conflict modal with three sides, an omitted file,
  and the truncation note.

## Known Gaps

- The conflict modal is reached from the supervision overlay's inspect row,
  which exists for merge-gate bounces. A Lane conflict states its counts on its
  transcript entry; the TUI has no pickable Lane-conflict row to open the modal
  from, and adding one is deferred with the rest of the Lane decision surface.
- The ambient pinned approval panel keeps the `input_preview` line. Hunk rows
  live in the approval overlay, which is where an approval is decided; the
  pinned panel is a fixed-height summary and growing it would push the pinned
  actions off a short terminal.
- The evidence inspector for `runtime.evidence_reads` is a separate batch.
- `QueryWorkspaceDiff` / `WorkspaceDiffLoaded` has no TUI reader yet: the TUI
  renders the diff Core attaches to an approval, not an operator diff pane.
- Core publishes no workspace-scoped operator identity (GUI-CORE-027), so the
  `/git` rows are actionable only for a Lane whose runtime owner Core has
  published. Every other target is refused locally with that reason stated. The
  TUI does not substitute a default owner, because the audit record such an
  action writes would then name nobody; the GUI refuses identically at
  `D1-OPERATOR-GIT-OWNER`. Lifting this needs a Core fact, not a client change.
