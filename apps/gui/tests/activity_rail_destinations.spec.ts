// @vitest-environment jsdom

import { beforeEach, describe, expect, test } from "vitest";

import {
  D1_RAIL_ROUTES,
  renderActivityRail,
  type D1RailDestination,
} from "../src/components/activity_rail";
import { translate, type MessageKey } from "../src/i18n/catalog";

/**
 * The activity rail's slots name the destination they actually open.
 *
 * The rail once carried an editor's file-explorer vocabulary — Search, Source
 * control, Evidence, Diagnostics, Inbox — over routes that go somewhere else
 * entirely. A label that names the wrong destination is worse than a missing
 * one: the operator learns the mapping by clicking, and every tooltip and
 * screen-reader announcement teaches it wrong in the meantime. G3 keeps that
 * property while the rail becomes a router over in-cockpit views.
 */

/** The destination each slot opens, by the centre view id it switches to. */
const DESTINATIONS: Record<D1RailDestination, { en: string; zh: string }> = {
  review: { en: "Diff review", zh: "Diff 评审" },
  evidence: { en: "Evidence", zh: "证据" },
  d2: { en: "Decisions", zh: "决策" },
  d10: { en: "Lane monitor", zh: "Lane 监视墙" },
  d12: { en: "Integration gate", zh: "集成闸" },
  d13: { en: "Fleet board", zh: "舰队看板" },
  d14: { en: "Audit timeline", zh: "审计时间线" },
};

function rail(): HTMLElement {
  return renderActivityRail("en", {
    lanesAvailable: true,
    lanesOpen: false,
    onToggleLanes: () => undefined,
    onOpenDestination: () => undefined,
    destinations: Object.fromEntries(
      (Object.keys(DESTINATIONS) as D1RailDestination[]).map((destination) => [
        destination,
        { available: true, current: false },
      ]),
    ),
  });
}

describe("activity rail destinations", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("labels every destination with the screen it opens, in both locales", () => {
    for (const [destination, names] of Object.entries(DESTINATIONS)) {
      const slot = D1_RAIL_ROUTES.find((candidate) => candidate.destination === destination);
      expect(slot, `no rail slot routes to ${destination}`).toBeDefined();
      const messageKey = slot!.key as Extract<MessageKey, `d1.activity.${string}`>;
      expect(translate("en", messageKey, {})).toBe(names.en);
      expect(translate("zh-CN", messageKey, {})).toBe(names.zh);
    }
  });

  test("carries the destination name on the tooltip and the accessible name", () => {
    const element = rail();

    for (const [destination, names] of Object.entries(DESTINATIONS)) {
      const slot = element.querySelector<HTMLButtonElement>(
        `[data-rail-route="${destination}"]`,
      )!;
      // D14 appends the raw-replay fallback, which is a fact about the
      // destination rather than a second name for it.
      expect(slot.getAttribute("aria-label")).toContain(names.en);
      expect(slot.title).toContain(names.en);
    }
  });

  test("routes every registered destination and nothing else", () => {
    const routed = Array.from(
      rail().querySelectorAll<HTMLElement>("[data-rail-route]"),
      (slot) => slot.dataset.railRoute,
    );
    expect(routed.sort()).toEqual(["d10", "d12", "d13", "d14", "d2", "evidence", "review"]);
  });

  test("gives each slot a distinct registered glyph", () => {
    const icons = D1_RAIL_ROUTES.map((slot) => slot.icon);

    expect(new Set(icons).size).toBe(icons.length);
    // Every glyph is copied from the design package's registered
    // `GUI/gui-icons.jsx` set; no slot invents art of its own.
    expect(icons).toEqual([
      "chat",
      "lanes",
      "review",
      "evidence",
      "decide",
      "diagnostics",
      "worktree",
      "fleet",
      "brief",
    ]);
  });

  test("draws the fleet glyph rather than falling through to the panel default", () => {
    const element = rail();
    const fleet = element.querySelector<HTMLButtonElement>('[data-rail-route="d13"]')!;
    const svg = fleet.querySelector("svg")!;

    // The fallback branch draws a rect; `fleet` is four nodes and their edges.
    expect(svg.querySelector("rect")).toBeNull();
    expect(svg.querySelectorAll("circle")).toHaveLength(4);
  });
});
