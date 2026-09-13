// @vitest-environment jsdom

import { beforeEach, describe, expect, test, vi } from "vitest";

import {
  renderD16AgentBoard,
  type D16AgentBoardProjection,
} from "../src/screens/d16_agent_board";
import {
  AGENT_SESSION_CONTROL_CAPABILITY,
  IDLE_AGENT_BOARD,
  type D16Action,
  type D16IntentResult,
  type D16Session,
} from "../src/models/board";
import type { D14AuditProjection } from "../src/screens/d14_audit_timeline";

/**
 * D16 Agent Board view specs.
 *
 * The projections here mirror what `apps/gui/tests/d16_agent_board.rs` proves
 * the Rust side produces from the canonical `agent-session-control.json`
 * fixture; these tests are about what the *view* does with such a projection —
 * the hierarchy, the disabled labels, the two-step switch, and the honest
 * absences.
 */

function actions(overrides: Partial<Record<string, Partial<D16Action>>> = {}): D16Action[] {
  const base: Record<string, D16Action> = {
    pause: { kind: "pause", available: true, block: null, request: false },
    resume: {
      kind: "resume",
      available: false,
      block: "session_status_running",
      request: false,
    },
    kill: { kind: "kill", available: true, block: null, request: false },
    stop: { kind: "stop", available: true, block: null, request: false },
    retry: {
      kind: "retry",
      available: false,
      block: "session_status_running",
      request: false,
    },
    switch_model: { kind: "switch_model", available: true, block: null, request: false },
    downgrade_manual: {
      kind: "downgrade_manual",
      available: false,
      block: "D16-NO-CORE-FACT",
      request: true,
    },
    ask_user: {
      kind: "ask_user",
      available: false,
      block: "D16-NO-CORE-FACT",
      request: true,
    },
  };
  for (const [kind, patch] of Object.entries(overrides)) {
    base[kind] = { ...base[kind], ...patch };
  }
  return Object.values(base);
}

function session(overrides: Partial<D16Session> = {}): D16Session {
  return {
    sessionId: "session_control",
    laneId: "lane_session_control",
    agentId: "codex-acp",
    model: "gpt-5-codex",
    task: "Return Err(Empty) from ConfigLoader::load",
    status: "running",
    diagnostic: null,
    control: null,
    terminal: false,
    awaitsHuman: false,
    actions: actions(),
    ...overrides,
  };
}

function board(overrides: Partial<D16AgentBoardProjection> = {}): D16AgentBoardProjection {
  const only = session();
  return {
    capabilityAvailable: true,
    lanes: [
      {
        laneId: "lane_session_control",
        published: true,
        summary: "config loader refactor",
        role: "coder",
        route: "acp",
        gateStrength: "cooperative",
        status: "running",
        branch: "vd/config-loader",
        worktree: null,
        sessions: [only],
        awaiting: 0,
      },
    ],
    totalLanes: 1,
    totalSessions: 1,
    awaitingTotal: 0,
    selectedSessionId: only.sessionId,
    detail: {
      sessionId: only.sessionId,
      laneId: only.laneId,
      conversation: [],
      evidence: [],
      switchModel: {
        loadSessionId: only.sessionId,
        currentModel: only.model,
        agentId: only.agentId,
      },
      switchModelBlock: null,
    },
    ...overrides,
  };
}

const EMPTY_AUDIT: D14AuditProjection = {
  outcome: { state: "confirmed", reason: null },
  rows: [],
  nextBefore: null,
  complete: true,
  loaded: true,
  pendingCommandId: null,
  capabilityAvailable: true,
  scope: { kind: "session", id: "session_control" },
};

function mount(
  projection: D16AgentBoardProjection,
  options: Parameters<typeof renderD16AgentBoard>[3] = {},
): HTMLElement {
  const root = document.createElement("div");
  document.body.append(root);
  renderD16AgentBoard(root, projection, "en", options);
  return root;
}

const flush = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0));

