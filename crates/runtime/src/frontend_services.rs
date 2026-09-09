use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use viden_config::{
    default_user_config_path, preview_reset_user_ui_preferences_at, preview_user_ui_preferences_at,
    reset_user_ui_preferences_at, resolve_user_ui_preferences_at, save_user_ui_preferences_at,
};
use viden_tools::patch::parse_diff_document;
use viden_tools::render_diff;
use viden_types::{
    ApprovalResponse, AuditQuery, DiffFile, LaneStatus, PermissionDecision, RecentWorkQuery,
    RuntimeCommand, RuntimeEvent, RuntimeEventKind, SourceTarget, ToolInput, ToolSpec,
    UiPreferencePatch, UiPreferences, WorkspaceChangeKind, WorkspaceDiffEntry, WorkspaceDiffPage,
    WorkspaceDiffQuery, WorkspaceDiffScope, WorkspaceFileEntry, WorkspaceFileKind,
    WorkspaceFilePage, WorkspaceFilesQuery, resolve_ui_preferences,
};

use crate::SessionEngine;
use crate::frontend_status::{
    GIT_COMMAND_TIMEOUT, GitOutput, run_git_capped, sample_workspace_source,
};
use crate::presentation::render_permission_denial;

/// Permission tool name the workspace inventory read is gated under.
///
/// A named tool rather than a bare path check so an operator can write the
/// same allow/ask/deny rules for it that they write for every other workspace
/// read, and so a denial names something recognizable in the transcript.
pub(crate) const WORKSPACE_FILE_INVENTORY_TOOL: &str = "workspace_file_inventory";

/// Directories that never appear in the inventory, whatever the workspace's
/// `.gitignore` says.
///
/// These hold runtime and agent state — Git's own object store, Viden's
/// session/workflow state, the agent scratch directory, lane worktrees, and
/// reference material — not workspace content. A `.gitignore` that happens to
/// omit one of them must not turn thousands of internal files into palette
/// rows, so the exclusion is unconditional rather than inherited.
const EXCLUDED_STATE_DIRECTORIES: [&str; 5] = [".git", ".viden", ".omx", ".worktrees", ".ref"];

/// Builds the rejection reason for a refused inventory read.
///
/// Returned as `Err` so the dispatch publishes `CommandRejected` with the
/// caller's own command id. It is deliberately neither an empty
/// [`WorkspaceFilePage`] — a client receiving one would render "this workspace
/// has no files", a fabricated fact rather than a stated refusal — nor a bare
/// `Error`, which carries no command id and would let a client with a read
/// outstanding mistake an unrelated failure for this refusal.
///
/// The actionable hint is folded into the reason text rather than dropped: a
/// `CommandRejected` has no `hint` field, and telling an operator only that
/// they were refused, without telling them what to grant, is a worse answer
/// than the one the `Error` shape carried.
fn workspace_files_refusal(reason: &str, message: &str) -> String {
    format!(
        "{}\nhint: grant the workspace file inventory permission to list workspace paths",
        render_permission_denial(WORKSPACE_FILE_INVENTORY_TOOL, reason, message)
    )
}

