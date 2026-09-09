import { translate, type Locale } from "../i18n/catalog";
import { formatShortcut } from "../i18n/format";
import type { D1CockpitProjection, TopbarSourceProjection } from "../models/workspace";
import { createCanonicalGuiIcon } from "./activity_rail";

const BRAND_MARK_URL = new URL(
  "../../../../docs/viden-design/Viden/brand-assets/icon.svg",
  import.meta.url,
).href;

/// The design's branch glyph, shared by the project selector and the worktree
/// chip. Registered in the GUI design kit; never an emoji.
const BRANCH_GLYPH = "⎇";

export interface CockpitTopbar {
  element: HTMLElement;
  contextDrawerToggle: HTMLButtonElement;
  /**
   * The design's `.tbtbtn` command-palette button. Disabled while no cockpit
   * handler is bound (the pre-Core shell), which keeps it visible and honest
   * rather than opening an overlay with nothing to act on.
   */
  commandPaletteToggle: HTMLButtonElement;
}

export interface CockpitTopbarOptions {
  /// Opens a restored screen, optionally preselecting one Core id.
  onNavigate?: (route: string, arg?: string) => void;
  onToggleCommandPalette?: () => void;
  commandPaletteOpen?: boolean;
  /**
   * Opens the project picker from the `.projsel` selector. Absent while no
   * host is bound and on the no-project Welcome, where there is no workspace
   * to switch away from; the selector then stays the design's inert label
   * rather than a chevron that cannot open anything.
   */
  onOpenProjectPicker?: () => void;
  projectPickerOpen?: boolean;
  /**
   * Opens the DiffReview view from the titlebar's changes marker.
   *
   * The marker is a *read* entry, not a git action: it opens a pane that
   * renders `QueryWorkspaceDiff`. That is why it can be a button while the
   * sync chip beside it stays a `role=status` element — GUI-CORE-020 governs
   * acting on the branch, not looking at it.
   *
   * Absent while no host is bound, which leaves the design's inert `●`.
   */
  onOpenReview?: () => void;
  /** Core published `runtime.structured_diff`; false disables the control. */
  reviewAvailable?: boolean;
  reviewOpen?: boolean;
}

/// The `.gitops` block: the workspace's source-control facts exactly as the
/// host projected them.
///
/// Read-only by contract. frontend-contract-v1 publishes no operator git
/// command (`GUI-CORE-020`), so the sync chip is a status element rather than
/// a button, and the only control is the worktree chip, which navigates.
function renderGitOps(
  source: TopbarSourceProjection,
  locale: Locale,
  onNavigate?: (route: string) => void,
): HTMLElement {
  const gitops = document.createElement("span");
  gitops.className = "gitops d1-topbar-gitops";
  gitops.dataset.topbarGitops = "true";
  gitops.dataset.tauriDragRegion = "true";

  const sync = document.createElement("span");
  sync.className = "gitchip d1-topbar-sync";
  sync.dataset.topbarSync = "true";
  // Status, not a control: there is nothing to press, so it must not be
  // announced or focused as if there were.
  sync.setAttribute("role", "status");
  sync.title = translate(locale, "d1.topbar.syncTitle", {
    ahead: String(source.ahead),
    behind: String(source.behind),
  });
  const ahead = document.createElement("span");
  ahead.className = "up";
  ahead.textContent = `↑${source.ahead}`;
  const behind = document.createElement("span");
  behind.className = "down";
  behind.textContent = `↓${source.behind}`;
  sync.append(ahead, behind);
  gitops.append(sync);

  if (source.truncated) {
    // Core sampled only part of the workspace. The counts above stay visible
    // because they are real, but they are never presented as complete.
    const marker = document.createElement("span");
    marker.className = "d1-topbar-truncated";
    marker.dataset.topbarTruncated = "true";
    marker.textContent = "…";
    marker.title = translate(locale, "d1.topbar.truncated", {});
    gitops.append(marker);
  }

  const worktrees = document.createElement("button");
  worktrees.type = "button";
  worktrees.className = "gitchip d1-topbar-worktrees";
  worktrees.dataset.topbarWorktrees = "true";
  worktrees.title = translate(locale, "d1.topbar.worktreesTitle", {});
  // Two catalog forms rather than one, so a single worktree does not read as
  // "1 worktrees". Locales without a plural distinction carry the same string.
  const worktreeLabel =
    source.laneWorktreeCount === 1 ? "d1.topbar.worktrees.one" : "d1.topbar.worktrees.other";
  worktrees.textContent = `${BRANCH_GLYPH} ${translate(locale, worktreeLabel, {
    count: String(source.laneWorktreeCount),
  })}`;
  // The Lane monitor is where the project's worktrees are actually inspected.
  // Without a router the chip stays visible and inert rather than lying.
  worktrees.disabled = !onNavigate;
  worktrees.addEventListener("click", () => onNavigate?.("d10"));
  gitops.append(worktrees);

  return gitops;
}

