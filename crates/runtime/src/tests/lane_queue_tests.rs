//! A Lane's own follow-up queue (C18, compatibility follow-up 16's Core half,
//! E3d/H7 defect 36).
//!
//! `QueueFollowUp` was accepted and announced on the accept path for a
//! session-scoped owner only. A Lane-scoped one did not match that arm, so it
//! fell through into the single supervisor worker's channel — the very worker
//! the Lane's unanswered turn was occupying. No `CommandAccepted`, no
//! `InputQueued`, no event at all: the GUI's `EVENTS` counter stood at `#49`
//! before and after, which is what E3d recorded and H7 named as Core's half.
//!
//! What these tests hold in place:
//!
//! 1. a Lane-scoped follow-up is acknowledged and published *while* the Lane's
//!    turn is still parked on its approval, and the projection lists it under
//!    the Lane's own owner;
//! 2. a completed Lane turn drains that Lane's queue, oldest first, each entry
//!    announced by `InputDequeued` and run as its own Lane-owned turn;
//! 3. a Lane turn that does not complete keeps the queue and runs nothing;
//! 4. the session queue and a Lane's queue are separate: neither drain reaches
//!    the other's entries.

use std::time::Duration;

use viden_types::{
    ApprovalResponse, ModelEvent, RuntimeCommand, RuntimeEvent, RuntimeEventKind, RuntimeOwner,
    ToolCall, ToolInput, TurnOutcome, TurnSource,
};

use super::{SequenceProvider, temp_dir};
use crate::{RuntimeSupervisor, SessionEngine};

fn shell_call(id: &str, command: &str) -> ToolCall {
    let mut input = ToolInput::new();
    input.insert("command".to_string(), command.to_string());
    ToolCall {
        id: id.to_string(),
        name: "shell".to_string(),
        input,
    }
}

fn collect_until(
    supervisor: &RuntimeSupervisor,
    what: &str,
    done: impl Fn(&[RuntimeEvent]) -> bool,
) -> Vec<RuntimeEvent> {
    let started = std::time::Instant::now();
    let mut events = Vec::new();
    while started.elapsed() < Duration::from_secs(30) {
        if let Some(event) = supervisor.recv_event_timeout(Duration::from_millis(50)) {
            events.push(event);
            if done(&events) {
                return events;
            }
        }
    }
    panic!("timed out waiting for {what}; collected: {events:#?}");
}

/// Waits until an approval for `tool_name` is pending and returns it, leaving
/// the worker parked inside the turn that raised it.
fn await_approval(
    supervisor: &RuntimeSupervisor,
    tool_name: &str,
) -> viden_types::ApprovalRequestView {
    let events = collect_until(
        supervisor,
        &format!("an approval for `{tool_name}`"),
        |events| {
            events.iter().any(|event| {
                matches!(&event.kind, RuntimeEventKind::ApprovalRequested { approval }
                    if approval.tool_name == tool_name)
            })
        },
    );
    events
        .iter()
        .rev()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::ApprovalRequested { approval } if approval.tool_name == tool_name => {
                Some(approval.clone())
            }
            _ => None,
        })
        .expect("the approval this wait returned on")
}

fn allow(supervisor: &RuntimeSupervisor, approval: &viden_types::ApprovalRequestView) {
    supervisor
        .send_command_from_owner(
            approval.owner.clone(),
            format!("allow-{}", approval.id),
            RuntimeCommand::RespondToApproval {
                request_id: approval.id.clone(),
                response: ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
}

fn queued_events(events: &[RuntimeEvent]) -> Vec<viden_types::QueuedInputView> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::InputQueued { input } => Some(input.clone()),
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

fn started_turns(events: &[RuntimeEvent]) -> Vec<(TurnSource, RuntimeOwner)> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::TurnStarted { turn } => {
                Some((turn.source.clone(), turn.owner.clone()))
            }
            _ => None,
        })
        .collect()
}

/// A supervisor whose provider parks the first Lane turn on a `shell`
/// approval, plus the Lane owner to command it with.
fn lane_supervisor_parked_on_shell(
    name: &str,
    lane_id: &str,
    turns: Vec<Vec<ModelEvent>>,
) -> (RuntimeSupervisor, RuntimeOwner) {
    let cwd = temp_dir(&format!("{name}_cwd"));
    let home = temp_dir(&format!("{name}_home"));
    let engine =
        SessionEngine::new_with_home(&cwd, Box::new(SequenceProvider::new(turns)), Some(home))
            .unwrap();
    let session_id = engine.session_id().to_string();
    let supervisor = RuntimeSupervisor::start(engine);
    let owner = RuntimeOwner {
        workspace_id: format!("workspace-{lane_id}"),
        project_id: format!("project-{lane_id}"),
        lane_id: Some(lane_id.to_string()),
        session_id: Some(session_id),
        task_id: None,
        turn_id: None,
    };
    (supervisor, owner)
}

