use super::{
    audit_panel::{AUDIT_ROW_WIDTH, audit_row},
    canvas::Frame,
    command_palette::render_command_suggestions,
    conflict_rows::{content_summary_row, detail_rows},
    decision::{
        DecisionPick, MAX_TRUST_TEXT_CHARS, SupervisionTarget, TextRequirement, available_actions,
        conflict_content_target, decision_picks, dormant_gate_count, find_gate, find_review,
        is_dormant_gate, overlay_actions, pending_conflict,
    },
    diff_rows::{
        MAX_APPROVAL_DIFF_ROWS, STRUCTURED_DIFF_CAPABILITY, decision_context_rows,
        has_renderable_diff,
    },
    evidence_panel::{EVIDENCE_ROW_WIDTH, evidence_rows},
    glyphs::Glyph,
    jump::JumpIndex,
    keymap::OverlayKind,
    operator_git::{OPERATOR_GIT_CAPABILITY, action_label_key, operator_git_owner},
    panel::panel,
    pending::SupervisionOutcome,
    preferences::{
        PreferenceValue, SettingsPanel, UI_PREFERENCE_PERSISTENCE_CAPABILITY,
        color_depth_label_key, density_label_key, mode_label_key, motion_label_key, skin_label_key,
    },
    projection::CockpitProjection,
    state::{
        AcpPickerPhase, ConflictDetailTarget, GitPickerPhase, InteractionPanel, TuiState,
        has_active_work,
    },
    text::{truncate, truncate_tail},
};

pub(super) const DEFAULT_APPROVAL_FOCUS: usize = 3;
const APPROVAL_FOCUS_APPLY_ALL: usize = 0;
const APPROVAL_FOCUS_DENY: usize = 1;
const APPROVAL_FOCUS_DIFF: usize = 2;
const APPROVAL_FOCUS_APPROVE: usize = 3;
const GLOBAL_JUMP_VISIBLE_ROWS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ApprovalAction {
    ToggleApplyAll,
    Deny,
    Diff,
    Approve,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AcpPickerRowKind {
    Session {
        session_id: String,
    },
    Adapter {
        agent_id: String,
        startability: viden_core::AgentStartability,
    },
    Disabled,
}

/// What one `/git` row does when it is picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum GitPickerRowKind {
    /// Sends this exact action. `Stage` carries no paths, so Core stages every
    /// changed path — the client never enumerates a working tree it does not
    /// own.
    Send(viden_core::OperatorGitAction),
    /// Opens the one-line message prompt. Nothing is sent yet: Core refuses an
    /// empty commit message, and an empty `git commit` would open an editor in
    /// a non-interactive child.
    Commit,
    /// Rendered, selectable, and inert. Grouping, never hiding: an operator has
    /// to be able to see that the surface exists and why it cannot act.
    Disabled,
    /// Local escape from a stranded correlation, offered only while one action
    /// is in flight and appended last so it never shifts a real row's index.
    /// It sends nothing and settles nothing: Core owns the command and the
    /// effect may still land.
    Dismiss,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GitPickerRow {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) kind: GitPickerRowKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AcpPickerRow {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) kind: AcpPickerRowKind,
}

pub(super) fn render_overlays(frame: &mut Frame, state: &TuiState, _right_rail_width: usize) {
    if let Some(overlay) = state.ui.overlay.as_ref() {
        let title_key = match overlay.kind {
            OverlayKind::GlobalJump => "overlay.title.global_jump",
            OverlayKind::Lane => "overlay.title.lanes",
            OverlayKind::Session => "overlay.title.sessions",
            OverlayKind::NewSession => "overlay.title.new_session",
            OverlayKind::CommandPalette => "overlay.title.commands",
            OverlayKind::Board => "overlay.title.board",
            OverlayKind::Decisions => "overlay.title.decisions",
            OverlayKind::SupervisionDecision => "overlay.title.supervision",
            OverlayKind::AuditTimeline => "overlay.title.audit",
            OverlayKind::EvidenceInspector => "overlay.title.evidence",
            OverlayKind::ConflictContent => "overlay.title.conflict",
            OverlayKind::ContextHelp => "overlay.title.context_help",
            OverlayKind::ExitConfirm => "overlay.title.exit",
            OverlayKind::Approval => "overlay.title.approval",
            OverlayKind::InteractionPanel => "overlay.title.select",
            OverlayKind::ComposerCommands => "overlay.title.commands",
        };
        let title = super::i18n::text(state, title_key);
        let overlay_height = match overlay.kind {
            // The approval panel grows with the hunk rows it renders so the
            // decision context never pushes the approval actions off a short
            // terminal. The growth is bounded by the row cap in `diff_rows`.
            OverlayKind::Approval => 14 + approval_diff_row_budget(state),
            OverlayKind::Decisions
            | OverlayKind::SupervisionDecision
            | OverlayKind::AuditTimeline => 14,
            // The inspector carries a scope row, a filter row, day headers, and
            // a content window, so it needs the conflict modal's height rather
            // than a decision list's.
            OverlayKind::EvidenceInspector => 22,
            // Three labelled sides per rejected hunk need a taller panel than
            // a decision list does.
            OverlayKind::ConflictContent => 22,
            _ => 10,
        };
        let hint = match overlay.kind {
            OverlayKind::GlobalJump => super::i18n::text(state, "overlay.global_hint"),
            OverlayKind::SupervisionDecision => super::i18n::text(state, "supervision.hint"),
            OverlayKind::AuditTimeline => super::i18n::text(state, "audit.hint"),
            OverlayKind::EvidenceInspector => super::i18n::text(state, "evidence.hint"),
            OverlayKind::ConflictContent => super::i18n::text(state, "conflict.hint"),
            _ => super::i18n::text(state, "overlay.close_hint"),
        };
        let block = panel(
            &title,
            global_overlay_rows(state, overlay.kind, &overlay.filter),
            frame.width.min(76),
            overlay_height,
            Some(&hint),
        );
        frame.write_block(
            4,
            frame.width.saturating_sub(frame.width.min(76)) / 2,
            &block,
        );
    } else if let Some(approval) = state.runtime.pending_approvals.first() {
        let target = truncate(&approval.target.display, 56);
        let input = truncate(&approval.input_preview, 56);
        let rows = vec![
            format!("{} · {:?}", approval.title, approval.risk),
            truncate(&approval.message, 68),
            super::i18n::translate(
                state,
                "approval.target_only",
                &[("target", target.as_str())],
            ),
            super::i18n::translate(state, "approval.input", &[("input", input.as_str())]),
            super::i18n::text(state, "approval.pinned"),
        ];
        let title = super::i18n::text(state, "overlay.title.approval");
        let block = panel(
            &title,
            rows,
            frame.width.min(76),
            8,
            Some(&approval.audit_id),
        );
        frame.write_block(
            4,
            frame.width.saturating_sub(frame.width.min(76)) / 2,
            &block,
        );
    } else if let Some(lane) = state
        .ui
        .focused_lane
        .as_ref()
        .and_then(|lane_id| state.runtime.lanes.iter().find(|lane| &lane.id == lane_id))
    {
        let mut rows = vec![
            format!("{} {}", lane.id, lane.role),
            "ROUTE main→side-1".to_string(),
            format!("STATE  {:?}", lane.status),
        ];
        rows.extend(blind_cost_rows(state, lane));
        rows.extend(lane.evidence.iter().cloned());
        rows.push("CONTROL [stop] [tmux] [pty] [send] [inspect]".to_string());
        let title = super::i18n::text(state, "overlay.title.lane_detail");
        let block = panel(&title, rows, frame.width.min(76), 10, Some(&lane.id));
        frame.write_block(
            4,
            frame.width.saturating_sub(frame.width.min(76)) / 2,
            &block,
        );
    } else if state.ui.interaction_panel.is_some() {
        let title_key = match state.ui.interaction_panel.as_ref() {
            Some(InteractionPanel::Settings(_)) => "interaction.settings",
            Some(InteractionPanel::Setup { .. }) => "interaction.setup",
            Some(InteractionPanel::ConnectProvider { .. }) => "interaction.connect_provider",
            Some(InteractionPanel::ModelPicker { .. }) => "interaction.select_model",
            Some(InteractionPanel::ProviderConfig { .. }) => "interaction.select",
            Some(InteractionPanel::AcpPicker { phase, .. }) => match phase {
                AcpPickerPhase::Browse => "interaction.acp",
                AcpPickerPhase::TaskEntry { .. } => "interaction.acp.task",
            },
            Some(InteractionPanel::GitPicker { phase, .. }) => match phase {
                GitPickerPhase::Browse => "interaction.git",
                GitPickerPhase::CommitMessage { .. } => "interaction.git.commit",
            },
            Some(InteractionPanel::NewLaneTask { .. }) => "interaction.native_lane.task",
            None => unreachable!("panel presence checked above"),
        };
        let title = super::i18n::text(state, title_key);
        let overlay_height = if matches!(
            state.ui.interaction_panel,
            Some(InteractionPanel::Settings(_))
        ) {
            16
        } else {
            10
        };
        let block = panel(
            &title,
            interaction_rows(state),
            frame.width.min(72),
            overlay_height,
            None,
        );
        frame.write_block(
            4,
            frame.width.saturating_sub(frame.width.min(72)) / 2,
            &block,
        );
    } else {
        render_command_suggestions(frame, state);
    }
}

/// Cost rows for one lane inspector.
///
/// A route whose `cost_meterability()` is `Blind` runs an external process Core
/// cannot attribute tokens or money to, so this surface never shows an inferred
/// token or dollar figure for it. It shows the blind marker plus exactly the
/// four bounded run facts Core measured — and nothing at all when Core has not
/// published `run_stats`, because an unobserved run is absence, not zero.
/// Metered routes keep their existing cost surface and get no rows here.
fn blind_cost_rows(state: &TuiState, lane: &viden_core::AgentLaneRecord) -> Vec<String> {
    if lane.route.cost_meterability() != viden_types::CostMeterability::Blind {
        return Vec::new();
    }
    let mut rows = vec![super::i18n::text(state, "lane.cost.blind")];
    if let Some(stats) = lane.run_stats.as_ref() {
        let exit = stats
            .last_exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| super::i18n::text(state, "lane.run_stats.exit_unknown"));
        rows.push(super::i18n::translate(
            state,
            "lane.run_stats",
            &[
                ("runs", &stats.run_count.to_string()),
                ("wall", &humanized_wall_time(stats.wall_time_ms)),
                ("diff", &stats.diff_bytes.to_string()),
                ("exit", &exit),
            ],
        ));
    }
    rows
}

/// Renders a Core-measured wall time at the scale an operator reads.
///
/// The measurement itself is never rounded away: sub-second runs keep their
/// millisecond precision, longer runs are shown in seconds with one decimal, and
/// runs past a minute get minutes plus whole seconds. The unit is carried in the
/// value rather than the catalog template because `ms`/`s`/`m` are SI symbols
/// and must not change with the interface locale.
fn humanized_wall_time(milliseconds: u64) -> String {
    if milliseconds < 1_000 {
        return format!("{milliseconds} ms");
    }
    let seconds = milliseconds as f64 / 1_000.0;
    if milliseconds < 60_000 {
        return format!("{seconds:.1} s");
    }
    format!(
        "{}m {}s",
        milliseconds / 60_000,
        (milliseconds % 60_000) / 1_000
    )
}

