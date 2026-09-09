use std::sync::{Arc, Mutex};

use serde::Deserialize;
use viden_core::{
    AgentLaneRecord, AgentRole, AgentRoute, AgentTaskKind, AgentTaskRecord, AgentTaskStatus,
    ApprovalRequestView, CheckRunStatus, CheckRunView, EvidenceView, FRONTEND_SCHEMA_V1,
    LaneRuntimeOwnerBinding, QueuedInputView, RuntimeCommand, RuntimeEvent, RuntimeEventEnvelope,
    RuntimeEventKind, RuntimeOwner, RuntimeSnapshot, RuntimeViewState, RuntimeWireEvent,
    ToolCallView, WorkspaceChangeKind, WorkspaceChangeView,
};
use viden_gui::{D1Intent, D6State, GuiCoreAdapter};

mod support;
use support::{TestCoreClient, TestOwner};

const D1_FIXTURE: &str = include_str!(
    "../../../crates/types/tests/fixtures/frontend-contract-v1/d1-vertical-slice.json"
);
const D1_MAIN_FIXTURE: &str =
    include_str!("../../../crates/types/tests/fixtures/frontend-contract-v1/d1-main-cockpit.json");

#[derive(Deserialize)]
struct Fixture {
    initial_snapshot: RuntimeSnapshot,
    events: Vec<RuntimeEventEnvelope>,
}

fn owner(lane_id: &str) -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace_contract_v1".into(),
        project_id: "project_viden".into(),
        lane_id: Some(lane_id.into()),
        session_id: Some("session_lane_d1_core".into()),
        task_id: Some("task_d1_core".into()),
        turn_id: Some("turn_d1_core".into()),
    }
}

fn test_owner(owner: &RuntimeOwner) -> TestOwner {
    TestOwner {
        workspace_id: owner.workspace_id.clone(),
        project_id: owner.project_id.clone(),
        lane_id: owner.lane_id.clone(),
        session_id: owner.session_id.clone(),
        task_id: owner.task_id.clone(),
        turn_id: owner.turn_id.clone(),
    }
}

fn d1_view() -> RuntimeViewState {
    let fixture: Fixture = serde_json::from_str(D1_FIXTURE).expect("parse D1 fixture");
    let mut view = RuntimeViewState::new(fixture.initial_snapshot);
    for envelope in fixture.events {
        if let RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
    }
    view.lane_runtime_owners.push(LaneRuntimeOwnerBinding {
        lane_id: "lane_d1_core".into(),
        owner: owner("lane_d1_core"),
    });
    view
}

fn d1_main_view() -> RuntimeViewState {
    let fixture: Fixture = serde_json::from_str(D1_MAIN_FIXTURE).expect("parse D1 main fixture");
    let mut view = RuntimeViewState::new(fixture.initial_snapshot);
    for envelope in fixture.events {
        if let RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
    }
    view
}

fn approval_from_fixture() -> ApprovalRequestView {
    let fixture: serde_json::Value =
        serde_json::from_str(D1_FIXTURE).expect("parse approval fixture");
    let event = fixture["events"]
        .as_array()
        .expect("fixture events")
        .iter()
        .find(|event| event["event"]["kind"]["type"] == "approval_requested")
        .expect("approval event");
    serde_json::from_value(event["event"]["kind"]["payload"]["approval"].clone())
        .expect("typed approval")
}

fn connected(
    view: RuntimeViewState,
    sent: Arc<Mutex<Vec<viden_core::RuntimeCommandEnvelope>>>,
) -> GuiCoreAdapter {
    let mut adapter = GuiCoreAdapter::new(Box::new(TestCoreClient::new(view, sent)));
    adapter.connect().expect("connect D1 client");
    adapter
}

#[test]
fn canonical_d1_projects_cockpit_regions_only_from_the_core_view() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let adapter = connected(d1_view(), sent);

    let projection = adapter
        .d1_cockpit(Some("lane_d1_core"))
        .expect("D1 projection");

    assert_eq!(projection.selected_lane_id.as_deref(), Some("lane_d1_core"));
    assert_eq!(projection.lanes.len(), 1);
    assert_eq!(projection.lanes[0].status, "running");
    assert_eq!(projection.environment.cwd, "workspace/viden");
    assert_eq!(projection.environment.provider_id, "deepseek");
    assert_eq!(projection.environment.model, "deepseek-v4-flash");
    assert!(projection.live_work.tasks.is_empty());
    assert!(projection.live_work.evidence.is_empty());
    assert!(projection.composer.editable);
    assert!(projection.composer.can_cancel);
    assert!(!projection.composer.can_submit_immediately);
    assert!(
        projection
            .transcript
            .iter()
            .all(|row| row.kind != "assistant_stream")
    );
    assert_eq!(projection.preferences.locale, "zh-CN");

    for feature in &projection.unavailable_features {
        assert!(!feature.available);
        assert!(feature.code.starts_with("GUI-CORE-"));
    }
    assert_eq!(
        projection
            .unavailable_features
            .iter()
            .map(|feature| feature.id)
            .collect::<Vec<_>>(),
        // `audit` is deliberately absent: Core's append-only audit timeline
        // closed GUI-CORE-014 and GUI-CORE-024, so D1 no longer declares it
        // missing. `diff` is absent for the same reason as of
        // `runtime.structured_diff`, which this fixture's Core advertises:
        // the row said Core publishes only an opaque patch string, and that
        // stopped being true. Every remaining code must name an open register
        // entry or a documented client-local reason.
        vec![
            "apply",
            "recovery",
            "transcript_user",
            "transcript_assistant",
        ]
    );
}

