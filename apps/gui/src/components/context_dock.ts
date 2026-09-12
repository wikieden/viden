import { translate, type Locale, type MessageKey } from "../i18n/catalog";
import type { WorkspaceDiffProjection } from "../models/diff_review";
import type { D1CockpitProjection, WorkspaceSourceProjection } from "../models/workspace";
import type { PaletteWorkspaceFiles } from "./command_palette";
import { renderDiffBody } from "./diff_rows";
import { formatCompactCount } from "./statusbar";
import { environmentValues } from "./environment";
import { boundedLiveWorkEntries } from "./live_work";

/**
 * The cockpit's right pane: the design's `.rail.dock` — a `.docktabs` strip
 * over one `.dockbody` panel
 * (`docs/viden-design/Viden/GUI/Viden - 桌面驾驶舱 (GUI).html`, `ContextDock`).
 *
 * **Three panels are live, three are named and disabled.** The design's strip
 * carries six tabs. Environment, Files and Diff are backed by facts Core
 * already publishes (`contextDock`, `runtime.workspace_files`,
 * `runtime.structured_diff`). Terminal, Code and Docs are not, so they render
 * as disabled tabs carrying the exact reason — a hidden tab would make the
 * dock look complete, and an enabled-and-inert one would lie about what a
 * click does.
 *
 * **The component is stateless.** The open tab, the expanded directories and
 * the selected path are the cockpit's presentation state; this renderer is a
 * pure function of the model it is handed, so an ordered Core refresh that
 * rebuilds the dock cannot move what the operator opened. Nothing is written
 * to `localStorage`: the frontend contract makes Core the single preference
 * authority, and a dock tab is a posture rather than a preference anyway.
 *
 * **Absence, emptiness and refusal stay three sentences.** Every panel below
 * distinguishes "Core publishes no such capability", "the read is out",
 * "Core answered with nothing" and "Core refused, in its own words". Folding
 * any pair of them into one empty list is the failure mode this dock exists to
 * avoid.
 *
 * `renderDockTab` is the seam later batches extend: G7 fills the Code tab once
 * it consumes `runtime.workspace_file_reads`, and each section below is its own
 * exported renderer so a batch can replace one without touching the strip.
 */

/** The six panels the design's `ALLTABS` names, in its own order. */
export type DockTab = "environment" | "files" | "terminal" | "code" | "diff" | "docs";

export interface DockTabDefinition {
  id: DockTab;
  labelKey: MessageKey;
  /**
   * The chord this build actually binds, or `null` when it binds none.
   *
   * The design's own chords are `⌥⌘E` / `⌘P` / `⌘J` / `⌘O` / `⌘D` / `⌘/`. Two
   * are not bound here: `⌘J` and `⌘/` belong to tabs that cannot open, and
   * `⌘O` is already the Welcome screen's folder picker, so the earlier binding
   * keeps the chord and the Code tab carries none.
   */
  chordKey: MessageKey | null;
  live: boolean;
  /** Why a non-live tab cannot open. Absent for a live tab. */
  reasonKey?: MessageKey;
}

export const DOCK_TABS: readonly DockTabDefinition[] = [
  {
    id: "environment",
    labelKey: "d1.dock.tab.environment",
    chordKey: "d1.dock.chord.environment",
    live: true,
  },
  { id: "files", labelKey: "d1.dock.tab.files", chordKey: "d1.dock.chord.files", live: true },
  {
    id: "terminal",
    labelKey: "d1.dock.tab.terminal",
    chordKey: null,
    live: false,
    reasonKey: "d1.dock.tab.terminal.reason",
  },
  {
    id: "code",
    labelKey: "d1.dock.tab.code",
    chordKey: null,
    live: false,
    reasonKey: "d1.dock.tab.code.reason",
  },
  { id: "diff", labelKey: "d1.dock.tab.diff", chordKey: "d1.dock.chord.diff", live: true },
  {
    id: "docs",
    labelKey: "d1.dock.tab.docs",
    chordKey: null,
    live: false,
    reasonKey: "d1.dock.tab.docs.reason",
  },
];

/** The tabs a chord or a click may actually open. */
export const LIVE_DOCK_TABS: readonly DockTab[] = DOCK_TABS.filter((tab) => tab.live).map(
  (tab) => tab.id,
);

export interface DockDiffModel {
  /** False while no host is bound: nothing can ever fill this panel. */
  bound: boolean;
  /** Null until the no-traffic capability read answers. */
  capabilityAvailable: boolean | null;
  /** Core's last `WorkspaceDiffLoaded` page for the dock's target. */
  projection: WorkspaceDiffProjection | null;
  selectedPath: string | null;
  /** Paths whose rows the operator opened. Collapsed is the default. */
  expanded: ReadonlySet<string>;
  onSelect: (path: string) => void;
  onToggle: (path: string) => void;
  /** Opens DiffReview on exactly this file. */
  onOpenInReview: (path: string) => void;
}

export interface DockFilesModel {
  bound: boolean;
  capabilityAvailable: boolean | null;
  /**
   * Core's pages keyed by the prefix each answered; `""` is the target root.
   *
   * One page per directory the operator opened, because `QueryWorkspaceFiles`
   * is prefix-scoped and bounded: reading the whole tree at once would be one
   * truncated page with no way to see past the bound.
   */
  pages: ReadonlyMap<string, PaletteWorkspaceFiles>;
  expanded: ReadonlySet<string>;
  selectedPath: string | null;
  onToggleDirectory: (path: string) => void;
  onSelect: (path: string) => void;
  /**
   * Whether Core published `runtime.workspace_file_reads`. Until a build
   * consumes it (G7), the inspector's Open stays disabled and names it.
   */
  fileReadsAvailable: boolean;
}

