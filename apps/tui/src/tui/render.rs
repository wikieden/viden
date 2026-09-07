use super::{
    canvas::Frame,
    composer::{composer_height, render_composer, render_welcome, should_render_welcome},
    geometry::effective_layout_width,
    modal::render_overlays,
    ops_screen::render_ops_body,
    panel::panel,
    preferences::{ColorDepth, TerminalCapabilities, resolve_appearance},
    projection::CockpitProjection,
    right_rail::right_rail,
    side_screen::render_side_body,
    state::{AgentTask, Lens, TuiState, agent_tasks},
    statusbar::{BOTTOM_BAR_HEIGHT, render_bottom_bar},
    text::{truncate, truncate_tail},
    topbar::{render_ops_top_bar, render_side_top_bar, render_top_bar},
    transcript::transcript_rows,
};
use std::time::{SystemTime, UNIX_EPOCH};

const MIN_HEIGHT: usize = 24;
pub(super) fn render_frame(state: &TuiState, width: u16, height: u16) -> String {
    let width = effective_layout_width(width);
    let height = (height as usize).max(MIN_HEIGHT);
    let mut frame = Frame::new(width, height);

    if state.ui.lens == Lens::Welcome && should_render_welcome(state) {
        render_welcome(&mut frame, state);
        // The welcome screen has no right rail; overlays must clear across the
        // full frame or setup hints can bleed through modal backgrounds.
        render_overlays(&mut frame, state, 0);
        return frame.to_string();
    }

    if matches!(
        state.ui.lens,
        Lens::Setup | Lens::Board | Lens::Decisions | Lens::Gallery
    ) {
        render_top_bar(&mut frame, state);
        render_lens_body(&mut frame, state);
        render_bottom_bar(&mut frame, state);
        render_overlays(&mut frame, state, 0);
        return frame.to_string();
    }

    render_top_bar(&mut frame, state);
    if width >= 112 && state.ui.right_rail_open {
        render_landscape_body(&mut frame, state);
    } else {
        render_compact_body(&mut frame, state);
    }
    render_composer(&mut frame, state, BOTTOM_BAR_HEIGHT);
    render_bottom_bar(&mut frame, state);
    render_overlays(&mut frame, state, right_rail_width(state));

    frame.to_string()
}

fn render_lens_body(frame: &mut Frame, state: &TuiState) {
    let body_top = 3;
    let body_height = frame.height.saturating_sub(body_top + BOTTOM_BAR_HEIGHT);
    let (title_key, rows) = match state.ui.lens {
        Lens::Setup => ("catalog.setup", setup_rows(state)),
        Lens::Board => ("catalog.board", board_rows(state)),
        Lens::Decisions => ("catalog.decisions", decision_rows(state)),
        Lens::Gallery => ("catalog.gallery", gallery_rows(state)),
        Lens::Welcome | Lens::Session => return,
    };
    let title = super::i18n::text(state, title_key);
    let block = panel(&title, rows, frame.width, body_height, None);
    frame.write_block(body_top, 0, &block);
}

fn setup_rows(state: &TuiState) -> Vec<String> {
    let mut rows = Vec::new();
    if let Some(probe) = state.runtime.project_probe.as_ref() {
        rows.push(format!("PROJECT {}", probe.root));
        rows.push(format!(
            "GIT {} · CONFIG {}",
            if probe.is_git_repository { "yes" } else { "no" },
            format!("{:?}", probe.config_state).to_ascii_lowercase()
        ));
        rows.push(format!("PATH {}", probe.config_path));
        if !probe.diagnostics.is_empty() {
            rows.extend(
                probe
                    .diagnostics
                    .iter()
                    .map(|diagnostic| format!("DIAGNOSTIC {diagnostic}")),
            );
        }
    } else {
        rows.push("PROJECT probe pending from Core".to_string());
    }

    if let Some(preview) = state.runtime.project_config_preview.as_ref() {
        rows.push(format!("PREVIEW {}", preview.relative_path));
        if let Some(contents) = preview.exact_contents.as_deref() {
            rows.extend(contents.lines().map(|line| format!("  {line}")));
        }
        rows.push("PENDING CORE CONFIRMATION".to_string());
    } else if state.runtime.confirmed_project_config.is_some() {
        rows.push("COMPLETE · CORE CONFIRMED".to_string());
    }

    if let Some(provider) = state.runtime.provider.as_ref() {
        rows.push(format!(
            "PROVIDER {} {} · {}",
            provider.provider_id, provider.status, provider.model
        ));
        let credential = provider.credential.as_ref().or_else(|| {
            state
                .runtime
                .credential_handles
                .iter()
                .find(|handle| handle.provider_id == provider.provider_id)
        });
        rows.push(match credential {
            Some(handle) => format!(
                "CREDENTIAL {} · HANDLE {}",
                format!("{:?}", handle.status).to_ascii_lowercase(),
                mask_credential_handle(&handle.backend_id)
            ),
            None => "CREDENTIAL unavailable · TRUSTED INGRESS unavailable".to_string(),
        });
    } else {
        rows.push("PROVIDER awaiting Core health".to_string());
    }
    rows
}

fn mask_credential_handle(value: &str) -> String {
    let characters = value.chars().collect::<Vec<_>>();
    if characters.len() <= 8 {
        return "***".to_string();
    }
    let prefix = characters.iter().take(3).collect::<String>();
    let suffix = characters
        .iter()
        .skip(characters.len().saturating_sub(4))
        .collect::<String>();
    format!("{prefix}…{suffix}")
}

