use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;

use viden_provider::ModelProvider;
use viden_types::{ModelEvent, ModelRequest, fresh_id};

mod audit_runtime_tests;
mod decide_review_tests;
mod frontend_services_tests;
mod git_command_tests;
mod interrupted_turn_recovery_tests;
mod lane_supervisor_tests;
mod live_deepseek_tests;
mod lsp_command_tests;
mod lsp_render_tests;
mod operator_git_tests;
mod permission_gate_tests;
mod project_runtime_tests;
mod runtime_command_tests;
mod runtime_contract_tests;
mod runtime_loop_tests;
mod runtime_supervisor_tests;
mod session_command_tests;
mod structured_diff_tests;
mod transcript_contract_tests;
mod trust_loop_tests;
mod workflow_command_tests;
mod workspace_files_tests;

struct SequenceProvider {
    model: String,
    turns: VecDeque<Vec<ModelEvent>>,
}

impl SequenceProvider {
    fn new(turns: Vec<Vec<ModelEvent>>) -> Self {
        Self {
            model: "test-model".to_string(),
            turns: turns.into(),
        }
    }
}

impl ModelProvider for SequenceProvider {
    fn provider_name(&self) -> &str {
        "sequence"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn set_model(&mut self, model: String) {
        self.model = model;
    }

    fn next_events(&mut self, _request: &ModelRequest) -> Result<Vec<ModelEvent>, String> {
        Ok(self
            .turns
            .pop_front()
            .unwrap_or_else(|| vec![ModelEvent::Done]))
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("viden_runtime_{name}_{}", fresh_id("tmp")));
    fs::create_dir_all(&dir).unwrap();
    dir
}
