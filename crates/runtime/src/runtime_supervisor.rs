use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
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
    LocalLaneEffectExecutor, StarterLaneAgentPolicy, WorkflowLanePersistence,
};
use viden_provider::ModelRequestControl;
use viden_types::{
    AgentAdapterView, AgentLaneRecord, AgentSessionControlView, AgentSessionRequest,
    AgentSessionStatus, AgentSessionTermination, AgentSessionView, ApprovalDecision,
    ApprovalDefaultAction, ApprovalRequestView, ApprovalResponse, ApprovalRisk, ApprovalScope,
    ApprovalTarget, AuditActor, AuditObjectRef, AuditOutcome, AuditRecord, CapabilityId,
    EventCursor, FRONTEND_SCHEMA_V1, FRONTEND_V1_CAPABILITIES, FRONTEND_V1_EXTENSION_CAPABILITIES,
    GapRecovery, LaneRuntimeOwnerBinding, OperatorGitAction, PermissionLevel, PermissionPrompt,
    ReplayBatch, ReplayRequest, RuntimeCommand, RuntimeCommandEnvelope, RuntimeErrorView,
    RuntimeEvent, RuntimeEventEnvelope, RuntimeEventKind, RuntimeOwner, RuntimeSnapshotEnvelope,
    RuntimeViewState, RuntimeWireEvent, StandingApprovalView, StarterLaneAgentBinding,
    StarterLaneSessionOutcome, ToolInput, TranscriptPage, TranscriptPageRequest, TurnOutcome,
    TurnSource, TurnView, WorkMode, fresh_id, now_timestamp,
};
use viden_workflows::stores::WorkflowStore;

use viden_agents::{
    AgentSessionApprover, cancel_typed_agent_session, cancelled_session_control,
    kill_typed_agent_session, mark_typed_agent_session_status, pause_typed_agent_session,
    probe_typed_agent_adapter, resume_typed_agent_session, retry_typed_agent_session,
    shutdown_resident_acp_sessions, start_typed_agent_session, typed_agent_adapter_views,
    typed_agent_session_request_from_compat_input, unknown_typed_agent_adapter_reason,
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
    /// Whether this request offered `ApprovalDecision::Edit`
    /// (`runtime.approval_scopes`, GUI-CORE-019). An `Edit` answering a
    /// request that did not offer one is rejected, exactly like a scope that
    /// was not offered.
    editable: bool,
    /// The exact input the operator was shown. The standing rule an `Always`
    /// grant persists is keyed on it, and a revision equal to it is rejected
    /// `revision_unchanged` rather than re-prompted.
    tool_input: ToolInput,
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
    /// A supervised Agent session turn completed; forward the next input the
    /// operator typed while it was paused, if any
    /// (`runtime.agent_session_control`).
    ///
    /// Posted by the session's own event sink, which runs on the adapter's
    /// monitor thread and therefore cannot touch the worker-owned held-input
    /// map. This is the same arm-then-drain shape C6 uses for the session
    /// queue, with the arming signal crossing a thread boundary.
    DrainHeldSessionInput {
        owner: RuntimeOwner,
        session_id: String,
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
    /// A session follow-up the accept path already pushed and announced.
    ///
    /// It carries no content: the entry is in Core's queue before this message
    /// is sent (compatibility follow-up 16). The worker only has to decide
    /// whether the drain was already armed by a completed turn, which is the
    /// one thing the intake side cannot know.
    SessionFollowUpQueued {
        owner: RuntimeOwner,
    },
    Snapshot {
        response: Sender<Result<RuntimeSnapshotEnvelope, String>>,
    },
    TranscriptPage {
        request: TranscriptPageRequest,
        response: Sender<Result<TranscriptPage, String>>,
    },
    /// Starts the session a starter Lane's Agent binding asked for
    /// (`runtime.starter_lane_agent`, GUI-CORE-030).
    ///
    /// Distinct from a plain `StartAgentSession` because the session id was
    /// minted and its Lane binding reserved on the creation path, so the
    /// receipt could name it: the worker must adopt that id rather than mint
    /// a second one the receipt would not match.
    StartStarterLaneSession {
        owner: RuntimeOwner,
        command_id: String,
        request: AgentSessionRequest,
        session_id: String,
    },
    /// Removes a revoked standing rule from the live permission engine
    /// (`runtime.approval_scopes`, GUI-CORE-019).
    ///
    /// The durable removal, the audit row, and the fact are all done by the
    /// time this is sent; the worker owns the engine and nothing else does.
    RevokeStandingRule {
        owner: RuntimeOwner,
        tool_name: String,
        rule_content: String,
    },
    /// One adapter handshake finished on its own thread
    /// (`runtime.agent_adapters`, GUI-CORE-035).
    ///
    /// The same posting shape `DrainHeldSessionInput` uses: the blocking work
    /// happened off the worker, and only the worker turns the result into a
    /// published fact. Carries the adapter id rather than the requesting
    /// command, because one handshake may answer several coalesced probes.
    AgentAdapterProbed {
        agent_id: String,
        result: Box<Result<AgentAdapterView, String>>,
    },
    Shutdown {
        response: Option<Sender<()>>,
    },
}

/// How many adapter handshakes may run at once (GUI-CORE-035).
///
/// A probe is a subprocess that can hold its stdout for the full ACP handshake
/// timeout — 30 s local, 90 s for a registry package started through `npx -y` —
/// so an unbounded fan-out would trade one blocked worker for an unbounded
/// number of blocked children. Four covers every adapter a client lists today
/// in one pass; a fifth waits for a slot rather than spawning.
pub(crate) const MAX_CONCURRENT_ADAPTER_PROBES: usize = 4;

/// One accepted `ProbeAgentAdapter` still waiting for its adapter's handshake.
pub(crate) struct AgentAdapterProbeWaiter {
    pub(crate) owner: RuntimeOwner,
    pub(crate) command_id: String,
}

/// The worker's off-worker adapter-probe scheduler (GUI-CORE-035).
///
/// Worker-local and therefore lock-free: every mutation happens on the command
/// worker, either when a probe is accepted or when a handshake thread posts
/// `SupervisorMessage::AgentAdapterProbed` back. The handshake itself is the
/// only part that runs elsewhere.
///
/// Coalescing is keyed on the adapter id, not the command id: two probes of the
/// same adapter share one handshake and therefore one `AgentAdapterProbed`
/// fact, while each keeps its own `CommandAccepted`. Probing the same adapter
/// twice would spawn the same process twice and answer with the same view, so
/// the second spawn buys a client nothing and costs it a process.
#[derive(Default)]
pub(crate) struct AgentAdapterProbes {
    /// Every adapter with an unfinished probe, and the commands waiting on it.
    waiters: BTreeMap<String, Vec<AgentAdapterProbeWaiter>>,
    /// The subset whose handshake thread is alive.
    running: BTreeSet<String>,
    /// Adapters admitted but not yet started, in arrival order.
    queued: VecDeque<String>,
}

impl AgentAdapterProbes {
    /// Registers one accepted probe and answers whether its handshake starts
    /// now.
    ///
    /// Pure bookkeeping, deliberately separate from spawning: `false` means the
    /// probe was either coalesced onto an adapter already pending or queued
    /// behind the concurrency bound, and in both cases the caller starts
    /// nothing.
    pub(crate) fn admit(&mut self, agent_id: String, waiter: AgentAdapterProbeWaiter) -> bool {
        let already_pending = self.waiters.contains_key(&agent_id);
        self.waiters
            .entry(agent_id.clone())
            .or_default()
            .push(waiter);
        if already_pending {
            return false;
        }
        if self.running.len() < MAX_CONCURRENT_ADAPTER_PROBES {
            self.running.insert(agent_id);
            return true;
        }
        self.queued.push_back(agent_id);
        false
    }

    /// Releases one finished adapter and answers with the commands that were
    /// waiting on it plus the adapters a freed slot admits, in arrival order.
    pub(crate) fn release(
        &mut self,
        agent_id: &str,
    ) -> (Vec<AgentAdapterProbeWaiter>, Vec<String>) {
        self.running.remove(agent_id);
        let waiters = self.waiters.remove(agent_id).unwrap_or_default();
        let mut started = Vec::new();
        while self.running.len() < MAX_CONCURRENT_ADAPTER_PROBES {
            let Some(next) = self.queued.pop_front() else {
                break;
            };
            if self.waiters.contains_key(&next) {
                self.running.insert(next.clone());
                started.push(next);
            }
        }
        (waiters, started)
    }

    /// Records one accepted probe and starts its handshake when a slot is free.
    fn submit(
        &mut self,
        cwd: &Path,
        agent_id: String,
        waiter: AgentAdapterProbeWaiter,
        worker_commands: &Sender<SupervisorMessage>,
    ) {
        if self.admit(agent_id.clone(), waiter) {
            self.start(cwd, agent_id, worker_commands);
        }
    }

    /// Spawns the blocking handshake for one adapter.
    ///
    /// Detached on purpose: the thread is bounded by the ACP handshake timeout
    /// and kills its child on the way out, so a shutdown that stops reading its
    /// result leaks neither a process nor an unbounded wait.
    fn start(&self, cwd: &Path, agent_id: String, worker_commands: &Sender<SupervisorMessage>) {
        let cwd = cwd.to_path_buf();
        let commands = worker_commands.clone();
        thread::spawn(move || {
            let result = probe_typed_agent_adapter(&cwd, &agent_id);
            let _ = commands.send(SupervisorMessage::AgentAdapterProbed {
                agent_id,
                result: Box::new(result),
            });
        });
    }

    /// Releases one finished adapter, admits the next queued one, and returns
    /// the commands that were waiting on it.
    fn finish(
        &mut self,
        cwd: &Path,
        agent_id: &str,
        worker_commands: &Sender<SupervisorMessage>,
    ) -> Vec<AgentAdapterProbeWaiter> {
        let (waiters, started) = self.release(agent_id);
        for agent_id in started {
            self.start(cwd, agent_id, worker_commands);
        }
        waiters
    }
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
        self.record_decision_for(
            audit_id, owner, request_id, tool_name, job_id, None, decision,
        )
    }

    /// [`Self::record_decision`] with the lane the decision released.
    ///
    /// Only the lane branch names one: a runtime-owned approval's objects are
    /// the request, the tool, and the job, and adding a lane object there
    /// would move rows every recorded fixture already holds.
    #[allow(clippy::too_many_arguments)]
    fn record_decision_for(
        &self,
        audit_id: &str,
        owner: &RuntimeOwner,
        request_id: &str,
        tool_name: &str,
        job_id: Option<&str>,
        lane_id: Option<&str>,
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
        if let Some(lane_id) = lane_id.filter(|id| !id.is_empty()) {
            objects.push(AuditObjectRef::new(AuditObjectRef::KIND_LANE, lane_id));
        }
        let mut args = BTreeMap::new();
        args.insert(
            "scope".to_string(),
            approval_scope_key(decision).to_string(),
        );
        // An `approval.edit` row that named only the tool would not say what
        // the operator changed it to, which is the whole content of the
        // decision (`runtime.approval_scopes`, GUI-CORE-019). The preview is
        // bounded for the same reason every other audit arg is.
        if let ApprovalDecision::Edit { revised_input } = decision {
            args.insert(
                "revised_input".to_string(),
                bounded_audit_preview(&viden_types::encode_tool_input(revised_input)),
            );
        }
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

    /// Appends the durable row for one operator session-control act
    /// (`runtime.agent_session_control`).
    ///
    /// Pause, resume, and kill change what an Agent session is allowed to do,
    /// so each is audited like any other operator mutation: actor `Operator`,
    /// and both the session and the Lane it belongs to linked, because an
    /// operator reading the Lane's timeline must see the session it controlled.
    fn record_session_control(
        &self,
        audit_id: &str,
        owner: &RuntimeOwner,
        action: &str,
        session_id: &str,
        lane_id: &str,
    ) -> Result<(), String> {
        let record = AuditRecord::sanitized(
            audit_id.to_string(),
            now_timestamp(),
            owner.clone(),
            AuditActor::Operator,
            format!("agent_session.{action}"),
            vec![
                AuditObjectRef::new(AuditObjectRef::KIND_SESSION, session_id),
                AuditObjectRef::new(AuditObjectRef::KIND_LANE, lane_id),
            ],
            AuditOutcome::Success,
            BTreeMap::new(),
        )?;
        self.workflows.append_audit_record(&record)
    }
}

/// Upper bound on the inputs one paused session may hold.
///
/// The same bound the runtime view already caps its collections at: a paused
/// session must not become an unbounded queue an operator cannot see the end
/// of, and a client renders the count from the view it is capped by.
const MAX_HELD_SESSION_INPUTS: usize = 50;

impl ApprovalAuditLog {
    /// Appends the durable row for one standing-approval revocation
    /// (`approval.revoke`).
    ///
    /// The objects are the rule and the tool it governed, so the row answers
    /// "what stopped being allowed" rather than only "a revocation happened".
    fn record_revocation(
        &self,
        audit_id: &str,
        owner: &RuntimeOwner,
        rule_id: &str,
        tool_name: &str,
    ) -> Result<(), String> {
        let record = AuditRecord::sanitized(
            audit_id.to_string(),
            now_timestamp(),
            owner.clone(),
            AuditActor::Operator,
            "approval.revoke".to_string(),
            vec![
                AuditObjectRef::new(AuditObjectRef::KIND_PERMISSION, rule_id),
                AuditObjectRef::new("tool", tool_name),
            ],
            AuditOutcome::Success,
            BTreeMap::from([("scope".to_string(), "always".to_string())]),
        )?;
        self.workflows.append_audit_record(&record)
    }
}

/// Upper bound on an audit argument's rendered value.
const MAX_AUDIT_ARG_CHARS: usize = 256;

/// Bounds one audit argument. Truncation is marked so a reader can tell a
/// short value from a shortened one.
fn bounded_audit_preview(value: &str) -> String {
    if value.chars().count() <= MAX_AUDIT_ARG_CHARS {
        return value.to_string();
    }
    let kept: String = value.chars().take(MAX_AUDIT_ARG_CHARS).collect();
    format!("{kept}...")
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
        // The two scopes `runtime.approval_scopes` adds. `always` is the
        // standing grant; `edit` is a decision in its own right, because the
        // revision it carries is re-decided and may still be refused.
        ApprovalDecision::Allow {
            scope: ApprovalScope::Always,
        } => "always",
        ApprovalDecision::Edit { .. } => "edit",
        ApprovalDecision::Deny => "deny",
    }
}

