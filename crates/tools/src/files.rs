use std::path::PathBuf;

use viden_types::{ToolInput, ToolSpec};

use crate::{BuiltinTool, ToolExecutionContext, ToolExecutionOutput, resolve_path};

pub(crate) struct ReadFileTool;
pub(crate) struct WriteFileTool;
pub(crate) struct EditFileTool;

impl BuiltinTool for ReadFileTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "read_file".to_string(),
            description: "Read a UTF-8 text file".to_string(),
            is_mutating: false,
            input_schema_hint: "path=relative/or/absolute/path max_bytes=8192".to_string(),
        }
    }

    fn run(
        &self,
        ctx: &ToolExecutionContext,
        input: &ToolInput,
    ) -> Result<ToolExecutionOutput, String> {
        let path = resolve_required_path(ctx, input)?;
        let max_bytes = input
            .get("max_bytes")
            .and_then(|raw| raw.parse::<usize>().ok())
            .unwrap_or(16 * 1024);
        if ctx.fs.is_dir(&path) {
            return Err(format!(
                "`read_file` expected a file but `{}` is a directory; use `glob` or `grep` to inspect files inside it",
                path.display()
            ));
        }
        let bytes = ctx.fs.read(&path)?;
        let slice = &bytes[..bytes.len().min(max_bytes)];
        Ok(ToolExecutionOutput {
            output: String::from_utf8_lossy(slice).to_string(),
            diff: None,
            success: true,
            exit_code: None,
        })
    }
}

impl BuiltinTool for WriteFileTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "write_file".to_string(),
            description: "Create or overwrite a file".to_string(),
            is_mutating: true,
            input_schema_hint: "path=file content='new contents'".to_string(),
        }
    }

    fn run(
        &self,
        ctx: &ToolExecutionContext,
        input: &ToolInput,
    ) -> Result<ToolExecutionOutput, String> {
        let path = resolve_required_path(ctx, input)?;
        let content = input
            .get("content")
            .ok_or_else(|| "write_file requires `content`".to_string())?;
        if ctx.fs.is_dir(&path) {
            return Err(format!(
                "`write_file` expected a file path but `{}` is an existing directory",
                path.display()
            ));
        }
        let before = ctx.fs.read_to_string(&path).unwrap_or_default();
        if let Some(parent) = path.parent() {
            ctx.fs.create_dir_all(parent)?;
        }
        ctx.fs.write(&path, content)?;
        Ok(ToolExecutionOutput {
            output: render_file_effect("write_file", &path, content, "wrote"),
            diff: Some(render_diff(&before, content)),
            success: true,
            exit_code: None,
        })
    }
}

impl BuiltinTool for EditFileTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "edit_file".to_string(),
            description: "Replace text inside a file".to_string(),
            is_mutating: true,
            input_schema_hint: "path=file old='find' new='replace'".to_string(),
        }
    }

    fn run(
        &self,
        ctx: &ToolExecutionContext,
        input: &ToolInput,
    ) -> Result<ToolExecutionOutput, String> {
        let path = resolve_required_path(ctx, input)?;
        let old = input
            .get("old")
            .ok_or_else(|| "edit_file requires `old`".to_string())?;
        let new = input
            .get("new")
            .ok_or_else(|| "edit_file requires `new`".to_string())?;
        if ctx.fs.is_dir(&path) {
            return Err(format!(
                "`edit_file` expected a file but `{}` is a directory; choose a file inside it",
                path.display()
            ));
        }
        let before = ctx.fs.read_to_string(&path)?;
        if !before.contains(old) {
            return Err("edit_file could not find the target text".to_string());
        }
        let after = before.replacen(old, new, 1);
        ctx.fs.write(&path, &after)?;
        Ok(ToolExecutionOutput {
            output: render_file_effect("edit_file", &path, &after, "edited"),
            diff: Some(render_diff(&before, &after)),
            success: true,
            exit_code: None,
        })
    }
}

fn resolve_required_path(ctx: &ToolExecutionContext, input: &ToolInput) -> Result<PathBuf, String> {
    let raw = input
        .get("path")
        .ok_or_else(|| "tool requires `path`".to_string())?;
    Ok(resolve_path(&ctx.cwd, raw))
}

/// Renders the header-only preview `write_file` and `edit_file` publish as
/// their tool-result diff.
///
/// Public because the approval path computes the *prospective* content in
/// memory and renders it with this exact function, so an approval preview and
/// the diff the executed tool publishes are produced by one renderer and
/// cannot describe the same edit differently. The text carries no `@@`
/// header; `patch::parse_diff_document` folds it into one whole-file hunk.
pub fn render_diff(before: &str, after: &str) -> String {
    let before_lines: Vec<_> = before.lines().collect();
    let after_lines: Vec<_> = after.lines().collect();
    let mut output = String::from("--- before\n+++ after\n");
    let max = before_lines.len().max(after_lines.len());
    for index in 0..max {
        match (before_lines.get(index), after_lines.get(index)) {
            (Some(left), Some(right)) if left == right => {
                output.push(' ');
                output.push_str(left);
                output.push('\n');
            }
            (Some(left), Some(right)) => {
                output.push('-');
                output.push_str(left);
                output.push('\n');
                output.push('+');
                output.push_str(right);
                output.push('\n');
            }
            (Some(left), None) => {
                output.push('-');
                output.push_str(left);
                output.push('\n');
            }
            (None, Some(right)) => {
                output.push('+');
                output.push_str(right);
                output.push('\n');
            }
            (None, None) => {}
        }
    }
    output
}

fn render_file_effect(tool: &str, path: &std::path::Path, content: &str, verb: &str) -> String {
    let line_count = content
        .lines()
        .count()
        .max(usize::from(!content.is_empty()));
    [
        format!("{tool} completed"),
        format!("path: {}", path.display()),
        format!("size: {}", format_bytes(content.len())),
        format!(
            "effect: {verb} {line_count} {}",
            if line_count == 1 { "line" } else { "lines" }
        ),
    ]
    .join("\n")
}

fn format_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    }
}
