//! Confirm-on-fact correlation for one in-flight supervision command.
//!
//! This is the TUI port of the GUI adapter's `PendingD12`/`D1OutcomeProjection`
//! pair (`apps/gui/src-tauri/src/adapter.rs`, `apps/gui/src-tauri/src/d1.rs`).
//! It is deliberately not a business reducer: it stores no gate, review,
//! conflict, or revert record, and it never mutates `RuntimeViewState`. It only
//! answers "did the ordered Core event stream publish the business fact this
//! command asked for?" so the client never infers success from a receipt or
//! from transcript copy.
//!
//! Invariants encoded here:
//!
//! - `CommandAccepted`/`LaneCommandAccepted` is a receipt, never an outcome. It
//!   only records that Core admitted the command.
//! - Confirmation requires the matching business fact: `MergeGateUpdated` with
//!   this gate id *and* the requested status, `ReviewRequestUpdated` with this
//!   review id *and* the requested status, `MergeConflictBounced` for this
//!   gate, or `RevertRecorded` for this gate.
//! - `CommandRejected` for this command id is the only local failure path, and
//!   the reason string is Core's, not a locally composed one.
//! - Facts for other ids, other statuses, or other commands leave the machine
//!   untouched.
//! - Exactly one supervision command may be in flight; a second is refused
//!   locally instead of racing two correlations against one event stream.

use viden_core::{MergeGateStatus, ReviewRequestStatus, RuntimeEvent, RuntimeEventKind};

/// The business fact one in-flight supervision command is waiting for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SupervisionExpectation {
    MergeGate {
        gate_id: String,
        status: MergeGateStatus,
    },
    Review {
        review_id: String,
        status: ReviewRequestStatus,
    },
    ConflictBounce {
        gate_id: String,
    },
    Revert {
        gate_id: String,
    },
}

impl SupervisionExpectation {
    /// Whether this ordered event is the exact Core fact the command asked for.
    fn is_satisfied_by(&self, kind: &RuntimeEventKind) -> bool {
        match (self, kind) {
            (Self::MergeGate { gate_id, status }, RuntimeEventKind::MergeGateUpdated { gate }) => {
                &gate.gate_id == gate_id && gate.status == *status
            }
            (
                Self::Review { review_id, status },
                RuntimeEventKind::ReviewRequestUpdated { review },
            ) => &review.review_id == review_id && review.status == *status,
            (
                Self::ConflictBounce { gate_id },
                RuntimeEventKind::MergeConflictBounced { conflict },
            ) => &conflict.gate_id == gate_id,
            (Self::Revert { gate_id }, RuntimeEventKind::RevertRecorded { revert }) => {
                &revert.gate_id == gate_id
            }
            _ => false,
        }
    }
}

/// One supervision command awaiting its ordered Core receipt and fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PendingSupervision {
    pub(super) command_id: String,
    pub(super) expect: SupervisionExpectation,
    /// Whether Core has admitted the command. Acceptance alone never confirms;
    /// it only proves the command reached Core, which is what makes a later
    /// matching fact attributable to this command rather than to another
    /// client's decision replayed into the same stream.
    accepted: bool,
}

impl PendingSupervision {
    pub(super) fn new(command_id: impl Into<String>, expect: SupervisionExpectation) -> Self {
        Self {
            command_id: command_id.into(),
            expect,
            accepted: false,
        }
    }

    #[cfg(test)]
    pub(super) fn is_accepted(&self) -> bool {
        self.accepted
    }
}

/// The presentation-facing state of the supervision decision loop.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) enum SupervisionOutcome {
    #[default]
    Idle,
    Pending {
        command_id: String,
    },
    Confirmed,
    Rejected {
        reason: String,
    },
}

/// Why a second supervision command was refused locally.
///
/// Carries the in-flight command id so the caller can render a catalog string
/// naming it. No command is sent, so Core never sees the refused intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SupervisionBusy {
    pub(super) pending_command_id: String,
}

/// TUI-local correlation state for supervision decisions.
///
/// Holds no authoritative record. Every field is derived from a command id the
/// Core client issued plus the ordered events Core published back.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct SupervisionMachine {
    pending: Option<PendingSupervision>,
    outcome: SupervisionOutcome,
}

