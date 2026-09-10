import { renderDiffFiles } from "../components/diff_rows";
import { translate, type Locale, type MessageKey } from "../i18n/catalog";
import {
  ABSENT_EVIDENCE_CONTENT,
  EVIDENCE_READS_CAPABILITY,
  evidenceKindChips,
  evidenceRowMatches,
  groupEvidenceByDay,
  localTimeLabel,
  type EvidenceArchiveProjection,
  type EvidenceContentProjection,
  type EvidenceRowProjection,
} from "../models/evidence";
import "./evidence_view.css";

/**
 * EvidenceView — the in-cockpit archive of what the Lanes actually produced.
 *
 * The registered `D-RAILNAV ②` family: `.evwrap > .evmain(.evbar + .evscroll)
 * + .evdet`. Like DiffReview it is a *view* inside D1's centre pane rather
 * than a standalone D-screen, because that is what the design registers and
 * what `D-RAILNAV` decided: the activity rail routes to the D-screens, and the
 * secondary surfaces open over the transcript one Escape from it.
 *
 * Read side only, and every fact on screen came off one of two Core answers.
 * The client never orders the archive, never decides where a page ends, never
 * opens the ContextStore, and never infers a row's content from its summary.
 *
 * Three rules are worth stating because a plausible client breaks each:
 *
 * 1. **`latest_evidence` is not this list.** The cockpit's recent-evidence
 *    window is a different projection with no ordering rule and no content.
 *    Nothing here merges the two.
 * 2. **The cursor is Core's.** "Load older" hands `next_after` back verbatim.
 *    Nothing parses, constructs, or compares one.
 * 3. **New evidence marks the list stale; it never reloads it.** A paged list
 *    that re-read itself under the operator would move rows out from under
 *    someone mid-read. The banner and Refresh are the whole mechanism.
 */

export type {
  EvidenceArchiveProjection,
  EvidenceContentProjection,
} from "../models/evidence";

export interface EvidenceViewHandlers {
  /** Re-reads the first page from Core. Absent while no host is bound. */
  onRefresh?: () => void;
  /** Sends one more `QueryEvidence` through Core's own cursor. */
  onLoadOlder?: () => void;
  /** Returns the centre pane to the transcript. */
  onClose?: () => void;
  /** Sends one `ReadEvidenceContent`. Absent leaves the detail rail read-only. */
  onSelect?: (evidenceId: string) => void;
  /** Remembered across an ordered Core refresh, like DiffReview's file path. */
  selectedId?: string | null;
  /** The chip filter, sent to Core as `EvidenceQuery.kinds`. */
  kinds?: string[];
  onKindsChange?: (kinds: string[]) => void;
  /** Client-side substring over the loaded rows. Never a Core query. */
  search?: string;
  onSearchChange?: (query: string) => void;
  /** Core's answer for the selected row. */
  content?: EvidenceContentProjection;
  /** Opens DiffReview in the same centre pane (G1a's `centerView` switch). */
  onOpenReview?: () => void;
  /** Whether Core published `runtime.structured_diff` for that view. */
  reviewAvailable?: boolean;
  /** Opens D14 scoped to one audit object. */
  onOpenAuditTrail?: (scope: { kind: string; id: string }) => void;
}

/**
 * The audit object kind Core records evidence under.
 *
 * `AuditObjectRef::KIND_EVIDENCE`, the same string D12's conflict pane routes
 * baseline bindings with. The one-way link `D-AUDIT` describes runs the other
 * way — an audit row links evidence — so this opens the trail *scoped to* the
 * evidence object rather than claiming the row has an audit id of its own.
 */
const AUDIT_KIND_EVIDENCE = "evidence";

/// Registered glyphs for the first-class kinds, plus a neutral marker for
/// anything else. A kind this build has no glyph for gets `·` rather than
/// borrowing another kind's symbol, which would mislabel it.
const KIND_GLYPH: Record<string, string> = {
  patch: "±",
  test_result: "✓",
  review: "⌾",
  doc_update: "¶",
  release_artifact: "⬒",
  task_summary: "≡",
};

const KIND_LABEL: Record<string, MessageKey> = {
  patch: "d1.evidence.kind.patch",
  test_result: "d1.evidence.kind.testResult",
  review: "d1.evidence.kind.review",
  doc_update: "d1.evidence.kind.docUpdate",
  release_artifact: "d1.evidence.kind.releaseArtifact",
};

