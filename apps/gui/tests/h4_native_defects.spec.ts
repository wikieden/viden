// @vitest-environment jsdom

/**
 * The native defects the E3a run found in the shipped `0.1.0-rc.5` cockpit
 * (`docs/release-evidence/gui-trusted-delivery/checkpoints.md`, defects 14-21).
 *
 * Each test here is the red half of one defect: it drives the shipped render
 * functions and the shipped host reducers over the exact state the native run
 * was in, and asserts the behaviour the run could not get. Nothing is mocked
 * that the run exercised for real — the agent menu, the cockpit router, the
 * dock's two source samples and the commit bar are the production modules.
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { beforeEach, describe, expect, test, vi } from "vitest";

import {
  AGENT_PROBE_TIMEOUT_MS,
  renderAgentMenu,
  type AgentMenuModel,
} from "../src/components/agent_menu";
import { renderContextDock, type ContextDockModel } from "../src/components/context_dock";
import type { WorkspaceDiffProjection } from "../src/models/diff_review";
import { IDLE_OPERATOR_GIT, type OperatorGitProjection } from "../src/models/operator_git";
import { IDLE_WORKSPACE_FILE } from "../src/models/workspace_file";
import {
  renderD1Cockpit,
  type D1CockpitProjection,
  type D1Controller,
  type D1IntentResult,
  type D1RenderOptions,
} from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

const flush = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0));

function projection(overrides: Partial<D1CockpitProjection> = {}): D1CockpitProjection {
  return { ...structuredClone(D1_PROJECTION), ...overrides };
}

function idle(next: D1CockpitProjection): D1IntentResult {
  return { projection: next, pendingCommandId: null, outcome: { state: "idle", reason: null } };
}

function mount(
  options: D1RenderOptions = {},
  initial: D1CockpitProjection = projection(),
  send: (intent: { type: string }) => Promise<D1IntentResult> = async () => idle(initial),
): { root: HTMLElement; controller: D1Controller } {
  document.body.innerHTML = '<main id="app"></main>';
  const root = document.querySelector<HTMLElement>("#app")!;
  const controller = renderD1Cockpit(
    root,
    initial,
    send as never,
    vi.fn(async () => idle(initial)),
    undefined,
    undefined,
    { poll: false, ...options },
  );
  return { root, controller };
}

/* ---- defect 14: Lane creation gated on ACP discovery ---- */

const MENU_MODEL: AgentMenuModel = {
  locale: "en",
  canCreateLane: true,
  usesGitIsolation: true,
  probing: false,
  eligibilityDiagnostic: null,
  discoveryDiagnostic: null,
  adapters: [
    { agentId: "codex-acp", displayName: "Codex", startability: "probe_required", diagnostics: [] },
  ],
};

function menu(model: AgentMenuModel) {
  document.body.innerHTML = '<div id="host"><button id="anchor">+</button></div>';
  const anchor = document.querySelector<HTMLButtonElement>("#anchor")!;
  return renderAgentMenu(anchor, model, vi.fn());
}

function typeTask(root: HTMLElement, text: string): void {
  const task = root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;
  task.value = text;
  task.dispatchEvent(new InputEvent("input", { bubbles: true }));
}

describe("defect 14 — the built-in agent needs no ACP probe", () => {
  beforeEach(() => {
    document.documentElement.lang = "en";
  });

  test("creates a Lane on the built-in agent while every ACP probe is still out", () => {
    const controller = menu({
      ...MENU_MODEL,
      probing: true,
      adapters: [
        {
          agentId: "codex-acp",
          displayName: "Codex",
          startability: "probe_required",
          diagnostics: [],
          probe: "checking",
        },
      ],
    });

    controller.root.querySelector<HTMLButtonElement>("[data-native-agent]")!.click();
    typeTask(controller.root, "Edit the fixture README heading");

    const create = controller.root.querySelector<HTMLButtonElement>("[data-lane-task-submit]")!;
    expect(create.disabled).toBe(false);
    expect(
      controller.root.querySelector("[data-native-agent]")?.getAttribute("aria-disabled"),
    ).toBe("false");
  });

  test("an ACP adapter whose probe is still out is checking, labelled, and unselectable", () => {
    const controller = menu({
      ...MENU_MODEL,
      probing: true,
      adapters: [
        {
          agentId: "codex-acp",
          displayName: "Codex",
          startability: "probe_required",
          diagnostics: [],
          probe: "checking",
        },
      ],
    });

    const row = controller.root.querySelector<HTMLElement>('[data-agent-id="codex-acp"]')!;
    expect(row.getAttribute("aria-disabled")).toBe("true");
    expect(row.textContent).toContain("Checking");
    // The diagnostic states the bound rather than an open-ended "checking…".
    expect(controller.root.querySelector(".agent-menu-diagnostic")?.textContent).toContain(
      String(Math.round(AGENT_PROBE_TIMEOUT_MS / 1000)),
    );
  });

  test("a probe that passed its bound says so and stays disabled-and-labelled", () => {
    const controller = menu({
      ...MENU_MODEL,
      probing: false,
      adapters: [
        {
          agentId: "codex-acp",
          displayName: "Codex",
          startability: "probe_required",
          diagnostics: [],
          probe: "timed_out",
        },
      ],
    });
    controller.root.querySelector<HTMLButtonElement>("[data-native-agent]")!.click();
    typeTask(controller.root, "Edit the fixture README heading");

    const row = controller.root.querySelector<HTMLElement>('[data-agent-id="codex-acp"]')!;
    expect(row.getAttribute("aria-disabled")).toBe("true");
    expect(row.dataset.agentProbe).toBe("timed_out");
    expect(row.textContent).toContain("did not answer");
    // The built-in path is unaffected by another adapter's dead probe.
    expect(
      controller.root.querySelector<HTMLButtonElement>("[data-lane-task-submit]")!.disabled,
    ).toBe(false);
  });

  test("an ACP selection carried back from D4 names why Create is refused", () => {
    // The wizard's Cancel returns the draft it was entered with, so a
    // selection can outlive the probe that qualified it.
    const controller = menu({
      ...MENU_MODEL,
      selected: { kind: "acp", agentId: "codex-acp" },
      adapters: [
        {
          agentId: "codex-acp",
          displayName: "Codex",
          startability: "ready",
          diagnostics: [],
          probe: "timed_out",
        },
      ],
    });
    typeTask(controller.root, "Edit the fixture README heading");

    expect(
      controller.root.querySelector<HTMLButtonElement>("[data-lane-task-submit]")!.disabled,
    ).toBe(true);
    expect(controller.root.querySelector(".agent-menu-diagnostic")?.textContent).toContain(
      "did not answer",
    );
  });
});

