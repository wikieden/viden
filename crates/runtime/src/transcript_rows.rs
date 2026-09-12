//! Producers for the owner-scoped transcript rows
//! (`runtime.transcript_rows`, C8, closes GUI-CORE-009).
//!
//! **Durable facts only.** Every row here is derived from an append-only log:
//! the session transcript JSONL that `SessionStore` owns, and the audit
//! timeline that `WorkflowStore` owns. Nothing is read from
//! `RuntimeViewState`, and that is the whole point. The live view's
//! `agent_conversation` is a *client-side reduction of whatever stream that
//! client received*: it is capped, it holds only this connection's window, and
//! it is empty after a restart. Paging it would answer "what was said" with
//! "what you happened to see" — the failure GUI-CORE-009 reported from the
//! other side, as two permanently unavailable placeholders.
//!
//! **Attribution comes from the log, not from timing.** A transcript file
//! holds more than one owner's work: the supervised input path runs a Lane's
//! native turn through the same engine as the session composer, so a Lane's
//! rows and the session's rows are interleaved lines of one file. Guessing
//! which is which from ordering would be exactly the inference the owner
//! contract exists to remove, so the turn driver writes the owner into the log
//! itself: [`SessionEngine::record_turn_owner_marker`] appends a
//! `turn_owner` session-meta entry when a native turn opens and a closing one
//! when it ends, and rows between them carry that owner verbatim.
//!
//! The marker is a session-meta key, which is the one persisted entry shape
//! whose unknown keys every replayer already ignores (`hydrate` matches
//! `work_mode`, `permission_mode`, `model`, `provider`, and drops the rest), so
//! an older build reading a newer transcript loses attribution rather than
//! quarantining the line.
//!
//! **A row Core could not attribute names only the session it is in.** That is
//! not a fabricated owner: the row really is a line of that session's log, and
//! it is all Core knows. Such a row never satisfies a Lane-scoped query,
//! because the shared owner matcher requires a set field to match exactly.
//!
//! **A row's text is the persisted text.** `TranscriptEntry::from_json_line`
//! decodes a persisted JSON string as UTF-8 (`viden_types::transcript`), so a
//! non-ASCII body replays byte-equal and the character-boundary bound below
//! cuts real characters. That held for none of the readers of the session log
//! until C11 fixed the decoder for all three — resume, the base
//! `runtime.transcript_page`, and these rows.
//!
//! **Ordering is stable under append.** Row positions are derived from the
//! source's own position rather than from a rank over the merged set, so a new
//! audit row landing with an old timestamp cannot renumber rows a client has
//! already paged. See [`owned_transcript_rows`].

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use viden_types::{
    AuditObjectRef, AuditRecord, EvidenceView, Message, OwnedTranscriptRow, Role, RuntimeOwner,
    ToolCall, ToolCallView, TranscriptEntry, TranscriptRowContent, TranscriptRowsPage,
    bound_transcript_row_text, truncate_for_preview,
};

use crate::work_evidence::native_patch_evidence_id;

/// Session-meta key carrying the owner of the native turn that follows.
///
/// An empty value closes the bracket. Both halves are written, because without
/// the closing one every entry appended after a turn ended — a slash command's
/// log line, a cost record — would inherit that turn's owner, and a row
/// attributed to a turn that was already over is a false attribution rather
/// than a missing one.
pub(crate) const TURN_OWNER_META_KEY: &str = "turn_owner";

/// Byte bound on the marker's JSON value.
///
/// An owner is six short ids, so this is generous. It exists because the marker
/// is written on the turn's hot path: an owner that somehow serialized larger
/// than this is not written at all, and the rows of that turn fall back to
/// naming only their session — the fail-closed outcome, never a truncated owner
/// that would name a *different* Lane.
const MAX_TURN_OWNER_MARKER_BYTES: usize = 2048;

/// Preview bound on a tool call's input and a tool result's output.
///
/// 500 bytes, the same bound the live `ToolCallStarted` preview and the live
/// `tool_result` evidence summary already use, so one tool block reads the same
/// whether it arrived on the stream or through this read. Neither field carries
/// a `truncated` flag, which is why they are previews rather than bodies: the
/// full bytes live in the evidence a row names.
const TOOL_PREVIEW_BYTES: usize = 500;

/// Prefix of the durable audit action C7 writes for an approval decision.
const APPROVAL_AUDIT_ACTION_PREFIX: &str = "approval.";

