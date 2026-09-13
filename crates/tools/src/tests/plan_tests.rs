//! The `update_plan` built-in (`runtime.plan_documents`, C13).
//!
//! Three things have to hold for a planning model to be able to use this tool
//! at all: the Codex shape it already emits parses unchanged, every refusal
//! names the key the model must fix, and the tool never mutates — because the
//! mode it exists for denies every tool that does.

use viden_types::{AgentRole, PlanSectionKind, PlanStepStatus, ToolCall, ToolInput, fresh_id};

use super::temp_dir;
use crate::plan::{UPDATE_PLAN_TOOL, parse_update_plan_input, update_plan_tool_spec};
use crate::{ToolExecutionContext, ToolRegistry};

fn input(pairs: &[(&str, &str)]) -> ToolInput {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect()
}

/// The tool plan mode is built around must itself be non-mutating; anything
/// else and the permission engine denies the one tool the mode exists to allow.
#[test]
fn the_update_plan_spec_is_non_mutating() {
    let spec = update_plan_tool_spec();
    assert_eq!(spec.name, UPDATE_PLAN_TOOL);
    assert!(!spec.is_mutating);
    assert!(
        ToolRegistry::builtin()
            .spec(UPDATE_PLAN_TOOL)
            .is_some_and(|spec| !spec.is_mutating),
        "the built-in registry must offer `update_plan` to the model"
    );
}

/// Codex's own shape — `explanation` plus `{step, status}` entries and nothing
/// else — parses unchanged, and the title falls back to the explanation rather
/// than to a manufactured name.
#[test]
fn the_bare_codex_shape_parses_unchanged() {
    let call = parse_update_plan_input(&input(&[
        ("explanation", "Split the parser rename"),
        (
            "plan",
            r#"[{"step":"rename the entry point","status":"completed"},
                {"step":"update the callers","status":"in_progress"},
                {"step":"add the regression test","status":"pending"}]"#,
        ),
    ]))
    .expect("the reference shape parses");

    assert_eq!(call.title, "Split the parser rename");
    assert_eq!(call.explanation.as_deref(), Some("Split the parser rename"));
    assert!(call.goal.is_empty());
    assert!(call.sections.is_empty());
    assert_eq!(call.steps.len(), 3);
    assert_eq!(call.steps[0].status, PlanStepStatus::Completed);
    assert_eq!(call.steps[1].status, PlanStepStatus::InProgress);
    assert_eq!(call.steps[2].status, PlanStepStatus::Pending);
    assert!(call.steps[0].role.is_none());
}

/// The Viden extensions ride the same call: a title and goal of their own,
/// prose sections, and the per-step fields a handoff turns into task specs.
#[test]
fn the_viden_extensions_ride_the_same_call() {
    let call = parse_update_plan_input(&input(&[
        ("title", "Model plan documents"),
        ("goal", "give the operator something to accept"),
        (
            "sections",
            r#"[{"kind":"risks","body":"the digest must cover the steps"},
                {"kind":"acceptance_criteria","body":"the fixture replays"}]"#,
        ),
        (
            "plan",
            r#"[{"step":"model the types","status":"pending","role":"coder",
                 "file_scope":["crates/types/src/plan.rs"]},
                {"step":"review the contract","status":"pending","role":"reviewer",
                 "dependencies":["step_1"]}]"#,
        ),
    ]))
    .expect("the extended shape parses");

    assert_eq!(call.title, "Model plan documents");
    assert_eq!(call.goal, "give the operator something to accept");
    assert_eq!(call.sections.len(), 2);
    assert_eq!(call.sections[0].kind, PlanSectionKind::Risks);
    assert_eq!(call.sections[1].kind, PlanSectionKind::AcceptanceCriteria);
    assert_eq!(call.steps[0].role, Some(AgentRole::Coder));
    assert_eq!(
        call.steps[0].file_scope,
        vec!["crates/types/src/plan.rs".to_string()]
    );
    assert_eq!(call.steps[1].role, Some(AgentRole::Reviewer));
    assert_eq!(call.steps[1].dependencies, vec!["step_1".to_string()]);
}

