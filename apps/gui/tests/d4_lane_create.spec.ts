// @vitest-environment jsdom

import { beforeEach, describe, expect, test, vi } from "vitest";

import {
  renderD4LaneCreate,
  type D4Intent,
  type D4IntentResult,
  type D4LaneCreateProjection,
  type D4QueueState,
  type D4StarterSeed,
} from "../src/screens/d4_lane_create";

const SEEDS: readonly D4StarterSeed[] = [
  { laneId: "starter-coder", preset: "coder", branch: null, worktreePath: null },
  { laneId: "starter-reviewer", preset: "reviewer", branch: null, worktreePath: null },
  { laneId: "starter-tester", preset: "tester", branch: null, worktreePath: null },
];

const EMPTY_PROJECTION: D4LaneCreateProjection = {
  availability: {
    available: true,
    capability: "runtime.starter_lane_preview",
    message: "Reviewed starter Lane creation is available.",
  },
  workMode: "build",
  canCreate: false,
  preview: null,
  receipt: null,
  pendingApproval: null,
  outcome: { state: "idle", reason: null, requiresRepreview: false },
  navigationLaneId: null,
};

const PREVIEW = {
  previewId: "preview-starter-coder",
  contentSha256: "a".repeat(64),
  owner: {
    workspaceId: "workspace-gui",
    projectId: "project-gui",
    laneId: "starter-coder",
    sessionId: "session-gui",
    taskId: null,
    turnId: "turn-gui",
  },
  lane: {
    id: "starter-coder",
    role: "coder",
    route: "built_in",
    gateStrength: "full",
    mutationPolicy: "propose_only",
    worktree: "/workspace/.worktrees/starter-coder",
    branch: "codex/starter-coder",
    target: "local",
    dataEgress: "deny",
    status: "draft",
    budget: {
      tokenLimit: null,
      costLimitMicroUsd: null,
      wallTimeLimitSecs: null,
    },
    summary: "coder starter lane",
  },
  branch: "codex/starter-coder",
  worktreePath: "/workspace/.worktrees/starter-coder",
  baseRevision: "b".repeat(40),
  diagnostics: [],
};

function result(
  projection: D4LaneCreateProjection = EMPTY_PROJECTION,
  pendingCommandId: string | null = null,
  pendingIntent: D4IntentResult["pendingIntent"] = null,
): D4IntentResult {
  return { projection, pendingCommandId, pendingIntent };
}

function setup(
  initial = result(),
  queue: readonly D4StarterSeed[] = SEEDS,
  queueIndex = 0,
  queueExtras: Partial<D4QueueState> = {},
) {
  document.body.innerHTML = '<main id="app"></main>';
  const root = document.querySelector<HTMLElement>("#app");
  if (!root) throw new Error("missing test root");
  const send = vi.fn<(intent: D4Intent) => Promise<D4IntentResult>>(async () => result());
  const poll = vi.fn<() => Promise<D4IntentResult>>(async () => result());
  const onCancel = vi.fn();
  const onNavigateToD1 = vi.fn<(laneId: string) => void>();
  const controller = renderD4LaneCreate(root, initial, send, poll, "en", {
    queue,
    queueIndex,
    completedLaneIds: [],
    onCancel,
    onNavigateToD1,
    ...queueExtras,
  });
  return { root, send, poll, onCancel, onNavigateToD1, controller };
}

