use super::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

fn runtime_snapshot_json() -> serde_json::Value {
    serde_json::json!({
        "cwd": "/tmp/viden",
        "provider_family": "deepseek",
        "model_label": "deepseek-v4-flash",
        "work_mode": "build",
        "permission_mode": "default",
        "permission_level": "ask",
        "config_summary": "provider=deepseek model=deepseek-v4-flash",
        "loaded_config_files": [],
        "startup_overrides": []
    })
}

#[test]
fn runtime_snapshot_without_ui_preferences_uses_safe_resolved_default() {
    let snapshot: RuntimeSnapshot = serde_json::from_value(runtime_snapshot_json()).unwrap();

    assert_eq!(
        snapshot.ui_preferences,
        ResolvedUiPreferences {
            locale: LocaleId::En,
            skin: UiSkin::Aurora,
            mode: UiColorMode::Dark,
            density: UiDensity::Regular,
            motion: UiMotion::System,
            diagnostics: Vec::new(),
        }
    );
}

#[test]
fn runtime_snapshot_serializes_exact_resolved_ui_preferences() {
    let mut encoded = runtime_snapshot_json();
    encoded["ui_preferences"] = serde_json::json!({
        "locale": "zh-CN",
        "skin": "aurora",
        "mode": "dark",
        "density": "regular",
        "motion": "reduced",
        "diagnostics": []
    });

    let snapshot: RuntimeSnapshot = serde_json::from_value(encoded).unwrap();
    let serialized = serde_json::to_value(snapshot).unwrap();

    assert_eq!(
        serialized["ui_preferences"],
        serde_json::json!({
            "locale": "zh-CN",
            "skin": "aurora",
            "mode": "dark",
            "density": "regular",
            "motion": "reduced",
            "diagnostics": []
        })
    );
}

#[test]
fn ui_preferences_serde_names_are_stable() {
    let cases = [
        (serde_json::to_value(LocaleId::System).unwrap(), "system"),
        (serde_json::to_value(LocaleId::En).unwrap(), "en"),
        (serde_json::to_value(LocaleId::ZhCn).unwrap(), "zh-CN"),
        (serde_json::to_value(UiSkin::Aurora).unwrap(), "aurora"),
        (serde_json::to_value(UiSkin::Ice).unwrap(), "ice"),
        (serde_json::to_value(UiSkin::Mono).unwrap(), "mono"),
        (serde_json::to_value(UiSkin::Amber).unwrap(), "amber"),
        (serde_json::to_value(UiSkin::Phosphor).unwrap(), "phosphor"),
        (serde_json::to_value(UiColorMode::System).unwrap(), "system"),
        (serde_json::to_value(UiColorMode::Dark).unwrap(), "dark"),
        (serde_json::to_value(UiColorMode::Light).unwrap(), "light"),
        (serde_json::to_value(UiDensity::Compact).unwrap(), "compact"),
        (serde_json::to_value(UiDensity::Regular).unwrap(), "regular"),
        (serde_json::to_value(UiDensity::Comfy).unwrap(), "comfy"),
        (serde_json::to_value(UiMotion::System).unwrap(), "system"),
        (serde_json::to_value(UiMotion::Reduced).unwrap(), "reduced"),
        (serde_json::to_value(UiMotion::Full).unwrap(), "full"),
        (
            serde_json::to_value(TuiColorDepth::Truecolor).unwrap(),
            "truecolor",
        ),
        (
            serde_json::to_value(TuiColorDepth::Ansi256).unwrap(),
            "ansi256",
        ),
        (
            serde_json::to_value(TuiColorDepth::Ansi16).unwrap(),
            "ansi16",
        ),
    ];

    for (encoded, expected) in cases {
        assert_eq!(encoded, serde_json::Value::String(expected.to_string()));
    }
}

#[test]
fn ui_preferences_valid_skin_mode_pairs_are_exactly_eight() {
    let pairs: Vec<_> = UiSkin::ALL
        .iter()
        .copied()
        .flat_map(|skin| {
            [UiColorMode::Dark, UiColorMode::Light]
                .into_iter()
                .map(move |mode| (skin, mode))
        })
        .filter(|(skin, mode)| UiPreferences::is_valid_effective_pair(*skin, *mode))
        .collect();

    assert_eq!(pairs.len(), 8);
    assert!(pairs.contains(&(UiSkin::Aurora, UiColorMode::Dark)));
    assert!(pairs.contains(&(UiSkin::Aurora, UiColorMode::Light)));
    assert!(pairs.contains(&(UiSkin::Ice, UiColorMode::Dark)));
    assert!(pairs.contains(&(UiSkin::Ice, UiColorMode::Light)));
    assert!(pairs.contains(&(UiSkin::Mono, UiColorMode::Dark)));
    assert!(pairs.contains(&(UiSkin::Mono, UiColorMode::Light)));
    assert!(pairs.contains(&(UiSkin::Amber, UiColorMode::Dark)));
    assert!(pairs.contains(&(UiSkin::Phosphor, UiColorMode::Dark)));
    assert!(!pairs.contains(&(UiSkin::Amber, UiColorMode::Light)));
    assert!(!pairs.contains(&(UiSkin::Phosphor, UiColorMode::Light)));
}

#[test]
fn ui_preferences_patch_serializes_as_schema_one_safe_values() {
    let patch = UiPreferencePatch {
        locale: Some(LocaleId::ZhCn),
        skin: Some(UiSkin::Ice),
        mode: Some(UiColorMode::Light),
        density: Some(UiDensity::Compact),
        motion: Some(UiMotion::Reduced),
    };

    assert_eq!(
        serde_json::to_value(patch).unwrap(),
        serde_json::json!({
            "locale": "zh-CN",
            "skin": "ice",
            "mode": "light",
            "density": "compact",
            "motion": "reduced"
        })
    );
    assert_eq!(
        serde_json::from_value::<UiPreferencePatch>(serde_json::json!({})).unwrap(),
        UiPreferencePatch::default()
    );
}

#[test]
fn ui_preferences_runtime_protocol_is_backward_compatible_schema_one_extension() {
    let command = RuntimeCommand::SetUiPreferences {
        patch: UiPreferencePatch {
            skin: Some(UiSkin::Mono),
            mode: Some(UiColorMode::Light),
            ..UiPreferencePatch::default()
        },
    };
    let encoded_command = serde_json::to_value(&command).unwrap();
    assert_eq!(encoded_command["type"], "set_ui_preferences");
    assert_eq!(encoded_command["patch"]["skin"], "mono");
    assert!(!encoded_command.to_string().contains("api_key"));

    let resolved = ResolvedUiPreferences {
        locale: LocaleId::En,
        skin: UiSkin::Mono,
        mode: UiColorMode::Light,
        density: UiDensity::Regular,
        motion: UiMotion::Reduced,
        diagnostics: Vec::new(),
    };
    let event = RuntimeEventKind::UiPreferencesUpdated {
        resolved: resolved.clone(),
        persisted: Some(UiPreferences {
            locale: LocaleId::En,
            skin: UiSkin::Mono,
            mode: UiColorMode::Light,
            density: UiDensity::Regular,
            motion: UiMotion::Reduced,
        }),
        diagnostics: Vec::new(),
    };
    let encoded_event = serde_json::to_value(&event).unwrap();
    assert_eq!(encoded_event["type"], "ui_preferences_updated");
    assert_eq!(
        serde_json::from_value::<RuntimeEventKind>(encoded_event).unwrap(),
        event
    );

    let snapshot = runtime_snapshot_for_contract();
    let live_view = RuntimeViewState::new(snapshot);
    assert_eq!(live_view.ui_preferences, live_view.snapshot.ui_preferences);
    let legacy_view = serde_json::to_value(live_view).unwrap();
    assert!(legacy_view.get("ui_preferences").is_none());
    let decoded: RuntimeViewState = serde_json::from_value(legacy_view).unwrap();
    assert_eq!(decoded.ui_preferences, ResolvedUiPreferences::default());
}

#[test]
fn recent_work_runtime_protocol_round_trips_safe_schema_one_payloads() {
    let command = RuntimeCommand::QueryRecentWork {
        query: RecentWorkQuery { limit: 501 },
    };
    let encoded_command = serde_json::to_value(&command).unwrap();
    assert_eq!(encoded_command["type"], "query_recent_work");
    assert_eq!(encoded_command["query"]["limit"], 501);
    assert_eq!(
        serde_json::from_value::<RuntimeCommand>(encoded_command).unwrap(),
        command
    );

    let sessions = vec![RecentSessionSummary {
        canonical_root: "/workspace/a".to_string(),
        session_id: "session-a".to_string(),
        created_at: 10,
        last_updated_at: 20,
        message_count: 2,
        tool_call_count: 1,
        command_count: 1,
    }];
    let projects = vec![RecentProjectSummary {
        canonical_root: "/workspace/a".to_string(),
        display_name: "a".to_string(),
        last_updated_at: 20,
        latest_session_id: Some("session-a".to_string()),
    }];
    let event = RuntimeEventKind::RecentWorkLoaded {
        projects: projects.clone(),
        sessions: sessions.clone(),
        diagnostics: vec!["recent.index_stale".to_string()],
    };
    let encoded_event = serde_json::to_value(&event).unwrap();
    assert_eq!(encoded_event["type"], "recent_work_loaded");
    assert_eq!(
        serde_json::from_value::<RuntimeEventKind>(encoded_event).unwrap(),
        event
    );

    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());
    view.apply_event(&RuntimeEvent::with_timestamp(1, Some(30), event));
    assert_eq!(view.recent_projects, projects);
    assert_eq!(view.recent_sessions, sessions);
    assert_eq!(view.recent_work_diagnostics, vec!["recent.index_stale"]);
}

#[test]
fn recent_work_serialized_event_and_view_exclude_private_session_fields() {
    let event = RuntimeEvent::with_timestamp(
        1,
        Some(30),
        RuntimeEventKind::RecentWorkLoaded {
            projects: vec![RecentProjectSummary {
                canonical_root: "/workspace/public".to_string(),
                display_name: "public".to_string(),
                last_updated_at: 20,
                latest_session_id: Some("session-public".to_string()),
            }],
            sessions: vec![RecentSessionSummary {
                canonical_root: "/workspace/public".to_string(),
                session_id: "session-public".to_string(),
                created_at: 10,
                last_updated_at: 20,
                message_count: 1,
                tool_call_count: 0,
                command_count: 0,
            }],
            diagnostics: Vec::new(),
        },
    );
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());
    view.apply_event(&event);
    let serialized = format!(
        "{}{}",
        serde_json::to_string(&event).unwrap(),
        serde_json::to_string(&view).unwrap()
    );

    for forbidden in [
        "transcript_path",
        "last_preview",
        "last_activity_preview",
        "credential_request_id",
        "backend_id",
        "sk-secret-message-body",
        "command output",
    ] {
        assert!(!serialized.contains(forbidden), "leaked {forbidden}");
    }
}

#[test]
fn ui_preferences_resolve_system_locale_density_and_reduced_motion() {
    let resolved = resolve_ui_preferences(
        None,
        None,
        Some(UiPreferences {
            locale: LocaleId::System,
            skin: UiSkin::Ice,
            mode: UiColorMode::System,
            density: UiDensity::Comfy,
            motion: UiMotion::Reduced,
        }),
        UiPreferences {
            locale: LocaleId::ZhCn,
            skin: UiSkin::Aurora,
            mode: UiColorMode::Light,
            density: UiDensity::Regular,
            motion: UiMotion::System,
        },
    );

    assert_eq!(resolved.locale, LocaleId::ZhCn);
    assert_eq!(resolved.skin, UiSkin::Ice);
    assert_eq!(resolved.mode, UiColorMode::Light);
    assert_eq!(resolved.density, UiDensity::Comfy);
    assert_eq!(resolved.motion, UiMotion::Reduced);
    assert!(resolved.diagnostics.is_empty());
}

#[test]
fn ui_preferences_invalid_dark_only_light_pair_falls_back_once() {
    let resolved = resolve_ui_preferences(
        Some(UiPreferences {
            locale: LocaleId::ZhCn,
            skin: UiSkin::Amber,
            mode: UiColorMode::Light,
            density: UiDensity::Compact,
            motion: UiMotion::Reduced,
        }),
        None,
        None,
        UiPreferences::client_default(),
    );

    assert_eq!(resolved.locale, LocaleId::ZhCn);
    assert_eq!(resolved.skin, UiSkin::Aurora);
    assert_eq!(resolved.mode, UiColorMode::Dark);
    assert_eq!(resolved.density, UiDensity::Regular);
    assert_eq!(resolved.motion, UiMotion::Reduced);
    assert_eq!(resolved.diagnostics.len(), 1);
    assert_eq!(resolved.diagnostics[0].code, "ui.invalid_skin_mode_pair");
    assert_eq!(resolved.diagnostics[0].field.as_deref(), Some("ui.mode"));
}

#[test]
fn ui_preferences_project_cannot_override_user_profile() {
    let resolved = resolve_ui_preferences(
        None,
        Some(UiPreferences {
            locale: LocaleId::En,
            skin: UiSkin::Mono,
            mode: UiColorMode::Dark,
            density: UiDensity::Compact,
            motion: UiMotion::Full,
        }),
        Some(UiPreferences {
            locale: LocaleId::ZhCn,
            skin: UiSkin::Ice,
            mode: UiColorMode::Light,
            density: UiDensity::Comfy,
            motion: UiMotion::Reduced,
        }),
        UiPreferences::client_default(),
    );

    assert_eq!(resolved.locale, LocaleId::En);
    assert_eq!(resolved.skin, UiSkin::Mono);
    assert_eq!(resolved.mode, UiColorMode::Dark);
    assert_eq!(resolved.density, UiDensity::Compact);
    assert_eq!(resolved.motion, UiMotion::Full);
}

#[test]
fn ui_preferences_chinese_locale_identifiers_map_to_builtin_zh_cn() {
    for raw in [
        "zh",
        "zh_CN",
        "zh-CN",
        "zh_CN.UTF-8",
        "zh-CN.UTF-8",
        "zh_TW",
        "zh-HK",
        "zh_Hant_TW.UTF-8",
        "zh.Hans",
    ] {
        assert_eq!(LocaleId::from_system_locale(raw), LocaleId::ZhCn, "{raw}");
    }
}

#[test]
fn ui_preferences_locale_detection_does_not_match_arbitrary_words() {
    for raw in ["zhuang", "zhfake", "english_zh", "en_US.UTF-8", ""] {
        assert_ne!(LocaleId::from_system_locale(raw), LocaleId::ZhCn, "{raw}");
    }
}

#[test]
fn work_modes_and_permission_levels_parse_cli_names() {
    assert_eq!(WorkMode::parse_cli("plan"), Some(WorkMode::Plan));
    assert_eq!(WorkMode::parse_cli("build"), Some(WorkMode::Build));
    assert_eq!(WorkMode::Plan.cli_name(), "plan");
    assert_eq!(WorkMode::default(), WorkMode::Build);

    assert_eq!(
        PermissionLevel::parse_cli("ask"),
        Some(PermissionLevel::Ask)
    );
    assert_eq!(
        PermissionLevel::parse_cli("auto_edit"),
        Some(PermissionLevel::AutoEdit)
    );
    assert_eq!(
        PermissionLevel::parse_cli("read-only"),
        Some(PermissionLevel::ReadOnly)
    );
    assert_eq!(
        PermissionLevel::from_legacy_mode(PermissionMode::Plan),
        PermissionLevel::ReadOnly
    );
    assert_eq!(
        PermissionLevel::from_legacy_mode(PermissionMode::AcceptEdits),
        PermissionLevel::AutoEdit
    );
}

#[test]
fn workflow_enums_roundtrip_through_cli_names_and_json() {
    assert_eq!(
        TaskStatus::parse_cli("in_progress"),
        Some(TaskStatus::InProgress)
    );
    assert_eq!(TaskStatus::Blocked.cli_name(), "blocked");
    assert_eq!(
        TaskPriority::parse_cli("critical"),
        Some(TaskPriority::Critical)
    );
    assert_eq!(
        MemoryScope::parse_cli("session"),
        Some(MemoryScope::Session)
    );
    assert_eq!(MemoryKind::Decision.cli_name(), "decision");
    assert_eq!(
        MemorySource::parse_cli("assistant_suggestion"),
        Some(MemorySource::AssistantSuggestion)
    );
    assert_eq!(MemoryStatus::Active.cli_name(), "active");

    let encoded = serde_json::to_string(&TaskStatus::InProgress).unwrap();
    assert_eq!(encoded, "\"in_progress\"");
    let decoded: TaskStatus = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, TaskStatus::InProgress);
}

#[test]
fn workflow_records_are_serializable() {
    let task = TaskRecord {
        task_id: "task_1".to_string(),
        title: "Design workflow state".to_string(),
        description: Some("Capture durable task state".to_string()),
        status: TaskStatus::Todo,
        priority: TaskPriority::High,
        labels: vec!["v2".to_string(), "workflow".to_string()],
        assignee_hint: Some("agent".to_string()),
        parent_task_id: None,
        dependency_ids: vec!["task_0".to_string()],
        blocked_by: Some("waiting on spec review".to_string()),
        notes: vec!["Use append-only logs".to_string()],
        created_at: 10,
        updated_at: 11,
        last_session_id: Some("session_1".to_string()),
        last_seen_at: Some(12),
        archived_at: None,
    };

    let memory = MemoryEntry {
        memory_id: "mem_1".to_string(),
        scope: MemoryScope::Project,
        session_id: None,
        kind: MemoryKind::Convention,
        content: "Use JSONL as canonical workflow storage".to_string(),
        source: MemorySource::User,
        status: MemoryStatus::Active,
        created_at: 20,
        updated_at: 21,
        related_task_ids: vec![task.task_id.clone()],
        confidence_hint: Some("high".to_string()),
    };

    let snapshot = ResumeContextSnapshot {
        active_tasks: vec![task],
        blocked_tasks: Vec::new(),
        recently_completed_tasks: Vec::new(),
        relevant_project_memory: vec![memory],
        recent_session_memory: Vec::new(),
        suggested_next_steps: vec!["Continue Task 1".to_string()],
        suggested_session_memory: vec!["Task 1 started".to_string()],
    };

    let encoded = serde_json::to_string(&snapshot).unwrap();
    let decoded: ResumeContextSnapshot = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded.suggested_next_steps, vec!["Continue Task 1"]);
    assert_eq!(decoded.active_tasks[0].priority, TaskPriority::High);
}

#[test]
fn agent_task_status_maps_operator_priority() {
    assert_eq!(
        AgentTaskStatus::parse("reviewing"),
        Some(AgentTaskStatus::Reviewing)
    );
    assert_eq!(
        AgentTaskStatus::parse("apply_conflict"),
        Some(AgentTaskStatus::Blocked)
    );
    assert!(AgentTaskStatus::WaitingApproval.is_active());
    assert!(AgentTaskStatus::WaitingApproval.priority() > AgentTaskStatus::RunningTool.priority());
    assert!(!AgentTaskStatus::Done.is_active());
}

#[test]
fn agent_task_and_context_records_roundtrip_json() {
    let task = AgentTaskRecord {
        id: "lane-1".to_string(),
        parent_id: None,
        role: AgentRole::Coder,
        kind: AgentTaskKind::Agent,
        route: AgentRoute::Terminal,
        title: "cargo test".to_string(),
        status: AgentTaskStatus::Running,
        activity: "running cargo test".to_string(),
        summary: "operator lane".to_string(),
        progress: 42,
        started_at: Some(1),
        updated_at: Some(2),
        workspace: Some("/tmp/work".to_string()),
        evidence: vec!["log lane-1.log".to_string()],
        permissions: vec!["shell approval".to_string()],
        decision: None,
        result: None,
        resume_handle: Some("tmux attach -t rc-lane-1".to_string()),
        pid: Some(1234),
        next_action: Some(AgentNextAction {
            label: "inspect".to_string(),
            command: Some("/lane inspect lane-1".to_string()),
            reason: Some("running lane".to_string()),
        }),
        owner: None,
    };
    assert!(task.is_active());
    assert_eq!(task.priority(), AgentTaskStatus::Running.priority());

    let bundle = ContextBundleRecord {
        bundle_id: "ctx-lane-1".to_string(),
        task_id: task.id.clone(),
        policy: "v1-priority-budget".to_string(),
        sources: vec![ContextSourceRecord {
            name: "latest-test".to_string(),
            kind: "test".to_string(),
            priority: 85,
            estimated_tokens: 200,
            summary: "tail compacted".to_string(),
            include_reason: "priority 85; selected by v1-priority-budget policy".to_string(),
            handle_id: Some("ctxh-test".to_string()),
            item_id: Some("ctxi-test".to_string()),
            view_id: Some("ctxv-test".to_string()),
            content_sha256: Some("ab".repeat(32)),
            view_sha256: Some("cd".repeat(32)),
            quality_id: Some("ctxq-test".to_string()),
        }],
        omitted_sources: vec![],
        estimated_tokens: 200,
        largest_sources: vec!["latest-test 200 tok".to_string()],
        compaction_notes: vec!["raw transcript preserved".to_string()],
        soft_token_budget: 800,
        hard_token_limit: 1000,
    };
    assert_eq!(bundle.pressure_percent(), 20);

    let encoded = serde_json::to_string(&(task, bundle)).unwrap();
    let (decoded_task, decoded_bundle): (AgentTaskRecord, ContextBundleRecord) =
        serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded_task.role, AgentRole::Coder);
    assert_eq!(decoded_bundle.sources[0].kind, "test");
    assert_eq!(
        decoded_bundle.sources[0].handle_id.as_deref(),
        Some("ctxh-test")
    );
    let legacy_json = serde_json::json!({
        "name": "legacy",
        "kind": "text",
        "priority": 1,
        "estimated_tokens": 2,
        "summary": "legacy summary",
        "include_reason": "legacy reason"
    });
    let legacy_source: ContextSourceRecord = serde_json::from_value(legacy_json).unwrap();
    assert!(legacy_source.handle_id.is_none());
    assert!(legacy_source.view_id.is_none());
}

#[test]
fn agent_lane_trust_loop_records_roundtrip_json() {
    let event = AgentLaneEventRecord {
        lane_id: "L1".to_string(),
        sequence: 1,
        timestamp: Some(10),
        kind: "lane.started".to_string(),
        summary: "started shell lane".to_string(),
        detail: Some("printf ok".to_string()),
        evidence_path: Some(".viden/lanes/L1.log".to_string()),
    };
    let isolation = AgentLaneIsolationRecord {
        lane_id: "L1".to_string(),
        workspace: "/tmp/project".to_string(),
        worktree: None,
        writable_scope: "current workspace".to_string(),
        env_vars: vec!["PATH".to_string()],
        cache_dirs: vec!["target/".to_string()],
        database_scope: None,
        service_ports: Vec::new(),
        setup_command: None,
        verification_command: Some("cargo test".to_string()),
        cleanup_command: None,
        risk_level: "medium".to_string(),
        warnings: vec!["shared workspace".to_string()],
    };
    let capability = AgentCapabilityRecord {
        id: "shell".to_string(),
        display_name: "Shell lane".to_string(),
        transport: "shell".to_string(),
        readiness: "ready".to_string(),
        entrypoint: "/lane run <command>".to_string(),
        mutation_mode: "permission-gated".to_string(),
        evidence_mode: "log+done+timeline".to_string(),
        config_source: None,
        known_limits: vec!["no process sandbox".to_string()],
    };

    let encoded = serde_json::to_string(&(event, isolation, capability)).unwrap();
    let (decoded_event, decoded_isolation, decoded_capability): (
        AgentLaneEventRecord,
        AgentLaneIsolationRecord,
        AgentCapabilityRecord,
    ) = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded_event.kind, "lane.started");
    assert_eq!(decoded_isolation.risk_level, "medium");
    assert_eq!(decoded_capability.evidence_mode, "log+done+timeline");
}

#[test]
fn typed_lane_records_use_the_frozen_v1_wire_names() {
    let lane = AgentLaneRecord {
        id: "lane_research".to_string(),
        task_id: Some("task_research".to_string()),
        role: AgentRole::Researcher,
        route: AgentRoute::Acp,
        gate_strength: GateStrength::Cooperative,
        mutation_policy: MutationPolicy::ReadOnly,
        worktree: Some(".worktrees/research".to_string()),
        branch: Some("codex/research".to_string()),
        target: ExecutionTarget::Ssh {
            host: "build.example.test".to_string(),
        },
        data_egress: DataEgressPolicy::AllowListed {
            domains: vec!["docs.example.test".to_string()],
        },
        status: LaneStatus::WaitingApproval,
        budget: LaneBudget {
            token_limit: Some(4_096),
            cost_limit_micro_usd: Some(250_000),
            wall_time_limit_secs: Some(300),
        },
        active_session_ids: vec!["session_research".to_string()],
        summary: "research is waiting for approval".to_string(),
        evidence: vec!["evidence_research".to_string()],
        run_stats: None,
    };

    let encoded = serde_json::to_value(&lane).unwrap();
    assert_eq!(encoded["role"], "researcher");
    assert_eq!(encoded["route"], "acp");
    assert_eq!(encoded["gate_strength"], "cooperative");
    assert_eq!(encoded["mutation_policy"], "read_only");
    assert_eq!(encoded["status"], "waiting_approval");
    assert_eq!(
        encoded["target"],
        serde_json::json!({"ssh": {"host": "build.example.test"}})
    );
    assert_eq!(
        encoded["data_egress"],
        serde_json::json!({"allow_listed": {"domains": ["docs.example.test"]}})
    );
    assert_eq!(
        serde_json::from_value::<AgentLaneRecord>(encoded).unwrap(),
        lane
    );
}

#[test]
fn typed_lane_enums_use_explicit_v1_json_names() {
    let roles = [
        (AgentRole::Planner, "planner"),
        (AgentRole::Coder, "coder"),
        (AgentRole::Reviewer, "reviewer"),
        (AgentRole::Tester, "tester"),
        (AgentRole::DocWriter, "doc_writer"),
        (AgentRole::Researcher, "researcher"),
        (AgentRole::ReleaseOperator, "release_operator"),
    ];
    for (value, wire) in roles {
        assert_eq!(serde_json::to_value(value).unwrap(), wire);
    }
    assert!(serde_json::from_str::<AgentRole>("\"external\"").is_err());

    let routes = [
        (AgentRoute::BuiltIn, "built_in"),
        (AgentRoute::Acp, "acp"),
        (AgentRoute::Terminal, "terminal"),
        (AgentRoute::Tmux, "tmux"),
    ];
    for (value, wire) in routes {
        assert_eq!(serde_json::to_value(value).unwrap(), wire);
    }

    let gates = [
        (GateStrength::Full, "full"),
        (GateStrength::Cooperative, "cooperative"),
        (GateStrength::Containment, "containment"),
    ];
    for (value, wire) in gates {
        assert_eq!(serde_json::to_value(value).unwrap(), wire);
    }

    let mutation_policies = [
        (MutationPolicy::Autonomous, "autonomous"),
        (MutationPolicy::ProposeOnly, "propose_only"),
        (MutationPolicy::ReadOnly, "read_only"),
    ];
    for (value, wire) in mutation_policies {
        assert_eq!(serde_json::to_value(value).unwrap(), wire);
    }

    let statuses = [
        (LaneStatus::Draft, "draft"),
        (LaneStatus::Queued, "queued"),
        (LaneStatus::Starting, "starting"),
        (LaneStatus::Running, "running"),
        (LaneStatus::WaitingApproval, "waiting_approval"),
        (LaneStatus::NeedsInput, "needs_input"),
        (LaneStatus::Blocked, "blocked"),
        (LaneStatus::Attached, "attached"),
        (LaneStatus::Detached, "detached"),
        (LaneStatus::Done, "done"),
        (LaneStatus::Failed, "failed"),
        (LaneStatus::Cancelled, "cancelled"),
        (LaneStatus::Archived, "archived"),
    ];
    for (value, wire) in statuses {
        assert_eq!(serde_json::to_value(value).unwrap(), wire);
    }

    let task_kinds = [
        (AgentTaskKind::Provider, "provider"),
        (AgentTaskKind::Tool, "tool"),
        (AgentTaskKind::Shell, "shell"),
        (AgentTaskKind::Test, "test"),
        (AgentTaskKind::Job, "job"),
        (AgentTaskKind::Agent, "agent"),
    ];
    for (value, wire) in task_kinds {
        assert_eq!(serde_json::to_value(value).unwrap(), wire);
    }
    assert_eq!(
        serde_json::to_value(ExecutionTarget::Local).unwrap(),
        "local"
    );
    assert_eq!(
        serde_json::to_value(DataEgressPolicy::AllowProvider).unwrap(),
        "allow_provider"
    );
}

#[test]
fn typed_lane_fixture_replays_as_the_frozen_v1_record() {
    let lanes: Vec<AgentLaneRecord> = serde_json::from_str(include_str!(
        "../tests/fixtures/frontend-contract-v1/typed-lanes.json"
    ))
    .unwrap();
    assert_eq!(lanes.len(), 4);
    assert_eq!(lanes[0].id, "L-start");
    assert_eq!(lanes[0].role, AgentRole::Coder);
    assert_eq!(lanes[0].route, AgentRoute::Terminal);
    assert_eq!(lanes[0].status, LaneStatus::Starting);
    assert_eq!(lanes[1].status, LaneStatus::Blocked);
    assert_eq!(lanes[2].route, AgentRoute::Tmux);
    assert_eq!(lanes[2].status, LaneStatus::Detached);
    assert_eq!(lanes[3].status, LaneStatus::Detached);
}

#[test]
fn legacy_lane_json_migrates_only_at_the_record_input_edge() {
    let legacy = serde_json::json!({
        "id": "lane_legacy",
        "task_id": "task_legacy",
        "agent": "codex",
        "screen": "lane",
        "transport": "tmux",
        "status": "stopped",
        "summary": "legacy lane stopped by operator",
        "evidence": ["log lane_legacy.log"]
    });
    let lane: AgentLaneRecord = serde_json::from_value(legacy).unwrap();
    assert_eq!(lane.role, AgentRole::Coder);
    assert_eq!(lane.route, AgentRoute::Tmux);
    assert_eq!(lane.status, LaneStatus::Detached);
    assert_eq!(lane.mutation_policy, MutationPolicy::ProposeOnly);
    assert_eq!(lane.target, ExecutionTarget::Local);
}

#[test]
fn legacy_lane_route_accepts_terminal_alias() {
    assert_eq!(legacy_lane_route("terminal").unwrap(), AgentRoute::Terminal);
}

#[test]
fn typed_task_records_preserve_v0_names_and_migrate_legacy_values() {
    let legacy = serde_json::json!({
        "id": "task_test",
        "parent_id": null,
        "agent": "shell",
        "kind": "test",
        "transport": "shell",
        "title": "cargo test",
        "status": "starting",
        "activity": "starting test",
        "summary": "test task",
        "progress": 0,
        "started_at": null,
        "updated_at": null,
        "workspace": null,
        "evidence": [],
        "permissions": [],
        "decision": null,
        "result": null,
        "resume_handle": null,
        "pid": null,
        "next_action": null
    });
    let task: AgentTaskRecord = serde_json::from_value(legacy).unwrap();
    assert_eq!(task.role, AgentRole::Tester);
    assert_eq!(task.kind, AgentTaskKind::Test);
    assert_eq!(task.route, AgentRoute::Terminal);
    assert_eq!(task.status, AgentTaskStatus::Thinking);

    let encoded = serde_json::to_value(&task).unwrap();
    assert_eq!(encoded["agent"], "tester");
    assert_eq!(encoded["transport"], "terminal");
    assert!(encoded.get("role").is_none());
    assert!(encoded.get("route").is_none());

    let mut external = encoded;
    external["agent"] = serde_json::json!("external");
    assert!(serde_json::from_value::<AgentTaskRecord>(external).is_err());
}

#[test]
fn legacy_task_transport_values_migrate_by_task_kind_not_key_renaming() {
    let cases = [
        (
            "viden",
            "provider",
            "deepseek",
            AgentTaskKind::Provider,
            AgentRoute::BuiltIn,
        ),
        (
            "viden",
            "provider",
            "anthropic",
            AgentTaskKind::Provider,
            AgentRoute::BuiltIn,
        ),
        (
            "shell",
            "shell",
            "terminal",
            AgentTaskKind::Shell,
            AgentRoute::Terminal,
        ),
        (
            "viden",
            "tool",
            "local",
            AgentTaskKind::Tool,
            AgentRoute::Terminal,
        ),
        (
            "acp",
            "job",
            "acp-session",
            AgentTaskKind::Job,
            AgentRoute::Acp,
        ),
    ];

    for (agent, kind, transport, expected_kind, expected_route) in cases {
        let task: AgentTaskRecord =
            serde_json::from_value(legacy_task_json(agent, kind, transport)).unwrap();
        assert_eq!(task.kind, expected_kind);
        assert_eq!(task.route, expected_route);

        let encoded = serde_json::to_value(task).unwrap();
        // v0 names remain, but their values are the v1 typed classifications.
        assert!(encoded.get("role").is_none());
        assert!(encoded.get("route").is_none());
        assert_eq!(encoded["agent"], "coder");
        assert_eq!(
            encoded["transport"],
            serde_json::to_value(expected_route).unwrap()
        );
    }

    assert!(
        serde_json::from_value::<AgentTaskRecord>(legacy_task_json(
            "viden",
            "tool",
            "unrecognized-transport",
        ))
        .is_err()
    );
}

