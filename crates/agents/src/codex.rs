use std::{
    collections::HashSet,
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

use super::acp::*;
use super::infra::*;
use viden_types::RuntimeOwner;

/// Build the Codex CLI argument vector for a delegated run under `sandbox`
/// (`read-only` or `workspace-write`).
pub fn codex_run_command_args(cwd: &Path, sandbox: &str, task: String) -> Vec<String> {
    vec![
        "exec".to_string(),
        "--cd".to_string(),
        cwd.display().to_string(),
        "--sandbox".to_string(),
        sandbox.to_string(),
        task,
    ]
}

/// `/agent review codex [--base <ref>] [prompt]`: start a tracked Codex review
/// job over the working tree's diff against `--base`.
pub fn handle_codex_review_command(cwd: &Path, args: &[String]) -> Result<String, String> {
    ensure_codex_target(args.first().map(String::as_str))?;
    let parsed = parse_codex_review_args(&args[1..])?;
    let mut command_args = vec!["review".to_string(), "--uncommitted".to_string()];
    if let Some(base) = parsed.base {
        command_args.extend(["--base".to_string(), base]);
    }
    if !parsed.prompt.is_empty() {
        command_args.push(parsed.prompt.clone());
    }
    start_codex_job(
        cwd,
        &codex_command(),
        CodexJobKind::Review,
        if parsed.prompt.is_empty() {
            "Review current working tree".to_string()
        } else {
            parsed.prompt
        },
        command_args,
    )
}

/// `/agent challenge codex [prompt]`: start a tracked Codex job that argues
/// against the current change rather than reviewing it neutrally.
pub fn handle_codex_challenge_command(cwd: &Path, args: &[String]) -> Result<String, String> {
    ensure_codex_target(args.first().map(String::as_str))?;
    let prompt = args[1..].join(" ");
    let challenge_prompt = if prompt.trim().is_empty() {
        "Run an adversarial code review. Challenge assumptions, look for regressions, missing tests, and unsafe implementation shortcuts.".to_string()
    } else {
        format!(
            "Run an adversarial code review focused on this request: {}. Challenge assumptions, look for regressions, missing tests, and unsafe implementation shortcuts.",
            prompt.trim()
        )
    };
    start_codex_job(
        cwd,
        &codex_command(),
        CodexJobKind::Challenge,
        challenge_prompt.clone(),
        vec![
            "review".to_string(),
            "--uncommitted".to_string(),
            challenge_prompt,
        ],
    )
}

pub(super) fn handle_codex_probe_command(cwd: &Path, args: &[String]) -> Result<String, String> {
    ensure_codex_target(args.first().map(String::as_str))?;
    let mode = parse_codex_probe_args(&args[1..])?;
    if matches!(mode, CodexProbeMode::Turn { write: true, .. })
        && !env_is_configured("VIDEN_EXPERIMENTAL_CODEX_APP_SERVER_WRITE")
    {
        return Err(
            "`/agent probe codex --turn-write` is disabled by default because Codex app-server workspace-write turns can mutate files before Viden receives an approval request. Set VIDEN_EXPERIMENTAL_CODEX_APP_SERVER_WRITE=1 only in a disposable workspace."
                .to_string(),
        );
    }
    let evidence = run_codex_app_server_probe(cwd, &codex_command(), mode.clone())?;
    let tracked_job = if let CodexProbeMode::Turn { task, .. } = &mode {
        Some(record_codex_app_server_turn_probe(cwd, task, &evidence)?)
    } else {
        None
    };
    let notification_count = evidence.notifications.len();
    let notifications = if evidence.notifications.is_empty() {
        "none".to_string()
    } else {
        evidence.notifications.join(", ")
    };
    let thread = evidence
        .thread_id
        .as_ref()
        .map(|thread_id| format!("  thread: {thread_id}\n"))
        .unwrap_or_default();
    let turn = evidence
        .turn_id
        .as_ref()
        .map(|turn_id| {
            let status = evidence.turn_status.as_deref().unwrap_or("unknown");
            format!("  turn: {turn_id} ({status})\n")
        })
        .unwrap_or_default();
    let tracked = tracked_job
        .as_ref()
        .map(|id| format!("  tracked_job: {id}\n"))
        .unwrap_or_default();
    Ok(format!(
        "Codex app-server probe ok.\n  user_agent: {}\n  codex_home: {}\n  platform: {}\n{}{}{}  notifications: {} ({})\n  log: {}",
        evidence.user_agent,
        evidence.codex_home,
        evidence.platform,
        thread,
        turn,
        tracked,
        notification_count,
        notifications,
        evidence.log_path.display()
    ))
}

pub(super) fn parse_codex_probe_args(args: &[String]) -> Result<CodexProbeMode, String> {
    let mut index = 0;
    let mut mode = CodexProbeMode::Initialize;
    while index < args.len() {
        match args[index].as_str() {
            "--thread" => mode = CodexProbeMode::Thread,
            "--turn" => {
                let task = args[index + 1..].join(" ");
                if task.trim().is_empty() {
                    return Err("Usage: /agent probe codex --turn <task>".to_string());
                }
                mode = CodexProbeMode::Turn { task, write: false };
                break;
            }
            "--turn-write" => {
                let task = args[index + 1..].join(" ");
                if task.trim().is_empty() {
                    return Err("Usage: /agent probe codex --turn-write <task>".to_string());
                }
                mode = CodexProbeMode::Turn { task, write: true };
                break;
            }
            other => {
                return Err(format!(
                    "Unknown Codex probe option `{other}`. Usage: /agent probe codex [--thread|--turn <task>|--turn-write <task>]"
                ));
            }
        }
        index += 1;
    }
    Ok(mode)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParsedCodexReviewArgs {
    pub(super) base: Option<String>,
    pub(super) prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A parsed `/agent run codex` invocation.
///
/// Unlike [`AcpRunArgs`](crate::AcpRunArgs) the fields are public: the caller
/// must read `write` to decide whether a permission clearance is required
/// before the job may start.
pub struct ParsedCodexRunArgs {
    /// `--write`: the job may mutate the workspace, so the caller must clear it
    /// through the permission gate before starting the job.
    pub write: bool,
    /// `--app-server`: drive Codex over its JSON-RPC app server instead of the
    /// one-shot CLI.
    pub app_server: bool,
    /// The delegated task text.
    pub task: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CodexProbeMode {
    Initialize,
    Thread,
    Turn { task: String, write: bool },
}

/// Which delegated Codex job a tracked record describes. Persisted as its
/// lowercase name, so the variants are part of the on-disk job format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexJobKind {
    Review,
    Challenge,
    Run,
}

impl CodexJobKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Review => "review",
            Self::Challenge => "challenge",
            Self::Run => "run",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CodexJobRecord {
    pub(super) id: String,
    pub(super) kind: String,
    pub(super) status: String,
    pub(super) pid: Option<u32>,
    pub(super) command: String,
    pub(super) task: String,
    pub(super) log_path: PathBuf,
    pub(super) result_path: PathBuf,
    pub(super) baseline_path: PathBuf,
    pub(super) updated_at: u128,
    pub(super) agent: Option<AgentJobMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AgentJobMetadata {
    pub(super) agent_id: String,
    pub(super) model: Option<String>,
    pub(super) owner: RuntimeOwner,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct CodexJobEvidence {
    pub(super) session_id: Option<String>,
    pub(super) files: Vec<String>,
}

/// Reject any `/agent <review|challenge|run>` target other than `codex`.
///
/// The Codex band is one strategy among several adapters, so the target word
/// is validated before the command is interpreted further.
pub fn ensure_codex_target(target: Option<&str>) -> Result<(), String> {
    match target {
        Some("codex") => Ok(()),
        Some(other) => Err(format!(
            "Unsupported agent `{other}` for this command. Use `codex`."
        )),
        None => Err("Usage: /agent <review|challenge|run> codex ...".to_string()),
    }
}

pub(super) fn parse_codex_review_args(args: &[String]) -> Result<ParsedCodexReviewArgs, String> {
    let mut base = None;
    let mut prompt = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--base" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("Usage: /agent review codex [--base <ref>] [prompt]".to_string());
                };
                base = Some(value.clone());
                index += 2;
            }
            value => {
                prompt.push(value.to_string());
                index += 1;
            }
        }
    }
    Ok(ParsedCodexReviewArgs {
        base,
        prompt: prompt.join(" "),
    })
}

/// Parse the argument tail of `/agent run codex` into a [`ParsedCodexRunArgs`].
///
/// The caller inspects [`ParsedCodexRunArgs::write`] to decide whether the job
/// needs a permission clearance before it may start.
pub fn parse_codex_run_args(args: &[String]) -> Result<ParsedCodexRunArgs, String> {
    let mut write = false;
    let mut app_server = false;
    let mut task = Vec::new();
    let mut end_of_options = false;
    for value in args {
        match value.as_str() {
            _ if end_of_options => task.push(value.clone()),
            "--write" => write = true,
            "--read-only" => write = false,
            "--app-server" => app_server = true,
            "--" => end_of_options = true,
            other if other.starts_with("--") && task.is_empty() => {
                return Err(format!(
                    "Unknown Codex run option `{other}`. Usage: /agent run codex [--write|--app-server] <task>"
                ));
            }
            _ => task.push(value.clone()),
        }
    }
    Ok(ParsedCodexRunArgs {
        write,
        app_server,
        task: task.join(" "),
    })
}

/// Spawn a tracked Codex CLI job and record it under `.viden/agents`.
///
/// The job runs detached; the returned string is the operator-facing
/// acknowledgement carrying the new job id.
pub fn start_codex_job(
    cwd: &Path,
    command: &str,
    kind: CodexJobKind,
    task: String,
    mut args: Vec<String>,
) -> Result<String, String> {
    let id = format!("codex-{}", timestamp_millis());
    let log_path = codex_job_artifact_path(cwd, &id, "log");
    let result_path = codex_job_artifact_path(cwd, &id, "result.md");
    let baseline_path = codex_job_artifact_path(cwd, &id, "baseline.status");
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    write_codex_status_baseline(cwd, &baseline_path)?;
    if kind == CodexJobKind::Run {
        let insert_at = args.len().saturating_sub(1);
        args.insert(insert_at, "-o".to_string());
        args.insert(insert_at + 1, result_path.display().to_string());
    }
    let stdout = fs::File::create(&log_path)
        .map_err(|err| format!("failed to create Codex job log: {err}"))?;
    let stderr = stdout
        .try_clone()
        .map_err(|err| format!("failed to clone Codex job log: {err}"))?;
    let mut child = Command::new(command)
        .args(&args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|err| {
            format!(
                "failed to launch `{}`: {err}",
                command_line_owned(command, &args)
            )
        })?;
    let pid = child.id();

    let record = CodexJobRecord {
        id: id.clone(),
        kind: kind.as_str().to_string(),
        status: "running".to_string(),
        pid: Some(pid),
        command: command_line_owned(command, &args),
        task,
        log_path: log_path.clone(),
        result_path: result_path.clone(),
        baseline_path: baseline_path.clone(),
        updated_at: timestamp_millis(),
        agent: None,
    };
    append_codex_job_record(cwd, "started", &record)?;
    let monitor_cwd = cwd.to_path_buf();
    let monitor_cancel_path = acp_job_cancel_path(cwd, &id);
    let mut monitor_record = record.clone();
    std::thread::spawn(move || {
        let status = child.wait();
        if monitor_cancel_path.exists()
            || find_codex_job(&monitor_cwd, &monitor_record.id)
                .ok()
                .flatten()
                .is_some_and(|job| job.status == "cancelled")
        {
            return;
        }
        monitor_record.status = match status {
            Ok(status) if status.success() => "finished".to_string(),
            Ok(_) | Err(_) => "failed".to_string(),
        };
        monitor_record.updated_at = timestamp_millis();
        let _ = append_codex_job_record(&monitor_cwd, "completed", &monitor_record);
    });

    Ok(format!(
        "Started Codex {} job `{}`.\n  pid: {}\n  log: {}\n  result: {}\n\nUse `/agent status` to watch it and `/agent result {}` to read output.",
        kind.as_str(),
        id,
        pid,
        log_path.display(),
        result_path.display(),
        id
    ))
}

/// Spawn a tracked Codex job over the app-server JSON-RPC transport instead of
/// the one-shot CLI. Read-only delegated tasks only.
pub fn start_codex_app_server_job(
    cwd: &Path,
    command: &str,
    task: String,
) -> Result<String, String> {
    let id = format!("codex-app-{}", timestamp_millis());
    let log_path = codex_job_artifact_path(cwd, &id, "jsonl");
    let result_path = codex_job_artifact_path(cwd, &id, "result.md");
    let baseline_path = codex_job_artifact_path(cwd, &id, "baseline.status");
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    write_codex_status_baseline(cwd, &baseline_path)?;
    let record = CodexJobRecord {
        id: id.clone(),
        kind: "app-server-turn".to_string(),
        status: "running".to_string(),
        pid: None,
        command: "codex app-server turn/start".to_string(),
        task: task.clone(),
        log_path: log_path.clone(),
        result_path: result_path.clone(),
        baseline_path: baseline_path.clone(),
        updated_at: timestamp_millis(),
        agent: None,
    };
    append_codex_job_record(cwd, "started", &record)?;

    let monitor_cwd = cwd.to_path_buf();
    let monitor_command = command.to_string();
    let monitor_task = task.clone();
    let mut monitor_record = record.clone();
    std::thread::spawn(move || {
        match run_codex_app_server_probe_with_log(
            &monitor_cwd,
            &monitor_command,
            CodexProbeMode::Turn {
                task: monitor_task,
                write: false,
            },
            log_path,
        ) {
            Ok(evidence) => {
                let _ = write_codex_app_server_turn_result(&result_path, &evidence);
                monitor_record.status = codex_app_server_turn_job_status(&evidence);
            }
            Err(error) => {
                // Host bookkeeping (job result scratch file), not a
                // model-driven effect: stays outside the capability seam.
                let _ = fs::write(
                    &result_path,
                    format!("# Codex app-server turn failed\n\n{error}\n"),
                );
                monitor_record.status = "failed".to_string();
            }
        }
        monitor_record.updated_at = timestamp_millis();
        let _ = append_codex_job_record(&monitor_cwd, "completed", &monitor_record);
    });

    Ok(format!(
        "Started Codex app-server job `{}`.\n  log: {}\n  result: {}\n\nUse `/agent status` to watch it and `/agent result {}` to read output.",
        id,
        record.log_path.display(),
        record.result_path.display(),
        id
    ))
}

/// Render the tracked Codex job table for `cwd`, refreshing each record's
/// liveness from its recorded pid.
pub fn render_codex_job_status(cwd: &Path) -> Result<String, String> {
    let jobs = latest_codex_jobs(cwd)?;
    if jobs.is_empty() {
        return Ok("Agent jobs:\n  no tracked jobs".to_string());
    }
    let mut lines = vec![
        "Agent jobs:".to_string(),
        "  id                    kind          status       pid     updated     task".to_string(),
    ];
    let mut observed = Vec::new();
    for mut job in jobs.into_iter().rev().take(8) {
        let observed_status = observed_codex_status(&job);
        if observed_status != job.status {
            job.status = observed_status;
            job.updated_at = timestamp_millis();
            observed.push(job.clone());
        }
        lines.push(format!(
            "  {:<21} {:<13} {:<12} {:<7} {:<11} {}",
            job.id,
            job.kind,
            job.status,
            job.pid
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "-".to_string()),
            relative_millis(job.updated_at),
            truncate_for_line(&job.task, 72)
        ));
        let evidence = codex_job_evidence(cwd, &job);
        if let Some(session_id) = evidence.session_id {
            lines.push(agent_job_session_line(&job, &session_id));
        }
        if !evidence.files.is_empty() {
            lines.push(format!(
                "    files: {}",
                evidence
                    .files
                    .into_iter()
                    .take(5)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    for job in observed {
        append_codex_job_record(cwd, "observed", &job)?;
    }
    Ok(lines.join("\n"))
}

/// Render one finished Codex job's captured result, or the most recent job's
/// when `id` is `None`.
pub fn render_codex_job_result(cwd: &Path, id: Option<&str>) -> Result<String, String> {
    let id = id.ok_or_else(|| "Usage: /agent result <id>".to_string())?;
    let job = find_codex_job(cwd, id)?.ok_or_else(|| format!("Unknown agent job `{id}`"))?;
    let status = observed_codex_status(&job);
    let result = fs::read_to_string(&job.result_path)
        .ok()
        .filter(|content| !content.trim().is_empty())
        .or_else(|| tail_text(&job.log_path, 60).ok())
        .unwrap_or_else(|| "No output captured yet.".to_string());
    let evidence = codex_job_evidence_from_text(cwd, &job, &result);
    let resume = evidence
        .session_id
        .as_ref()
        .map(|session_id| format!("{}\n", agent_job_session_line(&job, session_id)))
        .unwrap_or_default();
    let files = if evidence.files.is_empty() {
        String::new()
    } else {
        format!("  files: {}\n", evidence.files.join(", "))
    };
    Ok(format!(
        "{} `{}`\n  kind: {}\n  status: {}\n  pid: {}\n  command: {}\n  log: {}\n  result: {}\n{}{}\n{}",
        agent_job_label(&job),
        job.id,
        job.kind,
        status,
        job.pid
            .map(|pid| pid.to_string())
            .unwrap_or_else(|| "-".to_string()),
        job.command,
        job.log_path.display(),
        job.result_path.display(),
        resume,
        files,
        result.trim()
    ))
}

/// Bounded window for a live ACP session to deliver `session/cancel` before
/// cancellation falls back to process termination.
pub(super) const ACP_SESSION_CANCEL_GRACE: Duration = Duration::from_millis(1500);

/// Tests that assert cooperative-cancel evidence widen the window because
/// parallel test load can starve the session worker past the interactive
/// bound, and expiry legitimately selects the process-termination fallback,
/// which erases the evidence those tests assert.
#[cfg(test)]
pub(super) static ACP_SESSION_CANCEL_GRACE_OVERRIDE_MS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

pub(super) fn acp_session_cancel_grace() -> Duration {
    #[cfg(test)]
    {
        let override_ms =
            ACP_SESSION_CANCEL_GRACE_OVERRIDE_MS.load(std::sync::atomic::Ordering::Relaxed);
        if override_ms > 0 {
            return Duration::from_millis(override_ms);
        }
    }
    ACP_SESSION_CANCEL_GRACE
}

/// Why a live agent job was interrupted (`runtime.agent_session_control`).
///
/// The three differ in exactly two places, and both of them need the answer
/// after the operator command has returned: how long the adapter is given to
/// stop cooperatively, and whether the ACP child survives the interruption.
/// The reason therefore travels in the job's own marker file rather than in
/// memory, so the ACP runner and the session monitor — neither of which the
/// commanding thread can reach — read the same intent the operator expressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AgentJobInterrupt {
    /// Cooperative cancel with today's grace, then process termination.
    Cancel,
    /// Cooperative cancel with today's grace; the child stays resident so the
    /// session can be resumed without reloading it.
    Pause,
    /// Immediate process termination with no grace at all.
    Kill,
}

impl AgentJobInterrupt {
    /// The leading marker token. `cancel` is what every pre-0.3.5 marker
    /// already carries, so an in-flight cancel written by an older build keeps
    /// reading as a cancel.
    pub(super) fn marker_token(self) -> &'static str {
        match self {
            Self::Cancel => "cancel",
            Self::Pause => "pause",
            Self::Kill => "kill",
        }
    }

    fn from_marker_text(text: &str) -> Self {
        match text.split_whitespace().next() {
            Some("pause") => Self::Pause,
            Some("kill") => Self::Kill,
            _ => Self::Cancel,
        }
    }

    /// The durable job status this interruption settles the job at.
    pub(super) fn job_status(self) -> &'static str {
        match self {
            Self::Cancel => "cancelled",
            // A paused job is explicitly not terminal: the session is alive and
            // is waiting for the operator to resume or kill it.
            Self::Pause => "paused",
            Self::Kill => "killed",
        }
    }
}

/// Read the interruption the marker at `path` records, if any.
///
/// `None` means no interruption was requested, which is the only answer that
/// may be inferred from an absent marker.
pub(super) fn read_agent_job_interrupt(path: &Path) -> Option<AgentJobInterrupt> {
    fs::read_to_string(path)
        .ok()
        .map(|text| AgentJobInterrupt::from_marker_text(&text))
}

// Keep the liveness check and termination result as separate branches: the
// cancellation monitor races this path and depends on the original ordering.
/// Cancel a tracked Codex job by id, or the most recent one when `id` is
/// `None`, terminating its process group and recording the cancellation.
pub fn cancel_codex_job(cwd: &Path, id: Option<&str>) -> Result<String, String> {
    interrupt_codex_job(cwd, id, AgentJobInterrupt::Cancel).map(|(message, _)| message)
}

/// Interrupt a tracked Codex job with an explicit reason.
///
/// Returns the operator-facing message and whether the adapter's process was
/// left alive, which is true only for a pause the adapter answered inside the
/// grace.
///
/// Cancel is the historical path and is unchanged. Pause gives the adapter the
/// same grace but leaves a cooperatively stopped child alive, because the
/// resident connection is the whole point of pausing rather than cancelling.
/// Kill skips the grace outright: it is the operator's answer when the
/// cooperative path is what is failing.
#[allow(clippy::collapsible_if)]
pub(super) fn interrupt_codex_job(
    cwd: &Path,
    id: Option<&str>,
    interrupt: AgentJobInterrupt,
) -> Result<(String, bool), String> {
    let id = id.ok_or_else(|| "Usage: /agent cancel <id>".to_string())?;
    let mut job = find_codex_job(cwd, id)?.ok_or_else(|| format!("Unknown agent job `{id}`"))?;
    let label = agent_job_label(&job);
    if matches!(job.status.as_str(), "cancelled" | "finished" | "killed") {
        return Ok((format!("{label} `{id}` is already {}.", job.status), false));
    }
    let verb = interrupt.marker_token();
    // Every process-backed job receives a durable intent marker so its monitor
    // cannot race a confirmed cancellation with a later nonterminal record. The
    // marker also carries *which* interruption this is, because the ACP runner
    // decides from it whether the child survives.
    let cancel_path = Some(acp_job_cancel_path(cwd, &job.id));
    if let Some(path) = &cancel_path {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::write(
            path,
            format!("{verb} requested at {}\n", timestamp_millis()),
        )
        .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }
    // Preserve a nonterminal durable state until process termination is
    // confirmed. Restart recovery must never hide a still-live child behind a
    // terminal `cancelled` record.
    job.updated_at = timestamp_millis();
    append_codex_job_record(cwd, &format!("{verb}_requested"), &job)?;
    let Some(pid) = job.pid else {
        append_agent_job_log_event(
            &job.log_path,
            "system",
            &format!("{verb} requested for {label} `{id}` before process startup"),
        )?;
        return Err(format!(
            "Interruption requested for {label} `{id}`, but process termination is not confirmed yet; retry after startup completes."
        ));
    };
    let mut cooperative = false;
    if job.kind == "acp-session" && interrupt != AgentJobInterrupt::Kill {
        cooperative =
            wait_for_agent_job_text(&job.log_path, "session/cancel", acp_session_cancel_grace());
    }
    // A paused session that stopped cooperatively keeps its child: the ACP
    // runner has already stored it as the resident connection, and terminating
    // it here would turn a pause into a cancel with extra steps.
    let keep_child = interrupt == AgentJobInterrupt::Pause && cooperative;
    if !keep_child {
        if process_is_running(pid) {
            if let Err(error) = terminate_process(pid) {
                job.updated_at = timestamp_millis();
                append_codex_job_record(cwd, &format!("{verb}_failed"), &job)?;
                return Err(error);
            }
        }
        if process_is_running(pid) {
            job.updated_at = timestamp_millis();
            append_codex_job_record(cwd, &format!("{verb}_failed"), &job)?;
            return Err(format!(
                "process {pid} is still running after {verb}; retry the command"
            ));
        }
    }
    job.status = interrupt.job_status().to_string();
    job.updated_at = timestamp_millis();
    append_codex_job_record(cwd, interrupt.job_status(), &job)?;
    if !keep_child {
        write_agent_job_cancel_result(&job, pid)?;
    }
    Ok((
        format!(
            "{} {label} `{id}` (pid {pid}).",
            match interrupt {
                AgentJobInterrupt::Cancel => "Cancelled",
                AgentJobInterrupt::Pause => "Paused",
                AgentJobInterrupt::Kill => "Killed",
            }
        ),
        keep_child,
    ))
}

pub(super) fn write_agent_job_cancel_result(job: &CodexJobRecord, pid: u32) -> Result<(), String> {
    let note = if job.kind == "acp-session" {
        "Viden requested ACP session/cancel when the live session was available, then used process termination as a bounded fallback if the agent did not stop promptly."
    } else {
        "Agent job was stopped by terminating the process."
    };
    if let Some(parent) = job.result_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    if job.kind == "acp-session" {
        // The ACP monitor owns the richer protocol result. Give it a bounded
        // opportunity to publish that evidence and never overwrite it with the
        // process-level fallback summary.
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            if fs::metadata(&job.result_path).is_ok_and(|metadata| metadata.len() > 0) {
                return append_agent_job_log_event(
                    &job.log_path,
                    "system",
                    &format!(
                        "cancelled {} `{}` via pid {}",
                        agent_job_label(job),
                        job.id,
                        pid
                    ),
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fs::write(
        &job.result_path,
        format!(
            "# {} cancelled\n\nstatus: cancelled\npid: {}\ncommand: {}\ntask: {}\nlog: {}\n\n{}\n",
            agent_job_label(job),
            pid,
            job.command,
            job.task,
            job.log_path.display(),
            note
        ),
    )
    .map_err(|err| format!("failed to write {}: {err}", job.result_path.display()))?;
    append_agent_job_log_event(
        &job.log_path,
        "system",
        &format!(
            "cancelled {} `{}` via pid {}",
            agent_job_label(job),
            job.id,
            pid
        ),
    )
}

pub(super) fn agent_job_label(job: &CodexJobRecord) -> &'static str {
    if job.kind == "acp-session" {
        "ACP session job"
    } else {
        "Codex job"
    }
}

pub(super) fn agent_job_session_line(job: &CodexJobRecord, session_id: &str) -> String {
    if job.kind == "acp-session" {
        format!("    session: {session_id}")
    } else {
        format!("    resume: codex resume {session_id}")
    }
}

/// The Codex executable to invoke: `VIDEN_AGENT_CODEX_COMMAND` when set and
/// non-empty, otherwise `codex` from `PATH`.
pub fn codex_command() -> String {
    env::var("VIDEN_AGENT_CODEX_COMMAND")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "codex".to_string())
}

pub(super) fn codex_job_artifact_path(cwd: &Path, id: &str, ext: &str) -> PathBuf {
    cwd.join(".viden")
        .join("agents")
        .join(format!("{id}.{ext}"))
}

pub(super) fn acp_job_cancel_path(cwd: &Path, id: &str) -> PathBuf {
    codex_job_artifact_path(cwd, id, "cancel")
}

pub(super) fn acp_job_runtime_events_path(cwd: &Path, id: &str) -> PathBuf {
    codex_job_artifact_path(cwd, id, "runtime-events.jsonl")
}

pub(super) fn wait_for_agent_job_text(path: &Path, needle: &str, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if fs::read_to_string(path).is_ok_and(|text| text.contains(needle)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

pub(super) fn append_codex_job_record(
    cwd: &Path,
    event: &str,
    record: &CodexJobRecord,
) -> Result<(), String> {
    let path = codex_job_store_path(cwd);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|err| format!("failed to open {}: {err}", path.display()))?;
    writeln!(
        file,
        r#"{{"ts":{},"event":"{}","id":"{}","kind":"{}","status":"{}","pid":{},"command":"{}","task":"{}","log":"{}","result":"{}","baseline":"{}","agent_id":"{}","model":"{}","owner_workspace_id":"{}","owner_project_id":"{}","owner_lane_id":"{}","owner_session_id":"{}","owner_task_id":"{}","owner_turn_id":"{}"}}"#,
        record.updated_at,
        escape_json_fragment(event),
        escape_json_fragment(&record.id),
        escape_json_fragment(&record.kind),
        escape_json_fragment(&record.status),
        record
            .pid
            .map(|pid| pid.to_string())
            .unwrap_or_else(|| "null".to_string()),
        escape_json_fragment(&record.command),
        escape_json_fragment(&record.task),
        escape_json_fragment(&record.log_path.display().to_string()),
        escape_json_fragment(&record.result_path.display().to_string()),
        escape_json_fragment(&record.baseline_path.display().to_string()),
        escape_json_fragment(record.agent.as_ref().map(|agent| agent.agent_id.as_str()).unwrap_or("")),
        escape_json_fragment(record.agent.as_ref().and_then(|agent| agent.model.as_deref()).unwrap_or("")),
        escape_json_fragment(record.agent.as_ref().map(|agent| agent.owner.workspace_id.as_str()).unwrap_or("")),
        escape_json_fragment(record.agent.as_ref().map(|agent| agent.owner.project_id.as_str()).unwrap_or("")),
        escape_json_fragment(record.agent.as_ref().and_then(|agent| agent.owner.lane_id.as_deref()).unwrap_or("")),
        escape_json_fragment(record.agent.as_ref().and_then(|agent| agent.owner.session_id.as_deref()).unwrap_or("")),
        escape_json_fragment(record.agent.as_ref().and_then(|agent| agent.owner.task_id.as_deref()).unwrap_or("")),
        escape_json_fragment(record.agent.as_ref().and_then(|agent| agent.owner.turn_id.as_deref()).unwrap_or(""))
    )
    .map_err(|err| format!("failed to write {}: {err}", path.display()))
}

pub(super) fn latest_codex_jobs(cwd: &Path) -> Result<Vec<CodexJobRecord>, String> {
    let path = codex_job_store_path(cwd);
    let Ok(content) = fs::read_to_string(&path) else {
        return Ok(Vec::new());
    };
    let mut jobs = Vec::<CodexJobRecord>::new();
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let Some(record) = parse_codex_job_record(line) else {
            continue;
        };
        if let Some(existing) = jobs.iter_mut().find(|job| job.id == record.id) {
            *existing = record;
        } else {
            jobs.push(record);
        }
    }
    jobs.sort_by_key(|job| job.updated_at);
    Ok(jobs)
}

pub(super) fn find_codex_job(cwd: &Path, id: &str) -> Result<Option<CodexJobRecord>, String> {
    Ok(latest_codex_jobs(cwd)?.into_iter().find(|job| job.id == id))
}

pub(super) fn parse_codex_job_record(line: &str) -> Option<CodexJobRecord> {
    let id = json_string_field(line, "id")?;
    let log_path = PathBuf::from(json_string_field(line, "log")?);
    let result_path = PathBuf::from(json_string_field(line, "result")?);
    let baseline_path = json_string_field(line, "baseline")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            log_path
                .parent()
                .map(|parent| parent.join(format!("{id}.baseline.status")))
                .unwrap_or_else(|| PathBuf::from(format!("{id}.baseline.status")))
        });
    let optional = |field: &str| json_string_field(line, field).filter(|value| !value.is_empty());
    let agent = optional("agent_id").map(|agent_id| AgentJobMetadata {
        agent_id,
        model: optional("model"),
        owner: RuntimeOwner {
            workspace_id: optional("owner_workspace_id").unwrap_or_default(),
            project_id: optional("owner_project_id").unwrap_or_default(),
            lane_id: optional("owner_lane_id"),
            session_id: optional("owner_session_id"),
            task_id: optional("owner_task_id"),
            turn_id: optional("owner_turn_id"),
        },
    });
    Some(CodexJobRecord {
        id,
        kind: json_string_field(line, "kind")?,
        status: json_string_field(line, "status")?,
        pid: json_number_field(line, "pid").and_then(|value| value.parse().ok()),
        command: json_string_field(line, "command").unwrap_or_default(),
        task: json_string_field(line, "task").unwrap_or_default(),
        log_path,
        result_path,
        baseline_path,
        updated_at: json_number_field(line, "ts")?.parse().ok()?,
        agent,
    })
}

