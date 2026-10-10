//! D15 Plan Studio projection tests (`runtime.plan_documents`, C13).
//!
//! Every state is replayed from the canonical `plan-documents.json` fixture
//! rather than hand-built, so the screen is measured against the wire truth
//! Core froze. The fixture's twenty events walk the whole capability, and the
//! prefix each test replays is named in its own body.

use std::sync::{Arc, Mutex};

use viden_core::{PermissionMode, RuntimeSnapshot, RuntimeViewState, WorkMode};
use viden_gui::GuiCoreAdapter;

mod support;
use support::TestCoreClient;

const PLAN_FIXTURE: &str =
    include_str!("../../../crates/types/tests/fixtures/frontend-contract-v1/plan-documents.json");
/// A Lane-bearing fixture, for the Lane-scoped owner rule. `plan-documents`
/// publishes a session-scoped plan and no Lane.
const LANE_FIXTURE: &str =
    include_str!("../../../crates/types/tests/fixtures/frontend-contract-v1/multi-lane.json");

#[derive(serde::Deserialize)]
struct Fixture {
    initial_snapshot: RuntimeSnapshot,
    events: Vec<viden_core::RuntimeEventEnvelope>,
}

/// Replays the first `events` envelopes of a fixture through Core's own reducer.
///
/// The prefix is the point: the fixture ends handed off, and every earlier
/// state of the capability is a real Core state the same stream passed through.
fn replay(fixture: &str, events: usize) -> RuntimeViewState {
    let fixture: Fixture = serde_json::from_str(fixture).unwrap();
    let mut view = RuntimeViewState::new(fixture.initial_snapshot);
    for envelope in fixture.events.into_iter().take(events) {
        if let viden_core::RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
    }
    view
}

fn connected(view: RuntimeViewState) -> GuiCoreAdapter {
    let mut adapter = GuiCoreAdapter::new(Box::new(TestCoreClient::new(
        view,
        Arc::new(Mutex::new(Vec::new())),
    )));
    adapter.connect().unwrap();
    adapter
}

/// Core's own capability set minus `runtime.plan_documents`.
fn connected_without_capability(view: RuntimeViewState) -> GuiCoreAdapter {
    let mut client = TestCoreClient::new(view, Arc::new(Mutex::new(Vec::new())));
    client
        .capabilities
        .remove(viden_gui::PLAN_DOCUMENTS_CAPABILITY);
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().unwrap();
    adapter
}

#[test]
fn d15_reads_the_proposed_revision_core_published() {
    // Through event 7: the second `update_plan` call superseded revision 1 and
    // proposed revision 2.
    let view = replay(PLAN_FIXTURE, 7);
    let record = view.plan_documents[0].clone();
    let projection = connected(view).d15_plan_studio(None).expect("D15");

    assert!(projection.capability_available);
    let plan = projection.plan.expect("the fixture publishes one plan");
    assert_eq!(plan.plan_id, record.plan_id.0);
    assert_eq!(plan.status, "proposed");
    assert_eq!(plan.revision, 2);
    // The digest is echoed, never recomputed: the pin is Core's own bytes.
    assert_eq!(plan.content_sha256, record.content_sha256);
    assert_eq!(plan.steps.len(), 2);
    assert_eq!(plan.steps[1].dependencies, vec!["step_1".to_string()]);
    assert_eq!(plan.steps[1].role.as_deref(), Some("reviewer"));
    assert_eq!(plan.proposed_by.kind, "assistant");
    // Two roles, counted from the steps rather than invented.
    assert_eq!(plan.steps_by_role.len(), 2);
    assert_eq!(plan.steps_by_role[0].role, "coder");
    assert_eq!(plan.steps_by_role[0].steps, 1);
    assert_eq!(plan.completed_steps, 0);
    assert!(projection.decision.can_decide);
    assert!(!projection.decision.can_hand_off);
}

