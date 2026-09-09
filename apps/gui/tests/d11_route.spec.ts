// @vitest-environment jsdom

// D11 is reachable from the shell. These specs drive `hydrateShellFromCore`
// through an injected CoreClient — no Tauri module mock — so they prove the
// route itself, not a host detail.

import { beforeEach, describe, expect, test, vi } from "vitest";

import { hydrateShellFromCore } from "../src/main";
import type { CoreClient } from "../src/host/core_client";
import type { D11IntakeProjection, D11IntentResult } from "../src/screens/d11_intake";
import { D1_PROJECTION } from "./support/d1_projection";

const PREFERENCES = {
  locale: "en" as const,
  skin: "aurora" as const,
  mode: "dark" as const,
  density: "regular" as const,
  motion: "system" as const,
  diagnostics: [],
};

const D11_PROJECTION: D11IntakeProjection = {
  project: null,
  preview: null,
  confirmedConfig: null,
  provider: null,
  credentialHandles: [],
  starterLanes: [],
  pendingApproval: null,
  lastError: null,
  // Unavailable keeps the route spec free of the recent-work read: the
  // section renders the named capability gap and never calls the port.
  recentWork: {
    available: false,
    code: "capability_missing",
    message: "Core did not publish runtime.recent_work; recent history is unavailable.",
  },
  credentialIngress: {
    available: false,
    code: "GUI-CORE-026",
    message: "Platform credential intake is unavailable.",
  },
  capabilities: {
    projectOnboarding: true,
    credentialHandles: true,
    laneLifecycle: true,
  },
};

const D11_RESULT: D11IntentResult = {
  projection: D11_PROJECTION,
  pendingCommandId: null,
  pendingIntent: null,
};

function fakeCoreClient(overrides: Partial<CoreClient> = {}): CoreClient {
  const unreachable = (command: string) => async () => {
    throw new Error(`unexpected CoreClient call: ${command}`);
  };
  return {
    resolvedPreferences: async () => PREFERENCES,
    preferencesAvailable: async () => true,
    preferencesSave: unreachable("preferences_save"),
    preferencesRestore: unreachable("preferences_restore"),
    preferencesPoll: unreachable("preferences_poll"),
    queryRecentWork: unreachable("query_recent_work"),
    recentWorkPoll: unreachable("recent_work_poll"),
    queryWorkspaceFiles: unreachable("query_workspace_files"),
    workspaceFilesPoll: unreachable("workspace_files_poll"),
    queryWorkspaceDiff: unreachable("query_workspace_diff"),
    workspaceDiffPoll: unreachable("workspace_diff_poll"),
    workspaceDiff: unreachable("workspace_diff"),
    d1Cockpit: async () => D1_PROJECTION,
    d1SendIntent: async () => ({
      projection: D1_PROJECTION,
      pendingCommandId: null,
      outcome: { state: "idle", reason: null },
    }),
    d1Poll: async () => ({
      projection: D1_PROJECTION,
      pendingCommandId: null,
      outcome: { state: "idle", reason: null },
    }),
    setWorkMode: unreachable("set_work_mode"),
    setPermissionLevel: unreachable("set_permission_level"),
    selectModel: unreachable("select_model"),
    d11SendIntent: unreachable("d11_send_intent"),
    d11Poll: async () => D11_RESULT,
    d4SendIntent: unreachable("d4_send_intent"),
    d4Poll: unreachable("d4_poll"),
    d2Decisions: unreachable("d2_decisions"),
    d2SendIntent: unreachable("d2_send_intent"),
    d10LaneMonitor: unreachable("d10_lane_monitor"),
    d10Events: unreachable("d10_events"),
    d10EventsPoll: unreachable("d10_events_poll"),
    d12IntegrationGate: unreachable("d12_integration_gate"),
    d12SendIntent: unreachable("d12_send_intent"),
    d13FleetWorkflow: unreachable("d13_fleet_workflow"),
    d14AuditTimeline: unreachable("d14_audit_timeline"),
    d14AuditQuery: unreachable("d14_audit_query"),
    d14AuditLoadOlder: unreachable("d14_audit_load_older"),
    d14AuditPoll: unreachable("d14_audit_poll"),
    permissionSendIntent: unreachable("permission_send_intent"),
    d6Recover: unreachable("d6_recover"),
    d6SendIntent: unreachable("d6_send_intent"),
    agentContent: unreachable("agent_content"),
    openWorkspace: unreachable("open_workspace"),
    pickProjectFolder: unreachable("pick_project_folder"),
    onCoreWake: undefined,
    ...overrides,
  };
}

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
