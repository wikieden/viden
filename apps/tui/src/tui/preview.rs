use super::operator_git::OperatorGitSettlement;
use super::state::{
    ConflictDetailTarget, GitPickerPhase, InteractionPanel, Lens, ProviderAuthMode, ProviderOption,
    TuiEntry, TuiState,
};
use super::{render, terminal};
use viden_core::{
    AgentLaneRecord, AgentRole, AgentRoute, ApprovalDefaultAction, ApprovalRequestView,
    ApprovalRisk, ApprovalScope, ApprovalTarget, DataEgressPolicy, DecisionContext, DiffDocument,
    DiffFile, DiffHunk, DiffLine, DiffLineKind, ExecutionTarget, GateStrength, LaneBudget,
    LaneStatus, MutationPolicy, OperatorGitAction, OperatorGitFailureClass, OperatorGitOutcome,
    ProjectConfigState, ProjectProbe, ProviderHealthView, WorkspaceChangeKind,
    WorkspaceSourceStatus, WorkspaceSourceView,
};
use viden_types::{
    ConflictBaseline, ConflictContent, ConflictFile, ConflictHunk, ConflictHunkReason,
    LaneConflictView,
};

pub fn render_preview(provider: &str, model: &str) -> String {
    let state = preview_state(provider, model, "aurora-cyan");
    render::render_frame(&state, 140, 40)
}

pub fn render_idle_preview(provider: &str, model: &str) -> String {
    let state = idle_preview_state(provider, model, "aurora-cyan");
    render::render_frame(&state, 140, 40)
}

pub fn render_command_palette_preview(provider: &str, model: &str) -> String {
    let state = command_palette_preview_state(provider, model, "aurora-cyan");
    render::render_frame(&state, 140, 40)
}

pub fn render_setup_wizard_preview(provider: &str, model: &str) -> String {
    let state = setup_wizard_preview_state(provider, model, "aurora-cyan");
    render::render_frame(&state, 140, 40)
}

pub fn render_provider_selector_preview(provider: &str, model: &str) -> String {
    let state = provider_selector_preview_state(provider, model, "aurora-cyan");
    render::render_frame(&state, 140, 40)
}

pub fn render_provider_detail_preview(provider: &str, model: &str) -> String {
    let state = provider_detail_preview_state(provider, model, "aurora-cyan");
    render::render_frame(&state, 140, 40)
}

pub fn render_model_selector_preview(provider: &str, model: &str) -> String {
    let state = model_selector_preview_state(provider, model, "aurora-cyan");
    render::render_frame(&state, 140, 40)
}

pub fn render_lane_selector_preview(provider: &str, model: &str) -> String {
    let state = lane_selector_preview_state(provider, model, "aurora-cyan");
    render::render_frame(&state, 140, 40)
}

pub fn render_approval_hunks_preview(provider: &str, model: &str) -> String {
    let state = approval_hunks_preview_state(provider, model, "aurora-cyan");
    render::render_frame(&state, 140, 40)
}

pub fn render_git_picker_preview(provider: &str, model: &str) -> String {
    let state = git_picker_preview_state(provider, model, "aurora-cyan");
    render::render_frame(&state, 140, 40)
}

pub fn render_git_outcome_preview(provider: &str, model: &str) -> String {
    let state = git_outcome_preview_state(provider, model, "aurora-cyan");
    render::render_frame(&state, 140, 40)
}

pub fn render_conflict_detail_preview(provider: &str, model: &str) -> String {
    let state = conflict_detail_preview_state(provider, model, "aurora-cyan");
    render::render_frame(&state, 140, 40)
}

pub fn render_live_turn_preview(provider: &str, model: &str) -> String {
    let state = live_turn_preview_state(provider, model, "aurora-cyan");
    render::render_frame(&state, 140, 40)
}

pub fn render_resize_preview(provider: &str, model: &str) -> String {
    let state = resize_preview_state(provider, model, "aurora-cyan");
    render::render_frame(&state, 100, 30)
}

pub fn render_cjk_input_preview(provider: &str, model: &str) -> String {
    let state = cjk_input_preview_state(provider, model, "aurora-cyan");
    render::render_frame(&state, 100, 30)
}

pub fn render_ansi_preview_with_theme(
    provider: &str,
    model: &str,
    theme_name: Option<&str>,
) -> String {
    let theme_name = theme_name.unwrap_or("aurora-cyan");
    let state = preview_state(provider, model, theme_name);
    terminal::render_ansi_preview_with_theme(
        &render::render_frame(&state, 140, 40),
        Some(theme_name),
    )
}

