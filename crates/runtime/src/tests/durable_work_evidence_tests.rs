//! Durable work evidence for supervised and adapter-driven mutations
//! (`runtime.durable_work_evidence`, C7, closes GUI-CORE-028 and E1 defect 4).
//!
//! The `0.3.3` real task reached an approved, typed, applied edit and then
//! found nothing to review: the archive was empty and the approval's audit id
//! resolved to no row. Four separate claims have to hold for that to stop being
//! true, and each has a test here.
//!
//! 1. An applied native `edit_file`/`write_file` publishes an archived `patch`
//!    row beside the live workspace change, with canonical ContextStore bytes,
//!    the turn's owner, and the audit id of the approval that allowed it.
//! 2. Those facts survive the process. The proof is a restart: a fresh engine
//!    over the same workspace rebuilds the row from the durable projection and
//!    serves its bytes.
//! 3. An approval decision is a durable audit row under the id the request
//!    already showed the operator.
//! 4. A merge gate accepts such a row only when it names the gate's own task,
//!    so a session-scoped composer edit can never satisfy a Lane's gate.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use viden_types::{
    ApprovalResponse, AuditQuery, EvidenceContent, EvidenceQuery, EvidenceView, ModelEvent,
    RuntimeCommand, RuntimeEvent, RuntimeEventKind, RuntimeOwner, ToolCall, ToolInput,
};

use super::{SequenceProvider, temp_dir};
use crate::{RuntimeResumeRequest, RuntimeSupervisor, SessionEngine, mint_workspace_owner_binding};

fn edit_tool_call(path: &str, old: &str, new: &str) -> ToolCall {
    let mut input = ToolInput::new();
    input.insert("path".to_string(), path.to_string());
    input.insert("old".to_string(), old.to_string());
    input.insert("new".to_string(), new.to_string());
    ToolCall {
        id: "tool_edit_evidence".to_string(),
        name: "edit_file".to_string(),
        input,
    }
}

fn bound_engine(
    cwd: &Path,
    home: &Path,
    provider: Box<dyn viden_provider::ModelProvider>,
) -> SessionEngine {
    let mut engine = SessionEngine::new_with_home(cwd, provider, Some(home.to_path_buf())).unwrap();
    engine.bind_workspace_owner(mint_workspace_owner_binding(cwd).unwrap());
    engine
}

fn collect_until(
    supervisor: &RuntimeSupervisor,
    done: impl Fn(&[RuntimeEvent]) -> bool,
) -> Vec<RuntimeEvent> {
    let started = std::time::Instant::now();
    let mut events = Vec::new();
    while started.elapsed() < Duration::from_secs(20) {
        if let Some(event) = supervisor.recv_event_timeout(Duration::from_millis(50)) {
            events.push(event);
            if done(&events) {
                return events;
            }
        }
    }
    panic!("timed out waiting for runtime events; collected: {events:#?}");
}

/// The request id, the pre-minted audit id, and the owner the request was
/// published under. The owner matters: an approval is answered by the scope
/// that was asked, and C5's workspace identity is stamped onto the turn's owner
/// before the prompt is built.
fn approval_identity(events: &[RuntimeEvent]) -> (String, String, RuntimeOwner) {
    events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::ApprovalRequested { approval } => Some((
                approval.id.clone(),
                approval.audit_id.clone(),
                approval.owner.clone(),
            )),
            _ => None,
        })
        .expect("an approval was requested")
}

fn patch_rows(events: &[RuntimeEvent]) -> Vec<EvidenceView> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::EvidenceRecorded { evidence } if evidence.kind == "patch" => {
                Some(evidence.clone())
            }
            _ => None,
        })
        .collect()
}

/// Runs one supervised turn that edits a file behind an approval the operator
/// allows, and returns everything needed to inspect what it left behind.
struct AppliedEdit {
    cwd: PathBuf,
    home: PathBuf,
    session_id: String,
    events: Vec<RuntimeEvent>,
    audit_id: String,
}

