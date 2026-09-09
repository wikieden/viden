use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use viden_core::{
    AgentSessionStatus, AgentTaskRecord, CostLedgerTotals, PermissionLevel, ProviderHealthView,
    RuntimeSnapshot, RuntimeViewState, WorkMode,
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

/// The single "Core is busy" predicate for this client.
///
/// Command routing (queue versus submit, and what `Ctrl-C` cancels) and the
/// status line must never disagree about whether a turn is running, so both
/// read exactly these facts and nothing else. It is deliberately a function of
/// [`RuntimeViewState`] alone: no TUI-local presentation state may make Core
/// look busier or idler than the facts it published. The status line used to
/// omit `agent_sessions`, so an Agent turn that had published nothing else yet
/// read as busy to the composer and as idle to the status row.
///
/// `assistant_stream` stays in the set because for a built-in-provider turn it
/// is the only liveness fact Core publishes at all: that path emits no Agent
/// session and no task, and the supervisor streams its deltas from a worker
/// thread, so the composer is live while the turn runs. The known cost is that
/// Core settles the stream only on a terminal agent-session fact, so a
/// built-in turn's text stays there after it ends and this predicate stays
/// true. That residue is Core's recorded limitation (see the streaming
/// semantics note in `docs/core-0.3-compatibility.md`); closing it needs a
/// turn-liveness fact for the built-in path, not a client-side guess that a
/// turn ended.
pub(super) fn runtime_has_active_work(view: &RuntimeViewState) -> bool {
    !view.active_tool_calls.is_empty()
        || !view.pending_approvals.is_empty()
        || !view.assistant_stream.is_empty()
        || view.tasks.iter().any(|task| task.is_active())
        || view.lanes.iter().any(|lane| lane.is_active())
        || view.agent_sessions.iter().any(|session| {
            matches!(
                session.status,
                AgentSessionStatus::Starting
                    | AgentSessionStatus::Running
                    | AgentSessionStatus::WaitingApproval
            )
        })
        || !view.queued_inputs.is_empty()
}

/// Presentation-level "something is happening" signal.
///
/// This is the same predicate command routing uses; the status line and the
/// composer cannot describe one turn two ways.
pub(super) fn has_active_work(state: &TuiState) -> bool {
    runtime_has_active_work(&state.runtime)
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
    use std::{fs, path::Path, path::PathBuf};

    use viden_core::{
        AgentSessionStatus, AgentSessionView, PermissionLevel, PermissionMode, RuntimeSnapshot,
        RuntimeViewState, WorkMode,
    };

    use super::{TuiState, has_active_work, runtime_has_active_work};

    fn empty_view() -> RuntimeViewState {
        RuntimeViewState::new(RuntimeSnapshot {
            cwd: PathBuf::from("/workspace"),
            provider_family: "fallback".to_string(),
            model_label: "test-local".to_string(),
            work_mode: WorkMode::Build,
            permission_mode: PermissionMode::Default,
            permission_level: PermissionLevel::Ask,
            config_summary: "fixture".to_string(),
            loaded_config_files: Vec::new(),
            startup_overrides: Vec::new(),
            ui_preferences: Default::default(),
        })
    }

    fn running_session() -> AgentSessionView {
        AgentSessionView {
            session_id: "agent-session_1".to_string(),
            lane_id: "lane-a".to_string(),
            agent_id: "codex".to_string(),
            model: None,
            status: AgentSessionStatus::Running,
            owner: Default::default(),
            task: "review the gate".to_string(),
            diagnostic: None,
            output: None,
        }
    }

    /// The first case where the two definitions disagreed: a live Agent session
    /// is the only fact in the view. Command routing already queued against it
    /// while the status line called the client idle, so the composer and the
    /// status row described one turn two ways.
    #[test]
    fn a_live_agent_session_is_active_work_for_routing_and_for_status_text() {
        let mut view = empty_view();
        view.agent_sessions.push(running_session());
        let state = TuiState::new(view);

        assert!(runtime_has_active_work(&state.runtime));
        assert_eq!(
            has_active_work(&state),
            runtime_has_active_work(&state.runtime)
        );
    }

    /// A finished Agent session is not live work: the published status is the
    /// fact, not the presence of a session record. This is the other half of
    /// the case above — folding `agent_sessions` into the status line must not
    /// make a workspace with history read as permanently busy.
    #[test]
    fn a_completed_agent_session_is_not_active_work() {
        let mut view = empty_view();
        let mut session = running_session();
        session.status = AgentSessionStatus::Completed;
        view.agent_sessions.push(session);
        let state = TuiState::new(view);

        assert!(!runtime_has_active_work(&state.runtime));
        assert!(!has_active_work(&state));
    }

    /// A built-in-provider turn publishes no Agent session and no task, and the
    /// supervisor streams its deltas from a worker thread while the composer is
    /// live. `assistant_stream` is therefore the only fact that says the turn is
    /// running; dropping it from the unified predicate would submit a second
    /// concurrent turn instead of queueing a follow-up.
    #[test]
    fn a_streaming_built_in_turn_is_active_work_with_no_session_or_task_fact() {
        let mut view = empty_view();
        view.assistant_stream = "Working on the config loader...".to_string();
        let state = TuiState::new(view);

        assert!(state.runtime.agent_sessions.is_empty());
        assert!(state.runtime.tasks.is_empty());
        assert!(runtime_has_active_work(&state.runtime));
        assert!(has_active_work(&state));
    }

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
