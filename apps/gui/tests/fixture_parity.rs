use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use viden_core::{
    EventCursor, RuntimeEventEnvelope, RuntimeSnapshot, RuntimeViewState, RuntimeWireEvent,
};
use viden_gui::GuiCoreAdapter;

mod support;
use support::TestCoreClient;

const D1_MAIN_COCKPIT_FIXTURE: &str =
    include_str!("../../../crates/types/tests/fixtures/frontend-contract-v1/d1-main-cockpit.json");
const CONTEXT_BUDGETS_FIXTURE: &str =
    include_str!("../../../crates/types/tests/fixtures/frontend-contract-v1/context-budgets.json");

#[derive(Deserialize)]
struct Fixture {
    initial_snapshot: RuntimeSnapshot,
    events: Vec<RuntimeEventEnvelope>,
    expected_final_cursor: EventCursor,
    expected_view_sha256: String,
}

fn replay(raw: &str) -> (Fixture, RuntimeViewState) {
    let fixture: Fixture = serde_json::from_str(raw).expect("parse canonical Core fixture");
    let mut view = RuntimeViewState::new(fixture.initial_snapshot.clone());
    for envelope in &fixture.events {
        if let RuntimeWireEvent::Known(event) = &envelope.event {
            view.apply_event(event);
        }
    }
    (fixture, view)
}

fn replay_fixture() -> (Fixture, RuntimeViewState) {
    replay(D1_MAIN_COCKPIT_FIXTURE)
}

fn assert_fixture_digest(fixture: &Fixture, view: &RuntimeViewState) {
    let canonical_view = sort_json(serde_json::to_value(view).expect("serialize canonical view"));
    let digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&canonical_view).expect("encode canonical view"))
    );
    assert_eq!(digest, fixture.expected_view_sha256);
    assert_eq!(
        fixture.events.last().map(|event| &event.cursor),
        Some(&fixture.expected_final_cursor)
    );
}

fn connected(view: RuntimeViewState) -> GuiCoreAdapter {
    let mut adapter = GuiCoreAdapter::new(Box::new(TestCoreClient::new(
        view,
        Arc::new(Mutex::new(Vec::new())),
    )));
    adapter.connect().expect("connect canonical fixture");
    adapter
}

fn sort_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let sorted = map
                .into_iter()
                .map(|(key, value)| (key, sort_json(value)))
                .collect::<BTreeMap<_, _>>();
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(sort_json).collect())
        }
        other => other,
    }
}

