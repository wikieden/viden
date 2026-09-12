//! Minting and holding the workspace-scoped operator identity
//! (`runtime.workspace_owner`, GUI-CORE-027).
//!
//! The two halves of a workspace owner are obtained differently on purpose:
//!
//! - `workspace_id` is *derived*: `ws_` plus the first
//!   [`WORKSPACE_ID_DIGEST_CHARS`] hex characters of SHA-256 over the
//!   canonical root path. No store is consulted, so the same directory always
//!   mints the same id and a client can recompute it — and moving the
//!   repository changes it, which is the honest consequence of naming a
//!   location rather than a thing.
//! - `project_id` is *durable*: read from `.viden/project.toml`, minted and
//!   written there on the first open. That is the half that survives a move,
//!   and it is the one an audit trail joins on.
//!
//! Minting deliberately lives below the host rather than in
//! `LocalCoreHost::open_workspace`: the digest needs `sha2`, which is a
//! dependency of this crate and only a dev-dependency of `viden-core`. The
//! host still performs both steps at open; it just calls this to do them.

use std::path::Path;

use sha2::{Digest, Sha256};
use viden_config::read_or_mint_project_id_at;
use viden_types::{
    RuntimeOwner, WORKSPACE_ID_DIGEST_CHARS, WORKSPACE_ID_PREFIX, WorkspaceRuntimeOwnerBinding,
};

use crate::SessionEngine;

/// Derives the workspace id for a canonical root.
///
/// Lowercase hex, truncated: a workspace id identifies one directory on one
/// machine, not a security boundary, so 64 bits of digest is ample and the
/// short form stays readable in an audit row.
pub fn workspace_id_for_root(canonical_root: &Path) -> String {
    let digest = Sha256::digest(canonical_root.to_string_lossy().as_bytes());
    let hex = format!("{digest:x}");
    format!("{WORKSPACE_ID_PREFIX}{}", &hex[..WORKSPACE_ID_DIGEST_CHARS])
}

/// Mints (or re-reads) the workspace owner binding for a canonical root.
///
/// Writes `.viden/project.toml` only when the project has no usable id yet;
/// every later open reads that exact id back, because a second minted id would
/// split one project's audit history into two identities.
pub fn mint_workspace_owner_binding(
    canonical_root: &Path,
) -> Result<WorkspaceRuntimeOwnerBinding, String> {
    let (project_id, project_id_origin) = read_or_mint_project_id_at(canonical_root)?;
    let binding = WorkspaceRuntimeOwnerBinding {
        canonical_root: canonical_root.to_string_lossy().to_string(),
        owner: RuntimeOwner {
            workspace_id: workspace_id_for_root(canonical_root),
            project_id,
            // The workspace scope is a scope, not a fallback: it names no
            // Lane, session, task, or turn.
            lane_id: None,
            session_id: None,
            task_id: None,
            turn_id: None,
        },
        project_id_origin,
    };
    // Validated at the producer as well as in the reducer: a binding that does
    // not describe the workspace scope must never reach the event bus, because
    // the first fact after `SnapshotUpdated` is the one every later audit
    // record is filed under.
    binding.validate()?;
    Ok(binding)
}

impl SessionEngine {
    /// Installs the workspace owner the host minted at open.
    ///
    /// Called once, before the supervisor starts, so the binding is available
    /// to the very first snapshot prefix.
    pub fn bind_workspace_owner(&mut self, binding: WorkspaceRuntimeOwnerBinding) {
        self.workspace_owner_binding = Some(binding);
    }

    /// The workspace-scoped owner Core published, if any.
    ///
    /// `None` means this engine was built without a host binding — the direct
    /// `SessionEngine::new` paths and every test that does not opt in. It is
    /// never a defaulted owner: "Core published no workspace identity" and
    /// "the identity is empty" are the same fabricated-actor failure this
    /// capability exists to end.
    pub fn workspace_owner(&self) -> Option<&RuntimeOwner> {
        self.workspace_owner_binding
            .as_ref()
            .map(|binding| &binding.owner)
    }

    pub(crate) fn workspace_owner_binding(&self) -> Option<&WorkspaceRuntimeOwnerBinding> {
        self.workspace_owner_binding.as_ref()
    }
}
