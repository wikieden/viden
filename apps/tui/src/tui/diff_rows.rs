//! Approval decision-context rows (`runtime.structured_diff`, GUI-CORE-012).
//!
//! Core is the only producer of diff rows: `crates/types/src/diff.rs` carries
//! the parsed hunks and `crates/tools/src/patch.rs` — the parser that *applies*
//! patches — is what emits them. This module turns that typed shape into
//! terminal rows and does nothing else. It never parses `input_preview`, never
//! counts line numbers itself, and never derives a fact the document did not
//! state.
//!
//! Three rules keep the rendering honest:
//!
//! 1. **Absence is stated, never filled in.** A binary file, a file whose rows
//!    the byte bound dropped, and a document that hit the bound each get their
//!    own row. "Not shown" must never look like "nothing changed".
//! 2. **Git's numbers, not ours.** `old_line`/`new_line` are rendered exactly
//!    where the document has them and left blank on the side a row does not
//!    exist on. A removed line has no new-file number, and printing `0` there
//!    would read as a real line.
//! 3. **Bounded rows, bounded columns.** The overlay is a fixed panel, so the
//!    row list is capped and the remainder is *counted* rather than dropped
//!    silently; paths keep their distinctive tail and content lines are cut at
//!    the end with the registered `…` marker.

use viden_core::{DecisionContext, DiffDocument, DiffFile, DiffHunk, DiffLine, DiffLineKind};

use super::{
    state::TuiState,
    text::{truncate_end, truncate_tail},
};

/// The capability that publishes `ApprovalRequestView.decision_context`.
pub(super) const STRUCTURED_DIFF_CAPABILITY: &str = "runtime.structured_diff";

/// Largest number of diff rows one approval overlay renders.
///
/// The overlay grows with the diff, but not without end: a multi-file merge
/// patch would otherwise push the approval actions off a short terminal, which
/// is the one thing an approval surface may never do. The remainder is counted
/// in a stated row instead of being dropped.
pub(super) const MAX_APPROVAL_DIFF_ROWS: usize = 14;

/// Columns reserved for the two line-number gutters and the change marker.
const GUTTER_WIDTH: usize = 13;

/// Whether this approval carries structured hunks this client may render.
pub(super) fn has_renderable_diff(state: &TuiState, context: Option<&DecisionContext>) -> bool {
    state.has_capability(STRUCTURED_DIFF_CAPABILITY)
        && context.is_some_and(|context| context.diff.is_some())
}

/// The decision-context rows for one approval, already width-fitted.
///
/// Empty when the capability is absent or Core published no diff: the caller
/// then keeps today's `input_preview` row, which is exactly what the contract
/// requires of a client without the extension.
pub(super) fn decision_context_rows(
    state: &TuiState,
    context: Option<&DecisionContext>,
    width: usize,
) -> Vec<String> {
    if !state.has_capability(STRUCTURED_DIFF_CAPABILITY) {
        return Vec::new();
    }
    let Some(context) = context else {
        return Vec::new();
    };
    let Some(document) = context.diff.as_ref() else {
        return Vec::new();
    };
    let mut rows = document_rows(state, document, width);
    let hidden = rows.len().saturating_sub(MAX_APPROVAL_DIFF_ROWS);
    if hidden > 0 {
        rows.truncate(MAX_APPROVAL_DIFF_ROWS);
        rows.push(super::i18n::translate(
            state,
            "approval.diff.more",
            &[("count", &hidden.to_string())],
        ));
    }
    // The base hash is a property of the whole context, not of a file, so it
    // survives the row cap: it is what lets a reader detect afterwards that the
    // file moved between this preview and the execution.
    if let Some(base) = context.base_sha256.as_deref() {
        rows.push(super::i18n::translate(
            state,
            "approval.diff.base",
            &[("sha", short_sha(base))],
        ));
    }
    // One final width fit for every row, including the localized ones: a
    // catalog string is translated text of unknown length, so clamping only the
    // interpolated values would still let a Chinese label overflow the panel.
    rows.into_iter()
        .map(|row| truncate_end(&row, width))
        .collect()
}

/// The first eight characters of a hash, the length an operator can compare.
fn short_sha(sha: &str) -> &str {
    sha.get(..8).unwrap_or(sha)
}

fn document_rows(state: &TuiState, document: &DiffDocument, width: usize) -> Vec<String> {
    let mut rows = Vec::new();
    for file in &document.files {
        rows.extend(file_rows(state, file, width));
    }
    if document.truncated {
        rows.push(super::i18n::translate(
            state,
            "approval.diff.truncated",
            &[("limit", &document.byte_limit.to_string())],
        ));
    }
    rows
}

