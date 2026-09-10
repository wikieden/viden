// @vitest-environment jsdom

/**
 * The EvidenceView in-cockpit view (`runtime.evidence_reads`, GUI-CORE-025).
 *
 * The rules under test are the ones the evidence-reads contract states and a
 * plausible client breaks: Core's order survives with the undated group first,
 * the opaque cursor drives paging, `complete` and "there may be more" are
 * different sentences, the four unavailable reasons never share a sentence,
 * `HashMismatch` never ships a body, an absent capability is not an empty
 * archive, a recording marks the list stale without reloading it, and the
 * search box says it only sees the loaded rows.
 */

import { beforeEach, describe, expect, test, vi } from "vitest";

import {
  ABSENT_EVIDENCE_CONTENT,
  PENDING_EVIDENCE_ARCHIVE,
  evidenceKindChips,
  groupEvidenceByDay,
  type EvidenceArchiveProjection,
  type EvidenceContentProjection,
  type EvidenceRowProjection,
} from "../src/models/evidence";
import { renderD1Cockpit } from "../src/screens/d1_cockpit";
import { renderEvidenceView } from "../src/screens/evidence_view";
import { D1_PROJECTION } from "./support/d1_projection";

/// `2026-01-02T10:30:00Z` and `2026-01-03T11:45:00Z`, far enough apart that
/// they land in different local days in every zone the suite runs in.
const DAY_ONE = Date.parse("2026-01-02T10:30:00Z") / 1000;
const DAY_TWO = Date.parse("2026-01-05T11:45:00Z") / 1000;

function row(overrides: Partial<EvidenceRowProjection> = {}): EvidenceRowProjection {
  return {
    id: "evidence_alpha_patch",
    kind: "patch",
    summary: "canonical patch for the diff module",
    path: "crates/types/src/evidence_reads.rs",
    source: "lane_evidence_reads",
    timestamp: DAY_ONE,
    ownerLaneId: "lane_evidence_reads",
    ownerTaskId: "task_evidence_reads",
    canonical: {
      itemId: "item_evidence_alpha",
      bundleId: "bundle_evidence_reads",
      sourceHash: "a".repeat(64),
      producerIdentity: "lane_evidence_reads",
      producerRole: "coder",
      producerTaskId: "task_evidence_reads",
      verification: "verified",
      quality: "pass",
    },
    metadata: [{ key: "command", value: "cargo test -p viden-types" }],
    ...overrides,
  };
}

function archive(
  overrides: Partial<EvidenceArchiveProjection> = {},
): EvidenceArchiveProjection {
  return {
    ...PENDING_EVIDENCE_ARCHIVE,
    outcome: { state: "confirmed", reason: null },
    rows: [row()],
    loaded: true,
    complete: true,
    ...overrides,
  };
}

function content(
  overrides: Partial<EvidenceContentProjection> = {},
): EvidenceContentProjection {
  return {
    ...ABSENT_EVIDENCE_CONTENT,
    outcome: { state: "confirmed", reason: null },
    evidenceId: "evidence_alpha_patch",
    ...overrides,
  };
}

let host: HTMLElement;

beforeEach(() => {
  document.body.replaceChildren();
  host = document.createElement("div");
  document.body.append(host);
});

