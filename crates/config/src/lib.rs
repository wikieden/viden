use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use toml::{Value, map::Map};
use viden_types::{
    LocaleId, PermissionMode, ResolvedUiPreferences, UiColorMode, UiDensity, UiMotion,
    UiPreferenceDiagnostic, UiPreferences, UiSkin, resolve_ui_preferences,
};

mod project;
mod ui_layout;
mod ui_preferences;

pub use project::{
    EgressMode, EgressPolicy, OwnershipRule, ProjectFileConfig, ToolAllowlists,
    parse_project_config,
};
pub use ui_layout::{
    UiLayoutPreferenceFileState, reset_user_ui_layout_preferences_at,
    resolve_user_ui_layout_preferences_at, save_user_ui_layout_preferences_at,
};
pub use ui_preferences::{
    UiPreferenceFileState, preview_reset_user_ui_preferences_at, preview_user_ui_preferences_at,
    reset_user_ui_preferences_at, resolve_user_ui_preferences_at, save_user_ui_preferences_at,
};
#[cfg(test)]
pub(crate) use ui_preferences::{
    UiPreferenceWriteFailure, save_user_ui_preferences_at_with_failure,
};

#[derive(Debug, Clone, Default)]
pub struct CliOverrides {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub api_base: Option<String>,
    pub api_key: Option<String>,
    pub provider_plugin_dirs: Vec<PathBuf>,
    pub permission_mode: Option<PermissionMode>,
    pub session_home: Option<PathBuf>,
    pub request_timeout_secs: Option<u64>,
    pub max_retries: Option<u32>,
    pub config_path: Option<PathBuf>,
    pub ui: Option<UiPreferences>,
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub provider: String,
    pub model: Option<String>,
    pub api_base: Option<String>,
    pub api_key: Option<String>,
    pub provider_plugin_dirs: Vec<PathBuf>,
    pub permission_mode: PermissionMode,
    pub session_home: Option<PathBuf>,
    pub request_timeout_secs: u64,
    pub max_retries: u32,
    pub loaded_files: Vec<PathBuf>,
    pub ui: ResolvedUiPreferences,
    pub ui_diagnostics: Vec<UiPreferenceDiagnostic>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderConfigUpdate {
    pub api_base: Option<String>,
    pub api_key_env: Option<String>,
    pub default_model: Option<String>,
    pub models: Option<Vec<String>>,
    pub favorite_models: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderUiConfig {
    pub api_base: Option<String>,
    pub api_key_env: Option<String>,
    pub default_model: Option<String>,
    pub models: Vec<String>,
    pub favorite_models: Vec<String>,
}

impl Default for ResolvedConfig {
    fn default() -> Self {
        Self {
            provider: "deepseek".to_string(),
            model: None,
            api_base: None,
            api_key: None,
            provider_plugin_dirs: Vec::new(),
            permission_mode: PermissionMode::Default,
            session_home: None,
            request_timeout_secs: 90,
            max_retries: 1,
            loaded_files: Vec::new(),
            ui: resolve_ui_preferences(None, None, None, UiPreferences::client_default()),
            ui_diagnostics: Vec::new(),
        }
    }
}

impl ResolvedConfig {
    pub fn summary(&self) -> String {
        format!(
            "provider={} model={} plugin_dirs={} permission_mode={} session_home={} timeout={}s retries={}",
            self.provider,
            self.model.as_deref().unwrap_or("<default>"),
            format_path_list(&self.provider_plugin_dirs),
            self.permission_mode.cli_name(),
            self.session_home
                .as_deref()
                .map(Path::display)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "<default>".to_string()),
            self.request_timeout_secs,
            self.max_retries,
        )
    }
}

#[derive(Debug, Default, Deserialize, Clone)]
struct ProviderScopedFileConfig {
    api_base: Option<String>,
    api_key: Option<String>,
    api_key_env: Option<String>,
    default_model: Option<String>,
    models: Option<Vec<String>>,
    favorite_models: Option<Vec<String>>,
}

type ProvidersFileConfig = BTreeMap<String, ProviderScopedFileConfig>;

#[derive(Debug, Clone)]
struct UiPreferenceSource {
    profile: UiPreferences,
    diagnostic: Option<UiPreferenceDiagnostic>,
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    provider: Option<String>,
    model: Option<String>,
    api_base: Option<String>,
    api_key: Option<String>,
    api_key_env: Option<String>,
    provider_plugin_dirs: Option<Vec<String>>,
    permission_mode: Option<String>,
    session_home: Option<String>,
    request_timeout_secs: Option<u64>,
    max_retries: Option<u32>,
    providers: Option<ProvidersFileConfig>,
    #[serde(skip)]
    ui_raw: Option<Value>,
}

pub fn load_config(cwd: &Path, cli: &CliOverrides) -> Result<ResolvedConfig, String> {
    load_config_with_env(cwd, cli, &|key| std::env::var(key).ok())
}

pub fn default_user_config_path() -> Result<PathBuf, String> {
    default_config_path(&|key| std::env::var(key).ok()).ok_or_else(|| {
        "Cannot determine Viden config path; set VIDEN_CONFIG or HOME/APPDATA/XDG_CONFIG_HOME"
            .to_string()
    })
}

pub fn user_ui_config_path(
    cwd: &Path,
    config_path_override: Option<&Path>,
) -> Result<PathBuf, String> {
    user_ui_config_path_with_env(cwd, config_path_override, &|key| std::env::var(key).ok())
}

fn user_ui_config_path_with_env<F>(
    cwd: &Path,
    config_path_override: Option<&Path>,
    env_lookup: &F,
) -> Result<PathBuf, String>
where
    F: Fn(&str) -> Option<String>,
{
    let selected = config_path_override
        .map(Path::to_path_buf)
        .or_else(|| env_lookup("VIDEN_CONFIG").map(PathBuf::from));
    if let Some(path) = selected
        && !is_project_config_path(cwd, &path)
    {
        return Ok(path);
    }
    default_config_path(env_lookup).ok_or_else(|| {
        "Cannot determine Viden UI config path; set HOME/APPDATA/XDG_CONFIG_HOME".to_string()
    })
}

pub fn system_ui_preferences() -> UiPreferences {
    system_ui_preferences_with_env(&|key| std::env::var(key).ok())
}

pub fn save_user_provider_model_defaults(provider: &str, model: &str) -> Result<PathBuf, String> {
    let path = std::env::var("VIDEN_CONFIG")
        .ok()
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(default_user_config_path)?;
    save_user_provider_model_defaults_at(&path, provider, model)?;
    Ok(path)
}

pub fn save_user_provider_model_defaults_at(
    path: &Path,
    provider: &str,
    model: &str,
) -> Result<(), String> {
    let provider = provider.trim();
    let model = model.trim();
    if provider.is_empty() {
        return Err("Provider cannot be empty.".to_string());
    }
    if model.is_empty() {
        return Err("Model cannot be empty.".to_string());
    }

    let mut value = read_config_value_for_write(path)?;

    let table = value
        .as_table_mut()
        .ok_or_else(|| format!("Config {} must be a TOML table", path.display()))?;
    table.insert("provider".to_string(), Value::String(provider.to_string()));
    table.insert("model".to_string(), Value::String(model.to_string()));

    write_config_value(path, &value)
}

pub fn save_user_provider_config(
    provider: &str,
    update: ProviderConfigUpdate,
) -> Result<PathBuf, String> {
    let path = std::env::var("VIDEN_CONFIG")
        .ok()
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(default_user_config_path)?;
    save_user_provider_config_at(&path, provider, update)?;
    Ok(path)
}

pub fn save_user_provider_config_at(
    path: &Path,
    provider: &str,
    update: ProviderConfigUpdate,
) -> Result<(), String> {
    let provider = provider.trim();
    if provider.is_empty() {
        return Err("Provider cannot be empty.".to_string());
    }

    let api_base = trim_optional(update.api_base, "API base")?;
    let api_key_env = trim_optional(update.api_key_env, "API key env")?;
    let default_model = trim_optional(update.default_model, "Default model")?;
    let models = trim_optional_list(update.models, "Model")?;
    let favorite_models = trim_optional_list(update.favorite_models, "Favorite model")?;
    if api_base.is_none()
        && api_key_env.is_none()
        && default_model.is_none()
        && models.is_none()
        && favorite_models.is_none()
    {
        return Err("No provider config fields were provided.".to_string());
    }

    let mut value = read_config_value_for_write(path)?;
    let table = value
        .as_table_mut()
        .ok_or_else(|| format!("Config {} must be a TOML table", path.display()))?;
    let providers = table
        .entry("providers".to_string())
        .or_insert_with(|| Value::Table(Map::new()));
    let providers_table = providers.as_table_mut().ok_or_else(|| {
        format!(
            "Config {} field `providers` must be a table",
            path.display()
        )
    })?;
    let provider_value = providers_table
        .entry(provider.to_string())
        .or_insert_with(|| Value::Table(Map::new()));
    let provider_table = provider_value.as_table_mut().ok_or_else(|| {
        format!(
            "Config {} field `providers.{provider}` must be a table",
            path.display()
        )
    })?;

    if let Some(api_base) = api_base {
        provider_table.insert("api_base".to_string(), Value::String(api_base));
    }
    if let Some(api_key_env) = api_key_env {
        provider_table.insert("api_key_env".to_string(), Value::String(api_key_env));
    }
    if let Some(default_model) = default_model {
        provider_table.insert("default_model".to_string(), Value::String(default_model));
    }
    if let Some(models) = models {
        provider_table.insert(
            "models".to_string(),
            Value::Array(models.into_iter().map(Value::String).collect()),
        );
    }
    if let Some(favorite_models) = favorite_models {
        provider_table.insert(
            "favorite_models".to_string(),
            Value::Array(favorite_models.into_iter().map(Value::String).collect()),
        );
    }

    write_config_value(path, &value)
}

pub fn add_user_provider_model(provider: &str, model: &str) -> Result<PathBuf, String> {
    let path = std::env::var("VIDEN_CONFIG")
        .ok()
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(default_user_config_path)?;
    add_user_provider_model_at(&path, provider, model)?;
    Ok(path)
}

pub fn add_user_provider_model_at(path: &Path, provider: &str, model: &str) -> Result<(), String> {
    let mut current = load_provider_ui_config_at(path)?
        .remove(provider)
        .unwrap_or_default()
        .models;
    push_unique_string(&mut current, model.trim())?;
    save_user_provider_config_at(
        path,
        provider,
        ProviderConfigUpdate {
            models: Some(current),
            ..ProviderConfigUpdate::default()
        },
    )
}

pub fn add_user_provider_favorite_model(provider: &str, model: &str) -> Result<PathBuf, String> {
    let path = std::env::var("VIDEN_CONFIG")
        .ok()
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(default_user_config_path)?;
    add_user_provider_favorite_model_at(&path, provider, model)?;
    Ok(path)
}

pub fn add_user_provider_favorite_model_at(
    path: &Path,
    provider: &str,
    model: &str,
) -> Result<(), String> {
    let mut config = load_provider_ui_config_at(path)?
        .remove(provider)
        .unwrap_or_default();
    push_unique_string(&mut config.models, model.trim())?;
    move_unique_to_front(&mut config.favorite_models, model.trim())?;
    save_user_provider_config_at(
        path,
        provider,
        ProviderConfigUpdate {
            models: Some(config.models),
            favorite_models: Some(config.favorite_models),
            ..ProviderConfigUpdate::default()
        },
    )
}

pub fn load_provider_ui_configs(cwd: &Path) -> Result<BTreeMap<String, ProviderUiConfig>, String> {
    let env_lookup = |key: &str| std::env::var(key).ok();
    let cli = CliOverrides::default();
    let mut merged = BTreeMap::new();
    for path in config_paths(cwd, &cli, &env_lookup)? {
        merge_provider_ui_configs(&mut merged, load_provider_ui_config_at(&path)?);
    }
    Ok(merged)
}

pub fn load_provider_ui_config_at(
    path: &Path,
) -> Result<BTreeMap<String, ProviderUiConfig>, String> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let Some(file_config) = read_config_file(path, &|key| std::env::var(key).ok())? else {
        return Ok(BTreeMap::new());
    };
    Ok(file_config
        .providers
        .unwrap_or_default()
        .into_iter()
        .map(|(provider, config)| {
            (
                provider,
                ProviderUiConfig {
                    api_base: config.api_base,
                    api_key_env: config.api_key_env,
                    default_model: config.default_model,
                    models: config.models.unwrap_or_default(),
                    favorite_models: config.favorite_models.unwrap_or_default(),
                },
            )
        })
        .collect())
}

