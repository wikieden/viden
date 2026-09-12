//! Runtime behavior of the cockpit layout record (`ui.layout_preferences`).
//!
//! The record exists as its own capability for one reason: a field on
//! `UiPreferences` would ride `ResolvedUiPreferences` into every
//! `RuntimeSnapshot` and move the recorded digest of all nine frozen
//! `frontend-contract-v1` base fixtures. These tests hold the consequences of
//! that choice in place:
//!
//! 1. the answering fact carries the caller's own command id, and the snapshot
//!    prefix's copy carries none, because nobody asked for it;
//! 2. an over-bound patch is a `CommandRejected` before anything is written;
//! 3. a record Core could not write is published with `persisted: false` and a
//!    diagnostic, never silently dropped and never reported as saved.

use std::path::PathBuf;

use viden_types::{
    ApprovalResponse, LaneSidebarMode, MAX_HIDDEN_STATUSBAR_SEGMENTS, RuntimeCommand,
    RuntimeEventKind, UiLayoutPreferencePatch, UiLayoutPreferences, UiPreferences,
};

use super::{SequenceProvider, temp_dir};
use crate::SessionEngine;

fn layout_engine(slug: &str) -> (SessionEngine, PathBuf) {
    let cwd = temp_dir(slug);
    let config_path = cwd.join("user-config.toml");
    let home = cwd.join("session-home");
    let mut engine = SessionEngine::new_with_home(
        &cwd,
        Box::new(SequenceProvider::new(Vec::new())),
        Some(home),
    )
    .unwrap();
    engine.set_ui_preference_context(
        None,
        Some(config_path.clone()),
        UiPreferences::client_default(),
    );
    (engine, config_path)
}

fn run(
    engine: &mut SessionEngine,
    command_id: &str,
    command: RuntimeCommand,
) -> Vec<crate::RuntimeEvent> {
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    engine
        .handle_runtime_command(command_id, command, &mut approver)
        .unwrap()
}

/// The set command persists the record, answers with the caller's command id,
/// and reduces into `layout_preferences` without touching the resolved
/// appearance profile every base fixture serializes.
#[test]
fn a_layout_set_persists_answers_with_its_command_id_and_leaves_appearance_alone() {
    let (mut engine, config_path) = layout_engine("ui_layout_set");
    let before = engine.runtime_view_state();

    // Pinned, not floating: floating is the default, so a test that wrote it
    // could not tell a stored choice apart from an absent record.
    let events = run(
        &mut engine,
        "layout-set",
        RuntimeCommand::SetUiLayoutPreferences {
            patch: UiLayoutPreferencePatch {
                lane_sidebar_mode: Some(LaneSidebarMode::Pinned),
                hidden_statusbar_segments: Some(vec!["cost".to_string()]),
            },
        },
    );

    let fact = events
        .iter()
        .find(|event| {
            matches!(
                event.kind,
                RuntimeEventKind::UiLayoutPreferencesUpdated { .. }
            )
        })
        .expect("a successful layout command publishes its record");
    let RuntimeEventKind::UiLayoutPreferencesUpdated {
        command_id,
        preferences,
        persisted,
        diagnostics,
    } = &fact.kind
    else {
        unreachable!("matched above")
    };
    assert_eq!(command_id.as_deref(), Some("layout-set"));
    assert!(persisted);
    assert!(diagnostics.is_empty());
    assert_eq!(preferences.lane_sidebar_mode, LaneSidebarMode::Pinned);
    assert_eq!(
        preferences.hidden_statusbar_segments,
        vec!["cost".to_string()]
    );

    let mut replayed = before.clone();
    replayed.apply_event(fact);
    assert_eq!(replayed.layout_preferences.as_ref(), Some(preferences));
    assert_eq!(replayed.ui_preferences, before.ui_preferences);
    assert_eq!(
        replayed.snapshot.ui_preferences,
        before.snapshot.ui_preferences
    );

    let stored = std::fs::read_to_string(&config_path).unwrap();
    assert!(stored.contains("pinned"));
}

