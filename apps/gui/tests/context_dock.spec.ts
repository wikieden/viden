// @vitest-environment jsdom

// The context dock's tab strip and its three live panels (`.rail.dock` in
// `docs/viden-design/Viden/GUI/Viden - 桌面驾驶舱 (GUI).html`).
//
// Every claim here is a claim about a Core fact. The dock may show a number
// only because Core published it, and where Core publishes nothing the panel
// has to say so in its own words — absence, emptiness and refusal stay three
// different sentences, never one empty list.
import { beforeEach, describe, expect, test, vi } from "vitest";

import {
  DOCK_TABS,
  renderContextDock,
  type ContextDockModel,
  type DockTab,
} from "../src/components/context_dock";
import type { PaletteWorkspaceFiles } from "../src/components/command_palette";
import type { WorkspaceDiffProjection } from "../src/models/diff_review";
import { renderD1Cockpit } from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

const DIFF_PAGE: WorkspaceDiffProjection = {
  outcome: { state: "confirmed", reason: null },
  targetLaneId: "lane-core",
  source: {
    status: "ready",
    branch: "codex/lane-core",
    worktree: ".worktrees/lane-core",
    ahead: 1,
    behind: 0,
    added: 3,
    deleted: 1,
    dirty: true,
  },
  entries: [
    {
      path: "crates/config/src/lib.rs",
      index: "modified",
      worktree: null,
      staged: true,
      diff: {
        path: "crates/config/src/lib.rs",
        oldPath: null,
        kind: "modified",
        binary: false,
        omitted: false,
        additions: 24,
        deletions: 6,
        hunks: [
          {
            oldStart: 40,
            oldLines: 2,
            newStart: 40,
            newLines: 3,
            header: "fn load_config",
            lines: [
              { kind: "context", content: "pub fn load_config(path) {", oldLine: 40, newLine: 40 },
              { kind: "removed", content: "  let raw = read(path).unwrap();", oldLine: 41, newLine: null },
              { kind: "added", content: "  let raw = read(path)?;", oldLine: null, newLine: 41 },
            ],
          },
        ],
      },
    },
    {
      path: "crates/config/tests/binary.bin",
      index: null,
      worktree: "added",
      staged: false,
      diff: null,
    },
  ],
  truncated: false,
  loaded: true,
  pendingCommandId: null,
  capabilityAvailable: true,
  stale: false,
};

const FILE_ROOT: PaletteWorkspaceFiles = {
  outcome: { state: "confirmed", reason: null },
  entries: [
    { path: "Cargo.toml", kind: "file", sizeBytes: 512 },
    { path: "crates", kind: "dir", sizeBytes: null },
  ],
  complete: true,
  loaded: true,
  pendingCommandId: null,
  capabilityAvailable: true,
};

const FILE_CRATES: PaletteWorkspaceFiles = {
  outcome: { state: "confirmed", reason: null },
  entries: [
    { path: "crates/config", kind: "dir", sizeBytes: null },
    { path: "crates/config/src/lib.rs", kind: "file", sizeBytes: 2048 },
  ],
  complete: false,
  loaded: true,
  pendingCommandId: null,
  capabilityAvailable: true,
};

function model(overrides: Partial<ContextDockModel> = {}): ContextDockModel {
  return {
    projection: D1_PROJECTION,
    locale: "en",
    matchesSelectedLane: true,
    tab: "environment",
    onSelectTab: vi.fn(),
    laneSource: null,
    diff: {
      bound: true,
      capabilityAvailable: true,
      projection: DIFF_PAGE,
      selectedPath: null,
      expanded: new Set<string>(),
      onSelect: vi.fn(),
      onToggle: vi.fn(),
      onOpenInReview: vi.fn(),
    },
    files: {
      bound: true,
      capabilityAvailable: true,
      pages: new Map([["", FILE_ROOT]]),
      expanded: new Set<string>(),
      selectedPath: null,
      onToggleDirectory: vi.fn(),
      onSelect: vi.fn(),
      fileReadsAvailable: false,
    },
    commit: { available: true, reason: null, onCommitOrPush: vi.fn() },
    ...overrides,
  };
}

function sectionIds(dock: HTMLElement): string[] {
  return Array.from(
    dock.querySelectorAll<HTMLElement>("[data-context-section]"),
    (section) => section.dataset.contextSection!,
  );
}

