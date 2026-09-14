// @vitest-environment jsdom

import { describe, expect, test, vi } from "vitest";

import {
  renderD6Recovery,
  type D6Intent,
  type D6IntentResult,
  type D6RecoveryProjection,
} from "../src/screens/d6_recovery";

const BASE: D6RecoveryProjection = {
  connection: "live",
  state: "empty",
  detail: null,
  hint: null,
  recoverable: false,
  businessSuccessBlocked: false,
  usedTokens: null,
  hardTokenLimit: null,
  missingCapabilities: [],
  reconnect: null,
  actions: [
    { kind: "reconnect", available: false, code: "GUI-CORE-003" },
    { kind: "inspect", available: true, code: "presentation_only" },
    { kind: "restart", available: false, code: "GUI-CORE-003" },
    { kind: "close_lane", available: false, code: "GUI-CORE-003" },
    { kind: "checkpoint", available: false, code: "GUI-CORE-003" },
  ],
};

function setup(projection: D6RecoveryProjection, locale: "en" | "zh-CN" = "en") {
  document.body.innerHTML = '<main id="stage"></main>';
  const root = document.querySelector<HTMLElement>("#stage")!;
  const reconnect = vi.fn(async () => projection);
  const controller = renderD6Recovery(root, projection, reconnect, locale);
  return { root, reconnect, controller };
}

describe("D6 subordinate recovery surface", () => {
  test.each([
    "empty",
    "connecting",
    "disconnected",
    "provider_error",
    "agent_stopped",
    "context_overflow",
    "gate_queue_clear",
    "incompatible_schema",
    "missing_feature_capability",
    "event_gap",
  ] as const)("renders %s with a textual non-color state cue", (state) => {
    const { root } = setup({
      ...BASE,
      state,
      connection: state === "connecting" ? "connecting" : state === "event_gap" ? "recovering" : "live",
      detail: `${state} detail`,
    });
    const surface = root.querySelector<HTMLElement>("[data-d6-state]")!;
    expect(surface.dataset.d6State).toBe(state);
    expect(surface.querySelector("[data-d6-tag]")?.textContent?.length).toBeGreaterThan(0);
    expect(surface.textContent).toContain(`${state} detail`);
    expect(surface.getAttribute("aria-live")).toBe("polite");
  });

  test("renders exact context overflow facts and typed unavailable recovery actions", () => {
    const { root } = setup({
      ...BASE,
      state: "context_overflow",
      businessSuccessBlocked: true,
      usedTokens: 10_001,
      hardTokenLimit: 10_000,
    });
    expect(root.textContent).toContain("10,001");
    expect(root.textContent).toContain("10,000");
    for (const kind of ["restart", "close_lane", "checkpoint"]) {
      const button = root.querySelector<HTMLButtonElement>(`[data-d6-action="${kind}"]`)!;
      expect(button.disabled).toBe(true);
      expect(button.textContent).toContain("GUI-CORE-003");
    }
  });

  test("keeps D6 inside a cockpit work surface and excludes design-page scaffolding", () => {
    const { root } = setup(BASE);
    expect(root.querySelector(".d6-stage")).not.toBeNull();
    expect(root.querySelector(".statebar")).toBeNull();
    expect(root.textContent).not.toContain("D6 concept · draft 01");
  });
});

const STOPPED: D6RecoveryProjection = {
  ...BASE,
  state: "agent_stopped",
  detail: "ACP session failed",
  businessSuccessBlocked: true,
  actions: [
    { kind: "reconnect", available: false, code: "GUI-CORE-003" },
    { kind: "inspect", available: true, code: "presentation_only" },
    {
      kind: "restart",
      available: true,
      code: "core_command",
      sessionId: "session-1",
      laneId: "lane-1",
    },
    { kind: "close_lane", available: true, code: "core_command", laneId: "lane-1" },
    { kind: "checkpoint", available: false, code: "GUI-CORE-003" },
  ],
};

function setupWithIntent(projection: D6RecoveryProjection, next?: D6RecoveryProjection) {
  document.body.innerHTML = '<main id="stage"></main>';
  const root = document.querySelector<HTMLElement>("#stage")!;
  const sent: D6Intent[] = [];
  const sendIntent = vi.fn(async (intent: D6Intent): Promise<D6IntentResult> => {
    sent.push(intent);
    return { projection: next ?? projection, pendingCommandId: "gui-d6-1" };
  });
  const controller = renderD6Recovery(
    root,
    projection,
    async () => projection,
    "en",
    undefined,
    sendIntent,
  );
  return { root, sent, sendIntent, controller };
}