/// Walks the workspace once, orders it, then cuts the requested page.
///
/// Ordering happens *before* the prefix filter, the `after` cursor, and the
/// limit, so [`WorkspaceFilePage::complete`] and
/// [`WorkspaceFilePage::next_after`] describe the filtered ordered inventory
/// rather than whatever the walker produced first (the audit-page precedent).
/// The whole tree is walked for every page: that is what makes two adjacent
/// pages tile the same inventory instead of drifting against a walker whose
/// order is not guaranteed to repeat.
fn read_workspace_file_page(
    root: &Path,
    query: &WorkspaceFilesQuery,
) -> Result<WorkspaceFilePage, String> {
    let mut walker = ignore::WalkBuilder::new(root);
    walker
        // Dotfiles are workspace content an operator wants to jump to
        // (`.gitignore`, `.github/workflows/...`); the state directories above
        // are removed explicitly instead.
        .hidden(false)
        // `.gitignore` is honored even outside a Git repository: the file
        // states the operator's intent about their own tree whether or not
        // `git init` has run.
        .require_git(false)
        // The inventory describes this workspace, not the machine. A global
        // gitignore or a parent directory's rules would make the same
        // workspace enumerate differently on two computers.
        .git_global(false)
        .parents(false)
        .follow_links(false);
    let mut entries = Vec::new();
    for entry in walker.build() {
        let entry = entry.map_err(|error| format!("workspace inventory walk failed: {error}"))?;
        // Depth 0 is the workspace root itself, which is not an entry in its
        // own inventory.
        if entry.depth() == 0 {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        let path = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if path.is_empty() || is_excluded_state_path(&path) {
            continue;
        }
        let file_type = entry.file_type();
        let kind = match file_type {
            Some(file_type) if file_type.is_dir() => WorkspaceFileKind::Dir,
            Some(file_type) if file_type.is_file() => WorkspaceFileKind::File,
            // Neither a plain file nor a directory (a symlink, socket, or
            // device). `WorkspaceFileKind` is `#[non_exhaustive]` precisely so
            // this build never has to mislabel one, so it is omitted rather
            // than published as a file.
            _ => continue,
        };
        let size_bytes = match kind {
            // Absence means "not known here", never zero: unreadable metadata
            // must not be published as an empty file.
            WorkspaceFileKind::File => entry.metadata().ok().map(|metadata| metadata.len()),
            _ => None,
        };
        entries.push(WorkspaceFileEntry {
            path,
            kind,
            size_bytes,
        });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));

    let filtered = entries.into_iter().filter(|entry| {
        query
            .prefix
            .as_deref()
            .is_none_or(|prefix| entry.path.starts_with(prefix))
    });
    // Exclusive cursor: two adjacent pages tile the inventory without
    // repeating the boundary entry.
    let mut remaining = filtered
        .filter(|entry| {
            query
                .after
                .as_deref()
                .is_none_or(|after| entry.path.as_str() > after)
        })
        .collect::<Vec<_>>();
    let limit = query.clamped_limit();
    let complete = remaining.len() <= limit;
    remaining.truncate(limit);
    let next_after = if complete {
        None
    } else {
        remaining.last().map(|entry| entry.path.clone())
    };
    Ok(WorkspaceFilePage {
        entries: remaining,
        next_after,
        complete,
    })
}

/// Whether a workspace-relative path is inside an excluded state directory.
///
/// Matches the directory itself and everything under it, and only on a whole
/// path segment, so a legitimate `src/.gitkeep` or `docs/git-notes.md` is
/// never mistaken for state.
fn is_excluded_state_path(path: &str) -> bool {
    path.split('/')
        .next()
        .is_some_and(|first| EXCLUDED_STATE_DIRECTORIES.contains(&first))
}

