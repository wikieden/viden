// @vitest-environment jsdom

/**
 * E3e: a Lane selected on a quiet Core never gets its own projection
 * (`docs/release-evidence/gui-trusted-delivery/checkpoints.md`, defect 38 as
 * re-observed natively in E3e).
 *
 * The shipped desktop wires `onCoreWake`, so the cockpit re-reads its
 * projection only when the background pump wakes it, and the pump wakes only
 * for events it drained. A Lane selection is not a Core event. `selectLane`
 * moved the webview's selection and re-read the transcript and the commit bar's
 * owner, but nothing re-read the cockpit projection, which the host scopes to
 * the Lane it is asked about. On a quiet Core the projection therefore stayed
 * on the previous Lane forever: the status bar and titlebar kept naming it, the
 * context dock stood on `Waiting for the selected Lane context.`, and the
 * composer refused with `The selected Lane has no sole Core execution owner.`,
 * because the dock's Lane agent belonged to the other Lane.
 */

import { beforeEach, describe, expect, test } from "vitest";

import {
  renderD1Cockpit,
  type D1CockpitProjection,
  type D1IntentResult,
} from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

const LANE_A = "lane-core";
const LANE_B = "lane-review";

/** What the host answers for a read scoped to `laneId`: that Lane's cockpit. */
function scopedTo(laneId: string): D1CockpitProjection {
  const next = structuredClone(D1_PROJECTION);
  next.selectedLaneId = laneId;
  next.lanes = [
    { id: LANE_A, role: "coder", status: "running", summary: "First lane", branch: "viden/lane-core" },
    { id: LANE_B, role: "coder", status: "running", summary: "Second lane", branch: "viden/lane-review" },
  ];
  next.contextDock.laneAgent = next.contextDock.laneAgent
    ? { ...next.contextDock.laneAgent, laneId }
    : null;
  return next;
}

function free(next: D1CockpitProjection): D1IntentResult {
  return { projection: next, pendingCommandId: null, pendingCommand: null, outcome: { state: "idle", reason: null } };
}

const flush = async (): Promise<void> => {
  for (let tick = 0; tick < 8; tick += 1) await new Promise((resolve) => setTimeout(resolve, 0));
};

function mount(poll: (laneId?: string) => Promise<D1IntentResult>) {
  document.body.innerHTML = '<main id="app"></main>';
  const root = document.querySelector<HTMLElement>("#app")!;
  const wakes: Array<() => void> = [];
  const controller = renderD1Cockpit(
    root,
    scopedTo(LANE_A),
    (async () => free(scopedTo(LANE_A))) as never,
    poll as never,
    undefined,
    undefined,
    {
      showWelcome: false,
      onNavigate: () => undefined,
      // The desktop host: the projection is re-read on a pump wake, never on a
      // timer. No wake is fired below, because a quiet Core sends none.
      onCoreWake: (handler: () => void) => {
        wakes.push(handler);
        return () => undefined;
      },
    },
  );
  return { root, controller, wakes };
}

describe("E3e — a Lane selection re-reads the cockpit on a quiet Core", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("selecting another Lane reads that Lane's projection without waiting for a Core event", async () => {
    const asked: Array<string | undefined> = [];
    const { root, controller } = mount(async (laneId?: string) => {
      asked.push(laneId);
      return free(scopedTo(laneId ?? LANE_A));
    });
    await flush();

    controller.selectLane(LANE_B);
    await flush();

    expect(
      asked,
      "no read of the newly selected Lane's projection was sent: on a quiet Core the cockpit " +
        "stays on the previous Lane's projection forever (E3e, defect 38)",
    ).toContain(LANE_B);
    expect(root.querySelector("[data-context-dock-waiting]")).toBeNull();
    expect(root.textContent ?? "").not.toContain("The selected Lane has no sole Core execution owner.");
    controller.dispose();
  });

  test("a selection made while a drain for the previous Lane is in flight is read again", async () => {
    const asked: Array<string | undefined> = [];
    const gate: { releaseFirst?: () => void } = {};
    const { controller, wakes } = mount((laneId?: string) => {
      asked.push(laneId);
      if (asked.length === 1) {
        return new Promise<D1IntentResult>((resolve) => {
          gate.releaseFirst = () => resolve(free(scopedTo(laneId ?? LANE_A)));
        });
      }
      return Promise.resolve(free(scopedTo(laneId ?? LANE_A)));
    });
    await flush();

    // A pump wake starts a drain for Lane A; the operator selects Lane B while
    // it is still outstanding.
    wakes[0]?.();
    await flush();
    expect(asked).toEqual([LANE_A]);
    controller.selectLane(LANE_B);
    await flush();
    gate.releaseFirst?.();
    await flush();

    expect(asked[asked.length - 1], "the answer for the superseded Lane was the last read").toBe(LANE_B);
    controller.dispose();
  });
});
