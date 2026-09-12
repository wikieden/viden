# GUI Core Contract Requests

Chinese version: [contract-requests.zh-CN.md](contract-requests.zh-CN.md)

Planning note 2026-09-09 (`docs/release-0.3.3-plan.md`): 012, 015, and 020
are scheduled for `0.3.3` through the Core contract increment in
`docs/release-0.3.3-contract-design.md`, which also opens GUI-CORE-025
(evidence reads) for the EvidenceView surface. 013 is a `0.3.3` stretch
bundled with the creation flows. 009, 018, and 019 move to `0.3.4`; 021 and
023 stay `0.3.4+`. A schedule is not a closure: each entry closes only when
its stated condition is met on `main`.

Planning note 2026-09-12 (`docs/release-0.3.4-plan.md`): 027, 028, and 009
are scheduled for `0.3.4` through the Core contract increment to be written
in `docs/release-0.3.4-contract-design.md` (workspace owner, turn lifecycle,
durable work evidence, transcript rows, workspace file reads). 013, 018,
019, 021, 023, and 026 move to `0.3.5`; D5 gallery review will be opened as
GUI-CORE-029 for `0.3.5` when that contract design is written. That number is
still held for it: batch G7 numbered its four prose-only gaps 030 to 033 and
left 029 where `docs/release-0.3.4-plan.md` reserved it.

Status note 2026-09-12 (C9): the workspace file-read fact landed on the Core
side — `runtime.workspace_file_reads` is published with its
`workspace-file-reads` fixture, so `ReadWorkspaceFile` answers one file's bytes
behind the `read_file` permission gate. No register entry closes on it: it was
scheduled as contract content for the Files tab, the Code tab, and the palette's
file rows rather than as a numbered request, and the GUI adopts it in G7. The
open register is unchanged at 009, 013, 018, 019, 021, 023, 026, and 028.

Status note 2026-09-10: 012, 015, 020, and 025 are now fully closed — Core
published each capability and **both** clients adopted it, which is the
condition those entries stated. GUI-CORE-027 (workspace-scoped operator
identity) is opened by this pass and scheduled for `0.3.4`. The open register
is 009, 013, 018, 019, 021, 023, 026, and 027. These entries are recorded
against the `claude/int-0.3.3` integration branch; nothing here is on `main`
until that branch is merged.

Status note 2026-09-10 (E1): GUI-CORE-028 (durable evidence for
supervisor-driven work) is opened by the release-evidence pass and scheduled for
`0.3.4`, following the C5 adjudication of the same day. The open register is
009, 013, 018, 019, 021, 023, 026, 027, and 028.

Status note 2026-09-12 (C5): GUI-CORE-027 is closed **on the Core side** —
`runtime.workspace_owner` is published with its `workspace-owner` fixture — and
stays open as a client adoption item until the GUI drops
`D1-OPERATOR-GIT-OWNER` (G7) and the TUI enables its `/git` workspace rows (T2).
It is listed as closed here because the register tracks what Core owes; a Core
request is not re-opened by a client that has not yet adopted it. The open
register is 009, 013, 018, 019, 021, 023, 026, and 028. These entries are
recorded against the `claude/int-0.3.4` integration branch; nothing here is on
`main` until that branch is merged.

Status note 2026-09-12 (C7): GUI-CORE-028 is closed **on the Core side** —
`runtime.durable_work_evidence` archives an applied native mutation and an
adapter-reported patch as `patch` rows with canonical bytes, persists every
supervised turn batch into the projection the archive is rebuilt from, and
writes the approval decision as a durable audit row — and stays open as a client
adoption item until the GUI renders the archive rows and the D14 approval rows
(G7) and the TUI's evidence inspector shows archived patches (T2). The open
register is 009, 013, 018, 019, 021, 023, and 026.

Status note 2026-09-12 (C8): GUI-CORE-009 is closed **on the Core side** —
`runtime.transcript_rows` answers one owner's ordered typed rows from durable
session and audit facts, with its `transcript-rows` fixture proving that two
Lanes and the session composer cannot leak a row across owners — and stays open
as a client adoption item until the GUI's D1 transcript renders the rows and
retires its two unavailable placeholders (G7) and the TUI's transcript lens
reads them for the focused Lane (T2). C8 is the last Core batch of the `0.3.4`
increment, so the advertised extension set is final at 29. The open register is
013, 018, 019, 021, 023, and 026.

Status note 2026-09-12 (G7): 009, 027, and 028 are closed **on the client side
for the GUI**. The cockpit now sends a workspace-target `RunOperatorGitAction`
under the owner Core published and dropped `D1-OPERATOR-GIT-OWNER` (027); it
renders the archived `patch` rows with their canonical bytes, keeps the
`approval.*` audit rows with Core's operator actor and every object, and links a
permission's trail by that object (028); and the D1 transcript draws Core's own
ordered rows, retiring the `transcript_user` / `transcript_assistant`
unavailable placeholders (009). Each stays open as a *TUI* adoption item until
T2 lands the same three. The GUI also adopted `runtime.turn_lifecycle` (C6) and
`runtime.workspace_file_reads` (C9), neither of which was ever a numbered
request: liveness was a residue-based client predicate, and the file read was
adjudicated as the inventory's sibling.

Four gaps the G4 and G5 batches wrote as prose and left unnumbered are numbered
here as new OPEN entries — 030 to 033 — without being implemented. 029 was
skipped because the `0.3.4` plan already reserved it for the D5 gallery review.
G6 left none: its finding was the client's own decision-count divergence, which
G7 closed by deriving the rail badge, the statusbar chip and the D2 header from
one count. The open register is 013, 018, 019, 021, 023, 026, 030, 031, 032,
and 033, with 029 reserved and not yet written.

## GUI-CORE-008: Selected-Lane context scope — CLOSED

History: Core `0.3.5` exposed `RuntimeViewState.context_budgets`, but the
frontend-neutral `viden-core` facade did not re-export `ContextBudgetRecord`
and `ContextScope`. The GUI therefore could not prove that a budget belonged to
the selected Lane's task without reconstructing a private serialization schema,
so D1 projected `contextDock.context` as `null`; it never selected an arbitrary
budget, deserialized a guessed scope shape, or inferred usage from display text.

Core status: `viden-core` now re-exports `ContextScope` and
`ContextBudgetRecord`, asserted by the facade test. The schema-1 extension
fixture `context-budgets.json` publishes two concurrent Lanes with their exact
bound runtime owners and disjoint task-scoped budgets, one under soft pressure
and one over its hard limit, and its replay test proves each Lane's task scope
resolves exactly one budget and never the other Lane's.

GUI status: wired on `claude/core-contract-closures`. D1 resolves
`contextDock.context` through the typed `ContextScope::Task` named by the exact
runtime owner Core bound to the selected Lane, taking the freshest budget inside
that scope. Recency never crosses scopes. No exact owner, no task id, or no
budget in that scope still projects `null` rather than a budget that is merely
published. The statusbar's context segment is unchanged and remains the coarse
workspace-level "latest budget" indicator its type documents, not a per-Lane
number.

## GUI-CORE-009: Owner-scoped typed transcript rows — CLOSED (Core 2026-09-12; GUI client 2026-09-12)

History: the frontend contract exposed lane output as an untyped stream and a
global assistant stream, and no ordered owner-scoped user/assistant transcript
sequence at all. D1 therefore rendered only typed lane-output facts for the
selected exact owner and declared user/assistant rows unavailable; it never
inferred roles from display text. The base `runtime.transcript_page` did not
close the gap: it pages one session's storage log in persisted entry shapes and
carries no owner, which answers "what is in this session's file" rather than
"what happened in this owner's conversation".