export interface DockCommitModel {
  available: boolean;
  /** What is missing when `available` is false — a capability id or Core's own word. */
  reason: string | null;
  /** Routes to DiffReview's commit bar and focuses the titlebar sync chip. */
  onCommitOrPush: () => void;
}

export interface ContextDockModel {
  projection: D1CockpitProjection;
  locale: Locale;
  /**
   * False while the cockpit's Lane selection leads the projection Core has
   * republished. The dock then shows the switching notice rather than one
   * Lane's facts under another Lane's name.
   */
  matchesSelectedLane: boolean;
  tab: DockTab;
  onSelectTab: (tab: DockTab) => void;
  /**
   * The selected Lane's own worktree source (`C5`'s `lane_sources`), or null
   * when Core published none for it. The Local section prefers it over the
   * workspace sample and labels which one it is showing.
   */
  laneSource: WorkspaceSourceProjection | null;
  diff: DockDiffModel;
  files: DockFilesModel;
  commit: DockCommitModel;
}

function definition(list: HTMLDListElement, label: string, value: string): void {
  const term = document.createElement("dt");
  term.textContent = label;
  const detail = document.createElement("dd");
  detail.textContent = value;
  list.append(term, detail);
}

function emptyState(kind: string, text: string): HTMLElement {
  const element = document.createElement("p");
  element.className = "d1-empty";
  element.dataset.typedEmpty = kind;
  element.textContent = text;
  return element;
}

function statusLabel(locale: Locale, status: string): string {
  const keys: Record<string, MessageKey> = {
    connected: "d1.context.connected",
    ready: "d1.context.ready",
    degraded: "d1.context.degraded",
    offline: "d1.context.offline",
    unavailable: "d1.unavailable",
    added: "d1.status.added",
    modified: "d1.status.modified",
    deleted: "d1.status.deleted",
    renamed: "d1.status.renamed",
    untracked: "d1.status.untracked",
    running: "d1.status.running",
    passed: "d1.status.passed",
    failed: "d1.status.failed",
    cancelled: "d1.status.cancelled",
    queued: "d1.status.queued",
  };
  const key = keys[status];
  return key ? translate(locale, key, {} as never) : status;
}

/**
 * Core's own MCP `detail_key` values, localized.
 *
 * `mcp_runtime_service_health` publishes exactly one row today and it is
 * always `Unavailable`: configuration discovery is visibility only, and until
 * MCP tools enter the shared permission path Core must not publish a connected
 * fact. These two keys are that row's two reasons.
 */
const MCP_DETAIL_KEYS: Record<string, MessageKey | undefined> = {
  "mcp.config_not_found": "d1.dock.mcp.detail.notFound",
  "mcp.config_visible_runtime_unavailable": "d1.dock.mcp.detail.configVisible",
};

function dirtyValue(locale: Locale, dirty: boolean): string {
  return translate(locale, dirty ? "d1.context.dirty.true" : "d1.context.dirty.false", {});
}

/**
 * One collapsible `.envsec`.
 *
 * The heading stays a real `<button>` inside an `<h2>`: the design draws a
 * clickable `.envhd` row, and a div with a click handler is not reachable by
 * keyboard.
 */
function appendSection(
  dock: HTMLElement,
  id: string,
  title: string,
  renderBody: (body: HTMLElement) => void,
): HTMLElement {
  const section = document.createElement("section");
  section.className = "envsec";
  section.dataset.contextSection = id;
  section.setAttribute("aria-label", title);

  const heading = document.createElement("h2");
  heading.className = "envhd";
  const button = document.createElement("button");
  const body = document.createElement("div");
  const bodyId = `d1-context-section-${id}`;
  button.type = "button";
  button.className = "t";
  button.dataset.contextSectionToggle = "true";
  button.setAttribute("aria-expanded", "true");
  button.setAttribute("aria-controls", bodyId);
  const caret = document.createElement("span");
  caret.className = "caret";
  caret.setAttribute("aria-hidden", "true");
  caret.textContent = "▾";
  const label = document.createElement("span");
  label.textContent = title;
  button.append(caret, label);
  button.addEventListener("click", () => {
    const expanded = button.getAttribute("aria-expanded") === "true";
    button.setAttribute("aria-expanded", String(!expanded));
    heading.classList.toggle("collapsed", expanded);
    body.hidden = expanded;
  });
  button.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    button.click();
  });
  body.id = bodyId;
  body.dataset.contextSectionBody = id;
  renderBody(body);
  heading.append(button);
  section.append(heading, body);
  dock.append(section);
  return section;
}

function selectedLane(
  projection: D1CockpitProjection,
): D1CockpitProjection["lanes"][number] | null {
  return projection.lanes.find((lane) => lane.id === projection.selectedLaneId) ?? null;
}

