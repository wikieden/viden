use super::{
    canvas::Frame,
    geometry::effective_layout_width,
    panel::{bordered_row, panel_top},
    state::TuiState,
    text::{bottom_border, char_width, pad, truncate},
};

pub(super) const MIN_COMPOSER_HEIGHT: usize = 6;
const WELCOME_BOX_MAX_WIDTH: usize = 96;
const WELCOME_BOX_MIN_WIDTH: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(super) struct ComposerAnchor {
    pub(super) left: usize,
    pub(super) input_row: usize,
    pub(super) width: usize,
}

pub(super) fn should_render_welcome(state: &TuiState) -> bool {
    state.ui.focused_lane.is_none()
        && !super::state::has_active_work(state)
        && !state.ui.entries.iter().any(is_session_entry)
}

pub(super) fn render_welcome(frame: &mut Frame, state: &TuiState) {
    let width = frame.width;
    let height = frame.height;
    let box_width = welcome_box_width(width);
    let box_left = width.saturating_sub(box_width) / 2;
    let input_rows = welcome_input_rows(state, box_width);
    let composer_top = welcome_composer_top(width, height, input_rows.len());
    let logo_top = composer_top.saturating_sub(if width >= 100 { 8 } else { 4 });

    let logo = welcome_logo(width);
    for (offset, line) in logo.iter().enumerate() {
        frame.write_line(logo_top + offset, &centered_line(width, line));
    }

    for (offset, row) in input_rows.iter().enumerate() {
        frame.write_line(
            composer_top + 1 + offset,
            &positioned_line(width, box_left, row),
        );
    }
    let after_input = composer_top + 1 + input_rows.len();
    frame.write_line(
        after_input,
        &positioned_line(width, box_left, &welcome_spacer_row(box_width)),
    );
    frame.write_line(
        after_input + 1,
        &positioned_line(width, box_left, &welcome_context_row(state, box_width)),
    );
    frame.write_line(
        after_input + 3,
        &positioned_line(width, box_left, &welcome_hint_row(state, box_width)),
    );

    let bottom = welcome_status_row(state, width);
    frame.write_line(height.saturating_sub(2), &bottom);
}

pub(super) fn render_composer(frame: &mut Frame, state: &TuiState, bottom_bar_height: usize) {
    let rows = composer_rows(state, frame.width);
    let height = rows.len() + 5;
    let top = frame.height - height - bottom_bar_height;
    frame.write_line(top, &panel_top("Viden >_", frame.width, None));
    frame.write_line(top + 1, &composer_input_spacer_row(frame.width));
    for (index, row) in rows.iter().enumerate() {
        frame.write_line(
            top + 2 + index,
            &composer_input_row(state, frame.width, row, index == 0),
        );
    }
    let after_input = top + 2 + rows.len();
    frame.write_line(after_input, &composer_input_spacer_row(frame.width));
    frame.write_line(
        after_input + 1,
        &bordered_row(
            &composer_actions(state, frame.width.saturating_sub(4)),
            frame.width,
        ),
    );
    frame.write_line(after_input + 2, &bottom_border(frame.width));
}

pub(super) fn composer_height(state: &TuiState, width: usize) -> usize {
    (composer_rows(state, width).len() + 5).max(MIN_COMPOSER_HEIGHT)
}

fn composer_rows(state: &TuiState, width: usize) -> Vec<String> {
    if !state.ui.input.is_empty() {
        state
            .ui
            .input
            .visible_rows(composer_content_width(state, width))
    } else if super::state::has_active_work(state) {
        let count = state.runtime.queued_inputs.len();
        if count == 0 {
            vec![super::i18n::text(state, "composer.active")]
        } else {
            let count =
                if state.runtime.snapshot.ui_preferences.locale == viden_core::LocaleId::ZhCn {
                    count.to_string()
                } else {
                    queued_prompt_count_label(count)
                };
            vec![super::i18n::translate(
                state,
                "composer.queued",
                &[("count", count.as_str())],
            )]
        }
    } else {
        vec![super::i18n::text(state, "composer.prompt")]
    }
}

