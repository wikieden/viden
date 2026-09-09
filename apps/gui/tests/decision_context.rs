//! Approval decision context and completed-change rows (GUI-CORE-012).
//!
//! Three sites consume Core's structured diff besides the DiffReview read:
//! the D1 permission dock, the D2 decision detail, and D1's own changed-file
//! checklist. All three used to declare the rows unavailable; the claim stops
//! the moment Core publishes them, and stays exactly where Core publishes
//! nothing.

use std::sync::{Arc, Mutex};

use viden_core::{
    ApprovalRequestView, DecisionContext, DiffDocument, DiffFile, DiffHunk, DiffLine, DiffLineKind,
    RuntimeCommandEnvelope, RuntimeOwner, RuntimeSnapshot, RuntimeViewState, WorkspaceChangeKind,
    WorkspaceChangeView,
};
use viden_gui::{GuiCoreAdapter, OPERATOR_GIT_CAPABILITY, STRUCTURED_DIFF_CAPABILITY};

mod support;
use support::TestCoreClient;

const APPROVAL_FIXTURE: &str = include_str!(
    "../../../crates/types/tests/fixtures/frontend-contract-v1/approval-allow-deny.json"
);

fn base() -> (RuntimeViewState, ApprovalRequestView) {
    let fixture: serde_json::Value = serde_json::from_str(APPROVAL_FIXTURE).unwrap();
    let snapshot: RuntimeSnapshot =
        serde_json::from_value(fixture["initial_snapshot"].clone()).unwrap();
    let view = RuntimeViewState::new(snapshot);
    let approval: ApprovalRequestView = serde_json::from_value(
        fixture["events"][0]["event"]["kind"]["payload"]["approval"].clone(),
    )
    .unwrap();
    (view, approval)
}

fn edit_diff() -> DiffDocument {
    DiffDocument {
        files: vec![DiffFile {
            path: "crates/types/src/diff.rs".to_string(),
            old_path: None,
            kind: WorkspaceChangeKind::Modified,
            binary: false,
            omitted: false,
            additions: 1,
            deletions: 1,
            hunks: vec![DiffHunk {
                old_start: 42,
                old_lines: 2,
                new_start: 42,
                new_lines: 2,
                header: None,
                lines: vec![
                    DiffLine {
                        kind: DiffLineKind::Removed,
                        content: "    pub truncated: bool,".to_string(),
                        old_line: Some(42),
                        new_line: None,
                    },
                    DiffLine {
                        kind: DiffLineKind::Added,
                        content: "    pub truncated: bool, // bounded".to_string(),
                        old_line: None,
                        new_line: Some(42),
                    },
                ],
            }],
        }],
        truncated: false,
        byte_limit: 65_536,
    }
}

fn connected(view: RuntimeViewState, with_capability: bool) -> GuiCoreAdapter {
    let sent: Arc<Mutex<Vec<RuntimeCommandEnvelope>>> = Arc::new(Mutex::new(Vec::new()));
    let mut client = TestCoreClient::new(view, sent);
    if !with_capability {
        client.capabilities.remove(STRUCTURED_DIFF_CAPABILITY);
    }
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect");
    adapter
}

#[test]
fn an_approval_with_a_decision_context_projects_its_rows_and_base_hash() {
    let (mut view, mut approval) = base();
    approval.decision_context = Some(DecisionContext {
        diff: Some(edit_diff()),
        base_sha256: Some(
            "3f79bb7b435b05321651daefd374cdc681dc06faa65e374e38337b88ca046dea".to_string(),
        ),
    });
    view.pending_approvals.push(approval);
    let adapter = connected(view, true);

    let request = adapter
        .permission_dock()
        .expect("dock")
        .request
        .expect("approval");
    let context = request.decision_context.expect("Core published a context");
    assert_eq!(
        context.base_sha256.as_deref(),
        Some("3f79bb7b435b05321651daefd374cdc681dc06faa65e374e38337b88ca046dea")
    );
    let diff = context.diff.expect("Core computed a diff");
    assert!(!diff.truncated);
    assert_eq!(diff.byte_limit, 65_536);
    let file = &diff.files[0];
    assert_eq!(file.path, "crates/types/src/diff.rs");
    assert_eq!(file.hunks[0].lines[0].kind, "removed");
    assert_eq!(file.hunks[0].lines[0].old_line, Some(42));
    assert_eq!(file.hunks[0].lines[0].new_line, None);
    // The preview stays beside the rows: it is Core's own tool-input summary
    // and the rows are a preview of the effect, not a replacement for it.
    assert_eq!(request.input_preview, "cargo test");
}

#[test]
fn an_approval_core_attached_no_context_to_keeps_the_preview_alone() {
    let (mut view, approval) = base();
    // The ordinary case: every tool but `edit_file`, `write_file`, and the
    // trust loop's `MergeAgentPatch` carries no context at all, because Core
    // cannot predict an external process's effect without running it.
    view.pending_approvals.push(approval);
    let adapter = connected(view, true);

    let request = adapter
        .permission_dock()
        .expect("dock")
        .request
        .expect("approval");
    assert!(request.decision_context.is_none());
    assert_eq!(request.input_preview, "cargo test");
}

