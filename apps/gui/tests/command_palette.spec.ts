// @vitest-environment jsdom

/**
 * The ⌘K command palette component.
 *
 * The query grammar and the fuzzy scorer are a deliberate port of the TUI jump
 * index (`apps/tui/src/tui/jump.rs`), so an operator who learned `:`/`@`/`#`/
 * `>`/`~` in the terminal gets the same selectors in the desktop client. These
 * tests pin that parity, the honest-disabled Files row, and the fail-soft
 * cross-Lane section.
 */

import { afterEach, describe, expect, test, vi } from "vitest";

import {
  fuzzySubsequenceScore,
  paletteItems,
  parsePaletteQuery,
  renderCommandPalette,
  searchPalette,
  type CommandPaletteHandlers,
  type CommandPaletteModel,
  type PaletteCrossLane,
  type PaletteItem,
  type PaletteWorkspaceFiles,
} from "../src/components/command_palette";
import type { D1CockpitProjection } from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

const CROSS_LANE: PaletteCrossLane = {
  gates: [{ gateId: "gate-1", taskId: "task-core", status: "blocked", dormant: false }],
  asks: [{ id: "approval-shell", title: "Allow test", kind: "approval", laneId: "lane-core" }],
  unavailable: null,
};

const FILES: PaletteWorkspaceFiles = {
  outcome: { state: "confirmed", reason: null },
  entries: [
    { path: "AGENTS.md", kind: "file", sizeBytes: 4096 },
    { path: "crates", kind: "dir", sizeBytes: null },
    { path: "crates/core/src/lib.rs", kind: "file", sizeBytes: 8192 },
  ],
  complete: true,
  loaded: true,
  pendingCommandId: null,
  capabilityAvailable: true,
};

function projection(overrides: Partial<D1CockpitProjection> = {}): D1CockpitProjection {
  return { ...structuredClone(D1_PROJECTION), ...overrides };
}

function model(overrides: Partial<CommandPaletteModel> = {}): CommandPaletteModel {
  return {
    locale: "en",
    projection: projection(),
    query: "",
    crossLane: CROSS_LANE,
    files: FILES,
    canNavigate: true,
    canOpenSettings: true,
    canFocusComposer: true,
    canCancelTurn: true,
    reviewBound: true,
    reviewAvailable: true,
    evidenceBound: true,
    evidenceAvailable: true,
    ...overrides,
  };
}

function handlers(overrides: Partial<CommandPaletteHandlers> = {}): CommandPaletteHandlers {
  return {
    onNavigate: vi.fn(),
    onSelectLane: vi.fn(),
    onOpenSettings: vi.fn(),
    onFocusComposer: vi.fn(),
    onCancelTurn: vi.fn(),
    onOpenReview: vi.fn(),
    onClose: vi.fn(),
    ...overrides,
  };
}

function items(
  modelOverrides: Partial<CommandPaletteModel> = {},
  handlerOverrides: Partial<CommandPaletteHandlers> = {},
): PaletteItem[] {
  return paletteItems(model(modelOverrides), handlers(handlerOverrides));
}

function ids(list: PaletteItem[]): string[] {
  return list.map((item) => item.id);
}

describe("palette query grammar (TUI jump parity)", () => {
  test("every supported sigil scopes the same kinds the TUI scopes", () => {
    expect(parsePaletteQuery(":lane").kinds).toEqual(["lane"]);
    expect(parsePaletteQuery("@session").kinds).toEqual(["session"]);
    expect(parsePaletteQuery("#gate").kinds).toEqual(["gate", "ask"]);
    expect(parsePaletteQuery(">help").kinds).toEqual(["command"]);
    expect(parsePaletteQuery("~src").kinds).toEqual(["file"]);
  });

  test("an unscoped query accepts every kind and keeps its text", () => {
    const query = parsePaletteQuery("core");
    expect(query.kinds).toBeNull();
    expect(query.text).toBe("core");
  });

  test("the sigil is stripped and the remainder trimmed", () => {
    expect(parsePaletteQuery(":  lane-core  ").text).toBe("lane-core");
    expect(parsePaletteQuery(">").text).toBe("");
  });
});