Core status: delivered as `runtime.transcript_rows` (C8, the 0.3.4 contract
increment's section 5). `QueryTranscriptRows { query }` ->
`TranscriptRowsLoaded { command_id, page }` answers one owner's ordered rows in
typed `User`, `Assistant`, `ToolCall`, `ToolResult`, `CheckRun`, and
`Permission` shapes, each with a stable row id, a full `RuntimeOwner` that is
never widened, and an opaque backwards cursor. Rows are derived from durable
facts only — the append-only session transcript and the append-only audit
timeline — so a reconnect and a restart answer the same read the same way,
which the live `agent_conversation` could never do: it is a capped per-connection
reduction and is empty after a restart.

Attribution is written into the log rather than inferred from it, because one
transcript file holds more than one owner's work: the supervised input path runs
a Lane's native turn through the same engine as the session composer, so
`begin_native_turn`/`end_native_turn` write a `turn_owner` bracket and the rows
inside it carry that owner verbatim. A row Core attributed to no turn carries
only the session it is a line of, which is never enough to satisfy a
Lane-scoped query. The scope itself reuses `EvidenceQuery`'s own matcher, so it
fails closed both ways. Bodies are cut at 8 KiB with `truncated` and name the
canonical evidence that still holds them whole where such a row already exists;
the read never creates one. A cursor this build did not issue is a
`CommandRejected` naming the read, never an empty page.

The schema-1 extension fixture `transcript-rows.json` makes this canonical, and
it is the fixture this request asked for: three reads outstanding at once over
one durable transcript, answered out of order, with two Lanes and the session
composer each getting their own page and no row appearing on more than one of
them. It also proves the page boundary and the cursor round-trip, every row
variant, the real 8 KiB cut naming its evidence, and the cursor refusal.

GUI client status: adopted 2026-09-12 by batch G7. D1's transcript reads the
newest page for the selected Lane (or unscoped for the session), pages
backwards through Core's own `older` cursor on scroll-top and on an explicit
control, offers the canonical evidence behind a row Core's 8 KiB bound cut, and
draws `ToolCall` / `ToolResult` / `CheckRun` / `Permission` rows with G4's own
blocks rather than a second rendering of the same facts
(`apps/gui/src/components/transcript_rows.ts`,
`apps/gui/src-tauri/src/transcript_rows.rs`). The two `transcript_user` /
`transcript_assistant` unavailable rows are gone wherever Core publishes the
capability, and they still stand on a Core that publishes none — they were
claims about Core, so they retire exactly where the claim stops being true. The
TUI adopts it in T2, where the transcript lens reads the same rows for the
focused Lane; until then it still declares the rows unavailable rather than
inferring them.

## GUI-CORE-010: Owner-scoped live-work facts — CLOSED

History: `AgentTaskRecord`, active tool calls, queued inputs, and evidence views
did not carry a `RuntimeOwner` in frontend-contract-v1. D1 omitted these global
facts from a selected Lane and declared the gap with this code; it never
attributed them by timing or label.

Core status: delivered. `AgentTaskRecord`, `ToolCallView`, `QueuedInputView`,
and `EvidenceView` each carry an additive optional full `RuntimeOwner`, and
`RuntimeEventKind::ToolCallStarted` carries the same field because the reducer
folds the view out of the event rather than the envelope. The field is omitted
from the wire when absent, so a record with no known owner encodes to exactly
the bytes it did before and the nine frozen base fixtures are unchanged.

Core populates it only where the emitting site structurally holds a real owner
identity: the Lane worker's own binding for a queued Lane input, the owner Core
published an Agent session under for that session's tool calls and evidence, the
merge gate's own owner for gate-bound evidence, and the owner persisted with a
durable agent job for its task record. Every other site publishes `None`, which
means "Core did not know the owner at emission" — never a default owner, and
never one inferred from timing, ordering, or a label. In particular the built-in
engine's own turns stay unowned: they bind no Lane and no Agent session, and
Core mints no workspace/project identity of its own (see GUI-CORE-023).

The schema-1 extension fixture `owner-scoped-live-work.json` makes this
canonical: two Lanes live at once publish interleaved task, tool-call,
queued-input, and evidence facts under their exact bound owners, plus the same
four fact kinds with no owner at all. Its replay test proves a selected-owner
projection resolves that Lane's four facts and neither the other Lane's nor the
ownerless ones.

GUI status: wired on `claude/core-owner-facts`. D1 scopes `liveWork.tasks`,
`tools`, `queuedInputs`, and `evidence` by full `RuntimeOwner` equality against
the exact `LaneRuntimeOwnerBinding` for the selected Lane — the same matching
discipline the context dock already used for workspace changes and the
permission dock for approvals. A fact whose owner is absent or different stays
omitted from Lane scope, and a Lane with no exact Core owner projects no live
work at all. The `live_work_scope` unavailable-feature row is gone.

TUI adoption is not part of this closure. The same fields are available to it,
and `apps/tui` can adopt owner-scoped live work later without a Core change.

Residual, and still cited by this code in the D1 work-status strip
(`apps/gui/src/components/work_status.ts`): Core publishes no owner-scoped turn
*start timestamp*, so the strip's elapsed clock is anchored to the moment the
client observed work start and says so, and it still falls back to an
unscoped-status label when Core scopes no Agent session to the selected Lane.
Neither is a live-work fact, so neither was in this request's close criteria;
the owner fields do not supply them, and the strip must keep stating both
rather than inventing a start time or borrowing another Lane's status.

## GUI-CORE-011: Review decision command — CLOSED

History: `frontend-contract-v1` published `ReviewRequestRecord` with a
`Pending` status and `RuntimeCommand::RequestReview`, but no command recorded a
review decision. D2 listed pending reviews with their Core evidence and
rendered the accept/reject actions disabled with this code; it never reused
`AcceptLaneOutput` or an approval response as a substitute review verdict.

Core status: delivered in `core-v0.3.2` (`a04260af`) as
`RuntimeCommand::DecideReview { review_id, verdict, feedback, actor }`. Only
the independent reviewer lane may decide; the verdict settles the review and
stamps the gate validator on accept, an accepted verdict is never overwritten
by a later gate decision, and `AcceptMergeGate` fails closed after a rejected
review. `ReviewRequestRecord` gained an additive optional `feedback`. The
schema-1 extension fixture `review-decision.json` proves the
`Pending -> Accepted` transition.

GUI status: wired on `claude/gui-supervision-debts`. D2 sends `DecideReview`
with the actor `validate_review_decider` accepts, derived the way the TUI
derives it — the gate validator Core recorded for this review when it names the
reviewer Lane, otherwise Core's own `reviewer_owner_from_requester` shape (the
review owner re-pointed at the reviewer Lane, session and turn identity left
unclaimed). Reviewer prose is optional, trimmed, and refused above Core's
500-character cap rather than truncated. The verdict is confirmed only by an
ordered `ReviewRequestUpdated` naming this review with the status the command
asked for; `CommandAccepted` alone never confirms it, and the validator-stamping
`MergeGateUpdated` that follows is tolerated. A review Core would refuse stays
disabled with a local reason instead of this code: `D2-REVIEW-SETTLED` for an
already-decided review, `D2-NO-REVIEWER-ACTOR` when no acceptable reviewer
identity is derivable.

## GUI-CORE-012: Structured decision context for an approval — CLOSED

History: `ApprovalRequestView` carried only `input_preview`, an opaque display
string. The D2 design shows line-level diff rows for the pending mutation. D2
rendered the preview verbatim and declared the diff unavailable rather than
parsing display text into diff rows.

Core status: delivered by the typed structured diff (capability
`runtime.structured_diff`). `ApprovalRequestView.decision_context` carries a
`DiffDocument` — ordered hunks with file path, per-side line numbers, and a
change kind — exactly the shape this request asked for. Core is its only
producer: the unified-diff parser that *applies* patches
(`crates/tools/src/patch.rs`) is promoted to emit it, so the apply path and
every client agree about what a hunk is and no frontend parses display text.
The context is produced for `edit_file` and `write_file` by reading the target
file read-only and computing the proposed content in memory — nothing is
written at approval time, which is what keeps a denial meaningful — and for the
trust loop's `MergeAgentPatch` from the canonical patch bytes Core already
holds. That last one is the multi-file case this request named, and the
schema-1 extension fixture `structured-diff.json` makes it canonical alongside
the single-file `edit_file` preview.

Two limitations are recorded rather than hidden. `base_sha256` names the bytes
a single-file preview was computed against, because execution later runs the
proposed tool input against whatever the file holds then; a client or an audit
reader can detect a file that moved in between, but Core does not re-check
before execution in `0.3.3`. A multi-file patch carries no `base_sha256` at
all: one hash cannot describe several files, and naming one would invite a
client to verify the wrong one.

GUI status: adopted 2026-09-09 (G1a), read side. The DiffReview view renders
`QueryWorkspaceDiff` -> `WorkspaceDiffLoaded` as the registered
`.review > .filetree + .diffpane` family in the D1 centre pane; the D1
permission dock, the D2 decision detail, and D1's changed-file cards render
`decision_context` and `WorkspaceChangeView.diff` through the same row
renderer. D1's `diff` unavailable row is dropped when Core advertises
`runtime.structured_diff` and kept otherwise, and D2's marker is dropped per
decision — only for an approval Core actually attached a context to, because
`shell` and the `git_*` family legitimately carry none and the preview stays
the whole context there. `base_sha256` renders as "Preview computed against
<8 chars>", stated as the preimage the preview was computed against rather
than as a guarantee that the file has not moved since.

