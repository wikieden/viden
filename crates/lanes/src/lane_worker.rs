use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, RecvTimeoutError, Sender},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use viden_permissions::PermissionEngine;
use viden_types::{
    AgentLaneRecord, ApprovalDecision, ApprovalRequestView, ApprovalResponse, ApprovalScope,
    LaneStatus, PermissionDecision, PermissionMode, PermissionRuleSource, RuntimeCommand,
    RuntimeErrorView, RuntimeEventKind, RuntimeOwner, ToolInput, ToolSpec, fresh_id, now_timestamp,
};
use viden_workflows::lanes::{LaneEvent, LaneRunObservation};

use crate::lane_runtime::{
    LaneEffectExecutor, LaneEffectRequest, canonical_repo_root, resolve_lane_target,
};
use crate::lane_supervisor::LanePersistence;

pub type LaneEventSink = Arc<dyn Fn(RuntimeOwner, RuntimeEventKind) + Send + Sync>;

/// Re-validates a queued lane-mutation approval against a lane-scoped
/// permission engine.
///
/// The lane subsystem owns *when* a queued operator response is replayed, but
/// not *how* a permission is resolved: that sequence (`decide` -> ask ->
/// `apply_approval`, whose plan-mode re-check can still deny an operator
/// "allow") belongs to the embedding runtime's shared permission gate. It is
/// injected rather than called upward so the lane crate keeps no dependency on
/// the gate module. The queued `ApprovalResponse` stands in for the interactive
/// ask flow, and the returned decision is never `Ask`.
pub type LaneApprovalResolver = Arc<
    dyn Fn(&mut PermissionEngine, &ToolSpec, &ToolInput, ApprovalResponse) -> PermissionDecision
        + Send
        + Sync,
>;

