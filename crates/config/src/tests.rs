use super::*;
use std::collections::BTreeMap;
use viden_types::{
    LaneSidebarMode, LocaleId, ProjectIdOrigin, UiColorMode, UiDensity, UiLayoutPreferencePatch,
    UiLayoutPreferences, UiMotion, UiPreferencePatch, UiPreferences, UiSkin,
};

fn default_config_path_for_test(root: &Path) -> PathBuf {
    if cfg!(windows) {
        root.join("AppData")
            .join("Roaming")
            .join("viden")
            .join("config.toml")
    } else if cfg!(target_os = "macos") {
        root.join("Library")
            .join("Application Support")
            .join("viden")
            .join("config.toml")
    } else {
        root.join(".config").join("viden").join("config.toml")
    }
}

fn map_env(values: &[(&str, &str)]) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn ui_preferences_write_patch_preserves_unknown_top_level_and_ui_fields() {
    let root = std::env::temp_dir().join(format!("viden_ui_write_preserve_{}", std::process::id()));
    let path = root.join("config.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        &path,
        r#"custom = 7

[ui]
locale = "en"
skin = "ice"
mode = "dark"
density = "compact"
motion = "full"
future_field = "preserve-me"
"#,
    )
    .unwrap();

    save_user_ui_preferences_at(
        &path,
        &UiPreferencePatch {
            locale: Some(LocaleId::ZhCn),
            ..UiPreferencePatch::default()
        },
        UiPreferences::client_default(),
    )
    .unwrap();

    let value = fs::read_to_string(&path)
        .unwrap()
        .parse::<toml::Value>()
        .unwrap();
    assert_eq!(
        value.get("custom").and_then(toml::Value::as_integer),
        Some(7)
    );
    assert_eq!(
        value
            .get("ui")
            .and_then(toml::Value::as_table)
            .and_then(|ui| ui.get("future_field"))
            .and_then(toml::Value::as_str),
        Some("preserve-me")
    );
    assert_eq!(
        value
            .get("ui")
            .and_then(toml::Value::as_table)
            .and_then(|ui| ui.get("locale"))
            .and_then(toml::Value::as_str),
        Some("zh-CN")
    );
}

