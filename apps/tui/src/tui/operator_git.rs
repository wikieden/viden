//! Confirm-on-fact correlation for one operator source-control action
//! (`runtime.operator_git`, GUI-CORE-020).
//!
//! The sibling of `super::pending`, and deliberately narrower. A supervision
//! decision waits for a *record* fact (`MergeGateUpdated` for this gate id),
//! which is not correlated to the command that asked for it, so that machine
//! must see the acceptance receipt before it may attribute the fact. An
//! operator git action is answered by `OperatorGitActionFinished` carrying the
//! caller's own `command_id`, so the correlation is exact from the first byte
//! and a lost or reordered receipt can never strand the outcome.
//!
//! What this module refuses to do is the point of it:
//!
//! - It never infers success from `Completed.output`. Every fact a client needs
//!   is typed in the outcome or in the resampled `source`; the output is
//!   display text.
//! - It never collapses refused and failed. `CommandRejected` means nothing
//!   ran; a `Failed` outcome means the gate granted the action and `git`
//!   rejected it. Rendering the two alike would tell an operator "denied" about
//!   a problem in their own index.
//! - It never runs `git` itself and never falls back to a shell.
//! - Exactly one action is in flight at a time, refused locally rather than
//!   racing two correlations through one ordered stream.

use viden_core::{
    OperatorGitAction, OperatorGitFailureClass, OperatorGitOutcome, RuntimeEvent, RuntimeEventKind,
    RuntimeOwner, SourceTarget,
};

use super::{
    projection::{CancelOwnerProjection, CockpitProjection},
    state::TuiState,
};

/// The capability that publishes `RunOperatorGitAction`.
pub(super) const OPERATOR_GIT_CAPABILITY: &str = "runtime.operator_git";

/// One operator action awaiting its settled Core answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PendingOperatorGit {
    pub(super) command_id: String,
    pub(super) target: SourceTarget,
    pub(super) action: OperatorGitAction,
    /// Core admitted the command. Recorded for the status row only: unlike a
    /// supervision decision, attribution here never depends on it.
    accepted: bool,
}

impl PendingOperatorGit {
    #[cfg(test)]
    pub(super) fn is_accepted(&self) -> bool {
        self.accepted
    }
}

/// How one operator action ended, as Core stated it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum OperatorGitSettlement {
    /// A pre-effect refusal. `reason` is Core's own text, never a locally
    /// composed one, because only Core knows which rule refused.
    Rejected { reason: String },
    /// The gate granted the action and the effect was attempted.
    Finished {
        action: OperatorGitAction,
        outcome: Box<OperatorGitOutcome>,
        audit_id: String,
    },
}

/// Why a second operator action was refused locally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OperatorGitBusy {
    pub(super) pending_command_id: String,
}

/// TUI-local correlation for the operator source-control surface.
///
/// Holds no authoritative record: a command id this client issued, and the
/// ordered events Core published back against it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct OperatorGitMachine {
    pending: Option<PendingOperatorGit>,
}

impl OperatorGitMachine {
    pub(super) fn begin(
        &mut self,
        command_id: impl Into<String>,
        target: SourceTarget,
        action: OperatorGitAction,
    ) -> Result<(), OperatorGitBusy> {
        if let Some(pending) = self.pending.as_ref() {
            return Err(OperatorGitBusy {
                pending_command_id: pending.command_id.clone(),
            });
        }
        self.pending = Some(PendingOperatorGit {
            command_id: command_id.into(),
            target,
            action,
            accepted: false,
        });
        Ok(())
    }

    /// Reconciles one ordered Core event against the in-flight action.
    pub(super) fn observe_event(&mut self, event: &RuntimeEvent) -> Option<OperatorGitSettlement> {
        let pending = self.pending.as_mut()?;
        match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, reason }
                if command_id == &pending.command_id =>
            {
                self.pending = None;
                Some(OperatorGitSettlement::Rejected {
                    reason: reason.clone(),
                })
            }
            RuntimeEventKind::CommandAccepted { command_id, .. }
                if command_id == &pending.command_id =>
            {
                // A receipt is not an outcome; the action is still in flight.
                pending.accepted = true;
                None
            }
            RuntimeEventKind::OperatorGitActionFinished {
                command_id,
                action,
                outcome,
                audit_id,
                ..
            } if command_id == &pending.command_id => {
                self.pending = None;
                // The echoed action is rendered rather than the remembered one:
                // Core answers about what it actually ran.
                Some(OperatorGitSettlement::Finished {
                    action: action.clone(),
                    outcome: Box::new(outcome.clone()),
                    audit_id: audit_id.clone(),
                })
            }
            _ => None,
        }
    }

    pub(super) fn pending(&self) -> Option<&PendingOperatorGit> {
        self.pending.as_ref()
    }

    /// Stops attributing the in-flight action locally and frees the slot.
    ///
    /// Not a cancellation: Core owns the command and the effect may still land,
    /// so this settles nothing and composes no outcome.
    pub(super) fn abandon(&mut self) -> bool {
        self.pending.take().is_some()
    }
}

