//! Always and Edit in the permission dock (`runtime.approval_scopes`, C15).
//!
//! Every state is replayed from the canonical `approval-scopes.json` fixture
//! rather than hand-built, so the dock is measured against the wire truth Core
//! froze: the standing grant and its revocation, the `Edit` that produces a
//! `revised_from` request, and the high-risk request whose `allowed_scopes`
//! deliberately lacks `Always`.

use std::sync::{Arc, Mutex};

use viden_core::{RuntimeCommand, RuntimeCommandEnvelope, RuntimeSnapshot, RuntimeViewState};
use viden_gui::{GuiCoreAdapter, PermissionChoice, PermissionIntent};

mod support;
use support::TestCoreClient;

const SCOPES_FIXTURE: &str =
    include_str!("../../../crates/types/tests/fixtures/frontend-contract-v1/approval-scopes.json");

#[derive(serde::Deserialize)]
struct Fixture {
    initial_snapshot: RuntimeSnapshot,
    events: Vec<viden_core::RuntimeEventEnvelope>,
}

fn replay(events: usize) -> RuntimeViewState {
    let fixture: Fixture = serde_json::from_str(SCOPES_FIXTURE).unwrap();
    let mut view = RuntimeViewState::new(fixture.initial_snapshot);
    for envelope in fixture.events.into_iter().take(events) {
        if let viden_core::RuntimeWireEvent::Known(event) = envelope.event {
            view.apply_event(&event);
        }
    }
    view
}

fn connected(view: RuntimeViewState) -> GuiCoreAdapter {
    let mut adapter = GuiCoreAdapter::new(Box::new(TestCoreClient::new(
        view,
        Arc::new(Mutex::new(Vec::new())),
    )));
    adapter.connect().unwrap();
    adapter
}

fn connected_recording(
    view: RuntimeViewState,
) -> (GuiCoreAdapter, Arc<Mutex<Vec<RuntimeCommandEnvelope>>>) {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = GuiCoreAdapter::new(Box::new(TestCoreClient::new(view, sent.clone())));
    adapter.connect().unwrap();
    (adapter, sent)
}

fn connected_without_capability(view: RuntimeViewState) -> GuiCoreAdapter {
    let mut client = TestCoreClient::new(view, Arc::new(Mutex::new(Vec::new())));
    client
        .capabilities
        .remove(viden_gui::APPROVAL_SCOPES_CAPABILITY);
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().unwrap();
    adapter
}

fn action<'a>(
    request: &'a viden_gui::PermissionRequestProjection,
    kind: &str,
) -> &'a viden_gui::PermissionActionProjection {
    request
        .actions
        .iter()
        .find(|item| item.kind == kind)
        .unwrap_or_else(|| panic!("the option row is missing `{kind}`"))
}

/// Prefix 1: the medium-risk shell request Core offered the standing scope on.
#[test]
fn the_dock_offers_always_exactly_where_core_offered_the_scope() {
    let dock = connected(replay(1)).permission_dock().expect("dock");
    let request = dock.request.expect("one pending approval");
    assert_eq!(request.id, "approval_scopes_grant");
    let always = action(&request, "always");
    assert!(always.available);
    assert!(always.code.is_none());
}

/// Prefix 19: the high-risk `git_push` request. Core never offers a standing
/// rule above medium risk, so the tier is drawn with Core's own refusal name
/// rather than hidden or offered.
#[test]
fn a_request_without_the_scope_names_core_s_own_refusal() {
    let dock = connected(replay(19)).permission_dock().expect("dock");
    let request = dock.request.expect("one pending approval");
    assert_eq!(request.id, "approval_scopes_high");
    let always = action(&request, "always");
    assert!(!always.available);
    assert_eq!(always.code, Some("scope_not_offered"));
}

