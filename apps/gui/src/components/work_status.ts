import { translate, type Locale, type MessageKey } from "../i18n/catalog";
import type { D1CockpitProjection } from "../models/workspace";
import { localizedStatus } from "./tool_row";

/// Live turn feedback for the D1 work surface.
///
/// While Core is working, the operator needs four things the transcript cannot
/// give them: that work is still moving, *what kind* of work it is, how long
/// it has been moving, and how to stop it. Since `runtime.turn_lifecycle` (C6)
/// all four are Core's own facts:
///
/// - **the source** separates a prompt just typed from one an earlier
///   `QueueFollowUp` put in line and from an Agent session run. Without it a
///   drained queue entry reads as Core inventing work;
/// - **the elapsed clock** counts from Core's `started_at` rather than from
///   when this webview noticed. A client clock restarts on every remount, so a
///   turn running for ten minutes read as ten seconds;
/// - **the queue line** says which promise applies. Core drains the queue only
///   behind a *completed* turn, so "runs after the current turn" and "waits for
///   the next completed turn" are two different statements and collapsing them
///   would tell an operator their queue is about to run when Core has already
///   decided it is not;
/// - **a non-completed turn** keeps its outcome, because a composer that went
///   quiet with no sentence is the state an operator cannot act on.
///
/// The client-observed clock survives as the fallback for the one busy state
/// C6 does not bracket: an Agent session Core reports as `Starting` or
/// `WaitingApproval` before its turn fact exists. It says which it is showing.

export interface WorkStatusModel {
  busy: boolean;
  /// Core session status for the selected Lane, when Core scopes one to it.
  coreStatus: string | null;
  /// The task text Core recorded for the running session.
  task: string | null;
  canCancel: boolean;
  reducedMotion: boolean;
  /// The active turn's `source`, or `null` when Core published no turn for
  /// this target. Never defaulted to `user_input`: "nobody said" and "the
  /// operator typed it" are different facts.
  turnSource: string | null;
  /// Core's own start for that turn, in seconds. `null` falls back to the
  /// observed clock, labelled as such.
  turnStartedAt: number | null;
  /// How many inputs Core is holding behind the current turn.
  queuedCount: number;
  /// The last turn Core ended without completing it, when there is one.
  lastFailure: { outcome: string; reason: string | null } | null;
}

export function workStatusModel(
  projection: D1CockpitProjection,
  selectedLaneId: string | null,
  canCancel: boolean,
): WorkStatusModel {
  const session = selectedLaneId
    ? projection.agentSessions.find((candidate) => candidate.laneId === selectedLaneId)
    : undefined;
  // Exactly the turn Core scoped to this composer's target: the selected Lane,
  // or no Lane at all for the session-scoped composer. Another Lane's turn is
  // that Lane's work and never described here.
  const turn = (projection.activeTurns ?? []).find(
    (candidate) => (candidate.laneId ?? null) === selectedLaneId,
  );
  const failure = projection.turnFailure ?? null;
  return {
    busy: projection.composer.busy,
    // The Agent session is the only fact that carries a status scoped to this
    // Lane. Without one the strip says so rather than borrowing another
    // Lane's status.
    coreStatus: session?.status ?? null,
    task: session?.task ?? null,
    canCancel,
    reducedMotion: projection.preferences.motion === "reduced",
    turnSource: turn?.source ?? null,
    turnStartedAt: turn?.startedAt ?? null,
    queuedCount: projection.liveWork.queuedInputs.length,
    lastFailure:
      failure && (failure.laneId ?? null) === selectedLaneId
        ? { outcome: failure.outcome, reason: failure.reason }
        : null,
  };
}

/// Core's own source vocabulary, mapped to the strip's words. A source absent
/// from this table is named as unnamed; nothing is guessed.
const TURN_SOURCE_KEYS: Record<string, MessageKey> = {
  user_input: "d1.work.source.typed",
  queued_input: "d1.work.source.queued",
  agent_session: "d1.work.source.agent",
};

