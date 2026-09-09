use std::env;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use viden_config::{CliOverrides, load_config};
use viden_core::{LocalCoreTransport, StatefulCoreClient};
use viden_provider::list_supported_provider_strings;
use viden_runtime::{EngineEvent, RuntimeSupervisor, bootstrap_runtime_with_resolved_config};
use viden_types::{
    ApprovalResponse, PermissionPrompt, ResolvedUiPreferences, UiColorMode, UiPreferences, UiSkin,
    resolve_ui_preferences,
};

use viden_tui as tui;

fn main() {
    if let Err(err) = run() {
        eprintln!("viden: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cwd = env::current_dir().map_err(|err| err.to_string())?;
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("viden {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_startup_help();
        return Ok(());
    }
    let startup = parse_startup_options(&args)?;
    let cli_config = CliOverrides {
        provider: startup.provider.clone(),
        model: startup.model.clone(),
        api_base: startup.api_base.clone(),
        api_key: startup.api_key.clone(),
        provider_plugin_dirs: startup.provider_plugin_dirs.clone(),
        permission_mode: startup.permission_mode,
        session_home: startup.session_home.clone(),
        request_timeout_secs: startup.request_timeout_secs,
        max_retries: startup.max_retries,
        config_path: startup.config_path.clone(),
        ui: None,
    };
    let mut resolved_config = load_config(&cwd, &cli_config)?;
    if let Some(skin) = startup.tui_theme.as_deref() {
        resolved_config.ui = ui_preferences_with_skin(&resolved_config.ui, skin);
    }
    let preview_provider = resolved_config.provider.as_str();
    let preview_model = resolved_config.model.as_deref().unwrap_or("default");
    if startup.tui_preview || startup.tui_preview_ansi {
        if startup.tui_preview_ansi {
            print!(
                "{}",
                tui::render_ansi_preview_with_theme(
                    preview_provider,
                    preview_model,
                    startup.tui_theme.as_deref()
                )
            );
        } else {
            println!("{}", tui::render_preview(preview_provider, preview_model));
        }
        return Ok(());
    }
    if startup.tui_preview_idle || startup.tui_preview_idle_ansi {
        if startup.tui_preview_idle_ansi {
            print!(
                "{}",
                tui::render_ansi_idle_preview_with_theme(
                    preview_provider,
                    preview_model,
                    startup.tui_theme.as_deref()
                )
            );
        } else {
            println!(
                "{}",
                tui::render_idle_preview(preview_provider, preview_model)
            );
        }
        return Ok(());
    }
    if startup.tui_preview_live_turn || startup.tui_preview_live_turn_ansi {
        if startup.tui_preview_live_turn_ansi {
            print!(
                "{}",
                tui::render_ansi_live_turn_preview_with_theme(
                    preview_provider,
                    preview_model,
                    startup.tui_theme.as_deref()
                )
            );
        } else {
            println!(
                "{}",
                tui::render_live_turn_preview(preview_provider, preview_model)
            );
        }
        return Ok(());
    }
    if startup.tui_preview_resize || startup.tui_preview_resize_ansi {
        if startup.tui_preview_resize_ansi {
            print!(
                "{}",
                tui::render_ansi_resize_preview_with_theme(
                    preview_provider,
                    preview_model,
                    startup.tui_theme.as_deref()
                )
            );
        } else {
            println!(
                "{}",
                tui::render_resize_preview(preview_provider, preview_model)
            );
        }
        return Ok(());
    }
    if startup.tui_preview_cjk_input || startup.tui_preview_cjk_input_ansi {
        if startup.tui_preview_cjk_input_ansi {
            print!(
                "{}",
                tui::render_ansi_cjk_input_preview_with_theme(
                    preview_provider,
                    preview_model,
                    startup.tui_theme.as_deref()
                )
            );
        } else {
            println!(
                "{}",
                tui::render_cjk_input_preview(preview_provider, preview_model)
            );
        }
        return Ok(());
    }
    if startup.tui_preview_command_palette || startup.tui_preview_command_palette_ansi {
        if startup.tui_preview_command_palette_ansi {
            print!(
                "{}",
                tui::render_ansi_command_palette_preview_with_theme(
                    preview_provider,
                    preview_model,
                    startup.tui_theme.as_deref()
                )
            );
        } else {
            println!(
                "{}",
                tui::render_command_palette_preview(preview_provider, preview_model)
            );
        }
        return Ok(());
    }
    if startup.tui_preview_setup_wizard || startup.tui_preview_setup_wizard_ansi {
        if startup.tui_preview_setup_wizard_ansi {
            print!(
                "{}",
                tui::render_ansi_setup_wizard_preview_with_theme(
                    preview_provider,
                    preview_model,
                    startup.tui_theme.as_deref()
                )
            );
        } else {
            println!(
                "{}",
                tui::render_setup_wizard_preview(preview_provider, preview_model)
            );
        }
        return Ok(());
    }
    if startup.tui_preview_provider_selector || startup.tui_preview_provider_selector_ansi {
        if startup.tui_preview_provider_selector_ansi {
            print!(
                "{}",
                tui::render_ansi_provider_selector_preview_with_theme(
                    preview_provider,
                    preview_model,
                    startup.tui_theme.as_deref()
                )
            );
        } else {
            println!(
                "{}",
                tui::render_provider_selector_preview(preview_provider, preview_model)
            );
        }
        return Ok(());
    }
    if startup.tui_preview_provider_detail || startup.tui_preview_provider_detail_ansi {
        if startup.tui_preview_provider_detail_ansi {
            print!(
                "{}",
                tui::render_ansi_provider_detail_preview_with_theme(
                    preview_provider,
                    preview_model,
                    startup.tui_theme.as_deref()
                )
            );
        } else {
            println!(
                "{}",
                tui::render_provider_detail_preview(preview_provider, preview_model)
            );
        }
        return Ok(());
    }
    if startup.tui_preview_model_selector || startup.tui_preview_model_selector_ansi {
        if startup.tui_preview_model_selector_ansi {
            print!(
                "{}",
                tui::render_ansi_model_selector_preview_with_theme(
                    preview_provider,
                    preview_model,
                    startup.tui_theme.as_deref()
                )
            );
        } else {
            println!(
                "{}",
                tui::render_model_selector_preview(preview_provider, preview_model)
            );
        }
        return Ok(());
    }
    if startup.tui_preview_approval_hunks || startup.tui_preview_approval_hunks_ansi {
        if startup.tui_preview_approval_hunks_ansi {
            print!(
                "{}",
                tui::render_ansi_approval_hunks_preview_with_theme(
                    preview_provider,
                    preview_model,
                    startup.tui_theme.as_deref()
                )
            );
        } else {
            println!(
                "{}",
                tui::render_approval_hunks_preview(preview_provider, preview_model)
            );
        }
        return Ok(());
    }
    if startup.tui_preview_lane_selector || startup.tui_preview_lane_selector_ansi {
        if startup.tui_preview_lane_selector_ansi {
            print!(
                "{}",
                tui::render_ansi_lane_selector_preview_with_theme(
                    preview_provider,
                    preview_model,
                    startup.tui_theme.as_deref()
                )
            );
        } else {
            println!(
                "{}",
                tui::render_lane_selector_preview(preview_provider, preview_model)
            );
        }
        return Ok(());
    }
    if startup.tui_preview_lane || startup.tui_preview_lane_ansi {
        if startup.tui_preview_lane_ansi {
            print!(
                "{}",
                tui::render_ansi_lane_preview_with_theme(
                    preview_provider,
                    preview_model,
                    startup.tui_theme.as_deref()
                )
            );
        } else {
            println!(
                "{}",
                tui::render_lane_preview(preview_provider, preview_model)
            );
        }
        return Ok(());
    }
    if startup.tui_preview_side || startup.tui_preview_side_ansi {
        if startup.tui_preview_side_ansi {
            print!(
                "{}",
                tui::render_ansi_side_preview_with_theme(
                    preview_provider,
                    preview_model,
                    startup.tui_theme.as_deref()
                )
            );
        } else {
            println!(
                "{}",
                tui::render_side_preview(preview_provider, preview_model)
            );
        }
        return Ok(());
    }
    if startup.tui_preview_side_2 || startup.tui_preview_side_2_ansi {
        if startup.tui_preview_side_2_ansi {
            print!(
                "{}",
                tui::render_ansi_ops_preview_with_theme(
                    preview_provider,
                    preview_model,
                    startup.tui_theme.as_deref()
                )
            );
        } else {
            println!(
                "{}",
                tui::render_ops_preview(preview_provider, preview_model)
            );
        }
        return Ok(());
    }
    let bootstrap =
        bootstrap_runtime_with_resolved_config(&cwd, resolved_config, startup.summary_overrides())?;
    let provider_summary = bootstrap.provider_summary;
    let mut engine = bootstrap.engine;

    if startup.should_start_tui() {
        if startup.resume_selector.is_some() {
            return Err(
                "--resume requires a Core resume command before it can be used with the V3 TUI; use --no-tui for the legacy REPL"
                    .to_string(),
            );
        }
        let supervisor = RuntimeSupervisor::start(engine);
        let transport = LocalCoreTransport::new(supervisor);
        let client = StatefulCoreClient::new(transport);
        let mut options = tui::TuiOptions::new(&provider_summary);
        if startup.tui_startup_check {
            options = options.with_startup_check();
        }
        return tui::run_tui(client, options).map_err(|error| error.to_string());
    }

    let stdin = io::stdin();
    let mut stdin = stdin.lock();

    if let Some(selector) = startup.resume_selector.as_deref() {
        let mut approver = |prompt: PermissionPrompt| prompt_for_approval(prompt, &mut stdin);
        for event in
            engine.process_input_with_approval(&format!("/resume {selector}"), &mut approver)?
        {
            render_event(event);
        }
    }

    println!(
        "Viden session {}. Type /help for commands, Ctrl-D to exit.",
        engine.session_id()
    );
    println!("Startup provider: {provider_summary}");

    loop {
        print!("viden> ");
        io::stdout().flush().map_err(|err| err.to_string())?;
        let Some(line) = read_lossy_line(&mut stdin).map_err(|err| err.to_string())? else {
            println!();
            break;
        };
        let trimmed = line.trim();
        if is_exit_command(trimmed) {
            break;
        }
        let mut approver = |prompt: PermissionPrompt| prompt_for_approval(prompt, &mut stdin);
        let events = engine.process_input_with_approval(trimmed, &mut approver)?;
        for event in events {
            render_event(event);
        }
    }

    Ok(())
}

fn ui_preferences_with_skin(base: &ResolvedUiPreferences, skin: &str) -> ResolvedUiPreferences {
    let skin = match skin {
        "ice" => UiSkin::Ice,
        "mono" => UiSkin::Mono,
        "amber" => UiSkin::Amber,
        "phosphor" => UiSkin::Phosphor,
        _ => UiSkin::Aurora,
    };
    let mode = if UiPreferences::is_valid_effective_pair(skin, base.mode) {
        base.mode
    } else {
        UiColorMode::Dark
    };
    let profile = UiPreferences {
        locale: base.locale,
        skin,
        mode,
        density: base.density,
        motion: base.motion,
    };
    let mut resolved = resolve_ui_preferences(Some(profile), None, None, profile);
    resolved.diagnostics.extend(base.diagnostics.clone());
    resolved
}

#[cfg(test)]
fn format_provider_plugin_error(err: viden_provider::ProviderPluginError) -> String {
    let path = if err.path.as_os_str().is_empty() {
        "<registry>".to_string()
    } else {
        err.path.display().to_string()
    };
    format!(
        "provider plugin loading failed\n  kind: {:?}\n  path: {}\n  message: {}\n  detail: {}",
        err.kind, path, err.message, err
    )
}

fn is_exit_command(input: &str) -> bool {
    matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "exit" | "quit" | "/exit" | "/quit"
    )
}

#[derive(Debug, Default)]
struct StartupOptions {
    provider: Option<String>,
    model: Option<String>,
    api_base: Option<String>,
    api_key: Option<String>,
    provider_plugin_dirs: Vec<PathBuf>,
    permission_mode: Option<viden_types::PermissionMode>,
    session_home: Option<PathBuf>,
    request_timeout_secs: Option<u64>,
    max_retries: Option<u32>,
    config_path: Option<PathBuf>,
    resume_selector: Option<String>,
    tui: bool,
    no_tui: bool,
    tui_screen: Option<String>,
    tui_preview: bool,
    tui_preview_ansi: bool,
    tui_preview_idle: bool,
    tui_preview_idle_ansi: bool,
    tui_preview_live_turn: bool,
    tui_preview_live_turn_ansi: bool,
    tui_preview_resize: bool,
    tui_preview_resize_ansi: bool,
    tui_preview_cjk_input: bool,
    tui_preview_cjk_input_ansi: bool,
    tui_preview_command_palette: bool,
    tui_preview_command_palette_ansi: bool,
    tui_preview_setup_wizard: bool,
    tui_preview_setup_wizard_ansi: bool,
    tui_preview_provider_selector: bool,
    tui_preview_provider_selector_ansi: bool,
    tui_preview_provider_detail: bool,
    tui_preview_provider_detail_ansi: bool,
    tui_preview_model_selector: bool,
    tui_preview_model_selector_ansi: bool,
    tui_preview_approval_hunks: bool,
    tui_preview_approval_hunks_ansi: bool,
    tui_preview_lane_selector: bool,
    tui_preview_lane_selector_ansi: bool,
    tui_preview_lane: bool,
    tui_preview_lane_ansi: bool,
    tui_preview_side: bool,
    tui_preview_side_ansi: bool,
    tui_preview_side_2: bool,
    tui_preview_side_2_ansi: bool,
    tui_startup_check: bool,
    tui_theme: Option<String>,
}

impl StartupOptions {
    fn should_start_tui(&self) -> bool {
        self.tui || self.tui_screen.is_some() || !self.no_tui
    }

    fn summary_overrides(&self) -> Vec<String> {
        let mut overrides = Vec::new();
        if self.provider.is_some() {
            overrides.push("--provider".to_string());
        }
        if self.model.is_some() {
            overrides.push("--model".to_string());
        }
        if self.api_base.is_some() {
            overrides.push("--api-base".to_string());
        }
        if self.api_key.is_some() {
            overrides.push("--api-key".to_string());
        }
        if !self.provider_plugin_dirs.is_empty() {
            overrides.push("--provider-plugin-dir".to_string());
        }
        if self.permission_mode.is_some() {
            overrides.push("--permissions".to_string());
        }
        if self.session_home.is_some() {
            overrides.push("--session-home".to_string());
        }
        if self.request_timeout_secs.is_some() {
            overrides.push("--request-timeout".to_string());
        }
        if self.max_retries.is_some() {
            overrides.push("--max-retries".to_string());
        }
        if self.config_path.is_some() {
            overrides.push("--config".to_string());
        }
        if self.resume_selector.is_some() {
            overrides.push("--resume".to_string());
        }
        if self.tui {
            overrides.push("--tui".to_string());
        }
        if self.no_tui {
            overrides.push("--no-tui".to_string());
        }
        if self.tui_screen.is_some() {
            overrides.push("--tui-screen".to_string());
        }
        if self.tui_preview {
            overrides.push("--tui-preview".to_string());
        }
        if self.tui_preview_ansi {
            overrides.push("--tui-preview-ansi".to_string());
        }
        if self.tui_preview_idle {
            overrides.push("--tui-preview-idle".to_string());
        }
        if self.tui_preview_idle_ansi {
            overrides.push("--tui-preview-idle-ansi".to_string());
        }
        if self.tui_preview_live_turn {
            overrides.push("--tui-preview-live-turn".to_string());
        }
        if self.tui_preview_live_turn_ansi {
            overrides.push("--tui-preview-live-turn-ansi".to_string());
        }
        if self.tui_preview_resize {
            overrides.push("--tui-preview-resize".to_string());
        }
        if self.tui_preview_resize_ansi {
            overrides.push("--tui-preview-resize-ansi".to_string());
        }
        if self.tui_preview_cjk_input {
            overrides.push("--tui-preview-cjk-input".to_string());
        }
        if self.tui_preview_cjk_input_ansi {
            overrides.push("--tui-preview-cjk-input-ansi".to_string());
        }
        if self.tui_preview_command_palette {
            overrides.push("--tui-preview-command-palette".to_string());
        }
        if self.tui_preview_command_palette_ansi {
            overrides.push("--tui-preview-command-palette-ansi".to_string());
        }
        if self.tui_preview_setup_wizard {
            overrides.push("--tui-preview-setup-wizard".to_string());
        }
        if self.tui_preview_setup_wizard_ansi {
            overrides.push("--tui-preview-setup-wizard-ansi".to_string());
        }
        if self.tui_preview_provider_selector {
            overrides.push("--tui-preview-provider-selector".to_string());
        }
        if self.tui_preview_provider_selector_ansi {
            overrides.push("--tui-preview-provider-selector-ansi".to_string());
        }
        if self.tui_preview_provider_detail {
            overrides.push("--tui-preview-provider-detail".to_string());
        }
        if self.tui_preview_provider_detail_ansi {
            overrides.push("--tui-preview-provider-detail-ansi".to_string());
        }
        if self.tui_preview_model_selector {
            overrides.push("--tui-preview-model-selector".to_string());
        }
        if self.tui_preview_model_selector_ansi {
            overrides.push("--tui-preview-model-selector-ansi".to_string());
        }
        if self.tui_preview_approval_hunks {
            overrides.push("--tui-preview-approval-hunks".to_string());
        }
        if self.tui_preview_approval_hunks_ansi {
            overrides.push("--tui-preview-approval-hunks-ansi".to_string());
        }
        if self.tui_preview_lane_selector {
            overrides.push("--tui-preview-lane-selector".to_string());
        }
        if self.tui_preview_lane_selector_ansi {
            overrides.push("--tui-preview-lane-selector-ansi".to_string());
        }
        if self.tui_preview_lane {
            overrides.push("--tui-preview-lane".to_string());
        }
        if self.tui_preview_lane_ansi {
            overrides.push("--tui-preview-lane-ansi".to_string());
        }
        if self.tui_preview_side {
            overrides.push("--tui-preview-side".to_string());
        }
        if self.tui_preview_side_ansi {
            overrides.push("--tui-preview-side-ansi".to_string());
        }
        if self.tui_preview_side_2 {
            overrides.push("--tui-preview-side-2".to_string());
        }
        if self.tui_preview_side_2_ansi {
            overrides.push("--tui-preview-side-2-ansi".to_string());
        }
        if self.tui_theme.is_some() {
            overrides.push("--tui-theme".to_string());
        }
        if self.tui_startup_check {
            overrides.push("--tui-startup-check".to_string());
        }
        overrides
    }
}

fn parse_startup_options(args: &[String]) -> Result<StartupOptions, String> {
    let mut options = StartupOptions::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--provider" => {
                index += 1;
                options.provider = Some(required_flag_value(args, index, "--provider")?);
            }
            "--model" => {
                index += 1;
                options.model = Some(required_flag_value(args, index, "--model")?);
            }
            "--api-base" => {
                index += 1;
                options.api_base = Some(required_flag_value(args, index, "--api-base")?);
            }
            "--api-key" => {
                index += 1;
                options.api_key = Some(required_flag_value(args, index, "--api-key")?);
            }
            "--provider-plugin-dir" => {
                index += 1;
                options
                    .provider_plugin_dirs
                    .push(PathBuf::from(required_flag_value(
                        args,
                        index,
                        "--provider-plugin-dir",
                    )?));
            }
            "--permissions" => {
                index += 1;
                let value = required_flag_value(args, index, "--permissions")?;
                options.permission_mode = Some(
                    viden_types::PermissionMode::parse_cli(&value)
                        .ok_or_else(|| format!("Unknown permission level `{value}`"))?,
                );
            }
            "--session-home" => {
                index += 1;
                options.session_home = Some(PathBuf::from(required_flag_value(
                    args,
                    index,
                    "--session-home",
                )?));
            }
            "--request-timeout" => {
                index += 1;
                let value = required_flag_value(args, index, "--request-timeout")?;
                options.request_timeout_secs = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| "--request-timeout must be an integer".to_string())?,
                );
            }
            "--max-retries" => {
                index += 1;
                let value = required_flag_value(args, index, "--max-retries")?;
                options.max_retries = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| "--max-retries must be an integer".to_string())?,
                );
            }
            "--config" => {
                index += 1;
                options.config_path =
                    Some(PathBuf::from(required_flag_value(args, index, "--config")?));
            }
            "--resume" => {
                let next = args.get(index + 1);
                if matches!(next, Some(value) if !value.starts_with("--")) {
                    index += 1;
                    options.resume_selector = next.cloned();
                } else {
                    options.resume_selector = Some("latest".to_string());
                }
            }
            "--tui" => {
                options.tui = true;
            }
            "--no-tui" => {
                options.no_tui = true;
            }
            "--tui-screen" => {
                index += 1;
                let value = required_flag_value(args, index, "--tui-screen")?;
                if !matches!(value.as_str(), "main" | "side-1" | "side-2") {
                    return Err("--tui-screen must be `main`, `side-1`, or `side-2`".to_string());
                }
                options.tui_screen = Some(value);
            }
            "--tui-startup-check" => {
                options.tui_startup_check = true;
                options.tui = true;
            }
            "--tui-preview" => {
                options.tui_preview = true;
            }
            "--tui-preview-ansi" => {
                options.tui_preview_ansi = true;
            }
            "--tui-preview-idle" => {
                options.tui_preview_idle = true;
            }
            "--tui-preview-idle-ansi" => {
                options.tui_preview_idle_ansi = true;
            }
            "--tui-preview-live-turn" => {
                options.tui_preview_live_turn = true;
            }
            "--tui-preview-live-turn-ansi" => {
                options.tui_preview_live_turn_ansi = true;
            }
            "--tui-preview-resize" => {
                options.tui_preview_resize = true;
            }
            "--tui-preview-resize-ansi" => {
                options.tui_preview_resize_ansi = true;
            }
            "--tui-preview-cjk-input" => {
                options.tui_preview_cjk_input = true;
            }
            "--tui-preview-cjk-input-ansi" => {
                options.tui_preview_cjk_input_ansi = true;
            }
            "--tui-preview-command-palette" => {
                options.tui_preview_command_palette = true;
            }
            "--tui-preview-command-palette-ansi" => {
                options.tui_preview_command_palette_ansi = true;
            }
            "--tui-preview-setup-wizard" => {
                options.tui_preview_setup_wizard = true;
            }
            "--tui-preview-setup-wizard-ansi" => {
                options.tui_preview_setup_wizard_ansi = true;
            }
            "--tui-preview-provider-selector" => {
                options.tui_preview_provider_selector = true;
            }
            "--tui-preview-provider-selector-ansi" => {
                options.tui_preview_provider_selector_ansi = true;
            }
            "--tui-preview-provider-detail" => {
                options.tui_preview_provider_detail = true;
            }
            "--tui-preview-provider-detail-ansi" => {
                options.tui_preview_provider_detail_ansi = true;
            }
            "--tui-preview-model-selector" => {
                options.tui_preview_model_selector = true;
            }
            "--tui-preview-model-selector-ansi" => {
                options.tui_preview_model_selector_ansi = true;
            }
            "--tui-preview-approval-hunks" => {
                options.tui_preview_approval_hunks = true;
            }
            "--tui-preview-approval-hunks-ansi" => {
                options.tui_preview_approval_hunks_ansi = true;
            }
            "--tui-preview-lane-selector" => {
                options.tui_preview_lane_selector = true;
            }
            "--tui-preview-lane-selector-ansi" => {
                options.tui_preview_lane_selector_ansi = true;
            }
            "--tui-preview-lane" => {
                options.tui_preview_lane = true;
            }
            "--tui-preview-lane-ansi" => {
                options.tui_preview_lane_ansi = true;
            }
            "--tui-preview-side" => {
                options.tui_preview_side = true;
            }
            "--tui-preview-side-ansi" => {
                options.tui_preview_side_ansi = true;
            }
            "--tui-preview-side-2" => {
                options.tui_preview_side_2 = true;
            }
            "--tui-preview-side-2-ansi" => {
                options.tui_preview_side_2_ansi = true;
            }
            "--tui-theme" => {
                index += 1;
                let value = required_flag_value(args, index, "--tui-theme")?;
                if !tui::is_known_theme(&value) {
                    return Err(format!(
                        "--tui-theme must be one of: {}",
                        tui::theme_names().join(", ")
                    ));
                }
                options.tui_theme = Some(value);
            }
            unknown if unknown.starts_with("--") => {
                return Err(format!("Unknown startup flag `{unknown}`"));
            }
            _ => {}
        }
        index += 1;
    }
    if options.no_tui && options.tui {
        return Err("--no-tui cannot be combined with --tui".to_string());
    }
    if options.no_tui && options.tui_screen.is_some() {
        return Err("--no-tui cannot be combined with --tui-screen".to_string());
    }
    Ok(options)
}