describe("EvidenceView list", () => {
  test("renders the registered family and keeps Core's order with the undated group first", () => {
    renderEvidenceView(
      host,
      archive({
        rows: [
          row({ id: "evidence_undated", kind: "task_summary", timestamp: null }),
          row({ id: "evidence_day_one", timestamp: DAY_ONE }),
          row({ id: "evidence_day_two", kind: "test_result", timestamp: DAY_TWO }),
        ],
      }),
      "en",
    );

    expect(host.querySelector(".evwrap > .evmain > .evbar")).not.toBeNull();
    expect(host.querySelector(".evwrap > .evmain > .evscroll")).not.toBeNull();
    expect(host.querySelector(".evwrap > .evdet")).not.toBeNull();

    const days = [...host.querySelectorAll("[data-evidence-day]")].map(
      (node) => (node as HTMLElement).dataset.evidenceDay,
    );
    expect(days[0]).toBe("undated");
    expect(days).toHaveLength(3);

    const rows = [...host.querySelectorAll("[data-evidence-row]")].map(
      (node) => (node as HTMLElement).dataset.evidenceRow,
    );
    expect(rows).toEqual(["evidence_undated", "evidence_day_one", "evidence_day_two"]);
  });

  test("an undated row shows a dash rather than a fabricated time", () => {
    renderEvidenceView(
      host,
      archive({ rows: [row({ id: "evidence_undated", timestamp: null })] }),
      "en",
    );
    expect(host.querySelector(".evrow2 .tm2")?.textContent).toBe("—");
  });

  test("a row Core could not attribute carries no Lane chip", () => {
    renderEvidenceView(host, archive({ rows: [row({ ownerLaneId: null })] }), "en");
    expect(host.querySelector(".evrow2 .lane-b")).toBeNull();
  });

  test("the chip bar groups unknown kinds last and never hides them", () => {
    renderEvidenceView(
      host,
      archive({
        rows: [row({ id: "a", kind: "screenshot" }), row({ id: "b", kind: "patch" })],
      }),
      "en",
    );
    const chips = [...host.querySelectorAll("[data-evidence-kind-chip]")].map(
      (node) => (node as HTMLElement).dataset.evidenceKindChip,
    );
    expect(chips[0]).toBe("all");
    expect(chips).toContain("screenshot");
    expect(chips.indexOf("screenshot")).toBeGreaterThan(chips.indexOf("release_artifact"));
  });

  test("a kind chip sends a new Core query rather than filtering locally", () => {
    const onKindsChange = vi.fn();
    renderEvidenceView(host, archive(), "en", { kinds: [], onKindsChange });
    host.querySelector<HTMLButtonElement>("[data-evidence-kind-chip='patch']")!.click();
    expect(onKindsChange).toHaveBeenCalledWith(["patch"]);
  });

  test("the search box states that it only sees the loaded rows", () => {
    renderEvidenceView(host, archive({ rows: [row(), row({ id: "b" })] }), "en", {
      onSearchChange: () => undefined,
    });
    const box = host.querySelector<HTMLInputElement>("[data-evidence-search]")!;
    expect(box.title).toContain("2");
    expect(box.title).toContain("no evidence search");
  });

  test("a search that hides every loaded row is not drawn as an empty archive", () => {
    renderEvidenceView(host, archive(), "en", { search: "nothing-matches-this" });
    expect(host.querySelector("[data-evidence-state='search-empty']")).not.toBeNull();
    expect(host.querySelector("[data-evidence-state='empty']")).toBeNull();
  });

  test("an answered archive with zero rows is the only state drawn as 'no evidence'", () => {
    renderEvidenceView(host, archive({ rows: [] }), "en");
    const note = host.querySelector("[data-evidence-state='empty']");
    expect(note?.textContent).toBe("No evidence in this scope.");
  });

  test("a read with no answer yet says so instead of showing an empty archive", () => {
    renderEvidenceView(host, { ...PENDING_EVIDENCE_ARCHIVE }, "en");
    expect(host.querySelector("[data-evidence-state='pending']")).not.toBeNull();
    expect(host.querySelector("[data-evidence-state='empty']")).toBeNull();
  });

  test("an absent capability names it and denies being an empty archive", () => {
    renderEvidenceView(
      host,
      archive({ capabilityAvailable: false, loaded: false, rows: [] }),
      "en",
    );
    const note = host.querySelector("[data-evidence-state='unavailable']");
    expect(note?.textContent).toContain("runtime.evidence_reads");
    expect(note?.textContent).toContain("not an empty archive");
  });

  test("a refusal renders Core's own words in a role=alert and loads nothing", () => {
    renderEvidenceView(
      host,
      archive({
        outcome: {
          state: "rejected",
          reason: "evidence query kinds exceed the 32 entry bound: 33 requested",
        },
        loaded: false,
        rows: [],
      }),
      "en",
    );
    const note = host.querySelector("[data-evidence-state='rejected']");
    expect(note?.getAttribute("role")).toBe("alert");
    expect(note?.textContent).toContain("32 entry bound");
  });
});

