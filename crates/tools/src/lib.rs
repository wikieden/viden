use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use viden_types::{ToolCall, ToolInput, ToolResult, ToolSpec};

mod capability;
mod files;
mod git;
pub mod lane;
mod lsp;
pub mod patch;
pub mod process;
mod search;
mod shell;
mod web;

pub use capability::{
    FilesystemCapability, InteractiveInvocation, InteractiveProcess, InteractiveProcessControl,
    LocalFilesystem, LocalProcess, ProcessCapability, ProcessInvocation, ProcessOutput,
};
pub use files::render_diff;
use files::{EditFileTool, ReadFileTool, WriteFileTool};
use git::{
    GitAddTool, GitBranchTool, GitCommitTool, GitDiffTool, GitPushTool, GitRestoreTool,
    GitStashDropTool, GitStashListTool, GitStashPopTool, GitStashPushTool, GitStatusTool,
    GitSwitchTool, GitWorktreeAddTool, GitWorktreeListTool, GitWorktreeRemoveTool,
};
use lsp::{LspDiagnosticsTool, LspReferencesTool, LspSymbolsTool};
use search::{GlobTool, GrepTool};
use shell::ShellTool;
pub use shell::{build_shell_invocation, shell_requires_stdin};
use web::{WebFetchTool, WebSearchTool};
#[cfg(test)]
pub(crate) use web::{html_to_text, parse_duckduckgo_results, url_encode};

#[derive(Clone)]
pub struct ToolExecutionContext {
    pub cwd: PathBuf,
    pub semantic: Option<Arc<dyn SemanticToolProvider>>,
    /// OS capability seam: tools must reach the filesystem and processes only
    /// through these providers so one swap relocates every consumer together.
    pub fs: Arc<dyn FilesystemCapability>,
    pub process: Arc<dyn ProcessCapability>,
}

impl ToolExecutionContext {
    /// Context backed by the local OS: direct `std::fs` and `std::process`
    /// behavior, identical to the pre-seam tools.
    pub fn local(cwd: impl Into<PathBuf>) -> Self {
        Self {
            cwd: cwd.into(),
            semantic: None,
            fs: Arc::new(LocalFilesystem),
            process: Arc::new(LocalProcess),
        }
    }

    pub fn with_semantic(mut self, semantic: Arc<dyn SemanticToolProvider>) -> Self {
        self.semantic = Some(semantic);
        self
    }
}

#[derive(Debug, Clone)]
pub struct ToolExecutionOutput {
    pub output: String,
    pub diff: Option<String>,
    pub success: bool,
    pub exit_code: Option<i32>,
}

pub trait SemanticToolProvider: Send + Sync {
    fn diagnostics(&self, cwd: &Path, path: &Path) -> Result<String, String>;

    fn symbols(&self, cwd: &Path, path: &Path) -> Result<String, String>;

    fn references(
        &self,
        cwd: &Path,
        path: &Path,
        line: u32,
        character: u32,
    ) -> Result<String, String>;
}

pub trait BuiltinTool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    fn run(
        &self,
        ctx: &ToolExecutionContext,
        input: &ToolInput,
    ) -> Result<ToolExecutionOutput, String>;
}

pub fn context_read_tool_spec() -> ToolSpec {
    ToolSpec {
        name: "context_read".to_string(),
        description: "Read a runtime-owned context handle after scope and permission checks"
            .to_string(),
        is_mutating: false,
        input_schema_hint: "handle_id=<runtime handle>; reason_category=<bounded reason category>"
            .to_string(),
    }
}

/// Outcome of a [`ToolExecutionInterceptor::before_execute`] check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterceptVerdict {
    /// Continue toward the tool (or the next interceptor).
    Proceed,
    /// Stop the call before the tool runs; `message` becomes the execution
    /// error the caller observes.
    Reject { message: String },
}