function renderLaneAgent(
  body: HTMLElement,
  projection: D1CockpitProjection,
  locale: Locale,
): void {
  const lane = selectedLane(projection);
  const owner = projection.contextDock.laneAgent;
  if (!lane || !owner || owner.laneId !== lane.id) {
    body.append(emptyState("lane-agent", translate(locale, "d1.context.noAgentOwner", {})));
    return;
  }
  const sessions = projection.agentSessions.filter((session) => session.laneId === lane.id);
  if (sessions.length > 1 || (sessions[0] && sessions[0].sessionId !== owner.sessionId)) {
    body.append(emptyState("lane-agent", translate(locale, "d1.context.noAgentOwner", {})));
    return;
  }
  const session = sessions[0] ?? null;
  const list = document.createElement("dl");
  list.dataset.laneAgent = "true";
  definition(list, translate(locale, "d1.context.lane", {}), owner.laneId);
  definition(
    list,
    translate(locale, "d1.context.route", {}),
    session ? translate(locale, "d1.context.acp", {}) : translate(locale, "d1.context.native", {}),
  );
  definition(
    list,
    translate(locale, "d1.environment.model", {}),
    session?.model ?? projection.contextDock.provider?.model ?? "—",
  );
  definition(
    list,
    translate(locale, "d1.context.status", {}),
    statusLabel(locale, session?.status ?? lane.status),
  );
  definition(list, translate(locale, "d1.context.session", {}), owner.sessionId ?? "—");
  body.append(list);
}

/* ---- Environment panel sections ---- */

/**
 * The design's Changes row: `+n −m` over one row per changed file.
 *
 * The two halves come from different Core reads on purpose. The totals are the
 * workspace source's own `git diff --numstat` counts, which Core publishes in
 * the cockpit projection with no command at all. The rows need the structured
 * diff page, which is a separate read — so the totals can be honest while the
 * list is still loading, absent, or refused, and each of those says so.
 */
export function renderChangesSection(
  body: HTMLElement,
  model: ContextDockModel,
  source: WorkspaceSourceProjection | null,
): void {
  const { locale, diff } = model;
  const summary = document.createElement("p");
  summary.className = "envrow";
  summary.dataset.changesSummary = "true";
  if (source) {
    const added = document.createElement("span");
    added.className = "ad";
    added.textContent = `+${source.added}`;
    const removed = document.createElement("span");
    removed.className = "rm";
    removed.textContent = `−${source.deleted}`;
    const stat = document.createElement("span");
    stat.className = "stat";
    stat.append(added, removed);
    const label = document.createElement("span");
    label.className = "el";
    label.textContent = translate(locale, "d1.dock.changes.lines", {});
    summary.append(label, stat);
  } else {
    summary.textContent = translate(locale, "d1.context.noSource", {});
  }
  body.append(summary);

  if (!diff.bound) {
    body.append(emptyState("changes-unbound", translate(locale, "d1.dock.unbound", {})));
    return;
  }
  if (diff.capabilityAvailable === false) {
    body.append(
      emptyState(
        "changes-capability",
        translate(locale, "d1.dock.changes.unavailable", {
          capability: "runtime.structured_diff",
        }),
      ),
    );
    return;
  }
  if (diff.projection?.outcome.state === "rejected") {
    // Core's own refusal, verbatim: never an empty list, which would read as a
    // tree with nothing in it.
    const rejected = document.createElement("p");
    rejected.className = "d1-empty";
    rejected.dataset.changesRejected = "true";
    rejected.setAttribute("role", "alert");
    rejected.textContent = translate(locale, "d1.dock.changes.rejected", {
      reason: diff.projection.outcome.reason ?? "",
    });
    body.append(rejected);
    return;
  }
  if (!diff.projection?.loaded) {
    body.append(emptyState("changes-pending", translate(locale, "d1.dock.changes.pending", {})));
    return;
  }
  if (diff.projection.entries.length === 0) {
    // The one state that may say "no changes": Core answered, with none.
    body.append(emptyState("changes-empty", translate(locale, "d1.dock.changes.none", {})));
    return;
  }
  for (const entry of diff.projection.entries) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "envrow";
    row.dataset.changesRow = entry.path;
    row.title = translate(locale, "d1.dock.changes.open", { path: entry.path });
    const status = document.createElement("span");
    status.className = "ei";
    status.textContent = changeGlyph(entry.index ?? entry.worktree);
    const label = document.createElement("span");
    label.className = "el";
    label.textContent = entry.path;
    row.append(status, label);
    if (entry.diff) {
      const stat = document.createElement("span");
      stat.className = "stat";
      stat.dataset.changesDelta = "true";
      const added = document.createElement("span");
      added.className = "ad";
      added.textContent = `+${entry.diff.additions}`;
      const removed = document.createElement("span");
      removed.className = "rm";
      removed.textContent = `−${entry.diff.deletions}`;
      stat.append(added, removed);
      row.append(stat);
    }
    row.addEventListener("click", () => diff.onOpenInReview(entry.path));
    body.append(row);
  }
  if (diff.projection.truncated) {
    body.append(emptyState("changes-truncated", translate(locale, "d1.dock.changes.truncated", {})));
  }
}

/// Core's own status word reduced to the design's one-glyph gutter. An
/// unmodeled status keeps a neutral dot rather than being mapped onto a
/// status it is not.
function changeGlyph(status: string | null): string {
  switch (status) {
    case "added":
      return "A";
    case "modified":
      return "M";
    case "deleted":
      return "D";
    case "renamed":
      return "R";
    case "untracked":
      return "?";
    default:
      return "·";
  }
}

