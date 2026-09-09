import { renderDiffBody } from "../components/diff_rows";
import { translate, type Locale } from "../i18n/catalog";
import {
  STRUCTURED_DIFF_CAPABILITY,
  type WorkspaceDiffEntryProjection,
  type WorkspaceDiffProjection,
} from "../models/diff_review";
import "./diff_review.css";

/**
 * DiffReview — the in-cockpit per-file review of the working tree.
 *
 * The registered `D-RAILNAV ①` family: `.review > .filetree + .diffpane`. It
 * is a *view* inside D1's centre pane rather than a standalone D-screen,
 * because that is what the design registers and what the operator needs: the
 * transcript is one Escape away, not one navigation away.
 *
 * Read side only. Everything the pane draws comes from one
 * `WorkspaceDiffLoaded` page Core answered: the client never runs git, never
 * parses diff text, never re-sorts Core's order, and never derives `staged`
 * from the index classification Core already derived it from.
 *
 * The commit bar the design draws is present, visibly disabled, and labelled
 * with `GUI-CORE-020`. Operator git actions need a Core contract that does not
 * exist in `frontend-contract-v1`; a bar that looked live and resolved to
 * nothing would be worse than a bar that says why it cannot act. It carries no
 * handlers at all, so there is nothing behind it to accidentally enable.
 */

export type { WorkspaceDiffProjection } from "../models/diff_review";

/// The open register entry that has to close before the commit bar can act.
const OPERATOR_GIT_CODE = "GUI-CORE-020";

export interface DiffReviewHandlers {
  /** Re-reads the page from Core. Absent while no host is bound. */
  onRefresh?: () => void;
  /** Returns the centre pane to the transcript. */
  onClose?: () => void;
  /** Remembers the operator's file selection across an ordered Core refresh. */
  onSelect?: (path: string | null) => void;
  /** Path to open with, when the view is being rebuilt after a re-read. */
  selectedPath?: string | null;
}

/// The design's `.ftrow .stat` glyphs, one per `WorkspaceChangeKind`.
const KIND_GLYPH: Record<string, { glyph: string; variant: string }> = {
  modified: { glyph: "M", variant: "m" },
  added: { glyph: "A", variant: "a" },
  deleted: { glyph: "D", variant: "d" },
  renamed: { glyph: "R", variant: "m" },
  untracked: { glyph: "?", variant: "a" },
};

/**
 * The kind glyph for one row.
 *
 * With scope `Both` a path changed on both sides carries its *worktree* rows,
 * so the worktree classification is the one that describes what the operator's
 * file holds right now; the index classification stands in when the change is
 * staged only. `diff.kind` is the last resort, and a row Core classified in no
 * way at all gets a neutral marker rather than a borrowed letter.
 */
function entryKind(entry: WorkspaceDiffEntryProjection): string | null {
  return entry.worktree ?? entry.index ?? entry.diff?.kind ?? null;
}

/// Splits a path so the distinctive tail can be kept while the leading
/// directories ellipsize. `README.md` has no directory half, which is fine.
function splitPath(path: string): { directories: string; base: string } {
  const cut = path.lastIndexOf("/");
  return cut < 0
    ? { directories: "", base: path }
    : { directories: path.slice(0, cut + 1), base: path.slice(cut + 1) };
}

function stateNote(
  host: HTMLElement,
  state: string,
  text: string,
  alert = false,
): HTMLElement {
  const note = document.createElement("p");
  note.className = "review-state";
  note.dataset.reviewState = state;
  if (alert) note.setAttribute("role", "alert");
  note.textContent = text;
  host.append(note);
  return note;
}

/**
 * Renders the view into `host`, replacing whatever was there.
 *
 * Selection is presentation state and lives with the caller: an ordered Core
 * refresh rebuilds this DOM, so the selected path is passed in and reported
 * back rather than being read out of the tree afterwards.
 */
