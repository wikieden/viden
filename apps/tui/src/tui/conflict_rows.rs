//! Structured conflict rows (`runtime.conflict_content`, GUI-CORE-015).
//!
//! A failed merge or lane apply used to publish one prose sentence, so a client
//! could say *that* a patch conflicted and never *what* conflicted. Core now
//! publishes the lines. This module renders them and does nothing else.
//!
//! The invariant that shapes every row here: **this is not a three-way merge.**
//! `ours` is a read-only read of the target file where the hunk said its region
//! was, `theirs` is the incoming hunk's new side, and `base` is that hunk's own
//! preimage — the text the patch expected to find. Core computes no merge base
//! and resolves nothing, so this surface renders three sides, says so in words,
//! and never offers to auto-resolve.
//!
//! Two absences are different facts and are rendered differently: `base: None`
//! means the hunk had no preimage at all, while `Some(vec![])` means it
//! expected an empty region, which is what a creation hunk expects. Collapsing
//! them would tell a reviewer the patch expected nothing when it expected an
//! empty file.

use viden_core::{
    ConflictBaseline, ConflictContent, ConflictFile, ConflictHunk, ConflictHunkReason,
};

use super::{state::TuiState, text::truncate_end, text::truncate_tail};

/// The capability that publishes `ConflictBounce.content` and
/// `LaneConflictView.content`.
pub(super) const CONFLICT_CONTENT_CAPABILITY: &str = "runtime.conflict_content";

/// Largest number of detail rows one conflict modal renders.
///
/// A 256 KiB payload would otherwise scroll past anything an operator can read;
/// the remainder is counted in a stated row rather than dropped in silence.
pub(super) const MAX_CONFLICT_DETAIL_ROWS: usize = 60;

/// The countable facts of one `ConflictContent`, for a one-line row.
///
/// A projection of Core's own payload — counts and the baseline kind — not a
/// second copy of the lines. It exists so the Decision Center can say how big
/// a conflict is without cloning a quarter-megabyte body into every frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ConflictContentSummary {
    pub(super) files: usize,
    pub(super) hunks: usize,
    pub(super) omitted_files: usize,
    pub(super) truncated: bool,
    pub(super) baseline: ConflictBaselineTag,
}

/// What the `ours` side was read against, as a short tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConflictBaselineTag {
    Revision,
    Evidence,
    Unknown,
    /// A baseline kind a newer Core published that this build cannot name.
    /// Reported as unrecognized, never silently as `HEAD`.
    Unrecognized,
}

impl ConflictBaselineTag {
    pub(super) const fn label_key(self) -> &'static str {
        match self {
            Self::Revision => "conflict.baseline.revision",
            Self::Evidence => "conflict.baseline.evidence",
            Self::Unknown => "conflict.baseline.unknown",
            Self::Unrecognized => "conflict.baseline.unrecognized",
        }
    }
}

impl From<&ConflictBaseline> for ConflictBaselineTag {
    fn from(baseline: &ConflictBaseline) -> Self {
        match baseline {
            ConflictBaseline::Revision { .. } => Self::Revision,
            ConflictBaseline::Evidence { .. } => Self::Evidence,
            ConflictBaseline::Unknown => Self::Unknown,
            _ => Self::Unrecognized,
        }
    }
}

impl From<&ConflictContent> for ConflictContentSummary {
    fn from(content: &ConflictContent) -> Self {
        Self {
            files: content.files.len(),
            hunks: content.files.iter().map(|file| file.hunks.len()).sum(),
            omitted_files: content.files.iter().filter(|file| file.omitted).count(),
            truncated: content.truncated,
            baseline: ConflictBaselineTag::from(&content.baseline),
        }
    }
}

/// The one-line conflict fact for a decision row or a transcript entry.
///
/// `None` when the capability is absent or Core published no content, so the
/// caller keeps today's reason-only row. `None` means Core has nothing to show,
/// never that the conflict was empty.
pub(super) fn content_summary_row(
    state: &TuiState,
    summary: Option<ConflictContentSummary>,
) -> Option<String> {
    if !state.has_capability(CONFLICT_CONTENT_CAPABILITY) {
        return None;
    }
    let summary = summary?;
    let mut row = super::i18n::translate(
        state,
        "conflict.summary",
        &[
            ("files", &summary.files.to_string()),
            ("hunks", &summary.hunks.to_string()),
            (
                "baseline",
                &super::i18n::text(state, summary.baseline.label_key()),
            ),
        ],
    );
    if summary.omitted_files > 0 {
        row.push_str(&super::i18n::translate(
            state,
            "conflict.summary.omitted",
            &[("count", &summary.omitted_files.to_string())],
        ));
    }
    if summary.truncated {
        row.push_str(&super::i18n::text(state, "conflict.summary.truncated"));
    }
    Some(row)
}

