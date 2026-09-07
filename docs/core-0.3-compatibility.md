# Core 0.3 Compatibility

Chinese version: [core-0.3-compatibility.zh-CN.md](core-0.3-compatibility.zh-CN.md)

This document is the human-readable compatibility manifest for the frozen Core
0.3.0 `frontend-contract-v1` payload and its backward-compatible Core 0.3.3
extension candidate. It records the frontend schema, handshake capabilities,
migration order, deterministic fixture corpus, UI preference contract, and
design-entry hierarchy.

## Freeze Status

```text
component = viden-core
component_version = 0.3.0
supported_schema_versions = [1]
active_schema_version = 1
contract_payload_sha: 5bd2b80b0953f4194d082940a7b9164c7231ca2d
```

The recorded 40-character SHA identifies the reviewed contract payload commit.
This document is committed separately as evidence, and that evidence commit is
the exact common branch base for TUI and GUI; its parent must equal the recorded
payload SHA. This document does not authorize a tag, push, publish, or Homebrew
change.

### Execution Target Freeze

`ExecutionTarget` (`crates/types/src/agent.rs`) is frozen for the 0.3.x line
as a schema-1 lane fact with exactly two declared variants: `local` and
`ssh { host }`. Only `local` has an execution adapter; `ssh` is a declared
P1 target — a lane may carry it as contract data, but no runtime adapter
executes it yet, and clients must render that state honestly rather than
implying remote execution works. New target kinds are additive schema
changes and land only through the same contract review that governs every
frozen-surface addition in this document; the enum is intentionally exact
(not `non_exhaustive`) so client matches fail closed at compile time when a
variant is added.

## Frozen Capability Set

The frozen capability constant exposed to Core as `CORE_CLIENT_CAPABILITIES` is
the single source consumed by the handshake. Core 0.3.0 advertises this exact
unique, lexically sorted set:

```text
runtime.agent_dag
runtime.approvals
runtime.commands
runtime.context
runtime.cost
runtime.events
runtime.evidence
runtime.merge_gate
runtime.queued_input
runtime.replay
runtime.snapshot
runtime.transcript_page
runtime.typed_lanes
runtime.typed_tasks
ui.preferences
```

Every frozen base-fixture requirement must be present in this set. The separately
registered extension fixture may require only advertised extension capabilities.
A fixture requiring any unknown mandatory capability fails compatibility
validation; malformed or ambiguous legacy input is rejected rather than guessed.

## Schema-1 Post-Freeze Extension Candidate

The Core 0.3.0 frozen capability set and its original nine fixture bytes remain
unchanged. The Core 0.3.3 candidate advertises this exact lexically sorted
additive set through `FRONTEND_V1_EXTENSION_CAPABILITIES` and
`crates/core/frontend-contract-extensions.toml`:

```text
core.workspace_host
runtime.agent_adapters
runtime.agent_conversation
runtime.agent_permission_bridge
runtime.agent_session_input
runtime.agent_sessions
runtime.audit
runtime.cockpit_context_v1
runtime.credential_handles
runtime.credential_staging
runtime.lane_lifecycle
runtime.lane_owner_projection
runtime.project_onboarding
runtime.recent_work
runtime.starter_lane_preview
runtime.trust_loop
runtime.workspace_eligibility
runtime.workspace_files
ui.preference_persistence
```

The schema remains `1`. A client may connect with only the frozen base set;
missing extension capabilities disable only the corresponding feature and must
not block unrelated startup. A disabled feature stays visibly unavailable and
sends no command. In particular, the TUI gates stable Settings on
`ui.preference_persistence`; GUI gates D11 recent work on
`core.workspace_host` plus `runtime.recent_work`; TUI and GUI gate reviewed D4
creation on `runtime.starter_lane_preview`; and exact active-Lane cancellation
requires `runtime.lane_owner_projection` plus one authoritative binding.

The append-only audit timeline read (`QueryAudit` -> `AuditPageLoaded`) requires
`runtime.audit`. It is read-only: no permission prompt, no plan-mode block. A
client without the capability sends nothing and states the timeline is
unavailable rather than rendering an empty one.

