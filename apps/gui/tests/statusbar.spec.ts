// @vitest-environment jsdom

// The cockpit statusbar renders only host-projected Core facts. Absent facts
// render an explicit em-dash, the event segment is a stream position rather
// than an invented counter, and the pending-gate segment is the bar's only
// interactive element.
import { describe, expect, test, vi } from "vitest";

import { formatCompactCount, renderStatusbar } from "../src/components/statusbar";
import type { D1StatusbarProjection } from "../src/models/workspace";
import { renderD1Cockpit } from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

const FULL: D1StatusbarProjection = {
  workMode: "build",
  permissionLevel: "ask",
  context: { usedTokens: 42_100, hardTokenLimit: 128_000, exceeded: false },
  eventStreamPosition: 9,
  lane: { laneId: "L1", agentId: "codex-acp", status: "running", progress: 64 },
  latency: { lastLatencyMs: 840, averageLatencyMs: 1100 },
  tokens: { inputTokens: 42_100, outputTokens: 8_900 },
  diagnosticsCount: 1,
  requests: { requestCount: 128, errorCount: 0 },
  pendingDecisionCount: 1,
};

const EMPTY: D1StatusbarProjection = {
  workMode: "—",
  permissionLevel: "—",
  context: null,
  eventStreamPosition: 0,
  lane: null,
  latency: null,
  tokens: null,
  diagnosticsCount: 0,
  requests: null,
  pendingDecisionCount: 0,
};

function segmentText(bar: HTMLElement, id: string): string {
  return bar.querySelector(`[data-sb-segment="${id}"]`)?.textContent ?? "";
}

describe("statusbar", () => {
  test("renders every segment from published facts in the design vocabulary", () => {
    const bar = renderStatusbar(FULL, "en");

    expect(bar.dataset.shellLandmark).toBe("statusbar");
    expect(segmentText(bar, "mode")).toBe("MODE build");
    expect(segmentText(bar, "perm")).toBe("PERM ask");
    expect(segmentText(bar, "context")).toBe("CONTEXT 42.1k / 128k 33%");
    expect(segmentText(bar, "events")).toBe("EVENTS #9");
    expect(segmentText(bar, "lane")).toBe("LANE L1 codex-acp running 64%");
    expect(segmentText(bar, "latency")).toBe("LATENCY 840ms avg 1.1s");
    expect(segmentText(bar, "tokens")).toBe("TOKENS 42.1k↑ 8.9k↓");
    expect(segmentText(bar, "diag")).toBe("DIAG 1✕");
    expect(segmentText(bar, "req")).toBe("REQ 128 req / 0 err");
  });

  test("absent facts render an explicit em-dash, never a fabricated number", () => {
    const bar = renderStatusbar(EMPTY, "en");
    for (const id of ["context", "lane", "latency", "tokens", "req"]) {
      expect(segmentText(bar, id)).toContain("—");
    }
    expect(bar.querySelector("[data-sb-gate]")).toBeNull();
  });

  test("the event segment is titled as a stream position, not an event count", () => {
    const bar = renderStatusbar(FULL, "en");
    const events = bar.querySelector<HTMLElement>('[data-sb-segment="events"]');
    expect(events?.title).toContain("stream position");
    expect(events?.title).toContain("not an event count");
  });

  test("the pending-gate segment is a button that navigates to D2 and nothing else", () => {
    const onNavigate = vi.fn();
    const bar = renderStatusbar(FULL, "en", onNavigate);
    const gate = bar.querySelector<HTMLButtonElement>("[data-sb-gate]");
    // G7 made this the D2 queue's own total, printed in the queue's own words,
    // so the chip and the header the click lands on read the same sentence.
    expect(gate?.textContent).toContain("1 awaiting you");
    gate?.click();
    expect(onNavigate).toHaveBeenCalledExactlyOnceWith("d2");
    // Every other segment is inert text.
    expect(bar.querySelectorAll("button")).toHaveLength(1);
  });

  test("without a navigation handler the gate segment stays visible but disabled", () => {
    const bar = renderStatusbar(FULL, "en");
    expect(bar.querySelector<HTMLButtonElement>("[data-sb-gate]")?.disabled).toBe(true);
  });

  test("compact counts follow the terminal vocabulary", () => {
    expect(formatCompactCount(0)).toBe("0");
    expect(formatCompactCount(999)).toBe("999");
    expect(formatCompactCount(1_000)).toBe("1k");
    expect(formatCompactCount(42_100)).toBe("42.1k");
    expect(formatCompactCount(128_000)).toBe("128k");
  });

  test("the cockpit mounts the statusbar as the bottom shell landmark", () => {
    document.body.innerHTML = '<main id="app"></main>';
    const root = document.querySelector<HTMLElement>("#app")!;
    const idle = {
      projection: D1_PROJECTION,
      pendingCommandId: null,
      outcome: { state: "idle" as const, reason: null },
    };
    const onNavigate = vi.fn();
    const projection = {
      ...D1_PROJECTION,
      statusbar: { ...D1_PROJECTION.statusbar, pendingDecisionCount: 2 },
    };
    const controller = renderD1Cockpit(
      root,
      projection,
      vi.fn(async () => idle),
      vi.fn(async () => idle),
      undefined,
      undefined,
      { poll: false, onNavigate },
    );

    const frame = root.querySelector('[data-screen="d1-cockpit"]');
    expect(frame?.lastElementChild?.getAttribute("data-shell-landmark")).toBe("statusbar");
    expect(root.querySelector('[data-sb-segment="mode"]')?.textContent).toContain("build");

    root.querySelector<HTMLButtonElement>("[data-sb-gate]")!.click();
    expect(onNavigate).toHaveBeenCalledExactlyOnceWith("d2");
    controller.dispose();
  });
});
