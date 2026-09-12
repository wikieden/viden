// @vitest-environment jsdom

import { describe, expect, test, vi } from "vitest";

import {
  renderD10LaneMonitor,
  type D10Lane,
  type D10LaneMonitorProjection,
} from "../src/screens/d10_lane_monitor";

function lane(overrides: Partial<D10Lane> = {}): D10Lane {
  return {
    id: "lane-1",
    projectId: "project-boss-rush",
    summary: "gameplay · jump feel",
    role: "coder",
    route: "acp",
    gateStrength: "cooperative",
    mutationPolicy: "propose_only",
    status: "running",
    awaitsHuman: false,
    branch: "codex/lane-1",
    worktree: ".worktrees/lane-1",
    progress: 64,
    agents: [
      { sessionId: "session-1", agentId: "codex", model: "gpt-5-codex", status: "running" },
    ],
    evidence: [{ id: "evidence-1", kind: "check", summary: "replay regression 12/12" }],
    tokenLimit: null,
    costLimitMicroUsd: null,
    costMeterability: "metered",
    runStats: null,
    ...overrides,
  };
}

const PROJECTION: D10LaneMonitorProjection = {
  totalLanes: 3,
  totalProjects: 2,
  awaitingTotal: 1,
  lanes: [
    lane(),
    lane({
      id: "lane-2",
      projectId: "project-viden-core",
      summary: "provider retry policy",
      gateStrength: "full",
      status: "waiting_approval",
      awaitsHuman: true,
      progress: null,
    }),
    lane({
      id: "lane-3",
      projectId: null,
      summary: "unbound lane",
      gateStrength: "containment",
      status: "queued",
      progress: 0,
      agents: [],
      evidence: [],
    }),
  ],
  unavailable: [],
};

function setup(projection: D10LaneMonitorProjection = PROJECTION) {
  document.body.innerHTML = '<div id="host"></div>';
  const root = document.querySelector<HTMLElement>("#host")!;
  const openDecisionCenter = vi.fn();
  const controller = renderD10LaneMonitor(root, projection, "en", openDecisionCenter);
  return { root, openDecisionCenter, controller };
}

