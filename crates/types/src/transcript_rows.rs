//! Owner-scoped typed transcript rows (`runtime.transcript_rows`, C8,
//! closes GUI-CORE-009).
//!
//! The base `runtime.transcript_page` capability publishes
//! [`crate::TranscriptPage`], and it is deliberately left untouched: it is one
//! session's storage log, paged by `(session_id, ordinal)`, and every row it
//! carries is a persisted entry shape — a cost record, a session-meta pair, a
//! replayed runtime event. It answers "what is in this session's file".
//!
//! A cockpit needs a different question answered: "what happened in *this
//! owner's* conversation, in order, in shapes I can render". Those are not the
//! same read, and a client cannot derive the second from the first without
//! inventing the parts Core never sent — which is exactly what GUI-CORE-009
//! reported. `transcript_user` and `transcript_assistant` were unavailable
//! placeholders because no ordered owner-scoped row existed to fill them.
//!
//! This module is that read contract, and four invariants make its answers
//! honest:
//!
//! 1. **A row's owner is never widened.** [`OwnedTranscriptRow::owner`] is the
//!    full owner Core recorded for the turn that produced the row, and
//!    [`TranscriptRowsQuery::owner`] is a *prefix scope* match over it — the
//!    same matcher [`crate::EvidenceQuery`] applies to the evidence archive,
//!    reused rather than reimplemented so one narrowing rule governs both
//!    reads. It fails closed both ways: a query naming a Lane is never
//!    answered by a row Core could not attribute to that Lane, and a row whose
//!    owner names no Lane never answers a Lane-scoped query.
//! 2. **The cursor is opaque.** [`TranscriptRowsPage::older`] is a string a
//!    client passes back verbatim as [`TranscriptRowsQuery::before`]. It
//!    encodes the ordering position below, and clients must not parse,
//!    construct, or compare it: a client that reconstructed one would be
//!    re-deriving Core's ordering rule from a string, which is the coupling the
//!    opaque form exists to prevent.
//! 3. **Paging walks backwards, and a page reads forwards.** A transcript is
//!    read at its newest end and scrolled up, so `before` is an exclusive upper
//!    bound and a page holds the newest rows below it, **newest last**. That is
//!    the order a transcript renders in, so a client appends a page above what
//!    it already holds rather than reversing it first.
//! 4. **Bounded and honest.** Row text is cut on a character boundary at
//!    [`MAX_TRANSCRIPT_ROW_TEXT_BYTES`] and says so with `truncated`;
//!    `evidence_id` is populated only where a canonical evidence row already
//!    exists for that body, never minted by the read; and `None` always means
//!    Core did not know, never a default.
//!
//! Both enums are `#[non_exhaustive]`, and the answer is a query result rather
//! than a runtime fact: the reducer stores no part of it, so publishing a page
//! moves no snapshot digest.

use serde::{Deserialize, Serialize};

use crate::evidence_reads::owner_scope_matches;
use crate::{ApprovalDecision, CheckRunView, RuntimeOwner, ToolCallView};

/// Largest page one [`TranscriptRowsQuery`] may return.
pub const MAX_TRANSCRIPT_ROWS_PAGE: u16 = 200;

/// Page size a client gets when it expresses no preference.
pub const DEFAULT_TRANSCRIPT_ROWS_PAGE: u16 = 50;

/// Byte bound on the text one transcript row publishes.
///
/// 8 KiB, deliberately far below the 256 KiB an evidence content read serves.
/// A page carries up to [`MAX_TRANSCRIPT_ROWS_PAGE`] rows on one event that
/// every connected client receives, so the per-row bound is what stops a
/// scroll-up from putting a session-sized payload on the stream. A body cut by
/// it is not lost: the full bytes stay in the canonical evidence a row names,
/// when one exists.
pub const MAX_TRANSCRIPT_ROW_TEXT_BYTES: u32 = 8 * 1024;

/// Read-only query over one owner's ordered transcript rows.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TranscriptRowsQuery {
    /// Prefix scope match on the owner: every field the query sets must equal
    /// the row's, and the fields it leaves unset match anything. A default
    /// (all-unset) owner reads every row this build can order.
    #[serde(default)]
    pub owner: RuntimeOwner,
    /// Exclusive upper bound: the opaque cursor of the oldest row already
    /// delivered. Paging walks newest to oldest, one page at a time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// Clamped to `1..=`[`MAX_TRANSCRIPT_ROWS_PAGE`]; `None` uses
    /// [`DEFAULT_TRANSCRIPT_ROWS_PAGE`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u16>,
}

