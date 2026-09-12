import { translate, type Locale } from "../i18n/catalog";
import { currentProjectLabel, type D1CockpitProjection } from "../models/workspace";

/**
 * The workspace explorer rail.
 *
 * Visual vocabulary: the design's `WorkspacePanel`
 * (`docs/viden-design/Viden/GUI/Viden - 桌面驾驶舱 (GUI).html`) — a `.wsroot`
 * project header with a `▸`/`▾` collapse and a per-project `＋`, the `.wslanes`
 * rows beneath it, and a `.wsaddproj` footer.
 *
 * The design draws several project groups and a "Global" section. Both are
 * mock data: Core supervises exactly one workspace, so the rail renders exactly
 * one group — the open project — with no fabricated siblings and no global
 * bucket. Multi-root supervision is `GUI-CORE-023`; when Core publishes it,
 * this function grows a second group instead of the client inventing one.
 */

export function adjacentLaneId(
  laneIds: readonly string[],
  index: number,
  direction: "previous" | "next",
): string | null {
  if (laneIds.length === 0) return null;
  const offset = direction === "next" ? 1 : -1;
  return laneIds[(index + offset + laneIds.length) % laneIds.length] ?? null;
}

/**
 * The two `D-SIDEBAR` modes.
 *
 * `float` is the decision's default: the sidebar gives its horizontal space to
 * the transcript and peeks out of a hot zone on the activity rail's right edge.
 * `pinned` gives it a real layout column. The component is identical in both —
 * only its host changes, which is the decision's own rule ("两态内容/结构零分叉")
 * — and the single toggle entry is the activity rail's pin button above the
 * settings gear, not a second control in this header (the decision records
 * that entry as dead CSS, removed 2026-07-02).
 */
export type LaneSidebarMode = "pinned" | "floating";

export interface LaneRailOptions {
  projection: D1CockpitProjection;
  locale: Locale;
  open: boolean;
  selectedLaneId: string | null;
  /**
   * Which mode the sidebar is in. Presentation state the cockpit owns in
   * memory for this batch; see the seam note on `laneSidebarMode` in
   * `d1_cockpit.ts`.
   */
  mode?: LaneSidebarMode;
  /**
   * Whether the project group is collapsed. Purely local presentation state,
   * owned by the cockpit so an ordered Core refresh cannot silently re-expand
   * a group the operator folded away.
   */
  collapsed?: boolean;
  onToggleCollapsed?: () => void;
  onCreateLane: () => void;
  onDismiss: () => void;
  onSelectLane: (laneId: string) => void;
  onRetryAgent: (sessionId: string, laneId: string) => void;
  /**
   * Opens the project picker from the rail's `＋ Add project…` footer. Absent
   * while no host is bound, which keeps the footer out rather than rendering a
   * row that cannot open anything.
   */
  onAddProject?: () => void;
}

function railButton(label = ""): HTMLButtonElement {
  const element = document.createElement("button");
  element.type = "button";
  element.className = "d1-action";
  element.textContent = label;
  return element;
}

