import { translate, type Locale, type MessageKey } from "../i18n/catalog";
import { STRUCTURED_DIFF_CAPABILITY } from "../models/diff_review";
import { EVIDENCE_READS_CAPABILITY } from "../models/evidence";
import type { D1CockpitProjection } from "../models/workspace";
import { createCanonicalGuiIcon, type CanonicalGuiIcon } from "./activity_rail";
import "./command_palette.css";

/**
 * The ⌘K command palette.
 *
 * Visual vocabulary: the registered design component in
 * `docs/viden-design/Viden/GUI/Viden - 桌面驾驶舱 (GUI).html` (`scrim top` /
 * `palette` / `palin` / `palsec` / `palrow` / `pic` / `pl` / `pk`), expressed
 * with shared tokens only.
 *
 * Query grammar and ranking: a deliberate port of the TUI jump index
 * (`apps/tui/src/tui/jump.rs`). The sigils (`:` lanes, `@` sessions, `#` gates
 * and asks, `>` commands, `~` files) and the subsequence scorer behave exactly
 * as they do in the terminal client, so the selector language is one language
 * across frontends rather than two dialects.
 *
 * Contract position: the palette invents no fact. Lanes and Agent sessions come
 * from the D1 projection the cockpit already holds; cross-Lane gates and asks
 * come from an eager read of the existing D2/D12 Core projections, handed in by
 * the shell. A read that fails degrades to a stated note — it never blocks the
 * palette, and it is never replaced by a guess.
 */

/** Selector groups, one-to-one with the TUI's `JumpKind`. */
export type PaletteKind = "gate" | "ask" | "lane" | "session" | "command" | "file";

/**
 * Visual grouping, from the design's `['sec', …]` headers. Presentation only:
 * the query grammar is scoped by `kind`, which is why the Settings row stays a
 * `command` and remains reachable under `>`.
 */
export type PaletteSection = "actions" | "jump" | "settings" | "files";

export interface PaletteQuery {
  text: string;
  /** Null means "every kind", matching the TUI's unscoped query. */
  kinds: PaletteKind[] | null;
}

/**
 * Parses one palette query. Ported from `JumpQuery::parse`: a leading sigil
 * scopes the kinds and is stripped, and the remainder is trimmed.
 */
export function parsePaletteQuery(value: string): PaletteQuery {
  const sigil = value.slice(0, 1);
  const scopes: Partial<Record<string, PaletteKind[]>> = {
    ":": ["lane"],
    "@": ["session"],
    "#": ["gate", "ask"],
    ">": ["command"],
    "~": ["file"],
  };
  const kinds = scopes[sigil] ?? null;
  return { text: (kinds ? value.slice(1) : value).trim(), kinds };
}

/**
 * Returns a subsequence match score, or null when a character is missing.
 *
 * Ported from `fuzzy_subsequence_score`: the score accumulates each matched
 * index and subtracts two for a match adjacent to the previous one. Callers use
 * only presence or absence today, which preserves the stable section ordering
 * the design draws rather than re-ranking rows under the operator's cursor.
 */
export function fuzzySubsequenceScore(query: string, candidate: string): number | null {
  const wanted = Array.from(query.toLowerCase());
  if (wanted.length === 0) return null;
  const haystack = Array.from(candidate.toLowerCase());
  let cursor = 0;
  let score = 0;
  let previous: number | null = null;
  for (let index = 0; index < haystack.length; index += 1) {
    if (haystack[index] !== wanted[cursor]) continue;
    score += index;
    // `index.saturating_sub(1)` in the Rust original, so index 0 compares
    // against 0 and can never claim adjacency against an unset predecessor.
    if (previous !== null && previous === Math.max(0, index - 1)) {
      score = Math.max(0, score - 2);
    }
    previous = index;
    cursor += 1;
    if (cursor === wanted.length) return score;
  }
  return null;
}

export interface PaletteItem {
  kind: PaletteKind;
  section: PaletteSection;
  id: string;
  title: string;
  /** Secondary line: the Lane's role and status, a session's Lane, and so on. */
  context: string;
  /** Extra fuzzy-match surface that is not itself drawn. */
  keywords: string;
  /** The design's `.pk` keyboard hint, or null when there is no binding. */
  hint: string | null;
  enabled: boolean;
  /** Why an operator cannot act on this row. Rendered in place of the hint. */
  disabledReason: string | null;
  activate: (() => void) | null;
  /** Registered design-kit glyph for the `.pic` slot. */
  icon?: CanonicalGuiIcon;
}

