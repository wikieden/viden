use std::time::Duration;

use viden_core::{
    CORE_CLIENT_CAPABILITIES, CoreClient, CoreClientError, CoreHandshake, EventCursor,
    ReplayRequest, RuntimeCommand, RuntimeCommandEnvelope, RuntimeEvent, RuntimeEventEnvelope,
    RuntimeSnapshotEnvelope, RuntimeViewState, RuntimeWireEvent, validate_handshake,
    validate_schema_version,
};
use viden_types::{CapabilityId, FRONTEND_SCHEMA_V1, RuntimeOwner};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PumpOutcome {
    Idle,
    Applied(EventCursor),
    Recovered(EventCursor),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiClientError {
    Core(CoreClientError),
}

impl std::fmt::Display for TuiClientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Core(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for TuiClientError {}

impl From<CoreClientError> for TuiClientError {
    fn from(value: CoreClientError) -> Self {
        Self::Core(value)
    }
}

pub(super) struct TuiClientDriver<C: CoreClient> {
    client: C,
    handshake: CoreHandshake,
    confirmed: RuntimeSnapshotEnvelope,
    next_command: u64,
    owner: RuntimeOwner,
    applied_events: Vec<RuntimeEvent>,
}

impl<C: CoreClient> std::fmt::Debug for TuiClientDriver<C> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TuiClientDriver")
            .field("handshake", &self.handshake)
            .field("cursor", &self.confirmed.cursor)
            .field("next_command", &self.next_command)
            .finish_non_exhaustive()
    }
}

impl<C: CoreClient> TuiClientDriver<C> {
    pub(super) fn connect(mut client: C) -> Result<Self, TuiClientError> {
        let handshake = client.discover()?;
        validate_handshake(&handshake).map_err(CoreClientError::Compatibility)?;
        let confirmed = acquire_validated_snapshot(&mut client)?;
        Ok(Self {
            client,
            handshake,
            confirmed,
            next_command: 1,
            owner: RuntimeOwner::default(),
            applied_events: Vec::new(),
        })
    }

    pub(super) fn view(&self) -> &RuntimeViewState {
        &self.confirmed.view
    }

    pub(super) fn cursor(&self) -> &EventCursor {
        &self.confirmed.cursor
    }

    /// Extension features are enabled only when both discovery and the
    /// authoritative snapshot advertise them. Missing extensions never widen
    /// authority and never block the frozen base client from starting.
    pub(super) fn has_capability(&self, capability: &str) -> bool {
        let capability = CapabilityId(capability.to_string());
        self.handshake.capabilities.contains(&capability)
            && self.confirmed.capabilities.contains(&capability)
    }

    pub(super) fn capabilities(&self) -> std::collections::BTreeSet<CapabilityId> {
        self.handshake
            .capabilities
            .intersection(&self.confirmed.capabilities)
            .cloned()
            .collect()
    }

    /// Returns the ordered known events applied since the last projection.
    /// UI workflows use these receipts to distinguish command acceptance from
    /// the later authoritative fact that confirms an operation.
    pub(super) fn take_applied_events(&mut self) -> Vec<RuntimeEvent> {
        std::mem::take(&mut self.applied_events)
    }

    pub(super) fn send(&mut self, command: RuntimeCommand) -> Result<String, TuiClientError> {
        self.send_for_owner(self.owner.clone(), command)
    }

