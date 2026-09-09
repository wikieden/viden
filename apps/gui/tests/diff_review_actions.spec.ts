// @vitest-environment jsdom

/**
 * The DiffReview action side (`runtime.operator_git`, GUI-CORE-020).
 *
 * The rules under test are the honesty rules the source-control contract
 * states for actions, each of which has exactly one wrong way to render it:
 *
 * - a pre-effect `CommandRejected` and a post-gate `Failed` are different
 *   facts and never share a sentence;
 * - a success line reads Core's *resampled* source, never the output text;
 * - a failure is keyed on Core's class, and the recovery offered is the one
 *   the contract names for that class — fetch for a non-fast-forward, set
 *   upstream for a missing one, and nothing at all for an auth failure, which
 *   must not become a retry loop;
 * - every control that cannot act is visible, disabled, and labelled with the
 *   reason: an absent capability names the capability, an absent Core owner
 *   names the client-local code.
 */

import { beforeEach, describe, expect, test, vi } from "vitest";

import type { WorkspaceDiffProjection } from "../src/models/diff_review";
import {
  IDLE_OPERATOR_GIT,
  MAX_COMMIT_MESSAGE_BYTES,
  type OperatorGitActionRequest,
  type OperatorGitProjection,
} from "../src/models/operator_git";
import { renderD1Cockpit, type D1CockpitProjection } from "../src/screens/d1_cockpit";
import { renderDiffReview, type DiffReviewHandlers } from "../src/screens/diff_review";
import { D1_PROJECTION } from "./support/d1_projection";

const SOURCE = {
  status: "ready",
  branch: "claude/gui-diff-review-actions",
  worktree: "/workspace/viden",
  ahead: 1,
  behind: 0,
  added: 2,
  deleted: 0,
  dirty: true,
};

function page(overrides: Partial<WorkspaceDiffProjection> = {}): WorkspaceDiffProjection {
  return {
    outcome: { state: "confirmed", reason: null },
    targetLaneId: "lane-alpha",
    source: SOURCE,
    entries: [
      {
        path: "crates/types/src/source_control.rs",
        index: "modified",
        worktree: null,
        staged: true,
        diff: null,
      },
      {
        path: "apps/gui/src/screens/diff_review.ts",
        index: null,
        worktree: "modified",
        staged: false,
        diff: null,
      },
    ],
    truncated: false,
    loaded: true,
    pendingCommandId: null,
    capabilityAvailable: true,
    stale: false,
    ...overrides,
  };
}

function actions(overrides: Partial<OperatorGitProjection> = {}): OperatorGitProjection {
  return {
    ...IDLE_OPERATOR_GIT,
    capabilityAvailable: true,
    ownerAvailable: true,
    ...overrides,
  };
}

interface MountedActions {
  host: HTMLElement;
  onStageAll: ReturnType<typeof vi.fn>;
  onCommit: ReturnType<typeof vi.fn>;
  onCommitPush: ReturnType<typeof vi.fn>;
  onToggleStaged: ReturnType<typeof vi.fn>;
  onPushSetUpstream: ReturnType<typeof vi.fn>;
  onFetch: ReturnType<typeof vi.fn>;
  onMessageChange: ReturnType<typeof vi.fn>;
}

function mount(
  state: OperatorGitProjection,
  message = "",
  projection: WorkspaceDiffProjection = page(),
  extra: Partial<DiffReviewHandlers> = {},
): MountedActions {
  const host = document.createElement("div");
  document.body.append(host);
  const mounted: MountedActions = {
    host,
    onStageAll: vi.fn(),
    onCommit: vi.fn(),
    onCommitPush: vi.fn(),
    onToggleStaged: vi.fn(),
    onPushSetUpstream: vi.fn(),
    onFetch: vi.fn(),
    onMessageChange: vi.fn(),
  };
  renderDiffReview(host, projection, "en", {
    ...extra,
    actions: {
      state,
      message,
      onMessageChange: mounted.onMessageChange,
      onStageAll: mounted.onStageAll,
      onCommit: mounted.onCommit,
      onCommitPush: mounted.onCommitPush,
      onToggleStaged: mounted.onToggleStaged,
      onPushSetUpstream: mounted.onPushSetUpstream,
      onFetch: mounted.onFetch,
    },
  });
  return mounted;
}

