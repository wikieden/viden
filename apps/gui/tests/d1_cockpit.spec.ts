// @vitest-environment jsdom

import { beforeEach, describe, expect, test, vi } from "vitest";

import {
  renderD1Cockpit,
  type D1Intent,
  type D1IntentResult,
  type D1RenderOptions,
} from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

function setup(projection = D1_PROJECTION, options: D1RenderOptions = {}) {
  document.body.innerHTML = '<main id="app"></main>';
  const root = document.querySelector<HTMLElement>("#app");
  if (!root) throw new Error("missing root");
  const result: D1IntentResult = {
    projection,
    pendingCommandId: null,
    outcome: { state: "confirmed", reason: null },
  };
  const send = vi.fn(async (_intent: D1Intent) => result);
  const poll = vi.fn(async () => result);
  const controller = renderD1Cockpit(root, projection, send, poll, undefined, undefined, options);
  return { root, send, poll, controller };
}

const EMPTY_PROJECTION = {
  ...D1_PROJECTION,
  selectedLaneId: null,
  lanes: [],
  liveWork: { tasks: [], tools: [], approvals: [], queuedInputs: [], evidence: [] },
  transcript: [],
  composer: {
    editable: false,
    busy: false,
    canCancel: false,
    canSubmitImmediately: false,
  },
  recovery: {
    connection: "live" as const,
    state: "empty" as const,
    detail: null,
    hint: null,
    recoverable: false,
    businessSuccessBlocked: false,
    usedTokens: null,
    hardTokenLimit: null,
    missingCapabilities: ["runtime.recent_work"],
    actions: [],
  },
};

const PERMISSION_PROJECTION = {
  ...D1_PROJECTION,
  permissionDock: {
    workMode: "build",
    permissionLevel: "ask",
    request: {
      id: "permission-shell",
      toolName: "shell",
      title: "Permission request",
      message: "Run the focused GUI test.",
      inputPreview: "npm --prefix apps/gui test",
      isMutating: true,
      reason: "The command is outside the current allowlist.",
      risk: "high",
      target: { kind: "local", display: "viden", canonicalRef: null },
      policyReasonKey: "permission.command_not_allowlisted",
      policyReasonArgs: {},
      expiresAt: 0,
      defaultAction: "deny",
      auditId: "audit-shell",
      blockedByPlan: false,
      actions: [
        {
          kind: "once" as const,
          available: true,
          sessionId: null,
          paths: [],
          code: null,
        },
        {
          kind: "deny" as const,
          available: true,
          sessionId: null,
          paths: [],
          code: null,
        },
      ],
    },
  },
};

const RECOVERY_PROJECTION = {
  ...D1_PROJECTION,
  recovery: {
    ...D1_PROJECTION.recovery,
    connection: "disconnected" as const,
    state: "disconnected" as const,
    detail: "Core connection was interrupted.",
    hint: "Reconnect to restore the ordered event stream.",
    recoverable: true,
    businessSuccessBlocked: true,
    actions: [{ kind: "reconnect", available: true, code: "GUI-D6-RECONNECT" }],
  },
};

const CENTER_SEQUENCE_PROJECTION = {
  ...PERMISSION_PROJECTION,
  transcript: [
    {
      id: "user-refactor",
      kind: "user",
      content: "Refactor the config loader, then run focused tests.",
    },
    {
      id: "assistant-plan",
      kind: "assistant",
      content: "I will update src/config.rs behind the approval gate.",
    },
  ],
  contextDock: {
    ...D1_PROJECTION.contextDock,
    checklist: [
      {
        id: "change-config",
        kind: "workspace_change" as const,
        label: "src/config.rs",
        status: "modified",
        command: null,
        path: "src/config.rs",
        summary: null,
        failingLocation: null,
        additions: 1,
        deletions: 1,
        patch: "@@ src/config.rs 40-46 @@\n- let raw = fs::read_to_string(path).unwrap();\n+ let raw = fs::read_to_string(path)?;",
      },
      {
        id: "check-config",
        kind: "check_run" as const,
        label: "cargo test -p viden-cli config_tests",
        status: "failed",
        command: "cargo test -p viden-cli config_tests",
        path: null,
        summary: "failed · 101",
        failingLocation: "src/config.rs:42:15",
        additions: null,
        deletions: null,
        patch: null,
      },
    ],
  },
};

function shellLandmarks(root: HTMLElement): string[] {
  return Array.from(root.querySelectorAll<HTMLElement>("[data-shell-landmark]")).map(
    (landmark) => landmark.dataset.shellLandmark!,
  );
}

