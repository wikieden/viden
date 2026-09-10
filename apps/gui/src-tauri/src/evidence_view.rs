//! EvidenceView read side, projected from Core's evidence archive
//! (`runtime.evidence_reads`, GUI-CORE-025).
//!
//! Two Core reads carry this whole surface and the client adds nothing to
//! either. `QueryEvidence` -> `EvidencePageLoaded` pages the durable archive
//! Core rebuilds from the append-only workflow agent log, and
//! `ReadEvidenceContent` -> `EvidenceContentLoaded` answers the bytes behind
//! one row. Neither answer is reduced into `RuntimeViewState`, so publishing
//! one moves no snapshot digest.
//!
//! These are the renderable shapes only. The Core-typed projections that fill
//! them live beside the rest of the GUI's Core reads in `projection.rs`,
//! because the architecture boundary test keeps every Core contract type
//! inside the adapter/projection boundary.
//!
//! The rules this surface exists to keep, each of which a naive client would
//! break by accident:
//!
//! - **`latest_evidence` is a different projection.** It is a recent-window
//!   upsert list with no ordering rule, no cursor, and no content. Nothing
//!   here reads it, and an archive row never merges with one: a client that
//!   mixed them would be presenting a window it happened to receive as the
//!   archive Core actually holds.
//! - **The cursor is opaque.** [`EvidenceArchiveProjection::next_after`] is
//!   Core's own string, carried verbatim and handed back verbatim as the next
//!   query's `after`. It is never parsed, constructed, or compared here.
//! - **Core owns the order.** Rows arrive ascending on `(timestamp, id)` with
//!   an undated row first, and this module appends pages in arrival order
//!   without re-sorting. Day grouping is presentation over that order, never a
//!   second definition of it.
//! - **Content is verified canonical bytes or a typed reason.**
//!   [`EvidenceContentProjection`] mirrors `EvidenceContent` exactly: text,
//!   parsed diff rows, or one of the four unavailable reasons. There is no
//!   shape here for bytes Core could not verify, because Core publishes none.
//! - **`metadata` is rendered, never interpreted.** Core publishes free-form
//!   JSON; the projection flattens the top level into key/value facts and
//!   serializes each value exactly as Core sent it. Nothing infers a status, a
//!   count, or a link out of a metadata key.

use serde::Serialize;

use crate::D1OutcomeProjection;
use crate::diff_review::DiffDocumentProjection;

/// The frontend-contract-v1 capability that carries the evidence archive.
///
/// The exact id Core publishes in its handshake; the client must not invent a
/// finer-grained one, because an unpublished id can never become available.
pub const EVIDENCE_READS_CAPABILITY: &str = "runtime.evidence_reads";

/// Rows this client asks Core for per page.
///
/// Core clamps `1..=200` and defaults to 50; the view asks for the default
/// rather than the maximum because the list is day-grouped and read top to
/// bottom, and "Load older" is one click.
pub const EVIDENCE_PAGE_LIMIT: u16 = 50;

/// Local refusal code for a Lane selection Core published no owner binding for.
///
/// The archive read is owner-scoped. With a Lane selected and no exact Core
/// owner, the only two things this client could do are send an *unscoped*
/// read — which would show every Lane's evidence under one Lane's name — or
/// refuse and say so. It refuses; nothing is sent.
pub const EVIDENCE_NO_OWNER_CODE: &str = "D1-EVIDENCE-NO-OWNER";

/// One key/value fact out of `EvidenceView.metadata`.
///
/// Core's metadata is free-form JSON. The key is Core's own; the value is that
/// key's JSON rendered compactly, with a plain string unquoted so a path or an
/// exit code reads as itself. Nothing here decides what a key *means*.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceMetadataProjection {
    pub key: String,
    pub value: String,
}

/// The canonical ContextStore reference an evidence row names, when it has one.
///
/// `None` is exactly the row `ReadEvidenceContent` answers with
/// `Unavailable { SummaryOnly }`: display-only evidence the merge gate already
/// refuses. The two facts agree because they come from the same place.
///
/// `verification` and `quality` are Core's own verdicts on the reference,
/// carried as Core's serde tags. They are facts about the bytes, not about the
/// row: a `failed` verification still names bytes Core holds, which is a
/// different thing from the `None` above, and nothing here infers either
/// verdict from the hash, the summary, or whether a content read succeeded.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceCanonicalProjection {
    pub item_id: String,
    pub bundle_id: String,
    /// The hash Core verifies served bytes against, so a reader can join the
    /// rendered content to the reference the row published.
    pub source_hash: String,
    pub producer_identity: String,
    pub producer_role: String,
    pub producer_task_id: String,
    /// `unverified`, `verified`, or `failed` — Core's `EvidenceVerificationState`.
    pub verification: &'static str,
    /// `pass`, `warn`, or `fail` — the status of Core's `EvidenceQualityFacts`.
    /// The reason codes behind it stay with Core; this is the verdict only.
    pub quality: &'static str,
}

