use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::{collections::BTreeMap, fs};

use viden_context::{ContextEngine, ContextPutRequest};
use viden_provider::ModelProvider;
use viden_session::SessionStore;
use viden_types::{
    AgentConversationRole, AgentDagTaskSpec, AgentRole, AgentRoute, AgentSessionStatus,
    AgentSessionView, AgentTaskKind, AgentTaskStatus, ApprovalDecision, ApprovalResponse,
    CanonicalEvidenceReference, CapabilityId, ContextContentKind, ContextHandleRecord,
    ContextItemRecord, ContextReductionRecord, ContextScope, CostScope, EvidenceProducer,
    EvidenceQualityFacts, EvidenceQualityStatus, EvidenceVerificationState, EvidenceView,
    LaneStatus, MergeGateStatus, ModelEvent, ModelRequest, ModelUsage, PermissionBehavior,
    PermissionLevel, PermissionRule, PermissionRuleSource, PermissionRuleValue, RuntimeCommand,
    RuntimeEvent, RuntimeEventKind, RuntimeOwner, RuntimeViewState, ToolCall, ToolInput,
    TranscriptEntry, TranscriptPageRequest, WorkMode,
};
use viden_workflows::lanes::LaneEvent;
use viden_workflows::stores::{WorkflowAgentEvent, WorkflowStore};

use crate::{
    EngineEvent, SessionEngine,
    context_bundle::{ContextBuildMode, render_provider_context_message},
};

use super::{SequenceProvider, temp_dir};

struct CountingProvider {
    request_count: Arc<AtomicU64>,
    requests: Arc<Mutex<Vec<ModelRequest>>>,
    result: Result<Vec<ModelEvent>, String>,
}

fn stable_context_bundle_bytes(mut bundle: viden_types::ContextBundleRecord) -> Vec<u8> {
    bundle.bundle_id = "<bundle>".to_string();
    bundle.task_id = "<task>".to_string();
    for source in &mut bundle.sources {
        source.handle_id = source.handle_id.as_ref().map(|_| "<handle>".to_string());
        source.item_id = source.item_id.as_ref().map(|_| "<item>".to_string());
        source.view_id = source.view_id.as_ref().map(|_| "<view>".to_string());
        source.quality_id = source.quality_id.as_ref().map(|_| "<quality>".to_string());
    }
    serde_json::to_vec(&bundle).expect("context bundle serializes")
}

fn stable_provider_context_bytes(rendered: &str) -> Vec<u8> {
    rendered
        .lines()
        .map(|line| {
            if let Some(rest) = line.strip_prefix("Bundle: ") {
                let _ = rest;
                return "Bundle: <bundle>".to_string();
            }
            if let Some(rest) = line.strip_prefix("Scope: task:") {
                let _ = rest;
                return "Scope: task:<task>".to_string();
            }
            line.split_whitespace()
                .map(|token| {
                    if token.starts_with("handle=") {
                        "handle=<handle>".to_string()
                    } else if token.starts_with("item=") {
                        "item=<item>".to_string()
                    } else if token.starts_with("view=") {
                        "view=<view>".to_string()
                    } else if token.starts_with("quality=") {
                        "quality=<quality>".to_string()
                    } else {
                        token.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes()
}

fn context_reduction_records(events: &[RuntimeEvent]) -> Vec<ContextReductionRecord> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::ContextReductionRecorded { reduction } => Some(reduction.clone()),
            _ => None,
        })
        .collect()
}

impl CountingProvider {
    fn new(
        request_count: Arc<AtomicU64>,
        requests: Arc<Mutex<Vec<ModelRequest>>>,
        result: Result<Vec<ModelEvent>, String>,
    ) -> Self {
        Self {
            request_count,
            requests,
            result,
        }
    }
}

impl ModelProvider for CountingProvider {
    fn provider_name(&self) -> &str {
        "counting"
    }

    fn model(&self) -> &str {
        "counting-model"
    }

    fn set_model(&mut self, _model: String) {}

    fn next_events(&mut self, request: &ModelRequest) -> Result<Vec<ModelEvent>, String> {
        self.request_count.fetch_add(1, Ordering::SeqCst);
        self.requests.lock().unwrap().push(request.clone());
        self.result.clone()
    }
}

fn assert_strictly_increasing_sequences(events: &[viden_types::RuntimeEvent]) {
    for pair in events.windows(2) {
        assert!(
            pair[0].sequence < pair[1].sequence,
            "runtime event sequence must increase: {} then {}",
            pair[0].sequence,
            pair[1].sequence
        );
    }
}

fn single_cost<'a>(
    events: &'a [viden_types::RuntimeEvent],
    provider_id: &str,
) -> &'a viden_types::CostUsageRecord {
    let costs = events
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::CostUsageRecorded { cost } if cost.provider_id == provider_id => {
                Some(cost)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(costs.len(), 1, "expected one {provider_id} cost event");
    costs[0]
}

fn assert_scope_once(cost: &viden_types::CostUsageRecord, scope: CostScope) {
    assert_eq!(
        cost.scopes
            .iter()
            .filter(|candidate| **candidate == scope)
            .count(),
        1,
        "scope {scope:?} should appear exactly once in {:?}",
        cost.scopes
    );
}

fn cost_provider_task_id(cost: &viden_types::CostUsageRecord) -> String {
    cost.scopes
        .iter()
        .find_map(|scope| match scope {
            CostScope::AgentTask(id) => Some(id.clone()),
            _ => None,
        })
        .expect("provider cost should include task scope")
}

fn transcript_runtime_events(
    cwd: &std::path::Path,
    home: &std::path::Path,
    session_id: &str,
) -> Vec<RuntimeEventKind> {
    let store = SessionStore::new_with_home(home, cwd, Some(session_id.to_string())).unwrap();
    store
        .load_entries()
        .unwrap()
        .into_iter()
        .filter_map(|entry| match entry {
            TranscriptEntry::RuntimeEvent { event } => Some(event.kind),
            _ => None,
        })
        .collect()
}

fn workflow_agent_events(cwd: &std::path::Path, home: &std::path::Path) -> Vec<WorkflowAgentEvent> {
    WorkflowStore::new(home, cwd)
        .unwrap()
        .load_agent_events()
        .unwrap()
}

fn assert_no_project_side_channel_events(events: &[WorkflowAgentEvent]) {
    let forbidden = [
        "agent_dag_created",
        "agent_task_queued",
        "agent_task_started",
        "agent_task_completed",
        "agent_task_cancelled",
        "agent_task_failed",
        "agent_task_blocked",
        "agent_evidence_recorded",
        "merge_gate_proposed",
        "merge_gate_accepted",
        "merge_gate_rejected",
        "agent_artifact_accepted",
        "agent_artifact_rejected",
        "agent_patch_merge_intent",
        "agent_patch_merged",
        "agent_patch_conflict",
    ];
    assert!(
        events
            .iter()
            .all(|event| !forbidden.contains(&event.event_type.as_str())),
        "new project commands must not emit legacy per-event agent facts: {events:?}"
    );
}

#[test]
fn core_exports_runtime_view_state_without_tui_dependencies() {
    let cwd = temp_dir("runtime_contract_cwd");
    let home = temp_dir("runtime_contract_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::AssistantText {
            content: "hello from runtime".to_string(),
        },
        ModelEvent::Done,
    ]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let engine_events = engine
        .process_input_with_approval("say hello", &mut approver)
        .unwrap();
    let runtime_events = engine.runtime_events_for_engine_events(&engine_events);
    assert_strictly_increasing_sequences(&runtime_events);

    assert!(runtime_events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::SnapshotUpdated { snapshot }
                if snapshot.provider_family == "sequence"
                    && snapshot.model_label == "test-model"
        )
    }));
    assert!(runtime_events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::AssistantDelta { content, .. }
                if content.contains("hello from runtime")
        )
    }));
    assert!(runtime_events.iter().any(|event| {
        matches!(&event.kind, RuntimeEventKind::TaskUpdated { task } if task.kind == AgentTaskKind::Provider)
    }));
    assert!(
        runtime_events
            .iter()
            .any(|event| matches!(&event.kind, RuntimeEventKind::ProviderHealthUpdated { .. }))
    );

    let mut view = RuntimeViewState::new(engine.runtime_snapshot());
    for event in &runtime_events {
        view.apply_event(event);
    }

    assert_eq!(view.snapshot.provider_family, "sequence");
    assert!(view.assistant_stream.contains("hello from runtime"));
    assert!(view.provider.is_some());
    assert!(
        view.tasks
            .iter()
            .any(|task| task.kind == AgentTaskKind::Provider)
    );
}

#[test]
fn provider_cost_attribution_uses_explicit_workflow_and_smoke_without_duplicate_scopes() {
    let cwd = temp_dir("cost_attr_main_cwd");
    let home = temp_dir("cost_attr_main_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::AssistantText {
            content: "cost attribution".to_string(),
        },
        ModelEvent::Usage(ModelUsage {
            input_tokens: Some(10),
            output_tokens: Some(4),
            cached_input_tokens: Some(3),
            retrieval_tokens: None,
            total_tokens: Some(14),
            cost_micro_usd: None,
            actual_cost_micro_usd: None,
        }),
    ]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine.set_cost_workflow_id_for_test(Some("workflow-main-1"));
    engine.set_cost_smoke_run_id_for_test(Some("smoke-main-1"));
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let engine_events = engine
        .process_input_with_approval("attribute main turn", &mut approver)
        .unwrap();
    let runtime_events = engine.runtime_events_for_engine_events(&engine_events);
    let mut view = RuntimeViewState::new(engine.runtime_snapshot());
    for event in &runtime_events {
        view.apply_event(event);
    }
    let cost = single_cost(&runtime_events, "sequence");

    assert_scope_once(cost, CostScope::Request(cost.usage_id.clone()));
    assert_scope_once(cost, CostScope::AgentTask(cost_provider_task_id(cost)));
    assert_scope_once(cost, CostScope::Workflow("workflow-main-1".to_string()));
    assert_scope_once(cost, CostScope::SmokeRun("smoke-main-1".to_string()));
    assert!(
        !cost
            .scopes
            .contains(&CostScope::Workflow("interactive".to_string()))
    );
    assert_eq!(view.cost_ledger.input_tokens, 10);
    assert_eq!(view.cost_ledger.cached_input_tokens, 3);
    assert_eq!(
        view.cost_usage
            .iter()
            .filter(|record| record
                .scopes
                .contains(&CostScope::SmokeRun("smoke-main-1".to_string())))
            .count(),
        1
    );
}

#[test]
fn agent_task_provider_cost_includes_dag_workflow_and_smoke_scopes() {
    let cwd = temp_dir("cost_attr_agent_cwd");
    let home = temp_dir("cost_attr_agent_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::AssistantText {
            content: "agent done".to_string(),
        },
        ModelEvent::Usage(ModelUsage {
            input_tokens: Some(21),
            output_tokens: Some(8),
            cached_input_tokens: None,
            retrieval_tokens: None,
            total_tokens: Some(29),
            cost_micro_usd: None,
            actual_cost_micro_usd: None,
        }),
    ]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine.set_cost_workflow_id_for_test(Some("workflow-agent-1"));
    engine.set_cost_smoke_run_id_for_test(Some("smoke-agent-1"));
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let task_id = "agent-cost-task".to_string();
    let dag_id = "agent-cost-dag".to_string();

    engine
        .handle_runtime_command(
            "cmd_dag",
            RuntimeCommand::StartAgentDag {
                goal: "cost attribution dag".to_string(),
                tasks: vec![AgentDagTaskSpec {
                    task_id: task_id.clone(),
                    role: AgentRole::Coder,
                    title: "cost task".to_string(),
                    objective: "return usage".to_string(),
                    dependencies: Vec::new(),
                    workspace: None,
                    file_scope: Vec::new(),
                    context_bundle_id: None,
                    required_evidence: vec!["patch".to_string()],
                    permission_policy: "read_write".to_string(),
                }],
            },
            &mut approver,
        )
        .unwrap();
    engine.set_runtime_dag_id_for_test(&task_id, &dag_id);
    let events = engine
        .handle_runtime_command(
            "cmd_task",
            RuntimeCommand::StartAgentTask {
                task_id: task_id.clone(),
            },
            &mut approver,
        )
        .unwrap();
    let cost = single_cost(&events, "sequence");

    assert_scope_once(cost, CostScope::AgentTask(task_id));
    assert_scope_once(cost, CostScope::Dag(dag_id));
    assert_scope_once(cost, CostScope::Workflow("workflow-agent-1".to_string()));
    assert_scope_once(cost, CostScope::SmokeRun("smoke-agent-1".to_string()));
}

#[test]
fn core_runtime_bridge_records_tool_calls_and_results() {
    let cwd = temp_dir("runtime_contract_tool_cwd");
    let home = temp_dir("runtime_contract_tool_home");
    let mut input = ToolInput::new();
    input.insert("command".to_string(), "printf hello".to_string());
    let provider = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::ToolCall(ToolCall {
            id: "tool_1".to_string(),
            name: "shell".to_string(),
            input,
        }),
        ModelEvent::Done,
    ]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine.set_context_engine_root_for_test(cwd.join(".viden/private-context-test"));
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let engine_events = engine
        .process_input_with_approval("run printf", &mut approver)
        .unwrap();
    assert!(engine_events.iter().any(
        |event| matches!(event, EngineEvent::ToolResult { output, .. } if output.contains("hello"))
    ));

    let runtime_events = engine.runtime_events_for_engine_events(&engine_events);
    assert_strictly_increasing_sequences(&runtime_events);
    assert!(runtime_events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::ToolCallStarted { name, .. } if name == "shell"
        )
    }));
    assert!(runtime_events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::ToolCallFinished {
                success: true,
                evidence: Some(evidence),
                ..
            } if evidence.summary.contains("hello")
        )
    }));
}

#[test]
fn context_reducer_default_matches_native_bundle_without_adapter_provenance() {
    let cwd = temp_dir("runtime_contract_context_reducer_default_cwd");
    let home = temp_dir("runtime_contract_context_reducer_default_home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    engine.set_context_engine_root_for_test(cwd.join(".viden/private-context-test"));

    let built = engine
        .build_main_context_bundle_with_mode("ERROR src/a.rs:1 boom", ContextBuildMode::Normal);
    let bundle = built.bundle;

    assert!(bundle.sources.iter().all(|source| {
        source.summary != "adapter reduced" && !source.include_reason.contains("adapter")
    }));
    assert!(
        context_reduction_records(&built.events).is_empty(),
        "default disabled adapter path must not emit health noise"
    );
}

#[test]
fn context_reducer_explicit_disabled_matches_absent_native_provider_bytes() {
    let cwd_absent = temp_dir("runtime_contract_context_reducer_absent_equiv_cwd");
    let home_absent = temp_dir("runtime_contract_context_reducer_absent_equiv_home");
    let cwd_disabled = temp_dir("runtime_contract_context_reducer_disabled_equiv_cwd");
    let home_disabled = temp_dir("runtime_contract_context_reducer_disabled_equiv_home");
    let input = "ERROR src/a.rs:1 boom\nfinal tail";

    let mut absent = SessionEngine::new_with_home(
        &cwd_absent,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home_absent),
    )
    .unwrap();
    absent.set_context_engine_root_for_test(cwd_absent.join(".viden/private-context-test"));
    let absent_built = absent.build_main_context_bundle_with_mode(input, ContextBuildMode::Normal);
    let absent_bundle = absent_built.bundle;

    let mut disabled = SessionEngine::new_with_home(
        &cwd_disabled,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home_disabled),
    )
    .unwrap();
    disabled.set_context_engine_root_for_test(cwd_disabled.join(".viden/private-context-test"));
    disabled.set_disabled_context_reducer_adapter_for_test("adapter", "0.1.0");
    let disabled_built =
        disabled.build_main_context_bundle_with_mode(input, ContextBuildMode::Normal);
    let disabled_bundle = disabled_built.bundle;

    assert_eq!(
        stable_context_bundle_bytes(absent_bundle.clone()),
        stable_context_bundle_bytes(disabled_bundle.clone())
    );
    assert_eq!(
        stable_provider_context_bytes(&render_provider_context_message(&absent_bundle)),
        stable_provider_context_bytes(&render_provider_context_message(&disabled_bundle))
    );
    assert!(context_reduction_records(&absent_built.events).is_empty());
    assert!(context_reduction_records(&disabled_built.events).is_empty());
}

#[test]
fn context_reducer_opt_in_records_adapter_provenance_and_quality() {
    let cwd = temp_dir("runtime_contract_context_reducer_adapter_cwd");
    let home = temp_dir("runtime_contract_context_reducer_adapter_home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    engine.set_context_engine_root_for_test(cwd.join(".viden/private-context-test"));
    engine.set_context_reducer_adapter_for_test("adapter", "0.1.0", "adapter reduced");

    let built = engine
        .build_main_context_bundle_with_mode("ERROR src/a.rs:1 boom", ContextBuildMode::Normal);

    let user_task = built
        .bundle
        .sources
        .iter()
        .find(|source| source.name == "user-task")
        .expect("user task source materialized");
    assert_eq!(user_task.summary, "adapter reduced");
    assert!(user_task.include_reason.contains("adapter:0.1.0"));
    assert!(built.events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::ContextViewDerived { view, .. }
                if view.derivation.starts_with("adapter:0.1.0:user-task")
        )
    }));
    let reductions = context_reduction_records(&built.events);
    let user_task_reduction = reductions
        .iter()
        .find(|reduction| reduction.item_id == user_task.item_id.as_deref().unwrap_or_default())
        .expect("user task reduction health evidence recorded");
    assert_eq!(user_task_reduction.reducer_id, "adapter");
    assert_eq!(user_task_reduction.reducer_version, "0.1.0");
    assert_eq!(user_task_reduction.status, "ok");
    assert!(!user_task_reduction.fallback);
    assert!(user_task_reduction.host_latency_ms < 250);
    assert!(user_task_reduction.reason.is_none());

    let mut replayed = RuntimeViewState::new(engine.runtime_view_state().snapshot.clone());
    for event in &built.events {
        replayed.apply_event(event);
    }
    assert!(
        replayed
            .context_reductions
            .iter()
            .any(|reduction| reduction.reduction_id == user_task_reduction.reduction_id)
    );
    let evidence_json = serde_json::to_string(&replayed.context_reductions).unwrap();
    assert!(!evidence_json.contains("ERROR src/a.rs"));
    assert!(!evidence_json.contains("/Users/"));
    assert!(!evidence_json.contains("sk-"));
}

#[test]
fn context_reducer_absent_adapter_does_not_block_provider_request() {
    let cwd = temp_dir("runtime_contract_context_reducer_absent_cwd");
    let home = temp_dir("runtime_contract_context_reducer_absent_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::AssistantText {
            content: "native path still works".to_string(),
        },
        ModelEvent::Done,
    ]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine.set_context_engine_root_for_test(cwd.join(".viden/private-context-test"));
    engine.set_absent_context_reducer_adapter_for_test("adapter", "0.1.0");
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let events = engine
        .process_input_with_approval("ERROR src/a.rs:1 boom", &mut approver)
        .unwrap();

    assert!(events.iter().any(
        |event| matches!(event, EngineEvent::Assistant(text) if text == "native path still works")
    ));
}