/// The owner a row carries when Core attributed it to no turn.
///
/// The session it was written into and nothing else. Deliberately not
/// `RuntimeOwner::default()`: an all-empty owner would answer a query naming
/// this session, which would be a claim Core cannot make about a row it found
/// in a different log.
fn session_scope_owner(session_id: &str) -> RuntimeOwner {
    RuntimeOwner {
        session_id: Some(session_id.to_string()),
        ..RuntimeOwner::default()
    }
}

/// Decodes one `turn_owner` marker value.
///
/// An empty value closes a bracket. A value this build cannot parse also closes
/// it: inheriting the previous owner across an unreadable marker would attribute
/// a turn's rows to the turn before it.
fn decode_turn_owner_marker(value: &str) -> Option<RuntimeOwner> {
    if value.trim().is_empty() {
        return None;
    }
    serde_json::from_str::<RuntimeOwner>(value).ok()
}

/// Builds every transcript row this Core can order and attribute.
///
/// Rows carry their **full** text: bounding happens after the page is cut, in
/// [`bound_and_join_page_rows`], so the byte bound never has to be applied to a
/// row nobody asked for and the evidence join can still see the whole body.
///
/// `sequence` is derived from the source position, not from a rank over the
/// merged set:
///
/// - a transcript row at entry ordinal `o` gets `2 * o + 1`;
/// - an audit-derived permission row that belongs after the first `a` entries
///   gets `2 * a`.
///
/// The odd/even split gives one total order in which a permission row sits
/// exactly where its timestamp puts it, and it is stable under append: a new
/// audit row landing with an old timestamp takes a position of its own instead
/// of renumbering the transcript rows a client has already paged with a cursor.
pub(crate) fn owned_transcript_rows(
    session_id: &str,
    entries: &[TranscriptEntry],
    audit_records: &[AuditRecord],
) -> Vec<OwnedTranscriptRow> {
    let mut rows = Vec::new();
    let mut current_owner: Option<RuntimeOwner> = None;
    // Tool calls seen so far, so a result can be read together with the call
    // that produced it — which is what tells a check run apart from a plain
    // tool result.
    let mut pending_calls: BTreeMap<String, ToolCall> = BTreeMap::new();
    // The non-decreasing timestamp anchor of each entry position, used to place
    // the audit-derived rows. An entry Core recorded without a time inherits
    // the last known one rather than sorting to the epoch.
    let mut anchors: Vec<u64> = Vec::with_capacity(entries.len());
    let mut last_timestamp = 0u64;

    for (ordinal, entry) in entries.iter().enumerate() {
        if let Some(timestamp) = entry.timestamp() {
            last_timestamp = last_timestamp.max(timestamp);
        }
        anchors.push(last_timestamp);

        if let TranscriptEntry::SessionMeta { entry } = entry
            && entry.key == TURN_OWNER_META_KEY
        {
            current_owner = decode_turn_owner_marker(&entry.value);
            continue;
        }

        let owner = current_owner
            .clone()
            .unwrap_or_else(|| session_scope_owner(session_id));
        let Some(content) = row_content(entry, &owner, &mut pending_calls) else {
            continue;
        };
        rows.push(OwnedTranscriptRow {
            id: format!("{session_id}:{ordinal}"),
            owner,
            sequence: (ordinal as u64) * 2 + 1,
            timestamp: entry.timestamp(),
            content,
        });
    }

    for record in audit_records {
        let Some(row) = permission_row(record, &anchors) else {
            continue;
        };
        rows.push(row);
    }
    rows
}

