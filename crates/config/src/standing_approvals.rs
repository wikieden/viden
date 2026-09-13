//! Standing `Always` approvals in the user-level settings file
//! (`runtime.approval_scopes`, GUI-CORE-019).
//!
//! The file is `settings.toml` in the Viden home — `$VIDEN_HOME/settings.toml`,
//! otherwise `~/.viden/settings.toml` (`%APPDATA%\viden\settings.toml` on
//! Windows). The table is `[[permissions.standing]]`:
//!
//! ```toml
//! [[permissions.standing]]
//! rule_id = "rule_01J..."
//! tool_name = "shell"
//! rule_content = "command=cargo fmt --all"
//! granted_at = 1_700_000_000
//! granted_in_workspace = "workspace_..."
//! audit_id = "audit_01J..."
//! ```
//!
//! A standing rule is the one permission fact that outlives the process and is
//! not scoped to a workspace, so it lives beside the other user-level settings
//! rather than in project or session state. Every function here is total over
//! a malformed file: an entry Core cannot read is skipped with a diagnostic
//! rather than taking the whole table down, because a single bad row must not
//! silently re-enable prompts the operator believes they silenced — or worse,
//! leave a rule in force that cannot be revoked.

use std::path::{Path, PathBuf};

use toml::{Value, map::Map};
use viden_types::StandingApprovalView;

use crate::{read_config_value_for_write, write_config_value};

const PERMISSIONS_KEY: &str = "permissions";
const STANDING_KEY: &str = "standing";

/// The user-level settings file standing approvals are written to, given the
/// Viden home that owns it.
///
/// Resolved from the *home* rather than from the platform config directory on
/// purpose. A standing rule applies to every workspace, so it belongs beside
/// the other user-level Viden state rather than in a project file — and the
/// home is a value the caller holds, so a runtime started against a scratch
/// home writes there instead of the operator's real one. A path derived only
/// from process environment could not give that guarantee.
pub fn user_settings_path_in_home(home: &Path) -> PathBuf {
    home.join("settings.toml")
}

/// Reads the standing approvals in force.
///
/// Returns the rules plus one diagnostic per entry that could not be read. A
/// missing file is an empty list, not an error.
pub fn load_user_standing_approvals_at(
    path: &Path,
) -> Result<(Vec<StandingApprovalView>, Vec<String>), String> {
    let value = read_config_value_for_write(path)?;
    let mut rules = Vec::new();
    let mut diagnostics = Vec::new();
    let Some(entries) = value
        .get(PERMISSIONS_KEY)
        .and_then(|permissions| permissions.get(STANDING_KEY))
    else {
        return Ok((rules, diagnostics));
    };
    let Some(entries) = entries.as_array() else {
        diagnostics.push(format!(
            "standing approvals in {} must be an array of tables",
            path.display()
        ));
        return Ok((rules, diagnostics));
    };
    for (index, entry) in entries.iter().enumerate() {
        match standing_rule_from_value(entry) {
            Some(rule)
                if rules
                    .iter()
                    .any(|existing: &StandingApprovalView| existing.rule_id == rule.rule_id) =>
            {
                diagnostics.push(format!(
                    "standing approval {index} in {} repeats rule id `{}`",
                    path.display(),
                    rule.rule_id
                ));
            }
            Some(rule) => rules.push(rule),
            None => diagnostics.push(format!(
                "standing approval {index} in {} is missing a required field",
                path.display()
            )),
        }
    }
    Ok((rules, diagnostics))
}

/// Appends one standing approval, replacing any entry with the same rule id.
///
/// Write-then-announce: the caller publishes `StandingApprovalGranted` only
/// after this returns, so a client never shows a rule the file does not hold.
pub fn grant_user_standing_approval_at(
    path: &Path,
    rule: &StandingApprovalView,
) -> Result<(), String> {
    let mut value = read_config_value_for_write(path)?;
    let entries = standing_entries_mut(&mut value, path)?;
    entries.retain(|entry| {
        entry.get("rule_id").and_then(Value::as_str) != Some(rule.rule_id.as_str())
    });
    entries.push(standing_rule_to_value(rule));
    write_config_value(path, &value)
}

/// Removes one standing approval. Returns the rule that was in force, or
/// `None` when the id names nothing — which is the answer a revoke of an
/// already-revoked rule must give rather than an error.
pub fn revoke_user_standing_approval_at(
    path: &Path,
    rule_id: &str,
) -> Result<Option<StandingApprovalView>, String> {
    let mut value = read_config_value_for_write(path)?;
    let entries = standing_entries_mut(&mut value, path)?;
    let Some(index) = entries
        .iter()
        .position(|entry| entry.get("rule_id").and_then(Value::as_str) == Some(rule_id))
    else {
        return Ok(None);
    };
    let removed = entries.remove(index);
    write_config_value(path, &value)?;
    Ok(standing_rule_from_value(&removed))
}

fn standing_entries_mut<'a>(
    value: &'a mut Value,
    path: &Path,
) -> Result<&'a mut Vec<Value>, String> {
    let table = value
        .as_table_mut()
        .ok_or_else(|| format!("Config {} must be a TOML table", path.display()))?;
    let permissions = table
        .entry(PERMISSIONS_KEY.to_string())
        .or_insert_with(|| Value::Table(Map::new()))
        .as_table_mut()
        .ok_or_else(|| format!("`permissions` in {} must be a table", path.display()))?;
    permissions
        .entry(STANDING_KEY.to_string())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| {
            format!(
                "`permissions.standing` in {} must be an array of tables",
                path.display()
            )
        })
}

