# Viden 0.3.4 Core Contract Increment - Workspace Owner, Turn Lifecycle, Durable Work Evidence, Transcript Rows, Workspace File Reads

Chinese version: [release-0.3.4-contract-design.zh-CN.md](release-0.3.4-contract-design.zh-CN.md)

This is the design for the additive capabilities that
[release-0.3.4-plan.md](release-0.3.4-plan.md) dispatches as batches C5 to
C9. It is a design, not a description of implemented behavior. Field names
and bounds here are the brief for implementation; a batch may deviate only
with the reason recorded in its report and, if the deviation survives review,
in this document.

Target: Core `0.3.7`, schema `1`, `compatibility = additive_capability_gated`.
Governing rules: the protocol evolution rules and command ownership table in
`docs/frontend-integration-contract.md`, the extension procedure in
`docs/core-0.3-compatibility.md`, and the Shared Rules of
`release-0.3.3-contract-design.md`, which apply unchanged (additive only,
every new event known with a fixture in the same commit, `command_id`
correlation with `CommandRejected`, permission before effect on the same
`ToolSpec` vocabulary, audit before mutation, bounded and honest, explicit
target, capability gating with the nine base fixtures byte-immutable).

Written 2026-09-12 from a read of `main` at `25072a0a`. Facts cited below
were verified at that commit.

## Why These Six

The `0.3.3` real task stopped at four Core facts that do not exist:

| Missing fact | Where it stopped | Capability |
| --- | --- | --- |
| an operator identity for the workspace target | commit and push refused without a Lane (`RuntimeOwner::default()` everywhere, `crates/types/src/protocol.rs:64`) | 1 `runtime.workspace_owner` |
| a terminal fact for a built-in turn | `assistant_stream` never settles for the native path (`crates/types/src/runtime.rs:1204`); the session queue is pushed but never drained (`runtime_contract.rs:640`, no `InputDequeued` producer) | 3 `runtime.turn_lifecycle` |
| the archive seeing supervisor-driven work | `WorkspaceChangeUpdated` is not durable (`session_lifecycle.rs:1292`); ACP patch evidence is built with `canonical: None` (`crates/agents/src/glue.rs:1295`, `:1360`); `RespondToApproval` never writes an audit row (`runtime_supervisor.rs`) | 4 `runtime.durable_work_evidence` |
| an owner-scoped ordered transcript | GUI-CORE-009; the base `runtime.transcript_page` is unscoped and single-session | 5 `runtime.transcript_rows` |

Two more are needed by the cockpit alignment: a client-visible layout
preference that cannot ride on `UiPreferences` without moving every base
fixture digest (`ResolvedUiPreferences` always serializes into
`RuntimeViewState`, `crates/types/src/runtime.rs:1029`), and a read of one
workspace file behind the Files tab and the palette's file rows.

