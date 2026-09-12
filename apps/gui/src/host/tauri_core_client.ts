import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import type { CoreClient } from "./core_client";

/**
 * The Tauri implementation of the host boundary. This file is the only place
 * in the frontend allowed to import `@tauri-apps/*`; everything else consumes
 * the transport-neutral CoreClient interface.
 */
export function createTauriCoreClient(): CoreClient {
  // Only the desktop host publishes the wake. In a browser harness the event
  // bridge is absent, so the cockpit keeps its bounded long poll instead of
  // throwing on a subscription that cannot exist.
  const nativeShell = "__TAURI_INTERNALS__" in window;

  return {
    resolvedPreferences: () => invoke("resolved_preferences"),

    preferencesAvailable: () => invoke("preferences_available"),
    preferencesSave: (commandId, patch) => invoke("preferences_save", { commandId, patch }),
    preferencesRestore: (commandId) => invoke("preferences_restore", { commandId }),
    preferencesPoll: () => invoke("preferences_poll"),

    layoutPreferences: () => invoke("layout_preferences"),
    layoutPreferencesSet: (commandId, patch) =>
      invoke("layout_preferences_set", { commandId, patch }),
    layoutPreferencesReset: (commandId) => invoke("layout_preferences_reset", { commandId }),
    layoutPreferencesPoll: () => invoke("layout_preferences_poll"),

    queryRecentWork: (commandId, limit) => invoke("query_recent_work", { commandId, limit }),
    recentWorkPoll: () => invoke("recent_work_poll"),

    queryWorkspaceFiles: (commandId, prefix) =>
      invoke("query_workspace_files", { commandId, prefix: prefix ?? null }),
    workspaceFilesPoll: () => invoke("workspace_files_poll"),

    queryWorkspaceDiff: (commandId, laneId) =>
      invoke("query_workspace_diff", { commandId, laneId }),
    workspaceDiffPoll: () => invoke("workspace_diff_poll"),
    workspaceDiff: () => invoke("workspace_diff"),

    runOperatorGitAction: (commandId, laneId, action) =>
      invoke("run_operator_git_action", { commandId, laneId, action }),
    operatorGitPoll: (laneId) => invoke("operator_git_poll", { laneId }),
    operatorGit: (laneId) => invoke("operator_git", { laneId }),

    queryEvidence: (commandId, laneId, kinds) =>
      invoke("query_evidence", { commandId, laneId, kinds }),
    evidenceLoadOlder: (commandId) => invoke("evidence_load_older", { commandId }),
    evidencePoll: () => invoke("evidence_poll"),
    evidenceArchive: () => invoke("evidence_archive"),
    readEvidenceContent: (commandId, evidenceId) =>
      invoke("read_evidence_content", { commandId, evidenceId }),
    evidenceContentPoll: () => invoke("evidence_content_poll"),

    d1Cockpit: (selectedLaneId) => invoke("d1_cockpit", { selectedLaneId }),
    d1SendIntent: (commandId, intent) => invoke("d1_send_intent", { commandId, intent }),
    d1Poll: (selectedLaneId, waitForEvent) =>
      invoke("d1_poll", { selectedLaneId, waitForEvent }),

    setWorkMode: (commandId, mode, selectedLaneId) =>
      invoke("set_work_mode", { commandId, mode, selectedLaneId }),
    setPermissionLevel: (commandId, level, selectedLaneId) =>
      invoke("set_permission_level", { commandId, level, selectedLaneId }),
    selectModel: (commandId, providerId, model, selectedLaneId) =>
      invoke("select_model", { commandId, providerId, model, selectedLaneId }),

    d11SendIntent: (commandId, intent) => invoke("d11_send_intent", { commandId, intent }),
    d11Poll: () => invoke("d11_poll"),

    d4SendIntent: (commandId, intent) => invoke("d4_send_intent", { commandId, intent }),
    d4Poll: () => invoke("d4_poll"),

    d2Decisions: (selectedId) => invoke("d2_decisions", { selectedId }),
    d2SendIntent: (commandId, intent) => invoke("d2_send_intent", { commandId, intent }),

    d10LaneMonitor: () => invoke("d10_lane_monitor"),
    d10Events: (commandId) => invoke("d10_events", { commandId }),
    d10EventsPoll: () => invoke("d10_events_poll"),
    d12IntegrationGate: (selectedGateId) => invoke("d12_integration_gate", { selectedGateId }),
    d12SendIntent: (commandId, intent) => invoke("d12_send_intent", { commandId, intent }),
    d13FleetWorkflow: () => invoke("d13_fleet_workflow"),
    d14AuditTimeline: (after, limit) => invoke("d14_audit_timeline", { after, limit }),
    d14AuditQuery: (commandId, scope) => invoke("d14_audit_query", { commandId, scope }),
    d14AuditLoadOlder: (commandId) => invoke("d14_audit_load_older", { commandId }),
    d14AuditPoll: () => invoke("d14_audit_poll"),

    permissionSendIntent: (commandId, intent) =>
      invoke("permission_send_intent", { commandId, intent }),
    d6Recover: () => invoke("d6_recover"),
    d6SendIntent: (commandId, intent) => invoke("d6_send_intent", { commandId, intent }),

    agentContent: (reference) => invoke("agent_content", { reference }),

    openWorkspace: (root) => invoke("open_workspace", { root }),
    pickProjectFolder: async (title) => {
      const selected = await open({ directory: true, multiple: false, title });
      return typeof selected === "string" ? selected : null;
    },

    // The desktop host drains ordered Core events off the UI thread and emits
    // this wake, so the cockpit reads on a push instead of holding a drain
    // timer. The unlisten is awaited lazily; dispose only needs the handler to
    // stop firing.
    onCoreWake: !nativeShell
      ? undefined
      : (handler) => {
          const pending = listen("viden://core-advanced", () => handler());
          let stopped = false;
          void pending.then((unlisten) => {
            if (stopped) unlisten();
          });
          return () => {
            stopped = true;
            void pending.then((unlisten) => unlisten());
          };
        },
  };
}
