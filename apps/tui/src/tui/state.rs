use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use viden_core::{
    AgentSessionStatus, AgentTaskRecord, CostLedgerTotals, PermissionLevel, ProviderHealthView,
    RuntimeSnapshot, RuntimeViewState, WorkMode,
};
use viden_types::{AgentNextAction, CapabilityId};

pub(super) use super::native_turn::NativeTurnSlot;
pub(super) use super::operator_git::OperatorGitMachine;
pub(super) use super::pending::SupervisionMachine;
pub(super) use super::ui_state::{
    AcpPickerPhase, ConflictDetailTarget, FocusedConversation, GitPickerPhase, InteractionPanel,
    Lens, OverlayState, PendingAcpStart, PendingNativeLane, ProviderAuthMode, ProviderOption,
    SupervisionInput, SupervisionPanel, TuiEntry, TuiUiState,
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
    /// The same discipline for one in-flight operator source-control action.
    /// It is a separate slot because a `/git` action and a merge-gate decision
    /// answer different questions and must not block each other.
    pub(super) operator_git: OperatorGitMachine,
    /// The one liveness window this client holds for a built-in-provider turn,
    /// which Core publishes no turn-liveness fact for. See
    /// [`super::native_turn`] for the exact window and how it closes.
    pub(super) native_turn: NativeTurnSlot,
}

