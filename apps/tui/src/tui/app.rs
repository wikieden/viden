use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
#[cfg(test)]
use viden_core::ApprovalResponse;
use viden_core::{
    AgentSessionRequest, AgentSessionStatus, AgentSessionView, AgentStartability, AuditObjectRef,
    CoreClient, EventCursor, RuntimeCommand, RuntimeOwner, RuntimeViewState, StarterLanePreset,
    TuiColorDepth,
};

use super::audit_panel::AuditPanel;
use super::client::{PumpOutcome, TuiClientDriver, TuiClientError};
use super::command_palette::{
    close_on_escape, complete_selected, move_selection, reset_for_input_change,
    should_complete_on_enter,
};
use super::composer::composer_content_width;
use super::decision::{
    DecisionPick, SupervisionAction, SupervisionTarget, TextRequirement, audit_scope,
    build_dispatch, decision_picks, evidence_scope, overlay_actions,
};
use super::evidence_panel::{EVIDENCE_READS_CAPABILITY, EvidencePanel};
use super::geometry::effective_layout_width;
use super::input::{
    ApprovalKeyEffect, apply_approval_key, close_focus_on_escape, effective_input_mode, input_focus,
};
use super::jump::{JumpIndex, JumpItem, JumpKind};
use super::keymap::{InputIntent, InputMode, OverlayKind, RuntimeFacts, reduce_input};
use super::modal::{
    AcpPickerRowKind, DEFAULT_APPROVAL_FOCUS, GitPickerRowKind, acp_picker_rows, git_picker_rows,
    git_target, interaction_panel_choice_count, selected_interaction_command,
};
use super::operator_git::{
    OPERATOR_GIT_CAPABILITY, OperatorGitSettlement, action_label_key, collapsed_output,
    failure_copy, operator_git_owner,
};
use super::preferences::{
    ColorDepth, PreferenceField, SettingsPanel, TerminalCapabilities,
    UI_PREFERENCE_PERSISTENCE_CAPABILITY,
};
use super::projection::{CancelOwnerProjection, CockpitProjection};
use super::state::{
    AcpPickerPhase, ConflictDetailTarget, FocusedConversation, GitPickerPhase, InteractionPanel,
    Lens, OverlayState, PendingAcpStart, PendingNativeLane, SupervisionInput, SupervisionPanel,
    TuiEntry, TuiState, composer_target_busy, has_active_work,
};
use super::terminal::TerminalGuard;
use super::text::truncate_tail;
use super::workspace_files::WORKSPACE_FILES_CAPABILITY;

const PROJECT_ONBOARDING_CAPABILITY: &str = "runtime.project_onboarding";
const AGENT_ADAPTERS_CAPABILITY: &str = "runtime.agent_adapters";
const AGENT_SESSIONS_CAPABILITY: &str = "runtime.agent_sessions";
const WORKSPACE_ELIGIBILITY_CAPABILITY: &str = "runtime.workspace_eligibility";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiOptions {
    pub startup_summary: String,
    pub startup_check: bool,
    pub color_depth: TuiColorDepth,
}

impl TuiOptions {
    pub fn new(startup_summary: impl Into<String>) -> Self {
        Self {
            startup_summary: startup_summary.into(),
            startup_check: false,
            color_depth: detect_color_depth(),
        }
    }

    pub fn with_startup_check(mut self) -> Self {
        self.startup_check = true;
        self
    }

    pub fn with_color_depth(mut self, color_depth: TuiColorDepth) -> Self {
        self.color_depth = color_depth;
        self
    }
}

#[derive(Debug)]
pub enum TuiError {
    Client(TuiClientError),
    Terminal(String),
}

impl std::fmt::Display for TuiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Client(error) => write!(formatter, "{error}"),
            Self::Terminal(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for TuiError {}

impl From<TuiClientError> for TuiError {
    fn from(value: TuiClientError) -> Self {
        Self::Client(value)
    }
}

pub fn run_tui<C: CoreClient>(client: C, options: TuiOptions) -> Result<(), TuiError> {
    let mut driver = TuiClientDriver::connect(client)?;
    if driver.has_capability(PROJECT_ONBOARDING_CAPABILITY) {
        driver.send(RuntimeCommand::ProbeProject)?;
    }
    if options.startup_check {
        let _state = state_from_driver(&driver, &options);
        return Ok(());
    }
    let terminal_capabilities = TerminalCapabilities::detect();
    let color_depth = ColorDepth::from(options.color_depth);
    let mut terminal = TerminalGuard::enter_with_preferences(
        &driver.view().snapshot.ui_preferences,
        color_depth,
        terminal_capabilities,
    )
    .map_err(TuiError::Terminal)?;
    let mut state = state_from_driver(&driver, &options);
    state.ui.pulse_frame = super::render::sampled_pulse_frame();
    terminal.draw(&state).map_err(TuiError::Terminal)?;

    loop {
        let outcome = driver.pump()?;
        apply_pump_outcome(&mut state, &driver, outcome);
        project_driver_view(&mut state, &driver);
        observe_driver_events(&mut state, &mut driver)?;
        terminal.refresh_appearance(
            &driver.view().snapshot.ui_preferences,
            state.ui.color_depth,
            terminal_capabilities,
        );
        state.ui.pulse_frame = super::render::sampled_pulse_frame();
        terminal.draw(&state).map_err(TuiError::Terminal)?;

        if !event::poll(std::time::Duration::from_millis(100))
            .map_err(|err| TuiError::Terminal(err.to_string()))?
        {
            continue;
        }

        let event = event::read().map_err(|err| TuiError::Terminal(err.to_string()))?;
        let size = crossterm::terminal::size().unwrap_or((80, 24));
        if handle_ui_event(&mut driver, &mut state, event, size)? == UiEventOutcome::Exit {
            break;
        }
    }
    Ok(())
}

fn detect_color_depth() -> TuiColorDepth {
    let capabilities = TerminalCapabilities::detect();
    if capabilities.truecolor {
        TuiColorDepth::Truecolor
    } else if capabilities.ansi256 {
        TuiColorDepth::Ansi256
    } else {
        TuiColorDepth::Ansi16
    }
}

fn state_from_driver<C: CoreClient>(driver: &TuiClientDriver<C>, options: &TuiOptions) -> TuiState {
    let mut state = TuiState::new(driver.view().clone());
    state.ui.color_depth = ColorDepth::from(options.color_depth);
    state.ui.theme_name = ui_profile_label(&state.runtime.snapshot.ui_preferences);
    state.ui.entries.push(TuiEntry {
        label: "system".to_string(),
        body: options.startup_summary.clone(),
    });
    seed_settled_agent_sessions(&mut state, driver.view());
    project_driver_view(&mut state, driver);
    state
}

/// Records every session the authoritative snapshot already shows as finished.
///
/// The snapshot is this client's baseline of settled history: Core reduces the
/// persisted `.viden` log into it before the driver pumps a single event, so a
/// session terminal here finished before the operator was watching. Recording
/// them is what keeps replayed history out of the transcript when Core later
/// re-delivers their terminal facts on the live stream.
fn seed_settled_agent_sessions(state: &mut TuiState, view: &RuntimeViewState) {
    for session in &view.agent_sessions {
        if is_terminal_session_status(session.status) {
            state
                .ui
                .settled_agent_sessions
                .insert(session.session_id.clone());
        }
    }
}

/// The transcript entry a live-observed terminal agent-session fact leaves
/// behind, or `None` when the fact carries no reply worth showing.
///
/// Core settles the unscoped `assistant_stream` on this fact, and that stream
/// was the only surface rendering an ACP reply — the TUI reads neither
/// `agent_conversation` nor `session.output` anywhere else. Without this the
/// reply an operator just watched stream in would vanish at the instant it
/// completed. Replayed history must still vanish, which is why the caller
/// gates on whether this client had already seen the session finish.
/// The session an event settles, with the status that settled it.
///
/// The terminal set mirrors [`RuntimeViewState::apply_event`]'s own rule for
/// settling the unscoped assistant stream, so the transcript gains an entry
/// exactly where the stream loses its text. An `AgentSessionUpdated` counts
/// only when the session it carries reads terminal.
fn terminal_agent_session(
    kind: &viden_core::RuntimeEventKind,
) -> Option<(&AgentSessionView, AgentSessionStatus)> {
    match kind {
        viden_core::RuntimeEventKind::AgentSessionCompleted { session } => {
            Some((session, AgentSessionStatus::Completed))
        }
        viden_core::RuntimeEventKind::AgentSessionFailed { session } => {
            Some((session, AgentSessionStatus::Failed))
        }
        viden_core::RuntimeEventKind::AgentSessionUpdated { session }
            if is_terminal_session_status(session.status) =>
        {
            Some((session, session.status))
        }
        _ => None,
    }
}

fn is_terminal_session_status(status: AgentSessionStatus) -> bool {
    matches!(
        status,
        AgentSessionStatus::Completed | AgentSessionStatus::Failed | AgentSessionStatus::Cancelled
    )
}

fn settled_session_entry(
    session: &AgentSessionView,
    status: AgentSessionStatus,
) -> Option<TuiEntry> {
    let output = session.output.as_deref().unwrap_or_default().trim_end();
    if output.is_empty() {
        return None;
    }
    // The label keeps `assistant` as its leading kind so the entry reads and
    // classifies as a reply, and names the session tail-first because ACP
    // session ids differ only at their end.
    let label = match status {
        AgentSessionStatus::Completed => {
            format!("assistant · {}", truncate_tail(&session.session_id, 13))
        }
        AgentSessionStatus::Cancelled => format!(
            "assistant · {} · cancelled",
            truncate_tail(&session.session_id, 13)
        ),
        _ => format!(
            "assistant · {} · failed",
            truncate_tail(&session.session_id, 13)
        ),
    };
    // A failure's diagnostic is the reason the reply stops where it does, so it
    // travels with the reply rather than being dropped on the floor.
    let body = match session.diagnostic.as_deref().map(str::trim) {
        Some(diagnostic) if !diagnostic.is_empty() && status != AgentSessionStatus::Completed => {
            format!("{output}\n\n{diagnostic}")
        }
        _ => output.to_string(),
    };
    Some(TuiEntry { label, body })
}

fn ui_profile_label(preferences: &viden_core::ResolvedUiPreferences) -> String {
    let locale = match preferences.locale {
        viden_core::LocaleId::System => "system",
        viden_core::LocaleId::En => "en",
        viden_core::LocaleId::ZhCn => "zh-CN",
    };
    let skin = match preferences.skin {
        viden_core::UiSkin::Aurora => "aurora",
        viden_core::UiSkin::Ice => "ice",
        viden_core::UiSkin::Mono => "mono",
        viden_core::UiSkin::Amber => "amber",
        viden_core::UiSkin::Phosphor => "phosphor",
    };
    let mode = match preferences.mode {
        viden_core::UiColorMode::System => "system",
        viden_core::UiColorMode::Dark => "dark",
        viden_core::UiColorMode::Light => "light",
    };
    let density = match preferences.density {
        viden_core::UiDensity::Compact => "compact",
        viden_core::UiDensity::Regular => "regular",
        viden_core::UiDensity::Comfy => "comfy",
    };
    let motion = match preferences.motion {
        viden_core::UiMotion::System => "system",
        viden_core::UiMotion::Reduced => "reduced",
        viden_core::UiMotion::Full => "full",
    };
    format!("{skin}/{mode} · {locale} · {density} · {motion}")
}

/// Replaces TUI runtime presentation from the Core-owned projection while
/// preserving only local input/layout state and the startup/user transcript.
fn project_driver_view<C: CoreClient>(state: &mut TuiState, driver: &TuiClientDriver<C>) {
    // Capabilities are snapshot-scoped compatibility facts. Refresh them with
    // every atomic view projection so restart recovery cannot leave stale UI
    // affordances enabled after an extension disappears.
    state.capabilities = driver.capabilities();
    // Snapshot-scoped like every other capability fact: an extension that
    // disappears must take its affordance with it, so the `~` scope falls back
    // to the honest unavailable row instead of serving a stale inventory.
    let workspace_files = state.has_capability(WORKSPACE_FILES_CAPABILITY);
    state.ui.workspace_files.mark_available(workspace_files);
    project_runtime_view(state, driver.view(), driver.cursor());
}

fn project_runtime_view(state: &mut TuiState, view: &RuntimeViewState, _cursor: &EventCursor) {
    state.ui.theme_name = ui_profile_label(&view.snapshot.ui_preferences);
    state.runtime = view.clone();
    reconcile_ui_state_with_runtime(state);
}

/// Drops only presentation identities invalidated by the newly committed Core
/// view. Composer, mode, scrollback, and other local layout state survive the
/// same atomic snapshot/replay replacement.
fn reconcile_ui_state_with_runtime(state: &mut TuiState) {
    let had_core_selection = state.ui.focused_lane.is_some() || !state.ui.session_id.is_empty();
    let focused_lane = state
        .ui
        .focused_lane
        .as_ref()
        .and_then(|lane_id| state.runtime.lanes.iter().find(|lane| &lane.id == lane_id));
    let focused_acp_valid = match state.ui.focused_conversation.as_ref() {
        Some(FocusedConversation::AcpSession(session_id)) => state
            .runtime
            .agent_sessions
            .iter()
            .any(|session| session.session_id == *session_id),
        _ => false,
    };

    match focused_lane {
        None if had_core_selection => {
            state.ui.clear_lane_focus();
            state.ui.session_id.clear();
            state.ui.focused_conversation = None;
            if state.ui.lens == Lens::Session {
                state.ui.lens = Lens::Board;
            }
        }
        Some(lane)
            if !focused_acp_valid
                && (state.ui.session_id.is_empty()
                    || !lane.active_session_ids.contains(&state.ui.session_id)) =>
        {
            state.ui.session_id.clear();
            // The Lane is still selected but has no session to show, so the
            // board is the honest view — *while the operator is looking at the
            // Lane*. Once they close its detail panel the selection is only a
            // command target, and the board lens renders no composer
            // (`render::render_frame`), so keeping it here would leave `/git`
            // and every prompt to be typed into a surface that is not on
            // screen. That is the same defect this branch closes, one layer up.
            if state.ui.lens == Lens::Session && state.ui.lane_detail_open {
                state.ui.lens = Lens::Board;
            }
        }
        None | Some(_) => {}
    }

    let stale_approval_focus = state
        .ui
        .overlay
        .as_ref()
        .filter(|overlay| overlay.kind == OverlayKind::Approval)
        .is_some_and(|overlay| {
            overlay.selected_id.as_ref().map_or_else(
                || overlay.selected >= state.runtime.pending_approvals.len(),
                |request_id| {
                    !state
                        .runtime
                        .pending_approvals
                        .iter()
                        .any(|approval| &approval.id == request_id)
                },
            )
        });
    if stale_approval_focus {
        state.ui.overlay = None;
    }
}

pub(super) fn dispatch_intent<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    command: RuntimeCommand,
) -> Result<String, TuiClientError> {
    driver.send(command)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiEventOutcome {
    Redraw,
    Exit,
}

fn handle_ui_event<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
    event: Event,
    terminal_size: (u16, u16),
) -> Result<UiEventOutcome, TuiError> {
    match event {
        Event::Resize(_, _) | Event::FocusGained | Event::FocusLost => Ok(UiEventOutcome::Redraw),
        Event::Paste(text) => {
            let approval_pending = !driver.view().pending_approvals.is_empty();
            if state
                .ui
                .overlay
                .as_ref()
                .is_some_and(|overlay| overlay.kind == OverlayKind::SupervisionDecision)
            {
                // The supervision overlay has no filter. A paste is either the
                // reason the operator is typing, or composer text.
                match state
                    .ui
                    .supervision
                    .as_mut()
                    .and_then(|panel| panel.input.as_mut())
                {
                    Some(input) => input.text.push_str(&text),
                    None => {
                        state.ui.input.paste(&text);
                        reset_for_input_change(state);
                    }
                }
            } else if let Some(overlay) = state.ui.overlay.as_mut() {
                if overlay.kind != OverlayKind::Approval {
                    overlay.filter.push_str(&text);
                    overlay.selected = 0;
                }
            } else if state.ui.interaction_panel.is_some() {
                if let Some(InteractionPanel::Setup { draft, .. }) =
                    state.ui.interaction_panel.as_mut()
                {
                    // A pasted candidate is an explicit operator edit and
                    // replaces the generated template byte-for-byte.
                    *draft = text;
                } else {
                    for value in text.chars() {
                        edit_interaction_panel_text(state, Some(value));
                    }
                }
            } else if approval_pending {
                // Approval remains pinned while the composer stays editable;
                // pasted content must never resolve the approval.
                state.ui.input.paste(&text);
                reset_for_input_change(state);
            } else if matches!(
                effective_input_mode(state),
                InputMode::Insert | InputMode::Overlay
            ) {
                state.ui.input.paste(&text);
                reset_for_input_change(state);
            }
            Ok(UiEventOutcome::Redraw)
        }
        Event::Mouse(_) => Ok(UiEventOutcome::Redraw),
        Event::Key(key) => handle_ui_key(driver, state, key, terminal_size),
    }
}

fn handle_ui_key<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
    key: KeyEvent,
    terminal_size: (u16, u16),
) -> Result<UiEventOutcome, TuiError> {
    if key.code == KeyCode::Char('r')
        && key.modifiers.is_empty()
        && matches!(
            state.ui.interaction_panel,
            Some(InteractionPanel::AcpPicker {
                phase: AcpPickerPhase::Browse,
                ..
            })
        )
    {
        retry_selected_acp_session(driver, state)?;
        return Ok(UiEventOutcome::Redraw);
    }
    let mode = effective_input_mode(state);
    let focus = input_focus(state);
    let facts = RuntimeFacts {
        current_work_owner: current_work_owner(driver, state),
        has_active_work: has_active_work(state),
    };
    let is_ctrl_c = key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
    if !is_ctrl_c || facts.has_active_work {
        state.ui.idle_ctrl_c_armed = false;
    }
    let intent = reduce_input(mode, focus, key, facts);
    apply_input_intent(driver, state, key, intent, terminal_size)
}

fn retry_selected_acp_session<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &TuiState,
) -> Result<(), TuiClientError> {
    let Some(InteractionPanel::AcpPicker { selected, .. }) = state.ui.interaction_panel.as_ref()
    else {
        return Ok(());
    };
    let rows = acp_picker_rows(state);
    let Some(AcpPickerRowKind::Session { session_id }) = rows.get(*selected).map(|row| &row.kind)
    else {
        return Ok(());
    };
    let Some(session) = state.runtime.agent_sessions.iter().find(|session| {
        &session.session_id == session_id
            && matches!(
                session.status,
                viden_core::AgentSessionStatus::Failed | viden_core::AgentSessionStatus::Cancelled
            )
    }) else {
        return Ok(());
    };
    driver.send_for_owner(
        session.owner.clone(),
        RuntimeCommand::RetryAgentSession {
            session_id: session.session_id.clone(),
        },
    )?;
    Ok(())
}

fn apply_input_intent<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
    key: KeyEvent,
    intent: InputIntent,
    terminal_size: (u16, u16),
) -> Result<UiEventOutcome, TuiError> {
    if let Some(outcome) = apply_pending_approval_intent(driver, state, key, &intent)? {
        return Ok(outcome);
    }
    // Approval stays pinned and wins; the supervision overlay is regular and
    // Esc-dismissable, so it only claims keys once no approval owns them.
    if let Some(outcome) = apply_supervision_decision_intent(driver, state, &intent)? {
        return Ok(outcome);
    }
    if let Some(outcome) = apply_audit_timeline_intent(driver, state, &intent)? {
        return Ok(outcome);
    }
    if let Some(outcome) = apply_evidence_inspector_intent(driver, state, &intent)? {
        return Ok(outcome);
    }
    if let Some(outcome) = apply_conflict_detail_intent(state, &intent) {
        return Ok(outcome);
    }
    match intent {
        InputIntent::None => {}
        InputIntent::EnterInsert => state.ui.input_mode = InputMode::Insert,
        InputIntent::LeaveInsert => state.ui.input_mode = InputMode::Normal,
        InputIntent::OpenOverlay(kind) => {
            let previous_overlay = state.ui.overlay.take();
            state.ui.supervision = None;
            state.ui.audit = None;
            state.ui.evidence = None;
            state.ui.conflict_detail = None;
            state.ui.overlay = Some(if kind == OverlayKind::GlobalJump {
                OverlayState::global_jump(previous_overlay)
            } else {
                OverlayState::new(kind)
            });
            if kind == OverlayKind::GlobalJump {
                request_workspace_files(driver, state)?;
            }
            state.ui.idle_ctrl_c_armed = false;
        }
        InputIntent::CloseOverlay => match state.ui.overlay.take() {
            Some(overlay) if overlay.kind == OverlayKind::GlobalJump => {
                state.ui.overlay = overlay.previous_overlay.map(|previous| *previous);
            }
            Some(_) => {}
            None => close_interaction_panel_or_palette(key, state),
        },
        InputIntent::ClearSelection => {
            close_focus_on_escape(key, state);
        }
        InputIntent::ArmExitConfirmation => state.ui.idle_ctrl_c_armed = true,
        InputIntent::CancelCurrentWork { owner } => {
            state.ui.idle_ctrl_c_armed = false;
            if let Some(FocusedConversation::AcpSession(session_id)) =
                state.ui.focused_conversation.as_ref()
            {
                driver.send_for_owner(
                    owner,
                    RuntimeCommand::CancelAgentSession {
                        session_id: session_id.clone(),
                    },
                )?;
            } else {
                driver.send_for_owner(owner, RuntimeCommand::CancelActiveTurn)?;
            }
            state.ui.entries.push(TuiEntry {
                label: "command".to_string(),
                body: super::i18n::text(state, "cancel.requested"),
            });
        }
        InputIntent::CycleAgentFocus => cycle_agent_focus(state),
        InputIntent::OpenNativeLane => {
            state.ui.overlay = None;
            state.ui.interaction_panel = Some(InteractionPanel::NewLaneTask {
                task: String::new(),
            });
        }
        InputIntent::Exit => return Ok(UiEventOutcome::Exit),
        InputIntent::InsertChar(value) => {
            if let Some(overlay) = state.ui.overlay.as_mut() {
                overlay.filter.push(value);
                overlay.selected = 0;
            } else if state.ui.interaction_panel.is_some() {
                edit_interaction_panel_text(state, Some(value));
            } else {
                push_composer_char(state, value);
            }
        }
        InputIntent::InsertNewline => {
            state.ui.input.insert_newline();
            reset_for_input_change(state);
        }
        InputIntent::Backspace => {
            if let Some(overlay) = state.ui.overlay.as_mut() {
                overlay.filter.pop();
                overlay.selected = 0;
            } else if state.ui.interaction_panel.is_some() {
                edit_interaction_panel_text(state, None);
            } else {
                state.ui.input.backspace();
                reset_for_input_change(state);
            }
        }
        InputIntent::MoveCursorLeft => state.ui.input.move_left(),
        InputIntent::MoveCursorRight => state.ui.input.move_right(),
        InputIntent::MoveCursorUp => {
            let width = composer_content_width(state, effective_layout_width(terminal_size.0));
            state.ui.input.move_up(width);
        }
        InputIntent::MoveCursorDown => {
            let width = composer_content_width(state, effective_layout_width(terminal_size.0));
            state.ui.input.move_down(width);
        }
        InputIntent::Submit if state.ui.input.has_unclosed_code_fence() => {
            state.ui.input.insert_newline();
            reset_for_input_change(state);
        }
        InputIntent::Submit => submit_composer(driver, state)?,
        InputIntent::MoveSelection(delta) => {
            // The jump index reads the whole client state, so its row count is
            // resolved before the overlay is borrowed mutably to move within it.
            let jump_rows = state
                .ui
                .overlay
                .as_ref()
                .filter(|overlay| overlay.kind == OverlayKind::GlobalJump)
                .map(|overlay| JumpIndex::from_state(state).search(&overlay.filter).len());
            if let Some(overlay) = state.ui.overlay.as_mut() {
                let item_count = jump_rows.unwrap_or(usize::MAX);
                overlay.selected = if delta < 0 {
                    overlay.selected.saturating_sub(1)
                } else {
                    overlay
                        .selected
                        .saturating_add(1)
                        .min(item_count.saturating_sub(1))
                };
            } else if state.ui.interaction_panel.is_some() {
                move_interaction_selection(state, delta);
            } else {
                move_selection(state, delta);
            }
        }
        InputIntent::CompleteSelection => {
            if state.ui.overlay.is_none() && state.ui.interaction_panel.is_none() {
                complete_selected(state);
            }
        }
        InputIntent::CompleteOrSubmit => {
            if state
                .ui
                .overlay
                .as_ref()
                .is_some_and(|overlay| overlay.kind == OverlayKind::ExitConfirm)
            {
                if has_active_work(state) {
                    if let Some(owner) = current_work_owner(driver, state) {
                        driver.send_for_owner(owner, RuntimeCommand::CancelActiveTurn)?;
                        state.ui.entries.push(TuiEntry {
                            label: "command".to_string(),
                            body: super::i18n::text(state, "cancel.requested"),
                        });
                    }
                    state.ui.overlay = None;
                    state.ui.idle_ctrl_c_armed = false;
                    return Ok(UiEventOutcome::Redraw);
                }
                return Ok(UiEventOutcome::Exit);
            } else if let Some(overlay) = state.ui.overlay.take() {
                if overlay.kind == OverlayKind::GlobalJump {
                    complete_global_jump_selection(state, overlay);
                } else {
                    complete_overlay_selection(driver, state, overlay)?;
                }
            } else if state.ui.interaction_panel.is_some() {
                if apply_interaction_panel_selection(driver, state)? {
                    submit_composer(driver, state)?;
                }
            } else if should_complete_on_enter(state) {
                complete_selected(state);
            } else {
                submit_composer(driver, state)?;
            }
        }
        InputIntent::Scroll(delta) => scroll_transcript(state, delta),
        InputIntent::ScrollToStart => state.ui.transcript_scroll = usize::MAX / 2,
        InputIntent::ScrollToEnd => state.ui.transcript_scroll = 0,
    }
    Ok(UiEventOutcome::Redraw)
}

fn current_work_owner<C: CoreClient>(
    driver: &TuiClientDriver<C>,
    state: &TuiState,
) -> Option<viden_types::RuntimeOwner> {
    if let Some(FocusedConversation::AcpSession(session_id)) =
        state.ui.focused_conversation.as_ref()
        && let Some(session) = state.runtime.agent_sessions.iter().find(|session| {
            &session.session_id == session_id
                && matches!(
                    session.status,
                    viden_core::AgentSessionStatus::Starting
                        | viden_core::AgentSessionStatus::Running
                        | viden_core::AgentSessionStatus::WaitingApproval
                )
        })
    {
        return Some(session.owner.clone());
    }
    let focused = state.ui.focused_lane.as_deref().and_then(|lane_id| {
        state
            .runtime
            .lanes
            .iter()
            .find(|lane| lane.id == lane_id && lane.is_active())
            .map(|lane| lane.id.as_str())
    });
    let lane_id = focused.or_else(|| {
        let mut active = state.runtime.lanes.iter().filter(|lane| lane.is_active());
        let only = active.next()?;
        active.next().is_none().then_some(only.id.as_str())
    })?;
    let capabilities = driver.capabilities();
    let projection =
        CockpitProjection::from_with_capabilities(&state.runtime, &state.ui, &capabilities);
    match projection.cancel_owner_for_lane(lane_id) {
        CancelOwnerProjection::Available(owner) => Some(owner),
        CancelOwnerProjection::Unavailable(_) => None,
    }
}

fn apply_pending_approval_intent<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
    key: KeyEvent,
    intent: &InputIntent,
) -> Result<Option<UiEventOutcome>, TuiError> {
    let Some(approval_overlay) = state
        .ui
        .overlay
        .as_ref()
        .filter(|overlay| overlay.kind == OverlayKind::Approval)
    else {
        return Ok(None);
    };
    let approval_selected = approval_overlay.selected;
    let approval_request_id = approval_overlay.selected_id.clone();
    if state.runtime.pending_approvals.is_empty()
        || !matches!(
            intent,
            InputIntent::MoveSelection(_)
                | InputIntent::CompleteSelection
                | InputIntent::CompleteOrSubmit
                | InputIntent::InsertChar(_)
        )
    {
        return Ok(None);
    }

    match apply_approval_key(key, state) {
        ApprovalKeyEffect::ResolveScoped(response) => {
            let approval = approval_request_id.as_ref().map_or_else(
                || driver.view().pending_approvals.get(approval_selected),
                |request_id| {
                    driver
                        .view()
                        .pending_approvals
                        .iter()
                        .find(|approval| &approval.id == request_id)
                },
            );
            if let Some(approval) = approval {
                driver.send_for_owner(
                    approval.owner.clone(),
                    RuntimeCommand::RespondToApproval {
                        request_id: approval.id.clone(),
                        response,
                    },
                )?;
            }
            Ok(Some(UiEventOutcome::Redraw))
        }
        ApprovalKeyEffect::Redraw => Ok(Some(UiEventOutcome::Redraw)),
        ApprovalKeyEffect::None => Ok(None),
    }
}

/// Opens the supervision decision overlay on one Core record.
///
/// Opening is "initiating the next supervision action", so a settled outcome
/// from the previous decision resets here. A *pending* outcome is deliberately
/// preserved: the correlation is still live and its badge must keep showing.
fn open_supervision_decision(state: &mut TuiState, target: SupervisionTarget) {
    state.supervision.reset_if_settled();
    state.ui.supervision = Some(SupervisionPanel::new(target));
    state.ui.overlay = Some(OverlayState::new(OverlayKind::SupervisionDecision));
}

/// Opens the read-only audit timeline and dispatches its first page.
///
/// The command is sent *before* any state changes, so a transport failure
/// leaves the previous surface intact instead of opening an overlay that would
/// never be answered. `QueryAudit` mutates nothing and prompts for no
/// permission, so it is dispatched directly rather than through the supervision
/// correlation slot, and it stays available in Plan mode.
fn open_audit_timeline<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
    scope: Option<AuditObjectRef>,
) -> Result<(), TuiClientError> {
    let mut panel = AuditPanel::new(scope);
    let command_id = driver.send(RuntimeCommand::QueryAudit {
        query: panel.next_query(),
    })?;
    panel.begin(command_id);
    state.ui.supervision = None;
    state.ui.evidence = None;
    state.ui.audit = Some(panel);
    state.ui.overlay = Some(OverlayState::new(OverlayKind::AuditTimeline));
    Ok(())
}

/// Opens the read-only evidence inspector and dispatches its first page.
///
/// The command is sent *before* any state changes, so a transport failure
/// leaves the previous surface intact instead of opening an overlay that would
/// never be answered. `QueryEvidence` mutates nothing and prompts for no
/// permission — its gate posture is `QueryAudit`'s, not a workspace read's —
/// so it is dispatched directly rather than through the supervision
/// correlation slot, and it stays available in Plan mode.
///
/// Without the capability nothing is sent at all: the caller states the gap
/// instead, because a command Core never published can never be answered.
fn open_evidence_inspector<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
    scope: Option<RuntimeOwner>,
) -> Result<(), TuiClientError> {
    let mut panel = EvidencePanel::new(scope);
    let command_id = driver.send(RuntimeCommand::QueryEvidence {
        query: panel.next_query(),
    })?;
    panel.begin_page(command_id);
    state.ui.supervision = None;
    state.ui.audit = None;
    state.ui.conflict_detail = None;
    state.ui.evidence = Some(panel);
    state.ui.overlay = Some(OverlayState::new(OverlayKind::EvidenceInspector));
    Ok(())
}

/// The evidence scope of the surface the operator is looking at.
///
/// A focused Lane narrows the read to that Lane by the id Core published, with
/// every other owner field left unset so Core's prefix match treats them as
/// wildcards. Nothing else is filled in: a workspace or project id this client
/// invented would silently answer a different question than the one asked.
fn focused_lane_evidence_scope(state: &TuiState) -> Option<RuntimeOwner> {
    let lane_id = state.ui.focused_lane.as_deref()?;
    state
        .runtime
        .lanes
        .iter()
        .any(|lane| lane.id == lane_id)
        .then(|| RuntimeOwner {
            lane_id: Some(lane_id.to_string()),
            ..RuntimeOwner::default()
        })
}

/// States the missing evidence capability as a typed transcript fact.
///
/// Nothing is sent and no overlay opens: the surface stays visible and
/// labelled in the jump index and on the supervision overlay, which is what
/// keeps "this exists but Core does not publish it" distinguishable from "this
/// does not exist".
fn refuse_evidence_read(state: &mut TuiState) {
    state.ui.entries.push(TuiEntry {
        label: "system".to_string(),
        body: super::i18n::text(state, "supervision.evidence.unavailable"),
    });
}

/// Closes the evidence inspector and drops its page and content cache.
///
/// Dropping the panel also drops both correlations, so an answer that arrives
/// afterwards is ignored rather than applied to a surface nobody is looking at.
fn close_evidence_inspector(state: &mut TuiState) {
    state.ui.evidence = None;
    state.ui.overlay = None;
}

/// Owns every key while the evidence inspector overlay is focused.
///
/// The overlay is read-only. `Esc` unwinds the detail pane before the overlay
/// itself, arrows move the selection or scroll the open content, `Enter` opens
/// a row or asks for the next page, `f` cycles the kind filter, `r` reloads
/// from the first page, and every other printable character keeps editing the
/// composer so a streaming turn stays answerable while evidence is open.
fn apply_evidence_inspector_intent<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
    intent: &InputIntent,
) -> Result<Option<UiEventOutcome>, TuiError> {
    if !state
        .ui
        .overlay
        .as_ref()
        .is_some_and(|overlay| overlay.kind == OverlayKind::EvidenceInspector)
        || state.ui.evidence.is_none()
    {
        return Ok(None);
    }
    match intent {
        InputIntent::CloseOverlay => {
            if !state
                .ui
                .evidence
                .as_mut()
                .expect("panel checked above")
                .close_detail()
            {
                close_evidence_inspector(state);
            }
            Ok(Some(UiEventOutcome::Redraw))
        }
        InputIntent::MoveSelection(delta) => {
            let panel = state.ui.evidence.as_mut().expect("panel checked above");
            if panel.detail().is_some() {
                panel.scroll_detail(*delta);
            } else {
                panel.move_selection(*delta);
            }
            Ok(Some(UiEventOutcome::Redraw))
        }
        InputIntent::CompleteSelection | InputIntent::CompleteOrSubmit => {
            activate_evidence_row(driver, state)?;
            Ok(Some(UiEventOutcome::Redraw))
        }
        // `f` and `r` are the overlay's own two keys, named in its hint. Both
        // re-read from Core rather than reshaping rows already held.
        InputIntent::InsertChar('f') => {
            if state
                .ui
                .evidence
                .as_mut()
                .expect("panel checked above")
                .cycle_filter()
            {
                dispatch_evidence_page(driver, state)?;
            }
            Ok(Some(UiEventOutcome::Redraw))
        }
        InputIntent::InsertChar('r') => {
            state
                .ui
                .evidence
                .as_mut()
                .expect("panel checked above")
                .refresh();
            dispatch_evidence_page(driver, state)?;
            Ok(Some(UiEventOutcome::Redraw))
        }
        InputIntent::InsertChar(value) => {
            push_composer_char(state, *value);
            Ok(Some(UiEventOutcome::Redraw))
        }
        InputIntent::Backspace => {
            state.ui.input.backspace();
            reset_for_input_change(state);
            Ok(Some(UiEventOutcome::Redraw))
        }
        _ => Ok(None),
    }
}