describe("fuzzy subsequence scoring (TUI jump parity)", () => {
  test("a subsequence scores by position with an adjacency bonus", () => {
    // Ported from `fuzzy_subsequence_score`: score accumulates the matched
    // index and subtracts 2 for a match adjacent to the previous one.
    expect(fuzzySubsequenceScore("cmp", "Compiler")).toBe(3);
    expect(fuzzySubsequenceScore("cm", "Compiler")).toBe(2);
    // 0 + (1 - 2), saturating at zero exactly like the Rust usize arithmetic.
    expect(fuzzySubsequenceScore("co", "Compiler")).toBe(0);
  });

  test("missing characters score nothing at all", () => {
    expect(fuzzySubsequenceScore("xyz", "Compiler")).toBeNull();
  });

  test("matching is case-insensitive and an empty query never matches", () => {
    expect(fuzzySubsequenceScore("CMP", "compiler")).toBe(3);
    expect(fuzzySubsequenceScore("", "compiler")).toBeNull();
  });

  test("search checks title, context, and keywords", () => {
    const list: PaletteItem[] = [
      {
        kind: "lane",
        section: "jump",
        id: "lane-1",
        title: "Compiler",
        context: "Build lane",
        keywords: "rust",
        hint: null,
        enabled: true,
        disabledReason: null,
        activate: null,
      },
      {
        kind: "session",
        section: "jump",
        id: "session-1",
        title: "Notes",
        context: "Review context",
        keywords: "audit",
        hint: null,
        enabled: true,
        disabledReason: null,
        activate: null,
      },
    ];

    expect(searchPalette(list, "cmplr")).toHaveLength(1);
    expect(searchPalette(list, "rvw")).toHaveLength(1);
    expect(searchPalette(list, "adt")).toHaveLength(1);
    expect(searchPalette(list, "")).toHaveLength(2);
    expect(searchPalette(list, "missing")).toHaveLength(0);
    expect(searchPalette(list, ":cmplr")).toHaveLength(1);
    expect(searchPalette(list, "@cmplr")).toHaveLength(0);
  });
});

