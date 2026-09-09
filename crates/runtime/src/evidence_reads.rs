//! Producers for the evidence archive reads (`runtime.evidence_reads`,
//! GUI-CORE-025).
//!
//! Two answers live here, and the honesty rule differs for each.
//!
//! **The page** is cut from the durable archive, not from the recent window.
//! The archive is `SessionEngine::runtime_evidence`, which
//! `hydrate_workflow_agent_projection` rebuilds at open from the append-only
//! `runtime_projection` / `runtime_projection_batch` rows in the workflow
//! agent log — the same durable facts `agent_evidence_recorded` writes beside.
//! `RuntimeViewState::latest_evidence` is a *client-side reduction of whatever
//! stream that client received*: it carries no ordering rule, no cursor, and
//! no content, and a client that connected mid-session holds only its own
//! window of it. Paging that would answer "what evidence exists" with "what
//! you happened to see".
//!
//! **The content** is read only from canonical ContextStore bytes and only
//! after they verify against the `source_hash` the evidence row itself
//! published. Every other outcome is a typed
//! [`viden_types::EvidenceUnavailableReason`], never bytes. This is the rule
//! the merge gate already enforces for evidence it accepts, applied to the
//! read path: content that fails its own hash is exactly the content a
//! reviewer must not be shown as canonical.

use std::path::Path;

use viden_context::{ContextEngine, ContextError, store::ContextError as ContextStoreError};
use viden_tools::patch::parse_diff_document;
use viden_types::{
    EvidenceContent, EvidenceCursor, EvidencePage, EvidenceQuery, EvidenceUnavailableReason,
    EvidenceView, MAX_EVIDENCE_CONTENT_BYTES,
};

/// Evidence kind whose canonical bytes are a unified diff.
///
/// Named here rather than inlined because it is the same string the merge gate
/// reduces required evidence on, and the same one the design's "Open in
/// review" affordance keys off.
const PATCH_EVIDENCE_KIND: &str = "patch";

/// Filters, orders, then cuts one oldest-first page of the archive.
///
/// The order of those three steps is the contract: filtering happens *before*
/// the page is cut, so [`EvidencePage::complete`] and
/// [`EvidencePage::next_after`] describe the filtered archive rather than the
/// raw one. A client filtering a page it already holds cannot know whether a
/// matching row sits on a page it never loaded, so its "no `patch` evidence
/// for this lane" would be a completeness claim it has no evidence for.
///
/// The whole archive is re-sorted for every page. That is what makes two
/// adjacent pages tile the same archive: `runtime_evidence` is insertion
/// ordered by first-seen upsert, which is arrival order and not
/// `(timestamp, id)`, so paging it in place would drift the moment a row was
/// re-recorded.
pub(crate) fn evidence_page(entries: &[EvidenceView], query: &EvidenceQuery) -> EvidencePage {
    let mut matching = entries
        .iter()
        .filter(|entry| query.matches(entry))
        .cloned()
        .collect::<Vec<_>>();
    matching.sort_by_key(EvidenceCursor::of);

    if let Some(after) = query.after.as_deref().and_then(|raw| {
        // `EvidenceQuery::validate` already rejected an undecodable cursor, so
        // reaching this with `None` is not possible on the dispatch path; the
        // filter is kept so a direct caller cannot silently restart the walk.
        EvidenceCursor::decode(raw).ok()
    }) {
        // Exclusive: the cursor names the last row already delivered, so the
        // next page starts strictly newer than it.
        matching.retain(|entry| EvidenceCursor::of(entry) > after);
    }

    let limit = query.clamped_limit();
    let complete = matching.len() <= limit;
    matching.truncate(limit);
    let next_after = if complete {
        None
    } else {
        matching
            .last()
            .map(|entry| EvidenceCursor::of(entry).encode())
    };
    EvidencePage {
        entries: matching,
        complete,
        next_after,
    }
}

/// Resolves the bytes behind one evidence row, or the typed reason there are
/// none.
///
/// Never returns `Err`: "this row has no servable content" is a fact about the
/// archive that a client renders, not a failure it retries. A refusal of the
/// *command* — an evidence id this build has never recorded — is decided by
/// the caller before this runs, because "no such evidence" and "this evidence
/// has no content" are different facts and a client must be able to tell them
/// apart.
pub(crate) fn resolve_evidence_content(
    context_engine_root: &Path,
    entry: &EvidenceView,
) -> EvidenceContent {
    let Some(canonical) = entry.canonical.as_ref() else {
        // Display-only evidence: `task_summary` and anything else recorded
        // without canonical bytes. The gate already refuses to treat it as
        // merge evidence; the read path says the same thing in one word.
        return EvidenceContent::Unavailable {
            reason: EvidenceUnavailableReason::SummaryOnly,
        };
    };

    let engine = match ContextEngine::open(context_engine_root) {
        Ok(engine) => engine,
        // A store Core cannot open holds no bytes it may serve. This is the
        // same answer as a missing blob and deliberately not `HashMismatch`:
        // nothing was compared, so claiming a mismatch would invent a fact.
        Err(_) => {
            return EvidenceContent::Unavailable {
                reason: EvidenceUnavailableReason::MissingCanonicalBytes,
            };
        }
    };
    let verified = match engine.verify_item(
        &canonical.item_id,
        &canonical.source_hash,
        &canonical.evidence_scope,
    ) {
        Ok(verified) => verified,
        Err(error) => {
            return EvidenceContent::Unavailable {
                reason: unavailable_reason_for(&error),
            };
        }
    };

    let sha256 = canonical.source_hash.clone();
    let Ok(text) = String::from_utf8(verified.content) else {
        // Core has the bytes and they verify; there is simply no text or diff
        // shape to publish. That is a different fact from "missing", and a
        // client's affordance differs, so it gets its own reason.
        return EvidenceContent::Unavailable {
            reason: EvidenceUnavailableReason::Binary,
        };
    };

    if entry.kind == PATCH_EVIDENCE_KIND {
        // The same parser the structured diff capability uses, so an evidence
        // patch and a workspace diff render through identical rows and the
        // byte bound is enforced in one place.
        return EvidenceContent::Diff {
            document: parse_diff_document(&text, MAX_EVIDENCE_CONTENT_BYTES),
            sha256,
        };
    }

    let (text, truncated) = bound_evidence_text(text);
    EvidenceContent::Text {
        text,
        truncated,
        sha256,
    }
}

/// Maps a store failure onto the reason a client renders.
///
/// `HashMismatch` is kept distinct from every other failure because it is the
/// opposite fact: the bytes are *present* and are the ones this row must never
/// be shown as. Everything else collapses to `MissingCanonicalBytes`, which is
/// what the variant means — Core has no canonical bytes it may serve — rather
/// than a claim about why.
fn unavailable_reason_for(error: &ContextError) -> EvidenceUnavailableReason {
    match error {
        ContextError::Store(ContextStoreError::HashMismatch { .. }) => {
            EvidenceUnavailableReason::HashMismatch
        }
        _ => EvidenceUnavailableReason::MissingCanonicalBytes,
    }
}

/// Bounds text to [`MAX_EVIDENCE_CONTENT_BYTES`] on a char boundary.
///
/// `truncated` says the bound cut the content, never that the evidence was
/// short: a client renders the two differently, and a silent cut would let a
/// reviewer read a partial test log as the whole one.
fn bound_evidence_text(text: String) -> (String, bool) {
    let limit = MAX_EVIDENCE_CONTENT_BYTES as usize;
    if text.len() <= limit {
        return (text, false);
    }
    let mut cut = limit;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    (text[..cut].to_string(), true)
}
