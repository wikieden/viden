//! Panel-local state for the read-only audit timeline overlay.
//!
//! The TUI is the first client of the Core audit contract
//! (`crates/types/src/audit.rs`, `RuntimeCommand::QueryAudit` ->
//! `RuntimeEventKind::AuditPageLoaded`). Two rules shape this module:
//!
//! - **This is a read, not a decision.** It deliberately does *not* reuse
//!   [`super::pending::SupervisionMachine`]: that machine owns a single
//!   in-flight slot for supervision *mutations*, and an audit read must never
//!   block, or be blocked by, a pending gate/review/revert decision. The
//!   correlation lives here, scoped to one open overlay.
//! - **Nothing is inferred.** Records are rendered exactly as Core delivered
//!   them: the dotted `action` key is never localized, an unknown outcome never
//!   borrows a known status, and an empty list is only called empty once a page
//!   has actually arrived.
//!
//! Correlation: a page Core publishes names the exact `QueryAudit` command id
//! it answers (GUI-CORE-024), so this panel accepts a page only when that id is
//! the one it is awaiting. A page carrying *another* reader's id is ignored
//! even while a query of ours is in flight.
//!
//! The residual limitation applies only against a Core that predates that
//! field: such a page arrives with `command_id: None`, and the only attribution
//! left is the awaiting slot, so a concurrent reader's page can still be
//! attributed to this panel's in-flight query. That is accepted rather than
//! papered over — the page is still a real Core page, it is discarded when the
//! overlay closes, and the next query self-corrects. Speculative correlation
//! (matching on record contents, or guessing from the cursor) would invent
//! certainty the contract does not provide, so it is not built.

use std::collections::BTreeMap;

use viden_core::{
    AuditCursor, AuditObjectRef, AuditOutcome, AuditPage, AuditQuery, AuditRecord, RuntimeEvent,
    RuntimeEventKind,
};

use super::{glyphs::Glyph, text::truncate};

/// Page size for one `QueryAudit`. Core clamps to `1..=MAX_AUDIT_PAGE_SIZE`, so
/// this is a readability choice, not a protocol bound: a page an operator can
/// scroll, with older records reachable through the load-older row.
pub(super) const AUDIT_PAGE_LIMIT: u32 = 100;

/// Display width one rendered audit row is truncated to.
///
/// The overlay box is `frame.width.min(76)` and [`super::panel::bordered_row`]
/// spends four cells on borders and padding, leaving 72; the selection marker
/// takes two more. Matching the fixed widths the other overlays already use
/// keeps the row stable regardless of terminal size.
pub(super) const AUDIT_ROW_WIDTH: usize = 70;

/// Glyph for an audit outcome this build does not know.
///
/// [`AuditOutcome`] is `#[non_exhaustive]`, so a newer Core may publish an
/// outcome that is neither success nor failure. Rendering literal ASCII `?` is
/// the honest answer: it is visibly "not a status this build understands"
/// rather than a fabricated success or failure.
const UNKNOWN_OUTCOME_GLYPH: &str = "?";

/// TUI-local state of one open audit timeline overlay.
///
/// Presentation only. It holds no authoritative record: every field is either a
/// command id this client issued, or bytes Core published back. Closing the
/// overlay drops the whole struct, so a reopened overlay always re-queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AuditPanel {
    /// `None` browses the project timeline. Core's store is already scoped to
    /// the project's own workflow directory, so an unscoped query needs no
    /// locally invented `project_id`.
    scope: Option<AuditObjectRef>,
    /// Newest-first, exactly as Core delivered. Later pages are older, so they
    /// append at the end.
    records: Vec<AuditRecord>,
    next_before: Option<AuditCursor>,
    complete: bool,
    /// Command id of the in-flight `QueryAudit`, or `None` when idle.
    awaiting: Option<String>,
    /// Core's rejection reason, verbatim. Never a locally composed sentence.
    error: Option<String>,
    /// Catalog key of the last local refusal (a second query while one is in
    /// flight). Local refusals send nothing, so they are never Core errors and
    /// must not be rendered as one.
    notice: Option<&'static str>,
    /// Whether at least one page has arrived. Absence and emptiness are
    /// different facts: "nothing loaded yet" must never render as "no records".
    loaded: bool,
    selected: usize,
}

impl AuditPanel {
    pub(super) fn new(scope: Option<AuditObjectRef>) -> Self {
        Self {
            scope,
            records: Vec::new(),
            next_before: None,
            complete: false,
            awaiting: None,
            error: None,
            notice: None,
            loaded: false,
            selected: 0,
        }
    }

