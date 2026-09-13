use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::thread::{self, JoinHandle};

use sha2::{Digest, Sha256};
use viden_permissions::PermissionEngine;
use viden_types::{
    AgentLaneRecord, AgentRole, AgentRoute, ApprovalResponse, DataEgressPolicy, ExecutionTarget,
    GateStrength, LaneBudget, LaneRuntimeOwnerBinding, LaneStatus, MutationPolicy, RuntimeCommand,
    RuntimeEventKind, RuntimeOwner, StarterLanePreset, StarterLanePreview,
    StarterLanePreviewInvalidationReason, StarterLaneReceipt, StarterLaneRequest, WorkMode,
    WorkspaceEligibility, fresh_id, now_timestamp,
};
use viden_workflows::{
    lanes::{LaneEvent, LaneEventKind},
    stores::WorkflowStore,
};

use crate::lane_runtime::{
    LaneEffectExecutor, LaneEffectRequest, LaneEffectResult, resolve_lane_target,
};
use crate::lane_worker::{
    LaneApprovalResolver, LaneEventSink, LanePendingApproval, LaneTerminalKind, LaneWorkerHandle,
    LaneWorkerMessage,
};

/// Redacts a `RuntimeCommand` before the lane supervisor publishes it on a
/// `LaneCommandAccepted` event.
///
/// The redaction policy is a property of the runtime's event contract, not of
/// lane orchestration, so it is injected instead of imported: the lane crate
/// decides which commands are announced, the runtime decides what an announced
/// command is allowed to reveal.
pub type LaneCommandRedactor = Arc<dyn Fn(&RuntimeCommand) -> RuntimeCommand + Send + Sync>;

pub trait LanePersistence: Send + Sync {
    fn append(&self, event: &LaneEvent) -> Result<(), String>;
    fn load_lanes(&self) -> Result<BTreeMap<String, AgentLaneRecord>, String>;
}

pub struct WorkflowLanePersistence(pub WorkflowStore);

impl LanePersistence for WorkflowLanePersistence {
    fn append(&self, event: &LaneEvent) -> Result<(), String> {
        self.0.append_lane_event_checked(event)
    }

    fn load_lanes(&self) -> Result<BTreeMap<String, AgentLaneRecord>, String> {
        let mut lanes = self.0.load_lane_state()?.lanes().clone();
        for event in self.0.load_lane_events()? {
            let Some(origin_session_id) = event.origin_session_id else {
                continue;
            };
            let lane_ids = match event.kind {
                LaneEventKind::LegacyImported { lanes, .. } => {
                    lanes.into_iter().map(|lane| lane.id).collect::<Vec<_>>()
                }
                _ => vec![event.lane_id],
            };
            for lane_id in lane_ids {
                let Some(lane) = lanes.get_mut(&lane_id) else {
                    continue;
                };
                if lane.active_session_ids.is_empty()
                    && !lane.active_session_ids.contains(&origin_session_id)
                {
                    lane.active_session_ids.push(origin_session_id.clone());
                }
            }
        }
        Ok(lanes)
    }
}

pub struct LaneSupervisor {
    repo: PathBuf,
    persistence: Arc<dyn LanePersistence>,
    permissions: Arc<Mutex<PermissionEngine>>,
    permission_epoch: AtomicU64,
    effects: Arc<dyn LaneEffectExecutor>,
    events: LaneEventSink,
    approvals: LaneApprovalResolver,
    redact_command: LaneCommandRedactor,
    work_mode: Arc<dyn Fn() -> WorkMode + Send + Sync>,
    approval_ttl_secs: u64,
    lanes: Arc<Mutex<BTreeMap<String, LaneWorkerHandle>>>,
    hydrated_lanes: Arc<Mutex<BTreeMap<String, AgentLaneRecord>>>,
    terminal_lanes: Arc<Mutex<BTreeMap<String, LaneTerminalKind>>>,
    starter_previews: Arc<Mutex<BTreeMap<String, PendingStarterLanePreview>>>,
    starter_creations: Arc<Mutex<BTreeMap<String, PendingStarterLaneCreation>>>,
    terminal_sender: Option<mpsc::Sender<crate::lane_worker::LaneTerminalCompletion>>,
    terminal_reaper: Option<JoinHandle<()>>,
    hydration_recoveries: Vec<AgentLaneRecord>,
    hydration_error: Option<String>,
}

#[derive(Clone)]
struct PendingStarterLanePreview {
    request: StarterLaneRequest,
    preview: StarterLanePreview,
    owner: RuntimeOwner,
}

#[derive(Clone)]
struct PendingStarterLaneCreation {
    preview: StarterLanePreview,
    owner: RuntimeOwner,
    approval_request_id: Option<String>,
}

struct StarterLaneEffectGuard {
    inner: Arc<dyn LaneEffectExecutor>,
    pending: Arc<Mutex<BTreeMap<String, PendingStarterLaneCreation>>>,
}

impl LaneEffectExecutor for StarterLaneEffectGuard {
    fn execute(&self, request: LaneEffectRequest) -> Result<LaneEffectResult, String> {
        if let LaneEffectRequest::Create { repo, lane } = &request {
            let pending = self
                .pending
                .lock()
                .map_err(|_| "starter lane creation registry poisoned".to_string())?
                .get(&lane.id)
                .cloned();
            if let Some(pending) = pending {
                validate_starter_lane_effect(repo, lane, &pending.preview)?;
            }
        }
        self.inner.execute(request)
    }

    fn apply_transactionally(
        &self,
        request: LaneEffectRequest,
        persist: &mut dyn FnMut() -> Result<(), String>,
    ) -> Result<LaneEffectResult, String> {
        self.inner.apply_transactionally(request, persist)
    }

    fn shutdown_lane(&self, lane_id: &str) -> Result<Option<i32>, String> {
        self.inner.shutdown_lane(lane_id)
    }

    fn compensate_create(&self, repo: &Path, lane: &AgentLaneRecord) -> Result<(), String> {
        self.inner.compensate_create(repo, lane)
    }
}

