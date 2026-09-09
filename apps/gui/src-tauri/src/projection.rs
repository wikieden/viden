use std::collections::BTreeSet;

use serde::Serialize;
use viden_core::{
    AgentConversationRole, AgentDagStatus, AgentLaneRecord, AgentRole, AgentRoute,
    AgentSessionStatus, AgentStartability, AgentTaskStatus, ApprovalDefaultAction,
    ApprovalRequestView, ApprovalRisk, ApprovalScope, AuditObjectRef, COCKPIT_CONTEXT_CAPABILITY,
    CheckRunStatus, ConflictBounceStatus, ContextScope, ContractDecision, ContractRecord,
    CostMeterability, CredentialHandle, DecisionContext, DependencyState, DiffDocument, DiffFile,
    DiffHunk, DiffLine, DiffLineKind, EventCursor, GateStrength, LaneStatus, LocaleId,
    MergeGateRecord, MergeGateStatus, MergeGateType, MutationPolicy, OperatorGitAction,
    OperatorGitFailureClass, OperatorGitOutcome, ProjectConfigPreview, ProjectProbe,
    ProviderHealthView, ReviewRequestRecord, ReviewRequestStatus, RuntimeOwner, RuntimeServiceKind,
    RuntimeServiceStatus, RuntimeSnapshotEnvelope, RuntimeViewState, SourceTarget, UiColorMode,
    UiDensity, UiMotion, UiSkin, WorkMode, WorkspaceChangeKind, WorkspaceDiffEntry,
    WorkspaceSourceStatus, WorkspaceSourceView,
};