fn board_rows(state: &TuiState) -> Vec<String> {
    if state.runtime.lanes.is_empty() {
        return vec![super::i18n::text(state, "lane_board.empty")];
    }
    state
        .runtime
        .lanes
        .iter()
        .map(|lane| {
            let sessions = if lane.active_session_ids.is_empty() {
                "-".to_string()
            } else {
                lane.active_session_ids.join(",")
            };
            format!(
                "{} · {} · {:?} · SESSION {}",
                lane.id, lane.role, lane.status, sessions
            )
        })
        .collect()
}

fn decision_rows(state: &TuiState) -> Vec<String> {
    let projection =
        CockpitProjection::from_with_capabilities(&state.runtime, &state.ui, &state.capabilities);
    let mut rows = projection
        .approvals
        .iter()
        .map(|approval| {
            let expiry = projection
                .approval_actions
                .iter()
                .find(|action| action.request_id == approval.id)
                .map(|action| format!(" · {:?}", action.expiry))
                .unwrap_or_default();
            format!(
                "APPROVAL {} · {}{} · AUDIT {}",
                approval.id, approval.title, expiry, approval.audit_id
            )
        })
        .collect::<Vec<_>>();
    rows.extend(projection.merge_gates.iter().map(|gate| {
        format!(
            "GATE {} · {:?} · {:?}",
            gate.gate_id, gate.status, gate.decision
        )
    }));
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
    if rows.is_empty() {
        rows.push(super::i18n::text(state, "decisions.empty"));
    }
    rows
}

fn gallery_rows(state: &TuiState) -> Vec<String> {
    let projection =
        CockpitProjection::from_with_capabilities(&state.runtime, &state.ui, &state.capabilities);
    if projection.evidence.is_empty() && projection.evidence_decisions.is_empty() {
        return vec![super::i18n::text(state, "gallery.empty")];
    }
    let mut rows = projection
        .evidence
        .iter()
        .map(|evidence| format!("{} · {} · {}", evidence.id, evidence.kind, evidence.summary))
        .collect::<Vec<_>>();
    rows.extend(projection.evidence_decisions.iter().map(|decision| {
        format!(
            "{} · {:?} · GATE {}",
            decision.evidence_id, decision.decision, decision.gate_id
        )
    }));
    rows
}

pub(super) fn render_side_frame(state: &TuiState, width: u16, height: u16) -> String {
    let width = effective_layout_width(width);
    let height = (height as usize).max(MIN_HEIGHT);
    let mut frame = Frame::new(width, height);

    render_side_top_bar(&mut frame, state);
    render_side_body(&mut frame, state);
    render_bottom_bar(&mut frame, state);

    frame.to_string()
}

pub(super) fn render_ops_frame(state: &TuiState, width: u16, height: u16) -> String {
    let width = effective_layout_width(width);
    let height = (height as usize).max(MIN_HEIGHT);
    let mut frame = Frame::new(width, height);

    render_ops_top_bar(&mut frame, state);
    render_ops_body(&mut frame, state);
    render_bottom_bar(&mut frame, state);

    frame.to_string()
}

fn render_landscape_body(frame: &mut Frame, state: &TuiState) {
    let body_top = 3;
    let body_bottom = frame.height - composer_height(state, frame.width) - BOTTOM_BAR_HEIGHT - 1;
    let body_height = body_bottom.saturating_sub(body_top) + 1;
    let rail_width = right_rail_width(state);
    let rail_left = frame.width.saturating_sub(rail_width);
    let transcript_width = rail_left.saturating_sub(1);

    let transcript_rows = main_transcript_rows(
        state,
        transcript_width.saturating_sub(4),
        body_height.saturating_sub(2),
    );
    let transcript_badge = transcript_status_label(state);
    let transcript_title = super::i18n::text(state, "catalog.transcript");
    let transcript = panel(
        &transcript_title,
        transcript_rows,
        transcript_width,
        body_height,
        Some(transcript_badge.as_str()),
    );
    frame.write_block(body_top, 0, &transcript);

    let rail = right_rail(state, rail_width, body_height);
    frame.write_block(body_top, rail_left, &rail);
}

fn render_compact_body(frame: &mut Frame, state: &TuiState) {
    let body_top = 3;
    let body_bottom = frame.height - composer_height(state, frame.width) - BOTTOM_BAR_HEIGHT - 1;
    let body_height = body_bottom.saturating_sub(body_top) + 1;
    let transcript_rows = main_transcript_rows(
        state,
        frame.width.saturating_sub(4),
        body_height.saturating_sub(2),
    );
    let transcript_title = super::i18n::text(state, "catalog.transcript");
    let transcript = panel(
        &transcript_title,
        transcript_rows,
        frame.width,
        body_height,
        None,
    );
    frame.write_block(body_top, 0, &transcript);
}

fn main_transcript_rows(state: &TuiState, width: usize, max_rows: usize) -> Vec<String> {
    let mut rows = transcript_rows(state, width);
    let activity = operation_center_rows(state, width);
    if !activity.is_empty() {
        if !rows.is_empty() {
            rows.push(activity_separator(width));
        }
        rows.extend(activity);
    }
    recent_rows(rows, max_rows, state.ui.transcript_scroll)
}

fn transcript_status_label(state: &TuiState) -> String {
    if state.ui.transcript_scroll > 0 {
        let marker =
            if !state.runtime.assistant_stream.is_empty() || super::state::has_active_work(state) {
                " · new output"
            } else {
                ""
            };
        format!("history {}{marker}", state.ui.transcript_scroll)
    } else {
        "live session".to_string()
    }
}

