import { translate, type Locale } from "../i18n/catalog";
import { relativeAge, type RecentWorkState } from "../models/recent_work";
import "./project_picker.css";

/**
 * The project picker popover.
 *
 * Visual vocabulary: the registered design component `ProjectPicker` in
 * `docs/viden-design/Viden/GUI/Viden - 桌面驾驶舱 (GUI).html` — three columns
 * (Add / In workspace / Recent). Popover behaviour follows the settings-panel
 * conventions: Escape closes and returns focus, an outside click closes, and
 * the focus hand-back resolves the live anchor rather than a detached one.
 *
 * Two design facts are deliberately *not* reproduced. The prototype's "Global"
 * section and its several project groups are mock data: `LocalCoreHost` opens
 * exactly one workspace, and a successful `open_workspace` replaces it. So the
 * "In workspace" column holds exactly one row — the project that is open — and
 * choosing anything else is a workspace *switch*, which is why it goes through
 * the inline confirmation step below rather than straight to `openWorkspace`.
 * Concurrent multi-workspace supervision is `GUI-CORE-023`.
 */

/** The contract-request code covering everything this picker cannot do yet. */
export const MULTI_WORKSPACE_CODE = "GUI-CORE-023";

export interface ProjectPickerCurrent {
  /** Core's project name when it published one, otherwise the workspace path. */
  displayName: string;
  /** `environment.cwd` — the root Core actually opened. */
  canonicalRoot: string;
  /** Lanes Core reports as running work, and therefore torn down on a switch. */
  activeLaneCount: number;
  /** Agent sessions Core reports as live, and therefore shut down on a switch. */
  activeSessionCount: number;
  /** Every Lane in the open workspace, running or not. */
  laneCount: number;
}

export interface ProjectPickerModel {
  locale: Locale;
  current: ProjectPickerCurrent;
  recent: RecentWorkState;
  /** Injected so relative ages stay deterministic in tests and captures. */
  now: number;
}

export interface ProjectPickerHandlers {
  /** Opens the native folder chooser; resolves null when the operator cancels. */
  onPickDirectory: () => Promise<string | null>;
  /** Replaces the open workspace with `root`. Only the confirmation calls this. */
  onSwitchWorkspace: (root: string) => Promise<void>;
  /**
   * Opens D11 project intake for the workspace already open.
   *
   * D11 configures a *project* — probe, `viden.toml` confirmation, credential
   * handles, starter Lanes. This picker is the project surface, so it is where
   * that flow belongs; it used to hang off the New Lane popover's "Full
   * setup…", which asked the operator to configure the project in order to
   * make one Lane. Absent while no host is bound, which leaves the row out
   * rather than rendering one that opens nothing.
   */
  onConfigureProject?: () => void;
  onClose: () => void;
}

export interface ProjectPickerController {
  root: HTMLElement;
  close: () => void;
}

/** Which anchor this picker was opened from, so focus returns to the live one. */
export type ProjectPickerAnchorKind = "titlebar" | "rail";

const ANCHOR_SELECTORS: Record<ProjectPickerAnchorKind, string> = {
  titlebar: "[data-project-selector]",
  rail: "[data-add-project]",
};

function row(className: string): HTMLButtonElement {
  const element = document.createElement("button");
  element.type = "button";
  element.className = className;
  return element;
}

function labelled(name: string, detail: string): HTMLSpanElement {
  const text = document.createElement("span");
  text.className = "tx";
  const primary = document.createElement("span");
  primary.className = "n";
  primary.textContent = name;
  const secondary = document.createElement("span");
  secondary.className = "s";
  secondary.textContent = detail;
  text.append(primary, secondary);
  return text;
}

/** Localizes one Core timestamp as a coarse "how long ago" chip. */
export function recentAgeLabel(locale: Locale, lastUpdatedAt: number, now: number): string {
  const age = relativeAge(lastUpdatedAt, now);
  if (age.unit === "now") return translate(locale, "d1.recent.age.now", {});
  return translate(locale, `d1.recent.age.${age.unit}` as const, {
    count: String(age.count),
  });
}