#[test]
fn terminal_native_lane_without_an_owner_disables_the_composer() {
    let mut view = d1_view();
    view.lane_runtime_owners.clear();
    view.lanes[0].status = viden_core::LaneStatus::Cancelled;
    let adapter = connected(view, Arc::new(Mutex::new(Vec::new())));

    let projection = adapter
        .d1_cockpit(Some("lane_d1_core"))
        .expect("D1 projection");

    assert!(!projection.composer.editable);
    assert!(!projection.composer.can_submit_immediately);
}

#[test]
fn d1_cockpit_preserves_typed_workspace_patches_for_the_selected_lane() {
    let mut view = d1_main_view();
    view.workspace_changes[0].patch = Some("@@ typed patch @@".into());
    let projection = connected(view, Arc::new(Mutex::new(Vec::new())))
        .d1_cockpit(Some("lane-d1-main"))
        .expect("D1 main projection");
    let wire = serde_json::to_value(projection).expect("serialize D1 projection");

    assert!(
        wire["contextDock"]["checklist"]
            .as_array()
            .expect("typed checklist")
            .iter()
            .any(|item| item["kind"] == "workspace_change" && item["patch"] == "@@ typed patch @@"),
        "a typed WorkspaceChange patch must remain available to the GUI renderer",
    );
}

#[test]
fn d1_cockpit_context_dock_enforces_zero_one_or_duplicate_lane_agent_cardinality() {
    let mut zero = d1_main_view();
    zero.lane_runtime_owners.clear();
    let zero = connected(zero, Arc::new(Mutex::new(Vec::new())))
        .d1_cockpit(Some("lane-d1-main"))
        .expect("zero-owner projection");
    assert!(zero.context_dock.lane_agent.is_none());

    let one = connected(d1_main_view(), Arc::new(Mutex::new(Vec::new())))
        .d1_cockpit(Some("lane-d1-main"))
        .expect("one-owner projection");
    let one = one.context_dock.lane_agent.expect("exact Lane Agent");
    assert_eq!(one.lane_id, "lane-d1-main");
    assert_eq!(one.session_id.as_deref(), Some("agent-session-d1-main"));

    let mut duplicate = d1_main_view();
    duplicate.lane_runtime_owners.push(LaneRuntimeOwnerBinding {
        lane_id: "lane-d1-main".into(),
        owner: RuntimeOwner {
            session_id: Some("duplicate-session".into()),
            turn_id: Some("duplicate-turn".into()),
            ..duplicate.lane_runtime_owners[0].owner.clone()
        },
    });
    let duplicate = connected(duplicate, Arc::new(Mutex::new(Vec::new())))
        .d1_cockpit(Some("lane-d1-main"))
        .expect("duplicate-owner recovery projection");
    assert!(duplicate.context_dock.lane_agent.is_none());
    assert_eq!(duplicate.recovery.state, D6State::EventGap);
    assert_eq!(
        duplicate.recovery.detail.as_deref(),
        Some(viden_gui::D1_OWNER_CARDINALITY_CODE)
    );
}

#[test]
fn d1_cockpit_context_dock_switches_only_owner_scoped_facts_for_the_selected_lane() {
    let mut view = d1_main_view();
    let second_lane_id = "lane-review".to_string();
    let second_task_id = "task-review".to_string();
    let mut second_lane: AgentLaneRecord = view.lanes[0].clone();
    second_lane.id = second_lane_id.clone();
    second_lane.task_id = Some(second_task_id.clone());
    second_lane.active_session_ids = vec!["session-review".into()];
    view.lanes.push(second_lane);
    let second_owner = RuntimeOwner {
        workspace_id: "workspace-d1-main".into(),
        project_id: "project-viden".into(),
        lane_id: Some(second_lane_id.clone()),
        session_id: Some("session-review".into()),
        task_id: Some(second_task_id.clone()),
        turn_id: Some("turn-review".into()),
    };
    view.lane_runtime_owners.push(LaneRuntimeOwnerBinding {
        lane_id: second_lane_id.clone(),
        owner: second_owner.clone(),
    });
    view.workspace_changes.push(WorkspaceChangeView {
        id: "change-review".into(),
        owner: second_owner.clone(),
        path: "apps/gui/src/review.ts".into(),
        kind: WorkspaceChangeKind::Added,
        patch: None,
        additions: 7,
        deletions: 0,
        diff: None,
    });
    view.check_runs.push(CheckRunView {
        id: "check-review".into(),
        owner: second_owner,
        label: "review check".into(),
        command: "cargo test -p viden-gui".into(),
        status: CheckRunStatus::Passed,
        summary: "passed".into(),
        failing_location: None,
    });

    let adapter = connected(view, Arc::new(Mutex::new(Vec::new())));
    let first = adapter
        .d1_cockpit(Some("lane-d1-main"))
        .expect("first Lane");
    assert_eq!(
        first
            .context_dock
            .checklist
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["change-runtime-types", "check-viden-types"]
    );

    let second = adapter
        .d1_cockpit(Some("lane-review"))
        .expect("second Lane");
    assert_eq!(
        second
            .context_dock
            .lane_agent
            .as_ref()
            .and_then(|agent| agent.session_id.as_deref()),
        Some("session-review")
    );
    assert!(second.context_dock.context.is_none());
    assert_eq!(
        second
            .context_dock
            .checklist
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["change-review", "check-review"]
    );
}

