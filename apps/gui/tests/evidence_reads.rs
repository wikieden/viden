//! The EvidenceView read side, projected from Core's evidence archive
//! (`runtime.evidence_reads`, GUI-CORE-025).
//!
//! These tests cover the two correlation machines, the capability gate, the
//! owner scope, the opaque cursor, the refusal path, the four unavailable
//! reasons, and the staleness signal that drives the view's Refresh banner.
//! Both `EvidencePageLoaded` and `EvidenceContentLoaded` carry a *required*
//! `command_id`, so a read either names itself or belongs to another one;
//! there is no acceptance-first fallback here.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use viden_core::{
    DiffDocument, DiffFile, DiffHunk, DiffLine, DiffLineKind, EventCursor, EvidenceContent,
    EvidencePage, EvidenceQualityStatus, EvidenceUnavailableReason, EvidenceVerificationState,
    EvidenceView, FRONTEND_SCHEMA_V1, LaneRuntimeOwnerBinding, RuntimeCommand,
    RuntimeCommandEnvelope, RuntimeErrorView, RuntimeEvent, RuntimeEventEnvelope, RuntimeEventKind,
    RuntimeOwner, RuntimeSnapshot, RuntimeViewState, RuntimeWireEvent, WorkspaceChangeKind,
};
use viden_gui::{EVIDENCE_READS_CAPABILITY, GuiCoreAdapter};

mod support;
use support::TestCoreClient;

const TIMEOUT: Duration = Duration::from_millis(10);

fn view() -> RuntimeViewState {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../crates/types/tests/fixtures/frontend-contract-v1/multi-lane.json"
    ))
    .expect("fixture json");
    let snapshot: RuntimeSnapshot =
        serde_json::from_value(fixture["initial_snapshot"].clone()).expect("fixture snapshot");
    RuntimeViewState::new(snapshot)
}

/// The Lane the scoped tests bind, matching the multi-lane fixture's own id.
const LANE: &str = "lane-alpha";

/// The fixture view plus one Core-published owner binding for [`LANE`].
///
/// The binding carries a session and a task on purpose: the scope this client
/// builds must drop both, or a Lane-scoped read would silently become a
/// session-scoped one.
fn view_with_lane_owner() -> RuntimeViewState {
    let mut view = view();
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::LaneRuntimeOwnerBound {
            binding: LaneRuntimeOwnerBinding {
                lane_id: LANE.to_string(),
                owner: RuntimeOwner {
                    workspace_id: "workspace_contract_v1".to_string(),
                    project_id: "project_viden".to_string(),
                    lane_id: Some(LANE.to_string()),
                    session_id: Some("session-alpha".to_string()),
                    task_id: Some("task-alpha".to_string()),
                    turn_id: None,
                },
            },
        },
    ));
    view
}

fn envelope(sequence: u64, kind: RuntimeEventKind) -> RuntimeEventEnvelope {
    RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: RuntimeOwner::default(),
        cursor: EventCursor {
            stream_id: "gui-test".to_string(),
            sequence,
        },
        event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
            sequence,
            Some(1_700_000_000 + sequence),
            kind,
        )),
    }
}

fn page_loaded(sequence: u64, command_id: &str, page: EvidencePage) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::EvidencePageLoaded {
            command_id: command_id.to_string(),
            page,
        },
    )
}

fn content_loaded(
    sequence: u64,
    command_id: &str,
    evidence_id: &str,
    content: EvidenceContent,
) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::EvidenceContentLoaded {
            command_id: command_id.to_string(),
            evidence_id: evidence_id.to_string(),
            content,
        },
    )
}

fn refused(sequence: u64, command_id: &str, reason: &str) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::CommandRejected {
            command_id: command_id.to_string(),
            reason: reason.to_string(),
        },
    )
}

fn unrelated_error(sequence: u64, message: &str) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::Error {
            error: RuntimeErrorView {
                message: message.to_string(),
                recoverable: true,
                hint: None,
            },
        },
    )
}

fn recorded(sequence: u64) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::EvidenceRecorded {
            evidence: row("evidence_delta", "review", Some(1_700_000_400)),
        },
    )
}

