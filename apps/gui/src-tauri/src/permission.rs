use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionChoice {
    Once,
    Session,
    RepoAllowlist,
    Always,
    Edit,
    Deny,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum PermissionIntent {
    Respond {
        request_id: String,
        choice: PermissionChoice,
        feedback: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionTargetProjection {
    pub kind: String,
    pub display: String,
    pub canonical_ref: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionActionProjection {
    pub kind: String,
    pub available: bool,
    pub session_id: Option<String>,
    pub paths: Vec<String>,
    pub code: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequestProjection {
    pub id: String,
    pub tool_name: String,
    pub title: String,
    pub message: String,
    pub input_preview: String,
    pub is_mutating: bool,
    pub reason: Option<String>,
    pub risk: String,
    pub target: PermissionTargetProjection,
    pub policy_reason_key: String,
    pub policy_reason_args: std::collections::BTreeMap<String, String>,
    pub expires_at: u64,
    pub default_action: &'static str,
    pub audit_id: String,
    pub blocked_by_plan: bool,
    /// What Core knows about the change this approval would make
    /// (`runtime.structured_diff`, GUI-CORE-012).
    ///
    /// `None` for every approval Core attached no context to, which is the
    /// ordinary case: the dock then keeps rendering `input_preview` verbatim
    /// rather than an empty diff.
    pub decision_context: Option<crate::diff_review::DecisionContextProjection>,
    pub actions: Vec<PermissionActionProjection>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionDockProjection {
    pub work_mode: String,
    pub permission_level: String,
    pub request: Option<PermissionRequestProjection>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PermissionOutcomeProjection {
    pub state: &'static str,
    pub reason: Option<String>,
}

impl PermissionOutcomeProjection {
    pub(crate) fn idle() -> Self {
        Self {
            state: "idle",
            reason: None,
        }
    }

    pub(crate) fn pending() -> Self {
        Self {
            state: "pending",
            reason: None,
        }
    }

    pub(crate) fn confirmed() -> Self {
        Self {
            state: "confirmed",
            reason: None,
        }
    }

    pub(crate) fn rejected(reason: String) -> Self {
        Self {
            state: "rejected",
            reason: Some(reason),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionIntentResult {
    pub projection: PermissionDockProjection,
    pub pending_command_id: Option<String>,
    pub outcome: PermissionOutcomeProjection,
}
