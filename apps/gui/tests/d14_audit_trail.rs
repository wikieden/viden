//! D14 audit mode: the Core audit contract as the primary D14 surface.
//!
//! These tests cover the correlation machine, the capability gate, paging, and
//! the projection's fallback labels. Correlation has three cases since
//! GUI-CORE-024: a page naming this read is applied, a page naming another read
//! is ignored, and a page with no id at all (a Core predating the field) falls
//! back to the acceptance-first rule. The raw replay-cursor mode keeps its own
//! coverage in `d14_audit_timeline.rs`; the two modes are deliberately separate
//! because they read two different Core surfaces.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use viden_core::{
    AuditActor, AuditCursor, AuditObjectRef, AuditOutcome, AuditPage, AuditRecord, EventCursor,
    FRONTEND_SCHEMA_V1, RuntimeCommand, RuntimeCommandEnvelope, RuntimeEvent, RuntimeEventEnvelope,
    RuntimeEventKind, RuntimeOwner, RuntimeSnapshot, RuntimeViewState, RuntimeWireEvent,
};
use viden_gui::{AUDIT_CAPABILITY, D14AuditScopeInput, GuiCoreAdapter};

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

fn record(audit_id: &str, timestamp: u64, actor: AuditActor, outcome: AuditOutcome) -> AuditRecord {
    AuditRecord {
        audit_id: audit_id.to_string(),
        timestamp,
        owner: RuntimeOwner::default(),
        actor,
        action: "gate.decided".to_string(),
        objects: vec![AuditObjectRef::new(
            AuditObjectRef::KIND_MERGE_GATE,
            "gate-1",
        )],
        outcome,
        args: BTreeMap::from([("outcome".to_string(), "accepted".to_string())]),
    }
}

fn page(records: Vec<AuditRecord>, next_before: Option<AuditCursor>) -> AuditPage {
    AuditPage {
        complete: next_before.is_none(),
        records,
        next_before,
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

fn accepted(sequence: u64, command_id: &str) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::CommandAccepted {
            command_id: command_id.to_string(),
            command: RuntimeCommand::QueryAudit {
                query: Default::default(),
            },
        },
    )
}

/// A page naming the exact read it answers, as Core publishes it since
/// GUI-CORE-024.
fn loaded(sequence: u64, command_id: &str, page: AuditPage) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::AuditPageLoaded {
            command_id: Some(command_id.to_string()),
            page,
        },
    )
}

/// A page from a Core that predates the command id, where acceptance-first is
/// the only correlation available.
fn loaded_legacy(sequence: u64, page: AuditPage) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::AuditPageLoaded {
            command_id: None,
            page,
        },
    )
}

fn rejected(sequence: u64, command_id: &str, reason: &str) -> RuntimeEventEnvelope {
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
        client.capabilities.remove(AUDIT_CAPABILITY);
    }
    for event in events {
        client = client.with_envelope(event);
    }
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect");
    Harness { adapter, sent }
}

