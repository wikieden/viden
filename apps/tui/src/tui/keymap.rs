use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use viden_types::RuntimeOwner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InputMode {
    Normal,
    Insert,
    Overlay,
}

impl InputMode {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Overlay => "OVERLAY",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OverlayKind {
    GlobalJump,
    Lane,
    Session,
    NewSession,
    CommandPalette,
    Board,
    Decisions,
    /// Merge-gate / review / conflict decision surface. Reached only by picking
    /// a row in the Decision Center, so it has no global chord of its own.
    SupervisionDecision,
    /// Read-only structured conflict content
    /// (`runtime.conflict_content`, GUI-CORE-015). Reached from the
    /// supervision overlay's inspect row, so it has no global chord of its
    /// own; it shows three sides and resolves nothing.
    ConflictContent,
    /// Read-only audit timeline. Reached from the supervision overlay's audit
    /// row or the Decision Center footer pick, so it has no global chord of its
    /// own; it browses and never decides.
    AuditTimeline,
    ContextHelp,
    ExitConfirm,
    Approval,
    InteractionPanel,
    ComposerCommands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct InputFocus {
    pub(super) overlay: Option<OverlayKind>,
    pub(super) selection_active: bool,
    pub(super) idle_ctrl_c_armed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct RuntimeFacts {
    pub(super) current_work_owner: Option<RuntimeOwner>,
    pub(super) has_active_work: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum InputIntent {
    None,
    EnterInsert,
    LeaveInsert,
    OpenOverlay(OverlayKind),
    CloseOverlay,
    ClearSelection,
    ArmExitConfirmation,
    CancelCurrentWork { owner: RuntimeOwner },
    CycleAgentFocus,
    OpenNativeLane,
    Exit,
    InsertChar(char),
    InsertNewline,
    Backspace,
    MoveCursorLeft,
    MoveCursorRight,
    MoveCursorUp,
    MoveCursorDown,
    Submit,
    MoveSelection(i8),
    CompleteSelection,
    CompleteOrSubmit,
    Scroll(isize),
    ScrollToStart,
    ScrollToEnd,
}

/// Reduces terminal input into a UI intent without performing Core or local
/// effects. Global chords are resolved before mode-owned keys so their
/// behavior remains stable in Normal, Insert, and Overlay modes.
pub(super) fn reduce_input(
    mode: InputMode,
    focus: InputFocus,
    key: KeyEvent,
    runtime_facts: RuntimeFacts,
) -> InputIntent {
    let has_current_work = runtime_facts.current_work_owner.is_some();
    let has_active_work = runtime_facts.has_active_work || has_current_work;
    if is_control_char(key, 'c') {
        return match runtime_facts.current_work_owner {
            Some(owner) => InputIntent::CancelCurrentWork { owner },
            None if has_active_work => InputIntent::None,
            None if focus.idle_ctrl_c_armed => InputIntent::OpenOverlay(OverlayKind::ExitConfirm),
            None => InputIntent::ArmExitConfirmation,
        };
    }

    if let Some(kind) = global_overlay(key) {
        return InputIntent::OpenOverlay(kind);
    }
    if key.code == KeyCode::Char('?')
        && (key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT)
    {
        return InputIntent::OpenOverlay(OverlayKind::ContextHelp);
    }

    if key.code == KeyCode::Esc {
        if focus.overlay.is_some() {
            return InputIntent::CloseOverlay;
        }
        if focus.selection_active {
            return InputIntent::ClearSelection;
        }
        return match mode {
            InputMode::Insert => InputIntent::LeaveInsert,
            InputMode::Normal if has_active_work => runtime_facts
                .current_work_owner
                .map_or(InputIntent::None, |owner| InputIntent::CancelCurrentWork {
                    owner,
                }),
            InputMode::Normal => InputIntent::Exit,
            InputMode::Overlay => InputIntent::None,
        };
    }

    if key.code == KeyCode::Tab && mode != InputMode::Overlay {
        return InputIntent::CycleAgentFocus;
    }
    if key.code == KeyCode::Enter
        && key
            .modifiers
            .intersects(KeyModifiers::SHIFT | KeyModifiers::ALT)
    {
        return match focus.overlay {
            Some(OverlayKind::Approval) => InputIntent::InsertNewline,
            Some(_) => InputIntent::None,
            None if mode == InputMode::Insert => InputIntent::InsertNewline,
            None => InputIntent::None,
        };
    }
    match key.code {
        KeyCode::PageUp => return InputIntent::Scroll(12),
        KeyCode::PageDown => return InputIntent::Scroll(-12),
        KeyCode::Home if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return InputIntent::ScrollToStart;
        }
        KeyCode::End if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return InputIntent::ScrollToEnd;
        }
        _ => {}
    }

    match (mode, key.code) {
        (InputMode::Normal, KeyCode::Char('i')) => InputIntent::EnterInsert,
        (InputMode::Normal, KeyCode::Char('n')) => InputIntent::OpenNativeLane,
        (InputMode::Insert, KeyCode::Enter) | (InputMode::Insert, KeyCode::Char('j'))
            if key.modifiers.contains(KeyModifiers::CONTROL) || key.code == KeyCode::Enter =>
        {
            InputIntent::Submit
        }
        (InputMode::Insert, KeyCode::Backspace) => InputIntent::Backspace,
        (InputMode::Insert, KeyCode::Left) => InputIntent::MoveCursorLeft,
        (InputMode::Insert, KeyCode::Right) => InputIntent::MoveCursorRight,
        (InputMode::Insert, KeyCode::Up) => InputIntent::MoveCursorUp,
        (InputMode::Insert, KeyCode::Down) => InputIntent::MoveCursorDown,
        (InputMode::Insert, KeyCode::Char(value))
            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
        {
            InputIntent::InsertChar(value)
        }
        (InputMode::Overlay, KeyCode::Up | KeyCode::BackTab | KeyCode::Left) => {
            InputIntent::MoveSelection(-1)
        }
        (InputMode::Overlay, KeyCode::Down | KeyCode::Right) => InputIntent::MoveSelection(1),
        (InputMode::Overlay, KeyCode::Char('k'))
            if focus.overlay == Some(OverlayKind::GlobalJump) =>
        {
            InputIntent::MoveSelection(-1)
        }
        (InputMode::Overlay, KeyCode::Char('j'))
            if focus.overlay == Some(OverlayKind::GlobalJump) =>
        {
            InputIntent::MoveSelection(1)
        }
        (InputMode::Overlay, KeyCode::Tab) => InputIntent::CompleteSelection,
        (InputMode::Overlay, KeyCode::Enter) => InputIntent::CompleteOrSubmit,
        (InputMode::Overlay, KeyCode::Backspace) => InputIntent::Backspace,
        (InputMode::Overlay, KeyCode::Char(value))
            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
        {
            InputIntent::InsertChar(value)
        }
        _ => InputIntent::None,
    }
}

fn is_control_char(key: KeyEvent, value: char) -> bool {
    key.code == KeyCode::Char(value) && key.modifiers.contains(KeyModifiers::CONTROL)
}

fn global_overlay(key: KeyEvent) -> Option<OverlayKind> {
    if !key.modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }
    match key.code {
        KeyCode::Char('p') => Some(OverlayKind::GlobalJump),
        KeyCode::Char('l') => Some(OverlayKind::Lane),
        KeyCode::Char('s') => Some(OverlayKind::Session),
        KeyCode::Char('t') => Some(OverlayKind::NewSession),
        KeyCode::Char('k') => Some(OverlayKind::CommandPalette),
        KeyCode::Char('b') => Some(OverlayKind::Board),
        KeyCode::Char('g') => Some(OverlayKind::Decisions),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn control(value: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(value), KeyModifiers::CONTROL)
    }