export function renderLaneRail(options: LaneRailOptions): HTMLElement {
  const { projection, locale, selectedLaneId } = options;
  const collapsed = options.collapsed === true;
  const lanes = document.createElement("nav");
  lanes.id = "d1-lane-rail";
  lanes.className = "side d1-lanes";
  lanes.dataset.open = String(options.open);
  lanes.dataset.shellLandmark = "lane-rail";
  lanes.dataset.cockpitRole = "lanes";
  lanes.setAttribute("aria-label", translate(locale, "d1.lanes", {}));
  lanes.onkeydown = (event) => {
    if (event.key !== "Escape" || !options.open) return;
    event.preventDefault();
    event.stopPropagation();
    options.onDismiss();
  };

  const mode: LaneSidebarMode = options.mode ?? "floating";
  lanes.dataset.sidebarMode = mode;

  const laneTitle = document.createElement("h2");
  laneTitle.textContent = translate(locale, "d1.lanes", {});
  lanes.append(laneTitle);

  const scroll = document.createElement("div");
  scroll.className = "wsscroll d1-lane-scroll";

  const project = currentProjectLabel(projection);
  const group = document.createElement("div");
  group.className = "wsproj d1-lane-group";
  group.dataset.laneGroup = projection.environment.cwd;

  const head = document.createElement("div");
  head.className = "wsroot d1-lane-group-head";

  const laneListId = "d1-lane-group-lanes";
  const toggle = railButton();
  toggle.className = "d1-action d1-lane-group-toggle";
  toggle.dataset.laneGroupToggle = "true";
  toggle.setAttribute("aria-expanded", String(!collapsed));
  toggle.setAttribute("aria-controls", laneListId);
  const chevron = document.createElement("span");
  chevron.className = "ic";
  chevron.setAttribute("aria-hidden", "true");
  chevron.textContent = collapsed ? "▸" : "▾";
  const name = document.createElement("span");
  name.className = "nm";
  name.textContent = project;
  toggle.append(chevron, name);
  toggle.addEventListener("click", () => options.onToggleCollapsed?.());
  // Without a handler the group cannot fold, so the control states that
  // instead of swallowing the click.
  toggle.disabled = !options.onToggleCollapsed;

  const count = document.createElement("span");
  count.className = "lct";
  count.dataset.laneGroupCount = "true";
  if (projection.lanes.length === 0) count.classList.add("z");
  count.textContent = String(projection.lanes.length);

  // The design's per-project `＋`. It is the same Lane creation action the flat
  // rail exposed, so `data-create-lane` — and every keyboard path and test
  // that reaches it — is unchanged; only its place in the tree moved.
  const createLane = railButton("＋");
  createLane.dataset.createLane = "true";
  createLane.classList.add("wsghadd", "d1-create-lane");
  const createLabel = translate(locale, "d1.lane.create", {});
  createLane.title = createLabel;
  createLane.setAttribute("aria-label", createLabel);
  createLane.setAttribute("aria-haspopup", "menu");
  createLane.setAttribute("aria-expanded", "false");
  createLane.addEventListener("click", options.onCreateLane);

  head.append(toggle, count, createLane);
  group.append(head);

  const laneList = document.createElement("div");
  laneList.id = laneListId;
  laneList.className = "wslanes d1-lane-list";
  laneList.hidden = collapsed;

  projection.lanes.forEach((lane, index) => {
    const boundSession = projection.agentSessions.find(
      (candidate) => candidate.laneId === lane.id,
    );
    const item = railButton();
    item.className = "wslane d1-lane";
    item.dataset.laneId = lane.id;
    item.dataset.laneAgentId = boundSession?.agentId ?? "viden";
    item.setAttribute("aria-current", String(lane.id === selectedLaneId));

    const status = document.createElement("span");
    status.className = "d1-lane-status";
    status.dataset.laneStatus = "true";
    status.dataset.status = lane.status;
    status.setAttribute("aria-hidden", "true");
    const copy = document.createElement("span");
    copy.className = "lbody";
    const title = document.createElement("strong");
    title.textContent = lane.id;
    const detail = document.createElement("small");
    detail.textContent = boundSession
      ? `${boundSession.agentId} · ${boundSession.status}`
      : `Viden · ${lane.status}`;
    copy.append(title, detail);
    item.append(status, copy);
    item.addEventListener("keydown", (event) => {
      if (!["ArrowUp", "ArrowDown"].includes(event.key)) return;
      event.preventDefault();
      const target = adjacentLaneId(
        projection.lanes.map((candidate) => candidate.id),
        index,
        event.key === "ArrowDown" ? "next" : "previous",
      );
      if (target) {
        Array.from(lanes.querySelectorAll<HTMLElement>("[data-lane-id]"))
          .find((candidate) => candidate.dataset.laneId === target)
          ?.focus();
      }
    });
    item.addEventListener("click", () => options.onSelectLane(lane.id));
    laneList.append(item);

    if (boundSession && ["failed", "cancelled"].includes(boundSession.status)) {
      const retry = railButton(translate(locale, "d1.session.retry", {}));
      retry.className = "d1-lane-agent-retry";
      retry.dataset.retryLaneAgent = lane.id;
      retry.addEventListener("click", () =>
        options.onRetryAgent(boundSession.sessionId, lane.id),
      );
      laneList.append(retry);
    }
  });

  if (projection.lanes.length === 0) {
    const empty = document.createElement("p");
    empty.className = "wsempty d1-lane-empty";
    empty.dataset.laneGroupEmpty = "true";
    empty.textContent = translate(locale, "d1.rail.noLanes", {});
    laneList.append(empty);
  }

  group.append(laneList);
  scroll.append(group);

  if (options.onAddProject) {
    const addProject = railButton(translate(locale, "d1.rail.addProject", {}));
    addProject.className = "wsaddproj d1-add-project";
    addProject.dataset.addProject = "true";
    addProject.setAttribute("aria-haspopup", "dialog");
    addProject.setAttribute("aria-expanded", "false");
    addProject.addEventListener("click", () => options.onAddProject?.());
    scroll.append(addProject);
  }

  lanes.append(scroll);
  return lanes;
}