impl SessionEngine {
    /// Loads bounded history through the Core-owned session inventory.
    ///
    /// The session crate returns whitelist DTOs only; frontend code never sees
    /// transcript paths, body previews, or arbitrary metadata values.
    pub(crate) fn query_recent_work(
        &self,
        query: RecentWorkQuery,
    ) -> Result<Vec<RuntimeEvent>, String> {
        let inventory =
            viden_session::SessionStore::query_recent_work(self.store.home_dir(), query)?;
        Ok(vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::RecentWorkLoaded {
                projects: inventory.projects,
                sessions: inventory.sessions,
                diagnostics: inventory.diagnostics,
            },
        )])
    }

    /// Reads one newest-first page of the append-only audit timeline.
    ///
    /// Pure read: no permission prompt, no plan-mode gate, no mutation. The
    /// page is answered from the canonical JSONL, so it stays correct after
    /// the derived SQLite index is deleted.
    ///
    /// `command_id` is the id of the `QueryAudit` being answered and is
    /// published on the page, so a client can attribute the page to the exact
    /// read that asked instead of to whatever read it happened to have in
    /// flight. It is only ever the caller's own id: no other path may mint one.
    pub(crate) fn query_audit(
        &self,
        command_id: &str,
        query: AuditQuery,
    ) -> Result<Vec<RuntimeEvent>, String> {
        let page = self.workflows.query_audit(&query)?;
        Ok(vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::AuditPageLoaded {
                command_id: Some(command_id.to_string()),
                page,
            },
        )])
    }

    /// Reads one ordered page of the workspace file inventory (GUI-CORE-022).
    ///
    /// Unlike [`Self::query_audit`] this read touches the operator's working
    /// tree, so it goes through the permission engine *before* a single
    /// directory entry is read — "the same permission gate as other workspace
    /// reads" is the contract request's own close criterion. Three properties
    /// follow from that and are load-bearing:
    ///
    /// - **A refusal is a rejection of this exact read, never an empty page.**
    ///   "You may not read this" and "this workspace has no files" are
    ///   different facts, and a client that received the second for the first
    ///   would render a fabricated absence. The refusal travels as
    ///   `CommandRejected` carrying the caller's own command id rather than as
    ///   a bare `Error`: an uncorrelated `Error` is indistinguishable, to a
    ///   client with a read outstanding, from an unrelated lane or provider
    ///   failure that happened to land in the same window — so the client
    ///   would attribute that failure to its inventory read and render a
    ///   refusal that never happened. Returning `Err` here is what routes it
    ///   through the dispatch's `command_rejected` path.
    /// - **An unresolved `Ask` is also a refusal.** This is a read answering a
    ///   keystroke, not an interactive turn: blocking it on an approval prompt
    ///   would stall a client's palette behind a modal, so the gate is decided
    ///   non-interactively and an `Ask` is rejected exactly as a `Deny` is.
    ///   `approver` is deliberately not consulted.
    /// - **Plan mode still answers.** The tool is non-mutating, so the
    ///   engine's `SafeRead` branch allows it while every mutation stays
    ///   blocked (`PermissionEngine::decide`).
    ///
    /// `command_id` is the id of the `QueryWorkspaceFiles` being answered and
    /// is published on the page. Unlike the audit page it is required, so a
    /// client never has to fall back to correlating by its own acceptance.
    pub(crate) fn query_workspace_files(
        &self,
        command_id: &str,
        query: WorkspaceFilesQuery,
    ) -> Result<Vec<RuntimeEvent>, String> {
        query.validate()?;

        // The gate runs first: nothing below this point has read the disk.
        let tool = ToolSpec {
            name: WORKSPACE_FILE_INVENTORY_TOOL.to_string(),
            description: "Read the workspace file inventory".to_string(),
            // Non-mutating, which is what keeps the read answerable in Plan
            // mode through the engine's SafeRead branch.
            is_mutating: false,
            input_schema_hint: "path".to_string(),
        };
        let mut input = ToolInput::new();
        // The workspace root is the path being read, so the engine's path
        // scope check applies to exactly the tree the walk will cover.
        input.insert("path".to_string(), self.cwd.display().to_string());
        match self.permissions.decide(&tool, &input) {
            PermissionDecision::Allow(_) => {}
            PermissionDecision::Deny(deny) => {
                return Err(workspace_files_refusal(
                    &format!("{:?}", deny.decision_reason),
                    &deny.message,
                ));
            }
            PermissionDecision::Ask(ask) => {
                return Err(workspace_files_refusal("RequiresApproval", &ask.message));
            }
        }

        let page = read_workspace_file_page(&self.cwd, &query)?;
        Ok(vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::WorkspaceFilesLoaded {
                command_id: command_id.to_string(),
                page,
            },
        )])
    }

    /// Reads a bounded structured diff of the workspace or one Lane worktree
    /// (`runtime.structured_diff`, GUI-CORE-012).
    ///
    /// Written beside [`Self::query_workspace_files`] and following the same
    /// discipline, because both answer a client keystroke by touching the
    /// operator's tree:
    ///
    /// - **The gate runs first.** Nothing below the `decide` has spawned a
    ///   process or read a file. The tool is the existing non-mutating
    ///   `git_diff` with the *resolved* target root as the input path, so the
    ///   engine's path-scope check covers exactly the tree the read will
    ///   cover, and one rule set governs operator and agent alike.
    /// - **A refusal is a refusal.** Deny and an unresolved ask both return
    ///   `Err`, which the dispatch publishes as `CommandRejected` naming this
    ///   read. This read answers a keystroke rather than an interactive turn,
    ///   so it is decided non-interactively instead of parking a client behind
    ///   an approval prompt.
    /// - **Plan mode still answers**, because `git_diff` mutates nothing and
    ///   the engine's safe-read branch covers it.
    ///
    /// The target is resolved by Core from its own Lane records; a client
    /// never passes a path. An unknown or archived Lane is rejected rather
    /// than silently answered from the workspace root, which would describe
    /// one tree with another tree's facts.
    pub(crate) fn query_workspace_diff(
        &self,
        command_id: &str,
        query: WorkspaceDiffQuery,
    ) -> Result<Vec<RuntimeEvent>, String> {
        query.validate()?;
        let root = self.resolve_source_target_root(&query.target)?;

        // The gate runs first: nothing below this point has spawned `git`.
        let tool = ToolSpec {
            name: WORKSPACE_DIFF_TOOL.to_string(),
            description: "Show git diff for the current repository".to_string(),
            is_mutating: false,
            input_schema_hint: "path=optional/file/or/repo staged=false".to_string(),
        };
        let mut input = ToolInput::new();
        input.insert("path".to_string(), root.display().to_string());
        match self.permissions.decide(&tool, &input) {
            PermissionDecision::Allow(_) => {}
            PermissionDecision::Deny(deny) => {
                return Err(workspace_diff_refusal(
                    &format!("{:?}", deny.decision_reason),
                    &deny.message,
                ));
            }
            PermissionDecision::Ask(ask) => {
                return Err(workspace_diff_refusal("RequiresApproval", &ask.message));
            }
        }

        let page = read_workspace_diff_page(&root, query)?;
        Ok(vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::WorkspaceDiffLoaded {
                command_id: command_id.to_string(),
                page,
            },
        )])
    }

    /// Resolves the root a source-control read or action runs in, from
    /// Core-owned records only.
    ///
    /// A Lane that no record names, or one that has been archived or
    /// cancelled, is a rejection: answering it from the workspace root would
    /// publish a live tree's changes under a Lane's identity. A Lane with no
    /// worktree is a direct-workspace Lane whose real root *is* the
    /// workspace, so that one resolves rather than failing.
    pub(crate) fn resolve_source_target_root(
        &self,
        target: &SourceTarget,
    ) -> Result<PathBuf, String> {
        match target {
            SourceTarget::Lane { lane_id } => {
                let lanes = self
                    .workflows
                    .load_lane_state()
                    .map_err(|error| format!("lane records are unavailable: {error}"))?;
                let lane = lanes
                    .lanes()
                    .get(lane_id)
                    .ok_or_else(|| format!("lane `{lane_id}` does not exist"))?;
                if matches!(lane.status, LaneStatus::Archived | LaneStatus::Cancelled) {
                    return Err(format!(
                        "lane `{lane_id}` is {:?} and has no readable worktree",
                        lane.status
                    ));
                }
                Ok(lane
                    .worktree
                    .as_ref()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| self.cwd.clone()))
            }
            // `SourceTarget` is `#[non_exhaustive]`: a target kind this build
            // cannot resolve is refused rather than defaulted to the
            // workspace, which would answer a question nobody asked.
            SourceTarget::Workspace => Ok(self.cwd.clone()),
            unsupported => Err(format!(
                "source target {unsupported:?} is not supported by this Core"
            )),
        }
    }

    /// Installs only the non-secret inputs required to re-resolve preferences.
    /// The complete CLI override object may contain provider credentials and is
    /// deliberately never retained by the engine.
    pub(crate) fn set_ui_preference_context(
        &mut self,
        cli_override: Option<UiPreferences>,
        config_path: Option<PathBuf>,
        system_context: UiPreferences,
    ) {
        self.ui_cli_override = cli_override.filter(|profile| {
            resolve_ui_preferences(Some(*profile), None, None, system_context)
                .diagnostics
                .is_empty()
        });
        self.ui_system_context = system_context;
        if let Some(path) = config_path {
            self.user_config_path_override = Some(path);
        }
    }

    pub(crate) fn ui_preference_mutation_descriptor(
        &self,
        command: &RuntimeCommand,
    ) -> Result<Option<(&'static str, String)>, String> {
        match command {
            RuntimeCommand::SetUiPreferences { patch } => {
                let path = self.ui_user_config_path()?;
                let state = preview_user_ui_preferences_at(&path, patch, self.ui_system_context)?;
                Ok(Some((
                    "ui_preferences_set",
                    preference_preview(state.persisted.as_ref()),
                )))
            }
            RuntimeCommand::ResetUiPreferences => {
                let path = self.ui_user_config_path()?;
                preview_reset_user_ui_preferences_at(
                    &path,
                    self.ui_cli_override,
                    self.ui_system_context,
                )?;
                Ok(Some(("ui_preferences_reset", "reset [ui]".to_string())))
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn set_ui_preferences<F>(
        &mut self,
        patch: &UiPreferencePatch,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        let path = self.ui_user_config_path()?;
        let preview = preview_user_ui_preferences_at(&path, patch, self.ui_system_context)?;
        if let Some(denial) = self.ensure_workflow_permission(
            "ui_preferences_set",
            &preference_preview(preview.persisted.as_ref()),
            approver,
        )? {
            return Err(denial);
        }

        save_user_ui_preferences_at(&path, patch, self.ui_system_context)?;
        let state =
            resolve_user_ui_preferences_at(&path, self.ui_cli_override, self.ui_system_context)?;
        self.runtime_snapshot.ui_preferences = state.resolved.clone();
        Ok(vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::UiPreferencesUpdated {
                resolved: state.resolved,
                persisted: state.persisted,
                diagnostics: state.diagnostics,
            },
        )])
    }

    pub(crate) fn reset_ui_preferences<F>(
        &mut self,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        let path = self.ui_user_config_path()?;
        preview_reset_user_ui_preferences_at(&path, self.ui_cli_override, self.ui_system_context)?;
        if let Some(denial) =
            self.ensure_workflow_permission("ui_preferences_reset", "reset [ui]", approver)?
        {
            return Err(denial);
        }

        reset_user_ui_preferences_at(&path, self.ui_cli_override, self.ui_system_context)?;
        let state =
            resolve_user_ui_preferences_at(&path, self.ui_cli_override, self.ui_system_context)?;
        self.runtime_snapshot.ui_preferences = state.resolved.clone();
        Ok(vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::UiPreferencesUpdated {
                resolved: state.resolved,
                persisted: state.persisted,
                diagnostics: state.diagnostics,
            },
        )])
    }

    fn ui_user_config_path(&self) -> Result<PathBuf, String> {
        self.user_config_path_override
            .clone()
            .map(Ok)
            .unwrap_or_else(default_user_config_path)
    }
}