    fn reduce(mode: InputMode, key: KeyEvent) -> InputIntent {
        reduce_input(mode, InputFocus::default(), key, RuntimeFacts::default())
    }

    #[test]
    fn i_enters_insert_only_from_normal_and_printable_text_belongs_to_insert() {
        assert_eq!(
            reduce(InputMode::Normal, key(KeyCode::Char('i'))),
            InputIntent::EnterInsert
        );
        assert_eq!(
            reduce(InputMode::Insert, key(KeyCode::Char('i'))),
            InputIntent::InsertChar('i')
        );
        assert_eq!(
            reduce(InputMode::Normal, key(KeyCode::Char('x'))),
            InputIntent::None
        );
        assert_eq!(
            reduce(InputMode::Insert, key(KeyCode::Char('x'))),
            InputIntent::InsertChar('x')
        );
    }

    #[test]
    fn n_opens_native_lane_task_only_from_normal_mode() {
        assert_eq!(
            reduce(InputMode::Normal, key(KeyCode::Char('n'))),
            InputIntent::OpenNativeLane
        );
        assert_eq!(
            reduce(InputMode::Insert, key(KeyCode::Char('n'))),
            InputIntent::InsertChar('n')
        );
    }

    #[test]
    fn every_global_chord_and_context_help_pierces_all_modes() {
        let cases = [
            ('p', OverlayKind::GlobalJump),
            ('l', OverlayKind::Lane),
            ('s', OverlayKind::Session),
            ('t', OverlayKind::NewSession),
            ('k', OverlayKind::CommandPalette),
            ('b', OverlayKind::Board),
            ('g', OverlayKind::Decisions),
        ];
        for mode in [InputMode::Normal, InputMode::Insert, InputMode::Overlay] {
            for (chord, kind) in cases {
                assert_eq!(
                    reduce(mode, control(chord)),
                    InputIntent::OpenOverlay(kind),
                    "Ctrl-{chord} must be global in {mode:?}"
                );
            }
            assert_eq!(
                reduce(mode, key(KeyCode::Char('?'))),
                InputIntent::OpenOverlay(OverlayKind::ContextHelp)
            );
            assert_eq!(
                reduce_input(
                    mode,
                    InputFocus::default(),
                    KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT),
                    RuntimeFacts::default(),
                ),
                InputIntent::OpenOverlay(OverlayKind::ContextHelp),
                "terminals may report ? with the Shift modifier"
            );
        }
    }