fn read_config_value_for_write(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Ok(Value::Table(Map::new()));
    }
    let contents = fs::read_to_string(path)
        .map_err(|err| format!("Failed to read config {}: {err}", path.display()))?;
    if contents.trim().is_empty() {
        Ok(Value::Table(Map::new()))
    } else {
        contents
            .parse::<Value>()
            .map_err(|err| format!("Failed to parse config {}: {err}", path.display()))
    }
}

fn write_config_value(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create config dir {}: {err}", parent.display()))?;
    }
    let contents = toml::to_string_pretty(value)
        .map_err(|err| format!("Failed to serialize config {}: {err}", path.display()))?;
    fs::write(path, contents)
        .map_err(|err| format!("Failed to write config {}: {err}", path.display()))
}

fn trim_optional(value: Option<String>, field: &str) -> Result<Option<String>, String> {
    value
        .map(|value| {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                Err(format!("{field} cannot be empty."))
            } else {
                Ok(trimmed)
            }
        })
        .transpose()
}

fn trim_optional_list(
    value: Option<Vec<String>>,
    field: &str,
) -> Result<Option<Vec<String>>, String> {
    value
        .map(|items| {
            let mut cleaned = Vec::new();
            for item in items {
                let trimmed = item.trim();
                if trimmed.is_empty() {
                    return Err(format!("{field} cannot be empty."));
                }
                push_unique_string(&mut cleaned, trimmed)?;
            }
            Ok(cleaned)
        })
        .transpose()
}

