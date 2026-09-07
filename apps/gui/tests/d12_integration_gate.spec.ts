// @vitest-environment jsdom

import { describe, expect, test, vi } from "vitest";

import {
  renderD12IntegrationGate,
  type D12IntegrationGateProjection,
  type D12Intent,
  type D12IntentResult,
} from "../src/screens/d12_integration_gate";

const GATE = {
  gateId: "gate-1",
  taskId: "task-lane-3",
  status: "needs_changes",
  gateType: "patch",
  projectId: "project-boss-rush",
  laneId: "lane-3",
  requiresIndependentValidator: true,
  hasValidator: false,
  requiredEvidence: ["replay-regression"],
  evidenceIds: [] as string[],
  dormant: false,
};

const PROJECTION: D12IntegrationGateProjection = {
  gates: [GATE],
  selectedGateId: "gate-1",
  detail: {
    gate: GATE,
    missingEvidence: ["replay-regression"],
    bounces: [
      {
        bounceId: "bounce-1",
        originalLaneId: "lane-3",
        taskId: "task-lane-3",
        reason: "src/player/dash.gd conflicts with the merged baseline",
        status: "revalidated",
        evidenceIds: [],
      },
    ],
    reverts: [],
    checks: [{ id: "check-1", name: "replay-regression", status: "failed" }],
    actions: [
      { kind: "accept", available: false, code: null },
      { kind: "reject", available: true, code: null },
    ],
  },
  unavailable: [{ key: "d12.conflict.noStructuredHunk", code: "GUI-CORE-015" }],
};

/// An open gate Core would let the operator decide on.
const OPEN_GATE = { ...GATE, requiresIndependentValidator: false, hasValidator: true };

const DECIDABLE: D12IntegrationGateProjection = {
  ...PROJECTION,
  gates: [OPEN_GATE],
  detail: {
    ...PROJECTION.detail!,
    gate: OPEN_GATE,
    missingEvidence: [],
    actions: [
      { kind: "accept", available: true, code: null },
      { kind: "reject", available: true, code: null },
    ],
  },
};

function setup(
  projection: D12IntegrationGateProjection = PROJECTION,
  send?: (intent: D12Intent) => Promise<D12IntentResult>,
) {
  document.body.innerHTML = '<div id="host"></div>';
  const root = document.querySelector<HTMLElement>("#host")!;
  const onSelect = vi.fn();
  const onViewAuditTrail = vi.fn();
  renderD12IntegrationGate(root, projection, "en", onSelect, send, onViewAuditTrail);
  return { root, onSelect, onViewAuditTrail };
}

/// The merged gate Core leaves behind after a post-merge rollback.
function mergedProjection(): D12IntegrationGateProjection {
  return {
    ...PROJECTION,
    detail: {
      ...PROJECTION.detail!,
      gate: { ...GATE, status: "merged" },
      missingEvidence: [],
      actions: [
        { kind: "accept", available: false, code: null },
        { kind: "reject", available: false, code: null },
      ],
      reverts: [
        {
          revertId: "revert-1",
          appliedChangeId: "change-1",
          reason: "cancel window regressed",
          restoredPaths: ["src/player/dash.gd"],
          auditId: "audit-revert-1",
          auditScope: { kind: "revert", id: "revert-1" },
          revertedAt: 1_700_000_900,
        },
      ],
    },
  };
}

function result(
  projection: D12IntegrationGateProjection,
  outcome: D12IntentResult["outcome"] = { state: "confirmed", reason: null },
): D12IntentResult {
  return { projection, pendingCommandId: null, outcome };
}