/// The record is the latest revision only. The reducer upserts by plan id, so
/// the superseded revision 1 republished at event 6 leaves no second record —
/// and the view must not grow a history Core does not publish.
#[test]
fn d15_keeps_one_record_per_plan_and_no_invented_history() {
    let view = replay(PLAN_FIXTURE, 7);
    assert_eq!(view.plan_documents.len(), 1);
    let projection = connected(view).d15_plan_studio(None).unwrap();
    assert!(projection.ambiguous_plan_ids.is_empty());
    assert_eq!(projection.plan.unwrap().revision, 2);
}

/// Sections are ordered by `PlanSectionKind`, which derives `Ord` in its
/// declaration order. Core bounds a plan to one section per kind, so ordering
/// loses nothing and every plan reads in the order D15 draws.
#[test]
fn d15_orders_sections_by_the_declared_section_kind() {
    let mut view = replay(PLAN_FIXTURE, 7);
    let plan = &mut view.plan_documents[0];
    plan.sections.reverse();
    assert_eq!(plan.sections[0].kind.as_str(), "acceptance_criteria");

    let projection = connected(view).d15_plan_studio(None).unwrap();
    let sections = projection.plan.unwrap().sections;
    assert_eq!(sections[0].kind, "requirements");
    assert_eq!(sections[1].kind, "acceptance_criteria");
}

/// The accepted plan in plan mode: the decision is recorded and the handoff is
/// the one thing Core refuses, so it is disabled and names `plan_mode`.
#[test]
fn d15_blocks_the_handoff_while_plan_mode_is_active() {
    // Through event 12: the plan is accepted and the work mode is still Plan.
    let mut view = replay(PLAN_FIXTURE, 12);
    assert_eq!(view.snapshot.work_mode, WorkMode::Plan);
    // Core couples the two modes (`set_work_mode`), which the hand-authored
    // contract fixture does not; the coupled state is what a runtime produces.
    view.snapshot.permission_mode = PermissionMode::Plan;

    let projection = connected(view).d15_plan_studio(None).unwrap();
    assert!(projection.plan_mode_active);
    let plan = projection.plan.as_ref().unwrap();
    assert_eq!(plan.status, "accepted");
    assert_eq!(
        plan.decision_audit_id.as_deref(),
        Some("audit_plan_accepted")
    );
    assert!(!projection.decision.can_hand_off);
    assert_eq!(
        projection.decision.hand_off_block,
        Some(viden_gui::D15_PLAN_MODE_CODE)
    );
    // A decided plan takes no second decision either, and says which rule.
    assert!(!projection.decision.can_decide);
    assert_eq!(
        projection.decision.decide_block,
        Some(viden_gui::D15_ALREADY_DECIDED_CODE)
    );
}

#[test]
fn d15_opens_the_handoff_once_core_reports_build_mode() {
    // Through event 16: `SetWorkMode build` was accepted and the snapshot
    // republished with the build mode.
    let view = replay(PLAN_FIXTURE, 16);
    assert_eq!(view.snapshot.work_mode, WorkMode::Build);

    let projection = connected(view).d15_plan_studio(None).unwrap();
    assert!(!projection.plan_mode_active);
    assert!(projection.decision.can_hand_off);
    assert_eq!(projection.decision.hand_off_block, None);
}

#[test]
fn d15_names_the_dag_a_handed_off_plan_became() {
    let view = replay(PLAN_FIXTURE, 20);
    let projection = connected(view).d15_plan_studio(None).unwrap();
    let plan = projection.plan.expect("plan");
    assert_eq!(plan.status, "handed_off");
    let handoff = plan.handoff.expect("the handed-off plan names its DAG");
    assert_eq!(handoff.dag_id, "dag_plan_contract_v1");
    assert_eq!(handoff.audit_id, "audit_plan_handed_off");
    // Handed off is not a second handoff: the control says which rule holds it.
    assert!(!projection.decision.can_hand_off);
    assert_eq!(
        projection.decision.hand_off_block,
        Some(viden_gui::D15_ALREADY_HANDED_OFF_CODE)
    );
}

