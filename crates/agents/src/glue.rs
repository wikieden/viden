use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, ChildStdin},
    sync::{Arc, Mutex, OnceLock, mpsc},
    time::{Duration, Instant},
};

use super::acp::*;
use super::codex::*;
use super::infra::*;
use super::render::*;
use crate::RuntimeEventSink;
use serde_json::Value;
use viden_permissions::PermissionContext;
use viden_plugin_api::{AgentAuthMode, AgentPluginDescriptor, AgentSource, AgentTransport};
use viden_types::{
    AgentAdapterSource, AgentAdapterView, AgentAuthState, AgentAvailability, AgentNextAction,
    AgentRole, AgentRoute, AgentSessionRequest, AgentSessionStatus, AgentSessionView,
    AgentStartability, AgentTaskKind, AgentTaskRecord, AgentTaskStatus, ApprovalResponse,
    CapabilityId, EvidenceView, MergeGateDecision, MergeGateDecisionOutcome,
    MergeGatePolicySnapshot, MergeGateRecord, MergeGateStatus, MergeGateType, RuntimeEvent,
    RuntimeEventKind, RuntimeOwner, fresh_id, now_timestamp, truncate_for_preview,
};

pub(super) const MAX_RESIDENT_ACP_SESSIONS: usize = 8;

pub(super) const RESIDENT_ACP_SESSION_IDLE_TTL: Duration = Duration::from_secs(15 * 60);

/// How a typed agent session carries an `Ask` out to an operator.
///
/// Owned (`Box<..> + Send`) rather than borrowed because a session outlives
/// the call that starts it and may prompt from its own thread.
pub type AgentSessionApprover =
    Box<dyn FnMut(viden_types::PermissionPrompt) -> ApprovalResponse + Send + 'static>;

/// Project every registered agent adapter into the frontend-facing view,
/// without probing any of them.
pub fn typed_agent_adapter_views() -> Vec<AgentAdapterView> {
    acp_agent_descriptors()
        .iter()
        .map(typed_agent_adapter_view)
        .collect()
}

/// Probe one adapter by id and project the observed availability, auth state,
/// and startability into its view.
pub fn probe_typed_agent_adapter(cwd: &Path, agent_id: &str) -> Result<AgentAdapterView, String> {
    let agents = acp_agent_descriptors();
    let Some(agent) = agents.iter().find(|agent| agent.agent_id == agent_id) else {
        let known = agents
            .iter()
            .map(|agent| agent.agent_id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Unknown ACP agent `{agent_id}`. Known ACP agents: {known}"
        ));
    };
    Ok(probe_typed_agent_adapter_descriptor(cwd, agent))
}

pub(super) fn probe_typed_agent_adapter_descriptor(
    cwd: &Path,
    agent: &AgentPluginDescriptor,
) -> AgentAdapterView {
    let mut view = typed_agent_adapter_view(agent);
    match run_acp_initialize_probe_for_agent(cwd, agent) {
        Ok(evidence) => {
            view.availability = AgentAvailability::Available;
            // A completed ACP initialize exchange proves this process is
            // usable now. Advertised auth methods are future choices, not a
            // reason to keep a successfully initialized adapter ambiguous.
            view.auth_state = AgentAuthState::Ready;
            view.diagnostics = if evidence.auth_methods.is_empty() {
                Vec::new()
            } else {
                vec![format!(
                    "agent-native authentication methods advertised: {}",
                    evidence.auth_methods.join(", ")
                )]
            };
        }
        Err(error) => {
            let lower = error.to_ascii_lowercase();
            if lower.contains("not logged in")
                || lower.contains("not authenticated")
                || lower.contains("please log in")
            {
                view.availability = AgentAvailability::NeedsAuth;
                view.auth_state = AgentAuthState::LoggedOut;
            } else if !command_exists(&agent.command.command) {
                view.availability = if matches!(agent.source, AgentSource::Registry) {
                    AgentAvailability::NeedsInstall
                } else {
                    AgentAvailability::Unavailable
                };
                view.auth_state = AgentAuthState::Unknown;
            } else {
                view.availability = AgentAvailability::Unavailable;
                view.auth_state = AgentAuthState::Error;
            }
            // Agent-native stderr may contain credential material. The typed
            // frontend projection carries only a classified diagnostic.
            view.diagnostics = vec![if view.availability == AgentAvailability::NeedsAuth {
                "agent-native authentication is required".to_string()
            } else if view.availability == AgentAvailability::NeedsInstall {
                format!("agent command `{}` is not installed", agent.command.command)
            } else {
                "agent initialize probe failed; inspect local agent logs".to_string()
            }];
        }
    }
    view.startability = classify_agent_startability(view.availability, view.auth_state);
    view
}

pub(super) fn classify_agent_startability(
    availability: AgentAvailability,
    auth_state: AgentAuthState,
) -> AgentStartability {
    match (availability, auth_state) {
        (AgentAvailability::Available, AgentAuthState::Ready) => AgentStartability::Ready,
        (AgentAvailability::Available, AgentAuthState::Unknown) => AgentStartability::ProbeRequired,
        (AgentAvailability::NeedsInstall, _) => AgentStartability::InstallRequired,
        (AgentAvailability::NeedsAuth, _) | (_, AgentAuthState::LoggedOut) => {
            AgentStartability::AuthenticationRequired
        }
        _ => AgentStartability::Unavailable,
    }
}