describe("palette index", () => {
  test("sections follow the design: actions, jump to, settings, then files", () => {
    const sections = [...new Set(items().map((item) => item.section))];
    expect(sections).toEqual(["actions", "jump", "settings", "files"]);
  });

  test("actions cover every reachable screen plus the composer and cancel", () => {
    const actions = items().filter((item) => item.section === "actions");
    expect(ids(actions)).toEqual([
      "action:focus-composer",
      "action:cancel-turn",
      // DiffReview is an in-cockpit view, not a route, so it sits with the
      // in-place actions rather than with the screen navigations.
      "action:open-review",
      "action:navigate:d2",
      "action:navigate:d4",
      "action:navigate:d10",
      "action:navigate:d11",
      "action:navigate:d12",
      "action:navigate:d13",
      "action:navigate:d14",
    ]);
    expect(actions.every((item) => item.kind === "command")).toBe(true);
  });

  test("an unavailable capability leaves its action out rather than inert", () => {
    const list = items({
      canCancelTurn: false,
    reviewBound: false,
    reviewAvailable: false,
      canNavigate: false,
      canFocusComposer: false,
      canOpenSettings: false,
    });
    expect(ids(list).some((id) => id.startsWith("action:"))).toBe(false);
  });

  test("jump rows carry the lanes and Agent sessions the projection published", () => {
    const withSession = projection({
      agentSessions: [
        {
          sessionId: "session-lane-core",
          laneId: "lane-core",
          agentId: "codex-acp",
          model: null,
          status: "running",
          task: "Freeze contract",
          diagnostic: null,
        },
      ],
    });
    const list = items({ projection: withSession });
    expect(ids(list)).toContain("lane:lane-core");
    expect(ids(list)).toContain("session:session-lane-core");
    const lane = list.find((item) => item.id === "lane:lane-core")!;
    expect(lane.kind).toBe("lane");
    expect(lane.context).toContain("coder");
    expect(lane.context).toContain("running");
  });

  test("cross-Lane gates and asks come from the eager read, scoped by #", () => {
    const list = items();
    expect(ids(searchPalette(list, "#"))).toEqual(["gate:gate-1", "ask:approval-shell"]);
  });

  test("dormant gates stay in the # scope, tagged and ordered last", () => {
    // The D12 projection orders active gates first and flags the dormant tail;
    // the palette must preserve that and tag it rather than dropping a gate an
    // operator may still want to decide.
    const list = items({
      crossLane: {
        gates: [
          { gateId: "gate-live", taskId: "task-core", status: "proposed", dormant: false },
          { gateId: "gate-old", taskId: "task-old", status: "proposed", dormant: true },
        ],
        asks: [],
        unavailable: null,
      },
    });
    const gates = searchPalette(list, "#");
    expect(ids(gates)).toEqual(["gate:gate-live", "gate:gate-old"]);
    const dormant = gates.find((item) => item.id === "gate:gate-old")!;
    expect(dormant.context).toContain("dormant");
    // Grouping, never hiding: the dormant gate is still activatable.
    expect(dormant.enabled).toBe(true);
    expect(dormant.disabledReason).toBeNull();
    expect(gates.find((item) => item.id === "gate:gate-live")!.context).not.toContain("dormant");
  });

  test("a failed cross-Lane read renders a note instead of hiding the section", () => {
    const list = items({
      crossLane: { gates: [], asks: [], unavailable: "Core did not answer" },
    });
    const note = list.find((item) => item.id === "jump:cross-lane-unavailable")!;
    expect(note.enabled).toBe(false);
    expect(note.disabledReason).toContain("Core did not answer");
    // The rest of the palette still works while the section is degraded.
    expect(ids(list)).toContain("lane:lane-core");
  });

  test("a pending cross-Lane read is stated, never guessed at", () => {
    const list = items({ crossLane: null });
    const pending = list.find((item) => item.id === "jump:cross-lane-pending")!;
    expect(pending.enabled).toBe(false);
  });

  test("settings is its own section but stays inside the > command scope", () => {
    const list = items();
    const settings = list.filter((item) => item.section === "settings");
    expect(ids(settings)).toEqual(["action:open-settings"]);
    expect(settings[0]!.kind).toBe("command");
    expect(ids(searchPalette(list, ">"))).toContain("action:open-settings");
  });

  test("files lists the Core inventory in the order Core published it", () => {
    const list = items();
    const files = list.filter((item) => item.section === "files");
    expect(files).toHaveLength(3);
    expect(files.every((item) => item.kind === "file" && item.enabled)).toBe(true);
    // Core's lexicographic order is preserved verbatim; the client never
    // re-sorts, re-groups, or discovers a path of its own.
    expect(ids(files)).toEqual([
      "file:AGENTS.md",
      "file:crates",
      "file:crates/core/src/lib.rs",
    ]);
    expect(ids(searchPalette(list, "~"))).toEqual([
      "file:AGENTS.md",
      "file:crates",
      "file:crates/core/src/lib.rs",
    ]);
    expect(ids(searchPalette(list, "~lib"))).toEqual(["file:crates/core/src/lib.rs"]);
  });

  test("files stays one disabled row naming the request against an old Core", () => {
    const list = items({
      files: { ...FILES, capabilityAvailable: false, loaded: false, entries: [] },
    });
    const files = list.filter((item) => item.section === "files");
    expect(files).toHaveLength(1);
    expect(files[0]!.enabled).toBe(false);
    // The same honesty the TUI jump index ships: a Core that publishes no
    // inventory gets the contract request, never an empty list.
    expect(files[0]!.disabledReason).toContain("GUI-CORE-022");
    expect(ids(searchPalette(list, "~"))).toEqual(["file:core-file-inventory-unavailable"]);
  });

  test("an in-flight read renders a loading row rather than an empty list", () => {
    const list = items({
      files: {
        ...FILES,
        outcome: { state: "pending", reason: null },
        entries: [],
        loaded: false,
        pendingCommandId: "gui-files-1",
      },
    });
    const files = list.filter((item) => item.section === "files");
    expect(files).toHaveLength(1);
    expect(files[0]!.enabled).toBe(false);
    expect(files[0]!.disabledReason).not.toContain("GUI-CORE-022");
    expect(files[0]!.id).toBe("file:core-file-inventory-loading");
  });

  test("an empty inventory is stated as empty, never as unavailable", () => {
    const list = items({ files: { ...FILES, entries: [] } });
    const files = list.filter((item) => item.section === "files");
    expect(files).toHaveLength(1);
    expect(files[0]!.enabled).toBe(false);
    expect(files[0]!.id).toBe("file:core-file-inventory-empty");
    expect(files[0]!.disabledReason).not.toContain("GUI-CORE-022");
  });

  test("a refused read shows Core's own reason verbatim", () => {
    const list = items({
      files: {
        ...FILES,
        outcome: { state: "rejected", reason: "Permission decision: deny" },
        entries: [],
        loaded: false,
      },
    });
    const files = list.filter((item) => item.section === "files");
    expect(files).toHaveLength(1);
    expect(files[0]!.enabled).toBe(false);
    // Core's sentence, not a locally composed one.
    expect(files[0]!.disabledReason).toBe("Permission decision: deny");
  });

  test("the palette never reads the filesystem for the files scope", () => {
    // Every file row id is derived from a Core-published path and nothing
    // else, so a model with no inventory can produce no path rows at all.
    const list = items({ files: null });
    const files = list.filter((item) => item.section === "files");
    expect(files).toHaveLength(1);
    expect(files[0]!.enabled).toBe(false);
  });
});