Two additive schema-1 extensions since core-0.3.5 keep that read honest under
concurrency and filtering:

- `AuditPageLoaded.command_id` names the exact `QueryAudit` a page answers. A
  client requires an exact match; a page carrying another reader's id is
  ignored. The field is optional, so a page from a Core that predates it
  deserializes to `None` and the client falls back to correlating with its own
  accepted query. A client must never fabricate an id for such a page.
- `AuditQuery.actor`, `AuditQuery.from`, and `AuditQuery.until` filter by actor
  (`operator`, `system`, `any_agent`, or a named agent) and by a half-open
  `[from, until)` unix-second range. Core applies them before pagination, so
  `complete` and `next_before` describe the *filtered* timeline. An inverted
  range is rejected rather than answered with an empty page, because an empty
  page reads as "nothing happened in that window". A filter variant or actor
  variant this build cannot classify matches nothing, so a filter never claims a
  record it cannot name. All three fields default to absent, so a query written
  by an older client keeps its exact previous meaning.

The workspace file inventory read (`QueryWorkspaceFiles` ->
`WorkspaceFilesLoaded`) requires `runtime.workspace_files` (GUI-CORE-022).
Unlike the audit read it is permission-gated, because it reads the operator's
working tree:

- Core consults the permission engine before it reads a single directory
  entry, under the non-mutating tool name `workspace_file_inventory` with the
  workspace root as the input path — the same gate every other workspace read
  passes. A deny comes back as `CommandRejected` naming this exact read and
  carrying the refusal, and so does an unresolved ask: this read answers a
  keystroke rather than an interactive turn, so it is decided
  non-interactively instead of blocking a client behind an approval prompt.
  Neither case ever publishes an empty page, because "you may not read this"
  and "this workspace has no files" are different facts — and neither is a
  bare `Error`, which carries no command id and would let a client with a read
  outstanding mistake an unrelated lane or provider failure for its own
  refusal.
- The tool mutates nothing, so plan mode still answers through the permission
  engine's safe-read branch while every mutation stays blocked.
- The walk is gitignore-aware, honors `.gitignore` even outside a Git
  repository, and reads neither global nor parent ignore files, so one
  workspace enumerates identically on two machines. `.git/`, `.viden/`,
  `.omx/`, `.worktrees/`, and `.ref/` are removed unconditionally: they hold
  runtime and agent state, never workspace content.
- Entries are sorted lexicographically *before* the prefix filter, the
  exclusive `after` cursor, and the `1..=500` limit clamp, so `complete` and
  `next_after` describe the filtered ordered inventory. A prefix that leaves
  the workspace is rejected rather than clamped or answered with an empty page.
- `WorkspaceFilesLoaded.command_id` is **required**, not optional. The audit
  page shipped its correlation id as an addition and therefore carries a
  permanent `None` case; this event is new, so a client always has the exact
  read a page answers and never falls back to its own acceptance.
- A client without the capability sends nothing and states the inventory is
  unavailable rather than walking the filesystem itself, which is outside the
  client boundary and would bypass this gate.

One further additive schema-1 extension since core-0.3.5 makes live work
attributable (GUI-CORE-010). `AgentTaskRecord`, `ToolCallView`,
`QueuedInputView`, and `EvidenceView` each gained an optional full
`RuntimeOwner`, and `RuntimeEventKind::ToolCallStarted` carries the same field
because the reducer folds the view out of the event rather than the envelope.
Core populates it only where the emitting site holds a real owner identity: the
Lane worker's own binding for a queued Lane input, the owner Core published an
Agent session under for that session's tool calls and evidence, the merge gate's
own owner for gate-bound evidence, and the owner persisted with a durable agent
job for its task record. Everywhere else the field stays absent, which means
"Core did not know the owner at emission" — never a default owner, and never an
owner a client may infer from timing, ordering, or a display label. The field is
omitted from the wire when absent, so records with no known owner encode to
exactly the bytes they did before it existed and the frozen corpus is unchanged.