pub(super) fn typed_agent_adapter_view(agent: &AgentPluginDescriptor) -> AgentAdapterView {
    let installed = command_exists(&agent.command.command);
    let availability = if installed {
        AgentAvailability::Available
    } else if matches!(agent.source, AgentSource::Registry) {
        AgentAvailability::NeedsInstall
    } else {
        AgentAvailability::Unavailable
    };
    let diagnostics = if installed && agent.auth_modes.contains(&AgentAuthMode::AgentNative) {
        vec![agent_auth_hint(agent).to_string()]
    } else if installed {
        Vec::new()
    } else {
        vec![format!(
            "command `{}` is not installed",
            agent.command.command
        )]
    };
    AgentAdapterView {
        agent_id: agent.agent_id.clone(),
        display_name: agent.display_name.clone(),
        route: AgentRoute::Acp,
        source: match agent.source {
            AgentSource::Registry => AgentAdapterSource::Registry,
            AgentSource::LocalCommand | AgentSource::Custom => AgentAdapterSource::LocalCommand,
        },
        availability,
        auth_state: AgentAuthState::Unknown,
        startability: classify_agent_startability(availability, AgentAuthState::Unknown),
        capabilities: agent
            .capabilities
            .iter()
            .map(|capability| CapabilityId(agent_capability_id(capability).to_string()))
            .collect(),
        models: Vec::new(),
        diagnostics,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ResidentAcpSessionKey {
    pub(super) cwd: PathBuf,
    pub(super) logical_session_id: String,
}

pub(super) struct ResidentAcpSession {
    pub(super) child: Child,
    pub(super) stdin: ChildStdin,
    pub(super) receiver: mpsc::Receiver<std::io::Result<String>>,
    pub(super) remote_session_id: String,
    pub(super) next_request_id: u64,
    pub(super) agent_command: String,
    pub(super) mode_id: Option<String>,
    pub(super) model_id: Option<String>,
    pub(super) last_used_at: Instant,
}

pub(super) static RESIDENT_ACP_SESSIONS: OnceLock<
    Mutex<BTreeMap<ResidentAcpSessionKey, ResidentAcpSession>>,
> = OnceLock::new();

pub(super) fn resident_acp_sessions()
-> &'static Mutex<BTreeMap<ResidentAcpSessionKey, ResidentAcpSession>> {
    RESIDENT_ACP_SESSIONS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(super) fn resident_acp_session_key(
    cwd: &Path,
    logical_session_id: &str,
) -> ResidentAcpSessionKey {
    ResidentAcpSessionKey {
        cwd: fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf()),
        logical_session_id: logical_session_id.to_string(),
    }
}

pub(super) fn take_resident_acp_session(
    key: &ResidentAcpSessionKey,
    agent: &AgentPluginDescriptor,
    options: &AcpSessionOptions,
) -> Option<ResidentAcpSession> {
    let mut resident = resident_acp_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(key)?;
    let compatible = resident.agent_command == agent_command_line(agent)
        && resident.mode_id == options.mode_id
        && resident.model_id == options.model_id
        && options
            .load_session_id
            .as_deref()
            .is_none_or(|session_id| session_id == resident.remote_session_id);
    let running = matches!(resident.child.try_wait(), Ok(None));
    let fresh = resident.last_used_at.elapsed() <= RESIDENT_ACP_SESSION_IDLE_TTL;
    if compatible && running && fresh {
        return Some(resident);
    }
    stop_resident_acp_session(resident);
    None
}

pub(super) fn store_resident_acp_session(
    key: ResidentAcpSessionKey,
    mut resident: ResidentAcpSession,
) {
    if !matches!(resident.child.try_wait(), Ok(None)) {
        return;
    }
    resident.last_used_at = Instant::now();
    let retired = {
        let mut sessions = resident_acp_sessions()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let stale_keys = sessions
            .iter()
            .filter(|(_, session)| session.last_used_at.elapsed() > RESIDENT_ACP_SESSION_IDLE_TTL)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        let mut retired = stale_keys
            .into_iter()
            .filter_map(|key| sessions.remove(&key))
            .collect::<Vec<_>>();
        if let Some(replaced) = sessions.insert(key, resident) {
            retired.push(replaced);
        }
        while sessions.len() > MAX_RESIDENT_ACP_SESSIONS {
            let Some(oldest_key) = sessions
                .iter()
                .min_by_key(|(_, session)| session.last_used_at)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(oldest) = sessions.remove(&oldest_key) {
                retired.push(oldest);
            }
        }
        retired
    };
    for retired in retired {
        stop_resident_acp_session(retired);
    }
}

pub(super) fn remove_resident_acp_session(cwd: &Path, logical_session_id: &str) {
    let key = resident_acp_session_key(cwd, logical_session_id);
    let resident = resident_acp_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&key);
    if let Some(resident) = resident {
        stop_resident_acp_session(resident);
    }
}

/// Stop and drop every resident ACP session cached for `cwd`.
///
/// Called when the embedding runtime tears a project down, so no agent process
/// outlives the session that started it.
pub fn shutdown_resident_acp_sessions(cwd: &Path) {
    let canonical_cwd = fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let retired = {
        let mut sessions = resident_acp_sessions()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let keys = sessions
            .keys()
            .filter(|key| key.cwd == canonical_cwd)
            .cloned()
            .collect::<Vec<_>>();
        keys.into_iter()
            .filter_map(|key| sessions.remove(&key))
            .collect::<Vec<_>>()
    };
    for retired in retired {
        stop_resident_acp_session(retired);
    }
}

pub(super) fn stop_resident_acp_session(mut resident: ResidentAcpSession) {
    let _ = resident.child.kill();
    let _ = wait_child_timeout(&mut resident.child, Duration::from_secs(1));
}

/// Recognize a legacy `/agent run acp ...` line and translate it into a typed
/// [`AgentSessionRequest`].
///
/// Returns `None` when the input is not that command at all, so a caller can
/// fall through to ordinary dispatch; `Some(Err(..))` when it is that command
/// but its arguments are invalid.
pub fn typed_agent_session_request_from_compat_input(
    input: &str,
    lane_id: Option<&str>,
) -> Option<Result<AgentSessionRequest, String>> {
    let args = input
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if args.first().map(String::as_str) != Some("/agent")
        || args.get(1).map(String::as_str) != Some("run")
        || args.get(2).map(String::as_str) != Some("acp")
    {
        return None;
    }
    let parsed = match parse_acp_run_args(&args[3..]) {
        Ok(parsed) if parsed.async_job => parsed,
        Ok(_) => return None,
        Err(error) => return Some(Err(error)),
    };
    let Some(lane_id) = lane_id else {
        return Some(Err(
            "asynchronous ACP sessions require a lane-scoped runtime owner".to_string(),
        ));
    };
    if parsed.session.mode_id.is_some() {
        return Some(Err(
            "typed asynchronous ACP sessions do not yet accept --mode; select policy through the lane contract"
                .to_string(),
        ));
    }
    Some(Ok(AgentSessionRequest {
        lane_id: lane_id.to_string(),
        agent_id: parsed.agent_id,
        model: parsed.session.model_id,
        load_session_id: parsed.session.load_session_id,
        task: parsed.task,
    }))
}

/// Project every tracked agent job under `cwd` into the shared task records
/// the runtime reduces into its snapshot.
pub fn tracked_agent_job_tasks(cwd: &Path) -> Vec<AgentTaskRecord> {
    latest_codex_jobs(cwd)
        .unwrap_or_default()
        .into_iter()
        .map(|job| agent_task_from_job_record(cwd, job))
        .collect()
}