pub(super) fn write_codex_status_baseline(cwd: &Path, path: &Path) -> Result<(), String> {
    let content = match git_status_snapshot(cwd) {
        Ok(lines) => lines.join("\n"),
        Err(error) => format!("# unavailable: {error}"),
    };
    fs::write(path, content).map_err(|err| {
        format!(
            "failed to write Codex job baseline {}: {err}",
            path.display()
        )
    })
}

pub(super) fn codex_job_evidence(cwd: &Path, job: &CodexJobRecord) -> CodexJobEvidence {
    let result = fs::read_to_string(&job.result_path).unwrap_or_default();
    let log = tail_text(&job.log_path, 120).unwrap_or_default();
    codex_job_evidence_from_text(cwd, job, &format!("{result}\n{log}"))
}

pub(super) fn record_codex_app_server_turn_probe(
    cwd: &Path,
    task: &str,
    evidence: &CodexAppServerProbeEvidence,
) -> Result<String, String> {
    let id = evidence
        .turn_id
        .as_ref()
        .map(|turn_id| format!("codex-app-{turn_id}"))
        .unwrap_or_else(|| format!("codex-app-{}", timestamp_millis()));
    let result_path = codex_job_artifact_path(cwd, &id, "result.md");
    let baseline_path = codex_job_artifact_path(cwd, &id, "baseline.status");
    if let Some(parent) = result_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    write_codex_status_baseline(cwd, &baseline_path)?;
    let command = "codex app-server turn/start".to_string();
    let mut record = CodexJobRecord {
        id: id.clone(),
        kind: "app-server-turn".to_string(),
        status: "running".to_string(),
        pid: None,
        command,
        task: task.to_string(),
        log_path: evidence.log_path.clone(),
        result_path: result_path.clone(),
        baseline_path,
        updated_at: timestamp_millis(),
        agent: None,
    };
    append_codex_job_record(cwd, "started", &record)?;

    write_codex_app_server_turn_result(&result_path, evidence)?;
    record.status = codex_app_server_turn_job_status(evidence);
    record.updated_at = timestamp_millis();
    append_codex_job_record(cwd, "completed", &record)?;
    Ok(id)
}

