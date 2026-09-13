//! Shared permission gate: the decide -> ask -> apply_approval sequence,
//! written once.
//!
//! Every call site that resolves a permission interactively must produce the
//! same fail-closed sequence: a pure `decide()`, an operator prompt only for
//! `Ask`, and an `apply_approval()` whose plan-mode re-check can still deny an
//! operator "allow" (the double checkpoint in `viden-permissions`). Hand-writing
//! that sequence per call site is exactly how a future site forgets a step, so
//! [`resolve`] is the one place it exists. Sites keep their own transcript,
//! event, and rendering work inside the `ask_flow` closure so observable
//! behavior stays byte-identical to the pre-gate code.
//!
//! The sequence itself is `viden_permissions::resolve_permission`, re-exported
//! here as [`resolve`]. It lives next to the engine whose three calls it orders
//! because `viden-agents` gates its ACP reverse-RPC filesystem and terminal
//! mutations through the same function and must not import this crate. What
//! stays here is the runtime-shaped half: [`SharedPermissionEngine`] and the
//! registry backstop below.
//!
//! [`PermissionBackstopInterceptor`] is the structural second line: registered
//! on the `ToolRegistry`, it re-checks the pure `decide()` immediately before
//! any tool executes, so a call site that forgets the gate entirely still
//! cannot mutate against a `Deny` (plan mode included) or an unresolved `Ask`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, RwLock};

use viden_permissions::{PermissionContext, PermissionEngine};
use viden_tools::{InterceptVerdict, ToolExecutionContext, ToolExecutionInterceptor};
use viden_types::{
    ApprovalResponse, PermissionAskDecision, PermissionDecision, PermissionMode, PermissionRule,
    ToolCall, ToolInput, ToolResult, ToolSpec,
};

// The decide -> ask -> apply_approval sequence itself lives in
// `viden-permissions`, next to the engine whose three calls it orders, because
// `viden-agents` resolves its ACP reverse-RPC mutations through the same gate
// and must not import the runtime. Re-exported under the local name so every
// call site in this crate still reads `permission_gate::resolve`.
pub(crate) use viden_permissions::{PermissionDecider, resolve_permission as resolve};

/// Upper bound for remembered interactive clearances. One clearance is
/// consumed by the registry execution that immediately follows its approval;
/// the cap only bounds entries whose approved call never reached the registry
/// (for example `context_read`, which executes outside `ToolRegistry`).
const MAX_EXECUTION_CLEARANCES: usize = 32;

/// The session engine's permission engine behind a shared handle, so the
/// registry-level [`PermissionBackstopInterceptor`] always observes the
/// current mode and rules, including engines swapped in wholesale by
/// `set_permission_mode`/`set_work_mode`.
///
/// It also records one-shot execution clearances: an interactive `Ask`
/// resolved to `Allow` with scope `Once` installs no rule, so a pure
/// re-`decide()` would still say `Ask`. The clearance is how the backstop
/// distinguishes that approved call from one that skipped approval entirely.
#[derive(Clone)]
pub(crate) struct SharedPermissionEngine {
    engine: Arc<RwLock<PermissionEngine>>,
    cleared_executions: Arc<Mutex<VecDeque<String>>>,
}