/// Test-only access to the overlay body so sibling modules can assert on the
/// rendered rows without re-implementing the row builders.
#[cfg(test)]
pub(super) fn overlay_rows_for_test(state: &TuiState, kind: OverlayKind) -> Vec<String> {
    global_overlay_rows(state, kind, "")
}

#[cfg(test)]
pub(super) fn humanized_wall_time_for_test(milliseconds: u64) -> String {
    humanized_wall_time(milliseconds)
}

fn global_overlay_rows(state: &TuiState, kind: OverlayKind, filter: &str) -> Vec<String> {
    let mut rows = match kind {
        OverlayKind::GlobalJump => return global_jump_rows(state, filter),
        // The supervision overlay owns its keys: number keys pick an action and
        // other printable characters keep editing the composer, so it has no
        // text filter to apply here.
        OverlayKind::SupervisionDecision => return supervision_decision_rows(state),
        // The audit overlay is a browsing surface with no text filter: rows are
        // Core records, and printable characters keep editing the composer.
        OverlayKind::AuditTimeline => return audit_timeline_rows(state),
        // The inspector owns its keys the same way: `f` and `r` act on the
        // read, and every other printable character keeps editing the
        // composer, so it has no text filter to apply here.
        OverlayKind::EvidenceInspector => {
            return evidence_rows(state, EVIDENCE_ROW_WIDTH);
        }
        OverlayKind::ConflictContent => return conflict_content_rows(state),
        OverlayKind::Lane => state
            .runtime
            .lanes
            .iter()
            .map(|lane| format!("{}  {}  {:?}", lane.id, lane.role, lane.status))
            .collect(),
        OverlayKind::Board => state
            .runtime
            .tasks
            .iter()
            .map(|task| format!("{}  {}  {}", task.id, task.role, task.status.as_str()))
            .collect(),
        OverlayKind::Decisions => decision_center_rows(state),
        OverlayKind::ContextHelp => [
            "modal.context_help.mode",
            "modal.context_help.lanes",
            "modal.context_help.commands",
            "modal.context_help.exit",
        ]
        .into_iter()
        .map(|key| super::i18n::text(state, key))
        .collect(),
        OverlayKind::ExitConfirm if has_active_work(state) => vec![
            super::i18n::text(state, "modal.exit.active.blocked"),
            super::i18n::text(state, "modal.exit.active.core"),
            super::i18n::text(state, "modal.exit.active.stay"),
        ],
        OverlayKind::ExitConfirm => vec![
            super::i18n::text(state, "modal.exit.idle.ready"),
            super::i18n::text(state, "modal.exit.idle.select"),
        ],
        OverlayKind::Session => state
            .ui
            .focused_lane
            .as_ref()
            .and_then(|lane_id| state.runtime.lanes.iter().find(|lane| &lane.id == lane_id))
            .map(|lane| {
                lane.active_session_ids
                    .iter()
                    .map(|session_id| format!("{}  {}", lane.id, session_id))
                    .collect()
            })
            .unwrap_or_default(),
        OverlayKind::NewSession => vec![super::i18n::text(state, "modal.new_session.pending")],
        OverlayKind::CommandPalette | OverlayKind::ComposerCommands => vec![
            super::i18n::text(state, "modal.command.lanes"),
            super::i18n::text(state, "modal.command.sessions"),
            super::i18n::text(state, "modal.command.board"),
            super::i18n::text(state, "modal.command.decisions"),
        ],
        OverlayKind::Approval => focused_approval_rows(state),
        OverlayKind::InteractionPanel => Vec::new(),
    };
    if !filter.is_empty() {
        let needle = filter.to_ascii_lowercase();
        rows.retain(|row| row.to_ascii_lowercase().contains(&needle));
        rows.insert(0, format!("filter: {filter}"));
    }
    if rows.is_empty() {
        rows.push("No matching items.".to_string());
    }
    rows
}

/// The Decision Center row list.
///
/// The first rows are the pickable decisions, in the same order as
/// [`decision_picks`], so `overlay.selected` indexes one honestly. Everything
/// after them is context — recovery hints, the in-flight command, and Core
/// errors — and carries no action.
fn decision_center_rows(state: &TuiState) -> Vec<String> {
    let projection =
        CockpitProjection::from_with_capabilities(&state.runtime, &state.ui, &state.capabilities);
    let selected = state
        .ui
        .overlay
        .as_ref()
        .filter(|overlay| overlay.kind == OverlayKind::Decisions)
        .map_or(0, |overlay| overlay.selected);
    let picks = decision_picks(&state.runtime, state.supervision.pending().is_some());
    // Dormant gates are ordered last by `decision_picks`. One separator row is
    // rendered before the first of them so the operator can see where the
    // actionable list ends; it carries no action and no pick index, so it
    // cannot shift the selection.
    let dormant = dormant_gate_count(&state.runtime);
    let first_dormant = picks.iter().position(|pick| match pick {
        DecisionPick::Supervision(SupervisionTarget::Gate { gate_id }) => {
            find_gate(&state.runtime, gate_id)
                .is_some_and(|gate| is_dormant_gate(&state.runtime, gate))
        }
        _ => false,
    });
    let mut rows = Vec::new();
    for (index, pick) in picks.iter().enumerate() {
        if first_dormant == Some(index) {
            rows.push(super::i18n::translate(
                state,
                "decisions.row.dormant_separator",
                &[
                    ("glyph", Glyph::Gate.unicode()),
                    ("count", &dormant.to_string()),
                ],
            ));
        }
        let marker = if index == selected { ">" } else { " " };
        rows.push(decision_pick_row(state, &projection, marker, pick));
    }
    rows.extend(projection.recovery_actions.iter().map(|recovery| {
        format!(
            "RECOVERY {} · {} · {}",
            recovery.lane_id.as_deref().unwrap_or("runtime"),
            recovery.reason,
            recovery.action
        )
    }));
    if let Some(command) = projection.pending_command.as_ref() {
        rows.push(format!(
            "COMMAND {} · pending Core fact",
            command.command_id
        ));
    }
    rows.extend(
        projection
            .errors
            .iter()
            .map(|error| format!("ERROR {}", error.message)),
    );
    // Footer: the in-flight supervision decision and what dismissing it does
    // and does not do. Rendered after the picks so it never shifts an index.
    rows.extend(supervision_outcome_row(state));
    if state.supervision.pending().is_some() {
        rows.push(super::i18n::text(state, "supervision.dismiss.hint"));
    }
    rows
}

/// Renders one pickable Decision Center row.
///
/// Status glyphs come from the registered TUI vocabulary only (`⏸` gate, `◌`
/// awaiting, `⚠` conflict); the terminal layer rewrites them for ASCII-only
/// terminals. Nothing here is emoji and nothing is a private symbol.
fn decision_pick_row(
    state: &TuiState,
    projection: &CockpitProjection,
    marker: &str,
    pick: &DecisionPick,
) -> String {
    match pick {
        DecisionPick::Approval { request_id } => {
            let action = projection
                .approval_actions
                .iter()
                .find(|action| &action.request_id == request_id);
            let title = projection
                .approvals
                .iter()
                .find(|request| &request.id == request_id)
                .map_or("approval", |request| request.title.as_str());
            format!(
                "{marker} APPROVAL {request_id} · {title} · {:?} · AUDIT {}",
                action.map(|action| action.expiry),
                action.map_or("-", |action| action.audit_id.as_str())
            )
        }
        DecisionPick::Supervision(SupervisionTarget::Gate { gate_id }) => {
            let gate = projection
                .merge_gates
                .iter()
                .find(|gate| &gate.gate_id == gate_id);
            super::i18n::translate(
                state,
                "decisions.row.gate",
                &[
                    ("marker", marker),
                    ("glyph", Glyph::Gate.unicode()),
                    ("gate", gate_id),
                    ("status", &format!("{:?}", gate.map(|gate| gate.status))),
                    (
                        "decision",
                        &format!("{:?}", gate.and_then(|gate| gate.decision)),
                    ),
                ],
            )
        }
        DecisionPick::Supervision(SupervisionTarget::Review { review_id }) => {
            let review = projection
                .review_requests
                .iter()
                .find(|review| &review.review_id == review_id);
            super::i18n::translate(
                state,
                "decisions.row.review",
                &[
                    ("marker", marker),
                    ("glyph", Glyph::Wait.unicode()),
                    ("review", review_id),
                    ("gate", review.map_or("-", |review| review.gate_id.as_str())),
                    (
                        "requester",
                        review.map_or("-", |review| review.requester_lane_id.as_str()),
                    ),
                    (
                        "reviewer",
                        review.map_or("-", |review| review.reviewer_lane_id.as_str()),
                    ),
                ],
            )
        }
        DecisionPick::DismissSupervision => format!(
            "{marker} {}",
            super::i18n::text(state, "supervision.action.dismiss")
        ),
        DecisionPick::AuditTimeline => format!(
            "{marker} {}",
            super::i18n::text(state, "audit.pick.project")
        ),
        DecisionPick::Supervision(SupervisionTarget::Bounce { gate_id }) => {
            let bounce = projection
                .conflict_bounces
                .iter()
                .find(|bounce| &bounce.gate_id == gate_id);
            let mut row = super::i18n::translate(
                state,
                "decisions.row.conflict",
                &[
                    ("marker", marker),
                    ("glyph", Glyph::Warning.unicode()),
                    (
                        "bounce",
                        bounce.map_or("-", |bounce| bounce.bounce_id.as_str()),
                    ),
                    ("gate", gate_id),
                    (
                        "lane",
                        bounce.map_or("-", |bounce| bounce.original_lane_id.as_str()),
                    ),
                ],
            );
            // Appended, never substituted: a bounce with no content keeps
            // exactly the reason-only row it had before this capability.
            if let Some(summary) =
                content_summary_row(state, bounce.and_then(|bounce| bounce.content))
            {
                row.push_str(&format!(" · {summary}"));
            }
            row
        }
    }
}

/// The supervision decision overlay body.
///
/// Every row is derived from the full Core record re-read on this frame, not
/// from the compact Decision Center row that opened the overlay. An action list
/// that comes back empty is rendered as an explicit sentence rather than a blank
/// panel, so "no decision applies" never looks like a failed render.
fn supervision_decision_rows(state: &TuiState) -> Vec<String> {
    let Some(panel) = state.ui.supervision.as_ref() else {
        return Vec::new();
    };
    let mut rows = supervision_target_rows(state, &panel.target);
    let has_pending_command = state.supervision.pending().is_some();
    // "No decision applies" is a statement about decisions, so it is computed
    // from the decision list; the always-present audit row must not silence it.
    if available_actions(&state.runtime, &panel.target, has_pending_command).is_empty() {
        rows.push(super::i18n::text(state, "supervision.no_actions"));
    }
    let actions = overlay_actions(&state.runtime, &panel.target, has_pending_command);
    for (index, action) in actions.iter().enumerate() {
        let marker = if index == panel.focus { ">" } else { " " };
        rows.push(format!(
            "{marker} {} {}",
            index + 1,
            super::i18n::text(state, action.label_key())
        ));
        if action.is_irreversible() {
            rows.push(super::i18n::translate(
                state,
                "supervision.irreversible",
                &[("glyph", Glyph::Warning.unicode())],
            ));
        }
    }
    if let Some(input) = panel.input.as_ref() {
        let key = match input.action.text_requirement() {
            TextRequirement::Required => "supervision.input.required",
            TextRequirement::Optional | TextRequirement::None => "supervision.input.optional",
        };
        rows.push(super::i18n::translate(
            state,
            key,
            &[
                ("count", &input.text.chars().count().to_string()),
                ("limit", &MAX_TRUST_TEXT_CHARS.to_string()),
            ],
        ));
        rows.push(format!("> {}", input.text));
    }
    if let Some(notice) = panel.notice.as_ref() {
        rows.push(super::i18n::translate(
            state,
            notice,
            &[("limit", &MAX_TRUST_TEXT_CHARS.to_string())],
        ));
    }
    rows.extend(supervision_outcome_row(state));
    if state.supervision.pending().is_some() {
        rows.push(super::i18n::text(state, "supervision.dismiss.hint"));
    }
    rows
}

