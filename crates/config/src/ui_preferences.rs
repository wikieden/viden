use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use toml::{Value, map::Map};
use viden_types::{
    ResolvedUiPreferences, UiPreferenceDiagnostic, UiPreferencePatch, UiPreferences,
    resolve_ui_preferences,
};

static UI_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum UiPreferenceWriteFailure {
    AfterTempSync,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiPreferenceFileState {
    pub persisted: Option<UiPreferences>,
    pub resolved: ResolvedUiPreferences,
    pub diagnostics: Vec<UiPreferenceDiagnostic>,
}

pub fn preview_user_ui_preferences_at(
    path: &Path,
    patch: &UiPreferencePatch,
    system_context: UiPreferences,
) -> Result<UiPreferenceFileState, String> {
    let value = read_config_value(path)?;
    let current = strict_persisted_profile(&value)?.unwrap_or_else(UiPreferences::client_default);
    let persisted = apply_patch(current, patch);
    validated_state(Some(persisted), None, system_context)
}

pub fn save_user_ui_preferences_at(
    path: &Path,
    patch: &UiPreferencePatch,
    system_context: UiPreferences,
) -> Result<UiPreferenceFileState, String> {
    let mut value = read_config_value(path)?;
    let state = preview_state_from_value(&value, patch, system_context)?;
    let root = value
        .as_table_mut()
        .ok_or_else(|| format!("Config {} must be a TOML table", path.display()))?;
    let ui = root
        .entry("ui".to_string())
        .or_insert_with(|| Value::Table(Map::new()))
        .as_table_mut()
        .ok_or_else(|| format!("Config {} [ui] must be a TOML table", path.display()))?;
    apply_patch_to_table(ui, patch)?;
    atomic_write_config(path, &value, None)?;
    Ok(state)
}

pub fn preview_reset_user_ui_preferences_at(
    path: &Path,
    cli_override: Option<UiPreferences>,
    system_context: UiPreferences,
) -> Result<UiPreferenceFileState, String> {
    let value = read_config_value(path)?;
    strict_persisted_profile(&value)?;
    validated_state(None, cli_override, system_context)
}

#[cfg(test)]
pub(crate) fn save_user_ui_preferences_at_with_failure(
    path: &Path,
    patch: &UiPreferencePatch,
    system_context: UiPreferences,
    failure: UiPreferenceWriteFailure,
) -> Result<UiPreferenceFileState, String> {
    let mut value = read_config_value(path)?;
    let state = preview_state_from_value(&value, patch, system_context)?;
    let root = value
        .as_table_mut()
        .ok_or_else(|| format!("Config {} must be a TOML table", path.display()))?;
    let ui = root
        .entry("ui".to_string())
        .or_insert_with(|| Value::Table(Map::new()))
        .as_table_mut()
        .ok_or_else(|| format!("Config {} [ui] must be a TOML table", path.display()))?;
    apply_patch_to_table(ui, patch)?;
    atomic_write_config(path, &value, Some(failure))?;
    Ok(state)
}

pub fn reset_user_ui_preferences_at(
    path: &Path,
    cli_override: Option<UiPreferences>,
    system_context: UiPreferences,
) -> Result<UiPreferenceFileState, String> {
    let mut value = read_config_value(path)?;
    strict_persisted_profile(&value)?;
    let state = validated_state(None, cli_override, system_context)?;
    let root = value
        .as_table_mut()
        .ok_or_else(|| format!("Config {} must be a TOML table", path.display()))?;
    // `[ui.layout]` is a different record answering a different command
    // (`ui.layout_preferences`). It lives under `[ui]` for readability only,
    // so an appearance reset lifts it across rather than taking it with the
    // profile: an operator resetting their theme must not find their cockpit
    // rearranged.
    let layout = crate::ui_layout::take_layout_table(root);
    root.remove("ui");
    if let Some(layout) = layout {
        crate::ui_layout::restore_layout_table(root, layout);
    }
    atomic_write_config(path, &value, None)?;
    Ok(state)
}

pub fn resolve_user_ui_preferences_at(
    path: &Path,
    cli_override: Option<UiPreferences>,
    system_context: UiPreferences,
) -> Result<UiPreferenceFileState, String> {
    let value = read_config_value(path)?;
    let (persisted, source_diagnostic) = persisted_profile(&value);
    let mut state = validated_state(persisted, cli_override, system_context)?;
    if cli_override.is_none()
        && let Some(diagnostic) = source_diagnostic
    {
        state.resolved = ResolvedUiPreferences {
            locale: state.resolved.locale,
            skin: viden_types::UiSkin::Aurora,
            mode: viden_types::UiColorMode::Dark,
            density: viden_types::UiDensity::Regular,
            motion: state.resolved.motion,
            diagnostics: vec![diagnostic.clone()],
        };
        state.diagnostics = vec![diagnostic];
    }
    Ok(state)
}

fn preview_state_from_value(
    value: &Value,
    patch: &UiPreferencePatch,
    system_context: UiPreferences,
) -> Result<UiPreferenceFileState, String> {
    let current = strict_persisted_profile(value)?.unwrap_or_else(UiPreferences::client_default);
    let persisted = apply_patch(current, patch);
    validated_state(Some(persisted), None, system_context)
}

fn validated_state(
    persisted: Option<UiPreferences>,
    cli_override: Option<UiPreferences>,
    system_context: UiPreferences,
) -> Result<UiPreferenceFileState, String> {
    let resolved = resolve_ui_preferences(cli_override, persisted, None, system_context);
    if let Some(diagnostic) = resolved.diagnostics.first() {
        return Err(format!(
            "{}: {}",
            diagnostic.code,
            diagnostic
                .rejected_value
                .as_deref()
                .unwrap_or("invalid value")
        ));
    }
    Ok(UiPreferenceFileState {
        persisted,
        diagnostics: resolved.diagnostics.clone(),
        resolved,
    })
}

fn persisted_profile(value: &Value) -> (Option<UiPreferences>, Option<UiPreferenceDiagnostic>) {
    let Some(raw) = value.get("ui") else {
        return (None, None);
    };
    let source = super::parse_ui_preferences(raw);
    (Some(source.profile), source.diagnostic)
}

fn strict_persisted_profile(value: &Value) -> Result<Option<UiPreferences>, String> {
    let (profile, diagnostic) = persisted_profile(value);
    if let Some(diagnostic) = diagnostic {
        return Err(format!(
            "{}: {}",
            diagnostic.code,
            diagnostic
                .rejected_value
                .as_deref()
                .unwrap_or("invalid value")
        ));
    }
    Ok(profile)
}

fn apply_patch(mut profile: UiPreferences, patch: &UiPreferencePatch) -> UiPreferences {
    if let Some(locale) = patch.locale {
        profile.locale = locale;
    }
    if let Some(skin) = patch.skin {
        profile.skin = skin;
    }
    if let Some(mode) = patch.mode {
        profile.mode = mode;
    }
    if let Some(density) = patch.density {
        profile.density = density;
    }
    if let Some(motion) = patch.motion {
        profile.motion = motion;
    }
    profile
}

fn apply_patch_to_table(
    table: &mut Map<String, Value>,
    patch: &UiPreferencePatch,
) -> Result<(), String> {
    if let Some(locale) = patch.locale {
        table.insert("locale".to_string(), enum_value(locale)?);
    }
    if let Some(skin) = patch.skin {
        table.insert("skin".to_string(), enum_value(skin)?);
    }
    if let Some(mode) = patch.mode {
        table.insert("mode".to_string(), enum_value(mode)?);
    }
    if let Some(density) = patch.density {
        table.insert("density".to_string(), enum_value(density)?);
    }
    if let Some(motion) = patch.motion {
        table.insert("motion".to_string(), enum_value(motion)?);
    }
    Ok(())
}

fn enum_value<T: serde::Serialize>(value: T) -> Result<Value, String> {
    Value::try_from(value).map_err(|error| format!("failed to serialize UI preference: {error}"))
}

pub(crate) fn read_config_value(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Ok(Value::Table(Map::new()));
    }
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read config {}: {error}", path.display()))?;
    if contents.trim().is_empty() {
        Ok(Value::Table(Map::new()))
    } else {
        contents
            .parse::<Value>()
            .map_err(|error| format!("Failed to parse config {}: {error}", path.display()))
    }
}

