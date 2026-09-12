//! D13 fleet and workflow projections.
//!
//! A workflow is a Core `AgentDagRecord`. Its nodes are the task specs Core
//! declared and its edges are those specs' own dependency lists — the client
//! never infers an order from titles or timing. A node reports runtime status
//! only when Core is actually running that task, and it reports a blocker only
//! when Core recorded a blocked dependency for it.

use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D13BlockerProjection {
    pub dependency_id: String,
    pub depends_on_task_id: String,
    pub reason: String,
    pub audit_id: String,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D13NodeProjection {
    pub task_id: String,
    pub title: String,
    pub objective: String,
    pub role: String,
    pub depends_on: Vec<String>,
    pub required_evidence: Vec<String>,
    pub permission_policy: String,
    /// Runtime status, or `None` when Core runs no task for this spec.
    pub status: Option<String>,
    pub progress: Option<u8>,
    pub blocked: bool,
    pub blockers: Vec<D13BlockerProjection>,
    /// Every Lane whose Core record names this task, in Core's own lane order.
    ///
    /// Core publishes no node-to-Lane edge; it publishes `AgentLaneRecord::
    /// task_id`, and this is the join over it. The list is deliberately not an
    /// `Option`: zero, one, and several bound Lanes are three different facts,
    /// and collapsing them would make the screen guess which Lane a node drills
    /// into.
    pub lane_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D13WorkflowProjection {
    pub dag_id: String,
    pub goal: String,
    pub status: String,
    pub created_at: Option<u64>,
    pub updated_at: Option<u64>,
    pub nodes: Vec<D13NodeProjection>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D13HandoffProjection {
    pub handoff_id: String,
    pub task_id: String,
    pub from_lane_id: String,
    pub to_lane_id: String,
    pub summary: String,
    pub audit_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D13FleetWorkflowProjection {
    pub workflows: Vec<D13WorkflowProjection>,
    pub handoffs: Vec<D13HandoffProjection>,
}
