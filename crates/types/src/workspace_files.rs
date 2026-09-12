//! Read-only workspace file contracts: the inventory (GUI-CORE-022) and the
//! single-file read beside it (`runtime.workspace_file_reads`, C9).
//!
//! The two are a pair. The inventory says a path exists; the read says what is
//! in it. They live in one module because they share one rule — a client never
//! touches the operator's filesystem — and because a client that paged the
//! first must be able to reach the second without learning a second vocabulary
//! for what a workspace path is.
//!
//! A frontend needs the list of files in the open workspace to offer a file
//! jump target, but it must not produce that list itself: walking the
//! filesystem from a client is outside the client boundary and bypasses the
//! permission gate that governs every other path read. So the inventory is a
//! Core-owned query, and three invariants make its answer trustworthy:
//!
//! 1. **Permission-gated at the source.** Core consults the permission engine
//!    before it reads a single directory entry. A denial is published as an
//!    error, never as an empty page: an empty inventory and a refused
//!    inventory are different facts and must never render the same.
//! 2. **Ordered, then paged.** Entries are sorted lexicographically and the
//!    prefix filter and cursor are applied to that ordered inventory, so
//!    [`WorkspaceFilePage::complete`] and [`WorkspaceFilePage::next_after`]
//!    describe the filtered ordered tree — not whatever the walker happened to
//!    hand back first (the audit-page precedent).
//! 3. **Attributable.** [`crate::RuntimeEventKind::WorkspaceFilesLoaded`]
//!    carries the exact command id of the read it answers, required from the
//!    first byte the type ever shipped. GUI-CORE-024 established what an
//!    optional correlation id costs; this contract is new, so it has no legacy
//!    `None` case to accommodate and never gains one.
//!
//! The read below inherits all three and adds a fourth: **a path is refused,
//! never repaired.** See [`WorkspaceFileReadQuery::validate`].

use serde::{Deserialize, Serialize};

use crate::SourceTarget;

/// Largest page a single [`WorkspaceFilesQuery`] may return. Mirrors
/// `MAX_AUDIT_PAGE_SIZE`: a read bounded the same way the other paginated read
/// on this contract is bounded.
pub const MAX_WORKSPACE_FILE_PAGE_SIZE: u32 = 500;

/// Page size used when a query names no limit.
///
/// A query with no limit means "a page", never "the whole tree": an unbounded
/// answer would put a workspace-sized payload on the event stream, and a client
/// that wants more already has the `after` cursor to ask for it.
pub const DEFAULT_WORKSPACE_FILE_PAGE_SIZE: u32 = 200;

/// What one inventory entry is.
///
/// `#[non_exhaustive]` because the filesystem has more kinds than these two
/// (symlinks, sockets, devices). A newer Core that starts publishing one must
/// not force this build to mislabel it as a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WorkspaceFileKind {
    File,
    Dir,
}

/// One entry of the workspace inventory.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WorkspaceFileEntry {
    /// Workspace-relative, `/`-separated, with no leading separator. Always
    /// relative: an absolute path would leak the operator's home directory
    /// onto the wire and into fixtures.
    pub path: String,
    pub kind: WorkspaceFileKind,
    /// Byte size of a file. `None` for a directory, and also `None` for a file
    /// whose metadata Core could not read — absence means "not known here",
    /// never zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}

/// Read-only workspace inventory query.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WorkspaceFilesQuery {
    /// Workspace-relative `/`-separated prefix filter; `None` is the whole
    /// tree. Matching is on path prefix, so `crates/ty` keeps
    /// `crates/types/...`.
    #[serde(default)]
    pub prefix: Option<String>,
    /// Page size, clamped to `1..=MAX_WORKSPACE_FILE_PAGE_SIZE`. `None` uses
    /// [`DEFAULT_WORKSPACE_FILE_PAGE_SIZE`].
    #[serde(default)]
    pub limit: Option<u32>,
    /// Exclusive resume cursor: the last `path` of the previous page.
    ///
    /// Exclusive rather than inclusive so two adjacent pages tile the
    /// inventory without repeating the boundary entry.
    #[serde(default)]
    pub after: Option<String>,
}

impl WorkspaceFilesQuery {
    /// Page size actually used, clamped rather than rejected so a malformed
    /// client request still gets a well-formed page.
    pub fn clamped_limit(&self) -> usize {
        self.limit
            .unwrap_or(DEFAULT_WORKSPACE_FILE_PAGE_SIZE)
            .clamp(1, MAX_WORKSPACE_FILE_PAGE_SIZE) as usize
    }

