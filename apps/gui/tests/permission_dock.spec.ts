// @vitest-environment jsdom

import { describe, expect, test, vi } from "vitest";

import {
  renderPermissionDock,
  type PermissionDockProjection,
  type PermissionIntent,
} from "../src/components/permission_dock";

const PROJECTION: PermissionDockProjection = {
  workMode: "build",
  permissionLevel: "ask",
  standing: { capabilityAvailable: true, rules: [] },
  request: {
    // `shell` is on Core's editable list: its input is a flat text map an
    // operator can safely narrow, which is what makes Edit live here.
    editable: true,
    revisedFrom: null,
    inputFields: [{ key: "command", value: "rm -rf target && cargo build --release" }],
    editBlock: null,
    id: "approval-shell",
    toolName: "shell",
    title: "Approve shell",
    message: "Core requests scoped permission",
    inputPreview: "rm -rf target && cargo build --release",
    isMutating: true,
    reason: "recursive delete outside the allowlist",
    risk: "high",
    target: {
      kind: "repo_path",
      display: "/workspace/viden/target",
      canonicalRef: "repo://target",
    },
    policyReasonKey: "permission.requires_approval",
    policyReasonArgs: {},
    expiresAt: 1_700_003_600,
    defaultAction: "deny",
    auditId: "audit-shell",
    blockedByPlan: false,
    actions: [
      { kind: "once", available: true, sessionId: null, paths: [], code: null },
      {
        kind: "session",
        available: true,
        sessionId: "session-core",
        paths: [],
        code: null,
      },
      {
        kind: "repo_allowlist",
        available: true,
        sessionId: null,
        paths: ["/workspace/viden/apps/gui"],
        code: null,
      },
      {
        // High risk, so Core never offers the standing scope. The tier is still
        // drawn, with Core's own refusal name on it.
        kind: "always",
        available: false,
        sessionId: null,
        paths: [],
        code: "scope_not_offered",
      },
      { kind: "edit", available: true, sessionId: null, paths: [], code: null },
      { kind: "deny", available: true, sessionId: null, paths: [], code: null },
    ],
  },
};

function setup(projection = PROJECTION) {
  document.body.innerHTML = '<main id="dock"></main>';
  const root = document.querySelector<HTMLElement>("#dock")!;
  const send = vi.fn(async (_intent: PermissionIntent) => undefined);
  renderPermissionDock(root, projection, send, "en");
  return { root, send };
}