pub fn render_ansi_command_palette_preview_with_theme(
    provider: &str,
    model: &str,
    theme_name: Option<&str>,
) -> String {
    let theme_name = theme_name.unwrap_or("aurora-cyan");
    let state = command_palette_preview_state(provider, model, theme_name);
    terminal::render_ansi_preview_with_theme(
        &render::render_frame(&state, 140, 40),
        Some(theme_name),
    )
}

pub fn render_ansi_setup_wizard_preview_with_theme(
    provider: &str,
    model: &str,
    theme_name: Option<&str>,
) -> String {
    let theme_name = theme_name.unwrap_or("aurora-cyan");
    let state = setup_wizard_preview_state(provider, model, theme_name);
    terminal::render_ansi_preview_with_theme(
        &render::render_frame(&state, 140, 40),
        Some(theme_name),
    )
}

pub fn render_ansi_provider_selector_preview_with_theme(
    provider: &str,
    model: &str,
    theme_name: Option<&str>,
) -> String {
    let theme_name = theme_name.unwrap_or("aurora-cyan");
    let state = provider_selector_preview_state(provider, model, theme_name);
    terminal::render_ansi_preview_with_theme(
        &render::render_frame(&state, 140, 40),
        Some(theme_name),
    )
}

pub fn render_ansi_provider_detail_preview_with_theme(
    provider: &str,
    model: &str,
    theme_name: Option<&str>,
) -> String {
    let theme_name = theme_name.unwrap_or("aurora-cyan");
    let state = provider_detail_preview_state(provider, model, theme_name);
    terminal::render_ansi_preview_with_theme(
        &render::render_frame(&state, 140, 40),
        Some(theme_name),
    )
}

pub fn render_ansi_model_selector_preview_with_theme(
    provider: &str,
    model: &str,
    theme_name: Option<&str>,
) -> String {
    let theme_name = theme_name.unwrap_or("aurora-cyan");
    let state = model_selector_preview_state(provider, model, theme_name);
    terminal::render_ansi_preview_with_theme(
        &render::render_frame(&state, 140, 40),
        Some(theme_name),
    )
}

pub fn render_ansi_lane_selector_preview_with_theme(
    provider: &str,
    model: &str,
    theme_name: Option<&str>,
) -> String {
    let theme_name = theme_name.unwrap_or("aurora-cyan");
    let state = lane_selector_preview_state(provider, model, theme_name);
    terminal::render_ansi_preview_with_theme(
        &render::render_frame(&state, 140, 40),
        Some(theme_name),
    )
}

pub fn render_ansi_approval_hunks_preview_with_theme(
    provider: &str,
    model: &str,
    theme_name: Option<&str>,
) -> String {
    let theme_name = theme_name.unwrap_or("aurora-cyan");
    let state = approval_hunks_preview_state(provider, model, theme_name);
    terminal::render_ansi_preview_with_theme(
        &render::render_frame(&state, 140, 40),
        Some(theme_name),
    )
}

pub fn render_ansi_git_picker_preview_with_theme(
    provider: &str,
    model: &str,
    theme_name: Option<&str>,
) -> String {
    let theme_name = theme_name.unwrap_or("aurora-cyan");
    let state = git_picker_preview_state(provider, model, theme_name);
    terminal::render_ansi_preview_with_theme(
        &render::render_frame(&state, 140, 40),
        Some(theme_name),
    )
}

pub fn render_ansi_git_outcome_preview_with_theme(
    provider: &str,
    model: &str,
    theme_name: Option<&str>,
) -> String {
    let theme_name = theme_name.unwrap_or("aurora-cyan");
    let state = git_outcome_preview_state(provider, model, theme_name);
    terminal::render_ansi_preview_with_theme(
        &render::render_frame(&state, 140, 40),
        Some(theme_name),
    )
}

pub fn render_ansi_conflict_detail_preview_with_theme(
    provider: &str,
    model: &str,
    theme_name: Option<&str>,
) -> String {
    let theme_name = theme_name.unwrap_or("aurora-cyan");
    let state = conflict_detail_preview_state(provider, model, theme_name);
    terminal::render_ansi_preview_with_theme(
        &render::render_frame(&state, 140, 40),
        Some(theme_name),
    )
}

pub fn render_ansi_idle_preview_with_theme(
    provider: &str,
    model: &str,
    theme_name: Option<&str>,
) -> String {
    let theme_name = theme_name.unwrap_or("aurora-cyan");
    let state = idle_preview_state(provider, model, theme_name);
    terminal::render_ansi_preview_with_theme(
        &render::render_frame(&state, 140, 40),
        Some(theme_name),
    )
}

