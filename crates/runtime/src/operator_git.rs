//! Operator source-control actions (`runtime.operator_git`, GUI-CORE-020).
//!
//! A DiffReview commit bar needs Core to stage, commit, push, and fetch on the
//! operator's behalf. The tempting shape is a small operator-only git helper.
//! This module is deliberately not that: every action resolves to one of the
//! `git_*` tool specs an *agent* would use and runs through the same
//! `ToolRegistry`, so an operator and an agent produce identical effects,
//! answer to identical `viden.toml` rules, and are stopped by the identical
//! permission backstop. There is exactly one git implementation in Viden.
//!
//! The order of the flow is the contract, and each step exists because
//! skipping it produces a specific lie:
//!
//! 1. **Validate.** A malformed action is refused before a process spawns. A
//!    path that leaves the target is a refusal, never a clamp: clamping stages
//!    a file nobody asked for.
//! 2. **Gate.** `PermissionEngine::decide` on the *mapped* spec. Plan mode and
//!    a deny rule both stop here, before any effect, as `CommandRejected`.
//! 3. **Audit, then effect.** The authorization record is appended *before*
//!    the tool runs and fail-closed: if the audit append fails, the mutation
//!    does not happen, because a source-control change nobody can audit is
//!    worse than a change that did not occur.
//! 4. **Classify, never re-litigate.** Once permission was granted, a failure
//!    is an outcome, not a rejection: the effect *was* attempted. Core turns
//!    git's stderr into one [`OperatorGitFailureClass`] here, once, so no
//!    client ever greps output text for `has no upstream branch`.

use std::path::Path;

use viden_tools::ToolExecutionContext;
use viden_types::{
    AuditActor, AuditObjectRef, AuditOutcome, AuditRecord, MAX_OPERATOR_GIT_OUTPUT_BYTES,
    OperatorGitAction, OperatorGitFailureClass, OperatorGitOutcome, PermissionDecision,
    RuntimeEvent, RuntimeEventKind, RuntimeOwner, SourceTarget, ToolCall, ToolInput, ToolSpec,
    fresh_id, now_timestamp, workspace_owner_authorizes,
};

use crate::SessionEngine;
use crate::frontend_status::sample_workspace_source;
use crate::presentation::render_permission_denial;

/// Audit object kind for a source-control target.
///
/// Named here rather than inlined so the runtime, the fixture, and the
/// documentation cannot drift into three spellings of the same kind.
pub(crate) const AUDIT_KIND_SOURCE: &str = AuditObjectRef::KIND_SOURCE;

/// The tool spec and input one operator action resolves to.
///
/// The mapping is the whole point of the capability: an operator's "Stage all"
/// is an agent's `git_add all=true`, decided under the same rule, executed by
/// the same code.
pub(crate) struct OperatorGitToolCall {
    pub(crate) spec: ToolSpec,
    pub(crate) input: ToolInput,
}

