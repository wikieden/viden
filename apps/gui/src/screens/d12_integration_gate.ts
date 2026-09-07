import type { Locale } from "../i18n/catalog";
import type { D14AuditScope } from "./d14_audit_timeline";
import "./d12_integration_gate.css";

/// D12 integration gate.
///
/// The failure path of the acceptance loop. The screen never offers a manual
/// merge: `accept` opens only when Core has recorded every evidence id the
/// gate policy requires, and the conflict is resolved by bouncing it back to
/// the origin Lane.

export interface D12Unavailable {
  key: string;
  code: string;
}

export interface D12Gate {
  gateId: string;
  taskId: string;
  status: string;
  gateType: string;
  projectId: string;
  laneId: string | null;
  requiresIndependentValidator: boolean;
  hasValidator: boolean;
  requiredEvidence: string[];
  evidenceIds: string[];
  /**
   * The gate awaits a decision its Agent session can no longer feed. Purely a
   * grouping signal: a dormant gate is listed, selectable, and decidable
   * exactly like any other, it just sits below the gates still attached to a
   * live session instead of on top of them.
   */
  dormant: boolean;
}

export interface D12Bounce {
  bounceId: string;
  originalLaneId: string;
  taskId: string;
  reason: string;
  status: string;
  evidenceIds: string[];
}

export interface D12Revert {
  revertId: string;
  appliedChangeId: string;
  reason: string;
  restoredPaths: string[];
  auditId: string;
  /**
   * The audit object Core's `change.reverted` record links, when one exists.
   * The host computes it; the screen never spells an object kind itself.
   */
  auditScope: D14AuditScope | null;
  revertedAt: number;
}

export interface D12Check {
  id: string;
  name: string;
  status: string;
}

export interface D12Action {
  kind: string;
  available: boolean;
  code: string | null;
}

export interface D12GateDetail {
  gate: D12Gate;
  missingEvidence: string[];
  bounces: D12Bounce[];
  reverts: D12Revert[];
  checks: D12Check[];
  actions: D12Action[];
}

export interface D12IntegrationGateProjection {
  gates: D12Gate[];
  selectedGateId: string | null;
  detail: D12GateDetail | null;
  unavailable: D12Unavailable[];
}

export interface D12Controller {
  applyProjection: (projection: D12IntegrationGateProjection) => void;
}

export interface D12ReviewedEvidence {
  evidenceId: string;
  sourceHash: string;
}

/// One merge-gate decision. Both map to a single Core command
/// (`AcceptMergeGate` / `RejectMergeGate`); the screen never merges anything
/// itself and never invents an actor or an evidence hash — the host replays
/// those from the gate Core published.
export type D12Intent =
  | {
      type: "accept";
      gateId: string;
      reviewedEvidence?: D12ReviewedEvidence[];
      decision?: string;
    }
  | { type: "bounce"; gateId: string; reason: string };

export interface D12Outcome {
  state: string;
  reason: string | null;
}

export interface D12IntentResult {
  projection: D12IntegrationGateProjection;
  pendingCommandId: string | null;
  outcome: D12Outcome;
}

type SendD12Intent = (intent: D12Intent) => Promise<D12IntentResult>;

type Copy = Record<string, string>;

