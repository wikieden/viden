// @vitest-environment jsdom

/**
 * The cockpit's side of `runtime.turn_lifecycle` (C6).
 *
 * The Live Work strip used to answer "is something happening?" with a spinner
 * and a clock this webview started when it noticed. Core now brackets every
 * turn, so the strip answers three questions with Core's own facts: what is
 * running, where it came from, and since when.
 *
 * Four rules, each with a specific failure it prevents:
 *
 * 1. **The source is named.** Typed, queued, and agent are three different
 *    explanations for text appearing in the transcript; without the source a
 *    drained queue entry reads as Core inventing work.
 * 2. **Elapsed is Core's `started_at`.** A client clock restarts on every
 *    remount, so a turn running for ten minutes would read as ten seconds.
 * 3. **The queue says what it is waiting for.** Core drains it only behind a
 *    *completed* turn, so "runs after the current turn" and "waits for the next
 *    completed turn" are two different promises and the strip keeps them apart.
 * 4. **A failed turn's reason is a row.** The composer going quiet with no
 *    sentence is the state an operator cannot act on.
 */

import { describe, expect, test, vi } from "vitest";

import {
  formatElapsed,
  renderWorkStatus,
  workStatusModel,
  type WorkStatusModel,
} from "../src/components/work_status";
import { renderD1Cockpit, type D1Intent, type D1IntentResult } from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

const NOW_SECONDS = 1_700_000_600;

function laneTurn(overrides: Record<string, unknown> = {}) {
  return {
    turnId: "turn-1",
    laneId: "lane-core",
    source: "user_input",
    sourceInputId: null,
    sourceSessionId: null,
    startedAt: NOW_SECONDS - 75,
    ...overrides,
  };
}

function strip(model: WorkStatusModel): HTMLElement {
  return renderWorkStatus(model, "en", 0, () => NOW_SECONDS * 1000, vi.fn()).element;
}

function model(overrides: Partial<WorkStatusModel> = {}): WorkStatusModel {
  return {
    busy: true,
    coreStatus: null,
    task: null,
    canCancel: false,
    reducedMotion: true,
    turnSource: "user_input",
    turnStartedAt: NOW_SECONDS - 75,
    queuedCount: 0,
    lastFailure: null,
    ...overrides,
  };
}

describe("the live work strip reads Core's turn facts", () => {
  test("elapsed counts from Core's started_at, not from this webview's clock", () => {
    // The client-observed start is deliberately absurd: if it were used the
    // clock would read 0:00.
    const element = renderWorkStatus(
      model(),
      "en",
      NOW_SECONDS * 1000,
      () => NOW_SECONDS * 1000,
      vi.fn(),
    ).element;
    expect(element.querySelector("[data-work-elapsed]")?.textContent).toBe("1:15");
    expect(element.querySelector<HTMLElement>("[data-work-elapsed]")?.dataset.workElapsedSource)
      .toBe("core");
  });

  test("a turn with no Core start falls back to the observed clock and says so", () => {
    const element = renderWorkStatus(
      model({ turnStartedAt: null, turnSource: null }),
      "en",
      (NOW_SECONDS - 10) * 1000,
      () => NOW_SECONDS * 1000,
      vi.fn(),
    ).element;
    expect(element.querySelector("[data-work-elapsed]")?.textContent).toBe("0:10");
    expect(element.querySelector<HTMLElement>("[data-work-elapsed]")?.dataset.workElapsedSource)
      .toBe("client");
  });

  test("each source is named in its own words", () => {
    expect(strip(model({ turnSource: "user_input" })).querySelector("[data-work-source]")
      ?.textContent).toBe("typed");
    expect(strip(model({ turnSource: "queued_input" })).querySelector("[data-work-source]")
      ?.textContent).toBe("queued");
    expect(strip(model({ turnSource: "agent_session" })).querySelector("[data-work-source]")
      ?.textContent).toBe("agent");
    // An unmodelled source keeps the turn visible without naming it.
    expect(strip(model({ turnSource: "unknown" })).querySelector("[data-work-source]")
      ?.textContent).toBe("source not named");
  });

  test("the queue names what it is waiting for, and the two promises differ", () => {
    const running = strip(model({ queuedCount: 2 }));
    expect(running.querySelector("[data-work-queue]")?.textContent).toBe(
      "2 queued · runs after the current turn",
    );

    // Core keeps the queue behind a turn that did not complete, so the promise
    // is a different one.
    const stalled = strip(
      model({
        busy: false,
        queuedCount: 2,
        turnSource: null,
        turnStartedAt: null,
        lastFailure: { outcome: "failed", reason: "the provider refused" },
      }),
    );
    expect(stalled.querySelector("[data-work-queue]")?.textContent).toBe(
      "2 queued · waits for the next completed turn",
    );
  });

  test("an empty queue draws no queue line", () => {
    expect(strip(model()).querySelector("[data-work-queue]")).toBeNull();
  });

  test("formatElapsed is unchanged", () => {
    expect(formatElapsed(75_000)).toBe("1:15");
  });
});

