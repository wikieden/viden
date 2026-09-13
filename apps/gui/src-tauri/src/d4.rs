use std::time::Duration;

use serde::{Deserialize, Serialize};
use viden_core::{
    AgentLaneRecord, AgentRole, ApprovalDecision, ApprovalRequestView, ApprovalResponse,
    ApprovalRisk, FRONTEND_SCHEMA_V1, RuntimeCommand, RuntimeCommandEnvelope, RuntimeEventEnvelope,
    RuntimeEventKind, RuntimeOwner, RuntimeWireEvent, StarterLanePreset, StarterLanePreview,
    StarterLanePreviewInvalidationReason, StarterLaneReceipt, StarterLaneRequest, WorkMode,
};

use crate::GuiCoreAdapter;

pub const D4_STARTER_LANE_CAPABILITY: &str = "runtime.starter_lane_preview";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum D4Preset {
    Coder,
    Reviewer,
    Tester,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D4LaneRequest {
    pub lane_id: String,
    pub preset: D4Preset,
    pub branch: Option<String>,
    pub worktree_path: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum D4ApprovalIntent {
    AllowOnce,
    Deny,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum D4Intent {
    Preview {
        request: D4LaneRequest,
    },
    Create {
        request: D4LaneRequest,
    },
    RespondToApproval {
        request_id: String,
        decision: D4ApprovalIntent,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D4AvailabilityProjection {
    pub available: bool,
    pub capability: &'static str,
    pub message: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D4OwnerProjection {
    pub workspace_id: String,
    pub project_id: String,
    pub lane_id: Option<String>,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub turn_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D4BudgetProjection {
    pub token_limit: Option<u64>,
    pub cost_limit_micro_usd: Option<u64>,
    pub wall_time_limit_secs: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D4ResolvedLaneProjection {
    pub id: String,
    pub role: String,
    pub route: String,
    pub gate_strength: String,
    pub mutation_policy: String,
    pub worktree: Option<String>,
    pub branch: Option<String>,
    pub target: String,
    pub data_egress: String,
    pub status: String,
    pub budget: D4BudgetProjection,
    pub summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D4PreviewProjection {
    pub preview_id: String,
    pub content_sha256: String,
    pub owner: D4OwnerProjection,
    pub lane: D4ResolvedLaneProjection,
    pub branch: String,
    pub worktree_path: String,
    pub base_revision: String,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D4ApprovalProjection {
    pub id: String,
    pub title: String,
    pub risk: String,
    pub target: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D4OutcomeProjection {
    pub state: String,
    pub reason: Option<String>,
    pub requires_repreview: bool,
}

impl D4OutcomeProjection {
    pub(crate) fn idle() -> Self {
        Self {
            state: "idle".into(),
            reason: None,
            requires_repreview: false,
        }
    }

    fn waiting(state: &str) -> Self {
        Self {
            state: state.into(),
            reason: None,
            requires_repreview: false,
        }
    }

    fn terminal(state: &str, reason: impl Into<String>) -> Self {
        Self {
            state: state.into(),
            reason: Some(reason.into()),
            requires_repreview: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D4LaneCreateProjection {
    pub availability: D4AvailabilityProjection,
    pub work_mode: String,
    pub can_create: bool,
    pub preview: Option<D4PreviewProjection>,
    pub receipt: Option<D4PreviewProjection>,
    pub pending_approval: Option<D4ApprovalProjection>,
    pub outcome: D4OutcomeProjection,
    pub navigation_lane_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D4IntentResult {
    pub projection: D4LaneCreateProjection,
    pub pending_command_id: Option<String>,
    pub pending_intent: Option<String>,
}

pub(crate) struct ReviewedD4 {
    request: D4LaneRequest,
    preview: StarterLanePreview,
}

pub(crate) enum PendingD4 {
    Preview {
        command_id: String,
        request: D4LaneRequest,
        command: RuntimeCommand,
        owner: RuntimeOwner,
        accepted: bool,
    },
    Create {
        command_id: String,
        request: D4LaneRequest,
        command: RuntimeCommand,
        preview: Box<StarterLanePreview>,
        accepted: bool,
        approval: Option<Box<ApprovalRequestView>>,
    },
}

impl PendingD4 {
    fn command_id(&self) -> &str {
        match self {
            Self::Preview { command_id, .. } | Self::Create { command_id, .. } => command_id,
        }
    }

    fn intent_name(&self) -> &'static str {
        match self {
            Self::Preview { .. } => "preview_starter_lane",
            Self::Create { .. } => "create_starter_lane",
        }
    }

    fn approval(&self) -> Option<&ApprovalRequestView> {
        match self {
            Self::Create { approval, .. } => approval.as_deref(),
            Self::Preview { .. } => None,
        }
    }
}

impl GuiCoreAdapter {
    pub fn d4_lane_create(&self) -> Option<D4LaneCreateProjection> {
        let view = self.projection.view()?;
        let available = self.supports(D4_STARTER_LANE_CAPABILITY);
        let preview = self.reviewed_d4.as_ref().map(|reviewed| &reviewed.preview);
        let pending_approval = self.pending_d4.as_ref().and_then(PendingD4::approval);
        Some(D4LaneCreateProjection {
            availability: D4AvailabilityProjection {
                available,
                capability: D4_STARTER_LANE_CAPABILITY,
                message: if available {
                    "Reviewed starter Lane creation is available."
                } else {
                    "Core has not advertised reviewed starter Lane creation."
                },
            },
            work_mode: work_mode(view.snapshot.work_mode).into(),
            can_create: available
                && view.snapshot.work_mode == WorkMode::Build
                && preview.is_some()
                && self.pending_d4.is_none(),
            preview: preview.map(preview_projection),
            receipt: self.d4_receipt.as_ref().map(receipt_projection),
            pending_approval: pending_approval.map(approval_projection),
            outcome: self.d4_outcome.clone(),
            navigation_lane_id: self
                .d4_receipt
                .as_ref()
                .map(|receipt| receipt.lane.id.clone()),
        })
    }

    pub fn send_d4_intent_and_wait(
        &mut self,
        command_id: &str,
        intent: D4Intent,
        event_timeout: Duration,
    ) -> Result<D4IntentResult, String> {
        if !self.supports(D4_STARTER_LANE_CAPABILITY) {
            return Err(format!(
                "missing Core capability `{D4_STARTER_LANE_CAPABILITY}`"
            ));
        }
        match intent {
            D4Intent::Preview { request } => self.start_d4_preview(command_id, request)?,
            D4Intent::Create { request } => self.start_d4_create(command_id, request)?,
            D4Intent::RespondToApproval {
                request_id,
                decision,
            } => self.respond_to_d4_approval(command_id, request_id, decision)?,
        }
        self.poll_d4(event_timeout)
    }

    pub fn poll_d4(&mut self, event_timeout: Duration) -> Result<D4IntentResult, String> {
        let mut received = false;
        for _ in 0..8 {
            let event = match self
                .receive_event(event_timeout)
                .map_err(|error| error.to_string())?
            {
                Some(event) => event,
                None => break,
            };
            received = true;
            if self.observe_d4(&event) {
                break;
            }
        }
        if received {
            self.refresh_projection()
                .map_err(|error| error.to_string())?;
        }
        self.d4_result()
    }

    fn start_d4_preview(&mut self, command_id: &str, request: D4LaneRequest) -> Result<(), String> {
        if let Some(pending) = &self.pending_d4 {
            return Err(format!(
                "D4 command `{}` is still pending",
                pending.command_id()
            ));
        }
        validate_request(&request)?;
        let core_request = core_request(&request);
        let command = RuntimeCommand::PreviewStarterLane {
            request: core_request,
        };
        // The preview command has no owner fact yet. Scope it to the requested
        // Lane; the exact owner returned by Core becomes authoritative for all
        // later create and approval commands.
        let owner = RuntimeOwner {
            lane_id: Some(request.lane_id.clone()),
            ..Default::default()
        };
        self.send_d4_command(command_id, owner.clone(), command.clone())?;
        self.reviewed_d4 = None;
        self.d4_receipt = None;
        self.d4_outcome = D4OutcomeProjection::waiting("preview_pending");
        self.pending_d4 = Some(PendingD4::Preview {
            command_id: command_id.into(),
            request,
            command,
            owner,
            accepted: false,
        });
        Ok(())
    }

    fn start_d4_create(&mut self, command_id: &str, request: D4LaneRequest) -> Result<(), String> {
        if let Some(pending) = &self.pending_d4 {
            return Err(format!(
                "D4 command `{}` is still pending",
                pending.command_id()
            ));
        }
        let mode = self
            .projection
            .view()
            .map(|view| view.snapshot.work_mode)
            .ok_or_else(|| "Core has not published a work mode".to_string())?;
        if mode != WorkMode::Build {
            return Err(format!(
                "{} mode permits preview but disables starter Lane creation",
                work_mode_label(mode)
            ));
        }
        let reviewed = self
            .reviewed_d4
            .as_ref()
            .ok_or_else(|| "starter Lane must be re-previewed before create".to_string())?;
        if reviewed.request != request {
            self.d4_outcome = D4OutcomeProjection::terminal("request_changed", "request_changed");
            return Err("starter Lane request changed; re-preview is required".into());
        }
        let command = RuntimeCommand::CreateStarterLane {
            request: core_request(&request),
            preview_id: reviewed.preview.preview_id.clone(),
            content_sha256: reviewed.preview.content_sha256.clone(),
        };
        let preview = reviewed.preview.clone();
        self.send_d4_command(command_id, preview.owner.clone(), command.clone())?;
        self.d4_outcome = D4OutcomeProjection::waiting("create_pending");
        self.pending_d4 = Some(PendingD4::Create {
            command_id: command_id.into(),
            request,
            command,
            preview: Box::new(preview),
            accepted: false,
            approval: None,
        });
        Ok(())
    }

    fn respond_to_d4_approval(
        &mut self,
        command_id: &str,
        request_id: String,
        decision: D4ApprovalIntent,
    ) -> Result<(), String> {
        let owner = match self.pending_d4.as_ref() {
            Some(PendingD4::Create {
                preview,
                approval: Some(approval),
                ..
            }) if approval.id == request_id => preview.owner.clone(),
            _ => return Err("no exact starter Lane approval is pending".into()),
        };
        let response = match decision {
            D4ApprovalIntent::AllowOnce => ApprovalResponse::allow_once(None),
            D4ApprovalIntent::Deny => ApprovalResponse::deny(None),
        };
        self.send_d4_command(
            command_id,
            owner,
            RuntimeCommand::RespondToApproval {
                request_id,
                response,
            },
        )
    }

    fn send_d4_command(
        &mut self,
        command_id: &str,
        owner: RuntimeOwner,
        command: RuntimeCommand,
    ) -> Result<(), String> {
        self.client
            .send(RuntimeCommandEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                client_id: "viden-gui".into(),
                command_id: command_id.into(),
                owner,
                command,
            })
            .map_err(|error| error.to_string())
    }

    fn d4_result(&self) -> Result<D4IntentResult, String> {
        Ok(D4IntentResult {
            projection: self
                .d4_lane_create()
                .ok_or_else(|| "Core has not published a D4 projection".to_string())?,
            pending_command_id: self
                .pending_d4
                .as_ref()
                .map(|pending| pending.command_id().to_string()),
            pending_intent: self
                .pending_d4
                .as_ref()
                .map(|pending| pending.intent_name().to_string()),
        })
    }

    /// Returns true only when a complete receipt authorizes navigation.
    pub(crate) fn observe_d4(&mut self, envelope: &RuntimeEventEnvelope) -> bool {
        let RuntimeWireEvent::Known(event) = &envelope.event else {
            return false;
        };
        let Some(mut pending) = self.pending_d4.take() else {
            return false;
        };
        let mut keep_pending = true;

        match &mut pending {
            PendingD4::Preview {
                command_id,
                request,
                command,
                owner,
                accepted,
            } => match &event.kind {
                RuntimeEventKind::CommandAccepted {
                    command_id: candidate,
                    command: candidate_command,
                }
                | RuntimeEventKind::LaneCommandAccepted {
                    command_id: candidate,
                    command: candidate_command,
                } if candidate == command_id
                    && candidate_command == command
                    && envelope.owner.lane_id == owner.lane_id =>
                {
                    *accepted = true;
                }
                RuntimeEventKind::CommandRejected {
                    command_id: candidate,
                    reason,
                } if candidate == command_id && envelope.owner.lane_id == owner.lane_id => {
                    keep_pending = false;
                    self.d4_outcome = D4OutcomeProjection::terminal("rejected", reason);
                }
                RuntimeEventKind::StarterLanePreviewed { preview }
                    if *accepted && valid_preview(envelope, request, preview) =>
                {
                    self.reviewed_d4 = Some(ReviewedD4 {
                        request: request.clone(),
                        preview: preview.clone(),
                    });
                    self.d4_outcome = D4OutcomeProjection::waiting("reviewed");
                    keep_pending = false;
                }
                _ => {}
            },
            PendingD4::Create {
                command_id,
                request,
                command,
                preview,
                accepted,
                approval,
            } => match &event.kind {
                RuntimeEventKind::CommandAccepted {
                    command_id: candidate,
                    command: candidate_command,
                }
                | RuntimeEventKind::LaneCommandAccepted {
                    command_id: candidate,
                    command: candidate_command,
                } if candidate == command_id
                    && candidate_command == command
                    && envelope.owner == preview.owner =>
                {
                    *accepted = true;
                }
                RuntimeEventKind::CommandRejected {
                    command_id: candidate,
                    reason,
                } if candidate == command_id && envelope.owner == preview.owner => {
                    keep_pending = false;
                    self.reviewed_d4 = None;
                    self.d4_outcome = D4OutcomeProjection::terminal("rejected", reason);
                }
                RuntimeEventKind::ApprovalRequested {
                    approval: candidate,
                } if *accepted && valid_approval(envelope, preview, candidate) => {
                    *approval = Some(Box::new(candidate.clone()));
                    self.d4_outcome = D4OutcomeProjection::waiting("waiting_for_approval");
                }
                RuntimeEventKind::ApprovalResolved {
                    request_id,
                    decision,
                    owner,
                    ..
                } if approval
                    .as_ref()
                    .is_some_and(|pending| pending.id == *request_id)
                    && envelope.owner == preview.owner
                    && *owner == preview.owner =>
                {
                    match decision {
                        ApprovalDecision::Allow { .. } => {
                            self.d4_outcome = D4OutcomeProjection::waiting("waiting_for_receipt")
                        }
                        // Core re-decides the revision and publishes the
                        // outcome; the wizard stays on the decision step
                        // rather than claiming a receipt is coming.
                        ApprovalDecision::Edit { .. } => {
                            self.d4_outcome = D4OutcomeProjection::waiting("waiting_for_decision")
                        }
                        ApprovalDecision::Deny => {
                            keep_pending = false;
                            self.reviewed_d4 = None;
                            self.d4_outcome =
                                D4OutcomeProjection::terminal("denied", "permission_denied");
                        }
                    }
                }
                RuntimeEventKind::StarterLanePreviewInvalidated {
                    owner,
                    preview_id,
                    reason,
                } if envelope.owner == preview.owner
                    && *owner == preview.owner
                    && *preview_id == preview.preview_id =>
                {
                    keep_pending = false;
                    self.reviewed_d4 = None;
                    self.d4_outcome =
                        D4OutcomeProjection::terminal("invalidated", invalidation_reason(*reason));
                }
                RuntimeEventKind::StarterLaneCreated { receipt }
                    if *accepted && valid_receipt(envelope, request, preview, receipt) =>
                {
                    keep_pending = false;
                    self.d4_receipt = Some(receipt.clone());
                    self.reviewed_d4 = None;
                    self.d4_outcome = D4OutcomeProjection::waiting("created");
                }
                // LaneUpdated is durable Core state, but it is not the reviewed
                // creation receipt and therefore never authorizes navigation.
                RuntimeEventKind::LaneUpdated { .. } => {}
                _ => {}
            },
        }

        if keep_pending {
            self.pending_d4 = Some(pending);
        }
        !keep_pending
    }
}

fn core_request(request: &D4LaneRequest) -> StarterLaneRequest {
    StarterLaneRequest {
        // `runtime.starter_lane_agent` (GUI-CORE-030); the pick travels in G9.
        agent: None,
        lane_id: request.lane_id.clone(),
        preset: match request.preset {
            D4Preset::Coder => StarterLanePreset::Coder,
            D4Preset::Reviewer => StarterLanePreset::Reviewer,
            D4Preset::Tester => StarterLanePreset::Tester,
        },
        branch: request.branch.clone(),
        worktree_path: request.worktree_path.clone(),
    }
}

fn validate_request(request: &D4LaneRequest) -> Result<(), String> {
    if request.lane_id.trim().is_empty() {
        return Err("starter Lane id is required".into());
    }
    Ok(())
}

fn expected_role(preset: D4Preset) -> AgentRole {
    match preset {
        D4Preset::Coder => AgentRole::Coder,
        D4Preset::Reviewer => AgentRole::Reviewer,
        D4Preset::Tester => AgentRole::Tester,
    }
}

fn valid_preview(
    envelope: &RuntimeEventEnvelope,
    request: &D4LaneRequest,
    preview: &StarterLanePreview,
) -> bool {
    envelope.owner == preview.owner
        && preview.owner.lane_id.as_deref() == Some(request.lane_id.as_str())
        && preview.lane.id == request.lane_id
        && preview.lane.role == expected_role(request.preset)
        && request
            .branch
            .as_ref()
            .is_none_or(|branch| branch == &preview.branch)
        && request
            .worktree_path
            .as_ref()
            .is_none_or(|worktree| worktree == &preview.worktree_path)
}

fn valid_approval(
    envelope: &RuntimeEventEnvelope,
    preview: &StarterLanePreview,
    approval: &ApprovalRequestView,
) -> bool {
    envelope.owner == preview.owner
        && approval.owner == preview.owner
        && approval.tool_name == "lane_create"
        && (approval.target.display == preview.lane.id
            || approval.input_preview.contains(&preview.lane.id))
}

fn valid_receipt(
    envelope: &RuntimeEventEnvelope,
    request: &D4LaneRequest,
    preview: &StarterLanePreview,
    receipt: &StarterLaneReceipt,
) -> bool {
    envelope.owner == preview.owner
        && receipt.owner == preview.owner
        && receipt.preview_id == preview.preview_id
        && receipt.content_sha256 == preview.content_sha256
        && receipt.lane.id == request.lane_id
        && receipt.lane.task_id == preview.lane.task_id
        && receipt.lane.role == preview.lane.role
        && receipt.lane.route == preview.lane.route
        && receipt.lane.gate_strength == preview.lane.gate_strength
        && receipt.lane.mutation_policy == preview.lane.mutation_policy
        && receipt.lane.worktree == preview.lane.worktree
        && receipt.lane.branch == preview.lane.branch
        && receipt.lane.target == preview.lane.target
        && receipt.lane.data_egress == preview.lane.data_egress
        && receipt.lane.budget == preview.lane.budget
        && receipt.lane.active_session_ids == preview.lane.active_session_ids
        && receipt.lane.summary == preview.lane.summary
        && receipt.lane.evidence == preview.lane.evidence
        && receipt.branch == preview.branch
        && receipt.worktree_path == preview.worktree_path
        && receipt.base_revision == preview.base_revision
}

fn work_mode(mode: WorkMode) -> &'static str {
    match mode {
        WorkMode::Plan => "plan",
        WorkMode::Build => "build",
        WorkMode::Review => "review",
        WorkMode::Explore => "explore",
    }
}

fn work_mode_label(mode: WorkMode) -> &'static str {
    match mode {
        WorkMode::Plan => "Plan",
        WorkMode::Build => "Build",
        WorkMode::Review => "Review",
        WorkMode::Explore => "Explore",
    }
}

fn preview_projection(preview: &StarterLanePreview) -> D4PreviewProjection {
    D4PreviewProjection {
        preview_id: preview.preview_id.clone(),
        content_sha256: preview.content_sha256.clone(),
        owner: owner_projection(&preview.owner),
        lane: lane_projection(&preview.lane),
        branch: preview.branch.clone(),
        worktree_path: preview.worktree_path.clone(),
        base_revision: preview.base_revision.clone(),
        diagnostics: preview.diagnostics.clone(),
    }
}

fn receipt_projection(receipt: &StarterLaneReceipt) -> D4PreviewProjection {
    D4PreviewProjection {
        preview_id: receipt.preview_id.clone(),
        content_sha256: receipt.content_sha256.clone(),
        owner: owner_projection(&receipt.owner),
        lane: lane_projection(&receipt.lane),
        branch: receipt.branch.clone(),
        worktree_path: receipt.worktree_path.clone(),
        base_revision: receipt.base_revision.clone(),
        diagnostics: Vec::new(),
    }
}

fn owner_projection(owner: &RuntimeOwner) -> D4OwnerProjection {
    D4OwnerProjection {
        workspace_id: owner.workspace_id.clone(),
        project_id: owner.project_id.clone(),
        lane_id: owner.lane_id.clone(),
        session_id: owner.session_id.clone(),
        task_id: owner.task_id.clone(),
        turn_id: owner.turn_id.clone(),
    }
}

fn lane_projection(lane: &AgentLaneRecord) -> D4ResolvedLaneProjection {
    D4ResolvedLaneProjection {
        id: lane.id.clone(),
        role: lane.role.as_str().into(),
        route: enum_string(&lane.route),
        gate_strength: enum_string(&lane.gate_strength),
        mutation_policy: enum_string(&lane.mutation_policy),
        worktree: lane.worktree.clone(),
        branch: lane.branch.clone(),
        target: enum_string(&lane.target),
        data_egress: enum_string(&lane.data_egress),
        status: enum_string(&lane.status),
        budget: D4BudgetProjection {
            token_limit: lane.budget.token_limit,
            cost_limit_micro_usd: lane.budget.cost_limit_micro_usd,
            wall_time_limit_secs: lane.budget.wall_time_limit_secs,
        },
        summary: lane.summary.clone(),
    }
}

fn enum_string(value: &impl std::fmt::Debug) -> String {
    let debug = format!("{value:?}");
    let mut output = String::with_capacity(debug.len() + 4);
    for (index, character) in debug.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            output.push('_');
        }
        output.extend(character.to_lowercase());
    }
    output
}

fn approval_projection(approval: &ApprovalRequestView) -> D4ApprovalProjection {
    D4ApprovalProjection {
        id: approval.id.clone(),
        title: approval.title.clone(),
        risk: match approval.risk {
            ApprovalRisk::Low => "low",
            ApprovalRisk::Medium => "medium",
            ApprovalRisk::High => "high",
            ApprovalRisk::Critical => "critical",
        }
        .into(),
        target: approval.target.display.clone(),
    }
}

fn invalidation_reason(reason: StarterLanePreviewInvalidationReason) -> &'static str {
    match reason {
        StarterLanePreviewInvalidationReason::PlanModeDenied => "plan_mode_denied",
        StarterLanePreviewInvalidationReason::RequestChanged => "request_changed",
        StarterLanePreviewInvalidationReason::HashMismatch => "hash_mismatch",
        StarterLanePreviewInvalidationReason::BaseRevisionChanged => "base_revision_changed",
        StarterLanePreviewInvalidationReason::WorktreeUnavailable => "worktree_unavailable",
        StarterLanePreviewInvalidationReason::BranchUnavailable => "branch_unavailable",
        StarterLanePreviewInvalidationReason::LaneAlreadyRegistered => "lane_already_registered",
        StarterLanePreviewInvalidationReason::PermissionDenied => "permission_denied",
        StarterLanePreviewInvalidationReason::EffectFailed => "effect_failed",
    }
}