/// Confirms the selected row: the next page, or one row's canonical content.
///
/// A second read of either kind while one is in flight is refused locally and
/// nothing is sent — two answers racing one correlation slot could not be
/// attributed honestly — and content Core already published is re-rendered from
/// the overlay's own cache rather than asked for again.
fn activate_evidence_row<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
) -> Result<(), TuiClientError> {
    let Some(panel) = state.ui.evidence.as_ref() else {
        return Ok(());
    };
    if panel.detail().is_some() {
        return Ok(());
    }
    if panel.selected_is_load_more() {
        if !panel.can_load_more() {
            state
                .ui
                .evidence
                .as_mut()
                .expect("panel checked above")
                .refuse_second_read();
            return Ok(());
        }
        return dispatch_evidence_page(driver, state);
    }
    let Some(evidence_id) = panel.selected_entry().map(|entry| entry.id.clone()) else {
        return Ok(());
    };
    let needs_read = panel.should_read_content(&evidence_id);
    let cached = panel.content_for(&evidence_id).is_some();
    if cached || !needs_read {
        // Cached content re-renders from the overlay's own copy; a row whose
        // read cannot start yet still opens, and the pane says the single slot
        // is busy rather than claiming this row has no content.
        let panel = state.ui.evidence.as_mut().expect("panel checked above");
        panel.open_detail(evidence_id);
        if !cached {
            panel.refuse_second_read();
        }
        return Ok(());
    }
    // Sent before the pane opens, so a transport failure leaves the list intact
    // instead of opening a detail that would never be answered.
    let command_id = driver.send(RuntimeCommand::ReadEvidenceContent {
        evidence_id: evidence_id.clone(),
    })?;
    let panel = state.ui.evidence.as_mut().expect("panel checked above");
    panel.begin_content(command_id, evidence_id.clone());
    panel.open_detail(evidence_id);
    Ok(())
}

/// Sends the panel's own next query, whatever page it names.
fn dispatch_evidence_page<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
) -> Result<(), TuiClientError> {
    let Some(panel) = state.ui.evidence.as_ref() else {
        return Ok(());
    };
    let command_id = driver.send(RuntimeCommand::QueryEvidence {
        query: panel.next_query(),
    })?;
    state
        .ui
        .evidence
        .as_mut()
        .expect("panel checked above")
        .begin_page(command_id);
    Ok(())
}

/// Asks Core for the workspace file inventory when the jump index opens.
///
/// Sent only when Core advertises `runtime.workspace_files` and nothing has
/// been read yet: without the capability the client sends nothing at all and
/// the `~` scope states the gap, and with a page already in hand a second read
/// would race the first for one correlation slot. A transport failure leaves
/// the overlay open with the honest unavailable row rather than propagating,
/// because opening the jump index is not itself a read.
fn request_workspace_files<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
) -> Result<(), TuiClientError> {
    if !state.ui.workspace_files.should_read() {
        return Ok(());
    }
    let query = state.ui.workspace_files.next_query();
    match driver.send(RuntimeCommand::QueryWorkspaceFiles { query }) {
        Ok(command_id) => state.ui.workspace_files.begin(command_id),
        Err(error) => state.ui.workspace_files.fail(error.to_string()),
    }
    Ok(())
}

/// Opens the read-only conflict content for one record.
///
/// A pure local read: nothing is sent, nothing is decided, and the payload is
/// re-resolved from `RuntimeViewState` on every frame, so the modal cannot
/// outlive the fact it was opened on. Any in-flight supervision command is left
/// exactly as it was.
fn open_conflict_detail(state: &mut TuiState, target: ConflictDetailTarget) {
    state.ui.supervision = None;
    state.ui.audit = None;
    state.ui.evidence = None;
    state.ui.conflict_detail = Some(target);
    state.ui.overlay = Some(OverlayState::new(OverlayKind::ConflictContent));
}

/// Closes the conflict modal and drops the record it named.
fn close_conflict_detail(state: &mut TuiState) {
    state.ui.conflict_detail = None;
    state.ui.overlay = None;
}

/// Owns every key while the conflict modal is focused.
///
/// The modal resolves nothing, so it has exactly two behaviours: scroll and
/// close. There is deliberately no "resolve" or "take theirs" affordance —
/// Core computed no merge and this client must not invent one.
fn apply_conflict_detail_intent(
    state: &mut TuiState,
    intent: &InputIntent,
) -> Option<UiEventOutcome> {
    if !state
        .ui
        .overlay
        .as_ref()
        .is_some_and(|overlay| overlay.kind == OverlayKind::ConflictContent)
    {
        return None;
    }
    match intent {
        InputIntent::CloseOverlay => {
            close_conflict_detail(state);
            Some(UiEventOutcome::Redraw)
        }
        InputIntent::MoveSelection(delta) => {
            let overlay = state.ui.overlay.as_mut()?;
            overlay.selected = if *delta < 0 {
                overlay.selected.saturating_sub(1)
            } else {
                overlay.selected.saturating_add(1)
            };
            Some(UiEventOutcome::Redraw)
        }
        _ => None,
    }
}

/// Closes the audit timeline and drops its page.
///
/// The TUI has no overlay stack outside the Global Jump return path, so this
/// unwinds to the base state exactly as closing the Approval overlay opened
/// from the Decision Center does. Dropping the panel also drops the correlation,
/// so a page that arrives afterwards is ignored rather than applied to a surface
/// nobody is looking at.
fn close_audit_timeline(state: &mut TuiState) {
    state.ui.audit = None;
    state.ui.overlay = None;
}

/// Owns every key while the audit timeline overlay is focused.
///
/// The overlay is read-only: selection moves through rows, `Enter` on the
/// load-older row asks Core for the next page, `Esc` closes, and no row carries
/// a mutation. Printable characters keep editing the composer, exactly as in the
/// supervision overlay, so a streaming turn stays answerable while history is
/// open.
fn apply_audit_timeline_intent<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
    intent: &InputIntent,
) -> Result<Option<UiEventOutcome>, TuiError> {
    if !state
        .ui
        .overlay
        .as_ref()
        .is_some_and(|overlay| overlay.kind == OverlayKind::AuditTimeline)
        || state.ui.audit.is_none()
    {
        return Ok(None);
    }
    match intent {
        InputIntent::CloseOverlay => {
            close_audit_timeline(state);
            Ok(Some(UiEventOutcome::Redraw))
        }
        InputIntent::MoveSelection(delta) => {
            state
                .ui
                .audit
                .as_mut()
                .expect("panel checked above")
                .move_selection(*delta);
            Ok(Some(UiEventOutcome::Redraw))
        }
        InputIntent::CompleteSelection | InputIntent::CompleteOrSubmit => {
            load_older_audit_page(driver, state)?;
            Ok(Some(UiEventOutcome::Redraw))
        }
        InputIntent::InsertChar(value) => {
            // No text filter on this overlay: typing belongs to the composer.
            push_composer_char(state, *value);
            Ok(Some(UiEventOutcome::Redraw))
        }
        InputIntent::Backspace => {
            state.ui.input.backspace();
            reset_for_input_change(state);
            Ok(Some(UiEventOutcome::Redraw))
        }
        _ => Ok(None),
    }
}

/// Asks Core for the page older than the cursor it last handed back.
///
/// A record row carries no action in this version, so confirming one is a
/// deliberate no-op. A second query while one is in flight is refused locally
/// and nothing is sent: two pages racing one uncorrelated `AuditPageLoaded`
/// event could not be attributed honestly.
fn load_older_audit_page<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
) -> Result<(), TuiClientError> {
    let Some(panel) = state.ui.audit.as_ref() else {
        return Ok(());
    };
    if !panel.selected_is_load_older() {
        return Ok(());
    }
    if !panel.can_load_older() {
        state
            .ui
            .audit
            .as_mut()
            .expect("panel checked above")
            .refuse_second_query();
        return Ok(());
    }
    let command_id = driver.send(RuntimeCommand::QueryAudit {
        query: panel.next_query(),
    })?;
    state
        .ui
        .audit
        .as_mut()
        .expect("panel checked above")
        .begin(command_id);
    Ok(())
}

fn close_supervision_decision(state: &mut TuiState) {
    // Rule (b): closing the overlay while the last decision is settled clears
    // the echo. Pending is never auto-reset — the command is still in flight.
    state.supervision.reset_if_settled();
    state.ui.supervision = None;
    state.ui.overlay = None;
}

/// Owns every key while the supervision decision overlay is focused.
///
/// Ordering matters: this runs *after* `apply_pending_approval_intent`, so a
/// pinned approval always wins. Within the overlay, `Esc` unwinds in order —
/// first the reason line, then the overlay itself — and printable characters
/// that are not an action number keep editing the composer, so a streaming turn
/// stays answerable while a decision is open.
fn apply_supervision_decision_intent<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
    intent: &InputIntent,
) -> Result<Option<UiEventOutcome>, TuiError> {
    if !state
        .ui
        .overlay
        .as_ref()
        .is_some_and(|overlay| overlay.kind == OverlayKind::SupervisionDecision)
        || state.ui.supervision.is_none()
    {
        return Ok(None);
    }
    let actions = supervision_actions(state);
    match intent {
        InputIntent::CloseOverlay => {
            let panel = state.ui.supervision.as_mut().expect("panel checked above");
            if panel.input.take().is_some() {
                panel.notice = None;
            } else {
                close_supervision_decision(state);
            }
            Ok(Some(UiEventOutcome::Redraw))
        }
        InputIntent::MoveSelection(delta) => {
            let panel = state.ui.supervision.as_mut().expect("panel checked above");
            if panel.input.is_none() {
                panel.focus = move_focus(panel.focus, *delta, actions.len());
            }
            Ok(Some(UiEventOutcome::Redraw))
        }
        InputIntent::InsertChar(value) => {
            let panel = state.ui.supervision.as_mut().expect("panel checked above");
            if let Some(input) = panel.input.as_mut() {
                input.text.push(*value);
                panel.notice = None;
            } else if let Some(index) = value.to_digit(10).and_then(|digit| {
                (digit >= 1 && (digit as usize) <= actions.len()).then_some(digit as usize - 1)
            }) {
                panel.focus = index;
                panel.notice = None;
            } else {
                // This overlay has no text filter, so anything that is not an
                // action number belongs to the composer.
                push_composer_char(state, *value);
            }
            Ok(Some(UiEventOutcome::Redraw))
        }
        InputIntent::Backspace => {
            let panel = state.ui.supervision.as_mut().expect("panel checked above");
            if let Some(input) = panel.input.as_mut() {
                input.text.pop();
                panel.notice = None;
            } else {
                state.ui.input.backspace();
                reset_for_input_change(state);
            }
            Ok(Some(UiEventOutcome::Redraw))
        }
        InputIntent::CompleteSelection | InputIntent::CompleteOrSubmit => {
            confirm_supervision_action(driver, state, &actions)?;
            Ok(Some(UiEventOutcome::Redraw))
        }
        _ => Ok(None),
    }
}

fn supervision_actions(state: &TuiState) -> Vec<SupervisionAction> {
    state
        .ui
        .supervision
        .as_ref()
        .map_or_else(Vec::new, |panel| {
            overlay_actions(
                &state.runtime,
                &panel.target,
                state.supervision.pending().is_some(),
            )
        })
}

fn move_focus(focus: usize, delta: i8, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    if delta < 0 {
        focus.saturating_sub(1)
    } else {
        focus.saturating_add(1).min(len - 1)
    }
}

/// Confirms the focused action: open its text line, or build and send it.
fn confirm_supervision_action<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
    actions: &[SupervisionAction],
) -> Result<(), TuiError> {
    let Some(panel) = state.ui.supervision.as_ref() else {
        return Ok(());
    };
    let Some(action) = panel
        .input
        .as_ref()
        .map(|input| input.action)
        .or_else(|| actions.get(panel.focus).copied())
    else {
        return Ok(());
    };
    let text = panel
        .input
        .as_ref()
        .map(|input| input.text.clone())
        .unwrap_or_default();
    let target = panel.target.clone();
    let awaiting_text = panel.input.is_none() && action.text_requirement() != TextRequirement::None;

    if action == SupervisionAction::ConflictDetail {
        // A read, not a decision: it leaves any in-flight supervision command
        // exactly as it was and settles nothing.
        open_conflict_detail(
            state,
            ConflictDetailTarget::Bounce {
                gate_id: match &target {
                    SupervisionTarget::Gate { gate_id } | SupervisionTarget::Bounce { gate_id } => {
                        gate_id.clone()
                    }
                    SupervisionTarget::Review { .. } => return Ok(()),
                },
            },
        );
        return Ok(());
    }
    if action == SupervisionAction::EvidenceRead {
        // A read, not a decision: it opens the inspector scoped to this
        // record's own Core-published owner and leaves any in-flight
        // supervision command exactly as it was.
        if !driver.has_capability(EVIDENCE_READS_CAPABILITY) {
            set_supervision_panel(state, None, Some("supervision.evidence.unavailable"));
            return Ok(());
        }
        let Some(scope) = evidence_scope(&state.runtime, &target) else {
            set_supervision_panel(state, None, Some("supervision.error.record_missing"));
            return Ok(());
        };
        open_evidence_inspector(driver, state, Some(scope))?;
        return Ok(());
    }
    if action == SupervisionAction::AuditTrail {
        // A read, not a decision: it opens the timeline scoped to this record
        // and leaves any in-flight supervision command exactly as it was.
        open_audit_timeline(driver, state, Some(audit_scope(&target)))?;
        return Ok(());
    }
    if action == SupervisionAction::Dismiss {
        // Local escape only: the Core command keeps running and may still land,
        // so this settles nothing and the hint says exactly that.
        state.supervision.abandon();
        set_supervision_panel(state, None, Some("supervision.dismiss.hint"));
        return Ok(());
    }
    if awaiting_text {
        set_supervision_panel(
            state,
            Some(SupervisionInput {
                action,
                text: String::new(),
            }),
            None,
        );
        return Ok(());
    }
    // One correlation at a time. Refusing here rather than after `send` is what
    // makes "nothing was sent" true: a second command would otherwise race the
    // first one's fact through the same ordered stream.
    if state.supervision.pending().is_some() {
        set_supervision_panel(state, None, Some("supervision.pending.busy"));
        return Ok(());
    }
    match build_dispatch(&state.runtime, &target, action, &text) {
        // A local refusal sends nothing and never claims Core decided.
        Err(key) => set_supervision_panel(state, None, Some(key)),
        Ok(dispatch) => {
            // Rule (a): initiating the next decision clears the settled echo of
            // the previous one before this command's own outcome replaces it.
            state.supervision.reset_if_settled();
            let command_id = driver.send_for_owner(dispatch.owner, dispatch.command)?;
            state
                .supervision
                .begin(command_id, dispatch.expect)
                .expect("no supervision command is pending; checked above");
            set_supervision_panel(state, None, None);
        }
    }
    Ok(())
}

fn set_supervision_panel(
    state: &mut TuiState,
    input: Option<SupervisionInput>,
    notice: Option<&str>,
) {
    if let Some(panel) = state.ui.supervision.as_mut() {
        panel.input = input;
        panel.notice = notice.map(str::to_string);
    }
}

fn submit_composer<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
) -> Result<(), TuiError> {
    let content = state.ui.input.as_str().trim().to_string();
    if content.is_empty()
        || open_local_lens_command(driver, &content, state)?
        || open_local_picker_command(driver, &content, state)?
    {
        return Ok(());
    }
    state.ui.entries.push(TuiEntry {
        label: "user".to_string(),
        body: content.clone(),
    });
    if let Some(FocusedConversation::AcpSession(session_id)) =
        state.ui.focused_conversation.as_ref()
        && driver.has_capability("runtime.agent_session_input")
        && let Some(session) = state
            .runtime
            .agent_sessions
            .iter()
            .find(|session| &session.session_id == session_id)
    {
        driver.send_for_owner(
            session.owner.clone(),
            RuntimeCommand::SendAgentSessionInput {
                input: viden_core::AgentSessionInput {
                    session_id: session_id.clone(),
                    content,
                },
            },
        )?;
    } else {
        // Liveness is Core's answer, not this client's dispatch: Core
        // publishes `TurnStarted` immediately after the `CommandAccepted` for
        // this command and `TurnFinished` on every exit
        // (`runtime.turn_lifecycle`), so nothing is recorded locally about the
        // turn that was just asked for.
        dispatch_intent(driver, command_for_composer(state, &content))?;
    }
    state.ui.lens = Lens::Session;
    state.ui.input.clear();
    reset_for_input_change(state);
    Ok(())
}

fn open_local_lens_command<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    input: &str,
    state: &mut TuiState,
) -> Result<bool, TuiClientError> {
    match input.trim() {
        "/setup" => {
            state.ui.lens = Lens::Setup;
            state.ui.overlay = None;
            state.ui.interaction_panel = Some(InteractionPanel::Setup {
                selected: 0,
                draft: default_project_config_draft(&state.runtime),
            });
            if driver.has_capability(PROJECT_ONBOARDING_CAPABILITY) {
                driver.send(RuntimeCommand::ProbeProject)?;
            } else {
                state.ui.entries.push(TuiEntry {
                    label: "system".to_string(),
                    body: super::i18n::text(state, "interaction.setup.unavailable"),
                });
            }
        }
        "/lanes" | "/board" => {
            state.ui.lens = Lens::Board;
            state.ui.overlay = Some(OverlayState::new(OverlayKind::Lane));
            state.ui.interaction_panel = None;
        }
        "/decisions" => {
            state.ui.lens = Lens::Decisions;
            state.ui.overlay = Some(OverlayState::new(OverlayKind::Decisions));
            state.ui.interaction_panel = None;
        }
        "/gallery" => {
            state.ui.lens = Lens::Gallery;
            state.ui.overlay = None;
            state.ui.interaction_panel = None;
        }
        "/settings" => {
            state.ui.overlay = None;
            state.ui.interaction_panel = Some(InteractionPanel::Settings(Box::new(
                SettingsPanel::new(&state.runtime.snapshot.ui_preferences, state.ui.color_depth),
            )));
        }
        _ => return Ok(false),
    }
    state.ui.input.clear();
    reset_for_input_change(state);
    Ok(true)
}

fn default_project_config_draft(runtime: &RuntimeViewState) -> String {
    let name = runtime
        .project_probe
        .as_ref()
        .and_then(|probe| probe.project_name.as_deref())
        .unwrap_or_default();
    let pack = runtime
        .project_probe
        .as_ref()
        .and_then(|probe| probe.pack.as_deref())
        .unwrap_or_default();
    format!(
        "[project]\nname = \"{}\"\npack = \"{}\"\n",
        escape_toml_string(name),
        escape_toml_string(pack)
    )
}

fn escape_toml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn complete_overlay_selection<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
    overlay: OverlayState,
) -> Result<(), TuiClientError> {
    match overlay.kind {
        OverlayKind::GlobalJump => {}
        OverlayKind::Lane | OverlayKind::Board => {
            let needle = overlay.filter.to_ascii_lowercase();
            let selected = state
                .runtime
                .lanes
                .iter()
                .filter(|lane| {
                    needle.is_empty()
                        || format!("{} {} {:?}", lane.id, lane.role, lane.status)
                            .to_ascii_lowercase()
                            .contains(&needle)
                })
                .nth(overlay.selected)
                .map(|lane| (lane.id.clone(), lane.active_session_ids.clone()));
            if let Some((lane_id, session_ids)) = selected {
                state.ui.focus_lane(lane_id);
                match session_ids.as_slice() {
                    [] => {
                        state.ui.session_id.clear();
                        state.ui.lens = Lens::Board;
                    }
                    [session_id] => {
                        state.ui.session_id = session_id.clone();
                        state.ui.lens = Lens::Session;
                    }
                    _ => {
                        state.ui.session_id.clear();
                        state.ui.overlay = Some(OverlayState::new(OverlayKind::Session));
                    }
                }
            }
        }
        OverlayKind::Session => {
            let selected = state
                .ui
                .focused_lane
                .as_ref()
                .and_then(|lane_id| state.runtime.lanes.iter().find(|lane| &lane.id == lane_id))
                .and_then(|lane| lane.active_session_ids.get(overlay.selected));
            if let Some(session_id) = selected {
                state.ui.session_id = session_id.clone();
                state.ui.lens = Lens::Session;
            }
        }
        OverlayKind::Decisions => {
            state.ui.lens = Lens::Decisions;
            // Picks are indexed against the same ordered list the rows render,
            // so the selection always names the record the operator saw.
            match decision_picks(&state.runtime, state.supervision.pending().is_some())
                .into_iter()
                .nth(overlay.selected)
            {
                Some(DecisionPick::Approval { request_id }) => {
                    let mut approval_overlay = OverlayState::new(OverlayKind::Approval);
                    approval_overlay.selected_id = Some(request_id);
                    state.ui.approval_focus = DEFAULT_APPROVAL_FOCUS;
                    state.ui.overlay = Some(approval_overlay);
                }
                Some(DecisionPick::Supervision(target)) => open_supervision_decision(state, target),
                Some(DecisionPick::DismissSupervision) => {
                    // Local escape only: Core still owns the command.
                    state.supervision.abandon();
                    state.ui.overlay = Some(OverlayState::new(OverlayKind::Decisions));
                }
                // Unscoped: Core's audit store is already scoped to this
                // project's own workflow directory, so an invented project or
                // lane filter could only narrow the timeline dishonestly.
                Some(DecisionPick::AuditTimeline) => open_audit_timeline(driver, state, None)?,
                None => {}
            }
        }
        OverlayKind::Approval
        | OverlayKind::SupervisionDecision
        | OverlayKind::AuditTimeline
        | OverlayKind::EvidenceInspector
        | OverlayKind::ConflictContent => {}
        OverlayKind::CommandPalette
        | OverlayKind::NewSession
        | OverlayKind::ContextHelp
        | OverlayKind::ExitConfirm
        | OverlayKind::InteractionPanel
        | OverlayKind::ComposerCommands => {}
    }
    Ok(())
}

fn complete_global_jump_selection(state: &mut TuiState, overlay: OverlayState) {
    let index = JumpIndex::from_state(state);
    let results = index.search(&overlay.filter);
    let Some(item) = results.get(overlay.selected).map(|item| (*item).clone()) else {
        state.ui.overlay = overlay.previous_overlay.map(|previous| *previous);
        return;
    };
    if !item.enabled {
        state.ui.overlay = Some(overlay);
        return;
    }
    match item.kind {
        JumpKind::Lane => select_jump_lane(state, &item),
        JumpKind::Session => {
            match item.parent_id {
                Some(lane_id) => state.ui.focus_lane(lane_id),
                None => state.ui.clear_lane_focus(),
            }
            state.ui.session_id = item.id;
            state.ui.lens = Lens::Session;
        }
        JumpKind::Gate => state.ui.lens = Lens::Decisions,
        JumpKind::Ask => {
            state.ui.lens = Lens::Decisions;
            let mut approval = OverlayState::new(OverlayKind::Approval);
            approval.selected_id = Some(item.id);
            state.ui.approval_focus = DEFAULT_APPROVAL_FOCUS;
            state.ui.overlay = Some(approval);
        }
        JumpKind::Command => {
            state.ui.input.replace(item.id);
            reset_for_input_change(state);
        }
        JumpKind::File => unreachable!("unavailable file inventory cannot activate"),
    }
}

fn select_jump_lane(state: &mut TuiState, item: &JumpItem) {
    let sessions = state
        .runtime
        .lanes
        .iter()
        .find(|lane| lane.id == item.id)
        .map(|lane| lane.active_session_ids.clone())
        .unwrap_or_default();
    state.ui.focus_lane(item.id.clone());
    match sessions.as_slice() {
        [session] => {
            state.ui.session_id = session.clone();
            state.ui.lens = Lens::Session;
        }
        _ => {
            state.ui.session_id.clear();
            state.ui.lens = Lens::Board;
        }
    }
}

fn open_local_picker_command<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    input: &str,
    state: &mut TuiState,
) -> Result<bool, TuiClientError> {
    state.ui.interaction_panel = match input.trim() {
        "/connect" | "/provider" | "/settings provider" | "/setup provider" => {
            Some(InteractionPanel::ConnectProvider {
                search: String::new(),
                selected: 0,
            })
        }
        "/models" | "/model" | "/settings model" | "/setup model" => {
            Some(InteractionPanel::ModelPicker {
                provider_id: None,
                search: String::new(),
                selected: 0,
            })
        }
        "/acp" => {
            if state.ui.focused_lane.is_some() && driver.has_capability(AGENT_ADAPTERS_CAPABILITY) {
                driver.send(RuntimeCommand::QueryAgentAdapters)?;
            }
            Some(InteractionPanel::AcpPicker {
                selected: 0,
                phase: AcpPickerPhase::Browse,
            })
        }
        // Selector-first, exactly like `/acp`: opening the picker sends no
        // command and takes no lock. The rows stay listed without the
        // capability, so an operator can see the surface and why it is inert.
        "/git" => Some(InteractionPanel::GitPicker {
            selected: 0,
            phase: GitPickerPhase::Browse,
        }),
        // `/evidence` is not a picker: it is the read itself, scoped to the
        // focused Lane when there is one. Without the capability nothing is
        // sent and the gap is stated, because an inspector that can never be
        // answered is worse than a named absence.
        "/evidence" => {
            if driver.has_capability(EVIDENCE_READS_CAPABILITY) {
                let scope = focused_lane_evidence_scope(state);
                open_evidence_inspector(driver, state, scope)?;
            } else {
                refuse_evidence_read(state);
            }
            state.ui.input.clear();
            reset_for_input_change(state);
            return Ok(true);
        }
        _ => return Ok(false),
    };
    state.ui.input.clear();
    reset_for_input_change(state);
    Ok(true)
}

/// Picks one `/git` row: open the commit prompt, or send exactly one action.
///
/// Nothing here decides anything. The client sends `RunOperatorGitAction` and
/// waits for Core's ordered answer; it never runs `git`, never falls back to a
/// shell, and never treats the acceptance receipt as an outcome.
fn apply_git_picker_selection<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
    selected: usize,
    phase: &GitPickerPhase,
) -> Result<(), TuiClientError> {
    let action = match phase {
        GitPickerPhase::Browse => match git_picker_rows(state).get(selected).map(|row| &row.kind) {
            Some(GitPickerRowKind::Send(action)) => action.clone(),
            Some(GitPickerRowKind::Commit) => {
                state.ui.interaction_panel = Some(InteractionPanel::GitPicker {
                    selected,
                    phase: GitPickerPhase::CommitMessage {
                        draft: String::new(),
                    },
                });
                return Ok(());
            }
            Some(GitPickerRowKind::Dismiss) => {
                // Local escape only: Core still owns the command and may still
                // apply it, so this settles nothing and composes no outcome.
                state.operator_git.abandon();
                state.ui.entries.push(TuiEntry {
                    label: "system".to_string(),
                    body: super::i18n::text(state, "git.dismiss.hint"),
                });
                state.ui.interaction_panel = None;
                return Ok(());
            }
            Some(GitPickerRowKind::Disabled) => {
                // A row disabled for a missing capability has already said so
                // in its own label, and there is no further fact to state. A
                // row disabled because Core published no owner for this target
                // states that as a transcript fact: the audit record such an
                // action would write is the reason it cannot be sent, and an
                // operator has to be told which fact is missing.
                if driver.has_capability(OPERATOR_GIT_CAPABILITY)
                    && let Err(refusal) = operator_git_owner(state, &git_target(state))
                {
                    state.ui.entries.push(TuiEntry {
                        label: "system".to_string(),
                        body: refusal.message(state),
                    });
                    state.ui.interaction_panel = None;
                }
                return Ok(());
            }
            None => return Ok(()),
        },
        GitPickerPhase::CommitMessage { draft } => {
            let message = draft.trim();
            // Core refuses an empty message and an empty `git commit` would
            // open an editor in a non-interactive child. Refusing here means
            // nothing is sent, which is a different fact from "Core said no".
            if message.is_empty() {
                return Ok(());
            }
            viden_core::OperatorGitAction::Commit {
                message: message.to_string(),
            }
        }
    };
    // One action at a time: a second correlation against the same ordered
    // stream could not be attributed honestly, so it is refused before `send`
    // and "nothing was sent" stays true.
    if let Some(pending) = state.operator_git.pending() {
        state.ui.entries.push(TuiEntry {
            label: "system".to_string(),
            body: super::i18n::translate(
                state,
                "git.busy",
                &[("command_id", &pending.command_id.clone())],
            ),
        });
        state.ui.interaction_panel = None;
        return Ok(());
    }
    // Gated at the point of *sending*, not at the point of entry: the dismiss
    // row above must stay usable even if the capability disappeared while an
    // action was in flight, or the slot would be stranded forever.
    if !driver.has_capability(OPERATOR_GIT_CAPABILITY) {
        return Ok(());
    }
    let target = git_target(state);
    // Refused locally, before `send`: `RunOperatorGitAction` is an audited
    // mutation whose `owner` is the actor Core records, so an owner Core has
    // not published cannot be substituted with `RuntimeOwner::default()` —
    // that would file an authorized source-control change as belonging to
    // nobody. "Nothing was sent" stays true.
    let owner = match operator_git_owner(state, &target) {
        Ok(owner) => owner,
        Err(refusal) => {
            state.ui.entries.push(TuiEntry {
                label: "system".to_string(),
                body: refusal.message(state),
            });
            state.ui.interaction_panel = None;
            return Ok(());
        }
    };
    let command_id = driver.send_for_owner(
        owner.clone(),
        RuntimeCommand::RunOperatorGitAction {
            owner,
            target: target.clone(),
            action: action.clone(),
        },
    )?;
    state
        .operator_git
        .begin(command_id, target, action)
        .expect("no operator git action is pending; checked above");
    state.ui.interaction_panel = None;
    Ok(())
}

/// Renders one settled operator action as a typed system transcript entry.
///
/// Every fact comes from the typed outcome or from the resampled
/// `WorkspaceSourceView` beside it. `Completed.output` is folded to a line
/// count and its first line — display text that is rendered, never parsed.
fn operator_git_entry(state: &TuiState, settlement: &OperatorGitSettlement) -> TuiEntry {
    let body = match settlement {
        // Core's own reason, verbatim: only Core knows which rule refused, and
        // a locally composed sentence would be this client inventing a policy.
        OperatorGitSettlement::Rejected { reason } => {
            super::i18n::translate(state, "git.outcome.rejected", &[("reason", reason)])
        }
        OperatorGitSettlement::Finished {
            action,
            outcome,
            audit_id,
        } => {
            let verb = super::i18n::text(state, action_label_key(action));
            let detail = match outcome.as_ref() {
                viden_core::OperatorGitOutcome::Completed {
                    output,
                    truncated,
                    source,
                } => {
                    let (lines, first) = collapsed_output(output);
                    let truncated_note = if *truncated {
                        super::i18n::text(state, "git.output.truncated")
                    } else {
                        String::new()
                    };
                    super::i18n::translate(
                        state,
                        "git.outcome.completed",
                        &[
                            ("action", &verb),
                            (
                                "branch",
                                source
                                    .branch
                                    .as_deref()
                                    .unwrap_or(&super::i18n::text(state, "git.source.no_branch")),
                            ),
                            ("ahead", &source.ahead.to_string()),
                            ("behind", &source.behind.to_string()),
                            (
                                "dirty",
                                &super::i18n::text(
                                    state,
                                    if source.dirty {
                                        "git.source.dirty"
                                    } else {
                                        "git.source.clean"
                                    },
                                ),
                            ),
                            ("lines", &lines.to_string()),
                            ("output", &first),
                            ("truncated", &truncated_note),
                        ],
                    )
                }
                viden_core::OperatorGitOutcome::Failed { class, detail } => {
                    let (message_key, recovery_key) = failure_copy(*class);
                    super::i18n::translate(
                        state,
                        "git.outcome.failed",
                        &[
                            ("action", &verb),
                            ("message", &super::i18n::text(state, message_key)),
                            ("recovery", &super::i18n::text(state, recovery_key)),
                            ("detail", detail),
                        ],
                    )
                }
                // `#[non_exhaustive]`: an outcome this build cannot read is
                // named as unknown rather than reported as a success.
                _ => super::i18n::translate(state, "git.outcome.unknown", &[("action", &verb)]),
            };
            format!(
                "{detail}\n{}",
                super::i18n::translate(state, "git.outcome.audit", &[("audit_id", audit_id)])
            )
        }
    };
    TuiEntry {
        label: "system".to_string(),
        body,
    }
}

/// Deterministic previews render the same settled entries the event loop does,
/// so the evidence cannot drift from the production rendering.
pub(super) fn operator_git_entry_for_preview(
    state: &TuiState,
    settlement: &OperatorGitSettlement,
) -> TuiEntry {
    operator_git_entry(state, settlement)
}

fn move_interaction_selection(state: &mut TuiState, delta: i8) {
    let count = interaction_panel_choice_count(state);
    if count == 0 {
        set_interaction_panel_selected(state, 0);
        return;
    }
    let current = interaction_selected(state).min(count.saturating_sub(1));
    let next = if delta < 0 {
        current.saturating_sub(1)
    } else {
        (current + 1).min(count - 1)
    };
    set_interaction_panel_selected(state, next);
}

fn interaction_selected(state: &TuiState) -> usize {
    match state.ui.interaction_panel.as_ref() {
        Some(InteractionPanel::Settings(panel)) => panel.selected,
        Some(InteractionPanel::Setup { selected, .. })
        | Some(InteractionPanel::ConnectProvider { selected, .. })
        | Some(InteractionPanel::ProviderConfig { selected, .. })
        | Some(InteractionPanel::ModelPicker { selected, .. })
        | Some(InteractionPanel::AcpPicker { selected, .. })
        | Some(InteractionPanel::GitPicker { selected, .. }) => *selected,
        Some(InteractionPanel::NewLaneTask { .. }) => 0,
        _ => 0,
    }
}

fn cycle_agent_focus(state: &mut TuiState) {
    if state.runtime.lanes.is_empty() {
        state.ui.clear_lane_focus();
        return;
    }
    let next = state
        .ui
        .focused_lane
        .as_deref()
        .and_then(|focused| {
            state
                .runtime
                .lanes
                .iter()
                .position(|lane| lane.id == focused)
        })
        .map(|index| (index + 1) % state.runtime.lanes.len())
        .unwrap_or(0);
    state.ui.focus_lane(state.runtime.lanes[next].id.clone());
}

fn set_interaction_panel_selected(state: &mut TuiState, index: usize) {
    match state.ui.interaction_panel.as_mut() {
        Some(InteractionPanel::Settings(panel)) => panel.selected = index,
        Some(InteractionPanel::Setup { selected, .. })
        | Some(InteractionPanel::ConnectProvider { selected, .. })
        | Some(InteractionPanel::ProviderConfig { selected, .. })
        | Some(InteractionPanel::ModelPicker { selected, .. })
        | Some(InteractionPanel::AcpPicker { selected, .. })
        | Some(InteractionPanel::GitPicker { selected, .. }) => *selected = index,
        Some(InteractionPanel::NewLaneTask { .. }) => {}
        _ => {}
    }
}

fn edit_interaction_panel_text(state: &mut TuiState, value: Option<char>) {
    match state.ui.interaction_panel.as_mut() {
        Some(InteractionPanel::Settings(_)) => {}
        Some(InteractionPanel::ConnectProvider { search, selected })
        | Some(InteractionPanel::ModelPicker {
            search, selected, ..
        }) => {
            match value {
                Some(value) => search.push(value),
                None => {
                    search.pop();
                }
            }
            *selected = 0;
        }
        Some(InteractionPanel::Setup { draft, .. }) => match value {
            Some(value) => draft.push(value),
            None => {
                draft.pop();
            }
        },
        Some(InteractionPanel::AcpPicker { phase, .. }) => {
            if let AcpPickerPhase::TaskEntry { draft, .. } = phase {
                match value {
                    Some(value) => draft.push(value),
                    None => {
                        draft.pop();
                    }
                }
            }
        }
        Some(InteractionPanel::GitPicker { phase, .. }) => {
            if let GitPickerPhase::CommitMessage { draft } = phase {
                match value {
                    Some(value) => draft.push(value),
                    None => {
                        draft.pop();
                    }
                }
            }
        }
        Some(InteractionPanel::NewLaneTask { task }) => match value {
            Some(value) => task.push(value),
            None => {
                task.pop();
            }
        },
        Some(InteractionPanel::ProviderConfig { .. }) | None => {}
    }
}