/**
 * The design's Local row: branch, ahead/behind, dirty.
 *
 * Which source it is showing is part of the answer, not decoration: a Lane
 * worktree and the workspace root are different trees, and a branch printed
 * without saying whose it is is the exact mistake the Lane tab strip refuses
 * to make.
 */
export function renderLocalSection(
  body: HTMLElement,
  model: ContextDockModel,
  source: WorkspaceSourceProjection | null,
  scope: "lane" | "workspace",
): void {
  const { locale } = model;
  if (!source) {
    body.append(emptyState("local", translate(locale, "d1.context.noSource", {})));
    return;
  }
  const scopeRow = document.createElement("p");
  scopeRow.className = "d1-dock-scope";
  scopeRow.dataset.localScopeLabel = scope;
  scopeRow.textContent = translate(locale, "d1.dock.local.scope", {
    scope: translate(
      locale,
      scope === "lane" ? "d1.dock.local.lane" : "d1.dock.local.workspace",
      {},
    ),
  });
  const facts = document.createElement("dl");
  definition(facts, translate(locale, "d1.dock.local.branch", {}), source.branch ?? "—");
  definition(facts, translate(locale, "d4.worktree", {}), source.worktree ?? "—");
  definition(facts, translate(locale, "d1.context.ahead", {}), String(source.ahead));
  definition(facts, translate(locale, "d1.context.behind", {}), String(source.behind));
  definition(facts, translate(locale, "d1.context.dirty", {}), dirtyValue(locale, source.dirty));
  body.append(scopeRow, facts);
}

/** The design's "Commit or push" row, routed to the registered commit bar. */
export function renderCommitSection(body: HTMLElement, model: ContextDockModel): void {
  const { locale, commit } = model;
  const action = document.createElement("button");
  action.type = "button";
  action.className = "envrow";
  action.dataset.commitOrPush = "true";
  const label = document.createElement("span");
  label.className = "el";
  label.textContent = translate(locale, "d1.dock.commit.open", {});
  action.append(label);
  body.append(action);
  if (commit.available) {
    action.addEventListener("click", () => commit.onCommitOrPush());
    return;
  }
  const why = translate(locale, "d1.dock.commit.unavailable", {
    reason: commit.reason ?? "",
  });
  action.disabled = true;
  action.setAttribute("aria-disabled", "true");
  action.title = why;
  // Visible for the same reason the inspector's Open reason is: an operator
  // must be able to read why a control is inert without hovering it.
  const reason = document.createElement("p");
  reason.className = "d1-dock-reason";
  reason.dataset.commitReason = "true";
  reason.textContent = why;
  body.append(reason);
}

/** `D-BUDGET-BLIND`'s bar: Core's own used/limit, and a named blind spot. */
export function renderBudgetSection(body: HTMLElement, model: ContextDockModel): void {
  const { locale, projection } = model;
  const context = projection.contextDock.context;
  if (!context) {
    body.append(emptyState("context", translate(locale, "d1.context.noTypedContext", {})));
    return;
  }
  const used = document.createElement("p");
  used.className = "big";
  used.dataset.budgetUsed = "true";
  // The same compact form the statusbar's `CONTEXT` segment and the Lane tab
  // strip's meta slot use, so one budget reads the same in all three places.
  used.textContent = translate(locale, "d1.dock.budget.used", {
    used: formatCompactCount(context.usedTokens),
    limit: formatCompactCount(context.hardTokenLimit),
  });
  // Integer percent of the hard limit, which is the bound Core enforces. A
  // zero limit has no percentage rather than a fabricated 0%.
  const percent =
    context.hardTokenLimit > 0
      ? Math.min(100, Math.round((context.usedTokens / context.hardTokenLimit) * 100))
      : null;
  const bar = document.createElement("div");
  bar.className = "bar";
  bar.dataset.budgetBar = "true";
  if (percent !== null) {
    bar.dataset.budgetPercent = String(percent);
    bar.setAttribute("role", "progressbar");
    bar.setAttribute("aria-valuemin", "0");
    bar.setAttribute("aria-valuemax", "100");
    bar.setAttribute("aria-valuenow", String(percent));
    const fill = document.createElement("i");
    fill.style.width = `${percent}%`;
    bar.append(fill);
  }
  const wrapper = document.createElement("div");
  wrapper.className = "envctx";
  wrapper.append(used, bar);

  const footer = document.createElement("div");
  footer.className = "sub2";
  if (percent !== null) {
    const usedLabel = document.createElement("span");
    usedLabel.textContent = translate(locale, "d1.dock.budget.percent", {
      percent: String(percent),
    });
    footer.append(usedLabel);
  }
  if (projection.environment.costMicroUsd === null) {
    // `D-BUDGET-BLIND`: an external CLI bills its own tokens and Core cannot
    // meter them, so the panel names the blind spot rather than estimating.
    const blind = document.createElement("span");
    blind.className = "blindtag";
    blind.dataset.budgetBlind = "true";
    blind.textContent = translate(locale, "d1.dock.budget.blind", {});
    footer.append(blind);
  } else {
    const spent = document.createElement("span");
    spent.dataset.budgetCost = "true";
    spent.textContent = translate(locale, "d1.dock.budget.spent", {
      cost: `$${(projection.environment.costMicroUsd / 1_000_000).toFixed(6)}`,
    });
    footer.append(spent);
  }
  wrapper.append(footer);
  body.append(wrapper);
}

