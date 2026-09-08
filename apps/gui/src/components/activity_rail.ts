import { translate, type Locale, type MessageKey } from "../i18n/catalog";

/**
 * The rail's slots, in the accepted design's order.
 *
 * Each routing slot is labelled with the screen it actually opens. The rail
 * previously carried an editor's file-explorer vocabulary (Search, Source
 * control, Evidence, Diagnostics, Inbox) over routes that go elsewhere; a
 * label naming the wrong destination is worse than a missing one, because the
 * operator can only learn the real mapping by clicking, and every tooltip and
 * screen-reader announcement teaches it wrong until they do.
 */
export const D1_ACTIVITY_ITEMS = [
  { icon: "chat", key: "d1.activity.work" },
  { icon: "worktree", key: "d1.activity.integrationGate" },
  { icon: "lanes", key: "d1.activity.lanes" },
  { icon: "decide", key: "d1.activity.decisions" },
  { icon: "evidence", key: "d1.activity.audit" },
  { icon: "diagnostics", key: "d1.activity.laneMonitor" },
  { icon: "fleet", key: "d1.activity.fleet" },
] as const satisfies ReadonlyArray<{
  icon: CanonicalGuiIcon;
  key: MessageKey;
}>;

export type CanonicalGuiIcon =
  | "chat"
  | "worktree"
  | "lanes"
  | "review"
  | "fleet"
  | "evidence"
  | "diagnostics"
  | "inbox"
  | "settings"
  | "decide"
  | "palette"
  | "panel";

const SVG_NAMESPACE = "http://www.w3.org/2000/svg";

function svgNode<K extends keyof SVGElementTagNameMap>(
  name: K,
  attributes: Record<string, string>,
): SVGElementTagNameMap[K] {
  const node = document.createElementNS(SVG_NAMESPACE, name);
  for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, value);
  return node;
}

/**
 * Uses the exact registered paths from the design package's `GUI/gui-icons.jsx`.
 * Keeping this adapter data-only prevents the production client from loading the
 * prototype's React/Babel runtime while preserving the canonical icon artwork.
 */
export function createCanonicalGuiIcon(name: CanonicalGuiIcon): SVGSVGElement {
  const svg = svgNode("svg", {
    class: "ic",
    viewBox: "0 0 24 24",
    "aria-hidden": "true",
  });
  if (name === "chat") {
    svg.append(
      svgNode("path", {
        d: "M21 11.5a8.38 8.38 0 0 1-8.5 8.5 8.5 8.5 0 0 1-3.8-.9L3 21l1.9-5.7A8.38 8.38 0 0 1 4 11.5 8.5 8.5 0 0 1 12.5 3 8.38 8.38 0 0 1 21 11.5z",
      }),
    );
  } else if (name === "worktree") {
    svg.append(
      svgNode("circle", { cx: "6", cy: "6", r: "2.4" }),
      svgNode("circle", { cx: "6", cy: "18", r: "2.4" }),
      svgNode("circle", { cx: "18", cy: "9", r: "2.4" }),
      svgNode("path", { d: "M6 8.4v7.2M18 11.4c0 3-3 3.6-5.4 3.6H8.4" }),
    );
  } else if (name === "lanes") {
    svg.append(
      svgNode("path", {
        d: "M4 5v14M12 5v14M20 5v14",
        "stroke-dasharray": "3 3",
      }),
      svgNode("circle", { cx: "4", cy: "10", r: "2.2" }),
      svgNode("circle", { cx: "12", cy: "14", r: "2.2" }),
      svgNode("circle", { cx: "20", cy: "8", r: "2.2" }),
    );
  } else if (name === "review") {
    svg.append(
      svgNode("path", {
        d: "M9 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h4M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4M12 4v16",
      }),
    );
  } else if (name === "evidence") {
    svg.append(
      svgNode("path", { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }),
      svgNode("path", { d: "M14 2v6h6" }),
      svgNode("path", { d: "M9 13h6M9 17h4" }),
    );
  } else if (name === "diagnostics") {
    svg.append(svgNode("path", { d: "M3 12h4l3 8 4-16 3 8h4" }));
  } else if (name === "fleet") {
    svg.append(
      svgNode("circle", { cx: "12", cy: "5", r: "2.4" }),
      svgNode("circle", { cx: "5", cy: "19", r: "2.2" }),
      svgNode("circle", { cx: "12", cy: "19", r: "2.2" }),
      svgNode("circle", { cx: "19", cy: "19", r: "2.2" }),
      svgNode("path", { d: "M12 7.4v4M12 11.4L5.6 16.8M12 11.4l6.4 5.4M12 11.4V17" }),
    );
  } else if (name === "inbox") {
    svg.append(svgNode("path", { d: "M22 12h-6l-2 3h-4l-2-3H2M5 5h14l3 7v6a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2v-6z" }));
  } else if (name === "decide") {
    svg.append(
      svgNode("path", {
        d: "M9 11l3 3L22 4M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11",
      }),
    );
  } else if (name === "palette") {
    // The design kit's titlebar-tools glyph for the command palette.
    svg.append(
      svgNode("path", {
        d: "M9 6a3 3 0 1 0-3 3h12a3 3 0 1 0-3-3v12a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3z",
      }),
    );
  } else if (name === "settings") {
    svg.append(
      svgNode("circle", { cx: "12", cy: "12", r: "3" }),
      svgNode("path", {
        d: "M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z",
      }),
    );
  } else {
    svg.append(
      svgNode("rect", { x: "3", y: "4", width: "18", height: "16", rx: "2" }),
      svgNode("path", { d: "M15 4v16" }),
    );
  }
  return svg;
}

