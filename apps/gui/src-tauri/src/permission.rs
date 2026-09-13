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
        /// The revised tool input an `Edit` decision carries.
        ///
        /// Required for `PermissionChoice::Edit` and refused for every other
        /// choice: a revision is a new input Core re-evaluates through the same
        /// gate, not a variant of an allow.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        revised_input: Option<std::collections::BTreeMap<String, String>>,
    },
    /// Removes one standing `Always` rule (`RevokeStandingApproval`).
    ///
    /// The revocation lives beside the grant deliberately: a standing rule
    /// outlives the process, so a client that could install one and not remove
    /// one would be offering a decision the operator cannot take back.
    Revoke { rule_id: String },
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

/// One key/value of the tool input an `Edit` decision may revise.
///
/// Core publishes `input_preview` — its own rendering of the `ToolInput` map —
/// and no structured map, so these pairs are parsed from that rendering by the
/// projection, fail-closed, and the Edit control is withdrawn when the parse is
/// not exact. See `contract-requests.md` GUI-CORE-035.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionInputFieldProjection {
    pub key: String,
    pub value: String,
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
    /// Whether Core will accept an `ApprovalDecision::Edit` for this request
    /// (`runtime.approval_scopes`, GUI-CORE-019).
    pub editable: bool,
    /// The request this one replaces, when Core produced it from an `Edit`.
    ///
    /// The dock renders the revision linked to its original rather than as a
    /// second unrelated prompt, which is what the field exists for.
    pub revised_from: Option<String>,
    /// The editable key/value pairs, or empty when the preview could not be
    /// parsed exactly. Empty with `editable` true is why `edit_block` exists.
    pub input_fields: Vec<PermissionInputFieldProjection>,
    /// Why Edit is not offered, when Core says it is editable but this client
    /// cannot present the fields. `None` when Edit is available or when Core
    /// itself does not offer it.
    pub edit_block: Option<&'static str>,
    pub actions: Vec<PermissionActionProjection>,
}

/// One standing `Always` rule in force (`StandingApprovalView`).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StandingApprovalProjection {
    pub rule_id: String,
    pub tool_name: String,
    /// The same encoding a `Session` rule uses, printed verbatim: it is the
    /// contract's own key for what the rule matches.
    pub rule_content: String,
    pub granted_at: u64,
    pub granted_in_workspace: String,
    /// The audit row the grant wrote, which the D14 link opens.
    pub audit_id: String,
}

/// The standing approvals list and whether Core can answer for it at all.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StandingApprovalsProjection {
    /// Whether Core's handshake published `runtime.approval_scopes`.
    ///
    /// Empty with the capability present means no rule is installed; empty
    /// without it means nobody could answer. The two are different sentences
    /// and the panel prints them differently.
    pub capability_available: bool,
    pub rules: Vec<StandingApprovalProjection>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionDockProjection {
    pub work_mode: String,
    pub permission_level: String,
    pub request: Option<PermissionRequestProjection>,
    /// The standing rules in force, so the dock can say that an Always grant
    /// joined a list the operator can revoke rather than vanishing into a file.
    pub standing: StandingApprovalsProjection,
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

/// The answer to a standing-approval revocation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StandingApprovalIntentResult {
    pub projection: StandingApprovalsProjection,
    pub outcome: crate::D1OutcomeProjection,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionIntentResult {
    pub projection: PermissionDockProjection,
    pub pending_command_id: Option<String>,
    pub outcome: PermissionOutcomeProjection,
}
