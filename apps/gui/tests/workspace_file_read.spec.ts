// @vitest-environment jsdom

/**
 * The dock's side of `runtime.workspace_file_reads` (C9).
 *
 * G5 shipped the Files tab with an inspector whose Open was a disabled control
 * naming the missing capability, and a Code tab that could not open at all.
 * Both become real here, and the rules under test are the ones that keep the
 * read honest rather than merely present:
 *
 * 1. **Four bodies, four sentences.** Text, bytes Core will not publish,
 *    nothing to read with a typed reason, and a body shape this build predates
 *    are four different affordances. An empty pane over a binary asset, or
 *    over a file Core was not allowed to open, is the failure this prevents.
 * 2. **An answer belongs to the path it named.** A held answer for another
 *    file renders as "not read yet", never as this file's bytes.
 * 3. **Core's refusal is Core's words.** The permission gate's answer arrives
 *    as `CommandRejected` and is rendered verbatim, hint included.
 * 4. **`size` and `sha256` are the whole file's.** Absence is Core not
 *    knowing, never a zero.
 * 5. **Read-only is a contract fact.** The contract publishes no workspace
 *    write, so the Code tab states it instead of leaving it to be discovered.
 */

import { describe, expect, test, vi } from "vitest";

import {
  isDockTabLive,
  renderContextDock,
  type ContextDockModel,
} from "../src/components/context_dock";
import type { PaletteWorkspaceFiles } from "../src/components/command_palette";
import type { WorkspaceDiffProjection } from "../src/models/diff_review";
import {
  IDLE_WORKSPACE_FILE,
  type WorkspaceFileFacts,
  type WorkspaceFileProjection,
} from "../src/models/workspace_file";
import { renderD1Cockpit } from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

const PATH = "crates/config/src/lib.rs";

const FILE_ROOT: PaletteWorkspaceFiles = {
  outcome: { state: "confirmed", reason: null },
  entries: [
    { path: "Cargo.toml", kind: "file", sizeBytes: 512 },
    { path: PATH, kind: "file", sizeBytes: 2048 },
  ],
  complete: true,
  loaded: true,
  pendingCommandId: null,
  capabilityAvailable: true,
};

const EMPTY_DIFF: WorkspaceDiffProjection = {
  outcome: { state: "idle", reason: null },
  targetLaneId: null,
  source: null,
  entries: [],
  truncated: false,
  loaded: false,
  pendingCommandId: null,
  capabilityAvailable: false,
  stale: false,
};

function facts(overrides: Partial<WorkspaceFileFacts> = {}): WorkspaceFileFacts {
  return {
    path: PATH,
    body: "text",
    text: null,
    truncated: false,
    size: 2048,
    sha256: "abcdef0123456789abcdef",
    reason: null,
    ...overrides,
  };
}

function answered(
  file: WorkspaceFileFacts | null,
  overrides: Partial<WorkspaceFileProjection> = {},
): WorkspaceFileProjection {
  return {
    ...IDLE_WORKSPACE_FILE,
    capabilityAvailable: true,
    outcome: { state: file ? "confirmed" : "pending", reason: null },
    requestedPath: PATH,
    targetLaneId: null,
    file,
    ...overrides,
  };
}

function model(
  content: WorkspaceFileProjection,
  overrides: {
    tab?: ContextDockModel["tab"];
    selectedPath?: string | null;
    fileReadsAvailable?: boolean;
    onOpenFile?: (path: string) => void;
  } = {},
): ContextDockModel {
  return {
    projection: D1_PROJECTION,
    locale: "en",
    matchesSelectedLane: true,
    tab: overrides.tab ?? "files",
    onSelectTab: vi.fn(),
    laneSource: null,
    diff: {
      bound: false,
      capabilityAvailable: false,
      projection: EMPTY_DIFF,
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
      selectedPath: overrides.selectedPath === undefined ? PATH : overrides.selectedPath,
      onToggleDirectory: vi.fn(),
      onSelect: vi.fn(),
      fileReadsAvailable: overrides.fileReadsAvailable ?? true,
      onOpenFile: overrides.onOpenFile ?? vi.fn(),
      content,
    },
    commit: { available: false, reason: null, onCommitOrPush: vi.fn() },
  };
}

