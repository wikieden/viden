use std::path::{Path, PathBuf};

use viden_config::{
    CliOverrides, ResolvedConfig, load_config, system_ui_preferences, user_ui_config_path,
};
use viden_provider::{
    ModelProvider, ProviderConfig, ProviderHost, ProviderPluginError, ProviderRegistry,
};
use viden_session::{LoadedTranscript, SessionStore};
use viden_types::{PermissionLevel, RuntimeSnapshot, SessionSummary, UiPreferences, WorkMode};

use crate::{RuntimeResumeError, RuntimeResumeRequest, SessionEngine};

#[derive(Clone)]
pub struct RuntimeBootstrapRequest {
    pub cwd: PathBuf,
    pub cli_overrides: CliOverrides,
    pub startup_overrides: Vec<String>,
    pub resume: Option<RuntimeResumeRequest>,
}

impl RuntimeBootstrapRequest {
    pub fn new(cwd: impl Into<PathBuf>, cli_overrides: CliOverrides) -> Self {
        Self {
            cwd: cwd.into(),
            cli_overrides,
            startup_overrides: Vec::new(),
            resume: None,
        }
    }

    pub fn with_startup_overrides(mut self, startup_overrides: Vec<String>) -> Self {
        self.startup_overrides = startup_overrides;
        self
    }

    pub fn with_resume(mut self, resume: RuntimeResumeRequest) -> Self {
        self.resume = Some(resume);
        self
    }
}

pub struct RuntimeBootstrap {
    pub engine: SessionEngine,
    pub provider_summary: String,
    pub resolved_config: ResolvedConfig,
}

/// Builds the production runtime stack for every local frontend entrypoint.
///
/// Frontends select a workspace and pass CLI/config overrides here; provider
/// loading, config resolution, session storage, permission state, and runtime
/// snapshot construction stay on the shared Core/runtime path.
pub fn bootstrap_runtime(request: RuntimeBootstrapRequest) -> Result<RuntimeBootstrap, String> {
    let ui_cli_override = request.cli_overrides.ui;
    let ui_config_path =
        user_ui_config_path(&request.cwd, request.cli_overrides.config_path.as_deref())?;
    let ui_system_context = system_ui_preferences();
    let resolved_config = load_config(&request.cwd, &request.cli_overrides)?;
    bootstrap_runtime_with_context(
        &request.cwd,
        resolved_config,
        request.startup_overrides,
        request.resume,
        ui_cli_override,
        ui_config_path,
        ui_system_context,
    )
}

pub fn bootstrap_runtime_with_resolved_config(
    cwd: &Path,
    resolved_config: ResolvedConfig,
    startup_overrides: Vec<String>,
) -> Result<RuntimeBootstrap, String> {
    bootstrap_runtime_with_resolved_config_and_resume(cwd, resolved_config, startup_overrides, None)
}

pub fn bootstrap_runtime_with_resolved_config_and_resume(
    cwd: &Path,
    resolved_config: ResolvedConfig,
    startup_overrides: Vec<String>,
    resume: Option<RuntimeResumeRequest>,
) -> Result<RuntimeBootstrap, String> {
    bootstrap_runtime_with_context(
        cwd,
        resolved_config,
        startup_overrides,
        resume,
        None,
        user_ui_config_path(cwd, None)?,
        system_ui_preferences(),
    )
}

