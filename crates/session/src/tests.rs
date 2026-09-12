use super::*;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use viden_types::{
    CommandLogEntry, CostScope, CostUsageOutcome, CostUsageRecord, Message, PermissionLogEntry,
    Role, RuntimeEvent, RuntimeEventKind, SessionMetaEntry, TokenUsage, ToolCall, ToolResult,
    TranscriptCursor, TranscriptEntry, TranscriptPage, TranscriptPageRequest, TranscriptRow,
    TranscriptRowKind,
};

fn temp_home(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("viden_test_{name}_{}", fresh_id("tmp")));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn append_recent_metadata(store: &SessionStore, canonical_root: &Path, timestamp: u64) {
    store
        .append_entries_atomic(&[
            TranscriptEntry::SessionMeta {
                entry: SessionMetaEntry {
                    timestamp,
                    key: "canonical_root".to_string(),
                    value: canonical_root.canonicalize().unwrap().display().to_string(),
                },
            },
            TranscriptEntry::SessionMeta {
                entry: SessionMetaEntry {
                    timestamp,
                    key: "session_created_at".to_string(),
                    value: timestamp.to_string(),
                },
            },
        ])
        .unwrap();
}

fn write_recent_transcript(
    home: &Path,
    canonical_root: &Path,
    session_id: &str,
    created_at: u64,
    last_updated_at: u64,
    body: &str,
    arbitrary_metadata: Option<(&str, &str)>,
) -> PathBuf {
    let canonical_root = canonical_root.canonicalize().unwrap();
    let project_dir = home
        .join("projects")
        .join(project_key_for_path(&canonical_root));
    fs::create_dir_all(&project_dir).unwrap();
    let path = project_dir.join(format!("{session_id}.jsonl"));
    let mut lines = vec![
        TranscriptEntry::SessionMeta {
            entry: SessionMetaEntry {
                timestamp: created_at,
                key: "canonical_root".to_string(),
                value: canonical_root.display().to_string(),
            },
        }
        .to_json_line(),
        TranscriptEntry::SessionMeta {
            entry: SessionMetaEntry {
                timestamp: created_at,
                key: "session_created_at".to_string(),
                value: created_at.to_string(),
            },
        }
        .to_json_line(),
    ];
    if let Some((key, value)) = arbitrary_metadata {
        lines.push(
            TranscriptEntry::SessionMeta {
                entry: SessionMetaEntry {
                    timestamp: created_at,
                    key: key.to_string(),
                    value: value.to_string(),
                },
            }
            .to_json_line(),
        );
    }
    lines.push(
        TranscriptEntry::Message {
            message: message_with_id(
                &format!("message-{session_id}"),
                Role::User,
                body,
                last_updated_at,
            ),
        }
        .to_json_line(),
    );
    fs::write(&path, lines.join("\n") + "\n").unwrap();
    path
}

#[test]
fn recent_work_clamps_zero_and_orders_sessions_before_project_aggregation() {
    let home = temp_home("recent_work_limit_zero");
    let older_root = home.join("a-project");
    let newer_root = home.join("b-project");
    fs::create_dir_all(&older_root).unwrap();
    fs::create_dir_all(&newer_root).unwrap();
    let older_root = older_root.canonicalize().unwrap();
    let newer_root = newer_root.canonicalize().unwrap();
    let older = SessionStore::new_with_home(&home, &older_root, Some("older".into())).unwrap();
    let newer = SessionStore::new_with_home(&home, &newer_root, Some("newer".into())).unwrap();
    append_recent_metadata(&older, &older_root, 1);
    append_recent_metadata(&newer, &newer_root, 2);
    older
        .append_entry(&TranscriptEntry::Message {
            message: message_with_id("older-message", Role::User, "older", 10),
        })
        .unwrap();
    newer
        .append_entry(&TranscriptEntry::Message {
            message: message_with_id("newer-message", Role::User, "newer", 20),
        })
        .unwrap();

    let recent =
        SessionStore::query_recent_work(&home, viden_types::RecentWorkQuery { limit: 0 }).unwrap();

    assert_eq!(recent.sessions.len(), 1);
    assert_eq!(recent.sessions[0].session_id, "newer");
    assert_eq!(recent.projects.len(), 1);
    assert_eq!(
        recent.projects[0].canonical_root,
        newer_root.canonicalize().unwrap().display().to_string()
    );
    let explicit_one =
        SessionStore::query_recent_work(&home, viden_types::RecentWorkQuery { limit: 1 }).unwrap();
    assert_eq!(explicit_one, recent);
}

#[test]
fn recent_work_clamps_above_max_and_uses_a_total_tie_breaker() {
    let home = temp_home("recent_work_upper_limit");
    let root_a = home.join("a-project");
    let root_b = home.join("b-project");
    fs::create_dir_all(&root_a).unwrap();
    fs::create_dir_all(&root_b).unwrap();
    let root_a = root_a.canonicalize().unwrap();
    let root_b = root_b.canonicalize().unwrap();
    for index in 0..101 {
        write_recent_transcript(
            &home,
            if index % 2 == 0 { &root_a } else { &root_b },
            &format!("session-{index:03}"),
            1,
            100,
            "bounded",
            None,
        );
    }

    let recent =
        SessionStore::query_recent_work(&home, viden_types::RecentWorkQuery { limit: 501 })
            .unwrap();

    assert_eq!(recent.sessions.len(), 100);
    assert!(recent.projects.len() <= 100);
    assert!(recent.sessions.windows(2).all(|items| {
        (
            items[0].canonical_root.as_str(),
            items[0].session_id.as_str(),
        ) <= (
            items[1].canonical_root.as_str(),
            items[1].session_id.as_str(),
        )
    }));
}

