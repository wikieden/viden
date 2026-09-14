import { translate, type Locale, type MessageKey } from "../i18n/catalog";
import {
  AGENT_SESSION_CONTROL_CAPABILITY,
  D16_CAPABILITY_ABSENT_CODE,
  D16_NO_CORE_FACT_CODE,
  D16_NO_MODEL_CODE,
  D16_SESSION_COMPLETED_CODE,
  D16_SESSION_ENDED_CODE,
  D16_STATUS_BLOCK_PREFIX,
  type D16AgentBoardProjection,
  type D16Action,
  type D16Intent,
  type D16IntentResult,
  type D16Lane,
  type D16Session,
} from "../models/board";
import type { D14AuditProjection } from "./d14_audit_timeline";
import "./d16_agent_board.css";
import { verbatimField } from "../ui/verbatim_field";

/**
 * D16 Agent Board — the cockpit's centre-pane view over the Lane/session
 * hierarchy (`runtime.agent_session_control`, C14).
 *
 * Four rules, each of which exists because dropping it produces a lie the
 * design names explicitly:
 *
 * - **A session is never drawn detached from its Lane.** Rows are nested under
 *   the Lane Core attributes them to, including a Lane Core published no record
 *   for, which is drawn thin rather than dissolved.
 * - **Liveness is `status`.** `active_session_ids` is not a liveness signal —
 *   Core never prunes it — so the board never consults it.
 * - **A disabled control names its reason.** Every blocked control carries
 *   Core's own `session_status_<current>` string, or a screen code where the
 *   reason is the client's.
 * - **Switch model is two steps and says so.** There is no switch command; the
 *   panel spells out "end this session" and "start again, loading it", and each
 *   step sends its own command under its own id.
 */

export type { D16AgentBoardProjection } from "../models/board";

export interface D16Controller {
  applyProjection: (projection: D16AgentBoardProjection) => void;
  dispose: () => void;
}

export interface D16Options {
  /** Sends one control and resolves with Core's own answer. */
  send?: (intent: D16Intent) => Promise<D16IntentResult>;
  /**
   * Re-reads the board for one selection with no Core traffic. Selecting a row
   * re-reads rather than re-deriving, so the detail pane is always Core's own
   * answer for the session the operator picked.
   */
  reload?: (sessionId: string | null) => Promise<D16AgentBoardProjection>;
  /**
   * One bounded `QueryAudit` page scoped to the session
   * (`{ kind: "session", id }`), which is the event timeline. Absent leaves the
   * timeline unread rather than empty — "nobody asked" is not "no rows".
   */
  readEvents?: (sessionId: string) => Promise<D14AuditProjection>;
  /** Opens the audit trail scoped to one Core audit object. */
  onOpenAuditTrail?: (scope: { kind: string; id: string }) => void;
}

type Outcome = D16IntentResult["outcome"];

const IDLE_OUTCOME: Outcome = { state: "idle", reason: null };

/**
 * `AgentSessionStatus` → the design's glyph and its `.abses .stt` label key.
 *
 * Mapping only, never a second enumeration (`D-ROLES`): a status this build
 * cannot name keeps Core's own word and the neutral glyph rather than being
 * nearest-matched into one it does know.
 */
const STATUS_GLYPHS: Record<string, string> = {
  starting: "◌",
  running: "●",
  waiting_approval: "⏸",
  paused: "‖",
  completed: "✓",
  failed: "✕",
  cancelled: "⊗",
  killed: "⊘",
};

const STATUS_LABELS: Partial<Record<string, MessageKey>> = {
  starting: "d16.status.starting",
  running: "d16.status.running",
  waiting_approval: "d16.status.waitingApproval",
  paused: "d16.status.paused",
  completed: "d16.status.completed",
  failed: "d16.status.failed",
  cancelled: "d16.status.cancelled",
  killed: "d16.status.killed",
};

const ACTION_LABELS: Partial<Record<string, MessageKey>> = {
  pause: "d16.action.pause",
  resume: "d16.action.resume",
  kill: "d16.action.kill",
  stop: "d16.action.stop",
  retry: "d16.action.retry",
  switch_model: "d16.action.switchModel",
  downgrade_manual: "d16.action.downgradeManual",
  ask_user: "d16.action.askUser",
};

