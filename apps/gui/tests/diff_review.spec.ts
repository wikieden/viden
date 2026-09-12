// @vitest-environment jsdom

/**
 * The DiffReview in-cockpit view (`runtime.structured_diff`, GUI-CORE-012).
 *
 * The rules under test are the honesty rules the source-control contract
 * states: `omitted`, `binary`, and `diff: null` each get their own row and
 * none of them renders as "unchanged"; a truncated page carries a banner; and
 * an empty file list is only ever drawn as "no changes" when Core actually
 * answered with zero entries.
 */

import { beforeEach, describe, expect, test, vi } from "vitest";

import type { WorkspaceDiffProjection } from "../src/models/diff_review";
import { renderDiffReview } from "../src/screens/diff_review";

const SOURCE = {
  status: "ready",
  branch: "claude/gui-diff-review",
  worktree: "/workspace/viden",
  ahead: 1,
  behind: 0,
  added: 2,
  deleted: 0,
  dirty: true,
};

function modifiedFile(path: string) {
  return {
    path,
    oldPath: null,
    kind: "modified",
    binary: false,
    omitted: false,
    additions: 1,
    deletions: 1,
    hunks: [
      {
        oldStart: 42,
        oldLines: 3,
        newStart: 42,
        newLines: 3,
        header: "pub struct DiffDocument {",
        lines: [
          {
            kind: "context",
            content: "    pub files: Vec<DiffFile>,",
            oldLine: 42,
            newLine: 42,
          },
          { kind: "removed", content: "    pub truncated: bool,", oldLine: 43, newLine: null },
          {
            kind: "added",
            content: "    pub truncated: bool, // bounded",
            oldLine: null,
            newLine: 43,
          },
        ],
      },
    ],
  };
}

function loadedPage(
  overrides: Partial<WorkspaceDiffProjection> = {},
): WorkspaceDiffProjection {
  return {
    outcome: { state: "confirmed", reason: null },
    targetLaneId: null,
    source: SOURCE,
    entries: [
      {
        path: "crates/types/src/diff.rs",
        index: "modified",
        worktree: null,
        staged: true,
        diff: modifiedFile("crates/types/src/diff.rs"),
      },
      {
        path: "crates/types/tests/fixtures/frontend-contract-v1/structured-diff.json",
        index: null,
        worktree: "added",
        staged: false,
        diff: {
          path: "crates/types/tests/fixtures/frontend-contract-v1/structured-diff.json",
          oldPath: null,
          kind: "added",
          binary: false,
          omitted: true,
          additions: 1284,
          deletions: 0,
          hunks: [],
        },
      },
    ],
    truncated: true,
    loaded: true,
    pendingCommandId: null,
    capabilityAvailable: true,
    stale: false,
    ...overrides,
  };
}

function mount(
  projection: WorkspaceDiffProjection,
  handlers: Parameters<typeof renderDiffReview>[3] = {},
): HTMLElement {
  const host = document.createElement("div");
  document.body.replaceChildren(host);
  renderDiffReview(host, projection, "en", handlers);
  return host;
}

beforeEach(() => {
  document.body.replaceChildren();
});

