// @vitest-environment jsdom

import { afterEach, describe, expect, test, vi } from "vitest";

import { renderCockpitTopbar } from "../src/components/cockpit_topbar";
import {
  renderD1Cockpit,
  type D1CockpitProjection,
  type D1IntentResult,
} from "../src/screens/d1_cockpit";

function projection(overrides: Partial<D1CockpitProjection> = {}): D1CockpitProjection {
  return {
    preferences: {
      locale: "en",
      skin: "aurora",
      mode: "dark",
      density: "regular",
      motion: "reduced",
      diagnostics: [],
    },
    selectedLaneId: null,
    contextDock: {
      source: null,
      context: null,
      laneAgent: null,
      provider: null,
      services: [],
      checklist: [],
    },
    lanes: [],
    environment: {
      cwd: "/workspace/viden",
      providerId: "fallback",
      model: "test-local",
      workMode: "build",
      permissionLevel: "ask",
      tokenTotal: 0,
      costMicroUsd: null,
    },
    liveWork: { tasks: [], tools: [], approvals: [], queuedInputs: [], evidence: [] },
    transcript: [],
    workspaceEligibility: null,
    starterLanePreviews: [],
    agentAdapters: [],
    agentSessions: [],
    composer: {
      editable: true,
      busy: false,
      canCancel: false,
      canSubmitImmediately: true,
    },
    statusbar: {
      workMode: "build",
      permissionLevel: "ask",
      context: null,
      eventStreamPosition: 0,
      lane: null,
      latency: null,
      tokens: null,
      diagnosticsCount: 0,
      requests: null,
      pendingGateCount: 0,
    },
    permissionDock: { workMode: "build", permissionLevel: "ask", request: null },
    recovery: {
      connection: "live",
      state: "live",
      detail: null,
      hint: null,
      recoverable: false,
      businessSuccessBlocked: false,
      usedTokens: null,
      hardTokenLimit: null,
      missingCapabilities: [],
      actions: [],
    },
    unavailableFeatures: [],
    ...overrides,
  } as D1CockpitProjection;
}

function laneProjection(laneId: string, rows: number): D1CockpitProjection {
  return projection({
    selectedLaneId: laneId,
    lanes: [
      {
        id: laneId,
        summary: `${laneId} active`,
        status: "running",
        role: "coder",
        route: "built_in",
        branch: null,
        worktree: null,
        awaitingHuman: false,
      },
    ] as never,
    transcript: Array.from({ length: rows }, (_, index) => ({
      id: `row-${index}`,
      kind: index % 2 === 0 ? "user" : "assistant",
      content: `content-${index}`,
    })),
  });
}

function idleResult(next: D1CockpitProjection): D1IntentResult {
  return {
    projection: next,
    pendingCommandId: null,
    outcome: { state: "idle", reason: null },
  } as D1IntentResult;
}

function mount(
  initial: D1CockpitProjection,
  poll = vi.fn(async (..._args: unknown[]) => idleResult(projection())),
  options = {},
) {
  document.body.innerHTML = '<div id="app"></div>';
  const root = document.querySelector<HTMLElement>("#app")!;
  const controller = renderD1Cockpit(
    root,
    initial,
    vi.fn(async () => idleResult(initial)),
    poll,
    undefined,
    undefined,
    { poll: false, ...options },
  );
  return { root, controller, poll };
}

afterEach(() => {
  delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  vi.useRealTimers();
});