pub fn render_ansi_live_turn_preview_with_theme(
    provider: &str,
    model: &str,
    theme_name: Option<&str>,
) -> String {
    let theme_name = theme_name.unwrap_or("aurora-cyan");
    let state = live_turn_preview_state(provider, model, theme_name);
    terminal::render_ansi_preview_with_theme(
        &render::render_frame(&state, 140, 40),
        Some(theme_name),
    )
}

pub fn render_ansi_resize_preview_with_theme(
    provider: &str,
    model: &str,
    theme_name: Option<&str>,
) -> String {
    let theme_name = theme_name.unwrap_or("aurora-cyan");
    let state = resize_preview_state(provider, model, theme_name);
    terminal::render_ansi_preview_with_theme(
        &render::render_frame(&state, 100, 30),
        Some(theme_name),
    )
}

pub fn render_ansi_cjk_input_preview_with_theme(
    provider: &str,
    model: &str,
    theme_name: Option<&str>,
) -> String {
    let theme_name = theme_name.unwrap_or("aurora-cyan");
    let state = cjk_input_preview_state(provider, model, theme_name);
    terminal::render_ansi_preview_with_theme(
        &render::render_frame(&state, 100, 30),
        Some(theme_name),
    )
}

pub fn render_lane_preview(provider: &str, model: &str) -> String {
    let state = focused_lane_preview_state(provider, model, "aurora-cyan");
    render::render_frame(&state, 140, 40)
}

pub fn render_ansi_lane_preview_with_theme(
    provider: &str,
    model: &str,
    theme_name: Option<&str>,
) -> String {
    let theme_name = theme_name.unwrap_or("aurora-cyan");
    let state = focused_lane_preview_state(provider, model, theme_name);
    terminal::render_ansi_preview_with_theme(
        &render::render_frame(&state, 140, 40),
        Some(theme_name),
    )
}

pub fn render_side_preview(provider: &str, model: &str) -> String {
    let state = preview_state(provider, model, "aurora-cyan");
    render::render_side_frame(&state, 80, 40)
}

pub fn render_ansi_side_preview_with_theme(
    provider: &str,
    model: &str,
    theme_name: Option<&str>,
) -> String {
    let theme_name = theme_name.unwrap_or("aurora-cyan");
    let state = preview_state(provider, model, theme_name);
    terminal::render_ansi_preview_with_theme(
        &render::render_side_frame(&state, 80, 40),
        Some(theme_name),
    )
}

pub fn render_ops_preview(provider: &str, model: &str) -> String {
    let state = preview_state(provider, model, "aurora-cyan");
    render::render_ops_frame(&state, 80, 40)
}

pub fn render_ansi_ops_preview_with_theme(
    provider: &str,
    model: &str,
    theme_name: Option<&str>,
) -> String {
    let theme_name = theme_name.unwrap_or("aurora-cyan");
    let state = preview_state(provider, model, theme_name);
    terminal::render_ansi_preview_with_theme(
        &render::render_ops_frame(&state, 80, 40),
        Some(theme_name),
    )
}

fn preview_state(provider: &str, model: &str, theme_name: &str) -> TuiState {
    // Deterministic previews exercise the complete Core 0.3.2 surface. Runtime
    // startup still derives this set from the negotiated handshake and snapshot.
    let mut state = TuiState {
        capabilities: viden_core::frontend_capabilities(),
        ..TuiState::default()
    };
    state.runtime.snapshot.cwd = std::path::PathBuf::from("~/Documents/GitHub/viden");
    state.runtime.snapshot.provider_family = provider.to_string();
    state.runtime.snapshot.model_label = model.to_string();
    state.runtime.lanes = structured_preview_lanes();
    state.ui.session_id = "c4f2b7e".to_string();
    state.ui.provider_catalog = ProviderOption::fixture();
    state.ui.theme_name = theme_name.to_string();
    state.ui.input = "Add tests for load_config and summarize the diff".into();
    state.ui.entries = vec![
            TuiEntry {
                label: "user".to_string(),
                body: "Add a new function `load_config` that reads a TOML config file and returns `Config`.".to_string(),
            },
            TuiEntry {
                label: "assistant".to_string(),
                body: "I'll add `load_config` to `src/config.rs`, then cover success and error cases with focused tests.".to_string(),
            },
            TuiEntry {
                label: "tool-call".to_string(),
                body: "write_file path: tests/config_tests.rs lines: 1-200".to_string(),
            },
            TuiEntry {
                label: "tool-result".to_string(),
                body: "write_file completed\nWrote 86 lines to tests/config_tests.rs (3.4 KB)".to_string(),
            },
            TuiEntry {
                label: "assistant".to_string(),
                body: "Tests are staged. I found one parser edge case; next wait for test result before updating `src/config.rs`.".to_string(),
            },
            TuiEntry {
                label: "user".to_string(),
                body: "Good. Keep the change narrow and show me the diff before applying.".to_string(),
            },
            TuiEntry {
                label: "tool-call".to_string(),
                body: "write_file path: src/config.rs lines: 1-120".to_string(),
            },
            TuiEntry {
                label: "approval".to_string(),
                body: "Permission request for `write_file`\npath: src/config.rs\nPress y to allow, n/Esc to deny.".to_string(),
            },
            TuiEntry {
                label: "command".to_string(),
                body: [
                    "Test result:",
                    "  status: failed",
                    "  exit code: 101",
                    "  command: cargo test -p viden-cli config_tests",
                    "  duration: 42ms",
                    "  failure summary:",
                    "    - assertion failed in config_tests",
                    "  failing files:",
                    "    - src/config.rs:42:15",
                    "  output tail:",
                    "    thread 'config_tests' panicked at src/config.rs:42:15",
                ]
                .join("\n"),
            },
        ];
    state
}

