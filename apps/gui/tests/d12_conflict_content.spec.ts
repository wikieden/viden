// @vitest-environment jsdom

import { describe, expect, test, vi } from "vitest";

import { renderConflictContent, renderConflictUnavailable } from "../src/components/conflict_rows";
import type { ConflictContentProjection } from "../src/models/conflict";

/// The canonical `conflict-content.json` bounce content, as the host projects
/// it: an evidence baseline, one file, one hunk with all three sides.
const CONTENT: ConflictContentProjection = {
  baseline: {
    kind: "evidence",
    sha: null,
    shortSha: null,
    bindings: [
      {
        evidenceId: "ev_conflict_patch_b",
        sourceHash: "cf".repeat(32),
        shortHash: "cfcfcfcfcfcf",
        auditScope: { kind: "evidence", id: "ev_conflict_patch_b" },
      },
    ],
  },
  files: [
    {
      path: "crates/runtime/src/trust_loop.rs",
      omitted: false,
      hunks: [
        {
          oursStart: 42,
          ours: ["    let bounce = record_conflict_bounce(gate)?;"],
          theirsStart: 42,
          theirs: ["    let bounce = bounce_with_reason(gate, reason)?;"],
          base: ["    let bounce = record_bounce(gate)?;"],
          reason: "context_mismatch",
        },
      ],
    },
  ],
  truncated: false,
};

function host(): HTMLElement {
  document.body.innerHTML = '<div id="host"></div>';
  return document.querySelector<HTMLElement>("#host")!;
}

function mount(
  content: ConflictContentProjection,
  onOpenEvidence?: (scope: { kind: string; id: string }) => void,
): HTMLElement {
  const root = host();
  renderConflictContent(root, content, "en", onOpenEvidence);
  return root;
}

