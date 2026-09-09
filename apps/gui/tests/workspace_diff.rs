//! The DiffReview read side, projected from Core's structured diff
//! (`runtime.structured_diff`, GUI-CORE-012).
//!
//! These tests cover the correlation machine, the capability gate, the refusal
//! path, the honesty flags, and the staleness signal that drives the view's
//! re-query. Correlation has the same two cases the inventory read has:
//! `WorkspaceDiffLoaded.command_id` is a *required* field, so a page either
//! names this read or belongs to another one. There is no legacy id-less page
//! and therefore no acceptance-first fallback.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use viden_core::{
    DiffFile, DiffHunk, DiffLine, DiffLineKind, EventCursor, FRONTEND_SCHEMA_V1, RuntimeCommand,
    RuntimeCommandEnvelope, RuntimeErrorView, RuntimeEvent, RuntimeEventEnvelope, RuntimeEventKind,
    RuntimeOwner, RuntimeSnapshot, RuntimeViewState, RuntimeWireEvent, SourceTarget,
    WorkspaceChangeKind, WorkspaceChangeView, WorkspaceDiffEntry, WorkspaceDiffPage,
    WorkspaceDiffQuery, WorkspaceDiffScope, WorkspaceSourceStatus, WorkspaceSourceView,
};
use viden_gui::{GuiCoreAdapter, STRUCTURED_DIFF_CAPABILITY};

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

fn accepted(sequence: u64, command_id: &str) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::CommandAccepted {
            command_id: command_id.to_string(),
            command: RuntimeCommand::QueryWorkspaceDiff {
                query: WorkspaceDiffQuery::default(),
            },
        },
    )
}

fn source() -> WorkspaceSourceView {
    WorkspaceSourceView {
        status: WorkspaceSourceStatus::Ready,
        branch: Some("claude/gui-diff-review".to_string()),
        worktree: Some("/workspace/viden".to_string()),
        ahead: 1,
        behind: 0,
        added: 2,
        deleted: 0,
        dirty: true,
    }
}

/// One modified file with a real hunk, the shape the fixture's staged entry
/// carries.
fn modified_file(path: &str) -> DiffFile {
    DiffFile {
        path: path.to_string(),
        old_path: None,
        kind: WorkspaceChangeKind::Modified,
        binary: false,
        omitted: false,
        additions: 1,
        deletions: 1,
        hunks: vec![DiffHunk {
            old_start: 42,
            old_lines: 3,
            new_start: 42,
            new_lines: 3,
            header: Some("pub struct DiffDocument {".to_string()),
            lines: vec![
                DiffLine {
                    kind: DiffLineKind::Context,
                    content: "    pub files: Vec<DiffFile>,".to_string(),
                    old_line: Some(42),
                    new_line: Some(42),
                },
                DiffLine {
                    kind: DiffLineKind::Removed,
                    content: "    pub truncated: bool,".to_string(),
                    old_line: Some(43),
                    new_line: None,
                },
                DiffLine {
                    kind: DiffLineKind::Added,
                    content: "    pub truncated: bool, // bounded".to_string(),
                    old_line: None,
                    new_line: Some(43),
                },
            ],
        }],
    }
}

/// A file the byte bound dropped: real counts, zero rows, `omitted`.
fn omitted_file(path: &str) -> DiffFile {
    DiffFile {
        path: path.to_string(),
        old_path: None,
        kind: WorkspaceChangeKind::Added,
        binary: false,
        omitted: true,
        additions: 1_284,
        deletions: 0,
        hunks: Vec::new(),
    }
}

fn page(entries: Vec<WorkspaceDiffEntry>, truncated: bool) -> WorkspaceDiffPage {
    WorkspaceDiffPage {
        target: SourceTarget::Workspace,
        source: source(),
        entries,
        truncated,
    }
}

fn loaded(sequence: u64, command_id: &str, page: WorkspaceDiffPage) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::WorkspaceDiffLoaded {
            command_id: command_id.to_string(),
            page,
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

fn source_updated(sequence: u64) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::WorkspaceSourceUpdated { source: source() },
    )
}

