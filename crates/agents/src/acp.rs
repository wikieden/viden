use std::{
    collections::BTreeMap,
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

use super::codex::*;
use super::glue::*;
use super::infra::*;
use super::render::*;
use crate::RuntimeEventSink;
use serde_json::{Value, json};
use viden_permissions::{PermissionContext, PermissionEngine};
use viden_plugin_api::{
    AgentAuthMode, AgentCommandSpec, AgentPermissionProfile, AgentPluginCapability,
    AgentPluginDescriptor, AgentProtocolVersion, AgentSource, AgentTransport,
};
use viden_plugin_host::builtin_agent_descriptors;
use viden_tools::{
    FilesystemCapability, InteractiveInvocation, InteractiveProcessControl, LocalFilesystem,
    LocalProcess, ProcessCapability,
};
use viden_types::{
    AgentContentPart, ApprovalResponse, EvidenceView, MergeGateStatus, PermissionDecision,
    RuntimeEvent, RuntimeEventKind, RuntimeOwner, ToolInput, ToolSpec,
};

pub(super) const DEFAULT_LOCAL_ACP_HANDSHAKE_TIMEOUT_SECS: u64 = 30;

pub(super) const DEFAULT_REGISTRY_ACP_HANDSHAKE_TIMEOUT_SECS: u64 = 90;

pub(super) const DEFAULT_LOCAL_ACP_SESSION_TIMEOUT_SECS: u64 = 60;

pub(super) const DEFAULT_KIRO_ACP_SESSION_TIMEOUT_SECS: u64 = 120;

pub(super) const ACP_SESSION_CANCEL_REQUEST_ID: u64 = 90;

/// `/agent probe`: route a probe to the Codex band or to an ACP agent by id.
///
/// A probe only establishes reachability and handshake health; it starts no
/// prompt turn and causes no workspace mutation.
pub fn handle_agent_probe_command(cwd: &Path, args: &[String]) -> Result<String, String> {
    match args.first().map(String::as_str) {
        Some("codex") => handle_codex_probe_command(cwd, args),
        Some("acp") => handle_acp_agent_probe_command(cwd, args.get(1).map(String::as_str)),
            Some(id) if acp_agent_descriptors().iter().any(|agent| agent.agent_id == id) => {
                handle_acp_agent_probe_command(cwd, Some(id))
            }
        Some(other) => Err(format!(
            "Unsupported probe target `{other}`. Use `codex` or `acp <agent-id>`."
        )),
        None => Err(
            "Usage: /agent probe codex [--thread|--turn <task>|--turn-write <task>] | /agent probe acp <agent-id>"
                .to_string(),
        ),
    }
}

/// `/agent auth acp <agent-id> [method-id]`: run an ACP agent's authentication
/// method, or report the methods it advertises when none is named.
pub fn handle_agent_auth_command(cwd: &Path, args: &[String]) -> Result<String, String> {
    match args.first().map(String::as_str) {
        Some("acp") => {
            let agent_id = args.get(1).map(String::as_str);
            let method_id = args.get(2).map(String::as_str);
            handle_acp_agent_auth_command(cwd, agent_id, method_id)
        }
        _ => Err("Usage: /agent auth acp <agent-id> [method-id]".to_string()),
    }
}

pub(super) fn handle_acp_agent_probe_command(
    cwd: &Path,
    target: Option<&str>,
) -> Result<String, String> {
    let id = target.ok_or_else(|| "Usage: /agent probe acp <agent-id>".to_string())?;
    let agents = acp_agent_descriptors();
    let Some(agent) = agents.iter().find(|agent| agent.agent_id == id) else {
        let known = agents
            .iter()
            .map(|agent| agent.agent_id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Unknown ACP agent `{id}`. Known ACP agents: {known}"
        ));
    };
    if !matches!(agent.transport, AgentTransport::Acp) {
        return Err(format!("Agent `{id}` does not use ACP transport."));
    }
    let evidence = run_acp_initialize_probe_for_agent(cwd, agent)?;
    let auth = if evidence.auth_methods.is_empty() {
        "none advertised".to_string()
    } else {
        evidence.auth_methods.join(", ")
    };
    let capabilities = if evidence.capabilities.is_empty() {
        "none advertised".to_string()
    } else {
        evidence.capabilities.join(", ")
    };
    Ok(format!(
        "ACP initialize probe ok.\n  agent: {} ({})\n  command: {}\n  protocol: {}\n  remote: {}\n  auth: {}\n  capabilities: {}\n  log: {}",
        agent.agent_id,
        agent.display_name,
        agent_command_line(agent),
        evidence.protocol_version,
        evidence.agent_label,
        auth,
        capabilities,
        evidence.log_path.display()
    ))
}

pub(super) fn handle_acp_agent_auth_command(
    cwd: &Path,
    target: Option<&str>,
    method_id: Option<&str>,
) -> Result<String, String> {
    let id = target.ok_or_else(|| "Usage: /agent auth acp <agent-id> [method-id]".to_string())?;
    let agents = acp_agent_descriptors();
    let Some(agent) = agents.iter().find(|agent| agent.agent_id == id) else {
        let known = agents
            .iter()
            .map(|agent| agent.agent_id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Unknown ACP agent `{id}`. Known ACP agents: {known}"
        ));
    };
    if agent.agent_id == "kiro-cli" {
        return Ok(render_kiro_native_auth_instructions(agent));
    }
    let evidence = run_acp_authenticate_for_agent(cwd, agent, method_id)?;
    Ok(format!(
        "ACP authenticate completed.\n  agent: {} ({})\n  method: {}\n  status: {}\n  log: {}",
        agent.agent_id,
        agent.display_name,
        evidence.method_id,
        evidence.status,
        evidence.log_path.display()
    ))
}

pub(super) fn render_kiro_native_auth_instructions(agent: &AgentPluginDescriptor) -> String {
    format!(
        "Kiro CLI uses native authentication.\n  agent: {} ({})\n  command: {}\n  login: kiro-cli login --use-device-flow\n  verify: kiro-cli doctor\n  gate: /agent smoke acp --live\n  note: Viden does not store Kiro credentials; Kiro owns auth, billing, and agent configuration.",
        agent.agent_id,
        agent.display_name,
        agent_command_line(agent),
    )
}

pub(super) fn acp_agent_descriptors() -> Vec<AgentPluginDescriptor> {
    let mut agents = builtin_agent_descriptors();
    if let Some(custom) = custom_acp_agent_descriptor_from_env() {
        agents.push(custom);
    }
    agents
}

pub(super) fn custom_acp_agent_descriptor_from_env() -> Option<AgentPluginDescriptor> {
    let command = env::var("VIDEN_AGENT_ACP_COMMAND")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    Some(custom_acp_agent_descriptor(&command))
}

pub(super) fn custom_acp_agent_descriptor(command: &str) -> AgentPluginDescriptor {
    let (program, args) = shell_descriptor_command(command);
    AgentPluginDescriptor {
        agent_id: "custom-acp".to_string(),
        display_name: "Custom ACP agent".to_string(),
        version: "custom".to_string(),
        transport: AgentTransport::Acp,
        source: AgentSource::LocalCommand,
        command: AgentCommandSpec {
            command: program,
            args,
            env: Vec::new(),
        },
        registry_package: None,
        protocol_versions: vec![AgentProtocolVersion::AcpV1],
        auth_modes: vec![AgentAuthMode::AgentNative],
        capabilities: vec![
            AgentPluginCapability::SessionPrompt,
            AgentPluginCapability::StreamingUpdates,
            AgentPluginCapability::ToolCalls,
            AgentPluginCapability::SessionCancel,
        ],
        permission_profile: AgentPermissionProfile::RuntimeGated,
        experimental_methods: Vec::new(),
        config_schema_version: 1,
    }
}

pub(super) fn shell_descriptor_command(command: &str) -> (String, Vec<String>) {
    if cfg!(windows) {
        (
            "cmd".to_string(),
            vec!["/C".to_string(), command.to_string()],
        )
    } else {
        (
            "sh".to_string(),
            vec!["-lc".to_string(), command.to_string()],
        )
    }
}

/// Run the ACP smoke gate across the registered agents.
///
/// Without `live` the gate exercises only offline framing and descriptor
/// checks. With `live` it starts each agent, so `approver` carries any `Ask`
/// out to the embedding runtime's operator surface.
pub fn run_acp_smoke_gate(
    cwd: &Path,
    live: bool,
    approver: &mut impl FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
) -> Result<String, String> {
    let agents = acp_agent_descriptors();
    run_acp_smoke_gate_for_agents(cwd, &agents, live, approver)
}

pub(super) fn run_acp_smoke_gate_for_agents(
    cwd: &Path,
    agents: &[AgentPluginDescriptor],
    live: bool,
    approver: &mut impl FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
) -> Result<String, String> {
    let mut lines = vec![format!(
        "ACP smoke gate ({})",
        if live { "live session" } else { "initialize" }
    )];
    let mut failed = 0usize;
    let mut blocked = 0usize;
    for agent in agents {
        if !matches!(agent.transport, AgentTransport::Acp) {
            continue;
        }
        let result = if live {
            run_acp_session_prompt_for_agent(
                cwd,
                agent,
                "Reply with OK only.",
                AcpSessionOptions::default(),
                approver,
            )
            .map(|evidence| {
                format!(
                    "ok session={} status={} usage={}",
                    evidence.session_id,
                    evidence.final_status,
                    evidence
                        .usage_summary
                        .unwrap_or_else(|| "unavailable".to_string())
                )
            })
        } else {
            run_acp_initialize_probe_for_agent(cwd, agent).map(|evidence| {
                let auth = if evidence.auth_methods.is_empty() {
                    "none advertised".to_string()
                } else {
                    evidence.auth_methods.join(", ")
                };
                format!(
                    "ok protocol={} remote={} auth={}",
                    evidence.protocol_version, evidence.agent_label, auth
                )
            })
        };
        match result {
            Ok(summary) => lines.push(format!("  PASS {}: {}", agent.agent_id, summary)),
            Err(error) => {
                let classification = classify_acp_smoke_error(agent, &error);
                if classification == "blocked-auth" {
                    blocked += 1;
                } else {
                    failed += 1;
                }
                lines.push(format!(
                    "  {} {}: {}",
                    if classification == "blocked-auth" {
                        "BLOCKED"
                    } else {
                        "FAIL"
                    },
                    agent.agent_id,
                    smoke_error_summary(&error)
                ));
            }
        }
    }
    lines.push(format!(
        "summary: {} failed, {} blocked-auth",
        failed, blocked
    ));
    if failed > 0 || blocked > 0 {
        return Err(lines.join("\n"));
    }
    Ok(lines.join("\n"))
}

pub(super) fn classify_acp_smoke_error(
    _agent: &AgentPluginDescriptor,
    error: &str,
) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("not logged in")
        || lower.contains("not authenticated")
        || lower.contains("please log in")
    {
        "blocked-auth"
    } else if lower.contains("timed out") {
        "timeout"
    } else {
        "failed"
    }
}

pub(super) fn smoke_error_summary(error: &str) -> String {
    let mut summary = truncate_for_line(error.lines().next().unwrap_or(error), 220);
    if summary.contains("timed out") {
        summary
            .push_str(" (increase VIDEN_ACP_SESSION_TIMEOUT_SECS or run provider-native doctor)");
    }
    summary
}

