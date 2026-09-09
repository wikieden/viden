//! The DiffReview action side, projected from Core's operator source-control
//! actions (`runtime.operator_git`, GUI-CORE-020).
//!
//! Four rules are under test, and each exists because skipping it produces a
//! specific lie on screen:
//!
//! 1. **One action in flight, correlated by `command_id`.** Both events that
//!    can settle an action — `OperatorGitActionFinished` and
//!    `CommandRejected` — name the command they answer, so an event carrying
//!    another action's id is ignored outright.
//! 2. **Refused and failed stay distinct.** A `CommandRejected` is Core's
//!    pre-effect refusal, carried verbatim. A `Failed` outcome means the
//!    effect *was* attempted, and it is never rendered as a denial.
//! 3. **Nothing is inferred from output text.** The success line reads the
//!    resampled `source` Core put on the outcome; `output` is display text.
//! 4. **The owner is Core's, never the client's.** An action is sent only with
//!    the exact `RuntimeOwner` Core bound to the acting Lane; without one the
//!    command is refused locally rather than sent with a default owner, which
//!    would record an authorized mutation as belonging to nobody.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use viden_core::{
    ApprovalDefaultAction, ApprovalRequestView, ApprovalRisk, ApprovalScope, ApprovalTarget,
    EventCursor, FRONTEND_SCHEMA_V1, LaneRuntimeOwnerBinding, OperatorGitAction,
    OperatorGitFailureClass, OperatorGitOutcome, RuntimeCommand, RuntimeCommandEnvelope,
    RuntimeEvent, RuntimeEventEnvelope, RuntimeEventKind, RuntimeOwner, RuntimeSnapshot,
    RuntimeViewState, RuntimeWireEvent, SourceTarget, WorkspaceDiffPage, WorkspaceSourceStatus,
    WorkspaceSourceView,
};
use viden_gui::{GuiCoreAdapter, OPERATOR_GIT_CAPABILITY, OperatorGitIntent};

mod support;
use support::TestCoreClient;

const TIMEOUT: Duration = Duration::from_millis(10);
const LANE: &str = "lane-alpha";

/// The canonical snapshot with one Lane already bound to one runtime owner.
///
/// The binding is applied to the view rather than queued as an event so the
/// owner exists *before* the first action leaves, which is the order the
/// cockpit is in: a Lane is selected and bound long before its commit bar is
/// pressed. Queueing it would make the harness drain the settling events too.
fn view() -> RuntimeViewState {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../crates/types/tests/fixtures/frontend-contract-v1/multi-lane.json"
    ))
    .expect("fixture json");
    let snapshot: RuntimeSnapshot =
        serde_json::from_value(fixture["initial_snapshot"].clone()).expect("fixture snapshot");
    RuntimeViewState::new(snapshot)
}

fn view_with_lane_owner() -> RuntimeViewState {
    let mut view = view();
    view.apply_event(&RuntimeEvent::new(
        1,
        RuntimeEventKind::LaneRuntimeOwnerBound {
            binding: LaneRuntimeOwnerBinding {
                lane_id: LANE.to_string(),
                owner: lane_owner(),
            },
        },
    ));
    view
}

fn lane_owner() -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace_contract_v1".to_string(),
        project_id: "project_viden".to_string(),
        lane_id: Some(LANE.to_string()),
        session_id: Some("session-alpha".to_string()),
        task_id: Some("task-alpha".to_string()),
        turn_id: None,
    }
}

fn envelope(sequence: u64, kind: RuntimeEventKind) -> RuntimeEventEnvelope {
    RuntimeEventEnvelope {
        schema_version: FRONTEND_SCHEMA_V1,
        owner: RuntimeOwner::default(),
        cursor: EventCursor {
            stream_id: "gui-test".to_string(),
            sequence,
        },
        event: RuntimeWireEvent::Known(RuntimeEvent::with_timestamp(
            sequence,
            Some(1_700_000_000 + sequence),
            kind,
        )),
    }
}

