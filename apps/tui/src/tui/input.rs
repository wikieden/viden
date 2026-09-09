use crossterm::event::{KeyCode, KeyEvent};

use super::command_palette::is_command_palette_visible;
use super::keymap::{InputFocus, InputMode, OverlayKind};
use super::modal::{
    ApprovalAction, approval_is_expired, focused_approval_action, focused_approval_request,
    move_approval_focus, set_approval_focus_for_action,
};
use super::state::{TuiEntry, TuiState};
use viden_core::{ApprovalDecision, ApprovalResponse, ApprovalScope};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ApprovalKeyEffect {
    None,
    Redraw,
    ResolveScoped(ApprovalResponse),
}

pub(super) fn effective_input_mode(state: &TuiState) -> InputMode {
    if active_overlay_kind(state).is_some() {
        InputMode::Overlay
    } else {
        state.ui.input_mode
    }
}

pub(super) fn input_focus(state: &TuiState) -> InputFocus {
    InputFocus {
        overlay: active_overlay_kind(state),
        selection_active: state.ui.focused_lane.is_some(),
        idle_ctrl_c_armed: state.ui.idle_ctrl_c_armed,
    }
}

fn active_overlay_kind(state: &TuiState) -> Option<OverlayKind> {
    if let Some(overlay) = state.ui.overlay.as_ref() {
        Some(overlay.kind)
    } else if state.ui.interaction_panel.is_some() {
        Some(OverlayKind::InteractionPanel)
    } else if is_command_palette_visible(state) {
        Some(OverlayKind::ComposerCommands)
    } else {
        None
    }
}

pub(super) fn close_focus_on_escape(key: KeyEvent, state: &mut TuiState) -> bool {
    if key.code != KeyCode::Esc || state.ui.focused_lane.is_none() {
        return false;
    }
    state.ui.focused_lane = None;
    state.ui.entries.push(TuiEntry {
        label: "system".to_string(),
        body: "Closed lane detail focus.".to_string(),
    });
    true
}

pub(super) fn apply_approval_key(key: KeyEvent, state: &mut TuiState) -> ApprovalKeyEffect {
    if focused_approval_request(state).is_some_and(approval_is_expired)
        && matches!(
            key.code,
            KeyCode::Char('y' | 'Y' | 'n' | 'N' | '1' | '2' | '3' | '4') | KeyCode::Enter
        )
    {
        // Expiry is a Core-owned fact transition. The TUI leaves the request
        // visible and inert until ApprovalResolved arrives.
        return ApprovalKeyEffect::None;
    }
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            resolve_allowed_scope(state, |scope| matches!(scope, ApprovalScope::Once))
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            ApprovalKeyEffect::ResolveScoped(ApprovalResponse::deny(None))
        }
        KeyCode::Char('1') => {
            resolve_allowed_scope(state, |scope| matches!(scope, ApprovalScope::Once))
        }
        KeyCode::Char('2') => resolve_allowed_scope(state, |scope| {
            matches!(scope, ApprovalScope::Session { .. })
        }),
        KeyCode::Char('3') => resolve_allowed_scope(state, |scope| {
            matches!(scope, ApprovalScope::RepoAllowlist { .. })
        }),
        KeyCode::Char('4') => ApprovalKeyEffect::ResolveScoped(ApprovalResponse::deny(None)),
        KeyCode::Char(' ') => {
            if focused_approval_action(state) != ApprovalAction::ToggleApplyAll {
                return ApprovalKeyEffect::None;
            }
            state.ui.approval_apply_all = !state.ui.approval_apply_all;
            ApprovalKeyEffect::Redraw
        }
        KeyCode::Tab | KeyCode::Right | KeyCode::Down => {
            move_approval_focus(state, 1);
            ApprovalKeyEffect::Redraw
        }
        KeyCode::BackTab | KeyCode::Left | KeyCode::Up => {
            move_approval_focus(state, -1);
            ApprovalKeyEffect::Redraw
        }
        KeyCode::Enter => apply_approval_action(focused_approval_action(state), state),
        KeyCode::Char('d') | KeyCode::Char('D') => {
            set_approval_focus_for_action(state, ApprovalAction::Diff);
            ApprovalKeyEffect::Redraw
        }
        _ => ApprovalKeyEffect::None,
    }
}

fn resolve_allowed_scope(
    state: &TuiState,
    predicate: impl Fn(&ApprovalScope) -> bool,
) -> ApprovalKeyEffect {
    let Some(scope) = focused_approval_request(state)
        .and_then(|approval| {
            approval
                .allowed_scopes
                .iter()
                .find(|scope| predicate(scope))
        })
        .cloned()
    else {
        return ApprovalKeyEffect::None;
    };
    ApprovalKeyEffect::ResolveScoped(ApprovalResponse {
        decision: ApprovalDecision::Allow { scope },
        feedback: None,
    })
}

