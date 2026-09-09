# Viden 0.3.3 Core Contract Increment - DiffReview Host And Evidence Reads

Chinese version: [release-0.3.3-contract-design.zh-CN.md](release-0.3.3-contract-design.zh-CN.md)

This is the design for the four additive capabilities that
[release-0.3.3-plan.md](release-0.3.3-plan.md) dispatches as batches C1 to
C4. It is a design, not a description of implemented behavior. Field names
and bounds here are the brief for implementation; a batch may deviate only
with the reason recorded in its report and, if the deviation survives review,
in this document.

Target: Core `0.3.6`, schema `1`, `compatibility = additive_capability_gated`.
Governing rules: the protocol evolution rules and command ownership table in
`docs/frontend-integration-contract.md`, the extension procedure in
`docs/core-0.3-compatibility.md`, and the registrations for `DiffReview` and
`EvidenceView` in the design package (`SPEC.md` decision `D-RAILNAV`,
`DESIGN-REF.md` section "D1 secondary views").

## Why One Increment

GUI-CORE-020, 012, and 015 were deferred separately, but they share one
surface, the registered `DiffReview` view, and one substrate: unified diff
text that Core already produces or consumes at four sites.

| Site | Today | Consumer |
| --- | --- | --- |
| tool result diff | `WorkspaceChangeView.patch: Option<String>` (64 KiB bound) | D1 changed-file chips |
| approval preview | `ApprovalRequestView.input_preview: String` | D1 permission dock, D2 |
| merge apply | `LocalPatchBackend::prepare` parses files and hunks internally | trust loop |
| lane apply | `LaneEffectRequest::Apply { unified_diff }` | lane runtime |

Designing the three requests separately would give three diff shapes and two
git action paths. This increment adds one typed `DiffDocument`, attaches it at
the sites above, adds one operator action path that reuses the agent tool
specs, and one conflict shape built from the same lines. Evidence reads are
the fourth capability because the `EvidenceView` design needs paging and
content that the recent-window `latest_evidence` projection cannot give.

## Shared Rules

Every capability below follows these rules; a batch report states each one
as verified.

- **Additive only.** Schema stays `1`. New fields carry `#[serde(default)]`
  and `skip_serializing_if` for options, so a record without the field
  serializes to the bytes it did before. New enums are `#[non_exhaustive]`.
- **Every new event is known.** The `is_known_runtime_event_type` arm, the
  reducer arm where the fact reaches `RuntimeViewState`, and an extension
  fixture land in the same commit. This was missed three times for earlier
  events; the fixture test is the guard.
- **Correlation, not inference.** A read or action command carries a
  client-chosen `command_id`; the answering event repeats it. A pre-effect
  refusal is `CommandRejected { command_id, reason }` with the actionable
  hint folded into `reason`, never an empty page and never a bare `Error`
  (the `QueryWorkspaceFiles` precedent in
  `crates/runtime/src/frontend_services.rs`).
- **Permission before effect, one vocabulary.** An operator action or
  workspace read passes `PermissionEngine::decide` on the same `ToolSpec` an
  agent turn would use, so one `viden.toml` allow/ask/deny rule set governs
  agents and operators. `Ask` routes through the owner-scoped supervisor
  approval queue as `ApprovalRequested`; `Deny` and plan mode reject before
  any process spawns.
- **Audit before mutation.** Each mutating action appends its audit record
  before the effect, as the trust loop does, and the finishing event carries
  the `audit_id`.
- **Bounded and honest.** Every payload is bounded by bytes and rows and
  carries `truncated` or `omitted` flags. `None` means Core did not know;
  it is never a default and never an inferred value.
- **Explicit target.** Actions and reads name their target: the workspace
  root or one Lane worktree. Core validates the Lane exists, is not archived,
  and resolves its worktree path itself; clients never pass paths.
- **Capability gating.** Each capability is appended to
  `FRONTEND_V1_EXTENSION_CAPABILITIES` and
  `crates/core/frontend-contract-extensions.toml` with its fixture and view
  digest. The nine frozen base fixtures stay byte-immutable, and the
  capability count gate in `scripts/tui-regression.sh` moves from 19 to 23.

