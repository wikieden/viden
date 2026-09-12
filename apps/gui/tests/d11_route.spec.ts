// @vitest-environment jsdom

// D11 is reachable from the shell. These specs drive `hydrateShellFromCore`
// through an injected CoreClient — no Tauri module mock — so they prove the
// route itself, not a host detail.

import { beforeEach, describe, expect, test, vi } from "vitest";

import { hydrateShellFromCore } from "../src/main";
import { D11_RESULT, D4_RESULT, fakeCoreClient } from "./support/fake_core_client";

describe("D11 intake routing", () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="app"></main>';
    window.history.replaceState({}, "", "/");
  });

  test("renders D11 from the shell for ?screen=d11", async () => {
    const root = document.querySelector<HTMLElement>("#app");
    if (!root) throw new Error("test root is missing");
    window.history.replaceState({}, "", "/?screen=d11");
    const d11Poll = vi.fn(async () => D11_RESULT);

    await hydrateShellFromCore(root, fakeCoreClient({ d11Poll }));

    expect(d11Poll).toHaveBeenCalledTimes(1);
    expect(root.dataset.route).toBe("d11");
    expect(root.querySelector("[data-screen]")?.getAttribute("data-screen")).toBe("d11-intake");
    // The intake screen renders its own Core-owned controls, not the cockpit.
    expect(root.querySelector("[data-probe-project]")).not.toBeNull();
    expect(root.querySelector("[data-confirm-config]")).not.toBeNull();
  });

  // `0.3.4` moved this route. "Full setup…" sits inside the New Lane popover
  // and now opens the *Lane* wizard it belongs to; sending it to project
  // intake asked the operator to configure the whole project in order to make
  // one Lane. D11 stays reachable from the project surfaces.
  test("the agent menu's full setup opens the D4 Lane wizard, not D11", async () => {
    const root = document.querySelector<HTMLElement>("#app");
    if (!root) throw new Error("test root is missing");
    const d11Poll = vi.fn(async () => {
      throw new Error("full setup must not enter D11");
    });
    const d4Poll = vi.fn(async () => D4_RESULT);

    await hydrateShellFromCore(root, fakeCoreClient({ d11Poll, d4Poll }));
    expect(root.dataset.route).toBe("d1");

    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    // The menu portals outside the clipped rail, so it is found on the document.
    const task = await vi.waitFor(() => {
      const field = document.querySelector<HTMLTextAreaElement>("[data-lane-task]");
      if (!field) throw new Error("the task field is not rendered");
      return field;
    });
    task.value = "Refactor the config loader";
    task.dispatchEvent(new InputEvent("input", { bubbles: true }));
    document.querySelector<HTMLButtonElement>("[data-full-setup]")!.click();

    await vi.waitFor(() => expect(root.dataset.route).toBe("d4"));
    expect(root.querySelector("[data-screen]")?.getAttribute("data-screen")).toBe(
      "d4-lane-create",
    );
    // The draft named the Lane and its branch rather than being discarded.
    expect(root.querySelector<HTMLInputElement>("[data-lane-id]")?.value).toBe(
      "refactor-the-config-loader",
    );
    expect(root.querySelector("[data-seed-task]")?.textContent).toBe(
      "Refactor the config loader",
    );
    expect(d4Poll).toHaveBeenCalledTimes(1);
    expect(d11Poll).not.toHaveBeenCalled();
  });

  test("cancelling the wizard returns to the cockpit with the draft intact", async () => {
    const root = document.querySelector<HTMLElement>("#app");
    if (!root) throw new Error("test root is missing");
    const d4Poll = vi.fn(async () => D4_RESULT);

    await hydrateShellFromCore(root, fakeCoreClient({ d4Poll }));
    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    const task = await vi.waitFor(() => {
      const field = document.querySelector<HTMLTextAreaElement>("[data-lane-task]");
      if (!field) throw new Error("the task field is not rendered");
      return field;
    });
    task.value = "Refactor the config loader";
    task.dispatchEvent(new InputEvent("input", { bubbles: true }));
    document.querySelector<HTMLButtonElement>("[data-full-setup]")!.click();
    await vi.waitFor(() => expect(root.dataset.route).toBe("d4"));

    document.querySelector<HTMLButtonElement>("[data-cancel-d4]")!.click();
    await vi.waitFor(() => expect(root.dataset.route).toBe("d1"));
    const returned = await vi.waitFor(() => {
      const field = document.querySelector<HTMLTextAreaElement>("[data-lane-task]");
      if (!field) throw new Error("the popover did not reopen");
      return field;
    });
    expect(returned.value).toBe("Refactor the config loader");
  });
});
