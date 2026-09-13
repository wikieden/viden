//! Runtime behavior of plan documents (`runtime.plan_documents`, C13).
//!
//! Six things must hold before a client can put an Accept button on a plan:
//!
//! 1. one open plan per owner, so a second proposal revises rather than forks;
//! 2. the revision it replaced is republished as `Superseded`, so an
//!    acceptance Core no longer honours is never left rendering as live;
//! 3. a decision is pinned to the revision *and* the digest the operator read,
//!    and a mismatch settles nothing;
//! 4. the audit row is durable before the fact that announces the decision;
//! 5. handoff is the only mutation, plan mode refuses it, and the DAG exists
//!    before the plan says it became work;
//! 6. every revision survives a restart, because a plan a reopened session
//!    cannot see is a plan the operator has to accept twice.

use viden_types::{
    AgentDagStatus, PermissionMode, PlanAuthor, PlanDocumentDraft, PlanDocumentId,
    PlanDocumentStatus, PlanSection, PlanSectionKind, PlanStepDraft, PlanStepStatus,
    RuntimeCommand, RuntimeEvent, RuntimeEventKind, RuntimeOwner, WorkMode,
};

use super::{SequenceProvider, temp_dir};
use crate::plan_documents::plan_content_sha256;
use crate::{RuntimeResumeRequest, SessionEngine};

struct Fixture {
    cwd: std::path::PathBuf,
    home: std::path::PathBuf,
    session_id: String,
    engine: SessionEngine,
}

fn fixture(name: &str) -> Fixture {
    let cwd = temp_dir(&format!("{name}_cwd"));
    let home = temp_dir(&format!("{name}_home"));
    let engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
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

fn owner() -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "ws_plan".to_string(),
        project_id: "prj_plan".to_string(),
        lane_id: None,
        session_id: Some("session_plan".to_string()),
        task_id: None,
        turn_id: Some("turn_plan".to_string()),
    }
}

fn step(title: &str, dependencies: &[&str]) -> PlanStepDraft {
    PlanStepDraft {
        title: title.to_string(),
        status: PlanStepStatus::Pending,
        role: None,
        file_scope: vec!["crates/types/src/plan.rs".to_string()],
        dependencies: dependencies.iter().map(|id| (*id).to_string()).collect(),
    }
}

fn draft(title: &str) -> PlanDocumentDraft {
    PlanDocumentDraft {
        owner: owner(),
        title: title.to_string(),
        goal: "make the plan a fact".to_string(),
        sections: vec![PlanSection {
            kind: PlanSectionKind::AcceptanceCriteria,
            body: "the fixture replays".to_string(),
        }],
        steps: vec![
            step("model the types", &[]),
            step("wire the reducer", &["step_1"]),
        ],
    }
}

fn run(engine: &mut SessionEngine, command_id: &str, command: RuntimeCommand) -> Vec<RuntimeEvent> {
    engine
        .handle_runtime_command(command_id, command, &mut |_prompt| {
            panic!("no plan command may prompt for approval")
        })
        .unwrap()
}

fn rejection(events: &[RuntimeEvent]) -> Option<String> {
    events.iter().find_map(|event| match &event.kind {
        RuntimeEventKind::CommandRejected { reason, .. } => Some(reason.clone()),
        _ => None,
    })
}

fn updates(events: &[RuntimeEvent]) -> Vec<viden_types::PlanDocumentRecord> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::PlanDocumentUpdated { plan } => Some(plan.clone()),
            _ => None,
        })
        .collect()
}

fn only_plan(engine: &SessionEngine) -> viden_types::PlanDocumentRecord {
    let plans = engine.runtime_view_state().plan_documents;
    assert_eq!(plans.len(), 1, "one open plan per owner");
    plans[0].clone()
}

fn propose(engine: &mut SessionEngine, command_id: &str, title: &str) -> Vec<RuntimeEvent> {
    run(
        engine,
        command_id,
        RuntimeCommand::ProposePlanDocument {
            draft: draft(title),
        },
    )
}

// --- 1 and 2. one open plan, and the revision it replaced --------------------