describe("palette overlay", () => {
  afterEach(() => {
    document.body.replaceChildren();
  });

  function mount(
    modelOverrides: Partial<CommandPaletteModel> = {},
    handlerOverrides: Partial<CommandPaletteHandlers> = {},
  ) {
    const host = document.createElement("div");
    document.body.append(host);
    const bound = handlers(handlerOverrides);
    const controller = renderCommandPalette(host, model(modelOverrides), bound);
    return { controller, handlers: bound };
  }

  function rows(): HTMLElement[] {
    return Array.from(document.querySelectorAll<HTMLElement>("[data-palette-row]"));
  }

  function input(): HTMLInputElement {
    return document.querySelector<HTMLInputElement>("[data-palette-input]")!;
  }

  function type(value: string): void {
    const field = input();
    field.value = value;
    field.dispatchEvent(new Event("input", { bubbles: true }));
  }

  function press(key: string): KeyboardEvent {
    const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
    input().dispatchEvent(event);
    return event;
  }

  test("the overlay is a modal dialog whose input owns the listbox", () => {
    mount();
    const dialog = document.querySelector<HTMLElement>("[data-command-palette]")!;
    expect(dialog.getAttribute("role")).toBe("dialog");
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    const field = input();
    expect(field.getAttribute("role")).toBe("combobox");
    expect(field.getAttribute("aria-label")).toBeTruthy();
    const list = document.querySelector<HTMLElement>("[data-palette-list]")!;
    expect(list.getAttribute("role")).toBe("listbox");
    expect(field.getAttribute("aria-controls")).toBe(list.id);
    expect(document.activeElement).toBe(field);
  });

  test("the highlighted row is the input's active descendant", () => {
    mount();
    const first = rows()[0]!;
    expect(first.getAttribute("aria-selected")).toBe("true");
    expect(input().getAttribute("aria-activedescendant")).toBe(first.id);
  });

  test("arrow keys move the highlight and skip disabled rows", () => {
    // Against a Core with no inventory the Files scope is a single disabled
    // row, which can never be activated and so can never be highlighted.
    mount({ files: { ...FILES, capabilityAvailable: false, loaded: false, entries: [] } });
    press("ArrowDown");
    expect(rows()[1]!.getAttribute("aria-selected")).toBe("true");
    press("ArrowUp");
    expect(rows()[0]!.getAttribute("aria-selected")).toBe("true");
    type("~");
    expect(rows()).toHaveLength(1);
    expect(input().getAttribute("aria-activedescendant")).toBeNull();
  });

  test("a loaded inventory row is highlightable and activates a file", () => {
    const { handlers: bound } = mount();
    type("~lib");
    expect(rows()).toHaveLength(1);
    expect(rows()[0]!.dataset.paletteItemId).toBe("file:crates/core/src/lib.rs");
    expect(input().getAttribute("aria-activedescendant")).toBe(rows()[0]!.id);
    press("Enter");
    expect(bound.onClose).toHaveBeenCalledOnce();
  });

  test("typing filters live through the ported scorer", () => {
    mount();
    type(":lane");
    expect(rows()).toHaveLength(1);
    expect(rows()[0]!.dataset.paletteItemId).toBe("lane:lane-core");
  });

  test("a seeded query is applied before the first render", () => {
    mount({ query: ">" });
    expect(input().value).toBe(">");
    expect(
      rows().every((row) => (row.dataset.paletteItemId ?? "").startsWith("action:")),
    ).toBe(true);
  });

  test("Enter activates the highlighted row and closes the overlay", () => {
    const { handlers: bound } = mount();
    type(":lane");
    press("Enter");
    expect(bound.onSelectLane).toHaveBeenCalledExactlyOnceWith("lane-core");
    expect(bound.onClose).toHaveBeenCalledOnce();
  });

  test("a gate row navigates to D12 with the exact gate id", () => {
    const { handlers: bound } = mount();
    type("#gate-1");
    press("Enter");
    expect(bound.onNavigate).toHaveBeenCalledExactlyOnceWith("d12", "gate-1");
  });

  test("an ask row navigates to D2 with the exact decision id", () => {
    const { handlers: bound } = mount();
    type("#approval-shell");
    press("Enter");
    expect(bound.onNavigate).toHaveBeenCalledExactlyOnceWith("d2", "approval-shell");
  });

  test("clicking a row activates it; clicking a disabled row does nothing", () => {
    const { handlers: bound } = mount({
      files: { ...FILES, capabilityAvailable: false, loaded: false, entries: [] },
    });
    type("~");
    rows()[0]!.click();
    expect(bound.onClose).not.toHaveBeenCalled();
    type(":lane");
    rows()[0]!.click();
    expect(bound.onSelectLane).toHaveBeenCalledExactlyOnceWith("lane-core");
  });

  test("Escape closes and never reaches a window-level handler", () => {
    const outer = vi.fn();
    window.addEventListener("keydown", outer);
    const { handlers: bound } = mount();
    press("Escape");
    window.removeEventListener("keydown", outer);
    expect(bound.onClose).toHaveBeenCalledOnce();
    // The cockpit binds Escape to "cancel the running turn" on `window`. The
    // palette must swallow its own dismissal instead of cancelling Core work.
    expect(outer).not.toHaveBeenCalled();
  });

  test("clicking the scrim closes, clicking the panel does not", () => {
    const { handlers: bound } = mount();
    document.querySelector<HTMLElement>("[data-command-palette]")!.click();
    expect(bound.onClose).not.toHaveBeenCalled();
    document.querySelector<HTMLElement>("[data-command-palette-scrim]")!.click();
    expect(bound.onClose).toHaveBeenCalledOnce();
  });

  test("closing removes the overlay and restores the focus it took", () => {
    const trigger = document.createElement("button");
    document.body.append(trigger);
    trigger.focus();
    const { controller } = mount();
    expect(document.activeElement).not.toBe(trigger);
    controller.close();
    expect(document.querySelector("[data-command-palette]")).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  test("a late cross-Lane answer fills the section without losing the query", () => {
    const { controller } = mount({ crossLane: null });
    type("#gate");
    expect(rows()[0]!.dataset.paletteItemId).toBe("jump:cross-lane-pending");
    controller.setCrossLane(CROSS_LANE);
    expect(input().value).toBe("#gate");
    expect(rows()[0]!.dataset.paletteItemId).toBe("gate:gate-1");
  });

  test("an empty result set says so rather than rendering nothing", () => {
    mount();
    type("zzzzzz");
    expect(rows()).toHaveLength(0);
    expect(document.querySelector("[data-palette-empty]")).not.toBeNull();
  });
});