impl SessionEngine {
    /// Resolves, gates, audits, runs, and reports one operator git action.
    ///
    /// `Err` here means the action never ran and becomes `CommandRejected`.
    /// `Ok` always carries an `OperatorGitActionFinished` followed by a
    /// `WorkspaceSourceUpdated`, even when the outcome is `Failed`: a client
    /// that sent a push must learn what happened, and a client that only
    /// tracks the source chip must see the resampled branch either way.
    pub(crate) fn run_operator_git_action<F>(
        &mut self,
        command_id: &str,
        owner: &RuntimeOwner,
        target: SourceTarget,
        action: OperatorGitAction,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> viden_types::ApprovalResponse,
    {
        action.validate()?;
        self.authorize_operator_git_owner(&target, owner)?;
        let root = self.resolve_source_target_root(&target)?;
        let call = operator_git_tool_call(&self.tools, &action, &root)?;

        // The gate runs first: nothing below this point has spawned `git`.
        // Plan mode and a deny rule both land in the `Deny` arm through the
        // engine's own plan-mode branch, so a mutation cannot escape by way of
        // a call site that forgot to check the mode.
        let decision_context = self.operator_git_decision_context(&action, &root);
        let mut gate_permissions = self.permissions.clone();
        let decision = crate::permission_gate::resolve(
            &mut gate_permissions,
            &call.spec,
            &call.spec.name,
            &call.input,
            |_ask, mut prompt| {
                prompt.decision_context = decision_context.clone();
                approver(prompt)
            },
        );
        match decision {
            PermissionDecision::Allow(_) => {}
            PermissionDecision::Deny(deny) => {
                return Err(operator_git_refusal(
                    &call.spec.name,
                    &format!("{:?}", deny.decision_reason),
                    &deny.message,
                ));
            }
            // `resolve` never returns `Ask`: it either consults the approver or
            // denies. Treating a stray one as a grant would be the one failure
            // this whole path exists to prevent.
            PermissionDecision::Ask(ask) => {
                return Err(operator_git_refusal(
                    &call.spec.name,
                    "RequiresApproval",
                    &ask.message,
                ));
            }
        }

        // Audit before mutation, fail-closed. A failed append aborts the
        // action: an unauditable source-control change is worse than none.
        let audit_id = fresh_id("audit");
        let verb = action.audit_verb();
        self.append_trust_audit(&AuditRecord::sanitized(
            audit_id.clone(),
            now_timestamp(),
            owner.clone(),
            AuditActor::Operator,
            format!("source.{verb}"),
            operator_git_audit_objects(&target),
            // What succeeded at this point is the authorization, which is what
            // `phase` names. The audit log is append-only, so the outcome of
            // the effect cannot amend this record; it arrives as the
            // completion record below, which names this one.
            AuditOutcome::Success,
            [
                ("phase".to_string(), "authorized".to_string()),
                ("tool".to_string(), call.spec.name.clone()),
            ]
            .into_iter()
            .collect(),
        )?)?;

        // A push that would create an *untracked* remote branch is refused as a
        // typed failure instead of being run. `git_push` always names the
        // remote and the branch, so git itself would happily create
        // `origin/<branch>` with no tracking configured, and
        // `sample_workspace_source` reads ahead/behind through `@{upstream}` —
        // which would then report `ahead: 0` forever. A chip that says
        // "in sync" about a branch nobody is tracking is exactly the
        // fabricated fact this contract exists to prevent. The client's
        // recovery is to resend the push with `set_upstream: true`, which is
        // what the `NoUpstream` class tells it to offer.
        if let OperatorGitAction::Push {
            set_upstream: false,
            ..
        } = &action
            && !branch_has_upstream(&root)
        {
            return Ok(self.settle_operator_git_action(
                command_id,
                owner,
                target,
                action,
                audit_id,
                &root,
                OperatorGitOutcome::Failed {
                    class: OperatorGitFailureClass::NoUpstream,
                    detail: "the current branch has no upstream branch; push with set_upstream \
                             to create one"
                        .to_string(),
                },
            ));
        }

        let outcome = match self.tools.execute(
            &ToolCall {
                id: fresh_id("tool"),
                name: call.spec.name.clone(),
                input: call.input.clone(),
            },
            // Rooted at the *resolved* target, so a Lane action runs in the
            // Lane's worktree rather than in the workspace the session opened.
            &ToolExecutionContext::local(root.clone()),
        ) {
            Ok(result) => {
                let (output, truncated) = bound_git_output(&result.output);
                OperatorGitOutcome::Completed {
                    output,
                    truncated,
                    // Resampled after the effect, so the chip a client renders
                    // reflects what the action did and not what it predicted.
                    source: sample_workspace_source(&root),
                }
            }
            Err(error) => OperatorGitOutcome::Failed {
                class: classify_operator_git_failure(&error),
                detail: bound_git_output(&error).0,
            },
        };

        Ok(self.settle_operator_git_action(
            command_id, owner, target, action, audit_id, &root, outcome,
        ))
    }

    /// Refuses a workspace-target action whose actor Core cannot name
    /// (`runtime.workspace_owner`, GUI-CORE-027).
    ///
    /// The audit record this action appends *is* the authorization, so the
    /// owner on it has to be a real identity. Before this capability a
    /// workspace-target action had none to offer — `RuntimeOwner::default()`
    /// names nobody — and both clients refused locally rather than file an
    /// authorized source-control change as belonging to no one. Core now
    /// publishes the identity, so the refusal moves here, where it belongs,
    /// and names the contract request so an operator reading it learns which
    /// capability their client is missing rather than only that they were
    /// refused.
    ///
    /// A Lane target is not checked here: its identity is the Lane's own
    /// binding, which the Lane records already validate.
    fn authorize_operator_git_owner(
        &self,
        target: &SourceTarget,
        owner: &RuntimeOwner,
    ) -> Result<(), String> {
        if !matches!(target, SourceTarget::Workspace) {
            return Ok(());
        }
        if workspace_owner_authorizes(self.workspace_owner(), owner) {
            return Ok(());
        }
        Err(format!(
            "a workspace-target source-control action requires the workspace owner Core \
             published (GUI-CORE-027, `runtime.workspace_owner`); the command carried \
             workspace `{}` project `{}`\nhint: read `workspace_owner` from the runtime view \
             and send it as the command's owner",
            redact_id(&owner.workspace_id),
            redact_id(&owner.project_id),
        ))
    }

    /// Records the outcome and publishes the two events every settled action
    /// produces, whichever way it settled.
    ///
    /// Written once and shared by the executed path and the pre-run
    /// `NoUpstream` refusal, so the two cannot drift into publishing different
    /// event shapes for the same kind of answer.
    #[allow(clippy::too_many_arguments)]
    fn settle_operator_git_action(
        &self,
        command_id: &str,
        owner: &RuntimeOwner,
        target: SourceTarget,
        action: OperatorGitAction,
        audit_id: String,
        root: &Path,
        outcome: OperatorGitOutcome,
    ) -> Vec<RuntimeEvent> {
        // The completion record. Best-effort on purpose: the authorization
        // record already landed before the effect, so a failure to append this
        // one must not hide an effect that really happened by turning the whole
        // action into an error. It is a second record rather than an amendment
        // because the audit log is append-only, and it names the authorization
        // through `attempt` so the two read as one story.
        let mut args = vec![("attempt".to_string(), audit_id.clone())];
        let completion_outcome = match &outcome {
            OperatorGitOutcome::Completed { .. } => {
                args.push(("phase".to_string(), "completed".to_string()));
                AuditOutcome::Success
            }
            OperatorGitOutcome::Failed { class, .. } => {
                args.push(("phase".to_string(), "failed".to_string()));
                args.push(("class".to_string(), failure_class_token(*class).to_string()));
                AuditOutcome::Failed
            }
            // An outcome shape this build does not name is recorded as failed
            // rather than as success: an unrecognized settlement is not proof
            // that the action worked.
            _ => {
                args.push(("phase".to_string(), "unknown".to_string()));
                AuditOutcome::Failed
            }
        };
        if let Ok(record) = AuditRecord::sanitized(
            fresh_id("audit"),
            now_timestamp(),
            owner.clone(),
            AuditActor::Operator,
            format!("source.{}", action.audit_verb()),
            operator_git_audit_objects(&target),
            completion_outcome,
            args.into_iter().collect(),
        ) {
            let _ = self.append_trust_audit(&record);
        }

        let source = match &outcome {
            OperatorGitOutcome::Completed { source, .. } => source.clone(),
            // A failure still leaves facts worth resampling — a rejected push
            // leaves `ahead` where it was, a failed commit leaves the tree
            // dirty — and reading them is cheaper and more honest than
            // guessing which of them the failure preserved.
            _ => sample_workspace_source(root),
        };
        // A Lane's resampled source is *that Lane's*, so it is published as a
        // Lane row rather than as the workspace source. Publishing it as
        // `WorkspaceSourceUpdated` — which is what this did before
        // `runtime.workspace_owner` — put a Lane worktree's branch and
        // ahead/behind into the workspace chip, describing one tree with
        // another tree's facts.
        let source_fact = match &target {
            SourceTarget::Lane { lane_id } => RuntimeEventKind::LaneSourceUpdated {
                lane_id: lane_id.clone(),
                source,
            },
            _ => RuntimeEventKind::WorkspaceSourceUpdated { source },
        };
        vec![
            RuntimeEvent::new(
                1,
                RuntimeEventKind::OperatorGitActionFinished {
                    command_id: command_id.to_string(),
                    target,
                    action,
                    outcome,
                    audit_id,
                },
            ),
            RuntimeEvent::new(2, source_fact),
        ]
    }

    /// The staged diff an operator is about to commit, for the approval prompt.
    ///
    /// Only `Commit` gets one, and only from the *index*: that is exactly the
    /// content the commit will contain. A stage or an unstage has no
    /// prospective content of its own to show, and a push or a fetch is a
    /// remote operation whose effect Core cannot compute without running it —
    /// those get `None`, which means "Core previewed nothing", never "this
    /// changes nothing".
    fn operator_git_decision_context(
        &self,
        action: &OperatorGitAction,
        root: &Path,
    ) -> Option<viden_types::DecisionContext> {
        match action {
            OperatorGitAction::Commit { .. } => {
                Some(crate::frontend_services::staged_diff_context(root))
            }
            _ => None,
        }
    }
}

/// Repository-scoped audit objects for one target.
///
/// A Lane target names the Lane as well as the source, so an audit reader can
/// ask "what did this Lane do to its worktree" without joining on paths.
pub(crate) fn operator_git_audit_objects(target: &SourceTarget) -> Vec<AuditObjectRef> {
    match target {
        SourceTarget::Lane { lane_id } => vec![
            AuditObjectRef::new(AUDIT_KIND_SOURCE, lane_id),
            AuditObjectRef::new(AuditObjectRef::KIND_LANE, lane_id),
        ],
        SourceTarget::Workspace => vec![AuditObjectRef::new(AUDIT_KIND_SOURCE, "workspace")],
        _ => vec![AuditObjectRef::new(AUDIT_KIND_SOURCE, "workspace")],
    }
}

/// Stable token for an audit argument. Never localized prose.
fn failure_class_token(class: OperatorGitFailureClass) -> &'static str {
    match class {
        OperatorGitFailureClass::NothingToCommit => "nothing_to_commit",
        OperatorGitFailureClass::NonFastForward => "non_fast_forward",
        OperatorGitFailureClass::AuthenticationRequired => "authentication_required",
        OperatorGitFailureClass::RemoteUnreachable => "remote_unreachable",
        OperatorGitFailureClass::NoUpstream => "no_upstream",
        OperatorGitFailureClass::PathOutsideRepository => "path_outside_repository",
        _ => "other",
    }
}