fn apply_interaction_panel_selection<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
) -> Result<bool, TuiClientError> {
    if matches!(
        state.ui.interaction_panel,
        Some(InteractionPanel::Settings(_))
    ) {
        apply_settings_selection(driver, state)?;
        return Ok(false);
    }
    if let Some(InteractionPanel::Setup { selected, draft }) = state.ui.interaction_panel.as_ref() {
        if !driver.has_capability(PROJECT_ONBOARDING_CAPABILITY) {
            return Ok(false);
        }
        let selected = *selected;
        let command = match selected {
            0 => Some(RuntimeCommand::ProbeProject),
            1 => Some(RuntimeCommand::PreviewProjectConfig {
                contents: draft.clone(),
            }),
            2 => state
                .runtime
                .project_config_preview
                .as_ref()
                .and_then(|preview| {
                    (preview.is_valid()
                        && preview.exact_contents.as_deref() == Some(draft.as_str()))
                    .then(|| RuntimeCommand::ConfirmProjectConfig {
                        preview_id: preview.preview_id.clone(),
                        content_sha256: preview.content_sha256.clone(),
                    })
                }),
            _ => None,
        };
        if let Some(command) = command {
            driver.send(command)?;
        }
        return Ok(false);
    }
    if let Some(InteractionPanel::AcpPicker { selected, phase }) =
        state.ui.interaction_panel.clone()
    {
        match phase {
            AcpPickerPhase::Browse => {
                let Some(row) = acp_picker_rows(state).get(selected).cloned() else {
                    return Ok(false);
                };
                match row.kind {
                    AcpPickerRowKind::Session { session_id } => {
                        match state
                            .runtime
                            .agent_sessions
                            .iter()
                            .find(|session| session.session_id == session_id)
                            .map(|session| session.lane_id.clone())
                        {
                            Some(lane_id) => state.ui.focus_lane(lane_id),
                            None => state.ui.clear_lane_focus(),
                        }
                        state.ui.session_id = session_id.clone();
                        state.ui.focused_conversation =
                            Some(FocusedConversation::AcpSession(session_id));
                        state.ui.lens = Lens::Session;
                        state.ui.interaction_panel = None;
                    }
                    AcpPickerRowKind::Adapter {
                        agent_id,
                        startability: AgentStartability::Ready,
                    } => {
                        state.ui.interaction_panel = Some(InteractionPanel::AcpPicker {
                            selected: 0,
                            phase: AcpPickerPhase::TaskEntry {
                                agent_id,
                                draft: String::new(),
                            },
                        });
                    }
                    AcpPickerRowKind::Adapter {
                        agent_id,
                        startability: AgentStartability::ProbeRequired,
                    } => {
                        driver.send(RuntimeCommand::ProbeAgentAdapter { agent_id })?;
                    }
                    AcpPickerRowKind::Adapter { agent_id, .. } => {
                        state.ui.entries.push(TuiEntry {
                            label: "system".to_string(),
                            body: super::i18n::translate(
                                state,
                                "acp.not_startable",
                                &[("agent", &agent_id)],
                            ),
                        });
                    }
                    AcpPickerRowKind::Disabled => {}
                }
            }
            AcpPickerPhase::TaskEntry { agent_id, draft } => {
                let task = draft.trim();
                let Some(lane_id) = state.ui.focused_lane.clone() else {
                    return Ok(false);
                };
                if task.is_empty() || !driver.has_capability(AGENT_SESSIONS_CAPABILITY) {
                    return Ok(false);
                }
                driver.send_for_owner(
                    RuntimeOwner {
                        lane_id: Some(lane_id.clone()),
                        ..RuntimeOwner::default()
                    },
                    RuntimeCommand::StartAgentSession {
                        request: AgentSessionRequest {
                            lane_id: lane_id.clone(),
                            agent_id: agent_id.clone(),
                            model: None,
                            load_session_id: None,
                            task: task.to_string(),
                        },
                    },
                )?;
                state.ui.pending_acp_start = Some(PendingAcpStart {
                    lane_id: lane_id.clone(),
                    agent_id: agent_id.clone(),
                });
                state.ui.interaction_panel = None;
                state.ui.lens = Lens::Session;
            }
        }
        return Ok(false);
    }
    if let Some(InteractionPanel::GitPicker { selected, phase }) =
        state.ui.interaction_panel.clone()
    {
        apply_git_picker_selection(driver, state, selected, &phase)?;
        return Ok(false);
    }
    if let Some(InteractionPanel::NewLaneTask { task }) = state.ui.interaction_panel.clone() {
        if task.trim().is_empty()
            || !driver.has_capability(WORKSPACE_ELIGIBILITY_CAPABILITY)
            || !state
                .runtime
                .workspace_eligibility
                .as_ref()
                .is_some_and(|eligibility| eligibility.can_create_lane)
        {
            return Ok(false);
        }
        driver.send(RuntimeCommand::PreviewDefaultStarterLane {
            preset: StarterLanePreset::Coder,
        })?;
        state.ui.pending_native_lane = Some(PendingNativeLane::AwaitingPreview {
            task: task.trim().to_string(),
        });
        state.ui.interaction_panel = None;
        return Ok(false);
    }
    let command = selected_interaction_command(state);
    state.ui.interaction_panel = None;
    if let Some(command) = command {
        // Provider/model activation remains a Core command. The overlay only
        // selects the command; it never mutates provider authority directly.
        state.ui.input.replace(command);
        reset_for_input_change(state);
        Ok(true)
    } else {
        Ok(false)
    }
}

fn apply_settings_selection<C: CoreClient>(
    driver: &mut TuiClientDriver<C>,
    state: &mut TuiState,
) -> Result<(), TuiClientError> {
    if !driver.has_capability(UI_PREFERENCE_PERSISTENCE_CAPABILITY) {
        return Ok(());
    }
    let action = {
        let Some(InteractionPanel::Settings(panel)) = state.ui.interaction_panel.as_mut() else {
            return Ok(());
        };
        if panel.is_pending() {
            return Ok(());
        }
        if let Some(field) = panel.field {
            let choice = panel.choices(field).get(panel.selected).copied();
            if let Some(choice) = choice
                && panel.select(choice.value)
            {
                if field == PreferenceField::ColorDepth {
                    state.ui.color_depth = panel.color_depth();
                }
                panel.field = None;
                panel.selected = settings_field_index(field);
            }
            return Ok(());
        }
        match panel.selected {
            0..=5 => {
                let field = settings_field_at(panel.selected);
                panel.field = Some(field);
                panel.selected = panel
                    .choices(field)
                    .iter()
                    .position(|choice| settings_choice_is_selected(panel, choice.value))
                    .unwrap_or(0);
                return Ok(());
            }
            6 => panel.apply_command(),
            7 => Some(panel.reset_command()),
            _ => None,
        }
    };
    if let Some(command) = action {
        let command_id = driver.send(command.clone())?;
        if let Some(InteractionPanel::Settings(panel)) = state.ui.interaction_panel.as_mut() {
            panel.begin_pending(command_id, command);
        }
    }
    Ok(())
}

fn settings_field_at(index: usize) -> PreferenceField {
    match index {
        0 => PreferenceField::Locale,
        1 => PreferenceField::Skin,
        2 => PreferenceField::Mode,
        3 => PreferenceField::Density,
        4 => PreferenceField::Motion,
        5 => PreferenceField::ColorDepth,
        _ => PreferenceField::Locale,
    }
}

fn settings_field_index(field: PreferenceField) -> usize {
    match field {
        PreferenceField::Locale => 0,
        PreferenceField::Skin => 1,
        PreferenceField::Mode => 2,
        PreferenceField::Density => 3,
        PreferenceField::Motion => 4,
        PreferenceField::ColorDepth => 5,
    }
}

fn settings_choice_is_selected(
    panel: &SettingsPanel,
    value: super::preferences::PreferenceValue,
) -> bool {
    match value {
        super::preferences::PreferenceValue::Locale(value) => panel.selected_locale() == value,
        super::preferences::PreferenceValue::Skin(value) => panel.selected_skin() == value,
        super::preferences::PreferenceValue::Mode(value) => panel.selected_mode() == value,
        super::preferences::PreferenceValue::Density(value) => panel.selected_density() == value,
        super::preferences::PreferenceValue::Motion(value) => panel.selected_motion() == value,
        super::preferences::PreferenceValue::ColorDepth(value) => panel.color_depth() == value,
    }
}

fn close_interaction_panel_or_palette(key: KeyEvent, state: &mut TuiState) {
    if let Some(InteractionPanel::Settings(panel)) = state.ui.interaction_panel.as_mut()
        && let Some(field) = panel.field.take()
    {
        panel.selected = settings_field_index(field);
        return;
    }
    if let Some(InteractionPanel::AcpPicker { selected, phase }) =
        state.ui.interaction_panel.as_mut()
        && matches!(phase, AcpPickerPhase::TaskEntry { .. })
    {
        *selected = 0;
        *phase = AcpPickerPhase::Browse;
        return;
    }
    // Esc unwinds the commit prompt to the action rows before it closes the
    // panel, and discards the draft: nothing was sent, so nothing is pending.
    if let Some(InteractionPanel::GitPicker { selected, phase }) =
        state.ui.interaction_panel.as_mut()
        && matches!(phase, GitPickerPhase::CommitMessage { .. })
    {
        *selected = 1;
        *phase = GitPickerPhase::Browse;
        return;
    }
    if state.ui.interaction_panel.take().is_none() {
        close_on_escape(key, state);
    }
}

fn observe_driver_events<C: CoreClient>(
    state: &mut TuiState,
    driver: &mut TuiClientDriver<C>,
) -> Result<(), TuiClientError> {
    let events = driver.take_applied_events();
    for event in &events {
        // Confirm-on-fact: a supervision decision settles only when Core
        // publishes the business fact it asked for, never on the receipt.
        state.supervision.observe_event(event);
        // The operator source-control slot correlates on this client's own
        // command id, so a `/git` action and a supervision decision never
        // settle each other and neither can block the other.
        if let Some(settlement) = state.operator_git.observe_event(event) {
            let entry = operator_git_entry(state, &settlement);
            state.ui.entries.push(entry);
        }
        // The audit read correlates independently of the supervision slot, so a
        // pending decision can never block a page and a page can never settle a
        // decision. With no overlay open there is no panel and the page is
        // ignored entirely.
        if let Some(panel) = state.ui.audit.as_mut() {
            panel.observe_event(event);
        }
        // Both evidence reads correlate on their own required command ids, so
        // a page or a content answer for another reader is ignored, and neither
        // can settle the other. With no overlay open there is no panel and the
        // answer is dropped entirely.
        if let Some(panel) = state.ui.evidence.as_mut() {
            panel.observe_event(event);
        }
        // The inventory read correlates on the exact required command id, so a
        // page for another reader is ignored whether or not the jump overlay
        // is open.
        state.ui.workspace_files.observe_event(event);
        if let viden_core::RuntimeEventKind::UiPreferencesUpdated {
            resolved,
            diagnostics,
            ..
        } = &event.kind
        {
            state.ui.preference_diagnostics = diagnostics
                .iter()
                .chain(resolved.diagnostics.iter())
                .map(|diagnostic| diagnostic.code.clone())
                .collect();
            state.ui.preference_diagnostics.sort();
            state.ui.preference_diagnostics.dedup();
        }
        if let Some(InteractionPanel::Settings(panel)) = state.ui.interaction_panel.as_mut() {
            panel.observe_event(event);
        }
        // A terminal fact settles Core's unscoped `assistant_stream`, which was
        // the only surface rendering this reply. Copying the settled text into
        // the transcript is what keeps a turn the operator just watched from
        // vanishing the moment it completes. It is gated on this client not
        // already knowing the session as finished, because Core prefixes its
        // whole persisted runtime state to every turn's event batch: without
        // the gate, the first prompt in a workspace with history would replay
        // every old session's reply back into the transcript.
        if let Some((session, status)) = terminal_agent_session(&event.kind)
            && state
                .ui
                .settled_agent_sessions
                .insert(session.session_id.clone())
            && let Some(entry) = settled_session_entry(session, status)
        {
            state.ui.entries.push(entry);
        }
        if let viden_core::RuntimeEventKind::AgentSessionStarted { session } = &event.kind
            && state.ui.pending_acp_start.as_ref().is_some_and(|pending| {
                pending.lane_id == session.lane_id && pending.agent_id == session.agent_id
            })
        {
            state.ui.focus_lane(session.lane_id.clone());
            state.ui.session_id = session.session_id.clone();
            state.ui.focused_conversation =
                Some(FocusedConversation::AcpSession(session.session_id.clone()));
            state.ui.pending_acp_start = None;
            state.ui.lens = Lens::Session;
        }
        match (&event.kind, state.ui.pending_native_lane.clone()) {
            (
                viden_core::RuntimeEventKind::StarterLanePreviewed { preview },
                Some(PendingNativeLane::AwaitingPreview { task }),
            ) => {
                driver.send_for_owner(
                    preview.owner.clone(),
                    RuntimeCommand::CreateStarterLane {
                        request: viden_core::StarterLaneRequest {
                            lane_id: preview.lane.id.clone(),
                            preset: StarterLanePreset::Coder,
                            branch: preview.lane.branch.clone(),
                            worktree_path: None,
                        },
                        preview_id: preview.preview_id.clone(),
                        content_sha256: preview.content_sha256.clone(),
                    },
                )?;
                state.ui.pending_native_lane = Some(PendingNativeLane::AwaitingReceipt {
                    task,
                    preview_id: preview.preview_id.clone(),
                    content_sha256: preview.content_sha256.clone(),
                });
            }
            (
                viden_core::RuntimeEventKind::StarterLaneCreated { receipt },
                Some(PendingNativeLane::AwaitingReceipt {
                    task,
                    preview_id,
                    content_sha256,
                }),
            ) if receipt.preview_id == preview_id && receipt.content_sha256 == content_sha256 => {
                driver.send_for_owner(
                    receipt.owner.clone(),
                    RuntimeCommand::SubmitUserInput {
                        content: task.clone(),
                    },
                )?;
                state.ui.focus_lane(receipt.lane.id.clone());
                state.ui.focused_conversation =
                    Some(FocusedConversation::NativeLane(receipt.lane.id.clone()));
                state.ui.lens = Lens::Session;
                state.ui.pending_native_lane = None;
                state.ui.entries.push(TuiEntry {
                    label: "user".to_string(),
                    body: task,
                });
            }
            _ => {}
        }
    }
    Ok(())
}

fn scroll_transcript(state: &mut TuiState, delta: isize) {
    if delta > 0 {
        state.ui.transcript_scroll = state.ui.transcript_scroll.saturating_add(delta as usize);
    } else {
        state.ui.transcript_scroll = state
            .ui
            .transcript_scroll
            .saturating_sub(delta.unsigned_abs());
    }
}

fn push_composer_char(state: &mut TuiState, value: char) {
    let mut encoded = [0; 4];
    state.ui.input.insert(value.encode_utf8(&mut encoded));
    if looks_like_terminal_escape_residue(state.ui.input.as_str()) {
        state.ui.input.clear();
    }
    reset_for_input_change(state);
}

fn looks_like_terminal_escape_residue(input: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.len() < 6 || !(trimmed.ends_with('m') || trimmed.ends_with('M')) {
        return false;
    }
    let body = trimmed[..trimmed.len() - 1].trim_start_matches(['\u{1b}', '[', '<', '?']);
    let mut parts = body.split(';').collect::<Vec<_>>();
    if parts.len() < 3 || parts.len() > 5 {
        return false;
    }
    if parts[0].is_empty() {
        parts.remove(0);
    }
    parts
        .iter()
        .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
        && parts.len() >= 3
}

/// Routes one composer submission: a new turn, or a follow-up on the running
/// one.
///
/// The question is only ever about *this input's target*, which for a plain
/// composer submission is the driver's own envelope owner — the session, naming
/// no Lane. `state::composer_target_busy` documents the exact facts that answer
/// it and the three that deliberately do not, and mirrors the owner-scoped rule
/// the GUI's composer already applies.
///
/// When neither client nor Core has a fact saying the target is busy, this
/// submits. If Core disagrees it says so: the supervisor refuses a second job
/// for one owner with `CommandRejected`, which is a Core answer rather than a
/// client guess, and the transcript renders it.
fn command_for_composer(state: &TuiState, content: &str) -> RuntimeCommand {
    if composer_target_busy(state) {
        RuntimeCommand::QueueFollowUp {
            content: content.to_string(),
        }
    } else {
        RuntimeCommand::SubmitUserInput {
            content: content.to_string(),
        }
    }
}

#[cfg(test)]
fn approval_command(view: &RuntimeViewState, allow: bool) -> Option<RuntimeCommand> {
    let approval = view.pending_approvals.first()?;
    Some(RuntimeCommand::RespondToApproval {
        request_id: approval.id.clone(),
        response: if allow {
            ApprovalResponse::allow_once(None)
        } else {
            ApprovalResponse::deny(None)
        },
    })
}

