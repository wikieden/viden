use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use viden_provider::{ModelProvider, ModelRequestControl};
use viden_types::{
    AgentLaneRecord, AgentRole, AgentRoute, DataEgressPolicy, EventCursor, ExecutionTarget,
    FRONTEND_SCHEMA_V1, GateStrength, LaneBudget, LaneRuntimeOwnerBinding, LaneStatus, ModelEvent,
    ModelRequest, MutationPolicy, PermissionLevel, PermissionMode, ReplayRequest, RuntimeCommand,
    RuntimeEvent, RuntimeEventEnvelope, RuntimeEventKind, RuntimeOwner, RuntimeSnapshot,
    RuntimeViewState, RuntimeWireEvent, WorkMode,
};
use viden_workflows::{lanes::LaneEvent, stores::WorkflowStore};

use crate::{
    RuntimeSupervisor, SessionEngine, runtime_supervisor::set_before_supervisor_command_hook,
};
use viden_lanes::{
    LaneEffectExecutor, LaneEffectRequest, LaneEffectResult, LanePersistence,
    LocalLaneEffectExecutor, WorkflowLanePersistence, resolve_lane_output_log,
};

use super::{SequenceProvider, temp_dir};

static SUPERVISOR_COMMAND_HOOK_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[test]
fn lane_supervisor_protocol_round_trips_all_lifecycle_commands() {
    let commands = vec![
        RuntimeCommand::CreateLane {
            lane: lane("lane-a"),
        },
        RuntimeCommand::StartLane {
            lane_id: "lane-a".to_string(),
            command: "worker".to_string(),
            args: vec!["--run".to_string()],
            env: vec![("VIDEN_LANE".to_string(), "lane-a".to_string())],
            output_log: Some(".viden/lanes/lane-a.log".to_string()),
        },
        RuntimeCommand::StopLane {
            lane_id: "lane-a".to_string(),
        },
        RuntimeCommand::AttachLane {
            lane_id: "lane-a".to_string(),
        },
        RuntimeCommand::DetachLane {
            lane_id: "lane-a".to_string(),
        },
        RuntimeCommand::SendLaneInput {
            lane_id: "lane-a".to_string(),
            input: "continue\n".to_string(),
        },
        RuntimeCommand::AcceptLaneOutput {
            lane_id: "lane-a".to_string(),
            summary: "accepted".to_string(),
        },
        RuntimeCommand::ReviseLaneOutput {
            lane_id: "lane-a".to_string(),
            feedback: "add tests".to_string(),
        },
        RuntimeCommand::DiscardLaneOutput {
            lane_id: "lane-a".to_string(),
            reason: "superseded".to_string(),
        },
        RuntimeCommand::ApplyLaneChanges {
            lane_id: "lane-a".to_string(),
            unified_diff: "diff --git a/a b/a\n".to_string(),
        },
        RuntimeCommand::ResolveLaneConflict {
            lane_id: "lane-a".to_string(),
            unified_diff: "diff --git a/a b/a\n".to_string(),
        },
        RuntimeCommand::ArchiveLane {
            lane_id: "lane-a".to_string(),
            summary: "archived evidence".to_string(),
        },
        RuntimeCommand::CleanupLane {
            lane_id: "lane-a".to_string(),
            force: false,
        },
    ];
    let expected_types = [
        "create_lane",
        "start_lane",
        "stop_lane",
        "attach_lane",
        "detach_lane",
        "send_lane_input",
        "accept_lane_output",
        "revise_lane_output",
        "discard_lane_output",
        "apply_lane_changes",
        "resolve_lane_conflict",
        "archive_lane",
        "cleanup_lane",
    ];

    for (command, expected_type) in commands.into_iter().zip(expected_types) {
        let encoded = serde_json::to_value(&command).unwrap();
        assert_eq!(encoded["type"], expected_type);
        let decoded: RuntimeCommand = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, command);
    }
}

#[test]
fn lane_supervisor_protocol_round_trips_owner_scoped_events_and_projects_view() {
    let owner = owner("lane-a");
    let event_kinds = vec![
        RuntimeEventKind::LaneUpdated {
            lane: lane("lane-a"),
        },
        RuntimeEventKind::LaneOutputAppended {
            lane_id: "lane-a".to_string(),
            stream: "stdout".to_string(),
            content: "tests passed".to_string(),
        },
        RuntimeEventKind::LaneConflictDetected {
            lane_id: "lane-a".to_string(),
            summary: "patch conflict".to_string(),
            paths: vec!["src/lib.rs".to_string()],
            content: None,
        },
        RuntimeEventKind::LaneRecoveryRequired {
            lane_id: "lane-a".to_string(),
            reason: "terminal exited".to_string(),
            next_action: "restart lane".to_string(),
        },
    ];
    let mut view = RuntimeViewState::new(snapshot());

    for (offset, kind) in event_kinds.into_iter().enumerate() {
        let sequence = offset as u64 + 1;
        let envelope = RuntimeEventEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            owner: owner.clone(),
            cursor: EventCursor {
                stream_id: "lane-stream".to_string(),
                sequence,
            },
            event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
                sequence,
                Some(100 + sequence),
                kind,
            )),
        };
        let encoded = serde_json::to_vec(&envelope).unwrap();
        let decoded: RuntimeEventEnvelope = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded.owner, owner);
        let RuntimeWireEvent::Known(event) = decoded.event else {
            panic!("lane lifecycle event decoded as unknown");
        };
        view.apply_event(&event);
    }

    assert_eq!(view.lanes, vec![lane("lane-a")]);
    assert_eq!(view.lane_outputs.len(), 1);
    assert_eq!(view.lane_outputs[0].lane_id, "lane-a");
    assert_eq!(view.lane_outputs[0].content, "tests passed");
    assert_eq!(view.lane_conflicts.len(), 1);
    assert_eq!(view.lane_conflicts[0].paths, vec!["src/lib.rs"]);
    assert_eq!(view.lane_recoveries.len(), 1);
    assert_eq!(view.lane_recoveries[0].next_action, "restart lane");
}

#[test]
fn lane_runtime_owner_create_publishes_exact_binding_to_snapshot_and_replay() {
    let cwd = temp_dir("lane_runtime_owner_create_cwd");
    let home = temp_dir("lane_runtime_owner_create_home");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        Arc::new(RecordingLaneEffects::default()) as Arc<dyn LaneEffectExecutor>,
    );
    let exact_owner = owner("lane-owner-create");

    supervisor
        .send_command_from_owner(
            exact_owner.clone(),
            "create_lane_owner",
            RuntimeCommand::CreateLane {
                lane: autonomous_lane("lane-owner-create"),
            },
        )
        .unwrap();
    let events = collect_envelopes_until(&supervisor, |events| {
        let binding = events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneRuntimeOwnerBound { .. },
                    ..
                })
            )
        });
        let lane = events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneUpdated { lane },
                    ..
                }) if lane.id == "lane-owner-create"
            )
        });
        binding && lane
    });
    let binding_envelope = events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneRuntimeOwnerBound { .. },
                    ..
                })
            )
        })
        .unwrap();
    let expected = LaneRuntimeOwnerBinding {
        lane_id: "lane-owner-create".to_string(),
        owner: exact_owner.clone(),
    };
    assert_eq!(binding_envelope.owner, exact_owner);
    assert!(matches!(
        &binding_envelope.event,
        RuntimeWireEvent::Known(RuntimeEvent {
            kind: RuntimeEventKind::LaneRuntimeOwnerBound { binding },
            ..
        }) if binding == &expected
    ));
    let lane_sequence = events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneUpdated { lane },
                    ..
                }) if lane.id == "lane-owner-create"
            )
        })
        .unwrap()
        .cursor
        .sequence;
    assert!(binding_envelope.cursor.sequence < lane_sequence);

    let snapshot = supervisor.snapshot_envelope().unwrap();
    assert_eq!(snapshot.view.lane_runtime_owners, vec![expected.clone()]);
    let replay = supervisor
        .replay_events(ReplayRequest {
            after: EventCursor {
                stream_id: snapshot.cursor.stream_id,
                sequence: 0,
            },
            limit: 100,
        })
        .unwrap();
    assert!(replay.events.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::LaneRuntimeOwnerBound { binding },
                ..
            }) if binding == &expected
        )
    }));
}

#[test]
fn lane_runtime_owner_hydrated_start_publishes_fresh_exact_binding() {
    let cwd = temp_dir("lane_runtime_owner_start_cwd");
    let home = temp_dir("lane_runtime_owner_start_home");
    persist_lane(&home, &cwd, autonomous_lane("lane-owner-start"));
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        Arc::new(RecordingLaneEffects::default()) as Arc<dyn LaneEffectExecutor>,
    );
    assert!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .lane_runtime_owners
            .is_empty()
    );
    let exact_owner = owner("lane-owner-start");

    supervisor
        .send_command_from_owner(
            exact_owner.clone(),
            "start_lane_owner",
            RuntimeCommand::StartLane {
                lane_id: "lane-owner-start".to_string(),
                command: "worker".to_string(),
                args: Vec::new(),
                env: Vec::new(),
                output_log: None,
            },
        )
        .unwrap();
    let events = collect_envelopes_until(&supervisor, |events| {
        let bound = events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneRuntimeOwnerBound { binding },
                    ..
                }) if binding.owner == exact_owner
            )
        });
        let running = events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneUpdated { lane },
                    ..
                }) if lane.id == "lane-owner-start" && lane.status == LaneStatus::Running
            )
        });
        bound && running
    });
    let binding_sequence = events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneRuntimeOwnerBound { .. },
                    ..
                })
            )
        })
        .unwrap()
        .cursor
        .sequence;
    let running_sequence = events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneUpdated { lane },
                    ..
                }) if lane.status == LaneStatus::Running
            )
        })
        .unwrap()
        .cursor
        .sequence;
    assert!(binding_sequence < running_sequence);
    assert_eq!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .lane_runtime_owners,
        vec![LaneRuntimeOwnerBinding {
            lane_id: "lane-owner-start".to_string(),
            owner: exact_owner,
        }]
    );
}

#[test]
fn lane_runtime_owner_rejections_publish_no_binding_or_lane_mutation() {
    let cwd = temp_dir("lane_runtime_owner_reject_cwd");
    let home = temp_dir("lane_runtime_owner_reject_home");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );

    supervisor
        .send_command_from_owner(
            owner("lane-wrong-owner"),
            "create_wrong_owner",
            RuntimeCommand::CreateLane {
                lane: autonomous_lane("lane-target"),
            },
        )
        .unwrap();
    let events = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandRejected { command_id, .. },
                    ..
                }) if command_id == "create_wrong_owner"
            )
        })
    });
    assert!(!events.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::LaneRuntimeOwnerBound { .. },
                ..
            })
        )
    }));
    assert!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .lane_runtime_owners
            .is_empty()
    );
    assert!(effects.calls.lock().unwrap().is_empty());
}

#[test]
fn lane_runtime_owner_cancel_requires_exact_binding_and_terminal_clears_only_its_lane() {
    let cwd = temp_dir("lane_runtime_owner_cancel_cwd");
    let home = temp_dir("lane_runtime_owner_cancel_home");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let owner_a = owner("lane-owner-a");
    let owner_b = owner("lane-owner-b");
    for (command_id, exact_owner) in [
        ("create_owner_a", owner_a.clone()),
        ("create_owner_b", owner_b.clone()),
    ] {
        let lane_id = exact_owner.lane_id.clone().unwrap();
        supervisor
            .send_command_from_owner(
                exact_owner.clone(),
                command_id,
                RuntimeCommand::CreateLane {
                    lane: autonomous_lane(&lane_id),
                },
            )
            .unwrap();
        collect_envelopes_until(&supervisor, |events| {
            let bound = events.iter().any(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::LaneRuntimeOwnerBound { binding },
                        ..
                    }) if binding.lane_id == lane_id
                )
            });
            let durable = events.iter().any(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::LaneUpdated { lane },
                        ..
                    }) if lane.id == lane_id
                )
            });
            bound && durable
        });
    }
    assert_eq!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .lane_runtime_owners
            .len(),
        2
    );

    let default_cancel =
        supervisor.send_command("cancel_default_owner", RuntimeCommand::CancelActiveTurn);
    assert!(default_cancel.is_ok());
    let mut partial_owner = RuntimeOwner {
        lane_id: Some("lane-owner-a".to_string()),
        ..RuntimeOwner::default()
    };
    assert!(
        supervisor
            .send_command_from_owner(
                partial_owner.clone(),
                "cancel_partial_owner",
                RuntimeCommand::CancelActiveTurn,
            )
            .is_err()
    );
    partial_owner.workspace_id = owner_a.workspace_id.clone();
    partial_owner.project_id = owner_a.project_id.clone();
    partial_owner.session_id = owner_a.session_id.clone();
    partial_owner.turn_id = Some("turn-stale".to_string());
    assert!(
        supervisor
            .send_command_from_owner(
                partial_owner,
                "cancel_stale_owner",
                RuntimeCommand::CancelActiveTurn,
            )
            .is_err()
    );
    supervisor
        .send_command_from_owner(
            owner("lane-not-running"),
            "cancel_other_lane",
            RuntimeCommand::CancelActiveTurn,
        )
        .unwrap();
    assert!(
        effects
            .calls
            .lock()
            .unwrap()
            .iter()
            .all(|call| !call.starts_with("stop:"))
    );
    assert_eq!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .lane_runtime_owners
            .len(),
        2
    );

    supervisor
        .send_command_from_owner(
            owner_b.clone(),
            "complete_owner_b",
            RuntimeCommand::AcceptLaneOutput {
                lane_id: "lane-owner-b".to_string(),
                summary: "lane B done".to_string(),
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneUpdated { lane },
                    ..
                }) if lane.id == "lane-owner-b" && lane.status == LaneStatus::Done
            )
        })
    });
    assert_eq!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .lane_runtime_owners,
        vec![LaneRuntimeOwnerBinding {
            lane_id: "lane-owner-a".to_string(),
            owner: owner_a.clone(),
        }]
    );

    supervisor
        .send_command_from_owner(
            owner_a,
            "cancel_exact_owner",
            RuntimeCommand::CancelActiveTurn,
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneUpdated { lane },
                    ..
                }) if lane.id == "lane-owner-a" && lane.status == LaneStatus::Cancelled
            )
        })
    });
    assert!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .lane_runtime_owners
            .is_empty()
    );
    assert!(
        effects
            .calls
            .lock()
            .unwrap()
            .contains(&"stop:lane-owner-a".to_string())
    );
}