/// Why one operator source-control action cannot name a Core-published owner.
///
/// `RunOperatorGitAction` is an audited mutation, and Core requires the
/// command's `owner` to equal its envelope owner — that owner is the actor the
/// audit record names. `RuntimeOwner::default()` names nobody, so sending it
/// would file an authorized source-control change as belonging to no one.
/// Both cases below are therefore refused locally, before anything is sent,
/// which is the same refusal the GUI makes at `D1-OPERATOR-GIT-OWNER`: the two
/// clients state one shared gap rather than two different behaviors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum OperatorGitOwnerRefusal {
    /// Core publishes no workspace-scoped operator identity yet
    /// (GUI-CORE-027), so no workspace-target action can be attributed.
    WorkspaceUnscoped,
    /// Core has published no runtime owner for this exact Lane. The client
    /// never manufactures one: an owner naming a Lane whose runtime identity
    /// Core has not published would be this client inventing the actor.
    LaneOwnerUnpublished { lane_id: String },
}

impl OperatorGitOwnerRefusal {
    /// The transcript sentence stating what was refused and why.
    pub(super) fn message(&self, state: &TuiState) -> String {
        match self {
            Self::WorkspaceUnscoped => super::i18n::text(state, "git.owner.workspace_unscoped"),
            Self::LaneOwnerUnpublished { lane_id } => {
                super::i18n::translate(state, "git.owner.lane_unpublished", &[("lane", lane_id)])
            }
        }
    }

    /// The short reason appended to every disabled `/git` row, so the surface
    /// is grouped and labelled rather than hidden.
    pub(super) fn row_label(&self, state: &TuiState) -> String {
        match self {
            Self::WorkspaceUnscoped => {
                super::i18n::text(state, "git.owner.workspace_unscoped.short")
            }
            Self::LaneOwnerUnpublished { lane_id } => super::i18n::translate(
                state,
                "git.owner.lane_unpublished.short",
                &[("lane", lane_id)],
            ),
        }
    }
}

/// The envelope owner one operator action is attributed to, or the refusal
/// that stops it from being sent.
///
/// Core requires the command's `owner` and its envelope owner to be the same
/// value, so both come from here. For a Lane target it is the runtime owner
/// *Core published* for that exact Lane; the audit record still names the Lane
/// through the action's own target. Every other target — the workspace
/// included — has no Core-published operator identity yet, so it is refused
/// rather than filled in with a default.
///
/// The picker rows and the send path both read this one answer, so a row can
/// never be offered as pickable and then refused after the fact.
pub(super) fn operator_git_owner(
    state: &TuiState,
    target: &SourceTarget,
) -> Result<RuntimeOwner, OperatorGitOwnerRefusal> {
    let SourceTarget::Lane { lane_id } = target else {
        return Err(OperatorGitOwnerRefusal::WorkspaceUnscoped);
    };
    let projection =
        CockpitProjection::from_with_capabilities(&state.runtime, &state.ui, &state.capabilities);
    match projection.cancel_owner_for_lane(lane_id) {
        CancelOwnerProjection::Available(owner) => Ok(owner),
        // Every unavailable reason — no binding, an ambiguous pair, a stale
        // binding, a missing projection capability — means the same thing to
        // this surface: Core has published no owner this client may name.
        CancelOwnerProjection::Unavailable(_) => {
            Err(OperatorGitOwnerRefusal::LaneOwnerUnpublished {
                lane_id: lane_id.clone(),
            })
        }
    }
}