impl LaneSupervisor {
    #[cfg(any(test, feature = "testing"))]
    pub fn permission_template_snapshot_for_test(
        &self,
    ) -> Result<(viden_types::PermissionMode, u64), String> {
        Ok((self.permission_template()?.mode(), self.permission_epoch()))
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn lane_permission_snapshot_for_test(
        &self,
        lane_id: &str,
    ) -> Result<(viden_types::PermissionMode, u64), String> {
        let lanes = self
            .lanes
            .lock()
            .map_err(|_| "lane registry poisoned".to_string())?;
        let worker = lanes
            .get(lane_id)
            .ok_or_else(|| format!("lane `{lane_id}` is not active"))?;
        let (permissions, epoch) = worker.permission_snapshot()?;
        Ok((permissions.mode(), epoch))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo: PathBuf,
        persistence: Arc<dyn LanePersistence>,
        permissions: Arc<Mutex<PermissionEngine>>,
        effects: Arc<dyn LaneEffectExecutor>,
        events: LaneEventSink,
        approvals: LaneApprovalResolver,
        redact_command: LaneCommandRedactor,
        work_mode: Arc<dyn Fn() -> WorkMode + Send + Sync>,
        approval_ttl_secs: u64,
    ) -> Self {
        let repo = repo.canonicalize().unwrap_or(repo);
        let permissions = permissions
            .lock()
            .map(|permissions| {
                let mut scoped = PermissionEngine::new(repo.clone());
                scoped.restore_context(permissions.context_snapshot());
                scoped
            })
            .unwrap_or_else(|_| PermissionEngine::new(repo.clone()));
        let permissions = Arc::new(Mutex::new(permissions));
        let (mut hydrated_lanes, mut hydration_error) = match persistence.load_lanes() {
            Ok(lanes) => (lanes, None),
            Err(error) => (
                BTreeMap::new(),
                Some(format!("lane hydration failed: {error}")),
            ),
        };
        let mut hydration_recoveries = Vec::new();
        if hydration_error.is_none() {
            for lane in hydrated_lanes.values_mut().filter(|lane| {
                matches!(
                    lane.status,
                    viden_types::LaneStatus::Starting
                        | viden_types::LaneStatus::Running
                        | viden_types::LaneStatus::WaitingApproval
                )
            }) {
                let summary = "lane requires recovery after runtime restart";
                let event = LaneEvent::status_changed(
                    fresh_id("lane-event"),
                    lane.id.clone(),
                    viden_types::LaneStatus::Blocked,
                    summary,
                    now_timestamp(),
                    lane.active_session_ids.first().cloned(),
                );
                if let Err(error) = persistence.append(&event) {
                    hydration_error = Some(format!(
                        "lane hydration recovery failed for `{}`: {error}",
                        lane.id
                    ));
                    break;
                }
                lane.status = viden_types::LaneStatus::Blocked;
                lane.summary = summary.to_string();
                hydration_recoveries.push(lane.clone());
            }
        }
        let terminal_lanes: BTreeMap<String, LaneTerminalKind> = hydrated_lanes
            .iter()
            .filter(|(_, lane)| lane.status == viden_types::LaneStatus::Archived)
            .map(|(lane_id, lane)| {
                let kind = if lane.summary == "lane cleaned up" {
                    LaneTerminalKind::Cleaned
                } else {
                    LaneTerminalKind::Archived
                };
                (lane_id.clone(), kind)
            })
            .collect();
        let lanes = Arc::new(Mutex::new(BTreeMap::new()));
        let hydrated_lanes = Arc::new(Mutex::new(hydrated_lanes));
        let terminal_lanes = Arc::new(Mutex::new(terminal_lanes));
        let starter_previews = Arc::new(Mutex::new(BTreeMap::new()));
        let starter_creations = Arc::new(Mutex::new(BTreeMap::new()));
        let (terminal_sender, terminal_receiver) =
            mpsc::channel::<crate::lane_worker::LaneTerminalCompletion>();
        let reaper_lanes = Arc::clone(&lanes);
        let reaper_hydrated = Arc::clone(&hydrated_lanes);
        let reaper_terminal = Arc::clone(&terminal_lanes);
        let terminal_reaper = thread::spawn(move || {
            while let Ok(completion) = terminal_receiver.recv() {
                let lane_id = completion.lane.id.clone();
                if let Ok(mut hydrated) = reaper_hydrated.lock() {
                    hydrated.insert(lane_id.clone(), completion.lane);
                }
                if let Ok(mut terminal) = reaper_terminal.lock() {
                    terminal.insert(lane_id.clone(), completion.kind);
                }
                if let Ok(mut lanes) = reaper_lanes.lock() {
                    lanes.remove(&lane_id);
                }
            }
        });
        Self {
            repo,
            persistence,
            permissions,
            permission_epoch: AtomicU64::new(0),
            effects,
            events,
            approvals,
            redact_command,
            work_mode,
            approval_ttl_secs,
            lanes,
            hydrated_lanes,
            terminal_lanes,
            starter_previews,
            starter_creations,
            terminal_sender: Some(terminal_sender),
            terminal_reaper: Some(terminal_reaper),
            hydration_recoveries,
            hydration_error,
        }
    }

    pub fn handles(command: &RuntimeCommand) -> bool {
        matches!(
            command,
            RuntimeCommand::PreviewDefaultStarterLane { .. }
                | RuntimeCommand::PreviewStarterLane { .. }
                | RuntimeCommand::CreateStarterLane { .. }
                | RuntimeCommand::CreateLane { .. }
                | RuntimeCommand::StartLane { .. }
                | RuntimeCommand::StopLane { .. }
                | RuntimeCommand::AttachLane { .. }
                | RuntimeCommand::DetachLane { .. }
                | RuntimeCommand::SendLaneInput { .. }
                | RuntimeCommand::AcceptLaneOutput { .. }
                | RuntimeCommand::ReviseLaneOutput { .. }
                | RuntimeCommand::DiscardLaneOutput { .. }
                | RuntimeCommand::ApplyLaneChanges { .. }
                | RuntimeCommand::ResolveLaneConflict { .. }
                | RuntimeCommand::ArchiveLane { .. }
                | RuntimeCommand::CleanupLane { .. }
        )
    }

    pub fn hydration_recoveries(&self) -> &[AgentLaneRecord] {
        &self.hydration_recoveries
    }

    pub fn sync_permissions(
        &self,
        permissions: PermissionEngine,
        permission_epoch: u64,
    ) -> Result<(), String> {
        let mut scoped = PermissionEngine::new(self.repo.clone());
        scoped.restore_context(permissions.context_snapshot());
        *self
            .permissions
            .lock()
            .map_err(|_| "lane permission registry poisoned".to_string())? = scoped.clone();
        let lanes = self
            .lanes
            .lock()
            .map_err(|_| "lane registry poisoned".to_string())?;
        for worker in lanes.values() {
            worker.sync_permissions(scoped.clone(), permission_epoch)?;
        }
        self.permission_epoch
            .store(permission_epoch, Ordering::Release);
        Ok(())
    }

    fn permission_template(&self) -> Result<PermissionEngine, String> {
        self.permissions
            .lock()
            .map(|permissions| permissions.clone())
            .map_err(|_| "lane permission registry poisoned".to_string())
    }

    fn permission_epoch(&self) -> u64 {
        self.permission_epoch.load(Ordering::Acquire)
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn worker_finished_for_test(&self, lane_id: &str) -> bool {
        self.lanes
            .lock()
            .ok()
            .and_then(|lanes| lanes.get(lane_id).map(LaneWorkerHandle::is_finished))
            .unwrap_or(true)
    }

    /// Explicit retirement signal for a lane worker that reached a durable
    /// terminal state. The reaper records the terminal kind before removing
    /// (and therefore dropping and joining) the worker handle, so requiring
    /// both conditions proves the worker thread already exited. Unlike an
    /// empty-registry check, this predicate is monotone: it cannot be
    /// satisfied before the lane's worker was ever spawned.
    #[cfg(any(test, feature = "testing"))]
    pub fn worker_retired_for_test(&self, lane_id: &str) -> bool {
        let recorded_terminal = self
            .terminal_lanes
            .lock()
            .map(|terminal| terminal.contains_key(lane_id))
            .unwrap_or(false);
        recorded_terminal
            && self
                .lanes
                .lock()
                .map(|lanes| !lanes.contains_key(lane_id))
                .unwrap_or(false)
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn active_worker_count_for_test(&self) -> usize {
        self.lanes
            .lock()
            .map(|lanes| lanes.len())
            .unwrap_or_default()
    }

    pub fn send(
        &self,
        owner: RuntimeOwner,
        command_id: String,
        command: RuntimeCommand,
    ) -> Result<(), String> {
        if let RuntimeCommand::PreviewDefaultStarterLane { preset } = command {
            let eligibility = workspace_eligibility(&self.repo);
            self.emit(
                owner.clone(),
                RuntimeEventKind::WorkspaceEligibilityUpdated {
                    eligibility: eligibility.clone(),
                },
            );
            if !eligibility.can_create_lane {
                self.reject(
                    owner,
                    command_id,
                    eligibility
                        .diagnostic
                        .unwrap_or_else(|| "workspace_ineligible".to_string()),
                );
                return Ok(());
            }
            let lane_id = fresh_id("lane");
            let request = StarterLaneRequest {
                lane_id: lane_id.clone(),
                preset,
                branch: (eligibility.is_git_repository && eligibility.has_head)
                    .then(|| format!("viden/{lane_id}")),
                worktree_path: None,
            };
            let mut generated_owner = owner;
            generated_owner.lane_id = Some(lane_id);
            return self.preview_starter_lane(generated_owner, command_id, request);
        }
        let lane_id = command_lane_id(&command).to_string();
        if owner.lane_id.as_deref() != Some(lane_id.as_str()) {
            self.reject(
                owner,
                command_id,
                format!("lane `{lane_id}` owner mismatch"),
            );
            return Ok(());
        }
        if let RuntimeCommand::PreviewStarterLane { request } = command {
            return self.preview_starter_lane(owner, command_id, request);
        }
        // This gate deliberately precedes registry lookup, persistence, and effect dispatch.
        if Self::handles(&command) && (self.work_mode)() != WorkMode::Build {
            if let RuntimeCommand::CreateStarterLane { preview_id, .. } = &command
                && let Ok(Some(pending)) = self.consume_starter_preview(&owner, preview_id)
            {
                self.invalidate_starter_preview(
                    pending.preview,
                    StarterLanePreviewInvalidationReason::PlanModeDenied,
                );
            }
            self.reject(
                owner,
                command_id,
                "Plan mode blocks lane mutations".to_string(),
            );
            return Ok(());
        }
        if let Some(error) = &self.hydration_error {
            self.reject(owner.clone(), command_id, error.clone());
            self.error(owner, error.clone());
            return Ok(());
        }
        if !matches!(command, RuntimeCommand::CreateStarterLane { .. }) {
            let starter_creation_pending = self
                .starter_creations
                .lock()
                .map_err(|_| "starter lane creation registry poisoned".to_string())?
                .contains_key(&lane_id);
            if starter_creation_pending {
                self.reject(
                    owner,
                    command_id,
                    format!("starter lane `{lane_id}` creation is already pending"),
                );
                return Ok(());
            }
        }
        let mut lanes = self
            .lanes
            .lock()
            .map_err(|_| "lane registry poisoned".to_string())?;
        self.reap_terminal_lanes(&mut lanes)?;
        if let RuntimeCommand::CreateLane { lane } = command {
            drop(lanes);
            let accepted = RuntimeCommand::CreateLane { lane: lane.clone() };
            return self.create(owner, command_id, lane, accepted);
        }
        if let RuntimeCommand::CreateStarterLane {
            request,
            preview_id,
            content_sha256,
        } = command
        {
            drop(lanes);
            return self.create_starter_lane(
                owner,
                command_id,
                request,
                preview_id,
                content_sha256,
            );
        }
        let terminal = self
            .terminal_lanes
            .lock()
            .map_err(|_| "terminal lane registry poisoned".to_string())?
            .get(&lane_id)
            .copied();
        if let Some(terminal) = terminal {
            let archive_to_cleanup = terminal == LaneTerminalKind::Archived
                && matches!(command, RuntimeCommand::CleanupLane { .. });
            if archive_to_cleanup {
                self.terminal_lanes
                    .lock()
                    .map_err(|_| "terminal lane registry poisoned".to_string())?
                    .remove(&lane_id);
            } else {
                self.reject(
                    owner,
                    command_id,
                    format!("lane `{lane_id}` already reached a durable terminal state"),
                );
                return Ok(());
            }
        }
        let mut spawned_worker = false;
        if !lanes.contains_key(&lane_id) {
            let hydrated = self
                .hydrated_lanes
                .lock()
                .map_err(|_| "hydrated lane registry poisoned".to_string())?
                .get(&lane_id)
                .cloned();
            if let Some(lane) = hydrated {
                if !lane.active_session_ids.is_empty()
                    && !owner
                        .session_id
                        .as_ref()
                        .is_some_and(|session_id| lane.active_session_ids.contains(session_id))
                {
                    self.reject(
                        owner,
                        command_id,
                        format!("lane `{lane_id}` owner mismatch"),
                    );
                    return Ok(());
                }
                lanes.insert(
                    lane_id.clone(),
                    LaneWorkerHandle::spawn(
                        owner.clone(),
                        lane,
                        self.repo.clone(),
                        Arc::clone(&self.persistence),
                        self.permission_template()?,
                        self.permission_epoch(),
                        self.worker_effects(),
                        self.worker_event_sink(),
                        Arc::clone(&self.approvals),
                        self.approval_ttl_secs,
                        true,
                        self.terminal_sender
                            .as_ref()
                            .expect("lane terminal reaper sender")
                            .clone(),
                    ),
                );
                spawned_worker = true;
            }
        }
        let Some(worker) = lanes.get(&lane_id) else {
            self.reject(
                owner,
                command_id,
                format!("lane `{lane_id}` is not registered"),
            );
            return Ok(());
        };
        if worker.owner != owner {
            self.reject(
                owner,
                command_id,
                format!("lane `{lane_id}` owner mismatch"),
            );
            return Ok(());
        }
        self.emit(
            owner.clone(),
            RuntimeEventKind::LaneCommandAccepted {
                command_id: command_id.clone(),
                command: (self.redact_command)(&command),
            },
        );
        if spawned_worker {
            // Publish the binding before the worker can emit a terminal Lane
            // update, so replay can never resurrect a cleared owner.
            self.emit_worker_binding(&lane_id, worker)?;
        }
        worker.send(LaneWorkerMessage::Command {
            command_id,
            command: Box::new(command),
        })?;
        Ok(())
    }

    pub fn respond_to_approval(
        &self,
        owner: &RuntimeOwner,
        command_id: &str,
        request_id: &str,
        response: ApprovalResponse,
    ) -> Result<Option<bool>, String> {
        let lanes = self
            .lanes
            .lock()
            .map_err(|_| "lane registry poisoned".to_string())?;
        let Some((lane_id, worker)) = lanes
            .iter()
            .find(|(_, worker)| worker.owns_pending_approval(request_id))
        else {
            return Ok(None);
        };
        if &worker.owner != owner {
            self.reject(
                owner.clone(),
                command_id.to_string(),
                format!("approval request `{request_id}` owner mismatch for lane `{lane_id}`"),
            );
            return Ok(Some(false));
        }
        let (permissions, permission_epoch) = worker.permission_snapshot()?;
        let (completion, completed) = mpsc::channel();
        worker.send(LaneWorkerMessage::ResumeApproval {
            request_id: request_id.to_string(),
            response,
            permissions,
            permission_epoch,
            completion,
        })?;
        // This is the lane mutation linearization barrier: later supervisor
        // commands cannot publish state until the worker has resolved and
        // completed (or safely rejected) the approved effect.
        completed
            .recv()
            .map_err(|_| "lane approval worker terminated before completion".to_string())?;
        Ok(Some(true))
    }

    /// The pending approval this supervisor owns for `request_id`, with the
    /// live permission epoch its worker would re-check the answer against.
    ///
    /// Read-only. It exists so the runtime can audit a lane-path decision
    /// under the pre-minted id before it forwards the response
    /// (compatibility follow-up 15); `crates/lanes` still writes no audit row,
    /// because audit is runtime-owned policy that is injected into it.
    pub fn pending_approval(
        &self,
        request_id: &str,
    ) -> Result<Option<(LanePendingApproval, u64)>, String> {
        let lanes = self
            .lanes
            .lock()
            .map_err(|_| "lane registry poisoned".to_string())?;
        let Some(worker) = lanes
            .values()
            .find(|worker| worker.owns_pending_approval(request_id))
        else {
            return Ok(None);
        };
        let Some(pending) = worker.pending_approval(request_id) else {
            return Ok(None);
        };
        let (_, permission_epoch) = worker.permission_snapshot()?;
        Ok(Some((pending, permission_epoch)))
    }

    pub fn cancel(&self, owner: &RuntimeOwner, command_id: String) -> Result<bool, String> {
        let Some(lane_id) = owner.lane_id.as_deref() else {
            return Ok(false);
        };
        let pending_starter = self
            .starter_creations
            .lock()
            .map_err(|_| "starter lane creation registry poisoned".to_string())?
            .get(lane_id)
            .cloned();
        if let Some(pending) = pending_starter {
            if &pending.owner != owner {
                return Err(format!("lane `{lane_id}` owner mismatch"));
            }
            let Some(request_id) = pending.approval_request_id else {
                self.reject(
                    owner.clone(),
                    command_id,
                    format!("starter lane `{lane_id}` approval is not ready to cancel"),
                );
                return Ok(true);
            };
            if self.respond_to_approval(
                owner,
                &command_id,
                &request_id,
                ApprovalResponse::deny(None),
            )? != Some(true)
            {
                return Err(format!(
                    "starter lane `{lane_id}` approval disappeared before cancellation"
                ));
            }
            self.emit(
                owner.clone(),
                RuntimeEventKind::CommandAccepted {
                    command_id,
                    command: RuntimeCommand::CancelActiveTurn,
                },
            );
            return Ok(true);
        }
        let lanes = self
            .lanes
            .lock()
            .map_err(|_| "lane registry poisoned".to_string())?;
        let Some(worker) = lanes.get(lane_id) else {
            return Ok(false);
        };
        if &worker.owner != owner {
            return Err(format!("lane `{lane_id}` owner mismatch"));
        }
        worker.send(LaneWorkerMessage::Cancel { command_id })?;
        Ok(true)
    }

    fn preview_starter_lane(
        &self,
        owner: RuntimeOwner,
        command_id: String,
        request: StarterLaneRequest,
    ) -> Result<(), String> {
        let preview = match resolve_starter_lane_preview(&self.repo, &owner, &request) {
            Ok(preview) => preview,
            Err(reason) => {
                self.reject(owner, command_id, reason);
                return Ok(());
            }
        };
        self.starter_previews
            .lock()
            .map_err(|_| "starter lane preview registry poisoned".to_string())?
            .insert(
                preview.preview_id.clone(),
                PendingStarterLanePreview {
                    request: request.clone(),
                    preview: preview.clone(),
                    owner: owner.clone(),
                },
            );
        self.emit(
            owner.clone(),
            RuntimeEventKind::CommandAccepted {
                command_id,
                command: (self.redact_command)(&RuntimeCommand::PreviewStarterLane { request }),
            },
        );
        self.emit(owner, RuntimeEventKind::StarterLanePreviewed { preview });
        Ok(())
    }

    fn create_starter_lane(
        &self,
        owner: RuntimeOwner,
        command_id: String,
        request: StarterLaneRequest,
        preview_id: String,
        content_sha256: String,
    ) -> Result<(), String> {
        let pending = match self.consume_starter_preview(&owner, &preview_id) {
            Ok(Some(pending)) => pending,
            Ok(None) => {
                self.reject(
                    owner,
                    command_id,
                    format!("starter lane preview `{preview_id}` is unknown or already consumed"),
                );
                return Ok(());
            }
            Err(reason) => {
                self.reject(owner, command_id, reason);
                return Ok(());
            }
        };
        let rejection = if pending.preview.owner != owner {
            Some((
                StarterLanePreviewInvalidationReason::RequestChanged,
                "starter lane preview owner mismatch".to_string(),
            ))
        } else if pending.request != request {
            Some((
                StarterLanePreviewInvalidationReason::RequestChanged,
                "starter lane request changed after preview".to_string(),
            ))
        } else if pending.preview.content_sha256 != content_sha256
            || !starter_lane_preview_hash(&pending.preview)
                .is_ok_and(|actual| actual == content_sha256)
        {
            Some((
                StarterLanePreviewInvalidationReason::HashMismatch,
                "starter lane preview hash mismatch".to_string(),
            ))
        } else if starter_lane_base_revision(&self.repo, &pending.preview.lane).as_deref()
            != Ok(pending.preview.base_revision.as_str())
        {
            Some((
                StarterLanePreviewInvalidationReason::BaseRevisionChanged,
                "starter lane base revision is stale".to_string(),
            ))
        } else if pending.preview.lane.worktree.is_some()
            && Path::new(&pending.preview.worktree_path).exists()
        {
            Some((
                StarterLanePreviewInvalidationReason::WorktreeUnavailable,
                "starter lane worktree already exists".to_string(),
            ))
        } else if pending
            .preview
            .lane
            .branch
            .as_deref()
            .is_some_and(|branch| git_branch_exists(&self.repo, branch).unwrap_or(true))
        {
            Some((
                StarterLanePreviewInvalidationReason::BranchUnavailable,
                "starter lane branch already exists or cannot be inspected".to_string(),
            ))
        } else {
            None
        };
        if let Some((reason_code, reason)) = rejection {
            self.invalidate_starter_preview(pending.preview, reason_code);
            self.reject(owner, command_id, reason);
            return Ok(());
        }
        let lane_id = pending.preview.lane.id.clone();
        let live_conflict = self
            .lanes
            .lock()
            .map_err(|_| "lane registry poisoned".to_string())?
            .get(&lane_id)
            .is_some_and(|worker| worker.owner != owner || worker.is_registered());
        let durable_conflict = self
            .hydrated_lanes
            .lock()
            .map_err(|_| "hydrated lane registry poisoned".to_string())?
            .contains_key(&lane_id)
            || self
                .terminal_lanes
                .lock()
                .map_err(|_| "terminal lane registry poisoned".to_string())?
                .contains_key(&lane_id);
        if live_conflict || durable_conflict {
            self.invalidate_starter_preview(
                pending.preview,
                StarterLanePreviewInvalidationReason::LaneAlreadyRegistered,
            );
            self.reject(
                owner,
                command_id,
                format!("starter lane `{lane_id}` is already registered"),
            );
            return Ok(());
        }

        let accepted = RuntimeCommand::CreateStarterLane {
            request,
            preview_id,
            content_sha256,
        };
        let creation_reserved = {
            let mut creations = self
                .starter_creations
                .lock()
                .map_err(|_| "starter lane creation registry poisoned".to_string())?;
            if creations.contains_key(&lane_id) {
                false
            } else {
                creations.insert(
                    lane_id.clone(),
                    PendingStarterLaneCreation {
                        preview: pending.preview.clone(),
                        owner: owner.clone(),
                        approval_request_id: None,
                    },
                );
                true
            }
        };
        if !creation_reserved {
            self.invalidate_starter_preview(
                pending.preview,
                StarterLanePreviewInvalidationReason::LaneAlreadyRegistered,
            );
            self.reject(
                owner,
                command_id,
                format!("starter lane `{lane_id}` creation is already pending"),
            );
            return Ok(());
        }
        let result = self.create(owner, command_id, pending.preview.lane, accepted);
        if result.is_err() {
            let failed = self
                .starter_creations
                .lock()
                .ok()
                .and_then(|mut creations| creations.remove(&lane_id));
            if let Some(failed) = failed {
                self.invalidate_starter_preview(
                    failed.preview,
                    StarterLanePreviewInvalidationReason::EffectFailed,
                );
            }
        }
        result
    }

    fn consume_starter_preview(
        &self,
        owner: &RuntimeOwner,
        preview_id: &str,
    ) -> Result<Option<PendingStarterLanePreview>, String> {
        let mut previews = self
            .starter_previews
            .lock()
            .map_err(|_| "starter lane preview registry poisoned".to_string())?;
        if previews
            .get(preview_id)
            .is_some_and(|pending| &pending.owner != owner)
        {
            return Err("starter lane preview owner mismatch".to_string());
        }
        Ok(previews.remove(preview_id))
    }

    fn invalidate_starter_preview(
        &self,
        preview: StarterLanePreview,
        reason: StarterLanePreviewInvalidationReason,
    ) {
        self.emit(
            preview.owner.clone(),
            RuntimeEventKind::StarterLanePreviewInvalidated {
                owner: preview.owner,
                preview_id: preview.preview_id,
                reason,
            },
        );
    }

    fn worker_effects(&self) -> Arc<dyn LaneEffectExecutor> {
        Arc::new(StarterLaneEffectGuard {
            inner: Arc::clone(&self.effects),
            pending: Arc::clone(&self.starter_creations),
        })
    }

    fn worker_event_sink(&self) -> LaneEventSink {
        let events = Arc::clone(&self.events);
        let pending = Arc::clone(&self.starter_creations);
        Arc::new(move |owner, kind| {
            let lane_id = owner.lane_id.clone();
            let approval_request_id = match &kind {
                RuntimeEventKind::ApprovalRequested { approval } => Some(approval.id.clone()),
                _ => None,
            };
            let created_lane = match &kind {
                RuntimeEventKind::LaneUpdated { lane } => Some(lane.clone()),
                _ => None,
            };
            let terminal_failure = match &kind {
                RuntimeEventKind::CommandRejected { .. } => {
                    Some(StarterLanePreviewInvalidationReason::PermissionDenied)
                }
                RuntimeEventKind::LaneRecoveryRequired { .. } | RuntimeEventKind::Error { .. } => {
                    Some(StarterLanePreviewInvalidationReason::EffectFailed)
                }
                _ => None,
            };
            if let (Some(lane_id), Some(request_id)) = (&lane_id, approval_request_id)
                && let Ok(mut pending) = pending.lock()
                && let Some(creation) = pending.get_mut(lane_id)
                && creation.owner == owner
            {
                creation.approval_request_id = Some(request_id);
            }
            events(owner.clone(), kind);
            let Some(lane_id) = lane_id else {
                return;
            };
            if let Some(lane) = created_lane {
                let creation = pending
                    .lock()
                    .ok()
                    .and_then(|mut pending| pending.remove(&lane_id));
                if let Some(creation) = creation {
                    let preview = creation.preview;
                    events(
                        owner.clone(),
                        RuntimeEventKind::StarterLaneCreated {
                            receipt: StarterLaneReceipt {
                                preview_id: preview.preview_id,
                                content_sha256: preview.content_sha256,
                                lane,
                                branch: preview.branch,
                                worktree_path: preview.worktree_path,
                                base_revision: preview.base_revision,
                                owner: creation.owner,
                            },
                        },
                    );
                }
            } else if let Some(reason) = terminal_failure
                && let Some(creation) = pending
                    .lock()
                    .ok()
                    .and_then(|mut pending| pending.remove(&lane_id))
            {
                let preview = creation.preview;
                events(
                    owner,
                    RuntimeEventKind::StarterLanePreviewInvalidated {
                        owner: preview.owner,
                        preview_id: preview.preview_id,
                        reason,
                    },
                );
            }
        })
    }

    fn create(
        &self,
        owner: RuntimeOwner,
        command_id: String,
        mut lane: AgentLaneRecord,
        accepted_command: RuntimeCommand,
    ) -> Result<(), String> {
        if let Some(error) = &self.hydration_error {
            self.reject(owner.clone(), command_id, error.clone());
            self.error(owner, error.clone());
            return Ok(());
        }
        if let Some(session_id) = owner.session_id.clone()
            && !lane.active_session_ids.contains(&session_id)
        {
            lane.active_session_ids.push(session_id);
        }
        let mut lanes = self
            .lanes
            .lock()
            .map_err(|_| "lane registry poisoned".to_string())?;
        if let Some(worker) = lanes.get(&lane.id) {
            if worker.owner == owner && !worker.is_registered() {
                self.emit(
                    owner.clone(),
                    RuntimeEventKind::LaneCommandAccepted {
                        command_id: command_id.clone(),
                        command: (self.redact_command)(&accepted_command),
                    },
                );
                return worker.send(LaneWorkerMessage::Command {
                    command_id,
                    command: Box::new(RuntimeCommand::CreateLane { lane }),
                });
            }
            self.reject(
                owner,
                command_id,
                format!("lane `{}` is already registered", lane.id),
            );
            return Ok(());
        }
        if self
            .hydrated_lanes
            .lock()
            .map_err(|_| "hydrated lane registry poisoned".to_string())?
            .contains_key(&lane.id)
        {
            self.reject(
                owner,
                command_id,
                format!("lane `{}` is already registered", lane.id),
            );
            return Ok(());
        }
        self.emit(
            owner.clone(),
            RuntimeEventKind::LaneCommandAccepted {
                command_id: command_id.clone(),
                command: (self.redact_command)(&accepted_command),
            },
        );
        let worker = LaneWorkerHandle::spawn(
            owner.clone(),
            lane.clone(),
            self.repo.clone(),
            Arc::clone(&self.persistence),
            self.permission_template()?,
            self.permission_epoch(),
            self.worker_effects(),
            self.worker_event_sink(),
            Arc::clone(&self.approvals),
            self.approval_ttl_secs,
            false,
            self.terminal_sender
                .as_ref()
                .expect("lane terminal reaper sender")
                .clone(),
        );
        let lane_id = lane.id.clone();
        lanes.insert(lane_id.clone(), worker);
        let worker = lanes.get(&lane_id).expect("freshly inserted Lane worker");
        // Bind the exact live handle before its first command can publish Lane
        // state. The fresh worker is waiting on this sender at this boundary.
        self.emit_worker_binding(&lane_id, worker)?;
        worker.send(LaneWorkerMessage::Command {
            command_id,
            command: Box::new(RuntimeCommand::CreateLane { lane }),
        })?;
        Ok(())
    }

    fn emit_worker_binding(&self, lane_id: &str, worker: &LaneWorkerHandle) -> Result<(), String> {
        if worker.owner.lane_id.as_deref() != Some(lane_id) {
            return Err(format!("lane `{lane_id}` worker owner mismatch"));
        }
        let worker_owner = worker.owner.clone();
        self.emit(
            worker_owner.clone(),
            RuntimeEventKind::LaneRuntimeOwnerBound {
                binding: LaneRuntimeOwnerBinding {
                    lane_id: lane_id.to_string(),
                    owner: worker_owner,
                },
            },
        );
        Ok(())
    }

    fn emit(&self, owner: RuntimeOwner, kind: RuntimeEventKind) {
        (self.events)(owner, kind);
    }
    fn reject(&self, owner: RuntimeOwner, command_id: String, reason: String) {
        self.emit(
            owner,
            RuntimeEventKind::CommandRejected { command_id, reason },
        );
    }
    fn error(&self, owner: RuntimeOwner, message: String) {
        self.emit(
            owner,
            RuntimeEventKind::Error {
                error: viden_types::RuntimeErrorView {
                    message,
                    recoverable: true,
                    hint: Some("inspect lane recovery state".to_string()),
                },
            },
        );
    }

    fn reap_terminal_lanes(
        &self,
        lanes: &mut BTreeMap<String, LaneWorkerHandle>,
    ) -> Result<(), String> {
        let completions = lanes
            .iter()
            .filter_map(|(lane_id, worker)| {
                worker
                    .take_terminal_completion()
                    .map(|completion| (lane_id.clone(), completion))
            })
            .collect::<Vec<_>>();
        if completions.is_empty() {
            return Ok(());
        }
        let mut hydrated = self
            .hydrated_lanes
            .lock()
            .map_err(|_| "hydrated lane registry poisoned".to_string())?;
        let mut terminal = self
            .terminal_lanes
            .lock()
            .map_err(|_| "terminal lane registry poisoned".to_string())?;
        for (lane_id, completion) in completions {
            hydrated.insert(lane_id.clone(), completion.lane);
            terminal.insert(lane_id.clone(), completion.kind);
            lanes.remove(&lane_id);
        }
        Ok(())
    }
}

pub fn workspace_eligibility(repo: &Path) -> WorkspaceEligibility {
    let workspace_available = repo
        .canonicalize()
        .is_ok_and(|canonical| canonical.is_dir());
    let is_git_repository = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(repo)
        .output()
        .is_ok_and(|output| output.status.success());
    let has_head = is_git_repository
        && Command::new("git")
            .args(["rev-parse", "--verify", "HEAD^{commit}"])
            .current_dir(repo)
            .output()
            .is_ok_and(|output| output.status.success());
    WorkspaceEligibility {
        is_git_repository,
        has_head,
        can_create_lane: workspace_available,
        diagnostic: (!workspace_available).then(|| "workspace_root_unavailable".to_string()),
    }
}

impl Drop for LaneSupervisor {
    fn drop(&mut self) {
        if let Ok(mut lanes) = self.lanes.lock() {
            lanes.clear();
        }
        self.terminal_sender.take();
        if let Some(reaper) = self.terminal_reaper.take() {
            let _ = reaper.join();
        }
    }
}

fn command_lane_id(command: &RuntimeCommand) -> &str {
    match command {
        RuntimeCommand::PreviewStarterLane { request }
        | RuntimeCommand::CreateStarterLane { request, .. } => &request.lane_id,
        RuntimeCommand::CreateLane { lane } => &lane.id,
        RuntimeCommand::StartLane { lane_id, .. }
        | RuntimeCommand::StopLane { lane_id }
        | RuntimeCommand::AttachLane { lane_id }
        | RuntimeCommand::DetachLane { lane_id }
        | RuntimeCommand::SendLaneInput { lane_id, .. }
        | RuntimeCommand::AcceptLaneOutput { lane_id, .. }
        | RuntimeCommand::ReviseLaneOutput { lane_id, .. }
        | RuntimeCommand::DiscardLaneOutput { lane_id, .. }
        | RuntimeCommand::ApplyLaneChanges { lane_id, .. }
        | RuntimeCommand::ResolveLaneConflict { lane_id, .. }
        | RuntimeCommand::ArchiveLane { lane_id, .. }
        | RuntimeCommand::CleanupLane { lane_id, .. } => lane_id,
        _ => "",
    }
}

fn resolve_starter_lane_preview(
    repo: &Path,
    owner: &RuntimeOwner,
    request: &StarterLaneRequest,
) -> Result<StarterLanePreview, String> {
    validate_starter_lane_id(&request.lane_id)?;
    let eligibility = workspace_eligibility(repo);
    if !eligibility.can_create_lane {
        return Err(eligibility
            .diagnostic
            .unwrap_or_else(|| "workspace_ineligible".to_string()));
    }
    if !(eligibility.is_git_repository && eligibility.has_head) {
        if request
            .branch
            .as_deref()
            .is_some_and(|branch| !branch.is_empty())
            || request
                .worktree_path
                .as_deref()
                .is_some_and(|worktree| !worktree.is_empty())
        {
            return Err("non-Git starter lanes cannot request a branch or worktree".to_string());
        }
        let lane = starter_lane_record(request, None, None);
        let workspace_path = resolve_lane_target(repo, &lane, false)?;
        let mut preview = StarterLanePreview {
            preview_id: fresh_id("starter-lane-preview"),
            content_sha256: String::new(),
            owner: owner.clone(),
            lane,
            branch: String::new(),
            worktree_path: workspace_path.to_string_lossy().to_string(),
            base_revision: workspace_identity(repo)?,
            diagnostics: Vec::new(),
        };
        preview.content_sha256 = starter_lane_preview_hash(&preview)?;
        return Ok(preview);
    }

    let branch = request
        .branch
        .clone()
        .unwrap_or_else(|| format!("codex/{}", request.lane_id));
    validate_git_branch(repo, &branch)?;
    let configured_worktree = request
        .worktree_path
        .clone()
        .unwrap_or_else(|| format!(".worktrees/{}", request.lane_id));
    let mut lane = starter_lane_record(request, Some(branch.clone()), Some(configured_worktree));
    let worktree_path = resolve_lane_target(repo, &lane, true)?;
    lane.worktree = Some(worktree_path.to_string_lossy().to_string());
    let base_revision = current_base_revision(repo)?;
    let mut diagnostics = Vec::new();
    if worktree_path.exists() {
        diagnostics.push("starter_lane.worktree_exists".to_string());
    }
    if git_branch_exists(repo, &branch)? {
        diagnostics.push("starter_lane.branch_exists".to_string());
    }
    let mut preview = StarterLanePreview {
        preview_id: fresh_id("starter-lane-preview"),
        content_sha256: String::new(),
        owner: owner.clone(),
        lane,
        branch,
        worktree_path: worktree_path.to_string_lossy().to_string(),
        base_revision,
        diagnostics,
    };
    preview.content_sha256 = starter_lane_preview_hash(&preview)?;
    Ok(preview)
}

fn starter_lane_record(
    request: &StarterLaneRequest,
    branch: Option<String>,
    worktree_path: Option<String>,
) -> AgentLaneRecord {
    let (role, gate_strength) = match request.preset {
        StarterLanePreset::Coder => (AgentRole::Coder, GateStrength::Full),
        StarterLanePreset::Reviewer => (AgentRole::Reviewer, GateStrength::Full),
        StarterLanePreset::Tester => (AgentRole::Tester, GateStrength::Cooperative),
    };
    AgentLaneRecord {
        id: request.lane_id.clone(),
        task_id: None,
        role,
        route: AgentRoute::BuiltIn,
        gate_strength,
        // Presets remain permission-gated; role policy further narrows reviewer/tester tools.
        mutation_policy: MutationPolicy::ProposeOnly,
        worktree: worktree_path,
        branch,
        target: ExecutionTarget::Local,
        data_egress: DataEgressPolicy::Deny,
        status: LaneStatus::Draft,
        budget: LaneBudget::default(),
        active_session_ids: Vec::new(),
        summary: format!("{} starter lane", role.as_str()),
        evidence: Vec::new(),
        run_stats: None,
    }
}

fn validate_starter_lane_id(lane_id: &str) -> Result<(), String> {
    let valid = !lane_id.is_empty()
        && lane_id != "."
        && lane_id != ".."
        && lane_id.len() <= 96
        && lane_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'));
    if valid {
        Ok(())
    } else {
        Err(
            "starter lane id must use 1..=96 ASCII letters, digits, dot, dash, or underscore"
                .to_string(),
        )
    }
}

fn validate_git_branch(repo: &Path, branch: &str) -> Result<(), String> {
    let output = Command::new("git")
        .args(["check-ref-format", "--branch", branch])
        .current_dir(repo)
        .output()
        .map_err(|error| format!("cannot validate starter lane branch: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("invalid starter lane branch `{branch}`"))
    }
}

fn current_base_revision(repo: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD^{commit}"])
        .current_dir(repo)
        .output()
        .map_err(|error| format!("cannot resolve starter lane base revision: {error}"))?;
    if !output.status.success() {
        return Err("starter lane repository has no valid HEAD commit".to_string());
    }
    String::from_utf8(output.stdout)
        .map(|revision| revision.trim().to_string())
        .map_err(|_| "starter lane base revision is not UTF-8".to_string())
}

fn workspace_identity(repo: &Path) -> Result<String, String> {
    let canonical = repo
        .canonicalize()
        .map_err(|error| format!("invalid workspace root `{}`: {error}", repo.display()))?;
    if !canonical.is_dir() {
        return Err(format!(
            "workspace root `{}` is not a directory",
            canonical.display()
        ));
    }
    Ok(format!(
        "workspace:{:x}",
        Sha256::digest(canonical.to_string_lossy().as_bytes())
    ))
}

fn starter_lane_base_revision(repo: &Path, lane: &AgentLaneRecord) -> Result<String, String> {
    if lane.worktree.is_some() {
        current_base_revision(repo)
    } else {
        // Direct-workspace lanes have no immutable Git commit. Bind approval
        // to the canonical workspace identity instead of inventing a revision.
        workspace_identity(repo)
    }
}

fn git_branch_exists(repo: &Path, branch: &str) -> Result<bool, String> {
    let reference = format!("refs/heads/{branch}");
    let status = Command::new("git")
        .args(["show-ref", "--verify", "--quiet", &reference])
        .current_dir(repo)
        .status()
        .map_err(|error| format!("cannot inspect starter lane branch: {error}"))?;
    match status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err("cannot inspect starter lane branch".to_string()),
    }
}

fn starter_lane_preview_hash(preview: &StarterLanePreview) -> Result<String, String> {
    let bytes = serde_json::to_vec(&(
        &preview.owner,
        &preview.lane,
        &preview.branch,
        &preview.worktree_path,
        &preview.base_revision,
        &preview.diagnostics,
    ))
    .map_err(|error| format!("cannot hash starter lane preview: {error}"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn validate_starter_lane_effect(
    repo: &Path,
    lane: &AgentLaneRecord,
    preview: &StarterLanePreview,
) -> Result<(), String> {
    let mut reviewed_lane = lane.clone();
    reviewed_lane.active_session_ids.clear();
    if reviewed_lane != preview.lane
        || starter_lane_preview_hash(preview)? != preview.content_sha256
    {
        return Err("starter lane preview changed before effect".to_string());
    }
    if starter_lane_base_revision(repo, lane)? != preview.base_revision {
        return Err("starter lane base revision changed while approval was pending".to_string());
    }
    let resolved = resolve_lane_target(repo, lane, true)?;
    if resolved.to_string_lossy() != preview.worktree_path {
        return Err("starter lane workspace changed before effect".to_string());
    }
    if lane.worktree.is_some() && resolved.exists() {
        return Err("starter lane worktree appeared while approval was pending".to_string());
    }
    if let Some(branch) = lane.branch.as_deref()
        && git_branch_exists(repo, branch)?
    {
        return Err("starter lane branch appeared while approval was pending".to_string());
    }
    Ok(())
}
