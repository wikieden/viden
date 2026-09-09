import { translate } from "../i18n/catalog";
import { renderActivityRail } from "../components/activity_rail";
import {
  orderedAgentAdapters,
  renderAgentMenu,
  type AgentMenuController,
  type AgentMenuSelection,
} from "../components/agent_menu";
import { renderCockpitTopbar } from "../components/cockpit_topbar";
import {
  renderCommandPalette,
  type CommandPaletteController,
  type PaletteCrossLane,
  type PaletteWorkspaceFiles,
} from "../components/command_palette";
import { shouldRouteComposerMutation, shouldSubmitComposer } from "../components/composer";
import {
  modelGroups,
  renderComposerControls,
  type ComposerControlKind,
  type ComposerControlsHandle,
} from "../components/composer_controls";
import {
  renderSettingsPanel,
  type SettingsPanelController,
} from "../components/settings_panel";
import { renderStatusbar } from "../components/statusbar";
import { renderContextDock } from "../components/context_dock";
import { renderLaneRail } from "../components/lane_rail";
import {
  renderProjectPicker,
  type ProjectPickerAnchorKind,
  type ProjectPickerController,
} from "../components/project_picker";
import { renderLaneWorkSurface } from "../components/lane_work_surface";
import {
  renderPermissionDock,
  type PermissionIntent,
} from "../components/permission_dock";
import {
  appendTranscriptRows,
  transcriptAtBottom,
  type TranscriptAgent,
} from "../components/transcript";
import { renderWorkStatus, workStatusModel, type WorkStatusStrip } from "../components/work_status";
import { appendTypedWorkCards } from "../components/tool_row";
import { renderLiveWorkBar } from "../components/live_work";
import { renderWelcomeCenter } from "../components/welcome_center";
import type { ComposerControlIntent, D1Intent } from "../models/composer";
import {
  cancelPreferenceDraft,
  createPreferenceState,
  resolvedPreferencesFromWire,
  updatePreferenceDraft,
  type PreferenceDraft,
  type PreferenceIntentOutcome,
  type PreferenceState,
} from "../preferences";
import { BoundedTranscript, type D1TranscriptRow } from "../models/transcript";
import {
  RECENT_WORK_CAPABILITY,
  recentWorkStateFromResult,
  type RecentSessionView,
  type RecentWorkResult,
  type RecentWorkState,
} from "../models/recent_work";
import {
  activeWorkCounts,
  currentProjectLabel,
  type D1CockpitProjection,
  type D6RecoveryProjection,
} from "../models/workspace";
import { renderDiffReview } from "./diff_review";
import type { WorkspaceDiffProjection } from "../models/diff_review";
import {
  IDLE_OPERATOR_GIT,
  type OperatorGitActionRequest,
  type OperatorGitProjection,
} from "../models/operator_git";
import { renderD6Recovery, type SendD6Intent } from "./d6_recovery";
import "./d1_cockpit.css";

export type { D1Intent } from "../models/composer";
export { BoundedTranscript } from "../models/transcript";
export type { D1CockpitProjection } from "../models/workspace";

export interface D1IntentResult {
  projection: D1CockpitProjection;
  pendingCommandId: string | null;
  outcome: {
    state: "idle" | "pending" | "confirmed" | "rejected";
    reason: string | null;
  };
}

type SendD1Intent = (intent: D1Intent) => Promise<D1IntentResult>;
type PollD1 = (selectedLaneId?: string, waitForEvent?: boolean) => Promise<D1IntentResult>;
type SendPermissionIntent = (intent: PermissionIntent) => Promise<unknown>;
type RecoverD6 = () => Promise<D6RecoveryProjection>;

export interface D1Controller {
  applyProjection: (projection: D1CockpitProjection) => void;
  applyResult: (result: D1IntentResult) => void;
  transcript: BoundedTranscript;
  dispose: () => void;
}

function refreshPersistentRail(current: HTMLElement, next: HTMLElement): void {
  for (const attribute of Array.from(current.attributes)) {
    if (!next.hasAttribute(attribute.name)) current.removeAttribute(attribute.name);
  }
  for (const attribute of Array.from(next.attributes)) {
    current.setAttribute(attribute.name, attribute.value);
  }
  current.replaceChildren(...Array.from(next.childNodes));
  current.onkeydown = next.onkeydown;
}

export interface D1RenderOptions {
  onOpenProject?: () => void | Promise<void>;
  onCreateLane?: () => void;
  onFullSetup?: () => void;
  /**
   * Opens a restored screen from the activity rail, the status bar, or the
   * command palette. `arg` preselects one exact Core id (a D12 gate, a D2
   * decision); the target screen still re-reads its own Core projection.
   */
  onNavigate?: (route: string, arg?: string) => void;
  /**
   * Reads the cross-Lane gates and asks the command palette offers under `#`.
   *
   * The cockpit's own D1 projection is Lane-scoped, so this is a read of the
   * existing D2/D12 Core projections performed by the shell when the palette
   * opens. Absent while no host is bound, which states the section as
   * unavailable rather than showing an empty one. A rejection is fail-soft:
   * the palette still opens and still resolves lanes, sessions, and actions.
   */
  loadPaletteCrossLane?: () => Promise<{
    gates: PaletteCrossLane["gates"];
    asks: PaletteCrossLane["asks"];
  }>;
  /**
   * Reads the Core workspace file inventory the palette offers under `~`.
   *
   * The read belongs to the shell because it is a Core command, not a cockpit
   * projection. Absent while no host is bound, which states the section as
   * unavailable rather than showing an empty one — and the client never walks
   * the workspace itself, which is outside the client boundary and would
   * bypass Core's permission gate (GUI-CORE-022).
   */
  loadPaletteFiles?: () => Promise<PaletteWorkspaceFiles>;
  showWelcome?: boolean;
  poll?: boolean;
  /**
   * Subscribes to the host's ordered-Core-event wake and returns an
   * unsubscribe. When present the shell reads on a push and keeps no drain
   * timer; hosts without a wake fall back to the bounded long poll.
   */
  onCoreWake?: (handler: () => void) => () => void;
  /**
   * Reads content Core persisted in the workspace and returns something the
   * page can load. A webview cannot open a workspace path, so a reference
   * without its own scheme is unreadable until the host resolves it.
   */
  resolveContent?: (reference: string) => Promise<string>;
  /**
   * Sends one Core-backed D6 recovery action. Absent while no host is bound,
   * which keeps the restart and close-Lane controls disabled rather than
   * rendering a control that cannot reach Core.
   */
  sendD6Intent?: SendD6Intent;
  /**
   * Sends one composer control (work mode, permission level, model) as its
   * Core command and resolves with the refreshed D1 result. Absent while no
   * host is bound, which omits the selectors rather than rendering controls
   * that cannot reach Core.
   */
  sendComposerControl?: (
    intent: ComposerControlIntent,
    selectedLaneId: string | null,
  ) => Promise<D1IntentResult>;
  /**
   * The Core-owned preference loop behind the rail's gear. Absent while no
   * host is bound, which disables the gear rather than opening a panel that
   * cannot reach Core; a bound host supplies it even without the
   * `ui.preference_persistence` capability, so the operator can open the panel
   * and read why saving is unavailable.
   */
  preferences?: {
    isAvailable: () => Promise<boolean>;
    save: (state: PreferenceState) => Promise<PreferenceIntentOutcome>;
    restore: () => Promise<PreferenceIntentOutcome>;
  };
  /**
   * Reads Core's bounded recent-work inventory (`QueryRecentWork` ->
   * `RecentWorkLoaded`). Absent while no host is bound, which states the
   * Recent sections as unavailable rather than showing an empty list that
   * would read as "you have no history".
   */
  loadRecentWork?: () => Promise<RecentWorkResult>;
  /**
   * The Core-owned structured diff behind the DiffReview view (GUI-CORE-012).
   *
   * `read` is the no-traffic projection read: the cockpit calls it once to
   * learn whether Core published `runtime.structured_diff` at all, and again
   * on each ordered Core wake while the view is open, to learn whether the
   * page it is showing has been invalidated. `query` sends the actual
   * `QueryWorkspaceDiff` and resolves with Core's answer.
   *
   * Absent while no host is bound, which renders every review entry point
   * disabled-and-labelled rather than opening a view that can never fill.
   */
  workspaceDiff?: {
    read: () => Promise<WorkspaceDiffProjection>;
    query: (laneId: string | null) => Promise<WorkspaceDiffProjection>;
  };
  /**
   * Operator source-control actions (`runtime.operator_git`, GUI-CORE-020).
   *
   * `read` is the no-traffic projection read the commit bar and the titlebar
   * sync control use for the capability, the Core owner this client may act
   * as, and whether an action is already in flight. `run` sends one
   * `RunOperatorGitAction`; `poll` drains ordered Core events while an action
   * is still out, which is how an approval-gated action settles.
   *
   * Absent while no host is bound, which leaves the registered commit bar
   * visible and inert and the sync chip a `role=status` readout.
   */
  operatorGit?: {
    read: (laneId: string | null) => Promise<OperatorGitProjection>;
    run: (
      laneId: string | null,
      action: OperatorGitActionRequest,
    ) => Promise<OperatorGitProjection>;
    poll: (laneId: string | null) => Promise<OperatorGitProjection>;
  };
  /** Native folder chooser behind the picker's `Add directory…` row. */
  onPickProjectFolder?: () => Promise<string | null>;
  /**
   * Replaces the open workspace with another project root. Core supervises one
   * workspace, so this is destructive: the picker confirms it first, naming
   * the running Lanes and Agent sessions it tears down.
   */
  onOpenWorkspace?: (root: string) => void | Promise<void>;
}

type FocusedConversation =
  | { kind: "native"; laneId: string }
  | { kind: "acp"; laneId: string; sessionId: string };

function conversationForLane(
  projection: D1CockpitProjection,
  laneId: string | null,
): FocusedConversation | null {
  if (!laneId || projection.contextDock.laneAgent?.laneId !== laneId) return null;
  const sessions = projection.agentSessions.filter((candidate) => candidate.laneId === laneId);
  if (sessions.length > 1) return null;
  const session = sessions[0];
  if (session && session.sessionId !== projection.contextDock.laneAgent.sessionId) return null;
  return session
    ? { kind: "acp", laneId, sessionId: session.sessionId }
    : { kind: "native", laneId };
}

function composerMutationBlockReason(
  projection: D1CockpitProjection,
  laneId: string | null,
):
  | "d1.mutation.noLaneSelected"
  | "d1.mutation.noOwner"
  | "d1.mutation.duplicateSession"
  | "d1.mutation.ownerMismatch"
  | "d1.mutation.staleLane"
  | null {
  // No selection at all is an invitation, not a failure; a selection that
  // Core no longer publishes stays fail-closed.
  if (!laneId) {
    return "d1.mutation.noLaneSelected";
  }
  if (!projection.lanes.some((lane) => lane.id === laneId)) {
    return "d1.mutation.staleLane";
  }
  if (projection.contextDock.laneAgent?.laneId !== laneId) {
    return "d1.mutation.noOwner";
  }
  const sessions = projection.agentSessions.filter((session) => session.laneId === laneId);
  if (sessions.length > 1) {
    return "d1.mutation.duplicateSession";
  }
  if (sessions[0] && sessions[0].sessionId !== projection.contextDock.laneAgent.sessionId) {
    return "d1.mutation.ownerMismatch";
  }
  return null;
}

function button(label: string, marker?: string): HTMLButtonElement {
  const element = document.createElement("button");
  element.type = "button";
  element.className = "d1-action";
  element.textContent = label;
  if (marker) element.dataset[marker] = "true";
  return element;
}

function appendUnavailableTranscriptRows(
  root: HTMLElement,
  unavailableFeatures: D1CockpitProjection["unavailableFeatures"],
  locale: D1CockpitProjection["preferences"]["locale"],
  availableKinds: ReadonlySet<"user" | "assistant"> = new Set(),
): void {
  for (const kind of ["user", "assistant"] as const) {
    if (availableKinds.has(kind)) continue;
    const feature = unavailableFeatures.find((candidate) => candidate.id === `transcript_${kind}`);
    if (!feature) continue;
    const placeholder = document.createElement("article");
    placeholder.dataset.centerStep = kind;
    placeholder.dataset.typedEmpty = `transcript-${kind}`;
    placeholder.textContent = translate(
      locale,
      kind === "user" ? "d1.transcript.userUnavailable" : "d1.transcript.assistantUnavailable",
      {},
    );
    placeholder.title = feature.code;
    root.append(placeholder);
  }
}