const COPY: Record<Locale, Copy> = {
  en: {
    title: "Integration gate",
    gates: "gates",
    dormant: "dormant · session finished",
    conflict: "Merge conflict · bounce to the origin Lane",
    resolved: "Gate passed",
    strong: "strong gate · cannot be bypassed",
    validator: "independent validator required",
    validatorPresent: "validator recorded",
    validatorMissing: "no validator recorded",
    missing: "Missing required evidence",
    timeline: "Recovery timeline",
    noBounce: "Core recorded no bounce for this gate.",
    reverts: "Post-merge rollback",
    auditTrail: "View audit trail",
    checks: "Checks",
    accept: "Accept and merge",
    reject: "Bounce to origin Lane",
    reasonLabel: "Bounce reason",
    reasonPlaceholder: "Why the origin Lane is getting this back",
    noGate: "Core published no integration gate.",
    gate_closed: "the gate is already decided",
    missing_evidence: "Core is missing required evidence",
    evidence_not_canonical: "Core recorded no canonical evidence to verify",
    validator_required: "the policy requires an independent validator",
    conflict_pending: "the origin Lane has not revalidated the conflict",
    review_not_pending: "the validator's review request is not pending",
    no_actor: "Core published no owner this client may act as",
    "d12.conflict.noStructuredHunk":
      "Core publishes no structured conflict content, so the conflicting hunk cannot be shown.",
  },
  "zh-CN": {
    title: "集成闸",
    gates: "个闸",
    dormant: "休眠 · 会话已结束",
    conflict: "合并冲突 · 退回原 Lane",
    resolved: "闸已通过",
    strong: "强闸 · 不可绕过",
    validator: "要求独立验证方",
    validatorPresent: "已记录验证方",
    validatorMissing: "未记录验证方",
    missing: "缺少必需证据",
    timeline: "恢复时间线",
    noBounce: "Core 未为该闸记录任何退回。",
    reverts: "合入后回滚",
    auditTrail: "查看审计轨迹",
    checks: "检查",
    accept: "批准并合入",
    reject: "退回原 Lane",
    reasonLabel: "退回理由",
    reasonPlaceholder: "说明原 Lane 为什么要收回这份工作",
    noGate: "Core 未发布任何集成闸。",
    gate_closed: "该闸已决",
    missing_evidence: "Core 缺少必需证据",
    evidence_not_canonical: "Core 未记录可校验的规范化证据",
    validator_required: "策略要求独立验证方",
    conflict_pending: "原 Lane 尚未完成冲突复验",
    review_not_pending: "验证方的评审请求不处于待决状态",
    no_actor: "Core 未发布本客户端可代表的负责人",
    "d12.conflict.noStructuredHunk": "Core 不发布结构化冲突内容，冲突 hunk 无法展示。",
  },
};

function label(copy: Copy, key: string): string {
  return copy[key] ?? key;
}

/// Terminal Core statuses. Anything else is still an open gate.
const RESOLVED = new Set(["merged", "accepted", "reverted"]);