#[test]
fn context_reducer_sleeping_adapter_times_out_without_blocking_provider_request() {
    let cwd = temp_dir("runtime_contract_context_reducer_sleeping_cwd");
    let home = temp_dir("runtime_contract_context_reducer_sleeping_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::AssistantText {
            content: "native timeout path still works".to_string(),
        },
        ModelEvent::Done,
    ]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine.set_context_engine_root_for_test(cwd.join(".viden/private-context-test"));
    // The adapter sleep must dwarf the wall-clock bound below so the bound
    // distinguishes "host timed out and moved on" from "host waited for the
    // adapter" even when parallel test load slows the whole request path.
    engine.set_sleeping_context_reducer_adapter_for_test("adapter", "0.1.0", 30_000, 25);
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let started = std::time::Instant::now();

    let events = engine
        .process_input_with_approval("ERROR src/a.rs:1 boom", &mut approver)
        .unwrap();

    // Non-blocking is proven by finishing far below the 30s adapter sleep,
    // not by a tight latency budget that CPU contention can break.
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "provider request appears to have waited for the sleeping adapter: {:?}",
        started.elapsed()
    );
    assert!(events.iter().any(
        |event| matches!(event, EngineEvent::Assistant(text) if text == "native timeout path still works")
    ));
    let view = engine.runtime_view_state();
    assert!(view.context_reductions.iter().any(|reduction| {
        reduction.reducer_id == "adapter"
            && reduction.reducer_version == "0.1.0"
            && reduction.status == "timeout"
            && reduction.fallback
            && reduction.host_latency_ms <= 25
    }));
    assert!(view.context_reductions.iter().any(|reduction| {
        reduction.reducer_id == "adapter"
            && reduction.reducer_version == "0.1.0"
            && reduction.status == "circuit_open"
            && reduction.fallback
    }));
    let evidence_json = serde_json::to_string(&view.context_reductions).unwrap();
    assert!(!evidence_json.contains("ERROR src/a.rs"));
    assert!(!evidence_json.contains("/Users/"));
    assert!(!evidence_json.contains("sk-"));
}

#[test]
fn core_runtime_bridge_does_not_fail_successful_tool_output_with_error_words() {
    let cwd = temp_dir("runtime_contract_tool_error_word_cwd");
    let home = temp_dir("runtime_contract_tool_error_word_home");
    let mut input = ToolInput::new();
    input.insert(
        "command".to_string(),
        "printf 'Error format documentation'".to_string(),
    );
    let provider = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::ToolCall(ToolCall {
            id: "tool_error_word".to_string(),
            name: "shell".to_string(),
            input,
        }),
        ModelEvent::Done,
    ]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let engine_events = engine
        .process_input_with_approval("print help text", &mut approver)
        .unwrap();
    let runtime_events = engine.runtime_events_for_engine_events(&engine_events);

    assert!(runtime_events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::ToolCallFinished {
                success: true,
                evidence: Some(evidence),
                ..
            } if evidence.summary.contains("Error format documentation")
        )
    }));
}

#[test]
fn runtime_command_bus_switches_mode_and_submits_input() {
    let cwd = temp_dir("runtime_command_bus_cwd");
    let home = temp_dir("runtime_command_bus_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::AssistantText {
            content: "planned response".to_string(),
        },
        ModelEvent::Done,
    ]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let mode_events = engine
        .handle_runtime_command(
            "cmd_mode",
            RuntimeCommand::SetWorkMode {
                mode: WorkMode::Plan,
            },
            &mut approver,
        )
        .unwrap();
    assert_strictly_increasing_sequences(&mode_events);

    assert!(mode_events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::CommandAccepted { command_id, .. } if command_id == "cmd_mode"
        )
    }));
    assert!(mode_events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::SnapshotUpdated { snapshot }
                if snapshot.work_mode == WorkMode::Plan
                    && snapshot.permission_level == PermissionLevel::ReadOnly
        )
    }));

    let input_events = engine
        .handle_runtime_command(
            "cmd_input",
            RuntimeCommand::SubmitUserInput {
                content: "write a plan".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    assert_strictly_increasing_sequences(&input_events);

    assert!(input_events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::CommandAccepted { command_id, .. } if command_id == "cmd_input"
        )
    }));
    assert!(input_events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::AssistantDelta { content, .. }
                if content.contains("planned response")
        )
    }));
}

#[test]
fn failed_permission_controls_do_not_mutate_live_runtime_state() {
    for (case, successful_appends, command) in [
        (
            "permission",
            0,
            RuntimeCommand::SetPermissionLevel {
                level: PermissionLevel::Auto,
            },
        ),
        (
            "work-mode",
            1,
            RuntimeCommand::SetWorkMode {
                mode: WorkMode::Plan,
            },
        ),
    ] {
        let cwd = temp_dir(&format!("runtime_command_failed_{case}_cwd"));
        let home = temp_dir(&format!("runtime_command_failed_{case}_home"));
        let provider = Box::new(SequenceProvider::new(vec![]));
        let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
        let before = engine.runtime_snapshot();
        let before_permission_mode = engine.mode();
        let session_id = engine.session_id().to_string();
        let store = SessionStore::new_with_home(&home, &cwd, Some(session_id)).unwrap();
        let transcript_before = store.load_entries().unwrap();
        let mut approver = |_prompt| ApprovalResponse::allow_once(None);
        engine.fail_after_transcript_appends_for_test(successful_appends);

        let error = engine
            .handle_runtime_command(format!("failed_{case}"), command, &mut approver)
            .expect_err("injected transcript failure rejects the control command");

        assert!(error.contains("injected transcript append failure"));
        assert_eq!(engine.runtime_snapshot(), before, "{case} snapshot leaked");
        assert_eq!(
            engine.mode(),
            before_permission_mode,
            "{case} permission engine leaked"
        );
        assert_eq!(
            store.load_entries().unwrap(),
            transcript_before,
            "{case} control left partial session metadata"
        );
    }
}

#[test]
fn runtime_command_bus_covers_plan_build_review_permission_contract() {
    let cwd = temp_dir("runtime_command_mode_contract_cwd");
    let home = temp_dir("runtime_command_mode_contract_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    for (command_id, mode) in [
        ("cmd_plan", WorkMode::Plan),
        ("cmd_review", WorkMode::Review),
        ("cmd_explore", WorkMode::Explore),
    ] {
        let events = engine
            .handle_runtime_command(
                command_id,
                RuntimeCommand::SetWorkMode { mode },
                &mut approver,
            )
            .unwrap();

        assert_strictly_increasing_sequences(&events);
        assert!(events.iter().any(|event| {
            matches!(
                &event.kind,
                RuntimeEventKind::SnapshotUpdated { snapshot }
                    if snapshot.work_mode == mode
                        && snapshot.permission_level == PermissionLevel::ReadOnly
            )
        }));
    }

    let build_events = engine
        .handle_runtime_command(
            "cmd_build",
            RuntimeCommand::SetWorkMode {
                mode: WorkMode::Build,
            },
            &mut approver,
        )
        .unwrap();

    assert_strictly_increasing_sequences(&build_events);
    assert!(build_events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::SnapshotUpdated { snapshot }
                if snapshot.work_mode == WorkMode::Build
                    && snapshot.permission_level == PermissionLevel::Ask
        )
    }));
}

#[test]
fn runtime_command_bus_emits_approval_events_for_gated_tools() {
    let cwd = temp_dir("runtime_command_approval_cwd");
    let home = temp_dir("runtime_command_approval_home");
    let mut input = ToolInput::new();
    input.insert("command".to_string(), "printf approved".to_string());
    let provider = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::ToolCall(ToolCall {
            id: "tool_needs_approval".to_string(),
            name: "shell".to_string(),
            input,
        }),
        ModelEvent::Done,
    ]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let events = engine
        .handle_runtime_command(
            "cmd_approval",
            RuntimeCommand::SubmitUserInput {
                content: "run approved command".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    assert_strictly_increasing_sequences(&events);
    assert!(events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::ApprovalRequested { approval }
                if approval.tool_name == "shell"
                    && approval.input_preview.contains("printf approved")
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::ApprovalResolved {
                decision: ApprovalDecision::Allow { .. },
                ..
            }
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::ToolCallFinished {
                success: true,
                evidence: Some(evidence),
                ..
            } if evidence.summary.contains("approved")
        )
    }));
}

fn approval_tool_event_order(events: &[RuntimeEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::ApprovalRequested { .. } => Some("requested"),
            RuntimeEventKind::ApprovalResolved { .. } => Some("resolved"),
            RuntimeEventKind::ToolCallStarted { .. } => Some("started"),
            RuntimeEventKind::ToolCallFinished { .. } => Some("finished"),
            _ => None,
        })
        .collect()
}

#[test]
fn runtime_command_bus_non_streaming_turn_keeps_both_approved_tools() {
    let cwd = temp_dir("runtime_command_two_approved_tools_cwd");
    let home = temp_dir("runtime_command_two_approved_tools_home");
    let mut first = ToolInput::new();
    first.insert("path".to_string(), "first.txt".to_string());
    first.insert("content".to_string(), "first\n".to_string());
    let mut second = ToolInput::new();
    second.insert("path".to_string(), "second.txt".to_string());
    second.insert("content".to_string(), "second\n".to_string());
    let provider = Box::new(SequenceProvider::new(vec![
        vec![
            ModelEvent::ToolCall(ToolCall {
                id: "tool_first_approved".to_string(),
                name: "write_file".to_string(),
                input: first,
            }),
            ModelEvent::ToolCall(ToolCall {
                id: "tool_second_approved".to_string(),
                name: "write_file".to_string(),
                input: second,
            }),
        ],
        vec![ModelEvent::Done],
    ]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let events = engine
        .handle_runtime_command(
            "cmd_two_approved_tools",
            RuntimeCommand::SubmitUserInput {
                content: "run two approved tools".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    assert_eq!(
        approval_tool_event_order(&events),
        vec![
            "requested",
            "resolved",
            "started",
            "finished",
            "requested",
            "resolved",
            "started",
            "finished",
        ]
    );
}

#[test]
fn runtime_command_bus_non_streaming_agent_task_preserves_approval_boundaries() {
    let cwd = temp_dir("runtime_command_agent_two_approved_tools_cwd");
    let home = temp_dir("runtime_command_agent_two_approved_tools_home");
    let mut first = ToolInput::new();
    first.insert("path".to_string(), "agent-first.txt".to_string());
    first.insert("content".to_string(), "first\n".to_string());
    let mut second = ToolInput::new();
    second.insert("path".to_string(), "agent-second.txt".to_string());
    second.insert("content".to_string(), "second\n".to_string());
    let provider = Box::new(SequenceProvider::new(vec![
        vec![
            ModelEvent::ToolCall(ToolCall {
                id: "tool_agent_first_approved".to_string(),
                name: "write_file".to_string(),
                input: first,
            }),
            ModelEvent::ToolCall(ToolCall {
                id: "tool_agent_second_approved".to_string(),
                name: "write_file".to_string(),
                input: second,
            }),
        ],
        vec![ModelEvent::Done],
    ]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    engine
        .handle_runtime_command(
            "cmd_agent_two_approved_tools_dag",
            RuntimeCommand::StartAgentDag {
                goal: "run two approved tools".to_string(),
                tasks: vec![AgentDagTaskSpec {
                    task_id: "task_agent_two_approved_tools".to_string(),
                    role: AgentRole::Coder,
                    title: "Run approved tools".to_string(),
                    objective: "Run two approval-gated tools in order".to_string(),
                    dependencies: Vec::new(),
                    workspace: None,
                    file_scope: Vec::new(),
                    context_bundle_id: None,
                    required_evidence: Vec::new(),
                    permission_policy: "full_access".to_string(),
                }],
            },
            &mut approver,
        )
        .unwrap();

    let events = engine
        .handle_runtime_command(
            "cmd_agent_two_approved_tools",
            RuntimeCommand::StartAgentTask {
                task_id: "task_agent_two_approved_tools".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    assert_eq!(
        approval_tool_event_order(&events),
        vec![
            "requested",
            "resolved",
            "started",
            "finished",
            "requested",
            "resolved",
            "started",
            "finished",
        ]
    );
}

#[test]
fn runtime_command_bus_queues_follow_up_input() {
    let cwd = temp_dir("runtime_command_queue_cwd");
    let home = temp_dir("runtime_command_queue_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let events = engine
        .handle_runtime_command(
            "cmd_queue",
            RuntimeCommand::QueueFollowUp {
                content: "continue with tests".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    assert_strictly_increasing_sequences(&events);
    assert!(events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::CommandAccepted { command_id, .. } if command_id == "cmd_queue"
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::InputQueued { input }
                if input.content_preview == "continue with tests"
        )
    }));

    let view = engine.runtime_view_state();
    assert_eq!(view.queued_inputs.len(), 1);
    assert_eq!(view.queued_inputs[0].content_preview, "continue with tests");
}

#[test]
fn typed_agent_adapter_query_projects_builtin_acp_descriptors_without_commands() {
    let cwd = temp_dir("runtime_agent_adapter_query_cwd");
    let home = temp_dir("runtime_agent_adapter_query_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let events = engine
        .handle_runtime_command(
            "cmd_agent_adapters",
            RuntimeCommand::QueryAgentAdapters,
            &mut approver,
        )
        .unwrap();

    let adapters = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::AgentAdaptersLoaded { adapters } => Some(adapters),
            _ => None,
        })
        .expect("typed adapter event");
    assert_eq!(
        adapters
            .iter()
            .map(|adapter| adapter.agent_id.as_str())
            .collect::<Vec<_>>(),
        vec!["claude-acp", "codex-acp", "kiro-cli"]
    );
    assert!(
        adapters
            .iter()
            .all(|adapter| adapter.route == AgentRoute::Acp)
    );
    assert!(
        adapters.iter().all(|adapter| adapter
            .capabilities
            .iter()
            .all(|capability| capability.0.starts_with("agent.")
                && !capability.0.contains(['/', '_'])))
    );
    assert!(adapters.iter().all(|adapter| {
        adapter
            .capabilities
            .contains(&CapabilityId("agent.session.prompt".to_string()))
    }));
    let mut view = engine.runtime_view_state();
    for event in &events {
        view.apply_event(event);
    }
    assert_eq!(view.agent_adapters, *adapters);
    assert!(!cwd.join(".viden/agents").exists());
}

#[test]
fn typed_agent_adapter_probe_rejects_unknown_identity_without_projection() {
    let cwd = temp_dir("runtime_agent_adapter_unknown_cwd");
    let home = temp_dir("runtime_agent_adapter_unknown_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let events = engine
        .handle_runtime_command(
            "cmd_agent_probe_unknown",
            RuntimeCommand::ProbeAgentAdapter {
                agent_id: "unknown-acp".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { command_id, reason }
            if command_id == "cmd_agent_probe_unknown"
                && reason.contains("unknown-acp")
                && reason.contains("claude-acp")
                && reason.contains("codex-acp")
                && reason.contains("kiro-cli")
    )));
    assert!(
        events
            .iter()
            .all(|event| !matches!(event.kind, RuntimeEventKind::AgentAdapterProbed { .. }))
    );
    assert!(engine.runtime_view_state().agent_adapters.is_empty());
}

#[test]
fn hard_context_limit_rejects_before_provider_request() {
    let cwd = temp_dir("runtime_contract_hard_budget_cwd");
    let home = temp_dir("runtime_contract_hard_budget_home");
    let request_count = Arc::new(AtomicU64::new(0));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = Box::new(CountingProvider::new(
        Arc::clone(&request_count),
        Arc::clone(&requests),
        Ok(vec![ModelEvent::AssistantText {
            content: "should not be called".to_string(),
        }]),
    ));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine.set_context_budget_for_test(10, 20);
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let events = engine
        .handle_runtime_command(
            "cmd_hard_budget",
            RuntimeCommand::SubmitUserInput {
                content: "required evidence marker ".repeat(100),
            },
            &mut approver,
        )
        .unwrap();

    assert_eq!(request_count.load(Ordering::SeqCst), 0);
    assert!(requests.lock().unwrap().is_empty());
    assert!(events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::ContextBudgetExceeded { budget }
                if budget.exceeded
                    && budget.soft_token_limit == 10
                    && budget.hard_token_limit == 20
                    && budget.used_tokens > budget.hard_token_limit
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::CommandRejected { command_id, reason }
                if command_id == "cmd_hard_budget"
                    && reason.contains("context hard limit")
                    && reason.contains("reduce input")
        )
    }));
}

#[test]
fn soft_context_budget_evicts_low_priority_sources_before_provider_request() {
    let cwd = temp_dir("runtime_contract_soft_budget_cwd");
    let home = temp_dir("runtime_contract_soft_budget_home");
    let request_count = Arc::new(AtomicU64::new(0));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = Box::new(CountingProvider::new(
        Arc::clone(&request_count),
        Arc::clone(&requests),
        Ok(vec![ModelEvent::AssistantText {
            content: "soft budget ok".to_string(),
        }]),
    ));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine.set_context_budget_for_test(100, 1_000);
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    engine
        .process_input_with_approval(
            &format!("/memory add {}", "low-priority-memory ".repeat(300)),
            &mut approver,
        )
        .unwrap();
    let events = engine
        .handle_runtime_command(
            "cmd_soft_budget",
            RuntimeCommand::SubmitUserInput {
                content: "short task".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    assert_eq!(request_count.load(Ordering::SeqCst), 1);
    assert!(events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::ContextUpdated { context }
                if context.estimated_tokens <= context.soft_token_budget
                    && context.hard_token_limit == 1_000
                    && context.omitted_sources.iter().any(|source| source.name == "memory")
                    && context.sources.iter().any(|source| source.name == "user-task")
        )
    }));
}

#[test]
fn context_engine_events_replay_without_raw_secret_or_paths() {
    let cwd = temp_dir("runtime_contract_context_replay_cwd");
    let home = temp_dir("runtime_contract_context_replay_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::AssistantText {
            content: "context ready".to_string(),
        },
        ModelEvent::Done,
    ]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let secret = "sk-test-secret-1234567890";

    let events = engine
        .handle_runtime_command(
            "cmd_context_replay",
            RuntimeCommand::SubmitUserInput {
                content: format!("summarize without leaking {secret}"),
            },
            &mut approver,
        )
        .unwrap();

    let event_json = serde_json::to_string(&events).unwrap();
    assert!(!event_json.contains(secret));
    for event in &events {
        if matches!(
            event.kind,
            RuntimeEventKind::ContextItemStored { .. }
                | RuntimeEventKind::ContextViewDerived { .. }
                | RuntimeEventKind::ContextBundleBuilt { .. }
                | RuntimeEventKind::ContextBudgetExceeded { .. }
                | RuntimeEventKind::ContextUpdated { .. }
        ) {
            let context_event_json = serde_json::to_string(event).unwrap();
            assert!(!context_event_json.contains(cwd.to_string_lossy().as_ref()));
        }
    }
    assert!(events.iter().any(|event| {
        matches!(&event.kind, RuntimeEventKind::ContextItemStored { item }
            if !item.content_sha256.is_empty() && item.summary.contains("user-task"))
    }));
    assert!(events.iter().any(|event| {
        matches!(&event.kind, RuntimeEventKind::ContextViewDerived { view, handle }
            if view.token_count > 0
                && handle.item_id == view.item_id
                && !handle.content_sha256.is_empty()
                && !view.content_sha256.is_empty()
                && handle.preferred_view_id.as_deref() == Some(view.view_id.as_str()))
    }));
    assert!(events.iter().any(|event| {
        matches!(&event.kind, RuntimeEventKind::ContextBundleBuilt { handle_ids, estimated_tokens, .. }
            if !handle_ids.is_empty() && *estimated_tokens > 0)
    }));

    let mut view = RuntimeViewState::new(engine.runtime_snapshot());
    for event in &events {
        view.apply_event(event);
    }
    assert!(view.context.is_some());
    assert!(!view.context_items.is_empty());
    assert_eq!(view.context_items.len(), view.context_handles.len());
    assert_eq!(view.context_views.len(), view.context_handles.len());
}

#[test]
fn existing_context_source_hash_corruption_fails_visibly_on_rematerialization() {
    let cwd = temp_dir("runtime_contract_corrupt_context_cwd");
    let home = temp_dir("runtime_contract_corrupt_context_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::AssistantText {
            content: "context ready".to_string(),
        },
        ModelEvent::Done,
    ]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    engine
        .handle_runtime_command(
            "cmd_context",
            RuntimeCommand::SubmitUserInput {
                content: "build canonical context".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    let mut bundle = engine.provider_context_bundle().expect("context bundle");
    let source = bundle
        .sources
        .iter_mut()
        .find(|source| source.name == "user-task")
        .expect("user-task source");
    source.content_sha256 = Some("ff".repeat(32));

    let rebuilt =
        engine.materialize_existing_context_bundle(&bundle, ContextBuildMode::RequestTooLargeRetry);

    assert!(rebuilt.events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::ContextQualityFailed { quality }
                if quality.target_id == "user-task"
                    && quality.failure_reason.as_deref().is_some_and(|reason| {
                        reason.contains("hash mismatch") || reason.contains("context blob")
                    })
        )
    }));
}