impl TranscriptRowsQuery {
    /// Page size actually used, clamped rather than rejected so a malformed
    /// client request still gets a well-formed page. An unclamped `0` would
    /// answer every read with an empty page, which a client cannot tell apart
    /// from an exhausted transcript.
    pub fn clamped_limit(&self) -> usize {
        self.limit
            .unwrap_or(DEFAULT_TRANSCRIPT_ROWS_PAGE)
            .clamp(1, MAX_TRANSCRIPT_ROWS_PAGE) as usize
    }

    /// Rejects a query that cannot mean what it says.
    ///
    /// Unlike `limit`, an undecodable cursor is not ignored. Silently reading
    /// from the newest end would re-deliver a page the client already rendered
    /// and look like a duplicated conversation; silently reading from the
    /// oldest end would look like an exhausted one. Both are worse than a
    /// stated refusal, and the refusal carries this exact read's command id.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(before) = self.before.as_deref() {
            TranscriptRowsCursor::decode(before)?;
        }
        Ok(())
    }

    /// Whether this query keeps `row`, before ordering and paging.
    pub fn matches(&self, row: &OwnedTranscriptRow) -> bool {
        owner_scope_matches(&self.owner, &row.owner)
    }
}

/// Stable oldest-first ordering position of one transcript row.
///
/// `Ord` is derived and the field order carries the rule: `sequence` first,
/// then `id` to break a tie so two rows Core recorded at the same position
/// still page deterministically.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TranscriptRowsCursor {
    pub sequence: u64,
    pub id: String,
}

impl TranscriptRowsCursor {
    pub fn of(row: &OwnedTranscriptRow) -> Self {
        Self {
            sequence: row.sequence,
            id: row.id.clone(),
        }
    }

    /// The opaque wire form clients pass back verbatim.
    ///
    /// Tagged with a version-free prefix and positional afterwards, with the id
    /// taking the rest of the string, so an id containing the separator still
    /// round-trips.
    pub fn encode(&self) -> String {
        format!("s:{}:{}", self.sequence, self.id)
    }

    pub fn decode(raw: &str) -> Result<Self, String> {
        let invalid = || {
            format!(
                "transcript row cursor `{raw}` is not a cursor this build issued\nhint: pass back \
                 the `older` cursor from the previous page verbatim, or omit it to read the newest \
                 page"
            )
        };
        let rest = raw.strip_prefix("s:").ok_or_else(invalid)?;
        let (sequence, id) = rest.split_once(':').ok_or_else(invalid)?;
        if id.is_empty() {
            return Err(invalid());
        }
        Ok(Self {
            sequence: sequence.parse::<u64>().map_err(|_| invalid())?,
            id: id.to_string(),
        })
    }
}

/// One ordered transcript row, attributed to exactly one owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedTranscriptRow {
    /// Stable within this Core's durable facts, so a client can key a rendered
    /// row on it across pages and across reconnects.
    pub id: String,
    /// The full owner Core recorded, never widened and never defaulted to a
    /// scope the row does not belong to.
    pub owner: RuntimeOwner,
    /// Core's recorded ordering position. It is an ordering key, not a count:
    /// positions are stable under append so a cursor stays valid, which means
    /// they are not contiguous and a client must never treat a gap as a lost
    /// row.
    pub sequence: u64,
    /// `None` for a row Core recorded without a time — the durable tool-call
    /// and tool-result entries carry none — and never a substituted zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u64>,
    pub content: TranscriptRowContent,
}