/// Maps one persisted entry onto the row a transcript surface renders, or
/// `None` for an entry that is not part of the conversation.
///
/// The deliberate omissions: a `Permission` entry, because
/// [`viden_types::PermissionLogEntry`] carries neither the approval request id
/// nor the audit id a `Permission` row must name, and inventing either would
/// produce a row whose "audit" link resolves to nothing — the durable audit
/// timeline is the honest source and [`permission_row`] reads it. A `Command`,
/// `SessionMeta`, `CostUsage`, or replayed `RuntimeEvent` entry, because none
/// of them is something said in the conversation. And the assistant message
/// that *announces* a tool call, whose `tool_call_id` is set: the `ToolCall`
/// entry beside it is that row, and publishing both would render one call
/// twice.
fn row_content(
    entry: &TranscriptEntry,
    owner: &RuntimeOwner,
    pending_calls: &mut BTreeMap<String, ToolCall>,
) -> Option<TranscriptRowContent> {
    match entry {
        TranscriptEntry::Message { message } => message_row_content(message),
        TranscriptEntry::ToolCall { call } => {
            pending_calls.insert(call.id.clone(), call.clone());
            Some(TranscriptRowContent::ToolCall {
                call: ToolCallView {
                    tool_call_id: call.id.clone(),
                    name: call.name.clone(),
                    input_preview: truncate_for_preview(
                        &viden_types::encode_tool_input(&call.input),
                        TOOL_PREVIEW_BYTES,
                    ),
                    owner: Some(owner.clone()),
                },
            })
        }
        TranscriptEntry::ToolResult { result } => {
            // A check run is its own row rather than a tool result wearing a
            // label, because that is the distinction the transcript surface
            // draws — and it is derived by the same function the live cockpit
            // fact uses, so one shell command cannot be a check on the stream
            // and a plain result in the transcript.
            if let Some(call) = pending_calls.get(&result.tool_call_id)
                && let Some(check) =
                    crate::frontend_status::check_run_from_tool_result(call, result, owner)
            {
                return Some(TranscriptRowContent::CheckRun { check });
            }
            Some(TranscriptRowContent::ToolResult {
                tool_call_id: result.tool_call_id.clone(),
                success: result.success,
                summary: truncate_for_preview(&result.output, TOOL_PREVIEW_BYTES),
                // Resolved against the archive after the page is cut.
                evidence_id: None,
            })
        }
        _ => None,
    }
}

fn message_row_content(message: &Message) -> Option<TranscriptRowContent> {
    match message.role {
        Role::User => Some(TranscriptRowContent::User {
            text: message.content.clone(),
            truncated: false,
        }),
        // The tool-call announcement is not prose; the `ToolCall` entry is its
        // row.
        Role::Assistant if message.tool_call_id.is_none() => {
            Some(TranscriptRowContent::Assistant {
                text: message.content.clone(),
                truncated: false,
                evidence_id: None,
            })
        }
        _ => None,
    }
}

/// Builds the `Permission` row for one durable approval audit record.
///
/// `None` for every audit action that is not an approval decision, and for one
/// that carries no permission request object: a permission row that could not
/// name the request it answered would be a decision about nothing.
fn permission_row(record: &AuditRecord, anchors: &[u64]) -> Option<OwnedTranscriptRow> {
    let scope_key = record.action.strip_prefix(APPROVAL_AUDIT_ACTION_PREFIX)?;
    let request_id = record
        .objects
        .iter()
        .find(|object| object.kind == AuditObjectRef::KIND_PERMISSION)
        .map(|object| object.id.clone())?;
    // Number of transcript entries this decision happened at or after. The
    // anchors are non-decreasing, so this is the insertion position.
    let anchor = anchors
        .iter()
        .filter(|anchor| **anchor <= record.timestamp)
        .count();
    Some(OwnedTranscriptRow {
        id: format!("approval:{}", record.audit_id),
        owner: record.owner.clone(),
        sequence: (anchor as u64) * 2,
        timestamp: Some(record.timestamp),
        content: TranscriptRowContent::Permission {
            request_id,
            decision: decision_for_audit_scope(scope_key),
            audit_id: record.audit_id.clone(),
        },
    })
}

/// Reconstructs the operator's decision from the durable audit row, or admits
/// that the row does not hold it.
///
/// `allow_session` names a session and `allow_repo` names a path allow-list,
/// and the audit record keeps neither: it stores the scope *key* only.
/// Publishing `Allow { Once }` for either would misreport a standing grant as a
/// one-shot one, so those two answer `None`. `None` here means "Core did not
/// record which", never "no decision was made" — `audit_id` is present on every
/// permission row and is the row a client reads for the rest.
fn decision_for_audit_scope(scope_key: &str) -> Option<viden_types::ApprovalDecision> {
    use viden_types::{ApprovalDecision, ApprovalScope};
    match scope_key {
        "allow_once" => Some(ApprovalDecision::Allow {
            scope: ApprovalScope::Once,
        }),
        "deny" => Some(ApprovalDecision::Deny),
        _ => None,
    }
}