fn source(ahead: u32, dirty: bool) -> WorkspaceSourceView {
    WorkspaceSourceView {
        status: WorkspaceSourceStatus::Ready,
        branch: Some("claude/gui-diff-review-actions".to_string()),
        worktree: Some("/workspace/viden".to_string()),
        ahead,
        behind: 0,
        added: 0,
        deleted: 0,
        dirty,
    }
}

fn finished(
    sequence: u64,
    command_id: &str,
    action: OperatorGitAction,
    outcome: OperatorGitOutcome,
) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::OperatorGitActionFinished {
            command_id: command_id.to_string(),
            target: SourceTarget::Lane {
                lane_id: LANE.to_string(),
            },
            action,
            outcome,
            audit_id: "audit_operator_git".to_string(),
        },
    )
}

/// The diff page the open review is showing when an action runs.
fn loaded(sequence: u64, command_id: &str) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::WorkspaceDiffLoaded {
            command_id: command_id.to_string(),
            page: WorkspaceDiffPage {
                target: SourceTarget::Lane {
                    lane_id: LANE.to_string(),
                },
                source: source(1, true),
                entries: Vec::new(),
                truncated: false,
            },
        },
    )
}

fn refused(sequence: u64, command_id: &str, reason: &str) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::CommandRejected {
            command_id: command_id.to_string(),
            reason: reason.to_string(),
        },
    )
}

/// The `Ask` Core routes through the supervisor queue for a commit, exactly as
/// the canonical `operator-git.json` fixture shapes it.
fn git_approval(sequence: u64) -> RuntimeEventEnvelope {
    envelope(
        sequence,
        RuntimeEventKind::ApprovalRequested {
            approval: ApprovalRequestView {
                id: "approval_operator_git_commit".to_string(),
                tool_name: "git_commit".to_string(),
                title: "Approve git_commit".to_string(),
                message: "git_commit requires approval".to_string(),
                input_preview: "  message: feat: land the commit bar".to_string(),
                is_mutating: true,
                reason: Some("git_commit requires approval".to_string()),
                owner: lane_owner(),
                risk: ApprovalRisk::Medium,
                target: ApprovalTarget {
                    kind: "git".to_string(),
                    display: "git_commit (lane:lane-alpha)".to_string(),
                    canonical_ref: Some("lane:lane-alpha".to_string()),
                },
                allowed_scopes: vec![ApprovalScope::Once],
                policy_reason_key: "permission.requires_approval".to_string(),
                policy_reason_args: Default::default(),
                expires_at: 1_700_002_100,
                default_action: ApprovalDefaultAction::Deny,
                audit_id: "audit_operator_git_commit".to_string(),
                decision_context: None,
            },
        },
    )
}

struct Harness {
    adapter: GuiCoreAdapter,
    sent: Arc<Mutex<Vec<RuntimeCommandEnvelope>>>,
}

fn harness(events: Vec<RuntimeEventEnvelope>, with_capability: bool) -> Harness {
    harness_with_view(view_with_lane_owner(), events, with_capability)
}

fn harness_with_view(
    view: RuntimeViewState,
    events: Vec<RuntimeEventEnvelope>,
    with_capability: bool,
) -> Harness {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut client = TestCoreClient::new(view, sent.clone());
    if !with_capability {
        client.capabilities.remove(OPERATOR_GIT_CAPABILITY);
    }
    for event in events {
        client = client.with_envelope(event);
    }
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect");
    Harness { adapter, sent }
}

fn operator_commands(
    sent: &Arc<Mutex<Vec<RuntimeCommandEnvelope>>>,
) -> Vec<RuntimeCommandEnvelope> {
    sent.lock()
        .expect("sent lock")
        .iter()
        .filter(|envelope| {
            matches!(
                envelope.command,
                RuntimeCommand::RunOperatorGitAction { .. }
            )
        })
        .cloned()
        .collect()
}