describe("D10 lane monitor", () => {
  test("renders one card per Core lane with its own gate strength", () => {
    const { root } = setup();
    expect(root.querySelectorAll("[data-d10-lane]")).toHaveLength(3);
    expect(
      root.querySelector<HTMLElement>("[data-d10-lane='lane-2'] [data-d10-gate]")?.dataset
        .d10Gate,
    ).toBe("full");
    expect(
      root.querySelector<HTMLElement>("[data-d10-lane='lane-3'] [data-d10-gate]")?.dataset
        .d10Gate,
    ).toBe("containment");
  });

  test("escalates only the lane Core reports as blocking on a human", () => {
    const { root } = setup();
    const attention = root.querySelectorAll("[data-d10-attention='true']");
    expect(attention).toHaveLength(1);
    expect((attention[0] as HTMLElement).dataset.d10Lane).toBe("lane-2");
    expect(root.querySelector("[data-d10-counts]")?.textContent).toContain("1 awaiting you");
  });

  test("a lane with no Core task states the fact instead of showing zero progress", () => {
    const { root } = setup();
    const missing = root.querySelector<HTMLElement>(
      "[data-d10-lane='lane-2'] [data-d10-progress]",
    );
    const zero = root.querySelector<HTMLElement>(
      "[data-d10-lane='lane-3'] [data-d10-progress]",
    );
    expect(missing?.dataset.d10Progress).toBe("none");
    expect(missing?.textContent).toContain("no Core task");
    expect(zero?.dataset.d10Progress).toBe("0");
  });

  test("an unbound lane shows no project rather than a guessed one", () => {
    const { root } = setup();
    const project = root.querySelector<HTMLElement>(
      "[data-d10-lane='lane-3'] [data-d10-project]",
    );
    expect(project?.dataset.d10Project).toBe("");
    expect(project?.textContent).toBe("no project binding");
    // Filters offer only Core-bound projects.
    expect([...root.querySelectorAll("[data-d10-filter]")].map(
      (node) => (node as HTMLElement).dataset.d10Filter,
    )).toEqual(["all", "project-boss-rush", "project-viden-core"]);
  });

  test("the blocked lane routes the operator to the decision center", () => {
    const { root, openDecisionCenter } = setup();
    root.querySelector<HTMLButtonElement>(
      "[data-d10-lane='lane-2'] [data-d10-action='decide']",
    )!.click();
    expect(openDecisionCenter).toHaveBeenCalledOnce();
    // A lane that is not blocked offers no decision shortcut.
    expect(
      root.querySelector("[data-d10-lane='lane-1'] [data-d10-action='decide']"),
    ).toBeNull();
  });

  test("the ticker renders Core audit rows, ordered as Core delivered them", () => {
    const { root, controller } = setup();
    // H2 hygiene (compatibility follow-up 2): nothing has been read yet, which
    // is not the same fact as Core offering no timeline. The strip says so and
    // must never claim Core publishes nothing.
    const notRead = root.querySelector<HTMLElement>("[data-d10-ticker-state]");
    expect(notRead?.dataset.d10TickerState).toBe("not-read");
    expect(notRead?.textContent).not.toContain("runtime.audit");

    controller.applyEvents({
      capabilityAvailable: true,
      loaded: true,
      outcome: { state: "confirmed", reason: null },
      rows: [
        {
          auditId: "audit-delta",
          timestamp: 1_700_000_200,
          projectId: "project-docs",
          laneId: "lane-docs",
          actorKind: "operator",
          agentId: null,
          action: "review.decided",
          objects: [],
          outcome: "success",
          args: [],
        },
        {
          auditId: "audit-alpha",
          timestamp: 1_700_000_100,
          projectId: "project-viden",
          laneId: "lane-core",
          actorKind: "operator",
          agentId: null,
          action: "gate.decided",
          objects: [],
          outcome: "success",
          args: [],
        },
      ],
    });

    const rows = Array.from(root.querySelectorAll<HTMLElement>("[data-d10-event]"));
    expect(rows.map((row) => row.dataset.d10Event)).toEqual(["audit-delta", "audit-alpha"]);
    // The two rows come from two different projects, which is what makes this
    // a workspace ticker rather than a per-project list.
    expect(rows.map((row) => row.dataset.d10EventProject)).toEqual([
      "project-docs",
      "project-viden",
    ]);
    // Core's dotted keys, raw: localizing them would make the timeline
    // undiffable across languages.
    expect(rows[0]!.querySelector(".d10-tkind")?.textContent).toBe("review.decided");
    expect(root.querySelector("[data-d10-unavailable]")).toBeNull();
  });

  test("an unavailable audit capability states the gap instead of an empty strip", () => {
    const { root, controller } = setup();
    controller.applyEvents({
      capabilityAvailable: false,
      loaded: false,
      outcome: { state: "idle", reason: null },
      rows: [],
    });
    const note = root.querySelector<HTMLElement>("[data-d10-ticker-state]");
    expect(note?.dataset.d10TickerState).toBe("unavailable");
    expect(root.querySelectorAll("[data-d10-event]")).toHaveLength(0);
  });

  test("an answered but empty timeline says so, never that it is unavailable", () => {
    const { root, controller } = setup();
    controller.applyEvents({
      capabilityAvailable: true,
      loaded: true,
      outcome: { state: "confirmed", reason: null },
      rows: [],
    });
    expect(
      root.querySelector<HTMLElement>("[data-d10-ticker-state]")?.dataset.d10TickerState,
    ).toBe("empty");
  });

  test("a refused read shows Core's own reason", () => {
    const { root, controller } = setup();
    controller.applyEvents({
      capabilityAvailable: true,
      loaded: false,
      outcome: { state: "rejected", reason: "audit store unavailable" },
      rows: [],
    });
    const note = root.querySelector<HTMLElement>("[data-d10-ticker-state]");
    expect(note?.dataset.d10TickerState).toBe("rejected");
    expect(note?.textContent).toBe("audit store unavailable");
  });
});