#[test]
fn cancel_active_native_turn_keeps_its_lane_routable() {
    let cwd = temp_dir("cancel_native_turn_keeps_lane_cwd");
    let home = temp_dir("cancel_native_turn_keeps_lane_home");
    let entered = Arc::new(AtomicBool::new(false));
    let provider: Box<dyn ModelProvider> = Box::new(BlockingLaneProvider {
        entered: Arc::clone(&entered),
    });
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let exact_owner = owner("lane-native-turn");

    supervisor
        .send_command_from_owner(
            exact_owner.clone(),
            "create_native_turn_lane",
            RuntimeCommand::CreateLane {
                lane: autonomous_lane("lane-native-turn"),
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneRuntimeOwnerBound { binding },
                    ..
                }) if binding.owner == exact_owner
            )
        })
    });

    supervisor
        .send_command_from_owner(
            exact_owner.clone(),
            "submit_native_turn",
            RuntimeCommand::SubmitUserInput {
                content: "run until cancelled".to_string(),
            },
        )
        .unwrap();
    wait_until(Duration::from_secs(5), "native turn provider entry", || {
        entered.load(Ordering::SeqCst)
    });

    supervisor
        .send_command_from_owner(
            exact_owner.clone(),
            "cancel_native_turn",
            RuntimeCommand::CancelActiveTurn,
        )
        .unwrap();
    let first_cancel = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::Error { error },
                    ..
                }) if error.message.contains("Model request cancelled")
            ) || matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneUpdated { lane },
                    ..
                }) if lane.id == "lane-native-turn" && lane.status == LaneStatus::Cancelled
            )
        })
    });
    let lane_was_cancelled = first_cancel.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::LaneUpdated { lane },
                ..
            }) if lane.id == "lane-native-turn" && lane.status == LaneStatus::Cancelled
        )
    });
    assert!(
        !lane_was_cancelled,
        "cancelling a model turn must not cancel its Lane"
    );
    let snapshot = supervisor.snapshot_envelope().unwrap();
    assert!(
        snapshot
            .view
            .lane_runtime_owners
            .iter()
            .any(|binding| binding.owner == exact_owner)
    );
    assert!(
        effects
            .calls
            .lock()
            .unwrap()
            .iter()
            .all(|call| call != "stop:lane-native-turn")
    );
}

#[test]
fn lane_runtime_owner_restart_has_no_stale_binding_and_rebinds_on_live_worker_spawn() {
    let cwd = temp_dir("lane_runtime_owner_restart_cwd");
    let home = temp_dir("lane_runtime_owner_restart_home");
    let exact_owner = owner("lane-owner-restart");
    let first_stream = {
        let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
        let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
        engine
            .set_permission_mode(viden_types::PermissionMode::DontAsk)
            .unwrap();
        let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
            engine,
            Arc::new(RecordingLaneEffects::default()) as Arc<dyn LaneEffectExecutor>,
        );
        supervisor
            .send_command_from_owner(
                exact_owner.clone(),
                "create_owner_restart",
                RuntimeCommand::CreateLane {
                    lane: autonomous_lane("lane-owner-restart"),
                },
            )
            .unwrap();
        collect_envelopes_until(&supervisor, |events| {
            let bound = events.iter().any(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::LaneRuntimeOwnerBound { .. },
                        ..
                    })
                )
            });
            let durable = events.iter().any(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::LaneUpdated { lane },
                        ..
                    }) if lane.id == "lane-owner-restart"
                )
            });
            bound && durable
        });
        supervisor.snapshot_envelope().unwrap().cursor.stream_id
    };

    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        Arc::new(RecordingLaneEffects::default()) as Arc<dyn LaneEffectExecutor>,
    );
    let restarted = supervisor.snapshot_envelope().unwrap();
    assert_ne!(restarted.cursor.stream_id, first_stream);
    assert!(restarted.view.lane_runtime_owners.is_empty());

    supervisor
        .send_command_from_owner(
            exact_owner.clone(),
            "complete_restarted_owner",
            RuntimeCommand::AcceptLaneOutput {
                lane_id: "lane-owner-restart".to_string(),
                summary: "recovered".to_string(),
            },
        )
        .unwrap();
    let events = collect_envelopes_until(&supervisor, |events| {
        let rebound = events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneRuntimeOwnerBound { binding },
                    ..
                }) if binding.owner == exact_owner
            )
        });
        let done = events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneUpdated { lane },
                    ..
                }) if lane.id == "lane-owner-restart" && lane.status == LaneStatus::Done
            )
        });
        rebound && done
    });
    let rebound_sequence = events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneRuntimeOwnerBound { .. },
                    ..
                })
            )
        })
        .unwrap()
        .cursor
        .sequence;
    let done_sequence = events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneUpdated { lane },
                    ..
                }) if lane.id == "lane-owner-restart" && lane.status == LaneStatus::Done
            )
        })
        .unwrap()
        .cursor
        .sequence;
    assert!(rebound_sequence < done_sequence);
    assert!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .lane_runtime_owners
            .is_empty()
    );
}

#[test]
fn lane_supervisor_plan_mode_rejects_effectful_commands_before_effects() {
    let cwd = temp_dir("lane_supervisor_plan_gate_cwd");
    let home = temp_dir("lane_supervisor_plan_gate_home");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let mut approver = |_prompt| viden_types::ApprovalResponse::allow_once(None);
    engine
        .handle_runtime_command(
            "cmd_plan",
            RuntimeCommand::SetWorkMode {
                mode: WorkMode::Plan,
            },
            &mut approver,
        )
        .unwrap();
    let effects = Arc::new(CountingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let lane_owner = owner("lane-plan");

    let commands = [
        (
            "cmd_create_plan",
            RuntimeCommand::CreateLane {
                lane: lane("lane-plan"),
            },
        ),
        (
            "cmd_start_plan",
            RuntimeCommand::StartLane {
                lane_id: "lane-plan".to_string(),
                command: "worker".to_string(),
                args: Vec::new(),
                env: Vec::new(),
                output_log: None,
            },
        ),
        (
            "cmd_apply_plan",
            RuntimeCommand::ApplyLaneChanges {
                lane_id: "lane-plan".to_string(),
                unified_diff: "diff --git a/a b/a\n".to_string(),
            },
        ),
        (
            "cmd_cleanup_plan",
            RuntimeCommand::CleanupLane {
                lane_id: "lane-plan".to_string(),
                force: true,
            },
        ),
        (
            "cmd_archive_plan",
            RuntimeCommand::ArchiveLane {
                lane_id: "lane-plan".to_string(),
                summary: "archive".to_string(),
            },
        ),
        (
            "cmd_accept_plan",
            RuntimeCommand::AcceptLaneOutput {
                lane_id: "lane-plan".to_string(),
                summary: "accept".to_string(),
            },
        ),
        (
            "cmd_attach_plan",
            RuntimeCommand::AttachLane {
                lane_id: "lane-plan".to_string(),
            },
        ),
    ];
    for (command_id, command) in commands {
        supervisor
            .send_command_from_owner(lane_owner.clone(), command_id, command)
            .unwrap();
    }

    let envelopes = collect_envelopes_until(&supervisor, |events| {
        events
            .iter()
            .filter(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::CommandRejected { reason, .. },
                        ..
                    }) if reason.contains("Plan mode")
                )
            })
            .count()
            == 7
    });
    assert_eq!(effects.calls.load(Ordering::SeqCst), 0);
    assert!(envelopes.iter().all(|envelope| {
        envelope.owner == lane_owner
            || matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::WorkspaceSourceUpdated { .. }
                        | RuntimeEventKind::RuntimeServiceHealthUpdated { .. },
                    ..
                })
            )
    }));
    assert!(!envelopes.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::LaneRuntimeOwnerBound { .. },
                ..
            })
        )
    }));
    assert!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .lane_runtime_owners
            .is_empty()
    );
}

#[test]
fn lane_supervisor_approval_expires_without_a_response() {
    let cwd = temp_dir("lane_active_expiry_cwd");
    let home = temp_dir("lane_active_expiry_home");
    persist_done_propose_lane(&home, &cwd, "lane-active-expiry");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_and_approval_ttl_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
        0,
    );
    let owner = owner("lane-active-expiry");
    supervisor
        .send_command_from_owner(
            owner,
            "apply_active_expiry",
            RuntimeCommand::ApplyLaneChanges {
                lane_id: "lane-active-expiry".to_string(),
                unified_diff: "diff --git a/a b/a\n".to_string(),
            },
        )
        .unwrap();

    let events = collect_envelopes_until(&supervisor, |events| {
        let resolved = events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalResolved {
                        decision: viden_types::ApprovalDecision::Deny,
                        ..
                    },
                    ..
                })
            )
        });
        let restored = events.iter().any(|envelope| {
            matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::LaneUpdated { lane }, .. }) if lane.id == "lane-active-expiry" && lane.status == LaneStatus::Done)
        });
        resolved && restored
    });
    assert!(events.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::LaneUpdated { lane },
                ..
            }) if lane.id == "lane-active-expiry" && lane.status == LaneStatus::Done
        )
    }));
    assert!(!effects.calls.lock().unwrap().contains(&"apply".to_string()));
}

#[test]
fn lane_supervisor_output_log_is_confined_to_canonical_lane_root() {
    let root = temp_dir("lane_output_log_scope");
    let lane_root = root.join("lane");
    std::fs::create_dir_all(lane_root.join("logs")).unwrap();
    let resolved = resolve_lane_output_log(&lane_root, Some("logs/worker.log"), "lane-a")
        .expect("scoped relative log");
    assert_eq!(
        resolved,
        lane_root.canonicalize().unwrap().join("logs/worker.log")
    );
    assert!(resolve_lane_output_log(&lane_root, Some("../escape.log"), "lane-a").is_err());
    assert!(resolve_lane_output_log(&lane_root, Some("/tmp/escape.log"), "lane-a").is_err());

    #[cfg(unix)]
    {
        let outside = root.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, lane_root.join("logs/link")).unwrap();
        assert!(
            resolve_lane_output_log(&lane_root, Some("logs/link/escape.log"), "lane-a").is_err()
        );
    }
}

#[test]
fn lane_supervisor_apply_enforces_state_and_mutation_policy_before_effect() {
    let cwd = temp_dir("lane_apply_policy_cwd");
    let home = temp_dir("lane_apply_policy_home");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let owner = owner("lane-policy");

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "create_policy",
            RuntimeCommand::CreateLane {
                lane: lane("lane-policy"),
            },
        )
        .unwrap();
    approve_lane_tool(&supervisor, &owner, "lane_create", "approve_create_policy");
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "apply_wrong_state",
            RuntimeCommand::ApplyLaneChanges {
                lane_id: "lane-policy".to_string(),
                unified_diff: "diff --git a/a b/a\n".to_string(),
            },
        )
        .unwrap();
    let wrong_state = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandRejected { command_id, .. },
                    ..
                }) if command_id == "apply_wrong_state"
            )
        })
    });
    assert!(wrong_state.iter().any(|envelope| envelope.owner == owner));
    assert!(!effects.calls.lock().unwrap().contains(&"apply".to_string()));

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "accept_policy",
            RuntimeCommand::AcceptLaneOutput {
                lane_id: "lane-policy".to_string(),
                summary: "ready to apply".to_string(),
            },
        )
        .unwrap();
    approve_lane_tool(
        &supervisor,
        &owner,
        "lane_accept_output",
        "approve_accept_policy",
    );
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "apply_propose_only",
            RuntimeCommand::ApplyLaneChanges {
                lane_id: "lane-policy".to_string(),
                unified_diff: "diff --git a/a b/a\n".to_string(),
            },
        )
        .unwrap();
    let approval = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { approval },
                    ..
                }) if approval.tool_name == "lane_apply"
            )
        })
    });
    assert!(approval.iter().any(|envelope| envelope.owner == owner));
    let approval_target = approval
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) if approval.tool_name == "lane_apply" => Some(&approval.target),
            _ => None,
        })
        .unwrap();
    assert_eq!(approval_target.kind, "repository");
    let canonical_cwd = cwd.canonicalize().unwrap();
    assert_eq!(
        approval_target.canonical_ref.as_deref(),
        Some(canonical_cwd.to_string_lossy().as_ref())
    );
    assert!(!effects.calls.lock().unwrap().contains(&"apply".to_string()));
}

#[test]
fn lane_supervisor_status_mutation_asks_before_requested_status_append() {
    let cwd = temp_dir("lane_status_ask_cwd");
    let home = temp_dir("lane_status_ask_home");
    persist_lane(&home, &cwd, autonomous_lane("lane-status-ask"));
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        Arc::new(RecordingLaneEffects::default()) as Arc<dyn LaneEffectExecutor>,
    );
    supervisor
        .send_command_from_owner(
            owner("lane-status-ask"),
            "attach_requires_approval",
            RuntimeCommand::AttachLane {
                lane_id: "lane-status-ask".to_string(),
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalRequested { approval }, .. }) if approval.tool_name == "lane_attach"))
    });
    assert_eq!(
        WorkflowStore::new(home, &cwd)
            .unwrap()
            .load_lane_state()
            .unwrap()
            .lane("lane-status-ask")
            .unwrap()
            .status,
        LaneStatus::WaitingApproval
    );
}