/**
 * Cross-Lane decisions read eagerly when the palette opens.
 *
 * The cockpit's own D1 projection is Lane-scoped, so gates and asks belonging
 * to other Lanes are not in it. They come from the existing `d2_decisions` and
 * `d12_integration_gate` Core reads — no new Core capability is involved.
 */
export interface PaletteCrossLane {
  /**
   * Ordered by the D12 projection, which puts gates awaiting a live session
   * ahead of `dormant` ones left behind by a finished session. The palette
   * preserves that order and tags the dormant tail rather than dropping it:
   * every gate stays reachable from `#`.
   */
  gates: Array<{ gateId: string; taskId: string; status: string; dormant: boolean }>;
  asks: Array<{ id: string; title: string; kind: string; laneId: string | null }>;
  /** Core's own words for a read that failed, or null when it succeeded. */
  unavailable: string | null;
}

/**
 * The Core workspace file inventory the palette's `~` scope lists
 * (GUI-CORE-022).
 *
 * Mirrors the host's `WorkspacesFilesProjection` field for field. The client
 * must never produce these paths itself: walking the workspace is outside the
 * client boundary and bypasses the permission gate every other path read
 * passes, so a row exists only because Core published it.
 *
 * `capabilityAvailable`, `loaded`, and an empty `entries` are three different
 * facts — "Core publishes no inventory", "the read has not answered yet", and
 * "this workspace is empty" — and each gets its own row.
 */
export interface PaletteWorkspaceFiles {
  outcome: { state: string; reason: string | null };
  /** Lexicographic by path, exactly as Core delivered. */
  entries: Array<{ path: string; kind: string; sizeBytes: number | null }>;
  complete: boolean;
  loaded: boolean;
  pendingCommandId: string | null;
  capabilityAvailable: boolean;
}

export interface CommandPaletteModel {
  locale: Locale;
  projection: D1CockpitProjection;
  /** Seed query. `⌃P` opens the palette pre-scoped with `>`. */
  query: string;
  /** Null while the cross-Lane read is still in flight. */
  crossLane: PaletteCrossLane | null;
  /** Null while the shell has bound no inventory read at all. */
  files: PaletteWorkspaceFiles | null;
  /** False while the shell has bound no router, which omits the screen rows. */
  canNavigate: boolean;
  canOpenSettings: boolean;
  /**
   * Whether Core says this workspace can carry a new Lane
   * (`workspaceEligibility.canCreateLane`), and Core's own reason when it
   * cannot. `null` eligibility is a third state — Core published none — and
   * gets its own sentence rather than being folded into "no".
   */
  laneCreation: { available: boolean; reason: string | null };
  canFocusComposer: boolean;
  canCancelTurn: boolean;
  /** False while no host is bound, which omits the review row entirely. */
  reviewBound: boolean;
  /**
   * Whether Core published `runtime.structured_diff`. A bound host without the
   * capability keeps the row visible and disabled naming it (GUI-CORE-012),
   * the way the `~` file scope names GUI-CORE-022.
   */
  reviewAvailable: boolean;
  /** False while no host is bound, which omits the evidence row entirely. */
  evidenceBound: boolean;
  /**
   * Whether Core published `runtime.evidence_reads`. A bound host without the
   * capability keeps the row visible and disabled naming it, the same honesty
   * the review row and the `~` file scope ship.
   */
  evidenceAvailable: boolean;
  /**
   * Where focus goes on close when the palette was not opened from a focused
   * element (a shortcut fired with nothing focused). Re-resolved on close, so a
   * cockpit refresh that rebuilt the titlebar cannot strand focus on a detached
   * node.
   */
  returnFocus?: HTMLElement | null;
}

export interface CommandPaletteHandlers {
  /** Opens a restored screen, optionally preselecting one Core id. */
  onNavigate?: (route: string, arg?: string) => void;
  onSelectLane?: (laneId: string) => void;
  onOpenSettings?: () => void;
  onFocusComposer?: () => void;
  onCancelTurn?: () => void;
  /** Opens the New Lane popover, the one creation surface the cockpit has. */
  onCreateLane?: () => void;
  /** Opens the DiffReview view in the cockpit's centre pane. */
  onOpenReview?: () => void;
  /** Opens the EvidenceView in the cockpit's centre pane. */
  onOpenEvidence?: () => void;
  /** Keeps the operator's query in cockpit state across a forced remount. */
  onQueryChange?: (query: string) => void;
  onClose: () => void;
}