/// The canonical D1 fixture with a dirty workspace, so the titlebar's review
/// entry exists and the sync chip has real counts to decide a direction from.
function cockpitProjection(): D1CockpitProjection {
  const projection = structuredClone(D1_PROJECTION);
  projection.topbarSource = {
    project: "viden",
    branch: "claude/gui-diff-review-actions",
    ahead: 1,
    behind: 0,
    dirty: true,
    status: "ready",
    truncated: false,
    laneWorktreeCount: 1,
  };
  return projection;
}

const button = (host: HTMLElement, selector: string): HTMLButtonElement => {
  const found = host.querySelector<HTMLButtonElement>(selector);
  if (!found) throw new Error(`missing control ${selector}`);
  return found;
};

describe("DiffReview commit bar", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("a typed message enables Commit and Commit & Push and sends each once", () => {
    const mounted = mount(actions(), "feat(gui): land the commit bar");

    const input = mounted.host.querySelector<HTMLInputElement>("[data-review-commit-message]");
    expect(input).not.toBeNull();
    expect(input?.value).toBe("feat(gui): land the commit bar");

    const commit = button(mounted.host, "[data-review-action='commit']");
    const commitPush = button(mounted.host, "[data-review-action='commit_push']");
    const stageAll = button(mounted.host, "[data-review-action='stage_all']");
    expect(commit.disabled).toBe(false);
    expect(commitPush.disabled).toBe(false);
    expect(stageAll.disabled).toBe(false);

    commit.click();
    commitPush.click();
    stageAll.click();
    expect(mounted.onCommit).toHaveBeenCalledTimes(1);
    expect(mounted.onCommitPush).toHaveBeenCalledTimes(1);
    expect(mounted.onStageAll).toHaveBeenCalledTimes(1);
  });

  test("an empty message disables the two commit actions but never Stage all", () => {
    const mounted = mount(actions(), "   ");

    expect(button(mounted.host, "[data-review-action='commit']").disabled).toBe(true);
    expect(button(mounted.host, "[data-review-action='commit_push']").disabled).toBe(true);
    // Staging needs no message: refusing it would be a client-invented rule.
    expect(button(mounted.host, "[data-review-action='stage_all']").disabled).toBe(false);
  });

  test("a message over Core's byte bound is refused here with the real counts", () => {
    // Multi-byte on purpose: Core counts UTF-8 bytes, and a character-based
    // check would let a Chinese message past a bound Core then refuses.
    const message = "更".repeat(MAX_COMMIT_MESSAGE_BYTES);
    const mounted = mount(actions(), message);

    const note = mounted.host.querySelector<HTMLElement>("[data-review-commit-overflow]");
    expect(note?.hidden).toBe(false);
    expect(note?.textContent).toContain(String(MAX_COMMIT_MESSAGE_BYTES));
    expect(button(mounted.host, "[data-review-action='commit']").disabled).toBe(true);

    // Under the bound the note is present but hidden, so appearing and
    // disappearing as the operator types never moves the bar.
    const within = mount(actions(), "feat: short enough");
    expect(
      within.host.querySelector<HTMLElement>("[data-review-commit-overflow]")?.hidden,
    ).toBe(true);
  });

  test("an absent capability leaves every control visible, disabled and named", () => {
    const mounted = mount(actions({ capabilityAvailable: false }), "feat: x");

    const commit = button(mounted.host, "[data-review-action='commit']");
    expect(commit.disabled).toBe(true);
    expect(commit.title).toContain("runtime.operator_git");
    // GUI-CORE-020 closed on the Core side; an absent capability is a fact
    // about this Core build, not an open register entry.
    expect(commit.title).not.toContain("GUI-CORE-020");
    expect(mounted.host.querySelector("[data-review-commit-message]")).not.toBeNull();
  });

  test("an absent Core owner disables the bar with the client-local reason", () => {
    const mounted = mount(
      actions({
        ownerAvailable: false,
        ownerUnavailableReason:
          "D1-OPERATOR-GIT-OWNER: no Lane is selected, so this client has no Core owner",
      }),
      "feat: x",
    );

    const commit = button(mounted.host, "[data-review-action='commit']");
    expect(commit.disabled).toBe(true);
    expect(commit.title).toContain("D1-OPERATOR-GIT-OWNER");
  });

  test("an action in flight names what it is waiting on and stays inert", () => {
    const mounted = mount(
      actions({
        outcome: { state: "pending", reason: null },
        pendingCommandId: "gui-git-1",
        pendingAction: "commit",
      }),
      "feat: x",
    );

    const pending = mounted.host.querySelector("[data-review-action-state='pending']");
    expect(pending?.textContent).toContain("Commit");
    expect(button(mounted.host, "[data-review-action='commit']").disabled).toBe(true);
    expect(button(mounted.host, "[data-review-action='stage_all']").disabled).toBe(true);
  });

  test("a pending git approval says the dock owns the decision", () => {
    const mounted = mount(
      actions({
        outcome: { state: "pending", reason: null },
        pendingCommandId: "gui-git-1",
        pendingAction: "commit",
        awaitingApproval: true,
      }),
      "feat: x",
    );

    const state = mounted.host.querySelector("[data-review-action-state='awaiting_approval']");
    expect(state).not.toBeNull();
    expect(button(mounted.host, "[data-review-action='commit']").disabled).toBe(true);
  });
});