fn preference_preview(profile: Option<&UiPreferences>) -> String {
    profile
        .map(|profile| {
            format!(
                "locale={:?} skin={:?} mode={:?} density={:?} motion={:?}",
                profile.locale, profile.skin, profile.mode, profile.density, profile.motion
            )
            .to_ascii_lowercase()
        })
        .unwrap_or_else(|| "reset [ui]".to_string())
}

/// Permission tool name the structured diff read is gated under.
///
/// Deliberately the *existing* non-mutating `git_diff` tool an agent turn
/// already uses rather than a new operator-only name: one `viden.toml`
/// allow/ask/deny rule set then governs an operator opening a review pane and
/// an agent asking for the same bytes, which is the "one vocabulary" rule the
/// 0.3.3 contract design states.
pub(crate) const WORKSPACE_DIFF_TOOL: &str = "git_diff";

/// Upper bound on the bytes Core will read back from one `git` invocation for
/// a diff read.
///
/// Twice the largest page a client may request, so a caller asking for the
/// 1 MiB maximum still gets a full answer for both the worktree and the index
/// side before the page bound trims it.
const MAX_DIFF_GIT_OUTPUT_BYTES: usize = 2 * 1024 * 1024;

/// Builds the rejection reason for a refused diff read.
///
/// Same shape and same reasoning as [`workspace_files_refusal`]: an `Err` so
/// the dispatch publishes `CommandRejected` with the caller's own command id,
/// never an empty [`WorkspaceDiffPage`] — which a reviewer reads as "nothing
/// changed", the single most dangerous thing a diff surface can say wrongly —
/// and never a bare `Error`, which carries no command id at all.
fn workspace_diff_refusal(reason: &str, message: &str) -> String {
    format!(
        "{}\nhint: grant the `git_diff` permission to read structured workspace changes",
        render_permission_denial(WORKSPACE_DIFF_TOOL, reason, message)
    )
}

