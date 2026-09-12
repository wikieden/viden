import type { Locale } from "../i18n/catalog";
import "./d13_fleet_workflow.css";

/// D13 fleet and workflow.
///
/// One column per Core workflow DAG. Edges are the task specs' own dependency
/// lists, a node shows runtime status only when Core runs that task, and a
/// blocker is shown only when Core recorded a blocked dependency.

export interface D13Blocker {
  dependencyId: string;
  dependsOnTaskId: string;
  reason: string;
  auditId: string;
  updatedAt: number;
}

export interface D13Node {
  taskId: string;
  title: string;
  objective: string;
  role: string;
  dependsOn: string[];
  requiredEvidence: string[];
  permissionPolicy: string;
  status: string | null;
  progress: number | null;
  blocked: boolean;
  blockers: D13Blocker[];
  /**
   * Every Lane Core bound to this task, in the projection's own order.
   *
   * Core publishes no "node -> Lane" edge; it publishes each Lane's own
   * `task_id`, and the GUI projection joins on it. The list is what Core said,
   * not a choice: zero, one, and several are three different facts and the
   * screen answers each of them differently.
   */
  laneIds: string[];
}

export interface D13Workflow {
  dagId: string;
  goal: string;
  status: string;
  createdAt: number | null;
  updatedAt: number | null;
  nodes: D13Node[];
}

export interface D13Handoff {
  handoffId: string;
  taskId: string;
  fromLaneId: string;
  toLaneId: string;
  summary: string;
  auditId: string;
}

export interface D13FleetWorkflowProjection {
  workflows: D13Workflow[];
  handoffs: D13Handoff[];
}

export interface D13Controller {
  applyProjection: (projection: D13FleetWorkflowProjection) => void;
}

type Copy = Record<string, string>;

const COPY: Record<Locale, Copy> = {
  en: {
    title: "Fleet and workflow",
    empty: "Core published no workflow.",
    notStarted: "not started",
    dependsOn: "depends on",
    blockedBy: "blocked by",
    evidence: "required evidence",
    handoffs: "Handoffs",
    noHandoff: "Core recorded no handoff.",
    openLane: "Open Lane",
    noLane: "Core bound no Lane to this task, so there is nothing to open.",
    ambiguousLane:
      "Core bound more than one Lane to this task; the client will not choose one.",
    laneUnavailable: "No host is bound, so this node cannot open a Lane.",
  },
  "zh-CN": {
    title: "Fleet 编排与 Workflow",
    empty: "Core 未发布任何 workflow。",
    notStarted: "未启动",
    dependsOn: "依赖",
    blockedBy: "被阻塞于",
    evidence: "必需证据",
    handoffs: "交接",
    noHandoff: "Core 未记录任何交接。",
    openLane: "打开 Lane",
    noLane: "Core 未为该任务绑定任何 Lane，没有可打开的对象。",
    ambiguousLane: "Core 为该任务绑定了多个 Lane；客户端不会替你选择其中之一。",
    laneUnavailable: "未绑定宿主，该节点无法打开 Lane。",
  },
};

