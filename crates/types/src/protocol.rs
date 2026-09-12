use crate::{
    AgentLaneId, AgentTaskId, RuntimeCommand, RuntimeEvent, RuntimeSnapshot, RuntimeViewState,
    SessionId,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;

pub const FRONTEND_SCHEMA_V1: SchemaVersion = SchemaVersion(1);
pub const FRONTEND_V1_CAPABILITIES: &[&str] = &[
    "runtime.agent_dag",
    "runtime.approvals",
    "runtime.commands",
    "runtime.context",
    "runtime.cost",
    "runtime.events",
    "runtime.evidence",
    "runtime.merge_gate",
    "runtime.queued_input",
    "runtime.replay",
    "runtime.snapshot",
    "runtime.transcript_page",
    "runtime.typed_lanes",
    "runtime.typed_tasks",
    "ui.preferences",
];

/// Compatible schema-1 additions shipped after the immutable Core 0.3.0
/// checkpoint. They are advertised separately so the frozen capability
/// evidence remains byte-for-byte stable.
pub const FRONTEND_V1_EXTENSION_CAPABILITIES: &[&str] = &[
    "core.workspace_host",
    "runtime.agent_adapters",
    "runtime.agent_conversation",
    "runtime.agent_permission_bridge",
    "runtime.agent_session_input",
    "runtime.agent_sessions",
    "runtime.audit",
    "runtime.cockpit_context_v1",
    "runtime.conflict_content",
    "runtime.credential_handles",
    "runtime.credential_staging",
    "runtime.durable_work_evidence",
    "runtime.evidence_reads",
    "runtime.lane_lifecycle",
    "runtime.lane_owner_projection",
    "runtime.operator_git",
    "runtime.project_onboarding",
    "runtime.recent_work",
    "runtime.starter_lane_preview",
    "runtime.structured_diff",
    "runtime.trust_loop",
    "runtime.turn_lifecycle",
    "runtime.workspace_eligibility",
    "runtime.workspace_file_reads",
    "runtime.workspace_files",
    "runtime.workspace_owner",
    "ui.layout_preferences",
    "ui.preference_persistence",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchemaVersion(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilityId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RuntimeOwner {
    pub workspace_id: String,
    pub project_id: String,
    pub lane_id: Option<AgentLaneId>,
    pub session_id: Option<SessionId>,
    pub task_id: Option<AgentTaskId>,
    pub turn_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventCursor {
    pub stream_id: String,
    pub sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventCursorOrder {
    DuplicateOrOld,
    Next,
    Gap,
    StreamMismatch,
}

impl EventCursor {
    pub fn classify_incoming(&self, incoming: &Self) -> EventCursorOrder {
        // Different streams are distinct logs and must never look contiguous to a client.
        if self.stream_id != incoming.stream_id {
            return EventCursorOrder::StreamMismatch;
        }

        match incoming.sequence {
            sequence if sequence <= self.sequence => EventCursorOrder::DuplicateOrOld,
            sequence if sequence == self.sequence.saturating_add(1) => EventCursorOrder::Next,
            _ => EventCursorOrder::Gap,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCommandEnvelope {
    pub schema_version: SchemaVersion,
    pub client_id: String,
    pub command_id: String,
    pub owner: RuntimeOwner,
    pub command: RuntimeCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEventEnvelope {
    pub schema_version: SchemaVersion,
    pub owner: RuntimeOwner,
    pub cursor: EventCursor,
    pub event: RuntimeWireEvent,
}

impl RuntimeEventEnvelope {
    fn validate_known_event_sequence(&self) -> Result<(), String> {
        let RuntimeWireEvent::Known(event) = &self.event else {
            return Ok(());
        };
        if self.cursor.sequence == event.sequence {
            return Ok(());
        }

        Err(format!(
            "runtime event cursor sequence {} does not match event sequence {}",
            self.cursor.sequence, event.sequence
        ))
    }

    fn validate_known_event_owner(&self) -> Result<(), String> {
        let RuntimeWireEvent::Known(event) = &self.event else {
            return Ok(());
        };
        if let crate::RuntimeEventKind::AgentSessionStarted { session }
        | crate::RuntimeEventKind::AgentSessionUpdated { session }
        | crate::RuntimeEventKind::AgentSessionCompleted { session }
        | crate::RuntimeEventKind::AgentSessionFailed { session } = &event.kind
            && (session.owner.lane_id.as_deref() != Some(session.lane_id.as_str())
                || session.owner.session_id.as_deref() != Some(session.session_id.as_str()))
        {
            return Err(
                "agent session payload identity does not match its embedded owner".to_string(),
            );
        }
        let payload_owner = match &event.kind {
            crate::RuntimeEventKind::StarterLanePreviewed { preview } => Some(&preview.owner),
            crate::RuntimeEventKind::StarterLaneCreated { receipt } => Some(&receipt.owner),
            crate::RuntimeEventKind::StarterLanePreviewInvalidated { owner, .. } => Some(owner),
            crate::RuntimeEventKind::WorkspaceChangeUpdated { change } => Some(&change.owner),
            crate::RuntimeEventKind::CheckRunUpdated { check } => Some(&check.owner),
            crate::RuntimeEventKind::LaneRuntimeOwnerBound { binding } => Some(&binding.owner),
            crate::RuntimeEventKind::AgentSessionStarted { session }
            | crate::RuntimeEventKind::AgentSessionUpdated { session }
            | crate::RuntimeEventKind::AgentSessionCompleted { session }
            | crate::RuntimeEventKind::AgentSessionFailed { session } => Some(&session.owner),
            _ => None,
        };
        if payload_owner.is_none_or(|owner| owner == &self.owner) {
            return Ok(());
        }

        Err("owner-scoped event payload owner does not match envelope owner".to_string())
    }

    fn validate_known_event(&self) -> Result<(), String> {
        self.validate_known_event_sequence()?;
        self.validate_known_event_owner()
    }
}

impl Serialize for RuntimeEventEnvelope {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.validate_known_event()
            .map_err(serde::ser::Error::custom)?;
        RuntimeEventEnvelopeRef {
            schema_version: &self.schema_version,
            owner: &self.owner,
            cursor: &self.cursor,
            event: &self.event,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RuntimeEventEnvelope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let envelope = RuntimeEventEnvelopeWire::deserialize(deserializer)?;
        let envelope = Self {
            schema_version: envelope.schema_version,
            owner: envelope.owner,
            cursor: envelope.cursor,
            event: envelope.event,
        };
        envelope
            .validate_known_event()
            .map_err(serde::de::Error::custom)?;
        Ok(envelope)
    }
}

#[derive(Serialize)]
struct RuntimeEventEnvelopeRef<'a> {
    schema_version: &'a SchemaVersion,
    owner: &'a RuntimeOwner,
    cursor: &'a EventCursor,
    event: &'a RuntimeWireEvent,
}

#[derive(Deserialize)]
struct RuntimeEventEnvelopeWire {
    schema_version: SchemaVersion,
    owner: RuntimeOwner,
    cursor: EventCursor,
    event: RuntimeWireEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
// Boxing would change the frozen public construction shape without improving wire behavior.
#[allow(clippy::large_enum_variant)]
pub enum RuntimeWireEvent {
    Known(RuntimeEvent),
    Unknown {
        event_type: String,
        payload: serde_json::Value,
    },
}

impl Serialize for RuntimeWireEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Known(event) => event.serialize(serializer),
            Self::Unknown {
                event_type,
                payload,
            } => UnknownRuntimeEvent {
                kind: UnknownRuntimeEventKind {
                    event_type,
                    payload,
                },
            }
            .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for RuntimeWireEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = serde_json::Value::deserialize(deserializer)?;
        let kind = raw
            .get("kind")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| serde::de::Error::custom("runtime event must contain kind"))?;
        let event_type = kind
            .get("type")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| serde::de::Error::custom("runtime event kind must contain type"))?;

        if is_known_runtime_event_type(event_type) {
            match serde_json::from_value(raw.clone()) {
                Ok(event) => return Ok(Self::Known(event)),
                Err(error)
                    if !matches!(event_type, "command_accepted" | "lane_command_accepted") =>
                {
                    return Err(serde::de::Error::custom(error));
                }
                Err(_) => {
                    // CommandAccepted embeds RuntimeCommand. A schema-v1 client may know the
                    // outer event while not knowing a newer command variant; preserve that
                    // extension payload instead of dropping the entire replay stream.
                }
            }
        }

        // Preserve forward-compatible payloads so older clients can inspect a stream event
        // they cannot yet reduce, instead of rejecting the entire stream.
        Ok(Self::Unknown {
            event_type: event_type.to_string(),
            payload: kind
                .get("payload")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        })
    }
}

#[derive(Serialize)]
struct UnknownRuntimeEvent<'a> {
    kind: UnknownRuntimeEventKind<'a>,
}

#[derive(Serialize)]
struct UnknownRuntimeEventKind<'a> {
    #[serde(rename = "type")]
    event_type: &'a str,
    payload: &'a serde_json::Value,
}

fn is_known_runtime_event_type(event_type: &str) -> bool {
    matches!(
        event_type,
        "agent_adapter_probed"
            | "agent_adapters_loaded"
            | "agent_session_completed"
            | "agent_session_failed"
            | "agent_session_input_accepted"
            | "agent_session_started"
            | "agent_session_updated"
            | "workspace_eligibility_updated"
            | "ui_preferences_updated"
            | "recent_work_loaded"
            // The answer to a `QueryAudit`. Omitting it here would quarantine
            // every audit page as an unknown event on any serialized
            // snapshot/replay path, and a dropped page reads to an operator as
            // "nothing was audited" — the one failure an append-only timeline
            // exists to prevent.
            | "audit_page_loaded"
            // The two answers to an evidence read. Quarantining a page reads
            // to an operator as "no evidence was recorded", and quarantining a
            // content read as "this evidence has no content" — two fabricated
            // absences over a store whose whole purpose is to be citable.
            | "evidence_page_loaded"
            | "evidence_content_loaded"
            // The answer to a `QueryWorkspaceFiles`. Third event to need this
            // arm explicitly: quarantining an inventory page as an unknown
            // event reads to a client as "this workspace has no files", the
            // fabricated absence GUI-CORE-022 exists to prevent.
            | "workspace_files_loaded"
            // The answer to a `QueryWorkspaceDiff`. Quarantining a diff page
            // as an unknown event reads to a reviewer as "nothing changed",
            // which is the one thing a diff surface must never say wrongly.
            | "workspace_diff_loaded"
            // The answer to a `ReadWorkspaceFile`
            // (`runtime.workspace_file_reads`). Quarantining it leaves a
            // client that opened a file with no answer at all, and an empty
            // editor is indistinguishable from an empty file — so the one
            // event that can honestly say "binary", "missing", or "not
            // readable" must never arrive as unknown.
            | "workspace_file_loaded"
            // The settled answer to a `RunOperatorGitAction`. Quarantining it
            // leaves a client that asked for a push with no answer at all, and
            // an operator reads "no answer" as "it worked" — the worst reading
            // available for a mutation that may have been rejected by the
            // remote.
            | "operator_git_action_finished"
            | "workspace_source_updated"
            // The workspace-scoped operator identity
            // (`runtime.workspace_owner`, GUI-CORE-027). Quarantining it
            // leaves a client with no actor for a workspace-target commit, so
            // it renders the commit bar as unavailable and an operator reads
            // "this build cannot commit" about a Core that can.
            | "workspace_runtime_owner_bound"
            // One Lane worktree's source facts. Quarantining a Lane's row
            // leaves its tab strip and context dock reading the *workspace*
            // branch, which is a different tree wearing this Lane's name.
            | "lane_source_updated"
            // The persisted cockpit layout record. Quarantining it strands an
            // operator's sidebar and statusbar choices on a client that
            // believes Core never stored them.
            | "ui_layout_preferences_updated"
            | "runtime_service_health_updated"
            | "workspace_change_updated"
            | "check_run_updated"
            | "snapshot_updated"
            // The two halves of a turn bracket (`runtime.turn_lifecycle`).
            // Quarantining either strands a client on the guess this
            // capability exists to replace: a dropped `turn_started` reads as
            // "nothing is running" while a turn runs, and a dropped
            // `turn_finished` leaves a turn live in `active_turns` forever,
            // which is exactly the stuck composer the native path had.
            | "turn_started"
            | "turn_finished"
            | "assistant_delta"
            // Typed non-text Agent content. Omitting it here would quarantine
            // every image and file part as an unknown event, which is exactly
            // the dropped-content failure typed parts exist to prevent.
            | "agent_message_part"
            | "tool_call_started"
            | "tool_call_finished"
            | "approval_requested"
            | "approval_resolved"
            | "command_accepted"
            | "lane_command_accepted"
            | "command_rejected"
            | "transcript_page_loaded"
            | "input_queued"
            | "input_dequeued"
            | "task_updated"
            | "agent_dag_updated"
            | "lane_updated"
            | "lane_runtime_owner_bound"
            | "lane_output_appended"
            | "lane_conflict_detected"
            | "lane_recovery_required"
            | "project_probed"
            | "project_config_previewed"
            | "project_config_confirmed"
            | "credential_handle_stored"
            | "starter_lane_previewed"
            | "starter_lane_created"
            | "starter_lane_preview_invalidated"
            | "evidence_recorded"
            | "context_updated"
            | "context_bundle_built"
            | "context_item_stored"
            | "context_view_derived"
            | "context_reduction_recorded"
            | "context_retrieved"
            | "context_budget_exceeded"
            | "context_quality_failed"
            | "cost_usage_recorded"
            | "provider_cache_observed"
            | "evidence_canonicalized"
            | "merge_gate_updated"
            | "handoff_updated"
            | "review_request_updated"
            | "contract_updated"
            | "dependency_updated"
            | "merge_conflict_bounced"
            | "revert_recorded"
            | "provider_health_updated"
            | "token_cost_updated"
            | "error"
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSnapshotEnvelope {
    pub schema_version: SchemaVersion,
    pub capabilities: BTreeSet<CapabilityId>,
    pub cursor: EventCursor,
    pub snapshot: RuntimeSnapshot,
    pub view: RuntimeViewState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreHandshake {
    pub core_version: String,
    pub supported_schema_versions: Vec<SchemaVersion>,
    pub active_schema_version: SchemaVersion,
    pub capabilities: BTreeSet<CapabilityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayRequest {
    pub after: EventCursor,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayBatch {
    pub events: Vec<RuntimeEventEnvelope>,
    pub next: EventCursor,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GapRecovery {
    Replay(ReplayRequest),
    SnapshotRequired { reason_code: String },
}
