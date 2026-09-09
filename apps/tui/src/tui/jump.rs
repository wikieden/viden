use viden_core::RuntimeViewState;

use super::workspace_files::{WorkspaceFileIndex, file_row_context};

/// Stable selector groups. The index only projects typed Core facts; it never
/// discovers lanes, sessions, gates, or files from the local workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum JumpKind {
    Gate,
    Ask,
    Lane,
    Session,
    Command,
    File,
}

impl JumpKind {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Gate => "GATES",
            Self::Ask => "ASKS",
            Self::Lane => "LANES",
            Self::Session => "SESSIONS",
            Self::Command => "COMMANDS",
            Self::File => "FILES",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct JumpItem {
    pub(super) kind: JumpKind,
    pub(super) id: String,
    pub(super) title: String,
    pub(super) context: String,
    pub(super) keywords: String,
    pub(super) parent_id: Option<String>,
    pub(super) enabled: bool,
    pub(super) disabled_reason: Option<String>,
    /// Whether this row's title is an identifier or a path, whose distinctive
    /// part sits at its END.
    ///
    /// Ids and paths share long prefixes — every `gate-acp-session-…` gate,
    /// every `.github/workflows/…` file — so a head cut renders them all as the
    /// same row. Those titles are truncated tail-first. Human-authored titles
    /// (a command, an approval, an empty-state sentence) read from the front
    /// and keep the plain head cut.
    pub(super) tail_distinctive: bool,
}

impl JumpItem {
    #[cfg(test)]
    fn enabled(
        kind: JumpKind,
        id: impl Into<String>,
        title: impl Into<String>,
        context: impl Into<String>,
        keywords: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            id: id.into(),
            title: title.into(),
            context: context.into(),
            keywords: keywords.into(),
            parent_id: None,
            enabled: true,
            disabled_reason: None,
            tail_distinctive: false,
        }
    }