fn structured_preview_lanes() -> Vec<AgentLaneRecord> {
    [
        (
            "L1",
            AgentRole::Coder,
            LaneStatus::Running,
            "config tests pty/01",
        ),
        (
            "L2",
            AgentRole::Reviewer,
            LaneStatus::WaitingApproval,
            "codex-review config diff",
        ),
    ]
    .into_iter()
    .map(|(id, role, status, summary)| AgentLaneRecord {
        id: id.to_string(),
        task_id: Some(format!("task-{id}")),
        role,
        route: AgentRoute::Terminal,
        gate_strength: GateStrength::Containment,
        mutation_policy: MutationPolicy::ProposeOnly,
        worktree: None,
        branch: None,
        target: ExecutionTarget::Local,
        data_egress: DataEgressPolicy::Deny,
        status,
        budget: LaneBudget::default(),
        active_session_ids: Vec::new(),
        summary: summary.to_string(),
        evidence: if id == "L1" {
            vec![
                "CMD    codex exec test fixes".to_string(),
                "ATTACH /lane tmux L1".to_string(),
            ]
        } else {
            Vec::new()
        },
        run_stats: None,
    })
    .collect()
}

fn focused_lane_preview_state(provider: &str, model: &str, theme_name: &str) -> TuiState {
    let mut state = preview_state(provider, model, theme_name);
    state.ui.focused_lane = Some("L1".to_string());
    state
        .ui
        .entries
        .retain(|entry| entry.label != "approval" && !entry.body.contains("Press y"));
    state.ui.input = "/lane inspect L1".into();
    state
}

fn idle_preview_state(provider: &str, model: &str, theme_name: &str) -> TuiState {
    let mut state = preview_state(provider, model, theme_name);
    state.runtime.lanes.clear();
    state.ui.entries = vec![TuiEntry {
        label: "system".to_string(),
        body: "Viden TUI ready. Enter submits. Esc or Ctrl-C exits.".to_string(),
    }];
    state.ui.input.clear();
    state
}

fn command_palette_preview_state(provider: &str, model: &str, theme_name: &str) -> TuiState {
    let mut state = idle_preview_state(provider, model, theme_name);
    state.runtime.snapshot.provider_family = "deepseek".to_string();
    state.runtime.snapshot.model_label = "deepseek-v4-flash".to_string();
    state.ui.input = "/".into();
    state.ui.command_selection = 0;
    state
}

fn setup_wizard_preview_state(provider: &str, model: &str, theme_name: &str) -> TuiState {
    let mut state = command_palette_preview_state(provider, model, theme_name);
    state.ui.input.clear();
    state.ui.lens = Lens::Setup;
    state.runtime.project_probe = Some(ProjectProbe {
        root: "/workspace/demo".to_string(),
        is_git_repository: true,
        git_root: Some("/workspace/demo".to_string()),
        config_path: "/workspace/demo/viden.toml".to_string(),
        config_state: ProjectConfigState::Missing,
        project_name: None,
        pack: None,
        diagnostics: Vec::new(),
    });
    state.ui.interaction_panel = Some(InteractionPanel::Setup {
        selected: 1,
        draft: "[project]\nname = \"demo\"\npack = \"robot-pack\"\n".to_string(),
    });
    state
}

fn provider_selector_preview_state(provider: &str, model: &str, theme_name: &str) -> TuiState {
    let mut state = command_palette_preview_state(provider, model, theme_name);
    state.ui.input.clear();
    state.ui.interaction_panel = Some(InteractionPanel::ConnectProvider {
        search: String::new(),
        selected: 0,
    });
    state
}

