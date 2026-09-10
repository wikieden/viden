use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

const FORBIDDEN_RUNTIME_TYPES: [&str; 2] = ["SessionEngine", "RuntimeSupervisor"];
const PRESENTATION_MODULES: [&str; 4] = [
    "presentation/preferences.rs",
    "presentation/workspace.rs",
    "presentation/composer.rs",
    "presentation/transcript.rs",
];

fn gui_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri should live below apps/gui")
        .to_path_buf()
}

fn rust_sources(root: &Path) -> Vec<PathBuf> {
    fn collect(path: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(path).expect("read GUI Rust source directory") {
            let path = entry.expect("read GUI Rust source entry").path();
            if path.is_dir() {
                collect(&path, files);
            } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    collect(root, &mut files);
    files
}

#[test]
fn production_crate_has_an_explicit_version_and_only_core_as_a_viden_dependency() {
    let manifest_path = gui_root().join("src-tauri/Cargo.toml");
    let manifest: toml::Value = fs::read_to_string(manifest_path)
        .expect("read production GUI manifest")
        .parse()
        .expect("parse production GUI manifest");

    assert_eq!(manifest["package"]["version"].as_str(), Some("0.1.0-rc.4"));
    let mut internal = Vec::new();
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(table) = manifest.get(section).and_then(toml::Value::as_table) {
            for (name, specification) in table {
                let package = specification
                    .as_table()
                    .and_then(|value| value.get("package"))
                    .and_then(toml::Value::as_str)
                    .unwrap_or(name);
                if package.starts_with("viden-") {
                    internal.push(package.to_string());
                }
            }
        }
    }
    internal.sort();
    assert_eq!(internal, vec!["viden-core"]);
}

#[test]
fn rust_web_and_tauri_packages_share_the_explicit_rc_version() {
    let gui_root = gui_root();
    let package: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(gui_root.join("package.json")).expect("read GUI package.json"),
    )
    .expect("parse GUI package.json");
    let tauri: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(gui_root.join("src-tauri/tauri.conf.json"))
            .expect("read Tauri configuration"),
    )
    .expect("parse Tauri configuration");

    assert_eq!(package["version"].as_str(), Some("0.1.0-rc.4"));
    assert_eq!(tauri["version"].as_str(), Some("0.1.0-rc.4"));
}

#[test]
fn only_adapter_and_projection_hold_core_contract_types() {
    let source_root = gui_root().join("src-tauri/src");
    for required in ["adapter.rs", "projection.rs"] {
        assert!(source_root.join(required).is_file(), "missing {required}");
    }
    for required in PRESENTATION_MODULES {
        assert!(source_root.join(required).is_file(), "missing {required}");
    }

    for path in rust_sources(&source_root) {
        let source = fs::read_to_string(&path).expect("read GUI Rust source");
        let relative = path
            .strip_prefix(&source_root)
            .expect("relative GUI source path");
        for forbidden in FORBIDDEN_RUNTIME_TYPES {
            assert!(
                !source.contains(forbidden),
                "{} imports forbidden runtime type {forbidden}",
                relative.display()
            );
        }
        if source.contains("viden_core") {
            assert!(
                relative == Path::new("adapter.rs")
                    || relative == Path::new("d4.rs")
                    || relative == Path::new("projection.rs"),
                "{} holds Core types outside the adapter/projection boundary",
                relative.display()
            );
        }
    }
}

#[test]
fn root_workspace_contains_only_the_selected_tauri_production_crate() {
    let gui_root = gui_root();
    let repository_root = gui_root
        .parent()
        .and_then(Path::parent)
        .expect("repository root");
    let workspace: toml::Value = fs::read_to_string(repository_root.join("Cargo.toml"))
        .expect("read root workspace manifest")
        .parse()
        .expect("parse root workspace manifest");
    let members = workspace["workspace"]["members"]
        .as_array()
        .expect("workspace members")
        .iter()
        .filter_map(toml::Value::as_str)
        .filter(|member| member.starts_with("apps/gui"))
        .collect::<Vec<_>>();

    assert_eq!(members, vec!["apps/gui/src-tauri"]);
}

