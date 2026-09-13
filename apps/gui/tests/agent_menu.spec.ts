// @vitest-environment jsdom

import { beforeEach, describe, expect, test, vi } from "vitest";

import { renderAgentMenu, type AgentMenuModel } from "../src/components/agent_menu";

const MODEL: AgentMenuModel = {
  locale: "en",
  canCreateLane: true,
  usesGitIsolation: true,
  probing: false,
  eligibilityDiagnostic: null,
  discoveryDiagnostic: null,
  adapters: [
    {
      agentId: "codex-acp",
      displayName: "Codex",
      startability: "ready",
      diagnostics: [],
    },
  ],
};

function setup(model: AgentMenuModel = MODEL) {
  document.body.innerHTML = '<div id="host"><button id="anchor">+</button></div>';
  const anchor = document.querySelector<HTMLButtonElement>("#anchor")!;
  const onSelect = vi.fn();
  const controller = renderAgentMenu(anchor, model, onSelect);
  return { anchor, onSelect, controller };
}

describe("compact agent menu", () => {
  beforeEach(() => {
    document.documentElement.lang = "en";
  });

  test("offers every ready Agent as the unique owner of a new Lane", () => {
    const { controller } = setup();

    expect(controller.root.textContent).toContain("New lane");
    expect(controller.root.textContent).toContain("Viden Agent");
    expect(controller.root.textContent).not.toContain("DELEGATE TO CURRENT LANE");
    expect(controller.root.textContent).toContain("Codex");
    expect(controller.root.textContent).toContain("ACP");
    expect(controller.root.querySelector('[data-agent-logo="viden"]')).not.toBeNull();
    expect(controller.root.querySelector('[data-agent-logo="codex"]')).not.toBeNull();
    expect(
      controller.root.querySelector('[data-agent-id="codex-acp"]')?.getAttribute("aria-disabled"),
    ).toBe("false");
  });

  test("defaults to the built-in Agent and requires only a task", () => {
    const { controller } = setup();
    const radios = Array.from(
      controller.root.querySelectorAll<HTMLElement>('[role="radio"]'),
    );
    const task = controller.root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;
    const create = controller.root.querySelector<HTMLButtonElement>("[data-lane-task-submit]")!;

    expect(radios[0]?.getAttribute("aria-checked")).toBe("true");
    task.value = "Review the parser diff";
    task.dispatchEvent(new InputEvent("input", { bubbles: true }));
    expect(create.disabled).toBe(false);
    expect(create.textContent).toContain("Create lane");
  });

  // Defect 14 (E3a): discovery used to gate *every* creation path, including
  // the built-in runtime that needs no probe at all. The gate is now the
  // selected adapter's own startability, so ACP discovery blocks only ACP.
  test("blocks an ACP Lane, never the built-in one, throughout Agent discovery", () => {
    const { controller } = setup({
      ...MODEL,
      probing: true,
      adapters: [
        {
          agentId: "codex-acp",
          displayName: "Codex",
          startability: "probe_required",
          diagnostics: [],
          probe: "checking",
        },
      ],
    });
    const task = controller.root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;
    const create = controller.root.querySelector<HTMLButtonElement>("[data-lane-task-submit]")!;

    controller.root.querySelector<HTMLButtonElement>("[data-native-agent]")?.click();
    task.value = "Use the native Agent";
    task.dispatchEvent(new InputEvent("input", { bubbles: true }));

    expect(create.disabled).toBe(false);
    expect(
      controller.root.querySelector('[data-agent-id="codex-acp"]')?.getAttribute("aria-disabled"),
    ).toBe("true");
  });

  test("presents built-in ACP adapters in the stable product order", () => {
    const { controller } = setup({
      ...MODEL,
      adapters: [
        {
          agentId: "custom-z",
          displayName: "Zed ACP",
          startability: "ready",
          diagnostics: [],
        },
        { agentId: "kiro-cli", displayName: "Kiro", startability: "ready", diagnostics: [] },
        {
          agentId: "claude-acp",
          displayName: "Claude",
          startability: "ready",
          diagnostics: [],
        },
        {
          agentId: "custom-acp",
          displayName: "Custom ACP",
          startability: "ready",
          diagnostics: [],
        },
        {
          agentId: "codex-acp",
          displayName: "Codex",
          startability: "ready",
          diagnostics: [],
        },
      ],
    });

    expect(
      Array.from(controller.root.querySelectorAll<HTMLElement>("[data-agent-id]")).map(
        (item) => item.dataset.agentId,
      ),
    ).toEqual(["codex-acp", "claude-acp", "kiro-cli", "custom-acp", "custom-z"]);
  });

  test("keeps every Agent option inside the radiogroup after the heading", () => {
    const { controller } = setup();
    const heading = controller.root.querySelector(".agent-menu-heading");
    const group = controller.root.querySelector<HTMLElement>('[role="radiogroup"]');
    expect(heading).not.toBeNull();
    expect(group).not.toBeNull();

    expect(
      Array.from(controller.root.children).indexOf(heading as Element),
    ).toBeLessThan(Array.from(controller.root.children).indexOf(group as Element));
    expect(
      Array.from(controller.root.querySelectorAll<HTMLElement>("[data-agent-id]")).every(
        (agent) => agent.closest('[role="radiogroup"]') === group,
      ),
    ).toBe(true);
  });

  test("disables every Agent when Core says a new Lane is ineligible", () => {
    const { controller } = setup({ ...MODEL, canCreateLane: false });

    expect(
      controller.root.querySelector('[data-agent-id="codex-acp"]')?.getAttribute("aria-disabled"),
    ).toBe("true");
  });

  test("explains that a non-Git Lane uses the opened workspace directly", () => {
    const { controller } = setup({ ...MODEL, usesGitIsolation: false });
    const hint = controller.root.querySelector<HTMLElement>("[data-lane-hint]");

    expect(hint?.textContent).toContain("without a Git branch or worktree");
    expect(hint?.textContent).not.toContain(".worktrees/");
    expect(
      controller.root.querySelector("[data-native-agent]")?.getAttribute("aria-disabled"),
    ).toBe("false");
  });

  test("uses roving Agent keyboard focus and restores the trigger on Escape", () => {
    const { anchor, controller } = setup();
    const native = controller.root.querySelector<HTMLButtonElement>("[data-native-agent]")!;
    const acp = controller.root.querySelector<HTMLButtonElement>('[data-agent-id="codex-acp"]')!;
    expect(document.activeElement).toBe(
      controller.root.querySelector<HTMLTextAreaElement>("[data-lane-task]"),
    );

    native.focus();
    native.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    expect(document.activeElement).toBe(acp);
    acp.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));

    expect(document.activeElement).toBe(anchor);
    expect(document.querySelector('[role="menu"]')).toBeNull();
  });

  test("keeps textarea focus for navigation keys while editing the task", () => {
    const { controller } = setup();
    const task = controller.root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;
    task.focus();

    for (const key of ["ArrowUp", "ArrowDown", "Home", "End"]) {
      task.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
      expect(document.activeElement).toBe(task);
    }
  });

  test("selects only a Core-ready item", () => {
    const { onSelect, controller } = setup();
    controller.root.querySelector<HTMLButtonElement>('[data-agent-id="codex-acp"]')?.click();
    expect(onSelect).toHaveBeenCalledWith({ kind: "acp", agentId: "codex-acp" });
  });

  test("keeps Agent selection, task draft, and Create in one focused popover", () => {
    const { onSelect, controller } = setup();

    const task = controller.root.querySelector<HTMLTextAreaElement>("[data-lane-task]");
    const create = controller.root.querySelector<HTMLButtonElement>("[data-lane-task-submit]");
    expect(task).not.toBeNull();
    expect(create).not.toBeNull();
    expect(document.activeElement).toBe(task);
    expect(create?.disabled).toBe(true);

    controller.root.querySelector<HTMLButtonElement>('[data-agent-id="codex-acp"]')?.click();
    expect(onSelect).toHaveBeenCalledWith({ kind: "acp", agentId: "codex-acp" });
    expect(document.body.contains(controller.root)).toBe(true);
    expect(controller.root.querySelector('[data-agent-id="codex-acp"]')?.getAttribute("aria-pressed")).toBe(
      "true",
    );

    task!.value = "Review the parser diff";
    task!.dispatchEvent(new InputEvent("input", { bubbles: true }));
    expect(create?.disabled).toBe(false);
  });

  test("matches the compact reference shell and exposes Full setup", () => {
    document.body.innerHTML = '<div id="host"><button id="anchor">+</button></div>';
    const anchor = document.querySelector<HTMLButtonElement>("#anchor")!;
    const onFullSetup = vi.fn();
    const controller = renderAgentMenu(
      anchor,
      MODEL,
      vi.fn(),
      vi.fn(),
      async () => true,
      vi.fn(),
      onFullSetup,
    );

    expect(controller.root.getAttribute("role")).toBe("dialog");
    expect(controller.root.querySelector("[data-lane-scope]")?.textContent).toContain(
      "Current project",
    );
    expect(controller.root.querySelector("[data-lane-branch]")?.textContent).toContain(
      "vd/new-lane",
    );
    expect(controller.root.querySelector("[data-lane-isolation]")?.textContent).toContain(
      "local",
    );
    controller.root.querySelector<HTMLButtonElement>("[data-full-setup]")?.click();
    expect(onFullSetup).toHaveBeenCalledTimes(1);
  });

  test("portals both New Lane actions outside the clipped rail at 1228x768", async () => {
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 1228 });
    Object.defineProperty(window, "innerHeight", { configurable: true, value: 768 });
    document.body.innerHTML =
      '<nav id="rail" style="overflow:hidden"><button id="anchor">New Lane</button></nav>';
    const anchor = document.querySelector<HTMLButtonElement>("#anchor")!;
    anchor.getBoundingClientRect = () =>
      ({
        bottom: 118,
        height: 32,
        left: 64,
        right: 270,
        top: 86,
        width: 206,
        x: 64,
        y: 86,
        toJSON: () => ({}),
      }) as DOMRect;
    const onClose = vi.fn();
    const onCreate = vi.fn(async () => false);

    const controller = renderAgentMenu(
      anchor,
      MODEL,
      vi.fn(),
      onClose,
      onCreate,
    );

    expect(controller.root.parentElement).toBe(document.body);
    expect(controller.root.style.getPropertyValue("--agent-menu-anchor-inline")).toBe("270px");
    expect(controller.root.style.getPropertyValue("--agent-menu-anchor-block")).toBe("118px");

    const task = controller.root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;
    task.value = "Inspect README";
    task.dispatchEvent(new InputEvent("input", { bubbles: true }));
    controller.root.querySelector<HTMLButtonElement>("[data-native-agent]")?.click();
    controller.root.querySelector<HTMLButtonElement>("[data-lane-task-submit]")?.click();
    await vi.waitFor(() =>
      expect(onCreate).toHaveBeenCalledWith({ kind: "native" }, "Inspect README"),
    );

    controller.root.querySelector<HTMLButtonElement>(".agent-menu-actions button")?.click();
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test("creates from Cmd/Ctrl+Enter only outside IME composition", async () => {
    const onCreate = vi.fn(async () => true);
    document.body.innerHTML = '<div id="host"><button id="anchor">+</button></div>';
    const anchor = document.querySelector<HTMLButtonElement>("#anchor")!;
    const controller = renderAgentMenu(anchor, MODEL, vi.fn(), vi.fn(), onCreate);
    const task = controller.root.querySelector<HTMLTextAreaElement>("[data-lane-task]")!;

    controller.root.querySelector<HTMLButtonElement>("[data-native-agent]")?.click();
    task.value = "Write parser tests";
    task.dispatchEvent(new InputEvent("input", { bubbles: true }));
    task.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true }));
    task.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Enter", metaKey: true, bubbles: true }),
    );
    expect(onCreate).not.toHaveBeenCalled();

    task.dispatchEvent(new CompositionEvent("compositionend", { bubbles: true }));
    task.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Enter", metaKey: true, bubbles: true }),
    );
    await vi.waitFor(() => {
      expect(onCreate).toHaveBeenCalledWith({ kind: "native" }, "Write parser tests");
    });
  });
});