/// Project every tracked agent job under `cwd` into its session view.
pub fn tracked_agent_job_sessions(cwd: &Path) -> Vec<AgentSessionView> {
    latest_codex_jobs(cwd)
        .unwrap_or_default()
        .into_iter()
        .filter(|job| job.kind == "acp-session")
        .filter_map(|mut job| {
            let interrupted_status = matches!(job.status.as_str(), "running" | "waiting_approval")
                .then(|| job.status.clone());
            let recovery_error = if interrupted_status.is_some() {
                // A replacement Core cannot reattach stdio or an approval
                // receiver. Stop the orphan before publishing recovery state.
                match cancel_codex_job(cwd, Some(&job.id)) {
                    Ok(_) => {
                        job.status = "failed".to_string();
                        job.updated_at = timestamp_millis();
                        let _ = append_codex_job_record(cwd, "recovered_after_restart", &job);
                        None
                    }
                    Err(error) => Some(error),
                }
            } else {
                None
            };
            let metadata = job.agent.take()?;
            let lane_id = metadata.owner.lane_id.clone()?;
            let stored_status = interrupted_status
                .clone()
                .unwrap_or_else(|| observed_codex_status(&job));
            let status = if recovery_error.is_some() {
                if stored_status == "waiting_approval" {
                    AgentSessionStatus::WaitingApproval
                } else {
                    AgentSessionStatus::Running
                }
            } else {
                match stored_status.as_str() {
                "cancelled" => AgentSessionStatus::Cancelled,
                "failed" => AgentSessionStatus::Failed,
                "finished" => AgentSessionStatus::Completed,
                // A new Core cannot resume the old process' stdio or approval
                // channel. Surface an explicit recoverable failure instead of
                // pretending the external session is still controllable.
                "waiting_approval" | "running" => AgentSessionStatus::Failed,
                _ => AgentSessionStatus::Failed,
                }
            };
            Some(AgentSessionView {
                session_id: job.id,
                lane_id,
                agent_id: metadata.agent_id,
                model: metadata.model,
                status,
                owner: metadata.owner,
                task: job.task,
                diagnostic: recovery_error.map(|error| {
                    format!("Core restart could not stop the orphaned ACP process; retry cancel: {error}")
                }).or_else(|| (status == AgentSessionStatus::Failed).then(|| {
                    if stored_status == "waiting_approval" {
                        "Core restarted while ACP approval was pending; start a new session"
                            .to_string()
                    } else if stored_status == "running" {
                        "Core restarted while the ACP session was running; start a new session"
                            .to_string()
                    } else {
                        "restored failed ACP session".to_string()
                    }
                })),
                output: (status == AgentSessionStatus::Completed)
                    .then(|| read_acp_session_output(&job.result_path))
                    .flatten(),
            })
        })
        .collect()
}

pub(super) fn agent_task_from_job_record(cwd: &Path, mut job: CodexJobRecord) -> AgentTaskRecord {
    job.status = observed_codex_status(&job);
    let evidence = codex_job_evidence(cwd, &job);
    let mut evidence_lines = Vec::new();
    if let Some(session_id) = &evidence.session_id {
        if job.kind == "acp-session" {
            evidence_lines.push(format!("session {session_id}"));
        } else {
            evidence_lines.push(format!("resume {session_id}"));
        }
    }
    evidence_lines.extend(
        evidence
            .files
            .into_iter()
            .take(8)
            .map(|file| format!("file {file}")),
    );
    AgentTaskRecord {
        id: job.id.clone(),
        parent_id: None,
        role: AgentRole::Coder,
        kind: AgentTaskKind::Job,
        route: agent_job_route(&job),
        title: job.task.clone(),
        status: agent_job_task_status(&job.status),
        activity: agent_job_activity(&job, &evidence_lines),
        summary: job.task.clone(),
        progress: agent_job_progress(&job.status),
        started_at: None,
        updated_at: Some(job.updated_at.min(u128::from(u64::MAX)) as u64),
        workspace: None,
        evidence: evidence_lines,
        permissions: agent_job_permissions(&job),
        decision: None,
        result: Some(job.result_path.display().to_string()),
        resume_handle: evidence.session_id.filter(|_| job.kind != "acp-session"),
        pid: job.pid,
        next_action: Some(AgentNextAction {
            label: "inspect agent".to_string(),
            command: Some(format!("/agent result {}", job.id)),
            reason: Some("tracked agent job state is available".to_string()),
        }),
        // The owner Core persisted with the job record when it started the
        // Agent session. A job record without agent metadata predates that
        // identity and stays unowned rather than being given one.
        owner: job.agent.as_ref().map(|agent| agent.owner.clone()),
    }
}

pub(super) fn agent_job_route(job: &CodexJobRecord) -> AgentRoute {
    if job.kind == "acp-session" {
        AgentRoute::Acp
    } else {
        AgentRoute::Terminal
    }
}

pub(super) fn agent_job_task_status(status: &str) -> AgentTaskStatus {
    match status {
        "queued" => AgentTaskStatus::Queued,
        "running" => AgentTaskStatus::Thinking,
        "finished" | "observed" => AgentTaskStatus::Done,
        "failed" => AgentTaskStatus::Failed,
        "cancelled" | "canceled" => AgentTaskStatus::Cancelled,
        _ => AgentTaskStatus::Done,
    }
}

pub(super) fn agent_job_progress(status: &str) -> u8 {
    match status {
        "queued" => 10,
        "running" => 65,
        "finished" | "observed" | "failed" | "cancelled" | "canceled" => 100,
        _ => 0,
    }
}

pub(super) fn agent_job_activity(job: &CodexJobRecord, evidence: &[String]) -> String {
    match job.status.as_str() {
        "queued" => format!("queued: {}", job.task),
        "running" => evidence
            .first()
            .cloned()
            .unwrap_or_else(|| format!("running {}", job.kind)),
        "finished" | "observed" => "result ready".to_string(),
        "failed" => evidence
            .first()
            .cloned()
            .unwrap_or_else(|| "failed; inspect result".to_string()),
        status => format!("{status}: {}", job.task),
    }
}

pub(super) fn agent_job_permissions(job: &CodexJobRecord) -> Vec<String> {
    if job.kind == "acp-session" {
        vec!["agent permission gated".to_string()]
    } else if job.kind.contains("write") || job.kind.contains("rescue") {
        vec!["workspace-write approval".to_string()]
    } else {
        vec!["read-only".to_string()]
    }
}

/// Start a typed agent session and return its initial view.
///
/// `runtime_event_sink` and `approver` are the injected runtime policy: this
/// crate decides what happened, the runtime decides where it is recorded and
/// who is asked.
pub fn start_typed_agent_session(
    cwd: &Path,
    session_id: String,
    request: AgentSessionRequest,
    owner: RuntimeOwner,
    runtime_event_sink: RuntimeEventSink,
    approver: AgentSessionApprover,
) -> Result<AgentSessionView, String> {
    start_typed_agent_session_attempt(
        cwd,
        session_id.clone(),
        session_id,
        request,
        owner,
        runtime_event_sink,
        approver,
        None,
    )
}

