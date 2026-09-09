use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use viden_core::{
    AgentSessionInput, AgentSessionRequest, AgentSessionStatus, ApprovalDecision,
    ApprovalRequestView, ApprovalResponse, ApprovalScope, AuditActor, AuditCursor, AuditObjectRef,
    AuditOutcome, AuditQuery, AuditRecord, BoundCoreClient, CoreClient, CoreClientError,
    CoreHandshake, FRONTEND_SCHEMA_V1, LocalCoreHost, LocaleId, MergeGateStatus, PermissionLevel,
    ReplayBatch, ReplayRequest, ReviewRequestStatus, RuntimeCommand, RuntimeCommandEnvelope,
    RuntimeEventEnvelope, RuntimeEventKind, RuntimeOwner, RuntimeSnapshotEnvelope,
    RuntimeWireEvent, StarterLanePreset, StarterLaneRequest, TranscriptPage, TranscriptPageRequest,
    UiColorMode, UiDensity, UiMotion, UiPreferencePatch, UiPreferences, UiSkin, WorkMode,
    WorkspaceFileEntry, WorkspaceFileKind, WorkspaceFilesQuery, WorkspaceOpenRequest,
};
use viden_core::{RecentProjectSummary, RecentSessionSummary, RecentWorkQuery};
use viden_core::{SourceTarget, WorkspaceDiffQuery, WorkspaceDiffScope};

use crate::d1::{
    ComposerControlIntent, D1_OWNER_CAPABILITY, D1CockpitProjection, D1Intent, D1IntentResult,
    D1OutcomeProjection,
};
use crate::d4::{D4OutcomeProjection, PendingD4, ReviewedD4};
use crate::d10::D10_EVENT_TICKER_LIMIT;
use crate::d14::{
    AUDIT_CAPABILITY, D14_AUDIT_PAGE_LIMIT, D14AuditArgProjection, D14AuditObjectProjection,
    D14AuditProjection, D14AuditRowProjection, D14AuditScopeInput, D14AuditScopeProjection,
};
use crate::diff_review::{
    STRUCTURED_DIFF_CAPABILITY, WorkspaceDiffEntryProjection, WorkspaceDiffProjection,
};
use crate::projection::{
    PreferenceDiagnosticProjection, ResolvedPreferencesProjection, exact_terminal_agent_session,
    preference_diagnostic_projection, target_lane_id, workspace_diff_entry_projection,
    workspace_diff_source_projection,
};
use crate::recent_work::{
    RECENT_WORK_CAPABILITY, RecentProjectProjection, RecentSessionProjection, RecentWorkResult,
};
use crate::ui_preferences::{
    PreferenceIntent, PreferenceIntentResult, PreferencePatchInput,
    UI_PREFERENCE_PERSISTENCE_CAPABILITY,
};
use crate::workspace_files::{
    WORKSPACE_FILES_CAPABILITY, WORKSPACE_FILES_PAGE_LIMIT, WorkspaceFileRowProjection,
    WorkspaceFilesProjection,
};
use crate::{
    D6ConnectionState, D6RecoveryProjection, D6State, D11IntakeProjection, PermissionChoice,
    PermissionDockProjection, PermissionIntent, PermissionIntentResult,
    PermissionOutcomeProjection, RuntimeProjection,
};

/// The sole command and state boundary between the desktop shell and Core.
pub struct GuiCoreAdapter {
    pub(crate) client: Box<dyn CoreClient>,
    pub(crate) projection: RuntimeProjection,
    capabilities: BTreeSet<String>,
    pending_d11: Option<PendingD11>,
    pub(crate) pending_d4: Option<PendingD4>,
    pub(crate) reviewed_d4: Option<ReviewedD4>,
    pub(crate) d4_receipt: Option<viden_core::StarterLaneReceipt>,
    pub(crate) d4_outcome: D4OutcomeProjection,
    pending_d1: Option<PendingD1>,
    d1_outcome: D1OutcomeProjection,
    /// One merge-gate decision at a time. Gate decisions are project-scoped
    /// rather than Lane-scoped, so they keep their own slot instead of
    /// blocking (or being blocked by) the D1 composer slot.
    pending_d12: Option<PendingD12>,
    d12_outcome: D1OutcomeProjection,
    pending_permission: Option<PendingPermission>,
    permission_outcome: PermissionOutcomeProjection,
    /// One preference command at a time. Preferences are workspace-scoped
    /// rather than Lane-scoped, so they keep their own slot instead of
    /// blocking (or being blocked by) the D1 composer slot.
    pending_preference: Option<PendingPreference>,
    preference_outcome: D1OutcomeProjection,
    /// The last confirmed `UiPreferencesUpdated` receipt. Only a confirming
    /// event fills this, so the client cannot render a persistence claim the
    /// ordered Core stream never made.
    preference_receipt: Option<PreferenceReceipt>,
    /// One recent-work read at a time. `RecentWorkLoaded` carries no command
    /// id, so a second concurrent read could not be told apart from the first
    /// one's answer; the slot makes that impossible instead of guessing.
    pending_recent_work: Option<PendingRecentWork>,
    recent_work_outcome: D1OutcomeProjection,
    /// The last confirmed `RecentWorkLoaded` fact. Only a confirming event
    /// fills this, so the client cannot render an inventory Core never sent.
    recent_work_receipt: Option<RecentWorkReceipt>,
    connection: D6ConnectionState,
    connection_detail: Option<String>,
    /// D2 queue selection. Presentation state only: it never gates a Core
    /// command and is dropped when Core stops publishing the decision.
    d2_selected: Option<String>,
    /// One review verdict at a time. A review settles exactly once, so a second
    /// verdict sent while the first is unanswered could only race Core's own
    /// "already decided" rejection; the slot refuses it locally instead.
    pending_d2_review: Option<PendingD2Review>,
    /// One audit read at a time. Core now names the read each page answers
    /// (GUI-CORE-024), so two reads *could* be told apart, but a single slot
    /// keeps the receipt's replace/append decision unambiguous and keeps the
    /// screen from paging two lists into one. See [`PendingAuditPage`].
    pending_audit: Option<PendingAuditPage>,
    /// One inventory read at a time. `WorkspaceFilesLoaded` names the exact
    /// read it answers, so two reads *could* be told apart, but the palette
    /// shows one list and a second read would only race the first for it.
    pending_workspace_files: Option<PendingWorkspaceFiles>,
    workspace_files_outcome: D1OutcomeProjection,
    /// What Core published for the inventory reads confirmed so far. Only a
    /// confirming page fills this, so the palette can never render a path the
    /// ordered Core stream did not hand it.
    workspace_files_receipt: WorkspaceFilesReceipt,
    audit_outcome: D1OutcomeProjection,
    /// What Core published for the audit reads confirmed so far. Only a
    /// confirming page fills this, so D14 can never render a record the
    /// ordered Core stream did not hand it.
    audit_receipt: AuditReceipt,
    /// One structured diff read at a time, for the same reason the inventory
    /// keeps one: DiffReview shows one page and a second read would only race
    /// the first for it. `WorkspaceDiffLoaded` names the exact read it answers.
    pending_workspace_diff: Option<PendingWorkspaceDiff>,
    workspace_diff_outcome: D1OutcomeProjection,
    /// What Core published for the diff reads confirmed so far.
    workspace_diff_receipt: WorkspaceDiffReceipt,
    /// How many Core facts that invalidate a diff have been observed.
    ///
    /// Incremented in [`Self::receive_event`], the single funnel every drain
    /// path goes through, so a page cannot be invalidated by an event some
    /// other screen's poll happened to consume. Compared against the revision
    /// a loaded page was read at; it is a "re-read" signal, never a claim
    /// about what changed.
    workspace_revision: u64,
}

struct HostedCoreClient {
    bound: BoundCoreClient,
    handshake: Option<CoreHandshake>,
    confirmed: Option<RuntimeSnapshotEnvelope>,
    confirmed_dirty: bool,
}

impl CoreClient for HostedCoreClient {
    fn discover(&mut self) -> Result<CoreHandshake, CoreClientError> {
        let handshake = self.bound.client().discover()?;
        self.handshake = Some(handshake.clone());
        Ok(handshake)
    }

    fn send(&mut self, command: RuntimeCommandEnvelope) -> Result<(), CoreClientError> {
        self.bound.client().send(command)
    }

    fn recv(&mut self, timeout: Duration) -> Result<Option<RuntimeEventEnvelope>, CoreClientError> {
        let event = self.bound.client().recv(timeout)?;
        if event.is_some() {
            self.cache_confirmed_state();
            self.confirmed_dirty = true;
        }
        Ok(event)
    }

    fn snapshot(&mut self) -> Result<RuntimeSnapshotEnvelope, CoreClientError> {
        if self.confirmed_dirty {
            self.confirmed_dirty = false;
            return self
                .confirmed
                .clone()
                .ok_or(CoreClientError::MissingSnapshot);
        }
        let snapshot = self.bound.client().snapshot()?;
        self.confirmed = Some(snapshot.clone());
        Ok(snapshot)
    }

    fn replay(&mut self, request: ReplayRequest) -> Result<ReplayBatch, CoreClientError> {
        self.bound.client().replay(request)
    }

    fn transcript_page(
        &mut self,
        request: TranscriptPageRequest,
    ) -> Result<TranscriptPage, CoreClientError> {
        self.bound.client().transcript_page(request)
    }
}

impl HostedCoreClient {
    fn new(bound: BoundCoreClient) -> Self {
        Self {
            bound,
            handshake: None,
            confirmed: None,
            confirmed_dirty: false,
        }
    }

    fn cache_confirmed_state(&mut self) {
        let Some(handshake) = &self.handshake else {
            return;
        };
        let client = self.bound.client();
        let (Some(view), Some(cursor)) = (client.confirmed_view(), client.confirmed_cursor())
        else {
            return;
        };
        self.confirmed = Some(RuntimeSnapshotEnvelope {
            schema_version: handshake.active_schema_version,
            capabilities: handshake.capabilities.clone(),
            cursor: cursor.clone(),
            snapshot: view.snapshot.clone(),
            view: view.clone(),
        });
    }
}

pub fn open_local_workspace(root: impl AsRef<Path>) -> Result<GuiCoreAdapter, String> {
    let host = LocalCoreHost::new();
    open_workspace_with_host(&host, root)
}

fn open_workspace_with_host(
    host: &LocalCoreHost,
    root: impl AsRef<Path>,
) -> Result<GuiCoreAdapter, String> {
    let bound = host
        .open_workspace(WorkspaceOpenRequest::new(root.as_ref()))
        .map_err(|error| error.to_string())?;
    let mut adapter = GuiCoreAdapter::new(Box::new(HostedCoreClient::new(bound)));
    adapter.connect().map_err(|error| error.to_string())?;
    // D1 needs Core's workspace-mode decision before it can enable New Lane.
    // A project probe is read-only and publishes whether Core will use Git
    // isolation or the opened directory without routing into D11.
    adapter.send_d11_intent_and_wait(
        "gui-open-workspace-probe",
        D11Intent::ProbeProject,
        Duration::from_millis(250),
    )?;
    // ProjectProbed completes the D11 request before the ordered eligibility
    // event that follows it, so drain the remaining facts into the projection.
    adapter.poll_d11(Duration::from_millis(250))?;
    Ok(adapter)
}

pub(crate) fn default_local_adapter() -> Result<Option<GuiCoreAdapter>, String> {
    let Some(root) = default_workspace_root() else {
        return Ok(None);
    };
    open_local_workspace(root).map(Some)
}

fn default_workspace_root() -> Option<PathBuf> {
    // A desktop launch does not imply that its process or bundle directory is
    // the user's project. Only an explicit launch binding may bypass Welcome.
    std::env::var_os("VIDEN_GUI_WORKSPACE").map(PathBuf::from)
}

struct PendingPermission {
    command_id: String,
    request_id: String,
    owner: RuntimeOwner,
    audit_id: String,
}

enum PermissionObservation {
    Continue,
    Confirmed,
    Rejected(String),
}

impl PendingPermission {
    fn observe(&self, envelope: &RuntimeEventEnvelope) -> PermissionObservation {
        let RuntimeWireEvent::Known(event) = &envelope.event else {
            return PermissionObservation::Continue;
        };
        match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, reason }
                if command_id == &self.command_id =>
            {
                PermissionObservation::Rejected(reason.clone())
            }
            RuntimeEventKind::ApprovalResolved {
                request_id,
                owner,
                audit_id,
                ..
            } if request_id == &self.request_id
                && owner == &self.owner
                && audit_id == &self.audit_id =>
            {
                PermissionObservation::Confirmed
            }
            _ => PermissionObservation::Continue,
        }
    }
}

#[derive(Clone)]
enum PendingD1Kind {
    Submit,
    Queue,
    Cancel,
    QueryAgentAdapters,
    ProbeAgentAdapter {
        agent_id: String,
    },
    PreviewDefaultLane,
    CreateStarterLane {
        preview_id: String,
        content_sha256: String,
    },
    StartAgentSession {
        lane_id: String,
        agent_id: String,
        matching_sessions_before: usize,
    },
    SendAgentSessionInput {
        session_id: String,
    },
    RetryAgentSession {
        session_id: String,
    },
    CancelAgentSession {
        session_id: String,
    },
    SetWorkMode,
    SetPermissionLevel,
    SelectModel,
}

struct PendingD1 {
    command_id: String,
    owner: RuntimeOwner,
    kind: PendingD1Kind,
    accepted: bool,
}

enum D1Observation {
    Continue,
    Confirmed,
    Rejected(String),
}

impl PendingD1 {
    fn is_satisfied_by_agent_sessions(&self, sessions: &[viden_core::AgentSessionView]) -> bool {
        match &self.kind {
            PendingD1Kind::StartAgentSession {
                lane_id,
                agent_id,
                matching_sessions_before,
            } => {
                sessions
                    .iter()
                    .filter(|session| session.lane_id == *lane_id && session.agent_id == *agent_id)
                    .count()
                    > *matching_sessions_before
            }
            _ => false,
        }
    }

    fn observe(&mut self, envelope: &RuntimeEventEnvelope) -> D1Observation {
        let RuntimeWireEvent::Known(event) = &envelope.event else {
            return D1Observation::Continue;
        };
        match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, reason }
                if command_id == &self.command_id =>
            {
                D1Observation::Rejected(reason.clone())
            }
            RuntimeEventKind::CommandAccepted {
                command_id,
                command,
            }
            | RuntimeEventKind::LaneCommandAccepted {
                command_id,
                command,
            } if command_id == &self.command_id
                && self.acceptance_owner_matches(envelope, command)
                && self.matches_command(command) =>
            {
                if matches!(self.kind, PendingD1Kind::PreviewDefaultLane)
                    && matches!(command, RuntimeCommand::PreviewStarterLane { .. })
                {
                    // Core expands the default request into a concrete Lane owner;
                    // subsequent business facts must match that authoritative owner.
                    self.owner = envelope.owner.clone();
                }
                self.accepted = true;
                D1Observation::Continue
            }
            kind if self.accepted
                && self.business_owner_matches(envelope)
                && self.matches_business_fact(kind) =>
            {
                D1Observation::Confirmed
            }
            _ => D1Observation::Continue,
        }
    }

    fn matches_command(&self, command: &RuntimeCommand) -> bool {
        matches!(
            (&self.kind, command),
            (
                PendingD1Kind::Submit,
                RuntimeCommand::SubmitUserInput { .. }
            ) | (PendingD1Kind::Queue, RuntimeCommand::QueueFollowUp { .. })
                | (PendingD1Kind::Cancel, RuntimeCommand::CancelActiveTurn)
                | (
                    PendingD1Kind::QueryAgentAdapters,
                    RuntimeCommand::QueryAgentAdapters
                )
                | (
                    PendingD1Kind::ProbeAgentAdapter { .. },
                    RuntimeCommand::ProbeAgentAdapter { .. }
                )
                | (
                    PendingD1Kind::PreviewDefaultLane,
                    RuntimeCommand::PreviewDefaultStarterLane { .. }
                        | RuntimeCommand::PreviewStarterLane { .. }
                )
                | (
                    PendingD1Kind::CreateStarterLane { .. },
                    RuntimeCommand::CreateStarterLane { .. }
                )
                | (
                    PendingD1Kind::StartAgentSession { .. },
                    RuntimeCommand::StartAgentSession { .. }
                )
                | (
                    PendingD1Kind::SendAgentSessionInput { .. },
                    RuntimeCommand::SendAgentSessionInput { .. }
                )
                | (
                    PendingD1Kind::RetryAgentSession { .. },
                    RuntimeCommand::RetryAgentSession { .. }
                )
                | (
                    PendingD1Kind::CancelAgentSession { .. },
                    RuntimeCommand::CancelAgentSession { .. }
                )
                | (
                    PendingD1Kind::SetWorkMode,
                    RuntimeCommand::SetWorkMode { .. }
                )
                | (
                    PendingD1Kind::SetPermissionLevel,
                    RuntimeCommand::SetPermissionLevel { .. }
                )
                | (
                    PendingD1Kind::SelectModel,
                    RuntimeCommand::SelectModel { .. }
                )
        )
    }

    fn acceptance_owner_matches(
        &self,
        envelope: &RuntimeEventEnvelope,
        command: &RuntimeCommand,
    ) -> bool {
        match (&self.kind, command) {
            (PendingD1Kind::PreviewDefaultLane, RuntimeCommand::PreviewStarterLane { request }) => {
                envelope.owner.lane_id.as_deref() == Some(request.lane_id.as_str())
            }
            _ => envelope.owner == self.owner,
        }
    }

    fn business_owner_matches(&self, envelope: &RuntimeEventEnvelope) -> bool {
        match self.kind {
            PendingD1Kind::Submit
            | PendingD1Kind::Queue
            | PendingD1Kind::Cancel
            | PendingD1Kind::PreviewDefaultLane => envelope.owner == self.owner,
            _ => true,
        }
    }

    fn matches_business_fact(&self, kind: &RuntimeEventKind) -> bool {
        match (&self.kind, kind) {
            (PendingD1Kind::Queue, RuntimeEventKind::InputQueued { .. }) => true,
            (
                PendingD1Kind::Submit,
                RuntimeEventKind::AssistantDelta { .. }
                | RuntimeEventKind::ToolCallStarted { .. }
                | RuntimeEventKind::TaskUpdated { .. },
            ) => true,
            (PendingD1Kind::Cancel, RuntimeEventKind::LaneUpdated { lane }) => {
                self.owner.lane_id.as_deref() == Some(lane.id.as_str()) && !lane.is_active()
            }
            (PendingD1Kind::QueryAgentAdapters, RuntimeEventKind::AgentAdaptersLoaded { .. }) => {
                true
            }
            (
                PendingD1Kind::ProbeAgentAdapter { agent_id },
                RuntimeEventKind::AgentAdapterProbed { adapter },
            ) => adapter.agent_id == *agent_id,
            (PendingD1Kind::PreviewDefaultLane, RuntimeEventKind::StarterLanePreviewed { .. }) => {
                true
            }
            (
                PendingD1Kind::CreateStarterLane {
                    preview_id,
                    content_sha256,
                },
                RuntimeEventKind::StarterLaneCreated { receipt },
            ) => receipt.preview_id == *preview_id && receipt.content_sha256 == *content_sha256,
            (
                PendingD1Kind::StartAgentSession {
                    lane_id, agent_id, ..
                },
                RuntimeEventKind::AgentSessionStarted { session },
            ) => session.lane_id == *lane_id && session.agent_id == *agent_id,
            (
                PendingD1Kind::SendAgentSessionInput { session_id },
                RuntimeEventKind::AgentSessionInputAccepted {
                    session_id: accepted,
                    ..
                },
            ) => accepted == session_id,
            (
                PendingD1Kind::RetryAgentSession { session_id },
                RuntimeEventKind::AgentSessionStarted { session },
            ) => session.session_id == *session_id,
            (
                PendingD1Kind::CancelAgentSession { session_id },
                RuntimeEventKind::AgentSessionUpdated { session }
                | RuntimeEventKind::AgentSessionCompleted { session }
                | RuntimeEventKind::AgentSessionFailed { session },
            ) => {
                session.session_id == *session_id && session.status == AgentSessionStatus::Cancelled
            }
            // Core confirms every composer control by republishing the
            // snapshot the control mutated (runtime_contract emits
            // `SnapshotUpdated` for SetWorkMode/SetPermissionLevel/SelectModel).
            (
                PendingD1Kind::SetWorkMode
                | PendingD1Kind::SetPermissionLevel
                | PendingD1Kind::SelectModel,
                RuntimeEventKind::SnapshotUpdated { .. },
            ) => true,
            _ => false,
        }
    }
}