describe("the inspector's Open becomes a real read", () => {
  test("without the capability it stays disabled and names it, visibly", () => {
    const dock = renderContextDock(
      model(IDLE_WORKSPACE_FILE, { fileReadsAvailable: false }),
    );
    const open = dock.querySelector<HTMLButtonElement>("[data-inspector-open]");
    expect(open?.disabled).toBe(true);
    // The reason is on screen, not only in a tooltip: a fact nobody can read
    // without hovering is one a screenshot cannot check either.
    expect(dock.querySelector("[data-inspector-open-reason]")?.textContent).toContain(
      "runtime.workspace_file_reads",
    );
    expect(dock.querySelector("[data-file-answer]")).toBeNull();
  });

  test("with the capability a click reads exactly the selected path", () => {
    const onOpenFile = vi.fn();
    const dock = renderContextDock(model(IDLE_WORKSPACE_FILE, { onOpenFile }));
    dock.querySelector<HTMLButtonElement>("[data-inspector-open]")?.click();
    expect(onOpenFile).toHaveBeenCalledExactlyOnceWith(PATH);
  });

  test("before any read the inspector says the file is unread, not empty", () => {
    const dock = renderContextDock(model(IDLE_WORKSPACE_FILE));
    const state = dock.querySelector<HTMLElement>("[data-file-answer='inspector']");
    expect(state?.dataset.fileState).toBe("unread");
    expect(state?.textContent).toContain("Not read yet");
  });

  test("an answer for another path is never drawn under this file's name", () => {
    const dock = renderContextDock(
      model(answered(facts({ text: "elsewhere" }), { requestedPath: "README.md" })),
    );
    const state = dock.querySelector<HTMLElement>("[data-file-answer='inspector']");
    expect(state?.dataset.fileState).toBe("unread");
    expect(state?.textContent).not.toContain("elsewhere");
  });

  test("a pending read says which file it is reading", () => {
    const dock = renderContextDock(model(answered(null)));
    expect(dock.querySelector("[data-file-pending]")?.textContent).toContain(PATH);
  });

  test("Core's refusal is rendered verbatim in an alert, with nothing loaded", () => {
    const dock = renderContextDock(
      model(
        answered(null, {
          outcome: {
            state: "rejected",
            reason: "read_file for `crates/config/src/lib.rs` is denied by rule `deny: crates/**`",
          },
        }),
      ),
    );
    const refusal = dock.querySelector<HTMLElement>("[data-file-rejected]");
    expect(refusal?.getAttribute("role")).toBe("alert");
    expect(refusal?.textContent).toContain("denied by rule");
    expect(dock.querySelector("[data-file-body]")).toBeNull();
  });
});

describe("the four bodies keep four sentences", () => {
  test("text carries the body, the whole file's size and hash", () => {
    const dock = renderContextDock(
      model(answered(facts({ text: "pub fn load_config() {}\n" }))),
    );
    expect(dock.querySelector("[data-file-body='text']")?.textContent).toBe(
      "pub fn load_config() {}\n",
    );
    const shown = dock.querySelector("[data-file-facts]")?.textContent ?? "";
    expect(shown).toContain("2048 bytes on disk");
    expect(shown).toContain("sha256 abcdef012345");
    expect(shown).toContain("workspace root");
    expect(dock.querySelector("[data-file-cut]")).toBeNull();
  });

  test("a cut body says the bound cut it rather than that the file is short", () => {
    const dock = renderContextDock(
      model(answered(facts({ text: "pub fn load", truncated: true }))),
    );
    expect(dock.querySelector("[data-file-cut]")?.textContent).toContain(
      "head of the file",
    );
  });

  test("a Lane read names the worktree it was read from", () => {
    const dock = renderContextDock(
      model(answered(facts({ text: "fn main() {}" }), { targetLaneId: "lane-core" })),
    );
    expect(dock.querySelector("[data-file-facts]")?.textContent).toContain(
      "lane-core worktree",
    );
  });

  test("binary publishes no body and keeps the size it does know", () => {
    const dock = renderContextDock(
      model(answered(facts({ body: "binary", size: 20_480 }))),
    );
    expect(dock.querySelector("[data-file-body]")).toBeNull();
    expect(dock.querySelector("[data-typed-empty='file-binary']")?.textContent).toContain(
      "not text",
    );
    expect(dock.querySelector("[data-file-facts]")?.textContent).toContain("20480 bytes");
  });

  test("each unavailable reason keeps its own sentence and no fabricated size", () => {
    for (const [reason, expected] of [
      ["not_found", "found no file"],
      ["directory", "is a directory"],
      ["unreadable", "could not read these bytes"],
      ["some_future_reason", "cannot name"],
    ] as const) {
      const dock = renderContextDock(
        model(answered(facts({ body: "unavailable", reason, size: null, sha256: null }))),
      );
      expect(
        dock.querySelector("[data-typed-empty='file-unavailable']")?.textContent,
      ).toContain(expected);
      const shown = dock.querySelector("[data-file-facts]")?.textContent ?? "";
      expect(shown).toContain("no size");
      expect(shown).toContain("no hash");
    }
  });

  test("a body shape this build predates is named, never drawn as empty text", () => {
    const dock = renderContextDock(model(answered(facts({ body: "some_future_body" }))));
    expect(dock.querySelector("[data-typed-empty='file-some_future_body']")?.textContent).toContain(
      "some_future_body",
    );
    expect(dock.querySelector("[data-file-body]")).toBeNull();
  });
});

