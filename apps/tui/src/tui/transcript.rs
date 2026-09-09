use super::{
    state::{TuiEntry, TuiState},
    text::wrap_words,
};

pub(super) fn transcript_rows(state: &TuiState, width: usize) -> Vec<String> {
    let mut rows = Vec::new();
    for entry in &state.ui.entries {
        append_entry(&mut rows, entry, width);
    }
    if !state.runtime.assistant_stream.is_empty() {
        append_entry(
            &mut rows,
            &TuiEntry {
                label: "assistant".to_string(),
                body: state.runtime.assistant_stream.clone(),
            },
            width,
        );
    }
    for tool in &state.runtime.active_tool_calls {
        append_entry(
            &mut rows,
            &TuiEntry {
                label: format!("tool-call {}", tool.tool_call_id),
                body: format!("{}\n{}", tool.name, tool.input_preview),
            },
            width,
        );
    }
    for input in &state.runtime.queued_inputs {
        append_entry(
            &mut rows,
            &TuiEntry {
                label: format!("queued {}", input.id),
                body: input.content_preview.clone(),
            },
            width,
        );
    }
    for output in &state.runtime.lane_outputs {
        append_entry(
            &mut rows,
            &TuiEntry {
                label: format!("lane-output {} {}", output.lane_id, output.stream),
                body: output.content.clone(),
            },
            width,
        );
    }
    for conflict in &state.runtime.lane_conflicts {
        append_entry(
            &mut rows,
            &TuiEntry {
                label: format!("lane-conflict {}", conflict.lane_id),
                body: format!("{}\n{}", conflict.summary, conflict.paths.join(", ")),
            },
            width,
        );
    }
    for recovery in &state.runtime.lane_recoveries {
        append_entry(
            &mut rows,
            &TuiEntry {
                label: format!("lane-recovery {}", recovery.lane_id),
                body: format!("{}\nnext {}", recovery.reason, recovery.next_action),
            },
            width,
        );
    }
    for evidence in &state.runtime.latest_evidence {
        append_entry(
            &mut rows,
            &TuiEntry {
                label: "evidence".to_string(),
                body: evidence.summary.clone(),
            },
            width,
        );
    }
    for error in &state.runtime.errors {
        append_entry(
            &mut rows,
            &TuiEntry {
                label: "error".to_string(),
                body: error.message.clone(),
            },
            width,
        );
    }
    rows
}

fn append_entry(rows: &mut Vec<String>, entry: &TuiEntry, width: usize) {
    rows.push(entry.label.to_ascii_uppercase());
    rows.extend(
        wrap_words(&entry.body, width.saturating_sub(2))
            .into_iter()
            .map(|line| format!("  {line}")),
    );
    rows.push(String::new());
}

#[cfg(test)]
mod tests {
    use super::*;
    use viden_types::{
        LaneConflictView, LaneOutputView, LaneRecoveryView, QueuedInputView, ToolCallView,
    };

    #[test]
    fn transcript_copy_cannot_invent_runtime_errors() {
        let mut state = TuiState::default();
        state.ui.entries.push(TuiEntry {
            label: "assistant".to_string(),
            body: "ERROR fake".to_string(),
        });
        assert!(state.runtime.errors.is_empty());
        assert!(
            transcript_rows(&state, 40)
                .join("\n")
                .contains("ERROR fake")
        );
    }

    #[test]
    fn structured_supervision_timeline_keeps_owner_and_recovery_ids_visible() {
        let mut state = TuiState::default();
        state.runtime.assistant_stream = "streaming answer".to_string();
        state.runtime.active_tool_calls.push(ToolCallView {
            tool_call_id: "tool-41".to_string(),
            name: "cargo".to_string(),
            input_preview: "cargo test".to_string(),
            owner: None,
        });
        state.runtime.queued_inputs.push(QueuedInputView {
            id: "queue-17".to_string(),
            content_preview: "next task".to_string(),
            created_at: Some(1),
            owner: None,
        });
        state.runtime.lane_outputs.push(LaneOutputView {
            lane_id: "lane-core".to_string(),
            stream: "stdout".to_string(),
            content: "compiler output".to_string(),
            timestamp: Some(2),
        });
        state.runtime.lane_conflicts.push(LaneConflictView {
            lane_id: "lane-core".to_string(),
            summary: "conflict waiting for Core".to_string(),
            paths: vec!["src/lib.rs".to_string()],
            timestamp: Some(3),
            content: None,
        });
        state.runtime.lane_recoveries.push(LaneRecoveryView {
            lane_id: "lane-core".to_string(),
            reason: "worker disconnected".to_string(),
            next_action: "reconnect and replay".to_string(),
            timestamp: Some(4),
        });

        let rendered = transcript_rows(&state, 80).join("\n");

        for expected in [
            "streaming answer",
            "TOOL-41",
            "QUEUE-17",
            "LANE-CORE",
            "conflict waiting for Core",
            "reconnect and replay",
        ] {
            assert!(
                rendered.contains(expected),
                "missing {expected}:\n{rendered}"
            );
        }
    }
}