#[test]
fn a_context_whose_diff_core_could_not_compute_is_present_but_empty_handed() {
    let (mut view, mut approval) = base();
    approval.decision_context = Some(DecisionContext {
        // Core attached a context and produced no diff for it. That is not
        // "no change" — the client must not render it as an empty diff.
        diff: None,
        base_sha256: None,
    });
    view.pending_approvals.push(approval);
    let adapter = connected(view, true);

    let context = adapter
        .permission_dock()
        .expect("dock")
        .request
        .expect("approval")
        .decision_context
        .expect("context");
    assert!(context.diff.is_none());
    assert!(context.base_sha256.is_none());
}

#[test]
fn the_d2_decision_detail_drops_the_unavailable_marker_only_when_rows_arrive() {
    let (mut view, mut approval) = base();
    let id = approval.id.clone();
    approval.decision_context = Some(DecisionContext {
        diff: Some(edit_diff()),
        base_sha256: None,
    });
    view.pending_approvals.push(approval);
    let adapter = connected(view, true);

    let detail = adapter
        .d2_decisions_for(&id)
        .expect("d2 projection")
        .detail
        .expect("selected decision");
    assert!(
        detail.context.unavailable.is_none(),
        "the marker is a claim about Core; it must go when the rows arrive"
    );
    let context = detail
        .decision_context
        .expect("D2 renders the same rows the dock does");
    assert_eq!(context.diff.expect("diff").files.len(), 1);
}

#[test]
fn the_d2_marker_stays_for_an_approval_core_published_no_context_for() {
    let (mut view, approval) = base();
    let id = approval.id.clone();
    view.pending_approvals.push(approval);
    let adapter = connected(view, true);

    let detail = adapter
        .d2_decisions_for(&id)
        .expect("d2 projection")
        .detail
        .expect("selected decision");
    assert_eq!(
        detail.context.unavailable.map(|note| note.code),
        Some("GUI-CORE-012")
    );
    assert!(detail.decision_context.is_none());
}

#[test]
fn a_completed_workspace_change_carries_its_rows_onto_the_checklist() {
    let (mut view, _) = base();
    let owner = view
        .pending_approvals
        .first()
        .map(|approval| approval.owner.clone())
        .unwrap_or_else(RuntimeOwner::default);
    view.workspace_changes.push(WorkspaceChangeView {
        id: "change-diff".to_string(),
        owner,
        path: "crates/types/src/diff.rs".to_string(),
        kind: WorkspaceChangeKind::Modified,
        patch: Some("--- a\n+++ b\n".to_string()),
        additions: 1,
        deletions: 1,
        diff: Some(edit_diff()),
    });
    let adapter = connected(view, true);

    let projection = adapter.d1_cockpit(None).expect("cockpit");
    // The `diff` unavailable row is a claim about Core. Core now publishes the
    // rows, so the claim goes rather than standing as a stale sentence.
    assert!(
        !projection
            .unavailable_features
            .iter()
            .any(|feature| feature.id == "diff"),
        "the GUI-CORE-012 row must not outlive the capability"
    );
    // `apply` is gone for the same reason: this Core advertises
    // `runtime.operator_git`, so the commit bar and the titlebar sync control
    // reach real commands and the row would be a stale claim.
    assert!(
        !projection
            .unavailable_features
            .iter()
            .any(|feature| feature.id == "apply"),
        "the GUI-CORE-020 row must not outlive the capability"
    );
}

/// The other half of the same rule: a Core that really publishes no operator
/// source-control actions still says so, and names the capability rather than
/// leaving the operator to discover an inert bar.
#[test]
fn a_core_without_operator_git_keeps_the_apply_unavailable_row() {
    let (view, _) = base();
    let sent: Arc<Mutex<Vec<RuntimeCommandEnvelope>>> = Arc::new(Mutex::new(Vec::new()));
    let mut client = TestCoreClient::new(view, sent);
    client.capabilities.remove(OPERATOR_GIT_CAPABILITY);
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("connect");

    let projection = adapter.d1_cockpit(None).expect("cockpit");
    let apply = projection
        .unavailable_features
        .iter()
        .find(|feature| feature.id == "apply")
        .expect("the row survives a Core without the capability");
    assert_eq!(apply.code, "GUI-CORE-020");
    assert!(apply.message.contains("runtime.operator_git"));
}

#[test]
fn a_core_without_the_capability_keeps_every_unavailable_claim() {
    let (mut view, approval) = base();
    let id = approval.id.clone();
    view.pending_approvals.push(approval);
    let adapter = connected(view, false);

    let projection = adapter.d1_cockpit(None).expect("cockpit");
    assert!(
        projection
            .unavailable_features
            .iter()
            .any(|feature| feature.id == "diff" && feature.code == "GUI-CORE-012")
    );
    let detail = adapter
        .d2_decisions_for(&id)
        .expect("d2 projection")
        .detail
        .expect("selected decision");
    assert_eq!(
        detail.context.unavailable.map(|note| note.code),
        Some("GUI-CORE-012")
    );
}