describe("context dock tab strip", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("renders the design's six tabs in order, three live and three disabled-and-named", () => {
    const dock = renderContextDock(model());
    const tabs = Array.from(dock.querySelectorAll<HTMLButtonElement>("[data-dock-tab]"));

    expect(tabs.map((tab) => tab.dataset.dockTab)).toEqual([
      "environment",
      "files",
      "terminal",
      "code",
      "diff",
      "docs",
    ]);
    const live = tabs.filter((tab) => !tab.disabled).map((tab) => tab.dataset.dockTab);
    expect(live).toEqual(["environment", "files", "diff"]);

    for (const id of ["terminal", "code", "docs"] as DockTab[]) {
      const tab = tabs.find((candidate) => candidate.dataset.dockTab === id)!;
      expect(tab.getAttribute("aria-disabled")).toBe("true");
      // A disabled tab must say why it is disabled, and never disappear.
      expect(tab.title.length).toBeGreaterThan(0);
      expect(tab.dataset.dockTabUnavailable).toBe("true");
    }
    expect(DOCK_TABS.filter((tab) => tab.live).map((tab) => tab.id)).toEqual([
      "environment",
      "files",
      "diff",
    ]);
  });

  test("marks the current tab and reports a click through the handler only", () => {
    const onSelectTab = vi.fn();
    const dock = renderContextDock(model({ onSelectTab }));
    const strip = dock.querySelector<HTMLElement>("[data-dock-tabs]")!;
    expect(strip.getAttribute("role")).toBe("tablist");

    const current = dock.querySelector<HTMLButtonElement>("[data-dock-tab='environment']")!;
    expect(current.getAttribute("aria-selected")).toBe("true");
    expect(dock.querySelector("[data-dock-body]")?.getAttribute("data-dock-body")).toBe(
      "environment",
    );

    dock.querySelector<HTMLButtonElement>("[data-dock-tab='diff']")!.click();
    expect(onSelectTab).toHaveBeenCalledWith("diff");
    // The component is stateless: the caller owns the tab, so the click
    // changed nothing on screen by itself.
    expect(current.getAttribute("aria-selected")).toBe("true");

    dock.querySelector<HTMLButtonElement>("[data-dock-tab='terminal']")!.click();
    expect(onSelectTab).toHaveBeenCalledTimes(1);
  });

  test("keeps the switching notice instead of a panel while the projection lags", () => {
    const dock = renderContextDock(model({ matchesSelectedLane: false }));
    expect(dock.querySelector("[data-context-dock-waiting]")).not.toBeNull();
    expect(dock.querySelector("[data-dock-tabs]")).toBeNull();
  });
});