The commit bar the DiffReview family draws is live as of 2026-09-09; see
GUI-CORE-020 for the action side and what the client deliberately does not do.
The Split half of the Unified/Split segmented control is still visible and
disabled, for honesty — only the unified body is built. Conflict content
(GUI-CORE-015) is a separate batch.

## GUI-CORE-013: Pending contract-confirmation fact

`ContractRecord.decision` has only `Confirmed` and `Rejected`, so every
published contract is already decided. The D2 design shows a contract
confirmation queue awaiting a human. D2 lists contract records as decided
history and marks the group with this code; it must not treat a decided record
as a backlog item.

Close this request when Core publishes a pending contract-confirmation fact
with its proposer, target contract version, subscribers, and audit id, and the
canonical fixture proves that a pending contract becomes decided through
`ConfirmContract`.

GUI status, corrected on `claude/hygiene-h1`: the contract *group* declared the
gap while the contract *detail* still rendered live Confirm and Reject buttons
against a record Core had already decided. `confirm_contract` in
`RuntimeSupervisor` rejects an id it has recorded, so those controls could only
ever produce a refusal. Both verdicts now project `available: false` with this
code and render disabled-and-labelled — visible, so the operator can see the
decision exists, and inert, so the client never sends a doomed command. They
become conditional on the record's own state once Core publishes the pending
fact this request asks for.

## GUI-CORE-014: Ordered event log in the view state — CLOSED

History: `RuntimeViewState` published current facts but no ordered event log.
The D10 design shows a scribe-compiled event stream across projects, and D14
needed the same ordered history. D10 rendered no ticker and declared the gap
with this code; it never rebuilt a timeline by diffing successive snapshots.

Core status: delivered by the append-only audit timeline (`QueryAudit` ->
`AuditPageLoaded`, capability `runtime.audit`). An `AuditRecord` carries exactly
the facts this request asked for: the stable `audit_id`, a stable dotted `action`
key that is never localized prose, the full `RuntimeOwner`, and the unix-second
`timestamp`. Pagination is newest-first on `AuditRecord::cursor()` — the
`(timestamp, audit_id)` pair — so the order is total rather than per project.
The schema-1 extension fixture `audit-ordering.json` makes that canonical: one
bounded page over two interleaved projects, with a pair of records that share a
timestamp *across* the project boundary so the `audit_id` tiebreak is exercised
exactly where a client grouping by project, or falling back to arrival order on
a tie, would render a visibly different timeline. It is a separate fixture
rather than an edit to `audit-reads.json`, whose three records all sit in one
project and whose bytes are already registered.

GUI status: wired on `claude/core-workspace-files`. D10's ticker is one bounded
newest-first page (`D10_EVENT_TICKER_LIMIT` = 50) of that timeline, unscoped so
it spans every project, read through the same adapter slot D14 audit mode uses —
the two screens are two views of one Core timeline and only one is mounted at a
time. Each row renders Core's own stable id, dotted action key, owning project
and Lane, and timestamp; the dotted key is never localized, because a localized
timeline cannot be diffed across languages. Rows carry no action: the ticker is
ambient, and the Decision Center still owns the actionable queue. An absent
`runtime.audit`, an unanswered read, a refusal, and an answered-but-empty
timeline stay four different lines, so an empty strip never reads as "nothing
ever happened". The `d10.events.noOrderedLog` unavailable row is gone.

## GUI-CORE-015: Structured merge-conflict content — CLOSED

History: `MergeGateRecord` and `ConflictBounce` named the gate, the origin Lane,
and the reason, but carried no conflict content, and `LaneConflictView` carried
only a summary. The D12 design shows both Lanes' hunks side by side, so D12
rendered the Core reason text and declared the hunk unavailable rather than
reading the worktree or parsing the reason string into diff rows.

Core status: delivered as structured conflict content (capability
`runtime.conflict_content`). `ConflictBounce.content`, `LaneConflictView.content`,
and the `LaneConflictDetected` payload each carry an optional `ConflictContent`:
per file, the hunks the strict apply rejected, and per hunk `ours` at
`ours_start`, `theirs` at `theirs_start`, the patch preimage as `base`, and a
typed `ConflictHunkReason`. No new event type exists — both attach sites are
events a failed apply already published — so nothing existing changed shape and
the nine frozen base fixtures keep their bytes.

Two answers are narrower than the request's wording, and the difference is worth
naming. The request asked for "ours/theirs hunks"; what Core publishes is two
sides **plus the patch preimage**, and it is explicitly not a three-way merge.
Core computes no merge base and resolves nothing, so D12 must render the three
sides and must not present a merged result or offer to auto-resolve. And the
baseline is typed rather than a string: `Evidence { bindings }` when the gate
holds canonical reviewed evidence, because a gate's baseline is its bindings and
not a bare commit; `Revision { sha }` for the Lane apply path; `Unknown` when
Core held no baseline it could name, which D12 must render as unknown rather
than silently as `HEAD`.

Content is only ever built from a real apply failure. An operator
`BounceMergeConflict` is a human judgement with a reason and no failed apply, so
its content is `None`, and hunks after the refusal were never attempted and are
not listed. `None` means Core has nothing to show and never that the conflict
was empty, so an absent content keeps the design's unavailable marker rather
than rendering an empty conflict.

The canonical fixture is `conflict-content.json` rather than an extended
`merge-gate.json`, because the frozen base fixture must keep its bytes. It is
the two-Lane conflict on one file the request asked for: Lane A's patch merges,
Lane B's `MergeAgentPatch` is refused and the bounce carries one hunk with an
`Evidence` baseline, and a `LaneConflictDetected` beside it carries the same
shape with a `Revision` baseline.

GUI status: adopted 2026-09-09 (G2a). D12 renders each conflict record's
rejected hunks as OURS beside THEIRS at Core's own starts, with the patch
preimage as a collapsible third strip and a reason chip carrying both Core's
classification and what the operator can do about it. The pane states with
every conflict that this is two sides plus the preimage and not a merge result,
and the screen offers no merged text and no resolve control. The baseline is
rendered as Core typed it — evidence bindings as chips that open each evidence
object's own audit trail, a revision as its short sha with the full value in
the row title, `Unknown` as unknown — and `LaneConflictView.content` gets the
same pane, scoped to the Lanes the selected gate involves. Four absences stay
four sentences: a missing capability, a record Core published no content for
(named as the operator-bounce contract), an `omitted` file, and a `truncated`
payload. The old unavailable marker no longer cites this request, which is
closed; it now names `runtime.conflict_content`, the thing that can still be
absent. Evidence: `d12-conflict-content`, `d12-conflict-omitted`, and
`d12-conflict-none` under
`apps/gui/evidence/main-window-interactions/`.

The residual recorded at adoption is closed as of 2026-09-10 (F1).
`viden-core` re-exported `ConflictBounce` and `LaneConflictView` but not the
`ConflictContent`, `ConflictBaseline`, `ConflictFile`, `ConflictHunk`, and
`ConflictHunkReason` types they carry, and the GUI may hold no second
`viden-*` dependency (`apps/gui/tests/architecture_boundary.rs`), so the
projection read the value through Core's own canonical serde encoding rather
than naming the types. Those five names plus `MAX_CONFLICT_CONTENT_BYTES` are
now on the facade; the D12 projection matches each type exhaustively, with the
wildcard arm of each `#[non_exhaustive]` enum recovering Core's own tag so an
unnamed future kind still reaches the screen as itself. The TUI's conflict
rows import from the facade for the same reason.

## GUI-CORE-016: Streaming Agent message chunks — CLOSED

History: the ACP adapter already received `agent_message_chunk` updates, but
`crates/runtime/src/agent_commands.rs` accumulates them into a local string
and publishes one `AgentConversationMessageView` at turn end. No ordered event
carries a partial message, so the GUI cannot render a reply as it is produced;
it can only show the completed paragraph.

D1 therefore renders whole messages and shows the work-status strip for
liveness. It must not fake a typewriter effect over a completed message.

Close this request when Core publishes ordered chunk events carrying the
session id, the message id the chunk belongs to, the appended text, and a
terminal marker, and the canonical fixture proves that replaying the chunks
reconstructs exactly the final message.

Core status: delivered. `AssistantDelta` carries an optional session id, the
ACP adapter keeps one message id for a whole prompt turn, and the reducer grows
a single owner-scoped message. The schema-1 extension fixture
`streamed-turn.json` makes this canonical: its replay test proves the ordered
chunks reconstruct exactly the final message, that the terminal completion fact
settles the turn without appending a second copy, and that the unscoped
`assistant_stream` holds the identical reply for a client that predates
owner-scoped conversation.