/// Core's four typed unavailable reasons plus the unmodeled case. Each has its
/// own sentence: the affordance differs per reason, and `hash_mismatch` in
/// particular is the opposite fact from `missing_canonical_bytes`.
const REASON_COPY: Record<string, MessageKey> = {
  summary_only: "d1.evidence.reason.summaryOnly",
  missing_canonical_bytes: "d1.evidence.reason.missingCanonicalBytes",
  hash_mismatch: "d1.evidence.reason.hashMismatch",
  binary: "d1.evidence.reason.binary",
};

/// One localized noun for a Core kind, or the raw kind when this build has no
/// name for it — never a blank, and never a neighbouring kind's name.
function kindLabel(locale: Locale, kind: string): string {
  const key = KIND_LABEL[kind];
  return key ? translate(locale, key, {}) : kind;
}

function stateNote(
  host: HTMLElement,
  state: string,
  text: string,
  alert = false,
): HTMLElement {
  const note = document.createElement("p");
  note.className = "evidence-state";
  note.dataset.evidenceState = state;
  if (alert) note.setAttribute("role", "alert");
  note.textContent = text;
  host.append(note);
  return note;
}

function section(host: HTMLElement, title: string, id: string): HTMLElement {
  const block = document.createElement("div");
  block.className = "evsec";
  block.dataset.evidenceSection = id;
  const heading = document.createElement("h5");
  heading.textContent = title;
  block.append(heading);
  host.append(block);
  return block;
}

function keyValue(host: HTMLElement, key: string, value: string): void {
  const row = document.createElement("div");
  row.className = "evkv";
  row.dataset.evidenceFact = key;
  const label = document.createElement("span");
  label.className = "k";
  label.textContent = key;
  const text = document.createElement("span");
  text.className = "v";
  text.textContent = value;
  // The value is often a hash or a long path; the row ellipsizes, so the full
  // string stays reachable rather than being silently cut.
  text.title = value;
  row.append(label, text);
  host.append(row);
}

/**
 * Renders the view into `host`, replacing whatever was there.
 *
 * Selection, the chip filter, and the search text are presentation state and
 * live with the caller: an ordered Core refresh rebuilds this DOM, so each is
 * passed in and reported back rather than read out of the tree afterwards.
 */
