//! Plan document producers (`runtime.plan_documents`, C13).
//!
//! Plan mode denies every mutating tool, so a planning turn produces prose and
//! nothing else. This module turns that prose into a Core-owned fact with four
//! properties a transcript cannot have, and each one is enforced here rather
//! than at a call site:
//!
//! 1. **One open plan per owner.** A proposal against an owner that already
//!    has an open plan revises it rather than opening a second one, because two
//!    open plans for one Lane give an operator two things to accept and Core no
//!    way to say which became the work.
//! 2. **A voided decision is published.** Revising a `Proposed` or `Accepted`
//!    plan republishes the earlier revision as `Superseded` *before* the new
//!    one. A client that only saw the new revision would keep rendering the old
//!    acceptance as live, and an acceptance Core no longer honours is worse
//!    than no acceptance at all.
//! 3. **A decision is pinned to what the operator read.** `revision` and
//!    `content_sha256` both have to match, or the command is refused with
//!    `plan_revision_stale`. Without the pin, a revision the model proposed
//!    while the operator was reading would be accepted by their click.
//! 4. **Handoff is the only mutation, and plan mode refuses it.** Accepting a
//!    document changes a document; spawning agent tasks changes the world. The
//!    refusal carries reason `plan_mode`, the same family every other
//!    plan-mode refusal uses, and it happens before the audit row, because
//!    nothing was authorized.
//!
//! Audit ordering matches `runtime.operator_git`: the record is appended
//! durably *before* the fact that announces the decision, and a failed append
//! aborts the decision. A plan handoff nobody can audit is worse than a handoff
//! that did not occur.

use sha2::{Digest, Sha256};
use viden_types::{
    AgentDagTaskSpec, AgentRole, AuditActor, AuditObjectRef, AuditOutcome, AuditRecord,
    MAX_PLAN_REJECT_REASON_BYTES, PermissionMode, PlanAuthor, PlanDocumentDraft, PlanDocumentId,
    PlanDocumentRecord, PlanDocumentStatus, PlanHandoff, PlanStep, RuntimeEvent, RuntimeEventKind,
    RuntimeOwner, fresh_id, now_timestamp, plan_content_digest_source,
};

use crate::SessionEngine;

/// Audit object kind for a plan document.
///
/// Named here rather than inlined so the runtime, the fixture, and the
/// documentation cannot drift into three spellings of the same kind.
pub(crate) const AUDIT_KIND_PLAN: &str = "plan";

/// Refusal reason for a decision that names a revision or digest Core no longer
/// holds. A stable key rather than prose, because a client renders a specific
/// "the plan changed while you were reading it — re-read and decide again"
/// recovery for it and must not have to match on a sentence.
pub(crate) const PLAN_REVISION_STALE: &str = "plan_revision_stale";

/// Refusal reason for a handoff attempted in plan mode. The same reason family
/// the starter-Lane preview uses.
pub(crate) const PLAN_MODE_REFUSAL: &str = "plan_mode";

/// Which settlement a pinned decision command carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanDecision {
    Accept,
    Reject,
}

impl PlanDecision {
    fn status(self) -> PlanDocumentStatus {
        match self {
            Self::Accept => PlanDocumentStatus::Accepted,
            Self::Reject => PlanDocumentStatus::Rejected,
        }
    }

    fn audit_action(self) -> &'static str {
        match self {
            Self::Accept => "plan.accepted",
            Self::Reject => "plan.rejected",
        }
    }
}

impl SessionEngine {
    /// Folds a completed `update_plan` call into the owner's plan.
    ///
    /// Runs after the registry executed the tool, so the permission gate, the
    /// cost interceptor, and the transcript entry all saw a normal tool call —
    /// nothing about this tool bypasses the path every other tool takes. The
    /// registry's own run validated and summarized the input; this reads the
    /// same input through the same parser and turns it into the proposal, so
    /// the summary the model got and the record the operator decides on cannot
    /// describe different plans.
    ///
    /// `Err` becomes a failed tool result the model sees and can act on. The
    /// one non-obvious refusal is the owner: a plan Core cannot attribute is a
    /// plan no owner can find, so an engine that published no workspace
    /// identity refuses to record one rather than filing it under nobody.
    pub(crate) fn fold_update_plan_call(
        &mut self,
        input: &viden_types::ToolInput,
    ) -> Result<Vec<RuntimeEvent>, String> {
        let call = viden_tools::parse_update_plan_input(input)?;
        let owner = self.active_native_turn_owner().ok_or_else(|| {
            "this Core published no workspace identity, so a plan has no owner to belong to; the \
             plan was not recorded (`runtime.workspace_owner`)"
                .to_string()
        })?;
        let turn_id = owner.turn_id.clone().unwrap_or_default();
        let draft = PlanDocumentDraft {
            owner,
            title: call.title,
            goal: call.goal,
            sections: call.sections,
            steps: call.steps,
        };
        self.propose_plan_document(
            draft,
            PlanAuthor::Assistant {
                session_id: self.session_id().to_string(),
                turn_id,
            },
        )
    }

