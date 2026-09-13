//! Plan documents (`runtime.plan_documents`, C13).
//!
//! Plan mode already denies every mutating tool
//! (`crates/permissions/src/lib.rs`, the frozen `plan-denial` fixture), but
//! until this capability the planner's output was transcript prose: nothing
//! said what the plan *was*, whether an operator had accepted it, or what work
//! it became. A client could only render the assistant's text and hope an
//! operator read it the same way twice.
//!
//! Four rules shape the types below, and each exists because dropping it
//! produces a specific lie:
//!
//! 1. **A plan is a Core-owned durable fact, never client state.** Codex's
//!    `update_plan` and DeepSeek-TUI's read-only plan mode both keep the plan
//!    in the conversation; Viden keeps the same tool shape but writes the plan
//!    into Core, because a plan that lives in one client's memory cannot be
//!    decided, audited, or handed off, and a second client looking at the same
//!    session would see no plan at all.
//! 2. **A decision is pinned to what the operator saw.** [`PlanDocumentRecord`]
//!    carries `revision` and `content_sha256`, and an accept or reject names
//!    both. A decision that only named the plan id would silently accept a
//!    revision the model wrote while the operator was reading the previous one.
//! 3. **A superseded decision is published, not forgotten.** A new proposal
//!    over a decided plan republishes the earlier revision as
//!    [`PlanDocumentStatus::Superseded`] before publishing the new one, so the
//!    durable log says the earlier acceptance was voided rather than leaving a
//!    client holding an `Accepted` record that Core no longer honours.
//! 4. **Bounds refuse rather than truncate.** An over-bound `update_plan` call
//!    is a tool error the model sees and can correct. A truncated plan would be
//!    a plan the operator decides on and the model never wrote.

use serde::{Deserialize, Serialize};

use crate::{AgentDagId, AgentRole, AuditId, RuntimeOwner, SessionId};

/// Largest number of steps one plan may carry.
pub const MAX_PLAN_STEPS: usize = 64;
/// Byte bound for the plan title and for one step title.
pub const MAX_PLAN_TITLE_BYTES: usize = 256;
/// Byte bound for the plan goal.
pub const MAX_PLAN_GOAL_BYTES: usize = 4 * 1024;
/// Byte bound for one section body.
pub const MAX_PLAN_SECTION_BODY_BYTES: usize = 16 * 1024;
/// Largest number of workspace-relative paths one step may scope itself to.
pub const MAX_PLAN_STEP_FILE_SCOPE: usize = 64;
/// Byte bound for one `file_scope` entry.
pub const MAX_PLAN_FILE_SCOPE_ENTRY_BYTES: usize = 512;
/// Byte bound for the reason on a rejected plan.
pub const MAX_PLAN_REJECT_REASON_BYTES: usize = 2 * 1024;

/// Identity of one plan document, `plan_<id>`.
///
/// A newtype rather than the bare `String` the older id aliases use: a plan id
/// travels beside a revision and a content digest on every decision command,
/// and three strings in a row is exactly the shape that gets transposed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlanDocumentId(pub String);

impl PlanDocumentId {
    pub const PREFIX: &'static str = "plan";

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PlanDocumentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where one plan stands with its owner.
///
/// `#[non_exhaustive]`: a later status must not break a client's match arms,
/// and a client that cannot name the status still knows the plan exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PlanDocumentStatus {
    /// The planner has started writing; no revision proposed yet.
    Drafting,
    /// A revision awaits the operator's decision.
    Proposed,
    Accepted,
    Rejected,
    /// Accepted and turned into work; `handoff` names the DAG.
    HandedOff,
    /// A newer revision replaced this decision.
    Superseded,
}

impl PlanDocumentStatus {
    /// Whether a plan in this status is the owner's *open* plan.
    ///
    /// One open plan per owner is the rule a new proposal is measured against:
    /// a call against an open plan revises it, and a call with none open
    /// creates one. Named here so the runtime and the tool cannot drift into
    /// two definitions of "open".
    pub fn is_open(self) -> bool {
        matches!(self, Self::Drafting | Self::Proposed | Self::Accepted)
    }

    /// Whether the status is a settled operator decision.
    pub fn is_decided(self) -> bool {
        matches!(self, Self::Accepted | Self::Rejected | Self::HandedOff)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Drafting => "drafting",
            Self::Proposed => "proposed",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::HandedOff => "handed_off",
            Self::Superseded => "superseded",
        }
    }
}

/// Codex's three step states, unchanged.
///
/// Deliberately not extended: the reference tool's vocabulary is what a model
/// already produces, and a fourth state here would be a state no planner emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStepStatus {
    Pending,
    InProgress,
    Completed,
}

impl PlanStepStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }
}