/// A first proposal is revision 1 and reaches the view. A client renders the
/// absence of a plan as "no plan proposed"; a plan Core holds must never look
/// the same.
#[test]
fn a_first_proposal_opens_revision_one_and_reaches_the_view() {
    let mut fixture = fixture("plan_first_proposal");
    let events = propose(&mut fixture.engine, "plan-1", "Model plan documents");
    assert!(rejection(&events).is_none(), "{events:?}");

    let published = updates(&events);
    assert_eq!(published.len(), 1, "a first proposal supersedes nothing");
    assert_eq!(published[0].revision, 1);
    assert_eq!(published[0].status, PlanDocumentStatus::Proposed);
    assert_eq!(published[0].proposed_by, PlanAuthor::Operator);
    assert_eq!(published[0].steps[0].id, "step_1");
    assert_eq!(published[0].steps[1].dependencies, vec!["step_1"]);

    let held = only_plan(&fixture.engine);
    assert_eq!(held, published[0]);
    assert_eq!(
        held.content_sha256,
        plan_content_sha256(&held.title, &held.goal, &held.sections, &held.steps),
        "the digest a decision pins to is minted from the plan's own bytes"
    );
}

/// A second proposal revises the open plan and republishes the earlier
/// revision as `Superseded` first. Without that republication a client keeps
/// rendering an acceptance Core has already voided.
#[test]
fn a_second_proposal_supersedes_the_revision_it_replaces() {
    let mut fixture = fixture("plan_supersede");
    propose(&mut fixture.engine, "plan-1", "First shape");
    let first = only_plan(&fixture.engine);

    let events = propose(&mut fixture.engine, "plan-2", "Second shape");
    let published = updates(&events);
    assert_eq!(published.len(), 2, "the voided revision is published too");
    assert_eq!(published[0].revision, 1);
    assert_eq!(published[0].status, PlanDocumentStatus::Superseded);
    assert_eq!(published[1].revision, 2);
    assert_eq!(published[1].status, PlanDocumentStatus::Proposed);
    assert_eq!(
        published[1].plan_id, first.plan_id,
        "a revision keeps the plan's identity"
    );
    assert_eq!(
        published[1].created_at, first.created_at,
        "a revision keeps the plan's birth time"
    );
    assert_ne!(published[1].content_sha256, first.content_sha256);

    // The view carries the latest only; the durable log keeps both.
    let held = only_plan(&fixture.engine);
    assert_eq!(held.revision, 2);
    assert_eq!(held.title, "Second shape");
}

/// Accepting and then revising voids the decision: the new revision carries no
/// decision timestamp and no audit id, because the operator never saw it.
#[test]
fn a_revision_over_an_accepted_plan_clears_the_decision_it_voids() {
    let mut fixture = fixture("plan_revise_accepted");
    propose(&mut fixture.engine, "plan-1", "First shape");
    let first = only_plan(&fixture.engine);
    accept(&mut fixture.engine, "plan-accept", &first);
    assert_eq!(
        only_plan(&fixture.engine).status,
        PlanDocumentStatus::Accepted
    );

    propose(&mut fixture.engine, "plan-2", "Second shape");
    let revised = only_plan(&fixture.engine);
    assert_eq!(revised.status, PlanDocumentStatus::Proposed);
    assert_eq!(revised.decided_at, None);
    assert_eq!(revised.decision_audit_id, None);
}

// --- 3. the pin --------------------------------------------------------------

fn accept(
    engine: &mut SessionEngine,
    command_id: &str,
    plan: &viden_types::PlanDocumentRecord,
) -> Vec<RuntimeEvent> {
    run(
        engine,
        command_id,
        RuntimeCommand::AcceptPlanDocument {
            plan_id: plan.plan_id.clone(),
            revision: plan.revision,
            content_sha256: plan.content_sha256.clone(),
        },
    )
}

/// A decision naming a revision Core has moved past is refused with the stable
/// `plan_revision_stale` key and settles nothing. A decision against "whatever
/// the plan says now" would let a revision the model proposed while the
/// operator was reading be accepted by their click.
#[test]
fn a_stale_revision_is_refused_and_decides_nothing() {
    let mut fixture = fixture("plan_stale");
    propose(&mut fixture.engine, "plan-1", "First shape");
    let stale = only_plan(&fixture.engine);
    propose(&mut fixture.engine, "plan-2", "Second shape");

    let events = accept(&mut fixture.engine, "plan-accept-stale", &stale);
    let reason = rejection(&events).expect("a stale decision is refused");
    assert!(
        reason.starts_with("plan_revision_stale:"),
        "the refusal leads with the stable key: {reason}"
    );
    assert!(reason.contains("revision 2"), "{reason}");
    assert!(updates(&events).is_empty(), "a refusal publishes no record");
    assert_eq!(
        only_plan(&fixture.engine).status,
        PlanDocumentStatus::Proposed
    );
}