fn operation_center_rows(state: &TuiState, width: usize) -> Vec<String> {
    let status = live_activity_status(state);
    let mut rows = if status.is_live {
        live_work_strip_rows(state, &status, width)
    } else {
        let mut inactive_rows = vec![truncate(
            &format!(
                "     ┊  ✦ {} {}",
                status.summary,
                thinking_indicator(state, state.ui.pulse_frame)
            ),
            width,
        )];
        if let Some(detail) = status.details.first() {
            inactive_rows.push(truncate(&format!("     ┊    └ {}", detail), width));
        } else {
            inactive_rows.push(truncate(&format!("     ┊    └ {}", status.evidence), width));
        }
        if !status.evidence.starts_with("AgentTask")
            && !inactive_rows
                .iter()
                .any(|row| row.contains(&status.evidence))
        {
            inactive_rows.push(truncate(
                &format!("     ┊      signal {}", status.evidence),
                width,
            ));
        }
        inactive_rows
    };
    rows.extend(supervision_rows(state, width));
    rows
}

fn supervision_rows(state: &TuiState, width: usize) -> Vec<String> {
    let projection =
        CockpitProjection::from_with_capabilities(&state.runtime, &state.ui, &state.capabilities);
    if projection.approval_actions.is_empty()
        && projection.merge_gates.is_empty()
        && projection.recovery_actions.is_empty()
        && projection.pending_command.is_none()
    {
        return Vec::new();
    }
    let supervision = super::i18n::text(state, "supervision.header");
    let mut rows = vec![truncate(&format!("     ┊  {supervision}"), width)];
    rows.extend(projection.approval_actions.iter().map(|approval| {
        truncate(
            &format!(
                "     ┊  APPROVAL {} · {:?} · AUDIT {}",
                approval.request_id, approval.expiry, approval.audit_id
            ),
            width,
        )
    }));
    rows.extend(projection.merge_gates.iter().map(|gate| {
        truncate(
            &format!(
                "     ┊  GATE {} · {:?} · {:?}",
                gate.gate_id, gate.status, gate.decision
            ),
            width,
        )
    }));
    rows.extend(projection.recovery_actions.iter().map(|recovery| {
        truncate(
            &format!(
                "     ┊  RECOVERY {} · {} · {}",
                recovery.lane_id.as_deref().unwrap_or("runtime"),
                recovery.reason,
                recovery.action
            ),
            width,
        )
    }));
    if let Some(command) = projection.pending_command.as_ref() {
        rows.push(truncate(
            &format!("     ┊  COMMAND {} · pending Core fact", command.command_id),
            width,
        ));
    }
    rows
}

fn live_work_strip_rows(
    state: &TuiState,
    status: &LiveActivityStatus,
    width: usize,
) -> Vec<String> {
    let header_rule = "─".repeat(width.saturating_sub(24).min(96));
    let footer_rule = "─".repeat(width.saturating_sub(7).min(112));
    let phase = status.phase.as_deref().unwrap_or("active");
    let signal = live_work_signal(status)
        .map(|value| format!(" · signal {value}"))
        .unwrap_or_default();
    let guidance = status
        .next_action
        .as_deref()
        .map(|action| format!("next {action} · input open"))
        .unwrap_or_else(|| super::i18n::text(state, "live.input_open"));
    let title = super::i18n::text(state, "live.title");
    let mut rows = vec![
        truncate(
            &format!(
                "     ╭─ {title} {} {header_rule}",
                thinking_indicator(state, state.ui.pulse_frame)
            ),
            width,
        ),
        truncate(
            &format!("     │ ◉ {}", live_work_headline(&status.summary)),
            width,
        ),
        truncate(&format!("     │ phase {phase}{signal}"), width),
    ];
    if status.details.is_empty() {
        rows.push(truncate(&format!("     │ {}", status.evidence), width));
    } else {
        rows.extend(
            status
                .details
                .iter()
                .take(3)
                .map(|detail| truncate(&format!("     │ {detail}"), width)),
        );
    }
    rows.push(truncate(&format!("     │ {guidance}"), width));
    rows.push(truncate(&format!("     ╰{footer_rule}"), width));
    rows
}

fn live_work_headline(summary: &str) -> String {
    if summary.starts_with("Viden is ") {
        "Viden working".to_string()
    } else {
        summary.to_string()
    }
}

fn live_work_signal(status: &LiveActivityStatus) -> Option<&str> {
    status
        .evidence
        .split_once(" from ")
        .map(|(_, signal)| signal)
        .or_else(|| Some(status.evidence.as_str()).filter(|value| !value.starts_with("AgentTask")))
}