#[test]
fn rc_release_manifest_is_an_immutable_byte_equivalent_snapshot() {
    let gui_root = gui_root();
    let active_path = gui_root.join("release-manifest.toml");
    let snapshot_path = gui_root.join("manifests/0.1.0-rc.4.toml");
    let active = fs::read(&active_path).expect("read active GUI release manifest");
    let snapshot = fs::read(&snapshot_path).expect("read immutable beta release manifest");

    assert_eq!(
        active, snapshot,
        "RC manifest snapshot must be byte-equivalent"
    );
    let manifest: toml::Value = String::from_utf8(active)
        .expect("release manifest must be UTF-8")
        .parse()
        .expect("parse GUI release manifest");
    assert_eq!(manifest["component_version"].as_str(), Some("0.1.0-rc.4"));
    assert_eq!(manifest["release_channel"].as_str(), Some("rc"));
    assert_eq!(
        manifest["status"].as_str(),
        Some("trusted-delivery-beta-candidate")
    );
    assert_eq!(manifest["selected_framework"].as_str(), Some("tauri"));
    assert_eq!(manifest["core"]["minimum_version"].as_str(), Some("0.3.5"));
    assert_eq!(
        manifest["core"]["base_checkpoint"].as_str(),
        Some("f7fe1b31dfb237e4062209767a7051c2b2c68b93")
    );
    // Derived, not a literal: this digest describes the exact bytes of the
    // canonical fixture the manifest itself names. The literal it replaces was
    // last true before `d1-main-cockpit.json` was edited, and nothing noticed,
    // because a hand-maintained pin proves only that someone typed it once.
    let repository_root = gui_root
        .parent()
        .and_then(Path::parent)
        .expect("repository root");
    let canonical_fixture = repository_root.join(
        manifest["core"]["canonical_d1_fixture"]
            .as_str()
            .expect("canonical D1 fixture path"),
    );
    let canonical_bytes =
        fs::read(&canonical_fixture).expect("read the canonical D1 fixture the manifest names");
    assert_eq!(
        manifest["core"]["extension_fixture_sha256"].as_str(),
        Some(format!("{:x}", Sha256::digest(&canonical_bytes)).as_str())
    );
    let required = manifest["core"]["required_capabilities"]
        .as_array()
        .expect("required Core capabilities")
        .iter()
        .filter_map(toml::Value::as_str)
        .collect::<Vec<_>>();
    for capability in ["runtime.commands", "runtime.snapshot", "ui.preferences"] {
        assert!(required.contains(&capability), "missing {capability}");
    }
    for capability in [
        "runtime.credential_handles",
        "runtime.lane_lifecycle",
        "runtime.project_onboarding",
        "runtime.cockpit_context_v1",
    ] {
        assert!(
            !required.contains(&capability),
            "extension capability {capability} must not block base startup"
        );
    }
    let feature_gated = manifest["core"]["feature_capabilities"]
        .as_array()
        .expect("feature-gated Core capabilities")
        .iter()
        .filter_map(toml::Value::as_str)
        .collect::<Vec<_>>();
    for capability in [
        "runtime.project_onboarding",
        "runtime.starter_lane_preview",
        "runtime.lane_owner_projection",
        "runtime.agent_adapters",
        "runtime.agent_session_input",
        "runtime.agent_sessions",
        "runtime.workspace_eligibility",
        "ui.preference_persistence",
    ] {
        assert!(feature_gated.contains(&capability), "missing {capability}");
    }
    assert_eq!(
        manifest["evidence"]["root"].as_str(),
        Some("apps/gui/evidence/0.1.0-rc.3")
    );
    assert_eq!(
        manifest["evidence"]["accessibility"].as_str(),
        Some("apps/gui/evidence/0.1.0-rc.3/accessibility.json")
    );
    assert_eq!(
        manifest["evidence"]["performance"].as_str(),
        Some("apps/gui/evidence/0.1.0-rc.3/performance.json")
    );
    assert_eq!(
        manifest["evidence"]["design_reference"].as_str(),
        Some("apps/gui/evidence/0.1.0-rc.3/d1-design-reference.html")
    );
    assert_eq!(
        manifest["evidence"]["same_state_comparison"].as_str(),
        Some("apps/gui/evidence/0.1.0-rc.3/d1-design-reference-vs-actual.png")
    );
    assert_eq!(
        manifest["evidence"]["context_dock_bottom_capture"].as_str(),
        Some("apps/gui/evidence/0.1.0-rc.3/d1-context-dock-bottom-1280x800.png")
    );
    assert_eq!(
        manifest["evidence"]["accepted_target_role"].as_str(),
        Some("historical visual reference only; not same-state acceptance evidence")
    );
    let required_ids = manifest["fixtures"]["required_ids"]
        .as_array()
        .expect("fixture ids")
        .iter()
        .filter_map(toml::Value::as_str)
        .collect::<Vec<_>>();
    assert!(required_ids.contains(&"d1-main-cockpit"));
    // The four `0.3.3` contract fixtures the GUI replays in its own suites
    // (workspace_diff, operator_git, d12_integration_gate, evidence_reads).
    for id in [
        "structured-diff",
        "operator-git",
        "conflict-content",
        "evidence-reads",
    ] {
        assert!(required_ids.contains(&id), "missing required fixture {id}");
    }
    // Every id the manifest requires must name a fixture that exists, so a
    // renamed or dropped fixture fails the release record rather than the
    // suite that happens to read it.
    let corpus = repository_root.join(
        manifest["fixtures"]["corpus"]
            .as_str()
            .expect("fixture corpus path"),
    );
    for id in &required_ids {
        assert!(
            corpus.join(format!("{id}.json")).exists(),
            "required fixture {id} is not in the corpus"
        );
    }
}

