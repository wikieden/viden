// @vitest-environment jsdom

/**
 * The webview half of the E3c native defects
 * (`docs/release-evidence/gui-trusted-delivery/checkpoints.md`, defects
 * 29-33 and the client half of 34).
 *
 * Each test drives the shipped `renderD1Cockpit` over the exact state the
 * native run was in. Nothing is mocked that the run exercised for real: the
 * composer, the commit bar, the settings panel and the centre-view router are
 * the production modules and the production shapes.
 */

import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { IDLE_OPERATOR_GIT, type OperatorGitProjection } from "../src/models/operator_git";
import {
  renderD1Cockpit,
  type D1CockpitProjection,
  type D1Intent,
  type D1IntentResult,
} from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

const LANE_A = "lane-core";
const LANE_B = "lane-review";

function projection(overrides: Partial<D1CockpitProjection> = {}): D1CockpitProjection {
  return { ...structuredClone(D1_PROJECTION), ...overrides };
}

/** Two Lanes, exactly the shape E3c drove: one bound, one not. */
function twoLanes(selectedLaneId: string | null): D1CockpitProjection {
  return projection({
    selectedLaneId,
    lanes: [
      {
        id: LANE_A,
        role: "coder",
        status: "running",
        summary: "Streaming cockpit",
        branch: "codex/lane-core",
      },
      {
        id: LANE_B,
        role: "coder",
        status: "running",
        summary: "Review lane",
        branch: "codex/lane-review",
      },
    ],
  });
}

/**
 * Core's answer per Lane: `lane-core` has no exact owner and `lane-review`
 * does, which is the asymmetry that made the stale answer visible natively.
 */
function ownerFor(laneId: string | null): OperatorGitProjection {
  if (laneId === LANE_B) {
    return { ...IDLE_OPERATOR_GIT, capabilityAvailable: true, ownerAvailable: true };
  }
  return {
    ...IDLE_OPERATOR_GIT,
    capabilityAvailable: true,
    ownerAvailable: false,
    ownerUnavailableReason: `D1-OPERATOR-GIT-OWNER: Lane \`${laneId}\` does not have one exact Core operator git owner`,
  };
}

function free(next: D1CockpitProjection): D1IntentResult {
  return {
    projection: next,
    pendingCommandId: null,
    pendingCommand: null,
    outcome: { state: "idle", reason: null },
  };
}

const flush = async (): Promise<void> => {
  for (let index = 0; index < 8; index += 1) await Promise.resolve();
};

function mount(
  initial: D1CockpitProjection,
  options: Record<string, unknown> = {},
  send: (intent: D1Intent) => Promise<D1IntentResult> = async () => free(initial),
) {
  document.body.innerHTML = '<main id="app"></main>';
  const root = document.querySelector<HTMLElement>("#app")!;
  const controller = renderD1Cockpit(
    root,
    initial,
    send as never,
    (async () => free(initial)) as never,
    undefined,
    undefined,
    { poll: false, showWelcome: false, onNavigate: () => undefined, ...options },
  );
  return { root, controller };
}

beforeEach(() => {
  document.body.innerHTML = "";
});

afterEach(() => {
  vi.useRealTimers();
});

/* ---- defect 30: the operator-git owner follows the Lane selection ---- */

