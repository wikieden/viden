//! Runtime behavior of the evidence archive reads (`runtime.evidence_reads`,
//! GUI-CORE-025).
//!
//! Four things must hold for the `EvidenceView` surface to be usable by a
//! frontend that is forbidden from reading the ContextStore itself:
//!
//! 1. the page comes from the durable archive Core rebuilt at open, ordered on
//!    `(timestamp, id)` ascending, with the cursor tiling adjacent pages;
//! 2. filters run before the page is cut, so `complete` describes the filtered
//!    archive rather than the raw one;
//! 3. content is served only from canonical bytes that verified against the
//!    row's own `source_hash`, and every other outcome is a typed reason;
//! 4. a refusal is a `CommandRejected` naming this exact read, never an empty
//!    page and never a bare `Error`.

use std::fs;

use viden_context::{ContextEngine, ContextPutRequest};
use viden_types::{
    AgentDagTaskSpec, AgentRole, ApprovalResponse, CanonicalEvidenceReference, ContextContentKind,
    ContextScope, EvidenceContent, EvidencePage, EvidenceProducer, EvidenceQualityFacts,
    EvidenceQualityStatus, EvidenceQuery, EvidenceUnavailableReason, EvidenceVerificationState,
    RuntimeCommand, RuntimeEvent, RuntimeEventKind, WorkMode,
};

use super::{SequenceProvider, temp_dir};
use crate::SessionEngine;

fn engine_with_gate(name: &str) -> (std::path::PathBuf, SessionEngine) {
    let cwd = temp_dir(&format!("{name}_cwd"));
    let home = temp_dir(&format!("{name}_home"));
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    engine
        .handle_runtime_command(
            "start-evidence-dag",
            RuntimeCommand::StartAgentDag {
                goal: "evidence archive reads".to_string(),
                tasks: vec![AgentDagTaskSpec {
                    task_id: "task-evidence".to_string(),
                    role: AgentRole::Coder,
                    title: "Evidence reads".to_string(),
                    objective: "record evidence to page".to_string(),
                    dependencies: Vec::new(),
                    workspace: None,
                    file_scope: vec!["src".to_string()],
                    context_bundle_id: None,
                    required_evidence: vec!["patch".to_string()],
                    permission_policy: "scoped_mutation".to_string(),
                }],
            },
            &mut allow,
        )
        .unwrap();
    (cwd, engine)
}

/// Records one evidence row. `bytes` present means canonical ContextStore
/// bytes; `None` means display-only evidence, the summary case.
fn record_evidence(
    cwd: &std::path::Path,
    engine: &mut SessionEngine,
    evidence_id: &str,
    kind: &str,
    bytes: Option<&[u8]>,
) {
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    let canonical = bytes.map(|bytes| {
        let mut store = ContextEngine::open(cwd.join(".viden/context-engine")).unwrap();
        let stored = store
            .store(ContextPutRequest {
                scope: ContextScope::Task("task-evidence".to_string()),
                kind: ContextContentKind::Diff,
                content: bytes,
                evidence_id: Some(evidence_id.to_string()),
            })
            .unwrap();
        let canonical = CanonicalEvidenceReference {
            item_id: stored.item.item_id.clone(),
            bundle_id: format!("bundle-{evidence_id}"),
            source_hash: stored.item.content_sha256.clone(),
            producer: EvidenceProducer {
                identity: "lane-origin".to_string(),
                role: "coder".to_string(),
                task_id: "task-evidence".to_string(),
            },
            permission_snapshot_id: Some(format!("permission-{evidence_id}")),
            permission_scope: ContextScope::Task("task-evidence".to_string()),
            evidence_scope: ContextScope::Task("task-evidence".to_string()),
            verification: EvidenceVerificationState::Verified,
            quality: EvidenceQualityFacts {
                status: EvidenceQualityStatus::Pass,
                reason_codes: Vec::new(),
            },
        };
        engine.set_merge_gate_context_facts_for_test(&format!("bundle-{evidence_id}"), stored.item);
        canonical
    });
    engine
        .handle_runtime_command(
            format!("record-{evidence_id}"),
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task-evidence".to_string(),
                evidence_id: Some(evidence_id.to_string()),
                kind: kind.to_string(),
                summary: format!("{kind} evidence"),
                path: None,
                source: Some("lane-origin".to_string()),
                canonical,
            },
            &mut allow,
        )
        .unwrap();
}