pub(super) fn composer_content_width(state: &TuiState, layout_width: usize) -> usize {
    if should_render_welcome(state) {
        welcome_content_width(welcome_box_width(layout_width))
    } else {
        layout_width.saturating_sub(32).max(1)
    }
}

fn composer_input_row(state: &TuiState, width: usize, value: &str, first: bool) -> String {
    let content_width = width.saturating_sub(26);
    let input = format!(
        "{}{}",
        if first { "› " } else { "  " },
        pad(
            &truncate(value, content_width.saturating_sub(2)),
            content_width.saturating_sub(6)
        )
    );
    let mode_chip = format!(
        "MODE {} ▾",
        super::state::provider_status(state).work_mode.label()
    );
    format!(
        "│ {}│ {} │",
        pad(&input, width.saturating_sub(18)),
        pad(&truncate(&mode_chip, 12), 12)
    )
}

fn queued_prompt_count_label(count: usize) -> String {
    if count == 1 {
        "1 prompt".to_string()
    } else {
        format!("{count} prompts")
    }
}

fn composer_input_spacer_row(width: usize) -> String {
    format!("│ {}│ {} │", pad("", width.saturating_sub(18)), pad("", 12))
}

pub(super) fn composer_cursor_position(
    state: &TuiState,
    terminal_width: u16,
    terminal_height: u16,
    bottom_bar_height: usize,
) -> (u16, u16) {
    let width = effective_layout_width(terminal_width);
    let height = usize::from(terminal_height);
    if should_render_welcome(state) {
        return welcome_cursor_position(state, width, height);
    }
    let cell = state
        .ui
        .input
        .cursor_cell(composer_content_width(state, width));
    let column = 4 + cell.column;
    let composer_height = composer_height(state, width);
    let row = height
        .saturating_sub(bottom_bar_height)
        .saturating_sub(composer_height)
        + 2
        + cell.row;
    (column as u16, row as u16)
}

#[allow(dead_code)]
pub(super) fn composer_anchor(
    state: &TuiState,
    width: usize,
    height: usize,
    bottom_bar_height: usize,
) -> ComposerAnchor {
    if should_render_welcome(state) {
        let box_width = welcome_box_width(width);
        let box_left = width.saturating_sub(box_width) / 2;
        return ComposerAnchor {
            left: box_left,
            input_row: welcome_composer_top(
                width,
                height,
                welcome_input_rows(state, box_width).len(),
            ) + 1,
            width: box_width,
        };
    }

    ComposerAnchor {
        left: 0,
        input_row: height
            .saturating_sub(bottom_bar_height)
            .saturating_sub(composer_height(state, width))
            + 2,
        width,
    }
}

fn composer_actions(state: &TuiState, width: usize) -> String {
    let left = format!(
        "MODE [{}]  PERM [{}]",
        super::state::provider_status(state).work_mode.label(),
        super::state::provider_status(state)
            .permission_level
            .label()
    );
    let right = if super::state::has_active_work(state) {
        super::i18n::text(state, "composer.actions.active")
    } else {
        super::i18n::text(state, "composer.actions.idle")
    };
    let left_width = char_width(&left);
    let right_width = char_width(&right);
    if left_width + right_width + 3 <= width {
        return format!(
            "{}{} {right}",
            left,
            " ".repeat(width.saturating_sub(left_width + right_width + 1))
        );
    }
    if left_width <= width {
        return left;
    }
    truncate(
        &format!(
            "MODE {}  PERMISSIONS {}",
            super::state::provider_status(state).work_mode.label(),
            super::state::provider_status(state)
                .permission_level
                .label()
        ),
        width,
    )
}

fn is_session_entry(entry: &super::state::TuiEntry) -> bool {
    match entry.kind() {
        "user" => !entry.body.trim_start().starts_with('/'),
        "command" => is_work_command_entry(&entry.body),
        "assistant" | "tool-call" | "tool-result" | "approval" => true,
        _ => false,
    }
}