Capability count: 23 to 29 (the plan said 28; the layout preference is the
sixth, per the plan's own gate "if any base digest moves, ships as a separate
preference record instead"). The `0.3.4` plan is amended with this count.

## 1. `runtime.workspace_owner` (C5, closes GUI-CORE-027)

### Types

```rust
// crates/types/src/workspace_owner.rs (new)

/// The owner Core publishes for work that is scoped to the workspace root
/// rather than to one Lane. `lane_id`, `session_id`, `task_id`, and `turn_id`
/// are `None`; `workspace_id` and `project_id` are never empty.
#[derive(Serialize, Deserialize)]
pub struct WorkspaceRuntimeOwnerBinding {
    pub canonical_root: String,
    pub owner: RuntimeOwner,
    /// How `project_id` was obtained: read from `.viden/project.toml` or
    /// minted on this open and written there.
    pub project_id_origin: ProjectIdOrigin,
}

#[non_exhaustive]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectIdOrigin { Existing, Minted }
```

Events, commands, view state:

- `RuntimeEventKind::WorkspaceRuntimeOwnerBound { binding: WorkspaceRuntimeOwnerBinding }`
  is emitted once per open, as the first fact after `SnapshotUpdated` in
  `runtime_state_events` so a snapshot replay carries it, and again whenever
  the workspace is rebound.
- `RuntimeViewState.workspace_owner: Option<RuntimeOwner>` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]`. Absent means
  Core has not published one; the nine base fixtures never carry it, so their
  bytes do not move.
- `RuntimeEventKind::LaneSourceUpdated { lane_id, source: WorkspaceSourceView }`
  and `RuntimeViewState.lane_sources: BTreeMap<AgentLaneId, WorkspaceSourceView>`
  with `skip_serializing_if = "BTreeMap::is_empty"`. `WorkspaceSourceUpdated`
  keeps its meaning (the workspace root only).

### Semantics

- **Minting.** `workspace_id` is `ws_` followed by the first 16 hex chars of
  SHA-256 over the canonical root path: a stable identity for this location
  on this machine, derivable without a store and stated as such (moving the
  repository changes it). `project_id` is read from `.viden/project.toml`
  (`[project] id = "prj_<ulid>"`); when absent, Core mints one and writes the
  file on open. `.viden/` is already Core-owned and never committed.
  `LocalCoreHost::open_workspace` (`crates/core/src/host.rs:166`) performs
  both and `WorkspaceBinding` gains the two ids.
- **One source of owners.** `SessionEngine::workspace_owner()` returns the
  bound owner; every place that today builds an owner from
  `RuntimeOwner::default()` for a NEW binding (Lane bindings in
  `crates/lanes/src/lane_supervisor.rs:1114`, Agent session owners in
  `runtime_supervisor.rs:584`) starts from it, so a Lane owner carries the
  workspace and project ids. Existing empty-owner facts (the built-in turn's
  tool facts in `runtime_loop.rs:82`) are re-stamped by C7, not here.
- **Operator git on the workspace target.** `RunOperatorGitAction` with
  `SourceTarget::Workspace` is accepted when the command's `owner` equals the
  published workspace owner (the existing envelope-actor equality in
  `project_runtime.rs:222` does this once clients send that owner). The
  audit records name that owner. Both clients drop their local refusals
  (`D1-OPERATOR-GIT-OWNER`, the TUI `/git` refusal).
- **Per-Lane source.** `LaneSourceUpdated` is sampled with the existing
  `sample_workspace_source` against the Lane worktree at Lane worktree
  creation, after every operator git action whose target is that Lane, and
  on each snapshot prefix for every live Lane with a worktree. A Lane with no
  worktree of its own has no row (it is the workspace).

### Fixture `workspace-owner.json`

Open → `WorkspaceRuntimeOwnerBound` (project id minted) → a Lane created
whose binding carries the same workspace and project ids →
`RunOperatorGitAction { target: Workspace, action: Commit }` accepted and
audited under the workspace owner → `LaneSourceUpdated` for the Lane after a
Lane-target `Stage`. The digest test proves the nine base fixtures are
byte-identical with the new field absent.

### Clients

GUI: commit bar and sync chip enabled on the workspace target when
`workspace_owner` is present; the Lane tab strip and context dock read
`lane_sources[lane]` for a Lane's branch and ahead/behind; without the
capability both keep today's refusal text. TUI: `/git` workspace rows enabled
under the same condition.

## 2. `ui.layout_preferences` (C5, cockpit layout)

### Types

```rust
// crates/types/src/ui_preferences.rs (additions)

#[derive(Serialize, Deserialize, Default)]
pub struct UiLayoutPreferences {
    #[serde(default)]
    pub lane_sidebar_mode: LaneSidebarMode,
    /// Ambient statusbar segments the operator hid; identity and actionable
    /// segments are never listed here.
    #[serde(default)]
    pub hidden_statusbar_segments: Vec<String>, // bounded MAX_HIDDEN_STATUSBAR_SEGMENTS = 16
}