Agent selection requires `runtime.agent_adapters`; starting and cancelling a
typed external session requires `runtime.agent_sessions`; and ACP permission
requests are interactive only when `runtime.agent_permission_bridge` is
negotiated. Adapter views contain safe availability/auth facts, never raw
commands, environment references, or agent-native credentials. Foreground and
asynchronous ACP sessions share the Core-owned approval queue. On restart, Core
terminates an interrupted external process before publishing a recoverable
failed session, so replay never invents a live process.

Clients written against Core 0.3.0 continue to require only the frozen set and
must preserve unsupported schema-1 events as `RuntimeWireEvent::Unknown`. A
client enables the 13 lane lifecycle commands and the `LaneUpdated`,
`LaneCommandAccepted`, `LaneOutputAppended`, `LaneConflictDetected`, and
`LaneRecoveryRequired` event projections only after negotiating
`runtime.lane_lifecycle`. Lane command receipts use the extension-specific
top-level event so a 0.3.0 client preserves the whole payload as unknown rather
than failing on a nested command variant. Empty extension
projection vectors are omitted during serialization, so replaying the frozen
0.3.0 corpus retains its recorded canonical bytes and digests.

Terminal and tmux lanes are explicitly cost-blind: `AgentRoute::cost_meterability`
reports `blind` for them and `metered` for the built-in and ACP routes, and Core
never publishes an inferred token or dollar figure for a blind route. Their whole
cost surface is the bounded, directly observed `LaneRunStats` — accumulated wall
time, run count, applied diff bytes, and the exit code of the most recent
completed run. The exit code is best effort and stays absent whenever the
platform offered none, which is always the case for tmux, because `kill-session`
destroys the pane before any status can be read. These facts are reduced from a
new append-only `RunObserved` lane event with `started`, `stopped`, and `applied`
phases. Every path that tears down an active lane runtime — stop, cancel,
archive, and cleanup — closes the open run, so an operator ending a runaway
terminal lane still keeps its wall time and exit code. A teardown of a lane that
was never running records nothing. Unlike the lifecycle events, run observations
are reduced leniently: a
stop with no matching start records only the exit code and accumulates no wall
time, so a crash mid-run cannot make the lanes log unreplayable. An observation
for an unknown lane is still rejected. `AgentLaneRecord.run_stats` is additive
and optional: it is omitted from the wire when a lane has never been observed
running, so the recorded schema-1 fixture bytes and digests are unchanged, and
absence stays distinguishable from a measured zero.

`runtime.trust_loop` adds typed handoff, review request, contract, dependency,
merge-gate policy/validator/decision, conflict-bounce, and revert facts. The
eight new cross-lane commands, including explicit `RevalidateMergeConflict`
and `DecideReview`, and their events are permission-gated and replay through
the shared reducer. Schema remains `1`: new record fields use defaults, unknown
fields remain ignorable, and a pre-extension string merge decision deserializes
as a read-only `legacy` decision. New writes always serialize a typed decision.
Only real ContextStore bytes with a Core-issued permission receipt can produce
canonical acceptance; display summaries never substitute for evidence. Assigned
validators bind the exact id/hash set, while `RequestReview` itself is
authorized by the requesting gate owner. `DecideReview` is authorized only for
the independent reviewer lane, requires the reviewed evidence bindings to be
unchanged, stamps the gate validator on an accepted verdict, and blocks
`AcceptMergeGate` after a rejected one; a settled review is never overwritten by
a later gate decision. Dependency ids are stable edge ids and
cannot be rebound to different endpoints. Pure trust preflight completes before
approval. Merge persists a private content-addressed recovery snapshot and
workflow precommit before changing files; duplicate preimage blobs are reused,
and the private recovery lock refuses symlink traversal. Audited revert remains
available after restart without placing raw preimages in event logs.