fn change_updated(sequence: u64) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::WorkspaceChangeUpdated {
            change: WorkspaceChangeView {
                id: "change-1".to_string(),
                owner: RuntimeOwner::default(),
                path: "crates/types/src/diff.rs".to_string(),
                kind: WorkspaceChangeKind::Modified,
                patch: None,
                additions: 1,
                deletions: 1,
                diff: None,
            },
        },
    )
}

struct Harness {
    adapter: GuiCoreAdapter,
    sent: Arc<Mutex<Vec<RuntimeCommandEnvelope>>>,
}

fn harness(events: Vec<RuntimeEventEnvelope>, with_capability: bool) -> Harness {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut client = TestCoreClient::new(view(), sent.clone());
    if !with_capability {
        client.capabilities.remove(STRUCTURED_DIFF_CAPABILITY);
    }
    for event in events {
        client = client.with_envelope(event);
    }
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect");
    Harness { adapter, sent }
}

fn diff_queries(sent: &Arc<Mutex<Vec<RuntimeCommandEnvelope>>>) -> Vec<WorkspaceDiffQuery> {
    sent.lock()
        .expect("sent lock")
        .iter()
        .filter_map(|envelope| match &envelope.command {
            RuntimeCommand::QueryWorkspaceDiff { query, .. } => Some(query.clone()),
            _ => None,
        })
        .collect()
}

fn staged_entry() -> WorkspaceDiffEntry {
    WorkspaceDiffEntry {
        path: "crates/types/src/diff.rs".to_string(),
        index: Some(WorkspaceChangeKind::Modified),
        worktree: None,
        staged: true,
        diff: Some(modified_file("crates/types/src/diff.rs")),
    }
}

fn omitted_entry() -> WorkspaceDiffEntry {
    WorkspaceDiffEntry {
        path: "crates/types/tests/fixtures/frontend-contract-v1/structured-diff.json".to_string(),
        index: None,
        worktree: Some(WorkspaceChangeKind::Added),
        staged: false,
        diff: Some(omitted_file(
            "crates/types/tests/fixtures/frontend-contract-v1/structured-diff.json",
        )),
    }
}

#[test]
fn a_confirming_page_becomes_the_review_projection_in_cores_own_order() {
    let mut harness = harness(
        vec![
            accepted(1, "gui-diff-1"),
            loaded(
                2,
                "gui-diff-1",
                page(vec![staged_entry(), omitted_entry()], true),
            ),
        ],
        true,
    );
    let projection = harness
        .adapter
        .query_workspace_diff_and_wait("gui-diff-1", None, TIMEOUT)
        .expect("diff read");

    assert!(projection.capability_available);
    assert!(projection.loaded);
    assert_eq!(projection.outcome.state, "confirmed");
    assert_eq!(projection.pending_command_id, None);
    assert_eq!(projection.target_lane_id, None);
    assert!(projection.truncated, "the page bound dropped an entry");
    assert!(!projection.stale, "no source fact arrived after the page");

    // Core's order, verbatim: the client never re-sorts.
    assert_eq!(
        projection
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>(),
        vec![
            "crates/types/src/diff.rs",
            "crates/types/tests/fixtures/frontend-contract-v1/structured-diff.json",
        ]
    );

    let staged = &projection.entries[0];
    assert!(staged.staged);
    assert_eq!(staged.index, Some("modified"));
    assert_eq!(staged.worktree, None, "absence is not a classification");
    let diff = staged.diff.as_ref().expect("Core published rows");
    assert_eq!(diff.kind, "modified");
    assert_eq!(diff.additions, 1);
    assert_eq!(diff.deletions, 1);
    let hunk = &diff.hunks[0];
    assert_eq!(hunk.header.as_deref(), Some("pub struct DiffDocument {"));
    assert_eq!(hunk.old_start, 42);
    // Per-side numbering survives: a removed row has no new-file position and
    // an added row has no old-file position, and neither is zero.
    assert_eq!(hunk.lines[1].kind, "removed");
    assert_eq!(hunk.lines[1].old_line, Some(43));
    assert_eq!(hunk.lines[1].new_line, None);
    assert_eq!(hunk.lines[2].kind, "added");
    assert_eq!(hunk.lines[2].old_line, None);
    assert_eq!(hunk.lines[2].new_line, Some(43));

    // The omitted entry keeps its real counts and publishes no rows.
    let omitted = projection.entries[1].diff.as_ref().expect("entry diff");
    assert!(omitted.omitted);
    assert_eq!(omitted.additions, 1_284);
    assert!(omitted.hunks.is_empty());

    // The page resamples the target's source facts, so the view names the
    // branch the diff came from.
    let source = projection.source.as_ref().expect("resampled source");
    assert_eq!(source.branch.as_deref(), Some("claude/gui-diff-review"));

    // Default query: the whole workspace, both sides, every path, Core's bound.
    let queries = diff_queries(&harness.sent);
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0].target, SourceTarget::Workspace);
    assert_eq!(queries[0].scope, WorkspaceDiffScope::Both);
    assert!(queries[0].paths.is_empty());
    assert_eq!(queries[0].byte_limit, None);
}