fn supervised_applied_edit(name: &str, body: &str, replacement: &str) -> AppliedEdit {
    let cwd = temp_dir(&format!("{name}_cwd"));
    let home = temp_dir(&format!("{name}_home"));
    fs::write(cwd.join("src.txt"), body).unwrap();
    let provider = Box::new(SequenceProvider::new(vec![
        vec![ModelEvent::ToolCall(edit_tool_call(
            "src.txt",
            body.trim_end(),
            replacement,
        ))],
        vec![ModelEvent::AssistantText {
            content: "applied".to_string(),
        }],
    ]));
    let engine = bound_engine(&cwd, &home, provider);
    let session_id = engine.session_id().to_string();
    let supervisor = RuntimeSupervisor::start(engine);

    supervisor
        .send_command(
            "cmd_apply_edit",
            RuntimeCommand::SubmitUserInput {
                content: "edit the file".to_string(),
            },
        )
        .unwrap();
    let mut events = collect_until(&supervisor, |events| {
        events
            .iter()
            .any(|event| matches!(&event.kind, RuntimeEventKind::ApprovalRequested { .. }))
    });
    let (request_id, audit_id, approval_owner) = approval_identity(&events);
    supervisor
        .send_command_from_owner(
            approval_owner,
            "cmd_allow_edit",
            RuntimeCommand::RespondToApproval {
                request_id,
                response: ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    events.extend(collect_until(&supervisor, |events| {
        events
            .iter()
            .any(|event| matches!(&event.kind, RuntimeEventKind::TurnFinished { .. }))
    }));
    drop(supervisor);
    AppliedEdit {
        cwd,
        home,
        session_id,
        events,
        audit_id,
    }
}

fn reopened_engine(applied: &AppliedEdit) -> SessionEngine {
    let mut engine = SessionEngine::new_with_home(
        &applied.cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(applied.home.clone()),
    )
    .unwrap();
    engine
        .resume_session(RuntimeResumeRequest::exact_session_id(&applied.session_id))
        .unwrap();
    engine
}

/// An applied native mutation archives a `patch` row beside the live workspace
/// change, with canonical bytes, the turn's owner, and the approval receipt.
///
/// Before C7 this turn published `WorkspaceChangeUpdated` and nothing else: a
/// cockpit could render the change and a reviewer could never read it back.
#[test]
fn an_applied_native_edit_publishes_a_patch_row_with_canonical_bytes() {
    let applied = supervised_applied_edit(
        "durable_work_evidence_applied",
        "alpha\nbeta\ngamma\n",
        "BETA",
    );

    let rows = patch_rows(&applied.events);
    assert_eq!(rows.len(), 1, "exactly one patch row for one applied edit");
    let row = &rows[0];
    assert_eq!(row.id, "patch-tool_edit_evidence");
    assert_eq!(row.path.as_deref(), Some("src.txt"));
    assert_eq!(row.source.as_deref(), Some("native"));

    // The live change and the archived row describe the same edit, in that
    // order: the cockpit fact first, the reviewable artifact after it.
    let change_index = applied
        .events
        .iter()
        .position(|event| {
            matches!(
                &event.kind,
                RuntimeEventKind::WorkspaceChangeUpdated { change } if change.path == "src.txt"
            )
        })
        .expect("the live workspace change is published");
    let row_index = applied
        .events
        .iter()
        .position(|event| {
            matches!(&event.kind, RuntimeEventKind::EvidenceRecorded { evidence } if evidence.id == row.id)
        })
        .expect("the archived row is published");
    assert!(change_index < row_index);

    let owner = row.owner.as_ref().expect("the row names its turn owner");
    assert!(
        owner.turn_id.is_some(),
        "the archived row carries the turn id an audit row joins on, got {owner:?}"
    );
    assert_eq!(owner.lane_id, None);

    let canonical = row
        .canonical
        .as_ref()
        .expect("an applied edit under the byte bound keeps canonical bytes");
    assert_eq!(canonical.producer.identity, "native");
    assert_eq!(canonical.producer.role, "coder");
    assert_eq!(
        Some(canonical.producer.task_id.as_str()),
        owner.turn_id.as_deref(),
        "a session-scoped turn names its turn as the producer task"
    );
    assert_eq!(
        canonical.permission_snapshot_id.as_deref(),
        Some(applied.audit_id.as_str()),
        "the patch names the audit id of the approval that allowed it"
    );
    assert_eq!(
        canonical.verification,
        viden_types::EvidenceVerificationState::Verified
    );

    // The canonicalization is announced beside the row, so a client that has
    // the row also learns its bytes are servable.
    assert!(applied.events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::EvidenceCanonicalized { evidence_id, content_sha256, .. }
            if evidence_id == &row.id && content_sha256 == &canonical.source_hash
    )));
}