#[test]
fn ui_preferences_write_rejects_invalid_complete_pair_without_changing_bytes() {
    let root = std::env::temp_dir().join(format!(
        "viden_ui_write_invalid_pair_{}",
        std::process::id()
    ));
    let path = root.join("config.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let original = b"[ui]\nskin = \"amber\"\nmode = \"dark\"\n";
    fs::write(&path, original).unwrap();

    let error = save_user_ui_preferences_at(
        &path,
        &UiPreferencePatch {
            mode: Some(UiColorMode::Light),
            ..UiPreferencePatch::default()
        },
        UiPreferences::client_default(),
    )
    .unwrap_err();

    assert!(error.contains("ui.invalid_skin_mode_pair"));
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn ui_preferences_write_persists_exactly_eight_supported_skin_mode_pairs() {
    let root =
        std::env::temp_dir().join(format!("viden_ui_write_eight_pairs_{}", std::process::id()));
    let path = root.join("config.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let pairs = [
        (UiSkin::Aurora, UiColorMode::Dark),
        (UiSkin::Aurora, UiColorMode::Light),
        (UiSkin::Ice, UiColorMode::Dark),
        (UiSkin::Ice, UiColorMode::Light),
        (UiSkin::Mono, UiColorMode::Dark),
        (UiSkin::Mono, UiColorMode::Light),
        (UiSkin::Amber, UiColorMode::Dark),
        (UiSkin::Phosphor, UiColorMode::Dark),
    ];

    for (skin, mode) in pairs {
        let state = save_user_ui_preferences_at(
            &path,
            &UiPreferencePatch {
                locale: Some(LocaleId::En),
                skin: Some(skin),
                mode: Some(mode),
                density: Some(UiDensity::Regular),
                motion: Some(UiMotion::System),
            },
            UiPreferences::client_default(),
        )
        .unwrap();
        assert_eq!(state.persisted.unwrap().skin, skin);
        assert_eq!(state.resolved.mode, mode);
    }
}

#[test]
fn ui_preferences_write_reset_removes_entire_ui_table_only() {
    let root = std::env::temp_dir().join(format!("viden_ui_write_reset_{}", std::process::id()));
    let path = root.join("config.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        &path,
        "custom = 7\n[ui]\nskin = \"ice\"\nfuture_field = \"remove-too\"\n",
    )
    .unwrap();

    let state = reset_user_ui_preferences_at(&path, None, UiPreferences::client_default()).unwrap();

    let value = fs::read_to_string(&path)
        .unwrap()
        .parse::<toml::Value>()
        .unwrap();
    assert!(value.get("ui").is_none());
    assert_eq!(
        value.get("custom").and_then(toml::Value::as_integer),
        Some(7)
    );
    assert_eq!(state.persisted, None);
}

#[test]
fn ui_preferences_write_reset_rejects_invalid_override_before_changing_bytes() {
    let root = std::env::temp_dir().join(format!(
        "viden_ui_write_reset_invalid_{}",
        std::process::id()
    ));
    let path = root.join("config.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let original = b"custom = 7\n[ui]\nskin = \"ice\"\nmode = \"dark\"\n";
    fs::write(&path, original).unwrap();
    let invalid_override = UiPreferences {
        locale: LocaleId::En,
        skin: UiSkin::Amber,
        mode: UiColorMode::Light,
        density: UiDensity::Regular,
        motion: UiMotion::System,
    };

    assert!(
        reset_user_ui_preferences_at(
            &path,
            Some(invalid_override),
            UiPreferences::client_default(),
        )
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn ui_preferences_write_corrupt_toml_preserves_bytes_and_creates_no_temp() {
    let root = std::env::temp_dir().join(format!(
        "viden_ui_write_corrupt_toml_{}",
        std::process::id()
    ));
    let path = root.join("config.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let original = b"[ui\nskin = \"ice\"\n";
    fs::write(&path, original).unwrap();

    assert!(
        save_user_ui_preferences_at(
            &path,
            &UiPreferencePatch {
                locale: Some(LocaleId::ZhCn),
                ..UiPreferencePatch::default()
            },
            UiPreferences::client_default(),
        )
        .is_err()
    );

    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(temp_files_for(&root).len(), 0);
}

#[cfg(unix)]
#[test]
fn ui_preferences_write_installs_mode_0600() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!("viden_ui_write_mode_{}", std::process::id()));
    let path = root.join("config.toml");
    let _ = fs::remove_dir_all(&root);
    save_user_ui_preferences_at(
        &path,
        &UiPreferencePatch {
            locale: Some(LocaleId::En),
            ..UiPreferencePatch::default()
        },
        UiPreferences::client_default(),
    )
    .unwrap();

    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn ui_preferences_write_atomic_failure_preserves_destination_and_cleans_temp() {
    let root = std::env::temp_dir().join(format!(
        "viden_ui_write_atomic_failure_{}",
        std::process::id()
    ));
    let path = root.join("config.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let original = b"[ui]\nskin = \"mono\"\nmode = \"dark\"\n";
    fs::write(&path, original).unwrap();

    let error = save_user_ui_preferences_at_with_failure(
        &path,
        &UiPreferencePatch {
            skin: Some(UiSkin::Ice),
            ..UiPreferencePatch::default()
        },
        UiPreferences::client_default(),
        UiPreferenceWriteFailure::AfterTempSync,
    )
    .unwrap_err();

    assert!(error.contains("injected UI preference write failure"));
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(temp_files_for(&root).len(), 0);
}

fn temp_files_for(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(".ui-") && name.ends_with(".tmp"))
        })
        .collect()
}

#[test]
fn ui_preferences_cli_user_system_precedence_is_whole_profile() {
    let root = std::env::temp_dir().join(format!("viden_ui_precedence_{}", std::process::id()));
    let global_config_path = default_config_path_for_test(&root);
    let project_config_path = root.join("project").join(".viden").join("config.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(global_config_path.parent().unwrap()).unwrap();
    fs::create_dir_all(project_config_path.parent().unwrap()).unwrap();
    fs::write(
        &global_config_path,
        r#"
[ui]
locale = "en"
skin = "mono"
mode = "dark"
density = "compact"
motion = "full"
"#,
    )
    .unwrap();
    fs::write(
        &project_config_path,
        r#"
[ui]
locale = "zh-CN"
skin = "ice"
mode = "light"
density = "comfy"
motion = "reduced"
"#,
    )
    .unwrap();

    let env_map = map_env(&[("HOME", root.to_string_lossy().as_ref())]);
    let user_config =
        load_config_with_env(&root.join("project"), &CliOverrides::default(), &|key| {
            env_map.get(key).cloned()
        })
        .unwrap();

    assert_eq!(user_config.ui.locale, LocaleId::En);
    assert_eq!(user_config.ui.skin, UiSkin::Mono);
    assert_eq!(user_config.ui.mode, UiColorMode::Dark);
    assert_eq!(user_config.ui.density, UiDensity::Compact);
    assert_eq!(user_config.ui.motion, UiMotion::Full);

    let cli = CliOverrides {
        ui: Some(UiPreferences {
            locale: LocaleId::ZhCn,
            skin: UiSkin::Aurora,
            mode: UiColorMode::Light,
            density: UiDensity::Regular,
            motion: UiMotion::Reduced,
        }),
        ..CliOverrides::default()
    };
    let cli_config = load_config_with_env(&root.join("project"), &cli, &|key| {
        env_map.get(key).cloned()
    })
    .unwrap();

    assert_eq!(cli_config.ui.locale, LocaleId::ZhCn);
    assert_eq!(cli_config.ui.skin, UiSkin::Aurora);
    assert_eq!(cli_config.ui.mode, UiColorMode::Light);
    assert_eq!(cli_config.ui.density, UiDensity::Regular);
    assert_eq!(cli_config.ui.motion, UiMotion::Reduced);
}

#[test]
fn ui_preferences_project_config_does_not_set_personal_preferences() {
    let root =
        std::env::temp_dir().join(format!("viden_ui_project_default_{}", std::process::id()));
    let project_config_path = root.join("project").join(".viden").join("config.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(project_config_path.parent().unwrap()).unwrap();
    fs::write(
        &project_config_path,
        r#"
[ui]
locale = "system"
skin = "aurora"
mode = "system"
density = "regular"
motion = "reduced"
"#,
    )
    .unwrap();

    let env_map = map_env(&[
        ("HOME", root.to_string_lossy().as_ref()),
        ("LC_ALL", "zh_CN.UTF-8"),
    ]);
    let config = load_config_with_env(&root.join("project"), &CliOverrides::default(), &|key| {
        env_map.get(key).cloned()
    })
    .unwrap();

    assert_eq!(config.ui.locale, LocaleId::ZhCn);
    assert_eq!(config.ui.mode, UiColorMode::Dark);
    assert_eq!(config.ui.motion, UiMotion::System);
    assert!(config.ui_diagnostics.is_empty());
}

#[test]
fn ui_preferences_project_config_path_is_never_a_personal_write_target() {
    let root = std::env::temp_dir().join(format!(
        "viden_ui_project_write_target_{}",
        std::process::id()
    ));
    let cwd = root.join("project");
    let project_path = cwd.join(".viden").join("config.toml");
    let env_map = map_env(&[("HOME", root.to_string_lossy().as_ref())]);

    let selected =
        user_ui_config_path_with_env(&cwd, Some(&project_path), &|key| env_map.get(key).cloned())
            .unwrap();

    assert_eq!(selected, default_config_path_for_test(&root));
    assert_ne!(selected, project_path);

    let relative_selected =
        user_ui_config_path_with_env(&cwd, Some(Path::new(".viden/config.toml")), &|key| {
            env_map.get(key).cloned()
        })
        .unwrap();
    assert_eq!(relative_selected, default_config_path_for_test(&root));
}

#[test]
fn ui_preferences_missing_project_path_lexical_aliases_never_become_write_targets() {
    let root = std::env::temp_dir().join(format!(
        "viden_ui_missing_project_write_target_{}",
        std::process::id()
    ));
    let cwd = root.join("project");
    let project_dir = cwd.join(".viden");
    let project_path = project_dir.join("config.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&cwd).unwrap();
    let cwd_mtime = fs::metadata(&cwd).unwrap().modified().unwrap();
    let env_map = map_env(&[("HOME", root.to_string_lossy().as_ref())]);
    let expected = default_config_path_for_test(&root);
    let aliases = [
        PathBuf::from(".viden/../.viden/config.toml"),
        cwd.join(".viden/../.viden/config.toml"),
        PathBuf::from("./.viden/./nested/../config.toml"),
        cwd.join("./.viden/subdir/../../.viden/./config.toml"),
    ];

    for alias in aliases {
        let selected =
            user_ui_config_path_with_env(&cwd, Some(&alias), &|key| env_map.get(key).cloned())
                .unwrap();
        assert_eq!(selected, expected, "alias escaped exclusion: {alias:?}");
    }

    save_user_ui_preferences_at(
        &expected,
        &UiPreferencePatch {
            locale: Some(LocaleId::ZhCn),
            ..UiPreferencePatch::default()
        },
        UiPreferences::client_default(),
    )
    .unwrap();

    assert!(!project_path.exists());
    assert!(!project_dir.exists());
    assert_eq!(fs::metadata(&cwd).unwrap().modified().unwrap(), cwd_mtime);
}

#[cfg(unix)]
#[test]
fn ui_preferences_missing_project_path_resolves_existing_parent_symlink_alias() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "viden_ui_project_symlink_target_{}",
        std::process::id()
    ));
    let real_cwd = root.join("real-project");
    let linked_cwd = root.join("linked-project");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&real_cwd).unwrap();
    symlink(&real_cwd, &linked_cwd).unwrap();
    let env_map = map_env(&[("HOME", root.to_string_lossy().as_ref())]);

    let selected = user_ui_config_path_with_env(
        &linked_cwd,
        Some(&real_cwd.join(".viden/child/../config.toml")),
        &|key| env_map.get(key).cloned(),
    )
    .unwrap();

    assert_eq!(selected, default_config_path_for_test(&root));
    assert!(!real_cwd.join(".viden").exists());
}

#[cfg(unix)]
#[test]
fn ui_preferences_symlink_parent_traversal_cannot_hide_missing_project_target() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "viden_ui_project_symlink_parent_traversal_{}",
        std::process::id()
    ));
    let cwd = root.join("project");
    let symlink_target = cwd.join("nested").join("target");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&symlink_target).unwrap();
    symlink(&symlink_target, cwd.join("alias")).unwrap();
    let env_map = map_env(&[("HOME", root.to_string_lossy().as_ref())]);

    // Filesystem traversal follows `alias` before applying the parent
    // components, so this resolves to `<cwd>/.viden/config.toml` even though a
    // purely lexical collapse points outside `cwd`.
    let selected = user_ui_config_path_with_env(
        &cwd,
        Some(&cwd.join("alias/../../.viden/config.toml")),
        &|key| env_map.get(key).cloned(),
    )
    .unwrap();

    assert_eq!(selected, default_config_path_for_test(&root));
    assert!(!cwd.join(".viden").exists());
}

