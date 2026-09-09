import { renderDiffFiles } from "./diff_rows";
import { translate, type Locale, type MessageKey } from "../i18n/catalog";
import type {
  PermissionChoice,
  PermissionDockProjection,
} from "../models/workspace";
import "./permission_dock.css";

export type { PermissionChoice, PermissionDockProjection } from "../models/workspace";

export interface PermissionIntent {
  type: "respond";
  requestId: string;
  choice: PermissionChoice;
  feedback: string | null;
}

export interface PermissionIntentResult {
  projection: PermissionDockProjection;
  pendingCommandId: string | null;
  outcome: { state: "idle" | "pending" | "confirmed" | "rejected"; reason: string | null };
}

const ACTION_COPY = {
  once: "d1.permission.once",
  session: "d1.permission.session",
  repo_allowlist: "d1.permission.repoAllowlist",
  always: "d1.permission.always",
  edit: "d1.permission.edit",
  deny: "d1.permission.deny",
} as const;

/**
 * Known divergence from the design package, recorded rather than resolved.
 *
 * The design assigns `Shift+A` to "Always". Schema 1 has no Always approval
 * scope and no Edit decision — both render as fail-closed GUI-CORE-003
 * placeholders — so binding the chord to a decision Core cannot carry would
 * put a live shortcut on a dead action. `Shift+A` therefore stays on
 * `repo_allowlist`, the widest scope Core actually accepts, until Core adds
 * the scope (contract request GUI-CORE-019). Do not re-point it before then.
 */
const SHORTCUTS: Partial<Record<PermissionChoice, string>> = {
  once: "Y",
  session: "A",
  repo_allowlist: "Shift+A",
  edit: "E",
  deny: "N",
};

const RISK_COPY = {
  low: "d1.risk.low",
  medium: "d1.risk.medium",
  high: "d1.risk.high",
  critical: "d1.risk.critical",
} as const;

/// The `ApprovalTarget.kind` values this build can name. A kind it cannot is
/// rendered as unavailable rather than as a guess, which is why `git` — the
/// one kind Core stamps on all five operator source-control actions
/// (`runtime.operator_git`) — has to be listed here: without it a real commit
/// ask would read as a target the dock does not understand.
const TARGET_COPY = {
  local: "d1.permission.target.local",
  repo_path: "d1.permission.target.repoPath",
  git: "d1.permission.target.git",
} as const;

const POLICY_COPY = {
  "permission.command_not_allowlisted": "d1.permission.policy.commandNotAllowlisted",
  "permission.requires_approval": "d1.permission.policy.requiresApproval",
} as const;

function localizedProtocol(
  locale: Locale,
  value: string,
  copy: Partial<Record<string, MessageKey>>,
): string {
  const key = copy[value];
  return key ? translate(locale, key, {}) : translate(locale, "d1.permission.unavailable", {});
}