## 1. `runtime.structured_diff` (C1, closes GUI-CORE-012)

The substrate; lands first.

### Types

New module `crates/types/src/diff.rs`:

```rust
pub struct DiffDocument {
    pub files: Vec<DiffFile>,
    pub truncated: bool,
    pub byte_limit: u32,
}

pub struct DiffFile {
    pub path: String,
    pub old_path: Option<String>,
    pub kind: WorkspaceChangeKind,
    pub binary: bool,
    /// The file is part of the change set but its hunks were dropped by the
    /// byte bound. `additions` and `deletions` stay real.
    pub omitted: bool,
    pub additions: u32,
    pub deletions: u32,
    pub hunks: Vec<DiffHunk>,
}

pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub header: Option<String>,
    pub lines: Vec<DiffLine>,
}

pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}

#[non_exhaustive]
pub enum DiffLineKind { Context, Added, Removed }
```

`WorkspaceChangeKind` is the existing closed enum in
`crates/types/src/frontend_services.rs`. Core is the only producer: the
unified-diff parser already in `crates/tools/src/patch.rs` (file headers,
hunk headers, old and new lines) is promoted to emit `DiffDocument`, and
clients never parse text into rows.

### Attachment sites

1. **Approval decision context.** `ApprovalRequestView` gains
   `decision_context: Option<DecisionContext>`:

   ```rust
   pub struct DecisionContext {
       pub diff: Option<DiffDocument>,
       /// SHA-256 of the file contents the diff was computed against, when
       /// the diff came from a proposed single-file mutation.
       pub base_sha256: Option<String>,
   }
   ```

   Produced for `edit_file` and `write_file` approvals by reading the current
   file read-only, applying the proposed replacement in memory, and diffing;
   no mutation happens at approval time. Produced for the trust-loop
   approval of `MergeAgentPatch` from the canonical patch bytes, which is the
   multi-file case GUI-CORE-012 asks for. Stated limitation: the preview is
   computed at approval time and execution runs the proposed tool input; a
   file changed in between can produce a different result, and `base_sha256`
   lets a client or an audit reader detect that afterwards. A pre-execution
   recheck is not in `0.3.3`.

2. **Workspace change facts.** `WorkspaceChangeView` gains
   `diff: Option<DiffDocument>` beside the existing `patch` string, under the
   same `MAX_COCKPIT_PATCH_BYTES` bound. `patch` stays for base clients.

3. **Operator diff read.**

   ```rust
   RuntimeCommand::QueryWorkspaceDiff { command_id: String, query: WorkspaceDiffQuery }

   pub struct WorkspaceDiffQuery {
       pub target: SourceTarget,
       pub scope: WorkspaceDiffScope,
       /// Empty means every changed path.
       pub paths: Vec<String>,
       /// Clamped to 1..=1 MiB; default 256 KiB.
       pub byte_limit: Option<u32>,
   }

   #[non_exhaustive]
   pub enum SourceTarget { Workspace, Lane { lane_id: AgentLaneId } }

   #[non_exhaustive]
   pub enum WorkspaceDiffScope { Worktree, Index, Both }

   RuntimeEventKind::WorkspaceDiffLoaded { command_id: String, page: WorkspaceDiffPage }

   pub struct WorkspaceDiffPage {
       pub target: SourceTarget,
       pub source: WorkspaceSourceView,
       pub entries: Vec<WorkspaceDiffEntry>,
       pub truncated: bool,
   }

   pub struct WorkspaceDiffEntry {
       pub path: String,
       pub index: Option<WorkspaceChangeKind>,
       pub worktree: Option<WorkspaceChangeKind>,
       pub staged: bool,
       pub diff: Option<DiffFile>,
   }
   ```

   Gate: `PermissionEngine::decide` on the existing non-mutating `git_diff`
   spec with the target root as input; refusal is `CommandRejected`. Source:
   `git status --porcelain=v2` for entry status, then `git diff` and
   `git diff --cached` per scope, run by Core in the target root. Untracked
   entries follow the `QueryWorkspaceFiles` exclusion list. The page is a
   query answer like `WorkspaceFilesLoaded`; it is not stored in
   `RuntimeViewState`, so no snapshot digest moves.

