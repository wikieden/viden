// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { PERMISSION_LEVELS } from "../src/components/composer_controls";
import { renderSettingsPanel } from "../src/components/settings_panel";
import { renderD1Cockpit } from "../src/screens/d1_cockpit";
import { createPreferenceState } from "../src/preferences";
import type { ComposerControlIntent } from "../src/models/composer";
import { D1_PROJECTION } from "./support/d1_projection";

/**
 * The Settings overlay's two Core-command sections: Permissions (design
 * section 2) and Provider & Models (design section 1).
 *
 * Both are second surfaces onto commands the composer pills already own, so
 * every assertion here is about *sharing* one wire path — the same permission
 * enumeration, the same `modelGroups` source, the same intents — rather than
 * about a settings-private model.
 */

const RESOLVED = {
  locale: "en" as const,
  skin: "aurora" as const,
  mode: "dark" as const,
  density: "regular" as const,
  motion: "system" as const,
  diagnostics: [],
};

const CONTROLS = {
  permissionLevel: "ask",
  providerId: "deepseek",
  model: "deepseek-v4-flash",
  groups: [
    { providerId: "deepseek", label: "deepseek", models: ["deepseek-v4-flash"] },
    { providerId: "codex-acp", label: "Codex", models: ["gpt-5-codex", "o4-mini"] },
  ],
  cwd: "/workspace/viden",
  enabled: true,
  busy: false,
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

function mount(
  overrides: Partial<typeof CONTROLS> | null = {},
  spies = handlers(),
): { panel: HTMLElement; spies: ReturnType<typeof handlers>; close: () => void } {
  const controller = renderSettingsPanel(
    anchorButton(),
    {
      locale: "en",
      state: createPreferenceState(RESOLVED),
      available: true,
      saving: false,
      outcome: null,
      controls: overrides === null ? null : { ...CONTROLS, ...overrides },
    },
    spies,
  );
  return { panel: controller.root, spies, close: () => controller.close() };
}

function option(panel: HTMLElement, key: string): HTMLButtonElement {
  const found = panel.querySelector<HTMLButtonElement>(`[data-settings-option="${key}"]`);
  if (!found) throw new Error(`missing settings option ${key}`);
  return found;
}

function closeMountedPanels(): void {
  for (const panel of document.querySelectorAll<HTMLElement>("[data-settings-panel]")) {
    panel.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    panel.remove();
  }
}

describe("settings permissions section", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });
  afterEach(closeMountedPanels);

  test("renders exactly the permission levels the composer pill enumerates", () => {
    const { panel } = mount();
    const group = panel.querySelector<HTMLElement>('[data-settings-field="permission"]')!;

    expect(group.getAttribute("role")).toBe("radiogroup");
    expect(
      Array.from(
        group.querySelectorAll<HTMLElement>("[data-settings-option]"),
        (row) => row.dataset.settingsOption,
      ),
    ).toEqual(PERMISSION_LEVELS.map((level) => `permission:${level}`));
  });

  test("shows the Core identifier and a description beside each UI label", () => {
    const { panel } = mount();
    const ask = option(panel, "permission:ask");

    expect(ask.querySelector(".gset-pm-name")?.textContent).toBe("Ask");
    // The chip is the Core CLI name, not a reworded client label: an operator
    // reading a Core log must be able to match the two.
    expect(ask.querySelector(".gset-pm-cli")?.textContent).toBe("ask");
    expect(ask.querySelector(".gset-pm-detail")?.textContent).toBeTruthy();
  });

  test("marks the level Core published and dispatches the composer pill's intent", () => {
    const { panel, spies } = mount();

    expect(option(panel, "permission:ask").getAttribute("aria-checked")).toBe("true");
    expect(option(panel, "permission:auto").getAttribute("aria-checked")).toBe("false");

    option(panel, "permission:auto").click();

    expect(spies.onControl).toHaveBeenCalledWith({
      type: "set_permission_level",
      level: "auto",
    } satisfies ComposerControlIntent);
  });

  test("disables rather than hides the levels while Core cannot accept a control", () => {
    const { panel, spies } = mount({ enabled: false });

    const rows = panel.querySelectorAll<HTMLButtonElement>(
      '[data-settings-field="permission"] [data-settings-option]',
    );
    expect(rows).toHaveLength(PERMISSION_LEVELS.length);
    for (const row of rows) expect(row.disabled).toBe(true);

    rows[0]!.click();
    expect(spies.onControl).not.toHaveBeenCalled();
  });

  test("disables the levels while a control command is in flight", () => {
    const { panel } = mount({ busy: true });

    expect(option(panel, "permission:ask").disabled).toBe(true);
  });

  test("shows the workspace root Core published as a read-only row", () => {
    const { panel } = mount();
    const row = panel.querySelector<HTMLElement>("[data-settings-cwd]")!;

    expect(row.textContent).toContain("/workspace/viden");
    // Core publishes no command for editing the working-directory scope, so
    // the row must not offer one.
    expect(row.querySelector("button, input, textarea, select")).toBeNull();
  });

  test("draws no rules preview, because Core publishes no rule facts", () => {
    const { panel } = mount();

    expect(panel.querySelector("[data-settings-rules]")).toBeNull();
  });

  test("keeps the section visible and inert when the cockpit bound no dispatcher", () => {
    const { panel, spies } = mount(null);

    const rows = panel.querySelectorAll<HTMLButtonElement>(
      '[data-settings-field="permission"] [data-settings-option]',
    );
    expect(rows).toHaveLength(PERMISSION_LEVELS.length);
    for (const row of rows) expect(row.disabled).toBe(true);
    rows[0]!.click();
    expect(spies.onControl).not.toHaveBeenCalled();
  });

  test("stays operable while the preference capability is absent", () => {
    // The two capabilities are unrelated: `ui.preference_persistence` gates
    // the appearance draft, not `SetPermissionLevel`.
    const spies = handlers();
    const controller = renderSettingsPanel(
      anchorButton(),
      {
        locale: "en",
        state: createPreferenceState(RESOLVED),
        available: false,
        saving: false,
        outcome: null,
        controls: CONTROLS,
      },
      spies,
    );

    expect(option(controller.root, "permission:auto").disabled).toBe(false);
    controller.close();
  });
});

