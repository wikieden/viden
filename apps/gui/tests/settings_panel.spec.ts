// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { renderSettingsPanel } from "../src/components/settings_panel";
import { renderD1Cockpit } from "../src/screens/d1_cockpit";
import {
  createPreferenceState,
  type PreferenceIntentOutcome,
  type PreferenceState,
} from "../src/preferences";
import { D1_PROJECTION } from "./support/d1_projection";

const RESOLVED = {
  locale: "en" as const,
  skin: "aurora" as const,
  mode: "dark" as const,
  density: "regular" as const,
  motion: "system" as const,
  diagnostics: [],
};

function anchorButton(): HTMLButtonElement {
  const anchor = document.createElement("button");
  anchor.type = "button";
  anchor.dataset.settingsToggle = "true";
  document.body.append(anchor);
  return anchor;
}

function handlers() {
  return {
    onDraft: vi.fn(),
    onSave: vi.fn(),
    onCancel: vi.fn(),
    onRestore: vi.fn(),
    onClose: vi.fn(),
    onControl: vi.fn(),
  };
}

function option(panel: HTMLElement, key: string): HTMLButtonElement {
  const found = panel.querySelector<HTMLButtonElement>(`[data-settings-option="${key}"]`);
  if (!found) throw new Error(`missing settings option ${key}`);
  return found;
}

/**
 * Removes every panel this file mounted, listeners included.
 *
 * A panel registers a `document` mousedown listener for its outside-click
 * dismissal, so one left mounted keeps reacting during the next test. Clearing
 * `document.body` detaches the DOM but not the listener, which is why the
 * teardown closes controllers instead of wiping markup.
 */
function closeMountedPanels(): void {
  for (const panel of document.querySelectorAll<HTMLElement>("[data-settings-panel]")) {
    panel.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    panel.remove();
  }
}