/// Send another prompt turn into an existing typed agent session, reusing its
/// resident connection when one is still cached.
pub fn resume_typed_agent_session(
    cwd: &Path,
    session_id: &str,
    content: String,
    owner: RuntimeOwner,
    runtime_event_sink: RuntimeEventSink,
    approver: AgentSessionApprover,
) -> Result<String, String> {
    let job = find_codex_job(cwd, session_id)?
        .ok_or_else(|| format!("Unknown agent session `{session_id}`"))?;
    let metadata = job
        .agent
        .clone()
        .ok_or_else(|| format!("Agent session `{session_id}` has no typed metadata"))?;
    if metadata.owner != owner
        || owner.session_id.as_deref() != Some(session_id)
        || metadata.owner.lane_id != owner.lane_id
    {
        return Err("agent_session_owner_mismatch".to_string());
    }
    if matches!(job.status.as_str(), "running" | "waiting_approval") {
        return Err(format!("agent session `{session_id}` is still active"));
    }
    let result = fs::read_to_string(&job.result_path)
        .map_err(|_| format!("agent session `{session_id}` has no resumable result"))?;
    let remote_session_id = extract_codex_session_id(&result)
        .ok_or_else(|| format!("agent session `{session_id}` has no ACP resume handle"))?;
    let lane_id = owner
        .lane_id
        .clone()
        .ok_or_else(|| "agent_session_owner_mismatch".to_string())?;
    let input_id = fresh_id("agent-input");
    let request = AgentSessionRequest {
        lane_id,
        agent_id: metadata.agent_id,
        model: metadata.model,
        load_session_id: Some(remote_session_id),
        task: content,
    };
    start_typed_agent_session_attempt(
        cwd,
        session_id.to_string(),
        input_id.clone(),
        request,
        owner,
        runtime_event_sink,
        approver,
        Some(input_id.clone()),
    )?;
    Ok(input_id)
}