Amendment 2026-09-07 (review finding 4): the sentence above originally promised
that the unscoped `assistant_stream` still held the identical reply *after*
completion. That promise is narrowed, not withdrawn. The stream holds the
identical reply **during** the turn; the terminal fact now clears it, because
an unscoped stream that is never cleared concatenates every historical session's
reply into one unattributed blob on replay. After settlement the reply is
carried by the completion fact's `session.output` and by the owner-scoped
`agent_conversation`, both of which every schema-1 client already receives. The
`streamed-turn.json` replay test asserts both halves: the whole reply is in the
stream at the last pre-terminal event, and the stream is empty once the
completion fact is applied.

## GUI-CORE-017: Non-text Agent message content — CLOSED

History: `AgentConversationMessageView.content` was a single `String`, and
`acp_message_chunk_text` extracted only `content.type == "text"`. An ACP agent
that returned an image block therefore reached the client as prose claiming an
image exists, with no image fact behind it — exactly what an operator sees as
"the agent says it drew something and nothing is there". D1 rendered the text
Core published and never synthesized an attachment.

Core status: delivered. A conversation message carries typed content parts,
`AgentMessagePart` attaches a part to the message it belongs to, and inline
Agent bytes are persisted under `.viden/agents/parts/` with the content digest
as their file name, so the reference is immutable and the bytes are also
published as evidence. A part kind Core does not model round-trips losslessly
rather than being dropped. The desktop shell resolves a workspace reference
through the `agent_content` command, because a webview cannot open a workspace
path; it refuses any reference outside the parts directory.

The schema-1 extension fixture `message-parts.json` makes this canonical: an
ACP turn returns an image part alongside text, both parts attach to the message
their event named while a second message in the same session stays part-free,
the image reference is a parts-directory digest path rather than inline bytes,
and the unmodeled kind re-encodes to the exact object Core published.

Writing that fixture also closed a real wire gap: `agent_message_part` was
missing from the known schema-1 event types, so every part degraded to a
quarantined unknown event and was dropped during snapshot and replay. It is now
a known event type with a types-level round-trip test.

## GUI-CORE-018: Checkpoint capture and restore

D6 renders a "Restore checkpoint" recovery action, but schema 1 models no
checkpoint at all: no `RuntimeCommand` captures or restores one, no checkpoint
record exists in `RuntimeViewState`, and no event reports a restore outcome.
The other D6 actions now reach real Core commands — `restart` sends
`RetryAgentSession` and `close_lane` sends `StopLane` — which leaves
`checkpoint` as the one action with nothing behind it.

The GUI projects it as `available: false` with code `GUI-CORE-003` and never
attaches a handler. It must not simulate a restore from replay, re-open a
session at an earlier cursor, or present a snapshot re-read as a checkpoint.

Close this request when Core publishes a typed checkpoint record with a stable
id and the owner it belongs to, a command that restores one, an event carrying
the restore outcome, and a canonical `frontend-contract-v1` fixture covering a
capture followed by a restore.

## GUI-CORE-019: Always approval scope and Edit decision

`ApprovalScope` models `Once`, `Session`, and `RepoAllowlist`. The permission
dock's design also offers "Always" and "Edit": Always is a standing decision
across sessions and repositories, Edit returns a modified command for approval
instead of accepting or refusing the one proposed. Neither exists in schema 1,
so both render as fail-closed `GUI-CORE-003` placeholders and
`PermissionChoice::Always` / `PermissionChoice::Edit` are refused before any
command is built.

This also holds a keyboard divergence open. The design assigns `Shift+A` to
"Always"; the GUI keeps `Shift+A` on `repo_allowlist`, the widest scope Core
accepts, rather than binding a live chord to a dead action.

Close this request when Core models a persistent Always scope with the
revocation path that makes it safe to grant, models an Edit decision that
carries the revised command back through the same approval gate, and the
canonical fixture covers both. The GUI will then restore the design's
`Shift+A` binding.

## GUI-CORE-020: Operator-initiated git actions — CLOSED

History: the cockpit titlebar could show what the workspace's source control
looked like — branch, ahead/behind, dirty — but nothing let the operator act on
it. `RuntimeCommand` modelled no commit, push, fetch, stage, or unstage, and the
model-facing Git tools in `crates/tools` were `pub(crate)`: reachable by an
agent turn under the permission gate and by nothing else. So the sync chip
shipped as a `role=status` element rather than a button and the "Commit or push"
affordance was not built at all.

Core status: delivered by typed operator source-control actions (capability
`runtime.operator_git`). `RunOperatorGitAction { owner, target, action }` covers
`Stage`, `Unstage`, `Commit`, `Push`, and `Fetch` against the workspace or one
Lane worktree, and `OperatorGitActionFinished` answers it with a typed outcome
followed by a resampled `WorkspaceSourceUpdated`.

The seam is not the one this request proposed, and the difference is worth
naming: rather than a new typed effect on the lane executor, each action
resolves to the *existing agent tool spec* that performs it — `git_add`,
`git_restore`, `git_commit`, `git_push`, and a new `git_fetch` — and executes
through the same tool registry an agent's call goes through. That gets what the
request asked for (the same permission gate, the same append-only facts) and one
thing it did not: a single `viden.toml` rule set governs an operator's commit
bar and an agent's commit, and Viden keeps exactly one git implementation.

The request's "including refusal and conflict" is answered by a distinction the
contract now makes explicit. A refusal that happened *before* anything ran — a
malformed action, a path leaving the target, an unknown Lane, plan mode, a deny
rule, a denied approval — is a `CommandRejected` naming the command. A failure
*after* the gate granted the action is an `OperatorGitActionFinished` carrying
`Failed`, because the effect was attempted and the attempt is audited; Core
classifies git's stderr into `NothingToCommit`, `NonFastForward`,
`AuthenticationRequired`, `RemoteUnreachable`, `NoUpstream`,
`PathOutsideRepository`, or `Other`, so a client renders a localized message per
class and never parses output. The schema-1 extension fixture
`operator-git.json` makes all three canonical: a refused `Stage`, an approved
and completed `Commit`, and a `Push` that failed.

Two behaviors are recorded rather than left implicit. A `Push` with
`set_upstream: false` on a branch with no upstream is refused as
`Failed { NoUpstream }` instead of being run, because `git push <remote>
<branch>` would create an untracked remote branch after which ahead/behind is
unknowable and the sync chip would read "in sync" forever. And `pull`, `merge`,
`rebase`, `commit --amend`, `reset`, force push, branch delete, `switch`,
`checkout`, and `stash` are deliberately excluded, each for a reason recorded in
the frontend integration contract; `pull` is revisited in `0.3.4` once conflict
content can render its result.

GUI status: adopted 2026-09-09 (G1b). The DiffReview commit bar and the titlebar
sync chip both act. Each control sends one `RunOperatorGitAction` through the
CoreClient seam with a client-chosen `command_id`; only an ordered event naming
that id settles it; one action is in flight at a time. The client renders the
three facts this request's answer keeps apart and never merges them: a
`CommandRejected` is Core's pre-effect reason verbatim in an alert, a `Failed`
outcome is the localized sentence for Core's class plus git's `detail` verbatim,
and a `Completed` success line is built from the outcome's *resampled* `source`
rather than from git's output, which is collapsed behind a disclosure with its
own truncation note. The recovery offered is the one this contract names for the
class — fetch for `NonFastForward`, `Push and set upstream` for `NoUpstream` —
and `AuthenticationRequired` gets none, because a retry button there is a retry
loop against a credential the client cannot supply. An `Ask` reaches the
existing permission dock as an ordinary owner-scoped `ApprovalRequested` with
`target.kind = "git"`; the bar names the action it is waiting on and stays
inert until the dock resolves it.

What the client does **not** do, by this contract: no pull, merge, rebase,
`commit --amend`, reset, force push, branch delete, switch, checkout, or stash —
none of them is offered anywhere, and the sync chip's tooltip states in both
languages that pull is excluded rather than leaving the operator to wonder why a
"sync" control only fetches. It does not drive `git` itself and does not fall
back to a shell command. It does not infer success from `output`, and it never
sends the push half of "commit and push" until the commit reports `Completed`;
a failed or refused commit stops the pair and says so.