describe("DiffReview file tree", () => {
  test("renders the registered family markup with Core's own order", () => {
    const host = mount(loadedPage());
    expect(host.querySelector(".review > .filetree")).not.toBeNull();
    expect(host.querySelector(".review > .diffpane")).not.toBeNull();
    const rows = Array.from(host.querySelectorAll<HTMLElement>(".ftrow"));
    expect(rows.map((row) => row.dataset.path)).toEqual([
      "crates/types/src/diff.rs",
      "crates/types/tests/fixtures/frontend-contract-v1/structured-diff.json",
    ]);
  });

  test("shows the change kind glyph, per-file counts, and Core's staged mark", () => {
    const host = mount(loadedPage());
    const [first, second] = Array.from(host.querySelectorAll<HTMLElement>(".ftrow"));
    expect(first!.querySelector(".stat")?.textContent).toBe("M");
    expect(first!.querySelector(".stat")?.className).toContain("m");
    expect(first!.querySelector(".pm")?.textContent).toBe("+1 −1");
    expect(first!.querySelector(".ck")).not.toBeNull();
    expect(second!.querySelector(".stat")?.textContent).toBe("A");
    // Core derives `staged`; the client never re-derives it from `index`.
    expect(second!.querySelector(".ck")).toBeNull();
  });

  test("keeps the tail of a long path visible rather than cutting it off", () => {
    const host = mount(loadedPage());
    const second = Array.from(host.querySelectorAll<HTMLElement>(".ftrow"))[1]!;
    const name = second.querySelector<HTMLElement>(".fn")!;
    // The distinctive half is a separate, non-shrinking node; the leading
    // directories are the part allowed to ellipsize.
    expect(name.querySelector(".base")?.textContent).toBe("structured-diff.json");
    expect(name.title).toBe(
      "crates/types/tests/fixtures/frontend-contract-v1/structured-diff.json",
    );
  });

  test("marks the header totals partial when Core produced no diff for an entry", () => {
    const page = loadedPage({
      entries: [
        {
          path: "assets/logo.png",
          index: null,
          worktree: "modified",
          staged: false,
          diff: null,
        },
      ],
      truncated: false,
    });
    const host = mount(page);
    expect(host.querySelector<HTMLElement>("[data-review-count-partial]")).not.toBeNull();
  });
});

describe("DiffReview honesty rules", () => {
  test("an omitted file states the rows are not shown and keeps its real counts", () => {
    const host = mount(loadedPage());
    const rows = Array.from(host.querySelectorAll<HTMLElement>(".ftrow"));
    rows[1]!.click();
    const note = host.querySelector<HTMLElement>("[data-diff-note='omitted']");
    expect(note).not.toBeNull();
    expect(note!.textContent).toContain("1284");
    // Never the empty-diff sentence.
    expect(host.querySelector("[data-diff-note='no-diff']")).toBeNull();
  });

  test("a page truncated by the byte bound carries a banner", () => {
    const host = mount(loadedPage());
    expect(host.querySelector("[data-review-truncated]")).not.toBeNull();
  });

  test("a binary file states it has no rows rather than showing none", () => {
    const page = loadedPage({
      truncated: false,
      entries: [
        {
          path: "assets/logo.png",
          index: null,
          worktree: "modified",
          staged: false,
          diff: {
            path: "assets/logo.png",
            oldPath: null,
            kind: "modified",
            binary: true,
            omitted: false,
            additions: 0,
            deletions: 0,
            hunks: [],
          },
        },
      ],
    });
    const host = mount(page);
    expect(host.querySelector("[data-diff-note='binary']")).not.toBeNull();
  });

  test("an entry Core produced no diff for says so instead of unchanged", () => {
    const page = loadedPage({
      truncated: false,
      entries: [
        {
          path: "assets/logo.png",
          index: null,
          worktree: "modified",
          staged: false,
          diff: null,
        },
      ],
    });
    const host = mount(page);
    const note = host.querySelector<HTMLElement>("[data-diff-note='no-diff']");
    expect(note).not.toBeNull();
    // The sentence names Core as the producer and says outright that this is
    // not "unchanged"; the failure it guards against is an empty pane.
    expect(note!.textContent).toContain("Core produced no diff");
    expect(note!.textContent).toContain("does not mean it is unchanged");
    expect(host.querySelector("[data-review-state='empty']")).toBeNull();
  });

  test("an answered read with zero entries is the only state drawn as no changes", () => {
    const host = mount(
      loadedPage({ entries: [], truncated: false, loaded: true }),
    );
    expect(host.querySelector("[data-review-state='empty']")).not.toBeNull();
  });

  test("a read that has not answered says so rather than showing an empty tree", () => {
    const host = mount(
      loadedPage({
        entries: [],
        truncated: false,
        loaded: false,
        source: null,
        outcome: { state: "pending", reason: null },
        pendingCommandId: "gui-diff-1",
      }),
    );
    expect(host.querySelector("[data-review-state='pending']")).not.toBeNull();
    expect(host.querySelector("[data-review-state='empty']")).toBeNull();
  });

  test("a refused read renders Core's own words in an alert, never an empty tree", () => {
    const reason = "permission denied\ntool: git_diff\nreason: DenyRule";
    const host = mount(
      loadedPage({
        entries: [],
        truncated: false,
        loaded: false,
        source: null,
        outcome: { state: "rejected", reason },
      }),
    );
    const alert = host.querySelector<HTMLElement>("[data-review-state='rejected']");
    expect(alert).not.toBeNull();
    expect(alert!.getAttribute("role")).toBe("alert");
    expect(alert!.textContent).toContain("git_diff");
    expect(host.querySelector("[data-review-state='empty']")).toBeNull();
  });

  test("an absent capability names it and never renders as a clean tree", () => {
    const host = mount(
      loadedPage({
        entries: [],
        truncated: false,
        loaded: false,
        source: null,
        capabilityAvailable: false,
        outcome: { state: "idle", reason: null },
      }),
    );
    const note = host.querySelector<HTMLElement>("[data-review-state='unavailable']");
    expect(note).not.toBeNull();
    expect(note!.textContent).toContain("runtime.structured_diff");
    expect(host.querySelector("[data-review-state='empty']")).toBeNull();
  });
});

