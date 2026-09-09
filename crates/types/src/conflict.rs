//! Structured conflict contract (`runtime.conflict_content`, GUI-CORE-015).
//!
//! A merge or lane apply that fails used to publish one prose sentence — the
//! `reason` on a [`crate::ConflictBounce`], the `summary` on a
//! [`crate::LaneConflictView`]. A client could say *that* a patch conflicted
//! and never *what* conflicted, so the only way to see the collision was to
//! leave the product. This module is the typed shape that ends that.
//!
//! Three invariants make it honest:
//!
//! 1. **Two sides plus the patch preimage. Not a three-way merge.**
//!    [`ConflictHunk::ours`] is a read-only read of the target file where the
//!    hunk said its region was, [`ConflictHunk::theirs`] is the incoming
//!    hunk's new side, and [`ConflictHunk::base`] is that hunk's own
//!    preimage — the text the patch expected to find. Core computes no merge
//!    base and resolves nothing; a client must not present this as a
//!    three-way merge or offer to auto-resolve it.
//! 2. **Never fabricated.** Only the hunks the strict apply actually rejected
//!    are listed. A hunk the apply never reached, and a patch the parser could
//!    not locate a file header for, are absent rather than guessed — see
//!    `crates/tools/src/patch.rs`, the single producer.
//! 3. **Bounded and flagged.** [`MAX_CONFLICT_CONTENT_BYTES`] bounds the
//!    published lines. A file over the bound keeps its entry with
//!    [`ConflictFile::omitted`] set and no hunks, and the content sets
//!    [`ConflictContent::truncated`], so "not shown" is always
//!    distinguishable from "no conflict here".
//!
//! Everything here is additive: the fields that carry it are optional with
//! `skip_serializing_if`, so a record published without content encodes to
//! exactly the bytes it did before, and the enums are `#[non_exhaustive]`.

use serde::{Deserialize, Serialize};

use crate::ReviewedEvidenceBinding;

/// Byte bound on the lines one [`ConflictContent`] publishes.
///
/// 256 KiB, the same bound the diff reads and evidence text use. A conflict
/// payload rides an event that every connected client receives, so an
/// unbounded one would put a repository-sized body on the stream at the exact
/// moment the operator is already blocked.
pub const MAX_CONFLICT_CONTENT_BYTES: u32 = 256 * 1024;

/// What a failed apply collided with, as lines rather than prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictContent {
    /// What the `ours` side was read against. See [`ConflictBaseline`].
    pub baseline: ConflictBaseline,
    #[serde(default)]
    pub files: Vec<ConflictFile>,
    /// At least one file's hunks were dropped by
    /// [`MAX_CONFLICT_CONTENT_BYTES`].
    #[serde(default)]
    pub truncated: bool,
}

/// What the conflict was computed against.
///
/// `#[non_exhaustive]` so a later baseline kind — a content hash, a snapshot
/// id — cannot break a client match. [`Self::Unknown`] is a real answer, not a
/// default: it says Core held no baseline it could name, which a client must
/// render as "unknown baseline" rather than silently as the workspace `HEAD`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConflictBaseline {
    /// A Git revision the `ours` side was read from.
    Revision { sha: String },
    /// The canonical reviewed evidence a merge gate held as its baseline. This
    /// is the merge path's answer, because a gate's baseline is its evidence
    /// bindings rather than a bare commit.
    Evidence {
        bindings: Vec<ReviewedEvidenceBinding>,
    },
    /// Core could not name a baseline.
    Unknown,
}

/// One file the apply refused, with the hunks it refused inside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictFile {
    /// Target-relative, `/`-separated, no leading separator — the same
    /// spelling [`crate::DiffFile::path`] uses. An absolute path would leak
    /// the operator's home directory onto the wire.
    pub path: String,
    #[serde(default)]
    pub hunks: Vec<ConflictHunk>,
    /// The file conflicted but its lines were dropped by the byte bound. The
    /// entry survives, because a reviewer must be able to tell "not shown"
    /// from "this file was fine".
    #[serde(default)]
    pub omitted: bool,
}

/// One rejected hunk: what the file holds, what the patch carried, and what
/// the patch expected to find.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictHunk {
    /// 1-based first line of [`Self::ours`] in the current file, taken from
    /// the hunk's old-side start. `0` only when the patch declared none (a
    /// creation hunk, whose old side is empty).
    #[serde(default)]
    pub ours_start: u32,
    /// The current file's lines at the hunk's old range, read read-only and
    /// clamped to the file. Empty when that range is past the end of the file
    /// or the file does not exist.
    #[serde(default)]
    pub ours: Vec<String>,
    /// 1-based first line of [`Self::theirs`] in the patched file, taken from
    /// the hunk's new-side start.
    #[serde(default)]
    pub theirs_start: u32,
    /// The incoming hunk's new-side lines.
    #[serde(default)]
    pub theirs: Vec<String>,
    /// The hunk's own preimage: the lines it expected to find. `None` means
    /// the hunk had no preimage to show at all (a binary file); `Some(vec![])`
    /// means it expected an empty region, which is what a creation expects.
    /// The two are different facts and are encoded differently.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<Vec<String>>,
    pub reason: ConflictHunkReason,
}

/// Why the strict apply refused this hunk.
///
/// Classified by Core in one place, from what the apply saw, so no client
/// parses a message to decide what to offer. `#[non_exhaustive]` because a
/// later apply path will see rejections this one cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConflictHunkReason {
    /// The hunk's preimage was not found in the file.
    ContextMismatch,
    /// The hunk's preimage was not found, and the file already holds the
    /// hunk's new side at that range: the change appears to be in already.
    AlreadyApplied,
    /// The patch modifies or deletes a file that is not in the target tree.
    FileMissing,
    /// The patch deletes the file, but its preimage did not account for the
    /// whole file, so the file would survive the deletion.
    FileDeleted,
    /// The patch reported binary content, so there are no lines to match.
    Binary,
}