fn provider_detail_preview_state(_provider: &str, _model: &str, theme_name: &str) -> TuiState {
    let mut state = command_palette_preview_state("openai", "gpt-5.2", theme_name);
    state.ui.input.clear();
    state.ui.lens = Lens::Setup;
    state.ui.interaction_panel = None;
    state.runtime.provider = Some(ProviderHealthView {
        provider_id: "openai".to_string(),
        model: "gpt-5.2".to_string(),
        status: "healthy".to_string(),
        request_count: 0,
        error_count: 0,
        last_latency_ms: None,
        average_latency_ms: None,
        tokens_per_second: None,
        credential: None,
    });
    state
}

fn model_selector_preview_state(provider: &str, model: &str, theme_name: &str) -> TuiState {
    let mut state = command_palette_preview_state(provider, model, theme_name);
    state.ui.input.clear();
    state.ui.interaction_panel = Some(InteractionPanel::ModelPicker {
        provider_id: None,
        search: String::new(),
        selected: 0,
    });
    state.ui.provider_catalog = configured_model_preview_catalog();
    state
}

fn configured_model_preview_catalog() -> Vec<ProviderOption> {
    vec![ProviderOption {
        provider_id: "deepseek".to_string(),
        display_name: "DeepSeek".to_string(),
        default_api_base: Some("https://api.deepseek.com".to_string()),
        default_model: Some("deepseek-v4-flash".to_string()),
        known_models: vec![
            "deepseek-v4-flash".to_string(),
            "deepseek-v4-pro".to_string(),
            "deepseek-chat".to_string(),
        ],
        enabled_models: vec![
            "deepseek-v4-flash".to_string(),
            "deepseek-v4-pro".to_string(),
        ],
        favorite_models: vec!["deepseek-v4-pro".to_string()],
        api_key_env: None,
        api_base_env: Some("DEEPSEEK_API_BASE".to_string()),
        auth_modes: vec![ProviderAuthMode::ApiKey],
    }]
}

fn lane_selector_preview_state(provider: &str, model: &str, theme_name: &str) -> TuiState {
    let mut state = command_palette_preview_state(provider, model, theme_name);
    state.runtime.lanes = structured_preview_lanes();
    state.ui.input = "/lane".into();
    state.ui.command_selection = 0;
    state
}

fn live_turn_preview_state(provider: &str, model: &str, theme_name: &str) -> TuiState {
    let mut state = idle_preview_state(provider, model, theme_name);
    state.ui.entries = vec![
        TuiEntry {
            label: "system".to_string(),
            body: "Viden TUI ready. Enter submits. Esc or Ctrl-C exits.".to_string(),
        },
        TuiEntry {
            label: "user".to_string(),
            body: "Refactor the config loader, then run focused tests.".to_string(),
        },
    ];
    state.runtime.assistant_stream = "Working on the config loader...".to_string();
    state.runtime.lanes.clear();
    state.ui.input = "Add a note about the validation result".into();
    state
}

fn resize_preview_state(provider: &str, model: &str, theme_name: &str) -> TuiState {
    let mut state = live_turn_preview_state(provider, model, theme_name);
    state.ui.input = "Resize-safe redraw check".into();
    state.ui.entries.push(TuiEntry {
        label: "system".to_string(),
        body: "Resize-safe redraw check: stale borders cleared; composer and panels reflow from one frame.".to_string(),
    });
    state
}