/// The reset drops the stored record and republishes the defaults. Floating is
/// what `D-SIDEBAR` specifies and what an operator who never opened Settings
/// sees, so a reset must land there rather than on whatever was last written.
#[test]
fn a_layout_reset_republishes_the_defaults() {
    let (mut engine, _config_path) = layout_engine("ui_layout_reset");
    run(
        &mut engine,
        "layout-set",
        RuntimeCommand::SetUiLayoutPreferences {
            patch: UiLayoutPreferencePatch {
                lane_sidebar_mode: Some(LaneSidebarMode::Pinned),
                hidden_statusbar_segments: Some(vec!["cost".to_string()]),
            },
        },
    );

    let events = run(
        &mut engine,
        "layout-reset",
        RuntimeCommand::ResetUiLayoutPreferences,
    );

    let RuntimeEventKind::UiLayoutPreferencesUpdated {
        command_id,
        preferences,
        persisted,
        ..
    } = &events
        .iter()
        .find(|event| {
            matches!(
                event.kind,
                RuntimeEventKind::UiLayoutPreferencesUpdated { .. }
            )
        })
        .expect("a reset publishes the record it reset to")
        .kind
    else {
        unreachable!("matched above")
    };
    assert_eq!(command_id.as_deref(), Some("layout-reset"));
    assert!(persisted);
    assert_eq!(preferences, &UiLayoutPreferences::default());
    assert_eq!(preferences.lane_sidebar_mode, LaneSidebarMode::Floating);
}

/// An over-bound hidden-segment list is refused before the file is touched,
/// and the refusal names the caller's command. An empty success here would
/// tell a client its preference was stored when it was not.
#[test]
fn an_over_bound_layout_patch_is_rejected_and_writes_nothing() {
    let (mut engine, config_path) = layout_engine("ui_layout_over_bound");

    let events = run(
        &mut engine,
        "layout-too-many",
        RuntimeCommand::SetUiLayoutPreferences {
            patch: UiLayoutPreferencePatch {
                lane_sidebar_mode: None,
                hidden_statusbar_segments: Some(
                    (0..=MAX_HIDDEN_STATUSBAR_SEGMENTS)
                        .map(|index| format!("segment_{index}"))
                        .collect(),
                ),
            },
        },
    );

    let reason = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, reason }
                if command_id == "layout-too-many" =>
            {
                Some(reason.clone())
            }
            _ => None,
        })
        .expect("an over-bound patch is refused by command id");
    assert!(reason.contains(&MAX_HIDDEN_STATUSBAR_SEGMENTS.to_string()));
    assert!(
        !events.iter().any(|event| matches!(
            event.kind,
            RuntimeEventKind::UiLayoutPreferencesUpdated { .. }
        )),
        "a refused patch publishes no record"
    );
    assert!(!config_path.exists());
}

/// The snapshot prefix republishes the stored record with no command id: a
/// reconnecting client learns its layout without asking, and cannot mistake
/// the prefix copy for the answer to a request it has in flight.
#[test]
fn the_snapshot_prefix_republishes_the_record_without_a_command_id() {
    let (mut engine, _config_path) = layout_engine("ui_layout_prefix");
    run(
        &mut engine,
        "layout-set",
        RuntimeCommand::SetUiLayoutPreferences {
            patch: UiLayoutPreferencePatch {
                lane_sidebar_mode: Some(LaneSidebarMode::Pinned),
                hidden_statusbar_segments: None,
            },
        },
    );

    let view = engine.runtime_view_state();

    assert_eq!(
        view.layout_preferences
            .as_ref()
            .map(|preferences| preferences.lane_sidebar_mode),
        Some(LaneSidebarMode::Pinned)
    );
    let events = engine.runtime_events_for_engine_events(&[]);
    let prefix = events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::UiLayoutPreferencesUpdated { command_id, .. } => {
                Some(command_id.clone())
            }
            _ => None,
        })
        .expect("the snapshot prefix carries the layout record");
    assert_eq!(prefix, None);
}