fn load_config_with_env<F>(
    cwd: &Path,
    cli: &CliOverrides,
    env_lookup: &F,
) -> Result<ResolvedConfig, String>
where
    F: Fn(&str) -> Option<String>,
{
    let mut resolved = ResolvedConfig::default();
    let mut loaded_files = Vec::new();
    let mut merged_providers = ProvidersFileConfig::default();
    let mut user_ui = None;

    // UI preferences are personal state. Project config remains available for
    // project/provider policy, but its `[ui]` table never enters this chain.
    for path in config_paths(cwd, cli, env_lookup)? {
        if let Some(file_config) = read_config_file(&path, env_lookup)? {
            let ui_source = ui_source_from_file_config(&file_config);
            if !is_project_config_path(cwd, &path) {
                user_ui = ui_source;
            }
            if let Some(providers) = file_config.providers.clone() {
                merge_provider_configs(&mut merged_providers, providers);
            }
            apply_file_config(&mut resolved, file_config, cwd)?;
            loaded_files.push(path);
        }
    }

    apply_env_config(&mut resolved, env_lookup)?;
    apply_cli_provider_selection(&mut resolved, cli);
    let provider = resolved.provider.clone();
    apply_provider_specific_env_config(&mut resolved, env_lookup);
    apply_provider_scoped_config(&mut resolved, &provider, &merged_providers, env_lookup);
    apply_cli_config(&mut resolved, cli);
    let client_ui = system_ui_preferences_with_env(env_lookup);
    let selected_ui = cli
        .ui
        .map(|profile| UiPreferenceSource {
            profile,
            diagnostic: None,
        })
        .or(user_ui)
        .unwrap_or(UiPreferenceSource {
            profile: client_ui,
            diagnostic: None,
        });
    resolved.ui = resolve_ui_preferences(Some(selected_ui.profile), None, None, client_ui);
    if let Some(diagnostic) = selected_ui.diagnostic {
        resolved.ui = ResolvedUiPreferences {
            locale: resolved.ui.locale,
            skin: UiSkin::Aurora,
            mode: UiColorMode::Dark,
            density: UiDensity::Regular,
            motion: resolved.ui.motion,
            diagnostics: vec![diagnostic],
        };
    }
    resolved.ui_diagnostics = resolved.ui.diagnostics.clone();
    resolved.loaded_files = loaded_files;
    Ok(resolved)
}

