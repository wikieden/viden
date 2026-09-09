use viden_types::{
    AgentNextAction, AgentRole, AgentRoute, AgentTaskKind, AgentTaskRecord, AgentTaskStatus,
    EvidenceView, now_timestamp,
};

use crate::{SessionEngine, TestEvidence};

impl SessionEngine {
    pub fn agent_task_snapshot(&self) -> Vec<AgentTaskRecord> {
        let mut tasks = self.runtime_tasks.clone();
        tasks.sort_by(|left, right| {
            right
                .is_active()
                .cmp(&left.is_active())
                .then(right.priority().cmp(&left.priority()))
                .then(right.updated_at.cmp(&left.updated_at))
                .then(left.id.cmp(&right.id))
        });
        tasks
    }

    pub(crate) fn upsert_agent_task(&mut self, task: AgentTaskRecord) {
        let task = sanitize_agent_task_record(task);
        if let Some(existing) = self
            .runtime_tasks
            .iter_mut()
            .find(|item| item.id == task.id)
        {
            *existing = task;
        } else {
            self.runtime_tasks.push(task);
        }
        self.runtime_tasks.retain(|task| {
            task.is_active()
                || task
                    .updated_at
                    .map(|updated| updated >= now_millis().saturating_sub(15 * 60 * 1000))
                    .unwrap_or(true)
        });
    }

    pub(crate) fn upsert_runtime_evidence(&mut self, evidence: EvidenceView) {
        if let Some(existing) = self
            .runtime_evidence
            .iter_mut()
            .find(|item| item.id == evidence.id)
        {
            *existing = evidence;
        } else {
            self.runtime_evidence.push(evidence);
        }
    }

    /// The durable evidence archive this session rebuilt at open.
    ///
    /// Named rather than reached through the field so the read path documents
    /// what it is reading: `runtime_evidence` is replayed from the append-only
    /// workflow agent log by `hydrate_workflow_agent_projection`, so it is the
    /// archive rather than a window over the live stream. It is deliberately
    /// *not* `RuntimeViewState::latest_evidence`, which is a client-side
    /// reduction of whatever events that client received.
    pub(crate) fn evidence_archive(&self) -> &[EvidenceView] {
        &self.runtime_evidence
    }

    /// Root of the canonical ContextStore the evidence content read verifies
    /// bytes against.
    pub(crate) fn context_engine_root(&self) -> &std::path::Path {
        &self.context_engine_root
    }

    pub(crate) fn provider_task(
        &self,
        input: &str,
        status: AgentTaskStatus,
        activity: impl Into<String>,
        progress: u8,
    ) -> AgentTaskRecord {
        let now = now_millis();
        AgentTaskRecord {
            id: format!("turn-{}-{now}", compact_session_id(self.session_id())),
            parent_id: None,
            role: AgentRole::Coder,
            kind: AgentTaskKind::Provider,
            route: AgentRoute::BuiltIn,
            title: first_line(input),
            status,
            activity: activity.into(),
            summary: format!(
                "{} / {} is processing",
                self.provider_name(),
                self.model_name()
            ),
            progress,
            started_at: Some(now),
            updated_at: Some(now),
            workspace: Some(self.cwd.to_string_lossy().to_string()),
            evidence: vec![
                "live provider request".to_string(),
                format!("provider {}", self.provider_name()),
                format!("model {}", self.model_name()),
            ],
            permissions: Vec::new(),
            decision: None,
            result: None,
            resume_handle: None,
            pid: None,
            next_action: Some(AgentNextAction {
                label: "watch provider".to_string(),
                command: Some("/status".to_string()),
                reason: Some("provider turn is active".to_string()),
            }),
            // A built-in provider turn is bound to no Lane and no Agent
            // session, and Core mints no workspace/project identity of its own
            // (GUI-CORE-023), so there is no owner to attach here.
            owner: None,
        }
    }

