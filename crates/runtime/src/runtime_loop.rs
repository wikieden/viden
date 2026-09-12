use std::{sync::Arc, time::Instant};

use crate::{
    EngineEvent, PROVIDER_REASONING_CONTENT_KEY, SessionEngine,
    context_bundle::{ContextBuildMode, context_evidence_rows, render_provider_context_message},
    lsp_tools::LspToolAdapter,
    lsp_tools::render_lsp_diagnostics,
    presentation::render_permission_denial,
};
use viden_lsp::SemanticProvider;
use viden_provider::ModelRequestControl;
use viden_tools::ToolExecutionContext;
use viden_types::{
    AgentTaskStatus, ApprovalResponse, ContextBundleRecord, CostAmount, CostEstimate,
    CostUsageOutcome, CostUsageRecord, Message, ModelEvent, ModelRequest, ModelUsage,
    PermissionDecision, PermissionLogEntry, Role, TokenUsage, ToolCall, ToolResult,
    TranscriptEntry, fresh_id, now_timestamp,
};

use crate::CostAttribution;

#[derive(Debug, Default)]
struct EngineTurnProgress {
    engine_events: Vec<EngineEvent>,
    ordered_runtime_facts: Vec<crate::OrderedRuntimeFact>,
    ordered_approval_boundaries: Vec<crate::OrderedApprovalBoundary>,
}

#[derive(Debug, Default)]
struct TurnTaskTracker {
    task_ids: Vec<String>,
}

impl TurnTaskTracker {
    fn track(&mut self, task_id: &str) {
        if !self.task_ids.iter().any(|existing| existing == task_id) {
            self.task_ids.push(task_id.to_string());
        }
    }
}

impl EngineTurnProgress {
    fn carry<T>(&self, result: Result<T, String>) -> Result<T, crate::EngineTurnFailure> {
        result.map_err(|message| crate::EngineTurnFailure {
            message,
            completed: crate::EngineTurnOutput {
                engine_events: self.engine_events.clone(),
                ordered_runtime_facts: self.ordered_runtime_facts.clone(),
                ordered_approval_boundaries: self.ordered_approval_boundaries.clone(),
            },
        })
    }

    fn fail(&self, message: String) -> crate::EngineTurnFailure {
        crate::EngineTurnFailure {
            message,
            completed: crate::EngineTurnOutput {
                engine_events: self.engine_events.clone(),
                ordered_runtime_facts: self.ordered_runtime_facts.clone(),
                ordered_approval_boundaries: self.ordered_approval_boundaries.clone(),
            },
        }
    }

    fn finish(self) -> crate::EngineTurnOutput {
        crate::EngineTurnOutput {
            engine_events: self.engine_events,
            ordered_runtime_facts: self.ordered_runtime_facts,
            ordered_approval_boundaries: self.ordered_approval_boundaries,
        }
    }

    fn take_completed(&mut self) -> crate::EngineTurnOutput {
        crate::EngineTurnOutput {
            engine_events: std::mem::take(&mut self.engine_events),
            ordered_runtime_facts: std::mem::take(&mut self.ordered_runtime_facts),
            ordered_approval_boundaries: std::mem::take(&mut self.ordered_approval_boundaries),
        }
    }
}

/// Publishes the facts one completed tool call produces.
///
/// A method rather than a free function since `runtime.durable_work_evidence`:
/// the archived `patch` row an applied mutation produces needs the turn's owner,
/// the canonical store, and the approval receipt, none of which the tool loop
/// carries as parameters.
///
/// Order is the contract. The live `WorkspaceChangeUpdated` comes first because
/// it is what a cockpit renders immediately, and the archived `EvidenceRecorded`
/// follows it describing the same change; a client that received them the other
/// way round would show a reviewable artifact before the change it belongs to.
fn record_completed_tool(
    engine: &crate::SessionEngine,
    progress: &mut EngineTurnProgress,
    call: &ToolCall,
    result: &ToolResult,
) {
    progress.engine_events.push(EngineEvent::ToolResult {
        output: result.output.clone(),
        success: result.success,
        exit_code: result.exit_code,
    });
    let after_engine_event_index = progress.engine_events.len() - 1;
    // Consumed for every completed tool call, not only a mutating one, so an
    // approval that allowed a `shell` command cannot be claimed as the receipt
    // for an unapproved `edit_file` that runs after it.
    let permission_snapshot_id = engine.take_permission_receipt();
    let unbound_owner = viden_types::RuntimeOwner::default();
    let mut cockpit_facts =
        crate::frontend_status::workspace_changes_from_tool_result(call, result, &unbound_owner)
            .into_iter()
            .map(|change| {
                viden_types::RuntimeEvent::new(
                    0,
                    viden_types::RuntimeEventKind::WorkspaceChangeUpdated { change },
                )
            })
            .collect::<Vec<_>>();
    // The archived row carries the turn owner verbatim and is therefore not
    // rebound at emission the way the cockpit facts above are: the command's
    // owner names no turn, and an evidence row that lost its `turn_id` could no
    // longer be joined to the audit record for the approval that allowed it.
    cockpit_facts.extend(engine.native_patch_evidence_events(call, result, permission_snapshot_id));
    if let Some(check) =
        crate::frontend_status::check_run_from_tool_result(call, result, &unbound_owner)
    {
        cockpit_facts.push(viden_types::RuntimeEvent::new(
            0,
            viden_types::RuntimeEventKind::CheckRunUpdated { check },
        ));
    }
    progress
        .ordered_runtime_facts
        .extend(
            cockpit_facts
                .into_iter()
                .map(|event| crate::OrderedRuntimeFact {
                    after_engine_event_index,
                    event,
                }),
        );
}

fn record_started_tool(progress: &mut EngineTurnProgress, call: &ToolCall, encoded_input: &str) {
    progress.engine_events.push(EngineEvent::ToolCall(format!(
        "{} {}",
        call.name, encoded_input
    )));
}

