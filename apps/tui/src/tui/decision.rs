//! Supervision decision model: which actions a Core record actually offers,
//! and the exact command payload each one sends.
//!
//! This module is pure. It reads the authoritative `RuntimeViewState` records
//! and returns actions plus frozen `RuntimeCommand`s; it performs no dispatch,
//! owns no business state, and never composes an identity, hash, or reason from
//! rendered text.
//!
//! Two rules govern everything here:
//!
//! - **Core is the authority.** An action whose Core precondition merely *looks*
//!   unsatisfiable from local facts is still offered and still sent, because
//!   only Core may reject it. The single exception is the honest-UI rule: an
//!   action that can never apply to the record's current status (reverting a
//!   gate that was never merged) is not listed at all, so the operator is not
//!   shown a control that is meaningless rather than merely risky.
//! - **Payloads are replayed, never rebuilt.** Actors and reviewed-evidence
//!   bindings are copied out of Core's own records using the same derivation the
//!   GUI D12 adapter uses (`apps/gui/src-tauri/src/projection.rs`:
//!   `d12_accept_actor`, `d12_reject_actor`, `d12_reviewed_evidence`,
//!   `d12_canonical_bindings`). Core compares acceptance against those exact
//!   shapes, so a locally invented actor or hash would simply be rejected.
//!
//! When a payload cannot be replayed at all — no owner this client may act as,
//! no canonical evidence Core could verify — the action is refused locally with
//! a catalog key. That is not pre-empting Core validation: the client has
//! nothing valid to send.

use viden_core::{
    AuditObjectRef, ConflictBounce, ConflictBounceStatus, MergeGateRecord, MergeGateStatus,
    ReviewRequestRecord, ReviewRequestStatus, ReviewedEvidenceBinding, RuntimeCommand,
    RuntimeOwner, RuntimeViewState,
};
use viden_types::{AgentSessionStatus, ReviewVerdict};

use super::pending::SupervisionExpectation;
use super::supervision::{
    accept_merge_gate_intent, bounce_merge_conflict_intent, decide_review_intent,
    reject_merge_gate_intent, revalidate_merge_conflict_intent, revert_applied_change_intent,
};

/// Core's trust-text ceiling (`validate_trust_text` in
/// `crates/runtime/src/trust_loop.rs` truncates at this width for reasons,
/// bounce reasons, revert reasons, and review feedback). The client refuses
/// over-limit input instead of truncating it, so the operator never discovers
/// afterwards that Core stored half a sentence.
pub(super) const MAX_TRUST_TEXT_CHARS: usize = 500;

/// The Core record one supervision decision acts on.
///
/// Only identity is stored. Every action, label, and payload is derived by
/// re-reading the full record from `RuntimeViewState`, so a record that changed
/// or disappeared between render and keypress cannot be decided from a stale
/// compact row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SupervisionTarget {
    Gate { gate_id: String },
    Review { review_id: String },
    Bounce { gate_id: String },
}

/// Whether an action carries operator text, and whether Core requires it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TextRequirement {
    /// The command has no text field.
    None,
    /// Core stores the text and refuses an empty one.
    Required,
    /// Core accepts the command with or without the text.
    Optional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SupervisionAction {
    AcceptGate,
    RejectGate,
    Revalidate,
    Revert,
    AcceptReview,
    RejectReview,
    Bounce,
    /// Local-only escape from a stranded pending correlation. Sends nothing.
    Dismiss,
    /// Opens the read-only audit timeline scoped to this record. It is not a
    /// decision: it mutates nothing, needs no permission, and stays available
    /// in Plan mode and while another supervision command is in flight.
    AuditTrail,
}

impl SupervisionAction {
    pub(super) const fn label_key(self) -> &'static str {
        match self {
            Self::AcceptGate => "supervision.action.accept_gate",
            Self::RejectGate => "supervision.action.reject_gate",
            Self::Revalidate => "supervision.action.revalidate",
            Self::Revert => "supervision.action.revert",
            Self::AcceptReview => "supervision.action.accept_review",
            Self::RejectReview => "supervision.action.reject_review",
            Self::Bounce => "supervision.action.bounce",
            Self::Dismiss => "supervision.action.dismiss",
            Self::AuditTrail => "supervision.action.audit_trail",
        }
    }

    pub(super) const fn text_requirement(self) -> TextRequirement {
        match self {
            // Core stores these as the gate decision / bounce reason / revert
            // reason and rejects an empty one.
            Self::RejectGate | Self::Revert | Self::Bounce => TextRequirement::Required,
            // `DecideReview.feedback` is `Option<String>` for both verdicts.
            // The client must not invent a stricter rule than Core.
            Self::AcceptReview | Self::RejectReview => TextRequirement::Optional,
            Self::AcceptGate | Self::Revalidate | Self::Dismiss | Self::AuditTrail => {
                TextRequirement::None
            }
        }
    }

    /// Whether this action destroys work that cannot be recreated by Core.
    pub(super) const fn is_irreversible(self) -> bool {
        matches!(self, Self::Revert)
    }
}

/// One entry of the Decision Center pick list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DecisionPick {
    /// Routes to the pinned Approval overlay, unchanged from before.
    Approval { request_id: String },
    /// Opens the supervision decision overlay on this Core record.
    Supervision(SupervisionTarget),
    /// Local escape from a stranded pending correlation, offered here as well as
    /// inside the overlay because a lost receipt can strand the operator before
    /// they ever open one. It sends no Core command.
    DismissSupervision,
    /// Opens the project-wide audit timeline. Unlike every other pick it names
    /// no record: it is the "what happened here at all" entry point, which is
    /// exactly what an operator needs when no decision is pending.
    AuditTimeline,
}