/// The approval overlay with Core's structured decision context
/// (`runtime.structured_diff`, GUI-CORE-012).
///
/// Every row here comes from a `DecisionContext` value, never from
/// `input_preview`: the point of the evidence is that a reviewer sees Core's
/// hunks and Core's line numbers, including the file whose rows the byte bound
/// dropped.
fn approval_hunks_preview_state(provider: &str, model: &str, theme_name: &str) -> TuiState {
    let mut state = preview_state(provider, model, theme_name);
    state.ui.input = "".into();
    state.runtime.pending_approvals = vec![ApprovalRequestView {
        id: "approval-preview-edit".to_string(),
        tool_name: "edit_file".to_string(),
        title: "Approve edit_file".to_string(),
        message: "edit_file requires approval".to_string(),
        input_preview: "path: src/config.rs".to_string(),
        is_mutating: true,
        reason: Some("edit_file requires approval".to_string()),
        owner: Default::default(),
        risk: ApprovalRisk::Medium,
        target: ApprovalTarget {
            kind: "edit_file".to_string(),
            display: "src/config.rs".to_string(),
            canonical_ref: None,
        },
        allowed_scopes: vec![ApprovalScope::Once],
        policy_reason_key: "permission.requires_approval".to_string(),
        policy_reason_args: Default::default(),
        expires_at: 0,
        default_action: ApprovalDefaultAction::Deny,
        audit_id: "audit-preview-edit".to_string(),
        decision_context: Some(DecisionContext {
            diff: Some(DiffDocument {
                files: vec![
                    DiffFile {
                        path: "src/config.rs".to_string(),
                        old_path: None,
                        kind: WorkspaceChangeKind::Modified,
                        binary: false,
                        omitted: false,
                        additions: 2,
                        deletions: 1,
                        hunks: vec![DiffHunk {
                            old_start: 18,
                            old_lines: 4,
                            new_start: 18,
                            new_lines: 5,
                            header: Some("pub fn load_config".to_string()),
                            lines: vec![
                                preview_diff_line(
                                    DiffLineKind::Context,
                                    "pub fn load_config(path: &Path) -> Result<Config> {",
                                    Some(18),
                                    Some(18),
                                ),
                                preview_diff_line(
                                    DiffLineKind::Removed,
                                    "    let raw = std::fs::read_to_string(path)?;",
                                    Some(19),
                                    None,
                                ),
                                preview_diff_line(
                                    DiffLineKind::Added,
                                    "    let raw = read_config_file(path)?;",
                                    None,
                                    Some(19),
                                ),
                                preview_diff_line(
                                    DiffLineKind::Added,
                                    "    let parsed = toml::from_str(&raw)?;",
                                    None,
                                    Some(20),
                                ),
                                preview_diff_line(
                                    DiffLineKind::Context,
                                    "    Ok(parsed)",
                                    Some(20),
                                    Some(21),
                                ),
                            ],
                        }],
                    },
                    DiffFile {
                        path: "tests/config_tests.rs".to_string(),
                        old_path: None,
                        kind: WorkspaceChangeKind::Added,
                        binary: false,
                        omitted: true,
                        additions: 86,
                        deletions: 0,
                        hunks: Vec::new(),
                    },
                ],
                truncated: true,
                byte_limit: 65536,
            }),
            base_sha256: Some(
                "9c1185a5c5e9fc54612808977ee8f548b2258d31a0f4e6e6f2a1b9c3d4e5f607".to_string(),
            ),
        }),
    }];
    let mut overlay = super::state::OverlayState::new(super::keymap::OverlayKind::Approval);
    overlay.selected_id = Some("approval-preview-edit".to_string());
    state.ui.overlay = Some(overlay);
    state.ui.lens = Lens::Decisions;
    state
}

fn preview_diff_line(
    kind: DiffLineKind,
    content: &str,
    old_line: Option<u32>,
    new_line: Option<u32>,
) -> DiffLine {
    DiffLine {
        kind,
        content: content.to_string(),
        old_line,
        new_line,
    }
}

/// The `/git` operator source-control picker
/// (`runtime.operator_git`, GUI-CORE-020).
///
/// The context row under the choices names the target and the source facts
/// Core published; nothing in it is sampled locally.
fn git_picker_preview_state(provider: &str, model: &str, theme_name: &str) -> TuiState {
    let mut state = preview_state(provider, model, theme_name);
    state.ui.input = "".into();
    state.runtime.workspace_source = Some(WorkspaceSourceView {
        status: WorkspaceSourceStatus::Ready,
        branch: Some("codex/v3-tui-client".to_string()),
        worktree: Some("workspace/viden".to_string()),
        ahead: 2,
        behind: 0,
        added: 3,
        deleted: 1,
        dirty: true,
    });
    state.ui.interaction_panel = Some(InteractionPanel::GitPicker {
        selected: 1,
        phase: GitPickerPhase::Browse,
    });
    state
}

/// Both settled shapes of one operator action, side by side: a `Completed`
/// outcome rendered from the resampled source, and a `Failed` outcome rendered
/// from the typed class plus its recovery. Neither reads a fact out of git's
/// output text.
fn git_outcome_preview_state(provider: &str, model: &str, theme_name: &str) -> TuiState {
    let mut state = preview_state(provider, model, theme_name);
    state.ui.input = "".into();
    state.ui.entries.clear();
    let source = WorkspaceSourceView {
        status: WorkspaceSourceStatus::Ready,
        branch: Some("codex/v3-tui-client".to_string()),
        worktree: Some("workspace/viden".to_string()),
        ahead: 3,
        behind: 0,
        added: 0,
        deleted: 0,
        dirty: false,
    };
    for settlement in [
        OperatorGitSettlement::Finished {
            action: OperatorGitAction::Commit {
                message: "feat(tui): add the /git operator action picker".to_string(),
            },
            outcome: Box::new(OperatorGitOutcome::Completed {
                output: "main 1a2b3c4 feat(tui): add the /git operator action picker\n                          2 files changed, 118 insertions"
                    .to_string(),
                truncated: false,
                source: source.clone(),
            }),
            audit_id: "audit-preview-commit".to_string(),
        },
        OperatorGitSettlement::Finished {
            action: OperatorGitAction::Push {
                remote: None,
                set_upstream: false,
            },
            outcome: Box::new(OperatorGitOutcome::Failed {
                class: OperatorGitFailureClass::NoUpstream,
                detail: "the current branch has no upstream branch; push with set_upstream                          to create one"
                    .to_string(),
            }),
            audit_id: "audit-preview-push".to_string(),
        },
    ] {
        let entry = super::app::operator_git_entry_for_preview(&state, &settlement);
        state.ui.entries.push(entry);
    }
    state.ui.lens = Lens::Session;
    state
}

