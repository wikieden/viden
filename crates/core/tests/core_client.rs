use std::collections::{BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use viden_core::{
    CoreClient, CoreClientError, CoreTransport, LocalCoreTransport, StatefulCoreClient,
    frontend_capabilities,
};
use viden_provider::ModelProvider;
use viden_runtime::{RuntimeSupervisor, SessionEngine};
use viden_types::{
    CapabilityId, CoreHandshake, EventCursor, FRONTEND_SCHEMA_V1, ModelEvent, ModelRequest,
    PermissionLevel, PermissionMode, ReplayBatch, ReplayRequest, ResolvedUiPreferences,
    RuntimeCommand, RuntimeCommandEnvelope, RuntimeEvent, RuntimeEventEnvelope, RuntimeEventKind,
    RuntimeOwner, RuntimeSnapshot, RuntimeSnapshotEnvelope, RuntimeViewState, RuntimeWireEvent,
    SchemaVersion, TranscriptPage, TranscriptPageRequest, WorkMode,
};

#[derive(Clone)]
struct MockTransport {
    state: Arc<Mutex<MockTransportState>>,
}

struct MockTransportState {
    handshake: Result<CoreHandshake, CoreClientError>,
    sent: Vec<RuntimeCommandEnvelope>,
    receives: VecDeque<Result<Option<RuntimeEventEnvelope>, CoreClientError>>,
    snapshots: VecDeque<Result<RuntimeSnapshotEnvelope, CoreClientError>>,
    replays: VecDeque<Result<ReplayBatch, CoreClientError>>,
    replay_requests: Vec<ReplayRequest>,
    pages: VecDeque<Result<TranscriptPage, CoreClientError>>,
}

impl MockTransport {
    fn new() -> (Self, Arc<Mutex<MockTransportState>>) {
        let state = Arc::new(Mutex::new(MockTransportState {
            handshake: Ok(compatible_handshake()),
            sent: Vec::new(),
            receives: VecDeque::new(),
            snapshots: VecDeque::new(),
            replays: VecDeque::new(),
            replay_requests: Vec::new(),
            pages: VecDeque::new(),
        }));
        (
            Self {
                state: Arc::clone(&state),
            },
            state,
        )
    }
}

impl CoreTransport for MockTransport {
    fn discover(&mut self) -> Result<CoreHandshake, CoreClientError> {
        self.state.lock().unwrap().handshake.clone()
    }

    fn send(&mut self, command: RuntimeCommandEnvelope) -> Result<(), CoreClientError> {
        self.state.lock().unwrap().sent.push(command);
        Ok(())
    }

    fn recv(
        &mut self,
        _timeout: Duration,
    ) -> Result<Option<RuntimeEventEnvelope>, CoreClientError> {
        self.state
            .lock()
            .unwrap()
            .receives
            .pop_front()
            .unwrap_or(Ok(None))
    }

    fn snapshot(&mut self) -> Result<RuntimeSnapshotEnvelope, CoreClientError> {
        self.state
            .lock()
            .unwrap()
            .snapshots
            .pop_front()
            .unwrap_or_else(|| Err(CoreClientError::Transport("missing mock snapshot".into())))
    }

    fn replay(&mut self, request: ReplayRequest) -> Result<ReplayBatch, CoreClientError> {
        let mut state = self.state.lock().unwrap();
        state.replay_requests.push(request);
        state
            .replays
            .pop_front()
            .unwrap_or_else(|| Err(CoreClientError::Transport("missing mock replay".into())))
    }

    fn transcript_page(
        &mut self,
        _request: TranscriptPageRequest,
    ) -> Result<TranscriptPage, CoreClientError> {
        self.state
            .lock()
            .unwrap()
            .pages
            .pop_front()
            .unwrap_or_else(|| Err(CoreClientError::Transport("missing mock page".into())))
    }
}

fn compatible_handshake() -> CoreHandshake {
    CoreHandshake {
        core_version: "0.3.0-test".to_string(),
        supported_schema_versions: vec![FRONTEND_SCHEMA_V1],
        active_schema_version: FRONTEND_SCHEMA_V1,
        capabilities: frontend_capabilities(),
    }
}

fn snapshot(stream_id: &str, sequence: u64, assistant_stream: &str) -> RuntimeSnapshotEnvelope {
    let snapshot = runtime_snapshot();
    let mut view = RuntimeViewState::new(snapshot.clone());
    view.assistant_stream = assistant_stream.to_string();
    RuntimeSnapshotEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        capabilities: frontend_capabilities(),
        cursor: cursor(stream_id, sequence),
        snapshot,
        view,
    }
}