/// The restart proof. A fresh engine over the same workspace rebuilds the row
/// from the durable `runtime_projection` rows and serves the bytes behind it.
///
/// This is the claim `RuntimeSupervisor` could not make before: its turns
/// published to the live bus and persisted nothing, so every archive read after
/// a restart answered empty.
#[test]
fn a_restarted_engine_rebuilds_the_archived_patch_and_serves_its_diff() {
    let applied = supervised_applied_edit(
        "durable_work_evidence_restart",
        "alpha\nbeta\ngamma\n",
        "BETA",
    );
    let expected = applied.events[..]
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::EvidenceRecorded { evidence } if evidence.kind == "patch" => {
                Some(evidence.clone())
            }
            _ => None,
        })
        .expect("the live turn archived a patch row");

    let engine = reopened_engine(&applied);
    let archived = engine
        .evidence_archive()
        .iter()
        .find(|entry| entry.id == expected.id)
        .cloned()
        .expect("the restarted engine rebuilds the patch row from the durable projection");
    assert_eq!(archived.canonical, expected.canonical);

    let page = engine
        .query_evidence(
            "cmd_query_evidence",
            EvidenceQuery {
                kinds: vec!["patch".to_string()],
                ..EvidenceQuery::default()
            },
        )
        .unwrap();
    let entries = page
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::EvidencePageLoaded { page, .. } => Some(page.entries.clone()),
            _ => None,
        })
        .expect("the archive answers the page");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, expected.id);

    let content = engine
        .read_evidence_content("cmd_read_evidence", &expected.id)
        .unwrap()
        .into_iter()
        .find_map(|event| match event.kind {
            RuntimeEventKind::EvidenceContentLoaded { content, .. } => Some(content),
            _ => None,
        })
        .expect("the archive answers the content");
    let EvidenceContent::Diff { document, sha256 } = content else {
        panic!("a patch row's canonical bytes are served as a diff, got {content:?}");
    };
    assert_eq!(
        sha256,
        expected.canonical.as_ref().unwrap().source_hash,
        "the served bytes verified against the hash the row published"
    );
    assert!(
        document
            .files
            .iter()
            .any(|file| file.additions > 0 || file.deletions > 0),
        "the served diff carries the change's rows, got {document:?}"
    );
}

/// A diff over the canonical evidence bound is archived without bytes and the
/// summary says so. A silently dropped `canonical` would be read as
/// display-only evidence, which is a different fact.
#[test]
fn an_oversized_diff_is_archived_without_canonical_bytes_and_says_why() {
    let cwd = temp_dir("durable_work_evidence_oversized_cwd");
    let owner = RuntimeOwner {
        workspace_id: "ws_oversized".to_string(),
        project_id: "prj_oversized".to_string(),
        lane_id: None,
        session_id: None,
        task_id: None,
        turn_id: Some("turn_oversized".to_string()),
    };
    let mut diff = String::from("--- a/big.txt\n+++ b/big.txt\n");
    while diff.len() <= viden_types::MAX_EVIDENCE_CONTENT_BYTES as usize {
        diff.push_str("+a line that is long enough to get there quickly\n");
    }
    let evidence = crate::work_evidence::native_patch_evidence(
        &cwd,
        crate::work_evidence::NativePatchEvidenceInput {
            owner: &owner,
            tool_call_id: "tool_big",
            path: "big.txt",
            diff: &diff,
            bundle_id: "bundle_oversized",
            permission_snapshot_id: Some("audit_oversized".to_string()),
        },
    );
    assert_eq!(evidence.id, "patch-tool_big");
    assert!(evidence.canonical.is_none());
    assert!(
        evidence.summary.contains("no canonical bytes"),
        "the summary must say why there are none, got `{}`",
        evidence.summary
    );
    assert!(evidence.summary.contains("over the"));
}