impl TuiState {
    pub(super) fn new(runtime: RuntimeViewState) -> Self {
        Self {
            runtime,
            ui: TuiUiState::default(),
            capabilities: BTreeSet::new(),
            supervision: SupervisionMachine::default(),
            operator_git: OperatorGitMachine::default(),
            native_turn: NativeTurnSlot::default(),
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

/// Whether an Agent session Core published is running right now.
fn agent_session_is_live(session: &viden_core::AgentSessionView) -> bool {
    matches!(
        session.status,
        AgentSessionStatus::Starting
            | AgentSessionStatus::Running
            | AgentSessionStatus::WaitingApproval
    )
}

/// Whether a Lane lifecycle state means Core is running work for that Lane.
///
/// [`LaneStatus::is_active`](viden_types::LaneStatus::is_active) cannot answer
/// this: it is a *lifecycle* predicate and is true for `Draft`, `Attached`, and
/// `Detached` — `Draft` being exactly where Core leaves a starter Lane the
/// moment it is created. Treating it as liveness told this client that a
/// freshly created, idle Lane was a running turn, which is half of open
/// follow-up 6 in `docs/core-0.3-compatibility.md`.
///
/// `Blocked` is excluded on the same grounds: an apply conflict is waiting for
/// a person, and it is rendered as a decision rather than as activity.
fn lane_is_running(status: viden_types::LaneStatus) -> bool {
    matches!(
        status,
        viden_types::LaneStatus::Queued
            | viden_types::LaneStatus::Starting
            | viden_types::LaneStatus::Running
            | viden_types::LaneStatus::WaitingApproval
            | viden_types::LaneStatus::NeedsInput
    )
}

/// Whether a Core fact carrying an optional owner belongs to the session scope
/// this composer addresses.
///
/// The composer sends `SubmitUserInput` on the driver's own envelope owner,
/// which names no Lane, so a fact whose owner names a Lane addresses different
/// work. An *absent* owner is counted as this scope on purpose: the frontend
/// contract says an absent owner "means the fact belongs to no Lane scope"
/// (`docs/frontend-integration-contract.md`), and the built-in provider path
/// emits every one of its tool-call and evidence facts with `owner: None`
/// (`crates/runtime/src/runtime_contract.rs`). Reading those as somebody
/// else's work would let the composer start a second concurrent turn; a client
/// may err toward busy for an unattributed fact, never toward idle.
fn owner_is_session_scoped(owner: Option<&viden_core::RuntimeOwner>) -> bool {
    owner.is_none_or(|owner| owner.lane_id.is_none())
}

/// Whether the owner *this composer input addresses* is running a turn.
///
/// This is the routing half of the client's busy question — queue versus
/// submit — and it is owner-scoped, mirroring the rule the GUI's composer
/// already applies (`apps/gui/src-tauri/src/projection.rs`). It reads only
/// facts that say a turn is in flight for that owner:
///
/// - a native turn this client submitted and is still waiting on
///   ([`super::native_turn`], which also documents the exact residue window);
/// - an active tool call, a pending approval, an active task, or a live Agent
///   session whose published owner is this scope.
///
/// Three facts are deliberately *not* read, because none of them is about this
/// input's target:
///
/// - Lane lifecycle state. A `Draft` starter Lane, or any Lane running its own
///   turn, is different work with a different owner. This was E1's second
///   stall.
/// - `queued_inputs`. Core publishes the session queue and never drains it —
///   the only producer of `InputDequeued` is the Lane worker's own queue — so
///   once anything was queued this stayed true forever. That is Core's open
///   follow-up 5 in `docs/core-0.3-compatibility.md`, still open; this client
///   simply stops treating a queue Core will not run as evidence of a turn.
/// - `assistant_stream`. It is Core's *unscoped* stream and Core settles it
///   only on a terminal Agent-session fact, which the built-in path never
///   publishes, so a finished reply stays there for the rest of the session.
///   That was E1's first stall. The live window for that path is held by
///   [`super::native_turn`] instead.
pub(super) fn composer_target_busy(state: &TuiState) -> bool {
    let view = &state.runtime;
    state.native_turn.is_in_flight()
        || view
            .active_tool_calls
            .iter()
            .any(|call| owner_is_session_scoped(call.owner.as_ref()))
        || view
            .pending_approvals
            .iter()
            .any(|approval| owner_is_session_scoped(Some(&approval.owner)))
        || view
            .tasks
            .iter()
            .any(|task| task.is_active() && owner_is_session_scoped(task.owner.as_ref()))
        || view
            .agent_sessions
            .iter()
            .any(|session| agent_session_is_live(session) && session.owner.lane_id.is_none())
}

/// Presentation-level "something is happening" signal.
///
/// This is the status line's `ACTIVE` word, the live-work strip, the exit
/// confirmation, and what `Ctrl-C`/`Esc` treat as work in progress. Unlike
/// [`composer_target_busy`] it is not owner-scoped: a Lane running its own turn
/// is something happening, and the status row must say so even though the
/// composer's own target is idle.
///
/// H1's single-predicate rule survives the split as an implication rather than
/// an equality, and it holds *by construction* because this function starts
/// from the routing answer: whenever the composer queues, the status line says
/// `ACTIVE`. The two can never describe one turn two ways; they can only
/// describe two different turns. `state_tests::the_two_predicates_never_
/// disagree_about_the_composer_s_own_turn` pins that direction.
pub(super) fn has_active_work(state: &TuiState) -> bool {
    let view = &state.runtime;
    composer_target_busy(state)
        || !view.active_tool_calls.is_empty()
        || !view.pending_approvals.is_empty()
        || view.tasks.iter().any(|task| task.is_active())
        || view.lanes.iter().any(|lane| lane_is_running(lane.status))
        || view.agent_sessions.iter().any(agent_session_is_live)
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

    use super::{TuiState, composer_target_busy, has_active_work};

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

    /// H1's case, kept: a live Agent session is the only fact in the view. It is
    /// session-scoped here — the owner names no Lane — so it is this composer's
    /// own target and both predicates must see it. The bug this pinned was a
    /// status line that omitted `agent_sessions` while routing read it, so the
    /// composer and the status row described one turn two ways.
    #[test]
    fn a_live_agent_session_is_active_work_for_routing_and_for_status_text() {
        let mut view = empty_view();
        view.agent_sessions.push(running_session());
        let state = TuiState::new(view);

        assert!(composer_target_busy(&state));
        assert!(has_active_work(&state));
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

        assert!(!composer_target_busy(&state));
        assert!(!has_active_work(&state));
    }

    /// Moved baseline. It used to assert that Core's unscoped `assistant_stream`
    /// *is* the built-in path's liveness fact, which was true of the code and
    /// false of the runtime: Core settles that stream only on a terminal
    /// Agent-session fact, and the built-in path publishes none, so the residue
    /// never cleared and the composer queued for the rest of the session (E1's
    /// first stall, open follow-up 6). The liveness it was reaching for now
    /// lives in `native_turn`, which opens on this client's own dispatched
    /// command id and closes on the batch that ends the turn.
    #[test]
    fn a_streaming_built_in_turn_is_live_by_this_client_s_own_dispatch_not_by_stream_residue() {
        let mut view = empty_view();
        view.assistant_stream = "Working on the config loader...".to_string();
        let mut state = TuiState::new(view);

        assert!(state.runtime.agent_sessions.is_empty());
        assert!(state.runtime.tasks.is_empty());
        assert!(
            !composer_target_busy(&state),
            "settled stream text is residue, not a running turn"
        );
        assert!(!has_active_work(&state));

        state.native_turn.begin("tui-7");

        assert!(composer_target_busy(&state));
        assert!(has_active_work(&state));
    }

    /// E1's second stall at the predicate level. `LaneStatus::is_active` is a
    /// lifecycle predicate that counts `Draft` — where Core leaves a starter
    /// Lane — and `queued_inputs` never empties because Core never drains the
    /// session queue (open follow-up 5, still open). Neither says a turn is
    /// running, for this composer's target or for anyone.
    #[test]
    fn a_draft_lane_and_a_stuck_session_queue_are_not_active_work() {
        let mut view = empty_view();
        let mut lanes: Vec<viden_types::AgentLaneRecord> = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        lanes.truncate(1);
        lanes[0].status = viden_types::LaneStatus::Draft;
        view.lanes = lanes;
        view.queued_inputs.push(viden_core::QueuedInputView {
            id: "queued-1".to_string(),
            content_preview: "first".to_string(),
            created_at: None,
            owner: None,
        });
        let state = TuiState::new(view);

        assert!(
            state.runtime.lanes[0].is_active(),
            "the lifecycle fact holds"
        );
        assert!(!composer_target_busy(&state));
        assert!(!has_active_work(&state));
    }

    /// The documented shape of the split: routing is owner-scoped, presentation
    /// is not, and presentation is built *from* routing so the implication can
    /// never be broken by editing one of them. A Lane running its own turn is
    /// `ACTIVE` on the status row and is not this input's target.
    #[test]
    fn the_two_predicates_never_disagree_about_the_composer_s_own_turn() {
        let mut view = empty_view();
        let mut lanes: Vec<viden_types::AgentLaneRecord> = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        lanes.truncate(1);
        lanes[0].status = viden_types::LaneStatus::Running;
        view.lanes = lanes;
        let mut lane_session = running_session();
        lane_session.owner = viden_core::RuntimeOwner {
            lane_id: Some("lane-a".to_string()),
            ..Default::default()
        };
        view.agent_sessions.push(lane_session);
        let mut state = TuiState::new(view);

        assert!(has_active_work(&state), "the status row says ACTIVE");
        assert!(
            !composer_target_busy(&state),
            "another owner's turn is not this input's target"
        );

        // Whenever routing queues, presentation must agree. That direction is
        // structural: `has_active_work` starts from `composer_target_busy`.
        state.native_turn.begin("tui-1");
        assert!(composer_target_busy(&state));
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
