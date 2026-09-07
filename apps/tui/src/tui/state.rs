use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use viden_core::{
    AgentTaskRecord, CostLedgerTotals, PermissionLevel, ProviderHealthView, RuntimeSnapshot,
    RuntimeViewState, WorkMode,
};
use viden_types::{AgentNextAction, CapabilityId};

pub(super) use super::pending::SupervisionMachine;
pub(super) use super::ui_state::{
    AcpPickerPhase, FocusedConversation, InteractionPanel, Lens, OverlayState, PendingAcpStart,
    PendingNativeLane, ProviderAuthMode, ProviderOption, SupervisionInput, SupervisionPanel,
    TuiEntry, TuiUiState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TuiState {
    pub(super) runtime: RuntimeViewState,
    pub(super) ui: TuiUiState,
    /// Read-only compatibility facts negotiated by the Core client. They gate
    /// presentation actions but never reduce business state locally.
    pub(super) capabilities: BTreeSet<CapabilityId>,
    /// Confirm-on-fact correlation for one in-flight supervision command. This
    /// holds no authoritative record: it only remembers which command id the
    /// Core client issued and which published Core fact would settle it.
    pub(super) supervision: SupervisionMachine,
}

impl TuiState {
    pub(super) fn new(runtime: RuntimeViewState) -> Self {
        Self {
            runtime,
            ui: TuiUiState::default(),
            capabilities: BTreeSet::new(),
            supervision: SupervisionMachine::default(),
        }
    }

    pub(super) fn has_capability(&self, capability: &str) -> bool {
        self.capabilities
            .contains(&CapabilityId(capability.to_string()))
    }
}

impl Default for TuiState {
    fn default() -> Self {
        Self::new(RuntimeViewState::new(RuntimeSnapshot {
            cwd: PathBuf::from("."),
            provider_family: String::new(),
            model_label: String::new(),
            work_mode: WorkMode::Build,
            permission_mode: Default::default(),
            permission_level: PermissionLevel::Ask,
            config_summary: String::new(),
            loaded_config_files: Vec::new(),
            startup_overrides: Vec::new(),
            ui_preferences: Default::default(),
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AgentTask {
    pub(super) id: String,
    pub(super) parent_id: Option<String>,
    pub(super) agent: String,
    pub(super) kind: String,
    pub(super) transport: String,
    pub(super) title: String,
    pub(super) status: String,
    pub(super) progress: u8,
    pub(super) activity: String,
    pub(super) summary: String,
    pub(super) evidence: Vec<String>,
    pub(super) next_action: Option<AgentNextAction>,
    pub(super) started_at: Option<u64>,
    pub(super) updated_at: Option<u64>,
    pub(super) workspace: Option<String>,
    pub(super) permissions: Vec<String>,
    pub(super) decision: Option<String>,
    pub(super) result: Option<String>,
    pub(super) resume_handle: Option<String>,
    pub(super) pid: Option<u32>,
}

impl AgentTask {
    pub(super) fn is_active(&self) -> bool {
        matches!(
            self.status.as_str(),
            "queued"
                | "starting"
                | "running"
                | "waiting_approval"
                | "needs_input"
                | "blocked"
                | "attached"
                | "detached"
        )
    }

    pub(super) fn priority(&self) -> u8 {
        match self.status.as_str() {
            "waiting_approval" | "needs_input" => 5,
            "blocked" | "failed" => 4,
            "starting" | "running" | "attached" => 3,
            "queued" => 2,
            _ => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AgentLane {
    pub(super) id: String,
    pub(super) task_id: Option<String>,
    pub(super) agent: String,
    pub(super) screen: String,
    pub(super) transport: String,
    pub(super) status: String,
    pub(super) summary: String,
    pub(super) evidence: Vec<String>,
}

impl AgentLane {
    pub(super) fn is_active(&self) -> bool {
        matches!(
            self.status.as_str(),
            "queued"
                | "starting"
                | "running"
                | "waiting_approval"
                | "needs_input"
                | "blocked"
                | "attached"
                | "detached"
        )
    }
}

pub(super) fn agent_tasks(state: &TuiState) -> Vec<AgentTask> {
    let mut tasks = state
        .runtime
        .tasks
        .iter()
        .map(agent_task)
        .collect::<Vec<_>>();
    tasks.extend(state.runtime.lanes.iter().map(|lane| {
        AgentTask {
            id: lane.id.clone(),
            parent_id: None,
            agent: lane.role.to_string(),
            kind: "lane".to_string(),
            transport: format!("{:?}", lane.route).to_ascii_lowercase(),
            title: lane
                .task_id
                .clone()
                .unwrap_or_else(|| format!("{} lane", lane.role)),
            status: format!("{:?}", lane.status).to_ascii_lowercase(),
            progress: u8::from(matches!(lane.status, viden_core::LaneStatus::Done)) * 100,
            activity: lane.summary.clone(),
            summary: lane.summary.clone(),
            evidence: lane.evidence.clone(),
            next_action: None,
            started_at: None,
            updated_at: None,
            workspace: lane.worktree.clone(),
            permissions: vec![format!("{:?}", lane.mutation_policy).to_ascii_lowercase()],
            decision: None,
            result: None,
            resume_handle: lane.active_session_ids.first().cloned(),
            pid: None,
        }
    }));
    tasks.sort_by(|left, right| {
        right
            .priority()
            .cmp(&left.priority())
            .then_with(|| left.id.cmp(&right.id))
    });
    tasks
}

fn agent_task(task: &AgentTaskRecord) -> AgentTask {
    AgentTask {
        id: task.id.clone(),
        parent_id: task.parent_id.clone(),
        agent: task.role.to_string(),
        kind: task.kind.to_string(),
        transport: format!("{:?}", task.route).to_ascii_lowercase(),
        title: task.title.clone(),
        status: task.status.as_str().to_string(),
        progress: task.progress,
        activity: task.activity.clone(),
        summary: task.summary.clone(),
        evidence: task.evidence.clone(),
        next_action: task.next_action.clone(),
        started_at: task.started_at,
        updated_at: task.updated_at,
        workspace: task.workspace.clone(),
        permissions: task.permissions.clone(),
        decision: task.decision.clone(),
        result: task.result.clone(),
        resume_handle: task.resume_handle.clone(),
        pid: task.pid,
    }
}

pub(super) fn agent_lanes(state: &TuiState) -> Vec<AgentLane> {
    agent_tasks(state)
        .into_iter()
        .map(|task| AgentLane {
            id: task.id.clone(),
            task_id: Some(task.id),
            agent: task.agent,
            screen: if task.kind == "test" {
                "side-2".to_string()
            } else {
                "main".to_string()
            },
            transport: task.transport,
            status: task.status,
            summary: if task.summary.is_empty() {
                task.activity
            } else {
                task.summary
            },
            evidence: task.evidence,
        })
        .collect()
}

/// Presentation-level "something is happening" signal.
///
/// Like [`super::app::runtime_has_active_work`] this treats a non-empty
/// `assistant_stream` as in-flight work, which holds because Core settles the
/// stream on a terminal agent-session fact. It deliberately checks fewer facts
/// than that predicate: it drives status text, not command routing.
pub(super) fn has_active_work(state: &TuiState) -> bool {
    !state.runtime.active_tool_calls.is_empty()
        || !state.runtime.pending_approvals.is_empty()
        || !state.runtime.assistant_stream.is_empty()
        || state.runtime.tasks.iter().any(|task| task.is_active())
        || state.runtime.lanes.iter().any(|lane| lane.is_active())
        || !state.runtime.queued_inputs.is_empty()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProviderStatus {
    pub(super) connection: String,
    pub(super) telemetry: String,
    pub(super) context_window: String,
    pub(super) work_mode: WorkMode,
    pub(super) permission_level: PermissionLevel,
    pub(super) request_count: u64,
    pub(super) success_count: u64,
    pub(super) failure_count: u64,
    pub(super) last_latency_ms: Option<u128>,
    pub(super) average_latency_ms: Option<u128>,
    pub(super) last_event_count: usize,
    pub(super) last_error: Option<String>,
    pub(super) last_input_tokens: Option<u64>,
    pub(super) last_output_tokens: Option<u64>,
    pub(super) last_total_tokens: Option<u64>,
    pub(super) total_tokens: u64,
    pub(super) last_tokens_per_second: Option<u64>,
    pub(super) last_cost_micro_usd: Option<u64>,
    pub(super) total_cost_micro_usd: Option<u64>,
}

pub(super) fn provider_status(state: &TuiState) -> ProviderStatus {
    let provider = state.runtime.provider.as_ref();
    let request_count = provider.map_or(0, |value| value.request_count);
    let failure_count = provider.map_or(0, |value| value.error_count);
    let token_cost = state.runtime.token_cost.as_ref();
    ProviderStatus {
        connection: provider
            .map_or("Configured", |value| value.status.as_str())
            .to_string(),
        telemetry: format!(
            "{request_count} req / {} ok / {failure_count} err",
            request_count.saturating_sub(failure_count)
        ),
        context_window: state
            .runtime
            .context
            .as_ref()
            .map(|context| format!("{}/{}", context.estimated_tokens, context.hard_token_limit))
            .unwrap_or_else(|| "-".to_string()),
        work_mode: state.runtime.snapshot.work_mode,
        permission_level: state.runtime.snapshot.permission_level,
        request_count,
        success_count: request_count.saturating_sub(failure_count),
        failure_count,
        last_latency_ms: provider
            .and_then(|value| value.last_latency_ms)
            .map(u128::from),
        average_latency_ms: provider
            .and_then(|value| value.average_latency_ms)
            .map(u128::from),
        last_event_count: 0,
        last_error: state
            .runtime
            .errors
            .last()
            .map(|error| error.message.clone()),
        last_input_tokens: token_cost.map(|cost| cost.input_tokens),
        last_output_tokens: token_cost.map(|cost| cost.output_tokens),
        last_total_tokens: token_cost.map(|cost| cost.total_tokens),
        total_tokens: state.runtime.cost_ledger.total_tokens,
        last_tokens_per_second: provider.and_then(|value| value.tokens_per_second),
        last_cost_micro_usd: token_cost.and_then(|cost| cost.cost_micro_usd),
        total_cost_micro_usd: state
            .runtime
            .cost_ledger
            .total_actual_cost_micro_usd
            .or(Some(
                state.runtime.cost_ledger.total_estimated_cost_micro_usd,
            )),
    }
}

pub(super) fn workspace_root(state: &TuiState) -> &Path {
    &state.runtime.snapshot.cwd
}

pub(super) fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

pub(super) fn cost_ledger(state: &TuiState) -> &CostLedgerTotals {
    &state.runtime.cost_ledger
}

pub(super) fn provider_health(state: &TuiState) -> Option<&ProviderHealthView> {
    state.runtime.provider.as_ref()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    #[test]
    fn tui_state_has_no_flat_ui_deref_compatibility() {
        let production = include_str!("state.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production state source");
        let deref_impl = ["impl ", "Deref for TuiState"].concat();
        let deref_mut_impl = ["impl ", "DerefMut for TuiState"].concat();

        assert!(
            !production.contains(&deref_impl),
            "flat Deref compatibility remains"
        );
        assert!(
            !production.contains(&deref_mut_impl),
            "flat DerefMut compatibility remains"
        );
    }

    #[test]
    fn tui_source_has_no_authoritative_runtime_effects() {
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let forbidden = [
            "std::process::Command",
            "process::Command",
            "OpenOptions",
            "git worktree",
            "git apply",
            "tmux new-session",
            "SessionEngine",
            ".viden/lanes",
            "ProviderApiKey",
            "\"/provider key ",
        ];
        let mut violations = Vec::new();
        scan_production_rust_sources(&source_root, &forbidden, &mut violations);
        assert!(
            violations.is_empty(),
            "authoritative runtime effects remain in TUI production source:\n{}",
            violations.join("\n")
        );
    }

    fn scan_production_rust_sources(
        directory: &Path,
        forbidden: &[&str],
        violations: &mut Vec<String>,
    ) {
        for entry in fs::read_dir(directory).expect("read TUI source directory") {
            let path = entry.expect("source entry").path();
            if path.is_dir() {
                scan_production_rust_sources(&path, forbidden, violations);
                continue;
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                continue;
            }
            let source = fs::read_to_string(&path).expect("read Rust source");
            let production = source
                .split("#[cfg(test)]\nmod tests")
                .next()
                .unwrap_or(&source);
            for needle in forbidden {
                if production.contains(needle) {
                    violations.push(format!("{}: {needle}", path.display()));
                }
            }
        }
    }
}
