//! What the strict patch apply refused, as lines (`runtime.conflict_content`).
//!
//! These tests are the reason-classification table: one case per
//! `ConflictHunkReason` the local apply can produce, plus the two silences
//! that matter more than any of them — a patch that applies cleanly and a
//! patch nothing could be learned from both report *nothing* rather than an
//! empty conflict. `None` means Core has nothing to show.

use std::fs;

use viden_types::{ConflictBaseline, ConflictHunkReason};

use super::temp_dir;
use crate::patch::{LocalPatchBackend, PatchRequest, conflict_content};

fn request(cwd: &std::path::Path, unified_diff: &str) -> PatchRequest {
    PatchRequest {
        cwd: cwd.to_path_buf(),
        unified_diff: unified_diff.to_string(),
    }
}

/// The whole point of the capability: a rejected hunk publishes where the file
/// stands, what the patch carried, and what the patch expected to find, each
/// separately addressable.
#[test]
fn a_context_mismatch_reports_ours_theirs_and_the_preimage() {
    let cwd = temp_dir("conflict_context_mismatch");
    fs::write(cwd.join("a.txt"), "one\ntwo\nthree\n").unwrap();
    let diff = "--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,3 @@\n one\n-TWO\n+two point five\n three\n";

    let content = conflict_content(&request(&cwd, diff), ConflictBaseline::Unknown)
        .expect("a rejected hunk must publish content");

    assert!(!content.truncated);
    assert_eq!(content.files.len(), 1);
    let file = &content.files[0];
    assert_eq!(file.path, "a.txt");
    assert!(!file.omitted);
    assert_eq!(file.hunks.len(), 1);
    let hunk = &file.hunks[0];
    assert_eq!(hunk.reason, ConflictHunkReason::ContextMismatch);
    assert_eq!(hunk.ours_start, 1);
    assert_eq!(hunk.ours, vec!["one\n", "two\n", "three\n"]);
    assert_eq!(hunk.theirs_start, 1);
    assert_eq!(hunk.theirs, vec!["one\n", "two point five\n", "three\n"]);
    assert_eq!(
        hunk.base.as_deref(),
        Some(
            [
                "one\n".to_string(),
                "TWO\n".to_string(),
                "three\n".to_string()
            ]
            .as_slice()
        )
    );
}

/// A change that is already in the file is a different recovery from a context
/// mismatch — re-applying it is pointless, not dangerous — so it is classified
/// rather than folded into the generic reason.
#[test]
fn a_hunk_already_in_the_file_is_reported_as_already_applied() {
    let cwd = temp_dir("conflict_already_applied");
    fs::write(cwd.join("a.txt"), "one\ntwo point five\nthree\n").unwrap();
    let diff = "--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,3 @@\n one\n-two\n+two point five\n three\n";

    let content = conflict_content(&request(&cwd, diff), ConflictBaseline::Unknown)
        .expect("a rejected hunk must publish content");

    assert_eq!(
        content.files[0].hunks[0].reason,
        ConflictHunkReason::AlreadyApplied
    );
}

/// The file is absent, so every hunk failed for that one fact and `ours` is
/// empty for all of them. Empty because the file is gone is still empty — the
/// reason is what tells the two apart.
#[test]
fn a_patch_against_a_missing_file_reports_every_hunk_as_file_missing() {
    let cwd = temp_dir("conflict_file_missing");
    let diff = "--- a/gone.txt\n+++ b/gone.txt\n@@ -1,1 +1,1 @@\n-old\n+new\n@@ -5,1 +5,1 @@\n-five\n+FIVE\n";

    let content = conflict_content(&request(&cwd, diff), ConflictBaseline::Unknown)
        .expect("a missing target must publish content");

    let file = &content.files[0];
    assert_eq!(file.path, "gone.txt");
    assert_eq!(file.hunks.len(), 2);
    for hunk in &file.hunks {
        assert_eq!(hunk.reason, ConflictHunkReason::FileMissing);
        assert!(hunk.ours.is_empty());
    }
}

/// A deletion whose preimage does not account for the whole file would leave
/// the file behind. That is not a context mismatch: the hunk matched.
#[test]
fn a_deletion_that_would_leave_the_file_behind_is_reported_as_file_deleted() {
    let cwd = temp_dir("conflict_file_deleted");
    fs::write(cwd.join("a.txt"), "one\ntwo\n").unwrap();
    let diff = "--- a/a.txt\n+++ /dev/null\n@@ -1,1 +0,0 @@\n-one\n";

    let content = conflict_content(&request(&cwd, diff), ConflictBaseline::Unknown)
        .expect("an incomplete deletion must publish content");

    assert_eq!(
        content.files[0].hunks[0].reason,
        ConflictHunkReason::FileDeleted
    );
}