describe("DiffReview diff pane", () => {
  test("renders hunk, context, added, and removed rows with Git's own numbering", () => {
    const host = mount(loadedPage());
    const body = host.querySelector<HTMLElement>(".diffbody")!;
    const hunk = body.querySelector<HTMLElement>(".dl.hunk")!;
    expect(hunk.textContent).toContain("@@ -42,3 +42,3 @@");
    expect(hunk.textContent).toContain("pub struct DiffDocument {");

    const removed = body.querySelector<HTMLElement>(".dl.del")!;
    expect(removed.dataset.oldLine).toBe("43");
    // A removed line has no new-file position; the field is absent, not zero.
    expect(removed.dataset.newLine).toBeUndefined();
    expect(removed.querySelector(".ln")?.textContent).toBe("43");

    const added = body.querySelector<HTMLElement>(".dl.add")!;
    expect(added.dataset.newLine).toBe("43");
    expect(added.dataset.oldLine).toBeUndefined();

    const context = body.querySelector<HTMLElement>(".dl.ctx")!;
    expect(context.dataset.oldLine).toBe("42");
    expect(context.dataset.newLine).toBe("42");
  });

  test("Split stays visible and disabled because it is not built", () => {
    const host = mount(loadedPage());
    const split = host.querySelector<HTMLButtonElement>("[data-diff-view='split']")!;
    expect(split.disabled).toBe(true);
    expect(split.title.length).toBeGreaterThan(0);
    expect(
      host.querySelector<HTMLButtonElement>("[data-diff-view='unified']")!.classList,
    ).toContain("on");
  });

  test("a read-only mount keeps the commit bar visible and wholly inert", () => {
    // The action port is what makes the bar live (see
    // `diff_review_actions.spec.ts`). Without one — an unbound host — the
    // registered shape stays so the operator can see what is coming, and every
    // control is disabled rather than resolving to nothing.
    const host = mount(loadedPage());
    const bar = host.querySelector<HTMLElement>(".commitbar")!;
    expect(bar.dataset.reviewCommit).toBe("true");
    for (const control of Array.from(bar.querySelectorAll<HTMLButtonElement>("button"))) {
      expect(control.disabled).toBe(true);
    }
    expect(bar.querySelector<HTMLInputElement>("[data-review-commit-message]")?.disabled).toBe(
      true,
    );
  });
});