fn runtime_snapshot() -> RuntimeSnapshot {
    RuntimeSnapshot {
        cwd: PathBuf::from("/tmp/viden-core-client"),
        provider_family: "test".to_string(),
        model_label: "test-model".to_string(),
        work_mode: WorkMode::Build,
        permission_mode: PermissionMode::Default,
        permission_level: PermissionLevel::Ask,
        config_summary: "test".to_string(),
        loaded_config_files: Vec::new(),
        startup_overrides: Vec::new(),
        ui_preferences: ResolvedUiPreferences::default(),
    }
}

fn cursor(stream_id: &str, sequence: u64) -> EventCursor {
    EventCursor {
        stream_id: stream_id.to_string(),
        sequence,
    }
}

fn known_event(stream_id: &str, sequence: u64, content: &str) -> RuntimeEventEnvelope {
    RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: RuntimeOwner::default(),
        cursor: cursor(stream_id, sequence),
        event: RuntimeWireEvent::Known(RuntimeEvent::new(
            sequence,
            RuntimeEventKind::AssistantDelta {
                message_id: format!("message-{sequence}"),
                task_id: None,
                session_id: None,
                content: content.to_string(),
            },
        )),
    }
}

fn unknown_event(stream_id: &str, sequence: u64) -> RuntimeEventEnvelope {
    RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: RuntimeOwner::default(),
        cursor: cursor(stream_id, sequence),
        event: RuntimeWireEvent::Unknown {
            event_type: "future_event".to_string(),
            payload: Default::default(),
        },
    }
}

fn command(schema_version: SchemaVersion) -> RuntimeCommandEnvelope {
    RuntimeCommandEnvelope {
        schema_version,
        client_id: "test-client".to_string(),
        command_id: "command-1".to_string(),
        owner: RuntimeOwner::default(),
        command: RuntimeCommand::QueueFollowUp {
            content: "continue".to_string(),
        },
    }
}

fn discovered_client(
    transport: MockTransport,
) -> Result<StatefulCoreClient<MockTransport>, CoreClientError> {
    let mut client = StatefulCoreClient::new(transport);
    client.discover()?;
    Ok(client)
}

#[test]
fn core_client_is_object_safe_and_requires_discovery_before_operations() {
    let (transport, state) = MockTransport::new();
    let mut client = StatefulCoreClient::new(transport);

    assert_eq!(
        client.send(command(FRONTEND_SCHEMA_V1)),
        Err(CoreClientError::HandshakeRequired)
    );
    assert_eq!(
        client.recv(Duration::ZERO),
        Err(CoreClientError::HandshakeRequired)
    );
    assert_eq!(client.snapshot(), Err(CoreClientError::HandshakeRequired));
    assert_eq!(
        client.replay(ReplayRequest {
            after: cursor("stream-a", 0),
            limit: 1,
        }),
        Err(CoreClientError::HandshakeRequired)
    );
    assert_eq!(
        client.transcript_page(TranscriptPageRequest {
            session_id: "session-a".to_string(),
            before: None,
            limit: 1,
        }),
        Err(CoreClientError::HandshakeRequired)
    );

    client.discover().unwrap();
    client.send(command(FRONTEND_SCHEMA_V1)).unwrap();
    assert_eq!(state.lock().unwrap().sent.len(), 1);

    let boxed: Box<dyn CoreClient> = Box::new(client);
    drop(boxed);
}

#[test]
fn core_client_applies_next_event_and_ignores_duplicate_cursor() {
    let (transport, state) = MockTransport::new();
    {
        let mut state = state.lock().unwrap();
        state.snapshots.push_back(Ok(snapshot("stream-a", 0, "")));
        state
            .receives
            .push_back(Ok(Some(known_event("stream-a", 1, "one"))));
        state
            .receives
            .push_back(Ok(Some(known_event("stream-a", 1, "duplicate"))));
    }
    let mut client = discovered_client(transport).unwrap();
    client.snapshot().unwrap();

    assert!(client.recv(Duration::ZERO).unwrap().is_some());
    assert_eq!(client.confirmed_cursor(), Some(&cursor("stream-a", 1)));
    assert_eq!(client.confirmed_view().unwrap().assistant_stream, "one");
    assert!(client.recv(Duration::ZERO).unwrap().is_none());
    assert_eq!(client.confirmed_cursor(), Some(&cursor("stream-a", 1)));
    assert_eq!(client.confirmed_view().unwrap().assistant_stream, "one");
}

