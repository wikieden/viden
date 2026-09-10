//! Panel-local state for the read-only evidence inspector overlay
//! (`runtime.evidence_reads`, GUI-CORE-025).
//!
//! The TUI is the first client of the Core evidence read contract
//! (`crates/types/src/evidence_reads.rs`, `RuntimeCommand::QueryEvidence` ->
//! `RuntimeEventKind::EvidencePageLoaded`, and `ReadEvidenceContent` ->
//! `EvidenceContentLoaded`). It mirrors `super::audit_panel` deliberately —
//! both are reads, not decisions — and adds only what a second read needs.
//!
//! Four rules shape this module:
//!
//! - **The archive is not the recent window.** Rows are never derived from
//!   [`viden_core::RuntimeViewState::latest_evidence`]: that projection is an
//!   upsert-by-id list of whatever this client's stream happened to publish,
//!   with no ordering, no cursor and no content. Presenting it as the archive
//!   would be this client inventing the completeness Core never stated.
//! - **The cursor is opaque.** [`viden_core::EvidencePage::next_after`] is
//!   passed back verbatim as [`viden_core::EvidenceQuery::after`]. It is never
//!   parsed, constructed, compared, or reconstructed from a rendered row.
//! - **Two independent correlations.** A page read and a content read each own
//!   one in-flight slot, keyed on this client's own command id, because they
//!   answer different questions: paging must not be blocked by an expanded row,
//!   and an expanded row must not be settled by a page. Both events carry the
//!   command id as a *required* field, so unlike `AuditPageLoaded` there is no
//!   acceptance-gated fallback and an answer naming another reader is ignored.
//! - **Filters are Core's, not ours.** A kind filter re-queries from the first
//!   page instead of narrowing rows already held: Core applies filters before
//!   the page is cut, so `complete` and `next_after` describe the *filtered*
//!   archive, and a locally filtered page could not say whether a matching row
//!   sits on a page this client never loaded.

use std::collections::BTreeMap;

use viden_core::{
    EvidenceContent, EvidencePage, EvidenceQuery, EvidenceUnavailableReason, EvidenceView,
    RuntimeEvent, RuntimeEventKind, RuntimeOwner,
};

use super::{
    glyphs::Glyph,
    state::TuiState,
    text::{truncate_end, truncate_tail},
};

/// The frontend-contract-v1 capability that publishes both evidence reads.
///
/// This is the id Core publishes in its handshake
/// (`FRONTEND_V1_EXTENSION_CAPABILITIES`); the client must not invent a
/// finer-grained one, because an unpublished id can never become available.
pub(super) const EVIDENCE_READS_CAPABILITY: &str = "runtime.evidence_reads";

/// Page size for one `QueryEvidence`.
///
/// Core clamps to `1..=MAX_EVIDENCE_PAGE_SIZE`, so this is a readability choice
/// rather than a protocol bound: one page an operator can scroll, with the rest
/// reachable through the load-more row. It is Core's own documented default.
pub(super) const EVIDENCE_PAGE_LIMIT: u16 = viden_core::DEFAULT_EVIDENCE_PAGE_SIZE;

/// Display width one rendered evidence row is truncated to.
///
/// The overlay box is `frame.width.min(76)` and [`super::panel::bordered_row`]
/// spends four cells on borders and padding, leaving 72; the selection marker
/// takes two more, exactly as the audit timeline computes it.
pub(super) const EVIDENCE_ROW_WIDTH: usize = 70;

/// Largest number of content rows the detail pane renders at once.
///
/// The panel is a fixed box, so the content list is capped and the remainder is
/// *counted* in a stated row rather than dropped silently. Scrolling moves the
/// window; it never changes how much Core sent.
pub(super) const MAX_EVIDENCE_CONTENT_ROWS: usize = 12;

/// Seconds in one UTC day. The grouping key is a whole day, so an undated row
/// and a dated row can never share a group.
const SECONDS_PER_DAY: u64 = 86_400;

/// One content read awaiting its answer.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingContentRead {
    command_id: String,
    evidence_id: String,
}

/// TUI-local state of one open evidence inspector overlay.
///
/// Presentation only. Every field is either a command id this client issued or
/// bytes Core published back; closing the overlay drops the whole struct, so a
/// reopened inspector always re-queries rather than showing a page of unknown
/// age.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EvidencePanel {
    /// `None` reads the whole archive. A scope is an owner Core published —
    /// a gate's or review's own `owner`, or the focused Lane's id — never an
    /// owner this client composed field by field.
    scope: Option<RuntimeOwner>,
    /// Kinds Core has actually returned, in first-seen order. The filter cycle
    /// offers exactly these plus "all": a kind this client never saw is a kind
    /// it cannot honestly claim the archive has.
    kinds_seen: Vec<String>,
    /// `None` is the unfiltered read. `Some(kind)` sends Core a one-kind
    /// `kinds` filter, so `complete` still describes the filtered archive.
    filter: Option<String>,
    /// Oldest first, exactly as Core delivered. Later pages are newer, so they
    /// append at the end.
    entries: Vec<EvidenceView>,
    next_after: Option<String>,
    complete: bool,
    /// Command id of the in-flight `QueryEvidence`, or `None` when idle.
    awaiting_page: Option<String>,
    /// The in-flight `ReadEvidenceContent`, or `None` when idle. Separate from
    /// the page slot: expanding a row must not block paging.
    awaiting_content: Option<PendingContentRead>,
    /// Content Core already answered, keyed by evidence id and kept for this
    /// overlay's lifetime. A second Enter on the same row re-renders what Core
    /// already published instead of asking again.
    content: BTreeMap<String, EvidenceContent>,
    /// Core's rejection reason, verbatim. Never a locally composed sentence.
    error: Option<String>,
    /// Catalog key of the last local refusal (a second read while one is in
    /// flight). Local refusals send nothing, so they are never Core errors and
    /// must not be rendered as one.
    notice: Option<&'static str>,
    /// Whether at least one page has arrived. Absence and emptiness are
    /// different facts: "nothing loaded yet" must never render as "no evidence".
    loaded: bool,
    selected: usize,
    /// Evidence id whose detail pane is open, if any.
    detail: Option<String>,
    detail_scroll: usize,
    /// Core recorded evidence while this overlay was open, so what is on screen
    /// is a page of the archive as it was. Stated, never silently re-read: a
    /// background re-query would move the operator's rows under them.
    stale: bool,
}