fn query(engine: &mut SessionEngine, command_id: &str, query: EvidenceQuery) -> Vec<RuntimeEvent> {
    let mut denier = |_prompt| panic!("a read-only evidence query must not request approval");
    engine
        .handle_runtime_command(
            command_id,
            RuntimeCommand::QueryEvidence { query },
            &mut denier,
        )
        .unwrap()
}

fn page(events: &[RuntimeEvent], expected_command_id: &str) -> EvidencePage {
    let loaded = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::EvidencePageLoaded { command_id, page } => {
                Some((command_id.clone(), page.clone()))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected an EvidencePageLoaded, got {events:?}"));
    assert_eq!(
        loaded.0, expected_command_id,
        "the page must name the exact read it answers"
    );
    loaded.1
}

fn read_content(
    engine: &mut SessionEngine,
    command_id: &str,
    evidence_id: &str,
) -> EvidenceContent {
    let mut denier = |_prompt| panic!("a read-only content read must not request approval");
    let events = engine
        .handle_runtime_command(
            command_id,
            RuntimeCommand::ReadEvidenceContent {
                evidence_id: evidence_id.to_string(),
            },
            &mut denier,
        )
        .unwrap();
    events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::EvidenceContentLoaded {
                command_id: answered,
                evidence_id: answered_id,
                content,
            } => {
                assert_eq!(answered, command_id, "the answer names the exact read");
                assert_eq!(answered_id, evidence_id, "the answer names the exact row");
                Some(content.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected an EvidenceContentLoaded, got {events:?}"))
}

/// The archive is paged oldest-first on `(timestamp, id)` and the cursor tiles
/// two adjacent pages: every row appears exactly once across them.
#[test]
fn evidence_pages_tile_the_archive_oldest_first_without_repeating_a_row() {
    let (cwd, mut engine) = engine_with_gate("evidence_paging");
    for (id, kind) in [
        ("evidence-alpha", "patch"),
        ("evidence-bravo", "test_result"),
        ("evidence-charlie", "review"),
    ] {
        record_evidence(&cwd, &mut engine, id, kind, None);
    }

    let first = page(
        &query(
            &mut engine,
            "evidence-read-1",
            EvidenceQuery {
                limit: 2,
                ..EvidenceQuery::default()
            },
        ),
        "evidence-read-1",
    );
    assert_eq!(first.entries.len(), 2);
    assert!(!first.complete, "a cut page is not complete");
    let cursor = first
        .next_after
        .clone()
        .expect("an incomplete page names where to resume");

    let second = page(
        &query(
            &mut engine,
            "evidence-read-2",
            EvidenceQuery {
                limit: 2,
                after: Some(cursor),
                ..EvidenceQuery::default()
            },
        ),
        "evidence-read-2",
    );
    assert!(second.complete, "the archive is exhausted");
    assert_eq!(second.next_after, None);

    let mut seen = first
        .entries
        .iter()
        .chain(second.entries.iter())
        .map(|entry| entry.id.clone())
        .collect::<Vec<_>>();
    let unique = seen.clone();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), unique.len(), "no row appears on both pages");
    assert_eq!(seen.len(), 3, "every recorded row appears exactly once");

    // Ordering is ascending on the recorded timestamp, then the id.
    let ordered = unique
        .iter()
        .map(|id| {
            first
                .entries
                .iter()
                .chain(second.entries.iter())
                .find(|entry| entry.id == *id)
                .map(|entry| (entry.timestamp, entry.id.clone()))
                .unwrap()
        })
        .collect::<Vec<_>>();
    let mut sorted = ordered.clone();
    sorted.sort();
    assert_eq!(ordered, sorted, "entries are ascending on (timestamp, id)");
}

/// The kind filter runs before the page is cut, so `complete` describes the
/// filtered archive and not the archive the unfiltered read just published.
#[test]
fn a_kind_filtered_page_is_complete_for_the_filter_not_for_the_archive() {
    let (cwd, mut engine) = engine_with_gate("evidence_kind_filter");
    for (id, kind) in [
        ("evidence-alpha", "patch"),
        ("evidence-bravo", "test_result"),
        ("evidence-charlie", "test_result"),
    ] {
        record_evidence(&cwd, &mut engine, id, kind, None);
    }

    let filtered = page(
        &query(
            &mut engine,
            "evidence-read-patch",
            EvidenceQuery {
                kinds: vec!["patch".to_string()],
                limit: 2,
                ..EvidenceQuery::default()
            },
        ),
        "evidence-read-patch",
    );
    assert_eq!(filtered.entries.len(), 1);
    assert!(
        filtered.entries.iter().all(|entry| entry.kind == "patch"),
        "a filtered page contains only rows the filter kept"
    );
    assert!(
        filtered.complete,
        "complete describes the filtered archive, even though two unfiltered rows remain"
    );

    let unfiltered = page(
        &query(
            &mut engine,
            "evidence-read-all",
            EvidenceQuery {
                limit: 2,
                ..EvidenceQuery::default()
            },
        ),
        "evidence-read-all",
    );
    assert!(
        !unfiltered.complete,
        "the unfiltered archive still has a page left, which is the point"
    );
}

/// An over-limit filter list and a cursor this build never issued are both
/// refused as `CommandRejected` naming this exact read. Neither may become an
/// empty page: a client cannot tell a fabricated empty archive from a real one.
#[test]
fn a_malformed_evidence_query_is_rejected_by_command_id_and_never_answered_with_a_page() {
    let (cwd, mut engine) = engine_with_gate("evidence_rejection");
    record_evidence(&cwd, &mut engine, "evidence-alpha", "patch", None);

    for (command_id, malformed) in [
        (
            "evidence-read-overlimit",
            EvidenceQuery {
                kinds: (0..64).map(|index| format!("kind_{index}")).collect(),
                ..EvidenceQuery::default()
            },
        ),
        (
            "evidence-read-badcursor",
            EvidenceQuery {
                after: Some("not-a-cursor".to_string()),
                ..EvidenceQuery::default()
            },
        ),
    ] {
        let events = query(&mut engine, command_id, malformed);
        assert!(
            events
                .iter()
                .all(|event| !matches!(event.kind, RuntimeEventKind::EvidencePageLoaded { .. })),
            "a refused read must not publish a page"
        );
        assert!(
            events.iter().any(|event| matches!(
                &event.kind,
                RuntimeEventKind::CommandRejected { command_id: rejected, .. }
                    if rejected == command_id
            )),
            "the refusal names this exact read, got {events:?}"
        );
    }
}

/// A `limit` beyond the supported page size is clamped rather than refused, as
/// the audit query does: a client that asked for too many rows still means
/// something answerable, and an unclamped `0` would answer every read with an
/// empty page.
#[test]
fn an_out_of_range_evidence_limit_is_clamped_into_a_well_formed_page() {
    let (cwd, mut engine) = engine_with_gate("evidence_limit_clamp");
    record_evidence(&cwd, &mut engine, "evidence-alpha", "patch", None);

    for (command_id, limit) in [
        ("evidence-read-zero", 0_u16),
        ("evidence-read-huge", u16::MAX),
    ] {
        let answered = page(
            &query(
                &mut engine,
                command_id,
                EvidenceQuery {
                    limit,
                    ..EvidenceQuery::default()
                },
            ),
            command_id,
        );
        assert_eq!(answered.entries.len(), 1);
        assert!(answered.complete);
    }
}

/// A read-only archive page is answerable in Plan mode: it mutates nothing,
/// spawns nothing, and prompts for nothing.
#[test]
fn evidence_reads_stay_answerable_in_plan_mode() {
    let (cwd, mut engine) = engine_with_gate("evidence_plan_mode");
    record_evidence(&cwd, &mut engine, "evidence-alpha", "patch", None);
    let mut denier = |_prompt| panic!("plan mode must not prompt for a read");
    engine
        .handle_runtime_command(
            "set-plan",
            RuntimeCommand::SetWorkMode {
                mode: WorkMode::Plan,
            },
            &mut denier,
        )
        .unwrap();

    let answered = page(
        &query(&mut engine, "evidence-read-plan", EvidenceQuery::default()),
        "evidence-read-plan",
    );
    assert_eq!(answered.entries.len(), 1);
    assert!(matches!(
        read_content(&mut engine, "evidence-content-plan", "evidence-alpha"),
        EvidenceContent::Unavailable {
            reason: EvidenceUnavailableReason::SummaryOnly
        }
    ));
}

/// Content resolution, one row per outcome. The two that matter most are the
/// last: bytes whose hash no longer matches are never served, and an evidence
/// id that was never recorded is a refusal rather than an "unavailable" answer
/// for a row that does not exist.
#[test]
fn evidence_content_is_served_only_from_canonical_bytes_that_verify() {
    let (cwd, mut engine) = engine_with_gate("evidence_content");
    let patch = b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
    record_evidence(&cwd, &mut engine, "evidence-patch", "patch", Some(patch));
    record_evidence(
        &cwd,
        &mut engine,
        "evidence-log",
        "test_result",
        Some(b"running 3 tests\ntest result: ok. 3 passed\n"),
    );
    record_evidence(&cwd, &mut engine, "evidence-summary", "review", None);

    // `patch` kind resolves through the same parser the structured diff
    // capability uses, so an evidence patch renders as diff rows.
    let EvidenceContent::Diff { document, sha256 } =
        read_content(&mut engine, "content-patch", "evidence-patch")
    else {
        panic!("patch evidence must answer with a parsed diff document");
    };
    assert_eq!(document.files.len(), 1);
    assert_eq!(document.files[0].path, "src/lib.rs");
    assert_eq!(document.files[0].hunks.len(), 1);
    assert!(!document.truncated);
    assert_eq!(sha256.len(), 64, "the answer names the verified hash");

    // Every other kind resolves as bounded text.
    let EvidenceContent::Text {
        text, truncated, ..
    } = read_content(&mut engine, "content-log", "evidence-log")
    else {
        panic!("non-patch canonical evidence must answer with text");
    };
    assert!(text.contains("3 passed"));
    assert!(!truncated, "content under the bound is not truncated");

    // Display-only evidence has no canonical reference at all.
    assert!(matches!(
        read_content(&mut engine, "content-summary", "evidence-summary"),
        EvidenceContent::Unavailable {
            reason: EvidenceUnavailableReason::SummaryOnly
        }
    ));

    // Bytes whose hash no longer matches are never served as canonical.
    let source_hash = engine
        .runtime_view_state()
        .latest_evidence
        .iter()
        .find(|entry| entry.id == "evidence-patch")
        .and_then(|entry| entry.canonical.as_ref())
        .map(|canonical| canonical.source_hash.clone())
        .expect("the recorded patch carries a canonical reference");
    let blob = cwd
        .join(".viden/context-engine/blobs")
        .join(&source_hash[..2])
        .join(&source_hash);
    assert_eq!(
        fs::read(&blob).unwrap(),
        patch,
        "the canonical blob holds the reviewed bytes"
    );
    fs::write(&blob, b"tampered bytes that are not the reviewed patch\n").unwrap();
    assert!(
        matches!(
            read_content(&mut engine, "content-tampered", "evidence-patch"),
            EvidenceContent::Unavailable {
                reason: EvidenceUnavailableReason::HashMismatch
            }
        ),
        "content that fails its own hash must never be served as canonical"
    );

    // A reference whose bytes are gone is a different fact from a mismatch.
    fs::remove_file(&blob).unwrap();
    assert!(matches!(
        read_content(&mut engine, "content-missing", "evidence-patch"),
        EvidenceContent::Unavailable {
            reason: EvidenceUnavailableReason::MissingCanonicalBytes
        }
    ));

    // An id this build never recorded is a refusal, not an unavailable answer
    // for a row that does not exist.
    let mut denier = |_prompt| panic!("a read-only content read must not request approval");
    let events = engine
        .handle_runtime_command(
            "content-unknown",
            RuntimeCommand::ReadEvidenceContent {
                evidence_id: "evidence-never-recorded".to_string(),
            },
            &mut denier,
        )
        .unwrap();
    assert!(
        events
            .iter()
            .all(|event| !matches!(event.kind, RuntimeEventKind::EvidenceContentLoaded { .. }))
    );
    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { command_id, .. } if command_id == "content-unknown"
    )));
}