export function renderEvidenceView(
  host: HTMLElement,
  projection: EvidenceArchiveProjection,
  locale: Locale,
  handlers: EvidenceViewHandlers = {},
): void {
  const kinds = handlers.kinds ?? [];
  const search = handlers.search ?? "";
  const content = handlers.content ?? ABSENT_EVIDENCE_CONTENT;

  // Core already filtered by kind before it cut the page, so the only local
  // narrowing is the search box — and it is a search of the *loaded rows*.
  const visible = projection.rows.filter((row) => evidenceRowMatches(row, search));
  const requested = handlers.selectedId ?? null;
  const selected =
    visible.find((row) => row.id === requested) ?? visible[0] ?? null;

  const wrap = document.createElement("section");
  wrap.className = "evwrap";
  wrap.dataset.evidenceView = "true";
  wrap.setAttribute("role", "region");
  wrap.setAttribute("aria-label", translate(locale, "d1.evidence.region", {}));

  /* ---- list ---- */

  const main = document.createElement("div");
  main.className = "evmain";

  const bar = document.createElement("div");
  bar.className = "evbar";

  const chips = ["", ...evidenceKindChips(projection.rows)];
  for (const kind of chips) {
    const chip = document.createElement("button");
    chip.type = "button";
    const active = kind === "" ? kinds.length === 0 : kinds.includes(kind);
    chip.className = active ? "fchip on" : "fchip";
    chip.dataset.evidenceKindChip = kind === "" ? "all" : kind;
    chip.setAttribute("aria-pressed", String(active));
    chip.textContent =
      kind === "" ? translate(locale, "d1.evidence.filter.all", {}) : kindLabel(locale, kind);
    // A filter change is a *new Core query*, not a local narrowing: Core
    // applies `kinds` before it cuts the page, so a client filtering what it
    // holds could not tell whether a match sits on a page it never loaded.
    chip.disabled = !handlers.onKindsChange;
    chip.addEventListener("click", () => {
      if (kind === "") {
        handlers.onKindsChange?.([]);
        return;
      }
      const next = kinds.includes(kind)
        ? kinds.filter((entry) => entry !== kind)
        : [...kinds, kind];
      handlers.onKindsChange?.(next);
    });
    bar.append(chip);
  }

  // Search and Refresh wrap as one unit. The chip bar grows with every kind
  // Core returns, so it must wrap rather than hide a kind; keeping the two
  // controls in one group stops a wrap from stranding Refresh on a line by
  // itself, in whichever locale runs the chip labels wide.
  const find = document.createElement("div");
  find.className = "evfind";

  const searchBox = document.createElement("input");
  searchBox.type = "search";
  searchBox.className = "search";
  searchBox.dataset.evidenceSearch = "true";
  searchBox.value = search;
  searchBox.placeholder = translate(locale, "d1.evidence.search", {});
  // Said out loud rather than implied: Core exposes no evidence search, so
  // this box narrows the rows already on screen and nothing else. An operator
  // who read it as a search of the archive would take an empty result for
  // "there is none".
  const searchScope = translate(locale, "d1.evidence.searchScope", {
    rows: String(projection.rows.length),
  });
  searchBox.title = searchScope;
  searchBox.setAttribute("aria-label", searchScope);
  searchBox.disabled = !handlers.onSearchChange;
  searchBox.addEventListener("input", () => {
    handlers.onSearchChange?.(searchBox.value);
  });
  find.append(searchBox);

  const refresh = document.createElement("button");
  refresh.type = "button";
  refresh.className = "evidence-icon";
  refresh.dataset.evidenceRefresh = "true";
  refresh.textContent = "⟳";
  const refreshLabel = translate(locale, "d1.evidence.refresh", {});
  refresh.title = refreshLabel;
  refresh.setAttribute("aria-label", refreshLabel);
  refresh.disabled = !handlers.onRefresh;
  refresh.addEventListener("click", () => handlers.onRefresh?.());
  find.append(refresh);
  bar.append(find);
  main.append(bar);

  const scroll = document.createElement("div");
  scroll.className = "evscroll";

  if (projection.stale) {
    // The rows stay: a stale marker must never blank the only evidence the
    // operator has in front of them, and the re-read is theirs to trigger.
    const banner = document.createElement("p");
    banner.className = "evidence-banner";
    banner.dataset.evidenceStale = "true";
    banner.setAttribute("role", "status");
    banner.textContent = translate(locale, "d1.evidence.stale", {});
    const reload = document.createElement("button");
    reload.type = "button";
    reload.className = "evidence-banner-action";
    reload.dataset.evidenceStaleRefresh = "true";
    reload.textContent = translate(locale, "d1.evidence.staleRefresh", {});
    reload.disabled = !handlers.onRefresh;
    reload.addEventListener("click", () => handlers.onRefresh?.());
    banner.append(reload);
    scroll.append(banner);
  }

  for (const group of groupEvidenceByDay(visible)) {
    const day = document.createElement("div");
    day.className = "evday";
    day.dataset.evidenceDay = group.day ?? "undated";
    // The undated group leads, which is where Core's own order puts it.
    day.textContent =
      group.day ?? translate(locale, "d1.evidence.day.undated", {});
    scroll.append(day);

    for (const row of group.rows) {
      const line = document.createElement("button");
      line.type = "button";
      line.className = row === selected ? "evrow2 on" : "evrow2";
      line.dataset.evidenceRow = row.id;
      line.setAttribute("aria-current", String(row === selected));

      const time = document.createElement("span");
      time.className = "tm2";
      time.textContent =
        row.timestamp === null ? "—" : localTimeLabel(row.timestamp);
      line.append(time);

      const glyph = document.createElement("span");
      glyph.className = "ic2";
      glyph.textContent = KIND_GLYPH[row.kind] ?? "·";
      glyph.title = kindLabel(locale, row.kind);
      line.append(glyph);

      const title = document.createElement("span");
      title.className = "tt2";
      const kind = document.createElement("b");
      kind.textContent = kindLabel(locale, row.kind);
      title.append(kind, ` · ${row.summary}`);
      title.title = row.summary;
      line.append(title);

      // Core's own owner, or nothing. `source` is a producer label, not an
      // owner, so a row Core could not attribute carries no Lane chip.
      if (row.ownerLaneId) {
        const lane = document.createElement("span");
        lane.className = "lane-b";
        lane.textContent = row.ownerLaneId;
        lane.title = row.ownerLaneId;
        line.append(lane);
      }

      line.addEventListener("click", () => {
        handlers.onSelect?.(row.id);
      });
      scroll.append(line);
    }
  }

  // The list's non-row states, each with its own sentence. Only the last two
  // are ever drawn as "nothing here".
  if (!projection.capabilityAvailable) {
    stateNote(
      scroll,
      "unavailable",
      translate(locale, "d1.evidence.unavailable", {
        capability: EVIDENCE_READS_CAPABILITY,
      }),
    );
  } else if (projection.outcome.state === "rejected") {
    // Core's own words, unedited: the reason carries the actionable hint.
    stateNote(
      scroll,
      "rejected",
      projection.outcome.reason ?? translate(locale, "d1.evidence.rejected", {}),
      true,
    );
  } else if (!projection.loaded) {
    stateNote(scroll, "pending", translate(locale, "d1.evidence.pending", {}));
  } else if (projection.rows.length === 0) {
    stateNote(scroll, "empty", translate(locale, "d1.evidence.empty", {}));
  } else if (visible.length === 0) {
    // A different sentence from "no evidence": the archive has rows, this
    // client's own search box hid them.
    stateNote(
      scroll,
      "search-empty",
      translate(locale, "d1.evidence.searchEmpty", { rows: String(projection.rows.length) }),
    );
  }
  main.append(scroll);

  // Paging foot. `complete` and "there may be more" are different facts and
  // both are stated; neither is ever left to the absence of a button.
  if (projection.loaded) {
    const foot = document.createElement("div");
    foot.className = "evidence-paging";
    if (projection.complete) {
      const note = document.createElement("p");
      note.className = "evidence-paging-note";
      note.dataset.evidenceComplete = "true";
      note.textContent = translate(locale, "d1.evidence.complete", {});
      foot.append(note);
    } else {
      const older = document.createElement("button");
      older.type = "button";
      older.className = "evidence-older";
      older.dataset.evidenceLoadOlder = "true";
      older.textContent = translate(locale, "d1.evidence.loadOlder", {});
      // Core's own cursor is the only thing that can page: without one there
      // is nothing to ask with, and the client must not build one.
      older.disabled = !handlers.onLoadOlder || projection.nextAfter === null;
      older.addEventListener("click", () => handlers.onLoadOlder?.());
      foot.append(older);
    }
    main.append(foot);
  }

  /* ---- detail rail ---- */

  const detail = document.createElement("div");
  detail.className = "evdet";
  detail.dataset.evidenceDetail = "true";

  const head = document.createElement("div");
  head.className = "evdethead";
  const line1 = document.createElement("div");
  line1.className = "t1";
  const line2 = document.createElement("div");
  line2.className = "t2";

  const close = document.createElement("button");
  close.type = "button";
  close.className = "evidence-icon evidence-close";
  close.dataset.evidenceClose = "true";
  close.textContent = "✕";
  const closeLabel = translate(locale, "d1.evidence.close", {});
  close.title = closeLabel;
  close.setAttribute("aria-label", closeLabel);
  close.disabled = !handlers.onClose;
  close.addEventListener("click", () => handlers.onClose?.());

  if (selected) {
    line1.textContent = `${KIND_GLYPH[selected.kind] ?? "·"} ${kindLabel(locale, selected.kind)} · ${selected.summary}`;
    line1.title = selected.summary;
    const stamp =
      selected.timestamp === null
        ? translate(locale, "d1.evidence.undatedValue", {})
        : `${localTimeLabel(selected.timestamp)}`;
    line2.textContent = [selected.ownerLaneId, stamp, selected.id]
      .filter((part): part is string => Boolean(part))
      .join(" · ");
  } else {
    line1.textContent = translate(locale, "d1.evidence.selectRow", {});
  }
  line1.append(close);
  head.append(line1, line2);
  detail.append(head);

  if (selected) {
    /* Report: Core's own fields, each as a fact. */
    const report = section(detail, translate(locale, "d1.evidence.detail.report", {}), "report");
    keyValue(report, translate(locale, "d1.evidence.field.id", {}), selected.id);
    keyValue(report, translate(locale, "d1.evidence.field.kind", {}), selected.kind);
    const absent = translate(locale, "d1.evidence.absentValue", {});
    keyValue(
      report,
      translate(locale, "d1.evidence.field.source", {}),
      selected.source ?? absent,
    );
    keyValue(
      report,
      translate(locale, "d1.evidence.field.path", {}),
      selected.path ?? absent,
    );
    keyValue(
      report,
      translate(locale, "d1.evidence.field.owner", {}),
      selected.ownerLaneId ?? absent,
    );
    if (selected.canonical) {
      keyValue(
        report,
        translate(locale, "d1.evidence.field.item", {}),
        selected.canonical.itemId,
      );
      keyValue(
        report,
        translate(locale, "d1.evidence.field.bundle", {}),
        selected.canonical.bundleId,
      );
      keyValue(
        report,
        translate(locale, "d1.evidence.field.hash", {}),
        selected.canonical.sourceHash,
      );
      keyValue(
        report,
        translate(locale, "d1.evidence.field.producer", {}),
        `${selected.canonical.producerIdentity} · ${selected.canonical.producerRole}`,
      );
    } else {
      const note = document.createElement("p");
      note.className = "evidence-note";
      note.dataset.evidenceNoCanonical = "true";
      note.textContent = translate(locale, "d1.evidence.noCanonical", {});
      report.append(note);
    }

    /* Metadata: Core's keys, rendered, never interpreted. */
    if (selected.metadata.length > 0) {
      const meta = section(
        detail,
        translate(locale, "d1.evidence.detail.metadata", {}),
        "metadata",
      );
      const note = document.createElement("p");
      note.className = "evidence-note";
      note.dataset.evidenceMetadataNote = "true";
      note.textContent = translate(locale, "d1.evidence.metadataNote", {});
      meta.append(note);
      for (const fact of selected.metadata) keyValue(meta, fact.key, fact.value);
    }

    /* Content: verified canonical bytes, or the typed reason there are none. */
    const body = section(detail, translate(locale, "d1.evidence.detail.content", {}), "content");
    renderContent(body, selected, content, locale);

    /* Linked: the Core objects this row names. Facts, not navigation. */
    const linked = section(detail, translate(locale, "d1.evidence.detail.linked", {}), "linked");
    const chipTargets: Array<{ id: string; text: string; title: string }> = [];
    if (selected.canonical) {
      chipTargets.push({
        id: "canonical",
        text: `${translate(locale, "d1.evidence.field.item", {})} ${selected.canonical.itemId}`,
        title: `${selected.canonical.bundleId} · ${selected.canonical.sourceHash}`,
      });
    }
    if (selected.path) {
      chipTargets.push({
        id: "path",
        text: selected.path,
        title: selected.path,
      });
    }
    if (selected.ownerTaskId) {
      chipTargets.push({
        id: "task",
        text: `${translate(locale, "d1.evidence.field.task", {})} ${selected.ownerTaskId}`,
        title: selected.ownerTaskId,
      });
    }
    if (chipTargets.length === 0) {
      const note = document.createElement("p");
      note.className = "evidence-note";
      note.dataset.evidenceNoLinks = "true";
      note.textContent = translate(locale, "d1.evidence.noLinks", {});
      linked.append(note);
    }
    for (const target of chipTargets) {
      const chip = document.createElement("span");
      chip.className = "evchip";
      chip.dataset.evidenceChip = target.id;
      chip.textContent = target.text;
      chip.title = target.title;
      linked.append(chip);
    }

    /* Footer: the two destinations this row actually has. */
    const foot = document.createElement("div");
    foot.className = "evfoot";

    const review = document.createElement("button");
    review.type = "button";
    review.className = "d1-mpill ev-go";
    review.dataset.evidenceOpenReview = "true";
    review.textContent = translate(locale, "d1.evidence.openInReview", {});
    // Three separate conditions, each with its own sentence, because the
    // operator can act on a different one in each case.
    const reviewBlocked =
      selected.kind !== "patch"
        ? translate(locale, "d1.evidence.openInReview.notPatch", {})
        : !handlers.onOpenReview
          ? translate(locale, "d1.evidence.openInReview.unbound", {})
          : handlers.reviewAvailable === false
            ? translate(locale, "d1.evidence.openInReview.unavailable", {})
            : null;
    review.disabled = reviewBlocked !== null;
    // Stated even when enabled: DiffReview reads the *working tree*, and this
    // row is a patch Core recorded. They are related, not the same thing.
    review.title = reviewBlocked ?? translate(locale, "d1.evidence.openInReview.scope", {});
    review.addEventListener("click", () => handlers.onOpenReview?.());
    foot.append(review);

    const audit = document.createElement("button");
    audit.type = "button";
    audit.className = "d1-mpill";
    audit.dataset.evidenceOpenAudit = `${AUDIT_KIND_EVIDENCE}:${selected.id}`;
    audit.textContent = translate(locale, "d1.evidence.openAudit", {});
    audit.disabled = !handlers.onOpenAuditTrail;
    // The `D-AUDIT` link is one-way: audit rows link evidence, not the other
    // way round. So this opens the trail *scoped to* this evidence object
    // rather than claiming the row carries an audit id of its own.
    audit.title = translate(locale, "d1.evidence.openAudit.scope", {});
    audit.addEventListener("click", () =>
      handlers.onOpenAuditTrail?.({ kind: AUDIT_KIND_EVIDENCE, id: selected.id }),
    );
    foot.append(audit);
    detail.append(foot);
  }

  wrap.append(main, detail);
  host.replaceChildren(wrap);
}

