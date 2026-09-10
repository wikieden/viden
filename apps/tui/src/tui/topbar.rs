use super::{
    canvas::Frame,
    state::{Lens, TuiState, provider_status},
    text::{pad, truncate},
};

pub(super) fn render_top_bar(frame: &mut Frame, state: &TuiState) {
    let status = provider_status(state);
    let surface_key = match state.ui.lens {
        Lens::Welcome => None,
        Lens::Setup => Some("topbar.surface.setup"),
        Lens::Board => Some("topbar.surface.board"),
        Lens::Session => Some("topbar.surface.session"),
        Lens::Decisions => Some("topbar.surface.decisions"),
        Lens::Gallery => Some("topbar.surface.gallery"),
    };
    let surface = surface_key
        .map(|key| format!(" / {}", super::i18n::text(state, key)))
        .unwrap_or_default();
    frame.write_line(
        0,
        &pad(
            &format!(
                " VIDEN{surface}  {}  {}  {}",
                truncate(&state.runtime.snapshot.provider_family, 14),
                truncate(&state.runtime.snapshot.model_label, 18),
                status.connection
            ),
            frame.width,
        ),
    );
    let cwd = truncate(&state.runtime.snapshot.cwd.display().to_string(), 32);
    let approvals = state.runtime.pending_approvals.len().to_string();
    let context = super::i18n::translate(
        state,
        "topbar.context",
        &[
            ("cwd", cwd.as_str()),
            ("lane", state.ui.focused_lane.as_deref().unwrap_or("-")),
            (
                "session",
                if state.ui.session_id.is_empty() {
                    "-"
                } else {
                    state.ui.session_id.as_str()
                },
            ),
            ("approvals", approvals.as_str()),
        ],
    );
    frame.write_line(1, &pad(&format!(" {context}"), frame.width));
}

pub(super) fn render_side_top_bar(frame: &mut Frame, state: &TuiState) {
    let title = super::i18n::text(state, "topbar.side.title");
    frame.write_line(0, &pad(&format!(" {title}"), frame.width));
    let cwd = truncate(&state.runtime.snapshot.cwd.display().to_string(), 42);
    let count = state.runtime.lanes.len().to_string();
    let context = super::i18n::translate(
        state,
        "topbar.side.context",
        &[("cwd", cwd.as_str()), ("count", count.as_str())],
    );
    frame.write_line(1, &pad(&format!(" {context}"), frame.width));
}

pub(super) fn render_ops_top_bar(frame: &mut Frame, state: &TuiState) {
    let title = super::i18n::text(state, "topbar.ops.title");
    frame.write_line(0, &pad(&format!(" {title}"), frame.width));
    let evidence = state.runtime.latest_evidence.len().to_string();
    let errors = state.runtime.errors.len().to_string();
    let context = super::i18n::translate(
        state,
        "topbar.ops.context",
        &[
            (
                "context",
                state
                    .runtime
                    .context
                    .as_ref()
                    .map_or("-", |context| context.bundle_id.as_str()),
            ),
            ("evidence", evidence.as_str()),
            ("errors", errors.as_str()),
        ],
    );
    frame.write_line(1, &pad(&format!(" {context}"), frame.width));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cockpit_chrome_uses_core_resolved_locale_without_translating_identifiers() {
        let mut state = TuiState::default();
        state.runtime.snapshot.ui_preferences.locale = viden_core::LocaleId::ZhCn;
        state.runtime.snapshot.provider_family = "fallback".to_string();
        state.runtime.snapshot.model_label = "test-local".to_string();
        state.ui.lens = Lens::Session;
        state.ui.focus_lane("lane-7".to_string());
        state.ui.session_id = "session-9".to_string();
        let mut frame = Frame::new(112, 24);

        render_top_bar(&mut frame, &state);

        let rendered = frame.to_string();
        assert!(rendered.contains("COCKPIT · 驾驶舱"));
        assert!(rendered.contains("lane lane-7"));
        assert!(rendered.contains("会话 session-9"));
        assert!(rendered.contains("fallback"));
        assert!(rendered.contains("test-local"));
    }

    #[test]
    fn side_and_ops_chrome_follow_core_resolved_locale() {
        let mut state = TuiState::default();
        state.runtime.snapshot.ui_preferences.locale = viden_core::LocaleId::ZhCn;
        state.runtime.snapshot.cwd = "/workspace/raw".into();
        let mut side = Frame::new(100, 4);
        let mut ops = Frame::new(100, 4);

        render_side_top_bar(&mut side, &state);
        render_ops_top_bar(&mut ops, &state);

        let side = side.to_string();
        let ops = ops.to_string();
        assert!(side.contains("AGENT 通道"));
        assert!(side.contains("/workspace/raw"));
        assert!(ops.contains("OPS · 运维"));
        assert!(ops.contains("上下文"));
    }
}