fn legacy_task_json(agent: &str, kind: &str, transport: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "task_legacy",
        "parent_id": null,
        "agent": agent,
        "kind": kind,
        "transport": transport,
        "title": "legacy task",
        "status": "queued",
        "activity": "queued",
        "summary": "legacy task",
        "progress": 0,
        "started_at": null,
        "updated_at": null,
        "workspace": null,
        "evidence": [],
        "permissions": [],
        "decision": null,
        "result": null,
        "resume_handle": null,
        "pid": null,
        "next_action": null
    })
}

#[test]
fn agent_dag_context_evidence_and_merge_gate_roundtrip_json() {
    let task = AgentDagTaskSpec {
        task_id: "agent_task_1".to_string(),
        role: AgentRole::Planner,
        title: "Plan runtime split".to_string(),
        objective: "Create architecture and implementation plan".to_string(),
        dependencies: Vec::new(),
        workspace: Some("/tmp/viden".to_string()),
        file_scope: vec!["crates/runtime".to_string(), "crates/types".to_string()],
        context_bundle_id: Some("ctx_agent_task_1".to_string()),
        required_evidence: vec!["plan".to_string(), "architecture".to_string()],
        permission_policy: "read_only".to_string(),
    };
    let dag = AgentDagRecord {
        dag_id: "dag_1".to_string(),
        goal: "Complete 0.2.2 agent role runtime".to_string(),
        status: AgentDagStatus::Active,
        tasks: vec![task.clone()],
        created_at: Some(10),
        updated_at: Some(11),
    };
    let evidence = EvidenceRecord {
        evidence_id: "ev_1".to_string(),
        task_id: task.task_id.clone(),
        kind: EvidenceKind::Plan,
        summary: "planner produced a scoped DAG".to_string(),
        path: Some("docs/multi-agent-core-orchestration.md".to_string()),
        source: Some("planner".to_string()),
        created_at: Some(12),
    };
    let gate = MergeGateRecord {
        gate_id: "gate_1".to_string(),
        task_id: task.task_id.clone(),
        status: MergeGateStatus::CollectingEvidence,
        required_evidence: vec!["plan".to_string(), "review".to_string()],
        evidence_ids: vec![evidence.evidence_id.clone()],
        gate_type: MergeGateType::Artifact,
        owner: RuntimeOwner::default(),
        validator: None,
        policy_snapshot: MergeGatePolicySnapshot::default(),
        decision: None,
        conflict: None,
        applied_change_id: None,
        recovery_snapshot: None,
        audit_ids: Vec::new(),
        updated_at: Some(13),
    };

    assert_eq!(AgentRole::parse("doc-writer"), Some(AgentRole::DocWriter));
    assert_eq!(AgentRole::ReleaseOperator.as_str(), "release_operator");
    assert!(AgentDagStatus::Active.is_active());
    assert!(!MergeGateStatus::Merged.is_open());

    let encoded = serde_json::to_string(&(dag, evidence, gate)).unwrap();
    let (decoded_dag, decoded_evidence, decoded_gate): (
        AgentDagRecord,
        EvidenceRecord,
        MergeGateRecord,
    ) = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded_dag.tasks[0].role, AgentRole::Planner);
    assert_eq!(decoded_evidence.kind, EvidenceKind::Plan);
    assert_eq!(decoded_gate.status, MergeGateStatus::CollectingEvidence);
}

#[test]
fn schema_one_merge_gate_accepts_legacy_decisions_and_unknown_fields() {
    let gate: MergeGateRecord = serde_json::from_value(serde_json::json!({
        "gate_id": "gate_legacy",
        "task_id": "task_legacy",
        "status": "accepted",
        "required_evidence": ["patch"],
        "evidence_ids": ["evidence_patch"],
        "decision": "accepted before typed trust records",
        "updated_at": 13,
        "future_schema_one_field": {"ignored": true}
    }))
    .unwrap();

    let decision = gate
        .decision
        .clone()
        .expect("legacy decision should migrate");
    assert_eq!(decision.outcome, MergeGateDecisionOutcome::Legacy);
    assert_eq!(decision.reason, "accepted before typed trust records");
    assert_eq!(gate.gate_type, MergeGateType::Artifact);
    assert_eq!(gate.owner, RuntimeOwner::default());
    let legacy_encoded = serde_json::to_value(&gate).unwrap();
    assert_eq!(
        legacy_encoded["decision"],
        "accepted before typed trust records"
    );
    assert!(legacy_encoded.get("gate_type").is_none());
    assert!(legacy_encoded.get("owner").is_none());

    let encoded = serde_json::to_value(MergeGateRecord {
        gate_id: "gate_typed".to_string(),
        task_id: "task_typed".to_string(),
        status: MergeGateStatus::Accepted,
        required_evidence: vec!["patch".to_string()],
        evidence_ids: vec!["evidence_patch".to_string()],
        gate_type: MergeGateType::Patch,
        owner: RuntimeOwner {
            workspace_id: "workspace".to_string(),
            project_id: "project".to_string(),
            task_id: Some("task_typed".to_string()),
            ..RuntimeOwner::default()
        },
        validator: None,
        policy_snapshot: MergeGatePolicySnapshot::default(),
        decision: Some(MergeGateDecision {
            outcome: MergeGateDecisionOutcome::Accepted,
            reason: "typed acceptance".to_string(),
            owner: RuntimeOwner::default(),
            evidence_ids: vec!["evidence_patch".to_string()],
            reviewed_evidence: Vec::new(),
            review_request_id: None,
            audit_id: "audit_typed".to_string(),
            decided_at: 14,
        }),
        conflict: None,
        applied_change_id: None,
        recovery_snapshot: None,
        audit_ids: vec!["audit_typed".to_string()],
        updated_at: Some(14),
    })
    .unwrap();

    assert_eq!(encoded["decision"]["outcome"], "accepted");
    assert!(encoded["decision"].is_object());
}

#[test]
fn schema_one_trust_loop_commands_roundtrip_as_additive_typed_variants() {
    let owner = RuntimeOwner {
        workspace_id: "workspace-trust".to_string(),
        project_id: "project-trust".to_string(),
        lane_id: Some("lane-reviewer".to_string()),
        session_id: Some("session-reviewer".to_string()),
        task_id: Some("task-trust".to_string()),
        turn_id: None,
    };
    let commands = vec![
        RuntimeCommand::CreateHandoff {
            handoff_id: "handoff-trust".to_string(),
            task_id: "task-trust".to_string(),
            from_lane_id: "lane-coder".to_string(),
            to_lane_id: "lane-reviewer".to_string(),
            owner: owner.clone(),
            summary: "ready for review".to_string(),
            acceptance: HandoffAcceptance::Accepted,
        },
        RuntimeCommand::RequestReview {
            review_id: "review-trust".to_string(),
            gate_id: "gate-trust".to_string(),
            requester_lane_id: "lane-coder".to_string(),
            reviewer_lane_id: "lane-reviewer".to_string(),
            owner: owner.clone(),
            evidence_ids: vec!["evidence-trust".to_string()],
        },
        RuntimeCommand::ConfirmContract {
            contract_id: "contract-trust".to_string(),
            task_id: "task-trust".to_string(),
            owner: owner.clone(),
            summary: "contract confirmed".to_string(),
            decision: ContractDecision::Confirmed,
        },
        RuntimeCommand::SetDependency {
            dependency_id: "dependency-trust".to_string(),
            task_id: "task-trust".to_string(),
            depends_on_task_id: "task-base".to_string(),
            owner: owner.clone(),
            state: DependencyState::Blocked,
            reason: "waiting for base".to_string(),
        },
        RuntimeCommand::BounceMergeConflict {
            gate_id: "gate-trust".to_string(),
            original_lane_id: "lane-coder".to_string(),
            owner: owner.clone(),
            reason: "context mismatch".to_string(),
        },
        RuntimeCommand::RevalidateMergeConflict {
            gate_id: "gate-trust".to_string(),
            bounce_id: "bounce-trust".to_string(),
            actor: owner.clone(),
            evidence: ReviewedEvidenceBinding {
                evidence_id: "evidence-trust".to_string(),
                source_hash: "ab".repeat(32),
            },
        },
        RuntimeCommand::DecideReview {
            review_id: "review-trust".to_string(),
            verdict: ReviewVerdict::Accepted,
            feedback: Some("evidence matches the request".to_string()),
            actor: owner.clone(),
        },
        RuntimeCommand::RevertAppliedChange {
            gate_id: "gate-trust".to_string(),
            owner,
            reason: "verification failed".to_string(),
        },
    ];

    let encoded = serde_json::to_string(&commands).unwrap();
    let decoded: Vec<RuntimeCommand> = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, commands);
    for command_type in [
        "create_handoff",
        "request_review",
        "confirm_contract",
        "set_dependency",
        "bounce_merge_conflict",
        "revalidate_merge_conflict",
        "decide_review",
        "revert_applied_change",
    ] {
        assert!(encoded.contains(command_type));
    }
    assert!(encoded.contains("\"verdict\":\"accepted\""));
}

#[test]
fn schema_one_review_request_feedback_is_additive_and_optional() {
    // Legacy records written before the review verdict carried feedback must
    // still deserialize, and an absent verdict note must not appear on the wire.
    let legacy: ReviewRequestRecord = serde_json::from_value(serde_json::json!({
        "review_id": "review-legacy",
        "gate_id": "gate-legacy",
        "task_id": "task-legacy",
        "requester_lane_id": "lane-coder",
        "reviewer_lane_id": "lane-reviewer",
        "owner": RuntimeOwner::default(),
        "evidence_ids": ["evidence-legacy"],
        "status": "pending",
        "audit_id": "audit-legacy",
        "updated_at": 7
    }))
    .unwrap();
    assert_eq!(legacy.feedback, None);
    let encoded = serde_json::to_value(&legacy).unwrap();
    assert!(encoded.get("feedback").is_none());

    let decided = ReviewRequestRecord {
        status: ReviewRequestStatus::Rejected,
        feedback: Some("missing regression coverage".to_string()),
        ..legacy
    };
    let round_tripped: ReviewRequestRecord =
        serde_json::from_value(serde_json::to_value(&decided).unwrap()).unwrap();
    assert_eq!(round_tripped, decided);
}

#[test]
fn schema_one_reject_commands_default_missing_actor_for_legacy_json() {
    let reject_gate: RuntimeCommand = serde_json::from_value(serde_json::json!({
        "type": "reject_merge_gate",
        "gate_id": "gate-legacy",
        "reason": "legacy rejection"
    }))
    .unwrap();
    assert!(matches!(
        reject_gate,
        RuntimeCommand::RejectMergeGate { actor, .. } if actor == RuntimeOwner::default()
    ));

    let reject_artifact: RuntimeCommand = serde_json::from_value(serde_json::json!({
        "type": "reject_agent_artifact",
        "gate_id": "gate-legacy",
        "evidence_id": "evidence-legacy",
        "reason": "legacy artifact rejection"
    }))
    .unwrap();
    assert!(matches!(
        reject_artifact,
        RuntimeCommand::RejectAgentArtifact { actor, .. } if actor == RuntimeOwner::default()
    ));
}

#[test]
fn trust_decisions_bind_actor_reviewed_hashes_and_durable_recovery_reference() {
    let actor = RuntimeOwner {
        workspace_id: "workspace-trust".to_string(),
        project_id: "project-trust".to_string(),
        lane_id: Some("lane-reviewer".to_string()),
        session_id: Some("session-reviewer".to_string()),
        task_id: Some("task-trust".to_string()),
        turn_id: None,
    };
    let binding = ReviewedEvidenceBinding {
        evidence_id: "evidence-patch".to_string(),
        source_hash: "ab".repeat(32),
    };
    let command = RuntimeCommand::AcceptMergeGate {
        gate_id: "gate-trust".to_string(),
        actor: actor.clone(),
        reviewed_evidence: vec![binding.clone()],
        decision: Some("reviewed exact patch bytes".to_string()),
    };
    let recovery = RecoverySnapshotReference {
        snapshot_id: "recovery-change-1".to_string(),
        manifest_sha256: "cd".repeat(32),
    };

    let command_json = serde_json::to_value(&command).unwrap();
    assert_eq!(command_json["actor"]["lane_id"], "lane-reviewer");
    assert_eq!(
        command_json["reviewed_evidence"][0]["source_hash"],
        "ab".repeat(32)
    );
    assert_eq!(
        serde_json::from_value::<RuntimeCommand>(command_json).unwrap(),
        command
    );
    assert_eq!(
        serde_json::from_value::<RecoverySnapshotReference>(
            serde_json::to_value(&recovery).unwrap()
        )
        .unwrap(),
        recovery
    );
}

#[test]
fn canonical_evidence_status_preserves_legacy_summary_only_shape() {
    let legacy_json = r#"{
        "id":"evidence-legacy",
        "kind":"patch",
        "summary":"patch was generated from an older ACP flow",
        "path":null,
        "source":"acp",
        "timestamp":12
    }"#;
    let evidence: EvidenceView = serde_json::from_str(legacy_json).unwrap();

    assert_eq!(evidence.canonical, None);
    assert_eq!(
        canonical_evidence_status(&evidence),
        EvidenceCanonicalStatus::Missing
    );
    assert!(
        !serde_json::to_string(&evidence)
            .unwrap()
            .contains("canonical")
    );
}

#[test]
fn canonical_evidence_status_reports_verified_and_quality_failed_states() {
    let mut evidence = EvidenceView {
        id: "evidence-canonical".to_string(),
        kind: "test_result".to_string(),
        summary: "cargo test -p viden-runtime passed".to_string(),
        path: None,
        source: Some("cargo".to_string()),
        canonical: Some(CanonicalEvidenceReference {
            item_id: "ctxi-test".to_string(),
            bundle_id: "bundle-test".to_string(),
            source_hash: "ab".repeat(32),
            producer: EvidenceProducer {
                identity: "executor".to_string(),
                role: "tester".to_string(),
                task_id: "task-test".to_string(),
            },
            permission_snapshot_id: Some("perm-task-test".to_string()),
            permission_scope: ContextScope::Task("task-test".to_string()),
            evidence_scope: ContextScope::Task("task-test".to_string()),
            verification: EvidenceVerificationState::Verified,
            quality: EvidenceQualityFacts {
                status: EvidenceQualityStatus::Pass,
                reason_codes: Vec::new(),
            },
        }),
        metadata: None,
        timestamp: Some(12),
        owner: None,
    };

    assert_eq!(
        canonical_evidence_status(&evidence),
        EvidenceCanonicalStatus::Verified
    );

    let canonical = evidence.canonical.as_mut().unwrap();
    canonical.quality.status = EvidenceQualityStatus::Fail;
    canonical.quality.reason_codes = vec![EvidenceCanonicalReasonCode::QualityFailed];
    assert_eq!(
        canonical_evidence_status(&evidence),
        EvidenceCanonicalStatus::NeedsChanges
    );
}

#[test]
fn lsp_diagnostic_roundtrips_json() {
    let diagnostic = LspDiagnostic {
        path: "src/lib.rs".to_string(),
        range: LspRange {
            start: LspPosition {
                line: 1,
                character: 2,
            },
            end: LspPosition {
                line: 1,
                character: 5,
            },
        },
        severity: Some(2),
        source: Some("rust-analyzer".to_string()),
        code: Some("E0308".to_string()),
        message: "mismatched types".to_string(),
    };

    let encoded = serde_json::to_string(&diagnostic).unwrap();
    let decoded: LspDiagnostic = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, diagnostic);
}

#[test]
fn transcript_tool_result_preserves_exit_code() {
    let entry = TranscriptEntry::ToolResult {
        result: ToolResult {
            tool_call_id: "tool_1".to_string(),
            name: "shell".to_string(),
            output: "failed".to_string(),
            diff: None,
            success: false,
            exit_code: Some(7),
        },
    };

    let line = entry.to_json_line();
    assert!(line.contains("\"exit_code\":7"));
    assert_eq!(TranscriptEntry::from_json_line(&line).unwrap(), entry);
}

/// Compatibility follow-up 12: a persisted non-ASCII body replays byte-equal.
///
/// The writer already emits raw UTF-8 — the line on disk is correct — so the
/// decoder is the only place the text can be lost. Decoding it byte by byte
/// mapped every byte at or above `0x80` to its Latin-1 code point and replayed
/// `"café 你好"` as `"cafÃ© ä½ å¥½"`.
#[test]
fn transcript_message_replays_persisted_non_ascii_text_as_utf8() {
    let body = "café 你好 — ✓";
    let entry = TranscriptEntry::Message {
        message: Message {
            id: "msg_utf8".to_string(),
            role: Role::Assistant,
            content: body.to_string(),
            timestamp: 7,
            tool_name: None,
            tool_call_id: None,
        },
    };

    let line = entry.to_json_line();
    assert!(
        line.contains(body),
        "the writer persists the body as raw UTF-8, so nothing on disk changes: {line}"
    );

    let TranscriptEntry::Message { message } = TranscriptEntry::from_json_line(&line).unwrap()
    else {
        panic!("expected a message entry");
    };
    assert_eq!(
        message.content, body,
        "a persisted body must replay byte-equal, never as Latin-1 mojibake"
    );
}

/// Every string-carrying field of every hand-written entry shape decodes the
/// same way: one decoder serves them all, so proving one field would not prove
/// the surface a client actually reads.
#[test]
fn every_persisted_transcript_string_field_replays_as_utf8() {
    let body = "Änderung: 补丁 ✓";
    let mut input = ToolInput::new();
    input.insert("path".to_string(), "文档/naïve.txt".to_string());

    let entries = vec![
        TranscriptEntry::ToolCall {
            call: ToolCall {
                id: "call_utf8".to_string(),
                name: "edit_file".to_string(),
                input,
            },
        },
        TranscriptEntry::ToolResult {
            result: ToolResult {
                tool_call_id: "call_utf8".to_string(),
                name: "edit_file".to_string(),
                output: body.to_string(),
                diff: Some("+ 补丁".to_string()),
                success: true,
                exit_code: Some(0),
            },
        },
        TranscriptEntry::Permission {
            entry: PermissionLogEntry {
                timestamp: 3,
                tool_name: "edit_file".to_string(),
                decision: "allow".to_string(),
                reason: "作用域内".to_string(),
                message: Some("准许一次".to_string()),
            },
        },
        TranscriptEntry::Command {
            entry: CommandLogEntry {
                timestamp: 4,
                name: "résumé".to_string(),
                args: vec!["--模式".to_string(), "build".to_string()],
                output: body.to_string(),
            },
        },
        TranscriptEntry::SessionMeta {
            entry: SessionMetaEntry {
                timestamp: 5,
                key: "turn_owner".to_string(),
                value: "巷道-α".to_string(),
            },
        },
    ];

    for entry in entries {
        let line = entry.to_json_line();
        assert_eq!(
            TranscriptEntry::from_json_line(&line).unwrap(),
            entry,
            "entry must survive its own persisted line: {line}"
        );
    }
}

/// Escapes keep working beside the UTF-8 decode. The writer emits only `\"`,
/// `\\`, `\n`, `\r`, and `\t`, but a persisted line may also carry the
/// `\uXXXX` escapes JSON allows, including an astral character written as a
/// surrogate pair.
#[test]
fn transcript_string_escapes_decode_to_the_characters_they_name() {
    let entry = TranscriptEntry::SessionMeta {
        entry: SessionMetaEntry {
            timestamp: 1,
            key: "escapes".to_string(),
            value: "quote:\" slash:\\ nl:\n cr:\r tab:\t".to_string(),
        },
    };
    assert_eq!(
        TranscriptEntry::from_json_line(&entry.to_json_line()).unwrap(),
        entry,
        "the writer's own escapes still round-trip"
    );

    // A foreign writer's escaped form of the same text this crate writes raw.
    let escaped = r#"{"type":"session_meta","timestamp":1,"key":"escapes","value":"A\u0042 \u00e9 \u4f60\u597d \ud83d\ude80 \u0022q\u0022"}"#;
    let TranscriptEntry::SessionMeta { entry } = TranscriptEntry::from_json_line(escaped).unwrap()
    else {
        panic!("expected a session meta entry");
    };
    assert_eq!(
        entry.value, "AB é 你好 🚀 \"q\"",
        "a `\\uXXXX` escape names a character, and a surrogate pair names one astral character"
    );
}

/// An escape that names no character is unreadable, not reinterpreted. It
/// becomes U+FFFD so the rest of the line still replays; silently treating its
/// bytes as text is what follow-up 12 was.
#[test]
fn an_escape_that_names_no_character_replays_as_the_replacement_character() {
    for value in [
        // A lone leading surrogate, a lone trailing surrogate, a leading
        // surrogate followed by something that is not a trailing one, and a
        // `\u` with too few hex digits.
        r"\ud800 tail",
        r"\udc00 tail",
        r"\ud83d\u0041 tail",
        r"\u00 tail",
    ] {
        let line =
            format!(r#"{{"type":"session_meta","timestamp":1,"key":"k","value":"{value}"}}"#);
        let TranscriptEntry::SessionMeta { entry } =
            TranscriptEntry::from_json_line(&line).unwrap()
        else {
            panic!("expected a session meta entry");
        };
        assert!(
            entry.value.contains('\u{fffd}'),
            "`{value}` names no character and must replay as U+FFFD, got {:?}",
            entry.value
        );
    }
}

#[test]
fn transcript_cost_usage_roundtrips_canonical_and_legacy_shapes() {
    let cost = CostUsageRecord {
        usage_id: "usage-canonical-1".to_string(),
        provider_id: "deepseek".to_string(),
        model: "deepseek-v4-flash".to_string(),
        scopes: vec![CostScope::Request("req-1".to_string())],
        tokens: TokenUsage {
            input_tokens: Some(11),
            output_tokens: Some(3),
            cached_input_tokens: Some(2),
            retrieval_tokens: None,
            total_tokens: Some(14),
        },
        estimate: None,
        actual_cost: None,
        attempt_index: 0,
        outcome: CostUsageOutcome::Success,
        recorded_at: Some(123),
    };
    let entry = TranscriptEntry::CostUsage {
        cost: Box::new(cost.clone()),
    };
    let line = entry.to_json_line();

    assert_eq!(TranscriptEntry::from_json_line(&line).unwrap(), entry);

    let legacy = r#"{"type":"cost_usage","request_id":"req-legacy-1","provider_id":"legacy-provider","model":"legacy-model","scope":{"type":"task","id":"task-legacy-1"},"input_tokens":5,"output_tokens":7,"estimated_cost_micro_usd":9}"#;
    let TranscriptEntry::CostUsage { cost: legacy_cost } =
        TranscriptEntry::from_json_line(legacy).unwrap()
    else {
        panic!("expected cost usage entry");
    };
    assert_eq!(legacy_cost.tokens.input_tokens, Some(5));
    assert_eq!(legacy_cost.tokens.output_tokens, Some(7));
    assert_eq!(legacy_cost.tokens.cached_input_tokens, Some(0));
    assert_eq!(legacy_cost.actual_cost, None);
    assert!(
        legacy_cost
            .scopes
            .contains(&CostScope::Request("req-legacy-1".to_string()))
    );
    assert!(
        legacy_cost
            .scopes
            .contains(&CostScope::AgentTask("task-legacy-1".to_string()))
    );
}

#[test]
fn transcript_row_kinds_roundtrip_with_stable_serde_names() {
    let mut input = ToolInput::new();
    input.insert("command".to_string(), "cargo test".to_string());
    let cost = CostUsageRecord {
        usage_id: "usage-row".to_string(),
        provider_id: "deepseek".to_string(),
        model: "deepseek-v4-flash".to_string(),
        scopes: vec![CostScope::Request("request-row".to_string())],
        tokens: TokenUsage {
            input_tokens: Some(10),
            output_tokens: Some(2),
            cached_input_tokens: Some(1),
            retrieval_tokens: None,
            total_tokens: Some(12),
        },
        estimate: None,
        actual_cost: None,
        attempt_index: 0,
        outcome: CostUsageOutcome::Success,
        recorded_at: Some(44),
    };
    let runtime_event = RuntimeEvent::with_timestamp(
        7,
        Some(55),
        RuntimeEventKind::CostUsageRecorded { cost: cost.clone() },
    );
    let kinds = vec![
        TranscriptRowKind::Message {
            message: Message {
                id: "msg-row".to_string(),
                role: Role::Assistant,
                content: "hello".to_string(),
                timestamp: 11,
                tool_name: None,
                tool_call_id: None,
            },
        },
        TranscriptRowKind::ToolCall {
            call: ToolCall {
                id: "tool-call-row".to_string(),
                name: "shell".to_string(),
                input,
            },
        },
        TranscriptRowKind::ToolResult {
            result: ToolResult {
                tool_call_id: "tool-call-row".to_string(),
                name: "shell".to_string(),
                output: "ok".to_string(),
                diff: None,
                success: true,
                exit_code: Some(0),
            },
        },
        TranscriptRowKind::Permission {
            entry: PermissionLogEntry {
                timestamp: 12,
                tool_name: "shell".to_string(),
                decision: "allow".to_string(),
                reason: "test".to_string(),
                message: None,
            },
        },
        TranscriptRowKind::Command {
            entry: CommandLogEntry {
                timestamp: 13,
                name: "status".to_string(),
                args: vec!["--json".to_string()],
                output: "{}".to_string(),
            },
        },
        TranscriptRowKind::SessionMeta {
            entry: SessionMetaEntry {
                timestamp: 14,
                key: "model".to_string(),
                value: "deepseek".to_string(),
            },
        },
        TranscriptRowKind::CostUsage {
            cost: Box::new(cost),
        },
        TranscriptRowKind::RuntimeEvent {
            event: Box::new(runtime_event),
        },
    ];

    let encoded = serde_json::to_value(&kinds).unwrap();
    let names = encoded
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value["type"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "message",
            "tool_call",
            "tool_result",
            "permission",
            "command",
            "session_meta",
            "cost_usage",
            "runtime_event"
        ]
    );
    let decoded: Vec<TranscriptRowKind> = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, kinds);
}

#[test]
fn transcript_row_page_request_and_loaded_event_roundtrip_json() {
    let page = TranscriptPage {
        rows: vec![TranscriptRow {
            id: TranscriptRowId("session-a:0".to_string()),
            cursor: TranscriptCursor {
                session_id: "session-a".to_string(),
                ordinal: 0,
            },
            timestamp: Some(1),
            kind: TranscriptRowKind::Message {
                message: Message {
                    id: "msg-a".to_string(),
                    role: Role::User,
                    content: "hello".to_string(),
                    timestamp: 1,
                    tool_name: None,
                    tool_call_id: None,
                },
            },
        }],
        older: None,
        newer: None,
        has_more: false,
    };
    let command = RuntimeCommand::LoadTranscriptPage {
        request: TranscriptPageRequest {
            session_id: "session-a".to_string(),
            before: None,
            limit: 25,
        },
    };
    let event = RuntimeEventKind::TranscriptPageLoaded {
        page: Box::new(page),
    };

    assert_eq!(
        serde_json::to_value(&command).unwrap()["type"],
        "load_transcript_page"
    );
    assert_eq!(
        serde_json::to_value(&event).unwrap()["type"],
        "transcript_page_loaded"
    );
    assert!(serde_json::to_string(&event).unwrap().contains("\"rows\""));
}

fn runtime_snapshot_for_contract() -> RuntimeSnapshot {
    RuntimeSnapshot {
        cwd: PathBuf::from("/tmp/viden"),
        provider_family: "deepseek".to_string(),
        model_label: "deepseek-v4-flash".to_string(),
        work_mode: WorkMode::Build,
        permission_mode: PermissionMode::Default,
        permission_level: PermissionLevel::Ask,
        config_summary: "provider=deepseek model=deepseek-v4-flash".to_string(),
        loaded_config_files: vec![PathBuf::from("/tmp/viden/config.toml")],
        startup_overrides: vec!["--provider=deepseek".to_string()],
        ui_preferences: ResolvedUiPreferences::default(),
    }
}

fn starter_lane_for_contract(lane_id: &str) -> AgentLaneRecord {
    AgentLaneRecord {
        id: lane_id.to_string(),
        task_id: None,
        role: AgentRole::Coder,
        route: AgentRoute::BuiltIn,
        gate_strength: GateStrength::Full,
        mutation_policy: MutationPolicy::ProposeOnly,
        worktree: Some(format!("/tmp/viden/.worktrees/{lane_id}")),
        branch: Some(format!("codex/{lane_id}")),
        target: ExecutionTarget::Local,
        data_egress: DataEgressPolicy::Deny,
        status: LaneStatus::Draft,
        budget: LaneBudget::default(),
        active_session_ids: Vec::new(),
        summary: "coder starter lane".to_string(),
        evidence: Vec::new(),
        run_stats: None,
    }
}

#[test]
fn runtime_v1_envelopes_roundtrip_owner_cursor_and_capabilities() {
    let owner = RuntimeOwner {
        workspace_id: "workspace-a".to_string(),
        project_id: "project-a".to_string(),
        lane_id: Some("lane-a".to_string()),
        session_id: Some("session-a".to_string()),
        task_id: Some("task-a".to_string()),
        turn_id: Some("turn-a".to_string()),
    };
    let cursor = EventCursor {
        stream_id: "stream-a".to_string(),
        sequence: 7,
    };
    let capabilities = BTreeSet::from([
        CapabilityId("runtime.replay".to_string()),
        CapabilityId("runtime.snapshot".to_string()),
    ]);
    let event = RuntimeEvent::with_timestamp(
        cursor.sequence,
        Some(17),
        RuntimeEventKind::AssistantDelta {
            message_id: "message-a".to_string(),
            task_id: owner.task_id.clone(),
            session_id: None,
            content: "hello".to_string(),
        },
    );
    let command = RuntimeCommandEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        client_id: "tui-a".to_string(),
        command_id: "command-a".to_string(),
        owner: owner.clone(),
        command: RuntimeCommand::QueueFollowUp {
            content: "continue".to_string(),
        },
    };
    let event = RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: owner.clone(),
        cursor: cursor.clone(),
        event: RuntimeWireEvent::Known(event),
    };
    let snapshot = RuntimeSnapshotEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        capabilities: capabilities.clone(),
        cursor: cursor.clone(),
        snapshot: runtime_snapshot_for_contract(),
        view: RuntimeViewState::new(runtime_snapshot_for_contract()),
    };
    let handshake = CoreHandshake {
        core_version: "0.3.0".to_string(),
        supported_schema_versions: vec![FRONTEND_SCHEMA_V1],
        active_schema_version: FRONTEND_SCHEMA_V1,
        capabilities,
    };

    let encoded = serde_json::to_string(&(command, event, snapshot, handshake)).unwrap();
    let (command, event, snapshot, handshake): (
        RuntimeCommandEnvelope,
        RuntimeEventEnvelope,
        RuntimeSnapshotEnvelope,
        CoreHandshake,
    ) = serde_json::from_str(&encoded).unwrap();

    assert_eq!(command.schema_version, SchemaVersion(1));
    assert_eq!(command.owner, owner);
    assert_eq!(event.cursor, cursor);
    assert!(matches!(event.event, RuntimeWireEvent::Known(ref event) if event.sequence == 7));
    assert_eq!(snapshot.capabilities, handshake.capabilities);
}