    /// Rejects a query that cannot mean what it says.
    ///
    /// A prefix that escapes the workspace is rejected, not clamped: clamping
    /// it to the root would answer a question the caller did not ask, and
    /// answering it with an empty page would be indistinguishable from an
    /// empty subtree. Backslashes are rejected for the same reason the path is
    /// documented as `/`-separated — one separator, so a prefix means the same
    /// thing on every platform.
    pub fn validate(&self) -> Result<(), String> {
        let Some(prefix) = self.prefix.as_deref() else {
            return Ok(());
        };
        if prefix.starts_with('/') || prefix.starts_with('\\') {
            return Err(format!(
                "workspace file prefix `{prefix}` must be workspace-relative"
            ));
        }
        if prefix.contains('\\') {
            return Err(format!(
                "workspace file prefix `{prefix}` must use `/` separators"
            ));
        }
        if prefix.len() > 1 && prefix.as_bytes()[1] == b':' {
            return Err(format!(
                "workspace file prefix `{prefix}` must be workspace-relative"
            ));
        }
        if prefix.split('/').any(|segment| segment == "..") {
            return Err(format!(
                "workspace file prefix `{prefix}` leaves the workspace"
            ));
        }
        if prefix.contains('\0') {
            return Err(format!(
                "workspace file prefix `{prefix}` contains a null byte"
            ));
        }
        Ok(())
    }
}

/// One page of the workspace inventory, ordered lexicographically by `path`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WorkspaceFilePage {
    /// Lexicographic by `path`, ascending.
    pub entries: Vec<WorkspaceFileEntry>,
    /// Cursor to pass as the next query's `after`. `None` when complete.
    #[serde(default)]
    pub next_after: Option<String>,
    /// True when no further entry matches the query. Describes the *filtered*
    /// ordered inventory, because Core applies the prefix before cutting the
    /// page — a client filtering a page it already holds could not know whether
    /// a matching path sits on a page it never loaded.
    pub complete: bool,
}

/// Largest number of bytes one [`WorkspaceFileReadQuery`] may publish as text.
///
/// 1 MiB, the same ceiling [`crate::MAX_WORKSPACE_DIFF_BYTES`] puts on a diff
/// read. File content rides an event every connected client receives, so the
/// bound is what stops an operator clicking one palette row from putting an
/// arbitrary blob on the stream.
pub const MAX_WORKSPACE_FILE_BYTES: u32 = 1024 * 1024;

/// Byte bound a read gets when it expresses no preference. 256 KiB, matching
/// the diff read's default and the evidence content bound.
pub const DEFAULT_WORKSPACE_FILE_BYTES: u32 = 256 * 1024;

/// Read-only request for the content of exactly one file in the workspace or
/// one Lane worktree (`runtime.workspace_file_reads`).
///
/// The inventory above says a path *exists*; this says what is in it. Both are
/// Core-owned for the same reason: a client walking or opening files itself
/// would be outside the client boundary and past the permission gate that
/// governs every other path read.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WorkspaceFileReadQuery {
    /// The workspace root or one Lane worktree. Core resolves a Lane's path
    /// from its own records; a client never passes one.
    #[serde(default)]
    pub target: SourceTarget,
    /// Target-relative, `/`-separated, with no leading separator. Validated,
    /// never repaired: see [`WorkspaceFileReadQuery::validate`].
    pub path: String,
    /// Clamped to `1..=`[`MAX_WORKSPACE_FILE_BYTES`]; `None` uses
    /// [`DEFAULT_WORKSPACE_FILE_BYTES`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_limit: Option<u32>,
}

impl WorkspaceFileReadQuery {
    /// Byte bound actually used, clamped rather than rejected so a malformed
    /// bound still gets a well-formed answer. The asymmetry with
    /// [`Self::validate`] is deliberate: a caller that asked for too many
    /// bytes still means something answerable, while a caller that asked for
    /// the wrong *file* does not.
    pub fn clamped_byte_limit(&self) -> u32 {
        self.byte_limit
            .unwrap_or(DEFAULT_WORKSPACE_FILE_BYTES)
            .clamp(1, MAX_WORKSPACE_FILE_BYTES)
    }