use crate::d1::{
    D1_OWNER_CAPABILITY, D1_OWNER_CARDINALITY_CODE, D1AgentAdapterProjection,
    D1AgentConversationMessageProjection, D1AgentSessionProjection, D1ApprovalProjection,
    D1ChecklistItemProjection, D1CockpitProjection, D1ComposerProjection, D1ContentPartProjection,
    D1ContextDockProjection, D1ContextUsageProjection, D1CostUsageProjection, D1CursorProjection,
    D1EnvironmentProjection, D1EvidenceProjection, D1LaneAgentProjection, D1LaneProjection,
    D1LiveWorkProjection, D1ProviderHealthProjection, D1QueuedInputProjection,
    D1RuntimeServiceProjection, D1StarterLanePreviewProjection, D1StarterLaneReceiptProjection,
    D1StatusbarContextProjection, D1StatusbarLaneProjection, D1StatusbarLatencyProjection,
    D1StatusbarProjection, D1StatusbarRequestsProjection, D1StatusbarTokensProjection,
    D1TaskProjection, D1ToolProjection, D1TopbarSourceProjection, D1TranscriptRowProjection,
    D1WorkspaceEligibilityProjection, D1WorkspaceSourceProjection, unavailable_features,
};
use crate::d2::{
    D2_KIND_CONTRACT, D2_KIND_GATE, D2_KIND_REVIEW, D2ActionProjection, D2ContextProjection,
    D2DecisionsProjection, D2DetailProjection, D2EvidenceProjection, D2GroupProjection,
    D2QueueItemProjection, D2UnavailableProjection,
};
use crate::d10::{
    D10AgentProjection, D10EvidenceProjection, D10LaneMonitorProjection, D10LaneProjection,
};
use crate::d12::{
    D12ActionProjection, D12BounceProjection, D12CheckProjection, D12GateDetailProjection,
    D12GateProjection, D12IntegrationGateProjection, D12RevertProjection, d12_action_code,
};
use crate::d13::{
    D13BlockerProjection, D13FleetWorkflowProjection, D13HandoffProjection, D13NodeProjection,
    D13WorkflowProjection,
};
use crate::diff_review::{
    DecisionContextProjection, DiffDocumentProjection, DiffFileProjection, DiffHunkProjection,
    DiffLineProjection, WorkspaceDiffEntryProjection,
};
use crate::operator_git::OperatorGitResultProjection;
use crate::{
    D6ActionProjection, D6ConnectionState, D6RecoveryProjection, D6State,
    PermissionActionProjection, PermissionDockProjection, PermissionRequestProjection,
    PermissionTargetProjection,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceDiagnosticProjection {
    pub code: String,
    pub key: String,
    pub field: Option<String>,
    pub rejected_value: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedPreferencesProjection {
    pub locale: &'static str,
    pub skin: &'static str,
    pub mode: &'static str,
    pub density: &'static str,
    pub motion: &'static str,
    pub diagnostics: Vec<PreferenceDiagnosticProjection>,
}

/// Projects one Core-resolved preference set into the transport-safe GUI view.
///
/// Only Core-resolved values reach this function, so `System` has already
/// collapsed into the concrete locale/mode Core chose. The client renders that
/// resolution; it never re-runs the precedence rule itself.
pub fn resolved_preferences_projection(
    resolved: &viden_core::ResolvedUiPreferences,
) -> ResolvedPreferencesProjection {
    ResolvedPreferencesProjection {
        locale: match resolved.locale {
            LocaleId::System | LocaleId::En => "en",
            LocaleId::ZhCn => "zh-CN",
        },
        skin: match resolved.skin {
            UiSkin::Aurora => "aurora",
            UiSkin::Ice => "ice",
            UiSkin::Mono => "mono",
            UiSkin::Amber => "amber",
            UiSkin::Phosphor => "phosphor",
        },
        mode: match resolved.mode {
            UiColorMode::System | UiColorMode::Dark => "dark",
            UiColorMode::Light => "light",
        },
        density: match resolved.density {
            UiDensity::Compact => "compact",
            UiDensity::Regular => "regular",
            UiDensity::Comfy => "comfy",
        },
        motion: match resolved.motion {
            UiMotion::System => "system",
            UiMotion::Reduced => "reduced",
            UiMotion::Full => "full",
        },
        diagnostics: resolved
            .diagnostics
            .iter()
            .map(preference_diagnostic_projection)
            .collect(),
    }
}

pub fn preference_diagnostic_projection(
    diagnostic: &viden_core::UiPreferenceDiagnostic,
) -> PreferenceDiagnosticProjection {
    PreferenceDiagnosticProjection {
        code: diagnostic.code.clone(),
        key: diagnostic.key.clone(),
        field: diagnostic.field.clone(),
        rejected_value: diagnostic.rejected_value.clone(),
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D11ProjectProjection {
    pub root: String,
    pub is_git_repository: bool,
    pub config_state: &'static str,
    pub project_name: Option<String>,
    pub mode: Option<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D11ConfigProjection {
    pub preview_id: String,
    pub relative_path: String,
    pub content_sha256: String,
    pub exact_contents: Option<String>,
    pub valid: bool,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D11ConfirmedConfigProjection {
    pub preview_id: String,
    pub relative_path: String,
    pub content_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D11ProviderProjection {
    pub provider_id: String,
    pub model: String,
    pub status: String,
    pub warning: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D11CredentialProjection {
    pub provider_id: String,
    pub masked_handle: String,
    pub status: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D11StarterLaneProjection {
    pub id: String,
    pub role: String,
    pub status: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct D11ApprovalProjection {
    pub id: String,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D11AvailabilityProjection {
    pub available: bool,
    pub code: &'static str,
    pub message: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D11CapabilityProjection {
    pub project_onboarding: bool,
    pub credential_handles: bool,
    pub lane_lifecycle: bool,
    pub starter_lane_preview: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct D11IntakeProjection {
    pub project: Option<D11ProjectProjection>,
    pub preview: Option<D11ConfigProjection>,
    pub confirmed_config: Option<D11ConfirmedConfigProjection>,
    pub provider: Option<D11ProviderProjection>,
    pub credential_handles: Vec<D11CredentialProjection>,
    pub starter_lanes: Vec<D11StarterLaneProjection>,
    pub pending_approval: Option<D11ApprovalProjection>,
    pub last_error: Option<String>,
    pub recent_work: D11AvailabilityProjection,
    pub credential_ingress: D11AvailabilityProjection,
    pub capabilities: D11CapabilityProjection,
}

/// Last authoritative snapshot published by Core for presentation.
#[derive(Clone, Debug, Default)]
pub struct RuntimeProjection {
    confirmed: Option<RuntimeSnapshotEnvelope>,
}

impl RuntimeProjection {
    pub(crate) fn replace(&mut self, snapshot: RuntimeSnapshotEnvelope) {
        self.confirmed = Some(snapshot);
    }

    pub(crate) fn d11_approval_by_id(&self, request_id: &str) -> Option<D11ApprovalProjection> {
        self.confirmed.as_ref().and_then(|confirmed| {
            confirmed
                .view
                .pending_approvals
                .iter()
                .find(|approval| approval.id == request_id)
                .map(|approval| D11ApprovalProjection {
                    id: approval.id.clone(),
                    title: approval.title.clone(),
                })
        })
    }

    pub fn view(&self) -> Option<&RuntimeViewState> {
        self.confirmed.as_ref().map(|snapshot| &snapshot.view)
    }

    pub fn cursor(&self) -> Option<&EventCursor> {
        self.confirmed.as_ref().map(|snapshot| &snapshot.cursor)
    }

    pub fn permission_dock(&self) -> Option<PermissionDockProjection> {
        self.permission_dock_matching(|_| true)
    }

    pub(crate) fn permission_dock_for_owner(
        &self,
        owner: &viden_core::RuntimeOwner,
    ) -> Option<PermissionDockProjection> {
        self.permission_dock_matching(|approval| approval.owner == *owner)
    }

    fn empty_permission_dock(&self) -> Option<PermissionDockProjection> {
        self.permission_dock_matching(|_| false)
    }

    fn permission_dock_matching(
        &self,
        matches_owner: impl Fn(&viden_core::ApprovalRequestView) -> bool,
    ) -> Option<PermissionDockProjection> {
        let view = self.view()?;
        let request = view
            .pending_approvals
            .iter()
            .rev()
            .find(|approval| matches_owner(approval))
            .map(|approval| {
                let blocked_by_plan =
                    approval.is_mutating && view.snapshot.work_mode == WorkMode::Plan;
                let mut actions = approval
                    .allowed_scopes
                    .iter()
                    .map(|scope| match scope {
                        ApprovalScope::Once => PermissionActionProjection {
                            kind: "once".to_string(),
                            available: !blocked_by_plan,
                            session_id: None,
                            paths: Vec::new(),
                            code: None,
                        },
                        ApprovalScope::Session { session_id } => PermissionActionProjection {
                            kind: "session".to_string(),
                            available: !blocked_by_plan,
                            session_id: Some(session_id.clone()),
                            paths: Vec::new(),
                            code: None,
                        },
                        ApprovalScope::RepoAllowlist { paths } => PermissionActionProjection {
                            kind: "repo_allowlist".to_string(),
                            available: !blocked_by_plan,
                            session_id: None,
                            paths: paths.clone(),
                            code: None,
                        },
                    })
                    .collect::<Vec<_>>();
                // The design names Always/Edit, but schema 1 has no corresponding
                // decision variants. Keep them visible and fail closed.
                actions.extend([
                    PermissionActionProjection {
                        kind: "always".to_string(),
                        available: false,
                        session_id: None,
                        paths: Vec::new(),
                        code: Some("GUI-CORE-003"),
                    },
                    PermissionActionProjection {
                        kind: "edit".to_string(),
                        available: false,
                        session_id: None,
                        paths: Vec::new(),
                        code: Some("GUI-CORE-003"),
                    },
                    PermissionActionProjection {
                        kind: "deny".to_string(),
                        available: !blocked_by_plan,
                        session_id: None,
                        paths: Vec::new(),
                        code: None,
                    },
                ]);
                PermissionRequestProjection {
                    id: approval.id.clone(),
                    tool_name: approval.tool_name.clone(),
                    title: approval.title.clone(),
                    message: approval.message.clone(),
                    input_preview: approval.input_preview.clone(),
                    is_mutating: approval.is_mutating,
                    reason: approval.reason.clone(),
                    risk: match approval.risk {
                        ApprovalRisk::Low => "low",
                        ApprovalRisk::Medium => "medium",
                        ApprovalRisk::High => "high",
                        ApprovalRisk::Critical => "critical",
                    }
                    .to_string(),
                    target: PermissionTargetProjection {
                        kind: approval.target.kind.clone(),
                        display: approval.target.display.clone(),
                        canonical_ref: approval.target.canonical_ref.clone(),
                    },
                    policy_reason_key: approval.policy_reason_key.clone(),
                    policy_reason_args: approval.policy_reason_args.clone(),
                    expires_at: approval.expires_at,
                    default_action: match approval.default_action {
                        ApprovalDefaultAction::Deny => "deny",
                    },
                    audit_id: approval.audit_id.clone(),
                    blocked_by_plan,
                    decision_context: decision_context_projection(
                        approval.decision_context.as_ref(),
                    ),
                    actions,
                }
            });
        Some(PermissionDockProjection {
            work_mode: view.snapshot.work_mode.cli_name().to_string(),
            permission_level: view.snapshot.permission_level.cli_name().to_string(),
            request,
        })
    }

    /// Projects the D2 decision queue.
    ///
    /// `selected` is presentation state supplied by the shell. When it does not
    /// name a live decision the projection falls back to the first pending
    /// item, so the card never renders a decision Core no longer holds.
    pub fn d2_decisions(&self, selected: Option<&str>) -> Option<D2DecisionsProjection> {
        let view = self.view()?;
        let blocked_by_plan = view.snapshot.work_mode == WorkMode::Plan;

        let gate_items: Vec<D2QueueItemProjection> =
            view.pending_approvals.iter().map(gate_queue_item).collect();
        let contract_items: Vec<D2QueueItemProjection> =
            view.contracts.iter().map(contract_queue_item).collect();
        let review_items: Vec<D2QueueItemProjection> =
            view.review_requests.iter().map(review_queue_item).collect();

        // Only approvals and pending reviews are actually awaiting a human.
        let pending_total = gate_items.len()
            + review_items
                .iter()
                .filter(|item| item.status == "pending")
                .count();

        let groups = vec![
            D2GroupProjection {
                kind: D2_KIND_GATE.to_string(),
                items: gate_items,
                unavailable: None,
            },
            D2GroupProjection {
                kind: D2_KIND_REVIEW.to_string(),
                items: review_items,
                unavailable: None,
            },
            D2GroupProjection {
                kind: D2_KIND_CONTRACT.to_string(),
                items: contract_items,
                unavailable: Some(D2UnavailableProjection {
                    key: "d2.contract.noPendingFact",
                    code: "GUI-CORE-013",
                }),
            },
        ];

        let selected_id = selected
            .filter(|id| {
                groups
                    .iter()
                    .any(|group| group.items.iter().any(|item| item.id == *id))
            })
            .map(str::to_string)
            .or_else(|| {
                groups
                    .iter()
                    .flat_map(|group| group.items.iter())
                    .next()
                    .map(|item| item.id.clone())
            });

        let detail = selected_id
            .as_deref()
            .and_then(|id| self.d2_detail(view, id, blocked_by_plan));

        Some(D2DecisionsProjection {
            work_mode: view.snapshot.work_mode.cli_name().to_string(),
            permission_level: view.snapshot.permission_level.cli_name().to_string(),
            pending_total,
            selected_id,
            groups,
            detail,
        })
    }

    fn d2_detail(
        &self,
        view: &RuntimeViewState,
        id: &str,
        blocked_by_plan: bool,
    ) -> Option<D2DetailProjection> {
        if let Some(approval) = view.pending_approvals.iter().find(|entry| entry.id == id) {
            let mut actions: Vec<D2ActionProjection> = approval
                .allowed_scopes
                .iter()
                .map(|scope| match scope {
                    ApprovalScope::Once => D2ActionProjection {
                        kind: "once".to_string(),
                        available: !blocked_by_plan,
                        session_id: None,
                        paths: Vec::new(),
                        code: None,
                    },
                    ApprovalScope::Session { session_id } => D2ActionProjection {
                        kind: "session".to_string(),
                        available: !blocked_by_plan,
                        session_id: Some(session_id.clone()),
                        paths: Vec::new(),
                        code: None,
                    },
                    ApprovalScope::RepoAllowlist { paths } => D2ActionProjection {
                        kind: "repo_allowlist".to_string(),
                        available: !blocked_by_plan,
                        session_id: None,
                        paths: paths.clone(),
                        code: None,
                    },
                })
                .collect();
            actions.push(D2ActionProjection {
                kind: "deny".to_string(),
                available: !blocked_by_plan,
                session_id: None,
                paths: Vec::new(),
                code: None,
            });

            let decision_context = decision_context_projection(approval.decision_context.as_ref());
            return Some(D2DetailProjection {
                id: approval.id.clone(),
                kind: D2_KIND_GATE.to_string(),
                title: approval.title.clone(),
                project_id: approval.owner.project_id.clone(),
                lane_id: approval.owner.lane_id.clone(),
                task_id: approval.owner.task_id.clone(),
                audit_id: approval.audit_id.clone(),
                // The runtime emits no audit object for a tool approval
                // (`AuditObjectRef::KIND_PERMISSION` has no emission site), so
                // there is nothing to scope a trail by and none is offered.
                audit_scope: None,
                policy_reason_key: Some(approval.policy_reason_key.clone()),
                blocked_by_plan,
                context: D2ContextProjection {
                    source: "approval_input_preview",
                    text: approval.input_preview.clone(),
                    // The marker is a claim about Core, so it stands only
                    // while Core really published no rows for this approval.
                    // Core may legitimately produce none — `shell` and the
                    // `git_*` family never carry a context — and the preview
                    // stays the whole context there.
                    unavailable: decision_context
                        .is_none()
                        .then_some(D2UnavailableProjection {
                            key: "d2.context.noStructuredDiff",
                            code: "GUI-CORE-012",
                        }),
                },
                decision_context,
                evidence: owner_evidence(view, approval.owner.lane_id.as_deref()),
                actions,
            });
        }

        if let Some(review) = view
            .review_requests
            .iter()
            .find(|entry| entry.review_id == id)
        {
            // Fail-closed order matters: a settled review can never be
            // re-decided whatever the actor situation is, so that check comes
            // first and reports the honest reason.
            let code = if review.status != ReviewRequestStatus::Pending {
                Some(crate::D2_REVIEW_SETTLED_CODE)
            } else if d2_review_actor(view, review).is_none() {
                Some(crate::D2_REVIEW_NO_ACTOR_CODE)
            } else {
                None
            };
            // Plan mode is reported by `blocked_by_plan` and the shared plan
            // banner, exactly as it is for gate and contract decisions, so it
            // never masks a review-specific reason with a code of its own.
            let available = code.is_none() && !blocked_by_plan;
            return Some(D2DetailProjection {
                id: review.review_id.clone(),
                kind: D2_KIND_REVIEW.to_string(),
                title: review.review_id.clone(),
                project_id: review.owner.project_id.clone(),
                lane_id: review.owner.lane_id.clone(),
                task_id: Some(review.task_id.clone()),
                audit_id: review.audit_id.clone(),
                // Request, verdict, gate decision, and revert all link the
                // gate (crates/runtime/src/trust_loop.rs), so scoping by the
                // parent gate opens the decision chain rather than one record.
                audit_scope: Some(audit_scope(
                    AuditObjectRef::KIND_MERGE_GATE,
                    &review.gate_id,
                )),
                policy_reason_key: None,
                blocked_by_plan,
                decision_context: None,
                context: D2ContextProjection {
                    source: "review_request",
                    text: review.gate_id.clone(),
                    unavailable: None,
                },
                evidence: evidence_by_ids(view, &review.evidence_ids),
                // GUI-CORE-011 is closed: `RuntimeCommand::DecideReview` carries
                // the verdict, so the actions are live whenever Core would
                // accept one and carry their own local reason when it would not.
                actions: vec![
                    D2ActionProjection {
                        kind: "accept_review".to_string(),
                        available,
                        session_id: None,
                        paths: Vec::new(),
                        code,
                    },
                    D2ActionProjection {
                        kind: "reject_review".to_string(),
                        available,
                        session_id: None,
                        paths: Vec::new(),
                        code,
                    },
                ],
            });
        }

        let contract = view
            .contracts
            .iter()
            .find(|entry| entry.contract_id == id)?;
        Some(D2DetailProjection {
            id: contract.contract_id.clone(),
            kind: D2_KIND_CONTRACT.to_string(),
            title: contract.summary.clone(),
            project_id: contract.owner.project_id.clone(),
            lane_id: contract.owner.lane_id.clone(),
            task_id: Some(contract.task_id.clone()),
            audit_id: contract.audit_id.clone(),
            // `contract.confirmed` links `contract:<contract_id>`; a contract
            // record carries no gate, so its own object is the whole trail.
            audit_scope: Some(audit_scope(
                AuditObjectRef::KIND_CONTRACT,
                &contract.contract_id,
            )),
            policy_reason_key: None,
            blocked_by_plan,
            decision_context: None,
            context: D2ContextProjection {
                source: "contract_summary",
                text: contract.summary.clone(),
                unavailable: None,
            },
            evidence: owner_evidence(view, contract.owner.lane_id.as_deref()),
            // Every `ContractRecord` schema 1 publishes is already decided —
            // `ContractDecision` has only `Confirmed` and `Rejected` — and
            // Core rejects a second `ConfirmContract` for an id it already
            // recorded. A live verdict here could only ever produce a refusal,
            // so both stay visible and disabled under the request that would
            // make a pending contract exist (GUI-CORE-013). They become
            // conditional on the record's own state, not unconditionally dead,
            // the moment Core publishes that fact.
            actions: vec![
                D2ActionProjection {
                    kind: "confirm_contract".to_string(),
                    available: false,
                    session_id: None,
                    paths: Vec::new(),
                    code: Some(crate::D2_CONTRACT_DECIDED_CODE),
                },
                D2ActionProjection {
                    kind: "reject_contract".to_string(),
                    available: false,
                    session_id: None,
                    paths: Vec::new(),
                    code: Some(crate::D2_CONTRACT_DECIDED_CODE),
                },
            ],
        })
    }

    /// Projects the D13 fleet view: every Core workflow DAG and the handoffs
    /// Core recorded between Lanes.
    pub fn d13_fleet_workflow(&self) -> Option<D13FleetWorkflowProjection> {
        let view = self.view()?;
        let workflows = view
            .agent_dags
            .iter()
            .map(|dag| D13WorkflowProjection {
                dag_id: dag.dag_id.clone(),
                goal: dag.goal.clone(),
                status: agent_dag_status(dag.status).to_string(),
                created_at: dag.created_at,
                updated_at: dag.updated_at,
                nodes: dag
                    .tasks
                    .iter()
                    .map(|spec| {
                        let live = view.tasks.iter().find(|task| task.id == spec.task_id);
                        let blockers: Vec<D13BlockerProjection> = view
                            .dependencies
                            .iter()
                            .filter(|dependency| {
                                dependency.task_id == spec.task_id
                                    && dependency.state == DependencyState::Blocked
                            })
                            .map(|dependency| D13BlockerProjection {
                                dependency_id: dependency.dependency_id.clone(),
                                depends_on_task_id: dependency.depends_on_task_id.clone(),
                                reason: dependency.reason.clone(),
                                audit_id: dependency.audit_id.clone(),
                                updated_at: dependency.updated_at,
                            })
                            .collect();
                        D13NodeProjection {
                            task_id: spec.task_id.clone(),
                            title: spec.title.clone(),
                            objective: spec.objective.clone(),
                            role: agent_role(spec.role).to_string(),
                            depends_on: spec.dependencies.clone(),
                            required_evidence: spec.required_evidence.clone(),
                            permission_policy: spec.permission_policy.clone(),
                            // A planned spec with no Core task is not running.
                            status: live.map(|task| agent_task_status(task.status).to_string()),
                            progress: live.map(|task| task.progress),
                            blocked: !blockers.is_empty(),
                            blockers,
                        }
                    })
                    .collect(),
            })
            .collect();

        Some(D13FleetWorkflowProjection {
            workflows,
            handoffs: view
                .handoffs
                .iter()
                .map(|handoff| D13HandoffProjection {
                    handoff_id: handoff.handoff_id.clone(),
                    task_id: handoff.task_id.clone(),
                    from_lane_id: handoff.from_lane_id.clone(),
                    to_lane_id: handoff.to_lane_id.clone(),
                    summary: handoff.summary.clone(),
                    audit_id: handoff.audit_id.clone(),
                })
                .collect(),
        })
    }

    /// Projects the D12 integration gate, its bounce timeline, and any
    /// post-merge revert. `selected` is presentation state from the shell.
    ///
    /// The two actions are derived from the rules
    /// `RuntimeContract::decide_merge_gate` actually enforces, and they are
    /// derived fail-closed: every condition below is *necessary* for Core to
    /// accept the command, never sufficient. Core keeps facts the frontend
    /// contract does not carry (canonical context items, permission
    /// snapshots, evidence quality), so a command this projection allows may
    /// still be rejected — the rejection reason is rendered verbatim rather
    /// than pre-empted by a GUI-private gate model.
    pub fn d12_integration_gate(
        &self,
        selected: Option<&str>,
    ) -> Option<D12IntegrationGateProjection> {
        let view = self.view()?;
        let mut gates: Vec<D12GateProjection> = view
            .merge_gates
            .iter()
            .map(|gate| D12GateProjection {
                gate_id: gate.gate_id.clone(),
                task_id: gate.task_id.clone(),
                status: merge_gate_status(gate.status).to_string(),
                gate_type: merge_gate_type(gate.gate_type).to_string(),
                project_id: gate.owner.project_id.clone(),
                lane_id: gate.owner.lane_id.clone(),
                requires_independent_validator: gate.policy_snapshot.requires_independent_validator,
                has_validator: gate.validator.is_some(),
                required_evidence: gate.required_evidence.clone(),
                evidence_ids: gate.evidence_ids.clone(),
                dormant: is_dormant_gate(view, gate),
            })
            .collect();
        // Active gates first, dormant ones grouped after them. The sort is
        // stable, so Core's own order survives inside each group: this reorders
        // nothing an operator could act on relative to anything else.
        gates.sort_by_key(|gate| gate.dormant);

        let selected_gate_id = selected
            .filter(|id| gates.iter().any(|gate| gate.gate_id == *id))
            .map(str::to_string)
            // With nothing chosen the detail pane opens on work the operator
            // can still act on. A dormant gate is the fallback, never the
            // default: opening D12 onto a gate whose session finished weeks ago
            // is exactly the burial this grouping exists to undo.
            .or_else(|| {
                gates
                    .iter()
                    .find(|gate| !gate.dormant)
                    .or_else(|| gates.first())
                    .map(|gate| gate.gate_id.clone())
            });

        let detail = selected_gate_id.as_deref().and_then(|gate_id| {
            let gate = gates.iter().find(|gate| gate.gate_id == gate_id)?.clone();
            let record = view
                .merge_gates
                .iter()
                .find(|record| record.gate_id == gate_id)?;
            // A strong gate cannot be bypassed: `accept` opens only when Core
            // has recorded every evidence id the gate policy requires.
            let missing_evidence: Vec<String> = gate
                .required_evidence
                .iter()
                .filter(|required| !gate.evidence_ids.contains(required))
                .cloned()
                .collect();
            let bounces = view
                .conflict_bounces
                .iter()
                .filter(|bounce| bounce.gate_id == gate_id)
                .map(|bounce| D12BounceProjection {
                    bounce_id: bounce.bounce_id.clone(),
                    original_lane_id: bounce.original_lane_id.clone(),
                    task_id: bounce.task_id.clone(),
                    reason: bounce.reason.clone(),
                    status: conflict_bounce_status(bounce.status).to_string(),
                    evidence_ids: bounce.evidence_ids.clone(),
                })
                .collect();
            let reverts = view
                .reverts
                .iter()
                .filter(|revert| revert.gate_id == gate_id)
                .map(|revert| D12RevertProjection {
                    revert_id: revert.revert_id.clone(),
                    applied_change_id: revert.applied_change_id.clone(),
                    reason: revert.reason.clone(),
                    restored_paths: revert.restored_paths.clone(),
                    audit_id: revert.audit_id.clone(),
                    // `change.reverted` links `revert:<revert_id>`
                    // (crates/runtime/src/trust_loop.rs), which is the object
                    // `AuditQuery` can actually filter on.
                    audit_scope: Some(audit_scope(AuditObjectRef::KIND_REVERT, &revert.revert_id)),
                    reverted_at: revert.reverted_at,
                })
                .collect();
            let checks = view
                .check_runs
                .iter()
                .filter(|check| check.owner.task_id.as_deref() == Some(gate.task_id.as_str()))
                .map(|check| D12CheckProjection {
                    id: check.id.clone(),
                    name: check.label.clone(),
                    status: check_run_status(check.status).to_string(),
                })
                .collect();
            let terminal = matches!(gate.status.as_str(), "merged" | "reverted" | "accepted");
            let accept_block = d12_accept_block(view, record, terminal, &missing_evidence);
            let reject_block = d12_reject_block(record, terminal);
            Some(D12GateDetailProjection {
                actions: vec![
                    D12ActionProjection {
                        kind: "accept".to_string(),
                        available: accept_block.is_none(),
                        code: accept_block,
                    },
                    D12ActionProjection {
                        kind: "reject".to_string(),
                        available: reject_block.is_none(),
                        code: reject_block,
                    },
                ],
                gate,
                missing_evidence,
                bounces,
                reverts,
                checks,
            })
        });

        Some(D12IntegrationGateProjection {
            gates,
            selected_gate_id,
            detail,
            // The design renders the conflicting hunk side by side. Schema 1
            // carries no structured conflict content.
            unavailable: vec![D2UnavailableProjection {
                key: "d12.conflict.noStructuredHunk",
                code: "GUI-CORE-015",
            }],
        })
    }

    /// Projects the D10 lane monitor across every Core Lane and project.
    pub fn d10_lane_monitor(&self) -> Option<D10LaneMonitorProjection> {
        let view = self.view()?;
        let lanes: Vec<D10LaneProjection> = view
            .lanes
            .iter()
            .map(|lane| {
                // Project binding comes only from the Core owner binding. A
                // lane Core has not bound reports no project at all.
                let project_id = view
                    .lane_runtime_owners
                    .iter()
                    .find(|binding| binding.lane_id == lane.id)
                    .map(|binding| binding.owner.project_id.clone());
                let progress = lane.task_id.as_ref().and_then(|task_id| {
                    view.tasks
                        .iter()
                        .find(|task| &task.id == task_id)
                        .map(|task| task.progress)
                });
                let agents = lane
                    .active_session_ids
                    .iter()
                    .filter_map(|session_id| {
                        view.agent_sessions
                            .iter()
                            .find(|session| &session.session_id == session_id)
                    })
                    .map(|session| D10AgentProjection {
                        session_id: session.session_id.clone(),
                        agent_id: session.agent_id.clone(),
                        model: session.model.clone(),
                        status: agent_session_status(session.status).to_string(),
                    })
                    .collect();
                let evidence = lane
                    .evidence
                    .iter()
                    .filter_map(|id| view.latest_evidence.iter().find(|entry| &entry.id == id))
                    .map(|entry| D10EvidenceProjection {
                        id: entry.id.clone(),
                        kind: entry.kind.clone(),
                        summary: entry.summary.clone(),
                    })
                    .collect();
                D10LaneProjection {
                    id: lane.id.clone(),
                    project_id,
                    summary: lane.summary.clone(),
                    role: agent_role(lane.role).to_string(),
                    route: agent_route(lane.route).to_string(),
                    gate_strength: gate_strength(lane.gate_strength).to_string(),
                    mutation_policy: mutation_policy(lane.mutation_policy).to_string(),
                    status: lane_status(lane.status).to_string(),
                    awaits_human: lane_awaits_human(lane.status),
                    branch: lane.branch.clone(),
                    worktree: lane.worktree.clone(),
                    progress,
                    agents,
                    evidence,
                    token_limit: lane.budget.token_limit,
                    cost_limit_micro_usd: lane.budget.cost_limit_micro_usd,
                    cost_meterability: cost_meterability(lane.route).to_string(),
                    run_stats: lane_run_stats(lane),
                }
            })
            .collect();

        let mut projects: Vec<&String> = lanes
            .iter()
            .filter_map(|lane| lane.project_id.as_ref())
            .collect();
        projects.sort();
        projects.dedup();

        Some(D10LaneMonitorProjection {
            total_lanes: lanes.len(),
            total_projects: projects.len(),
            awaiting_total: lanes.iter().filter(|lane| lane.awaits_human).count(),
            lanes,
            // The design's scribe-compiled event ticker is read from the
            // append-only audit timeline (`QueryAudit` -> `AuditPageLoaded`),
            // not from view state, so it is not projected here and is no
            // longer declared unavailable (GUI-CORE-014). Nothing else in D10
            // is missing a Core fact.
            unavailable: Vec::new(),
        })
    }

    pub fn d6_recovery(
        &self,
        connection: D6ConnectionState,
        connection_detail: Option<&str>,
        supports_approvals: bool,
    ) -> Option<D6RecoveryProjection> {
        let view = self.view()?;
        let latest_error = view.errors.last();
        let exceeded = view
            .context_budgets
            .iter()
            .rev()
            .find(|budget| budget.exceeded);
        let agent_stopped = view.tasks.iter().any(|task| {
            matches!(
                task.status,
                AgentTaskStatus::Failed | AgentTaskStatus::Cancelled | AgentTaskStatus::Discarded
            )
        }) || view.lanes.iter().any(|lane| {
            matches!(
                lane.status,
                LaneStatus::Failed | LaneStatus::Cancelled | LaneStatus::Detached
            )
        });
        let provider_error = view
            .provider
            .as_ref()
            .is_some_and(|provider| provider.error_count > 0);
        // `GateQueueClear` says the operator has no decision left to make.
        // A gate abandoned by a finished session is not such a decision, so
        // letting it hold the queue open leaves the cockpit permanently
        // claiming work that no longer exists.
        let open_merge_gate = has_actionable_merge_gate(view);
        let missing_capabilities = (!supports_approvals)
            .then(|| "runtime.approvals".to_string())
            .into_iter()
            .collect::<Vec<_>>();

        let state = match connection {
            D6ConnectionState::Connecting => D6State::Connecting,
            D6ConnectionState::Disconnected => D6State::Disconnected,
            D6ConnectionState::Recovering => D6State::EventGap,
            D6ConnectionState::Incompatible => D6State::IncompatibleSchema,
            D6ConnectionState::Live if !missing_capabilities.is_empty() => {
                D6State::MissingFeatureCapability
            }
            D6ConnectionState::Live if view.lanes.is_empty() => D6State::Empty,
            D6ConnectionState::Live if exceeded.is_some() => D6State::ContextOverflow,
            D6ConnectionState::Live if agent_stopped => D6State::AgentStopped,
            D6ConnectionState::Live if provider_error => D6State::ProviderError,
            D6ConnectionState::Live if view.pending_approvals.is_empty() && !open_merge_gate => {
                D6State::GateQueueClear
            }
            D6ConnectionState::Live => D6State::Live,
        };
        let blocked = matches!(
            state,
            D6State::Connecting
                | D6State::Disconnected
                | D6State::ProviderError
                | D6State::AgentStopped
                | D6State::ContextOverflow
                | D6State::IncompatibleSchema
                | D6State::MissingFeatureCapability
                | D6State::EventGap
        );
        let detail = connection_detail
            .map(str::to_string)
            .or_else(|| latest_error.map(|error| error.message.clone()));
        let hint = latest_error.and_then(|error| error.hint.clone());
        let recoverable = latest_error.is_some_and(|error| error.recoverable)
            || matches!(state, D6State::Disconnected | D6State::EventGap);
        let reconnect_available = matches!(state, D6State::Disconnected | D6State::EventGap);
        // `restart` mirrors D1's retry rule: a Lane-bound ACP session Core
        // reports as failed or cancelled is the only `RetryAgentSession`
        // target. D6 carries no Lane selection, so more than one candidate is
        // an ambiguous target and fails closed rather than picking one for the
        // operator.
        let mut retryable = view.agent_sessions.iter().filter(|session| {
            session.owner.lane_id.as_deref() == Some(session.lane_id.as_str())
                && matches!(
                    session.status,
                    AgentSessionStatus::Failed | AgentSessionStatus::Cancelled
                )
        });
        let restart_target = match (retryable.next(), retryable.next()) {
            (Some(session), None) => Some(session),
            _ => None,
        };
        // `close_lane` targets the one active Lane Core published, under the
        // same cardinality rule.
        let mut closable = view.lanes.iter().filter(|lane| lane.is_active());
        let close_lane_target = match (closable.next(), closable.next()) {
            (Some(lane), None) => Some(lane),
            _ => None,
        };
        Some(D6RecoveryProjection {
            connection,
            state,
            detail,
            hint,
            recoverable,
            business_success_blocked: blocked,
            used_tokens: exceeded.map(|budget| budget.used_tokens),
            hard_token_limit: exceeded.map(|budget| budget.hard_token_limit),
            missing_capabilities,
            actions: vec![
                D6ActionProjection::untargeted(
                    "reconnect",
                    reconnect_available,
                    if reconnect_available {
                        "core_client"
                    } else {
                        "GUI-CORE-003"
                    },
                ),
                // `inspect` expands the facts already in this projection. It
                // is a presentation affordance and never reaches Core.
                D6ActionProjection::untargeted("inspect", true, "presentation_only"),
                D6ActionProjection {
                    kind: "restart",
                    available: restart_target.is_some(),
                    code: if restart_target.is_some() {
                        "core_command"
                    } else {
                        "no_retryable_session"
                    },
                    session_id: restart_target.map(|session| session.session_id.clone()),
                    lane_id: restart_target.map(|session| session.lane_id.clone()),
                },
                D6ActionProjection {
                    kind: "close_lane",
                    available: close_lane_target.is_some(),
                    code: if close_lane_target.is_some() {
                        "core_command"
                    } else {
                        "no_exact_lane"
                    },
                    session_id: None,
                    lane_id: close_lane_target.map(|lane| lane.id.clone()),
                },
                // Recorded Core contract request GUI-CORE-018: schema 1 models
                // no checkpoint at all — no `RuntimeCommand` creates or
                // restores one and no checkpoint record exists in the view —
                // so this action stays fail-closed rather than claiming a
                // restore the runtime cannot perform.
                D6ActionProjection::untargeted("checkpoint", false, "GUI-CORE-003"),
            ],
        })
    }

    /// Projects only Core-resolved preferences into a transport-safe GUI view.
    pub fn preferences(&self) -> Option<ResolvedPreferencesProjection> {
        Some(resolved_preferences_projection(
            &self.view()?.snapshot.ui_preferences,
        ))
    }

    /// Selects D11 facts without creating a second onboarding reducer in the GUI.
    pub fn d11_intake(&self) -> Option<D11IntakeProjection> {
        let confirmed = self.confirmed.as_ref()?;
        let view = &confirmed.view;
        let supports = |capability: &str| {
            confirmed
                .capabilities
                .iter()
                .any(|candidate| candidate.0 == capability)
        };
        Some(D11IntakeProjection {
            project: view.project_probe.as_ref().map(project_projection),
            preview: view.project_config_preview.as_ref().map(config_projection),
            confirmed_config: view
                .confirmed_project_config
                .as_ref()
                .map(confirmed_config_projection),
            provider: view.provider.as_ref().map(provider_projection),
            credential_handles: view
                .credential_handles
                .iter()
                .map(credential_projection)
                .collect(),
            starter_lanes: view.lanes.iter().map(starter_lane_projection).collect(),
            pending_approval: view
                .pending_approvals
                .iter()
                .rev()
                .find(|approval| {
                    approval.tool_name.contains("project_config_confirm")
                        || approval.tool_name.contains("credential_handle_store")
                        || approval.tool_name == "lane_create"
                })
                .map(|approval| D11ApprovalProjection {
                    id: approval.id.clone(),
                    title: approval.title.clone(),
                }),
            last_error: view.errors.last().map(|error| error.message.clone()),
            // Availability mirrors Core's handshake: the rows themselves come
            // from the shared `QueryRecentWork` read, not from this snapshot
            // projection, so the flag only says whether that read exists.
            recent_work: if supports(crate::RECENT_WORK_CAPABILITY) {
                D11AvailabilityProjection {
                    available: true,
                    code: "core_command",
                    message: "Recent project and session history is served by Core.",
                }
            } else {
                D11AvailabilityProjection {
                    available: false,
                    code: "capability_missing",
                    message: "Core did not publish runtime.recent_work; recent history is unavailable.",
                }
            },
            // Core carries `StoreCredentialHandle`, but it takes a
            // `CredentialRequestId` that only a trusted staging path can mint,
            // and schema 1 publishes no such path. The GUI must not take the
            // raw secret itself, so the row stays unavailable under the
            // register entry that asks for it.
            credential_ingress: D11AvailabilityProjection {
                available: false,
                code: "GUI-CORE-026",
                message: "Platform credential intake is unavailable.",
            },
            capabilities: D11CapabilityProjection {
                project_onboarding: supports("runtime.project_onboarding"),
                credential_handles: supports("runtime.credential_handles"),
                lane_lifecycle: supports("runtime.lane_lifecycle"),
                starter_lane_preview: supports("runtime.starter_lane_preview"),
            },
        })
    }

    /// Builds the canonical D1 cockpit from Core's reduced view and nothing else.
    pub fn d1_cockpit(&self, requested_lane_id: Option<&str>) -> Option<D1CockpitProjection> {
        let confirmed = self.confirmed.as_ref()?;
        let view = &confirmed.view;
        // Preserve an explicit selection even when its Lane disappears: choosing another
        // Lane would silently retarget a subsequent mutation.
        let selected_lane_id = requested_lane_id
            .map(str::to_string)
            .or_else(|| view.lanes.first().map(|lane| lane.id.clone()));
        let selected_lane = selected_lane_id
            .as_deref()
            .and_then(|lane_id| view.lanes.iter().find(|lane| lane.id == lane_id));
        let exact_owner_count = selected_lane
            .and_then(|_| selected_lane_id.as_deref())
            .map_or(0, |lane_id| exact_owner_count(view, lane_id));
        let exact_binding = selected_lane
            .and_then(|_| selected_lane_id.as_deref())
            .and_then(|lane_id| exact_owner_binding(view, lane_id));
        let restored_agent_session = exact_binding
            .is_none()
            .then(|| {
                selected_lane
                    .and_then(|_| selected_lane_id.as_deref())
                    .and_then(|lane_id| exact_terminal_agent_session(view, lane_id))
            })
            .flatten();
        let selected_owner = exact_binding
            .map(|binding| &binding.owner)
            .or_else(|| restored_agent_session.map(|session| &session.owner));
        let supports_owner = confirmed
            .capabilities
            .iter()
            .any(|capability| capability.0 == D1_OWNER_CAPABILITY);
        let supports_context_dock = confirmed
            .capabilities
            .iter()
            .any(|capability| capability.0 == COCKPIT_CONTEXT_CAPABILITY);
        let supports_structured_diff = confirmed
            .capabilities
            .iter()
            .any(|capability| capability.0 == crate::STRUCTURED_DIFF_CAPABILITY);
        let supports_operator_git = confirmed
            .capabilities
            .iter()
            .any(|capability| capability.0 == crate::OPERATOR_GIT_CAPABILITY);
        // Native turns publish `turn_id`; typed ACP attempts publish an exact,
        // owner-scoped session status. Both are Core facts, unlike the broad
        // Lane lifecycle state, so either may prove that the composer must queue.
        let busy = selected_owner.is_some_and(|owner| {
            owner.turn_id.is_some()
                || view.agent_sessions.iter().any(|session| {
                    &session.owner == owner
                        && matches!(
                            session.status,
                            AgentSessionStatus::Starting
                                | AgentSessionStatus::Running
                                | AgentSessionStatus::WaitingApproval
                        )
                })
        });

        let provider_id = view
            .provider
            .as_ref()
            .map(|provider| provider.provider_id.clone())
            .unwrap_or_else(|| view.snapshot.provider_family.clone());
        let model = view
            .provider
            .as_ref()
            .map(|provider| provider.model.clone())
            .unwrap_or_else(|| view.snapshot.model_label.clone());
        let token_total = view.token_cost.as_ref().map_or(0, |cost| cost.total_tokens);
        let cost_micro_usd = view
            .token_cost
            .as_ref()
            .and_then(|cost| cost.cost_micro_usd);

        let mut transcript = Vec::new();
        if exact_binding.is_some() {
            transcript.extend(
                view.lane_outputs
                    .iter()
                    .filter(|output| selected_lane_id.as_deref() == Some(output.lane_id.as_str()))
                    .enumerate()
                    .map(|(ordinal, output)| D1TranscriptRowProjection {
                        // Core does not expose a lane-output identity yet. This ordinal only
                        // disambiguates rows inside one projection; it is not durable identity.
                        id: format!(
                            "lane-output-{}-{}-{ordinal}",
                            output.lane_id,
                            output.timestamp.unwrap_or(0)
                        ),
                        kind: "lane_output",
                        content: output.content.clone(),
                    }),
            );
        }
        if transcript.len() > 240 {
            transcript.drain(..transcript.len() - 240);
        }

        let context_dock = if supports_context_dock {
            let lane_agent = exact_binding
                .map(|binding| (&binding.lane_id, &binding.owner))
                .or_else(|| {
                    restored_agent_session.map(|session| (&session.lane_id, &session.owner))
                })
                .map(|(lane_id, owner)| D1LaneAgentProjection {
                    lane_id: lane_id.clone(),
                    workspace_id: owner.workspace_id.clone(),
                    project_id: owner.project_id.clone(),
                    session_id: owner.session_id.clone(),
                    task_id: owner.task_id.clone(),
                    turn_id: owner.turn_id.clone(),
                });
            let selected_lane_matches =
                |owner: &viden_core::RuntimeOwner| selected_owner == Some(owner);
            let mut checklist = view
                .workspace_changes
                .iter()
                .filter(|change| selected_lane_matches(&change.owner))
                .map(|change| D1ChecklistItemProjection {
                    id: change.id.clone(),
                    kind: "workspace_change",
                    label: change.path.clone(),
                    status: workspace_change_kind(change.kind),
                    command: None,
                    path: Some(change.path.clone()),
                    summary: None,
                    patch: change.patch.clone(),
                    diff: change.diff.as_ref().map(diff_document_projection),
                    failing_location: None,
                    additions: Some(change.additions),
                    deletions: Some(change.deletions),
                })
                .collect::<Vec<_>>();
            checklist.extend(
                view.check_runs
                    .iter()
                    .filter(|check| selected_lane_matches(&check.owner))
                    .map(|check| D1ChecklistItemProjection {
                        id: check.id.clone(),
                        kind: "check_run",
                        label: check.label.clone(),
                        status: check_run_status(check.status),
                        command: Some(check.command.clone()),
                        path: None,
                        summary: Some(check.summary.clone()),
                        patch: None,
                        // A check run is not a file change; Core publishes no
                        // rows for one and none are invented here.
                        diff: None,
                        failing_location: check.failing_location.clone(),
                        additions: None,
                        deletions: None,
                    }),
            );
            D1ContextDockProjection {
                source: view
                    .workspace_source
                    .as_ref()
                    .map(|source| D1WorkspaceSourceProjection {
                        status: workspace_source_status(source.status),
                        branch: source.branch.clone(),
                        worktree: source.worktree.clone(),
                        ahead: source.ahead,
                        behind: source.behind,
                        added: source.added,
                        deleted: source.deleted,
                        dirty: source.dirty,
                    }),
                // GUI-CORE-008: a budget belongs to the selected Lane only
                // through the typed task scope named by the exact owner Core
                // bound to that Lane. No exact owner, no task, or no budget in
                // that scope stays `None`: the dock never borrows a budget that
                // is merely published, and never guesses a scope shape.
                context: selected_owner
                    .and_then(|owner| owner.task_id.as_deref())
                    .and_then(|task_id| {
                        let scope = ContextScope::Task(task_id.to_string());
                        // One task accumulates a budget record per built context
                        // bundle, so the Lane's current pressure is the freshest
                        // fact inside its own scope. Recency never crosses
                        // scopes, so another Lane's budget stays unreachable.
                        view.context_budgets
                            .iter()
                            .filter(|budget| budget.scope == scope)
                            .enumerate()
                            .max_by_key(|(index, budget)| (budget.updated_at, *index))
                            .map(|(_, budget)| budget)
                    })
                    .map(|budget| D1ContextUsageProjection {
                        budget_id: budget.budget_id.clone(),
                        used_tokens: budget.used_tokens,
                        soft_token_limit: budget.soft_token_limit,
                        hard_token_limit: budget.hard_token_limit,
                        remaining_tokens: budget.remaining_tokens,
                        exceeded: budget.exceeded,
                    }),
                lane_agent,
                provider: view
                    .provider
                    .as_ref()
                    .map(|provider| D1ProviderHealthProjection {
                        provider_id: provider.provider_id.clone(),
                        model: provider.model.clone(),
                        status: provider.status.clone(),
                        request_count: provider.request_count,
                        error_count: provider.error_count,
                        last_latency_ms: provider.last_latency_ms,
                        average_latency_ms: provider.average_latency_ms,
                        tokens_per_second: provider.tokens_per_second,
                    }),
                services: view
                    .runtime_services
                    .iter()
                    .map(|service| D1RuntimeServiceProjection {
                        id: service.id.clone(),
                        kind: runtime_service_kind(service.kind),
                        label: service.label.clone(),
                        status: runtime_service_status(service.status),
                        detail_key: service.detail_key.clone(),
                    })
                    .collect(),
                checklist,
            }
        } else {
            D1ContextDockProjection {
                source: None,
                context: None,
                lane_agent: None,
                provider: None,
                services: Vec::new(),
                checklist: Vec::new(),
            }
        };
        // Statusbar segments carry only published Core facts; a segment whose
        // fact is absent stays `None` so the frontend renders an explicit
        // placeholder instead of a fabricated number.
        let statusbar = D1StatusbarProjection {
            work_mode: view.snapshot.work_mode.cli_name().to_string(),
            permission_level: view.snapshot.permission_level.cli_name().to_string(),
            context: view
                .context_budgets
                .last()
                .map(|budget| D1StatusbarContextProjection {
                    used_tokens: budget.used_tokens,
                    hard_token_limit: budget.hard_token_limit,
                    exceeded: budget.exceeded,
                }),
            event_stream_position: confirmed.cursor.sequence,
            lane: selected_lane.map(|lane| {
                let mut sessions = view
                    .agent_sessions
                    .iter()
                    .filter(|session| session.lane_id == lane.id);
                let sole_session = match (sessions.next(), sessions.next()) {
                    (Some(session), None) => Some(session),
                    _ => None,
                };
                D1StatusbarLaneProjection {
                    lane_id: lane.id.clone(),
                    agent_id: sole_session.map(|session| session.agent_id.clone()),
                    status: lane_status(lane.status).to_string(),
                    progress: lane.task_id.as_ref().and_then(|task_id| {
                        view.tasks
                            .iter()
                            .find(|task| &task.id == task_id)
                            .map(|task| task.progress)
                    }),
                }
            }),
            latency: view
                .provider
                .as_ref()
                .map(|provider| D1StatusbarLatencyProjection {
                    last_latency_ms: provider.last_latency_ms,
                    average_latency_ms: provider.average_latency_ms,
                }),
            tokens: view
                .token_cost
                .as_ref()
                .map(|cost| D1StatusbarTokensProjection {
                    input_tokens: cost.input_tokens,
                    output_tokens: cost.output_tokens,
                }),
            diagnostics_count: view.errors.len() as u64,
            requests: view
                .provider
                .as_ref()
                .map(|provider| D1StatusbarRequestsProjection {
                    request_count: provider.request_count,
                    error_count: provider.error_count,
                }),
            // The badge must match what its navigation target can act on:
            // it takes the operator to the decision surfaces, so counting
            // gates whose session finished weeks ago sends them to a screen
            // with nothing live in it. Dormant gates are still listed there —
            // they are grouped, not hidden — but they are not a count of work
            // waiting on the operator.
            pending_gate_count: view.pending_approvals.len() as u64
                + view
                    .merge_gates
                    .iter()
                    .filter(|gate| gate.status.is_open() && !is_dormant_gate(view, gate))
                    .count() as u64,
        };
        // The titlebar git block is a workspace-level read, not a Lane-scoped
        // one: Core samples `workspace_source` from the workspace root. An
        // absent or unavailable sample projects `None`, so the titlebar omits
        // the block instead of rendering zeroes as a clean, in-sync tree.
        let topbar_source = view
            .workspace_source
            .as_ref()
            .filter(|source| source.status != WorkspaceSourceStatus::Unavailable)
            .map(|source| D1TopbarSourceProjection {
                project: view
                    .project_probe
                    .as_ref()
                    .and_then(|probe| probe.project_name.clone()),
                branch: source.branch.clone(),
                ahead: source.ahead,
                behind: source.behind,
                dirty: source.dirty,
                status: workspace_source_status(source.status),
                truncated: source.status == WorkspaceSourceStatus::Truncated,
                lane_worktree_count: lane_worktree_count(view),
            });
        let recovery = if supports_owner && exact_owner_count > 1 {
            // An ambiguous execution identity is not renderable. Enter the
            // existing snapshot/replay recovery path instead of choosing one.
            //
            // This is a client-local defence, not a Core contract request, so
            // it carries a `D1-` code rather than a `GUI-CORE-` one: the
            // reducer already admits at most one `LaneRuntimeOwnerBinding` per
            // Lane (`RuntimeViewState::apply` ignores a second binding for a
            // lane that already has one), so a view holding two is a view this
            // client should not trust rather than a fact Core has yet to
            // publish.
            self.d6_recovery(
                D6ConnectionState::Recovering,
                Some(D1_OWNER_CARDINALITY_CODE),
                true,
            )?
        } else {
            self.d6_recovery(D6ConnectionState::Live, None, true)?
        };

        Some(D1CockpitProjection {
            preferences: self.preferences()?,
            selected_lane_id,
            topbar_source,
            context_dock,
            lanes: view
                .lanes
                .iter()
                .map(|lane| D1LaneProjection {
                    id: lane.id.clone(),
                    role: lane.role.to_string(),
                    status: lane_status(lane.status).to_string(),
                    summary: lane.summary.clone(),
                    branch: lane.branch.clone(),
                })
                .collect(),
            environment: D1EnvironmentProjection {
                cwd: view.snapshot.cwd.display().to_string(),
                provider_id,
                model,
                work_mode: view.snapshot.work_mode.cli_name().to_string(),
                permission_level: view.snapshot.permission_level.cli_name().to_string(),
                token_total,
                cost_micro_usd,
            },
            live_work: D1LiveWorkProjection {
                // Every live-work fact is scoped by the exact runtime owner Core
                // published on it, matched the same way the context dock matches
                // a workspace change: full `RuntimeOwner` equality against the
                // Lane's own binding (GUI-CORE-010). A fact Core published with
                // no owner is not attributable to any Lane, so it is omitted
                // here rather than shown under whichever Lane happens to be
                // selected; it stays visible wherever this client renders
                // workspace-wide facts.
                tasks: view
                    .tasks
                    .iter()
                    .filter(|task| owned_by_selected_lane(selected_owner, task.owner.as_ref()))
                    .map(|task| D1TaskProjection {
                        id: task.id.clone(),
                        title: task.title.clone(),
                        status: format!("{:?}", task.status).to_lowercase(),
                        progress: task.progress,
                    })
                    .collect(),
                tools: view
                    .active_tool_calls
                    .iter()
                    .filter(|tool| owned_by_selected_lane(selected_owner, tool.owner.as_ref()))
                    .map(|tool| D1ToolProjection {
                        id: tool.tool_call_id.clone(),
                        name: tool.name.clone(),
                        input_preview: tool.input_preview.clone(),
                        state: "active",
                    })
                    .collect(),
                approvals: view
                    .pending_approvals
                    .iter()
                    .filter(|approval| selected_owner == Some(&approval.owner))
                    .map(|approval| D1ApprovalProjection {
                        id: approval.id.clone(),
                        title: approval.title.clone(),
                        risk: format!("{:?}", approval.risk).to_lowercase(),
                    })
                    .collect(),
                queued_inputs: view
                    .queued_inputs
                    .iter()
                    .filter(|input| owned_by_selected_lane(selected_owner, input.owner.as_ref()))
                    .map(|input| D1QueuedInputProjection {
                        id: input.id.clone(),
                        content_preview: input.content_preview.clone(),
                    })
                    .collect(),
                evidence: view
                    .latest_evidence
                    .iter()
                    .filter(|evidence| {
                        owned_by_selected_lane(selected_owner, evidence.owner.as_ref())
                    })
                    .map(|evidence| D1EvidenceProjection {
                        id: evidence.id.clone(),
                        kind: evidence.kind.clone(),
                        summary: evidence.summary.clone(),
                        path: evidence.path.clone(),
                    })
                    .collect(),
            },
            transcript,
            workspace_eligibility: view.workspace_eligibility.as_ref().map(|eligibility| {
                D1WorkspaceEligibilityProjection {
                    is_git_repository: eligibility.is_git_repository,
                    has_head: eligibility.has_head,
                    can_create_lane: eligibility.can_create_lane,
                    diagnostic: eligibility.diagnostic.clone(),
                }
            }),
            starter_lane_previews: view
                .starter_lane_previews
                .iter()
                .map(|preview| D1StarterLanePreviewProjection {
                    preview_id: preview.preview_id.clone(),
                    content_sha256: preview.content_sha256.clone(),
                    lane_id: preview.lane.id.clone(),
                    branch: preview.lane.branch.clone(),
                    diagnostics: preview.diagnostics.clone(),
                })
                .collect(),
            starter_lane_receipts: view
                .starter_lane_receipts
                .iter()
                .map(|receipt| D1StarterLaneReceiptProjection {
                    preview_id: receipt.preview_id.clone(),
                    lane_id: receipt.lane.id.clone(),
                })
                .collect(),
            agent_adapters: view
                .agent_adapters
                .iter()
                .filter(|adapter| adapter.route == AgentRoute::Acp)
                .map(|adapter| D1AgentAdapterProjection {
                    agent_id: adapter.agent_id.clone(),
                    display_name: adapter.display_name.clone(),
                    startability: agent_startability(adapter.startability).to_string(),
                    diagnostics: adapter.diagnostics.clone(),
                    models: adapter.models.clone(),
                })
                .collect(),
            agent_sessions: view
                .agent_sessions
                .iter()
                .filter(|session| selected_owner == Some(&session.owner))
                .map(|session| D1AgentSessionProjection {
                    session_id: session.session_id.clone(),
                    lane_id: session.lane_id.clone(),
                    agent_id: session.agent_id.clone(),
                    model: session.model.clone(),
                    status: agent_session_status(session.status).to_string(),
                    task: session.task.clone(),
                    diagnostic: session.diagnostic.clone(),
                    output: session.output.clone(),
                    conversation: view
                        .agent_conversation
                        .iter()
                        .filter(|message| message.session_id == session.session_id)
                        .map(|message| D1AgentConversationMessageProjection {
                            message_id: message.message_id.clone(),
                            role: match message.role {
                                AgentConversationRole::User => "user",
                                AgentConversationRole::Assistant => "assistant",
                            },
                            content: message.content.clone(),
                            parts: message.parts.iter().map(content_part).collect(),
                        })
                        .collect(),
                })
                .collect(),
            // Session-input views do not carry a RuntimeOwner. Exclude them rather
            // than associating a global history entry with the selected Lane.
            agent_session_inputs: Vec::new(),
            cost_usage: view
                .cost_usage
                .iter()
                .map(|cost| D1CostUsageProjection {
                    usage_id: cost.usage_id.clone(),
                    attempt_index: cost.attempt_index,
                    total_tokens: cost.tokens.total_tokens.unwrap_or(0),
                    actual_cost_micro_usd: cost
                        .actual_cost
                        .as_ref()
                        .map(|amount| amount.micro_units),
                    outcome: cost.outcome.as_str().to_string(),
                })
                .collect(),
            replay_cursor: D1CursorProjection {
                stream_id: confirmed.cursor.stream_id.clone(),
                sequence: confirmed.cursor.sequence,
            },
            composer: D1ComposerProjection {
                editable: supports_owner && selected_owner.is_some(),
                busy,
                can_cancel: supports_owner
                    && selected_lane.is_some_and(|lane| lane.is_active())
                    && exact_binding.is_some(),
                can_submit_immediately: supports_owner && selected_owner.is_some() && !busy,
            },
            statusbar,
            permission_dock: match selected_owner {
                Some(owner) => self.permission_dock_for_owner(owner)?,
                None => self.empty_permission_dock()?,
            },
            recovery,
            unavailable_features: unavailable_features(
                supports_structured_diff,
                supports_operator_git,
            ),
        })
    }
}

/// Distinct worktrees held by the project's active Lanes.
///
/// Core publishes no git worktree inventory, so the Lane records' `worktree`
/// names are the only published fact behind the titlebar's worktree chip.
/// Finished Lanes are excluded (their worktree is no longer live work) and
/// duplicates collapse, because two Lanes sharing a worktree are one worktree.
fn lane_worktree_count(view: &RuntimeViewState) -> u32 {
    view.lanes
        .iter()
        .filter(|lane| lane.is_active())
        .filter_map(|lane| lane.worktree.as_deref())
        .collect::<BTreeSet<_>>()
        .len() as u32
}

/// A GUI mutation may use an owner only when Core publishes exactly one binding for that Lane.
fn exact_owner_binding<'a>(
    view: &'a RuntimeViewState,
    lane_id: &str,
) -> Option<&'a viden_core::LaneRuntimeOwnerBinding> {
    let mut bindings = view
        .lane_runtime_owners
        .iter()
        .filter(|binding| binding.lane_id == lane_id);
    let binding = bindings.next()?;
    (bindings.next().is_none() && binding.owner.lane_id.as_deref() == Some(lane_id))
        .then_some(binding)
}

/// Whether a live-work fact belongs to the selected Lane (GUI-CORE-010).
///
/// Both sides must be present and exactly equal. A fact Core published with no
/// owner is not attributable to any Lane, and a Lane with no exact Core-bound
/// owner has nothing to match against — either way the answer is "no", never a
/// fallback that shows one Lane's work under another.
fn owned_by_selected_lane(
    selected_owner: Option<&viden_core::RuntimeOwner>,
    fact_owner: Option<&viden_core::RuntimeOwner>,
) -> bool {
    match (selected_owner, fact_owner) {
        (Some(selected), Some(fact)) => selected == fact,
        _ => false,
    }
}

fn exact_owner_count(view: &RuntimeViewState, lane_id: &str) -> usize {
    view.lane_runtime_owners
        .iter()
        .filter(|binding| binding.lane_id == lane_id)
        .count()
}

/// Restored terminal ACP sessions are durable Core facts even though process-local
/// Lane runtime owners intentionally disappear across a Core restart.
pub(crate) fn exact_terminal_agent_session<'a>(
    view: &'a RuntimeViewState,
    lane_id: &str,
) -> Option<&'a viden_core::AgentSessionView> {
    let mut sessions = view.agent_sessions.iter().filter(|session| {
        session.lane_id == lane_id
            && session.owner.lane_id.as_deref() == Some(lane_id)
            && view
                .agent_adapters
                .iter()
                .find(|adapter| adapter.agent_id == session.agent_id)
                .map_or(session.agent_id != "viden-built-in", |adapter| {
                    adapter.route == AgentRoute::Acp
                })
            && matches!(
                session.status,
                AgentSessionStatus::Completed
                    | AgentSessionStatus::Failed
                    | AgentSessionStatus::Cancelled
            )
    });
    let session = sessions.next()?;
    sessions.next().is_none().then_some(session)
}

fn project_projection(probe: &ProjectProbe) -> D11ProjectProjection {
    D11ProjectProjection {
        root: probe.root.clone(),
        is_git_repository: probe.is_git_repository,
        config_state: match probe.config_state {
            viden_core::ProjectConfigState::Missing => "missing",
            viden_core::ProjectConfigState::Valid => "valid",
            viden_core::ProjectConfigState::Invalid => "invalid",
        },
        project_name: probe.project_name.clone(),
        mode: probe.pack.clone(),
        diagnostics: probe.diagnostics.clone(),
    }
}

fn config_projection(preview: &ProjectConfigPreview) -> D11ConfigProjection {
    D11ConfigProjection {
        preview_id: preview.preview_id.clone(),
        relative_path: preview.relative_path.clone(),
        content_sha256: preview.content_sha256.clone(),
        exact_contents: preview.exact_contents.clone(),
        valid: preview.is_valid() && preview.exact_contents.is_some(),
        diagnostics: preview.diagnostics.clone(),
    }
}

fn confirmed_config_projection(preview: &ProjectConfigPreview) -> D11ConfirmedConfigProjection {
    D11ConfirmedConfigProjection {
        preview_id: preview.preview_id.clone(),
        relative_path: preview.relative_path.clone(),
        content_sha256: preview.content_sha256.clone(),
    }
}

fn provider_projection(provider: &ProviderHealthView) -> D11ProviderProjection {
    D11ProviderProjection {
        provider_id: provider.provider_id.clone(),
        model: provider.model.clone(),
        status: provider.status.clone(),
        warning: !matches!(provider.status.as_str(), "healthy" | "ready" | "available"),
    }
}

fn credential_projection(handle: &CredentialHandle) -> D11CredentialProjection {
    D11CredentialProjection {
        provider_id: handle.provider_id.clone(),
        masked_handle: mask_handle(&handle.backend_id),
        status: match handle.status {
            viden_core::CredentialStatus::Available => "available",
            viden_core::CredentialStatus::Missing => "missing",
            viden_core::CredentialStatus::Locked => "locked",
            viden_core::CredentialStatus::Error => "error",
        },
    }
}

fn mask_handle(handle: &str) -> String {
    let chars = handle.chars().collect::<Vec<_>>();
    if chars.len() <= 4 {
        return "••••".to_string();
    }
    format!(
        "{}{}••••{}{}",
        chars[0],
        chars[1],
        chars[chars.len() - 2],
        chars[chars.len() - 1]
    )
}

fn starter_lane_projection(lane: &AgentLaneRecord) -> D11StarterLaneProjection {
    D11StarterLaneProjection {
        id: lane.id.clone(),
        role: lane.role.to_string(),
        status: format!("{:?}", lane.status).to_lowercase(),
    }
}

fn lane_status(status: viden_core::LaneStatus) -> &'static str {
    match status {
        viden_core::LaneStatus::Draft => "draft",
        viden_core::LaneStatus::Queued => "queued",
        viden_core::LaneStatus::Starting => "starting",
        viden_core::LaneStatus::Running => "running",
        viden_core::LaneStatus::WaitingApproval => "waiting_approval",
        viden_core::LaneStatus::NeedsInput => "needs_input",
        viden_core::LaneStatus::Blocked => "blocked",
        viden_core::LaneStatus::Attached => "attached",
        viden_core::LaneStatus::Detached => "detached",
        viden_core::LaneStatus::Done => "done",
        viden_core::LaneStatus::Failed => "failed",
        viden_core::LaneStatus::Cancelled => "cancelled",
        viden_core::LaneStatus::Archived => "archived",
    }
}

fn agent_startability(startability: AgentStartability) -> &'static str {
    match startability {
        AgentStartability::Ready => "ready",
        AgentStartability::ProbeRequired => "probe_required",
        AgentStartability::InstallRequired => "install_required",
        AgentStartability::AuthenticationRequired => "authentication_required",
        AgentStartability::Unavailable => "unavailable",
    }
}

fn agent_session_status(status: AgentSessionStatus) -> &'static str {
    match status {
        AgentSessionStatus::Starting => "starting",
        AgentSessionStatus::Running => "running",
        AgentSessionStatus::WaitingApproval => "waiting_approval",
        AgentSessionStatus::Completed => "completed",
        AgentSessionStatus::Failed => "failed",
        AgentSessionStatus::Cancelled => "cancelled",
    }
}

pub(crate) fn workspace_source_status(status: WorkspaceSourceStatus) -> &'static str {
    match status {
        WorkspaceSourceStatus::Ready => "ready",
        WorkspaceSourceStatus::Unavailable => "unavailable",
        WorkspaceSourceStatus::Truncated => "truncated",
    }
}

fn runtime_service_kind(kind: RuntimeServiceKind) -> &'static str {
    match kind {
        RuntimeServiceKind::Mcp => "mcp",
        RuntimeServiceKind::Lsp => "lsp",
    }
}

fn runtime_service_status(status: RuntimeServiceStatus) -> &'static str {
    match status {
        RuntimeServiceStatus::Connected => "connected",
        RuntimeServiceStatus::Ready => "ready",
        RuntimeServiceStatus::Degraded => "degraded",
        RuntimeServiceStatus::Offline => "offline",
        RuntimeServiceStatus::Unavailable => "unavailable",
    }
}

pub(crate) fn workspace_change_kind(kind: WorkspaceChangeKind) -> &'static str {
    match kind {
        WorkspaceChangeKind::Added => "added",
        WorkspaceChangeKind::Modified => "modified",
        WorkspaceChangeKind::Deleted => "deleted",
        WorkspaceChangeKind::Renamed => "renamed",
        WorkspaceChangeKind::Untracked => "untracked",
    }
}

fn check_run_status(status: CheckRunStatus) -> &'static str {
    match status {
        CheckRunStatus::Queued => "queued",
        CheckRunStatus::Running => "running",
        CheckRunStatus::Passed => "passed",
        CheckRunStatus::Failed => "failed",
        CheckRunStatus::Cancelled => "cancelled",
    }
}

fn agent_task_status(status: AgentTaskStatus) -> &'static str {
    match status {
        AgentTaskStatus::Queued => "queued",
        AgentTaskStatus::Thinking => "thinking",
        AgentTaskStatus::Streaming => "streaming",
        AgentTaskStatus::Editing => "editing",
        AgentTaskStatus::RunningTool => "running_tool",
        AgentTaskStatus::Testing => "testing",
        AgentTaskStatus::WaitingApproval => "waiting_approval",
        AgentTaskStatus::NeedsInput => "needs_input",
        AgentTaskStatus::Blocked => "blocked",
        AgentTaskStatus::Reviewing => "reviewing",
        AgentTaskStatus::Running => "running",
        AgentTaskStatus::Attached => "attached",
        AgentTaskStatus::Done => "done",
        AgentTaskStatus::Applied => "applied",
        AgentTaskStatus::Discarded => "discarded",
        AgentTaskStatus::Failed => "failed",
        AgentTaskStatus::Cancelled => "cancelled",
        AgentTaskStatus::Archived => "archived",
    }
}

