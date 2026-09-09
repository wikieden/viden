#[cfg(test)]
use std::sync::{Arc, Mutex, OnceLock};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};
#[cfg(unix)]
use std::{
    ffi::CString,
    os::unix::{
        ffi::OsStrExt,
        fs::PermissionsExt,
        io::{AsRawFd, FromRawFd},
    },
};

use sha2::{Digest, Sha256};

use crate::context_bundle::{ContextBuildMode, redact_context_summary_for_event};
use crate::lsp_tools::render_lsp_diagnostics;
use crate::{CostAttribution, EngineEvent, ProviderTelemetry, SessionEngine};
use viden_agents::{
    probe_typed_agent_adapter, tracked_agent_job_runtime_events, tracked_agent_job_sessions,
    tracked_agent_job_tasks, typed_agent_adapter_views,
};
use viden_config::ProviderConfigUpdate;
use viden_context::{
    ContextEngine, ContextError as EngineContextError, ReductionPolicy, reduce,
    store::ContextError as StoreContextError,
};
use viden_lsp::SemanticProvider;
use viden_permissions::{PermissionContext, PermissionEngine};
use viden_provider::ModelRequestControl;
use viden_tools::{
    context_read_tool_spec,
    patch::{LocalPatchBackend, PatchApplication, PatchRequest},
};
use viden_types::{
    AgentDagRecord, AgentDagStatus, AgentDagTaskSpec, AgentNextAction, AgentRole, AgentRoute,
    AgentTaskKind, AgentTaskRecord, AgentTaskStatus, ApprovalDecision, ApprovalDefaultAction,
    ApprovalRequestView, ApprovalResponse, ApprovalRisk, ApprovalScope, ApprovalTarget,
    AuditObjectRef, CanonicalEvidenceReference, ContextContentKind, ContextHandleRecord,
    ContextItemRecord, ContextRetrievalRecord, ContextScope, ContextSourceRecord, CostUsageOutcome,
    CostUsageRecord, EvidenceCanonicalReasonCode, EvidenceCanonicalStatus,
    EvidenceCanonicalStatusReport, EvidenceQualityStatus, EvidenceVerificationState, EvidenceView,
    MergeGateDecision, MergeGateDecisionOutcome, MergeGatePolicySnapshot, MergeGateRecord,
    MergeGateStatus, MergeGateType, OperatorGitAction, PermissionBehavior, PermissionDecision,
    PermissionDecisionReason, PermissionLevel, PermissionMode, PermissionPrompt, PermissionRule,
    PermissionRuleSource, PermissionRuleValue, ProviderHealthView, QueuedInputView,
    ReviewRequestStatus, RuntimeCommand, RuntimeErrorView, RuntimeEvent, RuntimeEventKind,
    RuntimeOwner, RuntimeSnapshot, RuntimeViewState, TokenCostView, TokenUsage, ToolCallId,
    ToolInput, TranscriptPageRequest, WorkMode, canonical_evidence_status, fresh_id, now_timestamp,
    truncate_for_preview,
};
use viden_workflows::{
    recovery::{LoadedRecoverySnapshot, RecoverySnapshotEntry},
    stores::WorkflowAgentEvent,
};

/// Character bound for a commit message on the accepted-command event.
///
/// The accepted event describes the command; the executed action still carries
/// the operator's full message under the 4 KiB contract bound.
const MAX_COMMAND_EVENT_MESSAGE_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QueuedRuntimeInput {
    id: String,
    content: String,
    created_at: u64,
}

struct RecordAgentEvidenceRequest<'a> {
    gate_id: &'a str,
    evidence_id: Option<String>,
    kind: String,
    summary: String,
    path: Option<String>,
    source: Option<String>,
    canonical: Option<CanonicalEvidenceReference>,
}

#[derive(Debug, Clone)]
struct RuntimeDomainSnapshot {
    runtime_snapshot: RuntimeSnapshot,
    messages: Vec<viden_types::Message>,
    last_diff: Option<String>,
    last_test: Option<crate::TestEvidence>,
    tasks: Vec<AgentTaskRecord>,
    agent_dags: Vec<AgentDagRecord>,
    merge_gates: Vec<MergeGateRecord>,
    evidence: Vec<EvidenceView>,
    handoffs: Vec<viden_types::HandoffRecord>,
    review_requests: Vec<viden_types::ReviewRequestRecord>,
    contracts: Vec<viden_types::ContractRecord>,
    dependencies: Vec<viden_types::DependencyRecord>,
    conflict_bounces: Vec<viden_types::ConflictBounce>,
    reverts: Vec<viden_types::RevertRecord>,
    applied_change_rollbacks: std::collections::BTreeMap<String, Vec<crate::FileRollback>>,
    pending_project_previews:
        std::collections::BTreeMap<String, crate::project_runtime::PendingProjectConfig>,
    confirmed_project_config: Option<viden_types::ProjectConfigPreview>,
    credential_handles: Vec<viden_types::CredentialHandle>,
    provider_telemetry: ProviderTelemetry,
    provider_cost_usage: Vec<CostUsageRecord>,
    last_context_bundle: Option<viden_types::ContextBundleRecord>,
    last_context_runtime_events: Vec<RuntimeEvent>,
}

#[derive(Debug, Clone)]
struct RuntimePermissionScope {
    work_mode: WorkMode,
    permission_mode: PermissionMode,
    permission_level: PermissionLevel,
    permission_context: PermissionContext,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedContextRetrieval {
    pub(crate) pre_events: Vec<RuntimeEvent>,
    pub(crate) job: ContextRetrievalJob,
}

#[derive(Debug, Clone)]
pub(crate) struct ContextRetrievalJob {
    pub(crate) retrieval_id: String,
    pub(crate) handle: ContextHandleRecord,
    pub(crate) item: ContextItemRecord,
    pub(crate) scope: ContextScope,
    pub(crate) root: PathBuf,
    pub(crate) reason: String,
    pub(crate) reason_category: String,
    pub(crate) permission_decision: String,
    pub(crate) reason_rule_category: String,
    pub(crate) cost_attribution: CostAttribution,
}

#[derive(Debug, Clone)]
// The pending-approval branch carries the full frontend approval contract plus
// the resumable context job so the supervisor can emit auditable facts without
// re-reading mutable state before approval completes.
#[allow(clippy::large_enum_variant)]
pub(crate) enum SupervisorContextRetrievalPreparation {
    Ready(PreparedContextRetrieval),
    PendingApproval {
        approval: ApprovalRequestView,
        job: ContextRetrievalJob,
    },
}

#[cfg(test)]
type RetrieveContextTestHook = Arc<dyn Fn(&ModelRequestControl) + Send + Sync>;

#[cfg(test)]
static RETRIEVE_CONTEXT_TEST_HOOK: OnceLock<Mutex<Option<RetrieveContextTestHook>>> =
    OnceLock::new();

#[cfg(test)]
static RETRIEVE_CONTEXT_PUBLISH_TEST_HOOK: OnceLock<Mutex<Option<RetrieveContextTestHook>>> =
    OnceLock::new();

impl QueuedRuntimeInput {
    fn new(content: String) -> Self {
        Self {
            id: fresh_id("queued"),
            content,
            created_at: now_timestamp(),
        }
    }

    fn view(&self) -> QueuedInputView {
        QueuedInputView {
            id: self.id.clone(),
            content_preview: truncate_for_preview(&self.content, 500),
            created_at: Some(self.created_at),
            // A follow-up queued on the built-in engine belongs to no Lane and
            // no Agent session; Core has no owner to publish for it. Lane input
            // is queued by the Lane worker, which does attach its exact owner.
            owner: None,
        }
    }
}

impl SessionEngine {
    pub fn runtime_snapshot(&self) -> RuntimeSnapshot {
        self.runtime_snapshot.clone()
    }

    pub fn runtime_view_state(&self) -> RuntimeViewState {
        let mut view = RuntimeViewState::new(self.runtime_snapshot());
        for event in self.runtime_state_events() {
            view.apply_event(&event);
        }
        view
    }

    pub fn load_transcript_page(
        &self,
        request: &TranscriptPageRequest,
    ) -> Result<viden_types::TranscriptPage, String> {
        self.store.load_transcript_page(request)
    }

    pub fn runtime_events_for_engine_events(&self, events: &[EngineEvent]) -> Vec<RuntimeEvent> {
        Self::runtime_events_for_engine_output(self.runtime_state_events(), events, &[], &[], None)
    }

    fn runtime_events_for_turn_output(
        &self,
        output: &crate::EngineTurnOutput,
    ) -> Vec<RuntimeEvent> {
        Self::runtime_events_for_engine_output(
            self.runtime_state_events(),
            &output.engine_events,
            &output.ordered_runtime_facts,
            &output.ordered_approval_boundaries,
            None,
        )
    }

    fn runtime_events_for_engine_output(
        mut out: Vec<RuntimeEvent>,
        events: &[EngineEvent],
        ordered_runtime_facts: &[crate::OrderedRuntimeFact],
        ordered_approval_boundaries: &[crate::OrderedApprovalBoundary],
        identity_sequence_base: Option<u64>,
    ) -> Vec<RuntimeEvent> {
        let mut last_tool: Option<(ToolCallId, String)> = None;

        for (engine_event_index, event) in events.iter().enumerate() {
            append_ordered_approval_boundaries(
                &mut out,
                ordered_approval_boundaries,
                engine_event_index,
            );
            let sequence = next_sequence(&out);
            let identity_sequence = identity_sequence_base
                .map(|base| {
                    base.saturating_add(engine_event_index as u64)
                        .saturating_add(1)
                })
                .unwrap_or(sequence);
            match event {
                EngineEvent::System(text) => {
                    out.push(RuntimeEvent::new(
                        sequence,
                        RuntimeEventKind::EvidenceRecorded {
                            evidence: EvidenceView {
                                id: format!("system-{identity_sequence}"),
                                kind: "system".to_string(),
                                summary: truncate_for_preview(text, 500),
                                path: None,
                                source: Some("engine".to_string()),
                                canonical: None,
                                metadata: None,
                                timestamp: None,
                                // This projection is a pure function of engine
                                // events; it holds no owner identity at all.
                                owner: None,
                            },
                        },
                    ));
                }
                EngineEvent::Assistant(content) => {
                    out.push(RuntimeEvent::new(
                        sequence,
                        RuntimeEventKind::AssistantDelta {
                            message_id: format!("assistant-{identity_sequence}"),
                            task_id: None,
                            // The built-in engine has no Agent session; its
                            // deltas stay in the unscoped assistant stream.
                            session_id: None,
                            content: content.clone(),
                        },
                    ));
                }
                EngineEvent::ToolCall(text) => {
                    let (name, input_preview) = parse_legacy_tool_call(text);
                    let tool_call_id = format!("tool-event-{identity_sequence}");
                    last_tool = Some((tool_call_id.clone(), name.clone()));
                    out.push(RuntimeEvent::new(
                        sequence,
                        RuntimeEventKind::ToolCallStarted {
                            tool_call_id,
                            name,
                            input_preview,
                            // See the `System` arm: this projection carries no
                            // owner identity.
                            owner: None,
                        },
                    ));
                }
                EngineEvent::ToolResult {
                    output,
                    success,
                    exit_code,
                } => {
                    let (tool_call_id, name) = last_tool.take().unwrap_or_else(|| {
                        (
                            format!("tool-event-{identity_sequence}"),
                            "tool".to_string(),
                        )
                    });
                    out.push(RuntimeEvent::new(
                        sequence,
                        RuntimeEventKind::ToolCallFinished {
                            tool_call_id,
                            name,
                            success: *success,
                            exit_code: *exit_code,
                            evidence: Some(EvidenceView {
                                id: format!("tool-result-{identity_sequence}"),
                                kind: "tool_result".to_string(),
                                summary: truncate_for_preview(output, 500),
                                path: None,
                                source: Some("engine".to_string()),
                                canonical: None,
                                metadata: None,
                                timestamp: None,
                                owner: None,
                            }),
                        },
                    ));
                }
                EngineEvent::Command(text) => {
                    out.push(RuntimeEvent::new(
                        sequence,
                        RuntimeEventKind::EvidenceRecorded {
                            evidence: EvidenceView {
                                id: format!("command-{identity_sequence}"),
                                kind: "command".to_string(),
                                summary: truncate_for_preview(text, 500),
                                path: None,
                                source: Some("engine".to_string()),
                                canonical: None,
                                metadata: None,
                                timestamp: None,
                                owner: None,
                            },
                        },
                    ));
                }
            }
            for fact in ordered_runtime_facts
                .iter()
                .filter(|fact| fact.after_engine_event_index == engine_event_index)
            {
                let mut fact = fact.event.clone();
                fact.sequence = next_sequence(&out);
                out.push(fact);
            }
        }
        append_ordered_approval_boundaries(&mut out, ordered_approval_boundaries, events.len());

        out
    }

    fn runtime_events_for_streaming_output(
        &self,
        output: &crate::EngineTurnOutput,
        identity_sequence_base: u64,
        include_runtime_state: bool,
    ) -> Vec<RuntimeEvent> {
        Self::runtime_events_for_engine_output(
            if include_runtime_state {
                self.runtime_state_events()
            } else {
                Vec::new()
            },
            &output.engine_events,
            &output.ordered_runtime_facts,
            &output.ordered_approval_boundaries,
            Some(identity_sequence_base),
        )
    }