/// One in-flight merge-gate decision awaiting its ordered Core receipt.
struct PendingD12 {
    command_id: String,
    gate_id: String,
    /// Which decision was asked for, so a `MergeGateUpdated` that only reports
    /// some other transition cannot be read as this command's confirmation.
    status: MergeGateStatus,
    accepted: bool,
}

enum D12Observation {
    Continue,
    Confirmed,
    Rejected(String),
}

impl PendingD12 {
    /// Reconciles one ordered event against this decision.
    ///
    /// Acceptance is not the decision: only a `MergeGateUpdated` naming this
    /// gate, carrying the status this command asked for, confirms it. Core
    /// emits exactly that event from `decide_merge_gate`, so the client never
    /// has to infer the outcome from a republished snapshot.
    fn observe(&mut self, envelope: &RuntimeEventEnvelope) -> D12Observation {
        let RuntimeWireEvent::Known(event) = &envelope.event else {
            return D12Observation::Continue;
        };
        match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, reason }
                if command_id == &self.command_id =>
            {
                D12Observation::Rejected(reason.clone())
            }
            RuntimeEventKind::CommandAccepted {
                command_id,
                command,
            }
            | RuntimeEventKind::LaneCommandAccepted {
                command_id,
                command,
            } if command_id == &self.command_id && self.matches_command(command) => {
                self.accepted = true;
                D12Observation::Continue
            }
            RuntimeEventKind::MergeGateUpdated { gate }
                if self.accepted && gate.gate_id == self.gate_id && gate.status == self.status =>
            {
                D12Observation::Confirmed
            }
            _ => D12Observation::Continue,
        }
    }

    fn matches_command(&self, command: &RuntimeCommand) -> bool {
        match command {
            RuntimeCommand::AcceptMergeGate { gate_id, .. } => {
                self.status == MergeGateStatus::Accepted && gate_id == &self.gate_id
            }
            RuntimeCommand::RejectMergeGate { gate_id, .. } => {
                self.status == MergeGateStatus::NeedsChanges && gate_id == &self.gate_id
            }
            _ => false,
        }
    }
}

/// Trims and length-checks reviewer prose against Core's own rule.
///
/// Core trims before it validates, so trailing whitespace never makes an empty
/// note valid here either, and an empty note is absence rather than an empty
/// string on the wire. Over-limit input is refused instead of truncated:
/// Core truncates for its stored preview, and silently sending half a note
/// would misattribute the reviewer's words.
fn normalize_review_feedback(feedback: Option<&str>) -> Result<Option<String>, String> {
    let Some(trimmed) = feedback.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if trimmed.chars().any(char::is_control) {
        return Err("review feedback cannot contain control characters".to_string());
    }
    if trimmed.chars().count() > crate::D2_REVIEW_FEEDBACK_MAX_CHARS {
        return Err(format!(
            "review feedback is limited to {} characters",
            crate::D2_REVIEW_FEEDBACK_MAX_CHARS
        ));
    }
    Ok(Some(trimmed.to_string()))
}

/// Builds `RuntimeCommand::DecideReview` through Core's own wire encoding.
///
/// `viden-core` re-exports `RuntimeCommand` but not `ReviewVerdict`, and the
/// GUI track must not reach around the facade into `viden-types`, so the
/// verdict travels as the snake_case token Core's own `Deserialize` accepts.
/// A rename on Core's side therefore fails loudly here instead of silently
/// producing a command Core would reject.
fn decide_review_command(
    review_id: &str,
    accept: bool,
    feedback: Option<String>,
    actor: RuntimeOwner,
) -> Result<RuntimeCommand, String> {
    let mut wire = serde_json::json!({
        "type": "decide_review",
        "review_id": review_id,
        "verdict": if accept { "accepted" } else { "rejected" },
        "actor": actor,
    });
    if let Some(feedback) = feedback {
        wire["feedback"] = serde_json::Value::String(feedback);
    }
    serde_json::from_value(wire)
        .map_err(|error| format!("Core rejected this client's review verdict encoding: {error}"))
}

/// One in-flight review verdict awaiting its ordered Core receipt.
struct PendingD2Review {
    command_id: String,
    review_id: String,
    /// The status this verdict asked for, so a `ReviewRequestUpdated` that only
    /// reports some other transition cannot be read as this command's answer.
    expected_status: ReviewRequestStatus,
    accepted: bool,
}

impl PendingD2Review {
    /// Reconciles one ordered event against this verdict.
    ///
    /// Acceptance is not the decision: only a `ReviewRequestUpdated` naming
    /// this review and carrying the status this command asked for confirms it.
    /// Core emits that event first from `decide_review` and follows it with the
    /// validator-stamping `MergeGateUpdated`, which is tolerated and never
    /// mistaken for the confirmation.
    fn observe(&mut self, envelope: &RuntimeEventEnvelope) -> D12Observation {
        let RuntimeWireEvent::Known(event) = &envelope.event else {
            return D12Observation::Continue;
        };
        match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, reason }
                if command_id == &self.command_id =>
            {
                D12Observation::Rejected(reason.clone())
            }
            RuntimeEventKind::CommandAccepted {
                command_id,
                command,
            }
            | RuntimeEventKind::LaneCommandAccepted {
                command_id,
                command,
            } if command_id == &self.command_id && self.matches_command(command) => {
                self.accepted = true;
                D12Observation::Continue
            }
            RuntimeEventKind::ReviewRequestUpdated { review }
                if self.accepted
                    && review.review_id == self.review_id
                    && review.status == self.expected_status =>
            {
                D12Observation::Confirmed
            }
            _ => D12Observation::Continue,
        }
    }

    fn matches_command(&self, command: &RuntimeCommand) -> bool {
        match command {
            RuntimeCommand::DecideReview {
                review_id, verdict, ..
            } => {
                review_id == &self.review_id
                    && ReviewRequestStatus::from(*verdict) == self.expected_status
            }
            _ => false,
        }
    }
}

/// One in-flight preference command awaiting its ordered Core receipt.
struct PendingPreference {
    command_id: String,
    command: RuntimeCommand,
    accepted: bool,
}

/// What Core confirmed for the last completed preference command.
struct PreferenceReceipt {
    preferences: ResolvedPreferencesProjection,
    persisted: bool,
    diagnostics: Vec<PreferenceDiagnosticProjection>,
}

enum PreferenceObservation {
    Continue,
    Confirmed,
    Rejected(String),
}

impl PendingPreference {
    /// Reconciles one ordered event against this command.
    ///
    /// Acceptance alone is not persistence: only a `UiPreferencesUpdated`
    /// whose `persisted` table matches what this command asked for (or, for a
    /// reset, whose `persisted` table is gone) confirms the write. A
    /// republished snapshot never confirms it, and neither does another
    /// frontend's preference write.
    fn observe(&mut self, envelope: &RuntimeEventEnvelope) -> PreferenceObservation {
        let RuntimeWireEvent::Known(event) = &envelope.event else {
            return PreferenceObservation::Continue;
        };
        match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, reason }
                if command_id == &self.command_id =>
            {
                PreferenceObservation::Rejected(reason.clone())
            }
            RuntimeEventKind::CommandAccepted {
                command_id,
                command,
            } if command_id == &self.command_id
                && matches!(
                    (&self.command, command),
                    (
                        RuntimeCommand::SetUiPreferences { .. },
                        RuntimeCommand::SetUiPreferences { .. }
                    ) | (
                        RuntimeCommand::ResetUiPreferences,
                        RuntimeCommand::ResetUiPreferences
                    )
                ) =>
            {
                self.accepted = true;
                PreferenceObservation::Continue
            }
            RuntimeEventKind::UiPreferencesUpdated { persisted, .. }
                if self.accepted && update_matches(&self.command, persisted.as_ref()) =>
            {
                PreferenceObservation::Confirmed
            }
            _ => PreferenceObservation::Continue,
        }
    }
}

/// One in-flight `QueryRecentWork` awaiting its ordered Core answer.
struct PendingRecentWork {
    command_id: String,
    accepted: bool,
}

/// What Core published for the last completed recent-work read.
struct RecentWorkReceipt {
    projects: Vec<RecentProjectProjection>,
    sessions: Vec<RecentSessionProjection>,
    diagnostics: Vec<String>,
}

enum RecentWorkObservation {
    Continue,
    Confirmed,
    Rejected(String),
}

impl PendingRecentWork {
    /// Reconciles one ordered event against this read.
    ///
    /// Core answers a `QueryRecentWork` with exactly `CommandAccepted` then
    /// `RecentWorkLoaded`. The loaded fact carries no command id, so it only
    /// counts once *this* command was accepted — an inventory answer that
    /// arrives before the acceptance belongs to someone else's read, and a
    /// republished snapshot is never an answer at all.
    fn observe(&mut self, envelope: &RuntimeEventEnvelope) -> RecentWorkObservation {
        let RuntimeWireEvent::Known(event) = &envelope.event else {
            return RecentWorkObservation::Continue;
        };
        match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, reason }
                if command_id == &self.command_id =>
            {
                RecentWorkObservation::Rejected(reason.clone())
            }
            RuntimeEventKind::CommandAccepted {
                command_id,
                command,
            } if command_id == &self.command_id
                && matches!(command, RuntimeCommand::QueryRecentWork { .. }) =>
            {
                self.accepted = true;
                RecentWorkObservation::Continue
            }
            RuntimeEventKind::RecentWorkLoaded { .. } if self.accepted => {
                RecentWorkObservation::Confirmed
            }
            _ => RecentWorkObservation::Continue,
        }
    }
}

/// Label for a `#[non_exhaustive]` actor or outcome this build cannot name.
const UNKNOWN_AUDIT_LABEL: &str = "unknown";

/// Encodes a Core cursor as an opaque webview token.
///
/// `timestamp` is decimal digits and carries no `:`, so the first colon is
/// always the separator even though an audit id may legally contain one.
fn encode_audit_cursor(cursor: &AuditCursor) -> String {
    format!("{}:{}", cursor.timestamp, cursor.audit_id)
}

fn audit_object_projection(object: &AuditObjectRef) -> D14AuditObjectProjection {
    D14AuditObjectProjection {
        kind: object.kind.clone(),
        id: object.id.clone(),
    }
}

/// Projects one inventory entry verbatim.
///
/// The kind is stringified rather than passed through as a serialized enum so
/// an unmodeled `WorkspaceFileKind` — the type is `#[non_exhaustive]` — reaches
/// the frontend as a nameable `"unknown"` instead of being silently rendered
/// as a file.
fn workspace_file_row_projection(entry: &WorkspaceFileEntry) -> WorkspaceFileRowProjection {
    WorkspaceFileRowProjection {
        path: entry.path.clone(),
        kind: match entry.kind {
            WorkspaceFileKind::Dir => "dir",
            WorkspaceFileKind::File => "file",
            _ => "unknown",
        }
        .to_string(),
        size_bytes: entry.size_bytes,
    }
}

fn audit_row_projection(record: &AuditRecord) -> D14AuditRowProjection {
    let (actor_kind, agent_id) = match &record.actor {
        AuditActor::Operator => ("operator", None),
        AuditActor::System => ("system", None),
        AuditActor::Agent { agent_id } => ("agent", Some(agent_id.clone())),
        // `AuditActor` is `#[non_exhaustive]`: a newer Core's actor is named
        // as unknown rather than folded into one of the three above.
        _ => (UNKNOWN_AUDIT_LABEL, None),
    };
    D14AuditRowProjection {
        audit_id: record.audit_id.clone(),
        // The record's own owner, verbatim. D10's cross-project ticker needs
        // it to name which project an event came from, and no client may
        // infer a project from ordering or from the objects a record links.
        project_id: record.owner.project_id.clone(),
        lane_id: record.owner.lane_id.clone(),
        timestamp: record.timestamp,
        actor_kind: actor_kind.to_string(),
        agent_id,
        action: record.action.clone(),
        objects: record.objects.iter().map(audit_object_projection).collect(),
        outcome: match record.outcome {
            AuditOutcome::Success => "success",
            AuditOutcome::Denied => "denied",
            AuditOutcome::Failed => "failed",
            // See `actor_kind`: `AuditOutcome` is `#[non_exhaustive]` too, and
            // an unknown outcome must never borrow a known status.
            _ => UNKNOWN_AUDIT_LABEL,
        }
        .to_string(),
        args: record
            .args
            .iter()
            .map(|(key, value)| D14AuditArgProjection {
                key: key.clone(),
                value: value.clone(),
            })
            .collect(),
    }
}

/// One in-flight `QueryAudit` awaiting its ordered Core answer.
struct PendingAuditPage {
    command_id: String,
    accepted: bool,
    /// Whether this read continues the current list (load older) or replaces
    /// it (a first or re-scoped page). Applied only on confirmation.
    older: bool,
}

/// What Core published across the audit reads confirmed so far.
#[derive(Default)]
struct AuditReceipt {
    /// Newest first, exactly as Core delivered; older pages append at the end.
    rows: Vec<D14AuditRowProjection>,
    next_before: Option<AuditCursor>,
    complete: bool,
    /// True once one page has actually arrived. Absence and emptiness are
    /// different facts; see [`D14AuditProjection::loaded`].
    loaded: bool,
    /// The object filter the current list was read under, if any.
    scope: Option<AuditObjectRef>,
}

enum AuditObservation {
    Continue,
    Confirmed,
    Rejected(String),
}

/// One in-flight `QueryWorkspaceFiles` awaiting its ordered Core answer.
struct PendingWorkspaceFiles {
    /// The only correlation this read needs. Every event that can settle it —
    /// the page and the rejection alike — names the command id it answers, so
    /// there is no acceptance flag to fall back on and nothing is attributed
    /// by "a read was outstanding".
    command_id: String,
}

/// One in-flight `QueryWorkspaceDiff` awaiting its ordered Core answer.
struct PendingWorkspaceDiff {
    /// The only correlation this read needs: both events that can settle it —
    /// `WorkspaceDiffLoaded` and `CommandRejected` — name the command id they
    /// answer, and the page's id is a *required* field.
    command_id: String,
    /// The invalidation count at the moment the read was sent. A page is
    /// current against the tree as of this revision, so a fact that arrives
    /// while the read is in flight is already reflected in the answer.
    read_revision: u64,
}

impl PendingWorkspaceDiff {
    /// Reconciles one ordered event against this read.
    ///
    /// `RuntimeEventKind::Error` is deliberately not observed, for the reason
    /// the inventory read documents: it carries no command id, so treating one
    /// as this read's refusal because a read happened to be outstanding would
    /// fabricate a refusal Core never issued.
    fn observe(&self, envelope: &RuntimeEventEnvelope) -> AuditObservation {
        let RuntimeWireEvent::Known(event) = &envelope.event else {
            return AuditObservation::Continue;
        };
        match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, reason }
                if command_id == &self.command_id =>
            {
                AuditObservation::Rejected(reason.clone())
            }
            RuntimeEventKind::WorkspaceDiffLoaded { command_id, .. }
                if command_id == &self.command_id =>
            {
                AuditObservation::Confirmed
            }
            _ => AuditObservation::Continue,
        }
    }
}

/// What Core published for the diff reads confirmed so far.
#[derive(Default)]
struct WorkspaceDiffReceipt {
    target_lane_id: Option<String>,
    source: Option<crate::D1WorkspaceSourceProjection>,
    /// Lexicographic by path, exactly as Core delivered.
    entries: Vec<WorkspaceDiffEntryProjection>,
    truncated: bool,
    /// True once one page has actually arrived. Absence and emptiness are
    /// different facts; see [`WorkspaceDiffProjection::loaded`].
    loaded: bool,
    /// The invalidation count the confirmed page describes.
    read_revision: u64,
}

/// What Core published across the inventory reads confirmed so far.
#[derive(Default)]
struct WorkspaceFilesReceipt {
    /// Lexicographic by path, exactly as Core delivered.
    entries: Vec<WorkspaceFileRowProjection>,
    complete: bool,
    /// True once one page has actually arrived. Absence and emptiness are
    /// different facts; see [`WorkspaceFilesProjection::loaded`].
    loaded: bool,
}