export function renderDiffReview(
  host: HTMLElement,
  projection: WorkspaceDiffProjection,
  locale: Locale,
  handlers: DiffReviewHandlers = {},
): void {
  const entries = projection.entries;
  const requested = handlers.selectedPath ?? null;
  // A selection Core no longer publishes falls back to the head of the list
  // rather than leaving the pane pointed at a file that is gone.
  const selected =
    entries.find((entry) => entry.path === requested) ?? entries[0] ?? null;

  const review = document.createElement("section");
  review.className = "review";
  review.dataset.diffReview = "true";
  review.setAttribute("role", "region");
  review.setAttribute("aria-label", translate(locale, "d1.review.region", {}));

  /* ---- file tree ---- */

  const tree = document.createElement("div");
  tree.className = "filetree";

  const head = document.createElement("div");
  head.className = "fthead";
  const title = document.createElement("h3");
  title.textContent = translate(locale, "d1.review.title", {});
  head.append(title);

  // Totals are summed over the entries Core published rows for. An entry with
  // no diff contributes no counts, so the summary says so instead of quietly
  // under-reporting.
  const counted = entries.filter((entry) => entry.diff !== null);
  const additions = counted.reduce((total, entry) => total + (entry.diff?.additions ?? 0), 0);
  const deletions = counted.reduce((total, entry) => total + (entry.diff?.deletions ?? 0), 0);
  const count = document.createElement("span");
  count.className = "ct";
  count.textContent = translate(locale, "d1.review.count", {
    files: String(entries.length),
    additions: String(additions),
    deletions: String(deletions),
  });
  head.append(count);
  if (counted.length !== entries.length) {
    const partial = document.createElement("span");
    partial.className = "review-partial";
    partial.dataset.reviewCountPartial = "true";
    partial.textContent = "…";
    partial.title = translate(locale, "d1.review.countPartial", {});
    head.append(partial);
  }

  const refresh = document.createElement("button");
  refresh.type = "button";
  refresh.className = "review-icon";
  refresh.dataset.reviewRefresh = "true";
  refresh.textContent = "⟳";
  const refreshLabel = translate(locale, "d1.review.refresh", {});
  refresh.title = refreshLabel;
  refresh.setAttribute("aria-label", refreshLabel);
  // Visible and disabled without a host: the view is a read, and a read the
  // shell cannot issue must say so rather than look idle.
  refresh.disabled = !handlers.onRefresh;
  refresh.addEventListener("click", () => handlers.onRefresh?.());
  head.append(refresh);
  tree.append(head);

  const list = document.createElement("div");
  list.className = "ftlist";
  list.setAttribute("role", "list");
  for (const entry of entries) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = entry === selected ? "ftrow on" : "ftrow";
    row.dataset.path = entry.path;
    row.setAttribute("role", "listitem");
    row.setAttribute("aria-current", String(entry === selected));

    const kind = entryKind(entry);
    const marker = kind ? KIND_GLYPH[kind] : undefined;
    const stat = document.createElement("span");
    stat.className = marker ? `stat ${marker.variant}` : "stat";
    stat.textContent = marker?.glyph ?? "·";
    stat.title = kind ?? translate(locale, "d1.review.kindUnknown", {});
    row.append(stat);

    const name = document.createElement("span");
    name.className = "fn";
    name.title = entry.path;
    const { directories, base } = splitPath(entry.path);
    if (directories) {
      const head = document.createElement("span");
      head.className = "dir";
      head.textContent = directories;
      name.append(head);
    }
    const tail = document.createElement("span");
    tail.className = "base";
    tail.textContent = base;
    name.append(tail);
    row.append(name);

    if (entry.diff) {
      const counts = document.createElement("span");
      counts.className = "pm";
      // Real even for an omitted file: the counts are what make "not shown"
      // readable as something other than "unchanged".
      counts.textContent = `+${entry.diff.additions} −${entry.diff.deletions}`;
      row.append(counts);
    }
    if (entry.staged) {
      const staged = document.createElement("span");
      staged.className = "ck";
      staged.textContent = "✓";
      staged.title = translate(locale, "d1.review.staged", {});
      row.append(staged);
    }
    row.addEventListener("click", () => {
      handlers.onSelect?.(entry.path);
      renderDiffReview(host, projection, locale, {
        ...handlers,
        selectedPath: entry.path,
      });
    });
    list.append(row);
  }

  // The four non-row states, each with its own sentence. Only the last one is
  // ever drawn as "no changes".
  if (!projection.capabilityAvailable) {
    stateNote(
      list,
      "unavailable",
      translate(locale, "d1.review.unavailable", {
        capability: STRUCTURED_DIFF_CAPABILITY,
      }),
    );
  } else if (projection.outcome.state === "rejected") {
    // Core's own words, unedited: the reason carries the actionable hint.
    stateNote(
      list,
      "rejected",
      projection.outcome.reason ?? translate(locale, "d1.review.rejected", {}),
      true,
    );
  } else if (!projection.loaded) {
    stateNote(list, "pending", translate(locale, "d1.review.pending", {}));
  } else if (entries.length === 0) {
    stateNote(list, "empty", translate(locale, "d1.review.empty", {}));
  }
  tree.append(list);

  /* ---- diff pane ---- */

  const pane = document.createElement("div");
  pane.className = "diffpane";

  const paneHead = document.createElement("div");
  paneHead.className = "dphead";
  const paneName = document.createElement("span");
  paneName.className = "fn";
  if (selected) {
    paneName.title = selected.path;
    const { directories, base } = splitPath(selected.path);
    paneName.textContent = `${directories}${base}`;
  } else {
    paneName.textContent = translate(locale, "d1.review.selectFile", {});
  }
  paneHead.append(paneName);

  if (selected?.diff?.oldPath) {
    const renamed = document.createElement("span");
    renamed.className = "review-renamed";
    renamed.dataset.reviewRenamed = "true";
    renamed.textContent = translate(locale, "d1.review.renamed", {
      from: selected.diff.oldPath,
      to: selected.path,
    });
    paneHead.append(renamed);
  }

  const segmented = document.createElement("div");
  segmented.className = "seg";
  const unified = document.createElement("button");
  unified.type = "button";
  unified.className = "on";
  unified.dataset.diffView = "unified";
  unified.textContent = translate(locale, "d1.review.unified", {});
  unified.setAttribute("aria-pressed", "true");
  const split = document.createElement("button");
  split.type = "button";
  split.dataset.diffView = "split";
  split.textContent = translate(locale, "d1.review.split", {});
  // Visible and disabled rather than hidden: the design registers both halves
  // of this control, and only the unified body is built.
  split.disabled = true;
  split.title = translate(locale, "d1.review.splitUnavailable", {});
  split.setAttribute("aria-disabled", "true");
  segmented.append(unified, split);
  paneHead.append(segmented);

  const close = document.createElement("button");
  close.type = "button";
  close.className = "review-icon";
  close.dataset.reviewClose = "true";
  close.textContent = "✕";
  const closeLabel = translate(locale, "d1.review.close", {});
  close.title = closeLabel;
  close.setAttribute("aria-label", closeLabel);
  close.disabled = !handlers.onClose;
  close.addEventListener("click", () => handlers.onClose?.());
  paneHead.append(close);
  pane.append(paneHead);

  if (projection.truncated) {
    // A page bound is a fact about the page, not about the file on screen, so
    // it belongs above the body rather than inside one file's rows.
    const banner = document.createElement("p");
    banner.className = "review-banner";
    banner.dataset.reviewTruncated = "true";
    banner.textContent = translate(locale, "d1.review.truncated", {});
    pane.append(banner);
  }
  if (projection.stale) {
    // The rows stay: "re-read due" must never blank the only facts the
    // operator has in front of them.
    const banner = document.createElement("p");
    banner.className = "review-banner";
    banner.dataset.reviewStale = "true";
    banner.setAttribute("role", "status");
    banner.textContent = translate(locale, "d1.review.stale", {});
    pane.append(banner);
  }

  if (selected) {
    renderDiffBody(pane, selected.diff, locale);
  } else {
    const body = document.createElement("div");
    body.className = "diffbody";
    pane.append(body);
  }

  /* ---- commit bar (disabled: GUI-CORE-020) ---- */

  const commitBar = document.createElement("div");
  commitBar.className = "commitbar";
  commitBar.dataset.reviewCommit = "true";
  commitBar.dataset.reviewCommitCode = OPERATOR_GIT_CODE;
  const message = document.createElement("div");
  message.className = "msg-in";
  message.textContent = translate(locale, "d1.review.commitUnavailable", {
    code: OPERATOR_GIT_CODE,
  });
  commitBar.append(message);
  for (const [key, variant] of [
    ["d1.review.stageAll", ""],
    ["d1.review.commit", "commit"],
    ["d1.review.commitPush", "push"],
  ] as const) {
    const action = document.createElement("button");
    action.type = "button";
    action.className = variant ? `cbtn ${variant}` : "cbtn";
    action.textContent = translate(locale, key, {});
    // No listener at all. A disabled control with a handler behind it is one
    // edit away from claiming an effect Core cannot perform.
    action.disabled = true;
    action.setAttribute("aria-disabled", "true");
    action.title = translate(locale, "d1.review.commitUnavailable", {
      code: OPERATOR_GIT_CODE,
    });
    commitBar.append(action);
  }
  pane.append(commitBar);

  review.append(tree, pane);
  host.replaceChildren(review);
}
