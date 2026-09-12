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
use crate::patch::{LocalPatchBackend, PatchBackend, PatchRequest, conflict_content};

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

/*
 * H2 hygiene — compatibility follow-up 1: "strict apply silently drops binary
 * files from a patch".
 *
 * `parse_unified_diff` discarded any file it had scanned no rows for, which is
 * every binary file Git reports as `Binary files … differ` or `GIT binary
 * patch`. A mixed patch therefore applied its text files and said *nothing at
 * all* about the binary ones — the operator read "applied" and had no way to
 * learn that part of the change was never written. That silence is also why
 * `ConflictHunkReason::Binary` had no producer.
 *
 * The outcome is now stated per file, in the shape the types already model: the
 * text files still apply, and the apply result names the binary file in
 * `conflicts`. Nothing new is invented — no event, no capability, no fixture.
 */

/// A `git diff` over one text file and one binary file, exactly as Git writes
/// it: the binary section carries the `Binary files` marker and no `@@` at all.
const MIXED_BINARY_DIFF: &str = concat!(
    "diff --git a/notes.txt b/notes.txt\n",
    "--- a/notes.txt\n",
    "+++ b/notes.txt\n",
    "@@ -1,1 +1,1 @@\n",
    "-one\n",
    "+ONE\n",
    "diff --git a/logo.png b/logo.png\n",
    "index 1111111..2222222 100644\n",
    "Binary files a/logo.png and b/logo.png differ\n",
);

#[test]
fn a_binary_file_in_a_mixed_patch_is_reported_while_the_text_file_still_applies() {
    let cwd = temp_dir("patch_binary_mixed");
    fs::write(cwd.join("notes.txt"), "one\n").unwrap();
    fs::write(cwd.join("logo.png"), [0x89, 0x50, 0x4e, 0x47]).unwrap();

    let outcome = LocalPatchBackend
        .apply(&request(&cwd, MIXED_BINARY_DIFF))
        .expect("a binary file is a stated outcome, not a failed apply");

    // The text file is applied, because refusing the whole patch for a file
    // this apply cannot read would be a different and larger change.
    assert!(outcome.applied, "{outcome:?}");
    assert_eq!(fs::read_to_string(cwd.join("notes.txt")).unwrap(), "ONE\n");
    assert_eq!(outcome.writes.len(), 1, "{outcome:?}");
    assert!(
        outcome.writes[0].ends_with("notes.txt"),
        "only the text file is written: {outcome:?}"
    );

    // And the binary file is named, with a reason, instead of vanishing.
    assert_eq!(outcome.conflicts.len(), 1, "{outcome:?}");
    assert_eq!(
        outcome.conflicts[0].path,
        std::path::PathBuf::from("logo.png")
    );
    assert!(
        outcome.conflicts[0].message.contains("binary"),
        "{:?}",
        outcome.conflicts[0]
    );

    // The binary bytes are untouched: a refusal does not half-write a file.
    assert_eq!(
        fs::read(cwd.join("logo.png")).unwrap(),
        vec![0x89, 0x50, 0x4e, 0x47]
    );
}

/// `check` is the dry run the approval surfaces read. It must name the same
/// file the apply would, or an operator would approve a patch whose binary
/// half only appears afterwards.
#[test]
fn the_dry_run_names_the_binary_file_before_anything_is_written() {
    let cwd = temp_dir("patch_binary_check");
    fs::write(cwd.join("notes.txt"), "one\n").unwrap();

    let outcome = LocalPatchBackend
        .check(&request(&cwd, MIXED_BINARY_DIFF))
        .expect("a dry run over a mixed patch must answer");

    assert!(!outcome.applied);
    assert_eq!(outcome.writes.len(), 1, "{outcome:?}");
    assert_eq!(outcome.conflicts.len(), 1, "{outcome:?}");
    assert_eq!(
        outcome.conflicts[0].path,
        std::path::PathBuf::from("logo.png")
    );
    assert_eq!(fs::read_to_string(cwd.join("notes.txt")).unwrap(), "one\n");
}

/// A patch that is *only* binary applied nothing, so it must not report
/// `applied`. The old path returned "no unified diff patch found", which named
/// no file and read as a malformed patch rather than as one this apply cannot
/// carry.
#[test]
fn an_all_binary_patch_applies_nothing_and_still_names_its_files() {
    let cwd = temp_dir("patch_binary_only");
    let diff = concat!(
        "diff --git a/logo.png b/logo.png\n",
        "GIT binary patch\n",
        "literal 4\n",
    );

    let outcome = LocalPatchBackend
        .apply(&request(&cwd, diff))
        .expect("an all-binary patch is answered, not an error");

    assert!(!outcome.applied, "{outcome:?}");
    assert!(outcome.writes.is_empty(), "{outcome:?}");
    assert_eq!(outcome.conflicts.len(), 1, "{outcome:?}");
    assert_eq!(
        outcome.conflicts[0].path,
        std::path::PathBuf::from("logo.png")
    );
}

/// The variant the doc comment said had no producer. `ConflictHunkReason::
/// Binary` is what a client renders for a file with no lines to match, and the
/// hunk carries `base: None` — "no preimage at all", which is a different fact
/// from the `Some(vec![])` a creation expects.
#[test]
fn a_binary_file_publishes_conflict_content_with_the_binary_reason() {
    let cwd = temp_dir("patch_binary_conflict_content");
    fs::write(cwd.join("notes.txt"), "one\n").unwrap();

    let content = conflict_content(&request(&cwd, MIXED_BINARY_DIFF), ConflictBaseline::Unknown)
        .expect("a binary file Core cannot patch must publish content");

    let binary = content
        .files
        .iter()
        .find(|file| file.path == "logo.png")
        .expect("the binary file is published");
    assert_eq!(binary.hunks.len(), 1);
    assert_eq!(binary.hunks[0].reason, ConflictHunkReason::Binary);
    assert!(binary.hunks[0].ours.is_empty());
    assert!(binary.hunks[0].theirs.is_empty());
    assert_eq!(binary.hunks[0].base, None);
    assert!(!binary.omitted);
}

/// A file with no rows that is *not* binary is still dropped, as before: the
/// scanner produces one for a header-only section Git writes for a mode change
/// with no content change, and inventing a refusal for it would report a
/// conflict that does not exist.
#[test]
fn a_header_only_section_that_is_not_binary_is_still_not_reported() {
    let cwd = temp_dir("patch_mode_only");
    fs::write(cwd.join("notes.txt"), "one\n").unwrap();
    let diff = concat!(
        "diff --git a/notes.txt b/notes.txt\n",
        "--- a/notes.txt\n",
        "+++ b/notes.txt\n",
        "@@ -1,1 +1,1 @@\n",
        "-one\n",
        "+ONE\n",
        "diff --git a/script.sh b/script.sh\n",
        "old mode 100644\n",
        "new mode 100755\n",
    );

    let outcome = LocalPatchBackend
        .apply(&request(&cwd, diff))
        .expect("a mode-only section does not fail the apply");

    assert!(outcome.applied, "{outcome:?}");
    assert!(outcome.conflicts.is_empty(), "{outcome:?}");
}