pub(super) fn apply_approval_action(
    action: ApprovalAction,
    state: &mut TuiState,
) -> ApprovalKeyEffect {
    match action {
        ApprovalAction::ToggleApplyAll => {
            state.ui.approval_apply_all = !state.ui.approval_apply_all;
            ApprovalKeyEffect::Redraw
        }
        ApprovalAction::Deny => ApprovalKeyEffect::ResolveScoped(ApprovalResponse::deny(None)),
        ApprovalAction::Approve => {
            resolve_allowed_scope(state, |scope| matches!(scope, ApprovalScope::Once))
        }
        ApprovalAction::Diff => ApprovalKeyEffect::Redraw,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;
    use viden_core::{
        ApprovalDefaultAction, ApprovalRequestView, ApprovalRisk, ApprovalScope, ApprovalTarget,
    };

    use super::super::modal::DEFAULT_APPROVAL_FOCUS;
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn state_with_focus() -> TuiState {
        let mut state = TuiState::default();
        state.ui.session_id = "session_123".to_string();
        state.ui.provider_catalog = crate::tui::state::ProviderOption::fixture();
        state.ui.theme_name = "aurora-cyan".to_string();
        state.ui.focused_lane = Some("L1".to_string());
        state
    }

    fn state_with_four_choice_approval(expires_at: u64) -> TuiState {
        let mut state = state_with_focus();
        state.runtime.pending_approvals.push(ApprovalRequestView {
            id: "approval-four".to_string(),
            tool_name: "shell".to_string(),
            title: "Dangerous command".to_string(),
            message: "choose scope".to_string(),
            input_preview: "git push".to_string(),
            is_mutating: true,
            reason: None,
            owner: Default::default(),
            risk: ApprovalRisk::High,
            target: ApprovalTarget {
                kind: "command".to_string(),
                display: "git push".to_string(),
                canonical_ref: None,
            },
            allowed_scopes: vec![
                ApprovalScope::Once,
                ApprovalScope::Session {
                    session_id: "session-four".to_string(),
                },
                ApprovalScope::RepoAllowlist {
                    paths: vec!["refs/heads/main".to_string()],
                },
            ],
            policy_reason_key: "approval.fixture".to_string(),
            policy_reason_args: Default::default(),
            expires_at,
            default_action: ApprovalDefaultAction::Deny,
            audit_id: "audit-four".to_string(),
            decision_context: None,
        });
        let mut overlay = super::super::state::OverlayState::new(OverlayKind::Approval);
        overlay.selected_id = Some("approval-four".to_string());
        state.ui.overlay = Some(overlay);
        state
    }

    #[test]
    fn escape_closes_focus_before_exit() {
        let mut state = state_with_focus();

        assert!(close_focus_on_escape(key(KeyCode::Esc), &mut state));

        assert_eq!(state.ui.focused_lane, None);
        assert!(
            state.ui.entries[0]
                .body
                .contains("Closed lane detail focus")
        );
    }

    #[test]
    fn approval_digits_return_the_exact_typed_core_scope() {
        let mut state = state_with_four_choice_approval(u64::MAX);

        let once = format!(
            "{:?}",
            apply_approval_key(key(KeyCode::Char('1')), &mut state)
        );
        let session = format!(
            "{:?}",
            apply_approval_key(key(KeyCode::Char('2')), &mut state)
        );
        let repo = format!(
            "{:?}",
            apply_approval_key(key(KeyCode::Char('3')), &mut state)
        );
        let deny = format!(
            "{:?}",
            apply_approval_key(key(KeyCode::Char('4')), &mut state)
        );

        assert!(once.contains("Once"), "{once}");
        assert!(
            session.contains("Session") && session.contains("session-four"),
            "{session}"
        );
        assert!(
            repo.contains("RepoAllowlist") && repo.contains("refs/heads/main"),
            "{repo}"
        );
        assert!(deny.contains("Deny"), "{deny}");
    }

    #[test]
    fn expired_approval_stays_pending_until_core_resolves_it() {
        let mut state = state_with_four_choice_approval(1);

        assert_eq!(
            apply_approval_key(key(KeyCode::Char('y')), &mut state),
            ApprovalKeyEffect::None
        );
        assert_eq!(state.runtime.pending_approvals.len(), 1);
    }

    #[test]
    fn allow_once_shortcuts_cannot_bypass_the_typed_allowed_scopes() {
        let mut state = state_with_four_choice_approval(u64::MAX);
        state.ui.approval_focus = DEFAULT_APPROVAL_FOCUS;
        state.runtime.pending_approvals[0]
            .allowed_scopes
            .retain(|scope| !matches!(scope, ApprovalScope::Once));

        assert_eq!(
            apply_approval_key(key(KeyCode::Char('y')), &mut state),
            ApprovalKeyEffect::None
        );
        assert_eq!(
            apply_approval_key(key(KeyCode::Enter), &mut state),
            ApprovalKeyEffect::None
        );
    }

    #[test]
    fn escape_without_focus_leaves_selection_handler_idle() {
        let mut state = state_with_focus();
        state.ui.focused_lane = None;

        assert!(!close_focus_on_escape(key(KeyCode::Esc), &mut state));
    }

    #[test]
    fn approval_space_toggles_apply_all_only_when_checkbox_is_focused() {
        let mut state = state_with_focus();
        state.ui.approval_focus = DEFAULT_APPROVAL_FOCUS;

        assert_eq!(
            apply_approval_key(key(KeyCode::Char(' ')), &mut state),
            ApprovalKeyEffect::None
        );
        assert!(!state.ui.approval_apply_all);

        set_approval_focus_for_action(&mut state, ApprovalAction::ToggleApplyAll);

        assert_eq!(
            apply_approval_key(key(KeyCode::Char(' ')), &mut state),
            ApprovalKeyEffect::Redraw
        );
        assert!(state.ui.approval_apply_all);
    }

    #[test]
    fn approval_enter_activates_default_approve_focus() {
        let mut state = state_with_four_choice_approval(u64::MAX);
        state.ui.approval_focus = DEFAULT_APPROVAL_FOCUS;

        assert!(matches!(
            apply_approval_key(key(KeyCode::Enter), &mut state),
            ApprovalKeyEffect::ResolveScoped(ApprovalResponse {
                decision: ApprovalDecision::Allow {
                    scope: ApprovalScope::Once
                },
                ..
            })
        ));
    }

    #[test]
    fn approval_keyboard_focus_reaches_deny_diff_and_approve() {
        let mut state = state_with_four_choice_approval(u64::MAX);
        state.ui.approval_focus = DEFAULT_APPROVAL_FOCUS;

        assert_eq!(
            focused_approval_action(&state),
            ApprovalAction::Approve,
            "approval should default to the low-friction approve action"
        );

        assert_eq!(
            apply_approval_key(key(KeyCode::Left), &mut state),
            ApprovalKeyEffect::Redraw
        );
        assert_eq!(focused_approval_action(&state), ApprovalAction::Diff);

        assert_eq!(
            apply_approval_key(key(KeyCode::Left), &mut state),
            ApprovalKeyEffect::Redraw
        );
        assert_eq!(focused_approval_action(&state), ApprovalAction::Deny);

        assert!(matches!(
            apply_approval_key(key(KeyCode::Enter), &mut state),
            ApprovalKeyEffect::ResolveScoped(ApprovalResponse {
                decision: ApprovalDecision::Deny,
                ..
            })
        ));

        set_approval_focus_for_action(&mut state, ApprovalAction::Approve);
        assert!(matches!(
            apply_approval_key(key(KeyCode::Enter), &mut state),
            ApprovalKeyEffect::ResolveScoped(ApprovalResponse {
                decision: ApprovalDecision::Allow {
                    scope: ApprovalScope::Once
                },
                ..
            })
        ));
    }

    #[test]
    fn approval_shortcuts_resolve_without_focus_hopping() {
        let mut state = state_with_four_choice_approval(u64::MAX);
        set_approval_focus_for_action(&mut state, ApprovalAction::Deny);

        assert!(matches!(
            apply_approval_key(key(KeyCode::Char('y')), &mut state),
            ApprovalKeyEffect::ResolveScoped(ApprovalResponse {
                decision: ApprovalDecision::Allow {
                    scope: ApprovalScope::Once
                },
                ..
            })
        ));
        assert_eq!(focused_approval_action(&state), ApprovalAction::Deny);

        assert!(matches!(
            apply_approval_key(key(KeyCode::Char('n')), &mut state),
            ApprovalKeyEffect::ResolveScoped(ApprovalResponse {
                decision: ApprovalDecision::Deny,
                ..
            })
        ));
        assert_eq!(
            apply_approval_key(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                &mut state
            ),
            ApprovalKeyEffect::None,
            "Ctrl-C belongs to current-work cancellation and must never deny approval"
        );
    }
}