#[test]
fn core_client_gap_replays_missing_and_incoming_event_exactly_once() {
    let (transport, state) = MockTransport::new();
    {
        let mut state = state.lock().unwrap();
        state.snapshots.push_back(Ok(snapshot("stream-a", 0, "")));
        state
            .receives
            .push_back(Ok(Some(known_event("stream-a", 1, "one"))));
        state
            .receives
            .push_back(Ok(Some(known_event("stream-a", 3, "three"))));
        state.replays.push_back(Ok(ReplayBatch {
            events: vec![
                known_event("stream-a", 2, "two"),
                known_event("stream-a", 3, "three"),
            ],
            next: cursor("stream-a", 3),
            complete: true,
        }));
    }
    let mut client = discovered_client(transport).unwrap();
    client.snapshot().unwrap();
    client.recv(Duration::ZERO).unwrap();

    let delivered = client.recv(Duration::ZERO).unwrap().unwrap();
    assert_eq!(delivered.cursor, cursor("stream-a", 3));
    assert_eq!(client.confirmed_cursor(), Some(&cursor("stream-a", 3)));
    assert_eq!(
        client.confirmed_view().unwrap().assistant_stream,
        "onetwothree"
    );
    assert_eq!(
        state.lock().unwrap().replay_requests,
        vec![ReplayRequest {
            after: cursor("stream-a", 1),
            limit: 500,
        }]
    );
}

#[test]
fn core_client_gap_consumes_multiple_replay_batches_until_complete() {
    let (transport, state) = MockTransport::new();
    {
        let mut state = state.lock().unwrap();
        state
            .snapshots
            .push_back(Ok(snapshot("stream-a", 1, "one")));
        state
            .receives
            .push_back(Ok(Some(known_event("stream-a", 4, "four"))));
        state.replays.push_back(Ok(ReplayBatch {
            events: vec![known_event("stream-a", 2, "two")],
            next: cursor("stream-a", 2),
            complete: false,
        }));
        state.replays.push_back(Ok(ReplayBatch {
            events: vec![
                known_event("stream-a", 3, "three"),
                known_event("stream-a", 4, "four"),
            ],
            next: cursor("stream-a", 4),
            complete: true,
        }));
    }
    let mut client = discovered_client(transport).unwrap();
    client.snapshot().unwrap();

    let delivered = client.recv(Duration::ZERO).unwrap().unwrap();
    assert_eq!(delivered.cursor, cursor("stream-a", 4));
    assert_eq!(
        client.confirmed_view().unwrap().assistant_stream,
        "onetwothreefour"
    );
    assert_eq!(
        state.lock().unwrap().replay_requests,
        vec![
            ReplayRequest {
                after: cursor("stream-a", 1),
                limit: 500,
            },
            ReplayRequest {
                after: cursor("stream-a", 2),
                limit: 500,
            },
        ]
    );
}

#[test]
fn core_client_retention_and_stream_mismatch_replace_state_from_snapshot() {
    let (transport, state) = MockTransport::new();
    {
        let mut state = state.lock().unwrap();
        state
            .snapshots
            .push_back(Ok(snapshot("stream-a", 1, "one")));
        state
            .snapshots
            .push_back(Ok(snapshot("stream-b", 4, "rebuilt")));
        state
            .snapshots
            .push_back(Ok(snapshot("stream-c", 8, "restarted")));
        state
            .receives
            .push_back(Ok(Some(known_event("stream-a", 3, "gap"))));
        state
            .replays
            .push_back(Err(CoreClientError::SnapshotRequired {
                reason_code: "retention_expired".to_string(),
            }));
        state
            .receives
            .push_back(Ok(Some(known_event("other-stream", 9, "stale"))));
    }
    let mut client = discovered_client(transport).unwrap();
    client.snapshot().unwrap();

    assert!(client.recv(Duration::ZERO).unwrap().is_none());
    assert_eq!(client.confirmed_cursor(), Some(&cursor("stream-b", 4)));
    assert_eq!(client.confirmed_view().unwrap().assistant_stream, "rebuilt");

    // A mismatched envelope is not returned as confirmed after the snapshot supersedes it.
    assert!(client.recv(Duration::ZERO).unwrap().is_none());
    assert_eq!(client.confirmed_cursor(), Some(&cursor("stream-c", 8)));
    assert_eq!(
        client.confirmed_view().unwrap().assistant_stream,
        "restarted"
    );
}