    #[test]
    fn escape_unwinds_overlay_then_selection_then_insert() {
        let facts = RuntimeFacts::default();
        assert_eq!(
            reduce_input(
                InputMode::Overlay,
                InputFocus {
                    overlay: Some(OverlayKind::Lane),
                    selection_active: true,
                    ..InputFocus::default()
                },
                key(KeyCode::Esc),
                facts.clone(),
            ),
            InputIntent::CloseOverlay
        );
        assert_eq!(
            reduce_input(
                InputMode::Insert,
                InputFocus {
                    selection_active: true,
                    ..InputFocus::default()
                },
                key(KeyCode::Esc),
                facts.clone(),
            ),
            InputIntent::ClearSelection
        );
        assert_eq!(
            reduce_input(
                InputMode::Insert,
                InputFocus::default(),
                key(KeyCode::Esc),
                facts,
            ),
            InputIntent::LeaveInsert
        );
        assert_eq!(
            reduce_input(
                InputMode::Normal,
                InputFocus::default(),
                key(KeyCode::Esc),
                RuntimeFacts {
                    current_work_owner: Some(RuntimeOwner::default()),
                    has_active_work: true,
                },
            ),
            InputIntent::CancelCurrentWork {
                owner: RuntimeOwner::default(),
            },
            "Normal Esc must cancel only through the exact current owner"
        );
    }

