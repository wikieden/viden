//! Paged evidence archive reads (`runtime.evidence_reads`, GUI-CORE-025).
//!
//! [`crate::RuntimeViewState::latest_evidence`] is a *recent-window
//! projection*: it is an upsert-by-id list the reducer keeps beside the live
//! session, it holds only what the current stream published, and it carries no
//! ordering, no cursor, and no content. The registered `EvidenceView` surface
//! needs a day-grouped archive it can page through and the bytes behind a row,
//! and neither can be derived from that projection without a client inventing
//! the parts Core never sent. This module is the read contract that ends that.
//!
//! Four invariants make the answers honest:
//!
//! 1. **A total, deterministic order.** Entries sort ascending on
//!    `(timestamp, id)` — see [`EvidenceCursor`]. `timestamp` is
//!    [`crate::EvidenceView::timestamp`], and an entry Core never dated sorts
//!    **first**, before every dated row: an undated row is the oldest thing
//!    Core can honestly say about it, and putting it first means a client
//!    paging forward meets it once, at the start, instead of watching it
//!    appear after rows it has already rendered as newer.
//! 2. **The cursor is opaque.** [`EvidencePage::next_after`] is a string a
//!    client passes back verbatim as [`EvidenceQuery::after`]. It encodes the
//!    ordering position above, and clients must not parse, construct, or
//!    compare it — a client that reconstructed one would be re-deriving Core's
//!    ordering rule from a string, which is the coupling the opaque form
//!    exists to prevent.
//! 3. **Scoping and filtering fail closed.** [`EvidenceQuery::owner`] is a
//!    prefix scope match over [`crate::RuntimeOwner`], the same narrowing the
//!    audit timeline applies to its project and lane filters. Evidence whose
//!    own owner is `None` — evidence Core could not attribute — never
//!    satisfies a scoped read, because answering one with it would turn "Core
//!    did not know" into "this lane produced it". Empty `kinds` means every
//!    kind, never nothing.
//! 4. **Content is only ever canonical bytes.** [`EvidenceContent`] is either
//!    verified canonical ContextStore bytes or a typed
//!    [`EvidenceUnavailableReason`]. There is no third case where Core serves
//!    something it could not verify, because a client rendering unverified
//!    bytes under an evidence row would be showing a merge reviewer text
//!    nobody accepted.
//!
//! Both enums are `#[non_exhaustive]`, and both read answers are query results
//! rather than runtime facts: the reducer stores neither, so publishing a page
//! moves no snapshot digest.

use serde::{Deserialize, Serialize};

use crate::{DiffDocument, EvidenceView, RuntimeOwner};

/// Largest page one [`EvidenceQuery`] may return.
pub const MAX_EVIDENCE_PAGE_SIZE: u16 = 200;

/// Page size a client gets when it expresses no preference.
pub const DEFAULT_EVIDENCE_PAGE_SIZE: u16 = 50;

/// Byte bound on the content one [`EvidenceContent`] publishes.
///
/// 256 KiB, the same bound the diff reads and conflict content use. Evidence
/// bytes ride an event every connected client receives, so an unbounded read
/// would put an arbitrary blob on the stream because an operator expanded one
/// row.
pub const MAX_EVIDENCE_CONTENT_BYTES: u32 = 256 * 1024;

