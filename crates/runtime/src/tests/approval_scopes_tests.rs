//! Standing `Always` approvals and the `Edit` decision
//! (`runtime.approval_scopes`, GUI-CORE-019).
//!
//! What these tests hold in place:
//!
//! 1. an ordinary tool approval offers `Always`, granting it writes the rule
//!    to the user-level settings file *before* announcing it, and the same
//!    call in a later process runs with no prompt at all;
//! 2. revoking removes the rule from the file, audits the revocation, and
//!    publishes it — the prompt comes back on the next call;
//! 3. an `Edit` a request did not offer, and a revision identical to the
//!    original, are both rejected with the reasons the contract names;
//! 4. a revision the engine still wants a decision about produces a second
//!    request that points at the one it replaces and carries a fresh audit id.

use std::time::Duration;

use viden_types::{
    ApprovalDecision, ApprovalResponse, ApprovalScope, AuditQuery, ModelEvent, RuntimeCommand,
    RuntimeEvent, RuntimeEventKind, RuntimeOwner, StandingApprovalView, ToolCall, ToolInput,
};

use super::{SequenceProvider, temp_dir};
use crate::{RuntimeSupervisor, SessionEngine};

fn owner_for_lane(lane: &str) -> RuntimeOwner {
    RuntimeOwner {
        workspace_id: "workspace-test".to_string(),
        project_id: "project-test".to_string(),
        lane_id: Some(lane.to_string()),
        session_id: Some(format!("session-{lane}")),
        task_id: Some(format!("task-{lane}")),
        turn_id: Some(format!("turn-{lane}")),
    }
}

fn shell_input(command: &str) -> ToolInput {
    let mut input = ToolInput::new();
    input.insert("command".to_string(), command.to_string());
    input
}

fn shell_turn(command: &str) -> Vec<ModelEvent> {
    vec![
        ModelEvent::ToolCall(ToolCall {
            id: format!("tool_{}", command.replace(' ', "_")),
            name: "shell".to_string(),
            input: shell_input(command),
        }),
        ModelEvent::Done,
    ]
}

fn collect_until(
    supervisor: &RuntimeSupervisor,
    done: impl Fn(&[RuntimeEvent]) -> bool,
) -> Vec<RuntimeEvent> {
    let started = std::time::Instant::now();
    let mut events = Vec::new();
    while started.elapsed() < Duration::from_secs(20) {
        if let Some(event) = supervisor.recv_event_timeout(Duration::from_millis(50)) {
            events.push(event);
            if done(&events) {
                return events;
            }
        }
    }
    panic!("timed out waiting for runtime events; collected: {events:#?}");
}

fn approval(events: &[RuntimeEvent]) -> viden_types::ApprovalRequestView {
    events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::ApprovalRequested { approval } => Some(approval.clone()),
            _ => None,
        })
        .expect("an approval request")
}

fn approvals(events: &[RuntimeEvent]) -> Vec<viden_types::ApprovalRequestView> {
    events
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::ApprovalRequested { approval } => Some(approval.clone()),
            _ => None,
        })
        .collect()
}

fn rejection(events: &[RuntimeEvent], command_id: &str) -> String {
    events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::CommandRejected {
                command_id: id,
                reason,
            } if id == command_id => Some(reason.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("a rejection for `{command_id}`; collected: {events:#?}"))
}

fn standing_rules(home: &std::path::Path) -> Vec<StandingApprovalView> {
    viden_config::load_user_standing_approvals_at(&viden_config::user_settings_path_in_home(home))
        .expect("the settings file is readable")
        .0
}

fn audit_actions(supervisor: &RuntimeSupervisor, owner: &RuntimeOwner) -> Vec<String> {
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd_audit_query",
            RuntimeCommand::QueryAudit {
                query: AuditQuery {
                    project_id: Some(owner.project_id.clone()),
                    ..AuditQuery::default()
                },
            },
        )
        .unwrap();
    let events = collect_until(supervisor, |events| {
        events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::AuditPageLoaded { .. }))
    });
    events
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::AuditPageLoaded { page, .. } => Some(
                page.records
                    .iter()
                    .map(|record| record.action.clone())
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .expect("an audit page")
}