#[test]
fn d1_cockpit_preserves_an_explicit_stale_selection_without_falling_back() {
    let projection = connected(d1_main_view(), Arc::new(Mutex::new(Vec::new())))
        .d1_cockpit(Some("lane-removed"))
        .expect("stale D1 projection");

    assert_eq!(projection.selected_lane_id.as_deref(), Some("lane-removed"));
    assert!(projection.context_dock.lane_agent.is_none());
    assert!(!projection.composer.can_cancel);
}

#[test]
fn d1_busy_uses_the_exact_owner_turn_fact_not_lane_lifecycle_inference() {
    let mut view = d1_view();
    view.lane_runtime_owners[0].owner.turn_id = None;
    view.lanes[0].status = viden_core::LaneStatus::Running;

    let projection = connected(view, Arc::new(Mutex::new(Vec::new())))
        .d1_cockpit(Some("lane_d1_core"))
        .expect("D1 projection");
    assert!(!projection.composer.busy);
    assert!(projection.composer.can_submit_immediately);
}

#[test]
fn d1_permission_dock_never_falls_back_to_another_lane_without_one_exact_owner() {
    let mut view = d1_view();
    let mut approval = approval_from_fixture();
    approval.owner = RuntimeOwner {
        lane_id: Some("lane-other".into()),
        ..approval.owner
    };
    view.pending_approvals.push(approval);

    let adapter = connected(view.clone(), Arc::new(Mutex::new(Vec::new())));
    assert!(
        adapter
            .d1_cockpit(Some("lane_d1_core"))
            .expect("exact owner projection")
            .permission_dock
            .request
            .is_none()
    );
    assert!(
        adapter
            .d1_cockpit(Some("lane-removed"))
            .expect("stale projection")
            .permission_dock
            .request
            .is_none()
    );

    view.lane_runtime_owners.push(LaneRuntimeOwnerBinding {
        lane_id: "lane_d1_core".into(),
        owner: RuntimeOwner {
            turn_id: Some("duplicate".into()),
            ..owner("lane_d1_core")
        },
    });
    assert!(
        connected(view, Arc::new(Mutex::new(Vec::new())))
            .d1_cockpit(Some("lane_d1_core"))
            .expect("duplicate projection")
            .permission_dock
            .request
            .is_none()
    );
}

#[test]
fn d1_cockpit_scopes_transcript_and_live_work_to_the_selected_lane_or_omits_unowned_facts() {
    let mut view = d1_main_view();
    let mut review_lane = view.lanes[0].clone();
    review_lane.id = "lane-review".into();
    view.lanes.push(review_lane);
    view.lane_runtime_owners.push(LaneRuntimeOwnerBinding {
        lane_id: "lane-review".into(),
        owner: RuntimeOwner {
            session_id: Some("session-review".into()),
            turn_id: Some("turn-review".into()),
            ..owner("lane-review")
        },
    });
    view.apply_event(&RuntimeEvent::with_timestamp(
        9_001,
        Some(1),
        RuntimeEventKind::LaneOutputAppended {
            lane_id: "lane-d1-main".into(),
            stream: "stdout".into(),
            content: "main output".into(),
        },
    ));
    view.apply_event(&RuntimeEvent::with_timestamp(
        9_002,
        Some(2),
        RuntimeEventKind::LaneOutputAppended {
            lane_id: "lane-review".into(),
            stream: "stdout".into(),
            content: "review output".into(),
        },
    ));
    view.active_tool_calls = vec![ToolCallView {
        tool_call_id: "global-tool".into(),
        name: "shell".into(),
        input_preview: "must not leak".into(),
        owner: None,
    }];

    let adapter = connected(view, Arc::new(Mutex::new(Vec::new())));
    let main = adapter
        .d1_cockpit(Some("lane-d1-main"))
        .expect("main Lane projection");
    assert!(
        main.transcript
            .iter()
            .any(|row| row.content == "main output")
    );
    assert!(
        !main
            .transcript
            .iter()
            .any(|row| row.content == "review output")
    );
    assert!(main.live_work.tools.is_empty());

    let review = adapter
        .d1_cockpit(Some("lane-review"))
        .expect("review Lane projection");
    assert!(
        review
            .transcript
            .iter()
            .any(|row| row.content == "review output")
    );
    assert!(
        !review
            .transcript
            .iter()
            .any(|row| row.content == "main output")
    );
    assert!(review.live_work.tools.is_empty());
}