fn system_ui_preferences_with_env<F>(env_lookup: &F) -> UiPreferences
where
    F: Fn(&str) -> Option<String>,
{
    UiPreferences {
        locale: detect_system_locale(env_lookup),
        skin: UiSkin::Aurora,
        mode: UiColorMode::System,
        density: UiDensity::Regular,
        motion: UiMotion::System,
    }
}

fn config_paths<F>(cwd: &Path, cli: &CliOverrides, env_lookup: &F) -> Result<Vec<PathBuf>, String>
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(path) = cli
        .config_path
        .clone()
        .or_else(|| env_lookup("VIDEN_CONFIG").map(PathBuf::from))
    {
        return Ok(vec![path]);
    }

    let mut paths = Vec::new();
    if let Some(global) = default_config_path(env_lookup) {
        paths.push(global);
    }
    paths.push(cwd.join(".viden").join("config.toml"));
    Ok(paths)
}

fn default_config_path<F>(env_lookup: &F) -> Option<PathBuf>
where
    F: Fn(&str) -> Option<String>,
{
    if cfg!(windows) {
        env_lookup("APPDATA")
            .map(PathBuf::from)
            .map(|base| base.join("viden").join("config.toml"))
    } else if cfg!(target_os = "macos") {
        env_lookup("HOME").map(PathBuf::from).map(|base| {
            base.join("Library")
                .join("Application Support")
                .join("viden")
                .join("config.toml")
        })
    } else {
        env_lookup("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| env_lookup("HOME").map(|home| PathBuf::from(home).join(".config")))
            .map(|base| base.join("viden").join("config.toml"))
    }
}

fn read_config_file<F>(path: &Path, env_lookup: &F) -> Result<Option<FileConfig>, String>
where
    F: Fn(&str) -> Option<String>,
{
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path)
        .map_err(|err| format!("Failed to read config {}: {err}", path.display()))?;
    let raw_value: Value = toml::from_str(&contents)
        .map_err(|err| format!("Failed to parse config {}: {err}", path.display()))?;
    let mut config: FileConfig = toml::from_str(&contents)
        .map_err(|err| format!("Failed to parse config {}: {err}", path.display()))?;
    config.ui_raw = raw_value.get("ui").cloned();
    if config.api_key.is_none()
        && let Some(name) = config.api_key_env.as_deref()
    {
        config.api_key = env_lookup(name);
    }
    Ok(Some(config))
}

