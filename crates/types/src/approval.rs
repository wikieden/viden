use crate::{SessionId, ToolInput};

/// Stable risk buckets are semantic facts, not localized frontend labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalRisk {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ApprovalTarget {
    pub kind: String,
    pub display: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalScope {
    Once,
    Session {
        session_id: SessionId,
    },
    RepoAllowlist {
        paths: Vec<String>,
    },
    /// A standing rule across sessions and repositories, revocable
    /// (`runtime.approval_scopes`, GUI-CORE-019).
    ///
    /// Unlike every other scope, the rule this installs outlives the process:
    /// it is written to the user-level settings file `viden-config` owns and
    /// loaded at bootstrap for every workspace. That is why it is offered only
    /// when the request's risk is below `High` and the mode is not `Plan`, and
    /// why its revocation ships in the same increment as its grant.
    Always,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Allow {
        scope: ApprovalScope,
    },
    Deny,
    /// The operator revised the tool input; Core re-evaluates the revision
    /// through the same permission gate the original went through
    /// (`runtime.approval_scopes`, GUI-CORE-019).
    ///
    /// This is not an allow. A revision is a *new* input, so it is decided
    /// afresh: a rule or plan mode may still deny it, and an input the engine
    /// still wants a decision about produces a second request rather than a
    /// silent run.
    Edit {
        revised_input: ToolInput,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDefaultAction {
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ApprovalResponse {
    pub decision: ApprovalDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
}

impl ApprovalResponse {
    pub fn allow_once(feedback: Option<String>) -> Self {
        Self {
            decision: ApprovalDecision::Allow {
                scope: ApprovalScope::Once,
            },
            feedback,
        }
    }

    pub fn deny(feedback: Option<String>) -> Self {
        Self {
            decision: ApprovalDecision::Deny,
            feedback,
        }
    }

    pub fn allow_always(feedback: Option<String>) -> Self {
        Self {
            decision: ApprovalDecision::Allow {
                scope: ApprovalScope::Always,
            },
            feedback,
        }
    }

    pub fn edit(revised_input: ToolInput, feedback: Option<String>) -> Self {
        Self {
            decision: ApprovalDecision::Edit { revised_input },
            feedback,
        }
    }

    /// Whether this answer, on its own, permits the call.
    ///
    /// `Edit` is deliberately not allowed here: the revision has not been
    /// decided yet, so treating it as an allow would run an input no gate has
    /// seen. Call sites that must distinguish "not yet decided" from "refused"
    /// match the decision instead.
    pub fn is_allowed(&self) -> bool {
        matches!(self.decision, ApprovalDecision::Allow { .. })
    }
}

impl<'de> serde::Deserialize<'de> for ApprovalResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Structured {
            decision: ApprovalDecision,
            #[serde(default)]
            feedback: Option<String>,
        }

        #[derive(serde::Deserialize)]
        struct Legacy {
            approved: bool,
            #[serde(default)]
            feedback: Option<String>,
        }

        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Structured(Structured),
            Legacy(Legacy),
        }

        match Wire::deserialize(deserializer)? {
            Wire::Structured(value) => Ok(Self {
                decision: value.decision,
                feedback: value.feedback,
            }),
            Wire::Legacy(value) if value.approved => Ok(Self::allow_once(value.feedback)),
            Wire::Legacy(value) => Ok(Self::deny(value.feedback)),
        }
    }
}

/// One standing `Always` approval, as a client renders and revokes it
/// (`runtime.approval_scopes`, GUI-CORE-019).
///
/// `rule_content` is the same encoding a `Session` rule uses, so the standing
/// list and the session rules describe one vocabulary rather than two.
/// `granted_in_workspace` records where the operator granted it; the rule
/// itself applies everywhere, and a client that rendered the workspace as a
/// scope would understate what was granted.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StandingApprovalView {
    pub rule_id: String,
    pub tool_name: String,
    pub rule_content: String,
    pub granted_at: u64,
    pub granted_in_workspace: String,
    pub audit_id: String,
}
