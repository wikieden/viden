import { renderDiffFiles } from "../components/diff_rows";
import type { Locale } from "../i18n/catalog";
import type { DiffDocumentProjection } from "../models/diff_review";
import type { D14AuditScope } from "./d14_audit_timeline";
import "./d2_decisions.css";

/// D2 Decision Center.
///
/// One queue over every decision Core is holding for a human. The screen
/// renders only projected Core facts: it never derives a decision, a diff, or
/// a risk bucket from display text, and it renders declared gaps as explicit
/// unavailable notes carrying their contract-request code.

export interface D2Unavailable {
  key: string;
  code: string;
}

export interface D2QueueItem {
  id: string;
  kind: string;
  title: string;
  projectId: string;
  laneId: string | null;
  sessionId: string | null;
  taskId: string | null;
  risk: string | null;
  status: string;
  auditId: string;
  updatedAt: number | null;
  expiresAt: number | null;
}

export interface D2Group {
  kind: string;
  items: D2QueueItem[];
  unavailable: D2Unavailable | null;
}

export interface D2Context {
  source: string;
  text: string;
  unavailable: D2Unavailable | null;
}

export interface D2Evidence {
  id: string;
  kind: string;
  summary: string;
  path: string | null;
  source: string | null;
  timestamp: number | null;
}

export interface D2Action {
  kind: string;
  available: boolean;
  sessionId: string | null;
  paths: string[];
  code: string | null;
}

export interface D2Detail {
  id: string;
  kind: string;
  title: string;
  projectId: string;
  laneId: string | null;
  taskId: string | null;
  auditId: string;
  /**
   * The audit object this decision's records link, when one exists. `null` for
   * a decision Core links no queryable object for; the host decides, and the
   * screen never spells an object kind itself.
   */
  auditScope: D14AuditScope | null;
  policyReasonKey: string | null;
  blockedByPlan: boolean;
  context: D2Context;
  /**
   * The same Core decision context the D1 permission dock renders
   * (GUI-CORE-012). `null` for a decision Core published none for, where the
   * context pane keeps its unavailable marker and its preview.
   */
  decisionContext?: {
    diff: DiffDocumentProjection | null;
    baseSha256: string | null;
  } | null;
  evidence: D2Evidence[];
  actions: D2Action[];
}

export interface D2DecisionsProjection {
  workMode: string;
  permissionLevel: string;
  pendingTotal: number;
  selectedId: string | null;
  groups: D2Group[];
  detail: D2Detail | null;
}

export type D2Intent =
  | { type: "select"; id: string }
  | { type: "respond_approval"; requestId: string; choice: string; feedback: string | null }
  | { type: "decide_contract"; contractId: string; accept: boolean }
  | { type: "decide_review"; reviewId: string; accept: boolean; feedback: string | null };

/// Core's own cap on reviewer prose (`validate_trust_text`, 500 chars), which
/// the host mirrors as `D2_REVIEW_FEEDBACK_MAX_CHARS`. Over-limit text is
/// refused rather than truncated, so a half-sent note can never be attributed
/// to the reviewer.
export const D2_REVIEW_FEEDBACK_MAX_CHARS = 500;

export interface D2IntentResult {
  projection: D2DecisionsProjection;
  pendingCommandId: string | null;
  outcome: { state: string; reason: string | null };
}

export interface D2Controller {
  applyProjection: (projection: D2DecisionsProjection) => void;
}

/// Screen-local copy. Keys mirror Core fact families and declared-gap reason
/// keys, so an unmapped key falls back to the raw key rather than to a guess.
type Copy = Record<string, string>;