#[test]
fn starter_lane_events_roundtrip_as_known_with_exact_owner_and_cursor() {
    let owner = RuntimeOwner {
        workspace_id: "workspace-starter".to_string(),
        project_id: "project-starter".to_string(),
        lane_id: Some("lane-starter".to_string()),
        session_id: Some("session-starter".to_string()),
        task_id: None,
        turn_id: Some("turn-starter".to_string()),
    };
    let lane = starter_lane_for_contract("lane-starter");
    let preview = StarterLanePreview {
        preview_id: "preview-starter".to_string(),
        content_sha256: "ab".repeat(32),
        owner: owner.clone(),
        lane: lane.clone(),
        branch: "codex/lane-starter".to_string(),
        worktree_path: "/tmp/viden/.worktrees/lane-starter".to_string(),
        base_revision: "cd".repeat(20),
        diagnostics: Vec::new(),
    };
    let cases = [
        (
            "starter_lane_previewed",
            RuntimeEventKind::StarterLanePreviewed {
                preview: preview.clone(),
            },
        ),
        (
            "starter_lane_created",
            RuntimeEventKind::StarterLaneCreated {
                receipt: StarterLaneReceipt {
                    preview_id: preview.preview_id.clone(),
                    content_sha256: preview.content_sha256.clone(),
                    lane,
                    branch: preview.branch.clone(),
                    worktree_path: preview.worktree_path.clone(),
                    base_revision: preview.base_revision.clone(),
                    owner: owner.clone(),
                },
            },
        ),
        (
            "starter_lane_preview_invalidated",
            RuntimeEventKind::StarterLanePreviewInvalidated {
                owner: owner.clone(),
                preview_id: preview.preview_id,
                reason: StarterLanePreviewInvalidationReason::HashMismatch,
            },
        ),
    ];

    for (index, (expected_type, kind)) in cases.into_iter().enumerate() {
        let sequence = index as u64 + 1;
        let envelope = RuntimeEventEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            owner: owner.clone(),
            cursor: EventCursor {
                stream_id: "stream-starter".to_string(),
                sequence,
            },
            event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(sequence, Some(17), kind)),
        };
        let encoded = serde_json::to_value(&envelope).unwrap();
        assert_eq!(encoded["event"]["kind"]["type"], expected_type);
        let decoded: RuntimeEventEnvelope = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, envelope);
        assert!(
            matches!(
                &decoded.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewed { preview },
                    ..
                }) if preview.owner == decoded.owner
            ) || matches!(
                &decoded.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLaneCreated { receipt },
                    ..
                }) if receipt.owner == decoded.owner
            ) || matches!(
                &decoded.event,
                RuntimeWireEvent::Known(RuntimeEvent {
                    kind: RuntimeEventKind::StarterLanePreviewInvalidated { owner, .. },
                    ..
                }) if owner == &decoded.owner
            )
        );
        assert_eq!(decoded.owner, owner);
        assert_eq!(decoded.cursor.stream_id, "stream-starter");
        assert_eq!(decoded.cursor.sequence, sequence);
    }
}

#[test]
fn lane_runtime_owner_binding_reduces_by_lane_and_clears_only_terminal_lane() {
    let owner = |lane_id: &str, turn_id: &str| RuntimeOwner {
        workspace_id: "workspace-owner".to_string(),
        project_id: "project-owner".to_string(),
        lane_id: Some(lane_id.to_string()),
        session_id: Some(format!("session-{lane_id}")),
        task_id: Some(format!("task-{lane_id}")),
        turn_id: Some(turn_id.to_string()),
    };
    let binding_a = LaneRuntimeOwnerBinding {
        lane_id: "lane-a".to_string(),
        owner: owner("lane-a", "turn-a"),
    };
    let replacement_a = LaneRuntimeOwnerBinding {
        lane_id: "lane-a".to_string(),
        owner: owner("lane-a", "turn-a-next"),
    };
    let binding_b = LaneRuntimeOwnerBinding {
        lane_id: "lane-b".to_string(),
        owner: owner("lane-b", "turn-b"),
    };

    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());
    for binding in [binding_a.clone(), replacement_a.clone(), binding_b.clone()] {
        view.apply_event(&RuntimeEvent::new(
            1,
            RuntimeEventKind::LaneRuntimeOwnerBound { binding },
        ));
    }
    assert_eq!(
        view.lane_runtime_owners,
        vec![binding_a.clone(), binding_b.clone()]
    );

    let mismatched = LaneRuntimeOwnerBinding {
        lane_id: "lane-a".to_string(),
        owner: owner("lane-other", "turn-invalid"),
    };
    view.apply_event(&RuntimeEvent::new(
        2,
        RuntimeEventKind::LaneRuntimeOwnerBound {
            binding: mismatched,
        },
    ));
    assert_eq!(
        view.lane_runtime_owners,
        vec![binding_a.clone(), binding_b.clone()]
    );

    let mut running_lane = starter_lane_for_contract("lane-a");
    running_lane.status = LaneStatus::Running;
    view.apply_event(&RuntimeEvent::new(
        3,
        RuntimeEventKind::LaneUpdated { lane: running_lane },
    ));
    assert_eq!(
        view.lane_runtime_owners,
        vec![binding_a.clone(), binding_b.clone()]
    );

    for status in [
        LaneStatus::Done,
        LaneStatus::Failed,
        LaneStatus::Cancelled,
        LaneStatus::Archived,
    ] {
        let mut terminal_view = RuntimeViewState::new(runtime_snapshot_for_contract());
        for binding in [binding_a.clone(), binding_b.clone()] {
            terminal_view.apply_event(&RuntimeEvent::new(
                1,
                RuntimeEventKind::LaneRuntimeOwnerBound { binding },
            ));
        }
        let mut lane = starter_lane_for_contract("lane-a");
        lane.status = status;
        terminal_view.apply_event(&RuntimeEvent::new(
            2,
            RuntimeEventKind::LaneUpdated { lane },
        ));
        assert_eq!(terminal_view.lane_runtime_owners, vec![binding_b.clone()]);
    }
}

#[test]
fn lane_runtime_owner_event_roundtrips_as_known_while_future_event_is_inert() {
    let owner = RuntimeOwner {
        workspace_id: "workspace-owner".to_string(),
        project_id: "project-owner".to_string(),
        lane_id: Some("lane-a".to_string()),
        session_id: Some("session-a".to_string()),
        task_id: Some("task-a".to_string()),
        turn_id: Some("turn-a".to_string()),
    };
    let binding = LaneRuntimeOwnerBinding {
        lane_id: "lane-a".to_string(),
        owner: owner.clone(),
    };
    let envelope = RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner,
        cursor: EventCursor {
            stream_id: "stream-owner".to_string(),
            sequence: 1,
        },
        event: RuntimeWireEvent::Known(RuntimeEvent::new(
            1,
            RuntimeEventKind::LaneRuntimeOwnerBound {
                binding: binding.clone(),
            },
        )),
    };

    let encoded = serde_json::to_value(&envelope).unwrap();
    assert_eq!(encoded["event"]["kind"]["type"], "lane_runtime_owner_bound");
    let decoded: RuntimeEventEnvelope = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, envelope);
    assert!(matches!(
        decoded.event,
        RuntimeWireEvent::Known(RuntimeEvent {
            kind: RuntimeEventKind::LaneRuntimeOwnerBound { binding: decoded },
            ..
        }) if decoded == binding
    ));

    let future: RuntimeWireEvent = serde_json::from_value(serde_json::json!({
        "sequence": 2,
        "timestamp": null,
        "kind": {
            "type": "future_lane_runtime_owner_rotated",
            "payload": {"lane_id": "lane-a"}
        }
    }))
    .unwrap();
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::LaneRuntimeOwnerBound { binding },
    ));
    let before = view.clone();
    if let RuntimeWireEvent::Known(event) = future {
        view.apply_event(&event);
    }
    assert_eq!(view, before);
}

#[test]
fn lane_runtime_owner_event_transport_rejects_payload_owner_mismatch() {
    let payload_owner = RuntimeOwner {
        workspace_id: "workspace-owner".to_string(),
        project_id: "project-owner".to_string(),
        lane_id: Some("lane-a".to_string()),
        session_id: Some("session-payload".to_string()),
        task_id: Some("task-a".to_string()),
        turn_id: Some("turn-payload".to_string()),
    };
    let envelope_owner = RuntimeOwner {
        session_id: Some("session-envelope".to_string()),
        turn_id: Some("turn-envelope".to_string()),
        ..payload_owner.clone()
    };
    let envelope = RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: envelope_owner,
        cursor: EventCursor {
            stream_id: "stream-owner".to_string(),
            sequence: 1,
        },
        event: RuntimeWireEvent::Known(RuntimeEvent::new(
            1,
            RuntimeEventKind::LaneRuntimeOwnerBound {
                binding: LaneRuntimeOwnerBinding {
                    lane_id: "lane-a".to_string(),
                    owner: payload_owner,
                },
            },
        )),
    };

    assert!(serde_json::to_value(envelope).is_err());
}

#[test]
fn starter_lane_view_keys_previews_receipts_and_invalidation_by_owner_and_preview_id() {
    let owner_a = RuntimeOwner {
        workspace_id: "workspace-a".to_string(),
        project_id: "project".to_string(),
        lane_id: Some("lane-a".to_string()),
        ..RuntimeOwner::default()
    };
    let owner_b = RuntimeOwner {
        workspace_id: "workspace-b".to_string(),
        project_id: "project".to_string(),
        lane_id: Some("lane-b".to_string()),
        ..RuntimeOwner::default()
    };
    let preview = |owner: RuntimeOwner, lane_id: &str| StarterLanePreview {
        preview_id: "colliding-preview".to_string(),
        content_sha256: "ab".repeat(32),
        owner,
        lane: starter_lane_for_contract(lane_id),
        branch: format!("codex/{lane_id}"),
        worktree_path: format!("/tmp/viden/.worktrees/{lane_id}"),
        base_revision: "cd".repeat(20),
        diagnostics: Vec::new(),
    };
    let preview_a = preview(owner_a.clone(), "lane-a");
    let preview_b = preview(owner_b.clone(), "lane-b");
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());
    for preview in [preview_a.clone(), preview_b.clone()] {
        view.apply_event(&RuntimeEvent::new(
            1,
            RuntimeEventKind::StarterLanePreviewed { preview },
        ));
    }
    assert_eq!(view.starter_lane_previews.len(), 2);

    view.apply_event(&RuntimeEvent::new(
        2,
        RuntimeEventKind::StarterLanePreviewInvalidated {
            owner: owner_a.clone(),
            preview_id: preview_a.preview_id.clone(),
            reason: StarterLanePreviewInvalidationReason::HashMismatch,
        },
    ));
    assert_eq!(view.starter_lane_previews, vec![preview_b.clone()]);

    view.apply_event(&RuntimeEvent::new(
        3,
        RuntimeEventKind::StarterLanePreviewed {
            preview: preview_a.clone(),
        },
    ));
    for preview in [preview_a, preview_b] {
        view.apply_event(&RuntimeEvent::new(
            4,
            RuntimeEventKind::StarterLaneCreated {
                receipt: StarterLaneReceipt {
                    preview_id: preview.preview_id,
                    content_sha256: preview.content_sha256,
                    lane: preview.lane,
                    branch: preview.branch,
                    worktree_path: preview.worktree_path,
                    base_revision: preview.base_revision,
                    owner: preview.owner,
                },
            },
        ));
    }
    assert!(view.starter_lane_previews.is_empty());
    assert_eq!(view.starter_lane_receipts.len(), 2);
    assert!(
        view.starter_lane_receipts
            .iter()
            .any(|receipt| receipt.owner == owner_a)
    );
    assert!(
        view.starter_lane_receipts
            .iter()
            .any(|receipt| receipt.owner == owner_b)
    );
}

#[test]
fn starter_lane_event_transport_rejects_payload_owner_mismatch() {
    let envelope_owner = RuntimeOwner {
        workspace_id: "workspace-envelope".to_string(),
        project_id: "project".to_string(),
        ..RuntimeOwner::default()
    };
    let payload_owner = RuntimeOwner {
        workspace_id: "workspace-payload".to_string(),
        project_id: "project".to_string(),
        ..RuntimeOwner::default()
    };
    let events = [
        RuntimeEventKind::StarterLaneCreated {
            receipt: StarterLaneReceipt {
                preview_id: "preview-mismatch".to_string(),
                content_sha256: "ab".repeat(32),
                lane: starter_lane_for_contract("lane-mismatch"),
                branch: "codex/lane-mismatch".to_string(),
                worktree_path: "/tmp/viden/.worktrees/lane-mismatch".to_string(),
                base_revision: "cd".repeat(20),
                owner: payload_owner.clone(),
            },
        },
        RuntimeEventKind::StarterLanePreviewInvalidated {
            owner: payload_owner,
            preview_id: "preview-mismatch".to_string(),
            reason: StarterLanePreviewInvalidationReason::PermissionDenied,
        },
    ];

    for kind in events {
        let envelope = RuntimeEventEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            owner: envelope_owner.clone(),
            cursor: EventCursor {
                stream_id: "stream-mismatch".to_string(),
                sequence: 1,
            },
            event: RuntimeWireEvent::Known(RuntimeEvent::new(1, kind)),
        };
        assert!(serde_json::to_value(envelope).is_err());
    }
}

#[test]
fn runtime_v1_unknown_event_is_preserved() {
    let raw = r#"{
        "schema_version": 1,
        "owner": {
            "workspace_id": "workspace-a",
            "project_id": "project-a",
            "lane_id": "lane-a",
            "session_id": "session-a",
            "task_id": "task-a",
            "turn_id": "turn-a"
        },
        "cursor": {"stream_id": "stream-a", "sequence": 9},
        "event": {
            "sequence": 9,
            "timestamp": 17,
            "kind": {"type": "future_event", "payload": {"x": 1}}
        }
    }"#;

    let decoded: RuntimeEventEnvelope = serde_json::from_str(raw).unwrap();
    assert_eq!(decoded.schema_version, FRONTEND_SCHEMA_V1);
    assert_eq!(decoded.cursor.stream_id, "stream-a");
    assert!(matches!(
        decoded.event,
        RuntimeWireEvent::Unknown { ref event_type, ref payload }
            if event_type == "future_event" && payload == &serde_json::json!({"x": 1})
    ));
    assert_eq!(
        EventCursor {
            stream_id: "stream-a".to_string(),
            sequence: 8,
        }
        .classify_incoming(&decoded.cursor),
        EventCursorOrder::Next
    );

    let encoded = serde_json::to_string(&decoded).unwrap();
    let replayed: RuntimeEventEnvelope = serde_json::from_str(&encoded).unwrap();
    assert_eq!(replayed, decoded);
}

#[test]
fn frontend_host_capabilities_known_wire_events_roundtrip_without_placeholders() {
    let owner = RuntimeOwner {
        workspace_id: "workspace-host-fixture".to_string(),
        project_id: "project-host-fixture".to_string(),
        lane_id: Some("lane-host-fixture".to_string()),
        session_id: Some("session-host-fixture".to_string()),
        task_id: Some("task_host_fixture".to_string()),
        turn_id: Some("turn-host-fixture".to_string()),
    };
    let lane = starter_lane_for_contract("lane-host-fixture");
    let preview = StarterLanePreview {
        preview_id: "preview-host-fixture".to_string(),
        content_sha256: "ab".repeat(32),
        owner: owner.clone(),
        lane: lane.clone(),
        branch: "codex/lane-host-fixture".to_string(),
        worktree_path: "workspace/.worktrees/lane-host-fixture".to_string(),
        base_revision: "cd".repeat(20),
        diagnostics: Vec::new(),
    };
    let resolved = ResolvedUiPreferences {
        locale: LocaleId::ZhCn,
        skin: UiSkin::Ice,
        mode: UiColorMode::Dark,
        density: UiDensity::Compact,
        motion: UiMotion::Reduced,
        diagnostics: Vec::new(),
    };
    let cases = [
        RuntimeEventKind::UiPreferencesUpdated {
            resolved,
            persisted: None,
            diagnostics: Vec::new(),
        },
        RuntimeEventKind::RecentWorkLoaded {
            projects: vec![RecentProjectSummary {
                canonical_root: "workspace/project".to_string(),
                display_name: "project".to_string(),
                last_updated_at: 20,
                latest_session_id: Some("session-host-fixture".to_string()),
            }],
            sessions: vec![RecentSessionSummary {
                canonical_root: "workspace/project".to_string(),
                session_id: "session-host-fixture".to_string(),
                created_at: 10,
                last_updated_at: 20,
                message_count: 1,
                tool_call_count: 0,
                command_count: 1,
            }],
            diagnostics: Vec::new(),
        },
        RuntimeEventKind::StarterLanePreviewed {
            preview: preview.clone(),
        },
        RuntimeEventKind::StarterLaneCreated {
            receipt: StarterLaneReceipt {
                preview_id: preview.preview_id.clone(),
                content_sha256: preview.content_sha256.clone(),
                lane,
                branch: preview.branch.clone(),
                worktree_path: preview.worktree_path.clone(),
                base_revision: preview.base_revision.clone(),
                owner: owner.clone(),
            },
        },
        RuntimeEventKind::StarterLanePreviewInvalidated {
            owner: owner.clone(),
            preview_id: preview.preview_id,
            reason: StarterLanePreviewInvalidationReason::BaseRevisionChanged,
        },
        RuntimeEventKind::LaneRuntimeOwnerBound {
            binding: LaneRuntimeOwnerBinding {
                lane_id: "lane-host-fixture".to_string(),
                owner: owner.clone(),
            },
        },
    ];

    for (index, kind) in cases.into_iter().enumerate() {
        let sequence = index as u64 + 1;
        let envelope = RuntimeEventEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            owner: owner.clone(),
            cursor: EventCursor {
                stream_id: "fixture:frontend-host-services".to_string(),
                sequence,
            },
            event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
                sequence,
                Some(1_700_000_000 + sequence),
                kind,
            )),
        };
        let encoded = serde_json::to_value(&envelope).unwrap();
        let decoded: RuntimeEventEnvelope = serde_json::from_value(encoded).unwrap();
        assert!(
            matches!(decoded.event, RuntimeWireEvent::Known(_)),
            "event {sequence} must use a real known schema-1 wire fact"
        );
    }
}

#[test]
fn agent_adapter_and_session_commands_roundtrip_as_typed_schema_v1_intents() {
    let request = AgentSessionRequest {
        lane_id: "lane-agent".to_string(),
        agent_id: "claude-acp".to_string(),
        model: Some("sonnet".to_string()),
        load_session_id: Some("remote-session".to_string()),
        task: "review the runtime contract".to_string(),
    };
    let commands = [
        RuntimeCommand::QueryAgentAdapters,
        RuntimeCommand::ProbeAgentAdapter {
            agent_id: "claude-acp".to_string(),
        },
        RuntimeCommand::StartAgentSession {
            request: request.clone(),
        },
        RuntimeCommand::CancelAgentSession {
            session_id: "agent-session-1".to_string(),
        },
    ];

    for command in commands {
        let encoded = serde_json::to_string(&command).unwrap();
        let decoded: RuntimeCommand = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, command);
    }
}

#[test]
fn additive_agent_session_input_roundtrips_and_reduces() {
    let input = AgentSessionInput {
        session_id: "agent-session-1".to_string(),
        content: "continue with the failing test".to_string(),
    };
    let command = RuntimeCommand::SendAgentSessionInput {
        input: input.clone(),
    };
    let encoded = serde_json::to_value(&command).unwrap();
    let decoded: RuntimeCommand = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, command);

    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::AgentSessionInputAccepted {
            session_id: input.session_id.clone(),
            input_id: "agent-input-1".to_string(),
        },
    ));

    assert_eq!(
        view.agent_session_inputs,
        vec![AgentSessionInputView {
            session_id: input.session_id,
            input_id: "agent-input-1".to_string(),
        }]
    );
}

#[test]
fn agent_adapter_and_session_events_roundtrip_as_known_owner_scoped_facts() {
    let owner = RuntimeOwner {
        workspace_id: "workspace-agent".to_string(),
        project_id: "project-agent".to_string(),
        lane_id: Some("lane-agent".to_string()),
        session_id: Some("agent-session-1".to_string()),
        ..RuntimeOwner::default()
    };
    let adapter = AgentAdapterView {
        agent_id: "claude-acp".to_string(),
        display_name: "Claude Agent".to_string(),
        route: AgentRoute::Acp,
        source: AgentAdapterSource::Registry,
        availability: AgentAvailability::Available,
        auth_state: AgentAuthState::Ready,
        startability: AgentStartability::Ready,
        capabilities: vec![CapabilityId("agent.session.prompt".to_string())],
        models: Vec::new(),
        diagnostics: Vec::new(),
    };
    let session = AgentSessionView {
        session_id: "agent-session-1".to_string(),
        lane_id: "lane-agent".to_string(),
        agent_id: adapter.agent_id.clone(),
        model: None,
        status: AgentSessionStatus::Running,
        owner: owner.clone(),
        task: "review the runtime contract".to_string(),
        diagnostic: None,
        output: None,
    };
    let cases = [
        RuntimeEventKind::AgentAdaptersLoaded {
            adapters: vec![adapter.clone()],
        },
        RuntimeEventKind::AgentAdapterProbed { adapter },
        RuntimeEventKind::AgentSessionStarted {
            session: session.clone(),
        },
        RuntimeEventKind::AgentSessionUpdated {
            session: session.clone(),
        },
        RuntimeEventKind::AgentSessionCompleted {
            session: session.clone(),
        },
        RuntimeEventKind::AgentSessionFailed { session },
    ];

    for (index, kind) in cases.into_iter().enumerate() {
        let sequence = index as u64 + 1;
        let envelope = RuntimeEventEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            owner: owner.clone(),
            cursor: EventCursor {
                stream_id: "stream-agent".to_string(),
                sequence,
            },
            event: RuntimeWireEvent::Known(RuntimeEvent::new(sequence, kind)),
        };
        let encoded = serde_json::to_string(&envelope).unwrap();
        let decoded: RuntimeEventEnvelope = serde_json::from_str(&encoded).unwrap();
        assert!(matches!(decoded.event, RuntimeWireEvent::Known(_)));
    }

    let mismatched = RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: RuntimeOwner {
            lane_id: Some("other-lane".to_string()),
            ..owner
        },
        cursor: EventCursor {
            stream_id: "stream-agent".to_string(),
            sequence: 1,
        },
        event: RuntimeWireEvent::Known(RuntimeEvent::new(
            1,
            RuntimeEventKind::AgentSessionStarted {
                session: AgentSessionView {
                    session_id: "agent-session-2".to_string(),
                    lane_id: "lane-agent".to_string(),
                    agent_id: "claude-acp".to_string(),
                    model: None,
                    status: AgentSessionStatus::Starting,
                    owner: RuntimeOwner {
                        workspace_id: "workspace-agent".to_string(),
                        project_id: "project-agent".to_string(),
                        lane_id: Some("lane-agent".to_string()),
                        session_id: Some("agent-session-2".to_string()),
                        ..RuntimeOwner::default()
                    },
                    task: "review".to_string(),
                    diagnostic: None,
                    output: None,
                },
            },
        )),
    };
    assert!(serde_json::to_string(&mismatched).is_err());

    let embedded_owner = RuntimeOwner {
        workspace_id: "workspace-agent".to_string(),
        project_id: "project-agent".to_string(),
        lane_id: Some("lane-agent".to_string()),
        session_id: Some("agent-session-3".to_string()),
        ..RuntimeOwner::default()
    };
    let inconsistent_identity = RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: embedded_owner.clone(),
        cursor: EventCursor {
            stream_id: "stream-agent".to_string(),
            sequence: 1,
        },
        event: RuntimeWireEvent::Known(RuntimeEvent::new(
            1,
            RuntimeEventKind::AgentSessionStarted {
                session: AgentSessionView {
                    session_id: "different-session".to_string(),
                    lane_id: "different-lane".to_string(),
                    agent_id: "claude-acp".to_string(),
                    model: None,
                    status: AgentSessionStatus::Starting,
                    owner: embedded_owner,
                    task: "review".to_string(),
                    diagnostic: None,
                    output: None,
                },
            },
        )),
    };
    assert!(serde_json::to_string(&inconsistent_identity).is_err());
}

#[test]
fn legacy_agent_session_without_output_decodes_as_absent() {
    let legacy = serde_json::json!({
        "session_id": "agent-session-legacy",
        "lane_id": "lane-agent",
        "agent_id": "codex-acp",
        "model": null,
        "status": "completed",
        "owner": {
            "workspace_id": "workspace-agent",
            "project_id": "project-agent",
            "lane_id": "lane-agent",
            "session_id": "agent-session-legacy",
            "task_id": null,
            "turn_id": null
        },
        "task": "inspect the repository",
        "diagnostic": null
    });

    let decoded: AgentSessionView = serde_json::from_value(legacy).unwrap();

    assert_eq!(decoded.output, None);
}

#[test]
fn agent_adapter_and_session_events_reduce_by_stable_identity_and_owner() {
    let owner = RuntimeOwner {
        workspace_id: "workspace-agent".to_string(),
        project_id: "project-agent".to_string(),
        lane_id: Some("lane-agent".to_string()),
        session_id: Some("agent-session-1".to_string()),
        task_id: None,
        turn_id: Some("turn-agent".to_string()),
    };
    let adapter = AgentAdapterView {
        agent_id: "claude-acp".to_string(),
        display_name: "Claude Agent".to_string(),
        route: AgentRoute::Acp,
        source: AgentAdapterSource::Registry,
        availability: AgentAvailability::NeedsAuth,
        auth_state: AgentAuthState::LoggedOut,
        startability: AgentStartability::AuthenticationRequired,
        capabilities: vec![CapabilityId("agent.session.prompt".to_string())],
        models: vec!["sonnet".to_string()],
        diagnostics: vec!["run claude auth login".to_string()],
    };
    let running = AgentSessionView {
        session_id: "agent-session-1".to_string(),
        lane_id: "lane-agent".to_string(),
        agent_id: adapter.agent_id.clone(),
        model: Some("sonnet".to_string()),
        status: AgentSessionStatus::Running,
        owner: owner.clone(),
        task: "review the runtime contract".to_string(),
        diagnostic: None,
        output: None,
    };
    let completed = AgentSessionView {
        status: AgentSessionStatus::Completed,
        diagnostic: Some("evidence recorded".to_string()),
        output: Some("first answer".to_string()),
        ..running.clone()
    };
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());

    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::AgentAdaptersLoaded {
            adapters: vec![adapter.clone()],
        },
    ));
    view.apply_event(&RuntimeEvent::new(
        2,
        RuntimeEventKind::AgentAdapterProbed {
            adapter: AgentAdapterView {
                availability: AgentAvailability::Available,
                auth_state: AgentAuthState::Ready,
                diagnostics: Vec::new(),
                ..adapter
            },
        },
    ));
    view.apply_event(&RuntimeEvent::new(
        3,
        RuntimeEventKind::AgentSessionStarted {
            session: running.clone(),
        },
    ));
    view.apply_event(&RuntimeEvent::new(
        4,
        RuntimeEventKind::AgentSessionCompleted {
            session: completed.clone(),
        },
    ));
    view.apply_event(&RuntimeEvent::new(
        5,
        RuntimeEventKind::AgentSessionInputAccepted {
            session_id: running.session_id.clone(),
            input_id: "agent-input-2".to_string(),
        },
    ));
    let continued = AgentSessionView {
        task: "second question".to_string(),
        status: AgentSessionStatus::Running,
        output: None,
        diagnostic: None,
        ..running.clone()
    };
    view.apply_event(&RuntimeEvent::new(
        6,
        RuntimeEventKind::AgentSessionStarted {
            session: continued.clone(),
        },
    ));
    let continued = AgentSessionView {
        status: AgentSessionStatus::Completed,
        output: Some("second answer".to_string()),
        ..continued
    };
    view.apply_event(&RuntimeEvent::new(
        7,
        RuntimeEventKind::AgentSessionCompleted {
            session: continued.clone(),
        },
    ));

    assert_eq!(view.agent_adapters.len(), 1);
    assert_eq!(
        view.agent_adapters[0].availability,
        AgentAvailability::Available
    );
    assert_eq!(view.agent_sessions, vec![continued]);
    assert_eq!(
        view.agent_conversation
            .iter()
            .map(|message| (message.role, message.content.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (AgentConversationRole::User, "review the runtime contract"),
            (AgentConversationRole::Assistant, "first answer"),
            (AgentConversationRole::User, "second question"),
            (AgentConversationRole::Assistant, "second answer"),
        ]
    );
}

#[test]
fn first_agent_session_promotes_the_provisional_lane_runtime_owner() {
    let provisional_owner = RuntimeOwner {
        workspace_id: "workspace-agent".to_string(),
        project_id: "project-agent".to_string(),
        lane_id: Some("lane-agent".to_string()),
        session_id: None,
        task_id: None,
        turn_id: None,
    };
    let session_owner = RuntimeOwner {
        session_id: Some("agent-session-1".to_string()),
        turn_id: Some("turn-agent".to_string()),
        ..provisional_owner.clone()
    };
    let session = AgentSessionView {
        session_id: "agent-session-1".to_string(),
        lane_id: "lane-agent".to_string(),
        agent_id: "codex-acp".to_string(),
        model: None,
        status: AgentSessionStatus::Starting,
        owner: session_owner.clone(),
        task: "inspect the repository".to_string(),
        diagnostic: None,
        output: None,
    };
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::LaneRuntimeOwnerBound {
            binding: LaneRuntimeOwnerBinding {
                lane_id: "lane-agent".to_string(),
                owner: provisional_owner,
            },
        },
    ));

    view.apply_event(&RuntimeEvent::new(
        2,
        RuntimeEventKind::AgentSessionStarted {
            session: session.clone(),
        },
    ));

    assert_eq!(view.agent_sessions, vec![session]);
    assert_eq!(
        view.lane_runtime_owners,
        vec![LaneRuntimeOwnerBinding {
            lane_id: "lane-agent".to_string(),
            owner: session_owner,
        }]
    );
}

#[test]
fn legacy_runtime_view_without_agent_extensions_defaults_to_empty_collections() {
    let view = RuntimeViewState::new(runtime_snapshot_for_contract());
    let mut encoded = serde_json::to_value(view).unwrap();
    encoded.as_object_mut().unwrap().remove("agent_adapters");
    encoded.as_object_mut().unwrap().remove("agent_sessions");
    encoded
        .as_object_mut()
        .unwrap()
        .remove("agent_conversation");

    let decoded: RuntimeViewState = serde_json::from_value(encoded).unwrap();

    assert!(decoded.agent_adapters.is_empty());
    assert!(decoded.agent_sessions.is_empty());
    assert!(decoded.agent_conversation.is_empty());
}

#[test]
fn d1_main_cockpit_fixture_replays_workspace_and_service_facts() {
    #[derive(serde::Deserialize)]
    struct Fixture {
        initial_snapshot: RuntimeSnapshot,
        events: Vec<RuntimeEventEnvelope>,
        expected_final_cursor: EventCursor,
        expected_view_sha256: String,
    }

    let fixture: Fixture = serde_json::from_str(include_str!(
        "../tests/fixtures/frontend-contract-v1/d1-main-cockpit.json"
    ))
    .unwrap();
    let encoded = serde_json::to_value(&fixture.events).unwrap();
    let replayed: Vec<RuntimeEventEnvelope> = serde_json::from_value(encoded).unwrap();
    assert_eq!(replayed, fixture.events);

    let mut view = RuntimeViewState::new(fixture.initial_snapshot);
    let mut cursor = None;
    for envelope in &fixture.events {
        if let RuntimeWireEvent::Known(event) = &envelope.event {
            view.apply_event(event);
        }
        cursor = Some(envelope.cursor.clone());
    }
    assert_eq!(cursor, Some(fixture.expected_final_cursor));
    assert_eq!(canonical_view_sha256(&view), fixture.expected_view_sha256);

    assert_eq!(
        view.workspace_source,
        Some(WorkspaceSourceView {
            status: WorkspaceSourceStatus::Ready,
            branch: Some("codex/d1-cockpit-core".to_string()),
            worktree: Some(".worktrees/d1-cockpit-core".to_string()),
            ahead: 1,
            behind: 0,
            added: 3,
            deleted: 1,
            dirty: true,
        })
    );
    assert_eq!(view.runtime_services.len(), 2);
    assert!(view.runtime_services.iter().any(|service| {
        service.id == "codegraph"
            && service.kind == RuntimeServiceKind::Mcp
            && service.status == RuntimeServiceStatus::Connected
    }));
    assert!(view.runtime_services.iter().any(|service| {
        service.id == "rust-analyzer"
            && service.kind == RuntimeServiceKind::Lsp
            && service.status == RuntimeServiceStatus::Ready
    }));
    assert_eq!(view.workspace_changes.len(), 1);
    assert_eq!(
        view.workspace_changes[0].path,
        "crates/types/src/runtime.rs"
    );
    assert_eq!(
        view.workspace_changes[0].kind,
        WorkspaceChangeKind::Modified
    );
    assert_eq!(view.check_runs.len(), 1);
    assert_eq!(view.check_runs[0].status, CheckRunStatus::Failed);
    assert_eq!(
        view.check_runs[0].failing_location.as_deref(),
        Some("crates/types/src/tests.rs:2500")
    );
}

#[test]
fn d1_main_cockpit_fixture_uses_only_the_canonical_cockpit_capability() {
    let fixture = include_str!("../tests/fixtures/frontend-contract-v1/d1-main-cockpit.json");

    assert!(fixture.contains("\"runtime.cockpit_context_v1\""));
    assert!(!fixture.contains("runtime.workspace_facts"));
}