### Fixture `structured-diff.json`

An `edit_file` approval whose `decision_context.diff` holds one file and one
hunk; a `MergeAgentPatch` approval with a two-file diff; a
`WorkspaceDiffLoaded` page with two entries, one staged and one `omitted` by
the bound; a refused `QueryWorkspaceDiff` answered by `CommandRejected`.

### Clients

GUI: D2 and the D1 permission dock render hunk rows from `decision_context`
and drop the GUI-CORE-012 unavailable marker; the DiffReview file tree and
diff pane render `WorkspaceDiffLoaded`; D1 changed-file chips use
`WorkspaceChangeView.diff`. TUI: the approval overlay renders hunk rows when
the context is present and the preview text otherwise.

## 2. `runtime.operator_git` (C2, closes GUI-CORE-020)

### Types

```rust
RuntimeCommand::RunOperatorGitAction {
    command_id: String,
    target: SourceTarget,
    action: OperatorGitAction,
}

#[non_exhaustive]
pub enum OperatorGitAction {
    /// Empty paths means every changed path.
    Stage { paths: Vec<String> },
    Unstage { paths: Vec<String> },
    Commit { message: String },
    Push { remote: Option<String>, set_upstream: bool },
    Fetch { remote: Option<String> },
}

RuntimeEventKind::OperatorGitActionFinished {
    command_id: String,
    target: SourceTarget,
    action: OperatorGitAction,
    outcome: OperatorGitOutcome,
    audit_id: String,
}

#[non_exhaustive]
pub enum OperatorGitOutcome {
    /// `output` is bounded to 8 KiB; `source` is resampled after the effect.
    Completed { output: String, source: WorkspaceSourceView },
    Failed { class: OperatorGitFailureClass, detail: String },
}

#[non_exhaustive]
pub enum OperatorGitFailureClass {
    NothingToCommit,
    NonFastForward,
    AuthenticationRequired,
    RemoteUnreachable,
    NoUpstream,
    PathOutsideRepository,
    Other,
}
```

### Deliberate exclusions

- `pull`, `merge`, `rebase`: they move `HEAD` and can create conflicts, which
  belong to the Lane conflict machinery. `pull` is revisited in `0.3.4` once
  conflict content can render its result.
- `commit --amend`, `reset`, force push, branch delete: history rewrites with
  no affordance in the registered DiffReview and no undo story.
- `switch`, `checkout`: Lane identity is Core-owned through the starter Lane
  path; switching the workspace branch under a bound Lane invalidates its
  owner binding.
- `stash`: not in the design.

### Mapping and flow

Each action resolves to exactly one existing tool spec and input in
`crates/tools/src/git/`: `Stage` to `git_add`, `Unstage` to `git_restore`
with `staged=true`, `Commit` to `git_commit`, `Push` to `git_push`, and
`Fetch` to a new `git_fetch` spec (`is_mutating: true`, it writes refs).
The action runs through the same `BuiltinTool::run`, so an operator and an
agent produce identical effects and are governed by identical rules. Risk
classification: `Push` is `High`, `Commit` is `Medium`, the rest `Low`.

Flow: validate target and action; plan mode rejects; `PermissionEngine::decide`
on the mapped spec; `Allow` proceeds, `Ask` publishes `ApprovalRequested`
with `target.kind = "git"` and, for `Commit`, a `decision_context` holding
the staged diff; `Deny` rejects. Then the audit record (`AuditObjectRef`
kind `source`, plus the Lane ref for a Lane target) is appended, the tool
runs, `OperatorGitActionFinished` is emitted, and `WorkspaceSourceUpdated`
follows with the resampled source. A failure after a granted permission is
an event with a `Failed` outcome, because the effect was attempted and the
audit record carries it; `CommandRejected` is only for pre-effect refusals.
Core classifies the failure from git's stderr in one place so clients never
parse output text.