#[test]
fn command_accepted_redacts_start_agent_dag_secrets_and_paths() {
    let cwd = temp_dir("runtime_contract_dag_redaction_cwd");
    let home = temp_dir("runtime_contract_dag_redaction_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let secret = "sk-agent-dag-secret-123";
    let raw_scope = cwd.join("secret/file.rs").to_string_lossy().to_string();

    let events = engine
        .handle_runtime_command(
            "cmd_redacted_dag",
            RuntimeCommand::StartAgentDag {
                goal: format!("Plan {secret}"),
                tasks: vec![AgentDagTaskSpec {
                    task_id: "task_redacted".to_string(),
                    role: AgentRole::Planner,
                    title: format!("Title {secret}"),
                    objective: format!("Objective {secret}"),
                    dependencies: Vec::new(),
                    workspace: Some(cwd.to_string_lossy().to_string()),
                    file_scope: vec![raw_scope.clone()],
                    context_bundle_id: None,
                    required_evidence: vec!["plan".to_string()],
                    permission_policy: "read_only".to_string(),
                }],
            },
            &mut approver,
        )
        .unwrap();

    let accepted = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::CommandAccepted { command, .. } => Some(command),
            _ => None,
        })
        .expect("command accepted");
    let json = serde_json::to_string(accepted).unwrap();
    assert!(!json.contains(secret));
    assert!(!json.contains(cwd.to_string_lossy().as_ref()));
    assert!(!json.contains(&raw_scope));
    let RuntimeCommand::StartAgentDag { goal, tasks } = accepted else {
        panic!("expected StartAgentDag");
    };
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].file_scope.len(), 1);
    assert_eq!(goal, "Plan [REDACTED]");
    assert_eq!(tasks[0].workspace.as_deref(), Some("[REDACTED]"));
}

#[test]
fn retrieve_context_returns_safe_bytes_and_event_metadata() {
    let cwd = temp_dir("runtime_command_retrieve_context_cwd");
    let home = temp_dir("runtime_command_retrieve_context_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![ModelEvent::Done]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let safe_content = "retrieve-context-safe-body";
    engine
        .handle_runtime_command(
            "cmd_build_context",
            RuntimeCommand::SubmitUserInput {
                content: safe_content.to_string(),
            },
            &mut approver,
        )
        .unwrap();
    let view = engine.runtime_view_state();
    let context = view.context.as_ref().expect("runtime context bundle");
    let context_task_id = context.task_id.clone();
    engine.set_cost_workflow_id_for_test(Some("workflow-retrieval-1"));
    engine.set_cost_smoke_run_id_for_test(Some("smoke-retrieval-1"));
    engine
        .handle_runtime_command(
            "cmd_retrieval_dag",
            RuntimeCommand::StartAgentDag {
                goal: "retrieval ancestry".to_string(),
                tasks: vec![AgentDagTaskSpec {
                    task_id: context_task_id.clone(),
                    role: AgentRole::Reviewer,
                    title: "retrieval task".to_string(),
                    objective: "retrieve context".to_string(),
                    dependencies: Vec::new(),
                    workspace: None,
                    file_scope: Vec::new(),
                    context_bundle_id: None,
                    required_evidence: vec!["review".to_string()],
                    permission_policy: "read_only".to_string(),
                }],
            },
            &mut approver,
        )
        .unwrap();
    engine.set_runtime_dag_id_for_test(&context_task_id, "dag-retrieval-1");
    let handle = context
        .sources
        .iter()
        .find(|source| source.name == "user-task")
        .and_then(|source| {
            context_handle_from_source(source, &ContextScope::Task(context.task_id.clone()))
        })
        .expect("user-task context handle");

    let events = engine
        .handle_runtime_command(
            "cmd_retrieve_context",
            RuntimeCommand::RetrieveContext {
                handle_id: handle.handle_id.clone(),
                reason: "hydrate context for review with sk-test-secret in reason".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::ToolCallFinished {
                name,
                success: true,
                evidence: Some(evidence),
                ..
            } if name == "context_read" && evidence.summary.contains(safe_content)
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::ContextRetrieved { retrieval }
                if retrieval.handle_id == handle.handle_id
                    && retrieval.item_id == handle.item_id
                    && retrieval.scope == handle.scope
                    && retrieval.byte_count == safe_content.len() as u64
                    && retrieval.token_count > 0
                    && retrieval.reason_category == "hydrate"
                    && !retrieval.reason.contains("sk-test-secret")
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::CostUsageRecorded { cost }
                if cost.provider_id == "context"
                    && cost.model == "retrieval"
                    && cost.tokens.retrieval_tokens.is_some()
                    && cost.tokens.total_tokens == cost.tokens.retrieval_tokens
                    && cost.scopes.contains(&CostScope::AgentTask(context_task_id.clone()))
                    && cost.scopes.contains(&CostScope::Dag("dag-retrieval-1".to_string()))
                    && cost.scopes.contains(&CostScope::Workflow("workflow-retrieval-1".to_string()))
                    && cost.scopes.contains(&CostScope::SmokeRun("smoke-retrieval-1".to_string()))
        )
    }));
    assert!(
        !serde_json::to_string(&events)
            .unwrap()
            .contains("sk-test-secret")
    );

    let mut projected = RuntimeViewState::new(engine.runtime_snapshot());
    for event in &events {
        projected.apply_event(event);
    }
    assert_eq!(projected.context_retrievals.len(), 1);
}

