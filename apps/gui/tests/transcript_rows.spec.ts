// @vitest-environment jsdom

/**
 * The D1 transcript's ordered rows (`runtime.transcript_rows`, C8, closes
 * GUI-CORE-009) and the C7 audit joins that land beside them.
 *
 * The rules under test are the ones that make the read honest rather than
 * merely present:
 *
 * 1. **Core's page replaces the placeholders, and only when it answers.** The
 *    `transcript_user` / `transcript_assistant` unavailable rows were claims
 *    about Core; they retire exactly when Core answers with rows.
 * 2. **Four absences, four sentences.** Refused, out, answered-with-nothing,
 *    and no-capability are different facts.
 * 3. **The cursor is Core's.** Paging is offered only while Core published
 *    one, and `complete` is *stated*.
 * 4. **Scope is not a filter.** Rows read for another Lane are not drawn under
 *    this Lane's name.
 * 5. **A permission row joins its durable audit row** and opens D14 by the
 *    permission object, never by an audit id the client would construct.
 */

import { describe, expect, test, vi } from "vitest";

import { appendOrderedTranscriptRows } from "../src/components/transcript_rows";
import { renderD1Cockpit, type D1Intent, type D1IntentResult } from "../src/screens/d1_cockpit";
import {
  IDLE_TRANSCRIPT_ROWS,
  type TranscriptRowProjection,
  type TranscriptRowsProjection,
} from "../src/models/transcript_rows";
import { D1_PROJECTION } from "./support/d1_projection";

function row(overrides: Partial<TranscriptRowProjection>): TranscriptRowProjection {
  return {
    id: "row-1",
    kind: "user",
    laneId: "lane-core",
    sequence: 1,
    timestamp: 1_700_000_001,
    text: null,
    truncated: false,
    evidenceId: null,
    toolCallId: null,
    toolName: null,
    inputPreview: null,
    success: null,
    summary: null,
    checkId: null,
    label: null,
    command: null,
    status: null,
    failingLocation: null,
    requestId: null,
    decision: null,
    auditId: null,
    ...overrides,
  };
}

function page(
  rows: TranscriptRowProjection[],
  overrides: Partial<TranscriptRowsProjection> = {},
): TranscriptRowsProjection {
  return {
    ...IDLE_TRANSCRIPT_ROWS,
    capabilityAvailable: true,
    loaded: true,
    rows,
    complete: true,
    scopeLaneId: "lane-core",
    outcome: { state: "confirmed", reason: null },
    ...overrides,
  };
}

interface Harness {
  root: HTMLElement;
  queries: Array<string | null>;
  olderReads: number;
}

function mount(answers: {
  read: TranscriptRowsProjection;
  query?: TranscriptRowsProjection;
  older?: TranscriptRowsProjection;
  navigate?: (route: string, arg?: string) => void;
}): Harness {
  document.body.innerHTML = '<div id="host"></div>';
  const root = document.querySelector<HTMLElement>("#host")!;
  const result: D1IntentResult = {
    projection: D1_PROJECTION,
    pendingCommandId: null,
    outcome: { state: "confirmed" as const, reason: null },
  };
  const harness: Harness = { root, queries: [], olderReads: 0 };
  renderD1Cockpit(
    root,
    D1_PROJECTION,
    vi.fn(async (_intent: D1Intent) => result),
    vi.fn(async () => result),
    undefined,
    undefined,
    {
      poll: false,
      onNavigate: answers.navigate ?? (() => undefined),
      transcriptRows: {
        read: async () => answers.read,
        query: async (laneId) => {
          harness.queries.push(laneId);
          return answers.query ?? answers.read;
        },
        loadOlder: async () => {
          harness.olderReads += 1;
          return answers.older ?? answers.query ?? answers.read;
        },
      },
    },
  );
  return harness;
}

async function settle(hops = 10): Promise<void> {
  for (let hop = 0; hop < hops; hop += 1) await Promise.resolve();
}

describe("the ordered transcript replaces the GUI-CORE-009 placeholders", () => {
  test("Core's rows retire both unavailable placeholders", async () => {
    const harness = mount({
      read: page([]),
      query: page([
        row({ id: "row-user", kind: "user", text: "check the retry policy" }),
        row({ id: "row-assistant", kind: "assistant", sequence: 3, text: "three times" }),
      ]),
    });
    await settle();
    expect(harness.queries).toEqual([D1_PROJECTION.selectedLaneId]);
    expect(harness.root.querySelector("[data-typed-empty='transcript-user']")).toBeNull();
    expect(harness.root.querySelector("[data-typed-empty='transcript-assistant']")).toBeNull();
    const rows = Array.from(
      harness.root.querySelectorAll<HTMLElement>("[data-transcript-row-source='core']"),
    );
    expect(rows.map((element) => element.dataset.rowId)).toEqual(["row-user", "row-assistant"]);
  });

  test("an absent capability sends no read", async () => {
    // The placeholder itself is the *host projection's* answer now: since C8
    // `unavailable_features` drops both transcript rows when Core publishes
    // the capability, so the client-side check here is that no read is sent.
    const harness = mount({ read: IDLE_TRANSCRIPT_ROWS });
    await settle();
    expect(harness.queries).toHaveLength(0);
    expect(harness.root.querySelector("[data-transcript-row-source='core']")).toBeNull();
  });

  test("Core's refusal is its own words, and loads nothing", async () => {
    const harness = mount({
      read: page([], { loaded: false }),
      query: page([], {
        loaded: false,
        outcome: { state: "rejected", reason: "transcript row cursor `x` is not a cursor" },
      }),
    });
    await settle();
    const note = harness.root.querySelector<HTMLElement>("[data-transcript-rows-state]");
    expect(note?.dataset.transcriptRowsState).toBe("rejected");
    expect(note?.getAttribute("role")).toBe("alert");
    expect(note?.textContent).toContain("is not a cursor");
  });

  test("an answered-but-empty scope is its own sentence", async () => {
    const harness = mount({ read: page([], { loaded: false }), query: page([]) });
    await settle();
    expect(
      harness.root.querySelector<HTMLElement>("[data-transcript-rows-state]")?.dataset
        .transcriptRowsState,
    ).toBe("empty");
  });

  test("paging is offered only while Core published a cursor, and complete is stated", async () => {
    const withCursor = mount({
      read: page([], { loaded: false }),
      query: page([row({ id: "row-a", text: "a" })], {
        older: "s:1:row-a",
        complete: false,
      }),
    });
    await settle();
    const control = withCursor.root.querySelector<HTMLButtonElement>(
      "[data-transcript-load-older]",
    );
    expect(control).not.toBeNull();
    control?.click();
    await settle();
    expect(withCursor.olderReads).toBe(1);

    const complete = mount({
      read: page([], { loaded: false }),
      query: page([row({ id: "row-a", text: "a" })]),
    });
    await settle();
    expect(complete.root.querySelector("[data-transcript-load-older]")).toBeNull();
    expect(complete.root.querySelector("[data-transcript-complete]")).not.toBeNull();
  });

  test("rows read for another scope are not drawn under this Lane's name", async () => {
    const harness = mount({
      read: page([], { loaded: false }),
      query: page([row({ id: "row-elsewhere", text: "elsewhere" })], {
        scopeLaneId: "lane-elsewhere",
      }),
    });
    await settle();
    expect(harness.root.querySelector("[data-transcript-row-source='core']")).toBeNull();
  });
});