/* ---- Files panel ---- */

/// Direct children of one prefix inside the page Core answered for it.
///
/// Core's prefix match covers the whole subtree, so a page for `crates/`
/// carries `crates/config` *and* `crates/config/src/lib.rs`. Only the first is
/// a row at this level; the deeper one appears when its own directory is
/// opened and gets its own page.
function directChildren(
  page: PaletteWorkspaceFiles | undefined,
  prefix: string,
): PaletteWorkspaceFiles["entries"] {
  if (!page) return [];
  return page.entries.filter((entry) => {
    if (!entry.path.startsWith(prefix)) return false;
    const rest = entry.path.slice(prefix.length);
    return rest.length > 0 && !rest.includes("/");
  });
}

function appendFileRows(
  host: HTMLElement,
  model: ContextDockModel,
  prefix: string,
  depth: number,
): void {
  const { files, locale } = model;
  for (const entry of directChildren(files.pages.get(prefix), prefix)) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = entry.kind === "dir" ? "fr dir" : "fr";
    row.dataset.fileRow = entry.path;
    row.dataset.fileKind = entry.kind;
    row.style.paddingInlineStart = `${depth * 12}px`;
    const expanded = files.expanded.has(entry.path);
    if (entry.kind === "dir") {
      row.setAttribute("aria-expanded", String(expanded));
      row.title = translate(
        locale,
        expanded ? "d1.dock.files.collapse" : "d1.dock.files.expand",
        { path: entry.path },
      );
      row.addEventListener("click", () => files.onToggleDirectory(entry.path));
    } else {
      row.title = entry.path;
      if (files.selectedPath === entry.path) row.dataset.fileSelected = "true";
      row.addEventListener("click", () => files.onSelect(entry.path));
    }
    const glyph = document.createElement("span");
    glyph.className = "g";
    glyph.setAttribute("aria-hidden", "true");
    glyph.textContent = entry.kind === "dir" ? (expanded ? "▾" : "▸") : "·";
    const label = document.createElement("span");
    label.className = "fn";
    // The row shows the leaf, because the tree already draws the ancestry; the
    // full workspace-relative path stays in `data-file-row` and the title.
    label.textContent = entry.path.slice(prefix.length);
    row.append(glyph, label);
    if (entry.sizeBytes !== null) {
      const size = document.createElement("span");
      size.className = "pm";
      size.textContent = `${entry.sizeBytes}`;
      row.append(size);
    }
    host.append(row);
    if (entry.kind === "dir" && expanded) {
      const childPrefix = `${entry.path}/`;
      const page = files.pages.get(childPrefix);
      if (!page) {
        host.append(
          emptyState("files-pending", translate(locale, "d1.dock.files.pending", {})),
        );
      } else {
        appendFileRows(host, model, childPrefix, depth + 1);
      }
    }
  }
}

/** The design's `.dkfiles` tree, one Core page per opened directory. */
export function renderFilesPanel(panel: HTMLElement, model: ContextDockModel): void {
  const { files, locale } = model;
  const scope = document.createElement("p");
  scope.className = "dkh";
  scope.dataset.filesScope = "true";
  // `WorkspaceFilesQuery` carries no target, so the inventory is always the
  // workspace root. Saying so is the difference between a scoped answer and a
  // wrong one.
  scope.textContent = translate(locale, "d1.dock.files.scope", {});
  panel.append(scope);

  if (!files.bound) {
    panel.append(emptyState("files-unbound", translate(locale, "d1.dock.unbound", {})));
    return;
  }
  if (files.capabilityAvailable === false) {
    panel.append(
      emptyState(
        "files-capability",
        translate(locale, "d1.dock.files.unavailable", {
          capability: "runtime.workspace_files",
        }),
      ),
    );
    return;
  }
  const root = files.pages.get("");
  if (root?.outcome.state === "rejected") {
    const rejected = document.createElement("p");
    rejected.className = "d1-empty";
    rejected.dataset.filesRejected = "true";
    rejected.setAttribute("role", "alert");
    rejected.textContent = translate(locale, "d1.dock.files.rejected", {
      reason: root.outcome.reason ?? "",
    });
    panel.append(rejected);
    return;
  }
  if (!root?.loaded) {
    panel.append(emptyState("files-pending", translate(locale, "d1.dock.files.pending", {})));
    return;
  }
  const tree = document.createElement("div");
  tree.className = "ftree";
  tree.dataset.fileTree = "true";
  appendFileRows(tree, model, "", 0);
  if (tree.childElementCount === 0) {
    panel.append(emptyState("files-empty", translate(locale, "d1.dock.files.empty", {})));
  } else {
    panel.append(tree);
  }
  // Core's own bound, stated where it happened. `complete: false` on any page
  // means that subtree continues past what Core sent.
  if (Array.from(files.pages.values()).some((page) => page.loaded && !page.complete)) {
    const truncated = document.createElement("p");
    truncated.className = "d1-empty";
    truncated.dataset.fileTruncated = "true";
    truncated.textContent = translate(locale, "d1.dock.files.truncated", {});
    panel.append(truncated);
  }
  renderInspector(panel, model, "files");
}

/* ---- Diff panel ---- */

