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
  /**
   * The revised tool input an `Edit` decision carries
   * (`runtime.approval_scopes`, GUI-CORE-019).
   *
   * A revision is a new input Core re-evaluates through the same gate, not a
   * variant of an allow, so it travels only with `edit` and the whole map goes
   * — including the keys the operator left alone.
   */
  revisedInput?: Record<string, string> | null;
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
 * The design's scope tiers, restored (`D-PERM`, `SPEC` line 82).
 *
 * The package fixes exactly five keys — `y · a · ⇧A · e · n` — and `⇧A` is
 * "Always". The GUI parked that chord on `repo_allowlist` while schema 1 had no
 * Always scope, because binding a live chord to a dead action was worse than
 * the divergence; `runtime.approval_scopes` (C15) shipped the scope and its
 * revocation, so the chord returns to the tier the design assigns it.
 *
 * `repo_allowlist` gets no chord back, and that is deliberate rather than an
 * omission: the design's tier list does not contain it, so inventing a sixth
 * key would be this client legislating a shortcut the package never fixed. It
 * stays a button in the option row, which is where the design puts every scope
 * Core offers.
 */
const SHORTCUTS: Partial<Record<PermissionChoice, string>> = {
  once: "Y",
  session: "A",
  always: "Shift+A",
  edit: "E",
  deny: "N",
};

/**
 * The codes a blocked scope or decision carries, turned into sentences.
 *
 * `scope_not_offered` and `revision_unchanged` are Core's own refusal names, so
 * a code this table does not have is printed verbatim rather than softened: a
 * refusal the operator cannot match against Core's audit row is worse than a
 * raw string.
 */
const BLOCK_COPY: Partial<Record<string, MessageKey>> = {
  scope_not_offered: "d1.permission.scopeNotOffered",
  not_editable: "d1.permission.notEditable",
  "GUI-CORE-036": "d1.permission.editUnavailable",
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
  /**
   * Opens D14 scoped to the audit object this decision's record names
   * (`runtime.durable_work_evidence`, C7).
   *
   * Before C7 the `audit_id` the dock printed was minted, published on two
   * facts, and never written anywhere, so a link would have resolved to
   * nothing — which is why this row was a fact and not a control. C7 appends
   * the record under the permission object when the decision is applied, so
   * the link resolves for every decided request. Absent while the caller
   * cannot host D14, which leaves the row a fact rather than a dead control.
   */
  onOpenAuditTrail?: (scope: { kind: string; id: string }) => void,
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

  // A revision Core produced from an `Edit` is shown linked to the request it
  // replaces, not as a second unrelated prompt: the operator has to be able to
  // tell "Core wants another decision about the thing I just narrowed" from
  // "Core wants a decision about something else". It sits directly under the
  // input because that is the thing it revises, and because `.gperm-what`
  // scrolls in the dock — a link placed after the fact line would be true and
  // unread.
  if (request.revisedFrom) {
    const revision = document.createElement("p");
    revision.className = "gperm-revision";
    revision.dataset.permissionRevisedFrom = request.revisedFrom;
    revision.textContent = translate(locale, "d1.permission.revisedFrom", {
      request: request.revisedFrom,
    });
    facts.append(revision);
  }

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

  if (onOpenAuditTrail) {
    // `D-AUDIT`'s link runs one way: an audit row names a permission object,
    // so the trail is opened by that object and never by the audit id, which
    // the client would otherwise have to construct a query for.
    const trail = document.createElement("button");
    trail.type = "button";
    trail.className = "d1-action";
    trail.dataset.permissionAuditTrail = request.id;
    trail.textContent = translate(locale, "d1.permission.auditTrail", {});
    // The row is appended when the decision is *applied*, so while this
    // request is still open the trail is a correct query with nothing in it
    // yet. The control says so rather than implying a row already exists.
    trail.title = translate(locale, "d1.permission.auditTrailPending", {});
    facts.append(trail);
    trail.addEventListener("click", () =>
      onOpenAuditTrail({ kind: "permission", id: request.id }),
    );
  }

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

  /**
   * The inline editor, open only while the operator asked for it.
   *
   * The fields are Core's own tool-input pairs, recovered by the host; the
   * draft starts as a copy of them and every key travels back on send,
   * including the ones left alone, because Core re-evaluates the whole input
   * rather than a patch of it.
   */
  let editOpen = false;
  const draft = new Map(request.inputFields.map((field) => [field.key, field.value]));

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
    // A blocked scope or decision says why in the accessible name, not only as
    // a code chip: the operator has to learn from the control they pressed why
    // Core would refuse it.
    if (action.code) {
      const reason = BLOCK_COPY[action.code];
      const sentence = reason ? translate(locale, reason, {}) : action.code;
      actionButton.title = `${label} — ${sentence}`;
      actionButton.setAttribute("aria-label", `${label} — ${sentence}`);
      actionButton.dataset.permissionBlock = action.code;
    }
    actionButton.addEventListener("click", () => {
      if (!action.available) return;
      if (action.kind === "edit") {
        // Edit is not a decision on its own: it opens the field the operator
        // revises, and the revision is what gets sent.
        editOpen = !editOpen;
        editor.hidden = !editOpen;
        actionButton.setAttribute("aria-expanded", String(editOpen));
        if (editOpen) editor.querySelector("input")?.focus();
        return;
      }
      void send({
        type: "respond",
        requestId: request.id,
        choice: action.kind,
        feedback: null,
      });
    });
    if (action.kind === "edit") {
      actionButton.setAttribute("aria-expanded", "false");
      actionButton.setAttribute("aria-controls", "gperm-editor");
    }
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
          ? "always"
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
  // The editor is built unconditionally and hidden, so the Edit button has an
  // element to name in `aria-controls` before it is opened.
  const editor = document.createElement("form");
  editor.id = "gperm-editor";
  editor.className = "gperm-editor";
  editor.dataset.permissionEditor = "true";
  editor.hidden = true;
  const editorHead = document.createElement("p");
  editorHead.className = "gperm-editor-head";
  editorHead.textContent = translate(locale, "d1.permission.editHead", {});
  editor.append(editorHead);
  for (const field of request.inputFields) {
    const row = document.createElement("label");
    row.className = "gperm-editor-row";
    const name = document.createElement("span");
    name.className = "k";
    // Core's own key, printed raw: it is the contract's vocabulary for what
    // this value means to the tool.
    name.textContent = field.key;
    const input = document.createElement("input");
    input.type = "text";
    input.dataset.permissionEditField = field.key;
    input.value = field.value;
    input.addEventListener("input", () => draft.set(field.key, input.value));
    row.append(name, input);
    editor.append(row);
  }
  const editorSend = document.createElement("button");
  editorSend.type = "submit";
  editorSend.className = "gperm-opt";
  editorSend.dataset.permissionEditSend = "true";
  editorSend.textContent = translate(locale, "d1.permission.editSend", {});
  editor.append(editorSend);
  const editorNote = document.createElement("p");
  editorNote.className = "gperm-editor-note";
  editorNote.textContent = translate(locale, "d1.permission.editNote", {});
  editor.append(editorNote);
  editor.addEventListener("submit", (event) => {
    event.preventDefault();
    // A revision byte-identical to the original is `revision_unchanged` at
    // Core; the dock lets it go and renders Core's refusal rather than
    // second-guessing the comparison, because Core compares the canonical
    // input and this client only has the preview it parsed.
    void send({
      type: "respond",
      requestId: request.id,
      choice: "edit",
      feedback: null,
      revisedInput: Object.fromEntries(draft),
    });
  });

  dock.append(heading, facts, options, editor);
  root.replaceChildren(dock);
}