fn file_rows(state: &TuiState, file: &DiffFile, width: usize) -> Vec<String> {
    let counts = format!("+{} -{}", file.additions, file.deletions);
    let kind = format!("{:?}", file.kind);
    // The path keeps its tail: two files under one directory differ only after
    // the cut, so a head truncation would render both as the same row.
    let path_width = width.saturating_sub(counts.len() + kind.len() + 6).max(8);
    let mut rows = vec![super::i18n::translate(
        state,
        "approval.diff.file",
        &[
            ("path", &truncate_tail(&file.path, path_width)),
            ("kind", &kind),
            ("counts", &counts),
        ],
    )];
    if let Some(old_path) = file.old_path.as_deref() {
        rows.push(super::i18n::translate(
            state,
            "approval.diff.rename",
            &[("old_path", &truncate_tail(old_path, path_width))],
        ));
    }
    if file.binary {
        rows.push(super::i18n::text(state, "approval.diff.binary"));
    }
    if file.omitted {
        rows.push(super::i18n::text(state, "approval.diff.omitted"));
    }
    for hunk in &file.hunks {
        rows.extend(hunk_rows(state, hunk, width));
    }
    rows
}

fn hunk_rows(state: &TuiState, hunk: &DiffHunk, width: usize) -> Vec<String> {
    let header = hunk.header.as_deref().unwrap_or_default();
    let mut rows = vec![super::i18n::translate(
        state,
        "approval.diff.hunk",
        &[
            ("old", &format!("{},{}", hunk.old_start, hunk.old_lines)),
            ("new", &format!("{},{}", hunk.new_start, hunk.new_lines)),
            ("header", &truncate_end(header, width.saturating_sub(28))),
        ],
    )];
    rows.extend(hunk.lines.iter().map(|line| line_row(line, width)));
    rows
}

/// One diff row: the two gutters Git numbered, the marker, and the text.
fn line_row(line: &DiffLine, width: usize) -> String {
    let marker = match line.kind {
        DiffLineKind::Added => "+",
        DiffLineKind::Removed => "-",
        // `#[non_exhaustive]`: a row kind this build does not model is rendered
        // as context rather than mislabelled as an addition or a deletion.
        _ => " ",
    };
    format!(
        "{:>5} {:>5} {marker} {}",
        gutter(line.old_line),
        gutter(line.new_line),
        truncate_end(&line.content, width.saturating_sub(GUTTER_WIDTH))
    )
}

