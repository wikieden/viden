use viden_permissions::PermissionEngine;
use viden_session::{LoadedTranscript, SessionStore};
use viden_types::{
    AgentDagTaskSpec, AgentNextAction, CanonicalEvidenceReference, ConflictBounce,
    ContextBundleRecord, ContextItemRecord, ContextOmittedSourceRecord, ContextScope,
    ContextSourceRecord, ContractRecord, CostUsageRecord, DependencyRecord, HandoffRecord, Message,
    PermissionLevel, PermissionMode, RevertRecord, ReviewRequestRecord, Role, RuntimeEvent,
    RuntimeEventKind, SessionMetaEntry, SessionSummary, TranscriptEntry, WorkMode, fresh_id,
    now_timestamp, truncate_for_preview,
};
use viden_workflows::stores::WorkflowAgentEvent;

use crate::SessionEngine;

const WORKFLOW_PROJECTION_SCHEMA_VERSION: &str = "1";
const WORKFLOW_PROJECTION_EVENT_CAP: usize = 256;
const WORKFLOW_PROJECTION_BYTES_CAP: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeResumeRequest {
    ExactSessionId(String),
    Selector(String),
    Latest,
}

impl RuntimeResumeRequest {
    pub fn exact_session_id(session_id: impl Into<String>) -> Self {
        Self::ExactSessionId(session_id.into())
    }