describe("DiffReview action outcomes", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("a completed action reads Core's resampled source, not its output", () => {
    const mounted = mount(
      actions({
        outcome: { state: "confirmed", reason: null },
        result: {
          kind: "completed",
          action: "commit",
          targetLaneId: "lane-alpha",
          auditId: "audit_operator_git_commit",
          output: "[claude/x 1a2b3c4] feat(gui): land the commit bar\n 1 file changed",
          truncated: true,
          source: { ...SOURCE, ahead: 2, dirty: false },
          failureClass: null,
          detail: null,
        },
      }),
    );

    const success = mounted.host.querySelector("[data-review-action-state='completed']");
    expect(success).not.toBeNull();
    // The numbers come from `source`; nothing is read out of the transcript.
    expect(success?.textContent).toContain("2");
    expect(success?.textContent).toContain("claude/gui-diff-review-actions");

    const output = mounted.host.querySelector<HTMLDetailsElement>("[data-review-action-output]");
    expect(output).not.toBeNull();
    expect(output?.open).toBe(false);
    expect(output?.textContent).toContain("1 file changed");
    expect(mounted.host.querySelector("[data-review-action-truncated]")).not.toBeNull();
  });

  test("a NoUpstream failure offers the set_upstream push the contract names", () => {
    const mounted = mount(
      actions({
        outcome: { state: "confirmed", reason: null },
        result: {
          kind: "failed",
          action: "push",
          targetLaneId: "lane-alpha",
          auditId: "audit_operator_git_push",
          output: null,
          truncated: false,
          source: null,
          failureClass: "no_upstream",
          detail: "the current branch has no upstream branch; push with set_upstream to create one",
        },
      }),
    );

    const failure = mounted.host.querySelector("[data-review-action-state='failed']");
    expect(failure?.getAttribute("data-review-failure-class")).toBe("no_upstream");
    expect(failure?.textContent).toContain("upstream");

    const recovery = button(mounted.host, "[data-review-recovery='set_upstream']");
    recovery.click();
    expect(mounted.onPushSetUpstream).toHaveBeenCalledTimes(1);
    expect(mounted.host.querySelector("[data-review-recovery='fetch']")).toBeNull();
  });

  test("a NonFastForward failure offers fetch, and pull is never offered", () => {
    const mounted = mount(
      actions({
        outcome: { state: "confirmed", reason: null },
        result: {
          kind: "failed",
          action: "push",
          targetLaneId: null,
          auditId: "audit_operator_git_push",
          output: null,
          truncated: false,
          source: null,
          failureClass: "non_fast_forward",
          detail: "Updates were rejected because the remote contains work you do not have",
        },
      }),
    );

    button(mounted.host, "[data-review-recovery='fetch']").click();
    expect(mounted.onFetch).toHaveBeenCalledTimes(1);
    expect(mounted.host.querySelector("[data-review-recovery='pull']")).toBeNull();
  });

  test("an authentication failure explains and offers no retry control", () => {
    const mounted = mount(
      actions({
        outcome: { state: "confirmed", reason: null },
        result: {
          kind: "failed",
          action: "push",
          targetLaneId: null,
          auditId: "audit_operator_git_push",
          output: null,
          truncated: false,
          source: null,
          failureClass: "authentication_required",
          detail: "could not read Username for 'https://github.com'",
        },
      }),
    );

    expect(
      mounted.host.querySelector("[data-review-action-state='failed']")?.textContent,
    ).toBeTruthy();
    expect(mounted.host.querySelector("[data-review-recovery]")).toBeNull();
  });

  test("an unclassified failure shows Core's detail verbatim", () => {
    const detail = "fatal: something this build has never heard of";
    const mounted = mount(
      actions({
        outcome: { state: "confirmed", reason: null },
        result: {
          kind: "failed",
          action: "fetch",
          targetLaneId: null,
          auditId: "audit_operator_git_fetch",
          output: null,
          truncated: false,
          source: null,
          failureClass: "unknown",
          detail,
        },
      }),
    );

    expect(
      mounted.host.querySelector("[data-review-action-detail]")?.textContent,
    ).toBe(detail);
  });

  test("a pre-effect refusal is Core's own reason and never a failure line", () => {
    const reason =
      "permission denied\ntool: git_add\nreason: DenyRule\nmessage: git_add is denied by a workspace rule";
    const mounted = mount(actions({ outcome: { state: "rejected", reason } }));

    const rejected = mounted.host.querySelector("[data-review-action-state='rejected']");
    expect(rejected?.textContent).toBe(reason);
    expect(rejected?.getAttribute("role")).toBe("alert");
    expect(mounted.host.querySelector("[data-review-action-state='failed']")).toBeNull();
  });
});

