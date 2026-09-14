import type { Locale } from "../i18n/catalog";
import type {
  D6ActionProjection,
  D6Intent,
  D6IntentResult,
  D6RecoveryProjection,
  D6State,
} from "../models/workspace";
import "./d6_recovery.css";

export type {
  D6ActionProjection,
  D6Intent,
  D6IntentResult,
  D6RecoveryProjection,
} from "../models/workspace";

/** Sends one Core-backed recovery action; absent while no host is bound. */
export type SendD6Intent = (intent: D6Intent) => Promise<D6IntentResult>;

/**
 * Resolves the Core intent an action stands for, or null when the projection
 * named no target. A recovery action never invents an id, so an action Core
 * marked available but did not target stays inert rather than guessing one.
 */
/** Localized label for an action kind, falling back to the protocol name. */
function actionLabel(copy: (typeof COPY)[Locale], kind: string): string {
  const label = copy[kind as keyof typeof copy];
  return typeof label === "string" ? label : kind;
}

function intentFor(action: D6ActionProjection): D6Intent | null {
  if (action.kind === "restart" && action.sessionId) {
    return { kind: "restart", sessionId: action.sessionId };
  }
  if (action.kind === "close_lane" && action.laneId) {
    return { kind: "close_lane", laneId: action.laneId };
  }
  return null;
}

const COPY = {
  en: {
    empty: ["EMPTY", "No Core-owned Lane is available yet."],
    connecting: ["CONNECTING", "Establishing the versioned Core connection."],
    disconnected: ["DISCONNECTED", "The Core transport is unavailable."],
    provider_error: ["PROVIDER ERROR", "Core reports a provider failure."],
    agent_stopped: ["AGENT STOPPED", "Core reports that execution has stopped."],
    context_overflow: ["CONTEXT OVERFLOW", "The Core context hard limit was exceeded."],
    gate_queue_clear: ["QUEUE CLEAR", "Core reports no pending approvals or gates."],
    incompatible_schema: ["INCOMPATIBLE", "The frontend schema is incompatible with Core."],
    missing_feature_capability: ["CAPABILITY MISSING", "Core did not advertise a required capability."],
    event_gap: ["EVENT GAP", "Ordered events are incomplete; snapshot recovery is required."],
    live: ["LIVE", "Core state is current."],
    facts: "Core facts",
    reconnect: "Reconnect",
    inspect: "Inspect facts",
    restart: "Restart agent",
    close_lane: "Close Lane",
    checkpoint: "Restore checkpoint",
    open_project: "Open project",
    tokens: "Tokens",
    missing: "Missing",
    recovered: "Core snapshot is live.",
    diagnostics: "Diagnostics",
    connection_label: "Connection",
    state_label: "State",
    hint_label: "Hint",
    reconnect_attempt: "Reconnect attempt {attempt} of {max}.",
    reconnect_next: "Next attempt in {seconds}s.",
    reconnect_running: "Reconnecting now.",
    reconnect_gave_up: "The host stopped reconnecting after {attempts} attempts.",
  },
  "zh-CN": {
    empty: ["空工作区", "Core 尚未提供可用 Lane。"],
    connecting: ["连接中", "正在建立版本化 Core 连接。"],
    disconnected: ["已断开", "Core 传输当前不可用。"],
    provider_error: ["提供方错误", "Core 报告提供方故障。"],
    agent_stopped: ["智能体已停止", "Core 报告执行已停止。"],
    context_overflow: ["上下文溢出", "已超过 Core 上下文硬限制。"],
    gate_queue_clear: ["队列已清空", "Core 报告没有待处理批准或门禁。"],
    incompatible_schema: ["协议不兼容", "前端 schema 与 Core 不兼容。"],
    missing_feature_capability: ["能力缺失", "Core 未声明所需能力。"],
    event_gap: ["事件缺口", "有序事件不完整，需要快照恢复。"],
    live: ["在线", "Core 状态为最新。"],
    facts: "Core 事实",
    reconnect: "重新连接",
    inspect: "检查事实",
    restart: "重启智能体",
    close_lane: "关闭 Lane",
    checkpoint: "恢复检查点",
    open_project: "打开项目",
    tokens: "Token",
    missing: "缺失能力",
    recovered: "Core 快照已恢复在线。",
    diagnostics: "诊断",
    connection_label: "连接",
    state_label: "状态",
    hint_label: "提示",
    reconnect_attempt: "第 {attempt} 次重连尝试，共 {max} 次。",
    reconnect_next: "{seconds} 秒后重试。",
    reconnect_running: "正在重新连接。",
    reconnect_gave_up: "主机在 {attempts} 次尝试后停止重连。",
  },
} as const;

