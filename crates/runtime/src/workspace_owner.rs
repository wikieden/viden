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
//! dependency of this crate and only a dev-dependency of `viden-core`.
//!
//! Binding lives lower still — in the bootstrap every frontend entrypoint
//! already funnels through, see [`bind_workspace_owner_at_root`]. While the
//! host performed it, a host that did not use `LocalCoreHost` published no
//! identity for a whole session; `apps/cli` was exactly that host.

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

/// Mints and installs the workspace owner for an engine being bootstrapped —
/// the one binding path every host shares.
///
/// Called from `bootstrap_runtime_with_context`, the single funnel behind every
/// frontend entrypoint: `LocalCoreHost::open_workspace` for the GUI and the
/// embedders, and `bootstrap_runtime_with_resolved_config` for `apps/cli` and
/// its legacy `--no-tui` REPL. Binding here rather than in each caller is what
/// closes compatibility follow-up 11: the host was the only caller, `apps/cli`
/// does not use it, so a whole `viden` session ran with
/// `RuntimeViewState.workspace_owner` absent and every workspace-target
/// mutation refused for naming no actor.
///
/// The root is canonicalized first because `workspace_id` is a digest over the
/// root *path*: two callers naming one directory by different paths must not
/// mint two identities for one tree. A root this fails on is an error rather
/// than an unbound engine — an unbound engine is the failure this capability
/// exists to end, and it must not be reachable by falling back.
pub(crate) fn bind_workspace_owner_at_root(
    engine: &mut SessionEngine,
    root: &Path,
) -> Result<(), String> {
    let canonical_root = root.canonicalize().map_err(|error| {
        format!(
            "failed to resolve the workspace root {} for the workspace owner: {error}",
            root.display()
        )
    })?;
    engine.bind_workspace_owner(mint_workspace_owner_binding(&canonical_root)?);
    Ok(())
}

impl SessionEngine {
    /// Installs the workspace owner minted for the workspace root at open.
    ///
    /// Called once by [`bind_workspace_owner_at_root`] during bootstrap,
    /// before any caller starts the supervisor, so the binding is available to
    /// the very first snapshot prefix.
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