export function formatElapsed(milliseconds: number): string {
  const total = Math.max(0, Math.floor(milliseconds / 1000));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

export interface WorkStatusStrip {
  element: HTMLElement;
  /// Stops the elapsed clock. Always call before dropping the element.
  dispose: () => void;
}

export function renderWorkStatus(
  model: WorkStatusModel,
  locale: Locale,
  startedAt: number,
  now: () => number,
  onCancel: () => void,
): WorkStatusStrip {
  const strip = document.createElement("div");
  strip.className = "d1-work-status";
  strip.dataset.workStatus = model.busy ? "busy" : "idle";
  strip.setAttribute("role", "status");

  /// The queue line, which is the same whether or not a turn is running: the
  /// difference is *which* promise it can make.
  const appendQueue = (): void => {
    if (model.queuedCount <= 0) return;
    const queue = document.createElement("span");
    queue.className = "d1-work-queue";
    queue.dataset.workQueue = "true";
    // Core arms the drain on a *completed* turn. While one is running the next
    // input really is next; after a turn that failed or was cancelled the
    // queue waits for a completed one, and saying "runs next" there would be a
    // promise Core has already declined to keep.
    const stalled = !model.busy && model.lastFailure !== null;
    queue.textContent = translate(
      locale,
      stalled ? "d1.work.queue.stalled" : "d1.work.queue.next",
      { count: String(model.queuedCount) },
    );
    strip.append(queue);
  };

  /// The outcome of a turn Core ended without completing it.
  const appendFailure = (): void => {
    if (!model.lastFailure) return;
    const note = document.createElement("span");
    note.className = "d1-work-outcome";
    note.dataset.workOutcome = model.lastFailure.outcome;
    note.textContent =
      model.lastFailure.outcome === "failed" && model.lastFailure.reason
        ? translate(locale, "d1.work.failed", { reason: model.lastFailure.reason })
        : translate(
            locale,
            model.lastFailure.outcome === "cancelled"
              ? "d1.work.cancelled"
              : "d1.work.endedUnknown",
            {},
          );
    strip.append(note);
  };

  if (!model.busy) {
    const idle = document.createElement("span");
    idle.textContent = translate(locale, "d1.stream.idle", {});
    strip.append(idle);
    // An idle strip still carries both, because a queue nobody is draining and
    // a turn that stopped are exactly what an operator needs to read there.
    appendFailure();
    appendQueue();
    return { element: strip, dispose: () => undefined };
  }

  if (model.coreStatus) strip.dataset.workCoreStatus = model.coreStatus;

  const marker = document.createElement("span");
  marker.className = "d1-work-marker";
  marker.dataset.workMarker = "true";
  marker.ariaHidden = "true";
  // Motion is a Core-owned preference; a reduced-motion operator gets a
  // static marker rather than a spinner.
  marker.dataset.motion = model.reducedMotion ? "reduced" : "animated";
  strip.append(marker);

  const label = document.createElement("span");
  label.className = "d1-work-label";
  label.textContent = model.coreStatus
    ? localizedStatus(locale, model.coreStatus)
    : translate(locale, "d1.work.unscopedStatus", {});
  if (!model.coreStatus) label.title = "GUI-CORE-010";
  strip.append(label);

  if (model.turnSource) {
    // Core's own explanation for the work on screen. An unmodelled source is
    // named as unnamed rather than folded into `typed`, which would credit the
    // operator with a prompt they never wrote.
    const source = document.createElement("span");
    source.className = "d1-work-source";
    source.dataset.workSource = model.turnSource;
    source.textContent = translate(locale, TURN_SOURCE_KEYS[model.turnSource] ?? "d1.work.source.unknown", {});
    strip.append(source);
  }

  if (model.task) {
    const task = document.createElement("span");
    task.className = "d1-work-task";
    task.dataset.workTask = "true";
    task.textContent = model.task;
    strip.append(task);
  }

  const elapsed = document.createElement("time");
  elapsed.className = "d1-work-elapsed";
  elapsed.dataset.workElapsed = "true";
  // Screen readers should not hear a per-second tick inside a live region.
  elapsed.setAttribute("aria-hidden", "true");
  // Core's `started_at` when it published one, and the observed clock only as
  // the labelled fallback for a busy state C6 does not bracket.
  const coreStart = model.turnStartedAt === null ? null : model.turnStartedAt * 1000;
  elapsed.dataset.workElapsedSource = coreStart === null ? "client" : "core";
  elapsed.title = translate(
    locale,
    coreStart === null ? "d1.work.elapsedSource" : "d1.work.elapsedCore",
    {},
  );
  const paint = (): void => {
    elapsed.textContent = formatElapsed(now() - (coreStart ?? startedAt));
  };
  paint();
  const timer = window.setInterval(paint, 1000);
  strip.append(elapsed);

  if (model.canCancel) {
    const cancel = document.createElement("button");
    cancel.type = "button";
    cancel.className = "d1-work-cancel";
    cancel.dataset.workCancel = "true";
    cancel.textContent = translate(locale, "d1.work.cancel", {});
    cancel.addEventListener("click", onCancel);
    strip.append(cancel);
  }

  appendQueue();
  appendFailure();

  return {
    element: strip,
    dispose: () => window.clearInterval(timer),
  };
}