/// `/agent run acp`: run one delegated task against an ACP agent.
///
/// Every runtime concern arrives as a parameter — `permission_context` bounds
/// the agent's reverse-RPC filesystem and terminal requests, `approver`
/// resolves an `Ask`, and `runtime_event_sink` receives the turn's events —
/// so this crate never reaches for runtime-owned state.
pub fn handle_acp_agent_run_command(
    cwd: &Path,
    run: AcpRunArgs,
    approver: &mut impl FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    permission_context: PermissionContext,
    runtime_event_sink: Option<RuntimeEventSink>,
) -> Result<String, String> {
    let agents = acp_agent_descriptors();
    handle_acp_agent_run_command_with_agents(
        cwd,
        &agents,
        run,
        approver,
        permission_context,
        runtime_event_sink,
    )
}

pub(super) fn handle_acp_agent_run_command_with_agents(
    cwd: &Path,
    agents: &[AgentPluginDescriptor],
    run: AcpRunArgs,
    approver: &mut impl FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    permission_context: PermissionContext,
    runtime_event_sink: Option<RuntimeEventSink>,
) -> Result<String, String> {
    let Some(agent) = agents.iter().find(|agent| agent.agent_id == run.agent_id) else {
        let known = agents
            .iter()
            .map(|agent| agent.agent_id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Unknown ACP agent `{}`. Known ACP agents: {known}",
            run.agent_id
        ));
    };
    if !matches!(agent.transport, AgentTransport::Acp) {
        return Err(format!(
            "Agent `{}` does not use ACP transport.",
            run.agent_id
        ));
    }
    if run.async_job {
        return Err(
            "asynchronous ACP sessions must be submitted through RuntimeSupervisor so approvals, cancellation, and replay remain owner-scoped"
                .to_string(),
        );
    }
    let evidence = run_acp_session_prompt_for_agent_with_permissions(
        cwd,
        agent,
        &run.task,
        run.session,
        approver,
        permission_context,
        runtime_event_sink,
    )?;
    let tool_calls = if evidence.tool_calls.is_empty() {
        "none".to_string()
    } else {
        evidence.tool_calls.join(", ")
    };
    let message = if evidence.message.trim().is_empty() {
        "<empty>".to_string()
    } else {
        evidence.message
    };
    let usage = evidence
        .usage_summary
        .unwrap_or_else(|| "unavailable".to_string());
    Ok(format!(
        "ACP session completed.\n  agent: {} ({})\n  session: {}\n  status: {}\n  message: {}\n  tool_calls: {}\n  usage: {}\n  log: {}",
        agent.agent_id,
        agent.display_name,
        evidence.session_id,
        evidence.final_status,
        message,
        tool_calls,
        usage,
        evidence.log_path.display()
    ))
}

/// A parsed `/agent run acp` invocation.
///
/// Fields stay crate-private: callers build this with [`parse_acp_run_args`]
/// and hand it straight back to [`handle_acp_agent_run_command`], so the
/// argument grammar can change without moving the runtime's dispatch seam.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AcpRunArgs {
    pub(super) async_job: bool,
    pub(super) agent_id: String,
    pub(super) task: String,
    pub(super) session: AcpSessionOptions,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct AcpSessionOptions {
    pub(super) load_session_id: Option<String>,
    pub(super) mode_id: Option<String>,
    pub(super) model_id: Option<String>,
}

pub(super) struct AcpSessionPromptRunContext<'a, A, P>
where
    A: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    P: FnMut(u32),
{
    pub(super) approver: &'a mut A,
    pub(super) log_path: PathBuf,
    pub(super) cancel_path: Option<PathBuf>,
    /// Durable runtime-event log this turn appends to, when the caller wants
    /// one.
    ///
    /// Setting it transfers ownership of stream persistence to the runner:
    /// every event the runner produces is appended here as it is produced, so
    /// a caller must not re-persist `AcpSessionPromptEvidence::runtime_events`
    /// afterwards. Exactly one writer per event — the runner during the
    /// stream, the caller only for terminal facts the runner cannot know.
    pub(super) runtime_event_log_path: Option<PathBuf>,
    pub(super) permission_context: PermissionContext,
    pub(super) runtime_event_sink: Option<RuntimeEventSink>,
    pub(super) resident_session_id: Option<String>,
    /// Session id Core publishes this Agent session under.
    ///
    /// The ACP session id is the agent's own protocol handle. Every other
    /// Core fact — the session view, its accepted inputs, its conversation —
    /// is keyed by the Viden session id, so a streamed reply scoped by the
    /// protocol handle can never join the conversation a client renders.
    pub(super) owner_session_id: Option<String>,
    /// Artifact id of this prompt turn, unique across the session's turns.
    ///
    /// A resumed turn reuses the remote session and restarts ACP request ids,
    /// so the request id alone cannot keep two turns from sharing a message.
    pub(super) turn_id: Option<String>,
    /// Full runtime owner Core published this Agent session under.
    ///
    /// Every live-work fact this turn emits carries it verbatim, so a client
    /// can scope tool calls and evidence to the exact Lane Core bound
    /// (GUI-CORE-010). `None` on the ad-hoc probe path, where Core published
    /// no session and therefore knows no owner.
    pub(super) owner: Option<RuntimeOwner>,
    pub(super) on_pid: P,
}

/// Picks the session id runtime facts are scoped by.
///
/// Falls back to the ACP protocol handle only where Core has not published a
/// session of its own, which is the ad-hoc probe path rather than a supervised
/// Agent session.
pub(super) fn acp_scoped_session_id<'a>(
    owner_session_id: Option<&'a str>,
    acp_session_id: &'a str,
) -> &'a str {
    owner_session_id.unwrap_or(acp_session_id)
}

/// Builds the assistant message id every chunk of one prompt turn shares.
pub(super) fn acp_turn_message_id(
    scoped_session_id: &str,
    turn_id: Option<&str>,
    prompt_request_id: u64,
) -> String {
    match turn_id {
        Some(turn_id) => format!("acp-message-{scoped_session_id}-turn-{turn_id}"),
        None => format!("acp-message-{scoped_session_id}-turn-{prompt_request_id}"),
    }
}

/// Parse the argument tail of `/agent run acp` into an [`AcpRunArgs`].
///
/// Parsing is separated from execution so a malformed invocation is rejected
/// before any process is spawned.
pub fn parse_acp_run_args(args: &[String]) -> Result<AcpRunArgs, String> {
    let usage = "Usage: /agent run acp [--async] [--load-session <id>] [--mode <mode-id>] [--model <model-id>] <agent-id> <task>";
    let mut parsed = AcpRunArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--async" => {
                parsed.async_job = true;
                i += 1;
            }
            "--load-session" => {
                parsed.session.load_session_id = Some(
                    args.get(i + 1)
                        .filter(|value| !value.trim().is_empty())
                        .ok_or_else(|| usage.to_string())?
                        .clone(),
                );
                i += 2;
            }
            "--mode" => {
                parsed.session.mode_id = Some(
                    args.get(i + 1)
                        .filter(|value| !value.trim().is_empty())
                        .ok_or_else(|| usage.to_string())?
                        .clone(),
                );
                i += 2;
            }
            "--model" => {
                parsed.session.model_id = Some(
                    args.get(i + 1)
                        .filter(|value| !value.trim().is_empty())
                        .ok_or_else(|| usage.to_string())?
                        .clone(),
                );
                i += 2;
            }
            value if value.starts_with("--") => {
                return Err(format!("Unknown ACP run option `{value}`.\n{usage}"));
            }
            _ => break,
        }
    }
    parsed.agent_id = args
        .get(i)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| usage.to_string())?
        .clone();
    parsed.task = args.get(i + 1..).unwrap_or_default().join(" ");
    if parsed.task.trim().is_empty() {
        return Err(usage.to_string());
    }
    Ok(parsed)
}

pub(super) fn acp_agent_command_args(agent: &AgentPluginDescriptor) -> Vec<String> {
    let mut args = agent.command.args.clone();
    if is_kiro_agent(agent) {
        push_env_arg(&mut args, "--agent", "VIDEN_KIRO_AGENT");
        push_env_arg(&mut args, "--model", "VIDEN_KIRO_MODEL");
        push_env_arg(&mut args, "--effort", "VIDEN_KIRO_EFFORT");
        if env_flag_enabled("VIDEN_KIRO_TRUST_ALL_TOOLS")
            && !args.iter().any(|arg| arg == "--trust-all-tools")
        {
            args.push("--trust-all-tools".to_string());
        } else {
            push_env_arg(&mut args, "--trust-tools", "VIDEN_KIRO_TRUST_TOOLS");
        }
        push_env_arg(&mut args, "--agent-engine", "VIDEN_KIRO_AGENT_ENGINE");
    }
    args
}

pub(super) fn is_kiro_agent(agent: &AgentPluginDescriptor) -> bool {
    agent.agent_id == "kiro-cli" || agent.command.command == "kiro-cli"
}

pub(super) fn push_env_arg(args: &mut Vec<String>, flag: &str, env_name: &str) {
    if args.iter().any(|arg| arg == flag) {
        return;
    }
    let Ok(value) = env::var(env_name) else {
        return;
    };
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    args.push(flag.to_string());
    args.push(value.to_string());
}

pub(super) fn acp_session_job_status(evidence: &AcpSessionPromptEvidence) -> String {
    match evidence.final_status.as_str() {
        "completed" | "end_turn" => "finished".to_string(),
        "cancelled" => "cancelled".to_string(),
        "failed" | "interrupted" => "failed".to_string(),
        _ => "observed".to_string(),
    }
}