fn live_activity_status(state: &TuiState) -> LiveActivityStatus {
    // Priority mirrors operator urgency and reads from the normalized AgentTask
    // view so every panel describes the same runtime state.
    let active_agent_tasks = agent_tasks(state)
        .into_iter()
        .filter(AgentTask::is_active)
        .collect::<Vec<_>>();
    if !active_agent_tasks.is_empty() {
        let primary = &active_agent_tasks[0];
        let delegated_count = active_agent_tasks
            .iter()
            .filter(|task| matches!(task.kind.as_str(), "lane" | "job"))
            .count();
        let summary = operator_summary(primary, delegated_count);
        let phase = operator_status_label(primary).to_string();
        let next_action = next_operator_action(primary).map(str::to_string);
        return LiveActivityStatus {
            summary,
            evidence: format!(
                "AgentTask {} from {}",
                primary.id,
                primary_task_signal(primary)
                    .as_deref()
                    .or_else(|| primary.evidence.first().map(String::as_str))
                    .unwrap_or("runtime view")
            ),
            details: active_agent_tasks
                .into_iter()
                .map(|task| operator_detail(&task))
                .collect(),
            phase: Some(phase),
            next_action,
            is_live: true,
        };
    }

    if let Some(task) = agent_tasks(state).into_iter().rev().find(|task| {
        matches!(
            task.status.as_str(),
            "done" | "failed" | "cancelled" | "finished" | "observed" | "completed"
        ) && !task.evidence.is_empty()
    }) {
        return LiveActivityStatus {
            summary: historical_task_summary(&task),
            evidence: primary_task_signal(&task).unwrap_or_else(|| "agent task result".to_string()),
            details: vec![operator_detail(&task)],
            phase: None,
            next_action: None,
            is_live: false,
        };
    }

    if !state.runtime.assistant_stream.is_empty() {
        return LiveActivityStatus {
            summary: "Viden working".to_string(),
            evidence: "live provider request".to_string(),
            // The newest text is what an operator watching a live turn needs;
            // a head cut pins the display to the opening words forever.
            details: vec![truncate_tail(&state.runtime.assistant_stream, 72)],
            phase: Some("streaming".to_string()),
            next_action: Some("type next step anytime".to_string()),
            is_live: true,
        };
    }

    if let Some(entry) = state.ui.entries.last() {
        return LiveActivityStatus {
            summary: compact_activity_label(entry.label.as_str()).to_string(),
            evidence: "latest transcript event".to_string(),
            details: vec![compact_activity_detail(&entry.body)],
            phase: None,
            next_action: None,
            is_live: false,
        };
    }

    let provider_state = if super::state::provider_status(state).request_count == 0 {
        "idle; no provider request yet".to_string()
    } else {
        format!(
            "idle; last provider status {}",
            super::state::provider_status(state).connection
        )
    };
    LiveActivityStatus {
        summary: provider_state,
        evidence: "provider telemetry".to_string(),
        details: vec![format!(
            "{} / {}",
            state.runtime.snapshot.provider_family, state.runtime.snapshot.model_label
        )],
        phase: None,
        next_action: None,
        is_live: false,
    }
}

fn operator_summary(task: &AgentTask, delegated_count: usize) -> String {
    if delegated_count > 0 && matches!(task.kind.as_str(), "lane" | "job") {
        if task.status == "blocked" {
            return format!(
                "Supervising {} agent{}: blocked on {}",
                delegated_count,
                if delegated_count == 1 { "" } else { "s" },
                primary_task_signal(task).unwrap_or_else(|| task.activity.clone())
            );
        }
        return format!(
            "Supervising {} agent{}: {} {}",
            delegated_count,
            if delegated_count == 1 { "" } else { "s" },
            operator_agent_label(task),
            operator_status_label(task)
        );
    }
    match task.status.as_str() {
        "waiting_approval" => format!("Approval needed: {}", task.activity),
        "needs_input" if task.kind == "diff" => task.activity.clone(),
        "testing" => {
            if let Some(command) = evidence_value(task, "command ") {
                format!("Testing: {command}")
            } else {
                task.activity.clone()
            }
        }
        "editing" | "running_tool" => task.activity.clone(),
        "thinking" | "streaming" => format!(
            "{} {}",
            operator_agent_label(task),
            operator_activity_phrase(task)
        ),
        "needs_input" => format!("Needs input: {}", operator_agent_label(task)),
        "blocked" => format!(
            "Blocked: {}",
            primary_task_signal(task).unwrap_or_else(|| task.activity.clone())
        ),
        _ => task.activity.clone(),
    }
}

fn operator_detail(task: &AgentTask) -> String {
    let mut parts = vec![format!(
        "{} · {} {}",
        task.id,
        operator_agent_label(task),
        operator_status_label(task)
    )];
    if task.is_active() && task.progress > 0 && task.kind != "provider" {
        parts.push(format!("{}%", task.progress));
    }
    if let Some(next) = next_operator_action(task) {
        parts.push(format!("next {next}"));
    }
    if let Some(signal) = primary_task_signal(task) {
        parts.push(signal);
    } else if !task.activity.is_empty() {
        parts.push(truncate(&task.activity, 40));
    } else if !task.summary.is_empty() {
        parts.push(truncate(&task.summary, 40));
    } else if !task.title.is_empty() {
        parts.push(truncate(&task.title, 40));
    }
    if task.is_active()
        && let Some(started_at) = task.started_at
    {
        parts.push(format!("elapsed {}", elapsed_millis(started_at)));
    }
    if let Some(updated_at) = task.updated_at {
        parts.push(format!("updated {}", relative_millis(updated_at)));
    }
    parts.join(" · ")
}

fn historical_task_summary(task: &AgentTask) -> String {
    match (task.kind.as_str(), task.status.as_str()) {
        ("test", "failed") => evidence_value(task, "command ")
            .map(|command| format!("Tests failed: {command}"))
            .unwrap_or_else(|| "Tests failed".to_string()),
        ("test", "done") => evidence_value(task, "command ")
            .map(|command| format!("Tests passed: {command}"))
            .unwrap_or_else(|| "Tests passed".to_string()),
        (_, "failed") => format!(
            "Latest {} failed: {}",
            operator_agent_label(task),
            primary_task_signal(task).unwrap_or_else(|| task.activity.clone())
        ),
        (_, "cancelled") => format!("Latest {} task cancelled", operator_agent_label(task)),
        _ => format!("Latest {} task {}", operator_agent_label(task), task.status),
    }
}