#[test]
fn a_lane_target_asks_core_for_that_lanes_worktree() {
    let mut harness = harness(
        vec![
            accepted(1, "gui-diff-1"),
            loaded(2, "gui-diff-1", page(vec![staged_entry()], false)),
        ],
        true,
    );
    harness
        .adapter
        .query_workspace_diff_and_wait("gui-diff-1", Some("lane-core"), TIMEOUT)
        .expect("diff read");

    let queries = diff_queries(&harness.sent);
    assert_eq!(queries.len(), 1);
    match &queries[0].target {
        SourceTarget::Lane { lane_id } => assert_eq!(lane_id, "lane-core"),
        other => panic!("expected a Lane target, got {other:?}"),
    }
}

#[test]
fn an_answered_read_over_a_clean_tree_stays_loaded_and_empty() {
    let mut harness = harness(
        vec![
            accepted(1, "gui-diff-1"),
            loaded(2, "gui-diff-1", page(Vec::new(), false)),
        ],
        true,
    );
    let projection = harness
        .adapter
        .query_workspace_diff_and_wait("gui-diff-1", None, TIMEOUT)
        .expect("diff read");

    // The one case where an empty list is the truth. It is only readable as
    // such because `loaded` says Core answered.
    assert!(projection.loaded);
    assert!(projection.entries.is_empty());
    assert_eq!(projection.outcome.state, "confirmed");
}

#[test]
fn a_page_naming_another_read_is_ignored_with_no_acceptance_fallback() {
    let mut harness = harness(
        vec![
            accepted(1, "gui-diff-1"),
            loaded(2, "gui-diff-other", page(vec![staged_entry()], false)),
        ],
        true,
    );
    let projection = harness
        .adapter
        .query_workspace_diff_and_wait("gui-diff-1", None, TIMEOUT)
        .expect("diff read");

    assert!(
        !projection.loaded,
        "another read's page must not settle ours"
    );
    assert!(projection.entries.is_empty());
    assert_eq!(projection.pending_command_id.as_deref(), Some("gui-diff-1"));
    assert_eq!(projection.outcome.state, "pending");
}

#[test]
fn a_rejected_read_surfaces_cores_refusal_verbatim_and_never_an_empty_page() {
    let reason = "permission denied\ntool: git_diff\nreason: DenyRule\nhint: grant the `git_diff` permission to read structured workspace changes";
    let mut harness = harness(
        vec![accepted(1, "gui-diff-1"), refused(2, "gui-diff-1", reason)],
        true,
    );
    let projection = harness
        .adapter
        .query_workspace_diff_and_wait("gui-diff-1", None, TIMEOUT)
        .expect("diff read");

    assert_eq!(projection.outcome.state, "rejected");
    assert_eq!(
        projection.outcome.reason.as_deref(),
        Some(reason),
        "Core's own words, unedited"
    );
    // A refusal and a clean tree must never render the same.
    assert!(!projection.loaded);
    assert!(projection.entries.is_empty());
}