pub(super) fn write_acp_session_result(
    path: &Path,
    evidence: &AcpSessionPromptEvidence,
) -> Result<(), String> {
    let tool_calls = if evidence.tool_calls.is_empty() {
        "none".to_string()
    } else {
        evidence.tool_calls.join(", ")
    };
    fs::write(
        path,
        format!(
            "# ACP session result\n\nsession: {}\nstatus: {}\ntool_calls: {}\nusage: {}\nlog: {}\n\n{}",
            evidence.session_id,
            evidence.final_status,
            tool_calls,
            evidence.usage_summary.as_deref().unwrap_or("unavailable"),
            evidence.log_path.display(),
            evidence.message.trim()
        ),
    )
    .map_err(|err| format!("failed to write ACP session result: {err}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AcpProbeEvidence {
    pub(super) protocol_version: String,
    pub(super) agent_label: String,
    pub(super) auth_methods: Vec<String>,
    pub(super) auth_method_ids: Vec<String>,
    pub(super) capabilities: Vec<String>,
    pub(super) log_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AcpAuthEvidence {
    pub(super) method_id: String,
    pub(super) status: String,
    pub(super) log_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AcpSessionPromptEvidence {
    pub(super) session_id: String,
    pub(super) final_status: String,
    pub(super) message: String,
    pub(super) tool_calls: Vec<String>,
    pub(super) usage_summary: Option<String>,
    pub(super) runtime_events: Vec<RuntimeEvent>,
    pub(super) log_path: PathBuf,
}

pub(super) fn run_acp_initialize_probe(
    cwd: &Path,
    command: &str,
) -> Result<AcpProbeEvidence, String> {
    let log_path = acp_probe_log_path(cwd);
    let mut log_entries = Vec::new();
    let request = acp_initialize_request();
    log_entries.push(jsonl_event("client", &request));

    let mut child = spawn_acp_process(cwd, command)?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "failed to open ACP stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to open ACP stdout".to_string())?;
    stdin
        .write_all(request.as_bytes())
        .and_then(|_| stdin.write_all(b"\n"))
        .and_then(|_| stdin.flush())
        .map_err(|err| format!("failed to write initialize: {err}"))?;

    let response = match read_line_with_timeout(stdout, Duration::from_secs(8)) {
        Ok(response) => response,
        Err(error) => {
            return Err(finish_failed_probe(
                child,
                log_path.clone(),
                log_entries,
                error,
            ));
        }
    };
    log_entries.push(jsonl_event("agent", &response));
    let _ = child.kill();
    let _ = child.wait();
    write_probe_log(&log_path, &log_entries)?;

    if !response.contains("\"jsonrpc\"") || !response.contains("\"result\"") {
        return Err(format!(
            "unexpected initialize response; log {}",
            log_path.display()
        ));
    }
    Ok(acp_probe_evidence_from_response(&response, log_path))
}

pub(super) fn run_acp_initialize_probe_for_agent(
    cwd: &Path,
    agent: &AgentPluginDescriptor,
) -> Result<AcpProbeEvidence, String> {
    let log_path = acp_probe_log_path(cwd);
    let mut log_entries = Vec::new();
    let request = acp_initialize_request();
    log_entries.push(jsonl_event("client", &request));

    let mut child = spawn_acp_agent_process(cwd, agent)?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "failed to open ACP stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to open ACP stdout".to_string())?;
    stdin
        .write_all(request.as_bytes())
        .and_then(|_| stdin.write_all(b"\n"))
        .and_then(|_| stdin.flush())
        .map_err(|err| format!("failed to write initialize: {err}"))?;

    let response = match read_line_with_timeout(stdout, acp_agent_handshake_timeout(agent)) {
        Ok(response) => response,
        Err(error) => {
            return Err(finish_failed_probe(
                child,
                log_path.clone(),
                log_entries,
                error,
            ));
        }
    };
    log_entries.push(jsonl_event("agent", &response));
    let _ = child.kill();
    let _ = child.wait();
    write_probe_log(&log_path, &log_entries)?;

    if !response.contains("\"jsonrpc\"") || !response.contains("\"result\"") {
        return Err(format!(
            "unexpected initialize response; log {}",
            log_path.display()
        ));
    }
    Ok(acp_probe_evidence_from_response(&response, log_path))
}

pub(super) fn run_acp_authenticate_for_agent(
    cwd: &Path,
    agent: &AgentPluginDescriptor,
    method_id: Option<&str>,
) -> Result<AcpAuthEvidence, String> {
    let log_path = acp_probe_log_path(cwd);
    let mut log_entries = Vec::new();
    let mut child = spawn_acp_agent_process(cwd, agent)?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "failed to open ACP stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to open ACP stdout".to_string())?;
    let receiver = read_lines_async(stdout);

    let initialize = acp_initialize_request();
    if let Err(error) = write_acp_request(&mut stdin, &initialize, &mut log_entries) {
        return Err(finish_failed_probe(child, log_path, log_entries, error));
    }
    let initialize_response = match read_acp_response_line(
        &receiver,
        0,
        &mut log_entries,
        acp_agent_handshake_timeout(agent),
    ) {
        Ok(response) => response,
        Err(error) => return Err(finish_failed_probe(child, log_path, log_entries, error)),
    };
    let probe = acp_probe_evidence_from_response(&initialize_response, log_path.clone());
    let method = match method_id {
        Some(method) => method.to_string(),
        None if probe.auth_method_ids.len() == 1 => probe.auth_method_ids[0].clone(),
        None if probe.auth_method_ids.is_empty() => {
            return Err(finish_expected_acp_stop(
                child,
                log_path,
                log_entries,
                "ACP agent did not advertise authentication methods".to_string(),
            ));
        }
        None => {
            let methods = probe.auth_methods.join(", ");
            return Err(finish_expected_acp_stop(
                child,
                log_path,
                log_entries,
                format!("choose an auth method: {methods}"),
            ));
        }
    };
    if !probe.auth_method_ids.is_empty() && !probe.auth_method_ids.iter().any(|id| id == &method) {
        let methods = probe.auth_methods.join(", ");
        return Err(finish_expected_acp_stop(
            child,
            log_path,
            log_entries,
            format!("unknown ACP auth method `{method}`. Available methods: {methods}"),
        ));
    }

    let authenticate = acp_authenticate_request(&method);
    if let Err(error) = write_acp_request(&mut stdin, &authenticate, &mut log_entries) {
        return Err(finish_failed_probe(child, log_path, log_entries, error));
    }
    let response = match read_acp_response_line(
        &receiver,
        1,
        &mut log_entries,
        acp_agent_handshake_timeout(agent),
    ) {
        Ok(response) => response,
        Err(error) => return Err(finish_failed_probe(child, log_path, log_entries, error)),
    };
    let _ = child.kill();
    let _ = child.wait();
    write_probe_log(&log_path, &log_entries)?;
    if response.contains(r#""error""#) {
        return Err(format!(
            "ACP authenticate failed for method `{method}`; log {}",
            log_path.display()
        ));
    }
    Ok(AcpAuthEvidence {
        method_id: method,
        status: acp_auth_status_from_response(&response),
        log_path,
    })
}

pub(super) fn run_acp_session_prompt_for_agent(
    cwd: &Path,
    agent: &AgentPluginDescriptor,
    prompt: &str,
    session: AcpSessionOptions,
    approver: &mut impl FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
) -> Result<AcpSessionPromptEvidence, String> {
    run_acp_session_prompt_for_agent_with_permissions(
        cwd,
        agent,
        prompt,
        session,
        approver,
        PermissionContext::default(),
        None,
    )
}

pub(super) fn run_acp_session_prompt_for_agent_with_permissions(
    cwd: &Path,
    agent: &AgentPluginDescriptor,
    prompt: &str,
    session: AcpSessionOptions,
    approver: &mut impl FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    permission_context: PermissionContext,
    runtime_event_sink: Option<RuntimeEventSink>,
) -> Result<AcpSessionPromptEvidence, String> {
    let log_path = acp_session_log_path(cwd);
    run_acp_session_prompt_for_agent_with_log(
        cwd,
        agent,
        prompt,
        session,
        AcpSessionPromptRunContext {
            approver,
            log_path,
            cancel_path: None,
            runtime_event_log_path: None,
            permission_context,
            runtime_event_sink,
            resident_session_id: None,
            owner_session_id: None,
            turn_id: None,
            // An ad-hoc probe runs outside any published Agent session.
            owner: None,
            on_pid: |_| {},
        },
    )
}

pub(super) fn run_acp_session_prompt_for_agent_with_log<A, P>(
    cwd: &Path,
    agent: &AgentPluginDescriptor,
    prompt: &str,
    session: AcpSessionOptions,
    context: AcpSessionPromptRunContext<'_, A, P>,
) -> Result<AcpSessionPromptEvidence, String>
where
    A: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    P: FnMut(u32),
{
    let AcpSessionPromptRunContext {
        approver,
        log_path,
        cancel_path,
        runtime_event_log_path,
        permission_context,
        runtime_event_sink,
        resident_session_id,
        owner_session_id,
        turn_id,
        owner,
        mut on_pid,
    } = context;
    let mut log_entries = Vec::new();
    let mut permission_engine = PermissionEngine::new(cwd);
    permission_engine.restore_context(permission_context);
    // ACP client-requested effects (fs/*, terminal/*) run through the shared
    // OS capability seam; the local providers preserve direct std behavior.
    let acp_fs_capability: Arc<dyn FilesystemCapability> = Arc::new(LocalFilesystem);
    let acp_process_capability: Arc<dyn ProcessCapability> = Arc::new(LocalProcess);
    let resident_key = resident_session_id
        .as_deref()
        .map(|session_id| resident_acp_session_key(cwd, session_id));
    let resident = resident_key
        .as_ref()
        .and_then(|key| take_resident_acp_session(key, agent, &session));
    let (mut child, mut stdin, receiver, session_id, next_request_id) = if let Some(resident) =
        resident
    {
        on_pid(resident.child.id());
        log_entries.push(jsonl_event("system", "reused live ACP process and session"));
        (
            resident.child,
            resident.stdin,
            resident.receiver,
            resident.remote_session_id,
            resident.next_request_id,
        )
    } else {
        let mut child = spawn_acp_agent_process(cwd, agent)?;
        on_pid(child.id());
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "failed to open ACP stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "failed to open ACP stdout".to_string())?;
        let receiver = read_lines_async(stdout);

        let initialize = acp_initialize_request();
        if let Err(error) = write_acp_request(&mut stdin, &initialize, &mut log_entries) {
            return Err(finish_failed_probe(child, log_path, log_entries, error));
        }
        if let Err(error) = read_acp_response_line(
            &receiver,
            0,
            &mut log_entries,
            acp_agent_handshake_timeout(agent),
        ) {
            return Err(finish_failed_probe(child, log_path, log_entries, error));
        }

        let session_request = if let Some(session_id) = session.load_session_id.as_deref() {
            acp_session_load_request(cwd, session_id)
        } else {
            acp_session_new_request(cwd)
        };
        if let Err(error) = write_acp_request(&mut stdin, &session_request, &mut log_entries) {
            return Err(finish_failed_probe(child, log_path, log_entries, error));
        }
        let session_response = match read_acp_response_line(
            &receiver,
            1,
            &mut log_entries,
            acp_agent_handshake_timeout(agent),
        ) {
            Ok(response) => response,
            Err(error) => {
                return Err(finish_failed_probe(child, log_path, log_entries, error));
            }
        };
        let Some(session_id) = acp_session_id_from_response(&session_response)
            .or_else(|| session.load_session_id.clone())
        else {
            return Err(finish_failed_probe(
                child,
                log_path.clone(),
                log_entries.clone(),
                "ACP session creation did not return a session id".to_string(),
            ));
        };

        let mut next_request_id = 2;
        if let Some(mode_id) = session.mode_id.as_deref() {
            let set_mode = acp_session_set_mode_request(&session_id, mode_id, next_request_id);
            if let Err(error) = write_acp_request(&mut stdin, &set_mode, &mut log_entries) {
                return Err(finish_failed_probe(child, log_path, log_entries, error));
            }
            let response = match read_acp_response_line(
                &receiver,
                next_request_id,
                &mut log_entries,
                acp_agent_handshake_timeout(agent),
            ) {
                Ok(response) => response,
                Err(error) => {
                    return Err(finish_failed_probe(child, log_path, log_entries, error));
                }
            };
            if acp_response_has_error(&response) {
                return Err(finish_failed_probe(
                    child,
                    log_path,
                    log_entries,
                    acp_response_error_message_for(
                        "ACP session/set_mode failed",
                        &serde_json::from_str(&response).unwrap_or(Value::Null),
                    ),
                ));
            }
            next_request_id += 1;
        }
        if let Some(model_id) = session.model_id.as_deref() {
            let set_model = acp_session_set_model_request(&session_id, model_id, next_request_id);
            if let Err(error) = write_acp_request(&mut stdin, &set_model, &mut log_entries) {
                return Err(finish_failed_probe(child, log_path, log_entries, error));
            }
            let response = match read_acp_response_line(
                &receiver,
                next_request_id,
                &mut log_entries,
                acp_agent_handshake_timeout(agent),
            ) {
                Ok(response) => response,
                Err(error) => {
                    return Err(finish_failed_probe(child, log_path, log_entries, error));
                }
            };
            next_request_id += 1;
            if acp_response_is_method_not_found(&response) {
                let legacy_set_model =
                    acp_legacy_session_set_model_request(&session_id, model_id, next_request_id);
                if let Err(error) =
                    write_acp_request(&mut stdin, &legacy_set_model, &mut log_entries)
                {
                    return Err(finish_failed_probe(child, log_path, log_entries, error));
                }
                let legacy_response = match read_acp_response_line(
                    &receiver,
                    next_request_id,
                    &mut log_entries,
                    acp_agent_handshake_timeout(agent),
                ) {
                    Ok(response) => response,
                    Err(error) => {
                        return Err(finish_failed_probe(child, log_path, log_entries, error));
                    }
                };
                if acp_response_has_error(&legacy_response) {
                    return Err(finish_failed_probe(
                        child,
                        log_path,
                        log_entries,
                        acp_response_error_message_for(
                            "ACP session/set_model failed",
                            &serde_json::from_str(&legacy_response).unwrap_or(Value::Null),
                        ),
                    ));
                }
                next_request_id += 1;
            } else if acp_response_has_error(&response) {
                return Err(finish_failed_probe(
                    child,
                    log_path,
                    log_entries,
                    acp_response_error_message_for(
                        "ACP session/set_config_option failed",
                        &serde_json::from_str(&response).unwrap_or(Value::Null),
                    ),
                ));
            }
        }
        (child, stdin, receiver, session_id, next_request_id)
    };

    let prompt_request_id = next_request_id;
    let prompt_request = acp_session_prompt_request(agent, &session_id, prompt, prompt_request_id);
    if let Err(error) = write_acp_request(&mut stdin, &prompt_request, &mut log_entries) {
        return Err(finish_failed_probe(child, log_path, log_entries, error));
    }
    // One assistant message per prompt turn, scoped to the session Core
    // publishes rather than the agent's own protocol handle.
    let scoped_session_id = acp_scoped_session_id(owner_session_id.as_deref(), &session_id);
    let turn_message_id =
        acp_turn_message_id(scoped_session_id, turn_id.as_deref(), prompt_request_id);
    let mut message = String::new();
    let mut tool_calls = Vec::new();
    let mut runtime_events = Vec::new();
    let mut runtime_sequence = 1;
    let mut acp_gate_evidence_ids = Vec::new();
    push_acp_runtime_event(
        &mut runtime_events,
        &mut runtime_sequence,
        RuntimeEventKind::MergeGateUpdated {
            gate: acp_session_merge_gate(
                &session_id,
                owner.as_ref(),
                MergeGateStatus::Proposed,
                &acp_gate_evidence_ids,
            ),
        },
    );
    if let Some(path) = runtime_event_log_path.as_deref()
        && let Err(error) = append_acp_runtime_events(path, &runtime_events)
    {
        return Err(finish_failed_probe(child, log_path, log_entries, error));
    }
    if let Some(sink) = runtime_event_sink.as_ref() {
        sink(runtime_events.clone());
    }
    let mut terminals = AcpTerminalStore::default();
    let mut final_status = None;
    let mut usage_summary = None;
    let mut cancel_sent = false;
    let mut cancel_deadline = None;
    let session_timeout = acp_session_prompt_timeout(agent);
    let deadline = Instant::now() + session_timeout;
    while Instant::now() < deadline {
        if !cancel_sent && cancel_path.as_ref().is_some_and(|path| path.exists()) {
            let cancel_request = acp_session_cancel_request(&session_id);
            if let Err(error) = write_acp_request(&mut stdin, &cancel_request, &mut log_entries) {
                return Err(finish_failed_probe(child, log_path, log_entries, error));
            }
            cancel_sent = true;
            cancel_deadline = Some(Instant::now() + Duration::from_secs(2));
            continue;
        }
        if cancel_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            final_status = Some("cancelled".to_string());
            break;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(remaining.min(Duration::from_secs(1))) {
            Ok(Ok(line)) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                log_entries.push(jsonl_event("agent", line));
                let Ok(value) = serde_json::from_str::<Value>(line) else {
                    continue;
                };
                if value.get("id").and_then(Value::as_u64) == Some(prompt_request_id) {
                    if value.get("error").is_some() {
                        return Err(finish_failed_probe(
                            child,
                            log_path,
                            log_entries,
                            acp_response_error_message(&value),
                        ));
                    }
                    final_status = Some(acp_prompt_response_status(&value));
                    usage_summary = acp_usage_summary(&value);
                    break;
                }
                if value.get("id").and_then(Value::as_u64) == Some(ACP_SESSION_CANCEL_REQUEST_ID) {
                    final_status = Some(
                        value
                            .pointer("/result/status")
                            .and_then(Value::as_str)
                            .unwrap_or("cancelled")
                            .to_string(),
                    );
                    break;
                }
                if value.get("method").and_then(Value::as_str) == Some("session/request_permission")
                {
                    let prompt = acp_permission_prompt(&value);
                    let approval = approver(prompt);
                    let response = acp_permission_response(&value, approval.is_allowed());
                    if let Err(error) = write_acp_request(&mut stdin, &response, &mut log_entries) {
                        return Err(finish_failed_probe(child, log_path, log_entries, error));
                    }
                    continue;
                }
                if let Some(response) = acp_filesystem_client_request_response(
                    cwd,
                    &mut permission_engine,
                    approver,
                    &acp_fs_capability,
                    &value,
                ) {
                    if let Err(error) = write_acp_request(&mut stdin, &response, &mut log_entries) {
                        return Err(finish_failed_probe(child, log_path, log_entries, error));
                    }
                    continue;
                }
                if let Some(response) = acp_terminal_client_request_response(
                    cwd,
                    &mut permission_engine,
                    approver,
                    &acp_process_capability,
                    &mut terminals,
                    &value,
                ) {
                    if let Err(error) = write_acp_request(&mut stdin, &response, &mut log_entries) {
                        return Err(finish_failed_probe(child, log_path, log_entries, error));
                    }
                    continue;
                }
                if let Some(response) = acp_unsupported_client_request_response(&value) {
                    if let Err(error) = write_acp_request(&mut stdin, &response, &mut log_entries) {
                        return Err(finish_failed_probe(child, log_path, log_entries, error));
                    }
                    continue;
                }
                let method = value.get("method").and_then(Value::as_str);
                if method != Some("session/update") && method != Some("session/notification") {
                    continue;
                }
                let Some(update) = value.pointer("/params/update") else {
                    continue;
                };
                let before_event_count = runtime_events.len();
                append_acp_update_runtime_events(
                    &mut runtime_events,
                    &mut runtime_sequence,
                    scoped_session_id,
                    &turn_message_id,
                    &mut acp_gate_evidence_ids,
                    Some(cwd),
                    owner.as_ref(),
                    update,
                );
                if let Some(path) = runtime_event_log_path.as_deref()
                    && before_event_count < runtime_events.len()
                    && let Err(error) =
                        append_acp_runtime_events(path, &runtime_events[before_event_count..])
                {
                    return Err(finish_failed_probe(child, log_path, log_entries, error));
                }
                if before_event_count < runtime_events.len()
                    && let Some(sink) = runtime_event_sink.as_ref()
                {
                    sink(runtime_events[before_event_count..].to_vec());
                }
                match acp_update_kind(update).as_deref() {
                    Some("AgentMessageChunk") | Some("agent_message_chunk") => {
                        if let Some(text) = acp_message_chunk_text(update) {
                            message.push_str(&text);
                        }
                    }
                    Some("ToolCall")
                    | Some("tool_call")
                    | Some("ToolCallUpdate")
                    | Some("tool_call_update") => {
                        tool_calls.push(acp_tool_call_summary(update));
                    }
                    Some("TurnEnd") | Some("turn_end") => {
                        final_status = update
                            .get("status")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .or_else(|| Some("completed".to_string()));
                        break;
                    }
                    _ => {}
                }
            }
            Ok(Err(error)) => {
                return Err(finish_failed_probe(
                    child,
                    log_path,
                    log_entries,
                    format!("failed to read ACP session update: {error}"),
                ));
            }
            Err(_) => {}
        }
    }
    let Some(final_status) = final_status else {
        return Err(finish_failed_probe(
            child,
            log_path,
            log_entries,
            format!(
                "ACP session/prompt timed out before TurnEnd or final response after {}s",
                session_timeout.as_secs()
            ),
        ));
    };
    write_probe_log(&log_path, &log_entries)?;
    if let Some(resident_key) = resident_key
        && !cancel_sent
        && final_status != "cancelled"
    {
        store_resident_acp_session(
            resident_key,
            ResidentAcpSession {
                child,
                stdin,
                receiver,
                remote_session_id: session_id.clone(),
                next_request_id: prompt_request_id + 1,
                agent_command: agent_command_line(agent),
                mode_id: session.mode_id.clone(),
                model_id: session.model_id.clone(),
                last_used_at: Instant::now(),
            },
        );
    } else {
        let _ = child.kill();
        let _ = child.wait();
    }

    Ok(AcpSessionPromptEvidence {
        session_id,
        final_status,
        message,
        tool_calls,
        usage_summary,
        runtime_events,
        log_path,
    })
}

