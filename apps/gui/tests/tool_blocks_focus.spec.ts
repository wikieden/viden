// @vitest-environment jsdom

// The transcript's tool blocks and the cockpit's focus mode.
//
// The design draws a workspace change and a check run as the same `.tool`
// block — a `.th` header over a `.tb` body — with the diff rows inline under
// the header and the check's status and failing line as `.testrow`s. Focus
// mode is `D-SIDEBAR`'s override: "两侧强制 hover 浮窗(退出恢复)", both sides
// forced to hover panels, restored on exit.
import { beforeEach, describe, expect, test, vi } from "vitest";

import { renderD1Cockpit, type D1Intent, type D1IntentResult } from "../src/screens/d1_cockpit";
import type { D1RenderOptions } from "../src/screens/d1_cockpit";
import type { ChecklistItemProjection } from "../src/models/workspace";
import type { DiffDocumentProjection } from "../src/models/diff_review";
import { D1_PROJECTION } from "./support/d1_projection";

const DIFF: DiffDocumentProjection = {
  files: [
    {
      path: "src/config.rs",
      oldPath: null,
      kind: "modified",
      binary: false,
      omitted: false,
      additions: 1,
      deletions: 1,
      hunks: [
        {
          header: "fn load",
          oldStart: 40,
          oldLines: 3,
          newStart: 40,
          newLines: 3,
          lines: [
            {
              kind: "removed",
              content: "let raw = fs::read_to_string(path).unwrap();",
              oldLine: 41,
              newLine: null,
            },
            {
              kind: "added",
              content: "let raw = fs::read_to_string(path)?;",
              oldLine: null,
              newLine: 41,
            },
          ],
        },
      ],
    },
  ],
  truncated: false,
  byteLimit: 262_144,
};

const CHANGE: ChecklistItemProjection = {
  id: "change-config",
  kind: "workspace_change",
  label: "src/config.rs",
  status: "modified",
  command: null,
  path: "src/config.rs",
  summary: null,
  diff: DIFF,
  failingLocation: null,
  additions: 1,
  deletions: 1,
};

const CHECK: ChecklistItemProjection = {
  id: "check-config",
  kind: "check_run",
  label: "viden-cli config tests",
  status: "failed",
  command: "cargo test -p viden-cli config_tests",
  path: null,
  summary: "1 failed",
  diff: null,
  failingLocation: "src/config.rs:42:15",
  additions: null,
  deletions: null,
};

const PROJECTION = {
  ...D1_PROJECTION,
  contextDock: { ...D1_PROJECTION.contextDock, checklist: [CHANGE, CHECK] },
};

/** The same cockpit with nothing running, so `Esc` reaches the view layer. */
const IDLE_PROJECTION = {
  ...PROJECTION,
  composer: { editable: true, busy: false, canCancel: false, canSubmitImmediately: true },
};

function setup(projection = PROJECTION, options: Partial<D1RenderOptions> = {}) {
  const root = document.createElement("div");
  document.body.append(root);
  const send = vi.fn(
    async (_intent: D1Intent): Promise<D1IntentResult> => ({
      projection,
      pendingCommandId: null,
      outcome: { state: "idle", reason: null },
    }),
  );
  const poll = vi.fn(
    async (): Promise<D1IntentResult> => ({
      projection,
      pendingCommandId: null,
      outcome: { state: "idle", reason: null },
    }),
  );
  const controller = renderD1Cockpit(root, projection, send, poll, undefined, undefined, {
    poll: false,
    showWelcome: false,
    ...options,
  });
  return { root, controller };
}