const PROVIDER_REQUEST_CHAR_BUDGET: usize = 48_000;
const PROVIDER_RECENT_HISTORY_LIMIT: usize = 8;
const PROVIDER_HISTORY_MESSAGE_CHAR_LIMIT: usize = 1_500;
const PROVIDER_TAIL_MESSAGE_CHAR_LIMIT: usize = 4_000;
const PROVIDER_SUMMARY_LINE_LIMIT: usize = 180;
const PROVIDER_SUMMARY_LINE_COUNT: usize = 20;
const PROVIDER_RETRY_REQUEST_CHAR_BUDGET: usize = 16_000;
const PROVIDER_RETRY_HISTORY_MESSAGE_CHAR_LIMIT: usize = 800;
const PROVIDER_RETRY_TAIL_MESSAGE_CHAR_LIMIT: usize = 1_200;

impl SessionEngine {
    pub fn process_input_with_approval<F>(
        &mut self,
        input: &str,
        approver: &mut F,
    ) -> Result<Vec<EngineEvent>, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        self.process_input_with_approval_and_control(input, approver, &ModelRequestControl::new())
    }

    pub fn process_input_with_approval_and_control<F>(
        &mut self,
        input: &str,
        approver: &mut F,
        control: &ModelRequestControl,
    ) -> Result<Vec<EngineEvent>, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        self.process_engine_turn_with_approval_and_control(input, approver, control)
            .map(|output| output.engine_events)
            .map_err(|failure| failure.message)
    }

    pub(crate) fn process_engine_turn_with_approval_and_control<F>(
        &mut self,
        input: &str,
        approver: &mut F,
        control: &ModelRequestControl,
    ) -> Result<crate::EngineTurnOutput, crate::EngineTurnFailure>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        let mut on_approval_boundary = None;
        self.process_input_with_optional_context_bundle(
            input,
            approver,
            control,
            None,
            &mut on_approval_boundary,
        )
    }

    pub(crate) fn process_engine_turn_streaming_with_approval_and_control<F, C>(
        &mut self,
        input: &str,
        approver: &mut F,
        control: &ModelRequestControl,
        on_approval_boundary: &mut C,
    ) -> Result<crate::EngineTurnOutput, crate::EngineTurnFailure>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
        C: FnMut(crate::EngineTurnOutput),
    {
        let mut on_approval_boundary =
            Some(on_approval_boundary as &mut dyn FnMut(crate::EngineTurnOutput));
        self.process_input_with_optional_context_bundle(
            input,
            approver,
            control,
            None,
            &mut on_approval_boundary,
        )
    }

    pub(crate) fn process_engine_turn_with_built_context_bundle_and_control<F>(
        &mut self,
        input: &str,
        approver: &mut F,
        control: &ModelRequestControl,
        built_context_bundle: crate::context_bundle::BuiltContextBundle,
    ) -> Result<crate::EngineTurnOutput, crate::EngineTurnFailure>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        let mut on_approval_boundary = None;
        self.process_input_with_optional_context_bundle(
            input,
            approver,
            control,
            Some(built_context_bundle),
            &mut on_approval_boundary,
        )
    }

    pub(crate) fn process_engine_turn_streaming_with_built_context_bundle_and_control<F, C>(
        &mut self,
        input: &str,
        approver: &mut F,
        control: &ModelRequestControl,
        built_context_bundle: crate::context_bundle::BuiltContextBundle,
        on_approval_boundary: &mut C,
    ) -> Result<crate::EngineTurnOutput, crate::EngineTurnFailure>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
        C: FnMut(crate::EngineTurnOutput),
    {
        let mut on_approval_boundary =
            Some(on_approval_boundary as &mut dyn FnMut(crate::EngineTurnOutput));
        self.process_input_with_optional_context_bundle(
            input,
            approver,
            control,
            Some(built_context_bundle),
            &mut on_approval_boundary,
        )
    }

    fn process_input_with_optional_context_bundle<F>(
        &mut self,
        input: &str,
        approver: &mut F,
        control: &ModelRequestControl,
        built_context_bundle: Option<crate::context_bundle::BuiltContextBundle>,
        on_approval_boundary: &mut Option<&mut dyn FnMut(crate::EngineTurnOutput)>,
    ) -> Result<crate::EngineTurnOutput, crate::EngineTurnFailure>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        let mut progress = EngineTurnProgress::default();
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Ok(progress.finish());
        }
        if trimmed.starts_with('/') {
            return progress
                .carry(self.handle_command(trimmed, approver))
                .map(|engine_events| crate::EngineTurnOutput {
                    engine_events,
                    ordered_runtime_facts: Vec::new(),
                    ordered_approval_boundaries: Vec::new(),
                });
        }

        let user_message = Message::new(Role::User, trimmed);
        self.messages.push(user_message.clone());
        progress.carry(self.store_entry(TranscriptEntry::Message {
            message: user_message,
        }))?;
        #[cfg(test)]
        let context_build_started = Instant::now();
        let built_context_bundle = built_context_bundle.unwrap_or_else(|| {
            self.build_main_context_bundle_with_mode(trimmed, ContextBuildMode::Normal)
        });
        #[cfg(test)]
        let context_build_elapsed = context_build_started.elapsed();
        let mut context_bundle = built_context_bundle.bundle;
        self.last_context_runtime_events = built_context_bundle.events;
        self.last_context_bundle = Some(context_bundle.clone());
        if built_context_bundle.hard_exceeded {
            return Err(progress.fail(format!(
                "context hard limit exceeded before provider request: used {} tokens, hard limit {}. reduce input, narrow file scope, or split the task.",
                context_bundle.estimated_tokens, context_bundle.hard_token_limit
            )));
        }
        let mut provider_task = self.provider_task(
            trimmed,
            AgentTaskStatus::Thinking,
            "thinking through latest prompt",
            15,
        );
        provider_task
            .evidence
            .extend(context_evidence_rows(&context_bundle));
        let mut turn_tasks = TurnTaskTracker::default();
        turn_tasks.track(&provider_task.id);
        self.upsert_agent_task(provider_task.clone());

        let turn_result = (|| -> Result<(), crate::EngineTurnFailure> {
            let mut retried_request_too_large = false;
            let mut provider_retry_count = 0_u64;
            for attempt_index in 0..8 {
                provider_task.status = AgentTaskStatus::Thinking;
                provider_task.activity = "waiting for provider response".to_string();
                provider_task.progress = provider_task.progress.max(20);
                provider_task.updated_at = Some(now_millis());
                self.upsert_agent_task(provider_task.clone());
                let request_messages = self.build_provider_request_messages_for_current_mode(
                    &context_bundle,
                    retried_request_too_large,
                );
                #[cfg(test)]
                self.record_context_benchmark_metrics_for_test(
                    &request_messages,
                    &context_bundle,
                    context_build_elapsed,
                    provider_retry_count,
                );
                let request = ModelRequest {
                    session_id: self.session_id().to_string(),
                    model: self.provider.model().to_string(),
                    messages: request_messages,
                    tools: self.tools.specs(),
                    work_mode: self.runtime_snapshot.work_mode,
                    permission_mode: self.permissions.mode(),
                    permission_level: self.runtime_snapshot.permission_level,
                };
                let request_started = Instant::now();
                let model_events = match self.provider.next_events_with_control(&request, control) {
                    Ok(events) => {
                        let usage = aggregate_model_usage(&events);
                        let usage_id = provider_attempt_usage_id(&provider_task.id, attempt_index);
                        progress.carry(self.record_cost_usage(provider_attempt_cost_record(
                            self.provider_name(),
                            self.model_name(),
                            attempt_index,
                            usage.as_ref(),
                            CostUsageOutcome::Success,
                            self.cost_attribution_for_request(&usage_id, Some(&provider_task.id)),
                        )))?;
                        self.provider_telemetry.record_success(
                            request_started.elapsed(),
                            events.len(),
                            usage,
                        );
                        events
                    }
                    Err(err) => {
                        let usage_id = provider_attempt_usage_id(&provider_task.id, attempt_index);
                        progress.carry(self.record_cost_usage(provider_attempt_cost_record(
                            self.provider_name(),
                            self.model_name(),
                            attempt_index,
                            None,
                            CostUsageOutcome::Failure,
                            self.cost_attribution_for_request(&usage_id, Some(&provider_task.id)),
                        )))?;
                        self.provider_telemetry
                            .record_failure(request_started.elapsed(), &err);
                        if crate::provider_commands::is_request_too_large_provider_failure(&err)
                            && !retried_request_too_large
                        {
                            retried_request_too_large = true;
                            provider_retry_count = provider_retry_count.saturating_add(1);
                            let retry_context = self.materialize_existing_context_bundle(
                                &context_bundle,
                                ContextBuildMode::RequestTooLargeRetry,
                            );
                            context_bundle = retry_context.bundle;
                            self.last_context_runtime_events = retry_context.events;
                            self.last_context_bundle = Some(context_bundle.clone());
                            if retry_context.hard_exceeded {
                                return Err(progress.fail(format!(
                                    "context hard limit exceeded during request-too-large recovery: used {} tokens, hard limit {}. reduce input, narrow file scope, or split the task.",
                                    context_bundle.estimated_tokens, context_bundle.hard_token_limit
                                )));
                            }
                            let note =
                                "Provider request was too large; retrying with compacted context.";
                            let system_message = Message::new(Role::System, note);
                            self.messages.push(system_message.clone());
                            progress.carry(self.store_entry(TranscriptEntry::Message {
                                message: system_message,
                            }))?;
                            progress
                                .engine_events
                                .push(EngineEvent::System(note.to_string()));
                            provider_task.status = AgentTaskStatus::Thinking;
                            provider_task.activity =
                                "compacting provider context after request-too-large".to_string();
                            provider_task.progress = provider_task.progress.max(35);
                            provider_task
                                .evidence
                                .push("provider_retry request_too_large compacted".to_string());
                            provider_task.updated_at = Some(now_millis());
                            self.upsert_agent_task(provider_task.clone());
                            continue;
                        }
                        let rendered_error = self
                            .provider_model_recovery_prompt(&err)
                            .map(|hint| format!("{err}\n\n{hint}"))
                            .unwrap_or_else(|| err.clone());
                        provider_task.status = AgentTaskStatus::Failed;
                        provider_task.activity = format!("provider error: {err}");
                        provider_task.progress = 100;
                        provider_task.updated_at = Some(now_millis());
                        provider_task.result = Some(rendered_error.clone());
                        self.upsert_agent_task(provider_task.clone());
                        return Err(progress.fail(rendered_error));
                    }
                };
                let mut observed_tool_call = false;
                let mut observed_text = false;
                for model_event in model_events {
                    match model_event {
                        ModelEvent::AssistantText { content } => {
                            if content.trim().is_empty() {
                                continue;
                            }
                            provider_task.status = AgentTaskStatus::Streaming;
                            provider_task.activity = "streaming assistant response".to_string();
                            provider_task.progress = provider_task.progress.max(65);
                            provider_task.updated_at = Some(now_millis());
                            self.upsert_agent_task(provider_task.clone());
                            observed_text = true;
                            let assistant = Message::new(Role::Assistant, &content);
                            self.messages.push(assistant.clone());
                            progress.carry(
                                self.store_entry(TranscriptEntry::Message { message: assistant }),
                            )?;
                            progress.engine_events.push(EngineEvent::Assistant(content));
                        }
                        ModelEvent::ToolCall(call) => {
                            observed_tool_call = true;
                            provider_task.status = AgentTaskStatus::RunningTool;
                            provider_task.activity = format!("requested tool `{}`", call.name);
                            provider_task.progress = provider_task.progress.max(75);
                            provider_task.updated_at = Some(now_millis());
                            self.upsert_agent_task(provider_task.clone());
                            let _ = self.handle_tool_call(
                                call,
                                approver,
                                &mut progress,
                                &mut turn_tasks,
                                on_approval_boundary,
                            )?;
                        }
                        ModelEvent::Usage(_) => {}
                        ModelEvent::Done => {}
                    }
                }
                if !observed_tool_call || observed_text {
                    break;
                }
            }
            provider_task.status = AgentTaskStatus::Done;
            provider_task.activity = "provider turn complete".to_string();
            provider_task.progress = 100;
            provider_task.updated_at = Some(now_millis());
            provider_task.result = Some("turn complete".to_string());
            self.upsert_agent_task(provider_task.clone());
            Ok(())
        })();

        if let Err(failure) = turn_result {
            self.terminalize_failed_turn_tasks(&turn_tasks, &failure.message);
            return Err(failure);
        }

        Ok(progress.finish())
    }

    fn terminalize_failed_turn_tasks(&mut self, turn_tasks: &TurnTaskTracker, message: &str) {
        // Only task identities registered by this turn are eligible. Tasks
        // that already reached a terminal state keep their original outcome.
        for task_id in &turn_tasks.task_ids {
            let Some(mut task) = self
                .runtime_tasks
                .iter()
                .find(|task| task.id == *task_id)
                .cloned()
            else {
                continue;
            };
            if !task.is_active() {
                continue;
            }
            task.status = AgentTaskStatus::Failed;
            task.activity = "turn stopped before task completion".to_string();
            task.progress = 100;
            task.result = Some(message.to_string());
            task.updated_at = Some(now_millis());
            self.upsert_agent_task(task);
        }
    }

    fn handle_tool_call<F>(
        &mut self,
        call: ToolCall,
        approver: &mut F,
        progress: &mut EngineTurnProgress,
        turn_tasks: &mut TurnTaskTracker,
        on_approval_boundary: &mut Option<&mut dyn FnMut(crate::EngineTurnOutput)>,
    ) -> Result<ToolResult, crate::EngineTurnFailure>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        let mut call = call;
        let reasoning_content = call.input.remove(PROVIDER_REASONING_CONTENT_KEY);
        let tool_spec = progress.carry(
            self.tools
                .spec(&call.name)
                .ok_or_else(|| format!("Model requested unknown tool `{}`", call.name)),
        )?;
        progress.carry(self.store_entry(TranscriptEntry::ToolCall { call: call.clone() }))?;
        let encoded_input = viden_types::encode_tool_input(&call.input);
        let mut task = self.tool_task(
            &call.id,
            &call.name,
            &encoded_input,
            AgentTaskStatus::RunningTool,
            "checking permissions",
            15,
        );
        turn_tasks.track(&task.id);
        self.upsert_agent_task(task.clone());
        let mut assistant_input = call.input.clone();
        if let Some(reasoning_content) = reasoning_content {
            assistant_input.insert(
                PROVIDER_REASONING_CONTENT_KEY.to_string(),
                reasoning_content,
            );
        }
        let assistant_tool_call = Message {
            id: fresh_id("msg"),
            role: Role::Assistant,
            content: viden_types::encode_tool_input(&assistant_input),
            timestamp: now_timestamp(),
            tool_name: Some(call.name.clone()),
            tool_call_id: Some(call.id.clone()),
        };
        self.messages.push(assistant_tool_call.clone());
        progress.carry(self.store_entry(TranscriptEntry::Message {
            message: assistant_tool_call,
        }))?;
        // Decide -> ask -> apply flows through the shared permission gate;
        // this closure only carries the call site's task and approval-boundary
        // bookkeeping around obtaining the operator response.
        let mut gate_permissions = self.permissions.clone();
        let decision = crate::permission_gate::resolve(
            &mut gate_permissions,
            &tool_spec,
            &call.name,
            &call.input,
            |_ask, mut prompt| {
                // The one site that holds the proposed tool input, so the
                // preview is computed here and rides the prompt to every
                // `ApprovalRequestView` construction site — including the
                // ordered boundary replayed below — instead of the input
                // being threaded through four call chains. Read-only: see
                // `decision_context`.
                prompt.decision_context = crate::decision_context::tool_decision_context(
                    &self.cwd,
                    &call.name,
                    &call.input,
                );
                task.status = AgentTaskStatus::WaitingApproval;
                task.activity = format!("waiting for approval: `{}`", call.name);
                task.progress = 35;
                task.permissions
                    .push("operator approval required".to_string());
                task.updated_at = Some(now_millis());
                self.upsert_agent_task(task.clone());
                let capture_ordered_approval = on_approval_boundary.is_none();
                // Publish every completed prefix before the supervisor opens the
                // next live approval boundary.
                if let Some(on_approval_boundary) = on_approval_boundary.as_deref_mut() {
                    let completed = progress.take_completed();
                    if !completed.engine_events.is_empty()
                        || !completed.ordered_runtime_facts.is_empty()
                    {
                        on_approval_boundary(completed);
                    }
                }
                let approval = approver(prompt.clone());
                if capture_ordered_approval {
                    progress
                        .ordered_approval_boundaries
                        .push(crate::OrderedApprovalBoundary {
                            before_engine_event_index: progress.engine_events.len(),
                            prompt,
                            decision: approval.decision.clone(),
                        });
                }
                approval
            },
        );

        match decision {
            PermissionDecision::Allow(allow) => {
                progress.carry(self.store_entry(TranscriptEntry::Permission {
                    entry: PermissionLogEntry {
                        timestamp: now_timestamp(),
                        tool_name: call.name.clone(),
                        decision: "allow".to_string(),
                        reason: format!("{:?}", allow.decision_reason),
                        message: allow.accept_feedback.clone(),
                    },
                }))?;
                // Frontend-active lifetime starts only after the permission
                // decision is durable, so an append failure cannot orphan it.
                record_started_tool(progress, &call, &encoded_input);
                task.status = AgentTaskStatus::RunningTool;
                task.activity = format!("running `{}`", call.name);
                task.progress = 55;
                task.decision = Some("allow".to_string());
                task.updated_at = Some(now_millis());
                self.upsert_agent_task(task.clone());
                let result = match self.tools.execute(
                    &call,
                    &ToolExecutionContext::local(self.cwd.clone()).with_semantic(Arc::new(
                        LspToolAdapter {
                            runtime: Arc::clone(&self.lsp_runtime),
                        },
                    )),
                ) {
                    Ok(result) => result,
                    Err(error) => ToolResult {
                        tool_call_id: call.id.clone(),
                        name: call.name.clone(),
                        output: format!("Tool `{}` failed: {error}", call.name),
                        diff: None,
                        success: false,
                        exit_code: None,
                    },
                };
                let post_edit_diagnostics = if result.success {
                    self.post_edit_diagnostics_message(&call)
                } else {
                    None
                };
                record_completed_tool(self, progress, &call, &result);
                progress.carry(self.persist_tool_result(&result))?;
                task.status = if result.success {
                    AgentTaskStatus::Done
                } else {
                    AgentTaskStatus::Failed
                };
                task.activity = if result.success {
                    format!("`{}` completed", call.name)
                } else {
                    format!("`{}` failed", call.name)
                };
                task.progress = 100;
                task.result = Some(result.output.clone());
                task.evidence.push(format!("success {}", result.success));
                if let Some(code) = result.exit_code {
                    task.evidence.push(format!("exit_code {code}"));
                }
                task.updated_at = Some(now_millis());
                self.upsert_agent_task(task);
                if let Some(message) = post_edit_diagnostics {
                    let system_message = Message::new(Role::System, message.clone());
                    self.messages.push(system_message.clone());
                    progress.carry(self.store_entry(TranscriptEntry::Message {
                        message: system_message,
                    }))?;
                    progress.engine_events.push(EngineEvent::System(message));
                }
                Ok(result)
            }
            PermissionDecision::Ask(_) => {
                unreachable!("ask decisions should be resolved before execution")
            }
            PermissionDecision::Deny(deny) => {
                let reason = format!("{:?}", deny.decision_reason);
                progress.carry(self.store_entry(TranscriptEntry::Permission {
                    entry: PermissionLogEntry {
                        timestamp: now_timestamp(),
                        tool_name: call.name.clone(),
                        decision: "deny".to_string(),
                        reason: reason.clone(),
                        message: Some(deny.message.clone()),
                    },
                }))?;
                record_started_tool(progress, &call, &encoded_input);
                task.status = AgentTaskStatus::Cancelled;
                task.activity = format!("denied `{}`", call.name);
                task.progress = 100;
                task.decision = Some("deny".to_string());
                task.result = Some(deny.message.clone());
                task.updated_at = Some(now_millis());
                self.upsert_agent_task(task);
                let rendered_denial = render_permission_denial(&call.name, &reason, &deny.message);
                let result = ToolResult {
                    tool_call_id: call.id.clone(),
                    name: call.name.clone(),
                    output: rendered_denial.clone(),
                    diff: None,
                    success: false,
                    exit_code: None,
                };
                record_completed_tool(self, progress, &call, &result);
                progress.carry(self.persist_tool_result(&result))?;
                let system_message = Message::new(Role::System, rendered_denial.clone());
                self.messages.push(system_message.clone());
                progress.carry(self.store_entry(TranscriptEntry::Message {
                    message: system_message,
                }))?;
                progress
                    .engine_events
                    .push(EngineEvent::System(rendered_denial));
                Ok(result)
            }
        }
    }

    fn post_edit_diagnostics_message(&self, call: &ToolCall) -> Option<String> {
        if !matches!(call.name.as_str(), "write_file" | "edit_file") {
            return None;
        }
        let path = std::path::Path::new(call.input.get("path")?);
        let diagnostics = self.lsp_runtime.diagnostics(&self.cwd, path).ok()?;
        if diagnostics.is_empty() {
            return None;
        }
        Some(format!(
            "Post-edit LSP diagnostics after `{}`:\n{}",
            call.name,
            render_lsp_diagnostics(&self.cwd, &diagnostics)
        ))
    }

    fn persist_tool_result(&mut self, result: &ToolResult) -> Result<(), String> {
        if let Some(diff) = &result.diff {
            self.last_diff = Some(diff.clone());
        }
        self.store_entry(TranscriptEntry::ToolResult {
            result: result.clone(),
        })?;
        let tool_message = Message {
            id: fresh_id("msg"),
            role: Role::Tool,
            content: result.output.clone(),
            timestamp: now_timestamp(),
            tool_name: Some(result.name.clone()),
            tool_call_id: Some(result.tool_call_id.clone()),
        };
        self.messages.push(tool_message);
        Ok(())
    }

    pub(crate) fn run_named_tool<F>(
        &mut self,
        tool_name: &str,
        input: viden_types::ToolInput,
        approver: &mut F,
    ) -> Result<String, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        let mut progress = EngineTurnProgress::default();
        let mut turn_tasks = TurnTaskTracker::default();
        let mut on_approval_boundary = None;
        let call = ToolCall {
            id: fresh_id("tool"),
            name: tool_name.to_string(),
            input,
        };
        if let Err(failure) = self.handle_tool_call(
            call,
            approver,
            &mut progress,
            &mut turn_tasks,
            &mut on_approval_boundary,
        ) {
            self.terminalize_failed_turn_tasks(&turn_tasks, &failure.message);
            return Err(failure.message);
        }
        let output = progress
            .engine_events
            .into_iter()
            .filter_map(|event| match event {
                EngineEvent::System(text)
                | EngineEvent::Assistant(text)
                | EngineEvent::Command(text) => Some(text),
                EngineEvent::ToolResult { output, .. } => Some(output),
                EngineEvent::ToolCall(_) => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        if output.trim().is_empty() {
            Ok("Tool completed".to_string())
        } else {
            Ok(output)
        }
    }

    pub(crate) fn run_named_tool_result<F>(
        &mut self,
        tool_name: &str,
        input: viden_types::ToolInput,
        approver: &mut F,
    ) -> Result<ToolResult, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        let mut progress = EngineTurnProgress::default();
        let mut turn_tasks = TurnTaskTracker::default();
        let mut on_approval_boundary = None;
        let call = ToolCall {
            id: fresh_id("tool"),
            name: tool_name.to_string(),
            input,
        };
        match self.handle_tool_call(
            call,
            approver,
            &mut progress,
            &mut turn_tasks,
            &mut on_approval_boundary,
        ) {
            Ok(result) => Ok(result),
            Err(failure) => {
                self.terminalize_failed_turn_tasks(&turn_tasks, &failure.message);
                Err(failure.message)
            }
        }
    }
}