/// GUI-CORE-010: with owners on live-work facts, D1 projects the selected
/// Lane's own task, tool call, queued input, and evidence — and never the other
/// live Lane's, nor a fact Core published with no owner.
#[test]
fn d1_cockpit_projects_only_the_selected_owners_live_work() {
    let mut view = d1_main_view();
    let main_owner = view
        .lane_runtime_owners
        .iter()
        .find(|binding| binding.lane_id == "lane-d1-main")
        .map(|binding| binding.owner.clone())
        .expect("the main Lane publishes one exact owner");
    let review_owner = RuntimeOwner {
        session_id: Some("session-review".into()),
        turn_id: Some("turn-review".into()),
        ..owner("lane-review")
    };
    let mut review_lane = view.lanes[0].clone();
    review_lane.id = "lane-review".into();
    view.lanes.push(review_lane);
    view.lane_runtime_owners.push(LaneRuntimeOwnerBinding {
        lane_id: "lane-review".into(),
        owner: review_owner.clone(),
    });

    let task = |id: &str, owner: Option<&RuntimeOwner>| AgentTaskRecord {
        id: id.to_string(),
        parent_id: None,
        role: AgentRole::Coder,
        kind: AgentTaskKind::Job,
        route: AgentRoute::Acp,
        title: format!("{id} title"),
        status: AgentTaskStatus::RunningTool,
        activity: "running".into(),
        summary: format!("{id} summary"),
        progress: 40,
        started_at: None,
        updated_at: None,
        workspace: None,
        evidence: Vec::new(),
        permissions: Vec::new(),
        decision: None,
        result: None,
        resume_handle: None,
        pid: None,
        next_action: None,
        owner: owner.cloned(),
    };
    let tool = |id: &str, owner: Option<&RuntimeOwner>| ToolCallView {
        tool_call_id: id.to_string(),
        name: "shell".into(),
        input_preview: "cargo test".into(),
        owner: owner.cloned(),
    };
    let queued = |id: &str, owner: Option<&RuntimeOwner>| QueuedInputView {
        id: id.to_string(),
        content_preview: format!("{id} preview"),
        created_at: None,
        owner: owner.cloned(),
    };
    let evidence = |id: &str, owner: Option<&RuntimeOwner>| EvidenceView {
        id: id.to_string(),
        kind: "tool_log".into(),
        summary: format!("{id} summary"),
        path: None,
        source: None,
        canonical: None,
        metadata: None,
        timestamp: None,
        owner: owner.cloned(),
    };

    view.tasks = vec![
        task("task-main", Some(&main_owner)),
        task("task-review", Some(&review_owner)),
        task("task-unowned", None),
    ];
    view.active_tool_calls = vec![
        tool("tool-main", Some(&main_owner)),
        tool("tool-review", Some(&review_owner)),
        tool("tool-unowned", None),
    ];
    view.queued_inputs = vec![
        queued("queued-main", Some(&main_owner)),
        queued("queued-review", Some(&review_owner)),
        queued("queued-unowned", None),
    ];
    view.latest_evidence = vec![
        evidence("evidence-main", Some(&main_owner)),
        evidence("evidence-review", Some(&review_owner)),
        evidence("evidence-unowned", None),
    ];

    let adapter = connected(view, Arc::new(Mutex::new(Vec::new())));
    for (lane_id, suffix) in [("lane-d1-main", "main"), ("lane-review", "review")] {
        let projection = adapter.d1_cockpit(Some(lane_id)).expect("Lane projection");
        assert_eq!(
            projection
                .live_work
                .tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec![format!("task-{suffix}")],
            "{lane_id} tasks"
        );
        assert_eq!(
            projection
                .live_work
                .tools
                .iter()
                .map(|tool| tool.id.as_str())
                .collect::<Vec<_>>(),
            vec![format!("tool-{suffix}")],
            "{lane_id} tools"
        );
        assert_eq!(
            projection
                .live_work
                .queued_inputs
                .iter()
                .map(|input| input.id.as_str())
                .collect::<Vec<_>>(),
            vec![format!("queued-{suffix}")],
            "{lane_id} queued inputs"
        );
        assert_eq!(
            projection
                .live_work
                .evidence
                .iter()
                .map(|evidence| evidence.id.as_str())
                .collect::<Vec<_>>(),
            vec![format!("evidence-{suffix}")],
            "{lane_id} evidence"
        );
    }
}

