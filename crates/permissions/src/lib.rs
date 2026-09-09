use std::path::{Component, Path, PathBuf};

use viden_types::{
    AdditionalWorkingDirectory, ApprovalDecision, ApprovalResponse, ApprovalScope,
    PermissionAllowDecision, PermissionAskDecision, PermissionBehavior, PermissionDecision,
    PermissionDecisionReason, PermissionDenyDecision, PermissionMode, PermissionPrompt,
    PermissionRule, PermissionRuleSource, PermissionRuleValue, ToolInput, ToolSpec,
};

#[derive(Debug, Clone)]
pub struct PermissionContext {
    pub mode: PermissionMode,
    pub additional_working_directories: Vec<AdditionalWorkingDirectory>,
    pub allow_rules: Vec<PermissionRule>,
    pub deny_rules: Vec<PermissionRule>,
    pub ask_rules: Vec<PermissionRule>,
}

impl Default for PermissionContext {
    fn default() -> Self {
        Self {
            mode: PermissionMode::Default,
            additional_working_directories: Vec::new(),
            allow_rules: Vec::new(),
            deny_rules: Vec::new(),
            ask_rules: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PermissionEngine {
    cwd: PathBuf,
    context: PermissionContext,
}

impl PermissionEngine {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        Self {
            cwd: cwd.into(),
            context: PermissionContext::default(),
        }
    }

    pub fn mode(&self) -> PermissionMode {
        self.context.mode
    }

    pub fn context_snapshot(&self) -> PermissionContext {
        self.context.clone()
    }

    pub fn restore_context(&mut self, context: PermissionContext) {
        self.context = context;
    }

    pub fn set_mode(&mut self, mode: PermissionMode) {
        self.context.mode = mode;
    }

    pub fn add_directory(&mut self, path: impl Into<String>, source: PermissionRuleSource) {
        self.context
            .additional_working_directories
            .push(AdditionalWorkingDirectory {
                path: path.into(),
                source,
            });
    }

    pub fn add_rule(&mut self, rule: PermissionRule) {
        match rule.rule_behavior {
            PermissionBehavior::Allow => self.context.allow_rules.push(rule),
            PermissionBehavior::Deny => self.context.deny_rules.push(rule),
            PermissionBehavior::Ask => self.context.ask_rules.push(rule),
        }
    }

    pub fn decide(&self, tool: &ToolSpec, input: &ToolInput) -> PermissionDecision {
        let scoped_paths = extract_paths(input);
        if !scoped_paths.iter().all(|path| self.is_path_in_scope(path)) {
            if tool.name == "git_worktree_add" || tool.name == "git_worktree_remove" {
                return PermissionDecision::Ask(PermissionAskDecision {
                    message: format!(
                        "Approve {} outside the current working directory?",
                        tool.name
                    ),
                    updated_input: None,
                    decision_reason: Some(PermissionDecisionReason::RequiresApproval),
                });
            }
            return PermissionDecision::Deny(PermissionDenyDecision {
                message: "Path is outside the allowed working directory scope".to_string(),
                decision_reason: PermissionDecisionReason::OutOfScopePath,
            });
        }

        if self.matches_rule(&self.context.deny_rules, tool, input) {
            return PermissionDecision::Deny(PermissionDenyDecision {
                message: format!("Denied by permission rule for {}", tool.name),
                decision_reason: PermissionDecisionReason::RuleDeny,
            });
        }

        if self.matches_rule(&self.context.ask_rules, tool, input) {
            return PermissionDecision::Ask(PermissionAskDecision {
                message: format!("Approval required by permission rule for {}", tool.name),
                updated_input: None,
                decision_reason: Some(PermissionDecisionReason::RuleAsk),
            });
        }

        if self.matches_rule(&self.context.allow_rules, tool, input) {
            return PermissionDecision::Allow(PermissionAllowDecision {
                updated_input: None,
                user_modified: false,
                decision_reason: Some(PermissionDecisionReason::RuleAllow),
                accept_feedback: None,
            });
        }

        match self.context.mode {
            PermissionMode::BypassPermissions => {
                PermissionDecision::Allow(PermissionAllowDecision {
                    updated_input: None,
                    user_modified: false,
                    decision_reason: Some(PermissionDecisionReason::BypassMode),
                    accept_feedback: None,
                })
            }
            PermissionMode::DontAsk => PermissionDecision::Allow(PermissionAllowDecision {
                updated_input: None,
                user_modified: false,
                decision_reason: Some(PermissionDecisionReason::DontAskMode),
                accept_feedback: None,
            }),
            PermissionMode::Plan if tool.is_mutating => {
                PermissionDecision::Deny(PermissionDenyDecision {
                    message: format!("{} is blocked while plan mode is active", tool.name),
                    decision_reason: PermissionDecisionReason::PlanMode,
                })
            }
            PermissionMode::AcceptEdits
                if tool.name == "write_file" || tool.name == "edit_file" =>
            {
                PermissionDecision::Allow(PermissionAllowDecision {
                    updated_input: None,
                    user_modified: false,
                    decision_reason: Some(PermissionDecisionReason::AcceptEditsMode),
                    accept_feedback: None,
                })
            }
            _ if !tool.is_mutating && tool.name != "shell" => {
                PermissionDecision::Allow(PermissionAllowDecision {
                    updated_input: None,
                    user_modified: false,
                    decision_reason: Some(PermissionDecisionReason::SafeRead),
                    accept_feedback: None,
                })
            }
            _ => PermissionDecision::Ask(PermissionAskDecision {
                message: format!("Approve {}?", tool.name),
                updated_input: None,
                decision_reason: Some(PermissionDecisionReason::RequiresApproval),
            }),
        }
    }

    pub fn prompt_for(
        tool_name: &str,
        decision: &PermissionAskDecision,
        input: &ToolInput,
    ) -> PermissionPrompt {
        PermissionPrompt {
            tool_name: tool_name.to_string(),
            message: decision.message.clone(),
            input_preview: render_prompt_input(input),
            candidate_paths: candidate_paths(input),
            // The engine decides; it does not read the filesystem. The
            // runtime call site that holds the tool input fills this in
            // before the operator sees the prompt.
            decision_context: None,
        }
    }

    pub fn apply_approval(
        &mut self,
        response: ApprovalResponse,
        decision: &PermissionAskDecision,
        tool: &ToolSpec,
        input: &ToolInput,
    ) -> PermissionDecision {
        if matches!(self.context.mode, PermissionMode::Plan) && tool.is_mutating {
            return PermissionDecision::Deny(PermissionDenyDecision {
                message: format!("{} is blocked while plan mode is active", tool.name),
                decision_reason: PermissionDecisionReason::PlanMode,
            });
        }

        match response.decision {
            ApprovalDecision::Allow { scope } => {
                match scope {
                    ApprovalScope::Once => {}
                    ApprovalScope::Session { .. } => {
                        self.install_allow_rule(tool, input, None);
                    }
                    ApprovalScope::RepoAllowlist { paths } => {
                        if paths.is_empty() {
                            return deny_approval(decision);
                        }
                        let normalized_paths = match self.normalize_repo_allowlist_paths(&paths) {
                            Some(paths) => paths,
                            None => return deny_approval(decision),
                        };
                        let current_paths = extract_paths(input)
                            .into_iter()
                            .map(|path| normalize_path(&self.cwd, &path))
                            .collect::<Vec<_>>();
                        if current_paths.is_empty()
                            || !current_paths
                                .iter()
                                .all(|path| normalized_paths.iter().any(|allowed| allowed == path))
                        {
                            return deny_approval(decision);
                        }
                        for path in normalized_paths {
                            self.install_allow_rule(
                                tool,
                                input,
                                Some(exact_path_rule_content(&path)),
                            );
                        }
                    }
                }
                PermissionDecision::Allow(PermissionAllowDecision {
                    updated_input: decision.updated_input.clone(),
                    user_modified: false,
                    decision_reason: decision.decision_reason.clone(),
                    accept_feedback: response.feedback,
                })
            }
            ApprovalDecision::Deny => deny_approval(decision),
        }
    }

    fn normalize_repo_allowlist_paths(&self, paths: &[String]) -> Option<Vec<PathBuf>> {
        let mut normalized = Vec::new();
        for path in paths {
            let resolved = normalize_path(&self.cwd, path);
            if !self.is_normalized_path_in_scope(&resolved) {
                return None;
            }
            if !normalized.contains(&resolved) {
                normalized.push(resolved);
            }
        }
        Some(normalized)
    }

    fn install_allow_rule(
        &mut self,
        tool: &ToolSpec,
        input: &ToolInput,
        content_override: Option<String>,
    ) {
        self.context.allow_rules.push(PermissionRule {
            source: PermissionRuleSource::Session,
            rule_behavior: PermissionBehavior::Allow,
            rule_value: PermissionRuleValue {
                tool_name: tool.name.clone(),
                rule_content: Some(
                    content_override.unwrap_or_else(|| viden_types::encode_tool_input(input)),
                ),
            },
        });
    }

    fn matches_rule(&self, rules: &[PermissionRule], tool: &ToolSpec, input: &ToolInput) -> bool {
        let rendered = viden_types::encode_tool_input(input);
        let mut exact_paths = Vec::new();
        let mut matched_non_path_rule = false;
        for rule in rules
            .iter()
            .filter(|rule| rule.rule_value.tool_name == tool.name)
        {
            match &rule.rule_value.rule_content {
                Some(expected) if expected.starts_with(EXACT_PATH_RULE_PREFIX) => {
                    exact_paths.push(PathBuf::from(
                        expected.trim_start_matches(EXACT_PATH_RULE_PREFIX),
                    ));
                }
                Some(expected) if rendered.contains(expected) => matched_non_path_rule = true,
                None => matched_non_path_rule = true,
                Some(_) => {}
            }
        }
        if matched_non_path_rule {
            return true;
        }
        let current_paths = extract_paths(input)
            .into_iter()
            .map(|path| normalize_path(&self.cwd, &path))
            .collect::<Vec<_>>();
        !current_paths.is_empty()
            && current_paths
                .iter()
                .all(|path| exact_paths.iter().any(|allowed| allowed == path))
    }

    fn is_path_in_scope(&self, raw: &str) -> bool {
        let resolved = normalize_path(&self.cwd, raw);
        self.is_normalized_path_in_scope(&resolved)
    }

    fn is_normalized_path_in_scope(&self, resolved: &Path) -> bool {
        if resolved.starts_with(&self.cwd) {
            return true;
        }
        self.context
            .additional_working_directories
            .iter()
            .map(|directory| normalize_path(&self.cwd, &directory.path))
            .any(|directory| resolved.starts_with(directory))
    }
}

const EXACT_PATH_RULE_PREFIX: &str = "path_exact:";

fn deny_approval(decision: &PermissionAskDecision) -> PermissionDecision {
    PermissionDecision::Deny(PermissionDenyDecision {
        message: "User denied the permission request".to_string(),
        decision_reason: decision
            .decision_reason
            .clone()
            .unwrap_or(PermissionDecisionReason::RequiresApproval),
    })
}

fn render_prompt_input(input: &ToolInput) -> String {
    if input.is_empty() {
        return "  <no input>".to_string();
    }
    input
        .iter()
        .map(|(key, value)| format!("  {key}: {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_paths(input: &ToolInput) -> Vec<String> {
    ["path", "from", "to"]
        .iter()
        .filter_map(|key| input.get(*key).cloned())
        .collect()
}

// Keep path candidates structural so approval UIs do not scrape display text.
fn candidate_paths(input: &ToolInput) -> Vec<String> {
    extract_paths(input)
}

fn normalize_path(cwd: &Path, raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    let joined = if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn exact_path_rule_content(path: &Path) -> String {
    format!("{EXACT_PATH_RULE_PREFIX}{}", path.display())
}

/// The minimal engine surface the shared permission gate needs. Implemented
/// by the plain [`PermissionEngine`] (lane approval re-checks, ACP-local
/// engines) and by the runtime's shared session-engine handle.
pub trait PermissionDecider {
    fn decide(&self, tool: &ToolSpec, input: &ToolInput) -> PermissionDecision;
    fn apply_approval(
        &mut self,
        response: ApprovalResponse,
        ask: &PermissionAskDecision,
        tool: &ToolSpec,
        input: &ToolInput,
    ) -> PermissionDecision;
}

impl PermissionDecider for PermissionEngine {
    fn decide(&self, tool: &ToolSpec, input: &ToolInput) -> PermissionDecision {
        PermissionEngine::decide(self, tool, input)
    }

    fn apply_approval(
        &mut self,
        response: ApprovalResponse,
        ask: &PermissionAskDecision,
        tool: &ToolSpec,
        input: &ToolInput,
    ) -> PermissionDecision {
        PermissionEngine::apply_approval(self, response, ask, tool, input)
    }
}

/// Resolve a tool permission through the shared gate.
///
/// Every call site that resolves a permission interactively must produce the
/// same fail-closed sequence: a pure `decide()`, an operator prompt only for
/// `Ask`, and an `apply_approval()` whose plan-mode re-check can still deny an
/// operator "allow". Hand-writing that sequence per call site is exactly how a
/// future site forgets a step, so this is the one place it exists — shared by
/// `viden-runtime` and by the external agent adapters in `viden-agents`.
///
/// `ask_flow` runs only when `decide()` returns `Ask`. It receives the ask
/// decision and the rendered prompt, performs the call-site-specific work
/// (transcript entries, runtime events, task updates) around obtaining the
/// operator response, and returns that response. The gate then applies it via
/// `apply_approval`, which re-checks plan mode; the returned decision is never
/// `Ask`, so callers may treat that arm as unreachable.
pub fn resolve_permission<D: PermissionDecider>(
    decider: &mut D,
    tool: &ToolSpec,
    prompt_tool_name: &str,
    input: &ToolInput,
    ask_flow: impl FnOnce(&PermissionAskDecision, PermissionPrompt) -> ApprovalResponse,
) -> PermissionDecision {
    let decision = decider.decide(tool, input);
    let PermissionDecision::Ask(ask) = decision else {
        return decision;
    };
    let prompt = PermissionEngine::prompt_for(prompt_tool_name, &ask, input);
    let response = ask_flow(&ask, prompt);
    decider.apply_approval(response, &ask, tool, input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use viden_types::{
        ApprovalDecision, ApprovalResponse, ApprovalScope, PermissionRuleValue, SessionId,
        ToolInput, ToolSpec,
    };

    fn tool(name: &str, is_mutating: bool) -> ToolSpec {
        ToolSpec {
            name: name.to_string(),
            description: name.to_string(),
            is_mutating,
            input_schema_hint: String::new(),
        }
    }

    fn input(path: &str) -> ToolInput {
        let mut input = ToolInput::new();
        input.insert("path".to_string(), path.to_string());
        input
    }

    #[test]
    fn default_mode_allows_reads_in_scope() {
        let engine = PermissionEngine::new("/tmp/project");
        let decision = engine.decide(&tool("read_file", false), &input("src/main.rs"));
        assert!(matches!(decision, PermissionDecision::Allow(_)));
    }

    #[test]
    fn default_mode_asks_for_mutations() {
        let engine = PermissionEngine::new("/tmp/project");
        let decision = engine.decide(&tool("write_file", true), &input("src/main.rs"));
        assert!(matches!(decision, PermissionDecision::Ask(_)));
    }

    #[test]
    fn plan_mode_denies_mutations() {
        let mut engine = PermissionEngine::new("/tmp/project");
        engine.set_mode(PermissionMode::Plan);
        let decision = engine.decide(&tool("write_file", true), &input("src/main.rs"));
        assert!(matches!(decision, PermissionDecision::Deny(_)));
    }

    #[test]
    fn allow_rule_overrides_default_behavior() {
        let mut engine = PermissionEngine::new("/tmp/project");
        engine.add_rule(PermissionRule {
            source: PermissionRuleSource::Session,
            rule_behavior: PermissionBehavior::Allow,
            rule_value: PermissionRuleValue {
                tool_name: "shell".to_string(),
                rule_content: Some("cargo test".to_string()),
            },
        });
        let mut shell_input = ToolInput::new();
        shell_input.insert("command".into(), "cargo test".into());
        let decision = engine.decide(&tool("shell", true), &shell_input);
        assert!(matches!(decision, PermissionDecision::Allow(_)));
    }

    #[test]
    fn additional_directory_expands_scope() {
        let mut engine = PermissionEngine::new("/tmp/project");
        engine.add_directory("/tmp/shared", PermissionRuleSource::Session);
        let decision = engine.decide(&tool("read_file", false), &input("/tmp/shared/file.txt"));
        assert!(matches!(decision, PermissionDecision::Allow(_)));
    }

    #[test]
    fn git_worktree_out_of_scope_path_asks_instead_of_denying() {
        let engine = PermissionEngine::new("/tmp/project");
        let decision = engine.decide(
            &tool("git_worktree_add", true),
            &input("/tmp/project-worktrees/feature-demo"),
        );
        assert!(matches!(decision, PermissionDecision::Ask(_)));
    }

    #[test]
    fn permission_prompt_renders_input_as_stable_fields() {
        let mut input = ToolInput::new();
        input.insert("path".to_string(), "hello.py".to_string());
        input.insert("content".to_string(), "print('Hello')".to_string());
        let decision = PermissionAskDecision {
            message: "Approve write_file?".to_string(),
            updated_input: None,
            decision_reason: Some(PermissionDecisionReason::RequiresApproval),
        };

        let prompt = PermissionEngine::prompt_for("write_file", &decision, &input);

        assert_eq!(prompt.tool_name, "write_file");
        assert_eq!(prompt.message, "Approve write_file?");
        assert_eq!(
            prompt.input_preview,
            "  content: print('Hello')\n  path: hello.py"
        );
        assert_eq!(prompt.candidate_paths, vec!["hello.py"]);
    }

    #[test]
    fn approval_scope_once_allows_current_effect_without_persisting_rule() {
        let mut engine = PermissionEngine::new("/tmp/project");
        let tool = tool("shell", true);
        let mut input = ToolInput::new();
        input.insert("command".into(), "cargo test".into());
        let PermissionDecision::Ask(ask) = engine.decide(&tool, &input) else {
            panic!("shell should require approval before the once response");
        };

        let decision =
            engine.apply_approval(ApprovalResponse::allow_once(None), &ask, &tool, &input);

        assert!(matches!(decision, PermissionDecision::Allow(_)));
        assert!(engine.context_snapshot().allow_rules.is_empty());
        assert!(matches!(
            engine.decide(&tool, &input),
            PermissionDecision::Ask(_)
        ));
    }

    #[test]
    fn approval_scope_session_installs_allow_rule_before_followup_decision() {
        let mut engine = PermissionEngine::new("/tmp/project");
        let tool = tool("shell", true);
        let mut input = ToolInput::new();
        input.insert("command".into(), "cargo test".into());
        let PermissionDecision::Ask(ask) = engine.decide(&tool, &input) else {
            panic!("shell should require approval before the session response");
        };
        let session_id: SessionId = "session-a".into();

        let decision = engine.apply_approval(
            ApprovalResponse {
                decision: ApprovalDecision::Allow {
                    scope: ApprovalScope::Session { session_id },
                },
                feedback: None,
            },
            &ask,
            &tool,
            &input,
        );

        assert!(matches!(decision, PermissionDecision::Allow(_)));
        assert_eq!(engine.context_snapshot().allow_rules.len(), 1);
        assert!(matches!(
            engine.decide(&tool, &input),
            PermissionDecision::Allow(_)
        ));
    }

    #[test]
    fn approval_scope_repo_allowlist_installs_explicit_paths_and_rejects_empty_lists() {
        let mut engine = PermissionEngine::new("/tmp/project");
        let tool = tool("write_file", true);
        let scoped = input("src/lib.rs");
        let PermissionDecision::Ask(ask) = engine.decide(&tool, &scoped) else {
            panic!("write_file should require approval before the repo allowlist response");
        };

        let allow = engine.apply_approval(
            ApprovalResponse {
                decision: ApprovalDecision::Allow {
                    scope: ApprovalScope::RepoAllowlist {
                        paths: vec!["src/lib.rs".into()],
                    },
                },
                feedback: None,
            },
            &ask,
            &tool,
            &scoped,
        );

        assert!(matches!(allow, PermissionDecision::Allow(_)));
        assert!(matches!(
            engine.decide(&tool, &scoped),
            PermissionDecision::Allow(_)
        ));

        let unscoped = input("src/other.rs");
        assert!(matches!(
            engine.decide(&tool, &unscoped),
            PermissionDecision::Ask(_)
        ));

        let mut empty_engine = PermissionEngine::new("/tmp/project");
        let PermissionDecision::Ask(empty_ask) = empty_engine.decide(&tool, &scoped) else {
            panic!("write_file should require approval before empty allowlist response");
        };
        let empty = empty_engine.apply_approval(
            ApprovalResponse {
                decision: ApprovalDecision::Allow {
                    scope: ApprovalScope::RepoAllowlist { paths: Vec::new() },
                },
                feedback: None,
            },
            &empty_ask,
            &tool,
            &scoped,
        );
        assert!(matches!(empty, PermissionDecision::Deny(_)));
        assert!(empty_engine.context_snapshot().allow_rules.is_empty());
    }

    #[test]
    fn approval_scope_repo_allowlist_uses_normalized_exact_path_matching() {
        let mut engine = PermissionEngine::new("/tmp/project");
        let tool = tool("write_file", true);
        let scoped = input("./src/../src/lib.rs");
        let PermissionDecision::Ask(ask) = engine.decide(&tool, &scoped) else {
            panic!("write_file should require approval before repo allowlist response");
        };

        let allow = engine.apply_approval(
            ApprovalResponse {
                decision: ApprovalDecision::Allow {
                    scope: ApprovalScope::RepoAllowlist {
                        paths: vec!["src/lib.rs".into()],
                    },
                },
                feedback: None,
            },
            &ask,
            &tool,
            &scoped,
        );
        assert!(matches!(allow, PermissionDecision::Allow(_)));

        assert!(matches!(
            engine.decide(&tool, &input("src/lib.rs")),
            PermissionDecision::Allow(_)
        ));
        assert!(matches!(
            engine.decide(&tool, &input("src/lib.rs.bak")),
            PermissionDecision::Ask(_)
        ));

        let mut unrelated = ToolInput::new();
        unrelated.insert("path".to_string(), "src/other.rs".to_string());
        unrelated.insert("content".to_string(), "src/lib.rs".to_string());
        assert!(matches!(
            engine.decide(&tool, &unrelated),
            PermissionDecision::Ask(_)
        ));
    }

    #[test]
    fn approval_scope_repo_allowlist_requires_all_current_paths_to_match_exact_rules() {
        let mut engine = PermissionEngine::new("/tmp/project");
        let tool = tool("move_file", true);
        let mut move_input = ToolInput::new();
        move_input.insert("from".to_string(), "src/from.rs".to_string());
        move_input.insert("to".to_string(), "src/to.rs".to_string());
        let PermissionDecision::Ask(ask) = engine.decide(&tool, &move_input) else {
            panic!("move_file should require approval before repo allowlist response");
        };

        let allow = engine.apply_approval(
            ApprovalResponse {
                decision: ApprovalDecision::Allow {
                    scope: ApprovalScope::RepoAllowlist {
                        paths: vec!["src/from.rs".into(), "src/to.rs".into()],
                    },
                },
                feedback: None,
            },
            &ask,
            &tool,
            &move_input,
        );
        assert!(matches!(allow, PermissionDecision::Allow(_)));

        assert!(matches!(
            engine.decide(&tool, &move_input),
            PermissionDecision::Allow(_)
        ));

        let mut partial = ToolInput::new();
        partial.insert("from".to_string(), "src/from.rs".to_string());
        partial.insert("to".to_string(), "src/other.rs".to_string());
        assert!(matches!(
            engine.decide(&tool, &partial),
            PermissionDecision::Ask(_)
        ));
    }

    #[test]
    fn approval_scope_repo_allowlist_rejects_out_of_scope_and_supports_additional_dirs() {
        let tool = tool("write_file", true);
        let scoped = input("src/lib.rs");
        let mut denied_engine = PermissionEngine::new("/tmp/project");
        let PermissionDecision::Ask(ask) = denied_engine.decide(&tool, &scoped) else {
            panic!("write_file should require approval before repo allowlist response");
        };
        let denied = denied_engine.apply_approval(
            ApprovalResponse {
                decision: ApprovalDecision::Allow {
                    scope: ApprovalScope::RepoAllowlist {
                        paths: vec!["/tmp/outside.rs".into()],
                    },
                },
                feedback: None,
            },
            &ask,
            &tool,
            &scoped,
        );
        assert!(matches!(denied, PermissionDecision::Deny(_)));
        assert!(denied_engine.context_snapshot().allow_rules.is_empty());

        let mut additional_engine = PermissionEngine::new("/tmp/project");
        additional_engine.add_directory("/tmp/shared", PermissionRuleSource::Session);
        let additional_input = input("/tmp/shared/file.rs");
        let PermissionDecision::Ask(additional_ask) =
            additional_engine.decide(&tool, &additional_input)
        else {
            panic!("additional-dir write should require approval before allowlist response");
        };
        let allow = additional_engine.apply_approval(
            ApprovalResponse {
                decision: ApprovalDecision::Allow {
                    scope: ApprovalScope::RepoAllowlist {
                        paths: vec!["/tmp/shared/./file.rs".into()],
                    },
                },
                feedback: None,
            },
            &additional_ask,
            &tool,
            &additional_input,
        );
        assert!(matches!(allow, PermissionDecision::Allow(_)));
        assert!(matches!(
            additional_engine.decide(&tool, &additional_input),
            PermissionDecision::Allow(_)
        ));
    }

    #[test]
    fn approval_scope_explicit_deny_installs_no_allow_rule() {
        let mut engine = PermissionEngine::new("/tmp/project");
        let tool = tool("shell", true);
        let mut input = ToolInput::new();
        input.insert("command".into(), "cargo test".into());
        let PermissionDecision::Ask(ask) = engine.decide(&tool, &input) else {
            panic!("shell should require approval before the deny response");
        };

        let decision = engine.apply_approval(ApprovalResponse::deny(None), &ask, &tool, &input);

        assert!(matches!(decision, PermissionDecision::Deny(_)));
        assert!(engine.context_snapshot().allow_rules.is_empty());
    }

    #[test]
    fn approval_scope_plan_mode_denies_approval_without_installing_rule() {
        let mut engine = PermissionEngine::new("/tmp/project");
        let tool = tool("write_file", true);
        let scoped = input("src/lib.rs");
        let PermissionDecision::Ask(ask) = engine.decide(&tool, &scoped) else {
            panic!("write_file should require approval before plan-mode response");
        };
        engine.set_mode(PermissionMode::Plan);

        let decision = engine.apply_approval(
            ApprovalResponse {
                decision: ApprovalDecision::Allow {
                    scope: ApprovalScope::Session {
                        session_id: "session-a".to_string(),
                    },
                },
                feedback: None,
            },
            &ask,
            &tool,
            &scoped,
        );

        assert!(matches!(
            decision,
            PermissionDecision::Deny(PermissionDenyDecision {
                decision_reason: PermissionDecisionReason::PlanMode,
                ..
            })
        ));
        assert!(engine.context_snapshot().allow_rules.is_empty());
    }
}
