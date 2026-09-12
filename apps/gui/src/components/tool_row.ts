import { renderDiffFiles } from "./diff_rows";
import type { Locale } from "../i18n/catalog";
import { translate } from "../i18n/catalog";
import type {
  ChecklistItemProjection,
  D1CockpitProjection,
  PermissionDockProjection,
} from "../models/workspace";
import "./tool_row.css";

export const MAX_CENTER_WORK_CARDS = 24;

const STATUS_KEYS = {
  added: "d1.status.added",
  modified: "d1.status.modified",
  deleted: "d1.status.deleted",
  renamed: "d1.status.renamed",
  untracked: "d1.status.untracked",
  queued: "d1.status.queued",
  running: "d1.status.running",
  passed: "d1.status.passed",
  failed: "d1.status.failed",
  cancelled: "d1.status.cancelled",
} as const;

export function localizedStatus(locale: Locale, status: string): string {
  const key = STATUS_KEYS[status as keyof typeof STATUS_KEYS];
  return key ? translate(locale, key, {}) : status;
}

export function toolRowText(tool: D1CockpitProjection["liveWork"]["tools"][number]): string {
  return `${tool.name} · ${tool.inputPreview}`;
}

/**
 * The approval Core has open against one path, if any.
 *
 * A gate chip on a change is only honest when Core really is holding that
 * change: the match is on the approval's own `decisionContext` diff, which is
 * the only place Core says which file a pending proposal touches. An approval
 * with no context — the ordinary case for `shell` and the `git_*` family —
 * gates nothing on this block, because Core did not say it did.
 */
function pendingApprovalFor(
  dock: PermissionDockProjection,
  path: string | null,
): { toolName: string } | null {
  const request = dock.request;
  if (!request || path === null) return null;
  const files = request.decisionContext?.diff?.files ?? [];
  const touches = files.some((file) => file.path === path || file.oldPath === path);
  return touches ? { toolName: request.toolName } : null;
}

/**
 * One `.tool` block: the design's `.th` header over its `.tb` body.
 *
 * `collapsible` makes the header a disclosure button. A diff is collapsed by
 * default because a transcript is read top to bottom and a fifty-row hunk in
 * the middle of it buries the conversation; a check run is not, because its
 * body is three lines and the failing one is the reason the block exists.
 */
function toolBlock(
  locale: Locale,
  options: {
    name: string;
    detail: string;
    gate: { text: string; tone: "waiting" | "failed" | "passed" | "neutral" } | null;
    stats: string | null;
    collapsible: boolean;
  },
): { block: HTMLElement; body: HTMLElement } {
  const block = document.createElement("article");
  // `d1-work-card` is kept beside the design's `tool`: it is the class the
  // cockpit's own bounding and its tests address the block by, and the two
  // names describe the same node rather than two ideas.
  block.className = "tool d1-tool d1-work-card";

  const header = options.collapsible
    ? document.createElement("button")
    : document.createElement("div");
  header.className = "th d1-tool-head";
  if (header instanceof HTMLButtonElement) {
    header.type = "button";
    header.dataset.toolToggle = "true";
    header.setAttribute("aria-expanded", "false");
  }
  const name = document.createElement("span");
  name.className = "nm";
  name.textContent = options.name;
  const detail = document.createElement("span");
  detail.className = "pa";
  detail.title = options.detail;
  detail.textContent = options.detail;
  header.append(name, detail);
  if (options.stats !== null) {
    const stats = document.createElement("span");
    // The historical marker class stays: it is what the cockpit's "no counts
    // published" coverage asserts the absence of.
    stats.className = "d1-work-card-meta";
    stats.dataset.toolStats = "true";
    stats.textContent = options.stats;
    header.append(stats);
  }
  if (options.gate) {
    const gate = document.createElement("span");
    gate.className = "gate";
    gate.dataset.toolGate = options.gate.tone;
    gate.textContent = options.gate.text;
    header.append(gate);
  }

  const body = document.createElement("div");
  body.className = "tb d1-tool-body";
  body.dataset.toolBody = "true";
  if (options.collapsible && header instanceof HTMLButtonElement) {
    body.hidden = true;
    header.addEventListener("click", () => {
      const expanded = header.getAttribute("aria-expanded") === "true";
      header.setAttribute("aria-expanded", String(!expanded));
      body.hidden = expanded;
    });
    const expand = translate(locale, "d1.tool.expand", {});
    header.setAttribute("aria-label", `${options.name} · ${options.detail} · ${expand}`);
  }

  block.append(header, body);
  return { block, body };
}

