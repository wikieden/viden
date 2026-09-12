//! The dock inspector's single-file read (`runtime.workspace_file_reads`, C9).
//!
//! The inventory (GUI-CORE-022) says a path exists; this says what is in it.
//! Before C9 the inspector's Open was a disabled control naming the missing
//! capability, and these tests are what makes it a real read without letting
//! it lie:
//!
//! 1. **The three bodies stay three bodies.** Text Core will publish, bytes it
//!    will not, and nothing to read with a typed reason produce three
//!    different affordances. A fourth, unmodelled body is `unknown` rather
//!    than the nearest match — `WorkspaceFileBody` is `#[non_exhaustive]`.
//! 2. **`size` and `sha256` are the whole file's.** The hash is of the file,
//!    never of the published prefix, and both are absent where Core published
//!    none rather than zero and `""`.
//! 3. **A path is refused, never repaired.** Core's own validator runs before
//!    anything is sent, so `../` is one refusal in one wording on both sides.
//! 4. **A refusal is Core's words.** The permission gate's `deny`/`ask` answer
//!    reaches the inspector as `CommandRejected`, reason and hint intact.
//! 5. **An answer names its read.** `WorkspaceFileLoaded.command_id` is
//!    required, so an answer for another file is dropped rather than rendered
//!    under this path's name.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use viden_core::{
    EventCursor, FRONTEND_SCHEMA_V1, RuntimeCommand, RuntimeCommandEnvelope, RuntimeErrorView,
    RuntimeEvent, RuntimeEventEnvelope, RuntimeEventKind, RuntimeOwner, RuntimeSnapshot,
    RuntimeViewState, RuntimeWireEvent, SourceTarget, WorkspaceFileBody, WorkspaceFileContent,
    WorkspaceFileUnavailableReason,
};
use viden_gui::{GuiCoreAdapter, WORKSPACE_FILE_READS_CAPABILITY};

mod support;
use support::TestCoreClient;

const TIMEOUT: Duration = Duration::from_millis(10);
const PATH: &str = "crates/types/src/lib.rs";

fn view() -> RuntimeViewState {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../crates/types/tests/fixtures/frontend-contract-v1/multi-lane.json"
    ))
    .expect("fixture json");
    let snapshot: RuntimeSnapshot =
        serde_json::from_value(fixture["initial_snapshot"].clone()).expect("fixture snapshot");
    RuntimeViewState::new(snapshot)
}

fn envelope(sequence: u64, kind: RuntimeEventKind) -> RuntimeEventEnvelope {
    RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: RuntimeOwner::default(),
        cursor: EventCursor {
            stream_id: "gui-test".to_string(),
            sequence,
        },
        event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
            sequence,
            Some(1_700_000_000 + sequence),
            kind,
        )),
    }
}

fn loaded(sequence: u64, command_id: &str, file: WorkspaceFileContent) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::WorkspaceFileLoaded {
            command_id: command_id.to_string(),
            file,
        },
    )
}

/// Core's refusal: a `CommandRejected` naming the exact read it refuses.
fn refused(sequence: u64, command_id: &str, reason: &str) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::CommandRejected {
            command_id: command_id.to_string(),
            reason: reason.to_string(),
        },
    )
}

/// An unrelated failure with no command id, of the kind a lane or provider
/// emits while a file read happens to be outstanding.
fn unrelated_error(sequence: u64, message: &str) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::Error {
            error: RuntimeErrorView {
                message: message.to_string(),
                recoverable: true,
                hint: Some("check the path".to_string()),
            },
        },
    )
}

fn text_file(text: &str, truncated: bool) -> WorkspaceFileContent {
    WorkspaceFileContent {
        path: PATH.to_string(),
        size: Some(4_096),
        sha256: Some("f".repeat(64)),
        content: WorkspaceFileBody::Text {
            text: text.to_string(),
            truncated,
        },
    }
}

struct Harness {
    adapter: GuiCoreAdapter,
    sent: Arc<Mutex<Vec<RuntimeCommandEnvelope>>>,
}

fn harness(events: Vec<RuntimeEventEnvelope>, with_capability: bool) -> Harness {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut client = TestCoreClient::new(view(), sent.clone());
    if !with_capability {
        client.capabilities.remove(WORKSPACE_FILE_READS_CAPABILITY);
    }
    for event in events {
        client = client.with_envelope(event);
    }
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect");
    Harness { adapter, sent }
}