#[test]
fn d1_main_cockpit_keeps_one_agent_session_per_lane_projection() {
    #[derive(serde::Deserialize)]
    struct Fixture {
        initial_snapshot: RuntimeSnapshot,
        events: Vec<RuntimeEventEnvelope>,
    }

    let fixture: Fixture = serde_json::from_str(include_str!(
        "../tests/fixtures/frontend-contract-v1/d1-main-cockpit.json"
    ))
    .unwrap();
    let mut view = RuntimeViewState::new(fixture.initial_snapshot);
    for envelope in &fixture.events {
        if let RuntimeWireEvent::Known(event) = &envelope.event {
            view.apply_event(event);
        }
    }

    assert_eq!(view.lane_runtime_owners.len(), 1);
    assert_eq!(view.agent_sessions.len(), 1);
    let first = view.agent_sessions[0].clone();
    assert_eq!(first.lane_id, "lane-d1-main");
    let selected_lane = view
        .lanes
        .iter()
        .find(|lane| lane.id == first.lane_id)
        .unwrap();
    assert_eq!(
        selected_lane.active_session_ids,
        vec![first.session_id.clone()]
    );
    assert_eq!(
        view.lane_runtime_owners[0].owner.session_id.as_deref(),
        Some(first.session_id.as_str())
    );
    let first_binding = view.lane_runtime_owners[0].clone();

    let replacement = AgentSessionView {
        session_id: "agent-session-d1-replacement".to_string(),
        lane_id: first.lane_id.clone(),
        agent_id: "replacement-agent".to_string(),
        model: None,
        status: AgentSessionStatus::Starting,
        owner: RuntimeOwner {
            session_id: Some("agent-session-d1-replacement".to_string()),
            ..first.owner.clone()
        },
        task: "replacement must not join the Lane projection".to_string(),
        diagnostic: None,
        output: None,
    };
    let replacement_owner = replacement.owner.clone();
    let replacement_session = RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: replacement_owner.clone(),
        cursor: EventCursor {
            stream_id: "fixture:d1-main-cockpit".to_string(),
            sequence: 99,
        },
        event: RuntimeWireEvent::Known(RuntimeEvent::new(
            99,
            RuntimeEventKind::AgentSessionStarted {
                session: replacement,
            },
        )),
    };
    let replacement_session: RuntimeEventEnvelope =
        serde_json::from_value(serde_json::to_value(replacement_session).unwrap()).unwrap();
    let RuntimeWireEvent::Known(replacement_session) = replacement_session.event else {
        panic!("matching replacement session envelope must remain known");
    };
    view.apply_event(&replacement_session);

    let replacement_binding = LaneRuntimeOwnerBinding {
        lane_id: first.lane_id.clone(),
        owner: replacement_owner,
    };
    let replacement_binding = RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: replacement_binding.owner.clone(),
        cursor: EventCursor {
            stream_id: "fixture:d1-main-cockpit".to_string(),
            sequence: 100,
        },
        event: RuntimeWireEvent::Known(RuntimeEvent::new(
            100,
            RuntimeEventKind::LaneRuntimeOwnerBound {
                binding: replacement_binding,
            },
        )),
    };
    let replacement_binding: RuntimeEventEnvelope =
        serde_json::from_value(serde_json::to_value(replacement_binding).unwrap()).unwrap();
    let RuntimeWireEvent::Known(replacement_binding) = replacement_binding.event else {
        panic!("matching replacement binding envelope must remain known");
    };
    view.apply_event(&replacement_binding);

    assert_eq!(view.agent_sessions, vec![first]);
    assert_eq!(view.lane_runtime_owners, vec![first_binding]);
}

#[test]
fn d1_terminal_lane_cannot_rebind_a_second_execution_owner() {
    #[derive(serde::Deserialize)]
    struct Fixture {
        initial_snapshot: RuntimeSnapshot,
        events: Vec<RuntimeEventEnvelope>,
    }

    let fixture: Fixture = serde_json::from_str(include_str!(
        "../tests/fixtures/frontend-contract-v1/d1-main-cockpit.json"
    ))
    .unwrap();
    let mut view = RuntimeViewState::new(fixture.initial_snapshot);
    for envelope in &fixture.events {
        if let RuntimeWireEvent::Known(event) = &envelope.event {
            view.apply_event(event);
        }
    }

    let first_session = view.agent_sessions[0].clone();
    let first_binding = view.lane_runtime_owners[0].clone();
    view.apply_event(&RuntimeEvent::new(
        90,
        RuntimeEventKind::LaneRuntimeOwnerBound {
            binding: first_binding.clone(),
        },
    ));
    assert_eq!(view.lane_runtime_owners, vec![first_binding]);

    let mut terminal_lane = view
        .lanes
        .iter()
        .find(|lane| lane.id == first_session.lane_id)
        .unwrap()
        .clone();
    terminal_lane.status = LaneStatus::Done;
    view.apply_event(&RuntimeEvent::new(
        91,
        RuntimeEventKind::LaneUpdated {
            lane: terminal_lane,
        },
    ));
    assert!(view.lane_runtime_owners.is_empty());

    let replacement_owner = RuntimeOwner {
        session_id: Some("agent-session-d1-terminal-replacement".to_string()),
        ..first_session.owner.clone()
    };
    let replacement_session = AgentSessionView {
        session_id: "agent-session-d1-terminal-replacement".to_string(),
        lane_id: first_session.lane_id.clone(),
        agent_id: "replacement-agent".to_string(),
        model: None,
        status: AgentSessionStatus::Starting,
        owner: replacement_owner.clone(),
        task: "must not restart a terminal Lane".to_string(),
        diagnostic: None,
        output: None,
    };
    view.apply_event(&RuntimeEvent::new(
        92,
        RuntimeEventKind::AgentSessionStarted {
            session: replacement_session,
        },
    ));
    view.apply_event(&RuntimeEvent::new(
        93,
        RuntimeEventKind::LaneRuntimeOwnerBound {
            binding: LaneRuntimeOwnerBinding {
                lane_id: first_session.lane_id.clone(),
                owner: replacement_owner,
            },
        },
    ));

    assert_eq!(view.agent_sessions, vec![first_session]);
    assert!(view.lane_runtime_owners.is_empty());
}

fn canonical_view_sha256(view: &RuntimeViewState) -> String {
    let value = serde_json::to_value(view).expect("runtime view must serialize");
    let sorted = sort_json(value);
    let bytes = serde_json::to_vec(&sorted).expect("canonical JSON must serialize");
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
fn d1_workspace_and_service_facts_upsert_by_stable_identity_and_stay_bounded() {
    let owner = RuntimeOwner {
        workspace_id: "workspace-d1".to_string(),
        project_id: "project-d1".to_string(),
        lane_id: Some("lane-d1".to_string()),
        session_id: Some("session-d1".to_string()),
        ..RuntimeOwner::default()
    };
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::WorkspaceSourceUpdated {
            source: WorkspaceSourceView {
                status: WorkspaceSourceStatus::Ready,
                branch: Some("main".to_string()),
                worktree: None,
                ahead: 0,
                behind: 0,
                added: 0,
                deleted: 0,
                dirty: false,
            },
        },
    ));
    view.apply_event(&RuntimeEvent::new(
        2,
        RuntimeEventKind::WorkspaceSourceUpdated {
            source: WorkspaceSourceView {
                status: WorkspaceSourceStatus::Ready,
                branch: Some("codex/d1".to_string()),
                worktree: Some(".worktrees/d1".to_string()),
                ahead: 2,
                behind: 1,
                added: 4,
                deleted: 3,
                dirty: true,
            },
        },
    ));
    assert_eq!(
        view.workspace_source.as_ref().unwrap().branch.as_deref(),
        Some("codex/d1")
    );

    for kind in [RuntimeServiceKind::Mcp, RuntimeServiceKind::Lsp] {
        view.apply_event(&RuntimeEvent::new(
            3,
            RuntimeEventKind::RuntimeServiceHealthUpdated {
                service: RuntimeServiceHealthView {
                    id: "shared-service-id".to_string(),
                    kind,
                    label: "shared service id".to_string(),
                    status: RuntimeServiceStatus::Ready,
                    detail_key: None,
                },
            },
        ));
    }
    assert_eq!(
        view.runtime_services
            .iter()
            .filter(|service| service.id == "shared-service-id")
            .count(),
        2
    );

    for index in 0..=50 {
        view.apply_event(&RuntimeEvent::new(
            10 + index,
            RuntimeEventKind::RuntimeServiceHealthUpdated {
                service: RuntimeServiceHealthView {
                    id: format!("service-{index}"),
                    kind: RuntimeServiceKind::Mcp,
                    label: format!("service-{index}"),
                    status: RuntimeServiceStatus::Connected,
                    detail_key: None,
                },
            },
        ));
        view.apply_event(&RuntimeEvent::new(
            100 + index,
            RuntimeEventKind::WorkspaceChangeUpdated {
                change: WorkspaceChangeView {
                    id: format!("change-{index}"),
                    owner: owner.clone(),
                    path: format!("src/{index}.rs"),
                    kind: WorkspaceChangeKind::Modified,
                    patch: None,
                    additions: index as u32,
                    deletions: 0,
                    diff: None,
                },
            },
        ));
        view.apply_event(&RuntimeEvent::new(
            200 + index,
            RuntimeEventKind::CheckRunUpdated {
                check: CheckRunView {
                    id: format!("check-{index}"),
                    owner: owner.clone(),
                    label: format!("check-{index}"),
                    command: "cargo test".to_string(),
                    status: CheckRunStatus::Passed,
                    summary: "passed".to_string(),
                    failing_location: None,
                },
            },
        ));
    }
    assert_eq!(view.runtime_services.len(), 50);
    assert_eq!(view.workspace_changes.len(), 50);
    assert_eq!(view.check_runs.len(), 50);
    assert_eq!(view.runtime_services[0].id, "service-1");
    assert_eq!(view.workspace_changes[0].id, "change-1");
    assert_eq!(view.check_runs[0].id, "check-1");

    view.apply_event(&RuntimeEvent::new(
        300,
        RuntimeEventKind::RuntimeServiceHealthUpdated {
            service: RuntimeServiceHealthView {
                id: "service-50".to_string(),
                kind: RuntimeServiceKind::Mcp,
                label: "service-50".to_string(),
                status: RuntimeServiceStatus::Degraded,
                detail_key: Some("service.degraded".to_string()),
            },
        },
    ));
    view.apply_event(&RuntimeEvent::new(
        301,
        RuntimeEventKind::WorkspaceChangeUpdated {
            change: WorkspaceChangeView {
                id: "change-50".to_string(),
                owner: owner.clone(),
                path: "src/replaced.rs".to_string(),
                kind: WorkspaceChangeKind::Renamed,
                patch: Some("rename".to_string()),
                additions: 1,
                deletions: 1,
                diff: None,
            },
        },
    ));
    view.apply_event(&RuntimeEvent::new(
        302,
        RuntimeEventKind::CheckRunUpdated {
            check: CheckRunView {
                id: "check-50".to_string(),
                owner,
                label: "check-50".to_string(),
                command: "cargo test -p viden-types".to_string(),
                status: CheckRunStatus::Failed,
                summary: "failed".to_string(),
                failing_location: Some("src/replaced.rs:1".to_string()),
            },
        },
    ));
    assert_eq!(view.runtime_services.len(), 50);
    assert_eq!(view.workspace_changes.len(), 50);
    assert_eq!(view.check_runs.len(), 50);
    assert_eq!(
        view.runtime_services
            .iter()
            .find(|service| service.id == "service-50" && service.kind == RuntimeServiceKind::Mcp)
            .unwrap()
            .status,
        RuntimeServiceStatus::Degraded
    );
    assert_eq!(
        view.workspace_changes
            .iter()
            .find(|change| change.id == "change-50")
            .unwrap()
            .path,
        "src/replaced.rs"
    );
    assert_eq!(
        view.check_runs
            .iter()
            .find(|check| check.id == "check-50")
            .unwrap()
            .status,
        CheckRunStatus::Failed
    );
}

#[test]
fn d1_change_and_check_identity_is_scoped_by_runtime_owner() {
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());
    let owner_a = RuntimeOwner {
        workspace_id: "workspace-a".to_string(),
        project_id: "project-a".to_string(),
        lane_id: Some("lane-a".to_string()),
        session_id: Some("session-a".to_string()),
        ..RuntimeOwner::default()
    };
    let owner_b = RuntimeOwner {
        workspace_id: "workspace-b".to_string(),
        project_id: "project-b".to_string(),
        lane_id: Some("lane-b".to_string()),
        session_id: Some("session-b".to_string()),
        ..RuntimeOwner::default()
    };

    for owner in [owner_a.clone(), owner_b.clone()] {
        view.apply_event(&RuntimeEvent::new(
            1,
            RuntimeEventKind::WorkspaceChangeUpdated {
                change: WorkspaceChangeView {
                    id: "shared-tool-id".to_string(),
                    owner: owner.clone(),
                    path: format!("{}/src/lib.rs", owner.workspace_id),
                    kind: WorkspaceChangeKind::Modified,
                    patch: None,
                    additions: 1,
                    deletions: 0,
                    diff: None,
                },
            },
        ));
        view.apply_event(&RuntimeEvent::new(
            2,
            RuntimeEventKind::CheckRunUpdated {
                check: CheckRunView {
                    id: "shared-tool-id".to_string(),
                    owner,
                    label: "cargo test".to_string(),
                    command: "cargo test".to_string(),
                    status: CheckRunStatus::Passed,
                    summary: "passed".to_string(),
                    failing_location: None,
                },
            },
        ));
    }

    assert_eq!(view.workspace_changes.len(), 2);
    assert_eq!(view.check_runs.len(), 2);
    assert!(
        view.workspace_changes
            .iter()
            .any(|change| change.owner == owner_a)
    );
    assert!(
        view.workspace_changes
            .iter()
            .any(|change| change.owner == owner_b)
    );
}

#[test]
fn legacy_runtime_view_without_d1_workspace_facts_defaults_to_empty_collections() {
    let view = RuntimeViewState::new(runtime_snapshot_for_contract());
    let mut encoded = serde_json::to_value(view).unwrap();
    let object = encoded.as_object_mut().unwrap();
    object.remove("workspace_source");
    object.remove("runtime_services");
    object.remove("workspace_changes");
    object.remove("check_runs");

    let decoded: RuntimeViewState = serde_json::from_value(encoded).unwrap();

    assert_eq!(decoded.workspace_source, None);
    assert!(decoded.runtime_services.is_empty());
    assert!(decoded.workspace_changes.is_empty());
    assert!(decoded.check_runs.is_empty());
}

#[test]
fn d1_owner_bound_change_and_check_events_reject_mismatched_envelope_owner() {
    let payload_owner = RuntimeOwner {
        workspace_id: "workspace-d1".to_string(),
        project_id: "project-d1".to_string(),
        lane_id: Some("lane-d1".to_string()),
        session_id: Some("session-d1".to_string()),
        ..RuntimeOwner::default()
    };
    let envelope_owner = RuntimeOwner {
        session_id: Some("session-other".to_string()),
        ..payload_owner.clone()
    };
    let events = [
        RuntimeEventKind::WorkspaceChangeUpdated {
            change: WorkspaceChangeView {
                id: "change-d1".to_string(),
                owner: payload_owner.clone(),
                path: "src/lib.rs".to_string(),
                kind: WorkspaceChangeKind::Modified,
                patch: None,
                additions: 1,
                deletions: 0,
                diff: None,
            },
        },
        RuntimeEventKind::CheckRunUpdated {
            check: CheckRunView {
                id: "check-d1".to_string(),
                owner: payload_owner,
                label: "types".to_string(),
                command: "cargo test -p viden-types".to_string(),
                status: CheckRunStatus::Failed,
                summary: "failed".to_string(),
                failing_location: None,
            },
        },
    ];

    for kind in events {
        let envelope = RuntimeEventEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            owner: envelope_owner.clone(),
            cursor: EventCursor {
                stream_id: "fixture:d1-main-cockpit".to_string(),
                sequence: 1,
            },
            event: RuntimeWireEvent::Known(RuntimeEvent::new(1, kind)),
        };
        assert!(serde_json::to_value(envelope).is_err());
    }
}

#[test]
fn runtime_v1_known_event_with_unknown_nested_command_is_preserved() {
    let raw = r#"{
        "sequence": 9,
        "timestamp": 17,
        "kind": {
            "type": "command_accepted",
            "payload": {
                "command_id": "command-future",
                "command": {"type": "future_lane_command", "lane_id": "lane-a"}
            }
        }
    }"#;

    let decoded: RuntimeWireEvent = serde_json::from_str(raw).unwrap();
    assert!(matches!(
        decoded,
        RuntimeWireEvent::Unknown { ref event_type, ref payload }
            if event_type == "command_accepted"
                && payload["command_id"] == "command-future"
    ));
}

#[test]
fn runtime_v1_lane_event_with_unknown_nested_command_is_preserved() {
    let raw = r#"{
        "sequence": 10,
        "timestamp": 18,
        "kind": {
            "type": "lane_command_accepted",
            "payload": {
                "command_id": "command-future-lane",
                "command": {"type": "future_lane_command", "lane_id": "lane-a"}
            }
        }
    }"#;

    let decoded: RuntimeWireEvent = serde_json::from_str(raw).unwrap();
    assert!(matches!(
        decoded,
        RuntimeWireEvent::Unknown { ref event_type, ref payload }
            if event_type == "lane_command_accepted"
                && payload["command_id"] == "command-future-lane"
    ));
}

#[test]
fn approval_response_legacy_bool_decodes_but_structured_serialization_omits_approved() {
    let legacy_allow: ApprovalResponse =
        serde_json::from_str(r#"{"approved":true,"feedback":"ok"}"#).unwrap();
    assert_eq!(
        legacy_allow.decision,
        ApprovalDecision::Allow {
            scope: ApprovalScope::Once
        }
    );
    assert_eq!(legacy_allow.feedback.as_deref(), Some("ok"));

    let legacy_deny: ApprovalResponse = serde_json::from_str(r#"{"approved":false}"#).unwrap();
    assert_eq!(legacy_deny.decision, ApprovalDecision::Deny);

    let encoded = serde_json::to_value(ApprovalResponse::allow_once(None)).unwrap();
    assert_eq!(encoded["decision"]["allow"]["scope"], "once");
    assert!(encoded.get("approved").is_none());
}

#[test]
fn runtime_v1_known_event_sequence_mismatch_is_rejected_at_wire_boundary() {
    let mismatched = RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: RuntimeOwner::default(),
        cursor: EventCursor {
            stream_id: "stream-a".to_string(),
            sequence: 7,
        },
        event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
            8,
            None,
            RuntimeEventKind::InputDequeued {
                input_id: "input-a".to_string(),
            },
        )),
    };
    assert!(serde_json::to_string(&mismatched).is_err());

    let raw_known = r#"{
        "schema_version": 1,
        "owner": {
            "workspace_id": "workspace-a",
            "project_id": "project-a",
            "lane_id": null,
            "session_id": null,
            "task_id": null,
            "turn_id": null
        },
        "cursor": {"stream_id": "stream-a", "sequence": 7},
        "event": {
            "sequence": 8,
            "timestamp": null,
            "kind": {"type": "input_dequeued", "payload": {"input_id": "input-a"}}
        }
    }"#;
    assert!(serde_json::from_str::<RuntimeEventEnvelope>(raw_known).is_err());

    let raw_unknown = raw_known.replace("input_dequeued", "future_event");
    let unknown: RuntimeEventEnvelope = serde_json::from_str(&raw_unknown).unwrap();
    assert!(matches!(unknown.event, RuntimeWireEvent::Unknown { .. }));
}

#[test]
fn event_cursor_classifies_incoming_stream_order() {
    let current = EventCursor {
        stream_id: "stream-a".to_string(),
        sequence: 7,
    };

    assert_eq!(
        current.classify_incoming(&EventCursor {
            stream_id: "stream-a".to_string(),
            sequence: 7,
        }),
        EventCursorOrder::DuplicateOrOld
    );
    assert_eq!(
        current.classify_incoming(&EventCursor {
            stream_id: "stream-a".to_string(),
            sequence: 6,
        }),
        EventCursorOrder::DuplicateOrOld
    );
    assert_eq!(
        current.classify_incoming(&EventCursor {
            stream_id: "stream-a".to_string(),
            sequence: 8,
        }),
        EventCursorOrder::Next
    );
    assert_eq!(
        current.classify_incoming(&EventCursor {
            stream_id: "stream-a".to_string(),
            sequence: 9,
        }),
        EventCursorOrder::Gap
    );
    assert_eq!(
        current.classify_incoming(&EventCursor {
            stream_id: "stream-b".to_string(),
            sequence: 8,
        }),
        EventCursorOrder::StreamMismatch
    );
}

#[test]
fn runtime_commands_and_actions_roundtrip_json_without_ui_state() {
    let action = CommandAction {
        id: "mode.plan".to_string(),
        label: "Plan".to_string(),
        command: RuntimeCommand::SetWorkMode {
            mode: WorkMode::Plan,
        },
        enabled: true,
        disabled_reason: None,
        shortcut: Some("ctrl+p".to_string()),
        destructive: false,
    };

    let encoded = serde_json::to_value(&action).unwrap();
    assert_eq!(encoded["command"]["type"], "set_work_mode");
    assert_eq!(encoded["command"]["mode"], "plan");

    let decoded: CommandAction = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded.command, action.command);
    assert!(decoded.enabled);
}

#[test]
fn agent_dag_runtime_command_roundtrips_json() {
    let commands = vec![
        RuntimeCommand::StartAgentDag {
            goal: "Complete role runtime".to_string(),
            tasks: vec![AgentDagTaskSpec {
                task_id: "task_planner".to_string(),
                role: AgentRole::Planner,
                title: "Plan implementation".to_string(),
                objective: "Split work into safe tasks".to_string(),
                dependencies: Vec::new(),
                workspace: None,
                file_scope: vec!["crates/runtime".to_string()],
                context_bundle_id: Some("ctx_planner".to_string()),
                required_evidence: vec!["plan".to_string()],
                permission_policy: "read_only".to_string(),
            }],
        },
        RuntimeCommand::AcceptMergeGate {
            gate_id: "gate-task_planner".to_string(),
            actor: RuntimeOwner::default(),
            reviewed_evidence: Vec::new(),
            decision: Some("required evidence complete".to_string()),
        },
        RuntimeCommand::RejectMergeGate {
            gate_id: "gate-task_planner".to_string(),
            actor: RuntimeOwner::default(),
            reason: "missing test evidence".to_string(),
        },
        RuntimeCommand::RecordAgentEvidence {
            gate_id: "gate-task_planner".to_string(),
            evidence_id: Some("manual-test_result".to_string()),
            kind: "test_result".to_string(),
            summary: "focused tests passed".to_string(),
            path: Some("target/test.log".to_string()),
            source: Some("tester".to_string()),
            canonical: None,
        },
        RuntimeCommand::AcceptAgentArtifact {
            gate_id: "gate-task_planner".to_string(),
            evidence_id: "evidence-task_planner-plan".to_string(),
            actor: RuntimeOwner::default(),
            source_hash: String::new(),
            decision: Some("artifact evidence accepted".to_string()),
        },
        RuntimeCommand::RejectAgentArtifact {
            gate_id: "gate-task_planner".to_string(),
            evidence_id: "evidence-task_planner-plan".to_string(),
            actor: RuntimeOwner::default(),
            reason: "artifact is stale".to_string(),
        },
        RuntimeCommand::MergeAgentPatch {
            gate_id: "gate-task_planner".to_string(),
            actor: RuntimeOwner::default(),
            decision: Some("merge accepted artifact".to_string()),
        },
    ];

    let encoded = serde_json::to_value(&commands).unwrap();
    assert_eq!(encoded[0]["type"], "start_agent_dag");
    assert_eq!(encoded[0]["tasks"][0]["role"], "planner");
    assert_eq!(encoded[1]["type"], "accept_merge_gate");
    assert_eq!(encoded[2]["type"], "reject_merge_gate");
    assert_eq!(encoded[3]["type"], "record_agent_evidence");
    assert_eq!(encoded[3]["kind"], "test_result");
    assert_eq!(encoded[4]["type"], "accept_agent_artifact");
    assert_eq!(encoded[5]["type"], "reject_agent_artifact");
    assert_eq!(encoded[6]["type"], "merge_agent_patch");

    let decoded: Vec<RuntimeCommand> = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, commands);
}

#[test]
fn context_contracts_round_trip_without_exposing_storage_paths() {
    let handle = ContextHandleRecord {
        handle_id: "ctxh-1".into(),
        item_id: "ctxi-1".into(),
        preferred_view_id: Some("ctxv-1".into()),
        content_sha256: "ab".repeat(32),
        scope: ContextScope::Task("task-1".into()),
        expires_at: None,
    };
    let item = ContextItemRecord {
        item_id: "ctxi-1".into(),
        scope: ContextScope::Dag("dag-1".into()),
        kind: ContextContentKind::Diff,
        content_sha256: "cd".repeat(32),
        title: "runtime contract patch".into(),
        summary: "Diff summary only".into(),
        token_count: 120,
        evidence_id: Some("evidence-1".into()),
        created_at: Some(100),
    };
    let view_record = ContextViewRecord {
        view_id: "ctxv-1".into(),
        item_id: "ctxi-1".into(),
        kind: ContextContentKind::Text,
        derivation: "summary".into(),
        content_sha256: "ef".repeat(32),
        token_count: 24,
        quality_id: Some("ctxq-1".into()),
        created_at: Some(101),
    };
    let retrieval = ContextRetrievalRecord {
        retrieval_id: "ctxr-1".into(),
        handle_id: "ctxh-1".into(),
        item_id: "ctxi-1".into(),
        view_id: Some("ctxv-1".into()),
        scope: ContextScope::Task("task-1".into()),
        byte_count: 256,
        token_count: 64,
        reason_category: "hydrate".into(),
        permission_decision: "allow".into(),
        reason_rule_category: "safe_read".into(),
        reason: "answer follow-up".into(),
        requester: "runtime".into(),
        retrieved_at: Some(102),
    };
    let quality = ContextQualityRecord {
        quality_id: "ctxq-1".into(),
        target_id: "ctxv-1".into(),
        passed: true,
        score_microunits: Some(920_000),
        checks: vec!["sha256_match".into(), "evidence_present".into()],
        failure_reason: None,
        checked_at: Some(103),
    };
    let budget = ContextBudgetRecord {
        budget_id: "ctxb-1".into(),
        scope: ContextScope::Workflow("wf-1".into()),
        soft_token_limit: 8_000,
        hard_token_limit: 16_000,
        used_tokens: 1_200,
        remaining_tokens: 6_800,
        exceeded: false,
        updated_at: Some(104),
    };
    let cost = CostUsageRecord {
        usage_id: "cost-1".into(),
        provider_id: "deepseek".into(),
        model: "deepseek-reasoner".into(),
        scopes: vec![CostScope::AgentTask("task-1".into())],
        tokens: TokenUsage {
            input_tokens: Some(1_000),
            output_tokens: Some(250),
            cached_input_tokens: Some(500),
            retrieval_tokens: Some(0),
            total_tokens: Some(1_250),
        },
        estimate: Some(CostEstimate {
            amount: CostAmount {
                currency: "USD".into(),
                micro_units: 1_234,
            },
            provider_id: "deepseek".into(),
            model: "deepseek-reasoner".into(),
            price_table_version: "test".into(),
            estimated: true,
        }),
        actual_cost: None,
        attempt_index: 0,
        outcome: CostUsageOutcome::Success,
        recorded_at: Some(105),
    };

    let handle_json = serde_json::to_value(&handle).unwrap();
    assert_eq!(handle_json["scope"]["type"], "task");
    assert!(handle_json.get("storage_path").is_none());
    assert_eq!(
        serde_json::from_value::<ContextHandleRecord>(handle_json).unwrap(),
        handle
    );

    let records = serde_json::json!({
        "item": item,
        "view": view_record,
        "retrieval": retrieval,
        "quality": quality,
        "budget": budget,
        "cost": cost,
    });
    assert_eq!(records["item"]["kind"], "diff");
    assert_eq!(records["view"]["kind"], "text");
    assert_eq!(records["retrieval"]["reason"], "answer follow-up");
    assert_eq!(records["retrieval"]["scope"]["type"], "task");
    assert_eq!(records["retrieval"]["byte_count"], 256);
    assert_eq!(records["retrieval"]["reason_category"], "hydrate");
    assert_eq!(records["retrieval"]["permission_decision"], "allow");
    assert_eq!(records["retrieval"]["reason_rule_category"], "safe_read");
    assert_eq!(records["quality"]["score_microunits"], 920_000);
    assert_eq!(records["budget"]["scope"]["type"], "workflow");
    assert_eq!(records["cost"]["actual_cost"], serde_json::Value::Null);
    assert!(
        !serde_json::to_string(&records)
            .unwrap()
            .contains("storage_path")
    );
}

#[test]
fn context_and_cost_runtime_events_project_bounded_summaries() {
    let handle = ContextHandleRecord {
        handle_id: "ctxh-1".into(),
        item_id: "ctxi-1".into(),
        preferred_view_id: Some("ctxv-1".into()),
        content_sha256: "ab".repeat(32),
        scope: ContextScope::Task("task-1".into()),
        expires_at: None,
    };
    let item = ContextItemRecord {
        item_id: "ctxi-1".into(),
        scope: ContextScope::Task("task-1".into()),
        kind: ContextContentKind::Code,
        content_sha256: "cd".repeat(32),
        title: "runtime.rs".into(),
        summary: "Runtime contract definitions".into(),
        token_count: 320,
        evidence_id: None,
        created_at: Some(200),
    };
    let view_record = ContextViewRecord {
        view_id: "ctxv-1".into(),
        item_id: "ctxi-1".into(),
        kind: ContextContentKind::Text,
        derivation: "bounded_summary".into(),
        content_sha256: "ef".repeat(32),
        token_count: 80,
        quality_id: Some("ctxq-1".into()),
        created_at: Some(201),
    };
    let retrieval = ContextRetrievalRecord {
        retrieval_id: "ctxr-1".into(),
        handle_id: "ctxh-1".into(),
        item_id: "ctxi-1".into(),
        view_id: Some("ctxv-1".into()),
        scope: ContextScope::Task("task-1".into()),
        byte_count: 512,
        token_count: 80,
        reason_category: "hydrate".into(),
        permission_decision: "allow".into(),
        reason_rule_category: "safe_read".into(),
        reason: "hydrate evidence".into(),
        requester: "runtime".into(),
        retrieved_at: Some(202),
    };
    let budget = ContextBudgetRecord {
        budget_id: "ctxb-1".into(),
        scope: ContextScope::Task("task-1".into()),
        soft_token_limit: 1_000,
        hard_token_limit: 1_200,
        used_tokens: 1_300,
        remaining_tokens: 0,
        exceeded: true,
        updated_at: Some(203),
    };
    let quality = ContextQualityRecord {
        quality_id: "ctxq-1".into(),
        target_id: "ctxv-1".into(),
        passed: false,
        score_microunits: Some(400_000),
        checks: vec!["missing_canonical_evidence".into()],
        failure_reason: Some("canonical evidence was not attached".into()),
        checked_at: Some(204),
    };
    let cost = CostUsageRecord {
        usage_id: "cost-1".into(),
        provider_id: "deepseek".into(),
        model: "deepseek-reasoner".into(),
        scopes: vec![CostScope::AgentTask("task-1".into())],
        tokens: TokenUsage {
            input_tokens: Some(700),
            output_tokens: Some(300),
            cached_input_tokens: Some(200),
            retrieval_tokens: Some(0),
            total_tokens: Some(1_000),
        },
        estimate: Some(CostEstimate {
            amount: CostAmount {
                currency: "USD".into(),
                micro_units: 900,
            },
            provider_id: "deepseek".into(),
            model: "deepseek-reasoner".into(),
            price_table_version: "test".into(),
            estimated: true,
        }),
        actual_cost: Some(CostAmount {
            currency: "USD".into(),
            micro_units: 950,
        }),
        attempt_index: 0,
        outcome: CostUsageOutcome::Success,
        recorded_at: Some(205),
    };
    let events = vec![
        RuntimeEvent::new(
            1,
            RuntimeEventKind::ContextBundleBuilt {
                bundle_id: "bundle-1".into(),
                scope: ContextScope::Task("task-1".into()),
                handle_ids: vec![handle.handle_id.clone()],
                estimated_tokens: 400,
            },
        ),
        RuntimeEvent::new(2, RuntimeEventKind::ContextItemStored { item }),
        RuntimeEvent::new(
            3,
            RuntimeEventKind::ContextViewDerived {
                view: view_record,
                handle: handle.clone(),
            },
        ),
        RuntimeEvent::new(4, RuntimeEventKind::ContextRetrieved { retrieval }),
        RuntimeEvent::new(5, RuntimeEventKind::ContextBudgetExceeded { budget }),
        RuntimeEvent::new(6, RuntimeEventKind::ContextQualityFailed { quality }),
        RuntimeEvent::new(7, RuntimeEventKind::CostUsageRecorded { cost }),
        RuntimeEvent::new(
            8,
            RuntimeEventKind::ProviderCacheObserved {
                provider_id: "deepseek".into(),
                model: "deepseek-reasoner".into(),
                cached_input_tokens: 200,
                cache_hit_microunits: 160_000,
            },
        ),
        RuntimeEvent::new(
            9,
            RuntimeEventKind::EvidenceCanonicalized {
                evidence_id: "evidence-1".into(),
                item_id: "ctxi-1".into(),
                content_sha256: "cd".repeat(32),
            },
        ),
    ];

    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());
    for event in &events {
        view.apply_event(event);
    }

    assert_eq!(
        serde_json::to_value(&events[0]).unwrap()["kind"]["type"],
        "context_bundle_built"
    );
    assert_eq!(
        serde_json::to_value(RuntimeCommand::RetrieveContext {
            handle_id: handle.handle_id.clone(),
            reason: "answer follow-up".into(),
        })
        .unwrap()["type"],
        "retrieve_context"
    );
    assert_eq!(view.context_handles[0], handle);
    assert_eq!(view.context_items[0].item_id, "ctxi-1");
    assert_eq!(view.context_views[0].view_id, "ctxv-1");
    assert_eq!(view.context_retrievals[0].handle_id, "ctxh-1");
    assert_eq!(view.context_budgets[0].used_tokens, 1_300);
    assert!(!view.context_quality[0].passed);
    assert_eq!(view.cost_ledger.total_tokens, 1_000);
    assert_eq!(view.cost_ledger.total_estimated_cost_micro_usd, 900);
    assert_eq!(view.cost_ledger.total_actual_cost_micro_usd, Some(950));
    assert_eq!(view.provider_cache_observations[0].cached_input_tokens, 200);
    assert_eq!(view.canonical_evidence[0].evidence_id, "evidence-1");

    for index in 0..55 {
        view.apply_event(&RuntimeEvent::new(
            100 + index,
            RuntimeEventKind::ContextRetrieved {
                retrieval: ContextRetrievalRecord {
                    retrieval_id: format!("ctxr-extra-{index}"),
                    handle_id: "ctxh-1".into(),
                    item_id: "ctxi-1".into(),
                    view_id: Some("ctxv-1".into()),
                    scope: ContextScope::Task("task-1".into()),
                    byte_count: 64,
                    token_count: 16,
                    reason_category: "replay".into(),
                    permission_decision: "allow".into(),
                    reason_rule_category: "safe_read".into(),
                    reason: "bounded replay".into(),
                    requester: "runtime".into(),
                    retrieved_at: Some(300 + index),
                },
            },
        ));
    }
    assert_eq!(view.context_retrievals.len(), 50);
    assert_eq!(view.context_retrievals[0].retrieval_id, "ctxr-extra-5");

    let public_json = serde_json::to_string(&events).unwrap()
        + &serde_json::to_string(&view.context_bundles).unwrap()
        + &serde_json::to_string(&view.context_handles).unwrap()
        + &serde_json::to_string(&view.context_items).unwrap()
        + &serde_json::to_string(&view.context_views).unwrap()
        + &serde_json::to_string(&view.context_retrievals).unwrap()
        + &serde_json::to_string(&view.context_budgets).unwrap()
        + &serde_json::to_string(&view.context_quality).unwrap()
        + &serde_json::to_string(&view.cost_usage).unwrap()
        + &serde_json::to_string(&view.provider_cache_observations).unwrap()
        + &serde_json::to_string(&view.canonical_evidence).unwrap();
    assert!(!public_json.contains("storage_path"));
    assert!(!public_json.contains("/tmp/viden"));
}

