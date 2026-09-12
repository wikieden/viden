import type { LaneSidebarMode } from "./lane_rail";
import { translate, type Locale, type MessageKey } from "../i18n/catalog";

/**
 * The centre-pane destinations the rail routes to.
 *
 * `review` and `evidence` are the two secondary surfaces `D-RAILNAV`
 * registered; the five `d*` entries are the standalone D-screens the `0.3.4`
 * plan moved into the cockpit centre. Every one of them is a *view* over the
 * transcript, so the id is the centre view the cockpit switches to rather than
 * a window route.
 */
export type D1RailDestination = "review" | "evidence" | "d2" | "d10" | "d12" | "d13" | "d14";

export interface D1RailSlot {
  /** Stable slot id; equals the destination for a routing slot. */
  readonly id: "conversation" | "lanes" | D1RailDestination;
  readonly icon: CanonicalGuiIcon;
  readonly key: MessageKey;
  readonly kind: "conversation" | "lanes" | "destination";
  /** The centre view a `destination` slot opens. */
  readonly destination?: D1RailDestination;
}

/**
 * The rail, top to bottom, as one ordered router table (`D-RAILNAV`).
 *
 * This replaced two structures that had drifted apart — a slot list and a
 * separate route map keyed by message id — plus the ad-hoc `Work`/`Lanes`
 * branches inside the renderer. One table is what makes "every registered
 * destination is reachable from the rail" checkable rather than asserted: a
 * destination that is not in this array has no slot, and a slot that is in it
 * has exactly one destination.
 *
 * Each slot is labelled with the screen it actually opens and carries the
 * registered `GUI/gui-icons.jsx` glyph closest to it. The Settings gear is not
 * in the table: it sits below the rail spacer and opens an overlay rather than
 * a centre view.
 */
export const D1_RAIL_ROUTES: readonly D1RailSlot[] = [
  { id: "conversation", icon: "chat", key: "d1.activity.work", kind: "conversation" },
  { id: "lanes", icon: "lanes", key: "d1.activity.lanes", kind: "lanes" },
  {
    id: "review",
    icon: "review",
    key: "d1.activity.review",
    kind: "destination",
    destination: "review",
  },
  {
    id: "evidence",
    icon: "evidence",
    key: "d1.activity.evidence",
    kind: "destination",
    destination: "evidence",
  },
  {
    id: "d2",
    icon: "decide",
    key: "d1.activity.decisions",
    kind: "destination",
    destination: "d2",
  },
  {
    id: "d10",
    icon: "diagnostics",
    key: "d1.activity.laneMonitor",
    kind: "destination",
    destination: "d10",
  },
  {
    id: "d12",
    icon: "worktree",
    key: "d1.activity.integrationGate",
    kind: "destination",
    destination: "d12",
  },
  { id: "d13", icon: "fleet", key: "d1.activity.fleet", kind: "destination", destination: "d13" },
  { id: "d14", icon: "brief", key: "d1.activity.audit", kind: "destination", destination: "d14" },
];