Core owns lane permission evaluation and refreshes it from the current runtime
mode before every lane command. Side-effecting commands are evaluated against
one canonical worktree or repository target shared by permission checks and the
effect executor. Existing symlinks may resolve only inside the repository;
missing targets resolve through their nearest real parent, reject symlink
parents and `..`, and are revalidated immediately before local effects.
Approval previews redact command, argument, environment, input, and diff
payloads. Interrupted starting, running, or approval-waiting lanes hydrate as
blocked recovery facts and remain bound to their durable session owner.

Project onboarding probes the current directory without mutation.
`PreviewProjectConfig` validates repository-root `viden.toml` policy and
returns the exact reviewable UTF-8 contents plus its SHA-256 without writing.
The D11 parser accepts only the documented `project`, `gates`, `runner`,
`budget`, and `targets` schema, rejects unknown nested fields, and withholds
exact contents from candidates containing secret fields or credential-shaped
values.
`ConfirmProjectConfig` accepts only the cached preview id and hash, rechecks
the destination base hash, and writes those exact bytes after a Build-mode
permission approval. Credential commands carry only provider, backend, and
one-use ingress identifiers. Those identifiers use a bounded ASCII opaque-id
grammar and reject secret-like markers and path syntax. Secret bytes remain in the injected backend,
while replay and audit contain only `CredentialHandle` metadata.

Ordinary tool and lane approval responses observe supervisor command ordering
with permission and mode changes, but use two deliberately different generation
semantics. Ordinary tools consult submitted permission-control reservations, so
a queued permission or work-mode command invalidates a blocked approval
immediately and permanently, before the worker applies that command. Its
submitted generation is never decremented or reused, even when the control's
SessionMeta batch later fails to persist. The stale ordinary request resolves as
`Deny` and cannot be restored; the user must retrigger the tool to obtain a new
request. A failed reservation is still removed from the projected applied-state
queue so it cannot leak policy into later controls. Lane requests instead
capture the worker's applied generation atomically with the permission engine it
describes; that generation advances only after the queued control command is
successfully applied, so a lane approval may survive a failed control.
Permission and work-mode controls persist their complete session-metadata batch
before publishing the new live snapshot or permission engine; a failed batch
leaves the engine, snapshot, lane pair, and applied generation unchanged. Any
intervening applied permission or work-mode generation change invalidates the
pending lane approval even if the visible flags later return to their original
values. Once a lane response is accepted, the
supervisor waits
for its terminal `ApprovalResolved` and effect/persistence completion before it
processes or publishes a later permission snapshot. Lane approval-derived
session/repository allow rules are kept
inside the owning lane worker, so they survive normal authoritative permission
refreshes for that lane without authorizing another lane or owner; Plan/ReadOnly
refreshes discard them immediately.
Create and lane status transitions follow the same permission and mutation-policy
gate as other durable effects. Terminal workers unregister and join through the
completion reaper without waiting for another lane command.

## Client Boundary

Frontend clients use only `CoreClient` and protocol/view contracts re-exported
by `viden-core`. The transport interface is limited to discovery, command send,
event receive, snapshot, replay, and transcript paging. Frontends do not import
or call runtime, provider, tool, permission, session, or workflow internals.

`StatefulCoreClient` validates the handshake and schema before committing
state. It ignores duplicate/older cursors, applies only the contiguous next
event, stages gap replay until it is complete and valid, and requests a
validated snapshot for stream mismatch or snapshot-required recovery. A
frontend never synthesizes successful effect state.

`viden_core::legacy` is deprecated. It exists temporarily for the pre-v3 TUI
bootstrap and must not be used by new TUI, GUI, CLI, API, or plugin clients.

## Schema-1 Fixture Corpus

Fixture files live under
`crates/types/tests/fixtures/frontend-contract-v1/`. The digest cells below are
the tested fixture values and must change with the corresponding fixture state.