#[test]
fn retrieved_context_cost_survives_session_resume() {
    let cwd = temp_dir("runtime_command_retrieve_resume_cwd");
    let home = temp_dir("runtime_command_retrieve_resume_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![ModelEvent::Done]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    engine
        .handle_runtime_command(
            "cmd_build_context",
            RuntimeCommand::SubmitUserInput {
                content: "resume-safe-retrieval-body".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    let view = engine.runtime_view_state();
    let context = view.context.as_ref().expect("runtime context bundle");
    let handle = context
        .sources
        .iter()
        .find(|source| source.name == "user-task")
        .and_then(|source| {
            context_handle_from_source(source, &ContextScope::Task(context.task_id.clone()))
        })
        .expect("user-task context handle");
    let session_id = engine.session_id().to_string();

    engine
        .handle_runtime_command(
            "cmd_retrieve_context",
            RuntimeCommand::RetrieveContext {
                handle_id: handle.handle_id,
                reason: "hydrate resume-safe context".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    let resumed_provider = Box::new(SequenceProvider::new(vec![]));
    let mut resumed =
        SessionEngine::new_with_home(&cwd, resumed_provider, Some(home.clone())).unwrap();
    let resume_engine_events = resumed
        .process_input_with_approval(&format!("/resume {session_id}"), &mut approver)
        .unwrap();
    let resume_runtime_events = resumed.runtime_events_for_engine_events(&resume_engine_events);
    let mut resumed_view = RuntimeViewState::new(resumed.runtime_snapshot());
    for event in &resume_runtime_events {
        resumed_view.apply_event(event);
    }
    let retrieval_cost = resumed_view
        .cost_usage
        .iter()
        .find(|cost| cost.provider_id == "context" && cost.model == "retrieval")
        .expect("retrieval cost survives resume");

    assert_eq!(
        retrieval_cost.tokens.retrieval_tokens,
        retrieval_cost.tokens.total_tokens
    );
    assert!(retrieval_cost.tokens.retrieval_tokens.unwrap_or(0) > 0);
    assert_eq!(
        resumed_view.cost_ledger.retrieval_tokens,
        retrieval_cost.tokens.retrieval_tokens.unwrap()
    );
}

#[test]
fn retrieve_context_bounds_long_secret_and_path_reason_before_recording_event() {
    let cwd = temp_dir("runtime_command_retrieve_context_reason_cwd");
    let home = temp_dir("runtime_command_retrieve_context_reason_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![ModelEvent::Done]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    engine
        .handle_runtime_command(
            "cmd_build_context",
            RuntimeCommand::SubmitUserInput {
                content: "bounded reason body".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    let handle_id = engine
        .runtime_view_state()
        .context_handles
        .first()
        .unwrap()
        .handle_id
        .clone();
    let long_reason = format!(
        "hydrate {} /Users/wiki/private/context-store sk-test-secret-value {}",
        "安全".repeat(200),
        "tail".repeat(200)
    );

    let events = engine
        .handle_runtime_command(
            "cmd_retrieve_long_reason",
            RuntimeCommand::RetrieveContext {
                handle_id,
                reason: long_reason,
            },
            &mut approver,
        )
        .unwrap();

    let retrieval = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::ContextRetrieved { retrieval } => Some(retrieval),
            _ => None,
        })
        .expect("retrieval event");
    assert!(retrieval.reason.len() <= 256, "{}", retrieval.reason.len());
    assert!(retrieval.reason.chars().count() <= 160);
    assert!(!retrieval.reason.contains("/Users/wiki"));
    assert!(!retrieval.reason.contains("sk-test-secret"));
    assert!(std::str::from_utf8(retrieval.reason.as_bytes()).is_ok());
    assert_eq!(retrieval.permission_decision, "allow");
    assert_eq!(retrieval.reason_rule_category, "safe_read");
}

#[test]
fn retrieve_context_denies_unknown_and_cross_scope_handles_before_reading_bytes() {
    let cwd = temp_dir("runtime_command_retrieve_context_scope_cwd");
    let home = temp_dir("runtime_command_retrieve_context_scope_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![ModelEvent::Done]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let context_root = cwd.join(".viden/private-context-test");
    engine.set_context_engine_root_for_test(context_root.clone());
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    engine
        .handle_runtime_command(
            "cmd_build_context",
            RuntimeCommand::SubmitUserInput {
                content: "owned task context".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    let known_handle = engine
        .runtime_view_state()
        .context_handles
        .first()
        .cloned()
        .expect("known runtime handle");
    let mut unknown_cross_scope = known_handle;
    unknown_cross_scope.handle_id = "ctxh-cross-scope".to_string();
    unknown_cross_scope.scope = ContextScope::Task("task-other".to_string());
    {
        let mut store = ContextEngine::open(&context_root).unwrap();
        let stored = store
            .store(ContextPutRequest {
                scope: unknown_cross_scope.scope.clone(),
                kind: ContextContentKind::Text,
                content: b"cross scope bytes must never be read",
                evidence_id: None,
            })
            .unwrap();
        unknown_cross_scope.item_id = stored.handle.item_id;
        unknown_cross_scope.content_sha256 = stored.handle.content_sha256;
    }

    let events = engine
        .handle_runtime_command(
            "cmd_retrieve_cross",
            RuntimeCommand::RetrieveContext {
                handle_id: unknown_cross_scope.handle_id,
                reason: "hydrate foreign task".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    let json = serde_json::to_string(&events).unwrap();
    assert!(!json.contains("cross scope bytes"));
    assert!(events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::CommandRejected { reason, .. }
                if reason.contains("not known to the current runtime context")
        )
    }));
    assert!(events.iter().all(|event| {
        !matches!(
            event.kind,
            RuntimeEventKind::ContextRetrieved { .. } | RuntimeEventKind::ToolCallFinished { .. }
        )
    }));
}

#[test]
fn retrieve_context_rejects_prepare_failures_without_accepting_or_rewriting_command_ids() {
    let cwd = temp_dir("runtime_command_retrieve_context_prepare_order_cwd");
    let home = temp_dir("runtime_command_retrieve_context_prepare_order_home");
    let context_root = cwd.join(".viden/private-context-test");
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let mut unknown = engine_with_single_context(&cwd, &home, &context_root, "unknown body");
    let unknown_events = unknown
        .handle_runtime_command(
            "cmd_unknown_retrieve",
            RuntimeCommand::RetrieveContext {
                handle_id: "ctxh-does-not-exist".to_string(),
                reason: "hydrate unknown".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    assert_prepare_rejection_only(
        &unknown_events,
        unknown.runtime_snapshot(),
        "cmd_unknown_retrieve",
        "not known",
    );

    let mut denied = engine_with_single_context(&cwd, &home, &context_root, "denied body");
    let denied_handle_id = denied.runtime_view_state().context_handles[0]
        .handle_id
        .clone();
    denied.add_permission_rule_for_test(PermissionRule {
        source: PermissionRuleSource::Session,
        rule_behavior: PermissionBehavior::Deny,
        rule_value: PermissionRuleValue {
            tool_name: "context_read".to_string(),
            rule_content: None,
        },
    });
    let denied_events = denied
        .handle_runtime_command(
            "cmd_denied_retrieve",
            RuntimeCommand::RetrieveContext {
                handle_id: denied_handle_id,
                reason: "hydrate denied".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    assert_prepare_rejection_only(
        &denied_events,
        denied.runtime_snapshot(),
        "cmd_denied_retrieve",
        "Denied",
    );

    let mut expired = engine_with_single_context(&cwd, &home, &context_root, "expired body");
    let expired_handle_id = expired.runtime_view_state().context_handles[0]
        .handle_id
        .clone();
    expired.mutate_context_handle_for_test(&expired_handle_id, |handle| {
        handle.expires_at = Some(1);
    });
    let expired_events = expired
        .handle_runtime_command(
            "cmd_expired_retrieve",
            RuntimeCommand::RetrieveContext {
                handle_id: expired_handle_id,
                reason: "hydrate expired".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    assert_prepare_rejection_only(
        &expired_events,
        expired.runtime_snapshot(),
        "cmd_expired_retrieve",
        "expired",
    );
}

fn assert_prepare_rejection_only(
    events: &[RuntimeEvent],
    snapshot: viden_types::RuntimeSnapshot,
    command_id: &str,
    reason: &str,
) {
    assert!(
        events.iter().any(|event| {
            matches!(
                &event.kind,
                RuntimeEventKind::CommandRejected {
                    command_id: rejected_id,
                    reason: rejected_reason,
                } if rejected_id == command_id && rejected_reason.contains(reason)
            )
        }),
        "expected rejection for {command_id}: {events:#?}"
    );
    assert!(
        events.iter().all(|event| {
            !matches!(
                &event.kind,
                RuntimeEventKind::CommandAccepted {
                    command_id: accepted_id,
                    ..
                } if accepted_id == command_id
            )
        }),
        "prepare failure must not accept {command_id}: {events:#?}"
    );

    let mut projected = RuntimeViewState::new(snapshot);
    for event in events {
        projected.apply_event(event);
    }
    assert!(
        projected
            .errors
            .iter()
            .any(|error| error.message.contains(command_id) || error.message.contains(reason)),
        "replay should retain recoverable rejection evidence: {projected:#?}"
    );
}

#[test]
fn retrieve_context_uses_permission_policy_for_deny_ask_approve_and_plan_read() {
    let cwd = temp_dir("runtime_command_retrieve_context_permissions_cwd");
    let home = temp_dir("runtime_command_retrieve_context_permissions_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![ModelEvent::Done]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut allow = |_prompt| ApprovalResponse::allow_once(None);
    engine
        .handle_runtime_command(
            "cmd_build_context",
            RuntimeCommand::SubmitUserInput {
                content: "permission gated context".to_string(),
            },
            &mut allow,
        )
        .unwrap();
    let handle_id = engine
        .runtime_view_state()
        .context_handles
        .first()
        .unwrap()
        .handle_id
        .clone();

    engine.add_permission_rule_for_test(PermissionRule {
        source: PermissionRuleSource::Session,
        rule_behavior: PermissionBehavior::Deny,
        rule_value: PermissionRuleValue {
            tool_name: "context_read".to_string(),
            rule_content: None,
        },
    });
    let denied = engine
        .handle_runtime_command(
            "cmd_retrieve_denied",
            RuntimeCommand::RetrieveContext {
                handle_id: handle_id.clone(),
                reason: "hydrate denied".to_string(),
            },
            &mut allow,
        )
        .unwrap();
    assert!(denied.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::CommandRejected { reason, .. }
                if reason.contains("Denied by permission rule")
        )
    }));
    assert!(
        denied
            .iter()
            .all(|event| !matches!(event.kind, RuntimeEventKind::ContextRetrieved { .. }))
    );

    engine.clear_permission_rules_for_test();
    engine.add_permission_rule_for_test(PermissionRule {
        source: PermissionRuleSource::Session,
        rule_behavior: PermissionBehavior::Ask,
        rule_value: PermissionRuleValue {
            tool_name: "context_read".to_string(),
            rule_content: None,
        },
    });
    let mut saw_prompt = false;
    let mut approve = |prompt: viden_types::PermissionPrompt| {
        saw_prompt = prompt.tool_name == "context_read";
        ApprovalResponse::allow_once(None)
    };
    let approved = engine
        .handle_runtime_command(
            "cmd_retrieve_approved",
            RuntimeCommand::RetrieveContext {
                handle_id: handle_id.clone(),
                reason: "hydrate approved".to_string(),
            },
            &mut approve,
        )
        .unwrap();
    assert!(saw_prompt);
    assert!(
        approved
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::ContextRetrieved { .. }))
    );

    engine.clear_permission_rules_for_test();
    engine
        .handle_runtime_command(
            "cmd_plan_mode",
            RuntimeCommand::SetWorkMode {
                mode: WorkMode::Plan,
            },
            &mut allow,
        )
        .unwrap();
    let plan_read = engine
        .handle_runtime_command(
            "cmd_retrieve_plan",
            RuntimeCommand::RetrieveContext {
                handle_id,
                reason: "hydrate plan read".to_string(),
            },
            &mut allow,
        )
        .unwrap();
    assert!(
        plan_read
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::ContextRetrieved { .. }))
    );
}

#[test]
fn retrieve_context_redacts_secret_content_before_tool_result_and_events() {
    let cwd = temp_dir("runtime_command_retrieve_context_secret_cwd");
    let home = temp_dir("runtime_command_retrieve_context_secret_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![ModelEvent::Done]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    engine
        .handle_runtime_command(
            "cmd_build_context",
            RuntimeCommand::SubmitUserInput {
                content: "api_key=raw-secret-value keep-safe-context".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    let handle_id = engine
        .runtime_view_state()
        .context_handles
        .first()
        .unwrap()
        .handle_id
        .clone();

    let events = engine
        .handle_runtime_command(
            "cmd_retrieve_secret",
            RuntimeCommand::RetrieveContext {
                handle_id,
                reason: "hydrate secret-bearing context".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    let json = serde_json::to_string(&events).unwrap();
    assert!(!json.contains("raw-secret-value"));
    assert!(json.contains("keep-safe-context"));
}

#[test]
fn retrieve_context_reports_expired_missing_item_missing_blob_and_hash_mismatch_safely() {
    let cwd = temp_dir("runtime_command_retrieve_context_errors_cwd");
    let home = temp_dir("runtime_command_retrieve_context_errors_home");
    let context_root = cwd.join(".viden/private-context-test");
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let mut expired = engine_with_single_context(&cwd, &home, &context_root, "expired bytes");
    let expired_handle = expired.runtime_view_state().context_handles[0].clone();
    expired.mutate_context_handle_for_test(&expired_handle.handle_id, |handle| {
        handle.expires_at = Some(1);
    });
    let expired_events = expired
        .handle_runtime_command(
            "cmd_expired",
            RuntimeCommand::RetrieveContext {
                handle_id: expired_handle.handle_id.clone(),
                reason: "hydrate expired".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    assert!(expired_events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::CommandRejected { reason, .. } if reason.contains("expired")
        )
    }));

    let mut dag_mismatch = engine_with_single_context(&cwd, &home, &context_root, "dag bytes");
    let mismatch_handle = dag_mismatch.runtime_view_state().context_handles[0].clone();
    dag_mismatch.mutate_context_handle_for_test(&mismatch_handle.handle_id, |handle| {
        handle.scope = ContextScope::Dag("dag-other".to_string());
    });
    let mismatch_events = dag_mismatch
        .handle_runtime_command(
            "cmd_dag_mismatch",
            RuntimeCommand::RetrieveContext {
                handle_id: mismatch_handle.handle_id.clone(),
                reason: "hydrate mismatch".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    assert!(mismatch_events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::CommandRejected { reason, .. }
                if reason.contains("outside the active context scope")
        )
    }));

    let mut missing_item = engine_with_single_context(&cwd, &home, &context_root, "missing item");
    let missing_item_handle = missing_item.runtime_view_state().context_handles[0].clone();
    missing_item.remove_context_item_for_test(&missing_item_handle.item_id);
    let missing_item_events = missing_item
        .handle_runtime_command(
            "cmd_missing_item",
            RuntimeCommand::RetrieveContext {
                handle_id: missing_item_handle.handle_id.clone(),
                reason: "hydrate missing item".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    assert!(missing_item_events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::CommandRejected { reason, .. }
                if reason.contains("item") && reason.contains("missing")
        )
    }));

    let mut missing_blob = engine_with_single_context(&cwd, &home, &context_root, "missing blob");
    let missing_blob_handle = missing_blob.runtime_view_state().context_handles[0].clone();
    fs::remove_file(context_blob_path(
        &context_root,
        &missing_blob_handle.content_sha256,
    ))
    .unwrap();
    let missing_blob_events = missing_blob
        .handle_runtime_command(
            "cmd_missing_blob",
            RuntimeCommand::RetrieveContext {
                handle_id: missing_blob_handle.handle_id.clone(),
                reason: "hydrate missing blob".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    let missing_blob_json = serde_json::to_string(&missing_blob_events).unwrap();
    assert!(!missing_blob_json.contains(context_root.to_string_lossy().as_ref()));
    assert!(missing_blob_events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::CommandRejected { reason, .. }
                if reason.contains("blob") && reason.contains("not")
        )
    }));

    let mut hash_mismatch = engine_with_single_context(&cwd, &home, &context_root, "hash before");
    let hash_handle = hash_mismatch.runtime_view_state().context_handles[0].clone();
    fs::write(
        context_blob_path(&context_root, &hash_handle.content_sha256),
        b"hash after",
    )
    .unwrap();
    let hash_events = hash_mismatch
        .handle_runtime_command(
            "cmd_hash_mismatch",
            RuntimeCommand::RetrieveContext {
                handle_id: hash_handle.handle_id,
                reason: "hydrate hash mismatch".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    assert!(hash_events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::CommandRejected { reason, .. } if reason.contains("hash mismatch")
        )
    }));
}

fn context_handle_from_source(
    source: &viden_types::ContextSourceRecord,
    scope: &ContextScope,
) -> Option<ContextHandleRecord> {
    Some(ContextHandleRecord {
        handle_id: source.handle_id.clone()?,
        item_id: source.item_id.clone()?,
        preferred_view_id: source.view_id.clone(),
        content_sha256: source.content_sha256.clone()?,
        scope: scope.clone(),
        expires_at: None,
    })
}

fn engine_with_single_context(
    cwd: &std::path::Path,
    home: &std::path::Path,
    context_root: &std::path::Path,
    content: &str,
) -> SessionEngine {
    let provider = Box::new(SequenceProvider::new(vec![vec![ModelEvent::Done]]));
    let mut engine = SessionEngine::new_with_home(cwd, provider, Some(home.to_path_buf())).unwrap();
    engine.set_context_engine_root_for_test(context_root.to_path_buf());
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    engine
        .handle_runtime_command(
            "cmd_build_context",
            RuntimeCommand::SubmitUserInput {
                content: content.to_string(),
            },
            &mut approver,
        )
        .unwrap();
    engine
}

fn context_blob_path(root: &std::path::Path, content_sha256: &str) -> std::path::PathBuf {
    root.join("blobs")
        .join(&content_sha256[..2])
        .join(content_sha256)
}

#[test]
fn runtime_command_bus_configures_provider_and_active_models() {
    let cwd = temp_dir("runtime_command_provider_cwd");
    let home = temp_dir("runtime_command_provider_home");
    let config_path = cwd.join("user-config.toml");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine.set_user_config_path_override(config_path.clone());
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let configured = engine
        .handle_runtime_command(
            "cmd_provider_config",
            RuntimeCommand::ConfigureProvider {
                provider_id: "deepseek".to_string(),
                api_key_env: Some("DEEPSEEK_API_KEY".to_string()),
                endpoint: Some("https://api.deepseek.com".to_string()),
                default_model: Some("deepseek-chat".to_string()),
            },
            &mut approver,
        )
        .unwrap();

    assert_strictly_increasing_sequences(&configured);
    assert!(configured.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::EvidenceRecorded { evidence }
                if evidence.kind == "provider_config"
                    && evidence.summary.contains("deepseek")
        )
    }));

    let activated = engine
        .handle_runtime_command(
            "cmd_activate_model",
            RuntimeCommand::ActivateModel {
                provider_id: "deepseek".to_string(),
                model: "deepseek-reasoner".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    assert!(activated.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::EvidenceRecorded { evidence }
                if evidence.kind == "provider_model"
                    && evidence.summary.contains("deepseek-reasoner")
        )
    }));

    let deactivated = engine
        .handle_runtime_command(
            "cmd_deactivate_model",
            RuntimeCommand::DeactivateModel {
                provider_id: "deepseek".to_string(),
                model: "deepseek-chat".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    assert!(deactivated.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::EvidenceRecorded { evidence }
                if evidence.kind == "provider_model"
                    && evidence.summary.contains("removed")
        )
    }));

    let contents = fs::read_to_string(config_path).unwrap();
    assert!(contents.contains("[providers.deepseek]"));
    assert!(contents.contains(r#"api_key_env = "DEEPSEEK_API_KEY""#));
    assert!(contents.contains(r#"api_base = "https://api.deepseek.com""#));
    assert!(contents.contains(r#"default_model = "deepseek-chat""#));
    assert!(contents.contains(r#"models = ["deepseek-reasoner"]"#));
}

#[test]
fn merge_gate_rejects_summary_only_patch_evidence() {
    let cwd = temp_dir("runtime_contract_summary_only_gate_cwd");
    let home = temp_dir("runtime_contract_summary_only_gate_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    engine
        .handle_runtime_command(
            "cmd_agent_dag",
            RuntimeCommand::StartAgentDag {
                goal: "Require canonical evidence".to_string(),
                tasks: vec![AgentDagTaskSpec {
                    task_id: "task_canonical_patch".to_string(),
                    role: AgentRole::Coder,
                    title: "Patch with canonical evidence".to_string(),
                    objective: "Record a patch".to_string(),
                    dependencies: Vec::new(),
                    workspace: None,
                    file_scope: vec!["crates/runtime".to_string()],
                    context_bundle_id: None,
                    required_evidence: vec!["patch".to_string()],
                    permission_policy: "scoped_mutation".to_string(),
                }],
            },
            &mut approver,
        )
        .unwrap();

    let events = engine
        .handle_runtime_command(
            "cmd_summary_evidence",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task_canonical_patch".to_string(),
                evidence_id: Some("evidence-summary-patch".to_string()),
                kind: "patch".to_string(),
                summary: "diff --git a/src/lib.rs b/src/lib.rs".to_string(),
                path: None,
                source: Some("legacy-agent".to_string()),
                canonical: None,
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::MergeGateUpdated { gate }
                if gate.gate_id == "gate-task_canonical_patch"
                    && gate.status == MergeGateStatus::CollectingEvidence
                    && gate.decision.as_deref().is_some_and(|decision| decision.contains("missing_canonical"))
        )
    }));
    let store = viden_workflows::stores::WorkflowStore::new(home, &cwd).unwrap();
    let agent_events = store.load_agent_events().unwrap();
    assert!(
        agent_events
            .iter()
            .all(|event| event.event_type != "agent_evidence_recorded")
    );
    assert!(agent_events.iter().any(|event| {
        event.event_type == "runtime_projection_batch"
            && event.task_id.as_deref() == Some("task_canonical_patch")
            && event
                .payload
                .get("command_id")
                .is_some_and(|id| id == "cmd_summary_evidence")
            && event
                .payload
                .get("runtime_event_kinds")
                .is_some_and(|kinds| {
                    kinds.contains("evidence_recorded") && kinds.contains("merge_gate_updated")
                })
    }));
}

#[test]
fn accept_merge_gate_command_cannot_bypass_invalid_evidence() {
    let cwd = temp_dir("runtime_contract_accept_bypass_gate_cwd");
    let home = temp_dir("runtime_contract_accept_bypass_gate_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    engine
        .handle_runtime_command(
            "cmd_agent_dag",
            RuntimeCommand::StartAgentDag {
                goal: "Block direct accept".to_string(),
                tasks: vec![AgentDagTaskSpec {
                    task_id: "task_accept_bypass".to_string(),
                    role: AgentRole::Coder,
                    title: "Patch with summary evidence".to_string(),
                    objective: "Record an incomplete patch".to_string(),
                    dependencies: Vec::new(),
                    workspace: None,
                    file_scope: vec!["crates/runtime".to_string()],
                    context_bundle_id: None,
                    required_evidence: vec!["patch".to_string()],
                    permission_policy: "scoped_mutation".to_string(),
                }],
            },
            &mut approver,
        )
        .unwrap();
    engine
        .handle_runtime_command(
            "cmd_summary_evidence",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task_accept_bypass".to_string(),
                evidence_id: Some("evidence-summary-patch".to_string()),
                kind: "patch".to_string(),
                summary: "summary-only patch".to_string(),
                path: None,
                source: Some("legacy-agent".to_string()),
                canonical: None,
            },
            &mut approver,
        )
        .unwrap();

    let events = engine
        .handle_runtime_command(
            "cmd_accept_gate",
            RuntimeCommand::AcceptMergeGate {
                gate_id: "gate-task_accept_bypass".to_string(),
                actor: viden_types::RuntimeOwner::default(),
                reviewed_evidence: Vec::new(),
                decision: Some("force accept".to_string()),
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::CommandRejected { command_id, reason }
                if command_id == "cmd_accept_gate"
                    && reason.contains("missing_canonical")
        )
    }));
    assert_eq!(
        engine
            .runtime_view_state()
            .merge_gates
            .iter()
            .find(|gate| gate.gate_id == "gate-task_accept_bypass")
            .unwrap()
            .status,
        MergeGateStatus::CollectingEvidence
    );
}

#[test]
fn merge_gate_with_empty_required_evidence_collects_summary_evidence() {
    let cwd = temp_dir("runtime_contract_empty_required_summary_cwd");
    let home = temp_dir("runtime_contract_empty_required_summary_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let session_id = engine.session_id().to_string();
    let _ = start_gate_with_required(&mut engine, &mut approver, "task_empty_summary", Vec::new());

    let events = engine
        .handle_runtime_command(
            "cmd_empty_summary",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task_empty_summary".to_string(),
                evidence_id: Some("evidence-empty-summary".to_string()),
                kind: "patch".to_string(),
                summary: "summary evidence should not accept empty requirements".to_string(),
                path: None,
                source: Some("legacy-agent".to_string()),
                canonical: None,
            },
            &mut approver,
        )
        .unwrap();

    assert_gate_status_with_reason(
        &events,
        "gate-task_empty_summary",
        MergeGateStatus::CollectingEvidence,
        "missing_required_kind",
    );
    assert_resumed_runtime_matches(&cwd, &home, &session_id, &engine.runtime_view_state());
}

#[test]
fn merge_gate_with_empty_required_evidence_collects_canonical_evidence_and_replays() {
    let cwd = temp_dir("runtime_contract_empty_required_canonical_cwd");
    let home = temp_dir("runtime_contract_empty_required_canonical_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let session_id = engine.session_id().to_string();
    let mut start_events = start_gate_with_required(
        &mut engine,
        &mut approver,
        "task_empty_canonical",
        Vec::new(),
    );
    let (item, canonical) = stored_canonical_context(
        &cwd,
        "task_empty_canonical",
        "evidence-empty-canonical",
        "bundle-empty-canonical",
        ContextContentKind::Diff,
        b"canonical evidence still needs an explicit requirement",
    );
    engine.set_merge_gate_context_facts_for_test("bundle-empty-canonical", item);

    let mut events = engine
        .handle_runtime_command(
            "cmd_empty_canonical",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task_empty_canonical".to_string(),
                evidence_id: Some("evidence-empty-canonical".to_string()),
                kind: "patch".to_string(),
                summary: "canonical evidence still needs an explicit requirement".to_string(),
                path: None,
                source: Some("executor".to_string()),
                canonical: Some(canonical),
            },
            &mut approver,
        )
        .unwrap();

    assert_gate_status_with_reason(
        &events,
        "gate-task_empty_canonical",
        MergeGateStatus::CollectingEvidence,
        "missing_required_kind",
    );
    let live = engine.runtime_view_state();
    let mut replayed = RuntimeViewState::new(live.snapshot.clone());
    start_events.append(&mut events);
    for event in &start_events {
        replayed.apply_event(event);
    }
    assert_eq!(replayed.merge_gates, live.merge_gates);
    assert_eq!(replayed.latest_evidence, live.latest_evidence);
    assert_resumed_runtime_matches(&cwd, &home, &session_id, &live);
}

#[test]
fn accept_merge_gate_command_cannot_bypass_empty_required_evidence() {
    let cwd = temp_dir("runtime_contract_empty_required_accept_cwd");
    let home = temp_dir("runtime_contract_empty_required_accept_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let session_id = engine.session_id().to_string();
    let _ = start_gate_with_required(&mut engine, &mut approver, "task_empty_accept", Vec::new());

    let events = engine
        .handle_runtime_command(
            "cmd_accept_empty_required",
            RuntimeCommand::AcceptMergeGate {
                gate_id: "gate-task_empty_accept".to_string(),
                actor: viden_types::RuntimeOwner::default(),
                reviewed_evidence: Vec::new(),
                decision: Some("force accept empty".to_string()),
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { command_id, reason }
            if command_id == "cmd_accept_empty_required"
                && reason.contains("missing_required_kind")
    )));
    assert_eq!(
        engine
            .runtime_view_state()
            .merge_gates
            .iter()
            .find(|gate| gate.gate_id == "gate-task_empty_accept")
            .unwrap()
            .status,
        MergeGateStatus::Proposed
    );
    assert_resumed_runtime_matches(&cwd, &home, &session_id, &engine.runtime_view_state());
}

#[test]
fn record_agent_evidence_rolls_back_when_workflow_append_fails() {
    let cwd = temp_dir("runtime_contract_workflow_append_fail_cwd");
    let home = temp_dir("runtime_contract_workflow_append_fail_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let session_id = engine.session_id().to_string();
    start_single_patch_gate(&mut engine, &mut approver, "task_workflow_append_fail");
    let before = engine.runtime_view_state();
    let workflow_before = workflow_agent_events(&cwd, &home);
    let (item, canonical) = stored_canonical_context(
        &cwd,
        "task_workflow_append_fail",
        "evidence-workflow-append-fail",
        "bundle-workflow-append-fail",
        ContextContentKind::Diff,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );
    engine.set_merge_gate_context_facts_for_test("bundle-workflow-append-fail", item);
    engine.fail_next_workflow_append_for_test();

    let events = engine
        .handle_runtime_command(
            "cmd_workflow_append_fail",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task_workflow_append_fail".to_string(),
                evidence_id: Some("evidence-workflow-append-fail".to_string()),
                kind: "patch".to_string(),
                summary: "canonical patch should roll back on workflow append failure".to_string(),
                path: None,
                source: Some("executor".to_string()),
                canonical: Some(canonical),
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { command_id, reason }
            if command_id == "cmd_workflow_append_fail"
                && reason.contains("injected workflow append failure")
    )));
    assert_eq!(
        engine.runtime_view_state().merge_gates,
        before.merge_gates,
        "workflow append failure must not leak a gate update"
    );
    assert_eq!(
        engine.runtime_view_state().latest_evidence,
        before.latest_evidence,
        "workflow append failure must not leak evidence"
    );
    assert_eq!(
        workflow_agent_events(&cwd, &home),
        workflow_before,
        "failed command must not append a canonical projection batch"
    );
    assert_resumed_runtime_matches(&cwd, &home, &session_id, &engine.runtime_view_state());
}

#[test]
fn record_agent_evidence_does_not_dual_write_or_roll_back_on_transcript_failure() {
    let cwd = temp_dir("runtime_contract_no_dual_write_evidence_cwd");
    let home = temp_dir("runtime_contract_no_dual_write_evidence_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let session_id = engine.session_id().to_string();
    start_single_patch_gate(&mut engine, &mut approver, "task_no_dual_write");
    let (item, canonical) = stored_canonical_context(
        &cwd,
        "task_no_dual_write",
        "evidence-no-dual-write",
        "bundle-no-dual-write",
        ContextContentKind::Diff,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );
    engine.set_merge_gate_context_facts_for_test("bundle-no-dual-write", item);
    engine.fail_after_transcript_appends_for_test(1);

    let events = engine
        .handle_runtime_command(
            "cmd_no_dual_write_evidence",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task_no_dual_write".to_string(),
                evidence_id: Some("evidence-no-dual-write".to_string()),
                kind: "patch".to_string(),
                summary: "canonical patch should commit through workflow owner".to_string(),
                path: None,
                source: Some("executor".to_string()),
                canonical: Some(canonical),
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MergeGateUpdated { gate }
            if gate.gate_id == "gate-task_no_dual_write"
                && gate.status == MergeGateStatus::Accepted
    )));
    let transcript_events = transcript_runtime_events(&cwd, &home, &session_id);
    assert!(
        !transcript_events.iter().any(|kind| matches!(
            kind,
            RuntimeEventKind::EvidenceRecorded { .. }
                | RuntimeEventKind::EvidenceCanonicalized { .. }
                | RuntimeEventKind::MergeGateUpdated { .. }
                | RuntimeEventKind::TaskUpdated { .. }
        )),
        "project agent facts must not be dual-written to the session transcript"
    );
    let workflow_events = workflow_agent_events(&cwd, &home);
    assert_no_project_side_channel_events(&workflow_events);
    let projection_batches = workflow_events
        .iter()
        .filter(|event| event.event_type == "runtime_projection_batch")
        .collect::<Vec<_>>();
    assert_eq!(
        projection_batches.len(),
        2,
        "start + evidence projection batches"
    );
    assert!(projection_batches.iter().any(|event| {
        event
            .payload
            .get("command_id")
            .is_some_and(|id| id == "cmd_no_dual_write_evidence")
            && event.payload.contains_key("runtime_events_json")
            && !event.payload.contains_key("runtime_event_json")
    }));
    assert!(
        !workflow_events
            .iter()
            .any(|event| event.event_type == "runtime_projection"),
        "new commands must not write one projection event per runtime event"
    );
    assert_resumed_runtime_matches(&cwd, &home, &session_id, &engine.runtime_view_state());
}

#[test]
fn record_agent_evidence_projection_redacts_summary_source_and_unsafe_paths() {
    let cwd = temp_dir("runtime_contract_projection_redaction_cwd");
    let home = temp_dir("runtime_contract_projection_redaction_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    start_single_patch_gate(&mut engine, &mut approver, "task_redact_projection");
    let (item, canonical) = stored_canonical_context(
        &cwd,
        "task_redact_projection",
        "evidence-redact-projection",
        "bundle-redact-projection",
        ContextContentKind::Diff,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );
    engine.set_merge_gate_context_facts_for_test("bundle-redact-projection", item);
    let secret_summary = format!(
        "patch proof sk-secret-token {} /Users/wiki/private.txt",
        "x".repeat(700)
    );

    let events = engine
        .handle_runtime_command(
            "cmd_redact_projection",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task_redact_projection".to_string(),
                evidence_id: Some("evidence-redact-projection".to_string()),
                kind: "patch".to_string(),
                summary: secret_summary.clone(),
                path: Some("/Users/wiki/private.txt".to_string()),
                source: Some("source-sk-secret-token".to_string()),
                canonical: Some(canonical),
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { command_id, reason }
            if command_id == "cmd_redact_projection" && reason.contains("invalid evidence path")
    )));
    let serialized_events = serde_json::to_string(&events).unwrap();
    assert!(!serialized_events.contains("sk-secret-token"));
    assert!(!serialized_events.contains("/Users/wiki/private.txt"));
    let workflow_json = fs::read_to_string(
        WorkflowStore::new(&home, &cwd)
            .unwrap()
            .paths()
            .agent_log
            .clone(),
    )
    .unwrap();
    assert!(!workflow_json.contains("sk-secret-token"));
    assert!(!workflow_json.contains("/Users/wiki/private.txt"));
}

#[test]
fn workflow_projection_redacts_adversarial_project_command_payloads() {
    let cwd = temp_dir("runtime_contract_projection_adversarial_cwd");
    let home = temp_dir("runtime_contract_projection_adversarial_home");
    fs::create_dir_all(cwd.join("src")).unwrap();
    fs::write(
        cwd.join("src/lib.rs"),
        "pub const STATUS: &str = \"old\";\n",
    )
    .unwrap();
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let session_id = engine.session_id().to_string();
    let secret = "sk-project-command-secret";
    let private_path = "/Users/wiki/private/project.rs";

    engine
        .handle_runtime_command(
            "cmd_adversarial_dag",
            RuntimeCommand::StartAgentDag {
                goal: format!("ship {secret} from {private_path}"),
                tasks: vec![AgentDagTaskSpec {
                    task_id: "task_adversarial".to_string(),
                    role: AgentRole::Coder,
                    title: format!("Patch {secret}"),
                    objective: format!("Apply diff --git from {private_path}"),
                    dependencies: Vec::new(),
                    workspace: Some(private_path.to_string()),
                    file_scope: vec![private_path.to_string(), "../secret.rs".to_string()],
                    context_bundle_id: None,
                    required_evidence: vec!["patch".to_string()],
                    permission_policy: "scoped_mutation".to_string(),
                }],
            },
            &mut approver,
        )
        .unwrap();
    let patch = b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-pub const STATUS: &str = \"old\";\n+pub const STATUS: &str = \"new\";\n";
    let (item, canonical) = stored_canonical_context(
        &cwd,
        "task_adversarial",
        "evidence-adversarial-patch",
        "bundle-adversarial",
        ContextContentKind::Diff,
        patch,
    );
    engine.set_merge_gate_context_facts_for_test("bundle-adversarial", item);
    let events = engine
        .handle_runtime_command(
            "cmd_adversarial_evidence",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task_adversarial".to_string(),
                evidence_id: Some("evidence-adversarial-patch".to_string()),
                kind: "patch".to_string(),
                summary: format!("proof {secret} diff --git {private_path}"),
                path: Some("src/lib.rs".to_string()),
                source: Some(format!("source {secret}")),
                canonical: Some(canonical),
            },
            &mut approver,
        )
        .unwrap();
    assert!(
        serde_json::to_string(&events)
            .unwrap()
            .contains("[REDACTED]")
    );
    engine
        .handle_runtime_command(
            "cmd_adversarial_accept",
            RuntimeCommand::AcceptMergeGate {
                gate_id: "gate-task_adversarial".to_string(),
                actor: viden_types::RuntimeOwner::default(),
                reviewed_evidence: Vec::new(),
                decision: Some(format!("accept {secret} {private_path}")),
            },
            &mut approver,
        )
        .unwrap();
    engine
        .handle_runtime_command(
            "cmd_adversarial_merge",
            RuntimeCommand::MergeAgentPatch {
                gate_id: "gate-task_adversarial".to_string(),
                actor: viden_types::RuntimeOwner::default(),
                decision: Some(format!("merge {secret} diff --git {private_path}")),
            },
            &mut approver,
        )
        .unwrap();

    let workflow_events = workflow_agent_events(&cwd, &home);
    assert_no_project_side_channel_events(&workflow_events);
    assert!(
        workflow_events
            .iter()
            .all(|event| event.event_type == "runtime_projection_batch")
    );
    let workflow_json = fs::read_to_string(
        WorkflowStore::new(&home, &cwd)
            .unwrap()
            .paths()
            .agent_log
            .clone(),
    )
    .unwrap();
    assert!(!workflow_json.contains(secret));
    assert!(!workflow_json.contains(private_path));
    assert!(!workflow_json.contains("../secret.rs"));
    assert!(!workflow_json.contains("diff --git"));
    assert!(workflow_json.contains("[REDACTED]"));
    let runtime_json = serde_json::to_string(&engine.runtime_view_state()).unwrap();
    assert!(!runtime_json.contains(secret));
    assert!(!runtime_json.contains(private_path));
    assert_resumed_runtime_matches(&cwd, &home, &session_id, &engine.runtime_view_state());
}

#[test]
fn record_agent_evidence_rejects_adversarial_nested_canonical_metadata() {
    for field in [
        "item_id",
        "bundle_id",
        "source_hash",
        "producer.identity",
        "producer.role",
        "producer.task_id",
        "permission_snapshot_id",
        "permission_scope.task_id",
        "evidence_scope.workflow_id",
    ] {
        let cwd = temp_dir(&format!(
            "runtime_contract_bad_canonical_{}_cwd",
            field.replace('.', "_")
        ));
        let home = temp_dir(&format!(
            "runtime_contract_bad_canonical_{}_home",
            field.replace('.', "_")
        ));
        let provider = Box::new(SequenceProvider::new(vec![]));
        let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
        let mut approver = |_prompt| ApprovalResponse::allow_once(None);
        start_single_patch_gate(&mut engine, &mut approver, "task_bad_canonical");
        let (item, mut canonical) = stored_canonical_context(
            &cwd,
            "task_bad_canonical",
            "evidence-bad-canonical",
            "bundle-bad-canonical",
            ContextContentKind::Diff,
            b"safe canonical patch",
        );
        engine.set_merge_gate_context_facts_for_test("bundle-bad-canonical", item);
        let malicious = match field {
            "source_hash" => "/Users/wiki/private/sk-source-hash",
            "permission_snapshot_id" => "https://evil.example/sk-permission",
            "permission_scope.task_id" => "/Users/wiki/private/task",
            "evidence_scope.workflow_id" => "../workflow/sk-secret",
            _ => "sk-secret/../../Users/wiki/private",
        };
        match field {
            "item_id" => canonical.item_id = malicious.to_string(),
            "bundle_id" => canonical.bundle_id = malicious.to_string(),
            "source_hash" => canonical.source_hash = malicious.to_string(),
            "producer.identity" => canonical.producer.identity = malicious.to_string(),
            "producer.role" => canonical.producer.role = malicious.to_string(),
            "producer.task_id" => canonical.producer.task_id = malicious.to_string(),
            "permission_snapshot_id" => {
                canonical.permission_snapshot_id = Some(malicious.to_string());
            }
            "permission_scope.task_id" => {
                canonical.permission_scope = ContextScope::Task(malicious.to_string());
            }
            "evidence_scope.workflow_id" => {
                canonical.evidence_scope = ContextScope::Workflow(malicious.to_string());
            }
            _ => unreachable!(),
        }

        let events = engine
            .handle_runtime_command(
                format!("cmd_bad_canonical_{}", field.replace('.', "_")),
                RuntimeCommand::RecordAgentEvidence {
                    gate_id: "gate-task_bad_canonical".to_string(),
                    evidence_id: Some("evidence-bad-canonical".to_string()),
                    kind: "patch".to_string(),
                    summary: "safe summary".to_string(),
                    path: None,
                    source: Some("executor".to_string()),
                    canonical: Some(canonical),
                },
                &mut approver,
            )
            .unwrap();

        assert!(events.iter().any(|event| {
            matches!(
                &event.kind,
                RuntimeEventKind::CommandRejected { reason, .. }
                    if reason.contains("invalid_canonical_evidence_reference")
                        && reason.contains(field)
            )
        }));
        let serialized_events = serde_json::to_string(&events).unwrap();
        assert!(!serialized_events.contains(malicious));
        let workflow_json = fs::read_to_string(
            WorkflowStore::new(&home, &cwd)
                .unwrap()
                .paths()
                .agent_log
                .clone(),
        )
        .unwrap();
        assert!(!workflow_json.contains(malicious));
        let runtime_json = serde_json::to_string(&engine.runtime_view_state()).unwrap();
        assert!(!runtime_json.contains(malicious));
        assert!(
            engine
                .runtime_view_state()
                .latest_evidence
                .iter()
                .all(|evidence| evidence.id != "evidence-bad-canonical")
        );
    }
}

#[test]
fn record_agent_evidence_rejects_traversal_and_control_character_paths() {
    for (case, path) in [
        ("traversal", "../secret.txt"),
        ("control", "target/\u{0007}secret.log"),
    ] {
        let cwd = temp_dir(&format!("runtime_contract_invalid_path_{case}_cwd"));
        let home = temp_dir(&format!("runtime_contract_invalid_path_{case}_home"));
        let provider = Box::new(SequenceProvider::new(vec![]));
        let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
        let mut approver = |_prompt| ApprovalResponse::allow_once(None);
        start_single_patch_gate(&mut engine, &mut approver, "task_invalid_path");
        let events = engine
            .handle_runtime_command(
                format!("cmd_invalid_path_{case}"),
                RuntimeCommand::RecordAgentEvidence {
                    gate_id: "gate-task_invalid_path".to_string(),
                    evidence_id: Some(format!("evidence-invalid-path-{case}")),
                    kind: "patch".to_string(),
                    summary: "summary".to_string(),
                    path: Some(path.to_string()),
                    source: Some("executor".to_string()),
                    canonical: None,
                },
                &mut approver,
            )
            .unwrap();
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::CommandRejected { reason, .. } if reason.contains("invalid evidence path")
        )));
    }
}