    pub fn handle_runtime_command<F>(
        &mut self,
        command_id: impl Into<String>,
        command: RuntimeCommand,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(PermissionPrompt) -> ApprovalResponse,
    {
        let command_id = command_id.into();
        if let RuntimeCommand::RetrieveContext { handle_id, reason } = &command {
            let prepared = match self.prepare_context_retrieval(handle_id, reason, approver) {
                Ok(prepared) => prepared,
                Err(err) => return Ok(vec![command_rejected(command_id, err)]),
            };
            let mut events = vec![RuntimeEvent::new(
                1,
                RuntimeEventKind::CommandAccepted {
                    command_id: command_id.clone(),
                    command: redacted_runtime_command_for_event(&command),
                },
            )];
            append_resequenced(&mut events, prepared.pre_events);
            match execute_context_retrieval_job(prepared.job, &ModelRequestControl::new()) {
                Ok(retrieval_events) => append_resequenced(&mut events, retrieval_events),
                Err(err) => {
                    events.push(RuntimeEvent::new(
                        next_sequence(&events),
                        RuntimeEventKind::CommandRejected {
                            command_id: command_id.clone(),
                            reason: err,
                        },
                    ));
                }
            }
            self.persist_cost_usage_events(&events)?;
            self.persist_runtime_domain_events(&events)?;
            return Ok(events);
        }
        if let Err(reason) = self.validate_trust_command(&command) {
            return Ok(vec![command_rejected(command_id, reason)]);
        }

        let persist_after_match = true;
        // Merge gate decisions are valid only when both workflow and session facts
        // append successfully; restore live domain state if either boundary fails.
        let transaction_snapshot = transactional_runtime_command(&command)
            .then(|| self.begin_runtime_domain_transaction());
        let accepted = RuntimeEvent::new(
            1,
            RuntimeEventKind::CommandAccepted {
                command_id: command_id.clone(),
                command: redacted_runtime_command_for_event(&command),
            },
        );

        let mut events = vec![accepted];
        match command {
            RuntimeCommand::QueryAgentAdapters => {
                events.push(RuntimeEvent::new(
                    next_sequence(&events),
                    RuntimeEventKind::AgentAdaptersLoaded {
                        adapters: typed_agent_adapter_views(),
                    },
                ));
            }
            RuntimeCommand::ProbeAgentAdapter { agent_id } => {
                match probe_typed_agent_adapter(&self.cwd, &agent_id) {
                    Ok(adapter) => events.push(RuntimeEvent::new(
                        next_sequence(&events),
                        RuntimeEventKind::AgentAdapterProbed { adapter },
                    )),
                    Err(err) => return Ok(vec![command_rejected(command_id, err)]),
                }
            }
            RuntimeCommand::StartAgentSession { .. }
            | RuntimeCommand::SendAgentSessionInput { .. }
            | RuntimeCommand::RetryAgentSession { .. }
            | RuntimeCommand::CancelAgentSession { .. } => {
                return Ok(vec![command_rejected(
                    command_id,
                    "typed agent session execution must be handled by the runtime supervisor"
                        .to_string(),
                )]);
            }
            RuntimeCommand::ProbeProject => {
                append_resequenced(&mut events, self.project_probe_events());
            }
            RuntimeCommand::PreviewProjectConfig { contents } => {
                match self.preview_project_config_events(contents) {
                    Ok(project_events) => append_resequenced(&mut events, project_events),
                    Err(err) => return Ok(vec![command_rejected(command_id, err)]),
                }
            }
            RuntimeCommand::ConfirmProjectConfig {
                preview_id,
                content_sha256,
            } => match self.confirm_project_config(&preview_id, &content_sha256, approver) {
                Ok(project_events) => append_resequenced(&mut events, project_events),
                Err(err) => {
                    return self.command_rejected_after_transaction_rollback(
                        &transaction_snapshot,
                        command_id,
                        err,
                    );
                }
            },
            RuntimeCommand::StoreCredentialHandle {
                provider_id,
                backend_id,
                credential_request_id,
            } => match self.store_credential_handle(
                &provider_id,
                &backend_id,
                &credential_request_id,
                approver,
            ) {
                Ok(credential_events) => append_resequenced(&mut events, credential_events),
                Err(err) => {
                    return self.command_rejected_after_transaction_rollback(
                        &transaction_snapshot,
                        command_id,
                        err,
                    );
                }
            },
            RuntimeCommand::SetUiPreferences { patch } => {
                match self.set_ui_preferences(&patch, approver) {
                    Ok(preference_events) => append_resequenced(&mut events, preference_events),
                    Err(err) => {
                        return self.command_rejected_after_transaction_rollback(
                            &transaction_snapshot,
                            command_id,
                            err,
                        );
                    }
                }
            }
            RuntimeCommand::ResetUiPreferences => match self.reset_ui_preferences(approver) {
                Ok(preference_events) => append_resequenced(&mut events, preference_events),
                Err(err) => {
                    return self.command_rejected_after_transaction_rollback(
                        &transaction_snapshot,
                        command_id,
                        err,
                    );
                }
            },
            RuntimeCommand::QueryRecentWork { query } => match self.query_recent_work(query) {
                Ok(recent_events) => append_resequenced(&mut events, recent_events),
                Err(err) => return Ok(vec![command_rejected(command_id, err)]),
            },
            // Read-only, exactly like QueryRecentWork: no permission prompt,
            // no plan-mode block, no transaction snapshot.
            RuntimeCommand::QueryAudit { query } => match self.query_audit(&command_id, query) {
                Ok(audit_events) => append_resequenced(&mut events, audit_events),
                Err(err) => return Ok(vec![command_rejected(command_id, err)]),
            },
            // Read-only like the two queries above, but permission-gated: it
            // reads the operator's working tree, so the handler consults the
            // permission engine before touching disk. `Err` covers both a
            // malformed query (an escaping prefix) and a permission refusal,
            // and both come back as `CommandRejected` naming this exact read —
            // an uncorrelated `Error` would let a client with a read
            // outstanding mistake an unrelated failure for its own refusal.
            RuntimeCommand::QueryWorkspaceFiles { query } => {
                match self.query_workspace_files(&command_id, query) {
                    Ok(file_events) => append_resequenced(&mut events, file_events),
                    Err(err) => return Ok(vec![command_rejected(command_id, err)]),
                }
            }
            // The same shape as the inventory read above and for the same
            // reasons: permission-gated because it reads the operator's tree,
            // and every refusal — a path that escapes the target, an unknown
            // Lane, a deny, an unresolved ask — comes back as
            // `CommandRejected` naming this exact read. An empty page here
            // would render as "nothing changed", which is the one thing a
            // diff surface must never say wrongly.
            RuntimeCommand::QueryWorkspaceDiff { query } => {
                match self.query_workspace_diff(&command_id, query) {
                    Ok(diff_events) => append_resequenced(&mut events, diff_events),
                    Err(err) => return Ok(vec![command_rejected(command_id, err)]),
                }
            }
            // Dispatched beside the diff read because they share a target and
            // a resolution path, but this one mutates, so every `Err` below is
            // a *pre-effect* refusal — a malformed action, a path that leaves
            // the target, an unresolvable Lane, plan mode, or a denied
            // permission — and comes back as `CommandRejected` naming this
            // exact command. A failure *after* the gate granted the action is
            // not an error at all: it arrives as an `OperatorGitActionFinished`
            // carrying a `Failed` outcome, because the effect was attempted and
            // the attempt is audited.
            RuntimeCommand::RunOperatorGitAction {
                owner,
                target,
                action,
            } => {
                match self.run_operator_git_action(&command_id, &owner, target, action, approver) {
                    Ok(action_events) => append_resequenced(&mut events, action_events),
                    Err(err) => return Ok(vec![command_rejected(command_id, err)]),
                }
            }
            RuntimeCommand::SubmitUserInput { content } => {
                match self.process_runtime_turn_with_approval_and_control(
                    &content,
                    approver,
                    &ModelRequestControl::new(),
                ) {
                    Ok(input_events) => append_resequenced(&mut events, input_events),
                    Err(failure) if failure.message.contains("context hard limit") => {
                        append_resequenced(&mut events, failure.completed_events);
                        events.push(RuntimeEvent::new(
                            next_sequence(&events),
                            RuntimeEventKind::CommandRejected {
                                command_id,
                                reason: failure.message,
                            },
                        ));
                        return Ok(events);
                    }
                    Err(failure) => {
                        append_resequenced(&mut events, failure.completed_events);
                        events.push(RuntimeEvent::new(
                            next_sequence(&events),
                            RuntimeEventKind::Error {
                                error: RuntimeErrorView {
                                    message: failure.message,
                                    recoverable: true,
                                    hint: Some(
                                        "completed tool facts were preserved before the turn stopped"
                                            .to_string(),
                                    ),
                                },
                            },
                        ));
                        return Ok(events);
                    }
                }
            }
            RuntimeCommand::QueueFollowUp { content } => {
                let queued = QueuedRuntimeInput::new(content);
                let input = queued.view();
                self.queued_runtime_inputs.push(queued);
                events.push(RuntimeEvent::new(
                    next_sequence(&events),
                    RuntimeEventKind::InputQueued { input },
                ));
            }
            RuntimeCommand::SetWorkMode { mode } => {
                self.set_work_mode(mode)?;
                events.push(RuntimeEvent::new(
                    next_sequence(&events),
                    RuntimeEventKind::SnapshotUpdated {
                        snapshot: self.runtime_snapshot(),
                    },
                ));
            }
            RuntimeCommand::SetPermissionLevel { level } => {
                self.set_permission_mode(permission_mode_for_level(level))?;
                events.push(RuntimeEvent::new(
                    next_sequence(&events),
                    RuntimeEventKind::SnapshotUpdated {
                        snapshot: self.runtime_snapshot(),
                    },
                ));
            }
            RuntimeCommand::SelectModel { provider_id, model } => {
                if provider_id != self.provider_name() {
                    return Ok(vec![command_rejected(
                        command_id,
                        format!(
                            "provider `{provider_id}` is not the active provider `{}`",
                            self.provider_name()
                        ),
                    )]);
                }
                self.provider.set_model(model);
                self.runtime_snapshot.model_label = self.provider.model().to_string();
                self.persist_meta("model", self.provider.model())?;
                events.push(RuntimeEvent::new(
                    next_sequence(&events),
                    RuntimeEventKind::SnapshotUpdated {
                        snapshot: self.runtime_snapshot(),
                    },
                ));
            }
            RuntimeCommand::ConfigureProvider {
                provider_id,
                api_key_env,
                endpoint,
                default_model,
            } => match self.save_provider_config_update(
                &provider_id,
                ProviderConfigUpdate {
                    api_base: endpoint.clone(),
                    api_key_env: api_key_env.clone(),
                    default_model: default_model.clone(),
                    ..ProviderConfigUpdate::default()
                },
                provider_config_summary(&api_key_env, &endpoint, &default_model),
            ) {
                Ok(output) => push_evidence_event(&mut events, "provider_config", output),
                Err(err) => return Ok(vec![command_rejected(command_id, err)]),
            },
            RuntimeCommand::ActivateModel { provider_id, model } => {
                match self.add_provider_model(&provider_id, &model) {
                    Ok(output) => push_evidence_event(&mut events, "provider_model", output),
                    Err(err) => return Ok(vec![command_rejected(command_id, err)]),
                }
            }
            RuntimeCommand::DeactivateModel { provider_id, model } => {
                match self.remove_provider_model(&provider_id, &model) {
                    Ok(output) => push_evidence_event(&mut events, "provider_model", output),
                    Err(err) => return Ok(vec![command_rejected(command_id, err)]),
                }
            }
            RuntimeCommand::StartAgentDag { goal, tasks } => {
                match self.start_agent_dag(goal, tasks) {
                    Ok(dag_events) => append_resequenced(&mut events, dag_events),
                    Err(err) => {
                        return self.command_rejected_after_transaction_rollback(
                            &transaction_snapshot,
                            command_id,
                            err,
                        );
                    }
                }
            }
            RuntimeCommand::StartAgentTask { task_id } => {
                match self.run_agent_task(&task_id, approver) {
                    Ok(task_events) => append_resequenced(&mut events, task_events),
                    Err(err) => {
                        return self.command_rejected_after_transaction_rollback(
                            &transaction_snapshot,
                            command_id,
                            err,
                        );
                    }
                }
            }
            RuntimeCommand::CancelAgentTask { task_id } => match self.cancel_agent_task(&task_id) {
                Ok(cancel_events) => append_resequenced(&mut events, cancel_events),
                Err(err) => {
                    return self.command_rejected_after_transaction_rollback(
                        &transaction_snapshot,
                        command_id,
                        err,
                    );
                }
            },
            RuntimeCommand::CreateHandoff {
                handoff_id,
                task_id,
                from_lane_id,
                to_lane_id,
                owner,
                summary,
                acceptance,
            } => match self.create_handoff(
                handoff_id,
                task_id,
                from_lane_id,
                to_lane_id,
                owner,
                summary,
                acceptance,
                approver,
            ) {
                Ok(trust_events) => append_resequenced(&mut events, trust_events),
                Err(err) => {
                    return self.command_rejected_after_transaction_rollback(
                        &transaction_snapshot,
                        command_id,
                        err,
                    );
                }
            },
            RuntimeCommand::RequestReview {
                review_id,
                gate_id,
                requester_lane_id,
                reviewer_lane_id,
                owner,
                evidence_ids,
            } => match self.request_review(
                review_id,
                gate_id,
                requester_lane_id,
                reviewer_lane_id,
                owner,
                evidence_ids,
                approver,
            ) {
                Ok(trust_events) => append_resequenced(&mut events, trust_events),
                Err(err) => {
                    return self.command_rejected_after_transaction_rollback(
                        &transaction_snapshot,
                        command_id,
                        err,
                    );
                }
            },
            RuntimeCommand::DecideReview {
                review_id,
                verdict,
                feedback,
                actor,
            } => match self.decide_review(review_id, verdict, feedback, actor, approver) {
                Ok(trust_events) => append_resequenced(&mut events, trust_events),
                Err(err) => {
                    return self.command_rejected_after_transaction_rollback(
                        &transaction_snapshot,
                        command_id,
                        err,
                    );
                }
            },
            RuntimeCommand::ConfirmContract {
                contract_id,
                task_id,
                owner,
                summary,
                decision,
            } => match self.confirm_contract(
                contract_id,
                task_id,
                owner,
                summary,
                decision,
                approver,
            ) {
                Ok(trust_events) => append_resequenced(&mut events, trust_events),
                Err(err) => {
                    return self.command_rejected_after_transaction_rollback(
                        &transaction_snapshot,
                        command_id,
                        err,
                    );
                }
            },
            RuntimeCommand::SetDependency {
                dependency_id,
                task_id,
                depends_on_task_id,
                owner,
                state,
                reason,
            } => match self.set_dependency(
                dependency_id,
                task_id,
                depends_on_task_id,
                owner,
                state,
                reason,
                approver,
            ) {
                Ok(trust_events) => append_resequenced(&mut events, trust_events),
                Err(err) => {
                    return self.command_rejected_after_transaction_rollback(
                        &transaction_snapshot,
                        command_id,
                        err,
                    );
                }
            },
            RuntimeCommand::AcceptMergeGate {
                gate_id,
                actor,
                reviewed_evidence,
                decision,
            } => {
                match self.decide_merge_gate(
                    &gate_id,
                    MergeGateStatus::Accepted,
                    Some(actor),
                    reviewed_evidence,
                    decision.unwrap_or_else(|| "accepted".to_string()),
                    approver,
                ) {
                    Ok(decision_events) => append_resequenced(&mut events, decision_events),
                    Err(err) => {
                        return self.command_rejected_after_transaction_rollback(
                            &transaction_snapshot,
                            command_id,
                            err,
                        );
                    }
                }
            }
            RuntimeCommand::RejectMergeGate {
                gate_id,
                actor,
                reason,
            } => {
                match self.decide_merge_gate(
                    &gate_id,
                    MergeGateStatus::NeedsChanges,
                    Some(actor),
                    Vec::new(),
                    reason,
                    approver,
                ) {
                    Ok(decision_events) => append_resequenced(&mut events, decision_events),
                    Err(err) => {
                        return self.command_rejected_after_transaction_rollback(
                            &transaction_snapshot,
                            command_id,
                            err,
                        );
                    }
                }
            }
            RuntimeCommand::RecordAgentEvidence {
                gate_id,
                evidence_id,
                kind,
                summary,
                path,
                source,
                canonical,
            } => {
                let record_result = self.record_agent_evidence(
                    RecordAgentEvidenceRequest {
                        gate_id: &gate_id,
                        evidence_id,
                        kind,
                        summary,
                        path,
                        source,
                        canonical,
                    },
                    approver,
                );
                match record_result {
                    Ok(evidence_events) => append_resequenced(&mut events, evidence_events),
                    Err(err) => {
                        return self.command_rejected_after_transaction_rollback(
                            &transaction_snapshot,
                            command_id,
                            err,
                        );
                    }
                }
            }
            RuntimeCommand::AcceptAgentArtifact {
                gate_id,
                evidence_id,
                actor,
                source_hash,
                decision,
            } => {
                match self.accept_agent_artifact(
                    &gate_id,
                    evidence_id,
                    actor,
                    source_hash,
                    decision.unwrap_or_else(|| "artifact accepted".to_string()),
                    approver,
                ) {
                    Ok(artifact_events) => append_resequenced(&mut events, artifact_events),
                    Err(err) => {
                        return self.command_rejected_after_transaction_rollback(
                            &transaction_snapshot,
                            command_id,
                            err,
                        );
                    }
                }
            }
            RuntimeCommand::RejectAgentArtifact {
                gate_id,
                evidence_id,
                actor,
                reason,
            } => {
                match self.reject_agent_artifact(&gate_id, &evidence_id, actor, reason, approver) {
                    Ok(artifact_events) => append_resequenced(&mut events, artifact_events),
                    Err(err) => {
                        return self.command_rejected_after_transaction_rollback(
                            &transaction_snapshot,
                            command_id,
                            err,
                        );
                    }
                }
            }
            RuntimeCommand::MergeAgentPatch {
                gate_id,
                actor,
                decision,
            } => {
                match self.merge_agent_patch(
                    &gate_id,
                    actor,
                    decision.unwrap_or_else(|| "patch merged".to_string()),
                    approver,
                ) {
                    Ok(merge_events) => append_resequenced(&mut events, merge_events),
                    Err(err) => {
                        return self.command_rejected_after_transaction_rollback(
                            &transaction_snapshot,
                            command_id,
                            err,
                        );
                    }
                }
            }
            RuntimeCommand::RevalidateMergeConflict {
                gate_id,
                bounce_id,
                actor,
                evidence,
            } => {
                match self.revalidate_merge_conflict(gate_id, bounce_id, actor, evidence, approver)
                {
                    Ok(trust_events) => append_resequenced(&mut events, trust_events),
                    Err(err) => {
                        return self.command_rejected_after_transaction_rollback(
                            &transaction_snapshot,
                            command_id,
                            err,
                        );
                    }
                }
            }
            RuntimeCommand::BounceMergeConflict {
                gate_id,
                original_lane_id,
                owner,
                reason,
            } => {
                match self.bounce_merge_conflict(gate_id, original_lane_id, owner, reason, approver)
                {
                    Ok(trust_events) => append_resequenced(&mut events, trust_events),
                    Err(err) => {
                        return self.command_rejected_after_transaction_rollback(
                            &transaction_snapshot,
                            command_id,
                            err,
                        );
                    }
                }
            }
            RuntimeCommand::RevertAppliedChange {
                gate_id,
                owner,
                reason,
            } => match self.revert_applied_change(gate_id, owner, reason, approver) {
                Ok(trust_events) => append_resequenced(&mut events, trust_events),
                Err(err) => {
                    return self.command_rejected_after_transaction_rollback(
                        &transaction_snapshot,
                        command_id,
                        err,
                    );
                }
            },
            RuntimeCommand::LoadTranscriptPage { request } => {
                match self.store.load_transcript_page(&request) {
                    Ok(page) => events.push(RuntimeEvent::new(
                        next_sequence(&events),
                        RuntimeEventKind::TranscriptPageLoaded {
                            page: Box::new(page),
                        },
                    )),
                    Err(err) => return Ok(vec![command_rejected(command_id, err)]),
                }
            }
            RuntimeCommand::PreviewStarterLane { .. }
            | RuntimeCommand::PreviewDefaultStarterLane { .. }
            | RuntimeCommand::CreateStarterLane { .. }
            | RuntimeCommand::CreateLane { .. }
            | RuntimeCommand::StartLane { .. }
            | RuntimeCommand::StopLane { .. }
            | RuntimeCommand::AttachLane { .. }
            | RuntimeCommand::DetachLane { .. }
            | RuntimeCommand::SendLaneInput { .. }
            | RuntimeCommand::AcceptLaneOutput { .. }
            | RuntimeCommand::ReviseLaneOutput { .. }
            | RuntimeCommand::DiscardLaneOutput { .. }
            | RuntimeCommand::ApplyLaneChanges { .. }
            | RuntimeCommand::ResolveLaneConflict { .. }
            | RuntimeCommand::ArchiveLane { .. }
            | RuntimeCommand::CleanupLane { .. } => {
                return Ok(vec![command_rejected(
                    command_id,
                    "lane lifecycle commands must be routed through RuntimeSupervisor".to_string(),
                )]);
            }
            RuntimeCommand::RetrieveContext { .. } => unreachable!("handled before acceptance"),
            RuntimeCommand::CancelActiveTurn | RuntimeCommand::RespondToApproval { .. } => {
                return Ok(vec![command_rejected(
                    command_id,
                    "runtime command is declared but not implemented in core yet".to_string(),
                )]);
            }
        }

        if persist_after_match && let Err(err) = self.persist_runtime_domain_events(&events) {
            self.restore_transaction_snapshot(&transaction_snapshot)
                .map_err(|rollback| format!("{err}; rollback failed: {rollback}"))?;
            let mut recovery = vec![command_rejected(command_id, err.clone())];
            recovery.push(RuntimeEvent::new(
                2,
                RuntimeEventKind::Error {
                    error: RuntimeErrorView {
                        message: format!("runtime transaction failed before durable commit: {err}"),
                        recoverable: true,
                        hint: Some(
                            "in-memory facts and staged file bytes were restored; retry the command after checking workflow storage"
                                .to_string(),
                        ),
                    },
                },
            ));
            return Ok(recovery);
        }
        self.commit_transaction_snapshot(&transaction_snapshot);
        Ok(events)
    }

    fn begin_runtime_domain_transaction(&mut self) -> RuntimeDomainSnapshot {
        // Project facts are owned by the workflow log. This snapshot only
        // compensates live projections and staged file writes if workflow
        // append fails before the command is durably owned.
        self.transaction_file_rollback.borrow_mut().clear();
        RuntimeDomainSnapshot {
            runtime_snapshot: self.runtime_snapshot.clone(),
            messages: self.messages.clone(),
            last_diff: self.last_diff.clone(),
            last_test: self.last_test.clone(),
            tasks: self.runtime_tasks.clone(),
            agent_dags: self.runtime_agent_dags.clone(),
            merge_gates: self.runtime_merge_gates.clone(),
            evidence: self.runtime_evidence.clone(),
            handoffs: self.runtime_handoffs.clone(),
            review_requests: self.runtime_review_requests.clone(),
            contracts: self.runtime_contracts.clone(),
            dependencies: self.runtime_dependencies.clone(),
            conflict_bounces: self.runtime_conflict_bounces.clone(),
            reverts: self.runtime_reverts.clone(),
            applied_change_rollbacks: self.applied_change_rollbacks.clone(),
            pending_project_previews: self.pending_project_previews.clone(),
            confirmed_project_config: self.confirmed_project_config.clone(),
            credential_handles: self.credential_handles.clone(),
            provider_telemetry: self.provider_telemetry.clone(),
            provider_cost_usage: self.provider_cost_usage.clone(),
            last_context_bundle: self.last_context_bundle.clone(),
            last_context_runtime_events: self.last_context_runtime_events.clone(),
        }
    }

    fn restore_runtime_domain_snapshot(
        &mut self,
        snapshot: RuntimeDomainSnapshot,
    ) -> Result<(), String> {
        self.runtime_snapshot = snapshot.runtime_snapshot;
        self.messages = snapshot.messages;
        self.last_diff = snapshot.last_diff;
        self.last_test = snapshot.last_test;
        self.runtime_tasks = snapshot.tasks;
        self.runtime_agent_dags = snapshot.agent_dags;
        self.runtime_merge_gates = snapshot.merge_gates;
        self.runtime_evidence = snapshot.evidence;
        self.runtime_handoffs = snapshot.handoffs;
        self.runtime_review_requests = snapshot.review_requests;
        self.runtime_contracts = snapshot.contracts;
        self.runtime_dependencies = snapshot.dependencies;
        self.runtime_conflict_bounces = snapshot.conflict_bounces;
        self.runtime_reverts = snapshot.reverts;
        self.applied_change_rollbacks = snapshot.applied_change_rollbacks;
        self.pending_project_previews = snapshot.pending_project_previews;
        self.confirmed_project_config = snapshot.confirmed_project_config;
        self.credential_handles = snapshot.credential_handles;
        self.provider_telemetry = snapshot.provider_telemetry;
        self.provider_cost_usage = snapshot.provider_cost_usage;
        self.last_context_bundle = snapshot.last_context_bundle;
        self.last_context_runtime_events = snapshot.last_context_runtime_events;
        self.restore_transaction_files()?;
        self.transaction_file_rollback.borrow_mut().clear();
        Ok(())
    }

    fn restore_transaction_snapshot(
        &mut self,
        snapshot: &Option<RuntimeDomainSnapshot>,
    ) -> Result<(), String> {
        if let Some(snapshot) = snapshot {
            self.restore_runtime_domain_snapshot(snapshot.clone())?;
        }
        Ok(())
    }

    fn commit_transaction_snapshot(&mut self, snapshot: &Option<RuntimeDomainSnapshot>) {
        if snapshot.is_some() {
            self.transaction_file_rollback.borrow_mut().clear();
        }
    }

    fn command_rejected_after_transaction_rollback(
        &mut self,
        snapshot: &Option<RuntimeDomainSnapshot>,
        command_id: String,
        err: String,
    ) -> Result<Vec<RuntimeEvent>, String> {
        self.restore_transaction_snapshot(snapshot)
            .map_err(|rollback| format!("{err}; rollback failed: {rollback}"))?;
        let mut events = vec![command_rejected(command_id, err.clone())];
        if snapshot.is_some() {
            events.push(RuntimeEvent::new(
                2,
                RuntimeEventKind::Error {
                    error: RuntimeErrorView {
                        message: format!("runtime transaction did not commit: {err}"),
                        recoverable: true,
                        hint: Some(
                            "transaction facts and any staged file bytes remain unchanged or were restored"
                                .to_string(),
                        ),
                    },
                },
            ));
        }
        Ok(events)
    }

    fn persist_cost_usage_events(&mut self, events: &[RuntimeEvent]) -> Result<(), String> {
        let costs = events
            .iter()
            .filter_map(|event| match &event.kind {
                RuntimeEventKind::CostUsageRecorded { cost } => Some(cost.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for cost in costs {
            self.record_cost_usage(cost)?;
        }
        Ok(())
    }

    pub fn process_runtime_input_with_approval<F>(
        &mut self,
        input: &str,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(PermissionPrompt) -> ApprovalResponse,
    {
        match self.process_runtime_turn_with_approval_and_control(
            input,
            approver,
            &ModelRequestControl::new(),
        ) {
            Ok(events) => Ok(events),
            Err(failure) => {
                let mut events = failure.completed_events;
                events.push(RuntimeEvent::new(
                    next_sequence(&events),
                    RuntimeEventKind::Error {
                        error: RuntimeErrorView {
                            message: failure.message,
                            recoverable: true,
                            hint: Some(
                                "completed tool facts were preserved before the turn stopped"
                                    .to_string(),
                            ),
                        },
                    },
                ));
                Ok(events)
            }
        }
    }

    pub(crate) fn prepare_context_retrieval<F>(
        &mut self,
        handle_id: &str,
        reason: &str,
        approver: &mut F,
    ) -> Result<PreparedContextRetrieval, String>
    where
        F: FnMut(PermissionPrompt) -> ApprovalResponse,
    {
        let (handle, item, expected_scope) = self.resolve_current_context_handle(handle_id)?;
        validate_context_retrieval_scope_and_expiry(&handle, &expected_scope)?;
        let reason_category = context_retrieval_reason_category(reason);
        let bounded_reason = bound_redacted_context_reason(reason);
        let mut permission_input = ToolInput::new();
        permission_input.insert("handle_id".to_string(), handle.handle_id.clone());
        permission_input.insert("reason_category".to_string(), reason_category.clone());
        let tool_spec = context_read_tool_spec();
        let mut permission_decision = "allow".to_string();
        let mut asked_reason_category = None;
        let mut events = Vec::new();
        // Decide -> ask -> apply flows through the shared permission gate; the
        // closure keeps this site's approval runtime events and audit fields.
        let mut gate_permissions = self.permissions.clone();
        let decision = crate::permission_gate::resolve(
            &mut gate_permissions,
            &tool_spec,
            "context_read",
            &permission_input,
            |ask, prompt| {
                asked_reason_category =
                    Some(permission_reason_category(ask.decision_reason.as_ref()));
                let request_id = fresh_id("approval");
                events.push(RuntimeEvent::new(
                    next_sequence(&events),
                    RuntimeEventKind::ApprovalRequested {
                        approval: approval_request_view(&request_id, &prompt),
                    },
                ));
                let approval = approver(prompt);
                let approval_decision = approval.decision.clone();
                permission_decision =
                    if matches!(approval_decision, ApprovalDecision::Allow { .. }) {
                        "approved"
                    } else {
                        "denied"
                    }
                    .to_string();
                events.push(RuntimeEvent::new(
                    next_sequence(&events),
                    RuntimeEventKind::ApprovalResolved {
                        request_id,
                        decision: approval_decision,
                        owner: RuntimeOwner::default(),
                        audit_id: fresh_id("audit"),
                    },
                ));
                approval
            },
        );
        let reason_rule_category = asked_reason_category
            .unwrap_or_else(|| permission_reason_category_from_decision(&decision));
        match decision {
            PermissionDecision::Deny(deny) => return Err(deny.message),
            PermissionDecision::Ask(_) => unreachable!("ask decisions are resolved synchronously"),
            PermissionDecision::Allow(_) => {}
        }

        let retrieval_id = fresh_id("ctxr");
        let usage_id = format!("{retrieval_id}-cost");
        Ok(PreparedContextRetrieval {
            pre_events: events,
            job: ContextRetrievalJob {
                retrieval_id,
                cost_attribution: self
                    .cost_attribution_for_context_scope(&usage_id, &expected_scope),
                handle,
                item,
                scope: expected_scope,
                root: self.context_engine_root.clone(),
                reason: bounded_reason,
                reason_category,
                permission_decision,
                reason_rule_category,
            },
        })
    }

    pub(crate) fn prepare_context_retrieval_for_supervisor(
        &mut self,
        handle_id: &str,
        reason: &str,
    ) -> Result<SupervisorContextRetrievalPreparation, String> {
        let (handle, item, expected_scope) = self.resolve_current_context_handle(handle_id)?;
        validate_context_retrieval_scope_and_expiry(&handle, &expected_scope)?;
        let reason_category = context_retrieval_reason_category(reason);
        let bounded_reason = bound_redacted_context_reason(reason);
        let mut permission_input = ToolInput::new();
        permission_input.insert("handle_id".to_string(), handle.handle_id.clone());
        permission_input.insert("reason_category".to_string(), reason_category.clone());
        let tool_spec = context_read_tool_spec();
        let decision = self.permissions.decide(&tool_spec, &permission_input);
        let retrieval_id = fresh_id("ctxr");
        let usage_id = format!("{retrieval_id}-cost");
        let mut job = ContextRetrievalJob {
            retrieval_id,
            cost_attribution: self.cost_attribution_for_context_scope(&usage_id, &expected_scope),
            handle,
            item,
            scope: expected_scope,
            root: self.context_engine_root.clone(),
            reason: bounded_reason,
            reason_category,
            permission_decision: "allow".to_string(),
            reason_rule_category: permission_reason_category_from_decision(&decision),
        };

        match decision {
            PermissionDecision::Allow(_) => Ok(SupervisorContextRetrievalPreparation::Ready(
                PreparedContextRetrieval {
                    pre_events: Vec::new(),
                    job,
                },
            )),
            PermissionDecision::Deny(deny) => Err(deny.message),
            PermissionDecision::Ask(ask) => {
                job.permission_decision = "pending".to_string();
                job.reason_rule_category = permission_reason_category(ask.decision_reason.as_ref());
                let request_id = fresh_id("approval");
                let prompt = PermissionEngine::prompt_for("context_read", &ask, &permission_input);
                Ok(SupervisorContextRetrievalPreparation::PendingApproval {
                    approval: approval_request_view(&request_id, &prompt),
                    job,
                })
            }
        }
    }

    pub(crate) fn validate_context_retrieval_job_for_supervisor(
        &self,
        job: &ContextRetrievalJob,
    ) -> Result<(), String> {
        let (handle, item, expected_scope) =
            self.resolve_current_context_handle(&job.handle.handle_id)?;
        validate_context_retrieval_scope_and_expiry(&handle, &expected_scope)?;
        if expected_scope != job.scope
            || handle.item_id != job.handle.item_id
            || handle.content_sha256 != job.handle.content_sha256
            || item.item_id != job.item.item_id
        {
            return Err(format!(
                "context handle `{}` changed before approval completed",
                redact_identifier_for_event(&job.handle.handle_id)
            ));
        }
        Ok(())
    }

    fn resolve_current_context_handle(
        &self,
        handle_id: &str,
    ) -> Result<(ContextHandleRecord, ContextItemRecord, ContextScope), String> {
        let Some(context) = &self.last_context_bundle else {
            return Err("no current runtime context bundle is available".to_string());
        };
        if !context
            .sources
            .iter()
            .any(|source| source.handle_id.as_deref() == Some(handle_id))
        {
            return Err(format!(
                "context handle `{}` is not known to the current runtime context",
                redact_identifier_for_event(handle_id)
            ));
        }
        let view = self.runtime_view_state();
        let handle = view
            .context_handles
            .iter()
            .find(|handle| handle.handle_id == handle_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "context handle `{}` is missing from runtime view state",
                    redact_identifier_for_event(handle_id)
                )
            })?;
        let item = view
            .context_items
            .iter()
            .find(|item| item.item_id == handle.item_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "context item for handle `{}` is missing from runtime view state",
                    redact_identifier_for_event(handle_id)
                )
            })?;
        Ok((handle, item, ContextScope::Task(context.task_id.clone())))
    }

    #[cfg(test)]
    pub(crate) fn mutate_context_handle_for_test<F>(&mut self, handle_id: &str, mut mutate: F)
    where
        F: FnMut(&mut ContextHandleRecord),
    {
        for event in &mut self.last_context_runtime_events {
            if let RuntimeEventKind::ContextViewDerived { handle, .. } = &mut event.kind
                && handle.handle_id == handle_id
            {
                mutate(handle);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn remove_context_item_for_test(&mut self, item_id: &str) {
        self.last_context_runtime_events.retain(|event| {
            !matches!(
                &event.kind,
                RuntimeEventKind::ContextItemStored { item } if item.item_id == item_id
            )
        });
    }

    #[cfg(test)]
    pub(crate) fn set_merge_gate_context_facts_for_test(
        &mut self,
        bundle_id: &str,
        item: ContextItemRecord,
    ) {
        let source = ContextSourceRecord {
            name: "canonical evidence".to_string(),
            kind: "test".to_string(),
            priority: 1,
            estimated_tokens: item.token_count,
            summary: item.summary.clone(),
            include_reason: "test canonical evidence".to_string(),
            handle_id: None,
            item_id: Some(item.item_id.clone()),
            view_id: None,
            content_sha256: Some(item.content_sha256.clone()),
            view_sha256: None,
            quality_id: None,
        };
        if let Some(bundle) = &mut self.last_context_bundle {
            bundle.sources.push(source);
            bundle.estimated_tokens = bundle.estimated_tokens.saturating_add(item.token_count);
        } else {
            self.last_context_bundle = Some(viden_types::ContextBundleRecord {
                bundle_id: bundle_id.to_string(),
                task_id: match &item.scope {
                    ContextScope::Task(task_id) => task_id.clone(),
                    ContextScope::Dag(dag_id) | ContextScope::Workflow(dag_id) => dag_id.clone(),
                },
                policy: "test".to_string(),
                sources: vec![source],
                omitted_sources: Vec::new(),
                estimated_tokens: item.token_count,
                largest_sources: Vec::new(),
                compaction_notes: Vec::new(),
                soft_token_budget: 1_000,
                hard_token_limit: 2_000,
            });
        }
        let next = self.last_context_runtime_events.len() as u64 + 1;
        self.last_context_runtime_events.push(RuntimeEvent::new(
            next,
            RuntimeEventKind::ContextBundleBuilt {
                bundle_id: bundle_id.to_string(),
                scope: item.scope.clone(),
                handle_ids: Vec::new(),
                estimated_tokens: item.token_count,
            },
        ));
        self.last_context_runtime_events.push(RuntimeEvent::new(
            next + 1,
            RuntimeEventKind::ContextItemStored { item },
        ));
    }

    pub(crate) fn process_runtime_turn_with_approval_and_control<F>(
        &mut self,
        input: &str,
        approver: &mut F,
        control: &ModelRequestControl,
    ) -> Result<Vec<RuntimeEvent>, crate::RuntimeInputFailure>
    where
        F: FnMut(PermissionPrompt) -> ApprovalResponse,
    {
        match self.process_engine_turn_with_approval_and_control(input, approver, control) {
            Ok(output) => Ok(self.runtime_events_for_turn_output(&output)),
            Err(failure) => Err(crate::RuntimeInputFailure {
                message: failure.message,
                completed_events: self.runtime_events_for_turn_output(&failure.completed),
            }),
        }
    }

    pub(crate) fn process_runtime_turn_streaming_with_approval_and_control<F, E>(
        &mut self,
        input: &str,
        approver: &mut F,
        control: &ModelRequestControl,
        emit_completed: &mut E,
    ) -> Result<Vec<RuntimeEvent>, crate::RuntimeInputFailure>
    where
        F: FnMut(PermissionPrompt) -> ApprovalResponse,
        E: FnMut(Vec<RuntimeEvent>) + ?Sized,
    {
        let mut identity_sequence_base = 0_u64;
        let mut on_approval_boundary = |output: crate::EngineTurnOutput| {
            let event_count = output.engine_events.len() as u64;
            let events = Self::runtime_events_for_engine_output(
                Vec::new(),
                &output.engine_events,
                &output.ordered_runtime_facts,
                &output.ordered_approval_boundaries,
                Some(identity_sequence_base),
            );
            identity_sequence_base = identity_sequence_base.saturating_add(event_count);
            if !events.is_empty() {
                emit_completed(events);
            }
        };
        match self.process_engine_turn_streaming_with_approval_and_control(
            input,
            approver,
            control,
            &mut on_approval_boundary,
        ) {
            Ok(output) => {
                Ok(self.runtime_events_for_streaming_output(&output, identity_sequence_base, true))
            }
            Err(failure) => Err(crate::RuntimeInputFailure {
                message: failure.message,
                completed_events: self.runtime_events_for_streaming_output(
                    &failure.completed,
                    identity_sequence_base,
                    true,
                ),
            }),
        }
    }

    pub(crate) fn process_runtime_turn_with_built_context_bundle_and_control<F>(
        &mut self,
        input: &str,
        approver: &mut F,
        control: &ModelRequestControl,
        built_context_bundle: crate::context_bundle::BuiltContextBundle,
    ) -> Result<Vec<RuntimeEvent>, crate::RuntimeInputFailure>
    where
        F: FnMut(PermissionPrompt) -> ApprovalResponse,
    {
        match self.process_engine_turn_with_built_context_bundle_and_control(
            input,
            approver,
            control,
            built_context_bundle,
        ) {
            Ok(output) => Ok(self.runtime_events_for_turn_output(&output)),
            Err(failure) => Err(crate::RuntimeInputFailure {
                message: failure.message,
                completed_events: self.runtime_events_for_turn_output(&failure.completed),
            }),
        }
    }

    pub(crate) fn process_runtime_turn_streaming_with_built_context_bundle_and_control<F, E>(
        &mut self,
        input: &str,
        approver: &mut F,
        control: &ModelRequestControl,
        built_context_bundle: crate::context_bundle::BuiltContextBundle,
        emit_completed: &mut E,
    ) -> Result<Vec<RuntimeEvent>, crate::RuntimeInputFailure>
    where
        F: FnMut(PermissionPrompt) -> ApprovalResponse,
        E: FnMut(Vec<RuntimeEvent>) + ?Sized,
    {
        let mut identity_sequence_base = 0_u64;
        let mut on_approval_boundary = |output: crate::EngineTurnOutput| {
            let event_count = output.engine_events.len() as u64;
            let events = Self::runtime_events_for_engine_output(
                Vec::new(),
                &output.engine_events,
                &output.ordered_runtime_facts,
                &output.ordered_approval_boundaries,
                Some(identity_sequence_base),
            );
            identity_sequence_base = identity_sequence_base.saturating_add(event_count);
            if !events.is_empty() {
                emit_completed(events);
            }
        };
        match self.process_engine_turn_streaming_with_built_context_bundle_and_control(
            input,
            approver,
            control,
            built_context_bundle,
            &mut on_approval_boundary,
        ) {
            Ok(output) => {
                Ok(self.runtime_events_for_streaming_output(&output, identity_sequence_base, true))
            }
            Err(failure) => Err(crate::RuntimeInputFailure {
                message: failure.message,
                completed_events: self.runtime_events_for_streaming_output(
                    &failure.completed,
                    identity_sequence_base,
                    true,
                ),
            }),
        }
    }

    fn runtime_state_events(&self) -> Vec<RuntimeEvent> {
        let mut events = vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::SnapshotUpdated {
                snapshot: self.runtime_snapshot(),
            },
        )];
        if let Some(preview) = &self.confirmed_project_config {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::ProjectConfigConfirmed {
                    preview: preview.clone(),
                },
            ));
        }
        for handle in &self.credential_handles {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::CredentialHandleStored {
                    handle: handle.clone(),
                },
            ));
        }
        if let Some(context) = &self.last_context_bundle {
            for event in &self.last_context_runtime_events {
                events.push(RuntimeEvent::new(
                    next_sequence(&events),
                    event.kind.clone(),
                ));
            }
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::ContextUpdated {
                    context: context.clone(),
                },
            ));
        }
        events.push(RuntimeEvent::new(
            next_sequence(&events),
            RuntimeEventKind::ProviderHealthUpdated {
                provider: self.project_provider_health(),
            },
        ));
        if let Some(cost) = token_cost_view(&self.provider_telemetry) {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::TokenCostUpdated { cost },
            ));
        }
        for cost in &self.provider_cost_usage {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::CostUsageRecorded { cost: cost.clone() },
            ));
            if let Some(cached_input_tokens) = cost.tokens.cached_input_tokens
                && cached_input_tokens > 0
            {
                events.push(RuntimeEvent::new(
                    next_sequence(&events),
                    RuntimeEventKind::ProviderCacheObserved {
                        provider_id: cost.provider_id.clone(),
                        model: cost.model.clone(),
                        cached_input_tokens,
                        cache_hit_microunits: 0,
                    },
                ));
            }
        }
        for input in &self.queued_runtime_inputs {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::InputQueued {
                    input: input.view(),
                },
            ));
        }
        for dag in &self.runtime_agent_dags {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::AgentDagUpdated { dag: dag.clone() },
            ));
        }
        match self.workflows.load_lane_state() {
            Ok(lane_state) => {
                // Durable Lane records survive restart, but live worker owners
                // do not. Only LaneSupervisor may publish a fresh binding when
                // it creates a process-local worker from an owner-scoped command.
                for lane in lane_state.lanes().values() {
                    events.push(RuntimeEvent::new(
                        next_sequence(&events),
                        RuntimeEventKind::LaneUpdated { lane: lane.clone() },
                    ));
                }
            }
            Err(_) => events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::Error {
                    error: RuntimeErrorView {
                        message: format!(
                            "lane_state_unavailable: {}",
                            crate::LANE_STATE_UNAVAILABLE_MESSAGE
                        ),
                        recoverable: true,
                        hint: Some("Repair or restore the project's lanes.jsonl log.".to_string()),
                    },
                },
            )),
        }
        let tracked_runtime_events = tracked_agent_job_runtime_events(&self.runtime_snapshot.cwd);
        for event in tracked_runtime_events {
            let RuntimeEvent {
                timestamp, kind, ..
            } = event;
            events.push(RuntimeEvent::with_timestamp(
                next_sequence(&events),
                timestamp,
                kind,
            ));
        }
        for session in tracked_agent_job_sessions(&self.runtime_snapshot.cwd) {
            let kind = match session.status {
                viden_types::AgentSessionStatus::Completed => {
                    RuntimeEventKind::AgentSessionCompleted { session }
                }
                viden_types::AgentSessionStatus::Failed => {
                    RuntimeEventKind::AgentSessionFailed { session }
                }
                _ => RuntimeEventKind::AgentSessionStarted { session },
            };
            events.push(RuntimeEvent::new(next_sequence(&events), kind));
        }
        for task in tracked_agent_job_tasks(&self.runtime_snapshot.cwd) {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::TaskUpdated { task },
            ));
        }
        for task in self.agent_task_snapshot() {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::TaskUpdated {
                    task: redacted_task_for_event(task),
                },
            ));
        }
        for evidence in &self.runtime_evidence {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::EvidenceRecorded {
                    evidence: evidence.clone(),
                },
            ));
            if let Some(canonical) = &evidence.canonical {
                events.push(RuntimeEvent::new(
                    next_sequence(&events),
                    RuntimeEventKind::EvidenceCanonicalized {
                        evidence_id: evidence.id.clone(),
                        item_id: canonical.item_id.clone(),
                        content_sha256: canonical.source_hash.clone(),
                    },
                ));
            }
        }
        for gate in &self.runtime_merge_gates {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::MergeGateUpdated { gate: gate.clone() },
            ));
        }
        for handoff in &self.runtime_handoffs {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::HandoffUpdated {
                    handoff: handoff.clone(),
                },
            ));
        }
        for review in &self.runtime_review_requests {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::ReviewRequestUpdated {
                    review: review.clone(),
                },
            ));
        }
        for contract in &self.runtime_contracts {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::ContractUpdated {
                    contract: contract.clone(),
                },
            ));
        }
        for dependency in &self.runtime_dependencies {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::DependencyUpdated {
                    dependency: dependency.clone(),
                },
            ));
        }
        for conflict in &self.runtime_conflict_bounces {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::MergeConflictBounced {
                    conflict: conflict.clone(),
                },
            ));
        }
        for revert in &self.runtime_reverts {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::RevertRecorded {
                    revert: revert.clone(),
                },
            ));
        }
        events
    }

    fn merge_gate_validation_facts(&self) -> MergeGateValidationFacts {
        let view = self.runtime_view_state();
        MergeGateValidationFacts {
            context_engine_root: self.context_engine_root.clone(),
            context_bundles: view.context_bundles,
        }
    }

    fn start_agent_dag(
        &mut self,
        goal: String,
        tasks: Vec<AgentDagTaskSpec>,
    ) -> Result<Vec<RuntimeEvent>, String> {
        if tasks.is_empty() {
            return Err("agent DAG requires at least one task".to_string());
        }
        let goal = sanitize_runtime_domain_text(&goal, 160);
        let tasks = tasks
            .into_iter()
            .map(sanitize_agent_task_spec_for_domain)
            .collect::<Vec<_>>();
        validate_agent_dag_tasks(&tasks)?;
        let now = now_timestamp();
        let dag = AgentDagRecord {
            dag_id: fresh_id("dag"),
            goal,
            status: AgentDagStatus::Active,
            tasks,
            created_at: Some(now),
            updated_at: Some(now),
        };

        let mut events = vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::AgentDagUpdated { dag: dag.clone() },
        )];
        for spec in &dag.tasks {
            let task = agent_task_record_from_spec(self, spec, now);
            self.upsert_agent_task(task.clone());
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::TaskUpdated { task },
            ));
            // Create the merge gate with the task so every agent output has a
            // review/evidence target before implementation work starts.
            let gate = MergeGateRecord {
                gate_id: format!("gate-{}", spec.task_id),
                task_id: spec.task_id.clone(),
                status: MergeGateStatus::Proposed,
                required_evidence: spec.required_evidence.clone(),
                evidence_ids: Vec::new(),
                gate_type: if spec
                    .required_evidence
                    .iter()
                    .any(|kind| canonical_required_evidence_kind(kind) == "patch")
                {
                    MergeGateType::Patch
                } else {
                    MergeGateType::Artifact
                },
                owner: RuntimeOwner {
                    task_id: Some(spec.task_id.clone()),
                    ..RuntimeOwner::default()
                },
                validator: None,
                policy_snapshot: MergeGatePolicySnapshot {
                    required_evidence: spec.required_evidence.clone(),
                    permission_snapshot_id: None,
                    requires_independent_validator: false,
                    captured_at: Some(now),
                },
                decision: None,
                conflict: None,
                applied_change_id: None,
                recovery_snapshot: None,
                audit_ids: Vec::new(),
                updated_at: Some(now),
            };
            self.runtime_merge_gates.push(gate.clone());
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::MergeGateUpdated { gate },
            ));
        }
        self.runtime_agent_dags.push(dag);
        Ok(events)
    }

    fn update_agent_task_status(
        &mut self,
        task_id: &str,
        status: AgentTaskStatus,
        activity: &str,
        progress: u8,
    ) -> Result<Vec<RuntimeEvent>, String> {
        let Some(mut task) = self
            .runtime_tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
        else {
            return Err(format!("agent task `{task_id}` does not exist"));
        };
        task.status = status;
        task.activity = activity.to_string();
        task.progress = progress;
        task.updated_at = Some(now_timestamp().saturating_mul(1000));
        self.upsert_agent_task(task.clone());
        Ok(vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::TaskUpdated { task },
        )])
    }

    fn cancel_agent_task(&mut self, task_id: &str) -> Result<Vec<RuntimeEvent>, String> {
        let _dag_id = self
            .runtime_agent_dags
            .iter()
            .find(|dag| dag.tasks.iter().any(|task| task.task_id == task_id))
            .map(|dag| dag.dag_id.clone())
            .ok_or_else(|| format!("agent DAG for task `{task_id}` does not exist"))?;
        let events = self.update_agent_task_status(
            task_id,
            AgentTaskStatus::Cancelled,
            "cancelled by operator",
            100,
        )?;
        Ok(events)
    }

    fn update_agent_task_failure(
        &mut self,
        task_id: &str,
        activity: &str,
        failure_class: &str,
        recovery_suggestion: &str,
    ) -> Result<Vec<RuntimeEvent>, String> {
        let Some(mut task) = self
            .runtime_tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
        else {
            return Err(format!("agent task `{task_id}` does not exist"));
        };
        task.status = AgentTaskStatus::Failed;
        task.activity = activity.to_string();
        task.progress = 100;
        task.result = Some(format!("failed:{failure_class}"));
        task.updated_at = Some(now_timestamp().saturating_mul(1000));
        task.next_action = Some(AgentNextAction {
            label: "retry agent task".to_string(),
            command: Some(format!("/agent start {task_id}")),
            reason: Some(format!("{failure_class}: {recovery_suggestion}")),
        });
        self.upsert_agent_task(task.clone());
        Ok(vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::TaskUpdated { task },
        )])
    }

    fn run_agent_task<F>(
        &mut self,
        task_id: &str,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(PermissionPrompt) -> ApprovalResponse,
    {
        self.run_agent_task_with_control(task_id, approver, &ModelRequestControl::new())
    }

    pub(crate) fn run_agent_task_with_control<F>(
        &mut self,
        task_id: &str,
        approver: &mut F,
        control: &ModelRequestControl,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(PermissionPrompt) -> ApprovalResponse,
    {
        self.run_agent_task_with_control_inner(task_id, approver, control, None)
    }

    pub(crate) fn run_agent_task_streaming_with_control<F, E>(
        &mut self,
        task_id: &str,
        approver: &mut F,
        control: &ModelRequestControl,
        emit_completed: &mut E,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(PermissionPrompt) -> ApprovalResponse,
        E: FnMut(Vec<RuntimeEvent>),
    {
        self.run_agent_task_with_control_inner(task_id, approver, control, Some(emit_completed))
    }

    fn run_agent_task_with_control_inner<F>(
        &mut self,
        task_id: &str,
        approver: &mut F,
        control: &ModelRequestControl,
        mut emit_completed: Option<&mut dyn FnMut(Vec<RuntimeEvent>)>,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(PermissionPrompt) -> ApprovalResponse,
    {
        let (dag_id, spec) = self
            .runtime_agent_dags
            .iter()
            .find_map(|dag| {
                dag.tasks
                    .iter()
                    .find(|task| task.task_id == task_id)
                    .cloned()
                    .map(|task| (dag.dag_id.clone(), task))
            })
            .ok_or_else(|| format!("agent task `{task_id}` does not exist"))?;

        if let Some(blocked_events) = self.blocked_by_unfinished_dependency(&dag_id, &spec)? {
            return Ok(blocked_events);
        }

        let mut events = self.update_agent_task_status(
            task_id,
            AgentTaskStatus::Running,
            "running supervised role task",
            10,
        )?;
        let prompt = agent_task_prompt(&spec);
        let seeded_context = self.agent_context_bundle(&spec, &prompt);
        let built_context =
            self.materialize_existing_context_bundle(&seeded_context, ContextBuildMode::Normal);
        let context = built_context.bundle.clone();
        self.last_context_bundle = Some(context.clone());
        self.last_context_runtime_events = built_context.events.clone();
        if built_context.hard_exceeded {
            append_resequenced(&mut events, built_context.events);
            append_resequenced(
                &mut events,
                self.update_agent_task_failure(
                    task_id,
                    "context hard limit exceeded before provider request",
                    "context_hard_limit",
                    "reduce input, narrow file scope, or split the task",
                )?,
            );
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::Error {
                    error: RuntimeErrorView {
                        message: format!(
                            "context hard limit exceeded for agent task `{task_id}` before provider request"
                        ),
                        recoverable: true,
                        hint: Some("reduce input, narrow file scope, or split the task".to_string()),
                    },
                },
            ));
            return Ok(events);
        }
        let previous_permissions = self.apply_agent_permission_policy(&spec);
        let previous_cost_attribution = self.active_cost_attribution.replace(CostAttribution {
            request_id: None,
            agent_task_id: Some(task_id.to_string()),
            dag_id: Some(dag_id.clone()),
            workflow_id: self.cost_workflow_id.clone(),
            smoke_run_id: self.cost_smoke_run_id.clone(),
        });
        let provider_result = match emit_completed.as_deref_mut() {
            Some(emit_completed) => self
                .process_runtime_turn_streaming_with_built_context_bundle_and_control(
                    &prompt,
                    approver,
                    control,
                    built_context,
                    emit_completed,
                ),
            None => self.process_runtime_turn_with_built_context_bundle_and_control(
                &prompt,
                approver,
                control,
                built_context,
            ),
        };
        self.active_cost_attribution = previous_cost_attribution;
        self.restore_agent_permission_policy(previous_permissions);
        let provider_events = match provider_result {
            Ok(events) => events,
            Err(failure) => {
                append_resequenced(&mut events, failure.completed_events);
                let err = failure.message;
                let cancelled = err.to_lowercase().contains("cancel");
                let activity = if cancelled {
                    "cancelled during supervised role task".to_string()
                } else {
                    format!("provider error: {}", truncate_for_preview(&err, 200))
                };
                let mut error_hint = "agent task stopped before evidence was accepted".to_string();
                let task_update = if cancelled {
                    self.update_agent_task_status(
                        task_id,
                        AgentTaskStatus::Cancelled,
                        &activity,
                        100,
                    )
                } else {
                    let classification = classify_agent_task_failure(&err);
                    error_hint = classification.recovery_suggestion.to_string();
                    self.update_agent_task_failure(
                        task_id,
                        &activity,
                        classification.class,
                        classification.recovery_suggestion,
                    )
                };
                match task_update {
                    Ok(task_events) => append_resequenced(&mut events, task_events),
                    Err(status_error) => {
                        error_hint = format!(
                            "{error_hint}; failed to persist terminal task state: {status_error}"
                        );
                    }
                }
                events.push(RuntimeEvent::new(
                    next_sequence(&events),
                    RuntimeEventKind::Error {
                        error: RuntimeErrorView {
                            message: err,
                            recoverable: true,
                            hint: Some(error_hint),
                        },
                    },
                ));
                events.push(RuntimeEvent::new(
                    next_sequence(&events),
                    RuntimeEventKind::SnapshotUpdated {
                        snapshot: self.runtime_snapshot(),
                    },
                ));
                return Ok(events);
            }
        };
        let assistant_output = assistant_output_from_events(&provider_events);
        append_resequenced(&mut events, provider_events);
        events.push(RuntimeEvent::new(
            next_sequence(&events),
            RuntimeEventKind::SnapshotUpdated {
                snapshot: self.runtime_snapshot(),
            },
        ));
        for event in &self.last_context_runtime_events {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                event.kind.clone(),
            ));
        }
        events.push(RuntimeEvent::new(
            next_sequence(&events),
            RuntimeEventKind::ContextUpdated { context },
        ));

        // Assistant text is display state, never an artifact or effect receipt.
        // Canonical evidence enters through the permissioned artifact ingestion path.
        let evidence_kind = "task_summary";
        let evidence_id = format!("evidence-{task_id}-{evidence_kind}");
        let summary = if assistant_output.trim().is_empty() {
            format!("{} completed without assistant text", spec.role)
        } else {
            assistant_output.clone()
        };
        let evidence = EvidenceView {
            id: evidence_id.clone(),
            kind: evidence_kind.to_string(),
            summary: sanitize_evidence_summary_for_event(&summary),
            path: None,
            source: Some(spec.role.as_str().to_string()),
            canonical: None,
            metadata: None,
            timestamp: Some(now_timestamp()),
            // The owner Core recorded on the task this summary closes. Never a
            // fresh owner: the evidence belongs to whoever owns the task.
            owner: self
                .runtime_tasks
                .iter()
                .find(|task| task.id == task_id)
                .and_then(|task| task.owner.clone()),
        };
        self.upsert_runtime_evidence(evidence.clone());
        events.push(RuntimeEvent::new(
            next_sequence(&events),
            RuntimeEventKind::EvidenceRecorded {
                evidence: evidence.clone(),
            },
        ));
        if let Some(canonical) = &evidence.canonical {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::EvidenceCanonicalized {
                    evidence_id: evidence.id.clone(),
                    item_id: canonical.item_id.clone(),
                    content_sha256: canonical.source_hash.clone(),
                },
            ));
        }

        let Some(mut task) = self
            .runtime_tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
        else {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::Error {
                    error: RuntimeErrorView {
                        message: format!(
                            "agent task `{task_id}` disappeared after provider execution"
                        ),
                        recoverable: true,
                        hint: Some("completed provider and tool facts were preserved".to_string()),
                    },
                },
            ));
            return Ok(events);
        };
        task.status = AgentTaskStatus::Done;
        task.activity = "supervised role task complete".to_string();
        task.progress = 100;
        task.result = Some(truncate_for_preview(&summary, 500));
        task.evidence.push(evidence_id.clone());
        task.updated_at = Some(now_timestamp().saturating_mul(1000));
        self.upsert_agent_task(task.clone());
        events.push(RuntimeEvent::new(
            next_sequence(&events),
            RuntimeEventKind::TaskUpdated { task },
        ));

        let runtime_evidence = self.runtime_evidence.clone();
        let validation_facts = self.merge_gate_validation_facts();
        if let Some(gate) = self
            .runtime_merge_gates
            .iter_mut()
            .find(|gate| gate.task_id == task_id)
        {
            if !gate.evidence_ids.contains(&evidence_id) {
                gate.evidence_ids.push(evidence_id);
            }
            let report = reduce_merge_gate_status(gate, &runtime_evidence, &validation_facts);
            gate.status = merge_gate_status_from_canonical(report.status);
            let independent_review_pending = report.status == EvidenceCanonicalStatus::Verified
                && gate.policy_snapshot.requires_independent_validator
                && gate
                    .validator
                    .as_ref()
                    .is_none_or(|validator| validator.validated_at.is_none());
            if independent_review_pending {
                gate.status = MergeGateStatus::CollectingEvidence;
            }
            gate.decision = canonical_reason_summary(&report)
                .or_else(|| {
                    independent_review_pending.then(|| "independent_review_required".to_string())
                })
                .map(|reason| {
                    MergeGateDecision::decided_now(
                        MergeGateDecisionOutcome::AwaitingEvidence,
                        reason,
                        gate.owner.clone(),
                        gate.evidence_ids.clone(),
                        fresh_id("audit"),
                    )
                });
            gate.updated_at = Some(now_timestamp());
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::MergeGateUpdated { gate: gate.clone() },
            ));
        }

        Ok(events)
    }

    fn apply_agent_permission_policy(&mut self, spec: &AgentDagTaskSpec) -> RuntimePermissionScope {
        let previous = RuntimePermissionScope {
            work_mode: self.runtime_snapshot.work_mode,
            permission_mode: self.runtime_snapshot.permission_mode,
            permission_level: self.runtime_snapshot.permission_level,
            permission_context: self.permissions.context_snapshot(),
        };
        let scoped_mode =
            agent_permission_mode_for_policy(self.permissions.mode(), &spec.permission_policy);
        self.permissions.set_mode(scoped_mode);
        let mut scoped_engine = self.permissions.engine_snapshot();
        apply_agent_role_permission_rules(&mut scoped_engine, spec);
        self.permissions.replace_engine(scoped_engine);
        self.runtime_snapshot.permission_mode = scoped_mode;
        self.runtime_snapshot.permission_level = PermissionLevel::from_legacy_mode(scoped_mode);
        if scoped_mode == PermissionMode::Plan {
            self.runtime_snapshot.work_mode = WorkMode::Plan;
        }
        previous
    }

    fn restore_agent_permission_policy(&mut self, previous: RuntimePermissionScope) {
        self.permissions
            .restore_context(previous.permission_context);
        self.runtime_snapshot.work_mode = previous.work_mode;
        self.runtime_snapshot.permission_mode = previous.permission_mode;
        self.runtime_snapshot.permission_level = previous.permission_level;
    }

    fn agent_context_bundle(
        &self,
        spec: &AgentDagTaskSpec,
        prompt: &str,
    ) -> viden_types::ContextBundleRecord {
        let mut bundle = self.build_main_context_bundle(prompt);
        bundle.bundle_id = format!("ctx-agent-{}", spec.task_id);
        bundle.task_id = spec.task_id.clone();
        bundle.policy = format!("agent-role-{}-priority-budget", spec.role.as_str());
        let mut agent_sources = vec![ContextSourceRecord {
            name: "agent-role".to_string(),
            kind: "agent-task".to_string(),
            priority: 100,
            estimated_tokens: 160,
            summary: format!("{}: {}", spec.role.as_str(), spec.objective),
            include_reason: "role objective pins the ContextBundle to the AgentTask".to_string(),
            handle_id: None,
            item_id: None,
            view_id: None,
            content_sha256: None,
            view_sha256: None,
            quality_id: None,
        }];
        agent_sources.push(role_guidance_context_source(spec.role));
        if !spec.file_scope.is_empty() {
            agent_sources.push(agent_file_scope_context_source(spec));
        }
        if let Some(source) = agent_selected_files_context_source(&self.cwd, spec) {
            agent_sources.push(source);
        }
        if let Some(source) = agent_selected_symbols_context_source(&self.cwd, spec) {
            agent_sources.push(source);
        }
        if let Some(source) = self.agent_lsp_diagnostics_context_source(spec) {
            agent_sources.push(source);
        }
        if !spec.required_evidence.is_empty() {
            agent_sources.push(agent_evidence_contract_context_source(spec));
        }
        for (index, source) in agent_sources.into_iter().enumerate() {
            bundle.sources.insert(index, source);
        }
        bundle.estimated_tokens = bundle
            .sources
            .iter()
            .map(|source| source.estimated_tokens)
            .sum();
        bundle
            .compaction_notes
            .push(format!("agent task {}", spec.task_id));
        bundle
    }

    fn agent_lsp_diagnostics_context_source(
        &self,
        spec: &AgentDagTaskSpec,
    ) -> Option<ContextSourceRecord> {
        let mut diagnostics = Vec::new();
        for file in select_role_specific_files(&self.cwd, spec)
            .into_iter()
            .filter(|file| is_lsp_context_candidate(file))
            .take(4)
        {
            let Ok(mut file_diagnostics) =
                self.lsp_runtime.diagnostics(&self.cwd, Path::new(&file))
            else {
                continue;
            };
            diagnostics.append(&mut file_diagnostics);
            if diagnostics.len() >= 16 {
                break;
            }
        }
        if diagnostics.is_empty() {
            return None;
        }
        diagnostics.truncate(16);
        let rendered = render_lsp_diagnostics(&self.cwd, &diagnostics);
        Some(ContextSourceRecord {
            name: "role-lsp-diagnostics".to_string(),
            kind: "lsp-diagnostics".to_string(),
            priority: 93,
            estimated_tokens: diagnostics.len().saturating_mul(64).min(960) as u64,
            summary: truncate_for_preview(
                &redact_context_summary_for_event(&self.cwd, &rendered),
                1_500,
            ),
            include_reason: format!(
                "{} role receives live diagnostics from selected scoped files",
                spec.role.as_str()
            ),
            handle_id: None,
            item_id: None,
            view_id: None,
            content_sha256: None,
            view_sha256: None,
            quality_id: None,
        })
    }

    fn blocked_by_unfinished_dependency(
        &mut self,
        _dag_id: &str,
        spec: &AgentDagTaskSpec,
    ) -> Result<Option<Vec<RuntimeEvent>>, String> {
        let Some(blocking_dependency) = self.blocking_dependency_for_task(&spec.task_id) else {
            return Ok(None);
        };

        let mut task = self
            .runtime_tasks
            .iter()
            .find(|task| task.id == spec.task_id)
            .cloned()
            .ok_or_else(|| format!("agent task `{}` does not exist", spec.task_id))?;
        task.status = AgentTaskStatus::Blocked;
        task.activity = format!("waiting for dependency `{blocking_dependency}`");
        task.progress = 0;
        task.updated_at = Some(now_timestamp().saturating_mul(1000));
        self.upsert_agent_task(task.clone());
        Ok(Some(vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::TaskUpdated { task },
        )]))
    }

    pub(crate) fn validated_evidence_bindings_for_ids(
        &self,
        gate_index: usize,
        evidence_ids: &[String],
    ) -> Result<Vec<viden_types::ReviewedEvidenceBinding>, String> {
        let gate = &self.runtime_merge_gates[gate_index];
        let facts = self.merge_gate_validation_facts();
        let mut bindings = Vec::with_capacity(evidence_ids.len());
        for evidence_id in evidence_ids {
            let evidence = self
                .runtime_evidence
                .iter()
                .find(|evidence| evidence.id == *evidence_id)
                .ok_or_else(|| format!("review evidence `{evidence_id}` does not exist"))?;
            let report = validate_canonical_evidence_for_gate(gate, evidence, &facts);
            if report.status != EvidenceCanonicalStatus::Verified {
                return Err(format!(
                    "review evidence `{evidence_id}` is not canonical: {}",
                    canonical_reason_summary(&report)
                        .unwrap_or_else(|| "canonical_evidence_invalid".to_string())
                ));
            }
            let canonical = evidence
                .canonical
                .as_ref()
                .ok_or_else(|| format!("review evidence `{evidence_id}` is not canonical"))?;
            bindings.push(viden_types::ReviewedEvidenceBinding {
                evidence_id: evidence_id.clone(),
                source_hash: canonical.source_hash.clone(),
            });
        }
        bindings.sort();
        bindings.dedup();
        Ok(bindings)
    }

    pub(crate) fn preflight_accept_merge_gate(
        &self,
        gate_id: &str,
        actor: &RuntimeOwner,
        reviewed_evidence: &[viden_types::ReviewedEvidenceBinding],
    ) -> Result<(), String> {
        let gate_index = self
            .runtime_merge_gates
            .iter()
            .position(|gate| gate.gate_id == gate_id)
            .ok_or_else(|| format!("merge gate `{gate_id}` does not exist"))?;
        let gate = &self.runtime_merge_gates[gate_index];
        let report = reduce_merge_gate_status(
            gate,
            &self.runtime_evidence,
            &self.merge_gate_validation_facts(),
        );
        if report.status != EvidenceCanonicalStatus::Verified {
            return Err(format!(
                "merge gate `{gate_id}` cannot be accepted: {}",
                canonical_reason_summary(&report)
                    .unwrap_or_else(|| "canonical_evidence_invalid".to_string())
            ));
        }
        if gate
            .conflict
            .as_ref()
            .is_some_and(|conflict| conflict.status == viden_types::ConflictBounceStatus::Pending)
        {
            return Err(format!(
                "merge gate `{gate_id}` has a pending conflict that requires origin-lane revalidation"
            ));
        }
        let current_bindings =
            self.validated_evidence_bindings_for_ids(gate_index, &gate.evidence_ids)?;
        let mut reviewed_evidence = reviewed_evidence.to_vec();
        reviewed_evidence.sort();
        reviewed_evidence.dedup();
        if let Some(validator) = &gate.validator {
            if !validator.independent {
                return Err(format!(
                    "merge gate `{gate_id}` requires an independent validator"
                ));
            }
            if !runtime_owner_matches_validator_lane(actor, &validator.owner) {
                return Err(
                    "merge gate acceptance actor does not match validator owner".to_string()
                );
            }
            let review = self
                .runtime_review_requests
                .iter()
                .find(|review| review.review_id == validator.review_request_id)
                .ok_or_else(|| "merge gate validator review request does not exist".to_string())?;
            // `DecideReview` owns settled reviews: an accepted verdict still
            // lets the operator decide the gate, but a rejected one fails
            // closed here rather than being overwritten by the gate decision.
            if review.status == ReviewRequestStatus::Rejected {
                return Err(format!(
                    "cannot accept merge gate `{gate_id}`: independent review `{}` was rejected",
                    review.review_id
                ));
            }
            if current_bindings != review.evidence_bindings
                || reviewed_evidence != review.evidence_bindings
            {
                return Err("merge gate reviewed evidence changed after review request".to_string());
            }
        } else {
            if gate.policy_snapshot.requires_independent_validator {
                return Err(format!(
                    "merge gate `{gate_id}` requires an independent validator"
                ));
            }
            if actor != &gate.owner {
                return Err("merge gate acceptance actor does not match gate owner".to_string());
            }
            if gate.owner != RuntimeOwner::default() && reviewed_evidence != current_bindings {
                return Err("merge gate acceptance requires exact reviewed evidence".to_string());
            }
        }
        Ok(())
    }

    pub(crate) fn preflight_accept_agent_artifact(
        &self,
        gate_id: &str,
        evidence_id: &str,
        actor: &RuntimeOwner,
        source_hash: &str,
    ) -> Result<(), String> {
        let gate_index = self
            .runtime_merge_gates
            .iter()
            .position(|gate| gate.gate_id == gate_id)
            .ok_or_else(|| format!("merge gate `{gate_id}` does not exist"))?;
        let gate = &self.runtime_merge_gates[gate_index];
        if gate.policy_snapshot.requires_independent_validator || gate.validator.is_some() {
            return Err(
                "agent artifact shortcut cannot bypass independent review policy".to_string(),
            );
        }
        if gate.conflict.is_some() {
            return Err("agent artifact shortcut cannot accept a conflicted gate".to_string());
        }
        if gate.evidence_ids.len() != 1 || gate.evidence_ids[0] != evidence_id {
            return Err(
                "agent artifact shortcut requires a single exact gate evidence binding".to_string(),
            );
        }
        self.preflight_accept_merge_gate(
            gate_id,
            actor,
            &[viden_types::ReviewedEvidenceBinding {
                evidence_id: evidence_id.to_string(),
                source_hash: source_hash.to_string(),
            }],
        )
    }

    pub(crate) fn preflight_reject_merge_gate(
        &self,
        gate_id: &str,
        actor: &RuntimeOwner,
    ) -> Result<(), String> {
        let gate_index = self
            .runtime_merge_gates
            .iter()
            .position(|gate| gate.gate_id == gate_id)
            .ok_or_else(|| format!("merge gate `{gate_id}` does not exist"))?;
        let gate = &self.runtime_merge_gates[gate_index];
        validate_reject_actor(gate, actor, "merge gate rejection")
    }

    pub(crate) fn preflight_reject_agent_artifact(
        &self,
        gate_id: &str,
        evidence_id: &str,
        actor: &RuntimeOwner,
    ) -> Result<(), String> {
        if evidence_id.trim().is_empty() {
            return Err("agent artifact evidence id cannot be empty".to_string());
        }
        let gate_index = self
            .runtime_merge_gates
            .iter()
            .position(|gate| gate.gate_id == gate_id)
            .ok_or_else(|| format!("merge gate `{gate_id}` does not exist"))?;
        if !self
            .runtime_evidence
            .iter()
            .any(|evidence| evidence.id == evidence_id)
        {
            return Err(format!(
                "agent artifact evidence `{evidence_id}` does not exist"
            ));
        }
        if !self.runtime_merge_gates[gate_index]
            .evidence_ids
            .iter()
            .any(|id| id == evidence_id)
        {
            return Err(format!(
                "agent artifact evidence `{evidence_id}` is not bound to the gate evidence set"
            ));
        }
        validate_reject_actor(
            &self.runtime_merge_gates[gate_index],
            actor,
            "agent artifact rejection",
        )?;
        Ok(())
    }

    pub(crate) fn preflight_merge_agent_patch(
        &self,
        gate_id: &str,
        actor: &RuntimeOwner,
    ) -> Result<String, String> {
        let gate_index = self
            .runtime_merge_gates
            .iter()
            .position(|gate| gate.gate_id == gate_id)
            .ok_or_else(|| format!("merge gate `{gate_id}` does not exist"))?;
        let gate = &self.runtime_merge_gates[gate_index];
        let _dag_id = self.dag_id_for_task(&gate.task_id)?;
        if actor != &gate.owner {
            return Err("patch merge actor does not match gate owner".to_string());
        }
        if gate.status != MergeGateStatus::Accepted {
            return Err(format!(
                "merge gate `{gate_id}` must be accepted before patch merge"
            ));
        }
        let decision = gate
            .decision
            .as_ref()
            .ok_or_else(|| "accepted merge gate is missing a typed decision".to_string())?;
        if decision.outcome != MergeGateDecisionOutcome::Accepted {
            return Err("patch merge requires a typed Accepted decision".to_string());
        }
        let current_bindings =
            self.validated_evidence_bindings_for_ids(gate_index, &gate.evidence_ids)?;
        let mut decided_bindings = decision.reviewed_evidence.clone();
        decided_bindings.sort();
        decided_bindings.dedup();
        if decided_bindings != current_bindings {
            return Err(
                "accepted merge decision no longer binds exact canonical evidence".to_string(),
            );
        }
        if let Some(validator) = &gate.validator {
            if !validator.independent || validator.validated_at.is_none() {
                return Err("patch merge requires completed independent validation".to_string());
            }
            let review = self
                .runtime_review_requests
                .iter()
                .find(|review| review.review_id == validator.review_request_id)
                .ok_or_else(|| "merge gate validator review request does not exist".to_string())?;
            if review.status != ReviewRequestStatus::Accepted
                || review.evidence_bindings != current_bindings
                || decision.review_request_id.as_deref() != Some(review.review_id.as_str())
            {
                return Err("patch merge requires the accepted exact-evidence review".to_string());
            }
        }
        if gate
            .conflict
            .as_ref()
            .is_some_and(|conflict| conflict.status == viden_types::ConflictBounceStatus::Pending)
        {
            return Err("patch merge cannot bypass an unresolved conflict bounce".to_string());
        }
        let report = reduce_merge_gate_status(
            gate,
            &self.runtime_evidence,
            &self.merge_gate_validation_facts(),
        );
        if report.status != EvidenceCanonicalStatus::Verified {
            return Err(format!(
                "patch canonical preflight failed: {}",
                canonical_reason_summary(&report)
                    .unwrap_or_else(|| "canonical_evidence_invalid".to_string())
            ));
        }
        let patch_evidence = self
            .patch_evidence_for_gate(gate_index)
            .ok_or_else(|| "accepted merge gate has no patch evidence".to_string())?;
        self.verified_patch_content(patch_evidence)
            .map_err(|error| format!("patch canonical preflight failed: {error}"))
    }

    pub(crate) fn preflight_record_agent_evidence(
        &self,
        gate_id: &str,
        evidence_id: Option<&str>,
        kind: &str,
        canonical: Option<&CanonicalEvidenceReference>,
    ) -> Result<(), String> {
        let gate_index = self
            .runtime_merge_gates
            .iter()
            .position(|gate| gate.gate_id == gate_id)
            .ok_or_else(|| format!("merge gate `{gate_id}` does not exist"))?;
        if normalize_evidence_kind(kind).is_empty() {
            return Err("agent evidence kind cannot be empty".to_string());
        }
        let Some(canonical) = canonical else {
            return Ok(());
        };
        let mut canonical = validate_external_canonical_evidence_reference(canonical.clone())?;
        canonical.permission_snapshot_id = Some("permission-preflight".to_string());
        canonical.permission_scope = canonical.evidence_scope.clone();
        canonical.verification = EvidenceVerificationState::Verified;
        canonical.quality.status = EvidenceQualityStatus::Pass;
        canonical.quality.reason_codes.clear();
        let evidence = EvidenceView {
            id: evidence_id.unwrap_or("evidence-preflight").to_string(),
            kind: normalize_evidence_kind(kind),
            summary: "preflight".to_string(),
            path: None,
            source: None,
            canonical: Some(canonical),
            metadata: None,
            timestamp: None,
            // Validator input only; this record is never published.
            owner: None,
        };
        let report = validate_canonical_evidence_for_gate(
            &self.runtime_merge_gates[gate_index],
            &evidence,
            &self.merge_gate_validation_facts(),
        );
        if report.status != EvidenceCanonicalStatus::Verified {
            return Err(format!(
                "agent evidence canonical preflight failed: {}",
                canonical_reason_summary(&report)
                    .unwrap_or_else(|| "canonical_evidence_invalid".to_string())
            ));
        }
        Ok(())
    }

    fn decide_merge_gate<F>(
        &mut self,
        gate_id: &str,
        status: MergeGateStatus,
        actor: Option<RuntimeOwner>,
        mut reviewed_evidence: Vec<viden_types::ReviewedEvidenceBinding>,
        decision: String,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(PermissionPrompt) -> ApprovalResponse,
    {
        let decision = sanitize_runtime_domain_text(&decision, 240);
        if decision.trim().is_empty() {
            return Err("merge gate decision cannot be empty".to_string());
        }
        let gate_index = self
            .runtime_merge_gates
            .iter()
            .position(|gate| gate.gate_id == gate_id)
            .ok_or_else(|| format!("merge gate `{gate_id}` does not exist"))?;
        let task_id = self.runtime_merge_gates[gate_index].task_id.clone();
        let _dag_id = self
            .runtime_agent_dags
            .iter()
            .find(|dag| dag.tasks.iter().any(|task| task.task_id == task_id))
            .map(|dag| dag.dag_id.clone())
            .ok_or_else(|| format!("agent DAG for task `{task_id}` does not exist"))?;
        let mut accepted_review_request_id = None;
        if status == MergeGateStatus::Accepted {
            let report = reduce_merge_gate_status(
                &self.runtime_merge_gates[gate_index],
                &self.runtime_evidence,
                &self.merge_gate_validation_facts(),
            );
            if report.status != EvidenceCanonicalStatus::Verified {
                return Err(format!(
                    "merge gate `{gate_id}` cannot be accepted: {}",
                    canonical_reason_summary(&report)
                        .unwrap_or_else(|| "canonical_evidence_invalid".to_string())
                ));
            }
            if self.runtime_merge_gates[gate_index]
                .policy_snapshot
                .requires_independent_validator
                && !self.runtime_merge_gates[gate_index]
                    .validator
                    .as_ref()
                    .is_some_and(|validator| validator.independent)
            {
                return Err(format!(
                    "merge gate `{gate_id}` requires an independent validator"
                ));
            }
            if self.runtime_merge_gates[gate_index]
                .conflict
                .as_ref()
                .is_some_and(|conflict| {
                    conflict.status == viden_types::ConflictBounceStatus::Pending
                })
            {
                return Err(format!(
                    "merge gate `{gate_id}` has a pending conflict that requires origin-lane revalidation"
                ));
            }

            let actor = actor
                .as_ref()
                .ok_or_else(|| "merge gate acceptance requires an actor".to_string())?;
            let gate = &self.runtime_merge_gates[gate_index];
            let current_bindings =
                self.validated_evidence_bindings_for_ids(gate_index, &gate.evidence_ids)?;
            reviewed_evidence.sort();
            reviewed_evidence.dedup();
            if let Some(validator) = &gate.validator {
                if !runtime_owner_matches_validator_lane(actor, &validator.owner) {
                    return Err(
                        "merge gate acceptance actor does not match validator owner".to_string()
                    );
                }
                let review = self
                    .runtime_review_requests
                    .iter()
                    .find(|review| review.review_id == validator.review_request_id)
                    .ok_or_else(|| {
                        "merge gate validator review request does not exist".to_string()
                    })?;
                // See `preflight_accept_merge_gate`: a rejected independent
                // review blocks acceptance; an accepted one is honoured.
                if review.status == ReviewRequestStatus::Rejected {
                    return Err(format!(
                        "cannot accept merge gate `{gate_id}`: independent review `{}` was rejected",
                        review.review_id
                    ));
                }
                if current_bindings != review.evidence_bindings
                    || reviewed_evidence != review.evidence_bindings
                {
                    return Err(
                        "merge gate reviewed evidence changed after review request".to_string()
                    );
                }
                accepted_review_request_id = Some(review.review_id.clone());
            } else {
                if actor != &gate.owner {
                    return Err("merge gate acceptance actor does not match gate owner".to_string());
                }
                if gate.owner != RuntimeOwner::default() && reviewed_evidence != current_bindings {
                    return Err(
                        "merge gate acceptance requires exact reviewed evidence".to_string()
                    );
                }
                if gate.owner == RuntimeOwner::default() && reviewed_evidence.is_empty() {
                    reviewed_evidence = current_bindings;
                }
            }
        } else {
            let actor = actor
                .as_ref()
                .ok_or_else(|| "merge gate rejection requires an actor".to_string())?;
            validate_reject_actor(
                &self.runtime_merge_gates[gate_index],
                actor,
                "merge gate rejection",
            )?;
        }

        self.require_trust_permission(
            if status == MergeGateStatus::Accepted {
                "accept_merge_gate"
            } else {
                "reject_merge_gate"
            },
            &format!("gate={gate_id}"),
            approver,
        )?;

        let now = now_timestamp();
        let audit_id = fresh_id("audit");
        let owner = actor.unwrap_or_else(|| self.runtime_merge_gates[gate_index].owner.clone());
        // Audit before mutation, the same rule as every other trust site: the
        // operator's accept/reject is the most consequential decision in the
        // loop, so a failed audit append must fail the decision rather than
        // leave an unauditable gate transition.
        let mut decision_objects = vec![
            AuditObjectRef::new(AuditObjectRef::KIND_MERGE_GATE, gate_id),
            AuditObjectRef::new(AuditObjectRef::KIND_TASK, &task_id),
        ];
        if let Some(review_request_id) = accepted_review_request_id.as_deref().or_else(|| {
            self.runtime_merge_gates[gate_index]
                .validator
                .as_ref()
                .map(|validator| validator.review_request_id.as_str())
        }) {
            decision_objects.push(AuditObjectRef::new(
                AuditObjectRef::KIND_REVIEW_REQUEST,
                review_request_id,
            ));
        }
        self.append_trust_audit(&crate::trust_loop::trust_audit_record(
            &audit_id,
            now,
            &owner,
            "gate.decided",
            decision_objects,
            &[(
                "outcome",
                if status == MergeGateStatus::Accepted {
                    "accepted"
                } else {
                    "needs_changes"
                },
            )],
        )?)?;
        let gate = &mut self.runtime_merge_gates[gate_index];
        gate.status = status;
        let reviewed_evidence_ids = reviewed_evidence
            .iter()
            .map(|binding| binding.evidence_id.clone())
            .collect::<Vec<_>>();
        let mut typed_decision = MergeGateDecision::decided_now(
            if status == MergeGateStatus::Accepted {
                MergeGateDecisionOutcome::Accepted
            } else {
                MergeGateDecisionOutcome::Rejected
            },
            decision.clone(),
            owner,
            reviewed_evidence_ids.clone(),
            audit_id.clone(),
        );
        typed_decision.reviewed_evidence = reviewed_evidence;
        typed_decision.review_request_id = accepted_review_request_id.clone();
        gate.decision = Some(typed_decision);
        if status == MergeGateStatus::Accepted
            && let Some(validator) = &mut gate.validator
        {
            validator.validated_at = Some(now);
        }
        gate.audit_ids.push(audit_id);
        gate.updated_at = Some(now);
        let review_request_id = accepted_review_request_id.or_else(|| {
            gate.validator
                .as_ref()
                .map(|validator| validator.review_request_id.clone())
        });
        let gate = gate.clone();

        let mut events = vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::MergeGateUpdated { gate },
        )];
        // Legacy auto-flip: a gate decided without an explicit `DecideReview`
        // settles its still-pending review. A review already settled by the
        // reviewer lane is authoritative and is never overwritten here.
        if let Some(review_request_id) = review_request_id
            && let Some(review) = self
                .runtime_review_requests
                .iter_mut()
                .find(|review| review.review_id == review_request_id)
            && review.status == ReviewRequestStatus::Pending
        {
            review.status = if status == MergeGateStatus::Accepted {
                ReviewRequestStatus::Accepted
            } else {
                ReviewRequestStatus::Rejected
            };
            review.updated_at = now;
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::ReviewRequestUpdated {
                    review: review.clone(),
                },
            ));
        }
        if let Some(mut task) = self
            .runtime_tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
        {
            task.decision = Some(decision.clone());
            task.updated_at = Some(now.saturating_mul(1000));
            if status == MergeGateStatus::NeedsChanges {
                task.status = AgentTaskStatus::NeedsInput;
                task.activity = format!("merge gate requested changes: {decision}");
                task.next_action = Some(AgentNextAction {
                    label: "revise task".to_string(),
                    command: Some(format!("/agent start {}", task.id)),
                    reason: Some("merge gate rejected the current evidence".to_string()),
                });
            } else {
                task.activity = format!("merge gate accepted: {decision}");
                task.next_action = None;
            }
            self.upsert_agent_task(task.clone());
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::TaskUpdated { task },
            ));
        }

        Ok(events)
    }

    fn record_agent_evidence<F>(
        &mut self,
        request: RecordAgentEvidenceRequest<'_>,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(PermissionPrompt) -> ApprovalResponse,
    {
        let RecordAgentEvidenceRequest {
            gate_id,
            evidence_id,
            kind,
            summary,
            path,
            source,
            canonical,
        } = request;
        let kind = normalize_evidence_kind(&kind);
        if kind.is_empty() {
            return Err("agent evidence kind cannot be empty".to_string());
        }
        let summary = summary.trim().to_string();
        if summary.is_empty() {
            return Err("agent evidence summary cannot be empty".to_string());
        }
        let path = match path {
            Some(path) => Some(validate_evidence_path_for_event(&path)?),
            None => None,
        };
        let source = source
            .map(|source| sanitize_evidence_source_for_event(&source))
            .filter(|source| !source.is_empty());
        let summary = sanitize_evidence_summary_for_event(&summary);
        let mut canonical = canonical
            .map(validate_external_canonical_evidence_reference)
            .transpose()?;

        let gate_index = self
            .runtime_merge_gates
            .iter()
            .position(|gate| gate.gate_id == gate_id)
            .ok_or_else(|| format!("merge gate `{gate_id}` does not exist"))?;
        let task_id = self.runtime_merge_gates[gate_index].task_id.clone();
        let _dag_id = self.dag_id_for_task(&task_id)?;
        self.preflight_record_agent_evidence(
            gate_id,
            evidence_id.as_deref(),
            &kind,
            canonical.as_ref(),
        )?;
        self.require_trust_permission(
            "record_agent_evidence",
            &format!("gate={gate_id} kind={kind}"),
            approver,
        )?;
        if let Some(canonical) = &mut canonical {
            canonical.permission_snapshot_id = Some(fresh_id("permission-receipt"));
            canonical.permission_scope = canonical.evidence_scope.clone();
            canonical.verification = EvidenceVerificationState::Verified;
            canonical.quality.status = EvidenceQualityStatus::Pass;
            canonical.quality.reason_codes.clear();
        }
        let evidence_id = evidence_id
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| fresh_id(&format!("evidence-{task_id}-{kind}")));
        let now = now_timestamp();
        let evidence = EvidenceView {
            id: evidence_id.clone(),
            kind: kind.clone(),
            summary: truncate_for_preview(&summary, 500),
            path,
            source,
            canonical,
            metadata: None,
            timestamp: Some(now),
            // Gate-bound evidence carries the gate's own owner — the identity
            // Core already publishes on `MergeGateRecord` — never a new one.
            owner: Some(self.runtime_merge_gates[gate_index].owner.clone()),
        };
        self.upsert_runtime_evidence(evidence.clone());

        let runtime_evidence = self.runtime_evidence.clone();
        let validation_facts = self.merge_gate_validation_facts();
        {
            let gate = &mut self.runtime_merge_gates[gate_index];
            if !gate.evidence_ids.contains(&evidence_id) {
                gate.evidence_ids.push(evidence_id.clone());
            }
        }
        let report = reduce_merge_gate_status(
            &self.runtime_merge_gates[gate_index],
            &runtime_evidence,
            &validation_facts,
        );
        let gate_status = merge_gate_status_from_canonical(report.status);
        let canonical_reasons = canonical_reason_summary(&report).unwrap_or_default();
        let independent_review_pending = report.status == EvidenceCanonicalStatus::Verified
            && self.runtime_merge_gates[gate_index]
                .policy_snapshot
                .requires_independent_validator
            && self.runtime_merge_gates[gate_index]
                .validator
                .as_ref()
                .is_none_or(|validator| validator.validated_at.is_none());
        let conflict_pending = self.runtime_merge_gates[gate_index]
            .conflict
            .as_ref()
            .is_some_and(|conflict| conflict.status == viden_types::ConflictBounceStatus::Pending);
        let gate = &mut self.runtime_merge_gates[gate_index];
        gate.status = gate_status;
        if conflict_pending {
            gate.status = MergeGateStatus::NeedsChanges;
        } else if independent_review_pending {
            gate.status = MergeGateStatus::CollectingEvidence;
        }
        if !conflict_pending {
            gate.decision = if !canonical_reasons.is_empty() || independent_review_pending {
                let reason = if independent_review_pending {
                    "independent_review_required".to_string()
                } else {
                    canonical_reasons.clone()
                };
                Some(MergeGateDecision::decided_now(
                    MergeGateDecisionOutcome::AwaitingEvidence,
                    reason,
                    gate.owner.clone(),
                    gate.evidence_ids.clone(),
                    fresh_id("audit"),
                ))
            } else {
                None
            };
        }
        gate.updated_at = Some(now);
        let gate = gate.clone();

        let mut events = vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::EvidenceRecorded {
                evidence: evidence.clone(),
            },
        )];
        if let Some(canonical) = &evidence.canonical {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::EvidenceCanonicalized {
                    evidence_id: evidence.id.clone(),
                    item_id: canonical.item_id.clone(),
                    content_sha256: canonical.source_hash.clone(),
                },
            ));
        }
        events.push(RuntimeEvent::new(
            next_sequence(&events),
            RuntimeEventKind::MergeGateUpdated { gate },
        ));
        if let Some(mut task) = self
            .runtime_tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
        {
            if !task.evidence.contains(&evidence_id) {
                task.evidence.push(evidence_id.clone());
            }
            task.activity = format!("evidence recorded: {kind}");
            task.updated_at = Some(now.saturating_mul(1000));
            self.upsert_agent_task(task.clone());
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::TaskUpdated { task },
            ));
        }

        Ok(events)
    }

    fn accept_agent_artifact<F>(
        &mut self,
        gate_id: &str,
        evidence_id: String,
        actor: RuntimeOwner,
        source_hash: String,
        decision: String,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(PermissionPrompt) -> ApprovalResponse,
    {
        let decision = sanitize_runtime_domain_text(&decision, 240);
        if decision.trim().is_empty() {
            return Err("agent artifact decision cannot be empty".to_string());
        }
        if evidence_id.trim().is_empty() {
            return Err("agent artifact evidence id cannot be empty".to_string());
        }
        let gate_index = self
            .runtime_merge_gates
            .iter()
            .position(|gate| gate.gate_id == gate_id)
            .ok_or_else(|| format!("merge gate `{gate_id}` does not exist"))?;
        let task_id = self.runtime_merge_gates[gate_index].task_id.clone();
        let _dag_id = self.dag_id_for_task(&task_id)?;
        let gate = &self.runtime_merge_gates[gate_index];
        if gate.policy_snapshot.requires_independent_validator || gate.validator.is_some() {
            return Err(
                "agent artifact shortcut cannot bypass independent review policy".to_string(),
            );
        }
        if gate.conflict.is_some() {
            return Err("agent artifact shortcut cannot accept a conflicted gate".to_string());
        }
        if gate.evidence_ids.len() != 1 || gate.evidence_ids[0] != evidence_id {
            return Err(
                "agent artifact shortcut requires a single exact gate evidence binding".to_string(),
            );
        }
        let evidence = self
            .runtime_evidence
            .iter()
            .find(|evidence| evidence.id == evidence_id)
            .cloned()
            .ok_or_else(|| format!("agent artifact evidence `{evidence_id}` does not exist"))?;
        let canonical = evidence
            .canonical
            .as_ref()
            .ok_or_else(|| format!("agent artifact evidence `{evidence_id}` is not canonical"))?;
        if source_hash != canonical.source_hash {
            return Err("agent artifact source hash does not match canonical bytes".to_string());
        }
        self.decide_merge_gate(
            gate_id,
            MergeGateStatus::Accepted,
            Some(actor),
            vec![viden_types::ReviewedEvidenceBinding {
                evidence_id,
                source_hash,
            }],
            decision,
            approver,
        )
    }

    fn reject_agent_artifact<F>(
        &mut self,
        gate_id: &str,
        evidence_id: &str,
        actor: RuntimeOwner,
        reason: String,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(PermissionPrompt) -> ApprovalResponse,
    {
        let reason = sanitize_runtime_domain_text(&reason, 240);
        if reason.trim().is_empty() {
            return Err("agent artifact rejection reason cannot be empty".to_string());
        }
        self.preflight_reject_agent_artifact(gate_id, evidence_id, &actor)?;
        let gate_index = self
            .runtime_merge_gates
            .iter()
            .position(|gate| gate.gate_id == gate_id)
            .ok_or_else(|| format!("merge gate `{gate_id}` does not exist"))?;
        let task_id = self.runtime_merge_gates[gate_index].task_id.clone();
        let _dag_id = self.dag_id_for_task(&task_id)?;
        self.require_trust_permission(
            "reject_agent_artifact",
            &format!("gate={gate_id} evidence={evidence_id}"),
            approver,
        )?;
        let now = now_timestamp();
        let audit_id = fresh_id("audit");
        // Audit before mutation: rejecting an artifact is a permission-gated
        // operator decision that drops evidence from the gate.
        self.append_trust_audit(&crate::trust_loop::trust_audit_record(
            &audit_id,
            now,
            &actor,
            "evidence.rejected",
            vec![
                AuditObjectRef::new(AuditObjectRef::KIND_MERGE_GATE, gate_id),
                AuditObjectRef::new(AuditObjectRef::KIND_TASK, &task_id),
                AuditObjectRef::new(AuditObjectRef::KIND_EVIDENCE, evidence_id),
            ],
            &[("outcome", "rejected")],
        )?)?;
        let gate = &mut self.runtime_merge_gates[gate_index];
        gate.evidence_ids.retain(|id| id != evidence_id);
        gate.status = MergeGateStatus::NeedsChanges;
        gate.decision = Some(MergeGateDecision::decided_now(
            MergeGateDecisionOutcome::Rejected,
            reason.clone(),
            actor,
            gate.evidence_ids.clone(),
            audit_id,
        ));
        gate.updated_at = Some(now);
        let gate = gate.clone();

        let mut events = vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::MergeGateUpdated { gate },
        )];
        if let Some(mut task) = self
            .runtime_tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
        {
            task.evidence.retain(|id| id != evidence_id);
            task.status = AgentTaskStatus::NeedsInput;
            task.activity = format!("artifact rejected: {reason}");
            task.decision = Some(reason.clone());
            task.next_action = Some(AgentNextAction {
                label: "revise artifact".to_string(),
                command: Some(format!("/agent start {}", task.id)),
                reason: Some("merge gate rejected an artifact".to_string()),
            });
            task.updated_at = Some(now.saturating_mul(1000));
            self.upsert_agent_task(task.clone());
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::TaskUpdated { task },
            ));
        }

        Ok(events)
    }

    fn merge_agent_patch<F>(
        &mut self,
        gate_id: &str,
        actor: RuntimeOwner,
        decision: String,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(PermissionPrompt) -> ApprovalResponse,
    {
        let decision = sanitize_runtime_domain_text(&decision, 240);
        if decision.trim().is_empty() {
            return Err("patch merge decision cannot be empty".to_string());
        }
        let patch_content = self.preflight_merge_agent_patch(gate_id, &actor)?;
        let gate_index = self
            .runtime_merge_gates
            .iter()
            .position(|gate| gate.gate_id == gate_id)
            .ok_or_else(|| format!("merge gate `{gate_id}` does not exist"))?;
        let task_id = self.runtime_merge_gates[gate_index].task_id.clone();
        let dag_id = self.dag_id_for_task(&task_id)?;
        // The canonical patch bytes are already validated at this point, so
        // the operator approves the multi-file change itself rather than a
        // gate id. This is the multi-file case GUI-CORE-012 asked for.
        self.require_trust_permission_with_context(
            "merge_agent_patch",
            &format!("gate={gate_id}"),
            Some(crate::decision_context::patch_decision_context(
                &patch_content,
            )),
            approver,
        )?;
        let patch_backend = LocalPatchBackend;
        let patch_application = match patch_backend.prepare(&PatchRequest {
            cwd: self.cwd.clone(),
            unified_diff: patch_content.clone(),
        }) {
            Ok(application) => application,
            Err(err) => {
                // The apply refused before anything was written, so the same
                // canonical bytes can be re-examined read-only to publish what
                // collided (`runtime.conflict_content`).
                return self.mark_agent_patch_conflict(
                    gate_index,
                    &dag_id,
                    &task_id,
                    format!("patch conflict: {}", err),
                    Some(&patch_content),
                );
            }
        };
        self.stage_patch_rollback(&patch_application)?;
        let applied_change_id = fresh_id("change");
        let recovery_entries = self.recovery_entries_for_patch(&patch_application)?;
        let recovery_snapshot = self
            .workflows
            .write_recovery_snapshot(&applied_change_id, &recovery_entries)?;
        {
            let gate = &mut self.runtime_merge_gates[gate_index];
            gate.applied_change_id = Some(applied_change_id.clone());
            gate.recovery_snapshot = Some(recovery_snapshot);
        }
        self.persist_merge_gate_precommit(gate_index, "audit-merge-precommit")?;
        if let Err(err) = patch_backend.write_application(&patch_application) {
            self.restore_transaction_files()?;
            // No hunk was refused here: the apply resolved every one and the
            // write or the rollback failed afterwards. There is no collision
            // to show, so the bounce carries the reason alone.
            return self.mark_agent_patch_conflict(
                gate_index,
                &dag_id,
                &task_id,
                format!("patch conflict: {}", err),
                None,
            );
        }
        if let Err(err) = self.verify_patch_postimages(&patch_application) {
            self.restore_transaction_files()?;
            return self.mark_agent_patch_conflict(
                gate_index,
                &dag_id,
                &task_id,
                format!("patch postimage verification failed: {err}"),
                None,
            );
        }

        let now = now_timestamp();
        self.applied_change_rollbacks.insert(
            applied_change_id.clone(),
            self.transaction_file_rollback.borrow().clone(),
        );
        let resolved_conflict = self.mark_conflict_resolved(gate_index);
        let gate = &mut self.runtime_merge_gates[gate_index];
        gate.status = MergeGateStatus::Merged;
        let audit_id = fresh_id("audit");
        gate.decision = Some(MergeGateDecision::decided_now(
            MergeGateDecisionOutcome::Merged,
            decision.clone(),
            gate.owner.clone(),
            gate.evidence_ids.clone(),
            audit_id.clone(),
        ));
        gate.applied_change_id = Some(applied_change_id);
        gate.audit_ids.push(audit_id);
        gate.updated_at = Some(now);
        let gate = gate.clone();

        let mut events = vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::MergeGateUpdated { gate },
        )];
        if let Some(conflict) = resolved_conflict {
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::MergeConflictBounced { conflict },
            ));
        }
        if let Some(mut task) = self
            .runtime_tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
        {
            task.status = AgentTaskStatus::Applied;
            task.activity = format!("patch merged: {decision}");
            task.decision = Some(decision.clone());
            task.next_action = None;
            task.updated_at = Some(now.saturating_mul(1000));
            self.upsert_agent_task(task.clone());
            events.push(RuntimeEvent::new(
                next_sequence(&events),
                RuntimeEventKind::TaskUpdated { task },
            ));
        }

        Ok(events)
    }

    fn stage_patch_rollback(&self, application: &PatchApplication) -> Result<(), String> {
        let paths = application
            .write_paths()
            .map(Path::to_path_buf)
            .collect::<Vec<_>>();
        self.stage_rollback_paths(&paths)
    }

    fn recovery_entries_for_patch(
        &self,
        application: &PatchApplication,
    ) -> Result<Vec<RecoverySnapshotEntry>, String> {
        let rollbacks = self.transaction_file_rollback.borrow();
        let mut entries = Vec::with_capacity(rollbacks.len());
        for (path, postimage) in application.planned_postimages() {
            let rollback = rollbacks
                .iter()
                .find(|rollback| rollback.path == path)
                .ok_or_else(|| format!("missing staged rollback for `{}`", path.display()))?;
            let relative_path = path
                .strip_prefix(&rollback.root)
                .map_err(|_| format!("recovery path `{}` escaped workspace", path.display()))?
                .to_path_buf();
            let created_parent_dirs = rollback
                .created_parent_dirs
                .iter()
                .map(|parent| {
                    parent
                        .strip_prefix(&rollback.root)
                        .map(Path::to_path_buf)
                        .map_err(|_| {
                            format!("recovery parent `{}` escaped workspace", parent.display())
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            #[cfg(unix)]
            let unix_mode = {
                use std::os::unix::fs::PermissionsExt;
                rollback.permissions.as_ref().map(|mode| mode.mode())
            };
            #[cfg(not(unix))]
            let unix_mode = None;
            entries.push(RecoverySnapshotEntry {
                relative_path,
                preimage: rollback.contents.clone(),
                unix_mode,
                expected_postimage_sha256: postimage.map(runtime_sha256),
                created_parent_dirs,
            });
        }
        Ok(entries)
    }

    fn verify_patch_postimages(&self, application: &PatchApplication) -> Result<(), String> {
        for (path, expected) in application.planned_postimages() {
            let root = fs::canonicalize(&self.cwd)
                .map_err(|err| format!("{}: {err}", self.cwd.display()))?;
            verify_expected_postimage(&root, path, expected.map(runtime_sha256).as_deref())?;
        }
        Ok(())
    }

    pub(crate) fn load_recovery_rollbacks_for_gate(
        &self,
        gate_index: usize,
    ) -> Result<Vec<crate::FileRollback>, String> {
        let gate = &self.runtime_merge_gates[gate_index];
        let reference = gate.recovery_snapshot.as_ref().ok_or_else(|| {
            format!(
                "merge gate `{}` has no durable recovery snapshot",
                gate.gate_id
            )
        })?;
        let loaded = self.workflows.load_recovery_snapshot(reference)?;
        self.recovery_rollbacks_from_loaded(loaded)
    }

    fn recovery_rollbacks_from_loaded(
        &self,
        loaded: LoadedRecoverySnapshot,
    ) -> Result<Vec<crate::FileRollback>, String> {
        let root = fs::canonicalize(&self.cwd)
            .map_err(|error| format!("{}: {error}", self.cwd.display()))?;
        let mut rollbacks = Vec::with_capacity(loaded.entries.len());
        for entry in loaded.entries {
            let path = root.join(&entry.relative_path);
            ensure_transaction_path_inside_root(&root, &path)?;
            verify_expected_postimage(&root, &path, entry.expected_postimage_sha256.as_deref())?;
            #[cfg(unix)]
            let permissions = entry.unix_mode.map(|mode| {
                use std::os::unix::fs::PermissionsExt;
                fs::Permissions::from_mode(mode)
            });
            #[cfg(not(unix))]
            let permissions = None;
            rollbacks.push(crate::FileRollback {
                root: root.clone(),
                path,
                contents: entry.preimage,
                permissions,
                created_parent_dirs: entry
                    .created_parent_dirs
                    .into_iter()
                    .map(|parent| root.join(parent))
                    .collect(),
            });
        }
        Ok(rollbacks)
    }

    pub(crate) fn persist_merge_gate_precommit(
        &mut self,
        gate_index: usize,
        audit_prefix: &str,
    ) -> Result<(), String> {
        let audit_id = fresh_id(audit_prefix);
        let gate = &mut self.runtime_merge_gates[gate_index];
        gate.audit_ids.push(audit_id);
        gate.updated_at = Some(now_timestamp());
        let event = RuntimeEvent::new(1, RuntimeEventKind::MergeGateUpdated { gate: gate.clone() });
        // This write-ahead gate fact is durable before filesystem effects. If
        // the later commit fails, transaction compensation restores bytes and
        // the precommit remains an auditable marker of the interrupted intent.
        self.persist_runtime_domain_events(&[event])
    }

    pub(crate) fn stage_rollback_paths(&self, paths: &[PathBuf]) -> Result<(), String> {
        let mut rollback = self.transaction_file_rollback.borrow_mut();
        rollback.clear();
        let root =
            fs::canonicalize(&self.cwd).map_err(|err| format!("{}: {err}", self.cwd.display()))?;
        for path in paths {
            // Capture only missing parents: rollback may remove transaction-created
            // empty directories, but never a directory that predated the patch.
            ensure_transaction_path_inside_root(&root, path)?;
            let created_parent_dirs = missing_transaction_parent_dirs(&root, path)?;
            let (contents, permissions) = match fs::symlink_metadata(path) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() || !metadata.is_file() {
                        return Err(format!(
                            "unsafe transaction rollback target `{}`",
                            path.display()
                        ));
                    }
                    let existing = read_existing_private_file(&root, path)?;
                    (Some(existing.bytes), Some(existing.permissions))
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => (None, None),
                Err(err) => return Err(format!("{}: {err}", path.display())),
            };
            rollback.push(crate::FileRollback {
                root: root.clone(),
                path: path.to_path_buf(),
                contents,
                permissions,
                created_parent_dirs,
            });
        }
        Ok(())
    }

    fn restore_transaction_files(&self) -> Result<(), String> {
        self.restore_file_rollbacks(&self.transaction_file_rollback.borrow())
    }

    pub(crate) fn restore_file_rollbacks(
        &self,
        rollbacks: &[crate::FileRollback],
    ) -> Result<(), String> {
        for file in rollbacks.iter().rev() {
            ensure_transaction_path_inside_root(&file.root, &file.path)?;
            match &file.contents {
                Some(contents) => {
                    if let Some(parent) = file.path.parent() {
                        fs::create_dir_all(parent)
                            .map_err(|err| format!("{}: {err}", parent.display()))?;
                    }
                    fs::write(&file.path, contents)
                        .map_err(|err| format!("{}: {err}", file.path.display()))?;
                    if let Some(permissions) = &file.permissions {
                        fs::set_permissions(&file.path, permissions.clone())
                            .map_err(|err| format!("{}: {err}", file.path.display()))?;
                    }
                }
                None => {
                    if file.path.exists() {
                        fs::remove_file(&file.path)
                            .map_err(|err| format!("{}: {err}", file.path.display()))?;
                    }
                }
            }
            for parent in file.created_parent_dirs.iter().rev() {
                ensure_transaction_path_inside_root(&file.root, parent)?;
                match fs::remove_dir(parent) {
                    Ok(()) => {}
                    Err(err)
                        if matches!(
                            err.kind(),
                            std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                        ) => {}
                    Err(err) => return Err(format!("{}: {err}", parent.display())),
                }
            }
        }
        Ok(())
    }

    fn patch_evidence_for_gate(&self, gate_index: usize) -> Option<&EvidenceView> {
        let gate = &self.runtime_merge_gates[gate_index];
        gate.evidence_ids.iter().find_map(|id| {
            self.runtime_evidence
                .iter()
                .find(|evidence| evidence.id == *id && evidence.kind == "patch")
        })
    }

    fn verified_patch_content(&self, evidence: &EvidenceView) -> Result<String, String> {
        let canonical = evidence
            .canonical
            .as_ref()
            .ok_or_else(|| "patch evidence is not canonical".to_string())?;
        let engine =
            ContextEngine::open(&self.context_engine_root).map_err(|err| err.to_string())?;
        let verified = engine
            .verify_item(
                &canonical.item_id,
                &canonical.source_hash,
                &canonical.evidence_scope,
            )
            .map_err(|err| err.to_string())?;
        String::from_utf8(verified.content)
            .map_err(|_| "canonical patch evidence is not valid utf-8".to_string())
    }

    /// `rejected_patch` is the canonical patch text the apply refused, and is
    /// `None` for a failure with no rejected hunk behind it. Content is built
    /// only from a real refusal, so a bounce never carries invented lines.
    fn mark_agent_patch_conflict(
        &mut self,
        gate_index: usize,
        _dag_id: &str,
        task_id: &str,
        reason: String,
        rejected_patch: Option<&str>,
    ) -> Result<Vec<RuntimeEvent>, String> {
        let gate_owner = self.runtime_merge_gates[gate_index].owner.clone();
        let original_lane_id = gate_owner
            .lane_id
            .clone()
            .unwrap_or_else(|| format!("lane-{task_id}"));
        let content = rejected_patch.and_then(|patch| {
            // The same bindings the bounce will record as its baseline. A gate
            // whose evidence no longer validates still publishes its lines,
            // with a revision or `Unknown` baseline instead of a false one.
            let baseline_evidence = self
                .validate_conflict_bounce(gate_index, &original_lane_id, &gate_owner)
                .unwrap_or_default();
            crate::conflict_content::merge_conflict_content(&self.cwd, patch, &baseline_evidence)
        });
        self.record_conflict_bounce(gate_index, original_lane_id, gate_owner, reason, content)
    }

    fn dag_id_for_task(&self, task_id: &str) -> Result<String, String> {
        self.runtime_agent_dags
            .iter()
            .find(|dag| dag.tasks.iter().any(|task| task.task_id == task_id))
            .map(|dag| dag.dag_id.clone())
            .ok_or_else(|| format!("agent DAG for task `{task_id}` does not exist"))
    }

    #[allow(dead_code)]
    fn persist_agent_event(
        &self,
        dag_id: &str,
        task_id: Option<&str>,
        event_type: &str,
        payload_fields: &[(&str, &str)],
    ) -> Result<(), String> {
        let payload = sanitized_agent_event_payload(payload_fields)?;
        let event = WorkflowAgentEvent {
            event_id: fresh_id("agent_evt"),
            dag_id: dag_id.to_string(),
            task_id: task_id.map(ToString::to_string),
            event_type: event_type.to_string(),
            timestamp: now_timestamp(),
            origin_session_id: Some(self.session_id().to_string()),
            payload,
        };
        if self.should_fail_workflow_append_for_test() {
            return Err("injected workflow append failure".to_string());
        }
        self.workflows.append_agent_event(&event)
    }
}

#[allow(dead_code)]
fn sanitized_agent_event_payload(
    payload_fields: &[(&str, &str)],
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut payload = std::collections::BTreeMap::new();
    for (key, value) in payload_fields {
        let sanitized = match *key {
            "id" | "dag_id" | "task_id" | "gate_id" | "evidence_id" | "bundle_id" | "item_id"
            | "content_sha256" | "source_hash" | "status" | "kind" | "role" | "count"
            | "event_count" | "schema_version" | "batch_id" | "command_id" => {
                sanitize_identifier_payload_value(value)
            }
            "goal" | "title" | "summary" | "decision" | "reason" | "source" | "command"
            | "error" | "changed_files" | "path" => {
                let sanitized = sanitize_sensitive_payload_text(value, 160);
                if sanitized.contains("[REDACTED]") {
                    return Err(format!(
                        "workflow agent event payload `{key}` is not allowed"
                    ));
                }
                sanitized
            }
            _ => {
                return Err(format!(
                    "workflow agent event payload key `{key}` is not allowed"
                ));
            }
        };
        payload.insert((*key).to_string(), sanitized);
    }
    Ok(payload)
}

#[allow(dead_code)]
fn sanitize_identifier_payload_value(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':' | '.'))
        .take(128)
        .collect()
}