#[test]
fn d1_cockpit_fixture_projects_the_exact_committed_context_dock() {
    let (fixture, view) = replay_fixture();
    let canonical_view = sort_json(serde_json::to_value(&view).expect("serialize canonical view"));
    let digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&canonical_view).expect("encode canonical view"))
    );
    assert_eq!(digest, fixture.expected_view_sha256);
    assert_eq!(
        fixture.events.last().map(|event| &event.cursor),
        Some(&fixture.expected_final_cursor)
    );

    let mut adapter = GuiCoreAdapter::new(Box::new(TestCoreClient::new(
        view,
        Arc::new(Mutex::new(Vec::new())),
    )));
    adapter.connect().expect("connect canonical fixture");
    let projection = adapter
        .d1_cockpit(Some("lane-d1-main"))
        .expect("project canonical D1 cockpit");
    let context_dock =
        serde_json::to_value(&projection.context_dock).expect("serialize Context Dock");

    assert_eq!(
        context_dock,
        serde_json::json!({
            "source": {
                "status": "ready",
                "branch": "codex/d1-cockpit-core",
                "worktree": ".worktrees/d1-cockpit-core",
                "ahead": 1,
                "behind": 0,
                "added": 3,
                "deleted": 1,
                "dirty": true
            },
            // `C5` publishes a Lane source only for an active Lane that owns a
            // worktree; the canonical fixture's Lane works in the workspace
            // itself, so the field is null and nothing fills it in from the
            // workspace sample above.
            "laneSource": null,
            "context": null,
            "laneAgent": {
                "laneId": "lane-d1-main",
                "workspaceId": "workspace-d1-main",
                "projectId": "project-viden",
                "sessionId": "agent-session-d1-main",
                "taskId": "task-d1-main",
                "turnId": "turn-d1-main"
            },
            "provider": null,
            "services": [
                {
                    "id": "codegraph",
                    "kind": "mcp",
                    "label": "codegraph",
                    "status": "connected",
                    "detailKey": "service.connected"
                },
                {
                    "id": "rust-analyzer",
                    "kind": "lsp",
                    "label": "rust-analyzer",
                    "status": "ready",
                    "detailKey": null
                }
            ],
            "checklist": [
                {
                    "id": "change-runtime-types",
                    "kind": "workspace_change",
                    "label": "crates/types/src/runtime.rs",
                    "status": "modified",
                    "command": null,
                    "path": "crates/types/src/runtime.rs",
                    "summary": null,
                    "patch": null,
                    // Core published no structured rows for this change on
                    // the frozen fixture, so the field is present and null:
                    // "Core produced no diff", never "no change".
                    "diff": null,
                    "failingLocation": null,
                    "additions": 3,
                    "deletions": 1
                },
                {
                    "id": "check-viden-types",
                    "kind": "check_run",
                    "label": "viden-types",
                    "status": "failed",
                    "command": "cargo test -p viden-types",
                    "path": null,
                    "summary": "one assertion failed",
                    "patch": null,
                    // Core published no structured rows for this change on
                    // the frozen fixture, so the field is present and null:
                    // "Core produced no diff", never "no change".
                    "diff": null,
                    "failingLocation": "crates/types/src/tests.rs:2500",
                    "additions": null,
                    "deletions": null
                }
            ]
        })
    );
}

/// GUI-CORE-008: the Context Dock shows the selected Lane's own task budget.
///
/// The canonical fixture runs two Lanes with disjoint task-scoped budgets at
/// once, so a projection that took "the latest budget" would show one Lane the
/// other's pressure. Each selection must resolve exactly its own scope.
#[test]
fn context_budget_fixture_projects_each_lanes_own_task_scope() {
    let (fixture, view) = replay(CONTEXT_BUDGETS_FIXTURE);
    assert_fixture_digest(&fixture, &view);

    for (lane_id, expected) in [
        (
            "lane_context_alpha",
            serde_json::json!({
                "budgetId": "ctxbudget-bundle_context_alpha",
                "usedTokens": 52000,
                "softTokenLimit": 48000,
                "hardTokenLimit": 80000,
                "remainingTokens": 28000,
                "exceeded": false
            }),
        ),
        (
            "lane_context_beta",
            serde_json::json!({
                "budgetId": "ctxbudget-bundle_context_beta",
                "usedTokens": 41000,
                "softTokenLimit": 24000,
                "hardTokenLimit": 40000,
                "remainingTokens": 0,
                "exceeded": true
            }),
        ),
    ] {
        let projection = connected(view.clone())
            .d1_cockpit(Some(lane_id))
            .expect("project the canonical D1 cockpit");
        let context =
            serde_json::to_value(&projection.context_dock.context).expect("serialize context");
        assert_eq!(context, expected, "{lane_id} must project its own budget");
    }
}

/// A Lane Core bound without a task has no scope to resolve, so the dock stays
/// unavailable instead of borrowing a budget that is published and visible.
#[test]
fn a_lane_without_a_task_scope_projects_no_context_budget() {
    let (_, mut view) = replay(CONTEXT_BUDGETS_FIXTURE);
    let taskless = view
        .lane_runtime_owners
        .iter_mut()
        .find(|binding| binding.lane_id == "lane_context_alpha")
        .expect("the fixture binds an exact owner to every Lane");
    taskless.owner.task_id = None;

    let projection = connected(view)
        .d1_cockpit(Some("lane_context_alpha"))
        .expect("project the canonical D1 cockpit");
    assert!(
        projection.context_dock.context.is_none(),
        "a taskless owner must not resolve the other Lane's budget"
    );
}