/// One step of a plan.
///
/// `id` is assigned by Core as `step_<1-based position>` and is stable within
/// the plan, so `dependencies` can name siblings without the author inventing
/// identity. `role` and `file_scope` exist because a step becomes an
/// [`crate::AgentDagTaskSpec`] at handoff: a step with no role is a task Core
/// would have to guess a role for, and guessing is what `Coder` being the
/// documented default replaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub title: String,
    pub status: PlanStepStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<AgentRole>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_scope: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
}

impl PlanStep {
    /// The id Core assigns to the step at `position` (0-based).
    pub fn id_for_position(position: usize) -> String {
        format!("step_{}", position + 1)
    }
}

/// One step as an author writes it, before Core assigns identity.
///
/// Separate from [`PlanStep`] precisely so identity has exactly one producer:
/// if the draft carried an id, a second call could renumber a step an operator
/// had already read and a dependency edge would silently point somewhere else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStepDraft {
    pub title: String,
    pub status: PlanStepStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<AgentRole>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_scope: Vec<String>,
    /// Sibling step ids in `step_<n>` form, resolved against this same draft.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
}

/// The prose sections a plan may carry, one of each at most.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanSectionKind {
    Requirements,
    Assumptions,
    Constraints,
    ArchitectureDecisions,
    Risks,
    TestStrategy,
    AcceptanceCriteria,
}

impl PlanSectionKind {
    pub const ALL: [Self; 7] = [
        Self::Requirements,
        Self::Assumptions,
        Self::Constraints,
        Self::ArchitectureDecisions,
        Self::Risks,
        Self::TestStrategy,
        Self::AcceptanceCriteria,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requirements => "requirements",
            Self::Assumptions => "assumptions",
            Self::Constraints => "constraints",
            Self::ArchitectureDecisions => "architecture_decisions",
            Self::Risks => "risks",
            Self::TestStrategy => "test_strategy",
            Self::AcceptanceCriteria => "acceptance_criteria",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value.trim())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanSection {
    pub kind: PlanSectionKind,
    pub body: String,
}

/// Who proposed a revision.
///
/// The command path always records `Operator` and the `update_plan` tool
/// always records `Assistant`: the author is a property of the path the
/// proposal arrived on, never a field a client fills in, so a client cannot
/// file its own text as something the model wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum PlanAuthor {
    Assistant {
        session_id: SessionId,
        turn_id: String,
    },
    Operator,
}

/// The DAG an accepted plan became.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanHandoff {
    pub dag_id: AgentDagId,
    pub handed_off_at: u64,
    pub audit_id: AuditId,
}

/// One plan document, as Core holds it.
///
/// The record carries the *latest* revision only; the durable log keeps every
/// revision, including the superseded ones, which is what lets a reader
/// reconstruct why an acceptance stopped being honoured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanDocumentRecord {
    pub plan_id: PlanDocumentId,
    /// Session or Lane scope, never `RuntimeOwner::default()`: a plan that
    /// belongs to nobody cannot be found by the owner whose work it describes,
    /// and "one open plan per owner" would collapse into one global plan.
    pub owner: RuntimeOwner,
    pub title: String,
    pub goal: String,
    pub status: PlanDocumentStatus,
    /// 1-based; every proposal increments it.
    pub revision: u32,
    /// Digest over title, goal, sections, and steps — see
    /// [`plan_content_digest_source`]. Minted by Core and echoed back by a
    /// decision, so an operator's accept names the bytes they read.
    pub content_sha256: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<PlanSection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<PlanStep>,
    pub proposed_by: PlanAuthor,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_audit_id: Option<AuditId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff: Option<PlanHandoff>,
}

/// A proposal before Core gives it identity, a revision, and a digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanDocumentDraft {
    pub owner: RuntimeOwner,
    pub title: String,
    pub goal: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<PlanSection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<PlanStepDraft>,
}