#[test]
fn lane_supervisor_read_only_status_mutation_rejects_before_durable_append() {
    let cwd = temp_dir("lane_status_read_only_cwd");
    let home = temp_dir("lane_status_read_only_home");
    let mut record = lane("lane-status-read-only");
    record.mutation_policy = MutationPolicy::ReadOnly;
    persist_lane(&home, &cwd, record);
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::BypassPermissions)
        .unwrap();
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        Arc::new(RecordingLaneEffects::default()) as Arc<dyn LaneEffectExecutor>,
    );
    let lane_owner = owner("lane-status-read-only");
    let commands = [
        (
            "attach_read_only",
            RuntimeCommand::AttachLane {
                lane_id: "lane-status-read-only".to_string(),
            },
        ),
        (
            "detach_read_only",
            RuntimeCommand::DetachLane {
                lane_id: "lane-status-read-only".to_string(),
            },
        ),
        (
            "accept_read_only",
            RuntimeCommand::AcceptLaneOutput {
                lane_id: "lane-status-read-only".to_string(),
                summary: "done".to_string(),
            },
        ),
        (
            "revise_read_only",
            RuntimeCommand::ReviseLaneOutput {
                lane_id: "lane-status-read-only".to_string(),
                feedback: "revise".to_string(),
            },
        ),
        (
            "discard_read_only",
            RuntimeCommand::DiscardLaneOutput {
                lane_id: "lane-status-read-only".to_string(),
                reason: "discard".to_string(),
            },
        ),
    ];
    for (command_id, command) in commands {
        supervisor
            .send_command_from_owner(lane_owner.clone(), command_id, command)
            .unwrap();
    }
    collect_envelopes_until(&supervisor, |events| {
        events.iter().filter(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::CommandRejected { reason, .. }, .. }) if reason.contains("read-only"))).count() == 5
    });
    assert_eq!(
        WorkflowStore::new(home, &cwd)
            .unwrap()
            .load_lane_state()
            .unwrap()
            .lane("lane-status-read-only")
            .unwrap()
            .status,
        LaneStatus::Draft
    );
}

#[test]
fn lane_supervisor_syncs_dynamic_permissions_and_redacts_real_start_target() {
    let cwd = temp_dir("lane_dynamic_permission_cwd");
    let home = temp_dir("lane_dynamic_permission_home");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::BypassPermissions)
        .unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let owner = owner("lane-dynamic-permission");
    let mut autonomous = lane("lane-dynamic-permission");
    autonomous.mutation_policy = MutationPolicy::Autonomous;
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "create_dynamic_permission",
            RuntimeCommand::CreateLane { lane: autonomous },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::LaneUpdated { lane }, .. }) if lane.id == "lane-dynamic-permission"))
    });
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "permission_back_to_ask",
            RuntimeCommand::SetPermissionLevel {
                level: PermissionLevel::Ask,
            },
        )
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "start_after_permission_change",
            RuntimeCommand::StartLane {
                lane_id: "lane-dynamic-permission".to_string(),
                command: "runner --token super-secret".to_string(),
                args: vec!["--password".to_string(), "super-secret".to_string()],
                env: vec![("API_TOKEN".to_string(), "super-secret".to_string())],
                output_log: Some("logs/worker.log".to_string()),
            },
        )
        .unwrap();
    let events = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalRequested { approval }, .. }) if approval.tool_name == "lane_start"))
    });
    let approval = events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) if approval.tool_name == "lane_start" => Some(approval),
            _ => None,
        })
        .unwrap();
    let lane_path = cwd
        .canonicalize()
        .unwrap()
        .join(".worktrees/lane-dynamic-permission");
    assert!(
        approval
            .input_preview
            .contains(&lane_path.to_string_lossy().to_string())
    );
    assert!(approval.input_preview.contains("API_TOKEN=[REDACTED]"));
    assert!(approval.input_preview.contains("logs/worker.log"));
    assert_eq!(approval.target.kind, "worktree");
    assert_eq!(approval.target.display, lane_path.to_string_lossy());
    assert_eq!(
        approval.target.canonical_ref.as_deref(),
        Some(lane_path.to_string_lossy().as_ref())
    );
    assert!(approval.allowed_scopes.iter().any(|scope| matches!(scope, viden_types::ApprovalScope::RepoAllowlist { paths } if paths == &vec![lane_path.to_string_lossy().to_string()])));
    assert!(
        !serde_json::to_string(&events)
            .unwrap()
            .contains("super-secret")
    );
    assert!(
        !effects
            .calls
            .lock()
            .unwrap()
            .contains(&"start:lane-dynamic-permission".to_string())
    );
}

#[test]
fn lane_supervisor_create_and_cleanup_use_permission_targets_before_effects() {
    let cwd = temp_dir("lane_create_cleanup_permission_cwd");
    let home = temp_dir("lane_create_cleanup_permission_home");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let create_owner = owner("lane-create-permission");
    let mut autonomous = lane("lane-create-permission");
    autonomous.mutation_policy = MutationPolicy::Autonomous;
    supervisor
        .send_command_from_owner(
            create_owner,
            "create_requires_permission",
            RuntimeCommand::CreateLane { lane: autonomous },
        )
        .unwrap();
    let events = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalRequested { approval }, .. }) if approval.tool_name == "lane_create"))
    });
    let serialized = serde_json::to_string(&events).unwrap();
    assert!(serialized.contains(".worktrees/lane-create-permission"));
    assert!(
        !effects
            .calls
            .lock()
            .unwrap()
            .contains(&"create:lane-create-permission".to_string())
    );
    drop(supervisor);

    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        provider,
        Some(temp_dir("lane_cleanup_permission_home")),
    )
    .unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::BypassPermissions)
        .unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let owner = owner("lane-cleanup-permission");
    let mut autonomous = lane("lane-cleanup-permission");
    autonomous.mutation_policy = MutationPolicy::Autonomous;
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "create_cleanup_permission",
            RuntimeCommand::CreateLane { lane: autonomous },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::LaneUpdated { lane }, .. }) if lane.id == "lane-cleanup-permission"))
    });
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cleanup_permission_to_ask",
            RuntimeCommand::SetPermissionLevel {
                level: PermissionLevel::Ask,
            },
        )
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner,
            "cleanup_requires_permission",
            RuntimeCommand::CleanupLane {
                lane_id: "lane-cleanup-permission".to_string(),
                force: true,
            },
        )
        .unwrap();
    let events = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalRequested { approval }, .. }) if approval.tool_name == "lane_cleanup"))
    });
    let serialized = serde_json::to_string(&events).unwrap();
    assert!(serialized.contains(".worktrees/lane-cleanup-permission"));
    assert!(serialized.contains("force: true"));
    assert!(
        !effects
            .calls
            .lock()
            .unwrap()
            .contains(&"cleanup:lane-cleanup-permission".to_string())
    );
}

#[test]
fn lane_supervisor_propose_only_create_requires_approval_even_when_permissions_allow() {
    let cwd = temp_dir("lane_propose_create_cwd");
    let home = temp_dir("lane_propose_create_home");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::BypassPermissions)
        .unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let lane_owner = owner("lane-propose-create");

    supervisor
        .send_command_from_owner(
            lane_owner,
            "create_propose_only",
            RuntimeCommand::CreateLane {
                lane: lane("lane-propose-create"),
            },
        )
        .unwrap();

    let events = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { approval },
                    ..
                }) if approval.tool_name == "lane_create"
            )
        })
    });
    assert!(events.iter().all(|envelope| {
        !matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::LaneUpdated { lane },
                ..
            }) if lane.id == "lane-propose-create"
        )
    }));
    assert!(effects.calls.lock().unwrap().is_empty());
}

#[test]
fn lane_supervisor_expired_approval_reuses_audit_and_never_applies() {
    let cwd = temp_dir("lane_approval_expiry_cwd");
    let home = temp_dir("lane_approval_expiry_home");
    persist_done_propose_lane(&home, &cwd, "lane-expired");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_and_approval_ttl_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
        0,
    );
    let owner = owner("lane-expired");
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "apply_expired",
            RuntimeCommand::ApplyLaneChanges {
                lane_id: "lane-expired".to_string(),
                unified_diff: "diff --git a/a b/a\n".to_string(),
            },
        )
        .unwrap();
    let requested = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { .. },
                    ..
                })
            )
        })
    });
    let approval = requested
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(approval.clone()),
            _ => None,
        })
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "respond_expired",
            RuntimeCommand::RespondToApproval {
                request_id: approval.id.clone(),
                response: viden_types::ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    let resolved = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandRejected { reason, .. },
                    ..
                }) if reason.contains("expired")
            )
        })
    });
    assert!(resolved.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalResolved { audit_id, decision: viden_types::ApprovalDecision::Deny, .. },
                ..
            }) if audit_id == &approval.audit_id
        )
    }));
    assert!(!effects.calls.lock().unwrap().contains(&"apply".to_string()));
}

#[test]
fn lane_supervisor_plan_transition_precedes_queued_apply_effect() {
    let cwd = temp_dir("lane_plan_order_cwd");
    let home = temp_dir("lane_plan_order_home");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let owner = owner("lane-plan-order");
    let mut autonomous = lane("lane-plan-order");
    autonomous.mutation_policy = MutationPolicy::Autonomous;
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "create_plan_order",
            RuntimeCommand::CreateLane { lane: autonomous },
        )
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "accept_plan_order",
            RuntimeCommand::AcceptLaneOutput {
                lane_id: "lane-plan-order".to_string(),
                summary: "ready".to_string(),
            },
        )
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "set_plan_order",
            RuntimeCommand::SetWorkMode {
                mode: WorkMode::Plan,
            },
        )
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "apply_after_plan",
            RuntimeCommand::ApplyLaneChanges {
                lane_id: "lane-plan-order".to_string(),
                unified_diff: "diff --git a/a b/a\n".to_string(),
            },
        )
        .unwrap();
    let events = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandRejected { command_id, reason },
                    ..
                }) if command_id == "apply_after_plan" && reason.contains("Plan mode")
            )
        })
    });
    let plan_sequence = events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::SnapshotUpdated { snapshot },
                    ..
                }) if snapshot.work_mode == WorkMode::Plan
            )
        })
        .unwrap()
        .cursor
        .sequence;
    let rejection_sequence = events
        .iter()
        .find(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandRejected { command_id, .. },
                    ..
                }) if command_id == "apply_after_plan"
            )
        })
        .unwrap()
        .cursor
        .sequence;
    assert!(plan_sequence < rejection_sequence);
    assert!(!effects.calls.lock().unwrap().contains(&"apply".to_string()));
}

#[test]
fn lane_supervisor_permission_downgrade_precedes_queued_approval_response() {
    let cwd = temp_dir("lane_permission_order_cwd");
    let home = temp_dir("lane_permission_order_home");
    persist_done_propose_lane(&home, &cwd, "lane-permission-order");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let lane_owner = owner("lane-permission-order");
    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "apply_before_read_only",
            RuntimeCommand::ApplyLaneChanges {
                lane_id: "lane-permission-order".to_string(),
                unified_diff: "diff --git a/a b/a\n".to_string(),
            },
        )
        .unwrap();
    let requested = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalRequested { approval }, .. }) if approval.tool_name == "lane_apply"))
    });
    let request_id = requested
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(approval.id.clone()),
            _ => None,
        })
        .unwrap();
    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "set_read_only_before_resume",
            RuntimeCommand::SetPermissionLevel {
                level: PermissionLevel::ReadOnly,
            },
        )
        .unwrap();
    supervisor
        .send_command_from_owner(
            lane_owner,
            "resume_after_read_only",
            RuntimeCommand::RespondToApproval {
                request_id: request_id.clone(),
                response: viden_types::ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    let events = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalResolved { request_id: resolved, decision: viden_types::ApprovalDecision::Deny, .. }, .. }) if resolved == &request_id))
    });
    assert!(events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::SnapshotUpdated { snapshot }, .. }) if snapshot.permission_level == PermissionLevel::ReadOnly)));
    assert!(!effects.calls.lock().unwrap().contains(&"apply".to_string()));
}

#[test]
fn lane_supervisor_approval_uses_permission_snapshot_before_later_build() {
    let _guard = SUPERVISOR_COMMAND_HOOK_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap();
    let cwd = temp_dir("lane_permission_snapshot_cwd");
    let home = temp_dir("lane_permission_snapshot_home");
    persist_done_propose_lane(&home, &cwd, "lane-permission-snapshot");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let lane_owner = owner("lane-permission-snapshot");
    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "apply_before_snapshot_downgrade",
            RuntimeCommand::ApplyLaneChanges {
                lane_id: "lane-permission-snapshot".to_string(),
                unified_diff: "diff --git a/a b/a\n".to_string(),
            },
        )
        .unwrap();
    let requested = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalRequested { approval }, .. }) if approval.tool_name == "lane_apply"))
    });
    let request_id = requested
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(approval.id.clone()),
            _ => None,
        })
        .unwrap();
    let (entered_sender, entered_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::channel();
    let release_receiver = Arc::new(Mutex::new(release_receiver));
    set_before_supervisor_command_hook(Some(Arc::new(move |command_id| {
        if command_id == "set_snapshot_read_only" {
            let _ = entered_sender.send(());
            let _ = release_receiver.lock().unwrap().recv();
        }
    })));

    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "set_snapshot_read_only",
            RuntimeCommand::SetPermissionLevel {
                level: PermissionLevel::ReadOnly,
            },
        )
        .unwrap();
    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "allow_under_read_only_snapshot",
            RuntimeCommand::RespondToApproval {
                request_id: request_id.clone(),
                response: viden_types::ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    entered_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("supervisor reached the read-only command barrier");
    supervisor
        .send_command_from_owner(
            lane_owner,
            "restore_build_after_response",
            RuntimeCommand::SetPermissionLevel {
                level: PermissionLevel::Ask,
            },
        )
        .unwrap();
    release_sender.send(()).unwrap();
    let resolved = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalResolved { request_id: resolved, decision: viden_types::ApprovalDecision::Deny, .. }, .. }) if resolved == &request_id))
            && events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::SnapshotUpdated { snapshot }, .. }) if snapshot.permission_level == PermissionLevel::Ask))
    });
    set_before_supervisor_command_hook(None);

    assert!(resolved.iter().any(|envelope| matches!(
        &envelope.event,
        RuntimeWireEvent::Known(RuntimeEvent {
            kind: RuntimeEventKind::ApprovalResolved {
                decision: viden_types::ApprovalDecision::Deny,
                ..
            },
            ..
        })
    )));
    let deny_sequence = resolved
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind:
                    RuntimeEventKind::ApprovalResolved {
                        request_id: resolved,
                        decision: viden_types::ApprovalDecision::Deny,
                        ..
                    },
                ..
            }) if resolved == &request_id => Some(envelope.cursor.sequence),
            _ => None,
        })
        .unwrap();
    let build_sequence = resolved
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::SnapshotUpdated { snapshot },
                ..
            }) if snapshot.permission_level == PermissionLevel::Ask => {
                Some(envelope.cursor.sequence)
            }
            _ => None,
        })
        .unwrap();
    assert!(deny_sequence < build_sequence);
    assert!(!effects.calls.lock().unwrap().contains(&"apply".to_string()));
}