const COPY: Record<Locale, Copy> = {
  en: {
    title: "Decision Center",
    awaiting: "awaiting you",
    all: "All",
    group_gate: "Gate approvals",
    group_review: "Review requests",
    group_contract: "Contract records",
    context_approval_input_preview: "Tool input preview",
    context_review_request: "Review request",
    context_contract_summary: "Contract summary",
    evidence: "Evidence",
    noEvidence: "Core published no evidence for this decision.",
    decisionSink: "Decision and reason are written to audit",
    auditTrail: "View audit trail",
    once: "Approve once",
    session: "Approve for session",
    repo_allowlist: "Approve for repo paths",
    deny: "Deny",
    accept_review: "Accept review",
    reject_review: "Reject review",
    confirm_contract: "Confirm contract",
    reject_contract: "Reject contract",
    planBlocked: "Plan mode blocks mutating decisions.",
    reviewFeedbackLabel: "Reviewer note (optional)",
    reviewFeedbackPlaceholder: "What the reviewer wants the origin Lane to know",
    reviewFeedbackTooLong:
      "The reviewer note is limited to {max} characters. Shorten it; it is never truncated for you.",
    outcome_pending: "Sent. Waiting for Core to record the verdict.",
    outcome_confirmed: "Core recorded the verdict.",
    outcome_rejected: "Core refused this decision.",
    "D2-NO-REVIEWER-ACTOR":
      "Core published no independent reviewer identity for this review, so this client cannot decide it.",
    "D2-REVIEW-SETTLED": "Core already settled this review; a verdict is recorded once.",
    "GUI-CORE-013":
      "Core already recorded this contract's decision, and publishes no pending contract to confirm.",
    "d2.contract.noPendingFact":
      "Core records decided contracts only; there is no pending-confirmation fact.",
    "d2.context.noStructuredDiff":
      "Core exposes an opaque input preview; structured diff rows are unavailable.",
    computedAgainst: "computed against",
    noDiffComputed: "Core produced no diff for this decision. This does not mean it is empty.",
    empty: "Core is holding no decision for you.",
  },
  "zh-CN": {
    title: "决策中心",
    awaiting: "项等你",
    all: "全部",
    group_gate: "闸审批",
    group_review: "评审请求",
    group_contract: "契约记录",
    context_approval_input_preview: "工具入参预览",
    context_review_request: "评审请求",
    context_contract_summary: "契约摘要",
    evidence: "证据",
    noEvidence: "Core 未为该决策提供证据。",
    decisionSink: "决策与理由写入审计",
    auditTrail: "查看审计轨迹",
    once: "批准本次",
    session: "批准本会话",
    repo_allowlist: "批准指定仓库路径",
    deny: "驳回",
    accept_review: "接受评审",
    reject_review: "驳回评审",
    confirm_contract: "确认契约",
    reject_contract: "驳回契约",
    planBlocked: "Plan 模式阻止一切变更类决策。",
    reviewFeedbackLabel: "评审意见（可选）",
    reviewFeedbackPlaceholder: "评审者希望源 Lane 知道的内容",
    reviewFeedbackTooLong: "评审意见上限为 {max} 个字符。请自行精简；系统不会替你截断。",
    outcome_pending: "已发送，等待 Core 记录裁决。",
    outcome_confirmed: "Core 已记录该裁决。",
    outcome_rejected: "Core 拒绝了该决策。",
    "D2-NO-REVIEWER-ACTOR": "Core 未为该评审发布独立评审方身份，本客户端无法代为裁决。",
    "D2-REVIEW-SETTLED": "Core 已裁决该评审；裁决只记录一次。",
    "GUI-CORE-013": "Core 已记录该契约的裁决，且不发布任何待确认契约。",
    "d2.contract.noPendingFact": "Core 只记录已决契约，没有「待确认」这一事实。",
    "d2.context.noStructuredDiff": "Core 只提供不透明入参预览，结构化 diff 行不可用。",
    computedAgainst: "基于",
    noDiffComputed: "Core 没有为该决策产出 diff。这不代表它是空的。",
    empty: "Core 当前没有等你处理的决策。",
  },
};

function label(copy: Copy, key: string): string {
  return copy[key] ?? key;
}