/// Renders the typed content parts Core published with a message.
///
/// Text already arrives in the message body, so only non-text parts render
/// here. A part kind this build cannot draw is still named: an operator must
/// see that content exists rather than read prose about content that appears
/// to be missing.
function appendContentParts(
  row: HTMLElement,
  parts: NonNullable<
    NonNullable<D1CockpitProjection["agentSessions"][number]["conversation"]>[number]["parts"]
  >,
  resolveContent?: (reference: string) => Promise<string>,
): void {
  for (const part of parts) {
    if (part.kind === "text") continue;
    const holder = document.createElement("figure");
    holder.className = "d1-content-part";
    holder.dataset.contentPart = part.kind;

    if (part.kind === "image" && part.reference) {
      const reference = part.reference;
      const image = document.createElement("img");
      image.alt = part.label ?? "";
      image.loading = "lazy";
      if (hasOwnScheme(reference)) {
        // A reference the page can already load is used exactly as Core
        // published it; the client never rewrites it into another location.
        image.src = reference;
      } else {
        // Core persisted these bytes inside the workspace, which the webview
        // cannot open. Only the host may turn that reference into something
        // loadable, and until it does the part stays named rather than broken.
        holder.dataset.contentUnresolved = "true";
        void resolveContent?.(reference)
          .then((resolved) => {
            image.src = resolved;
            delete holder.dataset.contentUnresolved;
            holder.querySelector("[data-content-reference]")?.remove();
          })
          .catch(() => {
            /* the unresolved note already names the content. */
          });
      }
      holder.append(image);
      if (holder.dataset.contentUnresolved) {
        const note = document.createElement("figcaption");
        note.dataset.contentReference = "true";
        note.textContent = [part.mediaType, part.label, reference]
          .filter((value): value is string => Boolean(value))
          .join(" · ");
        holder.append(note);
      } else if (part.label) {
        const caption = document.createElement("figcaption");
        caption.textContent = part.label;
        holder.append(caption);
      }
    } else {
      const note = document.createElement("figcaption");
      note.textContent = [part.kind, part.mediaType, part.label, part.reference]
        .filter((value): value is string => Boolean(value))
        .join(" · ");
      holder.append(note);
    }
    row.append(holder);
  }
}

/// Whether a reference already names something the page can load itself.
function hasOwnScheme(reference: string): boolean {
  return /^[a-z][a-z0-9+.-]*:/i.test(reference);
}

/// Names the agent behind a session from the adapters Core published.
///
/// Core owns agent display names, so the client never carries its own table.
/// An agent Core did not describe keeps the id Core scoped the session by
/// rather than a friendlier name the client cannot vouch for.
function transcriptAgent(
  projection: D1CockpitProjection,
  session: D1CockpitProjection["agentSessions"][number],
): TranscriptAgent {
  const adapter = projection.agentAdapters.find(
    (candidate) => candidate.agentId === session.agentId,
  );
  return { id: session.agentId, displayName: adapter?.displayName ?? session.agentId };
}

function appendAcpConversationRows(
  root: HTMLElement,
  session: D1CockpitProjection["agentSessions"][number],
  agent: TranscriptAgent,
  resolveContent?: (reference: string) => Promise<string>,
): void {
  const conversation = session.conversation ?? [];
  if (conversation.length > 0) {
    const offset = root.children.length;
    appendTranscriptRows(
      root,
      conversation.map((message) => ({
        id: message.messageId,
        kind: message.role,
        content: message.content,
      })),
      agent,
    );
    Array.from(root.children)
      .slice(offset)
      .forEach((element, index) => {
        const message = conversation[index];
        (element as HTMLElement).dataset.acpMessageId = message.messageId;
        appendContentParts(element as HTMLElement, message.parts ?? [], resolveContent);
      });
    return;
  }

  // Older Core snapshots expose only the latest pair. Keep that pair visible
  // without treating it as durable multi-turn history.
  appendTranscriptRows(root, [
    { id: `acp-task-${session.sessionId}`, kind: "user", content: session.task },
  ]);
  root.lastElementChild?.setAttribute("data-acp-task", "true");
  appendTranscriptRows(
    root,
    [
      {
        id: `acp-output-${session.sessionId}`,
        kind: session.output ? "assistant" : "assistant_status",
        content: session.output ?? `${session.agentId} · ${session.status}`,
      },
    ],
    agent,
  );
  root.lastElementChild?.setAttribute(session.output ? "data-acp-output" : "data-acp-status", "true");
}

function duplicatesAcpConversation(
  row: D1TranscriptRow,
  conversation: NonNullable<D1CockpitProjection["agentSessions"][number]["conversation"]>,
): boolean {
  const role = row.kind.startsWith("user")
    ? "user"
    : row.kind.startsWith("assistant")
      ? "assistant"
      : null;
  if (!role) return false;
  const content = row.content.trim();
  if (!content) return false;
  return conversation.some((message) => {
    if (message.role !== role) return false;
    const canonical = message.content.trim();
    return canonical === content || (content.length >= 8 && canonical.endsWith(content));
  });
}