/// One `git status --porcelain=v2 -z` record, reduced to what the contract
/// publishes.
struct StatusEntry {
    path: String,
    index: Option<WorkspaceChangeKind>,
    worktree: Option<WorkspaceChangeKind>,
}

/// Maps one porcelain-v2 status letter.
///
/// `.` means "unmodified on this side" and becomes `None`: absence is the
/// fact that the side matches, never a default classification. A letter this
/// build does not model (a copy, a type change) also becomes `None` rather
/// than being flattened into `Modified`, because naming a change wrongly is
/// worse than not naming it.
fn status_letter_kind(letter: u8) -> Option<WorkspaceChangeKind> {
    match letter {
        b'M' => Some(WorkspaceChangeKind::Modified),
        b'A' => Some(WorkspaceChangeKind::Added),
        b'D' => Some(WorkspaceChangeKind::Deleted),
        b'R' => Some(WorkspaceChangeKind::Renamed),
        _ => None,
    }
}

/// Parses `git status --porcelain=v2 -z` into ordered entries.
///
/// `-z` because a path with a space, a quote, or a newline in it is a real
/// path and the non-`-z` form escapes it; a review pane that opened the
/// escaped spelling would open the wrong file. Header (`#`) records and
/// ignored (`!`) records are dropped; unmerged (`u`) records are reported as
/// modified on both sides, which is what an operator sees in the tree.
fn parse_porcelain_v2(raw: &str) -> Vec<StatusEntry> {
    let mut entries = Vec::new();
    let mut records = raw.split('\0').filter(|record| !record.is_empty());
    while let Some(record) = records.next() {
        let mut parts = record.splitn(2, ' ');
        let Some(marker) = parts.next() else {
            continue;
        };
        let rest = parts.next().unwrap_or("");
        match marker {
            "1" | "2" => {
                let fields = rest.split(' ').collect::<Vec<_>>();
                // `1` records carry eight leading fields before the path and
                // `2` records nine (the rename score); the path is whatever
                // follows, and it may itself contain spaces.
                let leading = if marker == "1" { 7 } else { 8 };
                if fields.len() <= leading {
                    continue;
                }
                let xy = fields[0].as_bytes();
                let path = fields[leading..].join(" ");
                if marker == "2" {
                    // The original path is its own NUL-terminated record.
                    let _ = records.next();
                }
                entries.push(StatusEntry {
                    path,
                    index: xy.first().copied().and_then(status_letter_kind),
                    worktree: xy.get(1).copied().and_then(status_letter_kind),
                });
            }
            "u" => {
                let fields = rest.split(' ').collect::<Vec<_>>();
                if fields.len() <= 10 {
                    continue;
                }
                entries.push(StatusEntry {
                    path: fields[10..].join(" "),
                    index: Some(WorkspaceChangeKind::Modified),
                    worktree: Some(WorkspaceChangeKind::Modified),
                });
            }
            "?" => entries.push(StatusEntry {
                path: rest.to_string(),
                index: None,
                worktree: Some(WorkspaceChangeKind::Untracked),
            }),
            _ => {}
        }
    }
    entries
}