pub struct RuntimeSupervisor {
    workspace_root: PathBuf,
    /// The Viden home the standing-approval settings file lives under
    /// (`runtime.approval_scopes`, GUI-CORE-019). Taken from the engine so a
    /// runtime started against a scratch home never writes the operator's.
    viden_home: PathBuf,
    commands: Sender<SupervisorMessage>,
    events: Receiver<RuntimeEventEnvelope>,
    event_bus: RuntimeEventBus,
    active_control: ActiveControlRegistry,
    pending_approvals: Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    approval_timers: Arc<ApprovalTimerRegistry>,
    lane_supervisor: Arc<LaneSupervisor>,
    permission_control: Arc<Mutex<PermissionControlState>>,
    approval_audit: ApprovalAuditLog,
    /// Core's session follow-up queue, shared with the engine on the worker
    /// thread so the accept path can acknowledge an entry while a turn runs
    /// (compatibility follow-up 16).
    session_queue: crate::runtime_contract::SessionInputQueue,
    worker_alive: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

/// The runtime half of the starter-Lane Agent binding
/// (`runtime.starter_lane_agent`, GUI-CORE-030).
///
/// Injected into `LaneSupervisor`, which owns Lane creation and knows nothing
/// about adapters or Agent sessions. Everything policy-shaped stays here: the
/// adapter registry, the mutation gate, the Lane-binding reservation, and the
/// dispatch onto the worker that owns the engine.
struct SupervisorStarterLaneAgentPolicy {
    commands: Sender<SupervisorMessage>,
    event_bus: RuntimeEventBus,
    active_control: ActiveControlRegistry,
    permission_control: Arc<Mutex<PermissionControlState>>,
}

impl SupervisorStarterLaneAgentPolicy {
    /// The refusal path, written once so every reason reaches the receipt in
    /// the same shape a client can render.
    fn refuse(reason: impl Into<String>) -> StarterLaneSessionOutcome {
        StarterLaneSessionOutcome::Refused {
            reason: reason.into(),
        }
    }
}

impl StarterLaneAgentPolicy for SupervisorStarterLaneAgentPolicy {
    fn adapter_is_known(&self, agent_id: &str) -> bool {
        typed_agent_adapter_views()
            .iter()
            .any(|adapter| adapter.agent_id == agent_id)
    }

