//! The `update_plan` built-in (`runtime.plan_documents`, C13).
//!
//! Codex's `update_plan` is the shape a planning model already emits, so the
//! input keeps it verbatim — `explanation` plus a `plan` list of
//! `{ step, status }` — and extends it only with fields Viden needs to turn an
//! accepted plan into agent work: a title, a goal, prose sections, and per-step
//! `role`, `file_scope`, and `dependencies`.
//!
//! Three properties of this tool are load-bearing:
//!
//! 1. **It is non-mutating.** Its effect is a Core fact about a plan, not a
//!    change to the workspace, memory, or task state, so the permission engine
//!    sees it as a safe read and it stays callable in plan mode. That is the
//!    whole point: plan mode exists to let a model plan, and until now the one
//!    thing it could not do was record the plan. Nothing here widens the gate
//!    for any other tool.
//! 2. **Over-bound input is a tool error.** Every bound in
//!    [`viden_types::PlanDocumentDraft::validate`] refuses rather than clamps,
//!    and the error goes back to the model, which can correct it. A truncated
//!    plan would be a plan the operator decides on and the model never wrote.
//! 3. **The parser is shared.** [`parse_update_plan_input`] is the one reader
//!    of this input: the registry-executed tool renders its summary from the
//!    parse, and the runtime builds the proposal from the same parse of the
//!    same call, so the summary the model reads and the record the operator
//!    decides on cannot describe different plans.

use viden_types::{
    AgentRole, PlanSection, PlanSectionKind, PlanStepDraft, PlanStepStatus, ToolInput, ToolSpec,
};

use crate::{BuiltinTool, ToolExecutionContext, ToolExecutionOutput};

/// The tool name, named once so the registry, the runtime fold, and the
/// contract documentation cannot drift into three spellings.
pub const UPDATE_PLAN_TOOL: &str = "update_plan";

pub(crate) struct UpdatePlanTool;

/// One parsed `update_plan` call, before Core gives it an owner and identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatePlanCall {
    pub title: String,
    pub goal: String,
    pub sections: Vec<PlanSection>,
    pub steps: Vec<PlanStepDraft>,
    pub explanation: Option<String>,
}

pub fn update_plan_tool_spec() -> ToolSpec {
    ToolSpec {
        name: UPDATE_PLAN_TOOL.to_string(),
        description: "Record or revise the plan for the current work as a Core fact the operator \
                      can accept, reject, and hand off"
            .to_string(),
        // Its effect is a plan document, not a workspace, memory, or task
        // mutation, so it is the one tool whose effect plan mode allows.
        is_mutating: false,
        input_schema_hint: "plan='[{\"step\":\"...\",\"status\":\"pending|in_progress|completed\",\
                            \"role\":\"coder\",\"file_scope\":[\"path\"],\"dependencies\":\
                            [\"step_1\"]}]' title='...' goal='...' explanation='...' \
                            sections='[{\"kind\":\"risks\",\"body\":\"...\"}]'"
            .to_string(),
    }
}

impl BuiltinTool for UpdatePlanTool {
    fn spec(&self) -> ToolSpec {
        update_plan_tool_spec()
    }

    fn run(
        &self,
        _ctx: &ToolExecutionContext,
        input: &ToolInput,
    ) -> Result<ToolExecutionOutput, String> {
        let call = parse_update_plan_input(input)?;
        Ok(ToolExecutionOutput {
            output: render_update_plan_summary(&call),
            diff: None,
            success: true,
            exit_code: None,
        })
    }
}

