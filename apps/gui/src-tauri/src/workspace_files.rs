//! D1 command-palette file scope, projected from the Core workspace inventory
//! (GUI-CORE-022).
//!
//! The palette's `~` scope lists files. The client is forbidden from producing
//! that list itself — walking the workspace is outside the client boundary and
//! bypasses the permission gate every other path read passes — so every row
//! here comes from a `WorkspaceFilesLoaded` page Core published, or there is no
//! row.
//!
//! Absence, emptiness, and refusal stay three different facts on the wire:
//! `capability_available` says whether Core publishes an inventory at all,
//! `loaded` says whether a page has actually arrived, and an empty `entries`
//! on a loaded projection is a genuinely empty workspace. The frontend owns the
//! localized sentence for each; it must never collapse them into one empty
//! list.

use serde::Serialize;

use crate::D1OutcomeProjection;

/// The frontend-contract-v1 capability that carries the workspace inventory.
///
/// This is the id Core publishes in its handshake
/// (`FRONTEND_V1_EXTENSION_CAPABILITIES`); the client must not invent a
/// finer-grained one, because an unpublished id can never become available.
pub const WORKSPACE_FILES_CAPABILITY: &str = "runtime.workspace_files";

/// Page size for one `QueryWorkspaceFiles`.
///
/// Core clamps to `1..=MAX_WORKSPACE_FILE_PAGE_SIZE`, so this is a readability
/// choice, not a protocol bound: one page the palette can fuzzy-match over
/// without pausing on a workspace-sized payload.
pub const WORKSPACE_FILES_PAGE_LIMIT: u32 = 500;

/// One inventory entry, exactly as Core published it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileRowProjection {
    /// Workspace-relative, `/`-separated. Never an absolute path.
    pub path: String,
    /// `file`, `dir`, or `unknown` for a kind this build cannot name —
    /// `WorkspaceFileKind` is `#[non_exhaustive]`, and mislabelling an
    /// unmodeled kind as a file would be worse than saying so.
    pub kind: String,
    /// Byte size for a file Core could stat. Absent for a directory, and also
    /// absent for a file whose metadata Core could not read: absence means
    /// "not known", never zero.
    pub size_bytes: Option<u64>,
}

/// What the palette may render after one `QueryWorkspaceFiles`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFilesProjection {
    pub outcome: D1OutcomeProjection,
    /// Lexicographic by path, exactly as Core delivered.
    pub entries: Vec<WorkspaceFileRowProjection>,
    /// True when no further entry matches the read.
    pub complete: bool,
    /// Whether at least one page has arrived. Absence and emptiness are
    /// different facts: "nothing loaded yet" must never render as "no files".
    pub loaded: bool,
    pub pending_command_id: Option<String>,
    /// False when Core's handshake published no `runtime.workspace_files`.
    pub capability_available: bool,
}

/// The frontend-contract-v1 capability that carries a single file's content
/// (`runtime.workspace_file_reads`, C9).
///
/// Separate from [`WORKSPACE_FILES_CAPABILITY`] because Core publishes them
/// separately: a build may list the tree and not read it, and the inspector's
/// Open has to say which of the two is missing.
pub const WORKSPACE_FILE_READS_CAPABILITY: &str = "runtime.workspace_file_reads";

/// What Core published for one file, flattened for the webview.
///
/// The three bodies are deliberately not collapsible. `text` is bytes Core
/// will publish, `binary` is bytes it will not, and `unavailable` is nothing to
/// read with the reason attached — and a client shown one for another would
/// render an empty editor over a binary asset or over a file it was never
/// allowed to open.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileFactsProjection {
    /// The normalized target-relative path Core resolved, echoed from the
    /// answer rather than from the request: a read sent as `./src/lib.rs` is
    /// keyed on the one spelling Core used.
    pub path: String,
    /// `text`, `binary`, `unavailable`, or `unknown` for a body shape this
    /// build cannot draw — `WorkspaceFileBody` is `#[non_exhaustive]`, and
    /// drawing an unmodelled body as empty text would be a fabricated file.
    pub body: &'static str,
    /// The published prefix, for a `text` body only.
    pub text: Option<String>,
    /// Core's byte bound cut the body. Its own flag: a short file and a cut
    /// one are otherwise indistinguishable.
    pub truncated: bool,
    /// Byte length of the *whole* file. `None` when there was no file to
    /// measure — never a substituted zero, which would read as "empty file"
    /// for a path that is not there.
    pub size: Option<u64>,
    /// SHA-256 of the whole file, never of the published prefix. `None` when
    /// there were no bytes to hash.
    pub sha256: Option<String>,
    /// `not_found`, `directory`, `unreadable`, or `unknown` for an
    /// `unavailable` body whose reason this build cannot name. `None` for
    /// every other body.
    pub reason: Option<&'static str>,
}

/// One answered (or refused) single-file read.
///
/// There is no `loaded` flag: every answer Core publishes carries a body, so
/// `file.is_none()` is exactly "nothing has been answered yet" and an
/// answered-with-nothing read is an `unavailable` body with its reason.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileProjection {
    /// `idle`, `pending`, `confirmed`, or `rejected` with Core's own reason.
    /// A rejection is where the permission gate's refusal and the path
    /// validator's refusal arrive, both in Core's words.
    pub outcome: D1OutcomeProjection,
    pub pending_command_id: Option<String>,
    /// False when Core's handshake published no `runtime.workspace_file_reads`.
    pub capability_available: bool,
    /// The path this client asked for, held so a refusal can say which read
    /// was refused: Core's rejection reason names the path, but the pending
    /// and idle states have no answer to read one from.
    pub requested_path: Option<String>,
    /// The Lane whose worktree was read, or `None` for the workspace root.
    pub target_lane_id: Option<String>,
    /// Core's answer, or `None` while nothing has been answered.
    pub file: Option<WorkspaceFileFactsProjection>,
}

impl WorkspaceFileProjection {
    /// The projection a client renders before anything has been read.
    pub fn idle(capability_available: bool) -> Self {
        Self {
            outcome: D1OutcomeProjection::idle(),
            pending_command_id: None,
            capability_available,
            requested_path: None,
            target_lane_id: None,
            file: None,
        }
    }
}
