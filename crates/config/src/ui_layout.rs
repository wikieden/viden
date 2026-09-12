//! Persistence for the cockpit layout record (`ui.layout_preferences`).
//!
//! This is the appearance-preference machinery next door
//! (`crates/config/src/ui_preferences.rs`) applied to a *different* record,
//! and it is deliberately separate rather than a wider `UiPreferences`:
//! `ResolvedUiPreferences` is serialized into every `RuntimeSnapshot`, so one
//! more field on it would move the recorded digest of all nine frozen
//! `frontend-contract-v1` base fixtures.
//!
//! Two rules that do not follow from the appearance module:
//!
//! 1. **An appearance reset is not a layout reset.** `[ui.layout]` lives under
//!    `[ui]` for readability, but the two records answer to two commands.
//!    [`take_layout_table`] is what lets `reset_user_ui_preferences_at` drop
//!    the appearance profile while carrying the layout table across, so an
//!    operator who resets their theme does not find their sidebar rearranged.
//! 2. **A bad stored value costs one field, not the record.** An unreadable
//!    `lane_sidebar_mode` falls back to the default *for that field* and
//!    reports a diagnostic; refusing the whole table would strand every other
//!    layout choice over one key a newer build wrote.

use std::path::Path;

use toml::{Value, map::Map};
use viden_types::{
    LaneSidebarMode, UiLayoutPreferencePatch, UiLayoutPreferences, UiPreferenceDiagnostic,
};

use crate::ui_preferences::{atomic_write_config, read_config_value};

/// The layout record plus whether it reached disk and what Core could not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiLayoutPreferenceFileState {
    pub preferences: UiLayoutPreferences,
    /// `false` means Core applied the record for this session but could not
    /// write it; the reason is in `diagnostics`. A client must render that
    /// difference rather than report a saved preference that will not survive
    /// a restart.
    pub persisted: bool,
    pub diagnostics: Vec<UiPreferenceDiagnostic>,
}

/// Reads the stored layout record, replacing unreadable fields with their
/// defaults and naming each one.
pub fn resolve_user_ui_layout_preferences_at(
    path: &Path,
) -> Result<UiLayoutPreferenceFileState, String> {
    let value = read_config_value(path)?;
    let mut diagnostics = Vec::new();
    let preferences = parse_layout_preferences(layout_table(&value), &mut diagnostics);
    Ok(UiLayoutPreferenceFileState {
        preferences,
        // Nothing was written, so nothing failed to be written. A read is
        // reported as persisted because the record it returns *is* the one on
        // disk.
        persisted: true,
        diagnostics,
    })
}

/// Applies `patch` to the stored record and writes it.
///
/// The patch is validated first, so an over-bound or malformed request never
/// reaches the file: the bytes on disk after a refusal are exactly the bytes
/// that were there before.
pub fn save_user_ui_layout_preferences_at(
    path: &Path,
    patch: &UiLayoutPreferencePatch,
) -> Result<UiLayoutPreferenceFileState, String> {
    patch.validate()?;
    let mut value = read_config_value(path)?;
    let mut diagnostics = Vec::new();
    let mut preferences = parse_layout_preferences(layout_table(&value), &mut diagnostics);
    apply_layout_patch(&mut preferences, patch);

    let root = value
        .as_table_mut()
        .ok_or_else(|| format!("Config {} must be a TOML table", path.display()))?;
    let ui = root
        .entry("ui".to_string())
        .or_insert_with(|| Value::Table(Map::new()))
        .as_table_mut()
        .ok_or_else(|| format!("Config {} [ui] must be a TOML table", path.display()))?;
    let layout = ui
        .entry("layout".to_string())
        .or_insert_with(|| Value::Table(Map::new()))
        .as_table_mut()
        .ok_or_else(|| format!("Config {} [ui.layout] must be a TOML table", path.display()))?;
    write_layout_table(layout, &preferences)?;

    match atomic_write_config(path, &value, None) {
        Ok(()) => Ok(UiLayoutPreferenceFileState {
            preferences,
            persisted: true,
            diagnostics,
        }),
        // The record still governs this session: refusing to apply it because
        // the disk is read-only would be a worse answer than applying it and
        // saying it will not survive a restart.
        Err(error) => {
            diagnostics.push(write_failure_diagnostic(&error));
            Ok(UiLayoutPreferenceFileState {
                preferences,
                persisted: false,
                diagnostics,
            })
        }
    }
}

/// Removes `[ui.layout]` and returns the defaults, leaving `[ui]` alone.
pub fn reset_user_ui_layout_preferences_at(
    path: &Path,
) -> Result<UiLayoutPreferenceFileState, String> {
    let mut value = read_config_value(path)?;
    let root = value
        .as_table_mut()
        .ok_or_else(|| format!("Config {} must be a TOML table", path.display()))?;
    if let Some(ui) = root.get_mut("ui").and_then(Value::as_table_mut) {
        ui.remove("layout");
    }
    let mut diagnostics = Vec::new();
    match atomic_write_config(path, &value, None) {
        Ok(()) => Ok(UiLayoutPreferenceFileState {
            preferences: UiLayoutPreferences::default(),
            persisted: true,
            diagnostics,
        }),
        Err(error) => {
            diagnostics.push(write_failure_diagnostic(&error));
            Ok(UiLayoutPreferenceFileState {
                preferences: UiLayoutPreferences::default(),
                persisted: false,
                diagnostics,
            })
        }
    }
}