/// The digest is pinned as well as the revision: a plan whose bytes changed
/// under a revision number is still not the plan the operator read.
#[test]
fn a_decision_naming_the_wrong_digest_is_refused() {
    let mut fixture = fixture("plan_digest");
    propose(&mut fixture.engine, "plan-1", "First shape");
    let plan = only_plan(&fixture.engine);

    let events = run(
        &mut fixture.engine,
        "plan-accept-wrong-digest",
        RuntimeCommand::AcceptPlanDocument {
            plan_id: plan.plan_id.clone(),
            revision: plan.revision,
            content_sha256: "0".repeat(64),
        },
    );
    assert!(
        rejection(&events)
            .expect("a wrong digest is refused")
            .starts_with("plan_revision_stale:")
    );
    assert_eq!(
        only_plan(&fixture.engine).status,
        PlanDocumentStatus::Proposed
    );
}

/// A plan id this build never minted is refused as a missing plan, not as a
/// stale one: "the plan changed" and "there is no such plan" are different
/// facts and a client offers a different recovery for each.
#[test]
fn a_decision_on_an_unknown_plan_is_refused_as_missing() {
    let mut fixture = fixture("plan_unknown");
    let events = run(
        &mut fixture.engine,
        "plan-accept-unknown",
        RuntimeCommand::AcceptPlanDocument {
            plan_id: PlanDocumentId("plan_never_minted".to_string()),
            revision: 1,
            content_sha256: "0".repeat(64),
        },
    );
    let reason = rejection(&events).expect("an unknown plan is refused");
    assert!(reason.contains("does not exist"), "{reason}");
    assert!(!reason.contains("plan_revision_stale"), "{reason}");
}

/// A decided plan is not decided twice. The recovery is a new revision, which
/// is what the refusal says.
#[test]
fn a_decided_plan_refuses_a_second_decision() {
    let mut fixture = fixture("plan_decided_twice");
    propose(&mut fixture.engine, "plan-1", "First shape");
    let plan = only_plan(&fixture.engine);
    accept(&mut fixture.engine, "plan-accept", &plan);
    let accepted = only_plan(&fixture.engine);

    let events = accept(&mut fixture.engine, "plan-accept-again", &accepted);
    let reason = rejection(&events).expect("a decided plan refuses a second decision");
    assert!(reason.contains("already been decided"), "{reason}");
}

// --- 4. audit before the announcing fact -------------------------------------

/// The accept audit row is durable before `PlanDocumentDecided`, and it names
/// the plan, the revision, and the digest that was accepted — so a reader can
/// prove *which bytes* the operator authorized rather than only that they did.
#[test]
fn an_accepted_plan_audits_the_decision_before_announcing_it() {
    let mut fixture = fixture("plan_accept_audit");
    propose(&mut fixture.engine, "plan-1", "First shape");
    let plan = only_plan(&fixture.engine);

    let events = accept(&mut fixture.engine, "plan-accept", &plan);
    let decided = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::PlanDocumentDecided {
                audit_id, status, ..
            } => Some((audit_id.clone(), *status)),
            _ => None,
        })
        .expect("an accept publishes a decided fact");
    assert_eq!(decided.1, PlanDocumentStatus::Accepted);

    let records = super::audit_runtime_tests::all_audit_records(&fixture.engine);
    let record = records
        .iter()
        .find(|record| record.audit_id == decided.0)
        .expect("the announced audit id names a durable record");
    assert_eq!(record.action, "plan.accepted");
    assert_eq!(record.owner, owner());
    assert_eq!(
        record.objects,
        vec![viden_types::AuditObjectRef::new(
            crate::plan_documents::AUDIT_KIND_PLAN,
            plan.plan_id.as_str()
        )]
    );
    assert_eq!(record.args.get("revision"), Some(&"1".to_string()));
    assert_eq!(
        record.args.get("content_sha256"),
        Some(&plan.content_sha256)
    );

    // The record the decision published carries the same audit id, so a reader
    // starting from the plan and one starting from the timeline meet.
    let held = only_plan(&fixture.engine);
    assert_eq!(held.status, PlanDocumentStatus::Accepted);
    assert_eq!(held.decision_audit_id, Some(decided.0));
    assert!(held.decided_at.is_some());
}