/// The read-only conflict content body.
///
/// Re-read from the full Core record on this frame, like the supervision rows
/// beside it, so the modal can never render a payload the view has since
/// replaced.
fn conflict_content_rows(state: &TuiState) -> Vec<String> {
    let Some(target) = state.ui.conflict_detail.as_ref() else {
        return Vec::new();
    };
    let content = match target {
        ConflictDetailTarget::Bounce { gate_id } => conflict_content_target(
            &state.runtime,
            &SupervisionTarget::Bounce {
                gate_id: gate_id.clone(),
            },
        ),
        ConflictDetailTarget::Lane { lane_id } => state
            .runtime
            .lane_conflicts
            .iter()
            .find(|conflict| &conflict.lane_id == lane_id)
            .and_then(|conflict| conflict.content.as_ref()),
    };
    detail_rows(state, content, CONFLICT_ROW_WIDTH)
}

/// Columns the conflict panel gives one row, inside its border: the panel is
/// capped at 76 and `bordered_row` spends four of them on the frame.
const CONFLICT_ROW_WIDTH: usize = 72;

fn supervision_target_rows(state: &TuiState, target: &SupervisionTarget) -> Vec<String> {
    match target {
        SupervisionTarget::Gate { gate_id } | SupervisionTarget::Bounce { gate_id } => {
            let Some(gate) = find_gate(&state.runtime, gate_id) else {
                return vec![super::i18n::text(state, "supervision.error.record_missing")];
            };
            let conflict = pending_conflict(&state.runtime, gate);
            vec![
                super::i18n::translate(
                    state,
                    "supervision.header.gate",
                    &[
                        ("glyph", Glyph::Gate.unicode()),
                        ("gate", &gate.gate_id),
                        ("status", &format!("{:?}", gate.status)),
                        ("task", &gate.task_id),
                    ],
                ),
                super::i18n::translate(
                    state,
                    "supervision.context.gate",
                    &[
                        ("evidence", &gate.evidence_ids.len().to_string()),
                        (
                            "validator",
                            gate.validator
                                .as_ref()
                                .map_or("-", |validator| validator.review_request_id.as_str()),
                        ),
                        (
                            "conflict",
                            conflict.map_or("-", |conflict| conflict.bounce_id.as_str()),
                        ),
                    ],
                ),
            ]
        }
        SupervisionTarget::Review { review_id } => {
            let Some(review) = find_review(&state.runtime, review_id) else {
                return vec![super::i18n::text(state, "supervision.error.record_missing")];
            };
            vec![
                super::i18n::translate(
                    state,
                    "supervision.header.review",
                    &[
                        ("glyph", Glyph::Wait.unicode()),
                        ("review", &review.review_id),
                        ("gate", &review.gate_id),
                        ("requester", &review.requester_lane_id),
                        ("reviewer", &review.reviewer_lane_id),
                        ("status", &format!("{:?}", review.status)),
                    ],
                ),
                super::i18n::translate(
                    state,
                    "supervision.context.review",
                    &[
                        ("evidence", &review.evidence_ids.len().to_string()),
                        ("audit", &review.audit_id),
                    ],
                ),
            ]
        }
    }
}

/// The outcome echo: pending, confirmed, or Core's own rejection reason.
///
/// The reason is rendered verbatim inside a localized frame. It is Core's
/// sentence, so it is never rewritten, re-worded, or replaced with a local guess.
pub(super) fn supervision_outcome_row(state: &TuiState) -> Option<String> {
    match state.supervision.outcome() {
        SupervisionOutcome::Idle => None,
        SupervisionOutcome::Pending { command_id } => Some(super::i18n::translate(
            state,
            "supervision.outcome.pending",
            &[("glyph", Glyph::Wait.unicode()), ("command", command_id)],
        )),
        SupervisionOutcome::Confirmed => Some(super::i18n::translate(
            state,
            "supervision.outcome.confirmed",
            &[("glyph", Glyph::Done.unicode())],
        )),
        SupervisionOutcome::Rejected { reason } => Some(super::i18n::translate(
            state,
            "supervision.outcome.rejected",
            &[("glyph", Glyph::Fail.unicode()), ("reason", reason)],
        )),
    }
}

/// The audit timeline overlay body.
///
/// Four states are kept visibly distinct, because collapsing them would let the
/// operator read absence as a fact: *loading* (a query is in flight and nothing
/// has arrived), *empty* (Core answered with an empty page), *error* (Core
/// rejected the query, rendered with its own reason verbatim), and the loaded
/// list. The footer always states how many records are loaded and whether older
/// ones remain, so a short list is never mistaken for the whole timeline.
fn audit_timeline_rows(state: &TuiState) -> Vec<String> {
    let Some(panel) = state.ui.audit.as_ref() else {
        return Vec::new();
    };
    let mut rows = vec![match panel.scope() {
        Some(scope) => super::i18n::translate(
            state,
            "audit.scope.object",
            &[("kind", &scope.kind), ("id", &scope.id)],
        ),
        None => super::i18n::text(state, "audit.scope.project"),
    }];
    if let Some(reason) = panel.error() {
        rows.push(super::i18n::translate(
            state,
            "audit.error",
            &[("glyph", Glyph::Fail.unicode()), ("reason", reason)],
        ));
    }
    if panel.is_loading() && panel.records().is_empty() {
        rows.push(super::i18n::translate(
            state,
            "audit.loading",
            &[("glyph", Glyph::Wait.unicode())],
        ));
    } else if panel.is_empty_result() {
        rows.push(super::i18n::text(state, "audit.empty"));
    }
    let selected = panel.selected();
    rows.extend(panel.records().iter().enumerate().map(|(index, record)| {
        let marker = if index == selected { ">" } else { " " };
        format!("{marker} {}", audit_row(record, AUDIT_ROW_WIDTH))
    }));
    if panel.shows_load_older_row() {
        let marker = if panel.selected_is_load_older() {
            ">"
        } else {
            " "
        };
        rows.push(format!(
            "{marker} {}",
            super::i18n::text(state, "audit.load_older")
        ));
    }
    if let Some(notice) = panel.notice() {
        rows.push(super::i18n::text(state, notice));
    }
    rows.push(super::i18n::translate(
        state,
        if panel.is_complete() {
            "audit.footer.complete"
        } else {
            "audit.footer.more"
        },
        &[("count", &panel.records().len().to_string())],
    ));
    rows
}

fn global_jump_rows(state: &TuiState, filter: &str) -> Vec<String> {
    let index = JumpIndex::from_state(state);
    let mut rows = Vec::new();
    let mut previous_kind = None;
    for (position, item) in index.search(filter).into_iter().enumerate() {
        if previous_kind != Some(item.kind) {
            rows.push((format!("[{}]", item.kind.label()), None, item.kind));
            previous_kind = Some(item.kind);
        }
        let marker = state
            .ui
            .overlay
            .as_ref()
            .is_some_and(|overlay| overlay.selected == position);
        let detail = item.disabled_reason.as_deref().unwrap_or(&item.context);
        rows.push((
            format!(
                "{} {:<16} {}{}",
                if marker { ">" } else { " " },
                // An id or path differs at its END, so a head cut renders
                // every `.github/workflows/*` file and every
                // `gate-acp-session-…` gate as the same row.
                if item.tail_distinctive {
                    truncate_tail(&item.title, 16)
                } else {
                    truncate(&item.title, 16)
                },
                if item.tail_distinctive {
                    truncate_tail(detail, 42)
                } else {
                    truncate(detail, 42)
                },
                if item.enabled { "" } else { " · unavailable" },
            ),
            Some(position),
            item.kind,
        ));
    }
    if rows.is_empty() {
        return vec!["No matching items. Try : @ # > or ~.".to_string()];
    }
    let selected = state
        .ui
        .overlay
        .as_ref()
        .map_or(0, |overlay| overlay.selected);
    let selected_row = rows
        .iter()
        .position(|(_, result_index, _)| *result_index == Some(selected))
        .unwrap_or(0);
    let start = selected_row
        .saturating_sub(GLOBAL_JUMP_VISIBLE_ROWS / 2)
        .min(rows.len().saturating_sub(GLOBAL_JUMP_VISIBLE_ROWS));
    let mut visible = rows
        .iter()
        .skip(start)
        .take(GLOBAL_JUMP_VISIBLE_ROWS)
        .map(|(text, _, _)| text.clone())
        .collect::<Vec<_>>();
    if start > 0 && rows[start].1.is_some() && rows[start - 1].2 == rows[start].2 {
        visible.insert(0, format!("[{}] · continued", rows[start].2.label()));
        if visible
            .iter()
            .position(|row| row.starts_with("> "))
            .is_some_and(|position| position >= GLOBAL_JUMP_VISIBLE_ROWS)
        {
            visible.remove(1);
        }
        visible.truncate(GLOBAL_JUMP_VISIBLE_ROWS);
    }
    visible
}