/// Editability is Core's answer, and the fields are recovered from Core's own
/// preview rendering. `apply_patch` is never editable; `shell` is.
#[test]
fn edit_is_offered_only_where_core_marked_the_request_editable() {
    let dock = connected(replay(13)).permission_dock().expect("dock");
    let request = dock.request.expect("one pending approval");
    assert_eq!(request.id, "approval_scopes_wide");
    assert!(request.editable);
    assert!(action(&request, "edit").available);
    assert_eq!(
        request
            .input_fields
            .iter()
            .map(|field| (field.key.as_str(), field.value.as_str()))
            .collect::<Vec<_>>(),
        vec![("command", "rm -rf target")]
    );
    assert!(request.edit_block.is_none());
}

/// A preview this client cannot parse exactly withdraws the control and names
/// the register entry rather than offering a revision built from a guess.
#[test]
fn an_unparseable_preview_withdraws_edit_with_its_own_code() {
    let mut view = replay(13);
    let approval = view
        .pending_approvals
        .iter_mut()
        .find(|request| request.id == "approval_scopes_wide")
        .expect("the editable request");
    approval.input_preview = "rm -rf target".to_string();

    let dock = connected(view).permission_dock().expect("dock");
    let request = dock.request.expect("one pending approval");
    assert!(request.editable);
    assert!(request.input_fields.is_empty());
    assert_eq!(request.edit_block, Some("GUI-CORE-035"));
    let edit = action(&request, "edit");
    assert!(!edit.available);
    assert_eq!(edit.code, Some("GUI-CORE-035"));
}

/// Prefix 15: the request Core produced from the `Edit`, carrying the request
/// it replaces so the dock can link the two rather than stack them.
#[test]
fn a_revision_carries_the_request_it_replaces() {
    let dock = connected(replay(15)).permission_dock().expect("dock");
    let request = dock.request.expect("one pending approval");
    assert_eq!(request.id, "approval_scopes_revised");
    assert_eq!(
        request.revised_from.as_deref(),
        Some("approval_scopes_wide")
    );
    assert_eq!(request.input_fields[0].value, "rm -rf target/debug");
}

/// Prefix 3: the standing rule the grant installed, listed with Core's own
/// rule key and the audit row the grant wrote.
#[test]
fn the_standing_list_is_core_s_own_rules() {
    let standing = connected(replay(3)).standing_approvals();
    assert!(standing.capability_available);
    assert_eq!(standing.rules.len(), 1);
    let rule = &standing.rules[0];
    assert_eq!(rule.rule_id, "rule_approval_scopes");
    assert_eq!(rule.tool_name, "shell");
    assert_eq!(rule.rule_content, "command=cargo fmt --all -- --check");
    assert_eq!(rule.audit_id, "audit_approval_scopes_always");
}

/// Prefix 10: the revocation removed the rule, so the list is empty *and* the
/// capability is still there. Empty and unanswerable are different sentences.
#[test]
fn an_empty_list_is_not_an_absent_capability() {
    let revoked = connected(replay(10)).standing_approvals();
    assert!(revoked.capability_available);
    assert!(revoked.rules.is_empty());

    let absent = connected_without_capability(replay(3)).standing_approvals();
    assert!(!absent.capability_available);
}

/// Always travels as `Allow { scope: Always }` under the owner Core published
/// on the request.
#[test]
fn always_sends_the_standing_scope_core_offered() {
    let view = replay(1);
    let owner = view.pending_approvals[0].owner.clone();
    let (mut adapter, sent) = connected_recording(view);
    adapter
        .send_permission_intent_and_wait(
            "gui-perm-always-1",
            PermissionIntent::Respond {
                request_id: "approval_scopes_grant".to_string(),
                choice: PermissionChoice::Always,
                feedback: None,
                revised_input: None,
            },
            std::time::Duration::ZERO,
        )
        .expect("always is sent");
    let sent = sent.lock().unwrap();
    let envelope = sent.last().expect("one command");
    assert_eq!(envelope.owner, owner);
    let RuntimeCommand::RespondToApproval { response, .. } = &envelope.command else {
        panic!("an approval response");
    };
    assert!(matches!(
        &response.decision,
        viden_core::ApprovalDecision::Allow {
            scope: viden_core::ApprovalScope::Always
        }
    ));
}

