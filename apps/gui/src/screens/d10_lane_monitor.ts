import type { Locale } from "../i18n/catalog";
import type { D14AuditRow } from "./d14_audit_timeline";
import "./d10_lane_monitor.css";

/// D10 Lane monitor.
///
/// One card per Core Lane across every project. Gate strength, status, project
/// binding, and progress are published Core facts; the screen renders them and
/// declares what the contract does not carry.

export interface D10Unavailable {
  key: string;
  code: string;
}

export interface D10Agent {
  sessionId: string;
  agentId: string;
  model: string | null;
  status: string;
}

export interface D10Evidence {
  id: string;
  kind: string;
  summary: string;
}

export interface D10RunStats {
  /** Humanized on the host, so every frontend reads the same duration. */
  wallTime: string;
  wallTimeMs: number;
  runCount: number;
  diffBytes: number;
  /** `null` is Core's own best-effort absence, rendered as unknown. */
  lastExitCode: number | null;
}

export interface D10Lane {
  id: string;
  projectId: string | null;
  summary: string;
  role: string;
  route: string;
  gateStrength: string;
  mutationPolicy: string;
  status: string;
  awaitsHuman: boolean;
  branch: string | null;
  worktree: string | null;
  progress: number | null;
  agents: D10Agent[];
  evidence: D10Evidence[];
  tokenLimit: number | null;
  costLimitMicroUsd: number | null;
  /** `metered` or `blind`, from Core's `AgentRoute::cost_meterability`. */
  costMeterability: string;
  /** Bounded process facts for a cost-blind lane; `null` when Core observed
   * no run. Absence is absence: it is never rendered as a measured zero. */
  runStats: D10RunStats | null;
}

export interface D10LaneMonitorProjection {
  totalLanes: number;
  totalProjects: number;
  awaitingTotal: number;
  lanes: D10Lane[];
  unavailable: D10Unavailable[];
}

/**
 * The D10 event ticker, read from the Core audit timeline (GUI-CORE-014).
 *
 * Shape-compatible with `D14AuditProjection`, because D10 and D14 are two views
 * of one Core timeline: `QueryAudit` -> `AuditPageLoaded`. One bounded
 * newest-first page over the whole workspace, ordered by Core across projects
 * (the `audit-ordering` fixture is the canonical proof). The screen never
 * rebuilds a timeline by diffing successive snapshots.
 *
 * `capabilityAvailable`, `loaded`, and an empty `rows` are three different
 * facts and each gets its own line.
 */
export interface D10Events {
  rows: D14AuditRow[];
  loaded: boolean;
  capabilityAvailable: boolean;
  outcome: { state: string; reason: string | null };
}

export interface D10Controller {
  applyProjection: (projection: D10LaneMonitorProjection) => void;
  applyEvents: (events: D10Events) => void;
}

/// What Core answered a supervision command with, carried back verbatim.
export interface D10ActionOutcome {
  state: string;
  reason: string | null;
}

/**
 * The card's supervision actions (`D10` design, "Attach / Pause / Kill").
 *
 * Only two of the design's four controls exist on this contract, and the split
 * is deliberate rather than cosmetic:
 *
 * - **Attach** is not a Core command at all. It is the cockpit's own Lane
 *   selection — the same path the Lane rail and the tab strip take — followed
 *   by a return to the transcript, so "attach" means "put my conversation on
 *   that Lane" and nothing is sent to Core.
 * - **Stop** is `RuntimeCommand::CancelAgentSession` for the Lane's own Agent
 *   session, correlated by command id and reported from the answering event.
 * - **Pause** and **Kill** have no Core command. They render disabled and say
 *   so; hiding them would erase a designed control, and wiring either to
 *   `CancelAgentSession` would make one verb answer for three different
 *   promises.
 *
 * Absent while no host is bound, which disables the whole row rather than
 * offering controls that cannot reach Core.
 */
export interface D10LaneActions {
  /** Selects the Lane in the cockpit and returns to the transcript. */
  attach: (laneId: string) => void;
  /** Sends one `CancelAgentSession` for that Lane's Agent session. */
  stop: (laneId: string, sessionId: string) => Promise<D10ActionOutcome>;
}

type Copy = Record<string, string>;

/// Gate strength glyphs are the registered design marks for the three Core
/// values, not decoration: they say how far the lane's output can be trusted.
const GATE_GLYPH: Record<string, string> = {
  full: "●",
  cooperative: "◐",
  containment: "○",
};