    fn start_session(
        &self,
        owner: &RuntimeOwner,
        lane: &AgentLaneRecord,
        binding: &StarterLaneAgentBinding,
    ) -> StarterLaneSessionOutcome {
        // The same checks `StartAgentSession` runs, in the same order, so the
        // two entry points cannot disagree about whether a session may start.
        // They run *after* the Lane exists, and every failure is a receipt
        // field rather than an unwind: a Lane that was created stays created.
        if !self.adapter_is_known(&binding.agent_id) {
            return Self::refuse(format!("agent adapter `{}` is not known", binding.agent_id));
        }
        if self
            .permission_control
            .lock()
            .map(|control| control.applied.blocks_mutation())
            .unwrap_or(true)
        {
            return Self::refuse(
                "agent session execution requires Build mode with Ask or Autonomous permission",
            );
        }
        let lane_owner = RuntimeOwner {
            lane_id: Some(lane.id.clone()),
            session_id: None,
            ..owner.clone()
        };
        match lane_agent_session_binding(&self.event_bus, &lane_owner) {
            Ok(Some(existing)) => {
                return Self::refuse(lane_agent_session_binding_rejection(&existing));
            }
            Ok(None) => {}
            Err(error) => return Self::refuse(error),
        }
        if let Some(owner_id) = active_agent_lane_owner_id(&self.active_control, &lane_owner) {
            return Self::refuse(format!(
                "active agent session `{owner_id}` is already running for this lane"
            ));
        }
        let request = AgentSessionRequest {
            lane_id: lane.id.clone(),
            agent_id: binding.agent_id.clone(),
            model: binding.model.clone(),
            load_session_id: None,
            // The preset's summary is the honest default objective for a
            // wizard that offered no task field.
            task: binding
                .task
                .clone()
                .filter(|task| !task.trim().is_empty())
                .unwrap_or_else(|| lane.summary.clone()),
        };
        if let Err(reason) = validate_typed_agent_session_request(&request) {
            return Self::refuse(reason);
        }
        // Minted here so the receipt can name the session it started. The
        // binding is reserved in the same step, which is also what makes the
        // reservation the one place two concurrent starts collide.
        let session_id = fresh_id("agent-session");
        if let Err(reason) = reserve_lane_agent_session_binding(
            &self.event_bus,
            &request.lane_id,
            &request.agent_id,
            &session_id,
        ) {
            return Self::refuse(reason);
        }
        if let Err(error) = self
            .commands
            .send(SupervisorMessage::StartStarterLaneSession {
                owner: lane_owner,
                command_id: fresh_id("command"),
                request,
                session_id: session_id.clone(),
            })
        {
            return Self::refuse(format!("runtime supervisor stopped: {error}"));
        }
        StarterLaneSessionOutcome::Started { session_id }
    }
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

    /// Writes the standing rule an `Allow { Always }` grants, or degrades the
    /// answer when it cannot be written.
    ///
    /// Returns the response to carry on with and the rule to announce. A
    /// non-`Always` answer passes through untouched, which is what keeps
    /// every other decision byte-identical to the pre-capability path.
    fn persist_standing_grant(
        &self,
        response: ApprovalResponse,
        owner: &RuntimeOwner,
        audit_id: &str,
        tool_name: &str,
        tool_input: &ToolInput,
    ) -> (ApprovalResponse, Option<StandingApprovalView>) {
        if !matches!(
            response.decision,
            ApprovalDecision::Allow {
                scope: ApprovalScope::Always
            }
        ) {
            return (response, None);
        }
        let rule = StandingApprovalView {
            rule_id: fresh_id("rule"),
            tool_name: tool_name.to_string(),
            // The same encoding a `Session` rule uses, so the standing list
            // and the session rules describe one vocabulary.
            rule_content: viden_types::encode_tool_input(tool_input),
            granted_at: now_timestamp(),
            granted_in_workspace: owner.workspace_id.clone(),
            audit_id: audit_id.to_string(),
        };
        let path = viden_config::user_settings_path_in_home(&self.viden_home);
        match viden_config::grant_user_standing_approval_at(&path, &rule) {
            Ok(()) => (response, Some(rule)),
            Err(error) => {
                emit_error(
                    &self.event_bus,
                    owner.clone(),
                    format!("standing approval was not persisted, granting once instead: {error}"),
                );
                (ApprovalResponse::allow_once(response.feedback), None)
            }
        }
    }