export interface CommandPaletteController {
  root: HTMLElement;
  /** Operator dismissal: removes the overlay, restores focus, reports close. */
  close: () => void;
  /** Removal without a dismissal: no focus move and no `onClose`. */
  dispose: () => void;
  setQuery: (query: string) => void;
  setCrossLane: (crossLane: PaletteCrossLane) => void;
  setFiles: (files: PaletteWorkspaceFiles) => void;
}

/** Screens the shell can restore, in the order the design's rail lists them. */
const NAVIGABLE_SCREENS: Array<{ route: string; icon: CanonicalGuiIcon }> = [
  { route: "d2", icon: "decide" },
  { route: "d4", icon: "lanes" },
  { route: "d10", icon: "diagnostics" },
  { route: "d11", icon: "evidence" },
  { route: "d12", icon: "worktree" },
  { route: "d13", icon: "inbox" },
  { route: "d14", icon: "evidence" },
];

const SCREEN_LABELS: Record<string, MessageKey> = {
  d2: "d1.palette.action.d2",
  d4: "d1.palette.action.d4",
  d10: "d1.palette.action.d10",
  d11: "d1.palette.action.d11",
  d12: "d1.palette.action.d12",
  d13: "d1.palette.action.d13",
  d14: "d1.palette.action.d14",
};

/**
 * Builds the palette index from Core facts only.
 *
 * An action whose capability the shell did not supply is omitted rather than
 * rendered inert; a declared *contract* gap (no file inventory, a failed
 * cross-Lane read) is rendered as a disabled row that states its reason, which
 * is the same honesty the TUI jump index ships.
 */