pub(super) fn write_codex_app_server_turn_result(
    result_path: &Path,
    evidence: &CodexAppServerProbeEvidence,
) -> Result<(), String> {
    let signals = codex_app_server_signal_summary(&evidence.notifications);
    fs::write(
        result_path,
        format!(
            "# Codex app-server turn\n\nthread: {}\nturn: {}\nstatus: {}\nlog: {}\nresume: {}\nmessage: {}\napprovals: {}\nsignals: {}\n",
            evidence.thread_id.as_deref().unwrap_or("unknown"),
            evidence.turn_id.as_deref().unwrap_or("unknown"),
            evidence.turn_status.as_deref().unwrap_or("unknown"),
            evidence.log_path.display(),
            evidence.thread_id.as_deref().unwrap_or("unknown"),
            evidence.final_message.as_deref().unwrap_or("none"),
            if evidence.approval_requests.is_empty() {
                "none".to_string()
            } else {
                evidence.approval_requests.join(", ")
            },
            signals
        ),
    )
    .map_err(|err| format!("failed to write {}: {err}", result_path.display()))
}

pub(super) fn codex_app_server_signal_summary(notifications: &[String]) -> String {
    let mut signals = Vec::new();
    for (method, label) in [
        ("item/commandExecution/outputDelta", "command-output"),
        ("item/fileChange/outputDelta", "file-change"),
        ("item/fileChange/patchUpdated", "file-patch"),
        ("turn/diff/updated", "diff-updated"),
        ("fs/changed", "fs-changed"),
        ("item/mcpToolCall", "mcp-tool-call"),
        ("item/mcpToolCall/completed", "mcp-tool-completed"),
        ("item/mcpToolCall/fs-write", "mcp-fs-write"),
        ("error", "app-server-error"),
    ] {
        if notifications.iter().any(|item| item == method) {
            signals.push(label);
        }
    }
    if signals.is_empty() {
        "none".to_string()
    } else {
        signals.join(", ")
    }
}

