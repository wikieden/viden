use std::fs;
use std::path::{Component, Path, PathBuf};

use viden_types::{DiffDocument, DiffFile, DiffHunk, DiffLine, DiffLineKind, WorkspaceChangeKind};

use crate::lane::LaneEffectError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchRequest {
    pub cwd: PathBuf,
    pub unified_diff: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchConflictReport {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchApplyOutcome {
    pub applied: bool,
    pub writes: Vec<PathBuf>,
    pub conflicts: Vec<PatchConflictReport>,
}

pub trait PatchBackend: Send + Sync {
    fn check(&self, request: &PatchRequest) -> Result<PatchApplyOutcome, LaneEffectError>;
    fn apply(&self, request: &PatchRequest) -> Result<PatchApplyOutcome, LaneEffectError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LocalPatchBackend;

#[derive(Debug, Clone)]
enum PatchChange {
    Write { path: PathBuf, contents: String },
    Delete { path: PathBuf },
}

impl PatchChange {
    fn path(&self) -> &Path {
        match self {
            Self::Write { path, .. } | Self::Delete { path } => path,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PatchApplication {
    root: PathBuf,
    changes: Vec<PatchChange>,
}

impl PatchApplication {
    /// Includes both file writes and deletions so transaction rollback can
    /// snapshot every path affected by a patch.
    pub fn write_paths(&self) -> impl Iterator<Item = &Path> {
        self.changes.iter().map(PatchChange::path)
    }

    /// Returns the exact bytes (or expected absence for deletes) that a
    /// prepared patch will publish. Recovery snapshots bind these postimages
    /// before mutation so a later revert cannot overwrite unrelated edits.
    pub fn planned_postimages(&self) -> impl Iterator<Item = (&Path, Option<&[u8]>)> {
        self.changes.iter().map(|change| match change {
            PatchChange::Write { path, contents } => (path.as_path(), Some(contents.as_bytes())),
            PatchChange::Delete { path } => (path.as_path(), None),
        })
    }
}

impl LocalPatchBackend {
    pub fn prepare(&self, request: &PatchRequest) -> Result<PatchApplication, LaneEffectError> {
        let root = fs::canonicalize(&request.cwd)
            .map_err(|error| LaneEffectError::Io(format!("{}: {error}", request.cwd.display())))?;
        if !root.is_dir() {
            return Err(LaneEffectError::Io(format!(
                "{} is not a directory",
                request.cwd.display()
            )));
        }
        let patch_files = parse_unified_diff(&request.unified_diff)?;
        if patch_files.is_empty() {
            return Err(patch_conflict(
                PathBuf::new(),
                "no unified diff patch found",
            ));
        }

        // Resolve and validate every target before touching the filesystem.
        // This keeps creates, writes, and deletes inside one rollback boundary.
        let mut changes = Vec::new();
        for patch_file in patch_files {
            changes.push(prepare_patch_file(&root, &patch_file)?);
        }

        Ok(PatchApplication { root, changes })
    }

    pub fn write_application(
        &self,
        application: &PatchApplication,
    ) -> Result<PatchApplyOutcome, LaneEffectError> {
        let rollback = stage_rollback(application)?;
        if let Err(err) = write_patch_application(application) {
            restore_rollback(&rollback)?;
            return Err(err);
        }
        Ok(success_outcome(application))
    }

    pub fn apply_transactionally<F>(
        &self,
        request: &PatchRequest,
        persist: F,
    ) -> Result<PatchApplyOutcome, LaneEffectError>
    where
        F: FnOnce() -> Result<(), String>,
    {
        let application = match self.prepare(request) {
            Ok(application) => application,
            Err(LaneEffectError::PatchConflict { path, message }) => {
                return Ok(conflict_outcome(path, message));
            }
            Err(err) => return Err(err),
        };
        let rollback = stage_rollback(&application)?;
        if let Err(err) = write_patch_application(&application) {
            restore_rollback(&rollback)?;
            return Err(err);
        }
        if let Err(err) = persist() {
            restore_rollback(&rollback)?;
            return Err(LaneEffectError::Io(err));
        }
        Ok(success_outcome(&application))
    }
}

impl PatchBackend for LocalPatchBackend {
    fn check(&self, request: &PatchRequest) -> Result<PatchApplyOutcome, LaneEffectError> {
        match self.prepare(request) {
            Ok(application) => Ok(PatchApplyOutcome {
                applied: false,
                writes: application.write_paths().map(Path::to_path_buf).collect(),
                conflicts: Vec::new(),
            }),
            Err(LaneEffectError::PatchConflict { path, message }) => {
                Ok(conflict_outcome(path, message))
            }
            Err(err) => Err(err),
        }
    }

    fn apply(&self, request: &PatchRequest) -> Result<PatchApplyOutcome, LaneEffectError> {
        match self.prepare(request) {
            Ok(application) => self.write_application(&application),
            Err(LaneEffectError::PatchConflict { path, message }) => {
                Ok(conflict_outcome(path, message))
            }
            Err(err) => Err(err),
        }
    }
}

fn success_outcome(application: &PatchApplication) -> PatchApplyOutcome {
    PatchApplyOutcome {
        applied: true,
        writes: application.write_paths().map(Path::to_path_buf).collect(),
        conflicts: Vec::new(),
    }
}

fn conflict_outcome(path: PathBuf, message: String) -> PatchApplyOutcome {
    PatchApplyOutcome {
        applied: false,
        writes: Vec::new(),
        conflicts: vec![PatchConflictReport { path, message }],
    }
}

fn patch_conflict(path: PathBuf, message: impl Into<String>) -> LaneEffectError {
    LaneEffectError::PatchConflict {
        path,
        message: message.into(),
    }
}

#[derive(Debug)]
struct PatchFile {
    old_path: String,
    new_path: String,
    hunks: Vec<PatchHunk>,
}

#[derive(Debug)]
struct PatchHunk {
    old_lines: Vec<String>,
    new_lines: Vec<String>,
}

/// One file as the scanner saw it, before either consumer shapes it.
///
/// The apply path and the structured document read the *same* scan: two
/// parsers over one text would eventually disagree about what a hunk is,
/// which is the failure the promotion in `runtime.structured_diff` exists to
/// prevent.
#[derive(Debug, Default)]
struct ScannedFile {
    old_path: String,
    new_path: String,
    /// Git reported binary content, so this file has no rows at all.
    binary: bool,
    /// A `rename from`/`rename to` pair was present in the extended header.
    renamed: bool,
    hunks: Vec<ScannedHunk>,
}

#[derive(Debug, Default)]
struct ScannedHunk {
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
    /// The section heading Git writes after the closing `@@`, when it wrote
    /// one. `None` means the header carried none, never an empty heading.
    header: Option<String>,
    /// `false` for a hunk the scanner synthesized because the text carried
    /// rows with no `@@` line (what `render_diff` produces). Its start and
    /// lengths are counted from the rows rather than read from a header.
    declared: bool,
    /// Marker plus content, newline preserved, exactly as the apply path
    /// needs it.
    rows: Vec<(char, String)>,
}

fn prepare_patch_file(cwd: &Path, patch_file: &PatchFile) -> Result<PatchChange, LaneEffectError> {
    match (
        patch_file.old_path.as_str() == "/dev/null",
        patch_file.new_path.as_str() == "/dev/null",
    ) {
        (true, true) => Err(patch_conflict(
            PathBuf::new(),
            "patch cannot create and delete /dev/null",
        )),
        (true, false) => {
            let relative_path = validate_patch_path(&patch_file.new_path)?;
            let full_path = resolve_patch_target(cwd, &relative_path)?;
            if fs::symlink_metadata(&full_path).is_ok() {
                return Err(patch_conflict(
                    relative_path,
                    "new-file patch target already exists",
                ));
            }
            let contents = apply_patch_file("", patch_file)
                .map_err(|message| patch_conflict(relative_path.clone(), message))?;
            Ok(PatchChange::Write {
                path: full_path,
                contents,
            })
        }
        (false, true) => {
            let relative_path = validate_patch_path(&patch_file.old_path)?;
            let full_path = resolve_patch_target(cwd, &relative_path)?;
            let current = read_patch_target(&full_path, &relative_path)?;
            let remaining = apply_patch_file(&current, patch_file)
                .map_err(|message| patch_conflict(relative_path.clone(), message))?;
            if !remaining.is_empty() {
                return Err(patch_conflict(
                    relative_path,
                    "deleted-file patch did not remove the complete file",
                ));
            }
            Ok(PatchChange::Delete { path: full_path })
        }
        (false, false) => {
            let old_path = validate_patch_path(&patch_file.old_path)?;
            let new_path = validate_patch_path(&patch_file.new_path)?;
            if old_path != new_path {
                return Err(patch_conflict(
                    new_path,
                    "rename patches are not supported by this adapter",
                ));
            }
            let full_path = resolve_patch_target(cwd, &new_path)?;
            let current = read_patch_target(&full_path, &new_path)?;
            let contents = apply_patch_file(&current, patch_file)
                .map_err(|message| patch_conflict(new_path.clone(), message))?;
            Ok(PatchChange::Write {
                path: full_path,
                contents,
            })
        }
    }
}

fn read_patch_target(path: &Path, relative_path: &Path) -> Result<String, LaneEffectError> {
    fs::read_to_string(path)
        .map_err(|err| LaneEffectError::Io(format!("{}: {err}", relative_path.display())))
}

fn write_patch_application(application: &PatchApplication) -> Result<(), LaneEffectError> {
    for change in &application.changes {
        ensure_patch_target_still_safe(&application.root, change.path())?;
        match change {
            PatchChange::Write { path, contents } => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|err| {
                        LaneEffectError::Io(format!("{}: {err}", parent.display()))
                    })?;
                }
                fs::write(path, contents)
                    .map_err(|err| LaneEffectError::Io(format!("{}: {err}", path.display())))?;
            }
            PatchChange::Delete { path } => {
                fs::remove_file(path)
                    .map_err(|err| LaneEffectError::Io(format!("{}: {err}", path.display())))?;
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
struct FileRollback {
    root: PathBuf,
    path: PathBuf,
    contents: Option<Vec<u8>>,
    permissions: Option<fs::Permissions>,
    created_parent_dirs: Vec<PathBuf>,
}

fn stage_rollback(application: &PatchApplication) -> Result<Vec<FileRollback>, LaneEffectError> {
    let mut rollback = Vec::new();
    for path in application.write_paths() {
        ensure_patch_target_still_safe(&application.root, path)?;
        let metadata = fs::symlink_metadata(path).ok();
        rollback.push(FileRollback {
            root: application.root.clone(),
            path: path.to_path_buf(),
            contents: fs::read(path).ok(),
            permissions: metadata.map(|metadata| metadata.permissions()),
            created_parent_dirs: missing_parent_dirs(&application.root, path)?,
        });
    }
    Ok(rollback)
}

fn restore_rollback(files: &[FileRollback]) -> Result<(), LaneEffectError> {
    for file in files.iter().rev() {
        ensure_patch_target_still_safe(&file.root, &file.path)?;
        match &file.contents {
            Some(contents) => {
                if let Some(parent) = file.path.parent() {
                    fs::create_dir_all(parent).map_err(|err| {
                        LaneEffectError::Io(format!("{}: {err}", parent.display()))
                    })?;
                }
                fs::write(&file.path, contents).map_err(|err| {
                    LaneEffectError::Io(format!("{}: {err}", file.path.display()))
                })?;
                if let Some(permissions) = &file.permissions {
                    fs::set_permissions(&file.path, permissions.clone()).map_err(|err| {
                        LaneEffectError::Io(format!("{}: {err}", file.path.display()))
                    })?;
                }
            }
            None => {
                if file.path.exists() {
                    fs::remove_file(&file.path).map_err(|err| {
                        LaneEffectError::Io(format!("{}: {err}", file.path.display()))
                    })?;
                }
            }
        }
    }
    for file in files.iter().rev() {
        for directory in &file.created_parent_dirs {
            ensure_patch_target_still_safe(&file.root, directory)?;
            match fs::remove_dir(directory) {
                Ok(()) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                    ) => {}
                Err(error) => {
                    return Err(LaneEffectError::Io(format!(
                        "{}: {error}",
                        directory.display()
                    )));
                }
            }
        }
    }
    Ok(())
}

fn missing_parent_dirs(root: &Path, path: &Path) -> Result<Vec<PathBuf>, LaneEffectError> {
    let mut missing = Vec::new();
    let mut parent = path.parent();
    while let Some(directory) = parent {
        if directory == root {
            break;
        }
        ensure_patch_target_still_safe(root, directory)?;
        match fs::symlink_metadata(directory) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(directory.to_path_buf());
                parent = directory.parent();
            }
            Err(error) => {
                return Err(LaneEffectError::Io(format!(
                    "{}: {error}",
                    directory.display()
                )));
            }
        }
    }
    Ok(missing)
}

/// Shapes the shared scan into the apply path's view of a patch.
///
/// Behavior is byte-for-byte what it was before the promotion: only files
/// that carry at least one hunk with rows survive, hunk rows keep their
/// trailing newline, and a context row lands on both sides.
fn parse_unified_diff(diff: &str) -> Result<Vec<PatchFile>, LaneEffectError> {
    let scanned = scan_unified_diff(diff)?;
    let mut files = Vec::new();
    for file in scanned {
        let hunks = file
            .hunks
            .into_iter()
            .map(|hunk| {
                let mut old_lines = Vec::new();
                let mut new_lines = Vec::new();
                for (marker, content) in hunk.rows {
                    match marker {
                        ' ' => {
                            old_lines.push(content.clone());
                            new_lines.push(content);
                        }
                        '-' => old_lines.push(content),
                        '+' => new_lines.push(content),
                        _ => {}
                    }
                }
                PatchHunk {
                    old_lines,
                    new_lines,
                }
            })
            .collect::<Vec<_>>();
        if hunks.is_empty() {
            continue;
        }
        files.push(PatchFile {
            old_path: file.old_path,
            new_path: file.new_path,
            hunks,
        });
    }
    Ok(files)
}

/// Parses unified diff text into the typed [`DiffDocument`] contract
/// (`runtime.structured_diff`).
///
/// This is the single Core-side producer of diff rows: the apply path above
/// and every client-visible diff come from the same scan, so a hunk means the
/// same thing in both. It is deliberately infallible. The text arrives from an
/// external `git` process, from an agent-authored patch, and from the
/// `render_diff` preview, and a structural problem in any of those must
/// degrade to a partial document rather than take down the read that asked
/// for it — the apply path keeps the strict `Result` because applying a patch
/// it half understood would corrupt the tree.
///
/// `byte_limit` bounds the published rows, not the file list: a file whose
/// rows would cross the bound is published `omitted` with its real counts, and
/// the document is marked `truncated`. A dropped file must still be visible,
/// because a reviewer reading "unchanged" where the truth is "not shown" is
/// exactly the failure the bound must not cause.
pub fn parse_diff_document(diff: &str, byte_limit: u32) -> DiffDocument {
    let scanned = scan_unified_diff(diff).unwrap_or_default();
    let mut document = DiffDocument {
        files: Vec::with_capacity(scanned.len()),
        truncated: false,
        byte_limit,
    };
    let mut spent: u64 = 0;
    for file in scanned {
        let kind = classify_scanned_file(&file);
        let path = display_patch_path(if kind == WorkspaceChangeKind::Deleted {
            &file.old_path
        } else {
            &file.new_path
        });
        let old_path = (kind == WorkspaceChangeKind::Renamed)
            .then(|| display_patch_path(&file.old_path))
            .filter(|old| *old != path);

        let mut additions = 0_u32;
        let mut deletions = 0_u32;
        let mut hunks = Vec::with_capacity(file.hunks.len());
        let mut bytes = 0_u64;
        for hunk in &file.hunks {
            // Line numbers come from the hunk header when there was one, and
            // from line 1 when the text carried rows with no `@@` at all.
            let mut old_cursor = hunk.old_start.max(1);
            let mut new_cursor = hunk.new_start.max(1);
            let mut lines = Vec::with_capacity(hunk.rows.len());
            for (marker, content) in &hunk.rows {
                let content = content.trim_end_matches('\n').trim_end_matches('\r');
                bytes = bytes.saturating_add(content.len() as u64);
                let line = match marker {
                    '-' => {
                        deletions = deletions.saturating_add(1);
                        let old_line = Some(old_cursor);
                        old_cursor = old_cursor.saturating_add(1);
                        DiffLine {
                            kind: DiffLineKind::Removed,
                            content: content.to_string(),
                            old_line,
                            new_line: None,
                        }
                    }
                    '+' => {
                        additions = additions.saturating_add(1);
                        let new_line = Some(new_cursor);
                        new_cursor = new_cursor.saturating_add(1);
                        DiffLine {
                            kind: DiffLineKind::Added,
                            content: content.to_string(),
                            old_line: None,
                            new_line,
                        }
                    }
                    _ => {
                        let old_line = Some(old_cursor);
                        let new_line = Some(new_cursor);
                        old_cursor = old_cursor.saturating_add(1);
                        new_cursor = new_cursor.saturating_add(1);
                        DiffLine {
                            kind: DiffLineKind::Context,
                            content: content.to_string(),
                            old_line,
                            new_line,
                        }
                    }
                };
                lines.push(line);
            }
            hunks.push(DiffHunk {
                old_start: hunk.old_start,
                old_lines: hunk.old_lines,
                new_start: hunk.new_start,
                new_lines: hunk.new_lines,
                header: hunk.header.clone(),
                lines,
            });
        }

        // The counts survive the bound; only the rows are dropped.
        let omitted = spent.saturating_add(bytes) > u64::from(byte_limit);
        if omitted {
            document.truncated = true;
            hunks.clear();
        } else {
            spent = spent.saturating_add(bytes);
        }
        document.files.push(DiffFile {
            path,
            old_path,
            kind,
            binary: file.binary,
            omitted,
            additions,
            deletions,
            hunks,
        });
    }
    document
}

/// Classifies one scanned file from its headers alone.
///
/// Core never guesses from content: `/dev/null` on either side is the create
/// or delete Git wrote, an explicit `rename from`/`rename to` pair or two
/// different real paths is a rename, and everything else is a modification.
fn classify_scanned_file(file: &ScannedFile) -> WorkspaceChangeKind {
    let old_missing = file.old_path == "/dev/null" || file.old_path.is_empty();
    let new_missing = file.new_path == "/dev/null" || file.new_path.is_empty();
    if old_missing && !new_missing {
        return WorkspaceChangeKind::Added;
    }
    if new_missing && !old_missing {
        return WorkspaceChangeKind::Deleted;
    }
    if file.renamed
        || (!old_missing
            && !new_missing
            && display_patch_path(&file.old_path) != display_patch_path(&file.new_path))
    {
        return WorkspaceChangeKind::Renamed;
    }
    WorkspaceChangeKind::Modified
}

/// Strips Git's `a/`/`b/` prefixes for display. Publishing the raw header
/// would leak Git's internal prefixes into every client's file tree.
fn display_patch_path(raw: &str) -> String {
    raw.trim()
        .strip_prefix("a/")
        .or_else(|| raw.trim().strip_prefix("b/"))
        .unwrap_or(raw.trim())
        .to_string()
}

/// The one scan both consumers read.
///
/// It accepts three shapes because Core has three producers: a `git diff`
/// with `diff --git` sections, a bare `---`/`+++` pair (an agent-authored
/// patch), and the header-only preview `render_diff` writes, which carries
/// rows with no `@@` line at all. The last one folds into a single
/// synthesized whole-file hunk; without that an approval preview would parse
/// to zero rows and a permission dock would show nothing at all.
fn scan_unified_diff(diff: &str) -> Result<Vec<ScannedFile>, LaneEffectError> {
    let mut files: Vec<ScannedFile> = Vec::new();
    let mut current_file: Option<ScannedFile> = None;
    let mut current_hunk: Option<ScannedHunk> = None;

    for raw_line in diff.split_inclusive('\n') {
        let line = raw_line.trim_end_matches('\n').trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("diff --git ") {
            finish_patch_hunk(&mut current_file, &mut current_hunk);
            if let Some(file) = current_file.take() {
                files.push(file);
            }
            let mut paths = rest.split_whitespace();
            let old_path = paths.next().ok_or_else(|| {
                patch_conflict(PathBuf::new(), format!("invalid diff header `{line}`"))
            })?;
            let new_path = paths.next().ok_or_else(|| {
                patch_conflict(PathBuf::new(), format!("invalid diff header `{line}`"))
            })?;
            current_file = Some(ScannedFile {
                old_path: old_path.to_string(),
                new_path: new_path.to_string(),
                ..ScannedFile::default()
            });
            continue;
        }

        if current_hunk.is_none()
            && let Some(path) = line.strip_prefix("--- ")
        {
            // A bare `---` with no preceding `diff --git` still opens a file:
            // `render_diff` and hand-written patches both start here.
            if current_file.is_none() {
                current_file = Some(ScannedFile::default());
            }
            if let Some(file) = current_file.as_mut() {
                file.old_path = header_path(path).to_string();
            }
            continue;
        }

        if current_hunk.is_none()
            && let Some(path) = line.strip_prefix("+++ ")
        {
            if let Some(file) = current_file.as_mut() {
                file.new_path = header_path(path).to_string();
            }
            continue;
        }

        if current_hunk.is_none()
            && let Some(file) = current_file.as_mut()
        {
            // Extended headers Git writes between the `diff --git` line and
            // the `---`/`+++` pair. A rename is read from the header rather
            // than inferred, and a binary file is a change with no rows.
            if let Some(from) = line.strip_prefix("rename from ") {
                file.renamed = true;
                file.old_path = from.trim().to_string();
                continue;
            }
            if let Some(to) = line.strip_prefix("rename to ") {
                file.renamed = true;
                file.new_path = to.trim().to_string();
                continue;
            }
            if line.starts_with("Binary files ") || line == "GIT binary patch" {
                file.binary = true;
                continue;
            }
        }

        if line.starts_with("@@") {
            let Some(file) = current_file.as_mut() else {
                return Err(patch_conflict(
                    PathBuf::new(),
                    "hunk appeared before file header",
                ));
            };
            if let Some(hunk) = current_hunk.take() {
                file.hunks.push(hunk);
            }
            current_hunk = Some(parse_hunk_header(line));
            continue;
        }

        let Some((prefix, content)) = split_patch_line(raw_line) else {
            continue;
        };
        if line.starts_with('\\') {
            continue;
        }
        // Rows with no `@@` above them belong to a synthesized whole-file
        // hunk (the `render_diff` shape); a row with no file header at all is
        // still ignored, exactly as before.
        if current_hunk.is_none() {
            if current_file.is_none() {
                continue;
            }
            current_hunk = Some(ScannedHunk {
                old_start: 1,
                new_start: 1,
                declared: false,
                ..ScannedHunk::default()
            });
        }
        let Some(hunk) = current_hunk.as_mut() else {
            continue;
        };
        hunk.rows.push((prefix, content));
    }

    finish_patch_hunk(&mut current_file, &mut current_hunk);
    if let Some(file) = current_file {
        files.push(file);
    }
    Ok(files)
}

/// Reads `@@ -old_start[,old_lines] +new_start[,new_lines] @@ [heading]`.
///
/// A malformed header yields a hunk with zero starts rather than an error:
/// the apply path never read this header at all, so refusing one here would
/// newly reject patches Core applies today.
fn parse_hunk_header(line: &str) -> ScannedHunk {
    let mut hunk = ScannedHunk {
        declared: true,
        ..ScannedHunk::default()
    };
    let Some(rest) = line.strip_prefix("@@") else {
        return hunk;
    };
    let Some((ranges, tail)) = rest.split_once("@@") else {
        return hunk;
    };
    let heading = tail.trim();
    if !heading.is_empty() {
        hunk.header = Some(heading.to_string());
    }
    for token in ranges.split_whitespace() {
        let Some((sign, span)) = token.split_at_checked(1) else {
            continue;
        };
        // A missing length means one line, which is what Git writes for a
        // single-line hunk; reading it as zero would misnumber every row.
        let (start, length) = match span.split_once(',') {
            Some((start, length)) => (start, length.parse::<u32>().unwrap_or(0)),
            None => (span, 1),
        };
        let start = start.parse::<u32>().unwrap_or(0);
        match sign {
            "-" => {
                hunk.old_start = start;
                hunk.old_lines = length;
            }
            "+" => {
                hunk.new_start = start;
                hunk.new_lines = length;
            }
            _ => {}
        }
    }
    hunk
}

fn header_path(value: &str) -> &str {
    value.trim().split_once('\t').map_or_else(
        || value.split_whitespace().next().unwrap_or(""),
        |(path, _)| path,
    )
}

fn finish_patch_hunk(file: &mut Option<ScannedFile>, hunk: &mut Option<ScannedHunk>) {
    if let (Some(file), Some(mut hunk)) = (file.as_mut(), hunk.take()) {
        if !hunk.declared {
            // A synthesized hunk has no header to read lengths from, so they
            // are counted off the rows it actually collected.
            hunk.old_lines = hunk
                .rows
                .iter()
                .filter(|(marker, _)| *marker != '+')
                .count() as u32;
            hunk.new_lines = hunk
                .rows
                .iter()
                .filter(|(marker, _)| *marker != '-')
                .count() as u32;
        }
        file.hunks.push(hunk);
    }
}

fn split_patch_line(raw_line: &str) -> Option<(char, String)> {
    let prefix = raw_line.chars().next()?;
    if !matches!(prefix, ' ' | '-' | '+') {
        return None;
    }
    Some((prefix, raw_line[prefix.len_utf8()..].to_string()))
}

fn apply_patch_file(current: &str, patch_file: &PatchFile) -> Result<String, String> {
    let mut lines = split_preserving_newlines(current);
    let mut cursor = 0usize;
    for hunk in &patch_file.hunks {
        let Some(index) = find_line_sequence(&lines, &hunk.old_lines, cursor) else {
            return Err("patch conflict: expected hunk context was not found".to_string());
        };
        lines.splice(index..index + hunk.old_lines.len(), hunk.new_lines.clone());
        cursor = index + hunk.new_lines.len();
    }
    Ok(lines.concat())
}

fn split_preserving_newlines(input: &str) -> Vec<String> {
    if input.is_empty() {
        Vec::new()
    } else {
        input
            .split_inclusive('\n')
            .map(ToString::to_string)
            .collect()
    }
}

fn find_line_sequence(lines: &[String], needle: &[String], start: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(start.min(lines.len()));
    }
    if needle.len() > lines.len() {
        return None;
    }
    (start..=lines.len() - needle.len())
        .find(|&index| lines[index..index + needle.len()] == *needle)
}

fn validate_patch_path(path: &str) -> Result<PathBuf, LaneEffectError> {
    let normalized = path
        .trim()
        .trim_start_matches("a/")
        .trim_start_matches("b/");
    if normalized.is_empty() || normalized == "/dev/null" {
        return Err(patch_conflict(
            PathBuf::new(),
            "patch path is empty or unsupported",
        ));
    }
    let candidate = Path::new(normalized);
    if candidate.is_absolute() {
        return Err(LaneEffectError::UnsafePath {
            path: normalized.to_string(),
        });
    }
    if candidate.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(LaneEffectError::UnsafePath {
            path: normalized.to_string(),
        });
    }
    Ok(candidate.to_path_buf())
}

fn resolve_patch_target(root: &Path, relative: &Path) -> Result<PathBuf, LaneEffectError> {
    let mut target = root.to_path_buf();
    for component in relative.components() {
        match component {
            Component::Normal(segment) => target.push(segment),
            Component::CurDir => continue,
            _ => {
                return Err(LaneEffectError::UnsafePath {
                    path: relative.display().to_string(),
                });
            }
        }
        match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(LaneEffectError::UnsafePath {
                    path: relative.display().to_string(),
                });
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(LaneEffectError::Io(format!(
                    "{}: {error}",
                    relative.display()
                )));
            }
        }
    }
    Ok(target)
}

fn ensure_patch_target_still_safe(root: &Path, target: &Path) -> Result<(), LaneEffectError> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| LaneEffectError::UnsafePath {
            path: target.display().to_string(),
        })?;
    let resolved = resolve_patch_target(root, relative)?;
    if resolved == target {
        Ok(())
    } else {
        Err(LaneEffectError::UnsafePath {
            path: relative.display().to_string(),
        })
    }
}