#[test]
fn core_client_unknown_event_advances_cursor_without_mutating_view() {
    let (transport, state) = MockTransport::new();
    {
        let mut state = state.lock().unwrap();
        state
            .snapshots
            .push_back(Ok(snapshot("stream-a", 0, "stable")));
        state
            .receives
            .push_back(Ok(Some(unknown_event("stream-a", 1))));
    }
    let mut client = discovered_client(transport).unwrap();
    client.snapshot().unwrap();
    let before = client.confirmed_view().unwrap().clone();

    assert!(client.recv(Duration::ZERO).unwrap().is_some());
    assert_eq!(client.confirmed_cursor(), Some(&cursor("stream-a", 1)));
    assert_eq!(client.confirmed_view(), Some(&before));
}

#[test]
fn core_client_recovery_failures_leave_last_confirmed_state_unchanged() {
    let (transport, state) = MockTransport::new();
    {
        let mut state = state.lock().unwrap();
        state
            .snapshots
            .push_back(Ok(snapshot("stream-a", 1, "one")));
        state
            .receives
            .push_back(Ok(Some(known_event("stream-a", 4, "four"))));
        state.replays.push_back(Ok(ReplayBatch {
            events: vec![known_event("stream-a", 2, "two")],
            next: cursor("stream-a", 2),
            complete: false,
        }));
        state
            .replays
            .push_back(Err(CoreClientError::Transport("replay failed".to_string())));
        state
            .receives
            .push_back(Ok(Some(known_event("other-stream", 2, "other"))));
        state.snapshots.push_back(Err(CoreClientError::Transport(
            "snapshot failed".to_string(),
        )));
    }
    let mut client = discovered_client(transport).unwrap();
    client.snapshot().unwrap();
    let before_view = client.confirmed_view().unwrap().clone();
    let before_cursor = client.confirmed_cursor().unwrap().clone();

    assert_eq!(
        client.recv(Duration::ZERO),
        Err(CoreClientError::Transport("replay failed".to_string()))
    );
    assert_eq!(client.confirmed_view(), Some(&before_view));
    assert_eq!(client.confirmed_cursor(), Some(&before_cursor));

    assert_eq!(
        client.recv(Duration::ZERO),
        Err(CoreClientError::Transport("snapshot failed".to_string()))
    );
    assert_eq!(client.confirmed_view(), Some(&before_view));
    assert_eq!(client.confirmed_cursor(), Some(&before_cursor));
}

#[test]
fn core_client_replay_compatibility_failure_does_not_commit_staged_events() {
    let (transport, state) = MockTransport::new();
    let mut incompatible = known_event("stream-a", 3, "three");
    incompatible.schema_version = SchemaVersion(2);
    {
        let mut state = state.lock().unwrap();
        state
            .snapshots
            .push_back(Ok(snapshot("stream-a", 1, "one")));
        state
            .receives
            .push_back(Ok(Some(known_event("stream-a", 3, "three"))));
        state.replays.push_back(Ok(ReplayBatch {
            events: vec![known_event("stream-a", 2, "two"), incompatible],
            next: cursor("stream-a", 3),
            complete: true,
        }));
    }
    let mut client = discovered_client(transport).unwrap();
    client.snapshot().unwrap();

    assert!(matches!(
        client.recv(Duration::ZERO),
        Err(CoreClientError::Compatibility(_))
    ));
    assert_eq!(client.confirmed_cursor(), Some(&cursor("stream-a", 1)));
    assert_eq!(client.confirmed_view().unwrap().assistant_stream, "one");
}