#[test]
fn an_action_travels_with_the_exact_core_owner_on_both_halves_of_the_envelope() {
    let harness = harness(Vec::new(), true);
    let mut harness = harness;
    harness
        .adapter
        .run_operator_git_action_and_wait(
            "gui-git-1",
            Some(LANE),
            OperatorGitIntent::Stage { paths: Vec::new() },
            TIMEOUT,
        )
        .expect("stage sends");

    let commands = operator_commands(&harness.sent);
    assert_eq!(commands.len(), 1, "exactly one command left the client");
    let envelope = &commands[0];
    assert_eq!(envelope.command_id, "gui-git-1");
    assert_eq!(envelope.owner, lane_owner());
    let RuntimeCommand::RunOperatorGitAction {
        owner,
        target,
        action,
    } = &envelope.command
    else {
        panic!("expected RunOperatorGitAction");
    };
    // The supervisor refuses a command whose actor differs from its envelope
    // owner, so the two halves must be the same Core-published owner.
    assert_eq!(owner, &envelope.owner);
    assert_eq!(
        target,
        &SourceTarget::Lane {
            lane_id: LANE.to_string()
        }
    );
    assert_eq!(action, &OperatorGitAction::Stage { paths: Vec::new() });
}

#[test]
fn an_action_without_an_exact_core_owner_is_refused_locally_rather_than_sent() {
    let mut harness = harness(Vec::new(), true);
    let error = harness
        .adapter
        .run_operator_git_action_and_wait(
            "gui-git-1",
            None,
            OperatorGitIntent::Commit {
                message: "feat: something".to_string(),
            },
            TIMEOUT,
        )
        .expect_err("no owner");
    assert!(
        error.contains("owner"),
        "the refusal names the missing owner: {error}"
    );
    assert!(
        operator_commands(&harness.sent).is_empty(),
        "nothing is sent with a default owner"
    );

    let projection = harness.adapter.operator_git(None);
    assert!(!projection.owner_available);
    assert!(projection.owner_unavailable_reason.is_some());
}

#[test]
fn a_missing_capability_disables_the_bar_and_sends_nothing() {
    let mut harness = harness(Vec::new(), false);
    let projection = harness.adapter.operator_git(Some(LANE));
    assert!(!projection.capability_available);

    let error = harness
        .adapter
        .run_operator_git_action_and_wait(
            "gui-git-1",
            Some(LANE),
            OperatorGitIntent::Fetch { remote: None },
            TIMEOUT,
        )
        .expect_err("capability absent");
    assert!(error.contains(OPERATOR_GIT_CAPABILITY), "{error}");
    assert!(operator_commands(&harness.sent).is_empty());
}

#[test]
fn a_completed_outcome_carries_cores_resampled_source_and_never_parses_output() {
    let mut harness = harness(
        vec![finished(
            2,
            "gui-git-1",
            OperatorGitAction::Commit {
                message: "feat(gui): land the commit bar".to_string(),
            },
            OperatorGitOutcome::Completed {
                output: "[claude/gui-diff-review-actions 1a2b3c4] feat(gui): land the commit \
                             bar\n 1 file changed, 2 insertions(+)"
                    .to_string(),
                truncated: true,
                source: source(2, false),
            },
        )],
        true,
    );
    let projection = harness
        .adapter
        .run_operator_git_action_and_wait(
            "gui-git-1",
            Some(LANE),
            OperatorGitIntent::Commit {
                message: "feat(gui): land the commit bar".to_string(),
            },
            TIMEOUT,
        )
        .expect("commit settles");

    assert_eq!(projection.outcome.state, "confirmed");
    assert_eq!(projection.pending_command_id, None);
    assert_eq!(projection.pending_action, None);
    let result = projection.result.expect("Core answered");
    assert_eq!(result.kind, "completed");
    assert_eq!(result.action, "commit");
    assert_eq!(result.audit_id, "audit_operator_git");
    assert!(result.truncated, "the 8 KiB bound cut the output");
    assert!(result.failure_class.is_none());
    // The chip reads the resampled facts, never the transcript text.
    let source = result.source.expect("resampled source");
    assert_eq!(source.ahead, 2);
    assert!(!source.dirty);
    assert_eq!(result.target_lane_id.as_deref(), Some(LANE));
}