    pub(super) fn scope(&self) -> Option<&AuditObjectRef> {
        self.scope.as_ref()
    }

    /// The query for the next page: the first page when nothing has loaded,
    /// otherwise the page older than the cursor Core handed back.
    pub(super) fn next_query(&self) -> AuditQuery {
        AuditQuery {
            object: self.scope.clone(),
            before: self.next_before.clone(),
            limit: AUDIT_PAGE_LIMIT,
            // Core's actor and time filters exist (GUI-CORE-024) but the TUI
            // overlay offers no filter control yet, so it asks for the whole
            // timeline rather than sending a filter no operator chose.
            ..AuditQuery::default()
        }
    }

    /// Registers the in-flight query this panel is waiting for.
    pub(super) fn begin(&mut self, command_id: impl Into<String>) {
        self.awaiting = Some(command_id.into());
        self.error = None;
        self.notice = None;
    }

    /// Whether a load-older query may be dispatched right now.
    pub(super) fn can_load_older(&self) -> bool {
        self.shows_load_older_row() && self.awaiting.is_none()
    }

    /// Refuses a second query locally. Nothing is sent, so Core never sees the
    /// refused intent and the in-flight page keeps its correlation.
    pub(super) fn refuse_second_query(&mut self) {
        self.notice = Some("audit.busy");
    }

    /// Reconciles one ordered Core event against the in-flight query.
    ///
    /// Returns whether this event changed the panel.
    pub(super) fn observe_event(&mut self, event: &RuntimeEvent) -> bool {
        match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, reason }
                if self.awaiting.as_deref() == Some(command_id.as_str()) =>
            {
                self.awaiting = None;
                self.error = Some(reason.clone());
                true
            }
            // Exact correlation when Core names the read: only the page for
            // the command id this panel is awaiting is applied, so another
            // reader's concurrent page can never replace what the operator is
            // looking at. See the module note for the `None` case.
            RuntimeEventKind::AuditPageLoaded { command_id, page }
                if self.accepts_page(command_id.as_deref()) =>
            {
                self.apply_page(page);
                true
            }
            _ => false,
        }
    }

    /// Whether a delivered page belongs to this panel's in-flight query.
    ///
    /// `Some(id)` is Core naming the read it answered: the page counts only
    /// when that id is exactly the one being awaited. `None` is a Core that
    /// predates the field, where the awaiting slot is the only attribution
    /// available; either way a page with no query in flight belongs to some
    /// other reader and is ignored.
    fn accepts_page(&self, command_id: Option<&str>) -> bool {
        match (self.awaiting.as_deref(), command_id) {
            (Some(awaiting), Some(published)) => awaiting == published,
            (Some(_), None) => true,
            (None, _) => false,
        }
    }

    fn apply_page(&mut self, page: &AuditPage) {
        if self.records.is_empty() {
            self.records = page.records.clone();
        } else {
            self.records.extend(page.records.iter().cloned());
        }
        self.next_before = page.next_before.clone();
        self.complete = page.complete;
        self.awaiting = None;
        self.loaded = true;
        self.selected = self.selected.min(self.row_count().saturating_sub(1));
    }

    pub(super) fn records(&self) -> &[AuditRecord] {
        &self.records
    }

    pub(super) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(super) fn notice(&self) -> Option<&'static str> {
        self.notice
    }

    pub(super) fn is_loading(&self) -> bool {
        self.awaiting.is_some()
    }

    pub(super) fn is_complete(&self) -> bool {
        self.complete
    }

    /// True only when Core answered with an empty page. A panel that has not
    /// loaded anything yet is loading or failed, never "empty".
    pub(super) fn is_empty_result(&self) -> bool {
        self.loaded && self.records.is_empty()
    }

    /// Whether the load-older row is offered at all. A complete timeline hides
    /// it: there is nothing older to ask for.
    pub(super) fn shows_load_older_row(&self) -> bool {
        !self.complete && self.next_before.is_some()
    }

    pub(super) fn row_count(&self) -> usize {
        self.records.len() + usize::from(self.shows_load_older_row())
    }

    pub(super) fn selected(&self) -> usize {
        self.selected
    }

    pub(super) fn selected_is_load_older(&self) -> bool {
        self.shows_load_older_row() && self.selected == self.records.len()
    }

    pub(super) fn move_selection(&mut self, delta: i8) {
        let count = self.row_count();
        if count == 0 {
            self.selected = 0;
            return;
        }
        self.selected = if delta < 0 {
            self.selected.saturating_sub(1)
        } else {
            self.selected.saturating_add(1).min(count - 1)
        };
    }
}

