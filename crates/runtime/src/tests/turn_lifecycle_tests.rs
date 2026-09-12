//! Turn liveness for the native path, and the queue that hangs off it
//! (`runtime.turn_lifecycle`, C6).
//!
//! Two compatibility follow-ups meet here. Follow-up 3: a built-in turn
//! published no terminal fact, so a client's "is work running" predicate fell
//! back to display residue. Follow-up 5: `QueueFollowUp` pushed onto
//! `SessionEngine::queued_runtime_inputs` and nothing ever removed or ran it,
//! so a queued prompt was a fact Core published and never acted on.
//!
//! What these tests hold in place:
//!
//! 1. every native exit — completed, failed, cancelled — ends with
//!    `TurnFinished` as the last fact, and starts with `TurnStarted`
//!    immediately after `CommandAccepted`;
//! 2. a completed turn drains the session queue oldest first, each drained
//!    entry announced by `InputDequeued` and run as its own bracketed turn;
//! 3. a turn that did not complete leaves the queue exactly where it is, and
//!    the snapshot prefix still re-lists it;
//! 4. a turn is never resumed across a restart: a fresh engine over the same
//!    workspace has no active turn at all.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use viden_provider::{ModelProvider, ModelRequestControl};
use viden_types::{
    ApprovalResponse, ModelEvent, ModelRequest, RuntimeCommand, RuntimeEvent, RuntimeEventKind,
    RuntimeOwner, TurnOutcome, TurnSource,
};

use super::{SequenceProvider, temp_dir};
use crate::{RuntimeSupervisor, SessionEngine, mint_workspace_owner_binding};

/// A provider whose every turn fails, so the engine takes the error exit.
struct FailingProvider {
    model: String,
}

impl ModelProvider for FailingProvider {
    fn provider_name(&self) -> &str {
        "failing"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn set_model(&mut self, model: String) {
        self.model = model;
    }

    fn next_events(&mut self, _request: &ModelRequest) -> Result<Vec<ModelEvent>, String> {
        Err("provider refused the request".to_string())
    }
}

/// A provider that parks inside the model call until it is cancelled, so a
/// cancellation can be observed while the turn is genuinely running.
struct BlockingProvider {
    model: String,
    entered: Arc<AtomicBool>,
}

impl ModelProvider for BlockingProvider {
    fn provider_name(&self) -> &str {
        "blocking"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn set_model(&mut self, model: String) {
        self.model = model;
    }

    fn next_events(&mut self, _request: &ModelRequest) -> Result<Vec<ModelEvent>, String> {
        Ok(Vec::new())
    }

    fn next_events_with_control(
        &mut self,
        _request: &ModelRequest,
        control: &ModelRequestControl,
    ) -> Result<Vec<ModelEvent>, String> {
        self.entered.store(true, Ordering::SeqCst);
        loop {
            control.check_cancelled()?;
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// A provider that counts the prompts it was actually asked to answer, so a
/// drained queue can be checked against what ran rather than only against what
/// was published.
struct RecordingProvider {
    model: String,
    prompts: Arc<std::sync::Mutex<Vec<String>>>,
    turns: Arc<AtomicU64>,
}

impl ModelProvider for RecordingProvider {
    fn provider_name(&self) -> &str {
        "recording"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn set_model(&mut self, model: String) {
        self.model = model;
    }

    fn next_events(&mut self, request: &ModelRequest) -> Result<Vec<ModelEvent>, String> {
        self.turns.fetch_add(1, Ordering::SeqCst);
        if let Some(last) = request.messages.last() {
            self.prompts.lock().unwrap().push(last.content.clone());
        }
        Ok(vec![ModelEvent::Done])
    }
}

fn bound_engine(name: &str, provider: Box<dyn ModelProvider>) -> SessionEngine {
    let cwd = temp_dir(&format!("{name}_cwd"));
    let home = temp_dir(&format!("{name}_home"));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine.bind_workspace_owner(mint_workspace_owner_binding(&cwd).unwrap());
    engine
}

fn started_turns(events: &[RuntimeEvent]) -> Vec<(String, TurnSource, RuntimeOwner)> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::TurnStarted { turn } => Some((
                turn.turn_id.clone(),
                turn.source.clone(),
                turn.owner.clone(),
            )),
            _ => None,
        })
        .collect()
}

fn finished_turns(events: &[RuntimeEvent]) -> Vec<(String, TurnOutcome)> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::TurnFinished {
                turn_id, outcome, ..
            } => Some((turn_id.clone(), outcome.clone())),
            _ => None,
        })
        .collect()
}

