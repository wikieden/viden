//! Operator source-control actions (`runtime.operator_git`, GUI-CORE-020).
//!
//! A registered DiffReview surface needs more than rows to read: it needs a
//! commit bar. This module is the typed half of that — what an operator may
//! ask Core to do to the index and the remote, and what came back.
//!
//! Three rules shape every type here:
//!
//! 1. **One vocabulary with agents.** Each action maps to exactly one existing
//!    `git_*` tool spec (see `crates/tools/src/git/`), so a single `viden.toml`
//!    allow/ask/deny rule set governs an operator's commit bar and an agent's
//!    `git_commit` call. There is no operator-only git implementation and no
//!    operator-only permission vocabulary.
//! 2. **Clients never parse output.** A failure is classified by Core into
//!    [`OperatorGitFailureClass`] from git's own stderr, once, in one place.
//!    A frontend renders a localized message per class; it never greps
//!    `stderr` for `has no upstream branch`, which would break the moment the
//!    operator's git speaks another language.
//! 3. **Refused and failed are different facts.** A pre-effect refusal is a
//!    `CommandRejected`; a failure *after* the permission was granted is an
//!    [`OperatorGitOutcome::Failed`] event, because the effect was attempted
//!    and the attempt is audited. Collapsing the two would let a client tell
//!    an operator "denied" when git actually ran and rejected the push.

use serde::{Deserialize, Serialize};

use crate::WorkspaceSourceView;

/// Largest git output an outcome may carry.
///
/// Eight kibibytes: enough for a commit summary or a push transcript, small
/// enough that a pathological hook cannot put a repository-sized payload on
/// the event stream. Output beyond the bound is cut and
/// [`OperatorGitOutcome::Completed::truncated`] says so.
pub const MAX_OPERATOR_GIT_OUTPUT_BYTES: usize = 8 * 1024;

/// Largest commit message an operator action may carry.
pub const MAX_OPERATOR_COMMIT_MESSAGE_BYTES: usize = 4 * 1024;

/// What an operator asks Core to do to a source-control target.
///
/// `#[non_exhaustive]`: the deliberate exclusions recorded in
/// `docs/release-0.3.3-contract-design.md` (pull, merge, rebase, amend, reset,
/// force push, switch, stash) are exclusions for `0.3.3`, not forever, and a
/// newer Core that adds one must not break this build's match arms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OperatorGitAction {
    /// Stage paths. Empty `paths` means every changed path.
    Stage {
        paths: Vec<String>,
    },
    /// Remove paths from the index, leaving the working tree alone. Empty
    /// `paths` means every staged path.
    Unstage {
        paths: Vec<String>,
    },
    Commit {
        message: String,
    },
    Push {
        /// `None` uses git's default remote name, `origin`.
        remote: Option<String>,
        set_upstream: bool,
    },
    Fetch {
        remote: Option<String>,
    },
}

impl OperatorGitAction {
    /// The stable dotted verb this action is audited under.
    ///
    /// A token, never prose: the audit timeline must stay translatable and
    /// diffable, so the verb is what a reader joins on.
    pub fn audit_verb(&self) -> &'static str {
        match self {
            Self::Stage { .. } => "stage",
            Self::Unstage { .. } => "unstage",
            Self::Commit { .. } => "commit",
            Self::Push { .. } => "push",
            Self::Fetch { .. } => "fetch",
        }
    }

    /// Rejects an action that cannot mean what it says, before any process
    /// spawns.
    ///
    /// Every rejection here becomes a `CommandRejected`. None of them is
    /// answered with a no-op success: "nothing was staged" and "your request
    /// was refused" are different facts, and only the second one tells an
    /// operator what to do next.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Stage { paths } | Self::Unstage { paths } => {
                for path in paths {
                    validate_operator_git_path(path)?;
                }
                Ok(())
            }
            Self::Commit { message } => {
                if message.trim().is_empty() {
                    // An empty message makes `git commit` open an editor in a
                    // non-interactive child, which hangs instead of failing.
                    return Err("operator git commit requires a non-empty message".to_string());
                }
                if message.len() > MAX_OPERATOR_COMMIT_MESSAGE_BYTES {
                    return Err(format!(
                        "operator git commit message exceeds {MAX_OPERATOR_COMMIT_MESSAGE_BYTES} bytes"
                    ));
                }
                Ok(())
            }
            Self::Push { remote, .. } | Self::Fetch { remote } => match remote {
                Some(remote) => validate_operator_git_remote(remote),
                None => Ok(()),
            },
        }
    }
}