/// Defect 36's Core half. The Lane-scoped follow-up is acknowledged and
/// published while the Lane's turn is still parked on its approval — not when
/// the worker that turn occupies finally reaches the command.
#[test]
fn a_lane_follow_up_queued_during_a_lane_turn_is_acknowledged_before_that_turn_ends() {
    let (supervisor, lane_owner) = lane_supervisor_parked_on_shell(
        "lane_queue_accept",
        "lane-queue-accept",
        vec![
            vec![ModelEvent::ToolCall(shell_call("tool_lane_first", "true"))],
            vec![ModelEvent::AssistantText {
                content: "first done".to_string(),
            }],
        ],
    );

    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "lane-first",
            RuntimeCommand::SubmitUserInput {
                content: "do the first thing".to_string(),
            },
        )
        .unwrap();
    let approval = await_approval(&supervisor, "shell");

    // The Lane's turn is parked inside this approval, so everything below is
    // observed strictly before that turn ends.
    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "lane-queue",
            RuntimeCommand::QueueFollowUp {
                content: "summarise what you just changed".to_string(),
            },
        )
        .unwrap();

    let accepted = collect_until(
        &supervisor,
        "the Lane follow-up acknowledgement",
        |events| {
            events
                .iter()
                .any(|event| matches!(&event.kind, RuntimeEventKind::InputQueued { .. }))
        },
    );
    assert!(
        finished_turns(&accepted).is_empty(),
        "the acknowledgement must arrive while the Lane turn is still parked, got {:?}",
        finished_turns(&accepted)
    );
    assert!(
        accepted.iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::CommandAccepted { command_id, .. } if command_id == "lane-queue"
        )),
        "`CommandAccepted` must name the Lane follow-up's command id, got {accepted:#?}"
    );
    let queued = queued_events(&accepted);
    assert_eq!(queued.len(), 1, "{queued:?}");
    assert_eq!(queued[0].content_preview, "summarise what you just changed");
    assert_eq!(
        queued[0].owner.as_ref(),
        Some(&lane_owner),
        "the queued entry names the Lane owner that sent it, so an owner-scoped \
         client matches it without inferring anything"
    );
    assert!(
        dequeued_ids(&accepted).is_empty(),
        "`InputDequeued` belongs to the drain, not to the acknowledgement"
    );

    let view = supervisor.live_view_for_test();
    assert_eq!(
        view.queued_inputs.len(),
        1,
        "the projection must list the Lane's queued input while its turn runs, got {:?}",
        view.queued_inputs
    );
    assert_eq!(
        view.queued_inputs[0].owner.as_ref(),
        Some(&lane_owner),
        "the projection carries the Lane owner"
    );

    allow(&supervisor, &approval);
}

/// A completed Lane turn drains that Lane's queue, oldest first, each entry
/// announced before the Lane-owned turn that runs it.
#[test]
fn a_completed_lane_turn_drains_the_lanes_queue_in_order() {
    let (supervisor, lane_owner) = lane_supervisor_parked_on_shell(
        "lane_queue_drain",
        "lane-queue-drain",
        vec![
            vec![ModelEvent::ToolCall(shell_call("tool_lane_first", "true"))],
            vec![ModelEvent::AssistantText {
                content: "first done".to_string(),
            }],
            vec![ModelEvent::AssistantText {
                content: "second done".to_string(),
            }],
            vec![ModelEvent::AssistantText {
                content: "third done".to_string(),
            }],
        ],
    );

    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "lane-first",
            RuntimeCommand::SubmitUserInput {
                content: "do the first thing".to_string(),
            },
        )
        .unwrap();
    let approval = await_approval(&supervisor, "shell");

    for (command_id, content) in [
        ("lane-queue-one", "summarise the change"),
        ("lane-queue-two", "then run the tests"),
    ] {
        supervisor
            .send_command_from_owner(
                lane_owner.clone(),
                command_id,
                RuntimeCommand::QueueFollowUp {
                    content: content.to_string(),
                },
            )
            .unwrap();
    }
    let accepted = collect_until(&supervisor, "both Lane follow-ups", |events| {
        queued_events(events).len() == 2
    });
    let queued = queued_events(&accepted);
    let queued_ids: Vec<String> = queued.iter().map(|input| input.id.clone()).collect();

    allow(&supervisor, &approval);
    let drained = collect_until(&supervisor, "the Lane queue to drain", |events| {
        finished_turns(events).len() == 3
    });

    assert_eq!(
        dequeued_ids(&drained),
        queued_ids,
        "the Lane's queue drains oldest first"
    );
    let opened = started_turns(&drained);
    assert_eq!(
        opened
            .iter()
            .map(|(source, _)| source.clone())
            .collect::<Vec<_>>(),
        vec![
            TurnSource::QueuedInput {
                input_id: queued_ids[0].clone()
            },
            TurnSource::QueuedInput {
                input_id: queued_ids[1].clone()
            },
        ],
        "each drained entry runs as its own turn naming the entry it came from"
    );
    assert!(
        opened
            .iter()
            .all(|(_, owner)| owner.lane_id.as_deref() == Some("lane-queue-drain")),
        "a drained Lane turn is owned by the Lane, got {opened:#?}"
    );
    assert!(
        finished_turns(&drained)
            .iter()
            .all(|(_, outcome)| *outcome == TurnOutcome::Completed),
        "{:?}",
        finished_turns(&drained)
    );
    assert!(
        supervisor.live_view_for_test().queued_inputs.is_empty(),
        "a fully drained Lane queue is empty in the projection"
    );
}