fn apply_pump_outcome<C: CoreClient>(
    state: &mut TuiState,
    driver: &TuiClientDriver<C>,
    outcome: PumpOutcome,
) {
    match outcome {
        // A replacement snapshot is a new baseline of settled history, exactly
        // like the startup one. Anything already terminal in it finished
        // outside this client's view of the stream, so it must not arrive in
        // the transcript as a completion the operator watched.
        PumpOutcome::Recovered(_) => {
            // Turn liveness needs no repair here: a replacement snapshot
            // re-lists the turns Core is running now in `active_turns`, so a
            // recovered client reads the same Core fact as a connected one and
            // this client holds no window that could be stranded.
            seed_settled_agent_sessions(state, driver.view());
        }
        PumpOutcome::Idle | PumpOutcome::Applied(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::ui_state::Lens;
    use super::*;
    use std::{
        collections::{BTreeSet, VecDeque},
        path::PathBuf,
        sync::{Arc, Mutex},
        time::Duration,
    };
    use viden_core::{
        CoreClient, CoreClientError, CoreHandshake, CoreTransport, EventCursor,
        RuntimeCommandEnvelope, RuntimeEvent, RuntimeEventEnvelope, RuntimeEventKind,
        RuntimeSnapshotEnvelope, RuntimeViewState, RuntimeWireEvent, StatefulCoreClient,
        frontend_capabilities, local_core_handshake,
    };
    use viden_types::{
        AgentLaneRecord, ApprovalDecision, ApprovalScope, CapabilityId, FRONTEND_SCHEMA_V1,
        LaneStatus, PermissionLevel, PermissionMode, ProjectConfigPreview, ReplayBatch,
        ReplayRequest, RuntimeOwner, RuntimeSnapshot, ToolCallView, TranscriptPage,
        TranscriptPageRequest, UiPreferencePatch, UiPreferences, WorkMode,
    };

    #[derive(Default)]
    struct FakeCoreTransport {
        sent: Vec<RuntimeCommandEnvelope>,
        events: VecDeque<RuntimeEventEnvelope>,
        view: Option<RuntimeViewState>,
        capabilities: Option<BTreeSet<CapabilityId>>,
        snapshot_cursor: Option<EventCursor>,
    }

    impl CoreTransport for FakeCoreTransport {
        fn discover(&mut self) -> Result<CoreHandshake, CoreClientError> {
            let mut handshake = local_core_handshake();
            if let Some(capabilities) = &self.capabilities {
                handshake.capabilities = capabilities.clone();
            }
            Ok(handshake)
        }

        fn send(&mut self, command: RuntimeCommandEnvelope) -> Result<(), CoreClientError> {
            self.sent.push(command);
            Ok(())
        }

        fn recv(
            &mut self,
            _timeout: Duration,
        ) -> Result<Option<RuntimeEventEnvelope>, CoreClientError> {
            Ok(self.events.pop_front())
        }

        fn snapshot(&mut self) -> Result<RuntimeSnapshotEnvelope, CoreClientError> {
            let snapshot = RuntimeSnapshot {
                cwd: PathBuf::from("/workspace"),
                provider_family: "fallback".to_string(),
                model_label: "test-local".to_string(),
                work_mode: WorkMode::Build,
                permission_mode: PermissionMode::Default,
                permission_level: PermissionLevel::Ask,
                config_summary: "fixture".to_string(),
                loaded_config_files: Vec::new(),
                startup_overrides: Vec::new(),
                ui_preferences: Default::default(),
            };
            let view = self
                .view
                .clone()
                .unwrap_or_else(|| RuntimeViewState::new(snapshot.clone()));
            let snapshot = view.snapshot.clone();
            Ok(RuntimeSnapshotEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                capabilities: self
                    .capabilities
                    .clone()
                    .unwrap_or_else(frontend_capabilities),
                cursor: EventCursor {
                    stream_id: self
                        .snapshot_cursor
                        .as_ref()
                        .map_or_else(|| "fixture".to_string(), |cursor| cursor.stream_id.clone()),
                    sequence: self
                        .snapshot_cursor
                        .as_ref()
                        .map_or(0, |cursor| cursor.sequence),
                },
                view,
                snapshot,
            })
        }

        fn replay(&mut self, _request: ReplayRequest) -> Result<ReplayBatch, CoreClientError> {
            Ok(ReplayBatch {
                events: VecDeque::new().into(),
                next: EventCursor {
                    stream_id: "fixture".to_string(),
                    sequence: 0,
                },
                complete: true,
            })
        }

        fn transcript_page(
            &mut self,
            _request: TranscriptPageRequest,
        ) -> Result<TranscriptPage, CoreClientError> {
            Err(CoreClientError::Transport("unused".to_string()))
        }
    }

    #[derive(Default)]
    struct FakeCoreClient {
        transport: FakeCoreTransport,
        sent: Arc<Mutex<Vec<RuntimeCommandEnvelope>>>,
    }

    impl CoreClient for FakeCoreClient {
        fn discover(&mut self) -> Result<CoreHandshake, CoreClientError> {
            CoreTransport::discover(&mut self.transport)
        }

        fn send(&mut self, command: RuntimeCommandEnvelope) -> Result<(), CoreClientError> {
            self.sent.lock().expect("sent commands").push(command);
            Ok(())
        }

        fn recv(
            &mut self,
            timeout: Duration,
        ) -> Result<Option<RuntimeEventEnvelope>, CoreClientError> {
            CoreTransport::recv(&mut self.transport, timeout)
        }

        fn snapshot(&mut self) -> Result<RuntimeSnapshotEnvelope, CoreClientError> {
            CoreTransport::snapshot(&mut self.transport)
        }

        fn replay(&mut self, request: ReplayRequest) -> Result<ReplayBatch, CoreClientError> {
            CoreTransport::replay(&mut self.transport, request)
        }

        fn transcript_page(
            &mut self,
            request: TranscriptPageRequest,
        ) -> Result<TranscriptPage, CoreClientError> {
            CoreTransport::transcript_page(&mut self.transport, request)
        }
    }

    fn pending_approval_driver() -> (
        TuiClientDriver<FakeCoreClient>,
        TuiState,
        Arc<Mutex<Vec<RuntimeCommandEnvelope>>>,
    ) {
        let mut view = RuntimeViewState::new(RuntimeSnapshot {
            cwd: PathBuf::from("/workspace"),
            provider_family: "fallback".to_string(),
            model_label: "test-local".to_string(),
            work_mode: WorkMode::Build,
            permission_mode: PermissionMode::Default,
            permission_level: PermissionLevel::Ask,
            config_summary: "fixture".to_string(),
            loaded_config_files: Vec::new(),
            startup_overrides: Vec::new(),
            ui_preferences: Default::default(),
        });
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/approval-allow-deny.json"
        ))
        .expect("approval fixture");
        let envelope: RuntimeEventEnvelope =
            serde_json::from_value(fixture["events"][0].clone()).expect("approval event");
        if let RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
        let approval = view
            .pending_approvals
            .first_mut()
            .expect("pending approval");
        approval.expires_at = 0;
        approval.allowed_scopes = vec![
            ApprovalScope::Once,
            ApprovalScope::Session {
                session_id: "session-contract".to_string(),
            },
            ApprovalScope::RepoAllowlist {
                paths: vec!["crates/core".to_string()],
            },
        ];
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                view: Some(view),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let sent = Arc::clone(&client.sent);
        let driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::new(driver.view().clone());
        state.ui.input_mode = InputMode::Insert;
        (driver, state, sent)
    }

    fn exact_lane_owner_driver() -> (
        TuiClientDriver<FakeCoreClient>,
        TuiState,
        Arc<Mutex<Vec<RuntimeCommandEnvelope>>>,
        RuntimeOwner,
    ) {
        let mut view = RuntimeViewState::new(RuntimeSnapshot {
            cwd: PathBuf::from("/workspace"),
            provider_family: "fallback".to_string(),
            model_label: "test-local".to_string(),
            work_mode: WorkMode::Build,
            permission_mode: PermissionMode::Default,
            permission_level: PermissionLevel::Ask,
            config_summary: "fixture".to_string(),
            loaded_config_files: Vec::new(),
            startup_overrides: Vec::new(),
            ui_preferences: Default::default(),
        });
        view.lanes = serde_json::from_str::<Vec<AgentLaneRecord>>(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes")
        .into_iter()
        .filter(|lane| lane.id == "L-start")
        .collect();
        let owner = RuntimeOwner {
            workspace_id: "workspace".to_string(),
            project_id: "viden".to_string(),
            lane_id: Some("L-start".to_string()),
            session_id: Some("session-start".to_string()),
            task_id: Some("task_start".to_string()),
            turn_id: Some("turn-start".to_string()),
        };
        view.lane_runtime_owners = vec![viden_types::LaneRuntimeOwnerBinding {
            lane_id: "L-start".to_string(),
            owner: owner.clone(),
        }];
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                view: Some(view),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let sent = Arc::clone(&client.sent);
        let driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::new(driver.view().clone());
        state.ui.focus_lane("L-start".to_string());
        (driver, state, sent, owner)
    }

    fn focus_pending_approval(driver: &mut TuiClientDriver<FakeCoreClient>, state: &mut TuiState) {
        handle_ui_event(
            driver,
            state,
            Event::Key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)),
            (120, 40),
        )
        .expect("open decisions");
        handle_ui_event(
            driver,
            state,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("focus approval");
        assert!(
            state
                .ui
                .overlay
                .as_ref()
                .is_some_and(|overlay| overlay.kind == OverlayKind::Approval)
        );
    }

    #[test]
    fn submit_queue_cancel_and_approval_use_runtime_commands() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");

        let submit_id = dispatch_intent(
            &mut driver,
            RuntimeCommand::SubmitUserInput {
                content: "first".to_string(),
            },
        )
        .expect("submit");
        let queue_id = dispatch_intent(
            &mut driver,
            RuntimeCommand::QueueFollowUp {
                content: "next".to_string(),
            },
        )
        .expect("queue");
        let cancel_id =
            dispatch_intent(&mut driver, RuntimeCommand::CancelActiveTurn).expect("cancel");
        let approval_id = dispatch_intent(
            &mut driver,
            RuntimeCommand::RespondToApproval {
                request_id: "approval-1".to_string(),
                response: ApprovalResponse::allow_once(None),
            },
        )
        .expect("approval");

        assert_eq!(
            [submit_id, queue_id, cancel_id, approval_id],
            ["tui-1", "tui-2", "tui-3", "tui-4"]
        );
        let sent = sent.lock().expect("sent commands");
        assert!(matches!(
            sent[0].command,
            RuntimeCommand::SubmitUserInput { .. }
        ));
        assert!(matches!(
            sent[1].command,
            RuntimeCommand::QueueFollowUp { .. }
        ));
        assert!(matches!(sent[2].command, RuntimeCommand::CancelActiveTurn));
        assert!(matches!(
            sent[3].command,
            RuntimeCommand::RespondToApproval { .. }
        ));
    }

    #[test]
    fn pinned_approval_never_owns_composer_y_n_d_or_enter() {
        let (mut driver, mut state, sent) = pending_approval_driver();
        assert!(
            super::super::modal::approval_focus_cursor(&state, 120, 40, 0).is_none(),
            "a pinned approval must leave terminal cursor ownership with composer"
        );
        let pinned = super::super::render::render_frame(&state, 120, 40);
        assert!(pinned.contains("PINNED · Ctrl-G Decisions"));
        assert!(!pinned.contains("[Deny (n)]"));

        for value in ['y', 'n', 'd'] {
            handle_ui_event(
                &mut driver,
                &mut state,
                Event::Key(KeyEvent::new(KeyCode::Char(value), KeyModifiers::NONE)),
                (120, 40),
            )
            .expect("approval-time composer key");
        }
        assert_eq!(state.ui.input, "ynd");
        assert!(sent.lock().expect("sent commands").is_empty());

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("approval-time composer submit");

        assert!(state.ui.input.is_empty());
        // Moved baseline: this used to expect `QueueFollowUp`. The contract
        // fixture's approval is owner-scoped to `lane_core`, so it is a Lane's
        // decision and not this input's target; routing on it queued a session
        // prompt behind another owner's gate. The subject of the test — the
        // pinned panel never owning `y`/`n`/`d`/Enter — is unchanged, and the
        // composer's text still reaches Core in one command.
        assert!(matches!(
            sent.lock().expect("sent commands").as_slice(),
            [RuntimeCommandEnvelope {
                command: RuntimeCommand::SubmitUserInput { content },
                ..
            }] if content == "ynd"
        ));
    }

    #[test]
    fn explicitly_focused_approval_owns_shortcuts_and_enter() {
        for (key, expected_allowed) in [
            (KeyCode::Char('y'), true),
            (KeyCode::Char('n'), false),
            (KeyCode::Enter, true),
        ] {
            let (mut driver, mut state, sent) = pending_approval_driver();
            focus_pending_approval(&mut driver, &mut state);
            assert!(
                super::super::modal::approval_focus_cursor(&state, 120, 40, 0).is_some(),
                "explicit focus owns the approval cursor"
            );

            handle_ui_event(
                &mut driver,
                &mut state,
                Event::Key(KeyEvent::new(key, KeyModifiers::NONE)),
                (120, 40),
            )
            .expect("focused approval action");

            assert!(matches!(
                sent.lock().expect("sent commands").as_slice(),
                [RuntimeCommandEnvelope {
                    command: RuntimeCommand::RespondToApproval { response, .. },
                    ..
                }] if response.is_allowed() == expected_allowed
            ));
        }

        let (mut driver, mut state, sent) = pending_approval_driver();
        focus_pending_approval(&mut driver, &mut state);
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("focused diff action");
        assert!(sent.lock().expect("sent commands").is_empty());
        assert_eq!(
            super::super::modal::focused_approval_action(&state),
            super::super::modal::ApprovalAction::Diff
        );
    }

    #[test]
    fn focused_approval_sends_exact_scope_through_the_request_owner() {
        for (key, expected_scope) in [
            (
                '2',
                ApprovalScope::Session {
                    session_id: "session-contract".to_string(),
                },
            ),
            (
                '3',
                ApprovalScope::RepoAllowlist {
                    paths: vec!["crates/core".to_string()],
                },
            ),
        ] {
            let (mut driver, mut state, sent) = pending_approval_driver();
            let expected_owner = state.runtime.pending_approvals[0].owner.clone();
            focus_pending_approval(&mut driver, &mut state);

            handle_ui_event(
                &mut driver,
                &mut state,
                Event::Key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE)),
                (120, 40),
            )
            .expect("focused typed approval scope");

            let sent = sent.lock().expect("sent commands");
            assert_eq!(sent.len(), 1);
            assert_eq!(sent[0].owner, expected_owner);
            assert!(matches!(
                &sent[0].command,
                RuntimeCommand::RespondToApproval {
                    response: ApprovalResponse {
                        decision: ApprovalDecision::Allow { scope },
                        ..
                    },
                    ..
                } if scope == &expected_scope
            ));
        }
    }

    #[test]
    fn expired_focused_approval_sends_nothing_until_core_resolves_it() {
        let (mut driver, mut state, sent) = pending_approval_driver();
        state.runtime.pending_approvals[0].expires_at = 1;
        focus_pending_approval(&mut driver, &mut state);

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("expired approval remains inert");

        assert!(sent.lock().expect("sent commands").is_empty());
        assert_eq!(state.runtime.pending_approvals.len(), 1);
    }

    #[test]
    fn setup_and_lanes_routes_change_lens_without_becoming_chat_input() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        state.ui.input = "/setup".into();

        submit_composer(&mut driver, &mut state).expect("open setup");

        assert_eq!(state.ui.lens, Lens::Setup);
        assert!(state.ui.entries.is_empty());
        assert!(matches!(
            sent.lock().expect("sent commands").as_slice(),
            [RuntimeCommandEnvelope {
                command: RuntimeCommand::ProbeProject,
                ..
            }]
        ));

        sent.lock().expect("sent commands").clear();
        state.ui.input = "/lanes".into();
        submit_composer(&mut driver, &mut state).expect("open lanes");

        assert_eq!(state.ui.lens, Lens::Board);
        assert!(
            state
                .ui
                .overlay
                .as_ref()
                .is_some_and(|overlay| overlay.kind == OverlayKind::Lane)
        );
        assert!(sent.lock().expect("sent commands").is_empty());
    }

    #[test]
    fn acp_command_queries_core_and_opens_picker_for_selected_lane() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        state.ui.focus_lane("lane-1".to_string());
        state.ui.input = "/acp".into();

        submit_composer(&mut driver, &mut state).expect("open ACP picker");

        assert!(matches!(
            state.ui.interaction_panel,
            Some(InteractionPanel::AcpPicker {
                phase: AcpPickerPhase::Browse,
                ..
            })
        ));
        assert!(matches!(
            sent.lock().expect("sent commands").as_slice(),
            [RuntimeCommandEnvelope {
                command: RuntimeCommand::QueryAgentAdapters,
                ..
            }]
        ));
    }

    /// `/git` mirrors `/acp`: opening the picker is local and sends nothing,
    /// and picking a row sends exactly one `RunOperatorGitAction` whose command
    /// `owner` equals its envelope owner — the equality Core's supervisor
    /// requires — and that owner is the one Core published for the target Lane,
    /// never a default.
    #[test]
    fn git_command_opens_a_local_picker_and_sends_one_targeted_action() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState {
            capabilities: driver.capabilities(),
            ..TuiState::default()
        };
        let (lane_id, published_owner) = focus_lane_with_published_owner(&mut state);
        state.ui.input = "/git".into();

        submit_composer(&mut driver, &mut state).expect("open the source-control picker");

        assert!(matches!(
            state.ui.interaction_panel,
            Some(InteractionPanel::GitPicker {
                phase: GitPickerPhase::Browse,
                ..
            })
        ));
        assert!(
            sent.lock().expect("sent commands").is_empty(),
            "opening a selector must send no command"
        );

        // Row 0 is Stage; empty paths mean "every changed path", so the client
        // never enumerates a working tree Core owns.
        apply_git_picker_selection(&mut driver, &mut state, 0, &GitPickerPhase::Browse)
            .expect("send stage");

        let commands = sent.lock().expect("sent commands");
        let envelope = commands.first().expect("one action");
        let RuntimeCommand::RunOperatorGitAction {
            owner,
            target,
            action,
        } = &envelope.command
        else {
            panic!("expected an operator git action: {:?}", envelope.command);
        };
        assert_eq!(
            owner, &envelope.owner,
            "command actor must match the envelope"
        );
        assert_eq!(
            owner, &published_owner,
            "the actor is the owner Core published, never RuntimeOwner::default()"
        );
        assert_ne!(owner, &RuntimeOwner::default());
        assert_eq!(target, &viden_core::SourceTarget::Lane { lane_id });
        assert_eq!(
            action,
            &viden_core::OperatorGitAction::Stage { paths: Vec::new() }
        );
        assert_eq!(
            state.operator_git.pending().map(|p| p.command_id.as_str()),
            Some(envelope.command_id.as_str())
        );
        assert!(state.ui.interaction_panel.is_none());
    }

    /// Commit is the only action with text, and an empty draft must not reach
    /// Core: `git commit` with no message opens an editor in a non-interactive
    /// child. A focused Lane makes the action name that Lane, never a path.
    #[test]
    fn git_commit_needs_a_message_and_a_focused_lane_targets_that_lane() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState {
            capabilities: driver.capabilities(),
            ..TuiState::default()
        };
        let (lane_id, _) = focus_lane_with_published_owner(&mut state);

        // Picking Commit opens the prompt and sends nothing.
        apply_git_picker_selection(&mut driver, &mut state, 1, &GitPickerPhase::Browse)
            .expect("open the commit prompt");
        assert!(matches!(
            state.ui.interaction_panel,
            Some(InteractionPanel::GitPicker {
                phase: GitPickerPhase::CommitMessage { .. },
                ..
            })
        ));
        assert!(sent.lock().expect("sent commands").is_empty());

        // A blank draft is refused locally: nothing sent, nothing pending.
        apply_git_picker_selection(
            &mut driver,
            &mut state,
            1,
            &GitPickerPhase::CommitMessage {
                draft: "   ".to_string(),
            },
        )
        .expect("refuse the blank message");
        assert!(sent.lock().expect("sent commands").is_empty());
        assert!(state.operator_git.pending().is_none());

        apply_git_picker_selection(
            &mut driver,
            &mut state,
            1,
            &GitPickerPhase::CommitMessage {
                draft: "  feat(tui): add the /git picker  ".to_string(),
            },
        )
        .expect("send the commit");

        let commands = sent.lock().expect("sent commands");
        let RuntimeCommand::RunOperatorGitAction { target, action, .. } =
            &commands.first().expect("one action").command
        else {
            panic!("expected an operator git action");
        };
        assert_eq!(target, &viden_core::SourceTarget::Lane { lane_id });
        assert_eq!(
            action,
            &viden_core::OperatorGitAction::Commit {
                message: "feat(tui): add the /git picker".to_string()
            }
        );
    }

    /// Gives `state` a focused Lane whose runtime owner Core has published.
    ///
    /// `RunOperatorGitAction` is only sendable for such a Lane: every other
    /// target has no Core-published operator identity, and this client refuses
    /// rather than filling one in.
    fn focus_lane_with_published_owner(state: &mut TuiState) -> (String, RuntimeOwner) {
        state.runtime.lanes = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        let lane_id = state.runtime.lanes[0].id.clone();
        let owner = RuntimeOwner {
            workspace_id: "workspace".to_string(),
            project_id: "viden".to_string(),
            lane_id: Some(lane_id.clone()),
            session_id: Some("session-start".to_string()),
            task_id: Some("task-start".to_string()),
            turn_id: Some("turn-start".to_string()),
        };
        state.runtime.lane_runtime_owners = vec![viden_types::LaneRuntimeOwnerBinding {
            lane_id: lane_id.clone(),
            owner: owner.clone(),
        }];
        state.ui.focus_lane(lane_id.clone());
        (lane_id, owner)
    }

    /// `RunOperatorGitAction` is an audited mutation, and Core requires the
    /// command's `owner` to equal its envelope owner: that owner is the actor
    /// the audit record names. When Core has published no runtime owner for
    /// the focused Lane the action is refused locally, before `send` —
    /// `RuntimeOwner::default()` names nobody, and sending it would record an
    /// authorized source-control change as belonging to no one. The GUI
    /// refuses this exact case at `D1-OPERATOR-GIT-OWNER`; the two clients
    /// state one gap rather than two behaviors.
    #[test]
    fn a_lane_without_a_core_published_owner_refuses_before_anything_is_sent() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState {
            capabilities: driver.capabilities(),
            ..TuiState::default()
        };
        state.runtime.lanes = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        let lane_id = state.runtime.lanes[0].id.clone();
        state.ui.focus_lane(lane_id.clone());
        // Core published no binding for this Lane, which is the whole point:
        // the client must not manufacture the missing identity.
        assert!(state.runtime.lane_runtime_owners.is_empty());

        // Grouping, never hiding: the rows stay listed and say why.
        let rows = crate::tui::modal::git_picker_rows(&state);
        assert_eq!(rows.len(), 4, "the surface stays visible");
        assert!(
            rows.iter()
                .all(|row| matches!(row.kind, GitPickerRowKind::Disabled)
                    && row.label.contains(&lane_id)),
            "{rows:?}"
        );

        apply_git_picker_selection(&mut driver, &mut state, 0, &GitPickerPhase::Browse)
            .expect("refuse the action locally");

        assert!(
            sent.lock().expect("sent commands").is_empty(),
            "an owner Core never published must never reach the driver"
        );
        assert!(state.operator_git.pending().is_none());
        let entry = state.ui.entries.last().expect("the refusal is stated");
        assert_eq!(entry.label, "system");
        assert!(entry.body.contains(&lane_id), "{entry:?}");
        assert!(entry.body.contains("runtime owner"), "{entry:?}");
    }

    /// Absence is the refusal, and it is about the actor rather than the tree.
    ///
    /// Since `runtime.workspace_owner` (C5) Core mints a workspace-scoped
    /// identity at open, so an absent `RuntimeViewState.workspace_owner` means
    /// *this* Core published none — an engine built without a host binding, or
    /// a build predating the capability. The rows stay listed and say what is
    /// missing, the TARGET row keeps stating the workspace source facts Core
    /// *did* publish, and nothing is sent: `RuntimeOwner::default()` names
    /// nobody, and the client neither substitutes it nor recomputes the id
    /// from the root path itself.
    #[test]
    fn a_workspace_target_without_a_published_owner_refuses_before_anything_is_sent() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState {
            capabilities: driver.capabilities(),
            ..TuiState::default()
        };
        state.runtime.workspace_source = Some(viden_core::WorkspaceSourceView {
            status: viden_core::WorkspaceSourceStatus::Ready,
            branch: Some("claude/tui-parity-t2".to_string()),
            worktree: Some("workspace".to_string()),
            ahead: 2,
            behind: 0,
            added: 1,
            deleted: 0,
            dirty: true,
        });
        assert!(state.runtime.workspace_owner.is_none());
        assert_eq!(
            crate::tui::modal::git_target(&state),
            viden_core::SourceTarget::Workspace
        );

        let rows = crate::tui::modal::git_picker_rows(&state);
        assert_eq!(rows.len(), 4, "the surface stays visible");
        assert!(
            rows.iter()
                .all(|row| matches!(row.kind, GitPickerRowKind::Disabled)
                    && row.label.contains("Core published no workspace owner")),
            "{rows:?}"
        );
        assert!(
            !rows.iter().any(|row| row.label.contains("GUI-CORE-027")),
            "the register entry is closed on the Core side: {rows:?}"
        );

        state.ui.interaction_panel = Some(InteractionPanel::GitPicker {
            selected: 0,
            phase: GitPickerPhase::Browse,
        });
        let panel = crate::tui::modal::interaction_rows(&state);
        assert!(
            panel
                .iter()
                .any(|row| row.contains("workspace") && row.contains("claude/tui-parity-t2")),
            "the TARGET row keeps the published source facts: {panel:?}"
        );

        apply_git_picker_selection(&mut driver, &mut state, 0, &GitPickerPhase::Browse)
            .expect("refuse the action locally");

        assert!(
            sent.lock().expect("sent commands").is_empty(),
            "a default owner must never reach the driver"
        );
        assert!(state.operator_git.pending().is_none());
        let entry = state.ui.entries.last().expect("the refusal is stated");
        assert_eq!(entry.label, "system");
        assert!(
            entry.body.contains("Core published no workspace owner"),
            "the refusal names the missing fact: {entry:?}"
        );
    }

    /// GUI-CORE-027 closed, from this client's side.
    ///
    /// With `RuntimeViewState.workspace_owner` published the four rows are
    /// pickable, and the command carries that owner verbatim in both places
    /// Core compares: the command's own `owner` field and the envelope's.
    /// Core authorizes a workspace-target action by the workspace and project
    /// ids, so copying the published owner is the whole of the client's job
    /// here — and the owner it copies carries no Lane, session, task, or turn,
    /// because a workspace owner is a scope rather than a fallback.
    #[test]
    fn a_published_workspace_owner_enables_the_rows_and_is_sent_verbatim() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState {
            capabilities: driver.capabilities(),
            ..TuiState::default()
        };
        let workspace_owner = RuntimeOwner {
            workspace_id: "ws_4f3c1a09b8d27e65".to_string(),
            project_id: "prj_contract_v1_workspace".to_string(),
            lane_id: None,
            session_id: None,
            task_id: None,
            turn_id: None,
        };
        state.runtime.workspace_owner = Some(workspace_owner.clone());
        assert_eq!(
            crate::tui::modal::git_target(&state),
            viden_core::SourceTarget::Workspace
        );

        let rows = crate::tui::modal::git_picker_rows(&state);
        assert_eq!(rows.len(), 4);
        assert!(
            rows.iter()
                .all(|row| !matches!(row.kind, GitPickerRowKind::Disabled)),
            "{rows:?}"
        );
        assert!(
            !rows.iter().any(|row| row.label.contains("·")),
            "an enabled row carries no refusal suffix: {rows:?}"
        );

        // Row 2 is Push, which needs no prompt, so one selection is one
        // command.
        apply_git_picker_selection(&mut driver, &mut state, 2, &GitPickerPhase::Browse)
            .expect("send the action");

        let commands = sent.lock().expect("sent commands");
        let envelope = commands.first().expect("one action");
        assert_eq!(
            envelope.owner, workspace_owner,
            "the envelope owner is the actor Core audits"
        );
        let RuntimeCommand::RunOperatorGitAction {
            owner,
            target,
            action,
        } = &envelope.command
        else {
            panic!("expected an operator git action: {:?}", envelope.command);
        };
        assert_eq!(owner, &workspace_owner, "command and envelope must agree");
        assert_eq!(target, &viden_core::SourceTarget::Workspace);
        assert_eq!(
            action,
            &viden_core::OperatorGitAction::Push {
                remote: None,
                set_upstream: false
            }
        );
    }

    /// A Lane target reads its own source row, not the workspace's.
    ///
    /// `runtime.workspace_owner` split `LaneSourceUpdated` out of
    /// `WorkspaceSourceUpdated` precisely so one tree's branch and
    /// ahead/behind stop appearing under another tree's name. A Lane Core has
    /// sampled shows its own facts; a Lane it has not shows unknown, never the
    /// workspace's numbers.
    #[test]
    fn a_lane_target_row_states_the_lanes_own_source_and_never_the_workspaces() {
        let client = FakeCoreClient::default();
        let driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState {
            capabilities: driver.capabilities(),
            ..TuiState::default()
        };
        let (lane_id, _owner) = focus_lane_with_published_owner(&mut state);
        state.runtime.workspace_source = Some(viden_core::WorkspaceSourceView {
            status: viden_core::WorkspaceSourceStatus::Ready,
            branch: Some("main".to_string()),
            worktree: Some("workspace".to_string()),
            ahead: 9,
            behind: 0,
            added: 0,
            deleted: 0,
            dirty: false,
        });
        state.ui.interaction_panel = Some(InteractionPanel::GitPicker {
            selected: 0,
            phase: GitPickerPhase::Browse,
        });

        let unsampled = crate::tui::modal::interaction_rows(&state);
        assert!(
            unsampled
                .iter()
                .any(|row| row.contains(&lane_id) && !row.contains("main")),
            "an unsampled Lane must not borrow the workspace's branch: {unsampled:?}"
        );

        state.runtime.lane_sources.insert(
            lane_id.clone(),
            viden_core::WorkspaceSourceView {
                status: viden_core::WorkspaceSourceStatus::Ready,
                branch: Some("viden/lane_alpha".to_string()),
                worktree: Some("workspace/.worktrees/lane_alpha".to_string()),
                ahead: 1,
                behind: 2,
                added: 3,
                deleted: 0,
                dirty: true,
            },
        );
        let sampled = crate::tui::modal::interaction_rows(&state);
        assert!(
            sampled.iter().any(|row| row.contains(&lane_id)
                && row.contains("viden/lane_alpha")
                && row.contains("behind 2")),
            "the Lane's own row is what the header states: {sampled:?}"
        );
        assert!(
            !sampled.iter().any(|row| row.contains("ahead 9")),
            "the workspace's numbers must not appear under a Lane: {sampled:?}"
        );
    }

    /// Without the capability every row stays listed, rendered disabled with
    /// the capability's own name, and Enter sends nothing. Grouping, never
    /// hiding — and never a shell fallback.
    #[test]
    fn git_rows_stay_listed_and_inert_without_the_operator_git_capability() {
        let mut capabilities = viden_core::frontend_capabilities();
        capabilities.remove(&viden_core::CapabilityId(
            OPERATOR_GIT_CAPABILITY.to_string(),
        ));
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                capabilities: Some(capabilities),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let sent = Arc::clone(&client.sent);
        let mut driver =
            TuiClientDriver::connect(client).expect("a missing extension must not block startup");
        let mut state = TuiState {
            capabilities: driver.capabilities(),
            ..TuiState::default()
        };

        let rows = crate::tui::modal::git_picker_rows(&state);

        assert_eq!(rows.len(), 4, "grouping never hides the surface");
        assert!(
            rows.iter()
                .all(|row| matches!(row.kind, GitPickerRowKind::Disabled)
                    && row.label.contains(OPERATOR_GIT_CAPABILITY)),
            "{rows:?}"
        );

        apply_git_picker_selection(&mut driver, &mut state, 0, &GitPickerPhase::Browse)
            .expect("a disabled row sends nothing");
        assert!(sent.lock().expect("sent commands").is_empty());
    }

    /// Replays the shared `operator-git.json` fixture: a refusal before
    /// anything ran, a completed commit, and a push that failed *after* the
    /// gate. All three reach the transcript as typed system entries, and none
    /// of them is read out of git's output text.
    /// The end-to-end loop, not just the machine: the picker sends, Core's
    /// ordered events come back through `pump`, and `observe_driver_events`
    /// turns the settled fact into one transcript entry. This is what proves
    /// the correlation survives the real event path.
    #[test]
    fn a_settled_operator_action_reaches_the_transcript_through_the_event_loop() {
        let source = viden_core::WorkspaceSourceView {
            status: viden_core::WorkspaceSourceStatus::Ready,
            branch: Some("main".to_string()),
            worktree: Some("workspace".to_string()),
            ahead: 0,
            behind: 0,
            added: 2,
            deleted: 0,
            dirty: true,
        };
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                events: VecDeque::from([
                    event(
                        1,
                        RuntimeEventKind::CommandAccepted {
                            command_id: "tui-1".to_string(),
                            command: RuntimeCommand::CancelActiveTurn,
                        },
                    ),
                    event(
                        2,
                        RuntimeEventKind::OperatorGitActionFinished {
                            command_id: "tui-1".to_string(),
                            target: viden_core::SourceTarget::Workspace,
                            action: viden_core::OperatorGitAction::Stage { paths: Vec::new() },
                            outcome: viden_core::OperatorGitOutcome::Completed {
                                output: "staged 2 paths".to_string(),
                                truncated: false,
                                source: source.clone(),
                            },
                            audit_id: "audit-stage".to_string(),
                        },
                    ),
                ]),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState {
            capabilities: driver.capabilities(),
            ..TuiState::default()
        };
        focus_lane_with_published_owner(&mut state);

        apply_git_picker_selection(&mut driver, &mut state, 0, &GitPickerPhase::Browse)
            .expect("send stage");
        assert!(state.operator_git.pending().is_some());

        while !matches!(driver.pump().expect("pump"), PumpOutcome::Idle) {
            observe_driver_events(&mut state, &mut driver).expect("observe");
        }
        observe_driver_events(&mut state, &mut driver).expect("observe");

        assert!(
            state.operator_git.pending().is_none(),
            "the finished event must free the slot"
        );
        let entry = state
            .ui
            .entries
            .iter()
            .find(|entry| entry.label == "system" && entry.body.contains("completed"))
            .unwrap_or_else(|| panic!("no settled entry: {:?}", state.ui.entries));
        assert!(entry.body.contains("main"), "{entry:?}");
        assert!(entry.body.contains("audit-stage"), "{entry:?}");
        // The typed source, not a fact read out of the output text.
        assert!(entry.body.contains("dirty"), "{entry:?}");
    }

    #[test]
    fn operator_git_fixture_replays_into_typed_outcome_entries() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/operator-git.json"
        ))
        .expect("operator git fixture");
        let events = fixture["events"]
            .as_array()
            .expect("fixture events")
            .iter()
            .filter_map(|value| {
                let envelope: RuntimeEventEnvelope =
                    serde_json::from_value(value.clone()).expect("fixture envelope");
                match envelope.event {
                    RuntimeWireEvent::Known(event) => Some(event),
                    RuntimeWireEvent::Unknown { .. } => None,
                }
            })
            .collect::<Vec<_>>();

        let mut state = TuiState {
            capabilities: viden_core::frontend_capabilities(),
            ..TuiState::default()
        };
        let mut settled = Vec::new();
        for command_id in [
            "operator_git_stage_refused",
            "operator_git_commit",
            "operator_git_push",
        ] {
            let mut machine = crate::tui::state::OperatorGitMachine::default();
            machine
                .begin(
                    command_id,
                    viden_core::SourceTarget::Workspace,
                    viden_core::OperatorGitAction::Fetch { remote: None },
                )
                .expect("nothing in flight");
            for event in &events {
                if let Some(settlement) = machine.observe_event(event) {
                    settled.push(operator_git_entry(&state, &settlement));
                    break;
                }
            }
        }
        state.ui.entries.extend(settled.clone());

        assert_eq!(settled.len(), 3, "every fixture command must settle");
        // Refused before anything ran: Core's reason verbatim, no recovery
        // invented, and never labelled as a failure of git itself.
        assert!(settled[0].body.contains("refused before anything ran"));
        assert!(
            settled[0]
                .body
                .contains("git_add is denied by a workspace rule")
        );
        // Completed: the resampled source, not a fact read out of the output.
        assert!(settled[1].body.contains("completed"));
        assert!(settled[1].body.contains("codex/v3-core-runtime"));
        assert!(settled[1].body.contains("ahead 2"));
        assert!(settled[1].body.contains("clean"));
        assert!(settled[1].body.contains("audit_operator_git_commit"));
        // Failed after the gate: the localized class and its recovery, plus
        // Core's own detail. This is an event, not a `CommandRejected`.
        assert!(settled[2].body.contains("failed"));
        assert!(settled[2].body.contains("this branch has no upstream"));
        assert!(settled[2].body.contains("push again with set upstream"));
        assert!(!settled[2].body.contains("refused before anything ran"));
    }

    #[test]
    fn focused_acp_composer_and_ctrl_c_target_the_exact_session() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        let owner = RuntimeOwner {
            workspace_id: "workspace-1".to_string(),
            project_id: "project-1".to_string(),
            lane_id: Some("lane-1".to_string()),
            session_id: Some("acp-1".to_string()),
            ..RuntimeOwner::default()
        };
        state
            .runtime
            .agent_sessions
            .push(viden_core::AgentSessionView {
                session_id: "acp-1".to_string(),
                lane_id: "lane-1".to_string(),
                agent_id: "codex-acp".to_string(),
                model: None,
                status: viden_core::AgentSessionStatus::Running,
                owner: owner.clone(),
                task: "implement".to_string(),
                diagnostic: None,
                output: None,
            });
        state.ui.focus_lane("lane-1".to_string());
        state.ui.session_id = "acp-1".to_string();
        state.ui.focused_conversation = Some(FocusedConversation::AcpSession("acp-1".to_string()));
        state.ui.input = "continue".into();

        submit_composer(&mut driver, &mut state).expect("send ACP follow-up");
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            (120, 40),
        )
        .expect("cancel ACP session");

        let commands = sent.lock().expect("sent commands");
        assert!(matches!(
            &commands[0],
            RuntimeCommandEnvelope {
                owner: command_owner,
                command: RuntimeCommand::SendAgentSessionInput { input },
                ..
            } if command_owner == &owner
                && input.session_id == "acp-1"
                && input.content == "continue"
        ));
        assert!(matches!(
            &commands[1],
            RuntimeCommandEnvelope {
                owner: command_owner,
                command: RuntimeCommand::CancelAgentSession { session_id },
                ..
            } if command_owner == &owner && session_id == "acp-1"
        ));
    }

    #[test]
    fn matching_agent_start_event_focuses_the_new_acp_session() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/interaction-closed-loop.json"
        ))
        .expect("interaction fixture");
        let mut event =
            serde_json::from_value::<RuntimeEventEnvelope>(fixture["events"][5].clone())
                .expect("agent session started event");
        event.cursor.stream_id = "fixture".to_string();
        event.cursor.sequence = 1;
        if let RuntimeWireEvent::Known(event) = &mut event.event {
            event.sequence = 1;
        }
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                events: VecDeque::from([event]),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        state.ui.focus_lane("lane-loop-coder".to_string());
        state.ui.pending_acp_start = Some(PendingAcpStart {
            lane_id: "lane-loop-coder".to_string(),
            agent_id: "viden-built-in".to_string(),
        });

        driver.pump().expect("agent start event");
        project_driver_view(&mut state, &driver);
        observe_driver_events(&mut state, &mut driver).expect("focus new ACP session");

        assert_eq!(state.ui.session_id, "session-loop-built-in");
        assert_eq!(
            state.ui.focused_conversation,
            Some(FocusedConversation::AcpSession(
                "session-loop-built-in".to_string()
            ))
        );
        assert!(state.ui.pending_acp_start.is_none());
        assert_eq!(state.ui.lens, Lens::Session);
    }

    #[test]
    fn native_lane_task_waits_for_preview_and_receipt_before_submitting() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/interaction-closed-loop.json"
        ))
        .expect("interaction fixture");
        let mut events = fixture["events"]
            .as_array()
            .expect("fixture events")
            .iter()
            .take(4)
            .map(|value| {
                serde_json::from_value::<RuntimeEventEnvelope>(value.clone())
                    .expect("runtime event")
            })
            .collect::<VecDeque<_>>();
        for event in &mut events {
            event.cursor.stream_id = "fixture".to_string();
        }
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                events,
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();

        for _ in 0..2 {
            driver.pump().expect("eligibility event");
        }
        project_driver_view(&mut state, &driver);
        observe_driver_events(&mut state, &mut driver).expect("observe eligibility");
        state.ui.interaction_panel = Some(InteractionPanel::NewLaneTask {
            task: "fix the parser".to_string(),
        });
        apply_interaction_panel_selection(&mut driver, &mut state).expect("request preview");
        assert!(matches!(
            sent.lock().expect("sent commands")[0].command,
            RuntimeCommand::PreviewDefaultStarterLane {
                preset: StarterLanePreset::Coder
            }
        ));
        assert!(
            !sent
                .lock()
                .expect("sent commands")
                .iter()
                .any(|envelope| matches!(envelope.command, RuntimeCommand::SubmitUserInput { .. }))
        );

        driver.pump().expect("preview event");
        project_driver_view(&mut state, &driver);
        observe_driver_events(&mut state, &mut driver).expect("create from preview");
        assert!(matches!(
            sent.lock().expect("sent commands")[1].command,
            RuntimeCommand::CreateStarterLane { .. }
        ));

        driver.pump().expect("receipt event");
        project_driver_view(&mut state, &driver);
        observe_driver_events(&mut state, &mut driver).expect("submit after receipt");
        let commands = sent.lock().expect("sent commands");
        assert!(matches!(
            &commands[2].command,
            RuntimeCommand::SubmitUserInput { content } if content == "fix the parser"
        ));
        assert!(state.ui.pending_native_lane.is_none());
        assert!(matches!(
            state.ui.focused_conversation,
            Some(FocusedConversation::NativeLane(_))
        ));
    }

    #[test]
    fn native_lane_task_preserves_direct_workspace_preview_without_fake_branch() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/interaction-closed-loop.json"
        ))
        .expect("interaction fixture");
        let mut events = fixture["events"]
            .as_array()
            .expect("fixture events")
            .iter()
            .take(3)
            .map(|value| {
                serde_json::from_value::<RuntimeEventEnvelope>(value.clone())
                    .expect("runtime event")
            })
            .collect::<VecDeque<_>>();
        for event in &mut events {
            event.cursor.stream_id = "fixture".to_string();
        }
        let preview = match &mut events[2].event {
            viden_core::RuntimeWireEvent::Known(viden_core::RuntimeEvent {
                kind: viden_core::RuntimeEventKind::StarterLanePreviewed { preview },
                ..
            }) => preview,
            event => panic!("expected starter Lane preview, got {event:?}"),
        };
        preview.lane.branch = None;
        preview.lane.worktree = None;
        preview.branch.clear();
        preview.worktree_path = "workspace/project".to_string();
        preview.base_revision = "workspace:direct".to_string();

        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                events,
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();

        for _ in 0..2 {
            driver.pump().expect("eligibility event");
        }
        project_driver_view(&mut state, &driver);
        observe_driver_events(&mut state, &mut driver).expect("observe eligibility");
        state.ui.interaction_panel = Some(InteractionPanel::NewLaneTask {
            task: "inspect this folder".to_string(),
        });
        apply_interaction_panel_selection(&mut driver, &mut state).expect("request preview");
        driver.pump().expect("direct workspace preview");
        project_driver_view(&mut state, &driver);
        observe_driver_events(&mut state, &mut driver).expect("create from direct preview");

        let commands = sent.lock().expect("sent commands");
        let RuntimeCommand::CreateStarterLane { request, .. } = &commands[1].command else {
            panic!("expected create starter Lane command");
        };
        assert_eq!(request.branch, None);
        assert_eq!(request.worktree_path, None);
    }

    #[test]
    fn r_retries_only_the_selected_failed_acp_session() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        let owner = RuntimeOwner {
            lane_id: Some("lane-1".to_string()),
            session_id: Some("acp-failed".to_string()),
            ..RuntimeOwner::default()
        };
        state.ui.focus_lane("lane-1".to_string());
        state
            .runtime
            .agent_sessions
            .push(viden_core::AgentSessionView {
                session_id: "acp-failed".to_string(),
                lane_id: "lane-1".to_string(),
                agent_id: "codex-acp".to_string(),
                model: None,
                status: viden_core::AgentSessionStatus::Failed,
                owner: owner.clone(),
                task: "failed task".to_string(),
                diagnostic: Some("recoverable".to_string()),
                output: None,
            });
        state.ui.interaction_panel = Some(InteractionPanel::AcpPicker {
            selected: 0,
            phase: AcpPickerPhase::Browse,
        });

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("retry failed ACP session");

        assert!(matches!(
            sent.lock().expect("sent commands").as_slice(),
            [RuntimeCommandEnvelope {
                owner: command_owner,
                command: RuntimeCommand::RetryAgentSession { session_id },
                ..
            }] if command_owner == &owner && session_id == "acp-failed"
        ));
    }

    #[test]
    fn exact_setup_enter_opens_setup_while_nonexact_prefix_only_completes() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        state.ui.input_mode = InputMode::Insert;
        state.ui.input = "/set".into();

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("complete setup prefix");
        assert_eq!(state.ui.input, "/setup");
        assert_eq!(state.ui.lens, Lens::Welcome);
        assert!(sent.lock().expect("sent commands").is_empty());

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("submit exact setup");

        assert_eq!(state.ui.lens, Lens::Setup);
        assert!(matches!(
            state.ui.interaction_panel,
            Some(InteractionPanel::Setup { ref draft, .. })
                if draft == "[project]\nname = \"\"\npack = \"\"\n"
        ));
        assert!(state.ui.input.is_empty());
        assert!(matches!(
            sent.lock().expect("sent commands").as_slice(),
            [RuntimeCommandEnvelope {
                command: RuntimeCommand::ProbeProject,
                ..
            }]
        ));

        let operator_draft =
            "[project]\nname = \"operator-demo\"\npack = \"robot-pack\"\n".to_string();
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Paste(operator_draft.clone()),
            (120, 40),
        )
        .expect("replace setup draft by paste");
        assert!(matches!(
            state.ui.interaction_panel,
            Some(InteractionPanel::Setup { ref draft, .. }) if draft == &operator_draft
        ));
    }

    #[test]
    fn setup_previews_exact_draft_before_core_confirmation() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        state.ui.lens = Lens::Setup;
        let exact_contents = "[project]\nname = \"demo\"\npack = \"robot-pack\"\n".to_string();
        state.ui.interaction_panel = Some(InteractionPanel::Setup {
            selected: 1,
            draft: exact_contents.clone(),
        });

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            (112, 40),
        )
        .expect("preview exact draft");

        assert!(matches!(
            sent.lock().expect("sent commands").as_slice(),
            [RuntimeCommandEnvelope {
                command: RuntimeCommand::PreviewProjectConfig { contents },
                ..
            }] if contents == &exact_contents
        ));
        assert!(state.runtime.project_config_preview.is_none());
        assert!(state.runtime.confirmed_project_config.is_none());
        assert!(matches!(
            state.ui.interaction_panel,
            Some(InteractionPanel::Setup { ref draft, .. }) if draft == &exact_contents
        ));

        state.runtime.project_config_preview = Some(ProjectConfigPreview {
            preview_id: "preview-core".to_string(),
            relative_path: "viden.toml".to_string(),
            content_sha256: "b".repeat(64),
            byte_len: exact_contents.len() as u64,
            exact_contents: Some(exact_contents.clone()),
            base_content_sha256: None,
            project_name: Some("demo".to_string()),
            pack: Some("robot-pack".to_string()),
            diagnostics: Vec::new(),
        });
        if let Some(InteractionPanel::Setup {
            selected, draft, ..
        }) = state.ui.interaction_panel.as_mut()
        {
            *selected = 2;
            draft.push_str("# changed after preview\n");
        }

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            (112, 40),
        )
        .expect("reject stale preview");
        assert_eq!(sent.lock().expect("sent commands").len(), 1);
        assert!(state.runtime.confirmed_project_config.is_none());

        if let Some(InteractionPanel::Setup { draft, .. }) = state.ui.interaction_panel.as_mut() {
            *draft = exact_contents.clone();
        }
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            (112, 40),
        )
        .expect("confirm matching preview");

        assert!(matches!(
            sent.lock().expect("sent commands").as_slice(),
            [
                RuntimeCommandEnvelope {
                    command: RuntimeCommand::PreviewProjectConfig { .. },
                    ..
                },
                RuntimeCommandEnvelope {
                    command: RuntimeCommand::ConfirmProjectConfig {
                        preview_id,
                        content_sha256,
                    },
                    ..
                }
            ] if preview_id == "preview-core" && content_sha256 == &"b".repeat(64)
        ));
        assert!(state.runtime.confirmed_project_config.is_none());
        assert_eq!(state.ui.lens, Lens::Setup);
    }

    #[test]
    fn lane_overlay_selection_uses_core_lane_and_session_identity() {
        let client = FakeCoreClient::default();
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        let mut lanes: Vec<AgentLaneRecord> = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        lanes.truncate(1);
        lanes[0].active_session_ids = vec!["session-from-core".to_string()];
        let lane_id = lanes[0].id.clone();
        state.runtime.lanes = lanes;
        state.ui.lens = Lens::Board;
        state.ui.overlay = Some(OverlayState::new(OverlayKind::Lane));

        apply_input_intent(
            &mut driver,
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            InputIntent::CompleteOrSubmit,
            (112, 40),
        )
        .expect("select lane");

        assert_eq!(state.ui.lens, Lens::Session);
        assert_eq!(state.ui.focused_lane.as_deref(), Some(lane_id.as_str()));
        assert_eq!(state.ui.session_id, "session-from-core");
    }

    #[test]
    fn lane_without_core_session_stays_on_board_with_lane_detail() {
        let client = FakeCoreClient::default();
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        let mut lanes: Vec<AgentLaneRecord> = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        lanes.truncate(1);
        lanes[0].active_session_ids.clear();
        let lane_id = lanes[0].id.clone();
        state.runtime.lanes = lanes;
        state.ui.lens = Lens::Board;
        state.ui.overlay = Some(OverlayState::new(OverlayKind::Lane));

        apply_input_intent(
            &mut driver,
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            InputIntent::CompleteOrSubmit,
            (112, 40),
        )
        .expect("select lane without session");

        assert_eq!(state.ui.lens, Lens::Board);
        assert_eq!(state.ui.focused_lane.as_deref(), Some(lane_id.as_str()));
        assert!(state.ui.session_id.is_empty());
    }

    #[test]
    fn lane_with_multiple_core_sessions_requires_session_selection() {
        let client = FakeCoreClient::default();
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        let mut lanes: Vec<AgentLaneRecord> = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        lanes.truncate(1);
        lanes[0].active_session_ids = vec!["session-a".to_string(), "session-b".to_string()];
        state.runtime.lanes = lanes;
        state.ui.lens = Lens::Board;
        state.ui.overlay = Some(OverlayState::new(OverlayKind::Lane));

        apply_input_intent(
            &mut driver,
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            InputIntent::CompleteOrSubmit,
            (112, 40),
        )
        .expect("select lane");

        assert!(
            state
                .ui
                .overlay
                .as_ref()
                .is_some_and(|overlay| overlay.kind == OverlayKind::Session)
        );
        state.ui.overlay.as_mut().unwrap().selected = 1;

        apply_input_intent(
            &mut driver,
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            InputIntent::CompleteOrSubmit,
            (112, 40),
        )
        .expect("select session");

        assert_eq!(state.ui.lens, Lens::Session);
        assert_eq!(state.ui.session_id, "session-b");
    }

    #[test]
    fn event_cursor_stream_never_overwrites_selected_session_identity() {
        let mut state = TuiState::default();
        let mut lanes: Vec<AgentLaneRecord> = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        lanes.truncate(1);
        lanes[0].active_session_ids = vec!["session-from-lane".to_string()];
        state.ui.focus_lane(lanes[0].id.clone());
        state.runtime.lanes = lanes;
        state.ui.session_id = "session-from-lane".to_string();
        let view = state.runtime.clone();

        project_runtime_view(
            &mut state,
            &view,
            &EventCursor {
                stream_id: "event-log-stream".to_string(),
                sequence: 7,
            },
        );

        assert_eq!(state.ui.session_id, "session-from-lane");
    }

    #[test]
    fn runtime_replacement_atomically_clears_stale_lane_and_session_identity() {
        let mut lanes: Vec<AgentLaneRecord> = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        lanes.truncate(1);
        lanes[0].active_session_ids = vec!["session-core".to_string()];
        let lane_id = lanes[0].id.clone();
        let mut state = TuiState::default();
        state.runtime.lanes = lanes.clone();
        state.ui.focus_lane(lane_id.clone());
        state.ui.session_id = "session-core".to_string();
        state.ui.lens = Lens::Session;
        state.ui.input_mode = InputMode::Insert;
        state.ui.input = "preserve draft".into();

        let mut without_session = state.runtime.clone();
        without_session.lanes[0].active_session_ids.clear();
        project_runtime_view(
            &mut state,
            &without_session,
            &EventCursor {
                stream_id: "fixture".to_string(),
                sequence: 8,
            },
        );

        assert_eq!(state.ui.focused_lane.as_deref(), Some(lane_id.as_str()));
        assert!(state.ui.session_id.is_empty());
        assert_eq!(state.ui.lens, Lens::Board);
        assert_eq!(state.ui.input, "preserve draft");
        assert_eq!(state.ui.input_mode, InputMode::Insert);

        let mut without_lane = without_session;
        without_lane.lanes.clear();
        project_runtime_view(
            &mut state,
            &without_lane,
            &EventCursor {
                stream_id: "fixture".to_string(),
                sequence: 9,
            },
        );

        assert!(state.ui.focused_lane.is_none());
        assert!(state.ui.session_id.is_empty());
        assert_eq!(state.ui.lens, Lens::Board);
        assert_eq!(state.ui.input, "preserve draft");
    }

    /// The lens half of the `/git` target fix.
    ///
    /// A selected Lane with no session pulls the operator to the board, which
    /// renders no composer. That is right while its detail panel is open and
    /// wrong once they have closed it to type a command: the selection is then
    /// only a target, and `/git` is typed in the composer. The Lane stays
    /// selected either way.
    #[test]
    fn a_lane_target_without_its_detail_panel_keeps_the_composer_on_screen() {
        let mut lanes: Vec<AgentLaneRecord> = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        lanes.truncate(1);
        lanes[0].active_session_ids.clear();
        let lane_id = lanes[0].id.clone();
        let view = {
            let mut view = TuiState::default().runtime;
            view.lanes = lanes;
            view
        };

        let mut looking_at_the_lane = TuiState::default();
        looking_at_the_lane.ui.focus_lane(lane_id.clone());
        looking_at_the_lane.ui.lens = Lens::Session;
        project_runtime_view(
            &mut looking_at_the_lane,
            &view,
            &EventCursor {
                stream_id: "fixture".to_string(),
                sequence: 1,
            },
        );
        assert_eq!(looking_at_the_lane.ui.lens, Lens::Board);

        let mut typing_a_command = TuiState::default();
        typing_a_command.ui.focus_lane(lane_id.clone());
        typing_a_command.ui.lane_detail_open = false;
        typing_a_command.ui.lens = Lens::Session;
        project_runtime_view(
            &mut typing_a_command,
            &view,
            &EventCursor {
                stream_id: "fixture".to_string(),
                sequence: 1,
            },
        );

        assert_eq!(
            typing_a_command.ui.lens,
            Lens::Session,
            "the composer must stay on screen for the command the Lane target is for"
        );
        assert_eq!(
            typing_a_command.ui.focused_lane.as_deref(),
            Some(lane_id.as_str())
        );
        assert_eq!(
            super::super::modal::git_target(&typing_a_command),
            viden_core::SourceTarget::Lane { lane_id }
        );
    }

    #[test]
    fn runtime_replacement_drops_stale_extension_visibility_and_cancel_transport() {
        let (_initial_driver, mut state, _initial_sent, _) = exact_lane_owner_driver();
        state.capabilities = frontend_capabilities();
        state.ui.lens = Lens::Board;
        assert!(
            crate::tui::render::render_side_frame(&state, 100, 70)
                .contains("CANCEL L-start · Ctrl-C")
        );

        let base_capabilities = viden_core::CORE_CLIENT_CAPABILITIES
            .iter()
            .map(|capability| CapabilityId((*capability).to_string()))
            .collect::<BTreeSet<_>>();
        let replacement_client = FakeCoreClient {
            transport: FakeCoreTransport {
                view: Some(state.runtime.clone()),
                capabilities: Some(base_capabilities),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let sent = Arc::clone(&replacement_client.sent);
        let mut replacement_driver =
            TuiClientDriver::connect(replacement_client).expect("base-only replacement");

        project_driver_view(&mut state, &replacement_driver);

        assert!(
            crate::tui::render::render_side_frame(&state, 100, 70)
                .contains("CANCEL UNAVAILABLE L-start · Core capability unavailable")
        );
        handle_ui_event(
            &mut replacement_driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            (120, 40),
        )
        .expect("extension loss fail-closes cancellation");
        assert!(sent.lock().expect("sent commands").is_empty());
        assert!(!state.ui.idle_ctrl_c_armed);
        assert!(state.ui.overlay.is_none());
    }

    #[test]
    fn runtime_replacement_switches_locale_without_cached_tui_authority() {
        let mut state = TuiState::default();
        state.ui.lens = Lens::Board;
        let english = crate::tui::render::render_frame(&state, 112, 40);
        assert!(english.contains("No Core lanes available."));

        let mut replacement = state.runtime.clone();
        replacement.snapshot.ui_preferences.locale = viden_core::LocaleId::ZhCn;
        replacement.ui_preferences.locale = viden_core::LocaleId::ZhCn;
        project_runtime_view(
            &mut state,
            &replacement,
            &EventCursor {
                stream_id: "ui-preferences".to_string(),
                sequence: 10,
            },
        );

        let chinese = crate::tui::render::render_frame(&state, 112, 40);
        assert!(chinese.contains("Core 暂无 lane。"));
        assert!(state.ui.theme_name.contains("zh-CN"));
        assert_eq!(state.ui.lens, Lens::Board);
    }

    #[test]
    fn settings_without_extension_are_visible_but_send_no_preference_command() {
        let base_capabilities = viden_core::CORE_CLIENT_CAPABILITIES
            .iter()
            .map(|capability| CapabilityId((*capability).to_string()))
            .collect::<BTreeSet<_>>();
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                capabilities: Some(base_capabilities),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("base-only client");
        let mut state = state_from_driver(&driver, &TuiOptions::new("startup"));
        state.ui.input = "/settings".into();

        submit_composer(&mut driver, &mut state).expect("open settings");
        assert!(matches!(
            state.ui.interaction_panel,
            Some(InteractionPanel::Settings(_))
        ));
        if let Some(InteractionPanel::Settings(panel)) = state.ui.interaction_panel.as_mut() {
            panel.selected = 7;
        }
        apply_interaction_panel_selection(&mut driver, &mut state).expect("disabled reset");

        assert!(sent.lock().expect("sent commands").is_empty());
    }

    #[test]
    fn plan_mode_settings_wait_for_matching_preference_update_before_success() {
        let persisted = UiPreferences {
            density: viden_core::UiDensity::Comfy,
            ..UiPreferences::default()
        };
        let resolved = viden_core::ResolvedUiPreferences {
            density: viden_core::UiDensity::Comfy,
            ..viden_core::ResolvedUiPreferences::default()
        };
        let command = RuntimeCommand::SetUiPreferences {
            patch: UiPreferencePatch {
                density: Some(viden_core::UiDensity::Comfy),
                ..UiPreferencePatch::default()
            },
        };
        let events = VecDeque::from([
            RuntimeEventEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                owner: RuntimeOwner::default(),
                cursor: EventCursor {
                    stream_id: "fixture".to_string(),
                    sequence: 1,
                },
                event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
                    1,
                    Some(1),
                    RuntimeEventKind::CommandAccepted {
                        command_id: "tui-1".to_string(),
                        command: command.clone(),
                    },
                )),
            },
            RuntimeEventEnvelope {
                schema_version: FRONTEND_SCHEMA_V1,
                owner: RuntimeOwner::default(),
                cursor: EventCursor {
                    stream_id: "fixture".to_string(),
                    sequence: 2,
                },
                event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
                    2,
                    Some(2),
                    RuntimeEventKind::UiPreferencesUpdated {
                        resolved,
                        persisted: Some(persisted),
                        diagnostics: Vec::new(),
                    },
                )),
            },
        ]);
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                events,
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("extension client");
        let mut state = state_from_driver(&driver, &TuiOptions::new("startup"));
        state.runtime.snapshot.work_mode = WorkMode::Plan;
        state.ui.interaction_panel = Some(InteractionPanel::Settings(Box::new(
            SettingsPanel::new(&state.runtime.snapshot.ui_preferences, ColorDepth::Auto),
        )));
        if let Some(InteractionPanel::Settings(panel)) = state.ui.interaction_panel.as_mut() {
            panel.select(super::super::preferences::PreferenceValue::Density(
                viden_core::UiDensity::Comfy,
            ));
            panel.selected = 6;
        }

        apply_interaction_panel_selection(&mut driver, &mut state)
            .expect("send plan-mode UI patch");
        assert_eq!(sent.lock().expect("sent commands")[0].command, command);

        driver.pump().expect("accepted event");
        observe_driver_events(&mut state, &mut driver).expect("observe preference event");
        let panel = match state.ui.interaction_panel.as_ref() {
            Some(InteractionPanel::Settings(panel)) => panel,
            other => panic!("settings panel missing: {other:?}"),
        };
        assert!(panel.is_pending());
        assert!(!panel.has_succeeded());

        driver.pump().expect("preference update");
        project_driver_view(&mut state, &driver);
        observe_driver_events(&mut state, &mut driver).expect("observe preference event");
        let panel = match state.ui.interaction_panel.as_ref() {
            Some(InteractionPanel::Settings(panel)) => panel,
            other => panic!("settings panel missing: {other:?}"),
        };
        assert!(!panel.is_pending());
        assert!(panel.has_succeeded());
        assert_eq!(
            state.runtime.snapshot.ui_preferences.density,
            viden_core::UiDensity::Comfy
        );
    }

    #[test]
    fn runtime_replacement_closes_stale_explicit_approval_focus() {
        let (_driver, mut state, _sent) = pending_approval_driver();
        state.ui.overlay = Some(OverlayState::new(OverlayKind::Approval));
        state.ui.input = "preserve draft".into();
        let mut replacement = state.runtime.clone();
        replacement.pending_approvals.clear();

        project_runtime_view(
            &mut state,
            &replacement,
            &EventCursor {
                stream_id: "fixture".to_string(),
                sequence: 1,
            },
        );

        assert!(state.ui.overlay.is_none());
        assert_eq!(state.ui.input, "preserve draft");
    }

    #[test]
    fn bootstrap_accepts_direct_core_client() {
        let options = TuiOptions::new("startup").with_startup_check();

        run_tui(FakeCoreClient::default(), options).expect("direct CoreClient bootstrap");
    }

    #[test]
    fn startup_requests_project_probe_after_capability_negotiation() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);

        run_tui(client, TuiOptions::new("startup").with_startup_check()).expect("startup probe");

        assert!(matches!(
            sent.lock().expect("sent commands").as_slice(),
            [RuntimeCommandEnvelope {
                command: RuntimeCommand::ProbeProject,
                ..
            }]
        ));
    }

    #[test]
    fn missing_project_onboarding_keeps_startup_and_setup_available_without_transport() {
        let base_capabilities = viden_core::CORE_CLIENT_CAPABILITIES
            .iter()
            .map(|capability| CapabilityId((*capability).to_string()))
            .collect::<BTreeSet<_>>();
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                capabilities: Some(base_capabilities.clone()),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let sent = Arc::clone(&client.sent);

        run_tui(client, TuiOptions::new("startup").with_startup_check())
            .expect("base-only startup");
        assert!(sent.lock().expect("sent commands").is_empty());

        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                capabilities: Some(base_capabilities),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("base-only client");
        let mut state = state_from_driver(&driver, &TuiOptions::new("startup"));

        assert!(open_local_lens_command(&mut driver, "/setup", &mut state).unwrap());
        assert_eq!(state.ui.lens, Lens::Setup);
        assert!(state.ui.interaction_panel.is_some());
        assert!(sent.lock().expect("sent commands").is_empty());
        let english = crate::tui::render::render_frame(&state, 112, 40);
        assert!(english.contains("PROJECT ONBOARDING unavailable"));
        assert!(
            super::super::i18n::text(&state, "interaction.setup.unavailable")
                .contains("runtime.project_onboarding")
        );

        state.runtime.snapshot.ui_preferences.locale = viden_core::LocaleId::ZhCn;
        let chinese = crate::tui::render::render_frame(&state, 112, 40);
        assert!(chinese.contains("项目接入不可用"));
        assert!(
            super::super::i18n::text(&state, "interaction.setup.unavailable")
                .contains("runtime.project_onboarding")
        );
    }

    #[test]
    fn supervision_decision_confirms_only_on_the_core_business_fact() {
        let gate = |status| viden_types::MergeGateRecord {
            gate_id: "gate-1".to_string(),
            task_id: "task-1".to_string(),
            status,
            required_evidence: Vec::new(),
            evidence_ids: Vec::new(),
            gate_type: Default::default(),
            owner: Default::default(),
            validator: None,
            policy_snapshot: Default::default(),
            decision: None,
            conflict: None,
            applied_change_id: None,
            recovery_snapshot: None,
            audit_ids: Vec::new(),
            updated_at: Some(1),
        };
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                events: VecDeque::from([
                    event(
                        1,
                        RuntimeEventKind::CommandAccepted {
                            command_id: "command-1".to_string(),
                            command: RuntimeCommand::AcceptMergeGate {
                                gate_id: "gate-1".to_string(),
                                actor: Default::default(),
                                reviewed_evidence: Vec::new(),
                                decision: None,
                            },
                        },
                    ),
                    event(
                        2,
                        RuntimeEventKind::MergeGateUpdated {
                            gate: gate(viden_types::MergeGateStatus::CollectingEvidence),
                        },
                    ),
                    event(
                        3,
                        RuntimeEventKind::MergeGateUpdated {
                            gate: gate(viden_types::MergeGateStatus::Accepted),
                        },
                    ),
                ]),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = state_from_driver(&driver, &TuiOptions::new("startup"));
        state
            .supervision
            .begin(
                "command-1",
                crate::tui::pending::SupervisionExpectation::MergeGate {
                    gate_id: "gate-1".to_string(),
                    status: viden_types::MergeGateStatus::Accepted,
                },
            )
            .expect("no other supervision command in flight");

        driver.pump().expect("command receipt");
        observe_driver_events(&mut state, &mut driver).expect("observe receipt");
        assert_eq!(
            state.supervision.outcome(),
            &crate::tui::pending::SupervisionOutcome::Pending {
                command_id: "command-1".to_string()
            },
            "a receipt and an unrelated gate transition must not confirm"
        );

        driver.pump().expect("intermediate gate transition");
        observe_driver_events(&mut state, &mut driver).expect("observe intermediate transition");
        assert_eq!(
            state.supervision.outcome(),
            &crate::tui::pending::SupervisionOutcome::Pending {
                command_id: "command-1".to_string()
            },
            "a gate transition to another status must not confirm this decision"
        );

        driver.pump().expect("gate fact");
        observe_driver_events(&mut state, &mut driver).expect("observe gate fact");
        assert_eq!(
            state.supervision.outcome(),
            &crate::tui::pending::SupervisionOutcome::Confirmed
        );
        assert!(state.supervision.pending().is_none());
    }

    // ---- supervision decision workflows -------------------------------------

    fn supervision_owner(lane: &str) -> RuntimeOwner {
        RuntimeOwner {
            workspace_id: "workspace".to_string(),
            project_id: "project".to_string(),
            lane_id: Some(lane.to_string()),
            session_id: Some("session-a".to_string()),
            task_id: Some("task-1".to_string()),
            turn_id: Some("turn-1".to_string()),
        }
    }

    fn supervision_gate(status: viden_types::MergeGateStatus) -> viden_types::MergeGateRecord {
        viden_types::MergeGateRecord {
            gate_id: "gate-1".to_string(),
            task_id: "task-1".to_string(),
            status,
            required_evidence: Vec::new(),
            evidence_ids: vec!["ev-1".to_string()],
            gate_type: Default::default(),
            owner: supervision_owner("lane-a"),
            validator: None,
            policy_snapshot: Default::default(),
            decision: None,
            conflict: None,
            applied_change_id: Some("change-1".to_string()),
            recovery_snapshot: None,
            audit_ids: Vec::new(),
            updated_at: Some(1),
        }
    }

    fn supervision_evidence(hash: &str) -> viden_core::EvidenceView {
        viden_core::EvidenceView {
            id: "ev-1".to_string(),
            kind: "test".to_string(),
            summary: "cargo test".to_string(),
            path: None,
            source: None,
            canonical: Some(viden_types::CanonicalEvidenceReference {
                item_id: "item-ev-1".to_string(),
                bundle_id: "bundle-1".to_string(),
                source_hash: hash.to_string(),
                producer: viden_types::EvidenceProducer {
                    identity: "lane-a".to_string(),
                    role: "coder".to_string(),
                    task_id: "task-1".to_string(),
                },
                permission_snapshot_id: None,
                permission_scope: viden_types::ContextScope::Task("task-1".to_string()),
                evidence_scope: viden_types::ContextScope::Task("task-1".to_string()),
                verification: viden_types::EvidenceVerificationState::Verified,
                quality: viden_types::EvidenceQualityFacts {
                    status: viden_types::EvidenceQualityStatus::Pass,
                    reason_codes: Vec::new(),
                },
            }),
            metadata: None,
            timestamp: Some(1),
            owner: None,
        }
    }

    fn supervision_review(
        status: viden_types::ReviewRequestStatus,
    ) -> viden_types::ReviewRequestRecord {
        viden_types::ReviewRequestRecord {
            review_id: "review-1".to_string(),
            gate_id: "gate-1".to_string(),
            task_id: "task-1".to_string(),
            requester_lane_id: "lane-a".to_string(),
            reviewer_lane_id: "lane-b".to_string(),
            owner: supervision_owner("lane-a"),
            evidence_ids: vec!["ev-1".to_string()],
            evidence_bindings: vec![viden_types::ReviewedEvidenceBinding {
                evidence_id: "ev-1".to_string(),
                source_hash: "hash-1".to_string(),
            }],
            status,
            feedback: None,
            audit_id: "audit-review".to_string(),
            updated_at: 2,
        }
    }

    fn supervision_bounce(
        status: viden_types::ConflictBounceStatus,
    ) -> viden_types::ConflictBounce {
        viden_types::ConflictBounce {
            bounce_id: "bounce-1".to_string(),
            gate_id: "gate-1".to_string(),
            task_id: "task-1".to_string(),
            original_lane_id: "lane-a".to_string(),
            owner: supervision_owner("lane-a"),
            reason: "base moved".to_string(),
            status,
            evidence_ids: vec!["ev-1".to_string()],
            baseline_evidence: vec![viden_types::ReviewedEvidenceBinding {
                evidence_id: "ev-1".to_string(),
                source_hash: "hash-baseline".to_string(),
            }],
            revalidation_evidence: Vec::new(),
            content: None,
            audit_id: "audit-bounce".to_string(),
            created_at: 3,
            revalidated_at: None,
        }
    }

    fn supervision_snapshot() -> RuntimeSnapshot {
        RuntimeSnapshot {
            cwd: PathBuf::from("/workspace"),
            provider_family: "fallback".to_string(),
            model_label: "test-local".to_string(),
            work_mode: WorkMode::Build,
            permission_mode: PermissionMode::Default,
            permission_level: PermissionLevel::Ask,
            config_summary: "fixture".to_string(),
            loaded_config_files: Vec::new(),
            startup_overrides: Vec::new(),
            ui_preferences: Default::default(),
        }
    }

    /// A driver over a Core view that already published the supervision records,
    /// plus whatever ordered events the case wants Core to publish back.
    fn supervision_driver(
        view: RuntimeViewState,
        events: Vec<RuntimeEventEnvelope>,
    ) -> (
        TuiClientDriver<FakeCoreClient>,
        TuiState,
        Arc<Mutex<Vec<RuntimeCommandEnvelope>>>,
    ) {
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                view: Some(view),
                events: VecDeque::from(events),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let sent = Arc::clone(&client.sent);
        let driver = TuiClientDriver::connect(client).expect("connect");
        let state = TuiState::new(driver.view().clone());
        (driver, state, sent)
    }

    fn key_event(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn press(
        driver: &mut TuiClientDriver<FakeCoreClient>,
        state: &mut TuiState,
        code: KeyCode,
    ) -> UiEventOutcome {
        handle_ui_event(driver, state, key_event(code), (120, 40)).expect("key")
    }

    fn type_text(driver: &mut TuiClientDriver<FakeCoreClient>, state: &mut TuiState, text: &str) {
        for value in text.chars() {
            press(driver, state, KeyCode::Char(value));
        }
    }

    #[test]
    fn decision_center_lists_supervision_rows_and_routes_every_pick() {
        let mut view = RuntimeViewState::new(supervision_snapshot());
        view.merge_gates.push(supervision_gate(
            viden_types::MergeGateStatus::CollectingEvidence,
        ));
        view.review_requests.push(supervision_review(
            viden_types::ReviewRequestStatus::Pending,
        ));
        view.conflict_bounces.push(supervision_bounce(
            viden_types::ConflictBounceStatus::Pending,
        ));
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/approval-allow-deny.json"
        ))
        .expect("approval fixture");
        let envelope: RuntimeEventEnvelope =
            serde_json::from_value(fixture["events"][0].clone()).expect("approval event");
        if let RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
        let approval_id = view.pending_approvals[0].id.clone();
        let (mut driver, mut state, sent) = supervision_driver(view, Vec::new());
        state.ui.overlay = Some(OverlayState::new(OverlayKind::Decisions));

        let rows = crate::tui::modal::overlay_rows_for_test(&state, OverlayKind::Decisions);
        let joined = rows.join("\n");
        for expected in [
            "APPROVAL",
            "GATE gate-1",
            "REVIEW review-1",
            "CONFLICT bounce-1",
        ] {
            assert!(joined.contains(expected), "missing {expected}:\n{joined}");
        }
        for glyph in ["\u{23f8}", "\u{25cc}", "\u{26a0}"] {
            assert!(
                joined.contains(glyph),
                "supervision rows must use registered glyphs: {joined}"
            );
        }

        // Index 0 is the pending approval and still routes to the pinned
        // Approval overlay rather than the supervision surface.
        press(&mut driver, &mut state, KeyCode::Enter);
        let overlay = state.ui.overlay.as_ref().expect("approval overlay");
        assert_eq!(overlay.kind, OverlayKind::Approval);
        assert_eq!(overlay.selected_id.as_deref(), Some(approval_id.as_str()));
        assert!(state.ui.supervision.is_none());

        for (index, expected) in [
            (
                1,
                SupervisionTarget::Gate {
                    gate_id: "gate-1".to_string(),
                },
            ),
            (
                2,
                SupervisionTarget::Review {
                    review_id: "review-1".to_string(),
                },
            ),
            (
                3,
                SupervisionTarget::Bounce {
                    gate_id: "gate-1".to_string(),
                },
            ),
        ] {
            let mut overlay = OverlayState::new(OverlayKind::Decisions);
            overlay.selected = index;
            state.ui.overlay = Some(overlay);
            press(&mut driver, &mut state, KeyCode::Enter);
            assert_eq!(
                state.ui.overlay.as_ref().map(|overlay| overlay.kind),
                Some(OverlayKind::SupervisionDecision)
            );
            assert_eq!(
                state
                    .ui
                    .supervision
                    .as_ref()
                    .map(|panel| panel.target.clone()),
                Some(expected)
            );
        }
        assert!(
            sent.lock().expect("sent").is_empty(),
            "picking a row selects; it never decides"
        );
    }

    #[test]
    fn supervision_overlay_unwinds_escape_in_order_and_yields_to_a_pinned_approval() {
        let mut view = RuntimeViewState::new(supervision_snapshot());
        view.merge_gates.push(supervision_gate(
            viden_types::MergeGateStatus::CollectingEvidence,
        ));
        let (mut driver, mut state, sent) = supervision_driver(view, Vec::new());
        open_supervision_decision(
            &mut state,
            SupervisionTarget::Gate {
                gate_id: "gate-1".to_string(),
            },
        );

        // Focus the reject action and open its reason line.
        press(&mut driver, &mut state, KeyCode::Char('2'));
        press(&mut driver, &mut state, KeyCode::Enter);
        assert!(
            state
                .ui
                .supervision
                .as_ref()
                .expect("panel")
                .input
                .is_some()
        );

        // First Esc unwinds the reason line, second closes the overlay.
        press(&mut driver, &mut state, KeyCode::Esc);
        assert_eq!(
            state.ui.overlay.as_ref().map(|overlay| overlay.kind),
            Some(OverlayKind::SupervisionDecision)
        );
        assert!(
            state
                .ui
                .supervision
                .as_ref()
                .expect("panel")
                .input
                .is_none()
        );
        press(&mut driver, &mut state, KeyCode::Esc);
        assert!(state.ui.overlay.is_none());
        assert!(state.ui.supervision.is_none());
        assert!(sent.lock().expect("sent").is_empty());
    }

    #[test]
    fn supervision_overlay_only_lists_actions_the_gate_status_can_accept() {
        let mut view = RuntimeViewState::new(supervision_snapshot());
        view.merge_gates
            .push(supervision_gate(viden_types::MergeGateStatus::Merged));
        let (_driver, mut state, _sent) = supervision_driver(view, Vec::new());
        open_supervision_decision(
            &mut state,
            SupervisionTarget::Gate {
                gate_id: "gate-1".to_string(),
            },
        );

        let merged =
            crate::tui::modal::overlay_rows_for_test(&state, OverlayKind::SupervisionDecision)
                .join("\n");
        assert!(merged.contains("Revert applied change"));
        assert!(
            !merged.contains("Accept merge gate"),
            "a merged gate can no longer be accepted: {merged}"
        );
        assert!(
            merged.contains("Core cannot put it back"),
            "revert must carry its irreversibility hint: {merged}"
        );

        state.runtime.merge_gates[0].status = viden_types::MergeGateStatus::CollectingEvidence;
        let open =
            crate::tui::modal::overlay_rows_for_test(&state, OverlayKind::SupervisionDecision)
                .join("\n");
        assert!(open.contains("Accept merge gate"));
        assert!(
            !open.contains("Revert applied change"),
            "a gate that was never merged has nothing to revert: {open}"
        );

        state.runtime.merge_gates[0].conflict = Some(supervision_bounce(
            viden_types::ConflictBounceStatus::Pending,
        ));
        let conflicted =
            crate::tui::modal::overlay_rows_for_test(&state, OverlayKind::SupervisionDecision)
                .join("\n");
        assert!(conflicted.contains("Revalidate conflict"));
        assert!(
            !conflicted.contains("Accept merge gate"),
            "acceptance waits for the origin Lane to revalidate: {conflicted}"
        );
    }

    #[test]
    fn a_required_reason_is_enforced_locally_and_nothing_is_sent() {
        let mut view = RuntimeViewState::new(supervision_snapshot());
        view.merge_gates.push(supervision_gate(
            viden_types::MergeGateStatus::CollectingEvidence,
        ));
        let (mut driver, mut state, sent) = supervision_driver(view, Vec::new());
        open_supervision_decision(
            &mut state,
            SupervisionTarget::Gate {
                gate_id: "gate-1".to_string(),
            },
        );
        press(&mut driver, &mut state, KeyCode::Char('2'));
        press(&mut driver, &mut state, KeyCode::Enter);

        // Empty reason.
        press(&mut driver, &mut state, KeyCode::Enter);
        assert_eq!(
            state
                .ui
                .supervision
                .as_ref()
                .expect("panel")
                .notice
                .as_deref(),
            Some("supervision.error.reason_required")
        );
        assert!(sent.lock().expect("sent").is_empty());
        assert_eq!(
            state.supervision.outcome(),
            &crate::tui::pending::SupervisionOutcome::Idle
        );

        // Over-limit reason.
        press(&mut driver, &mut state, KeyCode::Enter);
        type_text(&mut driver, &mut state, &"x".repeat(501));
        press(&mut driver, &mut state, KeyCode::Enter);
        assert_eq!(
            state
                .ui
                .supervision
                .as_ref()
                .expect("panel")
                .notice
                .as_deref(),
            Some("supervision.error.reason_too_long")
        );
        assert!(
            sent.lock().expect("sent").is_empty(),
            "over-limit text is refused, never truncated and sent"
        );
    }

    /// One full decision: send the exact envelope, stay pending through the
    /// receipt, and settle only on Core's own fact.
    fn assert_supervision_round_trip(
        view: RuntimeViewState,
        target: SupervisionTarget,
        action_index: usize,
        reason: Option<&str>,
        expected: RuntimeCommand,
        expected_owner: RuntimeOwner,
        fact: RuntimeEventKind,
    ) {
        let events = vec![
            event(
                1,
                RuntimeEventKind::CommandAccepted {
                    command_id: "tui-1".to_string(),
                    command: expected.clone(),
                },
            ),
            event(2, fact),
        ];
        let (mut driver, mut state, sent) = supervision_driver(view, events);
        open_supervision_decision(&mut state, target);
        for _ in 0..action_index {
            press(&mut driver, &mut state, KeyCode::Down);
        }
        // The first Enter confirms the action; an action that carries text opens
        // its line instead, and the second Enter submits it — empty when Core
        // treats the text as optional.
        press(&mut driver, &mut state, KeyCode::Enter);
        if state
            .ui
            .supervision
            .as_ref()
            .is_some_and(|panel| panel.input.is_some())
        {
            if let Some(reason) = reason {
                type_text(&mut driver, &mut state, reason);
            }
            press(&mut driver, &mut state, KeyCode::Enter);
        } else {
            assert!(reason.is_none(), "this action carries no operator text");
        }

        let envelopes = sent.lock().expect("sent").clone();
        assert_eq!(envelopes.len(), 1, "exactly one command per decision");
        assert_eq!(envelopes[0].command, expected);
        assert_eq!(envelopes[0].owner, expected_owner);
        assert_eq!(envelopes[0].command_id, "tui-1");
        assert_eq!(
            state.supervision.outcome(),
            &crate::tui::pending::SupervisionOutcome::Pending {
                command_id: "tui-1".to_string()
            }
        );

        driver.pump().expect("receipt");
        observe_driver_events(&mut state, &mut driver).expect("observe receipt");
        assert_eq!(
            state.supervision.outcome(),
            &crate::tui::pending::SupervisionOutcome::Pending {
                command_id: "tui-1".to_string()
            },
            "a receipt is never a decision"
        );

        driver.pump().expect("fact");
        observe_driver_events(&mut state, &mut driver).expect("observe fact");
        assert_eq!(
            state.supervision.outcome(),
            &crate::tui::pending::SupervisionOutcome::Confirmed
        );
    }

    #[test]
    fn every_supervision_decision_round_trips_through_its_exact_core_fact() {
        let base = || {
            let mut view = RuntimeViewState::new(supervision_snapshot());
            view.merge_gates.push(supervision_gate(
                viden_types::MergeGateStatus::CollectingEvidence,
            ));
            view.latest_evidence.push(supervision_evidence("hash-1"));
            view
        };
        let binding = viden_types::ReviewedEvidenceBinding {
            evidence_id: "ev-1".to_string(),
            source_hash: "hash-1".to_string(),
        };
        let gate_target = || SupervisionTarget::Gate {
            gate_id: "gate-1".to_string(),
        };

        // Accept the gate.
        let mut accepted = supervision_gate(viden_types::MergeGateStatus::Accepted);
        accepted.applied_change_id = Some("change-1".to_string());
        assert_supervision_round_trip(
            base(),
            gate_target(),
            0,
            None,
            RuntimeCommand::AcceptMergeGate {
                gate_id: "gate-1".to_string(),
                actor: supervision_owner("lane-a"),
                reviewed_evidence: vec![binding.clone()],
                decision: None,
            },
            supervision_owner("lane-a"),
            RuntimeEventKind::MergeGateUpdated { gate: accepted },
        );

        // Reject the gate with the operator's reason.
        assert_supervision_round_trip(
            base(),
            gate_target(),
            1,
            Some("evidence missing"),
            RuntimeCommand::RejectMergeGate {
                gate_id: "gate-1".to_string(),
                actor: supervision_owner("lane-a"),
                reason: "evidence missing".to_string(),
            },
            supervision_owner("lane-a"),
            RuntimeEventKind::MergeGateUpdated {
                gate: supervision_gate(viden_types::MergeGateStatus::NeedsChanges),
            },
        );

        // Revert a merged gate.
        let mut merged_view = base();
        merged_view.merge_gates[0].status = viden_types::MergeGateStatus::Merged;
        assert_supervision_round_trip(
            merged_view,
            gate_target(),
            0,
            Some("regression in main"),
            RuntimeCommand::RevertAppliedChange {
                gate_id: "gate-1".to_string(),
                owner: supervision_owner("lane-a"),
                reason: "regression in main".to_string(),
            },
            supervision_owner("lane-a"),
            RuntimeEventKind::RevertRecorded {
                revert: viden_types::RevertRecord {
                    revert_id: "revert-1".to_string(),
                    gate_id: "gate-1".to_string(),
                    applied_change_id: "change-1".to_string(),
                    owner: supervision_owner("lane-a"),
                    reason: "regression in main".to_string(),
                    restored_paths: Vec::new(),
                    audit_id: "audit-revert".to_string(),
                    reverted_at: 4,
                },
            },
        );

        // Bounce the gate back to its origin Lane.
        assert_supervision_round_trip(
            base(),
            SupervisionTarget::Bounce {
                gate_id: "gate-1".to_string(),
            },
            0,
            Some("base moved"),
            RuntimeCommand::BounceMergeConflict {
                gate_id: "gate-1".to_string(),
                original_lane_id: "lane-a".to_string(),
                owner: supervision_owner("lane-a"),
                reason: "base moved".to_string(),
            },
            supervision_owner("lane-a"),
            RuntimeEventKind::MergeConflictBounced {
                conflict: supervision_bounce(viden_types::ConflictBounceStatus::Pending),
            },
        );

        // Revalidate a pending conflict with a changed canonical receipt.
        let mut conflicted = base();
        conflicted.merge_gates[0].conflict = Some(supervision_bounce(
            viden_types::ConflictBounceStatus::Pending,
        ));
        conflicted.conflict_bounces.push(supervision_bounce(
            viden_types::ConflictBounceStatus::Pending,
        ));
        assert_supervision_round_trip(
            conflicted,
            gate_target(),
            0,
            None,
            RuntimeCommand::RevalidateMergeConflict {
                gate_id: "gate-1".to_string(),
                bounce_id: "bounce-1".to_string(),
                actor: supervision_owner("lane-a"),
                evidence: binding.clone(),
            },
            supervision_owner("lane-a"),
            RuntimeEventKind::MergeGateUpdated {
                gate: supervision_gate(viden_types::MergeGateStatus::CollectingEvidence),
            },
        );

        // Review verdicts, with and without feedback.
        let review_view = || {
            let mut view = base();
            view.merge_gates[0].validator = Some(viden_types::MergeGateValidator {
                owner: supervision_owner("lane-b"),
                review_request_id: "review-1".to_string(),
                independent: true,
                validated_at: None,
            });
            view.review_requests.push(supervision_review(
                viden_types::ReviewRequestStatus::Pending,
            ));
            view
        };
        let review_target = || SupervisionTarget::Review {
            review_id: "review-1".to_string(),
        };
        assert_supervision_round_trip(
            review_view(),
            review_target(),
            0,
            None,
            RuntimeCommand::DecideReview {
                review_id: "review-1".to_string(),
                verdict: viden_types::ReviewVerdict::Accepted,
                feedback: None,
                actor: supervision_owner("lane-b"),
            },
            supervision_owner("lane-b"),
            RuntimeEventKind::ReviewRequestUpdated {
                review: supervision_review(viden_types::ReviewRequestStatus::Accepted),
            },
        );
        assert_supervision_round_trip(
            review_view(),
            review_target(),
            1,
            Some("needs a regression test"),
            RuntimeCommand::DecideReview {
                review_id: "review-1".to_string(),
                verdict: viden_types::ReviewVerdict::Rejected,
                feedback: Some("needs a regression test".to_string()),
                actor: supervision_owner("lane-b"),
            },
            supervision_owner("lane-b"),
            RuntimeEventKind::ReviewRequestUpdated {
                review: supervision_review(viden_types::ReviewRequestStatus::Rejected),
            },
        );
    }

    #[test]
    fn core_rejection_renders_its_own_reason_and_frees_the_decision_slot() {
        let mut view = RuntimeViewState::new(supervision_snapshot());
        view.merge_gates.push(supervision_gate(
            viden_types::MergeGateStatus::CollectingEvidence,
        ));
        view.latest_evidence.push(supervision_evidence("hash-1"));
        let (mut driver, mut state, _sent) = supervision_driver(
            view,
            vec![event(
                1,
                RuntimeEventKind::CommandRejected {
                    command_id: "tui-1".to_string(),
                    reason: "merge gate `gate-1` is no longer open".to_string(),
                },
            )],
        );
        open_supervision_decision(
            &mut state,
            SupervisionTarget::Gate {
                gate_id: "gate-1".to_string(),
            },
        );
        press(&mut driver, &mut state, KeyCode::Enter);

        driver.pump().expect("rejection");
        observe_driver_events(&mut state, &mut driver).expect("observe rejection");

        assert_eq!(
            state.supervision.outcome(),
            &crate::tui::pending::SupervisionOutcome::Rejected {
                reason: "merge gate `gate-1` is no longer open".to_string()
            }
        );
        let rows =
            crate::tui::modal::overlay_rows_for_test(&state, OverlayKind::SupervisionDecision)
                .join("\n");
        assert!(
            rows.contains("merge gate `gate-1` is no longer open"),
            "Core's reason must be rendered verbatim: {rows}"
        );
        assert!(state.supervision.pending().is_none());
    }

    #[test]
    fn a_second_supervision_action_while_one_is_pending_sends_nothing() {
        let mut view = RuntimeViewState::new(supervision_snapshot());
        view.merge_gates.push(supervision_gate(
            viden_types::MergeGateStatus::CollectingEvidence,
        ));
        view.latest_evidence.push(supervision_evidence("hash-1"));
        let (mut driver, mut state, sent) = supervision_driver(view, Vec::new());
        open_supervision_decision(
            &mut state,
            SupervisionTarget::Gate {
                gate_id: "gate-1".to_string(),
            },
        );
        press(&mut driver, &mut state, KeyCode::Enter);
        assert_eq!(sent.lock().expect("sent").len(), 1);

        // The accept is still pending, so the reject is refused locally.
        press(&mut driver, &mut state, KeyCode::Down);
        press(&mut driver, &mut state, KeyCode::Enter);
        type_text(&mut driver, &mut state, "second thoughts");
        press(&mut driver, &mut state, KeyCode::Enter);

        assert_eq!(
            sent.lock().expect("sent").len(),
            1,
            "a busy correlation must not race a second command"
        );
        assert_eq!(
            state
                .ui
                .supervision
                .as_ref()
                .expect("panel")
                .notice
                .as_deref(),
            Some("supervision.pending.busy")
        );
        assert_eq!(
            state.supervision.outcome(),
            &crate::tui::pending::SupervisionOutcome::Pending {
                command_id: "tui-1".to_string()
            }
        );
    }

    #[test]
    fn dismiss_releases_a_stranded_pending_decision_without_sending_anything() {
        let mut view = RuntimeViewState::new(supervision_snapshot());
        view.merge_gates.push(supervision_gate(
            viden_types::MergeGateStatus::CollectingEvidence,
        ));
        view.latest_evidence.push(supervision_evidence("hash-1"));
        let (mut driver, mut state, sent) = supervision_driver(view, Vec::new());
        open_supervision_decision(
            &mut state,
            SupervisionTarget::Gate {
                gate_id: "gate-1".to_string(),
            },
        );
        press(&mut driver, &mut state, KeyCode::Enter);
        assert!(state.supervision.pending().is_some());

        // Accept / Reject / Dismiss: the escape is appended last.
        let rows =
            crate::tui::modal::overlay_rows_for_test(&state, OverlayKind::SupervisionDecision)
                .join("\n");
        assert!(rows.contains("Dismiss pending attribution"));
        assert!(rows.contains("does not cancel the Core command"));
        press(&mut driver, &mut state, KeyCode::Char('3'));
        press(&mut driver, &mut state, KeyCode::Enter);

        assert_eq!(
            state.supervision.outcome(),
            &crate::tui::pending::SupervisionOutcome::Idle,
            "dismissing settles nothing; it only stops attributing"
        );
        assert!(state.supervision.pending().is_none());
        assert_eq!(
            sent.lock().expect("sent").len(),
            1,
            "dismiss sends no command of its own"
        );
    }

    #[test]
    fn a_settled_outcome_resets_on_the_next_action_and_on_overlay_close() {
        let mut view = RuntimeViewState::new(supervision_snapshot());
        view.merge_gates.push(supervision_gate(
            viden_types::MergeGateStatus::CollectingEvidence,
        ));
        view.latest_evidence.push(supervision_evidence("hash-1"));
        let (mut driver, mut state, _sent) = supervision_driver(view, Vec::new());
        let target = || SupervisionTarget::Gate {
            gate_id: "gate-1".to_string(),
        };

        // Rule (b): closing the overlay while settled clears the echo.
        open_supervision_decision(&mut state, target());
        state
            .supervision
            .begin(
                "tui-0",
                crate::tui::pending::SupervisionExpectation::Revert {
                    gate_id: "gate-1".to_string(),
                },
            )
            .expect("first command");
        state.supervision.observe_event(&RuntimeEvent::new(
            1,
            RuntimeEventKind::CommandRejected {
                command_id: "tui-0".to_string(),
                reason: "no applied change".to_string(),
            },
        ));
        assert!(matches!(
            state.supervision.outcome(),
            crate::tui::pending::SupervisionOutcome::Rejected { .. }
        ));
        press(&mut driver, &mut state, KeyCode::Esc);
        assert_eq!(
            state.supervision.outcome(),
            &crate::tui::pending::SupervisionOutcome::Idle
        );

        // Rule (a): opening the next decision clears a settled echo too.
        state
            .supervision
            .begin(
                "tui-0",
                crate::tui::pending::SupervisionExpectation::Revert {
                    gate_id: "gate-1".to_string(),
                },
            )
            .expect("second command");
        state.supervision.observe_event(&RuntimeEvent::new(
            2,
            RuntimeEventKind::CommandRejected {
                command_id: "tui-0".to_string(),
                reason: "no applied change".to_string(),
            },
        ));
        open_supervision_decision(&mut state, target());
        assert_eq!(
            state.supervision.outcome(),
            &crate::tui::pending::SupervisionOutcome::Idle
        );

        // A pending decision is never auto-reset by either route.
        state
            .supervision
            .begin(
                "tui-9",
                crate::tui::pending::SupervisionExpectation::Revert {
                    gate_id: "gate-1".to_string(),
                },
            )
            .expect("third command");
        press(&mut driver, &mut state, KeyCode::Esc);
        assert_eq!(
            state.supervision.outcome(),
            &crate::tui::pending::SupervisionOutcome::Pending {
                command_id: "tui-9".to_string()
            }
        );
    }

    #[test]
    fn composer_stays_editable_while_the_supervision_overlay_is_open_during_a_stream() {
        let mut view = RuntimeViewState::new(supervision_snapshot());
        view.merge_gates.push(supervision_gate(
            viden_types::MergeGateStatus::CollectingEvidence,
        ));
        let (mut driver, mut state, sent) = supervision_driver(
            view,
            vec![event(
                1,
                RuntimeEventKind::AssistantDelta {
                    message_id: "assistant-1".to_string(),
                    task_id: None,
                    session_id: None,
                    content: "working".to_string(),
                },
            )],
        );
        driver.pump().expect("stream event");
        project_runtime_view(&mut state, driver.view(), driver.cursor());
        open_supervision_decision(
            &mut state,
            SupervisionTarget::Gate {
                gate_id: "gate-1".to_string(),
            },
        );

        // The overlay has no text filter: non-action characters keep editing the
        // composer, so a streaming turn stays answerable with a decision open.
        type_text(&mut driver, &mut state, "你好");
        assert_eq!(state.ui.input, "你好");
        press(&mut driver, &mut state, KeyCode::Backspace);
        assert_eq!(state.ui.input, "你");
        assert_eq!(driver.view().assistant_stream, "working");
        assert_eq!(
            state.ui.overlay.as_ref().map(|overlay| overlay.kind),
            Some(OverlayKind::SupervisionDecision)
        );
        assert!(sent.lock().expect("sent").is_empty());

        // Action numbers still belong to the overlay.
        press(&mut driver, &mut state, KeyCode::Char('2'));
        assert_eq!(state.ui.supervision.as_ref().expect("panel").focus, 1);
        assert_eq!(state.ui.input, "你");

        // A paste follows the same rule: the reason line when one is open,
        // otherwise the composer. It never becomes an overlay filter.
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Paste("好".to_string()),
            (120, 40),
        )
        .expect("paste");
        assert_eq!(state.ui.input, "你好");
        press(&mut driver, &mut state, KeyCode::Enter);
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Paste("evidence missing".to_string()),
            (120, 40),
        )
        .expect("paste into reason");
        assert_eq!(
            state
                .ui
                .supervision
                .as_ref()
                .and_then(|panel| panel.input.as_ref())
                .map(|input| input.text.as_str()),
            Some("evidence missing")
        );
        assert_eq!(state.ui.input, "你好");
        assert!(
            state
                .ui
                .overlay
                .as_ref()
                .is_some_and(|overlay| overlay.filter.is_empty())
        );
    }

    // ---- workspace file inventory ------------------------------------------

    /// Opening the jump index reads the Core inventory once, and only once.
    ///
    /// GUI-CORE-022: the client must never walk the workspace, so the `~` scope
    /// is either what Core published or an honest gap. A second open must not
    /// re-read a tree Core already handed over, because two pages racing one
    /// correlation slot cannot be told apart before they arrive.
    #[test]
    fn opening_the_jump_index_reads_the_workspace_inventory_exactly_once() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        project_driver_view(&mut state, &driver);
        assert!(
            state.has_capability(WORKSPACE_FILES_CAPABILITY),
            "the fake Core advertises the full extension set"
        );

        let open = Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        handle_ui_event(&mut driver, &mut state, open.clone(), (120, 40)).expect("open jump");
        let queries = || {
            sent.lock()
                .expect("sent commands")
                .iter()
                .filter(|envelope| {
                    matches!(envelope.command, RuntimeCommand::QueryWorkspaceFiles { .. })
                })
                .count()
        };
        assert_eq!(queries(), 1, "opening the jump index reads the inventory");

        // Answer the read, close, and reopen: the client has the page, so it
        // asks again for nothing.
        let command_id = sent
            .lock()
            .expect("sent commands")
            .iter()
            .find(|envelope| matches!(envelope.command, RuntimeCommand::QueryWorkspaceFiles { .. }))
            .expect("the inventory read")
            .command_id
            .clone();
        state.ui.workspace_files.observe_event(&RuntimeEvent::new(
            1,
            RuntimeEventKind::WorkspaceFilesLoaded {
                command_id,
                page: viden_types::WorkspaceFilePage {
                    entries: vec![viden_types::WorkspaceFileEntry {
                        path: "README.md".to_string(),
                        kind: viden_types::WorkspaceFileKind::File,
                        size_bytes: Some(12),
                    }],
                    next_after: None,
                    complete: true,
                },
            },
        ));
        assert!(state.ui.workspace_files.is_loaded());
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("close jump");
        handle_ui_event(&mut driver, &mut state, open, (120, 40)).expect("reopen jump");
        assert_eq!(queries(), 1, "a loaded inventory is not re-read");

        let index = JumpIndex::from_state(&state);
        let files = index
            .items()
            .iter()
            .filter(|item| item.kind == JumpKind::File)
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 1);
        assert!(files[0].enabled);
        assert_eq!(files[0].id, "README.md");
    }

    /// Without the capability the client sends nothing at all and keeps the
    /// honest disabled row, rather than showing an empty file list.
    #[test]
    fn a_core_without_the_inventory_capability_gets_no_query_and_an_honest_row() {
        let mut client = FakeCoreClient::default();
        let capability = viden_types::CapabilityId(WORKSPACE_FILES_CAPABILITY.to_string());
        // Start from the full advertised set and drop exactly this one, so the
        // test proves the gate rather than an unrelated missing capability.
        let mut capabilities = frontend_capabilities();
        capabilities.remove(&capability);
        client.transport.capabilities = Some(capabilities);
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        project_driver_view(&mut state, &driver);
        assert!(!state.has_capability(WORKSPACE_FILES_CAPABILITY));

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)),
            (120, 40),
        )
        .expect("open jump");
        assert!(
            !sent
                .lock()
                .expect("sent commands")
                .iter()
                .any(|envelope| matches!(
                    envelope.command,
                    RuntimeCommand::QueryWorkspaceFiles { .. }
                )),
            "a missing capability must send no command at all"
        );
        let index = JumpIndex::from_state(&state);
        let row = index
            .items()
            .iter()
            .find(|item| item.kind == JumpKind::File)
            .expect("file row");
        assert!(!row.enabled);
        assert_eq!(
            row.disabled_reason.as_deref(),
            Some("Core file inventory is unavailable.")
        );
    }

    // ---- audit timeline -----------------------------------------------------

    fn audit_record(
        audit_id: &str,
        timestamp: u64,
        action: &str,
        outcome: viden_types::AuditOutcome,
    ) -> viden_types::AuditRecord {
        viden_types::AuditRecord {
            audit_id: audit_id.to_string(),
            timestamp,
            owner: supervision_owner("lane-a"),
            actor: viden_types::AuditActor::Operator,
            action: action.to_string(),
            objects: vec![viden_types::AuditObjectRef::new(
                viden_types::AuditObjectRef::KIND_MERGE_GATE,
                "gate-1",
            )],
            outcome,
            args: std::collections::BTreeMap::from([(
                "decision".to_string(),
                "accepted".to_string(),
            )]),
        }
    }

    /// A scripted page with no command id.
    ///
    /// These overlay tests script their events before the driver mints the
    /// command id, so they cannot name the read. `None` is the honest wire
    /// shape for that (a Core predating GUI-CORE-024), and it exercises the
    /// legacy awaiting-gated path; exact-id correlation is covered where it
    /// lives, in `audit_panel`'s own tests.
    fn audit_page_event(
        sequence: u64,
        records: Vec<viden_types::AuditRecord>,
        next_before: Option<viden_types::AuditCursor>,
    ) -> RuntimeEventEnvelope {
        event(
            sequence,
            RuntimeEventKind::AuditPageLoaded {
                command_id: None,
                page: viden_types::AuditPage {
                    complete: next_before.is_none(),
                    records,
                    next_before,
                },
            },
        )
    }

    fn audit_query_of(envelope: &RuntimeCommandEnvelope) -> &viden_types::AuditQuery {
        match &envelope.command {
            RuntimeCommand::QueryAudit { query } => query,
            other => panic!("expected QueryAudit, got {other:?}"),
        }
    }

    fn audit_rows(state: &TuiState) -> String {
        crate::tui::modal::overlay_rows_for_test(state, OverlayKind::AuditTimeline).join("\n")
    }

    #[test]
    fn opening_the_timeline_scopes_the_query_to_the_record_or_to_the_whole_project() {
        let mut view = RuntimeViewState::new(supervision_snapshot());
        view.merge_gates.push(supervision_gate(
            viden_types::MergeGateStatus::CollectingEvidence,
        ));
        view.review_requests.push(supervision_review(
            viden_types::ReviewRequestStatus::Pending,
        ));
        view.merge_gates[0].validator = Some(viden_types::MergeGateValidator {
            owner: supervision_owner("lane-b"),
            review_request_id: "review-1".to_string(),
            independent: true,
            validated_at: None,
        });
        let (mut driver, mut state, sent) = supervision_driver(view, Vec::new());

        // A gate target audits the gate object, by the contract's own kind key.
        open_supervision_decision(
            &mut state,
            SupervisionTarget::Gate {
                gate_id: "gate-1".to_string(),
            },
        );
        let rows =
            crate::tui::modal::overlay_rows_for_test(&state, OverlayKind::SupervisionDecision)
                .join("\n");
        assert!(
            rows.contains("Audit trail"),
            "the audit row is offered on the decision overlay: {rows}"
        );
        // Accept / Reject / Evidence… / Audit trail: both reads are appended
        // last, so neither renumbers a decision.
        press(&mut driver, &mut state, KeyCode::Char('4'));
        press(&mut driver, &mut state, KeyCode::Enter);

        assert_eq!(
            state.ui.overlay.as_ref().map(|overlay| overlay.kind),
            Some(OverlayKind::AuditTimeline)
        );
        assert!(
            state.ui.supervision.is_none(),
            "the decision panel is released when the read takes the overlay"
        );
        let envelopes = sent.lock().expect("sent").clone();
        assert_eq!(envelopes.len(), 1);
        assert_eq!(
            audit_query_of(&envelopes[0]),
            &viden_types::AuditQuery {
                object: Some(viden_types::AuditObjectRef::new(
                    viden_types::AuditObjectRef::KIND_MERGE_GATE,
                    "gate-1"
                )),
                limit: 100,
                // The overlay ships no filter control yet, so it never sends an
                // actor or time filter the operator did not choose.
                ..viden_types::AuditQuery::default()
            }
        );
        assert!(audit_rows(&state).contains("SCOPE merge_gate:gate-1"));

        // A review target audits the review request instead.
        open_supervision_decision(
            &mut state,
            SupervisionTarget::Review {
                review_id: "review-1".to_string(),
            },
        );
        press(&mut driver, &mut state, KeyCode::Char('4'));
        press(&mut driver, &mut state, KeyCode::Enter);
        assert_eq!(
            audit_query_of(&sent.lock().expect("sent")[1]).object,
            Some(viden_types::AuditObjectRef::new(
                viden_types::AuditObjectRef::KIND_REVIEW_REQUEST,
                "review-1"
            ))
        );

        // The Decision Center footer pick opens the project timeline unscoped.
        let mut overlay = OverlayState::new(OverlayKind::Decisions);
        overlay.selected = decision_picks(&state.runtime, false).len() - 1;
        state.ui.overlay = Some(overlay);
        press(&mut driver, &mut state, KeyCode::Enter);
        let envelopes = sent.lock().expect("sent").clone();
        assert_eq!(envelopes.len(), 3);
        let query = audit_query_of(&envelopes[2]);
        assert_eq!(query.object, None);
        assert_eq!(query.project_id, None);
        assert_eq!(query.lane_id, None);
        assert!(audit_rows(&state).contains("SCOPE project timeline"));
    }

    #[test]
    fn the_first_page_replaces_older_pages_append_and_the_footer_states_what_remains() {
        let view = RuntimeViewState::new(supervision_snapshot());
        let (mut driver, mut state, sent) = supervision_driver(
            view,
            vec![
                audit_page_event(
                    1,
                    vec![
                        audit_record(
                            "a-3",
                            3_723,
                            "gate.decided",
                            viden_types::AuditOutcome::Success,
                        ),
                        audit_record(
                            "a-2",
                            3_722,
                            "handoff.created",
                            viden_types::AuditOutcome::Denied,
                        ),
                    ],
                    Some(viden_types::AuditCursor {
                        timestamp: 3_722,
                        audit_id: "a-2".to_string(),
                    }),
                ),
                audit_page_event(
                    2,
                    vec![audit_record(
                        "a-1",
                        3_721,
                        "change.reverted",
                        viden_types::AuditOutcome::Failed,
                    )],
                    None,
                ),
            ],
        );
        open_audit_timeline(&mut driver, &mut state, None).expect("open");

        // Loading is its own state: nothing has arrived, so nothing may claim
        // the timeline is empty.
        let loading = audit_rows(&state);
        assert!(loading.contains("Loading the audit page"), "{loading}");
        assert!(!loading.contains("no audit record"), "{loading}");

        driver.pump().expect("first page");
        observe_driver_events(&mut state, &mut driver).expect("observe first page");
        let first = audit_rows(&state);
        assert!(first.contains("01:02:03 gate.decided ✓"), "{first}");
        assert!(first.contains("01:02:02 handoff.created ✗"), "{first}");
        assert!(first.contains("merge_gate:gate-1"), "{first}");
        assert!(first.contains("decision=accepted"), "{first}");
        assert!(first.contains("Load older records"), "{first}");
        assert!(first.contains("LOADED 2 · older records remain"), "{first}");
        assert!(
            !first.contains("Loading the audit page"),
            "the page arrived: {first}"
        );

        // Select the load-older row and confirm it.
        press(&mut driver, &mut state, KeyCode::Down);
        press(&mut driver, &mut state, KeyCode::Down);
        press(&mut driver, &mut state, KeyCode::Enter);
        assert_eq!(
            audit_query_of(&sent.lock().expect("sent")[1]).before,
            Some(viden_types::AuditCursor {
                timestamp: 3_722,
                audit_id: "a-2".to_string(),
            }),
            "paging asks for records older than the cursor Core handed back"
        );

        driver.pump().expect("older page");
        observe_driver_events(&mut state, &mut driver).expect("observe older page");
        let complete = audit_rows(&state);
        let order = ["gate.decided", "handoff.created", "change.reverted"].map(|action| {
            complete
                .find(action)
                .unwrap_or_else(|| panic!("{action} missing"))
        });
        assert!(
            order[0] < order[1] && order[1] < order[2],
            "records stay newest-first, so an older page appends: {complete}"
        );
        assert!(
            !complete.contains("Load older records"),
            "a complete timeline hides the load-older row: {complete}"
        );
        assert!(
            complete.contains("LOADED 3 · nothing older matches"),
            "{complete}"
        );
    }

    #[test]
    fn a_rejected_query_shows_cores_reason_and_a_page_nobody_asked_for_is_ignored() {
        let view = RuntimeViewState::new(supervision_snapshot());
        let (mut driver, mut state, _sent) = supervision_driver(
            view,
            vec![
                event(
                    1,
                    RuntimeEventKind::CommandRejected {
                        command_id: "tui-other".to_string(),
                        reason: "another client's query".to_string(),
                    },
                ),
                event(
                    2,
                    RuntimeEventKind::CommandRejected {
                        command_id: "tui-1".to_string(),
                        reason: "audit store is unreadable".to_string(),
                    },
                ),
                audit_page_event(
                    3,
                    vec![audit_record(
                        "a-9",
                        3_723,
                        "gate.decided",
                        viden_types::AuditOutcome::Success,
                    )],
                    None,
                ),
            ],
        );
        open_audit_timeline(&mut driver, &mut state, None).expect("open");

        driver.pump().expect("foreign rejection");
        observe_driver_events(&mut state, &mut driver).expect("observe foreign rejection");
        assert!(
            audit_rows(&state).contains("Loading the audit page"),
            "a rejection for another command id leaves this query in flight"
        );

        driver.pump().expect("rejection");
        observe_driver_events(&mut state, &mut driver).expect("observe rejection");
        let rejected = audit_rows(&state);
        assert!(
            rejected.contains("audit store is unreadable"),
            "Core's reason is rendered verbatim: {rejected}"
        );
        assert!(rejected.contains("✗"), "{rejected}");
        assert!(!rejected.contains("Loading the audit page"), "{rejected}");

        // Nothing is in flight now, so the page that follows belongs to some
        // other reader and must not appear on this surface.
        driver.pump().expect("foreign page");
        observe_driver_events(&mut state, &mut driver).expect("observe foreign page");
        let after = audit_rows(&state);
        assert!(!after.contains("gate.decided"), "{after}");
        assert!(
            after.contains("audit store is unreadable"),
            "the error still stands: {after}"
        );
    }

    #[test]
    fn a_second_page_request_while_one_is_in_flight_sends_nothing() {
        let view = RuntimeViewState::new(supervision_snapshot());
        let (mut driver, mut state, sent) = supervision_driver(
            view,
            vec![audit_page_event(
                1,
                vec![audit_record(
                    "a-2",
                    3_722,
                    "gate.decided",
                    viden_types::AuditOutcome::Success,
                )],
                Some(viden_types::AuditCursor {
                    timestamp: 3_722,
                    audit_id: "a-2".to_string(),
                }),
            )],
        );
        open_audit_timeline(&mut driver, &mut state, None).expect("open");
        driver.pump().expect("first page");
        observe_driver_events(&mut state, &mut driver).expect("observe first page");

        press(&mut driver, &mut state, KeyCode::Down);
        press(&mut driver, &mut state, KeyCode::Enter);
        assert_eq!(sent.lock().expect("sent").len(), 2);

        // The second page has not arrived, so a third request is refused here
        // rather than racing two uncorrelated pages onto one panel.
        press(&mut driver, &mut state, KeyCode::Enter);
        assert_eq!(
            sent.lock().expect("sent").len(),
            2,
            "a busy panel must not send a second query"
        );
        let busy = audit_rows(&state);
        assert!(busy.contains("Nothing was sent"), "{busy}");
    }

    #[test]
    fn confirming_a_record_row_does_nothing_and_escape_closes_to_the_base_state() {
        let view = RuntimeViewState::new(supervision_snapshot());
        let (mut driver, mut state, sent) = supervision_driver(
            view,
            vec![audit_page_event(
                1,
                vec![audit_record(
                    "a-1",
                    3_723,
                    "gate.decided",
                    viden_types::AuditOutcome::Success,
                )],
                None,
            )],
        );
        open_audit_timeline(&mut driver, &mut state, None).expect("open");
        driver.pump().expect("page");
        observe_driver_events(&mut state, &mut driver).expect("observe page");

        // Read-only in this version: a record row carries no action.
        press(&mut driver, &mut state, KeyCode::Enter);
        assert_eq!(sent.lock().expect("sent").len(), 1);
        assert_eq!(
            state.ui.overlay.as_ref().map(|overlay| overlay.kind),
            Some(OverlayKind::AuditTimeline)
        );

        // The TUI keeps an overlay return path only for Global Jump, so Esc
        // unwinds to the base state exactly as the Approval overlay opened from
        // the Decision Center does. The page is dropped with the panel.
        press(&mut driver, &mut state, KeyCode::Esc);
        assert!(state.ui.overlay.is_none());
        assert!(state.ui.audit.is_none());
    }

    #[test]
    fn the_audit_timeline_is_readable_in_plan_mode() {
        let mut snapshot = supervision_snapshot();
        snapshot.work_mode = WorkMode::Plan;
        let (mut driver, mut state, sent) = supervision_driver(
            RuntimeViewState::new(snapshot),
            vec![audit_page_event(
                1,
                vec![audit_record(
                    "a-1",
                    3_723,
                    "gate.decided",
                    viden_types::AuditOutcome::Success,
                )],
                None,
            )],
        );

        // `QueryAudit` mutates nothing, so Plan mode neither blocks the dispatch
        // nor changes the page (`runtime_contract.rs`: the read path takes no
        // transaction snapshot and never reaches the approval prompt).
        open_audit_timeline(&mut driver, &mut state, None).expect("open in plan mode");
        assert_eq!(state.runtime.snapshot.work_mode, WorkMode::Plan);
        assert_eq!(sent.lock().expect("sent").len(), 1);

        driver.pump().expect("page");
        observe_driver_events(&mut state, &mut driver).expect("observe page");
        let rows = audit_rows(&state);
        assert!(rows.contains("01:02:03 gate.decided ✓"), "{rows}");
        assert!(rows.contains("LOADED 1"), "{rows}");
    }

    #[test]
    fn composer_stays_editable_while_the_audit_timeline_is_open_during_a_stream() {
        let view = RuntimeViewState::new(supervision_snapshot());
        let (mut driver, mut state, sent) = supervision_driver(
            view,
            vec![
                event(
                    1,
                    RuntimeEventKind::AssistantDelta {
                        message_id: "assistant-1".to_string(),
                        task_id: None,
                        session_id: None,
                        content: "working".to_string(),
                    },
                ),
                audit_page_event(
                    2,
                    vec![audit_record(
                        "a-1",
                        3_723,
                        "gate.decided",
                        viden_types::AuditOutcome::Success,
                    )],
                    None,
                ),
            ],
        );
        driver.pump().expect("stream event");
        project_runtime_view(&mut state, driver.view(), driver.cursor());
        open_audit_timeline(&mut driver, &mut state, None).expect("open");

        // The overlay has no text filter: every printable character keeps
        // editing the composer, so a streaming turn stays answerable while the
        // operator reads history.
        type_text(&mut driver, &mut state, "你好");
        assert_eq!(state.ui.input, "你好");
        press(&mut driver, &mut state, KeyCode::Backspace);
        assert_eq!(state.ui.input, "你");
        assert_eq!(driver.view().assistant_stream, "working");
        assert_eq!(
            state.ui.overlay.as_ref().map(|overlay| overlay.kind),
            Some(OverlayKind::AuditTimeline)
        );
        assert!(
            state
                .ui
                .overlay
                .as_ref()
                .is_some_and(|overlay| overlay.filter.is_empty()),
            "typing must never become an overlay filter here"
        );
        assert_eq!(sent.lock().expect("sent").len(), 1);

        driver.pump().expect("page");
        observe_driver_events(&mut state, &mut driver).expect("observe page");
        assert!(audit_rows(&state).contains("gate.decided"));
        assert_eq!(state.ui.input, "你");
    }

    // ---- evidence inspector -------------------------------------------------

    fn evidence_view(id: &str, kind: &str, timestamp: Option<u64>) -> viden_core::EvidenceView {
        viden_core::EvidenceView {
            id: id.to_string(),
            kind: kind.to_string(),
            summary: format!("{kind} recorded as {id}"),
            path: None,
            source: Some("lane-a".to_string()),
            canonical: None,
            metadata: None,
            timestamp,
            owner: Some(supervision_owner("lane-a")),
        }
    }

    fn evidence_page_event(
        sequence: u64,
        command_id: &str,
        entries: Vec<viden_core::EvidenceView>,
        next_after: Option<&str>,
    ) -> RuntimeEventEnvelope {
        event(
            sequence,
            RuntimeEventKind::EvidencePageLoaded {
                command_id: command_id.to_string(),
                page: viden_core::EvidencePage {
                    complete: next_after.is_none(),
                    entries,
                    next_after: next_after.map(str::to_string),
                },
            },
        )
    }

    fn evidence_query_of(envelope: &RuntimeCommandEnvelope) -> &viden_core::EvidenceQuery {
        match &envelope.command {
            RuntimeCommand::QueryEvidence { query } => query,
            other => panic!("expected QueryEvidence, got {other:?}"),
        }
    }

    fn evidence_rows_of(state: &TuiState) -> String {
        crate::tui::modal::overlay_rows_for_test(state, OverlayKind::EvidenceInspector).join("\n")
    }

    /// The overlay reads the archive Core owns: the scope is the record's own
    /// published owner, the second page is asked for with Core's own cursor
    /// verbatim, and the in-flight supervision decision is neither blocked by
    /// the read nor settled by its answer.
    #[test]
    fn the_evidence_inspector_scopes_to_the_record_and_pages_with_cores_own_cursor() {
        let mut view = RuntimeViewState::new(supervision_snapshot());
        view.merge_gates.push(supervision_gate(
            viden_types::MergeGateStatus::CollectingEvidence,
        ));
        let (mut driver, mut state, sent) = supervision_driver(
            view,
            vec![
                evidence_page_event(
                    1,
                    "tui-1",
                    vec![
                        evidence_view("ev-undated", "patch", None),
                        evidence_view("ev-dated", "test_result", Some(1_700_000_100)),
                    ],
                    Some("t:1700000100:ev-dated"),
                ),
                evidence_page_event(
                    2,
                    "tui-2",
                    vec![evidence_view("ev-newer", "patch", Some(1_700_100_000))],
                    None,
                ),
            ],
        );
        state.capabilities = driver.capabilities();

        open_supervision_decision(
            &mut state,
            SupervisionTarget::Gate {
                gate_id: "gate-1".to_string(),
            },
        );
        let rows =
            crate::tui::modal::overlay_rows_for_test(&state, OverlayKind::SupervisionDecision)
                .join("\n");
        assert!(
            rows.contains("Evidence…"),
            "the evidence row is offered on the decision overlay: {rows}"
        );

        // Accept / Reject / Evidence… : the reads are appended after every
        // decision, so opening one never renumbers a decision.
        press(&mut driver, &mut state, KeyCode::Char('3'));
        press(&mut driver, &mut state, KeyCode::Enter);

        assert_eq!(
            state.ui.overlay.as_ref().map(|overlay| overlay.kind),
            Some(OverlayKind::EvidenceInspector)
        );
        let envelopes = sent.lock().expect("sent").clone();
        assert_eq!(envelopes.len(), 1);
        assert_eq!(
            evidence_query_of(&envelopes[0]),
            &viden_core::EvidenceQuery {
                owner: Some(supervision_owner("lane-a")),
                kinds: Vec::new(),
                limit: viden_core::DEFAULT_EVIDENCE_PAGE_SIZE,
                after: None,
            },
            "the scope is the gate's own Core-published owner"
        );

        driver.pump().expect("first page");
        observe_driver_events(&mut state, &mut driver).expect("observe first page");
        let rendered = evidence_rows_of(&state);
        assert!(rendered.contains("UNDATED"), "{rendered}");
        assert!(rendered.contains("2023-11-14"), "{rendered}");
        assert!(rendered.contains("Load the next page"), "{rendered}");
        assert!(rendered.contains("· more"), "{rendered}");

        // Selecting the load-more row asks Core for the page after its cursor,
        // passed back exactly as Core issued it.
        for _ in 0..2 {
            press(&mut driver, &mut state, KeyCode::Down);
        }
        press(&mut driver, &mut state, KeyCode::Enter);
        let envelopes = sent.lock().expect("sent").clone();
        assert_eq!(envelopes.len(), 2);
        assert_eq!(
            evidence_query_of(&envelopes[1]).after.as_deref(),
            Some("t:1700000100:ev-dated")
        );
        assert_eq!(
            evidence_query_of(&envelopes[1]).owner,
            Some(supervision_owner("lane-a")),
            "paging must not widen the scope the operator opened"
        );

        driver.pump().expect("second page");
        observe_driver_events(&mut state, &mut driver).expect("observe second page");
        let rendered = evidence_rows_of(&state);
        assert!(rendered.contains("LOADED 3"), "{rendered}");
        assert!(rendered.contains("archive complete"), "{rendered}");
    }

    /// `/evidence` narrows to the focused Lane by the id Core published and
    /// nothing else, and `Enter` on a row reads that row's canonical bytes.
    #[test]
    fn the_evidence_command_scopes_to_the_focused_lane_and_reads_one_rows_content() {
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                events: VecDeque::from(vec![
                    evidence_page_event(
                        1,
                        "tui-1",
                        vec![evidence_view(
                            "ev-tests",
                            "test_result",
                            Some(1_700_000_100),
                        )],
                        None,
                    ),
                    event(
                        2,
                        RuntimeEventKind::EvidenceContentLoaded {
                            command_id: "tui-2".to_string(),
                            evidence_id: "ev-tests".to_string(),
                            content: viden_core::EvidenceContent::Text {
                                text: "test result: ok. 3 passed; 0 failed".to_string(),
                                truncated: false,
                                sha256: "bbbbbbbbcccccccc".to_string(),
                            },
                        },
                    ),
                ]),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState {
            capabilities: driver.capabilities(),
            ..TuiState::default()
        };
        let (lane_id, _) = focus_lane_with_published_owner(&mut state);
        state.ui.input = "/evidence".into();

        submit_composer(&mut driver, &mut state).expect("open the evidence inspector");

        assert_eq!(
            state.ui.overlay.as_ref().map(|overlay| overlay.kind),
            Some(OverlayKind::EvidenceInspector)
        );
        assert_eq!(
            evidence_query_of(&sent.lock().expect("sent")[0]).owner,
            Some(RuntimeOwner {
                lane_id: Some(lane_id),
                ..RuntimeOwner::default()
            }),
            "only the Lane id Core published is set; every other field stays a wildcard"
        );

        driver.pump().expect("page");
        observe_driver_events(&mut state, &mut driver).expect("observe page");
        press(&mut driver, &mut state, KeyCode::Enter);

        let commands = sent.lock().expect("sent").clone();
        assert_eq!(commands.len(), 2);
        assert!(matches!(
            &commands[1].command,
            RuntimeCommand::ReadEvidenceContent { evidence_id } if evidence_id == "ev-tests"
        ));
        let reading = evidence_rows_of(&state);
        assert!(
            reading.contains("Reading the canonical content"),
            "{reading}"
        );

        driver.pump().expect("content");
        observe_driver_events(&mut state, &mut driver).expect("observe content");
        let detail = evidence_rows_of(&state);
        assert!(detail.contains("test result: ok. 3 passed"), "{detail}");
        assert!(detail.contains("bbbbbbbb"), "{detail}");

        // A second Enter on the same row re-renders the cached bytes instead of
        // asking Core again.
        press(&mut driver, &mut state, KeyCode::Esc);
        press(&mut driver, &mut state, KeyCode::Enter);
        assert_eq!(
            sent.lock().expect("sent").len(),
            2,
            "content is cached for this overlay's lifetime"
        );
        // Esc unwinds the detail pane first, then the overlay.
        press(&mut driver, &mut state, KeyCode::Esc);
        assert_eq!(
            state.ui.overlay.as_ref().map(|overlay| overlay.kind),
            Some(OverlayKind::EvidenceInspector)
        );
        press(&mut driver, &mut state, KeyCode::Esc);
        assert!(state.ui.overlay.is_none());
        assert!(
            state.ui.evidence.is_none(),
            "closing drops the page and the content cache, so reopening re-reads"
        );
    }

    /// Recorded evidence is a fact about the recent window, not a page of the
    /// archive. The overlay says the page it holds is now behind and reloads
    /// only when the operator asks.
    #[test]
    fn recorded_evidence_marks_the_open_inspector_stale_and_r_reloads_page_one() {
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                events: VecDeque::from(vec![
                    evidence_page_event(
                        1,
                        "tui-1",
                        vec![evidence_view("ev-1", "patch", Some(1_700_000_100))],
                        None,
                    ),
                    event(
                        2,
                        RuntimeEventKind::EvidenceRecorded {
                            evidence: evidence_view("ev-2", "patch", Some(1_700_000_200)),
                        },
                    ),
                ]),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState {
            capabilities: driver.capabilities(),
            ..TuiState::default()
        };
        state.ui.input = "/evidence".into();
        submit_composer(&mut driver, &mut state).expect("open the evidence inspector");
        driver.pump().expect("page");
        observe_driver_events(&mut state, &mut driver).expect("observe page");
        driver.pump().expect("recorded");
        observe_driver_events(&mut state, &mut driver).expect("observe recorded");

        let stale = evidence_rows_of(&state);
        assert!(stale.contains("press r to reload"), "{stale}");
        assert!(
            !stale.contains("ev-2"),
            "the recent-window fact is never folded into the archive rows: {stale}"
        );

        press(&mut driver, &mut state, KeyCode::Char('r'));
        let commands = sent.lock().expect("sent").clone();
        assert_eq!(commands.len(), 2);
        assert_eq!(
            evidence_query_of(&commands[1]).after,
            None,
            "a reload restarts at the first page"
        );
        assert!(!evidence_rows_of(&state).contains("press r to reload"));
    }

    /// The kind filter is Core's, not a local narrowing: cycling it re-queries
    /// from the first page with the kind Core actually returned.
    #[test]
    fn the_kind_filter_cycles_through_kinds_core_returned_and_re_queries() {
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                events: VecDeque::from(vec![evidence_page_event(
                    1,
                    "tui-1",
                    vec![
                        evidence_view("ev-1", "patch", Some(1_700_000_100)),
                        evidence_view("ev-2", "test_result", Some(1_700_000_200)),
                    ],
                    None,
                )]),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState {
            capabilities: driver.capabilities(),
            ..TuiState::default()
        };
        state.ui.input = "/evidence".into();
        submit_composer(&mut driver, &mut state).expect("open the evidence inspector");
        driver.pump().expect("page");
        observe_driver_events(&mut state, &mut driver).expect("observe page");

        press(&mut driver, &mut state, KeyCode::Char('f'));

        let commands = sent.lock().expect("sent").clone();
        assert_eq!(commands.len(), 2);
        assert_eq!(evidence_query_of(&commands[1]).kinds, vec!["patch"]);
        assert_eq!(evidence_query_of(&commands[1]).after, None);
        let rows = evidence_rows_of(&state);
        assert!(rows.contains("FILTER kind patch"), "{rows}");
        // The composer is untouched: `f` belongs to the overlay, and every
        // other printable character keeps editing the turn.
        assert!(state.ui.input.as_str().is_empty());
        press(&mut driver, &mut state, KeyCode::Char('x'));
        assert_eq!(state.ui.input.as_str(), "x");
    }

    /// Without the capability nothing is sent and no overlay opens: the gap is
    /// stated, and the `/evidence` row stays listed and labelled in the jump
    /// index rather than disappearing.
    #[test]
    fn without_the_capability_the_evidence_read_sends_nothing_and_names_it() {
        let mut capabilities = viden_core::frontend_capabilities();
        capabilities.remove(&CapabilityId(EVIDENCE_READS_CAPABILITY.to_string()));
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                capabilities: Some(capabilities),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState {
            capabilities: driver.capabilities(),
            ..TuiState::default()
        };
        state.ui.input = "/evidence".into();

        submit_composer(&mut driver, &mut state).expect("refuse the evidence read");

        assert!(state.ui.overlay.is_none());
        assert!(state.ui.evidence.is_none());
        assert!(sent.lock().expect("sent").is_empty());
        let entry = state.ui.entries.last().expect("a stated refusal");
        assert!(entry.body.contains(EVIDENCE_READS_CAPABILITY), "{entry:?}");

        let row = JumpIndex::from_state(&state)
            .items()
            .iter()
            .find(|item| item.id == "/evidence")
            .expect("the row stays listed")
            .clone();
        assert!(!row.enabled);
        assert!(
            row.disabled_reason
                .as_deref()
                .is_some_and(|reason| reason.contains(EVIDENCE_READS_CAPABILITY))
        );
    }

    #[test]
    fn a_pending_supervision_decision_neither_blocks_nor_is_settled_by_an_audit_read() {
        let mut view = RuntimeViewState::new(supervision_snapshot());
        view.merge_gates.push(supervision_gate(
            viden_types::MergeGateStatus::CollectingEvidence,
        ));
        view.latest_evidence.push(supervision_evidence("hash-1"));
        let (mut driver, mut state, sent) = supervision_driver(
            view,
            vec![audit_page_event(
                1,
                vec![audit_record(
                    "a-1",
                    3_723,
                    "gate.decided",
                    viden_types::AuditOutcome::Success,
                )],
                None,
            )],
        );
        open_supervision_decision(
            &mut state,
            SupervisionTarget::Gate {
                gate_id: "gate-1".to_string(),
            },
        );
        press(&mut driver, &mut state, KeyCode::Enter);
        assert!(state.supervision.pending().is_some());

        // Accept / Reject / Dismiss / Evidence… / Audit trail while a decision
        // is pending.
        open_supervision_decision(
            &mut state,
            SupervisionTarget::Gate {
                gate_id: "gate-1".to_string(),
            },
        );
        press(&mut driver, &mut state, KeyCode::Char('5'));
        press(&mut driver, &mut state, KeyCode::Enter);
        assert_eq!(
            state.ui.overlay.as_ref().map(|overlay| overlay.kind),
            Some(OverlayKind::AuditTimeline),
            "a read is never refused because a mutation is in flight"
        );
        assert_eq!(sent.lock().expect("sent").len(), 2);
        assert_eq!(
            state.supervision.outcome(),
            &crate::tui::pending::SupervisionOutcome::Pending {
                command_id: "tui-1".to_string()
            },
            "opening a read leaves the decision correlation untouched"
        );

        driver.pump().expect("page");
        observe_driver_events(&mut state, &mut driver).expect("observe page");
        assert!(audit_rows(&state).contains("gate.decided"));
        assert_eq!(
            state.supervision.outcome(),
            &crate::tui::pending::SupervisionOutcome::Pending {
                command_id: "tui-1".to_string()
            },
            "an audit page is not the business fact the decision is waiting for"
        );
    }

    #[test]
    fn blind_lane_wall_time_is_rendered_at_the_scale_an_operator_reads() {
        for (milliseconds, expected) in [
            (0_u64, "0 ms"),
            (999, "999 ms"),
            (1_500, "1.5 s"),
            (59_900, "59.9 s"),
            (95_000, "1m 35s"),
        ] {
            assert_eq!(
                crate::tui::modal::humanized_wall_time_for_test(milliseconds),
                expected
            );
        }
    }

    #[test]
    fn command_accepted_does_not_synthesize_success() {
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                events: VecDeque::from([event(
                    1,
                    RuntimeEventKind::CommandAccepted {
                        command_id: "command-1".to_string(),
                        command: RuntimeCommand::SubmitUserInput {
                            content: "hello".to_string(),
                        },
                    },
                )]),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        state.ui.entries.push(TuiEntry {
            label: "user".to_string(),
            body: "hello".to_string(),
        });

        let outcome = driver.pump().expect("command receipt");
        apply_pump_outcome(&mut state, &driver, outcome);

        assert_eq!(
            state.ui.entries.len(),
            1,
            "receipt must not invent transcript facts"
        );
        assert!(driver.view().last_command.is_some());
    }

    #[test]
    fn command_rejected_reason_is_rendered() {
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                events: VecDeque::from([event(
                    1,
                    RuntimeEventKind::CommandRejected {
                        command_id: "command-1".to_string(),
                        reason: "forbidden".to_string(),
                    },
                )]),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        driver.pump().expect("command rejection");
        let mut state = TuiState::default();
        project_runtime_view(&mut state, driver.view(), driver.cursor());

        assert!(
            state
                .runtime
                .errors
                .iter()
                .any(|error| error.message.contains("forbidden"))
        );
    }

    #[test]
    fn composer_stays_editable_while_events_stream() {
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                events: VecDeque::from([event(
                    1,
                    RuntimeEventKind::AssistantDelta {
                        message_id: "assistant-1".to_string(),
                        task_id: None,
                        session_id: None,
                        content: "working".to_string(),
                    },
                )]),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        driver.pump().expect("stream event");
        project_runtime_view(&mut state, driver.view(), driver.cursor());

        state.ui.input_mode = InputMode::Insert;
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('你'), KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("key");

        assert_eq!(state.ui.input, "你");
        assert_eq!(driver.view().assistant_stream, "working");
    }

    fn event(sequence: u64, kind: RuntimeEventKind) -> RuntimeEventEnvelope {
        RuntimeEventEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            owner: Default::default(),
            cursor: EventCursor {
                stream_id: "fixture".to_string(),
                sequence,
            },
            event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
                sequence,
                Some(sequence),
                kind,
            )),
        }
    }

    #[test]
    fn focus_and_paste_events_force_repaint_without_becoming_input() {
        let mut driver =
            TuiClientDriver::connect(StatefulCoreClient::new(FakeCoreTransport::default()))
                .expect("connect");
        let mut state = TuiState::default();
        assert_eq!(
            handle_ui_event(
                &mut driver,
                &mut state,
                Event::Key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)),
                (120, 40),
            )
            .expect("insert mode"),
            UiEventOutcome::Redraw
        );
        assert_eq!(state.ui.input_mode, InputMode::Insert);
        assert_eq!(
            handle_ui_event(
                &mut driver,
                &mut state,
                Event::Paste("first\nsecond".to_string()),
                (120, 40),
            )
            .expect("paste"),
            UiEventOutcome::Redraw
        );
        assert_eq!(state.ui.input, "first\nsecond");
        assert!(
            !super::super::state::has_active_work(&state),
            "paste must never submit"
        );

        for event in [Event::FocusLost, Event::FocusGained, Event::Resize(100, 30)] {
            assert_eq!(
                handle_ui_event(&mut driver, &mut state, event, (100, 30)).expect("repaint event"),
                UiEventOutcome::Redraw
            );
        }
        assert_eq!(state.ui.input, "first\nsecond");
    }

    #[test]
    fn paste_normalizes_crlf_preserves_leading_slash_and_never_submits() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        state.ui.input_mode = InputMode::Insert;

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Paste("/status\r\nnext\rline".to_string()),
            (120, 40),
        )
        .expect("paste");

        assert_eq!(state.ui.input, "/status\nnext\nline");
        assert!(sent.lock().expect("sent commands").is_empty());
    }

    #[test]
    fn enter_inside_unclosed_code_fence_inserts_newline_without_scrollback_effects() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        state.ui.input_mode = InputMode::Insert;
        state.ui.input = "```rust\nfn main() {}".into();
        state.ui.transcript_scroll = 17;

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("enter");

        assert_eq!(state.ui.input, "```rust\nfn main() {}\n");
        assert_eq!(state.ui.transcript_scroll, 17);
        assert!(sent.lock().expect("sent commands").is_empty());
    }

    #[test]
    fn shift_and_alt_enter_insert_newlines_without_submitting() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        state.ui.input_mode = InputMode::Insert;
        state.ui.input = "first".into();

        for modifiers in [KeyModifiers::SHIFT, KeyModifiers::ALT] {
            handle_ui_event(
                &mut driver,
                &mut state,
                Event::Key(KeyEvent::new(KeyCode::Enter, modifiers)),
                (120, 40),
            )
            .expect("modified enter");
        }

        assert_eq!(state.ui.input, "first\n\n");
        assert!(sent.lock().expect("sent commands").is_empty());
    }

    #[test]
    fn modified_enter_does_not_complete_command_palette_or_interaction_filter() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        state.ui.input_mode = InputMode::Insert;
        state.ui.input = "/con".into();

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT)),
            (120, 40),
        )
        .expect("command palette modified enter");

        assert_eq!(state.ui.input, "/con");
        assert!(state.ui.interaction_panel.is_none());

        state.ui.provider_catalog = crate::tui::state::ProviderOption::fixture();
        state.ui.interaction_panel = Some(InteractionPanel::ConnectProvider {
            search: "deep".to_string(),
            selected: 0,
        });
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT)),
            (120, 40),
        )
        .expect("interaction modified enter");

        assert!(matches!(
            state.ui.interaction_panel,
            Some(InteractionPanel::ConnectProvider { ref search, selected })
                if search == "deep" && selected == 0
        ));
        assert!(sent.lock().expect("sent commands").is_empty());
    }

    #[test]
    fn modified_enter_edits_approval_composer_without_resolving_approval() {
        let mut view = RuntimeViewState::new(RuntimeSnapshot {
            cwd: PathBuf::from("/workspace"),
            provider_family: "fallback".to_string(),
            model_label: "test-local".to_string(),
            work_mode: WorkMode::Build,
            permission_mode: PermissionMode::Default,
            permission_level: PermissionLevel::Ask,
            config_summary: "fixture".to_string(),
            loaded_config_files: Vec::new(),
            startup_overrides: Vec::new(),
            ui_preferences: Default::default(),
        });
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/approval-allow-deny.json"
        ))
        .expect("approval fixture");
        let envelope: RuntimeEventEnvelope =
            serde_json::from_value(fixture["events"][0].clone()).expect("approval event");
        if let viden_types::RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                view: Some(view),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        project_runtime_view(&mut state, driver.view(), driver.cursor());
        state.ui.input_mode = InputMode::Insert;
        state.ui.input = "explain".into();

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT)),
            (120, 40),
        )
        .expect("approval modified enter");

        assert_eq!(state.ui.input, "explain\n");
        assert_eq!(driver.view().pending_approvals.len(), 1);
        assert!(sent.lock().expect("sent commands").is_empty());
    }

    #[test]
    fn narrow_welcome_vertical_motion_uses_rendered_cjk_emoji_width() {
        for (width, repeats) in [(40_u16, 5), (60, 10), (79, 14)] {
            let client = FakeCoreClient::default();
            let mut driver = TuiClientDriver::connect(client).expect("connect");
            let mut state = TuiState::default();
            state.ui.input_mode = InputMode::Insert;
            let line = "你👨‍👩‍👧‍👦".repeat(repeats);
            state.ui.input = format!("{line}\n{line}\n{line}").into();
            let before = super::super::composer::composer_cursor_position(
                &state,
                width,
                40,
                super::super::statusbar::BOTTOM_BAR_HEIGHT,
            );

            handle_ui_event(
                &mut driver,
                &mut state,
                Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
                (width, 40),
            )
            .expect("narrow up");
            let after = super::super::composer::composer_cursor_position(
                &state,
                width,
                40,
                super::super::statusbar::BOTTOM_BAR_HEIGHT,
            );

            assert_eq!(after.1 + 1, before.1, "width {width}");
            assert_eq!(after.0, before.0, "width {width}");

            handle_ui_event(
                &mut driver,
                &mut state,
                Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
                (width, 40),
            )
            .expect("narrow down");
            assert_eq!(
                super::super::composer::composer_cursor_position(
                    &state,
                    width,
                    40,
                    super::super::statusbar::BOTTOM_BAR_HEIGHT,
                ),
                before,
                "width {width}"
            );
        }
    }

    #[test]
    fn plain_enter_submits_a_closed_composer() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        state.ui.input_mode = InputMode::Insert;
        state.ui.input = "review this".into();

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("submit");

        assert!(state.ui.input.is_empty());
        assert_eq!(state.ui.lens, Lens::Session);
        let sent = sent.lock().expect("sent commands");
        assert!(matches!(
            sent.first().map(|command| &command.command),
            Some(RuntimeCommand::SubmitUserInput { content }) if content == "review this"
        ));
    }

    #[test]
    fn composer_discards_terminal_escape_residue_instead_of_rendering_it() {
        let mut driver =
            TuiClientDriver::connect(StatefulCoreClient::new(FakeCoreTransport::default()))
                .expect("connect");
        let mut state = TuiState::default();
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("insert mode");

        for value in "2;28;95;132m".chars() {
            handle_ui_event(
                &mut driver,
                &mut state,
                Event::Key(KeyEvent::new(KeyCode::Char(value), KeyModifiers::NONE)),
                (120, 40),
            )
            .expect("composer key");
        }

        assert!(state.ui.input.is_empty());
    }

    #[test]
    fn transcript_scroll_and_normal_insert_escape_survive_core_projection() {
        let mut driver =
            TuiClientDriver::connect(StatefulCoreClient::new(FakeCoreTransport::default()))
                .expect("connect");
        let mut state = TuiState::default();
        state.ui.transcript_scroll = 18;

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("scroll");
        assert_eq!(state.ui.transcript_scroll, 30);

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("insert mode");
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('草'), KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("draft");
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("leave insert");
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("normal key");

        assert_eq!(state.ui.input_mode, InputMode::Normal);
        assert_eq!(
            state.ui.input, "草",
            "Esc preserves the draft and Normal ignores x"
        );
        project_runtime_view(&mut state, driver.view(), driver.cursor());
        assert_eq!(
            state.ui.transcript_scroll, 30,
            "Core projection keeps scrollback"
        );
    }

    #[test]
    fn streaming_delta_does_not_steal_scrollback_when_user_scrolled_up() {
        let mut driver =
            TuiClientDriver::connect(StatefulCoreClient::new(FakeCoreTransport::default()))
                .expect("connect");
        let mut state = TuiState::default();
        state.ui.transcript_scroll = 18;
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("insert mode");

        project_runtime_view(&mut state, driver.view(), driver.cursor());

        assert_eq!(state.ui.transcript_scroll, 18);
        assert_eq!(state.ui.input_mode, InputMode::Insert);
    }

    #[test]
    fn runtime_provider_turn_starts_without_blocking_ui_thread() {
        let mut driver =
            TuiClientDriver::connect(StatefulCoreClient::new(FakeCoreTransport::default()))
                .expect("connect");
        let mut state = TuiState::default();
        state.runtime.active_tool_calls.push(ToolCallView {
            tool_call_id: "tool-1".to_string(),
            name: "slow".to_string(),
            input_preview: "request".to_string(),
            owner: None,
        });
        state.ui.input_mode = InputMode::Insert;

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('继'), KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("composer remains responsive");

        assert_eq!(state.ui.input, "继");
        assert!(super::super::state::has_active_work(&state));
    }

    #[test]
    fn active_approval_does_not_swallow_composer_typing() {
        let mut view = RuntimeViewState::new(RuntimeSnapshot {
            cwd: PathBuf::from("/workspace"),
            provider_family: "fallback".to_string(),
            model_label: "test-local".to_string(),
            work_mode: WorkMode::Build,
            permission_mode: PermissionMode::Default,
            permission_level: PermissionLevel::Ask,
            config_summary: "fixture".to_string(),
            loaded_config_files: Vec::new(),
            startup_overrides: Vec::new(),
            ui_preferences: Default::default(),
        });
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/approval-allow-deny.json"
        ))
        .expect("approval fixture");
        let envelope: RuntimeEventEnvelope =
            serde_json::from_value(fixture["events"][0].clone()).expect("approval event");
        if let viden_types::RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
        let mut driver = TuiClientDriver::connect(StatefulCoreClient::new(FakeCoreTransport {
            view: Some(view),
            ..FakeCoreTransport::default()
        }))
        .expect("connect");
        let mut state = TuiState::default();
        state.ui.input_mode = InputMode::Insert;

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("approval-time typing");

        assert_eq!(state.ui.input, "x");
        assert_eq!(driver.view().pending_approvals.len(), 1);

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Paste("\nsecond line".to_string()),
            (120, 40),
        )
        .expect("approval-time bracketed paste");

        assert_eq!(state.ui.input, "x\nsecond line");
        assert_eq!(driver.view().pending_approvals.len(), 1);
    }

    #[test]
    fn owner_scoped_cancel_uses_the_exact_live_lane_owner_without_denying_approval() {
        let mut view = RuntimeViewState::new(RuntimeSnapshot {
            cwd: PathBuf::from("/workspace"),
            provider_family: "fallback".to_string(),
            model_label: "test-local".to_string(),
            work_mode: WorkMode::Build,
            permission_mode: PermissionMode::Default,
            permission_level: PermissionLevel::Ask,
            config_summary: "fixture".to_string(),
            loaded_config_files: Vec::new(),
            startup_overrides: Vec::new(),
            ui_preferences: Default::default(),
        });
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/approval-allow-deny.json"
        ))
        .expect("approval fixture");
        let envelope: RuntimeEventEnvelope =
            serde_json::from_value(fixture["events"][0].clone()).expect("approval event");
        if let viden_types::RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
        let approval_owner = RuntimeOwner {
            workspace_id: "workspace".to_string(),
            project_id: "viden".to_string(),
            lane_id: Some("approval-lane".to_string()),
            session_id: Some("approval-session".to_string()),
            task_id: Some("approval-task".to_string()),
            turn_id: Some("approval-turn".to_string()),
        };
        view.pending_approvals[0].owner = approval_owner;
        let lane = serde_json::from_str::<Vec<AgentLaneRecord>>(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes")
        .into_iter()
        .find(|lane| lane.id == "L-start")
        .expect("active lane");
        view.lanes = vec![lane];
        let owner = RuntimeOwner {
            workspace_id: "workspace".to_string(),
            project_id: "viden".to_string(),
            lane_id: Some("L-start".to_string()),
            session_id: Some("session-review".to_string()),
            task_id: Some("task-review".to_string()),
            turn_id: Some("turn-review".to_string()),
        };
        view.lane_runtime_owners = vec![viden_types::LaneRuntimeOwnerBinding {
            lane_id: "L-start".to_string(),
            owner: owner.clone(),
        }];
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                view: Some(view),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::new(driver.view().clone());
        state.ui.focus_lane("L-start".to_string());
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            (120, 40),
        )
        .expect("cancel current work");

        let sent = sent.lock().expect("sent commands");
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].owner, owner);
        assert!(matches!(sent[0].command, RuntimeCommand::CancelActiveTurn));
    }

    #[test]
    fn owner_scoped_cancel_uses_the_exact_owner_from_normal_escape() {
        let (mut driver, mut state, sent, owner) = exact_lane_owner_driver();

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("escape closes the lane detail first");
        assert!(sent.lock().expect("sent commands").is_empty());
        assert!(!state.ui.lane_detail_open);

        // Moved baseline: the Lane target is its own rung now, so unwinding to
        // "no selection" takes one more `Esc` than it used to. Nothing is sent
        // on either rung.
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("escape clears the Lane target");
        assert!(sent.lock().expect("sent commands").is_empty());
        assert!(state.ui.focused_lane.is_none());

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("second escape cancels the only active lane");

        let sent = sent.lock().expect("sent commands");
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].owner, owner);
        assert!(matches!(sent[0].command, RuntimeCommand::CancelActiveTurn));
    }

    #[test]
    fn owner_scoped_cancel_uses_the_exact_owner_from_exit_confirmation() {
        let (mut driver, mut state, sent, owner) = exact_lane_owner_driver();
        state.ui.overlay = Some(OverlayState::new(OverlayKind::ExitConfirm));

        let outcome = handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("exit confirmation cancel");

        assert_eq!(outcome, UiEventOutcome::Redraw);
        assert!(state.ui.overlay.is_none());
        let sent = sent.lock().expect("sent commands");
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].owner, owner);
        assert!(matches!(sent[0].command, RuntimeCommand::CancelActiveTurn));
    }

    #[test]
    fn active_lane_without_a_core_owner_never_dispatches_cancel_or_exits() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        state.runtime.lanes = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");

        let outcome = handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("active lane blocks plain escape exit");
        assert_eq!(outcome, UiEventOutcome::Redraw);

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            (120, 40),
        )
        .expect("fail-closed lane cancel");
        assert!(sent.lock().expect("sent commands").is_empty());
        assert!(!state.ui.idle_ctrl_c_armed);
        assert!(state.ui.overlay.is_none());

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            (120, 40),
        )
        .expect("repeated fail-closed lane cancel");
        assert!(sent.lock().expect("sent commands").is_empty());
        assert!(state.ui.overlay.is_none());

        state.ui.overlay = Some(OverlayState::new(OverlayKind::ExitConfirm));
        let rendered = super::super::render::render_frame(&state, 120, 40);
        assert!(rendered.contains("exit is blocked"));
        assert!(rendered.contains("cancellable owner"));
        assert!(!rendered.contains("Press Enter to exit"));
        let outcome = handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("active lane blocks exit confirmation");
        assert_eq!(outcome, UiEventOutcome::Redraw);
        assert!(sent.lock().expect("sent commands").is_empty());
    }

    #[test]
    fn owner_scoped_cancel_fail_closes_for_another_lane_or_stale_binding() {
        let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

        let (mut driver, mut state, sent, _) = exact_lane_owner_driver();
        state.runtime.lane_runtime_owners[0].lane_id = "other-lane".to_string();
        state.runtime.lane_runtime_owners[0].owner.lane_id = Some("other-lane".to_string());
        handle_ui_event(&mut driver, &mut state, ctrl_c.clone(), (120, 40))
            .expect("other lane remains unavailable");
        assert!(sent.lock().expect("sent commands").is_empty());
        assert!(!state.ui.idle_ctrl_c_armed);
        assert!(state.ui.overlay.is_none());

        let (mut driver, mut state, sent, _) = exact_lane_owner_driver();
        state.runtime.lanes[0].status = LaneStatus::Done;
        state.runtime.active_tool_calls.push(ToolCallView {
            tool_call_id: "tool-after-restart".to_string(),
            name: "shell".to_string(),
            input_preview: "cargo test".to_string(),
            owner: None,
        });
        handle_ui_event(&mut driver, &mut state, ctrl_c, (120, 40))
            .expect("stale binding remains unavailable");
        assert!(sent.lock().expect("sent commands").is_empty());
        assert!(!state.ui.idle_ctrl_c_armed);
        assert!(state.ui.overlay.is_none());
    }

    #[test]
    fn idle_ctrl_c_does_not_send_cancel_or_exit_directly() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(
            handle_ui_event(&mut driver, &mut state, ctrl_c.clone(), (120, 40))
                .expect("first idle Ctrl-C"),
            UiEventOutcome::Redraw
        );
        assert!(state.ui.idle_ctrl_c_armed);
        assert!(state.ui.overlay.is_none());
        assert_eq!(
            handle_ui_event(&mut driver, &mut state, ctrl_c, (120, 40))
                .expect("second idle Ctrl-C"),
            UiEventOutcome::Redraw
        );
        assert!(matches!(
            state.ui.overlay.as_ref().map(|overlay| overlay.kind),
            Some(OverlayKind::ExitConfirm)
        ));

        assert!(sent.lock().expect("sent commands").is_empty());
    }

    #[test]
    fn active_work_cancel_clears_stale_idle_ctrl_c_arm() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

        handle_ui_event(&mut driver, &mut state, ctrl_c.clone(), (120, 40))
            .expect("arm idle Ctrl-C");
        assert!(state.ui.idle_ctrl_c_armed);

        state.runtime.active_tool_calls.push(ToolCallView {
            tool_call_id: "tool-active".to_string(),
            name: "shell".to_string(),
            input_preview: "cargo test".to_string(),
            owner: None,
        });
        handle_ui_event(&mut driver, &mut state, ctrl_c.clone(), (120, 40))
            .expect("cancel active work");
        assert!(
            !state.ui.idle_ctrl_c_armed,
            "active-work Ctrl-C must invalidate an earlier idle arm"
        );
        assert!(
            sent.lock().expect("sent commands").is_empty(),
            "active work without an exact Lane owner must send nothing"
        );

        state.runtime.active_tool_calls.clear();
        handle_ui_event(&mut driver, &mut state, ctrl_c, (120, 40)).expect("new first idle Ctrl-C");
        assert!(state.ui.idle_ctrl_c_armed);
        assert!(
            state.ui.overlay.is_none(),
            "one idle Ctrl-C after cancellation must not open exit confirmation"
        );
    }

    #[test]
    fn exit_confirmation_rechecks_work_that_arrived_before_enter() {
        let mut driver =
            TuiClientDriver::connect(StatefulCoreClient::new(FakeCoreTransport::default()))
                .expect("connect");
        let mut state = TuiState::default();
        let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        for event in [ctrl_c.clone(), ctrl_c] {
            handle_ui_event(&mut driver, &mut state, event, (120, 40))
                .expect("open exit confirmation");
        }
        assert!(matches!(
            state.ui.overlay.as_ref().map(|overlay| overlay.kind),
            Some(OverlayKind::ExitConfirm)
        ));

        state.runtime.active_tool_calls.push(ToolCallView {
            tool_call_id: "tool-arrived".to_string(),
            name: "shell".to_string(),
            input_preview: "cargo test".to_string(),
            owner: None,
        });
        let outcome = handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("reject stale exit confirmation");

        assert_eq!(outcome, UiEventOutcome::Redraw);
        assert!(state.ui.overlay.is_none());
    }

    /// Moved baseline: the chain gained a rung. It used to be overlay ->
    /// selection -> insert, which dropped the Lane the moment the operator put
    /// its panel away — and `/git` is typed after that, in the composer. It is
    /// now overlay -> lane detail -> Lane target -> insert. The draft still
    /// survives every rung.
    #[test]
    fn escape_closes_overlay_then_lane_detail_then_target_then_insert_and_preserves_draft() {
        let mut driver =
            TuiClientDriver::connect(StatefulCoreClient::new(FakeCoreTransport::default()))
                .expect("connect");
        let mut state = TuiState::default();
        state.ui.input_mode = InputMode::Insert;
        state.ui.input = "keep this draft".into();
        state.ui.focus_lane("lane-selected".to_string());
        state.ui.overlay = Some(OverlayState::new(OverlayKind::ContextHelp));
        let escape = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        handle_ui_event(&mut driver, &mut state, escape.clone(), (120, 40)).expect("close overlay");
        assert!(state.ui.overlay.is_none());
        assert_eq!(state.ui.focused_lane.as_deref(), Some("lane-selected"));
        assert_eq!(state.ui.input_mode, InputMode::Insert);

        handle_ui_event(&mut driver, &mut state, escape.clone(), (120, 40))
            .expect("close the lane detail");
        assert!(!state.ui.lane_detail_open);
        assert_eq!(
            state.ui.focused_lane.as_deref(),
            Some("lane-selected"),
            "the Lane stays the target for a command typed in the composer"
        );
        assert_eq!(state.ui.input_mode, InputMode::Insert);

        handle_ui_event(&mut driver, &mut state, escape.clone(), (120, 40))
            .expect("clear the Lane target");
        assert!(state.ui.focused_lane.is_none());
        assert_eq!(state.ui.input_mode, InputMode::Insert);

        handle_ui_event(&mut driver, &mut state, escape, (120, 40)).expect("leave insert");
        assert_eq!(state.ui.input_mode, InputMode::Normal);
        assert_eq!(state.ui.input, "keep this draft");
    }

    #[test]
    fn global_jump_escape_restores_approval_owner_selected_id_and_composer_context() {
        let mut driver =
            TuiClientDriver::connect(StatefulCoreClient::new(FakeCoreTransport::default()))
                .expect("connect");
        let mut state = TuiState::default();
        state.ui.input_mode = InputMode::Insert;
        state.ui.input = "keep this draft".into();
        state.ui.focus_lane("lane-before-jump".to_string());
        state.ui.session_id = "session-before-jump".to_string();
        let mut approval = OverlayState::new(OverlayKind::Approval);
        approval.selected = 2;
        approval.selected_id = Some("approval-core-id".to_string());
        state.ui.overlay = Some(approval);

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)),
            (120, 40),
        )
        .expect("open global jump");
        assert!(matches!(
            state.ui.overlay.as_ref().map(|overlay| overlay.kind),
            Some(OverlayKind::GlobalJump)
        ));

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("restore approval owner");

        let restored = state.ui.overlay.expect("approval restored");
        assert_eq!(restored.kind, OverlayKind::Approval);
        assert_eq!(restored.selected, 2);
        assert_eq!(restored.selected_id.as_deref(), Some("approval-core-id"));
        assert_eq!(state.ui.input.as_str(), "keep this draft");
        assert_eq!(state.ui.input_mode, InputMode::Insert);
        assert_eq!(state.ui.focused_lane.as_deref(), Some("lane-before-jump"));
        assert_eq!(state.ui.session_id, "session-before-jump");
    }

    #[test]
    fn global_jump_escape_exactly_restores_interaction_panel_ownership() {
        let panels = [
            InteractionPanel::Setup {
                selected: 1,
                draft: "[project]\nname = \"edited\"\n".to_string(),
            },
            InteractionPanel::ConnectProvider {
                search: "deep".to_string(),
                selected: 2,
            },
            InteractionPanel::ModelPicker {
                provider_id: Some("fallback".to_string()),
                search: "test".to_string(),
                selected: 1,
            },
        ];

        for expected in panels {
            let mut driver = TuiClientDriver::connect(FakeCoreClient::default()).expect("connect");
            let mut state = TuiState::default();
            state.ui.interaction_panel = Some(expected.clone());

            handle_ui_event(
                &mut driver,
                &mut state,
                Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)),
                (120, 40),
            )
            .expect("open global jump above interaction panel");
            handle_ui_event(
                &mut driver,
                &mut state,
                Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
                (120, 40),
            )
            .expect("close only global jump");

            assert_eq!(state.ui.interaction_panel, Some(expected));
            assert!(state.ui.overlay.is_none());
        }
    }

    #[test]
    fn global_jump_escape_preserves_visible_composer_command_suggestions() {
        let mut driver = TuiClientDriver::connect(FakeCoreClient::default()).expect("connect");
        let mut state = TuiState::default();
        state.ui.input = "/con".into();
        state.ui.command_selection = 1;
        let hidden_before = state.ui.command_palette_hidden_for.clone();
        assert!(super::super::command_palette::is_command_palette_visible(
            &state
        ));

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)),
            (120, 40),
        )
        .expect("open global jump above command suggestions");
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("close only global jump");

        assert_eq!(state.ui.command_palette_hidden_for, hidden_before);
        assert!(super::super::command_palette::is_command_palette_visible(
            &state
        ));
        assert!(state.ui.overlay.is_none());
    }

    #[test]
    fn global_jump_enter_completes_commands_but_disabled_file_never_activates() {
        let client = FakeCoreClient::default();
        let sent = Arc::clone(&client.sent);
        let mut driver = TuiClientDriver::connect(client).expect("connect");
        let mut state = TuiState::default();
        state.ui.overlay = Some(OverlayState::global_jump(None));
        state.ui.overlay.as_mut().expect("jump overlay").filter = ">help".to_string();

        apply_input_intent(
            &mut driver,
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            InputIntent::CompleteOrSubmit,
            (120, 40),
        )
        .expect("complete command intent");
        assert_eq!(state.ui.input.as_str(), "/help");
        assert!(state.ui.overlay.is_none());
        assert!(sent.lock().expect("commands").is_empty());

        state.ui.overlay = Some(OverlayState::global_jump(None));
        state.ui.overlay.as_mut().expect("jump overlay").filter = "~".to_string();
        apply_input_intent(
            &mut driver,
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            InputIntent::CompleteOrSubmit,
            (120, 40),
        )
        .expect("disabled file row remains inert");
        assert!(matches!(
            state.ui.overlay.as_ref().map(|overlay| overlay.kind),
            Some(OverlayKind::GlobalJump)
        ));
        assert_eq!(state.ui.input.as_str(), "/help");
    }

    fn open_global_jump_with_filter(
        driver: &mut TuiClientDriver<FakeCoreClient>,
        state: &mut TuiState,
        filter: &str,
    ) {
        handle_ui_event(
            driver,
            state,
            Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)),
            (120, 40),
        )
        .expect("open global jump");
        handle_ui_event(driver, state, Event::Paste(filter.to_string()), (120, 40))
            .expect("filter global jump");
    }

    fn enter_global_jump(driver: &mut TuiClientDriver<FakeCoreClient>, state: &mut TuiState) {
        handle_ui_event(
            driver,
            state,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("activate global jump result");
    }

    #[test]
    fn global_jump_lane_enter_routes_from_typed_lane_and_session_facts() {
        let mut driver = TuiClientDriver::connect(FakeCoreClient::default()).expect("connect");
        let mut state = TuiState::default();
        let mut lanes: Vec<AgentLaneRecord> = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        lanes.truncate(1);
        lanes[0].active_session_ids = vec!["session-lane-route".to_string()];
        let lane_id = lanes[0].id.clone();
        state.runtime.lanes = lanes;

        open_global_jump_with_filter(&mut driver, &mut state, &format!(":{lane_id}"));
        enter_global_jump(&mut driver, &mut state);

        assert_eq!(state.ui.focused_lane.as_deref(), Some(lane_id.as_str()));
        assert_eq!(state.ui.session_id, "session-lane-route");
        assert_eq!(state.ui.lens, Lens::Session);
    }

    #[test]
    fn global_jump_session_enter_uses_typed_parent_lane_and_session_id() {
        let mut driver = TuiClientDriver::connect(FakeCoreClient::default()).expect("connect");
        let mut state = TuiState::default();
        let mut lanes: Vec<AgentLaneRecord> = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        lanes.truncate(1);
        lanes[0].active_session_ids = vec!["session-global".to_string()];
        let lane_id = lanes[0].id.clone();
        state.runtime.lanes = lanes;

        open_global_jump_with_filter(&mut driver, &mut state, "@session-global");
        enter_global_jump(&mut driver, &mut state);

        assert_eq!(state.ui.focused_lane.as_deref(), Some(lane_id.as_str()));
        assert_eq!(state.ui.session_id, "session-global");
        assert_eq!(state.ui.lens, Lens::Session);
    }

    #[test]
    fn global_jump_gate_enter_routes_from_typed_merge_gate_fact() {
        let mut driver = TuiClientDriver::connect(FakeCoreClient::default()).expect("connect");
        let mut state = TuiState::default();
        state.runtime.merge_gates.push(
            serde_json::from_value(serde_json::json!({
                "gate_id": "gate-review",
                "task_id": "task-review",
                "status": "proposed",
                "required_evidence": [],
                "evidence_ids": [],
                "updated_at": 1
            }))
            .expect("typed merge gate"),
        );

        open_global_jump_with_filter(&mut driver, &mut state, "#gate-review");
        enter_global_jump(&mut driver, &mut state);

        assert_eq!(state.ui.lens, Lens::Decisions);
        assert!(state.ui.overlay.is_none());
    }

    #[test]
    fn global_jump_ask_enter_focuses_exact_typed_approval_id() {
        let mut driver = TuiClientDriver::connect(FakeCoreClient::default()).expect("connect");
        let mut state = TuiState::default();
        state.runtime.pending_approvals.push(
            serde_json::from_value(serde_json::json!({
                "id": "approval-review",
                "tool_name": "shell",
                "title": "Approval review",
                "message": "Review the proposed command",
                "input_preview": "cargo test",
                "is_mutating": true,
                "reason": "operator decision"
            }))
            .expect("typed approval"),
        );

        open_global_jump_with_filter(&mut driver, &mut state, "#approval");
        enter_global_jump(&mut driver, &mut state);

        let approval = state.ui.overlay.expect("approval focus overlay");
        assert_eq!(state.ui.lens, Lens::Decisions);
        assert_eq!(approval.kind, OverlayKind::Approval);
        assert_eq!(approval.selected_id.as_deref(), Some("approval-review"));
    }

    #[test]
    fn global_jump_navigation_clamps_arrows_and_jk_for_results_empty_and_disabled_rows() {
        let mut driver = TuiClientDriver::connect(FakeCoreClient::default()).expect("connect");
        let mut state = TuiState::default();
        open_global_jump_with_filter(&mut driver, &mut state, ">");

        for code in [KeyCode::Up, KeyCode::Char('k')] {
            handle_ui_event(
                &mut driver,
                &mut state,
                Event::Key(KeyEvent::new(code, KeyModifiers::NONE)),
                (120, 40),
            )
            .expect("clamp at first result");
        }
        assert_eq!(state.ui.overlay.as_ref().expect("jump").selected, 0);

        // The last command row: fifteen registered commands since `/git`
        // joined the registry, so the clamp sits one row further down.
        let last_command = super::super::command_palette::command_registry().len() - 1;
        state.ui.overlay.as_mut().expect("jump").selected = last_command;
        for code in [KeyCode::Down, KeyCode::Char('j')] {
            handle_ui_event(
                &mut driver,
                &mut state,
                Event::Key(KeyEvent::new(code, KeyModifiers::NONE)),
                (120, 40),
            )
            .expect("clamp at last result");
        }
        assert_eq!(
            state.ui.overlay.as_ref().expect("jump").selected,
            last_command
        );

        let overlay = state.ui.overlay.as_mut().expect("jump");
        overlay.filter = ">no-such-command".to_string();
        overlay.selected = 0;
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("empty results stay at zero");
        assert_eq!(state.ui.overlay.as_ref().expect("jump").selected, 0);

        let overlay = state.ui.overlay.as_mut().expect("jump");
        overlay.filter = "~".to_string();
        overlay.selected = 0;
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("disabled singleton stays selected");
        assert_eq!(state.ui.overlay.as_ref().expect("jump").selected, 0);
    }

    #[test]
    fn overlay_owns_filter_navigation_and_enter_without_touching_insert_draft() {
        let mut driver =
            TuiClientDriver::connect(StatefulCoreClient::new(FakeCoreTransport::default()))
                .expect("connect");
        let mut state = TuiState::default();
        state.ui.input_mode = InputMode::Insert;
        state.ui.input = "draft stays".into();

        for event in [
            Event::Key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL)),
            Event::Key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE)),
            Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
        ] {
            handle_ui_event(&mut driver, &mut state, event, (120, 40)).expect("overlay input");
        }

        let overlay = state.ui.overlay.as_ref().expect("lane overlay");
        assert_eq!(overlay.kind, OverlayKind::Lane);
        assert_eq!(overlay.filter, "f");
        assert_eq!(overlay.selected, 1);
        assert_eq!(state.ui.input, "draft stays");

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("overlay enter");
        assert!(state.ui.overlay.is_none());
        assert_eq!(state.ui.input_mode, InputMode::Insert);
        assert_eq!(state.ui.input, "draft stays");
    }

    #[test]
    fn paste_in_global_overlay_updates_filter_without_touching_hidden_draft() {
        let mut driver =
            TuiClientDriver::connect(StatefulCoreClient::new(FakeCoreTransport::default()))
                .expect("connect");
        let mut state = TuiState::default();
        state.ui.input_mode = InputMode::Insert;
        state.ui.input = "hidden draft".into();
        state.ui.overlay = Some(OverlayState::new(OverlayKind::Lane));

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Paste("review".to_string()),
            (120, 40),
        )
        .expect("overlay paste");

        let overlay = state.ui.overlay.as_ref().expect("lane overlay");
        assert_eq!(overlay.filter, "review");
        assert_eq!(overlay.selected, 0);
        assert_eq!(state.ui.input, "hidden draft");
    }

    #[test]
    fn paste_in_interaction_overlay_updates_its_filter_not_composer() {
        let mut driver =
            TuiClientDriver::connect(StatefulCoreClient::new(FakeCoreTransport::default()))
                .expect("connect");
        let mut state = TuiState::default();
        state.ui.input = "hidden draft".into();
        state.ui.interaction_panel = Some(InteractionPanel::ConnectProvider {
            search: String::new(),
            selected: 3,
        });

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Paste("deep".to_string()),
            (120, 40),
        )
        .expect("interaction paste");

        assert!(matches!(
            state.ui.interaction_panel,
            Some(InteractionPanel::ConnectProvider { ref search, selected })
                if search == "deep" && selected == 0
        ));
        assert_eq!(state.ui.input, "hidden draft");
    }

    #[test]
    fn provider_and_model_selector_paths_are_reachable_from_core_client_loop() {
        let mut driver =
            TuiClientDriver::connect(StatefulCoreClient::new(FakeCoreTransport::default()))
                .expect("connect");
        let mut state = TuiState::default();
        state.ui.provider_catalog = crate::tui::state::ProviderOption::fixture();
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("insert mode");
        state.ui.input = "/models".into();

        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("open models");

        assert!(matches!(
            state.ui.interaction_panel,
            Some(InteractionPanel::ModelPicker { .. })
        ));
        assert_eq!(effective_input_mode(&state), InputMode::Overlay);
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            (120, 40),
        )
        .expect("close selector");
        assert!(state.ui.interaction_panel.is_none());
        assert_eq!(state.ui.input_mode, InputMode::Insert);

        state.ui.input = "/models".into();
        for _ in 0..2 {
            handle_ui_event(
                &mut driver,
                &mut state,
                Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                (120, 40),
            )
            .expect("open and select model");
        }
        assert!(state.ui.interaction_panel.is_none());
        assert!(state.ui.interaction_panel.is_none());
        assert!(
            state
                .ui
                .entries
                .iter()
                .all(|entry| entry.label != "assistant")
        );
    }

    #[test]
    fn native_acp_fixture_render() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            initial_snapshot: RuntimeSnapshot,
            events: Vec<RuntimeEventEnvelope>,
            expected_final_cursor: EventCursor,
        }

        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/interaction-closed-loop.json"
        ))
        .expect("interaction fixture");
        let initial_cursor = EventCursor {
            stream_id: fixture.expected_final_cursor.stream_id.clone(),
            sequence: 0,
        };
        let client = FakeCoreClient {
            transport: FakeCoreTransport {
                events: fixture.events.clone().into(),
                view: Some(RuntimeViewState::new(fixture.initial_snapshot)),
                snapshot_cursor: Some(initial_cursor),
                ..FakeCoreTransport::default()
            },
            ..FakeCoreClient::default()
        };
        let mut driver = TuiClientDriver::connect(client).expect("connect canonical fixture");
        let mut observed = Vec::new();

        for _ in 0..fixture.events.len() {
            assert!(matches!(
                driver.pump().expect("apply ordered fixture event"),
                PumpOutcome::Applied(_)
            ));
            observed.extend(driver.take_applied_events());
        }

        assert_eq!(driver.cursor(), &fixture.expected_final_cursor);
        let mut state = state_from_driver(&driver, &TuiOptions::new("fixture parity"));
        let projection = CockpitProjection::from(&state.runtime, &state.ui);
        state.ui.lens = Lens::Board;
        let board_rendered = super::super::render::render_frame(&state, 160, 55);
        state.ui.lens = Lens::Gallery;
        let gallery_rendered = super::super::render::render_frame(&state, 160, 55);
        state.ui.lens = Lens::Decisions;
        let decisions_rendered = super::super::render::render_frame(&state, 160, 55);

        assert_eq!(projection.lanes.len(), 1);
        assert_eq!(projection.lanes[0].id, "lane-loop-coder");
        assert!(board_rendered.contains("lane-loop-coder"));

        // The canonical reducer keeps one execution identity per Lane. Both
        // start receipts remain observable, but the frontend cannot invent a
        // second concurrent Agent owner from historical session events.
        assert_eq!(state.runtime.agent_sessions.len(), 1);
        assert_eq!(
            state.runtime.agent_sessions[0].session_id,
            "session-loop-built-in"
        );
        assert!(observed.iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::AgentSessionStarted { session }
                if session.session_id == "session-loop-built-in"
        )));
        assert!(observed.iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::AgentSessionStarted { session }
                if session.session_id == "session-loop-acp"
        )));

        assert!(observed.iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::ToolCallStarted { tool_call_id, .. }
                if tool_call_id == "tool-loop-test"
        )));
        assert!(observed.iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::ToolCallFinished { tool_call_id, success, .. }
                if tool_call_id == "tool-loop-test" && *success
        )));
        assert!(projection.active_tools.is_empty());
        assert!(observed.iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::ApprovalRequested { approval }
                if approval.id == "approval-loop-tool"
        )));
        assert!(observed.iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::ApprovalResolved { request_id, .. }
                if request_id == "approval-loop-tool"
        )));
        assert!(projection.approvals.is_empty());

        assert_eq!(projection.evidence[0].id, "evidence-loop-test");
        assert_eq!(projection.merge_gates[0].gate_id, "gate-loop-apply");
        assert_eq!(projection.lane_conflicts[0].lane_id, "lane-loop-coder");
        assert_eq!(projection.lane_conflicts[0].paths, ["src/lib.rs"]);
        assert_eq!(projection.lane_recoveries[0].lane_id, "lane-loop-coder");
        assert_eq!(
            projection.recovery_actions[0].action,
            "action.revalidate_merge_conflict"
        );
        assert_eq!(
            state.runtime.agent_session_inputs[0].input_id,
            "agent-input-loop-follow-up"
        );
        assert!(observed.iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::AgentSessionStarted { session }
                if session.session_id == "session-loop-acp"
                    && session.task == "task.loop.follow_up"
        )));
        assert!(gallery_rendered.contains("evidence-loop-test"));
        assert!(gallery_rendered.contains("gate-loop-apply"));
        assert!(decisions_rendered.contains("gate-loop-apply"));
        assert!(decisions_rendered.contains("action.revalidate_merge_conflict"));
    }

    #[test]
    fn rendered_shortcut_hints_match_command_and_agent_handlers() {
        let mut driver =
            TuiClientDriver::connect(StatefulCoreClient::new(FakeCoreTransport::default()))
                .expect("connect");
        let mut state = TuiState::default();
        state.runtime.lanes = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        handle_ui_event(
            &mut driver,
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL)),
            (140, 40),
        )
        .expect("command shortcut");
        assert!(matches!(
            state.ui.overlay.as_ref().map(|overlay| overlay.kind),
            Some(OverlayKind::CommandPalette)
        ));

        let mut agent_state = state.clone();
        agent_state.ui.clear_lane_focus();
        agent_state.ui.overlay = None;
        handle_ui_event(
            &mut driver,
            &mut agent_state,
            Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            (140, 40),
        )
        .expect("agent shortcut");
        assert_eq!(agent_state.ui.focused_lane.as_deref(), Some("L-start"));
    }

    #[test]
    fn runtime_view_projects_authoritative_frontend_facts_without_workspace_fixture() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            initial_snapshot: RuntimeSnapshot,
            events: Vec<RuntimeEventEnvelope>,
        }

        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/d1-vertical-slice.json"
        ))
        .expect("shared fixture");
        let mut view = RuntimeViewState::new(fixture.initial_snapshot);
        let mut approval_event = None;
        for envelope in fixture.events {
            if let viden_types::RuntimeWireEvent::Known(event) = envelope.event {
                if matches!(
                    event.kind,
                    viden_types::RuntimeEventKind::ApprovalRequested { .. }
                ) {
                    approval_event = Some(event.clone());
                }
                view.apply_event(&event);
            }
        }
        view.apply_event(&approval_event.expect("approval fixture event"));
        view.queued_inputs.push(viden_types::QueuedInputView {
            id: "queue-1".to_string(),
            content_preview: "continue with tests".to_string(),
            created_at: Some(1),
            owner: None,
        });
        let client = StatefulCoreClient::new(FakeCoreTransport {
            view: Some(view),
            ..FakeCoreTransport::default()
        });
        let driver = TuiClientDriver::connect(client).expect("connect");

        let state = state_from_driver(&driver, &TuiOptions::new("startup"));

        assert_eq!(state.runtime.snapshot.cwd, PathBuf::from("workspace/viden"));
        assert_eq!(state.runtime.assistant_stream, "D1 cockpit state");
        assert!(!state.runtime.pending_approvals.is_empty());
        assert!(!state.runtime.errors.is_empty());
        assert_eq!(
            state.runtime.queued_inputs[0].content_preview,
            "continue with tests"
        );
        assert_eq!(state.runtime.tasks.len(), 1);
        assert!(state.runtime.cost_ledger.total_tokens > 0);
    }

    #[test]
    fn runtime_view_projects_multi_lane_fixture_facts() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            initial_snapshot: RuntimeSnapshot,
            events: Vec<RuntimeEventEnvelope>,
        }

        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/multi-lane.json"
        ))
        .expect("multi-lane shared fixture");
        let mut view = RuntimeViewState::new(fixture.initial_snapshot);
        for envelope in fixture.events {
            if let viden_types::RuntimeWireEvent::Known(event) = envelope.event {
                view.apply_event(&event);
            }
        }
        let driver = TuiClientDriver::connect(StatefulCoreClient::new(FakeCoreTransport {
            view: Some(view),
            ..FakeCoreTransport::default()
        }))
        .expect("connect");

        let state = state_from_driver(&driver, &TuiOptions::new("startup"));

        assert_eq!(state.runtime.lanes.len(), 2);
        let core = state
            .runtime
            .lanes
            .iter()
            .find(|lane| lane.id == "lane_core")
            .expect("core lane");
        assert_eq!(core.status, LaneStatus::Running);
        assert_eq!(
            core.role.to_string(),
            "coder",
            "role is the visible lane owner"
        );
        assert_eq!(core.task_id.as_deref(), Some("task_core"));
        assert_eq!(core.worktree.as_deref(), Some(".worktrees/lane_core"));

        let review = state
            .runtime
            .lanes
            .iter()
            .find(|lane| lane.id == "lane_review")
            .expect("review lane");
        assert_eq!(review.status, LaneStatus::WaitingApproval);
        assert_eq!(review.role.to_string(), "reviewer");
    }

    #[test]
    fn runtime_view_projects_representative_typed_lane_fixture() {
        let lanes: Vec<AgentLaneRecord> = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed-lanes shared fixture");
        let snapshot = RuntimeSnapshot {
            cwd: PathBuf::from("workspace/viden"),
            provider_family: "fallback".to_string(),
            model_label: "test-local".to_string(),
            work_mode: WorkMode::Build,
            permission_mode: PermissionMode::Default,
            permission_level: PermissionLevel::Ask,
            config_summary: "typed lanes".to_string(),
            loaded_config_files: Vec::new(),
            startup_overrides: Vec::new(),
            ui_preferences: Default::default(),
        };
        let mut view = RuntimeViewState::new(snapshot);
        view.lanes = lanes;
        let driver = TuiClientDriver::connect(StatefulCoreClient::new(FakeCoreTransport {
            view: Some(view),
            ..FakeCoreTransport::default()
        }))
        .expect("connect");

        let state = state_from_driver(&driver, &TuiOptions::new("startup"));

        assert_eq!(state.runtime.lanes.len(), 4);
        let detached = state
            .runtime
            .lanes
            .iter()
            .find(|lane| lane.id == "L-detached")
            .expect("detached lane");
        assert_eq!(detached.status, LaneStatus::Detached);
        assert_eq!(
            detached.role.to_string(),
            "coder",
            "role is the visible lane owner"
        );
        assert_eq!(detached.task_id.as_deref(), Some("task_detached"));
        assert_eq!(detached.summary, "legacy detached lane");
    }

    #[test]
    fn typed_done_review_and_blocked_lanes_project_into_rendered_statuses() {
        let mut lanes: Vec<AgentLaneRecord> = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed-lanes shared fixture");
        lanes.truncate(3);
        for (lane, (id, status, summary)) in lanes.iter_mut().zip([
            ("L-done", LaneStatus::Done, "result ready"),
            ("L-review", LaneStatus::WaitingApproval, "approval pending"),
            ("L-blocked", LaneStatus::Blocked, "dependency blocker"),
        ]) {
            lane.id = id.to_string();
            lane.status = status;
            lane.summary = summary.to_string();
            lane.worktree = None;
        }
        let snapshot = RuntimeSnapshot {
            cwd: PathBuf::from("workspace/viden"),
            provider_family: "fallback".to_string(),
            model_label: "test-local".to_string(),
            work_mode: WorkMode::Build,
            permission_mode: PermissionMode::Default,
            permission_level: PermissionLevel::Ask,
            config_summary: "typed lane rendering".to_string(),
            loaded_config_files: Vec::new(),
            startup_overrides: Vec::new(),
            ui_preferences: Default::default(),
        };
        let mut view = RuntimeViewState::new(snapshot);
        view.lanes = lanes;
        let driver = TuiClientDriver::connect(StatefulCoreClient::new(FakeCoreTransport {
            view: Some(view),
            ..FakeCoreTransport::default()
        }))
        .expect("connect");
        let state = state_from_driver(&driver, &TuiOptions::new("startup"));

        let rendered = crate::tui::render::render_side_frame(&state, 100, 70);

        assert!(rendered.contains("L-done"));
        assert!(rendered.contains("done"));
        assert!(rendered.contains("L-review"));
        assert!(rendered.contains("waitingapproval"));
        assert!(rendered.contains("approval pending"));
        assert!(rendered.contains("L-blocked"));
        assert!(rendered.contains("blocked"));
        assert!(rendered.contains("blocker"));
    }

    #[test]
    fn release_manifest_declares_requested_and_effective_presentation_inputs() {
        let manifest = include_str!("../../release-manifest.toml");
        let parsed = manifest
            .parse::<toml::Value>()
            .expect("release manifest TOML");

        assert_eq!(env!("CARGO_PKG_VERSION"), "0.3.5");
        assert!(manifest.contains("version = \"0.3.5\""));
        assert!(manifest.contains(
            "tokens_css = \"826826ee6ddab845897472701add67ee9f55aff25af539651e6089553b7e6398\""
        ));
        // The catalogs are pinned by *recomputation*, not by a literal: a
        // literal has to be edited by hand every time a string is added, which
        // is exactly when the pin stops proving anything. Computing it here
        // asserts what the manifest is for — the shipped catalogs are the ones
        // the certification names — and `scripts/tui-regression.sh` recomputes
        // the same two digests the same way.
        for (key, catalog) in [
            ("catalog_en", include_str!("../../i18n/en.json")),
            ("catalog_zh_cn", include_str!("../../i18n/zh-CN.json")),
        ] {
            use sha2::{Digest, Sha256};
            let digest = format!("{:x}", Sha256::digest(catalog.as_bytes()));
            assert!(
                manifest.contains(&format!("{key} = \"{digest}\"")),
                "{key} digest {digest} is not pinned in the TUI release manifest"
            );
        }
        assert!(manifest.contains("min_core_version = \"0.3.4\""));
        assert!(
            manifest
                .contains("base_core_checkpoint = \"54965464e87860f9c39a1fb656c2f528e354da94\"")
        );
        assert!(manifest.contains(
            "extension_fixture_sha256 = \"96dd5fde9f1241eb50f9d8978cf478d0ac5d3327448dc6ccde9d0e5018ce1580\""
        ));
        assert!(manifest.contains(
            "base_corpus_sha256 = \"e272d7bee25af5d4a0e719aa7226f1b5bf22086e90f0d02224196c41ce67fcab\""
        ));
        assert_eq!(
            parsed["fixture_revisions"]["base_fixture_sha256"]
                .as_array()
                .expect("base fixture digests")
                .len(),
            9
        );
        for capability in viden_core::CORE_CLIENT_CAPABILITIES {
            assert!(manifest.contains(&format!("\"{capability}\"")));
        }
        for capability in viden_core::CORE_EXTENSION_CAPABILITIES {
            assert!(manifest.contains(&format!("\"{capability}\"")));
        }
        let required = parsed["compatibility"]["required_capabilities"]
            .as_array()
            .expect("required capabilities")
            .iter()
            .map(|value| value.as_str().expect("capability string"))
            .collect::<Vec<_>>();
        assert_eq!(required, viden_core::CORE_CLIENT_CAPABILITIES);
        let extensions = parsed["extensions"]["capabilities"]
            .as_array()
            .expect("extension capabilities")
            .iter()
            .map(|value| value.as_str().expect("capability string"))
            .collect::<Vec<_>>();
        assert_eq!(extensions, viden_core::CORE_EXTENSION_CAPABILITIES);
        assert!(
            required
                .iter()
                .all(|capability| !extensions.contains(capability))
        );
        assert!(manifest.contains("locales = [\"system\", \"en\", \"zh-CN\"]"));
        assert!(manifest.contains("effective_locales = [\"en\", \"zh-CN\"]"));
        assert!(manifest.contains("modes = [\"system\", \"dark\", \"light\"]"));
        assert!(manifest.contains("effective_modes = [\"dark\", \"light\"]"));
        assert!(manifest.contains("densities = [\"compact\", \"regular\", \"comfy\"]"));
        assert!(manifest.contains("motion = [\"system\", \"reduced\", \"full\"]"));
        assert!(
            manifest
                .contains("tui_color_depth = [\"auto\", \"truecolor\", \"ansi256\", \"ansi16\"]")
        );
        assert!(
            manifest
                .contains("effective_tui_color_depth = [\"truecolor\", \"ansi256\", \"ansi16\"]")
        );
        assert!(manifest.contains("mouse_capture = false"));
    }

    #[test]
    fn startup_check_connects_core_client_without_entering_terminal() {
        let client = StatefulCoreClient::new(FakeCoreTransport::default());
        let options = TuiOptions::new("startup").with_startup_check();

        run_tui(client, options).expect("startup check");
    }

    #[test]
    fn active_turn_enter_queues_follow_up_instead_of_submitting_second_turn() {
        let mut state = TuiState::default();
        state.runtime.active_tool_calls.push(ToolCallView {
            tool_call_id: "tool-1".to_string(),
            name: "first".to_string(),
            input_preview: "{}".to_string(),
            owner: None,
        });

        assert!(matches!(
            command_for_composer(&state, "second"),
            RuntimeCommand::QueueFollowUp { content } if content == "second"
        ));
    }

    /// E1's first stall, reproduced.
    ///
    /// One completed built-in fallback turn leaves its reply in Core's unscoped
    /// `assistant_stream`, and Core settles that stream only on a terminal
    /// Agent-session fact, which the built-in path never publishes. Routing on
    /// the residue queued every later prompt for the rest of the session, and
    /// by the Core queue gap nothing ever ran them.
    #[test]
    fn a_settled_built_in_turn_stream_does_not_queue_the_next_prompt() {
        let mut state = TuiState::default();
        state.runtime.assistant_stream =
            "The config loader lives in crates/runtime/src/config.rs.".to_string();

        assert!(state.runtime.active_tool_calls.is_empty());
        assert!(state.runtime.pending_approvals.is_empty());
        assert!(matches!(
            command_for_composer(&state, "second"),
            RuntimeCommand::SubmitUserInput { content } if content == "second"
        ));
    }

    /// E1's second stall, reproduced.
    ///
    /// Core leaves a starter Lane in `Draft`, and `LaneStatus::is_active` is a
    /// lifecycle predicate that counts `Draft` as active. Once anything was
    /// queued, `queued_inputs` also stayed non-empty forever, because Core
    /// never drains the session queue. Neither fact says the owner this
    /// composer input addresses is running a turn.
    #[test]
    fn a_draft_starter_lane_and_a_stuck_session_queue_do_not_queue_the_next_prompt() {
        let mut state = TuiState::default();
        let mut lanes: Vec<AgentLaneRecord> = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        lanes.truncate(1);
        lanes[0].status = LaneStatus::Draft;
        state.runtime.lanes = lanes;
        state
            .runtime
            .queued_inputs
            .push(viden_core::QueuedInputView {
                id: "queued-1".to_string(),
                content_preview: "first".to_string(),
                created_at: None,
                owner: None,
            });

        assert!(
            state.runtime.lanes[0].is_active(),
            "the lifecycle fact holds"
        );
        assert!(matches!(
            command_for_composer(&state, "second"),
            RuntimeCommand::SubmitUserInput { content } if content == "second"
        ));
    }

    /// E1 defect 2, closed by the Core fact rather than by a client window.
    ///
    /// The turn is bracketed by `runtime.turn_lifecycle`, so routing follows
    /// Core's own `active_turns`: while the turn runs the next prompt queues,
    /// and the `TurnFinished` Core publishes on every exit puts the composer
    /// back to submitting. The events are pushed through the real reducer, not
    /// assigned to the view, so what is asserted is what a client that
    /// received those facts would hold.
    #[test]
    fn core_turn_brackets_decide_whether_the_next_prompt_queues_or_submits() {
        let mut state = TuiState::default();
        let owner = RuntimeOwner {
            workspace_id: "ws_fixture".to_string(),
            project_id: "prj_fixture".to_string(),
            lane_id: None,
            session_id: None,
            task_id: None,
            turn_id: Some("turn_first".to_string()),
        };
        let turn = viden_core::TurnView {
            turn_id: "turn_first".to_string(),
            owner: owner.clone(),
            source: viden_core::TurnSource::UserInput,
            started_at: 1_700_000_001,
        };

        state.runtime.apply_event(&RuntimeEvent::new(
            1,
            RuntimeEventKind::TurnStarted { turn },
        ));
        // Streamed text beside the turn is display, not the liveness fact.
        state.runtime.assistant_stream = "Applied the requested change.".to_string();

        assert!(matches!(
            command_for_composer(&state, "second"),
            RuntimeCommand::QueueFollowUp { content } if content == "second"
        ));

        state.runtime.apply_event(&RuntimeEvent::new(
            2,
            RuntimeEventKind::TurnFinished {
                turn_id: "turn_first".to_string(),
                owner,
                outcome: viden_core::TurnOutcome::Completed,
                finished_at: 1_700_000_005,
            },
        ));

        assert!(
            state.runtime.active_turns.is_empty(),
            "TurnFinished removes the turn"
        );
        assert!(
            state.runtime.assistant_stream.is_empty(),
            "the session-scoped end settles the unscoped stream"
        );
        assert!(matches!(
            command_for_composer(&state, "second"),
            RuntimeCommand::SubmitUserInput { content } if content == "second"
        ));
    }

    /// The other half of the owner rule: a Lane running its own turn is not
    /// this input's target. The GUI's composer is owner-scoped for exactly this
    /// reason, and the two clients must not disagree about what "busy" means.
    #[test]
    fn a_lane_scoped_turn_does_not_queue_a_session_scoped_prompt() {
        let mut state = TuiState::default();
        let lane_owner = RuntimeOwner {
            workspace_id: "workspace".to_string(),
            project_id: "viden".to_string(),
            lane_id: Some("lane-a".to_string()),
            session_id: Some("session-a".to_string()),
            task_id: None,
            turn_id: Some("turn-a".to_string()),
        };
        state.runtime.active_tool_calls.push(ToolCallView {
            tool_call_id: "tool-1".to_string(),
            name: "edit_file".to_string(),
            input_preview: "{}".to_string(),
            owner: Some(lane_owner),
        });

        assert!(matches!(
            command_for_composer(&state, "second"),
            RuntimeCommand::SubmitUserInput { content } if content == "second"
        ));
    }

    #[test]
    fn approval_shortcut_builds_response_for_core_request_id() {
        let mut view = RuntimeViewState::new(RuntimeSnapshot {
            cwd: PathBuf::from("/workspace"),
            provider_family: "fallback".to_string(),
            model_label: "test-local".to_string(),
            work_mode: WorkMode::Build,
            permission_mode: PermissionMode::Default,
            permission_level: PermissionLevel::Ask,
            config_summary: "fixture".to_string(),
            loaded_config_files: Vec::new(),
            startup_overrides: Vec::new(),
            ui_preferences: Default::default(),
        });
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/approval-allow-deny.json"
        ))
        .expect("approval fixture");
        let envelope: RuntimeEventEnvelope =
            serde_json::from_value(fixture["events"][0].clone()).expect("approval event");
        if let viden_types::RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }

        assert!(matches!(
            approval_command(&view, true),
            Some(RuntimeCommand::RespondToApproval { request_id, response })
                if request_id == "approval_allow" && response.is_allowed()
        ));
    }

    fn agent_session(
        session_id: &str,
        status: viden_core::AgentSessionStatus,
        output: Option<&str>,
    ) -> AgentSessionView {
        AgentSessionView {
            session_id: session_id.to_string(),
            lane_id: "lane-a".to_string(),
            agent_id: "codex".to_string(),
            model: None,
            status,
            owner: Default::default(),
            task: "review the gate".to_string(),
            diagnostic: None,
            output: output.map(str::to_string),
        }
    }

    fn view_with_sessions(sessions: Vec<AgentSessionView>) -> RuntimeViewState {
        let mut view = RuntimeViewState::new(RuntimeSnapshot {
            cwd: PathBuf::from("/workspace"),
            provider_family: "fallback".to_string(),
            model_label: "test-local".to_string(),
            work_mode: WorkMode::Build,
            permission_mode: PermissionMode::Default,
            permission_level: PermissionLevel::Ask,
            config_summary: "fixture".to_string(),
            loaded_config_files: Vec::new(),
            startup_overrides: Vec::new(),
            ui_preferences: Default::default(),
        });
        view.agent_sessions = sessions;
        view
    }

    fn drive_events(
        view: RuntimeViewState,
        events: Vec<RuntimeEventEnvelope>,
    ) -> Vec<crate::tui::state::TuiEntry> {
        let (mut driver, mut state, _sent) = supervision_driver(view, events.clone());
        seed_settled_agent_sessions(&mut state, driver.view());
        for _ in 0..events.len() {
            let outcome = driver.pump().expect("pump");
            apply_pump_outcome(&mut state, &driver, outcome);
            project_driver_view(&mut state, &driver);
            observe_driver_events(&mut state, &mut driver).expect("observe");
        }
        state.ui.entries
    }

    /// The regression this fix exists for: Core settles the unscoped
    /// `assistant_stream` when a turn ends, and that stream was the only
    /// surface rendering an ACP reply, so a turn the operator just watched
    /// would otherwise vanish at the instant it completed.
    #[test]
    fn a_live_observed_completion_keeps_its_settled_reply_in_the_transcript() {
        let session = agent_session(
            "agent-session_1785486260041818000",
            viden_core::AgentSessionStatus::Completed,
            Some("the merge gate needs a second reviewer\n"),
        );
        // The session is absent from the startup snapshot: it started and
        // finished while this client was watching.
        let entries = drive_events(
            view_with_sessions(Vec::new()),
            vec![event(
                1,
                RuntimeEventKind::AgentSessionCompleted {
                    session: session.clone(),
                },
            )],
        );

        let replies: Vec<_> = entries
            .iter()
            .filter(|entry| entry.kind() == "assistant")
            .collect();
        assert_eq!(replies.len(), 1, "exactly one settled reply is kept");
        assert_eq!(replies[0].body, "the merge gate needs a second reviewer");
        assert!(
            replies[0].label.starts_with("assistant · "),
            "the reply names its session: {}",
            replies[0].label
        );
        // Tail-first, because ACP session ids differ only at their end.
        assert!(
            replies[0].label.ends_with("260041818000"),
            "the session tail must survive: {}",
            replies[0].label
        );
        // Registered glyphs only: the separator is the same middle dot the
        // rest of the TUI uses, and nothing else non-ASCII appears.
        assert!(
            replies[0]
                .label
                .chars()
                .all(|ch| ch.is_ascii() || ch == '·' || ch == '…'),
            "no unregistered glyph in {}",
            replies[0].label
        );
    }

    /// Core re-delivers a terminal fact whenever it re-publishes runtime
    /// state, so the transcript must not gain a second copy of the same reply.
    #[test]
    fn a_repeated_terminal_fact_does_not_duplicate_the_reply() {
        let session = agent_session(
            "agent-session_1785486260041818000",
            viden_core::AgentSessionStatus::Completed,
            Some("done"),
        );
        let entries = drive_events(
            view_with_sessions(Vec::new()),
            vec![
                event(
                    1,
                    RuntimeEventKind::AgentSessionCompleted {
                        session: session.clone(),
                    },
                ),
                event(
                    2,
                    RuntimeEventKind::AgentSessionCompleted {
                        session: session.clone(),
                    },
                ),
            ],
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.kind() == "assistant")
                .count(),
            1,
            "the same session settles once"
        );
    }

    /// The critical gate. Core prefixes its whole persisted runtime state to
    /// every turn's event batch, so sessions that finished weeks ago
    /// re-deliver their terminal facts on the live stream. They are already
    /// terminal in the snapshot this client started from, and replayed history
    /// must stay out of the transcript — that is the whole point of settling
    /// the stream.
    #[test]
    fn startup_replayed_history_never_reaches_the_transcript() {
        let historical = vec![
            agent_session(
                "agent-session_1785486260041818000",
                viden_core::AgentSessionStatus::Completed,
                Some("Warning: an old reply from weeks ago"),
            ),
            agent_session(
                "agent-session_1785487371561133000",
                viden_core::AgentSessionStatus::Failed,
                Some("another old reply"),
            ),
        ];
        let events = historical
            .iter()
            .enumerate()
            .map(|(index, session)| {
                let kind = if session.status == viden_core::AgentSessionStatus::Failed {
                    RuntimeEventKind::AgentSessionFailed {
                        session: session.clone(),
                    }
                } else {
                    RuntimeEventKind::AgentSessionCompleted {
                        session: session.clone(),
                    }
                };
                event(index as u64 + 1, kind)
            })
            .collect();

        let entries = drive_events(view_with_sessions(historical), events);
        assert!(
            entries.iter().all(|entry| entry.kind() != "assistant"),
            "replayed history must not be rendered as a watched reply"
        );
    }

    /// A failed turn's reply is still the reply, and the diagnostic that ended
    /// it travels with it instead of being dropped.
    #[test]
    fn a_failed_session_keeps_its_reply_and_names_the_failure() {
        let mut session = agent_session(
            "agent-session_1785486260041818000",
            viden_core::AgentSessionStatus::Failed,
            Some("partial answer"),
        );
        session.diagnostic = Some("adapter closed the stream".to_string());
        let entries = drive_events(
            view_with_sessions(Vec::new()),
            vec![event(
                1,
                RuntimeEventKind::AgentSessionFailed {
                    session: session.clone(),
                },
            )],
        );
        let reply = entries
            .iter()
            .find(|entry| entry.kind() == "assistant")
            .expect("failed session keeps its reply");
        assert!(reply.label.ends_with(" · failed"), "{}", reply.label);
        assert!(reply.body.contains("partial answer"));
        assert!(reply.body.contains("adapter closed the stream"));
    }

    /// A terminal fact carrying no reply leaves no empty row behind.
    #[test]
    fn a_terminal_session_without_output_pushes_nothing() {
        let entries = drive_events(
            view_with_sessions(Vec::new()),
            vec![event(
                1,
                RuntimeEventKind::AgentSessionCompleted {
                    session: agent_session(
                        "agent-session_1",
                        viden_core::AgentSessionStatus::Completed,
                        None,
                    ),
                },
            )],
        );
        assert!(entries.iter().all(|entry| entry.kind() != "assistant"));
    }
}