    #[test]
    fn ctrl_c_cancels_only_the_current_owner_and_idle_double_press_opens_confirm() {
        let owner = RuntimeOwner {
            workspace_id: "workspace".to_string(),
            project_id: "project".to_string(),
            lane_id: Some("lane".to_string()),
            session_id: Some("session".to_string()),
            task_id: Some("task".to_string()),
            turn_id: Some("turn".to_string()),
        };
        for mode in [InputMode::Normal, InputMode::Insert, InputMode::Overlay] {
            assert_eq!(
                reduce_input(
                    mode,
                    InputFocus::default(),
                    control('c'),
                    RuntimeFacts {
                        current_work_owner: Some(owner.clone()),
                        has_active_work: true,
                    },
                ),
                InputIntent::CancelCurrentWork {
                    owner: owner.clone(),
                }
            );
            assert_eq!(reduce(mode, control('c')), InputIntent::ArmExitConfirmation);
            assert_eq!(
                reduce_input(
                    mode,
                    InputFocus {
                        idle_ctrl_c_armed: true,
                        ..InputFocus::default()
                    },
                    control('c'),
                    RuntimeFacts::default(),
                ),
                InputIntent::OpenOverlay(OverlayKind::ExitConfirm)
            );
        }
    }

    #[test]
    fn ownerless_active_work_blocks_escape_without_fabricating_a_cancel_owner() {
        let facts = RuntimeFacts {
            current_work_owner: None,
            has_active_work: true,
        };

        assert_eq!(
            reduce_input(
                InputMode::Normal,
                InputFocus::default(),
                key(KeyCode::Esc),
                facts.clone(),
            ),
            InputIntent::None
        );
        assert_eq!(
            reduce_input(
                InputMode::Normal,
                InputFocus::default(),
                control('c'),
                facts,
            ),
            InputIntent::None
        );
    }

    #[test]
    fn overlay_owns_arrows_filter_and_enter() {
        assert_eq!(
            reduce(InputMode::Overlay, key(KeyCode::Down)),
            InputIntent::MoveSelection(1)
        );
        assert_eq!(
            reduce(InputMode::Overlay, key(KeyCode::Char('f'))),
            InputIntent::InsertChar('f')
        );
        assert_eq!(
            reduce(InputMode::Overlay, key(KeyCode::Enter)),
            InputIntent::CompleteOrSubmit
        );
    }

    #[test]
    fn global_jump_adds_j_and_k_without_stealing_other_overlay_filters() {
        let jump_focus = InputFocus {
            overlay: Some(OverlayKind::GlobalJump),
            ..InputFocus::default()
        };
        assert_eq!(
            reduce_input(
                InputMode::Overlay,
                jump_focus,
                key(KeyCode::Char('j')),
                RuntimeFacts::default()
            ),
            InputIntent::MoveSelection(1)
        );
        assert_eq!(
            reduce_input(
                InputMode::Overlay,
                jump_focus,
                key(KeyCode::Char('k')),
                RuntimeFacts::default()
            ),
            InputIntent::MoveSelection(-1)
        );
        assert_eq!(
            reduce_input(
                InputMode::Overlay,
                InputFocus {
                    overlay: Some(OverlayKind::Lane),
                    ..InputFocus::default()
                },
                key(KeyCode::Char('j')),
                RuntimeFacts::default(),
            ),
            InputIntent::InsertChar('j')
        );
    }

    #[test]
    fn insert_mode_owns_cursor_motion_and_explicit_newline_chords() {
        for (code, expected) in [
            (KeyCode::Left, InputIntent::MoveCursorLeft),
            (KeyCode::Right, InputIntent::MoveCursorRight),
            (KeyCode::Up, InputIntent::MoveCursorUp),
            (KeyCode::Down, InputIntent::MoveCursorDown),
        ] {
            assert_eq!(reduce(InputMode::Insert, key(code)), expected);
        }
        for modifiers in [KeyModifiers::SHIFT, KeyModifiers::ALT] {
            assert_eq!(
                reduce(InputMode::Insert, KeyEvent::new(KeyCode::Enter, modifiers)),
                InputIntent::InsertNewline
            );
        }
        assert_eq!(
            reduce(InputMode::Insert, key(KeyCode::Enter)),
            InputIntent::Submit
        );
    }
}
