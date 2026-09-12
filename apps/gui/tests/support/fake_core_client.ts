/**
 * A frontend-safe `CoreClient` double for shell-level routing specs.
 *
 * Every command is unreachable by default: a route spec that reaches an
 * unexpected Core command fails naming it, rather than passing because a stub
 * quietly answered. Tests override exactly the reads the route under test is
 * allowed to make.
 */

import { IDLE_LAYOUT_PREFERENCES } from "../../src/models/layout_preferences";
import type { CoreClient } from "../../src/host/core_client";
import type { D11IntakeProjection, D11IntentResult } from "../../src/screens/d11_intake";
import type { D4IntentResult } from "../../src/screens/d4_lane_create";
import { D1_PROJECTION } from "./d1_projection";

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

export const D11_RESULT: D11IntentResult = {
  projection: D11_PROJECTION,
  pendingCommandId: null,
  pendingIntent: null,
};

/**
 * The D4 wizard's entry state: the capability advertised, nothing previewed
 * yet. It is what `d4_poll` answers on a fresh entry, which is the state the
 * "Full setup…" route lands in.
 */
export const D4_RESULT: D4IntentResult = {
  projection: {
    availability: {
      available: true,
      capability: "runtime.starter_lane_preview",
      message: "Reviewed starter Lane creation is available.",
    },
    workMode: "build",
    canCreate: false,
    preview: null,
    receipt: null,
    pendingApproval: null,
    outcome: { state: "idle", reason: null, requiresRepreview: false },
    navigationLaneId: null,
  },
  pendingCommandId: null,
  pendingIntent: null,
};

export function fakeCoreClient(overrides: Partial<CoreClient> = {}): CoreClient {
  const unreachable = (command: string) => async () => {
    throw new Error(`unexpected CoreClient call: ${command}`);
  };
  return {
    resolvedPreferences: async () => PREFERENCES,
    preferencesAvailable: async () => true,
    preferencesSave: unreachable("preferences_save"),
    preferencesRestore: unreachable("preferences_restore"),
    preferencesPoll: unreachable("preferences_poll"),
    // The layout record answers with the honest absence: a shell route spec
    // must not depend on a persisted cockpit layout existing.
    layoutPreferences: async () => IDLE_LAYOUT_PREFERENCES,
    layoutPreferencesSet: unreachable("layout_preferences_set"),
    layoutPreferencesReset: unreachable("layout_preferences_reset"),
    layoutPreferencesPoll: unreachable("layout_preferences_poll"),
    queryRecentWork: unreachable("query_recent_work"),
    recentWorkPoll: unreachable("recent_work_poll"),
    queryWorkspaceFiles: unreachable("query_workspace_files"),
    workspaceFilesPoll: unreachable("workspace_files_poll"),
    queryWorkspaceDiff: unreachable("query_workspace_diff"),
    workspaceDiffPoll: unreachable("workspace_diff_poll"),
    workspaceDiff: unreachable("workspace_diff"),
    runOperatorGitAction: unreachable("run_operator_git_action"),
    operatorGitPoll: unreachable("operator_git_poll"),
    operatorGit: unreachable("operator_git"),
    // The evidence archive stays unbound in a shell test: the entry points ask
    // only for the no-traffic projection, which answers with the capability.
    queryEvidence: unreachable("query_evidence"),
    evidenceLoadOlder: unreachable("evidence_load_older"),
    evidencePoll: unreachable("evidence_poll"),
    evidenceArchive: async () => ({
      outcome: { state: "idle", reason: null },
      rows: [],
      nextAfter: null,
      complete: false,
      loaded: false,
      pendingCommandId: null,
      capabilityAvailable: false,
      stale: false,
      scopeLaneId: null,
      kinds: [],
    }),
    readEvidenceContent: unreachable("read_evidence_content"),
    evidenceContentPoll: unreachable("evidence_content_poll"),
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