describe("defect 14 — the popover's own bound always ends the sentence", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("leaves `Checking ACP agents…` within the bound even with nothing in flight", async () => {
    // Only the clock: faking `queueMicrotask` too would stop the discovery
    // walk this test is about from ever advancing.
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    try {
      const probing = projection({
        selectedLaneId: null,
        lanes: [],
        agentAdapters: [
          {
            agentId: "codex-acp",
            displayName: "Codex",
            startability: "probe_required",
            diagnostics: [],
            models: [],
          },
        ],
      });
      // The adapter query answers; the probe never does — a host with no ACP
      // binary, which is what the native run ran on.
      const send = vi.fn(async (intent: { type: string }): Promise<D1IntentResult> => {
        if (intent.type === "query_agent_adapters") return idle(probing);
        return new Promise<D1IntentResult>(() => undefined);
      });
      const abandon = vi.fn(async () => idle(probing));
      // A quiet Core: no wake ever re-queues the discovery walk, which is how
      // the native run's popover stalled with nothing outstanding.
      const { root, controller } = mount({ agentProbe: { abandon } }, probing, send);
      root.querySelector<HTMLButtonElement>("[data-create-lane]")!.click();
      await vi.advanceTimersByTimeAsync(200);
      expect(document.querySelector("[data-new-lane-popover]")).not.toBeNull();

      await vi.advanceTimersByTimeAsync(AGENT_PROBE_TIMEOUT_MS + 200);

      const diagnostic = document.querySelector(".agent-menu-diagnostic")?.textContent ?? "";
      expect(diagnostic).not.toContain("Checking");
      expect(
        document.querySelector('[data-agent-id="codex-acp"]')?.getAttribute("data-agent-probe"),
      ).toBe("timed_out");
      // The slot is handed back through the host's own refusal-safe port.
      expect(abandon).toHaveBeenCalled();
      controller.dispose();
    } finally {
      vi.useRealTimers();
    }
  });
});

/* ---- defect 16: the Decision Center never surfaces the next approval ---- */

describe("defect 16 — a centre view re-reads Core after a detour", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("re-mounts D2 after DiffReview, so a second approval is read rather than remembered", () => {
    const mounted: string[] = [];
    const { root, controller } = mount({
      secondaryViews: {
        mount: (route, container) => {
          mounted.push(route);
          container.textContent = `${route} #${mounted.length}`;
        },
      },
      workspaceDiff: {
        read: async () => dirtyDiffPage(),
        query: async () => dirtyDiffPage(),
      },
    });

    controller.openCenterView("d2");
    expect(mounted).toEqual(["d2"]);

    // The operator leaves for the commit bar and comes back — the exact path
    // E3a took when the decided card would not refresh.
    controller.openCenterView("review");
    controller.openCenterView("d2");

    expect(mounted).toEqual(["d2", "d2"]);
    expect(root.querySelector("[data-secondary-body='d2']")?.textContent).toContain("#2");
  });
});

/* ---- defect 17 + 21: the settled action and the page it invalidates ---- */

function completedCommit(): OperatorGitProjection {
  return {
    ...IDLE_OPERATOR_GIT,
    outcome: { state: "confirmed", reason: null },
    capabilityAvailable: true,
    ownerAvailable: true,
    result: {
      kind: "completed",
      action: "commit",
      targetLaneId: null,
      auditId: "audit-1",
      output: "[main a5d3d53] Edit the fixture README heading",
      truncated: false,
      source: {
        status: "ready",
        branch: "main",
        worktree: null,
        ahead: 0,
        behind: 0,
        added: 0,
        deleted: 0,
        dirty: false,
      },
      failureClass: null,
      detail: null,
    },
  };
}