#[test]
fn start_agent_task_projects_state_to_workflow_not_transcript() {
    let cwd = temp_dir("runtime_contract_start_task_workflow_projection_cwd");
    let home = temp_dir("runtime_contract_start_task_workflow_projection_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::AssistantText {
            content: "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n".to_string(),
        },
        ModelEvent::Done,
    ]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let session_id = engine.session_id().to_string();
    start_single_patch_gate(&mut engine, &mut approver, "task_start_projection");

    let events = engine
        .handle_runtime_command(
            "cmd_start_projection",
            RuntimeCommand::StartAgentTask {
                task_id: "task_start_projection".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    let live = engine.runtime_view_state();
    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::TaskUpdated { task }
            if task.id == "task_start_projection"
                && task.status == AgentTaskStatus::Done
    )));
    assert!(live.latest_evidence.iter().any(|evidence| {
        evidence
            .id
            .starts_with("evidence-task_start_projection-task_summary")
            && evidence.canonical.is_none()
    }));
    assert!(live.merge_gates.iter().any(|gate| {
        gate.gate_id == "gate-task_start_projection"
            && gate.status == MergeGateStatus::CollectingEvidence
    }));
    let transcript_events = transcript_runtime_events(&cwd, &home, &session_id);
    assert!(
        !transcript_events.iter().any(|kind| matches!(
            kind,
            RuntimeEventKind::EvidenceRecorded { .. }
                | RuntimeEventKind::EvidenceCanonicalized { .. }
                | RuntimeEventKind::MergeGateUpdated { .. }
                | RuntimeEventKind::TaskUpdated { .. }
        )),
        "project agent facts must be workflow-owned for StartAgentTask"
    );
    let workflow_events = WorkflowStore::new(&home, &cwd)
        .unwrap()
        .load_agent_events()
        .unwrap();
    assert!(workflow_events.iter().any(|event| {
        event.event_type == "runtime_projection_batch"
            && event
                .payload
                .get("command_id")
                .is_some_and(|id| id == "cmd_start_projection")
            && event
                .payload
                .get("runtime_event_kinds")
                .is_some_and(|kinds| {
                    kinds.contains("evidence_recorded") && kinds.contains("merge_gate_updated")
                })
    }));
    assert_resumed_runtime_matches(&cwd, &home, &session_id, &live);
}

#[test]
fn start_agent_dag_rejects_projection_batches_over_event_cap_without_live_leak() {
    let cwd = temp_dir("runtime_contract_projection_event_cap_cwd");
    let home = temp_dir("runtime_contract_projection_event_cap_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let tasks = (0..260)
        .map(|index| AgentDagTaskSpec {
            task_id: format!("task_projection_cap_{index}"),
            role: AgentRole::Coder,
            title: "Cap".to_string(),
            objective: "Exceed projection event cap".to_string(),
            dependencies: Vec::new(),
            workspace: None,
            file_scope: vec!["src".to_string()],
            context_bundle_id: None,
            required_evidence: vec!["patch".to_string()],
            permission_policy: "scoped_mutation".to_string(),
        })
        .collect::<Vec<_>>();

    let events = engine
        .handle_runtime_command(
            "cmd_projection_cap",
            RuntimeCommand::StartAgentDag {
                goal: "oversized projection batch".to_string(),
                tasks,
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { command_id, reason }
            if command_id == "cmd_projection_cap"
                && reason.contains("workflow projection batch exceeds event cap")
    )));
    let live = engine.runtime_view_state();
    assert!(live.agent_dags.is_empty());
    assert!(live.tasks.is_empty());
    assert!(live.merge_gates.is_empty());
    assert!(
        workflow_agent_events(&cwd, &home)
            .iter()
            .all(|event| event.event_type != "runtime_projection_batch")
    );
    assert_resumed_runtime_matches(&cwd, &home, engine.session_id(), &live);
}

#[test]
fn start_agent_task_rolls_back_when_workflow_append_fails() {
    let cwd = temp_dir("runtime_contract_start_task_workflow_fail_cwd");
    let home = temp_dir("runtime_contract_start_task_workflow_fail_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::AssistantText {
            content: "this should not run after workflow append failure".to_string(),
        },
        ModelEvent::Done,
    ]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let session_id = engine.session_id().to_string();
    start_single_patch_gate(&mut engine, &mut approver, "task_start_workflow_fail");
    let before = engine.runtime_view_state();
    engine.fail_next_workflow_append_for_test();

    let events = engine
        .handle_runtime_command(
            "cmd_start_workflow_fail",
            RuntimeCommand::StartAgentTask {
                task_id: "task_start_workflow_fail".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { command_id, reason }
            if command_id == "cmd_start_workflow_fail"
                && reason.contains("injected workflow append failure")
    )));
    let live = engine.runtime_view_state();
    assert_eq!(live.tasks, before.tasks, "task update leaked");
    assert_eq!(live.agent_dags, before.agent_dags, "DAG update leaked");
    assert_eq!(live.merge_gates, before.merge_gates, "gate update leaked");
    assert_eq!(
        live.latest_evidence, before.latest_evidence,
        "evidence leaked"
    );
    assert_eq!(
        live.context_bundles, before.context_bundles,
        "context bundle leaked"
    );
    assert_resumed_runtime_matches(&cwd, &home, &session_id, &live);
}

#[test]
fn start_agent_dag_rolls_back_when_late_workflow_append_fails() {
    let cwd = temp_dir("runtime_contract_start_dag_late_workflow_fail_cwd");
    let home = temp_dir("runtime_contract_start_dag_late_workflow_fail_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let before = engine.runtime_view_state();
    engine.fail_after_workflow_appends_for_test(0);

    let events = engine
        .handle_runtime_command(
            "cmd_start_dag_late_fail",
            RuntimeCommand::StartAgentDag {
                goal: "late append fail".to_string(),
                tasks: vec![AgentDagTaskSpec {
                    task_id: "task_start_dag_late_fail".to_string(),
                    role: AgentRole::Coder,
                    title: "Patch safely".to_string(),
                    objective: "Should not leak live DAG state".to_string(),
                    dependencies: Vec::new(),
                    workspace: None,
                    file_scope: vec!["src".to_string()],
                    context_bundle_id: None,
                    required_evidence: vec!["patch".to_string()],
                    permission_policy: "scoped_mutation".to_string(),
                }],
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { command_id, reason }
            if command_id == "cmd_start_dag_late_fail"
                && reason.contains("injected workflow append failure")
    )));
    let live = engine.runtime_view_state();
    assert_eq!(live.agent_dags, before.agent_dags);
    assert_eq!(live.tasks, before.tasks);
    assert_eq!(live.merge_gates, before.merge_gates);
    assert_eq!(live.latest_evidence, before.latest_evidence);
}