/// The read-only conflict content modal
/// (`runtime.conflict_content`, GUI-CORE-015).
///
/// Three labelled sides for one rejected hunk plus a file the byte bound
/// dropped, so the evidence shows both the content and the two honesty
/// markers. The modal offers no resolution, because Core computed none.
fn conflict_detail_preview_state(provider: &str, model: &str, theme_name: &str) -> TuiState {
    let mut state = preview_state(provider, model, theme_name);
    state.ui.input = "".into();
    state.runtime.lane_conflicts = vec![LaneConflictView {
        lane_id: "L1".to_string(),
        summary: "patch conflict: expected hunk context was not found".to_string(),
        paths: vec!["src/config.rs".to_string()],
        timestamp: None,
        content: Some(ConflictContent {
            baseline: ConflictBaseline::Revision {
                sha: "9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f9f".to_string(),
            },
            files: vec![
                ConflictFile {
                    path: "src/config.rs".to_string(),
                    hunks: vec![ConflictHunk {
                        ours_start: 19,
                        ours: vec!["    let raw = std::fs::read_to_string(path)?;".to_string()],
                        theirs_start: 19,
                        theirs: vec!["    let raw = read_config_file(path)?;".to_string()],
                        base: Some(vec!["    let raw = load_raw(path)?;".to_string()]),
                        reason: ConflictHunkReason::ContextMismatch,
                    }],
                    omitted: false,
                },
                ConflictFile {
                    path: "tests/config_tests.rs".to_string(),
                    hunks: Vec::new(),
                    omitted: true,
                },
            ],
            truncated: true,
        }),
    }];
    state.ui.conflict_detail = Some(ConflictDetailTarget::Lane {
        lane_id: "L1".to_string(),
    });
    state.ui.overlay = Some(super::state::OverlayState::new(
        super::keymap::OverlayKind::ConflictContent,
    ));
    state.ui.lens = Lens::Decisions;
    state
}

fn cjk_input_preview_state(provider: &str, model: &str, theme_name: &str) -> TuiState {
    let mut state = idle_preview_state(provider, model, theme_name);
    state.ui.input = "你好，帮我检查当前变更".into();
    state.ui.entries.push(TuiEntry {
        label: "user".to_string(),
        body: "中文输入法候选窗应该靠近 composer 光标，输入区要保持足够高。".to_string(),
    });
    state
}

