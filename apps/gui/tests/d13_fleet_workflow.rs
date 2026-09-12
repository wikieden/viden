use std::sync::{Arc, Mutex};

use viden_core::{
    DependencyRecord, DependencyState, RuntimeOwner, RuntimeSnapshot, RuntimeViewState,
};
use viden_gui::GuiCoreAdapter;

mod support;
use support::TestCoreClient;

const DAG_FIXTURE: &str =
    include_str!("../../../crates/types/tests/fixtures/frontend-contract-v1/dag-blocker.json");
/// The lane-bearing fixture. `dag-blocker` publishes a workflow and no Lane, so
/// the node-to-Lane join is exercised by lending it this fixture's real Lane
/// records rather than by hand-building one the contract never emitted.
const LANE_FIXTURE: &str =
    include_str!("../../../crates/types/tests/fixtures/frontend-contract-v1/multi-lane.json");

fn replay(fixture: &str) -> RuntimeViewState {
    #[derive(serde::Deserialize)]
    struct Fixture {
        initial_snapshot: RuntimeSnapshot,
        events: Vec<viden_core::RuntimeEventEnvelope>,
    }
    let fixture: Fixture = serde_json::from_str(fixture).unwrap();
    let mut view = RuntimeViewState::new(fixture.initial_snapshot);
    for envelope in fixture.events {
        if let viden_core::RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
    }
    view
}

fn dag_view() -> RuntimeViewState {
    let view = replay(DAG_FIXTURE);
    assert!(
        !view.agent_dags.is_empty(),
        "the dag fixture must publish a workflow"
    );
    view
}

/// A workflow view that also carries Core's own Lane records, so the join the
/// D13 drill reads is exercised against published Lanes rather than a stub.
fn dag_view_with_lanes() -> RuntimeViewState {
    let mut view = dag_view();
    let lanes = replay(LANE_FIXTURE).lanes;
    assert!(
        lanes.len() >= 2,
        "the lane fixture must publish more than one Lane"
    );
    view.lanes = lanes;
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

#[test]
fn d13_projects_the_core_dag_with_its_declared_edges() {
    let view = dag_view();
    let dag = view.agent_dags[0].clone();
    let projection = connected(view).d13_fleet_workflow().expect("D13");

    let workflow = &projection.workflows[0];
    assert_eq!(workflow.dag_id, dag.dag_id);
    assert_eq!(workflow.goal, dag.goal);
    assert_eq!(workflow.nodes.len(), dag.tasks.len());
    for (node, spec) in workflow.nodes.iter().zip(dag.tasks.iter()) {
        // Edges are the Core-declared dependency list, not an inferred order.
        assert_eq!(node.task_id, spec.task_id);
        assert_eq!(node.depends_on, spec.dependencies);
        assert_eq!(node.required_evidence, spec.required_evidence);
    }
}

#[test]
fn d13_reports_runtime_status_only_for_a_task_core_is_actually_running() {
    let mut view = dag_view();
    let known_task = view.agent_dags[0].tasks[0].task_id.clone();
    view.tasks.retain(|task| task.id == known_task);
    let has_live_task = !view.tasks.is_empty();

    let projection = connected(view).d13_fleet_workflow().unwrap();
    let workflow = &projection.workflows[0];
    for node in &workflow.nodes {
        if node.task_id == known_task && has_live_task {
            assert!(node.status.is_some(), "a live Core task must report status");
        } else {
            // A planned node with no Core task is not shown as pending work.
            assert!(node.status.is_none());
        }
    }
}

#[test]
fn d13_names_the_blocking_dependency_from_the_core_record() {
    let mut view = dag_view();
    let blocked = view.agent_dags[0].tasks[0].task_id.clone();
    view.dependencies.push(DependencyRecord {
        dependency_id: "dependency-1".to_string(),
        task_id: blocked.clone(),
        depends_on_task_id: "task-upstream".to_string(),
        owner: RuntimeOwner {
            workspace_id: "workspace-viden".to_string(),
            project_id: "project-viden".to_string(),
            ..Default::default()
        },
        state: DependencyState::Blocked,
        reason: "waits for the upstream contract".to_string(),
        audit_id: "audit-dependency-1".to_string(),
        updated_at: 1_700_000_400,
    });

    let projection = connected(view).d13_fleet_workflow().unwrap();
    let node = projection.workflows[0]
        .nodes
        .iter()
        .find(|node| node.task_id == blocked)
        .expect("blocked node");
    assert_eq!(node.blockers.len(), 1);
    assert_eq!(node.blockers[0].depends_on_task_id, "task-upstream");
    assert_eq!(node.blockers[0].reason, "waits for the upstream contract");
    assert!(node.blocked);
}

#[test]
fn d13_lists_handoffs_between_lanes_without_inventing_a_route() {
    let projection = connected(dag_view()).d13_fleet_workflow().unwrap();
    // The fixture declares no handoff; the screen shows none rather than
    // deriving one from the dependency edges.
    assert!(projection.handoffs.is_empty());
}

/// D13's node drill reads one Core fact: the Lane's own `task_id`.
///
/// Core publishes no node-to-Lane edge, so the projection joins on the binding
/// Core does publish. Zero, one, and several bound Lanes are three different
/// facts and the projection carries them all rather than collapsing them into
/// an `Option` the screen would have to guess at.
#[test]
fn d13_binds_each_node_to_every_lane_core_gave_that_task() {
    let mut view = dag_view_with_lanes();
    let task = view.agent_dags[0].tasks[0].task_id.clone();
    view.lanes[0].task_id = Some(task.clone());
    let bound_lane = view.lanes[0].id.clone();
    // The second Lane keeps whatever binding the fixture gave it, which is a
    // different task, so it must not appear on this node.
    assert_ne!(view.lanes[1].task_id.as_deref(), Some(task.as_str()));

    let projection = connected(view).d13_fleet_workflow().unwrap();
    let node = projection.workflows[0]
        .nodes
        .iter()
        .find(|node| node.task_id == task)
        .expect("the bound node");

    assert_eq!(node.lane_ids, vec![bound_lane]);
}

#[test]
fn d13_reports_no_lane_for_a_task_core_bound_none_to() {
    let mut view = dag_view_with_lanes();
    for lane in &mut view.lanes {
        lane.task_id = None;
    }

    let projection = connected(view).d13_fleet_workflow().unwrap();
    for node in &projection.workflows[0].nodes {
        // Absence is absence: an unbound node carries an empty list, never a
        // Lane id borrowed from somewhere else in the view.
        assert!(
            node.lane_ids.is_empty(),
            "{} must bind no Lane",
            node.task_id
        );
    }
}

#[test]
fn d13_keeps_every_lane_when_core_bound_more_than_one_to_a_task() {
    let mut view = dag_view_with_lanes();
    let task = view.agent_dags[0].tasks[0].task_id.clone();
    for lane in &mut view.lanes {
        lane.task_id = Some(task.clone());
    }
    let expected: Vec<String> = view.lanes.iter().map(|lane| lane.id.clone()).collect();

    let projection = connected(view).d13_fleet_workflow().unwrap();
    let node = projection.workflows[0]
        .nodes
        .iter()
        .find(|node| node.task_id == task)
        .expect("the bound node");

    // The projection does not pick one. Choosing for the operator is exactly
    // the decision the screen refuses to make.
    assert_eq!(node.lane_ids, expected);
}