    pub fn selector(selector: impl Into<String>) -> Self {
        Self::Selector(selector.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeResumeResult {
    pub session_id: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeResumeError {
    NotFound { selector: String },
    Ambiguous { selector: String, sessions: String },
    InvalidIndex { selector: String, message: String },
    Load(String),
}

impl std::fmt::Display for RuntimeResumeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { selector } => write!(formatter, "session `{selector}` not found"),
            Self::Ambiguous { selector, sessions } => {
                write!(
                    formatter,
                    "session selector `{selector}` is ambiguous.\n\n{sessions}"
                )
            }
            Self::InvalidIndex { message, .. } => formatter.write_str(message),
            Self::Load(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for RuntimeResumeError {}

impl SessionEngine {
    pub(super) fn handle_sessions(&self) -> Result<String, String> {
        let sessions = self.store.list_sessions_for_cwd()?;
        Ok(self.render_session_list(&sessions))
    }

    pub(super) fn handle_resume(&mut self, selector: Option<&str>) -> Result<String, String> {
        let Some(selector) = selector else {
            return self.handle_sessions();
        };
        if selector == "list" {
            return self.handle_sessions();
        }
        let request = match selector {
            "latest" => RuntimeResumeRequest::Latest,
            other => RuntimeResumeRequest::Selector(other.to_string()),
        };
        let resumed = match self.resume_session(request) {
            Ok(resumed) => resumed,
            Err(RuntimeResumeError::NotFound { .. }) => {
                return Ok("No resumable sessions found for the current project.".to_string());
            }
            Err(err) => return Err(err.to_string()),
        };
        Ok(format!(
            "Resumed session {} ({})",
            resumed.session_id,
            resumed.title.unwrap_or_else(|| "untitled".to_string())
        ))
    }

    pub fn resume_session(
        &mut self,
        request: RuntimeResumeRequest,
    ) -> Result<RuntimeResumeResult, RuntimeResumeError> {
        let (summary, loaded) = self.resolve_typed_resume_request(request)?;
        self.activate_resolved_session(summary, loaded)
    }

    pub(crate) fn resolve_typed_resume_request(
        &self,
        request: RuntimeResumeRequest,
    ) -> Result<(SessionSummary, LoadedTranscript), RuntimeResumeError> {
        match request {
            RuntimeResumeRequest::ExactSessionId(session_id) => self
                .store
                .load_by_id_for_cwd(&session_id)
                .map_err(RuntimeResumeError::Load)?
                .ok_or(RuntimeResumeError::NotFound {
                    selector: session_id,
                }),
            RuntimeResumeRequest::Latest => self
                .store
                .load_latest_for_cwd()
                .map_err(RuntimeResumeError::Load)?
                .ok_or(RuntimeResumeError::NotFound {
                    selector: "latest".to_string(),
                }),
            RuntimeResumeRequest::Selector(selector) => self.resolve_resume_selector(&selector),
        }
    }

    pub(crate) fn activate_resolved_session(
        &mut self,
        summary: SessionSummary,
        loaded: LoadedTranscript,
    ) -> Result<RuntimeResumeResult, RuntimeResumeError> {
        let resumed_store = SessionStore::new_with_home(
            self.store.home_dir().to_path_buf(),
            self.cwd.clone(),
            Some(summary.session_id.clone()),
        )
        .map_err(RuntimeResumeError::Load)?;
        let legacy_lanes_path = self.cwd.join(".viden").join("lanes.tsv");
        if legacy_lanes_path.is_file() {
            // A legacy TUI can write the TSV after this engine was created.
            // Import before activating the resumed session so migration failure
            // cannot leave the in-memory session boundary half-switched.
            self.workflows
                .import_legacy_lanes_tsv_once(
                    &legacy_lanes_path,
                    now_timestamp(),
                    Some(summary.session_id.clone()),
                )
                .map_err(RuntimeResumeError::Load)?;
        }
        self.store = resumed_store;
        self.messages.clear();
        self.last_diff = None;
        self.last_test = None;
        self.provider_cost_usage.clear();
        self.permissions
            .replace_engine(PermissionEngine::new(&self.cwd));
        self.hydrate(loaded).map_err(RuntimeResumeError::Load)?;
        self.hydrate_workflow_agent_projection()
            .map_err(RuntimeResumeError::Load)?;
        Ok(RuntimeResumeResult {
            session_id: summary.session_id,
            title: summary.title,
        })
    }

    fn resolve_resume_selector(
        &self,
        selector: &str,
    ) -> Result<(SessionSummary, LoadedTranscript), RuntimeResumeError> {
        let sessions = self
            .store
            .list_sessions_for_cwd()
            .map_err(RuntimeResumeError::Load)?;
        if sessions.is_empty() {
            return Err(RuntimeResumeError::NotFound {
                selector: selector.to_string(),
            });
        }

        if let Some(loaded) = self
            .store
            .load_by_id_for_cwd(selector)
            .map_err(RuntimeResumeError::Load)?
        {
            return Ok(loaded);
        }

        let matches: Vec<_> = sessions
            .iter()
            .filter(|summary| {
                summary.session_id != self.session_id()
                    && (summary.session_id.starts_with(selector)
                        || summary
                            .session_id
                            .trim_start_matches("session_")
                            .starts_with(selector))
            })
            .cloned()
            .collect();
        match matches.as_slice() {
            [] => self.resolve_resume_index(&sessions, selector),
            [summary] => {
                let loaded = SessionStore::load_transcript_from_path(std::path::Path::new(
                    &summary.transcript_path,
                ))
                .map_err(RuntimeResumeError::Load)?;
                Ok((summary.clone(), loaded))
            }
            _ => Err(RuntimeResumeError::Ambiguous {
                selector: selector.to_string(),
                sessions: self.render_session_list(matches.as_slice()),
            }),
        }
    }

    fn resolve_resume_index(
        &self,
        sessions: &[SessionSummary],
        selector: &str,
    ) -> Result<(SessionSummary, LoadedTranscript), RuntimeResumeError> {
        let index_selector = selector.strip_prefix('#').unwrap_or(selector);
        let Ok(index) = index_selector.parse::<usize>() else {
            return Err(RuntimeResumeError::NotFound {
                selector: selector.to_string(),
            });
        };
        if index == 0 {
            return Err(RuntimeResumeError::InvalidIndex {
                selector: selector.to_string(),
                message: "Session indexes start at 1.".to_string(),
            });
        }
        if let Some(summary) = sessions.get(index - 1) {
            let loaded = SessionStore::load_transcript_from_path(std::path::Path::new(
                &summary.transcript_path,
            ))
            .map_err(RuntimeResumeError::Load)?;
            return Ok((summary.clone(), loaded));
        }
        Err(RuntimeResumeError::InvalidIndex {
            selector: selector.to_string(),
            message: format!("No session found at index {index}."),
        })
    }

    /// Rebuilds in-memory session state from a replayed transcript.
    ///
    /// Lines this build could not replay are surfaced as a system fact before
    /// the history, so a shorter transcript is never mistaken for the whole
    /// truth. The lines themselves remain on disk: transcripts are append-only.
    fn hydrate(&mut self, loaded: LoadedTranscript) -> Result<(), String> {
        let LoadedTranscript {
            entries,
            quarantined,
        } = loaded;
        if !quarantined.is_empty() {
            let count = quarantined.len();
            let first = &quarantined[0];
            self.messages.push(Message::new(
                Role::System,
                format!(
                    "{count} transcript lines quarantined (forward-compatibility); first at line {}: {}",
                    first.line_number, first.reason
                ),
            ));
        }
        let mut provider_meta = None;
        let mut model_meta = None;
        for entry in entries {
            match entry {
                TranscriptEntry::Message { message } => self.messages.push(message),
                TranscriptEntry::ToolResult { result } => {
                    if let Some(diff) = result.diff.clone() {
                        self.last_diff = Some(diff);
                    }
                    self.messages.push(Message {
                        id: fresh_id("msg"),
                        role: Role::Tool,
                        content: result.output,
                        timestamp: now_timestamp(),
                        tool_name: Some(result.name),
                        tool_call_id: Some(result.tool_call_id),
                    });
                }
                TranscriptEntry::SessionMeta { entry } => match entry.key.as_str() {
                    "work_mode" => {
                        if let Some(mode) = WorkMode::parse_cli(&entry.value) {
                            self.runtime_snapshot.work_mode = mode;
                        }
                    }
                    "permission_mode" => {
                        if let Some(mode) = PermissionMode::parse_cli(&entry.value) {
                            self.permissions.set_mode(mode);
                            self.runtime_snapshot.permission_mode = mode;
                            self.runtime_snapshot.permission_level =
                                PermissionLevel::from_legacy_mode(mode);
                            if mode == PermissionMode::Plan {
                                self.runtime_snapshot.work_mode = WorkMode::Plan;
                            }
                        }
                    }
                    "model" => {
                        model_meta = Some(entry.value.clone());
                    }
                    "provider" => {
                        provider_meta = Some(entry.value.clone());
                    }
                    _ => {}
                },
                TranscriptEntry::CostUsage { cost } => self.remember_cost_usage(*cost),
                TranscriptEntry::RuntimeEvent { event } => {
                    self.remember_runtime_domain_event(*event)
                }
                _ => {}
            }
        }
        self.close_interrupted_tool_calls();
        self.restore_provider_meta(provider_meta.as_deref(), model_meta.as_deref())?;
        Ok(())
    }

    /// Closes tool calls that were persisted without a result.
    ///
    /// `handle_tool_call` stores the assistant tool-call message before the
    /// tool runs and the result only after it finishes, so a crash or kill in
    /// between leaves an assistant message whose `tool_call_id` no `Role::Tool`
    /// message answers. Several providers reject that request shape outright.
    ///
    /// The closure is synthesized in memory only. Loads stay read-only, the
    /// JSONL keeps exactly the facts that were durable when the process died,
    /// and because the synthesis is a pure function of the replayed entries the
    /// "model-visible history is reconstructible from the transcript alone"
    /// invariant still holds: every later load of the same file rebuilds the
    /// identical history.
    ///
    /// A dangling call can sit anywhere in history, not only at the tail: a
    /// resumed session appends to the same transcript, so an interrupted call
    /// from an earlier run stays embedded before the turns that followed it.
    /// Each closure is therefore inserted directly after the call it answers,
    /// and a single recovery note is appended once at the end of the history.
    fn close_interrupted_tool_calls(&mut self) {
        let mut last_result_index = std::collections::HashMap::new();
        for (index, message) in self.messages.iter().enumerate() {
            if message.role == Role::Tool
                && let Some(tool_call_id) = &message.tool_call_id
            {
                last_result_index.insert(tool_call_id.clone(), index);
            }
        }

        let mut closures = Vec::new();
        for (index, message) in self.messages.iter().enumerate() {
            if message.role != Role::Assistant {
                continue;
            }
            let Some(tool_call_id) = message.tool_call_id.clone() else {
                continue;
            };
            // Only a result recorded after the call answers it; an earlier
            // `Role::Tool` message with the same id belongs to another turn.
            if last_result_index
                .get(&tool_call_id)
                .is_some_and(|result_index| *result_index > index)
            {
                continue;
            }
            let tool_name = message
                .tool_name
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            closures.push((
                index,
                Message {
                    id: fresh_id("msg"),
                    role: Role::Tool,
                    content: format!(
                        "Tool call `{tool_name}` was interrupted before completion (session recovered after crash or kill); no result was produced."
                    ),
                    timestamp: now_timestamp(),
                    tool_name: Some(tool_name),
                    tool_call_id: Some(tool_call_id),
                },
            ));
        }

        if closures.is_empty() {
            return;
        }
        let closed = closures.len();
        // Insert back to front so the recorded indexes stay valid.
        for (index, closure) in closures.into_iter().rev() {
            self.messages.insert(index + 1, closure);
        }
        self.messages.push(Message::new(
            Role::System,
            format!("{closed} interrupted tool call(s) closed during session recovery"),
        ));
    }

    fn restore_provider_meta(
        &mut self,
        provider_id: Option<&str>,
        model: Option<&str>,
    ) -> Result<(), String> {
        if let Some(provider_id) = provider_id
            && self.provider_host.is_some()
        {
            let next_provider = self.create_provider_from_runtime(provider_id, model)?;
            self.provider = next_provider;
            self.runtime_snapshot.provider_family = provider_id.to_string();
            self.runtime_snapshot.model_label = self.provider.model().to_string();
            return Ok(());
        }
        if let Some(model) = model {
            self.provider.set_model(model.to_string());
            self.runtime_snapshot.model_label = self.provider.model().to_string();
        }
        self.runtime_snapshot.provider_family = self.provider.provider_name().to_string();
        Ok(())
    }

    pub(super) fn persist_meta(&self, key: &str, value: &str) -> Result<(), String> {
        self.store_entry(TranscriptEntry::SessionMeta {
            entry: SessionMetaEntry {
                timestamp: now_timestamp(),
                key: key.to_string(),
                value: value.to_string(),
            },
        })
    }

    pub(super) fn persist_meta_batch(&self, entries: &[(&str, &str)]) -> Result<(), String> {
        if entries.is_empty() {
            return Ok(());
        }
        #[cfg(test)]
        if let Some(remaining) = self.fail_transcript_append_after.get() {
            if remaining < entries.len() {
                self.fail_transcript_append_after.set(None);
                return Err("injected transcript append failure".to_string());
            }
            self.fail_transcript_append_after
                .set(Some(remaining - entries.len()));
        }

        let timestamp = now_timestamp();
        let entries = entries
            .iter()
            .map(|(key, value)| TranscriptEntry::SessionMeta {
                entry: SessionMetaEntry {
                    timestamp,
                    key: (*key).to_string(),
                    value: (*value).to_string(),
                },
            })
            .collect::<Vec<_>>();
        self.store.append_entries_atomic(&entries)
    }

    pub(super) fn store_entry(&self, entry: TranscriptEntry) -> Result<(), String> {
        #[cfg(test)]
        if let Some(remaining) = self.fail_transcript_append_after.get() {
            if remaining == 0 {
                self.fail_transcript_append_after.set(None);
                return Err("injected transcript append failure".to_string());
            }
            self.fail_transcript_append_after.set(Some(remaining - 1));
        }

        self.store.append_entry(&entry)
    }

    pub(super) fn record_cost_usage(&mut self, cost: CostUsageRecord) -> Result<(), String> {
        self.store_entry(TranscriptEntry::CostUsage {
            cost: Box::new(cost.clone()),
        })?;
        self.remember_cost_usage(cost);
        Ok(())
    }

    pub(super) fn remember_cost_usage(&mut self, cost: CostUsageRecord) {
        if self
            .provider_cost_usage
            .iter()
            .any(|existing| existing.usage_id == cost.usage_id)
        {
            return;
        }
        self.provider_cost_usage.push(cost);
    }

    pub(crate) fn persist_runtime_domain_events(
        &mut self,
        events: &[RuntimeEvent],
    ) -> Result<(), String> {
        let command_id = events.iter().find_map(|event| match &event.kind {
            RuntimeEventKind::CommandAccepted { command_id, .. }
            | RuntimeEventKind::CommandRejected { command_id, .. } => Some(command_id.clone()),
            _ => None,
        });
        let domain_events = events
            .iter()
            .filter(|event| is_durable_runtime_domain_event(&event.kind))
            .cloned()
            .collect::<Vec<_>>();
        self.persist_workflow_runtime_projection_batch(command_id.as_deref(), &domain_events)?;
        for event in domain_events {
            self.remember_runtime_domain_event(event);
        }
        Ok(())
    }

    /// Archives the durable facts of one supervised batch
    /// (`runtime.durable_work_evidence`, C7).
    ///
    /// The wiring that has never existed. `handle_runtime_command` has always
    /// ended by persisting its own domain facts, but a turn driven by
    /// `RuntimeSupervisor` published to the live event bus and stopped there, so
    /// every fact a supervised turn produced — the whole real-work path, which
    /// is the only one a cockpit uses — was live-only. An operator who watched
    /// an approved edit apply then found `QueryEvidence` empty, because the
    /// archive is rebuilt from the durable projection and nothing had written
    /// one (E1 defect 4, GUI-CORE-028).
    ///
    /// What is durable is decided by `is_durable_runtime_domain_event` and not
    /// by this caller, so the supervised path and the command path archive
    /// exactly the same set: `EvidenceRecorded` and `EvidenceCanonicalized` are
    /// in it, while `WorkspaceChangeUpdated` and `CheckRunUpdated` stay live-only
    /// — they are a cockpit's view of the working tree, re-sampled at every
    /// connect, and persisting them would replay a stale tree as fact.
    ///
    /// Fail-closed against the *events*, not against the turn: an append failure
    /// is returned so the caller can publish it, and no fact is remembered in
    /// memory that did not reach the log. Remembering a row the projection never
    /// received would give this process an archive its own restart cannot
    /// rebuild.
    pub(crate) fn absorb_supervised_events(
        &mut self,
        events: &[RuntimeEvent],
    ) -> Result<(), String> {
        self.persist_runtime_domain_events(events)
    }

    fn persist_workflow_runtime_projection_batch(
        &self,
        command_id: Option<&str>,
        events: &[RuntimeEvent],
    ) -> Result<(), String> {
        if events.is_empty() {
            return Ok(());
        }
        if events.len() > WORKFLOW_PROJECTION_EVENT_CAP {
            return Err(format!(
                "workflow projection batch exceeds event cap: {} > {}",
                events.len(),
                WORKFLOW_PROJECTION_EVENT_CAP
            ));
        }
        let sanitized_events = events
            .iter()
            .map(sanitized_runtime_event_for_workflow_projection)
            .collect::<Vec<_>>();
        let events_json =
            serde_json::to_string(&sanitized_events).map_err(|err| err.to_string())?;
        if events_json.len() > WORKFLOW_PROJECTION_BYTES_CAP {
            return Err(format!(
                "workflow projection batch exceeds byte cap: {} > {}",
                events_json.len(),
                WORKFLOW_PROJECTION_BYTES_CAP
            ));
        }
        let (dag_id, task_id) = self.workflow_projection_batch_identity(events);
        let batch_id = command_id
            .map(|id| format!("cmd:{id}"))
            .unwrap_or_else(|| fresh_id("projection_batch"));
        let kinds = sanitized_events
            .iter()
            .map(|event| runtime_projection_kind_name(&event.kind))
            .collect::<Vec<_>>();
        let mut payload = std::collections::BTreeMap::new();
        payload.insert(
            "schema_version".to_string(),
            WORKFLOW_PROJECTION_SCHEMA_VERSION.to_string(),
        );
        payload.insert("batch_id".to_string(), batch_id);
        if let Some(command_id) = command_id {
            payload.insert("command_id".to_string(), command_id.to_string());
        }
        payload.insert(
            "event_count".to_string(),
            sanitized_events.len().to_string(),
        );
        payload.insert(
            "runtime_event_kinds".to_string(),
            serde_json::to_string(&kinds).map_err(|err| err.to_string())?,
        );
        payload.insert("runtime_events_json".to_string(), events_json);
        let projection = WorkflowAgentEvent {
            event_id: fresh_id("agent_projection"),
            dag_id,
            task_id,
            event_type: "runtime_projection_batch".to_string(),
            timestamp: now_timestamp(),
            origin_session_id: Some(self.session_id().to_string()),
            payload,
        };
        if self.should_fail_workflow_append_for_test() {
            return Err("injected workflow append failure".to_string());
        }
        self.workflows.append_agent_event(&projection)
    }

    fn workflow_projection_batch_identity(
        &self,
        events: &[RuntimeEvent],
    ) -> (String, Option<String>) {
        let mut dag_id = None;
        let mut task_id = None;
        for event in events {
            let Some((event_dag_id, event_task_id)) = self.workflow_projection_identity(event)
            else {
                continue;
            };
            dag_id.get_or_insert(event_dag_id);
            if task_id.is_none() {
                task_id = event_task_id;
            } else if task_id != event_task_id {
                task_id = None;
                break;
            }
        }
        (
            dag_id.unwrap_or_else(|| "project-agent-runtime".to_string()),
            task_id,
        )
    }

    fn workflow_projection_identity(
        &self,
        event: &RuntimeEvent,
    ) -> Option<(String, Option<String>)> {
        match &event.kind {
            RuntimeEventKind::AgentDagUpdated { dag } => Some((dag.dag_id.clone(), None)),
            RuntimeEventKind::TaskUpdated { task } => Some((
                self.dag_id_for_task_projection(&task.id)
                    .unwrap_or_else(|| "project-agent-runtime".to_string()),
                Some(task.id.clone()),
            )),
            RuntimeEventKind::EvidenceRecorded { evidence } => {
                let task_id = evidence
                    .canonical
                    .as_ref()
                    .map(|canonical| canonical.producer.task_id.clone())
                    .or_else(|| self.task_id_for_evidence(&evidence.id));
                Some((
                    task_id
                        .as_deref()
                        .and_then(|id| self.dag_id_for_task_projection(id))
                        .unwrap_or_else(|| "project-agent-runtime".to_string()),
                    task_id,
                ))
            }
            RuntimeEventKind::EvidenceCanonicalized { evidence_id, .. } => {
                let task_id = self
                    .runtime_evidence
                    .iter()
                    .find(|evidence| evidence.id == *evidence_id)
                    .and_then(|evidence| {
                        evidence
                            .canonical
                            .as_ref()
                            .map(|canonical| canonical.producer.task_id.clone())
                    })
                    .or_else(|| self.task_id_for_evidence(evidence_id));
                Some((
                    task_id
                        .as_deref()
                        .and_then(|id| self.dag_id_for_task_projection(id))
                        .unwrap_or_else(|| "project-agent-runtime".to_string()),
                    task_id,
                ))
            }
            RuntimeEventKind::MergeGateUpdated { gate } => Some((
                self.dag_id_for_task_projection(&gate.task_id)
                    .unwrap_or_else(|| "project-agent-runtime".to_string()),
                Some(gate.task_id.clone()),
            )),
            RuntimeEventKind::HandoffUpdated { handoff } => Some((
                self.dag_id_for_task_projection(&handoff.task_id)
                    .unwrap_or_else(|| "project-agent-runtime".to_string()),
                Some(handoff.task_id.clone()),
            )),
            RuntimeEventKind::ReviewRequestUpdated { review } => Some((
                self.dag_id_for_task_projection(&review.task_id)
                    .unwrap_or_else(|| "project-agent-runtime".to_string()),
                Some(review.task_id.clone()),
            )),
            RuntimeEventKind::ContractUpdated { contract } => Some((
                self.dag_id_for_task_projection(&contract.task_id)
                    .unwrap_or_else(|| "project-agent-runtime".to_string()),
                Some(contract.task_id.clone()),
            )),
            RuntimeEventKind::DependencyUpdated { dependency } => Some((
                self.dag_id_for_task_projection(&dependency.task_id)
                    .unwrap_or_else(|| "project-agent-runtime".to_string()),
                Some(dependency.task_id.clone()),
            )),
            RuntimeEventKind::MergeConflictBounced { conflict } => Some((
                self.dag_id_for_task_projection(&conflict.task_id)
                    .unwrap_or_else(|| "project-agent-runtime".to_string()),
                Some(conflict.task_id.clone()),
            )),
            RuntimeEventKind::RevertRecorded { revert } => {
                let task_id = self
                    .runtime_merge_gates
                    .iter()
                    .find(|gate| gate.gate_id == revert.gate_id)
                    .map(|gate| gate.task_id.clone());
                Some((
                    task_id
                        .as_deref()
                        .and_then(|id| self.dag_id_for_task_projection(id))
                        .unwrap_or_else(|| "project-agent-runtime".to_string()),
                    task_id,
                ))
            }
            RuntimeEventKind::ContextBundleBuilt { scope, .. } => {
                let task_id = task_id_for_context_scope(scope);
                Some((
                    task_id
                        .as_deref()
                        .and_then(|id| self.dag_id_for_task_projection(id))
                        .unwrap_or_else(|| "project-agent-runtime".to_string()),
                    task_id,
                ))
            }
            RuntimeEventKind::ContextItemStored { item } => {
                let task_id = task_id_for_context_scope(&item.scope);
                Some((
                    task_id
                        .as_deref()
                        .and_then(|id| self.dag_id_for_task_projection(id))
                        .unwrap_or_else(|| "project-agent-runtime".to_string()),
                    task_id,
                ))
            }
            RuntimeEventKind::ContextViewDerived { handle, .. } => {
                let task_id = task_id_for_context_scope(&handle.scope);
                Some((
                    task_id
                        .as_deref()
                        .and_then(|id| self.dag_id_for_task_projection(id))
                        .unwrap_or_else(|| "project-agent-runtime".to_string()),
                    task_id,
                ))
            }
            RuntimeEventKind::ContextBudgetExceeded { budget } => {
                let task_id = task_id_for_context_scope(&budget.scope);
                Some((
                    task_id
                        .as_deref()
                        .and_then(|id| self.dag_id_for_task_projection(id))
                        .unwrap_or_else(|| "project-agent-runtime".to_string()),
                    task_id,
                ))
            }
            RuntimeEventKind::ContextQualityFailed { .. }
            | RuntimeEventKind::ContextUpdated { .. } => {
                Some(("project-agent-runtime".to_string(), None))
            }
            _ => None,
        }
    }

    fn dag_id_for_task_projection(&self, task_id: &str) -> Option<String> {
        self.runtime_agent_dags
            .iter()
            .find(|dag| dag.tasks.iter().any(|task| task.task_id == task_id))
            .map(|dag| dag.dag_id.clone())
    }

    fn task_id_for_evidence(&self, evidence_id: &str) -> Option<String> {
        self.runtime_merge_gates
            .iter()
            .find(|gate| gate.evidence_ids.iter().any(|id| id == evidence_id))
            .map(|gate| gate.task_id.clone())
    }

    fn hydrate_workflow_agent_projection(&mut self) -> Result<(), String> {
        let events = self.workflows.load_agent_events()?;
        let mut seen_batches = std::collections::BTreeSet::new();
        for event in events {
            match event.event_type.as_str() {
                "runtime_projection" => {
                    let Some(json) = event.payload.get("runtime_event_json") else {
                        continue;
                    };
                    let runtime_event = serde_json::from_str::<RuntimeEvent>(json)
                        .map_err(|err| err.to_string())?;
                    self.remember_runtime_domain_event(runtime_event);
                }
                "runtime_projection_batch" => {
                    if event.payload.get("schema_version").map(String::as_str)
                        != Some(WORKFLOW_PROJECTION_SCHEMA_VERSION)
                    {
                        continue;
                    }
                    let Some(batch_id) = event.payload.get("batch_id") else {
                        continue;
                    };
                    if !seen_batches.insert(batch_id.clone()) {
                        continue;
                    }
                    let expected_count = event
                        .payload
                        .get("event_count")
                        .and_then(|count| count.parse::<usize>().ok());
                    let Some(json) = event.payload.get("runtime_events_json") else {
                        continue;
                    };
                    if json.len() > WORKFLOW_PROJECTION_BYTES_CAP {
                        return Err(format!(
                            "workflow projection batch exceeds byte cap: {} > {}",
                            json.len(),
                            WORKFLOW_PROJECTION_BYTES_CAP
                        ));
                    }
                    let runtime_events = serde_json::from_str::<Vec<RuntimeEvent>>(json)
                        .map_err(|err| err.to_string())?;
                    if runtime_events.len() > WORKFLOW_PROJECTION_EVENT_CAP
                        || expected_count != Some(runtime_events.len())
                    {
                        continue;
                    }
                    for runtime_event in runtime_events {
                        self.remember_runtime_domain_event(runtime_event);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(crate) fn should_fail_workflow_append_for_test(&self) -> bool {
        #[cfg(test)]
        {
            if self.fail_next_workflow_append.replace(false) {
                return true;
            }
            if let Some(remaining) = self.fail_workflow_append_after.get() {
                if remaining == 0 {
                    self.fail_workflow_append_after.set(None);
                    return true;
                }
                self.fail_workflow_append_after.set(Some(remaining - 1));
            }
        }
        false
    }

    fn remember_runtime_domain_event(&mut self, event: RuntimeEvent) {
        match event.kind {
            RuntimeEventKind::AgentDagUpdated { dag } => {
                if let Some(existing) = self
                    .runtime_agent_dags
                    .iter_mut()
                    .find(|existing| existing.dag_id == dag.dag_id)
                {
                    *existing = dag;
                } else {
                    self.runtime_agent_dags.push(dag);
                }
            }
            RuntimeEventKind::TaskUpdated { task } => self.upsert_agent_task(task),
            RuntimeEventKind::ContextBundleBuilt { .. }
            | RuntimeEventKind::ContextItemStored { .. }
            | RuntimeEventKind::ContextViewDerived { .. }
            | RuntimeEventKind::ContextReductionRecorded { .. }
            | RuntimeEventKind::ContextBudgetExceeded { .. }
            | RuntimeEventKind::ContextQualityFailed { .. } => {
                self.remember_context_runtime_event(event);
            }
            RuntimeEventKind::ContextUpdated { context } => {
                self.last_context_bundle = Some(context);
            }
            RuntimeEventKind::EvidenceRecorded { evidence } => {
                self.upsert_runtime_evidence(evidence)
            }
            RuntimeEventKind::MergeGateUpdated { gate } => {
                if let Some(existing) = self
                    .runtime_merge_gates
                    .iter_mut()
                    .find(|existing| existing.gate_id == gate.gate_id)
                {
                    *existing = gate;
                } else {
                    self.runtime_merge_gates.push(gate);
                }
            }
            RuntimeEventKind::HandoffUpdated { handoff } => {
                upsert_handoff(&mut self.runtime_handoffs, handoff)
            }
            RuntimeEventKind::ReviewRequestUpdated { review } => {
                upsert_review(&mut self.runtime_review_requests, review)
            }
            RuntimeEventKind::ContractUpdated { contract } => {
                upsert_contract(&mut self.runtime_contracts, contract)
            }
            RuntimeEventKind::DependencyUpdated { dependency } => {
                upsert_dependency(&mut self.runtime_dependencies, dependency)
            }
            RuntimeEventKind::MergeConflictBounced { conflict } => {
                upsert_conflict(&mut self.runtime_conflict_bounces, conflict)
            }
            RuntimeEventKind::RevertRecorded { revert } => {
                upsert_revert(&mut self.runtime_reverts, revert)
            }
            RuntimeEventKind::ProjectConfigConfirmed { preview } => {
                self.confirmed_project_config = Some(preview);
            }
            RuntimeEventKind::CredentialHandleStored { handle } => {
                if let Some(existing) = self.credential_handles.iter_mut().find(|existing| {
                    existing.provider_id == handle.provider_id
                        && existing.backend_id == handle.backend_id
                }) {
                    *existing = handle;
                } else {
                    self.credential_handles.push(handle);
                }
            }
            RuntimeEventKind::EvidenceCanonicalized { .. } => {}
            _ => {}
        }
    }

    fn remember_context_runtime_event(&mut self, event: RuntimeEvent) {
        if !self
            .last_context_runtime_events
            .iter()
            .any(|existing| existing.kind == event.kind)
        {
            self.last_context_runtime_events.push(event);
        }
    }
}

fn task_id_for_context_scope(scope: &viden_types::ContextScope) -> Option<String> {
    match scope {
        viden_types::ContextScope::Task(task_id) => Some(task_id.clone()),
        _ => None,
    }
}

fn runtime_projection_kind_name(kind: &RuntimeEventKind) -> &'static str {
    match kind {
        RuntimeEventKind::AgentDagUpdated { .. } => "agent_dag_updated",
        RuntimeEventKind::TaskUpdated { .. } => "task_updated",
        RuntimeEventKind::ContextBundleBuilt { .. } => "context_bundle_built",
        RuntimeEventKind::ContextItemStored { .. } => "context_item_stored",
        RuntimeEventKind::ContextViewDerived { .. } => "context_view_derived",
        RuntimeEventKind::ContextReductionRecorded { .. } => "context_reduction_recorded",
        RuntimeEventKind::ContextBudgetExceeded { .. } => "context_budget_exceeded",
        RuntimeEventKind::ContextQualityFailed { .. } => "context_quality_failed",
        RuntimeEventKind::ContextUpdated { .. } => "context_updated",
        RuntimeEventKind::EvidenceRecorded { .. } => "evidence_recorded",
        RuntimeEventKind::EvidenceCanonicalized { .. } => "evidence_canonicalized",
        RuntimeEventKind::MergeGateUpdated { .. } => "merge_gate_updated",
        RuntimeEventKind::HandoffUpdated { .. } => "handoff_updated",
        RuntimeEventKind::ReviewRequestUpdated { .. } => "review_request_updated",
        RuntimeEventKind::ContractUpdated { .. } => "contract_updated",
        RuntimeEventKind::DependencyUpdated { .. } => "dependency_updated",
        RuntimeEventKind::MergeConflictBounced { .. } => "merge_conflict_bounced",
        RuntimeEventKind::RevertRecorded { .. } => "revert_recorded",
        RuntimeEventKind::ProjectConfigConfirmed { .. } => "project_config_confirmed",
        RuntimeEventKind::CredentialHandleStored { .. } => "credential_handle_stored",
        _ => "runtime_event",
    }
}

fn sanitized_runtime_event_for_workflow_projection(event: &RuntimeEvent) -> RuntimeEvent {
    let mut event = event.clone();
    match &mut event.kind {
        RuntimeEventKind::AgentDagUpdated { dag } => {
            dag.goal = sanitize_projection_text(&dag.goal, 160);
            for task in &mut dag.tasks {
                sanitize_projection_task_spec(task);
            }
        }
        RuntimeEventKind::TaskUpdated { task } => {
            task.title = sanitize_projection_text(&task.title, 160);
            task.activity = sanitize_projection_text(&task.activity, 240);
            task.summary = sanitize_projection_text(&task.summary, 500);
            task.workspace = task
                .workspace
                .as_deref()
                .and_then(safe_project_relative_projection_path);
            task.decision = task
                .decision
                .as_deref()
                .map(|decision| sanitize_projection_text(decision, 240))
                .filter(|decision| !decision.is_empty());
            task.result = task
                .result
                .as_deref()
                .map(|result| sanitize_projection_text(result, 500))
                .filter(|result| !result.is_empty());
            if let Some(next_action) = &mut task.next_action {
                sanitize_projection_next_action(next_action);
            }
        }
        RuntimeEventKind::EvidenceRecorded { evidence } => {
            evidence.summary = sanitize_projection_text(&evidence.summary, 500);
            evidence.source = evidence
                .source
                .as_deref()
                .map(|source| sanitize_projection_text(source, 120))
                .filter(|source| !source.is_empty());
            evidence.path = evidence
                .path
                .as_deref()
                .and_then(safe_project_relative_projection_path);
            if let Some(canonical) = &mut evidence.canonical {
                sanitize_projection_canonical_reference(canonical);
            }
        }
        RuntimeEventKind::EvidenceCanonicalized {
            item_id,
            content_sha256,
            ..
        } => {
            *item_id = sanitize_projection_identifier(item_id, 128);
            *content_sha256 = sanitize_projection_hash(content_sha256);
        }
        RuntimeEventKind::MergeGateUpdated { gate } => {
            if let Some(decision) = &mut gate.decision {
                decision.reason = sanitize_projection_text(&decision.reason, 240);
            }
            if let Some(conflict) = &mut gate.conflict {
                conflict.reason = sanitize_projection_text(&conflict.reason, 500);
            }
        }
        RuntimeEventKind::HandoffUpdated { handoff } => {
            handoff.summary = sanitize_projection_text(&handoff.summary, 500);
        }
        RuntimeEventKind::ContractUpdated { contract } => {
            contract.summary = sanitize_projection_text(&contract.summary, 500);
        }
        RuntimeEventKind::DependencyUpdated { dependency } => {
            dependency.reason = sanitize_projection_text(&dependency.reason, 240);
        }
        RuntimeEventKind::MergeConflictBounced { conflict } => {
            conflict.reason = sanitize_projection_text(&conflict.reason, 500);
        }
        RuntimeEventKind::RevertRecorded { revert } => {
            revert.reason = sanitize_projection_text(&revert.reason, 500);
        }
        RuntimeEventKind::ContextUpdated { context } => sanitize_projection_context(context),
        RuntimeEventKind::ContextItemStored { item } => sanitize_projection_context_item(item),
        _ => {}
    }
    event
}

fn sanitize_projection_task_spec(task: &mut AgentDagTaskSpec) {
    task.title = sanitize_projection_text(&task.title, 160);
    task.objective = sanitize_projection_text(&task.objective, 240);
    task.workspace = task
        .workspace
        .as_deref()
        .and_then(safe_project_relative_projection_path);
    task.file_scope = task
        .file_scope
        .iter()
        .filter_map(|scope| safe_project_relative_projection_path(scope))
        .collect();
}

fn sanitize_projection_next_action(next_action: &mut AgentNextAction) {
    next_action.label = sanitize_projection_text(&next_action.label, 80);
    next_action.command = next_action
        .command
        .as_deref()
        .map(|command| sanitize_projection_text(command, 120))
        .filter(|command| !command.is_empty());
    next_action.reason = next_action
        .reason
        .as_deref()
        .map(|reason| sanitize_projection_text(reason, 160))
        .filter(|reason| !reason.is_empty());
}

fn sanitize_projection_context(context: &mut ContextBundleRecord) {
    context.policy = sanitize_projection_text(&context.policy, 120);
    for source in &mut context.sources {
        sanitize_projection_context_source(source);
    }
    for omitted in &mut context.omitted_sources {
        sanitize_projection_omitted_source(omitted);
    }
    context.largest_sources = context
        .largest_sources
        .iter()
        .map(|source| sanitize_projection_text(source, 120))
        .collect();
    context.compaction_notes = context
        .compaction_notes
        .iter()
        .map(|note| sanitize_projection_text(note, 240))
        .collect();
}

fn sanitize_projection_context_source(source: &mut ContextSourceRecord) {
    source.name = sanitize_projection_text(&source.name, 120);
    source.kind = sanitize_projection_text(&source.kind, 80);
    source.summary = sanitize_projection_text(&source.summary, 240);
    source.include_reason = sanitize_projection_text(&source.include_reason, 160);
}

fn sanitize_projection_omitted_source(source: &mut ContextOmittedSourceRecord) {
    source.name = sanitize_projection_text(&source.name, 120);
    source.kind = sanitize_projection_text(&source.kind, 80);
    source.reason = sanitize_projection_text(&source.reason, 160);
}

fn sanitize_projection_context_item(item: &mut ContextItemRecord) {
    item.title = sanitize_projection_text(&item.title, 120);
    item.summary = sanitize_projection_text(&item.summary, 240);
}

fn sanitize_projection_canonical_reference(canonical: &mut CanonicalEvidenceReference) {
    canonical.item_id = sanitize_projection_identifier(&canonical.item_id, 128);
    canonical.bundle_id = sanitize_projection_identifier(&canonical.bundle_id, 128);
    canonical.source_hash = sanitize_projection_hash(&canonical.source_hash);
    canonical.producer.identity = sanitize_projection_identifier(&canonical.producer.identity, 64);
    canonical.producer.role = sanitize_projection_identifier(&canonical.producer.role, 32);
    canonical.producer.task_id = sanitize_projection_identifier(&canonical.producer.task_id, 128);
    canonical.permission_snapshot_id = canonical
        .permission_snapshot_id
        .as_deref()
        .map(|id| sanitize_projection_identifier(id, 128))
        .filter(|id| id != "[REDACTED]");
    sanitize_projection_scope(&mut canonical.permission_scope);
    sanitize_projection_scope(&mut canonical.evidence_scope);
}

fn sanitize_projection_scope(scope: &mut ContextScope) {
    match scope {
        ContextScope::Task(id) | ContextScope::Dag(id) | ContextScope::Workflow(id) => {
            *id = sanitize_projection_identifier(id, 128);
        }
    }
}

fn sanitize_projection_hash(value: &str) -> String {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        value.to_string()
    } else {
        "[REDACTED]".to_string()
    }
}

fn sanitize_projection_identifier(value: &str, max_len: usize) -> String {
    if is_safe_projection_identifier(value, max_len) {
        value.to_string()
    } else {
        "[REDACTED]".to_string()
    }
}

fn is_safe_projection_identifier(value: &str, max_len: usize) -> bool {
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
    value.bytes().all(|byte| {
        matches!(
            byte,
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b':'
        )
    })
}

fn sanitize_projection_text(input: &str, max_chars: usize) -> String {
    if input.to_ascii_lowercase().contains("diff --git") {
        return "[REDACTED]".to_string();
    }
    let sanitized = input
        .split_whitespace()
        .map(|word| {
            let lower = word.to_ascii_lowercase();
            if lower.starts_with("sk-")
                || lower.contains("secret")
                || lower.contains("token=")
                || lower.contains("api_key")
                || word.starts_with('/')
                || word.contains("/Users/")
                || word.contains("/tmp/")
                || word.contains("/var/")
                || word.chars().any(char::is_control)
            {
                "[REDACTED]"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    truncate_for_preview(&sanitized, max_chars)
}

fn safe_project_relative_projection_path(path: &str) -> Option<String> {
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
        return None;
    }
    Some(
        trimmed
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn upsert_handoff(records: &mut Vec<HandoffRecord>, record: HandoffRecord) {
    if let Some(existing) = records
        .iter_mut()
        .find(|existing| existing.handoff_id == record.handoff_id)
    {
        *existing = record;
    } else {
        records.push(record);
    }
}

fn upsert_review(records: &mut Vec<ReviewRequestRecord>, record: ReviewRequestRecord) {
    if let Some(existing) = records
        .iter_mut()
        .find(|existing| existing.review_id == record.review_id)
    {
        *existing = record;
    } else {
        records.push(record);
    }
}

fn upsert_contract(records: &mut Vec<ContractRecord>, record: ContractRecord) {
    if let Some(existing) = records
        .iter_mut()
        .find(|existing| existing.contract_id == record.contract_id)
    {
        *existing = record;
    } else {
        records.push(record);
    }
}

fn upsert_dependency(records: &mut Vec<DependencyRecord>, record: DependencyRecord) {
    if let Some(existing) = records
        .iter_mut()
        .find(|existing| existing.dependency_id == record.dependency_id)
    {
        *existing = record;
    } else {
        records.push(record);
    }
}

fn upsert_conflict(records: &mut Vec<ConflictBounce>, record: ConflictBounce) {
    if let Some(existing) = records
        .iter_mut()
        .find(|existing| existing.bounce_id == record.bounce_id)
    {
        *existing = record;
    } else {
        records.push(record);
    }
}

fn upsert_revert(records: &mut Vec<RevertRecord>, record: RevertRecord) {
    if let Some(existing) = records
        .iter_mut()
        .find(|existing| existing.revert_id == record.revert_id)
    {
        *existing = record;
    } else {
        records.push(record);
    }
}

fn is_durable_runtime_domain_event(kind: &RuntimeEventKind) -> bool {
    // These facts rebuild runtime DAG/evidence/gate state. Transient command
    // acknowledgements stay out of JSONL so replay remains append-only domain state.
    matches!(
        kind,
        RuntimeEventKind::AgentDagUpdated { .. }
            | RuntimeEventKind::TaskUpdated { .. }
            | RuntimeEventKind::ContextBundleBuilt { .. }
            | RuntimeEventKind::ContextItemStored { .. }
            | RuntimeEventKind::ContextViewDerived { .. }
            | RuntimeEventKind::ContextReductionRecorded { .. }
            | RuntimeEventKind::ContextBudgetExceeded { .. }
            | RuntimeEventKind::ContextQualityFailed { .. }
            | RuntimeEventKind::ContextUpdated { .. }
            | RuntimeEventKind::EvidenceRecorded { .. }
            | RuntimeEventKind::EvidenceCanonicalized { .. }
            | RuntimeEventKind::MergeGateUpdated { .. }
            | RuntimeEventKind::HandoffUpdated { .. }
            | RuntimeEventKind::ReviewRequestUpdated { .. }
            | RuntimeEventKind::ContractUpdated { .. }
            | RuntimeEventKind::DependencyUpdated { .. }
            | RuntimeEventKind::MergeConflictBounced { .. }
            | RuntimeEventKind::RevertRecorded { .. }
            | RuntimeEventKind::ProjectConfigConfirmed { .. }
            | RuntimeEventKind::CredentialHandleStored { .. }
    )
}