#[cfg(unix)]
#[test]
fn ui_preferences_indeterminate_symlink_loop_falls_back_without_unsafe_write() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "viden_ui_indeterminate_symlink_loop_{}",
        std::process::id()
    ));
    let cwd = root.join("project");
    let loop_path = cwd.join("loop");
    let unsafe_path = loop_path.join("user").join("config.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&cwd).unwrap();
    symlink("loop", &loop_path).unwrap();
    let cwd_mtime = fs::metadata(&cwd).unwrap().modified().unwrap();
    let env_map = map_env(&[("HOME", root.to_string_lossy().as_ref())]);
    let expected = default_config_path_for_test(&root);

    let selected =
        user_ui_config_path_with_env(&cwd, Some(&unsafe_path), &|key| env_map.get(key).cloned())
            .unwrap();
    assert_eq!(selected, expected);

    save_user_ui_preferences_at(
        &selected,
        &UiPreferencePatch {
            locale: Some(LocaleId::ZhCn),
            ..UiPreferencePatch::default()
        },
        UiPreferences::client_default(),
    )
    .unwrap();
    assert_eq!(fs::read_link(&loop_path).unwrap(), PathBuf::from("loop"));
    assert_eq!(fs::metadata(&cwd).unwrap().modified().unwrap(), cwd_mtime);
}