fn aggregate_model_usage(events: &[ModelEvent]) -> Option<viden_types::ModelUsage> {
    events
        .iter()
        .filter_map(|event| match event {
            ModelEvent::Usage(usage) => Some(usage),
            _ => None,
        })
        .fold(None, |current, usage| {
            Some(match current {
                None => usage.clone(),
                Some(current) => viden_types::ModelUsage {
                    input_tokens: add_optional(current.input_tokens, usage.input_tokens),
                    output_tokens: add_optional(current.output_tokens, usage.output_tokens),
                    cached_input_tokens: add_optional(
                        current.cached_input_tokens,
                        usage.cached_input_tokens,
                    ),
                    retrieval_tokens: add_optional(
                        current.retrieval_tokens,
                        usage.retrieval_tokens,
                    ),
                    total_tokens: add_optional(current.total_tokens, usage.total_tokens),
                    cost_micro_usd: add_optional(current.cost_micro_usd, usage.cost_micro_usd),
                    actual_cost_micro_usd: add_optional(
                        current.actual_cost_micro_usd,
                        usage.actual_cost_micro_usd,
                    ),
                },
            })
        })
}

fn provider_attempt_cost_record(
    provider_id: &str,
    model: &str,
    attempt_index: u32,
    usage: Option<&ModelUsage>,
    outcome: CostUsageOutcome,
    attribution: CostAttribution,
) -> CostUsageRecord {
    let usage_id = attribution
        .request_id
        .clone()
        .unwrap_or_else(|| format!("provider-attempt-{attempt_index}"));
    let tokens = usage
        .map(|usage| TokenUsage {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            retrieval_tokens: usage.retrieval_tokens,
            total_tokens: usage.total_tokens,
        })
        .unwrap_or(TokenUsage {
            input_tokens: None,
            output_tokens: None,
            cached_input_tokens: None,
            retrieval_tokens: None,
            total_tokens: None,
        });
    CostUsageRecord {
        usage_id: usage_id.clone(),
        provider_id: provider_id.to_string(),
        model: model.to_string(),
        scopes: attribution.with_request_id(&usage_id).scopes(),
        estimate: usage.and_then(|usage| estimate_provider_cost(provider_id, model, usage)),
        actual_cost: usage
            .and_then(|usage| usage.actual_cost_micro_usd)
            .map(|micro_units| CostAmount {
                currency: "USD".to_string(),
                micro_units,
            }),
        tokens,
        attempt_index,
        outcome,
        recorded_at: Some(now_timestamp()),
    }
}