#[test]
fn cancel_agent_task_rolls_back_when_workflow_append_fails() {
    let cwd = temp_dir("runtime_contract_cancel_task_workflow_fail_cwd");
    let home = temp_dir("runtime_contract_cancel_task_workflow_fail_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    start_single_patch_gate(&mut engine, &mut approver, "task_cancel_workflow_fail");
    let before = engine.runtime_view_state();
    engine.fail_next_workflow_append_for_test();

    let events = engine
        .handle_runtime_command(
            "cmd_cancel_workflow_fail",
            RuntimeCommand::CancelAgentTask {
                task_id: "task_cancel_workflow_fail".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { command_id, reason }
            if command_id == "cmd_cancel_workflow_fail"
                && reason.contains("injected workflow append failure")
    )));
    let live = engine.runtime_view_state();
    assert_eq!(live.tasks, before.tasks, "cancel task update leaked");
    assert_eq!(live.agent_dags, before.agent_dags);
    assert_eq!(live.merge_gates, before.merge_gates);
}

#[test]
fn start_agent_task_workflow_projection_failure_leaves_logs_and_replay_unchanged() {
    let cwd = temp_dir("runtime_contract_start_task_projection_fail_cwd");
    let home = temp_dir("runtime_contract_start_task_projection_fail_home");
    let provider = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::AssistantText {
            content: "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n".to_string(),
        },
        ModelEvent::Done,
    ]]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let session_id = engine.session_id().to_string();
    start_single_patch_gate(&mut engine, &mut approver, "task_start_projection_fail");
    let before = engine.runtime_view_state();
    let workflow_before = WorkflowStore::new(&home, &cwd)
        .unwrap()
        .load_agent_events()
        .unwrap();
    engine.fail_next_workflow_append_for_test();

    let events = engine
        .handle_runtime_command(
            "cmd_start_projection_fail",
            RuntimeCommand::StartAgentTask {
                task_id: "task_start_projection_fail".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { command_id, reason }
            if command_id == "cmd_start_projection_fail"
                && reason.contains("injected workflow append failure")
    )));
    let live = engine.runtime_view_state();
    assert_eq!(live.tasks, before.tasks, "task update leaked");
    assert_eq!(live.merge_gates, before.merge_gates, "gate update leaked");
    assert_eq!(
        live.latest_evidence, before.latest_evidence,
        "evidence leaked"
    );
    assert_eq!(
        live.context_items, before.context_items,
        "context item leaked"
    );
    assert_eq!(
        WorkflowStore::new(&home, &cwd)
            .unwrap()
            .load_agent_events()
            .unwrap(),
        workflow_before
    );
    assert_resumed_runtime_matches(&cwd, &home, &session_id, &live);
}

#[test]
fn merge_agent_patch_workflow_precommit_failure_leaves_file_unchanged() {
    let cwd = temp_dir("runtime_contract_merge_patch_restore_after_write_cwd");
    let home = temp_dir("runtime_contract_merge_patch_restore_after_write_home");
    std::fs::create_dir_all(cwd.join("src")).unwrap();
    std::fs::write(cwd.join("src/lib.rs"), "old\n").unwrap();
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    start_single_patch_gate(&mut engine, &mut approver, "task_merge_restore");
    let patch = b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let (patch_item, patch_canonical) = stored_canonical_context(
        &cwd,
        "task_merge_restore",
        "evidence-merge-restore-patch",
        "bundle-merge-restore-patch",
        ContextContentKind::Diff,
        patch,
    );
    let patch_hash = patch_canonical.source_hash.clone();
    engine.set_merge_gate_context_facts_for_test("bundle-merge-restore-patch", patch_item);
    let evidence_events = engine
        .handle_runtime_command(
            "cmd_merge_restore_evidence",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task_merge_restore".to_string(),
                evidence_id: Some("evidence-merge-restore-patch".to_string()),
                kind: "patch".to_string(),
                summary: "canonical patch".to_string(),
                path: None,
                source: Some("executor".to_string()),
                canonical: Some(patch_canonical),
            },
            &mut approver,
        )
        .unwrap();
    assert!(evidence_events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MergeGateUpdated { gate }
            if gate.gate_id == "gate-task_merge_restore"
                && gate.status == MergeGateStatus::Accepted
    )));
    let actor = engine.runtime_view_state().merge_gates[0].owner.clone();
    engine
        .handle_runtime_command(
            "cmd_accept_merge_restore",
            RuntimeCommand::AcceptMergeGate {
                gate_id: "gate-task_merge_restore".to_string(),
                actor: actor.clone(),
                reviewed_evidence: vec![viden_types::ReviewedEvidenceBinding {
                    evidence_id: "evidence-merge-restore-patch".to_string(),
                    source_hash: patch_hash,
                }],
                decision: Some("accept exact canonical patch".to_string()),
            },
            &mut approver,
        )
        .unwrap();
    let before = engine.runtime_view_state();
    let workflow_before = WorkflowStore::new(&home, &cwd)
        .unwrap()
        .load_agent_events()
        .unwrap();
    engine.fail_next_workflow_append_for_test();

    let events = engine
        .handle_runtime_command(
            "cmd_merge_restore",
            RuntimeCommand::MergeAgentPatch {
                gate_id: "gate-task_merge_restore".to_string(),
                actor,
                decision: Some("merge with rollback".to_string()),
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { command_id, reason }
            if command_id == "cmd_merge_restore"
                && reason.contains("injected workflow append failure")
    )));
    assert_eq!(
        std::fs::read_to_string(cwd.join("src/lib.rs")).unwrap(),
        "old\n"
    );
    let live = engine.runtime_view_state();
    assert_eq!(live.merge_gates, before.merge_gates, "gate update leaked");
    assert_eq!(live.tasks, before.tasks, "task update leaked");
    assert_eq!(
        WorkflowStore::new(&home, &cwd)
            .unwrap()
            .load_agent_events()
            .unwrap(),
        workflow_before
    );
}

#[test]
fn merge_agent_patch_workflow_precommit_failure_removes_created_file_and_empty_parents() {
    let cwd = temp_dir("runtime_contract_merge_patch_create_rollback_cwd");
    let home = temp_dir("runtime_contract_merge_patch_create_rollback_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let patch = b"diff --git a/generated/deep/new.txt b/generated/deep/new.txt\n--- /dev/null\n+++ b/generated/deep/new.txt\n@@ -0,0 +1 @@\n+created\n";
    prepare_patch_merge_gate(
        &mut engine,
        &mut approver,
        &cwd,
        "task_merge_create_rollback",
        patch,
    );
    let actor = engine.runtime_view_state().merge_gates[0].owner.clone();
    engine.fail_next_workflow_append_for_test();

    let events = engine
        .handle_runtime_command(
            "cmd_merge_create_rollback",
            RuntimeCommand::MergeAgentPatch {
                gate_id: "gate-task_merge_create_rollback".to_string(),
                actor,
                decision: Some("merge created file with rollback".to_string()),
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { command_id, reason }
            if command_id == "cmd_merge_create_rollback"
                && reason.contains("injected workflow append failure")
    )));
    assert!(!cwd.join("generated/deep/new.txt").exists());
    assert!(
        !cwd.join("generated").exists(),
        "rollback must remove empty parent directories created by this transaction"
    );
}

#[test]
fn merge_agent_patch_workflow_precommit_failure_restores_deleted_file() {
    let cwd = temp_dir("runtime_contract_merge_patch_delete_rollback_cwd");
    let home = temp_dir("runtime_contract_merge_patch_delete_rollback_home");
    std::fs::create_dir_all(cwd.join("src")).unwrap();
    std::fs::write(cwd.join("src/obsolete.txt"), "old\n").unwrap();
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let patch = b"diff --git a/src/obsolete.txt b/src/obsolete.txt\n--- a/src/obsolete.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-old\n";
    prepare_patch_merge_gate(
        &mut engine,
        &mut approver,
        &cwd,
        "task_merge_delete_rollback",
        patch,
    );
    let actor = engine.runtime_view_state().merge_gates[0].owner.clone();
    engine.fail_next_workflow_append_for_test();

    let events = engine
        .handle_runtime_command(
            "cmd_merge_delete_rollback",
            RuntimeCommand::MergeAgentPatch {
                gate_id: "gate-task_merge_delete_rollback".to_string(),
                actor,
                decision: Some("merge deleted file with rollback".to_string()),
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { command_id, reason }
            if command_id == "cmd_merge_delete_rollback"
                && reason.contains("injected workflow append failure")
    )));
    assert_eq!(
        std::fs::read_to_string(cwd.join("src/obsolete.txt")).unwrap(),
        "old\n"
    );
}

#[test]
fn record_agent_evidence_ignores_transcript_batch_failure_for_project_facts() {
    let cwd = temp_dir("runtime_contract_project_fact_transcript_batch_fail_cwd");
    let home = temp_dir("runtime_contract_project_fact_transcript_batch_fail_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let session_id = engine.session_id().to_string();
    start_single_patch_gate(&mut engine, &mut approver, "task_project_fact_batch_fail");
    let (item, canonical) = stored_canonical_context(
        &cwd,
        "task_project_fact_batch_fail",
        "evidence-project-fact-batch-fail",
        "bundle-project-fact-batch-fail",
        ContextContentKind::Diff,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );
    engine.set_merge_gate_context_facts_for_test("bundle-project-fact-batch-fail", item);
    engine.fail_after_transcript_appends_for_test(2);

    let events = engine
        .handle_runtime_command(
            "cmd_project_fact_batch_fail",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task_project_fact_batch_fail".to_string(),
                evidence_id: Some("evidence-project-fact-batch-fail".to_string()),
                kind: "patch".to_string(),
                summary: "canonical patch should commit without transcript projection".to_string(),
                path: None,
                source: Some("executor".to_string()),
                canonical: Some(canonical),
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MergeGateUpdated { gate }
            if gate.gate_id == "gate-task_project_fact_batch_fail"
                && gate.status == MergeGateStatus::Accepted
    )));
    assert!(transcript_runtime_events(&cwd, &home, &session_id).is_empty());
    assert_resumed_runtime_matches(&cwd, &home, &session_id, &engine.runtime_view_state());
}

#[test]
fn merge_agent_patch_revalidates_non_patch_required_evidence_before_writing() {
    let cwd = temp_dir("runtime_contract_merge_revalidates_non_patch_cwd");
    let home = temp_dir("runtime_contract_merge_revalidates_non_patch_home");
    std::fs::create_dir_all(cwd.join("src")).unwrap();
    std::fs::write(cwd.join("src/lib.rs"), "old\n").unwrap();
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let _ = start_gate_with_required(
        &mut engine,
        &mut approver,
        "task_merge_revalidate",
        vec!["patch".to_string(), "test".to_string()],
    );
    let patch = b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let (patch_item, patch_canonical) = stored_canonical_context(
        &cwd,
        "task_merge_revalidate",
        "evidence-merge-patch",
        "bundle-merge-patch",
        ContextContentKind::Diff,
        patch,
    );
    let patch_hash = patch_item.content_sha256.clone();
    engine.set_merge_gate_context_facts_for_test("bundle-merge-patch", patch_item);
    let (test_item, test_canonical) = stored_canonical_context(
        &cwd,
        "task_merge_revalidate",
        "evidence-merge-test",
        "bundle-merge-test",
        ContextContentKind::Text,
        b"cargo test -p viden-runtime merge_revalidates -- ok",
    );
    let test_hash = test_item.content_sha256.clone();
    engine.set_merge_gate_context_facts_for_test("bundle-merge-test", test_item);

    let patch_events = engine
        .handle_runtime_command(
            "cmd_merge_patch_evidence",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task_merge_revalidate".to_string(),
                evidence_id: Some("evidence-merge-patch".to_string()),
                kind: "patch".to_string(),
                summary: "canonical patch".to_string(),
                path: None,
                source: Some("executor".to_string()),
                canonical: Some(patch_canonical),
            },
            &mut approver,
        )
        .unwrap();
    assert_gate_status_with_reason(
        &patch_events,
        "gate-task_merge_revalidate",
        MergeGateStatus::CollectingEvidence,
        "missing_required_kind",
    );
    let test_events = engine
        .handle_runtime_command(
            "cmd_merge_test_evidence",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task_merge_revalidate".to_string(),
                evidence_id: Some("evidence-merge-test".to_string()),
                kind: "test".to_string(),
                summary: "canonical test".to_string(),
                path: None,
                source: Some("executor".to_string()),
                canonical: Some(test_canonical),
            },
            &mut approver,
        )
        .unwrap();
    assert!(test_events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MergeGateUpdated { gate }
            if gate.gate_id == "gate-task_merge_revalidate"
                && gate.status == MergeGateStatus::Accepted
    )));
    let actor = engine.runtime_view_state().merge_gates[0].owner.clone();
    engine
        .handle_runtime_command(
            "cmd_accept_merge_revalidate",
            RuntimeCommand::AcceptMergeGate {
                gate_id: "gate-task_merge_revalidate".to_string(),
                actor: actor.clone(),
                reviewed_evidence: vec![
                    viden_types::ReviewedEvidenceBinding {
                        evidence_id: "evidence-merge-patch".to_string(),
                        source_hash: patch_hash,
                    },
                    viden_types::ReviewedEvidenceBinding {
                        evidence_id: "evidence-merge-test".to_string(),
                        source_hash: test_hash.clone(),
                    },
                ],
                decision: Some("accept exact patch and test evidence".to_string()),
            },
            &mut approver,
        )
        .unwrap();

    std::fs::remove_file(blob_path_for_hash(&cwd, &test_hash)).unwrap();
    let gate_before_denial = engine
        .runtime_view_state()
        .merge_gates
        .into_iter()
        .find(|gate| gate.gate_id == "gate-task_merge_revalidate")
        .unwrap();
    let mut deny_merge = |_prompt| ApprovalResponse::deny(None);
    let denied = engine
        .handle_runtime_command(
            "cmd_merge_after_test_tamper_denied",
            RuntimeCommand::MergeAgentPatch {
                gate_id: "gate-task_merge_revalidate".to_string(),
                actor: actor.clone(),
                decision: Some("deny stale evidence recovery mutation".to_string()),
            },
            &mut deny_merge,
        )
        .unwrap();
    assert!(
        denied
            .iter()
            .any(|event| matches!(&event.kind, RuntimeEventKind::CommandRejected { .. })),
        "denied merge must not produce recovery mutations: {denied:#?}"
    );
    assert_eq!(
        engine
            .runtime_view_state()
            .merge_gates
            .into_iter()
            .find(|gate| gate.gate_id == "gate-task_merge_revalidate")
            .unwrap(),
        gate_before_denial
    );

    let events = engine
        .handle_runtime_command(
            "cmd_merge_after_test_tamper",
            RuntimeCommand::MergeAgentPatch {
                gate_id: "gate-task_merge_revalidate".to_string(),
                actor,
                decision: Some("merge only if all evidence still verifies".to_string()),
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CommandRejected { reason, .. }
            if reason.contains("missing_source")
    )));
    assert_eq!(
        engine
            .runtime_view_state()
            .merge_gates
            .into_iter()
            .find(|gate| gate.gate_id == "gate-task_merge_revalidate")
            .unwrap(),
        gate_before_denial
    );
    assert_eq!(
        std::fs::read_to_string(cwd.join("src/lib.rs")).unwrap(),
        "old\n"
    );
}

#[test]
fn merge_gate_accepts_fully_verified_canonical_evidence() {
    let cwd = temp_dir("runtime_contract_verified_canonical_gate_cwd");
    let home = temp_dir("runtime_contract_verified_canonical_gate_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    engine
        .handle_runtime_command(
            "cmd_agent_dag",
            RuntimeCommand::StartAgentDag {
                goal: "Accept canonical evidence".to_string(),
                tasks: vec![AgentDagTaskSpec {
                    task_id: "task_verified_patch".to_string(),
                    role: AgentRole::Coder,
                    title: "Patch with canonical source".to_string(),
                    objective: "Record a verified patch".to_string(),
                    dependencies: Vec::new(),
                    workspace: None,
                    file_scope: vec!["crates/runtime".to_string()],
                    context_bundle_id: None,
                    required_evidence: vec!["patch".to_string()],
                    permission_policy: "scoped_mutation".to_string(),
                }],
            },
            &mut approver,
        )
        .unwrap();
    let (item, canonical) = stored_canonical_context(
        &cwd,
        "task_verified_patch",
        "evidence-canonical-patch",
        "bundle-patch",
        ContextContentKind::Diff,
        b"verified canonical patch evidence",
    );
    engine.set_merge_gate_context_facts_for_test("bundle-patch", item);

    let events = engine
        .handle_runtime_command(
            "cmd_canonical_evidence",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task_verified_patch".to_string(),
                evidence_id: Some("evidence-canonical-patch".to_string()),
                kind: "patch".to_string(),
                summary: "canonical patch evidence".to_string(),
                path: None,
                source: Some("executor".to_string()),
                canonical: Some(canonical),
            },
            &mut approver,
        )
        .unwrap();

    assert!(events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::MergeGateUpdated { gate }
                if gate.gate_id == "gate-task_verified_patch"
                    && gate.status == MergeGateStatus::Accepted
                    && gate.decision.is_none()
        )
    }));
}

#[test]
fn merge_gate_blocks_when_canonical_blob_is_missing_from_store() {
    let cwd = temp_dir("runtime_contract_missing_blob_gate_cwd");
    let home = temp_dir("runtime_contract_missing_blob_gate_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    start_single_patch_gate(&mut engine, &mut approver, "task_missing_blob");
    let (item, canonical) = stored_canonical_context(
        &cwd,
        "task_missing_blob",
        "evidence-missing-blob",
        "bundle-missing-blob",
        ContextContentKind::Diff,
        b"real canonical patch evidence",
    );
    engine.set_merge_gate_context_facts_for_test("bundle-missing-blob", item.clone());
    fs::remove_file(blob_path_for_hash(&cwd, &item.content_sha256)).unwrap();

    let events = engine
        .handle_runtime_command(
            "cmd_missing_blob_evidence",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task_missing_blob".to_string(),
                evidence_id: Some("evidence-missing-blob".to_string()),
                kind: "patch".to_string(),
                summary: "real canonical patch evidence".to_string(),
                path: None,
                source: Some("executor".to_string()),
                canonical: Some(canonical),
            },
            &mut approver,
        )
        .unwrap();

    assert_gate_status_with_reason(
        &events,
        "gate-task_missing_blob",
        MergeGateStatus::Blocked,
        "missing_source",
    );
}