/// Read-only query over the durable evidence archive. Every filter is an AND.
///
/// Filters are applied by Core *before* the page is cut, so
/// [`EvidencePage::complete`] and [`EvidencePage::next_after`] describe the
/// filtered archive. This is why the filters are a contract addition rather
/// than client work: a client filtering a page it already holds cannot know
/// whether a matching row sits on a page it never loaded, so its "no `patch`
/// evidence for this lane" would be a claim about completeness it has no
/// evidence for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceQuery {
    /// Prefix scope match on the owner: every field the query sets must equal
    /// the record's, and the fields it leaves unset match anything. An unset
    /// query owner (`None`) reads the whole archive.
    #[serde(default)]
    pub owner: Option<RuntimeOwner>,
    /// Exact [`crate::EvidenceView::kind`] matches. Empty means every kind.
    #[serde(default)]
    pub kinds: Vec<String>,
    /// Clamped to `1..=MAX_EVIDENCE_PAGE_SIZE` at query time rather than
    /// rejected, so a malformed client request still gets a well-formed page.
    /// An unclamped `0` would answer every read with an empty page, which a
    /// client cannot tell apart from an empty archive.
    pub limit: u16,
    /// Exclusive lower bound: the opaque cursor of the last entry already
    /// delivered. Paging walks oldest to newest.
    #[serde(default)]
    pub after: Option<String>,
}

impl Default for EvidenceQuery {
    fn default() -> Self {
        Self {
            owner: None,
            kinds: Vec::new(),
            limit: DEFAULT_EVIDENCE_PAGE_SIZE,
            after: None,
        }
    }
}

impl EvidenceQuery {
    pub fn clamped_limit(&self) -> usize {
        self.limit.clamp(1, MAX_EVIDENCE_PAGE_SIZE) as usize
    }

    /// Rejects a query that cannot mean what it says.
    ///
    /// Unlike `limit`, an undecodable cursor is not ignored: silently reading
    /// from the start would re-deliver a page the client already rendered and
    /// look like duplicated evidence, and silently reading to the end would
    /// look like an exhausted archive. Both are worse than a stated refusal.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(after) = self.after.as_deref() {
            EvidenceCursor::decode(after)?;
        }
        for kind in &self.kinds {
            if kind.trim().is_empty() {
                return Err("evidence query kinds cannot contain an empty kind".to_string());
            }
        }
        Ok(())
    }

    /// Whether this query keeps `entry`, before ordering and paging.
    pub fn matches(&self, entry: &EvidenceView) -> bool {
        if let Some(scope) = self.owner.as_ref() {
            // Fail closed: evidence with no recorded owner is not evidence
            // Core can place inside a scope, so a scoped read never sees it.
            let Some(owner) = entry.owner.as_ref() else {
                return false;
            };
            if !owner_scope_matches(scope, owner) {
                return false;
            }
        }
        if !self.kinds.is_empty() && !self.kinds.contains(&entry.kind) {
            return false;
        }
        true
    }
}

/// Whether `owner` sits inside the scope `scope` names.
///
/// The same narrowing the audit filters apply, generalized over every owner
/// field: a set field must match exactly, an unset one matches anything.
/// `workspace_id` and `project_id` are plain strings on [`RuntimeOwner`], so
/// empty is their "unset", exactly as the audit record validator treats them.
///
/// The asymmetry is deliberate: a query naming a task cannot be satisfied by a
/// record that only knows its lane. Core did not record that the row belonged
/// to that task, and a read that returned it anyway would let a client label
/// evidence with a task it was never attributed to.
fn owner_scope_matches(scope: &RuntimeOwner, owner: &RuntimeOwner) -> bool {
    if !scope.workspace_id.is_empty() && scope.workspace_id != owner.workspace_id {
        return false;
    }
    if !scope.project_id.is_empty() && scope.project_id != owner.project_id {
        return false;
    }
    for (wanted, actual) in [
        (scope.lane_id.as_deref(), owner.lane_id.as_deref()),
        (scope.session_id.as_deref(), owner.session_id.as_deref()),
        (scope.task_id.as_deref(), owner.task_id.as_deref()),
        (scope.turn_id.as_deref(), owner.turn_id.as_deref()),
    ] {
        if let Some(wanted) = wanted.filter(|wanted| !wanted.is_empty())
            && actual != Some(wanted)
        {
            return false;
        }
    }
    true
}

