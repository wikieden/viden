import { renderDiffFiles } from "./diff_rows";
import type { Locale } from "../i18n/catalog";
import { translate } from "../i18n/catalog";
import type { ChecklistItemProjection, D1CockpitProjection } from "../models/workspace";

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

/** Render only the typed selected-Lane change/check projections. */
export function appendTypedWorkCards(
  root: HTMLElement,
  checklist: ChecklistItemProjection[],
  locale: Locale,
): void {
  for (const item of checklist.slice(0, MAX_CENTER_WORK_CARDS)) {
    const card = document.createElement("article");
    card.className = "d1-work-card";
    card.dataset.centerStep = item.kind === "workspace_change" ? "workspace-change" : "check-run";
    if (item.kind === "workspace_change") {
      card.dataset.workspaceChange = item.id;
      card.setAttribute("aria-label", translate(locale, "d1.workspaceChange", {}));
      const heading = document.createElement("header");
      heading.textContent = `${item.label} · ${localizedStatus(locale, item.status)}`;
      card.append(heading);
      if (item.diff) {
        // Core published typed rows for this change. `patch` carries the same
        // computation as opaque text, so exactly one of them is drawn —
        // printing both would read as two separate changes (GUI-CORE-012).
        renderDiffFiles(card, item.diff.files, locale);
      } else {
        const patch = document.createElement("pre");
        if (item.patch) {
          patch.textContent = item.patch;
        } else {
          patch.dataset.typedEmpty = "workspace-change-patch";
          patch.textContent = translate(locale, "d1.workspaceChange.emptyPatch", {});
        }
        card.append(patch);
      }
      if (item.additions !== null && item.deletions !== null) {
        const stats = document.createElement("p");
        stats.className = "d1-work-card-meta";
        stats.textContent = `+${item.additions} −${item.deletions}`;
        card.append(stats);
      }
    } else {
      card.dataset.checkRun = item.id;
      card.setAttribute("aria-label", translate(locale, "d1.checkRun", {}));
      const heading = document.createElement("header");
      heading.textContent = `${item.label} · ${localizedStatus(locale, item.status)}`;
      const command = document.createElement("code");
      if (item.command) {
        command.textContent = item.command;
      } else {
        command.dataset.typedEmpty = "check-run-command";
        command.textContent = translate(locale, "d1.checkRun.emptyCommand", {});
      }
      const result = document.createElement("p");
      if (item.summary) {
        result.textContent = [item.summary, item.failingLocation].filter(Boolean).join(" · ");
      } else {
        result.dataset.typedEmpty = "check-run-result";
        result.textContent = translate(locale, "d1.checkRun.emptyResult", {});
      }
      card.append(heading, command, result);
    }
    root.append(card);
  }
}