fn agent_dag_status(status: AgentDagStatus) -> &'static str {
    match status {
        AgentDagStatus::Draft => "draft",
        AgentDagStatus::Active => "active",
        AgentDagStatus::Paused => "paused",
        AgentDagStatus::Blocked => "blocked",
        AgentDagStatus::Completed => "completed",
        AgentDagStatus::Cancelled => "cancelled",
    }
}

/// Whether a gate is dormant: still awaiting a decision, while the Agent
/// session it belongs to has already finished.
///
/// A finished session cannot produce the evidence its gate is waiting for, so
/// such a gate is residue rather than actionable work. Eight of them, from
/// sessions terminal for weeks, were rendering as first-class pending gates —
/// burying live work in D12's list, inflating the statusbar badge, and holding
/// D6 out of its clear state.
///
/// Dormancy changes ordering, labelling, and counting only. Every dormant gate
/// stays in the list, stays selectable, and keeps every action Core allows: this
/// is grouping, never hiding.
///
/// The join is `gate.owner.session_id` — the Agent session Core publishes, not
/// the ACP protocol handle a merge gate's id is keyed on. A gate that names no
/// session, or names one this view has never seen, is never dormant: an unknown
/// session is not a finished one. This is the single definition the D12 list,
/// the D1 statusbar badge, the D6 state machine, and the command palette all
/// read; four private copies would drift apart.
pub(crate) fn is_dormant_gate(view: &RuntimeViewState, gate: &MergeGateRecord) -> bool {
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

/// Whether any gate is both open and still attached to a live session — the
/// only kind of gate an operator can actually act on right now.
fn has_actionable_merge_gate(view: &RuntimeViewState) -> bool {
    view.merge_gates
        .iter()
        .any(|gate| gate.status.is_open() && !is_dormant_gate(view, gate))
}

fn merge_gate_status(status: MergeGateStatus) -> &'static str {
    match status {
        MergeGateStatus::Proposed => "proposed",
        MergeGateStatus::CollectingEvidence => "collecting_evidence",
        MergeGateStatus::Blocked => "blocked",
        MergeGateStatus::NeedsChanges => "needs_changes",
        MergeGateStatus::Accepted => "accepted",
        MergeGateStatus::Merged => "merged",
        MergeGateStatus::Reverted => "reverted",
    }
}

