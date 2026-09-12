// @vitest-environment jsdom

/**
 * The activity rail's decision-queue badge (`G6`).
 *
 * The badge is the one count on the rail, and it is the same Core number the
 * statusbar's `⏸` segment prints — `D1StatusbarProjection.pendingGateCount`,
 * which is also what the D2 view the slot opens is a queue of. Three states,
 * three different renderings, and only one of them is a number:
 *
 * - a Core count above zero is the badge;
 * - a Core count of zero is no badge, because there is nothing waiting;
 * - **no** Core count is also no badge, and the slot says why — a placeholder
 *   zero here would read as "nothing waiting" when the truth is "nobody
 *   counted".
 */

import { describe, expect, test, vi } from "vitest";

import { renderActivityRail } from "../src/components/activity_rail";
import { renderD1Cockpit, type D1Intent, type D1IntentResult } from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

function rail(badge: number | null): HTMLElement {
  document.body.innerHTML = "";
  const element = renderActivityRail("en", {
    lanesAvailable: true,
    lanesOpen: false,
    onToggleLanes: () => undefined,
    onOpenDestination: () => undefined,
    destinations: { d2: { available: true, current: false, badge } },
  });
  document.body.append(element);
  return element;
}

function cockpit(pendingGateCount: number | null) {
  document.body.innerHTML = '<div id="host"></div>';
  const root = document.querySelector<HTMLElement>("#host")!;
  const projection = {
    ...D1_PROJECTION,
    statusbar: { ...D1_PROJECTION.statusbar, pendingGateCount },
  };
  const result: D1IntentResult = {
    projection,
    pendingCommandId: null,
    outcome: { state: "confirmed" as const, reason: null },
  };
  renderD1Cockpit(
    root,
    projection,
    vi.fn(async (_intent: D1Intent) => result),
    vi.fn(async () => result),
    undefined,
    undefined,
    { poll: false, onNavigate: () => undefined },
  );
  return root;
}

describe("decision queue badge", () => {
  test("renders the Core count on the D2 slot", () => {
    const slot = rail(3).querySelector<HTMLElement>('[data-rail-route="d2"]')!;
    expect(slot.querySelector("[data-rail-badge]")?.textContent).toBe("3");
  });

  test("a Core count of zero carries no badge", () => {
    const slot = rail(0).querySelector<HTMLElement>('[data-rail-route="d2"]')!;
    expect(slot.querySelector("[data-rail-badge]")).toBeNull();
  });

  test("an absent Core count carries no badge either", () => {
    const slot = rail(null).querySelector<HTMLElement>('[data-rail-route="d2"]')!;
    expect(slot.querySelector("[data-rail-badge]")).toBeNull();
  });

  test("the cockpit feeds the badge from the projection the statusbar segment reads", () => {
    const root = cockpit(4);
    const slot = root.querySelector<HTMLElement>('[data-rail-route="d2"]')!;

    expect(slot.querySelector("[data-rail-badge]")?.textContent).toBe("4");
    // Same source, same number, in the one other place it is printed.
    expect(root.querySelector("[data-sb-gate]")?.textContent).toContain("4");
  });

  test("with no published count the slot says so instead of showing a zero", () => {
    const root = cockpit(null);
    const slot = root.querySelector<HTMLElement>('[data-rail-route="d2"]')!;

    expect(slot.querySelector("[data-rail-badge]")).toBeNull();
    expect(slot.getAttribute("aria-label")).toContain("Core published no decision count");
    expect(slot.title).toContain("Core published no decision count");
    // The statusbar's segment is absent for the same reason.
    expect(root.querySelector("[data-sb-gate]")).toBeNull();
  });
});