fn provider_attempt_usage_id(task_id: &str, attempt_index: u32) -> String {
    format!("{task_id}-provider-attempt-{attempt_index}")
}

pub(crate) fn estimate_provider_cost(
    provider_id: &str,
    model: &str,
    usage: &ModelUsage,
) -> Option<CostEstimate> {
    let price = price_table(provider_id, model)?;
    debug_assert!(!price.source_url.is_empty());
    if let Some(compatibility_until_utc) = price.compatibility_until_utc {
        debug_assert!(!compatibility_until_utc.is_empty());
    }
    let billable_input = usage
        .input_tokens
        .unwrap_or(0)
        .saturating_sub(usage.cached_input_tokens.unwrap_or(0));
    let cached = usage.cached_input_tokens.unwrap_or(0);
    let output = usage.output_tokens.unwrap_or(0);
    let micro_units = multiply_per_million(billable_input, price.input_micro_usd_per_million)
        .saturating_add(multiply_per_million(
            cached,
            price
                .cached_input_micro_usd_per_million
                .unwrap_or(price.input_micro_usd_per_million),
        ))
        .saturating_add(multiply_per_million(
            output,
            price.output_micro_usd_per_million,
        ));
    Some(CostEstimate {
        amount: CostAmount {
            currency: "USD".to_string(),
            micro_units,
        },
        provider_id: provider_id.to_string(),
        model: price.priced_model.to_string(),
        price_table_version: price.version.to_string(),
        estimated: true,
    })
}