| Fixture id | Frozen scenario | Expected final view SHA-256 |
| --- | --- | --- |
| `stream-tool` | Assistant stream, tool start, successful tool finish | `8478c7c0ce6f0adc3efdd3aa11497462e96b3aba50cf66e81b0ad9ddcd992eef` |
| `approval-allow-deny` | Structured scoped allow and deny without frontend-owned effects | `7788f2f4b34ce54893ab8ed41beb6e37958ff5fda95642d045ef2d1dedbf7b39` |
| `queued-follow-up` | Queue and dequeue while active work stays visible | `eb1bc1a00185d5642f9a95a2cffde7a81f2bd4ac4417385c5c1b6e2aefa8354a` |
| `dag-blocker` | Typed DAG/task dependency blocker and recovery action | `a496d331e42f730d41565afe58a3308bf38a7b7e3b92e0279d198e9c407e7719` |
| `multi-lane` | Multiple typed lanes with distinct role, route, gate, owner, target, budget, and session facts | `e491d3bc547601b3c54eae05dc1b1259c9cc8ccac948908be8519d432b62fe38` |
| `merge-gate` | Typed evidence and Core-owned MergeGate reduction | `41f4d842a12356586a461b173d072d7e7efedb4d7707471c99eb77dd37533321` |
| `context-pressure-cost-blind` | Context pressure/omission and explicit unknown or unmetered terminal cost | `2e39ec2e32fac56ae6279e8f681bcf4357701de51a6772bad14caee0ddb4ba5e` |
| `plan-denial` | Plan-mode mutation rejection without a successful mutation fact | `fa1fa859af8f056686c06b30b789706539d9ed19e02756519757993d5ee31b2d` |
| `d1-vertical-slice` | D1-visible transcript/tool, lane/task, decision, evidence/gate, context/cost, recovery, and UI preferences | `7dd8faf04cca9f3013198e25823894eae91c2869e27087aa1eb0a34890cdf804` |

The original nine rows above are the frozen base corpus. The separately
registered schema-1 extension fixtures are:

| Fixture id | Extension scenario | Expected final view SHA-256 | Canonical fixture bytes SHA-256 |
| --- | --- | --- | --- |
| `frontend-host-services` | UI preference persistence, safe recent work, reviewed starter-Lane preview/create/invalidation, exact live Lane owner, and one tolerated future optional event | `b118534bb0a568a6a1e781171cecf0512c7d987736c06e4f84d51b5835022a0e` | `96dd5fde9f1241eb50f9d8978cf478d0ac5d3327448dc6ccde9d0e5018ce1580` |
| `interaction-closed-loop` | Folder binding without implicit setup, reviewed Lane creation, built-in and ACP adapters/sessions, shared approval, evidence/gate, apply conflict, typed recovery, reconnect replay, and completion | `31b71bf154d42c8c7923fe9c64763a5245f785a2cd953913124f30a981589b51` | `596e82efa03d21b1f9645f40cf500ca8c4c1b86b2aa78be85a6bea0184822bff` |
| `review-decision` | Independent review verdict: `ReviewRequestStatus` `Pending -> Accepted` with reviewer feedback and the stamped gate validator, while the gate decision stays separate | `38f81bbc1966fbf5742b0087bdd9e871eb11d58cdee747628ed3f4ca1323713c` | `b8e0b5389c3f21be4b4f28cfeba8d902917a304c6b9252cf9911dcccb6146a2b` |
| `context-budgets` | Two concurrent Lanes with their exact bound owners and distinct task-scoped budgets, one under soft pressure and one over its hard limit | `1b251b312b05ef950cdfc8190347e848a38d92bdaf26fe7d196e1ba053fc667b` | `7fcbde9edc5aa1a40a5cd41b0a8442403c6424903cc754cbe64d45980389029f` |
| `streamed-turn` | Ordered `AssistantDelta` chunks under one session and message id reconstruct exactly the final reply, and the terminal completion fact does not duplicate it | `2567d9709e6ec96d621fa281acc205ba5a8fe0b8a08f5868b70ca386f70e3a7d` | `3b1129fb57860aa337c571a9f70be2eacca432c4419bfbb1f4c2943dd371b8e2` |
| `message-parts` | An ACP turn returning an image part alongside text: typed parts attach to their own message, the reference is an immutable parts-directory digest path, and an unmodeled kind round-trips losslessly | `0162e39121f8f9f4543970fdc8098580bc5e01e43786de56d50cc520baed32fe` | `2e7f430cf1694baedd4615c0b60faf9b5614475e52b3c1b365b69a22d0f3d0c2` |
| `audit-reads` | Two concurrent audit reads answered out of order, each page naming its own `command_id`, plus a filtered read whose `complete` describes the filtered timeline while older unfiltered records remain | `389739e9f28cfaf1e1cc9632316760e60fc43495f3702a21d2944874027bb28e` | `a1bdc24b45fc015b9601cf30ae7916dedd5ee0d5bcd2bbc1b5792e2964ef07d2` |
| `owner-scoped-live-work` | Two concurrent Lanes with interleaved task, tool-call, queued-input, and evidence facts under their exact bound owners, plus the same four fact kinds published with no owner | `6972686f93d9d2653fa3510a0f74c50d4b7905426ac0554362a07945ac2541d4` | `87dc66790932f819f84903b3efd457dca1c85e3992c862a44919d0fe5bdeefc2` |
| `audit-ordering` | One newest-first audit page over two interleaved projects, with a cross-project timestamp tie broken by the descending audit id | `4da28fdd43503046033cf65b5362c2cdd482c42ade083bb32f45a014b942c842` | `6c7de7344afb54cf58793c5878672c435da848aea426e5e0a89253ee98d9c3e4` |
| `workspace-files` | Two concurrent inventory reads on one project answered out of order, each page naming the required `command_id`, the scoped read `complete` for its subtree while the unscoped read is not, plus a second attached project with lane facts and no inventory read at all | `9f1c95e59ff5c4a172791d8c0c862f6286326311853b228cc5b83674e4775c37` | `f907b793d2817372fc71c95122e4e33152755682fff5fe46aa20150704bcb949` |