/// The full detail body: three labelled sides per rejected hunk.
pub(super) fn detail_rows(
    state: &TuiState,
    content: Option<&ConflictContent>,
    width: usize,
) -> Vec<String> {
    if !state.has_capability(CONFLICT_CONTENT_CAPABILITY) {
        return vec![super::i18n::translate(
            state,
            "conflict.detail.unavailable",
            &[("capability", CONFLICT_CONTENT_CAPABILITY)],
        )];
    }
    let Some(content) = content else {
        return vec![super::i18n::text(state, "conflict.detail.absent")];
    };
    // The disclaimer is the first row, before any line an operator could read
    // as a resolution. It is mandatory: Core resolved nothing.
    let mut rows = vec![
        super::i18n::translate(
            state,
            "conflict.detail.baseline",
            &[(
                "baseline",
                &super::i18n::text(
                    state,
                    ConflictBaselineTag::from(&content.baseline).label_key(),
                ),
            )],
        ),
        super::i18n::text(state, "conflict.detail.not_a_merge"),
    ];
    let mut body = Vec::new();
    for file in &content.files {
        body.extend(file_rows(state, file, width));
    }
    let hidden = body.len().saturating_sub(MAX_CONFLICT_DETAIL_ROWS);
    if hidden > 0 {
        body.truncate(MAX_CONFLICT_DETAIL_ROWS);
        body.push(super::i18n::translate(
            state,
            "conflict.detail.more",
            &[("count", &hidden.to_string())],
        ));
    }
    rows.extend(body);
    if content.truncated {
        rows.push(super::i18n::text(state, "conflict.detail.truncated"));
    }
    rows.into_iter()
        .map(|row| truncate_end(&row, width))
        .collect()
}

fn file_rows(state: &TuiState, file: &ConflictFile, width: usize) -> Vec<String> {
    let mut rows = vec![super::i18n::translate(
        state,
        "conflict.detail.file",
        &[("path", &truncate_tail(&file.path, width.saturating_sub(4)))],
    )];
    if file.omitted {
        rows.push(super::i18n::text(state, "conflict.detail.omitted"));
    }
    for hunk in &file.hunks {
        rows.extend(hunk_rows(state, hunk, width));
    }
    rows
}

fn hunk_rows(state: &TuiState, hunk: &ConflictHunk, width: usize) -> Vec<String> {
    let mut rows = vec![super::i18n::translate(
        state,
        "conflict.detail.hunk",
        &[(
            "reason",
            &super::i18n::text(state, hunk_reason_key(hunk.reason)),
        )],
    )];
    rows.extend(side_rows(
        state,
        "conflict.side.ours",
        hunk.ours_start,
        &hunk.ours,
        width,
    ));
    rows.extend(side_rows(
        state,
        "conflict.side.theirs",
        hunk.theirs_start,
        &hunk.theirs,
        width,
    ));
    match hunk.base.as_deref() {
        // The patch expected an empty region, which is what a creation hunk
        // expects. Distinct from "there was no preimage at all".
        Some([]) => rows.push(super::i18n::text(state, "conflict.side.base_empty")),
        Some(lines) => rows.extend(side_rows(
            state,
            "conflict.side.base",
            // The preimage is the patch's own text; it carries no separate
            // start, so the side is labelled without a line number rather than
            // borrowing one from another side.
            0,
            lines,
            width,
        )),
        None => rows.push(super::i18n::text(state, "conflict.side.base_absent")),
    }
    rows
}

/// One labelled side. An empty side is stated, because "the file has nothing
/// there" is a fact a reviewer needs, not a row to drop.
fn side_rows(
    state: &TuiState,
    side_key: &str,
    start: u32,
    lines: &[String],
    width: usize,
) -> Vec<String> {
    let side = super::i18n::text(state, side_key);
    if lines.is_empty() {
        return vec![super::i18n::translate(
            state,
            "conflict.side.empty",
            &[("side", &side)],
        )];
    }
    lines
        .iter()
        .enumerate()
        .map(|(offset, line)| {
            let number = if start == 0 {
                String::new()
            } else {
                (start + offset as u32).to_string()
            };
            format!(
                "  {side:<7}{number:>5}  {}",
                truncate_end(
                    line.trim_end_matches(['\n', '\r']),
                    width.saturating_sub(16)
                )
            )
        })
        .collect()
}