describe("the row renderer draws Core's shapes", () => {
  function render(rows: TranscriptRowProjection[], onOpenAudit?: (id: string) => void) {
    const host = document.createElement("div");
    appendOrderedTranscriptRows(host, rows, { locale: "en", onOpenAudit });
    return host;
  }

  test("a truncated body says the bound cut it and names its evidence", () => {
    const host = render([
      row({
        kind: "assistant",
        text: "cut here",
        truncated: true,
        evidenceId: "evidence_alpha",
      }),
    ]);
    expect(host.querySelector("[data-row-truncated]")?.textContent).toContain("8 KiB");
    expect(host.querySelector<HTMLElement>("[data-row-evidence]")?.dataset.rowEvidence).toBe(
      "evidence_alpha",
    );
  });

  test("a tool call, its result and a check run are the design's tool blocks", () => {
    const host = render([
      row({ id: "c", kind: "tool_call", toolName: "edit_file", inputPreview: "src/lib.rs" }),
      row({
        id: "r",
        kind: "tool_result",
        toolCallId: "call-1",
        success: false,
        summary: "refused",
      }),
      row({
        id: "k",
        kind: "check_run",
        checkId: "check-1",
        label: "unit tests",
        command: "cargo test",
        status: "failed",
        summary: "1 failed",
        failingLocation: "src/lib.rs:42",
      }),
    ]);
    expect(host.querySelector("[data-transcript-row='tool_call'] .nm")?.textContent).toBe(
      "edit_file",
    );
    expect(
      host.querySelector<HTMLElement>("[data-transcript-row='tool_result']")?.dataset.toolResult,
    ).toBe("failure");
    const check = host.querySelector<HTMLElement>("[data-transcript-row='check_run']")!;
    expect(check.querySelector("[data-check-row='failing']")?.textContent).toContain(
      "src/lib.rs:42",
    );
  });

  test("a permission row joins its audit row and opens D14 by the permission object", () => {
    const onOpenAudit = vi.fn();
    const host = render(
      [
        row({
          id: "p",
          kind: "permission",
          requestId: "approval-1",
          decision: "allow_once",
          auditId: "audit-1",
        }),
      ],
      onOpenAudit,
    );
    const permission = host.querySelector<HTMLElement>("[data-transcript-row='permission']")!;
    expect(permission.textContent).toContain("allow_once");
    permission.querySelector<HTMLButtonElement>("[data-transcript-audit]")?.click();
    expect(onOpenAudit).toHaveBeenCalledExactlyOnceWith("approval-1");
  });

  test("a scoped allow with no published decision says Core kept only the scope", () => {
    const host = render([
      row({ id: "p", kind: "permission", requestId: "approval-2", auditId: "audit-2" }),
    ]);
    expect(host.querySelector("[data-transcript-row='permission']")?.textContent).toContain(
      "scope only",
    );
  });

  test("an unmodelled row kind still occupies a row", () => {
    const host = render([row({ id: "x", kind: "some_future_kind" })]);
    const unknown = host.querySelector<HTMLElement>("[data-transcript-row='unknown']");
    expect(unknown?.textContent).toContain("some_future_kind");
  });
});

describe("the permission row's audit link lands in the cockpit", () => {
  test("it navigates to D14 scoped to the permission object", async () => {
    const navigate = vi.fn();
    const harness = mount({
      read: page([], { loaded: false }),
      query: page([
        row({
          id: "p",
          kind: "permission",
          requestId: "approval-9",
          decision: "deny",
          auditId: "audit-9",
        }),
      ]),
      navigate,
    });
    await settle();
    harness.root
      .querySelector<HTMLButtonElement>("[data-transcript-audit='approval-9']")
      ?.click();
    await settle();
    // Without a `secondaryViews` host the cockpit falls through to the shell's
    // own route, carrying the scope verbatim. `D-AUDIT` runs one way: the
    // object, never the audit id.
    expect(navigate).toHaveBeenCalledWith("d14", "permission:approval-9");
  });
});