#[non_exhaustive]
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LaneSidebarMode { #[default] Pinned, Floating } // amended 2026-09-12, see Amendments

pub struct UiLayoutPreferencePatch {
    pub lane_sidebar_mode: Option<LaneSidebarMode>,
    pub hidden_statusbar_segments: Option<Vec<String>>,
}
```

- Commands: `SetUiLayoutPreferences { command_id, patch }` and
  `ResetUiLayoutPreferences { command_id }`. Event:
  `UiLayoutPreferencesUpdated { command_id: Option<String>, preferences, persisted: bool }`
  (`command_id` absent on the snapshot prefix).
- View state: `RuntimeViewState.layout_preferences: Option<UiLayoutPreferences>`
  with `skip_serializing_if = "Option::is_none"`. This is the whole point of a
  separate record: `ResolvedUiPreferences` stays untouched and the base
  digests stay put.
- Persistence: `[ui.layout]` table beside `[ui]` in the same config file,
  through the existing `apply_patch_to_table` pattern
  (`crates/config/src/ui_preferences.rs:186`); `persisted: false` when the
  file could not be written, with the reason in a diagnostic, mirroring
  `ui.preference_persistence`.

### Fixture `ui-layout-preferences.json`

Set floating → updated and persisted → reset → default pinned (amended
2026-09-12, see Amendments: the default is floating, so the shipped sequence
is floating default → set pinned → reset back to floating); an unknown
segment name in `hidden_statusbar_segments` is kept verbatim (Core does not
know the client's segment vocabulary) but the list is bounded and refused
over the bound.

## 3. `runtime.turn_lifecycle` (C6)

### Types

```rust
// crates/types/src/turn_lifecycle.rs (new)

#[derive(Serialize, Deserialize)]
pub struct TurnView {
    pub turn_id: String,
    pub owner: RuntimeOwner,          // lane_id None for the session-scoped composer
    pub source: TurnSource,
    pub started_at: u64,
}

#[non_exhaustive]
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnSource {
    UserInput,
    QueuedInput { input_id: String },
    AgentSession { session_id: SessionId },
}

#[non_exhaustive]
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnOutcome {
    Completed,
    Failed { reason: String },      // sanitized, bounded 500
    Cancelled,
}
```

Events: `TurnStarted { turn: TurnView }`, `TurnFinished { turn_id, owner, outcome: TurnOutcome, finished_at }`.
View state: `RuntimeViewState.active_turns: Vec<TurnView>` with
`skip_serializing_if = "Vec::is_empty"`; `TurnStarted` upserts by `turn_id`,
`TurnFinished` removes.

### Semantics

- **Native path.** `SubmitUserInput` (`runtime_contract.rs:606`) emits
  `TurnStarted` immediately after `CommandAccepted`, with `owner` = the
  workspace owner from C5 (or the empty owner if C5 is absent) plus a fresh
  `turn_id`, and `TurnFinished` as the last fact of the turn on every exit:
  `Completed` on success, `Failed { reason }` on the engine error path beside
  the existing `Error` event, `Cancelled` when `CancelActiveTurn` stops it.
  The trailing `SnapshotUpdated` stays; it is no longer what clients read as
  the end.
- **ACP path.** A turn is one Agent session run: `TurnStarted` when the
  session enters `Running` for a prompt, `TurnFinished` beside the terminal
  `AgentSessionCompleted`/`Failed`/`Updated{Cancelled}` (`glue.rs:807`), with
  the session owner. Nothing else about Agent sessions changes.
- **Stream settlement.** The reducer clears `assistant_stream` on
  `TurnFinished` whose owner has `lane_id: None`; the existing
  `settles_turn` clear (`runtime.rs:1225`) stays for Agent-session facts.
- **Queue drain.** On `TurnFinished { outcome: Completed }` for the
  session-scoped owner, Core pops the oldest `queued_runtime_inputs` entry,
  emits `InputDequeued { input_id }`, and runs it as a new turn with
  `TurnSource::QueuedInput { input_id }`, repeating until the queue is empty
  or a turn does not complete. On `Failed` or `Cancelled` the queue is kept
  and re-listed by the snapshot prefix; nothing runs behind a failed turn.
  The Lane worker's own queue (`lane_worker.rs:959`) is unchanged.
- **Crash safety.** A turn is never resumed across a restart: the snapshot
  prefix re-emits only turns Core is running now, so a client that reconnects
  sees an empty `active_turns` and a queue it can inspect.

### Fixture `turn-lifecycle.json`

Submit → `TurnStarted { UserInput }` → deltas → `TurnFinished { Completed }`
with `assistant_stream` empty afterwards → two `QueueFollowUp` while a second
turn runs → `TurnFinished` → `InputDequeued` + `TurnStarted { QueuedInput }`
for the first, then the second → a cancelled third turn leaves a later
queued input queued.

### Clients

GUI: composer busy = an `active_turns` entry for its owner (replacing the
`turn_id` heuristics), "Queued" copy only while `queued_inputs` is non-empty
and a turn is active, "queued · waits for the next completed turn" after a
failure. TUI: `native_turn` window replaced by `active_turns`; the residue
statement in `docs/release-tui-0.3.4-source-control-parity.md` is closed.

## 4. `runtime.durable_work_evidence` (C7, closes GUI-CORE-028 and E1 defect 4)

### Shapes

No new types. The change is which facts reach the archive and the audit
log, and one new use of an existing event:

- An applied `write_file`/`edit_file` (`record_completed_tool`,
  `runtime_loop.rs:82`) produces, beside the live `WorkspaceChangeUpdated`,
  an `EvidenceRecorded { evidence }` with `kind: "patch"`, `id:
  "patch-<tool_call_id>"`, `owner` = the turn's owner, `timestamp`, `path`,
  `summary`, and `canonical: Some(CanonicalEvidenceReference)` whose bytes are
  the unified diff stored through `ContextEngine::store` (`ContextPutRequest
  { scope: the owner's evidence scope, kind: Patch, content, evidence_id }`),
  `source_hash` = SHA-256 of those bytes, `producer { identity: "native",
  role: "coder", task_id: <turn_id> }`, `permission_snapshot_id` = the
  approval's audit id when the mutation was approved, `verification:
  Verified`, `quality` as the store reports. A diff over the 64 KiB cockpit
  bound is still stored in full up to `MAX_EVIDENCE_CONTENT_BYTES`; beyond
  that the row is recorded with `canonical: None` and a summary saying so.
- An ACP patch fact (`glue.rs:1295`, `:1360`) keeps `canonical: None` at
  construction (the agents crate does not own a store) and the runtime's
  ingestion of it stores the bytes carried in `metadata` and publishes the
  existing `EvidenceCanonicalized { evidence_id, canonical }`.
- Durability: the supervisor hands every terminal turn batch to the engine
  through one new entry point, `SessionEngine::absorb_supervised_events`,
  which applies `is_durable_runtime_domain_event`, remembers, and persists
  through the existing `persist_workflow_runtime_projection_batch`. This is
  the wiring that has never existed; `WorkspaceChangeUpdated` and
  `CheckRunUpdated` stay live-only, the new `EvidenceRecorded` does not.
- `RespondToApproval` appends one `AuditRecord` before `ApprovalResolved`:
  `audit_id` = the pre-minted id the request already shows, `actor:
  Operator`, `action: "approval.<allow_once|allow_session|allow_repo|deny>"`,
  `objects: [approval request id, tool call id]`, `outcome: Applied`, `args`
  = scope. The supervisor gets a narrow audit-append handle for this.
- Merge gate: a native Lane's archived `patch` row satisfies a `patch`-
  required gate when `producer.task_id` matches the gate's task; a
  session-scoped turn's row never does (it names no task), and the gate says
  which.

### Fixture `durable-work-evidence.json`

Approve an `edit_file` → `ApprovalResolved` → audit row readable through
`QueryAudit` → `EvidenceRecorded { patch, canonical }` → `QueryEvidence`
returns it → `ReadEvidenceContent` answers `Diff` with the verified bytes →
an ACP patch fact followed by `EvidenceCanonicalized`.

### Clients

GUI: EvidenceView shows the row; D14 shows the approval; D2/permission dock
"audit" links resolve. TUI: evidence inspector detail now has live content.

## 5. `runtime.transcript_rows` (C8, closes GUI-CORE-009)

### Types

```rust
// crates/types/src/transcript_rows.rs (new)

pub const MAX_TRANSCRIPT_ROWS_PAGE: u16 = 200;
pub const DEFAULT_TRANSCRIPT_ROWS_PAGE: u16 = 50;
pub const MAX_TRANSCRIPT_ROW_TEXT_BYTES: u32 = 8 * 1024;

pub struct TranscriptRowsQuery {
    pub owner: RuntimeOwner,          // prefix scope, same matcher as EvidenceQuery
    pub before: Option<String>,       // opaque cursor from a previous page
    pub limit: Option<u16>,           // clamps
}

pub struct OwnedTranscriptRow {
    pub id: String,
    pub owner: RuntimeOwner,          // full owner, never widened
    pub sequence: u64,
    pub timestamp: Option<u64>,
    pub content: TranscriptRowContent,
}

#[non_exhaustive]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TranscriptRowContent {
    User { text: String, truncated: bool },
    Assistant { text: String, truncated: bool },
    ToolCall { call: ToolCallView },
    ToolResult { tool_call_id: String, success: bool, summary: String, evidence_id: Option<String> },
    CheckRun { check: CheckRunView },
    Permission { request_id: String, decision: Option<ApprovalDecision>, audit_id: String },
}

pub struct TranscriptRowsPage { pub rows: Vec<OwnedTranscriptRow>, pub older: Option<String>, pub complete: bool }
```

Commands and events: `QueryTranscriptRows { command_id, query }` →
`TranscriptRowsLoaded { command_id, page }`; refusal is `CommandRejected`.
Not reduced into `RuntimeViewState`.

### Semantics

- Rows are derived from durable facts Core already keeps: the native session
  transcript (`SessionStore`) for the session-scoped owner and the Agent
  session conversation and tool facts for Lane owners, ordered by their
  recorded sequence, newest last within a page, paged backwards with
  `before`. Text over the bound is cut on a char boundary with `truncated`
  and, where a canonical evidence row exists for the body, `evidence_id`
  names it.
- Owner scoping fails closed both ways, exactly as `EvidenceQuery.matches`:
  a row whose owner is unknown never answers a scoped query. The base
  `runtime.transcript_page` is untouched.

### Fixture `transcript-rows.json`

Two Lanes with interleaved turns and one session-scoped turn; three queries
(each Lane, the session) prove no row crosses owners; a page boundary; a
truncated assistant row naming its evidence.

### Clients

GUI: the D1 transcript renders ordered `User`/`Assistant` rows and retires
the `transcript_user`/`transcript_assistant` unavailable placeholders; tool
blocks keep their inline diff from `WorkspaceChangeView.diff`. TUI: the
transcript lens reads the same rows for the focused Lane.

## 6. `runtime.workspace_file_reads` (C9)

### Types

```rust
// crates/types/src/workspace_files.rs (additions)

pub const MAX_WORKSPACE_FILE_BYTES: u32 = 1024 * 1024;
pub const DEFAULT_WORKSPACE_FILE_BYTES: u32 = 256 * 1024;

pub struct WorkspaceFileReadQuery {
    pub target: SourceTarget,
    pub path: String,                 // project-relative; rejected, not clamped, if absolute, `..`, control chars
    pub byte_limit: Option<u32>,      // clamps 1..=MAX
}

pub struct WorkspaceFileContent {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub content: WorkspaceFileBody,
}

#[non_exhaustive]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceFileBody {
    Text { text: String, truncated: bool },
    Binary,
    Unavailable { reason: WorkspaceFileUnavailableReason },
}