fn ui_source_from_file_config(file_config: &FileConfig) -> Option<UiPreferenceSource> {
    file_config.ui_raw.as_ref().map(parse_ui_preferences)
}

fn parse_ui_preferences(raw: &Value) -> UiPreferenceSource {
    let Some(table) = raw.as_table() else {
        return UiPreferenceSource {
            profile: UiPreferences::client_default(),
            diagnostic: Some(UiPreferenceDiagnostic::new(
                "ui.invalid_type",
                "ui",
                "ui",
                Some(value_kind(raw).to_string()),
            )),
        };
    };

    let mut diagnostic = None;
    let locale = parse_locale_field(table, "locale", &mut diagnostic);
    let skin = parse_skin_field(table, "skin", &mut diagnostic);
    let mode = parse_mode_field(table, "mode", &mut diagnostic);
    let density = parse_density_field(table, "density", &mut diagnostic);
    let motion = parse_motion_field(table, "motion", &mut diagnostic);

    UiPreferenceSource {
        profile: UiPreferences {
            locale,
            skin,
            mode,
            density,
            motion,
        },
        diagnostic,
    }
}

fn parse_locale_field(
    table: &Map<String, Value>,
    key: &str,
    diagnostic: &mut Option<UiPreferenceDiagnostic>,
) -> LocaleId {
    match table.get(key).and_then(Value::as_str) {
        Some("system") => LocaleId::System,
        Some("en") => LocaleId::En,
        Some("zh-CN") | Some("zh_CN") | Some("zh-Hans-CN") => LocaleId::ZhCn,
        Some(value) => {
            record_ui_diagnostic(diagnostic, key, value);
            LocaleId::System
        }
        None if table.contains_key(key) => {
            record_ui_type_diagnostic(diagnostic, table, key);
            LocaleId::System
        }
        None => LocaleId::System,
    }
}

fn parse_skin_field(
    table: &Map<String, Value>,
    key: &str,
    diagnostic: &mut Option<UiPreferenceDiagnostic>,
) -> UiSkin {
    match table.get(key).and_then(Value::as_str) {
        Some("aurora") => UiSkin::Aurora,
        Some("ice") => UiSkin::Ice,
        Some("mono") => UiSkin::Mono,
        Some("amber") => UiSkin::Amber,
        Some("phosphor") => UiSkin::Phosphor,
        Some(value) => {
            record_ui_diagnostic(diagnostic, key, value);
            UiSkin::Aurora
        }
        None if table.contains_key(key) => {
            record_ui_type_diagnostic(diagnostic, table, key);
            UiSkin::Aurora
        }
        None => UiSkin::Aurora,
    }
}

fn parse_mode_field(
    table: &Map<String, Value>,
    key: &str,
    diagnostic: &mut Option<UiPreferenceDiagnostic>,
) -> UiColorMode {
    match table.get(key).and_then(Value::as_str) {
        Some("system") => UiColorMode::System,
        Some("dark") => UiColorMode::Dark,
        Some("light") => UiColorMode::Light,
        Some(value) => {
            record_ui_diagnostic(diagnostic, key, value);
            UiColorMode::System
        }
        None if table.contains_key(key) => {
            record_ui_type_diagnostic(diagnostic, table, key);
            UiColorMode::System
        }
        None => UiColorMode::System,
    }
}

