// @vitest-environment jsdom

// Proves the shell is host-agnostic: hydrate renders entirely from an injected
// CoreClient, with no @tauri-apps module mock anywhere in this file. A future
// Electron or web host only needs to supply the same interface.
import { beforeEach, describe, expect, test, vi } from "vitest";

import { hydrateShellFromCore } from "../src/main";
import type { CoreClient } from "../src/host/core_client";
import { D1_PROJECTION } from "./support/d1_projection";

const PREFERENCES = {
  locale: "en" as const,
  skin: "aurora" as const,
  mode: "dark" as const,
  density: "regular" as const,
  motion: "system" as const,
  diagnostics: [],
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
    d1SendIntent: unreachable("d1_send_intent"),
    d1Poll: async () => ({
      projection: D1_PROJECTION,
      pendingCommandId: null,
      outcome: { state: "idle", reason: null },
    }),
    setWorkMode: unreachable("set_work_mode"),
    setPermissionLevel: unreachable("set_permission_level"),
    selectModel: unreachable("select_model"),
    d11SendIntent: unreachable("d11_send_intent"),
    d11Poll: unreachable("d11_poll"),
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

describe("host-agnostic shell hydration", () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="app"></main>';
  });

  test("hydrates the connected D1 cockpit from an injected CoreClient", async () => {
    const root = document.querySelector<HTMLElement>("#app");
    if (!root) throw new Error("test root is missing");

    await hydrateShellFromCore(root, fakeCoreClient());

    expect(root.dataset.clientState).toBe("connected");
    expect(root.dataset.route).toBe("d1");
    expect(root.querySelector("[data-screen]")?.getAttribute("data-screen")).toBe("d1-cockpit");
  });

  test("falls back to the disconnected shell when the injected client fails", async () => {
    const root = document.querySelector<HTMLElement>("#app");
    if (!root) throw new Error("test root is missing");

    await hydrateShellFromCore(
      root,
      fakeCoreClient({
        resolvedPreferences: async () => {
          throw new Error("core is down");
        },
      }),
    );

    expect(root.dataset.clientState).toBe("disconnected");
    expect(root.querySelector("[data-d6-state]")?.getAttribute("data-d6-state")).toBe(
      "disconnected",
    );
  });

  test("routes the welcome open-project action through the injected client", async () => {
    const root = document.querySelector<HTMLElement>("#app");
    if (!root) throw new Error("test root is missing");

    const openWorkspace = vi.fn(async () => {});
    let cockpitCalls = 0;
    await hydrateShellFromCore(
      root,
      fakeCoreClient({
        // First read has no project: the welcome shell renders. After the
        // picker supplies a root, D1 re-reads the canonical projection.
        d1Cockpit: async () => (cockpitCalls++ === 0 ? null : D1_PROJECTION),
        pickProjectFolder: async () => "/tmp/demo-project",
        openWorkspace,
      }),
    );

    expect(root.dataset.clientState).toBe("empty");
    const openButton = root.querySelector<HTMLButtonElement>("[data-open-project]");
    expect(openButton).not.toBeNull();
    openButton?.click();
    await vi.waitFor(() => {
      expect(openWorkspace).toHaveBeenCalledWith("/tmp/demo-project");
      expect(root.dataset.clientState).toBe("connected");
    });
  });
});