#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceFileUnavailableReason { NotFound, Directory, Unreadable }
```

Commands and events: `ReadWorkspaceFile { command_id, query }` →
`WorkspaceFileLoaded { command_id, file }`. Not reduced into
`RuntimeViewState`.

### Semantics

- Gate on the real `read_file` `ToolSpec` from the registry
  (`self.tools.spec("read_file")`, the operator-git precedent,
  `operator_git.rs:366`), input `path` = the resolved absolute path, through
  `PermissionEngine::decide` non-interactively: `Allow` reads, `Ask` and
  `Deny` are `CommandRejected` with the rule named (the `QueryWorkspaceDiff`
  precedent). `read_file`'s own `resolve_path` scopes nothing, so the path
  rule above and `resolve_source_target_root` are what keep the read inside
  the target.
- `Binary` when the first 8 KiB contain a NUL byte or the bytes are not
  valid UTF-8; `Text` is cut on a char boundary with `truncated`; `sha256`
  is over the whole file. Symlinks resolving outside the target are
  `Unreadable`.

### Fixture `workspace-file-reads.json`

A text read, a truncated read, a binary, a missing path, a directory, a
`..` refusal, and a deny-rule refusal.

### Clients

GUI: Files tab content and Code tab, palette `~` rows open the file in the
Code tab. TUI: no parity minimum (the TUI has no file viewer registered);
recorded as such.

## Bookkeeping Per Capability

- `viden-types`: types, reducer arms for view-bearing facts (workspace owner,
  lane sources, layout preferences, active turns), known-event arms.
- `viden-runtime`: producers, gates, audit records, the supervisor-to-engine
  absorption path (C7), queue drain (C6).
- `viden-core`: capability constants, `WorkspaceBinding` ids, extension
  fixtures with refresh tests, manifest digests.
- `viden-config`: `[ui.layout]`.
- Docs in both languages: `docs/core-0.3-compatibility.md` capability list
  and corpus; `docs/frontend-integration-contract.md` ownership rows and a
  "Workspace Identity, Turn Lifecycle, And Durable Evidence" section;
  `apps/gui/contract-requests.md` closes for 009, 027, 028;
  `scripts/tui-regression.sh` and `apps/tui/src/tui/client.rs` counts 23 to
  29.
- Landing order: C5 (1 and 2 together), then C6, then C7 and C9 in parallel,
  then C8. Each is its own branch and review.

## Decided Defaults

The operator may override any of these before the batch that depends on it
is dispatched.

- `workspace_id` is derived from the canonical root; `project_id` is a
  minted ULID persisted in `.viden/project.toml`.
- The layout preference is a separate record and capability rather than a
  `UiPreferences` field, because the latter moves all nine base digests.
- The session queue drains only after a `Completed` turn; failed and
  cancelled turns leave it queued and visible.
- Native patch evidence names the turn as its producer task; a session-scoped
  turn's evidence never satisfies a Lane's merge gate.
- File reads gate on the real `read_file` spec, non-interactively.
- Bounds: 8 KiB per transcript row text, 256 KiB default and 1 MiB maximum
  per file read, 16 hidden statusbar segments.

## Amendments From Implementation (2026-09-12)

Recorded by batch C5 as the contract design's own rules require: a batch may
deviate only with the reason in its report and, if the deviation survives
review, here. The body above is left as written and the amended lines are
marked; this section is what shipped.

- **`LaneSidebarMode` defaults to `Floating`, not `Pinned`.** The body's
  `#[default] Pinned` contradicted design decision `D-SIDEBAR` in
  `docs/viden-design/Viden/docs/SPEC.md`, which makes `float` the default mode
  (hover peek, horizontal space to the transcript) and `pinned` the
  alternative. The design was wrong, not the design package; the GUI batch is
  aligned to `D-SIDEBAR` as well. The fixture now starts from the floating
  default, sets pinned, and resets back to floating, so the set and the reset
  land on different modes and neither is the fixture's own default.
