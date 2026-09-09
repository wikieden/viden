//! Where a failed merge apply gets the lines it collided with, and what those
//! lines were read against (`runtime.conflict_content`, GUI-CORE-015).
//!
//! The lines themselves come from the single producer in
//! `viden_tools::patch::conflict_content`. This module owns the other half —
//! the baseline rule — because only the runtime knows what the conflict was
//! computed against, and because a rule spelled out twice is a rule that will
//! eventually disagree with itself.

use std::path::Path;
use std::time::Duration;

use viden_tools::patch::{PatchRequest, conflict_content};
use viden_types::{ConflictBaseline, ConflictContent, ReviewedEvidenceBinding};

use crate::frontend_status::head_revision;

/// How long the baseline is allowed to spend asking Git for a revision.
///
/// The conflict is already published without it if this expires: a slow or
/// absent `git` degrades the baseline to `Unknown`, it never delays or drops
/// the lines an operator is waiting on.
const BASELINE_GIT_TIMEOUT: Duration = Duration::from_secs(2);

/// What the `ours` side of a merge-path conflict was read against.
///
/// The order is the contract, not a preference. A merge gate's baseline *is*
/// its canonical reviewed evidence — that is what the reviewer accepted and
/// what the apply was authorised against — so the bindings win whenever the
/// gate holds any. A bare workspace revision is the fallback for a gate whose
/// bindings could not be validated, and `Unknown` is a real answer rather than
/// a default: it says Core held no baseline it could name, which a client must
/// render as unknown instead of silently as `HEAD`.
pub(crate) fn merge_conflict_baseline(
    cwd: &Path,
    baseline_evidence: &[ReviewedEvidenceBinding],
) -> ConflictBaseline {
    if !baseline_evidence.is_empty() {
        return ConflictBaseline::Evidence {
            bindings: baseline_evidence.to_vec(),
        };
    }
    match head_revision(cwd, BASELINE_GIT_TIMEOUT) {
        Some(sha) => ConflictBaseline::Revision { sha },
        None => ConflictBaseline::Unknown,
    }
}

/// Builds the content for a merge apply that was refused.
///
/// `None` when the apply left nothing to show — see the producer's own
/// contract. The caller passes the canonical patch bytes it already held, so
/// no evidence is re-read and no file is written.
pub(crate) fn merge_conflict_content(
    cwd: &Path,
    patch: &str,
    baseline_evidence: &[ReviewedEvidenceBinding],
) -> Option<ConflictContent> {
    conflict_content(
        &PatchRequest {
            cwd: cwd.to_path_buf(),
            unified_diff: patch.to_string(),
        },
        merge_conflict_baseline(cwd, baseline_evidence),
    )
}
