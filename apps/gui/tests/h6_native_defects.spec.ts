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