fn bootstrap_runtime_with_context(
    cwd: &Path,
    resolved_config: ResolvedConfig,
    startup_overrides: Vec<String>,
    resume: Option<RuntimeResumeRequest>,
    ui_cli_override: Option<UiPreferences>,
    ui_config_path: PathBuf,
    ui_system_context: UiPreferences,
) -> Result<RuntimeBootstrap, String> {
    let resolved_resume = match resume {
        Some(request) => Some(resolve_bootstrap_resume(cwd, &resolved_config, request)?),
        None => None,
    };
    let provider_host = load_startup_provider_host(&resolved_config)?;
    let provider_selection = create_startup_provider(&provider_host, &resolved_config)?;
    let provider_summary = format!(
        "{} | config={} | files={}",
        provider_selection.summary,
        resolved_config.summary(),
        if resolved_config.loaded_files.is_empty() {
            "<none>".to_string()
        } else {
            resolved_config
                .loaded_files
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        }
    );
    let runtime_snapshot = RuntimeSnapshot {
        cwd: cwd.to_path_buf(),
        provider_family: resolved_config.provider.clone(),
        model_label: provider_selection.model_label.clone(),
        work_mode: if resolved_config.permission_mode == viden_types::PermissionMode::Plan {
            WorkMode::Plan
        } else {
            WorkMode::Build
        },
        permission_mode: resolved_config.permission_mode,
        permission_level: PermissionLevel::from_legacy_mode(resolved_config.permission_mode),
        config_summary: resolved_config.summary(),
        loaded_config_files: resolved_config.loaded_files.clone(),
        startup_overrides,
        ui_preferences: resolved_config.ui.clone(),
    };
    let resume_session_id = resolved_resume
        .as_ref()
        .map(|(summary, _)| summary.session_id.clone());
    let mut engine = SessionEngine::new_with_home_session_and_snapshot(
        cwd,
        provider_selection.provider,
        resolved_config.session_home.clone(),
        resume_session_id,
        runtime_snapshot,
    )?;
    engine.set_ui_preference_context(ui_cli_override, Some(ui_config_path), ui_system_context);
    // Read once, here: the snapshot prefix republishes this record for every
    // command and must not re-read the operator's config file each time.
    engine.load_ui_layout_preferences();
    if let Some((summary, loaded)) = resolved_resume {
        engine
            .activate_resolved_session(summary, loaded)
            .map_err(|err| err.to_string())?;
    }
    engine.set_provider_runtime(
        provider_host,
        resolved_config.provider_plugin_dirs.clone(),
        resolved_config.api_base.clone(),
        resolved_config.api_key.clone(),
        resolved_config.request_timeout_secs,
        resolved_config.max_retries,
    );
    engine.set_permission_mode(resolved_config.permission_mode)?;

    Ok(RuntimeBootstrap {
        engine,
        provider_summary,
        resolved_config,
    })
}

fn resolve_bootstrap_resume(
    cwd: &Path,
    resolved_config: &ResolvedConfig,
    request: RuntimeResumeRequest,
) -> Result<(SessionSummary, LoadedTranscript), String> {
    let Some(store) = (match &resolved_config.session_home {
        Some(home) => SessionStore::open_existing_for_query(home, cwd)?,
        None => SessionStore::open_default_existing_for_query(cwd)?,
    }) else {
        return Err(RuntimeResumeError::NotFound {
            selector: resume_selector(&request),
        }
        .to_string());
    };
    match request {
        RuntimeResumeRequest::ExactSessionId(session_id) => {
            store.load_by_id_for_cwd(&session_id)?.ok_or_else(|| {
                RuntimeResumeError::NotFound {
                    selector: session_id,
                }
                .to_string()
            })
        }
        RuntimeResumeRequest::Latest => store.load_latest_for_cwd()?.ok_or_else(|| {
            RuntimeResumeError::NotFound {
                selector: "latest".to_string(),
            }
            .to_string()
        }),
        RuntimeResumeRequest::Selector(selector) => resolve_bootstrap_selector(&store, &selector),
    }
}

fn resume_selector(request: &RuntimeResumeRequest) -> String {
    match request {
        RuntimeResumeRequest::ExactSessionId(session_id) => session_id.clone(),
        RuntimeResumeRequest::Selector(selector) => selector.clone(),
        RuntimeResumeRequest::Latest => "latest".to_string(),
    }
}

fn resolve_bootstrap_selector(
    store: &SessionStore,
    selector: &str,
) -> Result<(SessionSummary, LoadedTranscript), String> {
    let sessions = store.list_sessions_for_cwd()?;
    if sessions.is_empty() {
        return Err(RuntimeResumeError::NotFound {
            selector: selector.to_string(),
        }
        .to_string());
    }
    if let Some(loaded) = store.load_by_id_for_cwd(selector)? {
        return Ok(loaded);
    }
    let matches = sessions
        .iter()
        .filter(|summary| {
            summary.session_id.starts_with(selector)
                || summary
                    .session_id
                    .trim_start_matches("session_")
                    .starts_with(selector)
        })
        .cloned()
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [summary] => {
            let loaded = SessionStore::load_transcript_from_path(std::path::Path::new(
                &summary.transcript_path,
            ))?;
            Ok((summary.clone(), loaded))
        }
        [] => Err(RuntimeResumeError::NotFound {
            selector: selector.to_string(),
        }
        .to_string()),
        _ => Err(RuntimeResumeError::Ambiguous {
            selector: selector.to_string(),
            sessions: render_bootstrap_session_ids(&matches),
        }
        .to_string()),
    }
}

