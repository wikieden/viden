import type { Locale } from "../i18n/catalog";
import type { DiffDocumentProjection } from "./diff_review";
import type { D1TranscriptRow } from "./transcript";

export type PermissionChoice =
  | "once"
  | "session"
  | "repo_allowlist"
  | "always"
  | "edit"
  | "deny";

export interface PermissionDockProjection {
  workMode: string;
  permissionLevel: string;
  request: {
    id: string;
    toolName: string;
    title: string;
    message: string;
    inputPreview: string;
    isMutating: boolean;
    reason: string | null;
    risk: string;
    target: { kind: string; display: string; canonicalRef: string | null };
    policyReasonKey: string;
    policyReasonArgs: Record<string, string>;
    expiresAt: number;
    defaultAction: string;
    auditId: string;
    blockedByPlan: boolean;
    /**
     * What Core knows about the change this approval would make
     * (`runtime.structured_diff`, GUI-CORE-012).
     *
     * `null` for every approval Core attached no context to — the ordinary
     * case for `shell` and the `git_*` family, which Core cannot preview
     * without running them. The dock then renders `inputPreview` alone, with
     * the exact wording it has always had.
     */
    decisionContext?: {
      /** `null` means Core computed no diff — never "no change". */
      diff: DiffDocumentProjection | null;
      /**
       * SHA-256 of the bytes a single-file preview was computed against.
       * `null` for a multi-file patch: one hash cannot describe several files.
       */
      baseSha256: string | null;
    } | null;
    actions: Array<{
      kind: PermissionChoice;
      available: boolean;
      sessionId: string | null;
      paths: string[];
      code: string | null;
    }>;
  } | null;
}

export type D6State =
  | "live"
  | "empty"
  | "connecting"
  | "disconnected"
  | "provider_error"
  | "agent_stopped"
  | "context_overflow"
  | "gate_queue_clear"
  | "incompatible_schema"
  | "missing_feature_capability"
  | "event_gap";

export interface D6ActionProjection {
  kind: string;
  available: boolean;
  code: string;
  /**
   * Exact Core ids the action targets, when Core published them. The intent
   * replays these instead of rebuilding an identity from display text, so an
   * action is inert unless Core named something for it to act on.
   */
  sessionId?: string | null;
  laneId?: string | null;
}

export interface D6RecoveryProjection {
  connection: "disconnected" | "connecting" | "live" | "recovering" | "incompatible";
  state: D6State;
  detail: string | null;
  hint: string | null;
  recoverable: boolean;
  businessSuccessBlocked: boolean;
  usedTokens: number | null;
  hardTokenLimit: number | null;
  missingCapabilities: string[];
  actions: D6ActionProjection[];
}

/**
 * One D6 recovery action routed to the Core command that owns it. `inspect` is
 * absent on purpose: it expands facts the projection already carries.
 */
export type D6Intent =
  | { kind: "restart"; sessionId: string }
  | { kind: "close_lane"; laneId: string };

export interface D6IntentResult {
  projection: D6RecoveryProjection;
  pendingCommandId: string | null;
}

export interface WorkspaceSourceProjection {
  status: "ready" | "unavailable" | "truncated";
  branch: string | null;
  worktree: string | null;
  ahead: number;
  behind: number;
  added: number;
  deleted: number;
  dirty: boolean;
}

/**
 * Titlebar source-control facts, computed by the host from the confirmed Core
 * view. `null` on the cockpit projection means Core published no workspace
 * source, or one it could not sample: the titlebar then omits the git block
 * rather than rendering zeroes as a clean, in-sync tree.
 *
 * Read-only. frontend-contract-v1 carries no operator git command, so nothing
 * here backs a commit, push, or sync action (`GUI-CORE-020`).
 */
export interface TopbarSourceProjection {
  /** Project name Core published; `null` when Core named none. */
  project: string | null;
  branch: string | null;
  ahead: number;
  behind: number;
  dirty: boolean;
  status: "ready" | "truncated";
  /** Core sampled only part of the workspace; the counts are partial. */
  truncated: boolean;
  /** Distinct worktrees across the project's active Lanes. */
  laneWorktreeCount: number;
}

export interface ContextUsageProjection {
  budgetId: string;
  usedTokens: number;
  softTokenLimit: number;
  hardTokenLimit: number;
  remainingTokens: number;
  exceeded: boolean;
}

export interface LaneAgentProjection {
  laneId: string;
  workspaceId: string;
  projectId: string;
  sessionId: string | null;
  taskId: string | null;
  turnId: string | null;
}

export interface ProviderHealthProjection {
  providerId: string;
  model: string;
  status: string;
  requestCount: number;
  errorCount: number;
  lastLatencyMs: number | null;
  averageLatencyMs: number | null;
  tokensPerSecond: number | null;
}

export interface RuntimeServiceProjection {
  id: string;
  kind: "mcp" | "lsp";
  label: string;
  status: "connected" | "ready" | "degraded" | "offline" | "unavailable";
  detailKey: string | null;
}

export interface ChecklistItemProjection {
  id: string;
  kind: "workspace_change" | "check_run";
  label: string;
  status: string;
  command: string | null;
  path: string | null;
  summary: string | null;
  /** Typed WorkspaceChange patch; absent facts must render as unavailable. */
  patch?: string | null;
  /**
   * The same change as typed rows, when Core published them. Two views of one
   * Core computation, so exactly one of them is drawn: the rows when they
   * exist, the patch string otherwise.
   */
  diff?: DiffDocumentProjection | null;
  failingLocation: string | null;
  additions: number | null;
  deletions: number | null;
}