/** The design's `.dkdiff`: one collapsed entry per changed file. */
export function renderDiffPanel(panel: HTMLElement, model: ContextDockModel): void {
  const { diff, locale } = model;
  if (!diff.bound) {
    panel.append(emptyState("diff-unbound", translate(locale, "d1.dock.unbound", {})));
    return;
  }
  if (diff.capabilityAvailable === false) {
    panel.append(
      emptyState(
        "diff-capability",
        translate(locale, "d1.dock.changes.unavailable", {
          capability: "runtime.structured_diff",
        }),
      ),
    );
    return;
  }
  if (diff.projection?.outcome.state === "rejected") {
    const rejected = document.createElement("p");
    rejected.className = "d1-empty";
    rejected.dataset.diffRejected = "true";
    rejected.setAttribute("role", "alert");
    rejected.textContent = translate(locale, "d1.dock.changes.rejected", {
      reason: diff.projection.outcome.reason ?? "",
    });
    panel.append(rejected);
    return;
  }
  if (!diff.projection?.loaded) {
    panel.append(emptyState("diff-pending", translate(locale, "d1.dock.changes.pending", {})));
    return;
  }
  if (diff.projection.entries.length === 0) {
    panel.append(emptyState("diff-empty", translate(locale, "d1.dock.changes.none", {})));
    return;
  }
  for (const entry of diff.projection.entries) {
    const item = document.createElement("div");
    item.className = "dkdiff";
    item.dataset.diffEntry = entry.path;
    if (diff.selectedPath === entry.path) item.dataset.diffSelected = "true";

    const head = document.createElement("div");
    head.className = "dkh";
    const toggle = document.createElement("button");
    toggle.type = "button";
    toggle.dataset.diffToggle = entry.path;
    const expanded = diff.expanded.has(entry.path);
    toggle.setAttribute("aria-expanded", String(expanded));
    toggle.title = translate(locale, "d1.dock.diff.expand", { path: entry.path });
    toggle.textContent = expanded ? "▾" : "▸";
    toggle.addEventListener("click", () => diff.onToggle(entry.path));

    const select = document.createElement("button");
    select.type = "button";
    select.className = "dkpath";
    select.dataset.diffSelect = entry.path;
    select.textContent = entry.path;
    select.addEventListener("click", () => diff.onSelect(entry.path));
    head.append(toggle, select);
    if (entry.diff) {
      const stat = document.createElement("span");
      stat.className = "ct";
      stat.textContent = `+${entry.diff.additions} −${entry.diff.deletions}`;
      head.append(stat);
    }
    item.append(head);
    // Collapsed by default: the panel answers "which files", and a wall of
    // rows in a 300px column answers nothing.
    if (expanded) renderDiffBody(item, entry.diff, locale);
    panel.append(item);
  }
  if (diff.projection.truncated) {
    panel.append(
      emptyState("diff-truncated", translate(locale, "d1.dock.changes.truncated", {})),
    );
  }
  renderInspector(panel, model, "diff");
}

/**
 * The design's Inspector, for exactly one selected path.
 *
 * Every action it offers is either a route the cockpit already owns or a
 * disabled control naming the Core command that does not exist: Core publishes
 * no per-file stage or revert, and file content needs
 * `runtime.workspace_file_reads`, which G7 consumes.
 */
export function renderInspector(
  panel: HTMLElement,
  model: ContextDockModel,
  source: "files" | "diff",
): void {
  const { locale } = model;
  const path = source === "files" ? model.files.selectedPath : model.diff.selectedPath;
  const inspector = document.createElement("section");
  inspector.className = "envsec";
  inspector.dataset.dockInspector = source;
  inspector.setAttribute("aria-label", translate(locale, "d1.dock.inspector", {}));
  const heading = document.createElement("h2");
  heading.className = "envhd";
  heading.textContent = translate(locale, "d1.dock.inspector", {});
  inspector.append(heading);

  if (!path) {
    inspector.append(emptyState("inspector", translate(locale, "d1.dock.inspector.none", {})));
    panel.append(inspector);
    return;
  }
  const facts = document.createElement("dl");
  definition(facts, translate(locale, "d1.dock.inspector.file", {}), path);
  if (source === "diff") {
    const entry = model.diff.projection?.entries.find((candidate) => candidate.path === path);
    definition(
      facts,
      translate(locale, "d1.dock.inspector.diff", {}),
      entry?.diff ? `+${entry.diff.additions} −${entry.diff.deletions}` : "—",
    );
  }
  inspector.append(facts);

  const actions = document.createElement("div");
  actions.className = "d1-dock-actions";
  if (source === "diff") {
    const review = document.createElement("button");
    review.type = "button";
    review.dataset.inspectorReview = "true";
    review.textContent = translate(locale, "d1.dock.inspector.open", {});
    review.addEventListener("click", () => model.diff.onOpenInReview(path));
    actions.append(review);
    for (const marker of ["stage", "revert"] as const) {
      const action = document.createElement("button");
      action.type = "button";
      action.dataset.inspectorAction = marker;
      action.disabled = true;
      action.setAttribute("aria-disabled", "true");
      action.title = translate(locale, "d1.dock.inspector.actionUnavailable", {});
      action.textContent = translate(
        locale,
        marker === "stage" ? "d1.dock.inspector.stage" : "d1.dock.inspector.revert",
        {},
      );
      actions.append(action);
    }
  } else {
    const open = document.createElement("button");
    open.type = "button";
    open.dataset.inspectorOpen = "true";
    open.textContent = translate(locale, "d1.dock.inspector.openFile", {});
    if (!model.files.fileReadsAvailable) {
      const why = translate(locale, "d1.dock.inspector.openUnavailable", {
        capability: "runtime.workspace_file_reads",
      });
      open.disabled = true;
      open.setAttribute("aria-disabled", "true");
      open.title = why;
      actions.append(open);
      // Visible, not only a tooltip: the reason a control is inert is a fact
      // about Core, and a fact nobody can read without hovering is one a
      // screenshot cannot check either.
      const reason = document.createElement("p");
      reason.className = "d1-dock-reason";
      reason.dataset.inspectorOpenReason = "true";
      reason.textContent = why;
      actions.append(reason);
    } else {
      actions.append(open);
    }
  }
  inspector.append(actions);
  panel.append(inspector);
}