export function renderProjectPicker(
  anchor: HTMLElement,
  anchorKind: ProjectPickerAnchorKind,
  model: ProjectPickerModel,
  handlers: ProjectPickerHandlers,
): ProjectPickerController {
  const { locale, current } = model;

  const panel = document.createElement("div");
  panel.className = "ppick d1-project-picker";
  panel.dataset.projectPicker = "true";
  panel.setAttribute("role", "dialog");
  panel.setAttribute("aria-modal", "false");
  panel.setAttribute("aria-label", translate(locale, "d1.picker.title", {}));
  panel.tabIndex = -1;

  const header = document.createElement("header");
  header.className = "ppick-header";
  const heading = document.createElement("h2");
  heading.className = "ppick-heading";
  heading.textContent = translate(locale, "d1.picker.title", {});
  const close = document.createElement("button");
  close.type = "button";
  close.className = "ppick-close";
  close.dataset.pickerClose = "true";
  close.setAttribute("aria-label", translate(locale, "d1.picker.close", {}));
  close.textContent = "×";
  close.addEventListener("click", () => controller.close());
  header.append(heading, close);

  const body = document.createElement("div");
  body.className = "ppick-body";

  /// Renders one alert with the exact text Core (or the host) produced.
  const alert = (message: string): HTMLElement => {
    const element = document.createElement("p");
    element.className = "ppick-alert";
    element.dataset.pickerError = "true";
    element.setAttribute("role", "alert");
    element.textContent = message;
    return element;
  };

  /// The inline switch confirmation.
  ///
  /// Opening a project is not additive: `LocalCoreHost::open_workspace` builds
  /// a new supervisor and the GUI host swaps its single adapter slot, so the
  /// current workspace — and every Lane and resident ACP session in it — is
  /// torn down. That is stated with the counts Core published before the
  /// operator can confirm, never after.
  const renderConfirm = (root: string): void => {
    const confirm = document.createElement("section");
    confirm.className = "ppick-confirm";
    confirm.dataset.pickerConfirm = "true";

    const title = document.createElement("h3");
    title.textContent = translate(locale, "d1.picker.confirm.title", {});

    const target = document.createElement("p");
    target.className = "ppick-confirm-target";
    target.dataset.pickerConfirmTarget = "true";
    target.textContent = root;

    const explanation = document.createElement("p");
    explanation.textContent = translate(locale, "d1.picker.confirm.body", {
      code: MULTI_WORKSPACE_CODE,
    });

    // Zero running work is still a replacement, so it is still confirmed —
    // only the sentence is milder, because nothing is interrupted mid-flight.
    const running = current.activeLaneCount + current.activeSessionCount > 0;
    const impact = document.createElement("p");
    impact.dataset.pickerConfirmImpact = running ? "running" : "idle";
    impact.textContent = running
      ? translate(locale, "d1.picker.confirm.impact", {
          lanes: String(current.activeLaneCount),
          sessions: String(current.activeSessionCount),
          project: current.displayName,
        })
      : translate(locale, "d1.picker.confirm.impactIdle", {
          project: current.displayName,
        });

    const actions = document.createElement("div");
    actions.className = "ppick-confirm-actions";
    const cancel = document.createElement("button");
    cancel.type = "button";
    cancel.dataset.pickerConfirmCancel = "true";
    cancel.textContent = translate(locale, "d1.picker.confirm.cancel", {});
    cancel.addEventListener("click", () => renderColumns());
    const accept = document.createElement("button");
    accept.type = "button";
    accept.className = "ppick-confirm-accept";
    accept.dataset.pickerConfirmAccept = "true";
    accept.textContent = translate(locale, "d1.picker.confirm.accept", {});
    accept.addEventListener("click", () => {
      accept.disabled = true;
      cancel.disabled = true;
      accept.setAttribute("aria-busy", "true");
      void Promise.resolve(handlers.onSwitchWorkspace(root))
        .then(() => {
          // The shell re-renders the cockpit against the new workspace; the
          // popover must not outlive the projection it was built from.
          controller.close();
        })
        .catch((error: unknown) => {
          if (!panel.isConnected) return;
          accept.disabled = false;
          cancel.disabled = false;
          accept.removeAttribute("aria-busy");
          confirm.querySelector("[data-picker-error]")?.remove();
          confirm.append(alert(String(error)));
        });
    });
    actions.append(cancel, accept);

    confirm.append(title, target, explanation, impact, actions);
    body.replaceChildren(confirm);
    accept.focus();
  };

  /// Runs the native folder chooser, then the same confirmation a recent row
  /// takes. Cancelling the chooser leaves the picker exactly as it was.
  const addDirectory = (trigger: HTMLButtonElement): void => {
    trigger.disabled = true;
    trigger.setAttribute("aria-busy", "true");
    body.querySelector("[data-picker-error]")?.remove();
    void Promise.resolve(handlers.onPickDirectory())
      .then((selected) => {
        if (!panel.isConnected) return;
        if (selected === null) {
          trigger.disabled = false;
          trigger.removeAttribute("aria-busy");
          trigger.focus();
          return;
        }
        renderConfirm(selected);
      })
      .catch((error: unknown) => {
        if (!panel.isConnected) return;
        trigger.disabled = false;
        trigger.removeAttribute("aria-busy");
        body.append(alert(String(error)));
      });
  };

  const renderRecent = (column: HTMLElement): void => {
    switch (model.recent.kind) {
      case "loading": {
        const note = document.createElement("p");
        note.className = "ppick-note";
        note.dataset.pickerRecentLoading = "true";
        note.setAttribute("role", "status");
        note.textContent = translate(locale, "d1.picker.recent.loading", {});
        column.append(note);
        return;
      }
      case "unavailable":
      case "failed": {
        // An absent capability and a Core rejection are different facts and
        // stay different rows; neither is flattened into "no recent projects".
        const note = document.createElement("p");
        note.className = "ppick-note";
        note.dataset.pickerRecentState = model.recent.kind;
        note.setAttribute("role", "status");
        note.textContent = model.recent.reason;
        column.append(note);
        return;
      }
      case "loaded": {
        // Core's own root is already the "In workspace" row; repeating it under
        // Recent would offer a switch to the project already open.
        const rows = model.recent.projects.filter(
          (project) => project.canonicalRoot !== current.canonicalRoot,
        );
        if (rows.length === 0) {
          const note = document.createElement("p");
          note.className = "ppick-note";
          note.dataset.pickerRecentState = "empty";
          note.setAttribute("role", "status");
          note.textContent = translate(locale, "d1.picker.recent.empty", {});
          column.append(note);
        }
        for (const project of rows) {
          const item = row("pprow rec");
          item.dataset.pickerRecent = project.canonicalRoot;
          item.append(labelled(project.displayName, project.canonicalRoot));
          const when = document.createElement("span");
          when.className = "when";
          when.textContent = recentAgeLabel(locale, project.lastUpdatedAt, model.now);
          item.append(when);
          item.addEventListener("click", () => renderConfirm(project.canonicalRoot));
          column.append(item);
        }
        for (const diagnostic of model.recent.diagnostics) {
          // Core's diagnostics are rendered verbatim by code, never reworded.
          const note = document.createElement("p");
          note.className = "ppick-note";
          note.dataset.pickerRecentDiagnostic = diagnostic;
          note.textContent = diagnostic;
          column.append(note);
        }
      }
    }
  };

  function renderColumns(): void {
    const add = document.createElement("section");
    add.className = "ppcol";
    const addHeading = document.createElement("div");
    addHeading.className = "ppch";
    addHeading.textContent = translate(locale, "d1.picker.add", {});
    add.append(addHeading);

    const directory = row("ppadd");
    directory.dataset.pickerAdd = "directory";
    const directoryIcon = document.createElement("span");
    directoryIcon.className = "i";
    directoryIcon.setAttribute("aria-hidden", "true");
    directoryIcon.textContent = "＋";
    directory.append(
      directoryIcon,
      labelled(
        translate(locale, "d1.picker.add.directory", {}),
        translate(locale, "d1.picker.add.directoryHint", {}),
      ),
    );
    directory.addEventListener("click", () => addDirectory(directory));
    add.append(directory);

    // Core publishes no clone and no scaffold command. These rows stay visible
    // and disabled, naming the request, rather than enabled-and-inert.
    for (const [action, glyph] of [
      ["clone", "⎘"],
      ["empty", "⊞"],
    ] as const) {
      const item = row("ppadd");
      item.dataset.pickerAdd = action;
      item.dataset.pickerUnavailableCode = MULTI_WORKSPACE_CODE;
      item.disabled = true;
      item.title = translate(locale, "d1.picker.add.unavailable", {
        code: MULTI_WORKSPACE_CODE,
      });
      const icon = document.createElement("span");
      icon.className = "i";
      icon.setAttribute("aria-hidden", "true");
      icon.textContent = glyph;
      item.append(
        icon,
        labelled(
          translate(locale, `d1.picker.add.${action}` as const, {}),
          translate(locale, "d1.picker.add.unavailable", { code: MULTI_WORKSPACE_CODE }),
        ),
      );
      add.append(item);
    }

    const workspace = document.createElement("section");
    workspace.className = "ppcol mid";
    const workspaceHeading = document.createElement("div");
    workspaceHeading.className = "ppch";
    workspaceHeading.textContent = translate(locale, "d1.picker.inWorkspace", {});
    const count = document.createElement("span");
    count.className = "ct";
    // Exactly one, always: Core supervises one workspace at a time.
    count.textContent = "1";
    workspaceHeading.append(count);
    workspace.append(workspaceHeading);

    // The open project is not a switch target, so it is a status row rather
    // than a button: there is nothing to press.
    const currentRow = document.createElement("div");
    currentRow.className = "pprow on";
    currentRow.dataset.pickerCurrent = current.canonicalRoot;
    currentRow.setAttribute("aria-current", "true");
    const dot = document.createElement("span");
    dot.className = "dot";
    dot.setAttribute("aria-hidden", "true");
    const lanes = document.createElement("span");
    lanes.className = "lc";
    lanes.textContent = translate(
      locale,
      current.laneCount === 1 ? "d1.picker.lanes.one" : "d1.picker.lanes.other",
      { count: String(current.laneCount) },
    );
    currentRow.append(dot, labelled(current.displayName, current.canonicalRoot), lanes);
    workspace.append(currentRow);

    if (handlers.onConfigureProject) {
      const configure = row("pprow ppadd");
      configure.dataset.pickerConfigure = "true";
      const icon = document.createElement("span");
      icon.className = "i";
      icon.setAttribute("aria-hidden", "true");
      icon.textContent = "⛭";
      configure.append(
        icon,
        labelled(
          translate(locale, "d1.picker.configure", {}),
          translate(locale, "d1.picker.configureHint", {}),
        ),
      );
      configure.addEventListener("click", () => {
        handlers.onConfigureProject?.();
        // D11 replaces the window, so the popover must not outlive it.
        controller.close();
      });
      workspace.append(configure);
    }

    const recent = document.createElement("section");
    recent.className = "ppcol";
    const recentHeading = document.createElement("div");
    recentHeading.className = "ppch";
    recentHeading.textContent = translate(locale, "d1.picker.recent", {});
    recent.append(recentHeading);
    renderRecent(recent);

    body.replaceChildren(add, workspace, recent);
  }

  renderColumns();
  panel.append(header, body);

  const anchorRect = anchor.getBoundingClientRect();
  panel.style.setProperty("--ppick-anchor-inline", `${anchorRect.left}px`);
  panel.style.setProperty("--ppick-anchor-block", `${anchorRect.bottom}px`);
  anchor.setAttribute("aria-expanded", "true");
  // Portalled out of the frame so the popover is not clipped by the titlebar
  // or by the auto-hiding lane rail, matching the settings panel.
  (anchor.closest(".d1-frame")?.parentElement ?? document.body).append(panel);

  // The cockpit rebuilds the titlebar and the rail on every Core refresh, so
  // the node this popover was anchored to may already be detached. Both the
  // outside-click guard and the focus hand-back resolve the live anchor
  // instead — otherwise the trigger could no longer close its own popover, and
  // Escape would drop focus to the document body.
  const anchorSelector = ANCHOR_SELECTORS[anchorKind];
  const liveAnchor = (): HTMLElement =>
    anchor.isConnected
      ? anchor
      : (document.querySelector<HTMLElement>(anchorSelector) ?? anchor);

  let closed = false;
  const outside = (event: MouseEvent): void => {
    const target = event.target;
    if (panel.contains(target as Node)) return;
    if (target instanceof Element && target.closest(anchorSelector)) return;
    controller.close();
  };
  panel.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    // Escape inside the confirmation backs out of the switch rather than
    // dismissing the picker, so one keystroke never both cancels and closes.
    if (body.querySelector("[data-picker-confirm]")) {
      renderColumns();
      body.querySelector<HTMLButtonElement>('[data-picker-add="directory"]')?.focus();
      return;
    }
    controller.close();
  });

  const controller: ProjectPickerController = {
    root: panel,
    close: () => {
      if (closed) return;
      closed = true;
      document.removeEventListener("mousedown", outside);
      panel.remove();
      const trigger = liveAnchor();
      trigger.setAttribute("aria-expanded", "false");
      trigger.focus();
      handlers.onClose();
    },
  };
  document.addEventListener("mousedown", outside);
  return controller;
}