#[test]
fn standalone_binary_must_not_launch_with_an_empty_core_adapter() {
    let source = fs::read_to_string(gui_root().join("src-tauri/src/lib.rs"))
        .expect("read desktop bootstrap source");

    assert!(
        !source.contains("run_with_adapter(None)"),
        "the standalone App cannot enter D11/D4/D1 while its Core adapter is empty"
    );
}

#[test]
fn desktop_opens_projects_through_core_and_uses_integrated_macos_chrome() {
    let gui_root = gui_root();
    let source = fs::read_to_string(gui_root.join("src-tauri/src/lib.rs"))
        .expect("read desktop bootstrap source");
    assert!(source.contains("fn open_workspace("));
    assert!(source.contains("open_local_workspace(&root)"));
    assert!(source.contains("open_workspace,"));

    let adapter = fs::read_to_string(gui_root.join("src-tauri/src/adapter.rs"))
        .expect("read GUI adapter source");
    assert!(!adapter.contains("std::env::current_dir()"));
    assert!(!adapter.contains("std::env::current_exe()"));

    let config: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(gui_root.join("src-tauri/tauri.conf.json"))
            .expect("read Tauri configuration"),
    )
    .expect("parse Tauri configuration");
    let window = &config["app"]["windows"][0];
    assert_eq!(window["titleBarStyle"].as_str(), Some("Overlay"));
    assert_eq!(window["hiddenTitle"].as_bool(), Some(true));
    assert_eq!(window["decorations"].as_bool(), Some(true));
    assert_eq!(window["backgroundColor"].as_str(), Some("#070c12"));

    let capability: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(gui_root.join("src-tauri/capabilities/main.json"))
            .expect("read main-window capability"),
    )
    .expect("parse main-window capability");
    assert_eq!(
        capability["permissions"],
        serde_json::json!(["core:default", "dialog:allow-open"])
    );
}
