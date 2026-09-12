//! The client side of `runtime.transcript_rows` (C8, closes GUI-CORE-009).
//!
//! Five rules are under test, and each one is a way the read can lie:
//!
//! 1. **One read in flight, correlated by `command_id`.** `TranscriptRowsLoaded`
//!    carries the id it answers, so a page naming another read is ignored
//!    outright rather than appended to this owner's conversation.
//! 2. **The scope is Core's owner, not a Lane id the client invented.** A Lane
//!    selection reads that Lane's exact bound owner; no selection reads the
//!    session scope. A Lane Core published no owner for is refused locally
//!    rather than read unscoped, which would show every Lane's conversation
//!    under one Lane's name.
//! 3. **The cursor is opaque and travels verbatim.** Paging older passes
//!    Core's own `older` string back; the client never parses or builds one,
//!    and `complete` is stated rather than inferred from a missing control.
//! 4. **A page reads oldest first and is prepended.** Core pages backwards, so
//!    an older page goes above what the client already holds.
//! 5. **A refusal is Core's words, and loads nothing.**

use std::sync::{Arc, Mutex};
use std::time::Duration;

use viden_core::{
    ApprovalDecision, ApprovalScope, CheckRunStatus, CheckRunView, EventCursor, FRONTEND_SCHEMA_V1,
    LaneRuntimeOwnerBinding, OwnedTranscriptRow, RuntimeCommand, RuntimeCommandEnvelope,
    RuntimeEvent, RuntimeEventEnvelope, RuntimeEventKind, RuntimeOwner, RuntimeSnapshot,
    RuntimeViewState, RuntimeWireEvent, ToolCallView, TranscriptRowContent, TranscriptRowsPage,
};
use viden_gui::{GuiCoreAdapter, TRANSCRIPT_ROWS_CAPABILITY, TRANSCRIPT_ROWS_NO_OWNER_CODE};

mod support;
use support::TestCoreClient;

const TIMEOUT: Duration = Duration::from_millis(10);
const LANE: &str = "lane_core";

fn view() -> RuntimeViewState {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../crates/types/tests/fixtures/frontend-contract-v1/multi-lane.json"
    ))
    .expect("fixture json");
    let snapshot: RuntimeSnapshot =
        serde_json::from_value(fixture["initial_snapshot"].clone()).expect("fixture snapshot");
    let mut view = RuntimeViewState::new(snapshot);
    let envelopes: Vec<RuntimeEventEnvelope> =
        serde_json::from_value(fixture["events"].clone()).expect("fixture events");
    for envelope in envelopes {
        if let RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
    }
    view.lane_runtime_owners.push(LaneRuntimeOwnerBinding {
        lane_id: LANE.to_string(),
        owner: lane_owner(),
    });
    view
}

fn lane_owner() -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden".to_string(),
        lane_id: Some(LANE.to_string()),
        session_id: Some("session_multi-lane".to_string()),
        task_id: Some("task_core".to_string()),
        turn_id: Some("turn_core".to_string()),
    }
}

fn row(id: &str, sequence: u64, content: TranscriptRowContent) -> OwnedTranscriptRow {
    OwnedTranscriptRow {
        id: id.to_string(),
        owner: lane_owner(),
        sequence,
        timestamp: Some(1_700_000_000 + sequence),
        content,
    }
}

