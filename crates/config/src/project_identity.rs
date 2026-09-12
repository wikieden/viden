//! The durable project id (`runtime.workspace_owner`, GUI-CORE-027).
//!
//! A [`viden_types::RuntimeOwner`] carries two identities. `workspace_id`
//! names a *location* and is derived from the canonical root, so it needs no
//! store and changes when the repository moves. `project_id` is the half that
//! must survive that move, so it is written once into the project's own
//! Core-owned state directory (`.viden/project.toml`) and read back on every
//! later open.
//!
//! Two rules:
//!
//! 1. **Minted once, never rewritten.** An id already in the file is returned
//!    unchanged. Minting a second one would split a single project's audit
//!    history into two identities with nothing to join them on.
//! 2. **An unusable id is not an id.** An empty or non-string value is
//!    replaced by a fresh minted one rather than published, because an owner
//!    carrying an unusable project id is the "authorized mutation belonging to
//!    nobody" failure wearing a different spelling.

use std::fs;
use std::path::{Path, PathBuf};

use toml::{Value, map::Map};
use viden_types::{PROJECT_ID_PREFIX, ProjectIdOrigin, fresh_id};

use crate::ui_preferences::atomic_write_config;

/// Core's project-local state directory, relative to the workspace root.
const PROJECT_STATE_DIR: &str = ".viden";
const PROJECT_IDENTITY_FILE: &str = "project.toml";

/// Reads the project's durable id from `<root>/.viden/project.toml`, minting
/// and writing one when the file has none this build can use.
pub fn read_or_mint_project_id_at(root: &Path) -> Result<(String, ProjectIdOrigin), String> {
    let path = project_identity_path(root);
    let mut value = read_project_identity(&path)?;
    if let Some(existing) = stored_project_id(&value) {
        return Ok((existing, ProjectIdOrigin::Existing));
    }

    let minted = fresh_id(PROJECT_ID_PREFIX.trim_end_matches('_'));
    let table = value
        .as_table_mut()
        .ok_or_else(|| format!("{} must be a TOML table", path.display()))?;
    let project = table
        .entry("project".to_string())
        .or_insert_with(|| Value::Table(Map::new()))
        .as_table_mut()
        .ok_or_else(|| format!("{} [project] must be a TOML table", path.display()))?;
    project.insert("id".to_string(), Value::String(minted.clone()));
    // The same atomic temp-file-then-rename write the preference files use, so
    // a crash mid-write cannot leave a project with a half-written identity.
    atomic_write_config(&path, &value, None)?;
    Ok((minted, ProjectIdOrigin::Minted))
}

/// Where the durable project identity lives for a workspace root.
pub fn project_identity_path(root: &Path) -> PathBuf {
    root.join(PROJECT_STATE_DIR).join(PROJECT_IDENTITY_FILE)
}

fn read_project_identity(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Ok(Value::Table(Map::new()));
    }
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
    if contents.trim().is_empty() {
        return Ok(Value::Table(Map::new()));
    }
    contents
        .parse::<Value>()
        .map_err(|error| format!("Failed to parse {}: {error}", path.display()))
}

fn stored_project_id(value: &Value) -> Option<String> {
    let id = value.get("project")?.get("id")?.as_str()?.trim();
    if id.is_empty() {
        return None;
    }
    Some(id.to_string())
}