export function paletteItems(
  model: CommandPaletteModel,
  handlers: CommandPaletteHandlers,
): PaletteItem[] {
  const { locale, projection } = model;
  const items: PaletteItem[] = [];
  const enabled = (
    item: Omit<PaletteItem, "enabled" | "disabledReason">,
  ): PaletteItem => ({ ...item, enabled: true, disabledReason: null });
  const disabled = (
    item: Omit<PaletteItem, "enabled" | "disabledReason" | "activate">,
    reason: string,
  ): PaletteItem => ({ ...item, enabled: false, disabledReason: reason, activate: null });

  /* ---- Actions ---- */

  if (model.canFocusComposer && handlers.onFocusComposer) {
    items.push(
      enabled({
        kind: "command",
        section: "actions",
        id: "action:focus-composer",
        title: translate(locale, "d1.palette.action.focusComposer", {}),
        context: "",
        keywords: "composer prompt input",
        hint: null,
        icon: "chat",
        activate: () => handlers.onFocusComposer?.(),
      }),
    );
  }
  if (handlers.onCreateLane) {
    // The design's own row and label: "Delegate task to new worktree… ⌘L".
    // Creating a Lane *is* delegating a task to a fresh worktree, which is why
    // the design names it that way; the row opens the same New Lane popover
    // the Lane rail's and the tab strip's `＋` open rather than being a second
    // creation path.
    const title = translate(locale, "d1.palette.action.newLane", {});
    const keywords = "lane delegate worktree new create task";
    items.push(
      model.laneCreation.available
        ? enabled({
            kind: "command",
            section: "actions",
            id: "action:new-lane",
            title,
            context: "",
            keywords,
            hint: "⌘L",
            icon: "lanes",
            activate: () => handlers.onCreateLane?.(),
          })
        : disabled(
            {
              kind: "command",
              section: "actions",
              id: "action:new-lane",
              title,
              context: "",
              keywords,
              hint: null,
              icon: "lanes",
            },
            // Core's own diagnostic when it gave one; otherwise the fact that
            // it published no eligibility at all, which is not the same as a
            // refusal and must not be worded like one.
            model.laneCreation.reason ??
              translate(locale, "d1.palette.action.newLane.unknown", {}),
          ),
    );
  }
  if (model.canCancelTurn && handlers.onCancelTurn) {
    items.push(
      enabled({
        kind: "command",
        section: "actions",
        id: "action:cancel-turn",
        title: translate(locale, "d1.palette.action.cancelTurn", {}),
        context: "",
        keywords: "stop abort turn",
        hint: "Esc",
        icon: "diagnostics",
        activate: () => handlers.onCancelTurn?.(),
      }),
    );
  }
  if (model.reviewBound && handlers.onOpenReview) {
    const title = translate(locale, "d1.palette.action.openReview", {});
    items.push(
      model.reviewAvailable
        ? enabled({
            kind: "command",
            section: "actions",
            id: "action:open-review",
            title,
            context: "",
            keywords: "diff review changes git staged",
            hint: "⌘R",
            icon: "review",
            activate: () => handlers.onOpenReview?.(),
          })
        : disabled(
            {
              kind: "command",
              section: "actions",
              id: "action:open-review",
              title,
              context: "",
              keywords: "diff review changes git staged",
              hint: null,
              icon: "review",
            },
            translate(locale, "d1.review.entryUnavailable", {
              capability: STRUCTURED_DIFF_CAPABILITY,
            }),
          ),
    );
  }
  if (model.evidenceBound && handlers.onOpenEvidence) {
    const title = translate(locale, "d1.palette.action.openEvidence", {});
    items.push(
      model.evidenceAvailable
        ? enabled({
            kind: "command",
            section: "actions",
            id: "action:open-evidence",
            title,
            context: "",
            keywords: "evidence archive patch test review artifact",
            hint: "⌘E",
            icon: "evidence",
            activate: () => handlers.onOpenEvidence?.(),
          })
        : disabled(
            {
              kind: "command",
              section: "actions",
              id: "action:open-evidence",
              title,
              context: "",
              keywords: "evidence archive patch test review artifact",
              hint: null,
              icon: "evidence",
            },
            translate(locale, "d1.evidence.entryUnavailable", {
              capability: EVIDENCE_READS_CAPABILITY,
            }),
          ),
    );
  }
  if (model.canNavigate && handlers.onNavigate) {
    for (const screen of NAVIGABLE_SCREENS) {
      items.push(
        enabled({
          kind: "command",
          section: "actions",
          id: `action:navigate:${screen.route}`,
          title: translate(locale, SCREEN_LABELS[screen.route]!, {}),
          context: screen.route.toUpperCase(),
          keywords: screen.route,
          hint: null,
          icon: screen.icon,
          activate: () => handlers.onNavigate?.(screen.route),
        }),
      );
    }
  }

  /* ---- Jump to ---- */

  const crossLane = model.crossLane;
  if (!crossLane) {
    items.push(
      disabled(
        {
          kind: "gate",
          section: "jump",
          id: "jump:cross-lane-pending",
          title: translate(locale, "d1.palette.crossLane.pending", {}),
          context: "",
          keywords: "gate ask decision approval",
          hint: null,
          icon: "decide",
        },
        translate(locale, "d1.palette.crossLane.pending", {}),
      ),
    );
  } else if (crossLane.unavailable) {
    items.push(
      disabled(
        {
          kind: "gate",
          section: "jump",
          id: "jump:cross-lane-unavailable",
          title: translate(locale, "d1.palette.crossLane.unavailable", {}),
          context: crossLane.unavailable,
          keywords: "gate ask decision approval",
          hint: null,
          icon: "decide",
        },
        crossLane.unavailable,
      ),
    );
  } else {
    for (const gate of crossLane.gates) {
      items.push(
        enabled({
          kind: "gate",
          section: "jump",
          id: `gate:${gate.gateId}`,
          title: translate(locale, "d1.palette.gate", { gate: gate.gateId }),
          // A dormant gate stays in the `#` scope and stays activatable; the
          // tag says why it is at the bottom, so the operator is never left
          // wondering where a gate went.
          context: gate.dormant
            ? `${gate.taskId} · ${gate.status} · ${translate(locale, "d1.palette.gate.dormant", {})}`
            : `${gate.taskId} · ${gate.status}`,
          keywords: `${gate.taskId} ${gate.status}${gate.dormant ? " dormant" : ""}`,
          hint: null,
          icon: "worktree",
          // D12 owns the gate; the palette hands it the exact Core gate id and
          // the screen re-reads its own projection before it renders.
          activate: () => handlers.onNavigate?.("d12", gate.gateId),
        }),
      );
    }
    for (const ask of crossLane.asks) {
      items.push(
        enabled({
          kind: "ask",
          section: "jump",
          id: `ask:${ask.id}`,
          title: ask.title,
          context: [ask.kind, ask.laneId].filter(Boolean).join(" · "),
          keywords: `${ask.id} ${ask.kind}`,
          hint: null,
          icon: "decide",
          activate: () => handlers.onNavigate?.("d2", ask.id),
        }),
      );
    }
  }

  for (const lane of projection.lanes) {
    items.push(
      enabled({
        kind: "lane",
        section: "jump",
        id: `lane:${lane.id}`,
        title: lane.id,
        context: `${lane.role} · ${lane.status}`,
        keywords: `${lane.summary} ${lane.branch ?? ""}`,
        hint: null,
        icon: "lanes",
        activate: () => handlers.onSelectLane?.(lane.id),
      }),
    );
  }
  for (const session of projection.agentSessions) {
    items.push(
      enabled({
        kind: "session",
        section: "jump",
        id: `session:${session.sessionId}`,
        title: session.sessionId,
        context: `${session.laneId} · ${session.agentId} · ${session.status}`,
        keywords: session.task,
        hint: null,
        icon: "chat",
        // A session is reached through its Lane: the cockpit's focused
        // conversation follows the Lane selection Core published for it.
        activate: () => handlers.onSelectLane?.(session.laneId),
      }),
    );
  }

  /* ---- Settings ---- */

  if (model.canOpenSettings && handlers.onOpenSettings) {
    items.push(
      enabled({
        kind: "command",
        section: "settings",
        id: "action:open-settings",
        title: translate(locale, "d1.palette.action.openSettings", {}),
        context: "",
        keywords: "settings preferences language appearance skin mode density",
        hint: null,
        icon: "settings",
        activate: () => handlers.onOpenSettings?.(),
      }),
    );
  }

  /* ---- Files ---- */

  // Every row here comes from a page Core published, or there is no row: the
  // client never walks the workspace, shells out to a file lister, or
  // reconstructs a tree from paths that appear in evidence or tool previews.
  // A partial inventory presented as an inventory is worse than a stated gap,
  // so each distinct fact keeps its own disabled row rather than collapsing
  // into an empty list. Mirrors the TUI jump index one-for-one.
  const files = model.files;
  const filesRow = (id: string, title: string, reason: string) =>
    disabled(
      {
        kind: "file",
        section: "files",
        id,
        title,
        context: "",
        keywords: "file files path search",
        hint: null,
        icon: "evidence",
      },
      reason,
    );
  if (!files || !files.capabilityAvailable) {
    items.push(
      filesRow(
        "file:core-file-inventory-unavailable",
        translate(locale, "d1.palette.files.unavailable", {}),
        translate(locale, "d1.palette.files.reason", {}),
      ),
    );
  } else if (files.outcome.state === "rejected") {
    // Core's own words for the refusal — a permission denial must reach the
    // operator as Core wrote it, never as a locally composed sentence.
    items.push(
      filesRow(
        "file:core-file-inventory-rejected",
        translate(locale, "d1.palette.files.unavailable", {}),
        files.outcome.reason ?? translate(locale, "d1.palette.files.reason", {}),
      ),
    );
  } else if (!files.loaded) {
    items.push(
      filesRow(
        "file:core-file-inventory-loading",
        translate(locale, "d1.palette.files.pending", {}),
        translate(locale, "d1.palette.files.pendingReason", {}),
      ),
    );
  } else if (files.entries.length === 0) {
    items.push(
      filesRow(
        "file:core-file-inventory-empty",
        translate(locale, "d1.palette.files.empty", {}),
        translate(locale, "d1.palette.files.emptyReason", {}),
      ),
    );
  } else {
    for (const entry of files.entries) {
      items.push(
        enabled({
          kind: "file",
          section: "files",
          id: `file:${entry.path}`,
          title: entry.path,
          context: entry.kind,
          keywords: entry.path,
          hint: null,
          icon: "evidence",
          // Selecting a file closes the palette. `frontend-contract-v1`
          // publishes no "open this path" command, so activating a row must
          // not pretend to open an editor the client does not have.
          activate: () => undefined,
        }),
      );
    }
  }

  return items;
}

