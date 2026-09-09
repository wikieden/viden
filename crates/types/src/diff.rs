//! Structured diff contract (`runtime.structured_diff`, GUI-CORE-012).
//!
//! Core already produced or consumed unified diff *text* at four sites: the
//! tool-result patch on a workspace change, the approval preview, the merge
//! apply path inside `LocalPatchBackend`, and the lane apply request. Every
//! client that wanted hunk rows had to parse that text itself, which is a
//! second parser outside the Core boundary and a second definition of what a
//! hunk is. This module is the one typed shape instead, and Core is its only
//! producer: the unified-diff parser in `crates/tools/src/patch.rs` — the same
//! one that applies patches — emits it.
//!
//! Three invariants make the shape honest:
//!
//! 1. **Bounded and flagged.** A document names the byte bound it was built
//!    under and says whether that bound dropped anything
//!    ([`DiffDocument::truncated`]), and a file whose hunks were dropped says
//!    so ([`DiffFile::omitted`]) while keeping its real counts. A client can
//!    always tell "unchanged" from "not shown".
//! 2. **Absence means unknown.** [`DiffLine::old_line`] and
//!    [`DiffLine::new_line`] are absent for the side the line does not exist
//!    on. They are never `0`, which a client would render as a real line.
//! 3. **Additive only.** Every field defaults, options are omitted when
//!    absent, and the new enums are `#[non_exhaustive]`, so a record without
//!    these fields encodes to exactly the bytes it did before they existed and
//!    a future variant cannot break a sibling crate's match.

use serde::{Deserialize, Serialize};

use crate::{AgentLaneId, WorkspaceChangeKind, WorkspaceSourceView};

/// Largest byte bound a single [`WorkspaceDiffQuery`] may ask for.
///
/// One mebibyte: a diff read answers a keystroke in a review pane, and an
/// unbounded answer would put a repository-sized payload on the event stream.
pub const MAX_WORKSPACE_DIFF_BYTES: u32 = 1024 * 1024;

/// Byte bound used when a query names none.
pub const DEFAULT_WORKSPACE_DIFF_BYTES: u32 = 256 * 1024;

/// A parsed unified diff over one or more files.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffDocument {
    #[serde(default)]
    pub files: Vec<DiffFile>,
    /// At least one file's hunks were dropped by [`Self::byte_limit`].
    ///
    /// Distinct from [`DiffFile::omitted`], which names *which* file lost its
    /// rows: a client renders the per-file marker and the document-level flag
    /// tells it the list itself is partial.
    #[serde(default)]
    pub truncated: bool,
    /// The byte bound this document was built under, published so a client can
    /// state the limit rather than guess it.
    #[serde(default)]
    pub byte_limit: u32,
}

/// One file inside a [`DiffDocument`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffFile {
    /// Repository-relative, `/`-separated, no leading separator. The new path
    /// for a rename; an absolute path would leak the operator's home directory
    /// onto the wire and into fixtures.
    pub path: String,
    /// The pre-rename path, present only for [`WorkspaceChangeKind::Renamed`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    pub kind: WorkspaceChangeKind,
    /// Git reported binary content, so there are no hunks to render. Absence
    /// of hunks alone is ambiguous — an omitted file has none either — which
    /// is why this is its own flag.
    #[serde(default)]
    pub binary: bool,
    /// The file is part of the change set but its hunks were dropped by the
    /// byte bound. `additions` and `deletions` stay real.
    #[serde(default)]
    pub omitted: bool,
    #[serde(default)]
    pub additions: u32,
    #[serde(default)]
    pub deletions: u32,
    #[serde(default)]
    pub hunks: Vec<DiffHunk>,
}

/// One `@@` hunk. Starts and lengths come from the hunk header, so a client
/// renders the line numbers Git computed rather than counting rows itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffHunk {
    #[serde(default)]
    pub old_start: u32,
    #[serde(default)]
    pub old_lines: u32,
    #[serde(default)]
    pub new_start: u32,
    #[serde(default)]
    pub new_lines: u32,
    /// The section heading Git appends after the closing `@@`, when it wrote
    /// one. `None` means the header carried none, never an empty heading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    #[serde(default)]
    pub lines: Vec<DiffLine>,
}

/// One row of a hunk, with the line numbers it actually has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    /// The line text without its `+`/`-`/space marker and without the trailing
    /// newline; the marker is carried by `kind`.
    pub content: String,
    /// Absent for an added line, which has no old-file position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_line: Option<u32>,
    /// Absent for a removed line, which has no new-file position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_line: Option<u32>,
}

/// What one diff row is.
///
/// `#[non_exhaustive]` because unified diff has rows this build does not model
/// as their own kind (a `\ No newline at end of file` marker, conflict
/// markers). A newer Core that starts publishing one must not force this build
/// to mislabel it as context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
}