/** The `gate_strength` glyphs the design fixes (`D-GATESTR`). */
const GATE_GLYPHS: Record<string, string> = {
  full: "●",
  cooperative: "◐",
  containment: "○",
};

export function renderD16AgentBoard(
  root: HTMLElement,
  initial: D16AgentBoardProjection,
  locale: Locale,
  options: D16Options = {},
): D16Controller {
  let projection = initial;
  let outcome: Outcome = IDLE_OUTCOME;
  let disposed = false;
  let busy = false;
  /** The row the operator picked, or `null` to take the projection's own pick. */
  let selected: string | null = initial.selectedSessionId;
  /** Whether the two-step switch panel is open for the selected session. */
  let switchOpen = false;
  /** Step 2's model field, seeded from the model Core published. */
  let modelDraft: string | null = null;
  /** The audit page last read for `eventsFor`, or `null` while unread. */
  let events: D14AuditProjection | null = null;
  let eventsFor: string | null = null;
  let eventsInFlight = false;

  // Numbers are stringified here rather than at every call site: the catalog's
  // placeholder arguments are strings, and a count is the commonest argument on
  // this screen.
  const t = (key: MessageKey, args: Record<string, string | number> = {}): string =>
    translate(
      locale,
      key,
      Object.fromEntries(
        Object.entries(args).map(([name, value]) => [name, String(value)]),
      ) as never,
    );

  /**
   * The same block code as a short caption for the button face.
   *
   * The full sentence stays in the accessible name and the tooltip; the face
   * carries the design's own terse `.why` line, which is what keeps a disabled
   * control the size of an enabled one instead of a paragraph with a border.
   */
  const blockCaption = (block: string | null): string => {
    if (!block) return "";
    if (block === D16_CAPABILITY_ABSENT_CODE) return t("d16.why.capability", {});
    if (block === D16_SESSION_ENDED_CODE) return t("d16.why.ended", {});
    if (block === D16_SESSION_COMPLETED_CODE) return t("d16.why.completed", {});
    if (block === D16_NO_MODEL_CODE) return t("d16.why.noModel", {});
    if (block === D16_NO_CORE_FACT_CODE) return t("d16.why.noCoreFact", {});
    if (block.startsWith(D16_STATUS_BLOCK_PREFIX)) {
      const status = block.slice(D16_STATUS_BLOCK_PREFIX.length);
      const key = STATUS_LABELS[status];
      return t("d16.why.status", { status: key ? t(key, {}) : status });
    }
    return block;
  };

  /**
   * Turns a block code into a sentence.
   *
   * Core's own `session_status_<current>` family is rendered through the status
   * vocabulary the screen already has, so the operator reads "already paused"
   * rather than a protocol token; a code this build cannot name is printed
   * verbatim, because a refusal nobody can read is worse than a raw string.
   */
  const blockReason = (block: string | null): string => {
    if (!block) return "";
    if (block === D16_CAPABILITY_ABSENT_CODE) {
      return t("d16.block.capability", { capability: AGENT_SESSION_CONTROL_CAPABILITY });
    }
    if (block === D16_SESSION_ENDED_CODE) return t("d16.block.ended", {});
    if (block === D16_SESSION_COMPLETED_CODE) return t("d16.block.completed", {});
    if (block === D16_NO_MODEL_CODE) return t("d16.block.noModel", {});
    if (block === D16_NO_CORE_FACT_CODE) return t("d16.block.noCoreFact", {});
    if (block.startsWith(D16_STATUS_BLOCK_PREFIX)) {
      const status = block.slice(D16_STATUS_BLOCK_PREFIX.length);
      const key = STATUS_LABELS[status];
      return t("d16.block.status", { status: key ? t(key, {}) : status });
    }
    return block;
  };

  const currentSession = (): D16Session | null => {
    const wanted = selected ?? projection.selectedSessionId;
    if (!wanted) return null;
    for (const lane of projection.lanes) {
      const found = lane.sessions.find((session) => session.sessionId === wanted);
      if (found) return found;
    }
    return null;
  };

  const currentLane = (session: D16Session | null): D16Lane | null =>
    session ? (projection.lanes.find((lane) => lane.laneId === session.laneId) ?? null) : null;

  /**
   * Reads the session's audit page once per selection.
   *
   * One read per selected session and no polling: the timeline is a bounded
   * page of durable rows, not a live tail, and re-asking on every render would
   * put a Core command behind every keystroke.
   */
  const readEvents = (sessionId: string): void => {
    if (!options.readEvents || eventsInFlight || eventsFor === sessionId) return;
    eventsInFlight = true;
    void options
      .readEvents(sessionId)
      .then((next) => {
        eventsInFlight = false;
        if (disposed) return;
        events = next;
        eventsFor = sessionId;
        render();
      })
      .catch(() => {
        eventsInFlight = false;
      });
  };

  const select = (sessionId: string): void => {
    if (selected === sessionId) return;
    selected = sessionId;
    switchOpen = false;
    modelDraft = null;
    outcome = IDLE_OUTCOME;
    render();
    // Re-read rather than re-derive: the detail pane must be Core's answer for
    // the session the operator picked, not this render's slice of the last one.
    if (options.reload) {
      void options
        .reload(sessionId)
        .then((next) => {
          if (disposed) return;
          projection = next;
          render();
        })
        .catch(() => undefined);
    }
  };

  const send = (intent: D16Intent): void => {
    if (!options.send || busy) return;
    busy = true;
    outcome = { state: "pending", reason: null };
    render();
    void options
      .send(intent)
      .then((result) => {
        busy = false;
        if (disposed) return;
        projection = result.projection;
        outcome = result.outcome;
        // A control that landed re-reads the timeline: the pause, resume and
        // kill rows Core writes are exactly what the operator needs to see next.
        eventsFor = null;
        const session = currentSession();
        if (session) readEvents(session.sessionId);
        render();
      })
      .catch((error: unknown) => {
        busy = false;
        if (disposed) return;
        outcome = {
          state: "rejected",
          reason: error instanceof Error ? error.message : String(error),
        };
        render();
      });
  };

  /* ---- rendering ---- */

  const kv = (key: string, value: string, faint = false): HTMLElement => {
    const row = document.createElement("div");
    row.className = "evkv";
    const k = document.createElement("span");
    k.className = "k";
    k.textContent = key;
    const v = document.createElement("span");
    v.className = faint ? "v faint" : "v";
    v.textContent = value;
    row.append(k, v);
    return row;
  };

  const section = (title: string, slot: string): HTMLElement => {
    const wrap = document.createElement("div");
    wrap.className = "evsec";
    wrap.dataset.d16Section = slot;
    const heading = document.createElement("h5");
    heading.textContent = title;
    wrap.append(heading);
    return wrap;
  };

  const none = (text: string, slot: string): HTMLElement => {
    const row = document.createElement("p");
    row.className = "abnone";
    row.dataset.d16Absent = slot;
    row.textContent = text;
    return row;
  };

  const laneGroup = (lane: D16Lane, activeId: string | null): HTMLElement => {
    const group = document.createElement("section");
    group.className = lane.awaiting > 0 ? "ablane attn" : "ablane";
    group.dataset.d16Lane = lane.laneId;

    const head = document.createElement("header");
    head.className = "ablhd";
    const id = document.createElement("span");
    id.className = "lid";
    id.textContent = lane.laneId;
    head.append(id);
    const name = document.createElement("span");
    name.className = "nm";
    // A Lane with no published record says so instead of borrowing a name.
    name.textContent = lane.summary ?? t("d16.lane.unpublished", {});
    if (!lane.published) name.dataset.d16LaneUnpublished = "true";
    head.append(name);
    if (lane.branch) {
      const branch = document.createElement("span");
      branch.className = "br";
      branch.textContent = `⎇ ${lane.branch}`;
      head.append(branch);
    }
    if (lane.route) {
      const route = document.createElement("span");
      route.className = "abvia";
      route.dataset.d16Route = lane.route;
      route.textContent = lane.route;
      head.append(route);
    }
    if (lane.gateStrength) {
      const gate = document.createElement("span");
      gate.className = "abgs";
      gate.dataset.d16Gate = lane.gateStrength;
      gate.textContent = `${GATE_GLYPHS[lane.gateStrength] ?? "·"} ${lane.gateStrength}`;
      head.append(gate);
    }
    const spacer = document.createElement("span");
    spacer.className = "sp";
    head.append(spacer);
    const count = document.createElement("span");
    count.className = "ct";
    count.textContent = t("d16.lane.sessions", { count: lane.sessions.length });
    head.append(count);
    group.append(head);

    if (lane.sessions.length === 0) {
      group.append(none(t("d16.lane.noSessions", {}), "lane-sessions"));
      return group;
    }

    for (const session of lane.sessions) {
      const row = document.createElement("button");
      row.type = "button";
      row.className = session.sessionId === activeId ? "abses on" : "abses";
      row.dataset.d16Session = session.sessionId;
      if (session.sessionId === activeId) row.setAttribute("aria-current", "true");
      const glyph = document.createElement("span");
      glyph.className = `sg st-${session.status}`;
      glyph.textContent = STATUS_GLYPHS[session.status] ?? "·";
      glyph.setAttribute("aria-hidden", "true");
      const role = document.createElement("span");
      // Core publishes a role on the *Lane*, not on the session, so every
      // session under one Lane carries that Lane's role — which is true, and
      // is why the chip reads the Lane rather than inventing a per-session one.
      // A Lane Core published no record for has no role to show.
      role.textContent = lane.role ?? session.agentId;
      role.className = "role";
      const task = document.createElement("span");
      task.className = "task";
      task.textContent = session.task;
      const model = document.createElement("span");
      model.className = "mdl";
      // The design's backend·model pair: the adapter Core started and the model
      // it reported, in that order.
      model.textContent = `${session.agentId} · ${session.model ?? t("d16.model.absent", {})}`;
      const status = document.createElement("span");
      status.className = `stt st-${session.status}`;
      const statusKey = STATUS_LABELS[session.status];
      status.textContent = statusKey ? t(statusKey, {}) : session.status;
      row.append(glyph, role, task, model, status);
      if (session.diagnostic) {
        const diagnostic = document.createElement("span");
        diagnostic.className = session.status === "killed" ? "dx bad" : "dx";
        diagnostic.dataset.d16Diagnostic = session.sessionId;
        diagnostic.textContent = session.diagnostic;
        row.append(diagnostic);
      }
      row.setAttribute(
        "aria-label",
        `${session.agentId} · ${session.task} · ${status.textContent}`,
      );
      row.addEventListener("click", () => select(session.sessionId));
      group.append(row);
    }
    return group;
  };

  const actionButton = (session: D16Session, action: D16Action): HTMLButtonElement => {
    const button = document.createElement("button");
    button.type = "button";
    const classes = ["abact"];
    if (action.kind === "kill") classes.push("kill");
    if (action.request) classes.push("abreq");
    if (action.kind === "switch_model" && switchOpen) classes.push("go");
    button.className = classes.join(" ");
    button.dataset.d16Action = action.kind;
    const label = document.createElement("span");
    const key = ACTION_LABELS[action.kind];
    label.textContent = key ? t(key, {}) : action.kind;
    button.append(label);
    button.disabled = !action.available || busy || (!options.send && action.kind !== "switch_model");
    if (action.block) {
      // The reason is part of the accessible name, not only a caption: a
      // control an operator cannot press has to say why in the one string a
      // screen reader announces.
      const why = document.createElement("span");
      why.className = "why";
      why.dataset.d16Block = action.block;
      why.textContent = blockCaption(action.block);
      button.append(why);
      // The full sentence, not the caption: a control an operator cannot press
      // has to say why in the one string a screen reader announces.
      const sentence = blockReason(action.block);
      button.setAttribute("aria-label", `${label.textContent} — ${sentence}`);
      button.title = `${label.textContent} — ${sentence}`;
    } else if (action.kind === "switch_model") {
      const why = document.createElement("span");
      why.className = "why";
      why.textContent = t("d16.action.twoSteps", {});
      button.append(why);
    }
    if (!button.disabled) {
      button.addEventListener("click", () => {
        if (action.kind === "switch_model") {
          switchOpen = !switchOpen;
          modelDraft = switchOpen ? (session.model ?? "") : null;
          render();
          return;
        }
        send({ type: action.kind, sessionId: session.sessionId } as D16Intent);
      });
    }
    return button;
  };

  /** The two steps a model switch really is, spelled out and sent separately. */
  const switchPanel = (session: D16Session): HTMLElement => {
    const panel = document.createElement("section");
    panel.className = "abswap";
    panel.dataset.d16Switch = session.sessionId;
    const heading = document.createElement("p");
    heading.className = "sh";
    heading.textContent = t("d16.switch.head", {});
    panel.append(heading);

    const one = document.createElement("div");
    one.className = "abstep";
    const oneNumber = document.createElement("span");
    oneNumber.className = "n";
    oneNumber.textContent = "1";
    const oneText = document.createElement("span");
    oneText.textContent = t("d16.switch.stepOne", {});
    one.append(oneNumber, oneText);
    const oneActions = document.createElement("div");
    oneActions.className = "abstep-actions";
    for (const kind of ["kill", "stop"] as const) {
      const action = session.actions.find((item) => item.kind === kind);
      if (!action) continue;
      const button = actionButton(session, action);
      button.dataset.d16SwitchStep = "1";
      oneActions.append(button);
    }
    one.append(oneActions);
    panel.append(one);

    const two = document.createElement("div");
    two.className = "abstep";
    const twoNumber = document.createElement("span");
    twoNumber.className = "n";
    twoNumber.textContent = "2";
    const twoText = document.createElement("span");
    twoText.textContent = t("d16.switch.stepTwo", { session: session.sessionId });
    two.append(twoNumber, twoText);
    const field = verbatimField(document.createElement("input"));
    field.type = "text";
    field.className = "abswap-model";
    field.dataset.d16SwitchModel = "true";
    field.value = modelDraft ?? session.model ?? "";
    field.setAttribute("aria-label", t("d16.switch.modelLabel", {}));
    field.addEventListener("input", () => {
      modelDraft = field.value;
    });
    const start = document.createElement("button");
    start.type = "button";
    start.className = "abact go";
    start.dataset.d16Action = "start";
    start.dataset.d16SwitchStep = "2";
    start.textContent = t("d16.switch.start", {});
    // Step 2 is only meaningful once step 1 ended the session: starting a
    // second live session on the same Lane is not a *switch*, and the panel
    // must not let the two run at once.
    start.disabled = !options.send || busy || !session.terminal;
    if (!session.terminal) {
      const why = document.createElement("span");
      why.className = "why";
      why.textContent = t("d16.switch.endFirst", {});
      start.append(why);
      start.title = `${t("d16.switch.start", {})} — ${t("d16.switch.endFirst", {})}`;
    }
    start.addEventListener("click", () => {
      const model = (modelDraft ?? session.model ?? "").trim();
      send({
        type: "start",
        sessionId: session.sessionId,
        model: model === "" ? null : model,
      });
    });
    const actions = document.createElement("div");
    actions.className = "abstep-actions";
    actions.append(field, start);
    two.append(actions);
    panel.append(two);

    const foot = document.createElement("p");
    foot.className = "foot";
    foot.dataset.d16SwitchFoot = "true";
    foot.textContent = t("d16.switch.foot", {});
    panel.append(foot);
    return panel;
  };

  const detailPane = (): HTMLElement => {
    const pane = document.createElement("aside");
    pane.className = "abdet";
    pane.dataset.d16Detail = "true";
    const session = currentSession();
    if (!session) {
      const head = document.createElement("header");
      head.className = "evdethead";
      const title = document.createElement("p");
      title.className = "t1";
      title.textContent = t("d16.detail.none", {});
      const subtitle = document.createElement("p");
      subtitle.className = "t2";
      subtitle.textContent = t("d16.detail.noneHint", {});
      head.append(title, subtitle);
      const note = document.createElement("p");
      note.className = "abnote";
      note.dataset.d16Absent = "detail";
      note.textContent = t("d16.detail.noneBody", {});
      pane.append(head, note);
      return pane;
    }
    const lane = currentLane(session);
    readEvents(session.sessionId);

    const head = document.createElement("header");
    head.className = "evdethead";
    const title = document.createElement("p");
    title.className = "t1";
    const glyph = document.createElement("span");
    glyph.className = `sg st-${session.status}`;
    glyph.textContent = STATUS_GLYPHS[session.status] ?? "·";
    glyph.setAttribute("aria-hidden", "true");
    title.append(glyph, document.createTextNode(` ${session.agentId} · ${session.sessionId}`));
    const subtitle = document.createElement("p");
    subtitle.className = "t2";
    subtitle.textContent = [
      session.laneId,
      lane?.summary ?? t("d16.lane.unpublished", {}),
      lane?.branch ? `⎇ ${lane.branch}` : null,
      session.model ?? t("d16.model.absent", {}),
    ]
      .filter((part): part is string => part !== null)
      .join(" · ");
    head.append(title, subtitle);
    pane.append(head);

    const facts = section(t("d16.section.session", {}), "session");
    facts.append(kv(t("d16.field.task", {}), session.task));
    const statusKey = STATUS_LABELS[session.status];
    facts.append(
      kv(t("d16.field.status", {}), statusKey ? t(statusKey, {}) : session.status),
    );
    facts.append(
      kv(t("d16.field.route", {}), lane?.route ?? t("d16.field.absent", {}), !lane?.route),
    );
    facts.append(
      kv(
        t("d16.field.gate", {}),
        lane?.gateStrength
          ? `${GATE_GLYPHS[lane.gateStrength] ?? "·"} ${lane.gateStrength}`
          : t("d16.field.absent", {}),
        !lane?.gateStrength,
      ),
    );
    facts.append(
      kv(
        t("d16.field.diagnostic", {}),
        session.diagnostic ?? t("d16.field.none", {}),
        !session.diagnostic,
      ),
    );
    // The control record: present only once Core actually controlled the
    // session, so its absence is "never paused, resumed or terminated" rather
    // than a row of zeroes.
    if (session.control) {
      const control = session.control;
      const controlRow = document.createElement("div");
      controlRow.className = "evkv";
      controlRow.dataset.d16Control = session.sessionId;
      const k = document.createElement("span");
      k.className = "k";
      k.textContent = t("d16.field.control", {});
      const v = document.createElement("span");
      v.className = "v";
      v.textContent = [
        control.pausedAt !== null ? t("d16.control.pausedAt", { at: control.pausedAt }) : null,
        control.resumedAt !== null
          ? t("d16.control.resumedAt", { at: control.resumedAt })
          : null,
        control.terminatedBy !== null
          ? t("d16.control.terminatedBy", { by: control.terminatedBy })
          : null,
        t("d16.control.held", { count: control.heldInputCount }),
      ]
        .filter((part): part is string => part !== null)
        .join(" · ");
      controlRow.append(k, v);
      facts.append(controlRow);
    }
    pane.append(facts);

    const timeline = section(t("d16.section.events", {}), "events");
    if (!options.readEvents) {
      timeline.append(none(t("d16.events.unread", {}), "events-port"));
    } else if (events && events.capabilityAvailable === false) {
      timeline.append(none(t("d16.events.noCapability", {}), "events-capability"));
    } else if (!events || !events.loaded || eventsFor !== session.sessionId) {
      timeline.append(none(t("d16.events.loading", {}), "events-loading"));
    } else if (events.rows.length === 0) {
      timeline.append(none(t("d16.events.empty", {}), "events-empty"));
    } else {
      for (const row of events.rows) {
        const line = document.createElement("div");
        line.className = "abtl";
        line.dataset.d16Event = row.auditId;
        const time = document.createElement("span");
        time.className = "tm";
        time.textContent = String(row.timestamp);
        const kind = document.createElement("span");
        kind.className = "ki";
        kind.textContent = row.outcome === "denied" ? "✕" : "·";
        kind.setAttribute("aria-hidden", "true");
        const text = document.createElement("span");
        text.className = "tx";
        // Core's dotted action key is printed raw: it is the contract's stable
        // vocabulary, and localizing it would hide which row this is.
        text.textContent = `${row.action} · ${row.actorKind} · ${row.outcome}`;
        line.append(time, kind, text);
        timeline.append(line);
      }
    }
    if (projection.detail && projection.detail.conversation.length > 0) {
      const heading = document.createElement("p");
      heading.className = "abnote-sub";
      heading.textContent = t("d16.events.conversation", {});
      timeline.append(heading);
      for (const message of projection.detail.conversation) {
        const line = document.createElement("div");
        line.className = "abtl";
        line.dataset.d16Message = message.messageId;
        const time = document.createElement("span");
        time.className = "tm";
        // Core publishes no timestamp on a conversation message, so the column
        // is empty rather than filled with the audit row's time beside it.
        time.textContent = "";
        const kind = document.createElement("span");
        kind.className = "ki";
        kind.textContent = message.role === "assistant" ? "▸" : "‹";
        kind.setAttribute("aria-hidden", "true");
        const text = document.createElement("span");
        text.className = "tx";
        text.textContent = message.content;
        line.append(time, kind, text);
        timeline.append(line);
      }
    }
    if (options.onOpenAuditTrail) {
      const trail = document.createElement("button");
      trail.type = "button";
      trail.className = "d1-action";
      trail.dataset.d16AuditTrail = session.sessionId;
      trail.textContent = t("d16.events.openTrail", {});
      trail.addEventListener("click", () =>
        options.onOpenAuditTrail?.({ kind: "session", id: session.sessionId }),
      );
      timeline.append(trail);
    }
    pane.append(timeline);

    const evidence = section(t("d16.section.evidence", {}), "evidence");
    const rows = projection.detail?.evidence ?? [];
    if (rows.length === 0) {
      evidence.append(none(t("d16.evidence.empty", {}), "evidence"));
    } else {
      const chips = document.createElement("div");
      chips.className = "abchips";
      for (const row of rows) {
        const chip = document.createElement("span");
        chip.className = "evchip";
        chip.dataset.d16Evidence = row.id;
        chip.textContent = `${row.kind} · ${row.summary}`;
        chips.append(chip);
      }
      evidence.append(chips);
    }
    pane.append(evidence);

    const foot = document.createElement("div");
    foot.className = "evfoot";
    foot.dataset.d16Actions = session.sessionId;
    for (const action of session.actions) foot.append(actionButton(session, action));
    pane.append(foot);

    if (outcome.state === "rejected" && outcome.reason) {
      // Core's own refusal, verbatim. The board does not translate a refusal
      // into a friendlier sentence: the operator has to be able to match what
      // they read here against the audit row Core wrote.
      const alert = document.createElement("p");
      alert.className = "abrefusal";
      alert.setAttribute("role", "alert");
      alert.dataset.d16Refusal = outcome.reason.split(/[\s:]/)[0] ?? "rejected";
      alert.textContent = outcome.reason;
      pane.append(alert);
    }

    if (switchOpen) pane.append(switchPanel(session));

    const note = document.createElement("p");
    note.className = "abnote";
    note.textContent = t("d16.note", {});
    pane.append(note);
    return pane;
  };

  const render = (): void => {
    if (disposed) return;
    const stage = document.createElement("div");
    stage.className = "d16-stage";
    stage.dataset.d16Stage = "true";

    const centre = document.createElement("div");
    centre.className = "abmain";

    const top = document.createElement("header");
    top.className = "abtop";
    const title = document.createElement("h2");
    title.className = "t";
    title.textContent = t("d16.title", {});
    const counts = document.createElement("p");
    counts.className = "ct";
    counts.dataset.d16Counts = "true";
    counts.textContent =
      projection.lanes.length === 0
        ? t("d16.counts.noLane", {})
        : t("d16.counts", {
            lanes: projection.totalLanes,
            sessions: projection.totalSessions,
            awaiting: projection.awaitingTotal,
          });
    top.append(title, counts);
    if (!projection.capabilityAvailable) {
      // The board still reads without the capability — Lanes and sessions are
      // ordinary facts — so the banner names what is missing rather than
      // replacing the screen with an error.
      const banner = document.createElement("p");
      banner.className = "abbanner";
      banner.dataset.d16Absent = "capability";
      banner.textContent = t("d16.block.capability", {
        capability: AGENT_SESSION_CONTROL_CAPABILITY,
      });
      top.append(banner);
    }
    centre.append(top);

    const scroll = document.createElement("div");
    scroll.className = "abscroll";
    if (projection.lanes.length === 0) {
      const empty = document.createElement("div");
      empty.className = "abempty";
      empty.dataset.d16Empty = "no-lane";
      const heading = document.createElement("p");
      heading.className = "eh";
      heading.textContent = t("d16.empty.head", {});
      const body = document.createElement("p");
      body.className = "es";
      body.textContent = t("d16.empty.body", {});
      const rule = document.createElement("p");
      rule.className = "ek";
      rule.textContent = t("d16.empty.rule", {});
      empty.append(heading, body, rule);
      scroll.append(empty);
    } else {
      const activeId = currentSession()?.sessionId ?? null;
      for (const lane of projection.lanes) scroll.append(laneGroup(lane, activeId));
    }
    centre.append(scroll);

    stage.append(centre, detailPane());
    root.replaceChildren(stage);
  };

  render();
  return {
    applyProjection: (next) => {
      projection = next;
      render();
    },
    dispose: () => {
      disposed = true;
    },
  };
}
