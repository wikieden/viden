// @vitest-environment jsdom

/**
 * The webview half of the E3b native defects
 * (`docs/release-evidence/gui-trusted-delivery/checkpoints.md`, defects 22-24).
 *
 * Each test drives the shipped `renderD1Cockpit` over the exact state the
 * native run was in. Nothing is mocked that the run exercised for real: the
 * composer, its send path and the host's pending record are the production
 * modules and the production shapes.
 */

import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import {
  renderD1Cockpit,
  type D1CockpitProjection,
  type D1Intent,
  type D1IntentResult,
} from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

/**
 * The composer's send path waits out the client's bounded command-slot wait
 * (`COMMAND_SLOT_WAIT_MS`) before it reaches `sendAndWait`. The refusal is
 * published when that bound expires, so every test here advances past it
 * rather than pretending the wait is not there.
 */
const settle = async (): Promise<void> => {
  await vi.advanceTimersByTimeAsync(6_000);
  await vi.advanceTimersByTimeAsync(0);
};

function projection(overrides: Partial<D1CockpitProjection> = {}): D1CockpitProjection {
  return { ...structuredClone(D1_PROJECTION), ...overrides };
}

/**
 * The shape the host publishes while one command is unanswered: the id, plus
 * the record a refusal has to name.
 */
