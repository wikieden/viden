use std::collections::{BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use viden_core::{
    CoreClient, CoreClientError, CoreHandshake, EventCursor, FRONTEND_SCHEMA_V1, ReplayBatch,
    ReplayRequest, RuntimeCommandEnvelope, RuntimeEvent, RuntimeEventEnvelope, RuntimeEventKind,
    RuntimeSnapshotEnvelope, RuntimeViewState, RuntimeWireEvent, TranscriptPage,
    TranscriptPageRequest, frontend_capabilities, local_core_handshake,
};

/// Shared deterministic CoreClient used by GUI integration tests.
///
/// It applies queued Core events through the canonical `RuntimeViewState`
/// reducer before publishing snapshots, so screens exercise the same boundary
/// as the production adapter without embedding a second business reducer.
#[derive(Clone)]
pub struct TestCoreClient {
    view: RuntimeViewState,
    sent: Arc<Mutex<Vec<RuntimeCommandEnvelope>>>,
    pub capabilities: BTreeSet<String>,
    events: VecDeque<Result<Option<RuntimeEventEnvelope>, CoreClientError>>,
    send_error: Option<CoreClientError>,
    stream_id: String,
    /// Replay batches served in order to `replay`, oldest first.
    replay_batches: VecDeque<ReplayBatch>,
    /// A severed transport: once set, every `recv`, `snapshot` and `discover`
    /// fails with it. Shared so a test can cut the wire after handing the
    /// client to the adapter, which is how a mid-session drop happens
    /// (E3d defect 37).
    severed: Arc<Mutex<Option<CoreClientError>>>,
}

#[derive(Clone, Debug, Default)]
pub struct TestOwner {
    pub workspace_id: String,
    pub project_id: String,
    pub lane_id: Option<String>,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub turn_id: Option<String>,
}

#[allow(dead_code)]
impl TestCoreClient {
    pub fn new(view: RuntimeViewState, sent: Arc<Mutex<Vec<RuntimeCommandEnvelope>>>) -> Self {
        Self {
            view,
            sent,
            capabilities: frontend_capabilities()
                .into_iter()
                .map(|capability| capability.0)
                .collect(),
            events: VecDeque::new(),
            send_error: None,
            stream_id: "gui-test".to_string(),
            replay_batches: VecDeque::new(),
            severed: Arc::new(Mutex::new(None)),
        }
    }

    /// The handle a test keeps so it can cut the transport mid-session.
    ///
    /// Setting it makes every later `discover`, `recv` and `snapshot` fail,
    /// which is a dropped Core rather than an absent one. Clearing it lets a
    /// reconnect succeed, so a test can drive the whole state machine.
    pub fn severed_handle(&self) -> Arc<Mutex<Option<CoreClientError>>> {
        Arc::clone(&self.severed)
    }

    fn severed_error(&self) -> Option<CoreClientError> {
        self.severed.lock().ok().and_then(|held| held.clone())
    }

    pub fn with_replay_batch(mut self, batch: ReplayBatch) -> Self {
        self.replay_batches.push_back(batch);
        self
    }

    pub fn with_stream_id(mut self, stream_id: impl Into<String>) -> Self {
        self.stream_id = stream_id.into();
        self
    }

    pub fn with_event(self, kind: RuntimeEventKind) -> Self {
        self.with_owned_event(TestOwner::default(), kind)
    }

    pub fn with_owned_event(mut self, owner: TestOwner, kind: RuntimeEventKind) -> Self {
        let sequence = self.events.len() as u64 + 1;
        let event = RuntimeEvent::new(sequence, kind);
        let mut runtime_owner = RuntimeCommandEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            client_id: String::new(),
            command_id: String::new(),
            owner: Default::default(),
            command: viden_core::RuntimeCommand::CancelActiveTurn,
        }
        .owner;
        runtime_owner.workspace_id = owner.workspace_id;
        runtime_owner.project_id = owner.project_id;
        runtime_owner.lane_id = owner.lane_id;
        runtime_owner.session_id = owner.session_id;
        runtime_owner.task_id = owner.task_id;
        runtime_owner.turn_id = owner.turn_id;
        self.events.push_back(Ok(Some(RuntimeEventEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            owner: runtime_owner,
            cursor: EventCursor {
                stream_id: self.stream_id.clone(),
                sequence,
            },
            event: RuntimeWireEvent::Known(event),
        })));
        self
    }

    pub fn with_envelope(mut self, envelope: RuntimeEventEnvelope) -> Self {
        self.events.push_back(Ok(Some(envelope)));
        self
    }

    pub fn with_gap(mut self) -> Self {
        self.events.push_back(Ok(None));
        self
    }

    pub fn with_send_error(mut self, error: CoreClientError) -> Self {
        self.send_error = Some(error);
        self
    }

    pub fn with_recv_error(mut self, error: CoreClientError) -> Self {
        self.events.push_back(Err(error));
        self
    }

    fn envelope(&self) -> RuntimeSnapshotEnvelope {
        let mut capabilities = frontend_capabilities();
        capabilities.retain(|capability| self.capabilities.contains(&capability.0));
        RuntimeSnapshotEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            capabilities,
            cursor: EventCursor {
                stream_id: self.stream_id.clone(),
                sequence: 0,
            },
            snapshot: self.view.snapshot.clone(),
            view: self.view.clone(),
        }
    }
}

impl CoreClient for TestCoreClient {
    fn discover(&mut self) -> Result<CoreHandshake, CoreClientError> {
        if let Some(error) = self.severed_error() {
            return Err(error);
        }
        // Keep each test transport's explicitly selected capability set while
        // preserving the production handshake's schema and metadata.
        let mut wire =
            serde_json::to_value(local_core_handshake()).expect("serialize local Core handshake");
        wire["capabilities"] =
            serde_json::to_value(&self.capabilities).expect("serialize test capabilities");
        serde_json::from_value(wire)
            .map_err(|error| CoreClientError::Transport(format!("invalid test handshake: {error}")))
    }

    fn send(&mut self, command: RuntimeCommandEnvelope) -> Result<(), CoreClientError> {
        if let Some(error) = self.send_error.take() {
            return Err(error);
        }
        self.sent.lock().expect("sent command lock").push(command);
        Ok(())
    }

    fn recv(
        &mut self,
        _timeout: Duration,
    ) -> Result<Option<RuntimeEventEnvelope>, CoreClientError> {
        if let Some(error) = self.severed_error() {
            return Err(error);
        }
        let Some(next) = self.events.pop_front() else {
            return Ok(None);
        };
        let Some(envelope) = next? else {
            return Ok(None);
        };
        if let RuntimeWireEvent::Known(event) = &envelope.event {
            self.view.apply_event(event);
        }
        Ok(Some(envelope))
    }

    fn snapshot(&mut self) -> Result<RuntimeSnapshotEnvelope, CoreClientError> {
        if let Some(error) = self.severed_error() {
            return Err(error);
        }
        Ok(self.envelope())
    }

    fn replay(&mut self, _request: ReplayRequest) -> Result<ReplayBatch, CoreClientError> {
        self.replay_batches
            .pop_front()
            .ok_or_else(|| CoreClientError::Transport("unused replay".into()))
    }

    fn transcript_page(
        &mut self,
        _request: TranscriptPageRequest,
    ) -> Result<TranscriptPage, CoreClientError> {
        Err(CoreClientError::Transport("unused transcript".into()))
    }
}
