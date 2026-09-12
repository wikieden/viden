//! Cockpit layout preferences, as a client of the Core contract
//! (`ui.layout_preferences`, C5).
//!
//! Two seams the navigation shell (G3) deliberately held in memory close here:
//! the Lane sidebar's `D-SIDEBAR` mode and the statusbar's `D-STATUSBAR`
//! ambient set. Both were presentation state with no home, because the
//! frontend contract makes Core the single preference authority and the design
//! prototype's `localStorage` keys would have been the second one it forbids.
//!
//! Three rules shape what this module publishes, and each is the reason a field
//! exists rather than being derived:
//!
//! - **The webview keeps no copy beyond the current view.**
//!   [`LayoutPreferencesProjection::lane_sidebar_mode`] is `None` until Core
//!   publishes a record, and the cockpit renders the
//!   design's own default in that case rather than remembering the last value
//!   it saw. A client-held copy is how two authorities start disagreeing.
//! - **`persisted: false` is a different fact from confirmed.** Core can apply
//!   a layout for this session and fail to write the file; the operator must
//!   see that rather than be told their cockpit will come back rearranged.
//!   Hence `Option<bool>`: `None` before any command answered, `Some(false)`
//!   for an applied-but-unwritten record.
//! - **An unmodeled mode is not folded into a known one.** Core's
//!   `LaneSidebarMode` is `#[non_exhaustive]`; a mode this build cannot draw is
//!   published as `unknown` so the cockpit falls back to the design default
//!   visibly rather than rendering a pinned column for a mode that is not
//!   pinned.
//!
//! Like `ui_preferences.rs`, this module holds the transport-safe vocabulary
//! only. The two conversions that touch Core's own enum
//! (`lane_sidebar_mode_name`, `typed_lane_sidebar_mode`) live in
//! `projection.rs`, which is the module allowed to hold Core contract types —
//! `tests/architecture_boundary.rs` enforces that split.

use serde::{Deserialize, Serialize};

use crate::d1::D1OutcomeProjection;
use crate::projection::PreferenceDiagnosticProjection;

/// The frontend-contract-v1 capability that carries the layout record.
///
/// The exact id Core publishes in its handshake
/// (`FRONTEND_V1_EXTENSION_CAPABILITIES`); the client must not invent a
/// finer-grained one, because an unpublished id can never become available.
pub const LAYOUT_PREFERENCES_CAPABILITY: &str = "ui.layout_preferences";

/// The layout axes as the webview sends them.
///
/// Every field is optional and `None` means "leave it as it is", never
/// "reset it": a reset is its own command, so a client that omits an axis
/// cannot erase a preference it never meant to touch.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutPreferencePatchInput {
    pub lane_sidebar_mode: Option<String>,
    pub hidden_statusbar_segments: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayoutPreferenceIntent {
    Set { patch: LayoutPreferencePatchInput },
    Reset,
}

/// What the cockpit may render for the layout record.
///
/// The record half (`lane_sidebar_mode`, `hidden_statusbar_segments`) is read
/// from `RuntimeViewState`, so a snapshot prefix alone fills it and a
/// reconnecting cockpit needs no command. The receipt half (`persisted`,
/// `diagnostics`) comes only from a confirming `UiLayoutPreferencesUpdated`,
/// because the view state carries neither.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutPreferencesProjection {
    pub outcome: D1OutcomeProjection,
    pub pending_command_id: Option<String>,
    pub capability_available: bool,
    /// `pinned`, `floating`, or `unknown` for a mode this build cannot draw.
    /// `None` means Core published no record at all.
    pub lane_sidebar_mode: Option<String>,
    /// Names kept exactly as Core stored them, including ones this build's
    /// statusbar vocabulary does not contain: the operator hid them, and a
    /// client that dropped the unknown ones would quietly unhide them.
    pub hidden_statusbar_segments: Vec<String>,
    /// Whether the last answered command reached the config file. `None`
    /// before any command answered in this session.
    pub persisted: Option<bool>,
    pub diagnostics: Vec<PreferenceDiagnosticProjection>,
}