/// Builds one archive row from Core's own wire form.
///
/// Deserialized rather than constructed: `CanonicalEvidenceReference` and
/// `EvidenceProducer` are not part of the `viden-core` facade, and the GUI
/// crate may depend on nothing else. The JSON here is exactly the shape
/// `evidence-reads.json` publishes. The two verdict enums *are* on the facade
/// now, which is what lets [`distrusted_row`] set them by name.
fn row(id: &str, kind: &str, timestamp: Option<u64>) -> EvidenceView {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "kind": kind,
        "summary": format!("{kind} evidence {id}"),
        "path": "crates/types/src/evidence_reads.rs",
        "source": "lane_evidence_reads",
        "canonical": {
            "item_id": format!("item_{id}"),
            "bundle_id": "bundle_evidence_reads",
            "source_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "producer": {
                "identity": "lane_evidence_reads",
                "role": "coder",
                "task_id": "task_evidence_reads",
            },
            "permission_snapshot_id": "permission-receipt-evidence-reads",
            "permission_scope": { "type": "task", "id": "task_evidence_reads" },
            "evidence_scope": { "type": "task", "id": "task_evidence_reads" },
            "verification": "verified",
            "quality": { "status": "pass" },
        },
        "metadata": {
            "command": "cargo test -p viden-types",
            "exit": 0,
        },
        "timestamp": timestamp,
        "owner": {
            "workspace_id": "workspace_contract_v1",
            "project_id": "project_viden",
            "lane_id": "lane_evidence_reads",
            "session_id": "session_evidence_reads",
            "task_id": "task_evidence_reads",
            "turn_id": "turn_evidence_reads",
        },
    }))
    .expect("evidence row")
}

/// A display-only row: no canonical reference at all, which is exactly what
/// `Unavailable { SummaryOnly }` answers.
fn summary_row(id: &str) -> EvidenceView {
    EvidenceView {
        canonical: None,
        metadata: None,
        ..row(id, "task_summary", None)
    }
}

/// A row Core holds bytes for but could not verify, and whose quality it
/// warned on.
///
/// Both verdicts are Core's own enums, assigned here through the facade
/// re-exports. Neither may collapse into "no canonical reference": a reviewer
/// reading this row must be able to tell bytes Core distrusts from bytes Core
/// never had.
fn distrusted_row(id: &str) -> EvidenceView {
    let mut entry = row(id, "patch", Some(1_700_000_500));
    if let Some(canonical) = entry.canonical.as_mut() {
        canonical.verification = EvidenceVerificationState::Failed;
        canonical.quality.status = EvidenceQualityStatus::Warn;
    }
    entry
}

fn page(entries: Vec<EvidenceView>, complete: bool, next_after: Option<&str>) -> EvidencePage {
    EvidencePage {
        entries,
        complete,
        next_after: next_after.map(str::to_string),
    }
}

struct Harness {
    adapter: GuiCoreAdapter,
    sent: Arc<Mutex<Vec<RuntimeCommandEnvelope>>>,
}

fn harness(events: Vec<RuntimeEventEnvelope>, with_capability: bool) -> Harness {
    harness_with_view(view(), events, with_capability)
}

fn harness_with_view(
    view: RuntimeViewState,
    events: Vec<RuntimeEventEnvelope>,
    with_capability: bool,
) -> Harness {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut client = TestCoreClient::new(view, sent.clone());
    if !with_capability {
        client.capabilities.remove(EVIDENCE_READS_CAPABILITY);
    }
    for event in events {
        client = client.with_envelope(event);
    }
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect");
    Harness { adapter, sent }
}

fn evidence_queries(
    sent: &Arc<Mutex<Vec<RuntimeCommandEnvelope>>>,
) -> Vec<viden_core::EvidenceQuery> {
    sent.lock()
        .expect("sent lock")
        .iter()
        .filter_map(|envelope| match &envelope.command {
            RuntimeCommand::QueryEvidence { query } => Some(query.clone()),
            _ => None,
        })
        .collect()
}

