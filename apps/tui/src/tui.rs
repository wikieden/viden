mod app;
mod audit_panel;
mod canvas;
mod client;
// Deterministic design previews still exercise the richer selector surface;
// production runtime facts enter through the CoreClient loop in `app`.
#[allow(dead_code)]
mod command_palette;
mod composer;
mod composer_buffer;
mod conflict_rows;
mod decision;
mod diff_rows;
mod evidence_panel;
mod geometry;
mod glyphs;
mod i18n;
mod indicators;
#[allow(dead_code)]
mod input;
mod jump;
mod keymap;
#[allow(dead_code)]
mod lane;
mod lane_presenter;
#[allow(dead_code)]
mod modal;
mod operator_git;
mod ops_screen;
mod palette;
mod panel;
mod pending;
mod preferences;
mod preview;
mod projection;
mod render;
mod right_rail;
#[allow(dead_code)]
mod screen;
mod side_screen;
// Task 3 removes the remaining legacy persistence/read helpers. They are not
// called from the production CoreClient loop in Tasks 1-2.
#[allow(dead_code)]
mod state;
mod statusbar;
// Pure supervision intent builders, mirroring `lane`. The merge-gate, review,
// conflict, and revert builders are dispatched from the supervision decision
// overlay; the handoff/contract/dependency builders stay unused until the
// creation flows land and carry their own local allowance.
mod supervision;
#[allow(dead_code)]
mod terminal;
mod text;
#[allow(dead_code)]
mod theme;
mod topbar;
mod transcript;
mod ui_state;
mod workspace_files;

pub use app::{TuiError, TuiOptions, run_tui};
pub use preview::{
    render_ansi_approval_hunks_preview_with_theme, render_ansi_cjk_input_preview_with_theme,
    render_ansi_command_palette_preview_with_theme, render_ansi_conflict_detail_preview_with_theme,
    render_ansi_evidence_detail_preview_with_theme, render_ansi_evidence_list_preview_with_theme,
    render_ansi_git_outcome_preview_with_theme, render_ansi_git_picker_preview_with_theme,
    render_ansi_idle_preview_with_theme, render_ansi_lane_preview_with_theme,
    render_ansi_lane_selector_preview_with_theme, render_ansi_live_turn_preview_with_theme,
    render_ansi_model_selector_preview_with_theme, render_ansi_ops_preview_with_theme,
    render_ansi_preview_with_theme, render_ansi_provider_detail_preview_with_theme,
    render_ansi_provider_selector_preview_with_theme, render_ansi_resize_preview_with_theme,
    render_ansi_setup_wizard_preview_with_theme, render_ansi_side_preview_with_theme,
    render_approval_hunks_preview, render_cjk_input_preview, render_command_palette_preview,
    render_conflict_detail_preview, render_evidence_detail_preview, render_evidence_list_preview,
    render_git_outcome_preview, render_git_picker_preview, render_idle_preview,
    render_lane_preview, render_lane_selector_preview, render_live_turn_preview,
    render_model_selector_preview, render_ops_preview, render_preview,
    render_provider_detail_preview, render_provider_selector_preview, render_resize_preview,
    render_setup_wizard_preview, render_side_preview,
};

pub fn is_known_theme(name: &str) -> bool {
    theme::TuiTheme::is_known(name)
}

pub fn theme_names() -> &'static [&'static str] {
    theme::TuiTheme::names()
}