describe("EvidenceView paging", () => {
  test("an incomplete page offers Load older and passes Core's cursor back", () => {
    const onLoadOlder = vi.fn();
    renderEvidenceView(
      host,
      archive({ complete: false, nextAfter: "t:1700000200:evidence_bravo_tests" }),
      "en",
      { onLoadOlder },
    );
    const older = host.querySelector<HTMLButtonElement>("[data-evidence-load-older]")!;
    expect(older.disabled).toBe(false);
    older.click();
    expect(onLoadOlder).toHaveBeenCalledTimes(1);
  });

  test("no cursor disables the control rather than letting the client build one", () => {
    renderEvidenceView(host, archive({ complete: false, nextAfter: null }), "en", {
      onLoadOlder: () => undefined,
    });
    expect(
      host.querySelector<HTMLButtonElement>("[data-evidence-load-older]")!.disabled,
    ).toBe(true);
  });

  test("a complete archive states so instead of hiding the control silently", () => {
    renderEvidenceView(host, archive({ complete: true, nextAfter: null }), "en", {
      onLoadOlder: () => undefined,
    });
    expect(host.querySelector("[data-evidence-load-older]")).toBeNull();
    expect(host.querySelector("[data-evidence-complete]")?.textContent).toContain(
      "Archive complete",
    );
  });
});

describe("EvidenceView staleness", () => {
  test("a stale list keeps its rows and offers a Refresh", () => {
    const onRefresh = vi.fn();
    renderEvidenceView(host, archive({ stale: true }), "en", { onRefresh });
    expect(host.querySelectorAll("[data-evidence-row]")).toHaveLength(1);
    const banner = host.querySelector("[data-evidence-stale]");
    expect(banner?.getAttribute("role")).toBe("status");
    host.querySelector<HTMLButtonElement>("[data-evidence-stale-refresh]")!.click();
    expect(onRefresh).toHaveBeenCalledTimes(1);
  });
});