#[test]
fn core_client_validates_handshake_command_snapshot_and_event_schema() {
    let (transport, state) = MockTransport::new();
    state.lock().unwrap().handshake = Ok(CoreHandshake {
        core_version: "future".to_string(),
        supported_schema_versions: vec![SchemaVersion(2)],
        active_schema_version: FRONTEND_SCHEMA_V1,
        capabilities: frontend_capabilities(),
    });
    let mut client = StatefulCoreClient::new(transport);
    assert!(matches!(
        client.discover(),
        Err(CoreClientError::Compatibility(_))
    ));
    assert_eq!(
        client.send(command(FRONTEND_SCHEMA_V1)),
        Err(CoreClientError::HandshakeRequired)
    );

    let (transport, state) = MockTransport::new();
    let mut bad_snapshot = snapshot("stream-a", 0, "bad");
    bad_snapshot.schema_version = SchemaVersion(2);
    {
        let mut state = state.lock().unwrap();
        state.snapshots.push_back(Ok(bad_snapshot));
    }
    let mut client = discovered_client(transport).unwrap();
    assert!(matches!(
        client.snapshot(),
        Err(CoreClientError::Compatibility(_))
    ));
    assert!(client.confirmed_view().is_none());
    assert!(matches!(
        client.send(command(SchemaVersion(2))),
        Err(CoreClientError::Compatibility(_))
    ));

    let (transport, state) = MockTransport::new();
    let mut bad_event = known_event("stream-a", 1, "bad");
    bad_event.schema_version = SchemaVersion(2);
    {
        let mut state = state.lock().unwrap();
        state
            .snapshots
            .push_back(Ok(snapshot("stream-a", 0, "stable")));
        state.receives.push_back(Ok(Some(bad_event)));
    }
    let mut client = discovered_client(transport).unwrap();
    client.snapshot().unwrap();
    assert!(matches!(
        client.recv(Duration::ZERO),
        Err(CoreClientError::Compatibility(_))
    ));
    assert_eq!(client.confirmed_cursor(), Some(&cursor("stream-a", 0)));
    assert_eq!(client.confirmed_view().unwrap().assistant_stream, "stable");

    let (transport, state) = MockTransport::new();
    let mut mismatched_sequence = known_event("stream-a", 1, "bad");
    if let RuntimeWireEvent::Known(event) = &mut mismatched_sequence.event {
        event.sequence = 99;
    }
    {
        let mut state = state.lock().unwrap();
        state
            .snapshots
            .push_back(Ok(snapshot("stream-a", 0, "stable")));
        state.receives.push_back(Ok(Some(mismatched_sequence)));
    }
    let mut client = discovered_client(transport).unwrap();
    client.snapshot().unwrap();
    assert!(matches!(
        client.recv(Duration::ZERO),
        Err(CoreClientError::Protocol(_))
    ));
    assert_eq!(client.confirmed_cursor(), Some(&cursor("stream-a", 0)));
    assert_eq!(client.confirmed_view().unwrap().assistant_stream, "stable");
}

#[test]
fn core_client_rejects_non_contiguous_replay_without_committing_staged_state() {
    let (transport, state) = MockTransport::new();
    {
        let mut state = state.lock().unwrap();
        state
            .snapshots
            .push_back(Ok(snapshot("stream-a", 1, "one")));
        state
            .receives
            .push_back(Ok(Some(known_event("stream-a", 4, "four"))));
        state.replays.push_back(Ok(ReplayBatch {
            events: vec![
                known_event("stream-a", 2, "two"),
                known_event("stream-a", 4, "four"),
            ],
            next: cursor("stream-a", 4),
            complete: true,
        }));
    }
    let mut client = discovered_client(transport).unwrap();
    client.snapshot().unwrap();

    assert!(matches!(
        client.recv(Duration::ZERO),
        Err(CoreClientError::Protocol(message)) if message.contains("non-contiguous")
    ));
    assert_eq!(client.confirmed_cursor(), Some(&cursor("stream-a", 1)));
    assert_eq!(client.confirmed_view().unwrap().assistant_stream, "one");
}

struct DoneProvider;

impl ModelProvider for DoneProvider {
    fn provider_name(&self) -> &str {
        "test"
    }

    fn model(&self) -> &str {
        "test-model"
    }

    fn set_model(&mut self, _model: String) {}

    fn next_events(&mut self, _request: &ModelRequest) -> Result<Vec<ModelEvent>, String> {
        Ok(vec![ModelEvent::Done])
    }
}