    pub(super) fn send_for_owner(
        &mut self,
        owner: RuntimeOwner,
        command: RuntimeCommand,
    ) -> Result<String, TuiClientError> {
        let command_id = format!("tui-{}", self.next_command);
        self.next_command = self.next_command.saturating_add(1);
        self.client.send(RuntimeCommandEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            client_id: "viden-tui".to_string(),
            command_id: command_id.clone(),
            owner,
            command,
        })?;
        Ok(command_id)
    }

    pub(super) fn pump(&mut self) -> Result<PumpOutcome, TuiClientError> {
        self.pump_with_timeout(Duration::from_millis(0))
    }

    fn pump_with_timeout(&mut self, timeout: Duration) -> Result<PumpOutcome, TuiClientError> {
        let before = self.cursor().clone();
        let Some(delivered) = self.client.recv(timeout)? else {
            return Ok(PumpOutcome::Idle);
        };
        validate_event_envelope(&delivered)?;

        if delivered.cursor.stream_id != before.stream_id {
            let recovered = acquire_validated_snapshot(&mut self.client)?;
            // A replacement snapshot may already include the event that
            // announced its stream. Keep that Core receipt only when the
            // recovered cursor proves the authoritative view covers it.
            if recovered.cursor.stream_id == delivered.cursor.stream_id
                && recovered.cursor.sequence >= delivered.cursor.sequence
                && let RuntimeWireEvent::Known(event) = delivered.event
            {
                self.applied_events.push(event);
            }
            self.confirmed = recovered;
            return Ok(PumpOutcome::Recovered(self.confirmed.cursor.clone()));
        }
        if delivered.cursor.sequence <= before.sequence {
            return Ok(PumpOutcome::Idle);
        }
        if delivered.cursor.sequence > before.sequence.saturating_add(1) {
            return self.recover_gap(delivered);
        }

        let mut staged = self.confirmed.clone();
        apply_event_envelope(&mut staged, &delivered)?;
        self.confirmed = staged;
        if let RuntimeWireEvent::Known(event) = delivered.event {
            self.applied_events.push(event);
        }
        Ok(PumpOutcome::Applied(delivered.cursor))
    }

    fn recover_gap(
        &mut self,
        incoming: RuntimeEventEnvelope,
    ) -> Result<PumpOutcome, TuiClientError> {
        let mut staged = self.confirmed.clone();
        let mut staged_events = Vec::new();
        let mut request = ReplayRequest {
            after: staged.cursor.clone(),
            limit: 500,
        };
        let mut delivered = false;
        loop {
            let batch = match self.client.replay(request.clone()) {
                Ok(batch) => batch,
                Err(CoreClientError::SnapshotRequired { .. }) => {
                    self.confirmed = acquire_validated_snapshot(&mut self.client)?;
                    return Ok(PumpOutcome::Recovered(self.confirmed.cursor.clone()));
                }
                Err(error) => return Err(error.into()),
            };
            if batch.events.is_empty() && !batch.complete {
                return Err(CoreClientError::Protocol(
                    "runtime replay made no progress".to_string(),
                )
                .into());
            }
            for envelope in &batch.events {
                validate_event_envelope(envelope)?;
                if envelope.cursor.stream_id != staged.cursor.stream_id
                    || envelope.cursor.sequence != staged.cursor.sequence.saturating_add(1)
                {
                    return Err(CoreClientError::Protocol(format!(
                        "non-contiguous replay after {}:{}: received {}:{}",
                        request.after.stream_id,
                        request.after.sequence,
                        envelope.cursor.stream_id,
                        envelope.cursor.sequence
                    ))
                    .into());
                }
                apply_event_envelope(&mut staged, envelope)?;
                if let RuntimeWireEvent::Known(event) = &envelope.event {
                    staged_events.push(event.clone());
                }
                delivered |= envelope.cursor == incoming.cursor;
            }
            if batch.next != staged.cursor {
                return Err(CoreClientError::Protocol(format!(
                    "replay next cursor {}:{} does not match applied cursor {}:{}",
                    batch.next.stream_id,
                    batch.next.sequence,
                    staged.cursor.stream_id,
                    staged.cursor.sequence
                ))
                .into());
            }
            if batch.complete {
                break;
            }
            request = ReplayRequest {
                after: batch.next,
                limit: 500,
            };
        }
        if staged.cursor.sequence < incoming.cursor.sequence || !delivered {
            return Err(CoreClientError::Protocol(format!(
                "complete replay ended at {}:{} before incoming cursor {}:{}",
                staged.cursor.stream_id,
                staged.cursor.sequence,
                incoming.cursor.stream_id,
                incoming.cursor.sequence
            ))
            .into());
        }
        self.confirmed = staged;
        self.applied_events.extend(staged_events);
        Ok(PumpOutcome::Recovered(self.confirmed.cursor.clone()))
    }
}

fn acquire_validated_snapshot<C: CoreClient>(
    client: &mut C,
) -> Result<RuntimeSnapshotEnvelope, TuiClientError> {
    let snapshot = client.snapshot()?;
    validate_snapshot_envelope(&snapshot)?;
    Ok(snapshot)
}