#[test]
fn lane_supervisor_approval_uses_build_snapshot_before_later_read_only() {
    let cwd = temp_dir("lane_build_snapshot_cwd");
    let home = temp_dir("lane_build_snapshot_home");
    persist_done_propose_lane(&home, &cwd, "lane-build-snapshot");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let (effects, effect_entered, release_effect) = BlockingApplyLaneEffects::new();
    let effects = Arc::new(effects);
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let lane_owner = owner("lane-build-snapshot");
    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "apply_before_build_snapshot",
            RuntimeCommand::ApplyLaneChanges {
                lane_id: "lane-build-snapshot".to_string(),
                unified_diff: "diff --git a/a b/a\n".to_string(),
            },
        )
        .unwrap();
    let requested = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalRequested { approval }, .. }) if approval.tool_name == "lane_apply"))
    });
    let request_id = requested
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(approval.id.clone()),
            _ => None,
        })
        .unwrap();
    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "allow_under_build_snapshot",
            RuntimeCommand::RespondToApproval {
                request_id: request_id.clone(),
                response: viden_types::ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    effect_entered
        .recv_timeout(Duration::from_secs(2))
        .expect("lane worker entered the approved effect");
    supervisor
        .send_command_from_owner(
            lane_owner,
            "set_read_only_after_response",
            RuntimeCommand::SetPermissionLevel {
                level: PermissionLevel::ReadOnly,
            },
        )
        .unwrap();
    let started = Instant::now();
    let mut events = Vec::new();
    while started.elapsed() < Duration::from_millis(250) {
        if let Some(event) = supervisor.recv_event_envelope_timeout(Duration::from_millis(25)) {
            events.push(event);
        }
    }
    let snapshot_before_effect_completed = events.iter().any(|envelope| {
        matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::SnapshotUpdated { snapshot }, .. }) if snapshot.permission_level == PermissionLevel::ReadOnly)
    });
    release_effect.send(()).unwrap();
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5)
        && !(events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalResolved { request_id: resolved, decision: viden_types::ApprovalDecision::Allow { .. }, .. }, .. }) if resolved == &request_id))
            && events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::SnapshotUpdated { snapshot }, .. }) if snapshot.permission_level == PermissionLevel::ReadOnly)))
    {
        if let Some(event) =
            supervisor.recv_event_envelope_timeout(Duration::from_millis(50))
        {
            events.push(event);
        }
    }

    assert!(
        !snapshot_before_effect_completed,
        "a later permission snapshot must wait for the approved effect barrier"
    );
    let resolved_sequence = events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind:
                    RuntimeEventKind::ApprovalResolved {
                        request_id: resolved,
                        decision: viden_types::ApprovalDecision::Allow { .. },
                        ..
                    },
                ..
            }) if resolved == &request_id => Some(envelope.cursor.sequence),
            _ => None,
        })
        .expect("approval resolved before the later snapshot");
    let snapshot_sequence = events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::SnapshotUpdated { snapshot },
                ..
            }) if snapshot.permission_level == PermissionLevel::ReadOnly => {
                Some(envelope.cursor.sequence)
            }
            _ => None,
        })
        .expect("later ReadOnly snapshot was published");
    assert!(resolved_sequence < snapshot_sequence);
    assert!(effects.calls.lock().unwrap().contains(&"apply".to_string()));
}

#[test]
fn lane_supervisor_rejects_approval_after_permission_epoch_round_trip() {
    let cwd = temp_dir("lane_permission_epoch_round_trip_cwd");
    let home = temp_dir("lane_permission_epoch_round_trip_home");
    persist_done_propose_lane(&home, &cwd, "lane-epoch-round-trip");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let lane_owner = owner("lane-epoch-round-trip");
    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "apply_before_epoch_round_trip",
            RuntimeCommand::ApplyLaneChanges {
                lane_id: "lane-epoch-round-trip".to_string(),
                unified_diff: "diff --git a/a b/a\n".to_string(),
            },
        )
        .unwrap();
    let requested = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalRequested { approval }, .. }) if approval.tool_name == "lane_apply"))
    });
    let request_id = requested
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(approval.id.clone()),
            _ => None,
        })
        .unwrap();
    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "epoch_round_trip_read_only",
            RuntimeCommand::SetPermissionLevel {
                level: PermissionLevel::ReadOnly,
            },
        )
        .unwrap();
    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "epoch_round_trip_build",
            RuntimeCommand::SetPermissionLevel {
                level: PermissionLevel::Ask,
            },
        )
        .unwrap();
    supervisor
        .send_command_from_owner(
            lane_owner,
            "allow_after_epoch_round_trip",
            RuntimeCommand::RespondToApproval {
                request_id: request_id.clone(),
                response: viden_types::ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalResolved { request_id: resolved, decision: viden_types::ApprovalDecision::Deny, .. }, .. }) if resolved == &request_id))
    });

    assert!(!effects.calls.lock().unwrap().contains(&"apply".to_string()));
}

#[test]
fn lane_supervisor_pairs_applied_engine_with_its_applied_epoch() {
    let _guard = SUPERVISOR_COMMAND_HOOK_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap();
    let cases = [
        (
            "permission",
            RuntimeCommand::SetPermissionLevel {
                level: PermissionLevel::Ask,
            },
            vec![RuntimeCommand::SetPermissionLevel {
                level: PermissionLevel::Auto,
            }],
        ),
        (
            "work-mode",
            RuntimeCommand::SetWorkMode {
                mode: WorkMode::Build,
            },
            vec![
                RuntimeCommand::SetWorkMode {
                    mode: WorkMode::Plan,
                },
                RuntimeCommand::SetWorkMode {
                    mode: WorkMode::Build,
                },
            ],
        ),
    ];

    for (case, first_control, later_controls) in cases {
        let lane_id = format!("lane-applied-epoch-{case}");
        let cwd = temp_dir(&format!("lane_applied_epoch_{case}_cwd"));
        let home = temp_dir(&format!("lane_applied_epoch_{case}_home"));
        persist_done_propose_lane(&home, &cwd, &lane_id);
        let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
        let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
        engine
            .set_permission_mode(viden_types::PermissionMode::DontAsk)
            .unwrap();
        let effects = Arc::new(RecordingLaneEffects::default());
        let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
            engine,
            effects.clone() as Arc<dyn LaneEffectExecutor>,
        );
        let lane_owner = owner(&lane_id);
        let first_command_id = format!("first_control_{case}");
        let (entered_sender, entered_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let release_receiver = Arc::new(Mutex::new(release_receiver));
        let expected_command_id = first_command_id.clone();
        set_before_supervisor_command_hook(Some(Arc::new(move |command_id| {
            if command_id == expected_command_id {
                let _ = entered_sender.send(());
                let _ = release_receiver.lock().unwrap().recv();
            }
        })));

        supervisor
            .send_command_from_owner(lane_owner.clone(), first_command_id, first_control)
            .unwrap();
        entered_receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("supervisor paused before applying the first control command");
        supervisor
            .send_command_from_owner(
                lane_owner.clone(),
                format!("apply_between_controls_{case}"),
                RuntimeCommand::ApplyLaneChanges {
                    lane_id: lane_id.clone(),
                    unified_diff: "diff --git a/a b/a\n".to_string(),
                },
            )
            .unwrap();
        for (index, control) in later_controls.into_iter().enumerate() {
            supervisor
                .send_command_from_owner(
                    lane_owner.clone(),
                    format!("later_control_{case}_{index}"),
                    control,
                )
                .unwrap();
        }
        release_sender.send(()).unwrap();
        let requested = collect_envelopes_until(&supervisor, |events| {
            events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalRequested { approval }, .. }) if approval.tool_name == "lane_apply"))
        });
        let request_id = requested
            .iter()
            .find_map(|envelope| match &envelope.event {
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { approval },
                    ..
                }) if approval.tool_name == "lane_apply" => Some(approval.id.clone()),
                _ => None,
            })
            .unwrap();
        supervisor
            .send_command_from_owner(
                lane_owner,
                format!("allow_stale_applied_epoch_{case}"),
                RuntimeCommand::RespondToApproval {
                    request_id: request_id.clone(),
                    response: viden_types::ApprovalResponse::allow_once(None),
                },
            )
            .unwrap();
        collect_envelopes_until(&supervisor, |events| {
            events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalResolved { request_id: resolved, decision: viden_types::ApprovalDecision::Deny, .. }, .. }) if resolved == &request_id))
        });
        set_before_supervisor_command_hook(None);

        assert!(
            !effects.calls.lock().unwrap().contains(&"apply".to_string()),
            "{case} stale approval must not execute after later controls apply"
        );
    }
}

#[test]
fn failed_permission_controls_leave_lane_engine_and_epoch_unchanged() {
    for (case, successful_appends, control) in [
        (
            "permission",
            0,
            RuntimeCommand::SetPermissionLevel {
                level: PermissionLevel::Auto,
            },
        ),
        (
            "work-mode",
            1,
            RuntimeCommand::SetWorkMode {
                mode: WorkMode::Plan,
            },
        ),
    ] {
        let lane_id = format!("lane-failed-control-{case}");
        let cwd = temp_dir(&format!("lane_failed_control_{case}_cwd"));
        let home = temp_dir(&format!("lane_failed_control_{case}_home"));
        persist_done_propose_lane(&home, &cwd, &lane_id);
        let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
        let engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
        let before = engine.runtime_snapshot();
        engine.fail_after_transcript_appends_for_test(successful_appends);
        let effects = Arc::new(RecordingLaneEffects::default());
        let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
            engine,
            effects.clone() as Arc<dyn LaneEffectExecutor>,
        );
        let lane_owner = owner(&lane_id);
        supervisor
            .send_command_from_owner(
                lane_owner.clone(),
                format!("apply_before_failed_{case}"),
                RuntimeCommand::ApplyLaneChanges {
                    lane_id: lane_id.clone(),
                    unified_diff: "diff --git a/a b/a\n".to_string(),
                },
            )
            .unwrap();
        let requested = collect_envelopes_until(&supervisor, |events| {
            events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalRequested { approval }, .. }) if approval.tool_name == "lane_apply"))
        });
        let request_id = requested
            .iter()
            .find_map(|envelope| match &envelope.event {
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { approval },
                    ..
                }) if approval.tool_name == "lane_apply" => Some(approval.id.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            supervisor
                .lane_permission_snapshot_for_test(&lane_id)
                .unwrap(),
            (PermissionMode::Default, 0)
        );

        supervisor
            .send_command_from_owner(
                lane_owner.clone(),
                format!("failed_control_{case}"),
                control,
            )
            .unwrap();
        let failed = collect_envelopes_until(&supervisor, |events| {
            events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::Error { error }, .. }) if error.message.contains("injected transcript append failure")))
        });

        assert!(failed.iter().all(|envelope| !matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalResolved { request_id: resolved, .. },
                ..
            }) if resolved == &request_id
        )));
        assert_eq!(supervisor.snapshot_envelope().unwrap().snapshot, before);
        assert_eq!(
            supervisor
                .lane_permission_snapshot_for_test(&lane_id)
                .unwrap(),
            (PermissionMode::Default, 0),
            "{case} failure must not install a new lane engine under the old epoch"
        );
        assert!(!effects.calls.lock().unwrap().contains(&"apply".to_string()));

        supervisor
            .send_command_from_owner(
                lane_owner,
                format!("allow_after_failed_{case}"),
                RuntimeCommand::RespondToApproval {
                    request_id: request_id.clone(),
                    response: viden_types::ApprovalResponse::allow_once(None),
                },
            )
            .unwrap();
        // ApprovalResolved is emitted before the approved effect runs. The
        // response command is accepted only after LaneSupervisor's completion
        // barrier, so wait for both before observing the effect recorder.
        let completed = collect_envelopes_until(&supervisor, |events| {
            events.iter().any(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::ApprovalResolved {
                            request_id: resolved,
                            decision: viden_types::ApprovalDecision::Allow { .. },
                            ..
                        },
                        ..
                    }) if resolved == &request_id
                )
            }) && events.iter().any(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::CommandAccepted { command_id, .. },
                        ..
                    }) if command_id == &format!("allow_after_failed_{case}")
                )
            })
        });
        assert!(completed.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandAccepted { command_id, .. },
                    ..
                }) if command_id == &format!("allow_after_failed_{case}")
            )
        }));
        assert!(
            effects.calls.lock().unwrap().contains(&"apply".to_string()),
            "{case} AllowOnce must retain the lane approval epoch from before the failed control"
        );
    }
}

#[test]
fn lane_supervisor_repo_allow_rule_survives_authoritative_permission_sync() {
    let cwd = temp_dir("lane_allow_rule_sync_cwd");
    let home = temp_dir("lane_allow_rule_sync_home");
    let mut record = autonomous_lane("lane-allow-rule");
    record.status = LaneStatus::Running;
    persist_lane(&home, &cwd, record);
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let lane_owner = owner("lane-allow-rule");
    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "input_first",
            RuntimeCommand::SendLaneInput {
                lane_id: "lane-allow-rule".to_string(),
                input: "first".to_string(),
            },
        )
        .unwrap();
    let requested = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalRequested { approval }, .. }) if approval.tool_name == "lane_send_input"))
    });
    let approval = requested
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(approval.clone()),
            _ => None,
        })
        .unwrap();
    let paths = approval
        .allowed_scopes
        .iter()
        .find_map(|scope| match scope {
            viden_types::ApprovalScope::RepoAllowlist { paths } => Some(paths.clone()),
            _ => None,
        })
        .unwrap();
    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "approve_input_repo",
            RuntimeCommand::RespondToApproval {
                request_id: approval.id,
                response: viden_types::ApprovalResponse {
                    decision: viden_types::ApprovalDecision::Allow {
                        scope: viden_types::ApprovalScope::RepoAllowlist { paths },
                    },
                    feedback: None,
                },
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::Error { .. },
                    ..
                })
            )
        })
    });
    supervisor
        .send_command_from_owner(
            lane_owner,
            "input_second",
            RuntimeCommand::SendLaneInput {
                lane_id: "lane-allow-rule".to_string(),
                input: "second".to_string(),
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |_| {
        effects
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| call.as_str() == "input:lane-allow-rule")
            .count()
            == 2
    });
}