pub(super) fn write_acp_request(
    stdin: &mut impl Write,
    request: &str,
    log_entries: &mut Vec<String>,
) -> Result<(), String> {
    log_entries.push(jsonl_event("client", request));
    stdin
        .write_all(request.as_bytes())
        .and_then(|_| stdin.write_all(b"\n"))
        .and_then(|_| stdin.flush())
        .map_err(|err| format!("failed to write ACP request: {err}"))
}

pub(super) fn read_acp_response_line(
    receiver: &mpsc::Receiver<std::io::Result<String>>,
    id: u64,
    log_entries: &mut Vec<String>,
    timeout: Duration,
) -> Result<String, String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(remaining.min(Duration::from_secs(1))) {
            Ok(Ok(line)) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                log_entries.push(jsonl_event("agent", line));
                let Ok(value) = serde_json::from_str::<Value>(line) else {
                    continue;
                };
                if value.get("id").and_then(Value::as_u64) == Some(id) {
                    return Ok(line.to_string());
                }
            }
            Ok(Err(error)) => return Err(format!("failed to read ACP response: {error}")),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(format!("ACP command closed stdout before response id {id}"));
            }
        }
    }
    Err(format!("ACP response id {id} timed out"))
}

pub(super) fn acp_agent_handshake_timeout(agent: &AgentPluginDescriptor) -> Duration {
    if let Ok(raw) = env::var("VIDEN_ACP_HANDSHAKE_TIMEOUT_SECS")
        && let Ok(seconds) = raw.parse::<u64>()
        && seconds > 0
    {
        return Duration::from_secs(seconds);
    }
    if matches!(agent.source, AgentSource::Registry) {
        Duration::from_secs(DEFAULT_REGISTRY_ACP_HANDSHAKE_TIMEOUT_SECS)
    } else {
        Duration::from_secs(DEFAULT_LOCAL_ACP_HANDSHAKE_TIMEOUT_SECS)
    }
}

pub(super) fn acp_session_prompt_timeout(agent: &AgentPluginDescriptor) -> Duration {
    if let Ok(raw) = env::var("VIDEN_ACP_SESSION_TIMEOUT_SECS")
        && let Ok(seconds) = raw.parse::<u64>()
        && seconds > 0
    {
        return Duration::from_secs(seconds);
    }
    if is_kiro_agent(agent) {
        Duration::from_secs(DEFAULT_KIRO_ACP_SESSION_TIMEOUT_SECS)
    } else {
        Duration::from_secs(DEFAULT_LOCAL_ACP_SESSION_TIMEOUT_SECS)
    }
}

