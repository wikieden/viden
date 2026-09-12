//! Runtime behavior of the permission-gated single-file read
//! (`runtime.workspace_file_reads`, C9).
//!
//! The inventory beside this one proves a path exists. This proves what is in
//! it, and four things must hold for a frontend that is forbidden from opening
//! files itself to be able to trust the answer:
//!
//! 1. the gate runs on the *real* registry `read_file` spec before a byte is
//!    read, and every refusal is a `CommandRejected` naming this exact read
//!    rather than an empty or unavailable body;
//! 2. a path that cannot mean what it says is refused, never repaired into a
//!    different file or answered as `not_found`;
//! 3. every body variant is reachable and distinguishable — text, a truncated
//!    text cut on a character boundary, binary, missing, a directory, and an
//!    unreadable symlink that leaves the target;
//! 4. `sha256` is over the whole file and `size` is the file's length, so a
//!    truncated answer still identifies the revision it came from.

use std::fs;

use sha2::Digest;

use viden_types::{
    PermissionBehavior, PermissionRule, PermissionRuleSource, PermissionRuleValue, RuntimeCommand,
    RuntimeEventKind, SourceTarget, WorkspaceFileBody, WorkspaceFileContent,
    WorkspaceFileReadQuery, WorkspaceFileUnavailableReason,
};

use super::{SequenceProvider, temp_dir};
use crate::SessionEngine;

/// Builds a workspace holding one file of each body the contract can publish.
fn workspace_engine(name: &str) -> (std::path::PathBuf, SessionEngine) {
    let cwd = temp_dir(&format!("{name}_cwd"));
    let home = temp_dir(&format!("{name}_home"));
    fs::create_dir_all(cwd.join("src")).unwrap();
    fs::write(cwd.join("README.md"), "readme bytes\n").unwrap();
    // A multi-byte character straddling the byte bound below, so the cut has
    // to land on a character boundary rather than mid-sequence.
    fs::write(cwd.join("src/unicode.txt"), "ab\u{00e9}cd").unwrap();
    fs::write(cwd.join("src/binary.bin"), [0x00, 0x01, 0x02, 0x03]).unwrap();
    // Valid bytes for 8 KiB, then an invalid UTF-8 sequence: the NUL sniff
    // window is clean, so only the decode can classify this one.
    let mut late_invalid = vec![b'a'; 9 * 1024];
    late_invalid.extend_from_slice(&[0xff, 0xfe]);
    fs::write(cwd.join("src/late-invalid.txt"), late_invalid).unwrap();
    fs::create_dir_all(cwd.join("docs")).unwrap();
    let engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    (cwd, engine)
}

fn read_file_rule(rule_behavior: PermissionBehavior) -> PermissionRule {
    PermissionRule {
        source: PermissionRuleSource::Session,
        rule_behavior,
        rule_value: PermissionRuleValue {
            tool_name: "read_file".to_string(),
            rule_content: None,
        },
    }
}

fn read(
    engine: &mut SessionEngine,
    command_id: &str,
    query: WorkspaceFileReadQuery,
) -> Vec<viden_types::RuntimeEvent> {
    let mut denier = |_prompt| panic!("a read-only file read must not request approval");
    engine
        .handle_runtime_command(
            command_id,
            RuntimeCommand::ReadWorkspaceFile { query },
            &mut denier,
        )
        .unwrap()
}

fn query(path: &str) -> WorkspaceFileReadQuery {
    WorkspaceFileReadQuery {
        target: SourceTarget::Workspace,
        path: path.to_string(),
        byte_limit: None,
    }
}