Semantics fix 2026-09-07 (review finding 4): `RuntimeViewState.assistant_stream`
had no lifecycle — it was append-only for the life of the view, so startup
replay concatenated every historical session's reply into one unattributed blob.
A terminal agent-session fact (`AgentSessionCompleted`, `AgentSessionFailed`, or
an `AgentSessionUpdated` carrying a terminal status) now clears it. The stream
still holds the whole reply during the turn; after settlement the reply is
carried by the terminal fact's `session.output` and by the owner-scoped
`agent_conversation`. This moved the recorded final-view digest of the two
extension fixtures whose events place a terminal fact after their deltas —
`streamed-turn` and `message-parts` — and their canonical bytes with them, since
each fixture records its own expected view digest. No frozen base fixture moved:
none of the nine places a terminal agent-session fact after an `AssistantDelta`.
A turn with no agent session — the built-in local provider emits no
agent-session facts at all — has no terminal event, so its text stays in the
stream exactly as before. That is a recorded limitation of this fix, not an
oversight: no new event was invented for the local path.

Deferred review follow-ups 2026-09-07. Three inconsistencies were confirmed
while auditing the fix above and deliberately left for a separate change,
because each is a client-internal cleanup with no contract effect. Recorded
here so they are not rediscovered as new findings:

1. `CockpitProjection.assistant_stream` (`apps/tui/src/tui/projection.rs:26`,
   built at `:77`) is dead: nothing reads it. The TUI renders the stream
   straight from `RuntimeViewState`, so this field is a second copy that
   settles independently of the first.
2. The TUI carries two definitions of "active work" —
   `apps/tui/src/tui/app.rs:1878` gates command routing, and
   `apps/tui/src/tui/state.rs:234` drives status text. They read overlapping
   but unequal fact sets, so the composer and the status line can disagree
   about whether a turn is running.
3. An ACP merge gate is keyed on two different identifiers:
   `crates/agents/src/acp.rs:1126` emits the protocol session handle while
   `crates/agents/src/glue.rs` scopes the same gate by the published Agent
   session. The gate's own id keeps the protocol handle for continuity, and
   the owner binding added on 2026-09-07 is what makes it joinable; the two
   keys still have to be read together.

