import type { PaletteWorkspaceFiles } from "../components/command_palette";
import type { PermissionIntent, PermissionIntentResult } from "../components/permission_dock";
import type { WorkspaceDiffProjection } from "../models/diff_review";
import type {
  OperatorGitActionRequest,
  OperatorGitProjection,
} from "../models/operator_git";
import type { RecentWorkResult } from "../models/recent_work";
import type { D6Intent, D6IntentResult, D6RecoveryProjection } from "../models/workspace";
import type {
  PreferenceIntentResult,
  PreferencePatch,
  ResolvedPreferences,
} from "../preferences";
import type { D1CockpitProjection, D1Intent, D1IntentResult } from "../screens/d1_cockpit";
import type { D10LaneMonitorProjection } from "../screens/d10_lane_monitor";
import type { D11Intent, D11IntentResult } from "../screens/d11_intake";
import type {
  D12IntegrationGateProjection,
  D12Intent,
  D12IntentResult,
} from "../screens/d12_integration_gate";
import type { D13FleetWorkflowProjection } from "../screens/d13_fleet_workflow";
import type {
  D14AuditProjection,
  D14AuditScope,
  D14AuditTimelineProjection,
} from "../screens/d14_audit_timeline";
import type { D2DecisionsProjection, D2Intent, D2IntentResult } from "../screens/d2_decisions";
import type { D4Intent, D4IntentResult } from "../screens/d4_lane_create";

/**
 * The complete host boundary of the GUI shell. Every Core read, Core command,
 * host event subscription, and native chrome interaction the frontend needs
 * flows through this interface, so the screens and the shell stay ignorant of
 * which desktop host (Tauri today, any other later) carries the transport.
 *
 * Methods mirror the Core-owned projection/intent contract one-to-one; the
 * interface must not grow GUI-private state or reducers.
 */
export interface CoreClient {
  resolvedPreferences(): Promise<ResolvedPreferences | null>;

  /**
   * Whether Core's handshake published `ui.preference_persistence`.
   *
   * The Settings panel reads this before it renders so an absent capability
   * shows as an honest read-only state instead of controls that cannot reach
   * Core — and never as a client-side write.
   */
  preferencesAvailable(): Promise<boolean>;
  /**
   * Sends one preference change as Core's `SetUiPreferences`. Only the axes in
   * `patch` are requested; Core owns persistence, the permission gate,
   * precedence, and the skin/mode rule.
   */
  preferencesSave(commandId: string, patch: PreferencePatch): Promise<PreferenceIntentResult>;
  /** Sends `ResetUiPreferences`, dropping the user `[ui]` table. */
  preferencesRestore(commandId: string): Promise<PreferenceIntentResult>;
  /** Drains ordered Core events while a preference command is still pending. */
  preferencesPoll(): Promise<PreferenceIntentResult>;

  /**
   * Sends Core's read-only `QueryRecentWork` and resolves with whatever the
   * ordered `RecentWorkLoaded` fact published. Core owns the scan, the
   * `1..=100` clamp, the ordering, and the whitelist DTOs; the frontend never
   * inspects the session home, a transcript, or the SQLite index.
   */
  queryRecentWork(commandId: string, limit: number): Promise<RecentWorkResult>;
  /** Drains ordered Core events while a recent-work read is still pending. */
  recentWorkPoll(): Promise<RecentWorkResult>;

  /**
   * Sends Core's `QueryWorkspaceFiles` and resolves with whatever the ordered
   * `WorkspaceFilesLoaded` page published. Core owns the permission gate, the
   * gitignore-aware walk, the runtime-state exclusions, the ordering, and the
   * page clamp; the frontend never walks the workspace, shells out to a file
   * lister, or reconstructs a tree from paths seen elsewhere (GUI-CORE-022).
   */
  queryWorkspaceFiles(commandId: string): Promise<PaletteWorkspaceFiles>;
  /** Drains ordered Core events while an inventory read is still pending. */
  workspaceFilesPoll(): Promise<PaletteWorkspaceFiles>;

  /**
   * Sends Core's read-only `QueryWorkspaceDiff` and resolves with whatever the
   * ordered `WorkspaceDiffLoaded` page published. Core owns the permission
   * gate (the non-mutating `git_diff` tool), the `git status`/`git diff` runs,
   * the byte bound, the `omitted`/`truncated` flags, and the ordering; the
   * frontend never shells out to git and never parses diff text into rows
   * (GUI-CORE-012).
   *
   * `laneId` names one Lane's worktree; `null` reads the workspace root. Core
   * resolves the worktree from its own records — a client never passes a path.
   */
  queryWorkspaceDiff(commandId: string, laneId: string | null): Promise<WorkspaceDiffProjection>;
  /** Drains ordered Core events while a diff read is still pending. */
  workspaceDiffPoll(): Promise<WorkspaceDiffProjection>;
  /**
   * The current diff projection with no Core traffic. The open review reads it
   * on each host wake to learn whether an ordered Core fact invalidated its
   * page, which is what schedules the debounced re-query.
   */
  workspaceDiff(): Promise<WorkspaceDiffProjection>;