fn answer(events: &[viden_types::RuntimeEvent], expected_command_id: &str) -> WorkspaceFileContent {
    let loaded = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::WorkspaceFileLoaded { command_id, file } => {
                Some((command_id.clone(), file.clone()))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a WorkspaceFileLoaded, got {events:?}"));
    assert_eq!(
        loaded.0, expected_command_id,
        "the answer must name the exact read it belongs to"
    );
    loaded.1
}

fn rejection_reason(events: &[viden_types::RuntimeEvent], command_id: &str) -> String {
    assert!(
        !events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::WorkspaceFileLoaded { .. })),
        "a refused read must publish no answer at all, got {events:?}"
    );
    events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::CommandRejected {
                command_id: rejected,
                reason,
            } if rejected == command_id => Some(reason.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected CommandRejected for {command_id}, got {events:?}"))
}

#[test]
fn a_text_file_read_publishes_its_bytes_with_the_whole_file_hash_and_its_command_id() {
    let (_cwd, mut engine) = workspace_engine("file_read_text");
    let events = read(&mut engine, "file-read-1", query("README.md"));
    assert!(matches!(
        &events[0].kind,
        RuntimeEventKind::CommandAccepted { command_id, .. } if command_id == "file-read-1"
    ));
    let file = answer(&events, "file-read-1");
    assert_eq!(file.path, "README.md");
    assert_eq!(file.size, Some("readme bytes\n".len() as u64));
    assert_eq!(
        file.sha256.as_deref(),
        Some(format!("{:x}", sha2::Sha256::digest(b"readme bytes\n")).as_str()),
        "the hash is over the whole file, not the published body"
    );
    assert_eq!(
        file.content,
        WorkspaceFileBody::Text {
            text: "readme bytes\n".to_string(),
            truncated: false,
        }
    );
}

/// The answer echoes the normalized path, so a client that asked with
/// `./src/x` and one that asked with `src/x` key their view on one spelling.
#[test]
fn a_file_read_echoes_the_normalized_path() {
    let (_cwd, mut engine) = workspace_engine("file_read_normalized");
    let file = answer(
        &read(&mut engine, "file-read-norm", query("./src//unicode.txt")),
        "file-read-norm",
    );
    assert_eq!(file.path, "src/unicode.txt");
}

/// The bound cuts on a character boundary, never mid-sequence, and says so.
/// `size` and `sha256` still describe the whole file, which is what lets a
/// client tell a truncated view of revision A from a full view of revision B.
#[test]
fn a_truncated_text_read_cuts_on_a_character_boundary_and_says_so() {
    let (_cwd, mut engine) = workspace_engine("file_read_truncated");
    // "ab\u{00e9}cd" is a b <2-byte é> c d: a 3-byte bound lands inside the é.
    let file = answer(
        &read(
            &mut engine,
            "file-read-cut",
            WorkspaceFileReadQuery {
                byte_limit: Some(3),
                ..query("src/unicode.txt")
            },
        ),
        "file-read-cut",
    );
    assert_eq!(
        file.content,
        WorkspaceFileBody::Text {
            text: "ab".to_string(),
            truncated: true,
        },
        "the cut must land on a character boundary and never split the é"
    );
    assert_eq!(file.size, Some("ab\u{00e9}cd".len() as u64));
    assert_eq!(
        file.sha256.as_deref(),
        Some(format!("{:x}", sha2::Sha256::digest("ab\u{00e9}cd".as_bytes())).as_str()),
    );
}

/// A read whose bound happens to equal the file length is *not* truncated:
/// `truncated` says the bound cut the file, never that the file was short.
#[test]
fn a_read_bounded_exactly_at_the_file_length_is_not_truncated() {
    let (_cwd, mut engine) = workspace_engine("file_read_exact");
    let file = answer(
        &read(
            &mut engine,
            "file-read-exact",
            WorkspaceFileReadQuery {
                byte_limit: Some("readme bytes\n".len() as u32),
                ..query("README.md")
            },
        ),
        "file-read-exact",
    );
    assert_eq!(
        file.content,
        WorkspaceFileBody::Text {
            text: "readme bytes\n".to_string(),
            truncated: false,
        }
    );
}