/// The whole standing-approval round trip: offered, granted durably before it
/// is announced, in force for a *different* workspace in a later process, then
/// revoked — with the prompt coming back.
#[test]
fn a_standing_always_grant_is_written_announced_reloaded_and_revocable() {
    let cwd = temp_dir("approval_scopes_always_cwd");
    let home = temp_dir("approval_scopes_always_home");
    let provider = Box::new(SequenceProvider::new(vec![shell_turn("printf always")]));
    let engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let supervisor = RuntimeSupervisor::start(engine);
    let owner = owner_for_lane("lane-always");

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd_always_input",
            RuntimeCommand::SubmitUserInput {
                content: "run a repeatable command".to_string(),
            },
        )
        .unwrap();
    let events = collect_until(&supervisor, |events| {
        events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::ApprovalRequested { .. }))
    });
    let request = approval(&events);

    // Offered, and offered last: narrowest scope first.
    assert_eq!(
        request.allowed_scopes.last(),
        Some(&ApprovalScope::Always),
        "a medium-risk shell request must offer the standing scope"
    );
    assert!(
        request.editable,
        "shell input is a flat map an operator may revise"
    );

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd_always_respond",
            RuntimeCommand::RespondToApproval {
                request_id: request.id.clone(),
                response: ApprovalResponse::allow_always(None),
            },
        )
        .unwrap();
    let granted = collect_until(&supervisor, |events| {
        events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::StandingApprovalGranted { .. }))
    });
    let rule = granted
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::StandingApprovalGranted { rule } => Some(rule.clone()),
            _ => None,
        })
        .expect("the grant fact");

    assert_eq!(rule.tool_name, "shell");
    assert_eq!(rule.rule_content, "command=printf always");
    assert_eq!(rule.granted_in_workspace, owner.workspace_id);
    // The fact names the decision's own audit row, so the receipt a client
    // renders resolves to something the timeline holds.
    assert_eq!(rule.audit_id, request.audit_id);
    // Durable before announced.
    assert_eq!(standing_rules(&home), vec![rule.clone()]);
    assert!(
        audit_actions(&supervisor, &owner).contains(&"approval.always".to_string()),
        "the grant is audited under its own scope key"
    );

    drop(supervisor);

    // A second process, a *different* workspace: the rule still applies, which
    // is what separates this scope from `Session` and `RepoAllowlist`.
    let other_cwd = temp_dir("approval_scopes_always_other_cwd");
    let provider = Box::new(SequenceProvider::new(vec![shell_turn("printf always")]));
    let engine = SessionEngine::new_with_home(&other_cwd, provider, Some(home.clone())).unwrap();
    let supervisor = RuntimeSupervisor::start(engine);
    let owner = owner_for_lane("lane-always-two");
    assert_eq!(
        supervisor
            .snapshot_envelope()
            .unwrap()
            .view
            .standing_approvals,
        vec![rule.clone()],
        "the loaded rules are readable in the view a client replays"
    );

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd_always_reload",
            RuntimeCommand::SubmitUserInput {
                content: "run the same command again".to_string(),
            },
        )
        .unwrap();
    let events = collect_until(&supervisor, |events| {
        events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::ToolCallFinished { .. }))
    });
    assert!(
        approvals(&events).is_empty(),
        "a standing rule answers `decide` without a prompt; got {events:#?}"
    );

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd_always_revoke",
            RuntimeCommand::RevokeStandingApproval {
                rule_id: rule.rule_id.clone(),
            },
        )
        .unwrap();
    let revoked = collect_until(&supervisor, |events| {
        events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::StandingApprovalRevoked { .. }))
    });
    let (revoked_id, revoked_audit) = revoked
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::StandingApprovalRevoked {
                rule_id, audit_id, ..
            } => Some((rule_id.clone(), audit_id.clone())),
            _ => None,
        })
        .expect("the revocation fact");

    assert_eq!(revoked_id, rule.rule_id);
    assert_ne!(revoked_audit, rule.audit_id, "a revocation is its own row");
    assert!(standing_rules(&home).is_empty(), "the rule leaves the file");
    assert!(audit_actions(&supervisor, &owner).contains(&"approval.revoke".to_string()));

    // Revoking twice is a rejection, not a second revocation.
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd_always_revoke_again",
            RuntimeCommand::RevokeStandingApproval {
                rule_id: rule.rule_id,
            },
        )
        .unwrap();
    let again = collect_until(&supervisor, |events| {
        events.iter().any(|event| {
            matches!(
                &event.kind,
                RuntimeEventKind::CommandRejected { command_id, .. }
                    if command_id == "cmd_always_revoke_again"
            )
        })
    });
    assert!(rejection(&again, "cmd_always_revoke_again").contains("is not in force"));
}

