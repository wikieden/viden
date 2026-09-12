//! The client side of `runtime.workspace_owner` and `ui.layout_preferences`
//! (C5, closes GUI-CORE-027 for the GUI).
//!
//! Four rules are under test, and each exists because skipping it puts a
//! specific lie on screen:
//!
//! 1. **The workspace target acts as the owner Core minted.** A commit with no
//!    Lane selected travels with `RuntimeViewState.workspace_owner` on both
//!    halves of the envelope — never `RuntimeOwner::default()`, which names
//!    nobody and is the whole of GUI-CORE-027.
//! 2. **No published owner is a refusal, not a default.** Without the fact the
//!    bar is disabled and labelled with Core's absence, and *nothing is sent*.
//! 3. **A Lane's source is its own tree's.** `lane_sources[lane]` reaches the
//!    Lane projection, so the tab strip prints that worktree's branch and
//!    ahead/behind rather than the workspace chip's.
//! 4. **The layout record is Core's, and `persisted: false` is visible.** The
//!    webview holds no copy beyond the current view: the mode it renders comes
//!    from the published record, a pin sends a patch, and a record Core could
//!    not write keeps its diagnostic.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use viden_core::{
    EventCursor, FRONTEND_SCHEMA_V1, LaneSidebarMode, RuntimeCommand, RuntimeCommandEnvelope,
    RuntimeEvent, RuntimeEventEnvelope, RuntimeEventKind, RuntimeOwner, RuntimeSnapshot,
    RuntimeViewState, RuntimeWireEvent, SourceTarget, UiLayoutPreferences, UiPreferenceDiagnostic,
    WorkspaceSourceStatus, WorkspaceSourceView,
};
use viden_gui::{
    GuiCoreAdapter, LAYOUT_PREFERENCES_CAPABILITY, LayoutPreferenceIntent,
    LayoutPreferencePatchInput, OPERATOR_GIT_NO_WORKSPACE_OWNER_CODE, OperatorGitIntent,
};

mod support;
use support::TestCoreClient;

const TIMEOUT: Duration = Duration::from_millis(10);
const LANE: &str = "lane_core";

/// The canonical multi-Lane fixture, replayed through the real reducer so the
/// Lanes under test are the ones Core's own events produce.
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
    view
}

/// The owner Core mints at open: ids on both scopes, no Lane, no session.
fn workspace_owner() -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "ws_0123456789abcdef".to_string(),
        project_id: "prj_0123456789".to_string(),
        lane_id: None,
        session_id: None,
        task_id: None,
        turn_id: None,
    }
}

fn view_with_workspace_owner() -> RuntimeViewState {
    let mut view = view();
    view.workspace_owner = Some(workspace_owner());
    view
}

