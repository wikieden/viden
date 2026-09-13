//! The plan lens: reading and deciding one Core-owned plan document
//! (`runtime.plan_documents`, C13).
//!
//! This module is pure. It reads `RuntimeViewState.plan_documents`, returns
//! rendered rows and frozen `RuntimeCommand`s, and performs no dispatch. Four
//! rules shape it, each because dropping it produces a specific lie:
//!
//! - **Absence is "no plan proposed".** An empty `plan_documents` is a real
//!   answer. Nothing here parses a plan out of assistant prose, keeps one in
//!   client state, or renders an empty plan as a plan.
//! - **Every decision is pinned.** `Accept`, `Reject`, and `Hand off` carry the
//!   exact `revision` and `content_sha256` of the record that was *rendered*,
//!   re-read on the keypress. A decision that only named the plan id would
//!   accept bytes the operator never saw.
//! - **Refusals come from Core.** `plan_revision_stale` and `plan_mode` are
//!   shown as Core's own reason text. This client composes neither, and it
//!   never retries a refused decision on the operator's behalf.
//! - **Status gates the keys, and the gate is stated.** Only a `Proposed` plan
//!   is decidable and only an `Accepted` one is handed off; a key that cannot
//!   apply is listed and labelled with the status that blocked it rather than
//!   hidden.

use viden_core::{
    PlanAuthor, PlanDocumentRecord, PlanDocumentStatus, PlanStepStatus, RuntimeCommand,
};

use super::glyphs::GlyphSet;
use super::preferences::{ColorDepth, TerminalCapabilities, resolve_appearance};
use super::state::TuiState;

/// The capability that publishes plan documents and their decisions.
pub(super) const PLAN_DOCUMENTS_CAPABILITY: &str = "runtime.plan_documents";

/// Core's bound on the reason a rejection carries
/// (`viden_types::MAX_PLAN_REJECT_REASON_BYTES`). The client refuses
/// over-limit input rather than truncating it, so an operator never discovers
/// afterwards that Core stored half a sentence.
pub(super) const MAX_PLAN_REJECT_REASON_BYTES: usize = viden_types::MAX_PLAN_REJECT_REASON_BYTES;

/// The three operator decisions a plan record offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlanDecision {
    Accept,
    Reject,
    HandOff,
}

impl PlanDecision {
    pub(super) const fn label_key(self) -> &'static str {
        match self {
            Self::Accept => "plan.action.accept",
            Self::Reject => "plan.action.reject",
            Self::HandOff => "plan.action.handoff",
        }
    }

    /// Whether this decision can apply to a plan in `status`.
    ///
    /// An unknown status answers `false` for every decision: a
    /// `#[non_exhaustive]` status this build cannot name must not be treated
    /// as `Proposed` and must not be decided.
    pub(super) fn applies_to(self, status: PlanDocumentStatus) -> bool {
        match self {
            Self::Accept | Self::Reject => status == PlanDocumentStatus::Proposed,
            Self::HandOff => status == PlanDocumentStatus::Accepted,
        }
    }

    /// Whether Core stores operator text for this decision and refuses an
    /// empty one.
    pub(super) const fn needs_reason(self) -> bool {
        matches!(self, Self::Reject)
    }
}

/// One plan decision this client sent and has not seen settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PendingPlanDecision {
    pub(super) command_id: String,
    pub(super) decision: PlanDecision,
}

/// Core's own refusal of one plan decision, kept beside the decision it
/// refused so the row says which key was answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PlanRefusal {
    pub(super) decision: PlanDecision,
    pub(super) reason: String,
}

/// TUI-local state of the plan lens.
///
/// Presentation only. It remembers the scroll offset, the rejection reason
/// being typed, the last local refusal, and which command id is in flight. The
/// plan itself is re-read from `RuntimeViewState` on every frame and on every
/// keypress, so a revision that changed between render and keypress is decided
/// on its current bytes or refused by Core as stale — never decided from a
/// row this client cached.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct PlanPanel {
    pub(super) scroll: usize,
    /// Set while the operator is typing the rejection reason. `None` means the
    /// action row has focus.
    pub(super) reason: Option<String>,
    /// Catalog key of the last local refusal (empty reason, over-bound text).
    /// Cleared as soon as the operator acts again.
    pub(super) notice: Option<String>,
    pending: Option<PendingPlanDecision>,
    refusal: Option<PlanRefusal>,
}