/// A NUL in the sniff window is binary, and binary carries no payload: a
/// client renders size and hash and offers no editor rather than rendering
/// replacement characters as though they were content.
#[test]
fn a_file_with_a_nul_byte_is_published_as_binary_with_no_payload() {
    let (_cwd, mut engine) = workspace_engine("file_read_binary");
    let file = answer(
        &read(&mut engine, "file-read-bin", query("src/binary.bin")),
        "file-read-bin",
    );
    assert_eq!(file.content, WorkspaceFileBody::Binary);
    assert_eq!(file.size, Some(4));
    assert!(file.sha256.is_some(), "binary bytes are still identifiable");
}

/// Bytes that pass the NUL sniff but do not decode are binary too. Publishing
/// them as text would mean lossy replacement characters on the wire, which a
/// client cannot tell from content the file actually holds.
#[test]
fn a_file_whose_bytes_do_not_decode_is_published_as_binary() {
    let (_cwd, mut engine) = workspace_engine("file_read_invalid");
    let file = answer(
        &read(&mut engine, "file-read-inv", query("src/late-invalid.txt")),
        "file-read-inv",
    );
    assert_eq!(file.content, WorkspaceFileBody::Binary);
    assert_eq!(file.size, Some(9 * 1024 + 2));
}

/// A missing path, a directory, and a symlink that leaves the target are three
/// facts and never one. None is a rejection: the read was legal and Core
/// answered it truthfully.
#[test]
fn a_missing_path_and_a_directory_are_typed_unavailable_answers_not_refusals() {
    let (_cwd, mut engine) = workspace_engine("file_read_absent");
    let missing = answer(
        &read(&mut engine, "file-read-missing", query("src/nope.rs")),
        "file-read-missing",
    );
    assert_eq!(
        missing.content,
        WorkspaceFileBody::Unavailable {
            reason: WorkspaceFileUnavailableReason::NotFound,
        }
    );
    assert_eq!(
        (missing.size, missing.sha256),
        (None, None),
        "there is no file to measure and no bytes to hash; 0 and \"\" would render as an empty file"
    );
    assert_eq!(missing.path, "src/nope.rs");

    let directory = answer(
        &read(&mut engine, "file-read-dir", query("docs")),
        "file-read-dir",
    );
    assert_eq!(
        directory.content,
        WorkspaceFileBody::Unavailable {
            reason: WorkspaceFileUnavailableReason::Directory,
        }
    );
    assert_eq!((directory.size, directory.sha256), (None, None));
}

/// A symlink whose real location is outside the resolved target is refused as
/// `Unreadable` rather than followed. Following it would serve a file from
/// outside the tree the permission gate authorized — the path rule and the
/// target root are the only things scoping this read, because `read_file`'s
/// own `resolve_path` scopes nothing.
#[test]
#[cfg(unix)]
fn a_symlink_that_leaves_the_target_is_unreadable_rather_than_followed() {
    let (cwd, mut engine) = workspace_engine("file_read_symlink");
    let outside = temp_dir("file_read_symlink_outside");
    fs::write(outside.join("secret.txt"), "outside bytes").unwrap();
    std::os::unix::fs::symlink(outside.join("secret.txt"), cwd.join("escape.txt")).unwrap();
    let file = answer(
        &read(&mut engine, "file-read-link", query("escape.txt")),
        "file-read-link",
    );
    assert_eq!(
        file.content,
        WorkspaceFileBody::Unavailable {
            reason: WorkspaceFileUnavailableReason::Unreadable,
        },
        "a link out of the target must never be followed"
    );
    assert_eq!((file.size, file.sha256), (None, None));

    // A link that stays inside resolves normally: the rule is containment,
    // not a blanket refusal of links.
    std::os::unix::fs::symlink(cwd.join("README.md"), cwd.join("inside.txt")).unwrap();
    let inside = answer(
        &read(&mut engine, "file-read-link-ok", query("inside.txt")),
        "file-read-link-ok",
    );
    assert_eq!(
        inside.content,
        WorkspaceFileBody::Text {
            text: "readme bytes\n".to_string(),
            truncated: false,
        }
    );
}