/// A Lane with no exact Core owner projects no live work at all: without an
/// owner to match, every fact would be an attribution the client invented.
#[test]
fn d1_cockpit_projects_no_live_work_without_one_exact_owner() {
    let mut view = d1_main_view();
    view.lane_runtime_owners.clear();
    view.tasks = vec![AgentTaskRecord {
        id: "task-main".into(),
        parent_id: None,
        role: AgentRole::Coder,
        kind: AgentTaskKind::Job,
        route: AgentRoute::Acp,
        title: "task-main".into(),
        status: AgentTaskStatus::RunningTool,
        activity: "running".into(),
        summary: "task-main".into(),
        progress: 40,
        started_at: None,
        updated_at: None,
        workspace: None,
        evidence: Vec::new(),
        permissions: Vec::new(),
        decision: None,
        result: None,
        resume_handle: None,
        pid: None,
        next_action: None,
        owner: Some(owner("lane-d1-main")),
    }];

    let adapter = connected(view, Arc::new(Mutex::new(Vec::new())));
    let projection = adapter
        .d1_cockpit(Some("lane-d1-main"))
        .expect("Lane projection");
    assert!(projection.live_work.tasks.is_empty());
}

#[test]
fn d1_cockpit_context_dock_uses_typed_empty_states_and_never_parses_display_text() {
    let mut view = d1_view();
    view.workspace_source = None;
    view.runtime_services.clear();
    view.workspace_changes.clear();
    view.check_runs.clear();
    view.context_budgets.clear();
    view.provider = None;
    view.lane_runtime_owners.clear();
    view.assistant_stream =
        "branch main, provider connected, context 91%, tests passed, one Lane Agent".into();

    let projection = connected(view, Arc::new(Mutex::new(Vec::new())))
        .d1_cockpit(Some("lane_d1_core"))
        .expect("typed empty projection");

    assert!(projection.context_dock.source.is_none());
    assert!(projection.context_dock.context.is_none());
    assert!(projection.context_dock.lane_agent.is_none());
    assert!(projection.context_dock.provider.is_none());
    assert!(projection.context_dock.services.is_empty());
    assert!(projection.context_dock.checklist.is_empty());
}

#[test]
fn enter_queues_with_the_exact_core_owner_while_lane_is_busy() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = connected(d1_view(), Arc::clone(&sent));

    adapter
        .send_d1_intent(
            "queue-d1",
            D1Intent::Submit {
                lane_id: "lane_d1_core".into(),
                content: "继续验证".into(),
            },
        )
        .expect("queue follow-up");

    let sent = sent.lock().expect("sent lock");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].schema_version, FRONTEND_SCHEMA_V1);
    assert_eq!(sent[0].owner, owner("lane_d1_core"));
    assert!(matches!(
        &sent[0].command,
        RuntimeCommand::QueueFollowUp { content } if content == "继续验证"
    ));
}

#[test]
fn cancel_uses_one_exact_active_binding_and_never_falls_back() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = connected(d1_view(), Arc::clone(&sent));

    adapter
        .send_d1_intent(
            "cancel-d1",
            D1Intent::Cancel {
                lane_id: "lane_d1_core".into(),
            },
        )
        .expect("cancel exact owner");
    assert_eq!(
        sent.lock().expect("sent lock")[0].owner,
        owner("lane_d1_core")
    );

    let mut missing = d1_view();
    missing.lane_runtime_owners.clear();
    let missing_sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = connected(missing, Arc::clone(&missing_sent));
    let error = adapter
        .send_d1_intent(
            "cancel-missing",
            D1Intent::Cancel {
                lane_id: "lane_d1_core".into(),
            },
        )
        .expect_err("missing owner must fail closed");
    assert!(error.contains("exact Core runtime owner"));
    assert!(missing_sent.lock().expect("missing sent lock").is_empty());

    let mut ambiguous = d1_view();
    ambiguous.lane_runtime_owners.push(LaneRuntimeOwnerBinding {
        lane_id: "lane_d1_core".into(),
        owner: RuntimeOwner {
            // The raw binding still claims this Lane, but its nested owner is
            // malformed. Raw same-Lane cardinality must reject this before a
            // cancel can select the well-formed first binding.
            lane_id: Some("other-lane".into()),
            ..owner("lane_d1_core")
        },
    });
    let ambiguous_sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = connected(ambiguous, Arc::clone(&ambiguous_sent));
    assert!(
        adapter
            .send_d1_intent(
                "cancel-ambiguous",
                D1Intent::Cancel {
                    lane_id: "lane_d1_core".into(),
                },
            )
            .expect_err("ambiguous owner must fail closed")
            .contains("exact Core runtime owner")
    );
    assert!(
        ambiguous_sent
            .lock()
            .expect("ambiguous sent lock")
            .is_empty()
    );
}

#[test]
fn submit_requires_a_current_sole_runtime_binding() {
    let mut view = d1_view();
    view.lane_runtime_owners.clear();
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = connected(view, Arc::clone(&sent));

    let error = adapter
        .send_d1_intent(
            "submit-without-runtime-owner",
            D1Intent::Submit {
                lane_id: "lane_d1_core".into(),
                content: "must not use a starter receipt".into(),
            },
        )
        .expect_err("submit without a current runtime owner must fail closed");

    assert!(error.contains("one exact Core submit owner"));
    assert!(sent.lock().expect("sent lock").is_empty());
}

