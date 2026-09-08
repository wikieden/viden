// @vitest-environment jsdom

import { beforeEach, describe, expect, test, vi } from "vitest";

import { renderD1Cockpit, type D1CockpitProjection } from "../src/screens/d1_cockpit";
import type { PermissionIntent } from "../src/components/permission_dock";
import { D1_PROJECTION } from "./support/d1_projection";

/**
 * The design's deny redirect (`Viden - 桌面驾驶舱 (GUI).html`, the composer
 * beside the permission dock): after Deny the composer prompt becomes
 * "Tell <agent> what to do instead…" and the operator types the correction
 * there.
 *
 * This is presentation only. The redirect is set when the deny is dispatched,
 * not when Core answers, and it changes no fact: `feedback` stays `null`
 * because schema 1 carries no feedback field (GUI-CORE-019).
 */

const REQUEST: NonNullable<D1CockpitProjection["permissionDock"]["request"]> = {
  id: "approval-shell",
  toolName: "shell",
  title: "Approve shell",
  message: "Core requests scoped permission",
  inputPreview: "rm -rf target && cargo build --release",
  isMutating: true,
  reason: "recursive delete outside the allowlist",
  risk: "high",
  target: { kind: "repo_path", display: "/workspace/viden/target", canonicalRef: null },
  policyReasonKey: "permission.requires_approval",
  policyReasonArgs: {},
  expiresAt: 1_700_003_600,
  defaultAction: "deny",
  auditId: "audit-shell",
  blockedByPlan: false,
  actions: [
    { kind: "once", available: true, sessionId: null, paths: [], code: null },
    { kind: "deny", available: true, sessionId: null, paths: [], code: null },
  ],
};

function pending(
  overrides: Partial<D1CockpitProjection> = {},
): D1CockpitProjection {
  return {
    ...D1_PROJECTION,
    permissionDock: { ...D1_PROJECTION.permissionDock, request: REQUEST },
    ...overrides,
  };
}

/** The same request under a fresh Core id, as a later ask would arrive. */
function nextRequest(projection: D1CockpitProjection): D1CockpitProjection {
  return {
    ...projection,
    permissionDock: {
      ...projection.permissionDock,
      request: { ...REQUEST, id: "approval-write" },
    },
  };
}

function setup(projection = pending()) {
  const root = document.createElement("div");
  document.body.append(root);
  const sendPermission = vi.fn(async (_intent: PermissionIntent) => undefined);
  const send = vi.fn(async () => ({
    projection,
    pendingCommandId: null,
    outcome: { state: "idle" as const, reason: null },
  }));
  const controller = renderD1Cockpit(
    root,
    projection,
    send,
    async () => ({
      projection,
      pendingCommandId: null,
      outcome: { state: "idle" as const, reason: null },
    }),
    sendPermission,
    undefined,
    { poll: false, showWelcome: false },
  );
  return { root, controller, sendPermission, send };
}

function composer(root: HTMLElement): HTMLTextAreaElement {
  return root.querySelector<HTMLTextAreaElement>("[data-composer]")!;
}

function click(root: HTMLElement, choice: string): void {
  root.querySelector<HTMLButtonElement>(`[data-permission-action="${choice}"]`)!.click();
}

describe("permission deny redirect", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  test("switches the composer prompt and takes focus after a deny", () => {
    const { root, controller } = setup();
    const before = composer(root).placeholder;

    click(root, "deny");

    const after = composer(root);
    expect(after.placeholder).not.toBe(before);
    expect(after.placeholder).toBe("Tell the agent what to do instead…");
    expect(document.activeElement).toBe(after);
    controller.dispose();
  });

  test("names the agent when the focused conversation has one", () => {
    const projection = pending({
      agentSessions: [
        {
          // The session id Core's `laneAgent` fact names, which is what makes
          // this Lane's conversation the ACP one rather than the native one.
          sessionId: "session-lane-core",
          laneId: "lane-core",
          agentId: "codex-acp",
          model: null,
          status: "waiting_approval",
          task: "Freeze contract",
          diagnostic: null,
        },
      ],
    });
    const { root, controller } = setup(projection);

    click(root, "deny");

    // `Codex` is the adapter display name Core published, not a client guess.
    expect(composer(root).placeholder).toBe("Tell Codex what to do instead…");
    controller.dispose();
  });

  test("still sends the deny with a null feedback field", () => {
    const { root, controller, sendPermission } = setup();

    click(root, "deny");

    expect(sendPermission).toHaveBeenCalledWith({
      type: "respond",
      requestId: "approval-shell",
      choice: "deny",
      feedback: null,
    });
    controller.dispose();
  });

  test("leaves the prompt alone when the operator approves", () => {
    const { root, controller } = setup();
    const before = composer(root).placeholder;

    click(root, "once");

    expect(composer(root).placeholder).toBe(before);
    controller.dispose();
  });

  test("clears the redirect on the next submit", () => {
    const projection = pending({
      permissionDock: { ...D1_PROJECTION.permissionDock, request: null },
      composer: { editable: true, busy: false, canCancel: false, canSubmitImmediately: true },
    });
    const { root, controller } = setup(pending());
    click(root, "deny");
    expect(composer(root).placeholder).toBe("Tell the agent what to do instead…");

    controller.applyProjection(projection);
    const field = composer(root);
    field.value = "use a scoped clean instead";
    field.dispatchEvent(new Event("input", { bubbles: true }));
    root.querySelector<HTMLButtonElement>("[data-composer-send]")!.click();

    expect(composer(root).placeholder).toBe("Message the selected Lane. Shift+Enter adds a line.");
    controller.dispose();
  });

  test("clears the redirect when Core publishes a new permission request", () => {
    const { root, controller } = setup();
    click(root, "deny");
    expect(composer(root).placeholder).toBe("Tell the agent what to do instead…");

    controller.applyProjection(nextRequest(pending()));

    expect(composer(root).placeholder).toBe("Message the selected Lane. Shift+Enter adds a line.");
    controller.dispose();
  });

  test("keeps the redirect while the same request is still republished", () => {
    const { root, controller } = setup();
    click(root, "deny");

    // A refresh that still carries the request the operator just denied is the
    // ordinary case between the command and Core's answer; it must not undo
    // the redirect the operator is typing into.
    controller.applyProjection(pending({ statusbar: { ...D1_PROJECTION.statusbar, eventStreamPosition: 8 } }));

    expect(composer(root).placeholder).toBe("Tell the agent what to do instead…");
    controller.dispose();
  });
});
