import { translate, type Locale, type MessageKey } from "../i18n/catalog";
import type { D1StatusbarProjection } from "../models/workspace";

/// Window-level statusbar for the D1 cockpit.
///
/// Every segment renders a fact the host projected from the confirmed Core
/// view. A segment whose fact is absent renders an explicit em-dash — the
/// bar never invents a number. The pending-gate segment is the bar's only
/// interactive element: it navigates to the D2 decision queue.

const EM_DASH = "—";

/**
 * The ambient (environment) half of `D-STATUSBAR`.
 *
 * The decision splits the bar by *actionability*, not by urgency: identity and
 * actionable segments are pinned and must never be hideable, while the middle
 * band carries the environment long tail. These are the six the bar renders as
 * ambient, and they are the only ones the config gear offers to hide.
 */
export const STATUSBAR_AMBIENT_SEGMENTS = [
  "context",
  "events",
  "latency",
  "tokens",
  "diag",
  "req",
] as const;

export type StatusbarAmbientSegment = (typeof STATUSBAR_AMBIENT_SEGMENTS)[number];

/**
 * The pinned half. `mode` and `perm` and `lane` are the operator's identity
 * facts and `gate` is the bar's one action, so none of them is offered to the
 * gear — a control that can scroll away or be switched off is a control the
 * operator cannot reach when it matters (`O-B6`).
 */
export const STATUSBAR_PINNED_SEGMENTS = ["mode", "perm", "lane", "gate"] as const;

export type StatusbarAmbientVisibility = Record<StatusbarAmbientSegment, boolean>;

export const ALL_STATUSBAR_AMBIENT_VISIBLE: StatusbarAmbientVisibility = {
  context: true,
  events: true,
  latency: true,
  tokens: true,
  diag: true,
  req: true,
};

/**
 * The config gear's port.
 *
 * Absent on a bar nobody can configure — the gear is then not rendered at all,
 * rather than rendered and inert. The visibility map is derived from Core's
 * `UiLayoutPreferences.hidden_statusbar_segments` (C5); `note` is what Core
 * said about the last write, which is where a record Core applied but could
 * not persist becomes visible instead of silently coming back rearranged.
 */
export interface StatusbarConfig {
  ambient: StatusbarAmbientVisibility;
  open: boolean;
  onToggleOpen: () => void;
  onToggleSegment: (segment: StatusbarAmbientSegment) => void;
  /** Core's own sentence about the layout record, or `null` when it is clean. */
  note?: string | null;
}

/// Compact token counts in the design's terminal vocabulary ("42.1k").
export function formatCompactCount(value: number): string {
  if (value < 1000) return String(value);
  const thousands = value / 1000;
  if (thousands >= 100) return `${Math.round(thousands)}k`;
  return `${(Math.round(thousands * 10) / 10).toString()}k`;
}

function formatLatency(milliseconds: number): string {
  if (milliseconds >= 1000) return `${(milliseconds / 1000).toFixed(1)}s`;
  return `${milliseconds}ms`;
}

function segment(
  bar: HTMLElement,
  id: string,
  label: string,
  first: boolean,
): HTMLElement {
  if (!first) {
    const separator = document.createElement("span");
    separator.className = "sb-sep2";
    separator.setAttribute("aria-hidden", "true");
    separator.textContent = "┆";
    bar.append(separator);
  }
  const item = document.createElement("span");
  item.className = "sb-it";
  item.dataset.sbSegment = id;
  const name = document.createElement("span");
  name.className = "lbl";
  name.textContent = `${label} `;
  item.append(name);
  bar.append(item);
  return item;
}

function value(item: HTMLElement, text: string, className = ""): void {
  const strong = document.createElement("b");
  if (className) strong.className = className;
  strong.textContent = text;
  item.append(strong);
}

/// The label each ambient segment already renders in the bar, reused by the
/// config popover so the two can never name the same segment differently.
const SEGMENT_LABEL_KEYS = {
  context: "d1.statusbar.context",
  events: "d1.statusbar.events",
  latency: "d1.statusbar.latency",
  tokens: "d1.statusbar.tokens",
  diag: "d1.statusbar.diag",
  req: "d1.statusbar.req",
} as const satisfies Record<StatusbarAmbientSegment, MessageKey>;