/// Bounds the text of the rows in one page and joins them to the archive.
///
/// Runs after the page is cut so the bound and the digest are computed for the
/// rows a client actually asked for, not for the whole session. The joins:
///
/// - a **tool result** names the archived `patch` row C7 recorded for that exact
///   tool call, resolved through the shared id helper so the two cannot drift;
/// - an **assistant body** names an archived row whose canonical `source_hash`
///   is the digest of that body. It is a content join because that is the only
///   honest one available: nothing in this build records an evidence row *for*
///   an assistant message, so the read matches on the bytes or names nothing.
///   It never creates a row — an absent `evidence_id` means no canonical bytes
///   exist for that body, not that the read withheld them.
pub(crate) fn bound_and_join_page_rows(page: &mut TranscriptRowsPage, archive: &[EvidenceView]) {
    for row in &mut page.rows {
        match &mut row.content {
            TranscriptRowContent::User { text, truncated } => {
                let (bounded, cut) = bound_transcript_row_text(text);
                *text = bounded;
                *truncated = cut;
            }
            TranscriptRowContent::Assistant {
                text,
                truncated,
                evidence_id,
            } => {
                *evidence_id = canonical_evidence_id_for_body(archive, text);
                let (bounded, cut) = bound_transcript_row_text(text);
                *text = bounded;
                *truncated = cut;
            }
            TranscriptRowContent::ToolResult {
                tool_call_id,
                evidence_id,
                ..
            } => {
                let expected = native_patch_evidence_id(tool_call_id);
                *evidence_id = archive
                    .iter()
                    .find(|entry| entry.id == expected && entry.canonical.is_some())
                    .map(|entry| entry.id.clone());
            }
            _ => {}
        }
    }
}

/// The archived row whose canonical bytes are exactly `body`, if one exists.
fn canonical_evidence_id_for_body(archive: &[EvidenceView], body: &str) -> Option<String> {
    if archive.is_empty() || body.is_empty() {
        return None;
    }
    let digest = format!("{:x}", Sha256::digest(body.as_bytes()));
    archive
        .iter()
        .find(|entry| {
            entry
                .canonical
                .as_ref()
                .is_some_and(|canonical| canonical.source_hash == digest)
        })
        .map(|entry| entry.id.clone())
}

impl crate::SessionEngine {
    /// Writes the durable owner marker that makes a native turn's rows
    /// attributable (`runtime.transcript_rows`).
    ///
    /// Called from both turn drivers through `begin_native_turn` and
    /// `end_native_turn`, so the bracket in the log matches the bracket in
    /// memory. An append failure is *not* propagated: the caller is opening or
    /// closing a turn that is about to run, and refusing to run work because a
    /// read-side attribution line could not be written would trade a complete
    /// turn for a complete transcript read. The fail-closed consequence is
    /// stated instead — without the marker those rows name only their session,
    /// so they never answer a Lane-scoped query.
    pub(crate) fn record_turn_owner_marker(&self, owner: Option<&RuntimeOwner>) {
        let value = match owner {
            Some(owner) => match serde_json::to_string(owner) {
                Ok(value) if value.len() <= MAX_TURN_OWNER_MARKER_BYTES => value,
                // A truncated owner could name a different Lane, so the marker
                // is omitted rather than cut.
                _ => return,
            },
            None => String::new(),
        };
        // Appended straight to the store rather than through `persist_meta`,
        // which carries the test-only transcript-append failure injection. The
        // marker is read-side attribution, not one of the turn's own facts, and
        // a test that asks "what happens when the Nth *turn fact* fails to
        // persist" must keep meaning that — otherwise this line would silently
        // become the failure those tests injected.
        let _ = self.store.append_entry(&TranscriptEntry::SessionMeta {
            entry: viden_types::SessionMetaEntry {
                timestamp: viden_types::now_timestamp(),
                key: TURN_OWNER_META_KEY.to_string(),
                value,
            },
        });
    }

    /// Answers one owner-scoped transcript rows read.
    ///
    /// The sources are re-read for every page rather than cached, because both
    /// are append-only files that other paths write: a cached row set would let
    /// a scroll-up miss work that happened while the client was scrolled up.
    /// The transcript is the session the engine currently has open, which after
    /// a resume is the resumed one.
    pub(crate) fn durable_transcript_rows(&self) -> Result<Vec<OwnedTranscriptRow>, String> {
        let entries = self.store.load_entries()?;
        // A broken audit log is a refusal, not a silent omission: answering
        // with the transcript rows alone would publish a conversation that
        // quietly lost every approval in it, and an operator reading a missing
        // approval row concludes nothing was ever asked.
        let audit_records = self.workflows.load_audit_records()?;
        Ok(owned_transcript_rows(
            self.session_id(),
            &entries,
            &audit_records,
        ))
    }
}