#[test]
fn lane_supervisor_propose_only_still_asks_when_repo_rule_allows() {
    let cwd = temp_dir("lane_propose_rule_cwd");
    let home = temp_dir("lane_propose_rule_home");
    let mut record = lane("lane-propose-rule");
    record.status = LaneStatus::Running;
    persist_lane(&home, &cwd, record);
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let lane_owner = owner("lane-propose-rule");
    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "propose_input_first",
            RuntimeCommand::SendLaneInput {
                lane_id: "lane-propose-rule".to_string(),
                input: "first".to_string(),
            },
        )
        .unwrap();
    let requested = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalRequested { approval }, .. }) if approval.tool_name == "lane_send_input"))
    });
    let approval = requested
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(approval.clone()),
            _ => None,
        })
        .unwrap();
    let paths = approval
        .allowed_scopes
        .iter()
        .find_map(|scope| match scope {
            viden_types::ApprovalScope::RepoAllowlist { paths } => Some(paths.clone()),
            _ => None,
        })
        .unwrap();
    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "approve_propose_repo",
            RuntimeCommand::RespondToApproval {
                request_id: approval.id,
                response: viden_types::ApprovalResponse {
                    decision: viden_types::ApprovalDecision::Allow {
                        scope: viden_types::ApprovalScope::RepoAllowlist { paths },
                    },
                    feedback: None,
                },
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::Error { .. },
                    ..
                })
            )
        })
    });
    supervisor
        .send_command_from_owner(
            lane_owner,
            "propose_input_second",
            RuntimeCommand::SendLaneInput {
                lane_id: "lane-propose-rule".to_string(),
                input: "second".to_string(),
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalRequested { approval }, .. }) if approval.tool_name == "lane_send_input"))
    });
    assert_eq!(
        effects
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| call.as_str() == "input:lane-propose-rule")
            .count(),
        1
    );
}

#[test]
fn lane_supervisor_session_allow_rule_does_not_cross_lane_owner() {
    assert_lane_approval_scope_isolated(false);
}

#[test]
fn lane_supervisor_repo_allow_rule_does_not_cross_lane_owner() {
    assert_lane_approval_scope_isolated(true);
}

#[cfg(unix)]
#[test]
fn lane_supervisor_rejects_symlink_escape_targets_before_effects() {
    let cwd = temp_dir("lane_symlink_escape_cwd");
    let home = temp_dir("lane_symlink_escape_home");
    let outside = temp_dir("lane_symlink_escape_outside");
    std::fs::create_dir_all(cwd.join(".worktrees")).unwrap();
    for (lane_id, status) in [
        ("lane-escape-start", LaneStatus::Draft),
        ("lane-escape-send", LaneStatus::Running),
        ("lane-escape-cleanup", LaneStatus::Draft),
    ] {
        std::os::unix::fs::symlink(&outside, cwd.join(".worktrees").join(lane_id)).unwrap();
        let mut record = autonomous_lane(lane_id);
        record.status = status;
        persist_lane(&home, &cwd, record);
    }
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::BypassPermissions)
        .unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let commands = [
        (
            owner("lane-escape-start"),
            "start_symlink_escape",
            RuntimeCommand::StartLane {
                lane_id: "lane-escape-start".to_string(),
                command: "worker".to_string(),
                args: Vec::new(),
                env: Vec::new(),
                output_log: None,
            },
        ),
        (
            owner("lane-escape-send"),
            "send_symlink_escape",
            RuntimeCommand::SendLaneInput {
                lane_id: "lane-escape-send".to_string(),
                input: "blocked".to_string(),
            },
        ),
        (
            owner("lane-escape-cleanup"),
            "cleanup_symlink_escape",
            RuntimeCommand::CleanupLane {
                lane_id: "lane-escape-cleanup".to_string(),
                force: true,
            },
        ),
    ];
    for (lane_owner, command_id, command) in commands {
        supervisor
            .send_command_from_owner(lane_owner, command_id, command)
            .unwrap();
    }
    collect_envelopes_until(&supervisor, |events| {
        events
            .iter()
            .filter(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::CommandRejected { reason, .. }, .. }) if reason.contains("escapes")))
            .count()
            == 3
    });
    assert!(effects.calls.lock().unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn lane_supervisor_rejects_create_below_symlinked_parent() {
    let cwd = temp_dir("lane_create_symlink_parent_cwd");
    let home = temp_dir("lane_create_symlink_parent_home");
    let worktree_storage = cwd.join("worktree-storage");
    std::fs::create_dir_all(&worktree_storage).unwrap();
    std::os::unix::fs::symlink(&worktree_storage, cwd.join(".worktrees")).unwrap();
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::BypassPermissions)
        .unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    supervisor
        .send_command_from_owner(
            owner("lane-create-symlink-parent"),
            "create_below_symlink_parent",
            RuntimeCommand::CreateLane {
                lane: autonomous_lane("lane-create-symlink-parent"),
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::CommandRejected { command_id, reason }, .. }) if command_id == "create_below_symlink_parent" && reason.contains("symlink parent")))
    });
    assert!(effects.calls.lock().unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn lane_supervisor_approval_target_resolves_in_scope_symlink() {
    let cwd = temp_dir("lane_symlink_target_cwd");
    let home = temp_dir("lane_symlink_target_home");
    let real = cwd.join("real-lane");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::create_dir_all(cwd.join(".worktrees")).unwrap();
    std::os::unix::fs::symlink(&real, cwd.join(".worktrees/lane-symlink-target")).unwrap();
    let mut record = lane("lane-symlink-target");
    record.status = LaneStatus::Running;
    persist_lane(&home, &cwd, record);
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        Arc::new(RecordingLaneEffects::default()) as Arc<dyn LaneEffectExecutor>,
    );
    supervisor
        .send_command_from_owner(
            owner("lane-symlink-target"),
            "send_symlink_target",
            RuntimeCommand::SendLaneInput {
                lane_id: "lane-symlink-target".to_string(),
                input: "review target".to_string(),
            },
        )
        .unwrap();
    let events = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalRequested { approval }, .. }) if approval.tool_name == "lane_send_input"))
    });
    let target = events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(&approval.target),
            _ => None,
        })
        .unwrap();
    let canonical_real = real.canonicalize().unwrap();
    assert_eq!(
        target.canonical_ref.as_deref(),
        Some(canonical_real.to_string_lossy().as_ref())
    );
}

#[test]
fn lane_supervisor_rejects_duplicate_start_and_retires_archived_worker() {
    let cwd = temp_dir("lane_lifecycle_retire_cwd");
    let home = temp_dir("lane_lifecycle_retire_home");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let owner = owner("lane-retire");
    let mut autonomous = lane("lane-retire");
    autonomous.mutation_policy = MutationPolicy::Autonomous;
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "create_retire",
            RuntimeCommand::CreateLane { lane: autonomous },
        )
        .unwrap();
    for command_id in ["start_retire", "start_retire_duplicate"] {
        supervisor
            .send_command_from_owner(
                owner.clone(),
                command_id,
                RuntimeCommand::StartLane {
                    lane_id: "lane-retire".to_string(),
                    command: "worker".to_string(),
                    args: Vec::new(),
                    env: Vec::new(),
                    output_log: None,
                },
            )
            .unwrap();
    }
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "archive_retire",
            RuntimeCommand::ArchiveLane {
                lane_id: "lane-retire".to_string(),
                summary: "archived".to_string(),
            },
        )
        .unwrap();
    // Commands are queued asynchronously, so an empty worker registry is also
    // true before `create_retire` is processed. Wait on the retirement signal
    // instead, which is only recorded once the archived worker exited.
    wait_until(
        Duration::from_secs(5),
        "lane `lane-retire` worker retirement after archive",
        || supervisor.lane_worker_retired_for_test("lane-retire"),
    );
    assert_eq!(supervisor.active_lane_worker_count_for_test(), 0);
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "attach_after_archive",
            RuntimeCommand::AttachLane {
                lane_id: "lane-retire".to_string(),
            },
        )
        .unwrap();
    let events = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandRejected { command_id, .. },
                    ..
                }) if command_id == "attach_after_archive"
            )
        })
    });
    assert!(events.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::CommandRejected { command_id, .. },
                ..
            }) if command_id == "start_retire_duplicate"
        )
    }));
    assert!(supervisor.lane_worker_retired_for_test("lane-retire"));
    assert!(supervisor.lane_worker_finished_for_test("lane-retire"));
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "archive_retire_again",
            RuntimeCommand::ArchiveLane {
                lane_id: "lane-retire".to_string(),
                summary: "duplicate archive".to_string(),
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::CommandRejected { command_id, reason }, .. }) if command_id == "archive_retire_again" && reason.contains("terminal")))
    });
    let calls = effects.calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .filter(|call| *call == "start:lane-retire")
            .count(),
        1
    );
    assert_eq!(
        calls
            .iter()
            .filter(|call| *call == "stop:lane-retire")
            .count(),
        1
    );
}

#[test]
fn lane_supervisor_hydrates_persisted_lane_for_restart_commands() {
    let cwd = temp_dir("lane_restart_hydrate_cwd");
    let home = temp_dir("lane_restart_hydrate_home");
    let owner = owner("lane-restart");
    {
        let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
        let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
        engine
            .set_permission_mode(viden_types::PermissionMode::DontAsk)
            .unwrap();
        let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
            engine,
            Arc::new(RecordingLaneEffects::default()) as Arc<dyn LaneEffectExecutor>,
        );
        supervisor
            .send_command_from_owner(
                owner.clone(),
                "create_restart",
                RuntimeCommand::CreateLane {
                    lane: autonomous_lane("lane-restart"),
                },
            )
            .unwrap();
        collect_envelopes_until(&supervisor, |events| {
            events.iter().any(|envelope| {
                matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::LaneUpdated { lane },
                        ..
                    }) if lane.id == "lane-restart"
                )
            })
        });
    }

    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        Arc::new(RecordingLaneEffects::default()) as Arc<dyn LaneEffectExecutor>,
    );
    assert!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .lanes
            .iter()
            .any(|lane| lane.id == "lane-restart")
    );
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "accept_after_restart",
            RuntimeCommand::AcceptLaneOutput {
                lane_id: "lane-restart".to_string(),
                summary: "resumed".to_string(),
            },
        )
        .unwrap();
    let events = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneUpdated { lane },
                    ..
                }) if lane.id == "lane-restart" && lane.status == LaneStatus::Done
            )
        })
    });
    assert!(events.iter().all(|envelope| {
        envelope.owner == owner
            || matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::WorkspaceSourceUpdated { .. }
                        | RuntimeEventKind::RuntimeServiceHealthUpdated { .. },
                    ..
                })
            )
    }));
}

#[test]
fn lane_supervisor_hydration_failure_blocks_duplicate_create_effects() {
    let cwd = temp_dir("lane_hydration_failure_cwd");
    let home = temp_dir("lane_hydration_failure_home");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let store = WorkflowStore::new(home, &cwd).unwrap();
    let persistence = Arc::new(InjectableLanePersistence::with_load_failure(store));
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_and_persistence_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
        persistence,
    );

    supervisor
        .send_command_from_owner(
            owner("lane-load-fail"),
            "create_after_load_failure",
            RuntimeCommand::CreateLane {
                lane: lane("lane-load-fail"),
            },
        )
        .unwrap();
    let events = collect_envelopes_until(&supervisor, |events| {
        let rejected = events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandRejected { reason, .. },
                    ..
                }) if reason.contains("hydration")
            )
        });
        let errored = events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::Error { .. },
                    ..
                })
            )
        });
        rejected && errored
    });
    assert!(events.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::Error { error },
                ..
            }) if error.recoverable
        )
    }));
    assert!(!events.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::LaneRuntimeOwnerBound { .. },
                ..
            })
        )
    }));
    assert!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .lane_runtime_owners
            .is_empty()
    );
    assert!(effects.calls.lock().unwrap().is_empty());
}

#[test]
fn lane_hydration_keeps_first_persisted_origin_as_legacy_owner() {
    let cwd = temp_dir("lane_legacy_origin_cwd");
    let home = temp_dir("lane_legacy_origin_home");
    let store = WorkflowStore::new(home, &cwd).unwrap();
    store
        .append_lane_event_checked(&LaneEvent::created(
            "lane-origin-created",
            lane("lane-origin"),
            1,
            Some("session-origin".to_string()),
        ))
        .unwrap();
    store
        .append_lane_event_checked(&LaneEvent::status_changed(
            "lane-origin-updated",
            "lane-origin",
            LaneStatus::Done,
            "updated elsewhere",
            2,
            Some("session-unrelated".to_string()),
        ))
        .unwrap();

    let hydrated = WorkflowLanePersistence(store).load_lanes().unwrap();
    assert_eq!(
        hydrated["lane-origin"].active_session_ids,
        vec!["session-origin".to_string()]
    );
}

#[test]
fn lane_hydration_does_not_treat_durable_agent_identity_as_a_live_session() {
    let cwd = temp_dir("lane_agent_identity_not_live_cwd");
    let home = temp_dir("lane_agent_identity_not_live_home");
    let store = WorkflowStore::new(home, &cwd).unwrap();
    store
        .append_lane_event_checked(&LaneEvent::created(
            "lane-agent-identity-created",
            lane("lane-agent-identity"),
            1,
            None,
        ))
        .unwrap();
    store
        .bind_lane_agent_once(
            "lane-agent-identity",
            "codex-acp",
            "session-durable-identity",
            2,
        )
        .unwrap();

    let hydrated = WorkflowLanePersistence(store).load_lanes().unwrap();
    assert!(
        hydrated["lane-agent-identity"]
            .active_session_ids
            .is_empty()
    );
}