/// A rejection is audited under its own action with the operator's reason
/// bounded into the record, so a reader learns why work did not start.
#[test]
fn a_rejected_plan_audits_its_own_action_and_reason() {
    let mut fixture = fixture("plan_reject_audit");
    propose(&mut fixture.engine, "plan-1", "First shape");
    let plan = only_plan(&fixture.engine);

    let events = run(
        &mut fixture.engine,
        "plan-reject",
        RuntimeCommand::RejectPlanDocument {
            plan_id: plan.plan_id.clone(),
            revision: plan.revision,
            content_sha256: plan.content_sha256.clone(),
            reason: "the migration step is missing".to_string(),
        },
    );
    assert!(rejection(&events).is_none(), "{events:?}");
    assert_eq!(
        only_plan(&fixture.engine).status,
        PlanDocumentStatus::Rejected
    );

    let records = super::audit_runtime_tests::all_audit_records(&fixture.engine);
    let record = records
        .iter()
        .find(|record| record.action == "plan.rejected")
        .expect("a rejection is audited");
    assert!(
        record
            .args
            .get("reason")
            .is_some_and(|reason| reason.contains("migration")),
        "{:?}",
        record.args
    );
}

// --- 5. handoff --------------------------------------------------------------

fn hand_off(
    engine: &mut SessionEngine,
    command_id: &str,
    plan: &viden_types::PlanDocumentRecord,
) -> Vec<RuntimeEvent> {
    run(
        engine,
        command_id,
        RuntimeCommand::HandOffPlanDocument {
            plan_id: plan.plan_id.clone(),
            revision: plan.revision,
            content_sha256: plan.content_sha256.clone(),
        },
    )
}

/// Plan mode refuses the handoff with the same reason family every other
/// plan-mode refusal uses, before any audit row claims an authorization, and
/// no DAG is created. Accepting a document stays available; spawning the work
/// does not.
#[test]
fn plan_mode_refuses_the_handoff_and_audits_nothing() {
    let mut fixture = fixture("plan_handoff_plan_mode");
    fixture.engine.set_work_mode(WorkMode::Plan).unwrap();
    assert_eq!(
        fixture.engine.runtime_view_state().snapshot.permission_mode,
        PermissionMode::Plan
    );

    propose(&mut fixture.engine, "plan-1", "First shape");
    let plan = only_plan(&fixture.engine);
    // Accepting is still available in plan mode: it decides a document.
    accept(&mut fixture.engine, "plan-accept", &plan);
    let accepted = only_plan(&fixture.engine);
    assert_eq!(accepted.status, PlanDocumentStatus::Accepted);

    let events = hand_off(&mut fixture.engine, "plan-handoff", &accepted);
    let reason = rejection(&events).expect("plan mode refuses a handoff");
    assert!(reason.starts_with("plan_mode:"), "{reason}");
    assert!(
        fixture.engine.runtime_view_state().agent_dags.is_empty(),
        "a refused handoff creates no work"
    );
    assert!(
        !super::audit_runtime_tests::all_audit_records(&fixture.engine)
            .iter()
            .any(|record| record.action == "plan.handed_off"),
        "a refusal before the effect authorizes nothing"
    );
    assert_eq!(
        only_plan(&fixture.engine).status,
        PlanDocumentStatus::Accepted
    );
}

/// A plan that was never accepted cannot become work, whatever mode Core is in.
#[test]
fn only_an_accepted_plan_can_be_handed_off() {
    let mut fixture = fixture("plan_handoff_unaccepted");
    propose(&mut fixture.engine, "plan-1", "First shape");
    let proposed = only_plan(&fixture.engine);

    let events = hand_off(&mut fixture.engine, "plan-handoff", &proposed);
    let reason = rejection(&events).expect("an undecided plan refuses a handoff");
    assert!(reason.contains("only an accepted plan"), "{reason}");
    assert!(fixture.engine.runtime_view_state().agent_dags.is_empty());
}