describe("window chrome", () => {
  test("the HTML traffic lights render only outside the native shell", () => {
    const inBrowser = renderCockpitTopbar(projection(), "en", false);
    expect(inBrowser.element.querySelector(".tl")).not.toBeNull();

    (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    const inTauri = renderCockpitTopbar(projection(), "en", false);
    // macOS supplies the native overlay controls; a second HTML set is a
    // duplicated-chrome defect.
    expect(inTauri.element.querySelector(".tl")).toBeNull();
  });
});

describe("composer lane notices", () => {
  test("a zero-lane project invites lane creation instead of reporting a stale lane", () => {
    const { root } = mount(projection());
    const notice = root.querySelector("[data-mutation-blocked]");
    expect(notice?.textContent).not.toContain("no longer available");
    expect(notice?.textContent).toContain("Create or select a Lane");
  });

  test("a selection that vanished from a project that still has lanes stays fail-closed", () => {
    const { root, controller } = mount(laneProjection("lane-1", 2));
    const survivor = laneProjection("lane-2", 0);
    survivor.selectedLaneId = null;
    controller.applyProjection(survivor);
    // lane-1 is gone but lane-2 exists: keep the honest stale-lane block
    // rather than silently retargeting the composer.
    expect(root.querySelector("[data-mutation-blocked]")?.textContent).toContain(
      "no longer available",
    );
  });

  test("a project with zero lanes clears the vanished selection", () => {
    const { root, controller } = mount(laneProjection("lane-1", 2));
    const emptied = projection();
    controller.applyProjection(emptied);
    const notice = root.querySelector("[data-mutation-blocked]");
    expect(notice?.textContent).not.toContain("no longer available");
    expect(notice?.textContent).toContain("Create or select a Lane");
  });
});

describe("transcript scroll", () => {
  function geometry(region: HTMLElement, scrollTop: number): void {
    Object.defineProperty(region, "scrollHeight", { value: 1000, configurable: true });
    Object.defineProperty(region, "clientHeight", { value: 200, configurable: true });
    region.scrollTop = scrollTop;
  }

  test("a reader scrolled into history keeps their position across a Core refresh", () => {
    const { root, controller } = mount(laneProjection("lane-1", 40));
    const region = root.querySelector<HTMLElement>(".d1-transcript")!;
    geometry(region, 300);
    region.dispatchEvent(new Event("scroll"));
    expect(controller.transcript.followLatest).toBe(false);

    const next = laneProjection("lane-1", 41);
    controller.applyProjection(next);

    const refreshed = root.querySelector<HTMLElement>(".d1-transcript")!;
    // The refresh must not teleport the reader back to an edge.
    expect(refreshed.scrollTop).toBe(300);
    expect(controller.transcript.followLatest).toBe(false);
  });

  test("a reader at the bottom keeps following the newest output", () => {
    const { root, controller } = mount(laneProjection("lane-1", 40));
    const region = root.querySelector<HTMLElement>(".d1-transcript")!;
    geometry(region, 800);
    region.dispatchEvent(new Event("scroll"));
    expect(controller.transcript.followLatest).toBe(true);

    const next = laneProjection("lane-1", 41);
    controller.applyProjection(next);

    const refreshed = root.querySelector<HTMLElement>(".d1-transcript")!;
    expect(refreshed.scrollTop).toBe(refreshed.scrollHeight);
    expect(controller.transcript.followLatest).toBe(true);
  });
});

describe("poll cadence", () => {
  test("the background poll long-polls Core instead of draining an empty queue", async () => {
    vi.useFakeTimers();
    const poll = vi.fn(async (..._args: unknown[]) => idleResult(projection()));
    const { controller } = mount(projection(), poll, { poll: true });
    await vi.advanceTimersByTimeAsync(300);
    expect(poll).toHaveBeenCalled();
    // waitForEvent=true lets the Rust side hold the poll until an ordered
    // event arrives, so a reply renders when it happens rather than on the
    // next timer tick.
    expect(poll.mock.calls[0][1]).toBe(true);
    controller.dispose();
  });
});

describe("core event wake", () => {
  test("a Core wake refreshes immediately instead of waiting for the next tick", async () => {
    vi.useFakeTimers();
    const listeners: Array<() => void> = [];
    const poll = vi.fn(async (..._args: unknown[]) => idleResult(projection()));
    const { controller } = mount(projection(), poll, {
      poll: true,
      onCoreWake: (handler: () => void) => {
        listeners.push(handler);
        return () => undefined;
      },
    });
    expect(listeners).toHaveLength(1);

    listeners[0]();
    await vi.advanceTimersByTimeAsync(0);
    // The wake drives the read; no timer had to elapse first.
    expect(poll).toHaveBeenCalledTimes(1);
    controller.dispose();
  });

  test("the drain timer stops once Core pushes wakes", async () => {
    vi.useFakeTimers();
    const poll = vi.fn(async (..._args: unknown[]) => idleResult(projection()));
    const { controller } = mount(projection(), poll, {
      poll: true,
      onCoreWake: () => () => undefined,
    });
    await vi.advanceTimersByTimeAsync(2000);
    // With a push subscription the shell must not keep its own 250ms drain
    // loop; that loop is the fallback for hosts without the wake.
    expect(poll).not.toHaveBeenCalled();
    controller.dispose();
  });
});

describe("partitioned refresh", () => {
  test("an unrelated Core change leaves the context dock element identity intact", () => {
    const first = laneProjection("lane-1", 4);
    const { root, controller } = mount(first);
    const dockBefore = root.querySelector('[data-shell-landmark="context-dock"]');
    const statusBefore = root.querySelector('[data-shell-landmark="statusbar"]');

    // Only the transcript grows; the dock and status facts are untouched.
    controller.applyProjection(laneProjection("lane-1", 5));

    // Rebuilding a region whose facts did not change is the cost this
    // partitioning removes, and it is what drops :hover and focus mid-stream.
    expect(root.querySelector('[data-shell-landmark="context-dock"]')).toBe(dockBefore);
    expect(root.querySelector('[data-shell-landmark="statusbar"]')).toBe(statusBefore);
  });

  test("the drawer toggle drives the mounted dock after a partial refresh", () => {
    const { root, controller } = mount(laneProjection("lane-1", 4));
    // Grow only the transcript so the dock is skipped while the topbar is
    // rebuilt; the toggle must still reach the dock that is mounted.
    controller.applyProjection(laneProjection("lane-1", 5));

    const toggle = root.querySelector<HTMLButtonElement>("[data-context-drawer-toggle]")!;
    toggle.click();
    const dock = root.querySelector<HTMLElement>('[data-shell-landmark="context-dock"]')!;
    expect(dock.dataset.drawerOpen).toBe("true");
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
  });

  test("a changed region is still replaced", () => {
    const { root, controller } = mount(laneProjection("lane-1", 4));
    const statusBefore = root.querySelector('[data-shell-landmark="statusbar"]');

    // The statusbar renders Core statusbar facts, so a work-mode change is a
    // status-region change and must replace the mounted element.
    const next = laneProjection("lane-1", 4);
    next.statusbar = { ...next.statusbar, workMode: "plan" };
    controller.applyProjection(next);

    expect(root.querySelector('[data-shell-landmark="statusbar"]')).not.toBe(statusBefore);
    expect(root.querySelector('[data-shell-landmark="statusbar"]')?.textContent).toContain(
      "plan",
    );
  });
});

describe("working feedback", () => {
  function busyLane(seconds: number, status = "running"): D1CockpitProjection {
    const next = laneProjection("lane-1", 2);
    next.composer = { ...next.composer, busy: true, canCancel: true };
    next.agentSessions = [
      {
        sessionId: "session-1",
        laneId: "lane-1",
        agentId: "codex",
        model: "gpt-5-codex",
        status,
        task: "draw a cat",
        diagnostic: null,
        conversation: [],
      },
    ] as never;
    next.contextDock = {
      ...next.contextDock,
      laneAgent: {
        laneId: "lane-1",
        sessionId: "session-1",
        agentId: "codex",
        model: "gpt-5-codex",
        status,
      },
    } as never;
    void seconds;
    return next;
  }

  test("a busy turn shows a live activity marker instead of a static line", () => {
    const { root, controller } = mount(laneProjection("lane-1", 2));
    controller.applyProjection(busyLane(0));
    const strip = root.querySelector<HTMLElement>("[data-work-status]");
    expect(strip).not.toBeNull();
    expect(strip?.dataset.workStatus).toBe("busy");
    expect(strip?.querySelector("[data-work-marker]")).not.toBeNull();
  });

  test("the strip reports the Core session status rather than an invented narrative", () => {
    const { root, controller } = mount(laneProjection("lane-1", 2));
    controller.applyProjection(busyLane(0, "waiting_approval"));
    const strip = root.querySelector<HTMLElement>("[data-work-status]");
    expect(strip?.dataset.workCoreStatus).toBe("waiting_approval");
    expect(strip?.textContent).not.toContain("Thinking");
  });

  test("elapsed time ticks and is labelled as client-observed", async () => {
    vi.useFakeTimers();
    const { root, controller } = mount(laneProjection("lane-1", 2));
    controller.applyProjection(busyLane(0));
    const timer = () => root.querySelector<HTMLElement>("[data-work-elapsed]");
    expect(timer()?.textContent).toBe("0:00");
    await vi.advanceTimersByTimeAsync(65_000);
    expect(timer()?.textContent).toBe("1:05");
    // The anchor is the moment the client observed work start, because the
    // owner-scoped Core start timestamp is unavailable (GUI-CORE-010).
    expect(timer()?.title).toContain("GUI-CORE-010");
  });

  test("cancel is reachable from the strip and names its shortcut", () => {
    const { root, controller } = mount(laneProjection("lane-1", 2));
    controller.applyProjection(busyLane(0));
    const cancel = root.querySelector<HTMLButtonElement>("[data-work-cancel]");
    expect(cancel).not.toBeNull();
    expect(cancel?.textContent).toContain("Esc");
  });

  test("the strip disappears when Core reports the turn is no longer busy", () => {
    const { root, controller } = mount(laneProjection("lane-1", 2));
    controller.applyProjection(busyLane(0));
    expect(root.querySelector("[data-work-status='busy']")).not.toBeNull();
    controller.applyProjection(laneProjection("lane-1", 3));
    expect(root.querySelector("[data-work-status='busy']")).toBeNull();
  });
});

describe("agent message content parts", () => {
  function withParts(parts: unknown[]): D1CockpitProjection {
    const next = laneProjection("lane-1", 0);
    next.agentSessions = [
      {
        sessionId: "session-1",
        laneId: "lane-1",
        agentId: "codex",
        model: "gpt-5-codex",
        status: "running",
        task: "draw a cat",
        diagnostic: null,
        conversation: [
          { messageId: "turn-1", role: "assistant", content: "here is the cat", parts },
        ],
      },
    ] as never;
    next.contextDock = {
      ...next.contextDock,
      laneAgent: { laneId: "lane-1", sessionId: "session-1", agentId: "codex", model: null },
    } as never;
    return next;
  }

  test("an image part renders next to the text Core published", () => {
    const { root } = mount(
      withParts([
        { kind: "image", mediaType: "image/png", reference: "file:///tmp/cat.png", text: null, label: "an orange cat" },
      ]),
    );
    const image = root.querySelector<HTMLImageElement>("[data-content-part='image'] img");
    expect(image?.getAttribute("src")).toBe("file:///tmp/cat.png");
    expect(image?.alt).toBe("an orange cat");
  });

  test("a part this build cannot render is named rather than dropped", () => {
    const { root } = mount(
      withParts([{ kind: "hologram", mediaType: null, reference: null, text: null, label: null }]),
    );
    const part = root.querySelector<HTMLElement>("[data-content-part='hologram']");
    expect(part).not.toBeNull();
    expect(part?.textContent).toContain("hologram");
  });

  test("a message with no parts renders exactly as before", () => {
    const { root } = mount(withParts([]));
    expect(root.querySelector("[data-content-part]")).toBeNull();
  });

  test("a workspace reference is resolved through the host before it renders", async () => {
    const asked: string[] = [];
    const resolveContent = vi.fn(async (reference: string) => {
      asked.push(reference);
      return "data:image/png;base64,Y2F0";
    });
    const { root } = mount(
      withParts([
        {
          kind: "image",
          mediaType: "image/png",
          reference: ".viden/agents/parts/abc.png",
          text: null,
          label: null,
        },
      ]),
      undefined,
      { resolveContent },
    );

    expect(asked).toEqual([".viden/agents/parts/abc.png"]);
    await vi.waitFor(() => {
      const image = root.querySelector<HTMLImageElement>("[data-content-part='image'] img");
      expect(image?.getAttribute("src")).toBe("data:image/png;base64,Y2F0");
    });
  });

  test("a reference the host already serves is not re-resolved", () => {
    const resolveContent = vi.fn(async () => "data:image/png;base64,Y2F0");
    const { root } = mount(
      withParts([
        {
          kind: "image",
          mediaType: "image/png",
          reference: "https://example.test/cat.png",
          text: null,
          label: null,
        },
      ]),
      undefined,
      { resolveContent },
    );

    expect(resolveContent).not.toHaveBeenCalled();
    const image = root.querySelector<HTMLImageElement>("[data-content-part='image'] img");
    expect(image?.getAttribute("src")).toBe("https://example.test/cat.png");
  });

  test("content the host cannot read is named instead of rendering a broken image", async () => {
    const resolveContent = vi.fn(async () => {
      throw new Error("gui.agentContent.unreadable");
    });
    const { root } = mount(
      withParts([
        {
          kind: "image",
          mediaType: "image/png",
          reference: ".viden/agents/parts/gone.png",
          text: null,
          label: null,
        },
      ]),
      undefined,
      { resolveContent },
    );

    await vi.waitFor(() => {
      const holder = root.querySelector<HTMLElement>("[data-content-part='image']");
      expect(holder?.dataset.contentUnresolved).toBe("true");
      expect(holder?.textContent).toContain(".viden/agents/parts/gone.png");
    });
  });
});

describe("agent identity on a reply", () => {
  function acpProjection(agentId: string, adapters: unknown[]): D1CockpitProjection {
    const next = laneProjection("lane-1", 0);
    next.agentAdapters = adapters as never;
    next.agentSessions = [
      {
        sessionId: "session-1",
        laneId: "lane-1",
        agentId,
        model: "gpt-5-codex",
        status: "running",
        task: "draw a cat",
        diagnostic: null,
        conversation: [
          { messageId: "turn-1", role: "assistant", content: "here is the cat", parts: [] },
        ],
      },
    ] as never;
    next.contextDock = {
      ...next.contextDock,
      laneAgent: { laneId: "lane-1", sessionId: "session-1", agentId, model: null },
    } as never;
    return next;
  }

  function assistantIdentity(root: HTMLElement) {
    const row = root.querySelector<HTMLElement>("[data-transcript-row='assistant']");
    return {
      label: row?.querySelector<HTMLElement>("[data-row-label]")?.textContent,
      avatar: row?.querySelector<HTMLElement>(".d1-row-avatar")?.textContent,
      agent: row?.dataset.agentId,
    };
  }

  test("a reply names the agent Core published, not the shell", () => {
    const { root } = mount(
      acpProjection("codex-acp", [
        { agentId: "codex-acp", displayName: "Codex", startability: "ready", diagnostics: [] },
      ]),
    );
    const identity = assistantIdentity(root);
    expect(identity.label).toBe("CODEX");
    expect(identity.agent).toBe("codex-acp");
  });

  test("an agent with no adapter falls back to the id Core scoped the session by", () => {
    const { root } = mount(acpProjection("claude-acp", []));
    // Inventing a friendly name for an agent Core did not describe would
    // assert an identity the client cannot vouch for.
    const identity = assistantIdentity(root);
    expect(identity.label).toBe("CLAUDE-ACP");
    expect(identity.agent).toBe("claude-acp");
  });

  test("a known agent shows its registered brand mark, not a letter", () => {
    const { root } = mount(
      acpProjection("codex-acp", [
        { agentId: "codex-acp", displayName: "Codex", startability: "ready", diagnostics: [] },
      ]),
    );
    const row = root.querySelector<HTMLElement>("[data-transcript-row='assistant']");
    expect(row?.querySelector<HTMLElement>("[data-agent-logo]")?.dataset.agentLogo).toBe("codex");
  });

  test("an agent with no registered mark keeps the letter avatar", () => {
    const { root } = mount(acpProjection("mystery-acp", []));
    const row = root.querySelector<HTMLElement>("[data-transcript-row='assistant']");
    expect(row?.querySelector("[data-agent-logo]")).toBeNull();
    expect(row?.querySelector<HTMLElement>(".d1-row-avatar")?.textContent).toBe("M");
  });

  test("a Lane's own output rows carry the agent that owns the Lane", () => {
    // A native transcript row for a Lane whose Agent session Core confirmed is
    // that agent's output. Leaving it as VIDEN put the shell's name on Codex's
    // reply whenever the ACP conversation had not been published yet.
    const next = acpProjection("codex-acp", [
      { agentId: "codex-acp", displayName: "Codex", startability: "ready", diagnostics: [] },
    ]);
    (next.agentSessions as unknown as Array<{ conversation: unknown[] }>)[0].conversation = [];
    next.transcript = [
      { id: "row-0", kind: "assistant", content: "drawing the cat" },
    ] as never;

    const { root } = mount(next);
    const labels = Array.from(
      root.querySelectorAll<HTMLElement>("[data-transcript-row='assistant'] [data-row-label]"),
    ).map((element) => element.textContent);
    expect(labels.length).toBeGreaterThan(0);
    expect(labels).not.toContain("VIDEN");
  });

  test("a row with no agent behind it stays the shell's own reply", () => {
    const { root } = mount(laneProjection("lane-1", 2));
    expect(assistantIdentity(root)).toEqual({
      label: "VIDEN",
      avatar: "V",
      agent: undefined,
    });
  });
});

describe("activity rail navigation", () => {
  test("a routing slot opens its restored screen", () => {
    const navigated: string[] = [];
    document.body.innerHTML = '<div id="app"></div>';
    const root = document.querySelector<HTMLElement>("#app")!;
    renderD1Cockpit(
      root,
      laneProjection("lane-1", 1),
      vi.fn(async () => idleResult(laneProjection("lane-1", 1))),
      vi.fn(async () => idleResult(laneProjection("lane-1", 1))),
      undefined,
      undefined,
      { poll: false, onNavigate: (route: string) => navigated.push(route) },
    );

    root.querySelector<HTMLButtonElement>("[data-rail-route='d12']")!.click();
    root.querySelector<HTMLButtonElement>("[data-rail-route='d10']")!.click();
    expect(navigated).toEqual(["d12", "d10"]);
  });

  test("without a navigation handler the routing slots stay disabled", () => {
    const { root } = mount(laneProjection("lane-1", 1));
    const routed = root.querySelectorAll("[data-rail-route]");
    expect(routed).toHaveLength(0);
    // The slots remain visible but inert rather than opening an empty screen.
    expect(root.querySelectorAll(".d1-activity button:disabled").length).toBeGreaterThan(0);
  });

  test("the Work slot focuses the composer instead of being enabled and inert", () => {
    const { root } = mount(laneProjection("lane-1", 1));
    const work = root.querySelector<HTMLButtonElement>("[data-rail-focus-work]")!;

    // It is the current screen, so it stays marked as such rather than routing.
    expect(work.getAttribute("aria-current")).toBe("page");
    expect(work.disabled).toBe(false);
    expect(work.dataset.railRoute).toBeUndefined();

    root.querySelector<HTMLTextAreaElement>("[data-composer]")!.blur();
    work.click();
    expect(document.activeElement).toBe(root.querySelector("[data-composer]"));
  });

  test("no activity-rail control is enabled without an action behind it", () => {
    const { root } = mount(laneProjection("lane-1", 1), undefined, {
      onNavigate: () => {},
    });
    for (const control of root.querySelectorAll<HTMLButtonElement>(".d1-activity button")) {
      if (control.disabled) continue;
      const actionable = control.dataset.railRoute
        ?? control.dataset.lanesToggle
        ?? control.dataset.railFocusWork
        ?? control.dataset.laneSidebarPin;
      expect(actionable, control.getAttribute("aria-label") ?? "").toBeDefined();
    }
  });
});