/// A scope Core did not offer is refused locally rather than sent as a command
/// Core would answer with `scope_not_offered`.
#[test]
fn a_scope_core_did_not_offer_is_refused_before_it_is_sent() {
    let mut adapter = connected(replay(19));
    let error = adapter
        .send_permission_intent_and_wait(
            "gui-perm-always-2",
            PermissionIntent::Respond {
                request_id: "approval_scopes_high".to_string(),
                choice: PermissionChoice::Always,
                feedback: None,
                revised_input: None,
            },
            std::time::Duration::ZERO,
        )
        .expect_err("an unoffered scope is refused locally");
    assert!(error.contains("unavailable or ambiguous"));
}

/// Edit sends the whole revised input, and a revision on a request Core did
/// not mark editable is refused here rather than by Core.
#[test]
fn edit_sends_the_whole_revised_input_and_only_where_core_allows_it() {
    let (mut adapter, sent) = connected_recording(replay(13));
    adapter
        .send_permission_intent_and_wait(
            "gui-perm-edit-1",
            PermissionIntent::Respond {
                request_id: "approval_scopes_wide".to_string(),
                choice: PermissionChoice::Edit,
                feedback: None,
                revised_input: Some(
                    [("command".to_string(), "rm -rf target/debug".to_string())]
                        .into_iter()
                        .collect(),
                ),
            },
            std::time::Duration::ZERO,
        )
        .expect("edit is sent");
    {
        let sent = sent.lock().unwrap();
        let RuntimeCommand::RespondToApproval { response, .. } =
            &sent.last().expect("one command").command
        else {
            panic!("an approval response");
        };
        let viden_core::ApprovalDecision::Edit { revised_input } = &response.decision else {
            panic!("an Edit decision");
        };
        assert_eq!(
            revised_input.get("command").map(String::as_str),
            Some("rm -rf target/debug")
        );
    }

    // An Edit with no revision is refused: a revision *is* the decision. A
    // fresh adapter, because the dock holds one permission command at a time
    // and the first one above is still unanswered.
    let mut second = connected(replay(13));
    let error = second
        .send_permission_intent_and_wait(
            "gui-perm-edit-2",
            PermissionIntent::Respond {
                request_id: "approval_scopes_wide".to_string(),
                choice: PermissionChoice::Edit,
                feedback: None,
                revised_input: None,
            },
            std::time::Duration::ZERO,
        )
        .expect_err("an Edit without a revision is refused locally");
    assert!(error.contains("revised input"));
}

/// The revocation is refused without the capability and for a rule Core does
/// not publish, rather than sent as a command Core would have to reject.
#[test]
fn revocation_is_refused_locally_where_core_could_only_reject_it() {
    let mut absent = connected_without_capability(replay(3));
    let error = absent
        .revoke_standing_approval_and_wait(
            "gui-standing-revoke-1",
            "rule_approval_scopes",
            std::time::Duration::ZERO,
        )
        .expect_err("no capability means nothing is sent");
    assert!(error.contains(viden_gui::APPROVAL_SCOPES_CAPABILITY));

    let mut adapter = connected(replay(3));
    let error = adapter
        .revoke_standing_approval_and_wait(
            "gui-standing-revoke-2",
            "rule_missing",
            std::time::Duration::ZERO,
        )
        .expect_err("a rule Core does not publish is refused locally");
    assert!(error.contains("rule_missing"));
}

/// The revocation Core does publish travels as its own command.
#[test]
fn revocation_sends_core_s_own_command() {
    let (mut adapter, sent) = connected_recording(replay(3));
    adapter
        .revoke_standing_approval_and_wait(
            "gui-standing-revoke-3",
            "rule_approval_scopes",
            std::time::Duration::ZERO,
        )
        .expect("revocation is sent");
    let sent = sent.lock().unwrap();
    assert!(matches!(
        &sent.last().expect("one command").command,
        RuntimeCommand::RevokeStandingApproval { rule_id } if rule_id == "rule_approval_scopes"
    ));
}
