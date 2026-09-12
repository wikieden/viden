// @vitest-environment jsdom

// The Welcome "Recent" section is a client of Core's bounded recent-work
// inventory. Welcome renders only when no workspace is bound, so a recent row
// replaces nothing and needs no switch confirmation — the guarded switch is the
// project picker's job, because that is where a workspace exists to tear down.
import { beforeEach, describe, expect, test, vi } from "vitest";

import { renderWelcomeCenter, type WelcomeRecentWork } from "../src/components/welcome_center";
import type { RecentProjectView, RecentSessionView } from "../src/models/recent_work";

const NOW = Date.UTC(2026, 0, 1);
const EPOCH = Math.floor(NOW / 1000);

const PROJECTS: RecentProjectView[] = [
  {
    canonicalRoot: "/workspace/spatial-lm",
    displayName: "spatial-lm",
    lastUpdatedAt: EPOCH - 3 * 60 * 60,
    latestSessionId: "session-a",
  },
  {
    canonicalRoot: "/workspace/arm-ctrl",
    displayName: "arm-ctrl",
    lastUpdatedAt: EPOCH - 9 * 24 * 60 * 60,
    latestSessionId: "session-c",
  },
];

const SESSIONS: RecentSessionView[] = [
  {
    canonicalRoot: "/workspace/spatial-lm",
    sessionId: "session-a",
    createdAt: EPOCH - 4 * 60 * 60,
    lastUpdatedAt: EPOCH - 3 * 60 * 60,
    messageCount: 4,
    toolCallCount: 1,
    commandCount: 0,
  },
  {
    canonicalRoot: "/workspace/spatial-lm",
    sessionId: "session-b",
    createdAt: EPOCH - 8 * 60 * 60,
    lastUpdatedAt: EPOCH - 7 * 60 * 60,
    messageCount: 2,
    toolCallCount: 0,
    commandCount: 1,
  },
];

function mount(recent?: Partial<WelcomeRecentWork>) {
  document.body.innerHTML = '<main id="app"></main>';
  const root = document.querySelector<HTMLElement>("#app")!;
  const onOpenRecent = vi.fn();
  renderWelcomeCenter(
    root,
    "en",
    vi.fn(),
    recent === undefined
      ? undefined
      : {
          state: { kind: "loaded", projects: PROJECTS, diagnostics: [] },
          sessions: SESSIONS,
          now: NOW,
          onOpenRecent,
          ...recent,
        },
  );
  return { root, onOpenRecent };
}