impl PlanPanel {
    pub(super) fn begin(&mut self, command_id: impl Into<String>, decision: PlanDecision) {
        self.notice = None;
        self.refusal = None;
        self.pending = Some(PendingPlanDecision {
            command_id: command_id.into(),
            decision,
        });
    }

    pub(super) fn pending(&self) -> Option<&PendingPlanDecision> {
        self.pending.as_ref()
    }

    pub(super) fn refusal(&self) -> Option<&PlanRefusal> {
        self.refusal.as_ref()
    }

    /// Reconciles one ordered Core event against the in-flight decision.
    ///
    /// A `CommandRejected` for this client's own command id is the refusal;
    /// the settlement facts clear the slot. Nothing here changes the plan: the
    /// record arrives through `PlanDocumentUpdated` and is read from the view.
    pub(super) fn observe_event(&mut self, event: &viden_core::RuntimeEvent) -> bool {
        let Some(pending) = self.pending.as_ref() else {
            return false;
        };
        match &event.kind {
            viden_core::RuntimeEventKind::CommandRejected { command_id, reason }
                if command_id == &pending.command_id =>
            {
                self.refusal = Some(PlanRefusal {
                    decision: pending.decision,
                    reason: reason.clone(),
                });
                self.pending = None;
                true
            }
            viden_core::RuntimeEventKind::PlanDocumentDecided { .. }
            | viden_core::RuntimeEventKind::PlanDocumentHandedOff { .. } => {
                self.pending = None;
                true
            }
            _ => false,
        }
    }
}

/// The plan record the lens is looking at, or `None` for "no plan proposed".
///
/// A focused Lane narrows to the plans Core scoped to that Lane, and only when
/// Core published at least one for it: narrowing to an empty set would render
/// "no plan proposed" while the session's own plan sits one field away. Within
/// the candidates the *open* plan wins — `Drafting`, `Proposed`, `Accepted` —
/// because that is the one an operator can still act on, and ties break on
/// `updated_at`, which is Core's own ordering rather than this client's.
pub(super) fn focused_plan(state: &TuiState) -> Option<&PlanDocumentRecord> {
    let lane_scoped = state
        .ui
        .focused_lane
        .as_deref()
        .map(|lane_id| {
            state
                .runtime
                .plan_documents
                .iter()
                .filter(|plan| plan.owner.lane_id.as_deref() == Some(lane_id))
                .collect::<Vec<_>>()
        })
        .filter(|plans| !plans.is_empty());
    let candidates: Vec<&PlanDocumentRecord> = match lane_scoped {
        Some(plans) => plans,
        None => state.runtime.plan_documents.iter().collect(),
    };
    candidates
        .into_iter()
        .max_by_key(|plan| (plan.status.is_open(), plan.updated_at))
}

/// The plan lens as the overlay renders it: Core's rows through the panel's
/// own scroll window.
///
/// The window is applied here rather than inside [`plan_rows`] so the row
/// builder stays a pure function of the record, which is what the tests assert
/// against.
pub(super) fn plan_lens_rows(state: &TuiState) -> Vec<String> {
    let appearance = resolve_appearance(
        &state.runtime.snapshot.ui_preferences,
        ColorDepth::Auto,
        TerminalCapabilities::default(),
    );
    let rows = plan_rows(state, appearance.glyphs);
    let scroll = state
        .ui
        .plan
        .as_ref()
        .map(|panel| panel.scroll)
        .unwrap_or_default()
        .min(rows.len().saturating_sub(1));
    rows.into_iter().skip(scroll).collect()
}