fn merge_gate_type(gate_type: MergeGateType) -> &'static str {
    match gate_type {
        MergeGateType::Patch => "patch",
        MergeGateType::Review => "review",
        MergeGateType::Contract => "contract",
        MergeGateType::Handoff => "handoff",
        MergeGateType::Artifact => "artifact",
    }
}

fn conflict_bounce_status(status: ConflictBounceStatus) -> &'static str {
    match status {
        ConflictBounceStatus::Pending => "pending",
        ConflictBounceStatus::Revalidated => "revalidated",
        ConflictBounceStatus::Resolved => "resolved",
    }
}

/// The exact `RuntimeOwner` Core will demand as the acceptance actor.
///
/// `decide_merge_gate` splits on the validator: with one recorded, acceptance
/// must come from a Lane matching the validator's owner
/// (`runtime_owner_matches_validator_lane`, which requires a Lane id);
/// without one, the actor must equal the gate owner. Anything else is a
/// rejection, so a gate with no actor this client can replay stays closed.
pub(crate) fn d12_accept_actor(gate: &viden_core::MergeGateRecord) -> Option<RuntimeOwner> {
    match &gate.validator {
        Some(validator) => validator
            .owner
            .lane_id
            .is_some()
            .then(|| validator.owner.clone()),
        None => Some(gate.owner.clone()),
    }
}