describe("the Agent Board draws the Lane/session hierarchy", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("nests every session under the Lane that owns it", () => {
    const root = mount(board());
    const lane = root.querySelector<HTMLElement>("[data-d16-lane='lane_session_control']");
    expect(lane).not.toBeNull();
    // The row is a *descendant* of its Lane group, which is the invariant: a
    // session is never rendered detached from the Lane Core attributes it to.
    const row = lane!.querySelector<HTMLElement>("[data-d16-session='session_control']");
    expect(row).not.toBeNull();
    expect(root.querySelectorAll("[data-d16-session]")).toHaveLength(1);
  });

  test("says a Lane record is absent instead of borrowing a name for it", () => {
    const projection = board();
    projection.lanes[0] = {
      ...projection.lanes[0],
      published: false,
      summary: null,
      route: null,
      gateStrength: null,
      branch: null,
    };
    const root = mount(projection);
    const name = root.querySelector<HTMLElement>("[data-d16-lane-unpublished]");
    expect(name?.textContent).toContain("no Lane record");
    // The session is still there: hiding it would be the larger lie.
    expect(root.querySelector("[data-d16-session='session_control']")).not.toBeNull();
  });

  test("reports no Lane as absence rather than an empty board", () => {
    const root = mount(IDLE_AGENT_BOARD);
    const empty = root.querySelector<HTMLElement>("[data-d16-empty='no-lane']");
    expect(empty?.textContent).toContain("No Lane");
    expect(empty?.textContent).toContain("never invents one");
    // With no session the detail pane stays empty rather than showing a
    // placeholder agent.
    expect(root.querySelector("[data-d16-absent='detail']")).not.toBeNull();
    expect(root.querySelector("[data-d16-actions]")).toBeNull();
  });

  test("counts lanes, sessions and the rows blocking on a human from Core", () => {
    const root = mount(board({ totalLanes: 4, totalSessions: 8, awaitingTotal: 1 }));
    const counts = root.querySelector<HTMLElement>("[data-d16-counts]");
    expect(counts?.textContent).toBe("4 lanes · 8 sessions · 1 awaiting you");
  });
});

describe("every disabled control names the reason Core would answer with", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("renders Core's session_status_<current> family as a sentence", () => {
    const projection = board();
    projection.lanes[0].sessions[0] = session({
      status: "paused",
      actions: actions({
        pause: { available: false, block: "session_status_paused" },
        resume: { available: true, block: null },
      }),
    });
    const root = mount(projection, { send: async () => ({}) as D16IntentResult });
    const pause = root.querySelector<HTMLButtonElement>("[data-d16-action='pause']");
    expect(pause?.disabled).toBe(true);
    expect(pause?.getAttribute("aria-label")).toContain("the session is paused");
    expect(root.querySelector("[data-d16-block='session_status_paused']")).not.toBeNull();
    const resume = root.querySelector<HTMLButtonElement>("[data-d16-action='resume']");
    expect(resume?.disabled).toBe(false);
  });

  test("draws the two actions with no Core fact as dashed requests", () => {
    const root = mount(board(), { send: async () => ({}) as D16IntentResult });
    for (const kind of ["downgrade_manual", "ask_user"]) {
      const button = root.querySelector<HTMLButtonElement>(`[data-d16-action='${kind}']`);
      expect(button?.disabled).toBe(true);
      expect(button?.className).toContain("abreq");
      expect(button?.getAttribute("aria-label")).toContain("contract request");
    }
  });

  test("keeps the board readable and names the capability when it is absent", () => {
    const projection = board({ capabilityAvailable: false });
    projection.lanes[0].sessions[0] = session({
      actions: actions({
        pause: { available: false, block: "D16-NO-CAPABILITY" },
        kill: { available: false, block: "D16-NO-CAPABILITY" },
      }),
    });
    const root = mount(projection, { send: async () => ({}) as D16IntentResult });
    const banner = root.querySelector<HTMLElement>("[data-d16-absent='capability']");
    expect(banner?.textContent).toContain(AGENT_SESSION_CONTROL_CAPABILITY);
    // The rows are still there: Lanes and sessions are ordinary facts.
    expect(root.querySelector("[data-d16-session='session_control']")).not.toBeNull();
    const pause = root.querySelector<HTMLButtonElement>("[data-d16-action='pause']");
    expect(pause?.disabled).toBe(true);
    expect(pause?.getAttribute("aria-label")).toContain(AGENT_SESSION_CONTROL_CAPABILITY);
    // Stop predates C14 and must not be retired by its absence.
    expect(
      root.querySelector<HTMLButtonElement>("[data-d16-action='stop']")?.disabled,
    ).toBe(false);
  });
});