fn required_flag_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn print_startup_help() {
    println!("Viden startup flags:");
    println!("  --version, -V       Print the Viden terminal version");
    println!("  --provider <name>    Choose provider family");
    println!("  --model <name>       Override model name");
    println!("  --api-base <url>     Override provider base URL");
    println!("  --api-key <value>    Override API key");
    println!("  --provider-plugin-dir <dir>");
    println!("                       Add a dynamic provider plugin directory");
    println!("  --permissions <level>");
    println!("                       Set default permission level");
    println!("  --session-home <dir> Override transcript/index home");
    println!("  --request-timeout <s> Override provider HTTP timeout");
    println!("  --max-retries <n>    Override provider retry count");
    println!("  --config <path>      Load config from an explicit TOML file");
    println!("  --resume [id|latest] Resume a prior session");
    println!("  --tui                Start the cockpit terminal UI (default)");
    println!("  --tui-startup-check  Verify Core/TUI startup without entering raw mode");
    println!("  --no-tui             Start the legacy line REPL");
    println!("  --tui-screen <main|side-1|side-2>");
    println!("                       Start a specific TUI screen surface");
    println!(
        "  --tui-theme <name>   Select TUI theme: {}",
        tui::theme_names().join(", ")
    );
    println!("  --tui-preview        Print a non-interactive 140x40 TUI preview");
    println!("  --tui-preview-ansi   Print a themed ANSI 140x40 TUI preview");
    println!("  --tui-preview-idle   Print a 140x40 TUI preview without modal overlay");
    println!("  --tui-preview-idle-ansi");
    println!("                       Print a themed ANSI TUI preview without modal overlay");
    println!("  --tui-preview-live-turn");
    println!("                       Print a 140x40 TUI preview with a live provider turn");
    println!("  --tui-preview-live-turn-ansi");
    println!("                       Print a themed ANSI live provider turn preview");
    println!("  --tui-preview-resize");
    println!("                       Print a 100x30 resize-redraw TUI preview");
    println!("  --tui-preview-resize-ansi");
    println!("                       Print a themed ANSI resize-redraw preview");
    println!("  --tui-preview-cjk-input");
    println!("                       Print a 100x30 CJK input and cursor-placement preview");
    println!("  --tui-preview-cjk-input-ansi");
    println!("                       Print a themed ANSI CJK input preview");
    println!("  --tui-preview-command-palette");
    println!("                       Print a 140x40 TUI preview with slash command palette");
    println!("  --tui-preview-command-palette-ansi");
    println!("                       Print a themed ANSI slash command palette preview");
    println!("  --tui-preview-setup-wizard");
    println!("                       Print a first-run setup wizard preview");
    println!("  --tui-preview-setup-wizard-ansi");
    println!("                       Print a themed first-run setup wizard preview");
    println!("  --tui-preview-provider-selector");
    println!("                       Print a provider configuration selector preview");
    println!("  --tui-preview-provider-selector-ansi");
    println!("                       Print a themed provider configuration selector preview");
    println!("  --tui-preview-provider-detail");
    println!("                       Print a provider detail configuration preview");
    println!("  --tui-preview-provider-detail-ansi");
    println!("                       Print a themed provider detail configuration preview");
    println!("  --tui-preview-model-selector");
    println!("                       Print a grouped model selector preview");
    println!("  --tui-preview-model-selector-ansi");
    println!("                       Print a themed grouped model selector preview");
    println!("  --tui-preview-approval-hunks");
    println!("                       Print an approval preview with Core decision-context hunks");
    println!("  --tui-preview-approval-hunks-ansi");
    println!("                       Print a themed approval decision-context hunk preview");
    println!("  --tui-preview-lane-selector");
    println!("                       Print a lane action selector preview");
    println!("  --tui-preview-lane-selector-ansi");
    println!("                       Print a themed lane action selector preview");
    println!("  --tui-preview-lane   Print a 140x40 focused lane-detail preview");
    println!("  --tui-preview-lane-ansi");
    println!("                       Print a themed ANSI focused lane-detail preview");
    println!("  --tui-preview-side   Print a non-interactive 80x40 side-screen preview");
    println!("  --tui-preview-side-ansi");
    println!("                       Print a themed ANSI 80x40 side-screen preview");
    println!("  --tui-preview-side-2 Print a non-interactive 80x40 ops-screen preview");
    println!("  --tui-preview-side-2-ansi");
    println!("                       Print a themed ANSI 80x40 ops-screen preview");
    println!();
    println!(
        "Supported providers: {}",
        list_supported_provider_strings().join(", ")
    );
    println!();
    println!("Environment variables:");
    println!("  VIDEN_PROVIDER, VIDEN_MODEL, VIDEN_API_BASE, VIDEN_API_KEY");
    println!("  VIDEN_PROVIDER_PLUGIN_DIRS");
    println!("  VIDEN_PERMISSION_MODE, VIDEN_SESSION_HOME");
    println!("  VIDEN_REQUEST_TIMEOUT_SECS, VIDEN_MAX_RETRIES, VIDEN_CONFIG");
    println!("  VIDEN_SCREEN_LAUNCH_TEMPLATE, VIDEN_LANE_ATTACH_TEMPLATE");
    println!("  VIDEN_LANE_CODEX_TEMPLATE, VIDEN_LANE_CLAUDE_TEMPLATE");
    println!("  ANTHROPIC_API_KEY, OPENAI_API_KEY, DEEPSEEK_API_KEY, DEEPSEEK_API_BASE");
}