    /// The owner of the native turn currently running, when Core can name it.
    ///
    /// `None` covers both "no turn is open" and "this engine published no
    /// workspace identity": the second is a real answer, not an empty owner,
    /// which is the fabricated-actor failure `runtime.workspace_owner` exists
    /// to end.
    fn active_native_turn_owner(&self) -> Option<RuntimeOwner> {
        let owner = self
            .active_native_turn
            .as_ref()
            .map(|turn| turn.owner.clone())?;
        (owner != RuntimeOwner::default() && !owner.workspace_id.is_empty()).then_some(owner)
    }

    pub(crate) fn upsert_plan_document(&mut self, record: PlanDocumentRecord) {
        if let Some(existing) = self
            .runtime_plan_documents
            .iter_mut()
            .find(|existing| existing.plan_id == record.plan_id)
        {
            *existing = record;
        } else {
            self.runtime_plan_documents.push(record);
        }
    }

    /// Creates or revises the owner's plan.
    ///
    /// `author` is supplied by the arrival path — `Operator` for the command,
    /// `Assistant` for the `update_plan` tool — and never read off the wire, so
    /// a client cannot file its own text as something the model wrote.
    pub(crate) fn propose_plan_document(
        &mut self,
        draft: PlanDocumentDraft,
        author: PlanAuthor,
    ) -> Result<Vec<RuntimeEvent>, String> {
        draft.validate()?;
        let steps = draft.into_steps();
        let content_sha256 =
            plan_content_sha256(&draft.title, &draft.goal, &draft.sections, &steps);
        let now = now_timestamp();
        let mut events = Vec::new();

        let open = self
            .runtime_plan_documents
            .iter()
            .find(|plan| plan.status.is_open() && plans_share_owner(&plan.owner, &draft.owner))
            .cloned();

        let (plan_id, revision, created_at) = match open {
            Some(previous) => {
                // The earlier revision is republished as `Superseded` first, so
                // the durable log says the decision on it was voided rather
                // than leaving a reader to infer it from a revision bump.
                let mut superseded = previous.clone();
                superseded.status = PlanDocumentStatus::Superseded;
                superseded.updated_at = now;
                events.push(RuntimeEvent::new(
                    1,
                    RuntimeEventKind::PlanDocumentUpdated { plan: superseded },
                ));
                (
                    previous.plan_id.clone(),
                    previous.revision.saturating_add(1),
                    previous.created_at,
                )
            }
            None => (PlanDocumentId(fresh_id(PlanDocumentId::PREFIX)), 1, now),
        };

        let record = PlanDocumentRecord {
            plan_id,
            owner: draft.owner.clone(),
            title: draft.title.clone(),
            goal: draft.goal.clone(),
            status: PlanDocumentStatus::Proposed,
            revision,
            content_sha256,
            sections: draft.sections.clone(),
            steps,
            proposed_by: author,
            created_at,
            updated_at: now,
            // A new revision voids the earlier decision, so the decision fields
            // are cleared rather than carried forward: an `Accepted` timestamp
            // on a `Proposed` revision would name an acceptance of bytes the
            // operator never saw.
            decided_at: None,
            decision_audit_id: None,
            handoff: None,
        };
        self.upsert_plan_document(record.clone());
        events.push(RuntimeEvent::new(
            next_plan_sequence(&events),
            RuntimeEventKind::PlanDocumentUpdated { plan: record },
        ));
        Ok(events)
    }