/// Every refusal names the key the model has to fix. "Invalid input" is not
/// something a model can act on, and a silently dropped field would leave the
/// operator deciding on a plan the model did not write.
#[test]
fn every_refusal_names_the_input_it_refused() {
    let cases: Vec<(ToolInput, &str)> = vec![
        (input(&[("title", "No steps")]), "requires `plan`"),
        (
            input(&[("title", "Bad json"), ("plan", "not json")]),
            "not a JSON array",
        ),
        (
            input(&[("title", "Bad entry"), ("plan", r#"["just a string"]"#)]),
            "entry 1 is not a JSON object",
        ),
        (
            input(&[
                ("title", "No step text"),
                ("plan", r#"[{"status":"pending"}]"#),
            ]),
            "entry 1 has no `step` text",
        ),
        (
            input(&[
                ("title", "Bad status"),
                ("plan", r#"[{"step":"a","status":"blocked"}]"#),
            ]),
            "status `blocked`",
        ),
        (
            input(&[
                ("title", "Bad role"),
                ("plan", r#"[{"step":"a","role":"architect"}]"#),
            ]),
            "unknown role `architect`",
        ),
        (
            input(&[
                ("title", "Bad scope"),
                ("plan", r#"[{"step":"a","file_scope":"one/path"}]"#),
            ]),
            "non-array `file_scope`",
        ),
        (
            input(&[
                ("title", "Bad section"),
                ("plan", r#"[{"step":"a"}]"#),
                ("sections", r#"[{"kind":"vibes","body":"x"}]"#),
            ]),
            "unknown kind `vibes`",
        ),
        (
            input(&[("plan", r#"[{"step":"a"}]"#)]),
            "requires `title` or `explanation`",
        ),
    ];
    for (case, expected) in cases {
        let error = parse_update_plan_input(&case)
            .expect_err(&format!("{case:?} must be refused"))
            .to_string();
        assert!(error.contains(expected), "{error} does not name {expected}");
    }
}

/// The registry run summarizes what it understood rather than echoing the plan
/// back. Echoing would spend context restating the model's own input; the
/// counts are what tell it the call landed the way it meant.
#[test]
fn the_tool_run_summarizes_the_plan_it_understood() {
    let cwd = temp_dir("update_plan_run");
    let registry = ToolRegistry::builtin();
    let result = registry
        .execute(
            &ToolCall {
                id: fresh_id("tool"),
                name: UPDATE_PLAN_TOOL.to_string(),
                input: input(&[
                    ("title", "Model plan documents"),
                    (
                        "plan",
                        r#"[{"step":"a","status":"completed"},{"step":"b","status":"pending"}]"#,
                    ),
                ]),
            },
            &ToolExecutionContext::local(cwd),
        )
        .expect("a well-formed call runs");
    assert!(result.success);
    assert!(
        result.output.contains("Model plan documents"),
        "{}",
        result.output
    );
    assert!(result.output.contains("2 step(s)"), "{}", result.output);
    assert!(result.output.contains("1 pending"), "{}", result.output);
    assert!(result.output.contains("1 completed"), "{}", result.output);
    assert!(result.diff.is_none(), "a plan call changes no file");
}

/// A malformed call fails the tool rather than recording half a plan.
#[test]
fn a_malformed_call_fails_the_tool() {
    let cwd = temp_dir("update_plan_run_bad");
    let registry = ToolRegistry::builtin();
    let error = registry
        .execute(
            &ToolCall {
                id: fresh_id("tool"),
                name: UPDATE_PLAN_TOOL.to_string(),
                input: input(&[("title", "No steps at all")]),
            },
            &ToolExecutionContext::local(cwd),
        )
        .expect_err("a call with no plan is a tool error");
    assert!(error.contains("requires `plan`"), "{error}");
}