export interface ContextDockProjection {
  source: WorkspaceSourceProjection | null;
  context: ContextUsageProjection | null;
  laneAgent: LaneAgentProjection | null;
  provider: ProviderHealthProjection | null;
  services: RuntimeServiceProjection[];
  checklist: ChecklistItemProjection[];
}

/**
 * Window-level statusbar facts computed by the host from the confirmed Core
 * view. A `null` segment means Core published no fact for it; the renderer
 * shows an explicit placeholder instead of a fabricated number.
 */
export interface D1StatusbarProjection {
  workMode: string;
  permissionLevel: string;
  context: { usedTokens: number; hardTokenLimit: number; exceeded: boolean } | null;
  /**
   * Ordered event-stream position of the confirmed snapshot (the adapter's
   * replay cursor sequence). Not an event counter — Core publishes none.
   */
  eventStreamPosition: number;
  lane: {
    laneId: string;
    agentId: string | null;
    status: string;
    progress: number | null;
  } | null;
  latency: { lastLatencyMs: number | null; averageLatencyMs: number | null } | null;
  tokens: { inputTokens: number; outputTokens: number } | null;
  diagnosticsCount: number;
  requests: { requestCount: number; errorCount: number } | null;
  /**
   * Decisions Core is holding for a human — the number behind the statusbar's
   * `⏸` segment and the activity rail's D2 badge.
   *
   * `null` is "Core published no count", which the shell's pre-connection
   * placeholder projection carries. It is deliberately distinct from `0`
   * ("nothing is waiting"): rendering the placeholder as a zero would tell the
   * operator their queue is empty before anything has been counted.
   */
  pendingGateCount: number | null;
}

export interface D1CockpitProjection {
  preferences: {
    locale: Locale;
    skin: string;
    mode: string;
    density: string;
    motion: string;
    diagnostics: unknown[];
  };
  selectedLaneId: string | null;
  topbarSource: TopbarSourceProjection | null;
  contextDock: ContextDockProjection;
  lanes: Array<{
    id: string;
    role: string;
    status: string;
    summary: string;
    branch: string | null;
  }>;
  environment: {
    cwd: string;
    providerId: string;
    model: string;
    workMode: string;
    permissionLevel: string;
    tokenTotal: number;
    costMicroUsd: number | null;
  };
  liveWork: {
    tasks: Array<{ id: string; title: string; status: string; progress: number }>;
    tools: Array<{ id: string; name: string; inputPreview: string; state: string }>;
    approvals: Array<{ id: string; title: string; risk: string }>;
    queuedInputs: Array<{ id: string; contentPreview: string }>;
    evidence: Array<{ id: string; kind: string; summary: string; path: string | null }>;
  };
  transcript: D1TranscriptRow[];
  workspaceEligibility: {
    isGitRepository: boolean;
    hasHead: boolean;
    canCreateLane: boolean;
    diagnostic: string | null;
  } | null;
  starterLanePreviews: Array<{
    previewId: string;
    contentSha256: string;
    laneId: string;
    branch: string | null;
    diagnostics: string[];
  }>;
  agentAdapters: Array<{
    agentId: string;
    displayName: string;
    startability: string;
    diagnostics: string[];
    /** Model options Core published for this adapter; absent or empty when Core published none. */
    models?: string[];
  }>;
  agentSessions: Array<{
    sessionId: string;
    laneId: string;
    agentId: string;
    model: string | null;
    status: string;
    task: string;
    diagnostic: string | null;
    output?: string | null;
    conversation?: Array<{
      messageId: string;
      role: "user" | "assistant";
      content: string;
      /// Typed content Core published with the message. Absent when Core
      /// published none; the client never synthesizes a part.
      parts?: {
        kind: string;
        mediaType: string | null;
        reference: string | null;
        text: string | null;
        label: string | null;
      }[];
    }>;
  }>;
  composer: {
    editable: boolean;
    busy: boolean;
    canCancel: boolean;
    canSubmitImmediately: boolean;
  };
  statusbar: D1StatusbarProjection;
  permissionDock: PermissionDockProjection;
  recovery: D6RecoveryProjection;
  unavailableFeatures: Array<{
    id: string;
    available: false;
    code: string;
    message: string;
  }>;
}

/**
 * The label the titlebar, the rail group, and the picker all use for the open
 * project: Core's published project name when it has one, otherwise the
 * workspace path Core opened. A name is never derived from the path.
 */
export function currentProjectLabel(projection: D1CockpitProjection): string {
  return projection.topbarSource?.project ?? projection.environment.cwd;
}

/**
 * Lane statuses that represent no work in flight.
 *
 * Counting the *terminal* set rather than the running one is deliberate: an
 * unrecognized status then counts as active, which errs toward warning the
 * operator that a workspace switch will interrupt something.
 */
const SETTLED_LANE_STATUSES = new Set(["draft", "detached", "done", "failed", "cancelled"]);

/** Agent session statuses Core treats as finished. */
const SETTLED_SESSION_STATUSES = new Set(["completed", "failed", "cancelled"]);

/**
 * How much running work a workspace switch tears down.
 *
 * `LocalCoreHost::open_workspace` builds a new supervisor and the desktop host
 * swaps its single adapter slot, so dropping the old supervisor joins its
 * worker and shuts down every resident ACP session. These counts are what the
 * switch confirmation states before the operator commits.
 */
export function activeWorkCounts(projection: D1CockpitProjection): {
  lanes: number;
  sessions: number;
} {
  return {
    lanes: projection.lanes.filter((lane) => !SETTLED_LANE_STATUSES.has(lane.status)).length,
    sessions: projection.agentSessions.filter(
      (session) => !SETTLED_SESSION_STATUSES.has(session.status),
    ).length,
  };
}