/// A creation hunk expects an empty region, so its preimage is `Some(vec![])`
/// — the empty region it expected — and never `None`, which would say the hunk
/// had no preimage to show at all.
#[test]
fn a_creation_over_an_existing_file_reports_an_empty_preimage() {
    let cwd = temp_dir("conflict_create_over_existing");
    fs::write(cwd.join("new.txt"), "squatter\n").unwrap();
    let diff = "--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1,1 @@\n+fresh\n";

    let content = conflict_content(&request(&cwd, diff), ConflictBaseline::Unknown)
        .expect("a creation over an existing file must publish content");

    let hunk = &content.files[0].hunks[0];
    assert_eq!(hunk.reason, ConflictHunkReason::ContextMismatch);
    assert_eq!(hunk.ours, vec!["squatter\n"]);
    assert_eq!(hunk.theirs, vec!["fresh\n"]);
    assert_eq!(hunk.base.as_deref(), Some([].as_slice()));
}

/// A patch that applies is not a conflict, and a patch nothing could be
/// learned from is not one either. Both answer `None`, because publishing an
/// empty `ConflictContent` would tell a client "we looked and found nothing
/// colliding" when the truth is "we have nothing to show".
#[test]
fn a_clean_or_unreadable_patch_publishes_nothing() {
    let cwd = temp_dir("conflict_none");
    fs::write(cwd.join("a.txt"), "one\n").unwrap();
    let clean = "--- a/a.txt\n+++ b/a.txt\n@@ -1,1 +1,1 @@\n-one\n+ONE\n";
    assert!(conflict_content(&request(&cwd, clean), ConflictBaseline::Unknown).is_none());

    assert!(
        conflict_content(
            &request(&cwd, "not a diff at all\n"),
            ConflictBaseline::Unknown
        )
        .is_none()
    );

    let missing_root = cwd.join("no-such-dir");
    assert!(conflict_content(&request(&missing_root, clean), ConflictBaseline::Unknown).is_none());
}

/// The bound drops a file's lines but never the file: a reviewer who reads
/// "unchanged" where the truth is "not shown" is exactly the failure the flag
/// exists to prevent.
#[test]
fn the_byte_bound_omits_a_file_and_marks_the_content_truncated() {
    let cwd = temp_dir("conflict_bound");
    let big = "x".repeat(300 * 1024);
    fs::write(cwd.join("big.txt"), "something else entirely\n").unwrap();
    let diff = format!("--- a/big.txt\n+++ b/big.txt\n@@ -1,1 +1,1 @@\n-{big}\n+{big}y\n");

    let content = conflict_content(&request(&cwd, &diff), ConflictBaseline::Unknown)
        .expect("an over-bound conflict must still name its file");

    assert!(content.truncated);
    assert_eq!(content.files.len(), 1);
    assert!(content.files[0].omitted);
    assert!(content.files[0].hunks.is_empty());
    assert_eq!(content.files[0].path, "big.txt");
}

/// Core never invents a baseline. The producer publishes exactly what the
/// caller knew the conflict was computed against.
#[test]
fn the_baseline_is_the_callers_and_is_never_invented() {
    let cwd = temp_dir("conflict_baseline");
    fs::write(cwd.join("a.txt"), "one\n").unwrap();
    let diff = "--- a/a.txt\n+++ b/a.txt\n@@ -1,1 +1,1 @@\n-nope\n+ONE\n";
    let baseline = ConflictBaseline::Revision {
        sha: "ab".repeat(20),
    };

    let content = conflict_content(&request(&cwd, diff), baseline.clone())
        .expect("a rejected hunk must publish content");

    assert_eq!(content.baseline, baseline);
}

/// Callers that only want the reason must keep reading exactly what they read
/// before: the structured detail rides alongside the error, it does not
/// replace it.
#[test]
fn the_strict_apply_still_returns_its_original_conflict_message() {
    let cwd = temp_dir("conflict_error_string");
    fs::write(cwd.join("a.txt"), "one\n").unwrap();
    let diff = "--- a/a.txt\n+++ b/a.txt\n@@ -1,1 +1,1 @@\n-nope\n+ONE\n";

    let error = LocalPatchBackend
        .prepare(&request(&cwd, diff))
        .expect_err("a mismatched hunk must still fail the strict apply");

    assert!(
        error
            .to_string()
            .contains("patch conflict: expected hunk context was not found"),
        "{error}"
    );
}