One client-local limit is worth recording rather than leaving to be
rediscovered. `RunOperatorGitAction` requires an `owner` matching its envelope
owner, and Core publishes owner bindings per Lane, so this client acts only as
the exact `RuntimeOwner` Core bound to the cockpit's selected Lane. A cockpit
with no exactly-bound Lane has no actor to name: the bar and the chip are then
disabled and labelled `D1-OPERATOR-GIT-OWNER`, a client-local code, rather than
sending `RuntimeOwner::default()`, which would record an authorized mutation as
belonging to nobody. This is not a re-opened Core request — the capability works
as specified — but a workspace-scoped operator identity would remove the limit.
It is registered as GUI-CORE-027, which Core delivered on 2026-09-12 as
`runtime.workspace_owner`; this client adopts it in batch G7, and until then the
limit above still describes what ships. The TUI reaches the same conclusion and
refuses the same cases (T1a, 2026-09-09), so `/git` and the DiffReview commit
bar are both inert without a bound Lane.

D1's `apply` unavailable row is no longer unconditional: it survives only for a
Core build that genuinely publishes no `runtime.operator_git`, and then names
the capability rather than implying this entry is still open.

## GUI-CORE-021: Pull request and forge status

The design's titlebar and Lane surfaces carry a "Pull request status" row:
whether the branch has an open PR, its review state, and its checks. Schema 1
models no forge at all — no remote, no pull request record, no review or check
state originating outside the workspace. `CheckRunView` is the local check
runner, not a forge's CI, and `MergeGateView` is Viden's own gate, not a
remote's merge state.

The GUI therefore renders no PR row and no forge badge. It must not derive one
from a branch name, infer a remote from the ahead/behind counts in
`WorkspaceSourceView`, or call a forge API directly: a frontend that talks to a
network service is outside the client boundary, and forge credentials belong to
the same credential path Core already owns.

Close this request when Core publishes a typed forge-status record — the
association between a Lane's branch and a remote pull request, its review
state, and its check results — with the credential and data-egress policy that
makes fetching it safe, plus a canonical `frontend-contract-v1` fixture that
covers a branch with a pull request and one without.

## GUI-CORE-022: Workspace file inventory — CLOSED

History: the command palette ports the TUI jump index's `~` selector, and the
TUI shipped the same gap: `RuntimeViewState` carried lanes, sessions, merge
gates, and approvals, but no inventory of the files in the workspace. There was
no typed path list, no search index, and no read that would let a frontend
enumerate the tree without walking the filesystem itself — which is outside the
client boundary, and would also bypass the permission gate that governs every
other path read. Both clients rendered the `Files` section as exactly one
permanently disabled row naming this request; neither walked the workspace,
shelled out to a file lister, or reconstructed a tree from paths that happen to
appear in evidence records or tool previews.

Core status: delivered as `RuntimeCommand::QueryWorkspaceFiles` ->
`RuntimeEventKind::WorkspaceFilesLoaded` under the extension capability
`runtime.workspace_files`. It is permission-gated at the source: Core consults
the permission engine before it reads a single directory entry, under the
non-mutating tool `workspace_file_inventory` with the workspace root as the
input path — the same gate every other workspace read passes. A deny comes
back as `CommandRejected` naming this exact read and carrying the refusal, and
so does an unresolved ask, because this read answers a keystroke rather than an
interactive turn and must not stall a client behind an approval prompt. Neither
ever publishes an empty page: "you may not read this" and "this workspace has
no files" are different facts. Neither is a bare `Error` either — an event with
no command id would let a client with a read outstanding attribute an unrelated
lane or provider failure to its own read and render a refusal Core never
issued. The tool mutates nothing, so plan mode still answers through the
engine's safe-read branch.

The walk is gitignore-aware (honored outside a Git repository, and reading
neither global nor parent ignore files, so one workspace enumerates identically
on two machines) and unconditionally excludes `.git/`, `.viden/`, `.omx/`,
`.worktrees/`, and `.ref/`, which hold runtime and agent state rather than
workspace content. Entries are sorted lexicographically *before* the prefix
filter, the exclusive `after` cursor, and the `1..=500` limit clamp, so
`complete` and `next_after` describe the filtered ordered inventory; a prefix
that leaves the workspace is rejected rather than clamped or answered with an
empty page. `WorkspaceFilesLoaded.command_id` is required rather than optional,
so unlike an audit page there is no uncorrelated case and no acceptance-first
fallback. The schema-1 extension fixture `workspace-files.json` covers both
halves this request asked for: a project with an inventory (two concurrent
reads answered out of order, only the required command id attributing them) and
a second attached project with lane facts and no inventory read at all.

GUI status: wired on `claude/core-workspace-files`. The palette's `~` scope
lists the paths Core published, in Core's own lexicographic order, read once
when the palette opens through a single-slot pending read mirroring
`PendingAuditPage`. The TUI's jump index does the same through
`apps/tui/src/tui/workspace_files.rs`, reading once per loaded tree. Neither
client walks anything. Absence, in-flight, refusal, and emptiness stay four
different rows in both clients — a missing capability keeps the honest disabled
row naming this request and sends no command at all, a refusal shows Core's own
sentence verbatim, and an answered read over an empty workspace says the
workspace is empty rather than borrowing the unavailable sentence.

## GUI-CORE-023: Concurrent multi-workspace supervision

Core supervises exactly one workspace at a time.
`LocalCoreHost::open_workspace` (`crates/core/src/host.rs:166-234`) builds a
new `RuntimeSupervisor` on every call, and the desktop host swaps its single
`Mutex<Option<GuiCoreAdapter>>` slot (`apps/gui/src-tauri/src/lib.rs:79-94`).
Opening a project therefore *replaces* the open one: dropping the previous
supervisor (`crates/runtime/src/runtime_supervisor.rs:1373-1404`) joins its
worker and shuts down every resident ACP session, so each Lane and Agent
running in the old workspace stops.

That is not what the design draws. `WorkspacePanel` renders several `.wsroot`
project groups plus a cross-project "Global" lane section, `ProjectPicker`
lists an "In workspace" column with a project count and a per-project lane
count, and D13 shows a fleet spanning projects. None of that is expressible
against a single-root supervisor.

Until Core publishes multi-root supervision, the GUI holds these lines:

- the rail renders exactly one group — the open project — with no fabricated
  siblings and no "Global" bucket;
- the picker's "In workspace" column holds exactly one row, marked current and
  non-actionable, because there is nothing to switch *to* within the workspace;
- every other project is a **switch**, guarded by an inline confirmation that
  names the running Lanes and Agent sessions the replacement tears down;
- `Clone repo…` and `New empty project` render disabled naming this request:
  `frontend-contract-v1` publishes no repository-clone and no project-scaffold
  command, and the GUI must not shell out to `git clone` or write a project
  skeleton itself — both are mutations outside the Core permission gate.

Close this request when Core publishes:

1. **N-root supervision** — more than one workspace supervised concurrently,
   with `open_workspace` becoming additive (or gaining an explicit replace flag)
   instead of silently replacing;
2. **a project registry** — the set of attached roots as a typed, ordered fact
   in `RuntimeViewState`, so the rail renders groups from Core rather than from
   one `environment.cwd`;
3. **cross-project lane enumeration** — Lane, session, gate, and approval facts
   readable across attached roots, which is what the design's "Global" section
   and the D13 fleet board actually show;
4. **a `RuntimeOwner.project_id` derivation rule** — a documented, stable
   mapping from a canonical root to the `project_id` already carried in
   `RuntimeOwner`, so a client can attribute an owner-bound fact to a project
   without string-matching a path;
5. **a project bootstrap command** — clone-into-a-workspace and
   scaffold-a-new-project as Core commands under the same permission gate as
   every other mutation, which is what unlocks the two disabled picker rows;
6. **canonical `frontend-contract-v1` fixtures** covering one attached root and
   two, so the grouped rail and the cross-project fleet have generated evidence
   rather than hand-written projections.

The recent-work inventory (`runtime.recent_work`) is *not* this gap: it already
lets a client list projects it could open. What is missing is the ability to
have more than one of them open at once.

## GUI-CORE-024: AuditQuery filters and an AuditPageLoaded command id — CLOSED

D14 reads Core's audit timeline through `RuntimeCommand::QueryAudit` ->
`RuntimeEventKind::AuditPageLoaded` under the `runtime.audit` capability. Two
gaps in that contract shape what the client can honestly build.

**1. `AuditPageLoaded` carries no command id.** The client correlates a page to
its own read acceptance-first: the page counts only after Core accepted *this*
`command_id`, and a second concurrent read is refused locally so there is
always at most one in flight. That rules out a page that arrived before our
acceptance, but a *different* client querying the same Core concurrently could
still land its page between our acceptance and Core's answer, and we would
attribute it to this read. The page is still a real Core page, it is dropped
when the screen re-queries, and the alternative — matching on record contents
or guessing from the cursor — would invent certainty the contract does not
provide, so it is not built. The TUI documents the same limitation in
`apps/tui/src/tui/audit_panel.rs`.

