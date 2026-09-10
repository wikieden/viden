use super::{
    audit_panel::AuditPanel,
    composer_buffer::ComposerBuffer,
    decision::{SupervisionAction, SupervisionTarget},
    evidence_panel::EvidencePanel,
    keymap::{InputMode, OverlayKind},
    preferences::{ColorDepth, SettingsPanel},
    workspace_files::WorkspaceFileIndex,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProviderAuthMode {
    ApiKey,
    WebLogin,
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProviderOption {
    pub(super) provider_id: String,
    pub(super) display_name: String,
    pub(super) default_api_base: Option<String>,
    pub(super) default_model: Option<String>,
    pub(super) known_models: Vec<String>,
    pub(super) enabled_models: Vec<String>,
    pub(super) favorite_models: Vec<String>,
    pub(super) api_key_env: Option<String>,
    pub(super) api_base_env: Option<String>,
    pub(super) auth_modes: Vec<ProviderAuthMode>,
}

impl ProviderOption {
    pub(super) fn fixture() -> Vec<Self> {
        vec![
            Self::api_key(
                "anthropic",
                "Anthropic",
                "https://api.anthropic.com",
                "claude-sonnet-4-5",
                &["claude-opus-4-5", "claude-sonnet-4-5", "claude-haiku-4-5"],
                "ANTHROPIC_API_KEY",
            ),
            Self::api_key(
                "deepseek",
                "DeepSeek",
                "https://api.deepseek.com",
                "deepseek-v4-flash",
                &[
                    "deepseek-v4-flash",
                    "deepseek-v4-pro",
                    "deepseek-chat",
                    "deepseek-reasoner",
                ],
                "DEEPSEEK_API_KEY",
            ),
            Self::api_key(
                "dashscope-coding-plan",
                "DashScope Coding Plan",
                "https://coding.dashscope.aliyuncs.com/v1",
                "qwen3.6-plus",
                &[
                    "qwen3.6-plus",
                    "qwen3.5-plus",
                    "qwen3-coder-next",
                    "qwen3-coder-plus",
                    "kimi-k2.5",
                    "glm-5",
                    "MiniMax-M2.5",
                ],
                "DASHSCOPE_CODING_PLAN_API_KEY",
            ),
            Self::api_key(
                "dashscope-tokenplan",
                "DashScope TokenPlan",
                "https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1",
                "qwen3.6-plus",
                &[
                    "qwen3.7-max",
                    "qwen3.6-plus",
                    "qwen3.6-flash",
                    "deepseek-v4-flash",
                    "kimi-k2.6",
                    "glm-5.1",
                    "MiniMax-M2.5",
                ],
                "DASHSCOPE_API_KEY",
            ),
            Self::api_key(
                "openrouter",
                "OpenRouter",
                "https://openrouter.ai/api/v1",
                "deepseek/deepseek-v4-flash",
                &[
                    "openai/gpt-5.2",
                    "anthropic/claude-sonnet-4.5",
                    "qwen/qwen3-coder-plus",
                    "deepseek/deepseek-v4-flash",
                ],
                "OPENROUTER_API_KEY",
            ),
            Self {
                provider_id: "fallback".to_string(),
                display_name: "Fallback".to_string(),
                default_api_base: None,
                default_model: Some("fallback-local".to_string()),
                known_models: vec!["fallback-local".to_string(), "test-local".to_string()],
                enabled_models: vec!["fallback-local".to_string(), "test-local".to_string()],
                favorite_models: Vec::new(),
                api_key_env: None,
                api_base_env: None,
                auth_modes: vec![ProviderAuthMode::Local],
            },
            Self {
                provider_id: "openai".to_string(),
                display_name: "OpenAI".to_string(),
                default_api_base: Some("https://api.openai.com/v1".to_string()),
                default_model: Some("gpt-5.2".to_string()),
                known_models: vec![
                    "gpt-5.2".to_string(),
                    "gpt-5.2-codex".to_string(),
                    "gpt-5.1".to_string(),
                ],
                enabled_models: vec!["gpt-5.2".to_string()],
                favorite_models: Vec::new(),
                api_key_env: Some("OPENAI_API_KEY".to_string()),
                api_base_env: Some("VIDEN_API_BASE".to_string()),
                auth_modes: vec![ProviderAuthMode::WebLogin, ProviderAuthMode::ApiKey],
            },
        ]
    }

    fn api_key(
        provider_id: &str,
        display_name: &str,
        endpoint: &str,
        default_model: &str,
        models: &[&str],
        api_key_env: &str,
    ) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            display_name: display_name.to_string(),
            default_api_base: Some(endpoint.to_string()),
            default_model: Some(default_model.to_string()),
            known_models: models.iter().map(|model| (*model).to_string()).collect(),
            enabled_models: vec![default_model.to_string()],
            favorite_models: Vec::new(),
            api_key_env: Some(api_key_env.to_string()),
            api_base_env: None,
            auth_modes: vec![ProviderAuthMode::ApiKey],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AcpPickerPhase {
    Browse,
    TaskEntry { agent_id: String, draft: String },
}

/// Where the `/git` picker is: choosing an action, or typing the one piece of
/// text an action needs.
///
/// Only `Commit` carries text, and Core refuses an empty commit message
/// outright, so the prompt is a phase rather than an optional field: an empty
/// draft can never be sent by accident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum GitPickerPhase {
    Browse,
    CommitMessage { draft: String },
}

/// Which record's structured conflict content the detail modal is showing.
///
/// Both variants name a record, never a payload: the content is re-read from
/// `RuntimeViewState` on every frame, so a modal can never outlive the fact it
/// was opened on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ConflictDetailTarget {
    Bounce {
        gate_id: String,
    },
    #[allow(dead_code)]
    Lane {
        lane_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum FocusedConversation {
    NativeLane(String),
    AcpSession(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PendingNativeLane {
    AwaitingPreview {
        task: String,
    },
    AwaitingReceipt {
        task: String,
        preview_id: String,
        content_sha256: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PendingAcpStart {
    pub(super) lane_id: String,
    pub(super) agent_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum InteractionPanel {
    Settings(Box<SettingsPanel>),
    Setup {
        selected: usize,
        draft: String,
    },
    ConnectProvider {
        search: String,
        selected: usize,
    },
    #[allow(dead_code)]
    ProviderConfig {
        provider_id: String,
        selected: usize,
    },
    ModelPicker {
        provider_id: Option<String>,
        search: String,
        selected: usize,
    },
    AcpPicker {
        selected: usize,
        phase: AcpPickerPhase,
    },
    /// The `/git` operator source-control picker
    /// (`runtime.operator_git`, GUI-CORE-020).
    GitPicker {
        selected: usize,
        phase: GitPickerPhase,
    },
    NewLaneTask {
        task: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TuiEntry {
    pub(super) label: String,
    pub(super) body: String,
}

impl TuiEntry {
    /// The entry's kind: the leading segment of its label.
    ///
    /// A label may qualify its kind after a `·` — a settled ACP reply names the
    /// session it came from and whether that session failed — so anything
    /// classifying entries by kind must read the segment rather than compare
    /// the whole label, or a qualified reply stops counting as a reply.
    pub(super) fn kind(&self) -> &str {
        self.label
            .split_once(" · ")
            .map(|(kind, _)| kind)
            .unwrap_or(self.label.as_str())
    }
}

/// Local navigation only. Runtime facts and side effects remain Core-owned;
/// changing a lens never confirms project, lane, session, or approval state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Lens {
    Welcome,
    Setup,
    Board,
    Session,
    Decisions,
    Gallery,
}

/// TUI-local state of the supervision decision overlay.
///
/// Presentation only: it remembers which Core record the operator picked, which
/// action row has focus, the reason/feedback line being typed, and the last
/// local refusal. It never caches the record itself — actions and payloads are
/// re-derived from `RuntimeViewState` on every frame and on every keypress, so a
/// gate that changed status between render and Enter is decided on its current
/// facts, not on the row that was drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SupervisionPanel {
    pub(super) target: SupervisionTarget,
    pub(super) focus: usize,
    /// Set while the operator is typing the reason/feedback for `action`. `None`
    /// means the action bar has focus.
    pub(super) input: Option<SupervisionInput>,
    /// Catalog key of the last local refusal (empty required reason, over-limit
    /// text, no replayable actor). Cleared as soon as the operator acts again.
    pub(super) notice: Option<String>,
}

impl SupervisionPanel {
    pub(super) fn new(target: SupervisionTarget) -> Self {
        Self {
            target,
            focus: 0,
            input: None,
            notice: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SupervisionInput {
    pub(super) action: SupervisionAction,
    pub(super) text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OverlayState {
    pub(super) kind: OverlayKind,
    pub(super) filter: String,
    pub(super) selected: usize,
    pub(super) selected_id: Option<String>,
    pub(super) previous_overlay: Option<Box<OverlayState>>,
}

impl OverlayState {
    pub(super) fn new(kind: OverlayKind) -> Self {
        Self {
            kind,
            filter: String::new(),
            selected: 0,
            selected_id: None,
            previous_overlay: None,
        }
    }

    pub(super) fn global_jump(previous_overlay: Option<OverlayState>) -> Self {
        Self {
            previous_overlay: previous_overlay.map(Box::new),
            ..Self::new(OverlayKind::GlobalJump)
        }
    }
}

impl TuiUiState {
    /// Selects one Lane and opens its detail panel.
    ///
    /// Both facts move together on the way in — an operator who picks a Lane
    /// wants to see it — and unwind separately on the way out, so the Lane
    /// stays the target for a command typed in the composer after the panel is
    /// put away. See [`Self::focused_lane`].
    pub(super) fn focus_lane(&mut self, lane_id: impl Into<String>) {
        self.focused_lane = Some(lane_id.into());
        self.lane_detail_open = true;
    }

    /// Drops the Lane selection and its panel together.
    ///
    /// Used where the Lane itself is gone — a Core view that no longer carries
    /// it — rather than for the `Esc` unwind, which walks the two rungs.
    pub(super) fn clear_lane_focus(&mut self) {
        self.focused_lane = None;
        self.lane_detail_open = false;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TuiUiState {
    pub(super) session_id: String,
    pub(super) lens: Lens,
    pub(super) right_rail_open: bool,
    pub(super) provider_catalog: Vec<ProviderOption>,
    pub(super) theme_name: String,
    pub(super) input: ComposerBuffer,
    pub(super) command_selection: usize,
    pub(super) command_palette_hidden_for: Option<String>,
    pub(super) approval_focus: usize,
    pub(super) approval_apply_all: bool,
    pub(super) transcript_scroll: usize,
    pub(super) entries: Vec<TuiEntry>,
    /// The Lane this client's Lane-scoped commands address — the `/git`
    /// target, the evidence scope, the ACP picker's Lane — shown as `L:<lane>`
    /// on the status row.
    ///
    /// It is *selection*, not panel visibility: `/git` is typed in the composer,
    /// so a selection that died with the lane-detail panel could never reach it
    /// (open follow-up 7 in `docs/core-0.3-compatibility.md`). `Esc` still
    /// unwinds it, one rung later than the panel; see
    /// [`super::input::close_focus_on_escape`].
    pub(super) focused_lane: Option<String>,
    /// Whether the lane-detail panel for [`Self::focused_lane`] is on screen.
    ///
    /// Separate from the selection so the first `Esc` can put the panel away
    /// and keep the target. It is meaningless without a focused Lane and is
    /// cleared with it.
    pub(super) lane_detail_open: bool,
    pub(super) focused_conversation: Option<FocusedConversation>,
    pub(super) pending_native_lane: Option<PendingNativeLane>,
    pub(super) pending_acp_start: Option<PendingAcpStart>,
    pub(super) interaction_panel: Option<InteractionPanel>,
    pub(super) input_mode: InputMode,
    pub(super) overlay: Option<OverlayState>,
    /// Present exactly while `overlay` is `OverlayKind::SupervisionDecision`.
    pub(super) supervision: Option<SupervisionPanel>,
    /// Present exactly while `overlay` is `OverlayKind::ConflictContent`.
    pub(super) conflict_detail: Option<ConflictDetailTarget>,
    /// Present exactly while `overlay` is `OverlayKind::AuditTimeline`. It is
    /// dropped on close, so a reopened timeline always re-queries Core instead
    /// of showing a page of unknown age.
    pub(super) audit: Option<AuditPanel>,
    /// Present exactly while `overlay` is `OverlayKind::EvidenceInspector`. It
    /// is dropped on close, so a reopened inspector always re-queries Core
    /// instead of showing a page of unknown age, and the per-row content cache
    /// lives exactly as long as the overlay that filled it.
    pub(super) evidence: Option<EvidencePanel>,
    /// Client-local view of the Core workspace file inventory. It outlives one
    /// overlay so reopening the jump index does not re-read a tree Core
    /// already published, and it holds no authoritative record — see
    /// `workspace_files`.
    pub(super) workspace_files: WorkspaceFileIndex,
    pub(super) idle_ctrl_c_armed: bool,
    pub(super) color_depth: ColorDepth,
    pub(super) preference_diagnostics: Vec<String>,
    /// Agent sessions this client already knew had finished.
    ///
    /// Seeded from every authoritative snapshot and added to as terminal facts
    /// are observed, it is what separates a turn the operator just watched from
    /// replayed history. The distinction cannot be drawn from the event alone:
    /// Core prefixes its whole persisted runtime state to every turn's event
    /// batch, so a session that finished weeks ago re-delivers its terminal
    /// fact on any later turn. A session already terminal in the snapshot this
    /// client started from is history, whatever event carries it.
    pub(super) settled_agent_sessions: std::collections::BTreeSet<String>,
    /// Animation phase for the live-activity pulse. The render model must stay a
    /// pure function of state, so the frame is sampled from the clock once per
    /// draw in the event loop instead of being read inside `render_frame`.
    pub(super) pulse_frame: usize,
}

impl Default for TuiUiState {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            lens: Lens::Welcome,
            right_rail_open: false,
            provider_catalog: Vec::new(),
            theme_name: "aurora-cyan".to_string(),
            input: ComposerBuffer::default(),
            command_selection: 0,
            command_palette_hidden_for: None,
            approval_focus: 0,
            approval_apply_all: false,
            transcript_scroll: 0,
            entries: Vec::new(),
            focused_lane: None,
            lane_detail_open: false,
            focused_conversation: None,
            pending_native_lane: None,
            pending_acp_start: None,
            interaction_panel: None,
            input_mode: InputMode::Normal,
            overlay: None,
            supervision: None,
            conflict_detail: None,
            audit: None,
            evidence: None,
            workspace_files: WorkspaceFileIndex::default(),
            idle_ctrl_c_armed: false,
            color_depth: ColorDepth::Auto,
            preference_diagnostics: Vec::new(),
            settled_agent_sessions: std::collections::BTreeSet::new(),
            pulse_frame: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lens_defaults_to_welcome_and_right_rail_is_closed() {
        let ui = TuiUiState::default();

        assert_eq!(ui.lens, Lens::Welcome);
        assert!(!ui.right_rail_open);
    }
}