export function renderStatusbar(
  statusbar: D1StatusbarProjection,
  locale: Locale,
  onNavigate?: (route: string) => void,
  config?: StatusbarConfig,
): HTMLElement {
  const bar = document.createElement("footer");
  bar.className = "statusbar d1-status";
  bar.dataset.shellLandmark = "statusbar";
  bar.dataset.statusbar = "true";

  /// An ambient segment is drawn unless the operator switched it off. Without
  /// a config port every segment is shown, which is the pre-gear behaviour.
  const ambientVisible = (segment: StatusbarAmbientSegment): boolean =>
    config ? config.ambient[segment] : true;

  if (config) {
    // The design's `.sb-gear2`: it sits at the left of the bar and opens the
    // `.sbcfg` popover over it.
    const gear = document.createElement("button");
    gear.type = "button";
    gear.className = "sb-gear d1-sb-gear";
    gear.dataset.sbConfigToggle = "true";
    gear.textContent = "⚙";
    const gearLabel = translate(locale, "d1.statusbar.config", {});
    gear.title = gearLabel;
    gear.setAttribute("aria-label", gearLabel);
    gear.setAttribute("aria-haspopup", "dialog");
    gear.setAttribute("aria-expanded", String(config.open));
    gear.addEventListener("click", () => config.onToggleOpen());
    bar.append(gear);
  }

  const mode = segment(bar, "mode", translate(locale, "d1.statusbar.mode", {}), true);
  value(mode, statusbar.workMode, "gd");

  const perm = segment(bar, "perm", translate(locale, "d1.statusbar.perm", {}), false);
  value(perm, statusbar.permissionLevel);

  if (ambientVisible("context")) {
    const context = segment(bar, "context", translate(locale, "d1.statusbar.context", {}), false);
    if (statusbar.context) {
      const { usedTokens, hardTokenLimit, exceeded } = statusbar.context;
      value(context, `${formatCompactCount(usedTokens)} / ${formatCompactCount(hardTokenLimit)}`);
      if (hardTokenLimit > 0) {
        const percent = document.createElement("span");
        percent.className = exceeded ? "sb-exceeded" : "ac";
        percent.textContent = ` ${Math.round((usedTokens / hardTokenLimit) * 100)}%`;
        context.append(percent);
      }
    } else {
      value(context, EM_DASH);
    }
  }

  // The replay-cursor stream position — Core publishes no event counter, so
  // the segment is labeled and titled as a position, never a count.
  if (ambientVisible("events")) {
    const events = segment(bar, "events", translate(locale, "d1.statusbar.events", {}), false);
    events.title = translate(locale, "d1.statusbar.eventsTitle", {});
    value(events, `#${statusbar.eventStreamPosition}`);
  }

  const lane = segment(bar, "lane", translate(locale, "d1.statusbar.lane", {}), false);
  if (statusbar.lane) {
    value(lane, `${statusbar.lane.laneId} ${statusbar.lane.agentId ?? EM_DASH}`, "ac");
    const phase = document.createElement("span");
    phase.className = "sb-progress";
    phase.textContent = ` ${statusbar.lane.status} ${
      statusbar.lane.progress === null ? EM_DASH : `${statusbar.lane.progress}%`
    }`;
    lane.append(phase);
  } else {
    value(lane, EM_DASH);
  }

  if (ambientVisible("latency")) {
    const latency = segment(bar, "latency", translate(locale, "d1.statusbar.latency", {}), false);
    if (statusbar.latency) {
      value(
        latency,
        statusbar.latency.lastLatencyMs === null
          ? EM_DASH
          : formatLatency(statusbar.latency.lastLatencyMs),
      );
      const average = document.createElement("span");
      average.className = "sb-faint";
      average.textContent = ` avg ${
        statusbar.latency.averageLatencyMs === null
          ? EM_DASH
          : formatLatency(statusbar.latency.averageLatencyMs)
      }`;
      latency.append(average);
    } else {
      value(latency, EM_DASH);
    }
  }

  if (ambientVisible("tokens")) {
    const tokens = segment(bar, "tokens", translate(locale, "d1.statusbar.tokens", {}), false);
    if (statusbar.tokens) {
      value(
        tokens,
        `${formatCompactCount(statusbar.tokens.inputTokens)}↑ ${formatCompactCount(
          statusbar.tokens.outputTokens,
        )}↓`,
      );
    } else {
      value(tokens, EM_DASH);
    }
  }

  if (ambientVisible("diag")) {
    const diag = segment(bar, "diag", translate(locale, "d1.statusbar.diag", {}), false);
    value(
      diag,
      `${statusbar.diagnosticsCount}✕`,
      statusbar.diagnosticsCount > 0 ? "sb-error" : "",
    );
  }

  if (ambientVisible("req")) {
    const requests = segment(bar, "req", translate(locale, "d1.statusbar.req", {}), false);
    if (statusbar.requests) {
      value(requests, `${statusbar.requests.requestCount} req`, "ok");
      const errors = document.createElement("span");
      errors.className = statusbar.requests.errorCount > 0 ? "sb-error" : "sb-faint";
      errors.textContent = ` / ${statusbar.requests.errorCount} err`;
      requests.append(errors);
    } else {
      value(requests, EM_DASH);
    }
  }

  // An absent count omits the segment for the same reason a zero does: neither
  // is a decision waiting on the operator, and only one of them is a number.
  if (statusbar.pendingDecisionCount !== null && statusbar.pendingDecisionCount > 0) {
    // The only interactive segment: it opens the D2 decision queue, and it
    // prints that queue's own total in that queue's own words — the rail badge
    // reads the same field, so the three surfaces cannot disagree.
    const gate = document.createElement("button");
    gate.type = "button";
    gate.className = "sb-right d1-sb-gate";
    gate.dataset.sbGate = "true";
    gate.textContent = `⏸ ${translate(locale, "d1.statusbar.decisionsWaiting", {
      count: String(statusbar.pendingDecisionCount),
    })}`;
    gate.disabled = !onNavigate;
    gate.addEventListener("click", () => onNavigate?.("d2"));
    bar.append(gate);
  }

  if (config?.open) {
    // The design's `.sbcfg` popover. It lists the ambient band only: the
    // pinned segments are not offered, because `D-STATUSBAR` makes them
    // unconditional rather than a preference.
    const popover = document.createElement("div");
    popover.className = "sbcfg d1-sb-config";
    popover.dataset.sbConfig = "true";
    popover.setAttribute("role", "dialog");
    popover.setAttribute("aria-label", translate(locale, "d1.statusbar.config", {}));
    const head = document.createElement("p");
    head.className = "mhead";
    head.textContent = translate(locale, "d1.statusbar.config.head", {});
    popover.append(head);
    for (const id of STATUSBAR_AMBIENT_SEGMENTS) {
      const shown = config.ambient[id];
      const row = document.createElement("button");
      row.type = "button";
      row.className = shown ? "crow" : "crow off";
      row.dataset.sbConfigItem = id;
      row.setAttribute("aria-pressed", String(shown));
      const box = document.createElement("span");
      box.className = "bx";
      box.setAttribute("aria-hidden", "true");
      box.textContent = shown ? "[×]" : "[ ]";
      const name = document.createElement("span");
      name.textContent = translate(locale, SEGMENT_LABEL_KEYS[id], {});
      row.append(box, name);
      row.addEventListener("click", () => config.onToggleSegment(id));
      popover.append(row);
    }
    const foot = document.createElement("p");
    foot.className = "mfoot";
    foot.textContent = translate(locale, "d1.statusbar.config.foot", {});
    popover.append(foot);
    if (config.note) {
      // A layout Core applied and could not write, or refused outright. Said
      // where the change was made rather than only in a tooltip, because the
      // operator's next restart is what disagrees with it.
      const note = document.createElement("p");
      note.className = "mfoot d1-sb-config-note";
      note.dataset.sbConfigNote = "true";
      note.setAttribute("role", "status");
      note.textContent = config.note;
      popover.append(note);
    }
    bar.append(popover);
  }

  return bar;
}