#[allow(dead_code)]
fn sanitize_sensitive_payload_text(value: &str, max_chars: usize) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("sk-")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("secret")
        || lower.contains("token=")
        || value.starts_with('/')
        || value.contains("..")
        || value.chars().any(char::is_control)
        || value.contains("diff --git")
    {
        return "[REDACTED]".to_string();
    }
    truncate_for_preview(value, max_chars)
}

fn agent_task_prompt(spec: &AgentDagTaskSpec) -> String {
    format!(
        "You are the {role} agent for a supervised Viden Agent DAG task.\n\
         Task: {title}\n\
         Objective: {objective}\n\
         File scope: {scope}\n\
         Permission policy: {permission}\n\
         Required evidence: {evidence}\n\
         Return concise output that can be recorded as {role} evidence.",
        role = spec.role.as_str(),
        title = spec.title,
        objective = spec.objective,
        scope = if spec.file_scope.is_empty() {
            "<none>".to_string()
        } else {
            spec.file_scope.join(", ")
        },
        permission = spec.permission_policy,
        evidence = if spec.required_evidence.is_empty() {
            "<none>".to_string()
        } else {
            spec.required_evidence.join(", ")
        }
    )
}

fn assistant_output_from_events(events: &[RuntimeEvent]) -> String {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::AssistantDelta { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn role_guidance_context_source(role: AgentRole) -> ContextSourceRecord {
    let (name, summary) = match role {
        AgentRole::Planner => (
            "role-planning-context",
            "Focus on requirements, architecture boundaries, sequencing, risks, and explicit non-goals before implementation.",
        ),
        AgentRole::Coder => (
            "role-implementation-context",
            "Focus on scoped files, existing local patterns, minimal implementation changes, and compile/test feedback.",
        ),
        AgentRole::Reviewer => (
            "role-review-context",
            "Focus on behavioral regressions, permission violations, missing tests, and evidence quality before acceptance.",
        ),
        AgentRole::Tester => (
            "role-verification-context",
            "Focus on focused checks, full gates, failure classification, reproducible logs, and release evidence.",
        ),
        AgentRole::DocWriter => (
            "role-documentation-context",
            "Focus on user-visible behavior, architecture contracts, bilingual docs, and keeping roadmap status current.",
        ),
        AgentRole::Researcher => (
            "role-research-context",
            "Focus on verified sources, evidence boundaries, uncertainty, and concise findings for downstream decisions.",
        ),
        AgentRole::ReleaseOperator => (
            "role-release-context",
            "Focus on release gates, artifacts, version consistency, Homebrew sync, and post-publish validation.",
        ),
    };
    ContextSourceRecord {
        name: name.to_string(),
        kind: "role-guidance".to_string(),
        priority: 98,
        estimated_tokens: 220,
        summary: summary.to_string(),
        include_reason: format!(
            "{} role requires specialized context selection",
            role.as_str()
        ),
        handle_id: None,
        item_id: None,
        view_id: None,
        content_sha256: None,
        view_sha256: None,
        quality_id: None,
    }
}

fn agent_file_scope_context_source(spec: &AgentDagTaskSpec) -> ContextSourceRecord {
    ContextSourceRecord {
        name: "agent-file-scope".to_string(),
        kind: "file-scope".to_string(),
        priority: 94,
        estimated_tokens: 180,
        summary: spec.file_scope.join(", "),
        include_reason: "AgentTask file_scope limits which project areas this role should inspect"
            .to_string(),
        handle_id: None,
        item_id: None,
        view_id: None,
        content_sha256: None,
        view_sha256: None,
        quality_id: None,
    }
}

fn agent_evidence_contract_context_source(spec: &AgentDagTaskSpec) -> ContextSourceRecord {
    ContextSourceRecord {
        name: "agent-evidence-contract".to_string(),
        kind: "evidence-contract".to_string(),
        priority: 92,
        estimated_tokens: 160,
        summary: spec.required_evidence.join(", "),
        include_reason: "required_evidence defines the output contract for the merge gate"
            .to_string(),
        handle_id: None,
        item_id: None,
        view_id: None,
        content_sha256: None,
        view_sha256: None,
        quality_id: None,
    }
}

fn agent_selected_files_context_source(
    cwd: &Path,
    spec: &AgentDagTaskSpec,
) -> Option<ContextSourceRecord> {
    let selected_files = select_role_specific_files(cwd, spec);
    if selected_files.is_empty() {
        return None;
    }
    let estimated_tokens = selected_files.len().saturating_mul(48).min(640) as u64;
    Some(ContextSourceRecord {
        name: "role-selected-files".to_string(),
        kind: "selected-files".to_string(),
        priority: 96,
        estimated_tokens,
        summary: selected_files.join("\n"),
        include_reason: format!(
            "{} role gets deterministic file candidates from AgentTask file_scope",
            spec.role.as_str()
        ),
        handle_id: None,
        item_id: None,
        view_id: None,
        content_sha256: None,
        view_sha256: None,
        quality_id: None,
    })
}

fn agent_selected_symbols_context_source(
    cwd: &Path,
    spec: &AgentDagTaskSpec,
) -> Option<ContextSourceRecord> {
    let selected_symbols = select_role_specific_symbols(cwd, spec);
    if selected_symbols.is_empty() {
        return None;
    }
    let estimated_tokens = selected_symbols.len().saturating_mul(32).min(640) as u64;
    Some(ContextSourceRecord {
        name: "role-selected-symbols".to_string(),
        kind: "selected-symbols".to_string(),
        priority: 95,
        estimated_tokens,
        summary: selected_symbols.join("\n"),
        include_reason: format!(
            "{} role gets bounded symbol candidates from selected source files",
            spec.role.as_str()
        ),
        handle_id: None,
        item_id: None,
        view_id: None,
        content_sha256: None,
        view_sha256: None,
        quality_id: None,
    })
}

fn select_role_specific_files(cwd: &Path, spec: &AgentDagTaskSpec) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut candidates = Vec::new();
    for scope in &spec.file_scope {
        let scoped_path = cwd.join(scope);
        collect_role_file_candidates(cwd, &scoped_path, 0, &mut seen, &mut candidates);
        if candidates.len() >= 512 {
            break;
        }
    }
    candidates.sort_by(|left, right| {
        role_file_score(spec.role, right)
            .cmp(&role_file_score(spec.role, left))
            .then_with(|| left.cmp(right))
    });
    candidates.truncate(8);
    candidates
}

fn select_role_specific_symbols(cwd: &Path, spec: &AgentDagTaskSpec) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut candidates = Vec::new();
    for file in select_role_specific_files(cwd, spec) {
        let path = cwd.join(&file);
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        for symbol in extract_context_symbols_from_file(&file, &contents) {
            if seen.insert(symbol.clone()) {
                candidates.push(symbol);
            }
            if candidates.len() >= 128 {
                break;
            }
        }
        if candidates.len() >= 128 {
            break;
        }
    }
    candidates.sort_by(|left, right| {
        role_symbol_score(spec.role, right)
            .cmp(&role_symbol_score(spec.role, left))
            .then_with(|| left.cmp(right))
    });
    candidates.truncate(12);
    candidates
}

