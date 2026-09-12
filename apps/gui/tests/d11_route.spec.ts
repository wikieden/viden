// @vitest-environment jsdom

// D11 is reachable from the shell. These specs drive `hydrateShellFromCore`
// through an injected CoreClient — no Tauri module mock — so they prove the
// route itself, not a host detail.

import { beforeEach, describe, expect, test, vi } from "vitest";

import { hydrateShellFromCore } from "../src/main";
import { D11_RESULT, fakeCoreClient } from "./support/fake_core_client";

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

  test("the agent menu's full setup navigates to D11 instead of D4", async () => {
    const root = document.querySelector<HTMLElement>("#app");
    if (!root) throw new Error("test root is missing");
    const d11Poll = vi.fn(async () => D11_RESULT);
    const d4Poll = vi.fn(async () => {
      throw new Error("full setup must not enter D4");
    });

    await hydrateShellFromCore(root, fakeCoreClient({ d11Poll, d4Poll }));
    expect(root.dataset.route).toBe("d1");

    root.querySelector<HTMLButtonElement>("[data-create-lane]")?.click();
    // The menu portals outside the clipped rail, so it is found on the document.
    const fullSetup = await vi.waitFor(() => {
      const button = document.querySelector<HTMLButtonElement>("[data-full-setup]");
      if (!button) throw new Error("full setup is not rendered");
      return button;
    });
    fullSetup.click();

    await vi.waitFor(() => expect(root.dataset.route).toBe("d11"));
    expect(root.querySelector("[data-screen]")?.getAttribute("data-screen")).toBe("d11-intake");
    expect(d11Poll).toHaveBeenCalledTimes(1);
    expect(d4Poll).not.toHaveBeenCalled();
  });
});