  /**
   * Sends Core's `RunOperatorGitAction` and resolves with whatever the ordered
   * `OperatorGitActionFinished` published (GUI-CORE-020).
   *
   * Core owns the permission gate — on the *mapped agent tool spec*, so one
   * `viden.toml` rule set governs this commit bar and an agent's `git_commit`
   * alike — the audit record, the effect, and the classification of git's
   * stderr. The frontend never runs git, never falls back to a shell command,
   * and never parses `output`.
   *
   * `laneId` names both halves: the `SourceTarget` Core acts on and the Lane
   * whose Core-bound owner this client acts as. `null` has no owner binding and
   * is refused by the host rather than sent with an owner nobody published.
   */
  runOperatorGitAction(
    commandId: string,
    laneId: string | null,
    action: OperatorGitActionRequest,
  ): Promise<OperatorGitProjection>;
  /** Drains ordered Core events while an operator action is still pending. */
  operatorGitPoll(laneId: string | null): Promise<OperatorGitProjection>;
  /**
   * The current action projection with no Core traffic. The commit bar and the
   * titlebar sync control read it for the capability, the owner, and whether an
   * action is in flight; an approval-gated action settles only when the dock is
   * answered, so this is also what the cockpit's ordered wake re-reads.
   */
  operatorGit(laneId: string | null): Promise<OperatorGitProjection>;

  d1Cockpit(selectedLaneId: string | null): Promise<D1CockpitProjection | null>;
  d1SendIntent(commandId: string, intent: D1Intent): Promise<D1IntentResult>;
  d1Poll(selectedLaneId: string | null, waitForEvent: boolean): Promise<D1IntentResult>;

  /**
   * Composer controls. Each sends its exact Core command and resolves with
   * the refreshed D1 result, whose projection carries the mode, permission,
   * provider/model, and model options Core now publishes — including coupled
   * changes Core made that the click never asked for.
   */
  setWorkMode(
    commandId: string,
    mode: string,
    selectedLaneId: string | null,
  ): Promise<D1IntentResult>;
  setPermissionLevel(
    commandId: string,
    level: string,
    selectedLaneId: string | null,
  ): Promise<D1IntentResult>;
  selectModel(
    commandId: string,
    providerId: string,
    model: string,
    selectedLaneId: string | null,
  ): Promise<D1IntentResult>;

  /**
   * D11 project intake. `d11Poll` is also the screen's entry read: it drains
   * the ordered Core events already queued and returns the current intake
   * projection plus any command still awaiting its receipt, so the shell never
   * enters D11 on a stale view or a forgotten in-flight command.
   */
  d11SendIntent(commandId: string, intent: D11Intent): Promise<D11IntentResult>;
  d11Poll(): Promise<D11IntentResult>;

  d4SendIntent(commandId: string, intent: D4Intent): Promise<D4IntentResult>;
  d4Poll(): Promise<D4IntentResult>;

  d2Decisions(selectedId: string | null): Promise<D2DecisionsProjection | null>;
  d2SendIntent(commandId: string, intent: D2Intent): Promise<D2IntentResult>;

  d10LaneMonitor(): Promise<D10LaneMonitorProjection | null>;
  /**
   * Reads the D10 event ticker from the Core audit timeline (GUI-CORE-014).
   *
   * The same `QueryAudit` -> `AuditPageLoaded` contract D14 audit mode uses:
   * one bounded newest-first page over the whole workspace, ordered by Core
   * across projects. The screen never rebuilds a timeline by diffing
   * successive snapshots.
   */
  d10Events(commandId: string): Promise<D14AuditProjection>;
  /** Drains ordered Core events while a ticker read is still pending. */
  d10EventsPoll(): Promise<D14AuditProjection>;
  d12IntegrationGate(selectedGateId: string | null): Promise<D12IntegrationGateProjection | null>;
  /**
   * Sends one merge-gate decision as `AcceptMergeGate` or `RejectMergeGate`.
   * The host re-resolves the gate against the current Core view and replays
   * the actor and reviewed-evidence bindings Core itself published, so the
   * frontend never constructs a runtime identity or an evidence hash.
   */
  d12SendIntent(commandId: string, intent: D12Intent): Promise<D12IntentResult>;
  d13FleetWorkflow(): Promise<D13FleetWorkflowProjection | null>;
  /** D14's diagnostic mode: one raw replay page through the Core cursor. */
  d14AuditTimeline(after: string | null, limit: number): Promise<D14AuditTimelineProjection>;

  /**
   * D14's primary mode: Core's read-only `QueryAudit` for the newest page,
   * optionally scoped to one audit object. Core owns the store, the
   * newest-first order, the `1..=500` clamp, and the sanitized record bounds;
   * the frontend never reads the workflow directory or derives a record.
   *
   * An absent `runtime.audit` capability resolves with
   * `capabilityAvailable: false` and sends no command.
   */
  d14AuditQuery(commandId: string, scope: D14AuditScope | null): Promise<D14AuditProjection>;
  /** One `QueryAudit` for the page older than the cursor Core handed back. */
  d14AuditLoadOlder(commandId: string): Promise<D14AuditProjection>;
  /** Drains ordered Core events while an audit read is still pending. */
  d14AuditPoll(): Promise<D14AuditProjection>;

  permissionSendIntent(
    commandId: string,
    intent: PermissionIntent,
  ): Promise<PermissionIntentResult>;
  d6Recover(): Promise<D6RecoveryProjection>;
  /** Sends one D6 recovery action as its Core command and re-reads recovery. */
  d6SendIntent(commandId: string, intent: D6Intent): Promise<D6IntentResult>;

  /** Resolve Core-persisted content to an inline data URL the webview can show. */
  agentContent(reference: string): Promise<string>;

  openWorkspace(root: string): Promise<void>;
  /** Native folder picker; resolves null when the user cancels. */
  pickProjectFolder(title: string): Promise<string | null>;

  /**
   * Subscribe to the host's "Core advanced" wake. Undefined when the host has
   * no push bridge (e.g. a browser harness); the cockpit then keeps its
   * bounded long poll. The returned function stops the handler from firing.
   */
  onCoreWake: ((handler: () => void) => () => void) | undefined;
}