fn is_work_command_entry(body: &str) -> bool {
    body.contains("Latest diff:")
        || body.contains("Test result:")
        || body.contains("Tool result:")
        || body.contains("Shell result:")
}

fn welcome_box_width(width: usize) -> usize {
    width
        .saturating_sub(10)
        .min(WELCOME_BOX_MAX_WIDTH)
        .max(WELCOME_BOX_MIN_WIDTH.min(width))
}

fn welcome_composer_top(width: usize, height: usize, input_rows: usize) -> usize {
    let target = if width >= 100 {
        height.saturating_mul(55) / 100
    } else {
        height.saturating_mul(45) / 100
    };
    target
        .min(height.saturating_sub(input_rows.saturating_add(7)))
        .max(6)
}

fn welcome_logo(width: usize) -> Vec<&'static str> {
    if width < 100 {
        return vec!["Viden"];
    }
    vec![
        "        /\\        ",
        "   ____/  \\____   ",
        "  / __  /\\  __ \\  ",
        " /_/  |/__\\|  \\_\\ ",
        " |  []  /\\  []  | ",
        " |_____/  \\_____| ",
        "    /_VIDEN_\\  ",
    ]
}

fn welcome_cursor_position(state: &TuiState, width: usize, height: usize) -> (u16, u16) {
    let box_width = welcome_box_width(width);
    let box_left = width.saturating_sub(box_width) / 2;
    let input_width = welcome_content_width(box_width);
    let cell = state.ui.input.cursor_cell(input_width);
    let column = box_left + 2 + cell.column;
    let row = welcome_composer_top(width, height, welcome_input_rows(state, box_width).len())
        + 1
        + cell.row;
    (column as u16, row as u16)
}

fn welcome_content_width(box_width: usize) -> usize {
    // Keep one physical cell available for the terminal cursor when the row
    // is otherwise full; exact-width input wraps instead of placing it past
    // the right edge.
    box_width.saturating_sub(3).max(1)
}

fn welcome_input_rows(state: &TuiState, box_width: usize) -> Vec<String> {
    if state.ui.input.is_empty() {
        return vec![format!(
            "▌  {}",
            pad(
                &truncate(
                    &super::i18n::text(state, "welcome.ask"),
                    box_width.saturating_sub(3)
                ),
                box_width.saturating_sub(3)
            )
        )];
    }

    state
        .ui
        .input
        .visible_rows(welcome_content_width(box_width))
        .into_iter()
        .map(|row| format!("▌ {}", pad(&row, box_width.saturating_sub(2))))
        .collect()
}

fn welcome_spacer_row(box_width: usize) -> String {
    format!("▌ {}", pad("", box_width - 2))
}

fn welcome_context_row(state: &TuiState, box_width: usize) -> String {
    let provider = provider_display_name(state);
    let content = format!(
        "▌ Viden - Operator · {} {}",
        state.runtime.snapshot.model_label, provider
    );
    truncate(&pad(&content, box_width), box_width)
}

fn welcome_hint_row(state: &TuiState, box_width: usize) -> String {
    let hint = super::i18n::text(state, "welcome.hints");
    format!(
        "{}{}",
        " ".repeat(box_width.saturating_sub(char_width(&hint))),
        hint
    )
}

fn welcome_status_row(state: &TuiState, width: usize) -> String {
    let left = truncate(
        &format!("{}:{}", state.runtime.snapshot.cwd.display(), "core"),
        width.saturating_mul(45) / 100,
    );
    let lane_count = state.runtime.lanes.len().to_string();
    let middle = super::i18n::translate(state, "welcome.lanes", &[("count", lane_count.as_str())]);
    let right = format!("v{}", env!("CARGO_PKG_VERSION"));
    let left_width = char_width(&left);
    let middle_width = char_width(&middle);
    let right_width = char_width(&right);
    if left_width + middle_width + right_width + 6 > width {
        return pad(&format!("{left}  {right}"), width);
    }
    let middle_col = width.saturating_sub(middle_width) / 2;
    let right_col = width.saturating_sub(right_width).saturating_sub(2);
    let first_gap = middle_col.saturating_sub(left_width);
    let second_gap = right_col.saturating_sub(middle_col + middle_width);
    pad(
        &format!(
            "{left}{}{middle}{}{right}",
            " ".repeat(first_gap),
            " ".repeat(second_gap)
        ),
        width,
    )
}