/// Re-run a typed agent session's last prompt turn after a failure, reusing
/// the recorded request.
pub fn retry_typed_agent_session(
    cwd: &Path,
    session_id: &str,
    owner: RuntimeOwner,
    runtime_event_sink: RuntimeEventSink,
    approver: AgentSessionApprover,
) -> Result<String, String> {
    let job = find_codex_job(cwd, session_id)?
        .ok_or_else(|| format!("Unknown agent session `{session_id}`"))?;
    resume_typed_agent_session(
        cwd,
        session_id,
        job.task,
        owner,
        runtime_event_sink,
        approver,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn start_typed_agent_session_attempt(
    cwd: &Path,
    session_id: String,
    artifact_id: String,
    request: AgentSessionRequest,
    owner: RuntimeOwner,
    runtime_event_sink: RuntimeEventSink,
    mut approver: AgentSessionApprover,
    accepted_input_id: Option<String>,
) -> Result<AgentSessionView, String> {
    validate_typed_agent_session_request(&request)?;
    let agents = acp_agent_descriptors();
    let agent = agents
        .iter()
        .find(|agent| agent.agent_id == request.agent_id)
        .ok_or_else(|| {
            let known = agents
                .iter()
                .map(|agent| agent.agent_id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Unknown ACP agent `{}`. Known ACP agents: {known}",
                request.agent_id
            )
        })?;
    if owner.lane_id.as_deref() != Some(request.lane_id.as_str())
        || owner.session_id.as_deref() != Some(session_id.as_str())
    {
        return Err("agent session owner must match the requested lane and session".to_string());
    }

    let log_path = codex_job_artifact_path(cwd, &artifact_id, "jsonl");
    let result_path = codex_job_artifact_path(cwd, &artifact_id, "result.md");
    let runtime_event_path = acp_job_runtime_events_path(cwd, &artifact_id);
    let baseline_path = codex_job_artifact_path(cwd, &artifact_id, "baseline.status");
    let cancel_path = acp_job_cancel_path(cwd, &session_id);
    if cancel_path.exists() {
        fs::remove_file(&cancel_path)
            .map_err(|error| format!("failed to reset ACP cancellation marker: {error}"))?;
    }
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    write_codex_status_baseline(cwd, &baseline_path)?;
    let record = CodexJobRecord {
        id: session_id.clone(),
        kind: "acp-session".to_string(),
        status: "running".to_string(),
        pid: None,
        command: agent_command_line(agent),
        task: request.task.clone(),
        log_path: log_path.clone(),
        result_path: result_path.clone(),
        baseline_path,
        updated_at: timestamp_millis(),
        agent: Some(AgentJobMetadata {
            agent_id: request.agent_id.clone(),
            model: request.model.clone(),
            owner: owner.clone(),
        }),
    };
    append_codex_job_record(cwd, "started", &record)?;

    let session = AgentSessionView {
        session_id: session_id.clone(),
        lane_id: request.lane_id,
        agent_id: request.agent_id,
        model: request.model.clone(),
        status: AgentSessionStatus::Starting,
        owner,
        task: request.task,
        diagnostic: None,
        output: None,
    };
    let mut start_events = Vec::new();
    if let Some(input_id) = accepted_input_id {
        start_events.push(RuntimeEvent::new(
            0,
            RuntimeEventKind::AgentSessionInputAccepted {
                session_id: session_id.clone(),
                input_id,
            },
        ));
    }
    start_events.push(RuntimeEvent::new(
        0,
        RuntimeEventKind::AgentSessionStarted {
            session: session.clone(),
        },
    ));
    // Persist the accepted input and its exact task before process spawn so
    // snapshot reconstruction cannot collapse a multi-turn dialogue.
    append_acp_runtime_events(&runtime_event_path, &start_events)?;
    runtime_event_sink(start_events);

    let monitor_cwd = cwd.to_path_buf();
    let monitor_cancel_path = cancel_path.clone();
    let monitor_agent = agent.clone();
    let monitor_session = AcpSessionOptions {
        load_session_id: request.load_session_id,
        mode_id: None,
        model_id: request.model,
    };
    let monitor_runtime_event_path = runtime_event_path;
    let mut monitor_record = record;
    let monitor_view = session.clone();
    let terminal_sink = Arc::clone(&runtime_event_sink);
    let protocol_sink = Arc::clone(&runtime_event_sink);
    let pid_slot = Arc::new(Mutex::new(None::<u32>));
    let pid_slot_for_thread = Arc::clone(&pid_slot);
    // Both ids are captured before the runner starts: the session id Core
    // published this Agent session under, and the artifact id of this turn.
    let monitor_owner_session_id = session_id.clone();
    let monitor_turn_id = artifact_id.clone();
    // The exact owner Core published this Agent session under.
    let monitor_owner = session.owner.clone();
    std::thread::spawn(move || {
        let resident_session_id = monitor_record.id.clone();
        let result = run_acp_session_prompt_for_agent_with_log(
            &monitor_cwd,
            &monitor_agent,
            &monitor_view.task,
            monitor_session,
            AcpSessionPromptRunContext {
                approver: &mut approver,
                log_path: log_path.clone(),
                cancel_path: Some(cancel_path),
                runtime_event_log_path: Some(monitor_runtime_event_path.clone()),
                permission_context: PermissionContext::default(),
                runtime_event_sink: Some(protocol_sink),
                resident_session_id: Some(resident_session_id),
                owner_session_id: Some(monitor_owner_session_id),
                turn_id: Some(monitor_turn_id),
                owner: Some(monitor_owner),
                on_pid: |pid| {
                    if let Ok(mut slot) = pid_slot_for_thread.lock() {
                        *slot = Some(pid);
                    }
                    let mut pid_record = monitor_record.clone();
                    pid_record.pid = Some(pid);
                    pid_record.updated_at = timestamp_millis();
                    let _ = append_codex_job_record(&monitor_cwd, "pid", &pid_record);
                },
            },
        );
        // The marker expresses owner intent, but cancellation becomes terminal
        // only here, after the ACP runner and its child process have stopped.
        let was_cancelled = monitor_cancel_path.exists()
            || find_codex_job(&monitor_cwd, &monitor_record.id)
                .ok()
                .flatten()
                .is_some_and(|job| job.status == "cancelled");
        monitor_record.pid = pid_slot.lock().ok().and_then(|slot| *slot);
        let mut terminal = monitor_view;
        let kind = match result {
            Ok(evidence) => {
                // Exactly one writer per runtime event. The runner was handed
                // `runtime_event_log_path` above, so it already appended every
                // event in `evidence.runtime_events` as it streamed them.
                // Re-appending the accumulated vec here would double the whole
                // turn on disk — invisible live, because completion sinks only
                // the terminal event, but doubled for snapshot assembly and
                // replay, which rebuild from the file.
                let _ = write_acp_session_result(&result_path, &evidence);
                monitor_record.status = acp_session_job_status(&evidence);
                if was_cancelled {
                    monitor_record.status = "cancelled".to_string();
                    terminal.status = AgentSessionStatus::Cancelled;
                    RuntimeEventKind::AgentSessionUpdated { session: terminal }
                } else if monitor_record.status == "failed" {
                    terminal.status = AgentSessionStatus::Failed;
                    terminal.diagnostic = Some("ACP session reported a failed status".to_string());
                    RuntimeEventKind::AgentSessionFailed { session: terminal }
                } else {
                    terminal.status = AgentSessionStatus::Completed;
                    terminal.output = nonempty_acp_output(&evidence.message);
                    RuntimeEventKind::AgentSessionCompleted { session: terminal }
                }
            }
            Err(error) => {
                if was_cancelled {
                    monitor_record.status = "cancelled".to_string();
                    terminal.status = AgentSessionStatus::Cancelled;
                    terminal.diagnostic = Some("cancelled by owner".to_string());
                    RuntimeEventKind::AgentSessionUpdated { session: terminal }
                } else {
                    // Host bookkeeping (job result scratch file), not a
                    // model-driven effect: stays outside the capability seam.
                    let _ = fs::write(&result_path, format!("# ACP session failed\n\n{error}\n"));
                    monitor_record.status = "failed".to_string();
                    terminal.status = AgentSessionStatus::Failed;
                    terminal.diagnostic = Some(truncate_for_preview(&error, 320));
                    RuntimeEventKind::AgentSessionFailed { session: terminal }
                }
            }
        };
        monitor_record.updated_at = timestamp_millis();
        let _ = append_codex_job_record(&monitor_cwd, "completed", &monitor_record);
        let terminal_event = RuntimeEvent::new(0, kind);
        // The monitor's half of the single-writer rule: only the terminal
        // session fact, which the runner cannot know because it is decided
        // here from the run result and the cancellation marker.
        let _ = append_acp_runtime_events(
            &monitor_runtime_event_path,
            std::slice::from_ref(&terminal_event),
        );
        terminal_sink(vec![terminal_event]);
    });

    Ok(session)
}

pub(super) fn nonempty_acp_output(message: &str) -> Option<String> {
    let output = message.trim();
    (!output.is_empty()).then(|| output.to_string())
}

pub(super) fn read_acp_session_output(path: &Path) -> Option<String> {
    let result = fs::read_to_string(path).ok()?;
    let (_, output) = result.split_once("\n\n")?;
    let (_, output) = output.rsplit_once("\n\n")?;
    nonempty_acp_output(output)
}

/// Validate a typed agent session request before anything is spawned.
///
/// Rejecting here keeps a malformed request from reaching a process, so the
/// runtime can surface the error without a partially started session.
pub fn validate_typed_agent_session_request(request: &AgentSessionRequest) -> Result<(), String> {
    let agents = acp_agent_descriptors();
    let agent = agents
        .iter()
        .find(|agent| agent.agent_id == request.agent_id)
        .ok_or_else(|| {
            let known = agents
                .iter()
                .map(|agent| agent.agent_id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Unknown ACP agent `{}`. Known ACP agents: {known}",
                request.agent_id
            )
        })?;
    if !matches!(agent.transport, AgentTransport::Acp) {
        return Err(format!(
            "Agent `{}` does not use ACP transport.",
            request.agent_id
        ));
    }
    if !command_exists(&agent.command.command) {
        return Err(format!(
            "Agent `{}` is unavailable because command `{}` is not installed",
            request.agent_id, agent.command.command
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn start_acp_session_job(
    cwd: &Path,
    agent: &AgentPluginDescriptor,
    task: String,
    session: AcpSessionOptions,
    runtime_event_sink: Option<RuntimeEventSink>,
) -> Result<String, String> {
    let id = format!("acp-{}", timestamp_millis());
    let log_path = codex_job_artifact_path(cwd, &id, "jsonl");
    let result_path = codex_job_artifact_path(cwd, &id, "result.md");
    let runtime_event_path = acp_job_runtime_events_path(cwd, &id);
    let baseline_path = codex_job_artifact_path(cwd, &id, "baseline.status");
    let cancel_path = acp_job_cancel_path(cwd, &id);
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    write_codex_status_baseline(cwd, &baseline_path)?;
    let record = CodexJobRecord {
        id: id.clone(),
        kind: "acp-session".to_string(),
        status: "running".to_string(),
        pid: None,
        command: agent_command_line(agent),
        task: task.clone(),
        log_path: log_path.clone(),
        result_path: result_path.clone(),
        baseline_path: baseline_path.clone(),
        updated_at: timestamp_millis(),
        agent: None,
    };
    append_codex_job_record(cwd, "started", &record)?;

    let monitor_cwd = cwd.to_path_buf();
    let monitor_cancel_path = cancel_path.clone();
    let monitor_agent = agent.clone();
    let monitor_task = task.clone();
    let monitor_session = session.clone();
    let monitor_runtime_event_path = runtime_event_path.clone();
    let mut monitor_record = record.clone();
    let pid_slot = Arc::new(Mutex::new(None::<u32>));
    let pid_slot_for_thread = Arc::clone(&pid_slot);
    std::thread::spawn(move || {
        // Legacy job mechanics are exercised only in unit tests; production
        // async ACP work is routed through the supervisor-owned typed session.
        let mut background_approver = |_prompt: viden_types::PermissionPrompt| {
            ApprovalResponse::allow_once(Some("test-only async job approval".to_string()))
        };
        let result = run_acp_session_prompt_for_agent_with_log(
            &monitor_cwd,
            &monitor_agent,
            &monitor_task,
            monitor_session.clone(),
            AcpSessionPromptRunContext {
                approver: &mut background_approver,
                log_path: log_path.clone(),
                cancel_path: Some(cancel_path.clone()),
                runtime_event_log_path: Some(monitor_runtime_event_path.clone()),
                permission_context: PermissionContext::default(),
                runtime_event_sink,
                resident_session_id: None,
                owner_session_id: None,
                turn_id: None,
                // Core published no session for this legacy job path, so it
                // knows no owner to attach.
                owner: None,
                on_pid: |pid| {
                    if let Ok(mut slot) = pid_slot_for_thread.lock() {
                        *slot = Some(pid);
                    }
                    let mut pid_record = monitor_record.clone();
                    pid_record.pid = Some(pid);
                    pid_record.updated_at = timestamp_millis();
                    let _ = append_codex_job_record(&monitor_cwd, "pid", &pid_record);
                },
            },
        );
        let was_cancelled = monitor_cancel_path.exists()
            || find_codex_job(&monitor_cwd, &monitor_record.id)
                .ok()
                .flatten()
                .is_some_and(|job| job.status == "cancelled");
        if was_cancelled && result.is_err() {
            let _ = append_agent_job_log_event(
                &log_path,
                "system",
                &format!(
                    "observed cancellation for ACP session job `{}` after agent process stopped",
                    monitor_record.id
                ),
            );
            return;
        }
        monitor_record.pid = pid_slot.lock().ok().and_then(|slot| *slot);
        match result {
            Ok(evidence) => {
                // Overwrite rather than append, so this test-only path stays
                // single-writer despite the runner having already persisted
                // the same events incrementally: the runner's accumulated vec
                // IS the file's content, and nothing is appended before the
                // spawn here, so no earlier fact is dropped. The production
                // typed-session path instead lets the runner own the file
                // outright, because it does append start events first.
                let _ =
                    write_acp_runtime_events(&monitor_runtime_event_path, &evidence.runtime_events);
                let _ = write_acp_session_result(&result_path, &evidence);
                monitor_record.status = acp_session_job_status(&evidence);
                if was_cancelled && monitor_record.status != "cancelled" {
                    monitor_record.status = "cancelled".to_string();
                }
            }
            Err(error) => {
                // Host bookkeeping (job result scratch file), not a
                // model-driven effect: stays outside the capability seam.
                let _ = fs::write(&result_path, format!("# ACP session failed\n\n{error}\n"));
                monitor_record.status = "failed".to_string();
            }
        }
        monitor_record.updated_at = timestamp_millis();
        let _ = append_codex_job_record(&monitor_cwd, "completed", &monitor_record);
    });

    Ok(format!(
        "Started ACP session job `{}`.\n  agent: {} ({})\n  log: {}\n  result: {}\n\nUse `/agent status` to watch it, `/agent result {}` to read output, and `/agent cancel {}` to stop it.",
        id,
        agent.agent_id,
        agent.display_name,
        record.log_path.display(),
        record.result_path.display(),
        id,
        id
    ))
}

/// Cancel a typed agent session by id, stopping its process and recording the
/// terminal status.
pub fn cancel_typed_agent_session(cwd: &Path, session_id: &str) -> Result<(), String> {
    let result = cancel_codex_job(cwd, Some(session_id)).map(|_| ());
    // A completed turn may leave its ACP process idle for a fast follow-up.
    // Explicit session cancellation must also retire that resident connection.
    remove_resident_acp_session(cwd, session_id);
    result
}

/// Record a terminal or transitional status against a tracked agent session.
pub fn mark_typed_agent_session_status(
    cwd: &Path,
    session_id: &str,
    status: &str,
) -> Result<(), String> {
    let mut job = find_codex_job(cwd, session_id)?
        .ok_or_else(|| format!("Unknown agent session `{session_id}`"))?;
    if job.kind != "acp-session" {
        return Err(format!("Job `{session_id}` is not an ACP session"));
    }
    job.status = status.to_string();
    job.updated_at = timestamp_millis();
    append_codex_job_record(cwd, "status", &job)
}

/// Replay the runtime events persisted for every tracked agent job under
/// `cwd`, so a reconnecting frontend can rebuild agent state from facts.
pub fn tracked_agent_job_runtime_events(cwd: &Path) -> Vec<RuntimeEvent> {
    let agents = cwd.join(".viden").join("agents");
    let Ok(entries) = fs::read_dir(agents) else {
        return Vec::new();
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".runtime-events.jsonl"))
        })
        .collect::<Vec<_>>();
    paths.sort_by(|left, right| {
        acp_artifact_ordinal(left)
            .cmp(&acp_artifact_ordinal(right))
            .then_with(|| left.cmp(right))
    });
    // Gates written before they carried their owner name only the ACP protocol
    // handle their id is keyed on, so they join to nothing in the rebuilt view.
    // The artifact that produced them does name the session: a first turn is
    // logged under the Agent session id itself, while a follow-up turn is
    // logged under a fresh input id and re-emits the same gate. Binding a gate
    // once from a session-named artifact and carrying that binding to the same
    // gate id in later turns restores provenance the file layout already
    // records, without rewriting a single persisted byte. A gate that arrives
    // with an owner is authoritative and is never overwritten.
    let mut gate_owner_sessions: BTreeMap<String, String> = BTreeMap::new();
    let mut events = Vec::new();
    for path in paths {
        let artifact_session = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".runtime-events.jsonl"))
            .filter(|artifact| artifact.starts_with("agent-session_"))
            .map(str::to_string);
        for mut event in read_acp_runtime_events(&path) {
            if let RuntimeEventKind::MergeGateUpdated { gate } = &mut event.kind {
                match gate.owner.session_id.as_deref() {
                    Some(session_id) => {
                        gate_owner_sessions.insert(gate.gate_id.clone(), session_id.to_string());
                    }
                    None => {
                        if let Some(session_id) = gate_owner_sessions
                            .get(&gate.gate_id)
                            .cloned()
                            .or_else(|| artifact_session.clone())
                        {
                            gate_owner_sessions.insert(gate.gate_id.clone(), session_id.clone());
                            gate.owner.session_id = Some(session_id);
                        }
                    }
                }
            }
            events.push(event);
        }
    }
    events
}

pub(super) fn acp_artifact_ordinal(path: &Path) -> Option<u128> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.split_once('_'))
        .and_then(|(_, suffix)| suffix.split('.').next())
        .and_then(|ordinal| ordinal.parse().ok())
}