fn parse_density_field(
    table: &Map<String, Value>,
    key: &str,
    diagnostic: &mut Option<UiPreferenceDiagnostic>,
) -> UiDensity {
    match table.get(key).and_then(Value::as_str) {
        Some("compact") => UiDensity::Compact,
        Some("regular") => UiDensity::Regular,
        Some("comfy") => UiDensity::Comfy,
        Some(value) => {
            record_ui_diagnostic(diagnostic, key, value);
            UiDensity::Regular
        }
        None if table.contains_key(key) => {
            record_ui_type_diagnostic(diagnostic, table, key);
            UiDensity::Regular
        }
        None => UiDensity::Regular,
    }
}

fn parse_motion_field(
    table: &Map<String, Value>,
    key: &str,
    diagnostic: &mut Option<UiPreferenceDiagnostic>,
) -> UiMotion {
    match table.get(key).and_then(Value::as_str) {
        Some("system") => UiMotion::System,
        Some("reduced") => UiMotion::Reduced,
        Some("full") => UiMotion::Full,
        Some(value) => {
            record_ui_diagnostic(diagnostic, key, value);
            UiMotion::System
        }
        None if table.contains_key(key) => {
            record_ui_type_diagnostic(diagnostic, table, key);
            UiMotion::System
        }
        None => UiMotion::System,
    }
}

fn record_ui_diagnostic(diagnostic: &mut Option<UiPreferenceDiagnostic>, key: &str, value: &str) {
    if diagnostic.is_none() {
        *diagnostic = Some(UiPreferenceDiagnostic::new(
            "ui.invalid_value",
            key,
            format!("ui.{key}"),
            Some(value.to_string()),
        ));
    }
}

fn record_ui_type_diagnostic(
    diagnostic: &mut Option<UiPreferenceDiagnostic>,
    table: &Map<String, Value>,
    key: &str,
) {
    if diagnostic.is_none() {
        *diagnostic = Some(UiPreferenceDiagnostic::new(
            "ui.invalid_type",
            key,
            format!("ui.{key}"),
            table.get(key).map(value_kind).map(str::to_string),
        ));
    }
}

pub(crate) fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Integer(_) => "integer",
        Value::Float(_) => "float",
        Value::Boolean(_) => "boolean",
        Value::Datetime(_) => "datetime",
        Value::Array(_) => "array",
        Value::Table(_) => "table",
    }
}

fn detect_system_locale<F>(env_lookup: &F) -> LocaleId
where
    F: Fn(&str) -> Option<String>,
{
    env_lookup("LC_ALL")
        .or_else(|| env_lookup("LC_MESSAGES"))
        .or_else(|| env_lookup("LANG"))
        .as_deref()
        .map(LocaleId::from_system_locale)
        .unwrap_or(LocaleId::En)
}

fn is_project_config_path(cwd: &Path, path: &Path) -> bool {
    let Some(candidate_raw) = absolute_path(cwd, path) else {
        return true;
    };
    let Some(project_raw) = absolute_path(cwd, Path::new(".viden/config.toml")) else {
        return true;
    };
    let candidate = lexical_normalize(&candidate_raw);
    let project_path = lexical_normalize(&project_raw);
    if candidate == project_path {
        return true;
    }

    // Resolve every existing prefix before applying later parent components.
    // This preserves filesystem symlink semantics even when the final target
    // is missing, while unresolved components remain a lexical suffix.
    match (
        resolve_existing_components(&candidate_raw),
        resolve_existing_components(&project_raw),
    ) {
        (Some(candidate), Some(project)) => candidate == project,
        _ => true,
    }
}

fn absolute_path(cwd: &Path, path: &Path) -> Option<PathBuf> {
    let absolute_cwd = if cwd.is_absolute() {
        cwd.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(cwd)
    };
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        absolute_cwd.join(path)
    };
    Some(absolute)
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => match normalized.components().next_back() {
                Some(Component::Normal(_)) => {
                    normalized.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                Some(Component::ParentDir) | None => normalized.push(component.as_os_str()),
                Some(Component::CurDir) => unreachable!("curdir components are never retained"),
            },
        }
    }
    normalized
}

fn resolve_existing_components(path: &Path) -> Option<PathBuf> {
    let mut resolved = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                resolved.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(_) => {
                let next = resolved.join(component.as_os_str());
                match fs::canonicalize(&next) {
                    Ok(canonical) => resolved = canonical,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => resolved = next,
                    Err(_) => return None,
                }
            }
        }
    }
    Some(lexical_normalize(&resolved))
}