fn primary_task_signal(task: &AgentTask) -> Option<String> {
    // Operation center copy should surface the blocker or proof, not the lowest
    // level source label like "transcript tool-result".
    for prefix in [
        "failure ",
        "failing-file ",
        "conflict ",
        "approval ",
        "message ",
        "command ",
        "tail ",
        "rerun ",
        "summary ",
        "files ",
        "additions ",
        "deletions ",
        "changed ",
        "path ",
        "resume ",
        "thread ",
        "turn ",
    ] {
        if let Some(value) = evidence_value(task, prefix) {
            return Some(format!("{} {value}", prefix.trim()));
        }
    }
    task.evidence
        .iter()
        .find(|item| !item.starts_with("transcript "))
        .cloned()
}

fn evidence_value(task: &AgentTask, prefix: &str) -> Option<String> {
    task.evidence
        .iter()
        .find_map(|item| item.strip_prefix(prefix).map(str::trim))
        .map(|value| truncate(value, 88))
        .filter(|value| !value.is_empty())
}

fn next_operator_action(task: &AgentTask) -> Option<&'static str> {
    match task.status.as_str() {
        "waiting_approval" => Some("approve, diff, or deny"),
        "blocked" => Some("inspect conflict and revise/apply manually"),
        "failed" if task.kind == "test" => Some("open failure, patch, rerun tests"),
        "failed" => Some("inspect result and retry or discard"),
        "needs_input" if task.kind == "diff" => Some("review diff, then test or commit"),
        "needs_input" => Some("send follow-up to lane"),
        "testing" => Some("wait for test result"),
        "editing" | "running_tool" => Some("wait for tool result"),
        "done" => Some("review result or continue"),
        _ => None,
    }
}

fn operator_agent_label(task: &AgentTask) -> String {
    if task.agent == "viden" && task.kind == "provider" {
        "Viden".to_string()
    } else {
        task.agent.clone()
    }
}

fn operator_status_label(task: &AgentTask) -> &'static str {
    match task.status.as_str() {
        "waiting_approval" => "waiting approval",
        "running_tool" => "using tool",
        "needs_input" => "needs input",
        "cancelled" => "cancelled",
        "archived" => "archived",
        "queued" => "queued",
        "thinking" if task.kind == "provider" => "planning",
        "streaming" if task.kind == "provider" => "drafting",
        "thinking" => "thinking",
        "streaming" => "streaming",
        "editing" => "editing",
        "testing" => "testing",
        "blocked" => "blocked",
        "done" => "done",
        "failed" => "failed",
        _ => "active",
    }
}

fn operator_activity_phrase(task: &AgentTask) -> &'static str {
    if task.kind == "provider" && task.activity.contains("compacting") {
        return "is reducing context";
    }
    match (task.kind.as_str(), task.status.as_str()) {
        ("provider", "streaming") => "is drafting",
        ("provider", "thinking") => "is planning",
        (_, "streaming") => "is streaming",
        _ => "is thinking",
    }
}

fn compact_activity_label(label: &str) -> &'static str {
    match label {
        "assistant" => "reply ready",
        "tool-call" => "reply using tool",
        "tool-result" => "tool result ready",
        "system" => "system idle",
        _ => "session idle",
    }
}

fn compact_activity_detail(body: &str) -> String {
    let detail = body
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .unwrap_or_else(|| "no detail available".to_string());
    if let Some((tool, rest)) = detail.split_once(" path: ") {
        let path = rest.split_whitespace().next().unwrap_or(rest);
        if matches!(tool, "write_file" | "edit_file") {
            return format!("Editing {path}");
        }
    }
    detail
}

fn activity_separator(width: usize) -> String {
    truncate(
        &format!("     ┊  {}", "┄".repeat(width.saturating_sub(8).min(88))),
        width,
    )
}

/// Samples the current animation phase from the wall clock. Called once per draw
/// by the event loop, never from the render model, so a rendered frame stays a
/// pure function of `TuiState`.
pub(super) fn sampled_pulse_frame() -> usize {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    (now / 350) as usize
}

fn thinking_indicator(state: &TuiState, frame: usize) -> &'static str {
    let capabilities = TerminalCapabilities::detect();
    let appearance = resolve_appearance(
        &state.runtime.snapshot.ui_preferences,
        ColorDepth::Auto,
        capabilities,
    );
    appearance
        .glyphs
        .activity_indicator(appearance.reduced_motion(), frame)
}

pub(super) fn right_rail_width(state: &TuiState) -> usize {
    resolve_appearance(
        &state.runtime.snapshot.ui_preferences,
        ColorDepth::Auto,
        TerminalCapabilities::default(),
    )
    .geometry
    .right_rail_width
}

#[cfg(test)]
mod appearance_tests {
    use super::*;
    use viden_core::{UiColorMode, UiDensity, UiMotion, UiPreferenceDiagnostic, UiSkin};

    #[test]
    fn reduced_motion_keeps_the_live_indicator_static() {
        let mut state = TuiState::default();
        state.runtime.snapshot.ui_preferences.motion = UiMotion::Reduced;

        assert_eq!(thinking_indicator(&state, 0), thinking_indicator(&state, 7));

        state.runtime.snapshot.ui_preferences.motion = UiMotion::Full;
        assert_ne!(thinking_indicator(&state, 0), thinking_indicator(&state, 1));
    }

    #[test]
    fn core_density_changes_the_rendered_right_rail_geometry() {
        let mut state = TuiState::default();
        state.runtime.snapshot.ui_preferences.density = UiDensity::Compact;
        assert_eq!(right_rail_width(&state), 34);

        state.runtime.snapshot.ui_preferences.density = UiDensity::Regular;
        assert_eq!(right_rail_width(&state), 38);

        state.runtime.snapshot.ui_preferences.density = UiDensity::Comfy;
        assert_eq!(right_rail_width(&state), 42);
    }