/// Stable oldest-first ordering position of one evidence row.
///
/// `Ord` is derived, and the field order carries the rule: `Option<u64>`
/// orders `None` before every `Some`, so an undated row sorts first, and `id`
/// breaks a timestamp tie so two rows recorded in the same second still page
/// deterministically.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EvidenceCursor {
    /// [`crate::EvidenceView::timestamp`]. `None` is a real position — the
    /// row Core never dated — and is *not* the same position as `Some(0)`.
    pub timestamp: Option<u64>,
    pub id: String,
}

impl EvidenceCursor {
    pub fn of(entry: &EvidenceView) -> Self {
        Self {
            timestamp: entry.timestamp,
            id: entry.id.clone(),
        }
    }

    /// The opaque wire form clients pass back verbatim.
    ///
    /// Tagged rather than positional so the undated position stays distinct
    /// from `timestamp = 0`; the id keeps the rest of the string, so an id
    /// containing the separator still round-trips.
    pub fn encode(&self) -> String {
        match self.timestamp {
            Some(timestamp) => format!("t:{timestamp}:{}", self.id),
            None => format!("u:{}", self.id),
        }
    }

    pub fn decode(raw: &str) -> Result<Self, String> {
        let invalid = || format!("evidence cursor `{raw}` is not a cursor this build issued");
        if let Some(id) = raw.strip_prefix("u:") {
            if id.is_empty() {
                return Err(invalid());
            }
            return Ok(Self {
                timestamp: None,
                id: id.to_string(),
            });
        }
        let rest = raw.strip_prefix("t:").ok_or_else(invalid)?;
        let (timestamp, id) = rest.split_once(':').ok_or_else(invalid)?;
        if id.is_empty() {
            return Err(invalid());
        }
        Ok(Self {
            timestamp: Some(timestamp.parse::<u64>().map_err(|_| invalid())?),
            id: id.to_string(),
        })
    }
}

/// One oldest-first page of the durable evidence archive.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EvidencePage {
    /// Oldest first, on `(timestamp, id)`.
    #[serde(default)]
    pub entries: Vec<EvidenceView>,
    /// True when no newer entry matches the query.
    pub complete: bool,
    /// Opaque cursor to pass as the next query's `after`. `None` when
    /// complete. Clients pass it back verbatim and never parse it.
    #[serde(default)]
    pub next_after: Option<String>,
}

/// The bytes behind one evidence row, or the typed reason there are none.
///
/// `#[non_exhaustive]` so a later content shape cannot break a client match.
/// Every variant that carries content also carries the `sha256` Core verified
/// the bytes against, so a reader can join the rendered content to the
/// canonical reference the evidence row already published.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum EvidenceContent {
    /// UTF-8 canonical bytes, bounded by [`MAX_EVIDENCE_CONTENT_BYTES`].
    /// `truncated` says the bound cut them, never that the evidence was short.
    Text {
        text: String,
        truncated: bool,
        sha256: String,
    },
    /// Canonical bytes of kind `patch`, parsed by the same unified-diff
    /// producer the structured diff capability uses, so one evidence patch and
    /// one workspace diff render through identical rows.
    Diff {
        document: DiffDocument,
        sha256: String,
    },
    /// Core has no canonical bytes it may serve. The reason is typed because a
    /// client's affordance differs per case, and because "no content" and "the
    /// content no longer matches its hash" are opposite facts.
    Unavailable { reason: EvidenceUnavailableReason },
}

/// Why an evidence row has no servable canonical content.
///
/// `#[non_exhaustive]`. None of these is an error: each is a fact about the
/// archive that a client renders rather than retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EvidenceUnavailableReason {
    /// The row carries no canonical reference at all: display-only evidence
    /// such as `task_summary`, which is model prose and never merge evidence.
    SummaryOnly,
    /// The row names canonical bytes the ContextStore no longer holds.
    MissingCanonicalBytes,
    /// The stored bytes no longer hash to the reference the row published.
    /// They are never served: content that fails its own hash is exactly the
    /// content a reviewer must not be shown as canonical.
    HashMismatch,
    /// The canonical bytes are not UTF-8. Core has them and they verify; there
    /// is simply no text or diff shape to publish.
    Binary,
}