describe("DiffReview per-file staging", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("each row toggles between Stage and Unstage from Core's own flag", () => {
    const mounted = mount(actions());

    const rows = mounted.host.querySelectorAll<HTMLButtonElement>("[data-review-stage-toggle]");
    expect(rows).toHaveLength(2);
    const staged = mounted.host.querySelector<HTMLButtonElement>(
      "[data-review-stage-toggle='crates/types/src/source_control.rs']",
    );
    const unstaged = mounted.host.querySelector<HTMLButtonElement>(
      "[data-review-stage-toggle='apps/gui/src/screens/diff_review.ts']",
    );
    expect(staged?.dataset.reviewStaged).toBe("true");
    expect(unstaged?.dataset.reviewStaged).toBe("false");

    staged?.click();
    expect(mounted.onToggleStaged).toHaveBeenCalledWith(
      "crates/types/src/source_control.rs",
      true,
    );
    unstaged?.click();
    expect(mounted.onToggleStaged).toHaveBeenCalledWith(
      "apps/gui/src/screens/diff_review.ts",
      false,
    );
  });

  test("the toggle never selects the row it sits in", () => {
    const mounted = mount(actions());
    const onSelect = vi.fn();
    renderDiffReview(mounted.host, page(), "en", {
      onSelect,
      actions: {
        state: actions(),
        message: "",
        onMessageChange: vi.fn(),
        onStageAll: vi.fn(),
        onCommit: vi.fn(),
        onCommitPush: vi.fn(),
        onToggleStaged: mounted.onToggleStaged,
        onPushSetUpstream: vi.fn(),
        onFetch: vi.fn(),
      },
    });
    mounted.host
      .querySelector<HTMLButtonElement>(
        "[data-review-stage-toggle='apps/gui/src/screens/diff_review.ts']",
      )
      ?.click();
    expect(onSelect).not.toHaveBeenCalled();
  });

  test("without the capability the row toggles are disabled, not removed", () => {
    const mounted = mount(actions({ capabilityAvailable: false }));
    const toggles = mounted.host.querySelectorAll<HTMLButtonElement>(
      "[data-review-stage-toggle]",
    );
    expect(toggles).toHaveLength(2);
    for (const toggle of toggles) expect(toggle.disabled).toBe(true);
  });

  test("a read-only review without an action port draws no toggles at all", () => {
    const host = document.createElement("div");
    renderDiffReview(host, page(), "en", {});
    expect(host.querySelectorAll("[data-review-stage-toggle]")).toHaveLength(0);
    // The bar itself stays, disabled, so the operator can see what is coming.
    expect(host.querySelector("[data-review-commit]")).not.toBeNull();
    expect(host.querySelector<HTMLButtonElement>("[data-review-action='commit']")?.disabled).toBe(
      true,
    );
  });
});

/* ------------------------------------------------------------------ */
/* Cockpit wiring: the two-command "commit and push" pair              */
/* ------------------------------------------------------------------ */