pub(super) fn acp_update_kind(update: &Value) -> Option<String> {
    update
        .get("type")
        .or_else(|| update.get("sessionUpdate"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(super) fn acp_message_chunk_text(update: &Value) -> Option<String> {
    match update.get("content") {
        Some(Value::String(content)) => Some(content.clone()),
        Some(Value::Object(content)) => content
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => update
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

/// Reads a non-text ACP content block as a typed message part.
///
/// Text is already carried by `acp_message_chunk_text`, so this returns `None`
/// for it. A block whose bytes are inline rather than referenced is persisted
/// into the workspace first, and the returned evidence records those bytes; a
/// block that can be neither referenced nor persisted is preserved verbatim as
/// an unknown part, because dropping it would lose content and inventing a
/// reference for bytes Core has not persisted would be worse.
pub(super) fn acp_message_chunk_part(
    update: &Value,
    cwd: Option<&Path>,
    owner: Option<&RuntimeOwner>,
) -> Option<(AgentContentPart, Option<EvidenceView>)> {
    let content = update.get("content")?;
    let kind = content.get("type").and_then(Value::as_str)?;
    if kind == "text" {
        return None;
    }
    let media_type = content
        .get("mimeType")
        .or_else(|| content.get("mediaType"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let unknown = || {
        Some((
            AgentContentPart::Unknown {
                kind: kind.to_string(),
                payload: content.clone(),
            },
            None,
        ))
    };
    let referenced = content
        .get("uri")
        .or_else(|| content.get("reference"))
        .and_then(Value::as_str)
        .map(str::to_string);
    // A reference the Agent supplied is used as published. Only bytes with no
    // reference of their own are persisted, and then the workspace path Core
    // wrote becomes the reference.
    let (reference, evidence) = match referenced {
        Some(reference) => (reference, None),
        None => {
            let Some(cwd) = cwd else { return unknown() };
            let data = content
                .get("data")
                .or_else(|| content.get("blob"))
                .and_then(Value::as_str)?;
            match persist_acp_inline_bytes(cwd, kind, &media_type, data, owner) {
                Some(persisted) => persisted,
                None => return unknown(),
            }
        }
    };
    let part = match kind {
        "image" => AgentContentPart::Image {
            media_type,
            reference,
            alt: content
                .get("alt")
                .and_then(Value::as_str)
                .map(str::to_string),
        },
        "resource_link" | "resource" | "audio" => AgentContentPart::File {
            media_type,
            reference,
            name: content
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string),
        },
        _ => return unknown(),
    };
    Some((part, evidence))
}

/// Writes inline Agent bytes into the workspace and records them as evidence.
///
/// The file name is the digest of the bytes, so identical content resolves to
/// one immutable path: a replayed event can never point at a file that was
/// rewritten under it. Returns `None` when the payload is not decodable or the
/// workspace cannot be written, leaving the caller to preserve the raw block.
pub(super) fn persist_acp_inline_bytes(
    cwd: &Path,
    kind: &str,
    media_type: &str,
    data: &str,
    owner: Option<&RuntimeOwner>,
) -> Option<(String, Option<EvidenceView>)> {
    use base64::Engine as _;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.trim())
        .ok()?;
    let digest = sha256_hex(&bytes);
    let name = format!("{digest}.{}", acp_content_extension(media_type));
    let dir = cwd.join(".viden").join("agents").join("parts");
    fs::create_dir_all(&dir).ok()?;
    let path = dir.join(&name);
    if !path.exists() {
        fs::write(&path, &bytes).ok()?;
    }
    let reference = format!(".viden/agents/parts/{name}");
    let evidence = EvidenceView {
        id: format!("acp-content-{digest}"),
        kind: "agent-content".to_string(),
        summary: format!("{kind} returned by the Agent ({} bytes)", bytes.len()),
        path: Some(reference.clone()),
        source: Some("acp:content.v1".to_string()),
        canonical: None,
        metadata: Some(json!({
            "mediaType": media_type,
            "contentKind": kind,
            "sha256": digest,
            "bytes": bytes.len(),
        })),
        timestamp: None,
        owner: owner.cloned(),
    };
    Some((reference, Some(evidence)))
}

/// Maps a media type onto a file extension for persisted Agent content.
pub(super) fn acp_content_extension(media_type: &str) -> String {
    match media_type {
        "image/png" => return "png".to_string(),
        "image/jpeg" | "image/jpg" => return "jpg".to_string(),
        "image/gif" => return "gif".to_string(),
        "image/webp" => return "webp".to_string(),
        "image/svg+xml" => return "svg".to_string(),
        "application/pdf" => return "pdf".to_string(),
        _ => {}
    }
    let subtype = media_type
        .split('/')
        .nth(1)
        .unwrap_or_default()
        .split(['+', ';'])
        .next()
        .unwrap_or_default();
    let sanitized: String = subtype
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if sanitized.is_empty() {
        "bin".to_string()
    } else {
        sanitized
    }
}

pub(super) fn acp_patch_text(update: &Value) -> Option<String> {
    [
        "/diff",
        "/patch",
        "/unifiedDiff",
        "/unified_diff",
        "/content/diff",
        "/content/patch",
        "/content/unifiedDiff",
        "/content/unified_diff",
        "/fileChange/patch",
        "/fileChange/diff",
        "/file_change/patch",
        "/file_change/diff",
    ]
    .iter()
    .filter_map(|pointer| update.pointer(pointer).and_then(Value::as_str))
    .find(|text| looks_like_unified_diff(text))
    .map(str::to_string)
}

pub(super) fn acp_patch_path(update: &Value) -> Option<String> {
    [
        "/path",
        "/file",
        "/filePath",
        "/file_path",
        "/content/path",
        "/content/file",
        "/content/filePath",
        "/content/file_path",
        "/fileChange/path",
        "/fileChange/filePath",
        "/file_change/path",
        "/file_change/file_path",
    ]
    .iter()
    .find_map(|pointer| update.pointer(pointer).and_then(Value::as_str))
    .map(str::to_string)
}

pub(super) fn acp_patch_summary(patch: &str, explicit_path: Option<&str>) -> String {
    let metadata = acp_patch_metadata(patch, explicit_path, &Value::Null);
    let file_count = metadata
        .get("fileCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let additions = metadata
        .get("additions")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let deletions = metadata
        .get("deletions")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let path = explicit_path
        .map(str::to_string)
        .or_else(|| {
            metadata
                .pointer("/files/0/path")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "<unknown>".to_string());
    format!("ACP patch: {file_count} file(s), +{additions}/-{deletions}, first {path}")
}

pub(super) fn acp_patch_metadata(
    patch: &str,
    explicit_path: Option<&str>,
    update: &Value,
) -> Value {
    let (files, additions, deletions, hunks) = acp_patch_files(patch, explicit_path);
    json!({
        "schema": "acp.patch.v1",
        "format": "unified_diff",
        "fileCount": files.len(),
        "additions": additions,
        "deletions": deletions,
        "hunks": hunks,
        "files": files,
        "diff": patch,
        "origin": {
            "updateType": acp_update_kind(update),
            "toolCallId": update
                .get("toolCallId")
                .or_else(|| update.get("tool_call_id"))
                .and_then(Value::as_str),
        }
    })
}

pub(super) fn acp_patch_files(
    patch: &str,
    explicit_path: Option<&str>,
) -> (Vec<Value>, u64, u64, u64) {
    #[derive(Default)]
    struct FileStats {
        old_path: Option<String>,
        new_path: Option<String>,
        additions: u64,
        deletions: u64,
        hunks: u64,
    }

    fn push_file(files: &mut Vec<Value>, file: &mut Option<FileStats>) {
        let Some(stats) = file.take() else {
            return;
        };
        let path = stats
            .new_path
            .as_deref()
            .filter(|path| *path != "/dev/null")
            .or(stats
                .old_path
                .as_deref()
                .filter(|path| *path != "/dev/null"))
            .unwrap_or("<unknown>");
        files.push(json!({
            "path": path,
            "oldPath": stats.old_path,
            "newPath": stats.new_path,
            "additions": stats.additions,
            "deletions": stats.deletions,
            "hunks": stats.hunks,
        }));
    }

    let mut files = Vec::new();
    let mut current: Option<FileStats> = None;
    let mut total_additions = 0;
    let mut total_deletions = 0;
    let mut total_hunks = 0;

    for line in patch.lines() {
        if let Some((old_path, new_path)) = parse_diff_git_paths(line) {
            push_file(&mut files, &mut current);
            current = Some(FileStats {
                old_path: Some(old_path),
                new_path: Some(new_path),
                ..FileStats::default()
            });
            continue;
        }
        if let Some(old_path) = line.strip_prefix("--- ") {
            current.get_or_insert_with(FileStats::default).old_path =
                Some(normalize_diff_path(old_path));
            continue;
        }
        if let Some(new_path) = line.strip_prefix("+++ ") {
            current.get_or_insert_with(FileStats::default).new_path =
                Some(normalize_diff_path(new_path));
            continue;
        }
        if line.starts_with("@@") {
            let stats = current.get_or_insert_with(FileStats::default);
            stats.hunks += 1;
            total_hunks += 1;
            continue;
        }
        if line.starts_with('+') && !line.starts_with("+++") {
            let stats = current.get_or_insert_with(FileStats::default);
            stats.additions += 1;
            total_additions += 1;
            continue;
        }
        if line.starts_with('-') && !line.starts_with("---") {
            let stats = current.get_or_insert_with(FileStats::default);
            stats.deletions += 1;
            total_deletions += 1;
        }
    }
    push_file(&mut files, &mut current);

    if files.is_empty()
        && let Some(path) = explicit_path
    {
        files.push(json!({
            "path": path,
            "oldPath": path,
            "newPath": path,
            "additions": total_additions,
            "deletions": total_deletions,
            "hunks": total_hunks,
        }));
    }

    (files, total_additions, total_deletions, total_hunks)
}

pub(super) fn parse_diff_git_paths(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("diff --git ")?;
    let mut parts = rest.split_whitespace();
    let old_path = normalize_diff_path(parts.next()?);
    let new_path = normalize_diff_path(parts.next()?);
    Some((old_path, new_path))
}

pub(super) fn normalize_diff_path(path: &str) -> String {
    path.trim()
        .trim_matches('"')
        .strip_prefix("a/")
        .or_else(|| path.trim().trim_matches('"').strip_prefix("b/"))
        .unwrap_or_else(|| path.trim().trim_matches('"'))
        .to_string()
}

pub(super) fn looks_like_unified_diff(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("diff --git ") || (trimmed.contains("\n--- ") && trimmed.contains("\n+++ "))
}

pub(super) fn acp_prompt_response_status(response: &Value) -> String {
    response
        .pointer("/result/stopReason")
        .or_else(|| response.pointer("/result/status"))
        .and_then(Value::as_str)
        .unwrap_or("completed")
        .to_string()
}

pub(super) fn acp_usage_summary(response: &Value) -> Option<String> {
    let usage = response.pointer("/result/usage")?;
    let total = usage.get("totalTokens").and_then(Value::as_u64);
    let input = usage.get("inputTokens").and_then(Value::as_u64);
    let output = usage.get("outputTokens").and_then(Value::as_u64);
    match (total, input, output) {
        (Some(total), Some(input), Some(output)) => {
            Some(format!("total={total} input={input} output={output}"))
        }
        (Some(total), _, _) => Some(format!("total={total}")),
        _ => None,
    }
}

pub(super) fn acp_response_error_message(response: &Value) -> String {
    acp_response_error_message_for("ACP session/prompt failed", response)
}

pub(super) fn acp_response_error_message_for(context: &str, response: &Value) -> String {
    response
        .pointer("/error/message")
        .and_then(Value::as_str)
        .map(|message| format!("{context}: {message}"))
        .unwrap_or_else(|| context.to_string())
}

pub(super) fn acp_response_has_error(response: &str) -> bool {
    serde_json::from_str::<Value>(response)
        .ok()
        .is_some_and(|value| value.get("error").is_some())
}

pub(super) fn acp_response_is_method_not_found(response: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(response) else {
        return false;
    };
    value.pointer("/error/code").and_then(Value::as_i64) == Some(-32601)
}

pub(super) fn acp_session_new_request(cwd: &Path) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "session/new",
        "params": {
            "cwd": cwd.display().to_string(),
            "mcpServers": []
        }
    })
    .to_string()
}

pub(super) fn acp_session_load_request(cwd: &Path, session_id: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "session/load",
        "params": {
            "cwd": cwd.display().to_string(),
            "mcpServers": [],
            "sessionId": session_id
        }
    })
    .to_string()
}

pub(super) fn acp_authenticate_request(method_id: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "authenticate",
        "params": {
            "methodId": method_id
        }
    })
    .to_string()
}

