//! Emits the exact screen projections used for visual evidence.
//!
//! The captured pixels must be what Core facts actually produce, so the
//! harness never hand-writes a projection literal. This test runs the real
//! GUI projection over the canonical `frontend-contract-v1` fixtures and
//! serializes the result next to the capture page.
//!
//! Run explicitly, because it writes files:
//!
//! ```text
//! cargo test -p viden-gui --test capture_projections -- --ignored
//! ```
//!
//! Facts added on top of a fixture are listed in `EVIDENCE.md`; every such
//! addition uses the same typed Core record the runtime would publish.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use viden_core::{
    ApprovalScope, AuditActor, AuditCursor, AuditObjectRef, AuditOutcome, AuditPage, AuditRecord,
    ConflictBounce, ConflictBounceStatus, ContractDecision, ContractRecord, DependencyRecord,
    DependencyState, EventCursor, EvidenceView, FRONTEND_SCHEMA_V1, LaneRunStats, MergeGateStatus,
    ReplayBatch, RevertRecord, ReviewRequestRecord, ReviewRequestStatus, RuntimeEvent,
    RuntimeEventEnvelope, RuntimeEventKind, RuntimeOwner, RuntimeSnapshot, RuntimeViewState,
    RuntimeWireEvent,
};
use viden_gui::GuiCoreAdapter;

mod support;
use support::TestCoreClient;

const FIXTURE_ROOT: &str = "../../../crates/types/tests/fixtures/frontend-contract-v1";

fn output_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../evidence/gui-screen-restore/projections")
}

fn load(name: &str) -> RuntimeViewState {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE_ROOT)
        .join(name);
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    let fixture: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let snapshot: RuntimeSnapshot =
        serde_json::from_value(fixture["initial_snapshot"].clone()).unwrap();
    let mut view = RuntimeViewState::new(snapshot);
    if let Some(events) = fixture["events"].as_array() {
        for event in events {
            let envelope: RuntimeEventEnvelope = serde_json::from_value(event.clone()).unwrap();
            if let RuntimeWireEvent::Known(known) = envelope.event {
                view.apply_event(&known);
            }
        }
    }
    view
}

/// Reads the approval exactly as the fixture's event payload published it.
fn approval_from_fixture(name: &str) -> viden_core::ApprovalRequestView {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE_ROOT)
        .join(name);
    let fixture: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    serde_json::from_value(fixture["events"][0]["event"]["kind"]["payload"]["approval"].clone())
        .unwrap()
}

fn owner(lane: &str) -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace-viden".to_string(),
        project_id: "project-boss-rush".to_string(),
        lane_id: Some(lane.to_string()),
        session_id: Some(format!("session-{lane}")),
        task_id: Some(format!("task-{lane}")),
        turn_id: None,
    }
}

fn connected(view: RuntimeViewState) -> GuiCoreAdapter {
    let mut adapter = GuiCoreAdapter::new(Box::new(TestCoreClient::new(
        view,
        Arc::new(Mutex::new(Vec::new())),
    )));
    adapter.connect().unwrap();
    adapter
}

fn write(name: &str, value: &impl serde::Serialize) {
    let dir = output_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.json"));
    std::fs::write(&path, serde_json::to_string_pretty(value).unwrap()).unwrap();
    println!("wrote {}", path.display());
}