fn standing_rule_to_value(rule: &StandingApprovalView) -> Value {
    let mut entry = Map::new();
    entry.insert("rule_id".to_string(), Value::String(rule.rule_id.clone()));
    entry.insert(
        "tool_name".to_string(),
        Value::String(rule.tool_name.clone()),
    );
    entry.insert(
        "rule_content".to_string(),
        Value::String(rule.rule_content.clone()),
    );
    entry.insert(
        "granted_at".to_string(),
        Value::Integer(rule.granted_at.min(i64::MAX as u64) as i64),
    );
    entry.insert(
        "granted_in_workspace".to_string(),
        Value::String(rule.granted_in_workspace.clone()),
    );
    entry.insert("audit_id".to_string(), Value::String(rule.audit_id.clone()));
    Value::Table(entry)
}

fn standing_rule_from_value(entry: &Value) -> Option<StandingApprovalView> {
    let string = |key: &str| entry.get(key).and_then(Value::as_str).map(str::to_string);
    let rule_id = string("rule_id").filter(|value| !value.is_empty())?;
    let tool_name = string("tool_name").filter(|value| !value.is_empty())?;
    // The content may legitimately be empty: a rule with no content matches
    // every input of its tool, which is what `PermissionRuleValue` already
    // means by `None`.
    let rule_content = string("rule_content")?;
    Some(StandingApprovalView {
        rule_id,
        tool_name,
        rule_content,
        granted_at: entry
            .get("granted_at")
            .and_then(Value::as_integer)
            .unwrap_or_default()
            .max(0) as u64,
        granted_in_workspace: string("granted_in_workspace").unwrap_or_default(),
        audit_id: string("audit_id").unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("viden-standing-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn rule(rule_id: &str, tool_name: &str, content: &str) -> StandingApprovalView {
        StandingApprovalView {
            rule_id: rule_id.to_string(),
            tool_name: tool_name.to_string(),
            rule_content: content.to_string(),
            granted_at: 1_700_000_000,
            granted_in_workspace: "workspace_test".to_string(),
            audit_id: format!("audit_{rule_id}"),
        }
    }

    #[test]
    fn a_missing_settings_file_holds_no_standing_approvals() {
        let path = scratch("missing").join("settings.toml");

        let (rules, diagnostics) = load_user_standing_approvals_at(&path).unwrap();

        assert!(rules.is_empty());
        assert!(diagnostics.is_empty());
        assert!(!path.exists(), "reading must not create the file");
    }

    #[test]
    fn a_granted_rule_round_trips_and_is_revocable() {
        let path = scratch("round-trip").join("settings.toml");
        let first = rule("rule_one", "shell", "command=cargo fmt --all");
        let second = rule("rule_two", "read_file", "path=README.md");

        grant_user_standing_approval_at(&path, &first).unwrap();
        grant_user_standing_approval_at(&path, &second).unwrap();
        let (rules, diagnostics) = load_user_standing_approvals_at(&path).unwrap();

        assert_eq!(rules, vec![first.clone(), second.clone()]);
        assert!(diagnostics.is_empty());
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("[[permissions.standing]]"),
            "the table the contract names must be the one on disk"
        );

        let removed = revoke_user_standing_approval_at(&path, "rule_one").unwrap();

        assert_eq!(removed, Some(first));
        let (rules, _) = load_user_standing_approvals_at(&path).unwrap();
        assert_eq!(rules, vec![second]);
    }

    #[test]
    fn revoking_a_rule_that_is_not_in_force_removes_nothing() {
        let path = scratch("revoke-unknown").join("settings.toml");
        let existing = rule("rule_one", "shell", "command=cargo fmt --all");
        grant_user_standing_approval_at(&path, &existing).unwrap();

        assert_eq!(
            revoke_user_standing_approval_at(&path, "rule_missing").unwrap(),
            None
        );

        let (rules, _) = load_user_standing_approvals_at(&path).unwrap();
        assert_eq!(rules, vec![existing]);
    }

    #[test]
    fn granting_the_same_rule_id_twice_replaces_it_rather_than_stacking() {
        let path = scratch("replace").join("settings.toml");
        grant_user_standing_approval_at(&path, &rule("rule_one", "shell", "command=a")).unwrap();
        let replacement = rule("rule_one", "shell", "command=b");

        grant_user_standing_approval_at(&path, &replacement).unwrap();

        let (rules, diagnostics) = load_user_standing_approvals_at(&path).unwrap();
        assert_eq!(rules, vec![replacement]);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn a_grant_preserves_the_rest_of_the_settings_file() {
        let path = scratch("preserve").join("settings.toml");
        std::fs::write(&path, "provider = \"deepseek\"\nmodel = \"chat\"\n").unwrap();

        grant_user_standing_approval_at(&path, &rule("rule_one", "shell", "command=a")).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("provider = \"deepseek\""));
        assert!(contents.contains("model = \"chat\""));
    }

    #[test]
    fn an_unreadable_entry_is_skipped_with_a_diagnostic_and_the_rest_stay_in_force() {
        let path = scratch("malformed").join("settings.toml");
        std::fs::write(
            &path,
            "[[permissions.standing]]\ntool_name = \"shell\"\nrule_content = \"command=a\"\n\n\
             [[permissions.standing]]\nrule_id = \"rule_good\"\ntool_name = \"shell\"\n\
             rule_content = \"command=b\"\ngranted_at = 7\n",
        )
        .unwrap();

        let (rules, diagnostics) = load_user_standing_approvals_at(&path).unwrap();

        // The good rule still applies: one bad row must never silently
        // re-open prompts the operator believes they silenced.
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].rule_id, "rule_good");
        assert_eq!(rules[0].granted_at, 7);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("missing a required field"));
    }

    #[test]
    fn the_settings_file_lives_in_the_viden_home_it_is_asked_about() {
        let home = scratch("home");

        assert_eq!(
            user_settings_path_in_home(&home),
            home.join("settings.toml")
        );
    }
}