/// A path that leaves the target is refused before anything is resolved, and
/// never answered as `not_found`: "you may not ask that" and "your tree does
/// not contain it" are different facts, and the second is a claim about the
/// operator's own files that Core has not checked and must not make.
#[test]
fn a_path_that_leaves_the_target_is_rejected_rather_than_answered_as_missing() {
    let (_cwd, mut engine) = workspace_engine("file_read_escape");
    for (command_id, path) in [
        ("file-read-esc-1", "../secrets.txt"),
        ("file-read-esc-2", "src/../../secrets.txt"),
        ("file-read-esc-3", "/etc/passwd"),
        ("file-read-esc-4", "src\\lib.rs"),
        ("file-read-esc-5", ""),
    ] {
        let events = read(&mut engine, command_id, query(path));
        let reason = rejection_reason(&events, command_id);
        assert!(
            !reason.contains("not_found"),
            "a refusal must not masquerade as a missing file, got {reason}"
        );
    }
}

/// The gate runs on the real registry `read_file` spec before a byte is read,
/// and a deny is a rejection naming that tool with the grant hint folded in —
/// never an `Unavailable` body, which a client renders as a fact about the
/// file rather than as a decision about the operator.
#[test]
fn a_denied_file_read_is_rejected_with_the_read_file_rule_named() {
    let (_cwd, mut engine) = workspace_engine("file_read_denied");
    engine.add_permission_rule_for_test(read_file_rule(PermissionBehavior::Deny));
    let events = read(&mut engine, "file-read-denied", query("README.md"));
    let reason = rejection_reason(&events, "file-read-denied");
    assert!(
        reason.contains("read_file"),
        "the reason must name the permission that refused, got {reason}"
    );
    assert!(
        reason.contains("grant the `read_file` permission"),
        "the reason must keep the actionable grant hint, got {reason}"
    );
}

/// An unresolved ask is a refusal too. This read answers a keystroke, not an
/// interactive turn: parking a client's Code tab behind a modal would stall
/// the cockpit, so the approver is never reached.
#[test]
fn an_ask_rule_on_a_file_read_is_rejected_without_blocking_on_approval() {
    let (_cwd, mut engine) = workspace_engine("file_read_ask");
    engine.add_permission_rule_for_test(read_file_rule(PermissionBehavior::Ask));
    let events = read(&mut engine, "file-read-ask", query("README.md"));
    let reason = rejection_reason(&events, "file-read-ask");
    assert!(
        reason.contains("read_file"),
        "an unresolved ask must surface as a named refusal, got {reason}"
    );
}

/// Plan mode still answers: `read_file` mutates nothing, so the engine's
/// safe-read branch covers it and a planning operator can still read.
#[test]
fn plan_mode_still_answers_a_file_read() {
    let (_cwd, mut engine) = workspace_engine("file_read_plan");
    engine.set_work_mode(viden_types::WorkMode::Plan).unwrap();
    let file = answer(
        &read(&mut engine, "file-read-plan", query("README.md")),
        "file-read-plan",
    );
    assert!(matches!(file.content, WorkspaceFileBody::Text { .. }));
}

/// A Lane target is resolved from Core's own records, and a Lane no record
/// names is a rejection rather than a silent read of the workspace root, which
/// would serve one tree's file under another tree's identity.
#[test]
fn an_unknown_lane_target_is_rejected_rather_than_read_from_the_workspace() {
    let (_cwd, mut engine) = workspace_engine("file_read_lane");
    let events = read(
        &mut engine,
        "file-read-lane",
        WorkspaceFileReadQuery {
            target: SourceTarget::Lane {
                lane_id: "lane_does_not_exist".to_string(),
            },
            ..query("README.md")
        },
    );
    let reason = rejection_reason(&events, "file-read-lane");
    assert!(
        reason.contains("lane_does_not_exist"),
        "the refusal must name the Lane it could not resolve, got {reason}"
    );
}