fn lane_source() -> WorkspaceSourceView {
    WorkspaceSourceView {
        status: WorkspaceSourceStatus::Ready,
        branch: Some("vd/retry-policy".to_string()),
        worktree: Some("/workspace/viden/.worktrees/vd-retry-policy".to_string()),
        ahead: 3,
        behind: 1,
        added: 4,
        deleted: 2,
        dirty: true,
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

struct Harness {
    adapter: GuiCoreAdapter,
    sent: Arc<Mutex<Vec<RuntimeCommandEnvelope>>>,
}

fn harness(view: RuntimeViewState, events: Vec<RuntimeEventEnvelope>, layout: bool) -> Harness {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut client = TestCoreClient::new(view, sent.clone());
    if !layout {
        client.capabilities.remove(LAYOUT_PREFERENCES_CAPABILITY);
    }
    for event in events {
        client = client.with_envelope(event);
    }
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect");
    Harness { adapter, sent }
}

#[test]
fn a_workspace_target_action_travels_with_the_owner_core_published() {
    let mut harness = harness(view_with_workspace_owner(), Vec::new(), true);
    harness
        .adapter
        .run_operator_git_action_and_wait(
            "gui-git-ws-1",
            None,
            OperatorGitIntent::Commit {
                message: "feat: land the commit bar without a Lane".to_string(),
            },
            TIMEOUT,
        )
        .expect("a workspace commit is sent under Core's own owner");

    let commands = harness.sent.lock().expect("sent lock").clone();
    let action = commands
        .iter()
        .find(|envelope| {
            matches!(
                envelope.command,
                RuntimeCommand::RunOperatorGitAction { .. }
            )
        })
        .expect("one operator action left the client");
    assert_eq!(
        action.owner,
        workspace_owner(),
        "the envelope acts as the owner Core minted, not as a default"
    );
    match &action.command {
        RuntimeCommand::RunOperatorGitAction { owner, target, .. } => {
            assert_eq!(owner, &workspace_owner(), "both halves name the same owner");
            assert_eq!(
                target,
                &SourceTarget::Workspace,
                "no Lane selected is the workspace root"
            );
        }
        other => panic!("unexpected command {other:?}"),
    }
}

#[test]
fn a_workspace_target_is_refused_locally_when_core_published_no_owner() {
    let mut harness = harness(view(), Vec::new(), true);
    let error = harness
        .adapter
        .run_operator_git_action_and_wait(
            "gui-git-ws-2",
            None,
            OperatorGitIntent::Commit {
                message: "feat: nothing to act as".to_string(),
            },
            TIMEOUT,
        )
        .expect_err("a workspace action with no published owner is refused");
    assert!(
        error.contains(OPERATOR_GIT_NO_WORKSPACE_OWNER_CODE),
        "the refusal names the client-local code: {error}"
    );
    assert!(
        error.contains("workspace owner"),
        "the refusal says which Core fact is missing: {error}"
    );
    assert!(
        !harness
            .sent
            .lock()
            .expect("sent lock")
            .iter()
            .any(|envelope| matches!(
                envelope.command,
                RuntimeCommand::RunOperatorGitAction { .. }
            )),
        "nothing is sent without an owner to act as"
    );

    let projection = harness.adapter.operator_git(None);
    assert!(
        !projection.owner_available,
        "the bar renders disabled rather than enabled and inert"
    );
    assert!(
        projection
            .owner_unavailable_reason
            .as_deref()
            .is_some_and(|reason| reason.contains(OPERATOR_GIT_NO_WORKSPACE_OWNER_CODE)),
        "the reason is on the projection, in the client's own words"
    );
}

#[test]
fn a_lane_carries_its_own_worktree_source_rather_than_the_workspaces() {
    let mut view = view_with_workspace_owner();
    let mut sources = BTreeMap::new();
    sources.insert(LANE.to_string(), lane_source());
    view.lane_sources = sources;
    let harness = harness(view, Vec::new(), true);

    let projection = harness
        .adapter
        .d1_cockpit(Some(LANE))
        .expect("a cockpit projection");
    let lane = projection
        .lanes
        .iter()
        .find(|lane| lane.id == LANE)
        .expect("the Lane Core published");
    let source = lane
        .source
        .as_ref()
        .expect("Core sampled this Lane's own worktree");
    assert_eq!(source.branch.as_deref(), Some("vd/retry-policy"));
    assert_eq!((source.ahead, source.behind), (3, 1));
    assert!(source.dirty, "the Lane's own tree is dirty");

    let other = projection
        .lanes
        .iter()
        .find(|lane| lane.id != LANE)
        .expect("the fixture publishes a second Lane");
    assert!(
        other.source.is_none(),
        "a Lane Core sampled no worktree for has no row, never the workspace's"
    );
}

#[test]
fn the_layout_record_is_cores_and_a_pin_sends_a_patch() {
    let mut view = view_with_workspace_owner();
    view.layout_preferences = Some(UiLayoutPreferences {
        lane_sidebar_mode: LaneSidebarMode::Pinned,
        hidden_statusbar_segments: vec!["latency".to_string(), "req".to_string()],
    });
    let mut harness = harness(
        view,
        vec![envelope(
            1,
            RuntimeEventKind::UiLayoutPreferencesUpdated {
                command_id: Some("gui-layout-1".to_string()),
                preferences: UiLayoutPreferences {
                    lane_sidebar_mode: LaneSidebarMode::Floating,
                    hidden_statusbar_segments: vec!["latency".to_string()],
                },
                persisted: false,
                diagnostics: vec![UiPreferenceDiagnostic {
                    code: "ui.layout.persist_failed".to_string(),
                    key: "ui.layout.persistence_failed".to_string(),
                    field: Some("lane_sidebar_mode".to_string()),
                    rejected_value: None,
                }],
            },
        )],
        true,
    );

    let before = harness.adapter.layout_preferences();
    assert!(before.capability_available);
    assert_eq!(
        before.lane_sidebar_mode.as_deref(),
        Some("pinned"),
        "the rendered mode is the published record, not a caller default"
    );
    assert_eq!(
        before.hidden_statusbar_segments,
        vec!["latency".to_string(), "req".to_string()]
    );

    let result = harness
        .adapter
        .send_layout_preference_intent_and_wait(
            "gui-layout-1",
            LayoutPreferenceIntent::Set {
                patch: LayoutPreferencePatchInput {
                    lane_sidebar_mode: Some("floating".to_string()),
                    hidden_statusbar_segments: None,
                },
            },
            TIMEOUT,
        )
        .expect("the patch is sent");
    assert_eq!(result.outcome.state, "confirmed");
    assert_eq!(
        result.persisted,
        Some(false),
        "a record Core could not write says so"
    );
    assert_eq!(
        result.diagnostics.len(),
        1,
        "Core's own reason travels with it"
    );
    assert_eq!(result.lane_sidebar_mode.as_deref(), Some("floating"));

    let commands = harness.sent.lock().expect("sent lock").clone();
    let patch = commands
        .iter()
        .find_map(|envelope| match &envelope.command {
            RuntimeCommand::SetUiLayoutPreferences { patch } => Some(patch.clone()),
            _ => None,
        })
        .expect("one SetUiLayoutPreferences left the client");
    assert_eq!(patch.lane_sidebar_mode, Some(LaneSidebarMode::Floating));
    assert_eq!(
        patch.hidden_statusbar_segments, None,
        "an untouched axis is omitted rather than rewritten"
    );
}

#[test]
fn an_absent_layout_capability_sends_nothing_and_names_itself() {
    let mut harness = harness(view_with_workspace_owner(), Vec::new(), false);
    let projection = harness.adapter.layout_preferences();
    assert!(!projection.capability_available);
    assert_eq!(
        projection.lane_sidebar_mode, None,
        "no record is published, so none is claimed"
    );

    let error = harness
        .adapter
        .send_layout_preference_intent_and_wait(
            "gui-layout-2",
            LayoutPreferenceIntent::Reset,
            TIMEOUT,
        )
        .expect_err("a patch without the capability is refused before it is sent");
    assert!(
        error.contains(LAYOUT_PREFERENCES_CAPABILITY),
        "the refusal names the capability: {error}"
    );
    assert!(
        harness
            .sent
            .lock()
            .expect("sent lock")
            .iter()
            .all(|envelope| !matches!(
                envelope.command,
                RuntimeCommand::SetUiLayoutPreferences { .. }
                    | RuntimeCommand::ResetUiLayoutPreferences
            )),
        "nothing is sent"
    );
}
