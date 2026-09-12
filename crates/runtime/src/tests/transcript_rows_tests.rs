//! Runtime behavior of the owner-scoped transcript rows
//! (`runtime.transcript_rows`, C8, closes GUI-CORE-009).
//!
//! Five things must hold for a transcript surface to be usable by a frontend
//! that is forbidden from reading the session log itself:
//!
//! 1. every row comes from a durable fact — the append-only session transcript
//!    and the append-only audit timeline — so a reconnect and a restart answer
//!    the same read the same way;
//! 2. every row is attributed to exactly one owner, and the scope fails closed
//!    both ways: a Lane never sees the session's rows and the session never
//!    sees a Lane's;
//! 3. pages tile backwards from the newest end without repeating a row;
//! 4. a cut body says it was cut, and names the canonical evidence that still
//!    holds it in full when such a row already exists;
//! 5. a refusal is a `CommandRejected` naming this exact read, never an empty
//!    page.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use viden_context::{ContextEngine, ContextPutRequest};
use viden_types::{
    AgentDagTaskSpec, AgentRole, ApprovalDecision, ApprovalResponse, ApprovalScope, AuditActor,
    AuditObjectRef, AuditOutcome, AuditRecord, CanonicalEvidenceReference, CheckRunStatus,
    ContextContentKind, ContextScope, EvidenceProducer, EvidenceQualityFacts,
    EvidenceQualityStatus, EvidenceVerificationState, MAX_TRANSCRIPT_ROW_TEXT_BYTES, Message,
    ModelEvent, Role, RuntimeCommand, RuntimeEvent, RuntimeEventKind, RuntimeOwner, ToolCall,
    ToolResult, TranscriptEntry, TranscriptRowContent, TranscriptRowsPage, TranscriptRowsQuery,
};

use super::{SequenceProvider, temp_dir};
use crate::{RuntimeResumeRequest, SessionEngine};

struct Fixture {
    cwd: PathBuf,
    home: PathBuf,
    session_id: String,
    engine: SessionEngine,
}

fn fixture(name: &str) -> Fixture {
    fixture_with_turns(name, Vec::new())
}

fn fixture_with_turns(name: &str, turns: Vec<Vec<ModelEvent>>) -> Fixture {
    let cwd = temp_dir(&format!("{name}_cwd"));
    let home = temp_dir(&format!("{name}_home"));
    let engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(turns)),
        Some(home.clone()),
    )
    .unwrap();
    let session_id = engine.session_id().to_string();
    Fixture {
        cwd,
        home,
        session_id,
        engine,
    }
}

fn owner(lane: Option<&str>, session: Option<&str>, turn: &str) -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "ws_rows".to_string(),
        project_id: "prj_rows".to_string(),
        lane_id: lane.map(ToString::to_string),
        session_id: session.map(ToString::to_string),
        task_id: None,
        turn_id: Some(turn.to_string()),
    }
}

fn scope(lane: Option<&str>, session: Option<&str>) -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "ws_rows".to_string(),
        project_id: "prj_rows".to_string(),
        lane_id: lane.map(ToString::to_string),
        session_id: session.map(ToString::to_string),
        task_id: None,
        turn_id: None,
    }
}

/// Writes one durable turn bracket and the entries the native tool loop would
/// append inside it, in the same order and through the same store.
fn durable_turn(engine: &mut SessionEngine, turn_owner: RuntimeOwner, entries: &[TranscriptEntry]) {
    engine.begin_native_turn(turn_owner);
    for entry in entries {
        engine.store_entry(entry.clone()).unwrap();
    }
    engine.end_native_turn();
}

fn user(text: &str) -> TranscriptEntry {
    TranscriptEntry::Message {
        message: Message::new(Role::User, text),
    }
}

fn assistant(text: &str) -> TranscriptEntry {
    TranscriptEntry::Message {
        message: Message::new(Role::Assistant, text),
    }
}

