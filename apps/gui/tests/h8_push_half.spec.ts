// @vitest-environment jsdom

/**
 * E3b defect 25: the `git_push` half of `Commit & Push ⇡` reaching no
 * decidable surface (E3b step 22, E3d, H7 step 9).
 *
 * The Rust half of this defect is already green. `h7_shell_path.rs` drives the
 * production shell bodies against live Core with the desktop pump running, and
 * the push sent from the commit's own observation reaches Core, raises its
 * `git_push` approval on the dock the cockpit reads, and settles with Core's
 * typed `no_upstream`. So the shell path is not where the half is lost.
 *
 * What the shipped desktop adds on top of that path is its *drain model*:
 * `main.ts` wires `onCoreWake`, so `renderD1Cockpit` sets `coreWakeActive` and
 * `schedulePoll` returns immediately — the cockpit projection is re-read only
 * when the background pump emits a wake, and the pump emits a wake only for
 * events **it** drained. Every Tauri command drains ordered events itself
 * (`GuiCoreAdapter::poll_operator_git`, `apps/gui/src-tauri/src/adapter.rs`),
 * so an `ApprovalRequested` consumed inside `run_operator_git_action` is an
 * event the pump never sees and never wakes on.
 *
 * That is the whole of defect 25. Core publishes the push's `ApprovalRequested`
 * and then goes silent — there is nothing left to publish until someone answers
 * it — so the one event that would have woken the webview is the one the push
 * command swallowed. The commit bar learns about the approval, because the
 * action projection is the command's own return value; the permission dock, the
 * Decision Center and the rail badge do not, because nothing re-reads them. The
 * operator is told `Push is waiting for approval in the permission dock.` beside
 * `0 awaiting you`, for as long as they care to watch.
 */

import { beforeEach, describe, expect, test, vi } from "vitest";

import type { WorkspaceDiffProjection } from "../src/models/diff_review";
import {
  IDLE_OPERATOR_GIT,
  type OperatorGitActionRequest,
  type OperatorGitProjection,
} from "../src/models/operator_git";
import {
  renderD1Cockpit,
  type D1CockpitProjection,
  type D1IntentResult,
} from "../src/screens/d1_cockpit";
import { renderDiffReview } from "../src/screens/diff_review";
import { D1_PROJECTION } from "./support/d1_projection";

const LANE = "lane-core";

const SOURCE = {
  status: "ready",
  branch: "viden/lane-core",
  worktree: "/tmp/fixture/.worktrees/lane-core",
  ahead: 1,
  behind: 0,
  added: 1,
  deleted: 0,
  dirty: true,
};

function page(): WorkspaceDiffProjection {
  return {
    outcome: { state: "confirmed", reason: null },
    targetLaneId: LANE,
    source: SOURCE,
    entries: [
      { path: "README.md", index: "modified", worktree: null, staged: true, diff: null },
    ],
    truncated: false,
    loaded: true,
    pendingCommandId: null,
    capabilityAvailable: true,
    stale: false,
  };
}

function actionState(overrides: Partial<OperatorGitProjection> = {}): OperatorGitProjection {
  return {
    ...IDLE_OPERATOR_GIT,
    capabilityAvailable: true,
    ownerAvailable: true,
    ...overrides,
  };
}

/** Core's own `git_push` ask, as the dock projection carries it. */
function approvalRequest(
  toolName: string,
): NonNullable<D1CockpitProjection["permissionDock"]["request"]> {
  return {
    editable: false,
    revisedFrom: null,
    inputFields: [{ key: "set_upstream", value: "false" }],
    editBlock: null,
    id: `approval-${toolName}`,
    toolName,
    title: `Approve ${toolName}`,
    message: "Core requests scoped permission",
    inputPreview: "set_upstream: false",
    isMutating: true,
    reason: "operator source-control action",
    risk: "medium",
    target: { kind: "repo_path", display: "/tmp/fixture", canonicalRef: "repo://" },
    policyReasonKey: "permission.requires_approval",
    policyReasonArgs: {},
    expiresAt: 1_700_003_600,
    defaultAction: "deny",
    auditId: `audit-${toolName}`,
    blockedByPlan: false,
    actions: [
      { kind: "once", available: true, sessionId: null, paths: [], code: null },
      { kind: "deny", available: true, sessionId: null, paths: [], code: null },
    ],
  };
}