impl EvidencePanel {
    pub(super) fn new(scope: Option<RuntimeOwner>) -> Self {
        Self {
            scope,
            kinds_seen: Vec::new(),
            filter: None,
            entries: Vec::new(),
            next_after: None,
            complete: false,
            awaiting_page: None,
            awaiting_content: None,
            content: BTreeMap::new(),
            error: None,
            notice: None,
            loaded: false,
            selected: 0,
            detail: None,
            detail_scroll: 0,
            stale: false,
        }
    }

    pub(super) fn scope(&self) -> Option<&RuntimeOwner> {
        self.scope.as_ref()
    }

    pub(super) fn filter(&self) -> Option<&str> {
        self.filter.as_deref()
    }

    /// The query for the next page: the first page when nothing has loaded,
    /// otherwise the page after the cursor Core handed back verbatim.
    pub(super) fn next_query(&self) -> EvidenceQuery {
        EvidenceQuery {
            owner: self.scope.clone(),
            // One kind at a time. Core's `kinds` is an OR list, but the overlay
            // offers a single cycling filter, so sending more would claim a
            // selection the operator never made.
            kinds: self.filter.iter().cloned().collect(),
            limit: EVIDENCE_PAGE_LIMIT,
            after: self.next_after.clone(),
        }
    }

    /// Registers the in-flight page query this panel is waiting for.
    pub(super) fn begin_page(&mut self, command_id: impl Into<String>) {
        self.awaiting_page = Some(command_id.into());
        self.error = None;
        self.notice = None;
    }

    /// Registers the in-flight content read this panel is waiting for.
    pub(super) fn begin_content(
        &mut self,
        command_id: impl Into<String>,
        evidence_id: impl Into<String>,
    ) {
        self.awaiting_content = Some(PendingContentRead {
            command_id: command_id.into(),
            evidence_id: evidence_id.into(),
        });
        self.error = None;
        self.notice = None;
    }

    /// Whether a load-more query may be dispatched right now.
    pub(super) fn can_load_more(&self) -> bool {
        self.shows_load_more_row() && self.awaiting_page.is_none()
    }

    /// Refuses a second read locally. Nothing is sent, so Core never sees the
    /// refused intent and the in-flight answer keeps its correlation.
    pub(super) fn refuse_second_read(&mut self) {
        self.notice = Some("evidence.busy");
    }

    /// Advances the kind filter and clears everything the old filter answered.
    ///
    /// Returns whether the caller must dispatch [`Self::next_query`]. The rows
    /// are dropped rather than filtered in place because Core cut the pages
    /// against the previous filter: keeping them would leave `complete` and
    /// `next_after` describing an archive the rows no longer belong to.
    pub(super) fn cycle_filter(&mut self) -> bool {
        let next = match self.filter.as_deref() {
            None => self.kinds_seen.first().cloned(),
            Some(current) => {
                let position = self.kinds_seen.iter().position(|kind| kind == current);
                match position {
                    Some(index) => self.kinds_seen.get(index + 1).cloned(),
                    // A filter whose kind is no longer in the seen list can only
                    // return to "all"; guessing a neighbour would silently
                    // answer a different question.
                    None => None,
                }
            }
        };
        if next == self.filter {
            return false;
        }
        self.filter = next;
        self.reset_for_requery();
        true
    }

    /// Restarts from the first page, keeping the scope, the filter, the kinds
    /// already seen, and the content Core already published.
    pub(super) fn refresh(&mut self) {
        self.reset_for_requery();
    }

    fn reset_for_requery(&mut self) {
        self.entries.clear();
        self.next_after = None;
        self.complete = false;
        self.loaded = false;
        self.selected = 0;
        self.detail = None;
        self.detail_scroll = 0;
        self.error = None;
        self.notice = None;
        self.stale = false;
    }

    /// Reconciles one ordered Core event against the in-flight reads.
    ///
    /// Returns whether this event changed the panel.
    pub(super) fn observe_event(&mut self, event: &RuntimeEvent) -> bool {
        match &event.kind {
            RuntimeEventKind::CommandRejected { command_id, reason }
                if self.awaiting_page.as_deref() == Some(command_id.as_str()) =>
            {
                self.awaiting_page = None;
                self.error = Some(reason.clone());
                true
            }
            RuntimeEventKind::CommandRejected { command_id, reason }
                if self
                    .awaiting_content
                    .as_ref()
                    .is_some_and(|pending| pending.command_id == *command_id) =>
            {
                self.awaiting_content = None;
                self.error = Some(reason.clone());
                true
            }
            // `command_id` is required on both answers (unlike `AuditPageLoaded`,
            // which carries a permanent `None` case), so correlation is exact
            // from the first byte and another reader's answer is ignored even
            // while a read of ours is in flight.
            RuntimeEventKind::EvidencePageLoaded { command_id, page }
                if self.awaiting_page.as_deref() == Some(command_id.as_str()) =>
            {
                self.apply_page(page);
                true
            }
            RuntimeEventKind::EvidenceContentLoaded {
                command_id,
                evidence_id,
                content,
            } if self.awaiting_content.as_ref().is_some_and(|pending| {
                pending.command_id == *command_id && pending.evidence_id == *evidence_id
            }) =>
            {
                self.awaiting_content = None;
                self.content.insert(evidence_id.clone(), content.clone());
                true
            }
            // A new evidence fact makes the loaded pages a snapshot of an
            // archive that has since grown. It is stated rather than acted on:
            // re-reading behind the operator would move the rows they are
            // looking at.
            RuntimeEventKind::EvidenceRecorded { .. } => {
                self.stale = true;
                true
            }
            _ => false,
        }
    }

    fn apply_page(&mut self, page: &EvidencePage) {
        for entry in &page.entries {
            if !self.kinds_seen.iter().any(|kind| kind == &entry.kind) {
                self.kinds_seen.push(entry.kind.clone());
            }
        }
        self.entries.extend(page.entries.iter().cloned());
        self.next_after = page.next_after.clone();
        self.complete = page.complete;
        self.awaiting_page = None;
        self.loaded = true;
        self.selected = self.selected.min(self.row_count().saturating_sub(1));
    }

    pub(super) fn entries(&self) -> &[EvidenceView] {
        &self.entries
    }