describe("context dock environment panel", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("renders the design's collapsible sections in order", () => {
    const dock = renderContextDock(model());
    expect(sectionIds(dock)).toEqual([
      "environment",
      "changes",
      "local",
      "commit-or-push",
      "pr-status",
      "context",
      "subagents",
      "sources",
      "lane-agent",
      "mcp",
      "lsp",
      "task-checklist",
    ]);
  });

  test("Changes carries Core's line counts and one clickable row per changed file", () => {
    const onOpenInReview = vi.fn();
    const dock = renderContextDock(
      model({
        diff: { ...model().diff, onOpenInReview },
      }),
    );
    const changes = dock.querySelector<HTMLElement>("[data-context-section='changes']")!;
    // `+3 −1` are the workspace source's own `git diff --numstat` totals.
    expect(changes.querySelector("[data-changes-summary]")?.textContent).toContain("+3");
    expect(changes.querySelector("[data-changes-summary]")?.textContent).toContain("−1");

    const rows = Array.from(changes.querySelectorAll<HTMLElement>("[data-changes-row]"));
    expect(rows.map((row) => row.dataset.changesRow)).toEqual([
      "crates/config/src/lib.rs",
      "crates/config/tests/binary.bin",
    ]);
    // Per-file counts exist only where the structured diff gave them.
    expect(rows[0]!.textContent).toContain("+24");
    expect(rows[0]!.textContent).toContain("−6");
    expect(rows[1]!.querySelector("[data-changes-delta]")).toBeNull();

    rows[0]!.click();
    expect(onOpenInReview).toHaveBeenCalledWith("crates/config/src/lib.rs");
  });

  test("Changes tells an absent diff fact, a pending read and an empty answer apart", () => {
    const base = model();
    const absent = renderContextDock(
      model({ diff: { ...base.diff, capabilityAvailable: false, projection: null } }),
    );
    expect(
      absent.querySelector('[data-typed-empty="changes-capability"]')?.textContent,
    ).toContain("runtime.structured_diff");

    const pending = renderContextDock(model({ diff: { ...base.diff, projection: null } }));
    expect(pending.querySelector('[data-typed-empty="changes-pending"]')).not.toBeNull();

    const empty = renderContextDock(
      model({ diff: { ...base.diff, projection: { ...DIFF_PAGE, entries: [] } } }),
    );
    // The only state that may say "no changes": Core answered, with none.
    expect(empty.querySelector('[data-typed-empty="changes-empty"]')).not.toBeNull();
    expect(empty.querySelector('[data-typed-empty="changes-pending"]')).toBeNull();

    const rejected = renderContextDock(
      model({
        diff: {
          ...base.diff,
          projection: {
            ...DIFF_PAGE,
            loaded: false,
            entries: [],
            outcome: { state: "rejected", reason: "Denied by viden.toml" },
          },
        },
      }),
    );
    expect(rejected.querySelector("[data-changes-rejected]")?.textContent).toContain(
      "Denied by viden.toml",
    );
  });

  test("Local prefers the selected Lane's own source and names which one it shows", () => {
    const workspaceOnly = renderContextDock(model());
    const local = workspaceOnly.querySelector<HTMLElement>("[data-context-section='local']")!;
    expect(local.dataset.localScope).toBe("workspace");

    const laneScoped = renderContextDock(
      model({
        laneSource: {
          status: "ready",
          branch: "vd/retry-policy",
          worktree: ".worktrees/retry",
          ahead: 4,
          behind: 2,
          added: 9,
          deleted: 0,
          dirty: false,
        },
      }),
    );
    const laneLocal = laneScoped.querySelector<HTMLElement>("[data-context-section='local']")!;
    expect(laneLocal.dataset.localScope).toBe("lane");
    expect(laneLocal.textContent).toContain("vd/retry-policy");
    expect(laneLocal.textContent).toContain("4");
  });

  test("Commit or push routes when Core can act and is disabled-and-named when it cannot", () => {
    const onCommitOrPush = vi.fn();
    const live = renderContextDock(model({ commit: { available: true, reason: null, onCommitOrPush } }));
    const action = live.querySelector<HTMLButtonElement>("[data-commit-or-push]")!;
    expect(action.disabled).toBe(false);
    action.click();
    expect(onCommitOrPush).toHaveBeenCalledTimes(1);

    const blocked = renderContextDock(
      model({
        commit: {
          available: false,
          reason: "runtime.operator_git",
          onCommitOrPush,
        },
      }),
    );
    const disabled = blocked.querySelector<HTMLButtonElement>("[data-commit-or-push]")!;
    expect(disabled.disabled).toBe(true);
    expect(disabled.title).toContain("runtime.operator_git");
    expect(blocked.querySelector("[data-commit-reason]")?.textContent).toContain(
      "runtime.operator_git",
    );
    disabled.click();
    expect(onCommitOrPush).toHaveBeenCalledTimes(1);
  });

  test("PR status, Subagents and MCP each state the exact absence behind them", () => {
    const dock = renderContextDock(model());
    expect(dock.querySelector('[data-typed-empty="pr-status"]')?.textContent).toContain(
      "pull request",
    );
    // D-RAILNAV ④ deferred the embedded tree; the row says so and renders none.
    const subagents = dock.querySelector<HTMLElement>("[data-context-section='subagents']")!;
    expect(subagents.querySelector("[data-subagents-deferred]")).not.toBeNull();
    expect(subagents.querySelectorAll("[data-subagent]")).toHaveLength(0);
    expect(dock.querySelector('[data-typed-empty="mcp"]')).not.toBeNull();
  });

  test("the budget bar draws Core's own used/limit and keeps the cost-blind label", () => {
    const withBudget = renderContextDock(
      model({
        projection: {
          ...D1_PROJECTION,
          contextDock: {
            ...D1_PROJECTION.contextDock,
            context: {
              budgetId: "budget-main",
              usedTokens: 64,
              softTokenLimit: 96,
              hardTokenLimit: 128,
              remainingTokens: 64,
              exceeded: false,
            },
          },
        },
      }),
    );
    const bar = withBudget.querySelector<HTMLElement>("[data-budget-bar]")!;
    expect(bar.dataset.budgetPercent).toBe("50");
    expect(withBudget.querySelector("[data-budget-used]")?.textContent).toContain("64");
    expect(withBudget.querySelector("[data-budget-used]")?.textContent).toContain("128");

    const blind = renderContextDock(
      model({
        projection: {
          ...D1_PROJECTION,
          environment: { ...D1_PROJECTION.environment, costMicroUsd: null },
          contextDock: {
            ...D1_PROJECTION.contextDock,
            context: {
              budgetId: "budget-main",
              usedTokens: 64,
              softTokenLimit: 96,
              hardTokenLimit: 128,
              remainingTokens: 64,
              exceeded: false,
            },
          },
        },
      }),
    );
    expect(blind.querySelector("[data-budget-blind]")).not.toBeNull();
  });
});