/**
 * A scripted Core that answers the way the shipped shell answers.
 *
 * The one rule that makes it the shell and not a mock: an approval Core raises
 * while a command is in flight is consumed by **that command's** drain, so it
 * shows up in the command's own return value and emits **no wake**. Only an
 * event no command was holding the lock for wakes the webview.
 */
class ScriptedCore {
  readonly sent: OperatorGitActionRequest[] = [];
  private projection: D1CockpitProjection;
  private action: OperatorGitProjection = actionState();
  private wake: (() => void) | null = null;

  constructor() {
    this.projection = structuredClone(D1_PROJECTION);
    this.projection.selectedLaneId = LANE;
  }

  view(): D1CockpitProjection {
    return structuredClone(this.projection);
  }

  result(): D1IntentResult {
    return {
      projection: this.view(),
      pendingCommandId: null,
      pendingCommand: null,
      outcome: { state: "idle", reason: null },
    };
  }

  onCoreWake = (handler: () => void): (() => void) => {
    this.wake = handler;
    return () => {
      this.wake = null;
    };
  };

  /** One pump wake, for an event no command drained first. */
  pumpWake(): void {
    this.wake?.();
  }

  /** Core raises an ask; the in-flight command's own drain takes the event. */
  private raise(toolName: string, verb: string): void {
    this.projection.permissionDock = {
      ...this.projection.permissionDock,
      request: approvalRequest(toolName),
    };
    this.action = actionState({
      outcome: { state: "pending", reason: null },
      pendingCommandId: `gui-git-${verb}`,
      pendingAction: verb,
      awaitingApproval: true,
    });
  }

  /** The operator answers the dock, and Core completes the action. */
  completeCommit(): void {
    this.projection.permissionDock = {
      ...this.projection.permissionDock,
      request: null,
    };
    this.action = actionState({
      outcome: { state: "confirmed", reason: null },
      result: {
        kind: "completed",
        action: "commit",
        targetLaneId: LANE,
        auditId: "audit_commit",
        output: "commit ok",
        truncated: false,
        source: { ...SOURCE, ahead: 2, dirty: false },
        failureClass: null,
        detail: null,
      },
    });
  }

  operatorGitPort() {
    return {
      read: async (): Promise<OperatorGitProjection> => this.action,
      run: async (
        _laneId: string | null,
        action: OperatorGitActionRequest,
      ): Promise<OperatorGitProjection> => {
        this.sent.push(action);
        if (action.type === "commit") this.raise("git_commit", "commit");
        else if (action.type === "push") this.raise("git_push", "push");
        return this.action;
      },
      poll: async (): Promise<OperatorGitProjection> => this.action,
    };
  }
}