/// The exact `RuntimeOwner` Core will accept as the rejection actor.
///
/// `validate_reject_actor` refuses the default owner outright, then admits
/// either the validator's Lane or the gate owner. The gate owner is preferred
/// because it satisfies the second branch without the validator's Lane-id
/// requirement.
pub(crate) fn d12_reject_actor(gate: &viden_core::MergeGateRecord) -> Option<RuntimeOwner> {
    if gate.owner != RuntimeOwner::default() {
        return Some(gate.owner.clone());
    }
    gate.validator
        .as_ref()
        .filter(|validator| validator.owner.lane_id.is_some())
        .filter(|validator| validator.owner != RuntimeOwner::default())
        .map(|validator| validator.owner.clone())
}

/// The exact `RuntimeOwner` Core will accept as the review decider.
///
/// `validate_review_decider` demands an actor that is not the default owner,
/// whose `lane_id` is the review's independent reviewer Lane, whose `task_id`
/// is the review's task (`validate_owner`), and whose workspace/project match
/// the review owner's. Core stores exactly such an owner on the gate validator
/// when the review is requested, so that record is replayed first. The fallback
/// reproduces Core's own `reviewer_owner_from_requester`: the review owner
/// re-pointed at the reviewer Lane with session and turn identity left
/// unclaimed. A review that yields neither stays fail-closed rather than being
/// decided under an identity Core would reject.
pub(crate) fn d2_review_actor(
    view: &RuntimeViewState,
    review: &ReviewRequestRecord,
) -> Option<RuntimeOwner> {
    if let Some(validator) = view
        .merge_gates
        .iter()
        .find(|gate| gate.gate_id == review.gate_id)
        .and_then(|gate| gate.validator.clone())
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

/// The reviewed-evidence bindings Core will compare the acceptance against.
///
/// With a validator, `decide_merge_gate` requires the bindings to equal the
/// review request's recorded set exactly; without one it requires the
/// bindings derived from the gate's own evidence, except for a default-owner
/// gate, where Core fills an empty list in itself. Each case is replayed from
/// Core's own records rather than rebuilt from display text.
pub(crate) fn d12_reviewed_evidence(
    view: &RuntimeViewState,
    gate: &viden_core::MergeGateRecord,
) -> Option<Vec<viden_core::ReviewedEvidenceBinding>> {
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
    d12_canonical_bindings(view, gate)
}

/// Rebuilds the gate's evidence bindings from the canonical references Core
/// published. `None` when any listed evidence is absent or not canonical:
/// Core cannot verify such a gate, so the client must not offer to accept it.
fn d12_canonical_bindings(
    view: &RuntimeViewState,
    gate: &viden_core::MergeGateRecord,
) -> Option<Vec<viden_core::ReviewedEvidenceBinding>> {
    let mut bindings = Vec::with_capacity(gate.evidence_ids.len());
    for evidence_id in &gate.evidence_ids {
        let evidence = view
            .latest_evidence
            .iter()
            .find(|evidence| evidence.id == *evidence_id)?;
        let canonical = evidence.canonical.as_ref()?;
        bindings.push(viden_core::ReviewedEvidenceBinding {
            evidence_id: evidence_id.clone(),
            source_hash: canonical.source_hash.clone(),
        });
    }
    bindings.sort();
    bindings.dedup();
    Some(bindings)
}

/// Why Core would refuse `AcceptMergeGate`, or `None` when nothing the client
/// can see blocks it.
fn d12_accept_block(
    view: &RuntimeViewState,
    gate: &viden_core::MergeGateRecord,
    terminal: bool,
    missing_evidence: &[String],
) -> Option<&'static str> {
    if terminal {
        return Some(d12_action_code::GATE_CLOSED);
    }
    if !missing_evidence.is_empty() {
        return Some(d12_action_code::MISSING_EVIDENCE);
    }
    if gate.policy_snapshot.requires_independent_validator
        && !gate
            .validator
            .as_ref()
            .is_some_and(|validator| validator.independent)
    {
        return Some(d12_action_code::VALIDATOR_REQUIRED);
    }
    // A pending bounce means the origin Lane has not revalidated yet; Core
    // refuses acceptance until it has.
    if gate
        .conflict
        .as_ref()
        .is_some_and(|conflict| conflict.status == ConflictBounceStatus::Pending)
    {
        return Some(d12_action_code::CONFLICT_PENDING);
    }
    if d12_canonical_bindings(view, gate).is_none() {
        return Some(d12_action_code::EVIDENCE_NOT_CANONICAL);
    }
    if let Some(validator) = &gate.validator {
        let pending_review = view.review_requests.iter().any(|review| {
            review.review_id == validator.review_request_id
                && review.status == ReviewRequestStatus::Pending
        });
        if !pending_review {
            return Some(d12_action_code::REVIEW_NOT_PENDING);
        }
    }
    if d12_accept_actor(gate).is_none() || d12_reviewed_evidence(view, gate).is_none() {
        return Some(d12_action_code::NO_ACTOR);
    }
    None
}

