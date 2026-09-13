// @vitest-environment jsdom

import { expect, test, vi } from "vitest";

import { renderD1Cockpit } from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

test("CJK composition, multiline paste, and undo do not submit prematurely", () => {
  document.body.innerHTML = '<main id="app"></main>';
  const root = document.querySelector<HTMLElement>("#app")!;
  const result = {
    projection: D1_PROJECTION,
    pendingCommandId: null,
    outcome: { state: "confirmed" as const, reason: null },
  };
  const send = vi.fn(async () => result);
  renderD1Cockpit(root, D1_PROJECTION, send, async () => result);
  const composer = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;

  composer.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true }));
  composer.value = "你好";
  composer.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertCompositionText" }));
  composer.dispatchEvent(
    new KeyboardEvent("keydown", { key: "Enter", bubbles: true, isComposing: true }),
  );
  expect(send).not.toHaveBeenCalled();

  composer.dispatchEvent(new CompositionEvent("compositionend", { bubbles: true }));
  composer.value = "你好\n第二行";
  composer.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertFromPaste" }));
  composer.dispatchEvent(new InputEvent("beforeinput", { bubbles: true, inputType: "historyUndo" }));
  expect(send).not.toHaveBeenCalled();

  composer.value = "你好";
  composer.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  expect(send).toHaveBeenCalledWith({
    type: "submit",
    laneId: "lane-core",
    content: "你好",
  });
});

test("a pending approval keeps the composer editable through CJK composition and undo", () => {
  document.body.innerHTML = '<main id="app"></main>';
  const root = document.querySelector<HTMLElement>("#app")!;
  const result = {
    projection: D1_PROJECTION,
    pendingCommandId: null,
    outcome: { state: "confirmed" as const, reason: null },
  };
  const send = vi.fn(async () => result);
  renderD1Cockpit(
    root,
    {
      ...D1_PROJECTION,
      permissionDock: { standing: { capabilityAvailable: true, rules: [] },
        workMode: "build",
        permissionLevel: "ask",
        request: {
          id: "approval-ime",
          toolName: "shell",
          title: "Permission request",
          message: "Approval is pending.",
          inputPreview: "cargo test",
          isMutating: true,
          reason: null,
          risk: "high",
          target: { kind: "repo_path", display: "apps/gui", canonicalRef: null },
          policyReasonKey: "permission.command_not_allowlisted",
          policyReasonArgs: {},
          expiresAt: 0,
          defaultAction: "deny",
          auditId: "audit-ime",
          blockedByPlan: false,
          editable: false,
          revisedFrom: null,
          inputFields: [],
          editBlock: null,
          actions: [],
        },
      },
    },
    send,
    async () => result,
  );
  const composer = root.querySelector<HTMLTextAreaElement>("[data-composer]")!;

  expect(composer.disabled).toBe(false);
  composer.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true }));
  composer.value = "审批中\n保留草稿";
  composer.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertFromPaste" }));
  composer.dispatchEvent(new InputEvent("beforeinput", { bubbles: true, inputType: "historyUndo" }));
  composer.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, isComposing: true }));

  expect(send).not.toHaveBeenCalled();
  expect(composer.value).toBe("审批中\n保留草稿");
});