#[test]
fn runtime_view_state_does_not_double_count_duplicate_cost_usage_id() {
    let cost = CostUsageRecord {
        usage_id: "provider-attempt-1".into(),
        provider_id: "deepseek".into(),
        model: "deepseek-v4-flash".into(),
        scopes: vec![
            CostScope::Request("provider-attempt-1".into()),
            CostScope::AgentTask("task-1".into()),
            CostScope::Workflow("wf-1".into()),
        ],
        tokens: TokenUsage {
            input_tokens: Some(10),
            output_tokens: Some(5),
            cached_input_tokens: Some(3),
            retrieval_tokens: Some(0),
            total_tokens: Some(15),
        },
        estimate: None,
        actual_cost: None,
        attempt_index: 0,
        outcome: CostUsageOutcome::Success,
        recorded_at: Some(500),
    };
    let events = vec![
        RuntimeEvent::new(
            1,
            RuntimeEventKind::CostUsageRecorded { cost: cost.clone() },
        ),
        RuntimeEvent::new(2, RuntimeEventKind::CostUsageRecorded { cost }),
    ];
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());

    for event in &events {
        view.apply_event(event);
    }

    assert_eq!(view.cost_usage.len(), 1);
    assert_eq!(view.cost_ledger.input_tokens, 10);
    assert_eq!(view.cost_ledger.output_tokens, 5);
    assert_eq!(view.cost_ledger.cached_input_tokens, 3);
    assert_eq!(view.cost_ledger.total_tokens, 15);
}

#[test]
fn legacy_flat_cost_usage_events_replay_with_unknown_actual_preserved() {
    let fixture = include_str!("../tests/fixtures/runtime-contract-legacy-cost.json");
    let events: Vec<RuntimeEvent> = serde_json::from_str(fixture).unwrap();
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());

    for event in &events {
        view.apply_event(event);
    }

    assert_eq!(view.cost_usage.len(), 3);
    assert_ne!(view.cost_usage[0].usage_id, view.cost_usage[1].usage_id);
    assert_ne!(view.cost_usage[1].usage_id, view.cost_usage[2].usage_id);
    assert!(view.cost_usage[1].usage_id.contains("legacy-request-2"));
    assert!(view.cost_usage[2].usage_id.contains("legacy-request-3"));
    assert_eq!(view.cost_ledger.input_tokens, 50);
    assert_eq!(view.cost_ledger.output_tokens, 19);
    assert_eq!(view.cost_ledger.cached_input_tokens, 5);
    assert_eq!(view.cost_ledger.retrieval_tokens, 0);
    assert_eq!(view.cost_ledger.total_tokens, 69);
    assert_eq!(view.cost_ledger.total_estimated_cost_micro_usd, 1100);
    assert_eq!(view.cost_ledger.total_actual_cost_micro_usd, None);
    assert_eq!(view.cost_usage[0].attempt_index, 0);
    assert_eq!(view.cost_usage[0].outcome, CostUsageOutcome::Success);
    assert!(
        view.cost_usage[0]
            .scopes
            .contains(&CostScope::AgentTask("legacy-task".into()))
    );
    assert!(
        !view.cost_usage[0]
            .scopes
            .iter()
            .any(|scope| matches!(scope, CostScope::Request(_)))
    );
    assert!(
        view.cost_usage[1]
            .scopes
            .contains(&CostScope::Request("legacy-request-2".into()))
    );
    assert!(
        view.cost_usage[1]
            .scopes
            .contains(&CostScope::Workflow("legacy-workflow".into()))
    );
    assert_eq!(
        view.cost_usage[0]
            .estimate
            .as_ref()
            .unwrap()
            .price_table_version,
        "legacy-flat-cost-v1"
    );

    let serialized = serde_json::to_value(&view.cost_usage[0]).unwrap();
    assert!(serialized.get("scope").is_none());
    assert!(serialized.get("input_tokens").is_none());
    assert!(serialized.get("estimated_cost_micro_usd").is_none());
    assert!(serialized.get("scopes").is_some());
    assert!(serialized.get("tokens").is_some());
}

#[test]
fn legacy_cost_usage_rejects_ambiguous_or_malformed_shapes_without_raw_payload() {
    let ambiguous = serde_json::json!({
        "usage_id": "ambiguous",
        "provider_id": "deepseek",
        "model": "deepseek",
        "scope": {"type": "task", "id": "legacy-task"},
        "scopes": [{"type": "request", "id": "new-request"}],
        "input_tokens": 1,
        "output_tokens": 1,
        "estimated_cost_micro_usd": 1,
        "tokens": {"input_tokens": 1, "output_tokens": 1, "cached_input_tokens": 0, "retrieval_tokens": 0, "total_tokens": 2},
        "recorded_at": 1
    });
    let err = serde_json::from_value::<CostUsageRecord>(ambiguous)
        .expect_err("ambiguous legacy/new shape must be rejected")
        .to_string();
    assert!(err.contains("ambiguous cost usage"));
    assert!(!err.contains("sk-"));
    assert!(!err.contains("legacy-task"));

    let malformed = serde_json::json!({
        "provider_id": "deepseek",
        "model": "deepseek",
        "scope": {"type": "task", "id": "sk-secret-task"},
        "input_tokens": 1
    });
    let err = serde_json::from_value::<CostUsageRecord>(malformed)
        .expect_err("incomplete legacy shape must be rejected")
        .to_string();
    assert!(err.contains("malformed legacy cost usage"));
    assert!(!err.contains("sk-secret-task"));
}

#[test]
fn runtime_events_replay_into_ui_independent_view_state() {
    let snapshot = runtime_snapshot_for_contract();
    let mut view = RuntimeViewState::new(RuntimeSnapshot {
        provider_family: "fallback".to_string(),
        model_label: "test-local".to_string(),
        ..snapshot.clone()
    });

    let approval = ApprovalRequestView {
        id: "approval_1".to_string(),
        tool_name: "shell".to_string(),
        title: "Run cargo test".to_string(),
        message: "Shell command requires approval".to_string(),
        input_preview: "cargo test -p viden-types".to_string(),
        is_mutating: false,
        reason: Some("permission level is ask".to_string()),
        owner: RuntimeOwner::default(),
        risk: ApprovalRisk::Medium,
        target: ApprovalTarget {
            kind: "shell".to_string(),
            display: "cargo test -p viden-types".to_string(),
            canonical_ref: None,
        },
        allowed_scopes: vec![ApprovalScope::Once],
        policy_reason_key: "permission.requires_approval".to_string(),
        policy_reason_args: std::collections::BTreeMap::new(),
        expires_at: 1,
        default_action: ApprovalDefaultAction::Deny,
        audit_id: "audit_1".to_string(),
        decision_context: None,
    };
    let evidence = EvidenceView {
        id: "evidence_1".to_string(),
        kind: "test".to_string(),
        summary: "viden-types tests passed".to_string(),
        path: Some("target/test.log".to_string()),
        source: Some("cargo".to_string()),
        canonical: None,
        metadata: None,
        timestamp: Some(42),
        owner: None,
    };
    let task = AgentTaskRecord {
        id: "task_1".to_string(),
        parent_id: None,
        role: AgentRole::Planner,
        kind: AgentTaskKind::Agent,
        route: AgentRoute::BuiltIn,
        title: "Build runtime contract".to_string(),
        status: AgentTaskStatus::Thinking,
        activity: "designing contract".to_string(),
        summary: "phase 0 contract".to_string(),
        progress: 15,
        started_at: Some(1),
        updated_at: Some(2),
        workspace: Some("/tmp/viden".to_string()),
        evidence: vec![evidence.id.clone()],
        permissions: Vec::new(),
        decision: None,
        result: None,
        resume_handle: None,
        pid: None,
        next_action: None,
        owner: None,
    };
    let queued = QueuedInputView {
        id: "queued_1".to_string(),
        content_preview: "follow-up question".to_string(),
        created_at: Some(43),
        owner: None,
    };

    let events = vec![
        RuntimeEvent::new(
            1,
            RuntimeEventKind::SnapshotUpdated {
                snapshot: snapshot.clone(),
            },
        ),
        RuntimeEvent::new(
            2,
            RuntimeEventKind::AssistantDelta {
                message_id: "msg_1".to_string(),
                task_id: Some(task.id.clone()),
                session_id: None,
                content: "Working on the contract.".to_string(),
            },
        ),
        RuntimeEvent::new(3, RuntimeEventKind::ApprovalRequested { approval }),
        RuntimeEvent::new(4, RuntimeEventKind::EvidenceRecorded { evidence }),
        RuntimeEvent::new(5, RuntimeEventKind::TaskUpdated { task }),
        RuntimeEvent::new(
            6,
            RuntimeEventKind::InputQueued {
                input: queued.clone(),
            },
        ),
        RuntimeEvent::new(
            7,
            RuntimeEventKind::InputDequeued {
                input_id: queued.id.clone(),
            },
        ),
        RuntimeEvent::new(
            8,
            RuntimeEventKind::ApprovalResolved {
                request_id: "approval_1".to_string(),
                decision: ApprovalDecision::Allow {
                    scope: ApprovalScope::Once,
                },
                owner: RuntimeOwner::default(),
                audit_id: "audit_1".to_string(),
            },
        ),
    ];

    for event in &events {
        view.apply_event(event);
    }

    assert_eq!(view.snapshot.provider_family, "deepseek");
    assert_eq!(view.assistant_stream, "Working on the contract.");
    assert!(view.pending_approvals.is_empty());
    assert!(view.queued_inputs.is_empty());
    assert_eq!(view.latest_evidence[0].summary, "viden-types tests passed");
    assert_eq!(view.tasks[0].status_kind(), AgentTaskStatus::Thinking);

    let encoded = serde_json::to_string(&events).unwrap();
    let decoded: Vec<RuntimeEvent> = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, events);
}

#[test]
fn runtime_view_state_replays_agent_dag_and_merge_gate_events() {
    let snapshot = runtime_snapshot_for_contract();
    let dag = AgentDagRecord {
        dag_id: "dag_runtime".to_string(),
        goal: "Coordinate implementation".to_string(),
        status: AgentDagStatus::Active,
        tasks: vec![AgentDagTaskSpec {
            task_id: "task_runtime_planner".to_string(),
            role: AgentRole::Planner,
            title: "Plan work".to_string(),
            objective: "Split implementation".to_string(),
            dependencies: Vec::new(),
            workspace: Some("/tmp/viden".to_string()),
            file_scope: vec!["crates/runtime".to_string()],
            context_bundle_id: Some("ctx_runtime_planner".to_string()),
            required_evidence: vec!["plan".to_string()],
            permission_policy: "read_only".to_string(),
        }],
        created_at: Some(1),
        updated_at: Some(1),
    };
    let gate = MergeGateRecord {
        gate_id: "gate_runtime".to_string(),
        task_id: "task_runtime_planner".to_string(),
        status: MergeGateStatus::Proposed,
        required_evidence: vec!["plan".to_string()],
        evidence_ids: Vec::new(),
        gate_type: MergeGateType::Artifact,
        owner: RuntimeOwner::default(),
        validator: None,
        policy_snapshot: MergeGatePolicySnapshot::default(),
        decision: None,
        conflict: None,
        applied_change_id: None,
        recovery_snapshot: None,
        audit_ids: Vec::new(),
        updated_at: Some(2),
    };
    let mut view = RuntimeViewState::new(snapshot);

    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::AgentDagUpdated { dag: dag.clone() },
    ));
    view.apply_event(&RuntimeEvent::new(
        2,
        RuntimeEventKind::MergeGateUpdated { gate: gate.clone() },
    ));

    assert_eq!(view.agent_dags[0], dag);
    assert_eq!(view.merge_gates[0], gate);
}

#[test]
fn runtime_view_state_sanitizes_context_reduction_records_from_raw_jsonl() {
    let raw = r#"
    {
      "sequence": 1,
      "timestamp": 1,
      "kind": {
        "type": "context_reduction_recorded",
        "payload": {
          "reduction": {
            "reduction_id": "ctxr/../../bad",
            "item_id": "ctxi_/Users/wiki/private/sk-test-secret",
            "view_id": "ctxv_/Users/wiki/private",
            "reducer_id": "adapter/../../sk-test-secret",
            "reducer_version": "../0.1.0",
            "status": "timeout\n/Users/wiki/private",
            "reason": "stderr /Users/wiki/private sk-test-secret ".repeat(10),
            "fallback": true,
            "host_latency_ms": 999999999999,
            "created_at": 1
          }
        }
      }
    }
    "#;
    let raw = raw.replace(
        "\"stderr /Users/wiki/private sk-test-secret \".repeat(10)",
        &format!(
            "{:?}",
            "stderr /Users/wiki/private sk-test-secret ".repeat(10)
        ),
    );
    let event: RuntimeEvent = serde_json::from_str(&raw).unwrap();
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());

    view.apply_event(&event);

    let reduction = view.context_reductions.first().unwrap();
    assert_eq!(
        reduction.reducer_id,
        "adapter_.._.._sk_redacted_test-secret"
    );
    assert_eq!(reduction.reducer_version, ".._0.1.0");
    assert_eq!(reduction.status, "timeout__Users_wiki_private");
    assert!(reduction.reason.as_ref().unwrap().len() <= 160);
    let replayed = serde_json::to_string(&view.context_reductions).unwrap();
    assert!(!replayed.contains("/Users/"));
    assert!(!replayed.contains("sk-test-secret "));
}

#[test]
fn runtime_contract_fixture_replays_phase2_cross_frontend_facts() {
    let fixture = include_str!("../tests/fixtures/runtime-contract-phase2.json");
    let events: Vec<RuntimeEvent> = serde_json::from_str(fixture).unwrap();
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());

    for event in &events {
        view.apply_event(event);
    }

    assert_eq!(view.snapshot.provider_family, "deepseek");
    assert_eq!(view.snapshot.model_label, "deepseek-reasoner");
    assert_eq!(view.snapshot.work_mode, WorkMode::Review);
    assert_eq!(view.provider.as_ref().unwrap().model, "deepseek-reasoner");
    assert_eq!(view.token_cost.as_ref().unwrap().total_tokens, 3456);
    assert_eq!(view.token_cost.as_ref().unwrap().cost_micro_usd, Some(1234));
    assert_eq!(view.context_handles[0].handle_id, "ctxh_runtime_1");
    assert_eq!(view.context_items[0].kind, ContextContentKind::Code);
    assert_eq!(view.context_views[0].derivation, "bounded_summary");
    assert_eq!(view.context_bundles[0].bundle_id, "ctx_bundle_runtime_2");
    assert_eq!(view.context_retrievals[0].retrieval_id, "ctxr_runtime_1");
    assert!(view.context_budgets[0].exceeded);
    assert!(!view.context_quality[0].passed);
    assert_eq!(view.cost_ledger.total_tokens, 3456);
    assert_eq!(view.provider_cache_observations[0].cached_input_tokens, 600);
    assert_eq!(view.canonical_evidence[0].item_id, "ctxi_runtime_1");
    assert!(!fixture.contains("storage_path"));
    assert!(view.pending_approvals.is_empty());
    assert!(view.active_tool_calls.is_empty());
    assert_eq!(view.tasks[0].id, "task_runtime_1");
    assert_eq!(view.lanes[0].id, "lane_runtime_1");
    assert!(view.latest_evidence.iter().any(|evidence| {
        evidence.kind == "test" && evidence.summary.contains("workspace tests passed")
    }));
    assert!(matches!(
        view.last_command.as_ref().map(|receipt| &receipt.command),
        Some(RuntimeCommand::SelectModel { provider_id, model })
            if provider_id == "deepseek" && model == "deepseek-reasoner"
    ));
}

/// GUI-CORE-016: an ACP turn arrives as ordered chunks. The reducer must grow
/// one owner-scoped assistant message so a client can render the reply while
/// it is produced, instead of only seeing the completed paragraph.
#[test]
fn assistant_deltas_grow_one_owner_scoped_conversation_message() {
    let owner = RuntimeOwner {
        workspace_id: "workspace-viden".to_string(),
        project_id: "project-viden".to_string(),
        lane_id: Some("lane-agent".to_string()),
        session_id: Some("agent-session-1".to_string()),
        ..Default::default()
    };
    let session = AgentSessionView {
        session_id: "agent-session-1".to_string(),
        lane_id: "lane-agent".to_string(),
        agent_id: "codex".to_string(),
        model: None,
        status: AgentSessionStatus::Running,
        owner: owner.clone(),
        task: "draw a cat".to_string(),
        diagnostic: None,
        output: None,
    };
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::AgentSessionStarted {
            session: session.clone(),
        },
    ));

    for (sequence, chunk) in ["I will ", "draw ", "a cat."].into_iter().enumerate() {
        view.apply_event(&RuntimeEvent::new(
            sequence as u64 + 2,
            RuntimeEventKind::AssistantDelta {
                message_id: "acp-message-agent-session-1-turn-1".to_string(),
                task_id: Some("acp-session-agent-session-1".to_string()),
                session_id: Some("agent-session-1".to_string()),
                content: chunk.to_string(),
            },
        ));
    }

    let assistant: Vec<&AgentConversationMessageView> = view
        .agent_conversation
        .iter()
        .filter(|message| message.role == AgentConversationRole::Assistant)
        .collect();
    assert_eq!(
        assistant.len(),
        1,
        "chunks of one turn must grow one message, not one message per chunk"
    );
    assert_eq!(assistant[0].content, "I will draw a cat.");
    assert_eq!(assistant[0].session_id, "agent-session-1");
    // The unscoped stream stays intact for existing consumers.
    assert_eq!(view.assistant_stream, "I will draw a cat.");
}

/// A delta Core did not scope to a session cannot be attributed to one.
#[test]
fn an_unscoped_assistant_delta_never_joins_a_session_conversation() {
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::AssistantDelta {
            message_id: "msg_1".to_string(),
            task_id: None,
            session_id: None,
            content: "global stream only".to_string(),
        },
    ));
    assert!(view.agent_conversation.is_empty());
    assert_eq!(view.assistant_stream, "global stream only");
}

/// The completion event must not duplicate a reply the chunks already grew.
#[test]
fn a_completed_session_does_not_duplicate_the_streamed_reply() {
    let owner = RuntimeOwner {
        workspace_id: "workspace-viden".to_string(),
        project_id: "project-viden".to_string(),
        lane_id: Some("lane-agent".to_string()),
        session_id: Some("agent-session-1".to_string()),
        ..Default::default()
    };
    let mut session = AgentSessionView {
        session_id: "agent-session-1".to_string(),
        lane_id: "lane-agent".to_string(),
        agent_id: "codex".to_string(),
        model: None,
        status: AgentSessionStatus::Running,
        owner: owner.clone(),
        task: "draw a cat".to_string(),
        diagnostic: None,
        output: None,
    };
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::AgentSessionStarted {
            session: session.clone(),
        },
    ));
    view.apply_event(&RuntimeEvent::new(
        2,
        RuntimeEventKind::AssistantDelta {
            message_id: "acp-message-agent-session-1-turn-1".to_string(),
            task_id: None,
            session_id: Some("agent-session-1".to_string()),
            content: "here is the cat".to_string(),
        },
    ));
    session.status = AgentSessionStatus::Completed;
    session.output = Some("here is the cat".to_string());
    view.apply_event(&RuntimeEvent::new(
        3,
        RuntimeEventKind::AgentSessionCompleted { session },
    ));

    let assistant = view
        .agent_conversation
        .iter()
        .filter(|message| message.role == AgentConversationRole::Assistant)
        .count();
    assert_eq!(
        assistant, 1,
        "the completed output repeats the streamed text"
    );
}

/// GUI-CORE-017: an ACP turn can return an image. The message must carry the
/// typed part so a client can render it, instead of only prose claiming an
/// image exists.
#[test]
fn a_conversation_message_carries_typed_content_parts() {
    let message = AgentConversationMessageView {
        message_id: "acp-message-session-1-turn-1".to_string(),
        session_id: "session-1".to_string(),
        role: AgentConversationRole::Assistant,
        content: "here is the cat".to_string(),
        parts: vec![
            AgentContentPart::Text {
                text: "here is the cat".to_string(),
            },
            AgentContentPart::Image {
                media_type: "image/png".to_string(),
                reference: "evidence://acp/session-1/cat.png".to_string(),
                alt: Some("an orange cat".to_string()),
            },
        ],
    };

    let wire = serde_json::to_value(&message).unwrap();
    assert_eq!(wire["parts"][1]["type"], "image");
    assert_eq!(wire["parts"][1]["mediaType"], "image/png");
    let decoded: AgentConversationMessageView = serde_json::from_value(wire).unwrap();
    assert_eq!(decoded, message);
}

/// A producer that predates content parts must still decode: `parts` is
/// additive, so an older record keeps its text and reports no parts.
#[test]
fn a_message_without_parts_still_decodes() {
    let legacy = serde_json::json!({
        "message_id": "m1",
        "session_id": "s1",
        "role": "assistant",
        "content": "plain text only"
    });
    let decoded: AgentConversationMessageView = serde_json::from_value(legacy).unwrap();
    assert_eq!(decoded.content, "plain text only");
    assert!(decoded.parts.is_empty());
}

/// A part kind this build does not know must survive the round trip rather
/// than being dropped, so a newer Core never loses content on an older client.
#[test]
fn an_unknown_content_part_is_preserved() {
    let wire = serde_json::json!({
        "message_id": "m1",
        "session_id": "s1",
        "role": "assistant",
        "content": "",
        "parts": [{ "type": "hologram", "payload": { "frames": 3 } }]
    });
    let decoded: AgentConversationMessageView = serde_json::from_value(wire).unwrap();
    assert_eq!(decoded.parts.len(), 1);
    assert!(matches!(
        &decoded.parts[0],
        AgentContentPart::Unknown { kind, .. } if kind == "hologram"
    ));
}

/// GUI-CORE-017: a non-text block an Agent returned reaches the message it
/// belongs to, so a client can render it next to the streamed text.
#[test]
fn an_agent_message_part_attaches_to_its_streamed_message() {
    let owner = RuntimeOwner {
        workspace_id: "workspace-viden".to_string(),
        project_id: "project-viden".to_string(),
        lane_id: Some("lane-agent".to_string()),
        session_id: Some("agent-session-1".to_string()),
        ..Default::default()
    };
    let session = AgentSessionView {
        session_id: "agent-session-1".to_string(),
        lane_id: "lane-agent".to_string(),
        agent_id: "codex".to_string(),
        model: None,
        status: AgentSessionStatus::Running,
        owner,
        task: "draw a cat".to_string(),
        diagnostic: None,
        output: None,
    };
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::AgentSessionStarted { session },
    ));
    view.apply_event(&RuntimeEvent::new(
        2,
        RuntimeEventKind::AssistantDelta {
            message_id: "turn-1".to_string(),
            task_id: None,
            session_id: Some("agent-session-1".to_string()),
            content: "here is the cat".to_string(),
        },
    ));
    view.apply_event(&RuntimeEvent::new(
        3,
        RuntimeEventKind::AgentMessagePart {
            session_id: "agent-session-1".to_string(),
            message_id: "turn-1".to_string(),
            part: AgentContentPart::Image {
                media_type: "image/png".to_string(),
                reference: "evidence://acp/agent-session-1/cat.png".to_string(),
                alt: None,
            },
        },
    ));

    let message = view
        .agent_conversation
        .iter()
        .find(|message| message.message_id == "turn-1")
        .expect("the streamed message");
    assert_eq!(message.content, "here is the cat");
    assert_eq!(message.parts.len(), 1);
    assert!(matches!(
        &message.parts[0],
        AgentContentPart::Image { media_type, .. } if media_type == "image/png"
    ));
}

/// A typed content part must survive the wire, not degrade to an unknown event.
///
/// `RuntimeWireEvent` quarantines an event type it does not recognize, so an
/// `agent_message_part` missing from the known-type set would silently drop
/// every image and file an Agent returned during snapshot/replay — the exact
/// "the agent says it drew something and nothing is there" failure the typed
/// parts exist to prevent (GUI-CORE-017).
#[test]
fn an_agent_message_part_survives_the_wire_as_a_known_event() {
    let envelope = RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: RuntimeOwner {
            workspace_id: "workspace-viden".to_string(),
            project_id: "project-viden".to_string(),
            session_id: Some("agent-session-1".to_string()),
            ..Default::default()
        },
        cursor: EventCursor {
            stream_id: "stream-agent".to_string(),
            sequence: 1,
        },
        event: RuntimeWireEvent::Known(RuntimeEvent::new(
            1,
            RuntimeEventKind::AgentMessagePart {
                session_id: "agent-session-1".to_string(),
                message_id: "turn-1".to_string(),
                part: AgentContentPart::Image {
                    media_type: "image/png".to_string(),
                    reference: ".viden/agents/parts/aa.png".to_string(),
                    alt: None,
                },
            },
        )),
    };
    let encoded = serde_json::to_string(&envelope).unwrap();
    let decoded: RuntimeEventEnvelope = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, envelope);
    assert!(matches!(decoded.event, RuntimeWireEvent::Known(_)));
}

/// An audit page must survive the wire, not degrade to an unknown event.
///
/// Sibling of the `agent_message_part` gap: `audit_page_loaded` was missing
/// from the known schema-1 event types, so every audit page quarantined as
/// `RuntimeWireEvent::Unknown` on any serialized snapshot or replay path
/// (in-process delivery masked it). A quarantined page reads to an operator as
/// "nothing was audited", which is the one failure the append-only timeline
/// exists to prevent (GUI-CORE-024).
#[test]
fn an_audit_page_survives_the_wire_as_a_known_event() {
    let envelope = RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: RuntimeOwner {
            workspace_id: "workspace-viden".to_string(),
            project_id: "project-viden".to_string(),
            ..Default::default()
        },
        cursor: EventCursor {
            stream_id: "stream-audit".to_string(),
            sequence: 1,
        },
        event: RuntimeWireEvent::Known(RuntimeEvent::new(
            1,
            RuntimeEventKind::AuditPageLoaded {
                command_id: Some("client-audit-1".to_string()),
                page: AuditPage {
                    records: vec![
                        AuditRecord::sanitized(
                            "audit-1".to_string(),
                            1_700_000_000,
                            RuntimeOwner {
                                workspace_id: "workspace-viden".to_string(),
                                project_id: "project-viden".to_string(),
                                ..Default::default()
                            },
                            AuditActor::Operator,
                            "gate.decided".to_string(),
                            vec![AuditObjectRef::new(
                                AuditObjectRef::KIND_MERGE_GATE,
                                "gate-1",
                            )],
                            AuditOutcome::Success,
                            BTreeMap::new(),
                        )
                        .expect("the fixture record must satisfy the audit bounds"),
                    ],
                    next_before: None,
                    complete: true,
                },
            },
        )),
    };
    let encoded = serde_json::to_string(&envelope).unwrap();
    let decoded: RuntimeEventEnvelope = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, envelope);
    assert!(matches!(decoded.event, RuntimeWireEvent::Known(_)));
}

/// The page's `command_id` is additive: a page written by a Core that predates
/// it stays a known event and reads back as `None`, never as a fabricated id.
#[test]
fn a_legacy_audit_page_without_a_command_id_stays_known_and_uncorrelated() {
    let legacy = r#"{
        "sequence": 1,
        "timestamp": 1700000000,
        "kind": {
            "type": "audit_page_loaded",
            "payload": {"page": {"records": [], "next_before": null, "complete": true}}
        }
    }"#;
    let decoded: RuntimeEvent = serde_json::from_str(legacy).unwrap();
    let RuntimeEventKind::AuditPageLoaded { command_id, page } = &decoded.kind else {
        panic!("a legacy audit page must stay a known event, got {decoded:?}");
    };
    assert_eq!(*command_id, None);
    assert!(page.complete);

    // `skip_serializing_if` keeps the uncorrelated page byte-identical to what
    // an older Core wrote, so re-encoding never invents the field.
    let re_encoded = serde_json::to_value(&decoded).unwrap();
    assert!(re_encoded["kind"]["payload"].get("command_id").is_none());
}

/// A part for a message Core never published must not fabricate one.
#[test]
fn an_orphan_message_part_is_dropped() {
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::AgentMessagePart {
            session_id: "unknown-session".to_string(),
            message_id: "unknown-message".to_string(),
            part: AgentContentPart::Text {
                text: "orphan".to_string(),
            },
        },
    ));
    assert!(view.agent_conversation.is_empty());
}

#[test]
fn terminal_and_tmux_routes_are_cost_blind_while_managed_routes_are_metered() {
    assert_eq!(
        AgentRoute::BuiltIn.cost_meterability(),
        CostMeterability::Metered
    );
    assert_eq!(
        AgentRoute::Acp.cost_meterability(),
        CostMeterability::Metered
    );
    assert_eq!(
        AgentRoute::Terminal.cost_meterability(),
        CostMeterability::Blind
    );
    assert_eq!(
        AgentRoute::Tmux.cost_meterability(),
        CostMeterability::Blind
    );
    assert_eq!(
        serde_json::to_value(CostMeterability::Metered).unwrap(),
        "metered"
    );
    assert_eq!(
        serde_json::to_value(CostMeterability::Blind).unwrap(),
        "blind"
    );
}