/** One `.testrow`: a key Core named and the value it reported for it. */
function checkRow(host: HTMLElement, id: string, key: string, value: string): HTMLElement {
  const row = document.createElement("p");
  row.className = "testrow d1-check-row";
  row.dataset.checkRow = id;
  const label = document.createElement("span");
  label.className = "k";
  label.textContent = key;
  const detail = document.createElement("span");
  detail.textContent = value;
  row.append(label, detail);
  host.append(row);
  return row;
}

/**
 * Render only the typed selected-Lane change/check projections.
 *
 * Both kinds are drawn as the design's `.tool` block, which is how the
 * flagship's transcript shows a `write_file` and a `test` run: one header
 * naming what happened and where, one body carrying the evidence. The change
 * block's body is the shared hunk renderer — the same rows DiffReview, the
 * permission dock and D2 draw — so the transcript and the review can never
 * disagree about what a hunk is.
 */
export function appendTypedWorkCards(
  root: HTMLElement,
  checklist: ChecklistItemProjection[],
  locale: Locale,
  /**
   * The approval Core currently has open, for the gate chip. Absent means the
   * caller has none to offer, which draws no chip — never a "clear" one.
   */
  permissionDock?: PermissionDockProjection,
): void {
  for (const item of checklist.slice(0, MAX_CENTER_WORK_CARDS)) {
    if (item.kind === "workspace_change") {
      const approval = permissionDock
        ? pendingApprovalFor(permissionDock, item.path ?? item.label)
        : null;
      const { block, body } = toolBlock(locale, {
        // Core publishes no producing tool on a workspace change, so the slot
        // carries the change kind it did publish — unless an approval names
        // the tool that proposed exactly this path, which is the one case
        // where the design's `write_file` / `edit_file` is a Core fact.
        // Ordered typed tool-call rows are GUI-CORE-009.
        name: approval?.toolName ?? localizedStatus(locale, item.status),
        detail: item.path ?? item.label,
        gate: approval
          ? { text: translate(locale, "d1.tool.gateWaiting", {}), tone: "waiting" }
          : null,
        stats:
          item.additions !== null && item.deletions !== null
            ? `+${item.additions} −${item.deletions}`
            : null,
        collapsible: true,
      });
      block.dataset.centerStep = "workspace-change";
      block.dataset.workspaceChange = item.id;
      block.setAttribute("aria-label", translate(locale, "d1.workspaceChange", {}));
      if (item.diff) {
        // Core published typed rows for this change. `patch` carries the same
        // computation as opaque text, so exactly one of them is drawn —
        // printing both would read as two separate changes (GUI-CORE-012).
        renderDiffFiles(body, item.diff.files, locale);
      } else {
        const patch = document.createElement("pre");
        if (item.patch) {
          patch.textContent = item.patch;
        } else {
          patch.dataset.typedEmpty = "workspace-change-patch";
          patch.textContent = translate(locale, "d1.workspaceChange.emptyPatch", {});
        }
        body.append(patch);
      }
      root.append(block);
      continue;
    }

    const failed = item.status === "failed";
    const { block, body } = toolBlock(locale, {
      name: item.label,
      detail: item.command ?? translate(locale, "d1.checkRun.emptyCommand", {}),
      gate: {
        text: localizedStatus(locale, item.status),
        tone: failed ? "failed" : item.status === "passed" ? "passed" : "neutral",
      },
      stats: null,
      // A check's whole body is three rows, and the failing line is why the
      // block is on screen at all; hiding it behind a disclosure would hide
      // the answer.
      collapsible: false,
    });
    block.dataset.centerStep = "check-run";
    block.dataset.checkRun = item.id;
    block.setAttribute("aria-label", translate(locale, "d1.checkRun", {}));
    if (item.command === null) {
      block.querySelector<HTMLElement>(".pa")!.dataset.typedEmpty = "check-run-command";
    }
    checkRow(
      body,
      "status",
      translate(locale, "d1.checkRun.status", {}),
      localizedStatus(locale, item.status),
    );
    if (item.failingLocation) {
      // Only when Core reported one. A check can fail without Core naming a
      // location, and an empty "failing" row would read as "nowhere".
      checkRow(
        body,
        "failing",
        translate(locale, "d1.checkRun.failing", {}),
        item.failingLocation,
      );
    }
    // An empty summary is as absent as a null one: Core said nothing about the
    // result either way, and a blank line would read as "no problems".
    const result = checkRow(
      body,
      "result",
      translate(locale, "d1.checkRun.result", {}),
      item.summary ? item.summary : translate(locale, "d1.checkRun.emptyResult", {}),
    );
    if (!item.summary) {
      result.lastElementChild!.setAttribute("data-typed-empty", "check-run-result");
    }
    root.append(block);
  }
}
