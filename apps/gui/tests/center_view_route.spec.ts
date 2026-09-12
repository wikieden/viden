// @vitest-environment jsdom

/**
 * `?screen=` deep links for the in-cockpit views.
 *
 * The `0.3.4` plan keeps the deep links as entry points and moves what they
 * open: D2, D10, D12, D13, and D14 now render in the cockpit's centre pane
 * with the chrome retained, so a link opens the cockpit *on* that view rather
 * than a bare full-window screen with no way back. These specs drive
 * `hydrateShellFromCore` through an injected CoreClient, so they prove the
 * route, not a host detail.
 */

import { beforeEach, describe, expect, test, vi } from "vitest";

import { hydrateShellFromCore } from "../src/main";
import type { D2DecisionsProjection } from "../src/screens/d2_decisions";
import type { D13FleetWorkflowProjection } from "../src/screens/d13_fleet_workflow";
import { fakeCoreClient } from "./support/fake_core_client";

const D2_PROJECTION: D2DecisionsProjection = {
  workMode: "build",
  permissionLevel: "ask",
  pendingTotal: 0,
  selectedId: null,
  groups: [],
  detail: null,
};

const D13_PROJECTION: D13FleetWorkflowProjection = {
  workflows: [],
  handoffs: [],
};

function root(): HTMLElement {
  const element = document.querySelector<HTMLElement>("#app");
  if (!element) throw new Error("test root is missing");
  return element;
}

function frame(): HTMLElement {
  const element = root().querySelector<HTMLElement>('[data-screen="d1-cockpit"]');
  if (!element) throw new Error("the cockpit frame is missing");
  return element;
}

describe("centre-view deep links", () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="app"></main>';
    window.history.replaceState({}, "", "/");
  });

  test("?screen=d2 opens the cockpit on the decision view, chrome intact", async () => {
    window.history.replaceState({}, "", "/?screen=d2");
    const d2Decisions = vi.fn(async () => D2_PROJECTION);

    await hydrateShellFromCore(root(), fakeCoreClient({ d2Decisions }));

    expect(d2Decisions).toHaveBeenCalledTimes(1);
    // The shell route stays D1: the centre view is a fact about the cockpit.
    expect(root().dataset.route).toBe("d1");
    expect(frame().dataset.centerView).toBe("d2");
    expect(root().querySelector('[data-secondary-view="d2"]')).not.toBeNull();
    for (const landmark of ["topbar", "activity-rail", "lane-rail", "statusbar"]) {
      expect(root().querySelector(`[data-shell-landmark="${landmark}"]`), landmark).not.toBeNull();
    }
    expect(root().querySelector("[data-composer]")).not.toBeNull();
  });

  test("the Close control returns the deep-linked view to the transcript", async () => {
    window.history.replaceState({}, "", "/?screen=d13");
    const d13FleetWorkflow = vi.fn(async () => D13_PROJECTION);

    await hydrateShellFromCore(root(), fakeCoreClient({ d13FleetWorkflow }));
    expect(frame().dataset.centerView).toBe("d13");

    root().querySelector<HTMLButtonElement>("[data-secondary-close]")!.click();
    await vi.waitFor(() => expect(frame().dataset.centerView).toBe("transcript"));
    expect(root().querySelector(".d1-transcript")).not.toBeNull();
  });

  test("?screen=d11 still replaces the window rather than opening a view", async () => {
    window.history.replaceState({}, "", "/?screen=d11");

    await hydrateShellFromCore(root(), fakeCoreClient());

    expect(root().dataset.route).toBe("d11");
    expect(root().querySelector('[data-screen="d1-cockpit"]')).toBeNull();
  });
});