The `context-budgets` fixture backs the frontend-neutral facade export of
`ContextScope` and `ContextBudgetRecord`. A budget belongs to a Lane only
through the typed task scope named by that Lane's exact bound runtime owner;
"the most recent budget" is never a valid attribution, and the two scopes in the
fixture are deliberately disjoint. `ContextBudgetExceeded` is the carrier for
both soft pressure (`exceeded: false`) and a breached hard limit.

The `streamed-turn` and `message-parts` fixtures make the implemented streaming
and typed-content-part behavior canonical. `agent_message_part` is a known
schema-1 event type, so a part is reduced rather than quarantined as an unknown
event; parts attach only to the message their event named, and a part kind Core
does not model keeps the exact object it published.

The `owner-scoped-live-work` fixture is the generated evidence for owner-scoped
live work. Both Lanes are live at once and their facts are interleaved, so
neither ordering nor recency can stand in for ownership: only the published
owner resolves a fact to a Lane. The fourth group carries no owner and therefore
belongs to no Lane scope while staying visible workspace-wide, which is what an
honest client renders instead of attributing it to whichever Lane is selected.

The `audit-reads` fixture is the generated evidence for audit correlation and
server-side filtering. Two reads are accepted before either is answered and the
pages come back in the opposite order, so arrival order attributes them wrongly
and only the published `command_id` gets them right. The third read filters to
agent actors and comes back `complete` while strictly older operator and system
records are still visible on the unfiltered pages — the completeness fact a
client-side filter could never establish. Like `agent_message_part`, the
`audit_page_loaded` event type is now in the known schema-1 set, so an audit
page is reduced rather than quarantined as an unknown event; the fixture also
proves a page never folds into `RuntimeViewState`.

The `audit-ordering` fixture is the generated evidence that the audit timeline
is one order across projects rather than a per-project list (GUI-CORE-014). The
D10 event ticker is a bounded newest-first page spanning every project in the
workspace, so the order it renders has to be total. `audit-reads` cannot prove
that — all three of its records carry the same `project_id` — so this is a
separate fixture rather than an edit that would move an already registered
digest. Two projects interleave, and one pair of records shares a timestamp
*across* the project boundary, so the `audit_id` tiebreak is exercised exactly
where a client that grouped by project, or that fell back to arrival order on a
tie, would produce a visibly different ticker. The ordering key is
`AuditRecord::cursor()`, the same `(timestamp, audit_id)` pair pagination uses.

The `workspace-files` fixture is the generated evidence for the inventory read.
Two reads on one project are accepted before either is answered and the pages
come back in the opposite order, so arrival order attributes them wrongly and
only the required `command_id` gets them right; the scoped read comes back
`complete` for its subtree while the unscoped read is still incomplete, which is
the completeness fact a client filtering a page it already held could not
establish. A second attached project publishes ordinary lane facts and no
inventory read at all — the "project without one" half of the request — so a
client scoped there has no file list rather than an empty one. Like
`agent_message_part` and `audit_page_loaded`, the `workspace_files_loaded` event
type is in the known schema-1 set, so an inventory page is reduced rather than
quarantined as an unknown event; the fixture also proves a page never folds into
`RuntimeViewState`.

The extension fixture uses the six real known events
`UiPreferencesUpdated`, `RecentWorkLoaded`, `StarterLanePreviewed`,
`StarterLaneCreated`, `StarterLanePreviewInvalidated`, and
`LaneRuntimeOwnerBound`; it does not substitute a transient error or display
placeholder. The normal in-memory journal, snapshot, and replay path must
reduce these facts to the same `RuntimeViewState`. The optional future event
advances the cursor without mutating that state.

The interaction fixture has 18 ordered events and locale-neutral fact keys.
Its reconnect test deliberately observes a cursor gap, replays the missing
contiguous batch, and proves that the normalized final `RuntimeViewState`,
cursor, and digest equal uninterrupted replay. The manifest at
`crates/core/release-manifest.toml` records both fixture payloads and the Core
0.3.3 contract implementation checkpoint; it does not authorize a tag.