export type CanonicalGuiIcon =
  | "chat"
  | "worktree"
  | "lanes"
  | "review"
  | "fleet"
  | "evidence"
  | "brief"
  | "diagnostics"
  | "inbox"
  | "settings"
  | "decide"
  | "pin"
  | "palette"
  | "panel"
  | "focus";

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
  } else if (name === "brief") {
    // The registered `brief` glyph: the audit trail is a record of decisions,
    // which is a different document from the evidence archive's file.
    svg.append(
      svgNode("path", {
        d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zM14 2v6h6M9 13h6M9 17h6",
      }),
    );
  } else if (name === "pin") {
    svg.append(svgNode("path", { d: "M9 3h6M10 3v6l-3 3v2h10v-2l-3-3V3M12 16v5" }));
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
  } else if (name === "focus") {
    // The registered `IFocus` glyph: four corner brackets closing in.
    svg.append(
      svgNode("path", {
        d: "M4 8V5a1 1 0 0 1 1-1h3M16 4h3a1 1 0 0 1 1 1v3M20 16v3a1 1 0 0 1-1 1h-3M8 20H5a1 1 0 0 1-1-1v-3",
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

export interface D1RailDestinationState {
  /** True when activating the slot actually has somewhere to go. */
  available: boolean;
  /** True when this destination is the one the centre pane is showing. */
  current: boolean;
  /**
   * A sentence appended to the tooltip and the accessible name. It states the
   * Core capability the destination needs and does not have, or a condition
   * the operator would otherwise only discover by opening the view.
   */
  note?: string;
  /**
   * A count Core already published, rendered as the design's `.badge`. Nothing
   * is counted client-side: a destination with no Core-published count carries
   * no badge rather than a zero.
   */
  badge?: number | null;
}

export interface ActivityRailOptions {
  lanesAvailable: boolean;
  lanesOpen: boolean;
  onToggleLanes: () => void;
  /**
   * Focuses the work surface the Conversation slot marks as current. The rail
   * owns no screen state, so the cockpit supplies the focus target; without it
   * the slot is disabled rather than enabled and inert.
   */
  onFocusWork?: () => void;
  /** True while the transcript owns the centre pane. */
  conversationCurrent?: boolean;
  /**
   * Per-destination state, resolved by the cockpit: whether the destination
   * can be opened at all, whether it is the one showing, why it cannot be
   * opened, and any count Core published for it. A destination missing from
   * this map has no action behind it and renders disabled.
   */
  destinations?: Partial<Record<D1RailDestination, D1RailDestinationState>>;
  /** Switches the cockpit centre pane to one destination. */
  onOpenDestination?: (destination: D1RailDestination) => void;
  /**
   * Opens the Settings overlay from the rail's bottom gear, matching the
   * cockpit prototype. Absent while no host is bound, which disables the gear
   * rather than opening a panel that cannot reach Core. A *bound* host always
   * supplies it, even without the preference capability, so the unavailable
   * state is something the operator can open and read.
   */
  onOpenSettings?: () => void;
  settingsOpen?: boolean;
  /**
   * `D-SIDEBAR`'s single toggle entry: the pin button the decision places at
   * the bottom of the activity rail, directly above the settings gear. There
   * is deliberately no second entry — the sidebar-header `.pinbtn` the design
   * once drew was never rendered and its CSS was deleted (2026-07-02).
   *
   * Absent leaves the control out rather than rendering it inert.
   */
  laneSidebarMode?: LaneSidebarMode;
  onToggleLaneSidebarMode?: () => void;
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
  // Exactly one slot is current, and the table decides which: the conversation
  // is the home surface, so it is current precisely when no destination is.
  // Deriving it here rather than taking two independent flags is what stops
  // the rail from ever marking two destinations at once.
  const conversationCurrent =
    options.conversationCurrent ??
    !Object.values(options.destinations ?? {}).some((state) => state?.current);
  for (const slot of D1_RAIL_ROUTES) {
    const item = document.createElement("button");
    item.type = "button";
    item.className = "actbtn d1-action";
    item.dataset.railSlot = slot.id;
    const name = translate(locale, slot.key, {});
    const state = slot.destination ? options.destinations?.[slot.destination] : undefined;
    // The note is part of the name, not a decoration beside it: a slot whose
    // destination cannot be opened must say why in the one string a screen
    // reader announces, not only in a tooltip a pointer would have to find.
    const label = state?.note ? `${name} — ${state.note}` : name;
    item.title = label;
    item.setAttribute("aria-label", label);
    item.append(createCanonicalGuiIcon(slot.icon));

    if (slot.kind === "conversation") {
      // The transcript is the cockpit's home surface, so this slot returns to
      // it and focuses the composer rather than routing. It is marked current
      // only while the transcript is actually the view showing; with no focus
      // target it is disabled rather than enabled and inert.
      item.classList.toggle("on", conversationCurrent);
      if (conversationCurrent) item.setAttribute("aria-current", "page");
      item.disabled = !options.onFocusWork;
      if (options.onFocusWork) {
        item.dataset.railFocusWork = "true";
        item.addEventListener("click", () => options.onFocusWork?.());
      }
    } else if (slot.kind === "lanes") {
      item.dataset.lanesToggle = "true";
      item.disabled = !options.lanesAvailable;
      if (options.lanesAvailable) {
        item.setAttribute("aria-controls", "d1-lane-rail");
        item.setAttribute("aria-expanded", String(options.lanesOpen));
        item.classList.toggle("on", options.lanesOpen);
        item.addEventListener("click", options.onToggleLanes);
        item.addEventListener("keydown", (event) => {
          if (!["Enter", " "].includes(event.key)) return;
          event.preventDefault();
          item.click();
        });
      }
    } else if (state?.available && options.onOpenDestination) {
      const destination = slot.destination as D1RailDestination;
      item.dataset.railRoute = destination;
      if (state.current) {
        item.classList.add("on");
        item.setAttribute("aria-current", "page");
      }
      item.addEventListener("click", () => options.onOpenDestination?.(destination));
    } else {
      // No action behind it: disabled rather than enabled and inert. The label
      // above already carries the reason when the cockpit supplied one.
      item.disabled = true;
    }

    // Counts come from Core and nowhere else. A destination the cockpit has no
    // published count for carries no badge, never a zero that would read as
    // "nothing waiting" when the truth is "nobody counted".
    if (typeof state?.badge === "number" && state.badge > 0) {
      const badge = document.createElement("span");
      badge.className = "badge";
      badge.dataset.railBadge = "true";
      badge.textContent = String(state.badge);
      item.append(badge);
    }
    activity.append(item);
  }
  const spacer = document.createElement("span");
  spacer.className = "actspacer";
  spacer.ariaHidden = "true";
  activity.append(spacer);

  // `D-SIDEBAR`'s pin, between the spacer and the gear. `aria-pressed` reports
  // the *current* mode rather than the verb the button performs, which is what
  // the state means on a toggle: a screen reader hears "pinned, pressed".
  if (options.onToggleLaneSidebarMode) {
    const mode: LaneSidebarMode = options.laneSidebarMode ?? "floating";
    const pin = document.createElement("button");
    pin.type = "button";
    pin.className = "actbtn d1-action";
    pin.dataset.laneSidebarPin = mode;
    pin.setAttribute("aria-pressed", String(mode === "pinned"));
    const label = translate(locale, mode === "pinned" ? "d1.lanes.unpin" : "d1.lanes.pin", {});
    pin.title = label;
    pin.setAttribute("aria-label", label);
    pin.classList.toggle("on", mode === "pinned");
    pin.append(createCanonicalGuiIcon("pin"));
    pin.addEventListener("click", () => options.onToggleLaneSidebarMode?.());
    activity.append(pin);
  }

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