impl PlanDocumentDraft {
    /// Rejects an out-of-bounds or self-inconsistent draft.
    ///
    /// Every failure here is a refusal rather than a repair: a clamped plan is
    /// a plan the operator decides on and the author never wrote, and a
    /// silently dropped dependency edge is a task order nobody chose.
    pub fn validate(&self) -> Result<(), String> {
        if self.owner == RuntimeOwner::default() {
            return Err(
                "a plan document requires a real owner; `RuntimeOwner::default()` names nobody"
                    .to_string(),
            );
        }
        if self.title.trim().is_empty() {
            return Err("a plan document requires a title".to_string());
        }
        if self.title.len() > MAX_PLAN_TITLE_BYTES {
            return Err(format!(
                "plan title exceeds the {MAX_PLAN_TITLE_BYTES} byte bound"
            ));
        }
        if self.goal.len() > MAX_PLAN_GOAL_BYTES {
            return Err(format!(
                "plan goal exceeds the {MAX_PLAN_GOAL_BYTES} byte bound"
            ));
        }
        let mut seen_kinds = Vec::new();
        for section in &self.sections {
            if seen_kinds.contains(&section.kind) {
                return Err(format!(
                    "plan section `{}` appears more than once; a plan carries at most one section \
                     per kind",
                    section.kind.as_str()
                ));
            }
            seen_kinds.push(section.kind);
            if section.body.len() > MAX_PLAN_SECTION_BODY_BYTES {
                return Err(format!(
                    "plan section `{}` exceeds the {MAX_PLAN_SECTION_BODY_BYTES} byte bound",
                    section.kind.as_str()
                ));
            }
        }
        if self.steps.len() > MAX_PLAN_STEPS {
            return Err(format!(
                "plan carries {} steps, over the {MAX_PLAN_STEPS} step bound",
                self.steps.len()
            ));
        }
        let ids = (0..self.steps.len())
            .map(PlanStep::id_for_position)
            .collect::<Vec<_>>();
        for (position, step) in self.steps.iter().enumerate() {
            let id = &ids[position];
            if step.title.trim().is_empty() {
                return Err(format!("plan step `{id}` requires a title"));
            }
            if step.title.len() > MAX_PLAN_TITLE_BYTES {
                return Err(format!(
                    "plan step `{id}` title exceeds the {MAX_PLAN_TITLE_BYTES} byte bound"
                ));
            }
            if step.file_scope.len() > MAX_PLAN_STEP_FILE_SCOPE {
                return Err(format!(
                    "plan step `{id}` scopes {} paths, over the {MAX_PLAN_STEP_FILE_SCOPE} entry \
                     bound",
                    step.file_scope.len()
                ));
            }
            for path in &step.file_scope {
                if path.len() > MAX_PLAN_FILE_SCOPE_ENTRY_BYTES {
                    return Err(format!(
                        "plan step `{id}` scopes a path over the \
                         {MAX_PLAN_FILE_SCOPE_ENTRY_BYTES} byte bound"
                    ));
                }
                if path.starts_with('/') || path.split('/').any(|segment| segment == "..") {
                    return Err(format!(
                        "plan step `{id}` scopes `{path}`, which is not workspace-relative"
                    ));
                }
            }
            for dependency in &step.dependencies {
                if dependency == id {
                    return Err(format!("plan step `{id}` depends on itself"));
                }
                if !ids.contains(dependency) {
                    return Err(format!(
                        "plan step `{id}` depends on `{dependency}`, which is not a step of this \
                         plan"
                    ));
                }
            }
        }
        Ok(())
    }

    /// The draft's steps with Core-assigned identity.
    pub fn into_steps(&self) -> Vec<PlanStep> {
        self.steps
            .iter()
            .enumerate()
            .map(|(position, step)| PlanStep {
                id: PlanStep::id_for_position(position),
                title: step.title.clone(),
                status: step.status,
                role: step.role,
                file_scope: step.file_scope.clone(),
                dependencies: step.dependencies.clone(),
            })
            .collect()
    }
}

/// The exact bytes `content_sha256` digests.
///
/// Built here rather than at each producer so the runtime that mints a digest,
/// the client that echoes it, and the fixture that proves the round trip
/// cannot drift into three canonicalizations of the same plan. Deliberately a
/// hand-written encoding rather than `serde_json`: a field added to
/// [`PlanDocumentRecord`] later must not silently change the digest of a plan
/// whose title, goal, sections, and steps are unchanged, which is exactly what
/// a derived serialization would do.
///
/// The digest itself is computed by the caller, because `viden-types` carries
/// no hash implementation and gaining one for this would put a crypto
/// dependency under every crate in the workspace.
pub fn plan_content_digest_source(
    title: &str,
    goal: &str,
    sections: &[PlanSection],
    steps: &[PlanStep],
) -> String {
    let mut source = String::new();
    source.push_str("plan/v1\n");
    source.push_str(&format!("title:{}\n", title.len()));
    source.push_str(title);
    source.push('\n');
    source.push_str(&format!("goal:{}\n", goal.len()));
    source.push_str(goal);
    source.push('\n');
    source.push_str(&format!("sections:{}\n", sections.len()));
    for section in sections {
        source.push_str(&format!(
            "{}:{}\n",
            section.kind.as_str(),
            section.body.len()
        ));
        source.push_str(&section.body);
        source.push('\n');
    }
    source.push_str(&format!("steps:{}\n", steps.len()));
    for step in steps {
        source.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\n",
            step.id,
            step.status.as_str(),
            step.role.map(AgentRole::as_str).unwrap_or(""),
            step.dependencies.join(","),
            step.file_scope.join(","),
            step.title,
        ));
    }
    source
}