describe("welcome recent projects", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("renders one row per Core project with its age and session count", () => {
    const { root } = mount({});

    const rows = Array.from(root.querySelectorAll<HTMLElement>("[data-recent-project]"));
    // Core's order is preserved; the client never re-sorts the answer.
    expect(rows.map((row) => row.dataset.recentProject)).toEqual([
      "/workspace/spatial-lm",
      "/workspace/arm-ctrl",
    ]);
    expect(rows[0]?.textContent).toContain("spatial-lm");
    expect(rows[0]?.textContent).toContain("3h ago");
    // Counted from the same bounded fact, never from a directory scan.
    expect(rows[0]?.textContent).toContain("2 sessions");
    expect(rows[1]?.textContent).toContain("1w ago");
    expect(rows[1]?.textContent).toContain("0 sessions");
  });

  test("clicking a recent project opens it directly — Welcome has no workspace to replace", () => {
    const { root, onOpenRecent } = mount({});

    root.querySelector<HTMLButtonElement>('[data-recent-project="/workspace/arm-ctrl"]')!.click();

    expect(onOpenRecent).toHaveBeenCalledWith("/workspace/arm-ctrl");
    expect(root.querySelector("[data-picker-confirm]")).toBeNull();
  });

  test("the four read states stay distinct", () => {
    const unbound = mount();
    expect(
      unbound.root.querySelector<HTMLElement>('[data-recent-state="unavailable"]')?.textContent,
    ).toContain("runtime.recent_work");

    const failed = mount({ state: { kind: "failed", reason: "Core refused the read" } });
    expect(
      failed.root.querySelector<HTMLElement>('[data-recent-state="failed"]')?.textContent,
    ).toContain("Core refused the read");

    const loading = mount({ state: { kind: "loading" } });
    expect(loading.root.querySelector('[data-recent-state="loading"]')).not.toBeNull();

    const empty = mount({ state: { kind: "loaded", projects: [], diagnostics: [] } });
    expect(
      empty.root.querySelector<HTMLElement>('[data-recent-state="empty"]')?.textContent,
    ).toBe("No recent projects yet");
    expect(empty.root.querySelector("[data-recent-project]")).toBeNull();
  });

  test("Core's inventory diagnostics render verbatim", () => {
    const { root } = mount({
      state: { kind: "loaded", projects: PROJECTS, diagnostics: ["recent.record_skipped"] },
    });
    expect(
      root.querySelector('[data-recent-diagnostic="recent.record_skipped"]')?.textContent,
    ).toBe("recent.record_skipped");
  });

  test("a rejected open renders the host's own words without losing the rows", async () => {
    const { root } = mount({
      onOpenRecent: vi.fn(() => Promise.reject(new Error("workspace is gone"))),
    });

    root.querySelector<HTMLButtonElement>('[data-recent-project="/workspace/spatial-lm"]')!
      .click();
    for (let hop = 0; hop < 6; hop += 1) await Promise.resolve();

    expect(root.querySelector("[data-open-project-error]")?.textContent).toContain(
      "workspace is gone",
    );
    expect(root.querySelectorAll("[data-recent-project]")).toHaveLength(2);
  });
});

/*
 * H2 hygiene — E1 defect 9.
 *
 * Welcome renders only while no workspace is bound, so "the host has no Core
 * adapter" is not a fault there: it is the first-run state, and the operator's
 * next move is to open a project. The recent-work read still fails with the
 * host's own sentence, and that sentence stays on screen as the diagnostic it
 * is — but it is not the headline, because it describes a cause the operator
 * did not create and cannot act on.
 */
describe("welcome recent work when no project is bound", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("a failed read leads with D1's own no-project vocabulary", () => {
    const { root } = mount({
      state: { kind: "failed", reason: "Error: Core adapter is not connected" },
    });
    const note = root.querySelector<HTMLElement>('[data-recent-state="failed"]');
    expect(note?.querySelector("strong")?.textContent).toBe("No project open");
    // The lead sentence names the operator's situation, not the host's wiring.
    expect(note?.querySelector("[data-recent-first-run]")?.textContent).toContain(
      "Recent projects appear once a project is open",
    );
    expect(note?.textContent).not.toContain("unavailable");
  });

  test("the host's own sentence stays visible as the diagnostic beneath it", () => {
    const { root } = mount({
      state: { kind: "failed", reason: "Error: Core adapter is not connected" },
    });
    expect(
      root.querySelector<HTMLElement>("[data-recent-failure-reason]")?.textContent,
    ).toContain("Core adapter is not connected");
  });

  test("an absent capability keeps its own sentence rather than the first-run one", () => {
    // Core answered the handshake and did not publish the inventory. That is a
    // fact about Core, not about whether a project is open.
    const { root } = mount({
      state: { kind: "unavailable", reason: "Core has not published the runtime.recent_work inventory." },
    });
    const note = root.querySelector<HTMLElement>('[data-recent-state="unavailable"]');
    expect(note?.querySelector("strong")?.textContent).toBe("Recent project history is unavailable");
    expect(note?.querySelector("[data-recent-first-run]")).toBeNull();
  });

  test("the first-run wording is localized", () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    renderWelcomeCenter(root, "zh-CN", vi.fn(), {
      state: { kind: "failed", reason: "Core adapter is not connected" },
      sessions: [],
      now: NOW,
      onOpenRecent: vi.fn(),
    });
    const note = root.querySelector<HTMLElement>('[data-recent-state="failed"]');
    expect(note?.querySelector("strong")?.textContent).toBe("尚未打开项目");
  });
});
