use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use viden_core::{
    AgentSessionStatus, AgentTaskRecord, CostLedgerTotals, PermissionLevel, ProviderHealthView,
    RuntimeSnapshot, RuntimeViewState, WorkMode,
};
use viden_types::{AgentNextAction, CapabilityId};

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
}

impl TuiState {
    pub(super) fn new(runtime: RuntimeViewState) -> Self {
        Self {
            runtime,
            ui: TuiUiState::default(),
            capabilities: BTreeSet::new(),
            supervision: SupervisionMachine::default(),
            operator_git: OperatorGitMachine::default(),
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
/// the composer addresses when no Agent conversation is focused.
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

/// Which owner scope the next composer submission names.
///
/// Two, because the composer has two send paths: a focused Agent conversation
/// sends `SendAgentSessionInput` on that session's own owner, and everything
/// else sends `SubmitUserInput` on the driver's envelope owner, which names no
/// Lane. The scope is resolved once, here, so the routing predicate and the
/// send path can never disagree about whose turn they are asking about.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ComposerTarget {
    /// The session-scoped composer: `lane_id: None`.
    Session,
    /// A Lane's own conversation, named by the Lane the focused Agent session
    /// belongs to.
    Lane { lane_id: String },
}

/// The scope the next composer submission addresses.
///
/// A focused Agent session whose record Core no longer publishes falls back to
/// the session scope rather than to a Lane id this client remembers: the Lane
/// of a session Core has dropped is not a fact, and guessing one would gate
/// the composer on somebody else's turn.
fn composer_target(state: &TuiState) -> ComposerTarget {
    let Some(FocusedConversation::AcpSession(session_id)) = state.ui.focused_conversation.as_ref()
    else {
        return ComposerTarget::Session;
    };
    state
        .runtime
        .agent_sessions
        .iter()
        .find(|session| &session.session_id == session_id)
        .map(|session| ComposerTarget::Lane {
            lane_id: session.lane_id.clone(),
        })
        .unwrap_or(ComposerTarget::Session)
}

/// Whether Core is running a turn for `target` right now.
///
/// This is the whole of the liveness question since `runtime.turn_lifecycle`:
/// `RuntimeViewState.active_turns` is a Core fact, upserted on `TurnStarted`
/// and removed on `TurnFinished` for every exit — completed, failed, and
/// cancelled alike — on both the native and the ACP path. `turn_id` is
/// deliberately not matched on: it is a fresh per-turn value that exists so an
/// audit row has something to join on, and the frontend contract says a client
/// matches the *scope*.
///
/// An empty `active_turns` is a real answer meaning nothing is running,
/// including right after a restart, because Core never resumes a turn across
/// one and never persists either fact.
fn turn_is_active_for(state: &TuiState, target: &ComposerTarget) -> bool {
    state.runtime.active_turns.iter().any(|turn| match target {
        ComposerTarget::Session => turn.owner.lane_id.is_none(),
        ComposerTarget::Lane { lane_id } => turn.owner.lane_id.as_deref() == Some(lane_id.as_str()),
    })
}

/// Whether Core is running a session-scoped turn right now.
///
/// The queue copy needs this on its own, separate from
/// [`composer_target_busy`]: the session queue drains only behind a
/// *completed session-scoped* turn, so what a queued prompt is waiting for is
/// this exact fact and not "something is happening". A Lane's turn neither
/// arms nor disarms that drain.
pub(super) fn session_turn_is_active(state: &TuiState) -> bool {
    turn_is_active_for(state, &ComposerTarget::Session)
}

/// Whether the owner *this composer input addresses* is running a turn.
///
/// This is the routing half of the client's busy question — queue versus
/// submit — and it is owner-scoped, mirroring the rule the GUI's composer
/// applies (`apps/gui/src-tauri/src/projection.rs`). It reads only facts that
/// say a turn is in flight for that owner:
///
/// - a Core-published turn for that scope ([`turn_is_active_for`]);
/// - for the session scope, an active tool call, a pending approval, an active
///   task, or a live Agent session whose published owner is that scope;
/// - for a Lane's conversation, that Lane's own Agent session running.
///
/// Four facts are deliberately *not* read, because none of them is about this
/// input's target:
///
/// - `assistant_stream`. It is Core's *unscoped* stream; before
///   `runtime.turn_lifecycle` Core settled it only on a terminal Agent-session
///   fact, so a finished native reply stayed there for the rest of the session
///   and the composer read it as a running turn (E1's first stall). Core now
///   clears it on the session-scoped `TurnFinished`, and this predicate does
///   not read it either way.
/// - This client's own dispatched command id. `apps/tui/src/tui/native_turn.rs`
///   held that window until T2 and closed it on the first `SnapshotUpdated`
///   after dispatch, which is not a turn-liveness fact: three operator
///   commands publish one of their own, so changing work mode, permission
///   level or model mid-turn closed it early. `active_turns` replaces it with
///   the Core fact, and the client holds no window at all.
/// - `queued_inputs`. A queue is work Core has *not* started; the drain is
///   announced by `InputDequeued` and the turn it starts by `TurnStarted`,
///   which is the fact this reads.
/// - Lane lifecycle state. A `Draft` starter Lane, or any Lane running its own
///   turn, is different work with a different owner. That was E1's second
///   stall.
///
/// Without the `runtime.turn_lifecycle` capability `active_turns` is always
/// empty, so this client submits and Core answers: the supervisor refuses a
/// second job for one owner with `CommandRejected`. That is a Core answer
/// rather than a client guess, which is the direction the contract requires.
pub(super) fn composer_target_busy(state: &TuiState) -> bool {
    let view = &state.runtime;
    let target = composer_target(state);
    if turn_is_active_for(state, &target) {
        return true;
    }
    match &target {
        ComposerTarget::Lane { lane_id } => view
            .agent_sessions
            .iter()
            .any(|session| agent_session_is_live(session) && &session.lane_id == lane_id),
        ComposerTarget::Session => {
            view.active_tool_calls
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
                || view.agent_sessions.iter().any(|session| {
                    agent_session_is_live(session) && session.owner.lane_id.is_none()
                })
        }
    }
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
        || !view.active_turns.is_empty()
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
        AgentSessionStatus, AgentSessionView, PermissionLevel, PermissionMode, RuntimeOwner,
        RuntimeSnapshot, RuntimeViewState, TurnSource, TurnView, WorkMode,
    };

    use super::{FocusedConversation, TuiState, composer_target_busy, has_active_work};

    /// One Core-published turn, in the scope `lane_id` names.
    ///
    /// Built from a `TurnView` rather than from a client-side flag because the
    /// predicate under test must read Core's fact and nothing else.
    fn active_turn(turn_id: &str, lane_id: Option<&str>) -> TurnView {
        TurnView {
            turn_id: turn_id.to_string(),
            owner: RuntimeOwner {
                workspace_id: "ws_fixture".to_string(),
                project_id: "prj_fixture".to_string(),
                lane_id: lane_id.map(str::to_string),
                turn_id: Some(turn_id.to_string()),
                ..RuntimeOwner::default()
            },
            source: TurnSource::UserInput,
            started_at: 1_700_000_000,
        }
    }

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

    /// Moved baseline, twice. It first asserted that Core's unscoped
    /// `assistant_stream` *is* the built-in path's liveness fact, which was
    /// true of the code and false of the runtime (E1's first stall, follow-up
    /// 6); T1c moved it to this client's own dispatched command id. T2 moves
    /// it to the Core fact: `runtime.turn_lifecycle` brackets every native
    /// turn, so liveness is an `active_turns` entry for this scope and the
    /// client holds no window of its own.
    #[test]
    fn a_streaming_built_in_turn_is_live_by_cores_active_turn_not_by_stream_residue() {
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

        state.runtime.active_turns.push(active_turn("turn_1", None));

        assert!(composer_target_busy(&state));
        assert!(has_active_work(&state));
    }

    /// E1 defect 2, scenario one, at the predicate level: one fallback turn
    /// completes. Core removes the turn on `TurnFinished` and clears the
    /// unscoped stream, so the residue that used to keep this client queueing
    /// forever is gone *and* is not read even when a stream is left behind by
    /// a Core that does not settle it.
    #[test]
    fn the_composer_is_idle_after_one_completed_fallback_turn() {
        let mut view = empty_view();
        // What the reducer leaves behind after `TurnFinished`: no active turn.
        // The stream text is kept here deliberately, so the assertion holds
        // even against a Core that publishes the terminal fact without
        // settling the stream.
        view.assistant_stream = "The config loader lives in crates/config.".to_string();
        let state = TuiState::new(view);

        assert!(state.runtime.active_turns.is_empty());
        assert!(
            !composer_target_busy(&state),
            "a completed turn must not keep the composer queueing"
        );
        assert!(!has_active_work(&state));
    }

    /// The Lane half of the owner scoping. A Lane's own turn is that Lane
    /// conversation's target and is not the session composer's, and the
    /// session's turn is not the Lane conversation's.
    #[test]
    fn a_lane_turn_gates_that_lanes_conversation_and_not_the_session_composer() {
        let mut view = empty_view();
        view.agent_sessions.push(running_session());
        view.agent_sessions[0].status = AgentSessionStatus::Completed;
        view.active_turns
            .push(active_turn("turn_lane", Some("lane-a")));
        let mut state = TuiState::new(view);

        assert!(
            !composer_target_busy(&state),
            "another owner's turn is not the session composer's target"
        );
        assert!(has_active_work(&state), "the status row still says ACTIVE");

        state.ui.focused_conversation = Some(FocusedConversation::AcpSession(
            "agent-session_1".to_string(),
        ));
        assert!(
            composer_target_busy(&state),
            "the focused Lane conversation's own turn is its target"
        );

        state.runtime.active_turns.clear();
        state
            .runtime
            .active_turns
            .push(active_turn("turn_session", None));
        assert!(
            !composer_target_busy(&state),
            "the session's turn is not the Lane conversation's target"
        );
    }

    /// E1 defect 2, scenario two, at the predicate level: one Lane is created
    /// and sits in `Draft` with a session-level queue behind it.
    /// `LaneStatus::is_active` is a *lifecycle* predicate that counts `Draft` —
    /// where Core leaves a starter Lane — and a queue is work Core has not
    /// started: the drain is announced by `InputDequeued` and the turn it
    /// starts by `TurnStarted`, which is what the predicate reads. Neither
    /// says a turn is running, for this composer's target or for anyone.
    #[test]
    fn a_draft_lane_and_a_session_queue_are_not_active_work() {
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
        state
            .runtime
            .active_turns
            .push(active_turn("turn_session", None));
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