/// Why Core would refuse `RejectMergeGate`, or `None` when nothing blocks it.
fn d12_reject_block(gate: &viden_core::MergeGateRecord, terminal: bool) -> Option<&'static str> {
    if terminal {
        return Some(d12_action_code::GATE_CLOSED);
    }
    if d12_reject_actor(gate).is_none() {
        return Some(d12_action_code::NO_ACTOR);
    }
    None
}

/// Projects a Core content part. Nothing is resolved or fetched here: the
/// reference stays exactly as Core published it.
fn content_part(part: &viden_core::AgentContentPart) -> D1ContentPartProjection {
    match part {
        viden_core::AgentContentPart::Text { text } => D1ContentPartProjection {
            kind: "text".to_string(),
            media_type: None,
            reference: None,
            text: Some(text.clone()),
            label: None,
        },
        viden_core::AgentContentPart::Image {
            media_type,
            reference,
            alt,
        } => D1ContentPartProjection {
            kind: "image".to_string(),
            media_type: Some(media_type.clone()),
            reference: Some(reference.clone()),
            text: None,
            label: alt.clone(),
        },
        viden_core::AgentContentPart::File {
            media_type,
            reference,
            name,
        } => D1ContentPartProjection {
            kind: "file".to_string(),
            media_type: Some(media_type.clone()),
            reference: Some(reference.clone()),
            text: None,
            label: name.clone(),
        },
        // Preserved rather than dropped: the operator learns content exists
        // that this build cannot render.
        viden_core::AgentContentPart::Unknown { kind, .. } => D1ContentPartProjection {
            kind: kind.clone(),
            media_type: None,
            reference: None,
            text: None,
            label: None,
        },
    }
}