/// Reads the Codex-shaped input into the draft shape Core proposes from.
///
/// Every failure is a refusal with the key that caused it named, because the
/// caller is a model correcting its own call and "invalid input" is not
/// something it can act on.
pub fn parse_update_plan_input(input: &ToolInput) -> Result<UpdatePlanCall, String> {
    let raw_plan = input
        .get("plan")
        .map(String::as_str)
        .filter(|raw| !raw.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "`{UPDATE_PLAN_TOOL}` requires `plan`, a JSON array of {{\"step\", \"status\"}} \
                 entries"
            )
        })?;
    let entries: Vec<serde_json::Value> = serde_json::from_str(raw_plan)
        .map_err(|error| format!("`plan` is not a JSON array of plan steps: {error}"))?;

    let mut steps = Vec::with_capacity(entries.len());
    for (position, entry) in entries.iter().enumerate() {
        let label = position + 1;
        let object = entry
            .as_object()
            .ok_or_else(|| format!("`plan` entry {label} is not a JSON object"))?;
        let title = object
            .get("step")
            .and_then(|value| value.as_str())
            .ok_or_else(|| format!("`plan` entry {label} has no `step` text"))?
            .to_string();
        let status = match object.get("status").and_then(|value| value.as_str()) {
            None => PlanStepStatus::Pending,
            Some("pending") => PlanStepStatus::Pending,
            Some("in_progress") => PlanStepStatus::InProgress,
            Some("completed") => PlanStepStatus::Completed,
            Some(other) => {
                return Err(format!(
                    "`plan` entry {label} has status `{other}`; use `pending`, `in_progress`, or \
                     `completed`"
                ));
            }
        };
        let role =
            match object.get("role").and_then(|value| value.as_str()) {
                None => None,
                Some(name) => Some(parse_agent_role(name).ok_or_else(|| {
                    format!("`plan` entry {label} names an unknown role `{name}`")
                })?),
            };
        steps.push(PlanStepDraft {
            title,
            status,
            role,
            file_scope: string_list(object.get("file_scope"), "file_scope", label)?,
            dependencies: string_list(object.get("dependencies"), "dependencies", label)?,
        });
    }

    let explanation = input
        .get("explanation")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    // A plan has to be nameable. The title falls back to the explanation the
    // Codex shape already carries rather than to a manufactured one, and a call
    // with neither is refused: "Plan" is not a name the author wrote, and the
    // operator would be deciding on a document nobody titled.
    let title = input
        .get("title")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| explanation.clone())
        .ok_or_else(|| {
            format!("`{UPDATE_PLAN_TOOL}` requires `title` or `explanation` to name the plan")
        })?;

    let sections = match input
        .get("sections")
        .map(String::as_str)
        .filter(|raw| !raw.trim().is_empty())
    {
        None => Vec::new(),
        Some(raw) => parse_sections(raw)?,
    };

    Ok(UpdatePlanCall {
        title,
        goal: input.get("goal").cloned().unwrap_or_default(),
        sections,
        steps,
        explanation,
    })
}

fn parse_sections(raw: &str) -> Result<Vec<PlanSection>, String> {
    let entries: Vec<serde_json::Value> = serde_json::from_str(raw)
        .map_err(|error| format!("`sections` is not a JSON array of plan sections: {error}"))?;
    entries
        .iter()
        .enumerate()
        .map(|(position, entry)| {
            let label = position + 1;
            let object = entry
                .as_object()
                .ok_or_else(|| format!("`sections` entry {label} is not a JSON object"))?;
            let kind = object
                .get("kind")
                .and_then(|value| value.as_str())
                .ok_or_else(|| format!("`sections` entry {label} has no `kind`"))?;
            let kind = PlanSectionKind::parse(kind).ok_or_else(|| {
                format!(
                    "`sections` entry {label} names an unknown kind `{kind}`; use one of {}",
                    PlanSectionKind::ALL
                        .iter()
                        .map(|kind| kind.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;
            Ok(PlanSection {
                kind,
                body: object
                    .get("body")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string(),
            })
        })
        .collect()
}

fn string_list(
    value: Option<&serde_json::Value>,
    key: &str,
    label: usize,
) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .ok_or_else(|| format!("`plan` entry {label} has a non-array `{key}`"))?;
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(ToString::to_string)
                .ok_or_else(|| format!("`plan` entry {label} has a non-string `{key}` element"))
        })
        .collect()
}

fn parse_agent_role(name: &str) -> Option<AgentRole> {
    [
        AgentRole::Planner,
        AgentRole::Coder,
        AgentRole::Reviewer,
        AgentRole::Tester,
        AgentRole::DocWriter,
        AgentRole::Researcher,
        AgentRole::ReleaseOperator,
    ]
    .into_iter()
    .find(|role| role.as_str() == name.trim())
}

/// What the model reads back.
///
/// Counts rather than a repeat of the plan: the model just sent the steps, and
/// echoing them would spend context restating input. The counts are what tells
/// it the call was understood the way it meant.
pub fn render_update_plan_summary(call: &UpdatePlanCall) -> String {
    let count = |status: PlanStepStatus| {
        call.steps
            .iter()
            .filter(|step| step.status == status)
            .count()
    };
    format!(
        "Recorded the plan `{}`: {} step(s) ({} pending, {} in progress, {} completed) and {} \
         section(s). The operator decides it; it is not work until they hand it off.",
        call.title,
        call.steps.len(),
        count(PlanStepStatus::Pending),
        count(PlanStepStatus::InProgress),
        count(PlanStepStatus::Completed),
        call.sections.len(),
    )
}