/// The DAG exists before the plan says it became work, its tasks carry the
/// steps' mapped dependencies, and the plan's `handoff` names the DAG. A client
/// following the link must never reach a DAG Core has not published yet.
#[test]
fn a_handoff_publishes_the_dag_before_the_plan_names_it() {
    let mut fixture = fixture("plan_handoff");
    propose(&mut fixture.engine, "plan-1", "First shape");
    let plan = only_plan(&fixture.engine);
    accept(&mut fixture.engine, "plan-accept", &plan);
    let accepted = only_plan(&fixture.engine);

    let events = hand_off(&mut fixture.engine, "plan-handoff", &accepted);
    assert!(rejection(&events).is_none(), "{events:?}");

    let dag_index = events
        .iter()
        .position(|event| matches!(event.kind, RuntimeEventKind::AgentDagUpdated { .. }))
        .expect("a handoff publishes a DAG");
    let handed_off_index = events
        .iter()
        .position(|event| matches!(event.kind, RuntimeEventKind::PlanDocumentHandedOff { .. }))
        .expect("a handoff publishes its settlement");
    assert!(
        dag_index < handed_off_index,
        "the work exists before the plan says it became work"
    );

    let dag = fixture
        .engine
        .runtime_view_state()
        .agent_dags
        .first()
        .cloned()
        .expect("the DAG reaches the view");
    assert_eq!(dag.status, AgentDagStatus::Active);
    assert_eq!(dag.tasks.len(), 2, "one task per step");
    assert_eq!(dag.tasks[0].dependencies, Vec::<String>::new());
    assert_eq!(
        dag.tasks[1].dependencies,
        vec![dag.tasks[0].task_id.clone()],
        "a step dependency becomes a task dependency"
    );
    assert_eq!(
        dag.tasks[0].role,
        viden_types::AgentRole::Coder,
        "a step with no role becomes a Coder task"
    );

    let held = only_plan(&fixture.engine);
    assert_eq!(held.status, PlanDocumentStatus::HandedOff);
    let handoff = held.handoff.expect("a handed-off plan names its DAG");
    assert_eq!(handoff.dag_id, dag.dag_id);
    let records = super::audit_runtime_tests::all_audit_records(&fixture.engine);
    assert!(
        records
            .iter()
            .any(|record| record.audit_id == handoff.audit_id
                && record.action == "plan.handed_off"),
        "the handoff names a durable audit record"
    );
}

// --- 6. durability -----------------------------------------------------------

/// A reopened session answers with the plan it already held. A plan a restart
/// loses is a plan the operator has to accept twice, and the second acceptance
/// would be of bytes they never re-read.
#[test]
fn a_restart_rebuilds_every_plan_it_had_accepted() {
    let mut fixture = fixture("plan_restart");
    propose(&mut fixture.engine, "plan-1", "First shape");
    let first = only_plan(&fixture.engine);
    propose(&mut fixture.engine, "plan-2", "Second shape");
    let revised = only_plan(&fixture.engine);
    accept(&mut fixture.engine, "plan-accept", &revised);
    let before = only_plan(&fixture.engine);
    assert_eq!(before.revision, 2);
    assert_ne!(before.plan_id, PlanDocumentId(String::new()));
    assert_eq!(before.plan_id, first.plan_id);

    let mut reopened = SessionEngine::new_with_home(
        &fixture.cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(fixture.home.clone()),
    )
    .unwrap();
    reopened
        .resume_session(RuntimeResumeRequest::exact_session_id(&fixture.session_id))
        .unwrap();

    let after = only_plan(&reopened);
    assert_eq!(
        after, before,
        "a restart answers with the plan Core already held"
    );
    assert_eq!(after.status, PlanDocumentStatus::Accepted);
}

// --- bounds ------------------------------------------------------------------

/// An over-bound proposal is refused rather than truncated, and nothing is
/// published. A clamped plan is a plan the operator decides on and the author
/// never wrote.
#[test]
fn an_over_bound_proposal_is_refused_and_publishes_nothing() {
    let mut fixture = fixture("plan_bounds");
    let mut over = draft("Too many steps");
    over.steps = (0..=viden_types::MAX_PLAN_STEPS)
        .map(|index| step(&format!("step {index}"), &[]))
        .collect();

    let events = run(
        &mut fixture.engine,
        "plan-over",
        RuntimeCommand::ProposePlanDocument { draft: over },
    );
    let reason = rejection(&events).expect("an over-bound proposal is refused");
    assert!(reason.contains("step bound"), "{reason}");
    assert!(
        fixture
            .engine
            .runtime_view_state()
            .plan_documents
            .is_empty(),
        "a refused proposal opens no plan"
    );
}

/// A plan that belongs to nobody is refused: "one open plan per owner" would
/// collapse into one global plan the moment a default owner were allowed.
#[test]
fn a_proposal_with_no_owner_is_refused() {
    let mut fixture = fixture("plan_ownerless");
    let mut ownerless = draft("Ownerless");
    ownerless.owner = RuntimeOwner::default();

    let events = run(
        &mut fixture.engine,
        "plan-ownerless",
        RuntimeCommand::ProposePlanDocument { draft: ownerless },
    );
    assert!(
        rejection(&events)
            .expect("an ownerless plan is refused")
            .contains("names nobody")
    );
}