    /// Settles a proposed plan against the exact revision the operator read.
    pub(crate) fn decide_plan_document(
        &mut self,
        plan_id: &PlanDocumentId,
        revision: u32,
        content_sha256: &str,
        decision: PlanDecision,
        reason: Option<&str>,
    ) -> Result<Vec<RuntimeEvent>, String> {
        let plan = self.pinned_plan_document(plan_id, revision, content_sha256)?;
        if !matches!(
            plan.status,
            PlanDocumentStatus::Drafting | PlanDocumentStatus::Proposed
        ) {
            return Err(format!(
                "plan `{plan_id}` is `{}` and has already been decided\nhint: propose a new \
                 revision to replace a decided plan",
                plan.status.as_str()
            ));
        }

        let now = now_timestamp();
        let audit_id = fresh_id("audit");
        let mut args = std::collections::BTreeMap::from([
            ("revision".to_string(), revision.to_string()),
            ("content_sha256".to_string(), plan.content_sha256.clone()),
            ("steps".to_string(), plan.steps.len().to_string()),
        ]);
        if let Some(reason) = reason {
            args.insert(
                "reason".to_string(),
                bounded_plan_reason(reason).chars().take(96).collect(),
            );
        }
        // Audit before the announcing fact and fail-closed, exactly like an
        // operator git action: a decision nobody can audit is worse than a
        // decision that did not happen.
        self.append_trust_audit(&AuditRecord::sanitized(
            audit_id.clone(),
            now,
            plan.owner.clone(),
            AuditActor::Operator,
            decision.audit_action().to_string(),
            vec![AuditObjectRef::new(AUDIT_KIND_PLAN, plan_id.as_str())],
            AuditOutcome::Success,
            args,
        )?)?;

        let mut decided = plan;
        decided.status = decision.status();
        decided.updated_at = now;
        decided.decided_at = Some(now);
        decided.decision_audit_id = Some(audit_id.clone());
        self.upsert_plan_document(decided.clone());

        Ok(vec![
            RuntimeEvent::new(
                1,
                RuntimeEventKind::PlanDocumentUpdated {
                    plan: decided.clone(),
                },
            ),
            RuntimeEvent::new(
                2,
                RuntimeEventKind::PlanDocumentDecided {
                    plan_id: decided.plan_id.clone(),
                    revision: decided.revision,
                    status: decided.status,
                    audit_id,
                    owner: decided.owner,
                },
            ),
        ])
    }

    /// Turns an accepted plan into the agent DAG Core already owns.
    ///
    /// Reuses `start_agent_dag` rather than building a second DAG producer:
    /// the tasks, their merge gates, and their evidence requirements all come
    /// from the one path that already knows how to make them, so a plan's work
    /// and a hand-written DAG's work are the same work.
    pub(crate) fn hand_off_plan_document(
        &mut self,
        plan_id: &PlanDocumentId,
        revision: u32,
        content_sha256: &str,
    ) -> Result<Vec<RuntimeEvent>, String> {
        // Plan mode is checked before anything else, including the pin: this is
        // the only mutation in the capability and it must be refused before an
        // audit row claims something was authorized.
        if self.permissions.mode() == PermissionMode::Plan {
            return Err(format!(
                "{PLAN_MODE_REFUSAL}: handing a plan off starts agent work and is blocked while \
                 plan mode is active\nhint: accept the plan in Plan, then switch to Build and hand \
                 it off"
            ));
        }
        let plan = self.pinned_plan_document(plan_id, revision, content_sha256)?;
        if plan.status != PlanDocumentStatus::Accepted {
            return Err(format!(
                "plan `{plan_id}` is `{}`; only an accepted plan can be handed off",
                plan.status.as_str()
            ));
        }
        if plan.steps.is_empty() {
            return Err(format!(
                "plan `{plan_id}` has no steps to hand off\nhint: propose a revision with at least \
                 one step"
            ));
        }

        let now = now_timestamp();
        let audit_id = fresh_id("audit");
        self.append_trust_audit(&AuditRecord::sanitized(
            audit_id.clone(),
            now,
            plan.owner.clone(),
            AuditActor::Operator,
            "plan.handed_off".to_string(),
            vec![AuditObjectRef::new(AUDIT_KIND_PLAN, plan_id.as_str())],
            AuditOutcome::Success,
            std::collections::BTreeMap::from([
                ("revision".to_string(), revision.to_string()),
                ("content_sha256".to_string(), plan.content_sha256.clone()),
                ("tasks".to_string(), plan.steps.len().to_string()),
            ]),
        )?)?;

        let specs = plan_task_specs(&plan, self.runtime_snapshot.permission_mode);
        let dag_events = self.start_agent_dag(plan.title.clone(), specs)?;
        let dag_id = dag_events
            .iter()
            .find_map(|event| match &event.kind {
                RuntimeEventKind::AgentDagUpdated { dag } => Some(dag.dag_id.clone()),
                _ => None,
            })
            .ok_or_else(|| "the agent DAG path published no DAG to hand off to".to_string())?;

        let mut handed_off = plan;
        handed_off.status = PlanDocumentStatus::HandedOff;
        handed_off.updated_at = now;
        handed_off.handoff = Some(PlanHandoff {
            dag_id: dag_id.clone(),
            handed_off_at: now,
            audit_id: audit_id.clone(),
        });
        self.upsert_plan_document(handed_off.clone());

        // The DAG facts come first: a client must never learn a plan became
        // work before the work itself exists.
        let mut events = dag_events;
        events.push(RuntimeEvent::new(
            next_plan_sequence(&events),
            RuntimeEventKind::PlanDocumentUpdated {
                plan: handed_off.clone(),
            },
        ));
        events.push(RuntimeEvent::new(
            next_plan_sequence(&events),
            RuntimeEventKind::PlanDocumentHandedOff {
                plan_id: handed_off.plan_id.clone(),
                revision: handed_off.revision,
                dag_id,
                audit_id,
                owner: handed_off.owner,
            },
        ));
        Ok(events)
    }