#[test]
fn an_unrelated_error_never_fails_an_outstanding_read() {
    let mut harness = harness(
        vec![
            accepted(1, "gui-diff-1"),
            unrelated_error(2, "lane lane-other failed to start"),
            loaded(3, "gui-diff-1", page(vec![staged_entry()], false)),
        ],
        true,
    );
    let projection = harness
        .adapter
        .query_workspace_diff_and_wait("gui-diff-1", None, TIMEOUT)
        .expect("diff read");

    assert_eq!(
        projection.outcome.state, "confirmed",
        "an unrelated error must not refuse this read, got {:?}",
        projection.outcome
    );
    assert!(projection.loaded);
}

#[test]
fn the_review_sends_nothing_without_the_core_capability() {
    let mut harness = harness(Vec::new(), false);
    let projection = harness
        .adapter
        .query_workspace_diff_and_wait("gui-diff-1", None, TIMEOUT)
        .expect("diff read");

    assert!(!projection.capability_available);
    assert!(!projection.loaded);
    assert!(projection.entries.is_empty());
    assert_eq!(projection.outcome.state, "idle");
    assert!(
        diff_queries(&harness.sent).is_empty(),
        "a missing capability must send no command at all"
    );
}

#[test]
fn a_second_read_is_refused_locally_while_one_is_in_flight() {
    let mut harness = harness(vec![accepted(1, "gui-diff-1")], true);
    harness
        .adapter
        .query_workspace_diff_and_wait("gui-diff-1", None, TIMEOUT)
        .expect("first read");
    let error = harness
        .adapter
        .query_workspace_diff_and_wait("gui-diff-2", None, TIMEOUT)
        .expect_err("a second concurrent read must be refused");
    assert!(error.contains("gui-diff-1"), "got {error}");
    assert_eq!(diff_queries(&harness.sent).len(), 1);
}

/// The re-query trigger: the two Core facts that invalidate a loaded page.
#[test]
fn a_workspace_source_fact_after_the_page_marks_the_projection_stale() {
    let mut harness = harness(
        vec![
            accepted(1, "gui-diff-1"),
            loaded(2, "gui-diff-1", page(vec![staged_entry()], false)),
            source_updated(3),
        ],
        true,
    );
    let projection = harness
        .adapter
        .query_workspace_diff_and_wait("gui-diff-1", None, TIMEOUT)
        .expect("diff read");
    assert!(!projection.stale, "the page is current when it lands");

    harness.adapter.pump_events(TIMEOUT);
    let after = harness.adapter.workspace_diff();
    assert!(
        after.stale,
        "a workspace source fact after the page invalidates it"
    );
    // The rows stay on screen: stale means "re-read", never "discard the only
    // facts the operator has".
    assert!(after.loaded);
    assert_eq!(after.entries.len(), 1);
}

#[test]
fn a_workspace_change_fact_after_the_page_marks_the_projection_stale() {
    let mut harness = harness(
        vec![
            accepted(1, "gui-diff-1"),
            loaded(2, "gui-diff-1", page(vec![staged_entry()], false)),
            change_updated(3),
        ],
        true,
    );
    harness
        .adapter
        .query_workspace_diff_and_wait("gui-diff-1", None, TIMEOUT)
        .expect("diff read");
    harness.adapter.pump_events(TIMEOUT);
    assert!(harness.adapter.workspace_diff().stale);
}

/// An invalidating fact still queued when the read was sent leaves the page
/// stale, and deliberately so.
///
/// The revision is captured when the command leaves, not when the page lands.
/// A `WorkspaceSourceUpdated` ordered before the answer may or may not be
/// reflected in what Core diffed, and the client cannot tell which. The two
/// possible errors are not symmetric: an extra debounced re-read costs one
/// bounded Core query, while a missed one leaves an operator reviewing a tree
/// that has moved. This fails safe towards the re-read.
#[test]
fn an_invalidating_fact_racing_the_read_still_marks_the_page_stale() {
    let mut harness = harness(
        vec![
            source_updated(1),
            accepted(2, "gui-diff-1"),
            loaded(3, "gui-diff-1", page(vec![staged_entry()], false)),
        ],
        true,
    );
    let projection = harness
        .adapter
        .query_workspace_diff_and_wait("gui-diff-1", None, TIMEOUT)
        .expect("diff read");
    assert!(projection.loaded, "the page still lands and still renders");
    assert!(projection.stale);
}
