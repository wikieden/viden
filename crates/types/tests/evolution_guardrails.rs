//! Executable form of the protocol evolution rules.
//!
//! These tests are the enforcement layer for
//! `docs/frontend-integration-contract.md` -> "Protocol Evolution Rules". They
//! deliberately exercise only the public crate surface, because the rules
//! constrain what *other* builds may observe on the wire and on disk:
//!
//! 1. additive fields must not break older readers;
//! 2. an unknown event type must round-trip as `RuntimeWireEvent::Unknown`
//!    instead of failing the stream;
//! 3. a renamed variant must keep its legacy tag as a serde alias;
//! 4. transcript replay must classify an unknown persisted event as a
//!    quarantinable line instead of a fatal parse failure.
//!
//! A change that breaks one of these is a contract break, not a test failure.

use viden_types::{
    AgentTaskStatus, ConflictBaseline, ConflictContent, ConflictFile, ConflictHunk,
    ConflictHunkReason, RuntimeEvent, RuntimeEventKind, RuntimeWireEvent, TranscriptEntry,
};

/// Rule 1: additive fields on a known variant are ignored by older readers.
#[test]
fn unknown_additive_fields_on_a_known_event_still_deserialize() {
    let json = r#"{
        "sequence": 7,
        "timestamp": 11,
        "unreleased_envelope_field": {"nested": true},
        "kind": {
            "type": "agent_session_input_accepted",
            "payload": {
                "session_id": "session_a",
                "input_id": "input_a",
                "unreleased_payload_field": ["future"]
            }
        }
    }"#;

    let event: RuntimeWireEvent =
        serde_json::from_str(json).expect("additive fields must not fail");

    let RuntimeWireEvent::Known(event) = event else {
        panic!("a known event type with extra fields must stay Known");
    };
    assert_eq!(event.sequence, 7);
    assert_eq!(event.timestamp, Some(11));
    assert_eq!(
        event.kind,
        RuntimeEventKind::AgentSessionInputAccepted {
            session_id: "session_a".to_string(),
            input_id: "input_a".to_string(),
        }
    );
}

/// Rule 2: an unknown event type is preserved, never rejected.
#[test]
fn unknown_event_type_round_trips_as_the_unknown_wire_variant() {
    let json = r#"{
        "sequence": 3,
        "timestamp": null,
        "kind": {"type": "not_yet_invented", "payload": {"detail": "future"}}
    }"#;

    let event: RuntimeWireEvent =
        serde_json::from_str(json).expect("an unknown event type must not error");

    let RuntimeWireEvent::Unknown {
        event_type,
        payload,
    } = &event
    else {
        panic!("an unknown event type must deserialize to RuntimeWireEvent::Unknown");
    };
    assert_eq!(event_type, "not_yet_invented");
    assert_eq!(payload, &serde_json::json!({"detail": "future"}));

    // Re-serializing must preserve the extension payload for the next hop.
    let reserialized = serde_json::to_value(&event).expect("unknown events must re-serialize");
    assert_eq!(
        reserialized,
        serde_json::json!({
            "kind": {"type": "not_yet_invented", "payload": {"detail": "future"}}
        })
    );
    let round_tripped: RuntimeWireEvent =
        serde_json::from_value(reserialized).expect("unknown events must survive a round trip");
    assert_eq!(round_tripped, event);
}

/// Rule 3: renames keep the legacy tag as an alias. `AgentTaskStatus` is the
/// established pattern; this canary fails if a rename drops the old tag.
#[test]
fn renamed_variants_keep_deserializing_from_their_legacy_tags() {
    let legacy_pairs = [
        ("starting", AgentTaskStatus::Thinking),
        ("tool", AgentTaskStatus::RunningTool),
        ("approval", AgentTaskStatus::WaitingApproval),
        ("input", AgentTaskStatus::NeedsInput),
        ("apply_conflict", AgentTaskStatus::Blocked),
        ("completed", AgentTaskStatus::Done),
        ("accepted", AgentTaskStatus::Done),
        ("detached", AgentTaskStatus::Discarded),
        ("stopped", AgentTaskStatus::Discarded),
        ("canceled", AgentTaskStatus::Cancelled),
    ];

    for (legacy_tag, expected) in legacy_pairs {
        let parsed: AgentTaskStatus = serde_json::from_str(&format!("\"{legacy_tag}\""))
            .unwrap_or_else(|err| panic!("legacy tag `{legacy_tag}` must still parse: {err}"));
        assert_eq!(parsed, expected, "legacy tag `{legacy_tag}`");
    }

    // The canonical tag is the one that is written back out.
    assert_eq!(
        serde_json::to_string(&AgentTaskStatus::Done).unwrap(),
        "\"done\""
    );
}

