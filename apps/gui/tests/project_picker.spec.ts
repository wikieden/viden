// @vitest-environment jsdom

// The project picker is a client of two facts and nothing else: Core's bounded
// recent-work inventory, and the single workspace `LocalCoreHost` supervises.
// Because a successful `open_workspace` *replaces* that workspace — dropping
// the supervisor, joining its worker, and shutting down every resident ACP
// session — every switch goes through an inline confirmation naming the work it
// tears down. Nothing here scans the session home, invents a second project
// group, or enables a row Core cannot serve.
import { beforeEach, describe, expect, test, vi } from "vitest";

import {
  MULTI_WORKSPACE_CODE,
  renderProjectPicker,
  type ProjectPickerHandlers,
  type ProjectPickerModel,
} from "../src/components/project_picker";
import type { RecentProjectView } from "../src/models/recent_work";

const NOW = Date.UTC(2026, 0, 1) ;

const RECENT: RecentProjectView[] = [
  {
    canonicalRoot: "/workspace/spatial-lm",
    displayName: "spatial-lm",
    lastUpdatedAt: Math.floor(NOW / 1000) - 2 * 24 * 60 * 60,
    latestSessionId: "session-spatial",
  },
  {
    // Core's own root also appears in the inventory; the picker must not offer
    // a "switch" to the project that is already open.
    canonicalRoot: "/workspace/viden",
    displayName: "viden",
    lastUpdatedAt: Math.floor(NOW / 1000) - 60 * 60,
    latestSessionId: "session-viden",
  },
];

function model(overrides: Partial<ProjectPickerModel> = {}): ProjectPickerModel {
  return {
    locale: "en",
    current: {
      displayName: "viden",
      canonicalRoot: "/workspace/viden",
      activeLaneCount: 2,
      activeSessionCount: 1,
      laneCount: 3,
    },
    recent: { kind: "loaded", projects: RECENT, diagnostics: [] },
    now: NOW,
    ...overrides,
  };
}

function mount(
  modelOverrides: Partial<ProjectPickerModel> = {},
  handlerOverrides: Partial<ProjectPickerHandlers> = {},
) {
  document.body.innerHTML = '<main id="app"><button data-project-selector>viden</button></main>';
  const anchor = document.querySelector<HTMLElement>("[data-project-selector]")!;
  const onPickDirectory = vi.fn(async () => "/workspace/next" as string | null);
  const onSwitchWorkspace = vi.fn(async () => {});
  const onClose = vi.fn();
  const controller = renderProjectPicker(anchor, "titlebar", model(modelOverrides), {
    onPickDirectory,
    onSwitchWorkspace,
    onClose,
    ...handlerOverrides,
  });
  return { anchor, controller, onPickDirectory, onSwitchWorkspace, onClose };
}

function panel(): HTMLElement {
  return document.querySelector<HTMLElement>("[data-project-picker]")!;
}