pub(super) fn interaction_rows(state: &TuiState) -> Vec<String> {
    match state.ui.interaction_panel.as_ref() {
        Some(InteractionPanel::Settings(panel)) => settings_rows(state, panel),
        Some(InteractionPanel::Setup { selected, draft }) => {
            if !state.has_capability("runtime.project_onboarding") {
                return vec![super::i18n::text(state, "interaction.setup.unavailable")];
            }
            let mut rows = vec![
                setup_action_row(*selected, 0, "Probe project through Core"),
                setup_action_row(*selected, 1, "Preview exact draft through Core"),
            ];
            if state
                .runtime
                .project_config_preview
                .as_ref()
                .is_some_and(|preview| setup_preview_matches_draft(preview, draft))
            {
                let preview = state.runtime.project_config_preview.as_ref().unwrap();
                rows.push(setup_action_row(
                    *selected,
                    2,
                    &format!("Confirm {} through Core", preview.relative_path),
                ));
            }
            rows.push("DRAFT viden.toml (paste/type to edit)".to_string());
            rows.extend(draft.lines().map(|line| format!("  {line}")));
            rows
        }
        Some(InteractionPanel::ConnectProvider { search, .. }) => state
            .ui
            .provider_catalog
            .iter()
            .filter(|provider| {
                provider.provider_id.contains(search)
                    || provider
                        .display_name
                        .to_ascii_lowercase()
                        .contains(&search.to_ascii_lowercase())
            })
            .map(|provider| format!("{}  {}", provider.provider_id, provider.display_name))
            .collect(),
        Some(InteractionPanel::ProviderConfig { provider_id, .. }) => vec![
            format!("configure {provider_id}"),
            "credential handles are read-only".to_string(),
            "trusted ingress unavailable".to_string(),
        ],
        Some(InteractionPanel::ModelPicker {
            provider_id,
            search,
            ..
        }) => state
            .ui
            .provider_catalog
            .iter()
            .filter(|provider| !provider.enabled_models.is_empty())
            .filter(|provider| {
                provider_id
                    .as_ref()
                    .is_none_or(|id| id == &provider.provider_id)
            })
            .flat_map(|provider| {
                provider
                    .enabled_models
                    .iter()
                    .filter(|model| model.contains(search))
                    .map(|model| {
                        format!(
                            "{}  {model}  {}",
                            provider.provider_id, provider.display_name
                        )
                    })
            })
            .collect(),
        Some(InteractionPanel::AcpPicker { selected, phase }) => match phase {
            AcpPickerPhase::Browse => acp_picker_rows(state)
                .into_iter()
                .enumerate()
                .map(|(index, row)| {
                    format!(
                        "{} {}",
                        if index == *selected { ">" } else { " " },
                        row.label
                    )
                })
                .collect(),
            AcpPickerPhase::TaskEntry { agent_id, draft } => vec![
                super::i18n::translate(state, "acp.task.agent", &[("agent", agent_id)]),
                super::i18n::text(state, "acp.task.prompt"),
                format!("> {draft}"),
            ],
        },
        Some(InteractionPanel::GitPicker { selected, phase }) => match phase {
            GitPickerPhase::Browse => {
                let mut rows = git_picker_rows(state)
                    .into_iter()
                    .enumerate()
                    .map(|(index, row)| {
                        format!(
                            "{} {}",
                            if index == *selected { ">" } else { " " },
                            row.label
                        )
                    })
                    .collect::<Vec<_>>();
                // Appended *after* every pickable row, so it can never shift
                // the index a keyboard or mouse selection resolves to.
                rows.push(git_target_row(state));
                rows
            }
            GitPickerPhase::CommitMessage { draft } => vec![
                git_target_row(state),
                super::i18n::text(state, "git.commit.prompt"),
                format!("> {draft}"),
            ],
        },
        Some(InteractionPanel::NewLaneTask { task }) => {
            let eligibility = state.runtime.workspace_eligibility.as_ref();
            let status = match eligibility {
                Some(value)
                    if value.can_create_lane && !(value.is_git_repository && value.has_head) =>
                {
                    super::i18n::text(state, "native_lane.direct_workspace")
                }
                Some(value) if value.can_create_lane => {
                    super::i18n::text(state, "native_lane.eligible")
                }
                Some(value) => value
                    .diagnostic
                    .clone()
                    .unwrap_or_else(|| super::i18n::text(state, "native_lane.ineligible")),
                None => super::i18n::text(state, "native_lane.unknown"),
            };
            vec![
                status,
                super::i18n::text(state, "native_lane.task.prompt"),
                format!("> {task}"),
            ]
        }
        None => Vec::new(),
    }
}

/// The Core source-control target this client's `/git` rows act on.
///
/// A focused Lane names that Lane; anything else is the workspace. The client
/// passes the *target*, never a path: Core validates the Lane and resolves its
/// worktree from its own records, so a stale or archived Lane comes back as a
/// refusal instead of quietly acting on the workspace root.
pub(super) fn git_target(state: &TuiState) -> viden_core::SourceTarget {
    match state.ui.focused_lane.as_deref() {
        Some(lane_id) if state.runtime.lanes.iter().any(|lane| lane.id == lane_id) => {
            viden_core::SourceTarget::Lane {
                lane_id: lane_id.to_string(),
            }
        }
        _ => viden_core::SourceTarget::Workspace,
    }
}

/// The context row under the `/git` choices: which tree, and what Core last
/// said about it.
///
/// The source facts are the ones Core published; nothing here is sampled or
/// derived locally, and an unpublished source renders as unknown rather than as
/// a clean tree.
fn git_target_row(state: &TuiState) -> String {
    let target = match git_target(state) {
        viden_core::SourceTarget::Lane { lane_id } => lane_id,
        _ => super::i18n::text(state, "git.target.workspace"),
    };
    let Some(source) = state.runtime.workspace_source.as_ref() else {
        return super::i18n::translate(state, "git.source.unknown", &[("target", &target)]);
    };
    super::i18n::translate(
        state,
        "git.source",
        &[
            ("target", &target),
            (
                "branch",
                source
                    .branch
                    .as_deref()
                    .unwrap_or(&super::i18n::text(state, "git.source.no_branch")),
            ),
            ("ahead", &source.ahead.to_string()),
            ("behind", &source.behind.to_string()),
            (
                "dirty",
                &super::i18n::text(
                    state,
                    if source.dirty {
                        "git.source.dirty"
                    } else {
                        "git.source.clean"
                    },
                ),
            ),
        ],
    )
}

/// The four operator source-control rows, in the fixed order Stage, Commit,
/// Push, Fetch.
///
/// A row is pickable only when Core published both the capability *and* an
/// owner for this target; otherwise it stays listed and is rendered disabled
/// with the reason, because a client that hid them would tell an operator this
/// surface does not exist. Either way the client does not drive `git` itself
/// and does not fall back to a shell.
///
/// The missing capability is named first when both are missing: it is the more
/// fundamental fact, and an owner cannot matter for a command that is not
/// published at all.
pub(super) fn git_picker_rows(state: &TuiState) -> Vec<GitPickerRow> {
    let available = state.has_capability(OPERATOR_GIT_CAPABILITY);
    let unavailable = super::i18n::translate(
        state,
        "git.unavailable",
        &[("capability", OPERATOR_GIT_CAPABILITY)],
    );
    // Read once, from the same resolver the send path uses, so a row is never
    // offered as pickable and then refused after the operator picked it.
    let owner_refusal = operator_git_owner(state, &git_target(state)).err();
    let reason = if available {
        owner_refusal
            .as_ref()
            .map(|refusal| refusal.row_label(state))
    } else {
        Some(unavailable)
    };
    [
        (
            "stage",
            GitPickerRowKind::Send(viden_core::OperatorGitAction::Stage { paths: Vec::new() }),
        ),
        ("commit", GitPickerRowKind::Commit),
        (
            "push",
            GitPickerRowKind::Send(viden_core::OperatorGitAction::Push {
                remote: None,
                set_upstream: false,
            }),
        ),
        (
            "fetch",
            GitPickerRowKind::Send(viden_core::OperatorGitAction::Fetch { remote: None }),
        ),
    ]
    .into_iter()
    .map(|(id, kind)| {
        // A picker row says what picking it *does* — it stages everything, or
        // it opens a prompt. An outcome entry names the same action as a plain
        // verb, because "Stage all changes completed" reads as a sentence
        // fragment rather than a settled fact.
        let label_key = match &kind {
            GitPickerRowKind::Send(viden_core::OperatorGitAction::Stage { .. }) => {
                "git.action.stage_all"
            }
            GitPickerRowKind::Send(viden_core::OperatorGitAction::Unstage { .. }) => {
                "git.action.unstage_all"
            }
            GitPickerRowKind::Send(action) => action_label_key(action),
            GitPickerRowKind::Commit => "git.action.commit_prompt",
            GitPickerRowKind::Disabled | GitPickerRowKind::Dismiss => "git.unavailable",
        };
        let label = super::i18n::text(state, label_key);
        GitPickerRow {
            id: format!("git:{id}"),
            label: match reason.as_ref() {
                Some(reason) => format!("{label} · {reason}"),
                None => label,
            },
            kind: match reason.as_ref() {
                Some(_) => GitPickerRowKind::Disabled,
                None => kind,
            },
        }
    })
    .chain(state.operator_git.pending().map(|pending| GitPickerRow {
        id: "git:dismiss".to_string(),
        label: super::i18n::translate(
            state,
            "git.action.dismiss",
            &[("command_id", &pending.command_id)],
        ),
        kind: GitPickerRowKind::Dismiss,
    }))
    .collect()
}

pub(super) fn acp_picker_rows(state: &TuiState) -> Vec<AcpPickerRow> {
    let Some(lane_id) = state.ui.focused_lane.as_deref() else {
        return vec![AcpPickerRow {
            id: "disabled:no-lane".to_string(),
            label: super::i18n::text(state, "acp.no_lane"),
            kind: AcpPickerRowKind::Disabled,
        }];
    };
    let mut rows = state
        .runtime
        .agent_sessions
        .iter()
        .filter(|session| session.lane_id == lane_id)
        .map(|session| {
            let retry = matches!(
                session.status,
                viden_core::AgentSessionStatus::Failed | viden_core::AgentSessionStatus::Cancelled
            )
            .then(|| format!(" · {}", super::i18n::text(state, "acp.retry_hint")))
            .unwrap_or_default();
            AcpPickerRow {
                id: format!("session:{}", session.session_id),
                label: format!(
                    "{} · {} · {:?}{retry}",
                    session.agent_id, session.task, session.status
                ),
                kind: AcpPickerRowKind::Session {
                    session_id: session.session_id.clone(),
                },
            }
        })
        .collect::<Vec<_>>();
    rows.extend(
        state
            .runtime
            .agent_adapters
            .iter()
            .filter(|adapter| adapter.route == viden_core::AgentRoute::Acp)
            .map(|adapter| {
                let status_key = match adapter.startability {
                    viden_core::AgentStartability::Ready => "acp.status.ready",
                    viden_core::AgentStartability::ProbeRequired => "acp.status.probe_required",
                    viden_core::AgentStartability::InstallRequired => {
                        "acp.status.installation_required"
                    }
                    viden_core::AgentStartability::AuthenticationRequired => {
                        "acp.status.authentication_required"
                    }
                    viden_core::AgentStartability::Unavailable => "acp.status.unavailable",
                };
                AcpPickerRow {
                    id: format!("adapter:{}", adapter.agent_id),
                    label: format!(
                        "{} · {}",
                        adapter.display_name,
                        super::i18n::text(state, status_key)
                    ),
                    kind: AcpPickerRowKind::Adapter {
                        agent_id: adapter.agent_id.clone(),
                        startability: adapter.startability,
                    },
                }
            }),
    );
    if rows.is_empty() {
        rows.push(AcpPickerRow {
            id: "disabled:no-adapters".to_string(),
            label: super::i18n::text(state, "acp.no_adapters"),
            kind: AcpPickerRowKind::Disabled,
        });
    }
    rows
}