fn apply_file_config(
    resolved: &mut ResolvedConfig,
    file: FileConfig,
    cwd: &Path,
) -> Result<(), String> {
    if let Some(provider) = file.provider {
        resolved.provider = provider;
    }
    if let Some(model) = file.model {
        resolved.model = Some(model);
    }
    if let Some(api_base) = file.api_base {
        resolved.api_base = Some(api_base);
    }
    if let Some(api_key) = file.api_key {
        resolved.api_key = Some(api_key);
    }
    if let Some(provider_plugin_dirs) = file.provider_plugin_dirs {
        resolved.provider_plugin_dirs = provider_plugin_dirs
            .into_iter()
            .map(|path| resolve_path(cwd, &path))
            .collect();
    }
    if let Some(permission_mode) = file.permission_mode {
        resolved.permission_mode = PermissionMode::parse_cli(&permission_mode)
            .ok_or_else(|| format!("Unknown permission mode `{permission_mode}` in config"))?;
    }
    if let Some(session_home) = file.session_home {
        resolved.session_home = Some(resolve_path(cwd, &session_home));
    }
    if let Some(request_timeout_secs) = file.request_timeout_secs {
        resolved.request_timeout_secs = request_timeout_secs.max(1);
    }
    if let Some(max_retries) = file.max_retries {
        resolved.max_retries = max_retries;
    }
    Ok(())
}

fn apply_env_config<F>(resolved: &mut ResolvedConfig, env_lookup: &F) -> Result<(), String>
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(provider) = env_lookup("VIDEN_PROVIDER") {
        resolved.provider = provider;
    }
    if let Some(model) = env_lookup("VIDEN_MODEL")
        && !model.trim().is_empty()
    {
        resolved.model = Some(model);
    }
    if let Some(api_base) = env_lookup("VIDEN_API_BASE") {
        resolved.api_base = Some(api_base);
    }
    if let Some(api_key) = env_lookup("VIDEN_API_KEY") {
        resolved.api_key = Some(api_key);
    }
    if let Some(plugin_dirs) = env_lookup("VIDEN_PROVIDER_PLUGIN_DIRS") {
        resolved.provider_plugin_dirs = std::env::split_paths(&plugin_dirs).collect();
    }
    if let Some(permission_mode) = env_lookup("VIDEN_PERMISSION_MODE") {
        resolved.permission_mode = PermissionMode::parse_cli(&permission_mode)
            .ok_or_else(|| format!("Unknown permission mode `{permission_mode}` in environment"))?;
    }
    if let Some(session_home) = env_lookup("VIDEN_SESSION_HOME") {
        resolved.session_home = Some(PathBuf::from(session_home));
    }
    if let Some(request_timeout_secs) = env_lookup("VIDEN_REQUEST_TIMEOUT_SECS") {
        resolved.request_timeout_secs = request_timeout_secs
            .parse::<u64>()
            .map_err(|_| "VIDEN_REQUEST_TIMEOUT_SECS must be an integer".to_string())?
            .max(1);
    }
    if let Some(max_retries) = env_lookup("VIDEN_MAX_RETRIES") {
        resolved.max_retries = max_retries
            .parse::<u32>()
            .map_err(|_| "VIDEN_MAX_RETRIES must be an integer".to_string())?;
    }
    Ok(())
}

fn apply_provider_specific_env_config<F>(resolved: &mut ResolvedConfig, env_lookup: &F)
where
    F: Fn(&str) -> Option<String>,
{
    let env_prefix = provider_env_prefix(&resolved.provider);
    if let Some(api_key) = env_lookup(&format!("{env_prefix}_API_KEY"))
        .or_else(|| env_lookup(&format!("VIDEN_{env_prefix}_API_KEY")))
    {
        resolved.api_key = Some(api_key);
    }
    if let Some(api_base) = env_lookup(&format!("{env_prefix}_API_BASE"))
        .or_else(|| env_lookup(&format!("VIDEN_{env_prefix}_API_BASE")))
    {
        resolved.api_base = Some(api_base);
    }
}