export interface D6Controller {
  applyProjection: (projection: D6RecoveryProjection) => void;
}

export function renderD6Recovery(
  root: HTMLElement,
  initial: D6RecoveryProjection,
  reconnect: () => Promise<D6RecoveryProjection>,
  locale: Locale,
  openProject?: () => void,
  sendIntent?: SendD6Intent,
): D6Controller {
  let projection = initial;
  // One in-flight recovery action at a time: every Core-backed action blocks
  // the surface until the host returns Core's own projection.
  let recovering = false;
  // `inspect` is presentation-only; expanding the facts must never look like a
  // Core state change, so it lives outside the projection.
  let inspecting = false;
  // A rejected action (for example a target that vanished between render and
  // click) must fail visibly: the host's rejection message renders as an
  // alert instead of dying as an unhandled promise rejection.
  let actionError: string | null = null;
  const copy = COPY[locale];

  /** Runs one Core-backed action and re-renders from what Core published. */
  const dispatch = (run: () => Promise<D6RecoveryProjection>): void => {
    if (recovering) return;
    recovering = true;
    actionError = null;
    render();
    void run()
      .then((next) => {
        // Success is rendered only after the callback returns Core's projection.
        projection = next;
      })
      .catch((error: unknown) => {
        actionError = error instanceof Error ? error.message : String(error);
      })
      .finally(() => {
        recovering = false;
        render();
      });
  };

  /**
   * Expands the facts this projection already carries. When Core published no
   * detail or hint, the typed state and the per-action diagnostic codes are
   * still renderable, so `inspect` always shows why an action is unavailable
   * rather than opening an empty panel.
   */
  const renderInspectDetails = (): HTMLElement => {
    const details = document.createElement("section");
    details.id = "d6-inspect";
    details.className = "d6-inspect";
    details.dataset.d6Inspect = "true";
    const heading = document.createElement("h3");
    heading.textContent = copy.diagnostics;
    const list = document.createElement("dl");
    const rows: Array<[string, string]> = [
      [copy.state_label, projection.state],
      [copy.connection_label, projection.connection],
    ];
    if (projection.detail) rows.push([copy.facts, projection.detail]);
    if (projection.hint) rows.push([copy.hint_label, projection.hint]);
    if (projection.missingCapabilities.length > 0) {
      rows.push([copy.missing, projection.missingCapabilities.join(", ")]);
    }
    for (const action of projection.actions) {
      rows.push([actionLabel(copy, action.kind), action.code]);
    }
    for (const [term, value] of rows) {
      const dt = document.createElement("dt");
      dt.textContent = term;
      const dd = document.createElement("dd");
      dd.textContent = value;
      list.append(dt, dd);
    }
    details.append(heading, list);
    return details;
  };

  const render = (): void => {
    const stateCopy = copy[projection.state as D6State];
    const stage = document.createElement("section");
    stage.className = "d6-stage";
    stage.dataset.d6State = projection.state;
    stage.setAttribute("aria-live", "polite");
    stage.setAttribute("aria-busy", String(recovering));

    const tag = document.createElement("span");
    tag.className = "d6-tag";
    tag.dataset.d6Tag = "true";
    tag.textContent = `◌ ${stateCopy[0]}`;
    const title = document.createElement("h2");
    title.textContent = stateCopy[1];
    const detail = document.createElement("p");
    detail.className = "d6-detail";
    detail.textContent = projection.detail ?? projection.hint ?? copy.facts;
    stage.append(tag, title, detail);

    // E3d defect 37. `CONNECTING` on its own is a spinner: it says something is
    // happening and never says whether anything still is. The host's own retry
    // schedule is rendered beside it, ending in the sentence that says it
    // stopped — an operator can act on "it gave up" and cannot act on a dot.
    const retry = projection.reconnect;
    if (retry) {
      const line = document.createElement("p");
      line.className = "d6-detail";
      line.dataset.d6Reconnect = "true";
      line.setAttribute("role", "status");
      if (retry.gaveUp) {
        line.dataset.d6ReconnectGaveUp = "true";
        line.textContent = copy.reconnect_gave_up.replace(
          "{attempts}",
          String(retry.attempts),
        );
      } else {
        const attempt = copy.reconnect_attempt
          .replace("{attempt}", String(retry.attempts))
          .replace("{max}", String(retry.maxAttempts));
        const next =
          retry.nextAttemptInMs === null
            ? copy.reconnect_running
            : copy.reconnect_next.replace(
                "{seconds}",
                String(Math.max(1, Math.ceil(retry.nextAttemptInMs / 1000))),
              );
        line.textContent = `${attempt} ${next}`;
      }
      stage.append(line);
      if (retry.lastError) {
        const cause = document.createElement("p");
        cause.className = "d6-detail";
        cause.dataset.d6ReconnectError = "true";
        // The host's own words, unedited: an operator told only that something
        // failed has nothing to look up.
        cause.textContent = retry.lastError;
        stage.append(cause);
      }
    }

    const facts = document.createElement("dl");
    facts.className = "d6-facts";
    if (projection.usedTokens !== null || projection.hardTokenLimit !== null) {
      const dt = document.createElement("dt");
      dt.textContent = copy.tokens;
      const dd = document.createElement("dd");
      dd.textContent = `${projection.usedTokens?.toLocaleString(locale) ?? "—"} / ${projection.hardTokenLimit?.toLocaleString(locale) ?? "—"}`;
      facts.append(dt, dd);
    }
    if (projection.missingCapabilities.length > 0) {
      const dt = document.createElement("dt");
      dt.textContent = copy.missing;
      const dd = document.createElement("dd");
      dd.textContent = projection.missingCapabilities.join(", ");
      facts.append(dt, dd);
    }
    if (facts.children.length > 0) stage.append(facts);

    const actions = document.createElement("div");
    actions.className = "d6-actions";
    for (const action of projection.actions) {
      const actionButton = document.createElement("button");
      actionButton.type = "button";
      actionButton.dataset.d6Action = action.kind;
      const intent = intentFor(action);
      // A control is enabled only when it can actually do something: Core said
      // the action is available, it named a target, and a host can carry it.
      const operable = action.available
        && (action.kind === "inspect"
          || (action.kind === "reconnect" ? true : intent !== null && sendIntent !== undefined));
      actionButton.disabled = !operable || recovering;
      const label = actionLabel(copy, action.kind);
      actionButton.textContent = `${label}${action.available ? "" : ` · ${action.code}`}`;
      if (operable && action.kind === "reconnect") {
        actionButton.addEventListener("click", () => dispatch(reconnect));
      } else if (operable && action.kind === "inspect") {
        // Local affordance only: it expands facts already in the projection.
        actionButton.setAttribute("aria-expanded", String(inspecting));
        actionButton.setAttribute("aria-controls", "d6-inspect");
        actionButton.addEventListener("click", () => {
          inspecting = !inspecting;
          render();
        });
      } else if (operable && intent && sendIntent) {
        actionButton.addEventListener("click", () => {
          dispatch(async () => (await sendIntent(intent)).projection);
        });
      }
      actions.append(actionButton);
    }
    if (projection.state === "empty" && openProject) {
      const openProjectButton = document.createElement("button");
      openProjectButton.type = "button";
      openProjectButton.dataset.openProject = "true";
      openProjectButton.textContent = copy.open_project;
      openProjectButton.addEventListener("click", openProject);
      actions.prepend(openProjectButton);
    }
    stage.append(actions);
    if (actionError) {
      const failure = document.createElement("p");
      failure.dataset.d6Error = "true";
      failure.setAttribute("role", "alert");
      failure.textContent = actionError;
      stage.append(failure);
    }
    if (inspecting) stage.append(renderInspectDetails());
    if (projection.state === "live" && projection.connection === "live") {
      const success = document.createElement("p");
      success.dataset.d6Success = "true";
      success.setAttribute("role", "status");
      success.textContent = `✓ ${copy.recovered}`;
      stage.append(success);
    }
    root.replaceChildren(stage);
  };

  const controller: D6Controller = {
    applyProjection: (next) => {
      projection = next;
      if (next.connection === "live") recovering = false;
      render();
    },
  };
  render();
  return controller;
}
