//! The DiffReview action side, projected from Core's operator source-control
//! actions (`runtime.operator_git`, GUI-CORE-020).
//!
//! Core is the only actor. The client names an action, waits for the ordered
//! answer, and renders it; it never runs `git`, never shells out, and never
//! reads a fact out of `output`, which is display text under an 8 KiB bound.
//!
//! Three distinctions this module exists to keep on the wire, because each one
//! collapses into a lie if the frontend has to guess it:
//!
//! - **Refused vs failed.** A `CommandRejected` happened *before* anything ran
//!   — a malformed action, an escaping path, plan mode, a deny rule, a denied
//!   approval — and carries Core's own reason. A `Failed` outcome happened
//!   *after* the gate granted the action, so the effect was attempted and
//!   audited. Telling an operator
//!   "denied" about a non-fast-forward push would send them to the permission
//!   file instead of to `git fetch`.
//! - **Classified vs verbatim.** [`OperatorGitResultProjection::failure_class`]
//!   is the machine-readable half a frontend keys its localized copy and its
//!   recovery offer on; `detail` is git's own sentence, kept verbatim for a
//!   human. A client that matched English substrings would break the moment
//!   the operator's git speaks another language.
//! - **Resampled vs inferred.** `Completed` carries the target's source facts
//!   *after* the effect. The success line and the sync chip read those; they
//!   never derive "ahead is now 0" from a push transcript.

use serde::{Deserialize, Serialize};

use crate::{D1OutcomeProjection, D1WorkspaceSourceProjection};

/// The frontend-contract-v1 capability that carries operator git actions.
///
/// The exact id Core publishes in its handshake
/// (`FRONTEND_V1_EXTENSION_CAPABILITIES`); the client must not invent a
/// finer-grained one, because an unpublished id can never become available.
pub const OPERATOR_GIT_CAPABILITY: &str = "runtime.operator_git";

/// `ApprovalTarget.kind` Core stamps on every operator source-control ask.
///
/// One kind for all five actions on purpose: they are one surface to an
/// operator — the commit bar — so a dock groups them as one decision instead
/// of splitting `git_add` from `git_commit` across two rows.
pub const OPERATOR_GIT_APPROVAL_KIND: &str = "git";

/// The local code the bar renders when this client has no Core-published owner
/// to act as.
///
/// A client-local defence, not a Core contract request: `RunOperatorGitAction`
/// exists and works, but the *actor* must be an owner Core itself bound, and a
/// cockpit with no exactly-bound Lane has none. Sending a default owner would
/// record an authorized mutation as belonging to nobody.
pub const OPERATOR_GIT_NO_OWNER_CODE: &str = "D1-OPERATOR-GIT-OWNER";

/// One operator action as the frontend names it.
///
/// Mirrors Core's `OperatorGitAction` rather than wrapping it so the wire
/// shape stays camelCase like every other GUI intent. The conversion — and
/// with it *Core's own* validator, imported rather than reimplemented — lives
/// in `projection.rs`, the one module allowed to hold Core contract types.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum OperatorGitIntent {
    /// Empty `paths` means every changed path — Core's own "stage all".
    Stage {
        paths: Vec<String>,
    },
    Unstage {
        paths: Vec<String>,
    },
    Commit {
        message: String,
    },
    #[serde(rename_all = "camelCase")]
    Push {
        remote: Option<String>,
        set_upstream: bool,
    },
    Fetch {
        remote: Option<String>,
    },
}

/// What Core answered for one settled action.
///
/// `Completed` and `Failed` share one struct with disjoint halves rather than
/// two variants, because the wire shape crosses into TypeScript where a
/// discriminated union would have to be re-derived; [`Self::kind`] is the
/// discriminant both sides read.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperatorGitResultProjection {
    /// `completed` or `failed`. Never `rejected`: a refusal never reaches
    /// here, because no effect was attempted.
    pub kind: &'static str,
    /// The stable dotted verb Core audits the action under, echoed from the
    /// finished event rather than remembered from the request.
    pub action: &'static str,
    /// The Lane Core's answer names, or `None` for the workspace root.
    pub target_lane_id: Option<String>,
    /// The authorization record appended *before* the effect.
    pub audit_id: String,
    /// `Completed` only. Display text under Core's 8 KiB bound; every fact a
    /// client needs is typed elsewhere.
    pub output: Option<String>,
    /// The 8 KiB bound cut `output`. Its own flag, because a short output and
    /// a truncated one are otherwise indistinguishable.
    pub truncated: bool,
    /// `Completed` only: the target's facts resampled *after* the effect.
    pub source: Option<D1WorkspaceSourceProjection>,
    /// `Failed` only: the class the localized message and the recovery offer
    /// are keyed on. `unknown` for a class this build cannot name — the enum
    /// is `#[non_exhaustive]`, and squeezing an unrecognized failure into the
    /// nearest-looking class would send the operator down the wrong recovery.
    pub failure_class: Option<&'static str>,
    /// `Failed` only: git's own message, verbatim.
    pub detail: Option<String>,
}

/// What the commit bar and the titlebar sync control may render.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperatorGitProjection {
    /// `idle`, `pending`, `confirmed` (Core answered — see
    /// [`OperatorGitResultProjection::kind`] for what it answered), or
    /// `rejected` with Core's pre-effect reason.
    pub outcome: D1OutcomeProjection,
    pub pending_command_id: Option<String>,
    /// The audit verb of the action in flight, so the bar names what it is
    /// waiting on rather than saying "working".
    pub pending_action: Option<&'static str>,
    /// Core published a pending approval with `target.kind = "git"` for the
    /// acting owner. A Core fact, not a timeout guess: the bar says "awaiting
    /// approval" and stays inert until the dock resolves it.
    pub awaiting_approval: bool,
    pub result: Option<OperatorGitResultProjection>,
    /// False when Core's handshake published no `runtime.operator_git`.
    pub capability_available: bool,
    /// Whether Core published exactly one owner binding this client may act
    /// as. False leaves the bar visible, disabled, and labelled.
    pub owner_available: bool,
    /// The local reason there is no owner, in this client's own words so it is
    /// never mistaken for a Core refusal.
    pub owner_unavailable_reason: Option<String>,
}