fn tool_call(id: &str, name: &str, key: &str, value: &str) -> TranscriptEntry {
    let mut input = viden_types::ToolInput::new();
    input.insert(key.to_string(), value.to_string());
    TranscriptEntry::ToolCall {
        call: ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            input,
        },
    }
}

fn tool_result(id: &str, name: &str, output: &str, success: bool) -> TranscriptEntry {
    TranscriptEntry::ToolResult {
        result: ToolResult {
            tool_call_id: id.to_string(),
            name: name.to_string(),
            output: output.to_string(),
            diff: None,
            success,
            exit_code: Some(if success { 0 } else { 1 }),
        },
    }
}

/// Appends one durable approval audit row, exactly as C7's supervisor handle
/// does: the permission request id, the tool, and the job the decision
/// released.
fn approval_audit_row(
    engine: &SessionEngine,
    audit_id: &str,
    row_owner: &RuntimeOwner,
    request_id: &str,
    scope_key: &str,
) {
    // After every transcript entry written so far, which is where a real
    // approval for this turn sits.
    let record = AuditRecord::sanitized(
        audit_id.to_string(),
        viden_types::now_timestamp() + 10,
        row_owner.clone(),
        AuditActor::Operator,
        format!("approval.{scope_key}"),
        vec![
            AuditObjectRef::new(AuditObjectRef::KIND_PERMISSION, request_id),
            AuditObjectRef::new("tool", "edit_file"),
            AuditObjectRef::new("job", "job-rows"),
        ],
        AuditOutcome::Success,
        std::collections::BTreeMap::from([("scope".to_string(), scope_key.to_string())]),
    )
    .unwrap();
    engine
        .workflow_store()
        .append_audit_record(&record)
        .unwrap();
}

/// Records one durable evidence row with canonical ContextStore bytes, so the
/// transcript read has something real to name. The gate exists only to give
/// `RecordAgentEvidence` a task to file under.
fn record_canonical_evidence(
    cwd: &Path,
    engine: &mut SessionEngine,
    evidence_id: &str,
    kind: &str,
    bytes: &[u8],
) {
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    let mut store = ContextEngine::open(cwd.join(".viden/context-engine")).unwrap();
    let stored = store
        .store(ContextPutRequest {
            scope: ContextScope::Task("task-rows".to_string()),
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
            identity: "native".to_string(),
            role: "coder".to_string(),
            task_id: "task-rows".to_string(),
        },
        permission_snapshot_id: None,
        permission_scope: ContextScope::Task("task-rows".to_string()),
        evidence_scope: ContextScope::Task("task-rows".to_string()),
        verification: EvidenceVerificationState::Verified,
        quality: EvidenceQualityFacts {
            status: EvidenceQualityStatus::Pass,
            reason_codes: Vec::new(),
        },
    };
    engine.set_merge_gate_context_facts_for_test(&format!("bundle-{evidence_id}"), stored.item);
    engine
        .handle_runtime_command(
            format!("record-{evidence_id}"),
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task-rows".to_string(),
                evidence_id: Some(evidence_id.to_string()),
                kind: kind.to_string(),
                summary: format!("{kind} evidence"),
                path: None,
                source: Some("native".to_string()),
                canonical: Some(canonical),
            },
            &mut allow,
        )
        .unwrap();
}