fn extract_context_symbols_from_file(relative_file: &str, contents: &str) -> Vec<String> {
    let mut symbols = Vec::new();
    for raw_line in contents.lines().take(2_000) {
        let line = raw_line.trim();
        for (prefix, kind) in [
            ("pub async fn ", "fn"),
            ("async fn ", "fn"),
            ("pub fn ", "fn"),
            ("fn ", "fn"),
            ("pub struct ", "struct"),
            ("struct ", "struct"),
            ("pub enum ", "enum"),
            ("enum ", "enum"),
            ("pub trait ", "trait"),
            ("trait ", "trait"),
            ("impl ", "impl"),
        ] {
            if let Some(name) = parse_symbol_name(line, prefix) {
                symbols.push(format!("{relative_file}::{kind} {name}"));
                break;
            }
        }
    }
    symbols
}

fn parse_symbol_name(line: &str, prefix: &str) -> Option<String> {
    let rest = line.strip_prefix(prefix)?;
    let name = rest
        .split(|character: char| {
            character == '('
                || character == '<'
                || character == '{'
                || character == ':'
                || character == ';'
                || character.is_whitespace()
        })
        .next()?
        .trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn role_symbol_score(role: AgentRole, symbol: &str) -> u8 {
    let lower = symbol.to_ascii_lowercase();
    let is_test_file =
        lower.contains("/tests/") || lower.contains("_test.") || lower.contains(".test.");
    let is_test_symbol = lower.contains("::fn test")
        || lower.contains("::fn should_")
        || lower.contains("::fn runtime_supervisor_")
        || lower.contains("::fn provider_")
        || lower.contains("::fn workflow_");
    let is_type = lower.contains("::struct ")
        || lower.contains("::enum ")
        || lower.contains("::trait ")
        || lower.contains("::impl ");
    match role {
        AgentRole::Tester => {
            if is_test_file && is_test_symbol {
                98
            } else if is_test_file {
                88
            } else if is_test_symbol {
                82
            } else {
                40
            }
        }
        AgentRole::Coder => {
            if !is_test_file && is_type {
                94
            } else if !is_test_file {
                88
            } else {
                45
            }
        }
        AgentRole::Reviewer => {
            if is_type || is_test_symbol {
                88
            } else {
                70
            }
        }
        AgentRole::Planner
        | AgentRole::Researcher
        | AgentRole::DocWriter
        | AgentRole::ReleaseOperator => {
            if is_type {
                78
            } else {
                55
            }
        }
    }
}

fn is_lsp_context_candidate(file: &str) -> bool {
    let lower = file.to_ascii_lowercase();
    lower.ends_with(".rs")
        || lower.ends_with(".ts")
        || lower.ends_with(".tsx")
        || lower.ends_with(".js")
        || lower.ends_with(".jsx")
        || lower.ends_with(".py")
        || lower.ends_with(".go")
}

fn collect_role_file_candidates(
    cwd: &Path,
    path: &Path,
    depth: usize,
    seen: &mut BTreeSet<String>,
    candidates: &mut Vec<String>,
) {
    if candidates.len() >= 512 || depth > 5 {
        return;
    }
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.is_file() {
        if let Some(relative) = normalized_relative_file(cwd, path)
            && seen.insert(relative.clone())
        {
            candidates.push(relative);
        }
        return;
    }
    if !metadata.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let entry_path = entry.path();
        if should_skip_context_path(&entry_path) {
            continue;
        }
        collect_role_file_candidates(cwd, &entry_path, depth + 1, seen, candidates);
        if candidates.len() >= 512 {
            break;
        }
    }
}