#[test]
fn ui_preferences_normal_missing_user_path_remains_an_explicit_write_target() {
    let root = std::env::temp_dir().join(format!(
        "viden_ui_normal_missing_user_target_{}",
        std::process::id()
    ));
    let cwd = root.join("project");
    let explicit = root.join("user-config").join("nested").join("config.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&cwd).unwrap();
    let env_map = map_env(&[("HOME", root.to_string_lossy().as_ref())]);

    let selected =
        user_ui_config_path_with_env(&cwd, Some(&explicit), &|key| env_map.get(key).cloned())
            .unwrap();

    assert_eq!(selected, explicit);
    assert!(!selected.exists());
}

#[test]
fn ui_preferences_corrupt_table_preserves_file_and_returns_one_diagnostic() {
    let root = std::env::temp_dir().join(format!("viden_ui_corrupt_{}", std::process::id()));
    let path = default_config_path_for_test(&root);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = r#"
provider = "deepseek"

[ui]
locale = "zh-CN"
skin = "amber"
mode = "light"
density = 3
motion = "reduced"
"#;
    fs::write(&path, original).unwrap();

    let env_map = map_env(&[("HOME", root.to_string_lossy().as_ref())]);
    let config = load_config_with_env(&root, &CliOverrides::default(), &|key| {
        env_map.get(key).cloned()
    })
    .unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert_eq!(config.provider, "deepseek");
    assert_eq!(config.ui.locale, LocaleId::ZhCn);
    assert_eq!(config.ui.skin, UiSkin::Aurora);
    assert_eq!(config.ui.mode, UiColorMode::Dark);
    assert_eq!(config.ui.density, UiDensity::Regular);
    assert_eq!(config.ui.motion, UiMotion::Reduced);
    assert_eq!(config.ui_diagnostics.len(), 1);
    assert_eq!(config.ui_diagnostics[0].key, "density");
}

#[test]
fn ui_preferences_valid_cli_ignores_invalid_lower_priority_sources() {
    let root = write_ui_source_pair(
        "viden_ui_cli_ignores_invalid",
        r#"
[ui]
locale = "en"
skin = "amber"
mode = "light"
density = "compact"
motion = "full"
"#,
        r#"
[ui]
locale = "zh-CN"
skin = "phosphor"
mode = "light"
density = "comfy"
motion = "reduced"
"#,
    );
    let env_map = map_env(&[("HOME", root.to_string_lossy().as_ref())]);
    let cli = CliOverrides {
        ui: Some(UiPreferences {
            locale: LocaleId::ZhCn,
            skin: UiSkin::Ice,
            mode: UiColorMode::Light,
            density: UiDensity::Regular,
            motion: UiMotion::Reduced,
        }),
        ..CliOverrides::default()
    };

    let config = load_config_with_env(&root.join("project"), &cli, &|key| {
        env_map.get(key).cloned()
    })
    .unwrap();

    assert_eq!(config.ui.skin, UiSkin::Ice);
    assert_eq!(config.ui.mode, UiColorMode::Light);
    assert!(config.ui_diagnostics.is_empty());
}

#[test]
fn ui_preferences_valid_user_ignores_invalid_project_source() {
    let root = write_ui_source_pair(
        "viden_ui_user_ignores_project",
        r#"
[ui]
locale = "en"
skin = "mono"
mode = "light"
density = "compact"
motion = "full"
"#,
        r#"
[ui]
locale = "zh-CN"
skin = "amber"
mode = "light"
density = "comfy"
motion = "reduced"
"#,
    );
    let env_map = map_env(&[("HOME", root.to_string_lossy().as_ref())]);

    let config = load_config_with_env(&root.join("project"), &CliOverrides::default(), &|key| {
        env_map.get(key).cloned()
    })
    .unwrap();

    assert_eq!(config.ui.locale, LocaleId::En);
    assert_eq!(config.ui.skin, UiSkin::Mono);
    assert_eq!(config.ui.mode, UiColorMode::Light);
    assert_eq!(config.ui.density, UiDensity::Compact);
    assert_eq!(config.ui.motion, UiMotion::Full);
    assert!(config.ui_diagnostics.is_empty());
}

#[test]
fn ui_preferences_invalid_user_beats_invalid_project_with_one_user_diagnostic() {
    let root = write_ui_source_pair(
        "viden_ui_invalid_user_beats_project",
        r#"
[ui]
locale = "en"
skin = "amber"
mode = "light"
density = "compact"
motion = "full"
"#,
        r#"
[ui]
locale = "zh-CN"
skin = "phosphor"
mode = "light"
density = "comfy"
motion = "reduced"
"#,
    );
    let env_map = map_env(&[("HOME", root.to_string_lossy().as_ref())]);

    let config = load_config_with_env(&root.join("project"), &CliOverrides::default(), &|key| {
        env_map.get(key).cloned()
    })
    .unwrap();

    assert_eq!(config.ui.locale, LocaleId::En);
    assert_eq!(config.ui.skin, UiSkin::Aurora);
    assert_eq!(config.ui.mode, UiColorMode::Dark);
    assert_eq!(config.ui.density, UiDensity::Regular);
    assert_eq!(config.ui.motion, UiMotion::Full);
    assert_eq!(config.ui_diagnostics.len(), 1);
    assert_eq!(
        config.ui_diagnostics[0].rejected_value.as_deref(),
        Some("amber/light")
    );
}

#[test]
fn ui_preferences_absent_user_ignores_invalid_project_source() {
    let root =
        std::env::temp_dir().join(format!("viden_ui_invalid_project_{}", std::process::id()));
    let project_config_path = root.join("project").join(".viden").join("config.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(project_config_path.parent().unwrap()).unwrap();
    fs::write(
        &project_config_path,
        r#"
[ui]
locale = "zh-CN"
skin = "phosphor"
mode = "light"
density = "comfy"
motion = "reduced"
"#,
    )
    .unwrap();
    let env_map = map_env(&[("HOME", root.to_string_lossy().as_ref())]);

    let config = load_config_with_env(&root.join("project"), &CliOverrides::default(), &|key| {
        env_map.get(key).cloned()
    })
    .unwrap();

    assert_eq!(config.ui.locale, LocaleId::En);
    assert_eq!(config.ui.skin, UiSkin::Aurora);
    assert_eq!(config.ui.mode, UiColorMode::Dark);
    assert_eq!(config.ui.density, UiDensity::Regular);
    assert_eq!(config.ui.motion, UiMotion::System);
    assert!(config.ui_diagnostics.is_empty());
}

fn write_ui_source_pair(slug: &str, user: &str, project: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("{slug}_{}", std::process::id()));
    let global_config_path = default_config_path_for_test(&root);
    let project_config_path = root.join("project").join(".viden").join("config.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(global_config_path.parent().unwrap()).unwrap();
    fs::create_dir_all(project_config_path.parent().unwrap()).unwrap();
    fs::write(&global_config_path, user).unwrap();
    fs::write(&project_config_path, project).unwrap();
    root
}

#[test]
fn default_config_uses_deepseek_as_online_provider() {
    let cwd = std::env::temp_dir().join(format!("viden_default_deepseek_{}", std::process::id()));
    let _ = fs::remove_dir_all(&cwd);
    fs::create_dir_all(&cwd).unwrap();

    let config = load_config_with_env(&cwd, &CliOverrides::default(), &|_| None).unwrap();

    assert_eq!(config.provider, "deepseek");
    assert_eq!(config.model, None);
}

#[test]
fn project_file_overrides_global_file_and_env_overrides_files() {
    let root = std::env::temp_dir().join(format!("viden_config_{}", std::process::id()));
    let global_config_path = default_config_path_for_test(&root);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(global_config_path.parent().unwrap()).unwrap();
    fs::create_dir_all(root.join("project").join(".viden")).unwrap();
    fs::write(
        global_config_path,
        "provider = 'anthropic'\nmodel = 'global-model'\npermission_mode = 'default'\n",
    )
    .unwrap();
    fs::write(
        root.join("project").join(".viden").join("config.toml"),
        "model = 'project-model'\npermission_mode = 'plan'\nrequest_timeout_secs = 45\n",
    )
    .unwrap();
    let env_map = map_env(&[
        ("HOME", root.to_string_lossy().as_ref()),
        ("VIDEN_MODEL", "env-model"),
    ]);
    let cli = CliOverrides::default();
    let config = load_config_with_env(&root.join("project"), &cli, &|key| {
        env_map.get(key).cloned()
    })
    .unwrap();
    assert_eq!(config.provider, "anthropic");
    assert_eq!(config.model.as_deref(), Some("env-model"));
    assert_eq!(config.permission_mode, PermissionMode::Plan);
    assert_eq!(config.request_timeout_secs, 45);
    assert_eq!(config.loaded_files.len(), 2);
}

#[test]
fn cli_overrides_win() {
    let cwd = std::env::temp_dir();
    let plugin_dir = cwd.join("cli-provider-plugins");
    let cli = CliOverrides {
        provider: Some("openai".to_string()),
        model: Some("gpt-5.2".to_string()),
        provider_plugin_dirs: vec![plugin_dir.clone()],
        permission_mode: Some(PermissionMode::AcceptEdits),
        request_timeout_secs: Some(120),
        max_retries: Some(3),
        ..CliOverrides::default()
    };
    let env_map: BTreeMap<String, String> = BTreeMap::new();
    let config = load_config_with_env(&cwd, &cli, &|key| env_map.get(key).cloned()).unwrap();
    assert_eq!(config.provider, "openai");
    assert_eq!(config.model.as_deref(), Some("gpt-5.2"));
    assert_eq!(config.provider_plugin_dirs, vec![plugin_dir]);
    assert_eq!(config.permission_mode, PermissionMode::AcceptEdits);
    assert_eq!(config.request_timeout_secs, 120);
    assert_eq!(config.max_retries, 3);
}

#[test]
fn provider_plugin_dirs_resolve_from_file_env_and_cli_precedence() {
    let cwd = std::env::temp_dir().join(format!("viden_plugin_dirs_config_{}", std::process::id()));
    let _ = fs::remove_dir_all(&cwd);
    fs::create_dir_all(cwd.join(".viden")).unwrap();
    fs::write(
        cwd.join(".viden").join("config.toml"),
        r#"
provider_plugin_dirs = ["relative-plugins", "/absolute-plugins"]
"#,
    )
    .unwrap();

    let file_config = load_config_with_env(&cwd, &CliOverrides::default(), &|_| None).unwrap();
    assert_eq!(
        file_config.provider_plugin_dirs,
        vec![
            cwd.join("relative-plugins"),
            PathBuf::from("/absolute-plugins")
        ]
    );

    let env_dirs = std::env::join_paths([cwd.join("env-a"), cwd.join("env-b")]).unwrap();
    let env_dirs = env_dirs.to_string_lossy().to_string();
    let env_map = map_env(&[("VIDEN_PROVIDER_PLUGIN_DIRS", env_dirs.as_str())]);
    let env_config = load_config_with_env(&cwd, &CliOverrides::default(), &|key| {
        env_map.get(key).cloned()
    })
    .unwrap();
    assert_eq!(
        env_config.provider_plugin_dirs,
        vec![cwd.join("env-a"), cwd.join("env-b")]
    );

    let cli_dir = cwd.join("cli-plugins");
    let cli = CliOverrides {
        provider_plugin_dirs: vec![cli_dir.clone()],
        ..CliOverrides::default()
    };
    let cli_config = load_config_with_env(&cwd, &cli, &|key| env_map.get(key).cloned()).unwrap();
    assert_eq!(cli_config.provider_plugin_dirs, vec![cli_dir]);
}

#[test]
fn deepseek_provider_specific_env_overrides_generic_api_fields() {
    let cwd = std::env::temp_dir().join("viden_deepseek_config_test");
    let _ = fs::remove_dir_all(&cwd);
    fs::create_dir_all(cwd.join(".viden")).unwrap();
    fs::write(
        cwd.join(".viden").join("config.toml"),
        r#"
provider = "deepseek"
api_key = "generic-key"
api_base = "https://generic.example"
[providers.deepseek]
api_base = "https://provider.example"
"#,
    )
    .unwrap();

    let env_map = map_env(&[("DEEPSEEK_API_KEY", "provider-key")]);

    let config = load_config_with_env(&cwd, &CliOverrides::default(), &|key| {
        env_map.get(key).cloned()
    })
    .unwrap();

    assert_eq!(config.provider, "deepseek");
    assert_eq!(config.api_key.as_deref(), Some("provider-key"));
    assert_eq!(config.api_base.as_deref(), Some("https://provider.example"));
}

#[test]
fn save_user_provider_model_defaults_updates_existing_config_without_secrets() {
    let root =
        std::env::temp_dir().join(format!("viden_save_provider_model_{}", std::process::id()));
    let path = root.join("config.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        &path,
        r#"
permission_mode = "plan"

[providers.deepseek]
api_base = "https://api.deepseek.example"
"#,
    )
    .unwrap();

    save_user_provider_model_defaults_at(&path, "deepseek", "deepseek-v4-flash").unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains(r#"provider = "deepseek""#));
    assert!(contents.contains(r#"model = "deepseek-v4-flash""#));
    assert!(contents.contains(r#"permission_mode = "plan""#));
    assert!(contents.contains("[providers.deepseek]"));
    assert!(!contents.contains("api_key"));

    let cli = CliOverrides {
        config_path: Some(path),
        ..CliOverrides::default()
    };
    let config = load_config_with_env(&root, &cli, &|_| None).unwrap();
    assert_eq!(config.provider, "deepseek");
    assert_eq!(config.model.as_deref(), Some("deepseek-v4-flash"));
    assert_eq!(config.permission_mode, PermissionMode::Plan);
    assert_eq!(
        config.api_base.as_deref(),
        Some("https://api.deepseek.example")
    );
}

#[test]
fn save_user_provider_config_updates_scoped_provider_fields() {
    let root =
        std::env::temp_dir().join(format!("viden_save_provider_config_{}", std::process::id()));
    let path = root.join("config.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        &path,
        r#"
provider = "deepseek"
permission_mode = "plan"

[providers.deepseek]
api_base = "https://old.example"
"#,
    )
    .unwrap();

    save_user_provider_config_at(
        &path,
        "deepseek",
        ProviderConfigUpdate {
            api_base: Some("https://api.deepseek.com".to_string()),
            api_key_env: Some("DEEPSEEK_API_KEY".to_string()),
            default_model: Some("deepseek-v4-pro".to_string()),
            models: None,
            favorite_models: None,
        },
    )
    .unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains(r#"provider = "deepseek""#));
    assert!(contents.contains(r#"permission_mode = "plan""#));
    assert!(contents.contains("[providers.deepseek]"));
    assert!(contents.contains(r#"api_base = "https://api.deepseek.com""#));
    assert!(contents.contains(r#"api_key_env = "DEEPSEEK_API_KEY""#));
    assert!(contents.contains(r#"default_model = "deepseek-v4-pro""#));
    assert!(!contents.contains("api_key ="));

    let cli = CliOverrides {
        config_path: Some(path),
        provider: Some("deepseek".to_string()),
        model: None,
        ..CliOverrides::default()
    };
    let env_map = map_env(&[("DEEPSEEK_API_KEY", "deepseek-provider-key")]);
    let config = load_config_with_env(&root, &cli, &|key| env_map.get(key).cloned()).unwrap();

    assert_eq!(config.api_base.as_deref(), Some("https://api.deepseek.com"));
    assert_eq!(config.api_key.as_deref(), Some("deepseek-provider-key"));
    assert_eq!(config.model.as_deref(), Some("deepseek-v4-pro"));
}

#[test]
fn save_user_provider_config_rejects_empty_updates() {
    let root = std::env::temp_dir().join(format!(
        "viden_save_provider_config_empty_{}",
        std::process::id()
    ));
    let path = root.join("config.toml");
    let _ = fs::remove_dir_all(&root);

    let err = save_user_provider_config_at(
        &path,
        "deepseek",
        ProviderConfigUpdate {
            api_base: Some(" ".to_string()),
            ..ProviderConfigUpdate::default()
        },
    )
    .unwrap_err();

    assert!(err.contains("API base cannot be empty"));
}

#[test]
fn add_user_provider_favorite_model_moves_unique_model_to_front() {
    let root = std::env::temp_dir().join(format!(
        "viden_favorite_provider_model_{}",
        std::process::id()
    ));
    let path = root.join("config.toml");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        &path,
        r#"
[providers.deepseek]
models = ["deepseek-v4-flash", "deepseek-v4-pro"]
favorite_models = ["deepseek-v4-flash"]
"#,
    )
    .unwrap();

    add_user_provider_favorite_model_at(&path, "deepseek", "deepseek-v4-pro").unwrap();

    let config = load_provider_ui_config_at(&path).unwrap();
    let deepseek = config.get("deepseek").unwrap();
    assert_eq!(
        deepseek.models,
        vec![
            "deepseek-v4-flash".to_string(),
            "deepseek-v4-pro".to_string()
        ]
    );
    assert_eq!(
        deepseek.favorite_models,
        vec![
            "deepseek-v4-pro".to_string(),
            "deepseek-v4-flash".to_string()
        ]
    );
}

#[test]
fn deepseek_provider_scoped_config_from_global_applies_after_project_provider_selection() {
    let root = std::env::temp_dir().join(format!("viden_deepseek_global_{}", std::process::id()));
    let global_config_path = default_config_path_for_test(&root);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(global_config_path.parent().unwrap()).unwrap();
    fs::create_dir_all(root.join("project").join(".viden")).unwrap();
    fs::write(
        &global_config_path,
        r#"
[providers.deepseek]
api_base = "https://global-provider.example"
"#,
    )
    .unwrap();
    fs::write(
        root.join("project").join(".viden").join("config.toml"),
        r#"
provider = "deepseek"
api_base = "https://generic-project.example"
"#,
    )
    .unwrap();

    let env_map = map_env(&[("HOME", root.to_string_lossy().as_ref())]);
    let config = load_config_with_env(&root.join("project"), &CliOverrides::default(), &|key| {
        env_map.get(key).cloned()
    })
    .unwrap();

    assert_eq!(config.provider, "deepseek");
    assert_eq!(
        config.api_base.as_deref(),
        Some("https://global-provider.example")
    );
}

#[test]
fn deepseek_provider_scoped_config_applies_when_provider_is_selected_by_cli() {
    let root = std::env::temp_dir().join(format!("viden_deepseek_cli_{}", std::process::id()));
    let global_config_path = default_config_path_for_test(&root);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(global_config_path.parent().unwrap()).unwrap();
    fs::create_dir_all(root.join("project").join(".viden")).unwrap();
    fs::write(
        &global_config_path,
        r#"
[providers.deepseek]
api_base = "https://global-provider.example"
"#,
    )
    .unwrap();

    let env_map = map_env(&[("HOME", root.to_string_lossy().as_ref())]);
    let cli = CliOverrides {
        provider: Some("deepseek".to_string()),
        ..CliOverrides::default()
    };
    let config = load_config_with_env(&root.join("project"), &cli, &|key| {
        env_map.get(key).cloned()
    })
    .unwrap();

    assert_eq!(config.provider, "deepseek");
    assert_eq!(
        config.api_base.as_deref(),
        Some("https://global-provider.example")
    );
}

#[test]
fn deepseek_anthropic_uses_deepseek_scoped_config() {
    let root =
        std::env::temp_dir().join(format!("viden_deepseek_anthropic_{}", std::process::id()));
    let global_config_path = default_config_path_for_test(&root);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(global_config_path.parent().unwrap()).unwrap();
    fs::create_dir_all(root.join("project").join(".viden")).unwrap();
    fs::write(
        &global_config_path,
        r#"
[providers.deepseek]
api_base = "https://api.deepseek.com/anthropic"
api_key_env = "DEEPSEEK_API_KEY"
"#,
    )
    .unwrap();

    let env_map = map_env(&[
        ("HOME", root.to_string_lossy().as_ref()),
        ("DEEPSEEK_API_KEY", "deepseek-provider-key"),
    ]);
    let cli = CliOverrides {
        provider: Some("deepseek-anthropic".to_string()),
        ..CliOverrides::default()
    };
    let config = load_config_with_env(&root.join("project"), &cli, &|key| {
        env_map.get(key).cloned()
    })
    .unwrap();

    assert_eq!(config.provider, "deepseek-anthropic");
    assert_eq!(
        config.api_base.as_deref(),
        Some("https://api.deepseek.com/anthropic")
    );
    assert_eq!(config.api_key.as_deref(), Some("deepseek-provider-key"));
}

#[test]
fn arbitrary_provider_scoped_config_applies_to_selected_provider() {
    let root = std::env::temp_dir().join(format!("viden_openrouter_scoped_{}", std::process::id()));
    let global_config_path = default_config_path_for_test(&root);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(global_config_path.parent().unwrap()).unwrap();
    fs::create_dir_all(root.join("project").join(".viden")).unwrap();
    fs::write(
        &global_config_path,
        r#"
[providers.openrouter]
api_base = "https://openrouter.ai/api/v1"
api_key_env = "OPENROUTER_API_KEY"
default_model = "openai/gpt-5.2"
"#,
    )
    .unwrap();

    let env_map = map_env(&[
        ("HOME", root.to_string_lossy().as_ref()),
        ("OPENROUTER_API_KEY", "openrouter-key"),
    ]);
    let cli = CliOverrides {
        provider: Some("openrouter".to_string()),
        ..CliOverrides::default()
    };
    let config = load_config_with_env(&root.join("project"), &cli, &|key| {
        env_map.get(key).cloned()
    })
    .unwrap();

    assert_eq!(config.provider, "openrouter");
    assert_eq!(
        config.api_base.as_deref(),
        Some("https://openrouter.ai/api/v1")
    );
    assert_eq!(config.api_key.as_deref(), Some("openrouter-key"));
    assert_eq!(config.model.as_deref(), Some("openai/gpt-5.2"));
}

#[test]
fn provider_specific_env_uses_normalized_provider_name() {
    let cwd = std::env::temp_dir().join(format!("viden_openrouter_env_{}", std::process::id()));
    let _ = fs::remove_dir_all(&cwd);
    fs::create_dir_all(cwd.join(".viden")).unwrap();
    fs::write(
        cwd.join(".viden").join("config.toml"),
        r#"
provider = "openrouter"
api_key = "generic-key"
api_base = "https://generic.example"
"#,
    )
    .unwrap();

    let env_map = map_env(&[
        ("OPENROUTER_API_KEY", "provider-key"),
        ("OPENROUTER_API_BASE", "https://provider.example"),
    ]);
    let config = load_config_with_env(&cwd, &CliOverrides::default(), &|key| {
        env_map.get(key).cloned()
    })
    .unwrap();

    assert_eq!(config.provider, "openrouter");
    assert_eq!(config.api_key.as_deref(), Some("provider-key"));
    assert_eq!(config.api_base.as_deref(), Some("https://provider.example"));
}

#[test]
fn provider_specific_env_uses_shared_family_aliases() {
    let cwd = std::env::temp_dir().join(format!(
        "viden_deepseek_anthropic_env_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&cwd);
    fs::create_dir_all(cwd.join(".viden")).unwrap();
    fs::write(
        cwd.join(".viden").join("config.toml"),
        r#"
provider = "deepseek-anthropic"
"#,
    )
    .unwrap();

    let env_map = map_env(&[
        ("DEEPSEEK_API_KEY", "provider-key"),
        ("DEEPSEEK_API_BASE", "https://api.deepseek.com/anthropic"),
    ]);
    let config = load_config_with_env(&cwd, &CliOverrides::default(), &|key| {
        env_map.get(key).cloned()
    })
    .unwrap();

    assert_eq!(config.provider, "deepseek-anthropic");
    assert_eq!(config.api_key.as_deref(), Some("provider-key"));
    assert_eq!(
        config.api_base.as_deref(),
        Some("https://api.deepseek.com/anthropic")
    );
}

// --- ui.layout_preferences (C5) ----------------------------------------------

fn layout_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("viden_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

/// The layout record persists under `[ui.layout]` and leaves every other key
/// in the file — including the appearance profile beside it — untouched.
#[test]
fn ui_layout_write_patch_preserves_the_rest_of_the_file() {
    let path = layout_root("ui_layout_write").join("config.toml");
    fs::write(
        &path,
        "custom = 7\n\n[ui]\nlocale = \"en\"\nskin = \"ice\"\n",
    )
    .unwrap();

    // Pinned, not floating: floating is the default, so writing it would not
    // tell a stored choice apart from an absent one.
    let state = save_user_ui_layout_preferences_at(
        &path,
        &UiLayoutPreferencePatch {
            lane_sidebar_mode: Some(LaneSidebarMode::Pinned),
            hidden_statusbar_segments: Some(vec!["cost".to_string()]),
        },
    )
    .unwrap();

    assert!(state.persisted);
    assert_eq!(state.preferences.lane_sidebar_mode, LaneSidebarMode::Pinned);
    let value = fs::read_to_string(&path)
        .unwrap()
        .parse::<toml::Value>()
        .unwrap();
    assert_eq!(
        value.get("custom").and_then(toml::Value::as_integer),
        Some(7)
    );
    assert_eq!(
        value
            .get("ui")
            .and_then(|ui| ui.get("skin"))
            .and_then(toml::Value::as_str),
        Some("ice")
    );
    assert_eq!(
        value
            .get("ui")
            .and_then(|ui| ui.get("layout"))
            .and_then(|layout| layout.get("lane_sidebar_mode"))
            .and_then(toml::Value::as_str),
        Some("pinned")
    );

    let resolved = resolve_user_ui_layout_preferences_at(&path).unwrap();
    assert_eq!(
        resolved.preferences.lane_sidebar_mode,
        LaneSidebarMode::Pinned
    );
    assert_eq!(
        resolved.preferences.hidden_statusbar_segments,
        vec!["cost".to_string()]
    );
}

/// A patch sets only the fields it names. Omitting a field means "leave it",
/// never "reset it": a client that only changes the sidebar must not silently
/// unhide every statusbar segment the operator hid.
#[test]
fn ui_layout_patch_leaves_unnamed_fields_alone() {
    let path = layout_root("ui_layout_partial").join("config.toml");
    save_user_ui_layout_preferences_at(
        &path,
        &UiLayoutPreferencePatch {
            lane_sidebar_mode: Some(LaneSidebarMode::Pinned),
            hidden_statusbar_segments: Some(vec!["cost".to_string(), "lsp".to_string()]),
        },
    )
    .unwrap();

    let state = save_user_ui_layout_preferences_at(
        &path,
        &UiLayoutPreferencePatch {
            lane_sidebar_mode: Some(LaneSidebarMode::Floating),
            hidden_statusbar_segments: None,
        },
    )
    .unwrap();

    assert_eq!(
        state.preferences.lane_sidebar_mode,
        LaneSidebarMode::Floating
    );
    assert_eq!(
        state.preferences.hidden_statusbar_segments,
        vec!["cost".to_string(), "lsp".to_string()]
    );
}

/// Resetting the appearance profile must not reset the cockpit layout. The two
/// are separate records with separate commands; collapsing them because
/// `[ui.layout]` happens to live under `[ui]` would make an appearance reset
/// silently rearrange the operator's workspace.
#[test]
fn resetting_appearance_preferences_keeps_the_layout_record() {
    let path = layout_root("ui_layout_reset_isolation").join("config.toml");
    fs::write(&path, "[ui]\nskin = \"ice\"\n").unwrap();
    // Pinned, the non-default mode: reading floating back afterwards would be
    // indistinguishable from the record having been dropped.
    save_user_ui_layout_preferences_at(
        &path,
        &UiLayoutPreferencePatch {
            lane_sidebar_mode: Some(LaneSidebarMode::Pinned),
            hidden_statusbar_segments: None,
        },
    )
    .unwrap();

    reset_user_ui_preferences_at(&path, None, UiPreferences::client_default()).unwrap();

    let value = fs::read_to_string(&path)
        .unwrap()
        .parse::<toml::Value>()
        .unwrap();
    assert!(
        value.get("ui").and_then(|ui| ui.get("skin")).is_none(),
        "the appearance profile must be gone"
    );
    let resolved = resolve_user_ui_layout_preferences_at(&path).unwrap();
    assert_eq!(
        resolved.preferences.lane_sidebar_mode,
        LaneSidebarMode::Pinned
    );
}

/// Resetting the layout record removes `[ui.layout]` and republishes the
/// defaults, leaving the appearance profile in place.
#[test]
fn resetting_the_layout_record_keeps_appearance_preferences() {
    let path = layout_root("ui_layout_reset").join("config.toml");
    fs::write(&path, "[ui]\nskin = \"ice\"\n").unwrap();
    save_user_ui_layout_preferences_at(
        &path,
        &UiLayoutPreferencePatch {
            lane_sidebar_mode: Some(LaneSidebarMode::Pinned),
            hidden_statusbar_segments: Some(vec!["cost".to_string()]),
        },
    )
    .unwrap();

    let state = reset_user_ui_layout_preferences_at(&path).unwrap();

    assert_eq!(state.preferences, UiLayoutPreferences::default());
    // Spelled out as well as compared to `default()`: a reset lands on
    // floating per `D-SIDEBAR`, not on whatever was last written.
    assert_eq!(
        state.preferences.lane_sidebar_mode,
        LaneSidebarMode::Floating
    );
    assert!(state.persisted);
    let value = fs::read_to_string(&path)
        .unwrap()
        .parse::<toml::Value>()
        .unwrap();
    assert!(value.get("ui").and_then(|ui| ui.get("layout")).is_none());
    assert_eq!(
        value
            .get("ui")
            .and_then(|ui| ui.get("skin"))
            .and_then(toml::Value::as_str),
        Some("ice")
    );
}

/// A stored value this build cannot name is reported as a diagnostic and
/// replaced by the default for that field alone. Refusing to load the whole
/// record would strand every other layout choice over one bad key.
#[test]
fn an_unreadable_layout_value_falls_back_with_a_diagnostic() {
    let path = layout_root("ui_layout_invalid").join("config.toml");
    fs::write(
        &path,
        "[ui.layout]\nlane_sidebar_mode = \"hovering\"\nhidden_statusbar_segments = [\"cost\"]\n",
    )
    .unwrap();

    let state = resolve_user_ui_layout_preferences_at(&path).unwrap();

    assert_eq!(
        state.preferences.lane_sidebar_mode,
        LaneSidebarMode::Floating
    );
    assert_eq!(
        state.preferences.hidden_statusbar_segments,
        vec!["cost".to_string()]
    );
    assert_eq!(state.diagnostics.len(), 1);
    assert_eq!(state.diagnostics[0].code, "ui.layout.invalid_value");
    assert_eq!(
        state.diagnostics[0].rejected_value.as_deref(),
        Some("hovering")
    );
}

/// An over-bound patch is refused before anything is written, so the file on
/// disk still holds exactly what it held.
#[test]
fn an_over_bound_layout_patch_is_refused_without_changing_bytes() {
    let path = layout_root("ui_layout_over_bound").join("config.toml");
    fs::write(&path, "custom = 7\n").unwrap();
    let before = fs::read_to_string(&path).unwrap();

    let error = save_user_ui_layout_preferences_at(
        &path,
        &UiLayoutPreferencePatch {
            lane_sidebar_mode: None,
            hidden_statusbar_segments: Some(
                (0..=viden_types::MAX_HIDDEN_STATUSBAR_SEGMENTS)
                    .map(|index| format!("segment_{index}"))
                    .collect(),
            ),
        },
    )
    .expect_err("an over-bound patch must be refused");

    assert!(error.contains("16"));
    assert_eq!(fs::read_to_string(&path).unwrap(), before);
}

// --- runtime.workspace_owner: the durable project id (C5) --------------------

/// A project with no id gets one minted and written, and the next open reads
/// exactly that id back. The origin says which of the two happened, because an
/// operator seeing a new project id in an audit trail must be able to tell
/// "Core minted this" from "this project always had it" without diffing a file
/// Core owns.
#[test]
fn a_project_id_is_minted_once_and_read_back_afterwards() {
    let root = layout_root("project_id_mint");

    let (minted, origin) = read_or_mint_project_id_at(&root).unwrap();

    assert_eq!(origin, ProjectIdOrigin::Minted);
    assert!(minted.starts_with(viden_types::PROJECT_ID_PREFIX));
    assert!(root.join(".viden").join("project.toml").is_file());

    let (existing, origin) = read_or_mint_project_id_at(&root).unwrap();
    assert_eq!(origin, ProjectIdOrigin::Existing);
    assert_eq!(existing, minted);
}

/// The write preserves every other key in `project.toml`, and an id already in
/// the file is never rewritten: the project id is durable identity, so minting
/// a second one would split one project's audit history in two.
#[test]
fn an_existing_project_id_is_preserved_with_the_rest_of_the_file() {
    let root = layout_root("project_id_existing");
    let viden = root.join(".viden");
    fs::create_dir_all(&viden).unwrap();
    fs::write(
        viden.join("project.toml"),
        "custom = 7\n\n[project]\nid = \"prj_existing\"\nlabel = \"keep-me\"\n",
    )
    .unwrap();

    let (id, origin) = read_or_mint_project_id_at(&root).unwrap();

    assert_eq!(id, "prj_existing");
    assert_eq!(origin, ProjectIdOrigin::Existing);
    let contents = fs::read_to_string(viden.join("project.toml")).unwrap();
    assert!(contents.contains("custom = 7"));
    assert!(contents.contains("keep-me"));
}

/// An id this build cannot use — empty, or not a string — is replaced by a
/// freshly minted one rather than published as identity. An unusable id in an
/// owner is the `RuntimeOwner::default()` failure wearing a different spelling.
#[test]
fn an_unusable_stored_project_id_is_replaced_by_a_minted_one() {
    let root = layout_root("project_id_invalid");
    let viden = root.join(".viden");
    fs::create_dir_all(&viden).unwrap();
    fs::write(viden.join("project.toml"), "[project]\nid = 7\n").unwrap();

    let (id, origin) = read_or_mint_project_id_at(&root).unwrap();

    assert_eq!(origin, ProjectIdOrigin::Minted);
    assert!(id.starts_with(viden_types::PROJECT_ID_PREFIX));
    let (second, origin) = read_or_mint_project_id_at(&root).unwrap();
    assert_eq!(origin, ProjectIdOrigin::Existing);
    assert_eq!(second, id);
}

/// A config file with no `[ui.layout]` table at all resolves to the floating
/// default (`D-SIDEBAR`), with no diagnostic: an absent table is not a
/// malformed one.
#[test]
fn an_absent_layout_table_resolves_to_the_floating_default() {
    let path = layout_root("ui_layout_absent").join("config.toml");
    fs::write(&path, "custom = 7\n[ui]\nskin = \"ice\"\n").unwrap();

    let state = resolve_user_ui_layout_preferences_at(&path).unwrap();

    assert_eq!(state.preferences, UiLayoutPreferences::default());
    assert_eq!(
        state.preferences.lane_sidebar_mode,
        LaneSidebarMode::Floating
    );
    assert!(state.diagnostics.is_empty());
}
