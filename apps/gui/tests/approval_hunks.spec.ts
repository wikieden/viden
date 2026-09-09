// @vitest-environment jsdom

/**
 * Approval decision context rendered as hunk rows (GUI-CORE-012).
 *
 * The D1 permission dock, the D2 decision detail, and D1's changed-file cards
 * all draw Core's structured diff through the one shared row renderer. The
 * rules under test are the ones that keep the surfaces honest: the rows appear
 * only where Core published a context, `input_preview` keeps its exact
 * existing wording where Core published none, and `base_sha256` is stated as
 * the preimage the preview was computed against rather than as a guarantee.
 */

import { beforeEach, describe, expect, test } from "vitest";

import { renderPermissionDock } from "../src/components/permission_dock";
import { appendTypedWorkCards } from "../src/components/tool_row";
import { renderD2Decisions } from "../src/screens/d2_decisions";
import type { PermissionDockProjection } from "../src/models/workspace";

const DIFF = {
  files: [
    {
      path: "crates/types/src/diff.rs",
      oldPath: null,
      kind: "modified",
      binary: false,
      omitted: false,
      additions: 1,
      deletions: 1,
      hunks: [
        {
          oldStart: 42,
          oldLines: 2,
          newStart: 42,
          newLines: 2,
          header: null,
          lines: [
            { kind: "removed", content: "old", oldLine: 42, newLine: null },
            { kind: "added", content: "new", oldLine: null, newLine: 42 },
          ],
        },
      ],
    },
  ],
  truncated: false,
  byteLimit: 65536,
};

function dockProjection(
  decisionContext: {
    diff: typeof DIFF | null;
    baseSha256: string | null;
  } | null,
): PermissionDockProjection {
  return {
    workMode: "build",
    permissionLevel: "ask",
    request: {
      id: "approval-edit",
      toolName: "edit_file",
      title: "Approve edit_file",
      message: "edit_file requires approval",
      inputPreview: "path: crates/types/src/diff.rs",
      isMutating: true,
      reason: "edit_file requires approval",
      risk: "medium",
      target: { kind: "repo_path", display: "crates/types/src/diff.rs", canonicalRef: null },
      policyReasonKey: "permission.requires_approval",
      policyReasonArgs: {},
      expiresAt: 1_700_001_100,
      defaultAction: "deny",
      auditId: "audit-edit",
      blockedByPlan: false,
      decisionContext,
      actions: [
        { kind: "once", available: true, sessionId: null, paths: [], code: null },
        { kind: "deny", available: true, sessionId: null, paths: [], code: null },
      ],
    },
  };
}

beforeEach(() => {
  document.body.replaceChildren();
});

describe("permission dock decision context", () => {
  test("renders Core's hunk rows and names the preimage it was computed against", () => {
    const host = document.createElement("div");
    renderPermissionDock(
      host,
      dockProjection({
        diff: DIFF,
        baseSha256: "3f79bb7b435b05321651daefd374cdc681dc06faa65e374e38337b88ca046dea",
      }),
      async () => undefined,
      "en",
    );
    expect(host.querySelector(".diffbody .dl.del")).not.toBeNull();
    expect(host.querySelector(".diffbody .dl.add")).not.toBeNull();
    const base = host.querySelector<HTMLElement>("[data-decision-base]");
    expect(base).not.toBeNull();
    // Eight characters is enough to read and short enough not to be mistaken
    // for a full hash the operator could verify by eye.
    expect(base!.textContent).toContain("3f79bb7b");
    // The preview stays: it is Core's own tool-input summary, and the rows are
    // a preview of the effect rather than a replacement for the input.
    expect(host.querySelector(".gperm-what .cmd")?.textContent).toBe(
      "path: crates/types/src/diff.rs",
    );
  });

  test("keeps the existing preview wording untouched when Core published no context", () => {
    const host = document.createElement("div");
    renderPermissionDock(host, dockProjection(null), async () => undefined, "en");
    expect(host.querySelector(".diffbody")).toBeNull();
    expect(host.querySelector("[data-decision-context]")).toBeNull();
    expect(host.querySelector(".gperm-what .cmd")?.textContent).toBe(
      "path: crates/types/src/diff.rs",
    );
  });

  test("a context Core computed no diff for says so instead of showing nothing", () => {
    const host = document.createElement("div");
    renderPermissionDock(
      host,
      dockProjection({ diff: null, baseSha256: null }),
      async () => undefined,
      "en",
    );
    expect(host.querySelector("[data-diff-note='no-diff']")).not.toBeNull();
  });

  test("a multi-file patch carries no base hash and states none", () => {
    const host = document.createElement("div");
    renderPermissionDock(
      host,
      dockProjection({ diff: DIFF, baseSha256: null }),
      async () => undefined,
      "en",
    );
    expect(host.querySelector(".diffbody")).not.toBeNull();
    expect(host.querySelector("[data-decision-base]")).toBeNull();
  });
});