    /// Rejects a path that cannot mean what it says.
    ///
    /// Every case here is a refusal rather than a repair, and that is the
    /// contract. Normalizing `../../etc/passwd` to `etc/passwd` would serve a
    /// *different* file than the one asked for under the asked-for name, and
    /// answering with [`WorkspaceFileUnavailableReason::NotFound`] would tell
    /// a client the operator's own tree does not contain a file that may well
    /// exist. Only a stated refusal is honest about either.
    ///
    /// The rules are `safe_project_relative_projection_path`'s, which the
    /// session projection has applied to every path it publishes since 0.3.2,
    /// plus the `/`-separator rule the inventory prefix already enforces so
    /// one path spelling means the same thing on every platform.
    pub fn validate(&self) -> Result<(), String> {
        let path = self.path.as_str();
        if path.trim().is_empty() {
            return Err(
                "workspace file path cannot be empty\nhint: name a target-relative path \
                        such as `crates/types/src/lib.rs`"
                    .to_string(),
            );
        }
        if path.starts_with('/') || path.starts_with('\\') {
            return Err(format!(
                "workspace file path `{path}` must be target-relative"
            ));
        }
        if path.contains('\\') {
            return Err(format!(
                "workspace file path `{path}` must use `/` separators"
            ));
        }
        // A Windows drive prefix (`C:\...`, `C:/...`) is absolute even though
        // it starts with neither separator.
        if path.len() > 1 && path.as_bytes()[1] == b':' {
            return Err(format!(
                "workspace file path `{path}` must be target-relative"
            ));
        }
        if path.split('/').any(|segment| segment == "..") {
            return Err(format!("workspace file path `{path}` leaves the target"));
        }
        // NUL and every other control character: a path carrying one is either
        // a truncation attack against a C boundary or a display string that
        // would rewrite a client's own terminal.
        if path.chars().any(char::is_control) {
            return Err(format!(
                "workspace file path `{path}` contains a control character"
            ));
        }
        // Nothing but separators and `.` segments names no file at all, so it
        // would otherwise resolve to the target root itself.
        if path
            .split('/')
            .all(|segment| segment.is_empty() || segment == ".")
        {
            return Err(format!("workspace file path `{path}` names no file"));
        }
        Ok(())
    }

    /// The path with empty and `.` segments dropped, as Core resolves it.
    ///
    /// Only ever called after [`Self::validate`] passed, so this cannot turn a
    /// traversal into a legal path: `..` was already refused.
    pub fn normalized_path(&self) -> String {
        self.path
            .trim()
            .split('/')
            .filter(|segment| !segment.is_empty() && *segment != ".")
            .collect::<Vec<_>>()
            .join("/")
    }
}

/// One answer to a [`WorkspaceFileReadQuery`].
///
/// `size` and `sha256` are optional because there are real answers for which
/// neither exists: a path that is not there, or is a directory, has no length
/// to report and no bytes to hash. Publishing `0` and `""` for those would be
/// the fabricated default the contract's own bounds rule forbids — a client
/// would render "empty file" for a file that is simply missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceFileContent {
    /// The normalized target-relative path Core actually resolved, echoed so a
    /// client that asked with `./src/lib.rs` can key its view on one spelling.
    pub path: String,
    /// Byte length of the whole file on disk, not of the published body.
    /// `None` when there is no file to measure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// SHA-256 of the **whole** file, never of the truncated body: it is what
    /// lets a client tell one revision of a file from another, and hashing
    /// only the published prefix would make two different files with a common
    /// head indistinguishable. `None` when there were no bytes to hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    pub content: WorkspaceFileBody,
}

/// What Core can publish for one file.
///
/// `#[non_exhaustive]` so a later body shape cannot break a client match. The
/// three cases are deliberately not collapsible: "here is the text", "there is
/// text but it is not text Core can put on the wire", and "there is nothing to
/// read" produce three different affordances, and a client shown one for
/// another would render an empty editor over a binary asset or over a file it
/// was never allowed to open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum WorkspaceFileBody {
    /// UTF-8 text, cut on a character boundary at the query's clamped bound.
    /// `truncated` says the bound cut it, never that the file was short.
    Text { text: String, truncated: bool },
    /// The bytes are not text Core will publish. Deliberately carries no
    /// payload: a client renders the file's size and hash and offers no
    /// editor, rather than rendering replacement characters as content.
    Binary,
    /// There is nothing to read, and the typed reason says why, because the
    /// affordance differs per case: a missing path is a stale reference, a
    /// directory is a navigation target, and an unreadable one is a refusal.
    Unavailable {
        reason: WorkspaceFileUnavailableReason,
    },
}

/// Why one file read produced no content.
///
/// `#[non_exhaustive]`. None of these is an error: each is a fact about the
/// tree that a client renders rather than retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WorkspaceFileUnavailableReason {
    /// No entry at that path inside the resolved target.
    NotFound,
    /// The path resolves to a directory. Its own contents are the inventory
    /// read's answer, not this one's.
    Directory,
    /// Core resolved the path but could not read the bytes: an operating
    /// system refusal, an I/O failure, or — the case worth naming — a symlink
    /// whose real location is outside the resolved target. Following that link
    /// would serve a file from outside the tree the permission gate authorized,
    /// so it is refused rather than read.
    Unreadable,
}