/// The catalog key naming one failure class, and the recovery it implies.
///
/// The pairing lives here, once, so the picker and the transcript entry cannot
/// disagree about what `NoUpstream` should offer. `#[non_exhaustive]` means a
/// newer Core can publish a class this build does not know; that case gets its
/// own honest wording rather than the nearest-looking recovery.
pub(super) const fn failure_copy(class: OperatorGitFailureClass) -> (&'static str, &'static str) {
    match class {
        OperatorGitFailureClass::NothingToCommit => (
            "git.failure.nothing_to_commit",
            "git.recovery.nothing_to_commit",
        ),
        OperatorGitFailureClass::NonFastForward => (
            "git.failure.non_fast_forward",
            "git.recovery.non_fast_forward",
        ),
        OperatorGitFailureClass::AuthenticationRequired => (
            "git.failure.authentication_required",
            "git.recovery.authentication_required",
        ),
        OperatorGitFailureClass::RemoteUnreachable => (
            "git.failure.remote_unreachable",
            "git.recovery.remote_unreachable",
        ),
        OperatorGitFailureClass::NoUpstream => {
            ("git.failure.no_upstream", "git.recovery.no_upstream")
        }
        OperatorGitFailureClass::PathOutsideRepository => (
            "git.failure.path_outside_repository",
            "git.recovery.path_outside_repository",
        ),
        OperatorGitFailureClass::Other => ("git.failure.other", "git.recovery.other"),
        // A class this build cannot name is reported as unclassified with
        // Core's real `detail`, never squeezed into a class whose recovery
        // would send the operator down the wrong path.
        _ => ("git.failure.unclassified", "git.recovery.other"),
    }
}

/// The catalog key naming one action, for an outcome row.
pub(super) const fn action_label_key(action: &OperatorGitAction) -> &'static str {
    match action {
        OperatorGitAction::Stage { .. } => "git.action.stage",
        OperatorGitAction::Unstage { .. } => "git.action.unstage",
        OperatorGitAction::Commit { .. } => "git.action.commit",
        OperatorGitAction::Push { .. } => "git.action.push",
        OperatorGitAction::Fetch { .. } => "git.action.fetch",
        // `#[non_exhaustive]`: an action a newer Core added is named as
        // unknown rather than mislabelled as one of the five above.
        _ => "git.action.unknown",
    }
}