impl SharedPermissionEngine {
    pub(crate) fn new(engine: PermissionEngine) -> Self {
        Self {
            engine: Arc::new(RwLock::new(engine)),
            cleared_executions: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, PermissionEngine> {
        self.engine
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, PermissionEngine> {
        self.engine
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn mode(&self) -> PermissionMode {
        self.read().mode()
    }

    pub(crate) fn set_mode(&self, mode: PermissionMode) {
        self.write().set_mode(mode);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn add_rule(&self, rule: PermissionRule) {
        self.write().add_rule(rule);
    }

    pub(crate) fn context_snapshot(&self) -> PermissionContext {
        self.read().context_snapshot()
    }

    pub(crate) fn restore_context(&self, context: PermissionContext) {
        self.write().restore_context(context);
    }

    /// Clone of the current engine for consumers that need a detached copy
    /// (lane workers, prepare-then-commit mode changes).
    pub(crate) fn engine_snapshot(&self) -> PermissionEngine {
        self.read().clone()
    }

    /// Swap in a fully prepared engine. Used by the durable-first mode
    /// changes that build the next engine, persist metadata, then publish.
    pub(crate) fn replace_engine(&self, engine: PermissionEngine) {
        *self.write() = engine;
    }

    pub(crate) fn decide(&self, tool: &ToolSpec, input: &ToolInput) -> PermissionDecision {
        self.read().decide(tool, input)
    }

    /// Installs a standing `Always` rule in the live engine
    /// (`runtime.approval_scopes`, GUI-CORE-019). Idempotent, so bootstrap
    /// loading and a repeated grant converge on one rule.
    pub(crate) fn install_standing_allow_rule(&self, tool_name: &str, rule_content: &str) {
        self.write()
            .install_standing_allow_rule(tool_name, rule_content);
    }

    /// Removes a standing `Always` rule from the live engine. Returns whether
    /// one was in force.
    pub(crate) fn remove_standing_allow_rule(&self, tool_name: &str, rule_content: &str) -> bool {
        self.write()
            .remove_standing_allow_rule(tool_name, rule_content)
    }

    pub(crate) fn apply_approval(
        &self,
        response: ApprovalResponse,
        ask: &PermissionAskDecision,
        tool: &ToolSpec,
        input: &ToolInput,
    ) -> PermissionDecision {
        let decision = self.write().apply_approval(response, ask, tool, input);
        // Remember interactively resolved allows so the registry backstop can
        // tell an approved once-off from a call that skipped approval.
        if let PermissionDecision::Allow(allow) = &decision {
            // An `Edit` runs the *revised* bytes, so the clearance the
            // backstop will look for is keyed on those, not on the input the
            // operator was first shown.
            let cleared_input = allow.updated_input.as_ref().unwrap_or(input);
            let mut cleared = self
                .cleared_executions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            cleared.push_back(clearance_key(tool, cleared_input));
            while cleared.len() > MAX_EXECUTION_CLEARANCES {
                cleared.pop_front();
            }
        }
        decision
    }

    fn take_execution_clearance(&self, tool: &ToolSpec, input: &ToolInput) -> bool {
        let key = clearance_key(tool, input);
        let mut cleared = self
            .cleared_executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match cleared.iter().position(|entry| *entry == key) {
            Some(index) => {
                cleared.remove(index);
                true
            }
            None => false,
        }
    }
}

impl PermissionDecider for SharedPermissionEngine {
    fn decide(&self, tool: &ToolSpec, input: &ToolInput) -> PermissionDecision {
        SharedPermissionEngine::decide(self, tool, input)
    }

    fn apply_approval(
        &mut self,
        response: ApprovalResponse,
        ask: &PermissionAskDecision,
        tool: &ToolSpec,
        input: &ToolInput,
    ) -> PermissionDecision {
        SharedPermissionEngine::apply_approval(self, response, ask, tool, input)
    }
}

fn clearance_key(tool: &ToolSpec, input: &ToolInput) -> String {
    // Unit separator keeps tool name and encoded input unambiguous.
    format!(
        "{}\u{1f}{}",
        tool.name,
        viden_types::encode_tool_input(input)
    )
}

/// Defense-in-depth registry interceptor: re-checks the pure `decide()` right
/// before a mutating tool executes. It never prompts; `Ask` here means the
/// call site failed to resolve approval (no recorded clearance), so the call
/// is rejected fail-closed. This makes permission-before-mutation structural:
/// a future call site that forgets the gate still cannot mutate in plan mode
/// or against a deny rule.
pub(crate) struct PermissionBackstopInterceptor {
    permissions: SharedPermissionEngine,
}

impl PermissionBackstopInterceptor {
    pub(crate) fn new(permissions: SharedPermissionEngine) -> Self {
        Self { permissions }
    }
}

impl ToolExecutionInterceptor for PermissionBackstopInterceptor {
    fn before_execute(
        &self,
        spec: &ToolSpec,
        call: &ToolCall,
        _ctx: &ToolExecutionContext,
    ) -> InterceptVerdict {
        // Non-mutating tools cannot violate permission-before-mutation; the
        // interactive gate still governs their ask flows (for example shell
        // prompts) at the call site.
        if !spec.is_mutating {
            return InterceptVerdict::Proceed;
        }
        match self.permissions.decide(spec, &call.input) {
            PermissionDecision::Allow(_) => {
                // Consume a matching clearance if one exists so a rule-backed
                // allow does not leave a stale once-off entry behind.
                let _ = self.permissions.take_execution_clearance(spec, &call.input);
                InterceptVerdict::Proceed
            }
            PermissionDecision::Ask(_) => {
                if self.permissions.take_execution_clearance(spec, &call.input) {
                    InterceptVerdict::Proceed
                } else {
                    InterceptVerdict::Reject {
                        message: format!(
                            "permission backstop blocked `{}`: approval was not resolved before execution",
                            spec.name
                        ),
                    }
                }
            }
            PermissionDecision::Deny(deny) => InterceptVerdict::Reject {
                message: format!(
                    "permission backstop blocked `{}`: {}",
                    spec.name, deny.message
                ),
            },
        }
    }

    fn after_execute(&self, _call: &ToolCall, _result: &ToolResult) {}
}