fn content_reads(sent: &Arc<Mutex<Vec<RuntimeCommandEnvelope>>>) -> Vec<String> {
    sent.lock()
        .expect("sent lock")
        .iter()
        .filter_map(|envelope| match &envelope.command {
            RuntimeCommand::ReadEvidenceContent { evidence_id } => Some(evidence_id.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_confirming_page_becomes_the_archive_in_cores_own_order() {
    let mut harness = harness(
        vec![page_loaded(
            1,
            "gui-evidence-1",
            page(
                vec![
                    summary_row("evidence_undated"),
                    row("evidence_alpha_patch", "patch", Some(1_700_000_100)),
                    row("evidence_bravo_tests", "test_result", Some(1_700_000_200)),
                ],
                false,
                "t:1700000200:evidence_bravo_tests".into(),
            ),
        )],
        true,
    );
    let projection = harness
        .adapter
        .query_evidence_and_wait("gui-evidence-1", None, Vec::new(), TIMEOUT)
        .expect("evidence read");

    assert!(projection.capability_available);
    assert!(projection.loaded);
    assert_eq!(projection.outcome.state, "confirmed");
    assert_eq!(projection.pending_command_id, None);
    assert!(!projection.complete, "Core said a newer page exists");
    assert!(!projection.stale, "no recording arrived after the page");

    // Core's order, verbatim, with the undated row first. The client never
    // re-sorts and never moves the undated row to the end.
    assert_eq!(
        projection
            .rows
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "evidence_undated",
            "evidence_alpha_patch",
            "evidence_bravo_tests",
        ]
    );
    assert_eq!(projection.rows[0].timestamp, None);
    assert_eq!(
        projection.rows[0].canonical, None,
        "a display-only row names no canonical bytes"
    );

    // The cursor is carried verbatim; nothing is parsed out of it.
    assert_eq!(
        projection.next_after.as_deref(),
        Some("t:1700000200:evidence_bravo_tests")
    );

    // `metadata` is flattened into facts and never interpreted.
    let patch = &projection.rows[1];
    assert_eq!(
        patch
            .metadata
            .iter()
            .map(|fact| (fact.key.as_str(), fact.value.as_str()))
            .collect::<Vec<_>>(),
        vec![("command", "cargo test -p viden-types"), ("exit", "0")]
    );
    let reference = patch.canonical.as_ref().expect("canonical reference");
    assert_eq!(reference.item_id, "item_evidence_alpha_patch");
    assert_eq!(reference.producer_role, "coder");
    // Core's own verdicts on the reference, named rather than inferred from
    // the hash or the summary. The facade re-exports both enums, so the client
    // states what Core concluded instead of leaving the row silent about it.
    assert_eq!(reference.verification, "verified");
    assert_eq!(reference.quality, "pass");
    assert_eq!(patch.owner_lane_id.as_deref(), Some("lane_evidence_reads"));

    // The unscoped default: no owner, no kind filter, Core's default page.
    let queries = evidence_queries(&harness.sent);
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0].owner, None);
    assert!(queries[0].kinds.is_empty());
    assert_eq!(queries[0].after, None);
    assert_eq!(queries[0].limit, viden_gui::EVIDENCE_PAGE_LIMIT);
}

/// A distrusted reference states both verdicts; it does not become a row
/// without canonical bytes.
#[test]
fn a_failed_verification_and_a_warned_quality_reach_the_row_as_themselves() {
    let mut harness = harness(
        vec![page_loaded(
            1,
            "gui-evidence-1",
            page(vec![distrusted_row("evidence_distrusted")], true, None),
        )],
        true,
    );
    let projection = harness
        .adapter
        .query_evidence_and_wait("gui-evidence-1", None, Vec::new(), TIMEOUT)
        .expect("evidence read");
    let reference = projection.rows[0]
        .canonical
        .as_ref()
        .expect("the row still names the bytes Core holds");
    assert_eq!(reference.verification, "failed");
    assert_eq!(reference.quality, "warn");
}

#[test]
fn load_older_appends_the_next_page_through_cores_own_cursor() {
    let mut harness = harness(
        vec![
            page_loaded(
                1,
                "gui-evidence-1",
                page(
                    vec![row("evidence_alpha_patch", "patch", Some(1_700_000_100))],
                    false,
                    "t:1700000100:evidence_alpha_patch".into(),
                ),
            ),
            page_loaded(
                2,
                "gui-evidence-2",
                page(
                    vec![row(
                        "evidence_bravo_tests",
                        "test_result",
                        Some(1_700_000_200),
                    )],
                    true,
                    None,
                ),
            ),
        ],
        true,
    );
    harness
        .adapter
        .query_evidence_and_wait("gui-evidence-1", None, Vec::new(), TIMEOUT)
        .expect("first page");
    let projection = harness
        .adapter
        .load_older_evidence_and_wait("gui-evidence-2", TIMEOUT)
        .expect("second page");

    assert_eq!(
        projection
            .rows
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        vec!["evidence_alpha_patch", "evidence_bravo_tests"],
        "the older page appends; it never replaces the loaded rows"
    );
    assert!(projection.complete);
    assert_eq!(
        projection.next_after, None,
        "`next_after` is None exactly when `complete` is true"
    );

    // The cursor went back exactly as Core issued it.
    let queries = evidence_queries(&harness.sent);
    assert_eq!(queries.len(), 2);
    assert_eq!(
        queries[1].after.as_deref(),
        Some("t:1700000100:evidence_alpha_patch")
    );
}

#[test]
fn a_kind_filter_travels_to_core_and_is_remembered_by_the_older_page() {
    let mut harness = harness(
        vec![
            page_loaded(
                1,
                "gui-evidence-1",
                page(
                    vec![row("evidence_alpha_patch", "patch", Some(1_700_000_100))],
                    false,
                    "t:1700000100:evidence_alpha_patch".into(),
                ),
            ),
            page_loaded(2, "gui-evidence-2", page(Vec::new(), true, None)),
        ],
        true,
    );
    harness
        .adapter
        .query_evidence_and_wait("gui-evidence-1", None, vec!["patch".to_string()], TIMEOUT)
        .expect("filtered page");
    let projection = harness
        .adapter
        .load_older_evidence_and_wait("gui-evidence-2", TIMEOUT)
        .expect("older filtered page");

    assert_eq!(projection.kinds, vec!["patch".to_string()]);
    let queries = evidence_queries(&harness.sent);
    assert_eq!(queries[0].kinds, vec!["patch".to_string()]);
    assert_eq!(
        queries[1].kinds,
        vec!["patch".to_string()],
        "an older page continues the same question, filter included"
    );
}

#[test]
fn a_lane_scope_narrows_to_the_lane_without_carrying_cores_turn_binding() {
    let mut harness = harness_with_view(
        view_with_lane_owner(),
        vec![page_loaded(
            1,
            "gui-evidence-1",
            page(
                vec![row("evidence_alpha_patch", "patch", Some(1_700_000_100))],
                true,
                None,
            ),
        )],
        true,
    );

    let projection = harness
        .adapter
        .query_evidence_and_wait("gui-evidence-1", Some(LANE), Vec::new(), TIMEOUT)
        .expect("scoped read");
    assert_eq!(projection.scope_lane_id.as_deref(), Some(LANE));

    let queries = evidence_queries(&harness.sent);
    let scope = queries[0].owner.as_ref().expect("an owner scope");
    assert_eq!(scope.lane_id.as_deref(), Some(LANE));
    assert_eq!(scope.workspace_id, "workspace_contract_v1");
    // A prefix scope match: naming the turn would answer "this turn's
    // evidence" for a question about the Lane.
    assert_eq!(scope.session_id, None);
    assert_eq!(scope.task_id, None);
    assert_eq!(scope.turn_id, None);
}

#[test]
fn a_lane_with_no_core_owner_is_refused_locally_rather_than_read_unscoped() {
    let mut harness = harness(Vec::new(), true);
    let error = harness
        .adapter
        .query_evidence_and_wait(
            "gui-evidence-1",
            Some("lane-nobody-bound"),
            Vec::new(),
            TIMEOUT,
        )
        .expect_err("a Lane with no owner binding cannot be scoped");

    assert!(error.contains(viden_gui::EVIDENCE_NO_OWNER_CODE), "{error}");
    assert!(
        evidence_queries(&harness.sent).is_empty(),
        "an unscopable read is never sent as an unscoped one"
    );
}

#[test]
fn a_refusal_is_cores_own_reason_and_never_an_empty_archive() {
    let mut harness = harness(
        vec![refused(
            1,
            "gui-evidence-1",
            "evidence query kinds exceed the 32 entry bound: 33 requested\nhint: ask for fewer \
             kinds, or drop the filter and page the archive",
        )],
        true,
    );
    let projection = harness
        .adapter
        .query_evidence_and_wait("gui-evidence-1", None, Vec::new(), TIMEOUT)
        .expect("read");

    assert_eq!(projection.outcome.state, "rejected");
    assert!(
        projection
            .outcome
            .reason
            .as_deref()
            .expect("reason")
            .contains("32 entry bound")
    );
    assert!(
        !projection.loaded,
        "a refusal is not a loaded empty archive"
    );
    assert!(projection.rows.is_empty());
    assert_eq!(projection.pending_command_id, None);
}

#[test]
fn an_over_limit_filter_is_refused_by_cores_own_validator_before_anything_is_sent() {
    let mut harness = harness(Vec::new(), true);
    let kinds = (0..33).map(|index| format!("kind_{index}")).collect();
    let error = harness
        .adapter
        .query_evidence_and_wait("gui-evidence-1", None, kinds, TIMEOUT)
        .expect_err("Core's own bound");

    assert!(error.contains("32 entry bound"), "{error}");
    assert!(evidence_queries(&harness.sent).is_empty());
}

#[test]
fn an_event_naming_another_read_never_settles_this_one() {
    let mut harness = harness(
        vec![
            page_loaded(1, "gui-evidence-other", page(Vec::new(), true, None)),
            unrelated_error(2, "a lane failed"),
            page_loaded(
                3,
                "gui-evidence-1",
                page(
                    vec![row("evidence_alpha_patch", "patch", Some(1_700_000_100))],
                    true,
                    None,
                ),
            ),
        ],
        true,
    );
    let projection = harness
        .adapter
        .query_evidence_and_wait("gui-evidence-1", None, Vec::new(), TIMEOUT)
        .expect("read");

    assert_eq!(projection.outcome.state, "confirmed");
    assert_eq!(
        projection.rows.len(),
        1,
        "the other page is not this read's"
    );
}

#[test]
fn an_absent_capability_sends_nothing_and_states_itself() {
    let mut harness = harness(Vec::new(), false);
    let projection = harness
        .adapter
        .query_evidence_and_wait("gui-evidence-1", None, Vec::new(), TIMEOUT)
        .expect("read");

    assert!(!projection.capability_available);
    assert!(
        !projection.loaded,
        "absence is not an answered empty archive"
    );
    assert_eq!(projection.outcome.state, "idle");
    assert!(evidence_queries(&harness.sent).is_empty());
}

#[test]
fn a_recording_after_the_page_marks_the_list_stale_without_reloading_it() {
    let mut harness = harness(
        vec![
            page_loaded(
                1,
                "gui-evidence-1",
                page(
                    vec![row("evidence_alpha_patch", "patch", Some(1_700_000_100))],
                    true,
                    None,
                ),
            ),
            recorded(2),
        ],
        true,
    );
    let projection = harness
        .adapter
        .query_evidence_and_wait("gui-evidence-1", None, Vec::new(), TIMEOUT)
        .expect("read");
    assert!(!projection.stale, "the page is current when it lands");

    harness.adapter.pump_events(TIMEOUT);
    let after = harness.adapter.evidence_archive();
    assert!(after.stale, "an EvidenceRecorded fact invalidates the page");
    assert_eq!(
        after.rows.len(),
        1,
        "staleness is a re-read signal; the rows stay on screen"
    );
}

#[test]
fn text_content_carries_its_bound_and_the_hash_core_verified_it_against() {
    let mut harness = harness(
        vec![content_loaded(
            1,
            "gui-content-1",
            "evidence_bravo_tests",
            EvidenceContent::Text {
                text: "running 3 tests\ntest result: ok. 3 passed; 0 failed\n".to_string(),
                truncated: true,
                sha256: "bbbb".to_string(),
            },
        )],
        true,
    );
    let content = harness
        .adapter
        .read_evidence_content_and_wait("gui-content-1", "evidence_bravo_tests", TIMEOUT)
        .expect("content read");

    assert_eq!(content.outcome.state, "confirmed");
    assert_eq!(content.kind, "text");
    assert_eq!(content.evidence_id.as_deref(), Some("evidence_bravo_tests"));
    assert!(
        content.truncated,
        "the bound cut the bytes; the row says so"
    );
    assert_eq!(content.sha256.as_deref(), Some("bbbb"));
    assert!(content.text.as_deref().expect("text").contains("3 passed"));
    assert_eq!(content_reads(&harness.sent), vec!["evidence_bravo_tests"]);
}

#[test]
fn patch_content_arrives_as_parsed_rows_rather_than_text() {
    let document = DiffDocument {
        files: vec![DiffFile {
            path: "crates/types/src/evidence_reads.rs".to_string(),
            old_path: None,
            kind: WorkspaceChangeKind::Modified,
            binary: false,
            omitted: false,
            additions: 1,
            deletions: 1,
            hunks: vec![DiffHunk {
                old_start: 12,
                old_lines: 3,
                new_start: 12,
                new_lines: 3,
                header: Some("impl EvidenceQuery".to_string()),
                lines: vec![
                    DiffLine {
                        kind: DiffLineKind::Removed,
                        content: "        self.limit as usize".to_string(),
                        old_line: Some(13),
                        new_line: None,
                    },
                    DiffLine {
                        kind: DiffLineKind::Added,
                        content: "        self.limit.clamp(1, 200) as usize".to_string(),
                        old_line: None,
                        new_line: Some(13),
                    },
                ],
            }],
        }],
        truncated: false,
        byte_limit: 262_144,
    };
    let mut harness = harness(
        vec![content_loaded(
            1,
            "gui-content-1",
            "evidence_alpha_patch",
            EvidenceContent::Diff {
                document,
                sha256: "aaaa".to_string(),
            },
        )],
        true,
    );
    let content = harness
        .adapter
        .read_evidence_content_and_wait("gui-content-1", "evidence_alpha_patch", TIMEOUT)
        .expect("content read");

    assert_eq!(content.kind, "diff");
    assert_eq!(content.text, None, "a diff is rows, never a text body");
    let document = content.document.as_ref().expect("parsed document");
    assert_eq!(document.files.len(), 1);
    let hunk = &document.files[0].hunks[0];
    assert_eq!(hunk.header.as_deref(), Some("impl EvidenceQuery"));
    assert_eq!(hunk.lines[0].kind, "removed");
    assert_eq!(hunk.lines[0].new_line, None);
    assert_eq!(hunk.lines[1].kind, "added");
    assert_eq!(hunk.lines[1].old_line, None);
}

#[test]
fn the_durable_work_fixture_projects_cores_own_patch_row_and_its_canonical_bytes() {
    // Fixture truth rather than a hand-built row: C7
    // (`runtime.durable_work_evidence`) is what makes a patch *exist* in the
    // archive for a native tool edit and for an ACP patch, and this replays
    // the page and content events Core committed for it. The archive read
    // itself is unchanged — which is the point: the client needed no new
    // vocabulary to show the rows the runtime started writing.
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../crates/types/tests/fixtures/frontend-contract-v1/durable-work-evidence.json"
    ))
    .expect("fixture json");
    let envelopes: Vec<RuntimeEventEnvelope> =
        serde_json::from_value(fixture["events"].clone()).expect("fixture events");
    let reads: Vec<RuntimeEventEnvelope> = envelopes
        .into_iter()
        .filter(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(event) => matches!(
                event.kind,
                RuntimeEventKind::EvidencePageLoaded { .. }
                    | RuntimeEventKind::EvidenceContentLoaded { .. }
            ),
            _ => false,
        })
        .collect();
    assert_eq!(reads.len(), 2, "the fixture answers one page and one read");

    let mut harness = harness(reads, true);
    let projection = harness
        .adapter
        .query_evidence_and_wait(
            "cmd_durable_work_page",
            None,
            vec!["patch".to_string()],
            TIMEOUT,
        )
        .expect("the fixture's page");

    let entry = projection.rows.first().expect("the patch row");
    assert_eq!(entry.id, "patch-tool_durable_work_edit");
    assert_eq!(entry.kind, "patch");
    assert_eq!(
        entry.path.as_deref(),
        Some("crates/types/src/evidence_reads.rs")
    );
    // The row is archived work, so it carries the canonical reference the
    // content read resolves; a client that showed the summary without it
    // could not tell archived bytes from a display-only row.
    let canonical = entry.canonical.as_ref().expect("canonical reference");
    assert_eq!(canonical.item_id, "ctxi_durable_work_native");
    assert_eq!(canonical.verification, "verified");

    let content = harness
        .adapter
        .read_evidence_content_and_wait(
            "cmd_durable_work_content",
            "patch-tool_durable_work_edit",
            TIMEOUT,
        )
        .expect("the fixture's bytes");
    assert_eq!(content.kind, "diff");
    let document = content.document.as_ref().expect("parsed diff rows");
    assert_eq!(
        document.files[0].path, "crates/types/src/evidence_reads.rs",
        "the shared diff renderer draws Core's own rows"
    );
}