A Lane target validates that the Lane exists and is not archived. The
Lane's agent mutation policy governs the agent, not the operator; the
operator action is still permission-gated and audited against the Lane.

### Fixture `operator-git.json`

A `Stage` refused by policy answered by `CommandRejected`; a `Commit` on the
`Ask` path with `ApprovalRequested`, `ApprovalResolved`,
`OperatorGitActionFinished { Completed }`, and `WorkspaceSourceUpdated`
showing `ahead` incremented and `dirty` false; a `Push` finishing with
`Failed { NoUpstream }`. Outputs are fixed strings with no machine path.

### Clients

GUI DiffReview commit bar: Stage all, Commit, and Commit and Push, where the
last is two sequential commands and the second is sent only after the first
reports `Completed`. The titlebar sync chip becomes a control: push when
`ahead > 0`, fetch otherwise, disabled and labelled rather than hidden when
the capability is absent. TUI: a `/git` system command with selector rows
for stage, commit, push, and fetch sending the same command; outcomes render
as typed system entries with localized failure classes. Neither client
infers success from output text.

## 3. `runtime.conflict_content` (C3, closes GUI-CORE-015)

### Types

```rust
pub struct ConflictContent {
    pub baseline: ConflictBaseline,
    pub files: Vec<ConflictFile>,
    pub truncated: bool,
}

#[non_exhaustive]
pub enum ConflictBaseline {
    Revision { sha: String },
    Evidence { bindings: Vec<ReviewedEvidenceBinding> },
    Unknown,
}

pub struct ConflictFile {
    pub path: String,
    pub hunks: Vec<ConflictHunk>,
    pub omitted: bool,
}

pub struct ConflictHunk {
    /// Current file region at the hunk's old range.
    pub ours_start: u32,
    pub ours: Vec<String>,
    /// Incoming hunk lines from the patch.
    pub theirs_start: u32,
    pub theirs: Vec<String>,
    /// The patch preimage: what the hunk expected to find.
    pub base: Option<Vec<String>>,
    pub reason: ConflictHunkReason,
}

#[non_exhaustive]
pub enum ConflictHunkReason {
    ContextMismatch,
    AlreadyApplied,
    FileMissing,
    FileDeleted,
    Binary,
}
```

Attached as `ConflictBounce.content: Option<ConflictContent>` for a bounce
raised by a failed merge, and as `LaneConflictView.content` plus
`LaneConflictDetected.content` for the Lane apply path.

### Producer

When `LocalPatchBackend::prepare` or `LaneEffectRequest::Apply` rejects a
hunk, Core captures `theirs` from the incoming hunk, `ours` from a read-only
read of the current file at the hunk's old range, and `base` from the hunk's
own preimage lines. That preimage is exactly the baseline the conflict was
computed against, so nothing is invented. `ConflictBaseline` is `Evidence`
when the gate holds canonical baseline bindings (already on
`ConflictBounce.baseline_evidence`), `Revision` when the Lane's
`base_revision` is known and no bindings exist, otherwise `Unknown`. A bounce
raised by an operator through `BounceMergeConflict` with a reason has no
apply failure behind it and therefore no content. Bound: 256 KiB with
per-file `omitted`.

This content shows two sides and the preimage. It is not a three-way merge
and must not be presented as one.

### Fixture `conflict-content.json`

Two Lanes and one file: Lane A's patch merges (`MergeGateUpdated` to
`merged`); Lane B's `MergeAgentPatch` fails on the same file and
`MergeConflictBounced` carries content with one hunk (ours, theirs, base);
plus one `LaneConflictDetected` with content for the Lane apply path. The
view digest includes the bounce.

### Clients

GUI D12 renders ours and theirs side by side with the base row and the
reason; the GUI-CORE-015 unavailable marker is dropped. TUI: the decisions
overlay shows `n files · m hunks` and a detail modal lists hunks with
registered glyphs only.

## 4. `runtime.evidence_reads` (C4, opens and closes GUI-CORE-025)