fn provider_display_name(state: &TuiState) -> String {
    state
        .ui
        .provider_catalog
        .iter()
        .find(|provider| provider.provider_id == state.runtime.snapshot.provider_family)
        .map(|provider| provider.display_name.clone())
        .unwrap_or_else(|| state.runtime.snapshot.provider_family.clone())
}

fn centered_line(width: usize, value: &str) -> String {
    let left = width.saturating_sub(char_width(value)) / 2;
    positioned_line(width, left, value)
}

fn positioned_line(width: usize, left: usize, value: &str) -> String {
    let mut line = " ".repeat(left.min(width));
    line.push_str(value);
    pad(&line, width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use viden_types::{PermissionLevel, QueuedInputView, ToolCallView, WorkMode};

    fn state_with_input(input: &str) -> TuiState {
        let mut state = TuiState::default();
        state.ui.session_id = "session_123".to_string();
        state.runtime.snapshot.provider_family = "fallback".to_string();
        state.runtime.snapshot.model_label = "test-local".to_string();
        state.ui.provider_catalog = crate::tui::state::ProviderOption::fixture();
        state.ui.theme_name = "aurora-cyan".to_string();
        state.ui.input = input.into();
        state.ui.entries = vec![super::super::state::TuiEntry {
            label: "assistant".to_string(),
            body: "hello".to_string(),
        }];
        state
    }

    fn mark_active(state: &mut TuiState) {
        state.runtime.active_tool_calls.push(ToolCallView {
            tool_call_id: "tool-1".to_string(),
            name: "test".to_string(),
            input_preview: "{}".to_string(),
            owner: None,
        });
    }

    #[test]
    fn cursor_position_tracks_visible_input_end() {
        assert_eq!(
            composer_cursor_position(&state_with_input(""), 120, 40, 1),
            (4, 35)
        );
        assert_eq!(
            composer_cursor_position(&state_with_input("abc"), 120, 40, 1),
            (7, 35)
        );
    }

    #[test]
    fn composer_uses_taller_input_area() {
        let mut frame = Frame::new(120, 40);
        render_composer(&mut frame, &state_with_input("hello"), 1);
        let rendered = frame.to_string();
        let lines = rendered.lines().collect::<Vec<_>>();

        assert_eq!(composer_height(&state_with_input("hello"), 120), 6);
        assert!(lines[33].contains("Viden >_"));
        assert!(lines[34].contains("│"));
        assert!(lines[35].contains("› hello"));
        assert!(lines[36].contains("│"));
        assert!(lines[37].contains("MODE"));
        assert!(lines[37].contains("PERM"));
    }

    #[test]
    fn composer_actions_use_runtime_mode_and_permission_state() {
        let mut state = state_with_input("hello");
        state.runtime.snapshot.work_mode = WorkMode::Plan;
        state.runtime.snapshot.permission_level = PermissionLevel::ReadOnly;

        let mut frame = Frame::new(120, 40);
        render_composer(&mut frame, &state, 1);
        let rendered = frame.to_string();

        assert!(rendered.contains("MODE [Plan]"));
        assert!(rendered.contains("PERM [Read Only]"));
        assert!(rendered.contains("MODE Plan"));
        assert!(!rendered.contains("MODE Code"));
    }

    #[test]
    fn composer_invites_next_prompt_during_active_turn() {
        let mut state = state_with_input("");
        mark_active(&mut state);

        let mut frame = Frame::new(120, 40);
        render_composer(&mut frame, &state, 1);
        let rendered = frame.to_string();

        assert!(rendered.contains("Type next prompt while Viden works"));

        state.runtime.queued_inputs.push(QueuedInputView {
            id: "queue-1".to_string(),
            content_preview: "follow up".to_string(),
            created_at: None,
            owner: None,
        });
        let mut frame = Frame::new(120, 40);
        render_composer(&mut frame, &state, 1);
        let rendered = frame.to_string();

        assert!(rendered.contains("1 prompt queued; type another prompt"));

        state.runtime.snapshot.ui_preferences.locale = viden_core::LocaleId::ZhCn;
        let mut frame = Frame::new(120, 40);
        render_composer(&mut frame, &state, 1);
        assert!(frame.to_string().contains("已排队 1 条"));
    }

    #[test]
    fn composer_actions_show_queue_and_cancel_during_active_turn() {
        let mut state = state_with_input("");
        mark_active(&mut state);

        let mut frame = Frame::new(120, 40);
        render_composer(&mut frame, &state, 1);
        let rendered = frame.to_string();

        assert!(rendered.contains("[^J Queue]"));
        assert!(rendered.contains("[^C Cancel]"));
        assert!(!rendered.contains("[^R Regenerate]"));
    }

    #[test]
    fn cursor_sits_on_middle_input_row_for_ime_candidate_placement() {
        assert_eq!(
            composer_cursor_position(&state_with_input("你好"), 120, 40, 1),
            (8, 35)
        );
    }

    #[test]
    fn multiline_composer_grows_to_eight_content_rows_then_scrolls_internally() {
        let state = state_with_input("一\n二\n三\n四\n五\n六\n七\n八\n九\n十");
        let mut frame = Frame::new(120, 40);

        render_composer(&mut frame, &state, 1);
        let rendered = frame.to_string();

        assert_eq!(composer_height(&state, 120), 13);
        assert!(!rendered.contains("› 一"));
        assert!(rendered.contains("› 三"));
        assert!(rendered.contains("  十"));
        assert_eq!(composer_cursor_position(&state, 120, 40, 1), (6, 35));
    }

    #[test]
    fn welcome_cursor_tracks_centered_input() {
        let mut state = state_with_input("hello");
        state.ui.entries = vec![super::super::state::TuiEntry {
            label: "system".to_string(),
            body: "Viden TUI ready. Enter submits.".to_string(),
        }];

        let (column, row) = composer_cursor_position(&state, 140, 40, 1);

        assert!(column > 20);
        assert_eq!(row, 23);
    }

    #[test]
    fn fresh_welcome_uses_eight_row_multiline_viewport_and_cursor_geometry() {
        let mut state = TuiState::default();
        state.ui.input = "一\n二\n三\n四\n五\n六\n七\n八\n九\n十".into();
        assert!(should_render_welcome(&state));
        let mut frame = Frame::new(120, 40);

        render_welcome(&mut frame, &state);
        let rendered = frame.to_string();
        let lines = rendered.lines().collect::<Vec<_>>();

        assert_eq!(lines.len(), 40);
        assert!(lines[23].contains("▌ 三"));
        assert!(lines[30].contains("▌ 十"));
        assert!(!rendered.contains("▌ 一"));
        assert_eq!(composer_cursor_position(&state, 120, 40, 1), (16, 30));
    }

    #[test]
    fn welcome_copy_follows_core_resolved_chinese_locale() {
        let mut state = TuiState::default();
        state.runtime.snapshot.ui_preferences.locale = viden_core::LocaleId::ZhCn;
        let mut frame = Frame::new(112, 40);

        render_welcome(&mut frame, &state);

        let rendered = frame.to_string();
        assert!(rendered.contains("可以问任何问题"));
        assert!(rendered.contains("/connect"));
        assert!(rendered.contains("条 lane"));
    }

    #[test]
    fn narrow_render_and_cursor_use_the_same_actual_width_and_viewport() {
        for width in [40_u16, 60, 79] {
            let mut welcome = TuiState::default();
            welcome.ui.input = "一\n二\n三\n四\n五\n六\n七\n八\n九\n终".into();
            let rendered = super::super::render::render_frame(&welcome, width, 40);
            let lines = rendered.lines().collect::<Vec<_>>();
            let (column, row) = composer_cursor_position(
                &welcome,
                width,
                40,
                super::super::statusbar::BOTTOM_BAR_HEIGHT,
            );

            assert!(
                lines
                    .iter()
                    .all(|line| char_width(line) == usize::from(width)),
                "welcome frame must use physical width {width}"
            );
            assert!(lines[usize::from(row)].contains("▌ 终"));
            assert!(usize::from(column) < usize::from(width));

            let normal = state_with_input("一\n二\n三\n四\n五\n六\n七\n八\n九\n终");
            let rendered = super::super::render::render_frame(&normal, width, 40);
            let lines = rendered.lines().collect::<Vec<_>>();
            let (column, row) = composer_cursor_position(
                &normal,
                width,
                40,
                super::super::statusbar::BOTTOM_BAR_HEIGHT,
            );

            assert!(
                lines
                    .iter()
                    .all(|line| char_width(line) == usize::from(width)),
                "session frame must use physical width {width}"
            );
            assert!(
                lines[usize::from(row)].contains("终"),
                "width {width}, cursor ({column}, {row}), line {:?}",
                lines[usize::from(row)]
            );
            assert!(usize::from(column) < usize::from(width));
        }

        let mut edge = TuiState::default();
        edge.ui.input = "x".repeat(38).into();
        let (column, row) =
            composer_cursor_position(&edge, 40, 40, super::super::statusbar::BOTTOM_BAR_HEIGHT);
        let rendered = super::super::render::render_frame(&edge, 40, 40);

        assert!(column < 40, "cursor must remain physically visible");
        assert!(rendered.lines().nth(usize::from(row)).is_some());
    }

    #[test]
    fn welcome_empty_placeholder_starts_after_cursor_cell() {
        let mut state = state_with_input("");
        state.ui.entries = vec![super::super::state::TuiEntry {
            label: "system".to_string(),
            body: "Viden TUI ready. Enter submits.".to_string(),
        }];
        let mut frame = Frame::new(140, 40);
        render_welcome(&mut frame, &state);
        let rendered = frame.to_string();
        let (column, row) = composer_cursor_position(&state, 140, 40, 1);
        let line = rendered.lines().nth(row as usize).expect("cursor row");

        assert_eq!(line.chars().nth(column as usize), Some(' '));
        assert_eq!(line.chars().nth(column as usize + 1), Some('A'));
    }

    #[test]
    fn welcome_renders_mecha_logo_on_wide_terminals() {
        let mut state = state_with_input("");
        state.ui.entries = vec![super::super::state::TuiEntry {
            label: "system".to_string(),
            body: "Viden TUI ready. Enter submits.".to_string(),
        }];
        let mut frame = Frame::new(140, 40);
        render_welcome(&mut frame, &state);
        let rendered = frame.to_string();

        assert!(rendered.contains("/_VIDEN_\\"));
        assert!(rendered.contains("[]"));
    }

    #[test]
    fn welcome_survives_slash_setup_entries_until_user_starts_session() {
        let mut state = state_with_input("");
        state.ui.entries = vec![
            super::super::state::TuiEntry {
                label: "system".to_string(),
                body: "Viden TUI ready. Enter submits.".to_string(),
            },
            super::super::state::TuiEntry {
                label: "user".to_string(),
                body: "/connect".to_string(),
            },
            super::super::state::TuiEntry {
                label: "settings".to_string(),
                body: "Provider switched to deepseek.".to_string(),
            },
            super::super::state::TuiEntry {
                label: "setup".to_string(),
                body: "Provider setup completed.".to_string(),
            },
        ];

        assert!(should_render_welcome(&state));

        state.ui.entries.push(super::super::state::TuiEntry {
            label: "user".to_string(),
            body: "fix broken tests".to_string(),
        });

        assert!(!should_render_welcome(&state));
    }
}