describe("D4 reviewed starter Lane", () => {
  beforeEach(() => {
    document.documentElement.lang = "en";
  });

  test("renders a keyboard-operable four-step wizard with visible focus semantics", () => {
    const { root, onCancel } = setup();
    const steps = root.querySelectorAll<HTMLButtonElement>("[data-d4-step]");
    expect(steps).toHaveLength(4);
    expect(steps[0]?.getAttribute("aria-current")).toBe("step");
    expect(root.querySelector('[role="radiogroup"][aria-label="Starter role"]')).not.toBeNull();
    expect(root.querySelector('[role="radio"][aria-checked="true"]')?.textContent).toContain(
      "Coder",
    );
    expect(document.activeElement).toBe(root.querySelector("[data-lane-id]"));

    root.querySelector<HTMLElement>('[role="radio"][data-preset="coder"]')?.dispatchEvent(
      new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }),
    );
    expect(root.querySelector('[role="radio"][aria-checked="true"]')?.textContent).toContain(
      "Reviewer",
    );

    root.querySelector<HTMLInputElement>("[data-lane-id]")?.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Enter", bubbles: true }),
    );
    expect(root.querySelector('[data-d4-step="1"]')?.getAttribute("aria-current")).toBe("step");
    expect(document.activeElement).toBe(root.querySelector("[data-step-heading]"));

    root.querySelector<HTMLElement>("[data-screen='d4-lane-create']")?.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
    );
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  test("Ctrl or Command Enter sends the unchanged reviewed request", () => {
    const { root, send } = setup(
      result({ ...EMPTY_PROJECTION, canCreate: true, preview: PREVIEW }),
    );
    root.querySelector<HTMLElement>("[data-screen='d4-lane-create']")?.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Enter", ctrlKey: true, bubbles: true }),
    );
    expect(send).toHaveBeenCalledWith({
      type: "create",
      request: SEEDS[0],
    });
  });

  test("shows route, gate, target, policy and budget only from the Core preview", () => {
    const { root } = setup(result({ ...EMPTY_PROJECTION, canCreate: true, preview: PREVIEW }));
    root.querySelector<HTMLButtonElement>('[data-d4-step="3"]')?.click();

    expect(root.querySelector("[data-resolved-route]")?.textContent).toContain("built_in");
    expect(root.querySelector("[data-resolved-gate]")?.textContent).toContain("full");
    expect(root.querySelector("[data-resolved-target]")?.textContent).toContain("local");
    expect(root.querySelector("[data-resolved-budget]")?.textContent).toContain("Core default");
    expect(root.querySelector("[data-resolved-worktree]")?.textContent).toContain(
      "/workspace/.worktrees/starter-coder",
    );
    expect(root.querySelector("select[data-route-picker]")).toBeNull();
    expect(root.querySelector("input[data-budget-editor]")).toBeNull();
  });

  test("fails closed with visible capability status and sends nothing", () => {
    const { root, send } = setup(
      result({
        ...EMPTY_PROJECTION,
        availability: {
          available: false,
          capability: "runtime.starter_lane_preview",
          message: "Core has not advertised reviewed starter Lane creation.",
        },
      }),
    );

    expect(root.querySelector('[role="alert"]')?.textContent).toContain(
      "runtime.starter_lane_preview",
    );
    root.querySelector<HTMLButtonElement>("[data-preview-starter-lane]")?.click();
    expect(send).not.toHaveBeenCalled();
    expect(root.querySelector<HTMLButtonElement>("[data-preview-starter-lane]")?.disabled).toBe(
      true,
    );
  });

  test("Plan permits preview but keeps create visibly disabled", () => {
    const { root } = setup(
      result({ ...EMPTY_PROJECTION, workMode: "plan", canCreate: false, preview: PREVIEW }),
    );
    root.querySelector<HTMLButtonElement>('[data-d4-step="3"]')?.click();

    expect(root.querySelector<HTMLButtonElement>("[data-preview-starter-lane]")?.disabled).toBe(
      false,
    );
    expect(root.querySelector<HTMLButtonElement>("[data-create-starter-lane]")?.disabled).toBe(
      true,
    );
    expect(root.querySelector("[data-create-disabled-reason]")?.textContent).toContain("Plan");
  });

  test("editing a reviewed request keeps the draft but requires re-preview", () => {
    const { root, send, controller } = setup(
      result({ ...EMPTY_PROJECTION, canCreate: true, preview: PREVIEW }),
    );
    const branch = root.querySelector<HTMLInputElement>("[data-branch]");
    if (!branch) throw new Error("missing branch input");
    branch.value = "codex/changed";
    branch.dispatchEvent(new Event("input", { bubbles: true }));

    expect(controller.state.draft.branch).toBe("codex/changed");
    expect(root.querySelector<HTMLButtonElement>("[data-create-starter-lane]")?.disabled).toBe(
      true,
    );
    expect(root.querySelector("[data-repreview-required]")).not.toBeNull();
    root.querySelector<HTMLButtonElement>("[data-create-starter-lane]")?.click();
    expect(send).not.toHaveBeenCalled();
  });

  test("Cancel and Skip return to the project cockpit without sending before create", () => {
    const cancel = setup();
    cancel.root.querySelector<HTMLButtonElement>("[data-cancel-d4]")?.click();
    expect(cancel.onCancel).toHaveBeenCalledTimes(1);
    expect(cancel.send).not.toHaveBeenCalled();

    const skip = setup();
    skip.root.querySelector<HTMLButtonElement>("[data-skip-d4]")?.click();
    expect(skip.onCancel).toHaveBeenCalledTimes(1);
    expect(skip.send).not.toHaveBeenCalled();
  });

  test("after create is sent there is no fake cancel and denial is explicit", () => {
    const pendingProjection: D4LaneCreateProjection = {
      ...EMPTY_PROJECTION,
      preview: PREVIEW,
      pendingApproval: {
        id: "approval-starter-coder",
        title: "Create starter-coder",
        risk: "medium",
        target: "starter-coder",
      },
      outcome: { state: "waiting_for_approval", reason: null, requiresRepreview: false },
    };
    const { root } = setup(result(pendingProjection, "create-1", "create_starter_lane"));

    expect(root.querySelector("[data-cancel-starter-command]")).toBeNull();
    expect(root.querySelector("[data-create-waiting]")?.textContent).toContain("waiting");
    expect(root.querySelector<HTMLButtonElement>("[data-deny-starter-approval]")).not.toBeNull();
  });

  test("advances the review queue one receipt at a time and focuses the last created Lane", async () => {
    const firstCreated: D4LaneCreateProjection = {
      ...EMPTY_PROJECTION,
      receipt: { ...PREVIEW, lane: { ...PREVIEW.lane, status: "running" } },
      outcome: { state: "created", reason: null, requiresRepreview: false },
      navigationLaneId: "starter-coder",
    };
    const secondCreated: D4LaneCreateProjection = {
      ...firstCreated,
      receipt: {
        ...firstCreated.receipt!,
        previewId: "preview-starter-reviewer",
        lane: { ...PREVIEW.lane, id: "starter-reviewer", role: "reviewer" },
      },
      navigationLaneId: "starter-reviewer",
    };
    const thirdCreated: D4LaneCreateProjection = {
      ...secondCreated,
      receipt: {
        ...secondCreated.receipt!,
        previewId: "preview-starter-tester",
        lane: { ...PREVIEW.lane, id: "starter-tester", role: "tester" },
      },
      navigationLaneId: "starter-tester",
    };
    const screen = setup(result(firstCreated));
    expect(screen.root.querySelector<HTMLInputElement>("[data-lane-id]")?.value).toBe(
      "starter-reviewer",
    );
    expect(screen.onNavigateToD1).not.toHaveBeenCalled();

    screen.poll.mockResolvedValueOnce(result(secondCreated));
    await screen.controller.applyResult(result(secondCreated));
    expect(screen.root.querySelector<HTMLInputElement>("[data-lane-id]")?.value).toBe(
      "starter-tester",
    );
    expect(screen.onNavigateToD1).not.toHaveBeenCalled();

    await screen.controller.applyResult(result(thirdCreated));
    expect(screen.onNavigateToD1).toHaveBeenCalledWith("starter-tester");
  });

  test("invalidated or rejected outcomes preserve draft and expose re-preview", () => {
    const invalid = setup(
      result({
        ...EMPTY_PROJECTION,
        outcome: {
          state: "invalidated",
          reason: "base_revision_changed",
          requiresRepreview: true,
        },
      }),
    );
    expect(invalid.controller.state.draft.laneId).toBe("starter-coder");
    expect(invalid.root.querySelector('[role="alert"]')?.textContent).toContain(
      "base_revision_changed",
    );
    expect(invalid.root.querySelector("[data-repreview-required]")).not.toBeNull();
  });
});

