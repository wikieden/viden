//! The promoted unified-diff parser: one producer of `DiffDocument`.
//!
//! These tests are property-shaped on purpose. The parser is the only place
//! Core turns diff text into rows, so what matters is that a rendered diff
//! round-trips its counts, that the byte bound degrades a file to `omitted`
//! rather than to silence, and that malformed input never panics.

use viden_types::{DiffLineKind, WorkspaceChangeKind};

use crate::patch::parse_diff_document;
use crate::render_diff;

const UNBOUNDED: u32 = 1024 * 1024;

fn git_diff(body: &str) -> String {
    body.to_string()
}

#[test]
fn a_git_diff_parses_into_files_hunks_and_numbered_lines() {
    let diff = git_diff(
        "diff --git a/src/lib.rs b/src/lib.rs\n\
         --- a/src/lib.rs\n\
         +++ b/src/lib.rs\n\
         @@ -10,3 +10,4 @@ fn main() {\n\
         \x20let kept = 1;\n\
         -let removed = 2;\n\
         +let added = 2;\n\
         +let also_added = 3;\n\
         \x20let tail = 4;\n",
    );
    let document = parse_diff_document(&diff, UNBOUNDED);
    assert!(!document.truncated);
    assert_eq!(document.byte_limit, UNBOUNDED);
    assert_eq!(document.files.len(), 1);
    let file = &document.files[0];
    assert_eq!(file.path, "src/lib.rs");
    assert_eq!(file.old_path, None);
    assert_eq!(file.kind, WorkspaceChangeKind::Modified);
    assert!(!file.binary);
    assert!(!file.omitted);
    assert_eq!(file.additions, 2);
    assert_eq!(file.deletions, 1);

    assert_eq!(file.hunks.len(), 1);
    let hunk = &file.hunks[0];
    assert_eq!(hunk.old_start, 10);
    assert_eq!(hunk.old_lines, 3);
    assert_eq!(hunk.new_start, 10);
    assert_eq!(hunk.new_lines, 4);
    assert_eq!(hunk.header.as_deref(), Some("fn main() {"));

    // Line numbers come from the hunk header, so a context row carries both
    // sides, a removed row carries only the old side, and an added row only
    // the new one. Absence is what tells a client the line does not exist
    // there; `0` would render as a real line.
    let rows = hunk
        .lines
        .iter()
        .map(|line| {
            (
                line.kind,
                line.content.clone(),
                line.old_line,
                line.new_line,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rows,
        vec![
            (
                DiffLineKind::Context,
                "let kept = 1;".to_string(),
                Some(10),
                Some(10)
            ),
            (
                DiffLineKind::Removed,
                "let removed = 2;".to_string(),
                Some(11),
                None
            ),
            (
                DiffLineKind::Added,
                "let added = 2;".to_string(),
                None,
                Some(11)
            ),
            (
                DiffLineKind::Added,
                "let also_added = 3;".to_string(),
                None,
                Some(12)
            ),
            (
                DiffLineKind::Context,
                "let tail = 4;".to_string(),
                Some(12),
                Some(13)
            ),
        ]
    );
}

/// A hunk header with no comma means one line on that side, which is what
/// Git writes for a single-line hunk. Reading it as zero would number every
/// following row wrongly.
#[test]
fn a_hunk_header_without_a_length_means_one_line() {
    let document = parse_diff_document(
        "diff --git a/one.txt b/one.txt\n\
         --- a/one.txt\n\
         +++ b/one.txt\n\
         @@ -7 +7 @@\n\
         -old\n\
         +new\n",
        UNBOUNDED,
    );
    let hunk = &document.files[0].hunks[0];
    assert_eq!((hunk.old_start, hunk.old_lines), (7, 1));
    assert_eq!((hunk.new_start, hunk.new_lines), (7, 1));
    assert_eq!(hunk.header, None);
    assert_eq!(hunk.lines[0].old_line, Some(7));
    assert_eq!(hunk.lines[1].new_line, Some(7));
}

/// The three classifications Core can read straight out of the headers.
#[test]
fn added_deleted_and_renamed_files_are_classified_from_their_headers() {
    let document = parse_diff_document(
        "diff --git a/new.rs b/new.rs\n\
         new file mode 100644\n\
         --- /dev/null\n\
         +++ b/new.rs\n\
         @@ -0,0 +1,1 @@\n\
         +created\n\
         diff --git a/gone.rs b/gone.rs\n\
         deleted file mode 100644\n\
         --- a/gone.rs\n\
         +++ /dev/null\n\
         @@ -1,1 +0,0 @@\n\
         -removed\n\
         diff --git a/old/name.rs b/new/name.rs\n\
         similarity index 90%\n\
         rename from old/name.rs\n\
         rename to new/name.rs\n\
         --- a/old/name.rs\n\
         +++ b/new/name.rs\n\
         @@ -1,1 +1,1 @@\n\
         -before\n\
         +after\n",
        UNBOUNDED,
    );
    let kinds = document
        .files
        .iter()
        .map(|file| (file.path.clone(), file.kind, file.old_path.clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![
            ("new.rs".to_string(), WorkspaceChangeKind::Added, None),
            ("gone.rs".to_string(), WorkspaceChangeKind::Deleted, None),
            (
                "new/name.rs".to_string(),
                WorkspaceChangeKind::Renamed,
                Some("old/name.rs".to_string())
            ),
        ]
    );
}

/// A binary file is part of the change set with no rows to render. `binary`
/// is its own flag because "no hunks" alone is ambiguous — an omitted file
/// has none either.
#[test]
fn a_binary_file_is_published_with_no_hunks_and_its_own_flag() {
    let document = parse_diff_document(
        "diff --git a/logo.png b/logo.png\n\
         index 1111111..2222222 100644\n\
         Binary files a/logo.png and b/logo.png differ\n",
        UNBOUNDED,
    );
    assert_eq!(document.files.len(), 1);
    let file = &document.files[0];
    assert_eq!(file.path, "logo.png");
    assert!(file.binary);
    assert!(file.hunks.is_empty());
    assert!(!file.omitted);
}

/// The byte bound drops rows, never files, and never counts. A reviewer must
/// still see that the file changed and by how much.
#[test]
fn the_byte_bound_omits_hunks_while_the_counts_stay_real() {
    let big = (0..200)
        .map(|index| format!("+line number {index}\n"))
        .collect::<String>();
    let diff = format!(
        "diff --git a/small.txt b/small.txt\n\
         --- a/small.txt\n\
         +++ b/small.txt\n\
         @@ -1,0 +1,1 @@\n\
         +tiny\n\
         diff --git a/big.txt b/big.txt\n\
         --- a/big.txt\n\
         +++ b/big.txt\n\
         @@ -1,0 +1,200 @@\n\
         {big}"
    );
    let document = parse_diff_document(&diff, 64);
    assert!(document.truncated, "the document must admit it is partial");
    assert_eq!(document.byte_limit, 64);
    assert_eq!(document.files.len(), 2);

    let small = &document.files[0];
    assert!(!small.omitted);
    assert_eq!(small.additions, 1);
    assert_eq!(small.hunks.len(), 1);

    let dropped = &document.files[1];
    assert!(
        dropped.omitted,
        "the oversized file must say it is not shown"
    );
    assert!(dropped.hunks.is_empty());
    assert_eq!(
        dropped.additions, 200,
        "an omitted file keeps its real counts, or `not shown` reads as `unchanged`"
    );
    assert_eq!(dropped.deletions, 0);
}

/// Malformed input is a parse result, never a panic: this parser runs on
/// bytes produced by an external `git` and by agent-authored patches.
#[test]
fn malformed_diff_input_never_panics() {
    let cases = [
        "",
        "\n\n\n",
        "@@ -1,1 +1,1 @@\n+orphan hunk\n",
        "diff --git\n",
        "diff --git a/x b/x\n@@ not a header @@\n+row\n",
        "diff --git a/x b/x\n@@ -\u{1f600},\u{1f600} +\u{1f600} @@\n+unicode\n",
        "--- a/x\n",
        "+++ b/x\n",
        "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -4294967296,1 +1,1 @@\n-a\n+b\n",
        "\\ No newline at end of file\n",
    ];
    for case in cases {
        let document = parse_diff_document(case, UNBOUNDED);
        // The only guarantee is a well-formed answer; content varies per case.
        assert!(
            document.files.len() <= 8,
            "case `{case}` produced {document:?}"
        );
    }
}

/// The document is the same fact as the rendered text: parsing what
/// `render_diff` produced reproduces the counts it encoded. This is the
/// property that lets an approval preview and a tool-result patch describe
/// one change rather than two.
#[test]
fn parsing_a_rendered_diff_round_trips_its_counts() {
    let cases = [
        ("", "created\ncontent\n"),
        ("gone\n", ""),
        ("a\nb\nc\n", "a\nB\nc\n"),
        ("a\n", "a\nb\nc\n"),
        ("a\nb\nc\n", "a\n"),
        ("same\n", "same\n"),
    ];
    for (before, after) in cases {
        let rendered = render_diff(before, after);
        let expected_added = rendered
            .lines()
            .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
            .count() as u32;
        let expected_removed = rendered
            .lines()
            .filter(|line| line.starts_with('-') && !line.starts_with("---"))
            .count() as u32;
        let document = parse_diff_document(&rendered, UNBOUNDED);
        if expected_added == 0 && expected_removed == 0 {
            // An unchanged pair renders context only; a file with no change
            // is still a file, and its counts are honestly zero.
            let file = &document.files[0];
            assert_eq!((file.additions, file.deletions), (0, 0));
            continue;
        }
        assert_eq!(document.files.len(), 1, "rendered `{before}` -> `{after}`");
        let file = &document.files[0];
        assert_eq!(
            (file.additions, file.deletions),
            (expected_added, expected_removed),
            "rendered `{before}` -> `{after}`"
        );
        let rows = file
            .hunks
            .iter()
            .map(|hunk| hunk.lines.len())
            .sum::<usize>();
        assert_eq!(
            rows,
            rendered.lines().count() - 2,
            "every rendered row but the two headers becomes one line"
        );
    }
}

/// `render_diff` writes no `@@`, so the promoted parser folds its rows into
/// one implicit whole-file hunk numbered from line 1. Without this the
/// approval preview and the changed-file chip would both parse to zero rows.
#[test]
fn a_rendered_diff_without_a_hunk_header_becomes_one_whole_file_hunk() {
    let document = parse_diff_document(&render_diff("a\nb\n", "a\nB\nc\n"), UNBOUNDED);
    let file = &document.files[0];
    assert_eq!(file.hunks.len(), 1);
    let hunk = &file.hunks[0];
    assert_eq!((hunk.old_start, hunk.new_start), (1, 1));
    assert_eq!(hunk.old_lines, 2);
    assert_eq!(hunk.new_lines, 3);
    assert_eq!(hunk.lines[0].old_line, Some(1));
    assert_eq!(hunk.lines[0].new_line, Some(1));
}