/// Rule 4: a persisted event this build cannot reduce is a quarantinable line,
/// reported with the offending type, not an opaque parse failure.
#[test]
fn unknown_persisted_runtime_event_is_reported_as_an_unknown_type() {
    let line = r#"{"type":"runtime_event","event":{"sequence":1,"timestamp":2,"kind":{"type":"not_yet_invented","payload":{}}}}"#;

    let error = TranscriptEntry::from_json_line(line)
        .expect_err("an unknown persisted event kind must not load as a known entry");

    assert!(
        error.contains("not_yet_invented"),
        "quarantine reason must name the unknown event type: {error}"
    );
}

/// Rule 4 (top level): an unknown transcript entry type is reported by name.
#[test]
fn unknown_transcript_entry_type_is_reported_by_name() {
    let line = r#"{"type":"not_yet_invented","payload":{"detail":"future"}}"#;

    let error = TranscriptEntry::from_json_line(line)
        .expect_err("an unknown transcript entry type must not load as a known entry");

    assert!(
        error.contains("not_yet_invented"),
        "quarantine reason must name the unknown entry type: {error}"
    );
}

/// A known persisted event must still replay unchanged; forward compatibility
/// must not weaken the known path.
#[test]
fn known_persisted_runtime_event_still_replays() {
    let line = r#"{"type":"runtime_event","event":{"sequence":4,"timestamp":9,"kind":{"type":"agent_session_input_accepted","payload":{"session_id":"session_a","input_id":"input_a"}}}}"#;

    let entry = TranscriptEntry::from_json_line(line).expect("a known event must replay");

    let TranscriptEntry::RuntimeEvent { event } = entry else {
        panic!("expected a runtime event entry");
    };
    assert_eq!(event.sequence, 4);
    assert_eq!(
        event.kind,
        RuntimeEventKind::AgentSessionInputAccepted {
            session_id: "session_a".to_string(),
            input_id: "input_a".to_string(),
        }
    );
}

/// Rule 1, for `runtime.conflict_content`: adding `content` to an existing
/// event must not change what an older payload means or what an older build
/// can read. A pre-C3 `lane_conflict_detected` has no `content` key at all and
/// must stay a *known* event reducing to `None`; a post-C3 one must survive
/// the wire with its lines intact.
#[test]
fn lane_conflict_content_is_additive_in_both_directions() {
    let pre_c3 = r#"{
        "sequence": 2,
        "timestamp": 5,
        "kind": {
            "type": "lane_conflict_detected",
            "payload": {
                "lane_id": "lane_a",
                "summary": "patch conflict",
                "paths": ["a.rs"]
            }
        }
    }"#;
    let event: RuntimeWireEvent =
        serde_json::from_str(pre_c3).expect("a pre-C3 conflict payload must still parse");
    let RuntimeWireEvent::Known(known) = event else {
        panic!("lane_conflict_detected must stay a known event type");
    };
    let RuntimeEventKind::LaneConflictDetected { content, .. } = &known.kind else {
        panic!("the payload must decode as the lane conflict variant");
    };
    assert!(
        content.is_none(),
        "a payload written before the field must read as unknown, not as an empty conflict"
    );

    let with_content = RuntimeWireEvent::Known(RuntimeEvent::new(
        3,
        RuntimeEventKind::LaneConflictDetected {
            lane_id: "lane_a".to_string(),
            summary: "patch conflict".to_string(),
            paths: vec!["a.rs".to_string()],
            content: Some(ConflictContent {
                baseline: ConflictBaseline::Unknown,
                files: vec![ConflictFile {
                    path: "a.rs".to_string(),
                    hunks: vec![ConflictHunk {
                        ours_start: 1,
                        ours: vec!["ours".to_string()],
                        theirs_start: 1,
                        theirs: vec!["theirs".to_string()],
                        base: Some(vec!["base".to_string()]),
                        reason: ConflictHunkReason::ContextMismatch,
                    }],
                    omitted: false,
                }],
                truncated: false,
            }),
        },
    ));
    let encoded = serde_json::to_string(&with_content).expect("encode");
    let decoded: RuntimeWireEvent = serde_json::from_str(&encoded).expect("decode");
    assert_eq!(decoded, with_content);
}