#[test]
fn lane_run_stats_roundtrip_and_stay_absent_for_unobserved_lanes() {
    let mut lane = starter_lane_for_contract("lane_blind");
    lane.route = AgentRoute::Tmux;

    // A lane with no observations must serialize exactly the frozen v1 field
    // set so recorded fixture bytes and digests stay identical.
    let encoded = serde_json::to_value(&lane).unwrap();
    assert!(encoded.get("run_stats").is_none());
    assert_eq!(
        serde_json::from_value::<AgentLaneRecord>(encoded).unwrap(),
        lane
    );

    lane.run_stats = Some(LaneRunStats {
        wall_time_ms: 4_200,
        run_count: 3,
        diff_bytes: 512,
        last_exit_code: Some(0),
    });
    let encoded = serde_json::to_value(&lane).unwrap();
    assert_eq!(
        encoded["run_stats"],
        serde_json::json!({
            "wall_time_ms": 4_200,
            "run_count": 3,
            "diff_bytes": 512,
            "last_exit_code": 0
        })
    );
    assert_eq!(
        serde_json::from_value::<AgentLaneRecord>(encoded).unwrap(),
        lane
    );
}

#[test]
fn legacy_lane_json_without_run_stats_deserializes_as_unobserved() {
    let typed: AgentLaneRecord = serde_json::from_value(serde_json::json!({
        "id": "lane_legacy",
        "task_id": null,
        "role": "coder",
        "route": "terminal",
        "gate_strength": "full",
        "mutation_policy": "propose_only",
        "worktree": null,
        "branch": null,
        "target": "local",
        "data_egress": "deny",
        "status": "draft",
        "budget": {},
        "active_session_ids": [],
        "summary": "legacy lane",
        "evidence": []
    }))
    .unwrap();
    assert_eq!(typed.run_stats, None);

    let migrated: AgentLaneRecord = serde_json::from_value(serde_json::json!({
        "id": "L-legacy",
        "task_id": "task_legacy",
        "agent": "codex",
        "screen": "legacy",
        "transport": "tmux",
        "status": "running",
        "summary": "legacy lane",
        "evidence": []
    }))
    .unwrap();
    assert_eq!(migrated.run_stats, None);
    assert_eq!(migrated.route, AgentRoute::Tmux);
}

#[test]
fn lane_run_stats_default_is_a_measured_zero_distinct_from_absence() {
    let measured = LaneRunStats::default();
    assert_eq!(measured.wall_time_ms, 0);
    assert_eq!(measured.run_count, 0);
    assert_eq!(measured.diff_bytes, 0);
    assert_eq!(measured.last_exit_code, None);

    let lane = starter_lane_for_contract("lane_zero");
    assert_eq!(lane.run_stats, None);
    let observed = AgentLaneRecord {
        run_stats: Some(measured),
        ..lane.clone()
    };
    assert_ne!(observed, lane);
}

fn owner_scoped_live_work_owner() -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace_owner_facts".to_string(),
        project_id: "project_owner_facts".to_string(),
        lane_id: Some("lane_owner_facts".to_string()),
        session_id: Some("session_owner_facts".to_string()),
        task_id: Some("task_owner_facts".to_string()),
        turn_id: Some("turn_owner_facts".to_string()),
    }
}

#[test]
fn live_work_facts_carry_an_optional_runtime_owner() {
    let owner = owner_scoped_live_work_owner();

    let evidence = EvidenceView {
        id: "evidence_owner".to_string(),
        kind: "tool_log".to_string(),
        summary: "owned evidence".to_string(),
        path: None,
        source: Some("acp".to_string()),
        canonical: None,
        metadata: None,
        timestamp: Some(7),
        owner: Some(owner.clone()),
    };
    let tool = ToolCallView {
        tool_call_id: "tool_owner".to_string(),
        name: "shell".to_string(),
        input_preview: "cargo test".to_string(),
        owner: Some(owner.clone()),
    };
    let queued = QueuedInputView {
        id: "queued_owner".to_string(),
        content_preview: "follow-up".to_string(),
        created_at: Some(8),
        owner: Some(owner.clone()),
    };
    let task = AgentTaskRecord {
        id: "task_owner_facts".to_string(),
        parent_id: None,
        role: AgentRole::Coder,
        kind: AgentTaskKind::Job,
        route: AgentRoute::Acp,
        title: "owned job".to_string(),
        status: AgentTaskStatus::Thinking,
        activity: "running".to_string(),
        summary: "owned job".to_string(),
        progress: 10,
        started_at: Some(1),
        updated_at: Some(2),
        workspace: None,
        evidence: Vec::new(),
        permissions: Vec::new(),
        decision: None,
        result: None,
        resume_handle: None,
        pid: None,
        next_action: None,
        owner: Some(owner.clone()),
    };

    for value in [
        serde_json::to_value(&evidence).unwrap(),
        serde_json::to_value(&tool).unwrap(),
        serde_json::to_value(&queued).unwrap(),
        serde_json::to_value(&task).unwrap(),
    ] {
        assert_eq!(
            value.get("owner"),
            Some(&serde_json::to_value(&owner).unwrap())
        );
    }

    assert_eq!(
        serde_json::from_value::<EvidenceView>(serde_json::to_value(&evidence).unwrap()).unwrap(),
        evidence
    );
    assert_eq!(
        serde_json::from_value::<ToolCallView>(serde_json::to_value(&tool).unwrap()).unwrap(),
        tool
    );
    assert_eq!(
        serde_json::from_value::<QueuedInputView>(serde_json::to_value(&queued).unwrap()).unwrap(),
        queued
    );
    assert_eq!(
        serde_json::from_value::<AgentTaskRecord>(serde_json::to_value(&task).unwrap()).unwrap(),
        task
    );
}

#[test]
fn live_work_facts_without_an_owner_stay_byte_identical_on_the_wire() {
    let evidence = EvidenceView {
        id: "evidence_plain".to_string(),
        kind: "system".to_string(),
        summary: "no owner".to_string(),
        path: None,
        source: None,
        canonical: None,
        metadata: None,
        timestamp: None,
        owner: None,
    };
    let tool = ToolCallView {
        tool_call_id: "tool_plain".to_string(),
        name: "shell".to_string(),
        input_preview: "ls".to_string(),
        owner: None,
    };
    let queued = QueuedInputView {
        id: "queued_plain".to_string(),
        content_preview: "later".to_string(),
        created_at: None,
        owner: None,
    };

    for value in [
        serde_json::to_value(&evidence).unwrap(),
        serde_json::to_value(&tool).unwrap(),
        serde_json::to_value(&queued).unwrap(),
    ] {
        assert!(
            value.get("owner").is_none(),
            "an ownerless live-work fact must not add a wire field: {value}"
        );
    }

    // Legacy JSON that predates the field still parses, and absence stays
    // absence rather than becoming a default owner.
    let legacy_tool: ToolCallView = serde_json::from_value(serde_json::json!({
        "tool_call_id": "tool_plain",
        "name": "shell",
        "input_preview": "ls"
    }))
    .unwrap();
    assert_eq!(legacy_tool, tool);

    let legacy_queued: QueuedInputView = serde_json::from_value(serde_json::json!({
        "id": "queued_plain",
        "content_preview": "later",
        "created_at": null
    }))
    .unwrap();
    assert_eq!(legacy_queued, queued);

    let legacy_evidence: EvidenceView = serde_json::from_value(serde_json::json!({
        "id": "evidence_plain",
        "kind": "system",
        "summary": "no owner",
        "path": null,
        "source": null,
        "timestamp": null
    }))
    .unwrap();
    assert_eq!(legacy_evidence, evidence);

    let legacy_task: AgentTaskRecord = serde_json::from_value(serde_json::json!({
        "id": "task_plain",
        "parent_id": null,
        "agent": "codex",
        "kind": "tool",
        "transport": "shell",
        "title": "legacy",
        "status": "thinking",
        "activity": "running",
        "summary": "legacy",
        "progress": 0,
        "started_at": null,
        "updated_at": null,
        "workspace": null,
        "evidence": [],
        "permissions": [],
        "decision": null,
        "result": null,
        "resume_handle": null,
        "pid": null,
        "next_action": null
    }))
    .unwrap();
    assert_eq!(legacy_task.owner, None);
    let reencoded = serde_json::to_value(&legacy_task).unwrap();
    assert!(reencoded.get("owner").is_none());
}

#[test]
fn tool_call_started_publishes_the_owner_onto_the_active_tool_call() {
    let owner = owner_scoped_live_work_owner();
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::ToolCallStarted {
            tool_call_id: "tool_owner".to_string(),
            name: "shell".to_string(),
            input_preview: "cargo test".to_string(),
            owner: Some(owner.clone()),
        },
    ));
    view.apply_event(&RuntimeEvent::new(
        2,
        RuntimeEventKind::ToolCallStarted {
            tool_call_id: "tool_unowned".to_string(),
            name: "shell".to_string(),
            input_preview: "ls".to_string(),
            owner: None,
        },
    ));

    assert_eq!(view.active_tool_calls.len(), 2);
    assert_eq!(view.active_tool_calls[0].owner.as_ref(), Some(&owner));
    assert_eq!(view.active_tool_calls[1].owner, None);

    // A producer that predates the field still round-trips as unowned.
    let legacy: RuntimeEventKind = serde_json::from_value(serde_json::json!({
        "type": "tool_call_started",
        "payload": {
            "tool_call_id": "tool_legacy",
            "name": "shell",
            "input_preview": "ls"
        }
    }))
    .unwrap();
    let RuntimeEventKind::ToolCallStarted {
        owner: legacy_owner,
        ..
    } = &legacy
    else {
        panic!("legacy event must decode as a tool call start");
    };
    assert_eq!(legacy_owner, &None);
    let reencoded = serde_json::to_value(&legacy).unwrap();
    assert!(reencoded["payload"].get("owner").is_none());
}

/// A workspace file page must survive the wire, not degrade to an unknown
/// event.
///
/// Third in the line of the same omission: `agent_message_part` and
/// `audit_page_loaded` each shipped missing from the known schema-1 event
/// types and quarantined as `RuntimeWireEvent::Unknown` on every serialized
/// snapshot or replay path. A quarantined inventory page reads to a client as
/// "this workspace has no files", which is exactly the fabricated-absence
/// failure GUI-CORE-022 exists to prevent.
#[test]
fn a_workspace_file_page_survives_the_wire_as_a_known_event() {
    let envelope = RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: RuntimeOwner {
            workspace_id: "workspace-viden".to_string(),
            project_id: "project-viden".to_string(),
            ..Default::default()
        },
        cursor: EventCursor {
            stream_id: "stream-workspace-files".to_string(),
            sequence: 1,
        },
        event: RuntimeWireEvent::Known(RuntimeEvent::new(
            1,
            RuntimeEventKind::WorkspaceFilesLoaded {
                command_id: "client-files-1".to_string(),
                page: WorkspaceFilePage {
                    entries: vec![
                        WorkspaceFileEntry {
                            path: "crates".to_string(),
                            kind: WorkspaceFileKind::Dir,
                            size_bytes: None,
                        },
                        WorkspaceFileEntry {
                            path: "crates/types/src/lib.rs".to_string(),
                            kind: WorkspaceFileKind::File,
                            size_bytes: Some(2048),
                        },
                    ],
                    next_after: Some("crates/types/src/lib.rs".to_string()),
                    complete: false,
                },
            },
        )),
    };
    let encoded = serde_json::to_string(&envelope).unwrap();
    let decoded: RuntimeEventEnvelope = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, envelope);
    assert!(matches!(decoded.event, RuntimeWireEvent::Known(_)));
}

/// The command id is required from day one: an inventory page with no id is
/// not a page this build can attribute, so it must fail to decode rather than
/// arrive with an id a client would have to invent (the GUI-CORE-024 lesson,
/// applied before the field could ever be optional).
#[test]
fn a_workspace_file_page_without_a_command_id_is_rejected() {
    let idless = r#"{
        "sequence": 1,
        "timestamp": 1700000000,
        "kind": {
            "type": "workspace_files_loaded",
            "payload": {"page": {"entries": [], "next_after": null, "complete": true}}
        }
    }"#;
    let decoded: Result<RuntimeEvent, _> = serde_json::from_str(idless);
    assert!(
        decoded.is_err(),
        "a workspace file page must never decode without the read it answers"
    );
}

/// A workspace inventory page answers one paginated query. Folding it into the
/// capped view collections would silently truncate the tree, so the reducer
/// deliberately ignores it — the same rule an audit page follows.
#[test]
fn a_workspace_file_page_never_folds_into_the_view_state() {
    let snapshot: RuntimeSnapshot = serde_json::from_value(runtime_snapshot_json()).unwrap();
    let before = RuntimeViewState::new(snapshot.clone());
    let mut after = RuntimeViewState::new(snapshot);
    after.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::WorkspaceFilesLoaded {
            command_id: "client-files-1".to_string(),
            page: WorkspaceFilePage {
                entries: vec![WorkspaceFileEntry {
                    path: "README.md".to_string(),
                    kind: WorkspaceFileKind::File,
                    size_bytes: Some(12),
                }],
                next_after: None,
                complete: true,
            },
        },
    ));
    assert_eq!(before, after);
}

/// The page size clamp mirrors the audit precedent: a malformed request still
/// gets a well-formed page rather than a rejection, and an absent limit means
/// the default page rather than the whole tree.
#[test]
fn a_workspace_files_query_clamps_its_limit_and_defaults_when_absent() {
    let unbounded = WorkspaceFilesQuery {
        limit: Some(u32::MAX),
        ..WorkspaceFilesQuery::default()
    };
    assert_eq!(
        unbounded.clamped_limit(),
        MAX_WORKSPACE_FILE_PAGE_SIZE as usize
    );
    let zero = WorkspaceFilesQuery {
        limit: Some(0),
        ..WorkspaceFilesQuery::default()
    };
    assert_eq!(zero.clamped_limit(), 1);
    assert_eq!(
        WorkspaceFilesQuery::default().clamped_limit(),
        DEFAULT_WORKSPACE_FILE_PAGE_SIZE as usize
    );
}

/// `runtime.workspace_files` is a post-checkpoint addition, so it belongs to
/// the extension list and never to the frozen base capabilities.
#[test]
fn the_workspace_files_capability_is_an_advertised_extension() {
    assert!(FRONTEND_V1_EXTENSION_CAPABILITIES.contains(&"runtime.workspace_files"));
    assert!(!FRONTEND_V1_CAPABILITIES.contains(&"runtime.workspace_files"));
    assert!(
        FRONTEND_V1_EXTENSION_CAPABILITIES
            .windows(2)
            .all(|pair| pair[0] < pair[1]),
        "extension capabilities must stay sorted and unique"
    );
}

/// A prefix is a workspace-relative filter, never a path the caller can point
/// outside the workspace with. Rejecting is deliberate: clamping a traversal
/// prefix to the root would answer a question nobody asked, and answering it
/// with an empty page would be indistinguishable from an empty subtree.
#[test]
fn a_workspace_files_query_rejects_a_prefix_that_leaves_the_workspace() {
    let rejected = [
        "../secrets",
        "/etc",
        "crates/../../etc",
        "C:\\Windows",
        "crates\\types",
    ];
    for prefix in rejected {
        let query = WorkspaceFilesQuery {
            prefix: Some(prefix.to_string()),
            ..WorkspaceFilesQuery::default()
        };
        assert!(
            query.validate().is_err(),
            "prefix `{prefix}` must be rejected, not silently reinterpreted"
        );
    }
    for prefix in ["crates", "crates/types/src", "crates/types/src/lib.rs"] {
        let query = WorkspaceFilesQuery {
            prefix: Some(prefix.to_string()),
            ..WorkspaceFilesQuery::default()
        };
        assert!(query.validate().is_ok(), "prefix `{prefix}` must be legal");
    }
}

/// One file's bytes ride the wire as a known event, with every body variant
/// distinguishable after a round trip.
///
/// The three bodies are the point: a client that could not tell text from
/// binary from absent would render an empty editor over all three, which is
/// exactly the fabricated content this capability exists to prevent.
#[test]
fn a_workspace_file_read_answer_survives_the_wire_with_every_body_variant() {
    let bodies = [
        WorkspaceFileBody::Text {
            text: "pub fn main() {}\n".to_string(),
            truncated: false,
        },
        WorkspaceFileBody::Text {
            text: "pub fn main".to_string(),
            truncated: true,
        },
        WorkspaceFileBody::Binary,
        WorkspaceFileBody::Unavailable {
            reason: WorkspaceFileUnavailableReason::NotFound,
        },
        WorkspaceFileBody::Unavailable {
            reason: WorkspaceFileUnavailableReason::Directory,
        },
        WorkspaceFileBody::Unavailable {
            reason: WorkspaceFileUnavailableReason::Unreadable,
        },
    ];
    for body in bodies {
        let unavailable = matches!(body, WorkspaceFileBody::Unavailable { .. });
        let envelope = RuntimeEventEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            owner: RuntimeOwner {
                workspace_id: "workspace-viden".to_string(),
                project_id: "project-viden".to_string(),
                ..Default::default()
            },
            cursor: EventCursor {
                stream_id: "stream-workspace-file-reads".to_string(),
                sequence: 1,
            },
            event: RuntimeWireEvent::Known(RuntimeEvent::new(
                1,
                RuntimeEventKind::WorkspaceFileLoaded {
                    command_id: "client-file-1".to_string(),
                    file: WorkspaceFileContent {
                        path: "crates/types/src/lib.rs".to_string(),
                        // Absent for every unavailable case: there is no file
                        // to measure and no bytes to hash, and `0` plus `""`
                        // would render as an empty file rather than as none.
                        size: (!unavailable).then_some(17),
                        sha256: (!unavailable).then(|| "a".repeat(64)),
                        content: body.clone(),
                    },
                },
            )),
        };
        let encoded = serde_json::to_string(&envelope).unwrap();
        let decoded: RuntimeEventEnvelope = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, envelope);
        assert!(matches!(decoded.event, RuntimeWireEvent::Known(_)));
    }
}

/// The body tag names are contract, not an implementation detail: a client
/// matches on them, so a rename is a breaking change rather than a refactor.
#[test]
fn workspace_file_body_serde_names_are_stable() {
    assert_eq!(
        serde_json::to_value(WorkspaceFileBody::Text {
            text: "x".to_string(),
            truncated: true,
        })
        .unwrap(),
        serde_json::json!({"kind": "text", "text": "x", "truncated": true})
    );
    assert_eq!(
        serde_json::to_value(WorkspaceFileBody::Binary).unwrap(),
        serde_json::json!({"kind": "binary"})
    );
    assert_eq!(
        serde_json::to_value(WorkspaceFileBody::Unavailable {
            reason: WorkspaceFileUnavailableReason::Directory,
        })
        .unwrap(),
        serde_json::json!({"kind": "unavailable", "reason": "directory"})
    );
    for (reason, name) in [
        (WorkspaceFileUnavailableReason::NotFound, "not_found"),
        (WorkspaceFileUnavailableReason::Directory, "directory"),
        (WorkspaceFileUnavailableReason::Unreadable, "unreadable"),
    ] {
        assert_eq!(
            serde_json::to_value(reason).unwrap(),
            serde_json::Value::String(name.to_string())
        );
    }
}

/// An unavailable answer omits `size` and `sha256` entirely rather than
/// publishing zero and an empty hash, which a client would render as a real
/// empty file with a real digest.
#[test]
fn an_unavailable_workspace_file_answer_omits_size_and_hash() {
    let encoded = serde_json::to_value(WorkspaceFileContent {
        path: "docs".to_string(),
        size: None,
        sha256: None,
        content: WorkspaceFileBody::Unavailable {
            reason: WorkspaceFileUnavailableReason::Directory,
        },
    })
    .unwrap();
    assert_eq!(
        encoded,
        serde_json::json!({
            "path": "docs",
            "content": {"kind": "unavailable", "reason": "directory"}
        })
    );
}

/// The command id is required from day one, like `WorkspaceDiffLoaded`: an
/// answer with no id is not one this build can attribute, and a client opening
/// two files must never have to guess which answer is which.
#[test]
fn a_workspace_file_answer_without_a_command_id_is_rejected() {
    let idless = r#"{
        "sequence": 1,
        "timestamp": 1700000000,
        "kind": {
            "type": "workspace_file_loaded",
            "payload": {"file": {"path": "README.md", "content": {"kind": "binary"}}}
        }
    }"#;
    let decoded: Result<RuntimeEvent, _> = serde_json::from_str(idless);
    assert!(
        decoded.is_err(),
        "a file answer must never decode without the read it answers"
    );
}

/// One file's bytes answer one read. Folding them into view state would keep
/// an arbitrary blob alive in every snapshot after it and let a stale body
/// outlive the bytes on disk, so the reducer deliberately ignores the event.
#[test]
fn a_workspace_file_answer_never_folds_into_the_view_state() {
    let snapshot: RuntimeSnapshot = serde_json::from_value(runtime_snapshot_json()).unwrap();
    let before = RuntimeViewState::new(snapshot.clone());
    let mut after = RuntimeViewState::new(snapshot);
    after.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::WorkspaceFileLoaded {
            command_id: "client-file-1".to_string(),
            file: WorkspaceFileContent {
                path: "README.md".to_string(),
                size: Some(12),
                sha256: Some("b".repeat(64)),
                content: WorkspaceFileBody::Text {
                    text: "readme bytes".to_string(),
                    truncated: false,
                },
            },
        },
    ));
    assert_eq!(before, after);
}

/// The byte bound is clamped rather than rejected, the diff-read precedent: a
/// caller asking for too many bytes still means something answerable, and an
/// absent bound means the default rather than the whole file.
#[test]
fn a_workspace_file_read_query_clamps_its_byte_limit_and_defaults_when_absent() {
    let unbounded = WorkspaceFileReadQuery {
        path: "README.md".to_string(),
        byte_limit: Some(u32::MAX),
        ..WorkspaceFileReadQuery::default()
    };
    assert_eq!(unbounded.clamped_byte_limit(), MAX_WORKSPACE_FILE_BYTES);
    let zero = WorkspaceFileReadQuery {
        path: "README.md".to_string(),
        byte_limit: Some(0),
        ..WorkspaceFileReadQuery::default()
    };
    assert_eq!(zero.clamped_byte_limit(), 1);
    assert_eq!(
        WorkspaceFileReadQuery {
            path: "README.md".to_string(),
            ..WorkspaceFileReadQuery::default()
        }
        .clamped_byte_limit(),
        DEFAULT_WORKSPACE_FILE_BYTES
    );
    assert_eq!(DEFAULT_WORKSPACE_FILE_BYTES, 256 * 1024);
    assert_eq!(MAX_WORKSPACE_FILE_BYTES, 1024 * 1024);
}

/// A path is refused, never repaired.
///
/// Normalizing `../../etc/passwd` to `etc/passwd` would serve a different file
/// than the one asked for under the asked-for name, and answering `not_found`
/// would claim the operator's own tree lacks a file that may well exist. Only
/// a stated refusal is honest about either.
#[test]
fn a_workspace_file_read_query_rejects_a_path_that_cannot_mean_what_it_says() {
    let rejected = [
        "",
        "   ",
        "/etc/passwd",
        "\\server\\share",
        "C:\\Windows\\win.ini",
        "../secrets",
        "crates/../../etc/passwd",
        "crates\\types\\src\\lib.rs",
        "crates/types/\u{0}lib.rs",
        "crates/\u{7}types",
        ".",
        "./",
        "././.",
    ];
    for path in rejected {
        let query = WorkspaceFileReadQuery {
            path: path.to_string(),
            ..WorkspaceFileReadQuery::default()
        };
        assert!(
            query.validate().is_err(),
            "path `{path}` must be refused, not silently reinterpreted"
        );
    }
    for path in [
        "README.md",
        "crates/types/src/lib.rs",
        "./crates/types/src/lib.rs",
        "docs/viden-design/Viden/tokens.css",
        "a..b/c",
    ] {
        let query = WorkspaceFileReadQuery {
            path: path.to_string(),
            ..WorkspaceFileReadQuery::default()
        };
        assert!(query.validate().is_ok(), "path `{path}` must be legal");
    }
    // `.` and empty segments are dropped so one file has one spelling in the
    // answer; `..` never reaches this because validation refused it first.
    assert_eq!(
        WorkspaceFileReadQuery {
            path: "./crates//types/src/lib.rs".to_string(),
            ..WorkspaceFileReadQuery::default()
        }
        .normalized_path(),
        "crates/types/src/lib.rs"
    );
}

/// `runtime.workspace_file_reads` is a post-checkpoint addition, so it belongs
/// to the extension list and never to the frozen base capabilities.
#[test]
fn the_workspace_file_reads_capability_is_an_advertised_extension() {
    assert!(FRONTEND_V1_EXTENSION_CAPABILITIES.contains(&"runtime.workspace_file_reads"));
    assert!(!FRONTEND_V1_CAPABILITIES.contains(&"runtime.workspace_file_reads"));
    assert!(
        FRONTEND_V1_EXTENSION_CAPABILITIES
            .windows(2)
            .all(|pair| pair[0] < pair[1]),
        "extension capabilities must stay sorted and unique"
    );
}

/// Builds a session view in one status for the assistant-stream lifecycle tests.
fn stream_lifecycle_session(session_id: &str, status: AgentSessionStatus) -> AgentSessionView {
    AgentSessionView {
        session_id: session_id.to_string(),
        lane_id: "lane-stream".to_string(),
        agent_id: "codex".to_string(),
        model: None,
        status,
        owner: RuntimeOwner {
            workspace_id: "workspace-viden".to_string(),
            project_id: "project-viden".to_string(),
            lane_id: Some("lane-stream".to_string()),
            session_id: Some(session_id.to_string()),
            ..Default::default()
        },
        task: "settle the stream".to_string(),
        diagnostic: None,
        output: Some("the settled reply".to_string()),
    }
}

/// Streams one delta into the unscoped stream, then applies `terminal`.
fn stream_then_terminal(terminal: RuntimeEventKind) -> RuntimeViewState {
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::AgentSessionStarted {
            session: stream_lifecycle_session("agent-session-1", AgentSessionStatus::Running),
        },
    ));
    view.apply_event(&RuntimeEvent::new(
        2,
        RuntimeEventKind::AssistantDelta {
            message_id: "acp-message-agent-session-1-turn-1".to_string(),
            task_id: None,
            session_id: Some("agent-session-1".to_string()),
            content: "in-flight text".to_string(),
        },
    ));
    assert_eq!(
        view.assistant_stream, "in-flight text",
        "the unscoped stream must hold the reply while the turn is in flight"
    );
    view.apply_event(&RuntimeEvent::new(3, terminal));
    view
}

/// A completed turn settles the unscoped stream: the reply is carried onward by
/// the completion fact and the owner-scoped conversation.
#[test]
fn a_completed_agent_session_settles_the_unscoped_assistant_stream() {
    let view = stream_then_terminal(RuntimeEventKind::AgentSessionCompleted {
        session: stream_lifecycle_session("agent-session-1", AgentSessionStatus::Completed),
    });
    assert!(
        view.assistant_stream.is_empty(),
        "a settled turn must not leave its reply in the unscoped stream"
    );
    // The reply survives on the facts a client is meant to read after settlement.
    assert_eq!(
        view.agent_sessions[0].output.as_deref(),
        Some("the settled reply")
    );
    assert!(
        view.agent_conversation.iter().any(|message| {
            message.role == AgentConversationRole::Assistant && message.content == "in-flight text"
        }),
        "the owner-scoped conversation must still carry the streamed reply"
    );
}

/// A failed turn settles the stream too; its diagnostic is the durable fact.
#[test]
fn a_failed_agent_session_settles_the_unscoped_assistant_stream() {
    let view = stream_then_terminal(RuntimeEventKind::AgentSessionFailed {
        session: stream_lifecycle_session("agent-session-1", AgentSessionStatus::Failed),
    });
    assert!(view.assistant_stream.is_empty());
}

/// Cancellation arrives as an update carrying the cancelled status.
#[test]
fn a_cancelled_agent_session_update_settles_the_unscoped_assistant_stream() {
    let view = stream_then_terminal(RuntimeEventKind::AgentSessionUpdated {
        session: stream_lifecycle_session("agent-session-1", AgentSessionStatus::Cancelled),
    });
    assert!(view.assistant_stream.is_empty());
}

/// A non-terminal update is not settlement: the in-flight display must survive.
#[test]
fn a_running_agent_session_update_keeps_the_in_flight_assistant_stream() {
    let view = stream_then_terminal(RuntimeEventKind::AgentSessionUpdated {
        session: stream_lifecycle_session("agent-session-1", AgentSessionStatus::Running),
    });
    assert_eq!(
        view.assistant_stream, "in-flight text",
        "clearing must not eat an in-progress turn"
    );
}

/// Replaying many settled sessions must not concatenate their replies: each
/// session's terminal event ends its own stream segment.
#[test]
fn replayed_settled_sessions_do_not_concatenate_into_one_unattributed_blob() {
    let mut view = RuntimeViewState::new(runtime_snapshot_for_contract());
    let mut sequence = 0u64;
    let mut next = || {
        sequence += 1;
        sequence
    };
    for index in 0..3 {
        let session_id = format!("agent-session-{index}");
        view.apply_event(&RuntimeEvent::new(
            next(),
            RuntimeEventKind::AgentSessionStarted {
                session: stream_lifecycle_session(&session_id, AgentSessionStatus::Running),
            },
        ));
        view.apply_event(&RuntimeEvent::new(
            next(),
            RuntimeEventKind::AssistantDelta {
                message_id: format!("message-{index}"),
                task_id: None,
                session_id: Some(session_id.clone()),
                content: format!("reply {index}. "),
            },
        ));
        view.apply_event(&RuntimeEvent::new(
            next(),
            RuntimeEventKind::AgentSessionCompleted {
                session: stream_lifecycle_session(&session_id, AgentSessionStatus::Completed),
            },
        ));
    }
    assert!(
        view.assistant_stream.is_empty(),
        "replay must not leave a historical blob in the unscoped stream, got {:?}",
        view.assistant_stream
    );
}

/// A `DiffLine` carries the numbers it has and omits the ones it does not.
///
/// A removed line has no line number in the new file and an added line has
/// none in the old one. `None` there means "this line does not exist on that
/// side", so it must serialize as absence and never as `0`, which a client
/// would render as a real line.
#[test]
fn a_diff_line_omits_the_side_it_does_not_exist_on() {
    let removed = DiffLine {
        kind: DiffLineKind::Removed,
        content: "old".to_string(),
        old_line: Some(12),
        new_line: None,
    };
    let encoded = serde_json::to_value(&removed).unwrap();
    assert_eq!(encoded["kind"], "removed");
    assert_eq!(encoded["old_line"], 12);
    assert!(
        encoded.get("new_line").is_none(),
        "a removed line must omit the new-file number instead of publishing 0"
    );
    let decoded: DiffLine = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, removed);
}

/// A `DiffFile` written by a build that predates the optional fields still
/// deserializes: every additive field defaults rather than failing the record.
#[test]
fn a_diff_file_reads_back_from_its_required_fields_alone() {
    let file: DiffFile = serde_json::from_value(serde_json::json!({
        "path": "crates/types/src/diff.rs",
        "kind": "added",
    }))
    .expect("a diff file must default every additive field");
    assert_eq!(file.old_path, None);
    assert!(!file.binary);
    assert!(!file.omitted);
    assert_eq!(file.additions, 0);
    assert_eq!(file.deletions, 0);
    assert!(file.hunks.is_empty());
}

/// An omitted file keeps real counts. The bound drops the hunk rows, not the
/// fact that the file changed, so a client renders "142 additions, not shown"
/// rather than an unchanged file.
#[test]
fn an_omitted_diff_file_keeps_its_real_counts() {
    let file = DiffFile {
        path: "big.rs".to_string(),
        old_path: None,
        kind: WorkspaceChangeKind::Modified,
        binary: false,
        omitted: true,
        additions: 142,
        deletions: 7,
        hunks: Vec::new(),
    };
    let round_trip: DiffFile =
        serde_json::from_str(&serde_json::to_string(&file).unwrap()).unwrap();
    assert_eq!(round_trip, file);
    assert!(round_trip.hunks.is_empty());
    assert_eq!(round_trip.additions, 142);
}

/// `DiffLineKind` is `#[non_exhaustive]`, so a sibling crate matching on it
/// must already carry a wildcard arm and a future kind cannot break it.
#[test]
fn the_diff_line_kind_tags_are_snake_case() {
    for (kind, tag) in [
        (DiffLineKind::Context, "context"),
        (DiffLineKind::Added, "added"),
        (DiffLineKind::Removed, "removed"),
    ] {
        assert_eq!(serde_json::to_value(kind).unwrap(), tag);
    }
}

/// A decision context with no diff is still a fact: it says Core computed no
/// preview, which is different from Core not attaching a context at all.
#[test]
fn a_decision_context_omits_the_fields_core_did_not_know() {
    let empty = DecisionContext {
        diff: None,
        base_sha256: None,
    };
    let encoded = serde_json::to_value(&empty).unwrap();
    assert_eq!(encoded, serde_json::json!({}));
    let decoded: DecisionContext = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, empty);
}