    fn disabled(
        kind: JumpKind,
        id: impl Into<String>,
        title: impl Into<String>,
        context: impl Into<String>,
        keywords: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            id: id.into(),
            title: title.into(),
            context: context.into(),
            keywords: keywords.into(),
            parent_id: None,
            enabled: false,
            disabled_reason: Some(reason.into()),
            // Empty-state sentences read from the front.
            tail_distinctive: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct JumpQuery {
    text: String,
    kinds: Option<Vec<JumpKind>>,
}

impl JumpQuery {
    pub(super) fn parse(value: &str) -> Self {
        let (kinds, text) = match value.chars().next() {
            Some(':') => (Some(vec![JumpKind::Lane]), &value[1..]),
            Some('@') => (Some(vec![JumpKind::Session]), &value[1..]),
            Some('#') => (Some(vec![JumpKind::Gate, JumpKind::Ask]), &value[1..]),
            Some('>') => (Some(vec![JumpKind::Command]), &value[1..]),
            Some('~') => (Some(vec![JumpKind::File]), &value[1..]),
            _ => (None, value),
        };
        Self {
            text: text.trim().to_string(),
            kinds,
        }
    }

    #[cfg(test)]
    pub(super) fn kinds(&self) -> &[JumpKind] {
        self.kinds.as_deref().unwrap_or(&[])
    }

    fn accepts(&self, kind: JumpKind) -> bool {
        self.kinds
            .as_ref()
            .is_none_or(|kinds| kinds.contains(&kind))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct JumpIndex {
    items: Vec<JumpItem>,
}

impl JumpIndex {
    /// Projects typed Core facts plus the Core-published workspace inventory.
    ///
    /// `files` is client-local presentation state over pages Core sent; the
    /// index never discovers a path any other way.
    pub(super) fn from_view(view: &RuntimeViewState, files: &WorkspaceFileIndex) -> Self {
        let mut items = Vec::new();
        // Gates are listed active-first. A dormant gate — one still awaiting a
        // decision whose Agent session already finished — is tagged and ordered
        // last so it never buries an actionable gate. `Self::new` sorts by kind
        // with a stable sort, so this order survives grouping.
        let gate_item = |gate: &viden_core::MergeGateRecord, dormant: bool| JumpItem {
            kind: JumpKind::Gate,
            id: gate.gate_id.to_string(),
            title: format!("Merge gate {}", gate.gate_id),
            context: if dormant {
                format!("{} · {:?} · dormant", gate.task_id, gate.status)
            } else {
                format!("{} · {:?}", gate.task_id, gate.status)
            },
            keywords: if dormant {
                format!("dormant {}", gate.required_evidence.join(" "))
            } else {
                gate.required_evidence.join(" ")
            },
            parent_id: None,
            enabled: true,
            disabled_reason: None,
            tail_distinctive: true,
        };
        items.extend(
            view.merge_gates
                .iter()
                .filter(|gate| !super::decision::is_dormant_gate(view, gate))
                .map(|gate| gate_item(gate, false)),
        );
        items.extend(
            view.merge_gates
                .iter()
                .filter(|gate| super::decision::is_dormant_gate(view, gate))
                .map(|gate| gate_item(gate, true)),
        );
        items.extend(view.pending_approvals.iter().map(|approval| JumpItem {
            kind: JumpKind::Ask,
            id: approval.id.clone(),
            title: approval.title.clone(),
            context: approval.tool_name.clone(),
            keywords: format!(
                "{} {} {}",
                approval.message,
                approval.input_preview,
                approval.reason.as_deref().unwrap_or_default()
            ),
            parent_id: None,
            enabled: true,
            disabled_reason: None,
            tail_distinctive: false,
        }));
        items.extend(view.lanes.iter().map(|lane| JumpItem {
            kind: JumpKind::Lane,
            id: lane.id.clone(),
            title: lane.id.clone(),
            context: format!("{} · {:?}", lane.role, lane.status),
            keywords: format!("{} {}", lane.summary, lane.evidence.join(" ")),
            parent_id: None,
            enabled: true,
            disabled_reason: None,
            tail_distinctive: true,
        }));
        items.extend(view.lanes.iter().flat_map(|lane| {
            lane.active_session_ids
                .iter()
                .map(move |session_id| JumpItem {
                    kind: JumpKind::Session,
                    id: session_id.clone(),
                    title: session_id.clone(),
                    context: lane.id.clone(),
                    keywords: format!("{} {}", lane.role, lane.summary),
                    parent_id: Some(lane.id.clone()),
                    enabled: true,
                    disabled_reason: None,
                    tail_distinctive: true,
                })
        }));
        items.extend(
            super::command_palette::command_registry()
                .iter()
                .map(|command| JumpItem {
                    kind: JumpKind::Command,
                    id: command.command.to_string(),
                    title: command.command.to_string(),
                    context: command.summary.to_string(),
                    keywords: command.keywords.to_string(),
                    parent_id: None,
                    enabled: true,
                    disabled_reason: None,
                    tail_distinctive: false,
                }),
        );
        items.extend(workspace_file_items(files));
        Self::new(items)
    }

    #[cfg(test)]
    fn from_items(items: Vec<JumpItem>) -> Self {
        Self::new(items)
    }

    fn new(mut items: Vec<JumpItem>) -> Self {
        items.sort_by_key(|item| item.kind);
        Self { items }
    }

    #[cfg(test)]
    pub(super) fn items(&self) -> &[JumpItem] {
        &self.items
    }

    pub(super) fn search(&self, filter: &str) -> Vec<&JumpItem> {
        let query = JumpQuery::parse(filter);
        self.items
            .iter()
            .filter(|item| query.accepts(item.kind))
            .filter(|item| {
                query.text.is_empty()
                    || fuzzy_subsequence_score(&query.text, &item.title).is_some()
                    || fuzzy_subsequence_score(&query.text, &item.context).is_some()
                    || fuzzy_subsequence_score(&query.text, &item.keywords).is_some()
            })
            .collect()
    }
}

/// Builds the `~` scope's rows from what the client actually knows.
///
/// Five distinct facts, five distinct rows. Collapsing any of them into an
/// empty list would show an operator an inventory that does not exist:
/// "Core publishes none", "the read is in flight", "Core refused it", "the
/// workspace is empty", and the real listing are not the same statement.
fn workspace_file_items(files: &WorkspaceFileIndex) -> Vec<JumpItem> {
    let unavailable = |title: &str, context: &str, reason: &str| {
        vec![JumpItem::disabled(
            JumpKind::File,
            "core-file-inventory-unavailable",
            title,
            context,
            "file files path inventory",
            reason,
        )]
    };
    if !files.is_available() {
        return unavailable(
            "Files unavailable",
            "Core capability",
            "Core file inventory is unavailable.",
        );
    }
    if let Some(error) = files.error() {
        // Core's own sentence, verbatim: a permission denial must reach the
        // operator as Core wrote it.
        return unavailable("Files unavailable", "Core refused the read", error);
    }
    if !files.is_loaded() {
        let context = if files.is_reading() {
            "Reading"
        } else {
            "Not read yet"
        };
        return unavailable(
            "Files loading",
            context,
            "Reading the workspace file inventory.",
        );
    }
    if files.entries().is_empty() {
        return unavailable(
            "No workspace files",
            "Core inventory",
            "Core published an empty workspace file inventory.",
        );
    }
    files
        .entries()
        .iter()
        .map(|entry| JumpItem {
            kind: JumpKind::File,
            id: entry.path.clone(),
            title: entry.path.clone(),
            context: file_row_context(entry),
            keywords: entry.path.clone(),
            parent_id: None,
            enabled: true,
            disabled_reason: None,
            tail_distinctive: true,
        })
        .collect()
}

/// Returns a subsequence match score. Callers currently use only presence or
/// absence, preserving stable source and group ordering rather than ranking.
pub(super) fn fuzzy_subsequence_score(query: &str, candidate: &str) -> Option<usize> {
    let mut query = query.chars().flat_map(char::to_lowercase);
    let mut wanted = query.next()?;
    let mut score = 0;
    let mut previous = None;
    for (index, character) in candidate.chars().flat_map(char::to_lowercase).enumerate() {
        if character == wanted {
            score += index;
            if previous == Some(index.saturating_sub(1)) {
                score = score.saturating_sub(2);
            }
            previous = Some(index);
            match query.next() {
                Some(next) => wanted = next,
                None => return Some(score),
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::TuiState;
    use viden_core::{
        RuntimeEvent, RuntimeEventKind, WorkspaceFileEntry, WorkspaceFileKind, WorkspaceFilePage,
    };

    fn loaded_files(paths: &[(&str, WorkspaceFileKind)]) -> WorkspaceFileIndex {
        let mut index = WorkspaceFileIndex::default();
        index.mark_available(true);
        index.begin("files-1");
        index.apply_page(
            "files-1",
            &WorkspaceFilePage {
                entries: paths
                    .iter()
                    .map(|(path, kind)| WorkspaceFileEntry {
                        path: (*path).to_string(),
                        kind: *kind,
                        size_bytes: matches!(kind, WorkspaceFileKind::File).then_some(64),
                    })
                    .collect(),
                next_after: None,
                complete: true,
            },
        );
        index
    }

    /// The jump GATES section orders active gates before dormant ones and
    /// tags the dormant rows, matching the Decision Center.
    #[test]
    fn gate_rows_list_active_before_dormant_and_tag_the_dormant_ones() {
        use viden_core::{AgentSessionStatus, AgentSessionView, MergeGateRecord, MergeGateStatus};

        let session = |id: &str, status: AgentSessionStatus| AgentSessionView {
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
        let gate = |gate_id: &str, session_id: &str| MergeGateRecord {
            gate_id: gate_id.to_string(),
            task_id: format!("acp-session-{gate_id}"),
            status: MergeGateStatus::Proposed,
            required_evidence: Vec::new(),
            evidence_ids: Vec::new(),
            gate_type: Default::default(),
            owner: viden_core::RuntimeOwner {
                session_id: Some(session_id.to_string()),
                ..Default::default()
            },
            validator: None,
            policy_snapshot: Default::default(),
            decision: None,
            conflict: None,
            applied_change_id: None,
            recovery_snapshot: None,
            audit_ids: Vec::new(),
            updated_at: Some(1),
        };

        let mut state = TuiState::default();
        state
            .runtime
            .agent_sessions
            .push(session("session-done", AgentSessionStatus::Completed));
        state
            .runtime
            .agent_sessions
            .push(session("session-live", AgentSessionStatus::Running));
        // Source order deliberately puts the dormant gate first.
        state
            .runtime
            .merge_gates
            .push(gate("gate-dormant", "session-done"));
        state
            .runtime
            .merge_gates
            .push(gate("gate-active", "session-live"));

        let index = JumpIndex::from_view(&state.runtime, &WorkspaceFileIndex::default());
        let gates = index
            .items()
            .iter()
            .filter(|item| item.kind == JumpKind::Gate)
            .collect::<Vec<_>>();
        assert_eq!(
            gates
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["gate-active", "gate-dormant"],
            "the active gate must be listed first"
        );
        assert!(
            !gates[0].context.contains("dormant"),
            "an active gate carries no dormant tag: {:?}",
            gates[0].context
        );
        assert!(
            gates[1].context.contains("dormant"),
            "a dormant gate must carry the dormant context tag: {:?}",
            gates[1].context
        );
    }

    /// With the capability advertised and a page loaded, the `~` scope lists
    /// the real inventory Core published — no filesystem walking anywhere in
    /// the client.
    #[test]
    fn loaded_workspace_files_become_enabled_file_rows() {
        let state = TuiState::default();
        let files = loaded_files(&[
            ("README.md", WorkspaceFileKind::File),
            ("src", WorkspaceFileKind::Dir),
            ("src/main.rs", WorkspaceFileKind::File),
        ]);
        let index = JumpIndex::from_view(&state.runtime, &files);
        let rows = index
            .items()
            .iter()
            .filter(|item| item.kind == JumpKind::File)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|row| row.enabled));
        assert_eq!(
            rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            vec!["README.md", "src", "src/main.rs"],
            "rows must keep Core's lexicographic order"
        );
        // The `~` scope reaches exactly those rows and nothing else.
        assert_eq!(index.search("~").len(), 3);
        assert_eq!(index.search("~main").len(), 1);
    }

    /// A page whose command id is not the one in flight belongs to another
    /// read. Since `command_id` is required on this event there is no
    /// acceptance fallback to fall back to, so the page is simply ignored.
    #[test]
    fn a_page_for_another_read_never_becomes_file_rows() {
        let mut files = WorkspaceFileIndex::default();
        files.mark_available(true);
        files.begin("files-mine");
        let applied = files.apply_page(
            "files-theirs",
            &WorkspaceFilePage {
                entries: vec![WorkspaceFileEntry {
                    path: "other/project.rs".to_string(),
                    kind: WorkspaceFileKind::File,
                    size_bytes: Some(1),
                }],
                next_after: None,
                complete: true,
            },
        );
        assert!(!applied, "another reader's page must not be applied");
        assert!(!files.is_loaded());
        let state = TuiState::default();
        let index = JumpIndex::from_view(&state.runtime, &files);
        let rows = index
            .items()
            .iter()
            .filter(|item| item.kind == JumpKind::File)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].enabled, "an unanswered read must stay disabled");
    }

    /// An unrelated failure landing while a read is outstanding must not be
    /// read as this read's refusal.
    ///
    /// A lane or provider `Error` carries no command id, so attributing it to
    /// whatever read happens to be in flight would fabricate a refusal Core
    /// never issued — and would hide the real inventory answer behind it. Core
    /// rejects a refused inventory read with the exact command id instead, so
    /// this index has no reason to look at `Error` at all.
    #[test]
    fn an_unrelated_error_never_fails_an_outstanding_read() {
        let mut files = WorkspaceFileIndex::default();
        files.mark_available(true);
        files.begin("files-mine");
        assert!(
            !files.observe_event(&RuntimeEvent::new(
                1,
                RuntimeEventKind::Error {
                    error: viden_core::RuntimeErrorView {
                        message: "lane lane-other failed to start".to_string(),
                        recoverable: true,
                        hint: None,
                    },
                },
            )),
            "an unrelated error must not touch the inventory index"
        );
        assert_eq!(files.error(), None);
        assert!(files.is_reading(), "our read is still outstanding");

        // A rejection for another reader's command id is equally not ours.
        assert!(!files.observe_event(&RuntimeEvent::new(
            2,
            RuntimeEventKind::CommandRejected {
                command_id: "files-theirs".to_string(),
                reason: "someone else was refused".to_string(),
            },
        )));
        assert_eq!(files.error(), None);

        // The row still says "reading", never "unavailable".
        let state = TuiState::default();
        let index = JumpIndex::from_view(&state.runtime, &files);
        let row = index
            .items()
            .iter()
            .find(|item| item.kind == JumpKind::File)
            .expect("file row");
        assert_eq!(
            row.disabled_reason.as_deref(),
            Some("Reading the workspace file inventory.")
        );
    }

    /// Capability advertised but nothing has arrived yet: the row states that
    /// it is still reading, which is a different fact from "no files".
    #[test]
    fn an_in_flight_read_renders_a_loading_row_not_an_empty_list() {
        let mut files = WorkspaceFileIndex::default();
        files.mark_available(true);
        files.begin("files-1");
        let state = TuiState::default();
        let index = JumpIndex::from_view(&state.runtime, &files);
        let row = index
            .items()
            .iter()
            .find(|item| item.kind == JumpKind::File)
            .expect("file row");
        assert!(!row.enabled);
        assert_eq!(
            row.disabled_reason.as_deref(),
            Some("Reading the workspace file inventory.")
        );
    }

    /// An answered read over an empty workspace says so, and never borrows the
    /// unavailable-capability sentence.
    #[test]
    fn an_empty_inventory_is_stated_as_empty_not_as_unavailable() {
        let files = loaded_files(&[]);
        let state = TuiState::default();
        let index = JumpIndex::from_view(&state.runtime, &files);
        let row = index
            .items()
            .iter()
            .find(|item| item.kind == JumpKind::File)
            .expect("file row");
        assert!(!row.enabled);
        assert_eq!(
            row.disabled_reason.as_deref(),
            Some("Core published an empty workspace file inventory.")
        );
    }

    /// Core's rejection is rendered verbatim — a permission denial must reach
    /// the operator as Core wrote it, never as a locally composed sentence and
    /// never as an empty list.
    ///
    /// The refusal arrives as `CommandRejected` naming this exact read, so it
    /// is attributable; see the unrelated-error test below for why that
    /// matters.
    #[test]
    fn a_rejected_read_shows_cores_own_reason() {
        let mut files = WorkspaceFileIndex::default();
        files.mark_available(true);
        files.begin("files-1");
        assert!(files.observe_event(&RuntimeEvent::new(
            1,
            RuntimeEventKind::CommandRejected {
                command_id: "files-1".to_string(),
                reason: "Permission decision: deny (workspace_file_inventory)".to_string(),
            },
        )));
        let state = TuiState::default();
        let index = JumpIndex::from_view(&state.runtime, &files);
        let row = index
            .items()
            .iter()
            .find(|item| item.kind == JumpKind::File)
            .expect("file row");
        assert!(!row.enabled);
        assert_eq!(
            row.disabled_reason.as_deref(),
            Some("Permission decision: deny (workspace_file_inventory)")
        );
    }

    #[test]
    fn index_groups_typed_runtime_facts_in_stable_order_and_keeps_file_unavailable() {
        let state = TuiState::default();
        let index = JumpIndex::from_view(&state.runtime, &WorkspaceFileIndex::default());

        assert_eq!(
            index
                .items()
                .iter()
                .map(|item| item.kind)
                .collect::<Vec<_>>(),
            vec![
                // Fifteen registered commands since `/git` joined the registry
                // (`runtime.operator_git`, GUI-CORE-020).
                JumpKind::Command,
                JumpKind::Command,
                JumpKind::Command,
                JumpKind::Command,
                JumpKind::Command,
                JumpKind::Command,
                JumpKind::Command,
                JumpKind::Command,
                JumpKind::Command,
                JumpKind::Command,
                JumpKind::Command,
                JumpKind::Command,
                JumpKind::Command,
                JumpKind::Command,
                JumpKind::Command,
                JumpKind::File,
            ],
        );
        let file = index
            .items()
            .iter()
            .find(|item| item.kind == JumpKind::File)
            .expect("file capability row");
        assert!(!file.enabled);
        assert_eq!(
            file.disabled_reason.as_deref(),
            Some("Core file inventory is unavailable."),
        );
    }

    #[test]
    fn group_order_is_gate_ask_lane_session_command_file() {
        let index = JumpIndex::from_items(vec![
            JumpItem::enabled(JumpKind::File, "file", "File", "", ""),
            JumpItem::enabled(JumpKind::Command, "command", "Command", "", ""),
            JumpItem::enabled(JumpKind::Session, "session", "Session", "", ""),
            JumpItem::enabled(JumpKind::Lane, "lane", "Lane", "", ""),
            JumpItem::enabled(JumpKind::Ask, "ask", "Ask", "", ""),
            JumpItem::enabled(JumpKind::Gate, "gate", "Gate", "", ""),
        ]);

        assert_eq!(
            index
                .items()
                .iter()
                .map(|item| item.kind)
                .collect::<Vec<_>>(),
            vec![
                JumpKind::Gate,
                JumpKind::Ask,
                JumpKind::Lane,
                JumpKind::Session,
                JumpKind::Command,
                JumpKind::File,
            ],
        );
    }

    #[test]
    fn fuzzy_matching_checks_title_context_and_keywords() {
        let index = JumpIndex::from_items(vec![
            JumpItem::enabled(JumpKind::Lane, "lane-1", "Compiler", "Build lane", "rust"),
            JumpItem::enabled(
                JumpKind::Session,
                "session-1",
                "Notes",
                "Review context",
                "audit",
            ),
        ]);

        assert_eq!(index.search("cmplr").len(), 1);
        assert_eq!(index.search("rvw").len(), 1);
        assert_eq!(index.search("adt").len(), 1);
    }

    #[test]
    fn query_sigils_scope_every_supported_kind() {
        assert_eq!(JumpQuery::parse(":lane").kinds(), &[JumpKind::Lane]);
        assert_eq!(JumpQuery::parse("@session").kinds(), &[JumpKind::Session]);
        assert_eq!(
            JumpQuery::parse("#gate").kinds(),
            &[JumpKind::Gate, JumpKind::Ask]
        );
        assert_eq!(JumpQuery::parse(">help").kinds(), &[JumpKind::Command]);
        assert_eq!(JumpQuery::parse("~src").kinds(), &[JumpKind::File]);
    }

    #[test]
    fn empty_and_no_match_queries_are_explicit() {
        let index = JumpIndex::from_items(vec![JumpItem::enabled(
            JumpKind::Lane,
            "lane-1",
            "Compiler",
            "Build lane",
            "rust",
        )]);

        assert_eq!(index.search("").len(), 1);
        assert!(index.search("missing").is_empty());
    }

    #[test]
    fn fuzzy_subsequence_returns_no_score_when_characters_are_missing() {
        assert!(fuzzy_subsequence_score("cmp", "Compiler").is_some());
        assert!(fuzzy_subsequence_score("xyz", "Compiler").is_none());
    }
}