const COPY: Record<Locale, Copy> = {
  en: {
    title: "Lane Monitor",
    lanes: "lanes",
    projects: "projects",
    awaiting: "awaiting you",
    all: "All",
    unbound: "no project binding",
    noProgress: "no Core task",
    noLanes: "Core published no Lane.",
    decide: "Open Decision Center",
    blind: "cost-blind route",
    blindHint:
      "Core sees no provider exchange on this route, so it publishes bounded run facts instead of a token or dollar figure.",
    runsUnobserved: "Core has observed no run on this lane yet.",
    wallTime: "wall time",
    runCount: "runs",
    diffBytes: "applied diff",
    exitCode: "last exit",
    exitCodeUnknown: "unknown",
    gate_full: "full gate · per-call interception",
    gate_cooperative: "cooperative · advisory plus worktree fence",
    gate_containment: "containment · worktree fence and exit diff only",
    events: "Event stream",
    eventsPending: "Reading the Core audit timeline\u2026",
    eventsEmpty: "Core published no audited event yet.",
    eventsUnavailable: "Core publishes no audit timeline, so the event stream is unavailable.",
    attach: "Attach",
    attachHint: "Selects this Lane in the cockpit and returns to the conversation.",
    stop: "Stop",
    stopHint: "Sends Core's CancelAgentSession for this Lane's Agent session.",
    pause: "Pause",
    kill: "Kill",
    pauseUnavailable: "Core publishes no pause command for an Agent session.",
    killUnavailable:
      "Core publishes no kill command for an Agent session; Stop cancels it through CancelAgentSession.",
    actionsUnavailable: "No host is bound, so no Lane command can be sent.",
    stopNoSession: "Core published no Agent session for this Lane, so there is nothing to stop.",
    stopAmbiguous:
      "Core published more than one Agent session for this Lane; the client will not choose one.",
    stopPending: "Waiting for Core to answer the stop command\u2026",
    stopAccepted: "Core accepted the stop command.",
    stopRejected: "Core refused the stop command.",
  },
  "zh-CN": {
    title: "Lane 监视器",
    lanes: "条 lane",
    projects: "个项目",
    awaiting: "项等你",
    all: "全部",
    unbound: "无项目绑定",
    noProgress: "无 Core 任务",
    noLanes: "Core 未发布任何 Lane。",
    decide: "打开决策中心",
    blind: "成本不可计量路由",
    blindHint: "Core 在该路由上看不到任何模型调用，因此只发布有界的运行事实，而不是 token 或金额。",
    runsUnobserved: "Core 尚未在该 Lane 上观测到任何运行。",
    wallTime: "累计耗时",
    runCount: "运行次数",
    diffBytes: "已应用 diff",
    exitCode: "最近退出码",
    exitCodeUnknown: "未知",
    gate_full: "强门控 · 逐调用拦截",
    gate_cooperative: "半合作 · 建议性 + worktree 兜底",
    gate_containment: "围栏兜底 · 仅 worktree + 退出 diff",
    events: "事件流",
    eventsPending: "正在读取 Core 审计时间线…",
    eventsEmpty: "Core 尚未发布任何审计事件。",
    eventsUnavailable: "Core 未发布审计时间线，事件流不可用。",
    attach: "接管",
    attachHint: "在驾驶舱中选中该 Lane 并返回对话。",
    stop: "停止",
    stopHint: "为该 Lane 的 Agent 会话发送 Core 的 CancelAgentSession。",
    pause: "暂停",
    kill: "终止",
    pauseUnavailable: "Core 未发布针对 Agent 会话的暂停命令。",
    killUnavailable: "Core 未发布针对 Agent 会话的终止命令；“停止”通过 CancelAgentSession 取消会话。",
    actionsUnavailable: "未绑定宿主，无法发送任何 Lane 命令。",
    stopNoSession: "Core 未为该 Lane 发布 Agent 会话，没有可停止的对象。",
    stopAmbiguous: "Core 为该 Lane 发布了多个 Agent 会话；客户端不会替你选择其中之一。",
    stopPending: "等待 Core 回应停止命令…",
    stopAccepted: "Core 已接受停止命令。",
    stopRejected: "Core 拒绝了停止命令。",
  },
};

function label(copy: Copy, key: string): string {
  return copy[key] ?? key;
}