/// The Decision Center pick list, in the fixed operator order: tool approvals
/// (most urgent, auto-denied on expiry), then merge gates, then reviews still
/// awaiting a verdict, then conflicts still awaiting revalidation.
///
/// Settled reviews and resolved bounces are history, not decisions, so they are
/// not pickable here; they remain visible through the gate they belong to.
///
/// `has_pending_command` appends the local dismiss escape last, so it never
/// shifts the index of a real Core decision.
pub(super) fn decision_picks(
    view: &RuntimeViewState,
    has_pending_command: bool,
) -> Vec<DecisionPick> {
    let mut picks: Vec<DecisionPick> = view
        .pending_approvals
        .iter()
        .map(|approval| DecisionPick::Approval {
            request_id: approval.id.clone(),
        })
        .collect();
    picks.extend(
        view.merge_gates
            .iter()
            .filter(|gate| !is_dormant_gate(view, gate))
            .map(|gate| {
                DecisionPick::Supervision(SupervisionTarget::Gate {
                    gate_id: gate.gate_id.clone(),
                })
            }),
    );
    picks.extend(
        view.review_requests
            .iter()
            .filter(|review| review.status == ReviewRequestStatus::Pending)
            .map(|review| {
                DecisionPick::Supervision(SupervisionTarget::Review {
                    review_id: review.review_id.clone(),
                })
            }),
    );
    picks.extend(
        view.conflict_bounces
            .iter()
            .filter(|conflict| conflict.status == ConflictBounceStatus::Pending)
            .map(|conflict| {
                DecisionPick::Supervision(SupervisionTarget::Bounce {
                    gate_id: conflict.gate_id.clone(),
                })
            }),
    );
    // Dormant gates come after every actionable decision. They stay pickable
    // and fully decidable — this is grouping, never hiding — but a gate whose
    // session finished days ago must not sit above a live approval.
    picks.extend(
        view.merge_gates
            .iter()
            .filter(|gate| is_dormant_gate(view, gate))
            .map(|gate| {
                DecisionPick::Supervision(SupervisionTarget::Gate {
                    gate_id: gate.gate_id.clone(),
                })
            }),
    );
    if has_pending_command {
        picks.push(DecisionPick::DismissSupervision);
    }
    // The audit entry is appended after every conditional pick so adding it can
    // never shift the index of a real Core decision.
    picks.push(DecisionPick::AuditTimeline);
    picks
}

/// Whether a gate is dormant: still awaiting a decision, while the Agent
/// session it belongs to has already finished.
///
/// A finished session cannot produce the evidence its gate is waiting for, so
/// such a gate is residue rather than actionable work. Eight of them, from
/// sessions terminal for weeks, were burying every live decision in the
/// Decision Center. Dormancy changes only ordering and labelling: the row stays
/// selectable and every action stays available, so nothing is hidden.
///
/// The join is `gate.owner.session_id` — the Agent session Core published, not
/// the ACP protocol handle the gate id is keyed on. A gate that names no
/// session, or names one this view has never seen, is never dormant: an unknown
/// session is not a finished one.
pub(super) fn is_dormant_gate(view: &RuntimeViewState, gate: &MergeGateRecord) -> bool {
    // A decided gate is settled, not dormant, whatever its session is doing.
    if gate.decision.is_some() || !gate.status.is_open() {
        return false;
    }
    let Some(session_id) = gate.owner.session_id.as_deref() else {
        return false;
    };
    view.agent_sessions.iter().any(|session| {
        session.session_id == session_id
            && matches!(
                session.status,
                AgentSessionStatus::Completed
                    | AgentSessionStatus::Failed
                    | AgentSessionStatus::Cancelled
            )
    })
}

/// How many gates the Decision Center will group as dormant.
pub(super) fn dormant_gate_count(view: &RuntimeViewState) -> usize {
    view.merge_gates
        .iter()
        .filter(|gate| is_dormant_gate(view, gate))
        .count()
}

/// The audit object one supervision target's timeline is scoped to.
///
/// The kind keys come from [`AuditObjectRef`]'s own constants rather than from
/// string literals, so a renamed kind is a compile error here instead of a
/// silently empty timeline.
pub(super) fn audit_scope(target: &SupervisionTarget) -> AuditObjectRef {
    match target {
        SupervisionTarget::Gate { gate_id } | SupervisionTarget::Bounce { gate_id } => {
            AuditObjectRef::new(AuditObjectRef::KIND_MERGE_GATE, gate_id.clone())
        }
        SupervisionTarget::Review { review_id } => {
            AuditObjectRef::new(AuditObjectRef::KIND_REVIEW_REQUEST, review_id.clone())
        }
    }
}

/// The full row list of the supervision overlay: the Core decisions this
/// record's status can accept, plus the read-only audit row.
///
/// The audit row is appended last and is always present, including for a record
/// whose status accepts no decision at all: the timeline of a settled gate is
/// exactly what an operator wants to read then. It is kept out of
/// [`available_actions`] so "no decision applies" stays a true statement about
/// decisions.
pub(super) fn overlay_actions(
    view: &RuntimeViewState,
    target: &SupervisionTarget,
    has_pending_command: bool,
) -> Vec<SupervisionAction> {
    let mut actions = available_actions(view, target, has_pending_command);
    actions.push(SupervisionAction::AuditTrail);
    actions
}

pub(super) fn find_gate<'a>(
    view: &'a RuntimeViewState,
    gate_id: &str,
) -> Option<&'a MergeGateRecord> {
    view.merge_gates.iter().find(|gate| gate.gate_id == gate_id)
}

pub(super) fn find_review<'a>(
    view: &'a RuntimeViewState,
    review_id: &str,
) -> Option<&'a ReviewRequestRecord> {
    view.review_requests
        .iter()
        .find(|review| review.review_id == review_id)
}

/// The conflict bounce still awaiting origin-lane revalidation for this gate.
///
/// Core keeps the live copy on the gate record itself and mirrors it into the
/// bounce list, so the gate is read first and the list is only a fallback for a
/// view that received the bounce fact without a gate update.
pub(super) fn pending_conflict<'a>(
    view: &'a RuntimeViewState,
    gate: &'a MergeGateRecord,
) -> Option<&'a ConflictBounce> {
    gate.conflict
        .as_ref()
        .filter(|conflict| conflict.status == ConflictBounceStatus::Pending)
        .or_else(|| {
            view.conflict_bounces.iter().find(|conflict| {
                conflict.gate_id == gate.gate_id && conflict.status == ConflictBounceStatus::Pending
            })
        })
}