#[test]
fn recent_work_keeps_same_session_id_distinct_across_projects() {
    let home = temp_home("recent_work_same_id");
    let root_a = home.join("a-project");
    let root_b = home.join("b-project");
    fs::create_dir_all(&root_a).unwrap();
    fs::create_dir_all(&root_b).unwrap();
    write_recent_transcript(&home, &root_a, "same", 1, 10, "a", None);
    write_recent_transcript(&home, &root_b, "same", 2, 20, "b", None);

    let recent =
        SessionStore::query_recent_work(&home, viden_types::RecentWorkQuery { limit: 10 }).unwrap();

    assert_eq!(recent.sessions.len(), 2);
    assert_eq!(recent.projects.len(), 2);
    assert_eq!(recent.sessions[0].session_id, "same");
    assert_eq!(recent.sessions[1].session_id, "same");
    assert_ne!(
        recent.sessions[0].canonical_root,
        recent.sessions[1].canonical_root
    );
}

#[test]
fn recent_work_skips_legacy_and_tampered_project_metadata_deterministically() {
    let home = temp_home("recent_work_invalid_metadata");
    let valid_root = home.join("valid-project");
    let other_root = home.join("other-project");
    fs::create_dir_all(&valid_root).unwrap();
    fs::create_dir_all(&other_root).unwrap();
    write_recent_transcript(&home, &valid_root, "valid", 1, 5, "valid", None);

    let project_dir = home
        .join("projects")
        .join(project_key_for_path(&valid_root.canonicalize().unwrap()));
    fs::write(
        project_dir.join("legacy.jsonl"),
        TranscriptEntry::Message {
            message: message_with_id("legacy", Role::User, "legacy", 6),
        }
        .to_json_line()
            + "\n",
    )
    .unwrap();
    let tampered = project_dir.join("tampered.jsonl");
    fs::write(
        tampered,
        [
            TranscriptEntry::SessionMeta {
                entry: SessionMetaEntry {
                    timestamp: 2,
                    key: "canonical_root".to_string(),
                    value: other_root.canonicalize().unwrap().display().to_string(),
                },
            }
            .to_json_line(),
            TranscriptEntry::SessionMeta {
                entry: SessionMetaEntry {
                    timestamp: 2,
                    key: "session_created_at".to_string(),
                    value: "2".to_string(),
                },
            }
            .to_json_line(),
        ]
        .join("\n")
            + "\n",
    )
    .unwrap();

    let recent =
        SessionStore::query_recent_work(&home, viden_types::RecentWorkQuery { limit: 10 }).unwrap();

    assert_eq!(
        recent
            .sessions
            .iter()
            .map(|session| session.session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["valid"]
    );
    assert_eq!(
        recent.diagnostics,
        vec![
            "recent.missing_canonical_root".to_string(),
            "recent.project_identity_mismatch".to_string(),
        ]
    );
}

#[test]
fn recent_work_reconciles_a_nonempty_stale_index_with_canonical_jsonl() {
    let home = temp_home("recent_work_stale_index");
    let root = home.join("project");
    fs::create_dir_all(&root).unwrap();
    write_recent_transcript(&home, &root, "canonical", 1, 9, "canonical", None);
    if sqlite_available() {
        run_sql(
            &home.join("index.sqlite3"),
            "CREATE TABLE sessions (session_id TEXT PRIMARY KEY, cwd TEXT NOT NULL);\
             INSERT INTO sessions(session_id, cwd) VALUES ('stale', '/stale');",
        )
        .unwrap();
    }

    let recent =
        SessionStore::query_recent_work(&home, viden_types::RecentWorkQuery { limit: 10 }).unwrap();

    assert_eq!(recent.sessions[0].session_id, "canonical");
    if sqlite_available() {
        assert!(
            recent
                .diagnostics
                .contains(&"recent.index_stale".to_string())
        );
    }
}

#[test]
fn recent_work_missing_index_and_rebuild_inputs_are_byte_identical() {
    let home = temp_home("recent_work_rebuild_stable");
    let root = home.join("project");
    fs::create_dir_all(&root).unwrap();
    write_recent_transcript(&home, &root, "stable", 11, 22, "stable", None);
    let query = viden_types::RecentWorkQuery { limit: 10 };
    let before = SessionStore::query_recent_work(&home, query).unwrap();
    let before_bytes =
        serde_json::to_vec(&(&before.projects, &before.sessions, &before.diagnostics)).unwrap();

    let index = home.join("index.sqlite3");
    if index.exists() {
        fs::remove_file(index).unwrap();
    }
    let rebuilt = SessionStore::new_with_home(
        &home,
        root.canonicalize().unwrap(),
        Some("stable".to_string()),
    )
    .unwrap();
    rebuilt.rebuild_index_for_current().unwrap();
    let after = SessionStore::query_recent_work(&home, query).unwrap();
    let after_bytes =
        serde_json::to_vec(&(&after.projects, &after.sessions, &after.diagnostics)).unwrap();

    assert_eq!(after_bytes, before_bytes);
}

#[test]
fn recent_work_serialization_excludes_bodies_paths_and_arbitrary_metadata() {
    let base = temp_home("recent_work_privacy_base");
    let home = base.join("SECRET_HOME_TRANSCRIPT_PATH");
    let root = base.join("public-project");
    fs::create_dir_all(&root).unwrap();
    write_recent_transcript(
        &home,
        &root,
        "safe-session",
        1,
        2,
        "sk-secret-message-body",
        Some(("credential_backend", "secret-backend-value")),
    );

    let recent =
        SessionStore::query_recent_work(&home, viden_types::RecentWorkQuery { limit: 10 }).unwrap();
    let serialized = serde_json::to_string(&(&recent.projects, &recent.sessions)).unwrap();

    for forbidden in [
        "SECRET_HOME_TRANSCRIPT_PATH",
        "sk-secret-message-body",
        "credential_backend",
        "secret-backend-value",
        "transcript_path",
        "last_preview",
        "title",
    ] {
        assert!(!serialized.contains(forbidden), "leaked {forbidden}");
    }
}

#[test]
fn recent_work_accepts_runtime_event_and_cost_usage_without_leaking_payloads() {
    let home = temp_home("recent_work_runtime_entries");
    let root = home.join("public-project");
    fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let store = SessionStore::new_with_home(&home, &root, Some("runtime-session".into())).unwrap();
    append_recent_metadata(&store, &root, 1);
    let cost = CostUsageRecord {
        usage_id: "SECRET_USAGE_ID".to_string(),
        provider_id: "SECRET_PROVIDER".to_string(),
        model: "SECRET_MODEL".to_string(),
        scopes: vec![CostScope::Request("SECRET_REQUEST".to_string())],
        tokens: TokenUsage {
            input_tokens: Some(1),
            output_tokens: Some(1),
            cached_input_tokens: None,
            retrieval_tokens: None,
            total_tokens: Some(2),
        },
        estimate: None,
        actual_cost: None,
        attempt_index: 0,
        outcome: CostUsageOutcome::Success,
        recorded_at: Some(2),
    };
    store
        .append_entry(&TranscriptEntry::CostUsage {
            cost: Box::new(cost.clone()),
        })
        .unwrap();
    store
        .append_entry(&TranscriptEntry::RuntimeEvent {
            event: Box::new(RuntimeEvent::with_timestamp(
                1,
                Some(3),
                RuntimeEventKind::CostUsageRecorded { cost },
            )),
        })
        .unwrap();
    let nested_message = TranscriptEntry::Message {
        message: message_with_id(
            "SECRET_NESTED_MESSAGE",
            Role::User,
            "SECRET_NESTED_BODY",
            888,
        ),
    };
    store
        .append_entry(&TranscriptEntry::RuntimeEvent {
            event: Box::new(RuntimeEvent::with_timestamp(
                2,
                Some(10),
                RuntimeEventKind::TranscriptPageLoaded {
                    page: Box::new(TranscriptPage {
                        rows: vec![TranscriptRow::from_entry(
                            "SECRET_NESTED_SESSION",
                            0,
                            &nested_message,
                        )],
                        older: None,
                        newer: None,
                        has_more: false,
                    }),
                },
            )),
        })
        .unwrap();

    let recent =
        SessionStore::query_recent_work(&home, viden_types::RecentWorkQuery { limit: 10 }).unwrap();
    let serialized = serde_json::to_string(&(&recent.projects, &recent.sessions)).unwrap();

    assert_eq!(recent.sessions.len(), 1, "{:?}", recent.diagnostics);
    assert_eq!(recent.sessions[0].session_id, "runtime-session");
    assert_eq!(recent.sessions[0].last_updated_at, 10);
    for forbidden in [
        "SECRET_USAGE_ID",
        "SECRET_PROVIDER",
        "SECRET_MODEL",
        "SECRET_REQUEST",
        "SECRET_NESTED_MESSAGE",
        "SECRET_NESTED_BODY",
        "SECRET_NESTED_SESSION",
    ] {
        assert!(!serialized.contains(forbidden), "leaked {forbidden}");
    }
}

#[cfg(unix)]
#[test]
fn recent_work_does_not_follow_symlinks_outside_the_session_home() {
    use std::os::unix::fs::symlink;

    let home = temp_home("recent_work_scan_home");
    let outside = temp_home("recent_work_scan_outside");
    let outside_root = outside.join("project");
    fs::create_dir_all(&outside_root).unwrap();
    write_recent_transcript(
        &outside,
        &outside_root,
        "outside-secret-session",
        1,
        2,
        "outside",
        None,
    );
    symlink(outside.join("projects"), home.join("projects")).unwrap();

    let recent =
        SessionStore::query_recent_work(&home, viden_types::RecentWorkQuery { limit: 10 }).unwrap();

    assert!(recent.sessions.is_empty());
    assert!(recent.projects.is_empty());
}

#[test]
fn open_existing_for_query_does_not_create_pristine_home() {
    let base = temp_home("readonly_query_pristine_base");
    let home = base.join("missing-home");
    let cwd = base.join("workspace");
    fs::create_dir_all(&cwd).unwrap();

    let query = SessionStore::open_existing_for_query(&home, &cwd).unwrap();

    assert!(query.is_none());
    assert!(
        !home.exists(),
        "read-only session query must not create the home directory"
    );
}

#[test]
fn open_existing_for_query_preserves_existing_empty_home() {
    let home = temp_home("readonly_query_existing_home");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let sentinel = home.join("sentinel.txt");
    fs::write(&sentinel, "unchanged").unwrap();
    let before = fs::metadata(&sentinel).unwrap();

    let query = SessionStore::open_existing_for_query(&home, &cwd).unwrap();

    assert!(query.is_none());
    assert_eq!(fs::read_to_string(&sentinel).unwrap(), "unchanged");
    assert_eq!(fs::metadata(&sentinel).unwrap().len(), before.len());
    #[cfg(unix)]
    assert_eq!(
        fs::metadata(&sentinel).unwrap().permissions().mode(),
        before.permissions().mode()
    );
    assert_eq!(
        fs::metadata(&sentinel).unwrap().permissions().readonly(),
        before.permissions().readonly()
    );
    assert!(!home.join("projects").exists());
    assert!(!home.join("index.sqlite3").exists());
}

#[test]
fn jsonl_round_trip_works() {
    let home = temp_home("jsonl");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store = SessionStore::new_with_home(&home, &cwd, Some("session_roundtrip".into())).unwrap();
    store
        .append_entry(&TranscriptEntry::Message {
            message: Message::new(Role::User, "hello"),
        })
        .unwrap();
    let entries = store.load_entries().unwrap();
    assert_eq!(entries.len(), 1);
}

/// Compatibility follow-up 12: the store is the real persistence boundary, so
/// a non-ASCII body must come back from disk byte-equal. The bytes written are
/// already valid UTF-8 — this is a read-side contract and needs no migration.
#[test]
fn jsonl_round_trip_preserves_non_ascii_text() {
    let home = temp_home("jsonl_utf8");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store = SessionStore::new_with_home(&home, &cwd, Some("session_utf8".into())).unwrap();
    let body = "café 你好 — ✓";
    store
        .append_entry(&TranscriptEntry::Message {
            message: Message::new(Role::Assistant, body),
        })
        .unwrap();

    let raw = fs::read(store.transcript_path()).unwrap();
    let persisted = std::str::from_utf8(&raw).expect("the transcript file is valid UTF-8");
    assert!(
        persisted.contains(body),
        "the persisted line already holds the body verbatim: {persisted}"
    );

    let entries = store.load_entries().unwrap();
    match &entries[0] {
        TranscriptEntry::Message { message } => assert_eq!(
            message.content, body,
            "replay must return the persisted body, not a Latin-1 reading of it"
        ),
        other => panic!("expected a message entry, got {other:?}"),
    }
}

#[test]
fn replay_discards_trailing_partial_batch() {
    let home = temp_home("partial_batch_legacy");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store =
        SessionStore::new_with_home(&home, &cwd, Some("session_partial_batch".into())).unwrap();
    fs::write(
        store.transcript_path(),
        "{\"type\":\"runtime_event_batch_begin\",\"batch_id\":\"batch-a\",\"count\":1}\n{\"type\":\"message\",\"id\":\"msg_partial\",\"role\":\"user\",\"content\":\"discarded\",\"timestamp\":1,\"tool_name\":null,\"tool_call_id\":null}\n",
    )
    .unwrap();

    assert_eq!(store.load_entries().unwrap(), Vec::<TranscriptEntry>::new());
}

#[test]
fn replay_discards_malformed_uncommitted_batch_then_loads_later_committed_batch() {
    let home = temp_home("malformed_uncommitted_then_committed");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store =
        SessionStore::new_with_home(&home, &cwd, Some("session_malformed_uncommitted".into()))
            .unwrap();
    let committed = TranscriptEntry::Message {
        message: Message::new(Role::Assistant, "committed survives"),
    };
    fs::write(
        store.transcript_path(),
        format!(
            "{{\"type\":\"runtime_event_batch_begin\",\"batch_id\":\"bad\",\"count\":2}}\n{{not-json}}\n{{\"type\":\"runtime_event_batch_commit\",\"batch_id\":\"bad\",\"count\":2}}\n{{\"type\":\"runtime_event_batch_begin\",\"batch_id\":\"good\",\"count\":1}}\n{}\n{{\"type\":\"runtime_event_batch_commit\",\"batch_id\":\"good\",\"count\":1}}\n",
            committed.to_json_line()
        ),
    )
    .unwrap();

    assert_eq!(store.load_entries().unwrap(), vec![committed]);
}

#[test]
fn replay_discards_mismatched_id_and_count_batches() {
    let home = temp_home("mismatched_batch_markers");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store = SessionStore::new_with_home(&home, &cwd, Some("session_mismatch".into())).unwrap();
    let discarded_id = TranscriptEntry::Message {
        message: Message::new(Role::User, "wrong id"),
    };
    let discarded_count = TranscriptEntry::Message {
        message: Message::new(Role::User, "wrong count"),
    };
    let kept = TranscriptEntry::Message {
        message: Message::new(Role::User, "kept"),
    };
    fs::write(
        store.transcript_path(),
        format!(
            "{{\"type\":\"runtime_event_batch_begin\",\"batch_id\":\"a\",\"count\":1}}\n{}\n{{\"type\":\"runtime_event_batch_commit\",\"batch_id\":\"b\",\"count\":1}}\n{{\"type\":\"runtime_event_batch_begin\",\"batch_id\":\"c\",\"count\":2}}\n{}\n{{\"type\":\"runtime_event_batch_commit\",\"batch_id\":\"c\",\"count\":2}}\n{}\n",
            discarded_id.to_json_line(),
            discarded_count.to_json_line(),
            kept.to_json_line()
        ),
    )
    .unwrap();

    assert_eq!(store.load_entries().unwrap(), vec![kept]);
}

#[test]
fn replay_discards_nested_batch_without_hiding_later_valid_batch() {
    let home = temp_home("nested_batch");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store = SessionStore::new_with_home(&home, &cwd, Some("session_nested".into())).unwrap();
    let nested = TranscriptEntry::Message {
        message: Message::new(Role::User, "nested discarded"),
    };
    let kept = TranscriptEntry::Message {
        message: Message::new(Role::Assistant, "later valid"),
    };
    fs::write(
        store.transcript_path(),
        format!(
            "{{\"type\":\"runtime_event_batch_begin\",\"batch_id\":\"outer\",\"count\":1}}\n{{\"type\":\"runtime_event_batch_begin\",\"batch_id\":\"inner\",\"count\":1}}\n{}\n{{\"type\":\"runtime_event_batch_commit\",\"batch_id\":\"outer\",\"count\":1}}\n{{\"type\":\"runtime_event_batch_begin\",\"batch_id\":\"later\",\"count\":1}}\n{}\n{{\"type\":\"runtime_event_batch_commit\",\"batch_id\":\"later\",\"count\":1}}\n",
            nested.to_json_line(),
            kept.to_json_line()
        ),
    )
    .unwrap();

    assert_eq!(store.load_entries().unwrap(), vec![kept]);
}

#[test]
fn replay_loads_multiple_committed_batches_written_under_append_lock() {
    let home = temp_home("multiple_committed_batches");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store =
        SessionStore::new_with_home(&home, &cwd, Some("session_multiple_batches".into())).unwrap();
    let first = TranscriptEntry::Message {
        message: Message::new(Role::User, "first batch"),
    };
    let second = TranscriptEntry::Message {
        message: Message::new(Role::Assistant, "second batch"),
    };

    store
        .append_entries_atomic(std::slice::from_ref(&first))
        .unwrap();
    store
        .append_entries_atomic(std::slice::from_ref(&second))
        .unwrap();

    assert_eq!(store.load_entries().unwrap(), vec![first, second]);
}

#[test]
fn replay_quarantines_malformed_legacy_entries_and_keeps_the_rest() {
    let home = temp_home("malformed_legacy");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store =
        SessionStore::new_with_home(&home, &cwd, Some("session_malformed_legacy".into())).unwrap();
    let kept = TranscriptEntry::Message {
        message: Message::new(Role::User, "kept after the malformed line"),
    };
    fs::write(
        store.transcript_path(),
        format!("{{not-json}}\n{}\n", kept.to_json_line()),
    )
    .unwrap();

    let loaded = store.load_transcript().unwrap();

    assert_eq!(loaded.entries, vec![kept]);
    assert_eq!(loaded.quarantined.len(), 1);
    assert_eq!(loaded.quarantined[0].line_number, 1);
    assert_eq!(loaded.quarantined[0].raw, "{not-json}");
    assert!(
        loaded.quarantined[0].reason.contains("Malformed")
            || loaded.quarantined[0].reason.contains("Missing field"),
        "{}",
        loaded.quarantined[0].reason
    );
}

#[test]
fn replay_quarantines_unknown_top_level_entry_type_with_accurate_line_numbers() {
    let home = temp_home("unknown_entry_type");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store = SessionStore::new_with_home(&home, &cwd, Some("session_unknown_entry_type".into()))
        .unwrap();
    let first = TranscriptEntry::Message {
        message: Message::new(Role::User, "before"),
    };
    let second = TranscriptEntry::Message {
        message: Message::new(Role::Assistant, "after"),
    };
    let unknown = "{\"type\":\"not_yet_invented\",\"payload\":{\"detail\":\"future\"}}";
    fs::write(
        store.transcript_path(),
        format!(
            "{}\n\n{unknown}\n{}\n",
            first.to_json_line(),
            second.to_json_line()
        ),
    )
    .unwrap();

    let loaded = store.load_transcript().unwrap();

    assert_eq!(loaded.entries, vec![first, second]);
    assert_eq!(loaded.quarantined.len(), 1);
    // Blank lines still consume a line number so a report points at the file.
    assert_eq!(loaded.quarantined[0].line_number, 3);
    assert_eq!(loaded.quarantined[0].raw, unknown);
    assert!(
        loaded.quarantined[0].reason.contains("not_yet_invented"),
        "{}",
        loaded.quarantined[0].reason
    );
}

#[test]
fn replay_quarantines_unknown_runtime_event_kind_without_failing_the_session() {
    let home = temp_home("unknown_runtime_event_kind");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store =
        SessionStore::new_with_home(&home, &cwd, Some("session_unknown_kind".into())).unwrap();
    let kept = TranscriptEntry::Message {
        message: Message::new(Role::User, "survives an unknown event"),
    };
    let unknown = "{\"type\":\"runtime_event\",\"event\":{\"sequence\":1,\"timestamp\":2,\"kind\":{\"type\":\"not_yet_invented\",\"payload\":{}}}}";
    fs::write(
        store.transcript_path(),
        format!("{unknown}\n{}\n", kept.to_json_line()),
    )
    .unwrap();

    let loaded = store.load_transcript().unwrap();

    assert_eq!(loaded.entries, vec![kept]);
    assert_eq!(loaded.quarantined.len(), 1);
    assert_eq!(loaded.quarantined[0].line_number, 1);
    assert_eq!(loaded.quarantined[0].raw, unknown);
    assert!(
        loaded.quarantined[0].reason.contains("not_yet_invented"),
        "{}",
        loaded.quarantined[0].reason
    );
    // The legacy entry-only API keeps working and never returns a partial error.
    assert_eq!(store.load_entries().unwrap(), loaded.entries);
}

#[test]
fn replay_quarantines_inside_a_batch_still_drops_the_whole_batch() {
    let home = temp_home("quarantine_inside_batch");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store =
        SessionStore::new_with_home(&home, &cwd, Some("session_quarantine_batch".into())).unwrap();
    let dropped = TranscriptEntry::Message {
        message: Message::new(Role::User, "dropped with its batch"),
    };
    let kept = TranscriptEntry::Message {
        message: Message::new(Role::Assistant, "later committed batch"),
    };
    let unknown = "{\"type\":\"not_yet_invented\",\"payload\":{}}";
    fs::write(
        store.transcript_path(),
        format!(
            "{{\"type\":\"runtime_event_batch_begin\",\"batch_id\":\"bad\",\"count\":2}}\n{}\n{unknown}\n{{\"type\":\"runtime_event_batch_commit\",\"batch_id\":\"bad\",\"count\":2}}\n{{\"type\":\"runtime_event_batch_begin\",\"batch_id\":\"good\",\"count\":1}}\n{}\n{{\"type\":\"runtime_event_batch_commit\",\"batch_id\":\"good\",\"count\":1}}\n",
            dropped.to_json_line(),
            kept.to_json_line()
        ),
    )
    .unwrap();

    let loaded = store.load_transcript().unwrap();

    assert_eq!(loaded.entries, vec![kept]);
    assert_eq!(loaded.quarantined.len(), 1);
    assert_eq!(loaded.quarantined[0].line_number, 3);
    assert_eq!(loaded.quarantined[0].raw, unknown);
}

#[test]
fn recent_work_keeps_a_session_that_contains_a_quarantined_line() {
    let home = temp_home("recent_work_quarantine");
    let root = home.join("project");
    fs::create_dir_all(&root).unwrap();
    write_recent_transcript(&home, &root, "quarantined", 3, 4, "hello", None);

    let project_dir = home
        .join("projects")
        .join(project_key_for_path(&root.canonicalize().unwrap()));
    let transcript = project_dir.join("quarantined.jsonl");
    let mut contents = fs::read_to_string(&transcript).unwrap();
    contents.push_str("{\"type\":\"not_yet_invented\",\"payload\":{}}\n");
    fs::write(&transcript, contents).unwrap();

    let recent =
        SessionStore::query_recent_work(&home, viden_types::RecentWorkQuery { limit: 10 }).unwrap();

    assert_eq!(
        recent
            .sessions
            .iter()
            .map(|session| session.session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["quarantined"]
    );
    assert!(
        recent
            .diagnostics
            .contains(&"recent.quarantined_transcript_lines".to_string()),
        "{:?}",
        recent.diagnostics
    );
}

#[test]
fn jsonl_round_trip_preserves_cost_usage_entries() {
    let home = temp_home("jsonl_cost_usage");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store =
        SessionStore::new_with_home(&home, &cwd, Some("session_cost_roundtrip".into())).unwrap();
    let cost = CostUsageRecord {
        usage_id: "usage-session-1".to_string(),
        provider_id: "context".to_string(),
        model: "retrieval".to_string(),
        scopes: vec![CostScope::Request("req-session-1".to_string())],
        tokens: TokenUsage {
            input_tokens: None,
            output_tokens: None,
            cached_input_tokens: None,
            retrieval_tokens: Some(42),
            total_tokens: Some(42),
        },
        estimate: None,
        actual_cost: None,
        attempt_index: 0,
        outcome: CostUsageOutcome::Success,
        recorded_at: Some(now_timestamp()),
    };
    store
        .append_entry(&TranscriptEntry::CostUsage {
            cost: Box::new(cost.clone()),
        })
        .unwrap();

    let entries = store.load_entries().unwrap();
    assert_eq!(
        entries,
        vec![TranscriptEntry::CostUsage {
            cost: Box::new(cost)
        }]
    );
}

#[test]
fn sqlite_index_is_updated() {
    let home = temp_home("sqlite");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store = SessionStore::new_with_home(&home, &cwd, Some("session_index".into())).unwrap();
    store
        .append_entry(&TranscriptEntry::Message {
            message: Message::new(Role::User, "hello"),
        })
        .unwrap();
    let sessions = store.list_sessions_for_cwd().unwrap();
    assert!(!sessions.is_empty());
}

#[test]
fn summary_metadata_counts_messages_commands_and_tool_calls() {
    let home = temp_home("summary_meta");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store = SessionStore::new_with_home(&home, &cwd, Some("session_summary".into())).unwrap();
    store
        .append_entry(&TranscriptEntry::Message {
            message: Message::new(Role::User, "inspect summary"),
        })
        .unwrap();
    store
        .append_entry(&TranscriptEntry::ToolCall {
            call: ToolCall {
                id: "tool_1".into(),
                name: "read_file".into(),
                input: Default::default(),
            },
        })
        .unwrap();
    store
        .append_entry(&TranscriptEntry::Command {
            entry: CommandLogEntry {
                timestamp: now_timestamp(),
                name: "status".into(),
                args: vec![],
                output: "status output".into(),
            },
        })
        .unwrap();
    let summary = store
        .list_sessions_for_cwd()
        .unwrap()
        .into_iter()
        .find(|item| item.session_id == "session_summary")
        .unwrap();
    assert_eq!(summary.message_count, 1);
    assert_eq!(summary.tool_call_count, 1);
    assert_eq!(summary.command_count, 1);
    assert_eq!(summary.last_activity_kind.as_deref(), Some("command"));
    assert_eq!(
        summary.last_activity_preview.as_deref(),
        Some("status output")
    );
}

#[test]
fn sqlite_index_preserves_sessions_with_multiline_command_previews() {
    let home = temp_home("multiline_command_preview");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store = SessionStore::new_with_home(&home, &cwd, Some("session_multiline".into())).unwrap();
    store
        .append_entry(&TranscriptEntry::Command {
            entry: CommandLogEntry {
                timestamp: now_timestamp(),
                name: "provider".into(),
                args: vec!["deepseek".into(), "deepseek-v4-flash".into()],
                output: "Provider set to deepseek (deepseek-v4-flash)\nSaved default provider/model to /tmp/config.toml\nNext live turn uses deepseek / deepseek-v4-flash.".into(),
            },
        })
        .unwrap();

    let summary = store
        .list_sessions_for_cwd()
        .unwrap()
        .into_iter()
        .find(|item| item.session_id == "session_multiline")
        .unwrap();
    assert_eq!(summary.command_count, 1);
    assert_eq!(summary.last_activity_kind.as_deref(), Some("command"));
    assert_eq!(
        summary.last_activity_preview.as_deref(),
        Some("Provider set to deepseek (deepseek-v4-flash) Saved default provider/model to /tm...")
    );
    assert!(
        store
            .load_by_id_for_cwd("session_multiline")
            .unwrap()
            .is_some()
    );
}

#[test]
fn falls_back_to_project_scan_when_sqlite_index_has_old_schema() {
    let home = temp_home("sqlite_fallback");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store = SessionStore::new_with_home(&home, &cwd, Some("session_fallback".into())).unwrap();
    store
        .append_entry(&TranscriptEntry::Message {
            message: Message::new(Role::User, "fallback session"),
        })
        .unwrap();

    if sqlite_available() {
        let legacy_sql = "DROP TABLE IF EXISTS sessions;
            CREATE TABLE sessions (
                session_id TEXT PRIMARY KEY,
                cwd TEXT NOT NULL,
                project_key TEXT NOT NULL,
                title TEXT,
                last_preview TEXT,
                transcript_path TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                last_updated_at INTEGER NOT NULL
            );";
        run_sql(&home.join("index.sqlite3"), legacy_sql).unwrap();
    }

    let sessions = store.list_sessions_for_cwd().unwrap();
    assert!(
        sessions
            .iter()
            .any(|item| item.session_id == "session_fallback")
    );
}

fn message_with_id(id: &str, role: Role, content: &str, timestamp: u64) -> Message {
    Message {
        id: id.to_string(),
        role,
        content: content.to_string(),
        timestamp,
        tool_name: None,
        tool_call_id: None,
    }
}

#[test]
fn transcript_page_newest_older_newer_limits_and_order_are_stable() {
    let home = temp_home("transcript_page_newest");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store = SessionStore::new_with_home(&home, &cwd, Some("session_page".into())).unwrap();
    for index in 0..5 {
        store
            .append_entry(&TranscriptEntry::Message {
                message: message_with_id(
                    &format!("msg-{index}"),
                    Role::User,
                    &format!("content {index}"),
                    index + 1,
                ),
            })
            .unwrap();
    }

    let newest = store
        .load_transcript_page(&TranscriptPageRequest {
            session_id: "session_page".to_string(),
            before: None,
            limit: 2,
        })
        .unwrap();
    assert_eq!(
        newest
            .rows
            .iter()
            .map(|row| row.id.0.as_str())
            .collect::<Vec<_>>(),
        vec!["session_page:3", "session_page:4"]
    );
    assert!(newest.has_more);
    assert_eq!(newest.older.as_ref().unwrap().ordinal, 3);
    assert_eq!(newest.newer, None);

    let previous = store
        .load_transcript_page(&TranscriptPageRequest {
            session_id: "session_page".to_string(),
            before: newest.older.clone(),
            limit: 2,
        })
        .unwrap();
    assert_eq!(
        previous
            .rows
            .iter()
            .map(|row| row.id.0.as_str())
            .collect::<Vec<_>>(),
        vec!["session_page:1", "session_page:2"]
    );
    assert!(previous.has_more);
    assert_eq!(previous.older.as_ref().unwrap().ordinal, 1);
    assert_eq!(previous.newer.as_ref().unwrap().ordinal, 2);

    let oldest = store
        .load_transcript_page(&TranscriptPageRequest {
            session_id: "session_page".to_string(),
            before: previous.older.clone(),
            limit: 0,
        })
        .unwrap();
    assert_eq!(oldest.rows.len(), 1);
    assert_eq!(oldest.rows[0].id.0, "session_page:0");
    assert!(!oldest.has_more);
    assert_eq!(oldest.older, None);
    assert_eq!(oldest.newer.as_ref().unwrap().ordinal, 0);

    let all = store
        .load_transcript_page(&TranscriptPageRequest {
            session_id: "session_page".to_string(),
            before: None,
            limit: 900,
        })
        .unwrap();
    assert_eq!(all.rows.len(), 5);
    assert!(!all.has_more);
}

#[test]
fn transcript_page_rejects_wrong_session_cursor_and_loads_exact_other_session() {
    let home = temp_home("transcript_page_other_session");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store_a = SessionStore::new_with_home(&home, &cwd, Some("session_a".into())).unwrap();
    let store_b = SessionStore::new_with_home(&home, &cwd, Some("session_b".into())).unwrap();
    store_a
        .append_entry(&TranscriptEntry::Message {
            message: message_with_id("msg-a", Role::User, "a", 1),
        })
        .unwrap();
    store_b
        .append_entry(&TranscriptEntry::Message {
            message: message_with_id("msg-b", Role::User, "b", 2),
        })
        .unwrap();

    let err = store_a
        .load_transcript_page(&TranscriptPageRequest {
            session_id: "session_a".to_string(),
            before: Some(TranscriptCursor {
                session_id: "session_b".to_string(),
                ordinal: 0,
            }),
            limit: 25,
        })
        .unwrap_err();
    assert!(err.contains("cursor session"));

    let page = store_a
        .load_transcript_page(&TranscriptPageRequest {
            session_id: "session_b".to_string(),
            before: None,
            limit: 25,
        })
        .unwrap();
    assert_eq!(page.rows[0].id.0, "session_b:0");
}

#[test]
fn transcript_page_exact_lookup_recovers_project_jsonl_when_sqlite_index_is_stale() {
    let home = temp_home("transcript_page_stale_index");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store_a = SessionStore::new_with_home(&home, &cwd, Some("session_a".into())).unwrap();
    store_a
        .append_entry(&TranscriptEntry::Message {
            message: message_with_id("msg-a", Role::User, "indexed", 1),
        })
        .unwrap();
    if !sqlite_available() {
        return;
    }
    let store_b = SessionStore::new_with_home(&home, &cwd, Some("session_b".into())).unwrap();
    fs::write(
        store_b.transcript_path(),
        TranscriptEntry::Message {
            message: message_with_id("msg-b", Role::User, "jsonl only", 2),
        }
        .to_json_line()
            + "\n",
    )
    .unwrap();

    let indexed = store_a.list_sessions_from_sqlite().unwrap();
    assert!(
        indexed
            .iter()
            .any(|summary| summary.session_id == "session_a")
    );
    assert!(
        !indexed
            .iter()
            .any(|summary| summary.session_id == "session_b")
    );

    let page = store_a
        .load_transcript_page(&TranscriptPageRequest {
            session_id: "session_b".to_string(),
            before: None,
            limit: 25,
        })
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].id.0, "session_b:0");
}