fn settings_rows(state: &TuiState, panel: &SettingsPanel) -> Vec<String> {
    if !state.has_capability(UI_PREFERENCE_PERSISTENCE_CAPABILITY) {
        return vec![super::i18n::text(state, "settings.unavailable")];
    }
    let mut rows = if let Some(field) = panel.field {
        panel
            .choices(field)
            .into_iter()
            .enumerate()
            .map(|(index, choice)| {
                let marker = if index == panel.selected { ">" } else { " " };
                let current = if preference_value_is_current(panel, choice.value) {
                    super::i18n::text(state, "settings.current")
                } else {
                    String::new()
                };
                let effect = super::i18n::text(state, choice.effect_key);
                let invalid = choice
                    .invalid_reason_key
                    .map(|key| format!(" · {}", super::i18n::text(state, key)))
                    .unwrap_or_default();
                let disabled = if choice.enabled {
                    String::new()
                } else {
                    format!(" {}", super::i18n::text(state, "settings.label.disabled"))
                };
                format!(
                    "{marker} {} · {current} · {}: {effect}{disabled}{invalid}",
                    super::i18n::text(state, choice.label_key),
                    super::i18n::text(state, "settings.label.effect")
                )
            })
            .collect::<Vec<_>>()
    } else {
        let categories = [
            (
                "settings.field.locale",
                preference_value_label(state, PreferenceValue::Locale(panel.selected_locale())),
                "settings.effect.locale",
            ),
            (
                "settings.field.skin",
                preference_value_label(state, PreferenceValue::Skin(panel.selected_skin())),
                "settings.effect.skin",
            ),
            (
                "settings.field.mode",
                preference_value_label(state, PreferenceValue::Mode(panel.selected_mode())),
                "settings.effect.mode",
            ),
            (
                "settings.field.density",
                preference_value_label(state, PreferenceValue::Density(panel.selected_density())),
                "settings.effect.density",
            ),
            (
                "settings.field.motion",
                preference_value_label(state, PreferenceValue::Motion(panel.selected_motion())),
                "settings.effect.motion",
            ),
            (
                "settings.field.color_depth",
                preference_value_label(state, PreferenceValue::ColorDepth(panel.color_depth())),
                "settings.effect.color_depth",
            ),
            (
                "settings.action.apply",
                super::i18n::text(state, "settings.value.draft"),
                "settings.effect.apply",
            ),
            (
                "settings.action.reset",
                super::i18n::text(state, "settings.value.core_default"),
                "settings.effect.reset",
            ),
        ];
        categories
            .into_iter()
            .enumerate()
            .map(|(index, (key, current, effect_key))| {
                let marker = if index == panel.selected { ">" } else { " " };
                format!(
                    "{marker} {} · {}: {current} · {}: {}",
                    super::i18n::text(state, key),
                    super::i18n::text(state, "settings.label.current"),
                    super::i18n::text(state, "settings.label.effect"),
                    super::i18n::text(state, effect_key)
                )
            })
            .collect::<Vec<_>>()
    };
    if panel.is_pending() {
        rows.push(super::i18n::text(state, "settings.pending"));
    } else if let Some(reason) = panel.rejection_reason() {
        rows.push(super::i18n::translate(
            state,
            "settings.rejected",
            &[("reason", reason)],
        ));
    } else if panel.has_succeeded() {
        rows.push(super::i18n::text(state, "settings.saved"));
    }
    if !panel.diagnostics().is_empty() {
        rows.push(super::i18n::translate(
            state,
            "settings.diagnostics",
            &[("diagnostics", &panel.diagnostics().join(", "))],
        ));
    }
    rows
}

fn preference_value_label(state: &TuiState, value: PreferenceValue) -> String {
    let key = match value {
        PreferenceValue::Locale(viden_core::LocaleId::System) => "settings.value.system",
        PreferenceValue::Locale(viden_core::LocaleId::En) => "settings.value.en",
        PreferenceValue::Locale(viden_core::LocaleId::ZhCn) => "settings.value.zh_cn",
        PreferenceValue::Skin(value) => skin_label_key(value),
        PreferenceValue::Mode(value) => mode_label_key(value),
        PreferenceValue::Density(value) => density_label_key(value),
        PreferenceValue::Motion(value) => motion_label_key(value),
        PreferenceValue::ColorDepth(value) => color_depth_label_key(value),
    };
    super::i18n::text(state, key)
}

fn preference_value_is_current(panel: &SettingsPanel, value: PreferenceValue) -> bool {
    match value {
        PreferenceValue::Locale(value) => panel.selected_locale() == value,
        PreferenceValue::Skin(value) => panel.selected_skin() == value,
        PreferenceValue::Mode(value) => panel.selected_mode() == value,
        PreferenceValue::Density(value) => panel.selected_density() == value,
        PreferenceValue::Motion(value) => panel.selected_motion() == value,
        PreferenceValue::ColorDepth(value) => panel.color_depth() == value,
    }
}

fn setup_action_row(selected: usize, index: usize, label: &str) -> String {
    format!("{} {label}", if selected == index { ">" } else { " " })
}

fn focused_approval_rows(state: &TuiState) -> Vec<String> {
    focused_approval_request(state).map_or_else(Vec::new, |approval| {
        let once = approval
            .allowed_scopes
            .iter()
            .any(|scope| matches!(scope, viden_core::ApprovalScope::Once));
        let session = approval
            .allowed_scopes
            .iter()
            .any(|scope| matches!(scope, viden_core::ApprovalScope::Session { .. }));
        let repo = approval
            .allowed_scopes
            .iter()
            .any(|scope| matches!(scope, viden_core::ApprovalScope::RepoAllowlist { .. }));
        let availability = |available: bool| {
            if available {
                String::new()
            } else {
                super::i18n::text(state, "approval.scope_unavailable")
            }
        };
        let expiry = if approval_is_expired(approval) {
            super::i18n::text(state, "approval.expiry.core")
        } else if approval.expires_at > 0 {
            let expires_at = approval.expires_at.to_string();
            super::i18n::translate(
                state,
                "approval.expiry.auto_deny",
                &[("expires_at", expires_at.as_str())],
            )
        } else {
            super::i18n::text(state, "approval.expiry.none")
        };
        let target = truncate(&approval.target.display, 56);
        let input = truncate(&approval.input_preview, 56);
        let once_availability = availability(once);
        let session_availability = availability(session);
        let repo_availability = availability(repo);
        // Core's typed rows replace the preview string rather than joining it:
        // the preview is a lossy rendering of the same proposal, and showing
        // both would invite a reader to compare two spellings of one fact.
        let context = approval.decision_context.as_ref();
        let change_rows = if has_renderable_diff(state, context) {
            decision_context_rows(state, context, APPROVAL_ROW_WIDTH)
        } else {
            vec![super::i18n::translate(
                state,
                "approval.input",
                &[("input", input.as_str())],
            )]
        };
        let mut rows = vec![
            format!("{} · {:?}", approval.title, approval.risk),
            truncate(&approval.message, 68),
            super::i18n::translate(
                state,
                "approval.target_only",
                &[("target", target.as_str())],
            ),
        ];
        rows.extend(change_rows);
        rows.extend([
            super::i18n::translate(
                state,
                "approval.action.allow_once",
                &[("availability", once_availability.as_str())],
            ),
            super::i18n::translate(
                state,
                "approval.action.allow_session",
                &[("availability", session_availability.as_str())],
            ),
            super::i18n::translate(
                state,
                "approval.action.allow_repo",
                &[("availability", repo_availability.as_str())],
            ),
            super::i18n::text(state, "approval.action.deny"),
            expiry,
            super::i18n::translate(
                state,
                "approval.audit",
                &[("audit_id", approval.audit_id.as_str())],
            ),
        ]);
        // Appended last, so it can never shift the index of a row above it.
        // A client without the extension must say that hunk rows are
        // unavailable rather than let the preview text stand for the change.
        if !state.has_capability(STRUCTURED_DIFF_CAPABILITY) {
            rows.push(super::i18n::translate(
                state,
                "approval.diff.unavailable",
                &[("capability", STRUCTURED_DIFF_CAPABILITY)],
            ));
        }
        rows
    })
}

/// Columns the approval panel gives one row, inside its border: the panel is
/// capped at 76 and `bordered_row` spends four of them on the frame.
const APPROVAL_ROW_WIDTH: usize = 72;

/// Extra panel rows the focused approval's decision context needs.
///
/// Zero when there is none, so an approval without hunks renders exactly the
/// panel it did before this capability existed.
fn approval_diff_row_budget(state: &TuiState) -> usize {
    let Some(approval) = focused_approval_request(state) else {
        return 0;
    };
    let context = approval.decision_context.as_ref();
    if !has_renderable_diff(state, context) {
        return 0;
    }
    // One row of the budget already existed as the preview line the hunks
    // replace.
    decision_context_rows(state, context, APPROVAL_ROW_WIDTH)
        .len()
        .min(MAX_APPROVAL_DIFF_ROWS + 2)
        .saturating_sub(1)
}

pub(super) fn focused_approval_request(
    state: &TuiState,
) -> Option<&viden_core::ApprovalRequestView> {
    let overlay = state
        .ui
        .overlay
        .as_ref()
        .filter(|overlay| overlay.kind == OverlayKind::Approval)?;
    overlay.selected_id.as_ref().map_or_else(
        || state.runtime.pending_approvals.get(overlay.selected),
        |request_id| {
            state
                .runtime
                .pending_approvals
                .iter()
                .find(|approval| &approval.id == request_id)
        },
    )
}

pub(super) fn approval_is_expired(approval: &viden_core::ApprovalRequestView) -> bool {
    if approval.expires_at == 0 {
        return false;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| approval.expires_at <= duration.as_secs())
        .unwrap_or(false)
}

pub(super) fn interaction_panel_index_at(
    state: &TuiState,
    _width: u16,
    _height: u16,
    _rail: usize,
    _column: u16,
    row: u16,
) -> Option<usize> {
    state.ui.interaction_panel.as_ref()?;
    let index = usize::from(row.saturating_sub(5));
    (index < interaction_panel_choice_count(state)).then_some(index)
}

pub(super) fn interaction_panel_choice_count(state: &TuiState) -> usize {
    match state.ui.interaction_panel.as_ref() {
        Some(InteractionPanel::Settings(panel)) => {
            if !state.has_capability(UI_PREFERENCE_PERSISTENCE_CAPABILITY) {
                0
            } else {
                panel.field.map_or(8, |field| panel.choices(field).len())
            }
        }
        Some(InteractionPanel::Setup { .. }) => {
            if !state.has_capability("runtime.project_onboarding") {
                return 0;
            }
            let draft = match state.ui.interaction_panel.as_ref() {
                Some(InteractionPanel::Setup { draft, .. }) => draft,
                _ => unreachable!("setup panel matched"),
            };
            2 + usize::from(
                state
                    .runtime
                    .project_config_preview
                    .as_ref()
                    .is_some_and(|preview| setup_preview_matches_draft(preview, draft)),
            )
        }
        Some(InteractionPanel::GitPicker { phase, .. }) => match phase {
            GitPickerPhase::Browse => git_picker_rows(state).len(),
            GitPickerPhase::CommitMessage { .. } => 1,
        },
        Some(InteractionPanel::AcpPicker { phase, .. }) => match phase {
            AcpPickerPhase::Browse => acp_picker_rows(state).len(),
            AcpPickerPhase::TaskEntry { .. } => 1,
        },
        Some(InteractionPanel::NewLaneTask { .. }) => 1,
        _ => interaction_rows(state).len(),
    }
}

fn setup_preview_matches_draft(preview: &viden_core::ProjectConfigPreview, draft: &str) -> bool {
    preview.is_valid() && preview.exact_contents.as_deref() == Some(draft)
}

pub(super) fn selected_interaction_command(state: &TuiState) -> Option<String> {
    match state.ui.interaction_panel.as_ref()? {
        InteractionPanel::Settings(_) => None,
        InteractionPanel::Setup { .. } => None,
        InteractionPanel::ConnectProvider { selected, .. } => state
            .ui
            .provider_catalog
            .get(*selected)
            .map(|provider| format!("/provider use {}", provider.provider_id)),
        InteractionPanel::ProviderConfig { provider_id, .. } => {
            Some(format!("/provider configure {provider_id}"))
        }
        InteractionPanel::ModelPicker { selected, .. } => {
            interaction_rows(state).get(*selected).and_then(|row| {
                row.split_whitespace()
                    .collect::<Vec<_>>()
                    .get(..2)
                    .map(|parts| format!("/model use {} {}", parts[0], parts[1]))
            })
        }
        InteractionPanel::AcpPicker { .. }
        | InteractionPanel::GitPicker { .. }
        | InteractionPanel::NewLaneTask { .. } => None,
    }
}

