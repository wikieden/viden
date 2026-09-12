//! The workspace-scoped operator identity (`runtime.workspace_owner`,
//! GUI-CORE-027).
//!
//! Every audited mutation in Viden names a [`crate::RuntimeOwner`]. Until this
//! capability there was exactly one way to obtain a real one: run inside a
//! Lane. A commit made from the cockpit with no Lane selected therefore had no
//! actor at all, so both clients refused it locally rather than file an
//! authorized source-control change under `RuntimeOwner::default()`, which
//! names nobody. This module is the fact that ends that refusal.
//!
//! Three rules shape it:
//!
//! 1. **The workspace scope is a scope, not a fallback.** A workspace owner
//!    carries `workspace_id` and `project_id` and nothing else: `lane_id`,
//!    `session_id`, `task_id`, and `turn_id` are `None`. A binding that
//!    carries any of them is refused by [`WorkspaceRuntimeOwnerBinding::validate`]
//!    rather than trimmed, because a trimmed one would let a Lane's identity
//!    become the actor every workspace-target mutation is audited under.
//! 2. **The identity is derivable, and says so.** `workspace_id` is
//!    `ws_` plus the first 16 hex characters of SHA-256 over the canonical
//!    root path, so the same directory on the same machine always produces the
//!    same id with no store to consult — and moving the repository changes it.
//!    That is a stated property, not a bug: the id names a *location*.
//!    `project_id` is the durable half and lives in `.viden/project.toml`, so
//!    it survives a move.
//! 3. **Where the id came from is published.** [`ProjectIdOrigin`] separates
//!    "this project already had an id" from "Core minted one on this open".
//!    An operator who sees a new project id in an audit trail must be able to
//!    tell which of the two happened without diffing a file Core owns.

use serde::{Deserialize, Serialize};

use crate::RuntimeOwner;

/// Prefix every minted workspace id carries.
///
/// Named once so the minting site, the fixtures, and a client's own validation
/// cannot drift into three spellings.
pub const WORKSPACE_ID_PREFIX: &str = "ws_";

/// Hex characters of the canonical-root digest a workspace id carries.
///
/// Sixteen: 64 bits of a SHA-256, which is far more than enough to separate
/// the directories one operator opens and short enough to stay readable in an
/// audit row. A workspace id is an identity for a location on one machine, not
/// a security boundary, so collision resistance beyond that buys nothing.
pub const WORKSPACE_ID_DIGEST_CHARS: usize = 16;

/// Prefix every minted project id carries.
pub const PROJECT_ID_PREFIX: &str = "prj_";

/// How Core obtained the `project_id` it published.
///
/// `#[non_exhaustive]`: a future origin (an id supplied by a workspace host,
/// say) must not break a client's match arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProjectIdOrigin {
    /// Read from `.viden/project.toml`.
    Existing,
    /// Minted on this open and written to `.viden/project.toml`.
    Minted,
}

/// The owner Core publishes for work scoped to the workspace root rather than
/// to one Lane.
///
/// `canonical_root` is carried beside the owner because the whole identity is
/// derived from it: a client that wants to explain *why* the workspace id
/// changed after a move has the input in the same fact as the output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceRuntimeOwnerBinding {
    pub canonical_root: String,
    pub owner: RuntimeOwner,
    /// How `project_id` was obtained: read from `.viden/project.toml`, or
    /// minted on this open and written there.
    pub project_id_origin: ProjectIdOrigin,
}

impl WorkspaceRuntimeOwnerBinding {
    /// Rejects a binding that does not describe the workspace scope.
    ///
    /// Called by the producer before publishing *and* by the reducer before
    /// storing, because the reducer is the last place a client can be kept
    /// from treating a payload as an authority Core never published. Both ends
    /// refuse rather than normalize: a binding with a Lane in it is a
    /// different scope, and silently dropping the Lane would publish an owner
    /// nobody asked for.
    pub fn validate(&self) -> Result<(), String> {
        if self.canonical_root.trim().is_empty() {
            return Err("workspace owner binding requires a canonical root".to_string());
        }
        if self.owner.workspace_id.trim().is_empty() {
            return Err("workspace owner binding requires a workspace id".to_string());
        }
        if self.owner.project_id.trim().is_empty() {
            return Err("workspace owner binding requires a project id".to_string());
        }
        if self.owner.lane_id.is_some()
            || self.owner.session_id.is_some()
            || self.owner.task_id.is_some()
            || self.owner.turn_id.is_some()
        {
            return Err(
                "a workspace owner is scoped to the workspace and carries no lane, session, \
                 task, or turn"
                    .to_string(),
            );
        }
        Ok(())
    }
}

/// Whether `owner` may act on the workspace target under `bound`.
///
/// The rule is deliberately "same workspace and project", not full equality.
/// A Lane-scoped operator acting on the workspace root is a real case — the
/// TUI's `/git` with a Lane focused, a Lane worker committing the shared tree
/// — and its audit record should name the Lane it came from rather than be
/// rewritten to the bare workspace owner. What must never pass is an owner
/// that names *no* workspace, because that is the `RuntimeOwner::default()`
/// case GUI-CORE-027 is about: an authorized mutation filed as belonging to
/// nobody.
pub fn workspace_owner_authorizes(bound: Option<&RuntimeOwner>, owner: &RuntimeOwner) -> bool {
    if owner.workspace_id.trim().is_empty() || owner.project_id.trim().is_empty() {
        return false;
    }
    match bound {
        Some(bound) => {
            owner.workspace_id == bound.workspace_id && owner.project_id == bound.project_id
        }
        // Core published no workspace owner (an engine built without a host
        // binding). A non-empty identity is still an identity, so it is
        // accepted exactly as it was before this capability existed; only the
        // empty one, refused above, changes behavior.
        None => true,
    }
}