/// Read one turn's persisted runtime events, healing files written before the
/// single-writer rule was enforced.
///
/// Files produced by older builds hold each turn's streamed block twice: the
/// runner appended it live, then session completion re-appended the same
/// accumulated vec. A repeat is identified by the line being byte-identical to
/// one already read from this same file, which is sound because the runner's
/// `runtime_sequence` increments per event within a turn: two legitimate
/// events can never serialize identically, since `sequence` is part of the
/// encoding. A byte-identical line is therefore always the removed re-append,
/// never real streamed output — a chunk repeating the same text arrives at a
/// later sequence and is kept.
///
/// Healing on read keeps the artifacts append-only and untouched on disk while
/// fixing every consumer at once: snapshot assembly, replay, and raw reads.
pub(super) fn read_acp_runtime_events(path: &Path) -> Vec<RuntimeEvent> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    content
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .filter(|line| seen.insert(line.to_string()))
        .filter_map(|line| serde_json::from_str::<RuntimeEvent>(line).ok())
        .collect()
}

#[cfg(test)]
pub(super) fn write_acp_runtime_events(path: &Path, events: &[RuntimeEvent]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let mut content = String::new();
    for event in events {
        let line = serde_json::to_string(event)
            .map_err(|err| format!("failed to encode ACP runtime event: {err}"))?;
        content.push_str(&line);
        content.push('\n');
    }
    fs::write(path, content).map_err(|err| format!("failed to write {}: {err}", path.display()))
}