#[test]
fn merge_gate_blocks_when_canonical_blob_hash_is_tampered() {
    let cwd = temp_dir("runtime_contract_tampered_blob_gate_cwd");
    let home = temp_dir("runtime_contract_tampered_blob_gate_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    start_single_patch_gate(&mut engine, &mut approver, "task_tampered_blob");
    let (item, canonical) = stored_canonical_context(
        &cwd,
        "task_tampered_blob",
        "evidence-tampered-blob",
        "bundle-tampered-blob",
        ContextContentKind::Diff,
        b"real canonical patch evidence",
    );
    engine.set_merge_gate_context_facts_for_test("bundle-tampered-blob", item.clone());
    fs::write(blob_path_for_hash(&cwd, &item.content_sha256), b"tampered").unwrap();

    let events = engine
        .handle_runtime_command(
            "cmd_tampered_blob_evidence",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task_tampered_blob".to_string(),
                evidence_id: Some("evidence-tampered-blob".to_string()),
                kind: "patch".to_string(),
                summary: "real canonical patch evidence".to_string(),
                path: None,
                source: Some("executor".to_string()),
                canonical: Some(canonical),
            },
            &mut approver,
        )
        .unwrap();

    assert_gate_status_with_reason(
        &events,
        "gate-task_tampered_blob",
        MergeGateStatus::Blocked,
        "hash_mismatch",
    );
}

#[test]
fn merge_gate_accepts_all_required_canonical_evidence_kinds_idempotently_after_replay() {
    let cwd = temp_dir("runtime_contract_all_canonical_kinds_cwd");
    let home = temp_dir("runtime_contract_all_canonical_kinds_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let mut recorded_events = engine
        .handle_runtime_command(
            "cmd_agent_dag",
            RuntimeCommand::StartAgentDag {
                goal: "Require all canonical evidence kinds".to_string(),
                tasks: vec![AgentDagTaskSpec {
                    task_id: "task_all_kinds".to_string(),
                    role: AgentRole::ReleaseOperator,
                    title: "Release gate".to_string(),
                    objective: "Collect all gate evidence".to_string(),
                    dependencies: Vec::new(),
                    workspace: None,
                    file_scope: vec![".".to_string()],
                    context_bundle_id: None,
                    required_evidence: vec![
                        "patch".to_string(),
                        "test".to_string(),
                        "review".to_string(),
                        "doc".to_string(),
                        "release".to_string(),
                    ],
                    permission_policy: "scoped_mutation".to_string(),
                }],
            },
            &mut approver,
        )
        .unwrap();

    let mut patch_canonical = None;
    for (index, (kind, context_kind)) in [
        ("patch", ContextContentKind::Diff),
        ("test_result", ContextContentKind::Log),
        ("review", ContextContentKind::Text),
        ("doc_update", ContextContentKind::Text),
        ("release_artifact", ContextContentKind::Text),
    ]
    .into_iter()
    .enumerate()
    {
        let bundle_id = format!("bundle-{kind}");
        let evidence_id = format!("evidence-{kind}");
        let (item, canonical) = stored_canonical_context(
            &cwd,
            "task_all_kinds",
            &evidence_id,
            &bundle_id,
            context_kind,
            format!("canonical {kind} evidence").as_bytes(),
        );
        engine.set_merge_gate_context_facts_for_test(&bundle_id, item);
        if kind == "patch" {
            patch_canonical = Some(canonical.clone());
        }
        let mut events = engine
            .handle_runtime_command(
                format!("cmd_canonical_{kind}"),
                RuntimeCommand::RecordAgentEvidence {
                    gate_id: "gate-task_all_kinds".to_string(),
                    evidence_id: Some(evidence_id.clone()),
                    kind: kind.to_string(),
                    summary: format!("canonical {kind} evidence"),
                    path: None,
                    source: Some("executor".to_string()),
                    canonical: Some(canonical),
                },
                &mut approver,
            )
            .unwrap();
        if index == 4 {
            assert!(events.iter().any(|event| {
                matches!(
                    &event.kind,
                    RuntimeEventKind::MergeGateUpdated { gate }
                        if gate.gate_id == "gate-task_all_kinds"
                            && gate.status == MergeGateStatus::Accepted
                            && gate.evidence_ids.len() == 5
                )
            }));
        }
        recorded_events.append(&mut events);
    }

    let duplicate_events = engine
        .handle_runtime_command(
            "cmd_duplicate_patch",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task_all_kinds".to_string(),
                evidence_id: Some("evidence-patch".to_string()),
                kind: "patch".to_string(),
                summary: "duplicate canonical patch evidence".to_string(),
                path: None,
                source: Some("executor".to_string()),
                canonical: patch_canonical,
            },
            &mut approver,
        )
        .unwrap();
    recorded_events.extend(duplicate_events);

    let live = engine.runtime_view_state();
    let gate = live
        .merge_gates
        .iter()
        .find(|gate| gate.gate_id == "gate-task_all_kinds")
        .unwrap();
    assert_eq!(gate.status, MergeGateStatus::Accepted);
    assert_eq!(gate.evidence_ids.len(), 5);
    assert_eq!(
        live.latest_evidence
            .iter()
            .filter(|evidence| evidence.id == "evidence-patch")
            .count(),
        1
    );
    assert_eq!(live.canonical_evidence.len(), 5);
    let live_json = serde_json::to_string(&live).unwrap();
    assert!(!live_json.contains("storage_path"));

    let mut replayed = RuntimeViewState::new(live.snapshot.clone());
    for event in &recorded_events {
        replayed.apply_event(event);
    }
    assert_eq!(replayed.merge_gates, live.merge_gates);
    assert_eq!(replayed.latest_evidence, live.latest_evidence);
}

#[test]
fn assistant_summary_never_becomes_canonical_evidence_across_resume() {
    let cwd = temp_dir("runtime_contract_canonical_resume_cwd");
    let home = temp_dir("runtime_contract_canonical_resume_home");
    let patch = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\nitem_id=ctxi_fake source_hash=abcd verification=verified quality=pass permission_snapshot_id=perm_fake";
    let provider_a = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::AssistantText {
            content: patch.to_string(),
        },
        ModelEvent::Done,
    ]]));
    let mut engine_a = SessionEngine::new_with_home(&cwd, provider_a, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let session_id = engine_a.session_id().to_string();
    start_single_patch_gate(&mut engine_a, &mut approver, "task_resume_gate");
    let live_events = engine_a
        .handle_runtime_command(
            "cmd_start_resume_gate",
            RuntimeCommand::StartAgentTask {
                task_id: "task_resume_gate".to_string(),
            },
            &mut approver,
        )
        .unwrap();
    assert!(live_events.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::MergeGateUpdated { gate }
                if gate.gate_id == "gate-task_resume_gate"
                    && gate.status == MergeGateStatus::CollectingEvidence
        )
    }));
    let live = engine_a.runtime_view_state();

    let provider_b = Box::new(SequenceProvider::new(vec![]));
    let mut engine_b = SessionEngine::new_with_home(&cwd, provider_b, Some(home.clone())).unwrap();
    let resume_events = engine_b
        .process_input_with_approval(&format!("/resume {session_id}"), &mut approver)
        .unwrap();
    assert!(resume_events.iter().any(
        |event| matches!(event, crate::EngineEvent::Command(text) if text.contains("Resumed session"))
    ));
    let resumed = engine_b.runtime_view_state();

    assert_eq!(resumed.merge_gates, live.merge_gates);
    assert_eq!(resumed.latest_evidence, live.latest_evidence);
    assert_eq!(resumed.canonical_evidence, live.canonical_evidence);
    assert_eq!(resumed.context_bundles, live.context_bundles);
    assert_eq!(resumed.context_items, live.context_items);
    let assistant_evidence = live
        .latest_evidence
        .iter()
        .find(|evidence| evidence.id == "evidence-task_resume_gate-task_summary")
        .expect("assistant completion remains display evidence");
    assert_eq!(assistant_evidence.kind, "task_summary");
    assert_eq!(assistant_evidence.canonical, None);
    assert!(live.canonical_evidence.is_empty());
    assert!(live.context_items.iter().all(|item| {
        item.evidence_id.as_deref() != Some("evidence-task_resume_gate-task_summary")
    }));

    let entries = SessionStore::new_with_home(home.clone(), &cwd, Some(session_id.clone()))
        .unwrap()
        .load_entries()
        .unwrap();
    assert!(
        entries
            .iter()
            .all(|entry| !matches!(entry, viden_types::TranscriptEntry::RuntimeEvent { .. })),
        "new project agent facts must not be dual-written to transcript runtime_event entries"
    );

    let workflow_events = WorkflowStore::new(home, &cwd)
        .unwrap()
        .load_agent_events()
        .unwrap();
    assert!(workflow_events.iter().all(|event| {
        !matches!(
            event.event_type.as_str(),
            "merge_gate_proposed" | "agent_task_completed"
        )
    }));
    assert!(workflow_events.iter().any(|event| {
        event.event_type == "runtime_projection_batch"
            && event
                .payload
                .get("command_id")
                .is_some_and(|id| id == "cmd_agent_dag_task_resume_gate")
            && event
                .payload
                .get("runtime_event_kinds")
                .is_some_and(|kinds| {
                    kinds.contains("agent_dag_updated") && kinds.contains("merge_gate_updated")
                })
    }));
    assert!(workflow_events.iter().any(|event| {
        event.event_type == "runtime_projection_batch"
            && event
                .payload
                .get("command_id")
                .is_some_and(|id| id == "cmd_start_resume_gate")
            && event
                .payload
                .get("runtime_event_kinds")
                .is_some_and(|kinds| kinds.contains("evidence_recorded"))
    }));
    assert!(workflow_events.iter().any(|event| {
        event.event_type == "runtime_projection_batch"
            && event
                .payload
                .get("command_id")
                .is_some_and(|id| id == "cmd_start_resume_gate")
            && event
                .payload
                .get("runtime_event_kinds")
                .is_some_and(|kinds| kinds.contains("merge_gate_updated"))
    }));
}

#[test]
fn workflow_replay_keeps_legacy_single_runtime_projection_compatible() {
    let cwd = temp_dir("runtime_contract_legacy_single_projection_cwd");
    let home = temp_dir("runtime_contract_legacy_single_projection_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let session_id = engine.session_id().to_string();
    start_single_patch_gate(&mut engine, &mut approver, "task_legacy_projection");
    let live = engine.runtime_view_state();
    let dag_id = live.agent_dags[0].dag_id.clone();
    let mut gate = live
        .merge_gates
        .iter()
        .find(|gate| gate.gate_id == "gate-task_legacy_projection")
        .unwrap()
        .clone();
    gate.status = MergeGateStatus::NeedsChanges;
    gate.decision = Some(viden_types::MergeGateDecision {
        outcome: viden_types::MergeGateDecisionOutcome::Legacy,
        reason: "legacy projection replay".to_string(),
        owner: viden_types::RuntimeOwner::default(),
        evidence_ids: Vec::new(),
        reviewed_evidence: Vec::new(),
        review_request_id: None,
        audit_id: "legacy".to_string(),
        decided_at: 0,
    });
    let runtime_event = RuntimeEvent::new(1, RuntimeEventKind::MergeGateUpdated { gate });
    let mut payload = BTreeMap::new();
    payload.insert(
        "runtime_event_kind".to_string(),
        "merge_gate_updated".to_string(),
    );
    payload.insert(
        "runtime_event_json".to_string(),
        serde_json::to_string(&runtime_event).unwrap(),
    );
    WorkflowStore::new(&home, &cwd)
        .unwrap()
        .append_agent_event(&WorkflowAgentEvent {
            event_id: "legacy-single-projection".to_string(),
            dag_id,
            task_id: Some("task_legacy_projection".to_string()),
            event_type: "runtime_projection".to_string(),
            timestamp: 1,
            origin_session_id: Some(session_id.clone()),
            payload,
        })
        .unwrap();

    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut resumed = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let _ = resumed
        .process_input_with_approval(&format!("/resume {session_id}"), &mut approver)
        .unwrap();
    let resumed_gate = resumed
        .runtime_view_state()
        .merge_gates
        .into_iter()
        .find(|gate| gate.gate_id == "gate-task_legacy_projection")
        .unwrap();
    assert_eq!(resumed_gate.status, MergeGateStatus::NeedsChanges);
    assert_eq!(
        resumed_gate.decision.as_deref(),
        Some("legacy projection replay")
    );
}

#[test]
fn workflow_replay_dedupes_duplicate_runtime_projection_batch_id() {
    let cwd = temp_dir("runtime_contract_duplicate_projection_batch_cwd");
    let home = temp_dir("runtime_contract_duplicate_projection_batch_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let session_id = engine.session_id().to_string();
    start_single_patch_gate(&mut engine, &mut approver, "task_duplicate_batch");
    let (item, canonical) = stored_canonical_context(
        &cwd,
        "task_duplicate_batch",
        "evidence-duplicate-batch",
        "bundle-duplicate-batch",
        ContextContentKind::Diff,
        b"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
    );
    engine.set_merge_gate_context_facts_for_test("bundle-duplicate-batch", item);
    engine
        .handle_runtime_command(
            "cmd_duplicate_batch_evidence",
            RuntimeCommand::RecordAgentEvidence {
                gate_id: "gate-task_duplicate_batch".to_string(),
                evidence_id: Some("evidence-duplicate-batch".to_string()),
                kind: "patch".to_string(),
                summary: "canonical duplicate batch evidence".to_string(),
                path: None,
                source: Some("executor".to_string()),
                canonical: Some(canonical),
            },
            &mut approver,
        )
        .unwrap();
    let live = engine.runtime_view_state();
    let batch = workflow_agent_events(&cwd, &home)
        .into_iter()
        .find(|event| {
            event.event_type == "runtime_projection_batch"
                && event
                    .payload
                    .get("command_id")
                    .is_some_and(|id| id == "cmd_duplicate_batch_evidence")
        })
        .unwrap();
    let mut duplicate = batch.clone();
    duplicate.event_id = "duplicate-runtime-projection-batch".to_string();
    WorkflowStore::new(&home, &cwd)
        .unwrap()
        .append_agent_event(&duplicate)
        .unwrap();

    assert_resumed_runtime_matches(&cwd, &home, &session_id, &live);
}

#[test]
fn merge_gate_reports_stable_canonical_failure_reasons() {
    for (case, expected_status, expected_reason, configure) in canonical_failure_cases() {
        let cwd = temp_dir(&format!("runtime_contract_canonical_failure_{case}_cwd"));
        let home = temp_dir(&format!("runtime_contract_canonical_failure_{case}_home"));
        let provider = Box::new(SequenceProvider::new(vec![]));
        let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
        let mut approver = |_prompt| ApprovalResponse::allow_once(None);
        let task_id = format!("task_{case}");
        engine
            .handle_runtime_command(
                "cmd_agent_dag",
                RuntimeCommand::StartAgentDag {
                    goal: format!("Canonical failure {case}"),
                    tasks: vec![AgentDagTaskSpec {
                        task_id: task_id.clone(),
                        role: AgentRole::Tester,
                        title: format!("Canonical failure {case}"),
                        objective: "Record canonical test evidence".to_string(),
                        dependencies: Vec::new(),
                        workspace: None,
                        file_scope: vec!["crates/runtime".to_string()],
                        context_bundle_id: None,
                        required_evidence: vec!["test".to_string()],
                        permission_policy: "read_only".to_string(),
                    }],
                },
                &mut approver,
            )
            .unwrap();

        let mut canonical = if case == "missing_source" {
            canonical_reference(&task_id, "ctxi-missing-test", "bundle-test", "ab")
        } else {
            let (item, canonical) = stored_canonical_context(
                &cwd,
                &task_id,
                &format!("evidence-{case}"),
                "bundle-test",
                ContextContentKind::Log,
                format!("canonical test evidence for {case}").as_bytes(),
            );
            engine.set_merge_gate_context_facts_for_test("bundle-test", item);
            canonical
        };
        configure(&mut canonical);

        let events = engine
            .handle_runtime_command(
                "cmd_canonical_evidence",
                RuntimeCommand::RecordAgentEvidence {
                    gate_id: format!("gate-{task_id}"),
                    evidence_id: Some(format!("evidence-{case}")),
                    kind: "test_result".to_string(),
                    summary: "canonical test evidence".to_string(),
                    path: None,
                    source: Some("executor".to_string()),
                    canonical: Some(canonical),
                },
                &mut approver,
            )
            .unwrap();

        assert!(
            events.iter().any(|event| {
                matches!(
                    &event.kind,
                    RuntimeEventKind::MergeGateUpdated { gate }
                        if gate.gate_id == format!("gate-{task_id}")
                            && gate.status == expected_status
                            && gate
                                .decision
                                .as_deref()
                                .is_some_and(|decision| decision.contains(expected_reason))
                ) || matches!(
                    &event.kind,
                    RuntimeEventKind::CommandRejected { reason, .. }
                        if reason.contains(expected_reason)
                )
            }),
            "case {case} did not report {expected_status:?}/{expected_reason}: {events:#?}"
        );
    }
}

type CanonicalFailureConfigurator = fn(&mut CanonicalEvidenceReference);

fn canonical_failure_cases() -> Vec<(
    &'static str,
    MergeGateStatus,
    &'static str,
    CanonicalFailureConfigurator,
)> {
    vec![
        (
            "hash_mismatch",
            MergeGateStatus::Blocked,
            "hash_mismatch",
            |canonical| {
                canonical.source_hash = "cd".repeat(32);
            },
        ),
        (
            "missing_source",
            MergeGateStatus::Blocked,
            "missing_source",
            |_canonical| {},
        ),
        (
            "wrong_scope",
            MergeGateStatus::Blocked,
            "scope_mismatch",
            |canonical| {
                canonical.evidence_scope = ContextScope::Task("task-other".to_string());
                canonical.permission_scope = ContextScope::Task("task-other".to_string());
            },
        ),
        (
            "missing_producer",
            MergeGateStatus::Blocked,
            "missing_producer",
            |canonical| {
                canonical.producer.identity.clear();
            },
        ),
    ]
}

fn start_single_patch_gate(
    engine: &mut SessionEngine,
    approver: &mut impl FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    task_id: &str,
) {
    let _ = start_gate_with_required(engine, approver, task_id, vec!["patch".to_string()]);
}

fn prepare_patch_merge_gate(
    engine: &mut SessionEngine,
    approver: &mut impl FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    cwd: &std::path::Path,
    task_id: &str,
    patch: &[u8],
) {
    start_single_patch_gate(engine, approver, task_id);
    let evidence_id = format!("evidence-{task_id}");
    let bundle_id = format!("bundle-{task_id}");
    let (item, canonical) = stored_canonical_context(
        cwd,
        task_id,
        &evidence_id,
        &bundle_id,
        ContextContentKind::Diff,
        patch,
    );
    let source_hash = canonical.source_hash.clone();
    engine.set_merge_gate_context_facts_for_test(&bundle_id, item);

    let events = engine
        .handle_runtime_command(
            format!("cmd_evidence_{task_id}"),
            RuntimeCommand::RecordAgentEvidence {
                gate_id: format!("gate-{task_id}"),
                evidence_id: Some(evidence_id),
                kind: "patch".to_string(),
                summary: "canonical patch".to_string(),
                path: None,
                source: Some("executor".to_string()),
                canonical: Some(canonical),
            },
            approver,
        )
        .unwrap();
    assert!(events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MergeGateUpdated { gate }
            if gate.gate_id == format!("gate-{task_id}")
                && gate.status == MergeGateStatus::Accepted
    )));
    let actor = engine
        .runtime_view_state()
        .merge_gates
        .into_iter()
        .find(|gate| gate.gate_id == format!("gate-{task_id}"))
        .unwrap()
        .owner;
    engine
        .handle_runtime_command(
            format!("cmd_accept_{task_id}"),
            RuntimeCommand::AcceptMergeGate {
                gate_id: format!("gate-{task_id}"),
                actor,
                reviewed_evidence: vec![viden_types::ReviewedEvidenceBinding {
                    evidence_id: format!("evidence-{task_id}"),
                    source_hash,
                }],
                decision: Some("accept exact canonical patch".to_string()),
            },
            approver,
        )
        .unwrap();
}