    #[cfg(test)]
    pub(super) fn kinds_seen(&self) -> &[String] {
        &self.kinds_seen
    }

    pub(super) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(super) fn notice(&self) -> Option<&'static str> {
        self.notice
    }

    pub(super) fn is_loading_page(&self) -> bool {
        self.awaiting_page.is_some()
    }

    pub(super) fn is_reading_content(&self) -> bool {
        self.awaiting_content.is_some()
    }

    /// Whether the in-flight content read is for *this* row.
    ///
    /// The detail pane asks per row rather than about the slot: with a read
    /// outstanding for another row, saying "reading this content" would name a
    /// read that was never sent for the row on screen.
    pub(super) fn is_reading_content_for(&self, evidence_id: &str) -> bool {
        self.awaiting_content
            .as_ref()
            .is_some_and(|pending| pending.evidence_id == evidence_id)
    }

    pub(super) fn is_complete(&self) -> bool {
        self.complete
    }

    pub(super) fn is_stale(&self) -> bool {
        self.stale
    }

    /// True only when Core answered with an empty page. A panel that has not
    /// loaded anything yet is loading or failed, never "empty".
    pub(super) fn is_empty_result(&self) -> bool {
        self.loaded && self.entries.is_empty()
    }

    /// Whether the load-more row is offered at all. A complete archive hides
    /// it: Core said there is nothing newer that matches.
    pub(super) fn shows_load_more_row(&self) -> bool {
        !self.complete && self.next_after.is_some()
    }

    pub(super) fn row_count(&self) -> usize {
        self.entries.len() + usize::from(self.shows_load_more_row())
    }

    pub(super) fn selected(&self) -> usize {
        self.selected
    }

    pub(super) fn selected_is_load_more(&self) -> bool {
        self.shows_load_more_row() && self.selected == self.entries.len()
    }

    pub(super) fn selected_entry(&self) -> Option<&EvidenceView> {
        self.entries.get(self.selected)
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

    /// Opens the detail pane on one evidence id and resets its scroll.
    pub(super) fn open_detail(&mut self, evidence_id: impl Into<String>) {
        self.detail = Some(evidence_id.into());
        self.detail_scroll = 0;
        self.notice = None;
    }

    /// Closes the detail pane. Returns whether one was open, so `Esc` can
    /// unwind the pane before it unwinds the overlay.
    pub(super) fn close_detail(&mut self) -> bool {
        self.detail_scroll = 0;
        self.detail.take().is_some()
    }

    pub(super) fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    pub(super) fn detail_entry(&self) -> Option<&EvidenceView> {
        let id = self.detail.as_deref()?;
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub(super) fn detail_scroll(&self) -> usize {
        self.detail_scroll
    }

    pub(super) fn scroll_detail(&mut self, delta: i8) {
        self.detail_scroll = if delta < 0 {
            self.detail_scroll.saturating_sub(1)
        } else {
            self.detail_scroll.saturating_add(1)
        };
    }

    /// Content Core already published for one row, if any.
    pub(super) fn content_for(&self, evidence_id: &str) -> Option<&EvidenceContent> {
        self.content.get(evidence_id)
    }

    /// Whether a content read for this row must be dispatched.
    ///
    /// Cached content is never re-read, and a second read is refused while one
    /// is in flight so two answers cannot race one slot.
    pub(super) fn should_read_content(&self, evidence_id: &str) -> bool {
        !self.content.contains_key(evidence_id) && self.awaiting_content.is_none()
    }
}

/// The UTC day one evidence row is grouped under.
///
/// `None` is the undated group, which Core orders *first*: an undated row is
/// the oldest thing Core can honestly say about it. It is a distinct group from
/// day zero, so the two never merge.
pub(super) fn evidence_day(entry: &EvidenceView) -> Option<u64> {
    entry.timestamp.map(|timestamp| timestamp / SECONDS_PER_DAY)
}

/// Renders a UTC day index as `YYYY-MM-DD`.
///
/// Derived arithmetically from the epoch day, like the audit timeline's clock:
/// the TUI has no date library, and UTC is chosen deliberately because evidence
/// is compared across machines and a locale-shifted date would make two readers
/// disagree about which day a fact belongs to.
pub(super) fn format_evidence_day(day: u64) -> String {
    // Howard Hinnant's civil-from-days, shifted to the 0000-03-01 era origin.
    let shifted = day as i64 + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day_of_month = day_of_year - (153 * month_position + 2) / 5 + 1;
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day_of_month:02}")
}

/// One evidence row: `{time} [{kind}] {summary} · {lane}`.
///
/// The summary is human prose and is cut at its END with the registered `…`
/// marker; ids and paths keep their distinctive tail instead, and are rendered
/// in the detail pane where their full value matters.
pub(super) fn evidence_row(state: &TuiState, entry: &EvidenceView, width: usize) -> String {
    let time = match entry.timestamp {
        Some(timestamp) => super::audit_panel::format_audit_time(timestamp),
        // Not a zero clock: Core recorded no time, and `00:00:00` would read as
        // midnight.
        None => super::i18n::text(state, "evidence.row.undated_time"),
    };
    let lane = entry
        .owner
        .as_ref()
        .and_then(|owner| owner.lane_id.as_deref())
        .map(|lane| truncate_tail(lane, 16))
        .unwrap_or_else(|| super::i18n::text(state, "evidence.row.no_lane"));
    let kind = truncate_end(&entry.kind, 16);
    let head = format!("{time} [{kind}] ");
    let tail = format!(" · {lane}");
    let summary_width = width
        .saturating_sub(super::text::char_width(&head) + super::text::char_width(&tail))
        .max(8);
    truncate_end(
        &format!(
            "{head}{}{tail}",
            truncate_end(&entry.summary, summary_width)
        ),
        width,
    )
}

/// The catalog key stating why one row has no servable canonical content.
///
/// `EvidenceUnavailableReason` is `#[non_exhaustive]`, so a reason this build
/// does not know gets its own honest sentence rather than borrowing the wording
/// of a reason it is not.
pub(super) const fn unavailable_reason_key(reason: EvidenceUnavailableReason) -> &'static str {
    match reason {
        EvidenceUnavailableReason::SummaryOnly => "evidence.unavailable.summary_only",
        EvidenceUnavailableReason::MissingCanonicalBytes => "evidence.unavailable.missing_bytes",
        EvidenceUnavailableReason::HashMismatch => "evidence.unavailable.hash_mismatch",
        EvidenceUnavailableReason::Binary => "evidence.unavailable.binary",
        _ => "evidence.unavailable.unknown",
    }
}

