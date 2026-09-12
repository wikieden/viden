//! Owner-scoped ordered transcript rows, as a client of the Core contract
//! (`runtime.transcript_rows`, C8, closes GUI-CORE-009).
//!
//! Before this capability the cockpit had no ordered user/assistant history at
//! all: `RuntimeViewState` carries a Lane's ACP conversation and an unscoped
//! `assistant_stream`, and neither answers "what happened in this owner's
//! conversation, in order". The two `transcript_user` / `transcript_assistant`
//! unavailable placeholders were the honest rendering of that, and they retire
//! here.
//!
//! Four rules shape this module, and each is a way the read could lie:
//!
//! - **One read in flight, correlated by `command_id`.** `TranscriptRowsLoaded`
//!   carries the id it answers, so a page naming another read is ignored
//!   outright rather than appended to this owner's conversation.
//! - **The scope is Core's own owner.** A Lane reads the exact
//!   `RuntimeOwner` Core bound to it, narrowed to workspace/project/Lane and
//!   deliberately *not* carrying Core's turn binding, which would answer "this
//!   turn's rows" for a question about the Lane. A Lane Core published no owner
//!   for is refused locally rather than read unscoped, because an unscoped
//!   answer under a Lane's name would show every Lane's conversation as that
//!   Lane's.
//! - **The cursor is opaque.** `older` travels back verbatim as `before`; the
//!   client never parses, constructs, or compares one, and `complete` is
//!   *stated* rather than left to the absence of a control.
//!   Core's own bounds decide where a page ends.
//! - **A row's shape is Core's.** Every field below is one Core published;
//!   `None` means Core did not know, never a substituted default. A row kind
//!   this build cannot name is still a row, labelled `unknown`, because
//!   dropping it would silently shorten a conversation.
//!
//! The flat row with a `kind` discriminant mirrors
//! [`crate::OperatorGitResultProjection`] and for the same reason: the wire
//! shape crosses into TypeScript, where a Rust enum would have to be
//! re-derived by hand.

use serde::Serialize;

use crate::d1::D1OutcomeProjection;

/// The frontend-contract-v1 capability that carries ordered transcript rows.
pub const TRANSCRIPT_ROWS_CAPABILITY: &str = "runtime.transcript_rows";

/// Rows this client asks for per page.
///
/// Core's own `DEFAULT_TRANSCRIPT_ROWS_PAGE`. Named here so the value the
/// cockpit sends is visible beside the read rather than buried in a call.
pub const TRANSCRIPT_ROWS_PAGE_LIMIT: u16 = 50;

/// The local code the transcript renders when this client has no Core-published
/// owner to scope a Lane read by.
///
/// A client-local defence, not a Core contract request: `QueryTranscriptRows`
/// exists and works, but the *scope* must be an owner Core itself bound, and a
/// cockpit with a selected Lane and no exact binding has none.
pub const TRANSCRIPT_ROWS_NO_OWNER_CODE: &str = "D1-TRANSCRIPT-ROWS-NO-OWNER";

/// One ordered transcript row, flattened for the webview.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptRowProjection {
    /// Core's own stable id, so a rendered row keys on it across pages and
    /// reconnects.
    pub id: String,
    /// `user`, `assistant`, `tool_call`, `tool_result`, `check_run`,
    /// `permission`, or `unknown` for a content shape this build cannot draw —
    /// `TranscriptRowContent` is `#[non_exhaustive]`.
    pub kind: &'static str,
    /// The Lane the row's owner names, or `None` for a session-scoped row.
    pub lane_id: Option<String>,
    /// Core's ordering position. An ordering key, never a count: positions are
    /// stable under append, so they are not contiguous and a gap is not a lost
    /// row.
    pub sequence: u64,
    /// `None` for a row Core recorded without a time, never a substituted zero.
    pub timestamp: Option<u64>,
    /// Prose, for a `user` or `assistant` row.
    pub text: Option<String>,
    /// Core's 8 KiB row bound cut `text`. Its own flag: a short body and a cut
    /// one are otherwise indistinguishable, and a reviewer reading half an
    /// answer as the whole one is the failure this prevents.
    pub truncated: bool,
    /// The canonical evidence row that holds this body or this result's work.
    /// Absence means no canonical bytes exist, not that they were withheld.
    pub evidence_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub input_preview: Option<String>,
    /// `tool_result` only.
    pub success: Option<bool>,
    /// Core's bounded summary, for a `tool_result` or a `check_run`.
    pub summary: Option<String>,
    pub check_id: Option<String>,
    /// `check_run` only: the label Core gave the check.
    pub label: Option<String>,
    pub command: Option<String>,
    /// Core's own `CheckRunStatus` tag for a `check_run` row.
    pub status: Option<&'static str>,
    pub failing_location: Option<String>,
    /// `permission` only: the approval request this decision answered.
    pub request_id: Option<String>,
    /// `permission` only. `None` is Core saying it does not know: the durable
    /// audit row keeps no payload for a scoped allow, so publishing
    /// `allow_once` for one would misreport the operator's decision.
    pub decision: Option<String>,
    /// `permission` only: the durable audit row this fact was read from.
    pub audit_id: Option<String>,
}

/// One answered (or refused) transcript read.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptRowsProjection {
    /// `idle`, `pending`, `confirmed`, or `rejected` with Core's own reason.
    pub outcome: D1OutcomeProjection,
    pub pending_command_id: Option<String>,
    /// False when Core's handshake published no `runtime.transcript_rows`.
    pub capability_available: bool,
    /// Whether at least one page has been answered for the current scope.
    /// Distinct from an empty `rows`: "not read yet" and "this owner has no
    /// rows" are different facts and the renderer draws them differently.
    pub loaded: bool,
    /// Oldest first, newest last — the order a transcript renders in. An older
    /// page is prepended, which is the order Core pages in.
    pub rows: Vec<TranscriptRowProjection>,
    /// Core's opaque cursor for the page above these rows, or `None` when the
    /// scope is exhausted. Passed back verbatim; never parsed.
    pub older: Option<String>,
    /// Core's own statement that no older row matches the scope.
    pub complete: bool,
    /// The Lane this page was read for, or `None` for an unscoped read. Held
    /// so a Lane switch drops the previous Lane's rows rather than rendering
    /// them under the new Lane's name.
    pub scope_lane_id: Option<String>,
}

impl TranscriptRowsProjection {
    /// The projection a client renders before anything has been read.
    pub fn idle(capability_available: bool) -> Self {
        Self {
            outcome: D1OutcomeProjection::idle(),
            pending_command_id: None,
            capability_available,
            loaded: false,
            rows: Vec::new(),
            older: None,
            complete: false,
            scope_lane_id: None,
        }
    }
}
