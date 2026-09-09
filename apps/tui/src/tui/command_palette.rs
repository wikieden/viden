use crossterm::event::{KeyCode, KeyEvent};

use super::{canvas::Frame, panel::panel, state::TuiState, text::truncate};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CommandSuggestion {
    pub(super) command: String,
    pub(super) summary: String,
}

pub(super) struct CommandDefinition {
    pub(super) command: &'static str,
    pub(super) summary: &'static str,
    pub(super) keywords: &'static str,
}

const COMMANDS: &[CommandDefinition] = &[
    CommandDefinition {
        command: "/help",
        summary: "Show commands",
        keywords: "help shortcuts",
    },
    CommandDefinition {
        command: "/setup",
        summary: "Open first-run setup",
        keywords: "setup onboarding",
    },
    CommandDefinition {
        command: "/lanes",
        summary: "Open the Core lane board",
        keywords: "lane board",
    },
    CommandDefinition {
        command: "/acp",
        summary: "Choose an ACP agent or session",
        keywords: "agent codex claude kiro session",
    },
    CommandDefinition {
        command: "/git",
        summary: "Run a Core operator source-control action",
        keywords: "git source control stage commit push fetch",
    },
    CommandDefinition {
        command: "/decisions",
        summary: "Open approvals and gates",
        keywords: "approval gate ask",
    },
    CommandDefinition {
        command: "/gallery",
        summary: "Open Core evidence gallery",
        keywords: "evidence gallery",
    },
    CommandDefinition {
        command: "/connect",
        summary: "Configure a Core provider",
        keywords: "provider connect",
    },
    CommandDefinition {
        command: "/models",
        summary: "Select an available Core model",
        keywords: "model",
    },
    CommandDefinition {
        command: "/settings",
        summary: "Open stable UI preferences",
        keywords: "settings locale appearance density motion color",
    },
    CommandDefinition {
        command: "/mode plan",
        summary: "Request Plan work mode",
        keywords: "plan",
    },
    CommandDefinition {
        command: "/mode build",
        summary: "Request Build work mode",
        keywords: "build",
    },
    CommandDefinition {
        command: "/permissions ask",
        summary: "Request ask permission level",
        keywords: "permission approval",
    },
    CommandDefinition {
        command: "/permissions read-only",
        summary: "Request read-only permission level",
        keywords: "permission readonly",
    },
    CommandDefinition {
        command: "/status",
        summary: "Inspect structured runtime status",
        keywords: "status runtime",
    },
];

pub(super) fn command_registry() -> &'static [CommandDefinition] {
    COMMANDS
}

pub(super) fn is_command_palette_query(input: &str) -> bool {
    input.starts_with('/')
}

pub(super) fn is_command_palette_visible(state: &TuiState) -> bool {
    is_command_palette_query(state.ui.input.as_str())
        && state.ui.command_palette_hidden_for.as_deref() != Some(state.ui.input.as_str())
        && !suggestions(state).is_empty()
}

pub(super) fn selected_command(state: &TuiState) -> Option<CommandSuggestion> {
    suggestions(state).get(state.ui.command_selection).cloned()
}

pub(super) fn move_selection(state: &mut TuiState, delta: i8) -> bool {
    let len = suggestions(state).len();
    if len == 0 {
        return false;
    }
    state.ui.command_selection = if delta < 0 {
        state.ui.command_selection.saturating_sub(1)
    } else {
        (state.ui.command_selection + 1).min(len - 1)
    };
    true
}

pub(super) fn reset_for_input_change(state: &mut TuiState) {
    state.ui.command_selection = 0;
    state.ui.command_palette_hidden_for = None;
}

pub(super) fn close_on_escape(key: KeyEvent, state: &mut TuiState) -> bool {
    if key.code != KeyCode::Esc || !is_command_palette_visible(state) {
        return false;
    }
    state.ui.command_palette_hidden_for = Some(state.ui.input.as_str().to_string());
    true
}

pub(super) fn complete_selected(state: &mut TuiState) -> bool {
    let Some(selected) = selected_command(state) else {
        return false;
    };
    state.ui.input.replace(selected.command);
    reset_for_input_change(state);
    true
}

pub(super) fn select_suggestion_at(state: &mut TuiState, index: usize) -> bool {
    if index >= suggestions(state).len() {
        return false;
    }
    state.ui.command_selection = index;
    true
}

pub(super) fn command_suggestion_index_at(
    state: &TuiState,
    _terminal_width: u16,
    _terminal_height: u16,
    _bottom_bar_height: usize,
    _column: u16,
    row: u16,
) -> Option<usize> {
    let index = usize::from(row.saturating_sub(3));
    (index < suggestions(state).len()).then_some(index)
}