/// The actions this target's *current* Core status can accept.
///
/// `has_pending_command` adds the local `Dismiss` escape; it never removes a
/// Core action, because a stranded correlation is a client-side problem and must
/// not hide what the record still allows.
pub(super) fn available_actions(
    view: &RuntimeViewState,
    target: &SupervisionTarget,
    has_pending_command: bool,
) -> Vec<SupervisionAction> {
    let mut actions = match target {
        SupervisionTarget::Gate { gate_id } => match find_gate(view, gate_id) {
            Some(gate) if gate.status.is_open() => {
                // While a bounce is pending the origin lane owes a revalidation;
                // acceptance cannot apply until the conflict clears, so the slot
                // shows the action that can.
                let first = if pending_conflict(view, gate).is_some() {
                    SupervisionAction::Revalidate
                } else {
                    SupervisionAction::AcceptGate
                };
                vec![first, SupervisionAction::RejectGate]
            }
            // Reverting is the only decision a merged gate still has.
            Some(gate) if gate.status == MergeGateStatus::Merged => vec![SupervisionAction::Revert],
            Some(_) | None => Vec::new(),
        },
        SupervisionTarget::Review { review_id } => match find_review(view, review_id) {
            Some(review) if review.status == ReviewRequestStatus::Pending => vec![
                SupervisionAction::AcceptReview,
                SupervisionAction::RejectReview,
            ],
            Some(_) | None => Vec::new(),
        },
        SupervisionTarget::Bounce { gate_id } => match find_gate(view, gate_id) {
            Some(gate) if gate.status.is_open() => vec![SupervisionAction::Bounce],
            Some(_) | None => Vec::new(),
        },
    };
    if has_pending_command {
        actions.push(SupervisionAction::Dismiss);
    }
    actions
}

/// One dispatchable supervision decision: the envelope owner, the frozen
/// command, and the exact Core fact that will confirm it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SupervisionDispatch {
    pub(super) owner: RuntimeOwner,
    pub(super) command: RuntimeCommand,
    pub(super) expect: SupervisionExpectation,
}

/// Builds the command for one confirmed action, or returns the catalog key of a
/// local refusal.
///
/// `text` is the operator's raw reason/feedback line. It is validated here
/// rather than at the keymap so every entry point applies the same rule.
pub(super) fn build_dispatch(
    view: &RuntimeViewState,
    target: &SupervisionTarget,
    action: SupervisionAction,
    text: &str,
) -> Result<SupervisionDispatch, &'static str> {
    let text = validate_text(action, text)?;
    match action {
        // Neither one sends a Core command: dismiss is local attribution only,
        // and the audit row opens a read-only overlay.
        SupervisionAction::Dismiss | SupervisionAction::AuditTrail => {
            Err("supervision.error.not_dispatchable")
        }
        SupervisionAction::AcceptGate => {
            let gate = require_gate(view, target)?;
            let actor = accept_actor(gate).ok_or("supervision.error.no_actor")?;
            let reviewed_evidence =
                reviewed_evidence(view, gate).ok_or("supervision.error.no_canonical_evidence")?;
            Ok(SupervisionDispatch {
                owner: actor.clone(),
                command: accept_merge_gate_intent(
                    gate.gate_id.clone(),
                    actor,
                    reviewed_evidence,
                    None,
                ),
                expect: SupervisionExpectation::MergeGate {
                    gate_id: gate.gate_id.clone(),
                    status: MergeGateStatus::Accepted,
                },
            })
        }
        SupervisionAction::RejectGate => {
            let gate = require_gate(view, target)?;
            let actor = reject_actor(gate).ok_or("supervision.error.no_actor")?;
            Ok(SupervisionDispatch {
                owner: actor.clone(),
                command: reject_merge_gate_intent(gate.gate_id.clone(), actor, text),
                expect: SupervisionExpectation::MergeGate {
                    gate_id: gate.gate_id.clone(),
                    // Core's rejection path lands the gate on NeedsChanges so
                    // the origin lane can rework it; it is not a terminal state.
                    status: MergeGateStatus::NeedsChanges,
                },
            })
        }
        SupervisionAction::Revalidate => {
            let gate = require_gate(view, target)?;
            let conflict =
                pending_conflict(view, gate).ok_or("supervision.error.no_pending_conflict")?;
            let evidence = revalidation_evidence(view, gate, conflict)
                .ok_or("supervision.error.no_revalidation_evidence")?;
            let actor = conflict.owner.clone();
            Ok(SupervisionDispatch {
                owner: actor.clone(),
                command: revalidate_merge_conflict_intent(
                    gate.gate_id.clone(),
                    conflict.bounce_id.clone(),
                    actor,
                    evidence,
                ),
                // Revalidation publishes MergeConflictBounced *and* returns the
                // gate to CollectingEvidence (crates/runtime/src/trust_loop.rs
                // `revalidate_merge_conflict`). The gate transition is the fact
                // that proves the conflict was accepted as resolved.
                expect: SupervisionExpectation::MergeGate {
                    gate_id: gate.gate_id.clone(),
                    status: MergeGateStatus::CollectingEvidence,
                },
            })
        }
        SupervisionAction::Revert => {
            let gate = require_gate(view, target)?;
            let owner = revert_owner(gate).ok_or("supervision.error.no_actor")?;
            Ok(SupervisionDispatch {
                owner: owner.clone(),
                command: revert_applied_change_intent(gate.gate_id.clone(), owner, text),
                expect: SupervisionExpectation::Revert {
                    gate_id: gate.gate_id.clone(),
                },
            })
        }
        SupervisionAction::Bounce => {
            let gate = require_gate(view, target)?;
            let (owner, original_lane_id) =
                bounce_parties(gate).ok_or("supervision.error.no_actor")?;
            Ok(SupervisionDispatch {
                owner: owner.clone(),
                command: bounce_merge_conflict_intent(
                    gate.gate_id.clone(),
                    original_lane_id,
                    owner,
                    text,
                ),
                expect: SupervisionExpectation::ConflictBounce {
                    gate_id: gate.gate_id.clone(),
                },
            })
        }
        SupervisionAction::AcceptReview | SupervisionAction::RejectReview => {
            let SupervisionTarget::Review { review_id } = target else {
                return Err("supervision.error.record_missing");
            };
            let review = find_review(view, review_id).ok_or("supervision.error.record_missing")?;
            let actor = review_actor(view, review).ok_or("supervision.error.no_actor")?;
            let verdict = if action == SupervisionAction::AcceptReview {
                ReviewVerdict::Accepted
            } else {
                ReviewVerdict::Rejected
            };
            let feedback = (!text.is_empty()).then(|| text.clone());
            Ok(SupervisionDispatch {
                owner: actor.clone(),
                command: decide_review_intent(review.review_id.clone(), verdict, feedback, actor),
                expect: SupervisionExpectation::Review {
                    review_id: review.review_id.clone(),
                    status: ReviewRequestStatus::from(verdict),
                },
            })
        }
    }
}

/// Trims and length-checks operator text against Core's own rule.
///
/// Core trims before it validates, so trailing whitespace never makes an empty
/// reason valid here either. Over-limit input is refused rather than truncated:
/// silently sending half a reason would misattribute the operator's words.
fn validate_text(action: SupervisionAction, text: &str) -> Result<String, &'static str> {
    let trimmed = text.trim().to_string();
    match action.text_requirement() {
        TextRequirement::None => Ok(String::new()),
        TextRequirement::Required if trimmed.is_empty() => Err("supervision.error.reason_required"),
        TextRequirement::Required | TextRequirement::Optional => {
            if trimmed.chars().count() > MAX_TRUST_TEXT_CHARS {
                return Err("supervision.error.reason_too_long");
            }
            Ok(trimmed)
        }
    }
}