#[test]
fn transcript_page_rebuild_and_reconnect_preserve_ids_order_and_anchors() {
    let home = temp_home("transcript_page_reconnect");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store = SessionStore::new_with_home(&home, &cwd, Some("session_rebuild".into())).unwrap();
    for index in 0..3 {
        store
            .append_entry(&TranscriptEntry::Message {
                message: message_with_id(
                    &format!("msg-rebuild-{index}"),
                    Role::Assistant,
                    &format!("rebuild {index}"),
                    index + 1,
                ),
            })
            .unwrap();
    }
    let request = TranscriptPageRequest {
        session_id: "session_rebuild".to_string(),
        before: None,
        limit: 2,
    };
    let before = store.load_transcript_page(&request).unwrap();
    store.rebuild_index_for_current().unwrap();
    let reconnected =
        SessionStore::new_with_home(&home, &cwd, Some("session_rebuild".into())).unwrap();
    let after = reconnected.load_transcript_page(&request).unwrap();

    assert_eq!(after, before);
}

#[test]
fn transcript_page_includes_committed_batch_and_keeps_uncommitted_behavior() {
    let home = temp_home("transcript_page_committed_batch");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store = SessionStore::new_with_home(&home, &cwd, Some("session_batch".into())).unwrap();
    let committed = TranscriptEntry::Message {
        message: message_with_id("msg-committed", Role::Assistant, "committed", 1),
    };
    let discarded = TranscriptEntry::Message {
        message: message_with_id("msg-uncommitted", Role::Assistant, "discarded", 2),
    };
    store
        .append_entries_atomic(std::slice::from_ref(&committed))
        .unwrap();
    assert!(
        store
            .append_entries_uncommitted_for_test(std::slice::from_ref(&discarded), 1)
            .is_err()
    );

    let page = store
        .load_transcript_page(&TranscriptPageRequest {
            session_id: "session_batch".to_string(),
            before: None,
            limit: 10,
        })
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].id.0, "session_batch:0");
    assert!(matches!(
        &page.rows[0].kind,
        TranscriptRowKind::Message { message } if message.id == "msg-committed"
    ));
}