/// Registered glyph for one audit outcome.
///
/// Success and failure use the registered `✓` / `✗` vocabulary. An outcome this
/// build does not know renders literal ASCII `?`; see [`UNKNOWN_OUTCOME_GLYPH`].
pub(super) fn audit_outcome_glyph(outcome: AuditOutcome) -> &'static str {
    match outcome {
        AuditOutcome::Success => Glyph::Done.unicode(),
        AuditOutcome::Denied | AuditOutcome::Failed => Glyph::Fail.unicode(),
        // `AuditOutcome` is `#[non_exhaustive]`: this arm is unreachable from
        // this build's own variants and is compile-checked only, but it is what
        // keeps a newer Core's outcome from borrowing a status it never had.
        _ => UNKNOWN_OUTCOME_GLYPH,
    }
}

/// Renders a Core unix-seconds timestamp as `HH:MM:SS` UTC.
///
/// The TUI has no existing absolute-time surface and no date library, so the
/// clock time is derived arithmetically from the epoch second. UTC is chosen
/// deliberately: an audit record is evidence that gets compared across machines,
/// and a locale-shifted time would make two readers disagree about one fact.
pub(super) fn format_audit_time(unix_seconds: u64) -> String {
    let second_of_day = unix_seconds % 86_400;
    format!(
        "{:02}:{:02}:{:02}",
        second_of_day / 3_600,
        (second_of_day % 3_600) / 60,
        second_of_day % 60
    )
}

/// One audit row: `{time} {action} {outcome} {objects} {args}`.
///
/// `action` is rendered raw. It is Core's stable dotted vocabulary
/// (`gate.decided`, `handoff.created`, ...), which the contract keeps
/// deliberately free of prose so a reader can diff two timelines; localizing it
/// would destroy exactly that property. Overlay chrome around the row is
/// localized instead.
pub(super) fn audit_row(record: &AuditRecord, width: usize) -> String {
    let objects = truncate(&format_audit_objects(&record.objects), width);
    let args = truncate(&format_audit_args(&record.args), width);
    let row = format!(
        "{} {} {} {objects} {args}",
        format_audit_time(record.timestamp),
        record.action,
        audit_outcome_glyph(record.outcome),
    );
    truncate(row.trim_end(), width)
}

fn format_audit_objects(objects: &[AuditObjectRef]) -> String {
    objects
        .iter()
        .map(|object| format!("{}:{}", object.kind, object.id))
        .collect::<Vec<_>>()
        .join(",")
}