#[test]
fn lane_supervisor_hydration_marks_active_lane_recoverable_and_enforces_session_owner() {
    let cwd = temp_dir("lane_hydration_owner_cwd");
    let home = temp_dir("lane_hydration_owner_home");
    let first_owner = owner("lane-hydration-owner");
    let store = WorkflowStore::new(home.clone(), &cwd).unwrap();
    {
        let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
        let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
        engine
            .set_permission_mode(viden_types::PermissionMode::DontAsk)
            .unwrap();
        let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
            engine,
            Arc::new(RecordingLaneEffects::default()) as Arc<dyn LaneEffectExecutor>,
        );
        supervisor
            .send_command_from_owner(
                first_owner.clone(),
                "create_hydration_owner",
                RuntimeCommand::CreateLane {
                    lane: autonomous_lane("lane-hydration-owner"),
                },
            )
            .unwrap();
        collect_envelopes_until(&supervisor, |events| {
            events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::LaneUpdated { lane }, .. }) if lane.id == "lane-hydration-owner"))
        });
    }
    store
        .append_lane_event_checked(&LaneEvent::status_changed(
            "lane-event-active",
            "lane-hydration-owner",
            LaneStatus::Running,
            "runtime was active",
            viden_types::now_timestamp(),
            first_owner.session_id.clone(),
        ))
        .unwrap();

    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        Arc::new(RecordingLaneEffects::default()) as Arc<dyn LaneEffectExecutor>,
    );
    let snapshot = supervisor.snapshot_envelope().unwrap();
    assert!(snapshot.view.lanes.iter().any(|lane| {
        lane.id == "lane-hydration-owner"
            && lane.status == LaneStatus::Blocked
            && lane.summary.contains("recovery")
    }));
    assert!(snapshot.view.lane_recoveries.iter().any(|recovery| {
        recovery.lane_id == "lane-hydration-owner" && recovery.reason.contains("recovery")
    }));
    let mut unrelated = first_owner.clone();
    unrelated.session_id = Some("session-unrelated".to_string());
    unrelated.turn_id = Some("turn-unrelated".to_string());
    supervisor
        .send_command_from_owner(
            unrelated,
            "accept_unrelated_hydrated_lane",
            RuntimeCommand::AcceptLaneOutput {
                lane_id: "lane-hydration-owner".to_string(),
                summary: "steal lane".to_string(),
            },
        )
        .unwrap();
    let events = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::CommandRejected { command_id, reason }, .. }) if command_id == "accept_unrelated_hydrated_lane" && reason.contains("owner mismatch")))
    });
    assert!(events.iter().any(|envelope| matches!(
        &envelope.event,
        RuntimeWireEvent::Known(RuntimeEvent {
            kind: RuntimeEventKind::CommandRejected { .. },
            ..
        })
    )));
    let recovered = store.load_lane_state().unwrap();
    let recovered = recovered.lane("lane-hydration-owner").unwrap();
    assert_eq!(recovered.status, LaneStatus::Blocked);
    assert!(recovered.summary.contains("recovery"));
}

#[test]
fn lane_supervisor_compensates_create_and_start_and_preserves_cleanup_intent() {
    // Create: the worktree created by this command is removed when the first
    // durable event append fails.
    let cwd = temp_dir("lane_create_compensation_cwd");
    let home = temp_dir("lane_create_compensation_home");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let store = WorkflowStore::new(home, &cwd).unwrap();
    let persistence = Arc::new(InjectableLanePersistence::new(store, [1]));
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_and_persistence_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
        persistence,
    );
    let create_owner = owner("lane-create-fail");
    supervisor
        .send_command_from_owner(
            create_owner.clone(),
            "create_fail",
            RuntimeCommand::CreateLane {
                lane: autonomous_lane("lane-create-fail"),
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneRecoveryRequired { .. },
                    ..
                })
            )
        })
    });
    assert!(
        effects
            .calls
            .lock()
            .unwrap()
            .contains(&"compensate_create:lane-create-fail".to_string())
    );
    supervisor
        .send_command_from_owner(
            create_owner,
            "create_retry",
            RuntimeCommand::CreateLane {
                lane: autonomous_lane("lane-create-fail"),
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::LaneUpdated { lane }, .. }) if lane.id == "lane-create-fail"))
    });
    assert_eq!(
        effects
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| *call == "create:lane-create-fail")
            .count(),
        2
    );
    drop(supervisor);

    // Start: a failed Running append stops exactly the runtime spawned by the
    // command, so a handle cannot leak or be overwritten.
    let cwd = temp_dir("lane_start_compensation_cwd");
    let home = temp_dir("lane_start_compensation_home");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let store = WorkflowStore::new(home, &cwd).unwrap();
    let persistence = Arc::new(InjectableLanePersistence::new(store, [3]));
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_and_persistence_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
        persistence,
    );
    let start_owner = owner("lane-start-fail");
    let mut autonomous = lane("lane-start-fail");
    autonomous.mutation_policy = MutationPolicy::Autonomous;
    supervisor
        .send_command_from_owner(
            start_owner.clone(),
            "create_start_fail",
            RuntimeCommand::CreateLane { lane: autonomous },
        )
        .unwrap();
    supervisor
        .send_command_from_owner(
            start_owner.clone(),
            "start_fail",
            RuntimeCommand::StartLane {
                lane_id: "lane-start-fail".to_string(),
                command: "worker".to_string(),
                args: Vec::new(),
                env: Vec::new(),
                output_log: None,
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::LaneRecoveryRequired { lane_id, .. }, .. }) if lane_id == "lane-start-fail"))
    });
    // The recovery event is observable just before the same worker performs shutdown
    // compensation, so wait for that effect instead of racing the worker thread.
    wait_until(
        Duration::from_secs(5),
        "lane `lane-start-fail` shutdown compensation effect",
        || {
            effects
                .calls
                .lock()
                .map(|calls| calls.iter().any(|call| call == "stop:lane-start-fail"))
                .unwrap_or(false)
        },
    );
    let calls = effects.calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .filter(|call| *call == "start:lane-start-fail")
            .count(),
        1
    );
    assert_eq!(
        calls
            .iter()
            .filter(|call| *call == "stop:lane-start-fail")
            .count(),
        1
    );
    drop(calls);
    supervisor
        .send_command_from_owner(
            start_owner,
            "start_retry",
            RuntimeCommand::StartLane {
                lane_id: "lane-start-fail".to_string(),
                command: "worker".to_string(),
                args: Vec::new(),
                env: Vec::new(),
                output_log: None,
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::LaneUpdated { lane }, .. }) if lane.id == "lane-start-fail" && lane.status == LaneStatus::Running))
    });
    assert_eq!(
        effects
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| *call == "start:lane-start-fail")
            .count(),
        2
    );
    drop(supervisor);

    // Cleanup: completion failure leaves the durable pre-effect Starting
    // intent replayable. No fake recreation is attempted after force removal.
    let cwd = temp_dir("lane_cleanup_intent_cwd");
    let home = temp_dir("lane_cleanup_intent_home");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let store = WorkflowStore::new(home, &cwd).unwrap();
    let persistence = Arc::new(InjectableLanePersistence::new(store.clone(), [3]));
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_and_persistence_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
        persistence,
    );
    let owner = owner("lane-cleanup-fail");
    let mut cleanup_lane = lane("lane-cleanup-fail");
    cleanup_lane.mutation_policy = MutationPolicy::Autonomous;
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "create_cleanup_fail",
            RuntimeCommand::CreateLane { lane: cleanup_lane },
        )
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cleanup_fail",
            RuntimeCommand::CleanupLane {
                lane_id: "lane-cleanup-fail".to_string(),
                force: true,
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::LaneRecoveryRequired { reason, .. }, .. }) if reason.contains("completion")))
    });
    assert!(
        effects
            .calls
            .lock()
            .unwrap()
            .contains(&"cleanup:lane-cleanup-fail".to_string())
    );
    assert_eq!(
        store
            .load_lane_state()
            .unwrap()
            .lane("lane-cleanup-fail")
            .unwrap()
            .status,
        LaneStatus::Starting
    );
    supervisor
        .send_command_from_owner(
            owner,
            "cleanup_retry",
            RuntimeCommand::CleanupLane {
                lane_id: "lane-cleanup-fail".to_string(),
                force: true,
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::LaneUpdated { lane }, .. }) if lane.id == "lane-cleanup-fail" && lane.status == LaneStatus::Archived))
    });
    assert_eq!(
        effects
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| *call == "cleanup:lane-cleanup-fail")
            .count(),
        2
    );
}

#[test]
fn lane_supervisor_retries_real_cleanup_after_completion_append_failure() {
    let cwd = temp_dir("lane_real_cleanup_retry_cwd");
    let home = temp_dir("lane_real_cleanup_retry_home");
    for args in [
        vec!["init"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "Viden Test"],
    ] {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(&cwd)
                .status()
                .unwrap()
                .success()
        );
    }
    std::fs::write(cwd.join("README.md"), "root\n").unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["add", "README.md"])
            .current_dir(&cwd)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(&cwd)
            .status()
            .unwrap()
            .success()
    );

    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let store = WorkflowStore::new(home, &cwd).unwrap();
    let persistence = Arc::new(InjectableLanePersistence::new(store.clone(), [3]));
    let supervisor = RuntimeSupervisor::start_with_lane_effects_and_persistence_for_test(
        engine,
        Arc::new(LocalLaneEffectExecutor::default()) as Arc<dyn LaneEffectExecutor>,
        persistence,
    );
    let owner = owner("lane-real-cleanup");
    let mut autonomous = lane("lane-real-cleanup");
    autonomous.mutation_policy = MutationPolicy::Autonomous;
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "create_real_cleanup",
            RuntimeCommand::CreateLane { lane: autonomous },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::LaneUpdated { lane }, .. }) if lane.id == "lane-real-cleanup"))
    });
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cleanup_real_failure",
            RuntimeCommand::CleanupLane {
                lane_id: "lane-real-cleanup".to_string(),
                force: true,
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::LaneRecoveryRequired { reason, .. }, .. }) if reason.contains("completion")))
    });
    assert!(!cwd.join(".worktrees/lane-real-cleanup").exists());

    supervisor
        .send_command_from_owner(
            owner,
            "cleanup_real_retry",
            RuntimeCommand::CleanupLane {
                lane_id: "lane-real-cleanup".to_string(),
                force: true,
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::LaneUpdated { lane }, .. }) if lane.id == "lane-real-cleanup" && lane.status == LaneStatus::Archived))
    });
    assert_eq!(
        store
            .load_lane_state()
            .unwrap()
            .lane("lane-real-cleanup")
            .unwrap()
            .status,
        LaneStatus::Archived
    );
}

#[test]
fn lane_supervisor_keeps_waiting_lane_isolated_and_routes_owner_events() {
    let cwd = temp_dir("lane_supervisor_owner_routing_cwd");
    let home = temp_dir("lane_supervisor_owner_routing_home");
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let owner_a = owner("lane-a");
    let owner_b = owner("lane-b");
    let mut lane_b = lane("lane-b");
    lane_b.mutation_policy = MutationPolicy::Autonomous;

    supervisor
        .send_command_from_owner(
            owner_a.clone(),
            "cmd_create_a",
            RuntimeCommand::CreateLane {
                lane: lane("lane-a"),
            },
        )
        .unwrap();
    approve_lane_tool(&supervisor, &owner_a, "lane_create", "approve_create_a");
    supervisor
        .send_command_from_owner(
            owner_b.clone(),
            "cmd_create_b",
            RuntimeCommand::CreateLane { lane: lane_b },
        )
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner_a.clone(),
            "cmd_start_a",
            RuntimeCommand::StartLane {
                lane_id: "lane-a".to_string(),
                command: "worker-a".to_string(),
                args: Vec::new(),
                env: Vec::new(),
                output_log: None,
            },
        )
        .unwrap();

    let mut envelopes = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { .. },
                    ..
                })
            )
        })
    });
    let approval_id = envelopes
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(approval.id.clone()),
            _ => None,
        })
        .unwrap();

    supervisor
        .send_command_from_owner(
            owner_b.clone(),
            "cmd_wrong_owner_approval",
            RuntimeCommand::RespondToApproval {
                request_id: approval_id,
                response: viden_types::ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner_b.clone(),
            "cmd_start_b",
            RuntimeCommand::StartLane {
                lane_id: "lane-b".to_string(),
                command: "worker-b".to_string(),
                args: Vec::new(),
                env: Vec::new(),
                output_log: None,
            },
        )
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner_b.clone(),
            "cmd_input_b",
            RuntimeCommand::SendLaneInput {
                lane_id: "lane-b".to_string(),
                input: "continue\n".to_string(),
            },
        )
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner_b.clone(),
            "cmd_accept_b",
            RuntimeCommand::AcceptLaneOutput {
                lane_id: "lane-b".to_string(),
                summary: "lane B complete".to_string(),
            },
        )
        .unwrap();
    supervisor
        .send_command_from_owner(
            owner_a.clone(),
            "cmd_cancel_a",
            RuntimeCommand::CancelActiveTurn,
        )
        .unwrap();

    envelopes.extend(collect_envelopes_until(&supervisor, |events| {
        let has_b_done = events.iter().any(|envelope| {
            envelope.owner == owner_b
                && matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::LaneUpdated { lane },
                        ..
                    }) if lane.id == "lane-b" && lane.status == LaneStatus::Done
                )
        });
        let has_a_cancelled = events.iter().any(|envelope| {
            envelope.owner == owner_a
                && matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::LaneUpdated { lane },
                        ..
                    }) if lane.id == "lane-a" && lane.status == LaneStatus::Cancelled
                )
        });
        has_b_done && has_a_cancelled
    }));

    let calls = effects.calls.lock().unwrap().clone();
    assert!(calls.contains(&"create:lane-a".to_string()));
    assert!(calls.contains(&"create:lane-b".to_string()));
    assert!(calls.contains(&"start:lane-b".to_string()));
    assert!(calls.contains(&"input:lane-b".to_string()));
    assert!(!calls.contains(&"start:lane-a".to_string()));
    assert!(!calls.contains(&"stop:lane-a".to_string()));

    assert!(envelopes.iter().any(|envelope| {
        envelope.owner == owner_b
            && matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::CommandRejected { command_id, reason },
                    ..
                }) if command_id == "cmd_wrong_owner_approval" && reason.contains("owner mismatch")
            )
    }));
    assert!(!envelopes.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::CommandAccepted { command_id, .. },
                ..
            }) if command_id == "cmd_wrong_owner_approval"
        )
    }));
    let queued_sequence = envelopes
        .iter()
        .find(|envelope| {
            envelope.owner == owner_b
                && matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::InputQueued { .. },
                        ..
                    })
                )
        })
        .unwrap()
        .cursor
        .sequence;
    let dequeued_sequence = envelopes
        .iter()
        .find(|envelope| {
            envelope.owner == owner_b
                && matches!(
                    &envelope.event,
                    RuntimeWireEvent::Known(RuntimeEvent {
                        kind: RuntimeEventKind::InputDequeued { .. },
                        ..
                    })
                )
        })
        .unwrap()
        .cursor
        .sequence;
    assert!(queued_sequence < dequeued_sequence);
    // GUI-CORE-010: a queued input carries the exact worker owner Core also
    // published as the Lane's runtime owner binding, so a client can scope it
    // to the selected Lane without attributing it by timing or label.
    let bound_owner = envelopes
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::LaneRuntimeOwnerBound { binding },
                ..
            }) if binding.lane_id == "lane-b" => Some(binding.owner.clone()),
            _ => None,
        })
        .expect("the Lane worker must publish exactly one runtime owner binding");
    let queued_owner = envelopes
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::InputQueued { input },
                ..
            }) if envelope.owner == owner_b => Some(input.owner.clone()),
            _ => None,
        })
        .expect("the queued input fact must be published");
    assert_eq!(queued_owner.as_ref(), Some(&bound_owner));
    assert!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .queued_inputs
            .is_empty()
    );
    assert!(envelopes.iter().any(|envelope| {
        envelope.owner == owner_b
            && matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::InputQueued { .. },
                    ..
                })
            )
    }));
    assert!(envelopes.iter().any(|envelope| {
        envelope.owner == owner_b
            && matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::Error { .. },
                    ..
                })
            )
    }));
    assert!(envelopes.iter().all(|envelope| match &envelope.event {
        RuntimeWireEvent::Known(RuntimeEvent {
            kind: RuntimeEventKind::ApprovalRequested { approval },
            ..
        }) => envelope.owner == owner_a && approval.owner == owner_a,
        RuntimeWireEvent::Known(RuntimeEvent {
            kind: RuntimeEventKind::ApprovalResolved { owner, .. },
            ..
        }) => envelope.owner == owner_a && owner == &owner_a,
        RuntimeWireEvent::Known(RuntimeEvent {
            kind: RuntimeEventKind::LaneUpdated { lane },
            ..
        }) => envelope.owner.lane_id.as_deref() == Some(lane.id.as_str()),
        RuntimeWireEvent::Known(RuntimeEvent {
            kind: RuntimeEventKind::LaneOutputAppended { lane_id, .. },
            ..
        }) => envelope.owner.lane_id.as_deref() == Some(lane_id.as_str()),
        _ => true,
    }));
}