- **`SetUiLayoutPreferences { patch }` and `ResetUiLayoutPreferences` carry no
  `command_id` field.** No `RuntimeCommand` variant does: the id lives on
  `RuntimeCommandEnvelope` and the answering event repeats it, as
  `QueryWorkspaceDiff` → `WorkspaceDiffLoaded` already does. The correlation
  the design asked for is unchanged.
- **`UiLayoutPreferencesUpdated` gained `diagnostics: Vec<UiPreferenceDiagnostic>`.**
  The design says `persisted: false` carries "the reason in a diagnostic" but
  gave the event no field to carry one. Optional and skipped when empty.
- **`MAX_HIDDEN_STATUSBAR_SEGMENT_BYTES = 64` was added.** The design bounds
  the list's length but not each name, which leaves a "bounded" list unbounded
  in bytes. Blank names and control characters are refused on the same ground.
- **`project_id` is `prj_<nanosecond token>`, not a ULID.** No ULID crate is a
  dependency and the batch was not authorized to add one; `fresh_id` is the
  existing helper. Prefix and shape match the design.
- **Minting lives in `viden-runtime`, called from `LocalCoreHost::open_workspace`.**
  The workspace-id digest needs `sha2`, a dependency of `viden-runtime` and
  only a dev-dependency of `viden-core`. The host still performs both steps at
  open.