impl PendingWorkspaceFiles {
    /// Reconciles one ordered event against this read.
    ///
    /// Core answers a `QueryWorkspaceFiles` with `CommandAccepted` then either
    /// `WorkspaceFilesLoaded` or, when the permission gate refused it,
    /// `CommandRejected`. Both name the exact command id they answer — the
    /// page's is a *required* field — so an event carrying another reader's id
    /// is ignored outright and there is no acceptance-gated fallback to guess
    /// with: the residual limitation `AuditPageLoaded` still documents does not
    /// exist here.
    ///
    /// `RuntimeEventKind::Error` is deliberately not observed. It carries no
    /// command id, so treating one as this read's refusal because a read
    /// happened to be outstanding would let an unrelated lane or provider
    /// failure fabricate a refusal Core never issued.
    fn observe(&self, envelope: &RuntimeEventEnvelope) -> AuditObservation {
        let RuntimeWireEvent::Known(event) = &envelope.event else {
            return AuditObservation::Continue;
        };
        match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, reason }
                if command_id == &self.command_id =>
            {
                AuditObservation::Rejected(reason.clone())
            }
            RuntimeEventKind::WorkspaceFilesLoaded { command_id, .. }
                if command_id == &self.command_id =>
            {
                AuditObservation::Confirmed
            }
            _ => AuditObservation::Continue,
        }
    }
}

impl PendingAuditPage {
    /// Reconciles one ordered event against this read.
    ///
    /// Core answers a `QueryAudit` with exactly `CommandAccepted` then
    /// `AuditPageLoaded`. Since GUI-CORE-024 the page names the exact command
    /// id it answers, so a page carrying *another* reader's id is ignored even
    /// while this read is outstanding.
    ///
    /// The residual limitation, stated rather than papered over — the same one
    /// `apps/tui/src/tui/audit_panel.rs` documents — applies only against a
    /// Core that predates that field: such a page arrives with no id, the
    /// acceptance-first fallback is the only attribution left, and a second
    /// client's page landing between our acceptance and Core's answer would
    /// still be attributed to this read. The page is still a real Core page, it
    /// is dropped when the screen re-queries, and speculative correlation
    /// (matching on record contents or guessing from the cursor) would invent
    /// certainty the contract does not provide.
    fn observe(&mut self, envelope: &RuntimeEventEnvelope) -> AuditObservation {
        let RuntimeWireEvent::Known(event) = &envelope.event else {
            return AuditObservation::Continue;
        };
        match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, reason }
                if command_id == &self.command_id =>
            {
                AuditObservation::Rejected(reason.clone())
            }
            RuntimeEventKind::CommandAccepted {
                command_id,
                command,
            } if command_id == &self.command_id
                && matches!(command, RuntimeCommand::QueryAudit { .. }) =>
            {
                self.accepted = true;
                AuditObservation::Continue
            }
            // Exact match when Core names the read; acceptance-first fallback
            // only for a page that carries no id at all.
            RuntimeEventKind::AuditPageLoaded { command_id, .. }
                if match command_id.as_deref() {
                    Some(published) => published == self.command_id,
                    None => self.accepted,
                } =>
            {
                AuditObservation::Confirmed
            }
            _ => AuditObservation::Continue,
        }
    }
}

fn recent_project_projection(summary: &RecentProjectSummary) -> RecentProjectProjection {
    RecentProjectProjection {
        canonical_root: summary.canonical_root.clone(),
        display_name: summary.display_name.clone(),
        last_updated_at: summary.last_updated_at,
        latest_session_id: summary.latest_session_id.clone(),
    }
}