/// The byte limit is clamped rather than rejected, so a malformed client
/// request still gets a well-formed page (the `WorkspaceFilesQuery`
/// precedent).
#[test]
fn a_workspace_diff_query_clamps_its_byte_limit() {
    assert_eq!(
        WorkspaceDiffQuery::default().clamped_byte_limit(),
        DEFAULT_WORKSPACE_DIFF_BYTES
    );
    assert_eq!(
        WorkspaceDiffQuery {
            byte_limit: Some(0),
            ..WorkspaceDiffQuery::default()
        }
        .clamped_byte_limit(),
        1
    );
    assert_eq!(
        WorkspaceDiffQuery {
            byte_limit: Some(u32::MAX),
            ..WorkspaceDiffQuery::default()
        }
        .clamped_byte_limit(),
        MAX_WORKSPACE_DIFF_BYTES
    );
}

/// A path that leaves the target root is rejected rather than clamped or
/// answered with an empty page, exactly as a workspace file prefix is.
#[test]
fn a_workspace_diff_query_rejects_a_path_that_leaves_the_target() {
    for path in ["../secrets", "/etc/passwd", "crates\\types", "a/../../b"] {
        let query = WorkspaceDiffQuery {
            paths: vec![path.to_string()],
            ..WorkspaceDiffQuery::default()
        };
        assert!(
            query.validate().is_err(),
            "`{path}` must be rejected, not clamped"
        );
    }
    assert!(
        WorkspaceDiffQuery {
            paths: vec!["crates/types/src/diff.rs".to_string()],
            ..WorkspaceDiffQuery::default()
        }
        .validate()
        .is_ok()
    );
}

/// The two wire enums the diff read introduces are `#[non_exhaustive]` and
/// snake_case, so a client match keeps a wildcard arm and a future target or
/// scope cannot break a sibling build.
#[test]
fn the_workspace_diff_target_and_scope_tags_are_stable() {
    assert_eq!(
        serde_json::to_value(SourceTarget::Workspace).unwrap(),
        serde_json::json!("workspace")
    );
    assert_eq!(
        serde_json::to_value(SourceTarget::Lane {
            lane_id: "lane_alpha".to_string()
        })
        .unwrap(),
        serde_json::json!({ "lane": { "lane_id": "lane_alpha" } })
    );
    for (scope, tag) in [
        (WorkspaceDiffScope::Worktree, "worktree"),
        (WorkspaceDiffScope::Index, "index"),
        (WorkspaceDiffScope::Both, "both"),
    ] {
        assert_eq!(serde_json::to_value(scope).unwrap(), tag);
    }
}

/// An approval view written before `decision_context` existed still reads, and
/// an approval with no context serializes to exactly the bytes it did before
/// the field was added. That is what keeps the frozen fixture corpus stable.
#[test]
fn an_approval_without_a_decision_context_encodes_as_it_did_before() {
    let approval = ApprovalRequestView {
        id: "approval-1".to_string(),
        tool_name: "edit_file".to_string(),
        title: "Approve edit_file".to_string(),
        message: "edit_file requires approval".to_string(),
        input_preview: "path=src/lib.rs".to_string(),
        is_mutating: true,
        reason: None,
        owner: RuntimeOwner::default(),
        risk: ApprovalRisk::Medium,
        target: ApprovalTarget {
            kind: "edit_file".to_string(),
            display: "src/lib.rs".to_string(),
            canonical_ref: None,
        },
        allowed_scopes: Vec::new(),
        policy_reason_key: String::new(),
        policy_reason_args: BTreeMap::new(),
        expires_at: 0,
        default_action: ApprovalDefaultAction::Deny,
        audit_id: String::new(),
        decision_context: None,
    };
    let encoded = serde_json::to_value(&approval).unwrap();
    assert!(
        encoded.get("decision_context").is_none(),
        "an absent decision context must not appear on the wire"
    );
    let decoded: ApprovalRequestView = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, approval);
}

/// A workspace change without a structured diff encodes exactly as it did
/// before the field existed, and `patch` keeps its meaning for base clients.
#[test]
fn a_workspace_change_without_a_structured_diff_encodes_as_it_did_before() {
    let change = WorkspaceChangeView {
        id: "call-1:src/lib.rs".to_string(),
        owner: RuntimeOwner::default(),
        path: "src/lib.rs".to_string(),
        kind: WorkspaceChangeKind::Modified,
        patch: Some("--- before\n+++ after\n".to_string()),
        additions: 1,
        deletions: 0,
        diff: None,
    };
    let encoded = serde_json::to_value(&change).unwrap();
    assert!(encoded.get("diff").is_none());
    let decoded: WorkspaceChangeView = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, change);
}

/// `runtime.structured_diff` is a post-checkpoint addition, so it belongs to
/// the extension list and never to the frozen base capabilities.
#[test]
fn the_structured_diff_capability_is_an_advertised_extension() {
    assert!(FRONTEND_V1_EXTENSION_CAPABILITIES.contains(&"runtime.structured_diff"));
    assert!(!FRONTEND_V1_CAPABILITIES.contains(&"runtime.structured_diff"));
    assert!(
        FRONTEND_V1_EXTENSION_CAPABILITIES
            .windows(2)
            .all(|pair| pair[0] < pair[1]),
        "extension capabilities must stay sorted and unique"
    );
}

/// The finishing event must survive the wire as a *known* event.
///
/// Quarantining it as `RuntimeWireEvent::Unknown` would leave a client that
/// sent a `Push` with no answer at all on any serialized snapshot or replay
/// path, and "no answer" is the one state an operator reads as "it worked".
#[test]
fn a_finished_operator_git_action_survives_the_wire_as_a_known_event() {
    let envelope = RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: RuntimeOwner {
            workspace_id: "workspace-viden".to_string(),
            project_id: "project-viden".to_string(),
            ..Default::default()
        },
        cursor: EventCursor {
            stream_id: "stream-operator-git".to_string(),
            sequence: 1,
        },
        event: RuntimeWireEvent::Known(RuntimeEvent::new(
            1,
            RuntimeEventKind::OperatorGitActionFinished {
                command_id: "cmd_operator_git".to_string(),
                target: SourceTarget::Workspace,
                action: OperatorGitAction::Push {
                    remote: Some("origin".to_string()),
                    set_upstream: true,
                },
                outcome: OperatorGitOutcome::Failed {
                    class: OperatorGitFailureClass::NoUpstream,
                    detail: "fatal: The current branch work has no upstream branch.".to_string(),
                },
                audit_id: "audit_operator_git".to_string(),
            },
        )),
    };
    let encoded = serde_json::to_string(&envelope).unwrap();
    let decoded: RuntimeEventEnvelope = serde_json::from_str(&encoded).unwrap();
    assert!(matches!(decoded.event, RuntimeWireEvent::Known(_)));
    assert_eq!(decoded, envelope);
}

/// A commit message must be present and bounded before anything runs.
///
/// An empty message makes `git commit` open an editor in a non-interactive
/// child process, which hangs rather than fails; an unbounded one puts an
/// arbitrary payload on the command line.
#[test]
fn an_operator_commit_requires_a_bounded_non_empty_message() {
    assert_eq!(
        OperatorGitAction::Commit {
            message: "   ".to_string(),
        }
        .validate()
        .unwrap_err(),
        "operator git commit requires a non-empty message"
    );
    assert!(
        OperatorGitAction::Commit {
            message: "x".repeat(MAX_OPERATOR_COMMIT_MESSAGE_BYTES + 1),
        }
        .validate()
        .unwrap_err()
        .contains("exceeds")
    );
    assert!(
        OperatorGitAction::Commit {
            message: "fix: bound the message".to_string(),
        }
        .validate()
        .is_ok()
    );
}

/// A staged path names something inside the target, and a path that leaves it
/// is refused rather than clamped: clamping would stage a file nobody asked
/// for, and an empty answer would read as "there was nothing to stage".
#[test]
fn an_operator_stage_rejects_paths_that_leave_the_target() {
    for escaping in [
        "../secrets.txt",
        "/etc/passwd",
        "src\\main.rs",
        "a/../../b",
        "",
    ] {
        let error = OperatorGitAction::Stage {
            paths: vec![escaping.to_string()],
        }
        .validate()
        .unwrap_err();
        assert!(
            !error.is_empty(),
            "path `{escaping}` must be refused before any process runs"
        );
    }
    assert!(
        OperatorGitAction::Unstage {
            paths: vec!["crates/types/src/source_control.rs".to_string()],
        }
        .validate()
        .is_ok()
    );
    // Empty paths is not an error: it means every changed path.
    assert!(
        OperatorGitAction::Stage { paths: Vec::new() }
            .validate()
            .is_ok()
    );
}

/// The outcome is typed all the way down, so a client never parses git output.
#[test]
fn an_operator_git_outcome_round_trips_as_typed_facts() {
    let completed = OperatorGitOutcome::Completed {
        output: "[main abc1234] fix: bound the message".to_string(),
        truncated: false,
        source: WorkspaceSourceView {
            status: WorkspaceSourceStatus::Ready,
            branch: Some("main".to_string()),
            worktree: Some("workspace/viden".to_string()),
            ahead: 1,
            behind: 0,
            added: 0,
            deleted: 0,
            dirty: false,
        },
    };
    let encoded = serde_json::to_value(&completed).unwrap();
    assert_eq!(
        serde_json::from_value::<OperatorGitOutcome>(encoded).unwrap(),
        completed
    );
    let failed = OperatorGitOutcome::Failed {
        class: OperatorGitFailureClass::NoUpstream,
        detail: "fatal: The current branch work has no upstream branch.".to_string(),
    };
    let encoded = serde_json::to_value(&failed).unwrap();
    // Externally tagged like `SourceTarget`, so a client matches on the
    // variant key and reads the class as a token rather than parsing prose.
    assert_eq!(encoded["failed"]["class"], serde_json::json!("no_upstream"));
    assert_eq!(
        serde_json::from_value::<OperatorGitOutcome>(encoded).unwrap(),
        failed
    );
}

/// A finished operator action is a query answer, not view state: reducing one
/// must move nothing. The `WorkspaceSourceUpdated` that follows is what a
/// client's source chip reads, and that one *is* reduced.
#[test]
fn a_finished_operator_git_action_never_folds_into_view_state() {
    let mut state = RuntimeViewState::new(runtime_snapshot_for_contract());
    let baseline = serde_json::to_string(&state).unwrap();
    state.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::OperatorGitActionFinished {
            command_id: "cmd_operator_git".to_string(),
            target: SourceTarget::Workspace,
            action: OperatorGitAction::Fetch { remote: None },
            outcome: OperatorGitOutcome::Failed {
                class: OperatorGitFailureClass::RemoteUnreachable,
                detail: "fatal: Could not resolve host: example.invalid".to_string(),
            },
            audit_id: "audit_operator_git".to_string(),
        },
    ));
    assert_eq!(serde_json::to_string(&state).unwrap(), baseline);
}

/// `runtime.operator_git` is a post-checkpoint addition, so it belongs to the
/// extension list and never to the frozen base capabilities.
#[test]
fn the_operator_git_capability_is_an_advertised_extension() {
    assert!(FRONTEND_V1_EXTENSION_CAPABILITIES.contains(&"runtime.operator_git"));
    assert!(!FRONTEND_V1_CAPABILITIES.contains(&"runtime.operator_git"));
    assert!(
        FRONTEND_V1_EXTENSION_CAPABILITIES
            .windows(2)
            .all(|pair| pair[0] < pair[1]),
        "extension capabilities must stay sorted and unique"
    );
}

// ---------------------------------------------------------------------------
// `runtime.conflict_content` (C3, GUI-CORE-015)
// ---------------------------------------------------------------------------

fn conflict_content_sample() -> ConflictContent {
    ConflictContent {
        baseline: ConflictBaseline::Revision {
            sha: "ab".repeat(20),
        },
        files: vec![ConflictFile {
            path: "crates/types/src/lib.rs".to_string(),
            hunks: vec![ConflictHunk {
                ours_start: 12,
                ours: vec!["let value = 2;".to_string()],
                theirs_start: 12,
                theirs: vec!["let value = 3;".to_string()],
                base: Some(vec!["let value = 1;".to_string()]),
                reason: ConflictHunkReason::ContextMismatch,
            }],
            omitted: false,
        }],
        truncated: false,
    }
}

/// The conflict shape is two sides plus the patch preimage, and each side is
/// separately addressable: a client renders `ours` at `ours_start` and
/// `theirs` at `theirs_start` without recomputing either position.
#[test]
fn conflict_content_round_trips_both_sides_and_the_preimage() {
    let content = conflict_content_sample();
    let encoded = serde_json::to_string(&content).unwrap();
    let decoded: ConflictContent = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, content);
    assert!(encoded.contains("\"context_mismatch\""));
    assert!(encoded.contains("\"revision\""));
}

/// `base` absent and `base` empty are different facts: absent means the hunk
/// had no preimage to show at all (a binary file), while empty means the hunk
/// expected an empty region, which is what a creation patch expects.
#[test]
fn conflict_hunk_distinguishes_an_absent_preimage_from_an_empty_one() {
    let absent = ConflictHunk {
        ours_start: 0,
        ours: Vec::new(),
        theirs_start: 0,
        theirs: Vec::new(),
        base: None,
        reason: ConflictHunkReason::Binary,
    };
    let empty = ConflictHunk {
        base: Some(Vec::new()),
        reason: ConflictHunkReason::AlreadyApplied,
        ..absent.clone()
    };
    let absent_json = serde_json::to_string(&absent).unwrap();
    let empty_json = serde_json::to_string(&empty).unwrap();
    assert!(!absent_json.contains("\"base\""), "{absent_json}");
    assert!(empty_json.contains("\"base\":[]"), "{empty_json}");
    assert_ne!(absent_json, empty_json);
}

/// Additive rule: a bounce written before this capability existed encodes to
/// exactly the bytes it did before, and decodes back with `content: None`.
#[test]
fn a_conflict_bounce_without_content_keeps_its_pre_c3_bytes() {
    let mut bounce = ConflictBounce {
        bounce_id: "conflict_1".to_string(),
        gate_id: "gate_1".to_string(),
        task_id: "task_1".to_string(),
        original_lane_id: "lane_1".to_string(),
        owner: RuntimeOwner::default(),
        reason: "patch conflict".to_string(),
        status: ConflictBounceStatus::Pending,
        evidence_ids: Vec::new(),
        baseline_evidence: Vec::new(),
        revalidation_evidence: Vec::new(),
        content: None,
        audit_id: "audit_1".to_string(),
        created_at: 7,
        revalidated_at: None,
    };
    let without = serde_json::to_string(&bounce).unwrap();
    assert!(!without.contains("\"content\""), "{without}");

    let legacy: ConflictBounce = serde_json::from_str(&without).unwrap();
    assert!(legacy.content.is_none());

    bounce.content = Some(conflict_content_sample());
    let with = serde_json::to_string(&bounce).unwrap();
    assert!(with.contains("\"content\""));
    let decoded: ConflictBounce = serde_json::from_str(&with).unwrap();
    assert_eq!(decoded.content, bounce.content);
}

/// The reducer arm: `LaneConflictDetected` carries the content into the view,
/// so a client reads it from `RuntimeViewState` rather than holding the event.
#[test]
fn a_lane_conflict_event_copies_its_content_into_the_view() {
    let mut state = RuntimeViewState::new(runtime_snapshot_for_contract());
    state.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::LaneConflictDetected {
            lane_id: "lane_1".to_string(),
            summary: "patch conflict: expected hunk context was not found".to_string(),
            paths: vec!["crates/types/src/lib.rs".to_string()],
            content: Some(conflict_content_sample()),
        },
    ));
    let conflict = state
        .lane_conflicts
        .first()
        .expect("the lane conflict must reach the view");
    assert_eq!(conflict.content, Some(conflict_content_sample()));

    // A conflict published without content stays `None`: absence is never
    // replaced by a stale earlier payload on the upsert.
    state.apply_event(&RuntimeEvent::new(
        2,
        RuntimeEventKind::LaneConflictDetected {
            lane_id: "lane_1".to_string(),
            summary: "patch conflict".to_string(),
            paths: vec!["crates/types/src/lib.rs".to_string()],
            content: None,
        },
    ));
    assert_eq!(state.lane_conflicts.len(), 1);
    assert!(state.lane_conflicts[0].content.is_none());
}

/// A pre-C3 `lane_conflict_detected` payload has no `content` key at all and
/// must still deserialize as the known event, not as `Unknown`.
#[test]
fn a_pre_c3_lane_conflict_payload_deserializes_with_no_content() {
    let json = r#"{
        "sequence": 4,
        "timestamp": 9,
        "kind": {
            "type": "lane_conflict_detected",
            "payload": {
                "lane_id": "lane_1",
                "summary": "patch conflict",
                "paths": ["a.rs"]
            }
        }
    }"#;
    let event: RuntimeWireEvent = serde_json::from_str(json).unwrap();
    let RuntimeWireEvent::Known(event) = event else {
        panic!("lane_conflict_detected must stay a known event type");
    };
    assert_eq!(
        event.kind,
        RuntimeEventKind::LaneConflictDetected {
            lane_id: "lane_1".to_string(),
            summary: "patch conflict".to_string(),
            paths: vec!["a.rs".to_string()],
            content: None,
        }
    );
}

/// `runtime.conflict_content` is a post-checkpoint addition, so it belongs to
/// the extension list and never to the frozen base capabilities.
#[test]
fn the_conflict_content_capability_is_an_advertised_extension() {
    assert!(FRONTEND_V1_EXTENSION_CAPABILITIES.contains(&"runtime.conflict_content"));
    assert!(!FRONTEND_V1_CAPABILITIES.contains(&"runtime.conflict_content"));
    assert!(
        FRONTEND_V1_EXTENSION_CAPABILITIES
            .windows(2)
            .all(|pair| pair[0] < pair[1]),
        "extension capabilities must stay sorted and unique"
    );
}

fn evidence_reads_view(id: &str, kind: &str, timestamp: Option<u64>) -> EvidenceView {
    EvidenceView {
        id: id.to_string(),
        kind: kind.to_string(),
        summary: format!("{kind} evidence {id}"),
        path: None,
        source: Some("runtime".to_string()),
        canonical: None,
        metadata: None,
        timestamp,
        owner: Some(RuntimeOwner {
            workspace_id: "workspace_contract_v1".to_string(),
            project_id: "project_viden".to_string(),
            lane_id: Some("lane_core".to_string()),
            session_id: None,
            task_id: None,
            turn_id: None,
        }),
    }
}

/// The archive order is total and deterministic even when Core never dated a
/// row: an undated entry is the *oldest* thing Core can honestly say about it,
/// so it sorts first and a forward-paging client meets it before every dated
/// row rather than seeing it appear after newer ones.
#[test]
fn evidence_cursors_order_undated_evidence_first_then_by_timestamp_and_id() {
    let undated = EvidenceCursor::of(&evidence_reads_view("evidence_zulu", "patch", None));
    let old = EvidenceCursor::of(&evidence_reads_view(
        "evidence_alpha",
        "patch",
        Some(1_700_000_100),
    ));
    let same_second_later_id = EvidenceCursor::of(&evidence_reads_view(
        "evidence_bravo",
        "patch",
        Some(1_700_000_100),
    ));
    let newer = EvidenceCursor::of(&evidence_reads_view(
        "evidence_alpha",
        "patch",
        Some(1_700_000_200),
    ));

    assert!(undated < old, "an undated row sorts before every dated row");
    assert!(old < same_second_later_id, "the id breaks a timestamp tie");
    assert!(same_second_later_id < newer);
}

/// The wire cursor is opaque to clients but must round-trip exactly in Core,
/// including for an id that contains the separator and for the undated case,
/// which is a different position from `timestamp = 0`.
#[test]
fn evidence_cursors_round_trip_through_their_opaque_wire_form() {
    for cursor in [
        EvidenceCursor {
            timestamp: None,
            id: "evidence:with:colons".to_string(),
        },
        EvidenceCursor {
            timestamp: Some(0),
            id: "evidence_epoch".to_string(),
        },
        EvidenceCursor {
            timestamp: Some(1_700_000_100),
            id: "evidence_alpha".to_string(),
        },
    ] {
        let encoded = cursor.encode();
        assert_eq!(EvidenceCursor::decode(&encoded), Ok(cursor.clone()));
    }
    assert_ne!(
        EvidenceCursor {
            timestamp: None,
            id: "evidence_alpha".to_string(),
        }
        .encode(),
        EvidenceCursor {
            timestamp: Some(0),
            id: "evidence_alpha".to_string(),
        }
        .encode(),
        "an undated cursor and a `timestamp = 0` cursor are different positions"
    );
    assert!(EvidenceCursor::decode("not-a-cursor").is_err());
    assert!(EvidenceCursor::decode("t:notanumber:evidence_alpha").is_err());
}

/// `limit` is clamped rather than rejected, exactly as `AuditQuery` does, so a
/// malformed client request still receives a well-formed page. Zero is the
/// case that matters: an unclamped zero would answer every read with an empty
/// page, which a client cannot distinguish from an empty archive.
#[test]
fn evidence_query_limits_clamp_into_the_supported_page_range() {
    let limited = |limit: u16| EvidenceQuery {
        limit,
        ..EvidenceQuery::default()
    };
    assert_eq!(limited(0).clamped_limit(), 1);
    assert_eq!(limited(1).clamped_limit(), 1);
    assert_eq!(limited(50).clamped_limit(), 50);
    assert_eq!(
        limited(u16::MAX).clamped_limit(),
        MAX_EVIDENCE_PAGE_SIZE as usize
    );
    assert_eq!(
        EvidenceQuery::default().limit,
        DEFAULT_EVIDENCE_PAGE_SIZE,
        "the default query asks for the documented default page size"
    );
}

/// Owner scoping fails closed on both sides. Evidence Core could not attribute
/// must not be handed to a scoped read, because "this lane produced it" would
/// then be an inference from timing rather than a fact Core recorded.
#[test]
fn evidence_owner_scope_is_a_prefix_match_that_fails_closed_on_unknown_owners() {
    let scoped = |lane: Option<&str>, task: Option<&str>| EvidenceQuery {
        owner: Some(RuntimeOwner {
            workspace_id: "workspace_contract_v1".to_string(),
            project_id: "project_viden".to_string(),
            lane_id: lane.map(str::to_string),
            session_id: None,
            task_id: task.map(str::to_string),
            turn_id: None,
        }),
        ..EvidenceQuery::default()
    };
    let entry = evidence_reads_view("evidence_alpha", "patch", Some(1_700_000_100));

    assert!(scoped(None, None).matches(&entry), "a project prefix keeps");
    assert!(scoped(Some("lane_core"), None).matches(&entry));
    assert!(
        !scoped(Some("lane_other"), None).matches(&entry),
        "a different lane is a different scope"
    );
    assert!(
        !scoped(Some("lane_core"), Some("task_1")).matches(&entry),
        "a narrower scope than the record carries must not match"
    );

    let unowned = EvidenceView {
        owner: None,
        ..entry.clone()
    };
    assert!(
        !scoped(None, None).matches(&unowned),
        "evidence Core could not attribute never satisfies a scoped read"
    );
    assert!(
        EvidenceQuery::default().matches(&unowned),
        "an unscoped read still sees it"
    );
}

/// An empty `kinds` list means every kind. A filter that silently meant
/// "nothing" would answer a client's unfiltered read with a fabricated empty
/// archive.
#[test]
fn evidence_kind_filters_are_exact_and_empty_means_every_kind() {
    let patch = evidence_reads_view("evidence_alpha", "patch", Some(1));
    let test_result = evidence_reads_view("evidence_bravo", "test_result", Some(2));
    let filtered = EvidenceQuery {
        kinds: vec!["patch".to_string()],
        ..EvidenceQuery::default()
    };
    assert!(filtered.matches(&patch));
    assert!(!filtered.matches(&test_result));
    assert!(EvidenceQuery::default().matches(&patch));
    assert!(EvidenceQuery::default().matches(&test_result));
}

/// Both read answers are known event types. Quarantining an evidence page or a
/// content read would read to an operator as "there is no evidence" and "this
/// evidence has no content", two fabricated absences.
#[test]
fn evidence_read_answers_stay_known_event_types() {
    let page = r#"{
        "sequence": 1,
        "timestamp": 1700000100,
        "kind": {
            "type": "evidence_page_loaded",
            "payload": {
                "command_id": "evidence_read_first",
                "page": { "entries": [], "complete": true, "next_after": null }
            }
        }
    }"#;
    let RuntimeWireEvent::Known(page) = serde_json::from_str::<RuntimeWireEvent>(page).unwrap()
    else {
        panic!("evidence_page_loaded must stay a known event type");
    };
    assert_eq!(
        page.kind,
        RuntimeEventKind::EvidencePageLoaded {
            command_id: "evidence_read_first".to_string(),
            page: EvidencePage {
                entries: Vec::new(),
                complete: true,
                next_after: None,
            },
        }
    );

    let content = r#"{
        "sequence": 2,
        "timestamp": 1700000200,
        "kind": {
            "type": "evidence_content_loaded",
            "payload": {
                "command_id": "evidence_content_first",
                "evidence_id": "evidence_alpha",
                "content": { "type": "unavailable", "reason": "summary_only" }
            }
        }
    }"#;
    let RuntimeWireEvent::Known(content) =
        serde_json::from_str::<RuntimeWireEvent>(content).unwrap()
    else {
        panic!("evidence_content_loaded must stay a known event type");
    };
    assert_eq!(
        content.kind,
        RuntimeEventKind::EvidenceContentLoaded {
            command_id: "evidence_content_first".to_string(),
            evidence_id: "evidence_alpha".to_string(),
            content: EvidenceContent::Unavailable {
                reason: EvidenceUnavailableReason::SummaryOnly,
            },
        }
    );
}

/// Both answers are query results, not runtime facts: reducing them must leave
/// the view exactly as it was, so a bounded archive page can never truncate or
/// overwrite the recent-window `latest_evidence` projection beside it.
#[test]
fn evidence_read_answers_are_query_results_and_never_reach_the_view_state() {
    let snapshot: RuntimeSnapshot = serde_json::from_value(runtime_snapshot_json()).unwrap();
    let mut view = RuntimeViewState::new(snapshot);
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::EvidenceRecorded {
            evidence: evidence_reads_view("evidence_recent", "patch", Some(1_700_000_100)),
        },
    ));
    let before = view.clone();

    view.apply_event(&RuntimeEvent::new(
        2,
        RuntimeEventKind::EvidencePageLoaded {
            command_id: "evidence_read_first".to_string(),
            page: EvidencePage {
                entries: vec![evidence_reads_view("evidence_archive", "patch", Some(1))],
                complete: false,
                next_after: Some(
                    EvidenceCursor {
                        timestamp: Some(1),
                        id: "evidence_archive".to_string(),
                    }
                    .encode(),
                ),
            },
        },
    ));
    view.apply_event(&RuntimeEvent::new(
        3,
        RuntimeEventKind::EvidenceContentLoaded {
            command_id: "evidence_content_first".to_string(),
            evidence_id: "evidence_recent".to_string(),
            content: EvidenceContent::Text {
                text: "ok".to_string(),
                truncated: false,
                sha256: "a".repeat(64),
            },
        },
    ));

    assert_eq!(view.latest_evidence, before.latest_evidence);
    assert_eq!(
        view.latest_evidence.len(),
        1,
        "an archive page must not add rows to the recent window"
    );
}

/// `runtime.evidence_reads` is a post-checkpoint addition, so it belongs to the
/// extension list and never to the frozen base capabilities.
#[test]
fn the_evidence_reads_capability_is_an_advertised_extension() {
    assert!(FRONTEND_V1_EXTENSION_CAPABILITIES.contains(&"runtime.evidence_reads"));
    assert!(!FRONTEND_V1_CAPABILITIES.contains(&"runtime.evidence_reads"));
    assert!(
        FRONTEND_V1_EXTENSION_CAPABILITIES
            .windows(2)
            .all(|pair| pair[0] < pair[1]),
        "extension capabilities must stay sorted and unique"
    );
}

// --- runtime.workspace_owner (C5, GUI-CORE-027) ------------------------------

fn workspace_owner_fixture() -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "ws_0123456789abcdef".to_string(),
        project_id: "prj_contract_v1".to_string(),
        lane_id: None,
        session_id: None,
        task_id: None,
        turn_id: None,
    }
}

/// The published workspace owner reaches the view, and a view that never saw
/// one serializes without the field — which is what keeps the nine frozen base
/// fixtures byte-identical.
#[test]
fn a_bound_workspace_owner_reduces_and_is_absent_until_it_is_published() {
    let snapshot: RuntimeSnapshot = serde_json::from_value(runtime_snapshot_json()).unwrap();
    let mut view = RuntimeViewState::new(snapshot);
    assert!(view.workspace_owner.is_none());
    let encoded = serde_json::to_value(&view).unwrap();
    assert!(
        encoded.get("workspace_owner").is_none(),
        "an unpublished workspace owner must not appear on the wire"
    );
    assert!(encoded.get("lane_sources").is_none());
    assert!(encoded.get("layout_preferences").is_none());

    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::WorkspaceRuntimeOwnerBound {
            binding: WorkspaceRuntimeOwnerBinding {
                canonical_root: "workspace/viden".to_string(),
                owner: workspace_owner_fixture(),
                project_id_origin: ProjectIdOrigin::Minted,
            },
        },
    ));

    assert_eq!(view.workspace_owner, Some(workspace_owner_fixture()));
}

/// A workspace owner is the *workspace* scope: a binding that carries a Lane,
/// session, task, or turn is not that scope and is refused rather than
/// normalized, because publishing it would let one Lane's identity become the
/// actor every workspace-target mutation is audited under.
#[test]
fn a_workspace_owner_binding_rejects_a_lane_scoped_owner() {
    let mut owner = workspace_owner_fixture();
    owner.lane_id = Some("lane_a".to_string());
    let binding = WorkspaceRuntimeOwnerBinding {
        canonical_root: "workspace/viden".to_string(),
        owner,
        project_id_origin: ProjectIdOrigin::Existing,
    };
    assert!(binding.validate().is_err());

    let mut empty = workspace_owner_fixture();
    empty.workspace_id = String::new();
    assert!(
        WorkspaceRuntimeOwnerBinding {
            canonical_root: "workspace/viden".to_string(),
            owner: empty,
            project_id_origin: ProjectIdOrigin::Existing,
        }
        .validate()
        .is_err()
    );
}

/// An invalid binding never reaches the view. The reducer is the last place a
/// client can be protected from a payload that claims an authority Core did
/// not publish.
#[test]
fn an_invalid_workspace_owner_binding_is_not_reduced() {
    let snapshot: RuntimeSnapshot = serde_json::from_value(runtime_snapshot_json()).unwrap();
    let mut view = RuntimeViewState::new(snapshot);
    let mut owner = workspace_owner_fixture();
    owner.project_id = String::new();
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::WorkspaceRuntimeOwnerBound {
            binding: WorkspaceRuntimeOwnerBinding {
                canonical_root: "workspace/viden".to_string(),
                owner,
                project_id_origin: ProjectIdOrigin::Minted,
            },
        },
    ));
    assert!(view.workspace_owner.is_none());
}

/// A Lane's own source is keyed by Lane and never collapses into
/// `workspace_source`: the two answer different questions and a client showing
/// one for the other reports a Lane worktree's branch as the workspace's.
#[test]
fn lane_sources_are_keyed_by_lane_and_never_touch_the_workspace_source() {
    let snapshot: RuntimeSnapshot = serde_json::from_value(runtime_snapshot_json()).unwrap();
    let mut view = RuntimeViewState::new(snapshot);
    let source = WorkspaceSourceView {
        status: WorkspaceSourceStatus::Ready,
        branch: Some("codex/lane-a".to_string()),
        worktree: Some("workspace/.worktrees/lane-a".to_string()),
        ahead: 1,
        behind: 0,
        added: 3,
        deleted: 1,
        dirty: true,
    };
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::LaneSourceUpdated {
            lane_id: "lane_a".to_string(),
            source: source.clone(),
        },
    ));

    assert_eq!(view.lane_sources.get("lane_a"), Some(&source));
    assert!(view.workspace_source.is_none());

    let moved = WorkspaceSourceView {
        ahead: 2,
        dirty: false,
        ..source
    };
    view.apply_event(&RuntimeEvent::new(
        2,
        RuntimeEventKind::LaneSourceUpdated {
            lane_id: "lane_a".to_string(),
            source: moved.clone(),
        },
    ));
    assert_eq!(view.lane_sources.len(), 1);
    assert_eq!(view.lane_sources.get("lane_a"), Some(&moved));
}