describe("D12 integration gate", () => {
  test("groups dormant gates under one separator without removing any of them", () => {
    // The projection hands the screen active gates first with the dormant tail
    // flagged; the screen marks where that tail starts. Nothing is filtered:
    // every chip below the separator stays a live button.
    const dormant = { ...GATE, gateId: "gate-old", dormant: true };
    const dormantTwo = { ...GATE, gateId: "gate-older", dormant: true };
    const { root, onSelect } = setup({
      ...PROJECTION,
      gates: [GATE, dormant, dormantTwo],
    });

    const separator = root.querySelector<HTMLElement>("[data-d12-dormant-separator]");
    expect(separator).not.toBeNull();
    expect(separator!.dataset.d12DormantSeparator).toBe("2");
    expect(separator!.textContent).toContain("session finished");

    const chips = [...root.querySelectorAll<HTMLElement>("[data-d12-gate]")];
    expect(chips.map((chip) => chip.dataset.d12Gate)).toEqual([
      "gate-1",
      "gate-old",
      "gate-older",
    ]);
    expect(chips[0]!.dataset.d12GateDormant).toBeUndefined();
    expect(chips[1]!.dataset.d12GateDormant).toBe("true");

    // The separator sits before the first dormant chip, not anywhere else.
    const nodes = [...root.querySelector(".d12-gates")!.children];
    expect(nodes.indexOf(separator!)).toBe(1);

    // Grouping, never hiding: a dormant chip still selects its gate.
    chips[1]!.click();
    expect(onSelect).toHaveBeenCalledWith("gate-old");
  });

  test("renders no dormant separator when every gate is still live", () => {
    const { root } = setup();
    expect(root.querySelector("[data-d12-dormant-separator]")).toBeNull();
  });

  test("shows the conflict banner and the strong-gate policy", () => {
    const { root } = setup();
    expect(root.querySelector<HTMLElement>("[data-d12-banner]")?.dataset.d12Banner).toBe(
      "conflict",
    );
    expect(root.querySelector(".d12-strength")?.textContent).toContain("cannot be bypassed");
    expect(root.querySelector("[data-d12-policy]")?.textContent).toContain(
      "independent validator required",
    );
  });

  test("keeps accept closed and names the evidence Core is still missing", () => {
    const { root } = setup();
    const accept = root.querySelector<HTMLButtonElement>("[data-d12-action='accept']");
    const reject = root.querySelector<HTMLButtonElement>("[data-d12-action='reject']");
    expect(accept?.disabled).toBe(true);
    // Core marks the bounce available, but it still needs a reason and a host
    // callback before it can be sent; this render has neither.
    expect(reject?.disabled).toBe(true);
    expect(root.querySelector("[data-d12-missing]")?.textContent).toContain(
      "replay-regression",
    );
    // No manual-merge escape hatch exists in the rendered action bar.
    expect(root.querySelectorAll("[data-d12-action]")).toHaveLength(2);
  });

  test("renders the bounce timeline back to the origin lane", () => {
    const { root } = setup();
    const bounce = root.querySelector<HTMLElement>("[data-d12-bounce='bounce-1']");
    expect(bounce?.dataset.d12BounceStatus).toBe("revalidated");
    expect(bounce?.textContent).toContain("lane-3");
  });

  test("shows the post-merge rollback once Core records one", () => {
    const { root } = setup(mergedProjection());
    expect(root.querySelector<HTMLElement>("[data-d12-banner]")?.dataset.d12Banner).toBe(
      "resolved",
    );
    expect(root.querySelector("[data-d12-revert='revert-1']")?.textContent).toContain(
      "audit-revert-1",
    );
  });

  test("declares the missing conflict hunk instead of rendering one", () => {
    const { root } = setup();
    expect(
      root.querySelector<HTMLElement>("[data-d12-unavailable]")?.dataset.d12Unavailable,
    ).toBe("GUI-CORE-015");
    expect(root.querySelector(".d12-diff")).toBeNull();
  });

  test("names why a closed action is closed instead of going dark", () => {
    const { root } = setup({
      ...PROJECTION,
      detail: {
        ...PROJECTION.detail!,
        actions: [
          { kind: "accept", available: false, code: "validator_required" },
          { kind: "reject", available: true, code: null },
        ],
      },
    });
    const accept = root.querySelector<HTMLButtonElement>("[data-d12-action='accept']");
    expect(accept?.dataset.d12ActionCode).toBe("validator_required");
    expect(accept?.textContent).toContain("independent validator");
  });
});