describe("settings panel", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });
  afterEach(closeMountedPanels);

  test("renders every Core preference axis with the resolved value selected", () => {
    const spies = handlers();
    const controller = renderSettingsPanel(
      anchorButton(),
      {
        locale: "en",
        state: createPreferenceState(RESOLVED),
        available: true,
        saving: false,
        outcome: null,
        controls: null,
      },
      spies,
    );

    const panel = controller.root;
    for (const field of ["locale", "skin", "mode", "density", "motion"]) {
      const group = panel.querySelector(`[data-settings-field="${field}"]`);
      expect(group?.getAttribute("role")).toBe("radiogroup");
    }
    // `system` is offered as a real Core value for the axes Core resolves.
    expect(option(panel, "locale:system")).toBeTruthy();
    expect(option(panel, "mode:system")).toBeTruthy();
    expect(option(panel, "skin:aurora").getAttribute("aria-checked")).toBe("true");
    expect(option(panel, "mode:dark").getAttribute("aria-checked")).toBe("true");
    // Nothing is drafted yet, so there is nothing to save or cancel.
    expect(panel.querySelector<HTMLButtonElement>("[data-settings-save]")?.disabled).toBe(true);
    expect(panel.querySelector<HTMLButtonElement>("[data-settings-cancel]")?.disabled).toBe(true);
    // Restore never needs a draft: it removes whatever Core stored.
    expect(panel.querySelector<HTMLButtonElement>("[data-settings-restore]")?.disabled).toBe(
      false,
    );
  });

  test("a dark-only skin stays selectable so Core owns the pair rule", () => {
    const spies = handlers();
    const controller = renderSettingsPanel(
      anchorButton(),
      {
        locale: "en",
        state: createPreferenceState(RESOLVED),
        available: true,
        saving: false,
        outcome: null,
        controls: null,
      },
      spies,
    );

    const amber = option(controller.root, "skin:amber");
    expect(amber.disabled).toBe(false);
    amber.click();
    option(controller.root, "mode:light").click();
    expect(spies.onDraft).toHaveBeenNthCalledWith(1, { skin: "amber" });
    expect(spies.onDraft).toHaveBeenNthCalledWith(2, { mode: "light" });
  });

  test("a drafted axis enables save and shows the unsaved choice", () => {
    const spies = handlers();
    const state = { resolved: RESOLVED, draft: { skin: "ice" as const }, dirty: true };
    const controller = renderSettingsPanel(
      anchorButton(),
      { locale: "en", state, available: true, saving: false, outcome: null, controls: null },
      spies,
    );

    expect(option(controller.root, "skin:ice").getAttribute("aria-checked")).toBe("true");
    expect(option(controller.root, "skin:aurora").getAttribute("aria-checked")).toBe("false");
    const save = controller.root.querySelector<HTMLButtonElement>("[data-settings-save]");
    expect(save?.disabled).toBe(false);
    save?.click();
    expect(spies.onSave).toHaveBeenCalledOnce();
  });

  test("a Core rejection renders as an alert with Core's own reason", () => {
    const outcome: PreferenceIntentOutcome = {
      status: "rejected",
      reason: "amber has no light mode",
      diagnostics: [
        { code: "ui.invalid_skin_mode_pair", key: "ui.preference", field: "mode", rejectedValue: "light" },
      ],
    };
    const controller = renderSettingsPanel(
      anchorButton(),
      {
        locale: "en",
        state: { resolved: RESOLVED, draft: { skin: "amber" }, dirty: true },
        available: true,
        saving: false,
        outcome,
        controls: null,
      },
      handlers(),
    );

    const alert = controller.root.querySelector("[data-settings-alert]");
    expect(alert?.getAttribute("role")).toBe("alert");
    expect(alert?.textContent).toBe("amber has no light mode");
    expect(
      controller.root.querySelector('[data-settings-diagnostic="ui.invalid_skin_mode_pair"]'),
    ).not.toBeNull();
    // The rejected draft is still what the operator sees; nothing was applied.
    expect(option(controller.root, "skin:amber").getAttribute("aria-checked")).toBe("true");
  });

  test("a confirmed restore reports the fallback rather than a saved table", () => {
    const controller = renderSettingsPanel(
      anchorButton(),
      {
        locale: "en",
        state: createPreferenceState(RESOLVED),
        available: true,
        saving: false,
        outcome: { status: "confirmed", resolved: RESOLVED, persisted: false, diagnostics: [] },
        controls: null,
      },
      handlers(),
    );

    expect(
      controller.root.querySelector("[data-settings-status]")?.getAttribute("data-settings-status"),
    ).toBe("restored");
  });

  test("an absent capability renders a read-only panel naming the real capability", () => {
    const controller = renderSettingsPanel(
      anchorButton(),
      {
        locale: "en",
        state: { resolved: RESOLVED, draft: { density: "comfy" }, dirty: true },
        available: false,
        saving: false,
        outcome: null,
        controls: null,
      },
      handlers(),
    );

    const panel = controller.root;
    expect(panel.dataset.settingsAvailable).toBe("false");
    const notice = panel.querySelector("[data-settings-unavailable]");
    expect(notice?.textContent).toContain("ui.preference_persistence");
    // The entry is never hidden and never enabled-and-inert.
    expect(panel.querySelector<HTMLButtonElement>("[data-settings-save]")?.disabled).toBe(true);
    expect(panel.querySelector<HTMLButtonElement>("[data-settings-restore]")?.disabled).toBe(true);
    expect(option(panel, "skin:ice").disabled).toBe(true);
  });

  test("saving marks the panel busy and disables every control", () => {
    const controller = renderSettingsPanel(
      anchorButton(),
      {
        locale: "en",
        state: { resolved: RESOLVED, draft: { skin: "ice" }, dirty: true },
        available: true,
        saving: true,
        outcome: null,
        controls: null,
      },
      handlers(),
    );

    expect(controller.root.getAttribute("aria-busy")).toBe("true");
    expect(controller.root.querySelector<HTMLButtonElement>("[data-settings-save]")?.disabled).toBe(
      true,
    );
    expect(option(controller.root, "skin:mono").disabled).toBe(true);
  });

  test("Escape closes the panel and returns focus to the gear", () => {
    const anchor = anchorButton();
    const spies = handlers();
    const controller = renderSettingsPanel(
      anchor,
      {
        locale: "en",
        state: createPreferenceState(RESOLVED),
        available: true,
        saving: false,
        outcome: null,
        controls: null,
      },
      spies,
    );

    expect(anchor.getAttribute("aria-expanded")).toBe("true");
    controller.root.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
    );

    expect(document.querySelector("[data-settings-panel]")).toBeNull();
    expect(anchor.getAttribute("aria-expanded")).toBe("false");
    expect(document.activeElement).toBe(anchor);
    expect(spies.onClose).toHaveBeenCalledOnce();
  });

  test("an outside click closes the panel", () => {
    const spies = handlers();
    renderSettingsPanel(
      anchorButton(),
      {
        locale: "en",
        state: createPreferenceState(RESOLVED),
        available: true,
        saving: false,
        outcome: null,
        controls: null,
      },
      spies,
    );

    document.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));

    expect(document.querySelector("[data-settings-panel]")).toBeNull();
    expect(spies.onClose).toHaveBeenCalledOnce();
  });

  test("localizes its copy in Chinese without a private catalog", () => {
    const controller = renderSettingsPanel(
      anchorButton(),
      {
        locale: "zh-CN",
        state: createPreferenceState(RESOLVED),
        available: true,
        saving: false,
        outcome: null,
        controls: null,
      },
      handlers(),
    );

    expect(controller.root.getAttribute("aria-label")).toBe("设置");
    expect(option(controller.root, "mode:dark").textContent).toContain("深色");
  });
});