/** Filters the index by one query, preserving the design's section order. */
export function searchPalette(items: PaletteItem[], query: string): PaletteItem[] {
  const parsed = parsePaletteQuery(query);
  return items.filter((item) => {
    if (parsed.kinds && !parsed.kinds.includes(item.kind)) return false;
    if (parsed.text.length === 0) return true;
    return (
      fuzzySubsequenceScore(parsed.text, item.title) !== null ||
      fuzzySubsequenceScore(parsed.text, item.context) !== null ||
      fuzzySubsequenceScore(parsed.text, item.keywords) !== null
    );
  });
}

const SECTION_LABELS: Record<PaletteSection, MessageKey> = {
  actions: "d1.palette.section.actions",
  jump: "d1.palette.section.jump",
  settings: "d1.palette.section.settings",
  files: "d1.palette.section.files",
};

export function renderCommandPalette(
  host: HTMLElement,
  model: CommandPaletteModel,
  handlers: CommandPaletteHandlers,
): CommandPaletteController {
  const { locale } = model;
  let crossLane = model.crossLane;
  let files = model.files;
  let query = model.query;
  let highlighted = 0;
  let visible: PaletteItem[] = [];

  // The element that had focus when the palette took it. A shortcut fired with
  // nothing focused leaves this at the body, in which case the model's own
  // return target (the titlebar toggle) is used instead.
  const openedFrom =
    document.activeElement instanceof HTMLElement && document.activeElement !== document.body
      ? document.activeElement
      : null;

  const scrim = document.createElement("div");
  scrim.className = "scrim top gpal-scrim";
  scrim.dataset.commandPaletteScrim = "true";

  const panel = document.createElement("div");
  panel.className = "palette gpal-panel";
  panel.dataset.commandPalette = "true";
  panel.setAttribute("role", "dialog");
  panel.setAttribute("aria-modal", "true");
  panel.setAttribute("aria-label", translate(locale, "d1.palette.title", {}));

  const bar = document.createElement("div");
  bar.className = "palin";
  const sigil = document.createElement("span");
  sigil.className = "pr";
  sigil.ariaHidden = "true";
  sigil.textContent = "⌘";
  const field = document.createElement("input");
  field.type = "text";
  field.className = "q";
  field.dataset.paletteInput = "true";
  field.value = query;
  field.autocomplete = "off";
  field.spellcheck = false;
  field.placeholder = translate(locale, "d1.palette.placeholder", {});
  field.setAttribute("role", "combobox");
  field.setAttribute("aria-expanded", "true");
  field.setAttribute("aria-autocomplete", "list");
  field.setAttribute("aria-label", translate(locale, "d1.palette.inputLabel", {}));
  const escape = document.createElement("span");
  escape.className = "esc";
  escape.textContent = translate(locale, "d1.palette.escape", {});
  bar.append(sigil, field, escape);

  const list = document.createElement("div");
  list.className = "pallist gpal-list";
  list.id = "gpal-list";
  list.dataset.paletteList = "true";
  list.setAttribute("role", "listbox");
  list.setAttribute("aria-label", translate(locale, "d1.palette.title", {}));
  field.setAttribute("aria-controls", list.id);

  panel.append(bar, list);
  scrim.append(panel);

  function activate(item: PaletteItem): void {
    if (!item.enabled || !item.activate) return;
    // Close first, so the action lands on a cockpit that already owns focus
    // again rather than fighting the overlay for it.
    controller.close();
    item.activate();
  }

  function refresh(): void {
    const items = paletteItems({ ...model, query, crossLane, files }, handlers);
    visible = searchPalette(items, query);
    const selectable = visible.filter((item) => item.enabled);
    if (selectable.length === 0) highlighted = -1;
    else {
      const current = visible[highlighted];
      const index = current?.enabled ? highlighted : visible.indexOf(selectable[0]!);
      highlighted = index;
    }

    list.replaceChildren();
    let section: PaletteSection | null = null;
    for (const [index, item] of visible.entries()) {
      if (item.section !== section) {
        section = item.section;
        const heading = document.createElement("div");
        heading.className = "palsec";
        heading.dataset.paletteSection = section;
        // Presentational: the rows themselves carry the listbox semantics.
        heading.setAttribute("role", "presentation");
        heading.textContent = translate(locale, SECTION_LABELS[section], {});
        list.append(heading);
      }
      const row = document.createElement("div");
      row.className = "palrow";
      row.id = `gpal-row-${index}`;
      row.dataset.paletteRow = "true";
      row.dataset.paletteItemId = item.id;
      row.setAttribute("role", "option");
      row.setAttribute("aria-selected", String(index === highlighted));
      if (index === highlighted) row.classList.add("on");
      if (!item.enabled) {
        row.classList.add("off");
        row.setAttribute("aria-disabled", "true");
      }
      const icon = document.createElement("span");
      icon.className = "pic";
      icon.ariaHidden = "true";
      if (item.icon) icon.append(createCanonicalGuiIcon(item.icon));
      const label = document.createElement("span");
      label.className = "pl";
      label.textContent = item.title;
      row.append(icon, label);
      const trailing = item.disabledReason ?? item.context;
      if (trailing) {
        const note = document.createElement("span");
        note.className = item.disabledReason ? "pn" : "pc";
        note.textContent = trailing;
        // The row is one line, so a long contract reason ellipsizes; the full
        // sentence stays reachable rather than being cut off for good.
        note.title = trailing;
        row.append(note);
      }
      if (item.hint) {
        const hint = document.createElement("span");
        hint.className = "pk";
        const key = document.createElement("kbd");
        key.textContent = item.hint;
        hint.append(key);
        row.append(hint);
      }
      row.addEventListener("click", () => {
        if (!item.enabled) return;
        highlighted = index;
        activate(item);
      });
      list.append(row);
    }

    if (visible.length === 0) {
      const empty = document.createElement("p");
      empty.dataset.paletteEmpty = "true";
      empty.setAttribute("role", "status");
      empty.textContent = translate(locale, "d1.palette.empty", {});
      list.append(empty);
    }

    const active = highlighted >= 0 ? list.querySelector(`#gpal-row-${highlighted}`) : null;
    if (active) field.setAttribute("aria-activedescendant", active.id);
    else field.removeAttribute("aria-activedescendant");
  }

  function move(delta: number): void {
    const selectable = visible
      .map((item, index) => ({ item, index }))
      .filter(({ item }) => item.enabled);
    if (selectable.length === 0) return;
    const current = selectable.findIndex(({ index }) => index === highlighted);
    const next = (current + delta + selectable.length) % selectable.length;
    highlighted = selectable[next]!.index;
    refresh();
    list.querySelector(`#gpal-row-${highlighted}`)?.scrollIntoView?.({ block: "nearest" });
  }

  field.addEventListener("input", () => {
    query = field.value;
    highlighted = 0;
    handlers.onQueryChange?.(query);
    refresh();
  });

  field.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      // The cockpit binds a window-level Escape to "cancel the running turn".
      // Dismissing an overlay must never also cancel Core work, so the palette
      // consumes its own Escape instead of letting it reach that binding.
      event.stopPropagation();
      controller.close();
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      move(1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      move(-1);
    } else if (event.key === "Home") {
      event.preventDefault();
      highlighted = -1;
      move(1);
    } else if (event.key === "End") {
      event.preventDefault();
      highlighted = -1;
      move(-1);
    } else if (event.key === "Enter") {
      const item = visible[highlighted];
      if (!item) return;
      event.preventDefault();
      activate(item);
    }
  });

  scrim.addEventListener("click", (event) => {
    if (event.target !== scrim) return;
    controller.close();
  });

  let closed = false;
  const remove = (): void => {
    scrim.remove();
  };

  /// Re-resolves the return target. The cockpit rebuilds its titlebar on every
  /// Core refresh, so the toggle recorded at open time may already be detached.
  const liveReturnFocus = (): HTMLElement | null => {
    const preferred = openedFrom ?? model.returnFocus ?? null;
    if (preferred?.isConnected) return preferred;
    return document.querySelector<HTMLElement>("[data-command-palette-toggle]");
  };

  const controller: CommandPaletteController = {
    root: panel,
    close: () => {
      if (closed) return;
      closed = true;
      remove();
      // The close is reported first: the cockpit redraws its titlebar when the
      // palette closes, so the toggle that should take focus back is the one
      // that exists after that redraw, not the one recorded at open time.
      handlers.onClose();
      liveReturnFocus()?.focus();
    },
    dispose: () => {
      closed = true;
      remove();
    },
    setQuery: (next) => {
      query = next;
      field.value = next;
      highlighted = 0;
      handlers.onQueryChange?.(next);
      refresh();
      field.focus();
    },
    setCrossLane: (next) => {
      crossLane = next;
      refresh();
    },
    setFiles: (next) => {
      files = next;
      refresh();
    },
  };

  host.append(scrim);
  refresh();
  field.focus();
  field.setSelectionRange(query.length, query.length);
  return controller;
}