describe("project picker", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  // `0.3.4` moved D11 here from the New Lane popover's "Full setup…": D11
  // configures a project, and this is the project surface.
  test("offers project configuration beside the open project, only with a handler", () => {
    const onConfigureProject = vi.fn();
    const { controller } = mount({}, { onConfigureProject });

    const row = panel().querySelector<HTMLButtonElement>("[data-picker-configure]")!;
    expect(row).not.toBeNull();
    expect(row.textContent).toContain("Configure this project…");
    row.click();
    expect(onConfigureProject).toHaveBeenCalledTimes(1);
    // It replaces the window, so the popover does not outlive it.
    expect(document.querySelector("[data-project-picker]")).toBeNull();
    controller.close();

    // No handler, no row: a control that opens nothing is worse than absence.
    const bare = mount();
    expect(panel().querySelector("[data-picker-configure]")).toBeNull();
    bare.controller.close();
  });

  test("renders the design's three columns against Core facts", () => {
    const { controller } = mount();

    // Add: one real action plus the two Core cannot serve.
    expect(panel().querySelector('[data-picker-add="directory"]')).not.toBeNull();
    for (const action of ["clone", "empty"]) {
      const row = panel().querySelector<HTMLButtonElement>(`[data-picker-add="${action}"]`)!;
      expect(row.disabled).toBe(true);
      expect(row.dataset.pickerUnavailableCode).toBe(MULTI_WORKSPACE_CODE);
      expect(row.textContent).toContain(MULTI_WORKSPACE_CODE);
    }

    // In workspace: exactly one row, the open project, and it is not a button.
    const inWorkspace = panel().querySelectorAll("[data-picker-current]");
    expect(inWorkspace).toHaveLength(1);
    const current = inWorkspace[0] as HTMLElement;
    expect(current.tagName).not.toBe("BUTTON");
    expect(current.getAttribute("aria-current")).toBe("true");
    expect(current.textContent).toContain("viden");
    expect(current.textContent).toContain("/workspace/viden");
    expect(current.textContent).toContain("3 lanes");

    // Recent: Core's rows, minus the project already open.
    const recent = Array.from(panel().querySelectorAll<HTMLElement>("[data-picker-recent]"));
    expect(recent.map((row) => row.dataset.pickerRecent)).toEqual(["/workspace/spatial-lm"]);
    expect(recent[0]?.textContent).toContain("2d ago");

    // No fabricated "Global" bucket and no invented sibling projects.
    expect(panel().textContent).not.toContain("Global");
    controller.close();
  });

  test("an absent capability and a Core rejection stay distinct from an empty inventory", () => {
    const unavailable = mount({
      recent: { kind: "unavailable", reason: "Core has not published runtime.recent_work." },
    });
    expect(
      panel().querySelector<HTMLElement>('[data-picker-recent-state="unavailable"]')?.textContent,
    ).toContain("runtime.recent_work");
    unavailable.controller.close();

    const failed = mount({
      recent: { kind: "failed", reason: "recent work inventory is unavailable" },
    });
    expect(
      panel().querySelector<HTMLElement>('[data-picker-recent-state="failed"]')?.textContent,
    ).toBe("recent work inventory is unavailable");
    failed.controller.close();

    const empty = mount({ recent: { kind: "loaded", projects: [], diagnostics: [] } });
    expect(panel().querySelector('[data-picker-recent-state="empty"]')).not.toBeNull();
    empty.controller.close();
  });

  test("Core's inventory diagnostics render verbatim", () => {
    const { controller } = mount({
      recent: { kind: "loaded", projects: RECENT, diagnostics: ["recent.index_stale"] },
    });
    expect(
      panel().querySelector('[data-picker-recent-diagnostic="recent.index_stale"]')?.textContent,
    ).toBe("recent.index_stale");
    controller.close();
  });

  test("a recent row confirms the teardown before any workspace is opened", async () => {
    const { onSwitchWorkspace, controller } = mount();

    panel().querySelector<HTMLButtonElement>('[data-picker-recent="/workspace/spatial-lm"]')!
      .click();

    const confirm = panel().querySelector<HTMLElement>("[data-picker-confirm]")!;
    expect(confirm.querySelector("[data-picker-confirm-target]")?.textContent).toBe(
      "/workspace/spatial-lm",
    );
    // The replacement, its cause, and the exact running work are all named.
    expect(confirm.textContent).toContain(MULTI_WORKSPACE_CODE);
    const impact = confirm.querySelector<HTMLElement>("[data-picker-confirm-impact]")!;
    expect(impact.dataset.pickerConfirmImpact).toBe("running");
    // Counted from the current projection, never guessed: two Lanes are in
    // flight and one Agent session is live.
    expect(impact.textContent).toContain("Lanes: 2");
    expect(impact.textContent).toContain("Agent sessions: 1");
    expect(impact.textContent).toContain("viden");
    // Nothing has been opened yet.
    expect(onSwitchWorkspace).not.toHaveBeenCalled();

    confirm.querySelector<HTMLButtonElement>("[data-picker-confirm-accept]")!.click();
    await Promise.resolve();
    expect(onSwitchWorkspace).toHaveBeenCalledWith("/workspace/spatial-lm");
    controller.close();
  });

  test("an idle workspace still confirms, with the milder sentence", () => {
    const { onSwitchWorkspace, controller } = mount({
      current: {
        displayName: "viden",
        canonicalRoot: "/workspace/viden",
        activeLaneCount: 0,
        activeSessionCount: 0,
        laneCount: 1,
      },
    });

    panel().querySelector<HTMLButtonElement>('[data-picker-recent="/workspace/spatial-lm"]')!
      .click();

    const impact = panel().querySelector<HTMLElement>("[data-picker-confirm-impact]")!;
    expect(impact.dataset.pickerConfirmImpact).toBe("idle");
    expect(impact.textContent).toContain("Nothing is running");
    expect(onSwitchWorkspace).not.toHaveBeenCalled();
    controller.close();
  });

  test("cancel and Escape both back out of the confirmation without opening anything", () => {
    const { onSwitchWorkspace, onClose, controller } = mount();

    panel().querySelector<HTMLButtonElement>('[data-picker-recent="/workspace/spatial-lm"]')!
      .click();
    panel().querySelector<HTMLButtonElement>("[data-picker-confirm-cancel]")!.click();
    expect(panel().querySelector("[data-picker-confirm]")).toBeNull();
    expect(panel().querySelector('[data-picker-add="directory"]')).not.toBeNull();

    panel().querySelector<HTMLButtonElement>('[data-picker-recent="/workspace/spatial-lm"]')!
      .click();
    panel().dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    // Escape inside the confirmation backs out; it does not also close the
    // picker, so one keystroke never does two things.
    expect(panel().querySelector("[data-picker-confirm]")).toBeNull();
    expect(document.querySelector("[data-project-picker]")).not.toBeNull();
    expect(onClose).not.toHaveBeenCalled();
    expect(onSwitchWorkspace).not.toHaveBeenCalled();
    controller.close();
  });

  test("Add directory routes the chosen folder through the same confirmation", async () => {
    const { onPickDirectory, onSwitchWorkspace } = mount();

    panel().querySelector<HTMLButtonElement>('[data-picker-add="directory"]')!.click();
    await Promise.resolve();
    await Promise.resolve();

    expect(onPickDirectory).toHaveBeenCalledTimes(1);
    expect(onSwitchWorkspace).not.toHaveBeenCalled();
    expect(
      panel().querySelector("[data-picker-confirm-target]")?.textContent,
    ).toBe("/workspace/next");
  });

  test("cancelling the folder chooser leaves the picker exactly as it was", async () => {
    const { onSwitchWorkspace } = mount({}, { onPickDirectory: vi.fn(async () => null) });

    panel().querySelector<HTMLButtonElement>('[data-picker-add="directory"]')!.click();
    await Promise.resolve();
    await Promise.resolve();

    expect(panel().querySelector("[data-picker-confirm]")).toBeNull();
    const directory = panel().querySelector<HTMLButtonElement>('[data-picker-add="directory"]')!;
    expect(directory.disabled).toBe(false);
    expect(onSwitchWorkspace).not.toHaveBeenCalled();
  });

  test("a rejected switch renders the host's own words and keeps the picker open", async () => {
    mount(
      {},
      { onSwitchWorkspace: vi.fn(async () => Promise.reject(new Error("not a Git workspace"))) },
    );

    panel().querySelector<HTMLButtonElement>('[data-picker-recent="/workspace/spatial-lm"]')!
      .click();
    panel().querySelector<HTMLButtonElement>("[data-picker-confirm-accept]")!.click();
    for (let hop = 0; hop < 6; hop += 1) await Promise.resolve();

    expect(panel().querySelector("[data-picker-error]")?.textContent).toContain(
      "not a Git workspace",
    );
    expect(panel().querySelector<HTMLButtonElement>("[data-picker-confirm-accept]")?.disabled).toBe(
      false,
    );
  });

  test("Escape at the columns closes the popover and hands focus back to the anchor", () => {
    const { anchor, onClose } = mount();

    expect(anchor.getAttribute("aria-expanded")).toBe("true");
    panel().dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));

    expect(document.querySelector("[data-project-picker]")).toBeNull();
    expect(anchor.getAttribute("aria-expanded")).toBe("false");
    expect(document.activeElement).toBe(anchor);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test("an outside click closes the popover, and a click on the anchor does not", () => {
    const { anchor, onClose } = mount();

    anchor.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    expect(document.querySelector("[data-project-picker]")).not.toBeNull();

    document.body.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    expect(document.querySelector("[data-project-picker]")).toBeNull();
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test("focus returns to the live anchor after the cockpit rebuilt the titlebar", () => {
    const { anchor, controller } = mount();

    // The cockpit replaces the titlebar on every Core refresh; the node this
    // popover was anchored to is then detached.
    anchor.remove();
    const replacement = document.createElement("button");
    replacement.dataset.projectSelector = "true";
    document.querySelector("#app")!.append(replacement);

    controller.close();
    expect(document.activeElement).toBe(replacement);
    expect(replacement.getAttribute("aria-expanded")).toBe("false");
  });
});