struct LanePermissionState {
    // Keep the decision engine and its generation under one lock so a request
    // never observes a new policy with an old epoch (or the reverse).
    engine: PermissionEngine,
    epoch: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LaneTerminalKind {
    Archived,
    Cleaned,
}

#[derive(Debug, Clone)]
pub(crate) struct LaneTerminalCompletion {
    pub(crate) kind: LaneTerminalKind,
    pub(crate) lane: AgentLaneRecord,
}

pub(crate) enum LaneWorkerMessage {
    Command {
        command_id: String,
        command: Box<RuntimeCommand>,
    },
    ResumeApproval {
        request_id: String,
        response: ApprovalResponse,
        permissions: PermissionEngine,
        permission_epoch: u64,
        completion: Sender<()>,
    },
    Cancel {
        command_id: String,
    },
    Shutdown,
}

pub(crate) struct LaneWorkerHandle {
    pub(crate) owner: RuntimeOwner,
    pending_approval: Arc<Mutex<Option<String>>>,
    terminal_completion: Arc<Mutex<Option<LaneTerminalCompletion>>>,
    permissions: Arc<Mutex<LanePermissionState>>,
    registered: Arc<AtomicBool>,
    sender: Sender<LaneWorkerMessage>,
    worker: Option<JoinHandle<()>>,
}

impl LaneWorkerHandle {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn spawn(
        owner: RuntimeOwner,
        lane: AgentLaneRecord,
        repo: std::path::PathBuf,
        persistence: Arc<dyn LanePersistence>,
        permissions: PermissionEngine,
        permission_epoch: u64,
        effects: Arc<dyn LaneEffectExecutor>,
        events: LaneEventSink,
        approvals: LaneApprovalResolver,
        approval_ttl_secs: u64,
        registered: bool,
        terminal_sender: Sender<LaneTerminalCompletion>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let pending_approval = Arc::new(Mutex::new(None));
        let terminal_completion = Arc::new(Mutex::new(None));
        let permissions = Arc::new(Mutex::new(LanePermissionState {
            engine: permissions,
            epoch: permission_epoch,
        }));
        let registered = Arc::new(AtomicBool::new(registered));
        let worker_pending_approval = Arc::clone(&pending_approval);
        let worker_terminal_completion = Arc::clone(&terminal_completion);
        let worker_permissions = Arc::clone(&permissions);
        let worker_registered = Arc::clone(&registered);
        let worker_owner = owner.clone();
        let worker_lane = lane.clone();
        let worker = thread::spawn(move || {
            let mut runtime = LaneWorker {
                owner: worker_owner,
                lane: worker_lane,
                repo,
                persistence,
                permissions: worker_permissions,
                effects,
                events,
                approvals,
                pending_mutation: None,
                pending_approval: worker_pending_approval,
                terminal_completion: worker_terminal_completion,
                terminal_sender,
                registered: worker_registered,
                approval_ttl_secs,
                runtime_active: false,
                open_run_started_at: None,
            };
            loop {
                match receiver.recv_timeout(Duration::from_millis(25)) {
                    Ok(LaneWorkerMessage::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                    Ok(message) => {
                        runtime.handle(message);
                        if runtime.terminal_completed() {
                            while let Ok(message) = receiver.try_recv() {
                                runtime.reject_after_terminal(message);
                            }
                            break;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => runtime.expire_pending_approval(),
                }
            }
            if runtime.runtime_active {
                // Process teardown deliberately drops the open run instead of
                // observing it. This runs while `LaneWorkerHandle::drop` blocks
                // on `join`, and a durable append here would take the exclusive
                // lanes lock and reduce the whole log on the shutdown path,
                // against an event sink that may already be gone. The reducer's
                // newer-`Started`-wins rule absorbs the dangling open run.
                let _ = runtime.effects.shutdown_lane(&runtime.lane.id);
            }
        });
        Self {
            owner,
            pending_approval,
            terminal_completion,
            permissions,
            registered,
            sender,
            worker: Some(worker),
        }
    }

    pub(crate) fn send(&self, message: LaneWorkerMessage) -> Result<(), String> {
        self.sender
            .send(message)
            .map_err(|error| format!("lane worker stopped: {error}"))
    }

    pub(crate) fn owns_pending_approval(&self, request_id: &str) -> bool {
        self.pending_approval
            .lock()
            .is_ok_and(|pending| pending.as_deref() == Some(request_id))
    }

    pub(crate) fn take_terminal_completion(&self) -> Option<LaneTerminalCompletion> {
        self.terminal_completion
            .lock()
            .ok()
            .and_then(|mut completion| completion.take())
    }

    pub(crate) fn is_registered(&self) -> bool {
        self.registered.load(Ordering::Acquire)
    }

    pub(crate) fn sync_permissions(
        &self,
        mut authoritative: PermissionEngine,
        permission_epoch: u64,
    ) -> Result<(), String> {
        let mut installed = self
            .permissions
            .lock()
            .map_err(|_| "lane permission state poisoned".to_string())?;
        let approved_rules = installed
            .engine
            .context_snapshot()
            .allow_rules
            .into_iter()
            .filter(|rule| rule.source == PermissionRuleSource::Session);
        let mut context = authoritative.context_snapshot();
        if context.mode != PermissionMode::Plan {
            for rule in approved_rules {
                if !context.allow_rules.contains(&rule) {
                    context.allow_rules.push(rule);
                }
            }
        }
        authoritative.restore_context(context);
        installed.engine = authoritative;
        installed.epoch = permission_epoch;
        Ok(())
    }

    pub(crate) fn permission_snapshot(&self) -> Result<(PermissionEngine, u64), String> {
        self.permissions
            .lock()
            .map(|permissions| (permissions.engine.clone(), permissions.epoch))
            .map_err(|_| "lane permission state poisoned".to_string())
    }

    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn is_finished(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }
}

impl Drop for LaneWorkerHandle {
    fn drop(&mut self) {
        let _ = self.sender.send(LaneWorkerMessage::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct PendingMutation {
    request_id: String,
    command_id: String,
    audit_id: String,
    expires_at: u64,
    permission_epoch: u64,
    allowed_scopes: Vec<ApprovalScope>,
    tool: ToolSpec,
    input: ToolInput,
    previous_status: LaneStatus,
    operation: PendingOperation,
}

enum PendingOperation {
    Create(LaneEffectRequest),
    Start(LaneEffectRequest),
    Stop,
    ChangeStatus { status: LaneStatus, summary: String },
    SendInput(LaneEffectRequest),
    Apply(LaneEffectRequest),
    Archive { summary: String },
    Cleanup(LaneEffectRequest),
}

struct LaneWorker {
    owner: RuntimeOwner,
    lane: AgentLaneRecord,
    repo: std::path::PathBuf,
    persistence: Arc<dyn LanePersistence>,
    permissions: Arc<Mutex<LanePermissionState>>,
    effects: Arc<dyn LaneEffectExecutor>,
    events: LaneEventSink,
    approvals: LaneApprovalResolver,
    pending_mutation: Option<PendingMutation>,
    pending_approval: Arc<Mutex<Option<String>>>,
    terminal_completion: Arc<Mutex<Option<LaneTerminalCompletion>>>,
    terminal_sender: Sender<LaneTerminalCompletion>,
    registered: Arc<AtomicBool>,
    approval_ttl_secs: u64,
    runtime_active: bool,
    // Mirror of the durable reducer's open-run marker for this lane. It exists
    // only so a live `LaneUpdated` carries the same wall time replay will
    // compute; the lanes log stays authoritative.
    open_run_started_at: Option<u64>,
}

impl LaneWorker {
    fn terminal_completed(&self) -> bool {
        self.terminal_completion
            .lock()
            .is_ok_and(|completion| completion.is_some())
    }

    fn handle(&mut self, message: LaneWorkerMessage) {
        match message {
            LaneWorkerMessage::Command {
                command_id,
                command,
            } => self.handle_command(command_id, *command),
            LaneWorkerMessage::ResumeApproval {
                request_id,
                response,
                permissions,
                permission_epoch,
                completion,
            } => {
                self.resume_approval(request_id, response, permissions, permission_epoch);
                let _ = completion.send(());
            }
            LaneWorkerMessage::Cancel { command_id } => self.cancel(command_id),
            LaneWorkerMessage::Shutdown => {}
        }
    }

    fn reject_after_terminal(&self, message: LaneWorkerMessage) {
        match message {
            LaneWorkerMessage::Command { command_id, .. }
            | LaneWorkerMessage::Cancel { command_id } => {
                self.reject(command_id, "lane already reached a durable terminal state")
            }
            LaneWorkerMessage::ResumeApproval { completion, .. } => {
                let _ = completion.send(());
            }
            LaneWorkerMessage::Shutdown => {}
        }
    }

    fn handle_command(&mut self, command_id: String, command: RuntimeCommand) {
        if self.pending_mutation.is_some() {
            self.reject(command_id, "lane mutation approval is already pending");
            return;
        }
        if self.lane.status == LaneStatus::Archived
            && !matches!(
                command,
                RuntimeCommand::ArchiveLane { .. } | RuntimeCommand::CleanupLane { .. }
            )
        {
            self.reject(command_id, "archived lane is terminal");
            return;
        }
        match command {
            RuntimeCommand::CreateLane { .. } => {
                if self.registered.load(Ordering::Acquire) {
                    self.reject(command_id, "lane is already durably registered");
                    return;
                }
                self.dispatch_mutation(
                    command_id,
                    "lane_create",
                    PendingOperation::Create(LaneEffectRequest::Create {
                        repo: self.repo.clone(),
                        lane: self.lane.clone(),
                    }),
                );
            }
            RuntimeCommand::StartLane {
                command,
                args,
                env,
                output_log,
                ..
            } => {
                let request = LaneEffectRequest::Start {
                    repo: self.repo.clone(),
                    lane: self.lane.clone(),
                    command,
                    args,
                    env,
                    output_log,
                };
                if !matches!(
                    self.lane.status,
                    LaneStatus::Draft | LaneStatus::Queued | LaneStatus::Detached
                ) {
                    self.reject(
                        command_id,
                        format!("lane cannot start while status is {:?}", self.lane.status),
                    );
                    return;
                }
                self.dispatch_mutation(command_id, "lane_start", PendingOperation::Start(request));
            }
            RuntimeCommand::StopLane { .. } => {
                self.dispatch_mutation(command_id, "lane_stop", PendingOperation::Stop);
            }
            RuntimeCommand::AttachLane { .. } => {
                self.dispatch_mutation(
                    command_id,
                    "lane_attach",
                    PendingOperation::ChangeStatus {
                        status: LaneStatus::Attached,
                        summary: "lane attached".to_string(),
                    },
                );
            }
            RuntimeCommand::DetachLane { .. } => {
                self.dispatch_mutation(
                    command_id,
                    "lane_detach",
                    PendingOperation::ChangeStatus {
                        status: LaneStatus::Detached,
                        summary: "lane detached".to_string(),
                    },
                );
            }
            RuntimeCommand::SendLaneInput { input, .. } => {
                self.dispatch_mutation(
                    command_id,
                    "lane_send_input",
                    PendingOperation::SendInput(LaneEffectRequest::SendInput {
                        lane_id: self.lane.id.clone(),
                        input,
                    }),
                );
            }
            RuntimeCommand::AcceptLaneOutput { summary, .. } => {
                self.dispatch_mutation(
                    command_id,
                    "lane_accept_output",
                    PendingOperation::ChangeStatus {
                        status: LaneStatus::Done,
                        summary,
                    },
                );
            }
            RuntimeCommand::ReviseLaneOutput { feedback, .. } => {
                self.dispatch_mutation(
                    command_id,
                    "lane_revise_output",
                    PendingOperation::ChangeStatus {
                        status: LaneStatus::NeedsInput,
                        summary: feedback,
                    },
                );
            }
            RuntimeCommand::DiscardLaneOutput { reason, .. } => {
                self.dispatch_mutation(
                    command_id,
                    "lane_discard_output",
                    PendingOperation::ChangeStatus {
                        status: LaneStatus::Cancelled,
                        summary: reason,
                    },
                );
            }
            RuntimeCommand::ApplyLaneChanges { unified_diff, .. } => {
                if self.lane.status != LaneStatus::Done {
                    self.reject(
                        command_id,
                        format!(
                            "lane changes require done status, found {:?}",
                            self.lane.status
                        ),
                    );
                    return;
                }
                self.dispatch_mutation(
                    command_id,
                    "lane_apply",
                    PendingOperation::Apply(LaneEffectRequest::Apply {
                        cwd: self.repo.clone(),
                        unified_diff,
                    }),
                );
            }
            RuntimeCommand::ResolveLaneConflict { unified_diff, .. } => {
                if self.lane.status != LaneStatus::Blocked {
                    self.reject(
                        command_id,
                        format!(
                            "lane conflict resolution requires blocked status, found {:?}",
                            self.lane.status
                        ),
                    );
                    return;
                }
                self.dispatch_mutation(
                    command_id,
                    "lane_resolve_conflict",
                    PendingOperation::Apply(LaneEffectRequest::Apply {
                        cwd: self.repo.clone(),
                        unified_diff,
                    }),
                );
            }
            RuntimeCommand::ArchiveLane { summary, .. } => {
                self.dispatch_mutation(
                    command_id,
                    "lane_archive",
                    PendingOperation::Archive { summary },
                );
            }
            RuntimeCommand::CleanupLane { force, .. } => {
                self.dispatch_mutation(
                    command_id,
                    "lane_cleanup",
                    PendingOperation::Cleanup(LaneEffectRequest::Cleanup {
                        repo: self.repo.clone(),
                        lane: self.lane.clone(),
                        force,
                    }),
                );
            }
            _ => self.reject(command_id, "command is not a lane lifecycle command"),
        }
    }

    fn dispatch_mutation(
        &mut self,
        command_id: String,
        tool_name: &str,
        operation: PendingOperation,
    ) {
        if self.lane.mutation_policy == viden_types::MutationPolicy::ReadOnly {
            self.reject(
                command_id,
                format!("read-only lane cannot execute `{tool_name}`"),
            );
            return;
        }
        let tool = ToolSpec {
            name: tool_name.to_string(),
            description: format!("Core-owned mutation for lane `{}`", self.lane.id),
            is_mutating: true,
            input_schema_hint: "path=<actual target>; operation metadata is redacted".to_string(),
        };
        let input = match self.permission_input(&operation) {
            Ok(input) => input,
            Err(error) => {
                self.reject(command_id, error);
                return;
            }
        };
        // Deliberately NOT the shared permission gate: lane approvals are
        // queued and resolved later by `resume_approval` (TTL, scope, and
        // permission-epoch checks included), so there is no synchronous ask
        // flow to hand the gate here. Only the pure decide() runs now; the
        // gate-shaped re-check happens inside `resume_approval`.
        let (permission, permission_epoch) = match self.permissions.lock() {
            Ok(permissions) => (permissions.engine.decide(&tool, &input), permissions.epoch),
            Err(_) => {
                self.reject(command_id, "lane permission registry poisoned");
                return;
            }
        };
        match permission {
            PermissionDecision::Deny(denial) => self.reject(command_id, denial.message),
            PermissionDecision::Allow(_)
                if self.lane.mutation_policy == viden_types::MutationPolicy::Autonomous =>
            {
                self.execute_pending_operation(operation);
            }
            PermissionDecision::Allow(_) => {
                self.queue_mutation_approval(command_id, tool, input, operation, permission_epoch)
            }
            PermissionDecision::Ask(_) => {
                self.queue_mutation_approval(command_id, tool, input, operation, permission_epoch)
            }
        }
    }

    fn queue_mutation_approval(
        &mut self,
        command_id: String,
        tool: ToolSpec,
        input: ToolInput,
        operation: PendingOperation,
        permission_epoch: u64,
    ) {
        let previous_status = self.lane.status;
        let request_id = fresh_id("lane-approval");
        let audit_id = fresh_id("audit");
        let expires_at = now_timestamp().saturating_add(self.approval_ttl_secs);
        let mut allowed_scopes = vec![ApprovalScope::Once];
        if let Some(session_id) = self.owner.session_id.clone() {
            allowed_scopes.push(ApprovalScope::Session { session_id });
        }
        if let Some(path) = input.get("path") {
            allowed_scopes.push(ApprovalScope::RepoAllowlist {
                paths: vec![path.clone()],
            });
        }
        if self.registered.load(Ordering::Acquire)
            && self
                .change_status(LaneStatus::WaitingApproval, "lane mutation awaits approval")
                .is_err()
        {
            return;
        }
        let approval = lane_approval(
            &request_id,
            &audit_id,
            expires_at,
            allowed_scopes.clone(),
            &self.owner,
            &self.lane,
            &tool,
            &input,
        );
        self.pending_mutation = Some(PendingMutation {
            request_id: request_id.clone(),
            command_id,
            audit_id,
            expires_at,
            permission_epoch,
            allowed_scopes,
            tool,
            input,
            previous_status,
            operation,
        });
        if let Ok(mut pending) = self.pending_approval.lock() {
            *pending = Some(request_id);
        }
        self.emit(RuntimeEventKind::ApprovalRequested { approval });
    }

    fn permission_input(&self, operation: &PendingOperation) -> Result<ToolInput, String> {
        let mut input = ToolInput::new();
        let lane_path = match operation {
            PendingOperation::Apply(LaneEffectRequest::Apply { cwd, .. }) => {
                canonical_repo_root(cwd)?
            }
            PendingOperation::Start(_) => resolve_lane_target(&self.repo, &self.lane, true)?,
            PendingOperation::SendInput(_) => resolve_lane_target(&self.repo, &self.lane, true)?,
            _ => resolve_lane_target(&self.repo, &self.lane, true)?,
        }
        .to_string_lossy()
        .to_string();
        match operation {
            PendingOperation::Create(_) => {
                input.insert("path".to_string(), lane_path);
                input.insert(
                    "branch".to_string(),
                    self.lane.branch.clone().unwrap_or_default(),
                );
            }
            PendingOperation::Start(LaneEffectRequest::Start {
                command,
                args,
                env,
                output_log,
                ..
            }) => {
                input.insert("path".to_string(), lane_path);
                input.insert(
                    "command".to_string(),
                    format!("{} bytes [REDACTED]", command.len()),
                );
                input.insert(
                    "args".to_string(),
                    format!("{} argument(s) [REDACTED]", args.len()),
                );
                input.insert(
                    "env".to_string(),
                    env.iter()
                        .map(|(key, _)| format!("{key}=[REDACTED]"))
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                input.insert(
                    "output_log".to_string(),
                    output_log
                        .clone()
                        .unwrap_or_else(|| format!(".viden/lanes/{}.log", self.lane.id)),
                );
            }
            PendingOperation::Stop => {
                input.insert("path".to_string(), lane_path);
                input.insert("lane_id".to_string(), self.lane.id.clone());
            }
            PendingOperation::ChangeStatus { status, summary } => {
                input.insert("path".to_string(), lane_path);
                input.insert("status".to_string(), format!("{status:?}"));
                input.insert(
                    "summary".to_string(),
                    format!("{} bytes [REDACTED]", summary.len()),
                );
            }
            PendingOperation::SendInput(LaneEffectRequest::SendInput {
                input: content, ..
            }) => {
                input.insert("path".to_string(), lane_path);
                input.insert(
                    "input_summary".to_string(),
                    format!("{} bytes [REDACTED]", content.len()),
                );
            }
            PendingOperation::Apply(LaneEffectRequest::Apply { cwd, unified_diff }) => {
                let _ = cwd;
                input.insert("path".to_string(), lane_path);
                input.insert(
                    "diff_summary".to_string(),
                    format!(
                        "{} bytes, {} lines, payload [REDACTED]",
                        unified_diff.len(),
                        unified_diff.lines().count()
                    ),
                );
            }
            PendingOperation::Archive { summary } => {
                input.insert("path".to_string(), lane_path);
                input.insert("summary".to_string(), summary.clone());
            }
            PendingOperation::Cleanup(LaneEffectRequest::Cleanup { force, .. }) => {
                input.insert("path".to_string(), lane_path);
                input.insert("force".to_string(), force.to_string());
            }
            _ => {
                input.insert("path".to_string(), lane_path);
            }
        }
        Ok(input)
    }

    fn execute_pending_operation(&mut self, operation: PendingOperation) {
        match operation {
            PendingOperation::Create(request) => self.create(request),
            PendingOperation::Start(request) => self.start(request),
            PendingOperation::Stop => self.stop(),
            PendingOperation::ChangeStatus { status, summary } => {
                let _ = self.change_status(status, summary);
            }
            PendingOperation::SendInput(request) => self.send_input(request),
            PendingOperation::Apply(request) => self.apply(request),
            PendingOperation::Archive { summary } => self.archive(summary),
            PendingOperation::Cleanup(request) => self.cleanup(request),
        }
    }

    fn expire_pending_approval(&mut self) {
        let expired = self
            .pending_mutation
            .as_ref()
            .is_some_and(|pending| now_timestamp() >= pending.expires_at);
        if !expired {
            return;
        }
        let pending = self
            .pending_mutation
            .take()
            .expect("expired approval was checked above");
        if let Ok(mut approval) = self.pending_approval.lock() {
            *approval = None;
        }
        self.emit(RuntimeEventKind::ApprovalResolved {
            request_id: pending.request_id,
            decision: ApprovalDecision::Deny,
            owner: self.owner.clone(),
            audit_id: pending.audit_id,
        });
        if self.registered.load(Ordering::Acquire) {
            let _ = self.change_status(pending.previous_status, "lane mutation approval expired");
        }
        self.reject(pending.command_id, "lane mutation approval expired");
    }

    fn resume_approval(
        &mut self,
        request_id: String,
        response: ApprovalResponse,
        mut permissions: PermissionEngine,
        permission_epoch: u64,
    ) {
        let Some(pending) = self.pending_mutation.take() else {
            return;
        };
        if pending.request_id != request_id {
            self.pending_mutation = Some(pending);
            return;
        }
        if let Ok(mut approval) = self.pending_approval.lock() {
            *approval = None;
        }
        let valid_scope = match &response.decision {
            ApprovalDecision::Deny => true,
            ApprovalDecision::Allow { scope } => pending.allowed_scopes.contains(scope),
        };
        let unexpired = now_timestamp() < pending.expires_at;
        let current_epoch = permission_epoch == pending.permission_epoch;
        let permission_allowed =
            if valid_scope && unexpired && current_epoch && response.is_allowed() {
                // Re-check through the injected permission gate: the queued
                // operator response stands in for the interactive ask flow, so
                // the plan-mode re-check in apply_approval still applies.
                matches!(
                    (self.approvals)(
                        &mut permissions,
                        &pending.tool,
                        &pending.input,
                        response.clone(),
                    ),
                    PermissionDecision::Allow(_)
                )
            } else {
                false
            };
        let decision = if permission_allowed {
            response.decision.clone()
        } else {
            ApprovalDecision::Deny
        };
        self.emit(RuntimeEventKind::ApprovalResolved {
            request_id,
            decision,
            owner: self.owner.clone(),
            audit_id: pending.audit_id.clone(),
        });
        if permission_allowed {
            if let Ok(mut installed) = self.permissions.lock()
                && installed.engine.mode() != PermissionMode::Plan
            {
                let mut context = installed.engine.context_snapshot();
                for rule in permissions
                    .context_snapshot()
                    .allow_rules
                    .into_iter()
                    .filter(|rule| rule.source == PermissionRuleSource::Session)
                {
                    if !context.allow_rules.contains(&rule) {
                        context.allow_rules.push(rule);
                    }
                }
                installed.engine.restore_context(context);
            }
            self.execute_pending_operation(pending.operation);
        } else {
            if self.registered.load(Ordering::Acquire) {
                let _ =
                    self.change_status(pending.previous_status, "lane mutation approval denied");
            }
            let reason = if !unexpired {
                "lane mutation approval expired"
            } else if !current_epoch {
                "lane mutation permission epoch changed"
            } else if !valid_scope {
                "lane mutation approval scope is not allowed"
            } else {
                "lane mutation approval denied"
            };
            self.reject(pending.command_id, reason);
        }
    }

    fn cancel(&mut self, command_id: String) {
        if let Some(pending) = self.pending_mutation.take() {
            if let Ok(mut approval) = self.pending_approval.lock() {
                *approval = None;
            }
            self.emit(RuntimeEventKind::ApprovalResolved {
                request_id: pending.request_id,
                decision: ApprovalDecision::Deny,
                owner: self.owner.clone(),
                audit_id: pending.audit_id,
            });
            let _ = self.change_status(LaneStatus::Cancelled, "lane mutation cancelled");
        } else {
            match self.effects.shutdown_lane(&self.lane.id) {
                Err(error) => self.fail(error),
                Ok(exit_code) => {
                    self.close_active_run(exit_code);
                    let _ = self.change_status(LaneStatus::Cancelled, "lane cancelled");
                }
            }
        }
        self.emit(RuntimeEventKind::CommandAccepted {
            command_id,
            command: RuntimeCommand::CancelActiveTurn,
        });
    }

    fn start(&mut self, request: LaneEffectRequest) {
        if self
            .change_status(LaneStatus::Starting, "lane starting")
            .is_err()
        {
            return;
        }
        match self.effects.execute(request) {
            Ok(result) => {
                self.runtime_active = true;
                self.output("receipt", result.output);
                if self
                    .change_status(LaneStatus::Running, "lane running")
                    .is_err()
                {
                    // Deliberately NOT `close_active_run`: the Running append
                    // just failed, so no `Started` was ever recorded and the
                    // same persistence is still broken. A `Stopped` here would
                    // be an orphan observation for a run that never became
                    // durably visible.
                    let _ = self.effects.shutdown_lane(&self.lane.id);
                    self.runtime_active = false;
                    let _ = self.change_status(
                        LaneStatus::Detached,
                        "lane start requires retry after persistence failure",
                    );
                } else {
                    // Only a lane that actually reached Running counts as a run.
                    self.observe_run(LaneRunObservation::Started);
                }
            }
            Err(error) => self.fail(error),
        }
    }

    fn create(&mut self, request: LaneEffectRequest) {
        let result = match self.effects.execute(request) {
            Ok(result) => result,
            Err(error) => {
                self.error(error);
                return;
            }
        };
        let event = LaneEvent::created(
            fresh_id("lane-event"),
            self.lane.clone(),
            now_timestamp(),
            self.owner.session_id.clone(),
        );
        if let Err(error) = self.persistence.append(&event) {
            let compensation = self.effects.compensate_create(&self.repo, &self.lane);
            self.emit(RuntimeEventKind::LaneRecoveryRequired {
                lane_id: self.lane.id.clone(),
                reason: error.clone(),
                next_action: if compensation.is_ok() {
                    "retry lane registration".to_string()
                } else {
                    "remove the orphan worktree, then retry lane registration".to_string()
                },
            });
            self.error(error);
            return;
        }
        self.registered.store(true, Ordering::Release);
        self.output("receipt", result.output);
        self.emit(RuntimeEventKind::LaneUpdated {
            lane: self.lane.clone(),
        });
    }

    fn stop(&mut self) {
        match self.effects.shutdown_lane(&self.lane.id) {
            Err(error) => self.fail(error),
            Ok(exit_code) => {
                self.close_active_run(exit_code);
                let _ = self.change_status(LaneStatus::Detached, "lane stopped");
            }
        }
    }

    fn send_input(&mut self, request: LaneEffectRequest) {
        let LaneEffectRequest::SendInput { input, .. } = &request else {
            self.error("lane input operation received the wrong request".to_string());
            return;
        };
        let input_id = fresh_id("lane-input");
        self.emit(RuntimeEventKind::InputQueued {
            input: viden_types::QueuedInputView {
                id: input_id.clone(),
                content_preview: input.chars().take(160).collect(),
                created_at: Some(now_timestamp()),
                // The worker owner is the exact identity Core publishes as this
                // Lane's `LaneRuntimeOwnerBinding`, so an owner-scoped client
                // matches the queued input without inferring it (GUI-CORE-010).
                owner: Some(self.owner.clone()),
            },
        });
        let result = self.effects.execute(request);
        self.emit(RuntimeEventKind::InputDequeued { input_id });
        match result {
            Ok(result) => {
                self.output("receipt", result.output);
                let _ = self.change_status(self.lane.status, "lane input delivered");
            }
            Err(error) => self.fail(error),
        }
    }

    fn apply(&mut self, request: LaneEffectRequest) {
        // Measure the payload before the request is consumed by the executor.
        // The redaction summary in `permission_input` computes the same length,
        // but only as formatted approval text, so it cannot be reused here.
        let diff_bytes = match &request {
            LaneEffectRequest::Apply { unified_diff, .. } => unified_diff.len() as u64,
            _ => 0,
        };
        let event = LaneEvent::status_changed(
            fresh_id("lane-event"),
            self.lane.id.clone(),
            LaneStatus::Done,
            "lane changes applied",
            now_timestamp(),
            self.owner.session_id.clone(),
        );
        let persistence = Arc::clone(&self.persistence);
        let mut persist = || persistence.append(&event);
        match self.effects.apply_transactionally(request, &mut persist) {
            Ok(result) if result.conflict_paths.is_empty() => {
                self.output("receipt", result.output);
                self.lane.status = LaneStatus::Done;
                self.lane.summary = "lane changes applied".to_string();
                self.emit(RuntimeEventKind::LaneUpdated {
                    lane: self.lane.clone(),
                });
                self.observe_run(LaneRunObservation::Applied { diff_bytes });
            }
            Ok(result) => {
                self.emit(RuntimeEventKind::LaneConflictDetected {
                    lane_id: self.lane.id.clone(),
                    summary: result.output,
                    paths: result.conflict_paths,
                });
                let _ = self.change_status(LaneStatus::Blocked, "lane patch conflict");
            }
            Err(error) => self.fail(error),
        }
    }

    fn archive(&mut self, summary: String) {
        let exit_code = match self.effects.shutdown_lane(&self.lane.id) {
            Ok(exit_code) => exit_code,
            Err(error) => {
                self.fail(error);
                return;
            }
        };
        self.close_active_run(exit_code);
        if self.change_status(LaneStatus::Archived, summary).is_ok() {
            self.mark_terminal(LaneTerminalKind::Archived);
        }
    }

    fn cleanup(&mut self, request: LaneEffectRequest) {
        // Persist cleanup intent before irreversible worktree removal. A failed completion
        // append therefore remains retryable through the same live worker and after replay.
        if self
            .change_status(LaneStatus::Starting, "lane cleanup intent persisted")
            .is_err()
        {
            return;
        }
        let exit_code = match self.effects.shutdown_lane(&self.lane.id) {
            Ok(exit_code) => exit_code,
            Err(error) => {
                self.fail(error);
                return;
            }
        };
        self.close_active_run(exit_code);
        match self.effects.execute(request) {
            Ok(result) => {
                self.output("receipt", result.output);
                if self
                    .change_status(LaneStatus::Archived, "lane cleaned up")
                    .is_err()
                {
                    self.emit(RuntimeEventKind::LaneRecoveryRequired {
                        lane_id: self.lane.id.clone(),
                        reason: "cleanup completed but durable completion append failed"
                            .to_string(),
                        next_action: "replay cleanup intent and reconcile the worktree".to_string(),
                    });
                } else {
                    self.mark_terminal(LaneTerminalKind::Cleaned);
                }
            }
            Err(error) => self.fail(error),
        }
    }

    fn mark_terminal(&self, kind: LaneTerminalKind) {
        let completed = LaneTerminalCompletion {
            kind,
            lane: self.lane.clone(),
        };
        if let Ok(mut completion) = self.terminal_completion.lock() {
            *completion = Some(completed.clone());
        }
        let _ = self.terminal_sender.send(completed);
    }

    /// Close the run this worker has open, if any, recording whatever exit code
    /// the teardown observed.
    ///
    /// Every path that tears down an ACTIVE runtime routes through here, so an
    /// operator who cancels, archives, or cleans up a runaway terminal lane
    /// still keeps that run's wall time and exit code — precisely the
    /// blind-cost case these stats exist for. When no runtime was active there
    /// is nothing to close: the reducer tolerates an orphan stop, but Core must
    /// not manufacture one. A failed shutdown never reaches here, because no
    /// exit fact was observed in that case.
    fn close_active_run(&mut self, exit_code: Option<i32>) {
        if !self.runtime_active {
            return;
        }
        self.runtime_active = false;
        self.observe_run(LaneRunObservation::Stopped { exit_code });
    }

    /// Append one bounded run measurement and republish the lane so live clients
    /// see the numbers replay will produce.
    ///
    /// Append failure follows exactly the same policy as a failed
    /// `StatusChanged` append: publish `LaneRecoveryRequired` plus a recoverable
    /// error and leave the in-memory record untouched, because the durable log,
    /// not this mirror, is authoritative.
    fn observe_run(&mut self, observation: LaneRunObservation) {
        let timestamp = now_timestamp();
        let event = LaneEvent::run_observed(
            fresh_id("lane-event"),
            self.lane.id.clone(),
            observation,
            timestamp,
            self.owner.session_id.clone(),
        );
        if let Err(error) = self.persistence.append(&event) {
            self.emit(RuntimeEventKind::LaneRecoveryRequired {
                lane_id: self.lane.id.clone(),
                reason: error.clone(),
                next_action: "reload lane state and retry".to_string(),
            });
            self.error(error);
            return;
        }
        // Mirror `viden_workflows::lanes` exactly, including its second-to-
        // millisecond scaling and its saturating arithmetic.
        let mut stats = self.lane.run_stats.unwrap_or_default();
        match observation {
            LaneRunObservation::Started => {
                stats.run_count = stats.run_count.saturating_add(1);
                self.open_run_started_at = Some(timestamp);
            }
            LaneRunObservation::Stopped { exit_code } => {
                if let Some(started_at) = self.open_run_started_at.take() {
                    stats.wall_time_ms = stats
                        .wall_time_ms
                        .saturating_add(timestamp.saturating_sub(started_at).saturating_mul(1_000));
                }
                stats.last_exit_code = exit_code;
            }
            LaneRunObservation::Applied { diff_bytes } => {
                stats.diff_bytes = stats.diff_bytes.saturating_add(diff_bytes);
            }
        }
        self.lane.run_stats = Some(stats);
        self.emit(RuntimeEventKind::LaneUpdated {
            lane: self.lane.clone(),
        });
    }

    fn change_status(&mut self, status: LaneStatus, summary: impl Into<String>) -> Result<(), ()> {
        let summary = summary.into();
        let event = LaneEvent::status_changed(
            fresh_id("lane-event"),
            self.lane.id.clone(),
            status,
            summary.clone(),
            now_timestamp(),
            self.owner.session_id.clone(),
        );
        if let Err(error) = self.persistence.append(&event) {
            self.emit(RuntimeEventKind::LaneRecoveryRequired {
                lane_id: self.lane.id.clone(),
                reason: error.clone(),
                next_action: "reload lane state and retry".to_string(),
            });
            self.error(error);
            return Err(());
        }
        self.lane.status = status;
        self.lane.summary = summary;
        self.emit(RuntimeEventKind::LaneUpdated {
            lane: self.lane.clone(),
        });
        Ok(())
    }

    fn output(&self, stream: &str, content: String) {
        self.emit(RuntimeEventKind::LaneOutputAppended {
            lane_id: self.lane.id.clone(),
            stream: stream.to_string(),
            content,
        });
    }

    fn fail(&mut self, error: String) {
        let _ = self.change_status(LaneStatus::Failed, error.clone());
        self.error(error);
    }

    fn reject(&self, command_id: String, reason: impl Into<String>) {
        self.emit(RuntimeEventKind::CommandRejected {
            command_id,
            reason: reason.into(),
        });
    }

    fn error(&self, message: String) {
        self.emit(RuntimeEventKind::Error {
            error: RuntimeErrorView {
                message,
                recoverable: true,
                hint: Some("inspect the lane receipt and retry".to_string()),
            },
        });
    }

    fn emit(&self, kind: RuntimeEventKind) {
        (self.events)(self.owner.clone(), kind);
    }
}

#[allow(clippy::too_many_arguments)]
fn lane_approval(
    request_id: &str,
    audit_id: &str,
    expires_at: u64,
    allowed_scopes: Vec<ApprovalScope>,
    owner: &RuntimeOwner,
    lane: &AgentLaneRecord,
    tool: &ToolSpec,
    input: &ToolInput,
) -> ApprovalRequestView {
    ApprovalRequestView {
        id: request_id.to_string(),
        tool_name: tool.name.clone(),
        title: format!("Approve {}", tool.name),
        message: format!("Approve `{}` for lane `{}`", tool.name, lane.id),
        input_preview: input
            .iter()
            .map(|(key, value)| format!("{key}: {value}"))
            .collect::<Vec<_>>()
            .join("\n"),
        is_mutating: true,
        reason: Some("lane mutation policy requires approval".to_string()),
        owner: owner.clone(),
        risk: viden_types::ApprovalRisk::Medium,
        target: viden_types::ApprovalTarget {
            kind: if matches!(tool.name.as_str(), "lane_apply" | "lane_resolve_conflict") {
                "repository"
            } else {
                "worktree"
            }
            .to_string(),
            display: input
                .get("path")
                .cloned()
                .unwrap_or_else(|| lane.id.clone()),
            canonical_ref: input.get("path").cloned(),
        },
        allowed_scopes,
        policy_reason_key: "lane.requires_approval".to_string(),
        policy_reason_args: Default::default(),
        expires_at,
        default_action: viden_types::ApprovalDefaultAction::Deny,
        audit_id: audit_id.to_string(),
        decision_context: None,
    }
}