describe("D6 recovery actions reach the Core commands that own them", () => {
  test("restart sends the exact session id Core published", async () => {
    const live: D6RecoveryProjection = { ...STOPPED, state: "live", connection: "live" };
    const { root, sent, sendIntent } = setupWithIntent(STOPPED, live);
    const restart = root.querySelector<HTMLButtonElement>('[data-d6-action="restart"]')!;

    expect(restart.disabled).toBe(false);
    restart.click();
    expect(root.querySelector("[data-d6-state]")?.getAttribute("aria-busy")).toBe("true");

    await vi.waitFor(() => expect(sendIntent).toHaveBeenCalledOnce());
    expect(sent).toEqual([{ kind: "restart", sessionId: "session-1" }]);
    // The surface re-renders from the projection the host returned, never from
    // an assumed local success.
    await vi.waitFor(() => {
      expect(root.querySelector("[data-d6-state]")?.getAttribute("data-d6-state")).toBe("live");
      expect(root.querySelector("[data-d6-state]")?.getAttribute("aria-busy")).toBe("false");
    });
  });

  test("close lane sends the exact Lane id Core published", async () => {
    const { root, sent, sendIntent } = setupWithIntent(STOPPED);
    root.querySelector<HTMLButtonElement>('[data-d6-action="close_lane"]')!.click();

    await vi.waitFor(() => expect(sendIntent).toHaveBeenCalledOnce());
    expect(sent).toEqual([{ kind: "close_lane", laneId: "lane-1" }]);
  });

  test("an available action without a Core target stays inert", () => {
    const { root, sendIntent } = setupWithIntent({
      ...STOPPED,
      actions: STOPPED.actions.map((action) =>
        action.kind === "restart" ? { ...action, sessionId: undefined } : action,
      ),
    });
    const restart = root.querySelector<HTMLButtonElement>('[data-d6-action="restart"]')!;
    expect(restart.disabled).toBe(true);
    restart.click();
    expect(sendIntent).not.toHaveBeenCalled();
  });

  test("the unavailable checkpoint action is disabled and never dispatches", () => {
    const { root, sendIntent } = setupWithIntent(STOPPED);
    const checkpoint = root.querySelector<HTMLButtonElement>('[data-d6-action="checkpoint"]')!;
    expect(checkpoint.disabled).toBe(true);
    checkpoint.click();
    expect(sendIntent).not.toHaveBeenCalled();
  });

  test("without a host intent callback the Core-backed actions stay disabled", () => {
    const { root } = setup(STOPPED);
    for (const kind of ["restart", "close_lane"]) {
      expect(
        root.querySelector<HTMLButtonElement>(`[data-d6-action="${kind}"]`)!.disabled,
      ).toBe(true);
    }
  });

  test("inspect toggles a local details region and calls no Core command", () => {
    const { root, sendIntent } = setupWithIntent(STOPPED);
    const inspect = root.querySelector<HTMLButtonElement>('[data-d6-action="inspect"]')!;
    expect(root.querySelector("[data-d6-inspect]")).toBeNull();
    expect(inspect.getAttribute("aria-expanded")).toBe("false");

    inspect.click();
    const details = root.querySelector<HTMLElement>("[data-d6-inspect]")!;
    expect(details).not.toBeNull();
    // The diagnostic code block is always renderable, even with no extra facts.
    expect(details.textContent).toContain("ACP session failed");
    expect(details.textContent).toContain("GUI-CORE-003");
    expect(
      root.querySelector('[data-d6-action="inspect"]')?.getAttribute("aria-expanded"),
    ).toBe("true");

    root.querySelector<HTMLButtonElement>('[data-d6-action="inspect"]')!.click();
    expect(root.querySelector("[data-d6-inspect]")).toBeNull();
    expect(sendIntent).not.toHaveBeenCalled();
  });

  test("a rejected recovery action surfaces an alert and re-enables the surface", async () => {
    document.body.innerHTML = '<main id="stage"></main>';
    const root = document.querySelector<HTMLElement>("#stage")!;
    const sendIntent = vi.fn(async (): Promise<D6IntentResult> => {
      throw new Error("target session no longer exists");
    });
    renderD6Recovery(root, STOPPED, async () => STOPPED, "en", undefined, sendIntent);

    root.querySelector<HTMLButtonElement>('[data-d6-action="restart"]')!.click();

    await vi.waitFor(() => expect(root.querySelector("[data-d6-error]")).not.toBeNull());
    const failure = root.querySelector<HTMLElement>("[data-d6-error]")!;
    expect(failure.getAttribute("role")).toBe("alert");
    expect(failure.textContent).toContain("target session no longer exists");
    expect(
      root.querySelector<HTMLButtonElement>('[data-d6-action="restart"]')!.disabled,
    ).toBe(false);
    expect(root.querySelector("[data-d6-state]")?.getAttribute("aria-busy")).toBe("false");

    // A later successful dispatch clears the stale failure message.
    sendIntent.mockImplementationOnce(async () => ({
      projection: STOPPED,
      pendingCommandId: "gui-d6-2",
    }));
    root.querySelector<HTMLButtonElement>('[data-d6-action="restart"]')!.click();
    await vi.waitFor(() => expect(sendIntent).toHaveBeenCalledTimes(2));
    expect(root.querySelector("[data-d6-error]")).toBeNull();
  });
});