/// An approval decision is a durable audit row, under the id the request
/// already showed the operator.
///
/// The id was minted and published on `ApprovalRequested` and `ApprovalResolved`
/// long before C7 and was written nowhere, so every cockpit "audit" link for an
/// approval resolved to nothing (E1 defect 4).
#[test]
fn an_allowed_approval_is_a_durable_audit_row_under_its_published_id() {
    let applied = supervised_applied_edit("durable_work_evidence_audit", "alpha\nbeta\n", "BETA");
    let engine = reopened_engine(&applied);

    let page = engine
        .workflow_store()
        .query_audit(&AuditQuery {
            limit: 100,
            ..AuditQuery::default()
        })
        .unwrap();
    let record = page
        .records
        .iter()
        .find(|record| record.audit_id == applied.audit_id)
        .unwrap_or_else(|| {
            panic!(
                "the approval's published audit id must name a durable row; got {:?}",
                page.records
            )
        });
    assert_eq!(record.action, "approval.allow_once");
    assert_eq!(record.actor, viden_types::AuditActor::Operator);
    assert_eq!(record.outcome, viden_types::AuditOutcome::Success);
    assert_eq!(
        record.args.get("scope").map(String::as_str),
        Some("allow_once")
    );
    assert!(
        record
            .objects
            .iter()
            .any(|object| object.kind == "permission"),
        "the row names the approval request it decided, got {:?}",
        record.objects
    );
    assert!(
        record
            .objects
            .iter()
            .any(|object| object.kind == "tool" && object.id == "edit_file"),
        "the row names what was allowed, got {:?}",
        record.objects
    );

    // The `ApprovalResolved` a client saw carries the same id, so the join a
    // cockpit makes is the one the archive answers.
    assert!(applied.events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::ApprovalResolved { audit_id, .. } if audit_id == &applied.audit_id
    )));
}

/// The merge-gate rule. A patch row satisfies a `patch`-required gate only when
/// its producer names that gate's task; a session-scoped turn's row names its
/// turn instead and the gate says which reason blocked it.
#[test]
fn a_patch_row_satisfies_a_gate_only_when_its_producer_names_that_task() {
    let cwd = temp_dir("durable_work_evidence_gate_cwd");
    let lane_owner = RuntimeOwner {
        workspace_id: "ws_gate".to_string(),
        project_id: "prj_gate".to_string(),
        lane_id: Some("lane-gate".to_string()),
        session_id: Some("session-gate".to_string()),
        task_id: Some("task-gate".to_string()),
        turn_id: Some("turn_lane".to_string()),
    };
    let session_owner = RuntimeOwner {
        lane_id: None,
        task_id: None,
        turn_id: Some("turn_session".to_string()),
        ..lane_owner.clone()
    };
    let diff = "--- a/src.txt\n+++ b/src.txt\n@@ -1 +1 @@\n-beta\n+BETA\n";

    let lane_row = crate::work_evidence::native_patch_evidence(
        &cwd,
        crate::work_evidence::NativePatchEvidenceInput {
            owner: &lane_owner,
            tool_call_id: "tool_lane",
            path: "src.txt",
            diff,
            bundle_id: "bundle-gate",
            permission_snapshot_id: Some("audit_lane".to_string()),
        },
    );
    let session_row = crate::work_evidence::native_patch_evidence(
        &cwd,
        crate::work_evidence::NativePatchEvidenceInput {
            owner: &session_owner,
            tool_call_id: "tool_session",
            path: "src.txt",
            diff,
            bundle_id: "bundle-gate",
            permission_snapshot_id: Some("audit_session".to_string()),
        },
    );

    assert_eq!(
        lane_row.canonical.as_ref().unwrap().producer.task_id,
        "task-gate",
        "a Lane turn bound to a task names that task as the producer"
    );
    assert_eq!(
        session_row.canonical.as_ref().unwrap().producer.task_id,
        "turn_session",
        "a session-scoped turn names no task, so it names its turn"
    );

    let gate = gate_for_test("task-gate");
    let report = crate::runtime_contract::merge_gate_report_for_test(
        &cwd,
        &gate,
        &["bundle-gate"],
        std::slice::from_ref(&lane_row),
    );
    assert_eq!(
        report.status,
        viden_types::EvidenceCanonicalStatus::Verified,
        "a Lane's patch row naming the gate's task satisfies it, got {report:?}"
    );

    let refused = crate::runtime_contract::merge_gate_report_for_test(
        &cwd,
        &gate,
        &["bundle-gate"],
        std::slice::from_ref(&session_row),
    );
    assert_eq!(
        refused.status,
        viden_types::EvidenceCanonicalStatus::Blocked
    );
    assert!(
        refused
            .reason_codes
            .contains(&viden_types::EvidenceCanonicalReasonCode::MissingProducer),
        "the gate names why a session-scoped row does not count, got {refused:?}"
    );
}