const flush = async (): Promise<void> => {
  for (let tick = 0; tick < 6; tick += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
};

describe("defect 25 — the push half must reach a surface the operator can decide", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("the approval the bar says it is waiting for is on the dock the operator reads", async () => {
    const core = new ScriptedCore();
    const root = document.createElement("div");
    document.body.replaceChildren(root);
    const initial = core.view();
    const controller = renderD1Cockpit(
      root,
      initial,
      (async () => core.result()) as never,
      (async () => core.result()) as never,
      undefined,
      undefined,
      {
        showWelcome: false,
        onNavigate: () => undefined,
        onCoreWake: core.onCoreWake,
        workspaceDiff: { read: async () => page(), query: async () => page() },
        operatorGit: core.operatorGitPort(),
      },
    );
    await flush();

    root.querySelector<HTMLButtonElement>("[data-topbar-review]")?.click();
    await flush();

    // `Commit & Push ⇡`: the client sends the commit and arms the pair.
    const message = root.querySelector<HTMLInputElement>("[data-review-commit-message]");
    if (!message) throw new Error("no commit message box");
    message.value = "Add the push-leg line";
    message.dispatchEvent(new Event("input", { bubbles: true }));
    root.querySelector<HTMLButtonElement>("[data-review-action='commit_push']")?.click();
    await flush();

    // Core's `git_commit` ask reached the dock on a wake, because the operator
    // sat there long enough for the pump to drain something of its own.
    core.pumpWake();
    await flush();
    expect(root.querySelector("[data-permission-dock]")?.textContent).toContain("git_commit");

    // `Approve once`, and Core completes the commit. The wake that carries the
    // completion is the drain that sends the push — `advanceCommitPair`.
    core.completeCommit();
    core.pumpWake();
    await flush();

    expect(core.sent).toEqual([
      { type: "commit", message: "Add the push-leg line" },
      { type: "push", remote: null, setUpstream: false },
    ]);

    // Core raised the push's `git_push` ask *inside* that command's drain, so
    // no wake follows and nothing else is coming. This is the whole of defect
    // 25: whatever the bar says now, it must be something the operator can act
    // on.
    await flush();

    const bar = root.querySelector("[data-review-action-block]")?.textContent ?? "";
    const dock = root.querySelector("[data-permission-dock]")?.textContent ?? "";
    if (bar.includes("waiting for approval")) {
      expect(
        dock,
        `the bar says \`${bar.trim()}\` while the permission dock holds nothing to decide — ` +
          "E3b defect 25, a push that is waiting on an approval the operator cannot reach",
      ).toContain("git_push");
    }
    expect(
      dock,
      "the push's own approval never reached the surface the operator reads, so the half the " +
        "pair owes has no decidable surface at all",
    ).toContain("git_push");

    controller.dispose();
  });
});

describe("defect 25 — a pair whose push never left says so and offers Push", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  /**
   * The honest render for the one success that is also a half-done job.
   *
   * A `Commit & Push` whose commit completed and whose push never left reads,
   * without this, as a published branch: the completed line is true and it is
   * the only thing on screen. `pairStopped` beside a completed commit is
   * therefore its own sentence and its own control, not the existing
   * "the commit did not complete" note, which would be a second untruth.
   */
  test("a completed commit whose push was dropped names the missing half", () => {
    const host = document.createElement("div");
    document.body.append(host);
    const onPush = vi.fn();
    renderDiffReview(host, page(), "en", {
      actions: {
        state: actionState({
          outcome: { state: "confirmed", reason: null },
          result: {
            kind: "completed",
            action: "commit",
            targetLaneId: LANE,
            auditId: "audit_commit",
            output: null,
            truncated: false,
            source: { ...SOURCE, ahead: 2, dirty: false },
            failureClass: null,
            detail: null,
          },
        }),
        // The draft is still the operator's: it is retired only once the push
        // has actually left the client.
        message: "Add the push-leg line",
        pairStopped: true,
        onMessageChange: vi.fn(),
        onStageAll: vi.fn(),
        onCommit: vi.fn(),
        onCommitPush: vi.fn(),
        onToggleStaged: vi.fn(),
        onPush,
        onPushSetUpstream: vi.fn(),
        onFetch: vi.fn(),
      },
    });

    const note = host.querySelector<HTMLElement>("[data-review-pair-stopped='push_not_sent']");
    expect(note?.textContent).toContain("the push was not sent");
    expect(note?.getAttribute("role")).toBe("alert");
    expect(host.querySelector<HTMLInputElement>("[data-review-commit-message]")?.value).toBe(
      "Add the push-leg line",
    );

    const push = host.querySelector<HTMLButtonElement>("[data-review-recovery='push']");
    expect(push?.disabled).toBe(false);
    push?.click();
    expect(onPush).toHaveBeenCalledTimes(1);
  });
});