fn temp_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("viden-core-{label}-{unique}"));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn core_client_local_transport_supports_full_boundary_surface() {
    let cwd = temp_dir("local-cwd");
    let home = temp_dir("local-home");
    let engine =
        SessionEngine::new_with_home(&cwd, Box::new(DoneProvider), Some(home.clone())).unwrap();
    let session_id = engine.session_id().to_string();
    let supervisor = RuntimeSupervisor::start(engine);
    let mut transport = LocalCoreTransport::new(supervisor);
    let mut startup_events = Vec::new();
    while let Some(event) = CoreTransport::recv(&mut transport, Duration::ZERO).unwrap() {
        startup_events.push(event);
    }
    assert!(startup_events.iter().any(|event| {
        matches!(
            &event.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::WorkspaceSourceUpdated { .. },
                ..
            })
        )
    }));
    let mut client: Box<dyn CoreClient> = Box::new(StatefulCoreClient::new(transport));

    let handshake = client.discover().unwrap();
    assert_eq!(handshake.active_schema_version, FRONTEND_SCHEMA_V1);
    let initial = client.snapshot().unwrap();
    assert_eq!(
        initial.cursor.sequence,
        startup_events.last().unwrap().cursor.sequence
    );
    assert!(initial.view.workspace_source.is_some());

    client.send(command(FRONTEND_SCHEMA_V1)).unwrap();
    let event = client.recv(Duration::from_secs(1)).unwrap().unwrap();
    assert_eq!(event.cursor.sequence, initial.cursor.sequence + 1);

    let replay = client
        .replay(ReplayRequest {
            after: initial.cursor,
            limit: 10,
        })
        .unwrap();
    assert!(!replay.events.is_empty());
    let snapshot = client.snapshot().unwrap();
    assert!(snapshot.cursor.sequence >= event.cursor.sequence);

    let page = client
        .transcript_page(TranscriptPageRequest {
            session_id,
            before: None,
            limit: 10,
        })
        .unwrap();
    assert!(page.rows.len() <= 10);

    drop(client);
    std::fs::remove_dir_all(cwd).unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn core_client_handshake_rejects_missing_required_capability() {
    let (transport, state) = MockTransport::new();
    state.lock().unwrap().handshake = Ok(CoreHandshake {
        core_version: "0.3.0-test".to_string(),
        supported_schema_versions: vec![FRONTEND_SCHEMA_V1],
        active_schema_version: FRONTEND_SCHEMA_V1,
        capabilities: BTreeSet::from([CapabilityId("runtime.events".to_string())]),
    });
    let mut client = StatefulCoreClient::new(transport);
    assert!(matches!(
        client.discover(),
        Err(CoreClientError::Compatibility(_))
    ));
}

/// Every view a frontend has to name is reachable through the facade.
///
/// `viden-core` is the stable multi-frontend boundary: a frontend that has to
/// reach into `viden-types` for a type Core publishes on its own contract has
/// found a hole in that boundary, not a shortcut. T4 hit exactly that for the
/// standing-approval and starter-Lane session views.
#[test]
fn the_facade_re_exports_every_view_a_frontend_must_name() {
    let standing: viden_core::StandingApprovalView = viden_core::StandingApprovalView {
        rule_id: "rule-facade".to_string(),
        tool_name: "shell".to_string(),
        rule_content: "cargo fmt".to_string(),
        granted_at: 1,
        granted_in_workspace: "workspace-facade".to_string(),
        audit_id: "audit-facade".to_string(),
    };
    assert_eq!(standing.rule_id, "rule-facade");

    let binding: viden_core::StarterLaneAgentBinding = viden_core::StarterLaneAgentBinding {
        agent_id: "codex".to_string(),
        model: None,
        task: None,
    };
    assert_eq!(binding.agent_id, "codex");

    let outcome: viden_core::StarterLaneSessionOutcome =
        viden_core::StarterLaneSessionOutcome::Refused {
            reason: "no adapter".to_string(),
        };
    assert!(matches!(
        outcome,
        viden_core::StarterLaneSessionOutcome::Refused { .. }
    ));

    // The typed input an editable approval now publishes (GUI-CORE-036) is
    // named by `ApprovalRequestView.input`, so its type must be nameable too.
    let mut input = viden_core::ToolInput::new();
    input.insert("command".to_string(), "cargo fmt".to_string());
    assert_eq!(input.len(), 1);
}