/// The rows of the plan lens, before the panel's own scroll window.
pub(super) fn plan_rows(state: &TuiState, glyphs: GlyphSet) -> Vec<String> {
    if !state.has_capability(PLAN_DOCUMENTS_CAPABILITY) {
        return vec![super::i18n::translate(
            state,
            "plan.unavailable",
            &[("capability", PLAN_DOCUMENTS_CAPABILITY)],
        )];
    }
    let Some(plan) = focused_plan(state) else {
        return vec![super::i18n::text(state, "plan.empty")];
    };
    let mut rows = vec![
        super::i18n::translate(
            state,
            "plan.header",
            &[
                ("title", plan.title.as_str()),
                ("status", &status_label(state, plan.status)),
                ("revision", &plan.revision.to_string()),
            ],
        ),
        super::i18n::translate(state, "plan.goal", &[("goal", plan.goal.trim())]),
        author_row(state, &plan.proposed_by),
        super::i18n::translate(
            state,
            "plan.digest",
            &[("sha", plan.content_sha256.as_str())],
        ),
    ];
    if plan.status == PlanDocumentStatus::Superseded {
        rows.push(super::i18n::text(state, "plan.superseded"));
    }
    for section in &plan.sections {
        rows.push(super::i18n::translate(
            state,
            "plan.section",
            &[("kind", section.kind.as_str())],
        ));
        rows.extend(
            section
                .body
                .lines()
                .map(|line| format!("  {}", line.trim_end())),
        );
    }
    rows.push(super::i18n::translate(
        state,
        "plan.steps",
        &[("count", &plan.steps.len().to_string())],
    ));
    for step in &plan.steps {
        let dependencies = if step.dependencies.is_empty() {
            String::new()
        } else {
            super::i18n::translate(
                state,
                "plan.step.dependencies",
                &[("dependencies", &step.dependencies.join(", "))],
            )
        };
        let role = step
            .role
            .map(|role| super::i18n::translate(state, "plan.step.role", &[("role", role.as_str())]))
            .unwrap_or_default();
        rows.push(super::i18n::translate(
            state,
            "plan.step",
            &[
                ("glyph", step_glyph(glyphs, step.status)),
                ("id", step.id.as_str()),
                ("title", step.title.as_str()),
                ("role", role.as_str()),
                ("dependencies", dependencies.as_str()),
            ],
        ));
    }
    if let Some(handoff) = plan.handoff.as_ref() {
        rows.push(super::i18n::translate(
            state,
            "plan.handoff.linked",
            &[
                ("dag", handoff.dag_id.as_str()),
                ("audit", handoff.audit_id.as_str()),
            ],
        ));
    }
    rows.extend(decision_rows(state, plan));
    if let Some(panel) = state.ui.plan.as_ref() {
        if let Some(reason) = panel.reason.as_deref() {
            rows.push(super::i18n::text(state, "plan.reject.prompt"));
            rows.push(format!("› {reason}"));
        }
        if let Some(notice) = panel.notice.as_deref() {
            rows.push(super::i18n::text(state, notice));
        }
        if let Some(refusal) = panel.refusal() {
            rows.push(super::i18n::translate(
                state,
                "plan.refusal",
                &[
                    (
                        "action",
                        &super::i18n::text(state, refusal.decision.label_key()),
                    ),
                    ("reason", refusal.reason.trim()),
                ],
            ));
        }
        if let Some(pending) = panel.pending() {
            rows.push(super::i18n::translate(
                state,
                "plan.sent",
                &[("command_id", pending.command_id.as_str())],
            ));
        }
    }
    rows
}

/// The three decision rows, each stating what blocks it when it is blocked.
///
/// Hand off keeps its key in Plan mode and is labelled instead of hidden: Core
/// is the only authority that may refuse it, and the label is what tells the
/// operator the refusal is expected rather than a fault.
fn decision_rows(state: &TuiState, plan: &PlanDocumentRecord) -> Vec<String> {
    [
        PlanDecision::Accept,
        PlanDecision::Reject,
        PlanDecision::HandOff,
    ]
    .into_iter()
    .map(|decision| {
        let mut availability = if decision.applies_to(plan.status) {
            String::new()
        } else {
            super::i18n::translate(
                state,
                "plan.action.unavailable",
                &[("status", &status_label(state, plan.status))],
            )
        };
        if decision == PlanDecision::HandOff
            && state.runtime.snapshot.permission_mode == viden_core::PermissionMode::Plan
        {
            availability.push_str(&super::i18n::text(state, "plan.handoff.plan_mode"));
        }
        super::i18n::translate(
            state,
            decision.label_key(),
            &[("availability", availability.as_str())],
        )
    })
    .collect()
}

fn author_row(state: &TuiState, author: &PlanAuthor) -> String {
    match author {
        PlanAuthor::Assistant {
            session_id,
            turn_id,
        } => super::i18n::translate(
            state,
            "plan.author.assistant",
            &[("session", session_id.as_str()), ("turn", turn_id.as_str())],
        ),
        PlanAuthor::Operator => super::i18n::text(state, "plan.author.operator"),
        // `PlanAuthor` is `#[non_exhaustive]`: a newer Core may name an author
        // this build cannot. Say so rather than attributing the revision to
        // whichever side happens to be listed first.
        _ => super::i18n::text(state, "plan.author.unknown"),
    }
}