/// Cross-cutting policy seam around [`ToolRegistry::execute`].
///
/// Invariant carried by this seam: policies that must hold for every tool
/// execution — permission-before-mutation above all, plus cost metering and
/// evidence capture — attach here so they are structural properties of the
/// registry rather than call-site discipline. A call site that reaches
/// `execute` cannot bypass a registered interceptor.
pub trait ToolExecutionInterceptor: Send + Sync {
    /// Runs before the tool. The first `Reject` short-circuits the call: the
    /// tool does not run and later interceptors are not consulted.
    fn before_execute(
        &self,
        spec: &ToolSpec,
        call: &ToolCall,
        ctx: &ToolExecutionContext,
    ) -> InterceptVerdict;

    /// Runs after the tool completes, in reverse registration order.
    fn after_execute(&self, call: &ToolCall, result: &ToolResult) {
        let _ = (call, result);
    }
}

#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn BuiltinTool>>,
    /// Consulted in registration order before every execution; empty by
    /// default so a registry without interceptors behaves exactly as before
    /// the seam existed.
    interceptors: Vec<Arc<dyn ToolExecutionInterceptor>>,
}

impl ToolRegistry {
    pub fn builtin() -> Self {
        let mut registry = Self::default();
        registry.register(ReadFileTool);
        registry.register(WriteFileTool);
        registry.register(EditFileTool);
        registry.register(GlobTool);
        registry.register(GrepTool);
        registry.register(ShellTool);
        registry.register(WebSearchTool);
        registry.register(WebFetchTool);
        registry.register(GitStatusTool);
        registry.register(GitDiffTool);
        registry.register(GitBranchTool);
        registry.register(GitSwitchTool);
        registry.register(GitAddTool);
        registry.register(GitRestoreTool);
        registry.register(GitCommitTool);
        registry.register(GitPushTool);
        registry.register(GitStashListTool);
        registry.register(GitStashPushTool);
        registry.register(GitStashPopTool);
        registry.register(GitStashDropTool);
        registry.register(GitWorktreeListTool);
        registry.register(GitWorktreeAddTool);
        registry.register(GitWorktreeRemoveTool);
        registry.register(LspDiagnosticsTool);
        registry.register(LspSymbolsTool);
        registry.register(LspReferencesTool);
        registry
    }

    pub fn register<T>(&mut self, tool: T)
    where
        T: BuiltinTool + 'static,
    {
        self.tools.insert(tool.spec().name.clone(), Arc::new(tool));
    }

    pub fn register_interceptor(&mut self, interceptor: Arc<dyn ToolExecutionInterceptor>) {
        self.interceptors.push(interceptor);
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.values().map(|tool| tool.spec()).collect()
    }

    pub fn spec(&self, name: &str) -> Option<ToolSpec> {
        self.tools.get(name).map(|tool| tool.spec())
    }

    pub fn execute(
        &self,
        call: &ToolCall,
        ctx: &ToolExecutionContext,
    ) -> Result<ToolResult, String> {
        let tool = self
            .tools
            .get(&call.name)
            .ok_or_else(|| format!("Unknown tool: {}", call.name))?;
        let spec = tool.spec();
        // Pre-execute policy checks run in registration order; the first
        // rejection stops the call before the tool has any effect.
        for interceptor in &self.interceptors {
            if let InterceptVerdict::Reject { message } =
                interceptor.before_execute(&spec, call, ctx)
            {
                return Err(message);
            }
        }
        let output = tool.run(ctx, &call.input)?;
        let result = ToolResult {
            tool_call_id: call.id.clone(),
            name: call.name.clone(),
            output: output.output,
            diff: output.diff,
            success: output.success,
            exit_code: output.exit_code,
        };
        // Post-execute hooks unwind in reverse order, mirroring the nesting
        // of the pre-execute checks.
        for interceptor in self.interceptors.iter().rev() {
            interceptor.after_execute(call, &result);
        }
        Ok(result)
    }
}

fn resolve_path(cwd: &Path, raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}

#[cfg(test)]
mod tests;
