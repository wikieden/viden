// @vitest-environment jsdom

/**
 * D14's client-side actor and time filters, and the loaded-page rollup (`G6`).
 *
 * `AuditQuery` carries `project_id`, `lane_id`, `object`, and a `before`
 * cursor — no actor filter and no time range (GUI-CORE-024). The design shows
 * both, so the honest build is a filter over *the page the client is holding*,
 * said out loud: every chip, the rollup, and the empty state name the loaded
 * page rather than the audit store. The tests below pin exactly that
 * distinction, because a filter that reads as a total is the failure mode.
 */

import { describe, expect, test, vi } from "vitest";

import {
  renderD14,
  type D14AuditProjection,
  type D14AuditRow,
  type D14AuditTimelineProjection,
} from "../src/screens/d14_audit_timeline";

/** The frozen "now" every timestamp below is measured back from. */
const NOW_MS = Date.parse("2026-09-12T12:00:00.000Z");
const NOW = Math.floor(NOW_MS / 1000);
const HOUR = 3600;
const DAY = 24 * HOUR;

function row(overrides: Partial<D14AuditRow> = {}): D14AuditRow {
  return {
    auditId: "audit-1",
    timestamp: NOW - HOUR,
    projectId: "project-viden",
    laneId: "lane-core",
    actorKind: "operator",
    agentId: null,
    action: "gate.decided",
    objects: [],
    outcome: "success",
    args: [],
    ...overrides,
  };
}

const ROWS: D14AuditRow[] = [
  row({ auditId: "a-operator-now", actorKind: "operator", outcome: "success" }),
  row({
    auditId: "a-agent-yesterday",
    actorKind: "agent",
    agentId: "codex",
    outcome: "denied",
    timestamp: NOW - 30 * HOUR,
  }),
  row({
    auditId: "a-agent-lastweek",
    actorKind: "agent",
    agentId: "codex",
    outcome: "failed",
    timestamp: NOW - 5 * DAY,
  }),
  row({
    auditId: "a-system-old",
    actorKind: "system",
    outcome: "success",
    timestamp: NOW - 30 * DAY,
  }),
];

const RAW: D14AuditTimelineProjection = { rows: [], nextCursor: null, complete: true };

function audit(overrides: Partial<D14AuditProjection> = {}): D14AuditProjection {
  return {
    outcome: { state: "confirmed", reason: null },
    rows: ROWS,
    nextBefore: null,
    complete: true,
    loaded: true,
    pendingCommandId: null,
    capabilityAvailable: true,
    scope: null,
    ...overrides,
  };
}

function setup(projection: D14AuditProjection = audit()) {
  vi.spyOn(Date, "now").mockReturnValue(NOW_MS);
  document.body.innerHTML = '<div id="host"></div>';
  const root = document.querySelector<HTMLElement>("#host")!;
  renderD14(root, projection, "en", {
    queryAudit: async () => projection,
    loadOlderAudit: async () => projection,
    loadRaw: async () => RAW,
  });
  return root;
}

function shownIds(root: HTMLElement): string[] {
  return Array.from(
    root.querySelectorAll<HTMLElement>("[data-d14-audit-id]"),
    (item) => item.dataset.d14AuditId ?? "",
  );
}