/// Sums the published row bytes of one file, which is what the page bound
/// spends. Counting the rows rather than the raw `git` output means the bound
/// describes what a client receives, not what Core happened to read.
fn diff_file_row_bytes(file: &DiffFile) -> u64 {
    file.hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .map(|line| line.content.len() as u64)
        .sum()
}

/// Trims a page to its byte bound, entry by entry, in published order.
///
/// Entries are never dropped: a reviewer must still see that a path changed.
/// Only the rows go, and the file keeps its real counts, so "not shown" can
/// never be mistaken for "unchanged".
fn apply_diff_page_bound(entries: &mut [WorkspaceDiffEntry], byte_limit: u32) -> bool {
    let mut spent = 0_u64;
    let mut truncated = false;
    for entry in entries.iter_mut() {
        let Some(file) = entry.diff.as_mut() else {
            continue;
        };
        let bytes = diff_file_row_bytes(file);
        if spent.saturating_add(bytes) > u64::from(byte_limit) {
            file.omitted = true;
            file.hunks.clear();
            truncated = true;
        } else {
            spent = spent.saturating_add(bytes);
        }
    }
    truncated
}

/// Indexes one `git diff` invocation's output by published path.
/// The staged change an operator is about to commit
/// (`runtime.operator_git`, GUI-CORE-020).
///
/// Built from `git diff --cached` in the resolved target root, which is
/// precisely the content the commit will contain — not the working tree, which
/// holds edits the commit will not take. Reuses the C1 producer, so the rows a
/// reviewer approves here and the rows a diff read shows come from one parser.
///
/// There is no `base_sha256`: a staged change spans arbitrarily many files and
/// one hash cannot describe several, so naming one would invite a client to
/// verify the wrong file. An index with nothing in it still yields a document
/// with no files rather than `None`, because "you staged nothing" is a real
/// answer to "what am I committing" and is the one an operator needs to see
/// before they approve a commit that would fail.
pub(crate) fn staged_diff_context(root: &Path) -> viden_types::DecisionContext {
    let byte_limit = crate::decision_context::MAX_DECISION_CONTEXT_DIFF_BYTES;
    let mut document = viden_types::DiffDocument {
        files: diff_files_by_path(root, &["diff", "--cached"], byte_limit)
            .into_values()
            .collect(),
        truncated: false,
        byte_limit,
    };
    document.truncated = document.files.iter().any(|file| file.omitted);
    viden_types::DecisionContext {
        diff: Some(document),
        base_sha256: None,
    }
}