export function renderPermissionDock(
  root: HTMLElement,
  projection: PermissionDockProjection,
  send: (intent: PermissionIntent) => Promise<unknown>,
  locale: Locale,
): void {
  const request = projection.request;
  if (!request) {
    root.replaceChildren();
    return;
  }
  const dock = document.createElement("section");
  dock.className = "gperm dock";
  dock.dataset.permissionDock = "true";
  dock.tabIndex = -1;
  dock.setAttribute("role", "region");
  dock.setAttribute("aria-label", translate(locale, "d1.permission.region", {}));

  const heading = document.createElement("header");
  heading.className = "gperm-hd";
  const title = document.createElement("span");
  title.textContent = `${request.title} · ${request.toolName}`;
  const risk = document.createElement("span");
  risk.className = `risk ${["high", "critical"].includes(request.risk) ? "hi" : request.risk === "low" ? "lo" : "md"}`;
  risk.textContent = localizedProtocol(locale, request.risk, RISK_COPY);
  heading.append(title, risk);

  const facts = document.createElement("div");
  facts.className = "gperm-what";
  const command = document.createElement("code");
  command.className = "cmd";
  command.textContent = request.inputPreview;
  facts.append(command);
  const factText = document.createElement("p");
  factText.className = "why";
  const scope = request.actions
    .filter((action) => action.available && action.kind !== "deny")
    .map((action) => {
      if (action.kind === "session") {
        return `${translate(locale, ACTION_COPY[action.kind], {})}(${action.sessionId ?? translate(locale, "d1.permission.unavailable", {})})`;
      }
      if (action.kind === "repo_allowlist") return `${translate(locale, ACTION_COPY[action.kind], {})}(${action.paths.join(", ")})`;
      return translate(locale, ACTION_COPY[action.kind], {});
    })
    .join(", ");
  const policyArgs = Object.keys(request.policyReasonArgs).length > 0
    ? ` ${JSON.stringify(request.policyReasonArgs)}`
    : "";
  factText.textContent = [
    request.message,
    `${translate(locale, "d1.permission.target", {})}: ${localizedProtocol(locale, request.target.kind, TARGET_COPY)} · ${request.target.display}${request.target.canonicalRef ? ` · ${request.target.canonicalRef}` : ""}`,
    `${translate(locale, "d1.permission.reason", {})}: ${request.reason ?? translate(locale, "d1.permission.unavailable", {})}`,
    `${translate(locale, "d1.permission.scope", {})}: ${scope || translate(locale, "d1.permission.unavailable", {})}`,
    `${translate(locale, "d1.permission.policy", {})}: ${localizedProtocol(locale, request.policyReasonKey, POLICY_COPY)}${policyArgs}`,
    `${translate(locale, "d1.permission.expires", {})}: ${request.expiresAt}`,
    `${translate(locale, "d1.permission.defaultAction", {})}: ${localizedProtocol(locale, request.defaultAction, ACTION_COPY)}`,
    `${translate(locale, "d1.permission.audit", {})}: ${request.auditId}`,
  ].join(" · ");
  facts.append(factText);

  // Core's structured decision context, when it published one (GUI-CORE-012).
  //
  // The rows go *beside* `input_preview`, never instead of it: the preview is
  // Core's own summary of the tool input, the rows are a preview of the effect
  // that input would have, and an operator approving a mutation should see
  // both. Where Core published no context — `shell` and the `git_*` family
  // never carry one — nothing is added and the preview stands alone, with the
  // wording it has always had.
  const context = request.decisionContext;
  if (context) {
    const pane = document.createElement("div");
    pane.className = "gperm-diff";
    pane.dataset.decisionContext = "true";
    if (context.baseSha256) {
      // The preimage the preview was computed against. Stated as a note rather
      // than a guarantee: execution runs the proposed tool input against
      // whatever the file holds then, and Core does not re-check first, so
      // this is what lets a reader detect a file that moved in between.
      const base = document.createElement("p");
      base.className = "gperm-diff-base";
      base.dataset.decisionBase = context.baseSha256;
      base.textContent = translate(locale, "d1.permission.computedAgainst", {
        hash: context.baseSha256.slice(0, 8),
      });
      pane.append(base);
    }
    if (context.diff) {
      if (context.diff.truncated) {
        const banner = document.createElement("p");
        banner.className = "gperm-diff-base";
        banner.dataset.decisionTruncated = "true";
        banner.textContent = translate(locale, "d1.review.truncated", {});
        pane.append(banner);
      }
      renderDiffFiles(pane, context.diff.files, locale);
    } else {
      // Core attached a context and produced no diff for it. That is not "no
      // change", so the shared renderer's own sentence is used.
      renderDiffFiles(pane, [], locale);
      const note = document.createElement("p");
      note.className = "dl note";
      note.dataset.diffNote = "no-diff";
      note.textContent = translate(locale, "d1.review.noDiff", {});
      pane.append(note);
    }
    facts.append(pane);
  }

  if (request.blockedByPlan) {
    const alert = document.createElement("p");
    alert.className = "gperm-plan";
    alert.setAttribute("role", "alert");
    alert.textContent = translate(locale, "d1.permission.plan", {});
    facts.append(alert);
  }

  const options = document.createElement("div");
  options.className = "gperm-opts";
  const actionButtons = new Map<PermissionChoice, HTMLButtonElement>();
  for (const action of request.actions) {
    const actionButton = document.createElement("button");
    actionButton.type = "button";
    actionButton.className = `gperm-opt${action.kind === "deny" ? " deny" : ""}`;
    actionButton.dataset.permissionAction = action.kind;
    actionButton.disabled = !action.available;
    const shortcut = SHORTCUTS[action.kind];
    if (shortcut) actionButton.setAttribute("aria-keyshortcuts", shortcut);
    const label = translate(locale, ACTION_COPY[action.kind], {});
    actionButton.textContent = `${shortcut ? `${shortcut} · ` : ""}${label}${action.code ? ` · ${action.code}` : ""}`;
    actionButton.addEventListener("click", () => {
      if (!action.available) return;
      void send({
        type: "respond",
        requestId: request.id,
        choice: action.kind,
        feedback: null,
      });
    });
    actionButtons.set(action.kind, actionButton);
    options.append(actionButton);
  }
  dock.addEventListener("keydown", (event) => {
    const key = event.key.toLowerCase();
    const choice = key === "y"
      ? "once"
      : key === "n"
        ? "deny"
        : key === "a" && event.shiftKey
          ? "repo_allowlist"
          : key === "a"
            ? "session"
            : key === "e"
              ? "edit"
              : null;
    if (!choice) return;
    const action = actionButtons.get(choice);
    if (!action || action.disabled) return;
    event.preventDefault();
    action.focus();
    action.click();
  });
  dock.append(heading, facts, options);
  root.replaceChildren(dock);
}
