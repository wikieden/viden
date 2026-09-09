//! DiffReview read side, projected from Core's structured diff
//! (`runtime.structured_diff`, GUI-CORE-012).
//!
//! Core is the only producer of diff rows: the unified-diff parser that
//! *applies* patches emits `DiffDocument`, so the apply path and every client
//! agree about what a file, a hunk, and a line are. Nothing here parses text —
//! a row exists only because Core published it.
//!
//! The honesty pairs this module has to keep distinct on the wire, because the
//! frontend renders a different sentence for each:
//!
//! - `capability_available` (Core publishes no structured diff at all),
//!   `loaded` (no page has arrived yet), and an empty `entries` on a loaded
//!   page (Core answered, the working tree is clean) are three facts. An empty
//!   list must never stand in for either of the first two.
//! - [`DiffFileProjection::omitted`] means the byte bound dropped this file's
//!   rows while its counts stayed real; [`DiffFileProjection::binary`] means
//!   Git reported binary content. Both produce zero hunks and neither means
//!   "unchanged".
//! - [`WorkspaceDiffEntryProjection::diff`] being `None` means Core produced no
//!   diff for that path — never "no change".

use serde::Serialize;

use crate::{D1OutcomeProjection, D1WorkspaceSourceProjection};

/// The frontend-contract-v1 capability that carries structured diff rows.
///
/// The exact id Core publishes in its handshake
/// (`FRONTEND_V1_EXTENSION_CAPABILITIES`); the client must not invent a
/// finer-grained one, because an unpublished id can never become available.
pub const STRUCTURED_DIFF_CAPABILITY: &str = "runtime.structured_diff";

/// One row of a hunk.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLineProjection {
    /// `context`, `added`, `removed`, or `unknown` for a row kind this build
    /// cannot name — `DiffLineKind` is `#[non_exhaustive]`, and drawing an
    /// unmodeled row as context would silently mislabel it.
    pub kind: &'static str,
    /// The line text without its `+`/`-`/space marker, exactly as Core sent it.
    pub content: String,
    /// Absent for an added line, which has no old-file position. Never `0`,
    /// which a reader would take for a real line number.
    pub old_line: Option<u32>,
    /// Absent for a removed line, which has no new-file position.
    pub new_line: Option<u32>,
}

/// One `@@` hunk. Starts and lengths are Git's own, never counted from rows.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunkProjection {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// The section heading Git appended after the closing `@@`, when it wrote
    /// one. `None` means the header carried none, never an empty heading.
    pub header: Option<String>,
    pub lines: Vec<DiffLineProjection>,
}

/// One file's rows.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFileProjection {
    /// Repository-relative, `/`-separated. The post-change path.
    pub path: String,
    /// The pre-rename path, present only for a rename.
    pub old_path: Option<String>,
    pub kind: &'static str,
    /// Git reported binary content, so there are no rows to render.
    pub binary: bool,
    /// The byte bound dropped this file's rows. `additions` and `deletions`
    /// stay real, so "not shown" never renders as "unchanged".
    pub omitted: bool,
    pub additions: u32,
    pub deletions: u32,
    pub hunks: Vec<DiffHunkProjection>,
}

/// One changed path in a `WorkspaceDiffPage`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDiffEntryProjection {
    /// Target-relative, `/`-separated.
    pub path: String,
    /// How the path differs between `HEAD` and the index. `None` means the
    /// index matches `HEAD` there — never a default classification.
    pub index: Option<&'static str>,
    /// How the path differs between the index and the working tree.
    pub worktree: Option<&'static str>,
    /// Core's own derivation, not the client's.
    pub staged: bool,
    /// `None` means Core produced no diff for this path — never "no change".
    pub diff: Option<DiffFileProjection>,
}

/// What the DiffReview view may render after one `QueryWorkspaceDiff`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDiffProjection {
    pub outcome: D1OutcomeProjection,
    /// The Lane the confirmed page describes, or `None` for the workspace
    /// root. Read back from Core's own answer rather than from the request, so
    /// the view names the tree Core actually diffed.
    pub target_lane_id: Option<String>,
    /// The target's source-control facts, resampled by Core for this read.
    /// `None` until a page arrives.
    pub source: Option<D1WorkspaceSourceProjection>,
    /// Lexicographic by path, exactly as Core delivered.
    pub entries: Vec<WorkspaceDiffEntryProjection>,
    /// At least one entry's rows were dropped by the byte bound.
    pub truncated: bool,
    /// Whether a page has actually arrived. Absence and emptiness are
    /// different facts: "not read yet" must never render as "no changes".
    pub loaded: bool,
    pub pending_command_id: Option<String>,
    /// False when Core's handshake published no `runtime.structured_diff`.
    pub capability_available: bool,
    /// Core published a workspace source or change fact after the loaded page
    /// was confirmed, so the rows on screen describe an older tree.
    ///
    /// This is the whole re-query trigger. The client cannot subscribe to one
    /// event kind through the host seam, so the adapter counts the two facts
    /// that invalidate a diff (`WorkspaceSourceUpdated`,
    /// `WorkspaceChangeUpdated`) and the view compares that count against the
    /// one its page was read at. It is never a claim about *what* changed.
    pub stale: bool,
}