describe("EvidenceView detail rail", () => {
  test("the report renders Core's fields and marks the ones Core did not record", () => {
    renderEvidenceView(host, archive({ rows: [row({ path: null, source: null })] }), "en");
    const facts = [...host.querySelectorAll("[data-evidence-section='report'] .evkv")].map(
      (node) => node.textContent,
    );
    expect(facts.some((text) => text?.includes("item_evidence_alpha"))).toBe(true);
    expect(facts.some((text) => text?.includes("a".repeat(64)))).toBe(true);
    expect(facts.filter((text) => text?.includes("Core recorded none"))).toHaveLength(2);
  });

  test("the report states Core's verification and quality verdicts on the bytes", () => {
    renderEvidenceView(host, archive(), "en");
    const facts = [...host.querySelectorAll("[data-evidence-section='report'] .evkv")].map(
      (node) => node.textContent,
    );
    expect(facts.some((text) => text?.includes("verified"))).toBe(true);
    expect(facts.some((text) => text?.includes("pass"))).toBe(true);
  });

  test("a distrusted reference states both verdicts instead of reading as verified", () => {
    renderEvidenceView(
      host,
      archive({
        rows: [
          row({
            canonical: {
              ...row().canonical!,
              verification: "failed",
              quality: "warn",
            },
          }),
        ],
      }),
      "en",
    );
    const facts = [...host.querySelectorAll("[data-evidence-section='report'] .evkv")].map(
      (node) => node.textContent,
    );
    expect(facts.some((text) => text?.includes("failed"))).toBe(true);
    expect(facts.some((text) => text?.includes("warn"))).toBe(true);
    // The bytes are still Core's; a failed verdict is not an absent reference.
    expect(host.querySelector("[data-evidence-no-canonical]")).toBeNull();
  });

  test("a row with no canonical reference says so rather than showing empty fields", () => {
    renderEvidenceView(host, archive({ rows: [row({ canonical: null })] }), "en");
    expect(host.querySelector("[data-evidence-no-canonical]")?.textContent).toContain(
      "names no canonical bytes",
    );
  });

  test("metadata renders as facts under a note saying nothing is interpreted", () => {
    renderEvidenceView(
      host,
      archive({
        rows: [
          row({
            metadata: [
              { key: "command", value: "cargo test" },
              { key: "exit", value: "101" },
            ],
          }),
        ],
      }),
      "en",
    );
    expect(host.querySelector("[data-evidence-metadata-note]")?.textContent).toContain(
      "Nothing here is interpreted",
    );
    const facts = [...host.querySelectorAll("[data-evidence-section='metadata'] .evkv")];
    expect(facts).toHaveLength(2);
    expect(facts[1]?.textContent).toContain("101");
  });

  test("selecting a row reports the id so the caller can send one content read", () => {
    const onSelect = vi.fn();
    renderEvidenceView(host, archive({ rows: [row(), row({ id: "b" })] }), "en", { onSelect });
    host.querySelector<HTMLButtonElement>("[data-evidence-row='b']")!.click();
    expect(onSelect).toHaveBeenCalledWith("b");
  });

  test("text content renders with its bound and the hash Core verified it against", () => {
    renderEvidenceView(host, archive(), "en", {
      content: content({
        kind: "text",
        text: "running 3 tests\ntest result: ok",
        truncated: true,
        sha256: "b".repeat(64),
      }),
    });
    expect(host.querySelector("[data-evidence-content='text']")?.textContent).toContain(
      "3 tests",
    );
    expect(host.querySelector("[data-evidence-state='content-truncated']")?.textContent).toContain(
      "256 KiB",
    );
    expect(host.querySelector(`[data-evidence-content-hash='${"b".repeat(64)}']`)).not.toBeNull();
  });

  test("patch content renders through the shared diff rows, not as text", () => {
    renderEvidenceView(host, archive(), "en", {
      content: content({
        kind: "diff",
        sha256: "a".repeat(64),
        document: {
          truncated: false,
          byteLimit: 262_144,
          files: [
            {
              path: "crates/types/src/evidence_reads.rs",
              oldPath: null,
              kind: "modified",
              binary: false,
              omitted: false,
              additions: 1,
              deletions: 1,
              hunks: [
                {
                  oldStart: 12,
                  oldLines: 3,
                  newStart: 12,
                  newLines: 3,
                  header: "impl EvidenceQuery",
                  lines: [
                    {
                      kind: "removed",
                      content: "        self.limit as usize",
                      oldLine: 13,
                      newLine: null,
                    },
                    {
                      kind: "added",
                      content: "        self.limit.clamp(1, 200) as usize",
                      oldLine: null,
                      newLine: 13,
                    },
                  ],
                },
              ],
            },
          ],
        },
      }),
    });
    const body = host.querySelector("[data-evidence-content='diff']")!;
    expect(body.querySelector(".diffbody")).not.toBeNull();
    expect(body.textContent).toContain("@@ -12,3 +12,3 @@");
    expect(body.textContent).toContain("self.limit.clamp(1, 200)");
  });

  test.each([
    ["summary_only", "Display-only evidence"],
    ["missing_canonical_bytes", "no longer holds"],
    ["hash_mismatch", "failed verification"],
    ["binary", "not text"],
  ])("the %s reason keeps its own sentence and ships no body", (reason, phrase) => {
    renderEvidenceView(host, archive(), "en", {
      content: content({ kind: "unavailable", reason }),
    });
    const note = host.querySelector(`[data-evidence-state='unavailable-${reason}']`);
    expect(note?.textContent).toContain(phrase);
    expect(host.querySelector("[data-evidence-content='text']")).toBeNull();
    expect(host.querySelector("[data-evidence-content='diff']")).toBeNull();
  });

  test("a reason this build cannot name is stated rather than folded into a known one", () => {
    renderEvidenceView(host, archive(), "en", {
      content: content({ kind: "unavailable", reason: "quarantined" }),
    });
    expect(
      host.querySelector("[data-evidence-state='unavailable-quarantined']")?.textContent,
    ).toContain("quarantined");
  });

  test("a content shape this build cannot name is stated rather than left blank", () => {
    renderEvidenceView(host, archive(), "en", { content: content({ kind: "audio" }) });
    expect(host.querySelector("[data-evidence-state='content-unknown']")?.textContent).toContain(
      "audio",
    );
  });

  test("a refused content read carries Core's own words in a role=alert", () => {
    renderEvidenceView(host, archive(), "en", {
      content: content({
        outcome: { state: "rejected", reason: "evidence `x` is not an evidence id Core recorded" },
      }),
    });
    const note = host.querySelector("[data-evidence-state='content-rejected']");
    expect(note?.getAttribute("role")).toBe("alert");
    expect(note?.textContent).toContain("not an evidence id Core recorded");
  });

  test("an answer for another row is never rendered under this one", () => {
    renderEvidenceView(host, archive(), "en", {
      content: content({ evidenceId: "some-other-row", kind: "text", text: "not mine" }),
    });
    expect(host.querySelector("[data-evidence-content='text']")).toBeNull();
    expect(host.querySelector("[data-evidence-state='content-absent']")).not.toBeNull();
  });
});