describe("settings provider and models section", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });
  afterEach(closeMountedPanels);

  test("lists every group and model the composer model pill would list", () => {
    const { panel } = mount();
    const group = panel.querySelector<HTMLElement>('[data-settings-field="model"]')!;

    expect(group.getAttribute("role")).toBe("radiogroup");
    expect(
      Array.from(
        group.querySelectorAll<HTMLElement>("[data-settings-option]"),
        (row) => row.dataset.settingsOption,
      ),
    ).toEqual([
      "model:deepseek:deepseek-v4-flash",
      "model:codex-acp:gpt-5-codex",
      "model:codex-acp:o4-mini",
    ]);
    expect(
      Array.from(group.querySelectorAll(".gset-prov-group"), (chip) => chip.textContent),
    ).toEqual(["deepseek", "Codex", "Codex"]);
  });

  test("marks Core's current selection and dispatches the pill's select intent", () => {
    const { panel, spies } = mount();

    expect(option(panel, "model:deepseek:deepseek-v4-flash").getAttribute("aria-checked")).toBe(
      "true",
    );

    option(panel, "model:codex-acp:o4-mini").click();

    expect(spies.onControl).toHaveBeenCalledWith({
      type: "select_model",
      providerId: "codex-acp",
      model: "o4-mini",
    } satisfies ComposerControlIntent);
  });

  test("states the absence rather than drawing an empty list when Core published none", () => {
    const { panel } = mount({ groups: [] });

    expect(panel.querySelector("[data-settings-no-models]")?.textContent).toBe(
      "Core has not published options.",
    );
  });

  test("ships no add-provider action and no credential field", () => {
    const { panel } = mount();

    // Credentials are GUI-CORE-001-residual territory and the design's request
    // knobs have no Core contract, so neither may be drawn here.
    expect(panel.querySelector("[data-settings-add-provider]")).toBeNull();
    expect(panel.querySelector("input[type='password']")).toBeNull();
    expect(panel.querySelector("[data-settings-requests]")).toBeNull();
  });

  test("disables rather than hides the models while Core cannot accept a control", () => {
    const { panel, spies } = mount({ enabled: false });

    const rows = panel.querySelectorAll<HTMLButtonElement>(
      '[data-settings-field="model"] [data-settings-option]',
    );
    expect(rows).toHaveLength(3);
    for (const row of rows) expect(row.disabled).toBe(true);
    rows[0]!.click();
    expect(spies.onControl).not.toHaveBeenCalled();
  });
});