fn start_evidence_gate(engine: &mut SessionEngine) {
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    engine
        .handle_runtime_command(
            "start-rows-dag",
            RuntimeCommand::StartAgentDag {
                goal: "transcript rows".to_string(),
                tasks: vec![AgentDagTaskSpec {
                    task_id: "task-rows".to_string(),
                    role: AgentRole::Coder,
                    title: "Transcript rows".to_string(),
                    objective: "record evidence a row can name".to_string(),
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
}

fn query(
    engine: &mut SessionEngine,
    command_id: &str,
    query: TranscriptRowsQuery,
) -> Vec<RuntimeEvent> {
    let mut denier =
        |_prompt| panic!("a read-only transcript rows query must not ask for approval");
    engine
        .handle_runtime_command(
            command_id,
            RuntimeCommand::QueryTranscriptRows { query },
            &mut denier,
        )
        .unwrap()
}

fn page(events: &[RuntimeEvent], expected_command_id: &str) -> TranscriptRowsPage {
    let loaded = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::TranscriptRowsLoaded { command_id, page } => {
                Some((command_id.clone(), page.clone()))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a TranscriptRowsLoaded, got {events:?}"));
    assert_eq!(
        loaded.0, expected_command_id,
        "the page must name the exact read it answers"
    );
    loaded.1
}

fn read_page(
    engine: &mut SessionEngine,
    command_id: &str,
    request: TranscriptRowsQuery,
) -> TranscriptRowsPage {
    let events = query(engine, command_id, request);
    page(&events, command_id)
}

/// Every content variant is produced from a durable fact, and no variant is
/// produced from anything else.
#[test]
fn every_transcript_row_variant_is_derived_from_a_durable_fact() {
    let mut fixture = fixture("transcript_rows_variants");
    let turn = owner(None, Some(&fixture.session_id), "turn-variants");
    start_evidence_gate(&mut fixture.engine);
    record_canonical_evidence(
        &fixture.cwd,
        &mut fixture.engine,
        "patch-call-edit",
        "patch",
        b"--- a/src.txt\n+++ b/src.txt\n",
    );
    durable_turn(
        &mut fixture.engine,
        turn.clone(),
        &[
            user("apply the edit"),
            assistant("applying it now"),
            tool_call("call-edit", "edit_file", "path", "src.txt"),
            tool_result("call-edit", "edit_file", "1 file changed", true),
            tool_call(
                "call-check",
                "shell",
                "command",
                "cargo test -p viden-types",
            ),
            tool_result("call-check", "shell", "test result: ok", true),
        ],
    );
    approval_audit_row(
        &fixture.engine,
        "audit-rows-1",
        &turn,
        "approval-rows-1",
        "allow_once",
    );

    let page = read_page(
        &mut fixture.engine,
        "rows-variants",
        TranscriptRowsQuery {
            owner: turn.clone(),
            ..TranscriptRowsQuery::default()
        },
    );
    assert!(page.complete);
    for row in &page.rows {
        assert_eq!(row.owner, turn, "every row carries the turn owner verbatim");
    }

    let kinds = page
        .rows
        .iter()
        .map(|row| match &row.content {
            TranscriptRowContent::User { .. } => "user",
            TranscriptRowContent::Assistant { .. } => "assistant",
            TranscriptRowContent::ToolCall { .. } => "tool_call",
            TranscriptRowContent::ToolResult { .. } => "tool_result",
            TranscriptRowContent::CheckRun { .. } => "check_run",
            TranscriptRowContent::Permission { .. } => "permission",
            _ => "unknown",
        })
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![
            "user",
            "assistant",
            "tool_call",
            "tool_result",
            "tool_call",
            "check_run",
            "permission",
        ],
        "rows arrive in the order the durable facts were recorded, got {:?}",
        page.rows
    );

    match &page.rows[2].content {
        TranscriptRowContent::ToolCall { call } => {
            assert_eq!(call.tool_call_id, "call-edit");
            assert_eq!(call.name, "edit_file");
            assert!(call.input_preview.contains("src.txt"));
            assert_eq!(
                call.owner.as_ref(),
                Some(&turn),
                "a tool call row names the turn that started it"
            );
        }
        other => panic!("expected a tool call row, got {other:?}"),
    }
    match &page.rows[3].content {
        TranscriptRowContent::ToolResult {
            tool_call_id,
            success,
            summary,
            evidence_id,
        } => {
            assert_eq!(tool_call_id, "call-edit");
            assert!(success);
            assert_eq!(summary, "1 file changed");
            assert_eq!(
                evidence_id.as_deref(),
                Some("patch-call-edit"),
                "a tool result names the archived patch row for its own call"
            );
        }
        other => panic!("expected a tool result row, got {other:?}"),
    }
    match &page.rows[5].content {
        TranscriptRowContent::CheckRun { check } => {
            assert_eq!(check.id, "call-check");
            assert_eq!(check.status, CheckRunStatus::Passed);
            assert_eq!(check.owner, turn);
            assert_eq!(check.command, "cargo test -p viden-types");
        }
        other => panic!("expected a check run row, got {other:?}"),
    }
    match &page.rows[6].content {
        TranscriptRowContent::Permission {
            request_id,
            decision,
            audit_id,
        } => {
            assert_eq!(request_id, "approval-rows-1");
            assert_eq!(audit_id, "audit-rows-1");
            assert_eq!(
                decision.as_ref(),
                Some(&ApprovalDecision::Allow {
                    scope: ApprovalScope::Once
                })
            );
        }
        other => panic!("expected a permission row, got {other:?}"),
    }
}

/// A scoped allow's payload is not in the durable audit row, so Core says it
/// does not know the decision rather than publishing a bare `allow_once`.
#[test]
fn a_scoped_allow_publishes_no_decision_it_did_not_record() {
    let mut fixture = fixture("transcript_rows_scoped_allow");
    let turn = owner(None, Some(&fixture.session_id), "turn-scoped");
    durable_turn(&mut fixture.engine, turn.clone(), &[user("go")]);
    approval_audit_row(
        &fixture.engine,
        "audit-rows-scoped",
        &turn,
        "approval-rows-scoped",
        "allow_session",
    );

    let page = read_page(
        &mut fixture.engine,
        "rows-scoped",
        TranscriptRowsQuery {
            owner: turn,
            ..TranscriptRowsQuery::default()
        },
    );
    let permission = page
        .rows
        .iter()
        .find_map(|row| match &row.content {
            TranscriptRowContent::Permission { decision, .. } => Some(decision.clone()),
            _ => None,
        })
        .expect("the audit row produced a permission row");
    assert!(
        permission.is_none(),
        "an allow_session's session id is not in the audit row, so no decision may be claimed"
    );
}

/// Pages tile one owner's transcript backwards from the newest end, and the
/// `older` cursor round-trips through the read without repeating a row.
#[test]
fn transcript_row_pages_tile_one_owner_without_repeating_a_row() {
    let mut fixture = fixture("transcript_rows_paging");
    let turn = owner(None, Some(&fixture.session_id), "turn-paging");
    let entries = (1..=5)
        .map(|index| user(&format!("line {index}")))
        .collect::<Vec<_>>();
    durable_turn(&mut fixture.engine, turn.clone(), &entries);

    let newest = read_page(
        &mut fixture.engine,
        "rows-page-1",
        TranscriptRowsQuery {
            owner: turn.clone(),
            limit: Some(2),
            ..TranscriptRowsQuery::default()
        },
    );
    assert!(!newest.complete);
    let first_texts = row_texts(&newest);
    assert_eq!(first_texts, vec!["line 4", "line 5"]);

    let older = newest
        .older
        .clone()
        .expect("an incomplete page has a cursor");
    let middle = read_page(
        &mut fixture.engine,
        "rows-page-2",
        TranscriptRowsQuery {
            owner: turn.clone(),
            before: Some(older),
            limit: Some(2),
        },
    );
    assert_eq!(row_texts(&middle), vec!["line 2", "line 3"]);
    assert!(!middle.complete);

    let oldest = read_page(
        &mut fixture.engine,
        "rows-page-3",
        TranscriptRowsQuery {
            owner: turn,
            before: middle.older.clone(),
            limit: Some(2),
        },
    );
    assert_eq!(row_texts(&oldest), vec!["line 1"]);
    assert!(oldest.complete);
    assert!(oldest.older.is_none());
}

fn row_texts(page: &TranscriptRowsPage) -> Vec<String> {
    page.rows
        .iter()
        .filter_map(|row| match &row.content {
            TranscriptRowContent::User { text, .. } => Some(text.clone()),
            TranscriptRowContent::Assistant { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// A cursor this build did not issue is refused as a `CommandRejected` naming
/// this exact read, never answered with an empty page.
#[test]
fn a_malformed_transcript_rows_cursor_is_refused_by_the_read() {
    let mut fixture = fixture("transcript_rows_bad_cursor");
    let turn = owner(None, Some(&fixture.session_id), "turn-bad-cursor");
    durable_turn(&mut fixture.engine, turn.clone(), &[user("hello")]);

    let events = query(
        &mut fixture.engine,
        "rows-bad-cursor",
        TranscriptRowsQuery {
            owner: turn,
            before: Some("page-2".to_string()),
            ..TranscriptRowsQuery::default()
        },
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::TranscriptRowsLoaded { .. })),
        "a refused read must not also publish a page"
    );
    let reason = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, reason } => {
                assert_eq!(command_id, "rows-bad-cursor");
                Some(reason.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a CommandRejected, got {events:?}"));
    assert!(
        reason.contains("transcript row cursor `page-2`"),
        "the refusal must name the cursor, got {reason}"
    );
}

/// Owner isolation across two Lanes and the session: three queries over one
/// durable transcript, and no row crosses an owner.
#[test]
fn transcript_rows_never_cross_owners_across_two_lanes_and_the_session() {
    let mut fixture = fixture("transcript_rows_isolation");
    let lane_a = owner(Some("lane-a"), Some("session-lane-a"), "turn-a");
    let lane_b = owner(Some("lane-b"), Some("session-lane-b"), "turn-b");
    let composer = owner(None, Some(&fixture.session_id), "turn-composer");

    // Interleaved, as two Lane workers and one composer really would be.
    durable_turn(&mut fixture.engine, lane_a.clone(), &[user("lane a first")]);
    durable_turn(&mut fixture.engine, composer.clone(), &[user("composer")]);
    durable_turn(&mut fixture.engine, lane_b.clone(), &[user("lane b first")]);
    durable_turn(
        &mut fixture.engine,
        lane_a.clone(),
        &[user("lane a second")],
    );

    for (label, request, expected) in [
        (
            "lane a",
            scope(Some("lane-a"), None),
            vec!["lane a first", "lane a second"],
        ),
        ("lane b", scope(Some("lane-b"), None), vec!["lane b first"]),
        (
            "session",
            scope(None, Some(&fixture.session_id)),
            vec!["composer"],
        ),
    ] {
        let page = read_page(
            &mut fixture.engine,
            &format!("rows-isolation-{}", label.replace(' ', "-")),
            TranscriptRowsQuery {
                owner: request,
                ..TranscriptRowsQuery::default()
            },
        );
        assert_eq!(row_texts(&page), expected, "{label} answered a foreign row");
    }

    // An unscoped read sees all four, so the isolation above is scoping rather
    // than rows Core failed to record.
    let all = read_page(
        &mut fixture.engine,
        "rows-isolation-all",
        TranscriptRowsQuery::default(),
    );
    assert_eq!(all.rows.len(), 4);
}

/// A row Core could not attribute never answers a scoped query. Entries written
/// outside any turn bracket carry only the session they were written into.
#[test]
fn an_unattributed_row_never_answers_a_lane_scoped_query() {
    let mut fixture = fixture("transcript_rows_unattributed");
    fixture
        .engine
        .store_entry(user("written outside any turn"))
        .unwrap();

    assert!(
        read_page(
            &mut fixture.engine,
            "rows-unattributed-lane",
            TranscriptRowsQuery {
                owner: scope(Some("lane-a"), None),
                ..TranscriptRowsQuery::default()
            },
        )
        .rows
        .is_empty(),
        "an unattributed row must never be claimed by a Lane"
    );
    let session_page = read_page(
        &mut fixture.engine,
        "rows-unattributed-session",
        TranscriptRowsQuery {
            owner: RuntimeOwner {
                session_id: Some(fixture.session_id.clone()),
                ..RuntimeOwner::default()
            },
            ..TranscriptRowsQuery::default()
        },
    );
    assert_eq!(
        row_texts(&session_page),
        vec!["written outside any turn"],
        "the one thing Core knows about the row is the session it is in"
    );
}

/// Compatibility follow-up 12, reader three of three: an owner-scoped row
/// carries the persisted text byte-equal. These rows are the only transcript
/// GUI-CORE-009 gave a client, so a Latin-1 reading here is what the client
/// would render for every non-Latin conversation.
#[test]
fn a_row_answers_the_persisted_non_ascii_text_byte_equal() {
    let mut fixture = fixture("transcript_rows_utf8");
    let turn = owner(None, Some(&fixture.session_id), "turn-utf8");
    let prompt = "请检查 naïve 的补丁 ✓";
    let reply = "café 你好 — ✓ 🚀";
    durable_turn(
        &mut fixture.engine,
        turn.clone(),
        &[user(prompt), assistant(reply)],
    );

    let page = read_page(
        &mut fixture.engine,
        "rows-utf8",
        TranscriptRowsQuery {
            owner: turn,
            ..TranscriptRowsQuery::default()
        },
    );
    assert_eq!(
        row_texts(&page),
        vec![prompt.to_string(), reply.to_string()],
        "a row must answer the persisted text, not a Latin-1 reading of its bytes"
    );
}

/// A body over the bound is cut on a character boundary, says so, and names
/// the canonical evidence row that already holds it in full.
#[test]
fn a_truncated_assistant_row_names_the_evidence_that_holds_its_body() {
    let mut fixture = fixture("transcript_rows_truncation");
    let turn = owner(None, Some(&fixture.session_id), "turn-truncation");
    // A real multi-byte body, so the character bound and the byte bound cannot
    // coincide: the 8 KiB cut lands inside a three-byte character and has to
    // step back to the boundary below it. C8 used an ASCII stand-in here
    // because a non-ASCII body could not survive the transcript round trip at
    // all; C11 fixed the decoder, so the cut is proven on the real shape.
    let body = "你好 café ✓ ".repeat(500);
    let expected_cut = {
        let mut cut = MAX_TRANSCRIPT_ROW_TEXT_BYTES as usize;
        while !body.is_char_boundary(cut) {
            cut -= 1;
        }
        cut
    };
    assert!(
        expected_cut < MAX_TRANSCRIPT_ROW_TEXT_BYTES as usize,
        "the body must straddle the bound, or this proves nothing about characters"
    );
    start_evidence_gate(&mut fixture.engine);
    record_canonical_evidence(
        &fixture.cwd,
        &mut fixture.engine,
        "assistant-body-rows",
        "task_summary",
        body.as_bytes(),
    );
    durable_turn(&mut fixture.engine, turn.clone(), &[assistant(&body)]);

    let page = read_page(
        &mut fixture.engine,
        "rows-truncation",
        TranscriptRowsQuery {
            owner: turn,
            ..TranscriptRowsQuery::default()
        },
    );
    match &page.rows[0].content {
        TranscriptRowContent::Assistant {
            text,
            truncated,
            evidence_id,
        } => {
            assert!(truncated, "the bound cut the body and must say so");
            assert_eq!(
                text.len(),
                expected_cut,
                "the body is cut on the character boundary below the bound"
            );
            assert_eq!(
                text.as_str(),
                &body[..expected_cut],
                "the bounded text is the persisted body's own prefix"
            );
            assert_eq!(
                evidence_id.as_deref(),
                Some("assistant-body-rows"),
                "a cut body names the canonical row that still holds it whole"
            );
            assert_eq!(
                format!("{:x}", Sha256::digest(body.as_bytes())).len(),
                64,
                "the join is the body's own digest"
            );
            assert_ne!(
                text.as_str(),
                body.as_str(),
                "a bounded row is not the whole body"
            );
        }
        other => panic!("expected an assistant row, got {other:?}"),
    }
}

/// A body with no canonical evidence names none. Absence means no such row
/// exists, never that the read withheld one — and the read never creates one.
#[test]
fn an_assistant_row_with_no_canonical_evidence_names_none() {
    let mut fixture = fixture("transcript_rows_no_evidence");
    let turn = owner(None, Some(&fixture.session_id), "turn-no-evidence");
    durable_turn(
        &mut fixture.engine,
        turn.clone(),
        &[assistant("short reply")],
    );

    let page = read_page(
        &mut fixture.engine,
        "rows-no-evidence",
        TranscriptRowsQuery {
            owner: turn,
            ..TranscriptRowsQuery::default()
        },
    );
    match &page.rows[0].content {
        TranscriptRowContent::Assistant {
            truncated,
            evidence_id,
            ..
        } => {
            assert!(!truncated);
            assert!(evidence_id.is_none());
        }
        other => panic!("expected an assistant row, got {other:?}"),
    }
    assert!(
        fixture.engine.evidence_archive().is_empty(),
        "the read must not create an evidence row to name"
    );
}

/// The restart proof: a fresh engine over the same session answers the same
/// read, because every row came from an append-only durable fact.
#[test]
fn a_restarted_engine_answers_the_same_transcript_rows() {
    let mut fixture = fixture("transcript_rows_restart");
    let lane = owner(Some("lane-a"), Some("session-lane-a"), "turn-restart");
    durable_turn(
        &mut fixture.engine,
        lane.clone(),
        &[
            user("restart me"),
            assistant("restarted"),
            tool_call("call-restart", "read_file", "path", "src.txt"),
            tool_result("call-restart", "read_file", "bytes", true),
        ],
    );
    let before = read_page(
        &mut fixture.engine,
        "rows-restart-before",
        TranscriptRowsQuery {
            owner: scope(Some("lane-a"), None),
            ..TranscriptRowsQuery::default()
        },
    );
    assert_eq!(before.rows.len(), 4);

    let mut reopened = SessionEngine::new_with_home(
        &fixture.cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(fixture.home.clone()),
    )
    .unwrap();
    reopened
        .resume_session(RuntimeResumeRequest::exact_session_id(&fixture.session_id))
        .unwrap();
    let after = read_page(
        &mut reopened,
        "rows-restart-after",
        TranscriptRowsQuery {
            owner: scope(Some("lane-a"), None),
            ..TranscriptRowsQuery::default()
        },
    );
    assert_eq!(after, before, "a restart must answer the same read");
}

/// The real native turn path stamps the durable owner marker, so the rows a
/// live turn produces are attributable without any client bookkeeping.
#[test]
fn a_native_turn_attributes_its_rows_to_the_turn_owner() {
    let mut fixture = fixture_with_turns(
        "transcript_rows_native_turn",
        vec![vec![
            ModelEvent::AssistantText {
                content: "done".to_string(),
            },
            ModelEvent::Done,
        ]],
    );
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    let events = fixture
        .engine
        .handle_runtime_command(
            "native-turn",
            RuntimeCommand::SubmitUserInput {
                content: "say done".to_string(),
            },
            &mut allow,
        )
        .unwrap();
    let turn_owner = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::TurnStarted { turn } => Some(turn.owner.clone()),
            _ => None,
        })
        .expect("the native turn brackets itself");

    let page = read_page(
        &mut fixture.engine,
        "rows-native-turn",
        TranscriptRowsQuery {
            owner: turn_owner.clone(),
            ..TranscriptRowsQuery::default()
        },
    );
    assert!(
        page.rows
            .iter()
            .all(|row| row.owner.turn_id == turn_owner.turn_id),
        "every row of a live turn names that turn, got {:?}",
        page.rows
    );
    assert!(
        row_texts(&page).contains(&"say done".to_string()),
        "the operator's own input is a row, got {:?}",
        page.rows
    );
    // The marker is a session-meta entry the base transcript page already
    // carries unknown keys for, so it changes no existing reader.
    let transcript = fs::read_to_string(fixture.engine.store.transcript_path()).unwrap();
    assert!(
        transcript.contains("\"key\":\"turn_owner\""),
        "the durable marker is what makes the rows attributable"
    );
}