describe("D12 structured conflict content", () => {
  test("renders each hunk as two sides plus the preimage and never a merged result", () => {
    const root = mount(CONTENT);

    const ours = root.querySelector("[data-conflict-side='ours']")!;
    const theirs = root.querySelector("[data-conflict-side='theirs']")!;
    const base = root.querySelector("[data-conflict-side='base']")!;
    expect(ours.textContent).toContain("record_conflict_bounce");
    expect(theirs.textContent).toContain("bounce_with_reason");
    expect(base.textContent).toContain("record_bounce");

    // Core's own starts, not a client-side count.
    expect(ours.querySelector(".ln")!.textContent).toBe("42");
    expect(theirs.querySelector(".ln")!.textContent).toBe("42");

    // The pane says what it is, and offers no way to resolve anything.
    expect(root.querySelector("[data-conflict-not-merge]")!.textContent).toMatch(
      /not a merge result/i,
    );
    expect(root.querySelectorAll("button[data-conflict-resolve]")).toHaveLength(0);
  });

  test("names the baseline and routes each evidence chip to its own audit object", () => {
    const onOpenEvidence = vi.fn();
    const root = mount(CONTENT, onOpenEvidence);

    const baseline = root.querySelector("[data-conflict-baseline]")!;
    expect(baseline.getAttribute("data-conflict-baseline")).toBe("evidence");
    const chip = root.querySelector<HTMLButtonElement>(
      "[data-conflict-evidence='ev_conflict_patch_b']",
    )!;
    expect(chip.textContent).toContain("cfcfcfcfcfcf");
    // The full hash stays reachable even though the chip shows the short form.
    expect(chip.title).toContain("cf".repeat(32));
    chip.click();
    expect(onOpenEvidence).toHaveBeenCalledWith({ kind: "evidence", id: "ev_conflict_patch_b" });
  });

  test("renders a revision baseline with its short sha and an unknown one as unknown", () => {
    const revision = mount({
      ...CONTENT,
      baseline: { kind: "revision", sha: "9f".repeat(20), shortSha: "9f9f9f9f9f9f", bindings: [] },
    });
    expect(revision.querySelector("[data-conflict-baseline]")!.textContent).toContain(
      "9f9f9f9f9f9f",
    );

    const unknown = mount({
      ...CONTENT,
      baseline: { kind: "unknown", sha: null, shortSha: null, bindings: [] },
    });
    const line = unknown.querySelector("[data-conflict-baseline='unknown']")!;
    // Unknown is a real Core answer, never a silent `HEAD`.
    expect(line.textContent).toMatch(/unknown/i);
    expect(line.textContent).not.toMatch(/HEAD/);
  });

  test("says hunks are not shown for an omitted file and banners a truncated payload", () => {
    const root = mount({
      ...CONTENT,
      truncated: true,
      files: [{ path: "assets/atlas.png", hunks: [], omitted: true }],
    });

    expect(root.querySelector("[data-conflict-truncated]")).not.toBeNull();
    const note = root.querySelector("[data-conflict-note='omitted']")!;
    expect(note.textContent).toMatch(/not shown/i);
    // "Not shown" must never read as "no conflict".
    expect(note.textContent).not.toMatch(/no conflict/i);
  });

  test("gives each reason its own sentence and says what the operator can do", () => {
    const reasons = [
      "context_mismatch",
      "already_applied",
      "file_missing",
      "file_deleted",
      "binary",
    ];
    const seen = new Set<string>();
    for (const reason of reasons) {
      const root = mount({
        ...CONTENT,
        files: [{ ...CONTENT.files[0], hunks: [{ ...CONTENT.files[0].hunks[0], reason }] }],
      });
      const chip = root.querySelector(`[data-conflict-reason='${reason}']`)!;
      expect(chip.textContent!.length).toBeGreaterThan(0);
      expect(chip.textContent).not.toContain("[missing:");
      // Never the raw tag when the reason is one Core names.
      expect(chip.textContent).not.toContain(reason);
      // Every reason says what the operator can do about it.
      const remedy = root.querySelector(`[data-conflict-remedy='${reason}']`)!;
      expect(remedy.textContent!.length).toBeGreaterThan(0);
      expect(remedy.textContent).not.toContain("[missing:");
      seen.add(chip.textContent!);
    }
    expect(seen.size).toBe(reasons.length);
  });

  test("renders an unmodelled reason raw rather than blank or mislabelled", () => {
    const root = mount({
      ...CONTENT,
      files: [
        {
          ...CONTENT.files[0],
          hunks: [{ ...CONTENT.files[0].hunks[0], reason: "core_future_reason" }],
        },
      ],
    });
    const chip = root.querySelector("[data-conflict-reason='core_future_reason']")!;
    expect(chip.textContent).toContain("core_future_reason");
    expect(chip.textContent).not.toContain("[missing:");
  });

  test("keeps an absent preimage distinct from one that expected an empty region", () => {
    const absent = mount({
      ...CONTENT,
      files: [
        { ...CONTENT.files[0], hunks: [{ ...CONTENT.files[0].hunks[0], base: null, reason: "binary" }] },
      ],
    });
    expect(absent.querySelector("[data-conflict-base='absent']")).not.toBeNull();

    const empty = mount({
      ...CONTENT,
      files: [{ ...CONTENT.files[0], hunks: [{ ...CONTENT.files[0].hunks[0], base: [] }] }],
    });
    expect(empty.querySelector("[data-conflict-base='empty']")).not.toBeNull();
  });

  test("distinguishes a record with no content from a Core that publishes none", () => {
    const noContent = host();
    renderConflictUnavailable(noContent, "en", true);
    const record = noContent.querySelector("[data-conflict-absent='record']")!;
    expect(record.textContent).toContain("Core published no structured conflict content");
    // An operator bounce carries none by contract; say so rather than implying
    // the conflict was empty.
    expect(record.textContent).toMatch(/operator/i);

    const noCapability = host();
    renderConflictUnavailable(noCapability, "en", false);
    const capability = noCapability.querySelector("[data-conflict-absent='capability']")!;
    expect(capability.textContent).toContain("runtime.conflict_content");
  });
});