/// Wording every out-of-target path refusal shares.
///
/// Named once so the `PathOutsideRepository` failure class and the pre-effect
/// refusal describe the same condition with the same words: an operator who
/// sees one and then the other must not think they hit two different problems.
pub const OPERATOR_GIT_PATH_OUTSIDE_REPOSITORY: &str = "path is outside the repository target";

fn validate_operator_git_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("operator git path cannot be empty".to_string());
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(format!(
            "{OPERATOR_GIT_PATH_OUTSIDE_REPOSITORY}: `{path}` must be target-relative"
        ));
    }
    if path.contains('\\') {
        return Err(format!(
            "operator git path `{path}` must use `/` separators"
        ));
    }
    // `C:\...` and `C:/...`: an absolute Windows path wearing a relative
    // spelling.
    if path.len() > 1 && path.as_bytes()[1] == b':' {
        return Err(format!(
            "{OPERATOR_GIT_PATH_OUTSIDE_REPOSITORY}: `{path}` must be target-relative"
        ));
    }
    if path.split('/').any(|segment| segment == "..") {
        return Err(format!(
            "{OPERATOR_GIT_PATH_OUTSIDE_REPOSITORY}: `{path}` leaves the target"
        ));
    }
    if path.contains('\0') {
        return Err(format!("operator git path `{path}` contains a null byte"));
    }
    Ok(())
}

/// A remote is a git refname-ish token, never a URL and never an option.
///
/// Rejecting a leading `-` is what keeps a remote name from being read by git
/// as a flag; rejecting the rest keeps a client from smuggling a URL where
/// Core's audit record promises a stable token.
fn validate_operator_git_remote(remote: &str) -> Result<(), String> {
    if remote.is_empty() || remote.len() > 100 {
        return Err("operator git remote name is empty or too long".to_string());
    }
    if remote.starts_with('-') {
        return Err(format!(
            "operator git remote `{remote}` cannot start with `-`"
        ));
    }
    if !remote
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/'))
    {
        return Err(format!(
            "operator git remote `{remote}` must name a configured remote"
        ));
    }
    if remote.split('/').any(|segment| segment == "..") {
        return Err(format!(
            "operator git remote `{remote}` must name a configured remote"
        ));
    }
    Ok(())
}

/// What happened after the permission gate said yes.
///
/// `#[non_exhaustive]` so a future outcome (a partial push, say) cannot break
/// a client match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OperatorGitOutcome {
    Completed {
        /// Git's combined output, bounded by
        /// [`MAX_OPERATOR_GIT_OUTPUT_BYTES`]. Display text only: a client
        /// renders it, it never parses it for facts, because every fact it
        /// would want is already typed here or in `source`.
        output: String,
        /// The bound cut the output. Its own flag, because a short output and
        /// a truncated one are indistinguishable otherwise.
        truncated: bool,
        /// The target's source-control facts resampled *after* the effect, so
        /// a client's branch/ahead/dirty chip reflects what the action did
        /// rather than what it predicted.
        source: WorkspaceSourceView,
    },
    Failed {
        class: OperatorGitFailureClass,
        /// Git's own message, bounded and kept verbatim for a human reader.
        /// The machine-readable half is `class`.
        detail: String,
    },
}

/// Why an operator action failed, classified by Core from git's stderr.
///
/// The classification exists so a frontend can render a localized explanation
/// and offer the right next step (`NoUpstream` -> offer `set_upstream`,
/// `NonFastForward` -> offer fetch) without matching English substrings that
/// change with git's version and the operator's locale.
///
/// `Other` is deliberate and permanent: a class this build cannot recognize is
/// reported as unclassified with the real `detail`, never squeezed into the
/// nearest-looking class, which would send a client down the wrong recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OperatorGitFailureClass {
    NothingToCommit,
    NonFastForward,
    AuthenticationRequired,
    RemoteUnreachable,
    NoUpstream,
    PathOutsideRepository,
    Other,
}