/* ---- Environment panel ---- */

export function renderEnvironmentPanel(panel: HTMLElement, model: ContextDockModel): void {
  const { projection, locale } = model;
  // The Lane's own worktree source when C5 published one, the workspace sample
  // otherwise. Both are Core facts; which one is on screen is stated, never
  // implied.
  const source = model.laneSource ?? projection.contextDock.source;
  const scope: "lane" | "workspace" = model.laneSource ? "lane" : "workspace";

  appendSection(panel, "environment", translate(locale, "d1.environment", {}), (body) => {
    const environmentFacts = document.createElement("dl");
    const environmentLabels = [
      translate(locale, "d1.environment.provider", {}),
      translate(locale, "d1.environment.model", {}),
      translate(locale, "d1.environment.mode", {}),
      translate(locale, "d1.environment.permission", {}),
      translate(locale, "d1.environment.tokens", {}),
      translate(locale, "d1.environment.cost", {}),
    ];
    environmentValues(projection.environment).forEach((value, index) => {
      definition(environmentFacts, environmentLabels[index]!, value);
    });
    body.append(environmentFacts);
  });
  appendSection(panel, "changes", translate(locale, "d1.dock.changes", {}), (body) => {
    renderChangesSection(body, model, source);
  });
  const local = appendSection(panel, "local", translate(locale, "d1.dock.local", {}), (body) => {
    renderLocalSection(body, model, source, scope);
  });
  local.dataset.localScope = scope;
  appendSection(panel, "commit-or-push", translate(locale, "d1.dock.commit", {}), (body) => {
    renderCommitSection(body, model);
  });
  appendSection(panel, "pr-status", translate(locale, "d1.dock.pr", {}), (body) => {
    // No Core fact at all: frontend-contract-v1 publishes no forge status
    // (GUI-CORE-021), so the section states that rather than showing a
    // placeholder that would read as "no open PR".
    body.append(emptyState("pr-status", translate(locale, "d1.dock.pr.unavailable", {})));
  });
  appendSection(panel, "context", translate(locale, "d1.context.context", {}), (body) => {
    renderBudgetSection(body, model);
  });
  appendSection(panel, "subagents", translate(locale, "d1.dock.subagents", {}), (body) => {
    const deferred = document.createElement("p");
    deferred.className = "d1-empty";
    deferred.dataset.subagentsDeferred = "true";
    deferred.setAttribute("aria-disabled", "true");
    deferred.textContent = translate(locale, "d1.dock.subagents.deferred", {});
    body.append(deferred);
  });
  appendSection(panel, "sources", translate(locale, "d1.context.sources", {}), (body) => {
    if (!source) {
      body.append(emptyState("sources", translate(locale, "d1.context.noSource", {})));
      return;
    }
    const facts = document.createElement("dl");
    definition(facts, translate(locale, "d1.status.added", {}), String(source.added));
    definition(facts, translate(locale, "d1.status.deleted", {}), String(source.deleted));
    definition(facts, translate(locale, "d1.status.modified", {}), dirtyValue(locale, source.dirty));
    body.append(facts);
  });
  appendSection(panel, "lane-agent", translate(locale, "d1.context.laneAgent", {}), (body) => {
    renderLaneAgent(body, projection, locale);
  });
  appendSection(panel, "mcp", translate(locale, "d1.context.mcp", {}), (body) => {
    const services = projection.contextDock.services.filter((service) => service.kind === "mcp");
    if (services.length === 0) {
      // Core published no MCP row at all, which is not the same fact as a row
      // that says "unavailable"; the section states it rather than showing an
      // empty server list.
      body.append(emptyState("mcp", translate(locale, "d1.dock.mcp.unavailable", {})));
      return;
    }
    for (const service of services) {
      const item = document.createElement("div");
      item.className = "mcprow d1-context-item";
      item.dataset.serviceId = service.id;
      item.dataset.serviceStatus = service.status;
      const dot = document.createElement("span");
      dot.className = "dot";
      dot.setAttribute("aria-hidden", "true");
      const label = document.createElement("span");
      label.textContent = service.label;
      const status = document.createElement("span");
      status.className = "st";
      status.textContent = statusLabel(locale, service.status);
      item.append(dot, label, status);
      // Core's own `detail_key`, localized. An unmapped key renders nothing
      // rather than leaking a raw diagnostic identifier into the UI.
      const reasonKey = MCP_DETAIL_KEYS[service.detailKey ?? ""];
      if (reasonKey) {
        const reason = document.createElement("span");
        reason.className = "d1-dock-reason";
        reason.dataset.serviceReason = "true";
        reason.textContent = translate(locale, reasonKey, {});
        item.append(reason);
      }
      body.append(item);
    }
  });
  appendSection(panel, "lsp", translate(locale, "d1.context.lsp", {}), (body) => {
    const services = projection.contextDock.services.filter((service) => service.kind === "lsp");
    if (services.length === 0) {
      body.append(emptyState("lsp", translate(locale, "d1.context.noServices", {})));
      return;
    }
    for (const service of services) {
      const item = document.createElement("div");
      item.className = "d1-context-item";
      item.dataset.serviceId = service.id;
      item.textContent = `${service.label} · ${statusLabel(locale, service.status)}`;
      body.append(item);
    }
  });
  appendSection(
    panel,
    "task-checklist",
    translate(locale, "d1.context.taskChecklist", {}),
    (body) => {
      if (projection.contextDock.checklist.length === 0) {
        body.append(emptyState("task-checklist", translate(locale, "d1.context.noChecklist", {})));
        return;
      }
      for (const item of projection.contextDock.checklist) {
        const row = document.createElement("div");
        row.className = "d1-context-item";
        row.dataset.checklistItem = item.id;
        row.textContent = `${item.label} · ${statusLabel(locale, item.status)}`;
        body.append(row);
      }
    },
  );
  for (const { text } of boundedLiveWorkEntries(projection, locale)) {
    const item = document.createElement("div");
    item.className = "d1-work-item";
    item.textContent = text;
    panel.append(item);
  }
  for (const unavailable of projection.unavailableFeatures) {
    const item = document.createElement("div");
    item.className = "d1-unavailable";
    item.dataset.unavailableFeature = unavailable.id;
    item.setAttribute("aria-disabled", "true");
    item.textContent = `${translate(locale, "d1.unavailable", {})} · ${unavailable.id}`;
    panel.append(item);
  }
}