describe("cockpit settings entry", () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="app"></main>';
  });
  afterEach(closeMountedPanels);

  function cockpitRoot(): HTMLElement {
    const root = document.querySelector<HTMLElement>("#app");
    if (!root) throw new Error("test root is missing");
    return root;
  }

  const idleResult = {
    projection: D1_PROJECTION,
    pendingCommandId: null,
    outcome: { state: "idle" as const, reason: null },
  };

  test("the rail gear is disabled while no host preference port is bound", () => {
    const root = cockpitRoot();
    renderD1Cockpit(root, D1_PROJECTION, async () => idleResult, async () => idleResult, undefined, undefined, {
      poll: false,
    });

    const gear = root.querySelector<HTMLButtonElement>("[data-settings-toggle]");
    // Present, never hidden — but not enabled-and-inert.
    expect(gear).not.toBeNull();
    expect(gear?.disabled).toBe(true);
  });

  test("the gear opens the panel and a confirmed save becomes rendered authority", async () => {
    const root = cockpitRoot();
    const save = vi.fn(
      async (_state: PreferenceState): Promise<PreferenceIntentOutcome> => ({
        status: "confirmed",
        resolved: { ...RESOLVED, skin: "ice", density: "comfy" },
        persisted: true,
        diagnostics: [],
      }),
    );
    const controller = renderD1Cockpit(
      root,
      D1_PROJECTION,
      async () => idleResult,
      async () => idleResult,
      undefined,
      undefined,
      {
        poll: false,
        preferences: {
          isAvailable: async () => true,
          save,
          restore: async () => ({
            status: "confirmed",
            resolved: RESOLVED,
            persisted: false,
            diagnostics: [],
          }),
        },
      },
    );

    root.querySelector<HTMLButtonElement>("[data-settings-toggle]")?.click();
    await vi.waitFor(() => {
      expect(document.querySelector("[data-settings-panel]")).not.toBeNull();
    });

    const panel = document.querySelector<HTMLElement>("[data-settings-panel]");
    if (!panel) throw new Error("settings panel is missing");
    option(panel, "skin:ice").click();
    await vi.waitFor(() => {
      const next = document.querySelector<HTMLElement>("[data-settings-panel]");
      expect(next?.querySelector<HTMLButtonElement>("[data-settings-save]")?.disabled).toBe(false);
    });

    document
      .querySelector<HTMLElement>("[data-settings-panel]")
      ?.querySelector<HTMLButtonElement>("[data-settings-save]")
      ?.click();

    await vi.waitFor(() => {
      expect(save).toHaveBeenCalledOnce();
      const next = document.querySelector<HTMLElement>("[data-settings-panel]");
      // Core's resolution replaced the draft: Save is spent, and the axis
      // Core coupled in (`density`) shows even though the click never asked.
      expect(next?.querySelector<HTMLButtonElement>("[data-settings-save]")?.disabled).toBe(true);
      expect(
        next?.querySelector<HTMLButtonElement>('[data-settings-option="density:comfy"]')
          ?.getAttribute("aria-checked"),
      ).toBe("true");
      expect(next?.querySelector("[data-settings-status]")).not.toBeNull();
    });
    // The draft state passed to Core carried only the axis the operator chose.
    expect(save.mock.calls[0]?.[0]).toMatchObject({ draft: { skin: "ice" }, dirty: true });
    controller.dispose();
  });

  test("an absent capability opens the panel read-only instead of hiding it", async () => {
    const root = cockpitRoot();
    const controller = renderD1Cockpit(
      root,
      D1_PROJECTION,
      async () => idleResult,
      async () => idleResult,
      undefined,
      undefined,
      {
        poll: false,
        preferences: {
          isAvailable: async () => false,
          save: async () => ({
            status: "unavailable",
            capability: "ui.preference_persistence",
            diagnostic: {
              code: "gui.core_contract_unavailable",
              key: "ui.preference_persistence",
              field: "capability",
              rejectedValue: "ui.preference_persistence",
            },
          }),
          restore: async () => ({
            status: "unavailable",
            capability: "ui.preference_persistence",
            diagnostic: {
              code: "gui.core_contract_unavailable",
              key: "ui.preference_persistence",
              field: "capability",
              rejectedValue: "ui.preference_persistence",
            },
          }),
        },
      },
    );

    const gear = root.querySelector<HTMLButtonElement>("[data-settings-toggle]");
    expect(gear?.disabled).toBe(false);
    gear?.click();

    await vi.waitFor(() => {
      const panel = document.querySelector<HTMLElement>("[data-settings-panel]");
      expect(panel?.dataset.settingsAvailable).toBe("false");
      expect(panel?.querySelector("[data-settings-unavailable]")?.textContent).toContain(
        "ui.preference_persistence",
      );
    });
    controller.dispose();
  });

  test("the gear closes its own panel after a Core-driven rail rebuild", async () => {
    const root = cockpitRoot();
    const confirmedRestore: PreferenceIntentOutcome = {
      status: "confirmed",
      resolved: RESOLVED,
      persisted: false,
      diagnostics: [],
    };
    const controller = renderD1Cockpit(
      root,
      D1_PROJECTION,
      async () => idleResult,
      async () => idleResult,
      undefined,
      undefined,
      {
        poll: false,
        preferences: {
          isAvailable: async () => true,
          save: async () => confirmedRestore,
          restore: async () => confirmedRestore,
        },
      },
    );

    root.querySelector<HTMLButtonElement>("[data-settings-toggle]")?.click();
    await vi.waitFor(() => {
      expect(document.querySelector("[data-settings-panel]")).not.toBeNull();
    });

    // A Core refresh rebuilds the rail, so the gear the panel was anchored to
    // may no longer be the mounted one. The toggle must still close, not
    // close-then-reopen, and the mounted gear must still report its state.
    controller.applyProjection({ ...D1_PROJECTION, selectedLaneId: null });
    const gear = root.querySelector<HTMLButtonElement>("[data-settings-toggle]");
    expect(gear?.getAttribute("aria-expanded")).toBe("true");

    gear?.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    gear?.click();

    expect(document.querySelector("[data-settings-panel]")).toBeNull();
    expect(
      root.querySelector<HTMLButtonElement>("[data-settings-toggle]")?.getAttribute("aria-expanded"),
    ).toBe("false");
    controller.dispose();
  });

  test("a rejected restore keeps Core's reason visible and changes nothing", async () => {
    const root = cockpitRoot();
    const controller = renderD1Cockpit(
      root,
      D1_PROJECTION,
      async () => idleResult,
      async () => idleResult,
      undefined,
      undefined,
      {
        poll: false,
        preferences: {
          isAvailable: async () => true,
          save: async () => ({
            status: "rejected",
            reason: "unused",
            diagnostics: [],
          }),
          restore: async () => ({
            status: "rejected",
            reason: "plan mode denies ui_preferences_reset",
            diagnostics: [],
          }),
        },
      },
    );

    root.querySelector<HTMLButtonElement>("[data-settings-toggle]")?.click();
    await vi.waitFor(() => {
      expect(document.querySelector("[data-settings-panel]")).not.toBeNull();
    });
    document
      .querySelector<HTMLElement>("[data-settings-panel]")
      ?.querySelector<HTMLButtonElement>("[data-settings-restore]")
      ?.click();

    await vi.waitFor(() => {
      const alert = document.querySelector("[data-settings-alert]");
      expect(alert?.getAttribute("role")).toBe("alert");
      expect(alert?.textContent).toBe("plan mode denies ui_preferences_reset");
    });
    controller.dispose();
  });
});
