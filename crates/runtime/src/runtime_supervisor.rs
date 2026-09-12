use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
#[cfg(test)]
use std::sync::OnceLock;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, Sender},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use viden_lanes::{
    LaneApprovalResolver, LaneCommandRedactor, LaneEffectExecutor, LanePersistence, LaneSupervisor,
    LocalLaneEffectExecutor, WorkflowLanePersistence,
};
use viden_provider::ModelRequestControl;
use viden_types::{
    AgentSessionRequest, AgentSessionStatus, AgentSessionView, ApprovalDecision,
    ApprovalDefaultAction, ApprovalRequestView, ApprovalResponse, ApprovalRisk, ApprovalScope,
    ApprovalTarget, AuditActor, AuditObjectRef, AuditOutcome, AuditRecord, CapabilityId,
    EventCursor, FRONTEND_SCHEMA_V1, FRONTEND_V1_CAPABILITIES, FRONTEND_V1_EXTENSION_CAPABILITIES,
    GapRecovery, LaneRuntimeOwnerBinding, OperatorGitAction, PermissionLevel, PermissionPrompt,
    ReplayBatch, ReplayRequest, RuntimeCommand, RuntimeCommandEnvelope, RuntimeErrorView,
    RuntimeEvent, RuntimeEventEnvelope, RuntimeEventKind, RuntimeOwner, RuntimeSnapshotEnvelope,
    RuntimeViewState, RuntimeWireEvent, TranscriptPage, TranscriptPageRequest, TurnOutcome,
    TurnSource, TurnView, WorkMode, fresh_id, now_timestamp,
};
use viden_workflows::stores::WorkflowStore;

use viden_agents::{
    AgentSessionApprover, cancel_typed_agent_session, mark_typed_agent_session_status,
    resume_typed_agent_session, retry_typed_agent_session, shutdown_resident_acp_sessions,
    start_typed_agent_session, typed_agent_session_request_from_compat_input,
    validate_typed_agent_session_request,
};

use crate::{
    RuntimeEventSink, SessionEngine,
    event_journal::RuntimeEventJournal,
    project_runtime::SupervisorProjectMutationPreparation,
    runtime_contract::{
        ContextRetrievalJob, SupervisorContextRetrievalPreparation, execute_context_retrieval_job,
        redacted_runtime_command_for_event,
    },
};

struct PendingApproval {
    owner: RuntimeOwner,
    audit_id: String,
    /// The tool the operator is being asked about. Kept because the durable
    /// audit row for the decision (`runtime.durable_work_evidence`) names it:
    /// an audit timeline that recorded only "an approval was allowed" would not
    /// say what was allowed.
    tool_name: String,
    expires_at: u64,
    // An ordinary approval cannot survive any permission-control reservation.
    permission_epoch: u64,
    allowed_scopes: Vec<ApprovalScope>,
    target: PendingApprovalTarget,
}

impl PendingApprovalTarget {
    /// The active job this approval is blocking, which every variant names.
    fn owner_id(&self) -> Option<&str> {
        match self {
            Self::Channel { owner_id, .. }
            | Self::ContextRetrieval { owner_id, .. }
            | Self::ProjectMutation { owner_id, .. } => Some(owner_id.as_str()),
        }
    }
}

enum PendingApprovalTarget {
    Channel {
        owner_id: String,
        sender: Sender<ApprovalResponse>,
    },
    ContextRetrieval {
        owner_id: String,
        job: Box<ContextRetrievalJob>,
    },
    ProjectMutation {
        owner_id: String,
        command: Box<RuntimeCommand>,
    },
}

#[derive(Clone)]
enum ActiveJobState {
    Running,
    PendingApproval { request_id: String },
}

#[derive(Clone)]
struct ActiveRuntimeControl {
    owner_id: String,
    owner: RuntimeOwner,
    control: ModelRequestControl,
    state: ActiveJobState,
}

#[derive(Debug, Clone, Copy)]
struct PermissionControlValues {
    work_mode: WorkMode,
    permission_level: PermissionLevel,
}

impl PermissionControlValues {
    fn blocks_mutation(self) -> bool {
        self.work_mode != WorkMode::Build || self.permission_level == PermissionLevel::ReadOnly
    }