/**
 * The one place that decides what a dock tab draws.
 *
 * A later batch adds a case rather than a second switch somewhere else; the
 * three unopenable tabs never reach here, because the strip refuses to select
 * them.
 */
export function renderDockTab(panel: HTMLElement, model: ContextDockModel): void {
  switch (model.tab) {
    case "files":
      renderFilesPanel(panel, model);
      return;
    case "diff":
      renderDiffPanel(panel, model);
      return;
    default:
      renderEnvironmentPanel(panel, model);
  }
}

export function renderContextDock(model: ContextDockModel): HTMLElement {
  const { locale } = model;
  const dock = document.createElement("aside");
  dock.id = "d1-context-dock";
  dock.className = "rail dock d1-right";
  dock.dataset.shellLandmark = "context-dock";
  dock.dataset.cockpitRole = "context";
  dock.dataset.contextDock = "true";
  dock.dataset.dockTab = model.tab;
  dock.dataset.drawerOpen = "false";
  if (!model.matchesSelectedLane) {
    const waiting = document.createElement("p");
    waiting.dataset.contextDockWaiting = "true";
    waiting.textContent = translate(locale, "d1.context.switching", {});
    dock.append(waiting);
    return dock;
  }

  const strip = document.createElement("div");
  strip.className = "docktabs";
  strip.dataset.dockTabs = "true";
  strip.setAttribute("role", "tablist");
  strip.setAttribute("aria-label", translate(locale, "d1.dock.tabs", {}));
  const list = document.createElement("div");
  list.className = "dtablist";
  for (const definitionEntry of DOCK_TABS) {
    const tab = document.createElement("button");
    tab.type = "button";
    tab.className = "dtab";
    tab.dataset.dockTab = definitionEntry.id;
    tab.setAttribute("role", "tab");
    const label = translate(locale, definitionEntry.labelKey, {});
    tab.textContent = label;
    if (definitionEntry.live) {
      const current = definitionEntry.id === model.tab;
      tab.setAttribute("aria-selected", String(current));
      tab.setAttribute("aria-controls", "d1-dock-body");
      if (current) tab.classList.add("on");
      tab.title = definitionEntry.chordKey
        ? `${label} ${translate(locale, definitionEntry.chordKey, {})}`
        : label;
      tab.addEventListener("click", () => model.onSelectTab(definitionEntry.id));
    } else {
      // Never hidden: a tab that vanishes makes the dock look finished. It
      // carries the exact reason it cannot open instead.
      tab.disabled = true;
      tab.setAttribute("aria-disabled", "true");
      tab.setAttribute("aria-selected", "false");
      tab.dataset.dockTabUnavailable = "true";
      tab.title = translate(locale, definitionEntry.reasonKey!, {});
    }
    list.append(tab);
  }
  strip.append(list);

  const panel = document.createElement("div");
  panel.id = "d1-dock-body";
  panel.className = model.tab === "environment" ? "dockbody envp" : "dockbody";
  panel.dataset.dockBody = model.tab;
  panel.setAttribute("role", "tabpanel");
  panel.setAttribute("aria-label", translate(locale, dockTabLabelKey(model.tab), {}));
  renderDockTab(panel, model);

  dock.append(strip, panel);
  return dock;
}

function dockTabLabelKey(tab: DockTab): MessageKey {
  return DOCK_TABS.find((candidate) => candidate.id === tab)!.labelKey;
}
