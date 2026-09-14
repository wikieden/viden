// @vitest-environment jsdom

/**
 * The webview half of the E3d shell-path regressions
 * (`docs/release-evidence/gui-trusted-delivery/checkpoints.md`, defect 36).
 *
 * The Rust half lives in `h7_shell_path.rs`, which drives the production
 * `lib.rs` command handlers against live Core with the desktop pump running.
 * This half drives the production `renderD1Cockpit` over the answers that path
 * actually returns — in particular the one H6 read as success: a
 * `QueueFollowUp` the client has *sent* and Core has not acknowledged.
 */

import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import {
  renderD1Cockpit,
  type D1CockpitProjection,
  type D1Intent,
  type D1IntentResult,
} from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

const LANE = "lane-core";
const FOLLOW_UP = "Summarise what you just changed";

function free(next: D1CockpitProjection): D1IntentResult {
  return {
    projection: next,
    pendingCommandId: null,
    pendingCommand: null,
    outcome: { state: "confirmed", reason: null },
  };
}

/** Core reporting a live turn for the selected Lane, as E3d's step 8 was. */
function running(): D1CockpitProjection {
  const base = structuredClone(D1_PROJECTION);
  return {
    ...base,
    selectedLaneId: LANE,
    composer: { ...base.composer, editable: true, busy: true, canSubmitImmediately: false },
    liveWork: { ...base.liveWork, queuedInputs: [] },
  };
}

function mount(initial: D1CockpitProjection, options: Record<string, unknown> = {}) {
  document.body.innerHTML = '<main id="app"></main>';
  const root = document.querySelector<HTMLElement>("#app")!;
  const controller = renderD1Cockpit(
    root,
    initial,
    (async () => free(initial)) as never,
    (async () => free(initial)) as never,
    undefined,
    undefined,
    { poll: false, showWelcome: false, onNavigate: () => undefined, ...options },
  );
  return { root, controller };
}

const flush = async (): Promise<void> => {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
};

const composer = (root: HTMLElement): HTMLTextAreaElement =>
  root.querySelector<HTMLTextAreaElement>("[data-composer]")!;

function submit(root: HTMLElement, text: string): void {
  const input = composer(root);
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

/* ---- defect 36: a follow-up is retired only by Core's own InputQueued ---- */

describe("defect 36 — the composer never retires a draft it has not confirmed sent", () => {
  /** The shell's real answer for a Lane-scoped queue: sent, unacknowledged. */
  const unacknowledged = (view: D1CockpitProjection) => async (): Promise<D1IntentResult> => ({
    projection: view,
    pendingCommandId: "gui-d1-queue-1",
    pendingCommand: {
      commandId: "gui-d1-queue-1",
      kind: "queue_follow_up",
      ageMs: 120,
      abandonable: false,
    },
    outcome: { state: "pending", reason: null },
  });

  test("a follow-up Core never acknowledges keeps its text and says so", async () => {
    const view = running();
    const poll = vi.fn(unacknowledged(view));
    const { root, controller } = mount(view, {
      queueFollowUp: { send: vi.fn(unacknowledged(view)), poll },
    });
    await flush();

    submit(root, FOLLOW_UP);
    await flush();
    // The composer must not have emptied on the send alone: at this instant
    // Core has published nothing at all, which is exactly the state E3d's
    // composer cleared in.
    expect(composer(root).value).toBe(FOLLOW_UP);

    // Drain the bounded confirm wait.
    for (let tick = 0; tick < 24; tick += 1) {
      await vi.advanceTimersByTimeAsync(100);
      await flush();
    }

    expect(poll).toHaveBeenCalled();
    const alert = root.querySelector<HTMLElement>("[data-d1-queue-unconfirmed]");
    expect(alert, "an unacknowledged follow-up is announced, not swallowed").not.toBeNull();
    expect(alert!.getAttribute("role")).toBe("alert");
    expect(composer(root).value).toBe(FOLLOW_UP);
    controller.dispose();
  });

  test("Core's own InputQueued is what retires the draft", async () => {
    const view = running();
    const queuedView: D1CockpitProjection = {
      ...view,
      liveWork: {
        ...view.liveWork,
        queuedInputs: [{ id: "input-1", contentPreview: FOLLOW_UP }],
      },
    };
    const confirmed = async (): Promise<D1IntentResult> => ({
      projection: queuedView,
      pendingCommandId: null,
      pendingCommand: null,
      outcome: { state: "confirmed", reason: null },
    });
    const { root, controller } = mount(view, {
      queueFollowUp: { send: vi.fn(confirmed), poll: vi.fn(confirmed) },
    });
    await flush();

    submit(root, FOLLOW_UP);
    await flush();
    await vi.advanceTimersByTimeAsync(100);
    await flush();

    expect(composer(root).value).toBe("");
    expect(root.querySelector("[data-d1-queue-unconfirmed]")).toBeNull();
    controller.dispose();
  });

  test("a Core refusal keeps the text too, and renders Core's own words", async () => {
    const view = running();
    const rejected = async (): Promise<D1IntentResult> => ({
      projection: view,
      pendingCommandId: null,
      pendingCommand: null,
      outcome: { state: "rejected", reason: "queue_follow_up_owner_mismatch" },
    });
    const { root, controller } = mount(view, {
      queueFollowUp: { send: vi.fn(rejected), poll: vi.fn(rejected) },
    });
    await flush();

    submit(root, FOLLOW_UP);
    await flush();
    await vi.advanceTimersByTimeAsync(100);
    await flush();

    const rejection = root.querySelector<HTMLElement>("[data-d1-rejection]");
    expect(rejection).not.toBeNull();
    expect(rejection!.textContent).toContain("queue_follow_up_owner_mismatch");
    expect(composer(root).value).toBe(FOLLOW_UP);
    controller.dispose();
  });
});
