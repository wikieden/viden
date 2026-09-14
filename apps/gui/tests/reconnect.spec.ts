// @vitest-environment jsdom

import { expect, test, vi } from "vitest";

import {
  renderD6Recovery,
  type D6RecoveryProjection,
} from "../src/screens/d6_recovery";

test("reconnect stays recovering and blocks success until Core publishes a live snapshot", async () => {
  document.body.innerHTML = '<main id="stage"></main>';
  const root = document.querySelector<HTMLElement>("#stage")!;
  const recovering: D6RecoveryProjection = {
    connection: "recovering",
    state: "event_gap",
    detail: "expected 4, received 7",
    hint: null,
    recoverable: true,
    businessSuccessBlocked: true,
    usedTokens: null,
    hardTokenLimit: null,
    missingCapabilities: [],
    reconnect: null,
    actions: [{ kind: "reconnect", available: true, code: "core_client" }],
  };
  const live: D6RecoveryProjection = {
    ...recovering,
    connection: "live",
    state: "live",
    detail: null,
    businessSuccessBlocked: false,
  };
  let resolve!: (projection: D6RecoveryProjection) => void;
  const reconnect = vi.fn(
    () => new Promise<D6RecoveryProjection>((done) => {
      resolve = done;
    }),
  );
  const controller = renderD6Recovery(root, recovering, reconnect, "en");
  root.querySelector<HTMLButtonElement>('[data-d6-action="reconnect"]')!.click();
  expect(root.querySelector("[data-d6-state]")?.getAttribute("aria-busy")).toBe("true");
  expect(root.querySelector("[data-d6-success]")).toBeNull();

  resolve(live);
  await vi.waitFor(() => expect(reconnect).toHaveBeenCalledOnce());
  controller.applyProjection(live);
  expect(root.querySelector("[data-d6-state]")?.getAttribute("aria-busy")).toBe("false");
  expect(root.querySelector("[data-d6-success]")).not.toBeNull();
});