describe("the controls send Core's own commands", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("pause sends one pause intent for the selected session", async () => {
    const send = vi.fn(async () => ({
      projection: board(),
      pendingCommandId: null,
      outcome: { state: "confirmed" as const, reason: null },
    }));
    const root = mount(board(), { send });
    root.querySelector<HTMLButtonElement>("[data-d16-action='pause']")!.click();
    await flush();
    expect(send).toHaveBeenCalledWith({ type: "pause", sessionId: "session_control" });
  });

  test("renders Core's refusal verbatim rather than a friendlier sentence", async () => {
    const send = vi.fn(async () => ({
      projection: board(),
      pendingCommandId: null,
      outcome: { state: "rejected" as const, reason: "session_status_paused" },
    }));
    const root = mount(board(), { send });
    root.querySelector<HTMLButtonElement>("[data-d16-action='pause']")!.click();
    await flush();
    const alert = root.querySelector<HTMLElement>("[data-d16-refusal]");
    expect(alert?.textContent).toBe("session_status_paused");
    expect(alert?.getAttribute("role")).toBe("alert");
  });

  test("selecting a row re-reads the board for that session", async () => {
    const second = session({ sessionId: "session_two", task: "Review the sweep" });
    const projection = board();
    projection.lanes[0].sessions.push(second);
    projection.totalSessions = 2;
    const reload = vi.fn(async () => projection);
    const root = mount(projection, { reload });
    root.querySelector<HTMLButtonElement>("[data-d16-session='session_two']")!.click();
    await flush();
    expect(reload).toHaveBeenCalledWith("session_two");
  });
});

describe("switch model is drawn as the two steps it really is", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("opens a panel with both steps and no single switch command", () => {
    const root = mount(board(), { send: async () => ({}) as D16IntentResult });
    root.querySelector<HTMLButtonElement>("[data-d16-action='switch_model']")!.click();
    const panel = root.querySelector<HTMLElement>("[data-d16-switch]");
    expect(panel).not.toBeNull();
    expect(panel!.querySelectorAll("[data-d16-switch-step='1']").length).toBe(2);
    expect(panel!.querySelector("[data-d16-switch-step='2']")).not.toBeNull();
    expect(panel!.querySelector<HTMLElement>("[data-d16-switch-foot]")?.textContent).toContain(
      "no single switch-model command",
    );
  });

  test("refuses step 2 until step 1 actually ended the session", () => {
    const root = mount(board(), { send: async () => ({}) as D16IntentResult });
    root.querySelector<HTMLButtonElement>("[data-d16-action='switch_model']")!.click();
    const start = root.querySelector<HTMLButtonElement>("[data-d16-action='start']");
    expect(start?.disabled).toBe(true);
    expect(start?.title).toContain("end the session first");
  });

  test("step 2 starts the replacement only after step 1 ended the session", async () => {
    const killed = board();
    killed.lanes[0].sessions[0] = session({
      status: "killed",
      terminal: true,
      actions: actions({
        pause: { available: false, block: "session_status_killed" },
        kill: { available: false, block: "D16-SESSION-ENDED" },
        stop: { available: false, block: "D16-SESSION-ENDED" },
        retry: { available: true, block: null },
        switch_model: { available: false, block: "D16-SESSION-ENDED" },
      }),
    });
    const sent: unknown[] = [];
    const send = vi.fn(async (intent: unknown) => {
      sent.push(intent);
      return {
        projection: killed,
        pendingCommandId: null,
        outcome: { state: "confirmed" as const, reason: null },
      };
    });
    const root = mount(board(), { send });
    root.querySelector<HTMLButtonElement>("[data-d16-action='switch_model']")!.click();
    const field = root.querySelector<HTMLInputElement>("[data-d16-switch-model]")!;
    // The field starts from the model Core published, not from a guess.
    expect(field.value).toBe("gpt-5-codex");
    field.value = "claude-sonnet-4.6";
    field.dispatchEvent(new Event("input"));

    // Step 1: end the session. The panel stays open, because the switch is one
    // operator intention carried out by two commands.
    root.querySelector<HTMLButtonElement>("[data-d16-switch-step='1']")!.click();
    await flush();
    expect(sent[0]).toEqual({ type: "kill", sessionId: "session_control" });

    // Step 2 is live only now.
    const start = root.querySelector<HTMLButtonElement>("[data-d16-action='start']")!;
    expect(start.disabled).toBe(false);
    start.click();
    await flush();
    expect(sent[1]).toEqual({
      type: "start",
      sessionId: "session_control",
      model: "claude-sonnet-4.6",
    });
  });
});