export function renderD12IntegrationGate(
  root: HTMLElement,
  initial: D12IntegrationGateProjection,
  locale: Locale,
  onSelect?: (gateId: string) => void,
  send?: SendD12Intent,
  onViewAuditTrail?: (scope: D14AuditScope) => void,
): D12Controller {
  let projection = initial;
  const copy = COPY[locale];
  let busy = false;
  // Core's own words for the last refused decision. Never a locally invented
  // sentence: the screen has no model of why a gate may be accepted.
  let decisionError: string | null = null;
  // Draft only. The reason travels with the command and is never persisted
  // client-side; Core stores it as the gate decision.
  let bounceReason = "";

  const dispatch = (intent: D12Intent): void => {
    if (busy || !send) return;
    busy = true;
    decisionError = null;
    render();
    void send(intent)
      .then((result) => {
        // Success is rendered only from the projection Core confirmed.
        projection = result.projection;
        decisionError = result.outcome.state === "rejected" ? result.outcome.reason : null;
        if (result.outcome.state !== "rejected" && intent.type === "bounce") {
          bounceReason = "";
        }
      })
      .catch((error: unknown) => {
        decisionError = error instanceof Error ? error.message : String(error);
      })
      .finally(() => {
        busy = false;
        render();
      });
  };

  const section = (titleKey: string): HTMLElement => {
    const heading = document.createElement("div");
    heading.className = "d12-sec";
    heading.textContent = label(copy, titleKey);
    return heading;
  };

  const render = (): void => {
    const stage = document.createElement("section");
    stage.className = "d12-stage";
    stage.dataset.route = "d12";
    stage.setAttribute("aria-busy", String(busy));

    const bar = document.createElement("div");
    bar.className = "d12-head";
    const heading = document.createElement("h2");
    heading.className = "d12-title";
    heading.textContent = copy.title;
    const count = document.createElement("span");
    count.className = "d12-count";
    count.textContent = `${projection.gates.length} ${copy.gates}`;
    bar.append(heading, count);

    const list = document.createElement("div");
    list.className = "d12-gates";
    // The projection orders active gates first and flags the dormant tail. One
    // separator marks where that tail begins, so a gate left behind by a
    // finished session is grouped and labelled rather than removed: every chip
    // below the separator stays clickable and fully decidable.
    const dormantCount = projection.gates.filter((gate) => gate.dormant).length;
    let separated = false;
    for (const gate of projection.gates) {
      if (gate.dormant && !separated) {
        separated = true;
        const divider = document.createElement("span");
        divider.className = "d12-gdormant";
        divider.dataset.d12DormantSeparator = String(dormantCount);
        divider.textContent = `${copy.dormant} · ${dormantCount}`;
        list.append(divider);
      }
      const chip = document.createElement("button");
      chip.type = "button";
      chip.className = "d12-gchip";
      chip.dataset.d12Gate = gate.gateId;
      if (gate.dormant) chip.dataset.d12GateDormant = "true";
      chip.setAttribute("aria-pressed", String(projection.selectedGateId === gate.gateId));
      chip.textContent = gate.dormant
        ? `${gate.gateId} · ${gate.status} · ${copy.dormant}`
        : `${gate.gateId} · ${gate.status}`;
      if (onSelect) chip.addEventListener("click", () => onSelect(gate.gateId));
      list.append(chip);
    }
    bar.append(list);
    stage.append(bar);

    const detail = projection.detail;
    if (!detail) {
      const empty = document.createElement("p");
      empty.className = "d12-muted";
      empty.dataset.d12Empty = "true";
      empty.textContent = copy.noGate;
      stage.append(empty);
      root.replaceChildren(stage);
      return;
    }

    const resolved = RESOLVED.has(detail.gate.status);
    const banner = document.createElement("div");
    banner.className = "d12-banner";
    banner.dataset.d12Banner = resolved ? "resolved" : "conflict";
    const bannerText = document.createElement("span");
    bannerText.textContent = resolved ? copy.resolved : copy.conflict;
    const strength = document.createElement("span");
    strength.className = "d12-strength";
    strength.textContent = resolved ? detail.gate.status : copy.strong;
    banner.append(bannerText, strength);
    stage.append(banner);

    const policy = document.createElement("p");
    policy.className = "d12-policy";
    policy.dataset.d12Policy = detail.gate.gateType;
    const validatorNote = detail.gate.requiresIndependentValidator
      ? `${copy.validator} · ${detail.gate.hasValidator ? copy.validatorPresent : copy.validatorMissing}`
      : "";
    policy.textContent = [detail.gate.gateType, detail.gate.projectId, detail.gate.laneId, validatorNote]
      .filter((part): part is string => Boolean(part))
      .join(" · ");
    stage.append(policy);

    if (detail.missingEvidence.length > 0) {
      const missing = document.createElement("p");
      missing.className = "d12-missing";
      missing.dataset.d12Missing = String(detail.missingEvidence.length);
      missing.textContent = `${copy.missing}: ${detail.missingEvidence.join(", ")}`;
      stage.append(missing);
    }

    stage.append(section("timeline"));
    const timeline = document.createElement("ol");
    timeline.className = "d12-timeline";
    if (detail.bounces.length === 0) {
      const none = document.createElement("li");
      none.className = "d12-muted";
      none.textContent = copy.noBounce;
      timeline.append(none);
    }
    for (const bounce of detail.bounces) {
      const item = document.createElement("li");
      item.dataset.d12Bounce = bounce.bounceId;
      item.dataset.d12BounceStatus = bounce.status;
      item.textContent = `${bounce.originalLaneId} · ${bounce.status} · ${bounce.reason}`;
      timeline.append(item);
    }
    stage.append(timeline);

    if (detail.checks.length > 0) {
      stage.append(section("checks"));
      const checks = document.createElement("ul");
      checks.className = "d12-checks";
      for (const check of detail.checks) {
        const item = document.createElement("li");
        item.dataset.d12Check = check.id;
        item.textContent = `${check.name} · ${check.status}`;
        checks.append(item);
      }
      stage.append(checks);
    }

    if (detail.reverts.length > 0) {
      stage.append(section("reverts"));
      const reverts = document.createElement("ul");
      reverts.className = "d12-reverts";
      for (const revert of detail.reverts) {
        const item = document.createElement("li");
        item.dataset.d12Revert = revert.revertId;
        const text = document.createElement("span");
        text.textContent = `${revert.reason} · ${revert.restoredPaths.join(", ")} · ${revert.auditId}`;
        item.append(text);
        // The audit id alone is not queryable: `AuditQuery` filters by object.
        // The affordance therefore carries the object Core actually linked.
        const scope = revert.auditScope;
        if (scope && onViewAuditTrail) {
          const trail = document.createElement("button");
          trail.type = "button";
          trail.className = "d12-trail";
          trail.dataset.d12AuditTrail = `${scope.kind}:${scope.id}`;
          trail.textContent = copy.auditTrail;
          trail.addEventListener("click", () => onViewAuditTrail(scope));
          item.append(trail);
        }
        reverts.append(item);
      }
      stage.append(reverts);
    }

    const bar2 = document.createElement("div");
    bar2.className = "d12-gatebar";

    const rejectAction = detail.actions.find((action) => action.kind === "reject");
    // Core refuses an empty rejection reason, so the client does too.
    const canBounce = (): boolean =>
      Boolean(rejectAction?.available) &&
      send !== undefined &&
      !busy &&
      bounceReason.trim().length > 0;

    // The design bounces *with a reason*: the reason field sits in the action
    // bar next to the bounce control, because Core stores it as the gate
    // decision and the origin Lane's agent works from it.
    const reason = document.createElement("input");
    reason.type = "text";
    reason.className = "d12-reason";
    reason.dataset.d12Reason = "true";
    reason.value = bounceReason;
    reason.placeholder = copy.reasonPlaceholder;
    reason.setAttribute("aria-label", copy.reasonLabel);
    reason.disabled = busy || !send || !rejectAction?.available;
    reason.addEventListener("input", () => {
      bounceReason = reason.value;
      // Keep the bounce control in step with the reason without re-rendering
      // the field the operator is typing into.
      const bounce = bar2.querySelector<HTMLButtonElement>("[data-d12-action='reject']");
      if (bounce) bounce.disabled = !canBounce();
    });
    bar2.append(reason);

    for (const action of detail.actions) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "d12-gbtn";
      button.dataset.d12Action = action.kind;
      // A control is enabled only when it can actually do something: Core
      // says the action is available, a host callback can carry it, and — for
      // a bounce — the operator supplied the mandatory reason.
      const operable = action.available && send !== undefined && !busy;
      button.disabled =
        action.kind === "reject" ? !canBounce() : !operable;
      button.textContent = action.available
        ? label(copy, action.kind)
        : `${label(copy, action.kind)} · ${label(copy, action.code ?? "")}`;
      if (action.code) button.dataset.d12ActionCode = action.code;
      if (operable) {
        button.addEventListener("click", () => {
          if (action.kind === "accept") {
            dispatch({ type: "accept", gateId: detail.gate.gateId });
            return;
          }
          if (action.kind === "reject" && bounceReason.trim().length > 0) {
            dispatch({
              type: "bounce",
              gateId: detail.gate.gateId,
              reason: bounceReason.trim(),
            });
          }
        });
      }
      bar2.append(button);
    }
    stage.append(bar2);

    if (decisionError) {
      const failure = document.createElement("p");
      failure.className = "d12-error";
      failure.dataset.d12Error = "true";
      failure.setAttribute("role", "alert");
      failure.textContent = decisionError;
      stage.append(failure);
    }

    for (const entry of projection.unavailable) {
      const note = document.createElement("p");
      note.className = "d12-unavailable";
      note.dataset.d12Unavailable = entry.code;
      note.textContent = `${label(copy, entry.key)} · ${entry.code}`;
      stage.append(note);
    }

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