fn normalized_relative_file(cwd: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(cwd).ok()?;
    let normalized = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if normalized.is_empty() || should_skip_context_path(Path::new(&normalized)) {
        None
    } else {
        Some(normalized)
    }
}

fn should_skip_context_path(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        matches!(
            name.as_ref(),
            ".git"
                | ".viden"
                | ".worktrees"
                | ".ref"
                | "target"
                | "node_modules"
                | "dist"
                | "build"
        ) || (name.starts_with('.') && name != ".github")
    })
}

fn role_file_score(role: AgentRole, file: &str) -> u8 {
    let lower = file.to_ascii_lowercase();
    let is_doc = lower.ends_with(".md")
        || lower.contains("/docs/")
        || lower.ends_with("readme")
        || lower.contains("changelog");
    let is_test = lower.contains("/tests/")
        || lower.contains("_test.")
        || lower.contains(".test.")
        || lower.contains("_spec.")
        || lower.contains(".spec.");
    let is_source = lower.ends_with(".rs")
        || lower.ends_with(".ts")
        || lower.ends_with(".tsx")
        || lower.ends_with(".js")
        || lower.ends_with(".jsx")
        || lower.ends_with(".py")
        || lower.ends_with(".go");
    let is_manifest = lower.ends_with("cargo.toml")
        || lower.ends_with("package.json")
        || lower.ends_with("pyproject.toml")
        || lower.ends_with("makefile");

    match role {
        AgentRole::Planner => {
            if is_doc {
                90
            } else if is_manifest {
                78
            } else {
                40
            }
        }
        AgentRole::Coder => {
            if is_source && !is_test {
                95
            } else if is_manifest {
                75
            } else if is_test {
                65
            } else {
                35
            }
        }
        AgentRole::Reviewer => {
            if is_source || is_test {
                88
            } else if is_doc || is_manifest {
                76
            } else {
                35
            }
        }
        AgentRole::Tester => {
            if is_test {
                96
            } else if is_manifest {
                82
            } else if is_source {
                62
            } else {
                30
            }
        }
        AgentRole::DocWriter => {
            if is_doc {
                96
            } else if is_manifest {
                48
            } else {
                25
            }
        }
        AgentRole::Researcher => {
            if is_doc || is_source || is_manifest {
                82
            } else {
                40
            }
        }
        AgentRole::ReleaseOperator => {
            if lower.contains("release") || lower.contains("changelog") {
                96
            } else if is_manifest || is_doc {
                78
            } else {
                30
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
struct MergeGateValidationFacts {
    context_engine_root: PathBuf,
    context_bundles: Vec<viden_types::ContextBundleSummaryRecord>,
}

fn reduce_merge_gate_status(
    gate: &MergeGateRecord,
    evidence: &[EvidenceView],
    facts: &MergeGateValidationFacts,
) -> EvidenceCanonicalStatusReport {
    let mut status = EvidenceCanonicalStatus::Verified;
    let mut reasons = BTreeSet::new();
    let mut seen_valid_kinds = BTreeSet::new();
    let evidence_by_id = evidence_by_id(evidence);
    let required_kinds = gate
        .required_evidence
        .iter()
        .map(|required| canonical_required_evidence_kind(required))
        .collect::<BTreeSet<_>>();
    if required_kinds.is_empty() {
        return EvidenceCanonicalStatusReport {
            status: EvidenceCanonicalStatus::Missing,
            reason_codes: vec![EvidenceCanonicalReasonCode::MissingRequiredKind],
        };
    }

    for evidence_id in &gate.evidence_ids {
        let Some(item) = evidence_by_id.get(evidence_id.as_str()) else {
            continue;
        };
        if !required_kinds.contains(&canonical_required_evidence_kind(&item.kind)) {
            continue;
        }
        let report = validate_canonical_evidence_for_gate(gate, item, facts);
        merge_report_status(&mut status, &mut reasons, &report);
        if report.status == EvidenceCanonicalStatus::Verified {
            seen_valid_kinds.insert(canonical_required_evidence_kind(&item.kind));
        }
    }

    for required in &required_kinds {
        if !seen_valid_kinds.contains(required) {
            status = merge_status(status, EvidenceCanonicalStatus::Missing);
            reasons.insert(EvidenceCanonicalReasonCode::MissingRequiredKind);
        }
    }

    EvidenceCanonicalStatusReport {
        status,
        reason_codes: reasons.into_iter().collect(),
    }
}

fn ensure_transaction_path_inside_root(root: &Path, path: &Path) -> Result<(), String> {
    // Revalidate at both staging and restore time so a symlink swap cannot
    // redirect compensating writes or deletes outside the project root.
    let current_root =
        fs::canonicalize(root).map_err(|err| format!("{}: {err}", root.display()))?;
    if current_root != root || !current_root.is_dir() {
        return Err(format!(
            "transaction rollback root is no longer safe: `{}`",
            root.display()
        ));
    }
    let relative = path.strip_prefix(root).map_err(|_| {
        format!(
            "transaction rollback target `{}` is outside `{}`",
            path.display(),
            root.display()
        )
    })?;
    if relative.as_os_str().is_empty() {
        return Err("transaction rollback target cannot be the workspace root".to_string());
    }

    let mut current = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(segment) = component else {
            return Err(format!(
                "unsafe transaction rollback target `{}`",
                path.display()
            ));
        };
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "transaction rollback target crosses symlink `{}`",
                    current.display()
                ));
            }
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(format!("{}: {err}", current.display())),
        }
    }
    Ok(())
}