/// A line number, or blank for the side this row does not exist on.
///
/// Blank rather than `0`: the document omits the field precisely because there
/// is no such position, and `0` would render as a real line.
fn gutter(number: Option<u32>) -> String {
    number.map(|value| value.to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use viden_core::{CapabilityId, WorkspaceChangeKind};

    fn state_with_capability() -> TuiState {
        let mut state = TuiState::default();
        state
            .capabilities
            .insert(CapabilityId(STRUCTURED_DIFF_CAPABILITY.to_string()));
        state
    }

    fn line(kind: DiffLineKind, content: &str, old: Option<u32>, new: Option<u32>) -> DiffLine {
        DiffLine {
            kind,
            content: content.to_string(),
            old_line: old,
            new_line: new,
        }
    }

    fn one_file_context() -> DecisionContext {
        DecisionContext {
            diff: Some(DiffDocument {
                files: vec![DiffFile {
                    path: "crates/types/src/diff.rs".to_string(),
                    old_path: None,
                    kind: WorkspaceChangeKind::Modified,
                    binary: false,
                    omitted: false,
                    additions: 1,
                    deletions: 1,
                    hunks: vec![DiffHunk {
                        old_start: 42,
                        old_lines: 3,
                        new_start: 42,
                        new_lines: 3,
                        header: Some("pub struct DiffDocument".to_string()),
                        lines: vec![
                            line(
                                DiffLineKind::Context,
                                "    pub files: Vec",
                                Some(42),
                                Some(42),
                            ),
                            line(
                                DiffLineKind::Removed,
                                "    pub truncated: bool,",
                                Some(43),
                                None,
                            ),
                            line(
                                DiffLineKind::Added,
                                "    pub truncated: bool, // bounded",
                                None,
                                Some(43),
                            ),
                        ],
                    }],
                }],
                truncated: false,
                byte_limit: 65536,
            }),
            base_sha256: Some(
                "3f79bb7b435b05321651daefd374cdc681dc06faa65e374e38337b88ca046dea".to_string(),
            ),
        }
    }

    #[test]
    fn a_missing_capability_or_a_missing_diff_renders_no_rows() {
        let context = one_file_context();
        // No capability: today's preview text stays the whole story.
        assert!(decision_context_rows(&TuiState::default(), Some(&context), 70).is_empty());
        let state = state_with_capability();
        assert!(decision_context_rows(&state, None, 70).is_empty());
        assert!(
            decision_context_rows(
                &state,
                Some(&DecisionContext {
                    diff: None,
                    base_sha256: None
                }),
                70
            )
            .is_empty()
        );
        assert!(!has_renderable_diff(&TuiState::default(), Some(&context)));
        assert!(has_renderable_diff(&state, Some(&context)));
    }

    #[test]
    fn hunk_rows_render_gits_numbers_and_leave_the_absent_side_blank() {
        let state = state_with_capability();
        let rows = decision_context_rows(&state, Some(&one_file_context()), 70);

        assert!(rows[0].contains("diff.rs"), "{:?}", rows[0]);
        assert!(rows[0].contains("+1 -1"), "{:?}", rows[0]);
        assert!(rows[1].contains("-42,3"), "{:?}", rows[1]);
        assert!(rows[1].contains("+42,3"), "{:?}", rows[1]);
        // Context row: both gutters carry Git's numbers.
        assert!(rows[2].starts_with("   42    42  "), "{:?}", rows[2]);
        // Removed: no new-file number, and never a fabricated `0`.
        assert!(rows[3].starts_with("   43       -"), "{:?}", rows[3]);
        // Added: no old-file number.
        assert!(rows[4].starts_with("         43 +"), "{:?}", rows[4]);
        assert!(rows.iter().all(|row| !row.contains(" 0 ")));
    }

    #[test]
    fn the_base_hash_is_published_short_and_survives_the_row_cap() {
        let state = state_with_capability();
        let mut context = one_file_context();
        let document = context.diff.as_mut().expect("diff");
        let file = document.files[0].clone();
        document.files = std::iter::repeat_n(file, 20).collect();

        let rows = decision_context_rows(&state, Some(&context), 70);

        assert_eq!(rows.len(), MAX_APPROVAL_DIFF_ROWS + 2);
        assert!(rows[MAX_APPROVAL_DIFF_ROWS].contains("86"), "{rows:?}");
        assert!(rows.last().expect("base row").contains("3f79bb7b"));
        assert!(
            !rows.last().expect("base row").contains("ca046dea"),
            "the full hash is not an operator-comparable token"
        );
    }

    #[test]
    fn binary_omitted_and_truncated_are_stated_rather_than_rendered_as_empty() {
        let state = state_with_capability();
        let context = DecisionContext {
            diff: Some(DiffDocument {
                files: vec![
                    DiffFile {
                        path: "assets/logo.png".to_string(),
                        old_path: None,
                        kind: WorkspaceChangeKind::Modified,
                        binary: true,
                        omitted: false,
                        additions: 0,
                        deletions: 0,
                        hunks: Vec::new(),
                    },
                    DiffFile {
                        path: "crates/runtime/src/huge.rs".to_string(),
                        old_path: Some("crates/runtime/src/old.rs".to_string()),
                        kind: WorkspaceChangeKind::Renamed,
                        binary: false,
                        omitted: true,
                        additions: 900,
                        deletions: 12,
                        hunks: Vec::new(),
                    },
                ],
                truncated: true,
                byte_limit: 65536,
            }),
            base_sha256: None,
        };

        let rows = decision_context_rows(&state, Some(&context), 70).join("\n");

        assert!(rows.contains("binary"), "{rows}");
        assert!(rows.contains("omitted"), "{rows}");
        assert!(rows.contains("65536"), "{rows}");
        assert!(rows.contains("old.rs"), "{rows}");
        // The real counts survive the omission.
        assert!(rows.contains("+900 -12"), "{rows}");
    }

    #[test]
    fn every_row_fits_the_requested_width_including_a_narrow_overlay() {
        let state = state_with_capability();
        let context = one_file_context();
        for width in [24usize, 40, 70, 132] {
            for row in decision_context_rows(&state, Some(&context), width) {
                assert!(
                    super::super::text::char_width(&row) <= width,
                    "width {width} overflowed: {row}"
                );
            }
        }
    }
}