function occupied(
  next: D1CockpitProjection,
  kind: string,
  abandonable: boolean,
  ageMs = 42_000,
): D1IntentResult {
  return {
    projection: next,
    pendingCommandId: "gui-d1-create-lane",
    pendingCommand: { commandId: "gui-d1-create-lane", kind, ageMs, abandonable },
    outcome: { state: "pending", reason: null },
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

function mount(
  initial: D1CockpitProjection,
  send: (intent: D1Intent) => Promise<D1IntentResult>,
  options: Record<string, unknown> = {},
  poll: () => Promise<D1IntentResult> = async () => free(initial),
) {
  document.body.innerHTML = '<main id="app"></main>';
  const root = document.querySelector<HTMLElement>("#app")!;
  const controller = renderD1Cockpit(
    root,
    initial,
    send as never,
    vi.fn(poll) as never,
    undefined,
    undefined,
    { poll: false, ...options },
  );
  return { root, controller };
}

function typeAndSend(root: HTMLElement, text: string): void {
  const input = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
  input.value = text;
  input.dispatchEvent(new InputEvent("input", { bubbles: true }));
  root.querySelector<HTMLButtonElement>("[data-composer-send]")!.click();
}

beforeEach(() => {
  document.body.innerHTML = "";
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

/* ---- defect 23: a submit the client cannot send is never silent ---- */

describe("defect 23 — a refused submit names what holds the command slot", () => {
  test("a submit refused by an occupied slot renders a role=alert naming the pending kind and its age", async () => {
    const busy = projection({
      composer: { ...D1_PROJECTION.composer, busy: false, canSubmitImmediately: true },
    });
    const send = vi.fn(async () => occupied(busy, "create_starter_lane", false));
    const { root, controller } = mount(busy, send, {}, async () =>
      occupied(busy, "create_starter_lane", false),
    );

    // The first submit leaves and comes back with the slot still occupied.
    typeAndSend(root, "tool edit_file README.md");
    await settle();
    expect(send).toHaveBeenCalledTimes(1);

    // The second is the one the native run pressed ten times in silence.
    typeAndSend(root, "tool edit_file README.md");
    await settle();
    await settle();

    expect(send).toHaveBeenCalledTimes(1);
    const alert = root.querySelector<HTMLElement>("[data-d1-slot-busy]");
    expect(alert).not.toBeNull();
    expect(alert!.getAttribute("role")).toBe("alert");
    expect(alert!.textContent).toContain("create_starter_lane");
    expect(alert!.textContent).toContain("42s");
    controller.dispose();
  });

  test("the composer keeps the refused text", async () => {
    const busy = projection({
      composer: { ...D1_PROJECTION.composer, busy: false, canSubmitImmediately: true },
    });
    const send = vi.fn(async () => occupied(busy, "create_starter_lane", false));
    const { root, controller } = mount(busy, send, {}, async () =>
      occupied(busy, "create_starter_lane", false),
    );

    typeAndSend(root, "tool edit_file README.md");
    await settle();
    typeAndSend(root, "tool edit_file README.md");
    await settle();
    await settle();

    const input = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
    expect(input.value).toBe("tool edit_file README.md");
    controller.dispose();
  });

  test("a pending mutation is never offered a client-side abandon, and the label says so", async () => {
    const busy = projection({
      composer: { ...D1_PROJECTION.composer, busy: false, canSubmitImmediately: true },
    });
    const send = vi.fn(async () => occupied(busy, "create_starter_lane", false));
    const { root, controller } = mount(busy, send, {}, async () =>
      occupied(busy, "create_starter_lane", false),
    );

    typeAndSend(root, "one");
    await settle();
    typeAndSend(root, "two");
    await settle();
    await settle();

    expect(root.querySelector("[data-d1-slot-busy-abandon]")).toBeNull();
    const held = root.querySelector<HTMLElement>("[data-d1-slot-busy-mutation]");
    expect(held).not.toBeNull();
    expect(held!.textContent).toContain("create_starter_lane");
    controller.dispose();
  });

  test("an adapter discovery read is offered the host's own abandon", async () => {
    const busy = projection({
      composer: { ...D1_PROJECTION.composer, busy: false, canSubmitImmediately: true },
    });
    const send = vi.fn(async () => occupied(busy, "probe_agent_adapter", true));
    const abandon = vi.fn(async () => free(busy));
    const { root, controller } = mount(
      busy,
      send,
      { agentProbe: { abandon } },
      async () => occupied(busy, "probe_agent_adapter", true),
    );

    typeAndSend(root, "one");
    await settle();
    typeAndSend(root, "two");
    await settle();
    await settle();

    const control = root.querySelector<HTMLButtonElement>("[data-d1-slot-busy-abandon]");
    expect(control).not.toBeNull();
    expect(control!.textContent).toContain("probe_agent_adapter");
    control!.click();
    await settle();
    expect(abandon).toHaveBeenCalledTimes(1);
    controller.dispose();
  });

  test("the refusal is retired the moment the slot frees", async () => {
    const busy = projection({
      composer: { ...D1_PROJECTION.composer, busy: false, canSubmitImmediately: true },
    });
    const send = vi.fn(async () => occupied(busy, "create_starter_lane", false));
    const { root, controller } = mount(busy, send, {}, async () =>
      occupied(busy, "create_starter_lane", false),
    );

    typeAndSend(root, "one");
    await settle();
    typeAndSend(root, "two");
    await settle();
    await settle();
    expect(root.querySelector("[data-d1-slot-busy]")).not.toBeNull();

    controller.applyResult(free(busy));
    await settle();

    expect(root.querySelector("[data-d1-slot-busy]")).toBeNull();
    controller.dispose();
  });
});

/* ---- defect 24: Cancel follows Core's active-turn fact ---- */

describe("defect 24 — Cancel active turn is a claim about work", () => {
  test("no cancel control is drawn when Core reports no cancellable turn", () => {
    const idle = projection({
      composer: { ...D1_PROJECTION.composer, busy: false, canCancel: false },
    });
    const { root, controller } = mount(idle, vi.fn(async () => free(idle)));

    expect(root.querySelector("[data-cancel-turn]")).toBeNull();
    controller.dispose();
  });

  test("the cancel control is drawn on Core's own cancellable turn", () => {
    const live = projection({
      composer: { ...D1_PROJECTION.composer, busy: true, canCancel: true },
    });
    const { root, controller } = mount(live, vi.fn(async () => free(live)));

    expect(root.querySelector("[data-cancel-turn]")).not.toBeNull();
    controller.dispose();
  });
});