pub(super) fn acp_auth_status_from_response(response: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(response) else {
        return "unknown".to_string();
    };
    value
        .pointer("/result/status")
        .or_else(|| value.pointer("/result/outcome/status"))
        .or_else(|| value.pointer("/result/outcome"))
        .and_then(Value::as_str)
        .unwrap_or("ok")
        .to_string()
}

pub(super) fn acp_session_prompt_request(
    agent: &AgentPluginDescriptor,
    session_id: &str,
    prompt: &str,
    id: u64,
) -> String {
    let text_blocks = json!([
        {
            "type": "text",
            "text": prompt
        }
    ]);
    let mut params = serde_json::Map::new();
    params.insert(
        "sessionId".to_string(),
        Value::String(session_id.to_string()),
    );
    if acp_agent_uses_content_prompt(agent) {
        params.insert("content".to_string(), text_blocks);
    } else {
        params.insert("prompt".to_string(), text_blocks);
    }
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session/prompt",
        "params": Value::Object(params)
    })
    .to_string()
}

pub(super) fn acp_session_set_mode_request(session_id: &str, mode_id: &str, id: u64) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session/set_mode",
        "params": {
            "sessionId": session_id,
            "modeId": mode_id
        }
    })
    .to_string()
}

pub(super) fn acp_session_set_model_request(session_id: &str, model_id: &str, id: u64) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session/set_config_option",
        "params": {
            "sessionId": session_id,
            "configId": "model",
            "value": model_id
        }
    })
    .to_string()
}

pub(super) fn acp_legacy_session_set_model_request(
    session_id: &str,
    model_id: &str,
    id: u64,
) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session/set_model",
        "params": {
            "sessionId": session_id,
            "modelId": model_id
        }
    })
    .to_string()
}

pub(super) fn acp_session_cancel_request(session_id: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": ACP_SESSION_CANCEL_REQUEST_ID,
        "method": "session/cancel",
        "params": {
            "sessionId": session_id
        }
    })
    .to_string()
}

pub(super) fn acp_agent_uses_content_prompt(_agent: &AgentPluginDescriptor) -> bool {
    false
}

pub(super) fn acp_session_id_from_response(response: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(response).ok()?;
    value
        .pointer("/result/sessionId")
        .or_else(|| value.pointer("/result/session/id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(super) fn acp_permission_prompt(request: &Value) -> viden_types::PermissionPrompt {
    let tool_call = request.pointer("/params/toolCall");
    let tool_id = tool_call
        .and_then(|tool_call| {
            tool_call
                .get("toolCallId")
                .or_else(|| tool_call.get("id"))
                .and_then(Value::as_str)
        })
        .unwrap_or("permission");
    let title = tool_call
        .and_then(|tool_call| {
            tool_call
                .get("title")
                .or_else(|| tool_call.get("name"))
                .and_then(Value::as_str)
        })
        .unwrap_or("ACP permission request");
    let kind = tool_call
        .and_then(|tool_call| tool_call.get("kind").and_then(Value::as_str))
        .unwrap_or("external-agent");
    viden_types::PermissionPrompt {
        tool_name: format!("acp:{tool_id}"),
        message: format!("{title} ({kind})"),
        input_preview: request
            .pointer("/params")
            .map(Value::to_string)
            .unwrap_or_else(|| "{}".to_string()),
        candidate_paths: acp_request_path(request).into_iter().collect(),
        // An ACP permission request describes an external agent's proposed
        // effect in the agent's own vocabulary. Core did not compute a
        // prospective result for it, so it publishes none rather than a
        // guess dressed as a diff.
        decision_context: None,
    }
}

pub(super) fn acp_permission_response(request: &Value, approved: bool) -> String {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    match acp_permission_option_id(request, approved) {
        Some(option_id) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "outcome": {
                    "type": "selected",
                    "optionId": option_id
                }
            }
        })
        .to_string(),
        None => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "outcome": {
                    "type": "cancelled"
                }
            }
        })
        .to_string(),
    }
}

pub(super) fn acp_filesystem_client_request_response(
    cwd: &Path,
    permission_engine: &mut PermissionEngine,
    approver: &mut impl FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    fs_capability: &Arc<dyn FilesystemCapability>,
    request: &Value,
) -> Option<String> {
    let method = request.get("method").and_then(Value::as_str)?;
    match method {
        "fs/read_text_file" => Some(acp_read_text_file_response(
            cwd,
            permission_engine,
            fs_capability,
            request,
        )),
        "fs/write_text_file" => Some(acp_write_text_file_response(
            cwd,
            permission_engine,
            approver,
            fs_capability,
            request,
        )),
        _ => None,
    }
}

pub(super) fn acp_read_text_file_response(
    cwd: &Path,
    permission_engine: &PermissionEngine,
    fs_capability: &Arc<dyn FilesystemCapability>,
    request: &Value,
) -> String {
    let id = acp_request_id(request);
    let Some(path) = acp_request_path(request) else {
        return acp_client_error_response(id, -32602, "fs/read_text_file requires params.path");
    };
    let input = acp_file_tool_input(&path, None);
    let tool = acp_file_tool_spec("read_file", false);
    match permission_engine.decide(&tool, &input) {
        PermissionDecision::Allow(_) => {
            match read_acp_text_file(fs_capability, cwd, &path, request) {
                Ok(content) => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": content
                    }
                })
                .to_string(),
                Err(error) => acp_client_error_response(id, -32002, &error),
            }
        }
        PermissionDecision::Ask(ask) => acp_client_error_response(id, -32003, &ask.message),
        PermissionDecision::Deny(deny) => acp_client_error_response(id, -32003, &deny.message),
    }
}

pub(super) fn acp_write_text_file_response(
    cwd: &Path,
    permission_engine: &mut PermissionEngine,
    approver: &mut impl FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    fs_capability: &Arc<dyn FilesystemCapability>,
    request: &Value,
) -> String {
    let id = acp_request_id(request);
    let Some(path) = acp_request_path(request) else {
        return acp_client_error_response(id, -32602, "fs/write_text_file requires params.path");
    };
    let Some(content) = request.pointer("/params/content").and_then(Value::as_str) else {
        return acp_client_error_response(id, -32602, "fs/write_text_file requires params.content");
    };
    let input = acp_file_tool_input(&path, Some(content));
    let tool = acp_file_tool_spec("write_file", true);
    let decision = viden_permissions::resolve_permission(
        permission_engine,
        &tool,
        "write_file",
        &input,
        |_ask, prompt| approver(prompt),
    );
    match decision {
        PermissionDecision::Allow(_) => {
            match write_acp_text_file(fs_capability, cwd, &path, content) {
                Ok(()) => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {}
                })
                .to_string(),
                Err(error) => acp_client_error_response(id, -32002, &error),
            }
        }
        PermissionDecision::Ask(_) => unreachable!("ask decisions should be resolved"),
        PermissionDecision::Deny(deny) => acp_client_error_response(id, -32003, &deny.message),
    }
}

pub(super) fn acp_request_id(request: &Value) -> Value {
    request.get("id").cloned().unwrap_or(Value::Null)
}

pub(super) fn acp_request_path(request: &Value) -> Option<String> {
    request
        .pointer("/params/path")
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(super) fn acp_file_tool_input(path: &str, content: Option<&str>) -> ToolInput {
    let mut input = ToolInput::new();
    input.insert("path".to_string(), path.to_string());
    if let Some(content) = content {
        input.insert("content".to_string(), content.to_string());
    }
    input
}

pub(super) fn acp_file_tool_spec(name: &str, is_mutating: bool) -> ToolSpec {
    ToolSpec {
        name: name.to_string(),
        description: format!("ACP {name} client request"),
        is_mutating,
        input_schema_hint: "path=absolute/or/relative content=optional".to_string(),
    }
}

#[derive(Default)]
pub(super) struct AcpTerminalStore {
    pub(super) next_id: u64,
    pub(super) records: BTreeMap<String, AcpTerminalRecord>,
}

pub(super) struct AcpTerminalRecord {
    /// Present while the process runs; spawned through `ProcessCapability` so
    /// ACP terminals stay behind the OS capability seam.
    pub(super) control: Option<Box<dyn InteractiveProcessControl>>,
    pub(super) stdin: Option<Box<dyn Write + Send>>,
    pub(super) stdout: mpsc::Receiver<std::io::Result<Vec<u8>>>,
    pub(super) stderr: mpsc::Receiver<std::io::Result<Vec<u8>>>,
    pub(super) output: String,
    pub(super) output_byte_limit: Option<u64>,
    pub(super) truncated: bool,
    pub(super) exit_code: Option<i32>,
    pub(super) signal: Option<String>,
    pub(super) released: bool,
    pub(super) killed: bool,
}

pub(super) fn acp_terminal_client_request_response(
    cwd: &Path,
    permission_engine: &mut PermissionEngine,
    approver: &mut impl FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    process_capability: &Arc<dyn ProcessCapability>,
    terminals: &mut AcpTerminalStore,
    request: &Value,
) -> Option<String> {
    let method = request.get("method").and_then(Value::as_str)?;
    match method {
        "terminal/create" => Some(acp_terminal_create_response(
            cwd,
            permission_engine,
            approver,
            process_capability,
            terminals,
            request,
        )),
        "terminal/input" | "terminal/write" => {
            Some(acp_terminal_input_response(terminals, request))
        }
        "terminal/output" => Some(acp_terminal_output_response(terminals, request)),
        "terminal/wait_for_exit" => Some(acp_terminal_wait_for_exit_response(terminals, request)),
        "terminal/release" => Some(acp_terminal_release_response(terminals, request)),
        "terminal/kill" => Some(acp_terminal_kill_response(terminals, request)),
        _ => None,
    }
}

pub(super) fn acp_terminal_create_response(
    cwd: &Path,
    permission_engine: &mut PermissionEngine,
    approver: &mut impl FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    process_capability: &Arc<dyn ProcessCapability>,
    terminals: &mut AcpTerminalStore,
    request: &Value,
) -> String {
    let id = acp_request_id(request);
    let Some(command) = request.pointer("/params/command").and_then(Value::as_str) else {
        return acp_client_error_response(id, -32602, "terminal/create requires params.command");
    };
    let args = acp_terminal_args(request);
    let exec_cwd = match acp_terminal_cwd(cwd, request) {
        Ok(path) => path,
        Err(error) => return acp_client_error_response(id, -32602, &error),
    };
    let command_preview = acp_terminal_command_preview(command, &args);
    let mut input = ToolInput::new();
    input.insert("command".to_string(), command_preview.clone());
    input.insert("path".to_string(), exec_cwd.display().to_string());
    let tool = ToolSpec {
        name: "shell".to_string(),
        description: "ACP terminal/create client request".to_string(),
        is_mutating: true,
        input_schema_hint: "command='cargo test' path=/workspace".to_string(),
    };
    let decision = viden_permissions::resolve_permission(
        permission_engine,
        &tool,
        "shell",
        &input,
        |_ask, prompt| approver(prompt),
    );
    match decision {
        PermissionDecision::Allow(_) => match spawn_acp_terminal_command(
            process_capability,
            command,
            &args,
            &exec_cwd,
            acp_terminal_env(request),
            request
                .pointer("/params/outputByteLimit")
                .and_then(Value::as_u64),
        ) {
            Ok(record) => {
                terminals.next_id += 1;
                let terminal_id = format!("acp-terminal-{}", terminals.next_id);
                terminals.records.insert(terminal_id.clone(), record);
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "terminalId": terminal_id
                    }
                })
                .to_string()
            }
            Err(error) => acp_client_error_response(id, -32002, &error),
        },
        PermissionDecision::Ask(_) => unreachable!("ask decisions should be resolved"),
        PermissionDecision::Deny(deny) => acp_client_error_response(id, -32003, &deny.message),
    }
}