#[test]
fn cancel_rejects_inactive_or_owner_mismatched_lanes_without_transport() {
    let mut inactive = d1_view();
    inactive.lanes[0].status = viden_core::LaneStatus::Done;
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = connected(inactive, Arc::clone(&sent));
    assert!(
        adapter
            .send_d1_intent(
                "cancel-inactive",
                D1Intent::Cancel {
                    lane_id: "lane_d1_core".into(),
                },
            )
            .expect_err("inactive Lane must fail closed")
            .contains("active")
    );
    assert!(sent.lock().expect("sent lock").is_empty());

    let mut mismatch = d1_view();
    mismatch.lane_runtime_owners[0].owner.lane_id = Some("other-lane".into());
    let sent = Arc::new(Mutex::new(Vec::new()));
    let adapter = connected(mismatch, sent);
    let projection = adapter
        .d1_cockpit(Some("lane_d1_core"))
        .expect("projection");
    assert!(!projection.composer.can_cancel);
}

#[test]
fn d1_wire_is_camel_case_and_intents_are_closed() {
    let intent: D1Intent = serde_json::from_value(serde_json::json!({
        "type": "submit",
        "laneId": "lane_d1_core",
        "content": "next"
    }))
    .expect("D1 intent wire");
    assert!(matches!(intent, D1Intent::Submit { lane_id, .. } if lane_id == "lane_d1_core"));

    let adapter = connected(d1_view(), Arc::new(Mutex::new(Vec::new())));
    let wire = serde_json::to_value(adapter.d1_cockpit(Some("lane_d1_core")).unwrap())
        .expect("serialize D1");
    assert_eq!(wire["selectedLaneId"], "lane_d1_core");
    assert!(wire["liveWork"].is_object());
    assert!(wire["unavailableFeatures"].is_array());
}

#[test]
fn d1_default_lane_intent_sends_core_generated_preview_command() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = connected(d1_view(), Arc::clone(&sent));

    adapter
        .send_d1_intent(
            "preview-default",
            D1Intent::PreviewDefaultLane {
                preset: "coder".into(),
            },
        )
        .expect("preview default Lane");

    assert!(matches!(
        sent.lock().expect("sent")[0].command,
        RuntimeCommand::PreviewDefaultStarterLane {
            preset: viden_core::StarterLanePreset::Coder
        }
    ));
}

#[test]
fn d1_acp_follow_up_preserves_exact_session_and_owner() {
    let mut view = d1_view();
    let session_owner = RuntimeOwner {
        session_id: Some("acp-1".into()),
        turn_id: Some("acp-turn-1".into()),
        ..owner("lane_d1_core")
    };
    view.lane_runtime_owners[0].owner = session_owner.clone();
    view.agent_sessions.push(viden_core::AgentSessionView {
        session_id: "acp-1".into(),
        lane_id: "lane_d1_core".into(),
        agent_id: "codex-acp".into(),
        model: None,
        status: viden_core::AgentSessionStatus::Running,
        owner: session_owner.clone(),
        task: "review".into(),
        diagnostic: None,
        output: None,
    });
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = connected(view, Arc::clone(&sent));

    adapter
        .send_d1_intent(
            "acp-input",
            D1Intent::SendAgentSessionInput {
                lane_id: "lane_d1_core".into(),
                session_id: "acp-1".into(),
                content: "continue".into(),
            },
        )
        .expect("send ACP follow-up");

    let sent = sent.lock().expect("sent");
    assert_eq!(sent[0].owner, session_owner);
    assert!(matches!(
        &sent[0].command,
        RuntimeCommand::SendAgentSessionInput { input }
            if input.session_id == "acp-1" && input.content == "continue"
    ));
}

#[test]
fn restored_completed_acp_session_accepts_follow_up_without_a_live_lane_binding() {
    let mut view = d1_view();
    let session_owner = RuntimeOwner {
        session_id: Some("acp-restored".into()),
        turn_id: None,
        ..owner("lane_d1_core")
    };
    view.lane_runtime_owners.clear();
    view.lanes[0].status = viden_core::LaneStatus::Done;
    view.agent_sessions.push(viden_core::AgentSessionView {
        session_id: "acp-restored".into(),
        lane_id: "lane_d1_core".into(),
        agent_id: "codex-acp".into(),
        model: None,
        status: viden_core::AgentSessionStatus::Completed,
        owner: session_owner.clone(),
        task: "review".into(),
        diagnostic: None,
        output: Some("finished the previous turn".into()),
    });
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = connected(view, Arc::clone(&sent));

    adapter
        .send_d1_intent(
            "acp-restored-input",
            D1Intent::SendAgentSessionInput {
                lane_id: "lane_d1_core".into(),
                session_id: "acp-restored".into(),
                content: "continue after restart".into(),
            },
        )
        .expect("resume the sole restored ACP session");

    let sent = sent.lock().expect("sent");
    assert_eq!(sent[0].owner, session_owner);
    assert!(matches!(
        &sent[0].command,
        RuntimeCommand::SendAgentSessionInput { input }
            if input.session_id == "acp-restored"
                && input.content == "continue after restart"
    ));
}