fn missing_transaction_parent_dirs(root: &Path, path: &Path) -> Result<Vec<PathBuf>, String> {
    let parent = path.parent().ok_or_else(|| {
        format!(
            "transaction rollback target has no parent: `{}`",
            path.display()
        )
    })?;
    let relative = parent.strip_prefix(root).map_err(|_| {
        format!(
            "transaction rollback parent `{}` is outside `{}`",
            parent.display(),
            root.display()
        )
    })?;
    let mut current = root.to_path_buf();
    let mut missing = Vec::new();
    for component in relative.components() {
        let std::path::Component::Normal(segment) = component else {
            return Err(format!(
                "unsafe transaction rollback parent `{}`",
                parent.display()
            ));
        };
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(format!(
                    "unsafe transaction rollback parent `{}`",
                    current.display()
                ));
            }
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                missing.push(current.clone());
            }
            Err(err) => return Err(format!("{}: {err}", current.display())),
        }
    }
    Ok(missing)
}

fn transactional_runtime_command(command: &RuntimeCommand) -> bool {
    matches!(
        command,
        RuntimeCommand::ConfirmProjectConfig { .. }
            | RuntimeCommand::StoreCredentialHandle { .. }
            | RuntimeCommand::StartAgentDag { .. }
            | RuntimeCommand::StartAgentTask { .. }
            | RuntimeCommand::CancelAgentTask { .. }
            | RuntimeCommand::AcceptMergeGate { .. }
            | RuntimeCommand::RejectMergeGate { .. }
            | RuntimeCommand::RecordAgentEvidence { .. }
            | RuntimeCommand::AcceptAgentArtifact { .. }
            | RuntimeCommand::RejectAgentArtifact { .. }
            | RuntimeCommand::MergeAgentPatch { .. }
            | RuntimeCommand::RevalidateMergeConflict { .. }
            | RuntimeCommand::CreateHandoff { .. }
            | RuntimeCommand::RequestReview { .. }
            | RuntimeCommand::DecideReview { .. }
            | RuntimeCommand::ConfirmContract { .. }
            | RuntimeCommand::SetDependency { .. }
            | RuntimeCommand::BounceMergeConflict { .. }
            | RuntimeCommand::RevertAppliedChange { .. }
    )
}

fn runtime_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

struct ExistingPrivateFile {
    bytes: Vec<u8>,
    permissions: fs::Permissions,
}

fn read_existing_private_file(root: &Path, path: &Path) -> Result<ExistingPrivateFile, String> {
    ensure_transaction_path_inside_root(root, path)?;
    #[cfg(unix)]
    {
        read_existing_private_file_openat(root, path)
    }
    #[cfg(not(unix))]
    {
        read_existing_private_file_by_path(path)
    }
}

#[cfg(not(unix))]
fn read_existing_private_file_by_path(path: &Path) -> Result<ExistingPrivateFile, String> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    let mut file = options
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    read_open_private_file(path, &mut file)
}

#[cfg(unix)]
fn read_existing_private_file_openat(
    root: &Path,
    path: &Path,
) -> Result<ExistingPrivateFile, String> {
    let relative = path.strip_prefix(root).map_err(|_| {
        format!(
            "transaction rollback target `{}` is outside `{}`",
            path.display(),
            root.display()
        )
    })?;
    let mut components = relative.components().peekable();
    let root_c = cstring_for_open(root.as_os_str(), root)?;
    let root_fd = unsafe {
        libc::open(
            root_c.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if root_fd < 0 {
        return Err(format!(
            "{}: {}",
            root.display(),
            std::io::Error::last_os_error()
        ));
    }
    let mut current = unsafe { File::from_raw_fd(root_fd) };
    while let Some(component) = components.next() {
        let std::path::Component::Normal(segment) = component else {
            return Err(format!(
                "unsafe transaction rollback target `{}`",
                path.display()
            ));
        };
        let name = cstring_for_open(segment, path)?;
        let final_component = components.peek().is_none();
        let flags = if final_component {
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW
        } else {
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW
        };
        let fd = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(format!(
                "{}: {}",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }
        let mut opened = unsafe { File::from_raw_fd(fd) };
        if final_component {
            return read_open_private_file(path, &mut opened);
        }
        let metadata = opened
            .metadata()
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if !metadata.file_type().is_dir() {
            return Err(format!(
                "transaction rollback target crosses non-directory `{}`",
                path.display()
            ));
        }
        current = opened;
    }
    Err("transaction rollback target cannot be the workspace root".to_string())
}

#[cfg(unix)]
fn cstring_for_open(value: &std::ffi::OsStr, display_path: &Path) -> Result<CString, String> {
    CString::new(value.as_bytes()).map_err(|_| {
        format!(
            "transaction rollback target `{}` contains a NUL byte",
            display_path.display()
        )
    })
}

fn read_open_private_file(path: &Path, file: &mut File) -> Result<ExistingPrivateFile, String> {
    let metadata = file
        .metadata()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!(
            "unsafe transaction rollback target `{}`",
            path.display()
        ));
    }
    #[cfg(unix)]
    {
        // The mode is captured from the same descriptor that supplies bytes;
        // this prevents check-then-reopen swaps from changing rollback facts.
        let _mode = metadata.permissions().mode();
    }
    let size = usize::try_from(metadata.len())
        .map_err(|_| format!("{} is too large to snapshot safely", path.display()))?;
    let mut bytes = Vec::with_capacity(size);
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(ExistingPrivateFile {
        bytes,
        permissions: metadata.permissions(),
    })
}

fn verify_expected_postimage(
    root: &Path,
    path: &Path,
    expected_sha256: Option<&str>,
) -> Result<(), String> {
    match expected_sha256 {
        Some(expected) => {
            let existing = read_existing_private_file(root, path).map_err(|error| {
                format!("postimage `{}` is unavailable: {error}", path.display())
            })?;
            let actual = runtime_sha256(&existing.bytes);
            if actual != expected {
                return Err(format!(
                    "postimage `{}` changed after merge",
                    path.display()
                ));
            }
        }
        None => match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(format!(
                    "postimage `{}` was expected to be absent",
                    path.display()
                ));
            }
            Err(error) => return Err(format!("{}: {error}", path.display())),
        },
    }
    Ok(())
}

fn evidence_by_id(evidence: &[EvidenceView]) -> std::collections::BTreeMap<&str, &EvidenceView> {
    let mut by_id = std::collections::BTreeMap::new();
    for item in evidence {
        by_id.entry(item.id.as_str()).or_insert(item);
    }
    by_id
}

fn validate_canonical_evidence_for_gate(
    gate: &MergeGateRecord,
    evidence: &EvidenceView,
    facts: &MergeGateValidationFacts,
) -> EvidenceCanonicalStatusReport {
    let mut status = canonical_evidence_status(evidence);
    let mut reasons = BTreeSet::new();
    let Some(canonical) = &evidence.canonical else {
        reasons.insert(EvidenceCanonicalReasonCode::MissingCanonical);
        return EvidenceCanonicalStatusReport {
            status,
            reason_codes: reasons.into_iter().collect(),
        };
    };

    let verified_item = verify_canonical_context_item(facts, canonical);
    let bundle = facts
        .context_bundles
        .iter()
        .find(|bundle| bundle.bundle_id == canonical.bundle_id);
    match &verified_item {
        Ok(_) => {}
        Err(reason) => {
            status = merge_status(status, EvidenceCanonicalStatus::Blocked);
            reasons.insert(*reason);
        }
    }
    if bundle.is_none() {
        status = merge_status(status, EvidenceCanonicalStatus::Blocked);
        reasons.insert(EvidenceCanonicalReasonCode::MissingSource);
    }

    if !scope_matches_gate(&canonical.evidence_scope, gate)
        || verified_item
            .as_ref()
            .is_ok_and(|item| item.scope != canonical.evidence_scope)
    {
        status = merge_status(status, EvidenceCanonicalStatus::Blocked);
        reasons.insert(EvidenceCanonicalReasonCode::ScopeMismatch);
    }
    if canonical
        .permission_snapshot_id
        .as_deref()
        .unwrap_or("")
        .is_empty()
    {
        status = merge_status(status, EvidenceCanonicalStatus::Blocked);
        reasons.insert(EvidenceCanonicalReasonCode::MissingPermissionSnapshot);
    } else if canonical.permission_scope != canonical.evidence_scope
        || !scope_matches_gate(&canonical.permission_scope, gate)
    {
        status = merge_status(status, EvidenceCanonicalStatus::Blocked);
        reasons.insert(EvidenceCanonicalReasonCode::InvalidPermissionSnapshot);
    }
    if canonical.producer.identity.trim().is_empty()
        || canonical.producer.role.trim().is_empty()
        || canonical.producer.task_id != gate.task_id
    {
        status = merge_status(status, EvidenceCanonicalStatus::Blocked);
        reasons.insert(EvidenceCanonicalReasonCode::MissingProducer);
    }
    if canonical.quality.status == EvidenceQualityStatus::Fail {
        status = merge_status(status, EvidenceCanonicalStatus::NeedsChanges);
        reasons.insert(EvidenceCanonicalReasonCode::QualityFailed);
    }
    if canonical.verification == EvidenceVerificationState::Failed {
        status = merge_status(status, EvidenceCanonicalStatus::NeedsChanges);
        reasons.insert(EvidenceCanonicalReasonCode::VerificationFailed);
    }
    for reason in &canonical.quality.reason_codes {
        reasons.insert(*reason);
    }

    EvidenceCanonicalStatusReport {
        status,
        reason_codes: reasons.into_iter().collect(),
    }
}

fn verify_canonical_context_item(
    facts: &MergeGateValidationFacts,
    canonical: &CanonicalEvidenceReference,
) -> Result<ContextItemRecord, EvidenceCanonicalReasonCode> {
    // Merge gates must verify the stored blob, not view-state metadata, so
    // replay and live decisions fail identically on missing or tampered bytes.
    let engine = ContextEngine::open(&facts.context_engine_root)
        .map_err(|_| EvidenceCanonicalReasonCode::MissingSource)?;
    engine
        .verify_item(
            &canonical.item_id,
            &canonical.source_hash,
            &canonical.evidence_scope,
        )
        .map(|verified| verified.item)
        .map_err(canonical_reason_from_context_error)
}

fn canonical_reason_from_context_error(err: EngineContextError) -> EvidenceCanonicalReasonCode {
    match err {
        EngineContextError::Store(StoreContextError::HashMismatch { .. }) => {
            EvidenceCanonicalReasonCode::HashMismatch
        }
        EngineContextError::Store(StoreContextError::ScopeDenied { .. }) => {
            EvidenceCanonicalReasonCode::ScopeMismatch
        }
        EngineContextError::Store(
            StoreContextError::MissingBlob { .. } | StoreContextError::MissingHandle { .. },
        ) => EvidenceCanonicalReasonCode::MissingSource,
        _ => EvidenceCanonicalReasonCode::MissingSource,
    }
}

fn merge_report_status(
    status: &mut EvidenceCanonicalStatus,
    reasons: &mut BTreeSet<EvidenceCanonicalReasonCode>,
    report: &EvidenceCanonicalStatusReport,
) {
    *status = merge_status(*status, report.status);
    reasons.extend(report.reason_codes.iter().copied());
}

fn merge_status(
    current: EvidenceCanonicalStatus,
    next: EvidenceCanonicalStatus,
) -> EvidenceCanonicalStatus {
    use EvidenceCanonicalStatus::*;
    match (current, next) {
        (NeedsChanges, _) | (_, NeedsChanges) => NeedsChanges,
        (Blocked, _) | (_, Blocked) => Blocked,
        (Missing, _) | (_, Missing) => Missing,
        _ => Verified,
    }
}

fn canonical_reason_summary(report: &EvidenceCanonicalStatusReport) -> Option<String> {
    if report.reason_codes.is_empty() {
        return None;
    }
    Some(
        report
            .reason_codes
            .iter()
            .map(canonical_reason_code)
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn canonical_reason_code(reason: &EvidenceCanonicalReasonCode) -> &'static str {
    match reason {
        EvidenceCanonicalReasonCode::MissingCanonical => "missing_canonical",
        EvidenceCanonicalReasonCode::MissingRequiredKind => "missing_required_kind",
        EvidenceCanonicalReasonCode::MissingSource => "missing_source",
        EvidenceCanonicalReasonCode::HashMismatch => "hash_mismatch",
        EvidenceCanonicalReasonCode::ScopeMismatch => "scope_mismatch",
        EvidenceCanonicalReasonCode::MissingPermissionSnapshot => "missing_permission_snapshot",
        EvidenceCanonicalReasonCode::InvalidPermissionSnapshot => "invalid_permission_snapshot",
        EvidenceCanonicalReasonCode::MissingProducer => "missing_producer",
        EvidenceCanonicalReasonCode::QualityFailed => "quality_failed",
        EvidenceCanonicalReasonCode::VerificationFailed => "verification_failed",
    }
}

fn merge_gate_status_from_canonical(status: EvidenceCanonicalStatus) -> MergeGateStatus {
    match status {
        EvidenceCanonicalStatus::Verified => MergeGateStatus::Accepted,
        EvidenceCanonicalStatus::Missing => MergeGateStatus::CollectingEvidence,
        EvidenceCanonicalStatus::Blocked => MergeGateStatus::Blocked,
        EvidenceCanonicalStatus::NeedsChanges => MergeGateStatus::NeedsChanges,
    }
}

fn scope_matches_gate(scope: &ContextScope, gate: &MergeGateRecord) -> bool {
    matches!(scope, ContextScope::Task(task_id) if task_id == &gate.task_id)
}

fn canonical_required_evidence_kind(kind: &str) -> String {
    match normalize_evidence_kind(kind).as_str() {
        "test" | "tests" | "test_result" => "test".to_string(),
        "doc" | "docs" | "doc_update" => "doc".to_string(),
        "release" | "release_artifact" => "release".to_string(),
        other => other.to_string(),
    }
}

fn normalize_evidence_kind(kind: &str) -> String {
    kind.trim().replace('-', "_").to_ascii_lowercase()
}

struct AgentTaskFailureClassification {
    class: &'static str,
    recovery_suggestion: &'static str,
}

fn classify_agent_task_failure(error: &str) -> AgentTaskFailureClassification {
    let lower = error.to_ascii_lowercase();
    if lower.contains("api error (413)")
        || lower.contains("http 413")
        || lower.contains("payload too large")
        || lower.contains("request entity too large")
    {
        AgentTaskFailureClassification {
            class: "request_too_large",
            recovery_suggestion: "compact provider context, retry with a smaller prompt, or switch provider",
        }
    } else if lower.contains("api key")
        || lower.contains("missing key")
        || lower.contains("key=missing")
        || lower.contains("no api key")
    {
        AgentTaskFailureClassification {
            class: "missing_key",
            recovery_suggestion: "open provider config, export the listed key env var, then retry",
        }
    } else if lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("permission")
        || lower.contains("api error (401)")
        || lower.contains("api error (403)")
    {
        AgentTaskFailureClassification {
            class: "auth",
            recovery_suggestion: "run provider doctor, verify key scope, or switch to fallback",
        }
    } else if lower.contains("rate limit") || lower.contains("too many requests") {
        AgentTaskFailureClassification {
            class: "rate_limit",
            recovery_suggestion: "retry later, switch model/provider, or use fallback",
        }
    } else if lower.contains("timeout") || lower.contains("timed out") {
        AgentTaskFailureClassification {
            class: "timeout",
            recovery_suggestion: "retry, increase request timeout, or switch provider",
        }
    } else if lower.contains("context_length")
        || lower.contains("context_overflow")
        || lower.contains("maximum context")
        || lower.contains("context overflow")
    {
        AgentTaskFailureClassification {
            class: "context_overflow",
            recovery_suggestion: "compact context or switch to a larger-context model",
        }
    } else if lower.contains("tool_calls")
        || lower.contains("tool call")
        || lower.contains("tool_choice")
        || lower.contains("unsupported")
    {
        AgentTaskFailureClassification {
            class: "compatibility",
            recovery_suggestion: "switch to a known-compatible model or inspect provider doctor",
        }
    } else if lower.contains("model")
        || lower.contains("not found")
        || lower.contains("does not exist")
        || lower.contains("unavailable")
        || lower.contains("api error (400)")
        || lower.contains("api error (404)")
    {
        AgentTaskFailureClassification {
            class: "model_unavailable",
            recovery_suggestion: "open /models and switch to a known model candidate",
        }
    } else {
        AgentTaskFailureClassification {
            class: "provider_error",
            recovery_suggestion: "inspect provider status, retry, or switch provider",
        }
    }
}

fn validate_agent_dag_tasks(tasks: &[AgentDagTaskSpec]) -> Result<(), String> {
    let ids = tasks
        .iter()
        .map(|task| task.task_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if ids.len() != tasks.len() {
        return Err("agent DAG task ids must be unique".to_string());
    }
    for task in tasks {
        for dependency in &task.dependencies {
            if !ids.contains(dependency.as_str()) {
                return Err(format!(
                    "agent task `{}` depends on missing task `{dependency}`",
                    task.task_id
                ));
            }
        }
    }
    let edges = tasks
        .iter()
        .map(|task| (task.task_id.as_str(), task.dependencies.as_slice()))
        .collect::<std::collections::BTreeMap<_, _>>();
    for task in tasks {
        for dependency in &task.dependencies {
            if dependency_path_exists_in_specs(&edges, dependency, &task.task_id) {
                return Err(format!(
                    "agent DAG dependency cycle connects `{}` and `{dependency}`",
                    task.task_id
                ));
            }
        }
    }
    Ok(())
}

fn dependency_path_exists_in_specs<'a>(
    edges: &std::collections::BTreeMap<&'a str, &'a [String]>,
    start: &'a str,
    target: &str,
) -> bool {
    let mut pending = vec![start];
    let mut visited = std::collections::BTreeSet::new();
    while let Some(node) = pending.pop() {
        if node == target {
            return true;
        }
        if !visited.insert(node) {
            continue;
        }
        if let Some(dependencies) = edges.get(node) {
            pending.extend(dependencies.iter().map(String::as_str));
        }
    }
    false
}

fn agent_task_record_from_spec(
    engine: &SessionEngine,
    spec: &AgentDagTaskSpec,
    timestamp: u64,
) -> AgentTaskRecord {
    let now = timestamp.saturating_mul(1000);
    AgentTaskRecord {
        id: spec.task_id.clone(),
        parent_id: spec.dependencies.first().cloned(),
        role: spec.role,
        kind: AgentTaskKind::Agent,
        route: AgentRoute::BuiltIn,
        title: spec.title.clone(),
        status: AgentTaskStatus::Queued,
        activity: "queued for supervised execution".to_string(),
        summary: spec.objective.clone(),
        progress: 0,
        started_at: None,
        updated_at: Some(now),
        workspace: spec
            .workspace
            .clone()
            .or_else(|| Some(engine.cwd().to_string_lossy().to_string())),
        evidence: spec
            .required_evidence
            .iter()
            .map(|kind| format!("required {kind}"))
            .collect(),
        permissions: vec![format!("policy {}", spec.permission_policy)],
        decision: None,
        result: None,
        resume_handle: None,
        pid: None,
        next_action: Some(AgentNextAction {
            label: role_next_action_label(spec.role).to_string(),
            command: Some(format!("/agent start {}", spec.task_id)),
            reason: Some("task is queued in the supervised Agent DAG".to_string()),
        }),
        // A DAG spec binds no Lane, session, or turn, and the only identity it
        // adds is `spec.task_id`, which is already this record's `id`.
        owner: None,
    }
}

fn role_next_action_label(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "start planning",
        AgentRole::Coder => "start coding",
        AgentRole::Reviewer => "start review",
        AgentRole::Tester => "start testing",
        AgentRole::DocWriter => "start docs",
        AgentRole::Researcher => "start research",
        AgentRole::ReleaseOperator => "start release gate",
    }
}