pub(super) fn acp_terminal_input_response(
    terminals: &mut AcpTerminalStore,
    request: &Value,
) -> String {
    let id = acp_request_id(request);
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("terminal/input");
    let Some(terminal_id) = acp_request_terminal_id(request) else {
        return acp_client_error_response(
            id,
            -32602,
            &format!("{method} requires params.terminalId"),
        );
    };
    let Some(input) = acp_terminal_input_text(request) else {
        return acp_client_error_response(
            id,
            -32602,
            &format!("{method} requires params.input, params.text, params.content, or params.data"),
        );
    };
    let Some(record) = terminals.records.get_mut(&terminal_id) else {
        return acp_client_error_response(id, -32004, "unknown ACP terminal id");
    };
    acp_terminal_refresh(record);
    if record.control.is_none() {
        return acp_client_error_response(id, -32004, "ACP terminal is not running");
    }
    let Some(stdin) = record.stdin.as_mut() else {
        return acp_client_error_response(id, -32004, "ACP terminal stdin is unavailable");
    };
    if let Err(error) = stdin
        .write_all(input.as_bytes())
        .and_then(|_| stdin.flush())
    {
        record.stdin = None;
        return acp_client_error_response(
            id,
            -32002,
            &format!("failed to write ACP terminal input: {error}"),
        );
    }
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "bytesWritten": input.len()
        }
    })
    .to_string()
}

pub(super) fn acp_terminal_output_response(
    terminals: &mut AcpTerminalStore,
    request: &Value,
) -> String {
    let id = acp_request_id(request);
    let Some(terminal_id) = acp_request_terminal_id(request) else {
        return acp_client_error_response(id, -32602, "terminal/output requires params.terminalId");
    };
    let Some(record) = terminals.records.get_mut(&terminal_id) else {
        return acp_client_error_response(id, -32004, "unknown ACP terminal id");
    };
    acp_terminal_refresh(record);
    if record.output.is_empty() && record.control.is_some() {
        acp_terminal_poll_output(record, Duration::from_millis(150));
    }
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "output": record.output,
            "truncated": record.truncated,
            "exitStatus": acp_terminal_exit_status(record)
        }
    })
    .to_string()
}

pub(super) fn acp_terminal_wait_for_exit_response(
    terminals: &mut AcpTerminalStore,
    request: &Value,
) -> String {
    let id = acp_request_id(request);
    let Some(terminal_id) = acp_request_terminal_id(request) else {
        return acp_client_error_response(
            id,
            -32602,
            "terminal/wait_for_exit requires params.terminalId",
        );
    };
    let Some(record) = terminals.records.get_mut(&terminal_id) else {
        return acp_client_error_response(id, -32004, "unknown ACP terminal id");
    };
    acp_terminal_wait_for_exit(record);
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "exitCode": record.exit_code,
            "signal": record.signal
        }
    })
    .to_string()
}

pub(super) fn acp_terminal_release_response(
    terminals: &mut AcpTerminalStore,
    request: &Value,
) -> String {
    let id = acp_request_id(request);
    let Some(terminal_id) = acp_request_terminal_id(request) else {
        return acp_client_error_response(
            id,
            -32602,
            "terminal/release requires params.terminalId",
        );
    };
    let Some(record) = terminals.records.get_mut(&terminal_id) else {
        return acp_client_error_response(id, -32004, "unknown ACP terminal id");
    };
    acp_terminal_terminate(record, "released");
    record.released = true;
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {}
    })
    .to_string()
}

pub(super) fn acp_terminal_kill_response(
    terminals: &mut AcpTerminalStore,
    request: &Value,
) -> String {
    let id = acp_request_id(request);
    let Some(terminal_id) = acp_request_terminal_id(request) else {
        return acp_client_error_response(id, -32602, "terminal/kill requires params.terminalId");
    };
    let Some(record) = terminals.records.get_mut(&terminal_id) else {
        return acp_client_error_response(id, -32004, "unknown ACP terminal id");
    };
    acp_terminal_terminate(record, "killed");
    record.killed = true;
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {}
    })
    .to_string()
}

// The terminal child is a model-driven effect and is spawned only through the
// `ProcessCapability` seam; the record then owns the seam-provided stdin,
// output channels, and control handle.
pub(super) fn spawn_acp_terminal_command(
    process_capability: &Arc<dyn ProcessCapability>,
    command: &str,
    args: &[String],
    cwd: &Path,
    envs: Vec<(String, String)>,
    output_byte_limit: Option<u64>,
) -> Result<AcpTerminalRecord, String> {
    let spawned = process_capability
        .spawn_interactive(&InteractiveInvocation {
            program: command.to_string(),
            args: args.to_vec(),
            cwd: cwd.to_path_buf(),
            envs,
        })
        .map_err(|err| format!("failed to run ACP terminal command `{command}`: {err}"))?;
    Ok(AcpTerminalRecord {
        control: Some(spawned.control),
        stdin: Some(spawned.stdin),
        stdout: spawned.stdout,
        stderr: spawned.stderr,
        output: String::new(),
        output_byte_limit,
        truncated: false,
        exit_code: None,
        signal: None,
        released: false,
        killed: false,
    })
}