describe("context dock files panel", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("renders Core's root page as a tree and expands one directory at a time", () => {
    const onToggleDirectory = vi.fn();
    const base = model();
    const dock = renderContextDock(
      model({ tab: "files", files: { ...base.files, onToggleDirectory } }),
    );
    const rows = Array.from(dock.querySelectorAll<HTMLElement>("[data-file-row]"));
    expect(rows.map((row) => row.dataset.fileRow)).toEqual(["Cargo.toml", "crates"]);

    const directory = rows[1]!;
    expect(directory.dataset.fileKind).toBe("dir");
    expect(directory.getAttribute("aria-expanded")).toBe("false");
    directory.click();
    expect(onToggleDirectory).toHaveBeenCalledWith("crates");

    const expanded = renderContextDock(
      model({
        tab: "files",
        files: {
          ...base.files,
          expanded: new Set(["crates"]),
          pages: new Map([
            ["", FILE_ROOT],
            ["crates/", FILE_CRATES],
          ]),
        },
      }),
    );
    expect(
      Array.from(expanded.querySelectorAll<HTMLElement>("[data-file-row]"), (row) => row.dataset.fileRow),
    ).toEqual(["Cargo.toml", "crates", "crates/config"]);
    // Core said its page stopped short; the tree says so rather than implying
    // the subtree ends there.
    expect(expanded.querySelector("[data-file-truncated]")).not.toBeNull();
  });

  test("selecting a file fills the inspector and names the read capability Open needs", () => {
    const onSelect = vi.fn();
    const base = model();
    const dock = renderContextDock(
      model({ tab: "files", files: { ...base.files, onSelect } }),
    );
    dock.querySelector<HTMLElement>("[data-file-row='Cargo.toml']")!.click();
    expect(onSelect).toHaveBeenCalledWith("Cargo.toml");

    const selected = renderContextDock(
      model({ tab: "files", files: { ...base.files, selectedPath: "Cargo.toml" } }),
    );
    const inspector = selected.querySelector<HTMLElement>("[data-dock-inspector]")!;
    expect(inspector.textContent).toContain("Cargo.toml");
    const open = inspector.querySelector<HTMLButtonElement>("[data-inspector-open]")!;
    expect(open.disabled).toBe(true);
    expect(open.title).toContain("runtime.workspace_file_reads");
    // Readable without a pointer, so the capability gap is on screen.
    expect(
      inspector.querySelector("[data-inspector-open-reason]")?.textContent,
    ).toContain("runtime.workspace_file_reads");
  });

  test("an absent inventory capability states itself instead of an empty tree", () => {
    const base = model();
    const dock = renderContextDock(
      model({
        tab: "files",
        files: { ...base.files, capabilityAvailable: false, pages: new Map() },
      }),
    );
    expect(dock.querySelector('[data-typed-empty="files-capability"]')?.textContent).toContain(
      "runtime.workspace_files",
    );
    expect(dock.querySelectorAll("[data-file-row]")).toHaveLength(0);
  });
});