`RuntimeViewState.latest_evidence` is a recent-window projection. The
`EvidenceView` design needs a day-grouped archive with paging and the
content behind a row, so this capability adds two reads. The GUI opens
GUI-CORE-025 for them in the same batch that closes it.

### Types

```rust
RuntimeCommand::QueryEvidence { command_id: String, query: EvidenceQuery }

pub struct EvidenceQuery {
    /// Prefix scope match on the owner, as `AuditQuery.lane_id` does.
    pub owner: Option<RuntimeOwner>,
    /// Empty means every kind.
    pub kinds: Vec<String>,
    /// Clamped to 1..=200.
    pub limit: u16,
    pub after: Option<String>,
}

RuntimeEventKind::EvidencePageLoaded { command_id: String, page: EvidencePage }

pub struct EvidencePage {
    pub entries: Vec<EvidenceView>,
    pub complete: bool,
    pub next_after: Option<String>,
}

RuntimeCommand::ReadEvidenceContent { command_id: String, evidence_id: EvidenceId }

RuntimeEventKind::EvidenceContentLoaded {
    command_id: String,
    evidence_id: EvidenceId,
    content: EvidenceContent,
}

#[non_exhaustive]
pub enum EvidenceContent {
    /// Bounded to 256 KiB.
    Text { text: String, truncated: bool, sha256: String },
    Diff { document: DiffDocument, sha256: String },
    Unavailable { reason: EvidenceUnavailableReason },
}

#[non_exhaustive]
pub enum EvidenceUnavailableReason {
    SummaryOnly,
    MissingCanonicalBytes,
    HashMismatch,
    Binary,
}
```

### Semantics

Ordering is stable on `(timestamp, id)` and the cursor is opaque. Content is
read only from the canonical ContextStore bytes named by
`EvidenceView.canonical`. Display-only evidence such as `task_summary`
answers `Unavailable { SummaryOnly }`; bytes whose hash no longer matches
answer `Unavailable { HashMismatch }` and are never served as canonical.
Evidence of kind `patch` answers `Diff` through the same parser as
capability 1. Gate posture matches `QueryAudit`: bounded and owner-scoped,
not tool-gated, because the evidence store is Viden's own state rather than
the workspace.

### Fixture `evidence-reads.json`

Mirrors `audit-reads.json`: two pages, a kind filter, a text content read, a
summary-only `Unavailable`, and an over-limit query answered by
`CommandRejected`.

### Clients

GUI EvidenceView: day-grouped list, detail with key-value report and output
tail, linked chips from `metadata` and `canonical`, and "Open in review" for
`patch` kind into DiffReview. TUI: the evidence inspector deferred in the
`0.3.2` supervision checkpoints, opened from the decisions overlay.

## Bookkeeping Per Capability

- `viden-types`: types, reducer arms for view-bearing facts (approval
  context, workspace change diff, bounce and Lane conflict content), and the
  known-event arm.
- `viden-runtime`: producers, gates, audit records, failure classification.
- `viden-core`: capability constant, extension fixture with its refresh test,
  manifest digests.
- Docs in both languages: `docs/core-0.3-compatibility.md` capability list
  and fixture corpus; `docs/frontend-integration-contract.md` command
  ownership rows and a new "Source Control And Diff UI Contract" section;
  `apps/gui/contract-requests.md` close entries with dates;
  `scripts/tui-regression.sh` capability count.
- Landing order: 1, then 2 and 3 in parallel, then 4. Each is its own
  branch and review.

## Decided Defaults

These are the calls made in this design; the operator may override any of
them before the batch that depends on it is dispatched.

- `Fetch` is a mutating spec because it writes refs.
- "Commit and Push" is two commands, never one composite effect.
- Operator actions on a Lane worktree are allowed regardless of the Lane's
  agent mutation policy, always permission-gated and audited against the
  Lane.
- Diff and conflict payload bounds: 64 KiB for workspace change facts,
  256 KiB default and 1 MiB maximum for diff reads, 256 KiB for conflict
  content and evidence text.