/// What Core knows about the change an approval would make.
///
/// Attached to [`crate::ApprovalRequestView`] so a permission dock can show
/// the hunks instead of a truncated input preview. Stated limitation: the
/// preview is computed at approval time and execution runs the proposed tool
/// input, so a file changed in between can produce a different result;
/// [`Self::base_sha256`] is what lets a client or an audit reader detect that
/// afterwards. A pre-execution recheck is not in `0.3.3`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionContext {
    /// The prospective change, computed in memory. `None` means Core could not
    /// compute one — never "no change".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<DiffDocument>,
    /// SHA-256 of the file contents the diff was computed against, when the
    /// diff came from a proposed single-file mutation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_sha256: Option<String>,
}

/// What a read or action names as its target.
///
/// Clients never pass paths: Core validates the Lane exists, is not archived,
/// and resolves its worktree itself. `#[non_exhaustive]` so a future target
/// kind cannot break a client match.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SourceTarget {
    #[default]
    Workspace,
    Lane {
        lane_id: AgentLaneId,
    },
}

/// Which side of the index a diff read covers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WorkspaceDiffScope {
    /// Unstaged changes only (`git diff`).
    #[default]
    Worktree,
    /// Staged changes only (`git diff --cached`).
    Index,
    /// Both sides. A path present on both carries its worktree diff, because
    /// that is the operator's current file content; the staged half stays
    /// visible through the entry's `index` classification.
    Both,
}

/// Read-only structured diff query.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceDiffQuery {
    #[serde(default)]
    pub target: SourceTarget,
    #[serde(default)]
    pub scope: WorkspaceDiffScope,
    /// Target-relative `/`-separated paths. Empty means every changed path.
    #[serde(default)]
    pub paths: Vec<String>,
    /// Clamped to `1..=`[`MAX_WORKSPACE_DIFF_BYTES`]; `None` uses
    /// [`DEFAULT_WORKSPACE_DIFF_BYTES`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_limit: Option<u32>,
}

impl WorkspaceDiffQuery {
    /// Byte bound actually used, clamped rather than rejected so a malformed
    /// client request still gets a well-formed page.
    pub fn clamped_byte_limit(&self) -> u32 {
        self.byte_limit
            .unwrap_or(DEFAULT_WORKSPACE_DIFF_BYTES)
            .clamp(1, MAX_WORKSPACE_DIFF_BYTES)
    }

    /// Rejects a query that cannot mean what it says.
    ///
    /// A path that escapes the target root is rejected, not clamped: clamping
    /// would answer a question nobody asked, and answering with an empty page
    /// would be indistinguishable from "nothing changed there" — the exact
    /// fabricated absence this contract exists to prevent.
    pub fn validate(&self) -> Result<(), String> {
        for path in &self.paths {
            if path.trim().is_empty() {
                return Err("workspace diff path cannot be empty".to_string());
            }
            if path.starts_with('/') || path.starts_with('\\') {
                return Err(format!(
                    "workspace diff path `{path}` must be target-relative"
                ));
            }
            if path.contains('\\') {
                return Err(format!(
                    "workspace diff path `{path}` must use `/` separators"
                ));
            }
            if path.len() > 1 && path.as_bytes()[1] == b':' {
                return Err(format!(
                    "workspace diff path `{path}` must be target-relative"
                ));
            }
            if path.split('/').any(|segment| segment == "..") {
                return Err(format!("workspace diff path `{path}` leaves the target"));
            }
            if path.contains('\0') {
                return Err(format!("workspace diff path `{path}` contains a null byte"));
            }
        }
        Ok(())
    }
}

/// One answer to a [`WorkspaceDiffQuery`].
///
/// A query result, not view state: it is bounded and re-read on demand, so it
/// is deliberately never folded into `RuntimeViewState` and no snapshot digest
/// moves when one is published.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceDiffPage {
    pub target: SourceTarget,
    /// The target's source-control facts, resampled for this read, so a client
    /// renders the branch the diff came from rather than the last one it saw.
    pub source: WorkspaceSourceView,
    /// Lexicographic by `path`, ascending.
    #[serde(default)]
    pub entries: Vec<WorkspaceDiffEntry>,
    /// At least one entry's diff was dropped by the byte bound.
    #[serde(default)]
    pub truncated: bool,
}

/// One changed path in a [`WorkspaceDiffPage`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceDiffEntry {
    /// Target-relative, `/`-separated, no leading separator.
    pub path: String,
    /// How the path differs between `HEAD` and the index. `None` means the
    /// index matches `HEAD` there — never a default classification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<WorkspaceChangeKind>,
    /// How the path differs between the index and the working tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<WorkspaceChangeKind>,
    /// The path has staged content. Derived from `index`, published
    /// explicitly so a client need not reimplement the derivation.
    #[serde(default)]
    pub staged: bool,
    /// The file's rows for the requested scope. `None` means Core produced no
    /// diff for it (a binary path Git reported without content, or a read that
    /// failed) — never "no change".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<DiffFile>,
}