#[derive(Default)]
struct CountingLaneEffects {
    calls: AtomicUsize,
}

#[derive(Default)]
struct RecordingLaneEffects {
    calls: Mutex<Vec<String>>,
}

struct BlockingLaneProvider {
    entered: Arc<AtomicBool>,
}

impl ModelProvider for BlockingLaneProvider {
    fn provider_name(&self) -> &str {
        "blocking-lane"
    }

    fn model(&self) -> &str {
        "blocking-lane-model"
    }

    fn set_model(&mut self, _model: String) {}

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

struct BlockingApplyLaneEffects {
    calls: Mutex<Vec<String>>,
    entered: Mutex<Option<mpsc::Sender<()>>>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl BlockingApplyLaneEffects {
    fn new() -> (Self, mpsc::Receiver<()>, mpsc::Sender<()>) {
        let (entered_sender, entered_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        (
            Self {
                calls: Mutex::new(Vec::new()),
                entered: Mutex::new(Some(entered_sender)),
                release: Mutex::new(release_receiver),
            },
            entered_receiver,
            release_sender,
        )
    }
}

impl LaneEffectExecutor for BlockingApplyLaneEffects {
    fn execute(&self, request: LaneEffectRequest) -> Result<LaneEffectResult, String> {
        assert!(matches!(request, LaneEffectRequest::Apply { .. }));
        if let Some(entered) = self.entered.lock().unwrap().take() {
            let _ = entered.send(());
        }
        let _ = self.release.lock().unwrap().recv();
        self.calls.lock().unwrap().push("apply".to_string());
        Ok(LaneEffectResult::success("effect completed"))
    }
}

impl LaneEffectExecutor for RecordingLaneEffects {
    fn execute(&self, request: LaneEffectRequest) -> Result<LaneEffectResult, String> {
        let call = match request {
            LaneEffectRequest::Create { lane, .. } => format!("create:{}", lane.id),
            LaneEffectRequest::Start { lane, .. } => format!("start:{}", lane.id),
            LaneEffectRequest::Stop { lane_id } => format!("stop:{lane_id}"),
            LaneEffectRequest::SendInput { lane_id, .. } => {
                self.calls.lock().unwrap().push(format!("input:{lane_id}"));
                return Err(format!("lane `{lane_id}` input channel closed"));
            }
            LaneEffectRequest::Apply { .. } => "apply".to_string(),
            LaneEffectRequest::Cleanup { lane, .. } => format!("cleanup:{}", lane.id),
        };
        self.calls.lock().unwrap().push(call);
        Ok(LaneEffectResult::success("effect completed"))
    }

    fn compensate_create(
        &self,
        _repo: &std::path::Path,
        lane: &AgentLaneRecord,
    ) -> Result<(), String> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("compensate_create:{}", lane.id));
        Ok(())
    }
}

struct InjectableLanePersistence {
    store: WorkflowStore,
    append_count: AtomicUsize,
    fail_at: Vec<usize>,
    fail_load: bool,
}

impl InjectableLanePersistence {
    fn new(store: WorkflowStore, fail_at: impl IntoIterator<Item = usize>) -> Self {
        Self {
            store,
            append_count: AtomicUsize::new(0),
            fail_at: fail_at.into_iter().collect(),
            fail_load: false,
        }
    }

    fn with_load_failure(store: WorkflowStore) -> Self {
        Self {
            store,
            append_count: AtomicUsize::new(0),
            fail_at: Vec::new(),
            fail_load: true,
        }
    }
}

impl LanePersistence for InjectableLanePersistence {
    fn append(&self, event: &LaneEvent) -> Result<(), String> {
        let append = self.append_count.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_at.contains(&append) {
            Err(format!("injected lane append failure at {append}"))
        } else {
            self.store.append_lane_event_checked(event)
        }
    }

    fn load_lanes(&self) -> Result<std::collections::BTreeMap<String, AgentLaneRecord>, String> {
        if self.fail_load {
            return Err("injected lane hydration failure".to_string());
        }
        self.store
            .load_lane_state()
            .map(|state| state.lanes().clone())
    }
}

impl LaneEffectExecutor for CountingLaneEffects {
    fn execute(&self, _request: LaneEffectRequest) -> Result<LaneEffectResult, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(LaneEffectResult::success("effect completed"))
    }
}

fn collect_envelopes_until(
    supervisor: &RuntimeSupervisor,
    done: impl Fn(&[RuntimeEventEnvelope]) -> bool,
) -> Vec<RuntimeEventEnvelope> {
    let started = Instant::now();
    let mut events = Vec::new();
    while started.elapsed() < Duration::from_secs(5) {
        if let Some(event) = supervisor.recv_event_envelope_timeout(Duration::from_millis(50)) {
            events.push(event);
            if done(&events) {
                return events;
            }
        }
    }
    panic!("timed out waiting for lane envelopes: {events:#?}");
}

fn wait_until(timeout: Duration, condition: &str, done: impl Fn() -> bool) {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out after {timeout:?} waiting for {condition}");
}

fn owner(lane_id: &str) -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace-test".to_string(),
        project_id: "project-test".to_string(),
        lane_id: Some(lane_id.to_string()),
        session_id: Some(format!("session-{lane_id}")),
        task_id: None,
        turn_id: Some(format!("turn-{lane_id}")),
    }
}

fn lane(id: &str) -> AgentLaneRecord {
    AgentLaneRecord {
        id: id.to_string(),
        task_id: None,
        role: AgentRole::Coder,
        route: AgentRoute::Terminal,
        gate_strength: GateStrength::Containment,
        mutation_policy: MutationPolicy::ProposeOnly,
        worktree: Some(format!(".worktrees/{id}")),
        branch: Some(format!("codex/{id}")),
        target: ExecutionTarget::Local,
        data_egress: DataEgressPolicy::Deny,
        status: LaneStatus::Draft,
        budget: LaneBudget::default(),
        active_session_ids: Vec::new(),
        summary: "ready".to_string(),
        evidence: Vec::new(),
        run_stats: None,
    }
}

fn autonomous_lane(id: &str) -> AgentLaneRecord {
    let mut lane = lane(id);
    lane.mutation_policy = MutationPolicy::Autonomous;
    lane
}

fn snapshot() -> RuntimeSnapshot {
    RuntimeSnapshot {
        cwd: std::env::temp_dir(),
        provider_family: "test".to_string(),
        model_label: "test".to_string(),
        work_mode: WorkMode::Build,
        permission_mode: viden_types::PermissionMode::Default,
        permission_level: viden_types::PermissionLevel::Ask,
        config_summary: String::new(),
        loaded_config_files: Vec::new(),
        startup_overrides: Vec::new(),
        ui_preferences: viden_types::ResolvedUiPreferences::default(),
    }
}

fn persist_done_propose_lane(home: &std::path::Path, cwd: &std::path::Path, lane_id: &str) {
    let mut record = lane(lane_id);
    record.status = LaneStatus::Done;
    persist_lane(home, cwd, record);
}

fn persist_lane(home: &std::path::Path, cwd: &std::path::Path, mut record: AgentLaneRecord) {
    let store = WorkflowStore::new(home.to_path_buf(), cwd).unwrap();
    let lane_id = record.id.clone();
    record.active_session_ids.push(format!("session-{lane_id}"));
    store
        .append_lane_event_checked(&LaneEvent::created(
            format!("persist-{lane_id}"),
            record,
            viden_types::now_timestamp(),
            Some(format!("session-{lane_id}")),
        ))
        .unwrap();
}

fn approve_lane_tool(
    supervisor: &RuntimeSupervisor,
    owner: &RuntimeOwner,
    tool_name: &str,
    command_id: &str,
) {
    let requested = collect_envelopes_until(supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalRequested { approval },
                    ..
                }) if approval.tool_name == tool_name
            )
        })
    });
    let request_id = requested
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) if approval.tool_name == tool_name => Some(approval.id.clone()),
            _ => None,
        })
        .expect("lane approval request");
    supervisor
        .send_command_from_owner(
            owner.clone(),
            command_id,
            RuntimeCommand::RespondToApproval {
                request_id: request_id.clone(),
                response: viden_types::ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    collect_envelopes_until(supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::ApprovalResolved { request_id: resolved, .. },
                    ..
                }) if resolved == &request_id
            )
        })
    });
}

fn assert_lane_approval_scope_isolated(repo_scope: bool) {
    let suffix = if repo_scope { "repo" } else { "session" };
    let cwd = temp_dir(&format!("lane_scope_isolation_{suffix}_cwd"));
    let home = temp_dir(&format!("lane_scope_isolation_{suffix}_home"));
    for lane_id in ["lane-scope-a", "lane-scope-b"] {
        let mut record = autonomous_lane(lane_id);
        record.status = LaneStatus::Running;
        record.worktree = Some("shared-lane-root".to_string());
        persist_lane(&home, &cwd, record);
    }
    std::fs::create_dir_all(cwd.join("shared-lane-root")).unwrap();
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let effects = Arc::new(RecordingLaneEffects::default());
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        effects.clone() as Arc<dyn LaneEffectExecutor>,
    );
    let owner_a = owner("lane-scope-a");
    supervisor
        .send_command_from_owner(
            owner_a.clone(),
            format!("scope_{suffix}_input_a"),
            RuntimeCommand::SendLaneInput {
                lane_id: "lane-scope-a".to_string(),
                input: "same".to_string(),
            },
        )
        .unwrap();
    let requested = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalRequested { approval }, .. }) if approval.tool_name == "lane_send_input"))
    });
    let approval = requested
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::ApprovalRequested { approval },
                ..
            }) => Some(approval.clone()),
            _ => None,
        })
        .unwrap();
    let scope = if repo_scope {
        approval
            .allowed_scopes
            .iter()
            .find_map(|scope| match scope {
                viden_types::ApprovalScope::RepoAllowlist { paths } => {
                    Some(viden_types::ApprovalScope::RepoAllowlist {
                        paths: paths.clone(),
                    })
                }
                _ => None,
            })
            .unwrap()
    } else {
        viden_types::ApprovalScope::Session {
            session_id: owner_a.session_id.clone().unwrap(),
        }
    };
    supervisor
        .send_command_from_owner(
            owner_a,
            format!("approve_{suffix}_input_a"),
            RuntimeCommand::RespondToApproval {
                request_id: approval.id,
                response: viden_types::ApprovalResponse {
                    decision: viden_types::ApprovalDecision::Allow { scope },
                    feedback: None,
                },
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::Error { .. },
                    ..
                })
            )
        })
    });
    supervisor
        .send_command_from_owner(
            owner("lane-scope-b"),
            format!("scope_{suffix}_input_b"),
            RuntimeCommand::SendLaneInput {
                lane_id: "lane-scope-b".to_string(),
                input: "same".to_string(),
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| matches!(&envelope.event, RuntimeWireEvent::Known(RuntimeEvent { kind: RuntimeEventKind::ApprovalRequested { approval }, .. }) if approval.owner.lane_id.as_deref() == Some("lane-scope-b")))
    });
    assert_eq!(
        effects
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| call.starts_with("input:"))
            .count(),
        1
    );
}

/// Reports a fixed stop outcome so the exit-code thread can be observed end to
/// end without depending on a real child process.
#[derive(Default)]
struct ExitCodeLaneEffects {
    exit_code: Option<i32>,
}

impl LaneEffectExecutor for ExitCodeLaneEffects {
    fn execute(&self, request: LaneEffectRequest) -> Result<LaneEffectResult, String> {
        Ok(match request {
            LaneEffectRequest::Stop { .. } => {
                LaneEffectResult::stopped("lane runtime stopped", self.exit_code)
            }
            _ => LaneEffectResult::success("effect completed"),
        })
    }
}