/// Absence is absence. A Core that publishes the capability and holds no plan
/// for this owner reports no plan, and the decision row says which rule.
#[test]
fn d15_reports_no_plan_for_an_owner_core_holds_none_for() {
    let view = replay(LANE_FIXTURE, usize::MAX);
    assert!(view.plan_documents.is_empty());
    let projection = connected(view).d15_plan_studio(None).unwrap();
    assert!(projection.capability_available);
    assert!(projection.plan.is_none());
    assert_eq!(
        projection.decision.decide_block,
        Some(viden_gui::D15_NO_PLAN_CODE)
    );
}

/// The owner rule is the runtime's own `plans_share_owner`: a plan names the
/// same Lane, or both name the session. A session-scoped plan must not appear
/// under a Lane that has none.
#[test]
fn d15_scopes_a_plan_to_the_owner_the_view_is_reading() {
    let view = replay(PLAN_FIXTURE, 7);
    let adapter = connected(view);
    assert!(adapter.d15_plan_studio(None).unwrap().plan.is_some());
    // The fixture's plan is session-scoped; a Lane scope finds none.
    let lane_scoped = adapter.d15_plan_studio(Some("lane-1")).unwrap();
    assert!(lane_scoped.plan.is_none());
    assert_eq!(lane_scoped.owner_lane_id.as_deref(), Some("lane-1"));
}

#[test]
fn d15_finds_a_lane_scoped_plan_under_its_own_lane() {
    let mut view = replay(PLAN_FIXTURE, 7);
    view.plan_documents[0].owner.lane_id = Some("lane-1".to_string());

    let adapter = connected(view);
    let lane_scoped = adapter.d15_plan_studio(Some("lane-1")).unwrap();
    let plan = lane_scoped.plan.expect("the Lane's own plan");
    assert_eq!(plan.owner_lane_id.as_deref(), Some("lane-1"));
    // And the session scope no longer claims it.
    assert!(adapter.d15_plan_studio(None).unwrap().plan.is_none());
}

/// Two open plans for one owner is a state Core does not produce. If it ever
/// arrives, the client names the ambiguity rather than deciding one of them.
#[test]
fn d15_refuses_to_choose_between_two_open_plans_for_one_owner() {
    let mut view = replay(PLAN_FIXTURE, 7);
    let mut second = view.plan_documents[0].clone();
    second.plan_id = viden_core::PlanDocumentId("plan_second".to_string());
    second.updated_at += 10;
    view.plan_documents.push(second);

    let projection = connected(view).d15_plan_studio(None).unwrap();
    assert!(projection.plan.is_none());
    assert_eq!(projection.ambiguous_plan_ids.len(), 2);
    assert_eq!(
        projection.decision.decide_block,
        Some(viden_gui::D15_AMBIGUOUS_OWNER_CODE)
    );
}

/// The capability gate. Core without `runtime.plan_documents` gets a projection
/// that says so; nothing is sent and no plan is claimed.
#[test]
fn d15_reports_an_absent_capability_rather_than_an_empty_plan() {
    let view = replay(PLAN_FIXTURE, 7);
    assert!(
        !view.plan_documents.is_empty(),
        "the fixture's view state does carry a plan"
    );
    let adapter = connected_without_capability(view);
    let projection = adapter.d15_plan_studio(None).unwrap();
    assert!(!projection.capability_available);
    assert!(!adapter.supports_plan_documents());
    // The view state still carries the record, and the projection still
    // publishes none: the capability is the promise that these records mean
    // what the screen would say they mean, and rendering one under "plan
    // documents are unavailable" states two things about one Core at once.
    assert!(projection.plan.is_none());
    assert_eq!(
        projection.decision.decide_block,
        Some(viden_gui::D15_CAPABILITY_ABSENT_CODE)
    );
    assert_eq!(
        projection.decision.hand_off_block,
        Some(viden_gui::D15_CAPABILITY_ABSENT_CODE)
    );
}