describe("context dock diff panel", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("lists one collapsed entry per changed file and expands on request", () => {
    const onToggle = vi.fn();
    const base = model();
    const dock = renderContextDock(model({ tab: "diff", diff: { ...base.diff, onToggle } }));
    const files = Array.from(dock.querySelectorAll<HTMLElement>("[data-diff-entry]"));
    expect(files.map((entry) => entry.dataset.diffEntry)).toEqual([
      "crates/config/src/lib.rs",
      "crates/config/tests/binary.bin",
    ]);
    // Collapsed by default: the panel is a list of files, not a wall of rows.
    expect(dock.querySelector("[data-diff-body]")).toBeNull();

    dock
      .querySelector<HTMLButtonElement>("[data-diff-toggle='crates/config/src/lib.rs']")!
      .click();
    expect(onToggle).toHaveBeenCalledWith("crates/config/src/lib.rs");

    const expanded = renderContextDock(
      model({
        tab: "diff",
        diff: { ...base.diff, expanded: new Set(["crates/config/src/lib.rs"]) },
      }),
    );
    expect(expanded.querySelector("[data-diff-body]")).not.toBeNull();
  });

  test("the inspector carries File, Diff, an in-review route, and inert stage/revert", () => {
    const onOpenInReview = vi.fn();
    const base = model();
    const dock = renderContextDock(
      model({
        tab: "diff",
        diff: { ...base.diff, selectedPath: "crates/config/src/lib.rs", onOpenInReview },
      }),
    );
    const inspector = dock.querySelector<HTMLElement>("[data-dock-inspector]")!;
    expect(inspector.textContent).toContain("crates/config/src/lib.rs");
    expect(inspector.textContent).toContain("+24");

    inspector.querySelector<HTMLButtonElement>("[data-inspector-review]")!.click();
    expect(onOpenInReview).toHaveBeenCalledWith("crates/config/src/lib.rs");

    for (const marker of ["stage", "revert"]) {
      const action = inspector.querySelector<HTMLButtonElement>(
        `[data-inspector-action='${marker}']`,
      )!;
      expect(action.disabled).toBe(true);
      expect(action.title.length).toBeGreaterThan(0);
    }
  });

  test("an unbound host states itself rather than rendering an empty diff list", () => {
    const base = model();
    const dock = renderContextDock(
      model({ tab: "diff", diff: { ...base.diff, bound: false, projection: null } }),
    );
    expect(dock.querySelector('[data-typed-empty="diff-unbound"]')).not.toBeNull();
  });
});

/**
 * The dock inside the cockpit: the chords, the reads it issues, and the route
 * a change row takes into DiffReview.
 *
 * These are the wiring claims, not the rendering ones. The rendering lives
 * above and needs no shell.
 */
