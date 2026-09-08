// @vitest-environment jsdom

import { beforeEach, describe, expect, test } from "vitest";

import {
  D1_ACTIVITY_ITEMS,
  D1_RAIL_ROUTES,
  renderActivityRail,
} from "../src/components/activity_rail";
import { translate, type MessageKey } from "../src/i18n/catalog";

/**
 * The activity rail's routing slots name the screen they actually open.
 *
 * The rail previously carried an editor's file-explorer vocabulary — Search,
 * Source control, Evidence, Diagnostics, Inbox — over routes that go somewhere
 * else entirely. A label that names the wrong destination is worse than a
 * missing one: the operator learns the mapping by clicking, and every tooltip
 * and screen-reader announcement teaches it wrong in the meantime.
 */

/** The screen each routing slot opens, by the route id the cockpit dispatches. */
const DESTINATIONS: Record<string, { en: string; zh: string }> = {
  d12: { en: "Integration gate", zh: "集成闸" },
  d2: { en: "Decisions", zh: "决策" },
  d14: { en: "Audit timeline", zh: "审计时间线" },
  d10: { en: "Lane monitor", zh: "Lane 监视墙" },
  d13: { en: "Fleet board", zh: "舰队看板" },
};

function rail(): HTMLElement {
  return renderActivityRail("en", {
    lanesAvailable: true,
    lanesOpen: false,
    onToggleLanes: () => undefined,
    onNavigate: () => undefined,
  });
}

describe("activity rail destinations", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("labels every routing slot with the screen it opens, in both locales", () => {
    for (const [route, names] of Object.entries(DESTINATIONS)) {
      const key = Object.keys(D1_RAIL_ROUTES).find(
        (candidate) => D1_RAIL_ROUTES[candidate] === route,
      );
      expect(key, `no rail slot routes to ${route}`).toBeDefined();
      const messageKey = key as Extract<MessageKey, `d1.activity.${string}`>;
      expect(translate("en", messageKey, {})).toBe(names.en);
      expect(translate("zh-CN", messageKey, {})).toBe(names.zh);
    }
  });

  test("carries the destination name on the tooltip and the accessible name", () => {
    const element = rail();

    for (const [route, names] of Object.entries(DESTINATIONS)) {
      const slot = element.querySelector<HTMLButtonElement>(`[data-rail-route="${route}"]`)!;
      expect(slot.getAttribute("aria-label")).toBe(names.en);
      expect(slot.title).toBe(names.en);
    }
  });

  test("routes exactly the five restored screens and leaves routing unchanged", () => {
    // The honesty fix renames and re-glyphs; it must not silently re-point a
    // slot at a different screen.
    expect(Object.values(D1_RAIL_ROUTES).sort()).toEqual(["d10", "d12", "d13", "d14", "d2"]);
  });

  test("gives each destination a distinct registered glyph", () => {
    const icons = D1_ACTIVITY_ITEMS.map((item) => item.icon);

    expect(new Set(icons).size).toBe(icons.length);
    // Every glyph is copied from the design package's registered
    // `GUI/gui-icons.jsx` set; no slot invents art of its own.
    expect(icons).toEqual([
      "chat",
      "worktree",
      "lanes",
      "decide",
      "evidence",
      "diagnostics",
      "fleet",
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