fn dequeued_ids(events: &[RuntimeEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::InputDequeued { input_id } => Some(input_id.clone()),
            _ => None,
        })
        .collect()
}

fn position(events: &[RuntimeEvent], matcher: impl Fn(&RuntimeEventKind) -> bool) -> usize {
    events
        .iter()
        .position(|event| matcher(&event.kind))
        .expect("the fixture event is present")
}

// --- the native bracket, per exit path ---------------------------------------

/// The completed exit. `TurnStarted` is the fact right after `CommandAccepted`
/// and `TurnFinished` is the last fact of the whole batch — after the trailing
/// snapshot, which is deliberately no longer what a client reads as the end.
#[test]
fn a_completed_native_turn_opens_after_the_acceptance_and_closes_the_batch() {
    let mut engine = bound_engine(
        "turn_lifecycle_completed",
        Box::new(SequenceProvider::new(vec![vec![ModelEvent::Done]])),
    );
    let workspace = engine.workspace_owner().cloned().unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let events = engine
        .handle_runtime_command(
            "cmd_input",
            RuntimeCommand::SubmitUserInput {
                content: "say something".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    let accepted = position(
        &events,
        |kind| matches!(kind, RuntimeEventKind::CommandAccepted { command_id, .. } if command_id == "cmd_input"),
    );
    let started = position(&events, |kind| {
        matches!(kind, RuntimeEventKind::TurnStarted { .. })
    });
    assert_eq!(
        started,
        accepted + 1,
        "the turn must open immediately after its acceptance"
    );
    assert!(
        matches!(
            events.last().map(|event| &event.kind),
            Some(RuntimeEventKind::TurnFinished { .. })
        ),
        "the turn's end must be the last fact of the batch"
    );

    let opened = started_turns(&events);
    let closed = finished_turns(&events);
    assert_eq!(opened.len(), 1);
    assert_eq!(closed.len(), 1);
    assert_eq!(opened[0].1, TurnSource::UserInput);
    assert_eq!(closed[0].0, opened[0].0);
    assert_eq!(closed[0].1, TurnOutcome::Completed);

    // The turn owner is the published workspace identity plus this turn's own
    // id: the scope a client matches on, and a turn id an audit row can join.
    assert_eq!(opened[0].2.workspace_id, workspace.workspace_id);
    assert_eq!(opened[0].2.project_id, workspace.project_id);
    assert_eq!(opened[0].2.lane_id, None);
    assert_eq!(opened[0].2.turn_id.as_deref(), Some(opened[0].0.as_str()));
}

/// The failed exit. The existing `Error` fact stays exactly where it was — a
/// client that renders errors is unchanged — and the turn's end follows it, so
/// the bracket closes even when the turn did not finish its work.
#[test]
fn a_failed_native_turn_closes_as_failed_after_the_error() {
    let mut engine = bound_engine(
        "turn_lifecycle_failed",
        Box::new(FailingProvider {
            model: "test-model".to_string(),
        }),
    );
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let events = engine
        .handle_runtime_command(
            "cmd_input",
            RuntimeCommand::SubmitUserInput {
                content: "say something".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    let error = position(&events, |kind| {
        matches!(kind, RuntimeEventKind::Error { .. })
    });
    let finished = position(&events, |kind| {
        matches!(kind, RuntimeEventKind::TurnFinished { .. })
    });
    assert!(finished > error, "the error keeps its place before the end");
    assert_eq!(finished, events.len() - 1);

    let closed = finished_turns(&events);
    assert_eq!(closed.len(), 1);
    let TurnOutcome::Failed { reason } = &closed[0].1 else {
        panic!("a provider error must close the turn as failed: {closed:?}");
    };
    assert!(reason.contains("provider refused the request"), "{reason}");
}

// --- the queue drain ---------------------------------------------------------

/// Follow-up 5, closed. Two queued prompts run oldest first behind a completed
/// turn, each announced by `InputDequeued` and bracketed as its own turn that
/// names the queue entry it came from.
#[test]
fn a_completed_native_turn_drains_the_session_queue_oldest_first() {
    let prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
    let turns = Arc::new(AtomicU64::new(0));
    let mut engine = bound_engine(
        "turn_lifecycle_drain",
        Box::new(RecordingProvider {
            model: "test-model".to_string(),
            prompts: Arc::clone(&prompts),
            turns: Arc::clone(&turns),
        }),
    );
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    for (command_id, content) in [("cmd_q1", "queued one"), ("cmd_q2", "queued two")] {
        engine
            .handle_runtime_command(
                command_id,
                RuntimeCommand::QueueFollowUp {
                    content: content.to_string(),
                },
                &mut approver,
            )
            .unwrap();
    }
    let queued_ids = engine
        .runtime_view_state()
        .queued_inputs
        .iter()
        .map(|input| input.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(queued_ids.len(), 2);

    let events = engine
        .handle_runtime_command(
            "cmd_input",
            RuntimeCommand::SubmitUserInput {
                content: "typed one".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    // Three turns ran: the typed one and both queued ones, in that order.
    assert_eq!(turns.load(Ordering::SeqCst), 3);
    let asked = prompts.lock().unwrap().clone();
    assert_eq!(asked.len(), 3);
    assert!(asked[0].contains("typed one"), "{asked:?}");
    assert!(asked[1].contains("queued one"), "{asked:?}");
    assert!(asked[2].contains("queued two"), "{asked:?}");

    assert_eq!(dequeued_ids(&events), queued_ids);
    let opened = started_turns(&events);
    assert_eq!(opened.len(), 3);
    assert_eq!(opened[0].1, TurnSource::UserInput);
    assert_eq!(
        opened[1].1,
        TurnSource::QueuedInput {
            input_id: queued_ids[0].clone()
        }
    );
    assert_eq!(
        opened[2].1,
        TurnSource::QueuedInput {
            input_id: queued_ids[1].clone()
        }
    );
    let closed = finished_turns(&events);
    assert_eq!(closed.len(), 3);
    assert!(
        closed
            .iter()
            .all(|(_, outcome)| *outcome == TurnOutcome::Completed)
    );

    // Each drain is announced before the turn it starts, so a client never
    // sees a turn quoting a queue entry it has not yet been told left the
    // queue.
    let first_dequeue = position(
        &events,
        |kind| matches!(kind, RuntimeEventKind::InputDequeued { input_id } if *input_id == queued_ids[0]),
    );
    let first_queued_turn = position(&events, |kind| {
        matches!(
            kind,
            RuntimeEventKind::TurnStarted { turn }
                if turn.source == TurnSource::QueuedInput { input_id: queued_ids[0].clone() }
        )
    });
    assert!(first_dequeue < first_queued_turn);

    assert!(
        engine.runtime_view_state().queued_inputs.is_empty(),
        "a drained queue is empty and the snapshot prefix says so"
    );
}

/// Nothing runs behind a turn that did not complete. The queue is kept, and
/// the snapshot prefix still re-lists it, so an operator can see exactly what
/// is waiting rather than discover it was silently discarded or silently run.
#[test]
fn a_failed_native_turn_keeps_the_session_queue() {
    let mut engine = bound_engine(
        "turn_lifecycle_failed_queue",
        Box::new(FailingProvider {
            model: "test-model".to_string(),
        }),
    );
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    engine
        .handle_runtime_command(
            "cmd_q1",
            RuntimeCommand::QueueFollowUp {
                content: "queued one".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    let events = engine
        .handle_runtime_command(
            "cmd_input",
            RuntimeCommand::SubmitUserInput {
                content: "typed one".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    assert!(dequeued_ids(&events).is_empty());
    assert_eq!(started_turns(&events).len(), 1);
    let view = engine.runtime_view_state();
    assert_eq!(view.queued_inputs.len(), 1);
    assert_eq!(view.queued_inputs[0].content_preview, "queued one");
}

/// A turn is never resumed across a restart. The snapshot prefix carries the
/// facts Core can still stand behind — the queue it holds — and no turn at
/// all, because a turn that was running when the process stopped is not
/// running now.
#[test]
fn a_snapshot_prefix_carries_no_turn_and_a_fresh_engine_has_none_either() {
    let cwd = temp_dir("turn_lifecycle_restart_cwd");
    let home = temp_dir("turn_lifecycle_restart_home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(vec![vec![ModelEvent::Done]])),
        Some(home.clone()),
    )
    .unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    engine
        .handle_runtime_command(
            "cmd_q1",
            RuntimeCommand::QueueFollowUp {
                content: "queued one".to_string(),
            },
            &mut approver,
        )
        .unwrap();

    // The prefix this engine would replay to a reconnecting client.
    let view = engine.runtime_view_state();
    assert!(view.active_turns.is_empty());
    assert_eq!(view.queued_inputs.len(), 1);

    let restarted = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(vec![vec![ModelEvent::Done]])),
        Some(home),
    )
    .unwrap();
    assert!(restarted.runtime_view_state().active_turns.is_empty());
}

// --- the supervised path -----------------------------------------------------

fn collect_until(
    supervisor: &RuntimeSupervisor,
    timeout: Duration,
    done: impl Fn(&[RuntimeEvent]) -> bool,
) -> Vec<RuntimeEvent> {
    let deadline = std::time::Instant::now() + timeout;
    let mut events = Vec::new();
    while std::time::Instant::now() < deadline {
        while let Some(envelope) = supervisor
            .recv_event_envelope(Duration::from_millis(20))
            .unwrap_or(None)
        {
            if let viden_types::RuntimeWireEvent::Known(event) = envelope.event {
                events.push(event);
            }
        }
        if done(&events) {
            break;
        }
    }
    events
}

/// The same bracket over the supervised path, which is the one both clients
/// actually drive, plus the drain behind it.
#[test]
fn a_supervised_native_turn_brackets_itself_and_drains_what_was_already_queued() {
    let cwd = temp_dir("turn_lifecycle_supervised_cwd");
    let home = temp_dir("turn_lifecycle_supervised_home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::<Vec<ModelEvent>>::new())),
        Some(home),
    )
    .unwrap();
    engine.bind_workspace_owner(mint_workspace_owner_binding(&cwd).unwrap());
    let supervisor = RuntimeSupervisor::start(engine);

    supervisor
        .send_command(
            "cmd_queue",
            RuntimeCommand::QueueFollowUp {
                content: "queued one".to_string(),
            },
        )
        .unwrap();
    supervisor
        .send_command(
            "cmd_input",
            RuntimeCommand::SubmitUserInput {
                content: "typed one".to_string(),
            },
        )
        .unwrap();

    let events = collect_until(&supervisor, Duration::from_secs(10), |events| {
        finished_turns(events).len() >= 2
    });

    let opened = started_turns(&events);
    let closed = finished_turns(&events);
    assert_eq!(opened.len(), 2, "{opened:?}");
    assert_eq!(opened[0].1, TurnSource::UserInput);
    assert!(matches!(opened[1].1, TurnSource::QueuedInput { .. }));
    assert_eq!(closed.len(), 2);
    assert!(
        closed
            .iter()
            .all(|(_, outcome)| *outcome == TurnOutcome::Completed)
    );
    assert_eq!(dequeued_ids(&events).len(), 1);
}

/// A `QueueFollowUp` that arrives while the turn runs cannot be drained by
/// that turn: the supervisor is one worker and the command is still in its
/// channel when the turn ends. It is drained as soon as it lands, because the
/// completed turn left the session queue draining — which is the behavior an
/// operator sees, rather than a prompt that waits for an unrelated later turn.
#[test]
fn a_follow_up_queued_after_a_completed_turn_still_runs() {
    let cwd = temp_dir("turn_lifecycle_late_queue_cwd");
    let home = temp_dir("turn_lifecycle_late_queue_home");
    let engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::<Vec<ModelEvent>>::new())),
        Some(home),
    )
    .unwrap();
    let supervisor = RuntimeSupervisor::start(engine);

    supervisor
        .send_command(
            "cmd_input",
            RuntimeCommand::SubmitUserInput {
                content: "typed one".to_string(),
            },
        )
        .unwrap();
    let first = collect_until(&supervisor, Duration::from_secs(10), |events| {
        !finished_turns(events).is_empty()
    });
    assert_eq!(finished_turns(&first).len(), 1);

    supervisor
        .send_command(
            "cmd_queue",
            RuntimeCommand::QueueFollowUp {
                content: "queued one".to_string(),
            },
        )
        .unwrap();
    let second = collect_until(&supervisor, Duration::from_secs(10), |events| {
        !finished_turns(events).is_empty()
    });

    let opened = started_turns(&second);
    assert_eq!(opened.len(), 1, "{opened:?}");
    assert!(matches!(opened[0].1, TurnSource::QueuedInput { .. }));
    assert_eq!(dequeued_ids(&second).len(), 1);
    assert_eq!(finished_turns(&second)[0].1, TurnOutcome::Completed);
}

/// The cancelled exit, and the rule that hangs off it: a cancelled turn closes
/// as `Cancelled` and the queue behind it is kept, so `CancelActiveTurn` stops
/// the work rather than releasing everything queued behind it.
#[test]
fn a_cancelled_supervised_turn_closes_as_cancelled_and_keeps_the_queue() {
    let cwd = temp_dir("turn_lifecycle_cancel_cwd");
    let home = temp_dir("turn_lifecycle_cancel_home");
    let entered = Arc::new(AtomicBool::new(false));
    let engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(BlockingProvider {
            model: "test-model".to_string(),
            entered: Arc::clone(&entered),
        }),
        Some(home),
    )
    .unwrap();
    let supervisor = RuntimeSupervisor::start(engine);

    supervisor
        .send_command(
            "cmd_queue",
            RuntimeCommand::QueueFollowUp {
                content: "queued one".to_string(),
            },
        )
        .unwrap();
    supervisor
        .send_command(
            "cmd_input",
            RuntimeCommand::SubmitUserInput {
                content: "typed one".to_string(),
            },
        )
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !entered.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(entered.load(Ordering::SeqCst), "the turn must be running");

    supervisor
        .send_command("cmd_cancel", RuntimeCommand::CancelActiveTurn)
        .unwrap();
    let events = collect_until(&supervisor, Duration::from_secs(20), |events| {
        !finished_turns(events).is_empty()
    });

    let closed = finished_turns(&events);
    assert_eq!(closed.len(), 1, "{closed:?}");
    assert_eq!(closed[0].1, TurnOutcome::Cancelled);
    assert!(
        dequeued_ids(&events).is_empty(),
        "a cancelled turn releases nothing queued behind it"
    );
    assert!(
        events.iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::InputQueued { input } if input.content_preview == "queued one"
        )) || events
            .iter()
            .any(|event| matches!(&event.kind, RuntimeEventKind::SnapshotUpdated { .. })),
        "the queued prompt is still a published fact"
    );
}