fn prompt_for_approval(prompt: PermissionPrompt, stdin: &mut impl BufRead) -> ApprovalResponse {
    println!();
    println!("Permission request for `{}`", prompt.tool_name);
    println!("{}", prompt.message);
    println!("{}", prompt.input_preview);
    print!("Allow? [y/N]: ");
    io::stdout().flush().ok();
    let Ok(Some(response)) = read_lossy_line(stdin) else {
        return ApprovalResponse::deny(None);
    };
    let approved = matches!(response.trim(), "y" | "Y" | "yes" | "YES");
    if approved {
        ApprovalResponse::allow_once(None)
    } else {
        ApprovalResponse::deny(None)
    }
}

fn read_lossy_line(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut bytes = Vec::new();
    let read = reader.read_until(b'\n', &mut bytes)?;
    if read == 0 {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

fn render_event(event: EngineEvent) {
    match event {
        EngineEvent::System(text) => println!("[system] {text}"),
        EngineEvent::Assistant(text) => println!("[assistant]\n{text}"),
        EngineEvent::ToolCall(text) => println!("[tool-call] {text}"),
        EngineEvent::ToolResult { output, .. } => println!("[tool-result]\n{output}"),
        EngineEvent::Command(text) => println!("{text}"),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use viden_provider::{ProviderPluginError, ProviderPluginErrorKind};

    #[test]
    fn read_lossy_line_replaces_invalid_utf8() {
        let mut input = Cursor::new(vec![b'o', b'k', 0xff, b'\n']);
        let line = read_lossy_line(&mut input).unwrap().unwrap();
        assert_eq!(line, "ok\u{fffd}\n");
    }

    #[test]
    fn read_lossy_line_returns_none_on_eof() {
        let mut input = Cursor::new(Vec::<u8>::new());
        assert_eq!(read_lossy_line(&mut input).unwrap(), None);
    }

    #[test]
    fn parse_startup_options_collects_provider_plugin_dirs() {
        let args = vec![
            "--provider-plugin-dir".to_string(),
            "plugins-a".to_string(),
            "--provider-plugin-dir".to_string(),
            "plugins-b".to_string(),
        ];

        let options = parse_startup_options(&args).unwrap();

        assert_eq!(
            options.provider_plugin_dirs,
            vec![PathBuf::from("plugins-a"), PathBuf::from("plugins-b")]
        );
        assert_eq!(
            options.summary_overrides(),
            vec!["--provider-plugin-dir".to_string()]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_flag() {
        let args = vec!["--tui".to_string()];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui);
        assert!(options.should_start_tui());
        assert_eq!(options.summary_overrides(), vec!["--tui".to_string()]);
    }

    #[test]
    fn parse_startup_options_defaults_to_tui() {
        let args = Vec::<String>::new();

        let options = parse_startup_options(&args).unwrap();

        assert!(options.should_start_tui());
        assert!(options.summary_overrides().is_empty());
    }

    #[test]
    fn parse_startup_options_accepts_no_tui_escape_hatch() {
        let args = vec!["--no-tui".to_string()];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.no_tui);
        assert!(!options.should_start_tui());
        assert_eq!(options.summary_overrides(), vec!["--no-tui".to_string()]);
    }

    #[test]
    fn parse_startup_options_rejects_conflicting_tui_flags() {
        let tui_err = parse_startup_options(&["--no-tui".to_string(), "--tui".to_string()])
            .expect_err("--no-tui plus --tui should fail");
        assert!(tui_err.contains("--no-tui cannot be combined with --tui"));

        let screen_err = parse_startup_options(&[
            "--no-tui".to_string(),
            "--tui-screen".to_string(),
            "side-1".to_string(),
        ])
        .expect_err("--no-tui plus --tui-screen should fail");
        assert!(screen_err.contains("--no-tui cannot be combined with --tui-screen"));
    }

    #[test]
    fn parse_startup_options_accepts_tui_screen_side_flag() {
        let args = vec!["--tui-screen".to_string(), "side-1".to_string()];

        let options = parse_startup_options(&args).unwrap();

        assert_eq!(options.tui_screen.as_deref(), Some("side-1"));
        assert!(options.should_start_tui());
        assert_eq!(
            options.summary_overrides(),
            vec!["--tui-screen".to_string()]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_screen_side_2_flag() {
        let args = vec!["--tui-screen".to_string(), "side-2".to_string()];

        let options = parse_startup_options(&args).unwrap();

        assert_eq!(options.tui_screen.as_deref(), Some("side-2"));
        assert_eq!(
            options.summary_overrides(),
            vec!["--tui-screen".to_string()]
        );
    }

    #[test]
    fn parse_startup_options_rejects_unknown_tui_screen() {
        let args = vec!["--tui-screen".to_string(), "side-3".to_string()];

        let err = match parse_startup_options(&args) {
            Ok(_) => panic!("unknown TUI screen should fail"),
            Err(err) => err,
        };

        assert!(err.contains("main`, `side-1`, or `side-2"));
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_flag() {
        let args = vec!["--tui-preview".to_string()];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview);
        assert_eq!(
            options.summary_overrides(),
            vec!["--tui-preview".to_string()]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_ansi_flag() {
        let args = vec!["--tui-preview-ansi".to_string()];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview_ansi);
        assert_eq!(
            options.summary_overrides(),
            vec!["--tui-preview-ansi".to_string()]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_idle_flag() {
        let args = vec!["--tui-preview-idle".to_string()];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview_idle);
        assert_eq!(
            options.summary_overrides(),
            vec!["--tui-preview-idle".to_string()]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_idle_ansi_flag() {
        let args = vec!["--tui-preview-idle-ansi".to_string()];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview_idle_ansi);
        assert_eq!(
            options.summary_overrides(),
            vec!["--tui-preview-idle-ansi".to_string()]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_live_turn_flags() {
        let args = vec![
            "--tui-preview-live-turn".to_string(),
            "--tui-preview-live-turn-ansi".to_string(),
        ];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview_live_turn);
        assert!(options.tui_preview_live_turn_ansi);
        assert_eq!(
            options.summary_overrides(),
            vec![
                "--tui-preview-live-turn".to_string(),
                "--tui-preview-live-turn-ansi".to_string()
            ]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_reliability_flags() {
        let args = vec![
            "--tui-preview-resize".to_string(),
            "--tui-preview-resize-ansi".to_string(),
            "--tui-preview-cjk-input".to_string(),
            "--tui-preview-cjk-input-ansi".to_string(),
        ];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview_resize);
        assert!(options.tui_preview_resize_ansi);
        assert!(options.tui_preview_cjk_input);
        assert!(options.tui_preview_cjk_input_ansi);
        assert_eq!(
            options.summary_overrides(),
            vec![
                "--tui-preview-resize".to_string(),
                "--tui-preview-resize-ansi".to_string(),
                "--tui-preview-cjk-input".to_string(),
                "--tui-preview-cjk-input-ansi".to_string()
            ]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_command_palette_flag() {
        let args = vec!["--tui-preview-command-palette".to_string()];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview_command_palette);
        assert_eq!(
            options.summary_overrides(),
            vec!["--tui-preview-command-palette".to_string()]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_command_palette_ansi_flag() {
        let args = vec!["--tui-preview-command-palette-ansi".to_string()];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview_command_palette_ansi);
        assert_eq!(
            options.summary_overrides(),
            vec!["--tui-preview-command-palette-ansi".to_string()]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_setup_wizard_flags() {
        let args = vec![
            "--tui-preview-setup-wizard".to_string(),
            "--tui-preview-setup-wizard-ansi".to_string(),
        ];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview_setup_wizard);
        assert!(options.tui_preview_setup_wizard_ansi);
        assert_eq!(
            options.summary_overrides(),
            vec![
                "--tui-preview-setup-wizard".to_string(),
                "--tui-preview-setup-wizard-ansi".to_string()
            ]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_provider_selector_flags() {
        let args = vec![
            "--tui-preview-provider-selector".to_string(),
            "--tui-preview-provider-selector-ansi".to_string(),
        ];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview_provider_selector);
        assert!(options.tui_preview_provider_selector_ansi);
        assert_eq!(
            options.summary_overrides(),
            vec![
                "--tui-preview-provider-selector".to_string(),
                "--tui-preview-provider-selector-ansi".to_string()
            ]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_provider_detail_flags() {
        let args = vec![
            "--tui-preview-provider-detail".to_string(),
            "--tui-preview-provider-detail-ansi".to_string(),
        ];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview_provider_detail);
        assert!(options.tui_preview_provider_detail_ansi);
        assert_eq!(
            options.summary_overrides(),
            vec![
                "--tui-preview-provider-detail".to_string(),
                "--tui-preview-provider-detail-ansi".to_string()
            ]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_model_selector_flags() {
        let args = vec![
            "--tui-preview-model-selector".to_string(),
            "--tui-preview-model-selector-ansi".to_string(),
        ];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview_model_selector);
        assert!(options.tui_preview_model_selector_ansi);
        assert_eq!(
            options.summary_overrides(),
            vec![
                "--tui-preview-model-selector".to_string(),
                "--tui-preview-model-selector-ansi".to_string()
            ]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_lane_selector_flags() {
        let args = vec![
            "--tui-preview-lane-selector".to_string(),
            "--tui-preview-lane-selector-ansi".to_string(),
        ];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview_lane_selector);
        assert!(options.tui_preview_lane_selector_ansi);
        assert_eq!(
            options.summary_overrides(),
            vec![
                "--tui-preview-lane-selector".to_string(),
                "--tui-preview-lane-selector-ansi".to_string()
            ]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_lane_flag() {
        let args = vec!["--tui-preview-lane".to_string()];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview_lane);
        assert_eq!(
            options.summary_overrides(),
            vec!["--tui-preview-lane".to_string()]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_lane_ansi_flag() {
        let args = vec!["--tui-preview-lane-ansi".to_string()];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview_lane_ansi);
        assert_eq!(
            options.summary_overrides(),
            vec!["--tui-preview-lane-ansi".to_string()]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_side_flag() {
        let args = vec!["--tui-preview-side".to_string()];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview_side);
        assert_eq!(
            options.summary_overrides(),
            vec!["--tui-preview-side".to_string()]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_side_ansi_flag() {
        let args = vec!["--tui-preview-side-ansi".to_string()];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview_side_ansi);
        assert_eq!(
            options.summary_overrides(),
            vec!["--tui-preview-side-ansi".to_string()]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_side_2_flag() {
        let args = vec!["--tui-preview-side-2".to_string()];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview_side_2);
        assert_eq!(
            options.summary_overrides(),
            vec!["--tui-preview-side-2".to_string()]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_preview_side_2_ansi_flag() {
        let args = vec!["--tui-preview-side-2-ansi".to_string()];

        let options = parse_startup_options(&args).unwrap();

        assert!(options.tui_preview_side_2_ansi);
        assert_eq!(
            options.summary_overrides(),
            vec!["--tui-preview-side-2-ansi".to_string()]
        );
    }

    #[test]
    fn parse_startup_options_accepts_tui_theme_flag() {
        let args = vec!["--tui-theme".to_string(), "amber".to_string()];

        let options = parse_startup_options(&args).unwrap();

        assert_eq!(options.tui_theme.as_deref(), Some("amber"));
        assert_eq!(options.summary_overrides(), vec!["--tui-theme".to_string()]);
    }

    #[test]
    fn tui_skin_override_preserves_other_core_ui_preferences() {
        let base = viden_types::ResolvedUiPreferences {
            locale: viden_types::LocaleId::ZhCn,
            skin: viden_types::UiSkin::Aurora,
            mode: viden_types::UiColorMode::Light,
            density: viden_types::UiDensity::Comfy,
            motion: viden_types::UiMotion::Reduced,
            diagnostics: Vec::new(),
        };

        let resolved = ui_preferences_with_skin(&base, "ice");

        assert_eq!(resolved.locale, viden_types::LocaleId::ZhCn);
        assert_eq!(resolved.skin, viden_types::UiSkin::Ice);
        assert_eq!(resolved.mode, viden_types::UiColorMode::Light);
        assert_eq!(resolved.density, viden_types::UiDensity::Comfy);
        assert_eq!(resolved.motion, viden_types::UiMotion::Reduced);
    }

    #[test]
    fn parse_startup_options_rejects_unknown_tui_theme() {
        let args = vec!["--tui-theme".to_string(), "unknown".to_string()];

        let err = match parse_startup_options(&args) {
            Ok(_) => panic!("unknown TUI theme should fail"),
            Err(err) => err,
        };

        assert!(err.contains("aurora"), "{err}");
        assert!(err.contains("phosphor"), "{err}");
    }

    #[test]
    fn parse_startup_options_accepts_core_client_startup_check() {
        let args = vec!["--tui-startup-check".to_string()];

        let options = parse_startup_options(&args).expect("startup check flag");

        assert!(options.tui_startup_check);
        assert!(options.should_start_tui());
    }

    #[test]
    fn provider_plugin_error_format_includes_structured_diagnostics() {
        let err = ProviderPluginError {
            kind: ProviderPluginErrorKind::LoadLibrary,
            path: PathBuf::from("/tmp/broken-provider.dylib"),
            message: "not a dynamic library".to_string(),
        };

        let formatted = format_provider_plugin_error(err);

        assert!(formatted.contains("kind: LoadLibrary"), "{formatted}");
        assert!(
            formatted.contains("path: /tmp/broken-provider.dylib"),
            "{formatted}"
        );
        assert!(
            formatted.contains("message: not a dynamic library"),
            "{formatted}"
        );
    }
}
