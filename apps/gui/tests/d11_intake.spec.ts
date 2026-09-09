// @vitest-environment jsdom

import { beforeEach, describe, expect, test, vi } from "vitest";

import type {
  D11IntakeProjection,
  D11IntentResult,
  D11RecentWorkPort,
} from "../src/screens/d11_intake";
import type { RecentWorkResult } from "../src/models/recent_work";
import { renderD11Intake } from "../src/screens/d11_intake";
import type { D4StarterSeed } from "../src/screens/d4_lane_create";

const EMPTY_PROJECTION: D11IntakeProjection = {
  project: null,
  preview: null,
  confirmedConfig: null,
  provider: null,
  credentialHandles: [],
  starterLanes: [],
  pendingApproval: null,
  lastError: null,
  recentWork: {
    available: true,
    code: "core_command",
    message: "Recent project and session history is served by Core.",
  },
  credentialIngress: {
    available: false,
    code: "GUI-CORE-026",
    message: "Secure credential ingress is unavailable.",
  },
  capabilities: {
    projectOnboarding: true,
    credentialHandles: true,
    laneLifecycle: true,
  },
};

function setup(
  projection: D11IntakeProjection = EMPTY_PROJECTION,
  poll?: () => Promise<D11IntentResult>,
  reviewStarterQueue = vi.fn<(queue: readonly D4StarterSeed[]) => void>(),
  renderPendingPermission = vi.fn<(host: HTMLElement) => void>(),
  onExitToCockpit = vi.fn<() => void>(),
  recentWork?: D11RecentWorkPort,
) {
  document.body.innerHTML = '<main id="app"></main>';
  const root = document.querySelector<HTMLElement>("#app");
  if (!root) throw new Error("missing test root");
  const send = vi.fn(async (): Promise<D11IntentResult> => ({
    projection,
    pendingCommandId: null,
    pendingIntent: null,
  }));
  const screen = renderD11Intake(
    root,
    projection,
    send,
    "en",
    poll,
    undefined,
    reviewStarterQueue,
    renderPendingPermission,
    onExitToCockpit,
    recentWork,
  );
  return {
    root,
    screen,
    send,
    reviewStarterQueue,
    renderPendingPermission,
    onExitToCockpit,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe("D11 project intake", () => {
  beforeEach(() => {
    document.documentElement.lang = "en";
  });

  test("autofocuses the no-project input and hides cockpit chrome before work", () => {
    const { root } = setup();

    const input = root.querySelector<HTMLInputElement>("[data-project-path]");
    expect(input).not.toBeNull();
    expect(document.activeElement).toBe(input);
    expect(root.querySelector("[data-cockpit-activity-rail]")).toBeNull();
    expect(root.querySelector("[data-cockpit-lane-rail]")).toBeNull();
    expect(root.querySelector("[data-cockpit-transcript]")).toBeNull();
    expect(root.querySelector("[data-cockpit-environment]")).toBeNull();
  });

  test("renders detected project mode and a nonblocking provider warning from Core facts", () => {
    const { root } = setup({
      ...EMPTY_PROJECTION,
      project: {
        root: "/workspace/demo",
        isGitRepository: true,
        configState: "missing",
        projectName: "demo",
        mode: "rust",
        diagnostics: [],
      },
      provider: {
        providerId: "deepseek",
        model: "deepseek-chat",
        status: "credential_locked",
        warning: true,
      },
    });

    expect(root.textContent).toContain("/workspace/demo");
    expect(root.textContent).toContain("rust");
    expect(root.querySelector("[data-provider-warning]")).not.toBeNull();
    expect(root.querySelector<HTMLInputElement>("[data-project-path]")?.disabled).toBe(false);
  });

  test("shows the exact Core preview and sends confirm separately", async () => {
    const { root, send } = setup({
      ...EMPTY_PROJECTION,
      preview: {
        previewId: "preview-1",
        relativePath: "viden.toml",
        contentSha256: "a".repeat(64),
        exactContents: '[project]\nname = "demo"\n',
        valid: true,
        diagnostics: [],
      },
    });

    expect(root.querySelector("[data-config-preview]")?.textContent).toContain(
      '[project]\nname = "demo"',
    );
    send.mockResolvedValue({
      projection: EMPTY_PROJECTION,
      pendingCommandId: "confirm-pending",
      pendingIntent: "confirm_project_config",
    });
    const confirm = root.querySelector<HTMLButtonElement>("[data-confirm-config]");
    expect(confirm?.disabled).toBe(false);
    confirm?.click();
    await Promise.resolve();

    expect(send).toHaveBeenCalledWith({ type: "confirm_project_config" });
    expect(root.querySelector("[data-confirm-state]")?.textContent).toContain("Waiting for Core");
  });

  test("re-renders authoritative Core facts returned after an intent", async () => {
    const updated: D11IntakeProjection = {
      ...EMPTY_PROJECTION,
      preview: {
        previewId: "preview-after-event",
        relativePath: "viden.toml",
        contentSha256: "b".repeat(64),
        exactContents: '[project]\nname = "event-confirmed"\n',
        valid: true,
        diagnostics: [],
      },
    };
    const { root, send } = setup();
    send.mockResolvedValue({
      projection: updated,
      pendingCommandId: null,
      pendingIntent: null,
    });

    root.querySelector<HTMLButtonElement>("[data-preview-config]")?.click();
    await vi.waitFor(() => {
      expect(root.querySelector("[data-config-preview]")?.textContent).toContain(
        "event-confirmed",
      );
    });
  });

  test("cancel returns to the cockpit without sending a mutation", () => {
    const { root, send, onExitToCockpit } = setup();
    const input = root.querySelector<HTMLInputElement>("[data-project-path]");
    if (!input) throw new Error("missing project input");
    input.value = "/tmp/demo";
    input.dispatchEvent(new Event("input", { bubbles: true }));

    root.querySelector<HTMLButtonElement>("[data-cancel-intake]")?.click();

    expect(send).not.toHaveBeenCalled();
    expect(onExitToCockpit).toHaveBeenCalledTimes(1);
  });

  test("preserves cockpit navigation after Core re-renders the intake", async () => {
    const { root, send, onExitToCockpit } = setup();
    send.mockResolvedValue({
      projection: EMPTY_PROJECTION,
      pendingCommandId: null,
      pendingIntent: null,
    });

    root.querySelector<HTMLButtonElement>("[data-preview-config]")?.click();
    await vi.waitFor(() => {
      expect(send).toHaveBeenCalledTimes(1);
    });
    root.querySelector<HTMLButtonElement>("[data-cancel-intake]")?.click();

    expect(onExitToCockpit).toHaveBeenCalledTimes(1);
  });

  test("routes selected starter presets to D4 in stable review order without a Core mutation", () => {
    const reviewStarterQueue = vi.fn<(queue: readonly D4StarterSeed[]) => void>();
    const { root, send } = setup({
      ...EMPTY_PROJECTION,
      confirmedConfig: {
        previewId: "preview-1",
        relativePath: "viden.toml",
        contentSha256: "a".repeat(64),
      },
    }, undefined, reviewStarterQueue);

    root.querySelector<HTMLInputElement>('[data-starter-preset="tester"]')?.click();
    root.querySelector<HTMLInputElement>('[data-starter-preset="reviewer"]')?.click();

    root.querySelector<HTMLButtonElement>("[data-create-starter-lane]")?.click();

    expect(send).not.toHaveBeenCalled();
    expect(reviewStarterQueue).toHaveBeenCalledWith([
      { laneId: "starter-coder", preset: "coder", branch: null, worktreePath: null },
      { laneId: "starter-reviewer", preset: "reviewer", branch: null, worktreePath: null },
      { laneId: "starter-tester", preset: "tester", branch: null, worktreePath: null },
    ]);
  });

  test("opens D4 even when the reviewed starter capability is absent so D4 can explain the gate", () => {
    const reviewStarterQueue = vi.fn<(queue: readonly D4StarterSeed[]) => void>();
    const { root, send } = setup({
      ...EMPTY_PROJECTION,
      confirmedConfig: {
        previewId: "preview-1",
        relativePath: "viden.toml",
        contentSha256: "a".repeat(64),
      },
      capabilities: {
        ...EMPTY_PROJECTION.capabilities,
        starterLanePreview: false,
      },
    }, undefined, reviewStarterQueue);

    root.querySelector<HTMLButtonElement>("[data-create-starter-lane]")?.click();

    expect(send).not.toHaveBeenCalled();
    expect(reviewStarterQueue).toHaveBeenCalledTimes(1);
  });

  test("disables every D11 command synchronously before the first send promise resolves", async () => {
    const pending = deferred<D11IntentResult>();
    const { root, send } = setup({
      ...EMPTY_PROJECTION,
      preview: {
        previewId: "preview-1",
        relativePath: "viden.toml",
        contentSha256: "a".repeat(64),
        exactContents: '[project]\nname = "demo"\n',
        valid: true,
        diagnostics: [],
      },
    });
    send.mockReturnValueOnce(pending.promise);

    root.querySelector<HTMLButtonElement>("[data-preview-config]")?.click();
    root.querySelector<HTMLButtonElement>("[data-confirm-config]")?.click();

    expect(send).toHaveBeenCalledTimes(1);
    expect(root.querySelector<HTMLButtonElement>("[data-probe-project]")?.disabled).toBe(true);
    expect(root.querySelector<HTMLButtonElement>("[data-preview-config]")?.disabled).toBe(true);
    expect(root.querySelector<HTMLButtonElement>("[data-confirm-config]")?.disabled).toBe(true);

    pending.resolve({
      projection: EMPTY_PROJECTION,
      pendingCommandId: "preview-pending",
      pendingIntent: "preview_project_config",
    });
    await pending.promise;
    await Promise.resolve();
  });

  test("polls late Core facts and re-renders intermediate provider facts while pending", async () => {
    vi.useFakeTimers();
    const providerProjection: D11IntakeProjection = {
      ...EMPTY_PROJECTION,
      provider: {
        providerId: "deepseek",
        model: "deepseek-chat",
        status: "offline",
        warning: true,
      },
    };
    const completedProjection: D11IntakeProjection = {
      ...providerProjection,
      preview: {
        previewId: "late-preview",
        relativePath: "viden.toml",
        contentSha256: "c".repeat(64),
        exactContents: '[project]\nname = "late"\n',
        valid: true,
        diagnostics: [],
      },
    };
    const poll = vi
      .fn<() => Promise<D11IntentResult>>()
      .mockResolvedValueOnce({
        projection: providerProjection,
        pendingCommandId: "preview-late",
        pendingIntent: "preview_project_config",
      })
      .mockResolvedValueOnce({
        projection: completedProjection,
        pendingCommandId: null,
        pendingIntent: null,
      });
    const { root, send } = setup(EMPTY_PROJECTION, poll);
    send.mockResolvedValue({
      projection: EMPTY_PROJECTION,
      pendingCommandId: "preview-late",
      pendingIntent: "preview_project_config",
    });

    root.querySelector<HTMLButtonElement>("[data-preview-config]")?.click();
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(250);

    expect(root.querySelector("[data-provider-warning]")?.textContent).toContain("offline");
    expect(root.querySelector("[data-preview-state]")?.textContent).toContain(
      "Waiting for Core",
    );
    expect(root.querySelector<HTMLButtonElement>("[data-preview-config]")?.disabled).toBe(true);

    await vi.advanceTimersByTimeAsync(250);
    expect(root.querySelector("[data-config-preview]")?.textContent).toContain('name = "late"');
    expect(poll).toHaveBeenCalledTimes(2);
    vi.useRealTimers();
  });

  test("keeps draft and pending state across a transient poll error then recovers on a late fact", async () => {
    vi.useFakeTimers();
    const completedProjection: D11IntakeProjection = {
      ...EMPTY_PROJECTION,
      preview: {
        previewId: "late-preview",
        relativePath: "viden.toml",
        contentSha256: "c".repeat(64),
        exactContents: '[project]\nname = "late"\n',
        valid: true,
        diagnostics: [],
      },
    };
    const poll = vi
      .fn<() => Promise<D11IntentResult>>()
      .mockRejectedValueOnce(new Error("temporary poll failure"))
      .mockResolvedValueOnce({
        projection: completedProjection,
        pendingCommandId: null,
        pendingIntent: null,
      });
    const { root, send, screen } = setup(EMPTY_PROJECTION, poll);
    const draft = root.querySelector<HTMLTextAreaElement>("[data-config-draft]");
    if (!draft) throw new Error("missing config draft");
    draft.value = '[project]\nname = "edited"\n';
    draft.dispatchEvent(new Event("input", { bubbles: true }));
    send.mockResolvedValue({
      projection: EMPTY_PROJECTION,
      pendingCommandId: "preview-late",
      pendingIntent: "preview_project_config",
    });

    root.querySelector<HTMLButtonElement>("[data-preview-config]")?.click();
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(250);

    expect(root.querySelector("[data-preview-state]")?.textContent).toContain(
      "temporary poll failure",
    );
    expect(screen.draft.configContents).toContain('name = "edited"');
    expect(root.querySelector<HTMLButtonElement>("[data-preview-config]")?.disabled).toBe(true);

    await vi.advanceTimersByTimeAsync(500);
    expect(root.querySelector("[data-config-preview]")?.textContent).toContain('name = "late"');
    expect(root.querySelector<HTMLTextAreaElement>("[data-config-draft]")?.value).toContain(
      'name = "edited"',
    );
    vi.useRealTimers();
  });

  test("restores controls when send fails before Core registers pending state", async () => {
    const { root, send } = setup({
      ...EMPTY_PROJECTION,
      preview: {
        previewId: "preview-1",
        relativePath: "viden.toml",
        contentSha256: "a".repeat(64),
        exactContents: '[project]\nname = "demo"\n',
        valid: true,
        diagnostics: [],
      },
    });
    send.mockRejectedValueOnce(new Error("send failed"));

    root.querySelector<HTMLButtonElement>("[data-confirm-config]")?.click();

    await vi.waitFor(() => {
      expect(root.querySelector("[data-confirm-state]")?.textContent).toContain("send failed");
    });
    expect(root.querySelector<HTMLButtonElement>("[data-probe-project]")?.disabled).toBe(false);
    expect(root.querySelector<HTMLButtonElement>("[data-preview-config]")?.disabled).toBe(false);
    expect(root.querySelector<HTMLButtonElement>("[data-confirm-config]")?.disabled).toBe(false);
  });

  test("shows only masked credential handles and keeps raw secret ingress disabled", () => {
    const { root } = setup({
      ...EMPTY_PROJECTION,
      credentialHandles: [
        {
          providerId: "deepseek",
          maskedHandle: "ke••••ry",
          status: "locked",
        },
      ],
    });

    expect(root.textContent).toContain("ke••••ry");
    expect(root.textContent).not.toContain("keychain:deepseek-primary");
    expect(root.querySelector('input[type="password"]')).toBeNull();
    expect(root.querySelector<HTMLButtonElement>("[data-credential-ingress]")?.disabled).toBe(true);
    expect(root.textContent).toContain("Project switching waits for a Core-owned bootstrap channel");
    expect(root.textContent).not.toContain("GUI-CORE-");
  });

  test("renders Core approval and rejection facts without inferring success", () => {
    const { root, renderPendingPermission } = setup({
      ...EMPTY_PROJECTION,
      pendingApproval: {
        id: "approval-config-1",
        title: "Confirm project config",
      },
      lastError: "command confirm-1 rejected: permission denied",
    });

    expect(root.querySelector("[data-core-approval]")?.textContent).toContain(
      "Confirm project config",
    );
    expect(root.querySelector("[data-core-error]")?.textContent).toContain(
      "permission denied",
    );
    const permissionHost = root.querySelector<HTMLElement>("[data-d11-permission-host]");
    expect(permissionHost).not.toBeNull();
    expect(renderPendingPermission).toHaveBeenCalledWith(permissionHost);
  });

  test("renders Core recent-work rows through the shared read port", async () => {
    const loaded: RecentWorkResult = {
      outcome: { state: "confirmed", reason: null },
      projects: [
        {
          canonicalRoot: "/workspace/viden",
          displayName: "viden",
          lastUpdatedAt: 1_767_222_000,
          latestSessionId: "session-viden",
        },
      ],
      sessions: [
        {
          canonicalRoot: "/workspace/viden",
          sessionId: "session-viden",
          createdAt: 1_767_221_000,
          lastUpdatedAt: 1_767_222_000,
          messageCount: 4,
          toolCallCount: 1,
          commandCount: 2,
        },
      ],
      diagnostics: ["skipped 1 legacy record"],
      pendingCommandId: null,
      capabilityAvailable: true,
    };
    const load = vi.fn(async () => loaded);
    const { root } = setup(EMPTY_PROJECTION, undefined, undefined, undefined, undefined, {
      load,
      now: () => 1_767_225_600_000,
    });

    const recent = root.querySelector<HTMLElement>(".d11-recent");
    expect(recent?.dataset.recentState).toBe("loading");
    await Promise.resolve();
    await Promise.resolve();
    expect(recent?.dataset.recentState).toBe("loaded");
    const row = recent?.querySelector<HTMLElement>('[data-recent-project="/workspace/viden"]');
    expect(row?.textContent).toContain("viden");
    expect(row?.textContent).toContain("1 session");
    expect(recent?.textContent).toContain("skipped 1 legacy record");
    expect(load).toHaveBeenCalledTimes(1);
  });

  test("reports an empty inventory as empty, not as unavailable", async () => {
    const load = vi.fn(
      async (): Promise<RecentWorkResult> => ({
        outcome: { state: "confirmed", reason: null },
        projects: [],
        sessions: [],
        diagnostics: [],
        pendingCommandId: null,
        capabilityAvailable: true,
      }),
    );
    const { root } = setup(EMPTY_PROJECTION, undefined, undefined, undefined, undefined, {
      load,
    });

    await Promise.resolve();
    await Promise.resolve();
    const recent = root.querySelector<HTMLElement>(".d11-recent");
    expect(recent?.dataset.recentState).toBe("loaded");
    expect(recent?.textContent).toContain("No recent projects yet");
  });

  test("names the missing capability instead of claiming an empty history", () => {
    const load = vi.fn(async (): Promise<RecentWorkResult> => {
      throw new Error("must not be read without the capability");
    });
    const { root } = setup(
      {
        ...EMPTY_PROJECTION,
        recentWork: {
          available: false,
          code: "capability_missing",
          message: "Core did not publish runtime.recent_work; recent history is unavailable.",
        },
      },
      undefined,
      undefined,
      undefined,
      undefined,
      { load },
    );

    const recent = root.querySelector<HTMLElement>(".d11-recent");
    expect(recent?.dataset.recentState).toBe("unavailable");
    expect(recent?.dataset.recentCode).toBe("capability_missing");
    expect(recent?.textContent).toContain("runtime.recent_work");
    expect(load).not.toHaveBeenCalled();
  });

  test("reports a rejected recent-work read in Core's own words", async () => {
    const load = vi.fn(
      async (): Promise<RecentWorkResult> => ({
        outcome: { state: "rejected", reason: "inventory rebuild failed" },
        projects: [],
        sessions: [],
        diagnostics: [],
        pendingCommandId: null,
        capabilityAvailable: true,
      }),
    );
    const { root } = setup(EMPTY_PROJECTION, undefined, undefined, undefined, undefined, {
      load,
    });

    await Promise.resolve();
    await Promise.resolve();
    const recent = root.querySelector<HTMLElement>(".d11-recent");
    expect(recent?.dataset.recentState).toBe("failed");
    expect(recent?.textContent).toContain("inventory rebuild failed");
  });
});