fn agent_role(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "planner",
        AgentRole::Coder => "coder",
        AgentRole::Reviewer => "reviewer",
        AgentRole::Tester => "tester",
        AgentRole::DocWriter => "doc_writer",
        AgentRole::Researcher => "researcher",
        AgentRole::ReleaseOperator => "release_operator",
    }
}

fn agent_route(route: AgentRoute) -> &'static str {
    match route {
        AgentRoute::BuiltIn => "built_in",
        AgentRoute::Acp => "acp",
        AgentRoute::Terminal => "terminal",
        AgentRoute::Tmux => "tmux",
    }
}

/// One audit object filter, built from Core's own object-kind constants.
///
/// The kind vocabulary stays in Rust: a frontend that spelled `"merge_gate"`
/// itself could silently drift from `AuditObjectRef`'s constants and query an
/// object kind Core never links.
fn audit_scope(kind: &str, id: &str) -> crate::D14AuditScopeProjection {
    crate::D14AuditScopeProjection {
        kind: kind.to_string(),
        id: id.to_string(),
    }
}

/// Core's own cost-meterability verdict for a route.
///
/// Read from `AgentRoute::cost_meterability` rather than by re-matching the
/// route here: duplicating the policy would let the monitor disagree with Core
/// about which lanes can carry a cost figure at all.
fn cost_meterability(route: AgentRoute) -> String {
    match route.cost_meterability() {
        CostMeterability::Metered => "metered",
        CostMeterability::Blind => "blind",
    }
    .to_string()
}

/// The bounded run facts D10 shows in place of a cost figure Core cannot
/// produce. Projected only for a cost-blind lane; see `D10LaneProjection`.
fn lane_run_stats(lane: &AgentLaneRecord) -> Option<crate::D10RunStatsProjection> {
    if cost_meterability(lane.route) != "blind" {
        return None;
    }
    let stats = lane.run_stats.as_ref()?;
    Some(crate::D10RunStatsProjection {
        wall_time: humanize_wall_time(stats.wall_time_ms),
        wall_time_ms: stats.wall_time_ms,
        run_count: stats.run_count,
        diff_bytes: stats.diff_bytes,
        last_exit_code: stats.last_exit_code,
    })
}

/// Humanizes a duration on the host so TUI and GUI read the same string.
///
/// Three bands, chosen so the rendered precision never exceeds what the number
/// supports: whole milliseconds below a second, one decimal second below a
/// minute, and whole minutes with whole seconds above it.
fn humanize_wall_time(milliseconds: u64) -> String {
    if milliseconds < 1_000 {
        return format!("{milliseconds}ms");
    }
    if milliseconds < 60_000 {
        // Truncate rather than round: a rounded 59_950ms would read "60.0s",
        // which is a duration this band cannot express.
        return format!("{}.{}s", milliseconds / 1_000, (milliseconds % 1_000) / 100);
    }
    let seconds = milliseconds / 1_000;
    format!("{}m {}s", seconds / 60, seconds % 60)
}

fn gate_strength(strength: GateStrength) -> &'static str {
    match strength {
        GateStrength::Full => "full",
        GateStrength::Cooperative => "cooperative",
        GateStrength::Containment => "containment",
    }
}

fn mutation_policy(policy: MutationPolicy) -> &'static str {
    match policy {
        MutationPolicy::Autonomous => "autonomous",
        MutationPolicy::ProposeOnly => "propose_only",
        MutationPolicy::ReadOnly => "read_only",
    }
}

/// Core statuses that block on a person. Every other status is progress the
/// monitor must not escalate into the "awaiting you" count.
fn lane_awaits_human(status: LaneStatus) -> bool {
    matches!(
        status,
        LaneStatus::WaitingApproval | LaneStatus::NeedsInput | LaneStatus::Blocked
    )
}

fn approval_risk(risk: ApprovalRisk) -> &'static str {
    match risk {
        ApprovalRisk::Low => "low",
        ApprovalRisk::Medium => "medium",
        ApprovalRisk::High => "high",
        ApprovalRisk::Critical => "critical",
    }
}

fn gate_queue_item(approval: &ApprovalRequestView) -> D2QueueItemProjection {
    D2QueueItemProjection {
        id: approval.id.clone(),
        kind: D2_KIND_GATE.to_string(),
        title: approval.title.clone(),
        project_id: approval.owner.project_id.clone(),
        lane_id: approval.owner.lane_id.clone(),
        session_id: approval.owner.session_id.clone(),
        task_id: approval.owner.task_id.clone(),
        risk: Some(approval_risk(approval.risk).to_string()),
        status: "pending".to_string(),
        audit_id: approval.audit_id.clone(),
        updated_at: None,
        expires_at: Some(approval.expires_at),
    }
}

fn contract_queue_item(contract: &ContractRecord) -> D2QueueItemProjection {
    D2QueueItemProjection {
        id: contract.contract_id.clone(),
        kind: D2_KIND_CONTRACT.to_string(),
        title: contract.summary.clone(),
        project_id: contract.owner.project_id.clone(),
        lane_id: contract.owner.lane_id.clone(),
        session_id: contract.owner.session_id.clone(),
        task_id: Some(contract.task_id.clone()),
        risk: None,
        status: match contract.decision {
            ContractDecision::Confirmed => "confirmed",
            ContractDecision::Rejected => "rejected",
        }
        .to_string(),
        audit_id: contract.audit_id.clone(),
        updated_at: Some(contract.updated_at),
        expires_at: None,
    }
}

fn review_queue_item(review: &ReviewRequestRecord) -> D2QueueItemProjection {
    D2QueueItemProjection {
        id: review.review_id.clone(),
        kind: D2_KIND_REVIEW.to_string(),
        title: review.review_id.clone(),
        project_id: review.owner.project_id.clone(),
        lane_id: review.owner.lane_id.clone(),
        session_id: review.owner.session_id.clone(),
        task_id: Some(review.task_id.clone()),
        risk: None,
        status: match review.status {
            ReviewRequestStatus::Pending => "pending",
            ReviewRequestStatus::Accepted => "accepted",
            ReviewRequestStatus::Rejected => "rejected",
        }
        .to_string(),
        audit_id: review.audit_id.clone(),
        updated_at: Some(review.updated_at),
        expires_at: None,
    }
}

fn evidence_projection(evidence: &viden_core::EvidenceView) -> D2EvidenceProjection {
    D2EvidenceProjection {
        id: evidence.id.clone(),
        kind: evidence.kind.clone(),
        summary: evidence.summary.clone(),
        path: evidence.path.clone(),
        source: evidence.source.clone(),
        timestamp: evidence.timestamp,
    }
}

/// Evidence named by a review request, in the order Core recorded the ids.
fn evidence_by_ids(view: &RuntimeViewState, ids: &[String]) -> Vec<D2EvidenceProjection> {
    ids.iter()
        .filter_map(|id| {
            view.latest_evidence
                .iter()
                .find(|evidence| &evidence.id == id)
        })
        .map(evidence_projection)
        .collect()
}

/// Evidence attributed to a lane by its Core-recorded `source`. Evidence
/// without a source is not guessed onto a lane.
fn owner_evidence(view: &RuntimeViewState, lane_id: Option<&str>) -> Vec<D2EvidenceProjection> {
    let Some(lane_id) = lane_id else {
        return Vec::new();
    };
    view.latest_evidence
        .iter()
        .filter(|evidence| evidence.source.as_deref() == Some(lane_id))
        .map(evidence_projection)
        .collect()
}

/* ------------------------------------------------------------------ */
/* Structured diff: Core -> projection (GUI-CORE-012)                  */
/* ------------------------------------------------------------------ */