fn validate_snapshot_envelope(snapshot: &RuntimeSnapshotEnvelope) -> Result<(), TuiClientError> {
    validate_schema_version(snapshot.schema_version).map_err(CoreClientError::Compatibility)?;
    for required in CORE_CLIENT_CAPABILITIES {
        if !snapshot
            .capabilities
            .contains(&CapabilityId((*required).to_string()))
        {
            return Err(CoreClientError::Compatibility(format!(
                "missing core capability `{required}`"
            ))
            .into());
        }
    }
    if snapshot.cursor.stream_id.is_empty() {
        return Err(CoreClientError::Protocol(
            "runtime snapshot stream id must not be empty".to_string(),
        )
        .into());
    }
    if snapshot.snapshot != snapshot.view.snapshot {
        return Err(CoreClientError::Protocol(
            "runtime snapshot and projected view disagree".to_string(),
        )
        .into());
    }
    Ok(())
}

fn validate_event_envelope(envelope: &RuntimeEventEnvelope) -> Result<(), TuiClientError> {
    validate_schema_version(envelope.schema_version).map_err(CoreClientError::Compatibility)?;
    if let RuntimeWireEvent::Known(event) = &envelope.event
        && event.sequence != envelope.cursor.sequence
    {
        return Err(CoreClientError::Protocol(format!(
            "runtime event sequence {} does not match cursor sequence {}",
            event.sequence, envelope.cursor.sequence
        ))
        .into());
    }
    Ok(())
}

