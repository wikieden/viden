import { translate, type Locale } from "../i18n/catalog";
import type { RecentSessionView, RecentWorkState } from "../models/recent_work";
import { recentAgeLabel } from "./project_picker";
import "./welcome_center.css";

const BRAND_MARK_URL = new URL(
  "../../../../docs/viden-design/Viden/brand-assets/icon.svg",
  import.meta.url,
).href;

export interface WelcomeRecentWork {
  state: RecentWorkState;
  /** Sessions Core returned alongside the projects, for the per-project count. */
  sessions: readonly RecentSessionView[];
  /** Injected so relative ages stay deterministic in tests and captures. */
  now: number;
  /**
   * Opens one recent project. Welcome renders only when no workspace is bound,
   * so this replaces nothing and needs no switch confirmation — unlike the
   * project picker, which always has a workspace to tear down.
   */
  onOpenRecent: (canonicalRoot: string) => void | Promise<void>;
}

export function renderWelcomeCenter(
  root: HTMLElement,
  locale: Locale,
  openProject?: () => void | Promise<void>,
  recentWork?: WelcomeRecentWork,
): void {
  const welcome = document.createElement("section");
  welcome.className = "d1-welcome";
  welcome.dataset.d1Welcome = "true";
  welcome.setAttribute("aria-label", translate(locale, "d1.welcome.title", {}));

  const content = document.createElement("div");
  content.className = "d1-welcome-content";

  const brand = document.createElement("header");
  brand.className = "d1-welcome-brand";
  const mark = document.createElement("img");
  mark.src = BRAND_MARK_URL;
  mark.alt = "";
  mark.width = 92;
  mark.height = 92;
  const brandCopy = document.createElement("div");
  const eyebrow = document.createElement("p");
  eyebrow.className = "d1-welcome-eyebrow";
  eyebrow.textContent = translate(locale, "d1.welcome.eyebrow", {});
  const title = document.createElement("h2");
  title.textContent = translate(locale, "d1.welcome.title", {});
  const subtitle = document.createElement("p");
  subtitle.className = "d1-welcome-subtitle";
  subtitle.textContent = translate(locale, "d1.welcome.subtitle", {});
  brandCopy.append(eyebrow, title, subtitle);
  brand.append(mark, brandCopy);

  const start = document.createElement("section");
  start.className = "d1-welcome-section";
  const startTitle = document.createElement("h3");
  startTitle.textContent = translate(locale, "d1.welcome.getStarted", {});
  const open = document.createElement("button");
  open.type = "button";
  open.className = "d1-welcome-action";
  open.dataset.openProject = "true";
  open.setAttribute("aria-keyshortcuts", "Meta+O Control+O");
  open.disabled = !openProject;
  const openLabel = document.createElement("span");
  openLabel.textContent = translate(locale, "d1.welcome.openProject", {});
  const shortcut = document.createElement("kbd");
  shortcut.textContent = "⌘ O";
  open.append(openLabel, shortcut);
  open.addEventListener("click", () => {
    if (!openProject || open.disabled) return;
    open.disabled = true;
    open.setAttribute("aria-busy", "true");
    welcome.querySelector("[data-open-project-error]")?.remove();
    void Promise.resolve(openProject())
      .catch((error: unknown) => {
        const message = document.createElement("p");
        message.className = "d1-welcome-error";
        message.dataset.openProjectError = "true";
        message.setAttribute("role", "alert");
        message.textContent = String(error);
        start.append(message);
      })
      .finally(() => {
        if (!welcome.isConnected) return;
        open.disabled = false;
        open.removeAttribute("aria-busy");
      });
  });
  start.append(startTitle, open);

  // Recent work is a Core read (`QueryRecentWork` -> `RecentWorkLoaded`), not a
  // client scan of the session home. The four states stay distinct: an absent
  // capability, a Core rejection, and a genuinely empty inventory are different
  // facts, and collapsing them into one empty list would misreport Core.
  const recent = document.createElement("section");
  recent.className = "d1-welcome-section";
  recent.dataset.welcomeRecent = recentWork?.state.kind ?? "unavailable";
  const recentTitle = document.createElement("h3");
  recentTitle.textContent = translate(locale, "d1.welcome.recent", {});
  recent.append(recentTitle);

  /**
   * The note for a recent-work read that produced no list.
   *
   * Two different facts, two different headlines (E1 defect 9):
   *
   * - `unavailable` — Core answered the handshake and published no
   *   `runtime.recent_work`. That is a statement about Core, true whether or
   *   not a project is open, so it keeps the capability wording.
   * - `failed` — the read itself did not complete. Welcome is mounted *only*
   *   while no workspace is bound, so on this surface the overwhelmingly
   *   common cause is that there is no Core adapter yet because nothing has
   *   been opened. The host says `Core adapter is not connected`, which reads
   *   as a fault; here it is the ordinary first-run state, so the headline is
   *   D1's own `No project open` and the next step is named. The host's
   *   sentence is not discarded — it is kept underneath as the diagnostic,
   *   because a read that failed for some *other* reason must still be
   *   readable. "Core adapter is not connected" as a headline belongs to the
   *   bound-workspace surfaces, where the adapter really has gone away and the
   *   D6 connection state says so.
   */
  const stateNote = (detail: string, kind: string): HTMLElement => {
    const note = document.createElement("div");
    note.className = "d1-welcome-unavailable";
    note.dataset.unavailableFeature = "recent-work";
    note.dataset.recentState = kind;
    note.setAttribute("aria-disabled", "true");
    const title = document.createElement("strong");
    const firstRun = kind === "failed";
    title.textContent = translate(
      locale,
      firstRun ? "d1.welcome.noProject" : "d1.welcome.recentUnavailable",
      {},
    );
    const body = document.createElement("span");
    if (firstRun) {
      body.dataset.recentFirstRun = "true";
      body.textContent = translate(locale, "d1.welcome.recentFirstRun", {});
    } else {
      body.textContent = detail;
    }
    note.append(title, body);
    if (firstRun) {
      const reason = document.createElement("small");
      reason.className = "d1-welcome-recent-diagnostic";
      reason.dataset.recentFailureReason = "true";
      reason.textContent = detail;
      note.append(reason);
    }
    return note;
  };

  const state = recentWork?.state ?? {
    kind: "unavailable" as const,
    reason: translate(locale, "d1.recent.unavailable", { capability: "runtime.recent_work" }),
  };
  if (state.kind === "loading") {
    const note = document.createElement("p");
    note.className = "d1-welcome-recent-loading";
    note.dataset.recentState = "loading";
    note.setAttribute("role", "status");
    note.textContent = translate(locale, "d1.picker.recent.loading", {});
    recent.append(note);
  } else if (state.kind !== "loaded") {
    recent.append(stateNote(state.reason, state.kind));
  } else if (state.projects.length === 0) {
    const empty = document.createElement("p");
    empty.className = "d1-welcome-recent-empty";
    empty.dataset.recentState = "empty";
    empty.setAttribute("role", "status");
    empty.textContent = translate(locale, "d1.welcome.recentEmpty", {});
    recent.append(empty);
  } else {
    const list = document.createElement("ul");
    list.className = "d1-welcome-recent";
    for (const project of state.projects) {
      // The session count comes from the same bounded `RecentWorkLoaded` fact;
      // nothing is counted from a directory or a transcript.
      const sessions = (recentWork?.sessions ?? []).filter(
        (session) => session.canonicalRoot === project.canonicalRoot,
      ).length;
      const item = document.createElement("li");
      const open = document.createElement("button");
      open.type = "button";
      open.className = "d1-welcome-recent-item";
      open.dataset.recentProject = project.canonicalRoot;
      const name = document.createElement("strong");
      name.textContent = project.displayName;
      const meta = document.createElement("small");
      const age = recentAgeLabel(locale, project.lastUpdatedAt, recentWork?.now ?? Date.now());
      meta.textContent = `${age} · ${translate(
        locale,
        sessions === 1 ? "d1.welcome.recentSessions.one" : "d1.welcome.recentSessions.other",
        { count: String(sessions) },
      )}`;
      const path = document.createElement("small");
      path.className = "d1-welcome-recent-path";
      path.textContent = project.canonicalRoot;
      open.append(name, meta, path);
      open.addEventListener("click", () => {
        if (!recentWork) return;
        open.disabled = true;
        open.setAttribute("aria-busy", "true");
        welcome.querySelector("[data-open-project-error]")?.remove();
        void Promise.resolve(recentWork.onOpenRecent(project.canonicalRoot))
          .catch((error: unknown) => {
            const message = document.createElement("p");
            message.className = "d1-welcome-error";
            message.dataset.openProjectError = "true";
            message.setAttribute("role", "alert");
            message.textContent = String(error);
            recent.append(message);
          })
          .finally(() => {
            if (!welcome.isConnected) return;
            open.disabled = false;
            open.removeAttribute("aria-busy");
          });
      });
      item.append(open);
      list.append(item);
    }
    recent.append(list);
    for (const diagnostic of state.diagnostics) {
      // Core's diagnostics render verbatim; the client never rewords them.
      const note = document.createElement("p");
      note.className = "d1-welcome-recent-diagnostic";
      note.dataset.recentDiagnostic = diagnostic;
      note.textContent = diagnostic;
      recent.append(note);
    }
  }

  content.append(brand, start, recent);
  welcome.append(content);
  root.replaceChildren(welcome);
}