describe("DiffReview commit-and-push sequencing", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  const flush = async (): Promise<void> => {
    for (let tick = 0; tick < 4; tick += 1) {
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
  };

  function completed(action: string, ahead: number): OperatorGitProjection {
    return actions({
      outcome: { state: "confirmed", reason: null },
      result: {
        kind: "completed",
        action,
        targetLaneId: "lane_d1_core",
        auditId: `audit_${action}`,
        output: `${action} ok`,
        truncated: false,
        source: { ...SOURCE, ahead, dirty: false },
        failureClass: null,
        detail: null,
      },
    });
  }

  function failed(action: string, failureClass: string): OperatorGitProjection {
    return actions({
      outcome: { state: "confirmed", reason: null },
      result: {
        kind: "failed",
        action,
        targetLaneId: "lane_d1_core",
        auditId: `audit_${action}`,
        output: null,
        truncated: false,
        source: null,
        failureClass,
        detail: "git said no",
      },
    });
  }

  async function openReview(answers: OperatorGitProjection[]): Promise<{
    root: HTMLElement;
    sent: OperatorGitActionRequest[];
    dispose: () => void;
  }> {
    const root = document.createElement("div");
    document.body.replaceChildren(root);
    const projection = cockpitProjection();
    const sent: OperatorGitActionRequest[] = [];
    const queued = [...answers];
    const controller = renderD1Cockpit(
      root,
      projection,
      async () => ({
        projection,
        pendingCommandId: null,
        outcome: { state: "idle", reason: null },
      }),
      async () => ({
        projection,
        pendingCommandId: null,
        outcome: { state: "idle", reason: null },
      }),
      undefined,
      undefined,
      {
        poll: false,
        showWelcome: false,
        onNavigate: () => undefined,
        workspaceDiff: { read: async () => page(), query: async () => page() },
        operatorGit: {
          read: async () => actions(),
          run: async (_laneId, action) => {
            sent.push(action);
            return queued.shift() ?? actions();
          },
          poll: async () => queued.shift() ?? actions(),
        },
      },
    );
    await flush();
    root.querySelector<HTMLButtonElement>("[data-topbar-review]")?.click();
    await flush();
    return { root, sent, dispose: () => controller.dispose() };
  }

  const type = (root: HTMLElement, message: string): void => {
    const input = root.querySelector<HTMLInputElement>("[data-review-commit-message]");
    if (!input) throw new Error("no commit message box");
    input.value = message;
    input.dispatchEvent(new Event("input", { bubbles: true }));
  };

  test("the push is sent only after the commit reports Completed", async () => {
    const mounted = await openReview([completed("commit", 2), completed("push", 0)]);
    type(mounted.root, "feat(gui): land the commit bar");
    mounted.root.querySelector<HTMLButtonElement>("[data-review-action='commit_push']")?.click();
    await flush();

    expect(mounted.sent).toEqual([
      { type: "commit", message: "feat(gui): land the commit bar" },
      { type: "push", remote: null, setUpstream: false },
    ]);
    mounted.dispose();
  });

  test("a failed commit stops the pair and says the push was not sent", async () => {
    const mounted = await openReview([failed("commit", "nothing_to_commit")]);
    type(mounted.root, "feat(gui): nothing staged");
    mounted.root.querySelector<HTMLButtonElement>("[data-review-action='commit_push']")?.click();
    await flush();

    expect(mounted.sent).toEqual([{ type: "commit", message: "feat(gui): nothing staged" }]);
    expect(mounted.root.querySelector("[data-review-pair-stopped]")).not.toBeNull();
    mounted.dispose();
  });

  test("a plain Commit never queues a push", async () => {
    const mounted = await openReview([completed("commit", 2)]);
    type(mounted.root, "feat(gui): just the commit");
    mounted.root.querySelector<HTMLButtonElement>("[data-review-action='commit']")?.click();
    await flush();

    expect(mounted.sent).toEqual([{ type: "commit", message: "feat(gui): just the commit" }]);
    expect(mounted.root.querySelector("[data-review-pair-stopped]")).toBeNull();
    mounted.dispose();
  });

  test("a row toggle sends the inverse of Core's own staged flag", async () => {
    const mounted = await openReview([completed("unstage", 1)]);
    mounted.root
      .querySelector<HTMLButtonElement>(
        "[data-review-stage-toggle='crates/types/src/source_control.rs']",
      )
      ?.click();
    await flush();

    expect(mounted.sent).toEqual([
      { type: "unstage", paths: ["crates/types/src/source_control.rs"] },
    ]);
    mounted.dispose();
  });

  test("the titlebar chip pushes when Core's resampled counts say there is local work", async () => {
    const mounted = await openReview([completed("push", 0)]);
    mounted.root.querySelector<HTMLButtonElement>("[data-review-close]")?.click();
    await flush();
    const chip = mounted.root.querySelector<HTMLButtonElement>("[data-topbar-sync]");
    expect(chip?.tagName).toBe("BUTTON");
    expect(chip?.dataset.topbarSyncAction).toBe("push");
    chip?.click();
    await flush();

    expect(mounted.sent).toEqual([{ type: "push", remote: null, setUpstream: false }]);
    mounted.dispose();
  });
});