#[cfg(test)]
fn render_task6_lens_preview(lens: Lens, width: u16) -> String {
    let mut state = if lens == Lens::Welcome {
        idle_preview_state("fallback", "test-local", "aurora-cyan")
    } else {
        preview_state("fallback", "test-local", "aurora-cyan")
    };
    state.ui.lens = lens;
    if lens == Lens::Session {
        state.ui.focused_lane = Some("L1".to_string());
        state.ui.session_id = "session-preview-L1".to_string();
        if let Some(lane) = state.runtime.lanes.iter_mut().find(|lane| lane.id == "L1") {
            lane.active_session_ids = vec![state.ui.session_id.clone()];
        }
    }
    render::render_frame(&state, width, 40)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The render model must not read the wall clock: two renders of the same
    /// state separated by more than one animation period have to be identical,
    /// otherwise every render-equality assertion is a latent flake.
    #[test]
    fn lens_render_models_ignore_elapsed_wall_clock_time() {
        for lens in [Lens::Board, Lens::Session] {
            let first = render_task6_lens_preview(lens, 160);
            std::thread::sleep(std::time::Duration::from_millis(400));
            let second = render_task6_lens_preview(lens, 160);

            assert_eq!(first, second, "{lens:?} render drifted with wall clock");
        }
    }

    #[test]
    fn all_lenses_have_deterministic_80_112_160_render_models() {
        for lens in [
            Lens::Welcome,
            Lens::Setup,
            Lens::Board,
            Lens::Session,
            Lens::Decisions,
            Lens::Gallery,
        ] {
            for width in [80_u16, 112, 160] {
                let first = render_task6_lens_preview(lens, width);
                let second = render_task6_lens_preview(lens, width);

                assert_eq!(first, second, "{lens:?} at {width}");
                assert!(
                    first
                        .lines()
                        .all(|line| { crate::tui::text::char_width(line) == usize::from(width) }),
                    "{lens:?} physical width {width}"
                );
                let identity = match lens {
                    Lens::Welcome => "Ask anything",
                    Lens::Setup => "SETUP",
                    Lens::Board => "LANE BOARD",
                    Lens::Session => "COCKPIT",
                    Lens::Decisions => "DECISIONS",
                    Lens::Gallery => "GALLERY",
                };
                assert!(first.contains(identity), "{lens:?} at {width}");
            }
        }
    }

    #[test]
    fn previews_use_stable_demo_workspace_snapshot() {
        let main = render_preview("fallback", "test-local");
        let idle = render_idle_preview("fallback", "test-local");
        let live_turn = render_live_turn_preview("fallback", "test-local");
        let resize = render_resize_preview("fallback", "test-local");
        let cjk_input = render_cjk_input_preview("fallback", "test-local");
        let command_palette = render_command_palette_preview("deepseek", "deepseek-v4-flash");
        let setup_wizard = render_setup_wizard_preview("deepseek", "deepseek-v4-flash");
        let provider_selector = render_provider_selector_preview("deepseek", "deepseek-v4-flash");
        let provider_detail = render_provider_detail_preview("deepseek", "deepseek-v4-flash");
        let model_selector = render_model_selector_preview("deepseek", "deepseek-v4-flash");
        let lane_selector = render_lane_selector_preview("deepseek", "deepseek-v4-flash");
        let side = render_side_preview("fallback", "test-local");
        let ops = render_ops_preview("fallback", "test-local");

        assert_eq!(
            preview_state("fallback", "test-local", "aurora-cyan")
                .runtime
                .snapshot
                .cwd,
            std::path::PathBuf::from("~/Documents/GitHub/viden")
        );
        assert!(live_turn.contains("LIVE WORK"));
        assert!(live_turn.contains("Viden working"));
        assert!(live_turn.contains("live provider request"));
        assert!(resize.contains("LIVE WORK"));
        assert!(resize.contains("Resize-safe redraw check"));
        assert!(cjk_input.contains("你好，帮我检查当前变更"));
        assert!(main.contains("src/config.rs"));
        assert!(idle.contains("Ask anything"));
        assert!(idle.contains("ctrl+p commands"));
        assert!(!idle.contains("TRANSCRIPT"));
        assert!(command_palette.contains("COMMANDS"));
        assert!(command_palette.contains("/connect"));
        assert!(command_palette.contains("Configure a Core provider"));
        assert!(setup_wizard.contains("SETUP SELECTOR"));
        assert!(setup_wizard.contains("DRAFT viden.toml"));
        assert!(setup_wizard.contains("pack = \"robot-pack\""));
        assert!(provider_selector.contains("Connect a provider"));
        assert!(provider_selector.contains("DeepSeek"));
        assert!(provider_selector.contains("OpenRouter"));
        assert!(!provider_selector.contains("DEEPSEEK_API_KEY"));
        assert!(!provider_selector.contains("default endpoint"));
        assert!(provider_detail.contains("PROVIDER openai"));
        assert!(provider_detail.contains("TRUSTED INGRESS unavailable"));
        assert!(!provider_detail.contains("API key"));
        assert!(!provider_detail.contains("/provider key"));
        assert!(model_selector.contains("Select model"));
        assert!(model_selector.contains("deepseek-v4-flash"));
        assert!(model_selector.contains("deepseek"));
        assert!(!model_selector.contains("fallback-local"));
        assert!(lane_selector.contains("/lane"));
        assert!(!idle.contains("APPROVAL REQUIRED"));
        assert!(!main.contains("APPROVAL REQUIRED"));
        assert!(main.contains("tests/config_tests.rs"));
        assert!(side.contains("~/Documents/GitHub/viden"));
        assert!(ops.contains("ROOT"));
        assert!(ops.contains("MCP / CONTEXT"));
        assert!(ops.contains("no structured evidence yet"));
        assert!(!main.contains("docs/previews/generated"));
        assert!(!main.contains("scripts/tui-previews.sh"));
    }

    #[test]
    fn lane_preview_focuses_lane_detail_without_approval_overlay() {
        let preview = render_lane_preview("fallback", "test-local");

        assert!(preview.contains("LANE DETAIL"));
        assert!(preview.contains("/lane inspect L1"));
        assert!(!preview.contains("APPROVAL REQUIRED"));
    }
}