pub(super) fn codex_app_server_turn_job_status(evidence: &CodexAppServerProbeEvidence) -> String {
    match evidence.turn_status.as_deref() {
        Some("completed") => "finished".to_string(),
        Some("failed" | "interrupted") => "failed".to_string(),
        _ => "observed".to_string(),
    }
}

pub(super) fn codex_job_evidence_from_text(
    cwd: &Path,
    job: &CodexJobRecord,
    text: &str,
) -> CodexJobEvidence {
    let mut files = changed_files_since_codex_start(cwd, job);
    files.extend(extract_file_mentions(text));
    files.sort();
    files.dedup();
    CodexJobEvidence {
        session_id: extract_codex_session_id(text),
        files,
    }
}

pub(super) fn changed_files_since_codex_start(cwd: &Path, job: &CodexJobRecord) -> Vec<String> {
    let Ok(current) = git_status_snapshot(cwd) else {
        return Vec::new();
    };
    let baseline = fs::read_to_string(&job.baseline_path)
        .ok()
        .filter(|content| !content.starts_with("# unavailable:"))
        .unwrap_or_default();
    let before = baseline.lines().map(str::to_string).collect::<HashSet<_>>();
    let mut changed = Vec::new();
    for line in current {
        if before.contains(&line) {
            continue;
        }
        if let Some(path) = git_status_path(&line) {
            changed.push(path);
        }
    }
    changed.sort();
    changed.dedup();
    changed
}