**2. `AuditQuery` has no actor or time filter.** Its filters are `project_id`,
`lane_id`, `object`, and the `before` cursor. The D14 design shows actor and
time-range filter chips and day grouping; a client can only implement those by
filtering a page it already holds, which silently misreports "no records for
this actor" whenever the matching record sits on a page that was never loaded.
D14 therefore ships neither, rather than shipping a filter that lies about
completeness.

**Amended 2026-09-12 (`0.3.4`, batch G6).** D14 now ships both chips as an
explicit *loaded-page* filter rather than not at all. The property this entry
protects is unchanged and is what made the change admissible: every label on
the bar names the loaded page, the outcome rollup's caption says
`Outcomes on the loaded page` and never a total, and filtering to empty says
"no row on the loaded page matches" — a different sentence from "Core recorded
no audit entry for this view". Nothing here claims completeness, so the
misreport this entry warned about cannot be made. The request stays as written:
a server-side filter applied before pagination is still what would let the
chips describe the timeline instead of the page, and the export control the
design draws beside them stays disabled and labelled until a command exists.

Close this request when Core publishes:

1. **a command id on `AuditPageLoaded`**, so a page is attributable to the exact
   read that asked for it and concurrent readers stop being a correlation risk;
2. **server-side `AuditQuery` filters** for actor (operator / system / a named
   agent) and a time range, applied before pagination so `complete` and
   `next_before` describe the filtered timeline;
3. **a canonical `frontend-contract-v1` fixture** covering two concurrent audit
   reads and a filtered page, so correlation and filtering have generated
   evidence rather than hand-written projections.

Client-side follow-ups that need no Core change, and are therefore *not* part of
this request: day grouping of a loaded page, a detail rail for the selected
record, and a rollup of a loaded page. Export needs a host file-write path, not
a Core contract addition.

Core status: delivered, all three close criteria.

1. `AuditPageLoaded.command_id` is the exact `QueryAudit` command id the page
   answers, threaded from the command handler so no other path can mint one. It
   is additive and optional, so a page from a Core that predates the field
   deserializes to `None`.
2. `AuditQuery` gained `actor` (`AuditActorFilter`: operator / system /
   any_agent / a named agent, matched on the full id) and a half-open
   `[from, until)` unix-second range. Core applies them before pagination, so
   `complete` and `next_before` describe the filtered timeline. An inverted
   range is rejected rather than answered with an empty page, and an actor or
   filter variant this build cannot classify matches nothing.
3. The schema-1 extension fixture `audit-reads.json` accepts two reads before
   answering either, returns their pages in the opposite order — so arrival
   order attributes them wrongly and only the published id gets them right —
   and adds a filtered read whose `complete` describes the agent timeline while
   strictly older operator and system records remain visible on the unfiltered
   pages.

Closing this also fixed a real wire gap of the same shape as GUI-CORE-017's:
`audit_page_loaded` was missing from the known schema-1 event types, so every
audit page degraded to a quarantined unknown event on any serialized
snapshot/replay path. It is now a known event type with a types-level
round-trip test.

Client status: exact correlation adopted in both clients. The GUI host
(`apps/gui/src-tauri/src/adapter.rs`) and the TUI panel
(`apps/tui/src/tui/audit_panel.rs`) require a published `command_id` to equal
their own in-flight read, ignore a page naming another read, and keep the
acceptance-gated fallback only for a page with no id. Neither client fabricates
an id.

Remaining client-side follow-up, not blocked on Core: D14 actor and time-range
filter chips over the new `AuditQuery` fields. No client sends an actor or time
filter yet, because no operator control chooses one.

## GUI-CORE-025: Evidence reads — CLOSED

Request: the registered `EvidenceView` surface is a day-grouped archive with a
detail rail — a key-value report, an output tail, chips linked from `metadata`
and `canonical`, and "Open in review" for a `patch` row. The only evidence a
frontend can reach today is `RuntimeViewState.latest_evidence`, and it cannot
carry that surface for three reasons, none of which a client can work around:

1. It is a **recent-window projection** — a client-side reduction of whatever
   event stream that client happened to receive. A client that connected
   mid-session, or one recovering from a sequence gap, holds a strict subset,
   so "these are the last N pieces of evidence" would be a claim about the
   archive it has no basis for.
2. It carries **no ordering rule and no cursor**. It is an upsert-by-id list in
   arrival order, so day grouping would mean the GUI inventing a sort, and
   paging would mean the GUI deciding where a page ends — two definitions of
   the archive, one of them outside Core.
3. It carries **no content**. `EvidenceView` names a canonical reference but
   never the bytes, and the only way to render the report or the tail would be
   for the GUI to open the ContextStore itself — outside the client boundary,
   and bypassing the hash verification that is the entire reason canonical
   evidence means anything.

Core status: delivered as `runtime.evidence_reads`. `QueryEvidence` ->
`EvidencePageLoaded` pages the durable archive Core rebuilds at open from the
append-only workflow agent log, and `ReadEvidenceContent` ->
`EvidenceContentLoaded` answers the bytes behind one row. Two commands and two
events are new, so nothing existing changed shape and the nine frozen base
fixtures keep their bytes.

The answers to the three points above, in order. Ordering is ascending on
`(timestamp, id)`, and a row Core never dated sorts **first**: undated is the
oldest thing Core can honestly say about it, so a forward-paging client meets
it once at the start rather than after rows it already rendered as newer. The
cursor is opaque — `next_after` goes back verbatim as `after`, and the GUI must
not parse, construct, or compare it — and every filter runs before the page is
cut, so `complete` describes the filtered archive rather than the raw one.
Content is read only from canonical ContextStore bytes and only after they
verify against the row's own `source_hash`.

Two answers are worth naming because they are narrower than "give us the
content". `EvidenceContent` has no variant for bytes Core could not verify:
`SummaryOnly` for a row with no canonical reference, `MissingCanonicalBytes`
for one whose bytes are gone, `HashMismatch` for bytes that are present and
fail their own hash, and `Binary` for verified bytes with no text or diff
shape. `HashMismatch` in particular is never served as content — those are
exactly the bytes a reviewer must not be shown — so the detail rail renders a
typed reason there rather than a body. And the gate posture is `QueryAudit`'s
rather than `QueryWorkspaceFiles`': bounded and owner-scoped, never
tool-gated, because the archive is Viden's own state rather than the operator's
tree. Both reads stay answerable in Plan mode.

The canonical fixture is `evidence-reads.json`: two pages tiling one three-row
archive through the exact cursor the first published, a kind-filtered page
`complete` for its filter while the unfiltered archive is not, three content
reads answering text, parsed diff rows, and `Unavailable { SummaryOnly }`, and
an over-limit `kinds` query answered by `CommandRejected` with no page at all.
Those four render identically in a naive client — as "no evidence" — which is
why they share one fixture.

GUI status: adopted 2026-09-09 (G2b). EvidenceView is a centre-pane view in D1
(`apps/gui/src/screens/evidence_view.ts`), opened from the command palette's
`Open evidence` row or `⌘E`/`⌃E`, with both entry points disabled and naming
`runtime.evidence_reads` when Core does not publish it. The first page is one
`QueryEvidence` at limit 50 scoped to the selected Lane's Core-published owner,
the kind chips are sent as `kinds` so Core filters before it cuts the page, and
"Load older" hands `next_after` back verbatim. Selecting a row sends one
`ReadEvidenceContent` — one read in flight, cached per id for the view's
lifetime — and renders `Text`, `Diff` through the shared diff rows, or the
typed unavailable reason. An `EvidenceRecorded` while the view is open marks
the list stale with a banner and a Refresh and never reloads it underneath the
operator. `latest_evidence` is untouched and never merged into the archive
list. Evidence: `evidence`, `evidence-text`, `evidence-unavailable`,
`evidence-empty`, and `evidence-rejected` under
`apps/gui/evidence/main-window-interactions/`.

The residual recorded at adoption is closed as of 2026-09-10 (F1).
`viden-core` re-exported `EvidenceView` but not `EvidenceVerificationState` or
`EvidenceQualityStatus`, so the detail rail could name the row's canonical
reference, producer, and hash but not what Core concluded about the bytes.
Both enums are now on the facade and the report states both verdicts, in both
languages, with an unnamed tag stated as itself. A `failed` verification stays
distinct from an absent reference: it names bytes Core holds and distrusts.