function dirtyDiffPage(): WorkspaceDiffProjection {
  return {
    outcome: { state: "confirmed", reason: null },
    targetLaneId: null,
    source: {
      status: "ready",
      branch: "main",
      worktree: null,
      ahead: 0,
      behind: 0,
      added: 1,
      deleted: 1,
      dirty: true,
    },
    entries: [
      {
        path: "README.md",
        index: null,
        worktree: "modified",
        staged: false,
        diff: {
          path: "README.md",
          oldPath: null,
          kind: "modified",
          binary: false,
          omitted: false,
          additions: 1,
          deletions: 1,
          hunks: [],
        },
      },
    ],
    truncated: false,
    loaded: true,
    pendingCommandId: null,
    capabilityAvailable: true,
    stale: false,
  };
}

describe("defect 21 — a completed commit re-samples the review page", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("sends exactly one QueryWorkspaceDiff when Core finishes a commit", async () => {
    let state = { ...IDLE_OPERATOR_GIT, capabilityAvailable: true, ownerAvailable: true };
    const query = vi.fn(async () => dirtyDiffPage());
    const { controller } = mount({
      workspaceDiff: { read: async () => dirtyDiffPage(), query },
      operatorGit: {
        read: async () => state,
        run: async () => {
          state = completedCommit();
          return state;
        },
        poll: async () => state,
      },
    });

    controller.openCenterView("review");
    await flush();
    const before = query.mock.calls.length;

    const bar = document.querySelector<HTMLInputElement>("[data-review-commit-message]");
    expect(bar).not.toBeNull();
    bar!.value = "Edit the fixture README heading";
    bar!.dispatchEvent(new InputEvent("input", { bubbles: true }));
    document.querySelector<HTMLButtonElement>('[data-review-action="commit"]')!.click();
    await flush();
    await flush();

    expect(query.mock.calls.length).toBe(before + 1);
  });
});

/* ---- defect 20: the dock's two samples ---- */

function dockModel(overrides: Partial<ContextDockModel> = {}): ContextDockModel {
  const base = projection();
  base.contextDock.source = {
    status: "ready",
    branch: "main",
    worktree: null,
    ahead: 0,
    behind: 0,
    // The pre-edit sample E3a caught on screen beside a listed change.
    added: 0,
    deleted: 0,
    dirty: false,
  };
  return {
    projection: base,
    locale: "en",
    matchesSelectedLane: true,
    tab: "environment",
    onSelectTab: () => undefined,
    laneSource: null,
    diff: {
      bound: true,
      capabilityAvailable: true,
      projection: dirtyDiffPage(),
      selectedPath: null,
      expanded: new Set<string>(),
      onSelect: () => undefined,
      onToggle: () => undefined,
      onOpenInReview: () => undefined,
    },
    files: {
      bound: false,
      capabilityAvailable: false,
      pages: new Map(),
      expanded: new Set<string>(),
      selectedPath: null,
      onToggleDirectory: () => undefined,
      onSelect: () => undefined,
      fileReadsAvailable: false,
      onOpenFile: () => undefined,
      content: IDLE_WORKSPACE_FILE,
    },
    commit: { available: false, reason: null, onCommitOrPush: () => undefined },
    ...overrides,
  };
}

describe("defect 20 — Changes and Local read the sample the file list reads", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("the totals and the working-tree flag come from the loaded diff page", () => {
    const host = renderContextDock(dockModel());

    const summary = host.querySelector<HTMLElement>("[data-changes-summary]")!;
    expect(summary.textContent).toContain("+1");
    expect(summary.textContent).toContain("−1");
    expect(summary.dataset.changesSample).toBe("diff-page");
    const local = host.querySelector<HTMLElement>("[data-context-section='local']")!;
    expect(local.textContent).not.toContain("Clean");
  });
});

/* ---- defect 19: the tab strip's width budget ---- */

describe("defect 19 — the Lane tab strip never overlaps its own metadata", () => {
  test("every shrinkable slot in the strip declares its truncation rule", () => {
    const css = readFileSync(
      join(import.meta.dirname, "../src/components/lane_tabs.css"),
      "utf8",
    );
    const rule = (selector: string): string => {
      const start = css.indexOf(`${selector} {`);
      expect(start, `${selector} is not declared`).toBeGreaterThan(-1);
      return css.slice(start, css.indexOf("}", start));
    };

    // The strip clips, so anything inside it that can grow must also shrink
    // and ellipsise rather than paint over its neighbour.
    expect(rule(".d1-lane-tabs > .tab")).toContain("overflow: hidden");
    expect(rule(".d1-lane-tab-empty")).toContain("text-overflow: ellipsis");
    expect(rule(".d1-lane-tabs .tabmeta")).toContain("min-width: 0");
    expect(rule(".d1-lane-tabs .tabmeta")).not.toContain("flex: none");
    expect(rule(".d1-lane-tabs [data-lane-tab-project]")).toContain("text-overflow: ellipsis");
  });
});