pub(super) fn has_pending_approval(state: &TuiState) -> bool {
    !state.runtime.pending_approvals.is_empty()
}

pub(super) fn approval_action_at(
    state: &TuiState,
    _width: u16,
    _height: u16,
    _rail: usize,
    column: u16,
    _row: u16,
) -> Option<ApprovalAction> {
    has_explicit_approval_focus(state).then_some({
        if column < 28 {
            ApprovalAction::Deny
        } else if column < 48 {
            ApprovalAction::Diff
        } else {
            ApprovalAction::Approve
        }
    })
}

pub(super) fn approval_focus_cursor(
    state: &TuiState,
    width: u16,
    _height: u16,
    _rail: usize,
) -> Option<(u16, u16)> {
    has_explicit_approval_focus(state).then(|| {
        let column = match focused_approval_action(state) {
            ApprovalAction::ToggleApplyAll => 8,
            ApprovalAction::Deny => 18,
            ApprovalAction::Diff => 34,
            ApprovalAction::Approve => 52,
        };
        (column.min(width.saturating_sub(1)), 10)
    })
}

fn has_explicit_approval_focus(state: &TuiState) -> bool {
    state
        .ui
        .overlay
        .as_ref()
        .is_some_and(|overlay| overlay.kind == OverlayKind::Approval)
        && !focused_approval_rows(state).is_empty()
}

pub(super) fn move_approval_focus(state: &mut TuiState, delta: i8) {
    state.ui.approval_focus = if delta < 0 {
        state.ui.approval_focus.saturating_sub(1)
    } else {
        (state.ui.approval_focus + 1).min(APPROVAL_FOCUS_APPROVE)
    };
}

pub(super) fn set_approval_focus_for_action(state: &mut TuiState, action: ApprovalAction) {
    state.ui.approval_focus = match action {
        ApprovalAction::ToggleApplyAll => APPROVAL_FOCUS_APPLY_ALL,
        ApprovalAction::Deny => APPROVAL_FOCUS_DENY,
        ApprovalAction::Diff => APPROVAL_FOCUS_DIFF,
        ApprovalAction::Approve => APPROVAL_FOCUS_APPROVE,
    };
}