    /// The plan a decision names, or a refusal that says why it is not it.
    fn pinned_plan_document(
        &self,
        plan_id: &PlanDocumentId,
        revision: u32,
        content_sha256: &str,
    ) -> Result<PlanDocumentRecord, String> {
        let plan = self
            .runtime_plan_documents
            .iter()
            .find(|plan| plan.plan_id == *plan_id)
            .cloned()
            .ok_or_else(|| format!("plan `{plan_id}` does not exist"))?;
        if plan.revision != revision || plan.content_sha256 != content_sha256 {
            return Err(format!(
                "{PLAN_REVISION_STALE}: plan `{plan_id}` is at revision {} and the decision named \
                 revision {revision}\nhint: re-read the plan from `plan_documents` and decide the \
                 revision it now carries",
                plan.revision
            ));
        }
        Ok(plan)
    }
}

/// Two owners address the same plan when they name the same Lane, or both name
/// the session rather than a Lane.
///
/// Deliberately not full owner equality: a turn id changes on every turn, so
/// equality would open a new plan per turn and "one open plan per owner" would
/// mean nothing.
fn plans_share_owner(left: &RuntimeOwner, right: &RuntimeOwner) -> bool {
    left.workspace_id == right.workspace_id
        && left.project_id == right.project_id
        && left.lane_id == right.lane_id
        && (left.lane_id.is_some() || left.session_id == right.session_id)
}

/// One task spec per step.
///
/// `role` defaults to `Coder` because a step that names no role is still work
/// somebody has to do, and the alternative — refusing the handoff — would make
/// the common Codex-shaped plan, which has no roles at all, un-handoffable.
fn plan_task_specs(plan: &PlanDocumentRecord, mode: PermissionMode) -> Vec<AgentDagTaskSpec> {
    plan.steps
        .iter()
        .map(|step| AgentDagTaskSpec {
            task_id: plan_task_id(&plan.plan_id, step),
            role: step.role.unwrap_or(AgentRole::Coder),
            title: step.title.clone(),
            objective: step.title.clone(),
            dependencies: step
                .dependencies
                .iter()
                .filter_map(|id| {
                    plan.steps
                        .iter()
                        .find(|sibling| sibling.id == *id)
                        .map(|sibling| plan_task_id(&plan.plan_id, sibling))
                })
                .collect(),
            workspace: None,
            file_scope: step.file_scope.clone(),
            context_bundle_id: None,
            required_evidence: Vec::new(),
            // The Lane's own mutation policy, named rather than invented: a
            // handoff must not widen what the session itself may do.
            permission_policy: mode.cli_name().to_string(),
        })
        .collect()
}

/// Task identity derived from the plan and the step, so a re-read of the plan
/// and a read of the DAG name the same work.
fn plan_task_id(plan_id: &PlanDocumentId, step: &PlanStep) -> String {
    format!("{}-{}", plan_id.as_str(), step.id)
}

pub(crate) fn plan_content_sha256(
    title: &str,
    goal: &str,
    sections: &[viden_types::PlanSection],
    steps: &[PlanStep],
) -> String {
    format!(
        "{:x}",
        Sha256::digest(plan_content_digest_source(title, goal, sections, steps).as_bytes())
    )
}

/// Bounds a rejection reason before it reaches the audit log.
pub(crate) fn bounded_plan_reason(reason: &str) -> String {
    let mut cut = reason.len().min(MAX_PLAN_REJECT_REASON_BYTES);
    while cut > 0 && !reason.is_char_boundary(cut) {
        cut -= 1;
    }
    reason[..cut].to_string()
}

fn next_plan_sequence(events: &[RuntimeEvent]) -> u64 {
    events.last().map(|event| event.sequence + 1).unwrap_or(1)
}