#[test]
fn a_failed_outcome_is_a_classified_attempt_not_a_refusal() {
    let mut harness = harness(
        vec![finished(
            2,
            "gui-git-1",
            OperatorGitAction::Push {
                remote: None,
                set_upstream: false,
            },
            OperatorGitOutcome::Failed {
                class: OperatorGitFailureClass::NoUpstream,
                detail: "the current branch has no upstream branch; push with set_upstream \
                             to create one"
                    .to_string(),
            },
        )],
        true,
    );
    let projection = harness
        .adapter
        .run_operator_git_action_and_wait(
            "gui-git-1",
            Some(LANE),
            OperatorGitIntent::Push {
                remote: None,
                set_upstream: false,
            },
            TIMEOUT,
        )
        .expect("push settles");

    // Confirmed: Core answered the command. The *outcome* is the failure, and
    // the two are deliberately not collapsed.
    assert_eq!(projection.outcome.state, "confirmed");
    assert!(projection.outcome.reason.is_none());
    let result = projection.result.expect("Core answered");
    assert_eq!(result.kind, "failed");
    assert_eq!(result.action, "push");
    assert_eq!(result.failure_class, Some("no_upstream"));
    assert!(
        result
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("set_upstream")),
        "git's own words are kept verbatim"
    );
    assert!(result.source.is_none(), "a failure resamples nothing");
}

#[test]
fn a_pre_effect_refusal_is_cores_own_reason_verbatim() {
    let mut harness = harness(
        vec![refused(
            2,
            "gui-git-1",
            "permission denied\ntool: git_add\nreason: DenyRule\nmessage: git_add is denied \
                 by a workspace rule\nhint: grant the `git_add` permission to run source-control \
                 actions from this client",
        )],
        true,
    );
    let projection = harness
        .adapter
        .run_operator_git_action_and_wait(
            "gui-git-1",
            Some(LANE),
            OperatorGitIntent::Stage {
                paths: vec!["crates/types/src/source_control.rs".to_string()],
            },
            TIMEOUT,
        )
        .expect("stage settles");

    assert_eq!(projection.outcome.state, "rejected");
    assert!(
        projection
            .outcome
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("git_add is denied by a workspace rule")),
        "Core's words, unedited"
    );
    assert!(
        projection.result.is_none(),
        "a refusal produced no attempted effect to report"
    );
}

#[test]
fn an_event_naming_another_action_never_settles_this_one() {
    let mut harness = harness(
        vec![finished(
            2,
            "gui-git-other",
            OperatorGitAction::Fetch { remote: None },
            OperatorGitOutcome::Completed {
                output: String::new(),
                truncated: false,
                source: source(0, false),
            },
        )],
        true,
    );
    let projection = harness
        .adapter
        .run_operator_git_action_and_wait(
            "gui-git-1",
            Some(LANE),
            OperatorGitIntent::Fetch { remote: None },
            TIMEOUT,
        )
        .expect("fetch stays out");

    assert_eq!(projection.outcome.state, "pending");
    assert_eq!(projection.pending_command_id.as_deref(), Some("gui-git-1"));
    assert_eq!(projection.pending_action, Some("fetch"));
    assert!(projection.result.is_none());
}