pub(super) fn focused_approval_action(state: &TuiState) -> ApprovalAction {
    match state.ui.approval_focus {
        APPROVAL_FOCUS_APPLY_ALL => ApprovalAction::ToggleApplyAll,
        APPROVAL_FOCUS_DENY => ApprovalAction::Deny,
        APPROVAL_FOCUS_DIFF => ApprovalAction::Diff,
        _ => ApprovalAction::Approve,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::OverlayState;
    use viden_core::{
        ApprovalDefaultAction, ApprovalRequestView, ApprovalRisk, ApprovalScope, ApprovalTarget,
    };
    use viden_types::{
        LaneRecoveryView, RuntimeCommand, RuntimeCommandReceipt, RuntimeEventEnvelope,
        RuntimeSnapshot, RuntimeWireEvent,
    };

    #[test]
    fn approvals_are_not_inferred_from_transcript() {
        let mut state = TuiState::default();
        state.ui.entries.push(super::super::state::TuiEntry {
            label: "approval".to_string(),
            body: "Press y".to_string(),
        });
        assert!(!has_pending_approval(&state));
    }

    #[test]
    fn acp_picker_lists_lane_sessions_before_truthful_adapter_rows() {
        let mut state = TuiState::default();
        state.ui.focused_lane = Some("lane-1".to_string());
        state
            .runtime
            .agent_sessions
            .push(viden_core::AgentSessionView {
                session_id: "acp-1".to_string(),
                lane_id: "lane-1".to_string(),
                agent_id: "codex-acp".to_string(),
                model: None,
                status: viden_core::AgentSessionStatus::Running,
                owner: viden_core::RuntimeOwner {
                    lane_id: Some("lane-1".to_string()),
                    session_id: Some("acp-1".to_string()),
                    ..Default::default()
                },
                task: "continue implementation".to_string(),
                diagnostic: None,
                output: None,
            });
        state
            .runtime
            .agent_adapters
            .push(viden_core::AgentAdapterView {
                agent_id: "claude-acp".to_string(),
                display_name: "Claude ACP".to_string(),
                route: viden_core::AgentRoute::Acp,
                source: viden_core::AgentAdapterSource::Registry,
                availability: viden_core::AgentAvailability::NeedsAuth,
                auth_state: viden_core::AgentAuthState::LoggedOut,
                startability: viden_core::AgentStartability::AuthenticationRequired,
                capabilities: Vec::new(),
                models: Vec::new(),
                diagnostics: vec!["agent.auth.required".to_string()],
            });

        let rows = acp_picker_rows(&state);

        assert_eq!(rows[0].id, "session:acp-1");
        assert_eq!(rows[1].id, "adapter:claude-acp");
        assert!(rows[1].label.contains("Authentication required"));
    }

    #[test]
    fn setup_selector_title_follows_core_resolved_locale() {
        let mut state = TuiState::default();
        state.runtime.snapshot.ui_preferences.locale = viden_core::LocaleId::ZhCn;
        state.ui.interaction_panel = Some(InteractionPanel::Setup {
            selected: 0,
            draft: String::new(),
        });
        let mut frame = Frame::new(100, 30);

        render_overlays(&mut frame, &state, 0);

        assert!(frame.to_string().contains("SETUP SELECTOR · 设置选择"));
    }

    #[test]
    fn models_selector_filters_unconfigured_providers() {
        let mut state = TuiState::default();
        let mut catalog = super::super::state::ProviderOption::fixture();
        let unconfigured = catalog
            .iter_mut()
            .find(|provider| provider.provider_id == "anthropic")
            .expect("anthropic fixture");
        unconfigured.enabled_models.clear();
        state.ui.provider_catalog = catalog;
        state.ui.interaction_panel = Some(InteractionPanel::ModelPicker {
            provider_id: None,
            search: String::new(),
            selected: 0,
        });

        let rows = interaction_rows(&state).join("\n");

        assert!(!rows.contains("anthropic"));
        assert!(rows.contains("deepseek"));
    }

    #[test]
    fn global_jump_windows_rows_to_keep_selected_item_visible() {
        let mut state = TuiState::default();
        let mut overlay = OverlayState::global_jump(None);
        // One row further down since `/evidence` joined the command registry.
        overlay.selected = 14;
        state.ui.overlay = Some(overlay);
        let mut frame = Frame::new(120, 40);

        render_overlays(&mut frame, &state, 0);

        let rendered = frame.to_string();
        assert!(
            rendered.contains("> /permissions rea"),
            "selected result must stay visible inside the fixed-height panel:\n{rendered}"
        );
    }

    #[test]
    fn global_jump_empty_and_disabled_windows_keep_rows_aligned_with_selection() {
        let mut state = TuiState::default();
        let mut overlay = OverlayState::global_jump(None);
        overlay.filter = ">no-such-command".to_string();
        state.ui.overlay = Some(overlay);

        assert_eq!(
            global_jump_rows(&state, ">no-such-command"),
            vec!["No matching items. Try : @ # > or ~."]
        );

        state.ui.overlay.as_mut().expect("jump").filter = "~".to_string();
        let rows = global_jump_rows(&state, "~");
        assert_eq!(rows[0], "[FILES]");
        assert!(rows[1].starts_with("> Files unavailabl"));
        assert!(rows[1].contains("Core file inventory is unavailable."));
    }

    /// Builds a view with one live gate and `dormant` gates whose sessions
    /// have finished.
    fn view_with_dormant_gates(dormant: usize) -> TuiState {
        let mut state = TuiState::default();
        let session =
            |id: &str, status: viden_types::AgentSessionStatus| viden_types::AgentSessionView {
                session_id: id.to_string(),
                lane_id: "lane-a".to_string(),
                agent_id: "codex".to_string(),
                model: None,
                status,
                owner: Default::default(),
                task: "work".to_string(),
                diagnostic: None,
                output: None,
            };
        state.runtime.agent_sessions.push(session(
            "session-live",
            viden_types::AgentSessionStatus::Running,
        ));
        let mut live = acp_gate_fixture("live");
        live.owner.session_id = Some("session-live".to_string());
        state.runtime.merge_gates.push(live);
        for index in 0..dormant {
            let id = format!("session-done-{index}");
            state
                .runtime
                .agent_sessions
                .push(session(&id, viden_types::AgentSessionStatus::Completed));
            let mut gate = acp_gate_fixture(&format!("done-{index}"));
            gate.owner.session_id = Some(id);
            state.runtime.merge_gates.push(gate);
        }
        state
    }

    /// Dormant gates are grouped under one separator, after the actionable
    /// rows, and remain visible rather than being hidden.
    #[test]
    fn decision_center_groups_dormant_gates_under_one_separator() {
        let state = view_with_dormant_gates(3);
        let rows = decision_center_rows(&state);

        let separator = rows
            .iter()
            .position(|row| row.contains("DORMANT"))
            .expect("a dormant separator row must be rendered");
        assert!(
            rows[separator].contains("3 gates from finished sessions"),
            "the separator must count the dormant gates: {:?}",
            rows[separator]
        );
        assert!(
            rows[separator].contains(Glyph::Gate.unicode()),
            "the separator must use the registered gate glyph: {:?}",
            rows[separator]
        );

        let live_row = rows
            .iter()
            .position(|row| row.contains("gate-acp-session-live"))
            .expect("the live gate must still be listed");
        assert!(
            live_row < separator,
            "the actionable gate must render above the dormant group: {rows:?}"
        );
        // Grouping, never hiding: every dormant gate still has its own row.
        for index in 0..3 {
            let needle = format!("gate-acp-session-done-{index}");
            let position = rows
                .iter()
                .position(|row| row.contains(&needle))
                .unwrap_or_else(|| panic!("dormant gate {needle} must stay visible: {rows:?}"));
            assert!(position > separator);
        }
    }

    /// With nothing dormant the separator never appears.
    #[test]
    fn decision_center_renders_no_separator_without_dormant_gates() {
        let state = view_with_dormant_gates(0);
        let rows = decision_center_rows(&state);
        assert!(
            !rows.iter().any(|row| row.contains("DORMANT")),
            "no dormant gates means no separator: {rows:?}"
        );
    }

    /// Two workflow files whose paths differ only in the filename must not
    /// render as the same jump row. This is the exact live collision: both
    /// `.github/workflows/*` entries rendered as `.github/workflow`.
    #[test]
    fn jump_rows_keep_the_filename_of_two_long_sibling_paths() {
        use crate::tui::workspace_files::WorkspaceFileIndex;
        use viden_core::{WorkspaceFileEntry, WorkspaceFileKind, WorkspaceFilePage};

        let mut files = WorkspaceFileIndex::default();
        files.mark_available(true);
        files.begin("files-1");
        files.apply_page(
            "files-1",
            &WorkspaceFilePage {
                entries: [".github/workflows/ci.yml", ".github/workflows/release.yml"]
                    .iter()
                    .map(|path| WorkspaceFileEntry {
                        path: (*path).to_string(),
                        kind: WorkspaceFileKind::File,
                        size_bytes: Some(64),
                    })
                    .collect(),
                next_after: None,
                complete: true,
            },
        );
        let mut state = TuiState::default();
        state.ui.workspace_files = files;
        state.ui.overlay = Some(OverlayState::global_jump(None));

        let rows = global_jump_rows(&state, "~");
        let file_rows = rows
            .iter()
            .filter(|row| !row.starts_with('['))
            .collect::<Vec<_>>();
        assert_eq!(file_rows.len(), 2, "both files must be listed: {rows:?}");
        assert_ne!(
            file_rows[0], file_rows[1],
            "two different files must never render as the same row: {rows:?}"
        );
        assert!(
            file_rows[0].contains("ci.yml") && file_rows[1].contains("release.yml"),
            "each row must keep the filename that identifies it: {rows:?}"
        );
    }

    /// An ACP-shaped gate: the id is keyed on the protocol session, so several
    /// gates share a long prefix and differ only in the trailing handle.
    fn acp_gate_fixture(session_suffix: &str) -> viden_types::MergeGateRecord {
        viden_types::MergeGateRecord {
            gate_id: format!("gate-acp-session-{session_suffix}"),
            task_id: format!("acp-session-{session_suffix}"),
            status: viden_types::MergeGateStatus::Proposed,
            required_evidence: Vec::new(),
            evidence_ids: Vec::new(),
            gate_type: Default::default(),
            owner: Default::default(),
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

    /// Two gates whose ids differ only after a long shared prefix must not
    /// render as the same jump row.
    #[test]
    fn jump_rows_keep_the_distinctive_tail_of_same_prefix_gate_ids() {
        let mut state = TuiState::default();
        for suffix in [
            "019fb746-46bc-7641-91ff-ba2e4ac51cdc",
            "019fb769-b057-7861-b523-d2aff89ca6b8",
        ] {
            state.runtime.merge_gates.push(acp_gate_fixture(suffix));
        }
        state.ui.overlay = Some(OverlayState::global_jump(None));

        let rows = global_jump_rows(&state, "#");
        let gate_rows = rows
            .iter()
            .filter(|row| !row.starts_with('['))
            .collect::<Vec<_>>();
        assert_eq!(gate_rows.len(), 2, "both gates must be listed: {rows:?}");
        assert_ne!(
            gate_rows[0], gate_rows[1],
            "two different gates must never render as the same row: {rows:?}"
        );
        assert!(
            gate_rows[0].contains("ba2e4ac51cdc") && gate_rows[1].contains("d2aff89ca6b8"),
            "each row must keep the id tail that identifies it: {rows:?}"
        );
    }

    #[test]
    fn global_jump_window_keeps_default_disabled_tail_selected() {
        let mut state = TuiState::default();
        let mut overlay = OverlayState::global_jump(None);
        // The disabled FILES tail, one row further down since `/evidence`
        // joined the command registry.
        overlay.selected = 16;
        state.ui.overlay = Some(overlay);

        let rows = global_jump_rows(&state, "");

        assert!(
            rows.iter().any(|row| row.starts_with("> Files unavailabl")),
            "the disabled tail result must not be dropped by a continued header: {rows:?}"
        );
    }

    /// Replays the shared `runtime.structured_diff` fixture and asserts the
    /// approval overlay reaches the same business facts the GUI does: Core's
    /// hunks, Core's line numbers, and the base the preview was computed
    /// against. Nothing here parses `input_preview`.
    /// Replays the shared `runtime.conflict_content` fixture: the merge path's
    /// bounce and the Lane apply path's conflict reach the same business facts
    /// — three sides, Core's line numbers, Core's reason, and Core's baseline
    /// kind — and both say in words that this is not a merge result.
    #[test]
    fn conflict_content_fixture_replays_into_summary_rows_and_a_three_sided_detail() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/conflict-content.json"
        ))
        .expect("conflict content fixture");
        let mut state = TuiState {
            capabilities: viden_core::frontend_capabilities(),
            ..TuiState::default()
        };
        for envelope in fixture["events"].as_array().expect("fixture events") {
            let envelope: RuntimeEventEnvelope =
                serde_json::from_value(envelope.clone()).expect("fixture envelope");
            if let RuntimeWireEvent::Known(event) = envelope.event {
                state.runtime.apply_event(&event);
            }
        }

        // The Decision Center row states the size and the baseline kind.
        state.ui.lens = crate::tui::state::Lens::Decisions;
        let decision_rows = overlay_rows_for_test(&state, OverlayKind::Decisions).join("\n");
        assert!(
            decision_rows.contains("bounce_conflict_b"),
            "{decision_rows}"
        );
        assert!(
            decision_rows.contains("1 files · 1 hunks"),
            "{decision_rows}"
        );
        assert!(
            decision_rows.contains("reviewed evidence"),
            "the merge path's baseline is its gate bindings: {decision_rows}"
        );

        // The modal shows three labelled sides and never a merged result.
        state.ui.conflict_detail = Some(ConflictDetailTarget::Bounce {
            gate_id: "gate_conflict_b".to_string(),
        });
        state.ui.overlay = Some(OverlayState::new(OverlayKind::ConflictContent));
        let bounce_rows = overlay_rows_for_test(&state, OverlayKind::ConflictContent).join("\n");

        assert!(bounce_rows.contains("not a merge result"), "{bounce_rows}");
        assert!(bounce_rows.contains("trust_loop.rs"), "{bounce_rows}");
        assert!(bounce_rows.contains("context mismatch"), "{bounce_rows}");
        assert!(
            bounce_rows.contains("record_conflict_bounce"),
            "{bounce_rows}"
        );
        assert!(bounce_rows.contains("bounce_with_reason"), "{bounce_rows}");
        assert!(bounce_rows.contains("record_bounce"), "{bounce_rows}");
        for side in ["OURS", "THEIRS", "BASE"] {
            assert!(bounce_rows.contains(side), "{side} missing: {bounce_rows}");
        }

        // The Lane apply path carries its own content and its own baseline.
        state.ui.conflict_detail = Some(ConflictDetailTarget::Lane {
            lane_id: "lane_conflict_b".to_string(),
        });
        let lane_rows = overlay_rows_for_test(&state, OverlayKind::ConflictContent).join("\n");

        assert!(lane_rows.contains("lane_bounce"), "{lane_rows}");
        assert!(lane_rows.contains("revision"), "{lane_rows}");
        assert!(lane_rows.contains("not a merge result"), "{lane_rows}");

        // The lane conflict's transcript entry states the same counts.
        let transcript = crate::tui::transcript::transcript_rows(&state, 120).join("\n");
        assert!(transcript.contains("1 files · 1 hunks"), "{transcript}");
    }

    /// Without the capability the bounce row and the transcript entry are
    /// exactly what they were, and the modal says the content is unavailable
    /// rather than claiming there was nothing to show.
    #[test]
    fn a_conflict_without_the_capability_keeps_the_reason_only_row() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/conflict-content.json"
        ))
        .expect("conflict content fixture");
        let mut state = TuiState::default();
        for envelope in fixture["events"].as_array().expect("fixture events") {
            let envelope: RuntimeEventEnvelope =
                serde_json::from_value(envelope.clone()).expect("fixture envelope");
            if let RuntimeWireEvent::Known(event) = envelope.event {
                state.runtime.apply_event(&event);
            }
        }

        let decision_rows = overlay_rows_for_test(&state, OverlayKind::Decisions).join("\n");
        assert!(
            decision_rows.contains("bounce_conflict_b"),
            "{decision_rows}"
        );
        assert!(!decision_rows.contains("hunks"), "{decision_rows}");

        state.ui.conflict_detail = Some(ConflictDetailTarget::Bounce {
            gate_id: "gate_conflict_b".to_string(),
        });
        let rows = overlay_rows_for_test(&state, OverlayKind::ConflictContent).join("\n");
        assert!(rows.contains("runtime.conflict_content"), "{rows}");
    }

    #[test]
    fn structured_diff_fixture_replays_into_approval_hunk_rows() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/structured-diff.json"
        ))
        .expect("structured diff fixture");
        let mut state = TuiState {
            capabilities: viden_core::frontend_capabilities(),
            ..TuiState::default()
        };
        for envelope in fixture["events"].as_array().expect("fixture events") {
            let envelope: RuntimeEventEnvelope =
                serde_json::from_value(envelope.clone()).expect("fixture envelope");
            if let RuntimeWireEvent::Known(event) = envelope.event {
                state.runtime.apply_event(&event);
            }
        }

        let mut overlay = OverlayState::new(OverlayKind::Approval);
        overlay.selected_id = Some("approval_structured_edit".to_string());
        state.ui.overlay = Some(overlay);
        let edit_rows = focused_approval_rows(&state).join("\n");

        assert!(edit_rows.contains("diff.rs"), "{edit_rows}");
        assert!(edit_rows.contains("@@ -42,3 +42,3 @@"), "{edit_rows}");
        assert!(
            edit_rows.contains("pub truncated: bool, // bounded"),
            "{edit_rows}"
        );
        // Git's numbering, not a row count: the removed line keeps its old-file
        // number and gains no new-file one.
        assert!(edit_rows.contains("   43       -"), "{edit_rows}");
        assert!(
            edit_rows.contains("computed against 3f79bb7b"),
            "{edit_rows}"
        );
        // The preview string is replaced, not shown beside the typed rows.
        assert!(!edit_rows.contains("INPUT"), "{edit_rows}");

        let mut overlay = OverlayState::new(OverlayKind::Approval);
        overlay.selected_id = Some("approval_structured_merge".to_string());
        state.ui.overlay = Some(overlay);
        let merge_rows = focused_approval_rows(&state).join("\n");

        // The multi-file case GUI-CORE-012 asked for, and no invented base
        // hash: one hash cannot describe several files.
        assert!(merge_rows.contains("frontend_services.rs"), "{merge_rows}");
        assert!(merge_rows.contains("decision_context.rs"), "{merge_rows}");
        assert!(!merge_rows.contains("computed against"), "{merge_rows}");
    }

    /// Without the extension the overlay keeps the preview line *and* says the
    /// rows are unavailable, rather than letting a lossy string stand in for
    /// the change Core did not publish.
    #[test]
    fn an_approval_without_the_structured_diff_capability_states_the_gap() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/structured-diff.json"
        ))
        .expect("structured diff fixture");
        let mut state = TuiState::default();
        for envelope in fixture["events"].as_array().expect("fixture events") {
            let envelope: RuntimeEventEnvelope =
                serde_json::from_value(envelope.clone()).expect("fixture envelope");
            if let RuntimeWireEvent::Known(event) = envelope.event {
                state.runtime.apply_event(&event);
            }
        }
        let mut overlay = OverlayState::new(OverlayKind::Approval);
        overlay.selected_id = Some("approval_structured_edit".to_string());
        state.ui.overlay = Some(overlay);

        let rows = focused_approval_rows(&state).join("\n");

        assert!(
            rows.contains("INPUT   path: crates/types/src/diff.rs"),
            "{rows}"
        );
        assert!(rows.contains("runtime.structured_diff"), "{rows}");
        assert!(!rows.contains("@@"), "{rows}");
    }

    #[test]
    fn approval_overlay_renders_four_core_scopes_and_expiry_without_local_resolution() {
        let mut state = TuiState::default();
        state.runtime.pending_approvals.push(ApprovalRequestView {
            id: "approval-four".to_string(),
            tool_name: "shell".to_string(),
            title: "Dangerous command".to_string(),
            message: "requires operator choice".to_string(),
            input_preview: "git push --force".to_string(),
            is_mutating: true,
            reason: Some("protected branch".to_string()),
            owner: Default::default(),
            risk: ApprovalRisk::Critical,
            target: ApprovalTarget {
                kind: "command".to_string(),
                display: "git push --force".to_string(),
                canonical_ref: Some("command://git-push".to_string()),
            },
            allowed_scopes: vec![
                ApprovalScope::Once,
                ApprovalScope::Session {
                    session_id: "session-four".to_string(),
                },
                ApprovalScope::RepoAllowlist {
                    paths: vec!["refs/heads/main".to_string()],
                },
            ],
            policy_reason_key: "approval.protected_branch".to_string(),
            policy_reason_args: Default::default(),
            expires_at: 1,
            default_action: ApprovalDefaultAction::Deny,
            audit_id: "audit-four".to_string(),
            decision_context: None,
        });
        let mut overlay = OverlayState::new(OverlayKind::Approval);
        overlay.selected_id = Some("approval-four".to_string());
        state.ui.overlay = Some(overlay);

        let rows = focused_approval_rows(&state).join("\n");

        for expected in [
            "1 Allow once",
            "2 Allow for session",
            "3 Add repo allowlist",
            "4 Deny",
            "EXPIRED",
            "awaiting Core ApprovalResolved",
            "audit-four",
        ] {
            assert!(rows.contains(expected), "missing {expected}:\n{rows}");
        }

        state.runtime.snapshot.ui_preferences.locale = viden_core::LocaleId::ZhCn;
        let chinese_rows = focused_approval_rows(&state).join("\n");
        for expected in [
            "1 本次允许",
            "2 本会话允许",
            "3 加入仓库白名单",
            "4 拒绝",
            "等待 Core ApprovalResolved",
            "git push --force",
            "audit-four",
        ] {
            assert!(
                chinese_rows.contains(expected),
                "missing {expected}:\n{chinese_rows}"
            );
        }
        let mut frame = Frame::new(100, 30);
        render_overlays(&mut frame, &state, 0);
        assert!(frame.to_string().contains("APPROVAL · 审批"));
        assert_eq!(state.runtime.pending_approvals.len(), 1);
    }

    #[test]
    fn approval_overlay_keeps_all_four_scopes_expiry_and_audit_visible() {
        let mut state = TuiState::default();
        state.runtime.pending_approvals.push(ApprovalRequestView {
            id: "approval-visible".to_string(),
            tool_name: "shell".to_string(),
            title: "Visible approval".to_string(),
            message: "all rows must remain visible".to_string(),
            input_preview: "cargo test".to_string(),
            is_mutating: true,
            reason: None,
            owner: Default::default(),
            risk: ApprovalRisk::Medium,
            target: ApprovalTarget {
                kind: "command".to_string(),
                display: "cargo test".to_string(),
                canonical_ref: None,
            },
            allowed_scopes: vec![
                ApprovalScope::Once,
                ApprovalScope::Session {
                    session_id: "session-visible".to_string(),
                },
                ApprovalScope::RepoAllowlist {
                    paths: vec!["Cargo.toml".to_string()],
                },
            ],
            policy_reason_key: "approval.test".to_string(),
            policy_reason_args: Default::default(),
            expires_at: 0,
            default_action: ApprovalDefaultAction::Deny,
            audit_id: "audit-visible-last-row".to_string(),
            decision_context: None,
        });
        let mut overlay = OverlayState::new(OverlayKind::Approval);
        overlay.selected_id = Some("approval-visible".to_string());
        state.ui.overlay = Some(overlay);
        let mut frame = Frame::new(100, 30);

        render_overlays(&mut frame, &state, 0);

        let rendered = frame.to_string();
        assert!(rendered.contains("1 Allow once"));
        assert!(rendered.contains("4 Deny"));
        assert!(rendered.contains("default Deny"));
        assert!(rendered.contains("audit-visible-last-row"));
    }

    #[test]
    fn decisions_overlay_projects_typed_gates_recovery_and_pending_core_command() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            initial_snapshot: RuntimeSnapshot,
            events: Vec<RuntimeEventEnvelope>,
        }

        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/merge-gate.json"
        ))
        .expect("merge gate fixture");
        let mut runtime = viden_types::RuntimeViewState::new(fixture.initial_snapshot);
        for envelope in fixture.events {
            if let RuntimeWireEvent::Known(event) = envelope.event {
                runtime.apply_event(&event);
            }
        }
        runtime.lane_recoveries.push(LaneRecoveryView {
            lane_id: "lane-recover".to_string(),
            reason: "detached".to_string(),
            next_action: "reattach".to_string(),
            timestamp: None,
        });
        runtime.last_command = Some(RuntimeCommandReceipt {
            command_id: "cmd-review".to_string(),
            command: RuntimeCommand::CancelActiveTurn,
        });
        let mut state = TuiState::new(runtime);
        state.ui.overlay = Some(OverlayState::new(OverlayKind::Decisions));

        let rows = global_overlay_rows(&state, OverlayKind::Decisions, "").join("\n");

        // The fixture snapshot resolves to zh-CN. The record-kind token stays
        // stable across locales the way the overlay titles do, so a Core gate id
        // is always findable by the same label.
        assert!(rows.contains("GATE 合并门 gate_merge"));
        assert!(rows.contains("Accepted"));
        assert!(
            rows.contains(Glyph::Gate.unicode()),
            "gate rows must carry the registered pause glyph: {rows}"
        );
        assert!(
            rows.starts_with('>'),
            "the picked row must be marked: {rows}"
        );
        assert!(rows.contains("RECOVERY lane-recover · detached · reattach"));
        assert!(rows.contains("COMMAND cmd-review · pending Core fact"));

        state.runtime.snapshot.ui_preferences.locale = viden_core::LocaleId::En;
        let english = global_overlay_rows(&state, OverlayKind::Decisions, "").join("\n");
        assert!(english.contains("GATE gate_merge"));
    }

    #[test]
    fn blind_lane_inspector_shows_bounded_run_facts_and_never_fabricates_zeros() {
        let mut state = TuiState::default();
        state.runtime.lanes = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        state.runtime.lanes.truncate(1);
        let lane_id = state.runtime.lanes[0].id.clone();
        state.ui.focused_lane = Some(lane_id);

        // A metered route keeps its existing surface: no blind marker, no run facts.
        state.runtime.lanes[0].route = viden_core::AgentRoute::BuiltIn;
        state.runtime.lanes[0].run_stats = Some(viden_types::LaneRunStats {
            wall_time_ms: 1_500,
            run_count: 3,
            diff_bytes: 900,
            last_exit_code: Some(0),
        });
        let metered = blind_cost_rows(&state, &state.runtime.lanes[0]);
        assert!(
            metered.is_empty(),
            "metered lane gained cost rows: {metered:?}"
        );

        // A blind route with no observed run publishes the marker and nothing else.
        state.runtime.lanes[0].route = viden_core::AgentRoute::Tmux;
        state.runtime.lanes[0].run_stats = None;
        let unobserved = blind_cost_rows(&state, &state.runtime.lanes[0]).join("\n");
        assert!(unobserved.contains("blind"));
        assert!(
            !unobserved.contains('0'),
            "absent run stats must not render zeros: {unobserved}"
        );

        // A blind route with observed runs publishes exactly the four bounded facts.
        state.runtime.lanes[0].run_stats = Some(viden_types::LaneRunStats {
            wall_time_ms: 1_500,
            run_count: 3,
            diff_bytes: 900,
            last_exit_code: Some(2),
        });
        let mut frame = Frame::new(112, 40);
        render_overlays(&mut frame, &state, 0);
        let rendered = frame.to_string();
        for expected in ["blind", "3 runs", "1.5 s", "900 B diff", "exit 2"] {
            assert!(
                rendered.contains(expected),
                "missing {expected}:\n{rendered}"
            );
        }

        // A force-killed or tmux run has no exit-code channel; say so instead of
        // inventing a success code.
        state.runtime.lanes[0].run_stats = Some(viden_types::LaneRunStats {
            wall_time_ms: 10,
            run_count: 1,
            diff_bytes: 0,
            last_exit_code: None,
        });
        assert!(
            blind_cost_rows(&state, &state.runtime.lanes[0])
                .join("\n")
                .contains("exit unknown")
        );

        state.runtime.snapshot.ui_preferences.locale = viden_core::LocaleId::ZhCn;
        let chinese = blind_cost_rows(&state, &state.runtime.lanes[0]).join("\n");
        assert!(chinese.contains("盲区"), "missing:\n{chinese}");
        assert!(chinese.contains("退出 未知"), "missing:\n{chinese}");
    }

    #[test]
    fn exit_confirmation_blocks_ownerless_active_work_without_offering_enter_to_exit() {
        let mut state = TuiState::default();
        state.runtime.lanes = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");

        let active = global_overlay_rows(&state, OverlayKind::ExitConfirm, "").join("\n");
        assert!(active.contains("exit is blocked"));
        assert!(active.contains("cancellable owner"));
        assert!(!active.contains("Press Enter to exit"));

        state.runtime.lanes.clear();
        let inactive = global_overlay_rows(&state, OverlayKind::ExitConfirm, "").join("\n");
        assert!(inactive.contains("No current work is active"));
        assert!(inactive.contains("Press Enter to exit"));

        state.runtime.snapshot.ui_preferences.locale = viden_core::LocaleId::ZhCn;
        let help = global_overlay_rows(&state, OverlayKind::ContextHelp, "").join("\n");
        let inactive = global_overlay_rows(&state, OverlayKind::ExitConfirm, "").join("\n");
        assert!(help.contains("Ctrl-C 取消当前工作"));
        assert!(inactive.contains("当前没有正在运行的工作"));
        assert!(inactive.contains("按 Enter 退出"));
    }

    #[test]
    fn settings_modal_shows_authoritative_values_effects_and_unavailable_gate() {
        let mut state = TuiState::default();
        state.ui.interaction_panel = Some(InteractionPanel::Settings(Box::new(
            crate::tui::preferences::SettingsPanel::new(
                &state.runtime.snapshot.ui_preferences,
                crate::tui::preferences::ColorDepth::Auto,
            ),
        )));

        let unavailable = interaction_rows(&state).join("\n");
        assert!(unavailable.contains("SETTINGS unavailable"));
        assert!(unavailable.contains("ui.preference_persistence"));

        state.capabilities.insert(viden_types::CapabilityId(
            "ui.preference_persistence".to_string(),
        ));
        let available = interaction_rows(&state).join("\n");
        for expected in [
            "Locale",
            "Skin",
            "Mode",
            "Density",
            "Motion",
            "Color depth",
            "Reset",
        ] {
            assert!(
                available.contains(expected),
                "missing {expected}:\n{available}"
            );
        }
        assert!(available.contains("current"));
        assert!(available.contains("effect"));

        state.runtime.snapshot.ui_preferences.locale = viden_core::LocaleId::ZhCn;
        if let Some(InteractionPanel::Settings(panel)) = state.ui.interaction_panel.as_mut() {
            assert!(panel.select(PreferenceValue::Skin(viden_core::UiSkin::Amber)));
            panel.field = Some(crate::tui::preferences::PreferenceField::Mode);
        }
        let chinese = interaction_rows(&state).join("\n");
        assert!(
            chinese.contains("效果"),
            "missing translated effect: {chinese}"
        );
        assert!(
            chinese.contains("[不可用]"),
            "missing disabled label: {chinese}"
        );
        assert!(
            !chinese.contains("effect:"),
            "English effect leaked: {chinese}"
        );
        assert!(
            !chinese.contains("[disabled]"),
            "English disabled leaked: {chinese}"
        );
    }
}