/// A Lane turn that does not complete keeps its queue and runs nothing: the
/// same rule the session queue follows, applied to the Lane's own owner.
#[test]
fn a_lane_turn_that_does_not_complete_keeps_the_lanes_queue() {
    let (supervisor, lane_owner) = lane_supervisor_parked_on_shell(
        "lane_queue_cancel",
        "lane-queue-cancel",
        vec![
            vec![ModelEvent::ToolCall(shell_call("tool_lane_first", "true"))],
            vec![ModelEvent::AssistantText {
                content: "first done".to_string(),
            }],
        ],
    );

    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "lane-first",
            RuntimeCommand::SubmitUserInput {
                content: "do the first thing".to_string(),
            },
        )
        .unwrap();
    await_approval(&supervisor, "shell");
    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "lane-queue",
            RuntimeCommand::QueueFollowUp {
                content: "never run behind a cancelled turn".to_string(),
            },
        )
        .unwrap();
    let accepted = collect_until(
        &supervisor,
        "the Lane follow-up acknowledgement",
        |events| !queued_events(events).is_empty(),
    );
    let queued_id = queued_events(&accepted)[0].id.clone();

    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "lane-cancel",
            RuntimeCommand::CancelActiveTurn,
        )
        .unwrap();
    let cancelled = collect_until(&supervisor, "the cancelled Lane turn", |events| {
        !finished_turns(events).is_empty()
    });
    assert_eq!(finished_turns(&cancelled)[0].1, TurnOutcome::Cancelled);
    assert!(
        !dequeued_ids(&cancelled).contains(&queued_id),
        "a cancelled Lane turn releases nothing queued behind it, got {:?}",
        dequeued_ids(&cancelled)
    );
    let view = supervisor.live_view_for_test();
    assert_eq!(
        view.queued_inputs.len(),
        1,
        "the Lane keeps its queue across a cancellation, got {:?}",
        view.queued_inputs
    );
    assert_eq!(view.queued_inputs[0].id, queued_id);
}

/// The session queue and a Lane's queue are separate places. A completed
/// session turn does not run a Lane's follow-up, and a completed Lane turn
/// does not run the session's.
#[test]
fn a_lane_queue_and_the_session_queue_do_not_drain_each_other() {
    let (supervisor, lane_owner) = lane_supervisor_parked_on_shell(
        "lane_queue_isolation",
        "lane-queue-isolation",
        vec![
            vec![ModelEvent::ToolCall(shell_call("tool_lane_first", "true"))],
            vec![ModelEvent::AssistantText {
                content: "lane done".to_string(),
            }],
            vec![ModelEvent::AssistantText {
                content: "drained done".to_string(),
            }],
        ],
    );

    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "lane-first",
            RuntimeCommand::SubmitUserInput {
                content: "do the first thing".to_string(),
            },
        )
        .unwrap();
    let approval = await_approval(&supervisor, "shell");

    supervisor
        .send_command(
            "session-queue",
            RuntimeCommand::QueueFollowUp {
                content: "belongs to the session".to_string(),
            },
        )
        .unwrap();
    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "lane-queue",
            RuntimeCommand::QueueFollowUp {
                content: "belongs to the lane".to_string(),
            },
        )
        .unwrap();
    let accepted = collect_until(&supervisor, "both follow-ups", |events| {
        queued_events(events).len() == 2
    });
    let queued = queued_events(&accepted);
    let session_entry = queued
        .iter()
        .find(|input| input.content_preview == "belongs to the session")
        .expect("the session entry");
    let lane_entry = queued
        .iter()
        .find(|input| input.content_preview == "belongs to the lane")
        .expect("the lane entry");
    assert_eq!(
        session_entry.owner, None,
        "a session-scoped entry belongs to no Lane and publishes no owner"
    );
    assert_eq!(lane_entry.owner.as_ref(), Some(&lane_owner));
    let lane_entry_id = lane_entry.id.clone();
    let session_entry_id = session_entry.id.clone();

    allow(&supervisor, &approval);
    let drained = collect_until(&supervisor, "the Lane drain", |events| {
        dequeued_ids(events).contains(&lane_entry_id) && finished_turns(events).len() == 2
    });
    assert!(
        !dequeued_ids(&drained).contains(&session_entry_id),
        "a completed Lane turn must not spend the session's queue, got {:?}",
        dequeued_ids(&drained)
    );
    let view = supervisor.live_view_for_test();
    assert_eq!(
        view.queued_inputs.len(),
        1,
        "the session entry is still queued, got {:?}",
        view.queued_inputs
    );
    assert_eq!(view.queued_inputs[0].id, session_entry_id);
}