/// What one transcript row says.
///
/// `#[non_exhaustive]` so a later row kind cannot break a client match. The
/// variants are the shapes a transcript surface renders, not the shapes storage
/// happens to use: that mapping is Core's job, and doing it here is what lets a
/// client stop pattern-matching on persisted entry types it has no contract
/// for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum TranscriptRowContent {
    /// Operator prose. `truncated` says
    /// [`MAX_TRANSCRIPT_ROW_TEXT_BYTES`] cut it, never that the input was
    /// short.
    User { text: String, truncated: bool },
    /// Model prose.
    ///
    /// `evidence_id` names a canonical evidence row that *already* holds this
    /// body — it is how a truncated reply stays readable in full through the
    /// evidence content read. The read never creates such a row: absence means
    /// no canonical bytes exist for this body, not that they were withheld.
    Assistant {
        text: String,
        truncated: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        evidence_id: Option<String>,
    },
    /// A tool call as the cockpit already renders it elsewhere, so one tool
    /// block has one shape whether it arrived live or through this read.
    ToolCall { call: ToolCallView },
    /// The outcome of one tool call. `evidence_id` names the archived row for
    /// the work it did — C7's `patch` row for an applied mutation — when one
    /// exists.
    ToolResult {
        tool_call_id: String,
        success: bool,
        summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        evidence_id: Option<String>,
    },
    /// A check run, in the same shape the live cockpit fact carries. A check is
    /// its own row rather than a tool result with a label, because that is the
    /// distinction the transcript surface draws.
    CheckRun { check: CheckRunView },
    /// One permission decision, joined to the durable audit row that recorded
    /// it.
    ///
    /// `decision` is optional because the audit row does not keep a scoped
    /// allow's payload: an `allow_session` names a session and an `allow_repo`
    /// names paths, and neither is in the durable record. Publishing a bare
    /// `Allow { Once }` for them would misreport the operator's decision, so
    /// Core says it does not know instead. `audit_id` is always present — it is
    /// the row this fact was read from.
    Permission {
        request_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decision: Option<ApprovalDecision>,
        audit_id: String,
    },
}

/// One page of ordered transcript rows, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TranscriptRowsPage {
    /// Oldest first, newest last: the order a transcript renders in.
    #[serde(default)]
    pub rows: Vec<OwnedTranscriptRow>,
    /// Opaque cursor to pass as the next query's `before`, to read the page
    /// above this one. `None` when complete. Clients pass it back verbatim and
    /// never parse it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub older: Option<String>,
    /// True when no older row matches the query. Describes the *scoped*
    /// transcript, because Core applies the owner scope before cutting the
    /// page — a client filtering a page it already holds could not know whether
    /// a matching row sits on a page it never loaded.
    pub complete: bool,
}

/// Cuts `text` to [`MAX_TRANSCRIPT_ROW_TEXT_BYTES`] on a character boundary.
///
/// Returns whether the bound cut it. The flag is the contract: a client renders
/// a truncated body differently, and a silent cut would let a reviewer read
/// half an answer as the whole one.
pub fn bound_transcript_row_text(text: &str) -> (String, bool) {
    let limit = MAX_TRANSCRIPT_ROW_TEXT_BYTES as usize;
    if text.len() <= limit {
        return (text.to_string(), false);
    }
    let mut cut = limit;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    (text[..cut].to_string(), true)
}

/// Cuts one page out of `rows`, newest-end first.
///
/// `rows` is every row Core can order, in any order: the whole set is scoped
/// and re-sorted here for every page, which is what makes two adjacent pages
/// tile the same transcript even though the durable sources behind them are
/// appended to independently.
pub fn transcript_rows_page(
    rows: &[OwnedTranscriptRow],
    query: &TranscriptRowsQuery,
) -> TranscriptRowsPage {
    let mut matching = rows
        .iter()
        .filter(|row| query.matches(row))
        .cloned()
        .collect::<Vec<_>>();
    matching.sort_by_key(TranscriptRowsCursor::of);

    if let Some(before) = query
        .before
        .as_deref()
        // `validate` already refused an undecodable cursor on the dispatch
        // path; the filter is kept so a direct caller cannot silently restart
        // the walk at the newest end.
        .and_then(|raw| TranscriptRowsCursor::decode(raw).ok())
    {
        // Exclusive: the cursor names the oldest row already delivered, so this
        // page ends strictly older than it.
        matching.retain(|row| TranscriptRowsCursor::of(row) < before);
    }

    let limit = query.clamped_limit();
    let complete = matching.len() <= limit;
    if !complete {
        // Keep the newest `limit` rows: a transcript is read at its newest end.
        matching.drain(0..matching.len() - limit);
    }
    let older = if complete {
        None
    } else {
        matching
            .first()
            .map(|row| TranscriptRowsCursor::of(row).encode())
    };
    TranscriptRowsPage {
        rows: matching,
        older,
        complete,
    }
}