pub(super) fn acp_terminal_wait_timeout() -> Duration {
    env::var("VIDEN_ACP_TERMINAL_TIMEOUT_SECS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(30))
}

pub(super) fn acp_terminal_refresh(record: &mut AcpTerminalRecord) {
    acp_terminal_drain_output(record);
    if let Some(control) = record.control.as_mut() {
        match control.try_wait() {
            Ok(Some(exit_code)) => {
                record.exit_code = exit_code;
                record.stdin = None;
                record.control = None;
                acp_terminal_drain_output_after_exit(record);
            }
            Ok(None) => {}
            Err(error) => {
                record.signal = Some(format!("wait_error:{error}"));
                record.stdin = None;
                record.control = None;
            }
        }
    }
    acp_terminal_drain_output(record);
}

pub(super) fn acp_terminal_drain_output_after_exit(record: &mut AcpTerminalRecord) {
    for _ in 0..10 {
        acp_terminal_drain_output(record);
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub(super) fn acp_terminal_wait_for_exit(record: &mut AcpTerminalRecord) {
    let deadline = Instant::now() + acp_terminal_wait_timeout();
    loop {
        acp_terminal_refresh(record);
        if record.control.is_none() {
            break;
        }
        if Instant::now() >= deadline {
            acp_terminal_terminate(record, "timeout");
            acp_terminal_append_output(
                record,
                format!(
                    "\nACP terminal command timed out after {}s",
                    acp_terminal_wait_timeout().as_secs()
                )
                .as_bytes(),
            );
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    for _ in 0..10 {
        acp_terminal_drain_output(record);
        std::thread::sleep(Duration::from_millis(5));
    }
    acp_terminal_drain_output(record);
}

pub(super) fn acp_terminal_poll_output(record: &mut AcpTerminalRecord, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline && record.output.is_empty() && record.control.is_some() {
        acp_terminal_refresh(record);
        if !record.output.is_empty() || record.control.is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    acp_terminal_refresh(record);
}

pub(super) fn acp_terminal_terminate(record: &mut AcpTerminalRecord, signal: &str) {
    record.stdin = None;
    if let Some(control) = record.control.as_mut() {
        let _ = control.kill();
        if let Some(exit_code) =
            wait_interactive_exit_timeout(control.as_mut(), Duration::from_secs(1))
        {
            record.exit_code = exit_code;
        }
        record.signal = Some(signal.to_string());
        record.control = None;
    }
    acp_terminal_drain_output(record);
}

pub(super) fn acp_terminal_drain_output(record: &mut AcpTerminalRecord) {
    while let Ok(chunk) = record.stdout.try_recv() {
        match chunk {
            Ok(bytes) => acp_terminal_append_output(record, &bytes),
            Err(error) => {
                acp_terminal_append_output(record, format!("\nstdout error: {error}").as_bytes())
            }
        }
    }
    while let Ok(chunk) = record.stderr.try_recv() {
        match chunk {
            Ok(bytes) => acp_terminal_append_output(record, &bytes),
            Err(error) => {
                acp_terminal_append_output(record, format!("\nstderr error: {error}").as_bytes())
            }
        }
    }
}

pub(super) fn acp_terminal_append_output(record: &mut AcpTerminalRecord, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    record.output.push_str(&String::from_utf8_lossy(bytes));
    let (output, truncated) =
        truncate_terminal_output(record.output.clone(), record.output_byte_limit);
    record.output = output;
    record.truncated |= truncated;
}

pub(super) fn acp_terminal_args(request: &Value) -> Vec<String> {
    request
        .pointer("/params/args")
        .and_then(Value::as_array)
        .map(|args| {
            args.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn acp_terminal_input_text(request: &Value) -> Option<String> {
    [
        "/params/input",
        "/params/text",
        "/params/content",
        "/params/data",
    ]
    .iter()
    .find_map(|path| {
        request
            .pointer(path)
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

pub(super) fn acp_terminal_env(request: &Value) -> Vec<(String, String)> {
    request
        .pointer("/params/env")
        .and_then(Value::as_array)
        .map(|envs| {
            envs.iter()
                .filter_map(|entry| {
                    let name = entry.get("name").and_then(Value::as_str)?;
                    let value = entry.get("value").and_then(Value::as_str)?;
                    Some((name.to_string(), value.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn acp_terminal_cwd(cwd: &Path, request: &Value) -> Result<PathBuf, String> {
    let raw = request
        .pointer("/params/cwd")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());
    let path = raw
        .map(|raw| resolve_acp_path(cwd, raw))
        .unwrap_or_else(|| cwd.to_path_buf());
    if !path.exists() {
        return Err(format!("terminal cwd does not exist: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!(
            "terminal cwd is not a directory: {}",
            path.display()
        ));
    }
    Ok(path)
}

pub(super) fn acp_terminal_command_preview(command: &str, args: &[String]) -> String {
    std::iter::once(command.to_string())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn acp_request_terminal_id(request: &Value) -> Option<String> {
    request
        .pointer("/params/terminalId")
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(super) fn acp_terminal_exit_status(record: &AcpTerminalRecord) -> Value {
    json!({
        "exitCode": record.exit_code,
        "signal": record.signal
    })
}

pub(super) fn truncate_terminal_output(output: String, limit: Option<u64>) -> (String, bool) {
    let Some(limit) = limit.map(|value| value as usize) else {
        return (output, false);
    };
    if output.len() <= limit {
        return (output, false);
    }
    let mut start = output.len().saturating_sub(limit);
    while start < output.len() && !output.is_char_boundary(start) {
        start += 1;
    }
    (output[start..].to_string(), true)
}

// ACP client file requests are model-driven effects: they must reach the OS
// only through the shared `FilesystemCapability` seam so one provider swap
// relocates them together with the built-in file tools.
pub(super) fn read_acp_text_file(
    fs_capability: &Arc<dyn FilesystemCapability>,
    cwd: &Path,
    raw_path: &str,
    request: &Value,
) -> Result<String, String> {
    let path = resolve_acp_path(cwd, raw_path);
    if fs_capability.is_dir(&path) {
        return Err(format!("`{}` is a directory", path.display()));
    }
    let content = fs_capability
        .read_to_string(&path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    Ok(slice_acp_file_content(&content, request))
}

pub(super) fn write_acp_text_file(
    fs_capability: &Arc<dyn FilesystemCapability>,
    cwd: &Path,
    raw_path: &str,
    content: &str,
) -> Result<(), String> {
    let path = resolve_acp_path(cwd, raw_path);
    if fs_capability.is_dir(&path) {
        return Err(format!("`{}` is a directory", path.display()));
    }
    if let Some(parent) = path.parent() {
        fs_capability
            .create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    fs_capability
        .write(&path, content)
        .map_err(|err| format!("failed to write {}: {err}", path.display()))
}

pub(super) fn resolve_acp_path(cwd: &Path, raw_path: &str) -> PathBuf {
    let path = PathBuf::from(raw_path);
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}

pub(super) fn slice_acp_file_content(content: &str, request: &Value) -> String {
    let start_line = request
        .pointer("/params/startLine")
        .or_else(|| request.pointer("/params/line"))
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .saturating_sub(1) as usize;
    let limit = request
        .pointer("/params/limit")
        .or_else(|| request.pointer("/params/lineCount"))
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let lines = content.lines().skip(start_line);
    match limit {
        Some(limit) => lines.take(limit).collect::<Vec<_>>().join("\n"),
        None => lines.collect::<Vec<_>>().join("\n"),
    }
}

pub(super) fn acp_client_error_response(id: Value, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
    .to_string()
}

pub(super) fn acp_unsupported_client_request_response(request: &Value) -> Option<String> {
    let method = request.get("method").and_then(Value::as_str)?;
    if !method.starts_with("fs/") && !method.starts_with("terminal/") {
        return None;
    }
    let id = request.get("id")?.clone();
    Some(
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32001,
                "message": format!("ACP client method {method} is not available through the Viden ACP runtime bridge")
            }
        })
        .to_string(),
    )
}

pub(super) fn acp_permission_option_id(request: &Value, approved: bool) -> Option<String> {
    let options = request.pointer("/params/options")?.as_array()?;
    let preferred = options.iter().find(|option| {
        let kind = option
            .get("kind")
            .or_else(|| option.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        let option_id = option
            .get("optionId")
            .or_else(|| option.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        if approved {
            kind.contains("allow") || kind.contains("approve") || option_id.contains("allow")
        } else {
            kind.contains("reject") || kind.contains("deny") || option_id.contains("deny")
        }
    });
    preferred
        .or_else(|| options.first())
        .and_then(|option| {
            option
                .get("optionId")
                .or_else(|| option.get("id"))
                .and_then(Value::as_str)
        })
        .map(str::to_string)
}

pub(super) fn acp_tool_call_summary(update: &Value) -> String {
    let id = acp_tool_call_id(update);
    let status = update
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("updated");
    let content = update
        .get("content")
        .and_then(Value::as_str)
        .or_else(|| update.get("title").and_then(Value::as_str))
        .unwrap_or("");
    if content.is_empty() {
        format!("{id}:{status}")
    } else {
        format!("{id}:{status}:{content}")
    }
}

pub(super) fn acp_tool_call_id(update: &Value) -> String {
    update
        .get("toolCallId")
        .or_else(|| update.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("tool")
        .to_string()
}

pub(super) fn acp_tool_call_title(update: &Value) -> String {
    update
        .get("title")
        .or_else(|| update.get("name"))
        .or_else(|| update.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("acp_tool")
        .to_string()
}

pub(super) fn finish_failed_probe(
    mut child: Child,
    log_path: PathBuf,
    mut entries: Vec<String>,
    error: String,
) -> String {
    entries.push(jsonl_event("error", &error));
    let _ = child.kill();
    let exited = wait_child_timeout(&mut child, Duration::from_secs(2));
    let stderr = exited.then(|| child_stderr_tail(&mut child)).flatten();
    if let Some(stderr) = &stderr {
        entries.push(jsonl_event("stderr", stderr));
    }
    if !exited {
        entries.push(jsonl_event(
            "error",
            "ACP child did not exit within cleanup timeout after termination",
        ));
    }
    let _ = write_probe_log(&log_path, &entries);
    if let Some(stderr) = stderr {
        return format!("{error}; stderr: {stderr}; log {}", log_path.display());
    }
    format!("{error}; log {}", log_path.display())
}

pub(super) fn finish_expected_acp_stop(
    mut child: Child,
    log_path: PathBuf,
    mut entries: Vec<String>,
    message: String,
) -> String {
    entries.push(jsonl_event("error", &message));
    let _ = write_probe_log(&log_path, &entries);
    let _ = child.kill();
    let _ = wait_child_timeout(&mut child, Duration::from_secs(1));
    format!("{message}; log {}", log_path.display())
}

pub(super) fn acp_initialize_request() -> String {
    [
        r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"#,
        r#""protocolVersion":1,"#,
        r#""clientCapabilities":{"fs":{"readTextFile":true,"writeTextFile":true},"terminal":true},"#,
        r#""clientInfo":{"name":"viden","version":"0.1.6"}}"#,
        r#"}"#,
    ]
    .join("")
}

pub(super) fn spawn_acp_process(cwd: &Path, command: &str) -> Result<Child, String> {
    let mut command = shell_command(cwd, command)?;
    command
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to launch ACP command: {err}"))
}

pub(super) fn spawn_acp_agent_process(
    cwd: &Path,
    agent: &AgentPluginDescriptor,
) -> Result<Child, String> {
    let mut command = Command::new(&agent.command.command);
    command
        .args(acp_agent_command_args(agent))
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_acp_agent_process_env(cwd, agent, &mut command)?;
    command.spawn().map_err(|err| {
        format!(
            "failed to launch ACP agent `{}` with `{}`: {err}",
            agent.agent_id,
            agent_command_line(agent)
        )
    })
}

pub(super) fn configure_acp_agent_process_env(
    cwd: &Path,
    agent: &AgentPluginDescriptor,
    command: &mut Command,
) -> Result<(), String> {
    if matches!(agent.source, AgentSource::Registry) {
        // npm exec keeps generated bin shims under the cache's `_npx`
        // directory. Isolate each registry release so an interrupted install
        // cannot poison other agents or a later package version.
        let cache_dir = cwd
            .join(".viden")
            .join("cache")
            .join("npm")
            .join(acp_registry_cache_component(&agent.agent_id))
            .join(acp_registry_cache_component(&agent.version));
        fs::create_dir_all(&cache_dir).map_err(|err| {
            format!(
                "failed to create ACP registry npm cache {}: {err}",
                cache_dir.display()
            )
        })?;
        command
            .env("npm_config_cache", &cache_dir)
            .env("NPM_CONFIG_CACHE", &cache_dir)
            .env("npm_config_audit", "false")
            .env("npm_config_fund", "false")
            .env("npm_config_update_notifier", "false");
    }
    Ok(())
}

pub(super) fn acp_registry_cache_component(value: &str) -> String {
    let component = value
        .chars()
        .take(80)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if component.is_empty() {
        "unknown".to_string()
    } else {
        component
    }
}

pub(super) fn acp_probe_log_path(cwd: &Path) -> PathBuf {
    cwd.join(".viden")
        .join("agents")
        .join(format!("acp-doctor-{}.jsonl", timestamp_millis()))
}

pub(super) fn acp_session_log_path(cwd: &Path) -> PathBuf {
    cwd.join(".viden")
        .join("agents")
        .join(format!("acp-session-{}.jsonl", timestamp_millis()))
}

pub(super) fn agent_label_from_response(response: &str) -> String {
    let name = json_string_field(response, "name").unwrap_or_else(|| "unknown".to_string());
    let version = json_string_field(response, "version");
    match version {
        Some(version) => format!("{name} {version}"),
        None => name,
    }
}

pub(super) fn acp_probe_evidence_from_response(
    response: &str,
    log_path: PathBuf,
) -> AcpProbeEvidence {
    let Ok(value) = serde_json::from_str::<Value>(response) else {
        return AcpProbeEvidence {
            protocol_version: json_number_field(response, "protocolVersion")
                .unwrap_or_else(|| "unknown".to_string()),
            agent_label: agent_label_from_response(response),
            auth_methods: Vec::new(),
            auth_method_ids: Vec::new(),
            capabilities: Vec::new(),
            log_path,
        };
    };
    let protocol_version = value
        .pointer("/result/protocolVersion")
        .and_then(Value::as_i64)
        .map(|version| version.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let name = value
        .pointer("/result/agentInfo/name")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let version = value
        .pointer("/result/agentInfo/version")
        .and_then(Value::as_str);
    let agent_label = match version {
        Some(version) => format!("{name} {version}"),
        None => name.to_string(),
    };
    let (auth_methods, auth_method_ids) = acp_auth_methods_from_value(&value);
    AcpProbeEvidence {
        protocol_version,
        agent_label,
        auth_methods,
        auth_method_ids,
        capabilities: acp_capabilities_from_value(&value),
        log_path,
    }
}

pub(super) fn acp_auth_methods_from_value(value: &Value) -> (Vec<String>, Vec<String>) {
    let Some(methods) = value
        .pointer("/result/authMethods")
        .and_then(Value::as_array)
    else {
        return (Vec::new(), Vec::new());
    };
    let mut labels = Vec::new();
    let mut ids = Vec::new();
    for method in methods {
        let Some(id) = method.get("id").and_then(Value::as_str) else {
            continue;
        };
        ids.push(id.to_string());
        let name = method
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(id)
            .trim();
        if name == id {
            labels.push(id.to_string());
        } else {
            labels.push(format!("{id} ({name})"));
        }
    }
    (labels, ids)
}

pub(super) fn acp_capabilities_from_value(value: &Value) -> Vec<String> {
    let mut capabilities = Vec::new();
    if let Some(object) = value
        .pointer("/result/agentCapabilities")
        .and_then(Value::as_object)
    {
        for (key, item) in object {
            match item {
                Value::Bool(true) => capabilities.push(key.clone()),
                Value::Object(nested) if nested.is_empty() => capabilities.push(key.clone()),
                Value::Object(nested) => {
                    for nested_key in nested.keys() {
                        capabilities.push(format!("{key}.{nested_key}"));
                    }
                }
                _ => {}
            }
        }
    }
    capabilities.sort();
    capabilities
}