// The design's own four steps (`GUI/pages/Viden - D4 Lane创建流程 (GUI).html`
// `STEPS`): role & workstation, agent, skill pack, gates & target. Each is
// drawn within what Core models — and what Core does not model is stated on
// the step rather than filled in.
describe("D4 design steps", () => {
  beforeEach(() => {
    document.documentElement.lang = "en";
  });

  test("names the design's four steps and reports the position", () => {
    const { root } = setup();
    const steps = Array.from(
      root.querySelectorAll<HTMLButtonElement>("[data-d4-step]"),
      (button) => button.textContent,
    );
    expect(steps).toEqual([
      "1. Role and workstation",
      "2. Choose agent",
      "3. Skill pack",
      "4. Gates and target",
    ]);
    expect(root.querySelector("[data-d4-progress]")?.textContent).toBe(
      "Step 1 of 4 · Role and workstation",
    );
    expect(root.querySelector('[data-d4-step-body="role"]')).not.toBeNull();
    expect(root.querySelector('[data-d4-step-body="agent"]')).toBeNull();
  });

  test("lists the agents Core published and marks the one the popover chose", () => {
    const { root } = setup(result(), SEEDS, 0, {
      seed: { agentId: "codex-acp", task: "Refactor the config loader" },
      agents: [
        { agentId: "codex-acp", displayName: "Codex", startability: "ready" },
        { agentId: "claude-acp", displayName: "Claude", startability: "probe_required" },
      ],
    });
    root.querySelector<HTMLButtonElement>('[data-d4-step="1"]')!.click();

    const rows = Array.from(root.querySelectorAll<HTMLElement>("[data-d4-agent]"));
    expect(rows.map((row) => row.dataset.d4Agent)).toEqual(["codex-acp", "claude-acp"]);
    expect(rows[0]!.dataset.d4AgentChosen).toBe("true");
    expect(rows[1]!.dataset.d4AgentChosen).toBeUndefined();
    // The step says plainly that creating the Lane does not start the agent.
    expect(root.querySelector("[data-d4-agent-note]")?.textContent).toContain(
      "carries no agent",
    );
  });

  test("states the skill-pack step as unavailable instead of inventing one", () => {
    const { root } = setup();
    root.querySelector<HTMLButtonElement>('[data-d4-step="2"]')!.click();

    const note = root.querySelector<HTMLElement>('[data-d4-unavailable="skills"]')!;
    expect(note.textContent).toContain("Core publishes no skill pack");
    expect(note.textContent).toContain("GUI-CORE-030");
    // Nothing selectable, because nothing here could reach Core.
    expect(root.querySelector('[data-d4-step-body="skills"] input')).toBeNull();
    expect(root.querySelector('[data-d4-step-body="skills"] button')).toBeNull();
  });

  test("shows the Core-resolved mutation policy and the one execution target", () => {
    const { root } = setup(result({ ...EMPTY_PROJECTION, canCreate: true, preview: PREVIEW }));
    root.querySelector<HTMLButtonElement>('[data-d4-step="3"]')!.click();

    expect(root.querySelector("[data-resolved-mutation-policy]")?.textContent).toBe(
      "propose_only",
    );
    expect(root.querySelector("[data-d4-target-note]")?.textContent).toContain("local");
    expect(root.querySelector("[data-d4-target-note]")?.textContent).toContain("D9");
    expect(root.querySelector('[data-d4-step-body="gates"] select')).toBeNull();
  });

  test("names the Lane after the task the popover carried, and shows the task", () => {
    const { root, controller } = setup(result(), [], 0, {
      seed: { agentId: null, task: "Refactor the config loader" },
    });

    expect(controller.state.draft.laneId).toBe("refactor-the-config-loader");
    expect(controller.state.draft.branch).toBe("vd/refactor-the-config-loader");
    expect(root.querySelector("[data-seed-task]")?.textContent).toBe(
      "Refactor the config loader",
    );
    // The create command cannot carry the task; the step says so.
    expect(root.querySelector("[data-seed-note]")?.textContent).toContain(
      "not part of the create command",
    );
  });
});