#[test]
fn transcript_page_coalesces_repeated_assistant_message_id_to_first_ordinal_latest_content() {
    let home = temp_home("transcript_page_coalesce");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store = SessionStore::new_with_home(&home, &cwd, Some("session_stream".into())).unwrap();
    store
        .append_entry(&TranscriptEntry::Message {
            message: message_with_id("assistant-stream", Role::Assistant, "partial", 10),
        })
        .unwrap();
    store
        .append_entry(&TranscriptEntry::ToolCall {
            call: ToolCall {
                id: "tool-1".to_string(),
                name: "shell".to_string(),
                input: Default::default(),
            },
        })
        .unwrap();
    store
        .append_entry(&TranscriptEntry::Message {
            message: message_with_id("assistant-stream", Role::Assistant, "complete", 11),
        })
        .unwrap();

    let page = store
        .load_transcript_page(&TranscriptPageRequest {
            session_id: "session_stream".to_string(),
            before: None,
            limit: 10,
        })
        .unwrap();
    assert_eq!(
        page.rows
            .iter()
            .map(|row| row.id.0.as_str())
            .collect::<Vec<_>>(),
        vec!["session_stream:0", "session_stream:1"]
    );
    assert!(matches!(
        &page.rows[0].kind,
        TranscriptRowKind::Message { message }
            if message.content == "complete" && page.rows[0].cursor.ordinal == 0
    ));
}