    #[test]
    fn invalid_appearance_falls_back_to_regular_render_geometry() {
        let mut state = TuiState::default();
        state.runtime.snapshot.ui_preferences.skin = UiSkin::Amber;
        state.runtime.snapshot.ui_preferences.mode = UiColorMode::Light;
        state.runtime.snapshot.ui_preferences.density = UiDensity::Comfy;
        state.runtime.snapshot.ui_preferences.diagnostics = vec![UiPreferenceDiagnostic::new(
            "ui.invalid_skin_mode_pair",
            "skin_mode",
            "ui.mode",
            Some("amber/light".to_string()),
        )];

        assert_eq!(right_rail_width(&state), 38);
    }
}

struct LiveActivityStatus {
    summary: String,
    evidence: String,
    details: Vec<String>,
    phase: Option<String>,
    next_action: Option<String>,
    is_live: bool,
}

fn relative_millis(updated_at: u64) -> String {
    let updated_at = u128::from(updated_at);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(updated_at);
    let elapsed = now.saturating_sub(updated_at);
    if elapsed < 1_000 {
        "now".to_string()
    } else if elapsed < 60_000 {
        format!("{}s ago", elapsed / 1_000)
    } else if elapsed < 3_600_000 {
        format!("{}m ago", elapsed / 60_000)
    } else {
        format!("{}h ago", elapsed / 3_600_000)
    }
}

fn elapsed_millis(started_at: u64) -> String {
    let started_at = u128::from(started_at);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(started_at);
    let elapsed = now.saturating_sub(started_at);
    if elapsed < 1_000 {
        "<1s".to_string()
    } else if elapsed < 60_000 {
        format!("{}s", elapsed / 1_000)
    } else if elapsed < 3_600_000 {
        format!("{}m {}s", elapsed / 60_000, (elapsed % 60_000) / 1_000)
    } else if elapsed > 86_400_000 {
        "now".to_string()
    } else {
        format!(
            "{}h {}m",
            elapsed / 3_600_000,
            (elapsed % 3_600_000) / 60_000
        )
    }
}

fn recent_rows(mut rows: Vec<String>, max_rows: usize, scroll: usize) -> Vec<String> {
    if rows.len() > max_rows {
        let max_scroll = rows.len().saturating_sub(max_rows);
        let start = max_scroll.saturating_sub(scroll.min(max_scroll));
        rows = rows[start..start + max_rows].to_vec();
    }
    while rows
        .first()
        .is_some_and(|row| is_loose_timeline_connector(row))
    {
        rows.remove(0);
    }
    rows
}

fn is_loose_timeline_connector(row: &str) -> bool {
    let trimmed = row.trim();
    trimmed == "│" || trimmed == "│  ·" || trimmed == "│ ·"
}

#[cfg(test)]
mod structured_runtime_tests {
    use super::*;
    use crate::tui::state::{Lens, TuiEntry, TuiState};
    use viden_types::{
        CredentialHandle, CredentialStatus, ProjectConfigPreview, ProjectConfigState, ProjectProbe,
        ProviderHealthView, RuntimeErrorView, RuntimeEventEnvelope, RuntimeSnapshot,
        RuntimeWireEvent, ToolCallView,
    };

    fn state() -> TuiState {
        let mut state = TuiState::default();
        state.ui.session_id = "session-structured".to_string();
        state.ui.theme_name = "aurora/dark".to_string();
        state.runtime.snapshot.cwd = "/workspace/viden".into();
        state.runtime.snapshot.provider_family = "fallback".to_string();
        state.runtime.snapshot.model_label = "test-local".to_string();
        state.ui.entries.push(TuiEntry {
            label: "assistant".to_string(),
            body: "hello".to_string(),
        });
        state
    }

    #[test]
    fn frame_renders_local_transcript_and_structured_runtime_status() {
        let mut state = state();
        state.runtime.provider = Some(ProviderHealthView {
            provider_id: "fallback".to_string(),
            model: "test-local".to_string(),
            status: "healthy".to_string(),
            request_count: 2,
            error_count: 0,
            last_latency_ms: Some(42),
            average_latency_ms: Some(30),
            tokens_per_second: Some(20),
            credential: None,
        });
        state.runtime.active_tool_calls.push(ToolCallView {
            tool_call_id: "tool-1".to_string(),
            name: "search".to_string(),
            input_preview: "src".to_string(),
            owner: None,
        });

        let rendered = render_frame(&state, 140, 36);

        assert!(rendered.contains("hello"));
        assert!(rendered.contains("search"));
        assert!(rendered.contains("PROVIDER healthy"));
    }