fn recent_session_projection(summary: &RecentSessionSummary) -> RecentSessionProjection {
    RecentSessionProjection {
        canonical_root: summary.canonical_root.clone(),
        session_id: summary.session_id.clone(),
        created_at: summary.created_at,
        last_updated_at: summary.last_updated_at,
        message_count: summary.message_count,
        tool_call_count: summary.tool_call_count,
        command_count: summary.command_count,
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum D11Intent {
    ProbeProject,
    PreviewProjectConfig {
        contents: String,
    },
    ConfirmProjectConfig,
    StoreCredentialHandle {
        provider_id: String,
        backend_id: String,
        credential_request_id: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D11IntentResult {
    pub projection: D11IntakeProjection,
    pub pending_command_id: Option<String>,
    pub pending_intent: Option<String>,
}

enum D11CompletionTarget {
    ProjectProbe,
    ProjectConfigPreview {
        content_sha256: String,
    },
    ProjectConfigConfirm {
        preview_id: String,
        content_sha256: String,
    },
    CredentialHandle {
        provider_id: String,
        backend_id: String,
    },
}

impl D11CompletionTarget {
    fn from_command(command: &RuntimeCommand) -> Result<Self, String> {
        match command {
            RuntimeCommand::ProbeProject => Ok(Self::ProjectProbe),
            RuntimeCommand::PreviewProjectConfig { contents } => Ok(Self::ProjectConfigPreview {
                content_sha256: sha256(contents.as_bytes()),
            }),
            RuntimeCommand::ConfirmProjectConfig {
                preview_id,
                content_sha256,
            } => Ok(Self::ProjectConfigConfirm {
                preview_id: preview_id.clone(),
                content_sha256: content_sha256.clone(),
            }),
            RuntimeCommand::StoreCredentialHandle {
                provider_id,
                backend_id,
                ..
            } => Ok(Self::CredentialHandle {
                provider_id: provider_id.clone(),
                backend_id: backend_id.clone(),
            }),
            _ => Err("unsupported D11 Core command".to_string()),
        }
    }

    fn intent_name(&self) -> &'static str {
        match self {
            Self::ProjectProbe => "probe_project",
            Self::ProjectConfigPreview { .. } => "preview_project_config",
            Self::ProjectConfigConfirm { .. } => "confirm_project_config",
            Self::CredentialHandle { .. } => "store_credential_handle",
        }
    }

    fn matches_fact(&self, kind: &RuntimeEventKind) -> bool {
        match kind {
            RuntimeEventKind::ProjectProbed { .. } => matches!(self, Self::ProjectProbe),
            RuntimeEventKind::ProjectConfigPreviewed { preview } => {
                matches!(self, Self::ProjectConfigPreview { content_sha256 } if preview.content_sha256 == *content_sha256)
            }
            RuntimeEventKind::ProjectConfigConfirmed { preview } => {
                matches!(self, Self::ProjectConfigConfirm { preview_id, content_sha256 }
                    if preview.preview_id == *preview_id && preview.content_sha256 == *content_sha256)
            }
            RuntimeEventKind::CredentialHandleStored { handle } => {
                matches!(self, Self::CredentialHandle { provider_id, backend_id }
                    if handle.provider_id == *provider_id && handle.backend_id == *backend_id)
            }
            _ => false,
        }
    }

    fn matches_approval(&self, approval: &ApprovalRequestView) -> bool {
        match self {
            Self::ProjectConfigConfirm {
                preview_id,
                content_sha256,
            } => {
                matches!(
                    approval.tool_name.as_str(),
                    "project_config_confirm" | "workflow_project_config_confirm"
                ) && approval_metadata_exact(approval, "sha256", content_sha256, is_lower_hex_64)
                    && (!approval_metadata_field_exists(approval, "preview_id")
                        || approval_metadata_exact(approval, "preview_id", preview_id, |_| true))
            }
            Self::CredentialHandle {
                provider_id,
                backend_id,
            } => {
                approval.tool_name.contains("credential_handle_store")
                    && approval.input_preview.contains(provider_id)
                    && approval.input_preview.contains(backend_id)
            }
            Self::ProjectProbe | Self::ProjectConfigPreview { .. } => false,
        }
    }
}

struct PendingD11 {
    command_id: String,
    target: D11CompletionTarget,
    accepted: bool,
    approval_request_id: Option<String>,
}

enum PendingObservation {
    Continue,
    ClearedContinue,
    ClearedStop,
}

impl PendingD11 {
    fn observe(&mut self, envelope: &RuntimeEventEnvelope) -> PendingObservation {
        let RuntimeWireEvent::Known(event) = &envelope.event else {
            return PendingObservation::Continue;
        };
        match &event.kind {
            RuntimeEventKind::CommandRejected {
                command_id: rejected,
                ..
            } if rejected == &self.command_id => PendingObservation::ClearedContinue,
            RuntimeEventKind::CommandAccepted {
                command_id: accepted,
                ..
            }
            | RuntimeEventKind::LaneCommandAccepted {
                command_id: accepted,
                ..
            } if accepted == &self.command_id => {
                self.accepted = true;
                PendingObservation::Continue
            }
            RuntimeEventKind::ApprovalRequested { approval }
                if self.accepted && self.target.matches_approval(approval) =>
            {
                self.approval_request_id = Some(approval.id.clone());
                PendingObservation::Continue
            }
            RuntimeEventKind::ApprovalResolved {
                request_id,
                decision,
                ..
            } if self.approval_request_id.as_deref() == Some(request_id.as_str()) => match decision
            {
                ApprovalDecision::Allow { .. } => PendingObservation::Continue,
                ApprovalDecision::Deny => PendingObservation::ClearedContinue,
            },
            kind if self.accepted && self.target.matches_fact(kind) => {
                PendingObservation::ClearedStop
            }
            _ => PendingObservation::Continue,
        }
    }
}

impl GuiCoreAdapter {
    pub fn new(client: Box<dyn CoreClient>) -> Self {
        Self {
            client,
            projection: RuntimeProjection::default(),
            capabilities: BTreeSet::new(),
            pending_d11: None,
            pending_d4: None,
            reviewed_d4: None,
            d4_receipt: None,
            d4_outcome: D4OutcomeProjection::idle(),
            pending_d1: None,
            d1_outcome: D1OutcomeProjection::idle(),
            pending_d12: None,
            d12_outcome: D1OutcomeProjection::idle(),
            pending_permission: None,
            permission_outcome: PermissionOutcomeProjection::idle(),
            pending_preference: None,
            preference_outcome: D1OutcomeProjection::idle(),
            preference_receipt: None,
            pending_recent_work: None,
            recent_work_outcome: D1OutcomeProjection::idle(),
            recent_work_receipt: None,
            connection: D6ConnectionState::Disconnected,
            connection_detail: None,
            d2_selected: None,
            pending_d2_review: None,
            pending_audit: None,
            pending_workspace_files: None,
            workspace_files_outcome: D1OutcomeProjection::idle(),
            workspace_files_receipt: WorkspaceFilesReceipt::default(),
            audit_outcome: D1OutcomeProjection::idle(),
            audit_receipt: AuditReceipt::default(),
            pending_workspace_diff: None,
            workspace_diff_outcome: D1OutcomeProjection::idle(),
            workspace_diff_receipt: WorkspaceDiffReceipt::default(),
            workspace_revision: 0,
        }
    }

    pub fn connect(&mut self) -> Result<(), CoreClientError> {
        self.connection = D6ConnectionState::Connecting;
        self.connection_detail = None;
        let handshake = match self.client.discover() {
            Ok(handshake) => handshake,
            Err(error) => {
                self.record_connection_error(&error);
                return Err(error);
            }
        };
        self.capabilities = handshake
            .capabilities
            .into_iter()
            .map(|capability| capability.0)
            .collect();
        let snapshot = match self.client.snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.record_connection_error(&error);
                return Err(error);
            }
        };
        self.publish(snapshot);
        self.connection = D6ConnectionState::Live;
        self.connection_detail = None;
        Ok(())
    }

    pub fn send_intent(&mut self, command: RuntimeCommandEnvelope) -> Result<(), CoreClientError> {
        self.client.send(command)
    }

    pub fn pump(&mut self, timeout: Duration) -> Result<bool, CoreClientError> {
        let event = self.receive_and_publish(timeout)?;
        if let Some(event) = &event {
            self.observe_pending(event);
        }
        Ok(event.is_some())
    }

    pub fn projection(&self) -> &RuntimeProjection {
        &self.projection
    }

    /// Workspace root Core probed, when Core has published one.
    ///
    /// The client keeps no second copy of where the workspace lives; reading
    /// Core's probe keeps a reopened workspace from resolving against the
    /// previous root.
    pub fn workspace_root(&self) -> Option<String> {
        self.projection
            .view()?
            .project_probe
            .as_ref()
            .map(|probe| probe.root.clone())
    }

    pub fn supports(&self, capability: &str) -> bool {
        self.capabilities.contains(capability)
    }

    pub fn d1_cockpit(&self, selected_lane_id: Option<&str>) -> Option<D1CockpitProjection> {
        let mut projection = self.projection.d1_cockpit(selected_lane_id)?;
        if projection.permission_dock.request.is_none()
            && let Some(pending) = &self.pending_d1
            && pending.accepted
            && matches!(pending.kind, PendingD1Kind::CreateStarterLane { .. })
            && let Some(permission_dock) = self.projection.permission_dock_for_owner(&pending.owner)
        {
            // Lane creation approval precedes Lane registration, so no selected
            // owner exists yet. Route only the exact accepted Create owner;
            // never fall back to another owner's global pending approval.
            projection.permission_dock = permission_dock;
        }
        let transport_recovery = self.d6_recovery();
        if self.connection != D6ConnectionState::Live
            || projection.recovery.state != D6State::EventGap
        {
            projection.recovery = transport_recovery;
        }
        Some(projection)
    }

    pub fn permission_dock(&self) -> Option<PermissionDockProjection> {
        self.projection.permission_dock()
    }

    /// Whether Core's handshake published the audit timeline.
    ///
    /// D14 reads this before it renders, so an absent capability shows as an
    /// honest unavailable audit mode with the raw replay mode still usable,
    /// rather than as an empty timeline that reads as "nothing ever happened".
    pub fn supports_audit(&self) -> bool {
        self.supports(AUDIT_CAPABILITY)
    }

    /// Sends one `QueryAudit` for the newest page and waits for Core's answer.
    ///
    /// `scope` is the object filter, or `None` for the project timeline: Core's
    /// store is already scoped to the project's own workflow directory, so an
    /// unscoped query needs no locally invented `project_id`. The read is
    /// available in Plan mode and never requests approval.
    ///
    /// A missing capability is not an error: it returns the honest projection
    /// with `capability_available == false` and sends nothing, so D14 can fall
    /// back to raw replay mode instead of showing a failed screen.
    pub fn query_audit_and_wait(
        &mut self,
        command_id: &str,
        scope: Option<D14AuditScopeInput>,
        event_timeout: Duration,
    ) -> Result<D14AuditProjection, String> {
        let scope = scope.map(|scope| AuditObjectRef::new(scope.kind, scope.id));
        self.send_audit_query(
            command_id,
            scope,
            None,
            false,
            D14_AUDIT_PAGE_LIMIT,
            event_timeout,
        )
    }

    /// Sends one `QueryAudit` for the page older than Core's own cursor.
    ///
    /// The cursor travels back verbatim; the client never derives a position
    /// from a timestamp it rendered.
    pub fn load_older_audit_and_wait(
        &mut self,
        command_id: &str,
        event_timeout: Duration,
    ) -> Result<D14AuditProjection, String> {
        let before = self
            .audit_receipt
            .next_before
            .clone()
            .ok_or_else(|| "Core published no older audit cursor".to_string())?;
        let scope = self.audit_receipt.scope.clone();
        self.send_audit_query(
            command_id,
            scope,
            Some(before),
            true,
            D14_AUDIT_PAGE_LIMIT,
            event_timeout,
        )
    }

    /// Reads the D10 event ticker from the same audit contract D14 uses.
    ///
    /// GUI-CORE-014. The ticker is one bounded newest-first page over the whole
    /// workspace: unscoped, so it spans every project, and ordered by Core on
    /// `(timestamp, audit_id)` — the `audit-ordering` fixture is the canonical
    /// proof that this order is total across projects rather than per project.
    /// The client never rebuilds a timeline by diffing successive snapshots,
    /// which is what this request forbade.
    ///
    /// It shares the single audit slot deliberately: D10 and D14 are two views
    /// of one Core timeline, and only one of them is mounted at a time, so a
    /// second slot would buy nothing but a way to page two lists into one.
    pub fn d10_events_and_wait(
        &mut self,
        command_id: &str,
        event_timeout: Duration,
    ) -> Result<D14AuditProjection, String> {
        self.send_audit_query(
            command_id,
            None,
            None,
            false,
            D10_EVENT_TICKER_LIMIT,
            event_timeout,
        )
    }

    fn send_audit_query(
        &mut self,
        command_id: &str,
        scope: Option<AuditObjectRef>,
        before: Option<AuditCursor>,
        older: bool,
        limit: u32,
        event_timeout: Duration,
    ) -> Result<D14AuditProjection, String> {
        if !self.supports_audit() {
            return Ok(self.d14_audit());
        }
        if let Some(pending) = &self.pending_audit {
            return Err(format!(
                "audit query `{}` is still pending",
                pending.command_id
            ));
        }
        self.client
            .send(RuntimeCommandEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                client_id: "viden-gui".to_string(),
                command_id: command_id.to_string(),
                // The timeline spans every Lane in the project, so the read is
                // project-scoped rather than bound to a Lane owner.
                owner: RuntimeOwner::default(),
                command: RuntimeCommand::QueryAudit {
                    query: AuditQuery {
                        object: scope.clone(),
                        before,
                        limit,
                        // Core's actor and time filters exist, but D14 ships no
                        // filter chips yet, so the host never sends a filter the
                        // operator did not choose.
                        ..AuditQuery::default()
                    },
                },
            })
            .map_err(|error| error.to_string())?;
        self.pending_audit = Some(PendingAuditPage {
            command_id: command_id.to_string(),
            accepted: false,
            older,
        });
        self.audit_outcome = D1OutcomeProjection::pending();
        if !older {
            // A first or re-scoped read replaces the list. Clearing before the
            // answer keeps the previous scope's records from being read as
            // this scope's, and `loaded` goes back to false so the screen says
            // "loading", never "no records".
            self.audit_receipt = AuditReceipt {
                scope,
                ..AuditReceipt::default()
            };
        }
        self.poll_audit(event_timeout)
    }

    /// Drains ordered Core events for an audit read still in flight.
    pub fn poll_audit(&mut self, event_timeout: Duration) -> Result<D14AuditProjection, String> {
        let mut received = false;
        let mut receive_failed = false;
        for _ in 0..8 {
            let event = match self.receive_event_until(event_timeout) {
                Ok(Some(event)) => event,
                Ok(None) => break,
                Err(_) => {
                    // The transport state is already classified by receive_event.
                    receive_failed = true;
                    break;
                }
            };
            received = true;
            if self.observe_pending_audit(&event) {
                break;
            }
        }
        if received && !receive_failed {
            self.refresh_projection()
                .map_err(|error| error.to_string())?;
        }
        Ok(self.d14_audit())
    }

    /// Reconciles one ordered event against the in-flight audit read.
    ///
    /// Returns whether the read reached a terminal outcome. The desktop event
    /// pump calls this too, so a background drain can never swallow the only
    /// page the screen is waiting for.
    fn observe_pending_audit(&mut self, event: &RuntimeEventEnvelope) -> bool {
        let observation = self
            .pending_audit
            .as_mut()
            .map_or(AuditObservation::Continue, |pending| pending.observe(event));
        match observation {
            AuditObservation::Continue => false,
            AuditObservation::Confirmed => {
                let older = self
                    .pending_audit
                    .take()
                    .is_some_and(|pending| pending.older);
                self.audit_outcome = D1OutcomeProjection::confirmed();
                // The confirming page is the authority for the rows and for
                // the next cursor; nothing is re-ordered or recomputed.
                if let RuntimeWireEvent::Known(known) = &event.event
                    && let RuntimeEventKind::AuditPageLoaded { page, .. } = &known.kind
                {
                    let rows = page.records.iter().map(audit_row_projection);
                    if older {
                        self.audit_receipt.rows.extend(rows);
                    } else {
                        self.audit_receipt.rows = rows.collect();
                    }
                    self.audit_receipt.next_before = page.next_before.clone();
                    self.audit_receipt.complete = page.complete;
                    self.audit_receipt.loaded = true;
                }
                true
            }
            AuditObservation::Rejected(reason) => {
                self.pending_audit = None;
                self.audit_outcome = D1OutcomeProjection::rejected(reason);
                true
            }
        }
    }

    /// The audit mode's current projection, with no Core traffic of its own.
    pub fn d14_audit(&self) -> D14AuditProjection {
        D14AuditProjection {
            outcome: self.audit_outcome.clone(),
            rows: self.audit_receipt.rows.clone(),
            next_before: self
                .audit_receipt
                .next_before
                .as_ref()
                .map(encode_audit_cursor),
            complete: self.audit_receipt.complete,
            loaded: self.audit_receipt.loaded,
            pending_command_id: self
                .pending_audit
                .as_ref()
                .map(|pending| pending.command_id.clone()),
            capability_available: self.supports_audit(),
            scope: self
                .audit_receipt
                .scope
                .as_ref()
                .map(|object| D14AuditScopeProjection {
                    kind: object.kind.clone(),
                    id: object.id.clone(),
                }),
        }
    }

    /// Whether Core's handshake published the workspace file inventory.
    ///
    /// The palette reads this before it renders the `~` scope, so an absent
    /// capability shows the honest disabled row naming the contract request
    /// rather than an empty list that reads as "this workspace has no files".
    pub fn supports_workspace_files(&self) -> bool {
        self.supports(WORKSPACE_FILES_CAPABILITY)
    }

    /// Sends one `QueryWorkspaceFiles` for the whole tree and waits for Core.
    ///
    /// The palette fuzzy-matches locally over what Core sent, so it asks for
    /// the inventory rather than a prefix the operator has not typed. The read
    /// is permission-gated by Core, stays available in Plan mode because it
    /// mutates nothing, and never blocks on an approval prompt.
    ///
    /// A missing capability is not an error: it returns the honest projection
    /// with `capability_available == false` and sends nothing, so the palette
    /// keeps the disabled row instead of showing a failed screen.
    pub fn query_workspace_files_and_wait(
        &mut self,
        command_id: &str,
        event_timeout: Duration,
    ) -> Result<WorkspaceFilesProjection, String> {
        if !self.supports_workspace_files() {
            return Ok(self.d1_workspace_files());
        }
        if let Some(pending) = &self.pending_workspace_files {
            return Err(format!(
                "workspace files query `{}` is still pending",
                pending.command_id
            ));
        }
        self.client
            .send(RuntimeCommandEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                client_id: "viden-gui".to_string(),
                command_id: command_id.to_string(),
                // The inventory is the open workspace, not one Lane's
                // worktree, so the read carries no Lane owner.
                owner: RuntimeOwner::default(),
                command: RuntimeCommand::QueryWorkspaceFiles {
                    query: WorkspaceFilesQuery {
                        prefix: None,
                        limit: Some(WORKSPACE_FILES_PAGE_LIMIT),
                        after: None,
                    },
                },
            })
            .map_err(|error| error.to_string())?;
        self.pending_workspace_files = Some(PendingWorkspaceFiles {
            command_id: command_id.to_string(),
        });
        self.workspace_files_outcome = D1OutcomeProjection::pending();
        // A fresh read replaces the list. Clearing before the answer keeps a
        // previous workspace's paths from being read as this one's, and
        // `loaded` goes back to false so the palette says "loading", never
        // "no files".
        self.workspace_files_receipt = WorkspaceFilesReceipt::default();
        self.poll_workspace_files(event_timeout)
    }

    /// Drains ordered Core events for an inventory read still in flight.
    pub fn poll_workspace_files(
        &mut self,
        event_timeout: Duration,
    ) -> Result<WorkspaceFilesProjection, String> {
        let mut received = false;
        let mut receive_failed = false;
        for _ in 0..8 {
            let event = match self.receive_event_until(event_timeout) {
                Ok(Some(event)) => event,
                Ok(None) => break,
                Err(_) => {
                    receive_failed = true;
                    break;
                }
            };
            received = true;
            if self.observe_pending_workspace_files(&event) {
                break;
            }
        }
        if received && !receive_failed {
            self.refresh_projection()
                .map_err(|error| error.to_string())?;
        }
        Ok(self.d1_workspace_files())
    }

    /// Reconciles one ordered event against the in-flight inventory read.
    ///
    /// Returns whether the read reached a terminal outcome. The desktop event
    /// pump calls this too, so a background drain can never swallow the only
    /// page the palette is waiting for.
    pub(crate) fn observe_pending_workspace_files(&mut self, event: &RuntimeEventEnvelope) -> bool {
        let observation = self
            .pending_workspace_files
            .as_ref()
            .map_or(AuditObservation::Continue, |pending| pending.observe(event));
        match observation {
            AuditObservation::Continue => false,
            AuditObservation::Confirmed => {
                self.pending_workspace_files = None;
                self.workspace_files_outcome = D1OutcomeProjection::confirmed();
                // The confirming page is the authority for the rows; nothing is
                // re-ordered, re-sorted, or recomputed on this side.
                if let RuntimeWireEvent::Known(known) = &event.event
                    && let RuntimeEventKind::WorkspaceFilesLoaded { page, .. } = &known.kind
                {
                    self.workspace_files_receipt.entries = page
                        .entries
                        .iter()
                        .map(workspace_file_row_projection)
                        .collect();
                    self.workspace_files_receipt.complete = page.complete;
                    self.workspace_files_receipt.loaded = true;
                }
                true
            }
            AuditObservation::Rejected(reason) => {
                self.pending_workspace_files = None;
                self.workspace_files_outcome = D1OutcomeProjection::rejected(reason);
                true
            }
        }
    }

    /// The palette file scope's current projection, with no Core traffic.
    pub fn d1_workspace_files(&self) -> WorkspaceFilesProjection {
        WorkspaceFilesProjection {
            outcome: self.workspace_files_outcome.clone(),
            entries: self.workspace_files_receipt.entries.clone(),
            complete: self.workspace_files_receipt.complete,
            loaded: self.workspace_files_receipt.loaded,
            pending_command_id: self
                .pending_workspace_files
                .as_ref()
                .map(|pending| pending.command_id.clone()),
            capability_available: self.supports_workspace_files(),
        }
    }

    /// Whether Core's handshake published structured diff rows.
    ///
    /// DiffReview reads this before it offers an entry point, so a Core
    /// without the capability renders the entry disabled-and-labelled rather
    /// than opening a view that can never fill.
    pub fn supports_structured_diff(&self) -> bool {
        self.supports(STRUCTURED_DIFF_CAPABILITY)
    }

    /// Sends one `QueryWorkspaceDiff` and waits for Core's ordered answer.
    ///
    /// The query is the review pane's whole request: the named target, both
    /// sides of the index, every changed path, and Core's own default byte
    /// bound. `paths` stays empty because the operator has not filtered
    /// anything, and `byte_limit` stays `None` because Core owns the bound and
    /// publishes the one it used on the page.
    ///
    /// `lane_id` names one Lane's worktree; `None` is the workspace root.
    /// Core resolves a Lane's worktree from its own records — the client never
    /// passes a path — and an unknown or archived Lane comes back as a
    /// refusal rather than as the workspace's facts under a Lane's name.
    ///
    /// A missing capability is not an error: it returns the honest projection
    /// with `capability_available == false` and sends nothing.
    pub fn query_workspace_diff_and_wait(
        &mut self,
        command_id: &str,
        lane_id: Option<&str>,
        event_timeout: Duration,
    ) -> Result<WorkspaceDiffProjection, String> {
        if !self.supports_structured_diff() {
            return Ok(self.workspace_diff());
        }
        if let Some(pending) = &self.pending_workspace_diff {
            return Err(format!(
                "workspace diff query `{}` is still pending",
                pending.command_id
            ));
        }
        let target = match lane_id {
            Some(lane_id) => SourceTarget::Lane {
                lane_id: lane_id.to_string(),
            },
            None => SourceTarget::Workspace,
        };
        self.client
            .send(RuntimeCommandEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                client_id: "viden-gui".to_string(),
                command_id: command_id.to_string(),
                // The target is named in the query itself, so the read carries
                // no Lane owner; Core resolves the worktree.
                owner: RuntimeOwner::default(),
                command: RuntimeCommand::QueryWorkspaceDiff {
                    query: WorkspaceDiffQuery {
                        target,
                        scope: WorkspaceDiffScope::Both,
                        paths: Vec::new(),
                        byte_limit: None,
                    },
                },
            })
            .map_err(|error| error.to_string())?;
        self.pending_workspace_diff = Some(PendingWorkspaceDiff {
            command_id: command_id.to_string(),
            read_revision: self.workspace_revision,
        });
        self.workspace_diff_outcome = D1OutcomeProjection::pending();
        // A fresh read replaces the page. Clearing before the answer keeps one
        // target's rows from being read as another's, and `loaded` goes back
        // to false so the view says "reading", never "no changes".
        self.workspace_diff_receipt = WorkspaceDiffReceipt::default();
        self.poll_workspace_diff(event_timeout)
    }

    /// Drains ordered Core events for a diff read still in flight.
    pub fn poll_workspace_diff(
        &mut self,
        event_timeout: Duration,
    ) -> Result<WorkspaceDiffProjection, String> {
        let mut received = false;
        let mut receive_failed = false;
        for _ in 0..8 {
            let event = match self.receive_event_until(event_timeout) {
                Ok(Some(event)) => event,
                Ok(None) => break,
                Err(_) => {
                    receive_failed = true;
                    break;
                }
            };
            received = true;
            if self.observe_pending_workspace_diff(&event) {
                break;
            }
        }
        if received && !receive_failed {
            self.refresh_projection()
                .map_err(|error| error.to_string())?;
        }
        Ok(self.workspace_diff())
    }

    /// Reconciles one ordered event against the in-flight diff read.
    ///
    /// Returns whether the read reached a terminal outcome. The desktop event
    /// pump calls this too, so a background drain can never swallow the only
    /// page the view is waiting for.
    pub(crate) fn observe_pending_workspace_diff(&mut self, event: &RuntimeEventEnvelope) -> bool {
        let observation = self
            .pending_workspace_diff
            .as_ref()
            .map_or(AuditObservation::Continue, |pending| pending.observe(event));
        match observation {
            AuditObservation::Continue => false,
            AuditObservation::Confirmed => {
                let read_revision = self
                    .pending_workspace_diff
                    .take()
                    .map_or(self.workspace_revision, |pending| pending.read_revision);
                self.workspace_diff_outcome = D1OutcomeProjection::confirmed();
                // The confirming page is the authority: nothing is re-ordered,
                // re-sorted, or recomputed on this side.
                if let RuntimeWireEvent::Known(known) = &event.event
                    && let RuntimeEventKind::WorkspaceDiffLoaded { page, .. } = &known.kind
                {
                    self.workspace_diff_receipt = WorkspaceDiffReceipt {
                        target_lane_id: target_lane_id(&page.target),
                        source: Some(workspace_diff_source_projection(&page.source)),
                        entries: page
                            .entries
                            .iter()
                            .map(workspace_diff_entry_projection)
                            .collect(),
                        truncated: page.truncated,
                        loaded: true,
                        read_revision,
                    };
                }
                true
            }
            AuditObservation::Rejected(reason) => {
                self.pending_workspace_diff = None;
                self.workspace_diff_outcome = D1OutcomeProjection::rejected(reason);
                true
            }
        }
    }

    /// The DiffReview view's current projection, with no Core traffic.
    pub fn workspace_diff(&self) -> WorkspaceDiffProjection {
        WorkspaceDiffProjection {
            outcome: self.workspace_diff_outcome.clone(),
            target_lane_id: self.workspace_diff_receipt.target_lane_id.clone(),
            source: self.workspace_diff_receipt.source.clone(),
            entries: self.workspace_diff_receipt.entries.clone(),
            truncated: self.workspace_diff_receipt.truncated,
            loaded: self.workspace_diff_receipt.loaded,
            pending_command_id: self
                .pending_workspace_diff
                .as_ref()
                .map(|pending| pending.command_id.clone()),
            capability_available: self.supports_structured_diff(),
            // Only a page that is actually on screen can go stale; a read that
            // never answered is "pending", which is a different sentence.
            stale: self.workspace_diff_receipt.loaded
                && self.workspace_revision != self.workspace_diff_receipt.read_revision,
        }
    }

    /// Pages the raw event log through the Core replay cursor.
    ///
    /// This is D14's diagnostic mode, not its audit trail: it answers "which
    /// ordered events did Core emit", which stays meaningful when the audit
    /// capability is absent. `after` is the opaque `stream:sequence` cursor
    /// returned by the previous page. A replay failure is reported rather than
    /// rendered as a shorter trail, because a truncated view reads as a
    /// complete one.
    pub fn d14_audit_timeline(
        &mut self,
        after: Option<&str>,
        limit: u32,
    ) -> Result<crate::D14AuditTimelineProjection, String> {
        let cursor = match after {
            Some(raw) => parse_audit_cursor(raw)?,
            None => self
                .projection
                .cursor()
                .cloned()
                .map(|cursor| viden_core::EventCursor {
                    stream_id: cursor.stream_id,
                    sequence: 0,
                })
                .unwrap_or(viden_core::EventCursor {
                    stream_id: String::new(),
                    sequence: 0,
                }),
        };
        let batch = self
            .client
            .replay(viden_core::ReplayRequest {
                after: cursor,
                limit,
            })
            .map_err(|error| format!("Core replay failed: {error}"))?;
        let rows = batch
            .events
            .iter()
            .map(|envelope| {
                let (kind, known, timestamp) = match &envelope.event {
                    viden_core::RuntimeWireEvent::Known(event) => (
                        // The canonical name is Core's own serde discriminant,
                        // so the client never keeps a second event vocabulary.
                        core_event_kind(&event.kind),
                        true,
                        event.timestamp,
                    ),
                    _ => ("unknown".to_string(), false, None),
                };
                crate::D14RowProjection {
                    sequence: envelope.cursor.sequence,
                    stream_id: envelope.cursor.stream_id.clone(),
                    kind,
                    known,
                    timestamp,
                    project_id: envelope.owner.project_id.clone(),
                    lane_id: envelope.owner.lane_id.clone(),
                    session_id: envelope.owner.session_id.clone(),
                    task_id: envelope.owner.task_id.clone(),
                }
            })
            .collect();
        Ok(crate::D14AuditTimelineProjection {
            rows,
            next_cursor: Some(format!("{}:{}", batch.next.stream_id, batch.next.sequence)),
            complete: batch.complete,
        })
    }

    pub fn d13_fleet_workflow(&self) -> Option<crate::D13FleetWorkflowProjection> {
        self.projection.d13_fleet_workflow()
    }

    pub fn d12_integration_gate(&self) -> Option<crate::D12IntegrationGateProjection> {
        self.projection.d12_integration_gate(None)
    }

    /// Projects the gate the shell selected without persisting the selection.
    pub fn d12_integration_gate_for(
        &self,
        gate_id: &str,
    ) -> Option<crate::D12IntegrationGateProjection> {
        self.projection.d12_integration_gate(Some(gate_id))
    }

    /// Sends one merge-gate decision as the Core command that owns it, then
    /// republishes the gate projection.
    ///
    /// The gate id comes from the projection the operator acted on and is
    /// re-resolved against the current Core view here: a gate that
    /// disappeared, closed, or lost its actor between render and click is
    /// rejected locally instead of being sent as a command Core would have to
    /// reject. The actor and reviewed-evidence bindings are replayed from
    /// Core's own records — the GUI never rebuilds an identity or a hash from
    /// display text.
    pub fn send_d12_intent_and_wait(
        &mut self,
        command_id: &str,
        intent: crate::D12Intent,
        event_timeout: Duration,
    ) -> Result<crate::D12IntentResult, String> {
        if let Some(pending) = &self.pending_d12 {
            return Err(format!(
                "merge gate command `{}` is still pending",
                pending.command_id
            ));
        }
        let gate_id = match &intent {
            crate::D12Intent::Accept { gate_id, .. } | crate::D12Intent::Bounce { gate_id, .. } => {
                gate_id.clone()
            }
        };
        let status = match &intent {
            crate::D12Intent::Accept { .. } => MergeGateStatus::Accepted,
            crate::D12Intent::Bounce { .. } => MergeGateStatus::NeedsChanges,
        };
        let envelope = self.d12_envelope(command_id, &intent)?;
        self.pending_d12 = Some(PendingD12 {
            command_id: command_id.to_string(),
            gate_id: gate_id.clone(),
            status,
            accepted: false,
        });
        self.client.send(envelope).map_err(|error| {
            self.pending_d12 = None;
            error.to_string()
        })?;
        self.d12_outcome = D1OutcomeProjection::pending();
        self.poll_d12(&gate_id, event_timeout)
    }

    /// Drains ordered Core events for an in-flight gate decision without
    /// sending a command or creating GUI-owned runtime state.
    pub fn poll_d12(
        &mut self,
        gate_id: &str,
        event_timeout: Duration,
    ) -> Result<crate::D12IntentResult, String> {
        let mut received = false;
        for _ in 0..8 {
            let event = match self
                .receive_event_until(if received {
                    Duration::ZERO
                } else {
                    event_timeout
                })
                .map_err(|error| error.to_string())?
            {
                Some(event) => event,
                None => break,
            };
            received = true;
            if self.observe_pending_d12(&event) {
                break;
            }
        }
        if received {
            self.refresh_projection()
                .map_err(|error| error.to_string())?;
        }
        let projection = self
            .d12_integration_gate_for(gate_id)
            .or_else(|| self.d12_integration_gate())
            .ok_or_else(|| "Core has not published an integration gate projection".to_string())?;
        Ok(crate::D12IntentResult {
            projection,
            pending_command_id: self
                .pending_d12
                .as_ref()
                .map(|pending| pending.command_id.clone()),
            outcome: self.d12_outcome.clone(),
        })
    }

    /// Reconciles one ordered event against the in-flight gate decision.
    /// Returns whether the decision reached a terminal outcome.
    fn observe_pending_d12(&mut self, event: &RuntimeEventEnvelope) -> bool {
        let Some(pending) = self.pending_d12.as_mut() else {
            return false;
        };
        match pending.observe(event) {
            D12Observation::Continue => false,
            D12Observation::Confirmed => {
                self.pending_d12 = None;
                self.d12_outcome = D1OutcomeProjection::confirmed();
                true
            }
            D12Observation::Rejected(reason) => {
                self.pending_d12 = None;
                self.d12_outcome = D1OutcomeProjection::rejected(reason);
                true
            }
        }
    }

    fn d12_envelope(
        &self,
        command_id: &str,
        intent: &crate::D12Intent,
    ) -> Result<RuntimeCommandEnvelope, String> {
        let view = self
            .projection
            .view()
            .ok_or_else(|| "Core has not published an integration gate projection".to_string())?;
        let gate_id = match intent {
            crate::D12Intent::Accept { gate_id, .. } | crate::D12Intent::Bounce { gate_id, .. } => {
                gate_id.as_str()
            }
        };
        let gate = view
            .merge_gates
            .iter()
            .find(|gate| gate.gate_id == gate_id)
            .ok_or_else(|| format!("Core has no merge gate `{gate_id}` to decide"))?;
        if !gate.status.is_open() {
            return Err(format!("merge gate `{gate_id}` is no longer open"));
        }
        let (owner, command) = match intent {
            crate::D12Intent::Accept {
                reviewed_evidence,
                decision,
                ..
            } => {
                let actor = crate::projection::d12_accept_actor(gate).ok_or_else(|| {
                    format!("merge gate `{gate_id}` has no Core owner this client may accept as")
                })?;
                // A client-supplied binding set is honoured verbatim so a
                // reviewer can accept exactly what they read; otherwise the
                // bindings are replayed from Core's own canonical records.
                let reviewed_evidence = match reviewed_evidence {
                    Some(bindings) => bindings
                        .iter()
                        .map(|binding| viden_core::ReviewedEvidenceBinding {
                            evidence_id: binding.evidence_id.clone(),
                            source_hash: binding.source_hash.clone(),
                        })
                        .collect(),
                    None => {
                        crate::projection::d12_reviewed_evidence(view, gate).ok_or_else(|| {
                            format!(
                                "merge gate `{gate_id}` has no canonical evidence Core could verify"
                            )
                        })?
                    }
                };
                (
                    actor.clone(),
                    RuntimeCommand::AcceptMergeGate {
                        gate_id: gate_id.to_string(),
                        actor,
                        reviewed_evidence,
                        decision: decision.clone(),
                    },
                )
            }
            crate::D12Intent::Bounce { reason, .. } => {
                // Core stores the reason as the gate decision and refuses an
                // empty one, so the client will not send a blank bounce.
                if reason.trim().is_empty() {
                    return Err("a bounce to the origin Lane requires a reason".to_string());
                }
                let actor = crate::projection::d12_reject_actor(gate).ok_or_else(|| {
                    format!("merge gate `{gate_id}` has no Core owner this client may bounce as")
                })?;
                (
                    actor.clone(),
                    RuntimeCommand::RejectMergeGate {
                        gate_id: gate_id.to_string(),
                        actor,
                        reason: reason.clone(),
                    },
                )
            }
        };
        Ok(RuntimeCommandEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            client_id: "viden-gui".to_string(),
            command_id: command_id.to_string(),
            owner,
            command,
        })
    }

    pub fn d10_lane_monitor(&self) -> Option<crate::D10LaneMonitorProjection> {
        self.projection.d10_lane_monitor()
    }

    pub fn d2_decisions(&self) -> Option<crate::D2DecisionsProjection> {
        self.projection.d2_decisions(self.d2_selected.as_deref())
    }

    /// Projects the queue with an explicit selection without persisting it.
    pub fn d2_decisions_for(&self, id: &str) -> Option<crate::D2DecisionsProjection> {
        self.projection.d2_decisions(Some(id))
    }

    /// Sends one D2 decision without waiting on the event stream.
    ///
    /// Kept for the fact families whose confirmation is a projection refresh
    /// rather than an ordered receipt; a review verdict polls with a zero
    /// budget here and only drains what Core already delivered.
    pub fn d2_send_intent(
        &mut self,
        command_id: &str,
        intent: crate::D2Intent,
    ) -> Result<crate::D2IntentResult, String> {
        self.d2_send_intent_and_wait(command_id, intent, Duration::ZERO)
    }

    /// Sends one D2 decision. Selection is local; every real decision travels
    /// as the Core command that owns that fact family.
    ///
    /// `event_timeout` bounds the wait for the ordered Core receipt that
    /// settles a review verdict. Approval and contract decisions are unaffected
    /// by it: they report their own outcome through the permission dock and the
    /// republished projection.
    pub fn d2_send_intent_and_wait(
        &mut self,
        command_id: &str,
        intent: crate::D2Intent,
        event_timeout: Duration,
    ) -> Result<crate::D2IntentResult, String> {
        let outcome = match intent {
            crate::D2Intent::Select { id } => {
                self.d2_selected = Some(id);
                PermissionOutcomeProjection::idle()
            }
            crate::D2Intent::RespondApproval {
                request_id,
                choice,
                feedback,
            } => {
                let envelope = self.permission_envelope(
                    command_id,
                    PermissionIntent::Respond {
                        request_id: request_id.clone(),
                        choice,
                        feedback,
                    },
                )?;
                self.client.send(envelope).map_err(|e| e.to_string())?;
                self.d2_selected = Some(request_id);
                PermissionOutcomeProjection::pending()
            }
            crate::D2Intent::DecideContract {
                contract_id,
                accept,
            } => {
                let envelope = self.d2_contract_envelope(command_id, &contract_id, accept)?;
                self.client.send(envelope).map_err(|e| e.to_string())?;
                self.d2_selected = Some(contract_id);
                PermissionOutcomeProjection::pending()
            }
            crate::D2Intent::DecideReview {
                review_id,
                accept,
                feedback,
            } => {
                if let Some(pending) = &self.pending_d2_review {
                    return Err(format!(
                        "review command `{}` is still pending",
                        pending.command_id
                    ));
                }
                let expected_status = if accept {
                    ReviewRequestStatus::Accepted
                } else {
                    ReviewRequestStatus::Rejected
                };
                let envelope =
                    self.d2_review_envelope(command_id, &review_id, accept, feedback.as_deref())?;
                self.pending_d2_review = Some(PendingD2Review {
                    command_id: command_id.to_string(),
                    review_id: review_id.clone(),
                    expected_status,
                    accepted: false,
                });
                self.client.send(envelope).map_err(|error| {
                    self.pending_d2_review = None;
                    error.to_string()
                })?;
                self.d2_selected = Some(review_id);
                self.poll_d2_review(event_timeout)?
            }
        };
        let projection = self
            .d2_decisions()
            .ok_or_else(|| "Core has not published a decision projection".to_string())?;
        Ok(crate::D2IntentResult {
            projection,
            pending_command_id: match outcome.state {
                "pending" => Some(command_id.to_string()),
                _ => None,
            },
            outcome,
        })
    }

    /// Drains ordered Core events for an in-flight review verdict without
    /// sending a command or creating GUI-owned runtime state.
    fn poll_d2_review(
        &mut self,
        event_timeout: Duration,
    ) -> Result<PermissionOutcomeProjection, String> {
        let mut received = false;
        let mut outcome = PermissionOutcomeProjection::pending();
        for _ in 0..8 {
            let event = match self
                .receive_event_until(if received {
                    Duration::ZERO
                } else {
                    event_timeout
                })
                .map_err(|error| error.to_string())?
            {
                Some(event) => event,
                None => break,
            };
            received = true;
            let Some(pending) = self.pending_d2_review.as_mut() else {
                break;
            };
            match pending.observe(&event) {
                D12Observation::Continue => {}
                D12Observation::Confirmed => {
                    self.pending_d2_review = None;
                    outcome = PermissionOutcomeProjection::confirmed();
                    break;
                }
                D12Observation::Rejected(reason) => {
                    self.pending_d2_review = None;
                    outcome = PermissionOutcomeProjection::rejected(reason);
                    break;
                }
            }
        }
        if received {
            self.refresh_projection()
                .map_err(|error| error.to_string())?;
        }
        Ok(outcome)
    }

    /// Replays the review identity Core recorded and builds the verdict command.
    ///
    /// Everything here is re-resolved against the current Core view: a review
    /// that vanished, was already settled, or carries no actor
    /// `validate_review_decider` would accept is refused locally rather than
    /// sent as a command Core would have to reject.
    fn d2_review_envelope(
        &self,
        command_id: &str,
        review_id: &str,
        accept: bool,
        feedback: Option<&str>,
    ) -> Result<RuntimeCommandEnvelope, String> {
        let view = self
            .projection
            .view()
            .ok_or_else(|| "Core has not published a decision projection".to_string())?;
        if view.snapshot.work_mode == WorkMode::Plan {
            return Err("Plan mode blocks review decisions".to_string());
        }
        let review = view
            .review_requests
            .iter()
            .find(|entry| entry.review_id == review_id)
            .ok_or_else(|| format!("review `{review_id}` is not published by Core"))?;
        if review.status != ReviewRequestStatus::Pending {
            return Err(format!("review `{review_id}` is already decided"));
        }
        let actor = crate::projection::d2_review_actor(view, review).ok_or_else(|| {
            format!("review `{review_id}` has no independent reviewer identity this client may decide as")
        })?;
        let feedback = normalize_review_feedback(feedback)?;
        Ok(RuntimeCommandEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            client_id: "viden-gui".to_string(),
            command_id: command_id.to_string(),
            owner: actor.clone(),
            command: decide_review_command(review_id, accept, feedback, actor)?,
        })
    }

    /// Replays the contract identity Core recorded. The GUI never rebuilds an
    /// owner or task id from display text.
    fn d2_contract_envelope(
        &self,
        command_id: &str,
        contract_id: &str,
        accept: bool,
    ) -> Result<RuntimeCommandEnvelope, String> {
        let view = self
            .projection
            .view()
            .ok_or_else(|| "Core has not published a decision projection".to_string())?;
        if view.snapshot.work_mode == WorkMode::Plan {
            return Err("Plan mode blocks contract decisions".to_string());
        }
        let contract = view
            .contracts
            .iter()
            .find(|entry| entry.contract_id == contract_id)
            .ok_or_else(|| format!("contract `{contract_id}` is not published by Core"))?;
        Ok(RuntimeCommandEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            client_id: "viden-gui".to_string(),
            command_id: command_id.to_string(),
            owner: contract.owner.clone(),
            command: RuntimeCommand::ConfirmContract {
                contract_id: contract.contract_id.clone(),
                task_id: contract.task_id.clone(),
                owner: contract.owner.clone(),
                summary: contract.summary.clone(),
                decision: if accept {
                    viden_core::ContractDecision::Confirmed
                } else {
                    viden_core::ContractDecision::Rejected
                },
            },
        })
    }

    pub fn d6_recovery(&self) -> D6RecoveryProjection {
        self.projection
            .d6_recovery(
                self.connection,
                self.connection_detail.as_deref(),
                self.supports("runtime.approvals"),
            )
            .unwrap_or(D6RecoveryProjection {
                connection: self.connection,
                state: match self.connection {
                    D6ConnectionState::Connecting => crate::D6State::Connecting,
                    D6ConnectionState::Incompatible => crate::D6State::IncompatibleSchema,
                    D6ConnectionState::Recovering => crate::D6State::EventGap,
                    _ => crate::D6State::Disconnected,
                },
                detail: self.connection_detail.clone(),
                hint: None,
                recoverable: false,
                business_success_blocked: true,
                used_tokens: None,
                hard_token_limit: None,
                missing_capabilities: Vec::new(),
                actions: Vec::new(),
            })
    }

    /// Sends one D6 recovery action as the Core command that owns that fact
    /// family, then republishes the recovery projection.
    ///
    /// The target ids come from the projection the operator acted on, and are
    /// re-resolved against the current Core view here: a session or Lane that
    /// disappeared between render and click is rejected instead of being sent
    /// as a command Core would have to reject.
    pub fn send_d6_intent_and_wait(
        &mut self,
        command_id: &str,
        intent: crate::D6Intent,
        event_timeout: Duration,
    ) -> Result<crate::D6IntentResult, String> {
        let envelope = self.d6_envelope(command_id, &intent)?;
        self.client
            .send(envelope)
            .map_err(|error| error.to_string())?;
        // Drain the receipts Core already published so the returned projection
        // is not a turn stale; the ordered-event pump delivers the rest.
        self.pump_events(event_timeout);
        Ok(crate::D6IntentResult {
            projection: self.d6_recovery(),
            pending_command_id: Some(command_id.to_string()),
        })
    }

    fn d6_envelope(
        &self,
        command_id: &str,
        intent: &crate::D6Intent,
    ) -> Result<RuntimeCommandEnvelope, String> {
        let view = self
            .projection
            .view()
            .ok_or_else(|| "Core has not published a recovery projection".to_string())?;
        let (owner, command) = match intent {
            crate::D6Intent::Restart { session_id } => {
                let sessions = view
                    .agent_sessions
                    .iter()
                    .filter(|session| session.session_id == *session_id)
                    .collect::<Vec<_>>();
                let [session] = sessions.as_slice() else {
                    return Err(format!(
                        "Core has no unique ACP session `{session_id}` to restart"
                    ));
                };
                if !matches!(
                    session.status,
                    AgentSessionStatus::Failed | AgentSessionStatus::Cancelled
                ) {
                    return Err(format!("ACP session `{session_id}` is not retryable"));
                }
                (
                    session.owner.clone(),
                    RuntimeCommand::RetryAgentSession {
                        session_id: session_id.clone(),
                    },
                )
            }
            crate::D6Intent::CloseLane { lane_id } => {
                let lane = view
                    .lanes
                    .iter()
                    .find(|lane| lane.id == *lane_id)
                    .ok_or_else(|| format!("Core has no Lane `{lane_id}` to close"))?;
                if !lane.is_active() {
                    return Err(format!("Lane `{lane_id}` is not active"));
                }
                // Lane lifecycle is routed by the command's own lane id; the
                // owner still names the Lane so the audit trail keeps the
                // exact execution identity.
                (
                    self.exact_runtime_owner(lane_id).unwrap_or(RuntimeOwner {
                        lane_id: Some(lane_id.clone()),
                        ..RuntimeOwner::default()
                    }),
                    RuntimeCommand::StopLane {
                        lane_id: lane_id.clone(),
                    },
                )
            }
        };
        Ok(RuntimeCommandEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            client_id: "viden-gui".to_string(),
            command_id: command_id.to_string(),
            owner,
            command,
        })
    }

    pub fn recover(&mut self) -> Result<(), CoreClientError> {
        self.connection = D6ConnectionState::Recovering;
        let snapshot = match self.client.snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.record_connection_error(&error);
                return Err(error);
            }
        };
        self.publish(snapshot);
        self.connection = D6ConnectionState::Live;
        self.connection_detail = None;
        Ok(())
    }

    pub fn send_permission_intent(
        &mut self,
        command_id: &str,
        intent: PermissionIntent,
    ) -> Result<(), String> {
        let envelope = self.permission_envelope(command_id, intent)?;
        self.client
            .send(envelope)
            .map_err(|error| error.to_string())
    }

    pub fn send_permission_intent_and_wait(
        &mut self,
        command_id: &str,
        intent: PermissionIntent,
        event_timeout: Duration,
    ) -> Result<PermissionIntentResult, String> {
        if let Some(pending) = &self.pending_permission {
            return Err(format!(
                "permission command `{}` is still pending",
                pending.command_id
            ));
        }
        let envelope = self.permission_envelope(command_id, intent)?;
        let RuntimeCommand::RespondToApproval { request_id, .. } = &envelope.command else {
            return Err("permission dock produced a non-approval command".to_string());
        };
        let request = self
            .projection
            .view()
            .and_then(|view| {
                view.pending_approvals
                    .iter()
                    .find(|item| item.id == *request_id)
            })
            .ok_or_else(|| format!("approval `{request_id}` is no longer pending"))?;
        self.pending_permission = Some(PendingPermission {
            command_id: command_id.to_string(),
            request_id: request_id.clone(),
            owner: request.owner.clone(),
            audit_id: request.audit_id.clone(),
        });
        self.client.send(envelope).map_err(|error| {
            self.pending_permission = None;
            error.to_string()
        })?;
        self.permission_outcome = PermissionOutcomeProjection::pending();
        self.poll_permission(event_timeout)
    }

    pub fn poll_permission(
        &mut self,
        event_timeout: Duration,
    ) -> Result<PermissionIntentResult, String> {
        let mut received = false;
        for _ in 0..8 {
            let event = match self
                .receive_event_until(event_timeout)
                .map_err(|error| error.to_string())?
            {
                Some(event) => event,
                None => break,
            };
            received = true;
            let permission_complete = self.observe_pending_permission(&event);
            self.observe_pending(&event);
            self.observe_d4(&event);
            if permission_complete {
                break;
            }
        }
        if received {
            self.refresh_projection()
                .map_err(|error| error.to_string())?;
        }
        Ok(PermissionIntentResult {
            projection: self
                .permission_dock()
                .ok_or_else(|| "Core has not published a permission projection".to_string())?,
            pending_command_id: self
                .pending_permission
                .as_ref()
                .map(|pending| pending.command_id.clone()),
            outcome: self.permission_outcome.clone(),
        })
    }

    fn permission_envelope(
        &self,
        command_id: &str,
        intent: PermissionIntent,
    ) -> Result<RuntimeCommandEnvelope, String> {
        let PermissionIntent::Respond {
            request_id,
            choice,
            feedback,
        } = intent;
        let view = self
            .projection
            .view()
            .ok_or_else(|| "Core has not published a permission projection".to_string())?;
        let request = view
            .pending_approvals
            .iter()
            .find(|request| request.id == request_id)
            .ok_or_else(|| format!("approval `{request_id}` is no longer pending"))?;
        if request.is_mutating && view.snapshot.work_mode == WorkMode::Plan {
            return Err("Plan mode blocks mutating approval responses".to_string());
        }
        let exact_scope = |kind: PermissionChoice| {
            let scopes = request
                .allowed_scopes
                .iter()
                .filter(|scope| {
                    matches!(
                        (kind, *scope),
                        (PermissionChoice::Once, ApprovalScope::Once)
                            | (PermissionChoice::Session, ApprovalScope::Session { .. })
                            | (
                                PermissionChoice::RepoAllowlist,
                                ApprovalScope::RepoAllowlist { .. }
                            )
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            match scopes.as_slice() {
                [scope] => Ok(scope.clone()),
                _ => Err("the selected approval scope is unavailable or ambiguous".to_string()),
            }
        };
        let decision = match choice {
            PermissionChoice::Once
            | PermissionChoice::Session
            | PermissionChoice::RepoAllowlist => ApprovalDecision::Allow {
                scope: exact_scope(choice)?,
            },
            PermissionChoice::Deny => ApprovalDecision::Deny,
            PermissionChoice::Always | PermissionChoice::Edit => {
                return Err("GUI-CORE-003: typed approval action is unavailable".to_string());
            }
        };
        Ok(RuntimeCommandEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            client_id: "viden-gui".to_string(),
            command_id: command_id.to_string(),
            owner: request.owner.clone(),
            command: RuntimeCommand::RespondToApproval {
                request_id,
                response: ApprovalResponse { decision, feedback },
            },
        })
    }

    /// Sends the single D1 composer/cancel intent through Core's shared path.
    pub fn send_d1_intent(&mut self, command_id: &str, intent: D1Intent) -> Result<(), String> {
        let (envelope, _) = self.d1_envelope(command_id, intent)?;
        self.client
            .send(envelope)
            .map_err(|error| error.to_string())
    }

    fn d1_envelope(
        &self,
        command_id: &str,
        intent: D1Intent,
    ) -> Result<(RuntimeCommandEnvelope, PendingD1Kind), String> {
        let (owner, command, kind) = match intent {
            D1Intent::Submit { lane_id, content } => {
                if content.trim().is_empty() {
                    return Err("D1 composer content is empty".to_string());
                }
                let projection = self
                    .d1_cockpit(Some(&lane_id))
                    .ok_or_else(|| "Core has not published a D1 projection".to_string())?;
                if projection.selected_lane_id.as_deref() != Some(lane_id.as_str()) {
                    return Err(format!("Core has no Lane `{lane_id}`"));
                }
                let owner = self.exact_submit_owner(&lane_id)?;
                let command = if projection.composer.busy {
                    RuntimeCommand::QueueFollowUp { content }
                } else {
                    RuntimeCommand::SubmitUserInput { content }
                };
                let kind = if matches!(command, RuntimeCommand::QueueFollowUp { .. }) {
                    PendingD1Kind::Queue
                } else {
                    PendingD1Kind::Submit
                };
                (owner, command, kind)
            }
            D1Intent::Cancel { lane_id } => {
                if !self.supports(D1_OWNER_CAPABILITY) {
                    return Err(format!("missing Core capability `{D1_OWNER_CAPABILITY}`"));
                }
                let view = self
                    .projection
                    .view()
                    .ok_or_else(|| "Core has not published a D1 projection".to_string())?;
                let active = view
                    .lanes
                    .iter()
                    .find(|lane| lane.id == lane_id)
                    .is_some_and(|lane| lane.is_active());
                if !active {
                    return Err(format!("Lane `{lane_id}` is not active"));
                }
                let owner = self.exact_runtime_owner(&lane_id)?;
                (
                    owner,
                    RuntimeCommand::CancelActiveTurn,
                    PendingD1Kind::Cancel,
                )
            }
            D1Intent::QueryAgentAdapters => (
                RuntimeOwner::default(),
                RuntimeCommand::QueryAgentAdapters,
                PendingD1Kind::QueryAgentAdapters,
            ),
            D1Intent::ProbeAgentAdapter { agent_id } => (
                RuntimeOwner::default(),
                RuntimeCommand::ProbeAgentAdapter {
                    agent_id: agent_id.clone(),
                },
                PendingD1Kind::ProbeAgentAdapter { agent_id },
            ),
            D1Intent::PreviewDefaultLane { preset } => {
                let preset = parse_starter_preset(&preset)?;
                (
                    RuntimeOwner::default(),
                    RuntimeCommand::PreviewDefaultStarterLane { preset },
                    PendingD1Kind::PreviewDefaultLane,
                )
            }
            D1Intent::CreateStarterLane {
                lane_id,
                preset,
                branch,
                preview_id,
                content_sha256,
            } => {
                let preset = parse_starter_preset(&preset)?;
                let preview = self
                    .projection
                    .view()
                    .and_then(|view| {
                        view.starter_lane_previews.iter().find(|preview| {
                            preview.preview_id == preview_id
                                && preview.content_sha256 == content_sha256
                                && preview.lane.id == lane_id
                                && preview.lane.branch == branch
                        })
                    })
                    .ok_or_else(|| "Core has not published the exact Lane preview".to_string())?;
                (
                    preview.owner.clone(),
                    RuntimeCommand::CreateStarterLane {
                        request: StarterLaneRequest {
                            lane_id,
                            preset,
                            branch,
                            worktree_path: None,
                        },
                        preview_id: preview_id.clone(),
                        content_sha256: content_sha256.clone(),
                    },
                    PendingD1Kind::CreateStarterLane {
                        preview_id,
                        content_sha256,
                    },
                )
            }
            D1Intent::StartAgentSession {
                lane_id,
                agent_id,
                model,
                task,
            } => {
                if task.trim().is_empty() {
                    return Err("ACP task is empty".to_string());
                }
                let view = self
                    .projection
                    .view()
                    .ok_or_else(|| "Core has not published a D1 projection".to_string())?;
                if !view.lanes.iter().any(|lane| lane.id == lane_id) {
                    return Err(format!("Core has no Lane `{lane_id}`"));
                }
                let ready = view.agent_adapters.iter().any(|adapter| {
                    adapter.agent_id == agent_id
                        && adapter.route == viden_core::AgentRoute::Acp
                        && adapter.startability == viden_core::AgentStartability::Ready
                });
                if !ready {
                    return Err(format!("ACP agent `{agent_id}` is not startable"));
                }
                let matching_sessions_before = view
                    .agent_sessions
                    .iter()
                    .filter(|session| session.lane_id == lane_id && session.agent_id == agent_id)
                    .count();
                (
                    RuntimeOwner {
                        lane_id: Some(lane_id.clone()),
                        ..RuntimeOwner::default()
                    },
                    RuntimeCommand::StartAgentSession {
                        request: AgentSessionRequest {
                            lane_id: lane_id.clone(),
                            agent_id: agent_id.clone(),
                            model,
                            load_session_id: None,
                            task,
                        },
                    },
                    PendingD1Kind::StartAgentSession {
                        lane_id,
                        agent_id,
                        matching_sessions_before,
                    },
                )
            }
            D1Intent::SendAgentSessionInput {
                lane_id,
                session_id,
                content,
            } => {
                if content.trim().is_empty() {
                    return Err("ACP input is empty".to_string());
                }
                let session = self.exact_agent_session(&lane_id, &session_id)?;
                (
                    session.owner.clone(),
                    RuntimeCommand::SendAgentSessionInput {
                        input: AgentSessionInput {
                            session_id: session_id.clone(),
                            content,
                        },
                    },
                    PendingD1Kind::SendAgentSessionInput { session_id },
                )
            }
            D1Intent::RetryAgentSession {
                lane_id,
                session_id,
            } => {
                let session = self.exact_agent_session(&lane_id, &session_id)?;
                if !matches!(
                    session.status,
                    AgentSessionStatus::Failed | AgentSessionStatus::Cancelled
                ) {
                    return Err(format!("ACP session `{session_id}` is not retryable"));
                }
                (
                    session.owner.clone(),
                    RuntimeCommand::RetryAgentSession {
                        session_id: session_id.clone(),
                    },
                    PendingD1Kind::RetryAgentSession { session_id },
                )
            }
            D1Intent::CancelAgentSession {
                lane_id,
                session_id,
            } => {
                let session = self.exact_agent_session(&lane_id, &session_id)?;
                (
                    session.owner.clone(),
                    RuntimeCommand::CancelAgentSession {
                        session_id: session_id.clone(),
                    },
                    PendingD1Kind::CancelAgentSession { session_id },
                )
            }
        };
        Ok((
            RuntimeCommandEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                client_id: "viden-gui".to_string(),
                command_id: command_id.to_string(),
                owner,
                command,
            },
            kind,
        ))
    }

    pub fn send_d1_intent_and_wait(
        &mut self,
        command_id: &str,
        intent: D1Intent,
        event_timeout: Duration,
    ) -> Result<D1IntentResult, String> {
        if let Some(pending) = &self.pending_d1 {
            return Err(format!(
                "D1 command `{}` is still pending",
                pending.command_id
            ));
        }
        let (envelope, kind) = self.d1_envelope(command_id, intent)?;
        let owner = envelope.owner.clone();
        let selected_lane_id = owner.lane_id.clone();
        self.client
            .send(envelope)
            .map_err(|error| error.to_string())?;
        self.pending_d1 = Some(PendingD1 {
            command_id: command_id.to_string(),
            owner,
            kind,
            accepted: false,
        });
        self.d1_outcome = D1OutcomeProjection::pending();
        self.poll_d1(selected_lane_id.as_deref(), event_timeout)
    }

    pub fn poll_d1(
        &mut self,
        selected_lane_id: Option<&str>,
        event_timeout: Duration,
    ) -> Result<D1IntentResult, String> {
        let mut received = false;
        let mut receive_failed = false;
        for _ in 0..8 {
            let event = match self.receive_event_until(event_timeout) {
                Ok(Some(event)) => event,
                Ok(None) => break,
                Err(_) => {
                    // The transport state is already classified by receive_event.
                    // Return the last canonical snapshot with D6 recovery facts attached.
                    receive_failed = true;
                    break;
                }
            };
            received = true;
            self.observe_pending_permission(&event);
            let observation = self
                .pending_d1
                .as_mut()
                .map_or(D1Observation::Continue, |pending| pending.observe(&event));
            match observation {
                D1Observation::Continue => {}
                D1Observation::Confirmed => {
                    self.pending_d1 = None;
                    self.d1_outcome = D1OutcomeProjection::confirmed();
                    break;
                }
                D1Observation::Rejected(reason) => {
                    self.pending_d1 = None;
                    self.d1_outcome = D1OutcomeProjection::rejected(reason);
                    break;
                }
            }
        }
        if received && !receive_failed {
            self.refresh_projection()
                .map_err(|error| error.to_string())?;
        }
        let pending_satisfied = self
            .pending_d1
            .as_ref()
            .zip(self.projection.view())
            .is_some_and(|(pending, view)| {
                pending.is_satisfied_by_agent_sessions(&view.agent_sessions)
            });
        if pending_satisfied {
            // The reducer can publish the typed session before this adapter
            // observes the matching business event. Reconcile the accepted
            // command from Core state so a completed start never blocks the
            // next composer turn until the desktop process restarts.
            self.pending_d1 = None;
            self.d1_outcome = D1OutcomeProjection::confirmed();
        }
        let selected = selected_lane_id.or_else(|| {
            self.pending_d1
                .as_ref()
                .and_then(|pending| pending.owner.lane_id.as_deref())
        });
        Ok(D1IntentResult {
            projection: self
                .d1_cockpit(selected)
                .ok_or_else(|| "Core has not published a D1 projection".to_string())?,
            pending_command_id: self
                .pending_d1
                .as_ref()
                .map(|pending| pending.command_id.clone()),
            outcome: self.d1_outcome.clone(),
        })
    }

    /// Sends one composer control (work mode, permission level, model) as its
    /// Core command and waits for the receipt through the shared D1 pending
    /// pipeline.
    ///
    /// The GUI never applies Core's work-mode/permission coupling rule
    /// locally: the returned projection is re-read from the snapshot Core
    /// republished, so coupled changes (for example Plan forcing ReadOnly)
    /// arrive as Core truth, never as an optimistic client edit.
    pub fn send_composer_control_and_wait(
        &mut self,
        command_id: &str,
        intent: ComposerControlIntent,
        selected_lane_id: Option<&str>,
        event_timeout: Duration,
    ) -> Result<D1IntentResult, String> {
        if let Some(pending) = &self.pending_d1 {
            return Err(format!(
                "D1 command `{}` is still pending",
                pending.command_id
            ));
        }
        let (envelope, kind) = self.composer_control_envelope(command_id, &intent)?;
        let owner = envelope.owner.clone();
        self.client
            .send(envelope)
            .map_err(|error| error.to_string())?;
        self.pending_d1 = Some(PendingD1 {
            command_id: command_id.to_string(),
            owner,
            kind,
            accepted: false,
        });
        self.d1_outcome = D1OutcomeProjection::pending();
        self.poll_d1(selected_lane_id, event_timeout)
    }

    fn composer_control_envelope(
        &self,
        command_id: &str,
        intent: &ComposerControlIntent,
    ) -> Result<(RuntimeCommandEnvelope, PendingD1Kind), String> {
        let view = self
            .projection
            .view()
            .ok_or_else(|| "Core has not published a D1 projection".to_string())?;
        let (command, kind) = match intent {
            ComposerControlIntent::SetWorkMode { mode } => {
                let mode = WorkMode::parse_cli(mode)
                    .ok_or_else(|| format!("unknown work mode `{mode}`"))?;
                (
                    RuntimeCommand::SetWorkMode { mode },
                    PendingD1Kind::SetWorkMode,
                )
            }
            ComposerControlIntent::SetPermissionLevel { level } => {
                let level = PermissionLevel::parse_cli(level)
                    .ok_or_else(|| format!("unknown permission level `{level}`"))?;
                (
                    RuntimeCommand::SetPermissionLevel { level },
                    PendingD1Kind::SetPermissionLevel,
                )
            }
            ComposerControlIntent::SelectModel { provider_id, model } => {
                // Re-resolve the option against the current Core view: the
                // selector's options are the provider's current model and the
                // adapter model lists Core published, so anything else is a
                // stale or fabricated target and fails closed here instead of
                // being sent as a command Core would have to reject.
                let provider_published = view.provider.as_ref().is_some_and(|provider| {
                    provider.provider_id == *provider_id && provider.model == *model
                });
                let adapter_published = view.agent_adapters.iter().any(|adapter| {
                    adapter.agent_id == *provider_id
                        && adapter.models.iter().any(|candidate| candidate == model)
                });
                if !provider_published && !adapter_published {
                    return Err(format!(
                        "Core has not published model `{model}` for `{provider_id}`"
                    ));
                }
                (
                    RuntimeCommand::SelectModel {
                        provider_id: provider_id.clone(),
                        model: model.clone(),
                    },
                    PendingD1Kind::SelectModel,
                )
            }
        };
        Ok((
            RuntimeCommandEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                client_id: "viden-gui".to_string(),
                command_id: command_id.to_string(),
                // Composer controls mutate workspace-level runtime state, not
                // one Lane's execution, so they carry the default owner like
                // the adapter-discovery commands.
                owner: RuntimeOwner::default(),
                command,
            },
            kind,
        ))
    }

    /// Whether Core's handshake published preference persistence.
    ///
    /// The frontend reads this to render an honest unavailable Settings panel
    /// instead of an enabled-and-inert Save button, and never to substitute a
    /// client-side write.
    pub fn supports_ui_preference_persistence(&self) -> bool {
        self.supports(UI_PREFERENCE_PERSISTENCE_CAPABILITY)
    }

    /// Sends one preference mutation and waits for the ordered Core receipt.
    ///
    /// Unknown wire values and an empty patch fail closed before anything is
    /// sent. Core owns the skin/mode compatibility rule and the mode gate, so
    /// an invalid pair and a Plan/Review/Explore denial both come back as a
    /// Core rejection carrying Core's own reason.
    pub fn send_preference_intent_and_wait(
        &mut self,
        command_id: &str,
        intent: PreferenceIntent,
        event_timeout: Duration,
    ) -> Result<PreferenceIntentResult, String> {
        if !self.supports_ui_preference_persistence() {
            return Err(format!(
                "missing Core capability `{UI_PREFERENCE_PERSISTENCE_CAPABILITY}`"
            ));
        }
        if let Some(pending) = &self.pending_preference {
            return Err(format!(
                "preference command `{}` is still pending",
                pending.command_id
            ));
        }
        let command = match intent {
            PreferenceIntent::Save { patch } => {
                let patch = typed_preference_patch(&patch)?;
                if patch_is_empty(&patch) {
                    return Err("no preference change to save".to_string());
                }
                RuntimeCommand::SetUiPreferences { patch }
            }
            PreferenceIntent::Restore => RuntimeCommand::ResetUiPreferences,
        };
        self.client
            .send(RuntimeCommandEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                client_id: "viden-gui".to_string(),
                command_id: command_id.to_string(),
                // Personal preferences are user-scoped, not Lane-scoped.
                owner: RuntimeOwner::default(),
                command: command.clone(),
            })
            .map_err(|error| error.to_string())?;
        self.pending_preference = Some(PendingPreference {
            command_id: command_id.to_string(),
            command,
            accepted: false,
        });
        self.preference_outcome = D1OutcomeProjection::pending();
        self.preference_receipt = None;
        self.poll_preferences(event_timeout)
    }

    /// Drains ordered Core events for the in-flight preference command.
    pub fn poll_preferences(
        &mut self,
        event_timeout: Duration,
    ) -> Result<PreferenceIntentResult, String> {
        let mut received = false;
        let mut receive_failed = false;
        for _ in 0..8 {
            let event = match self.receive_event_until(event_timeout) {
                Ok(Some(event)) => event,
                Ok(None) => break,
                Err(_) => {
                    // The transport state is already classified by receive_event.
                    receive_failed = true;
                    break;
                }
            };
            received = true;
            if self.observe_pending_preference(&event) {
                break;
            }
        }
        if received && !receive_failed {
            self.refresh_projection()
                .map_err(|error| error.to_string())?;
        }
        Ok(self.preference_result())
    }

    /// Reconciles one ordered event against the in-flight preference command.
    ///
    /// Returns whether the command reached a terminal outcome. The desktop
    /// event pump calls this too, so a background drain can never swallow the
    /// only receipt the panel is waiting for.
    fn observe_pending_preference(&mut self, event: &RuntimeEventEnvelope) -> bool {
        let observation = self
            .pending_preference
            .as_mut()
            .map_or(PreferenceObservation::Continue, |pending| {
                pending.observe(event)
            });
        match observation {
            PreferenceObservation::Continue => false,
            PreferenceObservation::Confirmed => {
                self.pending_preference = None;
                self.preference_outcome = D1OutcomeProjection::confirmed();
                // The confirming fact is the authority for what was stored and
                // what Core had to correct; nothing is recomputed locally.
                if let RuntimeWireEvent::Known(known) = &event.event
                    && let RuntimeEventKind::UiPreferencesUpdated {
                        resolved,
                        persisted,
                        diagnostics,
                    } = &known.kind
                {
                    self.preference_receipt = Some(PreferenceReceipt {
                        preferences: crate::projection::resolved_preferences_projection(resolved),
                        persisted: persisted.is_some(),
                        diagnostics: diagnostics
                            .iter()
                            .map(preference_diagnostic_projection)
                            .collect(),
                    });
                }
                true
            }
            PreferenceObservation::Rejected(reason) => {
                self.pending_preference = None;
                self.preference_outcome = D1OutcomeProjection::rejected(reason);
                self.preference_receipt = None;
                true
            }
        }
    }

    fn preference_result(&self) -> PreferenceIntentResult {
        let receipt = self.preference_receipt.as_ref();
        PreferenceIntentResult {
            outcome: self.preference_outcome.clone(),
            preferences: receipt.map(|receipt| receipt.preferences.clone()),
            persisted: receipt.is_some_and(|receipt| receipt.persisted),
            diagnostics: receipt
                .map(|receipt| receipt.diagnostics.clone())
                .unwrap_or_default(),
            pending_command_id: self
                .pending_preference
                .as_ref()
                .map(|pending| pending.command_id.clone()),
            capability_available: self.supports_ui_preference_persistence(),
        }
    }

    /// Whether Core's handshake published the recent-work inventory.
    ///
    /// Welcome and the project picker read this before they render, so an
    /// absent capability shows as an honest unavailable section instead of an
    /// empty list that looks like "you have no history".
    pub fn supports_recent_work(&self) -> bool {
        self.supports(RECENT_WORK_CAPABILITY)
    }

    /// Sends one `QueryRecentWork` and waits for the ordered Core answer.
    ///
    /// The read is available in Plan mode and never requests approval. `limit`
    /// is passed through untouched: Core owns the `1..=100` clamp, and a client
    /// that pre-clamped would hide a future contract change.
    pub fn query_recent_work_and_wait(
        &mut self,
        command_id: &str,
        limit: u16,
        event_timeout: Duration,
    ) -> Result<RecentWorkResult, String> {
        if !self.supports_recent_work() {
            return Err(format!(
                "missing Core capability `{RECENT_WORK_CAPABILITY}`"
            ));
        }
        if let Some(pending) = &self.pending_recent_work {
            return Err(format!(
                "recent work command `{}` is still pending",
                pending.command_id
            ));
        }
        self.client
            .send(RuntimeCommandEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                client_id: "viden-gui".to_string(),
                command_id: command_id.to_string(),
                // The inventory spans projects, so it is user-scoped rather
                // than bound to the open workspace's Lane owner.
                owner: RuntimeOwner::default(),
                command: RuntimeCommand::QueryRecentWork {
                    query: RecentWorkQuery { limit },
                },
            })
            .map_err(|error| error.to_string())?;
        self.pending_recent_work = Some(PendingRecentWork {
            command_id: command_id.to_string(),
            accepted: false,
        });
        self.recent_work_outcome = D1OutcomeProjection::pending();
        self.recent_work_receipt = None;
        self.poll_recent_work(event_timeout)
    }

    /// Drains ordered Core events for a recent-work read still in flight.
    pub fn poll_recent_work(
        &mut self,
        event_timeout: Duration,
    ) -> Result<RecentWorkResult, String> {
        let mut received = false;
        let mut receive_failed = false;
        for _ in 0..8 {
            let event = match self.receive_event_until(event_timeout) {
                Ok(Some(event)) => event,
                Ok(None) => break,
                Err(_) => {
                    // The transport state is already classified by receive_event.
                    receive_failed = true;
                    break;
                }
            };
            received = true;
            if self.observe_pending_recent_work(&event) {
                break;
            }
        }
        if received && !receive_failed {
            self.refresh_projection()
                .map_err(|error| error.to_string())?;
        }
        Ok(self.recent_work_result())
    }

    /// Reconciles one ordered event against the in-flight recent-work read.
    ///
    /// Returns whether the read reached a terminal outcome. The desktop event
    /// pump calls this too, so a background drain can never swallow the only
    /// answer the picker is waiting for.
    fn observe_pending_recent_work(&mut self, event: &RuntimeEventEnvelope) -> bool {
        let observation = self
            .pending_recent_work
            .as_mut()
            .map_or(RecentWorkObservation::Continue, |pending| {
                pending.observe(event)
            });
        match observation {
            RecentWorkObservation::Continue => false,
            RecentWorkObservation::Confirmed => {
                self.pending_recent_work = None;
                self.recent_work_outcome = D1OutcomeProjection::confirmed();
                // The confirming fact is the authority for the rows and for
                // what Core had to skip; nothing is re-ordered or recomputed.
                if let RuntimeWireEvent::Known(known) = &event.event
                    && let RuntimeEventKind::RecentWorkLoaded {
                        projects,
                        sessions,
                        diagnostics,
                    } = &known.kind
                {
                    self.recent_work_receipt = Some(RecentWorkReceipt {
                        projects: projects.iter().map(recent_project_projection).collect(),
                        sessions: sessions.iter().map(recent_session_projection).collect(),
                        diagnostics: diagnostics.clone(),
                    });
                }
                true
            }
            RecentWorkObservation::Rejected(reason) => {
                self.pending_recent_work = None;
                self.recent_work_outcome = D1OutcomeProjection::rejected(reason);
                self.recent_work_receipt = None;
                true
            }
        }
    }

    fn recent_work_result(&self) -> RecentWorkResult {
        let receipt = self.recent_work_receipt.as_ref();
        RecentWorkResult {
            outcome: self.recent_work_outcome.clone(),
            projects: receipt
                .map(|receipt| receipt.projects.clone())
                .unwrap_or_default(),
            sessions: receipt
                .map(|receipt| receipt.sessions.clone())
                .unwrap_or_default(),
            diagnostics: receipt
                .map(|receipt| receipt.diagnostics.clone())
                .unwrap_or_default(),
            pending_command_id: self
                .pending_recent_work
                .as_ref()
                .map(|pending| pending.command_id.clone()),
            capability_available: self.supports_recent_work(),
        }
    }

    fn observe_pending_permission(&mut self, event: &RuntimeEventEnvelope) -> bool {
        let observation = self
            .pending_permission
            .as_ref()
            .map_or(PermissionObservation::Continue, |pending| {
                pending.observe(event)
            });
        match observation {
            PermissionObservation::Continue => false,
            PermissionObservation::Confirmed => {
                self.pending_permission = None;
                self.permission_outcome = PermissionOutcomeProjection::confirmed();
                true
            }
            PermissionObservation::Rejected(reason) => {
                self.pending_permission = None;
                self.permission_outcome = PermissionOutcomeProjection::rejected(reason);
                true
            }
        }
    }

    fn exact_runtime_owner(&self, lane_id: &str) -> Result<RuntimeOwner, String> {
        self.exact_lane_owner(lane_id, "runtime")
    }

    fn exact_submit_owner(&self, lane_id: &str) -> Result<RuntimeOwner, String> {
        self.exact_lane_owner(lane_id, "submit")
    }

    /// Raw same-Lane binding cardinality is authoritative; malformed duplicates block all routing.
    fn exact_lane_owner(&self, lane_id: &str, purpose: &str) -> Result<RuntimeOwner, String> {
        let view = self
            .projection
            .view()
            .ok_or_else(|| "Core has not published a D1 projection".to_string())?;
        let runtime = view
            .lane_runtime_owners
            .iter()
            .filter(|binding| binding.lane_id == lane_id)
            .collect::<Vec<_>>();
        match runtime.as_slice() {
            [binding] if binding.owner.lane_id.as_deref() == Some(lane_id) => {
                return Ok(binding.owner.clone());
            }
            _ => Err(format!(
                "Lane `{lane_id}` does not have one exact Core {purpose} owner"
            )),
        }
    }

    fn exact_agent_session(
        &self,
        lane_id: &str,
        session_id: &str,
    ) -> Result<&viden_core::AgentSessionView, String> {
        let view = self
            .projection
            .view()
            .ok_or_else(|| "Core has not published a D1 projection".to_string())?;
        let sessions = view
            .agent_sessions
            .iter()
            .filter(|session| session.lane_id == lane_id && session.session_id == session_id)
            .collect::<Vec<_>>();
        let [session] = sessions.as_slice() else {
            return Err(format!(
                "Core has no unique ACP session `{session_id}` for Lane `{lane_id}`"
            ));
        };
        let bindings = view
            .lane_runtime_owners
            .iter()
            .filter(|binding| binding.lane_id == lane_id)
            .collect::<Vec<_>>();
        match bindings.as_slice() {
            [binding]
                if binding.owner.lane_id.as_deref() == Some(lane_id)
                    && session.owner == binding.owner => {}
            [] if exact_terminal_agent_session(view, lane_id)
                .is_some_and(|restored| restored.session_id == session_id) => {}
            [binding] if binding.owner.lane_id.as_deref() == Some(lane_id) => {
                return Err(format!(
                    "ACP session `{session_id}` does not match the exact Core owner for Lane `{lane_id}`"
                ));
            }
            _ => {
                return Err(format!(
                    "Lane `{lane_id}` does not have one exact Core ACP owner"
                ));
            }
        }
        Ok(session)
    }

    pub fn send_d11_intent(&mut self, command_id: &str, intent: D11Intent) -> Result<(), String> {
        let envelope = self.d11_envelope(command_id, intent)?;
        self.client
            .send(envelope)
            .map_err(|error| error.to_string())
    }

    fn d11_envelope(
        &self,
        command_id: &str,
        intent: D11Intent,
    ) -> Result<RuntimeCommandEnvelope, String> {
        if matches!(&intent, D11Intent::StoreCredentialHandle { .. }) {
            return Err("GUI-CORE-026: platform credential intake is unavailable".to_string());
        }
        let required_capability = match intent {
            D11Intent::ProbeProject
            | D11Intent::PreviewProjectConfig { .. }
            | D11Intent::ConfirmProjectConfig => "runtime.project_onboarding",
            D11Intent::StoreCredentialHandle { .. } => "runtime.credential_handles",
        };
        if !self.supports(required_capability) {
            return Err(format!("missing Core capability `{required_capability}`"));
        }

        let command = self.d11_command(intent)?;
        let envelope = RuntimeCommandEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            client_id: "viden-gui".to_string(),
            command_id: command_id.to_string(),
            owner: Default::default(),
            command,
        };
        Ok(envelope)
    }

    pub fn send_d11_intent_and_wait(
        &mut self,
        command_id: &str,
        intent: D11Intent,
        event_timeout: Duration,
    ) -> Result<D11IntentResult, String> {
        if let Some(pending) = &self.pending_d11 {
            return Err(format!(
                "D11 command `{}` is still pending",
                pending.command_id
            ));
        }
        let envelope = self.d11_envelope(command_id, intent)?;
        let target = D11CompletionTarget::from_command(&envelope.command)?;
        self.client
            .send(envelope)
            .map_err(|error| error.to_string())?;
        self.pending_d11 = Some(PendingD11 {
            command_id: command_id.to_string(),
            target,
            accepted: false,
            approval_request_id: None,
        });
        // Command acceptance is not business success. Drain the ordered event
        // stream through CoreClient so the returned projection contains the
        // latest project/config/lane fact, rejection, or pending approval.
        self.poll_d11(event_timeout)
    }

    /// Polls Core without sending a command or creating GUI-owned runtime state.
    pub fn poll_d11(&mut self, event_timeout: Duration) -> Result<D11IntentResult, String> {
        let mut received = false;
        for _ in 0..8 {
            let event = match self
                .receive_event_until(event_timeout)
                .map_err(|error| error.to_string())?
            {
                Some(event) => event,
                None => break,
            };
            received = true;
            if self.observe_pending(&event) {
                break;
            }
        }
        if received {
            self.refresh_projection()
                .map_err(|error| error.to_string())?;
        }
        self.d11_result()
    }

    fn d11_result(&self) -> Result<D11IntentResult, String> {
        let mut projection = self
            .projection
            .d11_intake()
            .ok_or_else(|| "Core has not published a D11 projection".to_string())?;
        if let Some(pending) = &self.pending_d11 {
            if let Some(approval_request_id) = &pending.approval_request_id {
                projection.pending_approval =
                    self.projection.d11_approval_by_id(approval_request_id);
            } else {
                projection.pending_approval = None;
            }
        }
        Ok(D11IntentResult {
            projection,
            pending_command_id: self
                .pending_d11
                .as_ref()
                .map(|pending| pending.command_id.clone()),
            pending_intent: self
                .pending_d11
                .as_ref()
                .map(|pending| pending.target.intent_name().to_string()),
        })
    }

    /// Shutdown is intentionally non-mutating: dropping a GUI never sends a runtime command.
    pub fn shutdown(self) {}

    fn publish(&mut self, snapshot: RuntimeSnapshotEnvelope) {
        self.projection.replace(snapshot);
    }

    pub(crate) fn receive_and_publish(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<RuntimeEventEnvelope>, CoreClientError> {
        let event = self.receive_event(timeout)?;
        if event.is_some() {
            self.refresh_projection()?;
        }
        Ok(event)
    }

    pub(crate) fn receive_event(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<RuntimeEventEnvelope>, CoreClientError> {
        let event = match self.client.recv(timeout) {
            Ok(event) => event,
            Err(error) => {
                self.record_connection_error(&error);
                return Err(error);
            }
        };
        if let Some(envelope) = &event {
            self.note_workspace_revision(envelope);
        }
        Ok(event)
    }

    /// Counts the two ordered Core facts that invalidate a structured diff.
    ///
    /// This sits in the single receive funnel on purpose. Every screen's poll
    /// and the background pump all drain through here, so a source or change
    /// fact consumed by an unrelated read still advances the revision and the
    /// open review still learns its page is stale.
    fn note_workspace_revision(&mut self, envelope: &RuntimeEventEnvelope) {
        let RuntimeWireEvent::Known(event) = &envelope.event else {
            return;
        };
        if matches!(
            event.kind,
            RuntimeEventKind::WorkspaceSourceUpdated { .. }
                | RuntimeEventKind::WorkspaceChangeUpdated { .. }
        ) {
            self.workspace_revision = self.workspace_revision.saturating_add(1);
        }
    }

    /// Drains ordered Core events with one bounded wait and refreshes the
    /// projection when anything arrived.
    ///
    /// Returns whether observable state advanced. The desktop event pump uses
    /// this from a background thread to wake the frontend with a push instead
    /// of leaving every screen on its own drain timer; a quiet pump must
    /// therefore report `false` so idle loops stay silent.
    pub fn pump_events(&mut self, event_timeout: Duration) -> bool {
        let mut received = false;
        for _ in 0..16 {
            let event = match self.receive_event_until(if received {
                Duration::ZERO
            } else {
                event_timeout
            }) {
                Ok(Some(event)) => event,
                Ok(None) => break,
                Err(_) => {
                    // The transport state is already classified by
                    // receive_event; a connection change is progress too.
                    return true;
                }
            };
            received = true;
            self.observe_pending_permission(&event);
            self.observe_pending_preference(&event);
            self.observe_pending_recent_work(&event);
            self.observe_pending_audit(&event);
            self.observe_pending_workspace_files(&event);
            self.observe_pending_workspace_diff(&event);
            self.observe_pending(&event);
            self.observe_pending_d12(&event);
            self.observe_d4(&event);
        }
        if received {
            let _ = self.refresh_projection();
        }
        received
    }

    fn receive_event_until(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<RuntimeEventEnvelope>, CoreClientError> {
        if timeout.is_zero() {
            return self.receive_event(timeout);
        }
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            if let Some(event) = self.receive_event(remaining)? {
                return Ok(Some(event));
            }
            // StatefulCoreClient also returns None for duplicate events already
            // covered by its confirmed snapshot. Keep waiting within the same
            // deadline so those stale envelopes cannot hide a later fact.
            if Instant::now() >= deadline {
                return Ok(None);
            }
        }
    }

    pub(crate) fn refresh_projection(&mut self) -> Result<(), CoreClientError> {
        let snapshot = match self.client.snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.record_connection_error(&error);
                return Err(error);
            }
        };
        self.publish(snapshot);
        self.connection = D6ConnectionState::Live;
        self.connection_detail = None;
        Ok(())
    }

    fn record_connection_error(&mut self, error: &CoreClientError) {
        self.connection = match error {
            CoreClientError::Compatibility(_) => D6ConnectionState::Incompatible,
            CoreClientError::Transport(_) => D6ConnectionState::Disconnected,
            CoreClientError::Protocol(_)
            | CoreClientError::SnapshotRequired { .. }
            | CoreClientError::MissingSnapshot
            | CoreClientError::HandshakeRequired => D6ConnectionState::Recovering,
        };
        self.connection_detail = Some(error.to_string());
    }

    fn observe_pending(&mut self, event: &RuntimeEventEnvelope) -> bool {
        let observation = self
            .pending_d11
            .as_mut()
            .map_or(PendingObservation::Continue, |pending| {
                pending.observe(event)
            });
        match observation {
            PendingObservation::Continue => false,
            PendingObservation::ClearedContinue => {
                self.pending_d11 = None;
                false
            }
            PendingObservation::ClearedStop => {
                self.pending_d11 = None;
                true
            }
        }
    }

    fn d11_command(&self, intent: D11Intent) -> Result<RuntimeCommand, String> {
        Ok(match intent {
            D11Intent::ProbeProject => RuntimeCommand::ProbeProject,
            D11Intent::PreviewProjectConfig { contents } => {
                RuntimeCommand::PreviewProjectConfig { contents }
            }
            D11Intent::ConfirmProjectConfig => {
                let preview = self
                    .projection
                    .view()
                    .and_then(|view| view.project_config_preview.as_ref())
                    .filter(|preview| preview.is_valid() && preview.exact_contents.is_some())
                    .ok_or_else(|| "Core has no confirmable project config preview".to_string())?;
                RuntimeCommand::ConfirmProjectConfig {
                    preview_id: preview.preview_id.clone(),
                    content_sha256: preview.content_sha256.clone(),
                }
            }
            D11Intent::StoreCredentialHandle {
                provider_id,
                backend_id,
                credential_request_id,
            } => RuntimeCommand::StoreCredentialHandle {
                provider_id,
                backend_id,
                credential_request_id,
            },
        })
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Translates the wire patch into the typed Core patch, failing closed.
///
/// An unknown wire value is a client bug or a stale build: it fails here, with
/// the offending axis named, instead of travelling to Core as a command Core
/// would have to reject. The skin/mode pair is deliberately *not* validated
/// here — Core owns that rule and rejects an invalid pair itself.
fn typed_preference_patch(input: &PreferencePatchInput) -> Result<UiPreferencePatch, String> {
    fn locale(value: &str) -> Result<LocaleId, String> {
        match value {
            "system" => Ok(LocaleId::System),
            "en" => Ok(LocaleId::En),
            "zh-CN" => Ok(LocaleId::ZhCn),
            _ => Err(format!("unknown locale `{value}`")),
        }
    }
    fn skin(value: &str) -> Result<UiSkin, String> {
        match value {
            "aurora" => Ok(UiSkin::Aurora),
            "ice" => Ok(UiSkin::Ice),
            "mono" => Ok(UiSkin::Mono),
            "amber" => Ok(UiSkin::Amber),
            "phosphor" => Ok(UiSkin::Phosphor),
            _ => Err(format!("unknown skin `{value}`")),
        }
    }
    fn mode(value: &str) -> Result<UiColorMode, String> {
        match value {
            "system" => Ok(UiColorMode::System),
            "dark" => Ok(UiColorMode::Dark),
            "light" => Ok(UiColorMode::Light),
            _ => Err(format!("unknown mode `{value}`")),
        }
    }
    fn density(value: &str) -> Result<UiDensity, String> {
        match value {
            "compact" => Ok(UiDensity::Compact),
            "regular" => Ok(UiDensity::Regular),
            "comfy" => Ok(UiDensity::Comfy),
            _ => Err(format!("unknown density `{value}`")),
        }
    }
    fn motion(value: &str) -> Result<UiMotion, String> {
        match value {
            "system" => Ok(UiMotion::System),
            "reduced" => Ok(UiMotion::Reduced),
            "full" => Ok(UiMotion::Full),
            _ => Err(format!("unknown motion `{value}`")),
        }
    }
    Ok(UiPreferencePatch {
        locale: input.locale.as_deref().map(locale).transpose()?,
        skin: input.skin.as_deref().map(skin).transpose()?,
        mode: input.mode.as_deref().map(mode).transpose()?,
        density: input.density.as_deref().map(density).transpose()?,
        motion: input.motion.as_deref().map(motion).transpose()?,
    })
}

fn patch_is_empty(patch: &UiPreferencePatch) -> bool {
    patch.locale.is_none()
        && patch.skin.is_none()
        && patch.mode.is_none()
        && patch.density.is_none()
        && patch.motion.is_none()
}

/// Whether an ordered `UiPreferencesUpdated` fact is the receipt for this
/// command, following the TUI reconciliation
/// (`apps/tui/src/tui/preferences.rs`): a Set is confirmed by a persisted
/// table that carries every value it asked for, and a Reset is confirmed by
/// the absence of a persisted table.
fn update_matches(command: &RuntimeCommand, persisted: Option<&UiPreferences>) -> bool {
    match command {
        RuntimeCommand::SetUiPreferences { patch } => persisted.is_some_and(|persisted| {
            patch.locale.is_none_or(|value| persisted.locale == value)
                && patch.skin.is_none_or(|value| persisted.skin == value)
                && patch.mode.is_none_or(|value| persisted.mode == value)
                && patch.density.is_none_or(|value| persisted.density == value)
                && patch.motion.is_none_or(|value| persisted.motion == value)
        }),
        RuntimeCommand::ResetUiPreferences => persisted.is_none(),
        _ => false,
    }
}

fn parse_starter_preset(value: &str) -> Result<StarterLanePreset, String> {
    match value {
        "coder" => Ok(StarterLanePreset::Coder),
        "reviewer" => Ok(StarterLanePreset::Reviewer),
        "tester" => Ok(StarterLanePreset::Tester),
        _ => Err(format!("unsupported starter Lane preset `{value}`")),
    }
}

fn approval_metadata_exact(
    approval: &ApprovalRequestView,
    field: &str,
    expected: &str,
    valid_value: impl Fn(&str) -> bool,
) -> bool {
    approval_metadata_values(approval).any(|(candidate_field, value)| {
        candidate_field == field && value == expected && valid_value(value)
    })
}

fn approval_metadata_field_exists(approval: &ApprovalRequestView, field: &str) -> bool {
    approval_metadata_values(approval).any(|(candidate_field, _)| candidate_field == field)
}

fn approval_metadata_values(approval: &ApprovalRequestView) -> impl Iterator<Item = (&str, &str)> {
    [&approval.input_preview[..], &approval.target.display[..]]
        .into_iter()
        .chain(approval.target.canonical_ref.as_deref())
        .flat_map(|metadata| metadata.split(is_metadata_separator))
        .filter_map(|token| token.split_once('='))
        .filter(|(field, value)| !field.is_empty() && !value.is_empty())
}

fn is_metadata_separator(character: char) -> bool {
    character.is_ascii_whitespace() || matches!(character, ',' | ';')
}

fn is_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use viden_core::{AgentSessionStatus, AgentSessionView, LocalCoreHost, RuntimeOwner};

    use crate::{D6ConnectionState, D11Intent, PermissionChoice, PermissionIntent};

    use super::{PendingD1, PendingD1Kind, open_workspace_with_host};

    fn temp_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("viden-gui-{label}-{unique}"));
        std::fs::create_dir_all(&path).expect("create temporary GUI directory");
        path
    }

    fn initialize_git_repository(project: &PathBuf) {
        std::fs::write(project.join("README.md"), "# GUI certification fixture\n")
            .expect("write fixture readme");
        for args in [
            vec!["init"],
            vec!["add", "README.md"],
            vec![
                "-c",
                "user.name=Viden GUI Test",
                "-c",
                "user.email=viden-gui-test@localhost",
                "commit",
                "-m",
                "Initialize fixture",
            ],
        ] {
            let status = Command::new("git")
                .args(args)
                .current_dir(project)
                .status()
                .expect("run git fixture command");
            assert!(status.success(), "git fixture command must succeed");
        }
    }

    #[test]
    fn agent_start_reconciles_from_the_projected_session() {
        let lane_id = "lane-reconciled".to_string();
        let agent_id = "codex-acp".to_string();
        let pending = PendingD1 {
            command_id: "start-reconciled".to_string(),
            owner: RuntimeOwner {
                lane_id: Some(lane_id.clone()),
                ..RuntimeOwner::default()
            },
            kind: PendingD1Kind::StartAgentSession {
                lane_id: lane_id.clone(),
                agent_id: agent_id.clone(),
                matching_sessions_before: 0,
            },
            accepted: false,
        };
        let sessions = vec![AgentSessionView {
            session_id: "session-reconciled".to_string(),
            lane_id,
            agent_id,
            model: None,
            status: AgentSessionStatus::Completed,
            owner: RuntimeOwner::default(),
            task: "reply".to_string(),
            diagnostic: None,
            output: Some("done".to_string()),
        }];

        assert!(pending.is_satisfied_by_agent_sessions(&sessions));
    }

    #[test]
    fn opening_git_workspace_projects_lane_eligibility_without_entering_d11() {
        let session_home = temp_dir("eligibility-session-home");
        let project = temp_dir("eligibility-project");
        initialize_git_repository(&project);
        let host = LocalCoreHost::with_session_home(&session_home);

        let adapter = open_workspace_with_host(&host, &project).expect("open local workspace");
        let cockpit = adapter.d1_cockpit(None).expect("D1 cockpit projection");
        let eligibility = cockpit
            .workspace_eligibility
            .expect("workspace eligibility is projected during open");

        assert!(eligibility.is_git_repository);
        assert!(eligibility.has_head);
        assert!(eligibility.can_create_lane);
        assert_eq!(eligibility.diagnostic, None);
    }

    #[test]
    fn local_workspace_adapter_previews_default_lane_through_live_core() {
        let session_home = temp_dir("preview-session-home");
        let project = temp_dir("preview-project");
        initialize_git_repository(&project);
        let host = LocalCoreHost::with_session_home(&session_home);
        let mut adapter = open_workspace_with_host(&host, &project).expect("open local workspace");

        let result = adapter
            .send_d1_intent_and_wait(
                "live-default-preview",
                crate::D1Intent::PreviewDefaultLane {
                    preset: "coder".to_string(),
                },
                Duration::from_millis(250),
            )
            .expect("preview default Lane through live Core");

        assert_eq!(result.pending_command_id, None);
        assert_eq!(result.outcome.state, "confirmed");
        assert_eq!(result.projection.starter_lane_previews.len(), 1);
        assert!(
            result.projection.starter_lane_previews[0]
                .diagnostics
                .is_empty()
        );

        drop(adapter);
        drop(host);
        std::fs::remove_dir_all(session_home).expect("remove temporary session home");
        std::fs::remove_dir_all(project).expect("remove temporary project");
    }

    #[test]
    fn local_workspace_adapter_creates_lane_in_non_git_directory_without_worktree() {
        let session_home = temp_dir("non-git-create-session-home");
        let project = temp_dir("non-git-create-project");
        let host = LocalCoreHost::with_session_home(&session_home);
        let mut adapter = open_workspace_with_host(&host, &project).expect("open local workspace");

        let eligibility = adapter
            .d1_cockpit(None)
            .expect("D1 cockpit projection")
            .workspace_eligibility
            .expect("workspace eligibility is projected during open");
        assert!(!eligibility.is_git_repository);
        assert!(!eligibility.has_head);
        assert!(eligibility.can_create_lane);
        assert_eq!(eligibility.diagnostic, None);

        let preview = adapter
            .send_d1_intent_and_wait(
                "live-non-git-preview",
                crate::D1Intent::PreviewDefaultLane {
                    preset: "coder".to_string(),
                },
                Duration::from_millis(250),
            )
            .expect("preview a non-Git Lane through live Core")
            .projection
            .starter_lane_previews
            .into_iter()
            .next()
            .expect("Core publishes a non-Git starter Lane preview");
        assert_eq!(preview.branch, None);

        let create = adapter
            .send_d1_intent_and_wait(
                "live-non-git-create",
                crate::D1Intent::CreateStarterLane {
                    lane_id: preview.lane_id.clone(),
                    preset: "coder".to_string(),
                    branch: None,
                    preview_id: preview.preview_id.clone(),
                    content_sha256: preview.content_sha256,
                },
                Duration::from_millis(250),
            )
            .expect("request non-Git starter Lane creation");
        let request_id = create
            .projection
            .permission_dock
            .request
            .expect("Core approval remains required without Git")
            .id;
        adapter
            .send_permission_intent_and_wait(
                "live-non-git-create-allow-once",
                PermissionIntent::Respond {
                    request_id,
                    choice: PermissionChoice::Once,
                    feedback: None,
                },
                Duration::from_millis(250),
            )
            .expect("allow non-Git starter Lane creation once");
        let created = adapter
            .poll_d1(None, Duration::from_millis(250))
            .expect("observe non-Git starter Lane creation");

        assert!(
            created
                .projection
                .lanes
                .iter()
                .any(|lane| { lane.id == preview.lane_id && lane.branch.is_none() })
        );
        assert!(!project.join(".git").exists());
        assert!(!project.join(".worktrees").exists());

        drop(adapter);
        drop(host);
        std::fs::remove_dir_all(session_home).expect("remove temporary session home");
        std::fs::remove_dir_all(project).expect("remove temporary project");
    }

    #[test]
    fn local_workspace_adapter_creates_previewed_lane_after_live_core_approval() {
        let session_home = temp_dir("create-session-home");
        let project = temp_dir("create-project");
        initialize_git_repository(&project);
        let host = LocalCoreHost::with_session_home(&session_home);
        let mut adapter = open_workspace_with_host(&host, &project).expect("open local workspace");

        let preview = adapter
            .send_d1_intent_and_wait(
                "live-create-preview",
                crate::D1Intent::PreviewDefaultLane {
                    preset: "coder".to_string(),
                },
                Duration::from_millis(250),
            )
            .expect("preview default Lane through live Core")
            .projection
            .starter_lane_previews
            .into_iter()
            .next()
            .expect("Core publishes a starter Lane preview");

        let create = adapter
            .send_d1_intent_and_wait(
                "live-create-lane",
                crate::D1Intent::CreateStarterLane {
                    lane_id: preview.lane_id.clone(),
                    preset: "coder".to_string(),
                    branch: preview.branch,
                    preview_id: preview.preview_id.clone(),
                    content_sha256: preview.content_sha256,
                },
                Duration::from_millis(250),
            )
            .expect("request starter Lane creation through live Core");
        assert!(
            create.projection.permission_dock.request.is_some(),
            "the exact pending Lane owner approval must be visible before Lane registration"
        );
        let (exact_owner, exact_kind) = adapter
            .pending_d1
            .as_ref()
            .map(|pending| (pending.owner.clone(), pending.kind.clone()))
            .expect("starter Lane creation remains pending approval");
        adapter
            .pending_d1
            .as_mut()
            .expect("pending starter creation")
            .owner = RuntimeOwner {
            lane_id: Some("different-lane-owner".to_string()),
            ..RuntimeOwner::default()
        };
        assert!(
            adapter
                .d1_cockpit(None)
                .expect("D1 projection for mismatched owner")
                .permission_dock
                .request
                .is_none(),
            "another owner must never inherit the pending creation approval"
        );
        {
            let pending = adapter
                .pending_d1
                .as_mut()
                .expect("pending starter creation");
            pending.owner = exact_owner;
            pending.kind = PendingD1Kind::QueryAgentAdapters;
        }
        assert!(
            adapter
                .d1_cockpit(None)
                .expect("D1 projection for non-create command")
                .permission_dock
                .request
                .is_none(),
            "non-create pending commands must not expose an unselected Lane approval"
        );
        adapter
            .pending_d1
            .as_mut()
            .expect("pending starter creation")
            .kind = exact_kind;
        let request_id = adapter
            .permission_dock()
            .expect("global permission projection")
            .request
            .expect("Core requests approval before creating a worktree")
            .id;

        adapter
            .send_permission_intent_and_wait(
                "live-create-allow-once",
                PermissionIntent::Respond {
                    request_id,
                    choice: PermissionChoice::Once,
                    feedback: None,
                },
                Duration::from_millis(250),
            )
            .expect("allow starter Lane creation once");
        let created = adapter
            .poll_d1(None, Duration::from_millis(250))
            .expect("observe starter Lane creation through live Core");

        assert_eq!(created.pending_command_id, None);
        assert_eq!(created.outcome.state, "confirmed");
        assert!(
            created
                .projection
                .lanes
                .iter()
                .any(|lane| lane.id == preview.lane_id)
        );
        assert!(
            created
                .projection
                .starter_lane_receipts
                .iter()
                .any(|receipt| receipt.preview_id == preview.preview_id)
        );

        drop(adapter);
        drop(host);
        std::fs::remove_dir_all(session_home).expect("remove temporary session home");
        std::fs::remove_dir_all(project).expect("remove temporary project");
    }

    #[test]
    fn local_workspace_adapter_reaches_live_core_and_can_probe_project() {
        let session_home = temp_dir("session-home");
        let project = temp_dir("project");
        let canonical_project = project.canonicalize().expect("canonical project path");
        let host = LocalCoreHost::with_session_home(&session_home);

        let mut adapter = open_workspace_with_host(&host, &project).expect("open local workspace");

        assert_eq!(adapter.d6_recovery().connection, D6ConnectionState::Live);
        assert!(adapter.supports("runtime.project_onboarding"));
        let initial = adapter
            .poll_d11(Duration::ZERO)
            .expect("poll initial D11 state");
        assert_eq!(initial.pending_command_id, None);
        assert_eq!(initial.pending_intent, None);

        let result = adapter
            .send_d11_intent_and_wait(
                "local-host-project-probe",
                D11Intent::ProbeProject,
                Duration::from_millis(250),
            )
            .expect("probe bound project through Core");
        assert_eq!(result.pending_command_id, None);
        assert_eq!(result.pending_intent, None);
        assert_eq!(
            result.projection.project.expect("Core project probe").root,
            canonical_project.to_string_lossy()
        );

        let preview = adapter
            .send_d11_intent_and_wait(
                "local-host-config-preview",
                D11Intent::PreviewProjectConfig {
                    contents: "[project]\nname = \"project\"\npack = \"rust\"\n".to_string(),
                },
                Duration::from_millis(250),
            )
            .expect("preview project config through Core");
        assert_eq!(preview.pending_command_id, None);
        assert_eq!(preview.pending_intent, None);
        assert!(
            preview
                .projection
                .preview
                .expect("Core config preview")
                .valid
        );

        let confirm = adapter
            .send_d11_intent_and_wait(
                "local-host-config-confirm",
                D11Intent::ConfirmProjectConfig,
                Duration::from_millis(250),
            )
            .expect("confirm project config through Core");
        let confirm = if confirm.projection.pending_approval.is_some() {
            confirm
        } else {
            adapter
                .poll_d11(Duration::from_millis(250))
                .expect("poll project config approval")
        };
        let request_id = confirm
            .projection
            .pending_approval
            .expect("Core project config approval")
            .id;
        adapter
            .send_permission_intent_and_wait(
                "local-host-config-allow",
                PermissionIntent::Respond {
                    request_id,
                    choice: PermissionChoice::Once,
                    feedback: None,
                },
                Duration::from_millis(250),
            )
            .expect("allow project config through Core");
        let confirmed = adapter
            .poll_d11(Duration::from_millis(250))
            .expect("poll confirmed project config");
        assert_eq!(confirmed.pending_command_id, None);
        assert_eq!(confirmed.pending_intent, None);
        assert!(confirmed.projection.confirmed_config.is_some());

        drop(adapter);
        drop(host);
        std::fs::remove_dir_all(session_home).expect("remove temporary session home");
        std::fs::remove_dir_all(project).expect("remove temporary project");
    }
}

/// Parses the opaque `stream:sequence` audit cursor the client handed out.
fn parse_audit_cursor(raw: &str) -> Result<viden_core::EventCursor, String> {
    let (stream_id, sequence) = raw
        .rsplit_once(':')
        .ok_or_else(|| format!("malformed audit cursor `{raw}`"))?;
    Ok(viden_core::EventCursor {
        stream_id: stream_id.to_string(),
        sequence: sequence
            .parse()
            .map_err(|_| format!("malformed audit cursor `{raw}`"))?,
    })
}

/// Reads the canonical Core discriminant for an event kind.
///
/// Core owns the event vocabulary; serializing the tag keeps the client from
/// mirroring a 117-variant enum that would drift on the next Core change.
fn core_event_kind(kind: &viden_core::RuntimeEventKind) -> String {
    match serde_json::to_value(kind) {
        Ok(serde_json::Value::Object(map)) => map
            .get("type")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| "unknown".to_string()),
        _ => "unknown".to_string(),
    }
}