/// The evidence inspector overlay body: the list, or the detail pane when a
/// row is open.
pub(super) fn evidence_rows(state: &TuiState, width: usize) -> Vec<String> {
    let Some(panel) = state.ui.evidence.as_ref() else {
        return Vec::new();
    };
    if panel.detail().is_some() {
        return detail_rows(state, panel, width);
    }
    list_rows(state, panel, width)
}

fn list_rows(state: &TuiState, panel: &EvidencePanel, width: usize) -> Vec<String> {
    let mut rows = vec![scope_row(state, panel), filter_row(state, panel)];
    if !state.has_capability(EVIDENCE_READS_CAPABILITY) {
        rows.push(super::i18n::translate(
            state,
            "evidence.unavailable.capability",
            &[("capability", EVIDENCE_READS_CAPABILITY)],
        ));
    }
    if let Some(reason) = panel.error() {
        rows.push(super::i18n::translate(
            state,
            "evidence.error",
            &[("glyph", Glyph::Fail.unicode()), ("reason", reason)],
        ));
    }
    if panel.is_stale() {
        rows.push(super::i18n::translate(
            state,
            "evidence.stale",
            &[("glyph", Glyph::Warning.unicode())],
        ));
    }
    if panel.is_loading_page() && panel.entries().is_empty() {
        rows.push(super::i18n::translate(
            state,
            "evidence.loading",
            &[("glyph", Glyph::Wait.unicode())],
        ));
    } else if panel.is_empty_result() {
        rows.push(super::i18n::text(state, "evidence.empty"));
    }
    // Day headers are grouping, never hiding: every entry Core delivered gets a
    // row, and a header carries no selection index of its own.
    let mut previous_day: Option<Option<u64>> = None;
    for (index, entry) in panel.entries().iter().enumerate() {
        let day = evidence_day(entry);
        if previous_day != Some(day) {
            rows.push(match day {
                Some(day) => super::i18n::translate(
                    state,
                    "evidence.day",
                    &[("date", &format_evidence_day(day))],
                ),
                None => super::i18n::text(state, "evidence.undated"),
            });
            previous_day = Some(day);
        }
        let marker = if index == panel.selected() { ">" } else { " " };
        rows.push(format!("{marker} {}", evidence_row(state, entry, width)));
    }
    if panel.shows_load_more_row() {
        let marker = if panel.selected_is_load_more() {
            ">"
        } else {
            " "
        };
        rows.push(format!(
            "{marker} {}",
            super::i18n::text(state, "evidence.load_more")
        ));
    }
    if let Some(notice) = panel.notice() {
        rows.push(super::i18n::text(state, notice));
    }
    rows.push(super::i18n::translate(
        state,
        if panel.is_complete() {
            "evidence.footer.complete"
        } else {
            "evidence.footer.more"
        },
        &[("count", &panel.entries().len().to_string())],
    ));
    // One final width fit for every row, chrome included: a catalog string is
    // translated text of unknown length, so clamping only the entry rows would
    // still let a localized scope or footer overflow the panel. The two extra
    // columns are the selection marker the entry rows already carry.
    rows.into_iter()
        .map(|row| truncate_end(&row, width + 2))
        .collect()
}

fn scope_row(state: &TuiState, panel: &EvidencePanel) -> String {
    match panel.scope() {
        Some(owner) => super::i18n::translate(
            state,
            "evidence.scope.owner",
            &[("owner", &format_owner_scope(owner))],
        ),
        None => super::i18n::text(state, "evidence.scope.all"),
    }
}

fn filter_row(state: &TuiState, panel: &EvidencePanel) -> String {
    match panel.filter() {
        Some(kind) => super::i18n::translate(state, "evidence.filter.kind", &[("kind", kind)]),
        None => super::i18n::text(state, "evidence.filter.all"),
    }
}

/// A scope owner rendered as the fields Core actually set.
///
/// An unset field is a wildcard in Core's prefix match, so it is omitted rather
/// than printed as empty: showing `task=` would read as "a task with no id".
pub(super) fn format_owner_scope(owner: &RuntimeOwner) -> String {
    let mut parts = Vec::new();
    if !owner.workspace_id.is_empty() {
        parts.push(format!("workspace={}", owner.workspace_id));
    }
    if !owner.project_id.is_empty() {
        parts.push(format!("project={}", owner.project_id));
    }
    for (label, value) in [
        ("lane", owner.lane_id.as_deref()),
        ("session", owner.session_id.as_deref()),
        ("task", owner.task_id.as_deref()),
        ("turn", owner.turn_id.as_deref()),
    ] {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            parts.push(format!("{label}={value}"));
        }
    }
    if parts.is_empty() {
        // Every field unset is Core's "match anything", which is the whole
        // archive; saying so is more honest than rendering an empty scope.
        return String::new();
    }
    parts.join(" ")
}