pub(super) fn git_status_snapshot(cwd: &Path) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .current_dir(cwd)
        .output()
        .map_err(|err| format!("failed to run git status: {err}"))?;
    if !output.status.success() {
        return Err(first_output_line(&join_output(
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        ))
        .unwrap_or_else(|| format!("git status exited with {}", output.status)));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(git_status_line)
        .collect())
}

pub(super) fn git_status_line(line: &str) -> Option<String> {
    let path = git_status_path(line)?;
    if path.starts_with(".viden/") {
        return None;
    }
    Some(line.to_string())
}

pub(super) fn git_status_path(line: &str) -> Option<String> {
    let path = line.get(3..)?.trim();
    let path = path.rsplit(" -> ").next().unwrap_or(path).trim();
    let path = path.trim_matches('"');
    if path.is_empty() || path.starts_with(".viden/") {
        None
    } else {
        Some(path.to_string())
    }
}

pub(super) fn extract_codex_session_id(text: &str) -> Option<String> {
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower
            .find("codex resume ")
            .and_then(|index| line.get(index + "codex resume ".len()..))
            && let Some(id) = first_identifier_token(rest)
        {
            return Some(id);
        }
        for marker in [
            "session id:",
            "session:",
            "codex session:",
            "\"session_id\":",
            "\"thread_id\":",
        ] {
            if let Some(index) = lower.find(marker) {
                let rest = &line[index + marker.len()..];
                if let Some(id) = first_identifier_token(rest) {
                    return Some(id);
                }
            }
        }
    }
    None
}