export function renderCockpitTopbar(
  projection: D1CockpitProjection,
  locale: Locale,
  showWelcome: boolean,
  onNavigate?: (route: string, arg?: string) => void,
  options: CockpitTopbarOptions = {},
): CockpitTopbar {
  const titlebar = document.createElement("header");
  titlebar.className = "titlebar vbar d1-titlebar";
  titlebar.dataset.shellLandmark = "topbar";
  titlebar.dataset.tauriDragRegion = "true";

  // Inside the native shell the OS overlay supplies the real window controls;
  // rendering the design-kit lights as well duplicates the chrome. The HTML
  // lights exist only for browser-hosted harnesses and previews.
  const nativeShell = "__TAURI_INTERNALS__" in window;
  const lights = document.createElement("span");
  lights.className = "tl";
  lights.ariaHidden = "true";
  for (const lightClass of ["a", "b", "c"]) {
    const light = document.createElement("i");
    light.className = lightClass;
    lights.append(light);
  }

  const brand = document.createElement("span");
  brand.className = "d1-topbar-brand";
  brand.dataset.tauriDragRegion = "true";
  const mark = document.createElement("img");
  mark.src = BRAND_MARK_URL;
  mark.alt = "";
  mark.width = 24;
  mark.height = 24;
  const wordmark = document.createElement("strong");
  wordmark.textContent = "viden";
  brand.append(mark, wordmark);

  const source = showWelcome ? null : projection.topbarSource;
  /// The dirty marker when it is a control; appended beside `.projsel`
  /// because a button cannot be nested inside another button.
  let reviewEntry: HTMLButtonElement | null = null;

  // The design draws a `▾` project picker here. The chevron and the button
  // semantics appear only when a handler is bound and a workspace is open —
  // an enabled control that cannot open anything is worse than no control.
  const pickerAvailable = !showWelcome && !!options.onOpenProjectPicker;
  const project = document.createElement(pickerAvailable ? "button" : "span");
  project.className = "projsel d1-topbar-project";
  if (project instanceof HTMLButtonElement) {
    project.type = "button";
    project.dataset.projectSelector = "true";
    project.setAttribute("aria-haspopup", "dialog");
    project.setAttribute("aria-expanded", String(options.projectPickerOpen === true));
    project.title = translate(locale, "d1.picker.open", {});
    project.addEventListener("click", () => options.onOpenProjectPicker?.());
  } else {
    // A drag region swallows clicks, so only the inert label claims one.
    project.dataset.tauriDragRegion = "true";
  }
  if (showWelcome) {
    project.textContent = translate(locale, "d1.welcome.noProject", {});
  } else {
    // Core's project name when it published one; otherwise the workspace path
    // it did publish. A name is never derived from the path.
    project.append(source?.project ?? projection.environment.cwd);
    if (source?.branch) {
      const branch = document.createElement("span");
      branch.className = "br";
      branch.textContent = `${BRANCH_GLYPH} ${source.branch}`;
      project.append(" ", branch);
    }
    if (source?.dirty) {
      // The design's dirty marker, promoted to the review entry point when a
      // host is bound. It is deliberately outside the `.projsel` button when
      // it is itself a control — a button inside a button is not focusable —
      // and stays the plain inert marker otherwise.
      if (options.onOpenReview) {
        const marker = document.createElement("button");
        marker.type = "button";
        marker.className = "d1-topbar-dirty d1-topbar-review";
        marker.dataset.topbarDirty = "true";
        marker.dataset.topbarReview = "true";
        marker.textContent = "●";
        const reviewLabel = translate(locale, "d1.topbar.reviewChanges", {});
        // The label states the destination, never just "changes": a control
        // whose tooltip does not name where it goes teaches the wrong map.
        marker.title = options.reviewAvailable
          ? reviewLabel
          : translate(locale, "d1.review.entryUnavailable", {
              capability: "runtime.structured_diff",
            });
        marker.setAttribute("aria-label", reviewLabel);
        marker.setAttribute("aria-expanded", String(options.reviewOpen === true));
        // Visible and disabled, never hidden: an absent Core capability is a
        // fact the operator should be able to read off the chrome.
        marker.disabled = options.reviewAvailable !== true;
        marker.addEventListener("click", () => options.onOpenReview?.());
        reviewEntry = marker;
      } else {
        const marker = document.createElement("span");
        marker.className = "d1-topbar-dirty";
        marker.dataset.topbarDirty = "true";
        marker.textContent = "●";
        marker.title = translate(locale, "d1.topbar.dirty", {});
        project.append(marker);
      }
    }
  }
  if (pickerAvailable) {
    const chevron = document.createElement("span");
    chevron.className = "chev";
    chevron.setAttribute("aria-hidden", "true");
    chevron.textContent = "▾";
    project.append(chevron);
  }

  const lane = projection.lanes.find((candidate) => candidate.id === projection.selectedLaneId);
  const laneSummary = document.createElement("span");
  laneSummary.className = "d1-topbar-lane";
  laneSummary.dataset.tauriDragRegion = "true";
  laneSummary.textContent = lane
    ? `${lane.id} · ${lane.summary}${lane.branch ? ` · ${lane.branch}` : ""}`
    : translate(locale, "d1.lanes", {});

  const contextDrawerToggle = document.createElement("button");
  contextDrawerToggle.type = "button";
  contextDrawerToggle.className = "tbtbtn d1-context-drawer-toggle";
  contextDrawerToggle.dataset.contextDrawerToggle = "true";
  contextDrawerToggle.setAttribute("aria-controls", "d1-context-dock");
  contextDrawerToggle.setAttribute("aria-expanded", "false");
  contextDrawerToggle.setAttribute(
    "aria-label",
    `${translate(locale, "d1.environment", {})} dock`,
  );
  contextDrawerToggle.hidden = showWelcome;
  contextDrawerToggle.append(createCanonicalGuiIcon("panel"));

  // The design puts the palette button first in `.tbtools`, before the
  // context-panel toggle.
  const commandPaletteToggle = document.createElement("button");
  commandPaletteToggle.type = "button";
  commandPaletteToggle.className = "tbtbtn d1-command-palette-toggle";
  commandPaletteToggle.dataset.commandPaletteToggle = "true";
  commandPaletteToggle.setAttribute("aria-haspopup", "dialog");
  commandPaletteToggle.setAttribute("aria-expanded", String(options.commandPaletteOpen === true));
  const paletteLabel = `${translate(locale, "d1.palette.title", {})} ${formatShortcut("⌘K")}`;
  commandPaletteToggle.title = paletteLabel;
  commandPaletteToggle.setAttribute("aria-label", paletteLabel);
  commandPaletteToggle.disabled = !options.onToggleCommandPalette;
  if (options.commandPaletteOpen) commandPaletteToggle.classList.add("on");
  if (options.onToggleCommandPalette) {
    commandPaletteToggle.addEventListener("click", () => options.onToggleCommandPalette?.());
  }
  commandPaletteToggle.append(createCanonicalGuiIcon("palette"));

  const tools = document.createElement("span");
  tools.className = "tbtools";
  tools.append(commandPaletteToggle, contextDrawerToggle);
  if (!nativeShell) titlebar.append(lights);
  titlebar.append(brand, project);
  if (reviewEntry) titlebar.append(reviewEntry);
  if (source) titlebar.append(renderGitOps(source, locale, onNavigate));
  titlebar.append(laneSummary, tools);
  return { element: titlebar, contextDrawerToggle, commandPaletteToggle };
}