fn render_bootstrap_session_ids(sessions: &[SessionSummary]) -> String {
    let ids = sessions
        .iter()
        .map(|summary| summary.session_id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!("matching sessions: {ids}")
}

fn load_startup_provider_host(
    resolved_config: &viden_config::ResolvedConfig,
) -> Result<ProviderHost, String> {
    if resolved_config.provider_plugin_dirs.is_empty() {
        ProviderHost::load_default_diagnostic().map_err(format_provider_plugin_error)
    } else {
        ProviderHost::load_from_dirs_diagnostic(resolved_config.provider_plugin_dirs.clone())
            .map_err(format_provider_plugin_error)
    }
}

fn format_provider_plugin_error(err: ProviderPluginError) -> String {
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

struct StartupProviderSelection {
    provider: Box<dyn ModelProvider>,
    model_label: String,
    summary: String,
}

fn create_startup_provider(
    host: &ProviderHost,
    resolved_config: &viden_config::ResolvedConfig,
) -> Result<StartupProviderSelection, String> {
    match ProviderConfig::from_settings(
        &resolved_config.provider,
        resolved_config.model.as_deref(),
        resolved_config.api_base.as_deref(),
        resolved_config.api_key.as_deref(),
        resolved_config.request_timeout_secs,
        resolved_config.max_retries,
    ) {
        Ok(provider_config) => {
            let model_label = provider_config.model.clone();
            let summary = provider_config.summary();
            let provider = host.create(provider_config)?;
            Ok(StartupProviderSelection {
                provider,
                model_label,
                summary,
            })
        }
        Err(builtin_error) => create_dynamic_startup_provider(host, resolved_config)
            .map_err(|dynamic_error| format!("{builtin_error}; {dynamic_error}")),
    }
}

fn create_dynamic_startup_provider(
    host: &ProviderHost,
    resolved_config: &viden_config::ResolvedConfig,
) -> Result<StartupProviderSelection, String> {
    let registry = host.registry();
    let descriptor = registry
        .descriptor(&resolved_config.provider)
        .ok_or_else(|| format!("Provider `{}` is not registered", resolved_config.provider))?;
    let model_label = resolved_config
        .model
        .clone()
        .or_else(|| descriptor.default_model.clone())
        .ok_or_else(|| {
            format!(
                "Provider `{}` does not define a default model; pass --model",
                resolved_config.provider
            )
        })?;
    let provider = host.create_registered(
        &resolved_config.provider,
        resolved_config.model.as_deref(),
        resolved_config.api_base.as_deref(),
        resolved_config.api_key.as_deref(),
        resolved_config.request_timeout_secs,
        resolved_config.max_retries,
    )?;
    Ok(StartupProviderSelection {
        provider,
        model_label,
        summary: dynamic_provider_summary(&registry, resolved_config),
    })
}

fn dynamic_provider_summary(
    registry: &ProviderRegistry,
    resolved_config: &viden_config::ResolvedConfig,
) -> String {
    let descriptor = registry.descriptor(&resolved_config.provider);
    let model = resolved_config
        .model
        .as_deref()
        .or_else(|| descriptor.and_then(|descriptor| descriptor.default_model.as_deref()))
        .unwrap_or("<required>");
    let api_base = resolved_config
        .api_base
        .clone()
        .or_else(|| {
            descriptor
                .and_then(|descriptor| descriptor.env_mappings.api_base_env.as_deref())
                .and_then(|name| std::env::var(name).ok())
        })
        .or_else(|| descriptor.and_then(|descriptor| descriptor.default_api_base.clone()))
        .unwrap_or_else(|| "<required>".to_string());
    let key_present = resolved_config.api_key.is_some()
        || descriptor
            .and_then(|descriptor| descriptor.env_mappings.api_key_env.as_deref())
            .and_then(|name| std::env::var(name).ok())
            .is_some();
    format!(
        "provider={} model={} api_base={} key={} timeout={}s retries={}",
        resolved_config.provider,
        model,
        api_base,
        if key_present { "present" } else { "missing" },
        resolved_config.request_timeout_secs,
        resolved_config.max_retries,
    )
}