pub(super) fn first_identifier_token(value: &str) -> Option<String> {
    value
        .trim_start_matches(|ch: char| ch.is_whitespace() || ch == '"' || ch == '\'' || ch == '=')
        .split(|ch: char| ch.is_whitespace() || matches!(ch, '"' | '\'' | ',' | ')' | ']' | '}'))
        .find(|token| !token.trim().is_empty())
        .map(|token| {
            token
                .trim_matches(|ch: char| matches!(ch, '"' | '\'' | '`' | '.' | ',' | ';' | ':'))
                .to_string()
        })
        .filter(|token| !token.is_empty())
}

pub(super) fn extract_file_mentions(text: &str) -> Vec<String> {
    let mut files = Vec::new();
    for raw in text.split_whitespace() {
        let token = raw.trim_matches(|ch: char| {
            matches!(
                ch,
                '"' | '\'' | '`' | ',' | ';' | ':' | '(' | ')' | '[' | ']' | '{' | '}'
            )
        });
        if looks_like_repo_file(token) {
            files.push(token.to_string());
        }
    }
    files.sort();
    files.dedup();
    files
}

pub(super) fn looks_like_repo_file(token: &str) -> bool {
    if token.is_empty()
        || token.starts_with("http://")
        || token.starts_with("https://")
        || token.starts_with(".viden/")
        || token.contains("://")
    {
        return false;
    }
    let has_path_shape = token.contains('/') || token.contains('.');
    let has_known_extension = [
        ".rs", ".toml", ".md", ".json", ".yaml", ".yml", ".js", ".ts", ".tsx", ".jsx", ".py",
        ".go", ".java", ".c", ".cc", ".cpp", ".h", ".hpp", ".html", ".css", ".sh",
    ]
    .iter()
    .any(|extension| token.ends_with(extension));
    has_path_shape
        && has_known_extension
        && token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '_' | '-' | '.' | '+'))
}