pub(super) fn should_complete_on_enter(state: &TuiState) -> bool {
    if is_exact_command(state) {
        return false;
    }
    selected_command(state).is_some_and(|selected| selected.command != state.ui.input.as_str())
}

fn is_exact_command(state: &TuiState) -> bool {
    let input = state.ui.input.as_str();
    matches!(
        input,
        "/help"
            | "/setup"
            | "/lanes"
            | "/decisions"
            | "/gallery"
            | "/connect"
            | "/models"
            | "/settings"
            | "/mode plan"
            | "/mode build"
            | "/permissions ask"
            | "/permissions read-only"
            | "/status"
            | "/acp"
            | "/git"
    ) || state
        .runtime
        .lanes
        .iter()
        .any(|lane| input == format!("/lane inspect {}", lane.id))
}

pub(super) fn render_command_suggestions(frame: &mut Frame, state: &TuiState) {
    if !is_command_palette_visible(state) {
        return;
    }
    let rows = suggestions(state)
        .into_iter()
        .enumerate()
        .take(8)
        .map(|(index, item)| {
            let marker = if index == state.ui.command_selection {
                ">"
            } else {
                " "
            };
            format!(
                "{marker} {:<24} {}",
                truncate(&item.command, 24),
                truncate(&item.summary, 42)
            )
        })
        .collect::<Vec<_>>();
    let title = if state.ui.input.as_str() == "/setup" {
        "SETUP WIZARD"
    } else if state.ui.input.as_str().starts_with("/lane") {
        "LANE ACTIONS"
    } else {
        "COMMANDS"
    };
    let block = panel(title, rows, frame.width.min(76), 11, None);
    frame.write_block(3, 0, &block);
}

fn suggestions(state: &TuiState) -> Vec<CommandSuggestion> {
    let query = state.ui.input.as_str();
    if query == "/setup" {
        return [
            ("/connect", "Connect a provider"),
            ("/provider doctor", "Run provider doctor"),
            ("fallback test-local", "Offline fallback model"),
        ]
        .into_iter()
        .map(|(command, summary)| CommandSuggestion {
            command: command.to_string(),
            summary: summary.to_string(),
        })
        .collect();
    }
    let mut values = command_registry()
        .iter()
        .map(|command| CommandSuggestion {
            command: command.command.to_string(),
            summary: command.summary.to_string(),
        })
        .collect::<Vec<_>>();
    values.extend(state.runtime.lanes.iter().map(|lane| CommandSuggestion {
        command: format!("/lane inspect {}", lane.id),
        summary: format!("{} [{:?}]", lane.summary, lane.status),
    }));
    values
        .into_iter()
        .filter(|item| item.command.starts_with(query) || query == "/")
        .collect()
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn palette_uses_runtime_lanes() {
        let mut state = TuiState::default();
        state.ui.input = "/lane".into();
        assert_eq!(suggestions(&state)[0].command, "/lanes");

        state.runtime.lanes = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        assert!(
            suggestions(&state)
                .iter()
                .any(|item| item.command.starts_with("/lane inspect "))
        );
    }

    #[test]
    fn completion_never_executes_an_effect() {
        let mut state = TuiState::default();
        state.ui.input = "/con".into();
        assert!(complete_selected(&mut state));
        assert_eq!(state.ui.input, "/connect");
    }

    #[test]
    fn palette_exposes_task6_lens_routes() {
        let mut state = TuiState::default();
        state.ui.input = "/".into();

        let commands = suggestions(&state)
            .into_iter()
            .map(|item| item.command)
            .collect::<Vec<_>>();

        for route in ["/setup", "/settings", "/lanes", "/decisions", "/gallery"] {
            assert!(commands.iter().any(|command| command == route), "{route}");
        }
    }

    #[test]
    fn palette_exposes_acp_as_an_exact_system_command() {
        let mut state = TuiState::default();
        state.ui.input = "/".into();

        assert!(
            suggestions(&state)
                .iter()
                .any(|item| item.command == "/acp")
        );
        state.ui.input = "/acp".into();
        assert!(is_exact_command(&state));
        assert!(!should_complete_on_enter(&state));
    }

    #[test]
    fn settings_is_an_exact_selector_first_palette_route() {
        let mut state = TuiState::default();
        state.ui.input = "/settings".into();

        assert!(is_exact_command(&state));
        assert!(!should_complete_on_enter(&state));
        assert_eq!(
            selected_command(&state).map(|item| item.command),
            Some("/settings".into())
        );
    }
}