describe("defect 30 — the commit bar resolves the selected Lane's owner", () => {
  test("a selection change re-reads the owner and the refusal names only the new Lane", async () => {
    const asked: Array<string | null> = [];
    const { root, controller } = mount(twoLanes(LANE_A), {
      operatorGit: {
        read: async (laneId: string | null) => {
          asked.push(laneId);
          return ownerFor(laneId);
        },
        run: async () => ownerFor(LANE_B),
        poll: async () => ownerFor(LANE_B),
      },
    });
    await flush();
    expect(asked).toEqual([LANE_A]);
    expect(root.querySelector<HTMLElement>("[data-commit-or-push]")?.title).toContain(LANE_A);

    controller.selectLane(LANE_B);
    // Core republishes the same selection, the way the cockpit's next drain
    // carries it. The dock renders the selected Lane's facts only when the two
    // agree, so the assertion below is about the answer, not about a mismatch.
    controller.applyProjection(twoLanes(LANE_B));
    await flush();

    expect(asked).toEqual([LANE_A, LANE_B]);
    const entry = root.querySelector<HTMLElement>("[data-commit-or-push]");
    expect(entry).not.toBeNull();
    expect(entry!.title ?? "").not.toContain(LANE_A);
    controller.dispose();
  });

  test("a selection Core moves for this client is re-read too, and never keeps the previous answer", async () => {
    const asked: Array<string | null> = [];
    const { root, controller } = mount(twoLanes(null), {
      operatorGit: {
        read: async (laneId: string | null) => {
          asked.push(laneId);
          return ownerFor(laneId);
        },
        run: async () => ownerFor(null),
        poll: async () => ownerFor(null),
      },
    });
    await flush();
    expect(asked).toEqual([null]);

    // Core's own answer moves the selection onto a Lane; `selectLane` never
    // runs. This is the E3c path: the Lane the popover created became the
    // selection without the commit bar ever asking about it.
    controller.applyProjection(twoLanes(LANE_B));
    await flush();

    expect(asked).toEqual([null, LANE_B]);
    const entry = root.querySelector<HTMLElement>("[data-commit-or-push]");
    expect(entry!.title ?? "").not.toContain("null");
    controller.dispose();
  });

  test("while the answer for the new selection is outstanding, no other Lane's refusal is on screen", async () => {
    let release: ((state: OperatorGitProjection) => void) | null = null;
    const { root, controller } = mount(twoLanes(LANE_A), {
      operatorGit: {
        read: async (laneId: string | null) => {
          if (laneId === LANE_A) return ownerFor(LANE_A);
          return new Promise<OperatorGitProjection>((resolve) => {
            release = resolve;
          });
        },
        run: async () => ownerFor(LANE_B),
        poll: async () => ownerFor(LANE_B),
      },
    });
    await flush();
    expect(root.querySelector<HTMLElement>("[data-commit-or-push]")?.title).toContain(LANE_A);

    controller.selectLane(LANE_B);
    controller.applyProjection(twoLanes(LANE_B));
    await flush();

    const entry = root.querySelector<HTMLElement>("[data-commit-or-push]");
    expect(entry!.title ?? "").not.toContain(LANE_A);

    release!(ownerFor(LANE_B));
    await flush();
    expect(root.querySelector<HTMLElement>("[data-commit-or-push]")?.hasAttribute("disabled")).toBe(
      false,
    );
    controller.dispose();
  });
});

/* ---- defect 29: a submit behind a live turn is a queued follow-up ---- */

describe("defect 29 — a submit while a turn runs is queued, not refused", () => {
  /** Core reporting a live turn for the selected Lane, as E3c's step 15 was. */
  function running(): D1CockpitProjection {
    const base = projection();
    return {
      ...base,
      composer: { ...base.composer, editable: true, busy: true, canSubmitImmediately: false },
    };
  }

  function queued(next: D1CockpitProjection): D1IntentResult {
    return {
      projection: next,
      pendingCommandId: null,
      pendingCommand: null,
      outcome: { state: "confirmed", reason: null },
    };
  }

  test("the composer's send carries QueueFollowUp and never touches the D1 slot", async () => {
    const view = running();
    const queueFollowUp = vi.fn(async () => queued(view));
    const send = vi.fn(async () => free(view));
    const { root, controller } = mount(view, { queueFollowUp }, send);
    await flush();

    const input = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
    input.value = "Summarise what you just changed";
    input.dispatchEvent(new InputEvent("input", { bubbles: true }));
    root.querySelector<HTMLButtonElement>("[data-composer-send]")!.click();
    await flush();

    expect(queueFollowUp).toHaveBeenCalledTimes(1);
    expect(queueFollowUp.mock.calls[0]).toEqual([
      "lane-core",
      "Summarise what you just changed",
    ]);
    expect(send).not.toHaveBeenCalled();
    controller.dispose();
  });

  test("the label promises a queue only where the queue can be sent", async () => {
    const view = running();
    const withPort = mount(view, { queueFollowUp: vi.fn(async () => queued(view)) });
    await flush();
    expect(withPort.root.querySelector("label")!.textContent).toContain("Queue follow-up");
    withPort.controller.dispose();

    // The same Core state with no queue port: the promise is not made.
    const withoutPort = mount(view, {});
    await flush();
    expect(withoutPort.root.querySelector("label")!.textContent).not.toContain(
      "Queue follow-up",
    );
    withoutPort.controller.dispose();
  });

  test("a follow-up refused by its own occupied queue keeps the text", async () => {
    const view = running();
    // The host answers with the queue still pending: the next follow-up has
    // nowhere to go, and defect 23's refusal applies unchanged.
    const queueFollowUp = vi.fn(async () => ({
      projection: view,
      pendingCommandId: "gui-d1-queue-1",
      pendingCommand: {
        commandId: "gui-d1-queue-1",
        kind: "queue_follow_up",
        ageMs: 3_000,
        abandonable: false,
      },
      outcome: { state: "pending" as const, reason: null },
    }));
    const { root, controller } = mount(view, { queueFollowUp });
    await flush();

    const type = (text: string): void => {
      const input = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
      input.value = text;
      input.dispatchEvent(new InputEvent("input", { bubbles: true }));
      root.querySelector<HTMLButtonElement>("[data-composer-send]")!.click();
    };
    type("first follow-up");
    await flush();
    type("second follow-up");
    await flush();

    expect(queueFollowUp).toHaveBeenCalledTimes(1);
    const alert = root.querySelector<HTMLElement>("[data-d1-slot-busy]");
    expect(alert).not.toBeNull();
    expect(alert!.textContent).toContain("queue_follow_up");
    expect(root.querySelector<HTMLTextAreaElement>("[data-composer]")!.value).toBe(
      "second follow-up",
    );
    controller.dispose();
  });

  test("the queued row is Core's own InputQueued fact, and leaves when Core drains it", async () => {
    const view = running();
    const { root, controller } = mount(view, {
      queueFollowUp: vi.fn(async () => queued(view)),
    });
    await flush();
    // The fixture's own queued input, which is where `InputQueued` lands. The
    // client draws no queued row of its own: a locally invented one would
    // survive a rejection Core never accepted.
    const bar = (): HTMLElement => root.querySelector<HTMLElement>("[data-live-work-bar]")!;
    expect(bar().textContent).toContain("continue");

    const drained: D1CockpitProjection = {
      ...view,
      liveWork: { ...view.liveWork, queuedInputs: [] },
    };
    controller.applyProjection(drained);
    await flush();
    expect(bar().textContent).not.toContain("continue");
    controller.dispose();
  });
});