fn start_gate_with_required(
    engine: &mut SessionEngine,
    approver: &mut impl FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    task_id: &str,
    required_evidence: Vec<String>,
) -> Vec<RuntimeEvent> {
    engine
        .handle_runtime_command(
            format!("cmd_agent_dag_{task_id}"),
            RuntimeCommand::StartAgentDag {
                goal: format!("Require canonical evidence for {task_id}"),
                tasks: vec![AgentDagTaskSpec {
                    task_id: task_id.to_string(),
                    role: AgentRole::Coder,
                    title: "Canonical evidence gate".to_string(),
                    objective: "Record verified evidence".to_string(),
                    dependencies: Vec::new(),
                    workspace: None,
                    file_scope: vec!["crates/runtime".to_string()],
                    context_bundle_id: None,
                    required_evidence,
                    permission_policy: "scoped_mutation".to_string(),
                }],
            },
            approver,
        )
        .unwrap()
}

fn stored_canonical_context(
    cwd: &std::path::Path,
    task_id: &str,
    evidence_id: &str,
    bundle_id: &str,
    kind: ContextContentKind,
    content: &[u8],
) -> (ContextItemRecord, CanonicalEvidenceReference) {
    let mut store = ContextEngine::open(cwd.join(".viden").join("context-engine")).unwrap();
    let stored = store
        .store(ContextPutRequest {
            scope: ContextScope::Task(task_id.to_string()),
            kind,
            content,
            evidence_id: Some(evidence_id.to_string()),
        })
        .unwrap();
    let canonical = canonical_reference(
        task_id,
        &stored.item.item_id,
        bundle_id,
        &stored.item.content_sha256,
    );
    (stored.item, canonical)
}

fn blob_path_for_hash(cwd: &std::path::Path, hash: &str) -> std::path::PathBuf {
    cwd.join(".viden")
        .join("context-engine")
        .join("blobs")
        .join(&hash[..2])
        .join(hash)
}

fn assert_resumed_runtime_matches(
    cwd: &std::path::Path,
    home: &std::path::Path,
    session_id: &str,
    expected: &RuntimeViewState,
) {
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut resumed =
        SessionEngine::new_with_home(cwd, provider, Some(home.to_path_buf())).unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let resume_events = resumed
        .process_input_with_approval(&format!("/resume {session_id}"), &mut approver)
        .unwrap();
    assert!(resume_events.iter().any(
        |event| matches!(event, crate::EngineEvent::Command(text) if text.contains("Resumed session"))
    ));
    let actual = resumed.runtime_view_state();
    assert_eq!(actual.merge_gates, expected.merge_gates);
    assert_eq!(actual.latest_evidence, expected.latest_evidence);
    assert_eq!(actual.tasks, expected.tasks);
}

fn assert_gate_status_with_reason(
    events: &[RuntimeEvent],
    gate_id: &str,
    status: MergeGateStatus,
    reason: &str,
) {
    assert!(
        events.iter().any(|event| {
            matches!(
                &event.kind,
                RuntimeEventKind::MergeGateUpdated { gate }
                    if gate.gate_id == gate_id
                        && gate.status == status
                        && gate
                            .decision
                            .as_deref()
                            .is_some_and(|decision| decision.contains(reason))
            ) || matches!(
                &event.kind,
                RuntimeEventKind::CommandRejected { reason: rejected, .. }
                    if rejected.contains(reason)
            )
        }),
        "expected {gate_id} to be {status:?} with {reason}: {events:#?}"
    );
}

fn canonical_reference(
    task_id: &str,
    item_id: &str,
    bundle_id: &str,
    source_hash: &str,
) -> CanonicalEvidenceReference {
    CanonicalEvidenceReference {
        item_id: item_id.to_string(),
        bundle_id: bundle_id.to_string(),
        source_hash: if source_hash.len() == 64 {
            source_hash.to_string()
        } else {
            source_hash.repeat(32)
        },
        producer: EvidenceProducer {
            identity: "executor".to_string(),
            role: "coder".to_string(),
            task_id: task_id.to_string(),
        },
        permission_snapshot_id: Some(format!("perm-{task_id}")),
        permission_scope: ContextScope::Task(task_id.to_string()),
        evidence_scope: ContextScope::Task(task_id.to_string()),
        verification: EvidenceVerificationState::Verified,
        quality: EvidenceQualityFacts {
            status: EvidenceQualityStatus::Pass,
            reason_codes: Vec::new(),
        },
    }
}

#[test]
fn runtime_view_state_emits_lane_facts_from_core_store_legacy_lane_statuses() {
    let cwd = temp_dir("runtime_contract_lane_cwd");
    let home = temp_dir("runtime_contract_lane_home");
    let lane_dir = cwd.join(".viden");
    fs::create_dir_all(&lane_dir).unwrap();
    fs::write(
        lane_dir.join("lanes.tsv"),
        include_str!("../../../types/tests/fixtures/frontend-contract-v1/legacy-lanes.tsv"),
    )
    .unwrap();
    let provider = Box::new(SequenceProvider::new(vec![]));
    let engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();

    // The legacy file is migration input only. Runtime projection must remain
    // available after the source disappears and must not import it twice.
    fs::remove_file(lane_dir.join("lanes.tsv")).unwrap();

    let view = engine.runtime_view_state();

    assert_eq!(view.lanes.len(), 4);
    assert_eq!(
        view.lanes
            .iter()
            .find(|lane| lane.id == "L-start")
            .unwrap()
            .status,
        LaneStatus::Starting
    );
    assert_eq!(
        view.lanes
            .iter()
            .find(|lane| lane.id == "L-conflict")
            .unwrap()
            .status,
        LaneStatus::Blocked
    );
    for lane_id in ["L-detached", "L-stopped"] {
        assert_eq!(
            view.lanes
                .iter()
                .find(|lane| lane.id == lane_id)
                .unwrap()
                .status,
            LaneStatus::Detached
        );
    }

    let workflow_store = WorkflowStore::new(&home, &cwd).unwrap();
    assert_eq!(workflow_store.load_lane_events().unwrap().len(), 1);
    assert!(workflow_store.paths().lanes_log.exists());

    let second_provider = Box::new(SequenceProvider::new(vec![]));
    let second_engine =
        SessionEngine::new_with_home(&cwd, second_provider, Some(home.clone())).unwrap();
    assert_eq!(second_engine.runtime_view_state().lanes.len(), 4);
    assert_eq!(workflow_store.load_lane_events().unwrap().len(), 1);
}

#[test]
fn lane_runtime_owner_restart_projection_never_synthesizes_from_durable_lanes() {
    let cwd = temp_dir("lane_runtime_owner_contract_cwd");
    let home = temp_dir("lane_runtime_owner_contract_home");
    let lane_dir = cwd.join(".viden");
    fs::create_dir_all(&lane_dir).unwrap();
    fs::write(
        lane_dir.join("lanes.tsv"),
        include_str!("../../../types/tests/fixtures/frontend-contract-v1/legacy-lanes.tsv"),
    )
    .unwrap();
    let provider = Box::new(SequenceProvider::new(vec![]));
    let engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();

    let first = engine.runtime_view_state();
    assert!(!first.lanes.is_empty());
    assert!(first.lane_runtime_owners.is_empty());

    let provider = Box::new(SequenceProvider::new(vec![]));
    let restarted = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let restarted = restarted.runtime_view_state();
    assert!(!restarted.lanes.is_empty());
    assert!(restarted.lane_runtime_owners.is_empty());
}

#[test]
fn legacy_lane_migration_runs_once_at_resume_and_runtime_replays_typed_state() {
    let cwd = temp_dir("runtime_contract_lane_resume_cwd");
    let home = temp_dir("runtime_contract_lane_resume_home");
    let provider_a = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::AssistantText {
            content: "seed resumable session".to_string(),
        },
        ModelEvent::Done,
    ]]));
    let mut original = SessionEngine::new_with_home(&cwd, provider_a, Some(home.clone())).unwrap();
    let original_session_id = original.session_id().to_string();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    original
        .process_input_with_approval("seed the session", &mut approver)
        .unwrap();
    drop(original);

    let provider_b = Box::new(SequenceProvider::new(vec![]));
    let mut resumed = SessionEngine::new_with_home(&cwd, provider_b, Some(home.clone())).unwrap();
    let legacy_path = cwd.join(".viden").join("lanes.tsv");
    fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
    fs::write(
        &legacy_path,
        include_str!("../../../types/tests/fixtures/frontend-contract-v1/legacy-lanes.tsv"),
    )
    .unwrap();

    let resume_events = resumed
        .process_input_with_approval(&format!("/resume {original_session_id}"), &mut approver)
        .unwrap();
    assert!(resume_events.iter().any(
        |event| matches!(event, crate::EngineEvent::Command(text) if text.contains("Resumed session"))
    ));

    let workflow_store = WorkflowStore::new(&home, &cwd).unwrap();
    let imported = workflow_store.load_lane_state().unwrap();
    assert_eq!(imported.lanes().len(), 4);
    let audit = imported
        .migration(viden_workflows::lanes::LEGACY_LANES_MIGRATION_ID)
        .unwrap();
    assert_eq!(
        audit.origin_session_id.as_deref(),
        Some(original_session_id.as_str())
    );
    assert_eq!(workflow_store.load_lane_events().unwrap().len(), 1);

    workflow_store
        .append_lane_event_checked(&LaneEvent::status_changed(
            "evt_runtime_typed_lane_replay",
            "L-start",
            LaneStatus::Done,
            "typed replay wins",
            20,
            Some(original_session_id.clone()),
        ))
        .unwrap();
    fs::write(
        &legacy_path,
        "malformed legacy state that runtime must not read\n",
    )
    .unwrap();

    let projected = resumed.runtime_view_state();
    let lane = projected
        .lanes
        .iter()
        .find(|lane| lane.id == "L-start")
        .unwrap();
    assert_eq!(lane.status, LaneStatus::Done);
    assert_eq!(lane.summary, "typed replay wins");

    resumed
        .process_input_with_approval(&format!("/resume {original_session_id}"), &mut approver)
        .unwrap();
    let replayed = workflow_store.load_lane_state().unwrap();
    assert_eq!(replayed.lanes().len(), 4);
    assert_eq!(replayed.migrations().len(), 1);
    assert_eq!(workflow_store.load_lane_events().unwrap().len(), 2);
}

#[test]
fn runtime_view_state_reports_corrupt_lane_store() {
    let cwd = temp_dir("runtime_contract_corrupt_lane_cwd");
    let home = temp_dir("runtime_contract_corrupt_lane_home");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let workflow_store = WorkflowStore::new(&home, &cwd).unwrap();
    let secret = "customer-secret-lane";
    let invalid_event = LaneEvent::status_changed(
        "evt_secret_lane",
        secret,
        LaneStatus::Running,
        "must not leak",
        10,
        None,
    );
    fs::write(
        &workflow_store.paths().lanes_log,
        format!("{}\n", serde_json::to_string(&invalid_event).unwrap()),
    )
    .unwrap();

    let view = engine.runtime_view_state();

    assert!(view.errors.iter().any(|error| {
        error.recoverable
            && error.message.contains("lane_state_unavailable")
            && error.hint.as_deref() == Some("Repair or restore the project's lanes.jsonl log.")
    }));
    assert!(
        view.errors
            .iter()
            .all(|error| !error.message.contains(secret))
    );
}

#[test]
fn runtime_view_state_emits_tracked_acp_session_jobs() {
    let cwd = temp_dir("runtime_contract_acp_job_cwd");
    let home = temp_dir("runtime_contract_acp_job_home");
    let agent_dir = cwd.join(".viden").join("agents");
    fs::create_dir_all(&agent_dir).unwrap();
    let result_path = agent_dir.join("acp-1.result.md");
    let log_path = agent_dir.join("acp-1.jsonl");
    let baseline_path = agent_dir.join("acp-1.baseline.status");
    fs::write(
        &result_path,
        "# ACP session result\n\nsession: session_1\nstatus: completed\n\nimplemented adapter",
    )
    .unwrap();
    fs::write(&log_path, "{\"method\":\"session/update\"}\n").unwrap();
    fs::write(&baseline_path, "").unwrap();
    fs::write(
        agent_dir.join("acp-1.runtime-events.jsonl"),
        [
            serde_json::to_string(&RuntimeEvent::new(
                0,
                RuntimeEventKind::AgentSessionStarted {
                    session: AgentSessionView {
                        session_id: "acp-1".to_string(),
                        lane_id: "lane-acp".to_string(),
                        agent_id: "kiro-acp".to_string(),
                        model: None,
                        status: AgentSessionStatus::Running,
                        owner: RuntimeOwner {
                            lane_id: Some("lane-acp".to_string()),
                            session_id: Some("acp-1".to_string()),
                            ..RuntimeOwner::default()
                        },
                        task: "implement adapter".to_string(),
                        diagnostic: None,
                        output: None,
                    },
                },
            ))
            .unwrap(),
            serde_json::to_string(&RuntimeEvent::new(
                1,
                RuntimeEventKind::AssistantDelta {
                    message_id: "acp-session-session_1".to_string(),
                    task_id: None,
                    session_id: None,
                    content: "implemented adapter".to_string(),
                },
            ))
            .unwrap(),
            serde_json::to_string(&RuntimeEvent::new(
                2,
                RuntimeEventKind::EvidenceRecorded {
                    evidence: EvidenceView {
                        id: "acp-turn-session_1".to_string(),
                        kind: "acp_turn_end".to_string(),
                        summary: "ACP turn completed".to_string(),
                        path: Some(log_path.display().to_string()),
                        source: Some("acp".to_string()),
                        canonical: None,
                        metadata: None,
                        timestamp: None,
                        owner: None,
                    },
                },
            ))
            .unwrap(),
            serde_json::to_string(&RuntimeEvent::new(
                3,
                RuntimeEventKind::AgentSessionCompleted {
                    session: AgentSessionView {
                        session_id: "acp-1".to_string(),
                        lane_id: "lane-acp".to_string(),
                        agent_id: "kiro-acp".to_string(),
                        model: None,
                        status: AgentSessionStatus::Completed,
                        owner: RuntimeOwner {
                            lane_id: Some("lane-acp".to_string()),
                            session_id: Some("acp-1".to_string()),
                            ..RuntimeOwner::default()
                        },
                        task: "implement adapter".to_string(),
                        diagnostic: None,
                        output: Some("implemented adapter".to_string()),
                    },
                },
            ))
            .unwrap(),
        ]
        .join("\n"),
    )
    .unwrap();
    fs::write(
        agent_dir.join("codex-jobs.jsonl"),
        format!(
            "{{\"ts\":1783330000000,\"event\":\"completed\",\"id\":\"acp-1\",\"kind\":\"acp-session\",\"status\":\"finished\",\"pid\":null,\"command\":\"kiro-cli acp\",\"task\":\"implement adapter\",\"log\":\"{}\",\"result\":\"{}\",\"baseline\":\"{}\"}}\n",
            log_path.display(),
            result_path.display(),
            baseline_path.display()
        ),
    )
    .unwrap();
    let provider = Box::new(SequenceProvider::new(vec![]));
    let engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();

    let view = engine.runtime_view_state();

    let task = view
        .tasks
        .iter()
        .find(|task| task.id == "acp-1")
        .expect("tracked ACP session job should become a runtime task");
    assert_eq!(task.role, AgentRole::Coder);
    assert_eq!(task.kind, AgentTaskKind::Job);
    assert_eq!(task.route, AgentRoute::Acp);
    assert_eq!(task.status, AgentTaskStatus::Done);
    assert_eq!(task.result, Some(result_path.display().to_string()));
    assert!(task.evidence.contains(&"session session_1".to_string()));
    assert_eq!(
        task.next_action
            .as_ref()
            .and_then(|action| action.command.as_deref()),
        Some("/agent result acp-1")
    );
    // Replaying a settled session must not leave its reply in the unscoped
    // stream: startup replays every tracked job, so retaining the text there
    // concatenated every historical session into one unattributed blob. The
    // reply is asserted below on the owner-scoped conversation instead.
    assert!(
        view.assistant_stream.is_empty(),
        "a replayed settled session must leave the unscoped stream empty, got {:?}",
        view.assistant_stream
    );
    assert!(view.latest_evidence.iter().any(|evidence| {
        evidence.kind == "acp_turn_end" && evidence.summary.contains("completed")
    }));
    assert_eq!(
        view.agent_conversation
            .iter()
            .map(|message| (message.role, message.content.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (AgentConversationRole::User, "implement adapter"),
            (AgentConversationRole::Assistant, "implemented adapter"),
        ]
    );
}

#[test]
fn transcript_page_runtime_command_emits_transient_page_loaded_event() {
    let home = temp_dir("transcript_page_runtime_home");
    let cwd = temp_dir("transcript_page_runtime_cwd");
    let provider = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let session_id = engine.session_id().to_string();
    let store = SessionStore::new_with_home(&home, &cwd, Some(session_id.clone())).unwrap();
    store
        .append_entry(&TranscriptEntry::Message {
            message: viden_types::Message {
                id: "msg-runtime-page".to_string(),
                role: viden_types::Role::User,
                content: "page me".to_string(),
                timestamp: 1,
                tool_name: None,
                tool_call_id: None,
            },
        })
        .unwrap();

    let events = engine
        .handle_runtime_command(
            "cmd_page",
            RuntimeCommand::LoadTranscriptPage {
                request: TranscriptPageRequest {
                    session_id: session_id.clone(),
                    before: None,
                    limit: 20,
                },
            },
            &mut |_| ApprovalResponse::deny(None),
        )
        .unwrap();
    let loaded_events = events
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::TranscriptPageLoaded { page } => Some(page),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(loaded_events.len(), 1);
    let loaded = loaded_events[0];
    assert!(loaded.rows.iter().any(|row| {
        matches!(
            &row.kind,
            viden_types::TranscriptRowKind::Message { message }
                if message.id == "msg-runtime-page"
        )
    }));

    let persisted = store.load_entries().unwrap();
    assert!(!persisted.iter().any(|entry| matches!(
        entry,
        TranscriptEntry::RuntimeEvent { event }
            if matches!(event.kind, RuntimeEventKind::TranscriptPageLoaded { .. })
    )));
}