fn page(rows: Vec<OwnedTranscriptRow>, older: Option<&str>) -> TranscriptRowsPage {
    TranscriptRowsPage {
        complete: older.is_none(),
        older: older.map(str::to_string),
        rows,
    }
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

fn loaded(sequence: u64, command_id: &str, page: TranscriptRowsPage) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::TranscriptRowsLoaded {
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

struct Harness {
    adapter: GuiCoreAdapter,
    sent: Arc<Mutex<Vec<RuntimeCommandEnvelope>>>,
}

fn harness(events: Vec<RuntimeEventEnvelope>, with_capability: bool) -> Harness {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut client = TestCoreClient::new(view(), sent.clone());
    if !with_capability {
        client.capabilities.remove(TRANSCRIPT_ROWS_CAPABILITY);
    }
    for event in events {
        client = client.with_envelope(event);
    }
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect");
    Harness { adapter, sent }
}

fn queries(sent: &Arc<Mutex<Vec<RuntimeCommandEnvelope>>>) -> Vec<RuntimeCommandEnvelope> {
    sent.lock()
        .expect("sent lock")
        .iter()
        .filter(|envelope| matches!(envelope.command, RuntimeCommand::QueryTranscriptRows { .. }))
        .cloned()
        .collect()
}

#[test]
fn a_lane_read_carries_the_exact_core_owner_and_returns_typed_rows() {
    let rows = vec![
        row(
            "row-user",
            1,
            TranscriptRowContent::User {
                text: "check the retry policy".to_string(),
                truncated: false,
            },
        ),
        row(
            "row-assistant",
            3,
            TranscriptRowContent::Assistant {
                text: "the policy retries three times".to_string(),
                truncated: true,
                evidence_id: Some("evidence_alpha".to_string()),
            },
        ),
        row(
            "row-call",
            5,
            TranscriptRowContent::ToolCall {
                call: ToolCallView {
                    tool_call_id: "call-1".to_string(),
                    name: "edit_file".to_string(),
                    input_preview: "crates/runtime/src/retry.rs".to_string(),
                    owner: Some(lane_owner()),
                },
            },
        ),
        row(
            "row-result",
            7,
            TranscriptRowContent::ToolResult {
                tool_call_id: "call-1".to_string(),
                success: true,
                summary: "one hunk applied".to_string(),
                evidence_id: Some("patch-call-1".to_string()),
            },
        ),
        row(
            "row-check",
            9,
            TranscriptRowContent::CheckRun {
                check: CheckRunView {
                    id: "check-1".to_string(),
                    owner: lane_owner(),
                    label: "unit tests".to_string(),
                    command: "cargo test -p viden-runtime".to_string(),
                    status: CheckRunStatus::Failed,
                    summary: "1 failed".to_string(),
                    failing_location: Some("crates/runtime/src/retry.rs:42".to_string()),
                },
            },
        ),
        row(
            "row-permission",
            10,
            TranscriptRowContent::Permission {
                request_id: "approval-1".to_string(),
                decision: Some(ApprovalDecision::Allow {
                    scope: ApprovalScope::Once,
                }),
                audit_id: "audit-approval-1".to_string(),
            },
        ),
    ];
    let mut harness = harness(
        vec![loaded(1, "gui-rows-1", page(rows, Some("s:1:row-user")))],
        true,
    );
    let projection = harness
        .adapter
        .query_transcript_rows_and_wait("gui-rows-1", Some(LANE), None, TIMEOUT)
        .expect("the read is answered");

    let envelopes = queries(&harness.sent);
    assert_eq!(envelopes.len(), 1, "exactly one read left the client");
    match &envelopes[0].command {
        RuntimeCommand::QueryTranscriptRows { query } => {
            assert_eq!(
                query.owner.lane_id.as_deref(),
                Some(LANE),
                "the scope is Core's own owner for this Lane"
            );
            assert_eq!(
                query.owner.turn_id, None,
                "a Lane's conversation is not one turn's"
            );
            assert_eq!(query.before, None, "the first page carries no cursor");
        }
        other => panic!("unexpected command {other:?}"),
    }

    assert!(projection.loaded);
    assert!(projection.capability_available);
    assert_eq!(projection.rows.len(), 6);
    assert_eq!(projection.rows[0].kind, "user");
    assert_eq!(projection.rows[1].kind, "assistant");
    assert!(projection.rows[1].truncated);
    assert_eq!(
        projection.rows[1].evidence_id.as_deref(),
        Some("evidence_alpha")
    );
    assert_eq!(projection.rows[2].kind, "tool_call");
    assert_eq!(projection.rows[2].tool_name.as_deref(), Some("edit_file"));
    assert_eq!(projection.rows[3].kind, "tool_result");
    assert_eq!(projection.rows[3].success, Some(true));
    assert_eq!(projection.rows[4].kind, "check_run");
    assert_eq!(
        projection.rows[4].failing_location.as_deref(),
        Some("crates/runtime/src/retry.rs:42")
    );
    assert_eq!(projection.rows[5].kind, "permission");
    assert_eq!(projection.rows[5].decision.as_deref(), Some("allow_once"));
    assert_eq!(
        projection.rows[5].audit_id.as_deref(),
        Some("audit-approval-1")
    );
    assert_eq!(projection.older.as_deref(), Some("s:1:row-user"));
    assert!(!projection.complete);
}

#[test]
fn a_page_answering_another_read_is_ignored() {
    let mut harness = harness(
        vec![loaded(
            1,
            "someone-elses-read",
            page(
                vec![row(
                    "row-other",
                    1,
                    TranscriptRowContent::User {
                        text: "another read".to_string(),
                        truncated: false,
                    },
                )],
                None,
            ),
        )],
        true,
    );
    let projection = harness
        .adapter
        .query_transcript_rows_and_wait("gui-rows-2", Some(LANE), None, TIMEOUT)
        .expect("the read is sent");
    assert!(
        projection.rows.is_empty(),
        "a page naming another read is not this owner's conversation"
    );
    assert_eq!(projection.outcome.state, "pending");
    assert_eq!(
        projection.pending_command_id.as_deref(),
        Some("gui-rows-2"),
        "the read is still out"
    );
}

#[test]
fn an_older_page_passes_cores_cursor_back_verbatim_and_prepends() {
    let mut harness = harness(
        vec![
            loaded(
                1,
                "gui-rows-3",
                page(
                    vec![row(
                        "row-newer",
                        5,
                        TranscriptRowContent::User {
                            text: "newer".to_string(),
                            truncated: false,
                        },
                    )],
                    Some("s:5:row-newer"),
                ),
            ),
            loaded(
                2,
                "gui-rows-4",
                page(
                    vec![row(
                        "row-older",
                        1,
                        TranscriptRowContent::User {
                            text: "older".to_string(),
                            truncated: false,
                        },
                    )],
                    None,
                ),
            ),
        ],
        true,
    );
    harness
        .adapter
        .query_transcript_rows_and_wait("gui-rows-3", Some(LANE), None, TIMEOUT)
        .expect("first page");
    let projection = harness
        .adapter
        .load_older_transcript_rows_and_wait("gui-rows-4", Some(LANE), TIMEOUT)
        .expect("older page");

    let envelopes = queries(&harness.sent);
    assert_eq!(envelopes.len(), 2);
    match &envelopes[1].command {
        RuntimeCommand::QueryTranscriptRows { query } => assert_eq!(
            query.before.as_deref(),
            Some("s:5:row-newer"),
            "Core's own cursor travels back unparsed"
        ),
        other => panic!("unexpected command {other:?}"),
    }
    assert_eq!(
        projection
            .rows
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        vec!["row-older", "row-newer"],
        "an older page goes above what the client already holds"
    );
    assert!(projection.complete, "Core said the scope is exhausted");
    assert!(projection.older.is_none());
}

#[test]
fn a_lane_with_no_exact_core_owner_is_refused_locally_and_sends_nothing() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut view = view();
    view.lane_runtime_owners.clear();
    let client = TestCoreClient::new(view, sent.clone());
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect");

    let error = adapter
        .query_transcript_rows_and_wait("gui-rows-5", Some(LANE), None, TIMEOUT)
        .expect_err("an unscoped read under a Lane's name is refused");
    assert!(
        error.contains(TRANSCRIPT_ROWS_NO_OWNER_CODE),
        "the refusal names the client-local code: {error}"
    );
    assert!(queries(&sent).is_empty(), "nothing is sent");
}

#[test]
fn a_refusal_is_cores_words_and_loads_nothing() {
    let mut harness = harness(
        vec![refused(
            1,
            "gui-rows-6",
            "transcript row cursor `bogus` is not a cursor this build issued",
        )],
        true,
    );
    let projection = harness
        .adapter
        .query_transcript_rows_and_wait("gui-rows-6", Some(LANE), None, TIMEOUT)
        .expect("the read is sent");
    assert_eq!(projection.outcome.state, "rejected");
    assert_eq!(
        projection.outcome.reason.as_deref(),
        Some("transcript row cursor `bogus` is not a cursor this build issued")
    );
    assert!(projection.rows.is_empty());
    assert!(!projection.loaded);
}

#[test]
fn an_absent_capability_sends_nothing_and_names_itself() {
    let mut harness = harness(Vec::new(), false);
    let projection = harness
        .adapter
        .query_transcript_rows_and_wait("gui-rows-7", Some(LANE), None, TIMEOUT)
        .expect("an absent capability is an honest projection, not an error");
    assert!(!projection.capability_available);
    assert!(!projection.loaded);
    assert!(queries(&harness.sent).is_empty());
}

#[test]
fn no_selection_reads_unscoped_exactly_as_the_evidence_archive_does() {
    let mut harness = harness(vec![loaded(1, "gui-rows-8", page(Vec::new(), None))], true);
    harness
        .adapter
        .query_transcript_rows_and_wait("gui-rows-8", None, None, TIMEOUT)
        .expect("the session scope is readable");
    match &queries(&harness.sent)[0].command {
        RuntimeCommand::QueryTranscriptRows { query } => {
            // The contract's prefix matcher cannot say "no Lane" — an unset
            // field matches anything — so no selection reads unscoped, which
            // is the evidence archive's own rule. The transcript states the
            // scope it read rather than claiming a session-only one.
            assert_eq!(query.owner, RuntimeOwner::default());
        }
        other => panic!("unexpected command {other:?}"),
    }
}