/* ---- defect 31: the standing list mirrors Core's revoke ---- */

describe("defect 31 — a revoked standing rule leaves the settings list", () => {
  const RULE = {
    ruleId: "rule_edit_file",
    toolName: "edit_file",
    ruleContent: "edit_file(new=Requeued old=Edited)",
    grantedAt: 1_789_352_875,
    grantedInWorkspace: "ws_65ba50067a87fc27",
    auditId: "audit_1789352746460919000",
  };

  /** The cockpit projection Core published before the revoke. */
  function installed(): D1CockpitProjection {
    const base = projection();
    return {
      ...base,
      permissionDock: {
        ...base.permissionDock,
        standing: { capabilityAvailable: true, rules: [RULE] },
      },
    };
  }

  test("the row leaves on Core's own answer, without waiting for another snapshot", async () => {
    const view = installed();
    // Core's answer: the rule is gone. The native run's cockpit projection was
    // never refreshed after this, because the revoke's own drain consumed the
    // `StandingApprovalRevoked` and a quiet Core produced no later wake — so
    // the panel kept listing a rule that no longer stood.
    const revokeStandingApproval = vi.fn(async () => ({
      projection: { capabilityAvailable: true, rules: [] },
      outcome: { state: "confirmed", reason: null },
    }));
    const { root, controller } = mount(view, {
      revokeStandingApproval,
      preferences: {
        isAvailable: async () => true,
        save: async () => ({ status: "unavailable" }),
        restore: async () => ({ status: "unavailable" }),
      },
    });
    await flush();

    root.querySelector<HTMLButtonElement>("[data-settings-toggle]")!.click();
    await flush();
    const panel = (): HTMLElement => document.querySelector<HTMLElement>("[data-settings-panel]")!;
    expect(panel().querySelector("[data-settings-standing-rule='rule_edit_file']")).not.toBeNull();

    panel()
      .querySelector<HTMLButtonElement>("[data-settings-standing-revoke='rule_edit_file']")!
      .click();
    await flush();

    expect(revokeStandingApproval).toHaveBeenCalledWith("rule_edit_file");
    expect(panel().querySelector("[data-settings-standing-rule='rule_edit_file']")).toBeNull();
    expect(panel().querySelector("[data-settings-standing-absent='empty']")).not.toBeNull();
    controller.dispose();
  });

  test("a refused revoke leaves the rule listed, because it still stands", async () => {
    const view = installed();
    const revokeStandingApproval = vi.fn(async () => ({
      projection: { capabilityAvailable: true, rules: [RULE] },
      outcome: { state: "rejected", reason: "Core refused the revoke." },
    }));
    const { root, controller } = mount(view, {
      revokeStandingApproval,
      preferences: {
        isAvailable: async () => true,
        save: async () => ({ status: "unavailable" }),
        restore: async () => ({ status: "unavailable" }),
      },
    });
    await flush();
    root.querySelector<HTMLButtonElement>("[data-settings-toggle]")!.click();
    await flush();
    const panel = (): HTMLElement => document.querySelector<HTMLElement>("[data-settings-panel]")!;
    panel()
      .querySelector<HTMLButtonElement>("[data-settings-standing-revoke='rule_edit_file']")!
      .click();
    await flush();

    expect(panel().querySelector("[data-settings-standing-rule='rule_edit_file']")).not.toBeNull();
    expect(
      panel().querySelector<HTMLElement>("[data-settings-standing-outcome='rejected']")?.textContent,
    ).toContain("Core refused the revoke.");
    controller.dispose();
  });
});