describe("context dock inside the cockpit", () => {
  const flush = async (): Promise<void> => {
    await new Promise((resolve) => setTimeout(resolve, 0));
    await new Promise((resolve) => setTimeout(resolve, 0));
    await new Promise((resolve) => setTimeout(resolve, 0));
  };

  function mount(
    overrides: {
      files?: (prefix: string | null) => Promise<PaletteWorkspaceFiles>;
      diff?: WorkspaceDiffProjection;
      bindDiff?: boolean;
      bindFiles?: boolean;
    } = {},
  ): { root: HTMLElement; dispose: () => void } {
    const root = document.createElement("div");
    document.body.replaceChildren(root);
    const answer = async () => ({
      projection: D1_PROJECTION,
      pendingCommandId: null,
      outcome: { state: "idle" as const, reason: null },
    });
    const page = overrides.diff ?? DIFF_PAGE;
    const controller = renderD1Cockpit(root, D1_PROJECTION, answer, answer, undefined, undefined, {
      poll: false,
      showWelcome: false,
      onNavigate: () => undefined,
      workspaceDiff:
        overrides.bindDiff === false
          ? undefined
          : { read: async () => page, query: async () => page },
      loadWorkspaceFiles:
        overrides.bindFiles === false
          ? undefined
          : (overrides.files ?? (async () => FILE_ROOT)),
    });
    return { root, dispose: () => controller.dispose() };
  }

  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("fills the Changes section from one diff read at mount", async () => {
    const mounted = mount();
    await flush();
    const rows = Array.from(
      mounted.root.querySelectorAll<HTMLElement>("[data-changes-row]"),
      (row) => row.dataset.changesRow,
    );
    expect(rows).toEqual([
      "crates/config/src/lib.rs",
      "crates/config/tests/binary.bin",
    ]);
    mounted.dispose();
  });

  test("a change row opens DiffReview on that exact file", async () => {
    const mounted = mount();
    await flush();
    mounted.root
      .querySelector<HTMLButtonElement>("[data-changes-row='crates/config/src/lib.rs']")!
      .click();
    await flush();
    expect(mounted.root.querySelector("[data-diff-review]")).not.toBeNull();
    // DiffReview's own selection, seeded by the route rather than by a second
    // entry point into the view.
    expect(
      mounted.root
        .querySelector("[data-path='crates/config/src/lib.rs']")
        ?.getAttribute("aria-current"),
    ).toBe("true");
    mounted.dispose();
  });

  test("⌥⌘E, ⌘P and ⌘D switch panels and ⌘O stays the folder picker's", async () => {
    const prefixes: Array<string | null> = [];
    const mounted = mount({
      files: async (prefix) => {
        prefixes.push(prefix);
        return FILE_ROOT;
      },
    });
    await flush();
    const dock = () => mounted.root.querySelector<HTMLElement>("[data-context-dock]")!;
    expect(dock().dataset.dockTab).toBe("environment");

    const files = new KeyboardEvent("keydown", { key: "p", metaKey: true, cancelable: true });
    window.dispatchEvent(files);
    await flush();
    expect(files.defaultPrevented).toBe(true);
    expect(dock().dataset.dockTab).toBe("files");
    // The Files tab reads the workspace root, and the palette's `~` scope read
    // the same root earlier in the same mount, so the prefix is the only thing
    // that distinguishes them.
    expect(prefixes).toContain(null);

    const diff = new KeyboardEvent("keydown", { key: "d", metaKey: true, cancelable: true });
    window.dispatchEvent(diff);
    await flush();
    expect(diff.defaultPrevented).toBe(true);
    expect(dock().dataset.dockTab).toBe("diff");

    const environment = new KeyboardEvent("keydown", {
      key: "e",
      metaKey: true,
      altKey: true,
      cancelable: true,
    });
    window.dispatchEvent(environment);
    await flush();
    expect(environment.defaultPrevented).toBe(true);
    expect(dock().dataset.dockTab).toBe("environment");

    // `⌘O` is the Welcome screen's folder picker and keeps the chord; the Code
    // tab it would open in the design is disabled here anyway.
    const code = new KeyboardEvent("keydown", { key: "o", metaKey: true, cancelable: true });
    window.dispatchEvent(code);
    await flush();
    expect(dock().dataset.dockTab).toBe("environment");
    mounted.dispose();
  });

  test("⌃P still opens the command palette instead of the Files tab", async () => {
    const mounted = mount();
    await flush();
    const event = new KeyboardEvent("keydown", { key: "p", ctrlKey: true, cancelable: true });
    window.dispatchEvent(event);
    await flush();
    expect(mounted.root.querySelector("[data-command-palette]")).not.toBeNull();
    expect(
      mounted.root.querySelector<HTMLElement>("[data-context-dock]")!.dataset.dockTab,
    ).toBe("environment");
    mounted.dispose();
  });

  test("opening a directory reads exactly that prefix, once", async () => {
    const prefixes: Array<string | null> = [];
    const mounted = mount({
      files: async (prefix) => {
        prefixes.push(prefix);
        return prefix === null ? FILE_ROOT : FILE_CRATES;
      },
    });
    await flush();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "p", metaKey: true }));
    await flush();
    mounted.root.querySelector<HTMLButtonElement>("[data-file-row='crates']")!.click();
    await flush();
    expect(prefixes.filter((prefix) => prefix === "crates/")).toHaveLength(1);
    expect(mounted.root.querySelector("[data-file-row='crates/config']")).not.toBeNull();

    // Closing and reopening reuses the page Core already answered rather than
    // shelling out again; the rows come back without a second read.
    mounted.root.querySelector<HTMLButtonElement>("[data-file-row='crates']")!.click();
    await flush();
    expect(mounted.root.querySelector("[data-file-row='crates/config']")).toBeNull();
    mounted.root.querySelector<HTMLButtonElement>("[data-file-row='crates']")!.click();
    await flush();
    expect(prefixes.filter((prefix) => prefix === "crates/")).toHaveLength(1);
    expect(mounted.root.querySelector("[data-file-row='crates/config']")).not.toBeNull();
    mounted.dispose();
  });

  test("focus mode still takes the dock off the grid and into its hot zone", async () => {
    const mounted = mount();
    await flush();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: ".", metaKey: true }));
    await flush();
    const body = mounted.root.querySelector<HTMLElement>(".d1-body")!;
    expect(body.dataset.focusMode).toBe("true");
    // The same `.edgewrap.r` host G4 built: the dock is one pointer move away,
    // never deleted.
    expect(body.querySelector("[data-dock-edge]")).not.toBeNull();
    expect(
      body.querySelector("[data-dock-edge] [data-shell-landmark='context-dock']"),
    ).not.toBeNull();
    mounted.dispose();
  });

  test("an unbound file read leaves the Files tab stating itself", async () => {
    const mounted = mount({ bindFiles: false });
    await flush();
    mounted.root.querySelector<HTMLButtonElement>("[data-dock-tab='files']")!.click();
    await flush();
    expect(mounted.root.querySelector('[data-typed-empty="files-unbound"]')).not.toBeNull();
    mounted.dispose();
  });
});
