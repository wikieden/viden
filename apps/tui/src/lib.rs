//! Viden TUI app boundary.
//!
//! This crate owns terminal rendering and input orchestration. It consumes the
//! transport-neutral `viden_core::CoreClient` surface and must not own runtime,
//! provider, tool, permission, or workflow business logic.

mod tui;

pub use tui::{
    TuiError, TuiOptions, is_known_theme, render_ansi_approval_hunks_preview_with_theme,
    render_ansi_cjk_input_preview_with_theme, render_ansi_command_palette_preview_with_theme,
    render_ansi_idle_preview_with_theme, render_ansi_lane_preview_with_theme,
    render_ansi_lane_selector_preview_with_theme, render_ansi_live_turn_preview_with_theme,
    render_ansi_model_selector_preview_with_theme, render_ansi_ops_preview_with_theme,
    render_ansi_preview_with_theme, render_ansi_provider_detail_preview_with_theme,
    render_ansi_provider_selector_preview_with_theme, render_ansi_resize_preview_with_theme,
    render_ansi_setup_wizard_preview_with_theme, render_ansi_side_preview_with_theme,
    render_approval_hunks_preview, render_cjk_input_preview, render_command_palette_preview,
    render_idle_preview, render_lane_preview, render_lane_selector_preview,
    render_live_turn_preview, render_model_selector_preview, render_ops_preview, render_preview,
    render_provider_detail_preview, render_provider_selector_preview, render_resize_preview,
    render_setup_wizard_preview, render_side_preview, run_tui, theme_names,
};