fn diff_files_by_path(root: &Path, args: &[&str], byte_limit: u32) -> BTreeMap<String, DiffFile> {
    let raw = match run_git_capped(
        root,
        Path::new("git"),
        args,
        GIT_COMMAND_TIMEOUT,
        MAX_DIFF_GIT_OUTPUT_BYTES,
    ) {
        GitOutput::Complete(output) => output,
        // A failed, unavailable, or oversized `git diff` yields no rows. The
        // entries still ship from the status read, each with `diff: None`,
        // which says Core produced no rows for that path — not that the path
        // is unchanged.
        GitOutput::Truncated | GitOutput::Failed | GitOutput::Unavailable => String::new(),
    };
    parse_diff_document(&raw, byte_limit)
        .files
        .into_iter()
        .map(|file| (file.path.clone(), file))
        .collect()
}

/// Renders an untracked file as a whole-file addition.
///
/// `git diff` says nothing at all about an untracked path, so leaving it
/// without rows would tell a reviewer the file does not exist. Core reads it
/// once, bounded, and publishes its content as added lines.
fn untracked_diff_file(root: &Path, path: &str, byte_limit: u32) -> Option<DiffFile> {
    let bytes = std::fs::read(root.join(path)).ok()?;
    let content = String::from_utf8_lossy(&bytes);
    let mut document = parse_diff_document(&render_diff("", &content), byte_limit);
    let mut file = document.files.pop()?;
    file.path = path.to_string();
    file.old_path = None;
    file.kind = WorkspaceChangeKind::Added;
    Some(file)
}