export function renderD13FleetWorkflow(
  root: HTMLElement,
  initial: D13FleetWorkflowProjection,
  locale: Locale,
  /**
   * Selects the Lane a node's task is bound to and returns the cockpit to the
   * transcript — the same selection path the Lane rail and the tab strip take.
   * Absent while no host is bound, which states the node as non-navigating
   * rather than leaving a card that looks clickable and does nothing.
   */
  onOpenLane?: (laneId: string) => void,
): D13Controller {
  let projection = initial;
  const copy = COPY[locale];

  /** One muted line saying why this node opens nothing. Absence, not error. */
  const drillNote = (text: string): HTMLElement => {
    const note = document.createElement("p");
    note.className = "d13-muted";
    note.dataset.d13DrillNote = "true";
    note.textContent = text;
    return note;
  };

  const node = (item: D13Node): HTMLElement => {
    const card = document.createElement("article");
    card.className = "d13-node";
    card.dataset.d13Node = item.taskId;
    if (item.blocked) card.dataset.d13Blocked = "true";

    const head = document.createElement("div");
    head.className = "d13-nhead";
    const title = document.createElement("span");
    title.className = "d13-ntitle";
    title.textContent = item.title;
    const role = document.createElement("span");
    role.className = "d13-chip";
    role.textContent = item.role;
    const status = document.createElement("span");
    status.className = "d13-status";
    status.dataset.d13Status = item.status ?? "none";
    // A planned node is stated as not started, never faked as queued work.
    status.textContent = item.status ?? copy.notStarted;
    head.append(title, role, status);
    card.append(head);

    const objective = document.createElement("p");
    objective.className = "d13-objective";
    objective.textContent = item.objective;
    card.append(objective);

    if (item.dependsOn.length > 0) {
      const deps = document.createElement("p");
      deps.className = "d13-meta";
      deps.dataset.d13DependsOn = item.dependsOn.join(",");
      deps.textContent = `${copy.dependsOn}: ${item.dependsOn.join(", ")}`;
      card.append(deps);
    }
    if (item.requiredEvidence.length > 0) {
      const evidence = document.createElement("p");
      evidence.className = "d13-meta";
      evidence.textContent = `${copy.evidence}: ${item.requiredEvidence.join(", ")}`;
      card.append(evidence);
    }
    for (const blocker of item.blockers) {
      const row = document.createElement("p");
      row.className = "d13-blocker";
      row.dataset.d13Blocker = blocker.dependencyId;
      row.textContent = `${copy.blockedBy} ${blocker.dependsOnTaskId} · ${blocker.reason}`;
      card.append(row);
    }
    if (item.progress !== null) {
      const progress = document.createElement("div");
      progress.className = "d13-progress";
      progress.dataset.d13Progress = String(item.progress);
      const fill = document.createElement("i");
      fill.style.width = `${item.progress}%`;
      progress.append(fill);
      card.append(progress);
    }

    // The drill. Exactly one bound Lane is a destination; anything else is a
    // stated fact, because a node that cannot be opened must say so rather
    // than look like one that can.
    if (!onOpenLane) {
      card.dataset.d13Drill = "unavailable";
      card.append(drillNote(copy.laneUnavailable));
    } else if (item.laneIds.length === 0) {
      card.dataset.d13Drill = "none";
      card.append(drillNote(copy.noLane));
    } else if (item.laneIds.length > 1) {
      card.dataset.d13Drill = "ambiguous";
      card.append(drillNote(`${copy.ambiguousLane} ${item.laneIds.join(", ")}`));
    } else {
      const laneId = item.laneIds[0];
      card.dataset.d13Drill = laneId;
      card.setAttribute("role", "button");
      card.tabIndex = 0;
      const name = `${copy.openLane} ${laneId}`;
      card.title = name;
      card.setAttribute("aria-label", `${item.title} — ${name}`);
      // A visible cue, not only a cursor: the drill has to be legible in a
      // still frame and to a reader who never hovers, and it names the exact
      // Lane Core bound so the destination is known before the click.
      const open = document.createElement("p");
      open.className = "d13-drill-open";
      open.dataset.d13DrillOpen = laneId;
      open.textContent = `${name} ↗`;
      card.append(open);
      card.addEventListener("click", () => onOpenLane(laneId));
      card.addEventListener("keydown", (event) => {
        // Enter only: Space is the webview's own scroll on a non-button
        // element, and taking it would cost more than the chord is worth.
        if (event.key !== "Enter") return;
        event.preventDefault();
        onOpenLane(laneId);
      });
    }
    return card;
  };

  const render = (): void => {
    const stage = document.createElement("section");
    stage.className = "d13-stage";
    stage.dataset.route = "d13";

    const head = document.createElement("div");
    head.className = "d13-head";
    const heading = document.createElement("h2");
    heading.className = "d13-title";
    heading.textContent = copy.title;
    head.append(heading);
    stage.append(head);

    const board = document.createElement("div");
    board.className = "d13-board";
    if (projection.workflows.length === 0) {
      const empty = document.createElement("p");
      empty.className = "d13-muted";
      empty.dataset.d13Empty = "true";
      empty.textContent = copy.empty;
      board.append(empty);
    }
    for (const workflow of projection.workflows) {
      const column = document.createElement("div");
      column.className = "d13-column";
      column.dataset.d13Workflow = workflow.dagId;
      column.dataset.d13WorkflowStatus = workflow.status;

      const goal = document.createElement("h3");
      goal.className = "d13-goal";
      goal.textContent = `${workflow.goal} · ${workflow.status}`;
      column.append(goal);
      for (const item of workflow.nodes) column.append(node(item));
      board.append(column);
    }
    stage.append(board);

    const handoffs = document.createElement("div");
    handoffs.className = "d13-handoffs";
    const handoffHeading = document.createElement("div");
    handoffHeading.className = "d13-sec";
    handoffHeading.textContent = copy.handoffs;
    handoffs.append(handoffHeading);
    if (projection.handoffs.length === 0) {
      const none = document.createElement("p");
      none.className = "d13-muted";
      none.dataset.d13NoHandoff = "true";
      none.textContent = copy.noHandoff;
      handoffs.append(none);
    }
    for (const handoff of projection.handoffs) {
      const row = document.createElement("p");
      row.className = "d13-handoff";
      row.dataset.d13Handoff = handoff.handoffId;
      row.textContent = `${handoff.fromLaneId} → ${handoff.toLaneId} · ${handoff.summary}`;
      handoffs.append(row);
    }
    stage.append(handoffs);
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