fn status_label(state: &TuiState, status: PlanDocumentStatus) -> String {
    match status {
        PlanDocumentStatus::Drafting
        | PlanDocumentStatus::Proposed
        | PlanDocumentStatus::Accepted
        | PlanDocumentStatus::Rejected
        | PlanDocumentStatus::HandedOff
        | PlanDocumentStatus::Superseded => status.as_str().to_string(),
        // Rendered in place as unknown, never as `Proposed` and never as
        // decided, exactly as the contract requires.
        _ => super::i18n::text(state, "plan.status.unknown"),
    }
}

fn step_glyph(glyphs: GlyphSet, status: PlanStepStatus) -> &'static str {
    match status {
        PlanStepStatus::Pending => glyphs.wait,
        PlanStepStatus::InProgress => glyphs.run,
        PlanStepStatus::Completed => glyphs.done,
    }
}

/// The exact command one decision sends for the record that was rendered.
///
/// `revision` and `content_sha256` are copied out of the record rather than
/// remembered, so the pin always names bytes Core published.
pub(super) fn plan_decision_command(
    plan: &PlanDocumentRecord,
    decision: PlanDecision,
    reason: Option<String>,
) -> RuntimeCommand {
    match decision {
        PlanDecision::Accept => RuntimeCommand::AcceptPlanDocument {
            plan_id: plan.plan_id.clone(),
            revision: plan.revision,
            content_sha256: plan.content_sha256.clone(),
        },
        PlanDecision::Reject => RuntimeCommand::RejectPlanDocument {
            plan_id: plan.plan_id.clone(),
            revision: plan.revision,
            content_sha256: plan.content_sha256.clone(),
            reason: reason.unwrap_or_default(),
        },
        PlanDecision::HandOff => RuntimeCommand::HandOffPlanDocument {
            plan_id: plan.plan_id.clone(),
            revision: plan.revision,
            content_sha256: plan.content_sha256.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use viden_core::{
        PlanDocumentId, PlanSection, PlanSectionKind, PlanStep, RuntimeEvent, RuntimeEventKind,
        RuntimeOwner,
    };
    use viden_types::CapabilityId;

    fn record(status: PlanDocumentStatus, revision: u32) -> PlanDocumentRecord {
        PlanDocumentRecord {
            plan_id: PlanDocumentId("plan_1".to_string()),
            owner: RuntimeOwner {
                workspace_id: "ws".to_string(),
                project_id: "prj".to_string(),
                session_id: Some("session_1".to_string()),
                ..RuntimeOwner::default()
            },
            title: "Model the plan document".to_string(),
            goal: "Give the operator a decidable plan".to_string(),
            status,
            revision,
            content_sha256: "abc123".to_string(),
            sections: vec![PlanSection {
                kind: PlanSectionKind::Requirements,
                body: "One decidable plan per owner".to_string(),
            }],
            steps: vec![
                PlanStep {
                    id: "step_1".to_string(),
                    title: "Model the record".to_string(),
                    status: PlanStepStatus::Completed,
                    role: None,
                    file_scope: Vec::new(),
                    dependencies: Vec::new(),
                },
                PlanStep {
                    id: "step_2".to_string(),
                    title: "Decide it".to_string(),
                    status: PlanStepStatus::Pending,
                    role: None,
                    file_scope: Vec::new(),
                    dependencies: vec!["step_1".to_string()],
                },
            ],
            proposed_by: PlanAuthor::Assistant {
                session_id: "session_1".to_string(),
                turn_id: "turn_1".to_string(),
            },
            created_at: 1,
            updated_at: 2,
            decided_at: None,
            decision_audit_id: None,
            handoff: None,
        }
    }

    fn state_with(plan: Option<PlanDocumentRecord>) -> TuiState {
        let mut state = TuiState::default();
        state
            .capabilities
            .insert(CapabilityId(PLAN_DOCUMENTS_CAPABILITY.to_string()));
        state.ui.plan = Some(PlanPanel::default());
        if let Some(plan) = plan {
            state.runtime.plan_documents.push(plan);
        }
        state
    }

    #[test]
    fn an_empty_plan_list_renders_as_absence_not_as_an_empty_plan() {
        let state = state_with(None);

        let rows = plan_rows(&state, GlyphSet::new(true));

        assert_eq!(rows, vec!["No plan proposed.".to_string()]);
    }

    #[test]
    fn a_client_without_the_capability_names_it_instead_of_drawing_a_plan() {
        let mut state = state_with(Some(record(PlanDocumentStatus::Proposed, 2)));
        state.capabilities.clear();

        let rows = plan_rows(&state, GlyphSet::new(true));

        assert_eq!(rows.len(), 1);
        assert!(rows[0].contains(PLAN_DOCUMENTS_CAPABILITY), "{rows:?}");
    }

    #[test]
    fn rows_carry_the_title_status_revision_steps_and_registered_step_glyphs() {
        let state = state_with(Some(record(PlanDocumentStatus::Proposed, 2)));

        let rendered = plan_rows(&state, GlyphSet::new(true)).join("\n");

        assert!(rendered.contains("Model the plan document"), "{rendered}");
        assert!(rendered.contains("proposed"), "{rendered}");
        assert!(rendered.contains("rev 2"), "{rendered}");
        assert!(rendered.contains("requirements"), "{rendered}");
        assert!(rendered.contains("✓ step_1"), "{rendered}");
        assert!(rendered.contains("◌ step_2"), "{rendered}");
        assert!(rendered.contains("after step_1"), "{rendered}");
    }

    #[test]
    fn decision_rows_state_the_status_that_blocks_each_key() {
        let state = state_with(Some(record(PlanDocumentStatus::Proposed, 2)));

        let rendered = plan_rows(&state, GlyphSet::new(true)).join("\n");

        assert!(rendered.contains("a Accept\n"), "{rendered}");
        assert!(
            rendered.contains("h Hand off · unavailable in proposed"),
            "{rendered}"
        );
    }

    #[test]
    fn hand_off_stays_offered_in_plan_mode_and_says_core_will_refuse_it() {
        let mut state = state_with(Some(record(PlanDocumentStatus::Accepted, 2)));
        state.runtime.snapshot.permission_mode = viden_core::PermissionMode::Plan;

        let rendered = plan_rows(&state, GlyphSet::new(true)).join("\n");

        assert!(
            rendered.contains("h Hand off · refused in plan mode"),
            "{rendered}"
        );
        assert!(PlanDecision::HandOff.applies_to(PlanDocumentStatus::Accepted));
    }

    #[test]
    fn every_decision_pins_the_revision_and_digest_of_the_rendered_record() {
        let plan = record(PlanDocumentStatus::Proposed, 2);

        assert_eq!(
            plan_decision_command(&plan, PlanDecision::Accept, None),
            RuntimeCommand::AcceptPlanDocument {
                plan_id: PlanDocumentId("plan_1".to_string()),
                revision: 2,
                content_sha256: "abc123".to_string(),
            }
        );
        assert_eq!(
            plan_decision_command(&plan, PlanDecision::Reject, Some("too wide".to_string())),
            RuntimeCommand::RejectPlanDocument {
                plan_id: PlanDocumentId("plan_1".to_string()),
                revision: 2,
                content_sha256: "abc123".to_string(),
                reason: "too wide".to_string(),
            }
        );
        assert_eq!(
            plan_decision_command(&plan, PlanDecision::HandOff, None),
            RuntimeCommand::HandOffPlanDocument {
                plan_id: PlanDocumentId("plan_1".to_string()),
                revision: 2,
                content_sha256: "abc123".to_string(),
            }
        );
    }

    #[test]
    fn a_core_refusal_is_rendered_from_cores_own_reason() {
        let mut state = state_with(Some(record(PlanDocumentStatus::Proposed, 1)));
        let panel = state.ui.plan.as_mut().expect("panel");
        panel.begin("tui-7", PlanDecision::Accept);
        let settled = panel.observe_event(&RuntimeEvent {
            kind: RuntimeEventKind::CommandRejected {
                command_id: "tui-7".to_string(),
                reason: "plan_revision_stale: plan `plan_1` is at revision 2".to_string(),
            },
            sequence: 7,
            timestamp: Some(0),
        });

        assert!(settled);
        let rendered = plan_rows(&state, GlyphSet::new(true)).join("\n");
        assert!(rendered.contains("plan_revision_stale"), "{rendered}");
        assert!(
            state.ui.plan.as_ref().expect("panel").pending().is_none(),
            "a refused decision leaves nothing in flight"
        );
    }

    #[test]
    fn the_open_plan_wins_over_a_settled_one_for_the_same_owner() {
        let mut state = state_with(Some(record(PlanDocumentStatus::Rejected, 1)));
        let mut open = record(PlanDocumentStatus::Proposed, 2);
        open.plan_id = PlanDocumentId("plan_2".to_string());
        open.updated_at = 1;
        state.runtime.plan_documents.push(open);

        assert_eq!(
            focused_plan(&state).map(|plan| plan.plan_id.as_str()),
            Some("plan_2")
        );
    }
}