fn apply_event_envelope(
    confirmed: &mut RuntimeSnapshotEnvelope,
    envelope: &RuntimeEventEnvelope,
) -> Result<(), TuiClientError> {
    validate_event_envelope(envelope)?;
    if let RuntimeWireEvent::Known(event) = &envelope.event {
        confirmed.view.apply_event(event);
    }
    confirmed.cursor = envelope.cursor.clone();
    confirmed.snapshot = confirmed.view.snapshot.clone();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::preferences::{ColorDepth, PreferenceValue, SettingsPanel};
    use sha2::{Digest, Sha256};
    use std::{
        collections::{BTreeMap, VecDeque},
        path::PathBuf,
        time::Duration,
    };
    use viden_core::{
        CORE_EXTENSION_CAPABILITIES, CoreClientError, frontend_capabilities, local_core_handshake,
    };
    use viden_types::{
        PermissionLevel, PermissionMode, ReplayBatch, ReplayRequest, RuntimeEvent,
        RuntimeEventEnvelope, RuntimeEventKind, RuntimeSnapshot, RuntimeSnapshotEnvelope,
        RuntimeWireEvent, TranscriptPage, TranscriptPageRequest, WorkMode,
    };

    #[derive(Default)]
    struct FakeCoreClient {
        handshake: Option<CoreHandshake>,
        snapshot: Option<RuntimeSnapshotEnvelope>,
        recovery_snapshot: Option<RuntimeSnapshotEnvelope>,
        snapshot_calls: usize,
        events: VecDeque<RuntimeEventEnvelope>,
        replay: VecDeque<ReplayBatch>,
        replay_error: Option<CoreClientError>,
        sent: Vec<RuntimeCommandEnvelope>,
    }

    impl FakeCoreClient {
        fn compatible() -> Self {
            let snapshot = runtime_snapshot();
            let cursor = EventCursor {
                stream_id: "fixture".to_string(),
                sequence: 0,
            };
            Self {
                handshake: Some(local_core_handshake()),
                snapshot: Some(RuntimeSnapshotEnvelope {
                    schema_version: FRONTEND_SCHEMA_V1,
                    capabilities: frontend_capabilities(),
                    cursor: cursor.clone(),
                    view: RuntimeViewState::new(snapshot.clone()),
                    snapshot,
                }),
                ..Self::default()
            }
        }
    }

    impl CoreClient for FakeCoreClient {
        fn discover(&mut self) -> Result<CoreHandshake, CoreClientError> {
            self.handshake
                .clone()
                .ok_or(CoreClientError::HandshakeRequired)
        }

        fn send(&mut self, command: RuntimeCommandEnvelope) -> Result<(), CoreClientError> {
            self.sent.push(command);
            Ok(())
        }

        fn recv(
            &mut self,
            _timeout: Duration,
        ) -> Result<Option<RuntimeEventEnvelope>, CoreClientError> {
            Ok(self.events.pop_front())
        }

        fn snapshot(&mut self) -> Result<RuntimeSnapshotEnvelope, CoreClientError> {
            self.snapshot_calls = self.snapshot_calls.saturating_add(1);
            if self.snapshot_calls > 1
                && let Some(snapshot) = &self.recovery_snapshot
            {
                return Ok(snapshot.clone());
            }
            self.snapshot
                .clone()
                .ok_or(CoreClientError::MissingSnapshot)
        }

        fn replay(&mut self, _request: ReplayRequest) -> Result<ReplayBatch, CoreClientError> {
            if let Some(error) = self.replay_error.take() {
                return Err(error);
            }
            self.replay
                .pop_front()
                .ok_or_else(|| CoreClientError::Transport("missing replay fixture".to_string()))
        }

        fn transcript_page(
            &mut self,
            _request: TranscriptPageRequest,
        ) -> Result<TranscriptPage, CoreClientError> {
            Err(CoreClientError::Transport(
                "transcript page is not used by TUI client tests".to_string(),
            ))
        }
    }

    #[test]
    fn shared_frontend_fixtures_reduce_to_core_expected_facts() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            initial_snapshot: RuntimeSnapshot,
            events: Vec<RuntimeEventEnvelope>,
            expected_final_cursor: EventCursor,
            expected_view_sha256: String,
        }
        let fixtures = [
            (
                "stream-tool",
                include_str!(
                    "../../../../crates/types/tests/fixtures/frontend-contract-v1/stream-tool.json"
                ),
            ),
            (
                "approval-allow-deny",
                include_str!(
                    "../../../../crates/types/tests/fixtures/frontend-contract-v1/approval-allow-deny.json"
                ),
            ),
            (
                "queued-follow-up",
                include_str!(
                    "../../../../crates/types/tests/fixtures/frontend-contract-v1/queued-follow-up.json"
                ),
            ),
            (
                "dag-blocker",
                include_str!(
                    "../../../../crates/types/tests/fixtures/frontend-contract-v1/dag-blocker.json"
                ),
            ),
            (
                "multi-lane",
                include_str!(
                    "../../../../crates/types/tests/fixtures/frontend-contract-v1/multi-lane.json"
                ),
            ),
            (
                "merge-gate",
                include_str!(
                    "../../../../crates/types/tests/fixtures/frontend-contract-v1/merge-gate.json"
                ),
            ),
            (
                "context-pressure-cost-blind",
                include_str!(
                    "../../../../crates/types/tests/fixtures/frontend-contract-v1/context-pressure-cost-blind.json"
                ),
            ),
            (
                "plan-denial",
                include_str!(
                    "../../../../crates/types/tests/fixtures/frontend-contract-v1/plan-denial.json"
                ),
            ),
            (
                "d1-vertical-slice",
                include_str!(
                    "../../../../crates/types/tests/fixtures/frontend-contract-v1/d1-vertical-slice.json"
                ),
            ),
        ];

        for (fixture_id, fixture_json) in fixtures {
            let fixture: Fixture =
                serde_json::from_str(fixture_json).expect("shared frontend fixture");
            let fake = FakeCoreClient {
                handshake: Some(local_core_handshake()),
                snapshot: Some(RuntimeSnapshotEnvelope {
                    schema_version: FRONTEND_SCHEMA_V1,
                    capabilities: frontend_capabilities(),
                    cursor: EventCursor {
                        stream_id: fixture.expected_final_cursor.stream_id.clone(),
                        sequence: 0,
                    },
                    view: RuntimeViewState::new(fixture.initial_snapshot.clone()),
                    snapshot: fixture.initial_snapshot,
                }),
                events: fixture.events.into(),
                ..FakeCoreClient::default()
            };
            let event_count = fake.events.len();
            let mut driver = TuiClientDriver::connect(fake).expect("connect");

            for _ in 0..event_count {
                assert!(
                    matches!(driver.pump().unwrap(), PumpOutcome::Applied(_)),
                    "{fixture_id}"
                );
            }
            assert_eq!(
                canonical_view_sha256(driver.view()),
                fixture.expected_view_sha256,
                "{fixture_id}"
            );
            assert_eq!(
                driver.cursor(),
                &fixture.expected_final_cursor,
                "{fixture_id}"
            );
        }
    }

    #[test]
    fn lane_runtime_owner_extension_fixture_replays_the_exact_owner() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            initial_snapshot: RuntimeSnapshot,
            events: Vec<RuntimeEventEnvelope>,
            expected_final_cursor: EventCursor,
            expected_view_sha256: String,
        }
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/frontend-host-services.json"
        ))
        .expect("extension fixture");
        let snapshot = fixture.initial_snapshot;
        let fake = FakeCoreClient {
            handshake: Some(local_core_handshake()),
            snapshot: Some(RuntimeSnapshotEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                capabilities: frontend_capabilities(),
                cursor: EventCursor {
                    stream_id: fixture.expected_final_cursor.stream_id.clone(),
                    sequence: 0,
                },
                view: RuntimeViewState::new(snapshot.clone()),
                snapshot,
            }),
            events: fixture.events.into(),
            ..FakeCoreClient::default()
        };
        let event_count = fake.events.len();
        let mut driver = TuiClientDriver::connect(fake).expect("connect");

        for _ in 0..event_count {
            assert!(matches!(driver.pump().unwrap(), PumpOutcome::Applied(_)));
        }

        assert_eq!(driver.cursor(), &fixture.expected_final_cursor);
        assert_eq!(
            canonical_view_sha256(driver.view()),
            fixture.expected_view_sha256
        );
        assert_eq!(driver.view().lane_runtime_owners.len(), 1);
        let binding = &driver.view().lane_runtime_owners[0];
        assert_eq!(binding.lane_id, "lane-host-fixture");
        assert_eq!(binding.owner.lane_id.as_deref(), Some("lane-host-fixture"));
        assert_eq!(
            binding.owner.session_id.as_deref(),
            Some("session-host-fixture")
        );
        assert_eq!(binding.owner.task_id.as_deref(), Some("task_host_fixture"));
        assert_eq!(binding.owner.turn_id.as_deref(), Some("turn-host-fixture"));
    }

    fn canonical_view_sha256(view: &RuntimeViewState) -> String {
        let value = serde_json::to_value(view).expect("runtime view must serialize");
        let sorted = sort_json(value);
        let bytes = serde_json::to_vec(&sorted).expect("canonical json must serialize");
        format!("{:x}", Sha256::digest(bytes))
    }

    fn sort_json(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => {
                let sorted = map
                    .into_iter()
                    .map(|(key, value)| (key, sort_json(value)))
                    .collect::<BTreeMap<_, _>>();
                serde_json::Value::Object(sorted.into_iter().collect())
            }
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.into_iter().map(sort_json).collect())
            }
            other => other,
        }
    }

    #[test]
    fn lane_runtime_owner_restart_snapshot_discards_the_stale_binding() {
        let mut fake = FakeCoreClient::compatible();
        let mut initial = fake.snapshot.clone().expect("initial snapshot");
        initial.view.lane_runtime_owners = vec![viden_types::LaneRuntimeOwnerBinding {
            lane_id: "lane-old".to_string(),
            owner: RuntimeOwner {
                lane_id: Some("lane-old".to_string()),
                ..RuntimeOwner::default()
            },
        }];
        initial.snapshot = initial.view.snapshot.clone();
        fake.snapshot = Some(initial.clone());
        let mut replacement = initial;
        replacement.cursor.stream_id = "replacement".to_string();
        replacement.view.lane_runtime_owners.clear();
        replacement.snapshot = replacement.view.snapshot.clone();
        fake.recovery_snapshot = Some(replacement);
        let mut replacement_event = event(
            1,
            RuntimeEventKind::AssistantDelta {
                message_id: "replacement".to_string(),
                task_id: None,
                session_id: None,
                content: "replacement".to_string(),
            },
        );
        replacement_event.cursor.stream_id = "replacement".to_string();
        fake.events.push_back(replacement_event);

        let mut driver = TuiClientDriver::connect(fake).expect("connect");
        assert_eq!(driver.view().lane_runtime_owners.len(), 1);

        assert!(matches!(driver.pump().unwrap(), PumpOutcome::Recovered(_)));
        assert!(driver.view().lane_runtime_owners.is_empty());
    }

    #[test]
    fn stream_change_preference_update_finishes_the_accepted_settings_command() {
        let command = RuntimeCommand::SetUiPreferences {
            patch: viden_types::UiPreferencePatch {
                density: Some(viden_core::UiDensity::Comfy),
                ..viden_types::UiPreferencePatch::default()
            },
        };
        let accepted = event(
            1,
            RuntimeEventKind::CommandAccepted {
                command_id: "tui-1".to_string(),
                command: command.clone(),
            },
        );
        let persisted = viden_types::UiPreferences {
            density: viden_core::UiDensity::Comfy,
            ..viden_types::UiPreferences::default()
        };
        let resolved = viden_core::ResolvedUiPreferences {
            density: viden_core::UiDensity::Comfy,
            ..viden_core::ResolvedUiPreferences::default()
        };
        let mut updated = event(
            1,
            RuntimeEventKind::UiPreferencesUpdated {
                resolved,
                persisted: Some(persisted),
                diagnostics: Vec::new(),
            },
        );
        updated.cursor.stream_id = "replacement".to_string();

        let mut fake = FakeCoreClient::compatible();
        let baseline = fake.snapshot.as_ref().unwrap().view.ui_preferences.clone();
        let mut replacement = fake.snapshot.clone().unwrap();
        replacement.cursor = updated.cursor.clone();
        if let RuntimeWireEvent::Known(event) = &updated.event {
            replacement.view.apply_event(event);
        }
        replacement.snapshot = replacement.view.snapshot.clone();
        fake.recovery_snapshot = Some(replacement);
        fake.events.extend([accepted, updated]);

        let mut driver = TuiClientDriver::connect(fake).expect("connect");
        let mut panel = SettingsPanel::new(&baseline, ColorDepth::Auto);
        assert!(panel.select(PreferenceValue::Density(viden_core::UiDensity::Comfy)));
        panel.begin_pending("tui-1".to_string(), command);

        assert!(matches!(driver.pump().unwrap(), PumpOutcome::Applied(_)));
        for event in driver.take_applied_events() {
            panel.observe_event(&event);
        }
        assert!(panel.is_pending());

        assert!(matches!(driver.pump().unwrap(), PumpOutcome::Recovered(_)));
        for event in driver.take_applied_events() {
            panel.observe_event(&event);
        }
        assert!(!panel.is_pending());
        assert!(panel.has_succeeded());
    }

    #[test]
    fn incompatible_schema_fails_before_any_command_is_sent() {
        let mut fake = FakeCoreClient::compatible();
        let mut handshake = local_core_handshake();
        handshake.active_schema_version = viden_types::SchemaVersion(2);
        fake.handshake = Some(handshake);

        let error = TuiClientDriver::connect(fake).unwrap_err();

        assert!(matches!(
            error,
            TuiClientError::Core(CoreClientError::Compatibility(_))
        ));
    }

    #[test]
    fn unsupported_snapshot_schema_is_rejected_on_connect() {
        let mut fake = FakeCoreClient::compatible();
        fake.snapshot.as_mut().unwrap().schema_version = viden_types::SchemaVersion(2);

        let error = TuiClientDriver::connect(fake).expect_err("snapshot schema must be validated");

        assert!(matches!(
            error,
            TuiClientError::Core(CoreClientError::Compatibility(_))
        ));
    }

    #[test]
    fn missing_required_snapshot_capability_is_rejected_on_connect() {
        let mut fake = FakeCoreClient::compatible();
        fake.snapshot
            .as_mut()
            .unwrap()
            .capabilities
            .remove(&CapabilityId("runtime.commands".to_string()));

        let error =
            TuiClientDriver::connect(fake).expect_err("snapshot capabilities must be validated");

        assert!(matches!(
            error,
            TuiClientError::Core(CoreClientError::Compatibility(_))
        ));
    }

    #[test]
    fn each_missing_extension_is_feature_gated_without_blocking_startup() {
        // 23 -> 25 with the 0.3.4 contract increment's first batch (C5):
        // `runtime.workspace_owner` and `ui.layout_preferences`. The
        // milestone target is 29; each batch moves this by what it adds.
        assert_eq!(CORE_EXTENSION_CAPABILITIES.len(), 25);
        for capability in CORE_EXTENSION_CAPABILITIES {
            let mut fake = FakeCoreClient::compatible();
            let capability = CapabilityId((*capability).to_string());
            fake.handshake
                .as_mut()
                .unwrap()
                .capabilities
                .remove(&capability);
            fake.snapshot
                .as_mut()
                .unwrap()
                .capabilities
                .remove(&capability);

            let driver = TuiClientDriver::connect(fake)
                .unwrap_or_else(|error| panic!("missing {capability:?} blocked startup: {error}"));

            assert!(!driver.has_capability(&capability.0));
            for other in CORE_EXTENSION_CAPABILITIES
                .iter()
                .filter(|other| **other != capability.0)
            {
                assert!(driver.has_capability(other));
            }
        }
    }

    #[test]
    fn handshake_and_snapshot_must_both_advertise_an_extension_feature() {
        let capability = CapabilityId("runtime.lane_owner_projection".to_string());
        for remove_from_handshake in [true, false] {
            let mut fake = FakeCoreClient::compatible();
            if remove_from_handshake {
                fake.handshake
                    .as_mut()
                    .unwrap()
                    .capabilities
                    .remove(&capability);
            } else {
                fake.snapshot
                    .as_mut()
                    .unwrap()
                    .capabilities
                    .remove(&capability);
            }

            let driver = TuiClientDriver::connect(fake).expect("base contract still connects");

            assert!(!driver.has_capability(&capability.0));
        }
    }

    #[test]
    fn empty_snapshot_stream_id_is_rejected_on_connect() {
        let mut fake = FakeCoreClient::compatible();
        fake.snapshot.as_mut().unwrap().cursor.stream_id.clear();

        let error = TuiClientDriver::connect(fake).expect_err("snapshot stream id must be valid");

        assert!(matches!(
            error,
            TuiClientError::Core(CoreClientError::Protocol(_))
        ));
    }

    #[test]
    fn snapshot_view_disagreement_is_rejected_on_connect() {
        let mut fake = FakeCoreClient::compatible();
        fake.snapshot
            .as_mut()
            .unwrap()
            .snapshot
            .model_label
            .push_str("-disagrees");

        let error = TuiClientDriver::connect(fake).expect_err("snapshot and view must agree");

        assert!(matches!(
            error,
            TuiClientError::Core(CoreClientError::Protocol(_))
        ));
    }

    #[test]
    fn stream_change_recovery_rejects_an_invalid_snapshot() {
        let mut fake = FakeCoreClient::compatible();
        let mut replacement = fake.snapshot.clone().unwrap();
        replacement.schema_version = viden_types::SchemaVersion(2);
        fake.recovery_snapshot = Some(replacement);
        let mut replacement_event = event(
            1,
            RuntimeEventKind::AssistantDelta {
                message_id: "replacement".to_string(),
                task_id: None,
                session_id: None,
                content: "replacement".to_string(),
            },
        );
        replacement_event.cursor.stream_id = "replacement".to_string();
        fake.events.push_back(replacement_event);

        let mut driver = TuiClientDriver::connect(fake).expect("initial snapshot is valid");
        let error = driver
            .pump()
            .expect_err("stream recovery snapshot must be validated");

        assert!(matches!(
            error,
            TuiClientError::Core(CoreClientError::Compatibility(_))
        ));
        assert_eq!(driver.cursor().stream_id, "fixture");
    }

    #[test]
    fn snapshot_required_recovery_rejects_an_invalid_snapshot() {
        let mut fake = FakeCoreClient::compatible();
        let mut replacement = fake.snapshot.clone().unwrap();
        replacement.snapshot.model_label.push_str("-disagrees");
        fake.recovery_snapshot = Some(replacement);
        fake.replay_error = Some(CoreClientError::SnapshotRequired {
            reason_code: "expired".to_string(),
        });
        fake.events.push_back(event(
            3,
            RuntimeEventKind::AssistantDelta {
                message_id: "gap".to_string(),
                task_id: None,
                session_id: None,
                content: "gap".to_string(),
            },
        ));

        let mut driver = TuiClientDriver::connect(fake).expect("initial snapshot is valid");
        let error = driver
            .pump()
            .expect_err("SnapshotRequired recovery snapshot must be validated");

        assert!(matches!(
            error,
            TuiClientError::Core(CoreClientError::Protocol(_))
        ));
        assert_eq!(driver.cursor().sequence, 0);
    }

    #[test]
    fn sequence_gap_requests_replay_before_success_is_visible() {
        let mut fake = FakeCoreClient::compatible();
        fake.events.push_back(event(
            3,
            RuntimeEventKind::AssistantDelta {
                message_id: "m3".to_string(),
                task_id: None,
                session_id: None,
                content: "late".to_string(),
            },
        ));
        fake.replay.push_back(ReplayBatch {
            events: vec![
                event(
                    1,
                    RuntimeEventKind::AssistantDelta {
                        message_id: "m1".to_string(),
                        task_id: None,
                        session_id: None,
                        content: "a".to_string(),
                    },
                ),
                event(
                    2,
                    RuntimeEventKind::AssistantDelta {
                        message_id: "m2".to_string(),
                        task_id: None,
                        session_id: None,
                        content: "b".to_string(),
                    },
                ),
                event(
                    3,
                    RuntimeEventKind::AssistantDelta {
                        message_id: "m3".to_string(),
                        task_id: None,
                        session_id: None,
                        content: "c".to_string(),
                    },
                ),
            ],
            next: EventCursor {
                stream_id: "fixture".to_string(),
                sequence: 3,
            },
            complete: true,
        });

        let mut driver = TuiClientDriver::connect(fake).expect("connect");

        assert!(matches!(driver.pump().unwrap(), PumpOutcome::Recovered(_)));
        assert_eq!(driver.view().assistant_stream, "abc");
    }

    #[test]
    fn duplicate_events_do_not_duplicate_view_facts() {
        let mut fake = FakeCoreClient::compatible();
        fake.events.push_back(event(
            1,
            RuntimeEventKind::AssistantDelta {
                message_id: "m1".to_string(),
                task_id: None,
                session_id: None,
                content: "once".to_string(),
            },
        ));
        fake.events.push_back(event(
            1,
            RuntimeEventKind::AssistantDelta {
                message_id: "m1".to_string(),
                task_id: None,
                session_id: None,
                content: "once".to_string(),
            },
        ));

        let mut driver = TuiClientDriver::connect(fake).expect("connect");

        assert!(matches!(driver.pump().unwrap(), PumpOutcome::Applied(_)));
        assert!(matches!(driver.pump().unwrap(), PumpOutcome::Idle));
        assert_eq!(driver.view().assistant_stream, "once");
    }

    #[test]
    fn failed_replay_does_not_publish_partial_view_or_cursor() {
        let mut fake = FakeCoreClient::compatible();
        fake.events.push_back(event(
            3,
            RuntimeEventKind::AssistantDelta {
                message_id: "m3".to_string(),
                task_id: None,
                session_id: None,
                content: "late".to_string(),
            },
        ));
        fake.replay.push_back(ReplayBatch {
            events: vec![
                event(
                    1,
                    RuntimeEventKind::AssistantDelta {
                        message_id: "m1".to_string(),
                        task_id: None,
                        session_id: None,
                        content: "partial".to_string(),
                    },
                ),
                event(
                    3,
                    RuntimeEventKind::AssistantDelta {
                        message_id: "m3".to_string(),
                        task_id: None,
                        session_id: None,
                        content: "invalid-gap".to_string(),
                    },
                ),
            ],
            next: EventCursor {
                stream_id: "fixture".to_string(),
                sequence: 3,
            },
            complete: true,
        });

        let mut driver = TuiClientDriver::connect(fake).expect("connect");
        let error = driver.pump().expect_err("replay must fail");

        assert!(matches!(
            error,
            TuiClientError::Core(CoreClientError::Protocol(_))
        ));
        assert_eq!(driver.cursor().sequence, 0);
        assert!(driver.view().assistant_stream.is_empty());
    }

    #[test]
    fn complete_replay_without_incoming_rolls_back_all_staged_events() {
        let mut fake = FakeCoreClient::compatible();
        fake.events.push_back(event(
            3,
            RuntimeEventKind::AssistantDelta {
                message_id: "m3".to_string(),
                task_id: None,
                session_id: None,
                content: "incoming".to_string(),
            },
        ));
        fake.replay.push_back(ReplayBatch {
            events: vec![
                event(
                    1,
                    RuntimeEventKind::AssistantDelta {
                        message_id: "m1".to_string(),
                        task_id: None,
                        session_id: None,
                        content: "a".to_string(),
                    },
                ),
                event(
                    2,
                    RuntimeEventKind::AssistantDelta {
                        message_id: "m2".to_string(),
                        task_id: None,
                        session_id: None,
                        content: "b".to_string(),
                    },
                ),
            ],
            next: EventCursor {
                stream_id: "fixture".to_string(),
                sequence: 2,
            },
            complete: true,
        });

        let mut driver = TuiClientDriver::connect(fake).expect("connect");
        let error = driver.pump().expect_err("incoming must be present");

        assert!(matches!(
            error,
            TuiClientError::Core(CoreClientError::Protocol(_))
        ));
        assert_eq!(driver.cursor().sequence, 0);
        assert!(driver.view().assistant_stream.is_empty());
    }

    fn event(sequence: u64, kind: RuntimeEventKind) -> RuntimeEventEnvelope {
        RuntimeEventEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            owner: RuntimeOwner::default(),
            cursor: EventCursor {
                stream_id: "fixture".to_string(),
                sequence,
            },
            event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
                sequence,
                Some(sequence),
                kind,
            )),
        }
    }

    fn runtime_snapshot() -> RuntimeSnapshot {
        RuntimeSnapshot {
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
        }
    }
}