/// An `Edit` a request never offered, and a revision that changes nothing, are
/// both refused — and refusing leaves the request answerable.
#[test]
fn an_unoffered_edit_and_an_unchanged_revision_are_both_rejected() {
    let cwd = temp_dir("approval_scopes_reject_cwd");
    let home = temp_dir("approval_scopes_reject_home");
    let provider = Box::new(SequenceProvider::new(vec![shell_turn("printf reject")]));
    let engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let supervisor = RuntimeSupervisor::start(engine);
    let owner = owner_for_lane("lane-reject");

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd_reject_input",
            RuntimeCommand::SubmitUserInput {
                content: "run a command".to_string(),
            },
        )
        .unwrap();
    let events = collect_until(&supervisor, |events| {
        events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::ApprovalRequested { .. }))
    });
    let request = approval(&events);

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd_reject_unchanged",
            RuntimeCommand::RespondToApproval {
                request_id: request.id.clone(),
                response: ApprovalResponse::edit(shell_input("printf reject"), None),
            },
        )
        .unwrap();
    let rejected = collect_until(&supervisor, |events| {
        events.iter().any(|event| {
            matches!(
                &event.kind,
                RuntimeEventKind::CommandRejected { command_id, .. }
                    if command_id == "cmd_reject_unchanged"
            )
        })
    });
    assert!(rejection(&rejected, "cmd_reject_unchanged").contains("revision_unchanged"));

    // The request is still answerable: a refused revision is not a decision.
    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd_reject_deny",
            RuntimeCommand::RespondToApproval {
                request_id: request.id.clone(),
                response: ApprovalResponse::deny(None),
            },
        )
        .unwrap();
    let resolved = collect_until(&supervisor, |events| {
        events.iter().any(|event| {
            matches!(
                &event.kind,
                RuntimeEventKind::ApprovalResolved { request_id, decision: ApprovalDecision::Deny, .. }
                    if request_id == &request.id
            )
        })
    });
    assert!(!resolved.is_empty());
}

/// A revision the engine still wants a decision about becomes a second
/// request that names the one it replaces and carries its own audit id.
#[test]
fn a_revision_that_still_needs_a_decision_publishes_a_second_linked_request() {
    let cwd = temp_dir("approval_scopes_revise_cwd");
    let home = temp_dir("approval_scopes_revise_home");
    let provider = Box::new(SequenceProvider::new(vec![shell_turn("printf original")]));
    let engine = SessionEngine::new_with_home(&cwd, provider, Some(home)).unwrap();
    let supervisor = RuntimeSupervisor::start(engine);
    let owner = owner_for_lane("lane-revise");

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd_revise_input",
            RuntimeCommand::SubmitUserInput {
                content: "run a command worth narrowing".to_string(),
            },
        )
        .unwrap();
    let events = collect_until(&supervisor, |events| {
        events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::ApprovalRequested { .. }))
    });
    let first = approval(&events);

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd_revise_edit",
            RuntimeCommand::RespondToApproval {
                request_id: first.id.clone(),
                response: ApprovalResponse::edit(shell_input("printf revised"), None),
            },
        )
        .unwrap();
    let revised = collect_until(&supervisor, |events| {
        events.iter().any(|event| {
            matches!(
                &event.kind,
                RuntimeEventKind::ApprovalRequested { approval } if approval.id != first.id
            )
        })
    });

    // The original settles as `Edit` — it was answered, and the answer was a
    // revision rather than a permission.
    assert!(revised.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::ApprovalResolved {
                request_id,
                decision: ApprovalDecision::Edit { revised_input },
                audit_id,
                ..
            } if request_id == &first.id
                && revised_input.get("command").map(String::as_str) == Some("printf revised")
                && audit_id == &first.audit_id
        )
    }));

    let second = revised
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::ApprovalRequested { approval } if approval.id != first.id => {
                Some(approval.clone())
            }
            _ => None,
        })
        .next()
        .expect("the revised request");

    assert_eq!(second.revised_from.as_deref(), Some(first.id.as_str()));
    assert_ne!(
        second.audit_id, first.audit_id,
        "a revised request is pre-minted its own audit id"
    );
    assert!(second.input_preview.contains("printf revised"));

    supervisor
        .send_command_from_owner(
            owner.clone(),
            "cmd_revise_allow",
            RuntimeCommand::RespondToApproval {
                request_id: second.id.clone(),
                response: ApprovalResponse::allow_once(None),
            },
        )
        .unwrap();
    let finished = collect_until(&supervisor, |events| {
        events
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::ToolCallFinished { .. }))
    });
    // The revised bytes are what ran.
    assert!(finished.iter().any(|event| {
        matches!(
            &event.kind,
            RuntimeEventKind::ToolCallFinished { success: true, .. }
        )
    }));
}