TUI status: adopted 2026-09-10 (T1b). The evidence inspector deferred at the
`0.3.2` supervision checkpoint now pages the archive through `QueryEvidence`
and reads canonical content through `ReadEvidenceContent`, opened from the
decisions overlay's "Evidence…" row and from a `/evidence` system command. See
`docs/release-tui-0.3.4-source-control-parity.md`.

## GUI-CORE-026: Platform credential intake

`RuntimeCommand::StoreCredentialHandle` exists and takes a
`credential_request_id`. Nothing publishes one. `CredentialRequestId` is
documented as "opaque, one-use" and minted behind the trusted local host
boundary, and schema 1 exposes no command, event, or host surface that stages a
secret and returns that id. The only way for the GUI to fill the field would be
to take the raw secret in the client and mint an id itself, which puts secret
bytes in the frontend and forges an identity Core never issued.

D11 therefore projects `credentialIngress` as `available: false` with this code
and refuses `D11Intent::StoreCredentialHandle` before any command is built
(`apps/gui/src-tauri/src/adapter.rs`). Settings draws no per-provider API-key
chip and no add-provider action for the same reason.

Close this request when Core publishes a staging path that returns a
`CredentialRequestId` without the secret crossing the frontend boundary, the
event that reports its outcome, and a canonical `frontend-contract-v1` fixture
covering a staged credential that becomes a `CredentialHandle` and one that is
refused.

## GUI-CORE-027: Workspace-scoped operator identity — CLOSED (Core 2026-09-12; GUI client 2026-09-12)

`RunOperatorGitAction` validates that the command's `owner` equals the
envelope actor, and Core's validator already accepts `lane_id: None` for a
`SourceTarget::Workspace` target. What is missing is upstream of that check:
**Core never mints a `workspace_id` or a `project_id`.** Every non-Lane
command both clients send carries `RuntimeOwner::default()`, and even
`LaneRuntimeOwnerBound` echoes the empty ids the client sent, so the workspace
and project fields are semantically inert across the whole contract — a
pre-existing weakness this request also records, not only an operator-git one.

The consequence is concrete. An audited operator action against the workspace
root has no actor to name. Sending `RuntimeOwner::default()` would record an
authorized mutation as belonging to nobody, which is worse than not offering
the action, so both clients refuse locally instead and cite this number: the
GUI disables the DiffReview commit bar and the titlebar sync control with
`D1-OPERATOR-GIT-OWNER` (`apps/gui/src-tauri/src/operator_git.rs`), and the
TUI disables and labels the `/git` picker rows
(`apps/tui/src/tui/operator_git.rs`). With a Lane selected and an exact Core
owner bound, both work as specified; without one, `runtime.operator_git` is
reachable only through a Lane.

This is an additive contract request, not a defect in `runtime.operator_git`.
The capability behaves as designed for the target it can name.

Core status: delivered 2026-09-12 by batch C5 of the `0.3.4` contract increment
(capability `runtime.workspace_owner`), in the exact shape this entry
investigated. `LocalCoreHost::open_workspace` mints the identity —
`workspace_id` is `ws_` plus the first 16 hex characters of SHA-256 over the
canonical root, `project_id` is read from `.viden/project.toml` or minted there
on the first open — and `WorkspaceRuntimeOwnerBound { binding }` publishes it as
the first fact after `SnapshotUpdated`, carrying a `project_id_origin` that
separates an existing id from one minted on this open.
`RuntimeViewState.workspace_owner` is optional and absent until Core publishes
one, so the two ids stopped being semantically inert without moving any frozen
base fixture. A `RunOperatorGitAction` with `SourceTarget::Workspace` now runs
end to end under that owner and is audited under it; one that names no
workspace, or another workspace, is refused before any process spawns with a
`CommandRejected` quoting this number. The `workspace-owner` fixture is the
canonical evidence: the binding, a Lane created afterwards carrying the same two
ids, the authorized commit with its audit row, and the Lane's own
`LaneSourceUpdated` row — which also ends `WorkspaceSourceUpdated` carrying a
Lane worktree's branch.

GUI client status: adopted 2026-09-12 by batch G7. A workspace-target action
sends the owner from `RuntimeViewState.workspace_owner`
(`apps/gui/src-tauri/src/adapter.rs`, `operator_git_owner`), so the DiffReview
commit bar and the titlebar sync control work with no Lane selected, and
`D1-OPERATOR-GIT-OWNER` is gone from that path. Absence is still a refusal
rather than a default: `D1-OPERATOR-GIT-NO-WORKSPACE-OWNER` leaves both visible,
disabled and labelled with the missing Core fact and sends nothing, which is the
right answer for a Core that publishes no `runtime.workspace_owner`. The TUI
enables its `/git` workspace rows under the same condition in batch T2, and its
own refusal stands until then.

## GUI-CORE-028: Durable evidence for supervisor-driven work — CLOSED (Core 2026-09-12; GUI client 2026-09-12)

Opened 2026-09-10 by the E1 release-evidence pass, after the C5 adjudication of
the same day deferred it out of `0.3.3`. Scheduled for `0.3.4`.

`runtime.evidence_reads` (GUI-CORE-025) answers from the durable archive
`SessionEngine::runtime_evidence`, which is rebuilt at open from the append-only
`runtime_projection` / `runtime_projection_batch` rows of the workflow agent log
(`crates/runtime/src/evidence_reads.rs:1-24`) and written only by the
`EvidenceRecorded` arm of the engine's own reduction
(`crates/runtime/src/session_lifecycle.rs:860`, through
`crates/runtime/src/runtime_tasks.rs:42`). Nothing driven through
`RuntimeSupervisor` — a native Lane turn, an ACP session — reaches that arm.

The consequence, stated for each producer:

- A native tool mutation publishes transcript facts plus a **live**
  `WorkspaceChangeUpdated` / `CheckRunUpdated` derived from the structured tool
  result (`crates/runtime/src/runtime_loop.rs:82-118`, stamped with an owner in
  `crates/runtime/src/frontend_status.rs:262-263`) and a transient `tool_result`
  row in `RuntimeViewState.latest_evidence`. It records no `patch` evidence
  anywhere, so an applied `edit_file` or `write_file` leaves the archive empty.
- ACP publishes `EvidenceRecorded` for patches, tool logs, and turn ends
  (`crates/agents/src/glue.rs:1260`, `:1316`, `:1347`, `:1389`, `:1425`), but on
  the supervisor's bus rather than through the engine, and its patch evidence
  carries `canonical: None`, so even a reduced copy would name no bytes.
- The only durable producers are `StartAgentTask` (a `task_summary`),
  `RecordAgentEvidence`, and the provider-config commands
  (`crates/runtime/src/runtime_contract.rs:732`, `:916`). Neither client
  dispatches any of them.

Two things follow that are not cosmetic. `QueryEvidence` answers "no evidence in
this scope" for a session whose transcript is full of evidence, and both clients
render that honestly, which is the correct behaviour over an archive that is
genuinely empty. And a Lane driven by the native supervisor can never satisfy a
merge gate that requires `patch` evidence, because it never produces any.

Close this request when an applied native mutation and an ACP patch each produce
an archived `patch` row carrying canonical ContextStore bytes and a
`source_hash`, persisted through the same `runtime_projection` rows the archive
rebuilds from, with a `frontend-contract-v1` fixture covering both. The
live-event addition must be checked against the nine frozen base fixtures — they
are static JSON, so only regeneration can move them; the check is that no
regeneration is needed, not that the bytes happen to survive a code change.

Core status: delivered 2026-09-12 by batch C7 of the `0.3.4` contract increment
(capability `runtime.durable_work_evidence`), against each of the three causes
this entry names. An applied `write_file`/`edit_file` now publishes an
`EvidenceRecorded` of kind `patch`, id `patch-<tool_call_id>`, owned by the
turn, immediately after the live `WorkspaceChangeUpdated` that describes the
same change; its canonical bytes are the unified diff the tool produced, stored
in the ContextStore and re-read and re-hashed before the row claims `Verified`,
so `ReadEvidenceContent` serves and verifies them. An ACP patch keeps
`canonical: None` at construction — `viden-agents` owns no store — and the
runtime's ingestion of the batch stores the bytes it carries in `metadata` and
publishes `EvidenceCanonicalized` immediately after the row it completes. And
the supervisor hands every terminal native turn batch to the new
`SessionEngine::absorb_supervised_events`, which applies the existing
`is_durable_runtime_domain_event` and persists through
`persist_workflow_runtime_projection_batch`, so those rows reach the projection
the archive is rebuilt from; a restart test replays the row and serves its
verified bytes. `WorkspaceChangeUpdated` and `CheckRunUpdated` stay live-only,
because they are a view of the working tree that is re-sampled at every connect.