describe("the Code tab opens exactly where Core answers", () => {
  test("it is disabled and named while Core publishes no file reads", () => {
    const dock = renderContextDock(
      model(IDLE_WORKSPACE_FILE, { fileReadsAvailable: false }),
    );
    const tab = dock.querySelector<HTMLButtonElement>(
      "[data-dock-tabs] [data-dock-tab='code']",
    );
    expect(tab?.disabled).toBe(true);
    expect(tab?.title).toContain("runtime.workspace_file_reads");
    expect(isDockTabLive("code", { fileReads: false })).toBe(false);
  });

  test("with the capability it is selectable and states that it is read-only", () => {
    const dock = renderContextDock(
      model(answered(facts({ text: "pub fn load_config() {}\n" })), { tab: "code" }),
    );
    expect(isDockTabLive("code", { fileReads: true })).toBe(true);
    expect(dock.querySelector("[data-code-read-only]")?.textContent).toContain("no workspace write");
    expect(dock.querySelector("[data-file-answer='code'] [data-file-body='text']")?.textContent).toBe(
      "pub fn load_config() {}\n",
    );
  });

  test("with no file selected it asks for one instead of showing an empty editor", () => {
    const dock = renderContextDock(
      model(IDLE_WORKSPACE_FILE, { tab: "code", selectedPath: null }),
    );
    expect(dock.querySelector("[data-typed-empty='code-selection']")?.textContent).toContain(
      "Files tab",
    );
    expect(dock.querySelector("[data-file-body]")).toBeNull();
  });

  test("Terminal and Docs stay disabled: they have no Core fact at all", () => {
    expect(isDockTabLive("terminal", { fileReads: true })).toBe(false);
    expect(isDockTabLive("docs", { fileReads: true })).toBe(false);
  });
});

/**
 * The cockpit's own wiring: which read a click sends, and where a `~` palette
 * row lands. The rendering above needs no shell; these claims do.
 */
describe("the cockpit sends the read and the palette lands in the dock", () => {
  const flush = async (): Promise<void> => {
    for (let hop = 0; hop < 8; hop += 1) {
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
  };

  /**
   * The dock tree lists the direct children of each prefix Core answered, so
   * the root page's clickable row is a root-level path.
   */
  const ROOT_FILE = "Cargo.toml";

  function mount(bind: boolean, capabilityAvailable = true) {
    const reads: Array<{ laneId: string | null; path: string }> = [];
    const root = document.createElement("div");
    document.body.replaceChildren(root);
    const answer = async () => ({
      projection: D1_PROJECTION,
      pendingCommandId: null,
      outcome: { state: "idle" as const, reason: null },
    });
    const controller = renderD1Cockpit(
      root,
      D1_PROJECTION,
      answer,
      answer,
      undefined,
      undefined,
      {
        poll: false,
        showWelcome: false,
        onNavigate: () => undefined,
        loadWorkspaceFiles: async () => FILE_ROOT,
        workspaceFile: bind
          ? {
              read: async () => ({ ...IDLE_WORKSPACE_FILE, capabilityAvailable }),
              open: async (laneId, path) => {
                reads.push({ laneId, path });
                return answered(facts({ text: "pub fn load_config() {}\n" }), {
                  requestedPath: path,
                  targetLaneId: laneId,
                });
              },
            }
          : undefined,
      },
    );
    return { root, reads, dispose: () => controller.dispose() };
  }

  test("the inspector's Open sends one read for the selected Lane's worktree", async () => {
    const mounted = mount(true);
    await flush();
    // ⌘P opens the Files tab, which reads the inventory Core published.
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "p", metaKey: true }));
    await flush();
    mounted.root.querySelector<HTMLButtonElement>(`[data-file-row='${ROOT_FILE}']`)?.click();
    await flush();
    mounted.root.querySelector<HTMLButtonElement>("[data-inspector-open]")?.click();
    await flush();
    expect(mounted.reads).toEqual([
      { laneId: D1_PROJECTION.selectedLaneId, path: ROOT_FILE },
    ]);
    expect(
      mounted.root.querySelector("[data-file-answer='inspector'] [data-file-body='text']")
        ?.textContent,
    ).toContain("load_config");
    mounted.dispose();
  });

  test("a Core without the capability keeps Open disabled and sends nothing", async () => {
    const mounted = mount(true, false);
    await flush();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "p", metaKey: true }));
    await flush();
    mounted.root.querySelector<HTMLButtonElement>(`[data-file-row='${ROOT_FILE}']`)?.click();
    await flush();
    const open = mounted.root.querySelector<HTMLButtonElement>("[data-inspector-open]");
    expect(open?.disabled).toBe(true);
    open?.click();
    await flush();
    expect(mounted.reads).toEqual([]);
    mounted.dispose();
  });

  test("a palette file row reads the file into the dock inspector", async () => {
    const mounted = mount(true);
    await flush();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true }));
    await flush();
    const row = document.querySelector<HTMLElement>(`[data-palette-item-id='file:${ROOT_FILE}']`);
    expect(row).not.toBeNull();
    row?.click();
    await flush();
    expect(mounted.reads).toEqual([
      { laneId: D1_PROJECTION.selectedLaneId, path: ROOT_FILE },
    ]);
    // The dock is where a file's content lives in this client, so the row
    // switches to it rather than opening a surface the contract has no
    // command for.
    expect(
      mounted.root.querySelector<HTMLElement>("[data-context-dock]")?.dataset.dockTab,
    ).toBe("files");
    mounted.dispose();
  });
});