Each JSON fixture envelope contains:

- `fixture_id`;
- `schema_version: 1`;
- unique, lexically sorted `required_capabilities`;
- an `initial_snapshot`;
- non-empty `RuntimeEventEnvelope` values in contiguous cursor order;
- `expected_final_cursor`;
- `expected_view_sha256`.

For every known event, the event sequence equals the cursor sequence. Replaying
the parsed fixture twice from the same initial snapshot must produce
byte-identical canonical state plus the same cursor and digest. Fixture values
are deterministic and contain no machine-specific absolute path or secret.

## Canonical Digest

The final-view digest is SHA-256 over compact JSON for `RuntimeViewState` after
recursively sorting every object key. Array order is preserved because it is
semantic. The test compares the generated lowercase 64-character hex digest
with the fixture and this manifest.

## Migration Gate And Order

Migration runs before schema-1 fixture replay and must be idempotent:

1. Parse `legacy-lanes.tsv` through the supported v0 lane input boundary into
   typed `AgentLaneRecord` values.
2. Compare those values with `typed-lanes.json`, serialize the normalized typed
   values, parse them again, and require equality.
3. Parse the supported legacy flat cost shape into structured
   `CostUsageRecord`; preserve an unknown actual cost as `None`, serialize the
   normalized record, parse it again, and require equality.
4. Parse the supported legacy approval boolean into structured
   `ApprovalResponse`, serialize the normalized response without the legacy
   boolean, parse it again, and require equality.
5. Only after all migrations pass, replay every schema-1 fixture twice and
   validate identity, capabilities, cursor continuity, final state, and digest.

Unknown lane roles/routes/statuses, ambiguous cost shapes, malformed approval
records, and unknown mandatory fixture capabilities fail the gate. Migration
does not silently coerce them.

## UI Preference Compatibility

The schema-1 preference surface is frontend-neutral:

The effective frontend fact is
`RuntimeSnapshot.ui_preferences: ResolvedUiPreferences`. A client renders this
resolved value and does not re-run precedence or fallback policy locally.

| Dimension | Supported values |
| --- | --- |
| Built-in locale | `en`, `zh-CN` (`system` resolves to one of them) |
| Skin | `aurora`, `ice`, `mono`, `amber`, `phosphor` |
| Effective mode | `dark`, `light` |
| Density | `compact`, `regular`, `comfy` |
| Motion | `system`, `reduced`, `full` |

The eight valid effective skin/mode pairs are:

```text
aurora/dark
aurora/light
ice/dark
ice/light
mono/dark
mono/light
amber/dark
phosphor/dark
```

`amber` and `phosphor` are dark-only. Preference precedence is CLI, user,
project, then client default. An invalid effective pair uses the safe
`aurora/dark` and regular-density fallback and records a
`ui.invalid_skin_mode_pair` diagnostic; locale and motion remain resolved.

## Design Entry Hierarchy

Visual verification follows one path:

1. global index: `docs/viden-design/Viden/index.html`;
2. client index: `TUI/Viden - 设计稿索引 (TUI).html` or
   `GUI/Viden - 设计稿索引 (GUI).html`;
3. component library: `TUI/Viden - 组件库 (TUI).html` or
   `GUI/Viden - 组件库 (GUI).html`;
4. canonical product entry: `TUI/Viden - 统一原型 (TUI).html` or
   `GUI/Viden - 桌面驾驶舱 (GUI).html` (D1).

GUI `pages/Viden - D11 首启与项目接入 (GUI).html` is subordinate onboarding. It
is not the cockpit and cannot replace D1 as the GUI baseline. All relative
paths above start at `docs/viden-design/Viden/`. Old screenshots and generated
previews are historical evidence only; they do not override this hierarchy.

## Historical Compatibility

The v0 lane TSV input, legacy flat cost shape, legacy approval boolean, and
`viden_core::legacy` bridge are migration surfaces, not new client APIs. Keep
historical release evidence unchanged while clients move to schema `1` and the
CoreClient-only boundary.