describe("EvidenceView footer", () => {
  test("Open in review is offered for a patch and states what it actually opens", () => {
    const onOpenReview = vi.fn();
    renderEvidenceView(host, archive(), "en", { onOpenReview, reviewAvailable: true });
    const button = host.querySelector<HTMLButtonElement>("[data-evidence-open-review]")!;
    expect(button.disabled).toBe(false);
    expect(button.title).toContain("current working tree");
    button.click();
    expect(onOpenReview).toHaveBeenCalledTimes(1);
  });

  test.each<[Partial<EvidenceRowProjection>, boolean, string]>([
    [{ kind: "test_result" }, true, "Only a patch entry"],
    [{}, false, "no structured diff"],
  ])(
    "a blocked Open in review stays visible and names its own reason",
    (rowOverride, reviewAvailable, phrase) => {
      renderEvidenceView(host, archive({ rows: [row(rowOverride)] }), "en", {
        onOpenReview: () => undefined,
        reviewAvailable,
      });
      const button = host.querySelector<HTMLButtonElement>("[data-evidence-open-review]")!;
      expect(button.disabled).toBe(true);
      expect(button.title).toContain(phrase);
    },
  );

  test("Open audit trail routes the evidence object, never a fabricated audit id", () => {
    const onOpenAuditTrail = vi.fn();
    renderEvidenceView(host, archive(), "en", { onOpenAuditTrail });
    const button = host.querySelector<HTMLButtonElement>("[data-evidence-open-audit]")!;
    expect(button.dataset.evidenceOpenAudit).toBe("evidence:evidence_alpha_patch");
    expect(button.title).toContain("never the reverse");
    button.click();
    expect(onOpenAuditTrail).toHaveBeenCalledWith({
      kind: "evidence",
      id: "evidence_alpha_patch",
    });
  });

  test("linked chips name the Core objects the row carries and nothing else", () => {
    renderEvidenceView(host, archive(), "en");
    const chips = [...host.querySelectorAll("[data-evidence-chip]")].map(
      (node) => (node as HTMLElement).dataset.evidenceChip,
    );
    expect(chips).toEqual(["canonical", "path", "task"]);
  });

  test("a row naming no other Core object says so instead of showing an empty section", () => {
    renderEvidenceView(
      host,
      archive({ rows: [row({ canonical: null, path: null, ownerTaskId: null })] }),
      "en",
    );
    expect(host.querySelector("[data-evidence-no-links]")).not.toBeNull();
  });
});

describe("EvidenceView model helpers", () => {
  test("day grouping preserves Core's order and never sorts", () => {
    const groups = groupEvidenceByDay([
      row({ id: "a", timestamp: null }),
      row({ id: "b", timestamp: DAY_TWO }),
      row({ id: "c", timestamp: DAY_ONE }),
    ]);
    expect(groups.map((group) => group.rows.map((entry) => entry.id))).toEqual([
      ["a"],
      ["b"],
      ["c"],
    ]);
    expect(groups[0]?.day).toBeNull();
  });

  test("consecutive rows in the same local day share one group", () => {
    const groups = groupEvidenceByDay([
      row({ id: "a", timestamp: DAY_ONE }),
      row({ id: "b", timestamp: DAY_ONE + 60 }),
    ]);
    expect(groups).toHaveLength(1);
    expect(groups[0]?.rows).toHaveLength(2);
  });

  test("the chip vocabulary always leads with the five first-class kinds", () => {
    expect(evidenceKindChips([])).toEqual([
      "patch",
      "test_result",
      "review",
      "doc_update",
      "release_artifact",
    ]);
  });
});

/**
 * The cockpit entry points.
 *
 * `D-RAILNAV` keeps the activity rail a router to the standalone D-screens, so
 * EvidenceView is reached the way DiffReview is: a palette action and a chord,
 * both failing closed on the capability rather than opening a view that can
 * never fill.
 */