describe("D12 merge-gate decisions", () => {
  test("accept dispatches the Core command for the selected gate", async () => {
    const sent: D12Intent[] = [];
    const send = vi.fn(async (intent: D12Intent) => {
      sent.push(intent);
      return result({
        ...DECIDABLE,
        detail: {
          ...DECIDABLE.detail!,
          gate: { ...OPEN_GATE, status: "accepted" },
          actions: [
            { kind: "accept", available: false, code: "gate_closed" },
            { kind: "reject", available: false, code: "gate_closed" },
          ],
        },
      });
    });
    const { root } = setup(DECIDABLE, send);

    const accept = root.querySelector<HTMLButtonElement>("[data-d12-action='accept']")!;
    expect(accept.disabled).toBe(false);
    accept.click();
    // The screen states it is waiting for Core rather than claiming success.
    expect(root.querySelector("[data-route='d12']")?.getAttribute("aria-busy")).toBe("true");

    await vi.waitFor(() => expect(send).toHaveBeenCalledTimes(1));
    expect(sent).toEqual([{ type: "accept", gateId: "gate-1" }]);
    // Re-rendered from the projection Core confirmed, not from the click.
    await vi.waitFor(() =>
      expect(root.querySelector<HTMLElement>("[data-d12-banner]")?.dataset.d12Banner).toBe(
        "resolved",
      ),
    );
    expect(root.querySelector<HTMLButtonElement>("[data-d12-action='accept']")?.disabled).toBe(
      true,
    );
  });

  test("bounce requires a reason and sends the one the operator typed", async () => {
    const sent: D12Intent[] = [];
    const send = vi.fn(async (intent: D12Intent) => {
      sent.push(intent);
      return result(DECIDABLE);
    });
    const { root } = setup(DECIDABLE, send);

    const bounce = root.querySelector<HTMLButtonElement>("[data-d12-action='reject']")!;
    // Core refuses an empty rejection reason, so the control stays closed.
    expect(bounce.disabled).toBe(true);
    bounce.click();
    expect(send).not.toHaveBeenCalled();

    const reason = root.querySelector<HTMLInputElement>("[data-d12-reason]")!;
    expect(reason.disabled).toBe(false);
    reason.value = "  rebase onto the merged baseline  ";
    reason.dispatchEvent(new InputEvent("input", { bubbles: true }));
    expect(bounce.disabled).toBe(false);
    bounce.click();

    await vi.waitFor(() => expect(send).toHaveBeenCalledTimes(1));
    expect(sent).toEqual([
      { type: "bounce", gateId: "gate-1", reason: "rebase onto the merged baseline" },
    ]);
  });

  test("renders a Core rejection verbatim as an alert", async () => {
    const send = vi.fn(async () =>
      result(DECIDABLE, {
        state: "rejected",
        reason: "merge gate `gate-1` requires an independent validator",
      }),
    );
    const { root } = setup(DECIDABLE, send);

    root.querySelector<HTMLButtonElement>("[data-d12-action='accept']")?.click();

    const alert = await vi.waitFor(() => {
      const node = root.querySelector<HTMLElement>("[data-d12-error]");
      if (!node) throw new Error("no rejection alert");
      return node;
    });
    expect(alert.getAttribute("role")).toBe("alert");
    expect(alert.textContent).toBe("merge gate `gate-1` requires an independent validator");
    expect(root.querySelector("[data-route='d12']")?.getAttribute("aria-busy")).toBe("false");
  });

  test("keeps both controls closed when no host callback is injected", () => {
    const { root } = setup(DECIDABLE);
    expect(root.querySelector<HTMLButtonElement>("[data-d12-action='accept']")?.disabled).toBe(
      true,
    );
    expect(root.querySelector<HTMLButtonElement>("[data-d12-action='reject']")?.disabled).toBe(
      true,
    );
    expect(root.querySelector<HTMLInputElement>("[data-d12-reason]")?.disabled).toBe(true);
  });

  test("keeps both controls closed when Core says the gate is undecidable", () => {
    const send = vi.fn(async () => result(PROJECTION));
    const { root } = setup(
      {
        ...PROJECTION,
        detail: {
          ...PROJECTION.detail!,
          actions: [
            { kind: "accept", available: false, code: "missing_evidence" },
            { kind: "reject", available: false, code: "gate_closed" },
          ],
        },
      },
      send,
    );

    root.querySelector<HTMLButtonElement>("[data-d12-action='accept']")?.click();
    root.querySelector<HTMLButtonElement>("[data-d12-action='reject']")?.click();
    expect(send).not.toHaveBeenCalled();
    expect(root.querySelector<HTMLInputElement>("[data-d12-reason]")?.disabled).toBe(true);
  });
});

describe("D12 audit trail navigation", () => {
  test("a revert row navigates to the revert object Core linked, not its audit id", () => {
    const { root, onViewAuditTrail } = setup(mergedProjection());
    const trail = root.querySelector<HTMLButtonElement>("[data-d12-audit-trail]");
    expect(trail?.dataset.d12AuditTrail).toBe("revert:revert-1");
    trail!.click();
    expect(onViewAuditTrail).toHaveBeenCalledWith({ kind: "revert", id: "revert-1" });
  });
});