const fn hunk_reason_key(reason: ConflictHunkReason) -> &'static str {
    match reason {
        ConflictHunkReason::ContextMismatch => "conflict.reason.context_mismatch",
        ConflictHunkReason::AlreadyApplied => "conflict.reason.already_applied",
        ConflictHunkReason::FileMissing => "conflict.reason.file_missing",
        ConflictHunkReason::FileDeleted => "conflict.reason.file_deleted",
        ConflictHunkReason::Binary => "conflict.reason.binary",
        // `#[non_exhaustive]`: a rejection a newer apply path saw is named as
        // unrecognized rather than mislabelled as a context mismatch.
        _ => "conflict.reason.unrecognized",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use viden_core::CapabilityId;

    fn capable() -> TuiState {
        let mut state = TuiState::default();
        state
            .capabilities
            .insert(CapabilityId(CONFLICT_CONTENT_CAPABILITY.to_string()));
        state
    }

    fn hunk(base: Option<Vec<String>>) -> ConflictHunk {
        ConflictHunk {
            ours_start: 42,
            ours: vec!["    let bounce = record_conflict_bounce(gate)?;\n".to_string()],
            theirs_start: 42,
            theirs: vec!["    let bounce = bounce_with_reason(gate, reason)?;\n".to_string()],
            base,
            reason: ConflictHunkReason::ContextMismatch,
        }
    }

    fn content(base: Option<Vec<String>>) -> ConflictContent {
        ConflictContent {
            baseline: ConflictBaseline::Unknown,
            files: vec![ConflictFile {
                path: "crates/runtime/src/trust_loop.rs".to_string(),
                hunks: vec![hunk(base)],
                omitted: false,
            }],
            truncated: false,
        }
    }

    #[test]
    fn the_detail_always_says_it_is_not_a_merge_result() {
        let rows = detail_rows(
            &capable(),
            Some(&content(Some(vec![
                "    let bounce = record_bounce(gate)?;".to_string(),
            ]))),
            96,
        )
        .join("\n");

        assert!(rows.contains("not a merge result"), "{rows}");
        assert!(rows.contains("OURS"), "{rows}");
        assert!(rows.contains("THEIRS"), "{rows}");
        assert!(rows.contains("BASE"), "{rows}");
        // Core's own line numbers, on the sides that have them.
        assert!(rows.contains("42"), "{rows}");
        assert!(rows.contains("context mismatch"), "{rows}");
    }

    /// `None` and `Some(vec![])` are different facts about the patch preimage.
    #[test]
    fn an_absent_preimage_and_an_empty_one_never_render_alike() {
        let state = capable();
        let absent = detail_rows(&state, Some(&content(None)), 96).join("\n");
        let empty = detail_rows(&state, Some(&content(Some(Vec::new()))), 96).join("\n");

        assert_ne!(absent, empty);
        assert!(absent.contains("no preimage"), "{absent}");
        assert!(empty.contains("empty region"), "{empty}");
    }

    #[test]
    fn absent_content_and_a_missing_capability_are_stated_separately() {
        let capable = capable();
        assert!(
            detail_rows(&capable, None, 96)[0].contains("published no content"),
            "{:?}",
            detail_rows(&capable, None, 96)
        );
        let base = TuiState::default();
        assert!(
            detail_rows(&base, Some(&content(None)), 96)[0].contains(CONFLICT_CONTENT_CAPABILITY)
        );
        // Without the capability there is no summary row either, so the caller
        // keeps the reason-only row it always had.
        assert!(
            content_summary_row(&base, Some(ConflictContentSummary::from(&content(None))))
                .is_none()
        );
    }

    #[test]
    fn the_summary_counts_files_hunks_omissions_and_names_the_baseline() {
        let state = capable();
        let mut value = content(None);
        value.files.push(ConflictFile {
            path: "crates/runtime/src/huge.rs".to_string(),
            hunks: Vec::new(),
            omitted: true,
        });
        value.truncated = true;
        value.baseline = ConflictBaseline::Revision {
            sha: "9f9f9f9f".to_string(),
        };

        let summary = ConflictContentSummary::from(&value);
        assert_eq!(summary.files, 2);
        assert_eq!(summary.hunks, 1);
        assert_eq!(summary.omitted_files, 1);
        assert_eq!(summary.baseline, ConflictBaselineTag::Revision);

        let row = content_summary_row(&state, Some(summary)).expect("summary row");
        assert!(row.contains('2') && row.contains('1'), "{row}");
        assert!(row.contains("revision"), "{row}");
        assert!(row.contains("omitted"), "{row}");
        assert!(row.contains("truncated"), "{row}");
    }

    #[test]
    fn every_row_fits_the_requested_width() {
        let state = capable();
        let value = content(Some(vec![
            "    let bounce = record_bounce(gate)?; // a very long trailing comment".to_string(),
        ]));
        for width in [32usize, 60, 96, 132] {
            for row in detail_rows(&state, Some(&value), width) {
                assert!(
                    super::super::text::char_width(&row) <= width,
                    "width {width} overflowed: {row}"
                );
            }
        }
    }
}