The second consequence this entry names is closed with a rule rather than by
accident. A `patch` row's `producer.task_id` is the owner's task when the turn
is bound to one, so a Lane's native turn now satisfies that Lane's
`patch`-required merge gate; a session-scoped composer turn names its turn
instead and is refused with `MissingProducer`, which is the honest answer rather
than a silent pass. The approval half is closed too: `RespondToApproval` appends
one `AuditRecord` before `ApprovalResolved`, under the pre-minted `audit_id` the
request already showed the operator, so a permission dock's "audit" link
resolves through `QueryAudit` instead of naming a row that was never written.

The `durable-work-evidence` fixture is the canonical evidence: the approval, its
audit row, the archived patch with canonical bytes beside the live change, the
archive page and the content read that both answer it under the hash the row
published, and the agent patch the runtime's ingestion completes. The nine
frozen `frontend-contract-v1` base fixtures are byte-identical to their bytes at
`main` `25072a0a`, verified by digest per file; no regeneration was needed, and
this batch adds no `RuntimeViewState` field.

GUI client status: adopted 2026-09-12 by batch G7, and it needed no new client
vocabulary — which is the point of the increment: EvidenceView already read
`runtime.evidence_reads`, so the archived `patch` rows the runtime started
writing appear with their canonical reference, and `ReadEvidenceContent` draws
their bytes as Core's parsed diff rows in the shared renderer
(`apps/gui/tests/evidence_reads.rs`, the `durable-work-evidence` fixture replay).
D14 keeps Core's `approval.*` action, its operator actor and all three objects
verbatim, and the permission dock links the trail by the permission object
rather than by an audit id no filter expresses. The D2 queue item deliberately
offers no trail: the row is appended when the decision is applied, and a
pending item's timeline is empty. The TUI's evidence inspector detail gains live
content in batch T2. Neither client should present an absent
`permission_snapshot_id` as approved, or a `canonical: None` row as
display-only evidence: both are stated facts with their own copy.

## GUI-CORE-030: Agent binding on the starter-Lane request

Opened 2026-09-12 by batch G7, which numbered a request batch G4 wrote as prose
in the D4 wizard's own copy rather than as a register entry (the register was
frozen while `0.3.4` was in flight). Not scheduled; no client work depends on
it beyond the sentence already shipped.

`StarterLaneRequest` carries a role, a mutation policy and an execution target,
and **no agent binding**. So the design's "choose agent" step can show Core's
published adapters and mark the one the operator picked, and creating the Lane
still starts no Agent session: the pick reaches Core nowhere. The GUI says so on
the step (`apps/gui/src/screens/d4_lane_create.ts`, `d4.agent.noBinding`) rather
than offering a selection that cannot travel, and starting a session stays a
second, explicit `StartAgentSession` against the created Lane.

What a client needs: either an optional `agent_id` on `StarterLaneRequest`,
applied under the same permission gate `StartAgentSession` passes, or Core's
statement that the two-step flow is the contract — in which case the wizard's
step is documentation and the register entry closes as by-design. The GUI needs
no new event either way: `AgentSessionUpdated` already reports what started.

## GUI-CORE-031: Skill packs and context injection

Opened 2026-09-12 by batch G7, numbering the second half of the same G4 prose.
Not scheduled.

`frontend-contract-v1` publishes **no** skill, skill-pack, prompt-pack or
context-injection fact anywhere — not in `RuntimeViewState`, not on any
command, not on `StarterLaneRequest` — while the accepted design draws a "skill
pack" step in Lane creation and the `runtime.cockpit_context_v1` capability
covers only budgets, sources and evidence references. The GUI renders the step
stated-unavailable with that exact reason (`d4.skills.unavailable`) and ships no
local list, because a client-owned pack would be a second authority over what a
Lane is allowed to read.

What a client needs, in the order it could adopt them: a read of the packs Core
holds (id, label, and what they inject), a field on the creation request naming
one, and the fact that says which pack a running Lane resolved. Without the
third, a client could select a pack and never show whether it took effect.

## GUI-CORE-032: Integrated terminal facts

Opened 2026-09-12 by batch G7, numbering a gap batch G5 stated in the context
dock's own tab copy. Not scheduled; the design registers the surface with a
roadmap mark (`D-RAILNAV` ⑥), so this entry exists to make the Core side of it
lookup-able rather than to schedule it.

The dock's Terminal tab cannot open because Core publishes no PTY fact at all:
no session to attach to, no output stream, no write command, no exit status.
`runtime.commands` carries operator *git* actions and tool approvals, and
nothing there spawns or attaches an interactive process. The GUI keeps the tab
visible and disabled with that sentence (`d1.dock.tab.terminal.reason`) rather
than hiding it, because a tab that vanishes makes the dock look finished.

What a client needs: a Core-owned PTY session — open/attach under the
permission engine, ordered output chunks with a bound, a write command, and a
terminal status fact — or the statement that the integrated terminal is out of
scope for this contract, which would close this entry as by-design and leave the
tab's copy as the permanent answer.

## GUI-CORE-033: Workspace document facts

Opened 2026-09-12 by batch G7, numbering the dock's other disabled tab from the
same G5 pass. Not scheduled.

The Docs tab has no Core fact behind it in any shape: `runtime.workspace_files`
lists paths and `runtime.workspace_file_reads` (C9) reads one file's bytes, but
neither says which documents *matter* — the project's own `AGENTS.md` chain, the
design entry points, the release status document — nor renders one as a
document. A client could guess by path, and that guess would be a second
authority over which file is the policy for a scope, which is exactly what the
nested-`AGENTS.md` rule makes Core's business.

What a client needs: the document set for a scope (path, kind, and the scope it
governs), read through the file read that already exists. Until then the GUI
keeps the tab disabled and named (`d1.dock.tab.docs.reason`) and the palette's
`~` rows stay the only way to a file.

## Retired pre-register codes

`GUI-CORE-001` to `GUI-CORE-007` predate this register, which starts at 008.
They were never entries here, so a projection citing one named nothing a reader
could look up. Where they went:

| Retired code | Was | Today |
| --- | --- | --- |
| `GUI-CORE-001` | typed project intake, config preview/confirm, masked credential handles | project onboarding shipped; the credential half is GUI-CORE-026 |
| `GUI-CORE-002` | lane lifecycle commands and starter-lane creation | shipped; D4 and the starter-lane path send Core commands |
| `GUI-CORE-003` | structured connection and lane-recovery facts | mostly shipped; kept as the *rendered* fail-closed code for a D6 action schema 1 does not model, whose open requests are GUI-CORE-018 (checkpoint) and GUI-CORE-019 (Always / Edit) |
| `GUI-CORE-004` | stable append-only audit timeline | delivered; closed as GUI-CORE-014 and GUI-CORE-024 |
| `GUI-CORE-005` | preference mutation and persistence | shipped as the `ui.preference_persistence` capability |
| `GUI-CORE-006` | structured diff / test / apply / conflict / retry facts | split: diff is GUI-CORE-012, conflict content is GUI-CORE-015, operator apply and commit are GUI-CORE-020; check runs shipped |
| `GUI-CORE-007` | paginated recent project and session history | shipped as the `runtime.recent_work` capability |

`GUI-CORE-003` is the one code still emitted by production projections. It is
deliberate and documented in GUI-CORE-018 and GUI-CORE-019: those entries name
it as the code the GUI renders while the capability behind the action is
missing. Every other projection now cites an entry above.

## Client-local reason codes

Not every unavailable control is a Core gap. A control this client refuses to
offer for a reason of its own carries a screen-scoped code with no `GUI-CORE-`
prefix, so a reader can tell a contract request from a local rule at a glance.

| Code | Where | Means |
| --- | --- | --- |
| `D1-OWNER-CARDINALITY` | `apps/gui/src-tauri/src/d1.rs`, applied in the D1 cockpit projection | The selected Lane carries more than one exact runtime owner binding. Core's reducer keeps at most one per Lane, so this is a view the client refuses to render — it enters snapshot/replay recovery instead of choosing an owner — not a fact Core still owes it. |
| `D2-REVIEW-SETTLED` | `apps/gui/src-tauri/src/d2.rs` | Core already recorded this review's verdict; `DecideReview` settles a `Pending` review once. |
| `D2-NO-REVIEWER-ACTOR` | `apps/gui/src-tauri/src/d2.rs` | No actor `validate_review_decider` would accept is derivable from the published facts for this review. |