- **Resetting the appearance profile preserves `[ui.layout]`.** The design puts
  the layout table under `[ui]`, which the appearance reset removed wholesale;
  the two records answer to two different commands, so an operator resetting a
  theme must not find their cockpit rearranged.
- **A Lane-target operator git action publishes `LaneSourceUpdated` instead of
  `WorkspaceSourceUpdated`.** Required by the design's own "`WorkspaceSourceUpdated`
  keeps its meaning (the workspace root only)"; it had been putting one tree's
  branch and ahead/behind into another tree's chip.
- **Workspace-target authorization compares the workspace and project ids, not
  the whole owner.** A Lane-scoped operator acting on the workspace root is a
  real case and its audit record should name the Lane it came from. Refused is
  an owner naming *no* workspace — the `RuntimeOwner::default()` case
  GUI-CORE-027 is about — or naming a different one.
- **"Sampled at Lane worktree creation" is implemented in the Lane event sink,
  once per Lane**, on the first `LaneUpdated` announcing an existing worktree.
  Lane creation is dispatched to an asynchronous Lane worker, so at dispatch
  time the worktree does not exist yet; sampling on every `LaneUpdated` would
  put several bounded `git` invocations on that worker's hot path.
- **"On each snapshot prefix for every live Lane" is implemented in the status
  lifecycle sampling** — connect, every snapshot request, and every completed
  supervised command, which is where the workspace source is already sampled —
  rather than in `runtime_state_events`. That prefix is rebuilt for every
  command, so sampling N Lanes there would spawn 5N `git` processes per
  command. The snapshot envelope still carries the rows, because the sampling
  runs immediately before the envelope is built.