fn reads(
    sent: &Arc<Mutex<Vec<RuntimeCommandEnvelope>>>,
) -> Vec<viden_core::WorkspaceFileReadQuery> {
    sent.lock()
        .expect("sent lock")
        .iter()
        .filter_map(|envelope| match &envelope.command {
            RuntimeCommand::ReadWorkspaceFile { query } => Some(query.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn an_absent_capability_sends_nothing_and_names_itself() {
    let mut harness = harness(Vec::new(), false);
    let projection = harness
        .adapter
        .read_workspace_file_and_wait("gui-file-1", None, PATH, TIMEOUT)
        .expect("an absent capability is not an error");
    assert!(!projection.capability_available);
    assert_eq!(projection.outcome.state, "idle");
    assert!(projection.file.is_none());
    assert!(
        reads(&harness.sent).is_empty(),
        "nothing is sent for a capability Core never published"
    );
}

#[test]
fn a_text_body_carries_the_bound_that_cut_it_and_the_whole_files_hash() {
    let mut harness = harness(
        vec![loaded(
            1,
            "gui-file-1",
            text_file("pub mod protocol;\n", true),
        )],
        true,
    );
    let projection = harness
        .adapter
        .read_workspace_file_and_wait("gui-file-1", None, PATH, TIMEOUT)
        .expect("read");

    assert_eq!(projection.outcome.state, "confirmed");
    assert_eq!(projection.requested_path.as_deref(), Some(PATH));
    assert_eq!(projection.target_lane_id, None);
    let file = projection.file.expect("Core's answer");
    assert_eq!(file.body, "text");
    assert_eq!(file.text.as_deref(), Some("pub mod protocol;\n"));
    // The bound cut it: a short file and a cut one are otherwise
    // indistinguishable, and a reviewer reading half a file as the whole one is
    // the failure this flag prevents.
    assert!(file.truncated);
    // The length and the hash are the *whole* file's, which is what lets the
    // inspector say 4 KiB for a 17-byte body without contradicting itself.
    assert_eq!(file.size, Some(4_096));
    assert_eq!(file.sha256.as_deref(), Some(&"f".repeat(64)[..]));
    assert_eq!(file.reason, None);

    let queries = reads(&harness.sent);
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0].path, PATH);
    assert_eq!(queries[0].target, SourceTarget::Workspace);
    // Core owns the bound and publishes whether it cut the body, so the client
    // expresses no preference.
    assert_eq!(queries[0].byte_limit, None);
}

#[test]
fn a_lane_read_names_the_lane_and_never_a_path() {
    let mut harness = harness(
        vec![loaded(1, "gui-file-1", text_file("fn main() {}\n", false))],
        true,
    );
    let projection = harness
        .adapter
        .read_workspace_file_and_wait("gui-file-1", Some("lane_core"), PATH, TIMEOUT)
        .expect("read");
    assert_eq!(projection.target_lane_id.as_deref(), Some("lane_core"));
    assert_eq!(
        reads(&harness.sent)[0].target,
        SourceTarget::Lane {
            lane_id: "lane_core".to_string()
        },
        "Core resolves the worktree from its own records"
    );
}

#[test]
fn a_binary_body_publishes_no_bytes_and_keeps_its_size() {
    let mut harness = harness(
        vec![loaded(
            1,
            "gui-file-1",
            WorkspaceFileContent {
                path: "assets/icon.png".to_string(),
                size: Some(20_480),
                sha256: Some("a".repeat(64)),
                content: WorkspaceFileBody::Binary,
            },
        )],
        true,
    );
    let file = harness
        .adapter
        .read_workspace_file_and_wait("gui-file-1", None, "assets/icon.png", TIMEOUT)
        .expect("read")
        .file
        .expect("Core's answer");
    assert_eq!(file.body, "binary");
    // No text at all, rather than replacement characters rendered as content.
    assert_eq!(file.text, None);
    assert!(!file.truncated);
    assert_eq!(file.size, Some(20_480));
}

#[test]
fn every_unavailable_reason_keeps_its_own_name_and_no_fabricated_size() {
    for (reason, expected) in [
        (WorkspaceFileUnavailableReason::NotFound, "not_found"),
        (WorkspaceFileUnavailableReason::Directory, "directory"),
        (WorkspaceFileUnavailableReason::Unreadable, "unreadable"),
    ] {
        let mut harness = harness(
            vec![loaded(
                1,
                "gui-file-1",
                WorkspaceFileContent {
                    path: PATH.to_string(),
                    size: None,
                    sha256: None,
                    content: WorkspaceFileBody::Unavailable { reason },
                },
            )],
            true,
        );
        let file = harness
            .adapter
            .read_workspace_file_and_wait("gui-file-1", None, PATH, TIMEOUT)
            .expect("read")
            .file
            .expect("an unavailable body is still an answer");
        assert_eq!(file.body, "unavailable");
        assert_eq!(file.reason, Some(expected));
        // There was no file to measure and no bytes to hash. Publishing `0`
        // and `""` would render as a real, empty file.
        assert_eq!(file.size, None);
        assert_eq!(file.sha256, None);
    }
}

#[test]
fn a_path_that_leaves_the_target_is_refused_by_cores_validator_before_anything_is_sent() {
    let mut harness = harness(Vec::new(), true);
    let error = harness
        .adapter
        .read_workspace_file_and_wait("gui-file-1", None, "../../etc/passwd", TIMEOUT)
        .expect_err("a traversal is refused, never normalized");
    // Core's own wording: the same sentence an operator would meet from the
    // runtime, so one rule does not read as two problems.
    assert!(error.contains("leaves the target"), "{error}");
    assert!(
        reads(&harness.sent).is_empty(),
        "a refused path never reaches the wire"
    );
    assert_eq!(
        harness.adapter.workspace_file().outcome.state,
        "idle",
        "a local refusal is not Core refusing a read it never saw"
    );
}

#[test]
fn a_permission_refusal_is_cores_own_sentence_and_loads_nothing() {
    let mut harness = harness(
        vec![refused(
            1,
            "gui-file-1",
            "read_file for `crates/types/src/lib.rs` is denied by rule `deny: crates/**`\nhint: \
             allow the path in viden.toml",
        )],
        true,
    );
    let projection = harness
        .adapter
        .read_workspace_file_and_wait("gui-file-1", None, PATH, TIMEOUT)
        .expect("a refusal is an answer, not a transport failure");
    assert_eq!(projection.outcome.state, "rejected");
    let reason = projection.outcome.reason.expect("Core's reason");
    assert!(reason.contains("denied by rule"), "{reason}");
    // The hint travels with the reason: it is the only part of the refusal the
    // operator can act on.
    assert!(reason.contains("allow the path in viden.toml"), "{reason}");
    assert!(
        projection.file.is_none(),
        "a refused read shows no file, and never the previous one"
    );
    assert_eq!(projection.requested_path.as_deref(), Some(PATH));
}

#[test]
fn an_answer_naming_another_read_is_never_rendered_under_this_path() {
    let mut harness = harness(
        vec![loaded(
            1,
            "gui-file-other",
            text_file("someone else's file", false),
        )],
        true,
    );
    let projection = harness
        .adapter
        .read_workspace_file_and_wait("gui-file-1", None, PATH, TIMEOUT)
        .expect("read");
    assert_eq!(projection.outcome.state, "pending");
    assert!(projection.file.is_none());
    assert_eq!(
        projection.pending_command_id.as_deref(),
        Some("gui-file-1"),
        "the read is still out; the answer belonged to another one"
    );
}

#[test]
fn an_unrelated_error_never_fails_an_outstanding_read() {
    let mut harness = harness(vec![unrelated_error(1, "a lane failed to start")], true);
    let projection = harness
        .adapter
        .read_workspace_file_and_wait("gui-file-1", None, PATH, TIMEOUT)
        .expect("read");
    // `Error` carries no command id, so treating one as this read's refusal
    // would fabricate a refusal Core never issued.
    assert_eq!(projection.outcome.state, "pending");
    assert!(projection.file.is_none());
}

#[test]
fn a_second_read_is_refused_locally_while_one_is_in_flight() {
    let mut harness = harness(Vec::new(), true);
    harness
        .adapter
        .read_workspace_file_and_wait("gui-file-1", None, PATH, TIMEOUT)
        .expect("first read");
    let error = harness
        .adapter
        .read_workspace_file_and_wait("gui-file-2", None, "README.md", TIMEOUT)
        .expect_err("one read at a time");
    assert!(error.contains("gui-file-1"), "{error}");
    assert_eq!(reads(&harness.sent).len(), 1);
}

#[test]
fn a_new_read_drops_the_previous_files_bytes_before_it_leaves() {
    let mut harness = harness(
        vec![loaded(1, "gui-file-1", text_file("first file", false))],
        true,
    );
    harness
        .adapter
        .read_workspace_file_and_wait("gui-file-1", None, PATH, TIMEOUT)
        .expect("first read");
    let projection = harness
        .adapter
        .read_workspace_file_and_wait("gui-file-2", None, "README.md", TIMEOUT)
        .expect("second read");
    // The pending state must not show the previous file under the new path's
    // name, which is what a held receipt would do.
    assert_eq!(projection.outcome.state, "pending");
    assert!(projection.file.is_none());
    assert_eq!(projection.requested_path.as_deref(), Some("README.md"));
}