/// Bounds git output for the wire and says whether the bound cut it.
///
/// Cut on a character boundary: slicing bytes could split a multi-byte
/// character and produce output no client can decode.
fn bound_git_output(raw: &str) -> (String, bool) {
    if raw.len() <= MAX_OPERATOR_GIT_OUTPUT_BYTES {
        return (raw.to_string(), false);
    }
    let mut end = MAX_OPERATOR_GIT_OUTPUT_BYTES;
    while end > 0 && !raw.is_char_boundary(end) {
        end -= 1;
    }
    (raw[..end].to_string(), true)
}

/// Builds the rejection reason for a refused operator action.
///
/// A `CommandRejected` has no `hint` field, so the actionable half is folded
/// into the reason: telling an operator they were refused without telling them
/// what to grant is a worse answer than the one they could act on.
fn operator_git_refusal(tool_name: &str, reason: &str, message: &str) -> String {
    format!(
        "{}\nhint: grant the `{tool_name}` permission to run source-control actions from this \
         client",
        render_permission_denial(tool_name, reason, message)
    )
}

/// Maps one operator action onto the agent tool spec that performs it.
///
/// The specs come from the live registry rather than being retyped here, so a
/// spec whose `is_mutating` flag or name changes cannot leave the operator path
/// gating something different from what it runs.
pub(crate) fn operator_git_tool_call(
    tools: &viden_tools::ToolRegistry,
    action: &OperatorGitAction,
    root: &Path,
) -> Result<OperatorGitToolCall, String> {
    let (name, input) = match action {
        OperatorGitAction::Stage { paths } => {
            let mut input = ToolInput::new();
            if paths.is_empty() {
                input.insert("all".to_string(), "true".to_string());
            } else {
                // `paths` and not `path`: `git_add` reads `path` as *both* the
                // repository probe and a file to stage, so putting the target
                // root there would stage the whole tree beside the selection.
                input.insert("paths".to_string(), paths.join("\n"));
            }
            ("git_add", input)
        }
        OperatorGitAction::Unstage { paths } => {
            let mut input = ToolInput::new();
            input.insert("staged".to_string(), "true".to_string());
            // The working tree is left alone: unstaging must never discard the
            // operator's edits, which `worktree=true` would do.
            input.insert("worktree".to_string(), "false".to_string());
            input.insert(
                "paths".to_string(),
                if paths.is_empty() {
                    ".".to_string()
                } else {
                    paths.join("\n")
                },
            );
            ("git_restore", input)
        }
        OperatorGitAction::Commit { message } => {
            let mut input = ToolInput::new();
            input.insert("message".to_string(), message.clone());
            // Never `all=true`: an operator commits what they staged and saw in
            // the review pane, not whatever else the tree happens to hold.
            input.insert("path".to_string(), root.display().to_string());
            ("git_commit", input)
        }
        OperatorGitAction::Push {
            remote,
            set_upstream,
        } => {
            let mut input = ToolInput::new();
            input.insert(
                "remote".to_string(),
                remote.clone().unwrap_or_else(|| "origin".to_string()),
            );
            if *set_upstream {
                input.insert("set_upstream".to_string(), "true".to_string());
            }
            input.insert("path".to_string(), root.display().to_string());
            ("git_push", input)
        }
        OperatorGitAction::Fetch { remote } => {
            let mut input = ToolInput::new();
            input.insert(
                "remote".to_string(),
                remote.clone().unwrap_or_else(|| "origin".to_string()),
            );
            input.insert("path".to_string(), root.display().to_string());
            ("git_fetch", input)
        }
        _ => {
            return Err("operator git action is not supported by this Core".to_string());
        }
    };
    let spec = tools
        .spec(name)
        .ok_or_else(|| format!("tool `{name}` is not registered in this Core"))?;
    Ok(OperatorGitToolCall { spec, input })
}