### C6 `runtime.turn_lifecycle` (accepted on review, 2026-09-12)

- **The turn owner is the command's own scope plus a fresh `turn_id`, not the
  bare workspace owner.** The supervised input path also serves a Lane's
  native turn (`lane_id: Some`); forcing the workspace owner there would hide
  a Lane's own work from its own composer. The session-scoped composer gets
  exactly the design's owner: `workspace_owner()` when bound, the empty owner
  when not, never invented.
- **The session queue drain is *armed* by a completed turn rather than fired
  only at the instant of completion.** The supervisor is one worker, so a
  `QueueFollowUp` sent while a turn runs is still in its channel when the turn
  ends; a literal "drain at `TurnFinished { completed }`" would leave the
  common case queued forever. A completed session-scoped turn arms the drain,
  a follow-up landing while it is armed runs immediately, and any turn that
  does not complete disarms it. Nothing runs behind a failed or cancelled
  turn. A Lane's native turn neither arms nor disarms it.
- **Neither turn fact is persisted.** A persisted `TurnStarted` whose process
  died before its `TurnFinished` would replay as a phantom running turn Core
  cannot cancel. Both are live-sink-only and excluded from
  `is_durable_runtime_domain_event`; a restart yields empty `active_turns`.
  The session queue itself is in-memory and survives a reconnect, not a
  restart (pre-existing, unchanged).
- **ACP `TurnStarted` is emitted beside `AgentSessionStarted`.** There is no
  separate "enters `Running` for a prompt" transition on the ACP start path;
  `turn_id` is the runner's existing artifact id.
