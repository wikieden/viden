//! Standing approvals and input revisions
//! (`runtime.approval_scopes`, GUI-CORE-019).
//!
//! Two facts this client renders and one it must never invent:
//!
//! - **What is in force is `RuntimeViewState.standing_approvals`.** An
//!   `Always` grant installs a rule in the user-level settings file and is
//!   loaded at bootstrap for every workspace, so a client that listed the
//!   grants it happened to witness would be listing a strict subset of what is
//!   actually allowed. The list and the revoke both read Core's field.
//! - **A revision is not an allow.** `ApprovalDecision::Edit` hands Core new
//!   bytes to re-decide through the same gate; a rule or plan mode may still
//!   deny them, and an input the gate still wants a decision about produces a
//!   second request carrying `revised_from`. Nothing here treats sending an
//!   edit as having permitted anything.
//! - **The revision is never scraped from `input_preview`.** That field is a
//!   rendered `key: value` dump for a human reader. The draft is decoded with
//!   the contract's own `ToolInput` encoding instead, which is also why the
//!   field cannot be pre-filled: `ApprovalRequestView` carries no typed input.

use viden_core::RuntimeCommand;

/// The capability that publishes the standing scope, the revision decision,
/// and the standing-approval list.
pub(super) const APPROVAL_SCOPES_CAPABILITY: &str = "runtime.approval_scopes";

/// Removes one standing rule from the file and the live engine.
///
/// The `rule_id` is copied out of the record Core published; a rule id this
/// client composed could only ever name a rule Core does not hold, which Core
/// answers with `CommandRejected` rather than with a revocation that removed
/// nothing.
pub(super) fn revoke_standing_approval_intent(rule_id: impl Into<String>) -> RuntimeCommand {
    RuntimeCommand::RevokeStandingApproval {
        rule_id: rule_id.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::{InteractionPanel, TuiState};
    use viden_types::{CapabilityId, StandingApprovalView};

    fn state_with_rules(rules: Vec<StandingApprovalView>) -> TuiState {
        let mut state = TuiState::default();
        state
            .capabilities
            .insert(CapabilityId(APPROVAL_SCOPES_CAPABILITY.to_string()));
        state.runtime.standing_approvals = rules;
        state.ui.interaction_panel = Some(InteractionPanel::StandingApprovals { selected: 0 });
        state
    }

    fn rule(rule_id: &str, tool: &str) -> StandingApprovalView {
        StandingApprovalView {
            rule_id: rule_id.to_string(),
            tool_name: tool.to_string(),
            rule_content: "command=cargo fmt --all -- --check".to_string(),
            granted_at: 1_700_004_002,
            granted_in_workspace: "workspace_contract_v1".to_string(),
            audit_id: "audit_approval_scopes_always".to_string(),
        }
    }

    #[test]
    fn an_empty_standing_list_renders_as_absence() {
        let state = state_with_rules(Vec::new());

        assert_eq!(
            super::super::modal::standing_approval_rows(&state, 0),
            vec!["No standing approvals. Core holds no rule that outlives this session."]
        );
    }

    #[test]
    fn a_client_without_the_capability_names_it_instead_of_listing_rules() {
        let mut state = state_with_rules(vec![rule("rule_1", "shell")]);
        state.capabilities.clear();

        let rows = super::super::modal::standing_approval_rows(&state, 0);

        assert_eq!(rows.len(), 1);
        assert!(rows[0].contains(APPROVAL_SCOPES_CAPABILITY), "{rows:?}");
    }

    #[test]
    fn every_rule_names_its_tool_its_content_its_workspace_and_its_audit_row() {
        let state = state_with_rules(vec![rule("rule_1", "shell")]);

        let rendered = super::super::modal::standing_approval_rows(&state, 0).join("\n");

        assert!(rendered.contains("shell"), "{rendered}");
        assert!(rendered.contains("cargo fmt"), "{rendered}");
        assert!(rendered.contains("workspace_contract_v1"), "{rendered}");
        assert!(
            rendered.contains("audit_approval_scopes_always"),
            "{rendered}"
        );
    }

    #[test]
    fn revoking_names_the_rule_id_core_published() {
        assert_eq!(
            revoke_standing_approval_intent("rule_approval_scopes"),
            RuntimeCommand::RevokeStandingApproval {
                rule_id: "rule_approval_scopes".to_string(),
            }
        );
    }
}