impl SupervisionMachine {
    /// Registers one in-flight supervision command.
    ///
    /// Refuses locally while another one is pending, mirroring the GUI adapter's
    /// "still pending" guard: two correlations against one ordered stream cannot
    /// both be attributed honestly.
    pub(super) fn begin(
        &mut self,
        command_id: impl Into<String>,
        expect: SupervisionExpectation,
    ) -> Result<(), SupervisionBusy> {
        if let Some(pending) = self.pending.as_ref() {
            return Err(SupervisionBusy {
                pending_command_id: pending.command_id.clone(),
            });
        }
        let pending = PendingSupervision::new(command_id, expect);
        self.outcome = SupervisionOutcome::Pending {
            command_id: pending.command_id.clone(),
        };
        self.pending = Some(pending);
        Ok(())
    }

    /// Reconciles one ordered Core event against the in-flight command.
    ///
    /// Returns whether this event moved the machine to a terminal outcome.
    pub(super) fn observe_event(&mut self, event: &RuntimeEvent) -> bool {
        let Some(pending) = self.pending.as_mut() else {
            return false;
        };
        match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, reason }
                if command_id == &pending.command_id =>
            {
                self.pending = None;
                self.outcome = SupervisionOutcome::Rejected {
                    reason: reason.clone(),
                };
                true
            }
            RuntimeEventKind::CommandAccepted { command_id, .. }
            | RuntimeEventKind::LaneCommandAccepted { command_id, .. }
                if command_id == &pending.command_id =>
            {
                // A receipt is not a decision: stay pending until the fact lands.
                pending.accepted = true;
                false
            }
            kind if pending.accepted && pending.expect.is_satisfied_by(kind) => {
                self.pending = None;
                self.outcome = SupervisionOutcome::Confirmed;
                true
            }
            _ => false,
        }
    }

    /// Stops attributing the in-flight command locally and frees the single
    /// in-flight slot. Returns whether anything was pending.
    ///
    /// This is the escape for a stranded pending decision: a lost or never
    /// published receipt would otherwise keep the slot occupied forever. It is
    /// deliberately *not* a cancellation — Core owns the command and may still
    /// apply it — so it settles nothing: no `Confirmed`, no `Rejected`, and no
    /// locally composed reason. Because the correlation is dropped, a later
    /// matching fact is no longer attributed to this client's decision, which is
    /// the honest outcome: the operator asked to stop watching, not to undo.
    pub(super) fn abandon(&mut self) -> bool {
        if self.pending.take().is_none() {
            return false;
        }
        self.outcome = SupervisionOutcome::Idle;
        true
    }

    /// Clears a settled outcome back to `Idle`. Returns whether anything reset.
    ///
    /// Only `Confirmed` and `Rejected` reset. `Pending` is never auto-reset:
    /// dropping a live correlation silently would let the next fact confirm the
    /// wrong decision, so releasing a pending slot always goes through
    /// [`Self::abandon`] as an explicit operator action.
    pub(super) fn reset_if_settled(&mut self) -> bool {
        if !matches!(
            self.outcome,
            SupervisionOutcome::Confirmed | SupervisionOutcome::Rejected { .. }
        ) {
            return false;
        }
        self.outcome = SupervisionOutcome::Idle;
        true
    }

    pub(super) fn outcome(&self) -> &SupervisionOutcome {
        &self.outcome
    }

    pub(super) fn pending(&self) -> Option<&PendingSupervision> {
        self.pending.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use viden_core::{
        ConflictBounce, ConflictBounceStatus, MergeGateRecord, MergeGateType, RevertRecord,
        ReviewRequestRecord, RuntimeCommand, RuntimeOwner,
    };

    fn event(sequence: u64, kind: RuntimeEventKind) -> RuntimeEvent {
        RuntimeEvent {
            sequence,
            timestamp: Some(sequence),
            kind,
        }
    }

    fn accepted(command_id: &str) -> RuntimeEvent {
        event(
            1,
            RuntimeEventKind::CommandAccepted {
                command_id: command_id.to_string(),
                command: RuntimeCommand::CancelActiveTurn,
            },
        )
    }

    fn gate(gate_id: &str, status: MergeGateStatus) -> MergeGateRecord {
        MergeGateRecord {
            gate_id: gate_id.to_string(),
            task_id: "task-1".to_string(),
            status,
            required_evidence: Vec::new(),
            evidence_ids: Vec::new(),
            gate_type: MergeGateType::Patch,
            owner: RuntimeOwner::default(),
            validator: None,
            policy_snapshot: Default::default(),
            decision: None,
            conflict: None,
            applied_change_id: None,
            recovery_snapshot: None,
            audit_ids: Vec::new(),
            updated_at: Some(1),
        }
    }

    fn review(review_id: &str, status: ReviewRequestStatus) -> ReviewRequestRecord {
        ReviewRequestRecord {
            review_id: review_id.to_string(),
            gate_id: "gate-1".to_string(),
            task_id: "task-1".to_string(),
            requester_lane_id: "lane-a".to_string(),
            reviewer_lane_id: "lane-b".to_string(),
            owner: RuntimeOwner::default(),
            evidence_ids: Vec::new(),
            evidence_bindings: Vec::new(),
            status,
            feedback: None,
            audit_id: "audit-review".to_string(),
            updated_at: 2,
        }
    }

    fn bounce(gate_id: &str) -> ConflictBounce {
        ConflictBounce {
            bounce_id: "bounce-1".to_string(),
            gate_id: gate_id.to_string(),
            task_id: "task-1".to_string(),
            original_lane_id: "lane-a".to_string(),
            owner: RuntimeOwner::default(),
            reason: "conflict".to_string(),
            status: ConflictBounceStatus::Pending,
            evidence_ids: Vec::new(),
            baseline_evidence: Vec::new(),
            revalidation_evidence: Vec::new(),
            content: None,
            audit_id: "audit-bounce".to_string(),
            created_at: 3,
            revalidated_at: None,
        }
    }

    fn revert(gate_id: &str) -> RevertRecord {
        RevertRecord {
            revert_id: "revert-1".to_string(),
            gate_id: gate_id.to_string(),
            applied_change_id: "change-1".to_string(),
            owner: RuntimeOwner::default(),
            reason: "rollback".to_string(),
            restored_paths: Vec::new(),
            audit_id: "audit-revert".to_string(),
            reverted_at: 4,
        }
    }

    #[test]
    fn command_acceptance_never_confirms_a_supervision_decision() {
        let mut machine = SupervisionMachine::default();
        machine
            .begin(
                "cmd-1",
                SupervisionExpectation::MergeGate {
                    gate_id: "gate-1".to_string(),
                    status: MergeGateStatus::Accepted,
                },
            )
            .expect("first command");

        assert!(!machine.observe_event(&accepted("cmd-1")));

        assert_eq!(
            machine.outcome(),
            &SupervisionOutcome::Pending {
                command_id: "cmd-1".to_string()
            }
        );
        assert!(machine.pending().expect("still pending").is_accepted());
    }

    #[test]
    fn only_the_exact_merge_gate_fact_confirms_the_decision() {
        let mut machine = SupervisionMachine::default();
        machine
            .begin(
                "cmd-1",
                SupervisionExpectation::MergeGate {
                    gate_id: "gate-1".to_string(),
                    status: MergeGateStatus::Accepted,
                },
            )
            .expect("first command");
        machine.observe_event(&accepted("cmd-1"));

        // Wrong gate id.
        assert!(!machine.observe_event(&event(
            2,
            RuntimeEventKind::MergeGateUpdated {
                gate: gate("gate-other", MergeGateStatus::Accepted)
            }
        )));
        // Right gate, wrong status.
        assert!(!machine.observe_event(&event(
            3,
            RuntimeEventKind::MergeGateUpdated {
                gate: gate("gate-1", MergeGateStatus::NeedsChanges)
            }
        )));
        assert_eq!(
            machine.outcome(),
            &SupervisionOutcome::Pending {
                command_id: "cmd-1".to_string()
            }
        );

        assert!(machine.observe_event(&event(
            4,
            RuntimeEventKind::MergeGateUpdated {
                gate: gate("gate-1", MergeGateStatus::Accepted)
            }
        )));
        assert_eq!(machine.outcome(), &SupervisionOutcome::Confirmed);
        assert!(machine.pending().is_none());
    }

    #[test]
    fn review_conflict_and_revert_expectations_each_confirm_on_their_own_fact() {
        let cases: Vec<(SupervisionExpectation, RuntimeEventKind, RuntimeEventKind)> = vec![
            (
                SupervisionExpectation::Review {
                    review_id: "review-1".to_string(),
                    status: ReviewRequestStatus::Accepted,
                },
                RuntimeEventKind::ReviewRequestUpdated {
                    review: review("review-1", ReviewRequestStatus::Rejected),
                },
                RuntimeEventKind::ReviewRequestUpdated {
                    review: review("review-1", ReviewRequestStatus::Accepted),
                },
            ),
            (
                SupervisionExpectation::ConflictBounce {
                    gate_id: "gate-1".to_string(),
                },
                RuntimeEventKind::MergeConflictBounced {
                    conflict: bounce("gate-other"),
                },
                RuntimeEventKind::MergeConflictBounced {
                    conflict: bounce("gate-1"),
                },
            ),
            (
                SupervisionExpectation::Revert {
                    gate_id: "gate-1".to_string(),
                },
                RuntimeEventKind::RevertRecorded {
                    revert: revert("gate-other"),
                },
                RuntimeEventKind::RevertRecorded {
                    revert: revert("gate-1"),
                },
            ),
        ];

        for (expect, mismatch, exact) in cases {
            let mut machine = SupervisionMachine::default();
            machine
                .begin("cmd-1", expect.clone())
                .expect("first command");
            machine.observe_event(&accepted("cmd-1"));

            assert!(
                !machine.observe_event(&event(2, mismatch)),
                "{expect:?} confirmed on a non-matching fact"
            );
            assert!(
                machine.observe_event(&event(3, exact)),
                "{expect:?} did not confirm on its exact fact"
            );
            assert_eq!(machine.outcome(), &SupervisionOutcome::Confirmed);
        }
    }

    #[test]
    fn a_fact_for_another_command_never_confirms_an_unaccepted_decision() {
        let mut machine = SupervisionMachine::default();
        machine
            .begin(
                "cmd-1",
                SupervisionExpectation::MergeGate {
                    gate_id: "gate-1".to_string(),
                    status: MergeGateStatus::Accepted,
                },
            )
            .expect("first command");

        // Acceptance for a different command must not arm this correlation, and
        // the fact that follows it belongs to that other decision.
        assert!(!machine.observe_event(&accepted("cmd-other")));
        assert!(!machine.observe_event(&event(
            2,
            RuntimeEventKind::MergeGateUpdated {
                gate: gate("gate-1", MergeGateStatus::Accepted)
            }
        )));
        assert_eq!(
            machine.outcome(),
            &SupervisionOutcome::Pending {
                command_id: "cmd-1".to_string()
            }
        );
    }

    #[test]
    fn rejection_carries_the_core_reason_and_clears_the_pending_decision() {
        let mut machine = SupervisionMachine::default();
        machine
            .begin(
                "cmd-1",
                SupervisionExpectation::MergeGate {
                    gate_id: "gate-1".to_string(),
                    status: MergeGateStatus::Accepted,
                },
            )
            .expect("first command");

        assert!(!machine.observe_event(&event(
            2,
            RuntimeEventKind::CommandRejected {
                command_id: "cmd-other".to_string(),
                reason: "someone else".to_string(),
            }
        )));
        assert!(machine.observe_event(&event(
            3,
            RuntimeEventKind::CommandRejected {
                command_id: "cmd-1".to_string(),
                reason: "gate already closed".to_string(),
            }
        )));

        assert_eq!(
            machine.outcome(),
            &SupervisionOutcome::Rejected {
                reason: "gate already closed".to_string()
            }
        );
        assert!(machine.pending().is_none());
    }

    #[test]
    fn a_second_supervision_command_is_refused_while_one_is_pending() {
        let mut machine = SupervisionMachine::default();
        machine
            .begin(
                "cmd-1",
                SupervisionExpectation::MergeGate {
                    gate_id: "gate-1".to_string(),
                    status: MergeGateStatus::Accepted,
                },
            )
            .expect("first command");

        assert_eq!(
            machine.begin(
                "cmd-2",
                SupervisionExpectation::Revert {
                    gate_id: "gate-1".to_string()
                }
            ),
            Err(SupervisionBusy {
                pending_command_id: "cmd-1".to_string()
            })
        );
        assert_eq!(
            machine
                .pending()
                .expect("first command still pending")
                .expect,
            SupervisionExpectation::MergeGate {
                gate_id: "gate-1".to_string(),
                status: MergeGateStatus::Accepted
            }
        );

        machine.observe_event(&accepted("cmd-1"));
        machine.observe_event(&event(
            2,
            RuntimeEventKind::MergeGateUpdated {
                gate: gate("gate-1", MergeGateStatus::Accepted),
            },
        ));
        assert!(
            machine
                .begin(
                    "cmd-2",
                    SupervisionExpectation::Revert {
                        gate_id: "gate-1".to_string()
                    }
                )
                .is_ok(),
            "a settled decision must release the single in-flight slot"
        );
    }

    #[test]
    fn abandon_clears_a_stranded_pending_decision_without_settling_it() {
        let mut machine = SupervisionMachine::default();
        machine
            .begin(
                "cmd-1",
                SupervisionExpectation::MergeGate {
                    gate_id: "gate-1".to_string(),
                    status: MergeGateStatus::Accepted,
                },
            )
            .expect("first command");
        machine.observe_event(&accepted("cmd-1"));

        assert!(machine.abandon());

        // Abandoning is local attribution only: no confirmation, no rejection,
        // and no invented Core reason.
        assert_eq!(machine.outcome(), &SupervisionOutcome::Idle);
        assert!(machine.pending().is_none());
        assert!(!machine.abandon(), "abandoning twice is a no-op");

        // The Core command was never cancelled, so its later fact must not be
        // attributed to a decision this client stopped watching.
        assert!(!machine.observe_event(&event(
            9,
            RuntimeEventKind::MergeGateUpdated {
                gate: gate("gate-1", MergeGateStatus::Accepted)
            }
        )));
        assert_eq!(machine.outcome(), &SupervisionOutcome::Idle);
    }

    #[test]
    fn only_a_settled_outcome_resets_to_idle() {
        let mut machine = SupervisionMachine::default();
        assert!(!machine.reset_if_settled(), "idle is already idle");

        machine
            .begin(
                "cmd-1",
                SupervisionExpectation::Revert {
                    gate_id: "gate-1".to_string(),
                },
            )
            .expect("first command");
        assert!(
            !machine.reset_if_settled(),
            "a pending decision is never auto-reset"
        );
        assert_eq!(
            machine.outcome(),
            &SupervisionOutcome::Pending {
                command_id: "cmd-1".to_string()
            }
        );

        machine.observe_event(&accepted("cmd-1"));
        machine.observe_event(&event(
            2,
            RuntimeEventKind::RevertRecorded {
                revert: revert("gate-1"),
            },
        ));
        assert_eq!(machine.outcome(), &SupervisionOutcome::Confirmed);
        assert!(machine.reset_if_settled());
        assert_eq!(machine.outcome(), &SupervisionOutcome::Idle);

        machine
            .begin(
                "cmd-2",
                SupervisionExpectation::Revert {
                    gate_id: "gate-1".to_string(),
                },
            )
            .expect("second command");
        machine.observe_event(&event(
            3,
            RuntimeEventKind::CommandRejected {
                command_id: "cmd-2".to_string(),
                reason: "gate already reverted".to_string(),
            },
        ));
        assert!(machine.reset_if_settled());
        assert_eq!(machine.outcome(), &SupervisionOutcome::Idle);
    }

    #[test]
    fn events_are_ignored_entirely_when_no_supervision_command_is_pending() {
        let mut machine = SupervisionMachine::default();

        assert!(!machine.observe_event(&accepted("cmd-1")));
        assert!(!machine.observe_event(&event(
            2,
            RuntimeEventKind::MergeGateUpdated {
                gate: gate("gate-1", MergeGateStatus::Accepted)
            }
        )));

        assert_eq!(machine.outcome(), &SupervisionOutcome::Idle);
    }
}