#[test]
fn every_unavailable_reason_keeps_its_own_name() {
    for (reason, expected) in [
        (EvidenceUnavailableReason::SummaryOnly, "summary_only"),
        (
            EvidenceUnavailableReason::MissingCanonicalBytes,
            "missing_canonical_bytes",
        ),
        (EvidenceUnavailableReason::HashMismatch, "hash_mismatch"),
        (EvidenceUnavailableReason::Binary, "binary"),
    ] {
        let mut harness = harness(
            vec![content_loaded(
                1,
                "gui-content-1",
                "evidence_charlie_summary",
                EvidenceContent::Unavailable { reason },
            )],
            true,
        );
        let content = harness
            .adapter
            .read_evidence_content_and_wait("gui-content-1", "evidence_charlie_summary", TIMEOUT)
            .expect("content read");
        assert_eq!(content.kind, "unavailable");
        assert_eq!(content.reason, Some(expected));
        assert_eq!(
            content.text, None,
            "no unavailable reason ever ships a body beside it"
        );
        assert_eq!(
            content.sha256, None,
            "there is no hash for bytes Core did not serve"
        );
    }
}

#[test]
fn an_unknown_evidence_id_is_a_refusal_carrying_cores_own_words() {
    let mut harness = harness(
        vec![refused(
            1,
            "gui-content-1",
            "evidence `evidence_nope` is not an evidence id Core recorded\nhint: page the archive \
             and read a row Core published",
        )],
        true,
    );
    let content = harness
        .adapter
        .read_evidence_content_and_wait("gui-content-1", "evidence_nope", TIMEOUT)
        .expect("content read");

    assert_eq!(content.outcome.state, "rejected");
    assert_eq!(content.kind, "absent", "a refusal is not empty content");
    assert!(
        content
            .outcome
            .reason
            .as_deref()
            .expect("reason")
            .contains("not an evidence id Core recorded")
    );
    assert_eq!(content.evidence_id.as_deref(), Some("evidence_nope"));
}

#[test]
fn a_second_content_read_drops_the_previous_rows_bytes_before_it_leaves() {
    let mut harness = harness(
        vec![content_loaded(
            1,
            "gui-content-1",
            "evidence_bravo_tests",
            EvidenceContent::Text {
                text: "first".to_string(),
                truncated: false,
                sha256: "bbbb".to_string(),
            },
        )],
        true,
    );
    harness
        .adapter
        .read_evidence_content_and_wait("gui-content-1", "evidence_bravo_tests", TIMEOUT)
        .expect("first read");
    // The second read has no queued answer, so it stays pending — which is
    // exactly the moment a leftover body would be attributed to the new row.
    let content = harness
        .adapter
        .read_evidence_content_and_wait("gui-content-2", "evidence_alpha_patch", TIMEOUT)
        .expect("second read");

    assert_eq!(content.outcome.state, "pending");
    assert_eq!(content.evidence_id.as_deref(), Some("evidence_alpha_patch"));
    assert_eq!(content.kind, "absent");
    assert_eq!(content.text, None);
}