/// Lifts the `[ui.layout]` table out of an `[ui]` table about to be dropped.
///
/// Used by the appearance reset so the two records stay independent; see the
/// module note.
pub(crate) fn take_layout_table(root: &mut Map<String, Value>) -> Option<Value> {
    root.get_mut("ui")
        .and_then(Value::as_table_mut)
        .and_then(|ui| ui.remove("layout"))
}

/// Puts a lifted `[ui.layout]` table back after the appearance profile was
/// removed.
pub(crate) fn restore_layout_table(root: &mut Map<String, Value>, layout: Value) {
    let mut ui = Map::new();
    ui.insert("layout".to_string(), layout);
    root.insert("ui".to_string(), Value::Table(ui));
}

fn layout_table(value: &Value) -> Option<&Map<String, Value>> {
    value.get("ui")?.get("layout")?.as_table()
}

fn parse_layout_preferences(
    table: Option<&Map<String, Value>>,
    diagnostics: &mut Vec<UiPreferenceDiagnostic>,
) -> UiLayoutPreferences {
    let Some(table) = table else {
        return UiLayoutPreferences::default();
    };
    UiLayoutPreferences {
        lane_sidebar_mode: parse_lane_sidebar_mode(table, diagnostics),
        hidden_statusbar_segments: parse_hidden_segments(table, diagnostics),
    }
}

fn parse_lane_sidebar_mode(
    table: &Map<String, Value>,
    diagnostics: &mut Vec<UiPreferenceDiagnostic>,
) -> LaneSidebarMode {
    let key = "lane_sidebar_mode";
    match table.get(key) {
        None => LaneSidebarMode::default(),
        Some(Value::String(raw)) => match raw.as_str() {
            "pinned" => LaneSidebarMode::Pinned,
            "floating" => LaneSidebarMode::Floating,
            other => {
                diagnostics.push(layout_diagnostic(key, other));
                LaneSidebarMode::default()
            }
        },
        Some(other) => {
            diagnostics.push(layout_diagnostic(key, crate::value_kind(other)));
            LaneSidebarMode::default()
        }
    }
}

fn parse_hidden_segments(
    table: &Map<String, Value>,
    diagnostics: &mut Vec<UiPreferenceDiagnostic>,
) -> Vec<String> {
    let key = "hidden_statusbar_segments";
    let Some(raw) = table.get(key) else {
        return Vec::new();
    };
    let Some(items) = raw.as_array() else {
        diagnostics.push(layout_diagnostic(key, crate::value_kind(raw)));
        return Vec::new();
    };
    let mut segments = Vec::new();
    for item in items {
        match item.as_str() {
            // Names are kept verbatim, including ones this build does not
            // recognize: the statusbar vocabulary belongs to the client.
            Some(segment) => segments.push(segment.to_string()),
            None => diagnostics.push(layout_diagnostic(key, crate::value_kind(item))),
        }
    }
    // The stored list is truncated rather than refused: a file written by a
    // newer build with a larger bound must still load, and the alternative —
    // failing the read — would strand the whole record.
    segments.truncate(viden_types::MAX_HIDDEN_STATUSBAR_SEGMENTS);
    segments
}

fn apply_layout_patch(preferences: &mut UiLayoutPreferences, patch: &UiLayoutPreferencePatch) {
    if let Some(mode) = patch.lane_sidebar_mode {
        preferences.lane_sidebar_mode = mode;
    }
    if let Some(segments) = &patch.hidden_statusbar_segments {
        preferences.hidden_statusbar_segments = segments.clone();
    }
}

fn write_layout_table(
    table: &mut Map<String, Value>,
    preferences: &UiLayoutPreferences,
) -> Result<(), String> {
    table.insert(
        "lane_sidebar_mode".to_string(),
        Value::try_from(preferences.lane_sidebar_mode)
            .map_err(|error| format!("failed to serialize the lane sidebar mode: {error}"))?,
    );
    table.insert(
        "hidden_statusbar_segments".to_string(),
        Value::Array(
            preferences
                .hidden_statusbar_segments
                .iter()
                .map(|segment| Value::String(segment.clone()))
                .collect(),
        ),
    );
    Ok(())
}

fn layout_diagnostic(field: &str, rejected: &str) -> UiPreferenceDiagnostic {
    UiPreferenceDiagnostic::new(
        "ui.layout.invalid_value",
        "ui.layout",
        format!("ui.layout.{field}"),
        Some(rejected.to_string()),
    )
}

fn write_failure_diagnostic(error: &str) -> UiPreferenceDiagnostic {
    UiPreferenceDiagnostic::new(
        "ui.layout.not_persisted",
        "ui.layout",
        "ui.layout",
        Some(error.to_string()),
    )
}