fn format_audit_args(args: &BTreeMap<String, String>) -> String {
    args.iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use viden_core::{AuditActor, RuntimeOwner};

    fn record(audit_id: &str, timestamp: u64, outcome: AuditOutcome) -> AuditRecord {
        AuditRecord {
            audit_id: audit_id.to_string(),
            timestamp,
            owner: RuntimeOwner::default(),
            actor: AuditActor::Operator,
            action: "gate.decided".to_string(),
            objects: vec![AuditObjectRef::new(
                AuditObjectRef::KIND_MERGE_GATE,
                "gate-1",
            )],
            outcome,
            args: BTreeMap::from([("decision".to_string(), "accepted".to_string())]),
        }
    }

    fn page(records: Vec<AuditRecord>, next_before: Option<AuditCursor>) -> AuditPage {
        AuditPage {
            complete: next_before.is_none(),
            records,
            next_before,
        }
    }

    fn event(sequence: u64, kind: RuntimeEventKind) -> RuntimeEvent {
        RuntimeEvent {
            sequence,
            timestamp: Some(sequence),
            kind,
        }
    }

    fn cursor(timestamp: u64, audit_id: &str) -> AuditCursor {
        AuditCursor {
            timestamp,
            audit_id: audit_id.to_string(),
        }
    }

    #[test]
    fn the_first_query_is_unscoped_or_object_scoped_and_pages_from_the_returned_cursor() {
        let panel = AuditPanel::new(None);
        assert_eq!(
            panel.next_query(),
            AuditQuery {
                limit: AUDIT_PAGE_LIMIT,
                ..AuditQuery::default()
            }
        );

        let scope = AuditObjectRef::new(AuditObjectRef::KIND_REVIEW_REQUEST, "review-1");
        let mut panel = AuditPanel::new(Some(scope.clone()));
        assert_eq!(panel.next_query().object, Some(scope.clone()));
        assert_eq!(panel.next_query().before, None);

        panel.begin("tui-1");
        panel.observe_event(&event(
            1,
            RuntimeEventKind::AuditPageLoaded {
                command_id: None,
                page: page(
                    vec![record("a-2", 20, AuditOutcome::Success)],
                    Some(cursor(20, "a-2")),
                ),
            },
        ));
        assert_eq!(panel.next_query().before, Some(cursor(20, "a-2")));
        assert_eq!(
            panel.next_query().object,
            Some(scope),
            "paging must not widen the scope the operator opened"
        );
    }

    #[test]
    fn the_first_page_replaces_and_older_pages_append_in_delivery_order() {
        let mut panel = AuditPanel::new(None);
        panel.begin("tui-1");
        assert!(panel.is_loading());
        assert!(!panel.is_empty_result(), "nothing loaded is not emptiness");

        panel.observe_event(&event(
            1,
            RuntimeEventKind::AuditPageLoaded {
                command_id: None,
                page: page(
                    vec![
                        record("a-3", 30, AuditOutcome::Success),
                        record("a-2", 20, AuditOutcome::Denied),
                    ],
                    Some(cursor(20, "a-2")),
                ),
            },
        ));
        assert!(!panel.is_loading());
        assert!(!panel.is_complete());
        assert!(panel.shows_load_older_row());
        assert_eq!(panel.records().len(), 2);
        assert_eq!(panel.row_count(), 3, "the load-older row is selectable");

        assert!(panel.can_load_older());
        panel.begin("tui-2");
        assert!(
            !panel.can_load_older(),
            "a query is already in flight for this panel"
        );
        panel.observe_event(&event(
            2,
            RuntimeEventKind::AuditPageLoaded {
                command_id: None,
                page: page(vec![record("a-1", 10, AuditOutcome::Failed)], None),
            },
        ));

        assert_eq!(
            panel
                .records()
                .iter()
                .map(|record| record.audit_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a-3", "a-2", "a-1"],
            "records are newest-first, so an older page appends at the end"
        );
        assert!(panel.is_complete());
        assert!(
            !panel.shows_load_older_row(),
            "a complete timeline offers nothing older"
        );
        assert_eq!(panel.row_count(), 3);
    }

    #[test]
    fn an_empty_page_is_emptiness_only_after_it_arrives() {
        let mut panel = AuditPanel::new(None);
        assert!(!panel.is_empty_result());

        panel.begin("tui-1");
        assert!(!panel.is_empty_result());

        panel.observe_event(&event(
            1,
            RuntimeEventKind::AuditPageLoaded {
                command_id: None,
                page: page(Vec::new(), None),
            },
        ));
        assert!(panel.is_empty_result());
        assert!(!panel.is_loading());
        assert_eq!(panel.row_count(), 0);
    }

    #[test]
    fn only_a_rejection_for_this_query_becomes_an_error_and_it_is_cores_own_reason() {
        let mut panel = AuditPanel::new(None);
        panel.begin("tui-1");

        assert!(!panel.observe_event(&event(
            1,
            RuntimeEventKind::CommandRejected {
                command_id: "tui-other".to_string(),
                reason: "someone else".to_string(),
            }
        )));
        assert_eq!(panel.error(), None);
        assert!(panel.is_loading());

        assert!(panel.observe_event(&event(
            2,
            RuntimeEventKind::CommandRejected {
                command_id: "tui-1".to_string(),
                reason: "audit store unavailable".to_string(),
            }
        )));
        assert_eq!(panel.error(), Some("audit store unavailable"));
        assert!(!panel.is_loading());
    }

    #[test]
    fn a_page_with_nothing_in_flight_belongs_to_another_reader_and_is_ignored() {
        let mut panel = AuditPanel::new(None);

        assert!(!panel.observe_event(&event(
            1,
            RuntimeEventKind::AuditPageLoaded {
                command_id: None,
                page: page(vec![record("a-1", 10, AuditOutcome::Success)], None),
            }
        )));
        assert!(panel.records().is_empty());
        assert!(
            !panel.is_empty_result(),
            "an ignored page must not claim the timeline is empty"
        );
        assert!(!panel.is_complete());
    }

    /// The three correlation cases the contract now distinguishes.
    ///
    /// A page naming this panel's read is applied; a page naming somebody
    /// else's read is ignored even while ours is in flight; a page from a Core
    /// that predates the field keeps the legacy awaiting-gated behavior.
    #[test]
    fn a_page_is_applied_only_when_it_names_this_panels_read() {
        let page_for =
            |command_id: Option<&str>, audit_id: &str| RuntimeEventKind::AuditPageLoaded {
                command_id: command_id.map(ToString::to_string),
                page: page(vec![record(audit_id, 10, AuditOutcome::Success)], None),
            };

        // 1. Another reader's page is ignored, even though a query of ours is
        //    in flight and would have accepted an unnamed page.
        let mut panel = AuditPanel::new(None);
        panel.begin("tui-1");
        assert!(!panel.observe_event(&event(1, page_for(Some("gui-9"), "other-1"))));
        assert!(panel.records().is_empty());
        assert!(panel.is_loading(), "our read is still outstanding");
        assert!(
            !panel.is_empty_result(),
            "an ignored page must not claim the timeline is empty"
        );

        // 2. Our own page, named by Core, is applied.
        assert!(panel.observe_event(&event(2, page_for(Some("tui-1"), "ours-1"))));
        assert_eq!(panel.records()[0].audit_id, "ours-1");
        assert!(!panel.is_loading());

        // 3. A page from a Core that predates the field carries no id, so the
        //    awaiting slot is the only attribution left and still applies.
        let mut legacy = AuditPanel::new(None);
        legacy.begin("tui-2");
        assert!(legacy.observe_event(&event(1, page_for(None, "legacy-1"))));
        assert_eq!(legacy.records()[0].audit_id, "legacy-1");

        // ...but only while a query is in flight; an idle panel still ignores
        // an unnamed page.
        assert!(!legacy.observe_event(&event(2, page_for(None, "legacy-2"))));
        assert_eq!(legacy.records().len(), 1);
    }

    #[test]
    fn a_second_query_while_one_is_in_flight_is_refused_locally() {
        let mut panel = AuditPanel::new(None);
        panel.begin("tui-1");
        panel.observe_event(&event(
            1,
            RuntimeEventKind::AuditPageLoaded {
                command_id: None,
                page: page(
                    vec![record("a-2", 20, AuditOutcome::Success)],
                    Some(cursor(20, "a-2")),
                ),
            },
        ));
        panel.begin("tui-2");

        assert!(!panel.can_load_older());
        panel.refuse_second_query();
        assert_eq!(panel.notice(), Some("audit.busy"));
        assert_eq!(panel.error(), None, "a local refusal is not a Core error");

        // Beginning the next real query clears the local refusal.
        panel.observe_event(&event(
            2,
            RuntimeEventKind::AuditPageLoaded {
                command_id: None,
                page: page(Vec::new(), None),
            },
        ));
        panel.begin("tui-3");
        assert_eq!(panel.notice(), None);
    }

    #[test]
    fn selection_walks_records_then_the_load_older_row_and_never_leaves_the_list() {
        let mut panel = AuditPanel::new(None);
        panel.begin("tui-1");
        panel.observe_event(&event(
            1,
            RuntimeEventKind::AuditPageLoaded {
                command_id: None,
                page: page(
                    vec![
                        record("a-3", 30, AuditOutcome::Success),
                        record("a-2", 20, AuditOutcome::Success),
                    ],
                    Some(cursor(20, "a-2")),
                ),
            },
        ));

        assert_eq!(panel.selected(), 0);
        assert!(!panel.selected_is_load_older());
        panel.move_selection(-1);
        assert_eq!(panel.selected(), 0, "selection never walks off the top");
        for _ in 0..5 {
            panel.move_selection(1);
        }
        assert_eq!(panel.selected(), 2);
        assert!(panel.selected_is_load_older());

        // A completing page removes the load-older row, so the selection is
        // clamped back onto a real record instead of pointing at nothing.
        panel.begin("tui-2");
        panel.observe_event(&event(
            2,
            RuntimeEventKind::AuditPageLoaded {
                command_id: None,
                page: page(vec![record("a-1", 10, AuditOutcome::Success)], None),
            },
        ));
        assert_eq!(panel.selected(), 2);
        assert!(!panel.selected_is_load_older());
    }

    #[test]
    fn a_row_renders_the_raw_action_key_registered_outcome_glyphs_objects_and_args() {
        let mut success = record("a-1", 3_723, AuditOutcome::Success);
        success
            .objects
            .push(AuditObjectRef::new(AuditObjectRef::KIND_TASK, "task-1"));
        success.args.insert("gate".to_string(), "patch".to_string());

        // Rendered wide so the whole row is visible; the width bound itself is
        // asserted separately below.
        let rendered = audit_row(&success, 100);
        assert!(
            rendered.starts_with("01:02:03 gate.decided ✓ "),
            "{rendered}"
        );
        assert!(
            rendered.contains("merge_gate:gate-1,task:task-1"),
            "objects render as comma-joined kind:id pairs: {rendered}"
        );
        assert!(
            rendered.contains("decision=accepted gate=patch"),
            "args render as space-joined k=v pairs: {rendered}"
        );
        assert!(
            !rendered.contains("Gate decided"),
            "the dotted action key is Core's stable vocabulary and is never localized: {rendered}"
        );

        // Both failure outcomes share the registered failure glyph; there is no
        // third known status to invent one for.
        assert_eq!(audit_outcome_glyph(AuditOutcome::Success), "✓");
        assert_eq!(audit_outcome_glyph(AuditOutcome::Denied), "✗");
        assert_eq!(audit_outcome_glyph(AuditOutcome::Failed), "✗");
        assert_eq!(UNKNOWN_OUTCOME_GLYPH, "?");
        assert!(
            UNKNOWN_OUTCOME_GLYPH.is_ascii(),
            "the unknown-outcome fallback stays literal ASCII, not a borrowed glyph"
        );
    }

    /// The approval audit row `runtime.durable_work_evidence` (C7) made
    /// durable, replayed from Core's own fixture.
    ///
    /// Before C7 the `audit_id` an approval prompt showed the operator was a
    /// live correlation id written nowhere, so every "audit" link for an
    /// approval resolved to an empty timeline (E1 defect 4). `RespondToApproval`
    /// now appends the row under that exact pre-minted id, and this lens needs
    /// no new code to show it: the dotted action key is Core's stable
    /// vocabulary and is rendered raw, which is why `approval.allow_once`
    /// arrives readable without a catalog entry per decision.
    #[test]
    fn the_durable_work_evidence_fixture_replays_the_approval_decision_into_the_audit_lens() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/durable-work-evidence.json"
        ))
        .expect("durable work evidence fixture");
        let events = fixture["events"]
            .as_array()
            .expect("fixture events")
            .iter()
            .filter_map(|envelope| {
                let envelope: viden_core::RuntimeEventEnvelope =
                    serde_json::from_value(envelope.clone()).expect("fixture envelope");
                match envelope.event {
                    viden_core::RuntimeWireEvent::Known(event) => Some(event),
                    viden_core::RuntimeWireEvent::Unknown { .. } => None,
                }
            })
            .collect::<Vec<_>>();

        let mut panel = AuditPanel::new(None);
        // The fixture's own command id for the audit read.
        panel.begin("cmd_durable_work_audit");
        for event in &events {
            panel.observe_event(event);
        }

        let records = panel.records();
        assert_eq!(records.len(), 1, "{records:?}");
        let approval = &records[0];
        assert_eq!(approval.audit_id, "audit_durable_work_approval");
        assert_eq!(approval.actor, AuditActor::Operator);
        assert_eq!(approval.action, "approval.allow_once");

        let rendered = audit_row(approval, 120);
        assert!(rendered.contains("approval.allow_once"), "{rendered}");
        assert!(
            rendered.contains("permission:approval_durable_work"),
            "the row names the approval request it resolved: {rendered}"
        );
        assert!(
            rendered.contains("tool:edit_file"),
            "and the tool the decision released: {rendered}"
        );
    }

    #[test]
    fn a_row_is_truncated_to_the_overlay_width_by_display_width() {
        let mut wide = record("a-1", 0, AuditOutcome::Success);
        wide.objects = vec![AuditObjectRef::new(
            AuditObjectRef::KIND_LANE,
            "lane-abcdefgh".repeat(8),
        )];
        wide.args
            .insert("note".to_string(), "你好".repeat(60).to_string());

        let rendered = audit_row(&wide, AUDIT_ROW_WIDTH);
        assert!(super::super::text::char_width(&rendered) <= AUDIT_ROW_WIDTH);
    }

    #[test]
    fn timestamps_render_as_utc_clock_time() {
        assert_eq!(format_audit_time(0), "00:00:00");
        assert_eq!(format_audit_time(86_399), "23:59:59");
        assert_eq!(
            format_audit_time(1_760_000_000),
            format_audit_time(1_760_000_000 % 86_400),
            "only the time of day is rendered, and it never depends on the local zone"
        );
    }
}