export function renderD10LaneMonitor(
  root: HTMLElement,
  initial: D10LaneMonitorProjection,
  locale: Locale,
  openDecisionCenter?: () => void,
  initialEvents: D10Events | null = null,
  actions?: D10LaneActions,
): D10Controller {
  let projection = initial;
  let events = initialEvents;
  let filter = "all";
  /**
   * The last stop outcome per Lane, keyed by Lane id. Presentation state only:
   * the durable fact is Core's own event, which the next projection carries.
   */
  const stopState = new Map<string, { state: string; reason: string | null }>();
  const copy = COPY[locale];

  /**
   * Which Agent session `Stop` would cancel, or why it would not.
   *
   * Cardinality is decided fail-closed, the way the cockpit decides its owner
   * binding: exactly one published session is a target, zero is nothing to
   * stop, and more than one is a choice the client refuses to make on the
   * operator's behalf.
   */
  const stopTarget = (
    lane: D10Lane,
  ): { sessionId: string } | { sessionId: null; reasonKey: string } => {
    if (!actions) return { sessionId: null, reasonKey: "actionsUnavailable" };
    if (lane.agents.length === 0) return { sessionId: null, reasonKey: "stopNoSession" };
    if (lane.agents.length > 1) return { sessionId: null, reasonKey: "stopAmbiguous" };
    return { sessionId: lane.agents[0].sessionId };
  };

  /**
   * One action button. A control with nothing behind it is rendered disabled
   * and carries the reason in the name a screen reader announces, never hidden
   * and never left enabled and inert.
   */
  const actionButton = (
    kind: string,
    label: string,
    reason: string | null,
    onClick?: () => void,
  ): HTMLButtonElement => {
    const element = document.createElement("button");
    element.type = "button";
    element.className = "d10-action";
    element.dataset.d10Action = kind;
    element.textContent = label;
    const name = reason ? `${label} — ${reason}` : label;
    element.title = name;
    element.setAttribute("aria-label", name);
    if (reason || !onClick) {
      element.disabled = true;
      element.dataset.d10ActionDisabled = "true";
      return element;
    }
    element.addEventListener("click", onClick);
    return element;
  };

  /**
   * The design's card action row, with the two controls this contract has and
   * the two it does not.
   */
  const actionRow = (lane: D10Lane): HTMLElement => {
    const row = document.createElement("div");
    row.className = "d10-actions";
    row.dataset.d10Actions = lane.id;

    row.append(
      actionButton(
        "attach",
        copy.attach,
        actions ? null : copy.actionsUnavailable,
        actions ? () => actions.attach(lane.id) : undefined,
      ),
    );

    const target = stopTarget(lane);
    if (target.sessionId === null) {
      row.append(actionButton("stop", copy.stop, label(copy, target.reasonKey)));
    } else {
      const sessionId = target.sessionId;
      const pending = stopState.get(lane.id)?.state === "pending";
      const stop = actionButton(
        "stop",
        copy.stop,
        pending ? copy.stopPending : null,
        pending
          ? undefined
          : () => {
              if (!actions) return;
              // The command id is minted by the host; the screen only reports
              // what the answering event said about it.
              stopState.set(lane.id, { state: "pending", reason: null });
              render();
              void actions
                .stop(lane.id, sessionId)
                .then((outcome) => {
                  stopState.set(lane.id, {
                    state: outcome.state,
                    reason: outcome.reason,
                  });
                })
                .catch((error: unknown) => {
                  // A host failure is reported as a refusal with its own
                  // words, never as a silent success.
                  stopState.set(lane.id, {
                    state: "rejected",
                    reason: error instanceof Error ? error.message : String(error),
                  });
                })
                .finally(render);
            },
      );
      if (!pending) stop.dataset.d10StopSession = sessionId;
      row.append(stop);
    }

    // Both are registered design controls with no Core command behind them.
    row.append(actionButton("pause", copy.pause, copy.pauseUnavailable));
    row.append(actionButton("kill", copy.kill, copy.killUnavailable));

    const outcome = stopState.get(lane.id);
    if (outcome) {
      const note = document.createElement("p");
      note.className = "d10-muted";
      note.dataset.d10StopOutcome = outcome.state;
      note.setAttribute("role", "status");
      const headline =
        outcome.state === "pending"
          ? copy.stopPending
          : outcome.state === "rejected"
            ? copy.stopRejected
            : copy.stopAccepted;
      // Core's own words for a refusal; the client composes no reason of its own.
      note.textContent = outcome.reason ? `${headline} ${outcome.reason}` : headline;
      row.append(note);
    }
    return row;
  };

  const laneCard = (lane: D10Lane): HTMLElement => {
    const card = document.createElement("article");
    card.className = "d10-card";
    card.dataset.d10Lane = lane.id;
    card.dataset.d10Status = lane.status;
    if (lane.awaitsHuman) card.dataset.d10Attention = "true";

    const head = document.createElement("div");
    head.className = "d10-chead";
    const id = document.createElement("span");
    id.className = "d10-lid";
    id.textContent = lane.id;
    const summary = document.createElement("span");
    summary.className = "d10-summary";
    summary.textContent = lane.summary;
    head.append(id, summary);

    const meta = document.createElement("div");
    meta.className = "d10-meta";
    const project = document.createElement("span");
    project.className = "d10-project";
    project.dataset.d10Project = lane.projectId ?? "";
    project.textContent = lane.projectId ?? copy.unbound;
    meta.append(project);
    for (const agent of lane.agents) {
      const chip = document.createElement("span");
      chip.className = "d10-chip";
      chip.textContent = agent.model ? `${agent.agentId} · ${agent.model}` : agent.agentId;
      meta.append(chip);
    }
    const gate = document.createElement("span");
    gate.className = "d10-gate";
    gate.dataset.d10Gate = lane.gateStrength;
    gate.title = label(copy, `gate_${lane.gateStrength}`);
    gate.textContent = `${GATE_GLYPH[lane.gateStrength] ?? "?"} ${lane.gateStrength}`;
    const status = document.createElement("span");
    status.className = "d10-status";
    status.textContent = lane.status;
    meta.append(gate, status);

    const progress = document.createElement("div");
    progress.className = "d10-progress";
    if (lane.progress === null) {
      progress.dataset.d10Progress = "none";
      progress.textContent = copy.noProgress;
    } else {
      progress.dataset.d10Progress = String(lane.progress);
      const fill = document.createElement("i");
      fill.style.width = `${lane.progress}%`;
      progress.append(fill);
    }

    card.append(head, meta, progress);

    if (lane.costMeterability === "blind") {
      // The marker is a Core fact, not a warning badge: it says this lane's
      // cost is unobservable, which is why the row below carries process facts
      // instead of a token or dollar figure.
      const cost = document.createElement("div");
      cost.className = "d10-cost";
      cost.dataset.d10Meterability = "blind";
      const marker = document.createElement("span");
      marker.className = "d10-blind";
      marker.title = copy.blindHint;
      marker.textContent = copy.blind;
      cost.append(marker);

      if (lane.runStats) {
        const stats = lane.runStats;
        const facts: [string, string][] = [
          [copy.wallTime, stats.wallTime],
          [copy.runCount, String(stats.runCount)],
          [copy.diffBytes, `${stats.diffBytes} B`],
          [
            copy.exitCode,
            // A missing exit code is labelled, never defaulted to 0: Core
            // publishes `null` for a force-kill, a still-running process, or a
            // tmux session with no exit-code channel.
            stats.lastExitCode === null ? copy.exitCodeUnknown : String(stats.lastExitCode),
          ],
        ];
        for (const [name, value] of facts) {
          const fact = document.createElement("span");
          fact.className = "d10-runfact";
          fact.dataset.d10RunFact = name;
          fact.textContent = `${name} ${value}`;
          cost.append(fact);
        }
      } else {
        // Absence is absence. An unobserved lane must not be drawn as a
        // measured zero, which is a different Core fact.
        const none = document.createElement("span");
        none.className = "d10-runfact d10-muted";
        none.dataset.d10RunStats = "none";
        none.textContent = copy.runsUnobserved;
        cost.append(none);
      }
      card.append(cost);
    }

    for (const entry of lane.evidence) {
      const row = document.createElement("p");
      row.className = "d10-evidence";
      row.dataset.d10Evidence = entry.id;
      row.textContent = `${entry.kind} · ${entry.summary}`;
      card.append(row);
    }

    card.append(actionRow(lane));

    if (lane.awaitsHuman && openDecisionCenter) {
      const action = document.createElement("button");
      action.type = "button";
      action.className = "d10-action";
      action.dataset.d10Action = "decide";
      action.textContent = `${copy.decide} ↗`;
      action.addEventListener("click", openDecisionCenter);
      card.append(action);
    }
    return card;
  };

  /**
   * The ambient event ticker: one bounded newest-first page of the Core audit
   * timeline, rendered exactly as Core delivered it.
   *
   * Every row carries the stable audit id, Core's dotted action key (never
   * localized), the owning project and Lane, and the timestamp — the four
   * facts GUI-CORE-014's close criteria name. Rows carry no action: this is
   * ambient content, and the Decision Center owns the actionable queue.
   */
  const ticker = (): HTMLElement => {
    const section = document.createElement("section");
    section.className = "d10-ticker";
    section.dataset.d10Ticker = "true";
    const heading = document.createElement("h3");
    heading.className = "d10-thead";
    heading.textContent = copy.events;
    section.append(heading);

    const note = (text: string, state: string): HTMLElement => {
      const line = document.createElement("p");
      line.className = "d10-muted";
      line.dataset.d10TickerState = state;
      line.textContent = text;
      return line;
    };
    if (!events || !events.capabilityAvailable) {
      section.append(note(copy.eventsUnavailable, "unavailable"));
      return section;
    }
    if (events.outcome.state === "rejected") {
      // Core's own words for the refusal, never a client paraphrase.
      section.append(note(events.outcome.reason ?? copy.eventsUnavailable, "rejected"));
      return section;
    }
    if (!events.loaded) {
      section.append(note(copy.eventsPending, "pending"));
      return section;
    }
    if (events.rows.length === 0) {
      section.append(note(copy.eventsEmpty, "empty"));
      return section;
    }
    const list = document.createElement("ol");
    list.className = "d10-tlist";
    for (const row of events.rows) {
      const item = document.createElement("li");
      item.className = "d10-trow";
      item.dataset.d10Event = row.auditId;
      item.dataset.d10EventProject = row.projectId;

      const when = document.createElement("span");
      when.className = "d10-ttime";
      // Core's unix seconds, rendered in the viewer's locale. The ordering is
      // Core's; the client only formats.
      when.textContent = new Date(row.timestamp * 1000).toISOString().slice(11, 19);

      const kind = document.createElement("span");
      kind.className = "d10-tkind";
      // Core's stable dotted key, raw. Localizing it would make the timeline
      // undiffable across languages.
      kind.textContent = row.action;

      const owner = document.createElement("span");
      owner.className = "d10-towner";
      owner.textContent = row.laneId ?? row.projectId ?? copy.unbound;

      const outcome = document.createElement("span");
      outcome.className = "d10-toutcome";
      outcome.dataset.d10Outcome = row.outcome;
      outcome.textContent = row.outcome;

      item.append(when, kind, owner, outcome);
      list.append(item);
    }
    section.append(list);
    return section;
  };

  const render = (): void => {
    const stage = document.createElement("section");
    stage.className = "d10-stage";
    stage.dataset.route = "d10";

    const bar = document.createElement("div");
    bar.className = "d10-head";
    const heading = document.createElement("h2");
    heading.className = "d10-title";
    heading.textContent = copy.title;
    const counts = document.createElement("span");
    counts.className = "d10-counts";
    counts.dataset.d10Counts = `${projection.totalLanes}/${projection.totalProjects}/${projection.awaitingTotal}`;
    counts.textContent = `${projection.totalLanes} ${copy.lanes} · ${projection.totalProjects} ${copy.projects} · ${projection.awaitingTotal} ${copy.awaiting}`;
    bar.append(heading, counts);

    const projects = [
      ...new Set(
        projection.lanes
          .map((lane) => lane.projectId)
          .filter((id): id is string => typeof id === "string"),
      ),
    ];
    const chips = document.createElement("div");
    chips.className = "d10-fchips";
    for (const key of ["all", ...projects]) {
      const chip = document.createElement("button");
      chip.type = "button";
      chip.className = "d10-fchip";
      chip.dataset.d10Filter = key;
      chip.setAttribute("aria-pressed", String(filter === key));
      chip.textContent = key === "all" ? copy.all : key;
      chip.addEventListener("click", () => {
        filter = key;
        render();
      });
      chips.append(chip);
    }
    bar.append(chips);
    stage.append(bar);

    const grid = document.createElement("div");
    grid.className = "d10-grid";
    const shown = projection.lanes.filter(
      (lane) => filter === "all" || lane.projectId === filter,
    );
    if (shown.length === 0) {
      const empty = document.createElement("p");
      empty.className = "d10-muted";
      empty.dataset.d10Empty = "true";
      empty.textContent = copy.noLanes;
      grid.append(empty);
    }
    for (const lane of shown) grid.append(laneCard(lane));
    stage.append(grid);

    stage.append(ticker());

    for (const entry of projection.unavailable) {
      const note = document.createElement("p");
      note.className = "d10-unavailable";
      note.dataset.d10Unavailable = entry.code;
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
    applyEvents: (next) => {
      events = next;
      render();
    },
  };
}