#[test]
fn reactivated_restored_acp_session_stays_visible_while_running() {
    let mut view = d1_view();
    let session_owner = RuntimeOwner {
        session_id: Some("acp-restored".into()),
        turn_id: None,
        ..owner("lane_d1_core")
    };
    view.lane_runtime_owners.clear();
    view.lanes[0].status = viden_core::LaneStatus::Draft;
    let mut session = viden_core::AgentSessionView {
        session_id: "acp-restored".into(),
        lane_id: "lane_d1_core".into(),
        agent_id: "codex-acp".into(),
        model: None,
        status: viden_core::AgentSessionStatus::Completed,
        owner: session_owner.clone(),
        task: "previous turn".into(),
        diagnostic: None,
        output: Some("finished".into()),
    };
    view.agent_sessions.push(session.clone());
    view.apply_event(&RuntimeEvent::new(
        0,
        RuntimeEventKind::LaneRuntimeOwnerBound {
            binding: LaneRuntimeOwnerBinding {
                lane_id: "lane_d1_core".into(),
                owner: session_owner,
            },
        },
    ));
    session.status = viden_core::AgentSessionStatus::Starting;
    session.task = "continue after restart".into();
    session.output = None;
    view.apply_event(&RuntimeEvent::new(
        0,
        RuntimeEventKind::AgentSessionStarted { session },
    ));

    let projection = connected(view, Arc::new(Mutex::new(Vec::new())))
        .d1_cockpit(Some("lane_d1_core"))
        .expect("reactivated ACP projection");

    assert!(projection.composer.editable);
    assert!(projection.composer.busy);
    assert_eq!(projection.agent_sessions.len(), 1);
    assert_eq!(projection.agent_sessions[0].status, "starting");
    assert_eq!(
        projection
            .context_dock
            .lane_agent
            .unwrap()
            .session_id
            .as_deref(),
        Some("acp-restored")
    );
}

#[test]
fn acp_input_rejects_a_malformed_duplicate_lane_binding_without_transport() {
    let mut view = d1_view();
    let session_owner = RuntimeOwner {
        session_id: Some("acp-duplicate".into()),
        turn_id: Some("acp-turn".into()),
        ..owner("lane_d1_core")
    };
    view.lane_runtime_owners[0].owner = session_owner.clone();
    view.lane_runtime_owners.push(LaneRuntimeOwnerBinding {
        lane_id: "lane_d1_core".into(),
        owner: RuntimeOwner {
            lane_id: Some("wrong-inner-lane".into()),
            ..session_owner.clone()
        },
    });
    view.agent_sessions.push(viden_core::AgentSessionView {
        session_id: "acp-duplicate".into(),
        lane_id: "lane_d1_core".into(),
        agent_id: "codex-acp".into(),
        model: None,
        status: viden_core::AgentSessionStatus::Running,
        owner: session_owner,
        task: "review".into(),
        diagnostic: None,
        output: None,
    });
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = connected(view, Arc::clone(&sent));

    assert!(
        adapter
            .send_d1_intent(
                "acp-duplicate",
                D1Intent::SendAgentSessionInput {
                    lane_id: "lane_d1_core".into(),
                    session_id: "acp-duplicate".into(),
                    content: "do not route".into(),
                },
            )
            .expect_err("malformed duplicate must fail closed")
            .contains("one exact Core ACP owner")
    );
    assert!(sent.lock().expect("sent").is_empty());
}

#[test]
fn busy_acp_follow_up_queues_through_the_selected_exact_owner() {
    let mut view = d1_view();
    let acp_owner = RuntimeOwner {
        session_id: Some("acp-queue".into()),
        turn_id: Some("acp-queue-turn".into()),
        ..owner("lane_d1_core")
    };
    view.lane_runtime_owners[0].owner = acp_owner.clone();
    view.agent_sessions.push(viden_core::AgentSessionView {
        session_id: "acp-queue".into(),
        lane_id: "lane_d1_core".into(),
        agent_id: "codex-acp".into(),
        model: None,
        status: viden_core::AgentSessionStatus::Running,
        owner: acp_owner.clone(),
        task: "review".into(),
        diagnostic: None,
        output: None,
    });
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = connected(view, Arc::clone(&sent));

    adapter
        .send_d1_intent(
            "queue-acp",
            D1Intent::Submit {
                lane_id: "lane_d1_core".into(),
                content: "queue this ACP follow-up".into(),
            },
        )
        .expect("queue busy ACP through Core owner");

    let sent = sent.lock().expect("sent");
    assert_eq!(sent[0].owner, acp_owner);
    assert!(matches!(
        &sent[0].command,
        RuntimeCommand::QueueFollowUp { content } if content == "queue this ACP follow-up"
    ));
}