/// Names a row kind without guessing.
///
/// `DiffLineKind` is `#[non_exhaustive]`: a newer Core may publish a row this
/// build does not model (a `\ No newline` marker, a conflict marker). Folding
/// one into `context` would draw it as unchanged code, so it keeps its own
/// name and the frontend renders it as an unnamed row.
fn diff_line_kind(kind: DiffLineKind) -> &'static str {
    match kind {
        DiffLineKind::Context => "context",
        DiffLineKind::Added => "added",
        DiffLineKind::Removed => "removed",
        _ => "unknown",
    }
}

pub(crate) fn diff_line_projection(line: &DiffLine) -> DiffLineProjection {
    DiffLineProjection {
        kind: diff_line_kind(line.kind),
        content: line.content.clone(),
        old_line: line.old_line,
        new_line: line.new_line,
    }
}

pub(crate) fn diff_hunk_projection(hunk: &DiffHunk) -> DiffHunkProjection {
    DiffHunkProjection {
        old_start: hunk.old_start,
        old_lines: hunk.old_lines,
        new_start: hunk.new_start,
        new_lines: hunk.new_lines,
        header: hunk.header.clone(),
        lines: hunk.lines.iter().map(diff_line_projection).collect(),
    }
}

pub(crate) fn diff_file_projection(file: &DiffFile) -> DiffFileProjection {
    DiffFileProjection {
        path: file.path.clone(),
        old_path: file.old_path.clone(),
        kind: workspace_change_kind(file.kind),
        binary: file.binary,
        omitted: file.omitted,
        additions: file.additions,
        deletions: file.deletions,
        hunks: file.hunks.iter().map(diff_hunk_projection).collect(),
    }
}

pub(crate) fn diff_document_projection(document: &DiffDocument) -> DiffDocumentProjection {
    DiffDocumentProjection {
        files: document.files.iter().map(diff_file_projection).collect(),
        truncated: document.truncated,
        byte_limit: document.byte_limit,
    }
}

/// Projects an approval's decision context.
///
/// Returns `None` for an approval Core attached none to, which keeps the
/// preview-only path exactly where Core published nothing.
pub(crate) fn decision_context_projection(
    context: Option<&DecisionContext>,
) -> Option<DecisionContextProjection> {
    context.map(|context| DecisionContextProjection {
        diff: context.diff.as_ref().map(diff_document_projection),
        base_sha256: context.base_sha256.clone(),
    })
}

pub(crate) fn workspace_diff_entry_projection(
    entry: &WorkspaceDiffEntry,
) -> WorkspaceDiffEntryProjection {
    WorkspaceDiffEntryProjection {
        path: entry.path.clone(),
        index: entry.index.map(workspace_change_kind),
        worktree: entry.worktree.map(workspace_change_kind),
        staged: entry.staged,
        diff: entry.diff.as_ref().map(diff_file_projection),
    }
}

pub(crate) fn workspace_diff_source_projection(
    source: &WorkspaceSourceView,
) -> D1WorkspaceSourceProjection {
    D1WorkspaceSourceProjection {
        status: workspace_source_status(source.status),
        branch: source.branch.clone(),
        worktree: source.worktree.clone(),
        ahead: source.ahead,
        behind: source.behind,
        added: source.added,
        deleted: source.deleted,
        dirty: source.dirty,
    }
}

/* -- operator source-control actions (`runtime.operator_git`, GUI-CORE-020) -- */

/// Converts one frontend intent to the Core action and runs *Core's* validator.
///
/// The validator is imported, never reimplemented: an empty commit message, an
/// oversized one, and a path that leaves the target must be refused with the
/// same words here and in Core, or an operator who hits both would think they
/// met two different problems. Refusing here also keeps a malformed action off
/// the wire entirely.
pub(crate) fn operator_git_action(
    intent: crate::operator_git::OperatorGitIntent,
) -> Result<OperatorGitAction, String> {
    use crate::operator_git::OperatorGitIntent;
    let action = match intent {
        OperatorGitIntent::Stage { paths } => OperatorGitAction::Stage { paths },
        OperatorGitIntent::Unstage { paths } => OperatorGitAction::Unstage { paths },
        OperatorGitIntent::Commit { message } => OperatorGitAction::Commit { message },
        OperatorGitIntent::Push {
            remote,
            set_upstream,
        } => OperatorGitAction::Push {
            remote,
            set_upstream,
        },
        OperatorGitIntent::Fetch { remote } => OperatorGitAction::Fetch { remote },
    };
    action.validate()?;
    Ok(action)
}

/// The stable wire spelling of one failure class.
///
/// Matches `#[serde(rename_all = "snake_case")]` on the Core enum, so the
/// frontend keys its localized copy on the same token an audit reader joins
/// on. The wildcard is required and deliberate: the enum is
/// `#[non_exhaustive]`, and a class this build cannot name is reported as
/// `unknown` with the real `detail` rather than squeezed into the
/// nearest-looking class, which would offer the wrong recovery.
pub(crate) fn operator_git_failure_class(class: OperatorGitFailureClass) -> &'static str {
    match class {
        OperatorGitFailureClass::NothingToCommit => "nothing_to_commit",
        OperatorGitFailureClass::NonFastForward => "non_fast_forward",
        OperatorGitFailureClass::AuthenticationRequired => "authentication_required",
        OperatorGitFailureClass::RemoteUnreachable => "remote_unreachable",
        OperatorGitFailureClass::NoUpstream => "no_upstream",
        OperatorGitFailureClass::PathOutsideRepository => "path_outside_repository",
        OperatorGitFailureClass::Other => "other",
        _ => "unknown",
    }
}

/// Projects one settled outcome, keeping `Completed` and `Failed` apart.
pub(crate) fn operator_git_result_projection(
    action: &OperatorGitAction,
    target: &SourceTarget,
    outcome: &OperatorGitOutcome,
    audit_id: &str,
) -> OperatorGitResultProjection {
    let base = OperatorGitResultProjection {
        kind: "failed",
        // Core's own audit verb, echoed from the settling event rather than
        // remembered from the request, so the line names what Core ran.
        action: action.audit_verb(),
        target_lane_id: target_lane_id(target),
        audit_id: audit_id.to_string(),
        output: None,
        truncated: false,
        source: None,
        failure_class: None,
        detail: None,
    };
    match outcome {
        OperatorGitOutcome::Completed {
            output,
            truncated,
            source,
        } => OperatorGitResultProjection {
            kind: "completed",
            output: Some(output.clone()),
            truncated: *truncated,
            source: Some(workspace_diff_source_projection(source)),
            ..base
        },
        OperatorGitOutcome::Failed { class, detail } => OperatorGitResultProjection {
            kind: "failed",
            failure_class: Some(operator_git_failure_class(*class)),
            detail: Some(detail.clone()),
            ..base
        },
        // `OperatorGitOutcome` is `#[non_exhaustive]`: an outcome this build
        // cannot name is reported as an unclassified failure, which is the only
        // direction that fails safe — reading it as a success would let the bar
        // claim a commit Core never confirmed.
        _ => OperatorGitResultProjection {
            failure_class: Some("unknown"),
            ..base
        },
    }
}

/// Whether Core has a source-control approval outstanding for exactly this
/// actor.
///
/// Owner equality is exact on purpose: another Lane's ask is another Lane's
/// decision, and treating it as this bar's gate would leave the operator
/// waiting on a dialog they cannot see. `target.kind` is Core's own grouping
/// for the five actions, not a tool-name match.
pub(crate) fn awaits_operator_git_approval(
    approvals: &[viden_core::ApprovalRequestView],
    owner: &RuntimeOwner,
) -> bool {
    approvals.iter().any(|approval| {
        approval.target.kind == crate::operator_git::OPERATOR_GIT_APPROVAL_KIND
            && approval.owner == *owner
    })
}

/// The Lane a confirmed page describes, read back from Core's own answer.
///
/// `SourceTarget` is `#[non_exhaustive]`, so a target kind this build cannot
/// name resolves to `None` — "not a Lane this build knows" — rather than being
/// mislabelled as the workspace root.
pub(crate) fn target_lane_id(target: &SourceTarget) -> Option<String> {
    match target {
        SourceTarget::Workspace => None,
        SourceTarget::Lane { lane_id } => Some(lane_id.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use viden_core::{
        AgentConversationMessageView, AgentConversationRole, AgentLaneRecord, AgentRole,
        AgentRoute, AgentSessionStatus, AgentSessionView, COCKPIT_CONTEXT_CAPABILITY, CapabilityId,
        DataEgressPolicy, EventCursor, ExecutionTarget, FRONTEND_SCHEMA_V1, GateStrength,
        LaneBudget, LaneStatus, MutationPolicy, PermissionLevel, PermissionMode,
        ResolvedUiPreferences, RuntimeOwner, RuntimeSnapshot, RuntimeSnapshotEnvelope,
        RuntimeViewState, WorkMode,
    };

    use crate::d1::D1_OWNER_CAPABILITY;

    use super::RuntimeProjection;

    #[test]
    fn d1_projects_restored_completed_acp_output_without_a_live_lane_owner() {
        let snapshot = RuntimeSnapshot {
            cwd: PathBuf::from("/workspace/viden"),
            provider_family: "deepseek".to_string(),
            model_label: "deepseek-v4-flash".to_string(),
            work_mode: WorkMode::Build,
            permission_mode: PermissionMode::Default,
            permission_level: PermissionLevel::Ask,
            config_summary: String::new(),
            loaded_config_files: Vec::new(),
            startup_overrides: Vec::new(),
            ui_preferences: ResolvedUiPreferences::default(),
        };
        let owner = RuntimeOwner {
            workspace_id: "workspace-contract-v1".to_string(),
            project_id: "project-viden".to_string(),
            lane_id: Some("lane-acp".to_string()),
            session_id: Some("session-acp".to_string()),
            task_id: None,
            turn_id: None,
        };
        let mut view = RuntimeViewState::new(snapshot.clone());
        view.lanes.push(AgentLaneRecord {
            id: "lane-acp".to_string(),
            task_id: None,
            role: AgentRole::Coder,
            route: AgentRoute::Acp,
            gate_strength: GateStrength::Full,
            mutation_policy: MutationPolicy::ProposeOnly,
            worktree: Some("/workspace/viden/.worktrees/lane-acp".to_string()),
            branch: Some("viden/lane-acp".to_string()),
            target: ExecutionTarget::Local,
            data_egress: DataEgressPolicy::Deny,
            status: LaneStatus::Done,
            budget: LaneBudget::default(),
            active_session_ids: vec!["session-acp".to_string()],
            summary: "Return an exact response".to_string(),
            evidence: Vec::new(),
            run_stats: None,
        });
        view.agent_sessions.push(AgentSessionView {
            session_id: "session-acp".to_string(),
            lane_id: "lane-acp".to_string(),
            agent_id: "codex-acp".to_string(),
            model: None,
            status: AgentSessionStatus::Completed,
            owner,
            task: "Return an exact response".to_string(),
            diagnostic: None,
            output: Some("ACP-GUI-CLOSED-LOOP-OK".to_string()),
        });
        view.agent_conversation.extend([
            AgentConversationMessageView {
                message_id: "session-acp-message-1".to_string(),
                session_id: "session-acp".to_string(),
                role: AgentConversationRole::User,
                content: "Return an exact response".to_string(),
                parts: Vec::new(),
            },
            AgentConversationMessageView {
                message_id: "session-acp-message-2".to_string(),
                session_id: "session-acp".to_string(),
                role: AgentConversationRole::Assistant,
                content: "ACP-GUI-CLOSED-LOOP-OK".to_string(),
                parts: Vec::new(),
            },
        ]);
        let mut projection = RuntimeProjection::default();
        projection.replace(RuntimeSnapshotEnvelope {
            schema_version: FRONTEND_SCHEMA_V1,
            capabilities: BTreeSet::from([
                CapabilityId(D1_OWNER_CAPABILITY.to_string()),
                CapabilityId(COCKPIT_CONTEXT_CAPABILITY.to_string()),
            ]),
            cursor: EventCursor {
                stream_id: "stream-acp".to_string(),
                sequence: 1,
            },
            snapshot,
            view,
        });

        let cockpit = projection
            .d1_cockpit(Some("lane-acp"))
            .expect("D1 cockpit projection");

        assert_eq!(
            cockpit.agent_sessions[0].output.as_deref(),
            Some("ACP-GUI-CLOSED-LOOP-OK")
        );
        assert_eq!(
            cockpit.agent_sessions[0]
                .conversation
                .iter()
                .map(|message| (message.role, message.content.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("user", "Return an exact response"),
                ("assistant", "ACP-GUI-CLOSED-LOOP-OK"),
            ]
        );
        assert_eq!(
            cockpit
                .context_dock
                .lane_agent
                .as_ref()
                .and_then(|agent| agent.session_id.as_deref()),
            Some("session-acp")
        );
    }
}