describe("D2 decision detail", () => {
  const detail = (decisionContext: unknown, unavailable: unknown) => ({
    workMode: "build",
    permissionLevel: "ask",
    pendingTotal: 1,
    selectedId: "approval-edit",
    groups: [
      {
        kind: "gate",
        items: [
          {
            id: "approval-edit",
            kind: "gate",
            title: "Approve edit_file",
            projectId: "project",
            laneId: "lane-core",
            sessionId: null,
            taskId: null,
            risk: "medium",
            status: "pending",
            auditId: "audit-edit",
            updatedAt: null,
            expiresAt: null,
          },
        ],
        unavailable: null,
      },
    ],
    detail: {
      id: "approval-edit",
      kind: "gate",
      title: "Approve edit_file",
      projectId: "project",
      laneId: "lane-core",
      taskId: null,
      auditId: "audit-edit",
      auditScope: null,
      policyReasonKey: "permission.requires_approval",
      blockedByPlan: false,
      context: {
        source: "approval_input_preview",
        text: "path: crates/types/src/diff.rs",
        unavailable,
      },
      decisionContext,
      evidence: [],
      actions: [{ kind: "once", available: true, sessionId: null, paths: [], code: null }],
    },
  });

  test("renders hunk rows and drops the unavailable marker when Core sent a context", () => {
    const host = document.createElement("div");
    renderD2Decisions(
      host,
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      detail({ diff: DIFF, baseSha256: null }, null) as any,
      async () => ({ projection: null as never, pendingCommandId: null, outcome: { state: "idle", reason: null } }),
      "en",
    );
    expect(host.querySelector(".diffbody .dl.add")).not.toBeNull();
    expect(host.querySelector("[data-d2-unavailable='GUI-CORE-012']")).toBeNull();
  });

  test("keeps the preview and the marker when Core sent none", () => {
    const host = document.createElement("div");
    renderD2Decisions(
      host,
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      detail(null, { key: "d2.context.noStructuredDiff", code: "GUI-CORE-012" }) as any,
      async () => ({ projection: null as never, pendingCommandId: null, outcome: { state: "idle", reason: null } }),
      "en",
    );
    expect(host.querySelector(".diffbody")).toBeNull();
    expect(host.querySelector("[data-d2-unavailable='GUI-CORE-012']")).not.toBeNull();
    expect(host.querySelector(".d2-context-body")?.textContent).toBe(
      "path: crates/types/src/diff.rs",
    );
  });
});

describe("D1 changed-file cards", () => {
  test("render Core's rows when it published them and the patch string when it did not", () => {
    const withRows = document.createElement("div");
    appendTypedWorkCards(
      withRows,
      [
        {
          id: "change-1",
          kind: "workspace_change",
          label: "crates/types/src/diff.rs",
          status: "modified",
          command: null,
          path: "crates/types/src/diff.rs",
          summary: null,
          patch: "--- a\n+++ b\n",
          diff: DIFF,
          failingLocation: null,
          additions: 1,
          deletions: 1,
        },
      ],
      "en",
    );
    expect(withRows.querySelector(".diffbody .dl.add")).not.toBeNull();
    // The opaque patch string is not shown beside the rows: they are two views
    // of one Core computation, and printing both would read as two changes.
    expect(withRows.querySelector("pre")).toBeNull();

    const withoutRows = document.createElement("div");
    appendTypedWorkCards(
      withoutRows,
      [
        {
          id: "change-1",
          kind: "workspace_change",
          label: "crates/types/src/diff.rs",
          status: "modified",
          command: null,
          path: "crates/types/src/diff.rs",
          summary: null,
          patch: "--- a\n+++ b\n",
          diff: null,
          failingLocation: null,
          additions: 1,
          deletions: 1,
        },
      ],
      "en",
    );
    expect(withoutRows.querySelector(".diffbody")).toBeNull();
    expect(withoutRows.querySelector("pre")?.textContent).toBe("--- a\n+++ b\n");
  });
});