/// Turns git's own failure text into one class, in one place.
///
/// This function is the reason no client parses git output. It reads English
/// because that is what git writes on the porcelain paths Viden drives (Core
/// runs `git` without a forced locale), and everything it cannot recognize is
/// [`OperatorGitFailureClass::Other`] with the real text preserved — never the
/// nearest-looking class, which would send a client down the wrong recovery.
///
/// Order matters where messages overlap: a non-fast-forward push also mentions
/// the upstream, so the rejection is tested first.
pub(crate) fn classify_operator_git_failure(stderr: &str) -> OperatorGitFailureClass {
    let text = stderr.to_ascii_lowercase();
    if text.contains("nothing to commit")
        || text.contains("no changes added to commit")
        || text.contains("nothing added to commit")
    {
        return OperatorGitFailureClass::NothingToCommit;
    }
    if text.contains("! [rejected]")
        || text.contains("non-fast-forward")
        || text.contains("fetch first")
        || text.contains("updates were rejected")
    {
        return OperatorGitFailureClass::NonFastForward;
    }
    if text.contains("authentication failed")
        || text.contains("could not read username")
        || text.contains("could not read password")
        || text.contains("permission denied (publickey)")
        || text.contains("invalid username or password")
    {
        return OperatorGitFailureClass::AuthenticationRequired;
    }
    if text.contains("could not resolve host")
        || text.contains("connection refused")
        || text.contains("could not read from remote repository")
        || text.contains("connection timed out")
        // An unconfigured remote *name* is treated by git as a URL, and the
        // message is about the far end rather than about tracking. It is not
        // `NoUpstream`: telling a client to set an upstream would send the
        // operator to fix the wrong thing.
        || text.contains("does not appear to be a git repository")
        || text.contains("no such remote")
    {
        return OperatorGitFailureClass::RemoteUnreachable;
    }
    if text.contains("has no upstream branch")
        || text.contains("no upstream configured")
        || text.contains("no upstream branch")
    {
        return OperatorGitFailureClass::NoUpstream;
    }
    if text.contains("is outside repository")
        || text.contains("path is outside the repository")
        || text.contains("outside repository at")
    {
        return OperatorGitFailureClass::PathOutsideRepository;
    }
    OperatorGitFailureClass::Other
}

/// Whether the checked-out branch has an upstream configured.
///
/// `@{upstream}` is git's own answer to the question, so Core does not
/// reimplement the tracking lookup. A repository where the query fails for any
/// other reason (a detached HEAD, an unborn branch) answers `false`, which
/// routes the operator to the `set_upstream` recovery rather than to a push
/// whose result nothing could describe.
fn branch_has_upstream(root: &Path) -> bool {
    std::process::Command::new("git")
        .args([
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ])
        .current_dir(root)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Renders an identifier for a refusal message without echoing an unbounded
/// client-chosen string back onto the event stream.
///
/// `<none>` rather than an empty string, because "the command named no
/// workspace" is the fact an operator needs, and an empty pair of backticks
/// reads as a rendering bug.
fn redact_id(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "<none>".to_string();
    }
    viden_types::truncate_for_preview(trimmed, 40)
}