/// A decision cannot be sent at all without the capability: the adapter refuses
/// locally rather than sending a command a Core that never advertised the
/// capability would have to reject.
#[test]
fn d15_refuses_every_decision_without_the_capability() {
    let view = replay(PLAN_FIXTURE, 7);
    let plan_id = view.plan_documents[0].plan_id.0.clone();
    let sha = view.plan_documents[0].content_sha256.clone();
    let mut adapter = connected_without_capability(view);
    let error = adapter
        .send_d15_intent_and_wait(
            "gui-plan-accept-6",
            viden_gui::D15Intent::Accept {
                plan_id,
                revision: 2,
                content_sha256: sha,
            },
            std::time::Duration::ZERO,
        )
        .expect_err("no capability means nothing is sent");
    assert!(error.contains(viden_gui::PLAN_DOCUMENTS_CAPABILITY));
}

/// The pin travels from the render. A decision sent for a plan Core no longer
/// publishes is refused here rather than sent as a command Core must reject.
#[test]
fn d15_refuses_a_decision_on_a_plan_core_does_not_publish() {
    let view = replay(PLAN_FIXTURE, 7);
    let mut adapter = connected(view);
    let error = adapter
        .send_d15_intent_and_wait(
            "gui-plan-accept-1",
            viden_gui::D15Intent::Accept {
                plan_id: "plan_missing".to_string(),
                revision: 2,
                content_sha256: "0".repeat(64),
            },
            std::time::Duration::ZERO,
        )
        .expect_err("a plan Core does not publish is refused locally");
    assert!(error.contains("plan_missing"));
}

/// A rejection reason over Core's bound is refused, never truncated: a clamped
/// reason would be a sentence the operator did not write reaching an audit row
/// under their name.
#[test]
fn d15_refuses_an_over_bound_rejection_reason() {
    let view = replay(PLAN_FIXTURE, 7);
    let plan_id = view.plan_documents[0].plan_id.0.clone();
    let sha = view.plan_documents[0].content_sha256.clone();
    let mut adapter = connected(view);
    let error = adapter
        .send_d15_intent_and_wait(
            "gui-plan-reject-1",
            viden_gui::D15Intent::Reject {
                plan_id,
                revision: 2,
                content_sha256: sha,
                reason: "x".repeat(viden_core::MAX_PLAN_REJECT_REASON_BYTES + 1),
            },
            std::time::Duration::ZERO,
        )
        .expect_err("an over-bound reason is refused");
    assert!(error.contains("over Core's"));
}

/// The command carries exactly the revision and digest the render held, and it
/// travels as the owner Core published on the plan — never a client-built one.
#[test]
fn d15_sends_the_pinned_decision_as_the_owner_core_published() {
    let view = replay(PLAN_FIXTURE, 7);
    let record = view.plan_documents[0].clone();
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = GuiCoreAdapter::new(Box::new(TestCoreClient::new(view, sent.clone())));
    adapter.connect().unwrap();
    adapter
        .send_d15_intent_and_wait(
            "gui-plan-accept-2",
            viden_gui::D15Intent::Accept {
                plan_id: record.plan_id.0.clone(),
                // Deliberately the *stale* revision: the host must forward it
                // untouched so Core can refuse with `plan_revision_stale`.
                revision: 1,
                content_sha256: "062e22ca553ff5f82976fc2a85b7e065809fd1badacc4512594db1614b2ad316"
                    .to_string(),
            },
            std::time::Duration::ZERO,
        )
        .expect("the command is sent");

    let sent = sent.lock().unwrap();
    let envelope = sent.last().expect("one command");
    assert_eq!(envelope.command_id, "gui-plan-accept-2");
    assert_eq!(envelope.owner, record.owner);
    match &envelope.command {
        viden_core::RuntimeCommand::AcceptPlanDocument {
            plan_id,
            revision,
            content_sha256,
        } => {
            assert_eq!(plan_id, &record.plan_id);
            assert_eq!(*revision, 1);
            assert!(content_sha256.starts_with("062e22ca"));
        }
        other => panic!("expected AcceptPlanDocument, got {other:?}"),
    }
}

