import { translate, type Locale } from "../i18n/catalog";
import { workModeLabel } from "./composer_controls";
import { formatCompactCount } from "./statusbar";
import { currentProjectLabel, type D1CockpitProjection } from "../models/workspace";
import "./lane_tabs.css";

/**
 * The centre pane's Lane tab strip.
 *
 * Visual vocabulary: the design's `.tabstrip.lanebar` inside `ChatView`
 * (`docs/viden-design/Viden/GUI/Viden - 桌面驾驶舱 (GUI).html`) — a `.lanehd`
 * carrying the status dot, the `.lid` Lane id, the `.lnm` name, the `.lbr`
 * branch and the `.lscope` project, then a `.tabspacer` and a trailing
 * `.tabmeta` with the context budget and the work mode.
 *
 * The design draws that head for **one** Lane, because its prototype opens one
 * conversation at a time ("a lane IS one session — no session tabs"). The
 * cockpit really does hold several Lanes at once — Core publishes them all in
 * `projection.lanes` — so the strip is drawn with the same kit's `.tab` /
 * `.tabadd` / `.tabspacer` / `.tabmeta` parts, one `.tab` per Lane, each
 * carrying the `.lanehd` content. Nothing is invented: every part of both
 * families is in the flagship's own stylesheet.
 *
 * **What is a Lane fact and what is not.** The branch is `C5`'s per-Lane
 * worktree source (`lane_sources[lane]`) when Core sampled one, and the Lane's
 * own recorded `branch` otherwise — a sampled tree's branch is the live fact,
 * and the record is what Core wrote when the Lane was created. A Lane with
 * neither shows no branch: the workspace's branch belongs to the titlebar and
 * must never stand in for a Lane's. A sampled tree additionally carries its
 * own `↑ahead ↓behind` and dirty marker, which is exactly the fact that could
 * not be shown before `C5` — the titlebar chips are the *workspace* root's
 * position, and printing them on a Lane tab would name one tree with another
 * tree's numbers. The agent is Core's own `agentSessions` binding; a Lane with
 * no Agent session runs on the built-in runtime, which the Lane rail already
 * names the same way.
 *
 * **Why the meta slot describes only the selected Lane.** The cockpit
 * projection is Lane-scoped: `contextDock.context` is the budget Core sampled
 * for the Lane it was read for, and `statusbar.workMode` is the mode Core
 * resolved for that same read. Printing either against a Lane the read did not
 * cover would be a number with the wrong Lane's name on it, so they live in the
 * trailing `.tabmeta` the design already puts them in rather than on each tab.
 */

export interface LaneTabsOptions {
  projection: D1CockpitProjection;
  locale: Locale;
  /** The cockpit's selection, which leads Core's republished one. */
  selectedLaneId: string | null;
  onSelectLane: (laneId: string) => void;
  /** Opens the New Lane popover from the strip's trailing `＋`. */
  onCreateLane: () => void;
}

/**
 * Returns the Lane id one step from `laneId` in the projected order.
 *
 * `Ctrl+Tab` wraps, like the rail's arrow keys; an unknown or absent selection
 * starts from the first Lane so the chord always lands somewhere real.
 */
export function cycledLaneId(
  laneIds: readonly string[],
  laneId: string | null,
  direction: "next" | "previous",
): string | null {
  if (laneIds.length === 0) return null;
  const index = laneId === null ? -1 : laneIds.indexOf(laneId);
  if (index < 0) return laneIds[0] ?? null;
  const offset = direction === "next" ? 1 : -1;
  return laneIds[(index + offset + laneIds.length) % laneIds.length] ?? null;
}