    pub(crate) fn tool_task(
        &self,
        call_id: &str,
        tool_name: &str,
        encoded_input: &str,
        status: AgentTaskStatus,
        activity: impl Into<String>,
        progress: u8,
    ) -> AgentTaskRecord {
        let now = now_millis();
        AgentTaskRecord {
            id: format!("tool-{call_id}"),
            parent_id: None,
            role: AgentRole::Coder,
            kind: if tool_name == "shell" {
                AgentTaskKind::Shell
            } else {
                AgentTaskKind::Tool
            },
            route: AgentRoute::Terminal,
            title: format!("{tool_name} {encoded_input}"),
            status,
            activity: activity.into(),
            summary: format!("{tool_name} {encoded_input}"),
            progress,
            started_at: Some(now),
            updated_at: Some(now),
            workspace: Some(self.cwd.to_string_lossy().to_string()),
            evidence: vec![format!("tool_call_id {call_id}")],
            permissions: Vec::new(),
            decision: None,
            result: None,
            resume_handle: None,
            pid: None,
            next_action: Some(AgentNextAction {
                label: "inspect transcript".to_string(),
                command: Some("/status".to_string()),
                reason: Some("tool call is part of current turn".to_string()),
            }),
            // Same as `provider_task`: a built-in tool call belongs to the
            // engine turn, which carries no owner identity.
            owner: None,
        }
    }

    pub(crate) fn test_task(
        &self,
        command: &str,
        status: AgentTaskStatus,
        activity: impl Into<String>,
        progress: u8,
        evidence: Option<&TestEvidence>,
    ) -> AgentTaskRecord {
        let now = now_millis();
        let mut rows = vec![format!("command {command}")];
        let mut result = None;
        if let Some(evidence) = evidence {
            rows.push(format!("exit_code {:?}", evidence.exit_code));
            rows.push(format!("duration {}ms", evidence.duration_ms));
            if !evidence.output_tail.trim().is_empty() {
                rows.push(format!(
                    "tail {}",
                    first_line(&evidence.output_tail)
                        .chars()
                        .take(80)
                        .collect::<String>()
                ));
            }
            result = Some(format!(
                "{} exit={}",
                evidence.status,
                evidence
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "<unknown>".to_string())
            ));
        }
        AgentTaskRecord {
            id: format!("test-{}", stable_task_suffix(command)),
            parent_id: None,
            role: AgentRole::Tester,
            kind: AgentTaskKind::Test,
            route: AgentRoute::Terminal,
            title: command.to_string(),
            status,
            activity: activity.into(),
            summary: command.to_string(),
            progress,
            started_at: Some(now),
            updated_at: Some(now),
            workspace: Some(self.cwd.to_string_lossy().to_string()),
            evidence: rows,
            permissions: vec!["shell approval".to_string()],
            decision: None,
            result,
            resume_handle: None,
            pid: None,
            next_action: Some(AgentNextAction {
                label: "rerun test".to_string(),
                command: Some(format!("/test {command}")),
                reason: Some("latest test command can be rerun".to_string()),
            }),
            // Same as `provider_task`: an engine test task has no Lane or
            // Agent-session binding to name.
            owner: None,
        }
    }
}

fn now_millis() -> u64 {
    now_timestamp().saturating_mul(1000)
}

fn first_line(input: &str) -> String {
    input
        .lines()
        .next()
        .unwrap_or(input)
        .chars()
        .take(120)
        .collect()
}

fn sanitize_agent_task_record(mut task: AgentTaskRecord) -> AgentTaskRecord {
    task.title = redact_task_text(&task.title, 160);
    task.activity = redact_task_text(&task.activity, 240);
    task.summary = redact_task_text(&task.summary, 500);
    task.workspace = task.workspace.as_deref().and_then(safe_task_path);
    task.decision = task
        .decision
        .as_deref()
        .map(|decision| redact_task_text(decision, 240))
        .filter(|decision| !decision.is_empty());
    task.result = task
        .result
        .as_deref()
        .map(|result| redact_task_text(result, 500))
        .filter(|result| !result.is_empty());
    if let Some(next_action) = &mut task.next_action {
        next_action.label = redact_task_text(&next_action.label, 80);
        next_action.command = next_action
            .command
            .as_deref()
            .map(|command| redact_task_text(command, 120))
            .filter(|command| !command.is_empty());
        next_action.reason = next_action
            .reason
            .as_deref()
            .map(|reason| redact_task_text(reason, 160))
            .filter(|reason| !reason.is_empty());
    }
    task
}

fn redact_task_text(input: &str, max_chars: usize) -> String {
    let lower = input.to_ascii_lowercase();
    if lower.contains("diff --git") {
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
                || lower.contains("apikey")
                || word.starts_with('/')
                || word.contains("..")
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
    viden_types::truncate_for_preview(&sanitized, max_chars)
}

fn safe_task_path(path: &str) -> Option<String> {
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

fn compact_session_id(session_id: &str) -> String {
    session_id.chars().take(8).collect()
}

fn stable_task_suffix(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