/**
 * The content block for one row.
 *
 * Every branch is a Core fact. There is no branch for "probably fine": Core
 * publishes verified canonical bytes or a typed reason, and this renders
 * exactly what arrived.
 */
function renderContent(
  host: HTMLElement,
  row: EvidenceRowProjection,
  content: EvidenceContentProjection,
  locale: Locale,
): void {
  // A content answer belongs to the row Core echoed it for. While the selected
  // row is a different one, the block says "reading", never the other row's
  // bytes — attributing one row's content to another is the single worst thing
  // this pane could do.
  if (content.evidenceId !== row.id) {
    stateNote(host, "content-absent", translate(locale, "d1.evidence.content.absent", {}));
    return;
  }
  if (content.outcome.state === "rejected") {
    stateNote(
      host,
      "content-rejected",
      content.outcome.reason ?? translate(locale, "d1.evidence.content.rejected", {}),
      true,
    );
    return;
  }
  if (content.outcome.state === "pending") {
    stateNote(host, "content-pending", translate(locale, "d1.evidence.content.pending", {}));
    return;
  }

  if (content.kind === "text") {
    const term = document.createElement("div");
    term.className = "evterm";
    term.dataset.evidenceContent = "text";
    for (const line of (content.text ?? "").split("\n")) {
      const entry = document.createElement("div");
      entry.textContent = line;
      term.append(entry);
    }
    host.append(term);
    if (content.truncated) {
      stateNote(
        host,
        "content-truncated",
        translate(locale, "d1.evidence.content.truncated", {}),
      );
    }
    appendHash(host, content, locale);
    return;
  }

  if (content.kind === "diff") {
    const body = document.createElement("div");
    body.dataset.evidenceContent = "diff";
    // The same row renderer DiffReview and the approval surfaces use, because
    // Core parsed this patch with the same producer: one evidence patch and
    // one workspace diff must render as one shape, not two.
    renderDiffFiles(body, content.document?.files ?? [], locale);
    host.append(body);
    if (content.document?.truncated) {
      stateNote(host, "content-truncated", translate(locale, "d1.evidence.content.truncated", {}));
    }
    appendHash(host, content, locale);
    return;
  }

  if (content.kind === "unavailable") {
    const reason = content.reason ?? "unknown";
    const key = REASON_COPY[reason];
    stateNote(
      host,
      `unavailable-${reason}`,
      key
        ? translate(locale, key, {})
        : translate(locale, "d1.evidence.reason.unnamed", { reason }),
    );
    return;
  }

  // `EvidenceContent` is `#[non_exhaustive]`: a shape this build cannot name
  // says so rather than rendering an empty body, which would read as "there
  // was nothing".
  stateNote(
    host,
    "content-unknown",
    translate(locale, "d1.evidence.content.unknown", { kind: content.kind }),
  );
}

/// The hash Core verified the served bytes against, so a reader can join what
/// is on screen to the canonical reference the row published.
function appendHash(
  host: HTMLElement,
  content: EvidenceContentProjection,
  locale: Locale,
): void {
  if (!content.sha256) return;
  const note = document.createElement("p");
  note.className = "evidence-note";
  note.dataset.evidenceContentHash = content.sha256;
  note.textContent = translate(locale, "d1.evidence.content.hash", {
    hash: content.sha256,
  });
  host.append(note);
}