fn audit_queries(sent: &Arc<Mutex<Vec<RuntimeCommandEnvelope>>>) -> Vec<viden_core::AuditQuery> {
    sent.lock()
        .expect("sent lock")
        .iter()
        .filter_map(|envelope| match &envelope.command {
            RuntimeCommand::QueryAudit { query } => Some(query.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn audit_mode_is_unavailable_and_sends_nothing_without_the_core_capability() {
    let mut harness = harness(Vec::new(), false);
    let projection = harness
        .adapter
        .query_audit_and_wait("gui-audit-1", None, TIMEOUT)
        .expect("an absent capability is an honest projection, not a transport error");

    assert!(!projection.capability_available);
    assert!(projection.rows.is_empty());
    // Absence is not emptiness: nothing loaded, so the client must not be able
    // to render "no audit records".
    assert!(!projection.loaded);
    assert_eq!(projection.outcome.state, "idle");
    assert!(
        audit_queries(&harness.sent).is_empty(),
        "a missing capability must send zero commands"
    );
}

/// Legacy fallback: an unnamed page arriving before our own acceptance belongs
/// to some other reader, and acceptance-first is the only rule available for a
/// Core that predates the page's command id.
#[test]
fn a_page_arriving_before_our_acceptance_is_not_treated_as_our_answer() {
    // The page lands first; it belongs to some other reader's query.
    let mut harness = harness(
        vec![
            loaded_legacy(
                1,
                page(
                    vec![record(
                        "audit-other",
                        1_700_000_500,
                        AuditActor::Operator,
                        AuditOutcome::Success,
                    )],
                    None,
                ),
            ),
            accepted(2, "gui-audit-1"),
        ],
        true,
    );
    let projection = harness
        .adapter
        .query_audit_and_wait("gui-audit-1", None, TIMEOUT)
        .expect("query");

    assert!(projection.rows.is_empty());
    assert!(!projection.loaded);
    assert_eq!(projection.outcome.state, "pending");
    assert_eq!(
        projection.pending_command_id.as_deref(),
        Some("gui-audit-1")
    );
}

/// Exact correlation: a page naming a *different* read is ignored even after
/// our own acceptance, which is precisely the window acceptance-first could
/// not close. The following page, which names our read, still confirms it.
#[test]
fn a_page_naming_another_read_is_ignored_even_after_our_acceptance() {
    let mut harness = harness(
        vec![
            accepted(1, "gui-audit-1"),
            loaded(
                2,
                "someone-elses-read",
                page(
                    vec![record(
                        "audit-other",
                        1_700_000_500,
                        AuditActor::Operator,
                        AuditOutcome::Success,
                    )],
                    None,
                ),
            ),
            loaded(
                3,
                "gui-audit-1",
                page(
                    vec![record(
                        "audit-ours",
                        1_700_000_600,
                        AuditActor::Operator,
                        AuditOutcome::Success,
                    )],
                    None,
                ),
            ),
        ],
        true,
    );
    let projection = harness
        .adapter
        .query_audit_and_wait("gui-audit-1", None, TIMEOUT)
        .expect("query");

    assert_eq!(
        projection
            .rows
            .iter()
            .map(|row| row.audit_id.as_str())
            .collect::<Vec<_>>(),
        vec!["audit-ours"],
        "a concurrent reader's page must never be attributed to this read"
    );
    assert!(projection.loaded);
    assert_eq!(projection.outcome.state, "confirmed");
    assert_eq!(projection.pending_command_id, None);
}

#[test]
fn a_page_after_our_acceptance_confirms_the_read_and_supplies_every_row() {
    let mut harness = harness(
        vec![
            accepted(1, "gui-audit-1"),
            loaded(
                2,
                "gui-audit-1",
                page(
                    vec![
                        record(
                            "audit-2",
                            1_700_000_600,
                            AuditActor::Agent {
                                agent_id: "codex-acp".to_string(),
                            },
                            AuditOutcome::Denied,
                        ),
                        record(
                            "audit-1",
                            1_700_000_500,
                            AuditActor::System,
                            AuditOutcome::Failed,
                        ),
                    ],
                    Some(AuditCursor {
                        timestamp: 1_700_000_500,
                        audit_id: "audit-1".to_string(),
                    }),
                ),
            ),
        ],
        true,
    );
    let projection = harness
        .adapter
        .query_audit_and_wait("gui-audit-1", None, TIMEOUT)
        .expect("query");

    assert_eq!(projection.outcome.state, "confirmed");
    assert!(projection.loaded);
    assert!(projection.pending_command_id.is_none());
    assert_eq!(projection.rows.len(), 2);

    // Newest first, exactly as Core delivered; nothing is re-sorted here.
    assert_eq!(projection.rows[0].audit_id, "audit-2");
    assert_eq!(projection.rows[0].actor_kind, "agent");
    assert_eq!(projection.rows[0].agent_id.as_deref(), Some("codex-acp"));
    assert_eq!(projection.rows[0].outcome, "denied");
    // The dotted action key is Core's stable vocabulary and stays raw.
    assert_eq!(projection.rows[0].action, "gate.decided");
    assert_eq!(projection.rows[0].objects.len(), 1);
    assert_eq!(projection.rows[0].objects[0].kind, "merge_gate");
    assert_eq!(projection.rows[0].objects[0].id, "gate-1");
    assert_eq!(projection.rows[0].args.len(), 1);
    assert_eq!(projection.rows[0].args[0].key, "outcome");
    assert_eq!(projection.rows[0].args[0].value, "accepted");

    assert_eq!(projection.rows[1].actor_kind, "system");
    assert_eq!(projection.rows[1].outcome, "failed");

    assert!(!projection.complete);
    assert_eq!(
        projection.next_before.as_deref(),
        Some("1700000500:audit-1")
    );

    let queries = audit_queries(&harness.sent);
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0].project_id, None);
    assert_eq!(queries[0].lane_id, None);
    assert_eq!(queries[0].object, None);
    assert_eq!(queries[0].before, None);
    assert_eq!(queries[0].limit, viden_gui::D14_AUDIT_PAGE_LIMIT);
}

#[test]
fn a_rejected_query_reports_cores_own_reason_and_keeps_no_rows() {
    let mut harness = harness(
        vec![rejected(1, "gui-audit-1", "audit store is unavailable")],
        true,
    );
    let projection = harness
        .adapter
        .query_audit_and_wait("gui-audit-1", None, TIMEOUT)
        .expect("query");

    assert_eq!(projection.outcome.state, "rejected");
    assert_eq!(
        projection.outcome.reason.as_deref(),
        Some("audit store is unavailable")
    );
    assert!(projection.rows.is_empty());
    assert!(!projection.loaded);
}

#[test]
fn a_second_query_is_refused_locally_while_one_is_still_in_flight() {
    let mut harness = harness(vec![accepted(1, "gui-audit-1")], true);
    harness
        .adapter
        .query_audit_and_wait("gui-audit-1", None, TIMEOUT)
        .expect("first query");

    let error = harness
        .adapter
        .query_audit_and_wait("gui-audit-2", None, TIMEOUT)
        .expect_err("a second concurrent read has no correlation available");
    assert!(error.contains("gui-audit-1"), "got {error}");
    assert_eq!(
        audit_queries(&harness.sent).len(),
        1,
        "the refused read must send nothing"
    );
}

#[test]
fn an_empty_page_is_only_empty_after_core_confirmed_one() {
    let mut harness = harness(
        vec![
            accepted(1, "gui-audit-1"),
            loaded(2, "gui-audit-1", page(Vec::new(), None)),
        ],
        true,
    );
    let projection = harness
        .adapter
        .query_audit_and_wait("gui-audit-1", None, TIMEOUT)
        .expect("query");

    assert!(projection.loaded, "a confirmed empty page is loaded");
    assert!(projection.rows.is_empty());
    assert!(projection.complete);
    assert_eq!(projection.next_before, None);
}

#[test]
fn load_older_pages_from_cores_cursor_and_appends_at_the_end() {
    let mut harness = harness(
        vec![
            accepted(1, "gui-audit-1"),
            loaded(
                2,
                "gui-audit-1",
                page(
                    vec![record(
                        "audit-3",
                        1_700_000_700,
                        AuditActor::Operator,
                        AuditOutcome::Success,
                    )],
                    Some(AuditCursor {
                        timestamp: 1_700_000_700,
                        audit_id: "audit-3".to_string(),
                    }),
                ),
            ),
            accepted(3, "gui-audit-2"),
            loaded(
                4,
                "gui-audit-2",
                page(
                    vec![record(
                        "audit-1",
                        1_700_000_100,
                        AuditActor::Operator,
                        AuditOutcome::Success,
                    )],
                    None,
                ),
            ),
        ],
        true,
    );
    harness
        .adapter
        .query_audit_and_wait("gui-audit-1", None, TIMEOUT)
        .expect("first page");
    let projection = harness
        .adapter
        .load_older_audit_and_wait("gui-audit-2", TIMEOUT)
        .expect("older page");

    // Older pages append at the end: the list stays newest-first end to end.
    assert_eq!(
        projection
            .rows
            .iter()
            .map(|row| row.audit_id.as_str())
            .collect::<Vec<_>>(),
        vec!["audit-3", "audit-1"]
    );
    assert!(projection.complete);
    assert_eq!(projection.next_before, None);

    let queries = audit_queries(&harness.sent);
    assert_eq!(queries.len(), 2);
    assert_eq!(
        queries[1].before,
        Some(AuditCursor {
            timestamp: 1_700_000_700,
            audit_id: "audit-3".to_string(),
        }),
        "the older page must use Core's own cursor verbatim"
    );
}

#[test]
fn a_scoped_query_passes_the_exact_object_through_and_reports_the_scope() {
    let mut harness = harness(
        vec![
            accepted(1, "gui-audit-1"),
            loaded(2, "gui-audit-1", page(Vec::new(), None)),
        ],
        true,
    );
    let projection = harness
        .adapter
        .query_audit_and_wait(
            "gui-audit-1",
            Some(D14AuditScopeInput {
                kind: AuditObjectRef::KIND_REVERT.to_string(),
                id: "revert-1".to_string(),
            }),
            TIMEOUT,
        )
        .expect("query");

    let scope = projection.scope.expect("the scope is reported back");
    assert_eq!(scope.kind, "revert");
    assert_eq!(scope.id, "revert-1");

    let queries = audit_queries(&harness.sent);
    assert_eq!(
        queries[0].object,
        Some(AuditObjectRef::new("revert", "revert-1"))
    );
}

#[test]
fn an_approval_decision_row_keeps_cores_operator_actor_action_and_every_object() {
    // The C7 row (`runtime.durable_work_evidence`): `RespondToApproval` now
    // appends the record whose id the request already showed the operator, so
    // the permission dock's audit affordance resolves to a row instead of to
    // nothing (E1 defect 4). Nothing about the row is client-derived: the
    // action key, the operator actor, and all three objects are Core's, and
    // `scope` travels as an argument rather than as a second action.
    let approval = AuditRecord {
        audit_id: "audit_durable_work_approval".to_string(),
        timestamp: 1_700_006_006,
        owner: RuntimeOwner::default(),
        actor: AuditActor::Operator,
        action: "approval.allow_once".to_string(),
        objects: vec![
            AuditObjectRef::new(AuditObjectRef::KIND_PERMISSION, "approval_durable_work"),
            AuditObjectRef::new("tool", "edit_file"),
            AuditObjectRef::new("job", "cmd_durable_work_submit"),
        ],
        outcome: AuditOutcome::Success,
        args: BTreeMap::from([("scope".to_string(), "allow_once".to_string())]),
    };
    let mut harness = harness(
        vec![
            accepted(1, "gui-audit-1"),
            loaded(2, "gui-audit-1", page(vec![approval], None)),
        ],
        true,
    );
    let projection = harness
        .adapter
        .query_audit_and_wait(
            "gui-audit-1",
            Some(D14AuditScopeInput {
                kind: AuditObjectRef::KIND_PERMISSION.to_string(),
                id: "approval_durable_work".to_string(),
            }),
            TIMEOUT,
        )
        .expect("query");

    let row = projection.rows.first().expect("the approval row");
    assert_eq!(row.audit_id, "audit_durable_work_approval");
    assert_eq!(row.action, "approval.allow_once");
    assert_eq!(row.actor_kind, "operator");
    assert_eq!(row.agent_id, None, "an operator decision names no agent");
    // A denial is a decision that was carried out, so Core records it as a
    // success; rendering it as a failure would read as the runtime refusing
    // the operator.
    assert_eq!(row.outcome, "success");
    assert_eq!(
        row.objects
            .iter()
            .map(|object| (object.kind.as_str(), object.id.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("permission", "approval_durable_work"),
            ("tool", "edit_file"),
            ("job", "cmd_durable_work_submit"),
        ]
    );
    assert_eq!(row.args[0].key, "scope");
    assert_eq!(row.args[0].value, "allow_once");

    // The dock links by the object, which is the query Core answers.
    let queries = audit_queries(&harness.sent);
    assert_eq!(
        queries[0].object,
        Some(AuditObjectRef::new("permission", "approval_durable_work"))
    );
}

#[test]
fn dropping_the_scope_requeries_unscoped_and_discards_the_scoped_rows() {
    let mut harness = harness(
        vec![
            accepted(1, "gui-audit-1"),
            loaded(
                2,
                "gui-audit-1",
                page(
                    vec![record(
                        "audit-scoped",
                        1_700_000_700,
                        AuditActor::Operator,
                        AuditOutcome::Success,
                    )],
                    None,
                ),
            ),
            accepted(3, "gui-audit-2"),
            loaded(
                4,
                "gui-audit-2",
                page(
                    vec![record(
                        "audit-all",
                        1_700_000_800,
                        AuditActor::Operator,
                        AuditOutcome::Success,
                    )],
                    None,
                ),
            ),
        ],
        true,
    );
    harness
        .adapter
        .query_audit_and_wait(
            "gui-audit-1",
            Some(D14AuditScopeInput {
                kind: AuditObjectRef::KIND_MERGE_GATE.to_string(),
                id: "gate-1".to_string(),
            }),
            TIMEOUT,
        )
        .expect("scoped query");
    let projection = harness
        .adapter
        .query_audit_and_wait("gui-audit-2", None, TIMEOUT)
        .expect("unscoped requery");

    assert_eq!(projection.scope, None);
    assert_eq!(
        projection
            .rows
            .iter()
            .map(|row| row.audit_id.as_str())
            .collect::<Vec<_>>(),
        vec!["audit-all"],
        "a fresh query replaces the previous scope's rows rather than merging them"
    );
}

#[test]
fn the_desktop_event_pump_cannot_swallow_the_page_the_screen_is_waiting_for() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    // The gap makes the query's own drain stop before Core answers, so the
    // answer can only arrive through the shared desktop pump.
    let client = TestCoreClient::new(view(), sent.clone())
        .with_gap()
        .with_envelope(accepted(1, "gui-audit-1"))
        .with_envelope(loaded(
            2,
            "gui-audit-1",
            page(
                vec![record(
                    "audit-1",
                    1_700_000_500,
                    AuditActor::Operator,
                    AuditOutcome::Success,
                )],
                None,
            ),
        ));
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect");
    let mut harness = Harness { adapter, sent };
    // A zero wait sends and returns at the first gap without draining.
    let pending = harness
        .adapter
        .query_audit_and_wait("gui-audit-1", None, Duration::ZERO)
        .expect("query");
    assert_eq!(pending.outcome.state, "pending");
    while harness.adapter.pump_events(Duration::ZERO) {}

    let projection = harness.adapter.d14_audit();
    assert_eq!(projection.outcome.state, "confirmed");
    assert_eq!(projection.rows.len(), 1);
}