describe("the detail pane distinguishes absence from emptiness", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("says the timeline is unread when no audit port is bound", () => {
    const root = mount(board());
    expect(
      root.querySelector<HTMLElement>("[data-d16-absent='events-port']")?.textContent,
    ).toContain("unread rather than empty");
  });

  test("says no rows once Core answered with an empty page", async () => {
    const root = mount(board(), { readEvents: async () => EMPTY_AUDIT });
    await flush();
    expect(
      root.querySelector<HTMLElement>("[data-d16-absent='events-empty']")?.textContent,
    ).toContain("no rows yet");
  });

  test("reads the session's audit rows once per selection", async () => {
    const readEvents = vi.fn(async () => EMPTY_AUDIT);
    const root = mount(board(), { readEvents });
    await flush();
    expect(readEvents).toHaveBeenCalledTimes(1);
    expect(readEvents).toHaveBeenCalledWith("session_control");
    // A re-render for any other reason must not re-ask.
    root.querySelector<HTMLButtonElement>("[data-d16-session='session_control']")!.click();
    await flush();
    expect(readEvents).toHaveBeenCalledTimes(1);
  });

  test("prints the control record only once Core actually controlled the session", () => {
    const bare = mount(board());
    expect(bare.querySelector("[data-d16-control]")).toBeNull();

    const projection = board();
    projection.lanes[0].sessions[0] = session({
      status: "paused",
      control: {
        pausedAt: 1700006010,
        resumedAt: null,
        terminatedBy: null,
        heldInputCount: 2,
      },
    });
    const root = mount(projection);
    const control = root.querySelector<HTMLElement>("[data-d16-control='session_control']");
    expect(control?.textContent).toContain("1700006010");
    expect(control?.textContent).toContain("2 held input");
  });

  test("says none published rather than drawing an empty evidence row", () => {
    const root = mount(board());
    expect(
      root.querySelector<HTMLElement>("[data-d16-absent='evidence']")?.textContent,
    ).toContain("none published");
  });

  test("labels a conversation block as unordered in time", () => {
    const projection = board();
    projection.detail!.conversation = [
      { messageId: "m1", role: "assistant", content: "reading the loader" },
    ];
    const root = mount(projection);
    const message = root.querySelector<HTMLElement>("[data-d16-message='m1']");
    expect(message?.textContent).toContain("reading the loader");
    // Core publishes no timestamp on a conversation message, so the time column
    // stays empty rather than borrowing the audit row's.
    expect(message?.querySelector<HTMLElement>(".tm")?.textContent).toBe("");
  });
});