pub(super) fn observed_codex_status(job: &CodexJobRecord) -> String {
    // `paused` is deliberately here: a paused ACP session keeps a live child,
    // so observing the process would report it as still running and erase the
    // operator's pause.
    if matches!(
        job.status.as_str(),
        "cancelled" | "failed" | "finished" | "killed" | "paused"
    ) {
        return job.status.clone();
    }
    match job.pid {
        Some(pid) if process_is_running(pid) => "running".to_string(),
        Some(_) => "finished".to_string(),
        None => job.status.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CodexReadyReport {
    pub(super) version: String,
    pub(super) app_server: String,
    pub(super) auth: String,
    pub(super) config_sources: String,
    pub(super) job_store: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CodexProtocolProbeReport {
    pub(super) available: Vec<String>,
    pub(super) missing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CodexDiagnosticReport {
    Ready(CodexReadyReport),
    Unavailable(String),
}

pub(super) fn codex_diagnostics(cwd: &Path, command: &str) -> CodexDiagnosticReport {
    let version = match command_output(command, &["--version"], cwd, Duration::from_secs(4)) {
        Ok(output) => first_output_line(&output).unwrap_or_else(|| "unknown".to_string()),
        Err(error) => return CodexDiagnosticReport::Unavailable(error),
    };

    let app_server = match command_output(
        command,
        &["app-server", "--help"],
        cwd,
        Duration::from_secs(4),
    ) {
        Ok(output) if output.contains("app-server") => "ok (codex app-server)".to_string(),
        Ok(output) => format!(
            "unexpected help output ({})",
            first_output_line(&output).unwrap_or_else(|| "empty".to_string())
        ),
        Err(error) => format!("unavailable ({error})"),
    };

    let auth = match command_output(command, &["login", "status"], cwd, Duration::from_secs(5)) {
        Ok(output) => first_output_line(&output).unwrap_or_else(|| "unknown".to_string()),
        Err(error) => format!("setup needed ({error})"),
    };

    CodexDiagnosticReport::Ready(CodexReadyReport {
        version,
        app_server,
        auth,
        config_sources: codex_config_sources(cwd),
        job_store: codex_job_store_path(cwd),
    })
}

pub(super) fn codex_protocol_probe(
    cwd: &Path,
    command: &str,
) -> Result<CodexProtocolProbeReport, String> {
    let schema_dir = codex_protocol_schema_dir(cwd);
    if schema_dir.exists() {
        fs::remove_dir_all(&schema_dir).map_err(|err| {
            format!(
                "failed to remove stale schema dir {}: {err}",
                schema_dir.display()
            )
        })?;
    }
    fs::create_dir_all(&schema_dir).map_err(|err| {
        format!(
            "failed to create schema dir {}: {err}",
            schema_dir.display()
        )
    })?;
    let out = schema_dir.display().to_string();
    let probe = command_output(
        command,
        &[
            "app-server",
            "generate-json-schema",
            "--experimental",
            "--out",
            &out,
        ],
        cwd,
        Duration::from_secs(5),
    );
    let result = match probe {
        Ok(_) => codex_protocol_probe_from_dir(&schema_dir),
        Err(error) => Err(error),
    };
    let _ = fs::remove_dir_all(&schema_dir);
    result
}

pub(super) fn codex_protocol_schema_dir(cwd: &Path) -> PathBuf {
    cwd.join(".viden")
        .join("tmp")
        .join(format!("codex-schema-{}", timestamp_millis()))
}

pub(super) fn codex_protocol_probe_from_dir(
    dir: &Path,
) -> Result<CodexProtocolProbeReport, String> {
    let client = read_schema_file(dir, "ClientRequest.json")?;
    let server_notifications = read_schema_file(dir, "ServerNotification.json")?;
    let server_requests = read_schema_file(dir, "ServerRequest.json")?;
    let checks: [(&str, &str, &[&str]); 6] = [
        (
            "thread lifecycle",
            &client,
            &["thread/start", "thread/resume", "thread/read"][..],
        ),
        ("review", &client, &["review/start"][..]),
        (
            "turn control",
            &client,
            &["turn/start", "turn/interrupt"][..],
        ),
        (
            "events",
            &server_notifications,
            &["thread/started", "turn/started", "turn/completed"][..],
        ),
        (
            "evidence",
            &server_notifications,
            &[
                "item/commandExecution/outputDelta",
                "item/fileChange/outputDelta",
                "turn/diff/updated",
            ][..],
        ),
        (
            "approvals",
            &server_requests,
            &[
                "item/commandExecution/requestApproval",
                "item/fileChange/requestApproval",
                "item/permissions/requestApproval",
            ][..],
        ),
    ];
    let mut available = Vec::new();
    let mut missing = Vec::new();
    for (label, haystack, needles) in checks {
        if needles.iter().all(|needle| haystack.contains(needle)) {
            available.push(label.to_string());
        } else {
            missing.push(label.to_string());
        }
    }
    Ok(CodexProtocolProbeReport { available, missing })
}

pub(super) fn read_schema_file(dir: &Path, name: &str) -> Result<String, String> {
    fs::read_to_string(dir.join(name))
        .map_err(|err| format!("failed to read generated {name}: {err}"))
}

pub(super) fn codex_config_sources(cwd: &Path) -> String {
    let mut sources = Vec::new();
    let project_config = cwd.join(".codex").join("config.toml");
    if project_config.exists() {
        sources.push(format!("project {}", project_config.display()));
    }
    if let Some(home) = env::var_os("HOME") {
        let user_config = PathBuf::from(home).join(".codex").join("config.toml");
        if user_config.exists() {
            sources.push(format!("user {}", user_config.display()));
        }
    }
    if sources.is_empty() {
        "none found (Codex defaults apply)".to_string()
    } else {
        sources.join("; ")
    }
}

pub(super) fn codex_job_store_path(cwd: &Path) -> PathBuf {
    cwd.join(".viden").join("agents").join("codex-jobs.jsonl")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CodexAppServerProbeEvidence {
    pub(super) user_agent: String,
    pub(super) codex_home: String,
    pub(super) platform: String,
    pub(super) thread_id: Option<String>,
    pub(super) turn_id: Option<String>,
    pub(super) turn_status: Option<String>,
    pub(super) final_message: Option<String>,
    pub(super) notifications: Vec<String>,
    pub(super) approval_requests: Vec<String>,
    pub(super) log_path: PathBuf,
}

pub(super) fn run_codex_app_server_probe(
    cwd: &Path,
    command: &str,
    mode: CodexProbeMode,
) -> Result<CodexAppServerProbeEvidence, String> {
    run_codex_app_server_probe_with_log(cwd, command, mode, codex_app_server_probe_log_path(cwd))
}

pub(super) fn run_codex_app_server_probe_with_log(
    cwd: &Path,
    command: &str,
    mode: CodexProbeMode,
    log_path: PathBuf,
) -> Result<CodexAppServerProbeEvidence, String> {
    let request = codex_app_server_initialize_request();
    let mut log_entries = vec![jsonl_event("client", &request)];
    let mut child = spawn_codex_app_server(cwd, command)?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "failed to open Codex app-server stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to open Codex app-server stdout".to_string())?;
    stdin
        .write_all(request.as_bytes())
        .and_then(|_| stdin.write_all(b"\n"))
        .and_then(|_| stdin.flush())
        .map_err(|err| format!("failed to write Codex app-server initialize: {err}"))?;

    // Codex app-server stdio is newline-delimited JSON: responses and
    // notifications can arrive in either order, so keep every line as evidence.
    let receiver = read_lines_async(stdout);
    let mut notifications = Vec::new();
    let mut approval_requests = Vec::new();
    let response = match read_codex_app_server_response(
        &receiver,
        &mut stdin,
        1,
        &mut log_entries,
        &mut notifications,
        &mut approval_requests,
        Duration::from_secs(5),
    ) {
        Ok(response) => response,
        Err(error) => return Err(finish_failed_probe(child, log_path, log_entries, error)),
    };

    let start_thread = !matches!(mode, CodexProbeMode::Initialize);
    let write_turn = matches!(mode, CodexProbeMode::Turn { write: true, .. });
    let thread_id = if start_thread {
        let request = codex_app_server_thread_start_request(cwd, write_turn);
        log_entries.push(jsonl_event("client", &request));
        if let Err(error) = stdin
            .write_all(request.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
        {
            return Err(finish_failed_probe(
                child,
                log_path,
                log_entries,
                format!("failed to write Codex app-server thread/start: {error}"),
            ));
        }
        let thread_response = match read_codex_app_server_response(
            &receiver,
            &mut stdin,
            2,
            &mut log_entries,
            &mut notifications,
            &mut approval_requests,
            Duration::from_secs(8),
        ) {
            Ok(response) => response,
            Err(error) => return Err(finish_failed_probe(child, log_path, log_entries, error)),
        };
        json_object_string_field(&thread_response, "thread", "id")
    } else {
        None
    };

    let mut turn_id = None;
    let mut turn_status = None;
    if let CodexProbeMode::Turn { task, write } = &mode {
        let Some(thread_id) = thread_id.as_deref() else {
            return Err(finish_failed_probe(
                child,
                log_path.clone(),
                log_entries.clone(),
                "Codex app-server thread/start did not return a thread id".to_string(),
            ));
        };
        let request = codex_app_server_turn_start_request(cwd, thread_id, task, *write);
        log_entries.push(jsonl_event("client", &request));
        if let Err(error) = stdin
            .write_all(request.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
        {
            return Err(finish_failed_probe(
                child,
                log_path,
                log_entries,
                format!("failed to write Codex app-server turn/start: {error}"),
            ));
        }
        let turn_response = match read_codex_app_server_response(
            &receiver,
            &mut stdin,
            3,
            &mut log_entries,
            &mut notifications,
            &mut approval_requests,
            Duration::from_secs(8),
        ) {
            Ok(response) => response,
            Err(error) => return Err(finish_failed_probe(child, log_path, log_entries, error)),
        };
        turn_id = json_object_string_field(&turn_response, "turn", "id");
        turn_status = json_string_field(&turn_response, "status");
        if let Some(completed) = collect_codex_app_server_notifications(
            &receiver,
            &mut stdin,
            &mut log_entries,
            &mut notifications,
            &mut approval_requests,
            Some("turn/completed"),
            Duration::from_secs(30),
        ) {
            turn_status = json_string_field(&completed, "status").or(turn_status);
        }
    }

    while let Ok(Ok(line)) = receiver.recv_timeout(Duration::from_millis(150)) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        log_entries.push(jsonl_event("server", line));
        if let Some(method) = json_string_field(line, "method") {
            if is_codex_app_server_request(&method) {
                if let Some(response) =
                    codex_app_server_request_denial_response(line, &method, &mut approval_requests)
                {
                    log_entries.push(jsonl_event("client", &response));
                    let _ = stdin
                        .write_all(response.as_bytes())
                        .and_then(|_| stdin.write_all(b"\n"))
                        .and_then(|_| stdin.flush());
                }
            } else {
                record_codex_app_server_notification(line, &method, &mut notifications);
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    let final_message = codex_app_server_final_message(&log_entries);
    write_probe_log(&log_path, &log_entries)?;

    Ok(CodexAppServerProbeEvidence {
        user_agent: json_string_field(&response, "userAgent")
            .unwrap_or_else(|| "unknown".to_string()),
        codex_home: json_string_field(&response, "codexHome")
            .unwrap_or_else(|| "unknown".to_string()),
        platform: json_string_field(&response, "platformOs")
            .unwrap_or_else(|| "unknown".to_string()),
        thread_id,
        turn_id,
        turn_status,
        final_message,
        notifications,
        approval_requests,
        log_path,
    })
}

pub(super) fn spawn_codex_app_server(cwd: &Path, command: &str) -> Result<Child, String> {
    Command::new(command)
        .args(["app-server", "--listen", "stdio://"])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| format!("failed to launch Codex app-server: {err}"))
}

pub(super) fn read_codex_app_server_response(
    receiver: &mpsc::Receiver<std::io::Result<String>>,
    stdin: &mut impl Write,
    request_id: u32,
    log_entries: &mut Vec<String>,
    notifications: &mut Vec<String>,
    approval_requests: &mut Vec<String>,
    timeout: Duration,
) -> Result<String, String> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        let remaining = timeout
            .checked_sub(start.elapsed())
            .unwrap_or_else(|| Duration::from_millis(1));
        let line = match receiver.recv_timeout(remaining) {
            Ok(Ok(line)) if !line.trim().is_empty() => line.trim().to_string(),
            Ok(Ok(_)) => continue,
            Ok(Err(error)) => {
                return Err(format!("failed to read Codex app-server response: {error}"));
            }
            Err(_) => break,
        };
        log_entries.push(jsonl_event("server", &line));
        if line.contains(&format!(r#""id":{request_id}"#)) {
            if line.contains(r#""result""#) {
                return Ok(line);
            }
            return Err(format!(
                "Codex app-server request {request_id} failed: {line}"
            ));
        }
        if let Some(method) = json_string_field(&line, "method") {
            if is_codex_app_server_request(&method) {
                if let Some(response) =
                    codex_app_server_request_denial_response(&line, &method, approval_requests)
                {
                    log_entries.push(jsonl_event("client", &response));
                    stdin
                        .write_all(response.as_bytes())
                        .and_then(|_| stdin.write_all(b"\n"))
                        .and_then(|_| stdin.flush())
                        .map_err(|err| {
                            format!("failed to answer Codex app-server request: {err}")
                        })?;
                }
            } else {
                record_codex_app_server_notification(&line, &method, notifications);
            }
        }
    }
    Err(format!(
        "Codex app-server request {request_id} response timed out"
    ))
}

pub(super) fn collect_codex_app_server_notifications(
    receiver: &mpsc::Receiver<std::io::Result<String>>,
    stdin: &mut impl Write,
    log_entries: &mut Vec<String>,
    notifications: &mut Vec<String>,
    approval_requests: &mut Vec<String>,
    until_method: Option<&str>,
    timeout: Duration,
) -> Option<String> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        let remaining = timeout
            .checked_sub(start.elapsed())
            .unwrap_or_else(|| Duration::from_millis(1));
        let line = match receiver.recv_timeout(remaining) {
            Ok(Ok(line)) if !line.trim().is_empty() => line.trim().to_string(),
            Ok(Ok(_)) => continue,
            Ok(Err(_)) | Err(_) => break,
        };
        log_entries.push(jsonl_event("server", &line));
        if let Some(method) = json_string_field(&line, "method") {
            if is_codex_app_server_request(&method) {
                if let Some(response) =
                    codex_app_server_request_denial_response(&line, &method, approval_requests)
                {
                    log_entries.push(jsonl_event("client", &response));
                    let _ = stdin
                        .write_all(response.as_bytes())
                        .and_then(|_| stdin.write_all(b"\n"))
                        .and_then(|_| stdin.flush());
                }
            } else {
                record_codex_app_server_notification(&line, &method, notifications);
            }
            if until_method.is_some_and(|target| target == method) {
                return Some(line);
            }
        }
    }
    None
}

pub(super) fn codex_app_server_final_message(log_entries: &[String]) -> Option<String> {
    log_entries.iter().rev().find_map(|entry| {
        if !entry.contains("agentMessage") || !entry.contains("text") {
            return None;
        }
        let normalized = entry.replace("\\\"", "\"");
        json_string_field(&normalized, "text")
            .map(|message| message.chars().take(500).collect::<String>())
            .filter(|message| !message.trim().is_empty())
    })
}

pub(super) fn record_codex_app_server_notification(
    line: &str,
    method: &str,
    notifications: &mut Vec<String>,
) {
    notifications.push(method.to_string());
    // Some current Codex app-server write-capable turns perform work through
    // MCP tool items instead of emitting fileChange/fs notifications. Preserve
    // those as explicit signals so operator evidence does not collapse to
    // `signals: none` after a real workspace mutation.
    if line.contains(r#""type":"mcpToolCall""#) {
        notifications.push("item/mcpToolCall".to_string());
        if line.contains(r#""status":"completed""#) {
            notifications.push("item/mcpToolCall/completed".to_string());
        }
        if line.contains("writeFile") || line.contains("node:fs") {
            notifications.push("item/mcpToolCall/fs-write".to_string());
        }
    }
}

pub(super) fn is_codex_app_server_request(method: &str) -> bool {
    matches!(
        method,
        "item/commandExecution/requestApproval"
            | "item/fileChange/requestApproval"
            | "item/permissions/requestApproval"
            | "execCommandApproval"
            | "applyPatchApproval"
    )
}

pub(super) fn codex_app_server_request_denial_response(
    line: &str,
    method: &str,
    approval_requests: &mut Vec<String>,
) -> Option<String> {
    let id = json_number_field(line, "id")?;
    approval_requests.push(method.to_string());
    let result = match method {
        "item/commandExecution/requestApproval" => r#"{"decision":"decline"}"#,
        "item/fileChange/requestApproval" => r#"{"decision":"decline"}"#,
        "item/permissions/requestApproval" => {
            r#"{"permissions":{},"scope":"turn","strictAutoReview":true}"#
        }
        "execCommandApproval" | "applyPatchApproval" => r#"{"decision":"denied"}"#,
        _ => return None,
    };
    Some(format!(r#"{{"id":{id},"result":{result}}}"#))
}

pub(super) fn codex_app_server_probe_log_path(cwd: &Path) -> PathBuf {
    cwd.join(".viden")
        .join("agents")
        .join(format!("codex-app-server-{}.jsonl", timestamp_millis()))
}

pub(super) fn codex_app_server_initialize_request() -> String {
    [
        r#"{"id":1,"method":"initialize","params":{"clientInfo":{"name":"viden","version":""#,
        env!("CARGO_PKG_VERSION"),
        r#""},"capabilities":{"experimentalApi":true,"requestAttestation":false,"optOutNotificationMethods":[]}}}"#,
    ]
    .join("")
}

pub(super) fn codex_app_server_thread_start_request(cwd: &Path, write: bool) -> String {
    let cwd = escape_json_fragment(&cwd.display().to_string());
    let approval_policy = if write { "on-request" } else { "never" };
    let sandbox = if write {
        "workspace-write"
    } else {
        "read-only"
    };
    format!(
        r#"{{"id":2,"method":"thread/start","params":{{"model":null,"modelProvider":null,"cwd":"{cwd}","runtimeWorkspaceRoots":["{cwd}"],"approvalPolicy":"{approval_policy}","approvalsReviewer":"user","sandbox":"{sandbox}","permissions":null,"config":null,"serviceName":"viden","baseInstructions":null,"developerInstructions":null,"personality":null,"ephemeral":true,"sessionStartSource":"startup","threadSource":"subagent","environments":[],"dynamicTools":null,"experimentalRawEvents":false,"persistExtendedHistory":false}}}}"#
    )
}

pub(super) fn codex_app_server_turn_start_request(
    cwd: &Path,
    thread_id: &str,
    task: &str,
    write: bool,
) -> String {
    let cwd = escape_json_fragment(&cwd.display().to_string());
    let thread_id = escape_json_fragment(thread_id);
    let task = escape_json_fragment(task);
    let approval_policy = if write { "on-request" } else { "never" };
    let sandbox_policy = if write {
        format!(r#"{{"type":"workspaceWrite","writableRoots":["{cwd}"],"networkAccess":false}}"#)
    } else {
        r#"{"type":"readOnly","networkAccess":false}"#.to_string()
    };
    format!(
        r#"{{"id":3,"method":"turn/start","params":{{"threadId":"{thread_id}","input":[{{"type":"text","text":"{task}","text_elements":[]}}],"responsesapiClientMetadata":null,"environments":[],"cwd":"{cwd}","runtimeWorkspaceRoots":["{cwd}"],"approvalPolicy":"{approval_policy}","approvalsReviewer":"user","sandboxPolicy":{sandbox_policy},"permissions":null,"model":null,"serviceTier":null,"effort":null,"summary":null,"personality":null,"outputSchema":null,"collaborationMode":null}}}}"#
    )
}