fn persist_autonomous_lane_with_status(
    home: &std::path::Path,
    cwd: &std::path::Path,
    lane_id: &str,
    status: LaneStatus,
) {
    let mut record = autonomous_lane(lane_id);
    record.status = status;
    persist_lane(home, cwd, record);
}

#[test]
fn lane_start_and_stop_append_bounded_run_observations_with_the_exit_code() {
    let cwd = temp_dir("lane_run_stats_start_stop_cwd");
    let home = temp_dir("lane_run_stats_start_stop_home");
    persist_autonomous_lane_with_status(&home, &cwd, "lane-run-stats", LaneStatus::Draft);
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        Arc::new(ExitCodeLaneEffects {
            exit_code: Some(23),
        }) as Arc<dyn LaneEffectExecutor>,
    );
    let lane_owner = owner("lane-run-stats");

    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "start_run_stats_lane",
            RuntimeCommand::StartLane {
                lane_id: "lane-run-stats".to_string(),
                command: "worker".to_string(),
                args: Vec::new(),
                env: Vec::new(),
                output_log: None,
            },
        )
        .unwrap();
    // The observation append republishes the lane, so the run-stats update
    // arrives after the Running status change rather than with it.
    let started = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneUpdated { lane },
                    ..
                }) if lane.id == "lane-run-stats"
                    && lane.status == LaneStatus::Running
                    && lane.run_stats.is_some_and(|stats| stats.run_count == 1)
            )
        })
    });
    assert!(started.iter().any(|envelope| {
        matches!(
            &envelope.event,
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::LaneUpdated { lane },
                ..
            }) if lane.id == "lane-run-stats" && lane.run_stats.is_none()
        )
    }));

    supervisor
        .send_command_from_owner(
            lane_owner,
            "stop_run_stats_lane",
            RuntimeCommand::StopLane {
                lane_id: "lane-run-stats".to_string(),
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneUpdated { lane },
                    ..
                }) if lane.id == "lane-run-stats"
                    && lane.run_stats.is_some_and(|stats| stats.last_exit_code == Some(23))
            )
        })
    });

    let store = WorkflowStore::new(home, &cwd).unwrap();
    let observations = store
        .load_lane_events()
        .unwrap()
        .into_iter()
        .filter_map(|event| match event.kind {
            viden_workflows::lanes::LaneEventKind::RunObserved { observation } => Some(observation),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        observations,
        vec![
            viden_workflows::lanes::LaneRunObservation::Started,
            viden_workflows::lanes::LaneRunObservation::Stopped {
                exit_code: Some(23)
            },
        ]
    );
    let stats = store
        .load_lane_state()
        .unwrap()
        .lane("lane-run-stats")
        .unwrap()
        .run_stats
        .unwrap();
    assert_eq!(stats.run_count, 1);
    assert_eq!(stats.last_exit_code, Some(23));
    assert_eq!(stats.diff_bytes, 0);
}

#[test]
fn lane_apply_appends_the_applied_diff_byte_count() {
    const DIFF: &str = "diff --git a/a b/a\n--- a/a\n+++ b/a\n";
    let cwd = temp_dir("lane_run_stats_apply_cwd");
    let home = temp_dir("lane_run_stats_apply_home");
    persist_autonomous_lane_with_status(&home, &cwd, "lane-run-stats-apply", LaneStatus::Done);
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        Arc::new(ExitCodeLaneEffects::default()) as Arc<dyn LaneEffectExecutor>,
    );

    supervisor
        .send_command_from_owner(
            owner("lane-run-stats-apply"),
            "apply_run_stats_lane",
            RuntimeCommand::ApplyLaneChanges {
                lane_id: "lane-run-stats-apply".to_string(),
                unified_diff: DIFF.to_string(),
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneUpdated { lane },
                    ..
                }) if lane.id == "lane-run-stats-apply"
                    && lane.run_stats.is_some_and(|stats| stats.diff_bytes == DIFF.len() as u64)
            )
        })
    });

    let store = WorkflowStore::new(home, &cwd).unwrap();
    let stats = store
        .load_lane_state()
        .unwrap()
        .lane("lane-run-stats-apply")
        .unwrap()
        .run_stats
        .unwrap();
    assert_eq!(stats.diff_bytes, DIFF.len() as u64);
    assert_eq!(stats.run_count, 0);
    assert_eq!(stats.wall_time_ms, 0);
}

/// Drives a lane to Running, then tears it down with `teardown`, and returns the
/// run observations the durable lanes log recorded.
///
/// The one-second sleep is deliberate: `LaneEvent::timestamp` has second
/// resolution, so a teardown in the same wall-clock second as the start would
/// legitimately accumulate zero wall time and prove nothing about closure.
fn observations_after_teardown_while_running(
    label: &str,
    lane_id: &str,
    exit_code: Option<i32>,
    teardown: impl FnOnce(&RuntimeSupervisor, RuntimeOwner),
) -> (
    Vec<viden_workflows::lanes::LaneRunObservation>,
    viden_types::LaneRunStats,
) {
    let cwd = temp_dir(&format!("{label}_cwd"));
    let home = temp_dir(&format!("{label}_home"));
    persist_autonomous_lane_with_status(&home, &cwd, lane_id, LaneStatus::Draft);
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        Arc::new(ExitCodeLaneEffects { exit_code }) as Arc<dyn LaneEffectExecutor>,
    );
    let lane_owner = owner(lane_id);

    supervisor
        .send_command_from_owner(
            lane_owner.clone(),
            "start_before_teardown",
            RuntimeCommand::StartLane {
                lane_id: lane_id.to_string(),
                command: "worker".to_string(),
                args: Vec::new(),
                env: Vec::new(),
                output_log: None,
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneUpdated { lane },
                    ..
                }) if lane.id == lane_id
                    && lane.run_stats.is_some_and(|stats| stats.run_count == 1)
            )
        })
    });

    std::thread::sleep(Duration::from_millis(1_100));
    teardown(&supervisor, lane_owner);

    let store = WorkflowStore::new(home, &cwd).unwrap();
    wait_until(
        Duration::from_secs(5),
        "the teardown to close the open run",
        || {
            store.load_lane_events().is_ok_and(|events| {
                events.iter().any(|event| {
                    matches!(
                        event.kind,
                        viden_workflows::lanes::LaneEventKind::RunObserved {
                            observation: viden_workflows::lanes::LaneRunObservation::Stopped { .. }
                        }
                    )
                })
            })
        },
    );
    let observations = store
        .load_lane_events()
        .unwrap()
        .into_iter()
        .filter_map(|event| match event.kind {
            viden_workflows::lanes::LaneEventKind::RunObserved { observation } => Some(observation),
            _ => None,
        })
        .collect::<Vec<_>>();
    let stats = store
        .load_lane_state()
        .unwrap()
        .lane(lane_id)
        .unwrap()
        .run_stats
        .unwrap();
    (observations, stats)
}

#[test]
fn archiving_a_running_lane_closes_the_run_with_wall_time_and_exit_code() {
    let (observations, stats) = observations_after_teardown_while_running(
        "lane_run_stats_archive",
        "lane-run-stats-archive",
        Some(11),
        |supervisor, lane_owner| {
            supervisor
                .send_command_from_owner(
                    lane_owner,
                    "archive_running_lane",
                    RuntimeCommand::ArchiveLane {
                        lane_id: "lane-run-stats-archive".to_string(),
                        summary: "archived while running".to_string(),
                    },
                )
                .unwrap();
        },
    );

    assert_eq!(
        observations,
        vec![
            viden_workflows::lanes::LaneRunObservation::Started,
            viden_workflows::lanes::LaneRunObservation::Stopped {
                exit_code: Some(11)
            },
        ]
    );
    assert_eq!(stats.run_count, 1);
    assert_eq!(stats.last_exit_code, Some(11));
    assert!(
        stats.wall_time_ms >= 1_000,
        "an archived run must keep its wall time, got {}",
        stats.wall_time_ms
    );
}

#[test]
fn cleaning_up_a_running_lane_closes_the_run_with_wall_time_and_exit_code() {
    let (observations, stats) = observations_after_teardown_while_running(
        "lane_run_stats_cleanup",
        "lane-run-stats-cleanup",
        Some(4),
        |supervisor, lane_owner| {
            supervisor
                .send_command_from_owner(
                    lane_owner,
                    "cleanup_running_lane",
                    RuntimeCommand::CleanupLane {
                        lane_id: "lane-run-stats-cleanup".to_string(),
                        force: false,
                    },
                )
                .unwrap();
        },
    );

    assert_eq!(
        observations,
        vec![
            viden_workflows::lanes::LaneRunObservation::Started,
            viden_workflows::lanes::LaneRunObservation::Stopped { exit_code: Some(4) },
        ]
    );
    assert_eq!(stats.last_exit_code, Some(4));
    assert!(stats.wall_time_ms >= 1_000, "{}", stats.wall_time_ms);
}

#[test]
fn cancelling_a_running_lane_closes_the_run_with_wall_time_and_exit_code() {
    let (observations, stats) = observations_after_teardown_while_running(
        "lane_run_stats_cancel",
        "lane-run-stats-cancel",
        None,
        |supervisor, lane_owner| {
            supervisor
                .send_command_from_owner(
                    lane_owner,
                    "cancel_running_lane",
                    RuntimeCommand::CancelActiveTurn,
                )
                .unwrap();
        },
    );

    // A cancelled runtime is force-killed, so the exit code is legitimately
    // absent; the wall time still was observed and must survive.
    assert_eq!(
        observations,
        vec![
            viden_workflows::lanes::LaneRunObservation::Started,
            viden_workflows::lanes::LaneRunObservation::Stopped { exit_code: None },
        ]
    );
    assert_eq!(stats.last_exit_code, None);
    assert!(stats.wall_time_ms >= 1_000, "{}", stats.wall_time_ms);
}

#[test]
fn archiving_a_lane_that_never_ran_records_no_observation() {
    let cwd = temp_dir("lane_run_stats_archive_idle_cwd");
    let home = temp_dir("lane_run_stats_archive_idle_home");
    persist_autonomous_lane_with_status(&home, &cwd, "lane-run-stats-idle", LaneStatus::Draft);
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    engine
        .set_permission_mode(viden_types::PermissionMode::DontAsk)
        .unwrap();
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        Arc::new(ExitCodeLaneEffects { exit_code: Some(7) }) as Arc<dyn LaneEffectExecutor>,
    );

    supervisor
        .send_command_from_owner(
            owner("lane-run-stats-idle"),
            "archive_idle_lane",
            RuntimeCommand::ArchiveLane {
                lane_id: "lane-run-stats-idle".to_string(),
                summary: "archived without ever running".to_string(),
            },
        )
        .unwrap();
    collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneUpdated { lane },
                    ..
                }) if lane.id == "lane-run-stats-idle" && lane.status == LaneStatus::Archived
            )
        })
    });

    // No runtime was ever active, so there is no run to close. The reducer would
    // tolerate an orphan stop, but Core must not manufacture one.
    let store = WorkflowStore::new(home, &cwd).unwrap();
    assert!(store.load_lane_events().unwrap().iter().all(|event| {
        !matches!(
            event.kind,
            viden_workflows::lanes::LaneEventKind::RunObserved { .. }
        )
    }));
    assert_eq!(
        store
            .load_lane_state()
            .unwrap()
            .lane("lane-run-stats-idle")
            .unwrap()
            .run_stats,
        None
    );
}

/// `runtime.conflict_content` on the Lane apply path: a refused lane apply
/// publishes the lines that collided, against the revision the `ours` side was
/// read from.
#[test]
fn a_lane_apply_conflict_publishes_the_lines_that_collided() {
    let cwd = temp_dir("lane_conflict_content_cwd");
    let home = temp_dir("lane_conflict_content_home");
    for args in [
        vec!["init"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "Viden Test"],
    ] {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(&cwd)
                .status()
                .unwrap()
                .success()
        );
    }
    std::fs::write(cwd.join("a.txt"), "current\n").unwrap();
    for args in [vec!["add", "a.txt"], vec!["commit", "-m", "initial"]] {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(&cwd)
                .status()
                .unwrap()
                .success()
        );
    }
    let head = String::from_utf8(
        std::process::Command::new("git")
            .args(["rev-parse", "--verify", "HEAD^{commit}"])
            .current_dir(&cwd)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    let mut record = autonomous_lane("lane-conflict-content");
    record.status = LaneStatus::Done;
    persist_lane(&home, &cwd, record);
    let provider: Box<dyn ModelProvider> = Box::new(SequenceProvider::new(vec![]));
    let mut engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    engine.set_permission_mode(PermissionMode::DontAsk).unwrap();
    let supervisor = RuntimeSupervisor::start_with_lane_effects_for_test(
        engine,
        Arc::new(LocalLaneEffectExecutor::default()) as Arc<dyn LaneEffectExecutor>,
    );
    supervisor
        .send_command_from_owner(
            owner("lane-conflict-content"),
            "apply_lane_conflict_content",
            RuntimeCommand::ApplyLaneChanges {
                lane_id: "lane-conflict-content".to_string(),
                unified_diff: "--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n".to_string(),
            },
        )
        .unwrap();

    let events = collect_envelopes_until(&supervisor, |events| {
        events.iter().any(|envelope| {
            matches!(
                &envelope.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::LaneConflictDetected { .. },
                    ..
                })
            )
        })
    });
    let content = events
        .iter()
        .find_map(|envelope| match &envelope.event {
            RuntimeWireEvent::Known(RuntimeEvent {
                kind: RuntimeEventKind::LaneConflictDetected { content, .. },
                ..
            }) => content.clone(),
            _ => None,
        })
        .expect("a refused lane apply must publish its content");

    assert_eq!(
        content.baseline,
        viden_types::ConflictBaseline::Revision { sha: head },
        "a Lane holds no reviewed baseline, so the revision the file was read from is the answer"
    );
    assert!(!content.truncated);
    assert_eq!(content.files.len(), 1);
    assert_eq!(content.files[0].path, "a.txt");
    let hunk = &content.files[0].hunks[0];
    assert_eq!(
        hunk.reason,
        viden_types::ConflictHunkReason::ContextMismatch
    );
    assert_eq!(hunk.ours, vec!["current\n"]);
    assert_eq!(hunk.theirs, vec!["new\n"]);
    assert_eq!(hunk.base.as_deref(), Some(["old\n".to_string()].as_slice()));
}