describe("EvidenceView cockpit entry points", () => {
  const flush = async (): Promise<void> => {
    for (let tick = 0; tick < 4; tick += 1) {
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
  };

  async function mount(capabilityAvailable = true): Promise<{
    root: HTMLElement;
    queries: Array<{ laneId: string | null; kinds: string[] }>;
    dispose: () => void;
  }> {
    const root = document.createElement("div");
    document.body.replaceChildren(root);
    const projection = structuredClone(D1_PROJECTION);
    const queries: Array<{ laneId: string | null; kinds: string[] }> = [];
    const answer = archive({ capabilityAvailable, loaded: capabilityAvailable });
    const controller = renderD1Cockpit(
      root,
      projection,
      async () => ({
        projection,
        pendingCommandId: null,
        outcome: { state: "idle", reason: null },
      }),
      async () => ({
        projection,
        pendingCommandId: null,
        outcome: { state: "idle", reason: null },
      }),
      undefined,
      undefined,
      {
        poll: false,
        showWelcome: false,
        onNavigate: () => undefined,
        evidence: {
          read: async () => ({ ...PENDING_EVIDENCE_ARCHIVE, capabilityAvailable, loaded: false }),
          query: async (laneId, kinds) => {
            queries.push({ laneId, kinds });
            return answer;
          },
          loadOlder: async () => answer,
          content: async () => content({ kind: "text", text: "ok", sha256: "a".repeat(64) }),
        },
      },
    );
    await flush();
    return { root, queries, dispose: () => controller.dispose() };
  }

  test("the palette offers Open evidence with the ⌘E hint once Core publishes the archive", async () => {
    const { root, dispose } = await mount();
    root.querySelector<HTMLButtonElement>("[data-command-palette-toggle]")!.click();
    await flush();
    const item = root.querySelector<HTMLElement>(
      "[data-palette-row][data-palette-item-id='action:open-evidence']",
    );
    expect(item).not.toBeNull();
    expect(item?.textContent).toContain("⌘E");
    dispose();
  });

  test("without the capability the palette row stays visible, disabled, and names it", async () => {
    const { root, dispose } = await mount(false);
    root.querySelector<HTMLButtonElement>("[data-command-palette-toggle]")!.click();
    await flush();
    const item = root.querySelector<HTMLElement>(
      "[data-palette-row][data-palette-item-id='action:open-evidence']",
    );
    expect(item).not.toBeNull();
    expect(item?.textContent).toContain("runtime.evidence_reads");
    dispose();
  });

  test("⌘E opens the view and scopes the first page to the selected Lane", async () => {
    const { root, queries, dispose } = await mount();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "e", metaKey: true }));
    await flush();
    expect(root.querySelector("[data-evidence-view]")).not.toBeNull();
    // The scope follows the cockpit's Lane selection, exactly as DiffReview's
    // target does: a selected Lane reads that Lane's evidence, and no
    // selection reads the whole archive.
    expect(queries).toEqual([{ laneId: "lane-core", kinds: [] }]);
    dispose();
  });

  test("⌘E toggles back to the transcript rather than navigating away", async () => {
    const { root, dispose } = await mount();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "e", metaKey: true }));
    await flush();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "e", metaKey: true }));
    await flush();
    expect(root.querySelector("[data-evidence-view]")).toBeNull();
    expect(root.querySelector("[data-center-sequence]")).not.toBeNull();
    dispose();
  });

  test("⌘E stands down while Core publishes no archive", async () => {
    const { root, queries, dispose } = await mount(false);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "e", metaKey: true }));
    await flush();
    expect(root.querySelector("[data-evidence-view]")).toBeNull();
    expect(queries).toEqual([]);
    dispose();
  });

  test("⌘E stands down while the permission dock owns focus", async () => {
    // The dock binds a *bare* `e` to Edit and never inspects modifiers, so a
    // `⌘E` pressed there would otherwise reach both handlers. A decision the
    // operator is being asked to make outranks opening a read-only archive.
    const { root, queries, dispose } = await mount();
    const dock = document.createElement("div");
    dock.dataset.permissionDock = "true";
    const focusable = document.createElement("button");
    dock.append(focusable);
    root.append(dock);
    focusable.focus();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "e", metaKey: true }));
    await flush();
    expect(root.querySelector("[data-evidence-view]")).toBeNull();
    expect(queries).toEqual([]);
    dispose();
  });

  test("selecting a row reads its content once and caches it for this view", async () => {
    const { root, dispose } = await mount();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "e", metaKey: true }));
    await flush();
    root.querySelector<HTMLButtonElement>("[data-evidence-row='evidence_alpha_patch']")!.click();
    await flush();
    expect(root.querySelector("[data-evidence-content='text']")?.textContent).toContain("ok");
    dispose();
  });
});