describe("DiffReview controls", () => {
  test("Refresh asks the shell to re-read and Close returns to the transcript", () => {
    const onRefresh = vi.fn();
    const onClose = vi.fn();
    const host = mount(loadedPage(), { onRefresh, onClose });
    host.querySelector<HTMLButtonElement>("[data-review-refresh]")!.click();
    expect(onRefresh).toHaveBeenCalledTimes(1);
    host.querySelector<HTMLButtonElement>("[data-review-close]")!.click();
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test("a stale page keeps its rows visible while it says a re-read is due", () => {
    const host = mount(loadedPage({ stale: true }));
    expect(host.querySelector("[data-review-stale]")).not.toBeNull();
    expect(host.querySelectorAll(".ftrow").length).toBe(2);
  });

  test("selecting a file marks it current and switches the pane", () => {
    const host = mount(loadedPage());
    const rows = Array.from(host.querySelectorAll<HTMLElement>(".ftrow"));
    expect(rows[0]!.classList).toContain("on");
    rows[1]!.click();
    const after = Array.from(host.querySelectorAll<HTMLElement>(".ftrow"));
    expect(after[1]!.classList).toContain("on");
    expect(host.querySelector<HTMLElement>(".dphead .fn")?.title).toBe(
      "crates/types/tests/fixtures/frontend-contract-v1/structured-diff.json",
    );
  });
});

/* ------------------------------------------------------------------ */
/* Cockpit integration: entry points, capability gate, re-query rule    */
/* ------------------------------------------------------------------ */

import { renderD1Cockpit, type D1CockpitProjection } from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

function cockpitProjection(): D1CockpitProjection {
  const projection = structuredClone(D1_PROJECTION) as D1CockpitProjection;
  // Core reports uncommitted work, which is what puts the titlebar's changes
  // marker on screen at all.
  projection.topbarSource = {
    project: "viden",
    branch: "claude/gui-diff-review",
    ahead: 1,
    behind: 0,
    dirty: true,
    status: "ready",
    truncated: false,
    laneWorktreeCount: 1,
  };
  return projection;
}

function mountCockpit(options: {
  read?: () => Promise<WorkspaceDiffProjection>;
  query?: (laneId: string | null) => Promise<WorkspaceDiffProjection>;
  bind?: boolean;
  /** Runs the cockpit's own drain timer, which is what drives the rule. */
  poll?: boolean;
}): { root: HTMLElement; dispose: () => void } {
  const root = document.createElement("div");
  document.body.replaceChildren(root);
  const projection = cockpitProjection();
  const controller = renderD1Cockpit(
    root,
    projection,
    async () => ({ projection, pendingCommandId: null, outcome: { state: "idle", reason: null } }),
    async () => ({ projection, pendingCommandId: null, outcome: { state: "idle", reason: null } }),
    undefined,
    undefined,
    {
      poll: options.poll === true,
      showWelcome: false,
      onNavigate: () => undefined,
      workspaceDiff:
        options.bind === false
          ? undefined
          : {
              read: options.read ?? (async () => loadedPage()),
              query: options.query ?? (async () => loadedPage()),
            },
    },
  );
  return { root, dispose: () => controller.dispose() };
}

const flush = async (): Promise<void> => {
  await new Promise((resolve) => setTimeout(resolve, 0));
  await new Promise((resolve) => setTimeout(resolve, 0));
};

describe("DiffReview entry points", () => {
  test("the titlebar changes marker opens the view in the centre pane", async () => {
    const mounted = mountCockpit({});
    await flush();
    const entry = mounted.root.querySelector<HTMLButtonElement>("[data-topbar-review]")!;
    expect(entry.disabled).toBe(false);
    entry.click();
    await flush();
    expect(mounted.root.querySelector("[data-diff-review]")).not.toBeNull();
    // The transcript is replaced, not stacked on.
    expect(mounted.root.querySelector("[data-center-sequence]")).toBeNull();
    mounted.dispose();
  });

  test("closing returns the centre pane to the transcript", async () => {
    const mounted = mountCockpit({});
    await flush();
    mounted.root.querySelector<HTMLButtonElement>("[data-topbar-review]")!.click();
    await flush();
    mounted.root.querySelector<HTMLButtonElement>("[data-review-close]")!.click();
    await flush();
    expect(mounted.root.querySelector("[data-diff-review]")).toBeNull();
    expect(mounted.root.querySelector("[data-center-sequence]")).not.toBeNull();
    mounted.dispose();
  });

  test("Cmd+R opens the view and prevents the webview reload", async () => {
    const mounted = mountCockpit({});
    await flush();
    const event = new KeyboardEvent("keydown", { key: "r", metaKey: true, cancelable: true });
    window.dispatchEvent(event);
    await flush();
    expect(event.defaultPrevented).toBe(true);
    expect(mounted.root.querySelector("[data-diff-review]")).not.toBeNull();
    mounted.dispose();
  });

  test("an absent capability disables the entry point and sends no query", async () => {
    const query = vi.fn(async () => loadedPage());
    const mounted = mountCockpit({
      read: async () =>
        loadedPage({
          capabilityAvailable: false,
          loaded: false,
          entries: [],
          truncated: false,
          source: null,
          outcome: { state: "idle", reason: null },
        }),
      query,
    });
    await flush();
    const entry = mounted.root.querySelector<HTMLButtonElement>("[data-topbar-review]")!;
    // Disabled, not hidden: an absent Core capability is a fact the operator
    // should be able to read off the chrome.
    expect(entry.disabled).toBe(true);
    expect(entry.title).toContain("runtime.structured_diff");
    const event = new KeyboardEvent("keydown", { key: "r", metaKey: true, cancelable: true });
    window.dispatchEvent(event);
    await flush();
    expect(event.defaultPrevented).toBe(false);
    expect(query).not.toHaveBeenCalled();
    mounted.dispose();
  });

  test("opening sends exactly one query for the cockpit's selected Lane", async () => {
    const targets: Array<string | null> = [];
    const query = vi.fn(async (laneId: string | null) => {
      targets.push(laneId);
      return loadedPage();
    });
    const mounted = mountCockpit({ query });
    await flush();
    mounted.root.querySelector<HTMLButtonElement>("[data-topbar-review]")!.click();
    await flush();
    expect(query).toHaveBeenCalledTimes(1);
    // Core resolves the Lane's worktree from the id; the client passes no path.
    expect(targets).toEqual([D1_PROJECTION.selectedLaneId]);
    mounted.dispose();
  });
});

describe("DiffReview re-query rule", () => {
  /**
   * The rule, stated once: while the view is open, every ordered Core wake or
   * poll cycle checks the host's no-traffic projection. If Core has published
   * a `WorkspaceSourceUpdated` or `WorkspaceChangeUpdated` since the page was
   * read, that projection comes back `stale`; the banner appears immediately
   * and one re-read fires after a 400 ms debounce — one read per burst of
   * writes, not one per event. A closed review checks staleness not at all.
   *
   * The context dock's Environment panel lists the same changed files, so
   * since `G5` exactly one `QueryWorkspaceDiff` is issued per target when the
   * cockpit mounts, and opening the review reuses that page rather than
   * shelling out to git again. What stays true is the discipline the rule
   * exists for: no repeat query and no staleness probe for a pane nobody is
   * looking at.
   */
  test("a stale projection shows the banner and re-reads once after the debounce", async () => {
    vi.useFakeTimers();
    try {
      let stale = false;
      const query = vi.fn(async () => {
        stale = false;
        return loadedPage();
      });
      const mounted = mountCockpit({
        read: async () => loadedPage({ stale }),
        query,
        poll: true,
      });
      await vi.advanceTimersByTimeAsync(10);
      mounted.root.querySelector<HTMLButtonElement>("[data-topbar-review]")!.click();
      await vi.advanceTimersByTimeAsync(10);
      expect(query).toHaveBeenCalledTimes(1);

      // Core publishes invalidating facts while the view is open.
      stale = true;
      await vi.advanceTimersByTimeAsync(300);
      expect(mounted.root.querySelector("[data-review-stale]")).not.toBeNull();
      // The rows the operator is reading stay on screen while it re-reads.
      expect(mounted.root.querySelectorAll(".ftrow").length).toBe(2);
      expect(query).toHaveBeenCalledTimes(1);

      await vi.advanceTimersByTimeAsync(500);
      expect(query).toHaveBeenCalledTimes(2);
      mounted.dispose();
    } finally {
      vi.useRealTimers();
    }
  });

  test("a closed review never checks staleness", async () => {
    vi.useFakeTimers();
    try {
      const read = vi.fn(async () => loadedPage({ stale: true }));
      const query = vi.fn(async () => loadedPage());
      const mounted = mountCockpit({ read, query, poll: true });
      await vi.advanceTimersByTimeAsync(10);
      // Exactly the one capability read the cockpit does at mount.
      const atMount = read.mock.calls.length;
      // And exactly one page read, for the dock's Changes section. A closed
      // review adds nothing to it.
      expect(query).toHaveBeenCalledTimes(1);
      await vi.advanceTimersByTimeAsync(2_000);
      expect(read.mock.calls.length).toBe(atMount);
      expect(query).toHaveBeenCalledTimes(1);
      mounted.dispose();
    } finally {
      vi.useRealTimers();
    }
  });
});