fn apply_provider_scoped_config<F>(
    resolved: &mut ResolvedConfig,
    provider: &str,
    providers: &ProvidersFileConfig,
    env_lookup: &F,
) where
    F: Fn(&str) -> Option<String>,
{
    let scoped = providers
        .get(provider)
        .or_else(|| provider_alias(provider).and_then(|alias| providers.get(alias)));

    if let Some(scoped) = scoped {
        if let Some(api_base) = &scoped.api_base {
            resolved.api_base = Some(api_base.clone());
        }
        if let Some(api_key) = &scoped.api_key {
            resolved.api_key = Some(api_key.clone());
        }
        if let Some(api_key_env) = &scoped.api_key_env
            && let Some(value) = env_lookup(api_key_env)
        {
            resolved.api_key = Some(value);
        }
        if let Some(default_model) = &scoped.default_model
            && resolved.model.is_none()
        {
            resolved.model = Some(default_model.clone());
        }
    }
}

fn merge_provider_configs(target: &mut ProvidersFileConfig, incoming: ProvidersFileConfig) {
    for (provider, incoming) in incoming {
        merge_provider_scoped_config(target.entry(provider).or_default(), incoming);
    }
}

fn merge_provider_scoped_config(
    entry: &mut ProviderScopedFileConfig,
    incoming: ProviderScopedFileConfig,
) {
    if incoming.api_base.is_some() {
        entry.api_base = incoming.api_base;
    }
    if incoming.api_key.is_some() {
        entry.api_key = incoming.api_key;
    }
    if incoming.api_key_env.is_some() {
        entry.api_key_env = incoming.api_key_env;
    }
    if incoming.default_model.is_some() {
        entry.default_model = incoming.default_model;
    }
    if incoming.models.is_some() {
        entry.models = incoming.models;
    }
    if incoming.favorite_models.is_some() {
        entry.favorite_models = incoming.favorite_models;
    }
}

fn merge_provider_ui_configs(
    target: &mut BTreeMap<String, ProviderUiConfig>,
    incoming: BTreeMap<String, ProviderUiConfig>,
) {
    for (provider, incoming) in incoming {
        let entry = target.entry(provider).or_default();
        if incoming.api_base.is_some() {
            entry.api_base = incoming.api_base;
        }
        if incoming.api_key_env.is_some() {
            entry.api_key_env = incoming.api_key_env;
        }
        if incoming.default_model.is_some() {
            entry.default_model = incoming.default_model;
        }
        if !incoming.models.is_empty() {
            entry.models = incoming.models;
        }
        if !incoming.favorite_models.is_empty() {
            entry.favorite_models = incoming.favorite_models;
        }
    }
}

fn push_unique_string(values: &mut Vec<String>, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Model cannot be empty.".to_string());
    }
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
    Ok(())
}

fn move_unique_to_front(values: &mut Vec<String>, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Favorite model cannot be empty.".to_string());
    }
    values.retain(|existing| existing != value);
    values.insert(0, value.to_string());
    Ok(())
}

fn provider_alias(provider: &str) -> Option<&'static str> {
    match provider {
        "deepseek-anthropic" => Some("deepseek"),
        "openai-compatible" => Some("openai"),
        _ => None,
    }
}

fn provider_env_prefix(provider: &str) -> String {
    provider_alias(provider)
        .unwrap_or(provider)
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn apply_cli_config(resolved: &mut ResolvedConfig, cli: &CliOverrides) {
    if let Some(provider) = &cli.provider {
        resolved.provider = provider.clone();
    }
    if let Some(model) = &cli.model {
        resolved.model = Some(model.clone());
    }
    if let Some(api_base) = &cli.api_base {
        resolved.api_base = Some(api_base.clone());
    }
    if let Some(api_key) = &cli.api_key {
        resolved.api_key = Some(api_key.clone());
    }
    if !cli.provider_plugin_dirs.is_empty() {
        resolved.provider_plugin_dirs = cli.provider_plugin_dirs.clone();
    }
    if let Some(permission_mode) = cli.permission_mode {
        resolved.permission_mode = permission_mode;
    }
    if let Some(session_home) = &cli.session_home {
        resolved.session_home = Some(session_home.clone());
    }
    if let Some(request_timeout_secs) = cli.request_timeout_secs {
        resolved.request_timeout_secs = request_timeout_secs.max(1);
    }
    if let Some(max_retries) = cli.max_retries {
        resolved.max_retries = max_retries;
    }
}

fn apply_cli_provider_selection(resolved: &mut ResolvedConfig, cli: &CliOverrides) {
    if let Some(provider) = &cli.provider {
        resolved.provider = provider.clone();
    }
}

fn resolve_path(cwd: &Path, raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}

fn format_path_list(paths: &[PathBuf]) -> String {
    if paths.is_empty() {
        return "<default>".to_string();
    }
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests;