export function renderD1Cockpit(
  root: HTMLElement,
  initial: D1CockpitProjection,
  send: SendD1Intent,
  poll: PollD1,
  sendPermission?: SendPermissionIntent,
  recoverD6?: RecoverD6,
  options: D1RenderOptions = {},
): D1Controller {
  let projection = initial;
  let selectedLaneId = initial.selectedLaneId;
  let focusedConversation = conversationForLane(initial, initial.selectedLaneId);
  let transcriptLaneId = initial.selectedLaneId;
  let projectionKey = JSON.stringify(initial);
  let locale = initial.preferences.locale;
  let draft = "";
  let composing = false;
  let composerRefreshDeferred = false;
  let disposed = false;
  let pollTimer: number | null = null;
  let pollInFlight = false;
  let coreWakeActive = false;
  let unsubscribeCoreWake: (() => void) | null = null;
  let sending = false;
  let pendingCommandId: string | null = null;
  let submittedDraft: string | null = null;
  let composerDispatchQueued = false;
  let errorMessage: string | null = null;
  /**
   * The design's post-deny redirect: once the operator denies an ask, the
   * composer stops inviting a message and asks for the correction instead
   * ("Tell <agent> what to do instead…"), with the caret already there.
   *
   * Presentation only, and deliberately so. It is set when the deny is
   * *dispatched* rather than when Core answers, because it makes no claim
   * about the answer; `feedback` stays `null` because schema 1 carries no
   * feedback field (GUI-CORE-019). `agent` is Core's own adapter display name,
   * or null when the focused conversation has none, in which case the generic
   * wording is used rather than a guessed name.
   */
  let denyRedirect: { agent: string | null } | null = null;
  /// The request id the redirect is anchored to, so a refresh that still
  /// carries the same ask does not undo it but a *new* ask does.
  let lastPermissionRequestId: string | null = initial.permissionDock.request?.id ?? null;
  let contextDrawerOpen = false;
  /**
   * Which view owns D1's centre pane.
   *
   * DiffReview is a *view* inside the cockpit, not a route: the design
   * registers it as a D1 secondary surface, and `D-RAILNAV` keeps the activity
   * rail pointed at the standalone D-screens. Closing it returns to the
   * transcript without a navigation, so the operator never loses the
   * conversation they were reviewing for.
   */
  let centerView: "transcript" | "review" = "transcript";
  /** Core's last diff answer, or null before the first read. */
  let reviewProjection: WorkspaceDiffProjection | null = null;
  /** Selected file. Presentation state: an ordered refresh must not move it. */
  let reviewSelectedPath: string | null = null;
  /** True while a `QueryWorkspaceDiff` is out, so a wake cannot stack reads. */
  let reviewReadInFlight = false;
  /**
   * The debounce behind the re-query rule.
   *
   * While the view is open, every ordered Core wake checks the no-traffic
   * projection for `stale` — the host's count of `WorkspaceSourceUpdated` and
   * `WorkspaceChangeUpdated` facts published since the page was read. A burst
   * of file writes produces a burst of those facts, so the re-read is delayed
   * by one window and coalesced instead of firing per event.
   */
  let reviewStaleTimer: number | null = null;
  /**
   * Whether Core published `runtime.structured_diff`. Null until the first
   * no-traffic read answers; the entry points stay disabled until then rather
   * than promising a view that may not exist.
   */
  let reviewCapability: boolean | null = null;
  /**
   * Core's last word on the operator action side.
   *
   * Starts as "nothing available" rather than as an optimistic default: a bar
   * that looked live before the first no-traffic read answered would offer an
   * action this client may not be able to send.
   */
  let operatorGitState: OperatorGitProjection = IDLE_OPERATOR_GIT;
  /**
   * The commit message draft.
   *
   * Presentation state, held here rather than in the DOM, because an ordered
   * Core refresh rebuilds the review and a draft read back out of a rebuilt
   * input would be empty.
   */
  let commitMessage = "";
  /** True while a `RunOperatorGitAction` is out, so a click cannot stack one. */
  let operatorGitInFlight = false;
  /**
   * "Commit and push" is two sequential commands, never one.
   *
   * The push is sent only after the commit reports `Completed`; a commit that
   * failed or was refused stops the pair, and the view says so instead of
   * silently pushing nothing or silently dropping the second half.
   */
  let pushAfterCommit = false;
  let commitPairStopped = false;
  let laneRailOpen = false;
  let laneRailFocusTarget: "rail" | "toggle" | null = null;
  let menuController: AgentMenuController | null = null;
  // Composer-control selector state. The popover DOM is rebuilt every render,
  // so the open selector and the focus hand-back live here, not in the DOM.
  let openControl: ComposerControlKind | null = null;
  let controlInFlight = false;
  let controlReturnFocus: ComposerControlKind | null = null;
  let controlsHandle: ComposerControlsHandle | null = null;
  // Settings overlay state. The draft is GUI-local and unsaved; `resolved`
  // only ever changes on a confirmed Core result, so a rejected or pending
  // command can never leave the panel showing a preference Core did not
  // publish.
  let settingsOpen = false;
  let settingsController: SettingsPanelController | null = null;
  let remountingSettings = false;
  let preferenceState: PreferenceState = createPreferenceState(
    resolvedPreferencesFromWire(initial.preferences),
  );
  let preferencesAvailable = false;
  let preferenceSaving = false;
  let preferenceOutcome: PreferenceIntentOutcome | null = null;
  // Command palette state. The overlay DOM is disposable — a Core refresh can
  // rebuild the frame under it — so the open flag, the operator's query, and
  // the cross-Lane read live here rather than in the DOM.
  let paletteOpen = false;
  let paletteController: CommandPaletteController | null = null;
  let remountingPalette = false;
  let paletteQuery = "";
  let paletteCrossLane: PaletteCrossLane | null = null;
  let paletteFiles: PaletteWorkspaceFiles | null = null;
  // Project picker state. Like the other popovers the DOM is disposable, so
  // the open flag, which anchor opened it, and the recent-work answer live
  // here. The collapse flag is the rail's own presentation state: an ordered
  // Core refresh must not re-expand a group the operator folded away.
  let pickerOpen = false;
  let pickerController: ProjectPickerController | null = null;
  let remountingPicker = false;
  let pickerAnchorKind: ProjectPickerAnchorKind = "titlebar";
  let laneGroupCollapsed = false;
  let recentState: RecentWorkState = { kind: "loading" };
  let recentSessions: RecentSessionView[] = [];
  let recentWorkInFlight = false;
  let recentWorkRequested = false;
  /// Discards the answer to a read whose palette has already been closed.
  let paletteReadToken = 0;
  let agentMenuOpen = false;
  let remountingAgentMenu = false;
  let agentMenuComposing = false;
  let agentMenuRefreshDeferred = false;
  let newLaneSelection: AgentMenuSelection | undefined;
  let pendingAgentTaskDraft = "";
  let creatingLane = false;
  const commandSlotWaiters: Array<() => void> = [];
  let pendingLaneStart:
    | { laneId: string; task: string; agentId: string | null }
    | null = null;
  let agentDiscoveryStarted = false;
  let agentDiscoveryComplete = true;
  let agentQueryComplete = false;
  let agentDiscoveryDiagnostic: string | null = null;
  let discoveryDispatching = false;
  let discoveryInFlight:
    | { kind: "query" }
    | { kind: "probe"; agentId: string }
    | null = null;
  const attemptedAgentProbes = new Set<string>();
  const transcript = new BoundedTranscript(240);
  transcript.replace(initial.transcript);
  // Reader scroll position survives the full-surface refresh: renders rebuild
  // the transcript element, so the position is model state, not DOM state.
  let transcriptScrollTop = 0;
  // Elapsed is anchored to the moment the client first observed Core report
  // the turn busy: frontend-contract-v1 has no owner-scoped start timestamp.
  let busySince: number | null = null;
  let workStatusStrip: WorkStatusStrip | null = null;
  // Signature per shell region. A refresh only replaces the regions whose
  // own facts changed, so a streaming transcript cannot drop :hover, focus,
  // or scroll in the dock and status bar it never touched.
  const regionSignatures = new Map<string, string>();
  const regionChanged = (region: string, signature: string): boolean => {
    if (regionSignatures.get(region) === signature) return false;
    regionSignatures.set(region, signature);
    return true;
  };

  const shouldShowWelcome = (): boolean =>
    options.showWelcome ??
    (projection.recovery.state === "empty" && !!options.onOpenProject && !options.onCreateLane);

  const handleWindowKeydown = (event: KeyboardEvent): void => {
    if (
      !shouldShowWelcome() ||
      event.repeat ||
      (!event.metaKey && !event.ctrlKey) ||
      event.key.toLowerCase() !== "o"
    ) {
      return;
    }
    event.preventDefault();
    root.querySelector<HTMLButtonElement>("[data-open-project]")?.click();
  };
  const handleCancelShortcut = (event: KeyboardEvent): void => {
    // Esc cancels the running turn, matching the affordance the strip names.
    if (event.key !== "Escape" || event.repeat || composing) return;
    if (!projection.composer.busy) return;
    if (!root.querySelector("[data-work-cancel]")) return;
    event.preventDefault();
    cancelActiveTurn();
  };
  /**
   * The palette shortcuts.
   *
   * ⌘K (⌃K off macOS) toggles; ⌃P opens pre-scoped to `>`, matching the
   * composer caption the design draws (`⌘K palette · ⌃P commands`). This is a
   * deliberate divergence from the TUI, which maps the same two chords the
   * other way round; see `apps/gui/README.md`.
   *
   * Both require a modifier, so typing in the composer can never swallow them.
   * They stand down only while another modal popover owns focus, where the
   * chord belongs to that popover's own text field.
   */
  const handlePaletteShortcut = (event: KeyboardEvent): void => {
    if (event.repeat || composing) return;
    const key = event.key.toLowerCase();
    const commandPalette = (event.metaKey || event.ctrlKey) && key === "k";
    const commandScope = event.ctrlKey && !event.metaKey && !event.altKey && key === "p";
    if (!commandPalette && !commandScope) return;
    const active = document.activeElement;
    if (
      active instanceof HTMLElement &&
      active.closest("[data-settings-panel], [data-new-lane-popover], [data-control-popover]")
    ) {
      return;
    }
    event.preventDefault();
    if (commandPalette) {
      togglePalette("");
      return;
    }
    if (paletteOpen) paletteController?.setQuery(">");
    else openPalette(">");
  };
  /**
   * ⌘R / ⌃R toggles DiffReview.
   *
   * The design's own binding: DockSD's summon menu puts `⌘R` on Review. It is
   * unbound elsewhere in the GUI (the palette owns ⌘K and ⌃P, the composer
   * owns Enter and Escape), so nothing is being re-pointed. `preventDefault`
   * matters because the chord is the webview's reload: a reload would discard
   * the transcript and the composer draft to show a diff.
   *
   * It stands down while a modal popover owns focus, exactly as the palette
   * chords do, and while no host is bound or Core published no structured
   * diff — the same condition the visible entry points fail closed on.
   */
  const handleReviewShortcut = (event: KeyboardEvent): void => {
    if (event.repeat || composing) return;
    if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey) return;
    if (event.key.toLowerCase() !== "r") return;
    const active = document.activeElement;
    if (
      active instanceof HTMLElement &&
      active.closest(
        "[data-settings-panel], [data-new-lane-popover], [data-control-popover], [data-command-palette]",
      )
    ) {
      return;
    }
    if (!reviewAvailable()) return;
    event.preventDefault();
    if (centerView === "review") closeReview();
    else openReview();
  };
  window.addEventListener("keydown", handleCancelShortcut);
  window.addEventListener("keydown", handleWindowKeydown);
  window.addEventListener("keydown", handlePaletteShortcut);
  window.addEventListener("keydown", handleReviewShortcut);
  const handleWindowResize = (): void => {
    const grid = root.querySelector<HTMLElement>("[data-cockpit-grid]");
    if (grid) grid.dataset.cockpitLayout = window.innerWidth <= 1100 ? "narrow" : "desktop";
  };
  window.addEventListener("resize", handleWindowResize);

  /**
   * Retires the deny redirect when Core publishes a *different* ask.
   *
   * Anchoring on the request id rather than on presence is what separates the
   * two cases: refreshes that still carry the ask the operator just denied are
   * the ordinary state between the command and Core's answer and must not
   * clear the prompt out from under someone typing into it, while a new ask
   * genuinely supersedes the correction they were about to give.
   */
  function noteRedirectAgainst(next: D1CockpitProjection): void {
    const nextId = next.permissionDock.request?.id ?? null;
    if (nextId !== null && nextId !== lastPermissionRequestId) denyRedirect = null;
    lastPermissionRequestId = nextId;
  }

  /// Core's own display name for the agent owning the focused conversation, or
  /// null. Never derived from an agent id: an unnamed adapter gets the generic
  /// wording rather than a client-invented name.
  function focusedAgentDisplayName(): string | null {
    if (focusedConversation?.kind !== "acp") return null;
    const sessionId = focusedConversation.sessionId;
    const session = projection.agentSessions.find(
      (candidate) => candidate.sessionId === sessionId,
    );
    if (!session) return null;
    const adapter = projection.agentAdapters.find(
      (candidate) => candidate.agentId === session.agentId,
    );
    return adapter?.displayName ?? null;
  }

  /**
   * Dispatches one permission response, and redirects the composer on a deny.
   *
   * The redirect is applied before the await on purpose: it is a presentation
   * change that claims nothing about Core's answer, and making the operator
   * wait for a round trip before they can type the correction would defeat the
   * affordance. No fact is written here — the intent is passed through exactly
   * as the dock built it, `feedback` included.
   */
  const dispatchPermission = (intent: PermissionIntent): Promise<unknown> => {
    if (intent.choice === "deny") {
      denyRedirect = { agent: focusedAgentDisplayName() };
      render(true);
    }
    return sendPermission ? sendPermission(intent) : Promise.resolve(undefined);
  };

  const controller: D1Controller = {
    transcript,
    applyProjection: (next) => {
      if (disposed) return;
      const nextKey = JSON.stringify(next);
      if (nextKey === projectionKey) return;
      projection = next;
      if (!selectedLaneId) selectedLaneId = next.selectedLaneId;
      if (
        selectedLaneId &&
        next.lanes.length === 0 &&
        !next.lanes.some((lane) => lane.id === selectedLaneId)
      ) {
        // The whole project is back to zero Lanes; a stale selection would
        // pin the composer on a fail-closed notice forever.
        selectedLaneId = next.selectedLaneId;
      }
      focusedConversation = conversationForLane(next, selectedLaneId);
      noteRedirectAgainst(next);
      busySince = next.composer.busy ? (busySince ?? Date.now()) : null;
      projectionKey = nextKey;
      locale = next.preferences.locale;
      if (next.selectedLaneId === selectedLaneId) {
        if (transcriptLaneId !== selectedLaneId) transcript.reset(next.transcript);
        else transcript.replace(next.transcript);
        transcriptLaneId = selectedLaneId;
      }
      render(false);
      queueMicrotask(maybeResumeLaneStart);
    },
    applyResult: (result) => {
      if (disposed) return;
      const previousPending = pendingCommandId;
      pendingCommandId = result.pendingCommandId;
      if (result.outcome.state === "confirmed" && submittedDraft !== null) {
        // Do not erase edits typed after the submitted content entered pending state.
        if (draft === submittedDraft) draft = "";
        submittedDraft = null;
        errorMessage = null;
      } else if (result.outcome.state === "rejected") {
        submittedDraft = null;
        errorMessage = result.outcome.reason ?? "Core rejected the command.";
        if (
          previousPending &&
          pendingLaneStart &&
          !result.projection.lanes.some((lane) => lane.id === pendingLaneStart?.laneId)
        ) {
          pendingLaneStart = null;
        }
      }
      const nextKey = JSON.stringify(result.projection);
      const projectionChanged = nextKey !== projectionKey;
      if (projectionChanged) {
        projection = result.projection;
        if (!selectedLaneId) selectedLaneId = result.projection.selectedLaneId;
        {
          const next = result.projection;
          if (
            selectedLaneId &&
            next.lanes.length === 0 &&
            !next.lanes.some((lane) => lane.id === selectedLaneId)
          ) {
            // The whole project is back to zero Lanes; a stale selection would
            // pin the composer on a fail-closed notice forever.
            selectedLaneId = next.selectedLaneId;
          }
        }
        focusedConversation = conversationForLane(result.projection, selectedLaneId);
        noteRedirectAgainst(result.projection);
        busySince = result.projection.composer.busy ? (busySince ?? Date.now()) : null;
        projectionKey = nextKey;
        locale = result.projection.preferences.locale;
        if (result.projection.selectedLaneId === selectedLaneId) {
          if (transcriptLaneId !== selectedLaneId) transcript.reset(result.projection.transcript);
          else transcript.replace(result.projection.transcript);
          transcriptLaneId = selectedLaneId;
        }
      }
      if (pendingLaneStart && result.projection.permissionDock.request) {
        // The pre-registration Lane approval belongs to the center dock. Keep
        // its primary actions mouse-operable by dismissing the floating rail
        // that launched the New Lane overlay.
        laneRailOpen = false;
        laneRailFocusTarget = null;
      }
      const discoveryChanged = observeAgentDiscoveryResult(result);
      if (
        projectionChanged ||
        previousPending !== pendingCommandId ||
        result.outcome.state === "confirmed" ||
        result.outcome.state === "rejected" ||
        discoveryChanged
      ) {
        render(false);
      }
      releaseCommandSlotWaiters();
      queueMicrotask(maybeResumeLaneStart);
    },
    dispose: () => {
      disposed = true;
      agentMenuOpen = false;
      settingsOpen = false;
      settingsController?.close();
      settingsController = null;
      pickerOpen = false;
      pickerController?.close();
      pickerController = null;
      menuController?.close();
      controlsHandle?.dispose();
      controlsHandle = null;
      if (pollTimer !== null) window.clearTimeout(pollTimer);
      unsubscribeCoreWake?.();
      unsubscribeCoreWake = null;
      commandSlotWaiters.splice(0).forEach((resolve) => resolve());
      paletteOpen = false;
      paletteController?.dispose();
      paletteController = null;
      window.removeEventListener("keydown", handleWindowKeydown);
      window.removeEventListener("keydown", handleCancelShortcut);
      window.removeEventListener("keydown", handlePaletteShortcut);
      window.removeEventListener("keydown", handleReviewShortcut);
      if (reviewStaleTimer !== null) window.clearTimeout(reviewStaleTimer);
      reviewStaleTimer = null;
      workStatusStrip?.dispose();
      workStatusStrip = null;
      window.removeEventListener("resize", handleWindowResize);
    },
  };

  const commandSlotAvailable = (): boolean =>
    !sending &&
    !pendingCommandId &&
    !controlInFlight &&
    !discoveryDispatching &&
    !discoveryInFlight &&
    !pollInFlight;

  const releaseCommandSlotWaiters = (): void => {
    if (!commandSlotAvailable()) return;
    commandSlotWaiters.splice(0).forEach((resolve) => resolve());
  };

  const waitForCommandSlot = async (): Promise<boolean> => {
    if (!commandSlotAvailable()) {
      await new Promise<void>((resolve) => commandSlotWaiters.push(resolve));
    }
    return !disposed;
  };

  const drainOnce = (): void => {
    if (disposed || pollInFlight || !document.contains(root)) return;
    if (sending || discoveryDispatching) return;
    pollInFlight = true;
    void poll(selectedLaneId ?? undefined, false)
      .then((result) => {
        if (!disposed) controller.applyResult(result);
      })
      .catch(() => undefined)
      .finally(() => {
        pollInFlight = false;
        if (disposed) return;
        releaseCommandSlotWaiters();
        queueMicrotask(maybeResumeLaneStart);
        queueMicrotask(advanceAgentDiscovery);
        // The DiffReview re-query rule and the operator action's settlement
        // both ride the same ordered Core wake the cockpit already listens on,
        // so neither needs a timer of its own.
        queueMicrotask(noteReviewStaleness);
        queueMicrotask(noteOperatorGitPending);
      });
  };


  /* ---- DiffReview (GUI-CORE-012) ---- */

  /// Milliseconds a stale page waits before it re-reads.
  ///
  /// One window rather than one read per event: an agent writing a dozen files
  /// publishes a dozen invalidating facts, and twelve bounded Core queries for
  /// one visible change would be a client-made load spike.
  const REVIEW_RESTALE_MS = 400;

  /// Learns whether Core publishes structured diff rows at all, without
  /// sending a command. Entry points read the answer.
  const ensureReviewCapability = (): void => {
    if (reviewCapability !== null || !options.workspaceDiff) return;
    void options.workspaceDiff
      .read()
      .then((projection) => {
        if (disposed) return;
        const next = projection.capabilityAvailable;
        if (next === reviewCapability) return;
        reviewCapability = next;
        render(false);
      })
      .catch(() => {
        // A host that cannot answer is not a Core that lacks the capability.
        // Leaving it unresolved keeps the entry disabled without claiming why.
      });
  };

  /// Sends one `QueryWorkspaceDiff` for the current target and redraws.
  ///
  /// The target follows the cockpit's Lane selection: a selected Lane reviews
  /// that Lane's worktree, and no selection reviews the workspace root. Core
  /// resolves the worktree from the Lane id — the client passes no path.
  const readReview = (): void => {
    if (!options.workspaceDiff || reviewReadInFlight) return;
    reviewReadInFlight = true;
    const laneId = selectedLaneId;
    void options.workspaceDiff
      .query(laneId)
      .then((projection) => {
        if (disposed) return;
        reviewProjection = projection;
        reviewCapability = projection.capabilityAvailable;
      })
      .catch((error: unknown) => {
        if (disposed) return;
        // A transport failure is not Core's refusal, so it is reported as this
        // client's own error rather than dressed up as a Core rejection.
        reviewProjection = {
          outcome: { state: "rejected", reason: String(error) },
          targetLaneId: laneId,
          source: null,
          entries: [],
          truncated: false,
          loaded: false,
          pendingCommandId: null,
          capabilityAvailable: reviewCapability !== false,
          stale: false,
        };
      })
      .finally(() => {
        reviewReadInFlight = false;
        if (!disposed && centerView === "review") render(false);
      });
  };

  /// The re-query rule, evaluated on every ordered Core wake.
  ///
  /// Only while the view is open: a closed review re-reads on open anyway, and
  /// polling for a pane nobody is looking at would be traffic with no reader.
  const noteReviewStaleness = (): void => {
    if (centerView !== "review" || !options.workspaceDiff) return;
    if (reviewReadInFlight || reviewStaleTimer !== null) return;
    void options.workspaceDiff
      .read()
      .then((projection) => {
        if (disposed || centerView !== "review" || !projection.stale) return;
        reviewProjection = projection;
        render(false);
        reviewStaleTimer = window.setTimeout(() => {
          reviewStaleTimer = null;
          if (disposed || centerView !== "review") return;
          readReview();
        }, REVIEW_RESTALE_MS);
      })
      .catch(() => undefined);
  };

  /* ---- operator git actions (GUI-CORE-020) ---- */

  /// Learns the capability, the owner, and any in-flight action without
  /// sending anything. Every action control reads the answer.
  const readOperatorGit = (): void => {
    if (!options.operatorGit) return;
    void options.operatorGit
      .read(selectedLaneId)
      .then((state) => {
        if (disposed) return;
        operatorGitState = state;
        render(false);
      })
      .catch(() => {
        // A host that cannot answer is not a Core that lacks the capability.
        // Leaving the last known state keeps the controls disabled without
        // claiming a reason that may not be true.
      });
  };

  /**
   * Sends one action and applies Core's ordered answer.
   *
   * One at a time: the commit bar is one surface with one message box, and a
   * second action in flight could only race the first for the same index.
   */
  const runOperatorGit = (action: OperatorGitActionRequest): void => {
    const port = options.operatorGit;
    if (!port || operatorGitInFlight) return;
    operatorGitInFlight = true;
    commitPairStopped = false;
    const laneId = selectedLaneId;
    void port
      .run(laneId, action)
      .then((state) => {
        if (disposed) return;
        operatorGitState = state;
      })
      .catch((error: unknown) => {
        if (disposed) return;
        // A transport failure is this client's error, not Core's refusal, so
        // it is never dressed up as a `CommandRejected`. The pair stops here
        // too: nothing may be assumed about whether the effect ran.
        operatorGitState = {
          ...operatorGitState,
          outcome: { state: "rejected", reason: String(error) },
          pendingCommandId: null,
          pendingAction: null,
          awaitingApproval: false,
          result: null,
        };
      })
      .finally(() => {
        operatorGitInFlight = false;
        if (disposed) return;
        advanceCommitPair();
        // The action's own `WorkspaceSourceUpdated` invalidates the open page,
        // so the review re-reads through the same debounced staleness path a
        // Core-side write goes through. Nothing is patched into the rows here.
        queueMicrotask(noteReviewStaleness);
        queueMicrotask(noteOperatorGitPending);
        render(false);
      });
  };

  /**
   * The second half of "commit and push", or the sentence that says it stopped.
   *
   * The push is sent only on a `Completed` commit. Success is never inferred
   * from git's output text: `result.kind` is Core's own typed answer.
   */
  const advanceCommitPair = (): void => {
    if (!pushAfterCommit) return;
    const state = operatorGitState;
    if (state.outcome.state === "pending") return;
    const completedCommit =
      state.outcome.state === "confirmed" &&
      state.result?.kind === "completed" &&
      state.result.action === "commit";
    pushAfterCommit = false;
    if (!completedCommit) {
      // A failed or refused commit has nothing to push. Saying so is the whole
      // point: a silently dropped second half would leave the operator
      // believing the branch was published.
      commitPairStopped = true;
      return;
    }
    commitMessage = "";
    runOperatorGit({ type: "push", remote: null, setUpstream: false });
  };

  /// Drains ordered Core events while an action is still out.
  ///
  /// An approval-gated action settles only when the operator answers the dock,
  /// which is an ordered Core fact like any other, so this rides the cockpit's
  /// existing wake instead of holding a timer.
  const noteOperatorGitPending = (): void => {
    const port = options.operatorGit;
    if (!port || operatorGitInFlight) return;
    if (operatorGitState.outcome.state !== "pending") return;
    operatorGitInFlight = true;
    const laneId = selectedLaneId;
    void port
      .poll(laneId)
      .then((state) => {
        if (disposed) return;
        operatorGitState = state;
      })
      .catch(() => undefined)
      .finally(() => {
        operatorGitInFlight = false;
        if (disposed) return;
        advanceCommitPair();
        render(false);
      });
  };

  const reviewAvailable = (): boolean =>
    !!options.workspaceDiff && reviewCapability === true;

  const openReview = (): void => {
    if (!options.workspaceDiff) return;
    centerView = "review";
    render(false);
    readReview();
  };

  const closeReview = (): void => {
    centerView = "transcript";
    if (reviewStaleTimer !== null) {
      window.clearTimeout(reviewStaleTimer);
      reviewStaleTimer = null;
    }
    render(true);
  };

  const schedulePoll = (): void => {
    if (coreWakeActive) return;
    if (disposed || pollTimer !== null || pollInFlight) return;
    pollTimer = window.setTimeout(() => {
      pollTimer = null;
      if (disposed || !document.contains(root)) return;
      if (sending || discoveryDispatching) {
        schedulePoll();
        return;
      }
      pollInFlight = true;
      void poll(selectedLaneId ?? undefined, true)
        .then((result) => {
          if (!disposed) controller.applyResult(result);
        })
        .catch(() => undefined)
        .finally(() => {
          pollInFlight = false;
          if (disposed) return;
          releaseCommandSlotWaiters();
          queueMicrotask(maybeResumeLaneStart);
          queueMicrotask(advanceAgentDiscovery);
          queueMicrotask(noteReviewStaleness);
          queueMicrotask(noteOperatorGitPending);
          schedulePoll();
        });
    }, 250);
  };

  const sendAndWait = async (
    intent: D1Intent,
    onRejected?: () => void,
  ): Promise<D1IntentResult | null> => {
    if (sending || pendingCommandId) return null;
    sending = true;
    try {
      let result = await send(intent);
      controller.applyResult(result);
      for (
        let attempt = 0;
        result.pendingCommandId &&
        result.projection.permissionDock.request === null &&
        attempt < 24;
        attempt += 1
      ) {
        result = await poll(selectedLaneId ?? undefined, true);
        controller.applyResult(result);
      }
      return result;
    } catch (error: unknown) {
      root.dataset.d1Error = String(error);
      errorMessage = String(error);
      submittedDraft = null;
      onRejected?.();
      render(true);
      return null;
    } finally {
      sending = false;
      schedulePoll();
    }
  };

  const cancelActiveTurn = (): void => {
    const commandBlock = composerMutationBlockReason(projection, selectedLaneId);
    const route = conversationForLane(projection, selectedLaneId);
    if (commandBlock) {
      errorMessage = translate(locale, commandBlock, {});
      render(false);
      return;
    }
    if (route?.kind === "acp") {
      sendIntent({ type: "cancel_agent_session", laneId: route.laneId, sessionId: route.sessionId });
    } else if (selectedLaneId) {
      sendIntent({ type: "cancel", laneId: selectedLaneId });
    }
  };

  const sendIntent = (intent: D1Intent, onRejected?: () => void): void => {
    void sendAndWait(intent, onRejected);
  };

  const sendComposerIntent = (intent: D1Intent, onRejected?: () => void): void => {
    if (composerDispatchQueued) return;
    composerDispatchQueued = true;
    void (async () => {
      try {
        if (!commandSlotAvailable() && !(await waitForCommandSlot())) return;
        await sendAndWait(intent, onRejected);
      } finally {
        composerDispatchQueued = false;
        if (!disposed) render(true);
      }
    })();
    render(true);
  };

  /// Dispatches one composer control and re-renders from the returned state
  /// only. Core owns the mode/permission coupling rule, so both selectors
  /// always reflect the snapshot Core republished — never the clicked option.
  const sendControl = (intent: ComposerControlIntent): void => {
    const dispatch = options.sendComposerControl;
    if (!dispatch || controlInFlight || sending || pendingCommandId) return;
    controlInFlight = true;
    controlReturnFocus = openControl;
    openControl = null;
    errorMessage = null;
    void (async () => {
      try {
        let result = await dispatch(intent, selectedLaneId ?? null);
        controller.applyResult(result);
        for (
          let attempt = 0;
          result.pendingCommandId && attempt < 24;
          attempt += 1
        ) {
          result = await poll(selectedLaneId ?? undefined, true);
          controller.applyResult(result);
        }
      } catch (error: unknown) {
        // A host/transport failure renders through the same rejection alert
        // as a Core rejection (data-d1-rejection, role=alert).
        root.dataset.d1Error = String(error);
        errorMessage = String(error);
      } finally {
        controlInFlight = false;
        if (!disposed) {
          releaseCommandSlotWaiters();
          render(false);
          // The Settings overlay renders the same two controls, so it has to
          // follow the snapshot Core republished. `render` only re-anchors a
          // detached panel, so an open one is rebuilt explicitly.
          remountSettingsPanel();
        }
      }
    })();
    render(false);
    remountSettingsPanel();
  };

  const submitComposer = (content: string): void => {
    const mutationBlock = composerMutationBlockReason(projection, selectedLaneId);
    if (!shouldRouteComposerMutation(content, mutationBlock)) {
      if (mutationBlock) {
        errorMessage = translate(locale, mutationBlock, {});
        render(true);
      }
      return;
    }
    if (!selectedLaneId) return;
    submittedDraft = content;
    errorMessage = null;
    // The correction has been sent, so the prompt goes back to inviting a
    // message. A submit blocked above never gets here, and never clears it.
    denyRedirect = null;
    const route = conversationForLane(projection, selectedLaneId);
    if (route?.kind === "acp" && !projection.composer.busy) {
      sendComposerIntent(
        {
          type: "send_agent_session_input",
          laneId: route.laneId,
          sessionId: route.sessionId,
          content,
        },
        () => {
          draft = content;
        },
      );
      return;
    }
    sendComposerIntent({ type: "submit", laneId: selectedLaneId, content }, () => {
      draft = content;
    });
  };

  const startLane = async (task: string, agentId: string | null): Promise<boolean> => {
    // Native Lane creation remains available while ACP discovery runs, but
    // both use Core's one-command-at-a-time D1 adapter boundary.
    if (!(await waitForCommandSlot())) return false;
    const previewResult = await sendAndWait({ type: "preview_default_lane", preset: "coder" });
    const preview = previewResult?.projection.starterLanePreviews.at(-1);
    if (
      !preview ||
      previewResult?.outcome.state === "rejected" ||
      preview.diagnostics.length > 0
    ) {
      errorMessage =
        previewResult?.outcome.reason ??
        preview?.diagnostics[0] ??
        projection.workspaceEligibility?.diagnostic ??
        "Core did not publish a creatable Lane preview.";
      render(false);
      return false;
    }
    pendingLaneStart = { laneId: preview.laneId, task, agentId };
    const createResult = await sendAndWait({
      type: "create_starter_lane",
      laneId: preview.laneId,
      preset: "coder",
      branch: preview.branch,
      previewId: preview.previewId,
      contentSha256: preview.contentSha256,
    });
    if (!createResult || createResult.outcome.state === "rejected") {
      pendingLaneStart = null;
      return false;
    }
    await maybeResumeLaneStart();
    return true;
  };

  async function maybeResumeLaneStart(): Promise<void> {
    const pending = pendingLaneStart;
    if (!pending || !projection.lanes.some((lane) => lane.id === pending.laneId)) return;
    if (!commandSlotAvailable()) return;
    // Approval resolves asynchronously; resume only after Core projects the
    // exact confirmed Lane and releases the command slot, while retaining its
    // sole Agent task locally.
    pendingLaneStart = null;
    selectedLaneId = pending.laneId;
    if (pending.agentId === null) {
      focusedConversation = { kind: "native", laneId: pending.laneId };
      submittedDraft = pending.task;
      await sendAndWait({ type: "submit", laneId: pending.laneId, content: pending.task });
      return;
    }
    const result = await sendAndWait({
      type: "start_agent_session",
      laneId: pending.laneId,
      agentId: pending.agentId,
      model: null,
      task: pending.task,
    });
    const session = result?.projection.agentSessions
      .filter(
        (candidate) =>
          candidate.laneId === pending.laneId && candidate.agentId === pending.agentId,
      )
      .at(-1);
    if (session) {
      focusedConversation = {
        kind: "acp",
        laneId: session.laneId,
        sessionId: session.sessionId,
      };
      render(true);
    }
  };

  function discoveryIsProbing(): boolean {
    return agentDiscoveryStarted && !agentDiscoveryComplete;
  }

  /// Rebuilds the Settings overlay against the current model.
  ///
  /// The panel DOM is disposable, so the draft, the in-flight flag, and the
  /// last Core outcome live here rather than in the DOM: a Core refresh that
  /// redraws the cockpit must not silently discard an unsaved draft.
  function mountSettingsPanel(): SettingsPanelController | null {
    if (!settingsOpen || disposed || settingsController) return settingsController;
    const anchor = root.querySelector<HTMLButtonElement>("[data-settings-toggle]");
    if (!anchor) return null;
    settingsController = renderSettingsPanel(
      anchor,
      {
        locale,
        state: preferenceState,
        available: preferencesAvailable,
        saving: preferenceSaving,
        outcome: preferenceOutcome,
        // The panel's permission and model rows are a second surface onto the
        // composer pills, so they read the same Core facts and enumerate from
        // the same shared sources. Gating mirrors the pills exactly; without a
        // dispatcher the sections render inert rather than disappearing.
        controls: !options.sendComposerControl
          ? null
          : {
              permissionLevel: projection.statusbar.permissionLevel,
              providerId: projection.environment.providerId,
              model: projection.environment.model,
              groups: modelGroups(projection),
              cwd: projection.environment.cwd,
              enabled: projection.composer.editable && !shouldShowWelcome(),
              busy: controlInFlight || sending || Boolean(pendingCommandId),
            },
      },
      {
        onControl: (intent: ComposerControlIntent) => sendControl(intent),
        onDraft: (update: PreferenceDraft) => {
          preferenceState = updatePreferenceDraft(preferenceState, update);
          // A new edit supersedes the previous result; keeping a stale
          // "saved" line next to an unsaved draft would misreport Core.
          preferenceOutcome = null;
          remountSettingsPanel();
        },
        onCancel: () => {
          preferenceState = cancelPreferenceDraft(preferenceState);
          preferenceOutcome = null;
          remountSettingsPanel();
        },
        onSave: () => void runPreferenceCommand(() => options.preferences?.save(preferenceState)),
        onRestore: () => void runPreferenceCommand(() => options.preferences?.restore()),
        onClose: () => {
          settingsController = null;
          // A remount closes the old controller on purpose; only an operator
          // dismissal (Escape, ×, outside click) actually closes the overlay.
          if (!remountingSettings) settingsOpen = false;
        },
      },
    );
    return settingsController;
  }

  /// Replaces the overlay in place, preserving the operator's keyboard place.
  ///
  /// The panel DOM is rebuilt on every model change, so which control has
  /// focus is model state: closing the old controller hands focus back to the
  /// gear, and the equivalent control in the new panel takes it back.
  function remountSettingsPanel(): void {
    if (!settingsOpen || disposed) return;
    const active = document.activeElement as HTMLElement | null;
    const focusedOption = active?.dataset.settingsOption ?? null;
    const focusedAction = ["settingsSave", "settingsCancel", "settingsRestore"].find(
      (name) => active?.dataset[name] !== undefined,
    );
    remountingSettings = true;
    settingsController?.close();
    remountingSettings = false;
    settingsController = null;
    const next = mountSettingsPanel();
    const restored = focusedOption
      ? next?.root.querySelector<HTMLElement>(`[data-settings-option="${focusedOption}"]`)
      : focusedAction
        ? next?.root.querySelector<HTMLElement>(
            `[data-${focusedAction.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`)}]`,
          )
        : null;
    if (restored instanceof HTMLButtonElement && !restored.disabled) {
      restored.tabIndex = 0;
      restored.focus();
    }
  }

  /// Runs one preference command and renders only what Core confirmed.
  async function runPreferenceCommand(
    send: () => Promise<PreferenceIntentOutcome> | undefined,
  ): Promise<void> {
    if (preferenceSaving) return;
    preferenceSaving = true;
    preferenceOutcome = null;
    remountSettingsPanel();
    try {
      const outcome = await send();
      if (disposed || !outcome) return;
      preferenceOutcome = outcome;
      if (outcome.status === "confirmed") {
        // Core's resolution is the only thing that becomes authority; the
        // draft is cleared because Core has answered it.
        preferenceState = {
          resolved: outcome.resolved,
          draft: null,
          dirty: false,
        };
        locale = outcome.resolved.locale;
      }
      if (outcome.status === "unavailable") preferencesAvailable = false;
    } catch (error: unknown) {
      if (disposed) return;
      preferenceOutcome = { status: "rejected", reason: String(error), diagnostics: [] };
    } finally {
      preferenceSaving = false;
      if (!disposed) {
        remountSettingsPanel();
        render(false);
      }
    }
  }

  /// Opens the overlay after reading the capability from the handshake, so an
  /// absent capability renders as a read-only panel instead of controls that
  /// cannot reach Core.
  async function openSettingsPanel(): Promise<void> {
    if (settingsOpen || disposed) return;
    settingsOpen = true;
    preferenceOutcome = null;
    preferenceState = createPreferenceState(resolvedPreferencesFromWire(projection.preferences));
    try {
      preferencesAvailable = (await options.preferences?.isAvailable()) === true;
    } catch {
      preferencesAvailable = false;
    }
    if (disposed || !settingsOpen) return;
    render(false);
    mountSettingsPanel();
  }

  /// Reads Core's bounded recent-work inventory at most once per controller.
  ///
  /// The four rendered states stay distinct: an absent capability, a Core
  /// rejection, a read Core has not answered, and a genuinely empty inventory
  /// are different facts, and flattening them into one empty list would
  /// misreport Core. Nothing here scans the session home as a fallback.
  async function ensureRecentWork(): Promise<void> {
    if (recentWorkRequested || disposed) return;
    recentWorkRequested = true;
    if (!options.loadRecentWork) {
      recentState = {
        kind: "unavailable",
        reason: translate(locale, "d1.recent.unavailable", {
          capability: RECENT_WORK_CAPABILITY,
        }),
      };
      return;
    }
    recentWorkInFlight = true;
    recentState = { kind: "loading" };
    try {
      const result: RecentWorkResult = await options.loadRecentWork();
      if (disposed) return;
      const derived = recentWorkStateFromResult(result, {
        unavailable: translate(locale, "d1.recent.unavailable", {
          capability: RECENT_WORK_CAPABILITY,
        }),
        pending: translate(locale, "d1.recent.pending", {}),
      });
      recentState = derived.state;
      if (derived.state.kind === "loaded") {
        recentSessions = derived.sessions;
      }
    } catch (error: unknown) {
      if (disposed) return;
      recentState = { kind: "failed", reason: String(error) };
    } finally {
      recentWorkInFlight = false;
      if (!disposed) {
        remountProjectPicker();
        render(false);
      }
    }
  }

  /// Rebuilds the project picker against the current projection and read state.
  function mountProjectPicker(): ProjectPickerController | null {
    if (!pickerOpen || disposed || pickerController) return pickerController;
    const anchor = root.querySelector<HTMLElement>(
      pickerAnchorKind === "titlebar" ? "[data-project-selector]" : "[data-add-project]",
    );
    if (!anchor) return null;
    const active = activeWorkCounts(projection);
    pickerController = renderProjectPicker(
      anchor,
      pickerAnchorKind,
      {
        locale,
        current: {
          displayName: currentProjectLabel(projection),
          canonicalRoot: projection.environment.cwd,
          activeLaneCount: active.lanes,
          activeSessionCount: active.sessions,
          laneCount: projection.lanes.length,
        },
        recent: recentWorkInFlight ? { kind: "loading" } : recentState,
        now: Date.now(),
      },
      {
        onPickDirectory: async () => (await options.onPickProjectFolder?.()) ?? null,
        onSwitchWorkspace: async (nextRoot) => {
          await options.onOpenWorkspace?.(nextRoot);
        },
        onClose: () => {
          pickerController = null;
          // A remount closes the old controller on purpose; only an operator
          // dismissal actually closes the picker.
          if (!remountingPicker) pickerOpen = false;
        },
      },
    );
    return pickerController;
  }

  function remountProjectPicker(): void {
    if (!pickerOpen || disposed) return;
    remountingPicker = true;
    pickerController?.close();
    remountingPicker = false;
    pickerController = null;
    mountProjectPicker();
  }

  /// Opens the picker from whichever anchor asked for it.
  function openProjectPicker(anchorKind: ProjectPickerAnchorKind): void {
    if (disposed) return;
    if (pickerOpen) {
      pickerController?.close();
      return;
    }
    pickerOpen = true;
    pickerAnchorKind = anchorKind;
    void ensureRecentWork();
    render(false);
    mountProjectPicker();
  }

  /// The single in-cockpit Lane selection path.
  ///
  /// Shared by the Lane rail and the command palette so both leave the cockpit
  /// in exactly the same state: the transcript is dropped rather than carried
  /// across Lanes, and the focused conversation is re-derived from the Lane
  /// owner Core published.
  function selectLane(laneId: string): void {
    selectedLaneId = laneId;
    transcriptLaneId = null;
    transcript.reset([]);
    focusedConversation = conversationForLane(projection, laneId);
    // The owner an operator action may act as is the *selected* Lane's, so a
    // selection change re-reads it. Without this the commit bar would keep the
    // previous Lane's availability — and, worse, could look enabled for a Lane
    // Core published no binding for.
    readOperatorGit();
    render(false);
  }

  /// Where the palette portals to. Outside the frame, so a Core refresh that
  /// replaces the frame's regions cannot clip or drop the overlay.
  function paletteHost(): HTMLElement {
    return root.querySelector<HTMLElement>(".d1-frame")?.parentElement ?? root;
  }

  function mountCommandPalette(): void {
    if (!paletteOpen || disposed || paletteController) return;
    paletteController = renderCommandPalette(
      paletteHost(),
      {
        locale,
        projection,
        query: paletteQuery,
        crossLane: paletteCrossLane,
        files: paletteFiles,
        canNavigate: Boolean(options.onNavigate),
        canOpenSettings: Boolean(options.preferences),
        canFocusComposer: Boolean(root.querySelector("[data-composer]")),
        canCancelTurn: Boolean(root.querySelector("[data-work-cancel]")),
        // A bound host always gets the row; an absent capability renders it
        // disabled naming the capability, the same honesty the `~` file scope
        // ships, rather than a row that quietly disappears.
        reviewBound: Boolean(options.workspaceDiff),
        reviewAvailable: reviewAvailable(),
        returnFocus: root.querySelector<HTMLElement>("[data-command-palette-toggle]"),
      },
      {
        onNavigate: (route, arg) => options.onNavigate?.(route, arg),
        onSelectLane: (laneId) => {
          selectLane(laneId);
          // The palette is a keyboard surface: after a jump the caret belongs
          // in the composer of the Lane the operator just chose.
          root.querySelector<HTMLTextAreaElement>("[data-composer]")?.focus();
        },
        onOpenSettings: () => void openSettingsPanel(),
        onFocusComposer: () => {
          root.querySelector<HTMLTextAreaElement>("[data-composer]")?.focus();
        },
        onCancelTurn: () => cancelActiveTurn(),
        onOpenReview: () => openReview(),
        onQueryChange: (next) => {
          paletteQuery = next;
        },
        onClose: () => {
          paletteController = null;
          // A remount closes the old overlay on purpose; only an operator
          // dismissal actually closes the palette.
          if (remountingPalette) return;
          paletteOpen = false;
          paletteQuery = "";
          render(false);
        },
      },
    );
  }

  /// Opens the palette and starts the cross-Lane read.
  ///
  /// The read is eager rather than lazy so the `#` scope is answerable the
  /// moment the operator types it, and fail-soft so a Core read that refuses
  /// degrades one section instead of withholding the whole surface.
  function openPalette(seed: string): void {
    if (paletteOpen || disposed) return;
    paletteOpen = true;
    paletteQuery = seed;
    paletteCrossLane = null;
    paletteFiles = null;
    const token = (paletteReadToken += 1);
    if (!options.loadPaletteCrossLane) {
      paletteCrossLane = {
        gates: [],
        asks: [],
        unavailable: translate(locale, "d1.palette.crossLane.unavailable", {}),
      };
    }
    render(false);
    mountCommandPalette();
    if (!options.loadPaletteCrossLane) return;
    void options
      .loadPaletteCrossLane()
      .then((answer) => {
        if (disposed || token !== paletteReadToken) return;
        paletteCrossLane = { ...answer, unavailable: null };
        paletteController?.setCrossLane(paletteCrossLane);
      })
      .catch((error: unknown) => {
        if (disposed || token !== paletteReadToken) return;
        // Core's own words, not a client paraphrase.
        paletteCrossLane = {
          gates: [],
          asks: [],
          unavailable: error instanceof Error ? error.message : String(error),
        };
        paletteController?.setCrossLane(paletteCrossLane);
      });
    if (!options.loadPaletteFiles) return;
    void options
      .loadPaletteFiles()
      .then((answer) => {
        if (disposed || token !== paletteReadToken) return;
        paletteFiles = answer;
        paletteController?.setFiles(answer);
      })
      .catch((error: unknown) => {
        if (disposed || token !== paletteReadToken) return;
        // Core's own words for the refusal, never a client paraphrase, and
        // never an empty inventory that would read as "no files here".
        paletteFiles = {
          outcome: {
            state: "rejected",
            reason: error instanceof Error ? error.message : String(error),
          },
          entries: [],
          complete: false,
          loaded: false,
          pendingCommandId: null,
          capabilityAvailable: true,
        };
        paletteController?.setFiles(paletteFiles);
      });
  }

  function closePalette(): void {
    if (!paletteOpen) return;
    paletteReadToken += 1;
    paletteController?.close();
    if (paletteOpen) {
      // No controller was mounted, so no `onClose` will arrive.
      paletteOpen = false;
      paletteQuery = "";
      render(false);
    }
  }

  function togglePalette(seed: string): void {
    if (paletteOpen) closePalette();
    else openPalette(seed);
  }

  function mountAgentMenu(): void {
    if (!agentMenuOpen || disposed) return;
    const anchor = root.querySelector<HTMLButtonElement>("[data-create-lane]");
    if (!anchor) return;
    menuController = renderAgentMenu(
      anchor,
      {
        locale,
        canCreateLane: projection.workspaceEligibility?.canCreateLane === true,
        usesGitIsolation:
          projection.workspaceEligibility?.isGitRepository === true &&
          projection.workspaceEligibility?.hasHead === true,
        probing: discoveryIsProbing(),
        eligibilityDiagnostic: projection.workspaceEligibility?.diagnostic ?? null,
        discoveryDiagnostic: agentDiscoveryDiagnostic,
        selected: newLaneSelection,
        taskDraft: pendingAgentTaskDraft,
        submitting: creatingLane,
        adapters: projection.agentAdapters,
      },
      (selection) => {
        newLaneSelection = selection;
      },
      () => {
        agentMenuOpen = false;
        menuController = null;
        creatingLane = false;
        agentMenuComposing = false;
        agentMenuRefreshDeferred = false;
        if (!remountingAgentMenu) {
          pendingAgentTaskDraft = "";
          newLaneSelection = undefined;
        }
      },
      async (selection, task) => {
        pendingAgentTaskDraft = task;
        creatingLane = true;
        const accepted = await startLane(
          task,
          selection.kind === "native" ? null : selection.agentId,
        );
        creatingLane = false;
        if (!accepted) {
          render(false);
          return false;
        }
        // A Core approval can redraw and remount this async popover before
        // creation completes. Close the current controller explicitly once the
        // accepted request has yielded to that operator decision.
        agentMenuOpen = false;
        menuController?.close();
        menuController = null;
        pendingAgentTaskDraft = "";
        newLaneSelection = undefined;
        return true;
      },
      (task) => {
        pendingAgentTaskDraft = task;
      },
      () => {
        options.onFullSetup?.();
      },
      (isComposing, task) => {
        agentMenuComposing = isComposing;
        pendingAgentTaskDraft = task;
        if (!isComposing && agentMenuRefreshDeferred) {
          agentMenuRefreshDeferred = false;
          queueMicrotask(() => {
            if (!disposed && agentMenuOpen && !agentMenuComposing) render(false);
          });
        }
      },
      () => {
        restartAgentDiscovery();
      },
    );
  }

  function observeAgentDiscoveryResult(result: D1IntentResult): boolean {
    if (!discoveryInFlight || result.pendingCommandId !== null) return false;
    const completed = discoveryInFlight;
    discoveryInFlight = null;
    if (completed.kind === "query") {
      agentQueryComplete = result.outcome.state !== "rejected";
      if (!agentQueryComplete) {
        agentDiscoveryDiagnostic =
          result.outcome.reason ?? translate(locale, "d1.agentMenu.discoveryFailed", {});
        agentDiscoveryComplete = true;
      }
    } else if (result.outcome.state === "rejected") {
      agentDiscoveryDiagnostic =
        result.outcome.reason ?? translate(locale, "d1.agentMenu.probeFailed", {});
    }
    queueMicrotask(advanceAgentDiscovery);
    return true;
  }

  async function dispatchAgentDiscovery(
    command: { kind: "query" } | { kind: "probe"; agentId: string },
  ): Promise<void> {
    if (
      disposed ||
      discoveryDispatching ||
      discoveryInFlight ||
      pollInFlight ||
      sending ||
      pendingCommandId
    ) {
      return;
    }
    discoveryDispatching = true;
    discoveryInFlight = command;
    if (command.kind === "probe") attemptedAgentProbes.add(command.agentId);
    sending = true;
    try {
      const result = await send(
        command.kind === "query"
          ? { type: "query_agent_adapters" }
          : { type: "probe_agent_adapter", agentId: command.agentId },
      );
      if (disposed) return;
      controller.applyResult(result);
    } catch (error: unknown) {
      if (disposed) return;
      const failed = discoveryInFlight;
      discoveryInFlight = null;
      root.dataset.d1Error = String(error);
      errorMessage = String(error);
      agentDiscoveryDiagnostic = String(error);
      if (failed?.kind === "query") agentDiscoveryComplete = true;
      render(false);
    } finally {
      sending = false;
      discoveryDispatching = false;
      if (!disposed) {
        releaseCommandSlotWaiters();
        queueMicrotask(advanceAgentDiscovery);
        schedulePoll();
      }
    }
  }

  function advanceAgentDiscovery(): void {
    if (
      disposed ||
      !agentDiscoveryStarted ||
      agentDiscoveryComplete ||
      discoveryDispatching ||
      discoveryInFlight ||
      pollInFlight ||
      sending ||
      creatingLane ||
      pendingCommandId
    ) {
      return;
    }
    if (!agentQueryComplete) {
      void dispatchAgentDiscovery({ kind: "query" });
      return;
    }
    const next = orderedAgentAdapters(projection.agentAdapters).find(
      (adapter) =>
        adapter.startability === "probe_required" &&
        !attemptedAgentProbes.has(adapter.agentId),
    );
    if (next) {
      void dispatchAgentDiscovery({ kind: "probe", agentId: next.agentId });
      return;
    }
    agentDiscoveryComplete = true;
    render(false);
  }

  function restartAgentDiscovery(): void {
    if (disposed || discoveryIsProbing()) return;
    if (errorMessage === agentDiscoveryDiagnostic) errorMessage = null;
    agentDiscoveryStarted = true;
    agentDiscoveryComplete = false;
    agentQueryComplete = false;
    agentDiscoveryDiagnostic = null;
    discoveryInFlight = null;
    attemptedAgentProbes.clear();
    render(false);
    advanceAgentDiscovery();
  }

  function openAgentMenu(): void {
    if (agentMenuOpen) {
      menuController?.close();
      return;
    }
    agentMenuOpen = true;
    // Adapter discovery is cached for this cockpit lifetime. A second probe is
    // an explicit recovery action, never a side effect of reopening the menu.
    if (!agentDiscoveryStarted) {
      restartAgentDiscovery();
      return;
    }
    mountAgentMenu();
  }

  const render = (focusComposer = false): void => {
    // Replacing the focused textarea while macOS IME owns a composition drops
    // the candidate session. Core facts may advance, but visual remount waits
    // until compositionend commits the draft.
    if (composing || (agentMenuOpen && agentMenuComposing)) {
      if (composing) composerRefreshDeferred = true;
      if (agentMenuOpen && agentMenuComposing) agentMenuRefreshDeferred = true;
      return;
    }
    const reopenAgentMenu = agentMenuOpen;
    if (menuController) {
      remountingAgentMenu = true;
      agentMenuOpen = false;
      menuController.close();
      remountingAgentMenu = false;
      menuController = null;
      agentMenuOpen = reopenAgentMenu;
    }
    const previousComposer = root.querySelector<HTMLTextAreaElement>("[data-composer]");
    const restoreComposerFocus = previousComposer === document.activeElement;
    // An open control popover is rebuilt by every refresh; remember which
    // option held focus so a streaming re-render does not reset arrow-key
    // position.
    const activeControlOptionKey =
      document.activeElement instanceof HTMLElement
        ? (document.activeElement.dataset.controlOptionKey ?? null)
        : null;
    const selectionStart = previousComposer?.selectionStart ?? draft.length;
    const selectionEnd = previousComposer?.selectionEnd ?? selectionStart;
    const frame = document.createElement("section");
    frame.className = "frame d1-frame";
    frame.dataset.screen = "d1-cockpit";
    const showWelcome = shouldShowWelcome();
    if (showWelcome) frame.dataset.d1State = "welcome";

    const focusedAcpSessionId =
      focusedConversation?.kind === "acp" ? focusedConversation.sessionId : null;
    const focusedAcp = focusedAcpSessionId
      ? projection.agentSessions.find(
          (session) => session.sessionId === focusedAcpSessionId,
        )
      : null;
    const focusedAcpOwnsLiveSurface =
      focusedAcp !== null &&
      focusedAcp !== undefined &&
      ["starting", "running", "waiting_approval", "completed"].includes(focusedAcp.status);
    const showRecovery =
      !["live", "gate_queue_clear", "empty"].includes(projection.recovery.state) &&
      !(projection.recovery.state === "agent_stopped" && focusedAcpOwnsLiveSurface);
    // The composer only exists on the work surface, and only Core can say it
    // is editable. The rail's `Work` slot is offered a focus target only when
    // there will actually be one.
    const composerFocusable = !showWelcome && !showRecovery && projection.composer.editable;

    frame.dataset.nativeWindowShell = "true";
    // The picker replaces the open workspace, so it exists only where there is
    // one to replace: never on the no-project Welcome, and never without the
    // host callbacks that actually reach Core.
    const projectPickerAvailable =
      !showWelcome && !!options.onPickProjectFolder && !!options.onOpenWorkspace;
    const topbar = renderCockpitTopbar(projection, locale, showWelcome, options.onNavigate, {
      onToggleCommandPalette: () => togglePalette(""),
      commandPaletteOpen: paletteOpen,
      // The titlebar's dirty marker is the changes chip the design puts on the
      // source block, so it is the review's natural entry: it appears exactly
      // when Core reports uncommitted work. A bound host always gets the
      // control; an absent capability leaves it visible and disabled naming
      // the capability, never hidden.
      onOpenReview: options.workspaceDiff ? () => openReview() : undefined,
      reviewAvailable: reviewAvailable(),
      reviewOpen: centerView === "review",
      // The sync chip is a control wherever the host can carry an action; the
      // chip itself decides push versus fetch from Core's resampled counts and
      // stays disabled-and-labelled when it may not act.
      onSync: options.operatorGit
        ? (action) =>
            runOperatorGit(
              action === "push"
                ? { type: "push", remote: null, setUpstream: false }
                : { type: "fetch", remote: null },
            )
        : undefined,
      syncState: operatorGitState,
      onOpenProjectPicker: projectPickerAvailable
        ? () => openProjectPicker("titlebar")
        : undefined,
      projectPickerOpen: pickerOpen,
    });
    const titlebar = topbar.element;

    const body = document.createElement("div");
    body.className = "d1-body";
    body.dataset.cockpitGrid = "true";
    body.dataset.cockpitLayout = window.innerWidth <= 1100 ? "narrow" : "desktop";
    if (showWelcome) body.classList.add("d1-body-welcome");

    const activity = renderActivityRail(locale, {
      lanesAvailable: !showWelcome,
      lanesOpen: laneRailOpen,
      onNavigate: options.onNavigate,
      onFocusWork: !composerFocusable
        ? undefined
        : () => {
            // Resolved at activation, not at render: the rail is built before
            // the composer node exists in this frame.
            root.querySelector<HTMLTextAreaElement>("[data-composer]")?.focus();
          },
      onToggleLanes: () => {
        laneRailOpen = !laneRailOpen;
        laneRailFocusTarget = laneRailOpen ? "rail" : "toggle";
        render(false);
      },
      settingsOpen,
      onOpenSettings: !options.preferences
        ? undefined
        : () => {
            if (settingsOpen) {
              settingsController?.close();
              return;
            }
            void openSettingsPanel();
          },
    });
    const lanes = renderLaneRail({
      projection,
      locale,
      open: laneRailOpen,
      selectedLaneId,
      onCreateLane: () => {
        if (options.onCreateLane) options.onCreateLane();
        else void openAgentMenu();
      },
      onDismiss: () => {
        laneRailOpen = false;
        laneRailFocusTarget = "toggle";
        render(false);
      },
      onSelectLane: selectLane,
      onRetryAgent: (sessionId, laneId) => {
        selectedLaneId = laneId;
        focusedConversation = conversationForLane(projection, laneId);
        sendIntent({ type: "retry_agent_session", laneId, sessionId });
      },
      collapsed: laneGroupCollapsed,
      onToggleCollapsed: () => {
        laneGroupCollapsed = !laneGroupCollapsed;
        render(false);
      },
      onAddProject: projectPickerAvailable ? () => openProjectPicker("rail") : undefined,
    });
    const workSurface = document.createElement("section");
    workSurface.className = "d1-work-surface";
    if (showWelcome) {
      // Welcome renders only when no workspace is bound, so opening a recent
      // project replaces nothing and needs no switch confirmation — unlike the
      // picker, which always has a workspace to tear down.
      void ensureRecentWork();
      renderWelcomeCenter(
        workSurface,
        locale,
        options.onOpenProject,
        !options.onOpenWorkspace
          ? undefined
          : {
              state: recentWorkInFlight ? { kind: "loading" } : recentState,
              sessions: recentSessions,
              now: Date.now(),
              onOpenRecent: (canonicalRoot) => options.onOpenWorkspace?.(canonicalRoot),
            },
      );
    } else if (showRecovery) {
      renderD6Recovery(
        workSurface,
        projection.recovery,
        recoverD6 ?? (async () => projection.recovery),
        locale,
        options.onOpenProject,
        options.sendD6Intent,
      );
    } else if (centerView === "review") {
      // DiffReview owns the whole centre pane, including its own commit bar.
      // The composer stays mounted below it: reviewing a change and telling
      // the agent what to fix are the same sitting.
      renderDiffReview(
        workSurface,
        reviewProjection ?? {
          // Before the first answer the view renders the pending state rather
          // than an empty tree, which would read as "nothing changed".
          outcome: { state: "pending", reason: null },
          targetLaneId: selectedLaneId,
          source: null,
          entries: [],
          truncated: false,
          loaded: false,
          pendingCommandId: null,
          capabilityAvailable: reviewCapability !== false,
          stale: false,
        },
        locale,
        {
          onRefresh: options.workspaceDiff ? () => readReview() : undefined,
          onClose: () => closeReview(),
          onSelect: (path) => {
            reviewSelectedPath = path;
          },
          selectedPath: reviewSelectedPath,
          actions: !options.operatorGit
            ? undefined
            : {
                state: operatorGitState,
                message: commitMessage,
                pairStopped: commitPairStopped,
                onMessageChange: (next) => {
                  // No re-render: the input already holds the text, and
                  // rebuilding the DOM under a caret would move it.
                  commitMessage = next;
                },
                onStageAll: () => runOperatorGit({ type: "stage", paths: [] }),
                onCommit: () => {
                  pushAfterCommit = false;
                  runOperatorGit({ type: "commit", message: commitMessage });
                },
                onCommitPush: () => {
                  // The flag is set before the command leaves; the push itself
                  // is sent by `advanceCommitPair` only on a `Completed`
                  // commit.
                  pushAfterCommit = true;
                  runOperatorGit({ type: "commit", message: commitMessage });
                },
                onToggleStaged: (path, staged) =>
                  runOperatorGit(
                    staged
                      ? { type: "unstage", paths: [path] }
                      : { type: "stage", paths: [path] },
                  ),
                onPushSetUpstream: () =>
                  runOperatorGit({ type: "push", remote: null, setUpstream: true }),
                onFetch: () => runOperatorGit({ type: "fetch", remote: null }),
              },
        },
      );
    } else {
      const transcriptRegion = document.createElement("section");
      transcriptRegion.className = "d1-transcript";
      transcriptRegion.dataset.centerSequence = "true";
      transcriptRegion.setAttribute("aria-label", translate(locale, "d1.transcript", {}));
      transcriptRegion.setAttribute("role", "log");
      transcriptRegion.setAttribute("aria-live", "polite");
      transcriptRegion.setAttribute("aria-relevant", "additions text");
      transcriptRegion.setAttribute("aria-busy", String(projection.composer.busy));
      transcriptRegion.tabIndex = 0;
      const projectionMatchesSelectedLane = projection.selectedLaneId === selectedLaneId;
      let visibleRows = projectionMatchesSelectedLane
        ? transcript.visible(transcriptRegion.clientHeight || 720, 36)
        : [];
      const acpKinds = new Set<"user" | "assistant">();
      if (focusedAcp) {
        acpKinds.add("user");
        acpKinds.add("assistant");
        appendAcpConversationRows(
          transcriptRegion,
          focusedAcp,
          transcriptAgent(projection, focusedAcp),
          options.resolveContent,
        );
        const conversation = focusedAcp.conversation ?? [];
        if (conversation.length > 0) {
          visibleRows = visibleRows.filter(
            (row) => !duplicatesAcpConversation(row, conversation),
          );
        }
      }
      appendUnavailableTranscriptRows(
        transcriptRegion,
        projection.unavailableFeatures,
        locale,
        acpKinds,
      );
      // A Lane whose Agent session Core confirmed produced these rows too. The
      // ACP conversation is not always published yet, so leaving them
      // unattributed put the shell's name on the agent's own output.
      appendTranscriptRows(
        transcriptRegion,
        visibleRows,
        focusedAcp ? transcriptAgent(projection, focusedAcp) : undefined,
      );
      if (projectionMatchesSelectedLane) {
        appendTypedWorkCards(transcriptRegion, projection.contextDock.checklist, locale);
      }
      if (!projectionMatchesSelectedLane) {
        const switching = document.createElement("p");
        switching.dataset.typedEmpty = "transcript-switching";
        switching.textContent = translate(locale, "d1.transcript.switching", {});
        transcriptRegion.append(switching);
      }
      const liveWork = projectionMatchesSelectedLane ? renderLiveWorkBar(projection, locale) : null;
      if (liveWork) transcriptRegion.append(liveWork);
      workStatusStrip?.dispose();
      const canCancelTurn =
        !composerMutationBlockReason(projection, selectedLaneId) &&
        (focusedAcp
          ? ["starting", "running", "waiting_approval"].includes(focusedAcp.status)
          : Boolean(projection.composer.canCancel && selectedLaneId));
      workStatusStrip = renderWorkStatus(
        workStatusModel(projection, selectedLaneId, canCancelTurn),
        locale,
        busySince ?? Date.now(),
        () => Date.now(),
        () => cancelActiveTurn(),
      );
      transcriptRegion.append(workStatusStrip.element);
      transcriptRegion.addEventListener("scroll", () => {
        const atBottom = transcriptAtBottom(transcriptRegion);
        const first = transcriptRegion.querySelector<HTMLElement>("[data-row-id]")?.dataset.rowId;
        transcript.setFollowLatest(atBottom, first);
        transcriptScrollTop = transcriptRegion.scrollTop;
      });
      if (transcript.newOutputCount > 0) {
        const latest = button(
          translate(locale, "d1.newOutput", { count: String(transcript.newOutputCount) }),
          "newOutput",
        );
        latest.addEventListener("click", () => {
          transcript.setFollowLatest(true);
          render(false);
        });
        transcriptRegion.append(latest);
      }
      workSurface.append(transcriptRegion);
    }

    const permissionHost = document.createElement("div");
    permissionHost.className = "d1-permission-host";
    const projectionMatchesSelectedLane = projection.selectedLaneId === selectedLaneId;
    if (projectionMatchesSelectedLane && projection.permissionDock.request) {
      renderPermissionDock(
        permissionHost,
        projection.permissionDock,
        dispatchPermission,
        locale,
      );
    }

    const composerRegion = document.createElement("section");
    composerRegion.className = "d1-composer";
    const composerLabel = document.createElement("label");
    composerLabel.textContent = projection.composer.busy || composerDispatchQueued
      ? translate(locale, "d1.composer.queue", {})
      : translate(locale, "d1.composer.prompt", {});
    const composer = document.createElement("textarea");
    composer.dataset.composer = "true";
    composer.rows = 3;
    composer.value = draft;
    composer.disabled = !projection.composer.editable;
    // After a deny the prompt asks for the correction instead, naming the
    // agent when Core published a display name for it.
    composer.placeholder = !denyRedirect
      ? translate(locale, "d1.composer.placeholder", {})
      : denyRedirect.agent
        ? translate(locale, "d1.composer.redirect", { agent: denyRedirect.agent })
        : translate(locale, "d1.composer.redirect.generic", {});
    composer.setAttribute("aria-label", translate(locale, "d1.composer.inputLabel", {}));
    composer.addEventListener("compositionstart", () => {
      composing = true;
    });
    composer.addEventListener("compositionend", () => {
      composing = false;
      draft = composer.value;
      if (composerRefreshDeferred) {
        composerRefreshDeferred = false;
        queueMicrotask(() => {
          if (!disposed && !composing) render(false);
        });
      }
    });
    composer.addEventListener("input", () => {
      draft = composer.value;
    });
    composer.addEventListener("keydown", (event) => {
      if (!shouldSubmitComposer(event, composing)) return;
      event.preventDefault();
      submitComposer(composer.value);
    });
    composerLabel.append(composer);
    composerRegion.append(composerLabel);
    const submit = button(translate(locale, "d1.composer.send", {}), "composerSend");
    submit.setAttribute("aria-label", translate(locale, "d1.composer.sendLabel", {}));
    submit.disabled = composer.disabled || composerDispatchQueued;
    submit.addEventListener("click", () => submitComposer(composer.value));
    composerRegion.append(submit);
    const canCancelAcp = focusedAcp
      ? ["starting", "running", "waiting_approval"].includes(focusedAcp.status)
      : false;
    const canCancelFocusedConversation =
      focusedConversation?.kind === "acp"
        ? canCancelAcp
        : Boolean(projection.composer.canCancel && selectedLaneId);
    const mutationBlock = composerMutationBlockReason(projection, selectedLaneId);
    if (!mutationBlock && canCancelFocusedConversation) {
      const cancel = button(translate(locale, "d1.cancel", {}), "cancelTurn");
      cancel.addEventListener("click", () => {
        const commandBlock = composerMutationBlockReason(projection, selectedLaneId);
        const route = conversationForLane(projection, selectedLaneId);
        if (commandBlock) {
          errorMessage = translate(locale, commandBlock, {});
          render(false);
        } else if (route?.kind === "acp") {
          sendIntent({
            type: "cancel_agent_session",
            laneId: route.laneId,
            sessionId: route.sessionId,
          });
        } else if (selectedLaneId) {
          sendIntent({ type: "cancel", laneId: selectedLaneId });
        }
      });
      composerRegion.append(cancel);
    }
    controlsHandle?.dispose();
    controlsHandle = null;
    if (options.sendComposerControl) {
      // The selectors are disabled — never hidden — while the composer is not
      // editable or no workspace is open, and while any command holds the
      // one-command-at-a-time adapter slot.
      controlsHandle = renderComposerControls(
        {
          locale,
          workMode: projection.statusbar.workMode,
          permissionLevel: projection.statusbar.permissionLevel,
          providerId: projection.environment.providerId,
          model: projection.environment.model,
          groups: modelGroups(projection),
          enabled: projection.composer.editable && !showWelcome,
          busy: controlInFlight || sending || Boolean(pendingCommandId),
          open: openControl,
        },
        (next, restoreFocus) => {
          if (next === null && restoreFocus) controlReturnFocus = openControl;
          openControl = next;
          render(false);
        },
        (intent) => sendControl(intent),
      );
      composerRegion.append(controlsHandle.element);
    }
    if (errorMessage) {
      const rejection = document.createElement("p");
      rejection.dataset.d1Rejection = "true";
      rejection.setAttribute("role", "alert");
      rejection.textContent = errorMessage;
      composerRegion.append(rejection);
    }
    if (mutationBlock) {
      const blocked = document.createElement("p");
      blocked.dataset.mutationBlocked = "true";
      blocked.setAttribute("role", "status");
      blocked.textContent = translate(locale, mutationBlock, {});
      composerRegion.append(blocked);
    }
    const main = renderLaneWorkSurface({
      work: workSurface,
      permission: permissionHost,
      composer: composerRegion,
      showWelcome,
      ariaLabel: translate(locale, "d1.workSurface", {}),
    });
    // The selected ACP session supplies typed task/status/output rows, so stale
    // native transcript capability gaps must not contradict the visible facts.
    const contextDockProjection = focusedAcp
      ? {
          ...projection,
          unavailableFeatures: projection.unavailableFeatures.filter(
            ({ id }) => id !== "transcript_user" && id !== "transcript_assistant",
          ),
        }
      : projection;
    const right = renderContextDock(
      contextDockProjection,
      locale,
      projectionMatchesSelectedLane,
    );
    right.dataset.drawerOpen = String(contextDrawerOpen);
    topbar.contextDrawerToggle.setAttribute("aria-expanded", String(contextDrawerOpen));
    right.tabIndex = -1;

    const status = renderStatusbar(projection.statusbar, locale, options.onNavigate);
    const currentFrame = root.querySelector<HTMLElement>('[data-screen="d1-cockpit"]');
    const currentBody = currentFrame?.querySelector<HTMLElement>(":scope > .d1-body");
    const currentActivity = currentBody?.querySelector<HTMLElement>(
      ':scope > [data-shell-landmark="activity-rail"]',
    );
    const currentLanes = currentBody?.querySelector<HTMLElement>(
      ':scope > [data-shell-landmark="lane-rail"]',
    );
    const currentTopbar = currentFrame?.querySelector<HTMLElement>(
      ':scope > [data-shell-landmark="topbar"]',
    );
    const currentMain = currentBody?.querySelector<HTMLElement>(
      ':scope > [data-shell-landmark="lane-work-surface"]',
    );
    const currentRight = currentBody?.querySelector<HTMLElement>(
      ':scope > [data-shell-landmark="context-dock"]',
    );
    const currentStatus = currentFrame?.querySelector<HTMLElement>(
      ':scope > [data-shell-landmark="statusbar"]',
    );
    if (
      !showWelcome &&
      currentFrame &&
      currentBody &&
      currentActivity &&
      currentLanes &&
      currentTopbar &&
      currentMain &&
      currentRight &&
      currentStatus
    ) {
      // These two hover roots control the floating Lane rail. Keeping their
      // identity stable prevents ordered Core refreshes from dropping :hover
      // for a frame and flashing the entire sidebar.
      refreshPersistentRail(currentActivity, activity);
      refreshPersistentRail(currentLanes, lanes);
      if (regionChanged("topbar", titlebar.outerHTML)) currentTopbar.replaceWith(titlebar);
      // The work surface holds the live transcript and the composer, so it is
      // always refreshed; its scroll and focus are restored explicitly below.
      currentMain.replaceWith(main);
      if (regionChanged("dock", right.outerHTML)) currentRight.replaceWith(right);
      if (regionChanged("status", status.outerHTML)) currentStatus.replaceWith(status);
      currentFrame.className = frame.className;
      currentFrame.dataset.nativeWindowShell = frame.dataset.nativeWindowShell;
      currentBody.className = body.className;
      currentBody.dataset.cockpitLayout = body.dataset.cockpitLayout;
    } else {
      if (showWelcome) body.append(activity, main);
      else body.append(activity, lanes, main, right);
      frame.append(titlebar, body, status);
      root.replaceChildren(frame);
      regionSignatures.set("topbar", titlebar.outerHTML);
      regionSignatures.set("dock", right.outerHTML);
      regionSignatures.set("status", status.outerHTML);
    }

    // The dock and the topbar refresh independently, so the toggle must drive
    // the dock that is actually mounted rather than the one built this pass.
    const mountedDock = root.querySelector<HTMLElement>(
      '[data-shell-landmark="context-dock"]',
    );
    if (mountedDock) {
      mountedDock.dataset.drawerOpen = String(contextDrawerOpen);
      topbar.contextDrawerToggle.addEventListener("click", () => {
        contextDrawerOpen = !contextDrawerOpen;
        mountedDock.dataset.drawerOpen = String(contextDrawerOpen);
        topbar.contextDrawerToggle.setAttribute("aria-expanded", String(contextDrawerOpen));
        topbar.contextDrawerToggle.classList.toggle("on", contextDrawerOpen);
        if (contextDrawerOpen) mountedDock.focus();
      });
    }

    const mountedTranscript = root.querySelector<HTMLElement>(".d1-transcript");
    if (mountedTranscript) {
      // Following readers stay pinned to the newest output; readers in
      // history keep the exact position they scrolled to.
      mountedTranscript.scrollTop = transcript.followLatest
        ? mountedTranscript.scrollHeight
        : transcriptScrollTop;
    }

    if (agentMenuOpen) mountAgentMenu();
    // The rail is rebuilt every render, so a still-open overlay must be
    // re-anchored to the gear that actually exists in this frame.
    if (settingsOpen && !settingsController) mountSettingsPanel();
    // Same for the project picker: a full-frame rebuild detaches its portal,
    // and its anchor (the titlebar selector or the rail footer) was rebuilt
    // too, so it re-anchors to the node that exists in this frame.
    if (pickerOpen && !pickerController?.root.isConnected) {
      remountingPicker = true;
      pickerController?.close();
      remountingPicker = false;
      pickerController = null;
      mountProjectPicker();
    }
    // A full-frame rebuild detaches the palette portal. Remount it against the
    // frame that exists now, carrying the operator's query so the search does
    // not restart under their cursor. The index itself is re-read from the
    // projection this render just rendered.
    if (paletteOpen && !paletteController?.root.isConnected) {
      remountingPalette = true;
      paletteController?.dispose();
      paletteController = null;
      remountingPalette = false;
      mountCommandPalette();
    }

    // Control popovers and pills are rebuilt each render, so keyboard focus
    // is model state: an open popover regains its focused (else selected,
    // else first) option; a just-closed selector hands focus back to its pill.
    const controlFocusTarget = openControl
      ? ((activeControlOptionKey
          ? root.querySelector<HTMLElement>(
              `[data-control-popover="${openControl}"] [data-control-option-key="${activeControlOptionKey}"]`,
            )
          : null) ??
        root.querySelector<HTMLElement>(
          `[data-control-popover="${openControl}"] [aria-selected="true"]`,
        ) ??
        root.querySelector<HTMLElement>(
          `[data-control-popover="${openControl}"] [data-control-option]`,
        ) ??
        root.querySelector<HTMLElement>(`[data-control-popover="${openControl}"]`))
      : controlReturnFocus
        ? root.querySelector<HTMLElement>(`[data-control-toggle="${controlReturnFocus}"]`)
        : null;
    // The command palette is modal: while it is open it owns the caret, so a
    // Core refresh must not hand focus back to the composer underneath it —
    // and opening it over a focused composer must take focus in the first
    // place. Re-focusing an already-focused input is a no-op, so a streaming
    // refresh cannot reset the operator's caret inside the query either.
    const paletteInput = paletteOpen
      ? root.querySelector<HTMLInputElement>("[data-palette-input]")
      : null;
    if (paletteInput) {
      if (document.activeElement !== paletteInput) paletteInput.focus();
    } else if (laneRailFocusTarget) {
      const focusTarget =
        laneRailFocusTarget === "rail"
          ? root.querySelector<HTMLElement>("#d1-lane-rail [data-create-lane]")
          : root.querySelector<HTMLElement>("[data-lanes-toggle]");
      laneRailFocusTarget = null;
      focusTarget?.focus();
    } else if (controlFocusTarget) {
      if (!openControl) controlReturnFocus = null;
      controlFocusTarget.focus();
    } else if (focusComposer || restoreComposerFocus) {
      const nextComposer = root.querySelector<HTMLTextAreaElement>("[data-composer]");
      nextComposer?.focus();
      nextComposer?.setSelectionRange(selectionStart, selectionEnd);
    }
  };

  render(true);
  // One no-traffic read at mount, so the review entry points know whether Core
  // publishes structured diff rows before anyone clicks one. It sends no Core
  // command, so it costs nothing on a Core that does not have the capability.
  ensureReviewCapability();
  // The same at-mount, no-traffic read for the action side, so the commit bar
  // and the titlebar sync chip know whether they may act before anyone presses
  // one of them.
  readOperatorGit();
  if (options.poll !== false) {
    if (options.onCoreWake) {
      // A host push replaces the drain timer outright: reading on the wake
      // removes the average half-interval the timer added to every reply.
      coreWakeActive = true;
      unsubscribeCoreWake = options.onCoreWake(() => drainOnce());
    } else {
      schedulePoll();
    }
  }
  return controller;
}