pub(super) fn append_acp_runtime_events(
    path: &Path,
    events: &[RuntimeEvent],
) -> Result<(), String> {
    if events.is_empty() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| format!("failed to open {}: {err}", path.display()))?;
    for event in events {
        let line = serde_json::to_string(event)
            .map_err(|err| format!("failed to encode ACP runtime event: {err}"))?;
        writeln!(file, "{line}")
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn append_acp_update_runtime_events(
    events: &mut Vec<RuntimeEvent>,
    sequence: &mut u64,
    session_id: &str,
    // Assistant message id for the whole turn. Chunks of one reply must share
    // it so the reducer grows one message instead of one per chunk.
    turn_message_id: &str,
    gate_evidence_ids: &mut Vec<String>,
    // Workspace the session runs in, when one is known. Inline Agent bytes are
    // persisted under it; without it they stay unresolved rather than being
    // written outside the workspace.
    cwd: Option<&Path>,
    // Owner Core published this Agent session under, when it published one.
    // Every fact below carries it verbatim; `None` stays `None` rather than
    // becoming an owner derived from the session id alone (GUI-CORE-010).
    owner: Option<&RuntimeOwner>,
    update: &Value,
) {
    match acp_update_kind(update).as_deref() {
        Some("AgentMessageChunk") | Some("agent_message_chunk") => {
            if let Some(text) = acp_message_chunk_text(update) {
                push_acp_runtime_event(
                    events,
                    sequence,
                    RuntimeEventKind::AssistantDelta {
                        message_id: turn_message_id.to_string(),
                        task_id: Some(format!("acp-session-{session_id}")),
                        session_id: Some(session_id.to_string()),
                        content: text,
                    },
                );
            }
            // Content the text extractor cannot represent still belongs to the
            // reply. Publishing it as a typed part keeps an image or a file the
            // Agent returned from disappearing into prose about it.
            if let Some((part, evidence)) = acp_message_chunk_part(update, cwd, owner) {
                // Persisted bytes are published as evidence before the part
                // that references them, so a client never sees a reference
                // whose fact has not been recorded yet.
                if let Some(evidence) = evidence {
                    push_unique_evidence_id(gate_evidence_ids, &evidence.id);
                    push_acp_runtime_event(
                        events,
                        sequence,
                        RuntimeEventKind::EvidenceRecorded { evidence },
                    );
                }
                push_acp_runtime_event(
                    events,
                    sequence,
                    RuntimeEventKind::AgentMessagePart {
                        session_id: session_id.to_string(),
                        message_id: turn_message_id.to_string(),
                        part,
                    },
                );
            }
        }
        Some("ToolCall") | Some("tool_call") => {
            let tool_call_id = acp_tool_call_id(update);
            let title = acp_tool_call_title(update);
            push_acp_runtime_event(
                events,
                sequence,
                RuntimeEventKind::ToolCallStarted {
                    tool_call_id,
                    name: title,
                    input_preview: truncate_for_preview(&update.to_string(), 500),
                    owner: owner.cloned(),
                },
            );
        }
        Some("ToolCallUpdate") | Some("tool_call_update") => {
            let tool_call_id = acp_tool_call_id(update);
            let title = acp_tool_call_title(update);
            let status = update
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("completed");
            let content = update
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or(status);
            if let Some(patch) = acp_patch_text(update) {
                let path = acp_patch_path(update);
                let patch_evidence = EvidenceView {
                    id: format!("acp-patch-{tool_call_id}-{sequence}"),
                    kind: "patch".to_string(),
                    summary: acp_patch_summary(&patch, path.as_deref()),
                    path: path.clone(),
                    source: Some("acp:patch.v1".to_string()),
                    canonical: None,
                    metadata: Some(acp_patch_metadata(&patch, path.as_deref(), update)),
                    timestamp: None,
                    owner: owner.cloned(),
                };
                push_unique_evidence_id(gate_evidence_ids, &patch_evidence.id);
                push_acp_runtime_event(
                    events,
                    sequence,
                    RuntimeEventKind::EvidenceRecorded {
                        evidence: patch_evidence,
                    },
                );
            }
            let evidence = EvidenceView {
                id: format!("acp-tool-{tool_call_id}-{sequence}"),
                kind: "tool_log".to_string(),
                summary: truncate_for_preview(content, 500),
                path: None,
                source: Some("acp".to_string()),
                canonical: None,
                metadata: None,
                timestamp: None,
                owner: owner.cloned(),
            };
            push_acp_runtime_event(
                events,
                sequence,
                RuntimeEventKind::ToolCallFinished {
                    tool_call_id: tool_call_id.clone(),
                    name: title,
                    success: !matches!(status, "failed" | "error" | "cancelled" | "canceled"),
                    exit_code: None,
                    evidence: Some(evidence.clone()),
                },
            );
            push_unique_evidence_id(gate_evidence_ids, &evidence.id);
            push_acp_runtime_event(
                events,
                sequence,
                RuntimeEventKind::EvidenceRecorded { evidence },
            );
            push_acp_runtime_event(
                events,
                sequence,
                RuntimeEventKind::MergeGateUpdated {
                    gate: acp_session_merge_gate(
                        session_id,
                        owner,
                        MergeGateStatus::CollectingEvidence,
                        gate_evidence_ids,
                    ),
                },
            );
        }
        Some("Diff")
        | Some("diff")
        | Some("Patch")
        | Some("patch")
        | Some("FileChange")
        | Some("file_change")
        | Some("fileChange")
        | Some("file_change_patch")
        | Some("file-patch")
        | Some("diff-updated") => {
            if let Some(patch) = acp_patch_text(update) {
                let path = acp_patch_path(update);
                let evidence = EvidenceView {
                    id: format!("acp-patch-{session_id}-{sequence}"),
                    kind: "patch".to_string(),
                    summary: acp_patch_summary(&patch, path.as_deref()),
                    path: path.clone(),
                    source: Some("acp:patch.v1".to_string()),
                    canonical: None,
                    metadata: Some(acp_patch_metadata(&patch, path.as_deref(), update)),
                    timestamp: None,
                    owner: owner.cloned(),
                };
                push_unique_evidence_id(gate_evidence_ids, &evidence.id);
                push_acp_runtime_event(
                    events,
                    sequence,
                    RuntimeEventKind::EvidenceRecorded { evidence },
                );
                push_acp_runtime_event(
                    events,
                    sequence,
                    RuntimeEventKind::MergeGateUpdated {
                        gate: acp_session_merge_gate(
                            session_id,
                            owner,
                            MergeGateStatus::CollectingEvidence,
                            gate_evidence_ids,
                        ),
                    },
                );
            }
        }
        Some("TurnEnd") | Some("turn_end") => {
            let status = update
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("completed");
            let evidence = EvidenceView {
                id: format!("acp-turn-end-{session_id}-{sequence}"),
                kind: "acp_turn_end".to_string(),
                summary: format!("ACP session {session_id} ended with status {status}"),
                path: None,
                source: Some("acp".to_string()),
                canonical: None,
                metadata: None,
                timestamp: None,
                owner: owner.cloned(),
            };
            push_unique_evidence_id(gate_evidence_ids, &evidence.id);
            push_acp_runtime_event(
                events,
                sequence,
                RuntimeEventKind::EvidenceRecorded { evidence },
            );
            push_acp_runtime_event(
                events,
                sequence,
                RuntimeEventKind::MergeGateUpdated {
                    gate: acp_session_merge_gate(
                        session_id,
                        owner,
                        MergeGateStatus::CollectingEvidence,
                        gate_evidence_ids,
                    ),
                },
            );
        }
        _ => {}
    }
}

/// Builds the merge gate for one ACP session turn.
///
/// `session_id` is the scoped session id — the Agent session Core published
/// when there is one, and only otherwise the agent's own protocol handle. Every
/// fact of one turn's gate must be built from the same value: a gate whose
/// opening `Proposed` fact carried the protocol handle and whose updates
/// carried the published session split into two records under two ids.
/// `owner` carries the published session as well, which is what lets a frontend
/// tell a gate awaiting a live session from one left behind by a session that
/// already finished. `None` stays `None` rather than becoming an owner derived
/// from the session id alone (GUI-CORE-010).
///
/// The id is opaque to clients: nothing parses it, and gates already persisted
/// under the protocol handle keep that id on replay and are bound to their
/// session by the owner backfill in `tracked_agent_job_runtime_events`.
pub(super) fn acp_session_merge_gate(
    session_id: &str,
    owner: Option<&RuntimeOwner>,
    status: MergeGateStatus,
    evidence_ids: &[String],
) -> MergeGateRecord {
    let now = now_timestamp();
    let task_id = format!("acp-session-{session_id}");
    let required_evidence = acp_session_required_evidence(evidence_ids);
    MergeGateRecord {
        gate_id: format!("gate-acp-session-{session_id}"),
        task_id: task_id.clone(),
        status,
        required_evidence: required_evidence.clone(),
        evidence_ids: evidence_ids.to_vec(),
        gate_type: MergeGateType::Artifact,
        owner: RuntimeOwner {
            task_id: Some(task_id),
            ..owner.cloned().unwrap_or_default()
        },
        validator: None,
        policy_snapshot: MergeGatePolicySnapshot {
            required_evidence,
            permission_snapshot_id: None,
            requires_independent_validator: false,
            captured_at: Some(now),
        },
        decision: if status == MergeGateStatus::CollectingEvidence && !evidence_ids.is_empty() {
            Some(MergeGateDecision::decided_now(
                MergeGateDecisionOutcome::AwaitingEvidence,
                "missing_canonical".to_string(),
                RuntimeOwner::default(),
                evidence_ids.to_vec(),
                fresh_id("audit"),
            ))
        } else {
            None
        },
        conflict: None,
        applied_change_id: None,
        recovery_snapshot: None,
        audit_ids: Vec::new(),
        updated_at: Some(now),
    }
}

pub(super) fn acp_session_required_evidence(evidence_ids: &[String]) -> Vec<String> {
    let mut required = Vec::new();
    if evidence_ids.iter().any(|id| id.starts_with("acp-patch-")) {
        required.push("patch".to_string());
    }
    required.push("acp_turn_end".to_string());
    required
}

pub(super) fn push_unique_evidence_id(evidence_ids: &mut Vec<String>, evidence_id: &str) {
    if !evidence_ids.iter().any(|id| id == evidence_id) {
        evidence_ids.push(evidence_id.to_string());
    }
}

pub(super) fn push_acp_runtime_event(
    events: &mut Vec<RuntimeEvent>,
    sequence: &mut u64,
    kind: RuntimeEventKind,
) {
    events.push(RuntimeEvent::new(*sequence, kind));
    *sequence += 1;
}