fn detail_rows(state: &TuiState, panel: &EvidencePanel, width: usize) -> Vec<String> {
    let Some(entry) = panel.detail_entry() else {
        // The row left the list (a filter cycle, a refresh) while its pane was
        // open. Stating that is better than rendering a pane about nothing.
        return vec![super::i18n::text(state, "evidence.detail.missing")];
    };
    let mut rows = vec![
        super::i18n::translate(
            state,
            "evidence.detail.header",
            &[
                ("kind", &entry.kind),
                // The whole header row is width-fitted below, so the summary
                // gets every column the kind tag leaves it.
                (
                    "summary",
                    &truncate_end(&entry.summary, width.saturating_sub(8)),
                ),
            ],
        ),
        super::i18n::translate(
            state,
            "evidence.detail.identity",
            &[
                ("id", &truncate_tail(&entry.id, 40)),
                (
                    "time",
                    &match entry.timestamp {
                        Some(timestamp) => format!(
                            "{} {}",
                            format_evidence_day(timestamp / SECONDS_PER_DAY),
                            super::audit_panel::format_audit_time(timestamp)
                        ),
                        None => super::i18n::text(state, "evidence.row.undated_time"),
                    },
                ),
            ],
        ),
    ];
    rows.push(match entry.owner.as_ref() {
        Some(owner) => super::i18n::translate(
            state,
            "evidence.detail.owner",
            &[("owner", &truncate_end(&format_owner_scope(owner), width))],
        ),
        None => super::i18n::text(state, "evidence.detail.owner.unknown"),
    });
    if let Some(source) = entry.source.as_deref() {
        rows.push(super::i18n::translate(
            state,
            "evidence.detail.source",
            &[("source", &truncate_tail(source, width.saturating_sub(10)))],
        ));
    }
    if let Some(path) = entry.path.as_deref() {
        rows.push(super::i18n::translate(
            state,
            "evidence.detail.path",
            &[("path", &truncate_tail(path, width.saturating_sub(8)))],
        ));
    }
    if let Some(canonical) = entry.canonical.as_ref() {
        rows.push(super::i18n::translate(
            state,
            "evidence.detail.canonical",
            &[
                ("item", &truncate_tail(&canonical.item_id, 24)),
                ("bundle", &truncate_tail(&canonical.bundle_id, 24)),
            ],
        ));
        rows.push(super::i18n::translate(
            state,
            "evidence.detail.hash",
            &[("sha", short_sha(&canonical.source_hash))],
        ));
    }
    if let Some(keys) = metadata_keys(entry) {
        rows.push(super::i18n::translate(
            state,
            "evidence.detail.metadata",
            &[("keys", &truncate_end(&keys, width.saturating_sub(8)))],
        ));
    }
    rows.extend(content_rows(state, panel, entry, width));
    rows.into_iter()
        .map(|row| truncate_end(&row, width))
        .collect()
}

/// The metadata keys Core attached, as facts rather than as parsed values.
///
/// Only the keys are listed: a metadata value is free-form JSON a client must
/// not render as a typed fact, and naming the keys still tells an operator what
/// Core recorded beside this row.
fn metadata_keys(entry: &EvidenceView) -> Option<String> {
    let object = entry.metadata.as_ref()?.as_object()?;
    if object.is_empty() {
        return None;
    }
    Some(object.keys().cloned().collect::<Vec<_>>().join(" "))
}

fn short_sha(sha: &str) -> &str {
    sha.get(..8).unwrap_or(sha)
}