fn require_gate<'a>(
    view: &'a RuntimeViewState,
    target: &SupervisionTarget,
) -> Result<&'a MergeGateRecord, &'static str> {
    match target {
        SupervisionTarget::Gate { gate_id } | SupervisionTarget::Bounce { gate_id } => {
            find_gate(view, gate_id).ok_or("supervision.error.record_missing")
        }
        SupervisionTarget::Review { .. } => Err("supervision.error.record_missing"),
    }
}

/// Mirrors `d12_accept_actor`: with a validator recorded, Core requires the
/// acceptance to come from a Lane matching the validator's owner (which needs a
/// Lane id); without one, the actor must equal the gate owner.
fn accept_actor(gate: &MergeGateRecord) -> Option<RuntimeOwner> {
    match &gate.validator {
        Some(validator) => validator
            .owner
            .lane_id
            .is_some()
            .then(|| validator.owner.clone()),
        None => Some(gate.owner.clone()),
    }
}

/// Mirrors `d12_reject_actor`: Core refuses the default owner outright, then
/// admits either the gate owner or the validator's Lane. The gate owner is
/// preferred because it satisfies the rule without the validator's Lane-id
/// requirement.
fn reject_actor(gate: &MergeGateRecord) -> Option<RuntimeOwner> {
    if gate.owner != RuntimeOwner::default() {
        return Some(gate.owner.clone());
    }
    gate.validator
        .as_ref()
        .filter(|validator| validator.owner.lane_id.is_some())
        .filter(|validator| validator.owner != RuntimeOwner::default())
        .map(|validator| validator.owner.clone())
}

/// Mirrors `d12_reviewed_evidence`: with a validator, Core compares the
/// bindings against the review request's recorded set exactly; without one it
/// compares against the gate's own canonical bindings, except for a
/// default-owner gate, where Core fills an empty list in itself.
fn reviewed_evidence(
    view: &RuntimeViewState,
    gate: &MergeGateRecord,
) -> Option<Vec<ReviewedEvidenceBinding>> {
    if let Some(validator) = &gate.validator {
        let review = view
            .review_requests
            .iter()
            .find(|review| review.review_id == validator.review_request_id)?;
        return Some(review.evidence_bindings.clone());
    }
    if gate.owner == RuntimeOwner::default() {
        return Some(Vec::new());
    }
    canonical_bindings(view, gate)
}

/// Mirrors `d12_canonical_bindings`. `None` when any listed evidence is absent
/// or not canonical: Core cannot verify such a gate, so the client has no valid
/// acceptance payload to send.
fn canonical_bindings(
    view: &RuntimeViewState,
    gate: &MergeGateRecord,
) -> Option<Vec<ReviewedEvidenceBinding>> {
    let mut bindings = Vec::with_capacity(gate.evidence_ids.len());
    for evidence_id in &gate.evidence_ids {
        let evidence = view
            .latest_evidence
            .iter()
            .find(|evidence| evidence.id == *evidence_id)?;
        let canonical = evidence.canonical.as_ref()?;
        bindings.push(ReviewedEvidenceBinding {
            evidence_id: evidence_id.clone(),
            source_hash: canonical.source_hash.clone(),
        });
    }
    bindings.sort();
    bindings.dedup();
    Some(bindings)
}

/// The single canonical receipt Core will accept as proof the conflict moved.
///
/// `validate_conflict_revalidation` demands one binding that matches the gate's
/// current canonical bytes *and* differs from every baseline hash captured when
/// the bounce was recorded. Re-sending a baseline receipt would claim the origin
/// lane changed something when it did not, so a view with no newer canonical
/// receipt yields `None` and the action is refused locally.
fn revalidation_evidence(
    view: &RuntimeViewState,
    gate: &MergeGateRecord,
    conflict: &ConflictBounce,
) -> Option<ReviewedEvidenceBinding> {
    canonical_bindings(view, gate)?.into_iter().find(|binding| {
        !conflict
            .baseline_evidence
            .iter()
            .any(|baseline| baseline.source_hash == binding.source_hash)
    })
}

/// Core requires the revert owner to pass `validate_owner` against the gate's
/// task, which the gate owner does by construction. The default owner carries no
/// identity Core can authorize, so it is refused instead of sent.
fn revert_owner(gate: &MergeGateRecord) -> Option<RuntimeOwner> {
    (gate.owner != RuntimeOwner::default()).then(|| gate.owner.clone())
}

/// Core requires the bounce to target the gate owner's own origin Lane
/// (`validate_conflict_bounce`): the owner must equal the gate owner and the
/// lane id must equal that owner's lane. Both are replayed, never chosen.
fn bounce_parties(gate: &MergeGateRecord) -> Option<(RuntimeOwner, String)> {
    if gate.owner == RuntimeOwner::default() {
        return None;
    }
    let lane_id = gate.owner.lane_id.clone()?;
    Some((gate.owner.clone(), lane_id))
}