describe("settings sections in the cockpit", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });
  afterEach(closeMountedPanels);

  test("dispatches the permission level through the cockpit's shared control path", async () => {
    const root = document.createElement("div");
    document.body.append(root);
    const sendComposerControl = vi.fn(
      async () =>
        await new Promise<never>(() => {
          /* never settles: the capture must not move */
        }),
    );
    const controller = renderD1Cockpit(
      root,
      D1_PROJECTION,
      async () => {
        throw new Error("unused");
      },
      async () => {
        throw new Error("unused");
      },
      undefined,
      undefined,
      {
        poll: false,
        showWelcome: false,
        sendComposerControl,
        preferences: {
          isAvailable: async () => true,
          save: async () => ({ status: "unavailable" }) as never,
          restore: async () => ({ status: "unavailable" }) as never,
        },
      },
    );

    root.querySelector<HTMLButtonElement>("[data-settings-toggle]")!.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    const panel = document.querySelector<HTMLElement>("[data-settings-panel]")!;

    // The panel reads the level from the same statusbar fact the pill reads.
    expect(option(panel, "permission:ask").getAttribute("aria-checked")).toBe("true");
    option(panel, "permission:read_only").click();

    expect(sendComposerControl).toHaveBeenCalledWith(
      { type: "set_permission_level", level: "read_only" },
      "lane-core",
    );
    controller.dispose();
  });

  test("offers the working directory and the models from the Core projection", async () => {
    const root = document.createElement("div");
    document.body.append(root);
    const controller = renderD1Cockpit(
      root,
      {
        ...D1_PROJECTION,
        agentAdapters: [
          { ...D1_PROJECTION.agentAdapters[0]!, models: ["gpt-5-codex"] },
        ],
      },
      async () => {
        throw new Error("unused");
      },
      async () => {
        throw new Error("unused");
      },
      undefined,
      undefined,
      {
        poll: false,
        showWelcome: false,
        sendComposerControl: async () =>
          await new Promise<never>(() => {
            /* never settles */
          }),
        preferences: {
          isAvailable: async () => true,
          save: async () => ({ status: "unavailable" }) as never,
          restore: async () => ({ status: "unavailable" }) as never,
        },
      },
    );

    root.querySelector<HTMLButtonElement>("[data-settings-toggle]")!.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    const panel = document.querySelector<HTMLElement>("[data-settings-panel]")!;

    expect(panel.querySelector("[data-settings-cwd]")?.textContent).toContain("/workspace/viden");
    expect(
      Array.from(
        panel.querySelectorAll<HTMLElement>(
          '[data-settings-field="model"] [data-settings-option]',
        ),
        (row) => row.dataset.settingsOption,
      ),
    ).toEqual(["model:deepseek:deepseek-v4-flash", "model:codex-acp:gpt-5-codex"]);
    controller.dispose();
  });
});