    #[test]
    fn welcome_setup_lanes_and_cockpit_follow_core_facts() {
        let welcome = TuiState::default();
        let rendered = render_frame(&welcome, 112, 40);
        assert!(rendered.contains("Ask anything"));
        assert!(!rendered.contains("TRANSCRIPT"));

        let mut setup = TuiState::default();
        setup.ui.lens = Lens::Setup;
        setup.runtime.project_probe = Some(ProjectProbe {
            root: "/workspace/demo".to_string(),
            is_git_repository: true,
            git_root: Some("/workspace/demo".to_string()),
            config_path: "/workspace/demo/viden.toml".to_string(),
            config_state: ProjectConfigState::Missing,
            project_name: Some("demo".to_string()),
            pack: Some("robot-pack".to_string()),
            diagnostics: Vec::new(),
        });
        let preview = ProjectConfigPreview {
            preview_id: "preview-core".to_string(),
            relative_path: "viden.toml".to_string(),
            content_sha256: "a".repeat(64),
            byte_len: 44,
            exact_contents: Some("[project]\nname = \"demo\"\npack = \"robot-pack\"\n".to_string()),
            base_content_sha256: None,
            project_name: Some("demo".to_string()),
            pack: Some("robot-pack".to_string()),
            diagnostics: Vec::new(),
        };
        setup.runtime.project_config_preview = Some(preview.clone());
        setup.runtime.provider = Some(ProviderHealthView {
            provider_id: "provider-1".to_string(),
            model: "model-1".to_string(),
            status: "healthy".to_string(),
            request_count: 0,
            error_count: 0,
            last_latency_ms: None,
            average_latency_ms: None,
            tokens_per_second: None,
            credential: Some(CredentialHandle {
                provider_id: "provider-1".to_string(),
                backend_id: "keychain:item-1234".to_string(),
                status: CredentialStatus::Available,
            }),
        });

        let pending = render_frame(&setup, 112, 40);
        assert!(pending.contains("SETUP"));
        assert!(pending.contains("PROJECT /workspace/demo"));
        assert!(pending.contains("CONFIG missing"));
        assert!(pending.contains("PREVIEW viden.toml"));
        assert!(pending.contains("PENDING CORE CONFIRMATION"));
        assert!(pending.contains("PROVIDER provider-1 healthy"));
        assert!(pending.contains("CREDENTIAL available"));
        assert!(pending.contains("HANDLE key…1234"));
        assert!(!pending.contains("keychain:item-1234"));
        assert!(!pending.contains("COMPLETE · CORE CONFIRMED"));

        setup.runtime.project_config_preview = None;
        setup.runtime.confirmed_project_config = Some(preview);
        let confirmed = render_frame(&setup, 112, 40);
        assert!(confirmed.contains("COMPLETE · CORE CONFIRMED"));

        let mut board = TuiState::default();
        board.ui.lens = Lens::Board;
        board.runtime.lanes = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        board.runtime.lanes.truncate(1);
        board.runtime.lanes[0].active_session_ids = vec!["session-core".to_string()];
        let lane_id = board.runtime.lanes[0].id.clone();
        let board_rendered = render_frame(&board, 112, 40);
        assert!(board_rendered.contains("LANE BOARD"));
        assert!(board_rendered.contains(&lane_id));
        assert!(board_rendered.contains("session-core"));

        board.ui.lens = Lens::Session;
        board.ui.focused_lane = Some(lane_id.clone());
        board.ui.session_id = "session-core".to_string();
        board.ui.input = "composer stays editable".into();
        let cockpit = render_frame(&board, 112, 40);
        assert!(cockpit.contains("COCKPIT"));
        assert!(cockpit.contains(&lane_id));
        assert!(cockpit.contains("session-core"));
        assert!(cockpit.contains("composer stays editable"));
    }

    #[test]
    fn cockpit_lens_copy_follows_core_resolved_chinese_locale() {
        let mut state = TuiState::default();
        state.runtime.snapshot.ui_preferences.locale = viden_core::LocaleId::ZhCn;
        state.ui.lens = Lens::Board;

        let rendered = render_frame(&state, 112, 40);

        assert!(rendered.contains("LANE BOARD · LANE 看板"));
        assert!(rendered.contains("Core 暂无 lane。"));
    }