    /// Answers `RevokeStandingApproval`.
    ///
    /// The file is the authority: a rule id the file does not hold is
    /// rejected rather than answered with a revocation that removed nothing.
    /// The durable removal happens first, then the audit row, then the fact,
    /// and only then is the live engine told — so a crash between steps leaves
    /// a rule that is gone from disk and will be gone from the engine at the
    /// next bootstrap, never the reverse.
    fn revoke_standing_approval(
        &self,
        owner: RuntimeOwner,
        command_id: String,
        rule_id: String,
    ) -> Result<(), String> {
        let path = viden_config::user_settings_path_in_home(&self.viden_home);
        let removed = match viden_config::revoke_user_standing_approval_at(&path, &rule_id) {
            Ok(Some(rule)) => rule,
            Ok(None) => {
                emit_event(
                    &self.event_bus,
                    owner,
                    RuntimeEventKind::CommandRejected {
                        command_id,
                        reason: format!("standing approval `{rule_id}` is not in force"),
                    },
                );
                return Ok(());
            }
            Err(error) => {
                emit_event(
                    &self.event_bus,
                    owner,
                    RuntimeEventKind::CommandRejected {
                        command_id,
                        reason: error,
                    },
                );
                return Ok(());
            }
        };
        let audit_id = fresh_id("audit");
        emit_event(
            &self.event_bus,
            owner.clone(),
            RuntimeEventKind::CommandAccepted {
                command_id,
                command: redacted_runtime_command_for_event(
                    &RuntimeCommand::RevokeStandingApproval {
                        rule_id: rule_id.clone(),
                    },
                ),
            },
        );
        // Audit before the fact that announces it, the same order the grant
        // uses. Fail-open on the decision, not on the audit: the rule is
        // already off disk, and refusing to say so would be the worse lie.
        if let Err(error) =
            self.approval_audit
                .record_revocation(&audit_id, &owner, &rule_id, &removed.tool_name)
        {
            emit_error(
                &self.event_bus,
                owner.clone(),
                format!("standing approval was revoked but not audited: {error}"),
            );
        }
        emit_event(
            &self.event_bus,
            owner.clone(),
            RuntimeEventKind::StandingApprovalRevoked {
                rule_id,
                audit_id,
                owner: owner.clone(),
            },
        );
        // The live engine last, on the worker thread that owns it.
        self.commands
            .send(SupervisorMessage::RevokeStandingRule {
                owner,
                tool_name: removed.tool_name,
                rule_content: removed.rule_content,
            })
            .map_err(|err| format!("runtime supervisor stopped: {err}"))
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
        // Standing `Always` approvals are the one permission fact that
        // survives a restart (`runtime.approval_scopes`, GUI-CORE-019). They
        // are loaded here, before the engine moves onto the worker thread, so
        // the first `decide` of the process already answers `RuleAllow` for a
        // rule the operator granted in an earlier session — in *any*
        // workspace, which is what separates this scope from every other one.
        //
        // A rule the file cannot describe is reported and skipped rather than
        // taking the table down: a single malformed row must not silently
        // re-open prompts an operator believes they silenced.
        let mut standing_approvals = Vec::new();
        let mut standing_diagnostics = Vec::new();
        let viden_home = engine.home_dir().to_path_buf();
        match viden_config::load_user_standing_approvals_at(
            &viden_config::user_settings_path_in_home(&viden_home),
        ) {
            Ok((rules, diagnostics)) => {
                for rule in &rules {
                    engine.install_standing_approval(&rule.tool_name, &rule.rule_content);
                }
                standing_approvals = rules;
                standing_diagnostics = diagnostics;
            }
            Err(error) => {
                standing_diagnostics.push(format!("standing approvals were not loaded: {error}"));
            }
        }
        let mut live_view = engine.runtime_view_state();
        live_view.standing_approvals = standing_approvals;
        let lane_agent_store = engine.workflow_store();
        // Cloned before the engine moves onto the worker thread: the handle the
        // approval decision's durable row is written through.
        let approval_audit = ApprovalAuditLog {
            workflows: engine.workflow_store(),
        };
        // Same rule, same reason: taken before the move so the intake side of
        // the channel writes into the one queue the drain reads from.
        let session_queue = engine.session_input_queue();
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
        for diagnostic in standing_diagnostics {
            emit_error(&event_bus, RuntimeOwner::default(), diagnostic);
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
        let lane_supervisor = Arc::new(
            LaneSupervisor::new(
                lane_repo,
                lane_persistence,
                lane_permissions,
                lane_effects,
                lane_events,
                lane_approvals,
                lane_redact_command,
                lane_mode,
                approval_ttl_secs,
            )
            .with_starter_lane_agent_policy(Arc::new(
                SupervisorStarterLaneAgentPolicy {
                    commands: command_sender.clone(),
                    event_bus: event_bus.clone(),
                    active_control: Arc::clone(&active_control),
                    permission_control: Arc::clone(&permission_control),
                },
            )),
        );
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

        let worker_commands = command_sender.clone();
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
                worker_commands,
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
            viden_home,
            commands: command_sender,
            events: event_receiver,
            event_bus,
            active_control,
            pending_approvals,
            approval_timers,
            lane_supervisor,
            permission_control,
            approval_audit,
            session_queue,
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
            // Compatibility follow-up 16. The supervisor is one worker, so a
            // follow-up sent while a turn runs used to sit in that worker's
            // channel: `queued_inputs` stayed empty for the whole wait and
            // `InputQueued` and `InputDequeued` arrived in the same instant,
            // which is what left the composer showing "working" instead of
            // "queued". The entry is now pushed onto Core's queue and announced
            // here, on the accept path, and the worker is told only that it may
            // need to drain.
            //
            // Session scope only. A Lane's follow-ups are held and drained by
            // its own worker, and `drain_session_queue` refuses a Lane owner
            // for exactly that reason.
            RuntimeCommand::QueueFollowUp { content } if owner.lane_id.is_none() => {
                let input = self.session_queue.push(content.clone());
                emit_event(
                    &self.event_bus,
                    owner.clone(),
                    RuntimeEventKind::CommandAccepted {
                        command_id,
                        command: redacted_runtime_command_for_event(
                            &RuntimeCommand::QueueFollowUp { content },
                        ),
                    },
                );
                emit_event(
                    &self.event_bus,
                    owner.clone(),
                    RuntimeEventKind::InputQueued { input },
                );
                return self
                    .commands
                    .send(SupervisorMessage::SessionFollowUpQueued { owner })
                    .map_err(|err| format!("runtime supervisor stopped: {err}"));
            }
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
            // Every session-control command answers the same two questions
            // before the worker ever sees it: is the session known, and does
            // the commanding owner hold it. Neither depends on which control
            // was asked for, so they are decided once here.
            RuntimeCommand::CancelAgentSession { ref session_id }
            | RuntimeCommand::PauseAgentSession { ref session_id }
            | RuntimeCommand::ResumeAgentSession { ref session_id }
            | RuntimeCommand::KillAgentSession { ref session_id } => {
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
            RuntimeCommand::RevokeStandingApproval { rule_id } => {
                return self.revoke_standing_approval(owner, command_id, rule_id);
            }
            RuntimeCommand::RespondToApproval {
                request_id,
                response,
            } => {
                if let Some((pending, lane_permission_epoch)) =
                    self.lane_supervisor.pending_approval(&request_id)?
                {
                    if pending.owner != owner {
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
                    // Compatibility follow-up 15. Before this, the lane branch
                    // returned here and never reached `record_decision` below,
                    // so a `lane_create` approval published an `audit_id` the
                    // durable timeline had no row for. The row is written on
                    // the runtime branch — audit stays runtime-owned policy and
                    // `crates/lanes` gains no writer — and it is written
                    // *before* the response is forwarded, because the worker
                    // emits `ApprovalResolved` carrying this id as soon as it
                    // picks the message up.
                    //
                    // The decision recorded is the lane worker's own intake
                    // verdict for this answer, read through the same rule the
                    // worker applies, plus the runtime's mutation block: an
                    // expired, stale-epoch, or out-of-scope "allow" is enforced
                    // as a denial and is recorded as one. What can still
                    // diverge is the worker's injected permission-gate
                    // re-check, which runs on its own thread after this point;
                    // that refusal is published as `ApprovalResolved { Deny }`
                    // and is not re-recorded here, because a second row under
                    // one pre-minted id would break the receipt join.
                    let blocks_mutation = self
                        .permission_control
                        .lock()
                        .map_err(|_| "permission control state poisoned".to_string())?
                        .blocks_mutation();
                    let intake = pending.intake(&response, now_timestamp(), lane_permission_epoch);
                    let decision = if intake.is_allowed() && !blocks_mutation {
                        response.decision.clone()
                    } else {
                        ApprovalDecision::Deny
                    };
                    if let Err(error) = self.approval_audit.record_decision_for(
                        &pending.audit_id,
                        &pending.owner,
                        &request_id,
                        &pending.tool_name,
                        Some(pending.command_id.as_str()),
                        Some(pending.lane_id.as_str()),
                        &decision,
                    ) {
                        // Fail-open on the decision, not on the audit, exactly
                        // as the runtime-owned path does: an operator's answer
                        // is still delivered, and the failure is visible.
                        emit_error(
                            &self.event_bus,
                            pending.owner.clone(),
                            format!("approval decision was applied but not audited: {error}"),
                        );
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
                                // The stable reason `runtime.approval_scopes`
                                // names for an `Always` a request did not
                                // offer, and for an `Edit` on a tool whose
                                // input Core will not let an operator revise.
                                reason: format!(
                                    "approval request `{request_id}` scope_not_offered"
                                ),
                            },
                        );
                        return Ok(());
                    }
                    // A revision that changes nothing is not a decision: it
                    // would re-run `decide` over the same bytes and produce
                    // the same `Ask`, leaving the operator in a loop with a
                    // prompt they already answered. The request stays pending
                    // so the real answer can still arrive.
                    if let ApprovalDecision::Edit { revised_input } = &response.decision
                        && *revised_input == pending.tool_input
                    {
                        emit_event(
                            &self.event_bus,
                            owner.clone(),
                            RuntimeEventKind::CommandRejected {
                                command_id,
                                reason: format!(
                                    "approval request `{request_id}` revision_unchanged"
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
                // A revision is treated like an allow here on purpose: it asks
                // Core to run bytes, so it must clear the same two gates an
                // allow clears. Plan mode refusing a revision is the
                // `Deny plan_mode` the design names, and it is enforced on the
                // request rather than left to the re-decision, so the original
                // resolves as the refusal the operator has to read.
                let revises = matches!(response.decision, ApprovalDecision::Edit { .. });
                let response = if (response.is_allowed() || revises)
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
                        } else if revises {
                            "plan_mode"
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
                // Durable first. The rule reaches the user-level settings file
                // before `StandingApprovalGranted` announces it, so a client
                // never renders a standing rule this machine would not load at
                // its next bootstrap. A file that cannot be written degrades
                // the answer to `Allow Once` rather than publishing a grant
                // nothing holds — the operator sees the error and the call
                // still runs.
                let (response, granted_rule) = self.persist_standing_grant(
                    response,
                    &pending_owner,
                    &pending_audit_id,
                    &pending.tool_name,
                    &pending.tool_input,
                );
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
                // After the audit row, before `ApprovalResolved`: the grant is
                // the reason the decision was `always`, so a client reading
                // the stream sees the rule exist before the decision that
                // installed it settles.
                if let Some(rule) = granted_rule {
                    emit_event(
                        &self.event_bus,
                        pending_owner.clone(),
                        RuntimeEventKind::StandingApprovalGranted { rule },
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

    /// The live reduced view, read without going through the command channel.
    ///
    /// `snapshot_envelope` is answered by the worker, so it cannot be used to
    /// observe state *while* a turn occupies that worker — which is exactly
    /// what compatibility follow-up 16 is about.
    #[cfg(test)]
    pub(crate) fn live_view_for_test(&self) -> RuntimeViewState {
        self.event_bus
            .state
            .lock()
            .expect("runtime event state")
            .live_view
            .clone()
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
    // The worker's own handle back into its queue: an Agent session's sink
    // runs on the adapter monitor thread and posts the held-input drain here
    // rather than reaching into worker-owned state.
    worker_commands: Sender<SupervisorMessage>,
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
    // Inputs accepted while a session is paused, in arrival order
    // (`runtime.agent_session_control`).
    //
    // Worker-local because every command that reads or writes it — the input
    // that is held, the resume that forwards one, the kill that discards them —
    // is handled on this thread, and because a held input is not a durable
    // fact: it is a prompt that has not been sent, and a Core that restarts
    // never sends it.
    let mut held_session_inputs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // The durable audit handle for session control. Built from the same store
    // the engine owns, before the engine is borrowed by the command loop.
    let session_audit = ApprovalAuditLog {
        workflows: engine.workflow_store(),
    };
    // Adapter handshakes in flight (`runtime.agent_adapters`, GUI-CORE-035).
    //
    // Worker-local for the same reason `held_session_inputs` is: every read
    // and write happens on this thread, and a probe is not a durable fact — a
    // Core that restarts has nothing to resume.
    let mut adapter_probes = AgentAdapterProbes::default();
    while let Ok(message) = command_receiver.recv() {
        match message {
            SupervisorMessage::StartStarterLaneSession {
                owner,
                command_id,
                request,
                session_id,
            } => {
                install_runtime_event_sink(&mut engine, event_bus.clone(), owner.clone());
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
                    &worker_commands,
                    Some(session_id),
                );
            }
            SupervisorMessage::RevokeStandingRule {
                owner,
                tool_name,
                rule_content,
            } => {
                // Idempotent by construction: the file already lost the rule,
                // so an engine that no longer holds it is the state the caller
                // asked for, not a failure to report.
                if !engine.revoke_standing_approval(&tool_name, &rule_content) {
                    emit_error(
                        &event_bus,
                        owner,
                        format!(
                            "standing approval for `{tool_name}` was revoked from settings but \
                             was not in force in this session"
                        ),
                    );
                }
            }
            // One handshake, one fact. A probe coalesced behind this adapter
            // acknowledged its own command and shares this result; publishing a
            // second identical fact per waiter would make a client believe two
            // handshakes happened.
            SupervisorMessage::AgentAdapterProbed { agent_id, result } => {
                let cwd = engine.cwd().to_path_buf();
                let waiters = adapter_probes.finish(&cwd, &agent_id, &worker_commands);
                match *result {
                    Ok(adapter) => {
                        if let Some(first) = waiters.first() {
                            emit_event(
                                &event_bus,
                                first.owner.clone(),
                                RuntimeEventKind::AgentAdapterProbed { adapter },
                            );
                        }
                    }
                    // Only an adapter that left the registry between acceptance
                    // and the handshake reaches this, and it is still an answer
                    // per waiter rather than silence.
                    Err(reason) => {
                        for waiter in waiters {
                            emit_event(
                                &event_bus,
                                waiter.owner,
                                RuntimeEventKind::CommandRejected {
                                    command_id: waiter.command_id,
                                    reason: reason.clone(),
                                },
                            );
                        }
                    }
                }
            }
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
                // A probe is a subprocess handshake, so it never occupies this
                // worker (`runtime.agent_adapters`, GUI-CORE-035). It is
                // accepted here, runs on its own thread, and comes back as
                // `SupervisorMessage::AgentAdapterProbed`. Everything else this
                // worker owes an answer to keeps being answered meanwhile,
                // which is the whole point: before this, an uninstalled
                // registry adapter held every other command for up to 90 s.
                if let RuntimeCommand::ProbeAgentAdapter { agent_id } = &command {
                    let agent_id = agent_id.clone();
                    // Resolution is in-process, so an unknown id is still
                    // refused synchronously and exactly as before: a plain
                    // `CommandRejected` with no acceptance in front of it.
                    if let Some(reason) = unknown_typed_agent_adapter_reason(&agent_id) {
                        emit_event(
                            &event_bus,
                            owner,
                            RuntimeEventKind::CommandRejected { command_id, reason },
                        );
                        continue;
                    }
                    emit_event(
                        &event_bus,
                        owner.clone(),
                        RuntimeEventKind::CommandAccepted {
                            command_id: command_id.clone(),
                            command: redacted_runtime_command_for_event(&command),
                        },
                    );
                    let cwd = engine.cwd().to_path_buf();
                    adapter_probes.submit(
                        &cwd,
                        agent_id,
                        AgentAdapterProbeWaiter { owner, command_id },
                        &worker_commands,
                    );
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
                            &worker_commands,
                            None,
                        );
                    }
                    RuntimeCommand::SendAgentSessionInput { input } => {
                        // A paused session accepts input and holds it. The
                        // ordinary continuation path would reject it as "still
                        // active", which is true and useless: the operator
                        // paused it on purpose and expects to be able to type.
                        if agent_session_status(&event_bus, &input.session_id)
                            == Some(AgentSessionStatus::Paused)
                        {
                            hold_paused_agent_session_input(
                                owner,
                                command_id,
                                input,
                                &event_bus,
                                &mut held_session_inputs,
                            );
                        } else {
                            run_supervised_agent_session_continuation(
                                &engine,
                                owner,
                                command_id,
                                input.session_id,
                                Some(input.content),
                                None,
                                &event_bus,
                                &active_control,
                                &pending_approvals,
                                &approval_timers,
                                &permission_control,
                                approval_ttl_secs,
                                &worker_commands,
                            );
                        }
                    }
                    RuntimeCommand::RetryAgentSession { session_id } => {
                        // A retry re-runs the last prompt of a settled session
                        // and starts no new control history of its own.
                        run_supervised_agent_session_continuation(
                            &engine,
                            owner,
                            command_id,
                            session_id,
                            None,
                            None,
                            &event_bus,
                            &active_control,
                            &pending_approvals,
                            &approval_timers,
                            &permission_control,
                            approval_ttl_secs,
                            &worker_commands,
                        );
                    }
                    RuntimeCommand::CancelAgentSession { session_id } => {
                        // Cancel discards whatever a pause was holding: the
                        // session is ending either way, and a held prompt that
                        // outlived the session it was written for would be sent
                        // into the next one.
                        held_session_inputs.remove(&session_id);
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
                    RuntimeCommand::PauseAgentSession { session_id } => {
                        run_supervised_agent_session_pause(
                            &engine,
                            owner,
                            command_id,
                            session_id,
                            &event_bus,
                            &active_control,
                            &pending_approvals,
                            &session_audit,
                            &held_session_inputs,
                        );
                    }
                    RuntimeCommand::ResumeAgentSession { session_id } => {
                        run_supervised_agent_session_resume(
                            &engine,
                            owner,
                            command_id,
                            session_id,
                            &event_bus,
                            &active_control,
                            &pending_approvals,
                            &approval_timers,
                            &permission_control,
                            approval_ttl_secs,
                            &session_audit,
                            &mut held_session_inputs,
                            &worker_commands,
                        );
                    }
                    RuntimeCommand::KillAgentSession { session_id } => {
                        run_supervised_agent_session_kill(
                            &engine,
                            owner,
                            command_id,
                            session_id,
                            &event_bus,
                            &active_control,
                            &pending_approvals,
                            &session_audit,
                            &mut held_session_inputs,
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
                if let Err(error) =
                    sync_lane_permissions(&lane_supervisor, &engine, applied_permission_epoch)
                {
                    emit_error(&event_bus, RuntimeOwner::default(), error);
                }
            }
            // The accept path already pushed the entry and published
            // `InputQueued`; the only decision left is whether a completed turn
            // had armed the drain, which is worker-local state. Released here
            // rather than after an unrelated command: the queue is released by
            // a completed turn and by a new entry landing while it is still
            // draining, not by whatever else the operator happened to do next.
            SupervisorMessage::SessionFollowUpQueued { owner } => {
                if session_queue_draining && owner.lane_id.is_none() {
                    session_queue_draining = drain_session_queue(
                        &mut engine,
                        &owner,
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
                        owner,
                        engine.frontend_status_lifecycle_events(),
                    );
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
            SupervisorMessage::DrainHeldSessionInput { owner, session_id } => {
                install_runtime_event_sink(&mut engine, event_bus.clone(), owner.clone());
                drain_held_session_input(
                    &engine,
                    owner,
                    session_id,
                    &event_bus,
                    &active_control,
                    &pending_approvals,
                    &approval_timers,
                    &permission_control,
                    approval_ttl_secs,
                    &mut held_session_inputs,
                    &worker_commands,
                );
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
            let mut approval = approval_request_view(
                &request_id,
                &prompt,
                owner.clone(),
                approval_ttl_secs,
                false,
                None,
            );
            apply_operator_git_approval_shape(&mut approval, &command);
            // The git shape decides the risk, so the standing offer is
            // withdrawn after it rather than predicted before it.
            withdraw_standing_scope_for_high_risk(&mut approval);
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
                    editable: approval.editable,
                    tool_input: prompt.input.clone(),
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
            approval.allowed_scopes = allowed_approval_scopes(&approval.owner, &[], false);
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
                    editable: approval.editable,
                    tool_input: approval_input_for_pending(&approval),
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
        // An `Edit` is answerable only where the request said so, which is
        // the same rule the scopes follow: a client must not invent a
        // decision the prompt never offered.
        ApprovalDecision::Edit { .. } => pending.editable,
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
    worker_commands: &Sender<SupervisorMessage>,

    // `Some` only on the starter-Lane path, which already ran the checks
    // below, minted this id, and reserved its Lane binding so the receipt
    // could name it. Re-running them here would find that reservation and
    // refuse the session it describes.
    reserved_session_id: Option<String>,
) {
    if reserved_session_id.is_none() {
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
    let already_reserved = reserved_session_id.is_some();
    let session_id = reserved_session_id.unwrap_or_else(|| fresh_id("agent-session"));
    let session_owner = RuntimeOwner {
        lane_id: Some(request.lane_id.clone()),
        session_id: Some(session_id.clone()),
        ..owner
    };
    if !already_reserved
        && let Err(reason) = reserve_lane_agent_session_binding(
            event_bus,
            &request.lane_id,
            &request.agent_id,
            &session_id,
        )
    {
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
        control: None,
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
        engine.cwd().to_path_buf(),
        worker_commands.clone(),
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
    let mut previous_request_id: Option<String> = None;
    Box::new(move |prompt: PermissionPrompt| {
        let permission_epoch = approval_permission
            .lock()
            .map(|control| control.epoch())
            .unwrap_or(u64::MAX);
        let request_id = fresh_id("approval");
        let (approval_sender, approval_receiver) = mpsc::channel();
        // The request a revision replaces is always the one this approver
        // published immediately before it: `resolve_permission` re-prompts in
        // place, so the previous id is the correct predecessor rather than an
        // unrelated earlier prompt.
        let revised_from = prompt
            .is_revision
            .then(|| previous_request_id.clone())
            .flatten();
        previous_request_id = Some(request_id.clone());
        let approval = approval_request_view(
            &request_id,
            &prompt,
            owner.clone(),
            approval_ttl_secs,
            true,
            revised_from,
        );
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
                editable: approval.editable,
                tool_input: prompt.input.clone(),
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
    workspace_root: PathBuf,
    worker_commands: Sender<SupervisorMessage>,
) -> RuntimeEventSink {
    let sink_bus = event_bus.clone();
    let sink_active = Arc::clone(active_control);
    // The artifact this session's *current* turn is persisted to.
    //
    // `viden-agents` names a turn's runtime-event artifact after the turn id it
    // publishes on `TurnStarted`, and a follow-up turn gets a fresh one, so the
    // durable target is read from the bracket that comes through this same sink
    // rather than assumed to be the session id. The first turn's artifact *is*
    // the session id, which is the honest starting value.
    let sink_turn_id = Arc::new(Mutex::new(session_id.clone()));
    Arc::new(move |events: Vec<RuntimeEvent>| {
        let mut events = events;
        if let Some(turn_id) = events.iter().rev().find_map(|event| match &event.kind {
            RuntimeEventKind::TurnStarted { turn } => Some(turn.turn_id.clone()),
            _ => None,
        }) && let Ok(mut current) = sink_turn_id.lock()
        {
            *current = turn_id;
        }
        let completed = crate::work_evidence::canonicalize_agent_patch_evidence(
            &context_engine_root,
            &mut events,
        );
        // Compatibility follow-up 10. The adapter persisted its own patch fact
        // before this ran, with `canonical: None`, because it owns no store;
        // the completed row is written back to the same append-only artifact so
        // a restart rebuilds the archive from what Core published rather than
        // from the incomplete fact. Only the rows this ingestion changed are
        // appended, and a failure is surfaced rather than swallowed: the live
        // facts are already correct, so this is a durability warning, not a
        // reason to drop the batch.
        if !completed.is_empty()
            && let Ok(turn_id) = sink_turn_id.lock()
            && let Err(error) = viden_agents::append_agent_turn_runtime_events(
                &workspace_root,
                &turn_id,
                &completed,
            )
        {
            emit_error(
                &sink_bus,
                owner.clone(),
                format!("an agent patch was canonicalized but not persisted: {error}"),
            );
        }
        let terminal = events.iter().any(|event| {
            matches!(
                event.kind,
                RuntimeEventKind::AgentSessionCompleted { .. }
                    | RuntimeEventKind::AgentSessionFailed { .. }
                    | RuntimeEventKind::AgentSessionKilled { .. }
            ) || matches!(
                &event.kind,
                RuntimeEventKind::AgentSessionUpdated { session }
                    if session.status.is_terminal()
            )
        });
        // Only a turn that *completed* releases the next held input, which is
        // the rule C6 already applies to the session queue: nothing runs behind
        // a failed, cancelled, or killed turn. A failure leaves the inputs held
        // and counted, so the operator retries or kills rather than watching
        // every held prompt cascade into a session that is already broken.
        let completed = events.iter().any(|event| {
            matches!(event.kind, RuntimeEventKind::AgentSessionCompleted { .. })
                || matches!(
                    &event.kind,
                    RuntimeEventKind::AgentSessionUpdated { session }
                        if session.status == AgentSessionStatus::Completed
                )
        });
        emit_events(&sink_bus, owner.clone(), events);
        if terminal {
            clear_active_control(&sink_active, &session_id);
        }
        if completed {
            // Arm the drain on the worker. This runs on the adapter's monitor
            // thread, so the held-input map — which only the worker owns — is
            // reached by message, never by lock.
            let _ = worker_commands.send(SupervisorMessage::DrainHeldSessionInput {
                owner: owner.clone(),
                session_id: session_id.clone(),
            });
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
    // The session's operator-control history, carried onto every fact of the
    // turn this starts so a client watching `held_input_count` drain never sees
    // the record disappear between turns.
    control: Option<AgentSessionControlView>,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
    pending_approvals: &Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    approval_timers: &Arc<ApprovalTimerRegistry>,
    permission_control: &Arc<Mutex<PermissionControlState>>,
    approval_ttl_secs: u64,
    worker_commands: &Sender<SupervisorMessage>,
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
    // A killed session is retryable exactly as a failed one is: the operator
    // ended the turn, not the session's right to exist.
    if !session.status.is_terminal() {
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
    session.control = control;
    let request_control = ModelRequestControl::new();
    if let Err(error) = acquire_active_agent_session(
        active_control,
        session_id.clone(),
        owner.clone(),
        request_control,
    ) {
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
        engine.cwd().to_path_buf(),
        worker_commands.clone(),
    );
    let result = if let Some(content) = content {
        resume_typed_agent_session(
            engine.cwd(),
            &session_id,
            content,
            owner.clone(),
            runtime_event_sink,
            approver,
            control,
        )
    } else {
        retry_typed_agent_session(
            engine.cwd(),
            &session_id,
            owner.clone(),
            runtime_event_sink,
            approver,
            control,
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

/// The live status of one Agent session, or `None` when Core does not know it.
fn agent_session_status(
    event_bus: &RuntimeEventBus,
    session_id: &str,
) -> Option<AgentSessionStatus> {
    event_bus.state.lock().ok().and_then(|state| {
        state
            .live_view
            .agent_sessions
            .iter()
            .find(|session| session.session_id == session_id)
            .map(|session| session.status)
    })
}

/// The session view Core currently holds, cloned.
fn agent_session_view(event_bus: &RuntimeEventBus, session_id: &str) -> Option<AgentSessionView> {
    event_bus.state.lock().ok().and_then(|state| {
        state
            .live_view
            .agent_sessions
            .iter()
            .find(|session| session.session_id == session_id)
            .cloned()
    })
}

/// Stable rejection reason for a control command sent against a status that
/// does not allow it.
///
/// The current status is part of the reason because a client that retried
/// blindly would otherwise have no way to tell "already paused" from "already
/// finished", and both are answers the operator needs.
fn session_status_rejection(status: AgentSessionStatus) -> String {
    let token = match status {
        AgentSessionStatus::Starting => "starting",
        AgentSessionStatus::Running => "running",
        AgentSessionStatus::WaitingApproval => "waiting_approval",
        AgentSessionStatus::Completed => "completed",
        AgentSessionStatus::Failed => "failed",
        AgentSessionStatus::Cancelled => "cancelled",
        AgentSessionStatus::Paused => "paused",
        AgentSessionStatus::Killed => "killed",
    };
    format!("session_status_{token}")
}

/// Deny the approval a session was waiting on, with an audited reason.
///
/// The durable row is written before the fact that announces the decision, the
/// same order every other audited mutation uses: `ApprovalResolved` carries the
/// `audit_id` the request already showed the operator, and publishing it before
/// the row existed would hand a client a receipt it cannot resolve. The denial
/// itself is never withheld because the audit failed — a prompt whose turn is
/// gone must not stay blocked — so an audit failure is published as an `Error`.
fn deny_session_approval_with_audit(
    reason: &str,
    session_id: &str,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
    pending_approvals: &Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    audit: &ApprovalAuditLog,
) {
    let pending = active_control
        .lock()
        .ok()
        .and_then(|controls| {
            controls
                .values()
                .find(|active| active.owner_id == session_id)
                .and_then(|active| match &active.state {
                    ActiveJobState::PendingApproval { request_id } => Some(request_id.clone()),
                    _ => None,
                })
        })
        .and_then(|request_id| {
            pending_approvals
                .lock()
                .ok()
                .and_then(|mut approvals| approvals.remove(&request_id))
                .map(|pending| (request_id, pending))
        });
    let Some((request_id, pending)) = pending else {
        return;
    };
    if let Err(error) = audit.record_decision(
        &pending.audit_id,
        &pending.owner,
        &request_id,
        &pending.tool_name,
        pending.target.owner_id(),
        &ApprovalDecision::Deny,
    ) {
        emit_error(
            event_bus,
            pending.owner.clone(),
            format!("approval denial was applied but not audited: {error}"),
        );
    }
    resolve_removed_pending_approval(
        &request_id,
        pending,
        ApprovalDecision::Deny,
        event_bus,
        active_control,
        Some(reason.to_string()),
    );
}

/// Pause an Agent session: stop the in-flight prompt, keep the session.
///
/// The prompt is gone, so the approval it was waiting on cannot be answered by
/// anyone; Core denies it as `session_paused` rather than leaving a prompt on
/// screen that nothing will ever act on.
#[allow(clippy::too_many_arguments)]
fn run_supervised_agent_session_pause(
    engine: &SessionEngine,
    owner: RuntimeOwner,
    command_id: String,
    session_id: String,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
    pending_approvals: &Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    audit: &ApprovalAuditLog,
    held_inputs: &BTreeMap<String, Vec<String>>,
) {
    let Some(mut session) = agent_session_view(event_bus, &session_id) else {
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
    if !matches!(
        session.status,
        AgentSessionStatus::Starting
            | AgentSessionStatus::Running
            | AgentSessionStatus::WaitingApproval
    ) {
        emit_event(
            event_bus,
            owner,
            RuntimeEventKind::CommandRejected {
                command_id,
                reason: session_status_rejection(session.status),
            },
        );
        return;
    }
    deny_session_approval_with_audit(
        "session_paused",
        &session_id,
        event_bus,
        active_control,
        pending_approvals,
        audit,
    );
    if let Some(active) = active_control_for_owner(active_control, &session_id) {
        active.cancel();
    }
    let kept_connection = match pause_typed_agent_session(engine.cwd(), &session_id) {
        Ok(kept_connection) => kept_connection,
        Err(error) => {
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
    };
    let audit_id = fresh_id("audit");
    if let Err(error) =
        audit.record_session_control(&audit_id, &owner, "pause", &session_id, &session.lane_id)
    {
        emit_error(
            event_bus,
            owner.clone(),
            format!("agent session pause was applied but not audited: {error}"),
        );
    }
    emit_event(
        event_bus,
        owner.clone(),
        RuntimeEventKind::CommandAccepted {
            command_id,
            command: redacted_runtime_command_for_event(&RuntimeCommand::PauseAgentSession {
                session_id: session_id.clone(),
            }),
        },
    );
    let held = held_inputs
        .get(&session_id)
        .map_or(0, |inputs| inputs.len() as u32);
    session.status = AgentSessionStatus::Paused;
    session.control = Some(AgentSessionControlView {
        paused_at: Some(now_timestamp()),
        resumed_at: None,
        terminated_by: None,
        held_input_count: held,
    });
    // An adapter that did not answer the cooperative cancel was terminated, so
    // the resume will reload the session instead of reusing a live connection.
    // Saying so is the difference between a pause the operator can trust and
    // one that quietly became a restart.
    session.diagnostic = Some(if kept_connection {
        "paused by owner; the agent connection is held".to_string()
    } else {
        "paused by owner; the agent did not stop cooperatively, so resume reloads the session"
            .to_string()
    });
    emit_event(
        event_bus,
        owner,
        RuntimeEventKind::AgentSessionUpdated { session },
    );
    clear_active_control(active_control, &session_id);
}

/// Accept an input while the session is paused and hold it.
///
/// The acceptance is real: the input is durably ordered against the other held
/// inputs and counted in the session's control record, so a client never has to
/// invent a pending state for a prompt Core did not acknowledge.
fn hold_paused_agent_session_input(
    owner: RuntimeOwner,
    command_id: String,
    input: viden_types::AgentSessionInput,
    event_bus: &RuntimeEventBus,
    held_inputs: &mut BTreeMap<String, Vec<String>>,
) {
    let held = held_inputs.entry(input.session_id.clone()).or_default();
    if held.len() >= MAX_HELD_SESSION_INPUTS {
        emit_event(
            event_bus,
            owner,
            RuntimeEventKind::CommandRejected {
                command_id,
                reason: format!(
                    "a paused agent session holds at most {MAX_HELD_SESSION_INPUTS} inputs; resume or kill it"
                ),
            },
        );
        return;
    }
    held.push(input.content.clone());
    let held_input_count = held.len() as u32;
    let input_id = fresh_id("agent-input");
    emit_event(
        event_bus,
        owner.clone(),
        RuntimeEventKind::CommandAccepted {
            command_id,
            command: redacted_runtime_command_for_event(&RuntimeCommand::SendAgentSessionInput {
                input: input.clone(),
            }),
        },
    );
    emit_event(
        event_bus,
        owner.clone(),
        RuntimeEventKind::AgentSessionInputAccepted {
            session_id: input.session_id.clone(),
            input_id,
        },
    );
    if let Some(mut session) = agent_session_view(event_bus, &input.session_id) {
        let control = session.control.get_or_insert_with(Default::default);
        control.held_input_count = held_input_count;
        session.status = AgentSessionStatus::Paused;
        emit_event(
            event_bus,
            owner,
            RuntimeEventKind::AgentSessionUpdated { session },
        );
    }
}

/// Resume a paused Agent session.
///
/// With a held input the session runs it; with none it settles at `Completed`,
/// which in this contract already means "no turn in flight, session alive and
/// accepting input" — not "the work finished".
#[allow(clippy::too_many_arguments)]
fn run_supervised_agent_session_resume(
    engine: &SessionEngine,
    owner: RuntimeOwner,
    command_id: String,
    session_id: String,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
    pending_approvals: &Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    approval_timers: &Arc<ApprovalTimerRegistry>,
    permission_control: &Arc<Mutex<PermissionControlState>>,
    approval_ttl_secs: u64,
    audit: &ApprovalAuditLog,
    held_inputs: &mut BTreeMap<String, Vec<String>>,
    worker_commands: &Sender<SupervisorMessage>,
) {
    let Some(mut session) = agent_session_view(event_bus, &session_id) else {
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
    if session.status != AgentSessionStatus::Paused {
        emit_event(
            event_bus,
            owner,
            RuntimeEventKind::CommandRejected {
                command_id,
                reason: session_status_rejection(session.status),
            },
        );
        return;
    }
    let audit_id = fresh_id("audit");
    if let Err(error) =
        audit.record_session_control(&audit_id, &owner, "resume", &session_id, &session.lane_id)
    {
        emit_error(
            event_bus,
            owner.clone(),
            format!("agent session resume was applied but not audited: {error}"),
        );
    }
    emit_event(
        event_bus,
        owner.clone(),
        RuntimeEventKind::CommandAccepted {
            command_id: command_id.clone(),
            command: redacted_runtime_command_for_event(&RuntimeCommand::ResumeAgentSession {
                session_id: session_id.clone(),
            }),
        },
    );
    let queue = held_inputs.entry(session_id.clone()).or_default();
    let next = (!queue.is_empty()).then(|| queue.remove(0));
    let remaining = queue.len() as u32;
    if queue.is_empty() {
        held_inputs.remove(&session_id);
    }
    let mut control = session.control.unwrap_or_default();
    control.resumed_at = Some(now_timestamp());
    control.held_input_count = remaining;
    session.control = Some(control);
    let Some(next) = next else {
        // No held input: the session is alive with no turn in flight, which is
        // exactly what `Completed` already means here.
        session.status = AgentSessionStatus::Completed;
        session.diagnostic = Some("resumed by owner; no held input to forward".to_string());
        emit_event(
            event_bus,
            owner,
            RuntimeEventKind::AgentSessionUpdated { session },
        );
        return;
    };
    // Publishing the resumed session before the turn starts is what lets the
    // continuation path see a terminal status; the continuation republishes it
    // as `Starting` the moment the prompt is actually sent, carrying this same
    // control record so the count stays readable across the turn boundary.
    session.status = AgentSessionStatus::Completed;
    session.diagnostic = (remaining > 0)
        .then(|| format!("resumed by owner; {remaining} held input(s) run after this turn"));
    let control = session.control;
    emit_event(
        event_bus,
        owner.clone(),
        RuntimeEventKind::AgentSessionUpdated { session },
    );
    run_supervised_agent_session_continuation(
        engine,
        owner,
        command_id,
        session_id,
        Some(next),
        control,
        event_bus,
        active_control,
        pending_approvals,
        approval_timers,
        permission_control,
        approval_ttl_secs,
        worker_commands,
    );
}

/// Forward the next input a paused session held, once the turn before it
/// completed (`runtime.agent_session_control`).
///
/// This is the drain half of the arm-then-drain pair: the session's sink arms
/// it from the adapter's monitor thread, and it runs here, on the worker, which
/// is the only owner of the held-input map. Nothing runs behind a turn that did
/// not complete — the arming side already refuses to post for one — so a failed
/// turn leaves the remaining inputs held, counted, and visible rather than
/// cascading them into a session that is already broken.
#[allow(clippy::too_many_arguments)]
fn drain_held_session_input(
    engine: &SessionEngine,
    owner: RuntimeOwner,
    session_id: String,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
    pending_approvals: &Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    approval_timers: &Arc<ApprovalTimerRegistry>,
    permission_control: &Arc<Mutex<PermissionControlState>>,
    approval_ttl_secs: u64,
    held_inputs: &mut BTreeMap<String, Vec<String>>,
    worker_commands: &Sender<SupervisorMessage>,
) {
    let Some(queue) = held_inputs.get_mut(&session_id) else {
        return;
    };
    if queue.is_empty() {
        held_inputs.remove(&session_id);
        return;
    }
    let Some(session) = agent_session_view(event_bus, &session_id) else {
        return;
    };
    // A session paused again while its turn ran keeps holding: the operator's
    // newer intent wins over the drain the older turn armed.
    if session.status == AgentSessionStatus::Paused || !session.status.is_terminal() {
        return;
    }
    let next = queue.remove(0);
    let remaining = queue.len() as u32;
    if queue.is_empty() {
        held_inputs.remove(&session_id);
    }
    let mut control = session.control.unwrap_or_default();
    control.held_input_count = remaining;
    run_supervised_agent_session_continuation(
        engine,
        owner,
        fresh_id("cmd-drain-held-input"),
        session_id,
        Some(next),
        Some(control),
        event_bus,
        active_control,
        pending_approvals,
        approval_timers,
        permission_control,
        approval_ttl_secs,
        worker_commands,
    );
}

/// Kill an Agent session: terminate now, with no cancel grace.
///
/// Held inputs are discarded rather than silently carried into a later session,
/// and the diagnostic names how many, because an operator who typed three
/// prompts and killed the session must be told those prompts were never sent.
#[allow(clippy::too_many_arguments)]
fn run_supervised_agent_session_kill(
    engine: &SessionEngine,
    owner: RuntimeOwner,
    command_id: String,
    session_id: String,
    event_bus: &RuntimeEventBus,
    active_control: &ActiveControlRegistry,
    pending_approvals: &Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    audit: &ApprovalAuditLog,
    held_inputs: &mut BTreeMap<String, Vec<String>>,
) {
    let Some(mut session) = agent_session_view(event_bus, &session_id) else {
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
    if session.status.is_terminal() {
        emit_event(
            event_bus,
            owner,
            RuntimeEventKind::CommandRejected {
                command_id,
                reason: session_status_rejection(session.status),
            },
        );
        return;
    }
    deny_session_approval_with_audit(
        "session_killed",
        &session_id,
        event_bus,
        active_control,
        pending_approvals,
        audit,
    );
    if let Some(active) = active_control_for_owner(active_control, &session_id) {
        active.cancel();
    }
    if let Err(error) = kill_typed_agent_session(engine.cwd(), &session_id) {
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
    let discarded = held_inputs
        .remove(&session_id)
        .map_or(0, |inputs| inputs.len());
    let audit_id = fresh_id("audit");
    if let Err(error) =
        audit.record_session_control(&audit_id, &owner, "kill", &session_id, &session.lane_id)
    {
        emit_error(
            event_bus,
            owner.clone(),
            format!("agent session kill was applied but not audited: {error}"),
        );
    }
    emit_event(
        event_bus,
        owner.clone(),
        RuntimeEventKind::CommandAccepted {
            command_id,
            command: redacted_runtime_command_for_event(&RuntimeCommand::KillAgentSession {
                session_id: session_id.clone(),
            }),
        },
    );
    session.status = AgentSessionStatus::Killed;
    session.control = Some(AgentSessionControlView {
        paused_at: session.control.and_then(|control| control.paused_at),
        resumed_at: None,
        terminated_by: Some(AgentSessionTermination::Killed),
        held_input_count: 0,
    });
    session.diagnostic = Some(if discarded > 0 {
        format!("killed by owner; {discarded} held input(s) were discarded")
    } else {
        "killed by owner".to_string()
    });
    emit_event(
        event_bus,
        owner,
        RuntimeEventKind::AgentSessionKilled { session },
    );
    clear_active_control(active_control, &session_id);
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
    if session.status.is_terminal() {
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
            // Cancel and kill both settle a session; only this record says
            // which of the two the operator asked for.
            session.control = Some(cancelled_session_control());
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

    let mut previous_request_id: Option<String> = None;
    let mut approver = |prompt: PermissionPrompt| {
        let permission_epoch = permission_control
            .lock()
            .map(|control| control.epoch())
            .unwrap_or(u64::MAX);
        let request_id = fresh_id("approval");
        let (approval_sender, approval_receiver) = mpsc::channel();
        // The request a revision replaces is always the one this approver
        // published immediately before it: `resolve_permission` re-prompts in
        // place, so the previous id is the correct predecessor rather than an
        // unrelated earlier prompt.
        let revised_from = prompt
            .is_revision
            .then(|| previous_request_id.clone())
            .flatten();
        previous_request_id = Some(request_id.clone());
        let approval = approval_request_view(
            &request_id,
            &prompt,
            owner.clone(),
            approval_ttl_secs,
            true,
            revised_from,
        );
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
                editable: approval.editable,
                tool_input: prompt.input.clone(),
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

    let mut previous_request_id: Option<String> = None;
    let mut approver = |prompt: PermissionPrompt| {
        let permission_epoch = permission_control
            .lock()
            .map(|control| control.epoch())
            .unwrap_or(u64::MAX);
        let request_id = fresh_id("approval");
        let (approval_sender, approval_receiver) = mpsc::channel();
        // The request a revision replaces is always the one this approver
        // published immediately before it: `resolve_permission` re-prompts in
        // place, so the previous id is the correct predecessor rather than an
        // unrelated earlier prompt.
        let revised_from = prompt
            .is_revision
            .then(|| previous_request_id.clone())
            .flatten();
        previous_request_id = Some(request_id.clone());
        let approval = approval_request_view(
            &request_id,
            &prompt,
            owner.clone(),
            approval_ttl_secs,
            true,
            revised_from,
        );
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
                editable: approval.editable,
                tool_input: prompt.input.clone(),
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

/// Builds the request a client sees.
///
/// `interactive` is true for the approvals that reach
/// `PermissionEngine::apply_approval` — the native turn, the agent task, and
/// the agent session gates. Only those can install the live rule an `Always`
/// grant needs or re-decide an `Edit`, so only those offer the two decisions
/// `runtime.approval_scopes` adds; a project-mutation or context-retrieval
/// approval that advertised them would publish an answer Core cannot honor.
///
/// `revised_from` names the request this one replaces when the operator's
/// revision still needs a decision.
fn approval_request_view(
    request_id: &str,
    prompt: &PermissionPrompt,
    owner: RuntimeOwner,
    approval_ttl_secs: u64,
    interactive: bool,
    revised_from: Option<String>,
) -> ApprovalRequestView {
    // The standing scope is never offered for a high-risk request: a rule
    // that silences the most consequential prompts across every workspace is
    // not a convenience an operator should be able to grant in one keystroke.
    let editable = interactive && viden_permissions::tool_input_is_editable(&prompt.tool_name);
    let allowed_scopes = allowed_approval_scopes(&owner, &prompt.candidate_paths, interactive);
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
        revised_from,
        editable,
        // Only an editable request seeds a revision, so only an editable
        // request publishes the bytes (GUI-CORE-036). Every other request
        // encodes exactly as it did before this field existed.
        input: editable.then(|| prompt.input.clone()),
    }
}

/// Drops `ApprovalScope::Always` from a request whose risk turned out to be
/// `High`.
///
/// The risk of a git action is decided after the generic view is built, so the
/// standing offer is withdrawn here rather than predicted there.
fn withdraw_standing_scope_for_high_risk(approval: &mut ApprovalRequestView) {
    if approval.risk == ApprovalRisk::High || approval.risk == ApprovalRisk::Critical {
        approval
            .allowed_scopes
            .retain(|scope| !matches!(scope, ApprovalScope::Always));
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

fn allowed_approval_scopes(
    owner: &RuntimeOwner,
    candidate_paths: &[String],
    standing_offered: bool,
) -> Vec<ApprovalScope> {
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
    // Last, so the list reads from narrowest to widest. A client sending
    // `Always` when it is absent is rejected `scope_not_offered`.
    if standing_offered {
        scopes.push(ApprovalScope::Always);
    }
    scopes
}

/// The tool input a context-retrieval approval is about.
///
/// That path builds its request from a prepared view rather than a
/// `PermissionPrompt`, and it never offers `Always` or `Edit`, so the empty
/// map is the honest answer rather than a reconstruction of the preview text.
fn approval_input_for_pending(_approval: &ApprovalRequestView) -> ToolInput {
    ToolInput::new()
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
            agent: None,
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
                    session: None,
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