describe("D14 audit filters and rollup", () => {
  test("offers only the actor values the loaded page actually carries", () => {
    const root = setup();
    const actors = Array.from(
      root.querySelectorAll<HTMLElement>("[data-d14-actor-filter]"),
      (chip) => chip.dataset.d14ActorFilter,
    );

    // `unknown` is a legal Core actor kind and is not on this page, so it is
    // not offered: a chip that can only ever filter to nothing is a lie about
    // what the page holds.
    expect(actors).toEqual(["all", "operator", "agent", "system"]);
  });

  test("an actor chip narrows the rendered rows and nothing else", () => {
    const root = setup();
    expect(shownIds(root)).toHaveLength(4);

    root.querySelector<HTMLButtonElement>("[data-d14-actor-filter='agent']")!.click();

    expect(shownIds(root)).toEqual(["a-agent-yesterday", "a-agent-lastweek"]);
    expect(
      root.querySelector<HTMLElement>("[data-d14-actor-filter='agent']")!.getAttribute(
        "aria-pressed",
      ),
    ).toBe("true");
  });

  test("the time chips cut on the rows' own Core timestamps", () => {
    const root = setup();

    root.querySelector<HTMLButtonElement>("[data-d14-time-filter='today']")!.click();
    expect(shownIds(root)).toEqual(["a-operator-now"]);

    root.querySelector<HTMLButtonElement>("[data-d14-time-filter='24h']")!.click();
    expect(shownIds(root)).toEqual(["a-operator-now"]);

    root.querySelector<HTMLButtonElement>("[data-d14-time-filter='7d']")!.click();
    expect(shownIds(root)).toEqual([
      "a-operator-now",
      "a-agent-yesterday",
      "a-agent-lastweek",
    ]);

    root.querySelector<HTMLButtonElement>("[data-d14-time-filter='all']")!.click();
    expect(shownIds(root)).toHaveLength(4);
  });

  test("the rollup counts outcomes for the loaded page and says so", () => {
    const root = setup();
    const rollup = root.querySelector<HTMLElement>("[data-d14-rollup]")!;

    expect(rollup.textContent).toContain("loaded page");
    // Never "total", never "all records": the store is bigger than the page.
    expect(rollup.textContent).not.toContain("total");
    expect(
      rollup.querySelector<HTMLElement>("[data-d14-rollup-outcome='success']")!.dataset
        .d14RollupCount,
    ).toBe("2");
    expect(
      rollup.querySelector<HTMLElement>("[data-d14-rollup-outcome='denied']")!.dataset
        .d14RollupCount,
    ).toBe("1");
    expect(
      rollup.querySelector<HTMLElement>("[data-d14-rollup-outcome='failed']")!.dataset
        .d14RollupCount,
    ).toBe("1");
  });

  test("the rollup follows the filter and marks itself filtered", () => {
    const root = setup();

    root.querySelector<HTMLButtonElement>("[data-d14-actor-filter='agent']")!.click();
    const rollup = root.querySelector<HTMLElement>("[data-d14-rollup]")!;

    expect(rollup.dataset.d14Rollup).toBe("filtered");
    expect(rollup.textContent).toContain("2 of 4");
    expect(rollup.querySelector("[data-d14-rollup-outcome='success']")).toBeNull();
  });

  test("filtering to empty says no rows match, not that Core recorded none", () => {
    const root = setup();

    root.querySelector<HTMLButtonElement>("[data-d14-actor-filter='system']")!.click();
    root.querySelector<HTMLButtonElement>("[data-d14-time-filter='today']")!.click();

    expect(shownIds(root)).toHaveLength(0);
    expect(root.querySelector("[data-d14-no-match]")?.textContent).toContain(
      "No row on the loaded page matches",
    );
    // The two are different facts and must never share a sentence.
    expect(root.querySelector("[data-d14-audit-empty]")).toBeNull();
  });

  test("an empty Core page keeps its own answer rather than the filter's", () => {
    const root = setup(audit({ rows: [] }));

    expect(root.querySelector("[data-d14-audit-empty]")).not.toBeNull();
    expect(root.querySelector("[data-d14-no-match]")).toBeNull();
    // With no rows there is nothing to offer beyond the neutral chip.
    expect(root.querySelectorAll("[data-d14-actor-filter]")).toHaveLength(1);
  });

  test("export stays disabled and names the contract gap", () => {
    const root = setup();
    const exportButton = root.querySelector<HTMLButtonElement>("[data-d14-export]")!;

    expect(exportButton.disabled).toBe(true);
    expect(exportButton.getAttribute("aria-label")).toContain("GUI-CORE-024");
  });

  test("a fresh render starts unfiltered, so nothing is persisted across entries", () => {
    const first = setup();
    first.querySelector<HTMLButtonElement>("[data-d14-actor-filter='agent']")!.click();
    expect(shownIds(first)).toHaveLength(2);

    const second = setup();
    expect(shownIds(second)).toHaveLength(4);
    expect(
      second
        .querySelector<HTMLElement>("[data-d14-actor-filter='all']")!
        .getAttribute("aria-pressed"),
    ).toBe("true");
  });
});