#[test]
#[ignore = "writes visual-evidence projections; run with --ignored"]
fn emit_capture_projections() {
    // D2: the approval fixture, plus one contract record and one pending
    // review so all three queue groups are represented.
    let mut d2 = load("approval-allow-deny.json");
    // The fixture's event stream resolves its approval, so the pending queue
    // ends empty. Replay the approval exactly as Core published it in the
    // event payload so the gate group shows the real request.
    let mut approval = approval_from_fixture("approval-allow-deny.json");
    approval.owner = owner("lane-gate");
    approval.allowed_scopes = vec![
        ApprovalScope::Once,
        ApprovalScope::Session {
            session_id: "session-lane-gate".to_string(),
        },
    ];
    d2.pending_approvals.push(approval);
    d2.contracts.push(ContractRecord {
        contract_id: "contract-feel-v1-1".to_string(),
        task_id: "task-lane-contract".to_string(),
        owner: owner("lane-contract"),
        summary: "feel param ranges v1.0 -> v1.1".to_string(),
        decision: ContractDecision::Confirmed,
        audit_id: "audit-contract".to_string(),
        updated_at: 1_700_000_100,
    });
    d2.review_requests.push(ReviewRequestRecord {
        review_id: "review-jump-feel".to_string(),
        gate_id: "gate-integration".to_string(),
        task_id: "task-lane-review".to_string(),
        requester_lane_id: "lane-review".to_string(),
        reviewer_lane_id: "lane-owner".to_string(),
        owner: owner("lane-review"),
        evidence_ids: vec!["evidence-playtest".to_string()],
        evidence_bindings: Vec::new(),
        status: ReviewRequestStatus::Pending,
        feedback: None,
        audit_id: "audit-review".to_string(),
        updated_at: 1_700_000_200,
    });
    d2.latest_evidence.push(EvidenceView {
        id: "evidence-playtest".to_string(),
        kind: "playtest".to_string(),
        summary: "jump feel validated across 12 replays".to_string(),
        path: Some("evidence/playtest.json".to_string()),
        source: Some("lane-gate".to_string()),
        canonical: None,
        metadata: None,
        timestamp: Some(1_700_000_150),
        owner: None,
    });
    // The queue after Core settled the review, captured before the pending
    // view is consumed. This is the state `decide_review` actually leaves
    // behind, so the confirmed capture can render a settled review instead of
    // a confirmed receipt sitting over a still-pending row.
    let mut d2_decided = d2.clone();
    let decided = d2_decided
        .review_requests
        .iter_mut()
        .find(|review| review.review_id == "review-jump-feel")
        .expect("the capture fixture carries the pending review");
    decided.status = ReviewRequestStatus::Accepted;
    decided.feedback = Some("Replay evidence matches the recorded bindings.".to_string());
    // Core stamps a fresh audit id on the decision; the capture must not keep
    // showing the request's own id as the decision's audit sink.
    decided.audit_id = "audit-review-decided".to_string();
    decided.updated_at = 1_700_000_300;

    let d2_adapter = connected(d2);
    write("d2", &d2_adapter.d2_decisions().unwrap());
    // The same queue with the pending review selected, so the review action bar
    // and its reviewer-note field are captured from a real Core review record
    // rather than a hand-written detail.
    write(
        "d2-review",
        &d2_adapter.d2_decisions_for("review-jump-feel").unwrap(),
    );
    write(
        "d2-review-decided",
        &connected(d2_decided)
            .d2_decisions_for("review-jump-feel")
            .unwrap(),
    );

    // D10: the multi-lane fixture with one lane bound to a project, so both
    // the bound and unbound rendering paths appear.
    let mut d10 = load("multi-lane.json");
    let bound = d10.lanes[0].id.clone();
    d10.lanes[0].status = viden_core::LaneStatus::WaitingApproval;
    // Bounded run facts on the fixture's cost-blind terminal lane, so the
    // capture shows the facts D10 renders in place of a cost figure Core
    // cannot produce.
    let blind = d10
        .lanes
        .iter()
        .position(|lane| lane.route == viden_core::AgentRoute::Terminal)
        .expect("the multi-lane fixture carries a terminal lane");
    d10.lanes[blind].run_stats = Some(LaneRunStats {
        wall_time_ms: 200_400,
        run_count: 3,
        diff_bytes: 8_192,
        last_exit_code: Some(0),
    });
    d10.lane_runtime_owners
        .push(viden_core::LaneRuntimeOwnerBinding {
            lane_id: bound.clone(),
            owner: owner(&bound),
        });
    write("d10", &connected(d10).d10_lane_monitor().unwrap());

    // D12: the merge-gate fixture with the bounce and revert that make the
    // recovery timeline and the post-merge rollback visible.
    let mut d12 = load("merge-gate.json");
    let gate_id = d12.merge_gates[0].gate_id.clone();
    d12.merge_gates[0].status = MergeGateStatus::NeedsChanges;
    d12.merge_gates[0].required_evidence = vec!["replay-regression".to_string()];
    d12.merge_gates[0].evidence_ids.clear();
    d12.conflict_bounces.push(ConflictBounce {
        bounce_id: "bounce-1".to_string(),
        gate_id: gate_id.clone(),
        task_id: "task-lane-3".to_string(),
        original_lane_id: "lane-3".to_string(),
        owner: owner("lane-3"),
        reason: "src/player/dash.gd conflicts with the merged baseline".to_string(),
        status: ConflictBounceStatus::Revalidated,
        evidence_ids: Vec::new(),
        baseline_evidence: Vec::new(),
        revalidation_evidence: Vec::new(),
        content: None,
        audit_id: "audit-bounce-1".to_string(),
        created_at: 1_700_000_700,
        revalidated_at: Some(1_700_000_800),
    });
    d12.reverts.push(RevertRecord {
        revert_id: "revert-1".to_string(),
        gate_id,
        applied_change_id: "change-1".to_string(),
        owner: owner("lane-3"),
        reason: "cancel window regressed after merge".to_string(),
        restored_paths: vec!["src/player/dash.gd".to_string()],
        audit_id: "audit-revert-1".to_string(),
        reverted_at: 1_700_000_900,
    });
    write("d12", &connected(d12).d12_integration_gate().unwrap());

    // D12 conflict content: the canonical `conflict-content.json` fixture,
    // whose Lane B bounce and Lane apply conflict both carry structured
    // content. The second hunk and the omitted/truncated variant are added as
    // Core's own wire form rather than as a client-side literal, because
    // `viden-core` re-exports `ConflictBounce` but not the `ConflictContent`
    // family it carries.
    let mut conflict = load("conflict-content.json");
    let bounce = conflict
        .conflict_bounces
        .iter_mut()
        .find(|bounce| bounce.bounce_id == "bounce_conflict_b")
        .expect("the conflict fixture publishes Lane B's bounce");
    // A second hunk with a different reason, so the capture shows that each
    // rejection carries its own classification and remedy.
    bounce.content = serde_json::from_value(serde_json::json!({
        "baseline": { "evidence": { "bindings": [{
            "evidence_id": "ev_conflict_patch_b",
            "source_hash": "cf".repeat(32)
        }] } },
        "files": [{
            "path": "crates/runtime/src/trust_loop.rs",
            "hunks": [
                {
                    "ours_start": 42,
                    "ours": ["    let bounce = record_conflict_bounce(gate)?;\n"],
                    "theirs_start": 42,
                    "theirs": ["    let bounce = bounce_with_reason(gate, reason)?;\n"],
                    "base": ["    let bounce = record_bounce(gate)?;\n"],
                    "reason": "context_mismatch"
                },
                {
                    "ours_start": 96,
                    "ours": [
                        "    let audit = audit.record(gate, actor)?;\n",
                        "    supervisor.publish(audit);\n"
                    ],
                    "theirs_start": 96,
                    "theirs": [
                        "    let audit = audit.record(gate, actor)?;\n",
                        "    supervisor.publish(audit);\n"
                    ],
                    "base": [
                        "    let audit = audit.record(gate)?;\n",
                        "    supervisor.publish(audit);\n"
                    ],
                    "reason": "already_applied"
                }
            ],
            "omitted": false
        }],
        "truncated": false
    }))
    .expect("Core's own conflict-content encoding");
    write(
        "d12-conflict",
        &connected(conflict.clone())
            .d12_integration_gate_for("gate_conflict_b")
            .unwrap(),
    );

    // The bounded variant: one file over the byte bound, so its entry survives
    // with no hunks, and the payload flags the truncation.
    let bounce = conflict
        .conflict_bounces
        .iter_mut()
        .find(|bounce| bounce.bounce_id == "bounce_conflict_b")
        .expect("the conflict fixture publishes Lane B's bounce");
    bounce.content = serde_json::from_value(serde_json::json!({
        "baseline": { "revision": { "sha": "9f".repeat(20) } },
        "files": [
            {
                "path": "crates/runtime/src/trust_loop.rs",
                "hunks": [{
                    "ours_start": 42,
                    "ours": ["    let bounce = record_conflict_bounce(gate)?;\n"],
                    "theirs_start": 42,
                    "theirs": ["    let bounce = bounce_with_reason(gate, reason)?;\n"],
                    "base": ["    let bounce = record_bounce(gate)?;\n"],
                    "reason": "context_mismatch"
                }],
                "omitted": false
            },
            { "path": "assets/atlas.png", "hunks": [], "omitted": true }
        ],
        "truncated": true
    }))
    .expect("Core's own conflict-content encoding");
    write(
        "d12-conflict-omitted",
        &connected(conflict)
            .d12_integration_gate_for("gate_conflict_b")
            .unwrap(),
    );

    // D13: the DAG fixture with one blocked dependency.
    let mut d13 = load("dag-blocker.json");
    let blocked = d13.agent_dags[0].tasks[0].task_id.clone();
    d13.dependencies.push(DependencyRecord {
        dependency_id: "dependency-1".to_string(),
        task_id: blocked,
        depends_on_task_id: "task-upstream".to_string(),
        owner: owner("lane-plan"),
        state: DependencyState::Blocked,
        reason: "waits for the upstream contract".to_string(),
        audit_id: "audit-dependency-1".to_string(),
        updated_at: 1_700_000_400,
    });
    write("d13", &connected(d13).d13_fleet_workflow().unwrap());

    // D14: the replay contract has no fixture, so the batch is built from
    // typed Core events and served through the same replay path.
    let mut event_one = RuntimeEvent::new(
        1,
        RuntimeEventKind::Error {
            error: viden_core::RuntimeErrorView {
                message: "provider unavailable".to_string(),
                recoverable: true,
                hint: Some("select another provider".to_string()),
            },
        },
    );
    event_one.timestamp = Some(1_700_000_001);
    let mut event_two = RuntimeEvent::new(
        2,
        RuntimeEventKind::AssistantDelta {
            message_id: "message-1".to_string(),
            task_id: None,
            session_id: None,
            content: "widening the cancel window".to_string(),
        },
    );
    event_two.timestamp = Some(1_700_000_002);
    let envelope = |event: RuntimeEvent, lane: &str| RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: owner(lane),
        cursor: EventCursor {
            stream_id: "core".to_string(),
            sequence: event.sequence,
        },
        event: RuntimeWireEvent::Known(event),
    };
    let unknown = RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: owner("lane-3"),
        cursor: EventCursor {
            stream_id: "core".to_string(),
            sequence: 3,
        },
        event: RuntimeWireEvent::Unknown {
            event_type: "core.future_fact".to_string(),
            payload: serde_json::Value::Null,
        },
    };
    let client = TestCoreClient::new(load("multi-lane.json"), Arc::new(Mutex::new(Vec::new())))
        .with_replay_batch(ReplayBatch {
            events: vec![
                envelope(event_one, "lane-1"),
                envelope(event_two, "lane-2"),
                unknown,
            ],
            next: EventCursor {
                stream_id: "core".to_string(),
                sequence: 3,
            },
            complete: false,
        });
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().unwrap();
    write("d14-raw", &adapter.d14_audit_timeline(None, 200).unwrap());

    // D14 audit mode: the real acceptance-first read over a Core page. Audit
    // records are their own append-only store rather than view state, so the
    // page is built from typed `AuditRecord` values and delivered through the
    // ordered event stream the production correlation machine consumes.
    let audit_page = AuditPage {
        records: vec![
            audit_record(
                "audit-gate-decided",
                1_700_000_900,
                AuditActor::Operator,
                "gate.decided",
                vec![
                    AuditObjectRef::new(AuditObjectRef::KIND_MERGE_GATE, "gate-integration"),
                    AuditObjectRef::new(AuditObjectRef::KIND_TASK, "task-lane-3"),
                ],
                AuditOutcome::Success,
                &[("outcome", "accepted")],
            ),
            audit_record(
                "audit-review-decided",
                1_700_000_800,
                AuditActor::Operator,
                "review.decided",
                vec![
                    AuditObjectRef::new(AuditObjectRef::KIND_REVIEW_REQUEST, "review-jump-feel"),
                    AuditObjectRef::new(AuditObjectRef::KIND_MERGE_GATE, "gate-integration"),
                ],
                AuditOutcome::Success,
                &[("verdict", "accepted")],
            ),
            audit_record(
                "audit-evidence-rejected",
                1_700_000_700,
                AuditActor::Agent {
                    agent_id: "codex-acp".to_string(),
                },
                "evidence.rejected",
                vec![AuditObjectRef::new(
                    AuditObjectRef::KIND_EVIDENCE,
                    "replay-regression",
                )],
                AuditOutcome::Denied,
                &[("outcome", "rejected")],
            ),
        ],
        next_before: Some(AuditCursor {
            timestamp: 1_700_000_700,
            audit_id: "audit-evidence-rejected".to_string(),
        }),
        complete: false,
    };
    let mut adapter = GuiCoreAdapter::new(Box::new(
        TestCoreClient::new(load("multi-lane.json"), Arc::new(Mutex::new(Vec::new())))
            .with_envelope(audit_envelope(
                1,
                RuntimeEventKind::CommandAccepted {
                    command_id: "gui-audit-capture".to_string(),
                    command: viden_core::RuntimeCommand::QueryAudit {
                        query: Default::default(),
                    },
                },
            ))
            .with_envelope(audit_envelope(
                2,
                RuntimeEventKind::AuditPageLoaded {
                    command_id: Some("gui-audit-capture".to_string()),
                    page: audit_page,
                },
            )),
    ));
    adapter.connect().unwrap();
    write(
        "d14-audit",
        &adapter
            .query_audit_and_wait(
                "gui-audit-capture",
                None,
                std::time::Duration::from_millis(10),
            )
            .unwrap(),
    );
}

fn audit_record(
    audit_id: &str,
    timestamp: u64,
    actor: AuditActor,
    action: &str,
    objects: Vec<AuditObjectRef>,
    outcome: AuditOutcome,
    args: &[(&str, &str)],
) -> AuditRecord {
    AuditRecord::sanitized(
        audit_id.to_string(),
        timestamp,
        owner("lane-3"),
        actor,
        action.to_string(),
        objects,
        outcome,
        args.iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect(),
    )
    .expect("the capture must use records Core itself would accept")
}

fn audit_envelope(sequence: u64, kind: RuntimeEventKind) -> RuntimeEventEnvelope {
    RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: RuntimeOwner::default(),
        cursor: EventCursor {
            stream_id: "core".to_string(),
            sequence,
        },
        event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
            sequence,
            Some(1_700_000_000 + sequence),
            kind,
        )),
    }
}