#[test]
fn d1_projects_core_eligibility_and_truthful_acp_startability() {
    let mut view = d1_view();
    view.workspace_eligibility = Some(viden_core::WorkspaceEligibility {
        is_git_repository: true,
        has_head: true,
        can_create_lane: true,
        diagnostic: None,
    });
    view.agent_adapters.push(viden_core::AgentAdapterView {
        agent_id: "codex-acp".into(),
        display_name: "Codex".into(),
        route: viden_core::AgentRoute::Acp,
        source: viden_core::AgentAdapterSource::Registry,
        availability: viden_core::AgentAvailability::Available,
        auth_state: viden_core::AgentAuthState::Ready,
        startability: viden_core::AgentStartability::AuthenticationRequired,
        capabilities: Vec::new(),
        models: Vec::new(),
        diagnostics: vec!["sign in".into()],
    });
    let adapter = connected(view, Arc::new(Mutex::new(Vec::new())));

    let projection = adapter.d1_cockpit(Some("lane_d1_core")).unwrap();

    assert!(
        projection
            .workspace_eligibility
            .is_some_and(|eligibility| eligibility.can_create_lane)
    );
    assert_eq!(
        projection.agent_adapters[0].startability,
        "authentication_required"
    );
    assert_eq!(projection.agent_adapters[0].diagnostics, vec!["sign in"]);
}

#[test]
fn accepted_queue_stays_pending_until_the_matching_ordered_business_fact() {
    let runtime_owner = owner("lane_d1_core");
    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = TestCoreClient::new(d1_view(), Arc::clone(&sent)).with_owned_event(
        test_owner(&runtime_owner),
        RuntimeEventKind::CommandAccepted {
            command_id: "queue-pending".into(),
            command: RuntimeCommand::QueueFollowUp {
                content: "keep this draft".into(),
            },
        },
    );
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D1 client");

    let result = adapter
        .send_d1_intent_and_wait(
            "queue-pending",
            D1Intent::Submit {
                lane_id: "lane_d1_core".into(),
                content: "keep this draft".into(),
            },
            std::time::Duration::ZERO,
        )
        .expect("accepted queue remains pending");

    assert_eq!(result.pending_command_id.as_deref(), Some("queue-pending"));
    assert_eq!(result.outcome.state, "pending");
}

#[test]
fn accepted_queue_confirms_only_after_same_owner_input_queued() {
    let runtime_owner = owner("lane_d1_core");
    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = TestCoreClient::new(d1_view(), Arc::clone(&sent))
        .with_owned_event(
            test_owner(&runtime_owner),
            RuntimeEventKind::CommandAccepted {
                command_id: "queue-confirmed".into(),
                command: RuntimeCommand::QueueFollowUp {
                    content: "confirmed input".into(),
                },
            },
        )
        .with_owned_event(
            test_owner(&runtime_owner),
            RuntimeEventKind::InputQueued {
                input: QueuedInputView {
                    id: "queued-1".into(),
                    content_preview: "confirmed input".into(),
                    created_at: Some(1),
                    owner: None,
                },
            },
        );
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D1 client");

    let result = adapter
        .send_d1_intent_and_wait(
            "queue-confirmed",
            D1Intent::Submit {
                lane_id: "lane_d1_core".into(),
                content: "confirmed input".into(),
            },
            std::time::Duration::ZERO,
        )
        .expect("matching Core fact confirms queue");

    assert_eq!(result.pending_command_id, None);
    assert_eq!(result.outcome.state, "confirmed");
}

#[test]
fn exact_command_rejection_finishes_pending_with_the_core_reason() {
    let runtime_owner = owner("lane_d1_core");
    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = TestCoreClient::new(d1_view(), Arc::clone(&sent)).with_owned_event(
        test_owner(&runtime_owner),
        RuntimeEventKind::CommandRejected {
            command_id: "queue-rejected".into(),
            reason: "Core denied the queue".into(),
        },
    );
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect D1 client");

    let result = adapter
        .send_d1_intent_and_wait(
            "queue-rejected",
            D1Intent::Submit {
                lane_id: "lane_d1_core".into(),
                content: "do not clear me".into(),
            },
            std::time::Duration::ZERO,
        )
        .expect("Core rejection is an ordered result");

    assert_eq!(result.pending_command_id, None);
    assert_eq!(result.outcome.state, "rejected");
    assert_eq!(
        result.outcome.reason.as_deref(),
        Some("Core denied the queue")
    );
}

#[test]
fn same_timestamp_lane_outputs_receive_distinct_projection_row_ids() {
    let mut view = d1_view();
    for (sequence, content) in [(100, "first"), (101, "second")] {
        let mut event = RuntimeEvent::new(
            sequence,
            RuntimeEventKind::LaneOutputAppended {
                lane_id: "lane_d1_core".into(),
                stream: "stdout".into(),
                content: content.into(),
            },
        );
        event.timestamp = Some(42);
        view.apply_event(&event);
    }
    let adapter = connected(view, Arc::new(Mutex::new(Vec::new())));

    let projection = adapter.d1_cockpit(Some("lane_d1_core")).unwrap();
    let rows = projection
        .transcript
        .iter()
        .filter(|row| row.kind == "lane_output")
        .collect::<Vec<_>>();

    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0].id, rows[1].id);
    assert_eq!(rows[0].content, "first");
    assert_eq!(rows[1].content, "second");
}