/// Non-UTF-8 canonical bytes verify but have no text or diff shape to publish,
/// which is its own fact rather than a missing or mismatched one.
#[test]
fn binary_canonical_evidence_answers_binary_rather_than_missing() {
    let (cwd, mut engine) = engine_with_gate("evidence_binary");
    record_evidence(
        &cwd,
        &mut engine,
        "evidence-binary",
        "release_artifact",
        Some(&[0x00, 0xff, 0xfe, 0x80, 0x01]),
    );
    assert!(matches!(
        read_content(&mut engine, "content-binary", "evidence-binary"),
        EvidenceContent::Unavailable {
            reason: EvidenceUnavailableReason::Binary
        }
    ));
}

/// An archive page is a query answer, not runtime view state: publishing one
/// must leave `latest_evidence` exactly as the live stream left it.
#[test]
fn an_archive_page_never_rewrites_the_recent_evidence_window() {
    let (cwd, mut engine) = engine_with_gate("evidence_view_state");
    record_evidence(&cwd, &mut engine, "evidence-alpha", "patch", None);
    let before = engine.runtime_view_state().latest_evidence.clone();

    query(&mut engine, "evidence-read-1", EvidenceQuery::default());
    read_content(&mut engine, "content-1", "evidence-alpha");

    assert_eq!(
        engine.runtime_view_state().latest_evidence,
        before,
        "a query answer must not reach the view state"
    );
}