/// The completed output, collapsed to one row's worth of display text.
///
/// The output is bounded by Core at 8 KiB and is *display* text: no fact is
/// read out of it. Collapsing keeps a commit hook's chatter from burying the
/// typed source facts beside it, and the line count says what was folded away.
pub(super) fn collapsed_output(output: &str) -> (usize, String) {
    let lines = output.lines().collect::<Vec<_>>();
    let first = lines
        .iter()
        .find(|line| !line.trim().is_empty())
        .copied()
        .unwrap_or_default();
    (lines.len(), first.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use viden_core::{WorkspaceSourceStatus, WorkspaceSourceView};

    fn event(sequence: u64, kind: RuntimeEventKind) -> RuntimeEvent {
        RuntimeEvent {
            sequence,
            timestamp: Some(sequence),
            kind,
        }
    }

    fn source() -> WorkspaceSourceView {
        WorkspaceSourceView {
            status: WorkspaceSourceStatus::Ready,
            branch: Some("main".to_string()),
            worktree: Some("workspace".to_string()),
            ahead: 1,
            behind: 0,
            added: 0,
            deleted: 0,
            dirty: false,
        }
    }

    fn finished(command_id: &str, outcome: OperatorGitOutcome) -> RuntimeEvent {
        event(
            3,
            RuntimeEventKind::OperatorGitActionFinished {
                command_id: command_id.to_string(),
                target: SourceTarget::Workspace,
                action: OperatorGitAction::Commit {
                    message: "feat: land it".to_string(),
                },
                outcome,
                audit_id: "audit-1".to_string(),
            },
        )
    }

    fn machine_awaiting(command_id: &str) -> OperatorGitMachine {
        let mut machine = OperatorGitMachine::default();
        machine
            .begin(
                command_id,
                SourceTarget::Workspace,
                OperatorGitAction::Commit {
                    message: "feat: land it".to_string(),
                },
            )
            .expect("nothing in flight");
        machine
    }

    #[test]
    fn only_one_action_is_in_flight_and_the_second_is_refused_locally() {
        let mut machine = machine_awaiting("tui-1");

        let busy = machine
            .begin(
                "tui-2",
                SourceTarget::Workspace,
                OperatorGitAction::Fetch { remote: None },
            )
            .expect_err("a second action must be refused");

        assert_eq!(busy.pending_command_id, "tui-1");
        assert_eq!(
            machine.pending().map(|p| p.command_id.as_str()),
            Some("tui-1")
        );
    }

    #[test]
    fn acceptance_is_a_receipt_and_never_an_outcome() {
        let mut machine = machine_awaiting("tui-1");

        let settled = machine.observe_event(&event(
            1,
            RuntimeEventKind::CommandAccepted {
                command_id: "tui-1".to_string(),
                command: viden_core::RuntimeCommand::CancelActiveTurn,
            },
        ));

        assert!(settled.is_none());
        assert!(machine.pending().expect("still pending").is_accepted());
    }

    #[test]
    fn a_failure_after_the_gate_is_a_finished_event_not_a_rejection() {
        let mut machine = machine_awaiting("tui-1");

        let settled = machine
            .observe_event(&finished(
                "tui-1",
                OperatorGitOutcome::Failed {
                    class: OperatorGitFailureClass::NoUpstream,
                    detail: "no upstream branch".to_string(),
                },
            ))
            .expect("the finished event settles the action");

        match settled {
            OperatorGitSettlement::Finished { outcome, .. } => assert!(matches!(
                *outcome,
                OperatorGitOutcome::Failed {
                    class: OperatorGitFailureClass::NoUpstream,
                    ..
                }
            )),
            other => panic!("a granted-then-failed action is not a refusal: {other:?}"),
        }
        assert!(machine.pending().is_none(), "the slot must be freed");
    }

    #[test]
    fn a_pre_effect_refusal_carries_cores_reason_verbatim() {
        let mut machine = machine_awaiting("tui-1");

        let settled = machine
            .observe_event(&event(
                2,
                RuntimeEventKind::CommandRejected {
                    command_id: "tui-1".to_string(),
                    reason: "permission denied\ntool: git_commit".to_string(),
                },
            ))
            .expect("a rejection settles the action");

        assert_eq!(
            settled,
            OperatorGitSettlement::Rejected {
                reason: "permission denied\ntool: git_commit".to_string()
            }
        );
    }

    #[test]
    fn facts_for_another_command_leave_the_machine_untouched() {
        let mut machine = machine_awaiting("tui-1");

        assert!(
            machine
                .observe_event(&finished(
                    "tui-9",
                    OperatorGitOutcome::Completed {
                        output: "done".to_string(),
                        truncated: false,
                        source: source(),
                    },
                ))
                .is_none()
        );
        assert!(
            machine
                .observe_event(&event(
                    4,
                    RuntimeEventKind::CommandRejected {
                        command_id: "tui-9".to_string(),
                        reason: "other".to_string(),
                    },
                ))
                .is_none()
        );
        assert_eq!(
            machine.pending().map(|p| p.command_id.as_str()),
            Some("tui-1")
        );
    }

    /// A lost acceptance must not strand the outcome: unlike a supervision
    /// fact, the finished event is correlated by this client's own command id.
    #[test]
    fn a_finished_event_settles_even_when_the_receipt_was_never_seen() {
        let mut machine = machine_awaiting("tui-1");

        assert!(
            machine
                .observe_event(&finished(
                    "tui-1",
                    OperatorGitOutcome::Completed {
                        output: "ok".to_string(),
                        truncated: false,
                        source: source(),
                    },
                ))
                .is_some()
        );
    }

    #[test]
    fn every_failure_class_has_its_own_message_and_recovery_key() {
        let classes = [
            OperatorGitFailureClass::NothingToCommit,
            OperatorGitFailureClass::NonFastForward,
            OperatorGitFailureClass::AuthenticationRequired,
            OperatorGitFailureClass::RemoteUnreachable,
            OperatorGitFailureClass::NoUpstream,
            OperatorGitFailureClass::PathOutsideRepository,
            OperatorGitFailureClass::Other,
        ];
        let mut messages = classes.map(|class| failure_copy(class).0).to_vec();
        messages.sort_unstable();
        let unique = messages.len();
        messages.dedup();
        assert_eq!(messages.len(), unique, "two classes share one message");
        // The two recoveries the contract names by hand.
        assert_eq!(
            failure_copy(OperatorGitFailureClass::NoUpstream).1,
            "git.recovery.no_upstream"
        );
        assert_eq!(
            failure_copy(OperatorGitFailureClass::NonFastForward).1,
            "git.recovery.non_fast_forward"
        );
    }

    #[test]
    fn output_is_collapsed_to_its_first_real_line_and_a_count() {
        assert_eq!(
            collapsed_output("\n[main 1a2b3c4] land it\n 1 file changed\n"),
            (3, "[main 1a2b3c4] land it".to_string())
        );
        assert_eq!(collapsed_output(""), (0, String::new()));
    }
}