#[test]
fn a_second_action_is_refused_while_one_is_still_in_flight() {
    let mut harness = harness(Vec::new(), true);
    harness
        .adapter
        .run_operator_git_action_and_wait(
            "gui-git-1",
            Some(LANE),
            OperatorGitIntent::Commit {
                message: "feat: one".to_string(),
            },
            TIMEOUT,
        )
        .expect("first action");
    let error = harness
        .adapter
        .run_operator_git_action_and_wait(
            "gui-git-2",
            Some(LANE),
            OperatorGitIntent::Push {
                remote: None,
                set_upstream: false,
            },
            TIMEOUT,
        )
        .expect_err("one action at a time");
    assert!(error.contains("gui-git-1"), "{error}");
    assert_eq!(operator_commands(&harness.sent).len(), 1);
}

#[test]
fn a_pending_git_approval_for_the_acting_owner_marks_the_bar_awaiting() {
    let mut harness = harness(vec![git_approval(1)], true);
    let projection = harness
        .adapter
        .run_operator_git_action_and_wait(
            "gui-git-1",
            Some(LANE),
            OperatorGitIntent::Commit {
                message: "feat: land the commit bar".to_string(),
            },
            TIMEOUT,
        )
        .expect("commit waits on the dock");

    assert_eq!(projection.outcome.state, "pending");
    assert!(
        projection.awaiting_approval,
        "Core published a `git` approval for this owner; the bar says so and stays inert"
    );
}

#[test]
fn an_empty_commit_message_is_refused_by_cores_own_validator_before_anything_is_sent() {
    let mut harness = harness(Vec::new(), true);
    let error = harness
        .adapter
        .run_operator_git_action_and_wait(
            "gui-git-1",
            Some(LANE),
            OperatorGitIntent::Commit {
                message: "   ".to_string(),
            },
            TIMEOUT,
        )
        .expect_err("empty message");
    assert!(error.contains("non-empty message"), "{error}");
    assert!(operator_commands(&harness.sent).is_empty());
}

#[test]
fn a_path_that_leaves_the_target_is_refused_rather_than_clamped() {
    let mut harness = harness(Vec::new(), true);
    let error = harness
        .adapter
        .run_operator_git_action_and_wait(
            "gui-git-1",
            Some(LANE),
            OperatorGitIntent::Unstage {
                paths: vec!["../outside.rs".to_string()],
            },
            TIMEOUT,
        )
        .expect_err("escaping path");
    assert!(error.contains("outside the repository target"), "{error}");
    assert!(operator_commands(&harness.sent).is_empty());
}

/// The re-read rule for the open review, end to end.
///
/// Core emits `WorkspaceSourceUpdated` right after `OperatorGitActionFinished`.
/// That fact is what invalidates a diff page, so the action's own drain must
/// consume it: stopping at the settling event would leave the review showing
/// the pre-action tree until some unrelated poll happened to pick the update
/// up. The client still re-*reads* through the ordinary query — nothing is
/// patched into the rows from the action's answer.
#[test]
fn the_source_update_that_follows_an_action_marks_the_open_review_stale() {
    let mut harness = harness(
        vec![
            loaded(1, "gui-diff-1"),
            finished(
                2,
                "gui-git-1",
                OperatorGitAction::Commit {
                    message: "feat(gui): land the commit bar".to_string(),
                },
                OperatorGitOutcome::Completed {
                    output: "1 file changed".to_string(),
                    truncated: false,
                    source: source(2, false),
                },
            ),
            envelope(
                3,
                RuntimeEventKind::WorkspaceSourceUpdated {
                    source: source(2, false),
                },
            ),
        ],
        true,
    );
    let page = harness
        .adapter
        .query_workspace_diff_and_wait("gui-diff-1", Some(LANE), TIMEOUT)
        .expect("diff read");
    assert!(page.loaded);
    assert!(!page.stale, "nothing has changed the tree yet");

    harness
        .adapter
        .run_operator_git_action_and_wait(
            "gui-git-1",
            Some(LANE),
            OperatorGitIntent::Commit {
                message: "feat(gui): land the commit bar".to_string(),
            },
            TIMEOUT,
        )
        .expect("commit settles");

    assert!(
        harness.adapter.workspace_diff().stale,
        "the post-action source update invalidates the page the review is showing"
    );
}