fn agent_permission_mode_for_policy(current: PermissionMode, policy: &str) -> PermissionMode {
    if current == PermissionMode::Plan {
        return PermissionMode::Plan;
    }
    match policy
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .as_str()
    {
        "read_only" | "readonly" | "plan" => PermissionMode::Plan,
        "tester_verification"
        | "tester"
        | "test_only"
        | "docs_only"
        | "doc_writer"
        | "documentation"
        | "reviewer"
        | "review_only"
        | "scoped_mutation"
        | "coder"
        | "code"
        | "release_gate"
        | "release_operator"
        | "release"
        | "external_agent"
        | "external"
        | "least_privilege" => PermissionMode::Plan,
        "ask" | "default" => PermissionMode::Default,
        "auto_edit" | "accept_edits" => {
            if matches!(
                current,
                PermissionMode::AcceptEdits
                    | PermissionMode::DontAsk
                    | PermissionMode::BypassPermissions
            ) {
                PermissionMode::AcceptEdits
            } else {
                PermissionMode::Default
            }
        }
        "full_access" | "bypass" | "bypass_permissions" => {
            if current == PermissionMode::BypassPermissions {
                PermissionMode::BypassPermissions
            } else {
                PermissionMode::Default
            }
        }
        _ => PermissionMode::Default,
    }
}

fn apply_agent_role_permission_rules(engine: &mut PermissionEngine, spec: &AgentDagTaskSpec) {
    match normalized_policy_name(&spec.permission_policy).as_str() {
        "tester_verification" | "tester" | "test_only" => {
            allow_verification_shell_commands(engine);
            engine.add_rule(agent_permission_rule(
                PermissionBehavior::Deny,
                "write_file",
                None,
            ));
            engine.add_rule(agent_permission_rule(
                PermissionBehavior::Deny,
                "edit_file",
                None,
            ));
        }
        "scoped_mutation" | "coder" | "code" => {
            allow_scoped_file_mutations(engine, &spec.file_scope);
            allow_scoped_git_staging(engine, &spec.file_scope);
            deny_unscoped_file_roots(engine, &spec.file_scope);
            deny_unscoped_git_roots(engine, &spec.file_scope);
            allow_verification_shell_commands(engine);
            deny_high_risk_git_mutations(engine);
            deny_release_publication_tools(engine);
        }
        "docs_only" | "doc_writer" | "documentation" => {
            allow_docs_file_mutations(engine, &spec.file_scope);
            for blocked_prefix in ["crates/", "apps/", "plugins/"] {
                engine.add_rule(agent_permission_rule(
                    PermissionBehavior::Deny,
                    "write_file",
                    Some(blocked_prefix),
                ));
                engine.add_rule(agent_permission_rule(
                    PermissionBehavior::Deny,
                    "edit_file",
                    Some(blocked_prefix),
                ));
            }
        }
        "release_gate" | "release_operator" | "release" => {
            allow_verification_shell_commands(engine);
            allow_docs_file_mutations(engine, &spec.file_scope);
            allow_scoped_git_staging(engine, &spec.file_scope);
            deny_code_file_roots(engine);
            deny_unscoped_git_roots(engine, &spec.file_scope);
            deny_high_risk_git_mutations(engine);
            deny_release_publication_tools(engine);
        }
        "reviewer" | "review_only" => {
            engine.add_rule(agent_permission_rule(
                PermissionBehavior::Deny,
                "write_file",
                None,
            ));
            engine.add_rule(agent_permission_rule(
                PermissionBehavior::Deny,
                "edit_file",
                None,
            ));
        }
        "external_agent" | "external" | "least_privilege" => {
            for tool_name in [
                "write_file",
                "edit_file",
                "shell",
                "git_add",
                "git_commit",
                "git_push",
                "git_restore",
                "git_stash_drop",
                "git_stash_pop",
                "git_stash_push",
                "git_switch",
                "git_worktree_add",
                "git_worktree_remove",
            ] {
                engine.add_rule(agent_permission_rule(
                    PermissionBehavior::Deny,
                    tool_name,
                    None,
                ));
            }
        }
        _ => {}
    }
}

fn allow_verification_shell_commands(engine: &mut PermissionEngine) {
    for command_prefix in [
        "cargo test",
        "cargo nextest",
        "cargo build",
        "cargo check",
        "pytest",
        "npm test",
        "npm run test",
    ] {
        engine.add_rule(agent_permission_rule(
            PermissionBehavior::Allow,
            "shell",
            Some(command_prefix),
        ));
    }
}

fn allow_scoped_file_mutations(engine: &mut PermissionEngine, file_scope: &[String]) {
    for scope in file_scope {
        let scope = scope.trim();
        if scope.is_empty() {
            continue;
        }
        engine.add_rule(agent_permission_rule(
            PermissionBehavior::Allow,
            "write_file",
            Some(scope),
        ));
        engine.add_rule(agent_permission_rule(
            PermissionBehavior::Allow,
            "edit_file",
            Some(scope),
        ));
    }
}

fn allow_scoped_git_staging(engine: &mut PermissionEngine, file_scope: &[String]) {
    for scope in file_scope {
        let scope = scope.trim();
        if scope.is_empty() {
            continue;
        }
        engine.add_rule(agent_permission_rule(
            PermissionBehavior::Allow,
            "git_add",
            Some(scope),
        ));
    }
}

fn allow_docs_file_mutations(engine: &mut PermissionEngine, file_scope: &[String]) {
    let mut allowed_docs_scope = false;
    for scope in file_scope {
        if is_docs_scope(scope) {
            allowed_docs_scope = true;
            engine.add_rule(agent_permission_rule(
                PermissionBehavior::Allow,
                "write_file",
                Some(scope),
            ));
            engine.add_rule(agent_permission_rule(
                PermissionBehavior::Allow,
                "edit_file",
                Some(scope),
            ));
        }
    }
    if !allowed_docs_scope {
        for scope in ["docs", "README", "CHANGELOG"] {
            engine.add_rule(agent_permission_rule(
                PermissionBehavior::Allow,
                "write_file",
                Some(scope),
            ));
            engine.add_rule(agent_permission_rule(
                PermissionBehavior::Allow,
                "edit_file",
                Some(scope),
            ));
        }
    }
}

fn deny_unscoped_git_roots(engine: &mut PermissionEngine, file_scope: &[String]) {
    for root in ["apps/", "crates/", "docs/", "plugins/"] {
        if file_scope
            .iter()
            .any(|scope| scope_overlaps_root(scope, root))
        {
            continue;
        }
        engine.add_rule(agent_permission_rule(
            PermissionBehavior::Deny,
            "git_add",
            Some(root),
        ));
    }
}

fn deny_unscoped_file_roots(engine: &mut PermissionEngine, file_scope: &[String]) {
    for root in ["apps/", "crates/", "docs/", "plugins/"] {
        if file_scope
            .iter()
            .any(|scope| scope_overlaps_root(scope, root))
        {
            continue;
        }
        engine.add_rule(agent_permission_rule(
            PermissionBehavior::Deny,
            "write_file",
            Some(root),
        ));
        engine.add_rule(agent_permission_rule(
            PermissionBehavior::Deny,
            "edit_file",
            Some(root),
        ));
    }
}

fn deny_high_risk_git_mutations(engine: &mut PermissionEngine) {
    for tool_name in [
        "git_commit",
        "git_restore",
        "git_stash_drop",
        "git_stash_pop",
        "git_stash_push",
        "git_switch",
        "git_worktree_add",
        "git_worktree_remove",
    ] {
        engine.add_rule(agent_permission_rule(
            PermissionBehavior::Deny,
            tool_name,
            None,
        ));
    }
}

fn deny_code_file_roots(engine: &mut PermissionEngine) {
    for root in ["apps/", "crates/", "plugins/"] {
        engine.add_rule(agent_permission_rule(
            PermissionBehavior::Deny,
            "write_file",
            Some(root),
        ));
        engine.add_rule(agent_permission_rule(
            PermissionBehavior::Deny,
            "edit_file",
            Some(root),
        ));
    }
}

fn deny_release_publication_tools(engine: &mut PermissionEngine) {
    for (tool_name, rule_content) in [
        ("git_push", None),
        ("shell", Some("git push")),
        ("shell", Some("gh release")),
        ("shell", Some("cargo publish")),
    ] {
        engine.add_rule(agent_permission_rule(
            PermissionBehavior::Deny,
            tool_name,
            rule_content,
        ));
    }
}

fn scope_overlaps_root(scope: &str, root: &str) -> bool {
    let normalized_scope = scope.trim().trim_start_matches("./");
    if normalized_scope.is_empty() {
        return false;
    }
    normalized_scope.starts_with(root) || root.starts_with(normalized_scope)
}

fn agent_permission_rule(
    behavior: PermissionBehavior,
    tool_name: &str,
    rule_content: Option<&str>,
) -> PermissionRule {
    PermissionRule {
        source: PermissionRuleSource::PolicySettings,
        rule_behavior: behavior,
        rule_value: PermissionRuleValue {
            tool_name: tool_name.to_string(),
            rule_content: rule_content.map(ToString::to_string),
        },
    }
}

fn normalized_policy_name(policy: &str) -> String {
    policy.trim().to_ascii_lowercase().replace('-', "_")
}

fn is_docs_scope(scope: &str) -> bool {
    let normalized = scope.trim().trim_start_matches("./");
    normalized == "docs"
        || normalized.starts_with("docs/")
        || normalized.ends_with(".md")
        || normalized.ends_with(".mdx")
}

fn push_evidence_event(events: &mut Vec<RuntimeEvent>, kind: &str, summary: String) {
    let sequence = next_sequence(events);
    events.push(RuntimeEvent::new(
        sequence,
        RuntimeEventKind::EvidenceRecorded {
            evidence: runtime_evidence(sequence, kind, summary),
        },
    ));
}

fn runtime_evidence(sequence: u64, kind: &str, summary: String) -> EvidenceView {
    EvidenceView {
        id: format!("{kind}-{sequence}"),
        kind: kind.to_string(),
        summary: truncate_for_preview(&summary, 500),
        path: None,
        source: Some("runtime_command".to_string()),
        canonical: None,
        metadata: None,
        timestamp: None,
        // Command receipts are built from a sequence number and a kind; the
        // helper holds no owner identity.
        owner: None,
    }
}

fn provider_config_summary(
    api_key_env: &Option<String>,
    endpoint: &Option<String>,
    default_model: &Option<String>,
) -> String {
    let mut fields = Vec::new();
    if let Some(api_key_env) = api_key_env {
        fields.push(format!("key env {api_key_env}"));
    }
    if let Some(endpoint) = endpoint {
        fields.push(format!("endpoint {endpoint}"));
    }
    if let Some(default_model) = default_model {
        fields.push(format!("default model {default_model}"));
    }
    if fields.is_empty() {
        "no fields".to_string()
    } else {
        fields.join(", ")
    }
}

fn approval_request_view(request_id: &str, prompt: &PermissionPrompt) -> ApprovalRequestView {
    let audit_id = fresh_id("audit");
    ApprovalRequestView {
        id: request_id.to_string(),
        tool_name: prompt.tool_name.clone(),
        title: format!("Approve {}", prompt.tool_name),
        message: prompt.message.clone(),
        input_preview: prompt.input_preview.clone(),
        is_mutating: true,
        reason: Some(prompt.message.clone()),
        owner: RuntimeOwner::default(),
        risk: ApprovalRisk::Medium,
        target: ApprovalTarget {
            kind: prompt.tool_name.clone(),
            display: prompt.input_preview.clone(),
            canonical_ref: prompt.candidate_paths.first().cloned(),
        },
        allowed_scopes: vec![ApprovalScope::Once],
        policy_reason_key: "permission.requires_approval".to_string(),
        policy_reason_args: std::collections::BTreeMap::new(),
        expires_at: now_timestamp().saturating_add(300),
        default_action: ApprovalDefaultAction::Deny,
        audit_id,
        // Whatever the call site computed, carried through unchanged. `None`
        // means Core previewed nothing for this tool.
        decision_context: prompt.decision_context.clone(),
    }
}

fn command_rejected(command_id: String, reason: String) -> RuntimeEvent {
    RuntimeEvent::new(1, RuntimeEventKind::CommandRejected { command_id, reason })
}

pub(crate) const CONTEXT_RETRIEVAL_REASON_MAX_BYTES: usize = 256;
pub(crate) const CONTEXT_RETRIEVAL_REASON_MAX_CHARS: usize = 160;

pub(crate) fn execute_context_retrieval_job(
    job: ContextRetrievalJob,
    control: &ModelRequestControl,
) -> Result<Vec<RuntimeEvent>, String> {
    control.check_cancelled()?;
    #[cfg(test)]
    if let Some(hook) = retrieve_context_test_hook() {
        hook(control);
    }
    control.check_cancelled()?;
    let mut events = Vec::new();
    events.push(RuntimeEvent::new(
        next_sequence(&events),
        RuntimeEventKind::ToolCallStarted {
            tool_call_id: format!("context-read-{}", job.handle.handle_id),
            name: "context_read".to_string(),
            input_preview: format!(
                "handle_id={} reason_category={}",
                redact_identifier_for_event(&job.handle.handle_id),
                job.reason_category
            ),
            // A retrieval job carries a `ContextScope`, not a runtime owner.
            owner: None,
        },
    ));
    let bytes = ContextEngine::open(&job.root)
        .map_err(|err| sanitize_context_error(&err.to_string()))?
        .retrieve(&job.handle, &job.scope)
        .map_err(|err| sanitize_context_error(&err.to_string()))?;
    control.check_cancelled()?;
    let output = bounded_context_output(job.item.kind, &bytes)?;
    control.check_cancelled()?;
    #[cfg(test)]
    if let Some(hook) = retrieve_context_publish_test_hook() {
        hook(control);
    }
    control.check_cancelled()?;
    let byte_count = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let token_count = output.chars().count().div_ceil(4).max(1) as u64;
    let retrieval_id = job.retrieval_id;
    let usage_id = format!("{retrieval_id}-cost");
    let scope = job.scope.clone();
    let cost_attribution = job.cost_attribution.with_request_id(&usage_id);
    events.push(RuntimeEvent::new(
        next_sequence(&events),
        RuntimeEventKind::ToolCallFinished {
            tool_call_id: format!("context-read-{}", job.handle.handle_id),
            name: "context_read".to_string(),
            success: true,
            exit_code: None,
            evidence: Some(EvidenceView {
                id: fresh_id("ctx-read"),
                kind: "context_result".to_string(),
                summary: output,
                path: None,
                source: Some("runtime".to_string()),
                canonical: None,
                metadata: None,
                timestamp: Some(now_timestamp()),
                // See the matching `ToolCallStarted`: scope, not owner.
                owner: None,
            }),
        },
    ));
    events.push(RuntimeEvent::new(
        next_sequence(&events),
        RuntimeEventKind::ContextRetrieved {
            retrieval: ContextRetrievalRecord {
                retrieval_id,
                handle_id: job.handle.handle_id,
                item_id: job.handle.item_id,
                view_id: job.handle.preferred_view_id,
                scope: scope.clone(),
                byte_count,
                token_count,
                reason_category: job.reason_category,
                permission_decision: job.permission_decision,
                reason_rule_category: job.reason_rule_category,
                reason: job.reason,
                requester: "runtime".to_string(),
                retrieved_at: Some(now_timestamp()),
            },
        },
    ));
    events.push(RuntimeEvent::new(
        next_sequence(&events),
        RuntimeEventKind::CostUsageRecorded {
            cost: CostUsageRecord {
                usage_id: usage_id.clone(),
                provider_id: "context".to_string(),
                model: "retrieval".to_string(),
                scopes: cost_attribution.scopes(),
                tokens: TokenUsage {
                    input_tokens: None,
                    output_tokens: None,
                    cached_input_tokens: None,
                    retrieval_tokens: Some(token_count),
                    total_tokens: Some(token_count),
                },
                estimate: None,
                actual_cost: None,
                attempt_index: 0,
                outcome: CostUsageOutcome::Success,
                recorded_at: Some(now_timestamp()),
            },
        },
    ));
    Ok(events)
}