describe("the model reads the projection", () => {
  test("it picks the turn Core scoped to the selected Lane", () => {
    const projection = {
      ...D1_PROJECTION,
      activeTurns: [
        laneTurn({ turnId: "turn-other", laneId: "lane-other" }),
        laneTurn({ source: "queued_input", sourceInputId: "input-9" }),
      ],
      turnFailure: null,
    };
    const derived = workStatusModel(projection, "lane-core", false);
    expect(derived.turnSource).toBe("queued_input");
    expect(derived.turnStartedAt).toBe(NOW_SECONDS - 75);
  });

  test("with no Lane selected it reads the session-scoped turn", () => {
    const projection = {
      ...D1_PROJECTION,
      activeTurns: [laneTurn({ laneId: null, source: "queued_input" })],
      turnFailure: null,
    };
    const derived = workStatusModel(projection, null, false);
    expect(derived.turnSource).toBe("queued_input");
  });

  test("a Lane with no turn of its own carries no source and no start", () => {
    const projection = {
      ...D1_PROJECTION,
      activeTurns: [laneTurn({ laneId: "lane-other" })],
      turnFailure: null,
    };
    const derived = workStatusModel(projection, "lane-core", false);
    expect(derived.turnSource).toBeNull();
    expect(derived.turnStartedAt).toBeNull();
  });
});

describe("a failed turn is a transcript row", () => {
  function cockpit(turnFailure: unknown): HTMLElement {
    document.body.innerHTML = '<div id="host"></div>';
    const root = document.querySelector<HTMLElement>("#host")!;
    const projection = { ...D1_PROJECTION, activeTurns: [], turnFailure };
    const result: D1IntentResult = {
      projection: projection as typeof D1_PROJECTION,
      pendingCommandId: null,
      outcome: { state: "confirmed" as const, reason: null },
    };
    renderD1Cockpit(
      root,
      projection as typeof D1_PROJECTION,
      vi.fn(async (_intent: D1Intent) => result),
      vi.fn(async () => result),
      undefined,
      undefined,
      { poll: false },
    );
    return root;
  }

  test("Core's reason is rendered verbatim in an alert row", () => {
    const row = cockpit({
      turnId: "turn-2",
      laneId: "lane-core",
      outcome: "failed",
      reason: "the provider refused the request",
    }).querySelector<HTMLElement>("[data-turn-failed]");
    expect(row?.getAttribute("role")).toBe("alert");
    expect(row?.textContent).toContain("the provider refused the request");
    expect(row?.dataset.turnFailed).toBe("failed");
  });

  test("a cancellation is its own sentence and never an error", () => {
    const row = cockpit({
      turnId: "turn-3",
      laneId: "lane-core",
      outcome: "cancelled",
      reason: null,
    }).querySelector<HTMLElement>("[data-turn-failed]");
    expect(row?.dataset.turnFailed).toBe("cancelled");
    expect(row?.getAttribute("role")).toBe("status");
    expect(row?.textContent).toContain("stopped");
  });

  test("another Lane's failure is not this Lane's row", () => {
    expect(
      cockpit({
        turnId: "turn-4",
        laneId: "lane-elsewhere",
        outcome: "failed",
        reason: "not this Lane",
      }).querySelector("[data-turn-failed]"),
    ).toBeNull();
  });

  test("no failure draws no row", () => {
    expect(cockpit(null).querySelector("[data-turn-failed]")).toBeNull();
  });
});