/// One archive row, exactly as `EvidenceView` published it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceRowProjection {
    pub id: String,
    /// Core's own kind string. The first-class set is `patch`, `test_result`,
    /// `review`, `doc_update`, and `release_artifact`; any other kind Core
    /// returns is carried through rather than dropped or renamed.
    pub kind: String,
    /// Core's summary line. It is display text: nothing infers the row's
    /// content, outcome, or verification from it.
    pub summary: String,
    pub path: Option<String>,
    pub source: Option<String>,
    /// Seconds. `None` is a real position in Core's order — the row Core never
    /// dated, which sorts *first* — and is not the same as `Some(0)`.
    pub timestamp: Option<u64>,
    /// The owning Lane, when Core recorded an owner. `None` means Core could
    /// not attribute the row; it is never filled from `source`, which is a
    /// producer label rather than an owner.
    pub owner_lane_id: Option<String>,
    pub owner_task_id: Option<String>,
    pub canonical: Option<EvidenceCanonicalProjection>,
    /// Top-level `metadata` keys as facts, in Core's own key order.
    pub metadata: Vec<EvidenceMetadataProjection>,
}

/// What the EvidenceView list may render after one or more `QueryEvidence`
/// pages.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceArchiveProjection {
    pub outcome: D1OutcomeProjection,
    /// Every page confirmed so far, appended in Core's order. Never re-sorted.
    pub rows: Vec<EvidenceRowProjection>,
    /// Core's opaque cursor for the next page, carried verbatim. `None`
    /// exactly when `complete` is true.
    pub next_after: Option<String>,
    /// No newer entry matches the query. Because Core filters before it cuts
    /// the page, this describes the *filtered* archive.
    pub complete: bool,
    /// Whether a page has actually arrived. Absence and emptiness are
    /// different facts: "not read yet" must never render as "no evidence".
    pub loaded: bool,
    pub pending_command_id: Option<String>,
    /// False when Core's handshake published no `runtime.evidence_reads`.
    pub capability_available: bool,
    /// Core recorded new evidence after the loaded pages were read, so the
    /// list on screen is missing at least one row.
    ///
    /// A signal to re-read, never a claim about *what* was recorded, and never
    /// an auto-reload: a paged list must not shift under an operator mid-read.
    pub stale: bool,
    /// The Lane the confirmed pages are scoped to, or `None` for the whole
    /// archive. Read back from the query this client sent.
    pub scope_lane_id: Option<String>,
    /// The kind filter the confirmed pages were read under, in the order sent.
    pub kinds: Vec<String>,
}

/// The bytes behind one row, or the typed reason there are none.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceContentProjection {
    pub outcome: D1OutcomeProjection,
    /// The row this answer belongs to, echoed by Core beside the command id.
    /// `None` before any read has been confirmed.
    pub evidence_id: Option<String>,
    /// `absent` (nothing read yet), `text`, `diff`, `unavailable`, or
    /// `unknown` for a content shape this build cannot name. `unknown` is its
    /// own state rather than an empty body, because `EvidenceContent` is
    /// `#[non_exhaustive]` and a newer Core may publish a fourth shape.
    pub kind: &'static str,
    pub text: Option<String>,
    /// The byte bound cut the text. Never "the evidence was short".
    pub truncated: bool,
    /// The hash Core verified the served bytes against.
    pub sha256: Option<String>,
    /// Parsed by the same producer the structured diff capability uses, so an
    /// evidence patch and a workspace diff render through identical rows.
    pub document: Option<DiffDocumentProjection>,
    /// `summary_only`, `missing_canonical_bytes`, `hash_mismatch`, `binary`,
    /// or `unknown`. Clients switch on this rather than parsing a message.
    pub reason: Option<&'static str>,
    pub pending_command_id: Option<String>,
}

impl EvidenceContentProjection {
    /// Nothing has been read yet. Distinct from every answered state.
    pub(crate) fn absent() -> Self {
        Self {
            outcome: D1OutcomeProjection::idle(),
            evidence_id: None,
            kind: "absent",
            text: None,
            truncated: false,
            sha256: None,
            document: None,
            reason: None,
            pending_command_id: None,
        }
    }
}