pub(crate) fn atomic_write_config(
    path: &Path,
    value: &Value,
    #[cfg_attr(not(test), allow(unused_variables))] failure: Option<UiPreferenceWriteFailure>,
) -> Result<(), String> {
    let contents = toml::to_string_pretty(value)
        .map_err(|error| format!("Failed to serialize config {}: {error}", path.display()))?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create config dir {}: {error}", parent.display()))?;
    let temp_path = unique_temp_path(path);
    let mut cleanup = TempCleanup::new(temp_path.clone());
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut temp = options.open(&temp_path).map_err(|error| {
        format!(
            "Failed to create config temp {}: {error}",
            temp_path.display()
        )
    })?;
    temp.write_all(contents.as_bytes())
        .and_then(|()| temp.sync_all())
        .map_err(|error| {
            format!(
                "Failed to sync config temp {}: {error}",
                temp_path.display()
            )
        })?;
    drop(temp);
    #[cfg(test)]
    if failure == Some(UiPreferenceWriteFailure::AfterTempSync) {
        return Err("injected UI preference write failure after temp sync".to_string());
    }
    atomic_replace(&temp_path, path)?;
    cleanup.disarm();
    sync_parent_directory(parent)?;
    Ok(())
}

fn unique_temp_path(path: &Path) -> PathBuf {
    let sequence = UI_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.toml");
    path.with_file_name(format!(".{name}.ui-{}-{sequence}.tmp", std::process::id()))
}

struct TempCleanup {
    path: PathBuf,
    armed: bool,
}

impl TempCleanup {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempCleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }
    let destination_display = destination.display().to_string();
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // Windows rename does not replace an existing destination. MoveFileExW
    // supplies the required replace-existing and write-through semantics.
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(format!(
            "Failed to atomically replace config {destination_display}: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(|error| {
        format!(
            "Failed to atomically replace config {}: {error}",
            destination.display()
        )
    })
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path) -> Result<(), String> {
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("Failed to sync config dir {}: {error}", parent.display()))
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path) -> Result<(), String> {
    // MoveFileExW uses WRITE_THROUGH on Windows. Other non-Unix targets do not
    // expose a portable directory handle sync through std.
    Ok(())
}