fn multiply_per_million(tokens: u64, micro_usd_per_million: u64) -> u64 {
    let amount = u128::from(tokens).saturating_mul(u128::from(micro_usd_per_million)) / 1_000_000;
    amount.min(u128::from(u64::MAX)) as u64
}

#[derive(Clone, Copy)]
pub(crate) struct PriceRow {
    pub(crate) version: &'static str,
    pub(crate) source_url: &'static str,
    pub(crate) priced_model: &'static str,
    pub(crate) compatibility_until_utc: Option<&'static str>,
    pub(crate) input_micro_usd_per_million: u64,
    pub(crate) cached_input_micro_usd_per_million: Option<u64>,
    pub(crate) output_micro_usd_per_million: u64,
}

const DEEPSEEK_PRICING_VERSION: &str = "deepseek-pricing-2026-07-18";
const DEEPSEEK_PRICING_SOURCE_URL: &str = "https://api-docs.deepseek.com/quick_start/pricing/";
const DEEPSEEK_V4_COMPATIBILITY_UNTIL_UTC: &str = "2026-07-24T15:59:00Z";

pub(crate) fn price_table(provider_id: &str, model: &str) -> Option<PriceRow> {
    match (canonical_pricing_provider_id(provider_id), model) {
        ("sequence", "test-model") => Some(PriceRow {
            version: "test-2026-07-18",
            source_url: "test",
            priced_model: "test-model",
            compatibility_until_utc: None,
            input_micro_usd_per_million: 1_000_000,
            cached_input_micro_usd_per_million: Some(100_000),
            output_micro_usd_per_million: 2_000_000,
        }),
        ("deepseek", "deepseek-v4-flash" | "deepseek-chat" | "deepseek-reasoner") => {
            Some(PriceRow {
                version: DEEPSEEK_PRICING_VERSION,
                source_url: DEEPSEEK_PRICING_SOURCE_URL,
                priced_model: "deepseek-v4-flash",
                compatibility_until_utc: matches!(model, "deepseek-chat" | "deepseek-reasoner")
                    .then_some(DEEPSEEK_V4_COMPATIBILITY_UNTIL_UTC),
                input_micro_usd_per_million: 140_000,
                cached_input_micro_usd_per_million: Some(2_800),
                output_micro_usd_per_million: 280_000,
            })
        }
        ("deepseek", "deepseek-v4-pro") => Some(PriceRow {
            version: DEEPSEEK_PRICING_VERSION,
            source_url: DEEPSEEK_PRICING_SOURCE_URL,
            priced_model: "deepseek-v4-pro",
            compatibility_until_utc: None,
            input_micro_usd_per_million: 435_000,
            cached_input_micro_usd_per_million: Some(3_625),
            output_micro_usd_per_million: 870_000,
        }),
        _ => None,
    }
}