/// One plan command at a time: a second sent while the first is unanswered
/// could only race Core's own refusal, or hand the same plan off twice.
#[test]
fn d15_refuses_a_second_command_while_one_is_pending() {
    let view = replay(PLAN_FIXTURE, 7);
    let plan_id = view.plan_documents[0].plan_id.0.clone();
    let sha = view.plan_documents[0].content_sha256.clone();
    let mut adapter = connected(view);
    let intent = viden_gui::D15Intent::Accept {
        plan_id,
        revision: 2,
        content_sha256: sha,
    };
    adapter
        .send_d15_intent_and_wait(
            "gui-plan-accept-3",
            intent.clone(),
            std::time::Duration::ZERO,
        )
        .expect("the first command is sent");
    let error = adapter
        .send_d15_intent_and_wait("gui-plan-accept-4", intent, std::time::Duration::ZERO)
        .expect_err("the second is refused locally");
    assert!(error.contains("gui-plan-accept-3"));
}

/// Core's refusal is the outcome, in Core's own words. The fixture's own
/// `plan_revision_stale` sentence is what the view renders.
#[test]
fn d15_reports_cores_stale_refusal_verbatim() {
    let view = replay(PLAN_FIXTURE, 7);
    let record = view.plan_documents[0].clone();
    let client = TestCoreClient::new(view, Arc::new(Mutex::new(Vec::new()))).with_event(
        viden_core::RuntimeEventKind::CommandRejected {
            command_id: "gui-plan-accept-5".to_string(),
            reason: "plan_revision_stale: plan `plan_contract_v1` is at revision 2 and the \
                     decision named revision 1"
                .to_string(),
        },
    );
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().unwrap();
    let result = adapter
        .send_d15_intent_and_wait(
            "gui-plan-accept-5",
            viden_gui::D15Intent::Accept {
                plan_id: record.plan_id.0.clone(),
                revision: 1,
                content_sha256: "062e22ca553ff5f82976fc2a85b7e065809fd1badacc4512594db1614b2ad316"
                    .to_string(),
            },
            std::time::Duration::from_millis(10),
        )
        .expect("the refusal is an outcome, not an error");
    assert_eq!(result.outcome.state, "rejected");
    assert!(
        result
            .outcome
            .reason
            .unwrap()
            .contains(viden_gui::D15_REVISION_STALE_CODE)
    );
    // The view re-reads the record Core holds: still revision 2.
    assert_eq!(result.projection.plan.unwrap().revision, 2);
}

/// E3e: `Accept` pressed in the native window came back
/// `invalid args \`intent\` for command \`d15_send_intent\`: missing field
/// \`plan_id\`` — the webview sends `models/plan.ts`'s camelCase fields
/// (`planId`, `contentSha256`) and the host deserialized snake_case ones, so
/// no decision ever left the client. Every other intent here was built in Rust,
/// which is why none of them saw it. This parses the exact bytes Tauri hands
/// the command.
#[test]
fn the_intent_the_webview_sends_is_the_intent_the_host_reads() {
    let accept: viden_gui::D15Intent = serde_json::from_str(
        r#"{"type":"accept","planId":"plan_1","revision":1,"contentSha256":"93aaa047"}"#,
    )
    .expect("the webview's accept must deserialize");
    assert_eq!(
        accept,
        viden_gui::D15Intent::Accept {
            plan_id: "plan_1".to_string(),
            revision: 1,
            content_sha256: "93aaa047".to_string(),
        }
    );
    let reject: viden_gui::D15Intent = serde_json::from_str(
        r#"{"type":"reject","planId":"plan_1","revision":1,"contentSha256":"93aaa047","reason":"no"}"#,
    )
    .expect("the webview's reject must deserialize");
    assert!(matches!(reject, viden_gui::D15Intent::Reject { .. }));
    let hand_off: viden_gui::D15Intent = serde_json::from_str(
        r#"{"type":"hand_off","planId":"plan_1","revision":1,"contentSha256":"93aaa047"}"#,
    )
    .expect("the webview's hand-off must deserialize");
    assert!(matches!(hand_off, viden_gui::D15Intent::HandOff { .. }));
}