    fn apply(&mut self, command: &RuntimeCommand) {
        match command {
            RuntimeCommand::SetWorkMode { mode } => {
                self.work_mode = *mode;
                if *mode == WorkMode::Build && self.permission_level == PermissionLevel::ReadOnly {
                    self.permission_level = PermissionLevel::Ask;
                } else if *mode != WorkMode::Build {
                    self.permission_level = PermissionLevel::ReadOnly;
                }
            }
            RuntimeCommand::SetPermissionLevel { level } => {
                self.permission_level = *level;
                if *level == PermissionLevel::ReadOnly {
                    self.work_mode = WorkMode::Plan;
                } else if self.work_mode == WorkMode::Plan {
                    self.work_mode = WorkMode::Build;
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
struct SubmittedPermissionControl {
    epoch: u64,
    command: RuntimeCommand,
}

#[derive(Debug, Clone)]
struct PermissionControlState {
    applied: PermissionControlValues,
    applied_epoch: u64,
    submitted: Vec<SubmittedPermissionControl>,
    next_epoch: u64,
}

impl PermissionControlState {
    fn new(work_mode: WorkMode, permission_level: PermissionLevel) -> Self {
        Self {
            applied: PermissionControlValues {
                work_mode,
                permission_level,
            },
            applied_epoch: 0,
            submitted: Vec::new(),
            next_epoch: 0,
        }
    }

    fn projected(&self) -> PermissionControlValues {
        let mut values = self.applied;
        for submitted in &self.submitted {
            values.apply(&submitted.command);
        }
        values
    }

    fn blocks_mutation(&self) -> bool {
        self.projected().blocks_mutation()
    }

    fn epoch(&self) -> u64 {
        self.next_epoch
    }

    // Ordinary approvals fail closed at reservation time. A failed control is
    // removed from the applied-state projection, but its generation is never
    // rolled back or reused; lane approvals remain tied to applied_epoch.
    fn reserve(&mut self, command: &RuntimeCommand) -> u64 {
        self.next_epoch = self.next_epoch.saturating_add(1);
        let epoch = self.next_epoch;
        self.submitted.push(SubmittedPermissionControl {
            epoch,
            command: command.clone(),
        });
        epoch
    }

    fn commit(&mut self, epoch: u64) -> Result<(), String> {
        let Some(submitted) = self.submitted.first() else {
            return Err(format!(
                "permission control epoch `{epoch}` is not submitted"
            ));
        };
        if submitted.epoch != epoch {
            return Err(format!(
                "permission control epoch `{epoch}` cannot commit before `{}`",
                submitted.epoch
            ));
        }
        let submitted = self.submitted.remove(0);
        self.applied.apply(&submitted.command);
        self.applied_epoch = epoch;
        Ok(())
    }

    fn reject(&mut self, epoch: u64) {
        if let Some(index) = self
            .submitted
            .iter()
            .position(|submitted| submitted.epoch == epoch)
        {
            self.submitted.remove(index);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RuntimeOwnerKey {
    workspace_id: String,
    project_id: String,
    lane_id: Option<String>,
    session_id: Option<String>,
    task_id: Option<String>,
    turn_id: Option<String>,
}

impl From<&RuntimeOwner> for RuntimeOwnerKey {
    fn from(owner: &RuntimeOwner) -> Self {
        Self {
            workspace_id: owner.workspace_id.clone(),
            project_id: owner.project_id.clone(),
            lane_id: owner.lane_id.clone(),
            session_id: owner.session_id.clone(),
            task_id: owner.task_id.clone(),
            turn_id: owner.turn_id.clone(),
        }
    }
}

type ActiveControlRegistry = Arc<Mutex<BTreeMap<RuntimeOwnerKey, ActiveRuntimeControl>>>;

#[derive(Clone, Copy)]
struct SupervisorShared<'a> {
    event_bus: &'a RuntimeEventBus,
    active_control: &'a ActiveControlRegistry,
    pending_approvals: &'a Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    approval_timers: &'a Arc<ApprovalTimerRegistry>,
    permission_control: &'a Arc<Mutex<PermissionControlState>>,
}

#[derive(Default)]
struct ApprovalTimerRegistry {
    shutdown: AtomicBool,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl ApprovalTimerRegistry {
    fn push(&self, worker: JoinHandle<()>) {
        if let Ok(mut workers) = self.workers.lock() {
            workers.push(worker);
        }
    }

    fn shutdown_and_join(&self) {
        self.shutdown.store(true, Ordering::Release);
        let workers = self
            .workers
            .lock()
            .map(|mut workers| std::mem::take(&mut *workers))
            .unwrap_or_default();
        for worker in workers {
            let _ = worker.join();
        }
    }
}

#[derive(Clone)]
struct RuntimeEventBus {
    sender: Sender<RuntimeEventEnvelope>,
    state: Arc<Mutex<RuntimeEventState>>,
}

struct RuntimeEventState {
    journal: RuntimeEventJournal,
    live_view: RuntimeViewState,
    lane_agent_bindings: BTreeMap<String, LaneAgentExecutionBinding>,
    lane_agent_store: Option<WorkflowStore>,
}

#[derive(Debug, Clone)]
struct LaneAgentExecutionBinding {
    lane_id: String,
    agent_id: String,
    session_id: String,
}

#[cfg(test)]
type BeforeContextResumeHook = Arc<dyn Fn(&ModelRequestControl) + Send + Sync>;

#[cfg(test)]
static BEFORE_CONTEXT_RESUME_ENQUEUE_HOOK: OnceLock<Mutex<Option<BeforeContextResumeHook>>> =
    OnceLock::new();

#[cfg(test)]
pub(crate) fn set_before_context_resume_enqueue_hook(hook: Option<BeforeContextResumeHook>) {
    if let Ok(mut slot) = BEFORE_CONTEXT_RESUME_ENQUEUE_HOOK
        .get_or_init(|| Mutex::new(None))
        .lock()
    {
        *slot = hook;
    }
}

#[cfg(test)]
fn before_context_resume_enqueue_for_test(control: &ModelRequestControl) {
    let hook = BEFORE_CONTEXT_RESUME_ENQUEUE_HOOK
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|slot| slot.clone());
    if let Some(hook) = hook {
        hook(control);
    }
}

#[cfg(not(test))]
fn before_context_resume_enqueue_for_test(_control: &ModelRequestControl) {}

#[cfg(test)]
type BeforeSupervisorCommandHook = Arc<dyn Fn(&str) + Send + Sync>;

#[cfg(test)]
static BEFORE_SUPERVISOR_COMMAND_HOOK: std::sync::OnceLock<
    Mutex<Option<BeforeSupervisorCommandHook>>,
> = std::sync::OnceLock::new();

#[cfg(test)]
pub(crate) fn set_before_supervisor_command_hook(hook: Option<BeforeSupervisorCommandHook>) {
    if let Ok(mut slot) = BEFORE_SUPERVISOR_COMMAND_HOOK
        .get_or_init(|| Mutex::new(None))
        .lock()
    {
        *slot = hook;
    }
}

#[cfg(test)]
fn before_supervisor_command_for_test(command_id: &str) {
    let hook = BEFORE_SUPERVISOR_COMMAND_HOOK
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|slot| slot.clone());
    if let Some(hook) = hook {
        hook(command_id);
    }
}

#[cfg(not(test))]
fn before_supervisor_command_for_test(_command_id: &str) {}

#[cfg(test)]
type BeforePermissionControlHook = Arc<dyn Fn(&str, &SessionEngine) + Send + Sync>;

#[cfg(test)]
static BEFORE_PERMISSION_CONTROL_HOOK: OnceLock<Mutex<Option<BeforePermissionControlHook>>> =
    OnceLock::new();

#[cfg(test)]
pub(crate) fn set_before_permission_control_hook(hook: Option<BeforePermissionControlHook>) {
    if let Ok(mut slot) = BEFORE_PERMISSION_CONTROL_HOOK
        .get_or_init(|| Mutex::new(None))
        .lock()
    {
        *slot = hook;
    }
}

#[cfg(test)]
fn before_permission_control_for_test(command_id: &str, engine: &SessionEngine) {
    let hook = BEFORE_PERMISSION_CONTROL_HOOK
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|slot| slot.clone());
    if let Some(hook) = hook {
        hook(command_id, engine);
    }
}

#[cfg(not(test))]
fn before_permission_control_for_test(_command_id: &str, _engine: &SessionEngine) {}

// Internal channel payload mirrors RuntimeCommand construction. Boxing command
// variants would add indirection at every supervisor send site without changing
// the protocol boundary.
#[allow(clippy::large_enum_variant)]
enum SupervisorMessage {
    Command {
        owner: RuntimeOwner,
        command_id: String,
        command: RuntimeCommand,
        submitted_permission_epoch: Option<u64>,
    },
    ResumeContextRetrieval {
        owner_id: String,
        request_id: String,
        owner: RuntimeOwner,
        audit_id: String,
        job: Box<ContextRetrievalJob>,
    },
    ResumeProjectMutation {
        owner_id: String,
        owner: RuntimeOwner,
        command: RuntimeCommand,
        response: ApprovalResponse,
    },
    LaneApprovalResponse {
        owner: RuntimeOwner,
        command_id: String,
        request_id: String,
        response: ApprovalResponse,
    },
    Snapshot {
        response: Sender<Result<RuntimeSnapshotEnvelope, String>>,
    },
    TranscriptPage {
        request: TranscriptPageRequest,
        response: Sender<Result<TranscriptPage, String>>,
    },
    Shutdown {
        response: Option<Sender<()>>,
    },
}

/// The supervisor's narrow durable-audit handle
/// (`runtime.durable_work_evidence`, C7).
///
/// `RespondToApproval` is answered on the caller's thread while the engine is
/// owned by the worker, so the one durable write an approval decision needs
/// cannot go through `SessionEngine`. This is deliberately not a second engine
/// handle: it appends audit records and does nothing else, so the supervisor
/// gains no path to runtime state it does not already own.
///
/// The store re-validates every record on append, so a record this handle
/// cannot build is refused here rather than written malformed.
#[derive(Clone)]
struct ApprovalAuditLog {
    workflows: WorkflowStore,
}

impl ApprovalAuditLog {
    /// Appends the durable row for one approval decision.
    ///
    /// The `audit_id` is the one the request already showed the operator, so
    /// the row an `ApprovalResolved` names is the row `QueryAudit` returns —
    /// before this, that id was minted, published on two facts, and never
    /// written anywhere, which is what made the cockpit's "audit" links resolve
    /// to nothing (E1 defect 4).
    fn record_decision(
        &self,
        audit_id: &str,
        owner: &RuntimeOwner,
        request_id: &str,
        tool_name: &str,
        job_id: Option<&str>,
        decision: &ApprovalDecision,
    ) -> Result<(), String> {
        let mut objects = vec![
            AuditObjectRef::new(AuditObjectRef::KIND_PERMISSION, request_id),
            AuditObjectRef::new("tool", tool_name),
        ];
        // The active job the approval unblocks: the `command_id` the client
        // sent, which is also the turn's job key, so the row joins to the work
        // the decision released rather than only to the prompt.
        if let Some(job_id) = job_id.filter(|id| !id.is_empty()) {
            objects.push(AuditObjectRef::new("job", job_id));
        }
        let mut args = BTreeMap::new();
        args.insert(
            "scope".to_string(),
            approval_scope_key(decision).to_string(),
        );
        let record = AuditRecord::sanitized(
            audit_id.to_string(),
            now_timestamp(),
            owner.clone(),
            // An approval is answered by a human through a client surface; the
            // runtime never answers one on its own behalf.
            AuditActor::Operator,
            format!("approval.{}", approval_scope_key(decision)),
            objects,
            // The decision was applied: a denial is a decision that was carried
            // out, not an action that failed. `Denied` is reserved for an
            // action a policy refused, which this is not.
            AuditOutcome::Success,
            args,
        )?;
        self.workflows.append_audit_record(&record)
    }
}

/// Stable action/scope key for one approval decision.
fn approval_scope_key(decision: &ApprovalDecision) -> &'static str {
    match decision {
        ApprovalDecision::Allow {
            scope: ApprovalScope::Once,
        } => "allow_once",
        ApprovalDecision::Allow {
            scope: ApprovalScope::Session { .. },
        } => "allow_session",
        ApprovalDecision::Allow {
            scope: ApprovalScope::RepoAllowlist { .. },
        } => "allow_repo",
        ApprovalDecision::Deny => "deny",
    }
}

pub struct RuntimeSupervisor {
    workspace_root: PathBuf,
    commands: Sender<SupervisorMessage>,
    events: Receiver<RuntimeEventEnvelope>,
    event_bus: RuntimeEventBus,
    active_control: ActiveControlRegistry,
    pending_approvals: Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    approval_timers: Arc<ApprovalTimerRegistry>,
    lane_supervisor: Arc<LaneSupervisor>,
    permission_control: Arc<Mutex<PermissionControlState>>,
    approval_audit: ApprovalAuditLog,
    worker_alive: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl RuntimeSupervisor {
    pub fn start(engine: SessionEngine) -> Self {
        Self::start_with_approval_ttl(engine, 300)
    }

    fn start_with_approval_ttl(engine: SessionEngine, approval_ttl_secs: u64) -> Self {
        Self::start_with_effects_and_persistence(
            engine,
            approval_ttl_secs,
            Arc::new(LocalLaneEffectExecutor::default()),
            None,
        )
    }

    #[cfg(test)]
    fn start_with_effects(
        engine: SessionEngine,
        approval_ttl_secs: u64,
        lane_effects: Arc<dyn LaneEffectExecutor>,
    ) -> Self {
        Self::start_with_effects_and_persistence(engine, approval_ttl_secs, lane_effects, None)
    }

    fn start_with_effects_and_persistence(
        mut engine: SessionEngine,
        approval_ttl_secs: u64,
        lane_effects: Arc<dyn LaneEffectExecutor>,
        lane_persistence: Option<Arc<dyn LanePersistence>>,
    ) -> Self {
        let workspace_root = engine.cwd().to_path_buf();
        let permission_control = Arc::new(Mutex::new(PermissionControlState::new(
            engine.work_mode(),
            engine.permission_level(),
        )));
        let (command_sender, command_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::channel();
        let active_control: ActiveControlRegistry = Arc::new(Mutex::new(BTreeMap::new()));
        let pending_approvals = Arc::new(Mutex::new(BTreeMap::new()));
        let approval_timers = Arc::new(ApprovalTimerRegistry::default());
        let worker_alive = Arc::new(AtomicBool::new(true));
        let live_view = engine.runtime_view_state();
        let lane_agent_store = engine.workflow_store();
        // Cloned before the engine moves onto the worker thread: the handle the
        // approval decision's durable row is written through.
        let approval_audit = ApprovalAuditLog {
            workflows: engine.workflow_store(),
        };
        let mut lane_agent_hydration_error = None;
        let mut lane_agent_bindings = match lane_agent_store.load_lane_agent_bindings() {
            Ok(bindings) => bindings
                .values()
                .map(|binding| {
                    (
                        binding.lane_id.clone(),
                        LaneAgentExecutionBinding {
                            lane_id: binding.lane_id.clone(),
                            agent_id: binding.agent_id.clone(),
                            session_id: binding.session_id.clone(),
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>(),
            Err(error) => {
                lane_agent_hydration_error =
                    Some(format!("failed to hydrate Lane-agent bindings: {error}"));
                BTreeMap::new()
            }
        };
        for session in &live_view.agent_sessions {
            let recovered = LaneAgentExecutionBinding {
                lane_id: session.lane_id.clone(),
                agent_id: session.agent_id.clone(),
                session_id: session.session_id.clone(),
            };
            if let Some(existing) = lane_agent_bindings.get(&session.lane_id) {
                if existing.agent_id != recovered.agent_id
                    || existing.session_id != recovered.session_id
                {
                    lane_agent_hydration_error = Some(format!(
                        "conflicting durable Lane-agent bindings for lane `{}`: agent `{}` session `{}` versus agent `{}` session `{}`",
                        session.lane_id,
                        existing.agent_id,
                        existing.session_id,
                        recovered.agent_id,
                        recovered.session_id
                    ));
                }
            } else {
                lane_agent_bindings.insert(session.lane_id.clone(), recovered);
            }
        }
        let event_bus = RuntimeEventBus {
            sender: event_sender,
            state: Arc::new(Mutex::new(RuntimeEventState {
                journal: RuntimeEventJournal::default_with_stream(fresh_id("runtime-stream")),
                live_view,
                lane_agent_bindings,
                lane_agent_store: Some(lane_agent_store),
            })),
        };
        emit_frontend_status_events_if_changed(
            &event_bus,
            RuntimeOwner::default(),
            engine.frontend_status_lifecycle_events(),
        );
        if let Some(error) = lane_agent_hydration_error {
            emit_error(&event_bus, RuntimeOwner::default(), error);
        }

        let lane_repo = engine.cwd().to_path_buf();
        let lane_persistence = lane_persistence.unwrap_or_else(|| {
            Arc::new(WorkflowLanePersistence(engine.workflow_store())) as Arc<dyn LanePersistence>
        });
        let lane_permissions = Arc::new(Mutex::new(engine.lane_permission_engine()));
        let lane_event_bus = event_bus.clone();
        let lane_events = Arc::new(move |owner: RuntimeOwner, kind: RuntimeEventKind| {
            // A Lane's first source row is published here, at the moment its
            // worktree is announced. The periodic sampling in
            // `frontend_status_lifecycle_events` runs at connect, at every
            // snapshot, and after every completed supervised command, but Lane
            // creation is dispatched to an asynchronous Lane worker and reaches
            // none of those — so without this hook a freshly created Lane would
            // show no branch until something unrelated happened.
            let unsampled = first_lane_worktree_to_sample(&lane_event_bus, &kind);
            emit_event(&lane_event_bus, owner.clone(), kind);
            if let Some((lane_id, worktree)) = unsampled {
                emit_event(
                    &lane_event_bus,
                    owner,
                    RuntimeEventKind::LaneSourceUpdated {
                        lane_id,
                        source: crate::frontend_status::sample_workspace_source(&worktree),
                    },
                );
            }
        });
        let lane_mode_state = Arc::clone(&event_bus.state);
        let lane_mode = Arc::new(move || {
            lane_mode_state
                .lock()
                .map(|state| state.live_view.snapshot.work_mode)
                .unwrap_or(viden_types::WorkMode::Plan)
        });
        // The lane subsystem re-validates queued approvals and redacts the
        // commands it announces, but neither policy belongs to it: both are
        // injected here so the shared permission gate and the event redaction
        // contract stay owned by the runtime.
        let lane_approvals: LaneApprovalResolver =
            Arc::new(|permissions, tool, input, response: ApprovalResponse| {
                crate::permission_gate::resolve(
                    permissions,
                    tool,
                    &tool.name,
                    input,
                    |_ask, _prompt| response.clone(),
                )
            });
        let lane_redact_command: LaneCommandRedactor = Arc::new(redacted_runtime_command_for_event);
        let lane_supervisor = Arc::new(LaneSupervisor::new(
            lane_repo,
            lane_persistence,
            lane_permissions,
            lane_effects,
            lane_events,
            lane_approvals,
            lane_redact_command,
            lane_mode,
            approval_ttl_secs,
        ));
        for lane in lane_supervisor.hydration_recoveries() {
            let owner = RuntimeOwner {
                lane_id: Some(lane.id.clone()),
                session_id: lane.active_session_ids.first().cloned(),
                ..RuntimeOwner::default()
            };
            emit_event(
                &event_bus,
                owner.clone(),
                RuntimeEventKind::LaneUpdated { lane: lane.clone() },
            );
            emit_event(
                &event_bus,
                owner,
                RuntimeEventKind::LaneRecoveryRequired {
                    lane_id: lane.id.clone(),
                    reason: lane.summary.clone(),
                    next_action: "inspect the interrupted lane and explicitly resume or reconcile"
                        .to_string(),
                },
            );
        }

        install_runtime_event_sink(&mut engine, event_bus.clone(), RuntimeOwner::default());

        let worker_event_bus = event_bus.clone();
        let worker_active_control = Arc::clone(&active_control);
        let worker_pending_approvals = Arc::clone(&pending_approvals);
        let worker_approval_timers = Arc::clone(&approval_timers);
        let worker_lane_supervisor = Arc::clone(&lane_supervisor);
        let worker_permission_control = Arc::clone(&permission_control);
        let worker_liveness = Arc::clone(&worker_alive);
        let worker = thread::spawn(move || {
            run_supervisor_worker(
                engine,
                command_receiver,
                worker_event_bus,
                worker_active_control,
                worker_pending_approvals,
                worker_approval_timers,
                worker_lane_supervisor,
                worker_permission_control,
                approval_ttl_secs,
                worker_liveness,
            );
        });

        Self {
            workspace_root,
            commands: command_sender,
            events: event_receiver,
            event_bus,
            active_control,
            pending_approvals,
            approval_timers,
            lane_supervisor,
            permission_control,
            approval_audit,
            worker_alive,
            worker: Some(worker),
        }
    }

    #[cfg(test)]
    pub(crate) fn start_with_approval_ttl_for_test(
        engine: SessionEngine,
        approval_ttl_secs: u64,
    ) -> Self {
        Self::start_with_approval_ttl(engine, approval_ttl_secs)
    }

    #[cfg(test)]
    pub(crate) fn start_with_lane_effects_for_test(
        engine: SessionEngine,
        lane_effects: Arc<dyn LaneEffectExecutor>,
    ) -> Self {
        Self::start_with_effects(engine, 300, lane_effects)
    }

    #[cfg(test)]
    pub(crate) fn start_with_lane_effects_and_approval_ttl_for_test(
        engine: SessionEngine,
        lane_effects: Arc<dyn LaneEffectExecutor>,
        approval_ttl_secs: u64,
    ) -> Self {
        Self::start_with_effects(engine, approval_ttl_secs, lane_effects)
    }

    #[cfg(test)]
    pub(crate) fn start_with_lane_effects_and_persistence_for_test(
        engine: SessionEngine,
        lane_effects: Arc<dyn LaneEffectExecutor>,
        lane_persistence: Arc<dyn LanePersistence>,
    ) -> Self {
        Self::start_with_effects_and_persistence(engine, 300, lane_effects, Some(lane_persistence))
    }

    #[cfg(test)]
    pub(crate) fn lane_worker_finished_for_test(&self, lane_id: &str) -> bool {
        self.lane_supervisor.worker_finished_for_test(lane_id)
    }

    #[cfg(test)]
    pub(crate) fn lane_worker_retired_for_test(&self, lane_id: &str) -> bool {
        self.lane_supervisor.worker_retired_for_test(lane_id)
    }

    #[cfg(test)]
    pub(crate) fn active_lane_worker_count_for_test(&self) -> usize {
        self.lane_supervisor.active_worker_count_for_test()
    }

    #[cfg(test)]
    pub(crate) fn lane_permission_snapshot_for_test(
        &self,
        lane_id: &str,
    ) -> Result<(viden_types::PermissionMode, u64), String> {
        self.lane_supervisor
            .lane_permission_snapshot_for_test(lane_id)
    }

    #[cfg(test)]
    pub(crate) fn lane_permission_template_snapshot_for_test(
        &self,
    ) -> Result<(viden_types::PermissionMode, u64), String> {
        self.lane_supervisor.permission_template_snapshot_for_test()
    }

    #[cfg(test)]
    pub(crate) fn permission_control_state_for_test(
        &self,
    ) -> (u64, u64, u64, Vec<u64>, WorkMode, PermissionLevel) {
        let control = self
            .permission_control
            .lock()
            .expect("permission control state");
        (
            control.applied_epoch,
            control.epoch(),
            control.next_epoch,
            control
                .submitted
                .iter()
                .map(|submitted| submitted.epoch)
                .collect(),
            control.applied.work_mode,
            control.applied.permission_level,
        )
    }

    pub fn send_command(
        &self,
        command_id: impl Into<String>,
        command: RuntimeCommand,
    ) -> Result<(), String> {
        self.send_command_inner(RuntimeOwner::default(), command_id.into(), command)
    }

    pub fn send_command_from_owner(
        &self,
        owner: RuntimeOwner,
        command_id: impl Into<String>,
        command: RuntimeCommand,
    ) -> Result<(), String> {
        let command_id = command_id.into();
        self.send_command_inner(owner, command_id, command)
    }

    fn send_command_inner(
        &self,
        owner: RuntimeOwner,
        command_id: String,
        command: RuntimeCommand,
    ) -> Result<(), String> {
        let command = if let RuntimeCommand::SubmitUserInput { content } = &command {
            match typed_agent_session_request_from_compat_input(content, owner.lane_id.as_deref()) {
                Some(Ok(request)) => RuntimeCommand::StartAgentSession { request },
                Some(Err(reason)) => {
                    emit_event(
                        &self.event_bus,
                        owner,
                        RuntimeEventKind::CommandRejected { command_id, reason },
                    );
                    return Ok(());
                }
                None => command,
            }
        } else {
            command
        };
        if matches!(command, RuntimeCommand::SubmitUserInput { .. })
            && let Some(binding) = lane_agent_session_binding(&self.event_bus, &owner)?
            && binding.agent_id != "viden"
        {
            emit_event(
                &self.event_bus,
                owner,
                RuntimeEventKind::CommandRejected {
                    command_id,
                    reason: lane_agent_session_binding_rejection(&binding),
                },
            );
            return Ok(());
        }
        let submitted_permission_epoch = if matches!(
            command,
            RuntimeCommand::SetWorkMode { .. } | RuntimeCommand::SetPermissionLevel { .. }
        ) {
            Some(
                self.permission_control
                    .lock()
                    .map_err(|_| "permission control state poisoned".to_string())?
                    .reserve(&command),
            )
        } else {
            None
        };
        let continuation_session_id = match &command {
            RuntimeCommand::SendAgentSessionInput { input } => Some(input.session_id.as_str()),
            RuntimeCommand::RetryAgentSession { session_id } => Some(session_id.as_str()),
            _ => None,
        };
        if let Some(session_id) = continuation_session_id {
            let session = self
                .event_bus
                .state
                .lock()
                .map_err(|_| "runtime event state poisoned".to_string())?
                .live_view
                .agent_sessions
                .iter()
                .find(|session| session.session_id == session_id)
                .cloned();
            let Some(session) = session else {
                emit_event(
                    &self.event_bus,
                    owner,
                    RuntimeEventKind::CommandRejected {
                        command_id,
                        reason: format!("agent session `{session_id}` is not known"),
                    },
                );
                return Ok(());
            };
            if session.owner != owner {
                emit_event(
                    &self.event_bus,
                    owner,
                    RuntimeEventKind::CommandRejected {
                        command_id,
                        reason: "agent_session_owner_mismatch".to_string(),
                    },
                );
                return Ok(());
            }
            return self
                .commands
                .send(SupervisorMessage::Command {
                    owner,
                    command_id,
                    command,
                    submitted_permission_epoch,
                })
                .map_err(|err| format!("runtime supervisor stopped: {err}"));
        }
        let result = match command {
            RuntimeCommand::StartAgentSession { ref request }
                if owner.lane_id.as_deref() != Some(request.lane_id.as_str()) =>
            {
                emit_event(
                    &self.event_bus,
                    owner,
                    RuntimeEventKind::CommandRejected {
                        command_id,
                        reason: "agent session request lane does not match command owner"
                            .to_string(),
                    },
                );
                return Ok(());
            }
            RuntimeCommand::CancelAgentSession { ref session_id } => {
                let session = self
                    .event_bus
                    .state
                    .lock()
                    .map_err(|_| "runtime event state poisoned".to_string())?
                    .live_view
                    .agent_sessions
                    .iter()
                    .find(|session| &session.session_id == session_id)
                    .cloned();
                let Some(session) = session else {
                    emit_event(
                        &self.event_bus,
                        owner,
                        RuntimeEventKind::CommandRejected {
                            command_id,
                            reason: format!("agent session `{session_id}` is not known"),
                        },
                    );
                    return Ok(());
                };
                if session.owner != owner {
                    emit_event(
                        &self.event_bus,
                        owner,
                        RuntimeEventKind::CommandRejected {
                            command_id,
                            reason: format!("agent session `{session_id}` owner mismatch"),
                        },
                    );
                    return Ok(());
                }
                self.commands
                    .send(SupervisorMessage::Command {
                        owner,
                        command_id,
                        command,
                        submitted_permission_epoch,
                    })
                    .map_err(|err| format!("runtime supervisor stopped: {err}"))
            }
            RuntimeCommand::StartAgentSession { .. } => {
                if let Some(binding) = lane_agent_session_binding(&self.event_bus, &owner)? {
                    emit_event(
                        &self.event_bus,
                        owner,
                        RuntimeEventKind::CommandRejected {
                            command_id,
                            reason: lane_agent_session_binding_rejection(&binding),
                        },
                    );
                    return Ok(());
                }
                if let Some(owner_id) = active_agent_lane_owner_id(&self.active_control, &owner) {
                    emit_event(
                        &self.event_bus,
                        owner,
                        RuntimeEventKind::CommandRejected {
                            command_id,
                            reason: format!(
                                "active agent session `{owner_id}` is already running for this lane"
                            ),
                        },
                    );
                    return Ok(());
                }
                self.commands
                    .send(SupervisorMessage::Command {
                        owner,
                        command_id,
                        command,
                        submitted_permission_epoch,
                    })
                    .map_err(|err| format!("runtime supervisor stopped: {err}"))
            }
            RuntimeCommand::CancelActiveTurn => {
                let controls = self
                    .active_control
                    .lock()
                    .map_err(|_| "active turn lock poisoned".to_string())?;
                let control = controls.get(&RuntimeOwnerKey::from(&owner)).cloned();
                let another_owner_is_active = control.is_none() && !controls.is_empty();
                drop(controls);
                if another_owner_is_active {
                    emit_event(
                        &self.event_bus,
                        owner.clone(),
                        RuntimeEventKind::CommandRejected {
                            command_id,
                            reason: "active runtime job owner mismatch".to_string(),
                        },
                    );
                    return Ok(());
                }
                if let Some(control) = control {
                    if control.owner != owner {
                        emit_event(
                            &self.event_bus,
                            owner.clone(),
                            RuntimeEventKind::CommandRejected {
                                command_id,
                                reason: format!(
                                    "active runtime job `{}` owner mismatch",
                                    control.owner_id
                                ),
                            },
                        );
                        return Ok(());
                    }
                    control.control.cancel();
                    if let ActiveJobState::PendingApproval { request_id } = &control.state {
                        resolve_pending_approval_by_id(
                            request_id,
                            ApprovalDecision::Deny,
                            &self.event_bus,
                            &self.active_control,
                            &self.pending_approvals,
                            None,
                        );
                    }
                    emit_event(
                        &self.event_bus,
                        owner.clone(),
                        RuntimeEventKind::CommandAccepted {
                            command_id,
                            command: redacted_runtime_command_for_event(
                                &RuntimeCommand::CancelActiveTurn,
                            ),
                        },
                    );
                    return Ok(());
                }
                if self.lane_supervisor.cancel(&owner, command_id.clone())? {
                    return Ok(());
                }
                emit_event(
                    &self.event_bus,
                    owner.clone(),
                    RuntimeEventKind::CommandRejected {
                        command_id,
                        reason: "no active turn to cancel".to_string(),
                    },
                );
                Ok(())
            }
            RuntimeCommand::RespondToApproval {
                request_id,
                response,
            } => {
                if let Some(pending_owner) =
                    self.lane_supervisor.pending_approval_owner(&request_id)?
                {
                    if pending_owner != owner {
                        emit_event(
                            &self.event_bus,
                            owner,
                            RuntimeEventKind::CommandRejected {
                                command_id,
                                reason: format!("approval request `{request_id}` owner mismatch"),
                            },
                        );
                        return Ok(());
                    }
                    self.commands
                        .send(SupervisorMessage::LaneApprovalResponse {
                            owner,
                            command_id,
                            request_id,
                            response,
                        })
                        .map_err(|err| format!("runtime supervisor stopped: {err}"))?;
                    return Ok(());
                }
                let pending = {
                    let mut approvals = self
                        .pending_approvals
                        .lock()
                        .map_err(|_| "approval lock poisoned".to_string())?;
                    let Some(pending) = approvals.get(&request_id) else {
                        emit_event(
                            &self.event_bus,
                            owner.clone(),
                            RuntimeEventKind::CommandRejected {
                                command_id,
                                reason: format!("approval request `{request_id}` is not pending"),
                            },
                        );
                        return Ok(());
                    };
                    if pending.owner != owner {
                        emit_event(
                            &self.event_bus,
                            owner.clone(),
                            RuntimeEventKind::CommandRejected {
                                command_id,
                                reason: format!("approval request `{request_id}` owner mismatch"),
                            },
                        );
                        return Ok(());
                    }
                    if !approval_decision_is_allowed_by_request(&response.decision, pending) {
                        emit_event(
                            &self.event_bus,
                            owner.clone(),
                            RuntimeEventKind::CommandRejected {
                                command_id,
                                reason: format!(
                                    "approval request `{request_id}` scope is not allowed"
                                ),
                            },
                        );
                        return Ok(());
                    }
                    approvals.remove(&request_id).expect("pending approval")
                };
                let permission_control = self
                    .permission_control
                    .lock()
                    .map_err(|_| "permission control state poisoned".to_string())?;
                let stale_permission_epoch = pending.permission_epoch != permission_control.epoch();
                let response = if response.is_allowed()
                    && (stale_permission_epoch
                        || (matches!(
                            &pending.target,
                            PendingApprovalTarget::Channel { .. }
                                | PendingApprovalTarget::ProjectMutation { .. }
                        ) && permission_control.blocks_mutation()))
                {
                    ApprovalResponse::deny(Some(
                        if stale_permission_epoch {
                            "permission or work mode changed after approval was requested"
                        } else {
                            "permission or work mode changed before approval resumed"
                        }
                        .to_string(),
                    ))
                } else {
                    response
                };
                emit_event(
                    &self.event_bus,
                    owner.clone(),
                    RuntimeEventKind::CommandAccepted {
                        command_id,
                        command: redacted_runtime_command_for_event(
                            &RuntimeCommand::RespondToApproval {
                                request_id: request_id.clone(),
                                response: response.clone(),
                            },
                        ),
                    },
                );
                let pending_owner = pending.owner.clone();
                let pending_audit_id = pending.audit_id.clone();
                // Audit before the fact that announces it, the same order every
                // other mutation in the trust loop uses: `ApprovalResolved`
                // carries this `audit_id`, and publishing it before the row
                // existed would give a client a receipt it could not resolve.
                //
                // Fail-open on the decision, not on the audit: refusing to
                // deliver an operator's answer because the audit log could not
                // be appended would leave a tool call blocked on an approval
                // that has already been given. The failure is published as an
                // `Error` so it is visible rather than swallowed.
                if let Err(error) = self.approval_audit.record_decision(
                    &pending_audit_id,
                    &pending_owner,
                    &request_id,
                    &pending.tool_name,
                    pending.target.owner_id(),
                    &response.decision,
                ) {
                    emit_error(
                        &self.event_bus,
                        pending_owner.clone(),
                        format!("approval decision was applied but not audited: {error}"),
                    );
                }
                match pending.target {
                    PendingApprovalTarget::Channel { owner_id, sender } => {
                        if response.is_allowed() {
                            let _ = mark_active_running(&self.active_control, &owner_id);
                        }
                        emit_event(
                            &self.event_bus,
                            pending_owner.clone(),
                            approval_resolved_event(
                                &request_id,
                                response.decision.clone(),
                                pending_owner,
                                pending_audit_id,
                            ),
                        );
                        sender
                            .send(response.clone())
                            .map_err(|err| format!("failed to send approval response: {err}"))?;
                    }
                    PendingApprovalTarget::ContextRetrieval { owner_id, job } => {
                        if response.is_allowed() {
                            let mut job = *job;
                            job.permission_decision = "approved".to_string();
                            if let Err(err) = mark_active_running(&self.active_control, &owner_id) {
                                emit_error(&self.event_bus, pending_owner.clone(), err);
                                return Ok(());
                            }
                            let Some(control) =
                                active_control_for_owner(&self.active_control, &owner_id)
                            else {
                                emit_error(
                                    &self.event_bus,
                                    pending_owner.clone(),
                                    format!("active runtime job `{owner_id}` is no longer running"),
                                );
                                return Ok(());
                            };
                            emit_event(
                                &self.event_bus,
                                pending_owner.clone(),
                                approval_resolved_event(
                                    &request_id,
                                    response.decision.clone(),
                                    pending_owner.clone(),
                                    pending_audit_id.clone(),
                                ),
                            );
                            before_context_resume_enqueue_for_test(&control);
                            if let Err(err) =
                                self.commands
                                    .send(SupervisorMessage::ResumeContextRetrieval {
                                        owner_id: owner_id.clone(),
                                        request_id,
                                        owner: pending_owner.clone(),
                                        audit_id: pending_audit_id.clone(),
                                        job: Box::new(job),
                                    })
                            {
                                clear_active_control(&self.active_control, &owner_id);
                                emit_error(
                                    &self.event_bus,
                                    pending_owner.clone(),
                                    format!("runtime supervisor stopped: {err}"),
                                );
                            }
                        } else {
                            emit_event(
                                &self.event_bus,
                                pending_owner.clone(),
                                approval_resolved_event(
                                    &request_id,
                                    response.decision.clone(),
                                    pending_owner.clone(),
                                    pending_audit_id,
                                ),
                            );
                            emit_error(
                                &self.event_bus,
                                pending_owner.clone(),
                                "User denied the permission request".to_string(),
                            );
                            clear_active_control(&self.active_control, &owner_id);
                        }
                    }
                    PendingApprovalTarget::ProjectMutation { owner_id, command } => {
                        emit_event(
                            &self.event_bus,
                            pending_owner.clone(),
                            approval_resolved_event(
                                &request_id,
                                response.decision.clone(),
                                pending_owner.clone(),
                                pending_audit_id,
                            ),
                        );
                        if response.is_allowed() {
                            if let Err(err) = mark_active_running(&self.active_control, &owner_id) {
                                emit_error(&self.event_bus, pending_owner, err);
                                return Ok(());
                            }
                            if let Err(err) =
                                self.commands
                                    .send(SupervisorMessage::ResumeProjectMutation {
                                        owner_id: owner_id.clone(),
                                        owner: pending_owner.clone(),
                                        command: *command,
                                        response,
                                    })
                            {
                                clear_active_control(&self.active_control, &owner_id);
                                emit_error(
                                    &self.event_bus,
                                    pending_owner,
                                    format!("runtime supervisor stopped: {err}"),
                                );
                            }
                        } else {
                            emit_error(
                                &self.event_bus,
                                pending_owner,
                                "User denied the permission request".to_string(),
                            );
                            clear_active_control(&self.active_control, &owner_id);
                        }
                    }
                }
                Ok(())
            }
            RuntimeCommand::SubmitUserInput { .. }
            | RuntimeCommand::StartAgentTask { .. }
            | RuntimeCommand::RetrieveContext { .. } => {
                if let Some(owner_id) = active_owner_id(&self.active_control, &owner) {
                    emit_event(
                        &self.event_bus,
                        owner.clone(),
                        RuntimeEventKind::CommandRejected {
                            command_id,
                            reason: format!("active runtime job `{owner_id}` is already running"),
                        },
                    );
                    return Ok(());
                }
                self.commands
                    .send(SupervisorMessage::Command {
                        owner,
                        command_id,
                        command,
                        submitted_permission_epoch,
                    })
                    .map_err(|err| format!("runtime supervisor stopped: {err}"))
            }
            command => self
                .commands
                .send(SupervisorMessage::Command {
                    owner,
                    command_id,
                    command,
                    submitted_permission_epoch,
                })
                .map_err(|err| format!("runtime supervisor stopped: {err}")),
        };
        if result.is_err()
            && let Some(epoch) = submitted_permission_epoch
            && let Ok(mut control) = self.permission_control.lock()
        {
            control.reject(epoch);
        }
        result
    }

    pub fn recv_event_timeout(&self, timeout: Duration) -> Option<RuntimeEvent> {
        self.recv_event_envelope_timeout(timeout)
            .and_then(|envelope| match envelope.event {
                RuntimeWireEvent::Known(event) => Some(event),
                RuntimeWireEvent::Unknown { .. } => None,
            })
    }

    pub fn send_command_envelope(&self, envelope: RuntimeCommandEnvelope) -> Result<(), String> {
        if envelope.schema_version != FRONTEND_SCHEMA_V1 {
            return Err(format!(
                "unsupported frontend schema {}",
                envelope.schema_version.0
            ));
        }
        self.send_command_from_owner(envelope.owner, envelope.command_id, envelope.command)
    }

    pub fn recv_event_envelope(
        &self,
        timeout: Duration,
    ) -> Result<Option<RuntimeEventEnvelope>, String> {
        match self.events.try_recv() {
            Ok(envelope) => return Ok(Some(envelope)),
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err("runtime supervisor event stream stopped".to_string());
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if !self.worker_alive.load(Ordering::Acquire) {
            return Err("runtime supervisor event stream stopped".to_string());
        }

        match self.events.recv_timeout(timeout) {
            Ok(envelope) => Ok(Some(envelope)),
            Err(mpsc::RecvTimeoutError::Timeout) if self.worker_alive.load(Ordering::Acquire) => {
                Ok(None)
            }
            Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => {
                Err("runtime supervisor event stream stopped".to_string())
            }
        }
    }

    pub fn recv_event_envelope_timeout(&self, timeout: Duration) -> Option<RuntimeEventEnvelope> {
        self.recv_event_envelope(timeout).ok().flatten()
    }

    pub fn snapshot_envelope(&self) -> Result<RuntimeSnapshotEnvelope, String> {
        let (response, receiver) = mpsc::channel();
        self.commands
            .send(SupervisorMessage::Snapshot { response })
            .map_err(|err| format!("runtime supervisor stopped: {err}"))?;
        receiver
            .recv()
            .map_err(|err| format!("runtime supervisor snapshot failed: {err}"))?
    }

    pub fn replay_events(&self, request: ReplayRequest) -> Result<ReplayBatch, GapRecovery> {
        self.event_bus
            .state
            .lock()
            .map_err(|_| GapRecovery::SnapshotRequired {
                reason_code: "journal_unavailable".to_string(),
            })?
            .journal
            .replay(request)
    }

    pub fn load_transcript_page(
        &self,
        request: TranscriptPageRequest,
    ) -> Result<TranscriptPage, String> {
        let (response, receiver) = mpsc::channel();
        self.commands
            .send(SupervisorMessage::TranscriptPage { request, response })
            .map_err(|err| format!("runtime supervisor stopped: {err}"))?;
        receiver
            .recv()
            .map_err(|err| format!("runtime supervisor transcript page failed: {err}"))?
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn expire_pending_approvals_for_test(&self, now: u64) {
        expire_pending_approvals_at(
            now,
            &self.event_bus,
            &self.active_control,
            &self.pending_approvals,
        );
    }

    #[cfg(test)]
    pub(crate) fn stop_worker_for_test(&self) {
        let (response, receiver) = mpsc::channel();
        self.commands
            .send(SupervisorMessage::Shutdown {
                response: Some(response),
            })
            .expect("runtime supervisor test shutdown");
        receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("runtime supervisor worker stopped");
    }
}

impl Drop for RuntimeSupervisor {
    fn drop(&mut self) {
        if let Ok(controls) = self.active_control.lock() {
            for active in controls.values() {
                active.control.cancel();
            }
        }
        let pending_ids = self
            .pending_approvals
            .lock()
            .map(|pending| pending.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for request_id in pending_ids {
            resolve_pending_approval_by_id(
                &request_id,
                ApprovalDecision::Deny,
                &self.event_bus,
                &self.active_control,
                &self.pending_approvals,
                Some("runtime supervisor shutting down".to_string()),
            );
        }
        let _ = self
            .commands
            .send(SupervisorMessage::Shutdown { response: None });
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.approval_timers.shutdown_and_join();
        shutdown_resident_acp_sessions(&self.workspace_root);
    }
}

#[allow(clippy::too_many_arguments)]
fn run_supervisor_worker(
    mut engine: SessionEngine,
    command_receiver: Receiver<SupervisorMessage>,
    event_bus: RuntimeEventBus,
    active_control: ActiveControlRegistry,
    pending_approvals: Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    approval_timers: Arc<ApprovalTimerRegistry>,
    lane_supervisor: Arc<LaneSupervisor>,
    permission_control: Arc<Mutex<PermissionControlState>>,
    approval_ttl_secs: u64,
    worker_alive: Arc<AtomicBool>,
) {
    let _liveness = WorkerLivenessGuard(Arc::clone(&worker_alive));
    // Submitted permission generations invalidate ordinary pending approvals
    // immediately. Lane approvals instead observe this worker-owned generation,
    // which advances only with the SessionEngine state it describes.
    let mut applied_permission_epoch = 0_u64;
    // Whether the session queue is draining right now.
    //
    // The supervisor is one worker, so a `QueueFollowUp` a client sends *while*
    // a turn runs is still in this channel when that turn ends: the drain
    // inside `run_supervised_input` cannot see it, and a rule that only drained
    // at the instant a turn completed would leave the common case — queue, then
    // the turn finishes — sitting forever, which is compatibility follow-up 5
    // in a new shape. So a completed session-scoped turn *arms* the drain and a
    // turn that does not complete disarms it, and a follow-up that lands while
    // it is armed runs as soon as it is queued. Nothing runs behind a failed or
    // cancelled turn either way.
    let mut session_queue_draining = false;
    while let Ok(message) = command_receiver.recv() {
        match message {
            SupervisorMessage::Shutdown { response } => {
                worker_alive.store(false, Ordering::Release);
                if let Some(response) = response {
                    let _ = response.send(());
                }
                break;
            }
            SupervisorMessage::Command {
                owner,
                command_id,
                command,
                submitted_permission_epoch,
            } => {
                // Every new binding this command creates — a Lane worker, an
                // Agent session — starts from the envelope owner, so the
                // workspace identity is folded in here, once, rather than at
                // each producer.
                let owner = stamp_workspace_identity(&engine, owner, &command);
                let queued_follow_up = matches!(command, RuntimeCommand::QueueFollowUp { .. });
                let queue_owner = owner.clone();
                before_supervisor_command_for_test(&command_id);
                if LaneSupervisor::handles(&command) {
                    if let Err(error) =
                        sync_lane_permissions(&lane_supervisor, &engine, applied_permission_epoch)
                    {
                        emit_error(&event_bus, owner, error);
                        continue;
                    }
                    if let Err(error) = lane_supervisor.send(owner.clone(), command_id, command) {
                        emit_error(&event_bus, owner, error);
                    }
                    continue;
                }
                // Background engine work clones the installed sink, so bind
                // owner identity at command dispatch rather than consulting a
                // later active-owner value when the event finally arrives.
                install_runtime_event_sink(&mut engine, event_bus.clone(), owner.clone());
                match command {
                    RuntimeCommand::SubmitUserInput { content } => {
                        let session_scoped = owner.lane_id.is_none();
                        let draining = run_supervised_input(
                            &mut engine,
                            owner,
                            command_id,
                            content,
                            &event_bus,
                            &active_control,
                            &pending_approvals,
                            &approval_timers,
                            &permission_control,
                            approval_ttl_secs,
                        );
                        // A Lane's turn says nothing about the session queue,
                        // so it must not arm or disarm it.
                        if session_scoped {
                            session_queue_draining = draining;
                        }
                    }
                    RuntimeCommand::StartAgentTask { task_id } => {
                        run_supervised_agent_task(
                            &mut engine,
                            owner,
                            command_id,
                            task_id,
                            &event_bus,
                            &active_control,
                            &pending_approvals,
                            &approval_timers,
                            &permission_control,
                            approval_ttl_secs,
                        );
                    }
                    RuntimeCommand::StartAgentSession { request } => {
                        run_supervised_agent_session(
                            &engine,
                            owner,
                            command_id,
                            request,
                            &event_bus,
                            &active_control,
                            &pending_approvals,
                            &approval_timers,
                            &permission_control,
                            approval_ttl_secs,
                        );
                    }
                    RuntimeCommand::SendAgentSessionInput { input } => {
                        run_supervised_agent_session_continuation(
                            &engine,
                            owner,
                            command_id,
                            input.session_id,
                            Some(input.content),
                            &event_bus,
                            &active_control,
                            &pending_approvals,
                            &approval_timers,
                            &permission_control,
                            approval_ttl_secs,
                        );
                    }
                    RuntimeCommand::RetryAgentSession { session_id } => {
                        run_supervised_agent_session_continuation(
                            &engine,
                            owner,
                            command_id,
                            session_id,
                            None,
                            &event_bus,
                            &active_control,
                            &pending_approvals,
                            &approval_timers,
                            &permission_control,
                            approval_ttl_secs,
                        );
                    }
                    RuntimeCommand::CancelAgentSession { session_id } => {
                        run_supervised_agent_session_cancel(
                            &engine,
                            owner,
                            command_id,
                            session_id,
                            &event_bus,
                            &active_control,
                            &pending_approvals,
                        );
                    }
                    RuntimeCommand::RetrieveContext { handle_id, reason } => {
                        run_supervised_context_retrieval(
                            &mut engine,
                            owner,
                            command_id,
                            handle_id,
                            reason,
                            SupervisorShared {
                                event_bus: &event_bus,
                                active_control: &active_control,
                                pending_approvals: &pending_approvals,
                                approval_timers: &approval_timers,
                                permission_control: &permission_control,
                            },
                            approval_ttl_secs,
                        );
                    }
                    command @ (RuntimeCommand::ConfirmProjectConfig { .. }
                    | RuntimeCommand::StoreCredentialHandle { .. }
                    | RuntimeCommand::SetUiPreferences { .. }
                    | RuntimeCommand::ResetUiPreferences
                    | RuntimeCommand::CreateHandoff { .. }
                    | RuntimeCommand::RequestReview { .. }
                    | RuntimeCommand::DecideReview { .. }
                    | RuntimeCommand::ConfirmContract { .. }
                    | RuntimeCommand::SetDependency { .. }
                    | RuntimeCommand::AcceptMergeGate { .. }
                    | RuntimeCommand::RejectMergeGate { .. }
                    | RuntimeCommand::RecordAgentEvidence { .. }
                    | RuntimeCommand::AcceptAgentArtifact { .. }
                    | RuntimeCommand::RejectAgentArtifact { .. }
                    | RuntimeCommand::MergeAgentPatch { .. }
                    | RuntimeCommand::RevalidateMergeConflict { .. }
                    | RuntimeCommand::BounceMergeConflict { .. }
                    | RuntimeCommand::RevertAppliedChange { .. }
                    | RuntimeCommand::RunOperatorGitAction { .. }) => {
                        run_supervised_project_mutation(
                            &mut engine,
                            owner,
                            command_id,
                            command,
                            SupervisorShared {
                                event_bus: &event_bus,
                                active_control: &active_control,
                                pending_approvals: &pending_approvals,
                                approval_timers: &approval_timers,
                                permission_control: &permission_control,
                            },
                            approval_ttl_secs,
                        );
                    }
                    command => {
                        if submitted_permission_epoch.is_some() {
                            before_permission_control_for_test(&command_id, &engine);
                        }
                        let mut approver = |_prompt: PermissionPrompt| {
                            ApprovalResponse::deny(Some(
                                "runtime supervisor command path does not own this approval"
                                    .to_string(),
                            ))
                        };
                        match engine.handle_runtime_command(command_id, command, &mut approver) {
                            Ok(events) => {
                                if let Some(epoch) = submitted_permission_epoch {
                                    if let Err(error) = permission_control
                                        .lock()
                                        .map_err(|_| {
                                            "permission control state poisoned".to_string()
                                        })
                                        .and_then(|mut control| control.commit(epoch))
                                    {
                                        emit_error(&event_bus, owner, error);
                                        continue;
                                    }
                                    applied_permission_epoch = epoch;
                                }
                                emit_events(&event_bus, owner, events);
                            }
                            Err(err) => {
                                if let Some(epoch) = submitted_permission_epoch
                                    && let Ok(mut control) = permission_control.lock()
                                {
                                    control.reject(epoch);
                                }
                                emit_error(&event_bus, owner, err);
                            }
                        }
                    }
                }
                // Checked only after a `QueueFollowUp`, never after an
                // unrelated command: the queue is released by a completed turn
                // and by a new entry landing while it is still draining, not by
                // whatever else the operator happened to do next.
                if queued_follow_up && session_queue_draining && queue_owner.lane_id.is_none() {
                    session_queue_draining = drain_session_queue(
                        &mut engine,
                        &queue_owner,
                        SupervisorShared {
                            event_bus: &event_bus,
                            active_control: &active_control,
                            pending_approvals: &pending_approvals,
                            approval_timers: &approval_timers,
                            permission_control: &permission_control,
                        },
                        approval_ttl_secs,
                    );
                    emit_frontend_status_events_if_changed(
                        &event_bus,
                        queue_owner,
                        engine.frontend_status_lifecycle_events(),
                    );
                }
                if let Err(error) =
                    sync_lane_permissions(&lane_supervisor, &engine, applied_permission_epoch)
                {
                    emit_error(&event_bus, RuntimeOwner::default(), error);
                }
            }
            SupervisorMessage::LaneApprovalResponse {
                owner,
                command_id,
                request_id,
                response,
            } => {
                if let Err(error) =
                    sync_lane_permissions(&lane_supervisor, &engine, applied_permission_epoch)
                {
                    emit_error(&event_bus, owner, error);
                    continue;
                }
                match lane_supervisor.respond_to_approval(
                    &owner,
                    &command_id,
                    &request_id,
                    response.clone(),
                ) {
                    Ok(Some(true)) => emit_event(
                        &event_bus,
                        owner,
                        RuntimeEventKind::CommandAccepted {
                            command_id,
                            command: redacted_runtime_command_for_event(
                                &RuntimeCommand::RespondToApproval {
                                    request_id,
                                    response,
                                },
                            ),
                        },
                    ),
                    Ok(Some(false)) => {}
                    Ok(None) => emit_event(
                        &event_bus,
                        owner,
                        RuntimeEventKind::CommandRejected {
                            command_id,
                            reason: format!("approval request `{request_id}` is not pending"),
                        },
                    ),
                    Err(error) => emit_error(&event_bus, owner, error),
                }
            }
            SupervisorMessage::ResumeContextRetrieval {
                owner_id,
                request_id,
                owner,
                audit_id,
                job,
            } => {
                resume_context_retrieval_after_approval(
                    &mut engine,
                    owner_id,
                    request_id,
                    owner,
                    audit_id,
                    *job,
                    &event_bus,
                    &active_control,
                );
            }
            SupervisorMessage::ResumeProjectMutation {
                owner_id,
                owner,
                command,
                response,
            } => {
                resume_project_mutation_after_approval(
                    &mut engine,
                    owner_id,
                    owner,
                    command,
                    response,
                    &event_bus,
                    &active_control,
                );
            }
            SupervisorMessage::Snapshot { response } => {
                emit_frontend_status_events_if_changed(
                    &event_bus,
                    RuntimeOwner::default(),
                    engine.frontend_status_lifecycle_events(),
                );
                let envelope = match event_bus.state.lock() {
                    Ok(state) => {
                        // Cursor and live projection share one lock so a
                        // reconnect cannot pair a newer transient event with
                        // an older snapshot boundary.
                        Ok(RuntimeSnapshotEnvelope {
                            schema_version: FRONTEND_SCHEMA_V1,
                            capabilities: runtime_frontend_capabilities(),
                            cursor: state.journal.current_cursor(),
                            snapshot: state.live_view.snapshot.clone(),
                            view: state.live_view.clone(),
                        })
                    }
                    Err(_) => Err("runtime event journal lock poisoned".to_string()),
                };
                let _ = response.send(envelope);
            }
            SupervisorMessage::TranscriptPage { request, response } => {
                let _ = response.send(engine.load_transcript_page(&request));
            }
        }
    }
}

fn sync_lane_permissions(
    lane_supervisor: &LaneSupervisor,
    engine: &SessionEngine,
    applied_permission_epoch: u64,
) -> Result<(), String> {
    lane_supervisor.sync_permissions(engine.lane_permission_engine(), applied_permission_epoch)
}

struct WorkerLivenessGuard(Arc<AtomicBool>);

impl Drop for WorkerLivenessGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn install_runtime_event_sink(
    engine: &mut SessionEngine,
    event_bus: RuntimeEventBus,
    owner: RuntimeOwner,
) {
    engine.set_runtime_event_sink(Some(Arc::new(move |events| {
        emit_events(&event_bus, owner.clone(), events);
    })));
}

fn runtime_frontend_capabilities() -> BTreeSet<CapabilityId> {
    FRONTEND_V1_CAPABILITIES
        .iter()
        .chain(FRONTEND_V1_EXTENSION_CAPABILITIES)
        .map(|capability| CapabilityId(capability.to_string()))
        .collect()
}

fn run_supervised_project_mutation(
    engine: &mut SessionEngine,
    owner: RuntimeOwner,
    command_id: String,
    command: RuntimeCommand,
    shared: SupervisorShared<'_>,
    approval_ttl_secs: u64,
) {
    if let Some(owner_id) = active_owner_id(shared.active_control, &owner) {
        emit_event(
            shared.event_bus,
            owner,
            RuntimeEventKind::CommandRejected {
                command_id,
                reason: format!("active runtime job `{owner_id}` is already running"),
            },
        );
        return;
    }
    match engine.prepare_project_mutation_for_supervisor(&owner, &command) {
        Ok(SupervisorProjectMutationPreparation::Ready) => {
            let mut approver = |_prompt: PermissionPrompt| {
                ApprovalResponse::deny(Some("unexpected project mutation approval".to_string()))
            };
            match engine.handle_runtime_command(command_id, command, &mut approver) {
                Ok(events) => emit_events(shared.event_bus, owner, events),
                Err(error) => emit_error(shared.event_bus, owner, error),
            }
        }
        Ok(SupervisorProjectMutationPreparation::Pending(prompt)) => {
            let request_id = fresh_id("approval");
            let mut approval =
                approval_request_view(&request_id, &prompt, owner.clone(), approval_ttl_secs);
            apply_operator_git_approval_shape(&mut approval, &command);
            let control = ModelRequestControl::new();
            if let Err(error) = acquire_active_job(
                shared.active_control,
                command_id.clone(),
                owner.clone(),
                control,
                ActiveJobState::PendingApproval {
                    request_id: request_id.clone(),
                },
            ) {
                emit_event(
                    shared.event_bus,
                    owner,
                    RuntimeEventKind::CommandRejected {
                        command_id,
                        reason: error,
                    },
                );
                return;
            }
            emit_event(
                shared.event_bus,
                owner.clone(),
                RuntimeEventKind::CommandAccepted {
                    command_id: command_id.clone(),
                    command: redacted_runtime_command_for_event(&command),
                },
            );
            let permission_epoch = shared
                .permission_control
                .lock()
                .map(|control| control.epoch())
                .unwrap_or(u64::MAX);
            insert_pending_approval(
                shared.pending_approvals,
                request_id.clone(),
                PendingApproval {
                    owner: owner.clone(),
                    audit_id: approval.audit_id.clone(),
                    tool_name: approval.tool_name.clone(),
                    expires_at: approval.expires_at,
                    permission_epoch,
                    allowed_scopes: approval.allowed_scopes.clone(),
                    target: PendingApprovalTarget::ProjectMutation {
                        owner_id: command_id,
                        command: Box::new(command),
                    },
                },
            );
            emit_event(
                shared.event_bus,
                owner,
                RuntimeEventKind::ApprovalRequested {
                    approval: approval.clone(),
                },
            );
            schedule_approval_expiry(
                request_id,
                approval.expires_at,
                shared.event_bus,
                shared.active_control,
                shared.pending_approvals,
                shared.approval_timers,
            );
        }
        Err(reason) => emit_event(
            shared.event_bus,
            owner,
            RuntimeEventKind::CommandRejected { command_id, reason },
        ),
    }
}

fn resume_project_mutation_after_approval(
    engine: &mut SessionEngine,
    owner_id: String,
    owner: RuntimeOwner,
    command: RuntimeCommand,
    response: ApprovalResponse,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
) {
    let mut response = Some(response);
    let mut approver = |_prompt: PermissionPrompt| {
        response.take().unwrap_or_else(|| {
            ApprovalResponse::deny(Some("approval response was already consumed".to_string()))
        })
    };
    let result = engine.handle_runtime_command(owner_id.clone(), command, &mut approver);
    clear_active_control(active_control, &owner_id);
    match result {
        Ok(events) => {
            let events = events
                .into_iter()
                .filter(|event| !matches!(event.kind, RuntimeEventKind::CommandAccepted { .. }))
                .collect::<Vec<_>>();
            emit_events(event_bus, owner, events);
        }
        Err(error) => emit_error(event_bus, owner, error),
    }
}

fn run_supervised_context_retrieval(
    engine: &mut SessionEngine,
    owner: RuntimeOwner,
    command_id: String,
    handle_id: String,
    reason: String,
    shared: SupervisorShared<'_>,
    approval_ttl_secs: u64,
) {
    if let Some(owner_id) = active_owner_id(shared.active_control, &owner) {
        emit_event(
            shared.event_bus,
            owner.clone(),
            RuntimeEventKind::CommandRejected {
                command_id,
                reason: format!("active runtime job `{owner_id}` is already running"),
            },
        );
        return;
    }
    let prepared = match engine.prepare_context_retrieval_for_supervisor(&handle_id, &reason) {
        Ok(prepared) => prepared,
        Err(err) => {
            emit_event(
                shared.event_bus,
                owner.clone(),
                RuntimeEventKind::CommandRejected {
                    command_id,
                    reason: err,
                },
            );
            return;
        }
    };
    match prepared {
        SupervisorContextRetrievalPreparation::Ready(prepared) => {
            let control = ModelRequestControl::new();
            if let Err(err) = acquire_active_job(
                shared.active_control,
                command_id.clone(),
                owner.clone(),
                control.clone(),
                ActiveJobState::Running,
            ) {
                emit_event(
                    shared.event_bus,
                    owner.clone(),
                    RuntimeEventKind::CommandRejected {
                        command_id,
                        reason: err,
                    },
                );
                return;
            }
            emit_event(
                shared.event_bus,
                owner.clone(),
                RuntimeEventKind::CommandAccepted {
                    command_id: command_id.clone(),
                    command: redacted_runtime_command_for_event(&RuntimeCommand::RetrieveContext {
                        handle_id: handle_id.clone(),
                        reason: reason.clone(),
                    }),
                },
            );
            emit_events(shared.event_bus, owner.clone(), prepared.pre_events);
            start_context_retrieval_worker(
                command_id,
                prepared.job,
                control,
                owner.clone(),
                shared.event_bus,
                shared.active_control,
            );
        }
        SupervisorContextRetrievalPreparation::PendingApproval { mut approval, job } => {
            let permission_epoch = shared
                .permission_control
                .lock()
                .map(|control| control.epoch())
                .unwrap_or(u64::MAX);
            approval.owner = owner.clone();
            approval.audit_id = fresh_id("audit");
            approval.expires_at = now_timestamp().saturating_add(approval_ttl_secs);
            approval.allowed_scopes = allowed_approval_scopes(&approval.owner, &[]);
            let control = ModelRequestControl::new();
            if let Err(err) = acquire_active_job(
                shared.active_control,
                command_id.clone(),
                owner.clone(),
                control,
                ActiveJobState::PendingApproval {
                    request_id: approval.id.clone(),
                },
            ) {
                emit_event(
                    shared.event_bus,
                    owner.clone(),
                    RuntimeEventKind::CommandRejected {
                        command_id,
                        reason: err,
                    },
                );
                return;
            }
            emit_event(
                shared.event_bus,
                owner.clone(),
                RuntimeEventKind::CommandAccepted {
                    command_id: command_id.clone(),
                    command: redacted_runtime_command_for_event(&RuntimeCommand::RetrieveContext {
                        handle_id: handle_id.clone(),
                        reason: reason.clone(),
                    }),
                },
            );
            insert_pending_approval(
                shared.pending_approvals,
                approval.id.clone(),
                PendingApproval {
                    owner: approval.owner.clone(),
                    audit_id: approval.audit_id.clone(),
                    tool_name: approval.tool_name.clone(),
                    expires_at: approval.expires_at,
                    permission_epoch,
                    allowed_scopes: approval.allowed_scopes.clone(),
                    target: PendingApprovalTarget::ContextRetrieval {
                        owner_id: command_id,
                        job: Box::new(job),
                    },
                },
            );
            emit_event(
                shared.event_bus,
                owner,
                RuntimeEventKind::ApprovalRequested {
                    approval: approval.clone(),
                },
            );
            schedule_approval_expiry(
                approval.id,
                approval.expires_at,
                shared.event_bus,
                shared.active_control,
                shared.pending_approvals,
                shared.approval_timers,
            );
        }
    }
}

fn start_context_retrieval_worker(
    owner_id: String,
    job: ContextRetrievalJob,
    control: ModelRequestControl,
    owner: RuntimeOwner,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
) {
    let worker_event_bus = event_bus.clone();
    let worker_active_control = Arc::clone(active_control);
    thread::spawn(move || {
        let result = execute_context_retrieval_job(job, &control);
        clear_active_control(&worker_active_control, &owner_id);
        match result {
            Ok(events) => {
                if control.check_cancelled().is_ok() {
                    emit_events(&worker_event_bus, owner.clone(), events);
                } else {
                    emit_error(
                        &worker_event_bus,
                        owner.clone(),
                        "Model request cancelled".to_string(),
                    );
                }
            }
            Err(err) => emit_error(&worker_event_bus, owner, err),
        }
    });
}

fn acquire_active_job(
    active_control: &ActiveControlRegistry,
    owner_id: String,
    owner: RuntimeOwner,
    control: ModelRequestControl,
    state: ActiveJobState,
) -> Result<(), String> {
    let mut controls = active_control
        .lock()
        .map_err(|_| "active turn lock poisoned".to_string())?;
    let key = RuntimeOwnerKey::from(&owner);
    if let Some(active) = controls.get(&key) {
        return Err(format!(
            "active runtime job `{}` is already running",
            active.owner_id
        ));
    }
    controls.insert(
        key,
        ActiveRuntimeControl {
            owner_id,
            owner,
            control,
            state,
        },
    );
    Ok(())
}

fn acquire_active_agent_session(
    active_control: &ActiveControlRegistry,
    owner_id: String,
    owner: RuntimeOwner,
    control: ModelRequestControl,
) -> Result<(), String> {
    let mut controls = active_control
        .lock()
        .map_err(|_| "active turn lock poisoned".to_string())?;
    if let Some(active) = controls
        .values()
        .find(|active| runtime_owners_share_lane(&active.owner, &owner))
    {
        return Err(format!(
            "active agent session `{}` is already running for this lane",
            active.owner_id
        ));
    }
    controls.insert(
        RuntimeOwnerKey::from(&owner),
        ActiveRuntimeControl {
            owner_id,
            owner,
            control,
            state: ActiveJobState::Running,
        },
    );
    Ok(())
}

fn runtime_owners_share_lane(left: &RuntimeOwner, right: &RuntimeOwner) -> bool {
    left.workspace_id == right.workspace_id
        && left.project_id == right.project_id
        && left.lane_id.is_some()
        && left.lane_id == right.lane_id
}

fn active_control_for_owner(
    active_control: &ActiveControlRegistry,
    owner_id: &str,
) -> Option<ModelRequestControl> {
    active_control.lock().ok().and_then(|controls| {
        controls
            .values()
            .find(|active| active.owner_id == owner_id)
            .map(|active| active.control.clone())
    })
}

fn active_agent_lane_owner_id(
    active_control: &ActiveControlRegistry,
    owner: &RuntimeOwner,
) -> Option<String> {
    active_control.lock().ok().and_then(|controls| {
        controls
            .values()
            .find(|active| runtime_owners_share_lane(&active.owner, owner))
            .map(|active| active.owner_id.clone())
    })
}

fn mark_active_running(
    active_control: &ActiveControlRegistry,
    owner_id: &str,
) -> Result<(), String> {
    let mut controls = active_control
        .lock()
        .map_err(|_| "active turn lock poisoned".to_string())?;
    let Some(active) = controls
        .values_mut()
        .find(|active| active.owner_id == owner_id)
    else {
        return Err(format!(
            "active runtime job `{owner_id}` is no longer pending"
        ));
    };
    active.state = ActiveJobState::Running;
    Ok(())
}

fn mark_active_pending(
    active_control: &ActiveControlRegistry,
    owner_id: &str,
    request_id: String,
) -> Result<(), String> {
    let mut controls = active_control
        .lock()
        .map_err(|_| "active turn lock poisoned".to_string())?;
    let Some(active) = controls
        .values_mut()
        .find(|active| active.owner_id == owner_id)
    else {
        return Err(format!(
            "active runtime job `{owner_id}` is no longer running"
        ));
    };
    active.state = ActiveJobState::PendingApproval { request_id };
    Ok(())
}

fn insert_pending_approval(
    pending_approvals: &Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    request_id: String,
    pending: PendingApproval,
) {
    if let Ok(mut approvals) = pending_approvals.lock() {
        approvals.insert(request_id, pending);
    }
}

fn schedule_approval_expiry(
    request_id: String,
    expires_at: u64,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
    pending_approvals: &Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    approval_timers: &Arc<ApprovalTimerRegistry>,
) {
    let event_bus = event_bus.clone();
    let active_control = Arc::clone(active_control);
    let pending_approvals = Arc::clone(pending_approvals);
    let timers = Arc::clone(approval_timers);
    let timer_control = Arc::clone(approval_timers);
    let worker = thread::spawn(move || {
        while expires_at > now_timestamp() {
            if timer_control.shutdown.load(Ordering::Acquire) {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        if timer_control.shutdown.load(Ordering::Acquire) {
            return;
        }
        resolve_pending_approval_by_id(
            &request_id,
            ApprovalDecision::Deny,
            &event_bus,
            &active_control,
            &pending_approvals,
            Some("Approval expired; default action deny".to_string()),
        );
    });
    timers.push(worker);
}

fn expire_pending_approvals_at(
    now: u64,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
    pending_approvals: &Arc<Mutex<BTreeMap<String, PendingApproval>>>,
) {
    let expired_ids = pending_approvals
        .lock()
        .map(|approvals| {
            approvals
                .iter()
                .filter(|(_, approval)| approval.expires_at <= now)
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for request_id in expired_ids {
        resolve_pending_approval_by_id(
            &request_id,
            ApprovalDecision::Deny,
            event_bus,
            active_control,
            pending_approvals,
            Some("Approval expired; default action deny".to_string()),
        );
    }
}

fn resolve_pending_approval_by_id(
    request_id: &str,
    decision: ApprovalDecision,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
    pending_approvals: &Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    error_message: Option<String>,
) {
    let pending = pending_approvals
        .lock()
        .ok()
        .and_then(|mut approvals| approvals.remove(request_id));
    if let Some(pending) = pending {
        resolve_removed_pending_approval(
            request_id,
            pending,
            decision,
            event_bus,
            active_control,
            error_message,
        );
    }
}

fn resolve_removed_pending_approval(
    request_id: &str,
    pending: PendingApproval,
    decision: ApprovalDecision,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
    error_message: Option<String>,
) {
    let owner_id = match &pending.target {
        PendingApprovalTarget::Channel { owner_id, .. } => owner_id.clone(),
        PendingApprovalTarget::ContextRetrieval { owner_id, .. } => owner_id.clone(),
        PendingApprovalTarget::ProjectMutation { owner_id, .. } => owner_id.clone(),
    };
    let owner = pending.owner.clone();
    emit_event(
        event_bus,
        owner.clone(),
        approval_resolved_event(
            request_id,
            decision.clone(),
            pending.owner,
            pending.audit_id,
        ),
    );
    if !matches!(decision, ApprovalDecision::Allow { .. }) {
        clear_active_control(active_control, &owner_id);
    }
    match pending.target {
        PendingApprovalTarget::Channel { sender, .. } => {
            let _ = sender.send(ApprovalResponse {
                decision,
                feedback: error_message,
            });
        }
        PendingApprovalTarget::ContextRetrieval { .. }
        | PendingApprovalTarget::ProjectMutation { .. } => {
            if let Some(message) = error_message {
                emit_error(event_bus, owner, message);
            }
        }
    }
}

fn approval_decision_is_allowed_by_request(
    decision: &ApprovalDecision,
    pending: &PendingApproval,
) -> bool {
    match decision {
        ApprovalDecision::Deny => true,
        ApprovalDecision::Allow { scope } => pending
            .allowed_scopes
            .iter()
            .any(|allowed| allowed == scope),
    }
}

#[allow(clippy::too_many_arguments)]
fn resume_context_retrieval_after_approval(
    engine: &mut SessionEngine,
    owner_id: String,
    _request_id: String,
    owner: RuntimeOwner,
    _audit_id: String,
    job: ContextRetrievalJob,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
) {
    let Some(control) = active_control_for_owner(active_control, &owner_id) else {
        emit_error(
            event_bus,
            owner,
            format!("active runtime job `{owner_id}` is no longer pending"),
        );
        return;
    };
    if let Err(err) = engine.validate_context_retrieval_job_for_supervisor(&job) {
        clear_active_control(active_control, &owner_id);
        emit_error(event_bus, owner, err);
        return;
    }
    if let Err(err) = mark_active_running(active_control, &owner_id) {
        emit_error(event_bus, owner, err);
        return;
    }
    start_context_retrieval_worker(owner_id, job, control, owner, event_bus, active_control);
}

fn active_owner_id(active_control: &ActiveControlRegistry, owner: &RuntimeOwner) -> Option<String> {
    active_control.lock().ok().and_then(|controls| {
        controls
            .get(&RuntimeOwnerKey::from(owner))
            .map(|active| active.owner_id.clone())
    })
}

fn lane_agent_session_binding(
    event_bus: &RuntimeEventBus,
    owner: &RuntimeOwner,
) -> Result<Option<LaneAgentExecutionBinding>, String> {
    let Some(lane_id) = owner.lane_id.clone() else {
        return Ok(None);
    };
    let (store, cached) = event_bus
        .state
        .lock()
        .map_err(|_| "runtime event state poisoned".to_string())
        .map(|state| {
            (
                state.lane_agent_store.clone(),
                state.lane_agent_bindings.get(&lane_id).cloned(),
            )
        })?;
    let Some(store) = store else {
        return Ok(cached);
    };
    let durable = store
        .load_lane_agent_bindings()
        .map_err(|error| format!("failed to revalidate Lane-agent binding: {error}"))?
        .get(&lane_id)
        .map(|binding| LaneAgentExecutionBinding {
            lane_id: binding.lane_id.clone(),
            agent_id: binding.agent_id.clone(),
            session_id: binding.session_id.clone(),
        });
    let Some(durable) = durable else {
        return Ok(cached);
    };
    if let Some(cached) = cached
        && (cached.agent_id != durable.agent_id || cached.session_id != durable.session_id)
    {
        return Err(format!(
            "conflicting Lane-agent binding for lane `{lane_id}`: cached agent `{}` session `{}` versus durable agent `{}` session `{}`",
            cached.agent_id, cached.session_id, durable.agent_id, durable.session_id
        ));
    }
    event_bus
        .state
        .lock()
        .map_err(|_| "runtime event state poisoned".to_string())?
        .lane_agent_bindings
        .insert(lane_id, durable.clone());
    Ok(Some(durable))
}

fn reserve_lane_agent_session_binding(
    event_bus: &RuntimeEventBus,
    lane_id: &str,
    agent_id: &str,
    session_id: &str,
) -> Result<LaneAgentExecutionBinding, String> {
    let store = event_bus
        .state
        .lock()
        .map_err(|_| "runtime event state poisoned".to_string())?
        .lane_agent_store
        .clone();
    let binding = if let Some(store) = store {
        store
            .bind_lane_agent_once(lane_id, agent_id, session_id, now_timestamp())
            .map(|binding| LaneAgentExecutionBinding {
                lane_id: binding.lane_id,
                agent_id: binding.agent_id,
                session_id: binding.session_id,
            })
            .map_err(|error| format!("lane_already_bound_to_agent_session: {error}"))?
    } else {
        LaneAgentExecutionBinding {
            lane_id: lane_id.to_string(),
            agent_id: agent_id.to_string(),
            session_id: session_id.to_string(),
        }
    };
    let mut state = event_bus
        .state
        .lock()
        .map_err(|_| "runtime event state poisoned".to_string())?;
    if let Some(existing) = state.lane_agent_bindings.get(lane_id)
        && (existing.agent_id != binding.agent_id || existing.session_id != binding.session_id)
    {
        return Err(lane_agent_session_binding_rejection(existing));
    }
    state
        .lane_agent_bindings
        .insert(lane_id.to_string(), binding.clone());
    Ok(binding)
}

fn publish_reactivated_agent_session_owner(
    event_bus: &RuntimeEventBus,
    session: &AgentSessionView,
) -> Result<(), String> {
    let durable = lane_agent_session_binding(event_bus, &session.owner)?
        .ok_or_else(|| "agent session has no durable Lane-agent binding".to_string())?;
    if durable.lane_id != session.lane_id
        || durable.agent_id != session.agent_id
        || durable.session_id != session.session_id
    {
        return Err(lane_agent_session_binding_rejection(&durable));
    }

    let bindings = event_bus
        .state
        .lock()
        .map_err(|_| "runtime event state poisoned".to_string())?
        .live_view
        .lane_runtime_owners
        .iter()
        .filter(|binding| binding.lane_id == session.lane_id)
        .cloned()
        .collect::<Vec<_>>();
    match bindings.as_slice() {
        [] => {
            // A terminal ACP session survives restart, while its process-local
            // runtime owner does not. Publish the exact durable owner before
            // starting a continuation so every frontend keeps the Lane live.
            emit_event(
                event_bus,
                session.owner.clone(),
                RuntimeEventKind::LaneRuntimeOwnerBound {
                    binding: LaneRuntimeOwnerBinding {
                        lane_id: session.lane_id.clone(),
                        owner: session.owner.clone(),
                    },
                },
            );
            Ok(())
        }
        [binding] if binding.owner == session.owner => Ok(()),
        _ => Err(format!(
            "Lane `{}` does not have one exact Core ACP owner",
            session.lane_id
        )),
    }
}

fn lane_agent_session_binding_rejection(binding: &LaneAgentExecutionBinding) -> String {
    format!(
        "lane_already_bound_to_agent_session: lane `{}` has durable Lane-agent identity agent `{}` session `{}`; a different execution identity cannot be accepted",
        binding.lane_id, binding.agent_id, binding.session_id
    )
}

fn clear_active_control(active_control: &ActiveControlRegistry, owner_id: &str) {
    if let Ok(mut controls) = active_control.lock() {
        controls.retain(|_, active| active.owner_id != owner_id);
    }
}

#[allow(clippy::too_many_arguments)]
fn run_supervised_agent_session(
    engine: &SessionEngine,
    owner: RuntimeOwner,
    command_id: String,
    request: AgentSessionRequest,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
    pending_approvals: &Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    approval_timers: &Arc<ApprovalTimerRegistry>,
    permission_control: &Arc<Mutex<PermissionControlState>>,
    approval_ttl_secs: u64,
) {
    match lane_agent_session_binding(event_bus, &owner) {
        Ok(Some(binding)) => {
            emit_event(
                event_bus,
                owner,
                RuntimeEventKind::CommandRejected {
                    command_id,
                    reason: lane_agent_session_binding_rejection(&binding),
                },
            );
            return;
        }
        Ok(None) => {}
        Err(error) => {
            emit_error(event_bus, owner, error);
            return;
        }
    }
    if permission_control
        .lock()
        .map(|control| control.applied.blocks_mutation())
        .unwrap_or(true)
    {
        emit_event(
            event_bus,
            owner,
            RuntimeEventKind::CommandRejected {
                command_id,
                reason:
                    "agent session execution requires Build mode with Ask or Autonomous permission"
                        .to_string(),
            },
        );
        return;
    }
    if let Err(reason) = validate_typed_agent_session_request(&request) {
        emit_event(
            event_bus,
            owner,
            RuntimeEventKind::CommandRejected { command_id, reason },
        );
        return;
    }
    let session_id = fresh_id("agent-session");
    let session_owner = RuntimeOwner {
        lane_id: Some(request.lane_id.clone()),
        session_id: Some(session_id.clone()),
        ..owner
    };
    if let Err(reason) = reserve_lane_agent_session_binding(
        event_bus,
        &request.lane_id,
        &request.agent_id,
        &session_id,
    ) {
        emit_event(
            event_bus,
            session_owner,
            RuntimeEventKind::CommandRejected { command_id, reason },
        );
        return;
    }
    let control = ModelRequestControl::new();
    if let Err(error) = acquire_active_agent_session(
        active_control,
        session_id.clone(),
        session_owner.clone(),
        control,
    ) {
        emit_event(
            event_bus,
            session_owner,
            RuntimeEventKind::CommandRejected {
                command_id,
                reason: error,
            },
        );
        return;
    }
    emit_event(
        event_bus,
        session_owner.clone(),
        RuntimeEventKind::CommandAccepted {
            command_id: command_id.clone(),
            command: redacted_runtime_command_for_event(&RuntimeCommand::StartAgentSession {
                request: request.clone(),
            }),
        },
    );

    let session_template = AgentSessionView {
        session_id: session_id.clone(),
        lane_id: request.lane_id.clone(),
        agent_id: request.agent_id.clone(),
        model: request.model.clone(),
        status: AgentSessionStatus::Starting,
        owner: session_owner.clone(),
        task: request.task.clone(),
        diagnostic: None,
        output: None,
    };
    let approver = supervised_agent_session_approver(
        engine,
        session_template.clone(),
        session_owner.clone(),
        session_id.clone(),
        event_bus,
        active_control,
        pending_approvals,
        approval_timers,
        permission_control,
        approval_ttl_secs,
    );
    let runtime_event_sink = supervised_agent_session_sink(
        event_bus,
        session_owner.clone(),
        session_id.clone(),
        active_control,
        engine.context_engine_root().to_path_buf(),
    );

    if let Err(error) = start_typed_agent_session(
        engine.cwd(),
        session_id.clone(),
        request,
        session_owner.clone(),
        runtime_event_sink,
        approver,
    ) {
        clear_active_control(active_control, &session_id);
        let mut failed = session_template;
        failed.status = AgentSessionStatus::Failed;
        failed.diagnostic = Some(error.clone());
        emit_event(
            event_bus,
            session_owner.clone(),
            RuntimeEventKind::AgentSessionFailed { session: failed },
        );
        emit_error(event_bus, session_owner, error);
    }
}

#[allow(clippy::too_many_arguments)]
fn supervised_agent_session_approver(
    engine: &SessionEngine,
    session: AgentSessionView,
    owner: RuntimeOwner,
    session_id: String,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
    pending_approvals: &Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    approval_timers: &Arc<ApprovalTimerRegistry>,
    permission_control: &Arc<Mutex<PermissionControlState>>,
    approval_ttl_secs: u64,
) -> AgentSessionApprover {
    let approval_bus = event_bus.clone();
    let approval_cwd = engine.cwd().to_path_buf();
    let approval_active = Arc::clone(active_control);
    let approval_pending = Arc::clone(pending_approvals);
    let approval_timers = Arc::clone(approval_timers);
    let approval_permission = Arc::clone(permission_control);
    Box::new(move |prompt: PermissionPrompt| {
        let permission_epoch = approval_permission
            .lock()
            .map(|control| control.epoch())
            .unwrap_or(u64::MAX);
        let request_id = fresh_id("approval");
        let (approval_sender, approval_receiver) = mpsc::channel();
        let approval =
            approval_request_view(&request_id, &prompt, owner.clone(), approval_ttl_secs);
        let _ = mark_active_pending(&approval_active, &session_id, request_id.clone());
        let _ = mark_typed_agent_session_status(&approval_cwd, &session_id, "waiting_approval");
        insert_pending_approval(
            &approval_pending,
            request_id.clone(),
            PendingApproval {
                owner: approval.owner.clone(),
                audit_id: approval.audit_id.clone(),
                tool_name: approval.tool_name.clone(),
                expires_at: approval.expires_at,
                permission_epoch,
                allowed_scopes: approval.allowed_scopes.clone(),
                target: PendingApprovalTarget::Channel {
                    owner_id: session_id.clone(),
                    sender: approval_sender,
                },
            },
        );
        emit_event(
            &approval_bus,
            owner.clone(),
            RuntimeEventKind::ApprovalRequested {
                approval: approval.clone(),
            },
        );
        let mut waiting = session.clone();
        waiting.status = AgentSessionStatus::WaitingApproval;
        emit_event(
            &approval_bus,
            owner.clone(),
            RuntimeEventKind::AgentSessionUpdated { session: waiting },
        );
        schedule_approval_expiry(
            request_id,
            approval.expires_at,
            &approval_bus,
            &approval_active,
            &approval_pending,
            &approval_timers,
        );
        let response = approval_receiver.recv().unwrap_or_else(|_| {
            ApprovalResponse::deny(Some("approval response channel closed".to_string()))
        });
        let _ = mark_typed_agent_session_status(&approval_cwd, &session_id, "running");
        let mut running = session.clone();
        running.status = AgentSessionStatus::Running;
        emit_event(
            &approval_bus,
            owner.clone(),
            RuntimeEventKind::AgentSessionUpdated { session: running },
        );
        response
    })
}

/// The runtime's ingestion point for an external Agent's facts.
///
/// Also where an adapter-reported patch is canonicalized
/// (`runtime.durable_work_evidence`, C7). `viden-agents` is a leaf below the
/// runtime with no ContextStore of its own, so it publishes a `patch` fact with
/// `canonical: None` and the diff carried in the fact's `metadata`; the bytes
/// become canonical evidence here, on the runtime side of that boundary, before
/// the batch reaches the bus. Doing it here rather than in the adapter is what
/// keeps the dependency direction: the store stays runtime-owned and the
/// adapter keeps reporting rather than persisting.
fn supervised_agent_session_sink(
    event_bus: &RuntimeEventBus,
    owner: RuntimeOwner,
    session_id: String,
    active_control: &ActiveControlRegistry,
    context_engine_root: PathBuf,
) -> RuntimeEventSink {
    let sink_bus = event_bus.clone();
    let sink_active = Arc::clone(active_control);
    Arc::new(move |events: Vec<RuntimeEvent>| {
        let mut events = events;
        crate::work_evidence::canonicalize_agent_patch_evidence(&context_engine_root, &mut events);
        let terminal = events.iter().any(|event| {
            matches!(
                event.kind,
                RuntimeEventKind::AgentSessionCompleted { .. }
                    | RuntimeEventKind::AgentSessionFailed { .. }
            ) || matches!(
                &event.kind,
                RuntimeEventKind::AgentSessionUpdated { session }
                    if session.status == AgentSessionStatus::Cancelled
            )
        });
        emit_events(&sink_bus, owner.clone(), events);
        if terminal {
            clear_active_control(&sink_active, &session_id);
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn run_supervised_agent_session_continuation(
    engine: &SessionEngine,
    owner: RuntimeOwner,
    command_id: String,
    session_id: String,
    content: Option<String>,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
    pending_approvals: &Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    approval_timers: &Arc<ApprovalTimerRegistry>,
    permission_control: &Arc<Mutex<PermissionControlState>>,
    approval_ttl_secs: u64,
) {
    if permission_control
        .lock()
        .map(|control| control.applied.blocks_mutation())
        .unwrap_or(true)
    {
        emit_event(
            event_bus,
            owner,
            RuntimeEventKind::CommandRejected {
                command_id,
                reason:
                    "agent session execution requires Build mode with Ask or Autonomous permission"
                        .to_string(),
            },
        );
        return;
    }
    let session = event_bus.state.lock().ok().and_then(|state| {
        state
            .live_view
            .agent_sessions
            .iter()
            .find(|session| session.session_id == session_id)
            .cloned()
    });
    let Some(mut session) = session else {
        emit_event(
            event_bus,
            owner,
            RuntimeEventKind::CommandRejected {
                command_id,
                reason: format!("agent session `{session_id}` is not known"),
            },
        );
        return;
    };
    if session.owner != owner {
        emit_event(
            event_bus,
            owner,
            RuntimeEventKind::CommandRejected {
                command_id,
                reason: "agent_session_owner_mismatch".to_string(),
            },
        );
        return;
    }
    if !matches!(
        session.status,
        AgentSessionStatus::Completed | AgentSessionStatus::Failed | AgentSessionStatus::Cancelled
    ) {
        emit_event(
            event_bus,
            owner,
            RuntimeEventKind::CommandRejected {
                command_id,
                reason: format!("agent session `{session_id}` is still active"),
            },
        );
        return;
    }
    if let Err(reason) = publish_reactivated_agent_session_owner(event_bus, &session) {
        emit_event(
            event_bus,
            owner,
            RuntimeEventKind::CommandRejected { command_id, reason },
        );
        return;
    }
    if let Some(content) = content.as_ref() {
        session.task = content.clone();
    }
    session.status = AgentSessionStatus::Starting;
    session.diagnostic = None;
    let control = ModelRequestControl::new();
    if let Err(error) =
        acquire_active_agent_session(active_control, session_id.clone(), owner.clone(), control)
    {
        emit_event(
            event_bus,
            owner,
            RuntimeEventKind::CommandRejected {
                command_id,
                reason: error,
            },
        );
        return;
    }
    let accepted_command = if let Some(content) = content.clone() {
        RuntimeCommand::SendAgentSessionInput {
            input: viden_types::AgentSessionInput {
                session_id: session_id.clone(),
                content,
            },
        }
    } else {
        RuntimeCommand::RetryAgentSession {
            session_id: session_id.clone(),
        }
    };
    emit_event(
        event_bus,
        owner.clone(),
        RuntimeEventKind::CommandAccepted {
            command_id,
            command: redacted_runtime_command_for_event(&accepted_command),
        },
    );
    let approver = supervised_agent_session_approver(
        engine,
        session.clone(),
        owner.clone(),
        session_id.clone(),
        event_bus,
        active_control,
        pending_approvals,
        approval_timers,
        permission_control,
        approval_ttl_secs,
    );
    let runtime_event_sink = supervised_agent_session_sink(
        event_bus,
        owner.clone(),
        session_id.clone(),
        active_control,
        engine.context_engine_root().to_path_buf(),
    );
    let result = if let Some(content) = content {
        resume_typed_agent_session(
            engine.cwd(),
            &session_id,
            content,
            owner.clone(),
            runtime_event_sink,
            approver,
        )
    } else {
        retry_typed_agent_session(
            engine.cwd(),
            &session_id,
            owner.clone(),
            runtime_event_sink,
            approver,
        )
    };
    if let Err(error) = result {
        clear_active_control(active_control, &session_id);
        session.status = AgentSessionStatus::Failed;
        session.diagnostic = Some(error.clone());
        emit_event(
            event_bus,
            owner.clone(),
            RuntimeEventKind::AgentSessionFailed { session },
        );
        emit_error(event_bus, owner, error);
    }
}

fn run_supervised_agent_session_cancel(
    engine: &SessionEngine,
    owner: RuntimeOwner,
    command_id: String,
    session_id: String,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
    pending_approvals: &Arc<Mutex<BTreeMap<String, PendingApproval>>>,
) {
    let session = event_bus.state.lock().ok().and_then(|state| {
        state
            .live_view
            .agent_sessions
            .iter()
            .find(|session| session.session_id == session_id)
            .cloned()
    });
    let Some(mut session) = session else {
        emit_event(
            event_bus,
            owner,
            RuntimeEventKind::CommandRejected {
                command_id,
                reason: format!("agent session `{session_id}` is not known"),
            },
        );
        return;
    };
    if session.owner != owner {
        emit_event(
            event_bus,
            owner,
            RuntimeEventKind::CommandRejected {
                command_id,
                reason: format!("agent session `{session_id}` owner mismatch"),
            },
        );
        return;
    }
    if matches!(
        session.status,
        AgentSessionStatus::Completed | AgentSessionStatus::Failed | AgentSessionStatus::Cancelled
    ) {
        emit_event(
            event_bus,
            owner,
            RuntimeEventKind::CommandAccepted {
                command_id,
                command: redacted_runtime_command_for_event(&RuntimeCommand::CancelAgentSession {
                    session_id,
                }),
            },
        );
        return;
    }
    let active = active_control.lock().ok().and_then(|controls| {
        controls
            .values()
            .find(|active| active.owner_id == session_id)
            .cloned()
    });
    if let Some(active) = active {
        active.control.cancel();
        if let ActiveJobState::PendingApproval { request_id } = active.state {
            resolve_pending_approval_by_id(
                &request_id,
                ApprovalDecision::Deny,
                event_bus,
                active_control,
                pending_approvals,
                Some("agent session cancelled by owner".to_string()),
            );
        }
    }
    match cancel_typed_agent_session(engine.cwd(), &session_id) {
        Ok(()) => {
            emit_event(
                event_bus,
                owner.clone(),
                RuntimeEventKind::CommandAccepted {
                    command_id,
                    command: redacted_runtime_command_for_event(
                        &RuntimeCommand::CancelAgentSession {
                            session_id: session_id.clone(),
                        },
                    ),
                },
            );
            session.status = AgentSessionStatus::Cancelled;
            session.diagnostic = Some("cancelled by owner".to_string());
            emit_event(
                event_bus,
                owner,
                RuntimeEventKind::AgentSessionUpdated { session },
            );
            clear_active_control(active_control, &session_id);
        }
        Err(error) => emit_event(
            event_bus,
            owner,
            RuntimeEventKind::CommandRejected {
                command_id,
                reason: error,
            },
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_supervised_agent_task(
    engine: &mut SessionEngine,
    owner: RuntimeOwner,
    command_id: String,
    task_id: String,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
    pending_approvals: &Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    approval_timers: &Arc<ApprovalTimerRegistry>,
    permission_control: &Arc<Mutex<PermissionControlState>>,
    approval_ttl_secs: u64,
) {
    let control = ModelRequestControl::new();
    if let Err(err) = acquire_active_job(
        active_control,
        command_id.clone(),
        owner.clone(),
        control.clone(),
        ActiveJobState::Running,
    ) {
        emit_event(
            event_bus,
            owner.clone(),
            RuntimeEventKind::CommandRejected {
                command_id,
                reason: err,
            },
        );
        return;
    }
    emit_event(
        event_bus,
        owner.clone(),
        RuntimeEventKind::CommandAccepted {
            command_id: command_id.clone(),
            command: redacted_runtime_command_for_event(&RuntimeCommand::StartAgentTask {
                task_id: task_id.clone(),
            }),
        },
    );

    let mut approver = |prompt: PermissionPrompt| {
        let permission_epoch = permission_control
            .lock()
            .map(|control| control.epoch())
            .unwrap_or(u64::MAX);
        let request_id = fresh_id("approval");
        let (approval_sender, approval_receiver) = mpsc::channel();
        let approval =
            approval_request_view(&request_id, &prompt, owner.clone(), approval_ttl_secs);
        let _ = mark_active_pending(active_control, &command_id, request_id.clone());
        insert_pending_approval(
            pending_approvals,
            request_id.clone(),
            PendingApproval {
                owner: approval.owner.clone(),
                audit_id: approval.audit_id.clone(),
                tool_name: approval.tool_name.clone(),
                expires_at: approval.expires_at,
                permission_epoch,
                allowed_scopes: approval.allowed_scopes.clone(),
                target: PendingApprovalTarget::Channel {
                    owner_id: command_id.clone(),
                    sender: approval_sender,
                },
            },
        );
        emit_event(
            event_bus,
            owner.clone(),
            RuntimeEventKind::ApprovalRequested {
                approval: approval.clone(),
            },
        );
        schedule_approval_expiry(
            request_id.clone(),
            approval.expires_at,
            event_bus,
            active_control,
            pending_approvals,
            approval_timers,
        );
        approval_receiver
            .recv()
            .unwrap_or(ApprovalResponse::deny(Some(
                "approval response channel closed".to_string(),
            )))
    };

    let mut emit_completed = |events| emit_events(event_bus, owner.clone(), events);
    let result = engine.run_agent_task_streaming_with_control(
        &task_id,
        &mut approver,
        &control,
        &mut emit_completed,
    );
    clear_active_control(active_control, &command_id);
    match result {
        Ok(events) => emit_events(event_bus, owner.clone(), events),
        Err(err) => emit_error(event_bus, owner, err),
    }
}

/// Runs a supervised native turn and everything its completion releases.
///
/// Returns whether the session queue is still draining: `true` after a
/// completed session-scoped turn whose drain emptied the queue, `false` as soon
/// as a turn does not complete or the owner is a Lane, whose queue belongs to
/// the Lane worker rather than to this engine.
#[allow(clippy::too_many_arguments)]
fn run_supervised_input(
    engine: &mut SessionEngine,
    owner: RuntimeOwner,
    command_id: String,
    content: String,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
    pending_approvals: &Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    approval_timers: &Arc<ApprovalTimerRegistry>,
    permission_control: &Arc<Mutex<PermissionControlState>>,
    approval_ttl_secs: u64,
) -> bool {
    match lane_agent_session_binding(event_bus, &owner) {
        Ok(Some(binding)) if binding.agent_id != "viden" => {
            emit_event(
                event_bus,
                owner,
                RuntimeEventKind::CommandRejected {
                    command_id,
                    reason: lane_agent_session_binding_rejection(&binding),
                },
            );
            return false;
        }
        Ok(_) => {}
        Err(error) => {
            emit_error(event_bus, owner, error);
            return false;
        }
    }
    if let Some(lane_id) = owner.lane_id.as_deref() {
        let session_id = owner.session_id.as_deref().unwrap_or(engine.session_id());
        if let Err(reason) =
            reserve_lane_agent_session_binding(event_bus, lane_id, "viden", session_id)
        {
            emit_event(
                event_bus,
                owner,
                RuntimeEventKind::CommandRejected { command_id, reason },
            );
            return false;
        }
    }
    let shared = SupervisorShared {
        event_bus,
        active_control,
        pending_approvals,
        approval_timers,
        permission_control,
    };
    let outcome = run_one_supervised_native_turn(
        engine,
        &owner,
        command_id.clone(),
        Some(command_id),
        content,
        TurnSource::UserInput,
        shared,
        approval_ttl_secs,
    );
    // A Lane's native turn does not touch this queue: `queued_runtime_inputs`
    // is the built-in engine's session queue, and a Lane's own follow-ups are
    // held and drained by the Lane worker.
    let armed = outcome.drains_queue()
        && owner.lane_id.is_none()
        && drain_session_queue(engine, &owner, shared, approval_ttl_secs);
    // Sampled once for the whole sequence rather than per drained turn: the
    // status lifecycle spawns bounded `git` processes, and a three-entry queue
    // would otherwise sample the same tree three times for one operator
    // action.
    emit_frontend_status_events_if_changed(
        event_bus,
        owner,
        engine.frontend_status_lifecycle_events(),
    );
    armed
}

/// Runs every queued follow-up the completed turn released.
///
/// Oldest first, each announced by `InputDequeued` before the turn it starts,
/// and the loop stops at the first entry whose turn does not complete: a queue
/// is never spent against a session that has already stopped working. Returns
/// whether the queue drained cleanly, which is what arms the next drain.
fn drain_session_queue(
    engine: &mut SessionEngine,
    owner: &RuntimeOwner,
    shared: SupervisorShared<'_>,
    approval_ttl_secs: u64,
) -> bool {
    if owner.lane_id.is_some() {
        return false;
    }
    while let Some((input_id, content)) = engine.take_oldest_queued_runtime_input() {
        emit_event(
            shared.event_bus,
            owner.clone(),
            RuntimeEventKind::InputDequeued {
                input_id: input_id.clone(),
            },
        );
        // A drained turn is a turn: it acquires its own active job, so
        // `CancelActiveTurn` stops it, and it prompts for approval through the
        // same approver, so an operator answers it exactly as they would a
        // typed turn's. What it does not do is announce a `CommandAccepted`,
        // because no client sent a command for it.
        let outcome = run_one_supervised_native_turn(
            engine,
            owner,
            fresh_id("queued-turn"),
            None,
            content,
            TurnSource::QueuedInput { input_id },
            shared,
            approval_ttl_secs,
        );
        if !outcome.drains_queue() {
            return false;
        }
    }
    true
}

/// One supervised native turn, bracketed by `TurnStarted` and `TurnFinished`.
///
/// `job_id` is the active-job key this turn registers under, so a cancellation
/// and an approval can find it; `announced_command` is the client command to
/// acknowledge, and is absent for a drained turn that no client asked for.
#[allow(clippy::too_many_arguments)]
fn run_one_supervised_native_turn(
    engine: &mut SessionEngine,
    owner: &RuntimeOwner,
    job_id: String,
    announced_command: Option<String>,
    content: String,
    source: TurnSource,
    shared: SupervisorShared<'_>,
    approval_ttl_secs: u64,
) -> TurnOutcome {
    let SupervisorShared {
        event_bus,
        active_control,
        pending_approvals,
        approval_timers,
        permission_control,
    } = shared;
    let owner = owner.clone();
    let command_id = job_id;
    let control = ModelRequestControl::new();
    if let Err(err) = acquire_active_job(
        active_control,
        command_id.clone(),
        owner.clone(),
        control.clone(),
        ActiveJobState::Running,
    ) {
        emit_event(
            event_bus,
            owner.clone(),
            RuntimeEventKind::CommandRejected {
                command_id,
                reason: err,
            },
        );
        // Nothing started, so nothing is bracketed and nothing drains: a
        // refused acquisition is not a turn that failed.
        return TurnOutcome::Cancelled;
    }
    if let Some(announced_command) = announced_command {
        emit_event(
            event_bus,
            owner.clone(),
            RuntimeEventKind::CommandAccepted {
                command_id: announced_command,
                command: redacted_runtime_command_for_event(&RuntimeCommand::SubmitUserInput {
                    content: content.clone(),
                }),
            },
        );
    }
    // The turn's own owner: the command's scope plus this turn's id, so a
    // client matches its composer on the scope it already knows while an audit
    // row has a turn to join on.
    let turn_id = fresh_id("turn");
    let turn_owner = RuntimeOwner {
        turn_id: Some(turn_id.clone()),
        ..owner.clone()
    };
    // The attribution window for this turn's archived work
    // (`runtime.durable_work_evidence`): every `patch` row a tool call in it
    // produces is owned by this turn, and the slot below is where the approval
    // that allowed a mutating tool leaves its audit id.
    let permission_receipt = engine.begin_native_turn(turn_owner.clone());
    emit_event(
        event_bus,
        turn_owner.clone(),
        RuntimeEventKind::TurnStarted {
            turn: TurnView {
                turn_id: turn_id.clone(),
                owner: turn_owner.clone(),
                source,
                started_at: now_timestamp(),
            },
        },
    );

    // Everything this turn published, kept so the durable facts among them can
    // be archived once the turn ends. Only the batch is kept, never the live
    // emission order: the bus has already delivered these.
    let turn_batch: std::cell::RefCell<Vec<RuntimeEvent>> = std::cell::RefCell::new(Vec::new());

    let mut approver = |prompt: PermissionPrompt| {
        let permission_epoch = permission_control
            .lock()
            .map(|control| control.epoch())
            .unwrap_or(u64::MAX);
        let request_id = fresh_id("approval");
        let (approval_sender, approval_receiver) = mpsc::channel();
        let approval =
            approval_request_view(&request_id, &prompt, owner.clone(), approval_ttl_secs);
        let _ = mark_active_pending(active_control, &command_id, request_id.clone());
        insert_pending_approval(
            pending_approvals,
            request_id.clone(),
            PendingApproval {
                owner: approval.owner.clone(),
                audit_id: approval.audit_id.clone(),
                tool_name: approval.tool_name.clone(),
                expires_at: approval.expires_at,
                permission_epoch,
                allowed_scopes: approval.allowed_scopes.clone(),
                target: PendingApprovalTarget::Channel {
                    owner_id: command_id.clone(),
                    sender: approval_sender,
                },
            },
        );
        emit_event(
            event_bus,
            owner.clone(),
            RuntimeEventKind::ApprovalRequested {
                approval: approval.clone(),
            },
        );
        schedule_approval_expiry(
            request_id.clone(),
            approval.expires_at,
            event_bus,
            active_control,
            pending_approvals,
            approval_timers,
        );
        let response = approval_receiver
            .recv()
            .unwrap_or(ApprovalResponse::deny(Some(
                "approval response channel closed".to_string(),
            )));
        // The receipt the tool call about to run may claim. Written only for an
        // allowed decision, because a denied approval permits nothing and a
        // patch row naming it as its permission snapshot would be a receipt for
        // a mutation that was refused.
        if response.is_allowed()
            && let Ok(mut slot) = permission_receipt.lock()
        {
            *slot = Some(approval.audit_id.clone());
        }
        response
    };

    let mut emit_completed = |events: Vec<RuntimeEvent>| {
        turn_batch.borrow_mut().extend(events.iter().cloned());
        emit_events(event_bus, owner.clone(), events);
    };
    let result = engine.process_runtime_turn_streaming_with_approval_and_control(
        &content,
        &mut approver,
        &control,
        &mut emit_completed,
    );
    clear_active_control(active_control, &command_id);
    let outcome = match result {
        Ok(events) => {
            turn_batch.borrow_mut().extend(events.iter().cloned());
            emit_events(event_bus, owner.clone(), events);
            TurnOutcome::Completed
        }
        Err(failure) => {
            turn_batch
                .borrow_mut()
                .extend(failure.completed_events.iter().cloned());
            emit_events(event_bus, owner.clone(), failure.completed_events);
            emit_error(event_bus, owner.clone(), failure.message.clone());
            // A cancelled turn also unwinds through this error path, because
            // the engine reports cancellation as a failed model request. The
            // control is the only place that knows the owner asked for it, and
            // the difference matters: a cancellation is a decision about this
            // turn, while a failure is the turn breaking.
            if control.is_cancelled() {
                TurnOutcome::Cancelled
            } else {
                TurnOutcome::failed(failure.message)
            }
        }
    };
    engine.end_native_turn();
    // The durable half of the turn, handed to the engine as one batch on every
    // exit — a failed turn's completed tool facts are exactly as real as a
    // successful one's, and the archive that drops them would tell a reviewer
    // the files were never touched. An append failure is published rather than
    // silently dropped: the events are already on the live bus, so a client
    // that saw them must also learn they did not reach the log.
    let absorbed = engine.absorb_supervised_events(&turn_batch.borrow());
    if let Err(error) = absorbed {
        emit_error(
            event_bus,
            turn_owner.clone(),
            format!("supervised turn facts were published but not archived: {error}"),
        );
    }
    // The last fact of the turn on every exit, after the error a failure
    // publishes and after the trailing snapshot a completion publishes.
    emit_event(
        event_bus,
        turn_owner.clone(),
        RuntimeEventKind::TurnFinished {
            turn_id,
            owner: turn_owner,
            outcome: outcome.clone(),
            finished_at: now_timestamp(),
        },
    );
    outcome
}

fn approval_request_view(
    request_id: &str,
    prompt: &PermissionPrompt,
    owner: RuntimeOwner,
    approval_ttl_secs: u64,
) -> ApprovalRequestView {
    let allowed_scopes = allowed_approval_scopes(&owner, &prompt.candidate_paths);
    ApprovalRequestView {
        id: request_id.to_string(),
        tool_name: prompt.tool_name.clone(),
        title: format!("Approve {}", prompt.tool_name),
        message: prompt.message.clone(),
        input_preview: prompt.input_preview.clone(),
        is_mutating: true,
        reason: Some(prompt.message.clone()),
        owner,
        risk: ApprovalRisk::Medium,
        target: ApprovalTarget {
            kind: prompt.tool_name.clone(),
            display: prompt.input_preview.clone(),
            canonical_ref: prompt.candidate_paths.first().cloned(),
        },
        allowed_scopes,
        policy_reason_key: "permission.requires_approval".to_string(),
        policy_reason_args: BTreeMap::new(),
        expires_at: now_timestamp().saturating_add(approval_ttl_secs),
        default_action: ApprovalDefaultAction::Deny,
        audit_id: fresh_id("audit"),
        decision_context: prompt.decision_context.clone(),
    }
}

/// Classifies an operator source-control approval so a dock can rank and group
/// it (`runtime.operator_git`, GUI-CORE-020).
///
/// `target.kind` is `"git"` rather than the tool name, because these five
/// actions are one surface to an operator — the commit bar — and a dock that
/// grouped `git_add` apart from `git_commit` would split one decision across
/// two rows.
///
/// The risk order is about reversibility, not about how much each action
/// writes. `Push` is `High` because it is the only one that leaves the
/// machine and cannot be taken back locally. `Commit` is `Medium`: it moves
/// `HEAD`, but the operator can still amend or reset before publishing.
/// Staging, unstaging, and fetching are `Low` because none of them destroys
/// working-tree content — an unstage explicitly leaves the working tree
/// alone, and a fetch only writes remote-tracking refs.
fn apply_operator_git_approval_shape(approval: &mut ApprovalRequestView, command: &RuntimeCommand) {
    let RuntimeCommand::RunOperatorGitAction { target, action, .. } = command else {
        return;
    };
    approval.risk = match action {
        OperatorGitAction::Push { .. } => ApprovalRisk::High,
        OperatorGitAction::Commit { .. } => ApprovalRisk::Medium,
        _ => ApprovalRisk::Low,
    };
    approval.target = ApprovalTarget {
        kind: "git".to_string(),
        display: format!("{} ({})", approval.tool_name, source_target_display(target)),
        // The target Core resolved, named as a Lane id or the workspace — never
        // a filesystem path, which would put the operator's home directory into
        // every approval record.
        canonical_ref: Some(source_target_display(target)),
    };
}

fn source_target_display(target: &viden_types::SourceTarget) -> String {
    match target {
        viden_types::SourceTarget::Lane { lane_id } => format!("lane:{lane_id}"),
        viden_types::SourceTarget::Workspace => "workspace".to_string(),
        _ => "workspace".to_string(),
    }
}

fn allowed_approval_scopes(owner: &RuntimeOwner, candidate_paths: &[String]) -> Vec<ApprovalScope> {
    let mut scopes = vec![ApprovalScope::Once];
    if let Some(session_id) = owner.session_id.clone()
        && !session_id.is_empty()
    {
        scopes.push(ApprovalScope::Session { session_id });
    }
    if !candidate_paths.is_empty() {
        scopes.push(ApprovalScope::RepoAllowlist {
            paths: candidate_paths.to_vec(),
        });
    }
    scopes
}

fn approval_resolved_event(
    request_id: &str,
    decision: ApprovalDecision,
    owner: RuntimeOwner,
    audit_id: String,
) -> RuntimeEventKind {
    RuntimeEventKind::ApprovalResolved {
        request_id: request_id.to_string(),
        decision,
        owner,
        audit_id,
    }
}

fn emit_events(bus: &RuntimeEventBus, owner: RuntimeOwner, events: Vec<RuntimeEvent>) {
    for event in events {
        emit_known_event(bus, owner.clone(), event);
    }
}

/// The Lane whose worktree this fact announces for the first time, if any.
///
/// Deliberately once per Lane per session: `sample_workspace_source` spawns
/// several bounded `git` invocations, and a Lane's status changes often enough
/// that sampling on every `LaneUpdated` would put those on the lane worker's
/// hot path. A row that already exists is refreshed by the periodic sampling
/// instead, which is where every other Lane's row comes from.
fn first_lane_worktree_to_sample(
    bus: &RuntimeEventBus,
    kind: &RuntimeEventKind,
) -> Option<(String, std::path::PathBuf)> {
    let RuntimeEventKind::LaneUpdated { lane } = kind else {
        return None;
    };
    if !lane.is_active() {
        return None;
    }
    let worktree = std::path::PathBuf::from(lane.worktree.as_ref()?);
    // A Lane with no worktree of its own is the workspace; a worktree the
    // effect has not created yet is sampled by the next periodic pass rather
    // than published as `Unavailable`.
    if !worktree.is_dir() {
        return None;
    }
    let already_sampled = {
        let state = bus.state.lock().ok()?;
        state.live_view.lane_sources.contains_key(&lane.id)
    };
    if already_sampled {
        return None;
    }
    Some((lane.id.clone(), worktree))
}

/// Runs the Lane-announcement sampling above over a list of facts and returns
/// the source rows it would publish.
///
/// Exists so the once-per-Lane rule can be tested without standing up a whole
/// supervisor and a Lane worker thread, which is what the rule is *about*
/// avoiding work on.
#[cfg(test)]
pub(crate) fn lane_source_rows_for_test(
    facts: Vec<RuntimeEventKind>,
) -> Vec<(String, viden_types::WorkspaceSourceView)> {
    let (sender, _receiver) = mpsc::channel();
    let bus = RuntimeEventBus {
        sender,
        state: Arc::new(Mutex::new(RuntimeEventState {
            journal: RuntimeEventJournal::default_with_stream("test:lane-source"),
            live_view: RuntimeViewState::new(viden_types::RuntimeSnapshot {
                cwd: std::path::PathBuf::from("workspace"),
                provider_family: String::new(),
                model_label: String::new(),
                work_mode: WorkMode::Build,
                permission_mode: viden_types::PermissionMode::Default,
                permission_level: viden_types::PermissionLevel::Ask,
                config_summary: String::new(),
                loaded_config_files: Vec::new(),
                startup_overrides: Vec::new(),
                ui_preferences: viden_types::ResolvedUiPreferences::default(),
            }),
            lane_agent_bindings: BTreeMap::new(),
            lane_agent_store: None,
        })),
    };
    let mut rows = Vec::new();
    for kind in facts {
        let unsampled = first_lane_worktree_to_sample(&bus, &kind);
        emit_event(&bus, RuntimeOwner::default(), kind);
        if let Some((lane_id, worktree)) = unsampled {
            let source = crate::frontend_status::sample_workspace_source(&worktree);
            emit_event(
                &bus,
                RuntimeOwner::default(),
                RuntimeEventKind::LaneSourceUpdated {
                    lane_id: lane_id.clone(),
                    source: source.clone(),
                },
            );
            rows.push((lane_id, source));
        }
    }
    rows
}

/// Fills a bare envelope owner in with the workspace identity Core published.
///
/// Only the two id fields, and only when the client named neither: an owner a
/// client did name is authoritative and is never rewritten. A command that
/// carries its own actor is skipped entirely, because the supervisor validates
/// that the actor equals the envelope owner — stamping one side would either
/// break that check or launder a mismatch past it.
///
/// This is what makes a Lane created after open carry the workspace and
/// project ids in its binding instead of a pair of empty strings. Facts the
/// built-in turn already publishes under the empty owner are re-stamped by C7,
/// not here.
pub(crate) fn stamp_workspace_identity(
    engine: &SessionEngine,
    owner: RuntimeOwner,
    command: &RuntimeCommand,
) -> RuntimeOwner {
    let Some(workspace) = engine.workspace_owner() else {
        return owner;
    };
    if !owner.workspace_id.is_empty() || !owner.project_id.is_empty() {
        return owner;
    }
    if crate::project_runtime::supervised_command_actor(command).is_some() {
        return owner;
    }
    RuntimeOwner {
        workspace_id: workspace.workspace_id.clone(),
        project_id: workspace.project_id.clone(),
        ..owner
    }
}

fn emit_frontend_status_events_if_changed(
    bus: &RuntimeEventBus,
    owner: RuntimeOwner,
    events: Vec<RuntimeEvent>,
) {
    for event in events {
        let unchanged = bus
            .state
            .lock()
            .ok()
            .is_some_and(|state| match &event.kind {
                RuntimeEventKind::WorkspaceSourceUpdated { source } => {
                    state.live_view.workspace_source.as_ref() == Some(source)
                }
                RuntimeEventKind::LaneSourceUpdated { lane_id, source } => {
                    state.live_view.lane_sources.get(lane_id) == Some(source)
                }
                RuntimeEventKind::RuntimeServiceHealthUpdated { service } => state
                    .live_view
                    .runtime_services
                    .iter()
                    .any(|existing| existing == service),
                _ => false,
            });
        if !unchanged {
            emit_known_event(bus, owner.clone(), event);
        }
    }
}

fn emit_error(bus: &RuntimeEventBus, owner: RuntimeOwner, message: String) {
    emit_event(
        bus,
        owner,
        RuntimeEventKind::Error {
            error: RuntimeErrorView {
                message,
                recoverable: true,
                hint: None,
            },
        },
    );
}

fn emit_event(bus: &RuntimeEventBus, owner: RuntimeOwner, kind: RuntimeEventKind) {
    let event = RuntimeEvent::new(0, kind);
    emit_known_event(bus, owner, event);
}

fn emit_known_event(bus: &RuntimeEventBus, owner: RuntimeOwner, event: RuntimeEvent) {
    let mut event = event;
    crate::frontend_status::bind_fact_owner(&mut event.kind, &owner);
    let Ok(mut state) = bus.state.lock() else {
        return;
    };
    let envelope = state.journal.record(RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner,
        cursor: EventCursor {
            stream_id: String::new(),
            sequence: event.sequence,
        },
        event: RuntimeWireEvent::Known(event),
    });
    if let RuntimeWireEvent::Known(event) = &envelope.event {
        state.live_view.apply_event(event);
    }
    // The send remains inside the journal critical section so concurrent
    // producers cannot make a later envelope visible first.
    let _ = bus.sender.send(envelope);
}

#[cfg(test)]
mod contract_freeze_tests {
    use super::*;
    use std::path::PathBuf;
    use viden_types::{
        AgentLaneRecord, AgentRole, AgentRoute, DataEgressPolicy, ExecutionTarget, GateStrength,
        LaneBudget, LaneRuntimeOwnerBinding, LaneStatus, MutationPolicy, PermissionMode,
        RecentProjectSummary, RecentSessionSummary, ResolvedUiPreferences, StarterLanePreview,
        StarterLanePreviewInvalidationReason, StarterLaneReceipt, UiColorMode, UiDensity, UiMotion,
        UiPreferences, UiSkin,
    };

    #[test]
    fn frontend_host_extension_journal_snapshot_and_replay_preserve_normal_facts() {
        let snapshot = viden_types::RuntimeSnapshot {
            cwd: PathBuf::from("workspace/project"),
            provider_family: "fallback".to_string(),
            model_label: "test-local".to_string(),
            work_mode: WorkMode::Build,
            permission_mode: PermissionMode::Default,
            permission_level: PermissionLevel::Ask,
            config_summary: String::new(),
            loaded_config_files: Vec::new(),
            startup_overrides: Vec::new(),
            ui_preferences: ResolvedUiPreferences::default(),
        };
        let (sender, _receiver) = mpsc::channel();
        let bus = RuntimeEventBus {
            sender,
            state: Arc::new(Mutex::new(RuntimeEventState {
                journal: RuntimeEventJournal::default_with_stream("fixture:frontend-host-services"),
                live_view: RuntimeViewState::new(snapshot.clone()),
                lane_agent_bindings: BTreeMap::new(),
                lane_agent_store: None,
            })),
        };
        let owner = RuntimeOwner {
            workspace_id: "workspace-host-fixture".to_string(),
            project_id: "project-host-fixture".to_string(),
            lane_id: Some("lane-host-fixture".to_string()),
            session_id: Some("session-host-fixture".to_string()),
            task_id: Some("task_host_fixture".to_string()),
            turn_id: Some("turn-host-fixture".to_string()),
        };
        let lane = AgentLaneRecord {
            id: "lane-host-fixture".to_string(),
            task_id: owner.task_id.clone(),
            role: AgentRole::Coder,
            route: AgentRoute::BuiltIn,
            gate_strength: GateStrength::Full,
            mutation_policy: MutationPolicy::ProposeOnly,
            worktree: Some("workspace/.worktrees/lane-host-fixture".to_string()),
            branch: Some("codex/lane-host-fixture".to_string()),
            target: ExecutionTarget::Local,
            data_egress: DataEgressPolicy::Deny,
            status: LaneStatus::Running,
            budget: LaneBudget::default(),
            active_session_ids: vec!["session-host-fixture".to_string()],
            summary: "reviewed starter Lane".to_string(),
            evidence: Vec::new(),
            run_stats: None,
        };
        let preview = StarterLanePreview {
            preview_id: "preview-host-fixture".to_string(),
            content_sha256: "ab".repeat(32),
            owner: owner.clone(),
            lane: lane.clone(),
            branch: "codex/lane-host-fixture".to_string(),
            worktree_path: "workspace/.worktrees/lane-host-fixture".to_string(),
            base_revision: "cd".repeat(20),
            diagnostics: Vec::new(),
        };
        let resolved = ResolvedUiPreferences {
            locale: viden_types::LocaleId::ZhCn,
            skin: UiSkin::Ice,
            mode: UiColorMode::Dark,
            density: UiDensity::Compact,
            motion: UiMotion::Reduced,
            diagnostics: Vec::new(),
        };
        let facts = vec![
            RuntimeEventKind::UiPreferencesUpdated {
                resolved: resolved.clone(),
                persisted: Some(UiPreferences {
                    locale: resolved.locale,
                    skin: resolved.skin,
                    mode: resolved.mode,
                    density: resolved.density,
                    motion: resolved.motion,
                }),
                diagnostics: Vec::new(),
            },
            RuntimeEventKind::RecentWorkLoaded {
                projects: vec![RecentProjectSummary {
                    canonical_root: "workspace/project".to_string(),
                    display_name: "project".to_string(),
                    last_updated_at: 20,
                    latest_session_id: owner.session_id.clone(),
                }],
                sessions: vec![RecentSessionSummary {
                    canonical_root: "workspace/project".to_string(),
                    session_id: owner.session_id.clone().unwrap(),
                    created_at: 10,
                    last_updated_at: 20,
                    message_count: 2,
                    tool_call_count: 1,
                    command_count: 1,
                }],
                diagnostics: Vec::new(),
            },
            RuntimeEventKind::StarterLanePreviewed {
                preview: preview.clone(),
            },
            RuntimeEventKind::StarterLaneCreated {
                receipt: StarterLaneReceipt {
                    preview_id: preview.preview_id.clone(),
                    content_sha256: preview.content_sha256.clone(),
                    lane,
                    branch: preview.branch.clone(),
                    worktree_path: preview.worktree_path.clone(),
                    base_revision: preview.base_revision.clone(),
                    owner: owner.clone(),
                },
            },
            RuntimeEventKind::StarterLanePreviewInvalidated {
                owner: owner.clone(),
                preview_id: preview.preview_id,
                reason: StarterLanePreviewInvalidationReason::BaseRevisionChanged,
            },
            RuntimeEventKind::LaneRuntimeOwnerBound {
                binding: LaneRuntimeOwnerBinding {
                    lane_id: "lane-host-fixture".to_string(),
                    owner: owner.clone(),
                },
            },
        ];
        for fact in facts {
            emit_event(&bus, owner.clone(), fact);
        }

        let state = bus.state.lock().unwrap();
        let snapshot_cursor = state.journal.current_cursor();
        let snapshot_view = state.live_view.clone();
        let replay = state
            .journal
            .replay(ReplayRequest {
                after: state.journal.initial_cursor(),
                limit: 100,
            })
            .unwrap();
        assert_eq!(replay.next, snapshot_cursor);
        assert!(replay.complete);
        assert_eq!(replay.events.len(), 6);
        assert!(
            replay
                .events
                .iter()
                .all(|envelope| matches!(envelope.event, RuntimeWireEvent::Known(_)))
        );
        drop(state);

        let mut replayed_view = RuntimeViewState::new(snapshot);
        for envelope in replay.events {
            if let RuntimeWireEvent::Known(event) = envelope.event {
                replayed_view.apply_event(&event);
            }
        }
        assert_eq!(replayed_view, snapshot_view);
        assert_eq!(
            replayed_view.ui_preferences.locale,
            viden_types::LocaleId::ZhCn
        );
        assert_eq!(replayed_view.recent_sessions.len(), 1);
        assert!(replayed_view.starter_lane_previews.is_empty());
        assert_eq!(replayed_view.starter_lane_receipts.len(), 1);
        assert_eq!(replayed_view.lane_runtime_owners.len(), 1);
    }
}