describe("inline tool diffs", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("draws the design's header and keeps the hunk rows collapsed", () => {
    const { root, controller } = setup();

    const block = root.querySelector<HTMLElement>('[data-workspace-change="change-config"]')!;
    expect(block.classList.contains("tool")).toBe(true);
    const header = block.querySelector<HTMLButtonElement>("[data-tool-toggle]")!;
    expect(header.querySelector(".pa")?.textContent).toBe("src/config.rs");
    expect(header.querySelector("[data-tool-stats]")?.textContent).toBe("+1 −1");
    expect(header.getAttribute("aria-expanded")).toBe("false");

    const body = block.querySelector<HTMLElement>("[data-tool-body]")!;
    expect(body.hidden).toBe(true);
    // The rows are the shared hunk renderer, not a second idea of a diff.
    expect(body.querySelector("[data-diff-body]")).not.toBeNull();
    expect(body.textContent).toContain("let raw = fs::read_to_string(path)?;");

    header.click();
    expect(
      root
        .querySelector<HTMLElement>('[data-workspace-change="change-config"] [data-tool-body]')!
        .hidden,
    ).toBe(false);
    controller.dispose();
  });

  test("carries the gate state only while Core has an approval for that path", () => {
    const { root, controller } = setup({
      ...PROJECTION,
      permissionDock: {
        ...PROJECTION.permissionDock,
        request: {
          id: "approval-edit",
          toolName: "edit_file",
          title: "Edit src/config.rs",
          message: "",
          inputPreview: "",
          isMutating: true,
          reason: null,
          risk: "medium",
          target: { kind: "path", display: "src/config.rs", canonicalRef: null },
          policyReasonKey: "d1.permission.reason.mutating",
          policyReasonArgs: {},
          expiresAt: 0,
          defaultAction: "deny",
          auditId: "audit-1",
          blockedByPlan: false,
          decisionContext: { diff: DIFF, baseSha256: null },
          actions: [],
        },
      },
    });

    const block = root.querySelector<HTMLElement>('[data-workspace-change="change-config"]')!;
    const gate = block.querySelector<HTMLElement>("[data-tool-gate]")!;
    expect(gate.textContent).toContain("waiting");
    // Core named the tool that proposed it; the header stops guessing.
    expect(block.querySelector(".nm")?.textContent).toBe("edit_file");
    controller.dispose();
  });

  test("keeps today's row for a change Core published no rows for", () => {
    const { root, controller } = setup({
      ...PROJECTION,
      contextDock: {
        ...PROJECTION.contextDock,
        checklist: [{ ...CHANGE, diff: null, patch: null }],
      },
    });

    expect(
      root.querySelector('[data-typed-empty="workspace-change-patch"]')?.textContent,
    ).toBe("No typed patch is available.");
    expect(root.querySelector("[data-diff-body]")).toBeNull();
    controller.dispose();
  });

  test("renders a check run as the design's test block", () => {
    const { root, controller } = setup();

    const block = root.querySelector<HTMLElement>('[data-check-run="check-config"]')!;
    expect(block.classList.contains("tool")).toBe(true);
    expect(block.querySelector(".pa")?.textContent).toBe(
      "cargo test -p viden-cli config_tests",
    );
    const rows = Array.from(
      block.querySelectorAll<HTMLElement>("[data-check-row]"),
      (row) => row.dataset.checkRow,
    );
    expect(rows).toEqual(["status", "failing", "result"]);
    expect(block.querySelector('[data-check-row="failing"]')?.textContent).toContain(
      "src/config.rs:42:15",
    );
    controller.dispose();
  });

  test("leaves the failing row out when Core reported no location", () => {
    const { root, controller } = setup({
      ...PROJECTION,
      contextDock: {
        ...PROJECTION.contextDock,
        checklist: [{ ...CHECK, status: "passed", failingLocation: null }],
      },
    });

    expect(root.querySelector('[data-check-row="failing"]')).toBeNull();
    expect(root.querySelector('[data-check-row="status"]')?.textContent).toContain("Passed");
    controller.dispose();
  });
});

describe("focus mode", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  function body(root: HTMLElement): HTMLElement {
    return root.querySelector<HTMLElement>("[data-cockpit-grid]")!;
  }

  test("⌘. forces both sides to hover panels and gives the width to the transcript", () => {
    const { root, controller } = setup(PROJECTION, { laneSidebarMode: "pinned" });

    expect(body(root).dataset.focusMode).toBe("false");
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: ".", metaKey: true, bubbles: true }),
    );

    expect(body(root).dataset.focusMode).toBe("true");
    // The pinned column is gone and the sidebar is behind the hot zone.
    expect(body(root).dataset.laneColumn).toBe("false");
    expect(root.querySelector("[data-lane-edge]")).not.toBeNull();
    // The context dock is off the grid and behind its own right-edge zone.
    expect(root.querySelector("[data-dock-edge]")).not.toBeNull();
    expect(
      root.querySelector('[data-shell-landmark="context-dock"]')?.closest("[data-dock-edge]"),
    ).not.toBeNull();

    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: ".", metaKey: true, bubbles: true }),
    );
    // Leaving restores the mode the operator chose, which is the decision's
    // own "退出恢复".
    expect(body(root).dataset.focusMode).toBe("false");
    expect(body(root).dataset.laneColumn).toBe("true");
    expect(root.querySelector("[data-dock-edge]")).toBeNull();
    controller.dispose();
  });

  test("the titlebar's focus control toggles the same state", () => {
    const { root, controller } = setup();

    const toggle = root.querySelector<HTMLButtonElement>("[data-focus-toggle]")!;
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    toggle.click();
    expect(body(root).dataset.focusMode).toBe("true");
    expect(
      root.querySelector<HTMLButtonElement>("[data-focus-toggle]")!.getAttribute("aria-pressed"),
    ).toBe("true");
    controller.dispose();
  });

  test("Esc leaves focus mode, but only after every surface above it", () => {
    // No cancellable turn: priority 4 is the composer's cancel binding, and a
    // Lane that is really busy owns `Esc` before any view does.
    const { root, controller } = setup(IDLE_PROJECTION, {
      secondaryViews: {
        mount: (route, container) => {
          container.textContent = route;
        },
      },
    });

    root.querySelector<HTMLButtonElement>("[data-focus-toggle]")!.click();
    controller.openCenterView("d2");
    expect(body(root).dataset.focusMode).toBe("true");

    // The centre view is above focus mode in the priority list.
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(root.querySelector("[data-secondary-view]")).toBeNull();
    expect(body(root).dataset.focusMode).toBe("true");

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(body(root).dataset.focusMode).toBe("false");
    controller.dispose();
  });
});