/// The ingestion of an adapter-reported patch. `viden-agents` builds the fact
/// with `canonical: None` and the diff in `metadata`; the runtime stores those
/// bytes and publishes `EvidenceCanonicalized` immediately after the row it
/// completes.
#[test]
fn an_agent_patch_fact_is_canonicalized_at_ingestion() {
    let cwd = temp_dir("durable_work_evidence_agent_cwd");
    let diff = "--- a/src.txt\n+++ b/src.txt\n@@ -1 +1 @@\n-beta\n+BETA\n";
    let mut events = vec![
        RuntimeEvent::new(
            1,
            RuntimeEventKind::EvidenceRecorded {
                evidence: EvidenceView {
                    id: "acp-patch-tool-1".to_string(),
                    kind: "patch".to_string(),
                    summary: "ACP patch".to_string(),
                    path: Some("src.txt".to_string()),
                    source: Some("acp:patch.v1".to_string()),
                    canonical: None,
                    metadata: Some(serde_json::json!({
                        "schema": "acp.patch.v1",
                        "diff": diff,
                    })),
                    timestamp: None,
                    owner: None,
                },
            },
        ),
        RuntimeEvent::new(
            2,
            RuntimeEventKind::MergeGateUpdated {
                gate: gate_for_test("acp-session-s1"),
            },
        ),
    ];

    let completed = crate::work_evidence::canonicalize_agent_patch_evidence(&cwd, &mut events);
    assert_eq!(completed.len(), 2, "the completed row and its announcement");

    let RuntimeEventKind::EvidenceRecorded { evidence } = &events[0].kind else {
        panic!("the patch row stays first");
    };
    let canonical = evidence
        .canonical
        .as_ref()
        .expect("the runtime completed the row the adapter could not");
    assert_eq!(canonical.producer.identity, "agent");
    assert_eq!(canonical.producer.task_id, "acp-session-s1");
    assert_eq!(
        canonical.source_hash,
        format!("{:x}", Sha256::digest(diff.as_bytes()))
    );
    assert_eq!(
        canonical.permission_snapshot_id, None,
        "an adapter patch carries no operator approval receipt, and must not invent one"
    );

    assert!(
        matches!(
            &events[1].kind,
            RuntimeEventKind::EvidenceCanonicalized { evidence_id, .. }
                if evidence_id == "acp-patch-tool-1"
        ),
        "the announcement lands immediately after the row it completes, got {:?}",
        events[1].kind
    );

    // Idempotent: a row that already carries canonical bytes is left alone, so
    // a replayed batch cannot mint a second store item for the same diff.
    let again = crate::work_evidence::canonicalize_agent_patch_evidence(&cwd, &mut events);
    assert!(again.is_empty());
}

fn gate_for_test(task_id: &str) -> viden_types::MergeGateRecord {
    viden_types::MergeGateRecord {
        gate_id: format!("gate-{task_id}"),
        task_id: task_id.to_string(),
        status: viden_types::MergeGateStatus::CollectingEvidence,
        required_evidence: vec!["patch".to_string()],
        evidence_ids: Vec::new(),
        gate_type: viden_types::MergeGateType::Artifact,
        owner: RuntimeOwner {
            task_id: Some(task_id.to_string()),
            ..RuntimeOwner::default()
        },
        validator: None,
        policy_snapshot: viden_types::MergeGatePolicySnapshot {
            required_evidence: vec!["patch".to_string()],
            permission_snapshot_id: None,
            requires_independent_validator: false,
            captured_at: None,
        },
        decision: None,
        conflict: None,
        updated_at: None,
        applied_change_id: None,
        audit_ids: Vec::new(),
        recovery_snapshot: None,
    }
}