fn canonical_pricing_provider_id(provider_id: &str) -> &str {
    match provider_id {
        "deepseek" | "deepseek-anthropic" => "deepseek",
        provider_id => provider_id,
    }
}

fn add_optional(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn now_millis() -> u64 {
    now_timestamp().saturating_mul(1000)
}

impl SessionEngine {
    fn build_provider_request_messages_for_current_mode(
        &self,
        context_bundle: &ContextBundleRecord,
        retried_request_too_large: bool,
    ) -> Vec<Message> {
        #[cfg(test)]
        if matches!(
            self.context_benchmark_projection_mode,
            Some(crate::ContextBenchmarkProjectionMode::Off)
        ) {
            return build_provider_benchmark_baseline_request_messages(&self.messages);
        }

        if retried_request_too_large {
            build_provider_retry_request_messages(&self.messages, context_bundle)
        } else {
            build_provider_request_messages(&self.messages, context_bundle)
        }
    }

    #[cfg(test)]
    fn record_context_benchmark_metrics_for_test(
        &mut self,
        request_messages: &[Message],
        context_bundle: &ContextBundleRecord,
        context_build_elapsed: std::time::Duration,
        retry_count: u64,
    ) {
        let Some(mode) = self.context_benchmark_projection_mode else {
            return;
        };
        let request_input_chars = total_message_chars(request_messages);
        let projection_chars = match mode {
            crate::ContextBenchmarkProjectionMode::On => {
                render_provider_context_message(context_bundle)
                    .chars()
                    .count()
            }
            crate::ContextBenchmarkProjectionMode::Off => 0,
        };
        let raw_baseline_chars = total_message_chars(
            &build_provider_benchmark_baseline_request_messages(&self.messages),
        )
        .max(1);
        let compression_ratio = match mode {
            crate::ContextBenchmarkProjectionMode::Off => 1.0,
            crate::ContextBenchmarkProjectionMode::On => {
                request_input_chars as f64 / raw_baseline_chars as f64
            }
        };
        self.last_context_benchmark_metrics = Some(crate::ContextBenchmarkMetrics {
            request_input_chars,
            projection_chars,
            raw_baseline_chars,
            context_event_count: self.last_context_runtime_events.len(),
            retrieval_count: self
                .last_context_runtime_events
                .iter()
                .filter(|event| {
                    matches!(
                        event.kind,
                        viden_types::RuntimeEventKind::ContextRetrieved { .. }
                    )
                })
                .count(),
            retry_count,
            compression_ratio,
            bundle_build_ms: context_build_elapsed.as_millis(),
        });
    }
}

fn build_provider_request_messages(
    transcript: &[Message],
    context_bundle: &ContextBundleRecord,
) -> Vec<Message> {
    build_provider_request_messages_with_limits(
        transcript,
        context_bundle,
        ProviderRequestLimits::normal(),
        false,
    )
}

#[cfg(test)]
fn build_provider_benchmark_baseline_request_messages(transcript: &[Message]) -> Vec<Message> {
    fit_provider_request_budget(transcript.to_vec(), PROVIDER_REQUEST_CHAR_BUDGET)
}

fn build_provider_retry_request_messages(
    transcript: &[Message],
    context_bundle: &ContextBundleRecord,
) -> Vec<Message> {
    build_provider_request_messages_with_limits(
        transcript,
        context_bundle,
        ProviderRequestLimits::request_too_large_retry(),
        true,
    )
}

#[derive(Debug, Clone, Copy)]
struct ProviderRequestLimits {
    request_char_budget: usize,
    recent_history_limit: usize,
    history_message_char_limit: usize,
    tail_message_char_limit: usize,
    summary_line_limit: usize,
    summary_line_count: usize,
}

impl ProviderRequestLimits {
    fn normal() -> Self {
        Self {
            request_char_budget: PROVIDER_REQUEST_CHAR_BUDGET,
            recent_history_limit: PROVIDER_RECENT_HISTORY_LIMIT,
            history_message_char_limit: PROVIDER_HISTORY_MESSAGE_CHAR_LIMIT,
            tail_message_char_limit: PROVIDER_TAIL_MESSAGE_CHAR_LIMIT,
            summary_line_limit: PROVIDER_SUMMARY_LINE_LIMIT,
            summary_line_count: PROVIDER_SUMMARY_LINE_COUNT,
        }
    }

    fn request_too_large_retry() -> Self {
        Self {
            request_char_budget: PROVIDER_RETRY_REQUEST_CHAR_BUDGET,
            recent_history_limit: PROVIDER_RECENT_HISTORY_LIMIT / 2,
            history_message_char_limit: PROVIDER_RETRY_HISTORY_MESSAGE_CHAR_LIMIT,
            tail_message_char_limit: PROVIDER_RETRY_TAIL_MESSAGE_CHAR_LIMIT,
            summary_line_limit: PROVIDER_SUMMARY_LINE_LIMIT / 2,
            summary_line_count: PROVIDER_SUMMARY_LINE_COUNT / 2,
        }
    }
}

fn build_provider_request_messages_with_limits(
    transcript: &[Message],
    context_bundle: &ContextBundleRecord,
    limits: ProviderRequestLimits,
    request_too_large_retry: bool,
) -> Vec<Message> {
    let context_message = Message::new(
        Role::System,
        render_provider_context_message(context_bundle),
    );
    let Some(latest_user_index) = transcript
        .iter()
        .rposition(|message| message.role == Role::User)
    else {
        return vec![context_message];
    };

    let history = &transcript[..latest_user_index];
    let live_turn = &transcript[latest_user_index..];
    let mut request = Vec::new();
    if let Some(summary) = compact_transcript_summary(history, limits) {
        request.push(Message::new(Role::System, summary));
    }
    if request_too_large_retry {
        request.push(Message::new(
            Role::System,
            "Viden compacted provider request after a request-too-large error. The full transcript remains available in local audit storage.",
        ));
    }
    request.extend(recent_plain_history(history, limits));
    request.push(context_message);
    request.extend(
        live_turn
            .iter()
            .map(|message| compact_live_turn_message(message, limits)),
    );
    fit_provider_request_budget(request, limits.request_char_budget)
}

fn recent_plain_history(history: &[Message], limits: ProviderRequestLimits) -> Vec<Message> {
    let mut recent = Vec::new();
    for message in history.iter().rev() {
        if recent.len() >= limits.recent_history_limit {
            break;
        }
        if message.tool_name.is_some()
            || message.tool_call_id.is_some()
            || message.role == Role::Tool
        {
            continue;
        }
        let mut compacted = message.clone();
        compacted.content = compact_chars(&compacted.content, limits.history_message_char_limit);
        recent.push(compacted);
    }
    recent.reverse();
    recent
}

fn compact_transcript_summary(
    history: &[Message],
    limits: ProviderRequestLimits,
) -> Option<String> {
    if history.is_empty() {
        return None;
    }
    let omitted = history.len();
    let start = history.len().saturating_sub(limits.summary_line_count);
    let lines = history[start..]
        .iter()
        .map(|message| {
            let tool = message
                .tool_name
                .as_deref()
                .map(|name| format!(" tool={name}"))
                .unwrap_or_default();
            format!(
                "- {}{}: {}",
                message.role.as_str(),
                tool,
                one_line(&message.content, limits.summary_line_limit)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Some(format!(
        "Viden compacted transcript summary\nOmitted durable messages: {omitted}\nRecent omitted facts:\n{lines}\n\nThe full transcript remains in Viden storage; this summary is only the provider request view."
    ))
}

fn compact_live_turn_message(message: &Message, limits: ProviderRequestLimits) -> Message {
    let mut compacted = message.clone();
    compacted.content = compact_chars(&compacted.content, limits.tail_message_char_limit);
    compacted
}

fn fit_provider_request_budget(
    mut messages: Vec<Message>,
    request_char_budget: usize,
) -> Vec<Message> {
    while total_message_chars(&messages) > request_char_budget && messages.len() > 2 {
        let removable_recent_index = messages
            .iter()
            .position(|message| message.content.contains("Viden ContextBundle"))
            .filter(|context_index| *context_index > 1)
            .and_then(|context_index| {
                (1..context_index).find(|index| !is_provider_request_protected(&messages[*index]))
            });
        if let Some(index) = removable_recent_index {
            messages.remove(index);
        } else if let Some(index) = messages.iter().position(|message| {
            !is_provider_request_protected(message)
                && message.role != Role::User
                && message.tool_call_id.is_none()
        }) {
            messages.remove(index);
        } else {
            break;
        }
    }
    messages
}

fn is_provider_request_protected(message: &Message) -> bool {
    message
        .content
        .contains("Viden compacted transcript summary")
        || message.content.contains("Viden ContextBundle")
        || message
            .content
            .contains("Viden compacted provider request after a request-too-large error")
}

fn total_message_chars(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|message| message.content.chars().count())
        .sum()
}

fn compact_chars(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    let keep = max_chars.saturating_sub(96);
    let head = keep / 2;
    let tail = keep.saturating_sub(head);
    let start = input.chars().take(head).collect::<String>();
    let end = input
        .chars()
        .rev()
        .take(tail)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!(
        "{start}\n...[Viden compacted {} chars for provider request budget]...\n{end}",
        input.chars().count().saturating_sub(keep)
    )
}

fn one_line(input: &str, max_chars: usize) -> String {
    let collapsed = input.split_whitespace().collect::<Vec<_>>().join(" ");
    compact_chars(&collapsed, max_chars)
}