/// `runtime.workspace_owner` and `ui.layout_preferences` are post-checkpoint
/// additions, so they belong to the extension list and never to the frozen
/// base capabilities.
#[test]
fn the_workspace_owner_and_layout_capabilities_are_advertised_extensions() {
    for capability in ["runtime.workspace_owner", "ui.layout_preferences"] {
        assert!(FRONTEND_V1_EXTENSION_CAPABILITIES.contains(&capability));
        assert!(!FRONTEND_V1_CAPABILITIES.contains(&capability));
    }
    assert!(
        FRONTEND_V1_EXTENSION_CAPABILITIES
            .windows(2)
            .all(|pair| pair[0] < pair[1]),
        "extension capabilities must stay sorted and unique"
    );
}

// --- ui.layout_preferences (C5) ----------------------------------------------

/// The layout record is separate from `UiPreferences` precisely so the
/// resolved preference profile — which every base fixture serializes — cannot
/// move. Reducing a layout update must leave it alone.
#[test]
fn a_layout_preference_update_reduces_without_touching_resolved_ui_preferences() {
    let snapshot: RuntimeSnapshot = serde_json::from_value(runtime_snapshot_json()).unwrap();
    let before = snapshot.ui_preferences.clone();
    let mut view = RuntimeViewState::new(snapshot);
    let preferences = UiLayoutPreferences {
        lane_sidebar_mode: LaneSidebarMode::Floating,
        hidden_statusbar_segments: vec!["cost".to_string()],
    };
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::UiLayoutPreferencesUpdated {
            command_id: Some("layout_set".to_string()),
            preferences: preferences.clone(),
            persisted: true,
            diagnostics: Vec::new(),
        },
    ));

    assert_eq!(view.layout_preferences, Some(preferences));
    assert_eq!(view.ui_preferences, before);
    assert_eq!(view.snapshot.ui_preferences, before);
}

/// The default is *floating*, which is what `D-SIDEBAR` specifies and what the
/// D1 flagship renders; a record Core never published is `None` rather than a
/// fabricated default, so a client can tell "Core has no layout preference for
/// you" from "Core says floating".
#[test]
fn the_default_lane_sidebar_mode_is_floating_and_absence_stays_absent() {
    assert_eq!(LaneSidebarMode::default(), LaneSidebarMode::Floating);
    let defaults = UiLayoutPreferences::default();
    assert_eq!(defaults.lane_sidebar_mode, LaneSidebarMode::Floating);
    assert!(defaults.hidden_statusbar_segments.is_empty());

    // A record with no `lane_sidebar_mode` at all takes the same default, so a
    // payload written before the field existed and a payload that omits it
    // both land on floating rather than on the first declared variant.
    let decoded: UiLayoutPreferences = serde_json::from_str("{}").unwrap();
    assert_eq!(decoded.lane_sidebar_mode, LaneSidebarMode::Floating);

    let snapshot: RuntimeSnapshot = serde_json::from_value(runtime_snapshot_json()).unwrap();
    let view = RuntimeViewState::new(snapshot);
    assert!(view.layout_preferences.is_none());
}

/// The hidden-segment list is bounded and refused over the bound rather than
/// clamped: clamping would silently keep showing a segment the operator asked
/// to hide, and a client would have no way to learn that happened.
#[test]
fn an_over_bound_hidden_segment_list_is_refused_not_clamped() {
    let within = UiLayoutPreferencePatch {
        lane_sidebar_mode: None,
        hidden_statusbar_segments: Some(
            (0..MAX_HIDDEN_STATUSBAR_SEGMENTS)
                .map(|index| format!("segment_{index}"))
                .collect(),
        ),
    };
    assert!(within.validate().is_ok());

    let over = UiLayoutPreferencePatch {
        lane_sidebar_mode: None,
        hidden_statusbar_segments: Some(
            (0..=MAX_HIDDEN_STATUSBAR_SEGMENTS)
                .map(|index| format!("segment_{index}"))
                .collect(),
        ),
    };
    let error = over
        .validate()
        .expect_err("over-bound list must be refused");
    assert!(error.contains(&MAX_HIDDEN_STATUSBAR_SEGMENTS.to_string()));

    let blank = UiLayoutPreferencePatch {
        lane_sidebar_mode: None,
        hidden_statusbar_segments: Some(vec!["  ".to_string()]),
    };
    assert!(blank.validate().is_err());
}

/// A segment name this Core does not know is kept verbatim: the statusbar
/// vocabulary belongs to the client, so Core storing only the names it
/// recognizes would quietly unhide everything a newer client hid.
#[test]
fn an_unknown_statusbar_segment_round_trips_verbatim() {
    let preferences = UiLayoutPreferences {
        lane_sidebar_mode: LaneSidebarMode::Floating,
        hidden_statusbar_segments: vec!["a-future-client-segment".to_string()],
    };
    let encoded = serde_json::to_string(&preferences).unwrap();
    let decoded: UiLayoutPreferences = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, preferences);
    assert_eq!(
        decoded.hidden_statusbar_segments,
        vec!["a-future-client-segment".to_string()]
    );
}

/// Every new event type must be known to the wire decoder. An unknown one is
/// quarantined, and a quarantined `workspace_runtime_owner_bound` reads to a
/// client as "this workspace has no operator identity" — the exact fabricated
/// absence GUI-CORE-027 exists to end.
#[test]
fn the_new_c5_events_are_known_wire_event_types() {
    for kind in [
        RuntimeEventKind::WorkspaceRuntimeOwnerBound {
            binding: WorkspaceRuntimeOwnerBinding {
                canonical_root: "workspace/viden".to_string(),
                owner: workspace_owner_fixture(),
                project_id_origin: ProjectIdOrigin::Existing,
            },
        },
        RuntimeEventKind::LaneSourceUpdated {
            lane_id: "lane_a".to_string(),
            source: WorkspaceSourceView {
                status: WorkspaceSourceStatus::Ready,
                branch: Some("codex/lane-a".to_string()),
                worktree: Some("workspace/.worktrees/lane-a".to_string()),
                ahead: 0,
                behind: 0,
                added: 0,
                deleted: 0,
                dirty: false,
            },
        },
        RuntimeEventKind::UiLayoutPreferencesUpdated {
            command_id: None,
            preferences: UiLayoutPreferences::default(),
            persisted: true,
            diagnostics: Vec::new(),
        },
    ] {
        let event = RuntimeEvent::new(1, kind);
        let encoded = serde_json::to_string(&event).unwrap();
        let decoded: RuntimeWireEvent = serde_json::from_str(&encoded).unwrap();
        match decoded {
            RuntimeWireEvent::Known(known) => assert_eq!(known.kind, event.kind),
            RuntimeWireEvent::Unknown { event_type, .. } => {
                panic!("`{event_type}` must be a known runtime event type")
            }
        }
    }
}

/// The two layout commands round-trip on the wire. A command a client sends
/// and Core cannot decode is a silently dropped preference.
#[test]
fn the_layout_preference_commands_round_trip() {
    for command in [
        RuntimeCommand::SetUiLayoutPreferences {
            patch: UiLayoutPreferencePatch {
                lane_sidebar_mode: Some(LaneSidebarMode::Floating),
                hidden_statusbar_segments: Some(vec!["cost".to_string()]),
            },
        },
        RuntimeCommand::ResetUiLayoutPreferences,
    ] {
        let encoded = serde_json::to_string(&command).unwrap();
        let decoded: RuntimeCommand = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, command);
    }
}

// --- runtime.turn_lifecycle (C6) ---------------------------------------------

fn turn_lifecycle_session_owner() -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "ws_0123456789abcdef".to_string(),
        project_id: "prj_contract_v1".to_string(),
        lane_id: None,
        session_id: None,
        task_id: None,
        turn_id: Some("turn_native_1".to_string()),
    }
}

fn turn_lifecycle_view() -> RuntimeViewState {
    let snapshot: RuntimeSnapshot = serde_json::from_value(runtime_snapshot_json()).unwrap();
    RuntimeViewState::new(snapshot)
}

fn started_turn(turn_id: &str, owner: RuntimeOwner, source: TurnSource) -> RuntimeEvent {
    RuntimeEvent::new(
        1,
        RuntimeEventKind::TurnStarted {
            turn: TurnView {
                turn_id: turn_id.to_string(),
                owner,
                source,
                started_at: 1_700_005_000,
            },
        },
    )
}

fn finished_turn(turn_id: &str, owner: RuntimeOwner, outcome: TurnOutcome) -> RuntimeEvent {
    RuntimeEvent::new(
        2,
        RuntimeEventKind::TurnFinished {
            turn_id: turn_id.to_string(),
            owner,
            outcome,
            finished_at: 1_700_005_010,
        },
    )
}

/// Liveness is a fact, and its absence is the honest answer rather than a
/// field a client has to interpret. A view that never saw a turn serializes
/// without `active_turns` at all, which is what keeps the nine frozen base
/// fixtures byte-identical.
#[test]
fn a_started_turn_reduces_into_active_turns_and_a_finished_one_removes_it() {
    let mut view = turn_lifecycle_view();
    assert!(view.active_turns.is_empty());
    assert!(
        serde_json::to_value(&view)
            .unwrap()
            .get("active_turns")
            .is_none(),
        "an empty turn list must not appear on the wire"
    );

    view.apply_event(&started_turn(
        "turn_native_1",
        turn_lifecycle_session_owner(),
        TurnSource::UserInput,
    ));
    assert_eq!(view.active_turns.len(), 1);
    assert_eq!(view.active_turns[0].turn_id, "turn_native_1");
    assert_eq!(view.active_turns[0].source, TurnSource::UserInput);

    view.apply_event(&finished_turn(
        "turn_native_1",
        turn_lifecycle_session_owner(),
        TurnOutcome::Completed,
    ));
    assert!(view.active_turns.is_empty());
}

/// The snapshot prefix re-lists the turns Core is running now. A client that
/// reconnects mid-turn must not end up counting the same turn twice, because
/// its composer predicate is "is there an entry for my owner", not "how many".
#[test]
fn re_listing_a_running_turn_upserts_rather_than_duplicating() {
    let mut view = turn_lifecycle_view();
    for _ in 0..3 {
        view.apply_event(&started_turn(
            "turn_native_1",
            turn_lifecycle_session_owner(),
            TurnSource::UserInput,
        ));
    }
    assert_eq!(view.active_turns.len(), 1);
}

/// The residue both clients were reading as "work is still running". A
/// session-scoped turn's end settles the unscoped stream, exactly as a
/// terminal Agent-session fact already does.
#[test]
fn a_finished_session_scoped_turn_settles_the_unscoped_assistant_stream() {
    let mut view = turn_lifecycle_view();
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::AssistantDelta {
            message_id: "message_1".to_string(),
            task_id: None,
            session_id: None,
            content: "a finished reply".to_string(),
        },
    ));
    assert!(!view.assistant_stream.is_empty());

    view.apply_event(&finished_turn(
        "turn_native_1",
        turn_lifecycle_session_owner(),
        TurnOutcome::Completed,
    ));
    assert!(view.assistant_stream.is_empty());
}

/// A Lane's turn has an owner-scoped conversation of its own. Clearing the
/// unscoped stream on it would wipe a session-scoped reply still being
/// produced beside it, so the settlement is deliberately scoped.
#[test]
fn a_finished_lane_scoped_turn_leaves_the_unscoped_assistant_stream_alone() {
    let mut view = turn_lifecycle_view();
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::AssistantDelta {
            message_id: "message_1".to_string(),
            task_id: None,
            session_id: None,
            content: "a session-scoped reply".to_string(),
        },
    ));
    let lane_owner = RuntimeOwner {
        lane_id: Some("lane_a".to_string()),
        session_id: Some("agent-session-1".to_string()),
        ..turn_lifecycle_session_owner()
    };
    view.apply_event(&started_turn(
        "turn_lane_1",
        lane_owner.clone(),
        TurnSource::AgentSession {
            session_id: "agent-session-1".to_string(),
        },
    ));
    view.apply_event(&finished_turn(
        "turn_lane_1",
        lane_owner,
        TurnOutcome::Completed,
    ));

    assert_eq!(view.assistant_stream, "a session-scoped reply");
    assert!(view.active_turns.is_empty());
}

/// A failure reason is a projection string carried to every client. It runs
/// through the same sanitizer every other projection string uses and is cut at
/// the bound, so a provider error carrying a home path or a key-shaped token
/// cannot travel through this field.
#[test]
fn a_failed_turn_reason_is_sanitized_and_bounded() {
    let outcome = TurnOutcome::failed(format!(
        "provider rejected sk-live-secret at /Users/operator/repo: {}",
        "x".repeat(MAX_TURN_FAILURE_REASON_CHARS)
    ));
    let TurnOutcome::Failed { reason } = outcome else {
        panic!("failed() must build the failed outcome");
    };
    assert!(!reason.contains("sk-live-secret"));
    assert!(!reason.contains("/Users/"));
    assert_eq!(reason.chars().count(), MAX_TURN_FAILURE_REASON_CHARS);
}

/// Only a completed turn drains the queue behind it. Every producer asks this
/// one question, so it is asked in one place.
#[test]
fn only_a_completed_turn_drains_the_session_queue() {
    assert!(TurnOutcome::Completed.drains_queue());
    assert!(!TurnOutcome::failed("stopped").drains_queue());
    assert!(!TurnOutcome::Cancelled.drains_queue());
}

/// A quarantined turn fact strands a client on the guess this capability
/// replaces: no `turn_started` reads as "nothing is running" while a turn
/// runs, and no `turn_finished` leaves a turn live forever.
#[test]
fn the_turn_lifecycle_events_are_known_wire_events() {
    for kind in [
        RuntimeEventKind::TurnStarted {
            turn: TurnView {
                turn_id: "turn_native_1".to_string(),
                owner: turn_lifecycle_session_owner(),
                source: TurnSource::QueuedInput {
                    input_id: "queued_1".to_string(),
                },
                started_at: 1_700_005_000,
            },
        },
        RuntimeEventKind::TurnFinished {
            turn_id: "turn_native_1".to_string(),
            owner: turn_lifecycle_session_owner(),
            outcome: TurnOutcome::failed("the provider stopped"),
            finished_at: 1_700_005_010,
        },
    ] {
        let event = RuntimeEvent::new(1, kind);
        let encoded = serde_json::to_string(&event).unwrap();
        let decoded: RuntimeWireEvent = serde_json::from_str(&encoded).unwrap();
        match decoded {
            RuntimeWireEvent::Known(known) => assert_eq!(known.kind, event.kind),
            RuntimeWireEvent::Unknown { event_type, .. } => {
                panic!("`{event_type}` must be a known runtime event type")
            }
        }
    }
}

/// The tagged names are the wire contract two clients and a fixture corpus
/// read; renaming one silently retires a fact.
#[test]
fn the_turn_lifecycle_serde_names_are_stable() {
    assert_eq!(
        serde_json::to_value(TurnSource::UserInput).unwrap(),
        serde_json::json!({ "kind": "user_input" })
    );
    assert_eq!(
        serde_json::to_value(TurnSource::QueuedInput {
            input_id: "queued_1".to_string()
        })
        .unwrap(),
        serde_json::json!({ "kind": "queued_input", "input_id": "queued_1" })
    );
    assert_eq!(
        serde_json::to_value(TurnSource::AgentSession {
            session_id: "agent-session-1".to_string()
        })
        .unwrap(),
        serde_json::json!({ "kind": "agent_session", "session_id": "agent-session-1" })
    );
    assert_eq!(
        serde_json::to_value(TurnOutcome::Completed).unwrap(),
        serde_json::json!({ "kind": "completed" })
    );
    assert_eq!(
        serde_json::to_value(TurnOutcome::Cancelled).unwrap(),
        serde_json::json!({ "kind": "cancelled" })
    );
    assert_eq!(
        serde_json::to_value(TurnOutcome::failed("stopped")).unwrap(),
        serde_json::json!({ "kind": "failed", "reason": "stopped" })
    );
}

#[test]
fn the_turn_lifecycle_capability_is_an_advertised_extension() {
    assert!(FRONTEND_V1_EXTENSION_CAPABILITIES.contains(&"runtime.turn_lifecycle"));
    assert!(!FRONTEND_V1_CAPABILITIES.contains(&"runtime.turn_lifecycle"));
    assert!(
        FRONTEND_V1_EXTENSION_CAPABILITIES
            .windows(2)
            .all(|pair| pair[0] < pair[1]),
        "extension capabilities must stay sorted and unique"
    );
}

// ---------------------------------------------------------------------------
// `runtime.durable_work_evidence` (C7, GUI-CORE-028 and E1 defect 4)
// ---------------------------------------------------------------------------

/// The capability adds no type and no event: what it changes is which facts
/// reach the durable archive and the audit log. It is still gated, because a
/// client that reads the evidence archive for applied work needs to know
/// whether this Core writes one at all — before it, a cockpit that paged the
/// archive after a real edit got an empty page and no way to tell that apart
/// from "this session changed nothing".
#[test]
fn the_durable_work_evidence_capability_is_an_advertised_extension() {
    assert!(FRONTEND_V1_EXTENSION_CAPABILITIES.contains(&"runtime.durable_work_evidence"));
    assert!(!FRONTEND_V1_CAPABILITIES.contains(&"runtime.durable_work_evidence"));
    assert!(
        FRONTEND_V1_EXTENSION_CAPABILITIES
            .windows(2)
            .all(|pair| pair[0] < pair[1]),
        "extension capabilities must stay sorted and unique"
    );
}

/// The two facts the capability makes durable are the two the reducer already
/// knew, so a client that gated on it needs no new event vocabulary. This is
/// the guard against the opposite mistake: shipping the durability while a
/// client could not decode the facts it makes durable.
#[test]
fn the_durable_work_evidence_facts_stay_known_wire_events() {
    for kind in [
        RuntimeEventKind::EvidenceRecorded {
            evidence: EvidenceView {
                id: "patch-tool_1".to_string(),
                kind: "patch".to_string(),
                summary: "native session edit: src.txt (+1/-1)".to_string(),
                path: Some("src.txt".to_string()),
                source: Some("native".to_string()),
                canonical: None,
                metadata: None,
                timestamp: Some(1_700_000_000),
                owner: None,
            },
        },
        RuntimeEventKind::EvidenceCanonicalized {
            evidence_id: "patch-tool_1".to_string(),
            item_id: "ctxi_1".to_string(),
            content_sha256: "a".repeat(64),
        },
    ] {
        let encoded = serde_json::to_string(&RuntimeEvent::new(1, kind.clone())).unwrap();
        let decoded = serde_json::from_str::<RuntimeWireEvent>(&encoded).unwrap();
        match decoded {
            RuntimeWireEvent::Known(event) => assert_eq!(event.kind, kind),
            RuntimeWireEvent::Unknown { event_type, .. } => {
                panic!("`{event_type}` must stay a known wire event")
            }
        }
    }
}

/// Builds one owner-scoped transcript row for the paging and scoping tests.
fn transcript_row(
    id: &str,
    owner: RuntimeOwner,
    sequence: u64,
    content: TranscriptRowContent,
) -> OwnedTranscriptRow {
    OwnedTranscriptRow {
        id: id.to_string(),
        owner,
        sequence,
        timestamp: Some(1_700_000_000 + sequence),
        content,
    }
}

fn transcript_rows_owner(lane: Option<&str>, session: Option<&str>) -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "ws_rows".to_string(),
        project_id: "prj_rows".to_string(),
        lane_id: lane.map(ToString::to_string),
        session_id: session.map(ToString::to_string),
        task_id: None,
        turn_id: None,
    }
}

/// Every content variant has a stable tagged wire shape, because a client
/// matches on `kind` and a renamed tag is a broken transcript surface.
#[test]
fn every_transcript_row_content_variant_has_a_stable_tag() {
    let cases = [
        (
            TranscriptRowContent::User {
                text: "ship it".to_string(),
                truncated: false,
            },
            serde_json::json!({"kind": "user", "text": "ship it", "truncated": false}),
        ),
        (
            TranscriptRowContent::Assistant {
                text: "on it".to_string(),
                truncated: true,
                evidence_id: Some("assistant-body-1".to_string()),
            },
            serde_json::json!({
                "kind": "assistant",
                "text": "on it",
                "truncated": true,
                "evidence_id": "assistant-body-1"
            }),
        ),
        (
            TranscriptRowContent::ToolCall {
                call: ToolCallView {
                    tool_call_id: "call-1".to_string(),
                    name: "edit_file".to_string(),
                    input_preview: "path=src/lib.rs".to_string(),
                    owner: None,
                },
            },
            serde_json::json!({
                "kind": "tool_call",
                "call": {
                    "tool_call_id": "call-1",
                    "name": "edit_file",
                    "input_preview": "path=src/lib.rs"
                }
            }),
        ),
        (
            TranscriptRowContent::ToolResult {
                tool_call_id: "call-1".to_string(),
                success: true,
                summary: "1 file changed".to_string(),
                evidence_id: Some("patch-call-1".to_string()),
            },
            serde_json::json!({
                "kind": "tool_result",
                "tool_call_id": "call-1",
                "success": true,
                "summary": "1 file changed",
                "evidence_id": "patch-call-1"
            }),
        ),
        (
            TranscriptRowContent::CheckRun {
                check: CheckRunView {
                    id: "call-2".to_string(),
                    owner: transcript_rows_owner(Some("lane-a"), None),
                    label: "cargo test".to_string(),
                    command: "cargo test -p viden-types".to_string(),
                    status: CheckRunStatus::Passed,
                    summary: "passed".to_string(),
                    failing_location: None,
                },
            },
            serde_json::json!({
                "kind": "check_run",
                "check": {
                    "id": "call-2",
                    "owner": {
                        "workspace_id": "ws_rows",
                        "project_id": "prj_rows",
                        "lane_id": "lane-a",
                        "session_id": null,
                        "task_id": null,
                        "turn_id": null
                    },
                    "label": "cargo test",
                    "command": "cargo test -p viden-types",
                    "status": "passed",
                    "summary": "passed",
                    "failing_location": null
                }
            }),
        ),
        (
            TranscriptRowContent::Permission {
                request_id: "approval-1".to_string(),
                decision: Some(ApprovalDecision::Deny),
                audit_id: "audit-1".to_string(),
            },
            serde_json::json!({
                "kind": "permission",
                "request_id": "approval-1",
                "decision": "deny",
                "audit_id": "audit-1"
            }),
        ),
    ];
    for (content, expected) in cases {
        let encoded = serde_json::to_value(&content).unwrap();
        assert_eq!(encoded, expected, "row content wire shape moved");
        let decoded: TranscriptRowContent = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, content);
    }
}

/// An unknown decision is omitted rather than defaulted: the durable audit row
/// does not keep a scoped allow's payload, and publishing a bare `allow_once`
/// for an `allow_session` would misreport what the operator chose.
#[test]
fn a_permission_row_omits_a_decision_core_did_not_record() {
    let encoded = serde_json::to_value(TranscriptRowContent::Permission {
        request_id: "approval-2".to_string(),
        decision: None,
        audit_id: "audit-2".to_string(),
    })
    .unwrap();
    assert_eq!(
        encoded,
        serde_json::json!({
            "kind": "permission",
            "request_id": "approval-2",
            "audit_id": "audit-2"
        })
    );
}

/// The limit is clamped rather than rejected, the evidence-read precedent: a
/// client asking for too many rows still means something answerable, while an
/// unclamped `0` would answer every read with an empty page a client cannot
/// tell apart from an exhausted transcript.
#[test]
fn a_transcript_rows_query_clamps_its_limit_and_defaults_when_absent() {
    assert_eq!(MAX_TRANSCRIPT_ROWS_PAGE, 200);
    assert_eq!(DEFAULT_TRANSCRIPT_ROWS_PAGE, 50);
    assert_eq!(MAX_TRANSCRIPT_ROW_TEXT_BYTES, 8 * 1024);
    assert_eq!(
        TranscriptRowsQuery {
            limit: Some(u16::MAX),
            ..TranscriptRowsQuery::default()
        }
        .clamped_limit(),
        MAX_TRANSCRIPT_ROWS_PAGE as usize
    );
    assert_eq!(
        TranscriptRowsQuery {
            limit: Some(0),
            ..TranscriptRowsQuery::default()
        }
        .clamped_limit(),
        1
    );
    assert_eq!(
        TranscriptRowsQuery::default().clamped_limit(),
        DEFAULT_TRANSCRIPT_ROWS_PAGE as usize
    );
}

/// A cursor this build did not issue is refused, never reinterpreted. Reading
/// from the newest end would re-deliver a page the client already rendered and
/// reading from the oldest would look like an exhausted conversation.
#[test]
fn a_malformed_transcript_rows_cursor_is_refused() {
    for raw in ["", "7", "t:7:row", "s:row", "s::row", "s:7:", "s:x:row"] {
        let query = TranscriptRowsQuery {
            before: Some(raw.to_string()),
            ..TranscriptRowsQuery::default()
        };
        assert!(
            query.validate().is_err(),
            "cursor `{raw}` must be refused, not silently reinterpreted"
        );
    }
    let cursor = TranscriptRowsCursor {
        sequence: 7,
        id: "row:with:colons".to_string(),
    };
    assert_eq!(cursor.encode(), "s:7:row:with:colons");
    assert_eq!(
        TranscriptRowsCursor::decode(&cursor.encode()).unwrap(),
        cursor
    );
    assert!(
        TranscriptRowsQuery {
            before: Some(cursor.encode()),
            ..TranscriptRowsQuery::default()
        }
        .validate()
        .is_ok()
    );
}

/// Paging walks backwards and a page reads forwards: `before` is exclusive,
/// the page holds the newest rows below it, and `older` reaches the page above.
#[test]
fn transcript_row_paging_walks_backwards_and_tiles_without_repeating() {
    let owner = transcript_rows_owner(None, Some("session-rows"));
    let rows = (1..=5)
        .map(|sequence| {
            transcript_row(
                &format!("row-{sequence}"),
                owner.clone(),
                sequence,
                TranscriptRowContent::User {
                    text: format!("line {sequence}"),
                    truncated: false,
                },
            )
        })
        .collect::<Vec<_>>();

    let newest = transcript_rows_page(
        &rows,
        &TranscriptRowsQuery {
            limit: Some(2),
            ..TranscriptRowsQuery::default()
        },
    );
    assert_eq!(
        newest
            .rows
            .iter()
            .map(|row| row.sequence)
            .collect::<Vec<_>>(),
        vec![4, 5],
        "a page must hold the newest rows, oldest first"
    );
    assert!(!newest.complete);
    let older = newest
        .older
        .clone()
        .expect("an incomplete page has a cursor");

    let middle = transcript_rows_page(
        &rows,
        &TranscriptRowsQuery {
            before: Some(older),
            limit: Some(2),
            ..TranscriptRowsQuery::default()
        },
    );
    assert_eq!(
        middle
            .rows
            .iter()
            .map(|row| row.sequence)
            .collect::<Vec<_>>(),
        vec![2, 3],
        "two adjacent pages must tile without repeating the boundary row"
    );
    assert!(!middle.complete);

    let oldest = transcript_rows_page(
        &rows,
        &TranscriptRowsQuery {
            before: middle.older.clone(),
            limit: Some(2),
            ..TranscriptRowsQuery::default()
        },
    );
    assert_eq!(
        oldest
            .rows
            .iter()
            .map(|row| row.sequence)
            .collect::<Vec<_>>(),
        vec![1]
    );
    assert!(oldest.complete, "no older row matches the query");
    assert!(oldest.older.is_none(), "a complete page carries no cursor");
}

/// The owner scope fails closed both ways, exactly as the evidence archive
/// read does: a Lane-scoped query never sees the session's own rows, and a
/// session-scoped query never sees a Lane's.
#[test]
fn transcript_row_owner_scoping_fails_closed_in_both_directions() {
    let rows = vec![
        transcript_row(
            "row-session",
            transcript_rows_owner(None, Some("session-rows")),
            1,
            TranscriptRowContent::User {
                text: "composer".to_string(),
                truncated: false,
            },
        ),
        transcript_row(
            "row-lane-a",
            transcript_rows_owner(Some("lane-a"), Some("session-lane-a")),
            2,
            TranscriptRowContent::User {
                text: "lane a".to_string(),
                truncated: false,
            },
        ),
        transcript_row(
            "row-lane-b",
            transcript_rows_owner(Some("lane-b"), Some("session-lane-b")),
            3,
            TranscriptRowContent::User {
                text: "lane b".to_string(),
                truncated: false,
            },
        ),
    ];

    for (scope, expected) in [
        (
            transcript_rows_owner(Some("lane-a"), None),
            vec!["row-lane-a"],
        ),
        (
            transcript_rows_owner(Some("lane-b"), None),
            vec!["row-lane-b"],
        ),
        (
            transcript_rows_owner(None, Some("session-rows")),
            vec!["row-session"],
        ),
    ] {
        let page = transcript_rows_page(
            &rows,
            &TranscriptRowsQuery {
                owner: scope.clone(),
                ..TranscriptRowsQuery::default()
            },
        );
        assert_eq!(
            page.rows
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            expected,
            "scope {scope:?} answered with a row it does not own"
        );
    }

    // A query naming a task cannot be satisfied by a row that only knows its
    // Lane: Core never attributed the row to that task.
    let mut task_scope = transcript_rows_owner(Some("lane-a"), None);
    task_scope.task_id = Some("task-1".to_string());
    assert!(
        transcript_rows_page(
            &rows,
            &TranscriptRowsQuery {
                owner: task_scope,
                ..TranscriptRowsQuery::default()
            },
        )
        .rows
        .is_empty()
    );
}

/// Row text is cut on a character boundary and says so. A silent cut would let
/// a reviewer read half an answer as the whole one.
#[test]
fn transcript_row_text_is_bounded_on_a_character_boundary() {
    let (short, truncated) = bound_transcript_row_text("caf\u{e9}");
    assert_eq!(short, "caf\u{e9}");
    assert!(!truncated);

    let limit = MAX_TRANSCRIPT_ROW_TEXT_BYTES as usize;
    // One byte short of the bound, then a two-byte character straddling it.
    let long = format!("{}\u{e9}{}", "a".repeat(limit - 1), "b".repeat(64));
    let (cut, truncated) = bound_transcript_row_text(&long);
    assert!(truncated);
    assert_eq!(cut.len(), limit - 1, "the cut lands on a char boundary");
    assert!(cut.chars().all(|ch| ch == 'a'));
}

/// One transcript rows page answers one read. Folding it into view state would
/// put one owner's conversation into a view every client shares and let a
/// scroll-up move the snapshot digest.
#[test]
fn a_transcript_rows_page_never_folds_into_the_view_state() {
    let snapshot: RuntimeSnapshot = serde_json::from_value(runtime_snapshot_json()).unwrap();
    let before = RuntimeViewState::new(snapshot.clone());
    let mut after = RuntimeViewState::new(snapshot);
    after.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::TranscriptRowsLoaded {
            command_id: "client-rows-1".to_string(),
            page: TranscriptRowsPage {
                rows: vec![transcript_row(
                    "row-1",
                    transcript_rows_owner(None, Some("session-rows")),
                    1,
                    TranscriptRowContent::User {
                        text: "hello".to_string(),
                        truncated: false,
                    },
                )],
                older: None,
                complete: true,
            },
        },
    ));
    assert_eq!(before, after);
}

/// The command id is required from day one, like `WorkspaceFileLoaded`: a page
/// with no id is not one this build can attribute, and a client paging two
/// owners must never guess which answer is which.
#[test]
fn a_transcript_rows_page_without_a_command_id_is_rejected() {
    let idless = r#"{
        "sequence": 1,
        "timestamp": 1700000000,
        "kind": {
            "type": "transcript_rows_loaded",
            "payload": {"page": {"rows": [], "complete": true}}
        }
    }"#;
    let decoded: Result<RuntimeEvent, _> = serde_json::from_str(idless);
    assert!(
        decoded.is_err(),
        "a transcript rows page must never decode without the read it answers"
    );
}

/// `runtime.transcript_rows` is a post-checkpoint addition, so it belongs to
/// the extension list and never to the frozen base capabilities, which still
/// advertise the untouched `runtime.transcript_page`.
#[test]
fn the_transcript_rows_capability_is_an_advertised_extension() {
    assert!(FRONTEND_V1_EXTENSION_CAPABILITIES.contains(&"runtime.transcript_rows"));
    assert!(!FRONTEND_V1_CAPABILITIES.contains(&"runtime.transcript_rows"));
    assert!(FRONTEND_V1_CAPABILITIES.contains(&"runtime.transcript_page"));
    assert!(
        FRONTEND_V1_EXTENSION_CAPABILITIES
            .windows(2)
            .all(|pair| pair[0] < pair[1]),
        "extension capabilities must stay sorted and unique"
    );
    // The milestone's final count: C8 is the last Core batch of 0.3.4.
    assert_eq!(FRONTEND_V1_EXTENSION_CAPABILITIES.len(), 29);
}