    #[test]
    fn active_cockpit_keeps_composer_and_pinned_actions_at_all_widths() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            initial_snapshot: RuntimeSnapshot,
            events: Vec<RuntimeEventEnvelope>,
        }

        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/d1-vertical-slice.json"
        ))
        .expect("D1 fixture");
        let mut runtime = viden_types::RuntimeViewState::new(fixture.initial_snapshot);
        let mut approval = None;
        for envelope in fixture.events {
            if let RuntimeWireEvent::Known(event) = envelope.event {
                if let viden_types::RuntimeEventKind::ApprovalRequested {
                    approval: requested,
                } = &event.kind
                {
                    approval = Some(requested.clone());
                }
                runtime.apply_event(&event);
            }
        }
        runtime.pending_approvals = vec![approval.expect("approval fixture")];
        runtime.active_tool_calls.push(ToolCallView {
            tool_call_id: "tool-active".to_string(),
            name: "search".to_string(),
            input_preview: "src".to_string(),
            owner: None,
        });
        runtime.assistant_stream = "streaming".to_string();
        let mut state = TuiState::new(runtime);
        state.ui.lens = Lens::Session;
        state.ui.focused_lane = Some("lane_d1_core".to_string());
        state.ui.session_id = "session_lane_d1_core".to_string();
        state.ui.input = "edit me".into();

        for width in [40_u16, 80, 112, 160] {
            let rendered = render_frame(&state, width, 40);

            assert!(
                rendered
                    .lines()
                    .all(|line| super::super::text::char_width(line) == usize::from(width)),
                "physical frame width {width}"
            );
            assert!(rendered.contains("edit me"), "composer width {width}");
            assert!(rendered.contains("P:viden"), "project identity {width}");
            assert!(rendered.contains("L:lane_d1_core"), "lane identity {width}");
            assert!(rendered.contains("PERM:Ask"), "permission action {width}");
            // Registered gold gate badge from the TUI status glyph vocabulary.
            assert!(rendered.contains("⏸ 1"), "gate action {width}");
            assert!(rendered.contains("E:1"), "error action {width}");
            assert!(
                !rendered.contains("RUNTIME"),
                "right rail must default closed at {width}"
            );
        }
    }

    #[test]
    fn supervision_loop_is_visible_across_cockpit_decisions_and_gallery() {
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
        runtime.lane_recoveries.push(viden_types::LaneRecoveryView {
            lane_id: "lane-render".to_string(),
            reason: "detached".to_string(),
            next_action: "reattach".to_string(),
            timestamp: None,
        });
        runtime.last_command = Some(viden_types::RuntimeCommandReceipt {
            command_id: "cmd-render".to_string(),
            command: viden_types::RuntimeCommand::CancelActiveTurn,
        });
        runtime.merge_gates[0].decision = Some(viden_types::MergeGateDecision {
            outcome: viden_types::MergeGateDecisionOutcome::Accepted,
            reason: "typed review accepted".to_string(),
            owner: Default::default(),
            evidence_ids: runtime.merge_gates[0].evidence_ids.clone(),
            reviewed_evidence: Vec::new(),
            review_request_id: None,
            audit_id: "audit-render".to_string(),
            decided_at: 1_700_000_060,
        });
        let mut state = TuiState::new(runtime);
        state.ui.lens = Lens::Session;
        state.ui.input = "composer remains available".into();
        state.ui.right_rail_open = true;

        let cockpit = render_frame(&state, 160, 48);
        assert!(cockpit.contains("SUPERVISION"));
        assert!(cockpit.contains("GATE gate_merge · Accepted"));
        assert!(cockpit.contains("RECOVERY lane-render · detached · reattach"));
        assert!(cockpit.contains("COMMAND cmd-render · pending Core fact"));
        assert!(cockpit.contains("composer remains available"));
        assert!(cockpit.contains("CHANGES · EVIDENCE · CONTEXT"));

        state.ui.lens = Lens::Decisions;
        let decisions = render_frame(&state, 120, 40);
        assert!(decisions.contains("GATE gate_merge · Accepted"));
        assert!(decisions.contains("RECOVERY lane-render · detached · reattach"));
        assert!(decisions.contains("COMMAND cmd-render · pending Core fact"));

        state.ui.lens = Lens::Gallery;
        let gallery = render_frame(&state, 120, 40);
        assert!(gallery.contains("EvidenceAccepted"));
        assert!(gallery.contains("gate_merge"));
    }

    #[test]
    fn supervision_strip_leaves_lane_cancel_actions_to_the_side_surface() {
        let mut state = state();
        state.runtime.lanes = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");

        let rows = supervision_rows(&state, 120).join("\n");

        assert!(rows.is_empty());
    }

    #[test]
    fn transcript_wording_cannot_create_runtime_errors() {
        let mut state = state();
        state.ui.entries.push(TuiEntry {
            label: "assistant".to_string(),
            body: "provider failed with invented error".to_string(),
        });
        let without_error = render_ops_frame(&state, 80, 36);

        state.runtime.errors.push(RuntimeErrorView {
            message: "canonical failure".to_string(),
            recoverable: true,
            hint: Some("retry".to_string()),
        });
        let with_error = render_ops_frame(&state, 80, 36);

        assert!(without_error.contains("ERRORS   0"));
        assert!(with_error.contains("ERRORS   1"));
    }

    #[test]
    fn rendered_rows_keep_terminal_cell_width_with_unicode() {
        let mut state = state();
        state.runtime.snapshot.model_label = "模型-👋🏻".to_string();
        state.ui.input = "检查中文输入宽度".into();
        let width = 140usize;

        let rendered = render_frame(&state, width as u16, 36);

        for line in rendered.lines() {
            assert_eq!(super::super::text::char_width(line), width, "{line}");
        }
    }

    #[test]
    fn slash_commands_render_above_composer() {
        let mut state = state();
        state.ui.input = "/p".into();

        let rendered = render_frame(&state, 140, 36);

        assert!(rendered.contains("COMMANDS"));
        assert!(rendered.contains("/permissions"));
    }

    #[test]
    fn render_frame_uses_welcome_layout_for_first_empty_session() {
        let mut state = TuiState::default();
        state.ui.entries.push(TuiEntry {
            label: "system".to_string(),
            body: "Viden TUI ready".to_string(),
        });

        let rendered = render_frame(&state, 140, 40);

        assert!(rendered.contains("Ask anything"));
        assert!(!rendered.contains("TRANSCRIPT"));
    }

    #[test]
    fn render_frame_keeps_live_activity_visible_for_lanes_and_tool_calls() {
        let mut state = state();
        state.runtime.lanes = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .expect("typed lanes");
        state.runtime.active_tool_calls.push(ToolCallView {
            tool_call_id: "tool-1".to_string(),
            name: "search".to_string(),
            input_preview: "src".to_string(),
            owner: None,
        });

        let rendered = render_frame(&state, 140, 40);
        assert!(rendered.contains("LIVE WORK"));
        assert!(rendered.contains("L-start"));
        assert!(rendered.contains("search"));

        state.runtime.snapshot.ui_preferences.locale = viden_core::LocaleId::ZhCn;
        let chinese = render_frame(&state, 140, 40);
        assert!(chinese.contains("LIVE WORK · 实时工作"));
        assert!(chinese.contains("L-start"));
        assert!(chinese.contains("search"));
    }

    #[test]
    fn agent_tasks_do_not_keep_failed_provider_turn_active() {
        let mut state = state();
        state.runtime.errors.push(RuntimeErrorView {
            message: "provider failed".to_string(),
            recoverable: true,
            hint: Some("retry".to_string()),
        });

        assert!(!super::super::state::has_active_work(&state));
    }
}