export function renderLaneTabs(options: LaneTabsOptions): HTMLElement {
  const { projection, locale, selectedLaneId } = options;
  const strip = document.createElement("div");
  strip.className = "tabstrip lanebar d1-lane-tabs";
  strip.dataset.laneTabs = "true";
  strip.setAttribute("role", "tablist");
  strip.setAttribute("aria-label", translate(locale, "d1.laneTabs", {}));

  for (const lane of projection.lanes) {
    const boundSession = projection.agentSessions.find(
      (candidate) => candidate.laneId === lane.id,
    );
    const tab = document.createElement("button");
    tab.type = "button";
    tab.className = "tab d1-lane-tab";
    tab.dataset.laneTab = lane.id;
    tab.setAttribute("role", "tab");
    const current = lane.id === selectedLaneId;
    tab.setAttribute("aria-selected", String(current));
    // A tablist moves with arrow keys; only the current tab is in the tab
    // order, which is what keeps `Ctrl+Tab` and `Tab` from fighting.
    tab.tabIndex = current ? 0 : -1;
    if (current) tab.classList.add("on");

    const head = document.createElement("span");
    head.className = "lanehd";
    const status = document.createElement("span");
    status.className = "st d1-lane-tab-status";
    status.dataset.status = lane.status;
    status.setAttribute("aria-hidden", "true");
    const id = document.createElement("span");
    id.className = "lid";
    id.textContent = lane.id;
    const name = document.createElement("span");
    name.className = "lnm";
    name.textContent = lane.summary;
    head.append(status, id, name);
    // `C5`'s sampled worktree wins over the recorded branch, because it is
    // the live fact about that tree; the record is what Core wrote at
    // creation. Neither means no branch is drawn.
    const laneSource = lane.source ?? null;
    const branchName = laneSource?.branch ?? lane.branch;
    if (branchName) {
      const branch = document.createElement("span");
      branch.className = "lbr";
      branch.dataset.laneTabBranch = "true";
      if (laneSource?.branch) branch.dataset.laneTabBranchSource = "lane_sources";
      branch.title = branchName;
      branch.textContent = branchName;
      head.append(branch);
    }
    if (laneSource && laneSource.status !== "unavailable") {
      // This Lane's own position and dirt, never the workspace chip's. A
      // sample Core could not take renders nothing rather than zeroes, which
      // would read as "clean and in sync".
      const position = document.createElement("span");
      position.className = "lscope d1-lane-tab-source";
      position.dataset.laneTabSource = lane.id;
      const marks = [`↑${laneSource.ahead} ↓${laneSource.behind}`];
      if (laneSource.dirty) marks.push("●");
      position.textContent = marks.join(" ");
      position.title = translate(locale, "d1.laneTabs.source", {
        ahead: String(laneSource.ahead),
        behind: String(laneSource.behind),
      });
      head.append(position);
    }
    const agent = document.createElement("span");
    agent.className = "lscope d1-lane-tab-agent";
    agent.dataset.laneTabAgent = "true";
    agent.textContent = boundSession
      ? boundSession.agentId
      : translate(locale, "d1.agentMenu.viden", {});
    head.append(agent);

    tab.append(head);
    // The accessible name states the Lane and its status; the visual row keeps
    // the design's compact glyph.
    tab.setAttribute(
      "aria-label",
      `${lane.id} · ${lane.summary} · ${lane.status}${branchName ? ` · ${branchName}` : ""}`,
    );
    tab.addEventListener("click", () => options.onSelectLane(lane.id));
    tab.addEventListener("keydown", (event) => {
      const direction =
        event.key === "ArrowRight" ? "next" : event.key === "ArrowLeft" ? "previous" : null;
      if (!direction) return;
      event.preventDefault();
      const target = cycledLaneId(
        projection.lanes.map((candidate) => candidate.id),
        lane.id,
        direction,
      );
      if (target) {
        strip.querySelector<HTMLElement>(`[data-lane-tab="${CSS.escape(target)}"]`)?.focus();
      }
    });
    strip.append(tab);
  }

  if (projection.lanes.length === 0) {
    // The strip stays. A cockpit with no Lanes still has a project and still
    // has one thing to do, so the empty state names both rather than leaving a
    // blank rule where the tabs will be.
    const empty = document.createElement("span");
    empty.className = "tab d1-lane-tab-empty";
    empty.dataset.laneTabEmpty = "true";
    empty.textContent = translate(locale, "d1.rail.noLanes", {});
    strip.append(empty);
  }

  const add = document.createElement("button");
  add.type = "button";
  add.className = "tabadd d1-lane-tab-add";
  add.dataset.laneTabAdd = "true";
  add.textContent = "＋";
  const addLabel = translate(locale, "d1.lane.create", {});
  add.title = `${addLabel} ${translate(locale, "d1.shortcut.newLane", {})}`;
  add.setAttribute("aria-label", addLabel);
  add.setAttribute("aria-haspopup", "menu");
  add.addEventListener("click", () => options.onCreateLane());
  strip.append(add);

  const spacer = document.createElement("span");
  spacer.className = "tabspacer";
  spacer.setAttribute("aria-hidden", "true");
  strip.append(spacer);

  const meta = document.createElement("span");
  meta.className = "tabmeta";
  meta.dataset.laneTabMeta = "true";
  const project = document.createElement("span");
  project.dataset.laneTabProject = "true";
  project.textContent = currentProjectLabel(projection);
  meta.append(project);
  const context = projection.contextDock.context;
  if (context) {
    // Core's own budget sample for the Lane this projection was read for. No
    // sample means no number — a zero would read as "nothing used yet".
    const used = document.createElement("span");
    used.dataset.laneTabContext = "true";
    used.textContent = formatCompactCount(context.usedTokens);
    meta.append(used);
  }
  const mode = document.createElement("span");
  mode.className = "d1-lane-tab-mode";
  mode.dataset.laneTabMode = "true";
  mode.textContent = workModeLabel(locale, projection.statusbar.workMode);
  meta.append(mode);
  strip.append(meta);

  return strip;
}