/// The owner Core accepts as the review decider.
///
/// `validate_review_decider` demands an actor whose Lane is the review's
/// independent reviewer Lane, whose task matches the review, and whose
/// workspace/project match the review owner. Core stores exactly that owner on
/// the gate's validator when the review is requested, so it is replayed from
/// there first. The fallback reproduces Core's own
/// `reviewer_owner_from_requester`: the requester owner re-pointed at the
/// reviewer Lane with session and turn identity left unclaimed.
fn review_actor(view: &RuntimeViewState, review: &ReviewRequestRecord) -> Option<RuntimeOwner> {
    if let Some(validator) =
        find_gate(view, &review.gate_id).and_then(|gate| gate.validator.clone())
        && validator.review_request_id == review.review_id
        && validator.owner.lane_id.as_deref() == Some(review.reviewer_lane_id.as_str())
        && validator.owner != RuntimeOwner::default()
    {
        return Some(validator.owner);
    }
    if review.owner == RuntimeOwner::default() {
        return None;
    }
    let mut reviewer = review.owner.clone();
    reviewer.lane_id = Some(review.reviewer_lane_id.clone());
    reviewer.session_id = None;
    reviewer.turn_id = None;
    Some(reviewer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use viden_core::{EvidenceView, MergeGateType, MergeGateValidator, RuntimeSnapshot};
    use viden_types::{
        CanonicalEvidenceReference, ContextScope, EvidenceProducer, EvidenceQualityFacts,
        EvidenceQualityStatus, EvidenceVerificationState, PermissionLevel, PermissionMode,
        WorkMode,
    };

    pub(super) fn view() -> RuntimeViewState {
        RuntimeViewState::new(RuntimeSnapshot {
            cwd: PathBuf::from("/workspace"),
            provider_family: "fallback".to_string(),
            model_label: "test-local".to_string(),
            work_mode: WorkMode::Build,
            permission_mode: PermissionMode::Default,
            permission_level: PermissionLevel::Ask,
            config_summary: "fixture".to_string(),
            loaded_config_files: Vec::new(),
            startup_overrides: Vec::new(),
            ui_preferences: Default::default(),
        })
    }

    pub(super) fn owner(lane: &str) -> RuntimeOwner {
        RuntimeOwner {
            workspace_id: "workspace".to_string(),
            project_id: "project".to_string(),
            lane_id: Some(lane.to_string()),
            session_id: Some("session-a".to_string()),
            task_id: Some("task-1".to_string()),
            turn_id: Some("turn-1".to_string()),
        }
    }

    pub(super) fn gate(status: MergeGateStatus) -> MergeGateRecord {
        MergeGateRecord {
            gate_id: "gate-1".to_string(),
            task_id: "task-1".to_string(),
            status,
            required_evidence: Vec::new(),
            evidence_ids: vec!["ev-1".to_string()],
            gate_type: MergeGateType::Patch,
            owner: owner("lane-a"),
            validator: None,
            policy_snapshot: Default::default(),
            decision: None,
            conflict: None,
            applied_change_id: Some("change-1".to_string()),
            recovery_snapshot: None,
            audit_ids: Vec::new(),
            updated_at: Some(1),
        }
    }

    pub(super) fn evidence(id: &str, hash: &str) -> EvidenceView {
        EvidenceView {
            id: id.to_string(),
            kind: "test".to_string(),
            summary: "cargo test".to_string(),
            path: None,
            source: None,
            canonical: Some(CanonicalEvidenceReference {
                item_id: format!("item-{id}"),
                bundle_id: "bundle-1".to_string(),
                source_hash: hash.to_string(),
                producer: EvidenceProducer {
                    identity: "lane-a".to_string(),
                    role: "coder".to_string(),
                    task_id: "task-1".to_string(),
                },
                permission_snapshot_id: None,
                permission_scope: ContextScope::Task("task-1".to_string()),
                evidence_scope: ContextScope::Task("task-1".to_string()),
                verification: EvidenceVerificationState::Verified,
                quality: EvidenceQualityFacts {
                    status: EvidenceQualityStatus::Pass,
                    reason_codes: Vec::new(),
                },
            }),
            metadata: None,
            timestamp: Some(1),
            owner: None,
        }
    }

    pub(super) fn review(status: ReviewRequestStatus) -> ReviewRequestRecord {
        ReviewRequestRecord {
            review_id: "review-1".to_string(),
            gate_id: "gate-1".to_string(),
            task_id: "task-1".to_string(),
            requester_lane_id: "lane-a".to_string(),
            reviewer_lane_id: "lane-b".to_string(),
            owner: owner("lane-a"),
            evidence_ids: vec!["ev-1".to_string()],
            evidence_bindings: vec![ReviewedEvidenceBinding {
                evidence_id: "ev-1".to_string(),
                source_hash: "hash-1".to_string(),
            }],
            status,
            feedback: None,
            audit_id: "audit-review".to_string(),
            updated_at: 2,
        }
    }

    pub(super) fn bounce(status: ConflictBounceStatus) -> ConflictBounce {
        ConflictBounce {
            bounce_id: "bounce-1".to_string(),
            gate_id: "gate-1".to_string(),
            task_id: "task-1".to_string(),
            original_lane_id: "lane-a".to_string(),
            owner: owner("lane-a"),
            reason: "base moved".to_string(),
            status,
            evidence_ids: vec!["ev-1".to_string()],
            baseline_evidence: vec![ReviewedEvidenceBinding {
                evidence_id: "ev-1".to_string(),
                source_hash: "hash-baseline".to_string(),
            }],
            revalidation_evidence: Vec::new(),
            content: None,
            audit_id: "audit-bounce".to_string(),
            created_at: 3,
            revalidated_at: None,
        }
    }

    #[test]
    fn action_availability_follows_the_records_current_status() {
        let mut view = view();
        view.merge_gates
            .push(gate(MergeGateStatus::CollectingEvidence));
        let gate_target = SupervisionTarget::Gate {
            gate_id: "gate-1".to_string(),
        };

        assert_eq!(
            available_actions(&view, &gate_target, false),
            vec![SupervisionAction::AcceptGate, SupervisionAction::RejectGate],
            "an open gate with no conflict offers accept and reject"
        );

        // A merged gate can never be accepted or rejected again, and a gate that
        // was never merged can never be reverted: neither control is listed.
        view.merge_gates[0].status = MergeGateStatus::Merged;
        assert_eq!(
            available_actions(&view, &gate_target, false),
            vec![SupervisionAction::Revert]
        );
        view.merge_gates[0].status = MergeGateStatus::Accepted;
        assert!(available_actions(&view, &gate_target, false).is_empty());

        // A pending bounce replaces accept with revalidation until it clears.
        view.merge_gates[0].status = MergeGateStatus::CollectingEvidence;
        view.merge_gates[0].conflict = Some(bounce(ConflictBounceStatus::Pending));
        assert_eq!(
            available_actions(&view, &gate_target, false),
            vec![SupervisionAction::Revalidate, SupervisionAction::RejectGate]
        );
        view.merge_gates[0].conflict = Some(bounce(ConflictBounceStatus::Revalidated));
        assert_eq!(
            available_actions(&view, &gate_target, false),
            vec![SupervisionAction::AcceptGate, SupervisionAction::RejectGate]
        );

        // The stranded-pending escape is additive, never a replacement.
        assert_eq!(
            available_actions(&view, &gate_target, true),
            vec![
                SupervisionAction::AcceptGate,
                SupervisionAction::RejectGate,
                SupervisionAction::Dismiss
            ]
        );

        let review_target = SupervisionTarget::Review {
            review_id: "review-1".to_string(),
        };
        view.review_requests
            .push(review(ReviewRequestStatus::Pending));
        assert_eq!(
            available_actions(&view, &review_target, false),
            vec![
                SupervisionAction::AcceptReview,
                SupervisionAction::RejectReview
            ]
        );
        view.review_requests[0].status = ReviewRequestStatus::Rejected;
        assert!(
            available_actions(&view, &review_target, false).is_empty(),
            "a settled review is history, not a decision"
        );

        // A record that vanished between render and keypress offers nothing.
        assert!(
            available_actions(
                &view,
                &SupervisionTarget::Gate {
                    gate_id: "gate-gone".to_string()
                },
                false
            )
            .is_empty()
        );
    }

    #[test]
    fn accept_payload_mirrors_the_gui_actor_and_reviewed_evidence_derivation() {
        let mut view = view();
        view.merge_gates
            .push(gate(MergeGateStatus::CollectingEvidence));
        view.latest_evidence.push(evidence("ev-1", "hash-1"));
        let target = SupervisionTarget::Gate {
            gate_id: "gate-1".to_string(),
        };

        // No validator: the actor is the gate owner and the bindings are
        // rebuilt from Core's canonical references.
        let dispatch = build_dispatch(&view, &target, SupervisionAction::AcceptGate, "")
            .expect("accept payload");
        assert_eq!(
            dispatch.command,
            RuntimeCommand::AcceptMergeGate {
                gate_id: "gate-1".to_string(),
                actor: owner("lane-a"),
                reviewed_evidence: vec![ReviewedEvidenceBinding {
                    evidence_id: "ev-1".to_string(),
                    source_hash: "hash-1".to_string(),
                }],
                decision: None,
            }
        );
        assert_eq!(dispatch.owner, owner("lane-a"));
        assert_eq!(
            dispatch.expect,
            SupervisionExpectation::MergeGate {
                gate_id: "gate-1".to_string(),
                status: MergeGateStatus::Accepted
            }
        );

        // A listed evidence id with no canonical reference cannot be verified by
        // Core, so there is no valid payload to send.
        view.latest_evidence[0].canonical = None;
        assert_eq!(
            build_dispatch(&view, &target, SupervisionAction::AcceptGate, ""),
            Err("supervision.error.no_canonical_evidence")
        );
        view.latest_evidence[0] = evidence("ev-1", "hash-1");

        // With a validator the actor is the validator Lane and the bindings are
        // the review request's recorded set, verbatim.
        view.merge_gates[0].validator = Some(MergeGateValidator {
            owner: owner("lane-b"),
            review_request_id: "review-1".to_string(),
            independent: true,
            validated_at: None,
        });
        view.review_requests
            .push(review(ReviewRequestStatus::Accepted));
        view.review_requests[0].evidence_bindings = vec![ReviewedEvidenceBinding {
            evidence_id: "ev-1".to_string(),
            source_hash: "hash-reviewed".to_string(),
        }];
        let dispatch = build_dispatch(&view, &target, SupervisionAction::AcceptGate, "")
            .expect("validator accept payload");
        assert_eq!(
            dispatch.command,
            RuntimeCommand::AcceptMergeGate {
                gate_id: "gate-1".to_string(),
                actor: owner("lane-b"),
                reviewed_evidence: vec![ReviewedEvidenceBinding {
                    evidence_id: "ev-1".to_string(),
                    source_hash: "hash-reviewed".to_string(),
                }],
                decision: None,
            }
        );

        // A validator with no Lane id is not an actor Core can match.
        view.merge_gates[0]
            .validator
            .as_mut()
            .expect("validator")
            .owner
            .lane_id = None;
        assert_eq!(
            build_dispatch(&view, &target, SupervisionAction::AcceptGate, ""),
            Err("supervision.error.no_actor")
        );
    }

    #[test]
    fn reject_revert_and_bounce_replay_core_owned_parties_and_require_a_reason() {
        let mut view = view();
        view.merge_gates
            .push(gate(MergeGateStatus::CollectingEvidence));
        let target = SupervisionTarget::Gate {
            gate_id: "gate-1".to_string(),
        };

        assert_eq!(
            build_dispatch(&view, &target, SupervisionAction::RejectGate, "   "),
            Err("supervision.error.reason_required")
        );
        assert_eq!(
            build_dispatch(
                &view,
                &target,
                SupervisionAction::RejectGate,
                &"x".repeat(MAX_TRUST_TEXT_CHARS + 1)
            ),
            Err("supervision.error.reason_too_long"),
            "over-limit text is refused, never silently truncated"
        );
        let dispatch = build_dispatch(
            &view,
            &target,
            SupervisionAction::RejectGate,
            " evidence missing ",
        )
        .expect("reject payload");
        assert_eq!(
            dispatch.command,
            RuntimeCommand::RejectMergeGate {
                gate_id: "gate-1".to_string(),
                actor: owner("lane-a"),
                reason: "evidence missing".to_string(),
            }
        );
        assert_eq!(
            dispatch.expect,
            SupervisionExpectation::MergeGate {
                gate_id: "gate-1".to_string(),
                status: MergeGateStatus::NeedsChanges
            }
        );

        view.merge_gates[0].status = MergeGateStatus::Merged;
        let dispatch = build_dispatch(
            &view,
            &target,
            SupervisionAction::Revert,
            "regression in main",
        )
        .expect("revert payload");
        assert_eq!(
            dispatch.command,
            RuntimeCommand::RevertAppliedChange {
                gate_id: "gate-1".to_string(),
                owner: owner("lane-a"),
                reason: "regression in main".to_string(),
            }
        );
        assert_eq!(
            dispatch.expect,
            SupervisionExpectation::Revert {
                gate_id: "gate-1".to_string()
            }
        );

        view.merge_gates[0].status = MergeGateStatus::CollectingEvidence;
        let bounce_target = SupervisionTarget::Bounce {
            gate_id: "gate-1".to_string(),
        };
        let dispatch = build_dispatch(
            &view,
            &bounce_target,
            SupervisionAction::Bounce,
            "base moved",
        )
        .expect("bounce payload");
        assert_eq!(
            dispatch.command,
            RuntimeCommand::BounceMergeConflict {
                gate_id: "gate-1".to_string(),
                original_lane_id: "lane-a".to_string(),
                owner: owner("lane-a"),
                reason: "base moved".to_string(),
            }
        );
        assert_eq!(
            dispatch.expect,
            SupervisionExpectation::ConflictBounce {
                gate_id: "gate-1".to_string()
            }
        );

        // A default-owner gate carries no identity Core would authorize.
        view.merge_gates[0].owner = RuntimeOwner::default();
        assert_eq!(
            build_dispatch(
                &view,
                &bounce_target,
                SupervisionAction::Bounce,
                "base moved"
            ),
            Err("supervision.error.no_actor")
        );
    }

    #[test]
    fn revalidation_carries_the_bounce_identity_and_a_changed_canonical_receipt() {
        let mut view = view();
        let mut record = gate(MergeGateStatus::CollectingEvidence);
        record.conflict = Some(bounce(ConflictBounceStatus::Pending));
        view.merge_gates.push(record);
        view.conflict_bounces
            .push(bounce(ConflictBounceStatus::Pending));
        view.latest_evidence.push(evidence("ev-1", "hash-baseline"));
        let target = SupervisionTarget::Gate {
            gate_id: "gate-1".to_string(),
        };

        // The origin Lane has not produced a new receipt yet, so there is
        // nothing Core would accept as proof the conflict moved.
        assert_eq!(
            build_dispatch(&view, &target, SupervisionAction::Revalidate, ""),
            Err("supervision.error.no_revalidation_evidence")
        );

        view.latest_evidence[0] = evidence("ev-1", "hash-revalidated");
        let dispatch = build_dispatch(&view, &target, SupervisionAction::Revalidate, "")
            .expect("revalidate payload");
        assert_eq!(
            dispatch.command,
            RuntimeCommand::RevalidateMergeConflict {
                gate_id: "gate-1".to_string(),
                bounce_id: "bounce-1".to_string(),
                actor: owner("lane-a"),
                evidence: ReviewedEvidenceBinding {
                    evidence_id: "ev-1".to_string(),
                    source_hash: "hash-revalidated".to_string(),
                },
            }
        );
        assert_eq!(
            dispatch.expect,
            SupervisionExpectation::MergeGate {
                gate_id: "gate-1".to_string(),
                status: MergeGateStatus::CollectingEvidence
            },
            "revalidation confirms on the gate returning to evidence collection"
        );
    }

    #[test]
    fn review_verdicts_carry_the_reviewer_lane_actor_and_optional_feedback() {
        let mut view = view();
        let mut record = gate(MergeGateStatus::CollectingEvidence);
        record.validator = Some(MergeGateValidator {
            owner: owner("lane-b"),
            review_request_id: "review-1".to_string(),
            independent: true,
            validated_at: None,
        });
        view.merge_gates.push(record);
        view.review_requests
            .push(review(ReviewRequestStatus::Pending));
        let target = SupervisionTarget::Review {
            review_id: "review-1".to_string(),
        };

        // Accept with no feedback: Core's field is optional, so the client must
        // not invent a stricter rule than Core.
        let dispatch = build_dispatch(&view, &target, SupervisionAction::AcceptReview, "")
            .expect("accept review payload");
        assert_eq!(
            dispatch.command,
            RuntimeCommand::DecideReview {
                review_id: "review-1".to_string(),
                verdict: ReviewVerdict::Accepted,
                feedback: None,
                actor: owner("lane-b"),
            }
        );
        assert_eq!(
            dispatch.expect,
            SupervisionExpectation::Review {
                review_id: "review-1".to_string(),
                status: ReviewRequestStatus::Accepted
            }
        );

        // Reject with feedback, and rejection without feedback is equally valid.
        let dispatch = build_dispatch(
            &view,
            &target,
            SupervisionAction::RejectReview,
            "needs a regression test",
        )
        .expect("reject review payload");
        assert_eq!(
            dispatch.command,
            RuntimeCommand::DecideReview {
                review_id: "review-1".to_string(),
                verdict: ReviewVerdict::Rejected,
                feedback: Some("needs a regression test".to_string()),
                actor: owner("lane-b"),
            }
        );
        assert_eq!(
            dispatch.expect,
            SupervisionExpectation::Review {
                review_id: "review-1".to_string(),
                status: ReviewRequestStatus::Rejected
            }
        );
        assert!(build_dispatch(&view, &target, SupervisionAction::RejectReview, "").is_ok());

        // Without the validator record, the reviewer owner is derived exactly
        // the way Core derives it from the requester.
        view.merge_gates[0].validator = None;
        let dispatch = build_dispatch(&view, &target, SupervisionAction::AcceptReview, "")
            .expect("derived reviewer actor");
        assert_eq!(
            dispatch.owner,
            RuntimeOwner {
                workspace_id: "workspace".to_string(),
                project_id: "project".to_string(),
                lane_id: Some("lane-b".to_string()),
                session_id: None,
                task_id: Some("task-1".to_string()),
                turn_id: None,
            }
        );
    }

    #[test]
    fn decision_picks_list_approvals_gates_pending_reviews_then_pending_bounces() {
        let mut view = view();
        view.merge_gates
            .push(gate(MergeGateStatus::CollectingEvidence));
        view.review_requests
            .push(review(ReviewRequestStatus::Pending));
        let mut settled = review(ReviewRequestStatus::Accepted);
        settled.review_id = "review-settled".to_string();
        view.review_requests.push(settled);
        view.conflict_bounces
            .push(bounce(ConflictBounceStatus::Pending));
        let mut resolved = bounce(ConflictBounceStatus::Resolved);
        resolved.bounce_id = "bounce-resolved".to_string();
        view.conflict_bounces.push(resolved);

        assert_eq!(
            decision_picks(&view, false),
            vec![
                DecisionPick::Supervision(SupervisionTarget::Gate {
                    gate_id: "gate-1".to_string()
                }),
                DecisionPick::Supervision(SupervisionTarget::Review {
                    review_id: "review-1".to_string()
                }),
                DecisionPick::Supervision(SupervisionTarget::Bounce {
                    gate_id: "gate-1".to_string()
                }),
                DecisionPick::AuditTimeline,
            ],
            "settled reviews and resolved bounces are history, not decisions"
        );
        assert_eq!(
            decision_picks(&view, true)
                .into_iter()
                .rev()
                .take(2)
                .collect::<Vec<_>>(),
            vec![
                DecisionPick::AuditTimeline,
                DecisionPick::DismissSupervision
            ],
            "the non-decision entries are appended, never interleaved"
        );
    }

    #[test]
    fn the_audit_row_is_offered_for_every_target_including_ones_with_no_decision_left() {
        let mut view = view();
        view.merge_gates.push(gate(MergeGateStatus::Accepted));
        let gate_target = SupervisionTarget::Gate {
            gate_id: "gate-1".to_string(),
        };

        assert!(
            available_actions(&view, &gate_target, false).is_empty(),
            "an accepted gate accepts no further decision"
        );
        assert_eq!(
            overlay_actions(&view, &gate_target, false),
            vec![SupervisionAction::AuditTrail],
            "its history is still readable"
        );

        view.merge_gates[0].status = MergeGateStatus::CollectingEvidence;
        assert_eq!(
            overlay_actions(&view, &gate_target, true),
            vec![
                SupervisionAction::AcceptGate,
                SupervisionAction::RejectGate,
                SupervisionAction::Dismiss,
                SupervisionAction::AuditTrail,
            ],
            "the audit row is last, so it never shifts a decision's number"
        );

        // The audit row dispatches nothing: it is refused as a Core command and
        // is routed to the read-only overlay instead.
        assert_eq!(
            build_dispatch(&view, &gate_target, SupervisionAction::AuditTrail, ""),
            Err("supervision.error.not_dispatchable")
        );
    }

    #[test]
    fn audit_scope_uses_the_contracts_own_object_kind_constants() {
        assert_eq!(
            audit_scope(&SupervisionTarget::Gate {
                gate_id: "gate-1".to_string()
            }),
            AuditObjectRef::new(AuditObjectRef::KIND_MERGE_GATE, "gate-1")
        );
        assert_eq!(
            audit_scope(&SupervisionTarget::Bounce {
                gate_id: "gate-1".to_string()
            }),
            AuditObjectRef::new(AuditObjectRef::KIND_MERGE_GATE, "gate-1"),
            "a conflict bounce is audited against the gate it belongs to"
        );
        assert_eq!(
            audit_scope(&SupervisionTarget::Review {
                review_id: "review-1".to_string()
            }),
            AuditObjectRef::new(AuditObjectRef::KIND_REVIEW_REQUEST, "review-1")
        );
    }
}

#[cfg(test)]
mod dormant_gate_tests {
    use super::tests::{gate, view};
    use super::*;
    use viden_core::{AgentSessionView, MergeGateStatus};
    use viden_types::{MergeGateDecision, MergeGateDecisionOutcome};

    fn session(session_id: &str, status: AgentSessionStatus) -> AgentSessionView {
        AgentSessionView {
            session_id: session_id.to_string(),
            lane_id: "lane-a".to_string(),
            agent_id: "codex".to_string(),
            model: None,
            status,
            owner: Default::default(),
            task: "work".to_string(),
            diagnostic: None,
            output: None,
        }
    }

    /// Builds a view holding one gate bound to one session.
    fn view_with(
        gate_status: MergeGateStatus,
        session_status: AgentSessionStatus,
    ) -> RuntimeViewState {
        let mut view = view();
        let mut record = gate(gate_status);
        record.owner.session_id = Some("session-a".to_string());
        view.merge_gates.push(record);
        view.agent_sessions
            .push(session("session-a", session_status));
        view
    }

    /// The classification table: pending + terminal session is dormant, and
    /// nothing else is.
    #[test]
    fn dormancy_requires_a_pending_decision_and_a_finished_session() {
        for terminal in [
            AgentSessionStatus::Completed,
            AgentSessionStatus::Failed,
            AgentSessionStatus::Cancelled,
        ] {
            let view = view_with(MergeGateStatus::Proposed, terminal);
            assert!(
                is_dormant_gate(&view, &view.merge_gates[0]),
                "pending gate on a {terminal:?} session is dormant"
            );
        }
        for live in [
            AgentSessionStatus::Starting,
            AgentSessionStatus::Running,
            AgentSessionStatus::WaitingApproval,
        ] {
            let view = view_with(MergeGateStatus::Proposed, live);
            assert!(
                !is_dormant_gate(&view, &view.merge_gates[0]),
                "pending gate on a {live:?} session is actionable, not dormant"
            );
        }
    }

    /// A decided gate is settled history, never dormant residue.
    #[test]
    fn a_decided_gate_on_a_finished_session_is_settled_not_dormant() {
        let mut view = view_with(MergeGateStatus::Proposed, AgentSessionStatus::Completed);
        view.merge_gates[0].decision = Some(MergeGateDecision::decided_now(
            MergeGateDecisionOutcome::Accepted,
            "looks right".to_string(),
            Default::default(),
            Vec::new(),
            "audit-1".to_string(),
        ));
        assert!(!is_dormant_gate(&view, &view.merge_gates[0]));

        // A closed status is settled too, even with no decision record.
        let mut view = view_with(MergeGateStatus::Merged, AgentSessionStatus::Completed);
        view.merge_gates[0].decision = None;
        assert!(!is_dormant_gate(&view, &view.merge_gates[0]));
    }

    /// An unknown session is not a finished one: without a session this view
    /// has actually seen, the gate stays first-class.
    #[test]
    fn a_gate_whose_session_is_unknown_is_never_dormant() {
        let mut view = view_with(MergeGateStatus::Proposed, AgentSessionStatus::Completed);
        view.agent_sessions.clear();
        assert!(!is_dormant_gate(&view, &view.merge_gates[0]));

        let mut view = view_with(MergeGateStatus::Proposed, AgentSessionStatus::Completed);
        view.merge_gates[0].owner.session_id = None;
        assert!(!is_dormant_gate(&view, &view.merge_gates[0]));
    }

    /// Dormant gates are ordered after every actionable decision, and stay
    /// pickable so the operator can still decide them.
    #[test]
    fn dormant_gates_are_ordered_after_active_decisions_and_stay_pickable() {
        let mut view = view();
        view.agent_sessions
            .push(session("session-live", AgentSessionStatus::Running));
        view.agent_sessions
            .push(session("session-done", AgentSessionStatus::Completed));
        // Source order deliberately puts the dormant gate first.
        for (gate_id, session_id) in [
            ("gate-dormant", "session-done"),
            ("gate-active", "session-live"),
        ] {
            let mut record = gate(MergeGateStatus::Proposed);
            record.gate_id = gate_id.to_string();
            record.owner.session_id = Some(session_id.to_string());
            view.merge_gates.push(record);
        }

        let picks = decision_picks(&view, false);
        let gate_order = picks
            .iter()
            .filter_map(|pick| match pick {
                DecisionPick::Supervision(SupervisionTarget::Gate { gate_id }) => {
                    Some(gate_id.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            gate_order,
            vec!["gate-active", "gate-dormant"],
            "the active gate must come first even though the dormant one is listed first"
        );
        assert_eq!(dormant_gate_count(&view), 1);
        // Grouping, never hiding: the dormant gate is still a pick.
        assert!(
            picks.contains(&DecisionPick::Supervision(SupervisionTarget::Gate {
                gate_id: "gate-dormant".to_string(),
            }))
        );
    }
}