describe("D10 cost meterability", () => {
  test("marks a cost-blind lane and renders the four bounded run facts", () => {
    const { root } = setup({
      ...PROJECTION,
      lanes: [
        lane({
          id: "lane-terminal",
          route: "terminal",
          costMeterability: "blind",
          runStats: {
            wallTime: "3m 20s",
            wallTimeMs: 200_400,
            runCount: 3,
            diffBytes: 8_192,
            lastExitCode: 0,
          },
        }),
      ],
    });
    const card = root.querySelector<HTMLElement>("[data-d10-lane='lane-terminal']")!;
    expect(card.querySelector<HTMLElement>("[data-d10-meterability]")?.dataset.d10Meterability).toBe(
      "blind",
    );
    const facts = [...card.querySelectorAll("[data-d10-run-fact]")].map(
      (node) => node.textContent,
    );
    // The humanized duration comes from the host; the screen never formats one.
    expect(facts).toEqual([
      "wall time 3m 20s",
      "runs 3",
      "applied diff 8192 B",
      "last exit 0",
    ]);
  });

  test("labels a missing exit code unknown rather than defaulting it to zero", () => {
    const { root } = setup({
      ...PROJECTION,
      lanes: [
        lane({
          id: "lane-tmux",
          route: "tmux",
          costMeterability: "blind",
          runStats: {
            wallTime: "1.5s",
            wallTimeMs: 1_500,
            runCount: 1,
            diffBytes: 0,
            lastExitCode: null,
          },
        }),
      ],
    });
    const card = root.querySelector<HTMLElement>("[data-d10-lane='lane-tmux']")!;
    expect(card.querySelector("[data-d10-run-fact='last exit']")?.textContent).toBe(
      "last exit unknown",
    );
  });

  test("states that an unobserved blind lane has no run facts instead of showing zeros", () => {
    const { root } = setup({
      ...PROJECTION,
      lanes: [
        lane({ id: "lane-fresh", route: "terminal", costMeterability: "blind", runStats: null }),
      ],
    });
    const card = root.querySelector<HTMLElement>("[data-d10-lane='lane-fresh']")!;
    expect(card.querySelector("[data-d10-run-stats='none']")).not.toBeNull();
    expect(card.querySelectorAll("[data-d10-run-fact]")).toHaveLength(0);
  });

  test("leaves a metered lane's card unchanged", () => {
    const { root } = setup();
    const card = root.querySelector<HTMLElement>("[data-d10-lane='lane-1']")!;
    expect(card.querySelector("[data-d10-meterability]")).toBeNull();
    expect(card.querySelectorAll("[data-d10-run-fact]")).toHaveLength(0);
  });
});

/*
 * H2 hygiene — compatibility follow-up 2.
 *
 * "Not offered" and "not read yet" are two different facts and had one
 * sentence between them, which read as the first for both: a page nobody had
 * loaded reported that Core publishes no audit timeline. The gate for the
 * first is the capability, and nothing else.
 */
describe("D10 event stream: not offered versus not read yet", () => {
  test("no read issued yet says exactly that, and names no capability gap", () => {
    const { root } = setup();
    const note = root.querySelector<HTMLElement>("[data-d10-ticker-state]");
    expect(note?.dataset.d10TickerState).toBe("not-read");
    expect(note?.textContent).toContain("has not been read yet");
    expect(note?.textContent).not.toContain("runtime.audit");
  });

  test("an absent capability is the only state that says Core offers no timeline", () => {
    const { root, controller } = setup();
    controller.applyEvents({
      capabilityAvailable: false,
      loaded: false,
      outcome: { state: "idle", reason: null },
      rows: [],
    });
    const note = root.querySelector<HTMLElement>("[data-d10-ticker-state]");
    expect(note?.dataset.d10TickerState).toBe("unavailable");
    // Gated on the capability and checkable against Core's handshake.
    expect(note?.textContent).toContain("runtime.audit");
  });

  test("the two states never share a sentence, in either language", () => {
    // Rendered directly rather than through `setup`, which is fixed to `en`.
    for (const locale of ["en", "zh-CN"] as const) {
      document.body.innerHTML = '<div id="h1"></div><div id="h2"></div>';
      const first = document.querySelector<HTMLElement>("#h1")!;
      renderD10LaneMonitor(first, PROJECTION, locale);
      const second = document.querySelector<HTMLElement>("#h2")!;
      renderD10LaneMonitor(second, PROJECTION, locale, undefined, {
        capabilityAvailable: false,
        loaded: false,
        outcome: { state: "idle", reason: null },
        rows: [],
      });
      const notRead = first.querySelector<HTMLElement>("[data-d10-ticker-state]");
      const notOffered = second.querySelector<HTMLElement>("[data-d10-ticker-state]");
      expect(notRead?.dataset.d10TickerState).toBe("not-read");
      expect(notOffered?.dataset.d10TickerState).toBe("unavailable");
      expect(notRead?.textContent).toBeTruthy();
      expect(notRead?.textContent).not.toBe(notOffered?.textContent);
    }
  });
});