describe("Permission Dock", () => {
  test("renders exact risk target scope reason input expiry default and audit facts", () => {
    const { root } = setup();
    const dock = root.querySelector<HTMLElement>("[data-permission-dock]")!;
    expect(dock.classList.contains("gperm")).toBe(true);
    expect(dock.classList.contains("dock")).toBe(true);
    expect(dock.textContent).toContain("High risk");
    expect(dock.textContent).toContain("Repository path");
    expect(dock.textContent).toContain("/workspace/viden/target");
    expect(dock.textContent).toContain("recursive delete outside the allowlist");
    expect(dock.textContent).toContain("rm -rf target && cargo build --release");
    expect(dock.textContent).toContain("1700003600");
    expect(dock.textContent).toContain("Default: Deny");
    expect(dock.textContent).toContain("audit-shell");
    expect(dock.getAttribute("role")).toBe("region");
  });

  test("sends only exact typed choices and names a scope Core did not offer", () => {
    const { root, send } = setup();
    root.querySelector<HTMLButtonElement>('[data-permission-action="repo_allowlist"]')!.click();
    expect(send).toHaveBeenCalledWith({
      type: "respond",
      requestId: "approval-shell",
      choice: "repo_allowlist",
      feedback: null,
    });
    // Always is the one tier this high-risk request does not get: Core never
    // offers a standing rule above medium risk, and the chip says so in Core's
    // own refusal name rather than hiding the tier.
    const always = root.querySelector<HTMLButtonElement>('[data-permission-action="always"]')!;
    expect(always.disabled).toBe(true);
    expect(always.dataset.permissionBlock).toBe("scope_not_offered");
    expect(always.getAttribute("aria-label")).toContain("did not offer a standing rule");
  });

  test("uses the compact required action labels without changing typed choices", () => {
    const { root } = setup();
    expect(
      ["once", "session", "always", "edit", "deny"].map(
        (kind) =>
          root
            .querySelector<HTMLButtonElement>(`[data-permission-action="${kind}"]`)
            ?.textContent,
      ),
    ).toEqual([
      "Y · Once",
      "A · Session",
      // `D-PERM`'s ⇧A tier is back on Always; this request is high-risk, so
      // Core did not offer it and the chip carries Core's own refusal name.
      "Shift+A · Always · scope_not_offered",
      "E · Edit",
      "N · Deny",
    ]);
  });

  test("Plan mutation denial disables every approval response and transports nothing", () => {
    const projection: PermissionDockProjection = {
      ...PROJECTION,
      workMode: "plan",
      request: {
        ...PROJECTION.request!,
        blockedByPlan: true,
        actions: PROJECTION.request!.actions.map((action) => ({ ...action, available: false })),
      },
    };
    const { root, send } = setup(projection);
    expect(root.querySelector('[role="alert"]')?.textContent).toContain("Plan");
    root.querySelectorAll<HTMLButtonElement>("[data-permission-action]").forEach((button) => {
      button.click();
    });
    expect(send).not.toHaveBeenCalled();
    expect(root.querySelector<HTMLButtonElement>('[data-permission-action="deny"]')!.disabled).toBe(true);
  });

  test("supports keyboard shortcuts and visible focus without color-only status", () => {
    const { root, send } = setup();
    const dock = root.querySelector<HTMLElement>("[data-permission-dock]")!;
    dock.dispatchEvent(new KeyboardEvent("keydown", { key: "y", bubbles: true }));
    expect(send).toHaveBeenCalledWith({
      type: "respond",
      requestId: "approval-shell",
      choice: "once",
      feedback: null,
    });
    const first = root.querySelector<HTMLButtonElement>('[data-permission-action="once"]')!;
    expect(document.activeElement).toBe(first);
    expect(first.getAttribute("aria-keyshortcuts")).toBe("Y");
  });

  test("honors every advertised available permission shortcut", () => {
    // `Shift+A` is the design's Always tier (`D-PERM`), so the fixture has to
    // be a request Core actually offered the standing scope on.
    const offered: PermissionDockProjection = {
      ...PROJECTION,
      request: {
        ...PROJECTION.request!,
        risk: "medium",
        actions: PROJECTION.request!.actions.map((action) =>
          action.kind === "always" ? { ...action, available: true, code: null } : action,
        ),
      },
    };
    const { root, send } = setup(offered);
    const dock = root.querySelector<HTMLElement>("[data-permission-dock]")!;
    dock.dispatchEvent(new KeyboardEvent("keydown", { key: "a", bubbles: true }));
    dock.dispatchEvent(new KeyboardEvent("keydown", { key: "A", shiftKey: true, bubbles: true }));
    expect(send).toHaveBeenNthCalledWith(1, {
      type: "respond", requestId: "approval-shell", choice: "session", feedback: null,
    });
    expect(send).toHaveBeenNthCalledWith(2, {
      type: "respond", requestId: "approval-shell", choice: "always", feedback: null,
    });
  });

  test("Edit opens the inline field and sends the whole revised input", () => {
    const { root, send } = setup();
    const dock = root.querySelector<HTMLElement>("[data-permission-dock]")!;
    const edit = dock.querySelector<HTMLButtonElement>("[data-permission-action='edit']")!;
    expect(edit.disabled).toBe(false);
    expect(dock.querySelector<HTMLElement>("[data-permission-editor]")!.hidden).toBe(true);

    edit.click();
    const editor = dock.querySelector<HTMLFormElement>("[data-permission-editor]")!;
    expect(editor.hidden).toBe(false);
    expect(edit.getAttribute("aria-expanded")).toBe("true");
    // Pressing Edit is not itself a decision: nothing is sent until the
    // revision is.
    expect(send).not.toHaveBeenCalled();

    const field = editor.querySelector<HTMLInputElement>("[data-permission-edit-field='command']")!;
    expect(field.value).toBe("rm -rf target && cargo build --release");
    field.value = "rm -rf target/debug";
    field.dispatchEvent(new Event("input"));
    editor.dispatchEvent(new Event("submit", { cancelable: true }));

    expect(send).toHaveBeenCalledWith({
      type: "respond",
      requestId: "approval-shell",
      choice: "edit",
      feedback: null,
      revisedInput: { command: "rm -rf target/debug" },
    });
  });

  test("withdraws Edit when Core marked it editable and the preview will not parse", () => {
    const unparseable: PermissionDockProjection = {
      ...PROJECTION,
      request: {
        ...PROJECTION.request!,
        inputFields: [],
        editBlock: "GUI-CORE-035",
        actions: PROJECTION.request!.actions.map((action) =>
          action.kind === "edit"
            ? { ...action, available: false, code: "GUI-CORE-035" }
            : action,
        ),
      },
    };
    const { root } = setup(unparseable);
    const edit = root.querySelector<HTMLButtonElement>("[data-permission-action='edit']")!;
    expect(edit.disabled).toBe(true);
    expect(edit.getAttribute("aria-label")).toContain("could not be parsed exactly");
  });

  test("shows a revision linked to the request it replaces", () => {
    const revision: PermissionDockProjection = {
      ...PROJECTION,
      request: { ...PROJECTION.request!, revisedFrom: "approval-shell-original" },
    };
    const { root } = setup(revision);
    const row = root.querySelector<HTMLElement>(
      "[data-permission-revised-from='approval-shell-original']",
    );
    expect(row?.textContent).toContain("approval-shell-original");
  });

  test("keeps all selected approval actions keyboard-addressable with their exact request id", () => {
    const { root, send } = setup();
    const dock = root.querySelector<HTMLElement>("[data-permission-dock]")!;

    expect(
      Array.from(dock.querySelectorAll<HTMLButtonElement>("[data-permission-action]"), (button) =>
        button.dataset.permissionAction,
      ),
    ).toEqual(["once", "session", "repo_allowlist", "always", "edit", "deny"]);
    dock.dispatchEvent(new KeyboardEvent("keydown", { key: "n", bubbles: true }));
    expect(send).toHaveBeenCalledWith({
      type: "respond",
      requestId: "approval-shell",
      choice: "deny",
      feedback: null,
    });
  });

  test("does not render a permission request from a different selected Lane", () => {
    const { root } = setup({ ...PROJECTION, request: null });
    expect(root.querySelector("[data-permission-dock]")).toBeNull();
  });
});