/// Rail slots that open a restored screen.
///
/// The destinations are honest as of this change: every slot's label, tooltip,
/// and accessible name is the screen it opens, and each carries the registered
/// `GUI/gui-icons.jsx` glyph closest to that screen. The `worktree` slot also
/// matches the D12 design page, whose own rail highlights it for the
/// integration gate.
///
/// What remains open is the *design* question, not the labelling one: the
/// accepted rail has no slot for the decision queue, the audit trail, the lane
/// monitor, or the fleet board, and the design instead reaches several of them
/// as in-cockpit secondary views. Those four slots therefore stay a routing
/// decision awaiting design adjudication — the screens are reachable here
/// rather than only from a URL. Do not read the honest labels as the
/// adjudication having happened.
export const D1_RAIL_ROUTES: Partial<Record<string, string>> = {
  "d1.activity.integrationGate": "d12",
  "d1.activity.decisions": "d2",
  "d1.activity.audit": "d14",
  "d1.activity.laneMonitor": "d10",
  "d1.activity.fleet": "d13",
};

export interface ActivityRailOptions {
  lanesAvailable: boolean;
  lanesOpen: boolean;
  onToggleLanes: () => void;
  /// Opens a restored screen. Absent while no Core projection is bound, which
  /// keeps every routing slot disabled rather than opening an empty screen.
  onNavigate?: (route: string) => void;
  /// Focuses the work surface the `Work` slot already marks as current. The
  /// rail owns no screen state, so the cockpit supplies the focus target;
  /// without it the slot is disabled rather than enabled and inert.
  onFocusWork?: () => void;
  /// Opens the Settings overlay from the rail's bottom gear, matching the
  /// cockpit prototype. Absent while no host is bound, which disables the
  /// gear rather than opening a panel that cannot reach Core. A *bound* host
  /// always supplies it, even without the preference capability, so the
  /// unavailable state is something the operator can open and read.
  onOpenSettings?: () => void;
  settingsOpen?: boolean;
}

export function renderActivityRail(
  locale: Locale,
  options: ActivityRailOptions,
): HTMLElement {
  const activity = document.createElement("nav");
  activity.className = "act d1-activity";
  activity.dataset.shellLandmark = "activity-rail";
  activity.dataset.cockpitRole = "activity";
  activity.setAttribute("aria-label", translate(locale, "d1.activity", {}));
  for (const activityItem of D1_ACTIVITY_ITEMS) {
    const item = document.createElement("button");
    item.type = "button";
    item.className = "actbtn d1-action";
    item.title = translate(locale, activityItem.key, {});
    item.setAttribute("aria-label", translate(locale, activityItem.key, {}));
    item.append(createCanonicalGuiIcon(activityItem.icon));
    if (activityItem.key === "d1.activity.work") {
      // `Work` is the screen already showing, so it stays marked current
      // instead of routing. Activating it returns focus to the work surface;
      // with no focus target it is disabled rather than enabled and inert.
      item.classList.add("on");
      item.setAttribute("aria-current", "page");
      item.disabled = !options.onFocusWork;
      if (options.onFocusWork) {
        item.dataset.railFocusWork = "true";
        item.addEventListener("click", () => options.onFocusWork?.());
      }
    } else if (activityItem.key === "d1.activity.lanes") {
      item.dataset.lanesToggle = "true";
      item.disabled = !options.lanesAvailable;
      if (options.lanesAvailable) {
        item.setAttribute("aria-controls", "d1-lane-rail");
        item.setAttribute("aria-expanded", String(options.lanesOpen));
        item.addEventListener("click", options.onToggleLanes);
        item.addEventListener("keydown", (event) => {
          if (!["Enter", " "].includes(event.key)) return;
          event.preventDefault();
          item.click();
        });
      }
    } else if (D1_RAIL_ROUTES[activityItem.key] && options.onNavigate) {
      const route = D1_RAIL_ROUTES[activityItem.key] as string;
      item.dataset.railRoute = route;
      item.addEventListener("click", () => options.onNavigate?.(route));
    } else {
      item.disabled = true;
    }
    activity.append(item);
  }
  const spacer = document.createElement("span");
  spacer.className = "actspacer";
  spacer.ariaHidden = "true";
  activity.append(spacer);

  // The cockpit prototype puts ⚙ below the rail spacer; the accepted design
  // component is `GUI/gui-settings.jsx`.
  const settings = document.createElement("button");
  settings.type = "button";
  settings.className = "actbtn d1-action";
  settings.dataset.settingsToggle = "true";
  settings.title = translate(locale, "d1.activity.settings", {});
  settings.setAttribute("aria-label", translate(locale, "d1.activity.settings", {}));
  settings.setAttribute("aria-haspopup", "dialog");
  settings.setAttribute("aria-expanded", String(options.settingsOpen === true));
  settings.append(createCanonicalGuiIcon("settings"));
  settings.disabled = !options.onOpenSettings;
  if (options.onOpenSettings) {
    settings.addEventListener("click", () => options.onOpenSettings?.());
  }
  activity.append(settings);
  return activity;
}