- **A drained turn hitting the context hard limit answers with `Error`, not
  `CommandRejected`.** It has no command in flight, and rejecting the
  original command id would settle a request the client already saw accepted.

### C9 `runtime.workspace_file_reads` (accepted on review, 2026-09-12)

- **`ReadWorkspaceFile { query }` carries no `command_id` field.** Same
  convention as C5: the id lives on the envelope and
  `WorkspaceFileLoaded { command_id, file }` repeats it.
- **`size` and `sha256` are `Option`s, omitted on `Unavailable`.** A missing
  file has no length and no digest; `0` and `""` would be fabricated values,
  which the Shared Rules forbid.
- **The permission gate receives the lexically joined absolute path, not the
  canonical one.** The permission engine's scope check is lexical against the
  working directory, so a canonical path would refuse every workspace whose
  own path runs through a symlink. Containment is checked separately against
  the canonical root, after the gate and before any byte is read.
- **The query's path is normalized (`./` and empty segments dropped) and the
  answer echoes the normalized spelling.** `..` is refused before
  normalization, so this never legalizes a traversal.
- **Known cost:** `sha256` covers the whole file, streamed in 64 KiB chunks,
  so a very large file is bounded in memory but not in time. A NUL first
  appearing after the 8 KiB sniff window is not caught by the sniff.

### C7 `runtime.durable_work_evidence` (accepted on review, 2026-09-12)

- **`EvidenceCanonicalized` keeps its shipped shape `{ evidence_id, item_id,
  content_sha256 }`.** Changing a live event's payload is a breaking wire
  change; the full reference is on the `EvidenceRecorded` row it follows.
- **Audit outcome is `Success`, not `Applied`**, which the enum does not
  have; `Denied` means a policy refused, and an operator's deny is a decision
  that was carried out.
- **Audit objects are the permission request id, the tool name, and the job
  id, not the tool call id.** No tool call id exists on the approval path;
  the job id is the real join to the work the decision released.
- **`producer.task_id` is the owner's task when the turn has one, else the
  turn id, else the tool call id.** A literal turn id could never satisfy a
  task-keyed merge gate; a session-scoped turn still names no task and is
  refused with `MissingProducer`.
- **ACP canonicalization is live-only in this batch** (compatibility
  follow-up 10). The native path is fully durable.
- **`bundle_id`** is the turn's last context bundle for a native row and the
  store handle id for an agent row; never fabricated.
- **Supervised turns now persist the same durable set as the command path**,
  including context projections and `system`/`command` evidence rows; an
  over-cap batch surfaces as an `Error` rather than a silent drop.
- **Lane approvals resolved through `LaneSupervisor` are not audited by this
  handle**; that is a separate seam.
- **E1 defect 5 closed alongside defect 4**: both are the same delivery.

### C8 `runtime.transcript_rows` (accepted on review, 2026-09-12)

- **`Assistant` rows carry `evidence_id: Option<String>`** beside
  `ToolResult`, because the design's own semantics and fixture require a
  truncated assistant body to name its canonical row. Additive.
- **`Permission` rows derive from C7's durable `approval.<scope>` audit
  records, not from the persisted permission log entry**, which carries
  neither the request id nor the audit id; a minted id would resolve to
  nothing. `decision` is `None` for `allow_session`/`allow_repo`, whose
  audit row keeps only the scope key.
- **Owner attribution adds a durable `turn_owner` session-meta bracket**
  written by `begin_native_turn`/`end_native_turn`. Session-meta is the one
  persisted shape older replayers ignore rather than quarantine; an append
  failure never blocks the turn, and rows outside a bracket name only their
  session (fail-closed for Lane queries).
- **`sequence` orders but does not count**: transcript entries get
  `2·ordinal + 1`, audit-derived rows `2·ordinal`, so a late audit row cannot
  renumber rows a client already paged.
- **A row attributed to no turn carries only its session**, never an empty
  owner that would answer any session's query.
- Previews (`input_preview`, `ToolResult.summary`) are bounded at 500 bytes
  like their live counterparts; a check run replaces, not accompanies, the
  tool result row for the same call.
