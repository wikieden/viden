mod memory_commands;
mod task_commands;

use super::SessionEngine;
use crate::presentation::render_permission_denial;
use viden_types::{
    ApprovalResponse, PermissionDecision, PermissionLogEntry, ToolInput, ToolSpec, TranscriptEntry,
    now_timestamp,
};

impl SessionEngine {
    pub(crate) fn ensure_workflow_permission<F>(
        &mut self,
        action: &str,
        preview: &str,
        approver: &mut F,
    ) -> Result<Option<String>, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        self.ensure_workflow_permission_with_context(action, preview, None, approver)
    }

    /// The same gate, carrying what Core knows about the change the operator
    /// is being asked to approve (`runtime.structured_diff`).
    ///
    /// Only the sites that already hold a validated proposal pass a context —
    /// the trust loop's patch merge holds the canonical patch bytes. Every
    /// other workflow mutation passes `None`, which means Core computed no
    /// preview rather than "this changes nothing".
    pub(crate) fn ensure_workflow_permission_with_context<F>(
        &mut self,
        action: &str,
        preview: &str,
        decision_context: Option<viden_types::DecisionContext>,
        approver: &mut F,
    ) -> Result<Option<String>, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        let tool_name = format!("workflow_{action}");
        let tool = ToolSpec {
            name: tool_name.clone(),
            description: format!("Workflow mutation: {action}"),
            is_mutating: true,
            input_schema_hint: "workflow action".to_string(),
        };
        let mut input = ToolInput::new();
        input.insert("action".to_string(), action.to_string());
        input.insert("preview".to_string(), preview.to_string());
        let mut gate_permissions = self.permissions.clone();
        let decision =
            crate::permission_gate::resolve(&mut gate_permissions, &tool, &tool_name, &input, {
                |_ask, mut prompt| {
                    prompt.decision_context = decision_context.clone();
                    approver(prompt)
                }
            });
        match decision {
            PermissionDecision::Allow(allow) => {
                self.store_entry(TranscriptEntry::Permission {
                    entry: PermissionLogEntry {
                        timestamp: now_timestamp(),
                        tool_name,
                        decision: "allow".to_string(),
                        reason: format!("{:?}", allow.decision_reason),
                        message: allow.accept_feedback,
                    },
                })?;
                Ok(None)
            }
            PermissionDecision::Ask(_) => unreachable!("ask decisions should be resolved"),
            PermissionDecision::Deny(deny) => {
                let reason = format!("{:?}", deny.decision_reason);
                self.store_entry(TranscriptEntry::Permission {
                    entry: PermissionLogEntry {
                        timestamp: now_timestamp(),
                        tool_name: tool_name.clone(),
                        decision: "deny".to_string(),
                        reason: reason.clone(),
                        message: Some(deny.message.clone()),
                    },
                })?;
                Ok(Some(render_permission_denial(
                    &tool_name,
                    &reason,
                    &deny.message,
                )))
            }
        }
    }
}
