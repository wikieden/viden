//! What Core knows about the change an approval would make
//! (`runtime.structured_diff`, GUI-CORE-012).
//!
//! An approval used to arrive as `input_preview`: a flat `key: value` dump
//! that says a file will be edited but not what the edit is. A reviewer
//! deciding on that is deciding blind, so Core computes the *prospective*
//! result here and publishes it as rows.
//!
//! Two rules govern this module and neither is negotiable:
//!
//! 1. **Read-only.** Building a preview reads the target file and nothing
//!    else. It never creates, writes, or truncates, because the whole point
//!    of an approval is that the operator can still say no. The preview runs
//!    before the permission decision, so a write here would mutate against a
//!    deny.
//! 2. **Absence is a fact.** A tool Core cannot preview gets `None`, not an
//!    empty [`DiffDocument`] — an empty document renders as "this changes
//!    nothing", which is a different and much worse claim.
//!
//! Stated limitation, recorded rather than hidden: the preview is computed at
//! approval time and execution later runs the proposed tool input against
//! whatever the file holds then. A file changed in between produces a
//! different result. [`DecisionContext::base_sha256`] is what lets a client or
//! an audit reader detect that afterwards; a pre-execution recheck is not in
//! `0.3.3`.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use viden_tools::{FilesystemCapability, LocalFilesystem, patch::parse_diff_document, render_diff};
use viden_types::{DecisionContext, ToolInput, WorkspaceChangeKind};

/// Byte bound for a previewed diff.
///
/// Deliberately the same 64 KiB the completed change publishes under
/// (`frontend_status::MAX_COCKPIT_PATCH_BYTES`), so the preview a reviewer
/// approves and the change they see afterwards are bounded identically and
/// cannot disagree about what was truncated.
pub(crate) const MAX_DECISION_CONTEXT_DIFF_BYTES: u32 = 64 * 1024;

/// Builds the decision context for one proposed tool call, if Core can.
///
/// Only the two tools whose whole effect is a known file mutation are
/// previewable: `edit_file` replaces the first occurrence of `old` with `new`,
/// and `write_file` replaces the file wholesale. `shell`, `git_*`, and every
/// other mutating tool run an external process whose effect Core cannot
/// predict without running it, so they get `None` rather than a guess.
pub(crate) fn tool_decision_context(
    cwd: &Path,
    tool_name: &str,
    input: &ToolInput,
) -> Option<DecisionContext> {
    if !matches!(tool_name, "edit_file" | "write_file") {
        return None;
    }
    let raw_path = input.get("path")?.trim();
    if raw_path.is_empty() {
        return None;
    }
    // The same resolution the tool itself performs, so the preview describes
    // the file execution would touch and not a neighbouring one.
    let resolved = resolve_tool_path(cwd, raw_path);

    // The same capability seam the tool reaches the filesystem through: one
    // swap relocates the tool and its preview together.
    let fs = LocalFilesystem;
    if fs.is_dir(&resolved) {
        return None;
    }
    let before = fs.read(&resolved).ok();
    let base_sha256 = before
        .as_ref()
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)));
    // A file Core could not read has no preimage to hash. Publishing the hash
    // of nothing would let a client believe it held a real base.
    let before_text = before
        .as_ref()
        .map(|bytes| String::from_utf8_lossy(bytes).to_string());
    let kind = if before.is_some() {
        WorkspaceChangeKind::Modified
    } else {
        WorkspaceChangeKind::Added
    };
    let existing = before_text.clone().unwrap_or_default();

    let after = match tool_name {
        "write_file" => input.get("content")?.clone(),
        _ => {
            let old = input.get("old")?;
            let new = input.get("new")?;
            if !existing.contains(old.as_str()) {
                // The edit would fail at execution, so there is no
                // prospective content to show. The base hash still ships:
                // it is what tells a reviewer which bytes Core looked at.
                return Some(DecisionContext {
                    diff: None,
                    base_sha256,
                });
            }
            existing.replacen(old.as_str(), new, 1)
        }
    };

    // Rendered by the same function the executed tool publishes its diff
    // with, then read back by the one promoted parser, so the preview and the
    // completed change are two views of one computation rather than two
    // independent ones.
    let rendered = render_diff(&existing, &after);
    let mut document = parse_diff_document(&rendered, MAX_DECISION_CONTEXT_DIFF_BYTES);
    // `render_diff` writes placeholder `before`/`after` headers, so Core
    // stamps the path and classification it actually resolved. The tool input
    // is the authoritative source for both; the rendered header is not.
    let display_path = workspace_relative_display(cwd, &resolved, raw_path);
    for file in &mut document.files {
        file.path = display_path.clone();
        file.old_path = None;
        file.kind = kind;
    }
    Some(DecisionContext {
        diff: Some(document),
        base_sha256,
    })
}

/// Builds the decision context for a multi-file patch whose canonical bytes
/// Core already holds (the trust loop's `MergeAgentPatch`).
///
/// No file is read here at all: the patch text *is* the proposal, and it was
/// already validated against canonical evidence before this point. There is no
/// single preimage, so no `base_sha256`: one hash cannot describe several
/// files, and a hash of one of them would invite a client to check the wrong
/// thing.
pub(crate) fn patch_decision_context(unified_diff: &str) -> DecisionContext {
    DecisionContext {
        diff: Some(parse_diff_document(
            unified_diff,
            MAX_DECISION_CONTEXT_DIFF_BYTES,
        )),
        base_sha256: None,
    }
}

fn resolve_tool_path(cwd: &Path, raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}

/// Publishes a workspace-relative path.
///
/// An absolute path would put the operator's home directory on the wire and
/// into every recorded fixture. A path outside the workspace keeps the raw
/// input the caller wrote, which is what the operator typed and already sees.
fn workspace_relative_display(cwd: &Path, resolved: &Path, raw: &str) -> String {
    resolved
        .strip_prefix(cwd)
        .ok()
        .map(|relative| {
            relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        })
        .filter(|relative| !relative.is_empty())
        .unwrap_or_else(|| raw.to_string())
}