fn validate_context_retrieval_scope_and_expiry(
    handle: &ContextHandleRecord,
    expected_scope: &ContextScope,
) -> Result<(), String> {
    if &handle.scope != expected_scope {
        return Err(format!(
            "context handle `{}` is outside the active context scope",
            redact_identifier_for_event(&handle.handle_id)
        ));
    }
    if handle
        .expires_at
        .is_some_and(|expires_at| expires_at <= now_timestamp())
    {
        return Err(format!(
            "context handle `{}` is expired",
            redact_identifier_for_event(&handle.handle_id)
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn set_retrieve_context_test_hook(hook: Option<RetrieveContextTestHook>) {
    let slot = RETRIEVE_CONTEXT_TEST_HOOK.get_or_init(|| Mutex::new(None));
    *slot.lock().expect("retrieve context test hook lock") = hook;
}

#[cfg(test)]
fn retrieve_context_test_hook() -> Option<RetrieveContextTestHook> {
    RETRIEVE_CONTEXT_TEST_HOOK
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("retrieve context test hook lock")
        .clone()
}

#[cfg(test)]
pub(crate) fn set_retrieve_context_publish_test_hook(hook: Option<RetrieveContextTestHook>) {
    let slot = RETRIEVE_CONTEXT_PUBLISH_TEST_HOOK.get_or_init(|| Mutex::new(None));
    *slot
        .lock()
        .expect("retrieve context publish test hook lock") = hook;
}

#[cfg(test)]
fn retrieve_context_publish_test_hook() -> Option<RetrieveContextTestHook> {
    RETRIEVE_CONTEXT_PUBLISH_TEST_HOOK
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("retrieve context publish test hook lock")
        .clone()
}

fn bounded_context_output(kind: ContextContentKind, bytes: &[u8]) -> Result<String, String> {
    const MAX_CONTEXT_RESULT_BYTES: usize = 16 * 1024;
    let policy = ReductionPolicy {
        max_input_bytes: 2 * 1024 * 1024,
        max_output_bytes: MAX_CONTEXT_RESULT_BYTES,
        max_output_tokens: 4_096,
        ..ReductionPolicy::default()
    };
    let reduced = reduce(kind, bytes, &policy)
        .map_err(|err| sanitize_context_error(&format!("context reduction failed: {err}")))?;
    Ok(truncate_for_preview(
        &redact_command_text(&reduced.content),
        MAX_CONTEXT_RESULT_BYTES,
    ))
}

fn context_retrieval_reason_category(reason: &str) -> String {
    let normalized = reason
        .split_whitespace()
        .next()
        .unwrap_or("retrieve")
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .take(32)
        .collect::<String>()
        .to_ascii_lowercase();
    if normalized.is_empty() {
        "retrieve".to_string()
    } else {
        normalized
    }
}

fn bound_redacted_context_reason(reason: &str) -> String {
    let redacted = redact_command_text(reason);
    let mut out = String::new();
    for (chars, ch) in redacted.chars().enumerate() {
        if chars >= CONTEXT_RETRIEVAL_REASON_MAX_CHARS {
            break;
        }
        let next_len = out.len() + ch.len_utf8();
        if next_len > CONTEXT_RETRIEVAL_REASON_MAX_BYTES {
            break;
        }
        out.push(ch);
    }
    out
}

fn permission_reason_category_from_decision(decision: &PermissionDecision) -> String {
    match decision {
        PermissionDecision::Allow(allow) => {
            permission_reason_category(allow.decision_reason.as_ref())
        }
        PermissionDecision::Ask(ask) => permission_reason_category(ask.decision_reason.as_ref()),
        PermissionDecision::Deny(deny) => permission_reason_category(Some(&deny.decision_reason)),
    }
}

fn permission_reason_category(reason: Option<&PermissionDecisionReason>) -> String {
    match reason {
        Some(PermissionDecisionReason::RuleAllow) => "rule_allow",
        Some(PermissionDecisionReason::RuleDeny) => "rule_deny",
        Some(PermissionDecisionReason::RuleAsk) => "rule_ask",
        Some(PermissionDecisionReason::SafeRead) => "safe_read",
        Some(PermissionDecisionReason::RequiresApproval) => "requires_approval",
        Some(PermissionDecisionReason::OutOfScopePath) => "out_of_scope_path",
        Some(PermissionDecisionReason::BypassMode) => "bypass_mode",
        Some(PermissionDecisionReason::DontAskMode) => "dont_ask_mode",
        Some(PermissionDecisionReason::PlanMode) => "plan_mode",
        Some(PermissionDecisionReason::AcceptEditsMode) => "accept_edits_mode",
        None => "unknown",
    }
    .to_string()
}

fn sanitize_context_error(message: &str) -> String {
    redact_command_text(message)
}

fn redact_identifier_for_event(input: &str) -> String {
    input
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .take(96)
        .collect()
}

pub(crate) fn redacted_runtime_command_for_event(command: &RuntimeCommand) -> RuntimeCommand {
    match command {
        RuntimeCommand::QueryAgentAdapters => RuntimeCommand::QueryAgentAdapters,
        RuntimeCommand::ProbeAgentAdapter { agent_id } => RuntimeCommand::ProbeAgentAdapter {
            agent_id: redact_identifier_for_event(agent_id),
        },
        RuntimeCommand::StartAgentSession { request } => RuntimeCommand::StartAgentSession {
            request: viden_types::AgentSessionRequest {
                lane_id: redact_identifier_for_event(&request.lane_id),
                agent_id: redact_identifier_for_event(&request.agent_id),
                model: request.model.as_deref().map(redact_identifier_for_event),
                load_session_id: request
                    .load_session_id
                    .as_deref()
                    .map(redact_identifier_for_event),
                task: "[REDACTED]".to_string(),
            },
        },
        RuntimeCommand::SendAgentSessionInput { input } => RuntimeCommand::SendAgentSessionInput {
            input: viden_types::AgentSessionInput {
                session_id: redact_identifier_for_event(&input.session_id),
                content: "[REDACTED]".to_string(),
            },
        },
        RuntimeCommand::RetryAgentSession { session_id } => RuntimeCommand::RetryAgentSession {
            session_id: redact_identifier_for_event(session_id),
        },
        RuntimeCommand::CancelAgentSession { session_id } => RuntimeCommand::CancelAgentSession {
            session_id: redact_identifier_for_event(session_id),
        },
        RuntimeCommand::ProbeProject => RuntimeCommand::ProbeProject,
        RuntimeCommand::PreviewProjectConfig { .. } => RuntimeCommand::PreviewProjectConfig {
            contents: "[REDACTED]".to_string(),
        },
        RuntimeCommand::ConfirmProjectConfig {
            preview_id,
            content_sha256,
        } => RuntimeCommand::ConfirmProjectConfig {
            preview_id: redact_identifier_for_event(preview_id),
            content_sha256: redact_identifier_for_event(content_sha256),
        },
        RuntimeCommand::StoreCredentialHandle {
            provider_id,
            backend_id,
            credential_request_id,
        } => RuntimeCommand::StoreCredentialHandle {
            provider_id: redact_identifier_for_event(provider_id),
            backend_id: redact_identifier_for_event(backend_id),
            credential_request_id: redact_identifier_for_event(credential_request_id),
        },
        RuntimeCommand::SetUiPreferences { patch } => {
            RuntimeCommand::SetUiPreferences { patch: *patch }
        }
        RuntimeCommand::ResetUiPreferences => RuntimeCommand::ResetUiPreferences,
        RuntimeCommand::QueryRecentWork { query } => {
            RuntimeCommand::QueryRecentWork { query: *query }
        }
        // The audit query carries only stable identifiers and a cursor; there
        // is no free text to redact, so it passes through like QueryRecentWork.
        RuntimeCommand::QueryAudit { query } => RuntimeCommand::QueryAudit {
            query: query.clone(),
        },
        // The prefix is a workspace-relative path fragment the operator typed
        // into their own client, already validated to stay inside the
        // workspace. Passing the identifier redactor over it would strip the
        // separators and dots and publish a *different* query than the one
        // Core answered, which is worse than verbatim: the accepted-command
        // event would no longer describe the read that produced the page.
        RuntimeCommand::QueryWorkspaceFiles { query } => RuntimeCommand::QueryWorkspaceFiles {
            query: query.clone(),
        },
        // Same reasoning as the inventory prefix above: the paths are
        // target-relative fragments the operator selected in their own
        // client, already validated to stay inside the target. Running the
        // identifier redactor over them would publish a *different* query
        // than the one Core answered.
        RuntimeCommand::QueryWorkspaceDiff { query } => RuntimeCommand::QueryWorkspaceDiff {
            query: query.clone(),
        },
        // Nothing is scrubbed here and that is deliberate. The paths are
        // target-relative fragments the operator selected in their own client
        // and Core already validated; the remote is a configured remote name,
        // not a URL. Only the commit message can run long, so it is *bounded*
        // rather than redacted: running the identifier redactor over prose the
        // operator wrote would publish an accepted command that does not
        // describe the commit Core is about to make.
        RuntimeCommand::RunOperatorGitAction {
            owner,
            target,
            action,
        } => RuntimeCommand::RunOperatorGitAction {
            owner: redacted_runtime_owner(owner),
            target: target.clone(),
            action: match action {
                OperatorGitAction::Commit { message } => OperatorGitAction::Commit {
                    message: truncate_for_preview(message, MAX_COMMAND_EVENT_MESSAGE_CHARS),
                },
                other => other.clone(),
            },
        },
        RuntimeCommand::PreviewStarterLane { request } => RuntimeCommand::PreviewStarterLane {
            request: redacted_starter_lane_request(request),
        },
        RuntimeCommand::PreviewDefaultStarterLane { preset } => {
            RuntimeCommand::PreviewDefaultStarterLane { preset: *preset }
        }
        RuntimeCommand::CreateStarterLane {
            request,
            preview_id,
            content_sha256,
        } => RuntimeCommand::CreateStarterLane {
            request: redacted_starter_lane_request(request),
            preview_id: redact_identifier_for_event(preview_id),
            content_sha256: redact_identifier_for_event(content_sha256),
        },
        RuntimeCommand::SubmitUserInput { content } => RuntimeCommand::SubmitUserInput {
            content: redact_command_text(content),
        },
        RuntimeCommand::QueueFollowUp { content } => RuntimeCommand::QueueFollowUp {
            content: redact_command_text(content),
        },
        RuntimeCommand::StartAgentDag { goal, tasks } => RuntimeCommand::StartAgentDag {
            goal: redact_command_text(goal),
            tasks: tasks
                .iter()
                .map(|task| AgentDagTaskSpec {
                    task_id: task.task_id.clone(),
                    role: task.role,
                    title: redact_command_text(&task.title),
                    objective: redact_command_text(&task.objective),
                    dependencies: task.dependencies.clone(),
                    workspace: task.workspace.as_ref().map(|_| "[REDACTED]".to_string()),
                    file_scope: task
                        .file_scope
                        .iter()
                        .map(|_| "[REDACTED]".to_string())
                        .collect(),
                    context_bundle_id: task.context_bundle_id.clone(),
                    required_evidence: task.required_evidence.clone(),
                    permission_policy: task.permission_policy.clone(),
                })
                .collect(),
        },
        RuntimeCommand::RetrieveContext { handle_id, reason } => RuntimeCommand::RetrieveContext {
            handle_id: redact_identifier_for_event(handle_id),
            reason: bound_redacted_context_reason(reason),
        },
        RuntimeCommand::LoadTranscriptPage { request } => RuntimeCommand::LoadTranscriptPage {
            request: request.clone(),
        },
        RuntimeCommand::CancelActiveTurn => RuntimeCommand::CancelActiveTurn,
        RuntimeCommand::SetWorkMode { mode } => RuntimeCommand::SetWorkMode { mode: *mode },
        RuntimeCommand::SetPermissionLevel { level } => {
            RuntimeCommand::SetPermissionLevel { level: *level }
        }
        RuntimeCommand::RespondToApproval {
            request_id,
            response,
        } => RuntimeCommand::RespondToApproval {
            request_id: redact_identifier_for_event(request_id),
            response: ApprovalResponse {
                decision: response.decision.clone(),
                feedback: response.feedback.as_deref().map(redact_command_text),
            },
        },
        RuntimeCommand::ConfigureProvider {
            provider_id,
            api_key_env,
            endpoint,
            default_model,
        } => RuntimeCommand::ConfigureProvider {
            provider_id: redact_identifier_for_event(provider_id),
            api_key_env: api_key_env.as_deref().map(redact_command_text),
            endpoint: endpoint.as_deref().map(redact_command_text),
            default_model: default_model.as_deref().map(redact_command_text),
        },
        RuntimeCommand::SelectModel { provider_id, model } => RuntimeCommand::SelectModel {
            provider_id: redact_identifier_for_event(provider_id),
            model: redact_command_text(model),
        },
        RuntimeCommand::ActivateModel { provider_id, model } => RuntimeCommand::ActivateModel {
            provider_id: redact_identifier_for_event(provider_id),
            model: redact_command_text(model),
        },
        RuntimeCommand::DeactivateModel { provider_id, model } => RuntimeCommand::DeactivateModel {
            provider_id: redact_identifier_for_event(provider_id),
            model: redact_command_text(model),
        },
        RuntimeCommand::CreateLane { lane } => {
            let mut lane = lane.clone();
            lane.worktree = lane.worktree.as_ref().map(|_| "[REDACTED]".to_string());
            lane.branch = lane.branch.as_deref().map(redact_identifier_for_event);
            lane.active_session_ids = lane
                .active_session_ids
                .iter()
                .map(|id| redact_identifier_for_event(id))
                .collect();
            lane.summary = redact_command_text(&lane.summary);
            lane.evidence = lane
                .evidence
                .iter()
                .map(|id| redact_identifier_for_event(id))
                .collect();
            RuntimeCommand::CreateLane { lane }
        }
        RuntimeCommand::StartLane {
            lane_id,
            command,
            args,
            env,
            output_log,
        } => RuntimeCommand::StartLane {
            lane_id: redact_identifier_for_event(lane_id),
            command: redact_command_text(command),
            args: args.iter().map(|arg| redact_command_text(arg)).collect(),
            env: env
                .iter()
                .map(|(key, _)| (redact_identifier_for_event(key), "[REDACTED]".to_string()))
                .collect(),
            output_log: output_log.as_ref().map(|_| "[REDACTED]".to_string()),
        },
        RuntimeCommand::StopLane { lane_id } => RuntimeCommand::StopLane {
            lane_id: redact_identifier_for_event(lane_id),
        },
        RuntimeCommand::AttachLane { lane_id } => RuntimeCommand::AttachLane {
            lane_id: redact_identifier_for_event(lane_id),
        },
        RuntimeCommand::DetachLane { lane_id } => RuntimeCommand::DetachLane {
            lane_id: redact_identifier_for_event(lane_id),
        },
        RuntimeCommand::SendLaneInput { lane_id, .. } => RuntimeCommand::SendLaneInput {
            lane_id: redact_identifier_for_event(lane_id),
            input: "[REDACTED]".to_string(),
        },
        RuntimeCommand::AcceptLaneOutput { lane_id, summary } => RuntimeCommand::AcceptLaneOutput {
            lane_id: redact_identifier_for_event(lane_id),
            summary: redact_command_text(summary),
        },
        RuntimeCommand::ReviseLaneOutput { lane_id, feedback } => {
            RuntimeCommand::ReviseLaneOutput {
                lane_id: redact_identifier_for_event(lane_id),
                feedback: redact_command_text(feedback),
            }
        }
        RuntimeCommand::DiscardLaneOutput { lane_id, reason } => {
            RuntimeCommand::DiscardLaneOutput {
                lane_id: redact_identifier_for_event(lane_id),
                reason: redact_command_text(reason),
            }
        }
        RuntimeCommand::ApplyLaneChanges { lane_id, .. } => RuntimeCommand::ApplyLaneChanges {
            lane_id: redact_identifier_for_event(lane_id),
            unified_diff: "[REDACTED]".to_string(),
        },
        RuntimeCommand::ResolveLaneConflict { lane_id, .. } => {
            RuntimeCommand::ResolveLaneConflict {
                lane_id: redact_identifier_for_event(lane_id),
                unified_diff: "[REDACTED]".to_string(),
            }
        }
        RuntimeCommand::ArchiveLane { lane_id, summary } => RuntimeCommand::ArchiveLane {
            lane_id: redact_identifier_for_event(lane_id),
            summary: redact_command_text(summary),
        },
        RuntimeCommand::CleanupLane { lane_id, force } => RuntimeCommand::CleanupLane {
            lane_id: redact_identifier_for_event(lane_id),
            force: *force,
        },
        RuntimeCommand::StartAgentTask { task_id } => RuntimeCommand::StartAgentTask {
            task_id: redact_identifier_for_event(task_id),
        },
        RuntimeCommand::CancelAgentTask { task_id } => RuntimeCommand::CancelAgentTask {
            task_id: redact_identifier_for_event(task_id),
        },
        RuntimeCommand::CreateHandoff {
            handoff_id,
            task_id,
            from_lane_id,
            to_lane_id,
            owner,
            summary,
            acceptance,
        } => RuntimeCommand::CreateHandoff {
            handoff_id: redact_identifier_for_event(handoff_id),
            task_id: redact_identifier_for_event(task_id),
            from_lane_id: redact_identifier_for_event(from_lane_id),
            to_lane_id: redact_identifier_for_event(to_lane_id),
            owner: redacted_runtime_owner(owner),
            summary: redact_command_text(summary),
            acceptance: *acceptance,
        },
        RuntimeCommand::RequestReview {
            review_id,
            gate_id,
            requester_lane_id,
            reviewer_lane_id,
            owner,
            evidence_ids,
        } => RuntimeCommand::RequestReview {
            review_id: redact_identifier_for_event(review_id),
            gate_id: redact_identifier_for_event(gate_id),
            requester_lane_id: redact_identifier_for_event(requester_lane_id),
            reviewer_lane_id: redact_identifier_for_event(reviewer_lane_id),
            owner: redacted_runtime_owner(owner),
            evidence_ids: evidence_ids
                .iter()
                .map(|id| redact_identifier_for_event(id))
                .collect(),
        },
        RuntimeCommand::DecideReview {
            review_id,
            verdict,
            feedback,
            actor,
        } => RuntimeCommand::DecideReview {
            review_id: redact_identifier_for_event(review_id),
            verdict: *verdict,
            // Reviewer prose is redacted exactly like a gate rejection reason:
            // the verdict and the ids are contract facts, the note is not.
            feedback: feedback.as_deref().map(redact_command_text),
            actor: redacted_runtime_owner(actor),
        },
        RuntimeCommand::ConfirmContract {
            contract_id,
            task_id,
            owner,
            summary,
            decision,
        } => RuntimeCommand::ConfirmContract {
            contract_id: redact_identifier_for_event(contract_id),
            task_id: redact_identifier_for_event(task_id),
            owner: redacted_runtime_owner(owner),
            summary: redact_command_text(summary),
            decision: *decision,
        },
        RuntimeCommand::SetDependency {
            dependency_id,
            task_id,
            depends_on_task_id,
            owner,
            state,
            reason,
        } => RuntimeCommand::SetDependency {
            dependency_id: redact_identifier_for_event(dependency_id),
            task_id: redact_identifier_for_event(task_id),
            depends_on_task_id: redact_identifier_for_event(depends_on_task_id),
            owner: redacted_runtime_owner(owner),
            state: *state,
            reason: redact_command_text(reason),
        },
        RuntimeCommand::AcceptMergeGate {
            gate_id,
            actor,
            reviewed_evidence,
            decision,
        } => RuntimeCommand::AcceptMergeGate {
            gate_id: redact_identifier_for_event(gate_id),
            actor: redacted_runtime_owner(actor),
            reviewed_evidence: reviewed_evidence.clone(),
            decision: decision.as_deref().map(redact_command_text),
        },
        RuntimeCommand::RejectMergeGate {
            gate_id,
            actor,
            reason,
        } => RuntimeCommand::RejectMergeGate {
            gate_id: redact_identifier_for_event(gate_id),
            actor: redacted_runtime_owner(actor),
            reason: redact_command_text(reason),
        },
        RuntimeCommand::RecordAgentEvidence {
            gate_id,
            evidence_id,
            kind,
            summary,
            ..
        } => RuntimeCommand::RecordAgentEvidence {
            gate_id: redact_identifier_for_event(gate_id),
            evidence_id: evidence_id.as_deref().map(redact_identifier_for_event),
            kind: redact_identifier_for_event(kind),
            summary: redacted_evidence_summary_marker(summary),
            path: None,
            source: None,
            canonical: None,
        },
        RuntimeCommand::AcceptAgentArtifact {
            gate_id,
            evidence_id,
            actor,
            source_hash,
            decision,
        } => RuntimeCommand::AcceptAgentArtifact {
            gate_id: redact_identifier_for_event(gate_id),
            evidence_id: redact_identifier_for_event(evidence_id),
            actor: redacted_runtime_owner(actor),
            source_hash: source_hash.clone(),
            decision: decision.as_deref().map(redact_command_text),
        },
        RuntimeCommand::RejectAgentArtifact {
            gate_id,
            evidence_id,
            actor,
            reason,
        } => RuntimeCommand::RejectAgentArtifact {
            gate_id: redact_identifier_for_event(gate_id),
            evidence_id: redact_identifier_for_event(evidence_id),
            actor: redacted_runtime_owner(actor),
            reason: redact_command_text(reason),
        },
        RuntimeCommand::MergeAgentPatch {
            gate_id,
            actor,
            decision,
        } => RuntimeCommand::MergeAgentPatch {
            gate_id: redact_identifier_for_event(gate_id),
            actor: redacted_runtime_owner(actor),
            decision: decision.as_deref().map(redact_command_text),
        },
        RuntimeCommand::RevalidateMergeConflict {
            gate_id,
            bounce_id,
            actor,
            evidence,
        } => RuntimeCommand::RevalidateMergeConflict {
            gate_id: redact_identifier_for_event(gate_id),
            bounce_id: redact_identifier_for_event(bounce_id),
            actor: redacted_runtime_owner(actor),
            evidence: evidence.clone(),
        },
        RuntimeCommand::BounceMergeConflict {
            gate_id,
            original_lane_id,
            owner,
            reason,
        } => RuntimeCommand::BounceMergeConflict {
            gate_id: redact_identifier_for_event(gate_id),
            original_lane_id: redact_identifier_for_event(original_lane_id),
            owner: redacted_runtime_owner(owner),
            reason: redact_command_text(reason),
        },
        RuntimeCommand::RevertAppliedChange {
            gate_id,
            owner,
            reason,
        } => RuntimeCommand::RevertAppliedChange {
            gate_id: redact_identifier_for_event(gate_id),
            owner: redacted_runtime_owner(owner),
            reason: redact_command_text(reason),
        },
    }
}

fn redacted_runtime_owner(owner: &RuntimeOwner) -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: redact_identifier_for_event(&owner.workspace_id),
        project_id: redact_identifier_for_event(&owner.project_id),
        lane_id: owner.lane_id.as_deref().map(redact_identifier_for_event),
        session_id: owner.session_id.as_deref().map(redact_identifier_for_event),
        task_id: owner.task_id.as_deref().map(redact_identifier_for_event),
        turn_id: owner.turn_id.as_deref().map(redact_identifier_for_event),
    }
}

fn redacted_starter_lane_request(
    request: &viden_types::StarterLaneRequest,
) -> viden_types::StarterLaneRequest {
    viden_types::StarterLaneRequest {
        lane_id: redact_identifier_for_event(&request.lane_id),
        preset: request.preset,
        branch: request.branch.as_deref().map(redact_identifier_for_event),
        worktree_path: request
            .worktree_path
            .as_ref()
            .map(|_| "[REDACTED]".to_string()),
    }
}

fn redacted_evidence_summary_marker(summary: &str) -> String {
    if summary.trim().is_empty() {
        "[REDACTED:empty]".to_string()
    } else {
        "[REDACTED:bounded-summary]".to_string()
    }
}

fn validate_external_canonical_evidence_reference(
    canonical: CanonicalEvidenceReference,
) -> Result<CanonicalEvidenceReference, String> {
    validate_canonical_identifier("item_id", &canonical.item_id, 128)?;
    validate_canonical_identifier("bundle_id", &canonical.bundle_id, 128)?;
    validate_canonical_hash("source_hash", &canonical.source_hash)?;
    validate_optional_canonical_identifier("producer.identity", &canonical.producer.identity, 64)?;
    if !canonical.producer.role.is_empty() && AgentRole::parse(&canonical.producer.role).is_none() {
        return Err("invalid_canonical_evidence_reference:producer.role".to_string());
    }
    validate_optional_canonical_identifier("producer.role", &canonical.producer.role, 32)?;
    validate_canonical_identifier("producer.task_id", &canonical.producer.task_id, 128)?;
    if let Some(snapshot_id) = &canonical.permission_snapshot_id {
        validate_canonical_identifier("permission_snapshot_id", snapshot_id, 128)?;
    }
    validate_canonical_scope("permission_scope", &canonical.permission_scope)?;
    validate_canonical_scope("evidence_scope", &canonical.evidence_scope)?;
    Ok(canonical)
}

fn validate_canonical_scope(field: &str, scope: &ContextScope) -> Result<(), String> {
    match scope {
        ContextScope::Task(id) => {
            validate_canonical_identifier(&format!("{field}.task_id"), id, 128)
        }
        ContextScope::Dag(id) => validate_canonical_identifier(&format!("{field}.dag_id"), id, 128),
        ContextScope::Workflow(id) => {
            validate_canonical_identifier(&format!("{field}.workflow_id"), id, 128)
        }
    }
}

fn validate_canonical_hash(field: &str, value: &str) -> Result<(), String> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(format!("invalid_canonical_evidence_reference:{field}"))
    }
}

fn validate_canonical_identifier(field: &str, value: &str, max_len: usize) -> Result<(), String> {
    if is_safe_canonical_identifier(value, max_len) {
        Ok(())
    } else {
        Err(format!("invalid_canonical_evidence_reference:{field}"))
    }
}

fn validate_optional_canonical_identifier(
    field: &str,
    value: &str,
    max_len: usize,
) -> Result<(), String> {
    if value.is_empty() {
        Ok(())
    } else {
        validate_canonical_identifier(field, value, max_len)
    }
}

fn is_safe_canonical_identifier(value: &str, max_len: usize) -> bool {
    if value.is_empty()
        || value.len() > max_len
        || value.trim() != value
        || value.contains('/')
        || value.contains('\\')
        || value.contains("..")
        || value.contains("://")
        || value.chars().any(char::is_control)
    {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("sk-")
        || lower.contains("secret")
        || lower.contains("token=")
        || lower.contains("api_key")
        || lower.contains("apikey")
    {
        return false;
    }
    value.bytes().all(
        |byte| matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b':'),
    )
}

fn sanitize_agent_task_spec_for_domain(mut spec: AgentDagTaskSpec) -> AgentDagTaskSpec {
    spec.title = sanitize_runtime_domain_text(&spec.title, 160);
    spec.objective = sanitize_runtime_domain_text(&spec.objective, 240);
    spec.workspace = spec.workspace.as_deref().and_then(safe_runtime_domain_path);
    spec.file_scope = spec
        .file_scope
        .iter()
        .filter_map(|scope| safe_runtime_domain_path(scope))
        .collect();
    spec
}

fn sanitize_runtime_domain_text(value: &str, max_chars: usize) -> String {
    truncate_for_preview(&redact_command_text(value), max_chars)
}

fn safe_runtime_domain_path(path: &str) -> Option<String> {
    validate_evidence_path_for_event(path).ok()
}

fn validate_evidence_path_for_event(path: &str) -> Result<String, String> {
    let trimmed = path.trim();
    if trimmed.is_empty()
        || trimmed.chars().any(char::is_control)
        || std::path::Path::new(trimmed).is_absolute()
        || std::path::Path::new(trimmed).components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err("invalid evidence path".to_string());
    }
    Ok(trimmed
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/"))
}

fn sanitize_evidence_summary_for_event(summary: &str) -> String {
    truncate_for_preview(&redact_command_text(summary), 500)
}

fn sanitize_evidence_source_for_event(source: &str) -> String {
    truncate_for_preview(&redact_command_text(source), 120)
}

fn redact_command_text(input: &str) -> String {
    if input.to_ascii_lowercase().contains("diff --git") {
        return "[REDACTED]".to_string();
    }
    input
        .split_whitespace()
        .map(|word| {
            let lower = word.to_ascii_lowercase();
            if lower.starts_with("sk-")
                || lower.contains("secret")
                || lower.contains("token=")
                || lower.contains("api_key")
                || lower.contains("apikey")
                || word.starts_with('/')
                || word.contains("..")
                || word.contains("/Users/")
                || word.contains("/tmp/")
                || word.contains("/var/")
                || word.contains("diff --git")
                || word.chars().any(char::is_control)
            {
                "[REDACTED]"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn redacted_task_for_event(mut task: AgentTaskRecord) -> AgentTaskRecord {
    task.title = redact_command_text(&task.title);
    task.activity = redact_command_text(&task.activity);
    task.summary = redact_command_text(&task.summary);
    if let Some(result) = task.result {
        task.result = Some(redact_command_text(&result));
    }
    task.evidence = task
        .evidence
        .into_iter()
        .map(|evidence| redact_command_text(&evidence))
        .collect();
    task
}

fn permission_mode_for_level(level: PermissionLevel) -> PermissionMode {
    match level {
        PermissionLevel::Ask => PermissionMode::Default,
        PermissionLevel::AutoEdit | PermissionLevel::Auto => PermissionMode::AcceptEdits,
        PermissionLevel::ReadOnly => PermissionMode::Plan,
        PermissionLevel::FullAccess => PermissionMode::BypassPermissions,
    }
}

fn next_sequence(events: &[RuntimeEvent]) -> u64 {
    events.last().map(|event| event.sequence + 1).unwrap_or(1)
}

fn append_resequenced(target: &mut Vec<RuntimeEvent>, source: Vec<RuntimeEvent>) {
    for mut event in source {
        event.sequence = next_sequence(target);
        target.push(event);
    }
}

fn append_ordered_approval_boundaries(
    target: &mut Vec<RuntimeEvent>,
    boundaries: &[crate::OrderedApprovalBoundary],
    before_engine_event_index: usize,
) {
    for (boundary_index, boundary) in boundaries
        .iter()
        .enumerate()
        .filter(|(_, boundary)| boundary.before_engine_event_index == before_engine_event_index)
    {
        let request_id = format!("approval-{}", boundary_index + 1);
        target.push(RuntimeEvent::new(
            next_sequence(target),
            RuntimeEventKind::ApprovalRequested {
                approval: approval_request_view(&request_id, &boundary.prompt),
            },
        ));
        target.push(RuntimeEvent::new(
            next_sequence(target),
            RuntimeEventKind::ApprovalResolved {
                request_id,
                decision: boundary.decision.clone(),
                owner: RuntimeOwner::default(),
                audit_id: fresh_id("audit"),
            },
        ));
    }
}

fn runtime_owner_matches_validator_lane(actor: &RuntimeOwner, validator: &RuntimeOwner) -> bool {
    actor.workspace_id == validator.workspace_id
        && actor.project_id == validator.project_id
        && actor.task_id == validator.task_id
        && actor.lane_id.is_some()
        && actor.lane_id == validator.lane_id
}

fn validate_reject_actor(
    gate: &MergeGateRecord,
    actor: &RuntimeOwner,
    action: &str,
) -> Result<(), String> {
    if actor == &RuntimeOwner::default() {
        return Err(format!("{action} requires an actor"));
    }
    if let Some(validator) = &gate.validator
        && runtime_owner_matches_validator_lane(actor, &validator.owner)
    {
        return Ok(());
    }
    if actor == &gate.owner {
        return Ok(());
    }
    Err(format!(
        "{action} actor is not authorized for the merge gate"
    ))
}

pub(crate) fn provider_health_view(
    provider_id: &str,
    model: &str,
    telemetry: &ProviderTelemetry,
) -> ProviderHealthView {
    let status = if telemetry.failure_count > telemetry.success_count {
        "degraded"
    } else {
        "healthy"
    };
    ProviderHealthView {
        provider_id: provider_id.to_string(),
        model: model.to_string(),
        status: status.to_string(),
        request_count: telemetry.request_count,
        error_count: telemetry.failure_count,
        last_latency_ms: telemetry.last_latency_ms.and_then(clamp_u128_to_u64),
        average_latency_ms: telemetry.average_latency_ms.and_then(clamp_u128_to_u64),
        tokens_per_second: telemetry.last_tokens_per_second,
        credential: None,
    }
}

fn token_cost_view(telemetry: &ProviderTelemetry) -> Option<TokenCostView> {
    if telemetry.total_tokens == 0 && telemetry.total_cost_micro_usd.is_none() {
        return None;
    }
    Some(TokenCostView {
        input_tokens: telemetry.total_input_tokens,
        output_tokens: telemetry.total_output_tokens,
        total_tokens: telemetry.total_tokens,
        cost_micro_usd: telemetry.total_cost_micro_usd,
    })
}

fn clamp_u128_to_u64(value: u128) -> Option<u64> {
    Some(value.min(u128::from(u64::MAX)) as u64)
}

fn parse_legacy_tool_call(text: &str) -> (String, String) {
    let trimmed = text.trim();
    match trimmed.split_once(' ') {
        Some((name, input)) => (name.to_string(), input.to_string()),
        None => (trimmed.to_string(), String::new()),
    }
}