#[test]
fn transcript_page_converts_every_entry_kind() {
    let home = temp_home("transcript_page_entry_kinds");
    let cwd = home.join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let store = SessionStore::new_with_home(&home, &cwd, Some("session_kinds".into())).unwrap();
    let cost = CostUsageRecord {
        usage_id: "usage-kind".to_string(),
        provider_id: "deepseek".to_string(),
        model: "deepseek-v4-flash".to_string(),
        scopes: vec![CostScope::Request("request-kind".to_string())],
        tokens: TokenUsage {
            input_tokens: Some(1),
            output_tokens: Some(1),
            cached_input_tokens: Some(0),
            retrieval_tokens: None,
            total_tokens: Some(2),
        },
        estimate: None,
        actual_cost: None,
        attempt_index: 0,
        outcome: CostUsageOutcome::Success,
        recorded_at: Some(22),
    };
    let entries = vec![
        TranscriptEntry::Message {
            message: message_with_id("msg-kind", Role::User, "msg", 1),
        },
        TranscriptEntry::ToolCall {
            call: ToolCall {
                id: "tool-kind".to_string(),
                name: "shell".to_string(),
                input: Default::default(),
            },
        },
        TranscriptEntry::ToolResult {
            result: ToolResult {
                tool_call_id: "tool-kind".to_string(),
                name: "shell".to_string(),
                output: "ok".to_string(),
                diff: None,
                success: true,
                exit_code: Some(0),
            },
        },
        TranscriptEntry::Permission {
            entry: PermissionLogEntry {
                timestamp: 2,
                tool_name: "shell".to_string(),
                decision: "allow".to_string(),
                reason: "test".to_string(),
                message: None,
            },
        },
        TranscriptEntry::Command {
            entry: CommandLogEntry {
                timestamp: 3,
                name: "status".to_string(),
                args: Vec::new(),
                output: "ok".to_string(),
            },
        },
        TranscriptEntry::SessionMeta {
            entry: SessionMetaEntry {
                timestamp: 4,
                key: "model".to_string(),
                value: "deepseek".to_string(),
            },
        },
        TranscriptEntry::CostUsage {
            cost: Box::new(cost.clone()),
        },
        TranscriptEntry::RuntimeEvent {
            event: Box::new(RuntimeEvent::with_timestamp(
                1,
                Some(5),
                RuntimeEventKind::CostUsageRecorded { cost },
            )),
        },
    ];
    for entry in &entries {
        store.append_entry(entry).unwrap();
    }

    let page = store
        .load_transcript_page(&TranscriptPageRequest {
            session_id: "session_kinds".to_string(),
            before: None,
            limit: 20,
        })
        .unwrap();
    let kind_names = page
        .rows
        .iter()
        .map(|row| match &row.kind {
            TranscriptRowKind::Message { .. } => "message",
            TranscriptRowKind::ToolCall { .. } => "tool_call",
            TranscriptRowKind::ToolResult { .. } => "tool_result",
            TranscriptRowKind::Permission { .. } => "permission",
            TranscriptRowKind::Command { .. } => "command",
            TranscriptRowKind::SessionMeta { .. } => "session_meta",
            TranscriptRowKind::CostUsage { .. } => "cost_usage",
            TranscriptRowKind::RuntimeEvent { .. } => "runtime_event",
        })
        .collect::<Vec<_>>();
    assert_eq!(
        kind_names,
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
}