/// Assembles one diff page: status first, then rows for the requested scope.
///
/// The status read is the authority for *which* paths changed and how; the
/// `git diff` reads only supply rows. Deriving the entry list from the diffs
/// instead would silently drop every untracked file, and a reviewer would see
/// a page that says a new file does not exist.
fn read_workspace_diff_page(
    root: &Path,
    query: WorkspaceDiffQuery,
) -> Result<WorkspaceDiffPage, String> {
    let byte_limit = query.clamped_byte_limit();
    let status = match run_git_capped(
        root,
        Path::new("git"),
        &["status", "--porcelain=v2", "-z", "--untracked-files=normal"],
        GIT_COMMAND_TIMEOUT,
        MAX_DIFF_GIT_OUTPUT_BYTES,
    ) {
        GitOutput::Complete(output) => output,
        GitOutput::Truncated => {
            return Err(format!(
                "workspace diff status output exceeded {MAX_DIFF_GIT_OUTPUT_BYTES} bytes"
            ));
        }
        GitOutput::Failed | GitOutput::Unavailable => {
            return Err(format!(
                "workspace diff is unavailable: `git status` failed in {}",
                root.display()
            ));
        }
    };

    let wants_worktree = matches!(
        query.scope,
        WorkspaceDiffScope::Worktree | WorkspaceDiffScope::Both
    );
    let wants_index = matches!(
        query.scope,
        WorkspaceDiffScope::Index | WorkspaceDiffScope::Both
    );
    // Parsed unbounded here; the page bound below is applied across the
    // assembled entries so it describes the answer rather than one `git` run.
    let worktree_files = if wants_worktree {
        diff_files_by_path(root, &["diff"], u32::MAX)
    } else {
        BTreeMap::new()
    };
    let index_files = if wants_index {
        diff_files_by_path(root, &["diff", "--cached"], u32::MAX)
    } else {
        BTreeMap::new()
    };

    let mut entries = Vec::new();
    for status_entry in parse_porcelain_v2(&status) {
        // The same unconditional exclusions the file inventory applies:
        // these directories hold runtime and agent state, never workspace
        // content, whatever the workspace's `.gitignore` says.
        if is_excluded_state_path(&status_entry.path) {
            continue;
        }
        if !query.paths.is_empty()
            && !query
                .paths
                .iter()
                .any(|filter| path_matches_filter(&status_entry.path, filter))
        {
            continue;
        }
        let staged = status_entry.index.is_some();
        let untracked = status_entry.worktree == Some(WorkspaceChangeKind::Untracked);
        // Scope resolution for a path present on both sides: the worktree
        // diff wins, because that is the content the operator's file actually
        // holds right now. The staged half stays visible through `index`, so
        // nothing is hidden — only the rows pick one side.
        let diff = if untracked {
            if wants_worktree {
                untracked_diff_file(root, &status_entry.path, byte_limit)
            } else {
                None
            }
        } else {
            worktree_files
                .get(&status_entry.path)
                .or_else(|| index_files.get(&status_entry.path))
                .cloned()
        };
        entries.push(WorkspaceDiffEntry {
            path: status_entry.path,
            index: status_entry.index,
            worktree: status_entry.worktree,
            staged,
            diff,
        });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    let truncated = apply_diff_page_bound(&mut entries, byte_limit);

    Ok(WorkspaceDiffPage {
        target: query.target,
        // Resampled for this read, so a client renders the branch the rows
        // came from rather than the last one it happened to see.
        source: sample_workspace_source(root),
        entries,
        truncated,
    })
}

/// Whether a changed path is selected by one client-supplied filter.
///
/// Prefix matching on whole path segments, so `crates` selects
/// `crates/types/src/lib.rs` but never `crates-old/x`. The filter selects
/// among paths Core already found; it can never introduce one.
fn path_matches_filter(path: &str, filter: &str) -> bool {
    let filter = filter.trim_end_matches('/');
    path == filter
        || path
            .strip_prefix(filter)
            .is_some_and(|rest| rest.starts_with('/'))
}