describe("operator source-control asks", () => {
  test("a `git` target is named, not rendered as a kind the dock cannot read", () => {
    // Core stamps `target.kind = "git"` on all five operator source-control
    // actions, so the dock groups them as one decision. A kind this build
    // cannot name falls back to the unavailable sentence, which for a real
    // commit ask would be plainly wrong.
    const root = document.createElement("div");
    renderPermissionDock(
      root,
      {
        ...PROJECTION,
        request: {
          ...PROJECTION.request!,
          id: "approval_operator_git_commit",
          toolName: "git_commit",
          risk: "medium",
          target: {
            kind: "git",
            display: "git_commit (workspace)",
            canonicalRef: "workspace",
          },
        },
      },
      async () => undefined,
      "en",
    );

    const facts = root.textContent ?? "";
    expect(facts).toContain("Source control");
    expect(facts).toContain("git_commit (workspace)");
    expect(facts).not.toContain("Unavailable");
  });
});

describe("the audit trail C7 made durable", () => {
  test("the link opens D14 by the permission object, never by the audit id", () => {
    const onOpenAuditTrail = vi.fn();
    const root = document.createElement("div");
    renderPermissionDock(root, PROJECTION, async () => undefined, "en", onOpenAuditTrail);
    const trail = root.querySelector<HTMLButtonElement>(
      "[data-permission-audit-trail='approval-shell']",
    );
    // `D-AUDIT` runs one way: a record names the object, so the trail is
    // opened by the object. Querying by `audit-shell` would ask D14 for a row
    // by id, which its filters do not express.
    trail?.click();
    expect(onOpenAuditTrail).toHaveBeenCalledExactlyOnceWith({
      kind: "permission",
      id: "approval-shell",
    });
    // The row exists once the decision is applied, and the control says so
    // rather than implying this open request already has one.
    expect(trail?.title).toContain("when your decision is applied");
  });

  test("with no host for D14 the audit id stays a fact and no control appears", () => {
    const root = document.createElement("div");
    renderPermissionDock(root, PROJECTION, async () => undefined, "en");
    expect(root.querySelector("[data-permission-audit-trail]")).toBeNull();
    expect(root.textContent).toContain("audit-shell");
  });
});