describe("D1 canonical streaming cockpit", () => {
  beforeEach(() => {
    document.documentElement.lang = "en";
  });

  test("exposes the transport-safe Context Dock on the D1 model", () => {
    expect(D1_PROJECTION.contextDock.laneAgent?.sessionId).toBe("session-lane-core");
  });

  test("renders activity, Lane, transcript, composer, Environment, and Live Work regions", () => {
    const { root } = setup();

    expect(root.querySelector('[data-screen="d1-cockpit"]')).not.toBeNull();
    expect(root.querySelector('nav[aria-label="Activity"]')).not.toBeNull();
    expect(root.querySelector('nav[aria-label="Lanes"]')).not.toBeNull();
    expect(root.querySelector('[aria-label="Transcript"]')?.textContent).toContain("Working");
    const transcript = root.querySelector('[aria-label="Transcript"]');
    expect(transcript?.getAttribute("role")).toBe("log");
    expect(transcript?.getAttribute("aria-live")).toBe("polite");
    expect(transcript?.getAttribute("aria-relevant")).toBe("additions text");
    expect(transcript?.getAttribute("aria-busy")).toBe("true");
    expect(root.querySelector('[aria-label="Environment"]')?.textContent).toContain("deepseek");
    expect(root.querySelector('[aria-label="Live Work"]')?.textContent).toContain("cargo");
    // Three, not four: the `audit` row went away when Core published the audit
    // timeline, and the dock never renders a gap Core has already closed.
    expect(root.querySelectorAll('[data-unavailable-feature]')).toHaveLength(3);
    expect(document.activeElement).toBe(root.querySelector("[data-composer]"));
  });

  test("shows completed ACP output instead of an unrelated stale agent-stopped recovery", () => {
    const projection = {
      ...D1_PROJECTION,
      agentSessions: [
        {
          sessionId: "session-lane-core",
          laneId: "lane-core",
          agentId: "codex-acp",
          model: null,
          status: "completed",
          task: "Return an exact response",
          diagnostic: null,
          output: "ACP-GUI-CLOSED-LOOP-OK",
        },
      ],
      composer: {
        ...D1_PROJECTION.composer,
        busy: false,
        // Native turn state may lag behind the selected ACP route.
        canCancel: true,
      },
      unavailableFeatures: [
        ...D1_PROJECTION.unavailableFeatures,
        {
          id: "transcript_user",
          available: false as const,
          code: "GUI-CORE-009",
          message: "Typed user prompt rows are unavailable.",
        },
        {
          id: "transcript_assistant",
          available: false as const,
          code: "GUI-CORE-009",
          message: "Owner-scoped assistant rows are unavailable.",
        },
      ],
      recovery: {
        ...D1_PROJECTION.recovery,
        state: "agent_stopped" as const,
        detail: "A previous execution stopped.",
        recoverable: true,
        businessSuccessBlocked: true,
      },
    };

    const { root, controller } = setup(projection, { poll: false });

    expect(root.querySelector("[data-d6-state='agent_stopped']")).toBeNull();
    expect(root.querySelector("[data-acp-output] pre")?.textContent).toBe(
      "ACP-GUI-CLOSED-LOOP-OK",
    );
    expect(root.querySelector("[data-cancel-turn]")).toBeNull();
    expect(root.querySelector('[data-unavailable-feature="transcript_user"]')).toBeNull();
    expect(root.querySelector('[data-unavailable-feature="transcript_assistant"]')).toBeNull();
    controller.dispose();
  });

  test("keeps recovery visible when the selected ACP session itself failed", () => {
    const projection = {
      ...D1_PROJECTION,
      agentSessions: [
        {
          sessionId: "session-lane-core",
          laneId: "lane-core",
          agentId: "codex-acp",
          model: null,
          status: "failed",
          task: "Return an exact response",
          diagnostic: "ACP transport stopped",
          output: null,
        },
      ],
      recovery: {
        ...D1_PROJECTION.recovery,
        state: "agent_stopped" as const,
        detail: "ACP transport stopped",
        recoverable: true,
        businessSuccessBlocked: true,
      },
    };

    const { root, controller } = setup(projection, { poll: false });

    expect(root.querySelector("[data-d6-state='agent_stopped']")).not.toBeNull();
    expect(root.querySelector("[data-acp-output]")).toBeNull();
    controller.dispose();
  });

  test("renders the selected Lane work surface in typed center sequence with semantic landmarks", () => {
    const { root, controller } = setup(CENTER_SEQUENCE_PROJECTION, { poll: false });
    const sequence = root.querySelector<HTMLElement>("[data-center-sequence]");

    expect(root.querySelector("[data-lane-work-surface]")?.getAttribute("aria-label")).toBe(
      "Lane work surface",
    );
    expect(
      Array.from(sequence?.querySelectorAll<HTMLElement>("[data-center-step]") ?? []).map(
        (element) => element.dataset.centerStep,
      ),
    ).toEqual(["user", "assistant", "workspace-change", "check-run", "live-work"]);
    expect(sequence?.querySelector('[data-transcript-row="user"]')?.textContent).toContain(
      "Refactor the config loader",
    );
    expect(sequence?.querySelector('[data-workspace-change="change-config"]')?.textContent).toContain(
      "let raw = fs::read_to_string(path)?;",
    );
    expect(sequence?.querySelector('[data-check-run="check-config"]')?.textContent).toContain(
      "src/config.rs:42:15",
    );
    expect(sequence?.querySelector("[data-live-work-bar]")?.getAttribute("role")).toBe("status");
    expect(root.querySelector("[data-permission-dock]")).not.toBeNull();
    expect(root.querySelector("[data-composer-region] [data-composer]")).not.toBeNull();
    controller.dispose();
  });

  test("renders the one-Agent Context Dock in typed collapsible order", () => {
    const projection = {
      ...D1_PROJECTION,
      contextDock: {
        ...D1_PROJECTION.contextDock,
        source: null,
        context: null,
        services: [
          {
            id: "github-mcp",
            kind: "mcp" as const,
            label: "GitHub MCP",
            status: "unavailable" as const,
            detailKey: "transport_missing",
          },
          {
            id: "rust-analyzer",
            kind: "lsp" as const,
            label: "rust-analyzer",
            status: "offline" as const,
            detailKey: null,
          },
        ],
        checklist: [],
      },
    };
    const { root, controller } = setup(projection, { poll: false });
    const sections = Array.from(
      root.querySelectorAll<HTMLElement>("[data-context-section]"),
      (section) => section.dataset.contextSection,
    );

    expect(sections).toEqual([
      "environment",
      "changes-source",
      "context",
      "lane-agent",
      "sources",
      "mcp",
      "lsp",
      "task-checklist",
    ]);
    expect(root.querySelectorAll("[data-lane-agent]")).toHaveLength(1);
    expect(root.querySelector("[data-lane-agent]")?.textContent).toContain("lane-core");
    expect(root.querySelector("[data-lane-agent]")?.textContent).toContain("Native");
    expect(root.querySelector("[data-lane-agent]")?.textContent).toContain("deepseek-v4-flash");
    expect(root.querySelector("[data-lane-agent]")?.textContent).toContain("Running");
    expect(root.querySelector('[data-typed-empty="source"]')?.textContent).toBe(
      "No source facts are available.",
    );
    expect(root.querySelector('[data-typed-empty="context"]')?.textContent).toBe(
      "No typed context budget is available.",
    );
    expect(root.querySelector("[data-context-section='mcp']")?.textContent).toContain(
      "Unavailable",
    );
    expect(root.querySelector("[data-context-section='mcp']")?.textContent).not.toContain(
      "Connected",
    );
    expect(root.querySelector("[data-context-section='lsp']")?.textContent).toContain("Offline");
    expect(root.querySelector('[data-typed-empty="task-checklist"]')?.textContent).toBe(
      "No task checklist is available.",
    );
    for (const button of root.querySelectorAll<HTMLButtonElement>("[data-context-section-toggle]")) {
      expect(button.getAttribute("aria-expanded")).toBe("true");
      button.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
      expect(button.getAttribute("aria-expanded")).toBe("false");
      button.dispatchEvent(new KeyboardEvent("keydown", { key: " ", bubbles: true }));
      expect(button.getAttribute("aria-expanded")).toBe("true");
    }
    expect(root.textContent).not.toContain("Subagents");
    expect(root.textContent).not.toContain("GUI-CORE-");
    controller.dispose();
  });

  test("localizes non-empty zh-CN Context Dock facts without raw diagnostic accessibility copy", () => {
    const { root, controller } = setup(
      {
        ...D1_PROJECTION,
        preferences: { ...D1_PROJECTION.preferences, locale: "zh-CN" },
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
          checklist: [
            {
              id: "change-zh",
              kind: "workspace_change" as const,
              label: "src/main.rs",
              status: "modified",
              command: null,
              path: "src/main.rs",
              summary: null,
              failingLocation: null,
              additions: 2,
              deletions: 1,
            },
          ],
        },
      },
      { poll: false },
    );

    const contextDock = root.querySelector<HTMLElement>("[data-context-dock]")!;
    expect(contextDock.textContent).toContain("执行身份");
    expect(contextDock.textContent).toContain("领先");
    expect(contextDock.textContent).toContain("落后");
    expect(contextDock.textContent).toContain("有变更");
    expect(contextDock.textContent).toContain("预算");
    expect(contextDock.textContent).toContain("剩余");
    expect(contextDock.querySelector("[data-checklist-item='change-zh']")?.textContent).toContain(
      "已修改",
    );
    expect(contextDock.textContent).not.toContain("Ahead");
    expect(contextDock.textContent).not.toContain("Behind");
    expect(contextDock.textContent).not.toContain("Dirty");
    expect(contextDock.textContent).not.toContain("Budget");
    expect(contextDock.textContent).not.toContain("Remaining");
    expect(contextDock.textContent).not.toContain("Lane");
    for (const row of contextDock.querySelectorAll<HTMLElement>("[data-unavailable-feature]")) {
      expect(row.textContent).not.toContain("GUI-CORE-");
      expect(row.getAttribute("title")).toBeNull();
      expect(row.getAttribute("aria-label") ?? "").not.toContain("GUI-CORE-");
    }
    controller.dispose();
  });

  test("renders localized typed empty states when a change patch or check result is absent", () => {
    const { root, controller } = setup(
      {
        ...CENTER_SEQUENCE_PROJECTION,
        contextDock: {
          ...CENTER_SEQUENCE_PROJECTION.contextDock,
          checklist: CENTER_SEQUENCE_PROJECTION.contextDock.checklist.map((item) =>
            item.kind === "workspace_change"
              ? { ...item, patch: null, additions: null, deletions: null }
              : { ...item, command: null, summary: "", failingLocation: null },
          ),
        },
      },
      { poll: false },
    );

    expect(root.querySelector('[data-typed-empty="workspace-change-patch"]')?.textContent).toBe(
      "No typed patch is available.",
    );
    expect(root.querySelector('[data-typed-empty="check-run-result"]')?.textContent).toBe(
      "No typed check result is available.",
    );
    expect(root.querySelector('[data-typed-empty="check-run-command"]')?.textContent).toBe(
      "No typed check command is available.",
    );
    expect(root.querySelector(".d1-work-card-meta")).toBeNull();
    controller.dispose();
  });

  test("bounds typed checklist cards and every live-work collection", () => {
    const checklist = Array.from({ length: 30 }, (_, index) => ({
      ...CENTER_SEQUENCE_PROJECTION.contextDock.checklist[0]!,
      id: `change-${index}`,
      label: `src/${index}.ts`,
    }));
    const workItems = Array.from({ length: 30 }, (_, index) => ({
      id: `task-${index}`,
      title: `Task ${index}`,
      status: "running",
      progress: index,
    }));
    const { root, controller } = setup(
      {
        ...CENTER_SEQUENCE_PROJECTION,
        contextDock: { ...CENTER_SEQUENCE_PROJECTION.contextDock, checklist },
        liveWork: {
          tasks: workItems,
          tools: Array.from({ length: 30 }, (_, index) => ({
            id: `tool-${index}`,
            name: "shell",
            inputPreview: `command ${index}`,
            state: "running",
          })),
          approvals: Array.from({ length: 30 }, (_, index) => ({
            id: `approval-${index}`,
            title: `Approval ${index}`,
            risk: "high",
          })),
          queuedInputs: Array.from({ length: 30 }, (_, index) => ({
            id: `queued-${index}`,
            contentPreview: `queued ${index}`,
          })),
          evidence: Array.from({ length: 30 }, (_, index) => ({
            id: `evidence-${index}`,
            kind: "test",
            summary: `evidence ${index}`,
            path: null,
          })),
        },
      },
      { poll: false },
    );

    expect(root.querySelectorAll(".d1-work-card")).toHaveLength(24);
    expect(root.querySelector("[data-live-work-primary]")?.textContent).toBe("Task 0");
    expect(root.querySelector("[data-live-work-secondary]")?.textContent).toContain("Running");
    expect(root.querySelector("[data-live-work-secondary]")?.textContent).not.toContain("Approval 0");
    expect(root.querySelector("[data-live-work-secondary]")?.textContent).not.toContain("queued 0");
    expect(root.querySelector("[data-live-work-secondary]")?.textContent).not.toContain("Task 24");
    expect(root.querySelectorAll(".d1-right .d1-work-item")).toHaveLength(24);
    controller.dispose();
  });

  test("disables Send when the selected Lane has no sole owner", () => {
    const { root, send, controller } = setup(
      {
        ...D1_PROJECTION,
        composer: { ...D1_PROJECTION.composer, editable: false },
        contextDock: { ...D1_PROJECTION.contextDock, laneAgent: null },
      },
      { poll: false },
    );
    const composer = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
    expect(composer.disabled).toBe(true);
    expect(root.querySelector<HTMLButtonElement>("[data-composer-send]")?.disabled).toBe(true);
    composer.value = "keep this draft";
    composer.dispatchEvent(new InputEvent("input", { bubbles: true }));
    composer.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    expect(send).not.toHaveBeenCalled();
    expect(root.querySelector("[data-mutation-blocked]")?.textContent).toBe(
      "The selected Lane has no sole Core execution owner.",
    );
    expect(root.querySelector("[data-cancel-turn]")).toBeNull();
    controller.dispose();
  });

  test("blocks duplicate ACP sessions instead of selecting the first session", () => {
    const duplicateSessions = ["acp-one", "acp-two"].map((sessionId) => ({
      sessionId,
      laneId: "lane-core",
      agentId: "codex-acp",
      model: null,
      status: "running",
      task: "review",
      diagnostic: null,
    }));
    const { root, send, controller } = setup(
      { ...D1_PROJECTION, agentSessions: duplicateSessions },
      { poll: false },
    );
    const composer = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
    composer.value = "do not guess";
    composer.dispatchEvent(new InputEvent("input", { bubbles: true }));
    composer.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    expect(send).not.toHaveBeenCalled();
    expect(root.querySelector("[data-mutation-blocked]")?.textContent).toBe(
      "The selected Lane has duplicate Agent sessions; recover from Core state.",
    );
    controller.dispose();
  });

  test("keeps a removed selected Lane stale instead of retargeting a follow-up or cancel", () => {
    const reviewLane = { ...D1_PROJECTION.lanes[0]!, id: "lane-review", role: "reviewer" };
    const { root, send, controller } = setup(
      { ...D1_PROJECTION, lanes: [D1_PROJECTION.lanes[0]!, reviewLane] },
      { poll: false },
    );
    controller.applyProjection({
      ...D1_PROJECTION,
      selectedLaneId: "lane-core",
      lanes: [reviewLane],
      contextDock: { ...D1_PROJECTION.contextDock, laneAgent: null },
    });
    const composer = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
    composer.value = "never retarget";
    composer.dispatchEvent(new InputEvent("input", { bubbles: true }));
    composer.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    expect(send).not.toHaveBeenCalled();
    expect(root.querySelector("[data-mutation-blocked]")?.textContent).toBe(
      "The selected Lane is no longer available in Core state.",
    );
    expect(root.querySelector('[data-lane-id="lane-review"]')?.getAttribute("aria-current")).toBe(
      "false",
    );
    expect(root.querySelector("[data-cancel-turn]")).toBeNull();
    controller.dispose();
  });

  test("cross-checks the sole ACP session against the authoritative owner session", () => {
    const { root, send, controller } = setup(
      {
        ...D1_PROJECTION,
        contextDock: {
          ...D1_PROJECTION.contextDock,
          laneAgent: { ...D1_PROJECTION.contextDock.laneAgent!, sessionId: "owner-session" },
        },
        agentSessions: [
          {
            sessionId: "other-session",
            laneId: "lane-core",
            agentId: "codex-acp",
            model: null,
            status: "running",
            task: "review",
            diagnostic: null,
          },
        ],
      },
      { poll: false },
    );
    const composer = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
    composer.value = "never guess session";
    composer.dispatchEvent(new InputEvent("input", { bubbles: true }));
    composer.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    expect(send).not.toHaveBeenCalled();
    expect(root.querySelector("[data-mutation-blocked]")?.textContent).toBe(
      "The selected Lane session does not match its Core owner.",
    );
    controller.dispose();
  });

  test("queues a busy ACP follow-up through the selected Lane owner instead of sending ACP input", () => {
    const { root, send, controller } = setup(
      {
        ...D1_PROJECTION,
        contextDock: {
          ...D1_PROJECTION.contextDock,
          laneAgent: { ...D1_PROJECTION.contextDock.laneAgent!, sessionId: "acp-owner" },
        },
        agentSessions: [
          {
            sessionId: "acp-owner",
            laneId: "lane-core",
            agentId: "codex-acp",
            model: null,
            status: "running",
            task: "review",
            diagnostic: null,
          },
        ],
        composer: { ...D1_PROJECTION.composer, busy: true, canSubmitImmediately: false },
      },
      { poll: false },
    );
    const composer = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
    composer.value = "queue this ACP follow-up";
    composer.dispatchEvent(new InputEvent("input", { bubbles: true }));
    composer.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    expect(send).toHaveBeenCalledWith({
      type: "submit",
      laneId: "lane-core",
      content: "queue this ACP follow-up",
    });
    controller.dispose();
  });

  test("renders typed unavailable transcript roles rather than inventing user or assistant rows", () => {
    const { root, controller } = setup(
      {
        ...D1_PROJECTION,
        transcript: [{ id: "lane-output", kind: "lane_output", content: "typed lane output" }],
        unavailableFeatures: [
          {
            id: "transcript_user",
            available: false,
            code: "GUI-CORE-009",
            message: "Typed user prompt rows are unavailable.",
          },
          {
            id: "transcript_assistant",
            available: false,
            code: "GUI-CORE-009",
            message: "Owner-scoped assistant rows are unavailable.",
          },
        ],
      },
      { poll: false },
    );

    expect(root.querySelector('[data-typed-empty="transcript-user"]')?.textContent).toBe(
      "Typed user prompt rows are unavailable.",
    );
    expect(root.querySelector('[data-typed-empty="transcript-assistant"]')?.textContent).toBe(
      "Owner-scoped assistant rows are unavailable.",
    );
    expect(root.querySelector('[data-transcript-row="user"]')).toBeNull();
    expect(root.querySelector('[data-transcript-row="assistant"]')).toBeNull();
    controller.dispose();
  });

  test("puts localized unavailable user and assistant placeholders before selected-Lane output", () => {
    const { root, controller } = setup(
      {
        ...D1_PROJECTION,
        preferences: { ...D1_PROJECTION.preferences, locale: "zh-CN" },
        transcript: [{ id: "lane-output", kind: "lane_output", content: "已类型化输出" }],
        unavailableFeatures: [
          {
            id: "transcript_user",
            available: false,
            code: "GUI-CORE-009",
            message: "ignored source message",
          },
          {
            id: "transcript_assistant",
            available: false,
            code: "GUI-CORE-009",
            message: "ignored source message",
          },
        ],
      },
      { poll: false },
    );
    const sequence = Array.from(
      root.querySelectorAll<HTMLElement>("[data-center-sequence] [data-center-step]"),
      (element) => element.dataset.centerStep,
    );
    expect(sequence.slice(0, 2)).toEqual(["user", "assistant"]);
    expect(root.querySelector('[data-typed-empty="transcript-user"]')?.textContent).toBe(
      "已类型化的用户提示行不可用。",
    );
    expect(root.querySelector('[data-typed-empty="transcript-assistant"]')?.textContent).toBe(
      "按 Owner 范围限定的助手行不可用。",
    );
    controller.dispose();
  });

  test("clears old Lane transcript rows while waiting for the selected Lane projection", () => {
    const reviewLane = { ...D1_PROJECTION.lanes[0]!, id: "lane-review", role: "reviewer" };
    const { root, controller } = setup(
      { ...D1_PROJECTION, lanes: [D1_PROJECTION.lanes[0]!, reviewLane] },
      { poll: false },
    );
    root.querySelector<HTMLButtonElement>('[data-lane-id="lane-review"]')?.click();

    expect(root.querySelector('[data-row-id="stream"]')).toBeNull();
    expect(root.querySelector(".d1-work-card")).toBeNull();
    expect(root.querySelector("[data-live-work-bar]")).toBeNull();
    expect(root.querySelector("[data-context-dock-waiting]")?.textContent).toBe(
      "Waiting for the selected Lane context.",
    );
    expect(root.querySelector('[data-typed-empty="transcript-switching"]')?.textContent).toBe(
      "Waiting for the selected Lane transcript.",
    );
    controller.dispose();
  });

  test("keeps the canonical D1 activity rail destinations in design order", () => {
    const { root } = setup();
    const activity = root.querySelector('nav[aria-label="Activity"]')!;

    expect(
      Array.from(activity.querySelectorAll("button"), (button) =>
        button.getAttribute("aria-label"),
      ),
    ).toEqual([
      // The `D-RAILNAV` order: the conversation, the Lane sidebar, then every
      // registered destination, then the Settings gear below the spacer. Each
      // slot names the screen it actually opens, not a file-explorer verb it
      // borrowed from another editor's rail — and a destination whose Core
      // capability is missing says so in the name a screen reader announces
      // rather than only in a tooltip.
      "Conversation",
      "Lanes",
      "Diff review — unavailable: Core publishes no runtime.structured_diff",
      "Evidence — unavailable: Core publishes no runtime.evidence_reads",
      "Decisions",
      "Lane monitor",
      "Integration gate",
      "Fleet board",
      "Audit timeline — falls back to raw event replay when Core publishes no runtime.audit",
      "Settings",
    ]);
  });

  test("opens and dismisses the floating Lane rail from its keyboard-operable activity control", () => {
    const { root } = setup();
    const toggle = root.querySelector<HTMLButtonElement>("[data-lanes-toggle]")!;

    expect(toggle.disabled).toBe(false);
    expect(toggle.getAttribute("aria-controls")).toBe("d1-lane-rail");
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(root.querySelector("#d1-lane-rail")?.getAttribute("data-open")).toBe("false");

    toggle.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    const openedRail = root.querySelector<HTMLElement>("#d1-lane-rail")!;
    expect(
      root.querySelector<HTMLButtonElement>("[data-lanes-toggle]")?.getAttribute("aria-expanded"),
    ).toBe("true");
    expect(openedRail.dataset.open).toBe("true");
    expect(document.activeElement).toBe(openedRail.querySelector("[data-create-lane]"));

    openedRail.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));

    expect(
      root.querySelector<HTMLButtonElement>("[data-lanes-toggle]")?.getAttribute("aria-expanded"),
    ).toBe("false");
    expect(root.querySelector("#d1-lane-rail")?.getAttribute("data-open")).toBe("false");
    expect(document.activeElement).toBe(root.querySelector("[data-lanes-toggle]"));

    root
      .querySelector<HTMLButtonElement>("[data-lanes-toggle]")
      ?.dispatchEvent(new KeyboardEvent("keydown", { key: " ", bubbles: true }));
    expect(
      root.querySelector<HTMLButtonElement>("[data-lanes-toggle]")?.getAttribute("aria-expanded"),
    ).toBe("true");
  });

  test.each([
    ["no workspace", EMPTY_PROJECTION, { onOpenProject: vi.fn(), poll: false }],
    ["zero Lane", EMPTY_PROJECTION, { onCreateLane: vi.fn(), poll: false }],
    ["active Lane", D1_PROJECTION, { poll: false }],
    ["pending approval", PERMISSION_PROJECTION, { poll: false }],
    ["typed recovery", RECOVERY_PROJECTION, { poll: false }],
  ])("pins the persistent D1 landmark order in %s", (_label, projection, options) => {
    const { root, controller } = setup(projection, options);

    expect(shellLandmarks(root)).toEqual(
      expect.arrayContaining(["topbar", "activity-rail", "lane-work-surface", "statusbar"]),
    );
    expect(
      shellLandmarks(root).indexOf("topbar"),
    ).toBeLessThan(shellLandmarks(root).indexOf("activity-rail"));
    expect(
      shellLandmarks(root).indexOf("activity-rail"),
    ).toBeLessThan(shellLandmarks(root).indexOf("lane-work-surface"));
    expect(
      shellLandmarks(root).indexOf("lane-work-surface"),
    ).toBeLessThan(shellLandmarks(root).indexOf("statusbar"));
    controller.dispose();
  });

  test.each([
    ["zero Lane", EMPTY_PROJECTION],
    ["active Lane", D1_PROJECTION],
    ["pending approval", PERMISSION_PROJECTION],
    ["typed recovery", RECOVERY_PROJECTION],
  ])("keeps work, permission, composer, and status regions structurally separate in %s", (
    _label,
    projection,
  ) => {
    const { root, controller } = setup(projection, {
      onCreateLane: vi.fn(),
      poll: false,
    });
    const surface = root.querySelector<HTMLElement>("[data-lane-work-surface]");

    expect(surface?.querySelector("[data-work-surface]")).not.toBeNull();
    expect(surface?.querySelector("[data-permission-region]")).not.toBeNull();
    expect(surface?.querySelector("[data-composer-region]")).not.toBeNull();
    expect(surface?.querySelector("[data-permission-region] [data-composer]")).toBeNull();
    expect(root.querySelector("[data-statusbar]")).not.toBeNull();
    controller.dispose();
  });

  test.each([
    [1440, "desktop"],
    [1280, "desktop"],
    [960, "narrow"],
  ])("publishes non-overlapping cockpit grid roles at %ipx", (width, layout) => {
    Object.defineProperty(window, "innerWidth", { configurable: true, value: width });
    const { root, controller } = setup(D1_PROJECTION, { poll: false });
    const body = root.querySelector<HTMLElement>("[data-cockpit-grid]");

    expect(body?.dataset.cockpitLayout).toBe(layout);
    expect(
      Array.from(body?.children ?? []).map(
        (child) => (child as HTMLElement).dataset.cockpitRole,
      ),
    ).toEqual(["activity", "lanes", "work", "context"]);
    expect(root.querySelector("[data-native-window-shell]")).not.toBeNull();
    expect(root.querySelector("[data-browser-page-frame]")).toBeNull();
    controller.dispose();
  });

  test("keeps Context Dock facts in a keyboard-focusable narrow-width drawer", () => {
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 960 });
    const { root, controller } = setup(D1_PROJECTION, { poll: false });
    const toggle = root.querySelector<HTMLButtonElement>("[data-context-drawer-toggle]");
    const dock = root.querySelector<HTMLElement>("[data-context-dock]");

    expect(toggle?.type).toBe("button");
    expect(toggle?.getAttribute("aria-controls")).toBe(dock?.id);
    expect(toggle?.getAttribute("aria-expanded")).toBe("false");
    expect(dock?.textContent).toContain("deepseek");
    toggle?.focus();
    expect(document.activeElement).toBe(toggle);
    toggle?.click();
    expect(toggle?.getAttribute("aria-expanded")).toBe("true");
    expect(dock?.dataset.drawerOpen).toBe("true");
    toggle?.click();
    expect(toggle?.getAttribute("aria-expanded")).toBe("false");
    expect(dock?.dataset.drawerOpen).toBe("false");
    controller.dispose();
  });

  test("turns the empty D1 workspace into a branded welcome center with only real entry points", () => {
    const onOpenProject = vi.fn();
    const { root, controller } = setup(EMPTY_PROJECTION, { onOpenProject, poll: false });

    const welcome = root.querySelector<HTMLElement>('[data-d1-welcome]');
    expect(welcome?.getAttribute("aria-label")).toBe("Welcome to Viden");
    expect(welcome?.querySelector<HTMLImageElement>("img")?.src).toContain("image/svg+xml");
    expect(welcome?.querySelector<HTMLImageElement>("img")?.alt).toBe("");
    expect(welcome?.textContent).toContain("Your local-first agent workspace");
    expect(welcome?.textContent).toContain("Get started");
    expect(welcome?.textContent).toContain("Recent projects");
    // No `loadRecentWork` port is bound here, so the Recent section names the
    // exact Core capability it is waiting on instead of showing an empty list
    // that would read as "you have no history".
    expect(welcome?.textContent).toContain("Recent project history is unavailable");
    expect(welcome?.textContent).toContain("runtime.recent_work");
    expect(welcome?.textContent).not.toContain("GUI-CORE-");
    expect(root.querySelector('[aria-label="Transcript"]')).toBeNull();
    expect(root.querySelector("[data-composer]")).toBeNull();
    expect(root.querySelector(".d1-lanes")).toBeNull();
    expect(root.querySelector(".d1-right")).toBeNull();

    root.querySelector<HTMLButtonElement>("[data-open-project]")?.click();
    expect(onOpenProject).toHaveBeenCalledTimes(1);
    controller.dispose();
  });

  test("treats an opened project with no Lanes as a project cockpit, not as welcome", () => {
    const onCreateLane = vi.fn();
    const { root, controller } = setup(
      EMPTY_PROJECTION,
      { onCreateLane, poll: false } as unknown as D1RenderOptions,
    );

    expect(root.querySelector("[data-d1-welcome]")).toBeNull();
    const createLane = root.querySelector<HTMLButtonElement>("[data-create-lane]");
    expect(createLane).not.toBeNull();
    createLane?.click();
    expect(onCreateLane).toHaveBeenCalledTimes(1);
    controller.dispose();
  });

  test("opens project from the visible shortcut and removes it when the cockpit is disposed", () => {
    const onOpenProject = vi.fn();
    const { controller } = setup(EMPTY_PROJECTION, { onOpenProject, poll: false });

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "o", metaKey: true }));
    expect(onOpenProject).toHaveBeenCalledTimes(1);

    controller.dispose();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "o", metaKey: true }));
    expect(onOpenProject).toHaveBeenCalledTimes(1);
  });

  test("localizes the welcome center from Core-owned presentation preferences", () => {
    const { root, controller } = setup(
      {
        ...EMPTY_PROJECTION,
        preferences: { ...EMPTY_PROJECTION.preferences, locale: "zh-CN" },
      },
      { onOpenProject: vi.fn(), poll: false },
    );

    expect(root.querySelector('[data-d1-welcome]')?.getAttribute("aria-label")).toBe(
      "欢迎使用 Viden",
    );
    expect(root.textContent).toContain("本地优先的智能开发工作区");
    expect(root.textContent).toContain("打开项目");
    expect(root.textContent).toContain("最近项目");
    controller.dispose();
  });

  test("keeps composer editable while busy and Enter queues through one typed intent", () => {
    const { root, send } = setup();
    const composer = root.querySelector<HTMLTextAreaElement>("[data-composer]");
    if (!composer) throw new Error("missing composer");
    expect(composer.disabled).toBe(false);
    composer.value = "继续验证";
    composer.dispatchEvent(new InputEvent("input", { bubbles: true }));
    composer.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    expect(send).toHaveBeenCalledWith({
      type: "submit",
      laneId: "lane-core",
      content: "继续验证",
    });
  });

  test("renders a visible Send action that uses the same exact-owner queue path", () => {
    const { root, send, controller } = setup({
      ...D1_PROJECTION,
      composer: { ...D1_PROJECTION.composer, busy: true, canSubmitImmediately: false },
    });
    const composer = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
    composer.value = "queue from visible Send";
    composer.dispatchEvent(new InputEvent("input", { bubbles: true }));

    const submit = root.querySelector<HTMLButtonElement>("[data-composer-send]")!;
    expect(submit.textContent).toBe("Send");
    expect(submit.disabled).toBe(false);
    submit.click();

    expect(send).toHaveBeenCalledWith({
      type: "submit",
      laneId: "lane-core",
      content: "queue from visible Send",
    });
    expect(root.querySelector("[data-cancel-turn]")).not.toBeNull();
    controller.dispose();
  });

  test("restores the sole ACP Agent as the Lane conversation", () => {
    const session = {
      sessionId: "acp-restored",
      laneId: "lane-core",
      agentId: "codex-acp",
      model: null,
      status: "running",
      task: "continue the review",
      diagnostic: null,
    };
    const { root, send } = setup({
      ...D1_PROJECTION,
      contextDock: {
        ...D1_PROJECTION.contextDock,
        laneAgent: { ...D1_PROJECTION.contextDock.laneAgent!, sessionId: "acp-restored" },
      },
      composer: { ...D1_PROJECTION.composer, busy: false, canSubmitImmediately: true },
      agentSessions: [session],
    });
    const composer = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
    composer.value = "continue";
    composer.dispatchEvent(new InputEvent("input", { bubbles: true }));
    composer.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    expect(send).toHaveBeenCalledWith({
      type: "send_agent_session_input",
      laneId: "lane-core",
      sessionId: "acp-restored",
      content: "continue",
    });
    expect(root.querySelector("[data-acp-status] pre")?.textContent).toBe(
      "codex-acp · running",
    );
  });

  test("renders the complete Core-owned ACP conversation in dialogue order", () => {
    const session = {
      sessionId: "acp-conversation",
      laneId: "lane-core",
      agentId: "codex-acp",
      model: null,
      status: "completed",
      task: "second question",
      diagnostic: null,
      output: "second answer",
      conversation: [
        { messageId: "message-1", role: "user" as const, content: "first question" },
        { messageId: "message-2", role: "assistant" as const, content: "first answer" },
        { messageId: "message-3", role: "user" as const, content: "second question" },
        { messageId: "message-4", role: "assistant" as const, content: "second answer" },
      ],
    };
    const { root, controller } = setup({
      ...D1_PROJECTION,
      contextDock: {
        ...D1_PROJECTION.contextDock,
        laneAgent: { ...D1_PROJECTION.contextDock.laneAgent!, sessionId: session.sessionId },
      },
      agentSessions: [session],
    });

    expect(
      Array.from(root.querySelectorAll<HTMLElement>("[data-acp-message-id]")).map(
        (row) => [row.dataset.transcriptRow, row.textContent],
      ),
    ).toEqual([
      ["user", expect.stringContaining("first question")],
      ["assistant", expect.stringContaining("first answer")],
      ["user", expect.stringContaining("second question")],
      ["assistant", expect.stringContaining("second answer")],
    ]);
    controller.dispose();
  });

  test("does not repeat completed ACP dialogue from reconstructed transcript rows", () => {
    const session = {
      sessionId: "acp-reconstructed",
      laneId: "lane-core",
      agentId: "codex-acp",
      model: null,
      status: "completed",
      task: "first question",
      diagnostic: null,
      output: "first answer",
      conversation: [
        { messageId: "message-1", role: "user" as const, content: "first question" },
        {
          messageId: "message-2",
          role: "assistant" as const,
          content: "diagnostic prelude\n\nfirst answer",
        },
      ],
    };
    const { root, controller } = setup({
      ...D1_PROJECTION,
      contextDock: {
        ...D1_PROJECTION.contextDock,
        laneAgent: { ...D1_PROJECTION.contextDock.laneAgent!, sessionId: session.sessionId },
      },
      agentSessions: [session],
      transcript: [
        { id: "replayed-answer", kind: "assistant_stream", content: "first answer" },
        { id: "lane-output", kind: "lane_output", content: "prepared worktree" },
      ],
    });

    expect(root.querySelectorAll('[data-transcript-row="assistant"]')).toHaveLength(1);
    expect(root.querySelector('[data-row-id="lane-output"]')?.textContent).toContain(
      "prepared worktree",
    );
    controller.dispose();
  });

  test("queues a completed ACP follow-up until the Core command slot is released", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const session = {
      sessionId: "acp-completed",
      laneId: "lane-core",
      agentId: "codex-acp",
      model: null,
      status: "completed",
      task: "inspect the project",
      diagnostic: null,
    };
    const projection = {
      ...D1_PROJECTION,
      contextDock: {
        ...D1_PROJECTION.contextDock,
        laneAgent: { ...D1_PROJECTION.contextDock.laneAgent!, sessionId: session.sessionId },
      },
      composer: { ...D1_PROJECTION.composer, busy: false, canSubmitImmediately: true },
      agentSessions: [session],
    };
    const result: D1IntentResult = {
      projection,
      pendingCommandId: null,
      outcome: { state: "confirmed", reason: null },
    };
    const send = vi.fn(async (_intent: D1Intent) => result);
    const poll = vi.fn(async () => result);
    const controller = renderD1Cockpit(
      root,
      projection,
      send,
      poll,
      undefined,
      undefined,
      { poll: false },
    );
    controller.applyResult({
      projection,
      pendingCommandId: "command-finishing",
      outcome: { state: "pending", reason: null },
    });

    const composer = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
    composer.value = "hello after completion";
    composer.dispatchEvent(new InputEvent("input", { bubbles: true }));
    root.querySelector<HTMLButtonElement>("[data-composer-send]")!.click();
    expect(send).not.toHaveBeenCalled();

    controller.applyResult(result);

    await vi.waitFor(() =>
      expect(send).toHaveBeenCalledWith({
        type: "send_agent_session_input",
        laneId: "lane-core",
        sessionId: "acp-completed",
        content: "hello after completion",
      }),
    );
    controller.dispose();
  });

  test("Cancel is offered only from typed canCancel state", () => {
    const enabled = setup();
    enabled.root.querySelector<HTMLButtonElement>("[data-cancel-turn]")?.click();
    expect(enabled.send).toHaveBeenCalledWith({ type: "cancel", laneId: "lane-core" });

    const disabled = setup({
      ...D1_PROJECTION,
      composer: { ...D1_PROJECTION.composer, canCancel: false },
    });
    expect(disabled.root.querySelector("[data-cancel-turn]")).toBeNull();
  });

  test("focuses the exact Lane selected by the D4 receipt", () => {
    const { root } = setup();
    const selected = root.querySelector<HTMLElement>('[data-lane-id="lane-core"]');
    expect(selected?.getAttribute("aria-current")).toBe("true");
  });

  test("supports keyboard-only Lane rail traversal without changing Core facts", () => {
    const projection = {
      ...D1_PROJECTION,
      lanes: [
        D1_PROJECTION.lanes[0]!,
        { ...D1_PROJECTION.lanes[0]!, id: "lane-review", role: "reviewer" },
      ],
    };
    const { root } = setup(projection);
    const first = root.querySelector<HTMLElement>('[data-lane-id="lane-core"]')!;
    const second = root.querySelector<HTMLElement>('[data-lane-id="lane-review"]')!;
    first.focus();
    first.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    expect(document.activeElement).toBe(second);
    expect(second.getAttribute("aria-current")).toBe("false");
  });

  test("streaming projection refresh preserves composer draft, focus, and selection", () => {
    const { root, controller } = setup();
    const composer = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
    composer.value = "keep this draft";
    composer.dispatchEvent(new InputEvent("input", { bubbles: true }));
    composer.focus();
    composer.setSelectionRange(5, 9);

    controller.applyProjection({
      ...D1_PROJECTION,
      transcript: [
        ...D1_PROJECTION.transcript,
        { id: "stream-next", kind: "assistant_stream", content: "More output" },
      ],
    });

    const refreshed = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
    expect(refreshed.value).toBe("keep this draft");
    expect(document.activeElement).toBe(refreshed);
    expect([refreshed.selectionStart, refreshed.selectionEnd]).toEqual([5, 9]);
  });

  test("keeps the conversation textarea mounted while CJK composition crosses a Core redraw", async () => {
    const { root, controller } = setup();
    const composer = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
    composer.focus();
    composer.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true }));
    composer.value = "继续处理";
    composer.dispatchEvent(
      new InputEvent("input", { bubbles: true, inputType: "insertCompositionText" }),
    );

    controller.applyProjection({
      ...D1_PROJECTION,
      transcript: [
        ...D1_PROJECTION.transcript,
        { id: "stream-during-ime", kind: "assistant_stream", content: "More output" },
      ],
    });

    expect(root.querySelector<HTMLTextAreaElement>("[data-composer]")).toBe(composer);
    expect(document.activeElement).toBe(composer);
    expect(composer.value).toBe("继续处理");

    composer.dispatchEvent(new CompositionEvent("compositionend", { bubbles: true }));
    await vi.waitFor(() =>
      expect(root.querySelector<HTMLTextAreaElement>("[data-composer]")).not.toBe(composer),
    );
    expect(root.querySelector<HTMLTextAreaElement>("[data-composer]")?.value).toBe("继续处理");
  });

  test("idle identical projections do not replace the rendered viewport", () => {
    const { root, controller } = setup();
    const transcript = root.querySelector<HTMLElement>('[aria-label="Transcript"]')!;
    controller.applyProjection(D1_PROJECTION);
    expect(root.querySelector('[aria-label="Transcript"]')).toBe(transcript);
  });

  test("keeps the hover rails mounted while Core refreshes Lane Agent status", () => {
    const startingSession = {
      sessionId: "session-lane-core",
      laneId: "lane-core",
      agentId: "codex-acp",
      model: null,
      status: "starting" as const,
      task: "Review the project",
      diagnostic: null,
    };
    const projection = { ...D1_PROJECTION, agentSessions: [startingSession] };
    const { root, controller } = setup(projection);
    const activityRail = root.querySelector<HTMLElement>('[data-cockpit-role="activity"]')!;
    const laneRail = root.querySelector<HTMLElement>("#d1-lane-rail")!;

    controller.applyResult({
      projection: {
        ...projection,
        agentSessions: [{ ...startingSession, status: "completed" as const }],
      },
      pendingCommandId: null,
      outcome: { state: "confirmed", reason: null },
    });

    expect(root.querySelector('[data-cockpit-role="activity"]')).toBe(activityRail);
    expect(root.querySelector("#d1-lane-rail")).toBe(laneRail);
    expect(laneRail.querySelector('[data-lane-id="lane-core"]')?.textContent).toContain(
      "completed",
    );
  });

  test("transport rejection restores the exact composer draft", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const send = vi.fn(async () => {
      throw new Error("transport unavailable");
    });
    const confirmed: D1IntentResult = {
      projection: D1_PROJECTION,
      pendingCommandId: null,
      outcome: { state: "confirmed", reason: null },
    };
    renderD1Cockpit(root, D1_PROJECTION, send, async () => confirmed);
    const composer = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
    composer.value = "do not lose me";
    composer.dispatchEvent(new InputEvent("input", { bubbles: true }));
    composer.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    await vi.waitFor(() => {
      expect(root.querySelector<HTMLTextAreaElement>("[data-composer]")?.value).toBe(
        "do not lose me",
      );
    });
  });

  test("keeps the submitted draft pending until ordered Core facts confirm it", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const pending: D1IntentResult = {
      projection: D1_PROJECTION,
      pendingCommandId: "queue-pending",
      outcome: { state: "pending", reason: null },
    };
    const send = vi.fn(async () => pending);
    const controller = renderD1Cockpit(root, D1_PROJECTION, send, async () => pending);
    const composer = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
    composer.value = "wait for Core";
    composer.dispatchEvent(new InputEvent("input", { bubbles: true }));
    composer.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    await vi.waitFor(() => {
      expect(root.querySelector<HTMLTextAreaElement>("[data-composer]")?.value).toBe(
        "wait for Core",
      );
    });

    controller.applyResult({
      projection: D1_PROJECTION,
      pendingCommandId: null,
      outcome: { state: "confirmed", reason: null },
    });
    expect(root.querySelector<HTMLTextAreaElement>("[data-composer]")?.value).toBe("");
  });

  test("Core rejection preserves the draft and renders the typed reason", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const rejected: D1IntentResult = {
      projection: D1_PROJECTION,
      pendingCommandId: null,
      outcome: { state: "rejected", reason: "Core denied the queue" },
    };
    const send = vi.fn(async () => rejected);
    renderD1Cockpit(root, D1_PROJECTION, send, async () => rejected);
    const composer = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
    composer.value = "do not lose me";
    composer.dispatchEvent(new InputEvent("input", { bubbles: true }));
    composer.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    await vi.waitFor(() => {
      expect(root.querySelector<HTMLTextAreaElement>("[data-composer]")?.value).toBe(
        "do not lose me",
      );
      expect(root.querySelector("[data-d1-rejection]")?.textContent).toBe(
        "Core denied the queue",
      );
    });
  });

  test("waits for the Core Lane receipt before submitting the native task", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const sent: D1Intent[] = [];
    const preview = {
      previewId: "preview-7",
      contentSha256: "a".repeat(64),
      laneId: "lane-7",
      branch: "codex/lane-7",
      diagnostics: [],
    };
    const lane = { ...D1_PROJECTION.lanes[0]!, id: "lane-7", branch: "codex/lane-7" };
    const send = vi.fn(async (intent: D1Intent): Promise<D1IntentResult> => {
      sent.push(intent);
      const next =
        intent.type === "preview_default_lane"
          ? { ...D1_PROJECTION, starterLanePreviews: [preview] }
          : intent.type === "create_starter_lane" || intent.type === "submit"
            ? {
                ...D1_PROJECTION,
                selectedLaneId: "lane-7",
                lanes: [...D1_PROJECTION.lanes, lane],
                starterLanePreviews: [preview],
              }
            : D1_PROJECTION;
      return {
        projection: next,
        pendingCommandId: null,
        outcome: { state: "confirmed", reason: null },
      };
    });
    renderD1Cockpit(root, D1_PROJECTION, send, async () => {
      throw new Error("unexpected poll");
    });

    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() =>
      expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe("false"),
    );
    root.querySelector<HTMLButtonElement>("[data-native-agent]")?.click();
    const task = root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;
    task.value = "fix the parser";
    task.dispatchEvent(new InputEvent("input", { bubbles: true }));
    root.querySelector<HTMLButtonElement>("[data-lane-task-submit]")?.click();

    await vi.waitFor(() => {
      expect(sent.at(-1)).toEqual({
        type: "submit",
        laneId: "lane-7",
        content: "fix the parser",
      });
    });
    expect(sent.slice(1, 4)).toEqual([
      { type: "preview_default_lane", preset: "coder" },
      {
        type: "create_starter_lane",
        laneId: "lane-7",
        preset: "coder",
        branch: "codex/lane-7",
        previewId: "preview-7",
        contentSha256: "a".repeat(64),
      },
      { type: "submit", laneId: "lane-7", content: "fix the parser" },
    ]);
  });

  test("resumes the native task after an approved Lane arrives on a later Core poll", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const sent: D1Intent[] = [];
    const preview = {
      previewId: "preview-approved",
      contentSha256: "b".repeat(64),
      laneId: "lane-approved",
      branch: "codex/lane-approved",
      diagnostics: [],
    };
    const previewed: D1IntentResult = {
      projection: { ...D1_PROJECTION, starterLanePreviews: [preview] },
      pendingCommandId: null,
      outcome: { state: "confirmed", reason: null },
    };
    const pendingCreate: D1IntentResult = {
      projection: previewed.projection,
      pendingCommandId: "create-awaiting-approval",
      outcome: { state: "pending", reason: null },
    };
    const lane = {
      ...D1_PROJECTION.lanes[0]!,
      id: "lane-approved",
      branch: "codex/lane-approved",
    };
    const created: D1IntentResult = {
      projection: {
        ...D1_PROJECTION,
        selectedLaneId: "lane-approved",
        lanes: [...D1_PROJECTION.lanes, lane],
        starterLanePreviews: [preview],
      },
      pendingCommandId: null,
      outcome: { state: "confirmed", reason: null },
    };
    const send = vi.fn(async (intent: D1Intent): Promise<D1IntentResult> => {
      sent.push(intent);
      if (intent.type === "preview_default_lane") return previewed;
      if (intent.type === "create_starter_lane") return pendingCreate;
      return created;
    });
    const controller = renderD1Cockpit(
      root,
      D1_PROJECTION,
      send,
      async () => pendingCreate,
      undefined,
      undefined,
      { poll: false },
    );

    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() =>
      expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe("false"),
    );
    root.querySelector<HTMLButtonElement>("[data-native-agent]")?.click();
    const task = root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;
    task.value = "read README after approval";
    task.dispatchEvent(new InputEvent("input", { bubbles: true }));
    root.querySelector<HTMLButtonElement>("[data-lane-task-submit]")?.click();

    await vi.waitFor(() =>
      expect(sent.some((intent) => intent.type === "create_starter_lane")).toBe(true),
    );
    expect(sent.some((intent) => intent.type === "submit")).toBe(false);

    controller.applyResult(created);

    await vi.waitFor(() => {
      expect(sent.at(-1)).toEqual({
        type: "submit",
        laneId: "lane-approved",
        content: "read README after approval",
      });
    });
  });

  test("yields a pending Lane creation to the projected approval instead of polling forever", async () => {
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 1228 });
    Object.defineProperty(window, "innerHeight", { configurable: true, value: 768 });
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const preview = {
      previewId: "preview-awaiting-approval",
      contentSha256: "c".repeat(64),
      laneId: "lane-awaiting-approval",
      branch: "codex/lane-awaiting-approval",
      diagnostics: [],
    };
    const previewed: D1IntentResult = {
      projection: { ...D1_PROJECTION, starterLanePreviews: [preview] },
      pendingCommandId: null,
      outcome: { state: "confirmed", reason: null },
    };
    const awaitingApproval: D1IntentResult = {
      projection: {
        ...PERMISSION_PROJECTION,
        starterLanePreviews: [preview],
      },
      pendingCommandId: "create-awaiting-approval",
      outcome: { state: "pending", reason: null },
    };
    const created: D1IntentResult = {
      projection: {
        ...D1_PROJECTION,
        selectedLaneId: preview.laneId,
        lanes: [
          ...D1_PROJECTION.lanes,
          {
            ...D1_PROJECTION.lanes[0]!,
            id: preview.laneId,
            branch: preview.branch,
          },
        ],
        starterLanePreviews: [preview],
      },
      pendingCommandId: null,
      outcome: { state: "confirmed", reason: null },
    };
    const send = vi.fn(async (intent: D1Intent): Promise<D1IntentResult> => {
      if (intent.type === "preview_default_lane") return previewed;
      if (intent.type === "create_starter_lane") return awaitingApproval;
      if (intent.type === "submit") return created;
      throw new Error(`unexpected intent ${intent.type}`);
    });
    const poll = vi.fn(async (): Promise<D1IntentResult> => {
      throw new Error("interactive approval must return control before polling");
    });
    const controller = renderD1Cockpit(
      root,
      D1_PROJECTION,
      send,
      poll,
      undefined,
      undefined,
      { poll: false },
    );

    root.querySelector<HTMLButtonElement>("[data-lanes-toggle]")?.click();
    expect(root.querySelector("#d1-lane-rail")?.getAttribute("data-open")).toBe("true");
    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() =>
      expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe("false"),
    );
    root.querySelector<HTMLButtonElement>("[data-native-agent]")?.click();
    const task = root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;
    task.value = "Inspect README after approval";
    task.dispatchEvent(new InputEvent("input", { bubbles: true }));
    root.querySelector<HTMLButtonElement>("[data-lane-task-submit]")?.click();

    await vi.waitFor(() => {
      expect(root.querySelector("[data-permission-dock]")).not.toBeNull();
      expect(root.querySelector("[data-new-lane-popover]")).toBeNull();
      expect(root.querySelector("#d1-lane-rail")?.getAttribute("data-open")).toBe("false");
      expect(root.querySelector('[data-permission-action="once"]')).not.toBeNull();
      expect(root.querySelector('[data-permission-action="deny"]')).not.toBeNull();
    });
    expect(poll).not.toHaveBeenCalled();
    expect(
      send.mock.calls.some(([intent]) => intent.type === "submit"),
    ).toBe(false);

    controller.applyResult(created);
    await vi.waitFor(() =>
      expect(send.mock.calls.at(-1)?.[0]).toEqual({
        type: "submit",
        laneId: preview.laneId,
        content: "Inspect README after approval",
      }),
    );
  });

  test("drops the pending native task when the interactive Lane creation is rejected", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const preview = {
      previewId: "preview-denied",
      contentSha256: "d".repeat(64),
      laneId: "lane-denied",
      branch: "codex/lane-denied",
      diagnostics: [],
    };
    const previewed: D1IntentResult = {
      projection: { ...D1_PROJECTION, starterLanePreviews: [preview] },
      pendingCommandId: null,
      outcome: { state: "confirmed", reason: null },
    };
    const awaitingApproval: D1IntentResult = {
      projection: { ...PERMISSION_PROJECTION, starterLanePreviews: [preview] },
      pendingCommandId: "create-awaiting-denial",
      outcome: { state: "pending", reason: null },
    };
    const rejected: D1IntentResult = {
      projection: { ...D1_PROJECTION, starterLanePreviews: [] },
      pendingCommandId: null,
      outcome: { state: "rejected", reason: "Lane creation was denied." },
    };
    const laterLane: D1IntentResult = {
      projection: {
        ...D1_PROJECTION,
        lanes: [
          ...D1_PROJECTION.lanes,
          {
            ...D1_PROJECTION.lanes[0]!,
            id: preview.laneId,
            branch: preview.branch,
          },
        ],
      },
      pendingCommandId: null,
      outcome: { state: "confirmed", reason: null },
    };
    const sent: D1Intent[] = [];
    const send = vi.fn(async (intent: D1Intent): Promise<D1IntentResult> => {
      sent.push(intent);
      if (intent.type === "preview_default_lane") return previewed;
      if (intent.type === "create_starter_lane") return awaitingApproval;
      return laterLane;
    });
    const controller = renderD1Cockpit(
      root,
      D1_PROJECTION,
      send,
      async () => awaitingApproval,
      undefined,
      undefined,
      { poll: false },
    );

    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() =>
      expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe("false"),
    );
    root.querySelector<HTMLButtonElement>("[data-native-agent]")?.click();
    const task = root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;
    task.value = "Do not run after denial";
    task.dispatchEvent(new InputEvent("input", { bubbles: true }));
    root.querySelector<HTMLButtonElement>("[data-lane-task-submit]")?.click();
    await vi.waitFor(() =>
      expect(root.querySelector("[data-new-lane-popover]")).toBeNull(),
    );

    controller.applyResult(rejected);
    expect(root.querySelector("[data-d1-rejection]")?.textContent).toContain(
      "Lane creation was denied.",
    );
    controller.applyResult(laterLane);
    await Promise.resolve();
    expect(sent.some((intent) => intent.type === "submit")).toBe(false);
  });

  test("preserves the native task draft across ordered Core projection redraws", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const send = vi.fn(async (): Promise<D1IntentResult> => ({
      projection: D1_PROJECTION,
      pendingCommandId: null,
      outcome: { state: "confirmed", reason: null },
    }));
    const controller = renderD1Cockpit(
      root,
      D1_PROJECTION,
      send,
      async () => {
        throw new Error("unexpected poll");
      },
      undefined,
      undefined,
      { poll: false },
    );

    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() =>
      expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe("false"),
    );
    root.querySelector<HTMLButtonElement>("[data-native-agent]")?.click();
    const task = root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;
    task.value = "read README only";
    task.dispatchEvent(new InputEvent("input", { bubbles: true }));

    controller.applyProjection({
      ...D1_PROJECTION,
      environment: { ...D1_PROJECTION.environment, tokenTotal: 1 },
    });

    expect(root.querySelector<HTMLTextAreaElement>("[data-lane-task]")?.value).toBe(
      "read README only",
    );
  });

  test("keeps the New Lane textarea mounted while CJK composition crosses a Core redraw", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const send = vi.fn(async (): Promise<D1IntentResult> => ({
      projection: D1_PROJECTION,
      pendingCommandId: null,
      outcome: { state: "confirmed", reason: null },
    }));
    const controller = renderD1Cockpit(
      root,
      D1_PROJECTION,
      send,
      async () => {
        throw new Error("unexpected poll");
      },
      undefined,
      undefined,
      { poll: false },
    );

    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() =>
      expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe("false"),
    );
    const task = root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;
    task.focus();
    task.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true }));
    task.value = "中文";
    task.dispatchEvent(
      new InputEvent("input", { bubbles: true, inputType: "insertCompositionText" }),
    );

    controller.applyProjection({
      ...D1_PROJECTION,
      environment: { ...D1_PROJECTION.environment, tokenTotal: 1 },
    });

    expect(root.querySelector<HTMLTextAreaElement>("[data-lane-task]")).toBe(task);
    expect(document.activeElement).toBe(task);
    expect(task.value).toBe("中文");

    task.dispatchEvent(new CompositionEvent("compositionend", { bubbles: true }));
    await vi.waitFor(() =>
      expect(root.querySelector<HTMLTextAreaElement>("[data-lane-task]")).not.toBe(task),
    );
    expect(root.querySelector<HTMLTextAreaElement>("[data-lane-task]")?.value).toBe("中文");
  });

  test("preserves the New Lane task draft when Core rejects preview dispatch", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const rejected: D1IntentResult = {
      projection: D1_PROJECTION,
      pendingCommandId: null,
      outcome: { state: "rejected", reason: "Core denied preview" },
    };
    renderD1Cockpit(
      root,
      D1_PROJECTION,
      vi.fn(async () => rejected),
      async () => rejected,
      undefined,
      undefined,
      { poll: false },
    );

    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() =>
      expect(root.querySelector("[data-new-lane-popover]")?.getAttribute("aria-busy")).toBe(
        "false",
      ),
    );
    root.querySelector<HTMLButtonElement>("[data-native-agent]")?.click();
    const task = root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;
    task.value = "keep this lane task";
    task.dispatchEvent(new InputEvent("input", { bubbles: true }));
    root.querySelector<HTMLButtonElement>("[data-lane-task-submit]")?.click();

    await vi.waitFor(() => {
      expect(root.querySelector<HTMLTextAreaElement>("[data-lane-task]")?.value).toBe(
        "keep this lane task",
      );
      expect(root.querySelector<HTMLButtonElement>("[data-lane-task-submit]")?.disabled).toBe(
        false,
      );
      expect(root.querySelector("[data-d1-rejection]")?.textContent).toBe("Core denied preview");
    });
  });

  test("creates a dedicated Lane for ACP and keeps one Agent per Lane", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const sent: D1Intent[] = [];
    const initial = {
      ...D1_PROJECTION,
      selectedLaneId: null,
      lanes: [],
      composer: {
        ...D1_PROJECTION.composer,
        editable: false,
        canCancel: false,
      },
    };
    const preview = {
      previewId: "preview-acp",
      contentSha256: "c".repeat(64),
      laneId: "lane-acp",
      branch: "codex/lane-acp",
      diagnostics: [],
    };
    const lane = {
      ...D1_PROJECTION.lanes[0]!,
      id: "lane-acp",
      branch: "codex/lane-acp",
    };
    const laneContextDock = {
      ...D1_PROJECTION.contextDock,
      laneAgent: {
        ...D1_PROJECTION.contextDock.laneAgent!,
        laneId: "lane-acp",
        sessionId: "acp-1",
      },
    };
    const idleComposer = {
      ...D1_PROJECTION.composer,
      busy: false,
      canSubmitImmediately: true,
    };
    const session = {
      sessionId: "acp-1",
      laneId: "lane-acp",
      agentId: "codex-acp",
      model: null,
      status: "running",
      task: "review the diff",
      diagnostic: null,
    };
    const send = vi.fn(async (intent: D1Intent): Promise<D1IntentResult> => {
      sent.push(intent);
      const next =
        intent.type === "preview_default_lane"
          ? { ...initial, starterLanePreviews: [preview] }
          : intent.type === "create_starter_lane"
            ? {
                ...initial,
                selectedLaneId: "lane-acp",
                lanes: [lane],
                starterLanePreviews: [preview],
                contextDock: laneContextDock,
              }
            : intent.type === "start_agent_session" ||
                intent.type === "send_agent_session_input"
              ? {
                  ...initial,
                  selectedLaneId: "lane-acp",
                  lanes: [lane],
                  starterLanePreviews: [preview],
                  agentSessions: [session],
                  contextDock: laneContextDock,
                  composer: idleComposer,
                }
              : initial;
      return {
        projection: next,
        pendingCommandId: null,
        outcome: { state: "confirmed", reason: null },
      };
    });
    renderD1Cockpit(root, initial, send, async () => {
      throw new Error("unexpected poll");
    });

    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() =>
      expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe("false"),
    );
    root.querySelector<HTMLButtonElement>('[data-agent-id="codex-acp"]')?.click();
    const task = root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;
    task.value = "review the diff";
    task.dispatchEvent(new InputEvent("input", { bubbles: true }));
    root.querySelector<HTMLButtonElement>("[data-lane-task-submit]")?.click();

    await vi.waitFor(() => {
      expect(sent.at(-1)).toEqual({
        type: "start_agent_session",
        laneId: "lane-acp",
        agentId: "codex-acp",
        model: null,
        task: "review the diff",
      });
    });
    expect(sent.slice(1, 4)).toEqual([
      { type: "preview_default_lane", preset: "coder" },
      {
        type: "create_starter_lane",
        laneId: "lane-acp",
        preset: "coder",
        branch: "codex/lane-acp",
        previewId: "preview-acp",
        contentSha256: "c".repeat(64),
      },
      {
        type: "start_agent_session",
        laneId: "lane-acp",
        agentId: "codex-acp",
        model: null,
        task: "review the diff",
      },
    ]);
    expect(root.querySelector("[data-agent-session-id]")).toBeNull();

    const composer = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
    composer.value = "continue";
    composer.dispatchEvent(new InputEvent("input", { bubbles: true }));
    composer.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await vi.waitFor(() => {
      expect(sent.at(-1)).toEqual({
        type: "send_agent_session_input",
        laneId: "lane-acp",
        sessionId: "acp-1",
        content: "continue",
      });
    });
    root.querySelector<HTMLButtonElement>("[data-cancel-turn]")?.click();
    await vi.waitFor(() => {
      expect(sent.at(-1)).toEqual({
        type: "cancel_agent_session",
        laneId: "lane-acp",
        sessionId: "acp-1",
      });
    });
  });

  test("renders the ACP startup rejection after its Lane is created", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const initial = { ...D1_PROJECTION, selectedLaneId: null, lanes: [] };
    const preview = {
      previewId: "preview-acp-rejected",
      contentSha256: "f".repeat(64),
      laneId: "lane-acp-rejected",
      branch: "codex/lane-acp-rejected",
      diagnostics: [],
    };
    const lane = {
      ...D1_PROJECTION.lanes[0]!,
      id: preview.laneId,
      branch: preview.branch,
    };
    const created = {
      ...initial,
      selectedLaneId: preview.laneId,
      lanes: [lane],
      starterLanePreviews: [preview],
    };
    const send = vi.fn(async (intent: D1Intent): Promise<D1IntentResult> => {
      if (intent.type === "preview_default_lane") {
        return {
          projection: { ...initial, starterLanePreviews: [preview] },
          pendingCommandId: null,
          outcome: { state: "confirmed", reason: null },
        };
      }
      if (intent.type === "create_starter_lane") {
        return {
          projection: created,
          pendingCommandId: null,
          outcome: { state: "confirmed", reason: null },
        };
      }
      if (intent.type === "start_agent_session") {
        return {
          projection: created,
          pendingCommandId: null,
          outcome: { state: "rejected", reason: "Codex is not signed in" },
        };
      }
      return {
        projection: initial,
        pendingCommandId: null,
        outcome: { state: "confirmed", reason: null },
      };
    });
    renderD1Cockpit(
      root,
      initial,
      send,
      async () => ({
        projection: initial,
        pendingCommandId: null,
        outcome: { state: "idle", reason: null },
      }),
      undefined,
      undefined,
      { poll: false },
    );

    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() =>
      expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe(
        "false",
      ),
    );
    root.querySelector<HTMLButtonElement>('[data-agent-id="codex-acp"]')?.click();
    const task = root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;
    task.value = "review with Codex";
    task.dispatchEvent(new InputEvent("input", { bubbles: true }));
    root.querySelector<HTMLButtonElement>("[data-lane-task-submit]")?.click();

    await vi.waitFor(() =>
      expect(root.querySelector("[data-d1-rejection]")?.textContent).toBe(
        "Codex is not signed in",
      ),
    );
  });

  test("waits for Lane creation confirmation before starting the selected ACP Agent", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const sent: D1Intent[] = [];
    const initial = {
      ...D1_PROJECTION,
      selectedLaneId: null,
      lanes: [],
      composer: {
        ...D1_PROJECTION.composer,
        editable: false,
        canCancel: false,
      },
    };
    const preview = {
      previewId: "preview-acp-confirmed",
      contentSha256: "e".repeat(64),
      laneId: "lane-acp-confirmed",
      branch: "codex/lane-acp-confirmed",
      diagnostics: [],
    };
    const lane = {
      ...D1_PROJECTION.lanes[0]!,
      id: preview.laneId,
      branch: preview.branch,
    };
    const projectedLane = {
      ...initial,
      selectedLaneId: preview.laneId,
      lanes: [lane],
      starterLanePreviews: [preview],
    };
    const pendingProjectedLane: D1IntentResult = {
      projection: {
        ...projectedLane,
        permissionDock: PERMISSION_PROJECTION.permissionDock,
      },
      pendingCommandId: "create-acp-awaiting-confirmation",
      outcome: { state: "pending", reason: null },
    };
    const confirmedLane: D1IntentResult = {
      projection: projectedLane,
      pendingCommandId: null,
      outcome: { state: "confirmed", reason: null },
    };
    const session = {
      sessionId: "acp-confirmed",
      laneId: preview.laneId,
      agentId: "codex-acp",
      model: null,
      status: "running",
      task: "confirm before ACP start",
      diagnostic: null,
    };
    const send = vi.fn(async (intent: D1Intent): Promise<D1IntentResult> => {
      sent.push(intent);
      if (intent.type === "preview_default_lane") {
        return {
          projection: { ...initial, starterLanePreviews: [preview] },
          pendingCommandId: null,
          outcome: { state: "confirmed", reason: null },
        };
      }
      if (intent.type === "create_starter_lane") return pendingProjectedLane;
      if (intent.type === "start_agent_session") {
        return {
          projection: { ...projectedLane, agentSessions: [session] },
          pendingCommandId: null,
          outcome: { state: "confirmed", reason: null },
        };
      }
      return {
        projection: initial,
        pendingCommandId: null,
        outcome: { state: "confirmed", reason: null },
      };
    });
    const controller = renderD1Cockpit(
      root,
      initial,
      send,
      async () => pendingProjectedLane,
      undefined,
      undefined,
      { poll: false },
    );

    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() =>
      expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe("false"),
    );
    root.querySelector<HTMLButtonElement>('[data-agent-id="codex-acp"]')?.click();
    const task = root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;
    task.value = "confirm before ACP start";
    task.dispatchEvent(new InputEvent("input", { bubbles: true }));
    root.querySelector<HTMLButtonElement>("[data-lane-task-submit]")?.click();

    await vi.waitFor(() =>
      expect(sent.some((intent) => intent.type === "create_starter_lane")).toBe(true),
    );
    expect(sent.some((intent) => intent.type === "start_agent_session")).toBe(false);

    controller.applyResult(confirmedLane);

    await vi.waitFor(() =>
      expect(sent.at(-1)).toEqual({
        type: "start_agent_session",
        laneId: preview.laneId,
        agentId: "codex-acp",
        model: null,
        task: "confirm before ACP start",
      }),
    );
  });

  test("drains an async ACP probe queue from ordered Core poll results", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const sent: D1Intent[] = [];
    const queriedAdapters = [
      {
        agentId: "custom-acp",
        displayName: "Custom ACP",
        startability: "probe_required",
        diagnostics: [],
      },
      {
        agentId: "kiro-cli",
        displayName: "Kiro",
        startability: "probe_required",
        diagnostics: [],
      },
      {
        agentId: "codex-acp",
        displayName: "Codex",
        startability: "probe_required",
        diagnostics: [],
      },
      {
        agentId: "claude-acp",
        displayName: "Claude",
        startability: "probe_required",
        diagnostics: [],
      },
    ];
    let current = { ...D1_PROJECTION, agentAdapters: [] as typeof queriedAdapters };
    let probePolls = 0;
    let releaseCodexProbe = false;
    const send = vi.fn(async (intent: D1Intent): Promise<D1IntentResult> => {
      sent.push(intent);
      if (intent.type === "query_agent_adapters") {
        current = { ...current, agentAdapters: queriedAdapters };
      } else if (intent.type === "probe_agent_adapter") {
        if (intent.agentId === "codex-acp") {
          return {
            projection: current,
            pendingCommandId: "probe-codex-pending",
            outcome: { state: "pending", reason: null },
          };
        }
        current = {
          ...current,
          agentAdapters: current.agentAdapters.map((adapter) =>
            adapter.agentId === intent.agentId
              ? { ...adapter, startability: "ready" }
              : adapter,
          ),
        };
      } else if (intent.type === "start_agent_session") {
        current = {
          ...current,
          agentSessions: [
            {
              sessionId: "acp-probed",
              laneId: intent.laneId,
              agentId: intent.agentId,
              model: null,
              status: "running",
              task: intent.task,
              diagnostic: null,
            },
          ],
        };
      }
      return {
        projection: current,
        pendingCommandId: null,
        outcome: { state: "confirmed", reason: null },
      };
    });
    const poll = vi.fn(async (): Promise<D1IntentResult> => {
      probePolls += 1;
      if (!releaseCodexProbe || probePolls <= 3) {
        return {
          projection: current,
          pendingCommandId: "probe-codex-pending",
          outcome: { state: "pending", reason: null },
        };
      }
      current = {
        ...current,
        agentAdapters: current.agentAdapters.map((adapter) =>
          adapter.agentId === "codex-acp"
            ? { ...adapter, startability: "ready" }
            : adapter,
        ),
      };
      return {
        projection: current,
        pendingCommandId: null,
        outcome: { state: "confirmed", reason: null },
      };
    });
    renderD1Cockpit(root, current, send, poll);

    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() => {
      expect(sent).toEqual([
        { type: "query_agent_adapters" },
        { type: "probe_agent_adapter", agentId: "codex-acp" },
      ]);
      expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe("true");
      expect(root.querySelector("[data-native-agent]")?.getAttribute("aria-disabled")).toBe(
        "false",
      );
      expect(
        Array.from(root.querySelectorAll("[data-agent-id]")).every(
          (item) => item.getAttribute("aria-disabled") === "true",
        ),
      ).toBe(true);
    });

    root
      .querySelector<HTMLElement>('[data-new-lane-popover]')
      ?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(root.querySelector('[data-new-lane-popover]')).toBeNull();
    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() =>
      expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe("true"),
    );
    expect(sent).toEqual([
      { type: "query_agent_adapters" },
      { type: "probe_agent_adapter", agentId: "codex-acp" },
    ]);
    await vi.waitFor(() => expect(probePolls).toBeGreaterThanOrEqual(3), { timeout: 2_000 });
    expect(sent).toEqual([
      { type: "query_agent_adapters" },
      { type: "probe_agent_adapter", agentId: "codex-acp" },
    ]);
    releaseCodexProbe = true;

    await vi.waitFor(
      () => {
        expect(probePolls).toBeGreaterThanOrEqual(4);
        expect(sent.slice(0, 5)).toEqual([
          { type: "query_agent_adapters" },
          { type: "probe_agent_adapter", agentId: "codex-acp" },
          { type: "probe_agent_adapter", agentId: "claude-acp" },
          { type: "probe_agent_adapter", agentId: "kiro-cli" },
          { type: "probe_agent_adapter", agentId: "custom-acp" },
        ]);
        expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe("false");
        expect(
          Array.from(root.querySelectorAll<HTMLElement>("[data-agent-id]")).map(
            (item) => item.dataset.agentId,
          ),
        ).toEqual(["codex-acp", "claude-acp", "kiro-cli", "custom-acp"]);
      },
      { timeout: 2_000 },
    );

    expect(root.querySelector("[data-agent-session-id]")).toBeNull();
  });

  test("reuses the completed ACP discovery result when New Lane is reopened", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const sent: D1Intent[] = [];
    const send = vi.fn(async (intent: D1Intent): Promise<D1IntentResult> => {
      sent.push(intent);
      return {
        projection: D1_PROJECTION,
        pendingCommandId: null,
        outcome: { state: "confirmed", reason: null },
      };
    });
    renderD1Cockpit(
      root,
      D1_PROJECTION,
      send,
      async () => ({
        projection: D1_PROJECTION,
        pendingCommandId: null,
        outcome: { state: "idle", reason: null },
      }),
      undefined,
      undefined,
      { poll: false },
    );

    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() =>
      expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe(
        "false",
      ),
    );
    expect(sent).toEqual([{ type: "query_agent_adapters" }]);

    root.querySelector<HTMLButtonElement>(".agent-menu-close")?.click();
    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() =>
      expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe(
        "false",
      ),
    );

    expect(sent).toEqual([{ type: "query_agent_adapters" }]);
  });

  test("surfaces ACP discovery failure in New Lane and retries only on request", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const sent: D1Intent[] = [];
    let queryAttempt = 0;
    let resolveRetry!: (result: D1IntentResult) => void;
    const retryResult = new Promise<D1IntentResult>((resolve) => {
      resolveRetry = resolve;
    });
    const send = vi.fn(async (intent: D1Intent): Promise<D1IntentResult> => {
      sent.push(intent);
      queryAttempt += 1;
      if (queryAttempt > 1) return retryResult;
      return {
        projection: D1_PROJECTION,
        pendingCommandId: null,
        outcome: { state: "rejected", reason: "ACP discovery unavailable" },
      };
    });
    renderD1Cockpit(
      root,
      D1_PROJECTION,
      send,
      async () => ({
        projection: D1_PROJECTION,
        pendingCommandId: null,
        outcome: { state: "idle", reason: null },
      }),
      undefined,
      undefined,
      { poll: false },
    );

    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() => {
      expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe(
        "false",
      );
      expect(root.querySelector("[data-agent-discovery-error]")?.textContent).toContain(
        "ACP discovery unavailable",
      );
      expect(root.querySelector("[data-agent-discovery-retry]")).not.toBeNull();
    });
    expect(sent).toEqual([{ type: "query_agent_adapters" }]);

    root.querySelector<HTMLButtonElement>("[data-agent-discovery-retry]")?.click();
    await vi.waitFor(() => expect(sent).toHaveLength(2));
    expect(root.querySelectorAll("[data-new-lane-popover]")).toHaveLength(1);
    resolveRetry({
      projection: D1_PROJECTION,
      pendingCommandId: null,
      outcome: { state: "confirmed", reason: null },
    });
    await vi.waitFor(() =>
      expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe(
        "false",
      ),
    );
    expect(sent).toEqual([
      { type: "query_agent_adapters" },
      { type: "query_agent_adapters" },
    ]);
    expect(root.querySelector("[data-agent-discovery-error]")).toBeNull();
  });

  test("blocks a native direct-workspace Lane until ACP discovery completes", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const sent: D1Intent[] = [];
    const initial = {
      ...D1_PROJECTION,
      selectedLaneId: null,
      lanes: [],
      agentAdapters: [],
      workspaceEligibility: {
        isGitRepository: false,
        hasHead: false,
        canCreateLane: true,
        diagnostic: null,
      },
    };
    const preview = {
      previewId: "preview-native-during-probe",
      contentSha256: "d".repeat(64),
      laneId: "lane-native-during-probe",
      branch: null,
      diagnostics: [],
    };
    const lane = {
      ...D1_PROJECTION.lanes[0]!,
      id: preview.laneId,
      branch: preview.branch,
    };
    let resolveQuery!: (result: D1IntentResult) => void;
    const deferredQuery = new Promise<D1IntentResult>((resolve) => {
      resolveQuery = resolve;
    });
    const send = vi.fn(async (intent: D1Intent): Promise<D1IntentResult> => {
      sent.push(intent);
      if (intent.type === "query_agent_adapters") return deferredQuery;
      const projection =
        intent.type === "preview_default_lane"
          ? { ...initial, starterLanePreviews: [preview] }
          : {
              ...initial,
              selectedLaneId: preview.laneId,
              lanes: [lane],
              starterLanePreviews: [preview],
            };
      return {
        projection,
        pendingCommandId: null,
        outcome: { state: "confirmed", reason: null },
      };
    });
    renderD1Cockpit(
      root,
      initial,
      send,
      async () => ({
        projection: initial,
        pendingCommandId: null,
        outcome: { state: "idle", reason: null },
      }),
      undefined,
      undefined,
      { poll: false },
    );

    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() => expect(sent).toEqual([{ type: "query_agent_adapters" }]));
    expect(root.querySelector("[data-native-agent]")?.getAttribute("aria-disabled")).toBe(
      "false",
    );
    root.querySelector<HTMLButtonElement>("[data-native-agent]")?.click();

    const task = root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;
    task.value = "Inspect README without modifying files";
    task.dispatchEvent(new InputEvent("input", { bubbles: true }));
    root.querySelector<HTMLButtonElement>("[data-lane-task-submit]")?.click();
    expect(sent).toEqual([{ type: "query_agent_adapters" }]);

    resolveQuery({
      projection: initial,
      pendingCommandId: null,
      outcome: { state: "confirmed", reason: null },
    });
    await vi.waitFor(() =>
      expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe("false"),
    );
    root.querySelector<HTMLButtonElement>("[data-lane-task-submit]")?.click();

    await vi.waitFor(() => {
      expect(sent).toEqual([
        { type: "query_agent_adapters" },
        { type: "preview_default_lane", preset: "coder" },
        {
          type: "create_starter_lane",
          laneId: preview.laneId,
          preset: "coder",
          branch: preview.branch,
          previewId: preview.previewId,
          contentSha256: preview.contentSha256,
        },
        {
          type: "submit",
          laneId: preview.laneId,
          content: "Inspect README without modifying files",
        },
      ]);
    });
    expect(root.querySelector("[data-d1-rejection]")).toBeNull();
  });

  test("waits on the Core event condition when a native Lane preview completes later", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const sent: D1Intent[] = [];
    const initial = {
      ...D1_PROJECTION,
      selectedLaneId: null,
      lanes: [],
      agentAdapters: [],
    };
    const preview = {
      previewId: "preview-delayed-native",
      contentSha256: "e".repeat(64),
      laneId: "lane-delayed-native",
      branch: "viden/lane-delayed-native",
      diagnostics: [],
    };
    const previewed: D1IntentResult = {
      projection: { ...initial, starterLanePreviews: [preview] },
      pendingCommandId: null,
      outcome: { state: "confirmed", reason: null },
    };
    const pendingPreview: D1IntentResult = {
      projection: initial,
      pendingCommandId: "preview-delayed",
      outcome: { state: "pending", reason: null },
    };
    let resolvePreview!: (result: D1IntentResult) => void;
    const delayedPreview = new Promise<D1IntentResult>((resolve) => {
      resolvePreview = resolve;
    });
    const send = vi.fn(async (intent: D1Intent): Promise<D1IntentResult> => {
      sent.push(intent);
      if (intent.type === "preview_default_lane") {
        window.setTimeout(() => resolvePreview(previewed), 25);
        return pendingPreview;
      }
      const projection =
        intent.type === "query_agent_adapters"
          ? initial
          : {
              ...initial,
              selectedLaneId: preview.laneId,
              lanes: [{ ...D1_PROJECTION.lanes[0]!, id: preview.laneId, branch: preview.branch }],
              starterLanePreviews: [preview],
            };
      return {
        projection,
        pendingCommandId: null,
        outcome: { state: "confirmed", reason: null },
      };
    });
    const poll = vi.fn(
      async (_laneId?: string, waitForEvent = false): Promise<D1IntentResult> =>
        waitForEvent ? delayedPreview : pendingPreview,
    );
    renderD1Cockpit(root, initial, send, poll, undefined, undefined, { poll: false });

    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() =>
      expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe("false"),
    );
    root.querySelector<HTMLButtonElement>("[data-native-agent]")?.click();
    const task = root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;
    task.value = "Inspect README after delayed preview";
    task.dispatchEvent(new InputEvent("input", { bubbles: true }));
    root.querySelector<HTMLButtonElement>("[data-lane-task-submit]")?.click();

    await vi.waitFor(() => {
      expect(sent).toContainEqual({
        type: "create_starter_lane",
        laneId: preview.laneId,
        preset: "coder",
        branch: preview.branch,
        previewId: preview.previewId,
        contentSha256: preview.contentSha256,
      });
    });
    expect(poll).toHaveBeenCalledWith(undefined, true);
  });

  test("serializes discovery behind a stale periodic poll before accepting terminal facts", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const sent: D1Intent[] = [];
    const adapter = {
      agentId: "codex-acp",
      displayName: "Codex",
      startability: "probe_required",
      diagnostics: [] as string[],
    };
    let current: D1IntentResult["projection"] = { ...D1_PROJECTION, agentAdapters: [] };
    let resolveStalePoll!: (result: D1IntentResult) => void;
    const stalePoll = new Promise<D1IntentResult>((resolve) => {
      resolveStalePoll = resolve;
    });
    let pollCall = 0;
    const poll = vi.fn(async (): Promise<D1IntentResult> => {
      pollCall += 1;
      if (pollCall === 1) return stalePoll;
      if (pollCall === 2) {
        current = { ...current, agentAdapters: [adapter] };
        return {
          projection: current,
          pendingCommandId: null,
          outcome: { state: "confirmed", reason: null },
        };
      }
      current = {
        ...current,
        agentAdapters: [{ ...adapter, startability: "ready" }],
      };
      return {
        projection: current,
        pendingCommandId: null,
        outcome: { state: "confirmed", reason: null },
      };
    });
    const send = vi.fn(async (intent: D1Intent): Promise<D1IntentResult> => {
      sent.push(intent);
      return {
        projection: current,
        pendingCommandId:
          intent.type === "query_agent_adapters" ? "query-pending" : "probe-pending",
        outcome: { state: "pending", reason: null },
      };
    });
    renderD1Cockpit(root, current, send, poll);

    await vi.waitFor(() => expect(poll).toHaveBeenCalledTimes(1));
    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await Promise.resolve();
    expect(sent).toEqual([]);

    resolveStalePoll({
      projection: current,
      pendingCommandId: null,
      outcome: { state: "idle", reason: null },
    });

    await vi.waitFor(
      () => {
        expect(sent).toEqual([
          { type: "query_agent_adapters" },
          { type: "probe_agent_adapter", agentId: "codex-acp" },
        ]);
        expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe("false");
        expect(
          root.querySelector('[data-agent-id="codex-acp"]')?.getAttribute("aria-disabled"),
        ).toBe("false");
      },
      { timeout: 2_000 },
    );
  });

  test("drops a deferred discovery result after the cockpit is disposed", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const sent: D1Intent[] = [];
    let resolveQuery!: (result: D1IntentResult) => void;
    const deferredQuery = new Promise<D1IntentResult>((resolve) => {
      resolveQuery = resolve;
    });
    const send = vi.fn(async (intent: D1Intent): Promise<D1IntentResult> => {
      sent.push(intent);
      return deferredQuery;
    });
    const controller = renderD1Cockpit(
      root,
      D1_PROJECTION,
      send,
      async () => ({
        projection: D1_PROJECTION,
        pendingCommandId: null,
        outcome: { state: "idle", reason: null },
      }),
      undefined,
      undefined,
      { poll: false },
    );

    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() => expect(sent).toEqual([{ type: "query_agent_adapters" }]));
    controller.dispose();
    root.innerHTML = '<p data-next-page="true">next page</p>';
    const disposedMarkup = root.innerHTML;
    resolveQuery({
      projection: {
        ...D1_PROJECTION,
        agentAdapters: [
          {
            agentId: "codex-acp",
            displayName: "Codex",
            startability: "probe_required",
            diagnostics: [],
          },
        ],
      },
      pendingCommandId: null,
      outcome: { state: "confirmed", reason: null },
    });
    await new Promise((resolve) => window.setTimeout(resolve, 0));
    expect(root.innerHTML).toBe(disposedMarkup);
    expect(sent).toEqual([{ type: "query_agent_adapters" }]);
  });

  test("continues after a failed probe without repeating or skipping adapters", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const sent: D1Intent[] = [];
    let current: D1IntentResult["projection"] = {
      ...D1_PROJECTION,
      agentAdapters: [
        {
          agentId: "claude-acp",
          displayName: "Claude",
          startability: "probe_required",
          diagnostics: [],
        },
        {
          agentId: "codex-acp",
          displayName: "Codex",
          startability: "probe_required",
          diagnostics: [],
        },
      ],
    };
    const send = vi.fn(async (intent: D1Intent): Promise<D1IntentResult> => {
      sent.push(intent);
      if (intent.type === "probe_agent_adapter" && intent.agentId === "codex-acp") {
        throw new Error("codex probe transport failed");
      }
      if (intent.type === "probe_agent_adapter") {
        current = {
          ...current,
          agentAdapters: current.agentAdapters.map((adapter) =>
            adapter.agentId === intent.agentId
              ? { ...adapter, startability: "unavailable", diagnostics: ["not signed in"] }
              : adapter,
          ),
        };
      }
      return {
        projection: current,
        pendingCommandId: null,
        outcome: { state: "confirmed", reason: null },
      };
    });
    renderD1Cockpit(root, current, send, async () => ({
      projection: current,
      pendingCommandId: null,
      outcome: { state: "idle", reason: null },
    }));

    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    await vi.waitFor(() =>
      expect(root.querySelector('[data-new-lane-popover]')?.getAttribute("aria-busy")).toBe("false"),
    );

    expect(sent).toEqual([
      { type: "query_agent_adapters" },
      { type: "probe_agent_adapter", agentId: "codex-acp" },
      { type: "probe_agent_adapter", agentId: "claude-acp" },
    ]);
    expect(
      root.querySelector('[data-agent-id="claude-acp"]')?.getAttribute("aria-disabled"),
    ).toBe("true");
  });

  test("renders and retries the sole failed Agent as a Lane-level action", async () => {
    const failed = {
      sessionId: "acp-failed",
      laneId: "lane-core",
      agentId: "codex-acp",
      model: null,
      status: "failed",
      task: "review",
      diagnostic: "recoverable",
    };
    const projection = { ...D1_PROJECTION, agentSessions: [failed] };
    const { root, send } = setup(projection);

    const lane = root.querySelector<HTMLElement>('[data-lane-id="lane-core"]');
    expect(lane?.dataset.laneAgentId).toBe("codex-acp");
    expect(lane?.textContent).toContain("codex-acp");
    expect(root.querySelector("[data-agent-session-id]")).toBeNull();
    root.querySelector<HTMLButtonElement>(
      '[data-retry-lane-agent="lane-core"]',
    )?.click();

    expect(send).toHaveBeenCalledWith({
      type: "retry_agent_session",
      laneId: "lane-core",
      sessionId: "acp-failed",
    });
  });
});