fn content_rows(
    state: &TuiState,
    panel: &EvidencePanel,
    entry: &EvidenceView,
    width: usize,
) -> Vec<String> {
    let Some(content) = panel.content_for(&entry.id) else {
        if panel.is_reading_content_for(&entry.id) {
            return vec![super::i18n::translate(
                state,
                "evidence.detail.reading",
                &[("glyph", Glyph::Wait.unicode())],
            )];
        }
        if panel.is_reading_content() {
            // Another row's read owns the single slot. Saying so is the honest
            // answer: this row was never asked for.
            return vec![super::i18n::text(state, "evidence.busy")];
        }
        // Neither content nor a read in flight: Core refused the read, and the
        // list's error row carries its own sentence verbatim.
        return vec![super::i18n::text(state, "evidence.detail.no_content")];
    };
    let (header, body) = match content {
        EvidenceContent::Text {
            text,
            truncated,
            sha256,
        } => {
            let mut header = vec![super::i18n::translate(
                state,
                "evidence.detail.text",
                &[("sha", short_sha(sha256))],
            )];
            if *truncated {
                header.push(super::i18n::text(state, "evidence.detail.text.truncated"));
            }
            (
                header,
                text.lines()
                    .map(|line| format!("  {}", truncate_end(line, width.saturating_sub(2))))
                    .collect::<Vec<_>>(),
            )
        }
        EvidenceContent::Diff { document, sha256 } => (
            vec![super::i18n::translate(
                state,
                "evidence.detail.diff",
                &[("sha", short_sha(sha256))],
            )],
            // The same hunk renderer the approval overlay uses, so one evidence
            // patch and one workspace diff render through identical rows.
            super::diff_rows::diff_document_rows(state, document, width),
        ),
        EvidenceContent::Unavailable { reason } => (
            vec![super::i18n::translate(
                state,
                "evidence.unavailable",
                &[
                    ("glyph", Glyph::Warning.unicode()),
                    (
                        "reason",
                        &super::i18n::text(state, unavailable_reason_key(*reason)),
                    ),
                ],
            )],
            Vec::new(),
        ),
        // `EvidenceContent` is `#[non_exhaustive]`: a shape this build cannot
        // render is named rather than rendered as empty content.
        _ => (
            vec![super::i18n::text(state, "evidence.detail.unknown_shape")],
            Vec::new(),
        ),
    };
    let mut rows = header;
    let total = body.len();
    let start = panel.detail_scroll().min(total.saturating_sub(1));
    let window = body
        .into_iter()
        .skip(start)
        .take(MAX_EVIDENCE_CONTENT_ROWS)
        .collect::<Vec<_>>();
    let shown = window.len();
    rows.extend(window);
    let hidden = total.saturating_sub(start + shown);
    if hidden > 0 {
        rows.push(super::i18n::translate(
            state,
            "evidence.detail.more",
            &[("count", &hidden.to_string())],
        ));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use viden_core::{
        CapabilityId, DiffDocument, DiffFile, DiffHunk, DiffLine, DiffLineKind,
        RuntimeEventEnvelope, RuntimeWireEvent, WorkspaceChangeKind,
    };

    fn entry(id: &str, kind: &str, timestamp: Option<u64>) -> EvidenceView {
        EvidenceView {
            id: id.to_string(),
            kind: kind.to_string(),
            summary: format!("{kind} summary for {id}"),
            path: None,
            source: Some("lane-1".to_string()),
            canonical: None,
            metadata: None,
            timestamp,
            owner: Some(RuntimeOwner {
                lane_id: Some("lane-1".to_string()),
                ..RuntimeOwner::default()
            }),
        }
    }

    fn page(entries: Vec<EvidenceView>, next_after: Option<&str>) -> EvidencePage {
        EvidencePage {
            complete: next_after.is_none(),
            entries,
            next_after: next_after.map(str::to_string),
        }
    }

    fn event(sequence: u64, kind: RuntimeEventKind) -> RuntimeEvent {
        RuntimeEvent {
            sequence,
            timestamp: Some(sequence),
            kind,
        }
    }

    fn state_with_panel(panel: EvidencePanel) -> TuiState {
        let mut state = TuiState {
            capabilities: viden_core::frontend_capabilities(),
            ..TuiState::default()
        };
        state.ui.evidence = Some(panel);
        state
    }

    #[test]
    fn the_first_query_is_scoped_and_pages_from_cores_own_cursor_verbatim() {
        let panel = EvidencePanel::new(None);
        assert_eq!(
            panel.next_query(),
            EvidenceQuery {
                owner: None,
                kinds: Vec::new(),
                limit: EVIDENCE_PAGE_LIMIT,
                after: None,
            }
        );

        let scope = RuntimeOwner {
            lane_id: Some("lane-1".to_string()),
            ..RuntimeOwner::default()
        };
        let mut panel = EvidencePanel::new(Some(scope.clone()));
        assert_eq!(panel.next_query().owner, Some(scope.clone()));

        panel.begin_page("tui-1");
        panel.observe_event(&event(
            1,
            RuntimeEventKind::EvidencePageLoaded {
                command_id: "tui-1".to_string(),
                page: page(vec![entry("e-1", "patch", Some(10))], Some("t:10:e-1")),
            },
        ));

        assert_eq!(panel.next_query().after.as_deref(), Some("t:10:e-1"));
        assert_eq!(
            panel.next_query().owner,
            Some(scope),
            "paging must not widen the scope the operator opened"
        );
    }

    #[test]
    fn a_page_answering_another_reader_is_ignored() {
        let mut panel = EvidencePanel::new(None);
        panel.begin_page("tui-1");

        assert!(!panel.observe_event(&event(
            1,
            RuntimeEventKind::EvidencePageLoaded {
                command_id: "someone-else".to_string(),
                page: page(vec![entry("e-9", "patch", Some(10))], None),
            },
        )));
        assert!(panel.entries().is_empty());
        assert!(!panel.is_empty_result(), "nothing loaded is not empty");
        assert!(panel.is_loading_page());
    }

    #[test]
    fn pages_tile_oldest_first_and_the_footer_states_completeness() {
        let mut panel = EvidencePanel::new(None);
        panel.begin_page("tui-1");
        panel.observe_event(&event(
            1,
            RuntimeEventKind::EvidencePageLoaded {
                command_id: "tui-1".to_string(),
                page: page(
                    vec![entry("e-1", "patch", None), entry("e-2", "patch", Some(10))],
                    Some("t:10:e-2"),
                ),
            },
        ));
        assert!(panel.shows_load_more_row());
        assert!(!panel.is_complete());

        panel.begin_page("tui-2");
        panel.observe_event(&event(
            2,
            RuntimeEventKind::EvidencePageLoaded {
                command_id: "tui-2".to_string(),
                page: page(vec![entry("e-3", "test_result", Some(90_000))], None),
            },
        ));

        assert_eq!(
            panel
                .entries()
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["e-1", "e-2", "e-3"]
        );
        assert!(panel.is_complete());
        assert!(!panel.shows_load_more_row());
        assert_eq!(panel.kinds_seen(), ["patch", "test_result"]);
    }

    #[test]
    fn the_undated_group_comes_first_and_dated_rows_group_by_utc_day() {
        let mut panel = EvidencePanel::new(None);
        panel.begin_page("tui-1");
        panel.observe_event(&event(
            1,
            RuntimeEventKind::EvidencePageLoaded {
                command_id: "tui-1".to_string(),
                page: page(
                    vec![
                        entry("e-undated", "patch", None),
                        entry("e-day-1", "patch", Some(1_700_000_100)),
                        entry("e-day-2", "patch", Some(1_700_100_000)),
                    ],
                    None,
                ),
            },
        ));
        let state = state_with_panel(panel);

        let rows = evidence_rows(&state, EVIDENCE_ROW_WIDTH).join("\n");
        let undated = rows.find("UNDATED").expect("undated group header");
        let first_day = rows.find("2023-11-14").expect("first dated group header");
        let second_day = rows.find("2023-11-16").expect("second dated group header");

        assert!(undated < first_day, "undated group must come first: {rows}");
        assert!(first_day < second_day, "days ascend: {rows}");
        assert!(
            rows.contains("--:--:--"),
            "an undated row states no clock: {rows}"
        );
        assert!(rows.contains("archive complete"), "{rows}");
    }

    #[test]
    fn a_kind_filter_re_queries_from_the_first_page_and_never_hides_rows_locally() {
        let mut panel = EvidencePanel::new(None);
        panel.begin_page("tui-1");
        panel.observe_event(&event(
            1,
            RuntimeEventKind::EvidencePageLoaded {
                command_id: "tui-1".to_string(),
                page: page(
                    vec![
                        entry("e-1", "patch", Some(10)),
                        entry("e-2", "test_result", Some(20)),
                    ],
                    Some("t:20:e-2"),
                ),
            },
        ));

        assert!(panel.cycle_filter());
        assert_eq!(panel.filter(), Some("patch"));
        assert_eq!(panel.next_query().kinds, vec!["patch".to_string()]);
        assert_eq!(
            panel.next_query().after,
            None,
            "a filtered read starts at the first page: Core cuts pages after filtering"
        );
        assert!(panel.entries().is_empty());
        assert!(
            !panel.is_empty_result(),
            "a re-query is not an empty answer"
        );

        assert!(panel.cycle_filter());
        assert_eq!(panel.filter(), Some("test_result"));
        assert!(panel.cycle_filter());
        assert_eq!(panel.filter(), None, "the cycle returns to every kind");
        assert!(panel.next_query().kinds.is_empty());
    }

    #[test]
    fn content_is_cached_per_row_and_one_read_is_in_flight_at_a_time() {
        let mut panel = EvidencePanel::new(None);
        panel.begin_page("tui-1");
        panel.observe_event(&event(
            1,
            RuntimeEventKind::EvidencePageLoaded {
                command_id: "tui-1".to_string(),
                page: page(vec![entry("e-1", "test_result", Some(10))], None),
            },
        ));

        assert!(panel.should_read_content("e-1"));
        panel.begin_content("tui-2", "e-1");
        assert!(
            !panel.should_read_content("e-2"),
            "one content read is in flight at a time"
        );
        panel.observe_event(&event(
            2,
            RuntimeEventKind::EvidenceContentLoaded {
                command_id: "tui-2".to_string(),
                evidence_id: "e-1".to_string(),
                content: EvidenceContent::Text {
                    text: "test result: ok".to_string(),
                    truncated: false,
                    sha256: "bbbbbbbbbbbb".to_string(),
                },
            },
        ));

        assert!(
            !panel.should_read_content("e-1"),
            "cached content is never re-read"
        );
        assert!(panel.content_for("e-1").is_some());
        assert!(!panel.is_reading_content());
    }

    /// The detail pane answers about the row on screen, not about the slot: a
    /// read outstanding for a *different* row must not render as "reading this
    /// content", because this row was never asked for.
    #[test]
    fn a_detail_pane_never_claims_another_rows_read_as_its_own() {
        let mut panel = EvidencePanel::new(None);
        panel.begin_page("tui-1");
        panel.observe_event(&event(
            1,
            RuntimeEventKind::EvidencePageLoaded {
                command_id: "tui-1".to_string(),
                page: page(
                    vec![
                        entry("e-1", "patch", Some(10)),
                        entry("e-2", "test_result", Some(20)),
                    ],
                    None,
                ),
            },
        ));
        panel.begin_content("tui-2", "e-1");
        assert!(panel.is_reading_content_for("e-1"));
        assert!(!panel.is_reading_content_for("e-2"));

        let mut state = state_with_panel(panel);
        state
            .ui
            .evidence
            .as_mut()
            .expect("panel")
            .open_detail("e-2".to_string());

        let rows = evidence_rows(&state, 72).join("\n");
        assert!(
            rows.contains("already in flight"),
            "the busy slot is stated rather than borrowed: {rows}"
        );
        assert!(!rows.contains("Reading the canonical content"), "{rows}");
    }

    #[test]
    fn a_refused_read_states_cores_reason_verbatim_and_settles_the_slot() {
        let mut panel = EvidencePanel::new(None);
        panel.begin_page("tui-1");
        panel.observe_event(&event(
            1,
            RuntimeEventKind::CommandRejected {
                command_id: "tui-1".to_string(),
                reason: "evidence query kinds exceed the 32 entry bound: 33 requested".to_string(),
            },
        ));

        assert_eq!(
            panel.error(),
            Some("evidence query kinds exceed the 32 entry bound: 33 requested")
        );
        assert!(!panel.is_loading_page());
        assert!(
            !panel.is_empty_result(),
            "a refusal is not an empty archive"
        );

        panel.begin_content("tui-2", "e-1");
        panel.observe_event(&event(
            2,
            RuntimeEventKind::CommandRejected {
                command_id: "tui-2".to_string(),
                reason: "evidence `e-1` is not a row Core recorded".to_string(),
            },
        ));
        assert_eq!(
            panel.error(),
            Some("evidence `e-1` is not a row Core recorded")
        );
        assert!(!panel.is_reading_content());
    }

    #[test]
    fn recorded_evidence_marks_the_page_stale_and_a_refresh_restarts_it() {
        let mut panel = EvidencePanel::new(None);
        panel.begin_page("tui-1");
        panel.observe_event(&event(
            1,
            RuntimeEventKind::EvidencePageLoaded {
                command_id: "tui-1".to_string(),
                page: page(vec![entry("e-1", "patch", Some(10))], None),
            },
        ));
        assert!(!panel.is_stale());

        panel.observe_event(&event(
            2,
            RuntimeEventKind::EvidenceRecorded {
                evidence: entry("e-2", "patch", Some(20)),
            },
        ));
        assert!(panel.is_stale());
        assert_eq!(
            panel.entries().len(),
            1,
            "the recent-window fact is never folded into the archive rows"
        );

        panel.refresh();
        assert!(!panel.is_stale());
        assert!(panel.entries().is_empty());
        assert_eq!(panel.next_query().after, None);
    }

    #[test]
    fn the_detail_pane_renders_text_diff_and_every_unavailable_reason() {
        let mut panel = EvidencePanel::new(None);
        panel.begin_page("tui-1");
        panel.observe_event(&event(
            1,
            RuntimeEventKind::EvidencePageLoaded {
                command_id: "tui-1".to_string(),
                page: page(
                    vec![
                        entry("e-text", "test_result", Some(1_700_000_100)),
                        entry("e-diff", "patch", Some(1_700_000_200)),
                        entry("e-summary", "task_summary", Some(1_700_000_300)),
                    ],
                    None,
                ),
            },
        ));
        panel.begin_content("tui-2", "e-text");
        panel.observe_event(&event(
            2,
            RuntimeEventKind::EvidenceContentLoaded {
                command_id: "tui-2".to_string(),
                evidence_id: "e-text".to_string(),
                content: EvidenceContent::Text {
                    text: "running 3 tests\ntest result: ok".to_string(),
                    truncated: true,
                    sha256: "bbbbbbbbbbbbbbbb".to_string(),
                },
            },
        ));
        panel.begin_content("tui-3", "e-diff");
        panel.observe_event(&event(
            3,
            RuntimeEventKind::EvidenceContentLoaded {
                command_id: "tui-3".to_string(),
                evidence_id: "e-diff".to_string(),
                content: EvidenceContent::Diff {
                    document: DiffDocument {
                        files: vec![DiffFile {
                            path: "crates/types/src/evidence_reads.rs".to_string(),
                            old_path: None,
                            kind: WorkspaceChangeKind::Modified,
                            binary: false,
                            omitted: false,
                            additions: 1,
                            deletions: 1,
                            hunks: vec![DiffHunk {
                                old_start: 12,
                                old_lines: 3,
                                new_start: 12,
                                new_lines: 3,
                                header: Some("impl EvidenceQuery".to_string()),
                                lines: vec![DiffLine {
                                    kind: DiffLineKind::Added,
                                    content: "        self.limit.clamp(1, 200) as usize"
                                        .to_string(),
                                    old_line: None,
                                    new_line: Some(13),
                                }],
                            }],
                        }],
                        truncated: false,
                        byte_limit: 262_144,
                    },
                    sha256: "aaaaaaaaaaaaaaaa".to_string(),
                },
            },
        ));
        panel.begin_content("tui-4", "e-summary");
        panel.observe_event(&event(
            4,
            RuntimeEventKind::EvidenceContentLoaded {
                command_id: "tui-4".to_string(),
                evidence_id: "e-summary".to_string(),
                content: EvidenceContent::Unavailable {
                    reason: EvidenceUnavailableReason::SummaryOnly,
                },
            },
        ));

        let mut state = state_with_panel(panel);
        let open = |state: &mut TuiState, id: &str| {
            state
                .ui
                .evidence
                .as_mut()
                .expect("panel")
                .open_detail(id.to_string());
            evidence_rows(state, 72).join("\n")
        };

        let text = open(&mut state, "e-text");
        assert!(text.contains("test result: ok"), "{text}");
        assert!(
            text.contains("bbbbbbbb"),
            "the verified hash is shown: {text}"
        );
        assert!(text.contains("truncated"), "{text}");

        let diff = open(&mut state, "e-diff");
        assert!(diff.contains("@@ -12,3 +12,3 @@"), "{diff}");
        assert!(diff.contains("clamp(1, 200)"), "{diff}");

        let summary = open(&mut state, "e-summary");
        assert!(summary.contains("display-only"), "{summary}");

        // Every typed reason has its own sentence, and the hash mismatch says
        // exactly why the bytes are not shown.
        for (reason, needle) in [
            (
                EvidenceUnavailableReason::MissingCanonicalBytes,
                "no longer",
            ),
            (
                EvidenceUnavailableReason::HashMismatch,
                "failed verification",
            ),
            (EvidenceUnavailableReason::Binary, "not UTF-8"),
        ] {
            let key = unavailable_reason_key(reason);
            let sentence = super::super::i18n::text(&state, key);
            assert!(sentence.contains(needle), "{key}: {sentence}");
        }
    }

    #[test]
    fn evidence_reads_fixture_replays_into_rows_pages_content_and_a_refusal() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../crates/types/tests/fixtures/frontend-contract-v1/evidence-reads.json"
        ))
        .expect("evidence reads fixture");
        let events = fixture["events"]
            .as_array()
            .expect("fixture events")
            .iter()
            .filter_map(|envelope| {
                let envelope: RuntimeEventEnvelope =
                    serde_json::from_value(envelope.clone()).expect("fixture envelope");
                match envelope.event {
                    RuntimeWireEvent::Known(event) => Some(event),
                    RuntimeWireEvent::Unknown { .. } => None,
                }
            })
            .collect::<Vec<_>>();
        let replay = |panel: &mut EvidencePanel| {
            for event in &events {
                panel.observe_event(event);
            }
        };

        // Two pages tile into one oldest-first list.
        let mut panel = EvidencePanel::new(None);
        panel.begin_page("evidence_read_first");
        replay(&mut panel);
        panel.begin_page("evidence_read_second");
        replay(&mut panel);
        assert_eq!(
            panel
                .entries()
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "evidence_alpha_patch",
                "evidence_bravo_tests",
                "evidence_charlie_summary"
            ]
        );
        assert!(panel.is_complete());
        assert_eq!(panel.kinds_seen(), ["patch", "test_result", "task_summary"]);

        // The kind filter is Core's answer, not a local narrowing.
        assert!(panel.cycle_filter());
        assert_eq!(panel.next_query().kinds, vec!["patch".to_string()]);
        panel.begin_page("evidence_read_patches");
        replay(&mut panel);
        assert_eq!(
            panel
                .entries()
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["evidence_alpha_patch"]
        );

        // Text, Diff, and the summary-only refusal all arrive correlated.
        for (command_id, evidence_id) in [
            ("evidence_content_tests", "evidence_bravo_tests"),
            ("evidence_content_patch", "evidence_alpha_patch"),
            ("evidence_content_summary", "evidence_charlie_summary"),
        ] {
            panel.begin_content(command_id, evidence_id);
            replay(&mut panel);
        }
        assert!(matches!(
            panel.content_for("evidence_bravo_tests"),
            Some(EvidenceContent::Text { .. })
        ));
        assert!(matches!(
            panel.content_for("evidence_alpha_patch"),
            Some(EvidenceContent::Diff { .. })
        ));
        assert!(matches!(
            panel.content_for("evidence_charlie_summary"),
            Some(EvidenceContent::Unavailable {
                reason: EvidenceUnavailableReason::SummaryOnly
            })
        ));

        // The over-limit query is a stated refusal, never an empty page.
        panel.begin_page("evidence_read_overlimit");
        replay(&mut panel);
        assert_eq!(
            panel.error(),
            Some(
                "evidence query kinds exceed the 32 entry bound: 33 requested\nhint: ask for \
                 fewer kinds, or drop the filter and page the archive"
            )
        );
    }

    #[test]
    fn a_missing_capability_is_named_on_the_overlay_itself() {
        let mut panel = EvidencePanel::new(None);
        panel.begin_page("tui-1");
        panel.observe_event(&event(
            1,
            RuntimeEventKind::EvidencePageLoaded {
                command_id: "tui-1".to_string(),
                page: page(Vec::new(), None),
            },
        ));
        let mut state = state_with_panel(panel);
        state
            .capabilities
            .remove(&CapabilityId(EVIDENCE_READS_CAPABILITY.to_string()));

        let rows = evidence_rows(&state, EVIDENCE_ROW_WIDTH).join("\n");

        assert!(rows.contains(EVIDENCE_READS_CAPABILITY), "{rows}");
        assert!(
            rows.contains("No evidence in this scope"),
            "an empty answer is still stated as empty: {rows}"
        );
    }

    #[test]
    fn every_row_fits_the_requested_width() {
        let mut panel = EvidencePanel::new(None);
        panel.begin_page("tui-1");
        panel.observe_event(&event(
            1,
            RuntimeEventKind::EvidencePageLoaded {
                command_id: "tui-1".to_string(),
                page: page(
                    vec![EvidenceView {
                        summary: "a very long evidence summary that keeps going well past any \
                                  terminal column budget"
                            .to_string(),
                        ..entry(
                            "evidence-with-a-very-long-identifier-suffix",
                            "patch",
                            Some(10),
                        )
                    }],
                    None,
                ),
            },
        ));
        let state = state_with_panel(panel);

        for width in [24usize, 40, 70] {
            for row in evidence_rows(&state, width) {
                assert!(
                    super::super::text::char_width(&row) <= width + 2,
                    "width {width} overflowed: {row}"
                );
            }
        }
    }

    #[test]
    fn the_civil_calendar_matches_known_utc_days() {
        assert_eq!(format_evidence_day(0), "1970-01-01");
        assert_eq!(
            format_evidence_day(1_700_000_100 / SECONDS_PER_DAY),
            "2023-11-14"
        );
        assert_eq!(format_evidence_day(19_723), "2024-01-01");
    }
}