export function renderD2Decisions(
  root: HTMLElement,
  initial: D2DecisionsProjection,
  send: (intent: D2Intent) => Promise<D2IntentResult>,
  locale: Locale,
  onViewAuditTrail?: (scope: D14AuditScope) => void,
): D2Controller {
  let projection = initial;
  let filter = "all";
  let busy = false;
  const copy = COPY[locale];
  /// The ordered Core receipt for the last decision, or a local refusal. Never
  /// a locally invented success: only `outcome.state` says what Core recorded.
  let outcome: { state: string; reason: string | null } | null = null;
  /// Reviewer prose draft. It travels with the command and is never persisted
  /// client-side; Core stores it on the review record.
  let reviewFeedback = "";

  const dispatch = (intent: D2Intent): void => {
    if (busy) return;
    if (intent.type === "decide_review") {
      // Mirror Core's cap locally so an over-limit note is refused before it
      // becomes a command Core would have to reject.
      const note = (intent.feedback ?? "").trim();
      if ([...note].length > D2_REVIEW_FEEDBACK_MAX_CHARS) {
        outcome = {
          state: "rejected",
          reason: label(copy, "reviewFeedbackTooLong").replace(
            "{max}",
            String(D2_REVIEW_FEEDBACK_MAX_CHARS),
          ),
        };
        render();
        return;
      }
    }
    busy = true;
    outcome = intent.type === "select" ? null : { state: "pending", reason: null };
    render();
    void send(intent)
      .then((result) => {
        // Success is rendered only from the projection Core confirmed.
        projection = result.projection;
        outcome = intent.type === "select" ? null : result.outcome;
        if (intent.type === "decide_review" && result.outcome.state === "confirmed") {
          reviewFeedback = "";
        }
      })
      .catch((error: unknown) => {
        // The host refused before the wire (no actor, settled review, a verdict
        // already in flight). Its sentence is the honest reason.
        outcome = {
          state: "rejected",
          reason: error instanceof Error ? error.message : String(error),
        };
      })
      .finally(() => {
        busy = false;
        render();
      });
  };

  const renderQueue = (): HTMLElement => {
    const queue = document.createElement("div");
    queue.className = "d2-queue";
    queue.dataset.d2Queue = "true";
    for (const group of projection.groups) {
      if (filter !== "all" && filter !== group.kind) continue;
      const section = document.createElement("div");
      section.className = "d2-qsec";
      section.dataset.d2Group = group.kind;

      const heading = document.createElement("div");
      heading.className = "d2-qhead";
      heading.textContent = `${label(copy, `group_${group.kind}`)} · ${group.items.length}`;
      section.append(heading);

      if (group.unavailable) {
        const note = document.createElement("p");
        note.className = "d2-unavailable";
        note.dataset.d2Unavailable = group.unavailable.code;
        note.textContent = `${label(copy, group.unavailable.key)} · ${group.unavailable.code}`;
        section.append(note);
      }

      for (const item of group.items) {
        const button = document.createElement("button");
        button.type = "button";
        button.className = "d2-qitem";
        button.dataset.d2Item = item.id;
        button.dataset.d2ItemKind = item.kind;
        button.setAttribute(
          "aria-pressed",
          String(projection.selectedId === item.id),
        );
        const first = document.createElement("span");
        first.className = "d2-qtitle";
        first.textContent = item.title;
        const second = document.createElement("span");
        second.className = "d2-qmeta";
        const facts = [item.projectId, item.laneId, item.status, item.risk].filter(
          (fact): fact is string => typeof fact === "string" && fact.length > 0,
        );
        second.textContent = facts.join(" · ");
        button.append(first, second);
        button.addEventListener("click", () => dispatch({ type: "select", id: item.id }));
        section.append(button);
      }
      queue.append(section);
    }
    return queue;
  };

  const renderDetail = (detail: D2Detail): HTMLElement => {
    const main = document.createElement("div");
    main.className = "d2-main";
    main.dataset.d2Detail = detail.id;
    main.dataset.d2DetailKind = detail.kind;

    const head = document.createElement("div");
    head.className = "d2-rhead";
    const title = document.createElement("span");
    title.className = "d2-rtitle";
    title.textContent = detail.title;
    head.append(title);
    for (const chip of [detail.projectId, detail.laneId, detail.taskId]) {
      if (!chip) continue;
      const element = document.createElement("span");
      element.className = "d2-chip";
      element.textContent = chip;
      head.append(element);
    }
    main.append(head);

    const split = document.createElement("div");
    split.className = "d2-split";

    const context = document.createElement("div");
    context.className = "d2-pane d2-pane-code";
    context.dataset.d2Context = detail.context.source;
    const contextHead = document.createElement("div");
    contextHead.className = "d2-ph";
    contextHead.textContent = label(copy, `context_${detail.context.source}`);
    const contextBody = document.createElement("pre");
    contextBody.className = "d2-context-body";
    contextBody.textContent = detail.context.text;
    context.append(contextHead, contextBody);
    // Core's typed rows, when it published them. The preview stays above
    // them: it is the tool input, and the rows are a preview of its effect.
    if (detail.decisionContext) {
      const pane = document.createElement("div");
      pane.className = "d2-decision-context";
      pane.dataset.d2DecisionContext = "true";
      if (detail.decisionContext.baseSha256) {
        const base = document.createElement("p");
        base.className = "d2-decision-base";
        base.dataset.decisionBase = detail.decisionContext.baseSha256;
        base.textContent = `${label(copy, "computedAgainst")} ${detail.decisionContext.baseSha256.slice(0, 8)}`;
        pane.append(base);
      }
      if (detail.decisionContext.diff) {
        renderDiffFiles(pane, detail.decisionContext.diff.files, locale);
      } else {
        const note = document.createElement("p");
        note.className = "dl note";
        note.dataset.diffNote = "no-diff";
        note.textContent = label(copy, "noDiffComputed");
        pane.append(note);
      }
      context.append(pane);
    }
    if (detail.context.unavailable) {
      const note = document.createElement("p");
      note.className = "d2-unavailable";
      note.dataset.d2Unavailable = detail.context.unavailable.code;
      note.textContent = `${label(copy, detail.context.unavailable.key)} · ${detail.context.unavailable.code}`;
      context.append(note);
    }

    const evidence = document.createElement("div");
    evidence.className = "d2-pane d2-pane-evidence";
    evidence.dataset.d2Evidence = "true";
    const evidenceHead = document.createElement("div");
    evidenceHead.className = "d2-ph";
    evidenceHead.textContent = copy.evidence;
    evidence.append(evidenceHead);
    if (detail.evidence.length === 0) {
      const none = document.createElement("p");
      none.className = "d2-muted";
      none.textContent = copy.noEvidence;
      evidence.append(none);
    }
    for (const entry of detail.evidence) {
      const row = document.createElement("div");
      row.className = "d2-evrow";
      row.dataset.d2EvidenceId = entry.id;
      row.textContent = `${entry.kind} · ${entry.summary}${entry.path ? ` · ${entry.path}` : ""}`;
      evidence.append(row);
    }
    split.append(context, evidence);
    main.append(split);

    if (detail.kind === "review") {
      // The reviewer's own words. Optional: absence is absence, and Core keeps
      // a settled review without a note rather than storing an empty one.
      const decidable = detail.actions.some((action) => action.available);
      const field = document.createElement("label");
      field.className = "d2-review-note";
      const caption = document.createElement("span");
      caption.className = "d2-ph";
      caption.textContent = copy.reviewFeedbackLabel;
      const input = document.createElement("textarea");
      input.className = "d2-review-input";
      input.dataset.d2ReviewFeedback = "true";
      input.rows = 2;
      input.value = reviewFeedback;
      input.placeholder = copy.reviewFeedbackPlaceholder;
      input.disabled = busy || !decidable;
      input.addEventListener("input", () => {
        reviewFeedback = input.value;
      });
      field.append(caption, input);
      main.append(field);
    }

    const gatebar = document.createElement("div");
    gatebar.className = "d2-gatebar";
    const sink = document.createElement("span");
    sink.className = "d2-gsink";
    sink.textContent = `${copy.decisionSink} ${detail.auditId}`;
    gatebar.append(sink);
    // The audit id names where the decision is written; the trail is reachable
    // only through the object Core linked, because `AuditQuery` filters by
    // object and never by audit id.
    const auditScope = detail.auditScope;
    if (auditScope && onViewAuditTrail) {
      const trail = document.createElement("button");
      trail.type = "button";
      trail.className = "d2-trail";
      trail.dataset.d2AuditTrail = `${auditScope.kind}:${auditScope.id}`;
      trail.disabled = busy;
      trail.textContent = copy.auditTrail;
      trail.addEventListener("click", () => onViewAuditTrail(auditScope));
      gatebar.append(trail);
    }
    if (detail.blockedByPlan) {
      const blocked = document.createElement("span");
      blocked.className = "d2-muted";
      blocked.dataset.d2PlanBlocked = "true";
      blocked.textContent = copy.planBlocked;
      gatebar.append(blocked);
    }
    for (const action of detail.actions) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "d2-gbtn";
      button.dataset.d2Action = action.kind;
      button.disabled = !action.available || busy;
      // A disabled control always says why: either the host's own reason code
      // or the plan-mode banner beside it. It is never enabled-and-inert.
      if (action.code) button.dataset.d2ActionCode = action.code;
      button.textContent = action.available
        ? label(copy, action.kind)
        : `${label(copy, action.kind)} · ${action.code ?? ""}`;
      if (action.available) {
        button.addEventListener("click", () => {
          if (detail.kind === "gate") {
            dispatch({
              type: "respond_approval",
              requestId: detail.id,
              choice: action.kind === "deny" ? "deny" : action.kind,
              feedback: null,
            });
            return;
          }
          if (detail.kind === "review") {
            const note = reviewFeedback.trim();
            dispatch({
              type: "decide_review",
              reviewId: detail.id,
              accept: action.kind === "accept_review",
              feedback: note.length > 0 ? note : null,
            });
            return;
          }
          if (detail.kind === "contract") {
            dispatch({
              type: "decide_contract",
              contractId: detail.id,
              accept: action.kind === "confirm_contract",
            });
          }
        });
      }
      gatebar.append(button);
    }
    main.append(gatebar);

    // The reason a disabled review action names, spelled out once rather than
    // left as a bare code beside every button.
    const blockingCode = detail.actions.find((action) => !action.available && action.code)?.code;
    if (blockingCode) {
      const note = document.createElement("p");
      note.className = "d2-unavailable";
      note.dataset.d2Unavailable = blockingCode;
      note.textContent = `${label(copy, blockingCode)} · ${blockingCode}`;
      main.append(note);
    }

    if (outcome) {
      const receipt = document.createElement("p");
      receipt.className = "d2-outcome";
      receipt.dataset.d2Outcome = outcome.state;
      if (outcome.state === "rejected") receipt.setAttribute("role", "alert");
      // Core's own words for a refusal; the screen never paraphrases them.
      receipt.textContent = outcome.reason
        ? `${label(copy, `outcome_${outcome.state}`)} ${outcome.reason}`
        : label(copy, `outcome_${outcome.state}`);
      main.append(receipt);
    }
    return main;
  };

  const render = (): void => {
    const stage = document.createElement("section");
    stage.className = "d2-stage";
    stage.dataset.route = "d2";
    stage.setAttribute("aria-busy", String(busy));

    const bar = document.createElement("div");
    bar.className = "d2-cmdbar";
    const heading = document.createElement("h2");
    heading.className = "d2-title";
    heading.textContent = copy.title;
    const count = document.createElement("span");
    count.className = "d2-count";
    count.dataset.d2PendingTotal = String(projection.pendingTotal);
    count.textContent = `${projection.pendingTotal} ${copy.awaiting}`;
    bar.append(heading, count);

    const chips = document.createElement("div");
    chips.className = "d2-fchips";
    for (const kind of ["all", ...projection.groups.map((group) => group.kind)]) {
      const chip = document.createElement("button");
      chip.type = "button";
      chip.className = "d2-fchip";
      chip.dataset.d2Filter = kind;
      chip.setAttribute("aria-pressed", String(filter === kind));
      chip.textContent =
        kind === "all" ? copy.all : label(copy, `group_${kind}`);
      chip.addEventListener("click", () => {
        filter = kind;
        render();
      });
      chips.append(chip);
    }
    bar.append(chips);

    const meta = document.createElement("span");
    meta.className = "d2-meta";
    meta.textContent = `${projection.workMode} · ${projection.permissionLevel}`;
    bar.append(meta);
    stage.append(bar);

    const inner = document.createElement("div");
    inner.className = "d2-inner";
    inner.append(renderQueue());
    if (projection.detail) {
      inner.append(renderDetail(projection.detail));
    } else {
      const empty = document.createElement("p");
      empty.className = "d2-muted";
      empty.dataset.d2Empty = "true";
      empty.textContent = copy.empty;
      inner.append(empty);
    }
    stage.append(inner);
    root.replaceChildren(stage);
  };

  render();
  return {
    applyProjection: (next) => {
      projection = next;
      render();
    },
  };
}
