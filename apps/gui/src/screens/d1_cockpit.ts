import { translate, type MessageKey } from "../i18n/catalog";
import {
  renderActivityRail,
  type D1RailDestination,
  type D1RailDestinationState,
} from "../components/activity_rail";
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
import {
  ALL_STATUSBAR_AMBIENT_VISIBLE,
  STATUSBAR_AMBIENT_SEGMENTS,
  renderStatusbar,
  type StatusbarAmbientSegment,
  type StatusbarAmbientVisibility,
} from "../components/statusbar";
import {
  DOCK_TABS,
  isDockTabLive,
  renderContextDock,
  type DockTab,
} from "../components/context_dock";
import { renderLaneRail, type LaneSidebarMode } from "../components/lane_rail";
import { appendOrderedTranscriptRows } from "../components/transcript_rows";
import {
  IDLE_TRANSCRIPT_ROWS,
  TRANSCRIPT_ROWS_CAPABILITY,
  type TranscriptRowsProjection,
} from "../models/transcript_rows";
import {
  IDLE_WORKSPACE_FILE,
  WORKSPACE_FILE_READS_CAPABILITY,
  type WorkspaceFileProjection,
} from "../models/workspace_file";
import {
  LAYOUT_PREFERENCES_CAPABILITY,
  IDLE_LAYOUT_PREFERENCES,
  type LayoutPreferencePatch,
  type LayoutPreferencesProjection,
} from "../models/layout_preferences";
import { cycledLaneId, renderLaneTabs } from "../components/lane_tabs";
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
import { renderEvidenceView } from "./evidence_view";
import {
  ABSENT_EVIDENCE_CONTENT,
  PENDING_EVIDENCE_ARCHIVE,
  type EvidenceArchiveProjection,
  type EvidenceContentProjection,
} from "../models/evidence";
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

/**
 * `D-SIDEBAR`'s hover-peek close delay.
 *
 * The decision specifies "~700ms": long enough that a pointer travelling past
 * the hot zone does not make the sidebar blink, short enough that it is out of
 * the way by the time the operator is reading again.
 */
const LANE_PEEK_CLOSE_MS = 700;

/**
 * The centre view's own title, taken from the rail slot that opens it.
 *
 * One label per destination, shared between the rail and the view head: a
 * second string here is how a tooltip and a heading start naming the same
 * screen differently.
 */
const SECONDARY_VIEW_LABELS = {
  d2: "d1.activity.decisions",
  d10: "d1.activity.laneMonitor",
  d12: "d1.activity.integrationGate",
  d13: "d1.activity.fleet",
  d14: "d1.activity.audit",
} as const satisfies Record<"d2" | "d10" | "d12" | "d13" | "d14", MessageKey>;

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

/**
 * Which view owns D1's centre pane.
 *
 * `transcript` is the cockpit's home surface. Everything else is a *view over
 * it*, not a route: the titlebar, both rails, the context dock, the composer
 * and the statusbar stay mounted, the composer keeps addressing the selected
 * Lane, and closing returns to the conversation rather than navigating.
 */
export type D1CenterView = "transcript" | D1RailDestination;

/** The five D-screens the shell mounts inside the centre pane. */
export type D1SecondaryRoute = "d2" | "d10" | "d12" | "d13" | "d14";

export interface D1Controller {
  applyProjection: (projection: D1CockpitProjection) => void;
  applyResult: (result: D1IntentResult) => void;
  transcript: BoundedTranscript;
  /**
   * Switches the centre pane. `arg` is the one Core id the destination
   * preselects (a D2 decision, a D12 gate, a D14 `kind:id` audit scope); the
   * screen still re-reads its own Core projection before it renders.
   */
  openCenterView: (view: D1CenterView, arg?: string) => void;
  /** Returns the centre pane to the transcript. */
  closeCenterView: () => void;
  /**
   * Selects one Lane and returns the centre pane to the conversation.
   *
   * The single Lane hand-off a centre view has: D10's `Attach` and D13's node
   * drill both land here, and it goes through the same `selectLane` the Lane
   * rail, the tab strip and `⌃⇥` use, followed by the router's own
   * `conversation` route. A second selection path is how two surfaces start
   * disagreeing about which Lane the composer is addressing.
   */
  selectLane: (laneId: string) => void;
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

/**
 * The New Lane popover's draft: the agent the operator picked (`null` is the
 * built-in runtime, which is the popover's own default) and the task they
 * typed. Presentation state the cockpit owns, handed across the D4 round trip.
 */
export interface NewLaneDraft {
  agentId: string | null;
  task: string;
}

export interface D1RenderOptions {
  onOpenProject?: () => void | Promise<void>;
  onCreateLane?: () => void;
  /**
   * Opens the full Lane wizard (D4) on the popover's current draft.
   *
   * The draft travels because the operator already typed it: "Full setup…" is
   * the same intention with more steps, not a restart. Core's
   * `StarterLaneRequest` carries neither an agent binding nor a task, so what
   * D4 can do with the draft is presentation — the seam and its contract gap
   * are documented on the wizard's own seed.
   */
  onFullSetup?: (draft: NewLaneDraft) => void;
  /**
   * Reopens the New Lane popover on a draft the caller is returning with —
   * the D4 wizard's Cancel, which must not silently throw away what the
   * operator typed before they went looking for the full form.
   */
  newLaneDraft?: NewLaneDraft;
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
   * Reads one page of the Core workspace file inventory.
   *
   * Two surfaces share it: the palette's `~` scope, which asks for the whole
   * tree, and the context dock's Files tab, which asks per directory —
   * `prefix` is the `/`-terminated directory Core scopes the page to, and
   * `null` is the workspace root.
   *
   * The read belongs to the shell because it is a Core command, not a cockpit
   * projection. Absent while no host is bound, which states the section as
   * unavailable rather than showing an empty one — and the client never walks
   * the workspace itself, which is outside the client boundary and would
   * bypass Core's permission gate (GUI-CORE-022).
   */
  loadWorkspaceFiles?: (prefix: string | null) => Promise<PaletteWorkspaceFiles>;
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
  /**
   * The Core-owned evidence archive behind the EvidenceView (GUI-CORE-025).
   *
   * `read` is the no-traffic projection read: the cockpit calls it once to
   * learn whether Core published `runtime.evidence_reads` at all, and again on
   * each ordered wake while the view is open, to learn whether Core recorded
   * evidence since the loaded pages were read. `query` sends one
   * `QueryEvidence`; `loadOlder` sends the next page through Core's own opaque
   * cursor; `content` sends one `ReadEvidenceContent`.
   *
   * Absent while no host is bound, which renders every evidence entry point
   * disabled-and-labelled rather than opening a view that can never fill.
   */
  evidence?: {
    read: () => Promise<EvidenceArchiveProjection>;
    query: (laneId: string | null, kinds: string[]) => Promise<EvidenceArchiveProjection>;
    loadOlder: () => Promise<EvidenceArchiveProjection>;
    content: (evidenceId: string) => Promise<EvidenceContentProjection>;
  };
  /**
   * Mounts one of the five secondary D-screens inside the cockpit's centre
   * pane (`D-RAILNAV`, resolved by the `0.3.4` plan in favour of the D1
   * flagship's own in-page view switching).
   *
   * The shell owns *what* is mounted, because each screen is a Core read this
   * client boundary performs; the cockpit owns *where* — the centre pane, the
   * chrome around it, the Close control, and the `Esc` return. Absent while no
   * host is bound, which makes every rail destination fall back to the shell's
   * own `onNavigate` route rather than opening a pane nothing can fill.
   *
   * `container` is a stable node: the cockpit keeps it across ordered Core
   * refreshes, so a mounted screen holds its own selection and filters instead
   * of being rebuilt under the operator on every wake.
   */
  secondaryViews?: {
    mount: (
      route: D1SecondaryRoute,
      container: HTMLElement,
      arg: string | null,
    ) => void | Promise<void>;
  };
  /**
   * The Lane sidebar's `D-SIDEBAR` mode before Core answers, and the mode a
   * host that publishes no layout record keeps. Defaults to the decision's own
   * default, `floating`.
   *
   * With `layout` bound this is only the frame drawn *before* the first read
   * lands: from then on the mode is Core's published
   * `UiLayoutPreferences.lane_sidebar_mode`, re-read on every wake, so the
   * webview holds no copy of the operator's layout beyond the view it is
   * rendering. Without the capability the mode stays session-local — the G3
   * behaviour — and the pin says so.
   *
   * The pinned column's width is still fixed at the design's default
   * (`--rail-left`, 218px). The 176–360 drag the token documents has no field
   * in `UiLayoutPreferences` — the record carries the mode and the hidden
   * statusbar segments, and nothing else — so a draggable column would have to
   * keep its width in the client, which is the second preference model the
   * contract forbids. It stays unimplemented until Core publishes a width.
   */
  laneSidebarMode?: LaneSidebarMode;
  /**
   * The ordered owner-scoped transcript (`runtime.transcript_rows`, C8,
   * GUI-CORE-009).
   *
   * `read` is the no-traffic projection the transcript reads for the
   * capability and for the rows it already holds; `query` reads the newest
   * page for one scope; `loadOlder` pages backwards through Core's own cursor.
   * Absent while no host is bound, which keeps the two unavailable rows the
   * cockpit has always drawn rather than showing an empty conversation.
   */
  transcriptRows?: {
    read: () => Promise<TranscriptRowsProjection>;
    query: (laneId: string | null) => Promise<TranscriptRowsProjection>;
    loadOlder: (laneId: string | null) => Promise<TranscriptRowsProjection>;
  };
  /**
   * One workspace file's content (`runtime.workspace_file_reads`, C9).
   *
   * `read` is the no-traffic projection the dock reads for the capability
   * before it offers anything; `open` sends one `ReadWorkspaceFile` for a
   * target-relative path in the workspace root (`laneId: null`) or in one
   * Lane's worktree. Absent while no host is bound, which keeps the
   * inspector's Open disabled-and-labelled rather than opening an editor that
   * can never fill.
   *
   * Core owns the byte bound, the permission gate, and the path validator;
   * the cockpit renders the three bodies and Core's refusal, and never reads a
   * path itself.
   */
  workspaceFile?: {
    read: () => Promise<WorkspaceFileProjection>;
    open: (laneId: string | null, path: string) => Promise<WorkspaceFileProjection>;
  };
  /**
   * The Core-owned cockpit layout record (`ui.layout_preferences`, C5).
   *
   * `read` is a no-traffic projection: the cockpit calls it at mount and on
   * every ordered wake, which is what makes Core the single authority for the
   * sidebar mode and the statusbar's hidden segments. `set` sends one
   * `SetUiLayoutPreferences` carrying only the axis the operator changed, and
   * the rendered layout moves when Core's answer comes back — never
   * optimistically, because a `persisted: false` answer is a different fact
   * from a saved one and the operator has to see which they got.
   */
  layout?: {
    read: () => Promise<LayoutPreferencesProjection>;
    set: (patch: LayoutPreferencePatch) => Promise<LayoutPreferencesProjection>;
  };
  /** Opens D14 scoped to one audit object, for EvidenceView's footer. */
  onOpenAuditTrail?: (scope: { kind: string; id: string }) => void;
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
   * Which context-dock panel is open (`Environment`, `Files`, `Diff`).
   *
   * **Seam.** In memory, deliberately and permanently: the dock tab is a
   * posture the operator takes for one look, not a preference, so it is
   * neither written to `localStorage` — the frontend contract makes Core the
   * single preference authority — nor part of `C5`'s `UiLayoutPreferences`.
   * A cockpit that reopens on Environment is the design's own default.
   */
  let dockTab: DockTab = "environment";
  /**
   * Core's workspace-inventory pages, keyed by the prefix each answered.
   *
   * `""` is the workspace root and `"crates/"` is one opened directory.
   * `QueryWorkspaceFiles` is prefix-scoped and bounded, so the Files tab reads
   * one page per directory the operator opens rather than one truncated page
   * for the whole tree. The map is presentation cache over rows Core
   * published; nothing here is derived, merged, or re-sorted.
   */
  const dockFilePages = new Map<string, PaletteWorkspaceFiles>();
  /** Prefixes with a read in flight, so a second click cannot stack reads. */
  const dockFileReads = new Set<string>();
  const dockFilesExpanded = new Set<string>();
  let dockFileSelected: string | null = null;
  /**
   * Core's last answered file (`runtime.workspace_file_reads`, C9).
   *
   * Never reduced into a view and never cached per path: the dock re-reads a
   * file it reopens, because bytes read a minute ago are not the file now, and
   * a cache would let the inspector show a stale body under a fresh path.
   */
  let workspaceFileAnswer: WorkspaceFileProjection = IDLE_WORKSPACE_FILE;
  /** True while one read is out, so a second click cannot stack reads. */
  let workspaceFileInFlight = false;
  /** Paths whose diff rows the operator opened in the Diff tab. */
  const dockDiffExpanded = new Set<string>();
  let dockDiffSelected: string | null = null;
  /**
   * Which view owns D1's centre pane.
   *
   * DiffReview is a *view* inside the cockpit, not a route: the design
   * registers it as a D1 secondary surface, and `D-RAILNAV` keeps the activity
   * rail pointed at the standalone D-screens. Closing it returns to the
   * transcript without a navigation, so the operator never loses the
   * conversation they were reviewing for.
   */
  let centerView: D1CenterView = "transcript";
  /**
   * The one Core id the open secondary view was asked to preselect, and the
   * key the mount is memoised under. A refresh must not re-mount the screen
   * (it would discard its selection and re-read Core); a *different* route or
   * argument must.
   */
  let secondaryArg: string | null = null;
  let secondaryMountKey: string | null = null;
  /** The stable host node the secondary screen renders into. */
  let secondaryBody: HTMLElement | null = null;
  let secondaryShell: HTMLElement | null = null;
  /// Core's own words when a secondary read was refused, rendered in place of
  /// the screen rather than leaving the pane blank.
  let secondaryError: string | null = null;
  /**
   * The Lane sidebar mode (`D-SIDEBAR`) this frame draws.
   *
   * Core's published record once `layout.read` has answered — re-read on every
   * wake rather than remembered — and the caller's default until then, or for
   * the whole session on a Core that publishes no layout record.
   */
  let laneSidebarMode: LaneSidebarMode = options.laneSidebarMode ?? "floating";
  /** Core's last transcript answer, or the idle projection before the first. */
  let transcriptRowsProjection: TranscriptRowsProjection = IDLE_TRANSCRIPT_ROWS;
  /** True while a transcript read is out, so a wake cannot stack reads. */
  let transcriptRowsInFlight = false;
  /**
   * The scope the last transcript read was *issued* for.
   *
   * Tracked here rather than read back from the projection's `scopeLaneId`,
   * which is the host's echo of what it read: a client must not depend on an
   * echo to decide whether it has already asked. `undefined` means nothing has
   * been asked yet, which differs from "asked unscoped" (`null`).
   */
  let transcriptRowsScope: string | null | undefined = undefined;
  /**
   * The layout record exactly as Core last published it.
   *
   * Held for one purpose only: rendering `persisted` and Core's diagnostics.
   * The two *values* it carries are copied into `laneSidebarMode` and
   * `statusbarAmbient` on arrival, and both are re-derived from the next read,
   * so nothing here outlives the projection it came from.
   */
  let layoutRecord: LayoutPreferencesProjection = IDLE_LAYOUT_PREFERENCES;
  /** True while a layout command is out, so a wake cannot stack patches. */
  let layoutCommandInFlight = false;
  /**
   * Focus mode (`⌘.`).
   *
   * `D-SIDEBAR` gives it its meaning: "focus 专注模式覆盖此偏好 → 两侧强制
   * hover 浮窗(退出恢复)" — focus overrides the sidebar preference, both sides
   * become hover panels, and leaving restores what the operator had. So this
   * flag never *writes* `laneSidebarMode`: it shadows it for as long as it is
   * on, which is why exiting needs no saved copy.
   *
   * In memory, like the sidebar mode and the statusbar's ambient set, and for
   * the same reason: the frontend contract makes Core the single preference
   * authority. Unlike those two it is also not a preference — it is a posture
   * the operator takes for a few minutes — so it is not part of the `C5`
   * `UiLayoutPreferences` seam.
   */
  let focusMode = false;
  /** True while the floating sidebar is peeked open. */
  let laneRailPeek = false;
  /** The same peek, for the context dock's right-edge zone under focus mode. */
  let dockPeek = false;
  let dockPeekTimer: number | null = null;
  /** The design's ~700 ms peek delay, so a pointer crossing it does not slam. */
  let lanePeekTimer: number | null = null;
  /**
   * Which ambient statusbar segments are shown (`D-STATUSBAR`).
   *
   * Derived from Core's `hidden_statusbar_segments` once the layout record has
   * answered: a segment is shown unless the operator hid it. Names Core stored
   * that this build's vocabulary does not contain are kept in the record and
   * ignored here rather than dropped, because dropping them on the next write
   * would quietly unhide a newer client's segment.
   */
  let statusbarAmbient: StatusbarAmbientVisibility = { ...ALL_STATUSBAR_AMBIENT_VISIBLE };
  let statusbarConfigOpen = false;
  /** Core's last diff answer, or null before the first read. */
  let reviewProjection: WorkspaceDiffProjection | null = null;
  /** Selected file. Presentation state: an ordered refresh must not move it. */
  let reviewSelectedPath: string | null = null;
  /** True while a `QueryWorkspaceDiff` is out, so a wake cannot stack reads. */
  let reviewReadInFlight = false;
  /**
   * The target the last diff read was *issued* for.
   *
   * Tracked here rather than read back from `reviewProjection.targetLaneId`,
   * because that field is Core's echo of the resolved target and a client must
   * not depend on an echo to decide whether it has already asked. `undefined`
   * means nothing has been asked yet, which is a different state from "asked
   * about the workspace root" (`null`).
   */
  let reviewReadTarget: string | null | undefined = undefined;
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
  /* ---- EvidenceView state (GUI-CORE-025) ---- */
  /** Core's last archive answer, or null before the first read. */
  let evidenceProjection: EvidenceArchiveProjection | null = null;
  /** Selected row. Presentation state: an ordered refresh must not move it. */
  let evidenceSelectedId: string | null = null;
  /**
   * The chip filter, sent to Core as `EvidenceQuery.kinds`.
   *
   * A filter change is a *new query*, not a local narrowing: Core applies the
   * filter before it cuts the page, so a client narrowing what it holds could
   * not tell whether a match sits on a page it never loaded.
   */
  let evidenceKinds: string[] = [];
  /**
   * The search box. Purely local, and the box says so: Core exposes no
   * evidence search, so this narrows the loaded rows and nothing else.
   */
  let evidenceSearch = "";
  /**
   * Content answers keyed by the row Core echoed them for, for this view's
   * lifetime only.
   *
   * The cache exists so re-selecting a row costs no second Core read; it is
   * dropped when the view closes, because a cached body outliving the view
   * could be shown against an archive that has since moved.
   */
  let evidenceContent = new Map<string, EvidenceContentProjection>();
  /** True while a `QueryEvidence` is out, so a wake cannot stack reads. */
  let evidenceReadInFlight = false;
  /** True while a `ReadEvidenceContent` is out. One row's bytes at a time. */
  let evidenceContentInFlight = false;
  /**
   * Whether Core published `runtime.evidence_reads`. Null until the first
   * no-traffic read answers; the entry points stay disabled until then rather
   * than promising a view that may not exist.
   */
  let evidenceCapability: boolean | null = null;
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
  /**
   * Whether the pinned column is showing. Pinned opens with it: a mode whose
   * whole point is a permanent column would be a strange thing to switch into
   * and see nothing. Floating tracks `laneRailPeek` instead.
   */
  let laneRailOpen = options.laneSidebarMode === "pinned";
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
  /**
   * Which control the New Lane popover is anchored to.
   *
   * Two controls open the same popover — the Lane rail's per-project `＋` and
   * the tab strip's trailing `＋` — and in floating sidebar mode the rail's is
   * off screen. Anchoring is therefore state rather than a fixed selector:
   * a popover that opened beside a hidden sidebar would appear to come from
   * nowhere, and `Escape` would hand focus back to something invisible.
   */
  let newLaneAnchor: "rail" | "tabs" = "rail";
  let remountingAgentMenu = false;
  let agentMenuComposing = false;
  let agentMenuRefreshDeferred = false;
  let newLaneSelection: AgentMenuSelection | undefined = options.newLaneDraft
    ? options.newLaneDraft.agentId === null
      ? { kind: "native" }
      : { kind: "acp", agentId: options.newLaneDraft.agentId }
    : undefined;
  let pendingAgentTaskDraft = options.newLaneDraft?.task ?? "";
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
  /**
   * Selectors for the surfaces that own their own `Escape`.
   *
   * Each of these consumes the key and stops propagation, so the window-level
   * handler below would normally never see it. They are listed anyway because
   * "never see it" depends on where focus is: a popover open while focus sits
   * elsewhere would otherwise let a stray `Escape` close the view behind it.
   */
  const ESCAPE_OWNERS =
    "[data-settings-panel], [data-new-lane-popover], [data-control-popover]," +
    " [data-command-palette], [data-project-picker], [data-permission-dock]";

  /**
   * The cockpit's single `Escape` handler, in one explicit priority order.
   *
   * There is exactly one because the alternative — a second listener per new
   * surface — is how a key ends up doing two things at once, and because the
   * order *is* the contract:
   *
   * 1. an IME composition owns the key outright;
   * 2. an open overlay owns it (settings, palette, popovers, the project
   *    picker, the permission dock) — a decision the operator is being asked
   *    to make outranks any navigation;
   * 3. the floating Lane sidebar's peek, which is the most transient thing
   *    on screen and the one `D-SIDEBAR` binds `Escape` to;
   * 4. the composer's cancel-turn binding, which the Live Work strip names
   *    and which stops real work rather than moving a view;
   * 5. the centre view's return path — only then, and only when the view
   *    actually owns the centre pane.
   */
  const handleEscape = (event: KeyboardEvent): void => {
    if (event.key !== "Escape" || event.repeat || composing) return;
    const active = document.activeElement;
    if (active instanceof HTMLElement && active.closest(ESCAPE_OWNERS)) return;
    if (paletteOpen || settingsOpen || pickerOpen || agentMenuOpen || openControl !== null) return;
    if (laneRailPeek) {
      event.preventDefault();
      hideLanePeek();
      return;
    }
    if (projection.composer.busy && root.querySelector("[data-work-cancel]")) {
      event.preventDefault();
      cancelActiveTurn();
      return;
    }
    if (centerView !== "transcript") {
      event.preventDefault();
      closeCenterView();
      return;
    }
    // 6. focus mode, last of all: it hides no decision and interrupts no work,
    //    so every surface above owns the key first.
    if (!focusMode) return;
    event.preventDefault();
    setFocusMode(false);
  };

  /**
   * `⌘.` / `⌃.` toggles focus mode.
   *
   * The design's own binding, in both the keyboard registry ("Focus mode ⌘.")
   * and the titlebar control's tooltip. It stands down while a modal popover
   * owns focus, exactly as the other view chords do — changing the layout out
   * from under an open dialog would move the thing the operator is reading.
   */
  const handleFocusShortcut = (event: KeyboardEvent): void => {
    if (event.repeat || composing) return;
    if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey) return;
    if (event.key !== ".") return;
    const active = document.activeElement;
    if (active instanceof HTMLElement && active.closest(ESCAPE_OWNERS)) return;
    if (paletteOpen || settingsOpen || pickerOpen || agentMenuOpen || openControl !== null) return;
    event.preventDefault();
    setFocusMode(!focusMode);
  };

  /**
   * What Core says about carrying another Lane in this workspace.
   *
   * Three states, not two: Core said yes, Core said no and gave a reason, and
   * Core published no eligibility at all. The last is not a refusal, so it is
   * never worded as one.
   */
  function laneCreationState(): { available: boolean; reason: string | null } {
    const eligibility = projection.workspaceEligibility;
    if (!eligibility) return { available: false, reason: null };
    return {
      available: eligibility.canCreateLane,
      reason: eligibility.canCreateLane ? null : eligibility.diagnostic,
    };
  }

  /**
   * `⌘L` / `⌃L` opens the New Lane popover.
   *
   * The design's keyboard registry binds "New lane / delegate" to `⌘L`, and
   * the workspace rail's `＋` carries `⌘L` in its own tooltip. The chord was
   * unbound in this shell.
   *
   * It stands down while a modal popover owns focus — the same guard the
   * palette, review and evidence chords use — and while Core says this
   * workspace cannot carry a Lane, which is the condition the palette row
   * fails closed on with Core's own sentence. Failing closed here rather than
   * opening a popover whose Create button is already disabled keeps one answer
   * to "can I make a Lane" instead of two.
   */
  const handleNewLaneShortcut = (event: KeyboardEvent): void => {
    if (event.repeat || composing) return;
    if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey) return;
    if (event.key.toLowerCase() !== "l") return;
    const active = document.activeElement;
    if (
      active instanceof HTMLElement &&
      active.closest(
        "[data-settings-panel], [data-new-lane-popover], [data-control-popover], [data-command-palette], [data-project-picker], [data-permission-dock]",
      )
    ) {
      return;
    }
    if (!laneCreationState().available) return;
    event.preventDefault();
    if (options.onCreateLane) {
      options.onCreateLane();
      return;
    }
    openAgentMenu("tabs");
  };

  /**
   * `⌘G` / `⌃G` opens the decision queue.
   *
   * The design's keyboard registry puts the decision centre on `⌘G`, and the
   * chord was unbound in this shell. It toggles like the other view chords: a
   * second press returns to the transcript.
   */
  const handleDecisionsShortcut = (event: KeyboardEvent): void => {
    if (event.repeat || composing) return;
    if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey) return;
    if (event.key.toLowerCase() !== "g") return;
    const active = document.activeElement;
    if (active instanceof HTMLElement && active.closest(ESCAPE_OWNERS)) return;
    if (!options.secondaryViews && !options.onNavigate) return;
    event.preventDefault();
    openCenterView("d2");
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
  /**
   * `⌘E` / `⌃E` toggles EvidenceView, mirroring the review chord exactly.
   *
   * `⌘E` was unbound in this shell: the only cockpit chords are `⌘K` and `⌃P`
   * for the palette, `⌘O` for the folder picker, and `⌘R` for DiffReview. The
   * same guards apply — it stands down while a modal popover owns focus, and
   * while no host is bound or Core published no evidence archive, which is the
   * condition the visible entry points fail closed on.
   *
   * One near-collision is guarded explicitly: the permission dock binds a
   * *bare* `e` to its Edit action and does not inspect modifiers, so a `⌘E`
   * pressed with the dock focused would reach both handlers. Edit is disabled
   * under `GUI-CORE-003` today, so nothing happens there yet — which is
   * exactly why it is guarded now rather than when it starts firing twice. A
   * decision the operator is being asked to make also outranks opening a
   * read-only archive.
   */
  const handleEvidenceShortcut = (event: KeyboardEvent): void => {
    if (event.repeat || composing) return;
    if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey) return;
    if (event.key.toLowerCase() !== "e") return;
    const active = document.activeElement;
    if (
      active instanceof HTMLElement &&
      active.closest(
        "[data-settings-panel], [data-new-lane-popover], [data-control-popover], [data-command-palette], [data-permission-dock]",
      )
    ) {
      return;
    }
    if (!evidenceAvailable()) return;
    event.preventDefault();
    if (centerView === "evidence") closeEvidence();
    else openEvidence();
  };
  /**
   * `⌘F` focuses the evidence search box while the view is open.
   *
   * The registered `.evbar .search` carries that chord in the design. It is
   * bound only while EvidenceView owns the centre pane, so every other surface
   * keeps the webview's own behaviour.
   */
  const handleEvidenceSearchShortcut = (event: KeyboardEvent): void => {
    if (centerView !== "evidence" || composing) return;
    if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey) return;
    if (event.key.toLowerCase() !== "f") return;
    const field = root.querySelector<HTMLInputElement>("[data-evidence-search]");
    if (!field || field.disabled) return;
    event.preventDefault();
    field.focus();
    field.select();
  };
  /**
   * `⌃⇥` / `⌃⇧⇥` move to the next and previous Lane.
   *
   * The design's keyboard registry (`GUI/gui-settings.jsx` `SecKeyboard`) binds
   * "Next / previous lane" to exactly this pair. The chord goes through
   * `selectLane`, the same path the Lane rail and the tab strip use, so a
   * switch by keyboard and a switch by pointer are one code path.
   *
   * It stands down in the same places `handleEscape` does: an IME composition
   * owns the key outright, and an open overlay — the palette, the settings
   * panel, the picker, the New Lane popover, a control popover, the permission
   * dock — owns `Tab` for its own focus ring. Moving the conversation out from
   * under a decision the operator is being asked to make is the one thing this
   * chord must never do.
   */
  const handleLaneCycleShortcut = (event: KeyboardEvent): void => {
    if (event.key !== "Tab" || composing) return;
    if (!event.ctrlKey || event.metaKey || event.altKey) return;
    if (paletteOpen || settingsOpen || pickerOpen || agentMenuOpen || openControl !== null) return;
    const active = document.activeElement;
    if (active instanceof HTMLElement && active.closest(ESCAPE_OWNERS)) return;
    const target = cycledLaneId(
      projection.lanes.map((lane) => lane.id),
      selectedLaneId,
      event.shiftKey ? "previous" : "next",
    );
    // One Lane is already current and zero Lanes have nothing to cycle; in both
    // cases the webview keeps its own `Tab`.
    if (target === null || target === selectedLaneId) return;
    event.preventDefault();
    selectLane(target);
  };
  /**
   * The context dock's own chords: `⌥⌘E`, `⌘P`, `⌘D`.
   *
   * The design's `TABMETA` binds one chord per dock tab. Three of the six are
   * bound here, and the reasons the other three are not are worth stating
   * because they are collisions, not omissions:
   *
   * - `⌥⌘E` (Environment) is free: `⌘E` is EvidenceView's and that handler
   *   stands down whenever `altKey` is held, so the two never both fire.
   * - `⌘P` (Files) is bound on the *meta* key only. The command palette's
   *   scope chord is `⌃P` and keeps it, so off macOS — where `⌘` reads as
   *   `⌃` everywhere else in this cockpit — `⌃P` still opens the palette and
   *   the Files tab is reached from the strip.
   * - `⌘D` (Diff) was unbound in this shell.
   * - `⌘J` (Terminal) and `⌘/` (Docs) open tabs that cannot open at all, so
   *   binding them would promise a panel that does not exist.
   * - `⌘O` (Code) is already the Welcome screen's folder picker. The earlier
   *   binding keeps the chord; the Code tab carries none.
   *
   * Each stands down while a modal overlay owns focus, exactly as the review
   * and evidence chords do.
   */
  const handleDockTabShortcut = (event: KeyboardEvent): void => {
    if (event.repeat || composing) return;
    if (!(event.metaKey || event.ctrlKey) || event.shiftKey) return;
    const key = event.key.toLowerCase();
    let tab: DockTab | null = null;
    if (event.altKey && key === "e") tab = "environment";
    else if (!event.altKey && event.metaKey && !event.ctrlKey && key === "p") tab = "files";
    else if (!event.altKey && key === "d") tab = "diff";
    if (tab === null) return;
    const active = document.activeElement;
    if (active instanceof HTMLElement && active.closest(ESCAPE_OWNERS)) return;
    if (paletteOpen || settingsOpen || pickerOpen || agentMenuOpen || openControl !== null) return;
    event.preventDefault();
    setDockTab(tab, true);
  };
  window.addEventListener("keydown", handleEscape);
  window.addEventListener("keydown", handleDockTabShortcut);
  window.addEventListener("keydown", handleLaneCycleShortcut);
  window.addEventListener("keydown", handleNewLaneShortcut);
  window.addEventListener("keydown", handleFocusShortcut);
  window.addEventListener("keydown", handleDecisionsShortcut);
  window.addEventListener("keydown", handleWindowKeydown);
  window.addEventListener("keydown", handlePaletteShortcut);
  window.addEventListener("keydown", handleReviewShortcut);
  window.addEventListener("keydown", handleEvidenceShortcut);
  window.addEventListener("keydown", handleEvidenceSearchShortcut);
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
    openCenterView: (view, arg) => {
      if (disposed) return;
      openCenterView(view, arg);
    },
    selectLane: (laneId) => {
      if (disposed) return;
      selectLane(laneId);
      navigate("conversation");
    },
    closeCenterView: () => {
      if (disposed) return;
      closeCenterView();
    },
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
        // its primary actions mouse-operable by dismissing the sidebar that
        // launched the New Lane overlay — in whichever mode it is showing.
        laneRailOpen = false;
        laneRailPeek = false;
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
      window.removeEventListener("keydown", handleEscape);
      window.removeEventListener("keydown", handleDockTabShortcut);
      window.removeEventListener("keydown", handleLaneCycleShortcut);
      window.removeEventListener("keydown", handleNewLaneShortcut);
      window.removeEventListener("keydown", handleFocusShortcut);
      window.removeEventListener("keydown", handleDecisionsShortcut);
      window.removeEventListener("keydown", handlePaletteShortcut);
      window.removeEventListener("keydown", handleReviewShortcut);
      window.removeEventListener("keydown", handleEvidenceShortcut);
      window.removeEventListener("keydown", handleEvidenceSearchShortcut);
      if (reviewStaleTimer !== null) window.clearTimeout(reviewStaleTimer);
      if (lanePeekTimer !== null) window.clearTimeout(lanePeekTimer);
      if (dockPeekTimer !== null) window.clearTimeout(dockPeekTimer);
      reviewStaleTimer = null;
      // The content cache is view-scoped on purpose: a body that outlived the
      // view could be rendered against an archive that has since moved.
      evidenceContent = new Map();
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
        queueMicrotask(noteEvidenceStaleness);
        queueMicrotask(noteOperatorGitPending);
        // The layout record rides the same wake: a mode or a hidden segment
        // changed anywhere else is Core's fact, and re-reading is what keeps
        // this webview from holding a second copy of it.
        queueMicrotask(() => void refreshLayoutRecord());
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
        // The dock's Environment and Diff panels are the only surfaces that
        // read without an explicit open, so a capability that resolves after
        // the first frame still has to fill them.
        ensureDockDiff();
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
    reviewReadTarget = laneId;
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
        // Redrawn whether or not the review owns the centre pane: since `G5`
        // the context dock's Changes section and Diff tab render the same page,
        // so an answer that only updated the dock still has to reach it.
        if (!disposed) render(false);
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

  /* ---- context dock panels (G5) ---- */

  /**
   * Fills the dock's diff-backed panels from the same page DiffReview uses.
   *
   * One `QueryWorkspaceDiff` serves the Changes section, the Diff tab and the
   * review view: the host allows one diff read in flight, and three surfaces
   * asking the same question of the same target would be three answers that
   * can disagree. The read is skipped entirely until the no-traffic capability
   * probe says Core publishes structured diff at all, and re-run when the
   * target Lane changes, because a page read for one worktree describes a
   * different tree than the one now selected.
   *
   * Core decides this read non-interactively — a deny or an unresolved ask
   * comes back as a rejection rather than parking the client behind an
   * approval prompt — so the dock may issue it without an operator gesture.
   */
  const ensureDockDiff = (): void => {
    if (!options.workspaceDiff) return;
    if (reviewCapability === null) {
      ensureReviewCapability();
      return;
    }
    if (reviewCapability !== true || reviewReadInFlight) return;
    if (reviewReadTarget === selectedLaneId) return;
    readReview();
  };

  /**
   * Reads one directory of the workspace inventory.
   *
   * `prefix` is `""` for the root and `"crates/"` for an opened directory, the
   * same shape Core's `WorkspaceFilesQuery.prefix` takes. A prefix already in
   * flight is not re-sent, so a double click cannot stack reads.
   */
  const readDockFiles = (prefix: string): void => {
    if (!options.loadWorkspaceFiles || dockFileReads.has(prefix)) return;
    dockFileReads.add(prefix);
    void options
      .loadWorkspaceFiles(prefix === "" ? null : prefix)
      .then((answer) => {
        if (disposed) return;
        dockFilePages.set(prefix, answer);
      })
      .catch((error: unknown) => {
        if (disposed) return;
        // A transport failure is this client's own error, never dressed up as
        // Core's refusal and never as an empty tree.
        dockFilePages.set(prefix, {
          outcome: {
            state: "rejected",
            reason: error instanceof Error ? error.message : String(error),
          },
          entries: [],
          complete: false,
          loaded: false,
          pendingCommandId: null,
          capabilityAvailable: true,
        });
      })
      .finally(() => {
        dockFileReads.delete(prefix);
        if (!disposed) render(false);
      });
  };

  /**
   * Switches the dock panel and starts whatever read it needs.
   *
   * `reveal` is set by the chords: a keystroke has to *show* the dock it just
   * switched, which under focus mode means peeking the right-edge panel and on
   * a narrow window means opening the drawer. A click needs neither, because
   * the dock the operator clicked is already on screen.
   */
  const setDockTab = (tab: DockTab, reveal = false): void => {
    // The disabled tabs are refused here too, so a stray caller cannot open a
    // panel that can never fill. Code is openable exactly where Core
    // publishes `runtime.workspace_file_reads` (C9), which is why this asks
    // the same question the strip does rather than reading `live` alone.
    if (
      !isDockTabLive(tab, {
        fileReads: !!options.workspaceFile && workspaceFileAnswer.capabilityAvailable,
      })
    ) {
      return;
    }
    dockTab = tab;
    if (reveal) {
      contextDrawerOpen = true;
      if (focusMode) showDockPeek();
    }
    render(false);
    if (tab === "files") readDockFiles("");
    else ensureDockDiff();
  };

  /** Opens or closes one directory, reading its page the first time. */
  const toggleDockDirectory = (path: string): void => {
    if (dockFilesExpanded.has(path)) dockFilesExpanded.delete(path);
    else {
      dockFilesExpanded.add(path);
      if (!dockFilePages.has(`${path}/`)) readDockFiles(`${path}/`);
    }
    render(false);
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
        queueMicrotask(noteEvidenceStaleness);
        queueMicrotask(noteOperatorGitPending);
        // The layout record rides the same wake: a mode or a hidden segment
        // changed anywhere else is Core's fact, and re-reading is what keeps
        // this webview from holding a second copy of it.
        queueMicrotask(() => void refreshLayoutRecord());
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

  /* ---- evidence archive reads (GUI-CORE-025) ---- */

  /// Learns whether Core publishes the evidence archive at all, without
  /// sending a command. Entry points read the answer.
  const ensureEvidenceCapability = (): void => {
    if (evidenceCapability !== null || !options.evidence) return;
    void options.evidence
      .read()
      .then((projection) => {
        if (disposed) return;
        const next = projection.capabilityAvailable;
        if (next === evidenceCapability) return;
        evidenceCapability = next;
        render(false);
      })
      .catch(() => {
        // A host that cannot answer is not a Core that lacks the capability.
        // Leaving it unresolved keeps the entry disabled without claiming why.
      });
  };

  /// Sends one `QueryEvidence` for the first page and redraws.
  ///
  /// The scope follows the cockpit's Lane selection: a selected Lane reads
  /// that Lane's evidence, and no selection reads the whole archive. The
  /// selected row and the loaded content are dropped, because a re-scoped list
  /// is a different list.
  const readEvidence = (): void => {
    if (!options.evidence || evidenceReadInFlight) return;
    evidenceReadInFlight = true;
    const laneId = selectedLaneId;
    const kinds = [...evidenceKinds];
    void options.evidence
      .query(laneId, kinds)
      .then((projection) => {
        if (disposed) return;
        evidenceProjection = projection;
        evidenceCapability = projection.capabilityAvailable;
      })
      .catch((error: unknown) => {
        if (disposed) return;
        // A transport failure is not Core's refusal, so it is reported as this
        // client's own error rather than dressed up as a Core rejection.
        evidenceProjection = {
          ...PENDING_EVIDENCE_ARCHIVE,
          outcome: { state: "rejected", reason: String(error) },
          capabilityAvailable: evidenceCapability !== false,
          scopeLaneId: laneId,
          kinds,
        };
      })
      .finally(() => {
        evidenceReadInFlight = false;
        if (!disposed && centerView === "evidence") render(false);
      });
  };

  /// Sends one more `QueryEvidence` through Core's own opaque cursor.
  const loadOlderEvidence = (): void => {
    if (!options.evidence || evidenceReadInFlight) return;
    evidenceReadInFlight = true;
    void options.evidence
      .loadOlder()
      .then((projection) => {
        if (disposed) return;
        evidenceProjection = projection;
      })
      .catch(() => undefined)
      .finally(() => {
        evidenceReadInFlight = false;
        if (!disposed && centerView === "evidence") render(false);
      });
  };

  /// Selects one row and reads its canonical bytes once per view lifetime.
  const selectEvidence = (evidenceId: string): void => {
    evidenceSelectedId = evidenceId;
    const port = options.evidence;
    if (!port || evidenceContent.has(evidenceId) || evidenceContentInFlight) {
      render(false);
      return;
    }
    evidenceContentInFlight = true;
    render(false);
    void port
      .content(evidenceId)
      .then((content) => {
        if (disposed) return;
        // Keyed by the id *Core echoed*, never by the id this client asked
        // with, so an answer can never be filed under the wrong row.
        if (content.evidenceId) evidenceContent.set(content.evidenceId, content);
        else evidenceContent.set(evidenceId, content);
      })
      .catch((error: unknown) => {
        if (disposed) return;
        evidenceContent.set(evidenceId, {
          ...ABSENT_EVIDENCE_CONTENT,
          outcome: { state: "rejected", reason: String(error) },
          evidenceId,
        });
      })
      .finally(() => {
        evidenceContentInFlight = false;
        if (!disposed && centerView === "evidence") render(false);
      });
  };

  /// The re-query *signal*, evaluated on every ordered Core wake.
  ///
  /// Unlike DiffReview this never re-reads on its own. A diff pane holds one
  /// page of one tree; an evidence list holds several pages an operator paged
  /// through by hand, and reloading it underneath them would discard that and
  /// move the rows they were reading. The banner and Refresh are the whole
  /// mechanism, and the rule is documented in `apps/gui/README.md`.
  const noteEvidenceStaleness = (): void => {
    if (centerView !== "evidence" || !options.evidence) return;
    if (evidenceReadInFlight) return;
    void options.evidence
      .read()
      .then((projection) => {
        if (disposed || centerView !== "evidence" || !projection.stale) return;
        if (evidenceProjection?.stale) return;
        evidenceProjection = projection;
        render(false);
      })
      .catch(() => undefined);
  };

  /**
   * Redraws the list after a search keystroke without disturbing the caret.
   *
   * A full `render()` rebuilds the whole frame including the search input, so
   * the focus and selection are restored around the redraw. The box is a local
   * filter over loaded rows; it reaches no Core command.
   */
  const renderEvidenceListOnly = (): void => {
    if (disposed || centerView !== "evidence") return;
    const before = root.querySelector<HTMLInputElement>("[data-evidence-search]");
    const caret = before?.selectionStart ?? null;
    render(false);
    const after = root.querySelector<HTMLInputElement>("[data-evidence-search]");
    if (!after) return;
    after.focus();
    if (caret !== null) after.setSelectionRange(caret, caret);
  };

  const evidenceAvailable = (): boolean =>
    !!options.evidence && evidenceCapability === true;

  const openEvidence = (): void => {
    if (!options.evidence) return;
    centerView = "evidence";
    render(false);
    readEvidence();
  };

  const closeEvidence = (): void => {
    centerView = "transcript";
    render(true);
  };

  const reviewAvailable = (): boolean =>
    !!options.workspaceDiff && reviewCapability === true;

  /**
   * Opens DiffReview, optionally on one exact file.
   *
   * `path` is the argument seam the dock's Changes rows and the Diff tab's
   * inspector use: DiffReview already takes a `selectedPath`, so opening it on
   * a file is a matter of seeding the cockpit's own selection before the view
   * mounts rather than a second entry point into the view. A path Core's next
   * page does not carry is dropped by DiffReview's own resolution, which is
   * why nothing here validates it against a page that may not have arrived.
   */
  const openReview = (path?: string): void => {
    if (!options.workspaceDiff) return;
    if (path !== undefined) reviewSelectedPath = path;
    centerView = "review";
    render(false);
    // The dock's Environment panel already reads this page, so a review opened
    // over a fresh page for the same target reuses it instead of shelling out
    // to git a second time. A page read for another Lane, a stale one, or no
    // page at all is re-read.
    if (
      reviewProjection?.loaded &&
      reviewReadTarget === selectedLaneId &&
      !reviewProjection.stale
    ) {
      return;
    }
    readReview();
  };

  /**
   * The dock's "Commit or push" route.
   *
   * It opens no new surface and sends no command: `D-RAILNAV ①` makes
   * DiffReview the registered host of the operator's git actions, so the row
   * opens that view and moves the keyboard to the control the operator came
   * for — the commit message field when the bar rendered one, and the
   * titlebar's sync chip otherwise, which is the same action pair's other half
   * (a push with nothing staged is what "or push" means).
   *
   * Focus is taken after a frame because the view mounts in its pending state
   * first; a `querySelector` in the same tick would find the bar that has not
   * been drawn yet.
   */
  const openCommitBar = (): void => {
    openReview();
    window.setTimeout(() => {
      if (disposed || centerView !== "review") return;
      const field = root.querySelector<HTMLElement>("[data-review-commit-message]");
      if (field) {
        field.focus();
        return;
      }
      root.querySelector<HTMLElement>("[data-topbar-sync]")?.focus();
    }, 0);
  };

  const closeReview = (): void => {
    centerView = "transcript";
    if (reviewStaleTimer !== null) {
      window.clearTimeout(reviewStaleTimer);
      reviewStaleTimer = null;
    }
    render(true);
  };

  const SECONDARY_ROUTES: readonly D1SecondaryRoute[] = ["d2", "d10", "d12", "d13", "d14"];

  const isSecondaryRoute = (view: D1CenterView): view is D1SecondaryRoute =>
    (SECONDARY_ROUTES as readonly string[]).includes(view);

  /**
   * Whether a rail destination has somewhere to go.
   *
   * The two registered secondary surfaces ride their own Core capability; the
   * five D-screens ride the shell's secondary host. A destination with neither
   * is disabled and labelled rather than enabled and inert.
   */
  const destinationAvailable = (destination: D1RailDestination): boolean => {
    if (destination === "review") return reviewAvailable();
    if (destination === "evidence") return evidenceAvailable();
    return !!options.secondaryViews || !!options.onNavigate;
  };

  /** The sentence a disabled or conditional destination carries in its name. */
  const destinationNote = (destination: D1RailDestination): string | undefined => {
    if (destination === "review" && !reviewAvailable()) {
      return translate(locale, "d1.activity.unavailable", {
        capability: "runtime.structured_diff",
      });
    }
    if (destination === "evidence" && !evidenceAvailable()) {
      return translate(locale, "d1.activity.unavailable", {
        capability: "runtime.evidence_reads",
      });
    }
    // D14 is never blocked by a capability: without `runtime.audit` it opens
    // in raw event replay, which is a different view of the same question and
    // must be said up front rather than discovered after the click.
    if (destination === "d14") return translate(locale, "d1.activity.auditFallback", {});
    // The queue badge is absent in two different situations and only one of
    // them is "nothing waiting". No published count says so in the name, so
    // the missing badge is never read as an empty queue.
    if (destination === "d2" && projection.statusbar.pendingDecisionCount === null) {
      return translate(locale, "d1.activity.queueUnknown", {});
    }
    return undefined;
  };

  const railDestinations = (): Partial<Record<D1RailDestination, D1RailDestinationState>> => {
    const states: Partial<Record<D1RailDestination, D1RailDestinationState>> = {};
    for (const destination of [
      "review",
      "evidence",
      ...SECONDARY_ROUTES,
    ] as readonly D1RailDestination[]) {
      states[destination] = {
        available: destinationAvailable(destination),
        current: centerView === destination,
        note: destinationNote(destination),
        // The only count Core already publishes for a rail destination. D2 is
        // the decision queue and `pendingDecisionCount` is its size; nothing else
        // gets a badge, because nothing else has a Core-published number.
        //
        // It is the same number the statusbar's `⏸` segment prints, on purpose:
        // two counts on one screen for one queue is how they start disagreeing.
        // Note that the D2 view's own `pendingTotal` is a *different* sum —
        // approvals plus pending reviews, where this is approvals plus open
        // non-dormant merge gates — so the badge and the view's header can
        // legitimately differ. Reconciling them is a projection question for
        // the owning batch, not something to hide by counting twice here.
        badge: destination === "d2" ? projection.statusbar.pendingDecisionCount : null,
      };
    }
    return states;
  };

  /**
   * Opens one secondary D-screen in the centre pane.
   *
   * The mount is memoised on `route + arg`: an ordered Core refresh re-renders
   * the chrome around a screen that keeps its own state, while a different
   * route or a different preselected id is a genuinely different view and is
   * mounted afresh.
   */
  const mountSecondary = (): void => {
    if (!isSecondaryRoute(centerView) || !options.secondaryViews || !secondaryBody) return;
    const key = `${centerView}\u0000${secondaryArg ?? ""}`;
    if (key === secondaryMountKey) return;
    secondaryMountKey = key;
    secondaryError = null;
    const route = centerView;
    const container = secondaryBody;
    container.replaceChildren();
    try {
      const mounted = options.secondaryViews.mount(route, container, secondaryArg);
      if (mounted instanceof Promise) {
        void mounted.catch((error: unknown) => {
          if (disposed || centerView !== route) return;
          secondaryError = error instanceof Error ? error.message : String(error);
          render(false);
        });
      }
    } catch (error: unknown) {
      secondaryError = error instanceof Error ? error.message : String(error);
    }
  };

  /**
   * The cockpit's router.
   *
   * Every entry point — the rail, the statusbar's gate segment, the palette,
   * the titlebar chips, a screen's own cross-links — comes through here, so
   * "which destinations are in-cockpit views" is one decision in one place.
   * A destination the cockpit cannot host falls through to the shell's own
   * window route, which is what `?screen=d4` and `?screen=d11` still are.
   */
  const navigate = (route: string, arg?: string): void => {
    // The rail's home slot is a route like any other, so a screen that wants
    // the operator back in the conversation names it rather than reaching for
    // the close control.
    if (route === "conversation" || route === "transcript") {
      closeCenterView();
      return;
    }
    if (route === "review" || route === "evidence") {
      openCenterView(route, arg);
      return;
    }
    if (isSecondaryRoute(route as D1CenterView) && options.secondaryViews) {
      openCenterView(route as D1SecondaryRoute, arg);
      return;
    }
    // The second argument stays absent when there is none: the shell's route
    // handlers are the same ones the palette and the D-screens already call.
    if (arg === undefined) options.onNavigate?.(route);
    else options.onNavigate?.(route, arg);
  };

  const openCenterView = (view: D1CenterView, arg?: string): void => {
    if (view === "transcript") {
      closeCenterView();
      return;
    }
    // Toggling the slot that is already showing returns to the transcript, the
    // way `⌘R` and `⌘E` already toggle their own views. An open review asked
    // for a *different* file is a new request, not a toggle: the operator
    // clicked another row, which must move the selection rather than close.
    if (view === "review" && centerView === "review" && arg !== undefined) {
      openReview(arg);
      return;
    }
    if (centerView === view && (arg ?? null) === secondaryArg) {
      closeCenterView();
      return;
    }
    if (view === "review") {
      // The argument is a workspace-relative path here, not a Core id: the
      // review's preselection is a file, which is what the dock's change rows
      // and its inspector hand over.
      openReview(arg);
      return;
    }
    if (view === "evidence") {
      openEvidence();
      return;
    }
    if (!options.secondaryViews) {
      if (arg === undefined) options.onNavigate?.(view);
      else options.onNavigate?.(view, arg);
      return;
    }
    centerView = view;
    secondaryArg = arg ?? null;
    render(false);
    mountSecondary();
  };

  const closeCenterView = (): void => {
    if (centerView === "review") {
      closeReview();
      return;
    }
    if (centerView === "evidence") {
      closeEvidence();
      return;
    }
    if (centerView === "transcript") return;
    centerView = "transcript";
    secondaryArg = null;
    secondaryMountKey = null;
    secondaryError = null;
    // The host node goes with the view: a screen kept alive behind the
    // transcript would keep reading Core for a pane nobody is looking at.
    secondaryShell = null;
    secondaryBody = null;
    render(true);
  };

  /**
   * The sidebar mode this frame actually draws. Focus forces `floating`
   * without touching the operator's own choice.
   */
  const effectiveLaneSidebarMode = (): LaneSidebarMode =>
    focusMode ? "floating" : laneSidebarMode;

  /** Turns focus mode on or off and redraws the shell around it. */
  const setFocusMode = (next: boolean): void => {
    if (focusMode === next) return;
    focusMode = next;
    // A peek that was open belongs to the layout that is going away.
    hideLanePeek(false);
    render(false);
  };

  /** Hides the floating sidebar peek and cancels any pending delay. */
  const hideLanePeek = (rerender = true): void => {
    if (lanePeekTimer !== null) {
      window.clearTimeout(lanePeekTimer);
      lanePeekTimer = null;
    }
    if (!laneRailPeek) return;
    laneRailPeek = false;
    if (rerender) render(false);
  };

  /** The dock's mirror of the sidebar peek, with the same ~700 ms delay. */
  const hideDockPeek = (rerender = true): void => {
    if (dockPeekTimer !== null) {
      window.clearTimeout(dockPeekTimer);
      dockPeekTimer = null;
    }
    if (!dockPeek) return;
    dockPeek = false;
    if (rerender) render(false);
  };

  const showDockPeek = (): void => {
    if (dockPeekTimer !== null) {
      window.clearTimeout(dockPeekTimer);
      dockPeekTimer = null;
    }
    if (dockPeek) return;
    dockPeek = true;
    render(false);
  };

  const scheduleDockPeekClose = (): void => {
    if (dockPeekTimer !== null) window.clearTimeout(dockPeekTimer);
    dockPeekTimer = window.setTimeout(() => {
      dockPeekTimer = null;
      hideDockPeek();
    }, LANE_PEEK_CLOSE_MS);
  };

  const showLanePeek = (): void => {
    if (lanePeekTimer !== null) {
      window.clearTimeout(lanePeekTimer);
      lanePeekTimer = null;
    }
    if (laneRailPeek) return;
    laneRailPeek = true;
    render(false);
  };

  /// The design's ~700 ms close delay, so crossing the hot zone on the way
  /// somewhere else does not flash the sidebar open and shut.
  const scheduleLanePeekClose = (): void => {
    if (lanePeekTimer !== null) window.clearTimeout(lanePeekTimer);
    lanePeekTimer = window.setTimeout(() => {
      lanePeekTimer = null;
      if (disposed) return;
      laneRailPeek = false;
      render(false);
    }, LANE_PEEK_CLOSE_MS);
  };

  const applyLaneSidebarMode = (next: LaneSidebarMode): void => {
    if (laneSidebarMode === next) return;
    laneSidebarMode = next;
    // The two modes are two hosts for one component, so the flag that means
    // "visible" is reset rather than carried across as a stale value: pinning
    // shows the column (that is what the operator asked for) and unpinning
    // returns the width to the transcript until the hot zone is used.
    laneRailPeek = false;
    laneRailOpen = next === "pinned";
    render(false);
  };

  /**
   * Reads the newest page of the selected scope's ordered transcript.
   *
   * One read in flight, and the scope is re-read whenever the selection moves:
   * a Lane's conversation belongs to that Lane, so rows read for the previous
   * selection are dropped by the host rather than left on screen under a new
   * Lane's name.
   */
  const readTranscriptRows = (): void => {
    if (!options.transcriptRows || transcriptRowsInFlight) return;
    if (transcriptRowsProjection.capabilityAvailable === false) return;
    const laneId = selectedLaneId;
    transcriptRowsInFlight = true;
    transcriptRowsScope = laneId;
    void options.transcriptRows
      .query(laneId)
      .then((projection) => {
        if (disposed) return;
        transcriptRowsProjection = projection;
      })
      .catch((error: unknown) => {
        if (disposed) return;
        // A transport failure is this client's own, so it is reported as such
        // rather than dressed up as a Core refusal.
        transcriptRowsProjection = {
          ...transcriptRowsProjection,
          outcome: { state: "rejected", reason: String(error) },
          loaded: false,
        };
      })
      .finally(() => {
        transcriptRowsInFlight = false;
        if (!disposed) render(false);
      });
  };

  /**
   * Pages backwards through Core's own cursor.
   *
   * Bound to the scroll-top of the transcript and to the explicit control, so
   * a keyboard-only operator reaches the same page. Core published no cursor
   * means the scope is exhausted and nothing is sent.
   */
  const loadOlderTranscriptRows = (): void => {
    if (!options.transcriptRows || transcriptRowsInFlight) return;
    if (!transcriptRowsProjection.older) return;
    const laneId = selectedLaneId;
    transcriptRowsInFlight = true;
    void options.transcriptRows
      .loadOlder(laneId)
      .then((projection) => {
        if (disposed) return;
        transcriptRowsProjection = projection;
      })
      .catch((error: unknown) => {
        if (disposed) return;
        transcriptRowsProjection = {
          ...transcriptRowsProjection,
          outcome: { state: "rejected", reason: String(error) },
        };
      })
      .finally(() => {
        transcriptRowsInFlight = false;
        if (!disposed) render(false);
      });
  };

  /**
   * Reads one file from Core into the dock inspector and the Code tab.
   *
   * `laneId` is the cockpit's own selection, so a Lane's worktree is read for
   * a Lane and the workspace root otherwise — Core resolves the worktree and
   * the client never passes a path. The answer replaces whatever was held
   * before it is sent, so a pending inspector never shows the previous file's
   * bytes under the new path's name.
   */
  const openWorkspaceFile = (path: string): void => {
    if (!options.workspaceFile || workspaceFileInFlight) return;
    const laneId = selectedLaneId;
    workspaceFileInFlight = true;
    workspaceFileAnswer = {
      ...IDLE_WORKSPACE_FILE,
      capabilityAvailable: workspaceFileAnswer.capabilityAvailable,
      outcome: { state: "pending", reason: null },
      requestedPath: path,
      targetLaneId: laneId,
    };
    render(false);
    void options.workspaceFile
      .open(laneId, path)
      .then((answer) => {
        if (disposed) return;
        workspaceFileAnswer = answer;
      })
      .catch((error: unknown) => {
        if (disposed) return;
        // A transport failure is this client's own, so it is reported as one
        // rather than dressed up as a Core refusal.
        workspaceFileAnswer = {
          ...workspaceFileAnswer,
          outcome: { state: "rejected", reason: String(error) },
          file: null,
        };
      })
      .finally(() => {
        workspaceFileInFlight = false;
        if (!disposed) render(false);
      });
  };

  /// One no-traffic read for the capability, before anything is asked for.
  const ensureWorkspaceFile = (): void => {
    if (!options.workspaceFile) return;
    void options.workspaceFile
      .read()
      .then((answer) => {
        if (disposed) return;
        workspaceFileAnswer = answer;
        render(false);
      })
      .catch(() => {
        /* the capability stays unknown; the inspector's Open stays disabled. */
      });
  };

  /// One no-traffic read for the capability, before anything is asked for.
  const ensureTranscriptRows = (): void => {
    if (!options.transcriptRows) return;
    void options.transcriptRows
      .read()
      .then((projection) => {
        if (disposed) return;
        transcriptRowsProjection = projection;
        // A Core that publishes the capability gets the first page asked for
        // immediately; one that does not is left alone, and the transcript
        // keeps its named unavailable rows.
        if (projection.capabilityAvailable) readTranscriptRows();
        else render(false);
      })
      .catch(() => {
        /* the capability stays unknown; the transcript says nothing new. */
      });
  };

  /**
   * The rail pin's action.
   *
   * With the capability the mode is Core's, so the patch goes out and the
   * layout moves when Core answers — the same rule the Settings panel follows
   * for the appearance record. Without it the toggle stays session-local,
   * which is the honest fallback: a client that persisted it itself would be
   * the second preference authority the contract forbids.
   */
  const setLaneSidebarMode = (next: LaneSidebarMode): void => {
    if (!options.layout || !layoutRecord.capabilityAvailable) {
      applyLaneSidebarMode(next);
      return;
    }
    void sendLayoutPatch({ laneSidebarMode: next });
  };

  /**
   * Sends one layout patch and adopts whatever Core published back.
   *
   * Only the axis the operator touched enters the patch, so an untouched one
   * keeps whatever Core resolves. A refusal leaves the rendered layout exactly
   * where it was and keeps Core's reason on the record, because a layout that
   * moved after a refusal would claim a write that never happened.
   */
  async function sendLayoutPatch(patch: LayoutPreferencePatch): Promise<void> {
    if (!options.layout || layoutCommandInFlight || disposed) return;
    layoutCommandInFlight = true;
    try {
      const answer = await options.layout.set(patch);
      if (disposed) return;
      adoptLayoutRecord(answer);
    } catch (error: unknown) {
      if (disposed) return;
      // A transport failure is the client's own, so it is reported as the
      // client's rather than dressed up as a Core refusal.
      layoutRecord = {
        ...layoutRecord,
        outcome: { state: "rejected", reason: String(error) },
      };
    } finally {
      layoutCommandInFlight = false;
      if (!disposed) render(false);
    }
  }

  /**
   * Adopts one layout answer as the layout this frame draws.
   *
   * `unknown` and `null` both fall back to the design's own default rather
   * than to the last value this client happened to render: `D-SIDEBAR` makes
   * `floating` what an operator who never opened Settings sees, and a mode
   * this build cannot draw is not a reason to invent a pinned column.
   */
  function adoptLayoutRecord(record: LayoutPreferencesProjection): void {
    layoutRecord = record;
    if (!record.capabilityAvailable) return;
    applyLaneSidebarMode(record.laneSidebarMode === "pinned" ? "pinned" : "floating");
    const hidden = new Set(record.hiddenStatusbarSegments);
    statusbarAmbient = Object.fromEntries(
      STATUSBAR_AMBIENT_SEGMENTS.map((segment) => [segment, !hidden.has(segment)]),
    ) as StatusbarAmbientVisibility;
  }

  /**
   * Re-reads the layout record with no Core traffic.
   *
   * Called at mount and on every ordered wake, which is the whole of "the
   * webview keeps no copy": a mode changed in another window, or a record
   * Core republished on a snapshot prefix, lands here instead of being
   * overwritten by what this client last drew.
   */
  async function refreshLayoutRecord(): Promise<void> {
    if (!options.layout || layoutCommandInFlight || disposed) return;
    try {
      const record = await options.layout.read();
      if (disposed) return;
      const before = laneSidebarMode;
      const beforeHidden = JSON.stringify(statusbarAmbient);
      adoptLayoutRecord(record);
      if (before === laneSidebarMode && beforeHidden === JSON.stringify(statusbarAmbient)) {
        return;
      }
      render(false);
    } catch {
      // The record is a read; a failed one leaves the layout as drawn.
    }
  }

  /**
   * Core's sentence for a layout record it applied but could not write, or
   * refused outright. `null` when there is nothing to say.
   */
  const layoutNote = (): string | null => {
    if (layoutRecord.outcome.state === "rejected") {
      return translate(locale, "d1.layout.rejected", {
        reason: layoutRecord.outcome.reason ?? "",
      });
    }
    if (layoutRecord.persisted === false) {
      return translate(locale, "d1.layout.unpersisted", {});
    }
    if (options.layout && layoutRecord.capabilityAvailable === false) {
      return translate(locale, "d1.layout.unavailable", {
        capability: LAYOUT_PREFERENCES_CAPABILITY,
      });
    }
    return null;
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
          queueMicrotask(noteEvidenceStaleness);
          queueMicrotask(noteOperatorGitPending);
          queueMicrotask(() => void refreshLayoutRecord());
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
        // D11 is project intake and this is the project surface. It replaces
        // the window, so it goes through the shell's own route.
        onConfigureProject: options.onNavigate ? () => options.onNavigate?.("d11") : undefined,
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
    // The ordered transcript is owner-scoped, so a selection change is a
    // different conversation and is re-read rather than filtered locally.
    readTranscriptRows();
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
        laneCreation: laneCreationState(),
        canFocusComposer: Boolean(root.querySelector("[data-composer]")),
        canCancelTurn: Boolean(root.querySelector("[data-work-cancel]")),
        // A bound host always gets the row; an absent capability renders it
        // disabled naming the capability, the same honesty the `~` file scope
        // ships, rather than a row that quietly disappears.
        reviewBound: Boolean(options.workspaceDiff),
        reviewAvailable: reviewAvailable(),
        evidenceBound: Boolean(options.evidence),
        evidenceAvailable: evidenceAvailable(),
        returnFocus: root.querySelector<HTMLElement>("[data-command-palette-toggle]"),
      },
      {
        // The palette's `#` rows land on the same in-cockpit views the rail
      // opens, carrying the exact Core id they named.
      onNavigate: (route, arg) => navigate(route, arg),
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
        // One creation surface. The palette closes and the popover opens
        // against the tab strip's `＋`, which is on screen in both sidebar
        // modes — the rail's is not, in the default floating one.
        onCreateLane: () => {
          closePalette();
          openAgentMenu("tabs");
        },
        onOpenReview: () => openReview(),
        onOpenEvidence: () => openEvidence(),
        // A `~` row reads the file into the dock inspector, which is the one
        // surface this client has for a file's content: the contract publishes
        // no editor command, and the dock is where the read already lives.
        onOpenFile:
          options.workspaceFile && workspaceFileAnswer.capabilityAvailable
            ? (path) => {
                closePalette();
                dockTab = "files";
                dockFileSelected = path;
                contextDrawerOpen = true;
                openWorkspaceFile(path);
              }
            : undefined,
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
    // The two reads are independent: a host that bound one and not the other
    // gets that one. (Before G7 an absent cross-Lane read returned here and
    // took the file inventory with it, which left the `~` scope permanently
    // unavailable on such a host.)
    if (options.loadPaletteCrossLane) {
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
    }
    if (!options.loadWorkspaceFiles) return;
    void options
      .loadWorkspaceFiles(null)
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
    const anchor =
      (newLaneAnchor === "tabs"
        ? root.querySelector<HTMLButtonElement>("[data-lane-tab-add]")
        : null) ?? root.querySelector<HTMLButtonElement>("[data-create-lane]");
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
        options.onFullSetup?.({
          agentId: newLaneSelection?.kind === "acp" ? newLaneSelection.agentId : null,
          task: pendingAgentTaskDraft,
        });
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

  function openAgentMenu(anchor: "rail" | "tabs" = "rail"): void {
    newLaneAnchor = anchor;
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
    // The centre view is a fact about the cockpit, not a route: `data-route`
    // stays `d1` on the shell root, and this names which view owns the pane.
    frame.dataset.centerView = centerView;
    // The picker replaces the open workspace, so it exists only where there is
    // one to replace: never on the no-project Welcome, and never without the
    // host callbacks that actually reach Core.
    const projectPickerAvailable =
      !showWelcome && !!options.onPickProjectFolder && !!options.onOpenWorkspace;
    const topbar = renderCockpitTopbar(projection, locale, showWelcome, (route, arg) =>
      navigate(route, arg), {
      onToggleCommandPalette: () => togglePalette(""),
      commandPaletteOpen: paletteOpen,
      onToggleFocus: () => setFocusMode(!focusMode),
      focusMode,
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
    const sidebarMode = effectiveLaneSidebarMode();
    body.dataset.laneSidebarMode = sidebarMode;
    // Whether the pinned column is currently taking a grid track. Floating
    // never does: its host is an overlay, so the transcript keeps the width.
    body.dataset.laneColumn = String(sidebarMode === "pinned" && laneRailOpen);
    // `D-SIDEBAR`'s focus override, read by the grid: two tracks, both side
    // panels behind their own hot zones.
    body.dataset.focusMode = String(focusMode);
    if (showWelcome) body.classList.add("d1-body-welcome");

    const activity = renderActivityRail(locale, {
      lanesAvailable: !showWelcome,
      lanesOpen: sidebarMode === "pinned" ? laneRailOpen : laneRailPeek,
      conversationCurrent: centerView === "transcript",
      destinations: showWelcome ? {} : railDestinations(),
      onOpenDestination: (destination) => openCenterView(destination),
      onFocusWork: !composerFocusable
        ? undefined
        : () => {
            // Returning to the conversation is what this slot means, so a view
            // over the transcript is closed before the composer is focused.
            closeCenterView();
            // Resolved at activation, not at render: the rail is built before
            // the composer node exists in this frame.
            root.querySelector<HTMLTextAreaElement>("[data-composer]")?.focus();
          },
      onToggleLanes: () => {
        // One slot, two hosts. Floating toggles the overlay peek — the
        // keyboard path `D-SIDEBAR` owes an operator who cannot land a pointer
        // on a 12px strip; pinned toggles the column itself, which is the only
        // way to give that width back without leaving the mode.
        if (sidebarMode === "floating") {
          laneRailFocusTarget = laneRailPeek ? "toggle" : "rail";
          if (laneRailPeek) hideLanePeek();
          else showLanePeek();
          return;
        }
        laneRailOpen = !laneRailOpen;
        laneRailFocusTarget = laneRailOpen ? "rail" : "toggle";
        render(false);
      },
      // `D-SIDEBAR`'s single toggle entry, above the gear. It reports the
      // operator's own mode even under focus, because that is the preference
      // focus is temporarily overriding — not replacing.
      laneSidebarMode,
      onToggleLaneSidebarMode: () =>
        setLaneSidebarMode(laneSidebarMode === "pinned" ? "floating" : "pinned"),
      laneSidebarNote: layoutNote(),
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
    const laneRailVisible = sidebarMode === "pinned" ? laneRailOpen : laneRailPeek;
    const lanes = renderLaneRail({
      projection,
      locale,
      open: laneRailVisible,
      selectedLaneId,
      mode: sidebarMode,
      onCreateLane: () => {
        if (options.onCreateLane) options.onCreateLane();
        else openAgentMenu("rail");
      },
      onDismiss: () => {
        // The rail's own `Escape`. Focus goes back to the control that opened
        // it in both modes; only what is being closed differs.
        laneRailFocusTarget = "toggle";
        if (sidebarMode === "floating") {
          hideLanePeek();
          return;
        }
        laneRailOpen = false;
        render(false);
      },
      onSelectLane: (laneId) => {
        // A floating sidebar has done its job once a Lane is chosen; it gives
        // the horizontal space straight back to the transcript.
        if (sidebarMode === "floating") hideLanePeek(false);
        selectLane(laneId);
      },
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
    /**
     * The sidebar's host.
     *
     * `pinned` keeps the rail directly in the body, where it already lived.
     * `floating` wraps the *same* node in the design's `.edgewrap.l` hot zone
     * — a 12 px strip against the activity rail with the `.edgehint` cue —
     * so the component is identical in both modes and only its host changes.
     */
    const laneHost = ((): HTMLElement => {
      if (sidebarMode === "pinned") return lanes;
      const edge = document.createElement("div");
      edge.className = "edgewrap l d1-lane-edge";
      edge.dataset.laneEdge = "true";
      // The host is what occupies the sidebar's place among the body's grid
      // children, so it carries the role in floating mode; the rail inside it
      // keeps its own `lane-rail` landmark, which is what everything else
      // queries.
      edge.dataset.cockpitRole = "lanes";
      edge.dataset.peek = String(laneRailPeek);
      const hint = document.createElement("span");
      hint.className = "edgehint";
      hint.setAttribute("aria-hidden", "true");
      edge.append(hint);
      const panel = document.createElement("div");
      panel.className = "floatpanel d1-lane-float";
      panel.append(lanes);
      edge.append(panel);
      edge.addEventListener("pointerenter", () => showLanePeek());
      edge.addEventListener("pointerleave", () => scheduleLanePeekClose());
      return edge;
    })();

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
    } else if (centerView === "evidence") {
      // EvidenceView owns the whole centre pane, like DiffReview. The composer
      // stays mounted below it: reading what a Lane produced and telling it
      // what to do next are the same sitting.
      renderEvidenceView(
        workSurface,
        evidenceProjection ?? {
          // Before the first answer the view renders the pending state rather
          // than an empty list, which would read as "no evidence".
          ...PENDING_EVIDENCE_ARCHIVE,
          capabilityAvailable: evidenceCapability !== false,
          scopeLaneId: selectedLaneId,
          kinds: [...evidenceKinds],
        },
        locale,
        {
          onRefresh: options.evidence ? () => readEvidence() : undefined,
          onLoadOlder: options.evidence ? () => loadOlderEvidence() : undefined,
          onClose: () => closeEvidence(),
          onSelect: (evidenceId) => selectEvidence(evidenceId),
          selectedId: evidenceSelectedId,
          kinds: evidenceKinds,
          onKindsChange: !options.evidence
            ? undefined
            : (next) => {
                // A filter change is a new Core query, so the selection and
                // the cursor go with it: the previous list's row may not be on
                // the filtered archive at all.
                evidenceKinds = next;
                evidenceSelectedId = null;
                readEvidence();
                render(false);
              },
          search: evidenceSearch,
          onSearchChange: (next) => {
            // No re-render on the keystroke itself: the input already holds
            // the text, and rebuilding the DOM under a caret would move it.
            evidenceSearch = next;
            renderEvidenceListOnly();
          },
          content:
            evidenceSelectedId === null
              ? ABSENT_EVIDENCE_CONTENT
              : (evidenceContent.get(evidenceSelectedId) ?? {
                  ...ABSENT_EVIDENCE_CONTENT,
                  evidenceId: evidenceSelectedId,
                  outcome: evidenceContentInFlight
                    ? { state: "pending", reason: null }
                    : { state: "idle", reason: null },
                }),
          onOpenReview: options.workspaceDiff ? () => openReview() : undefined,
          reviewAvailable: reviewAvailable(),
          onOpenAuditTrail: options.onOpenAuditTrail,
        },
      );
    } else if (isSecondaryRoute(centerView)) {
      // The registered D-screen owns the centre pane and nothing else: the
      // titlebar, both rails, the context dock, the composer and the statusbar
      // are built by this same pass and stay exactly where they were, so the
      // operator can keep telling the selected Lane what to do while reading
      // the queue, the monitor, the gate, the fleet, or the audit trail.
      //
      // The shell node is kept across renders on purpose. The screens hold
      // their own selection, filters, and mode, and each mount is a Core read;
      // rebuilding them on every ordered wake would reset the operator's place
      // and cost a read per event.
      const route = centerView;
      if (!secondaryShell || secondaryShell.dataset.secondaryView !== route) {
        const shell = document.createElement("section");
        shell.className = "d1-secondary";
        shell.dataset.secondaryView = route;
        const head = document.createElement("div");
        head.className = "d1-secondary-head";
        const title = document.createElement("h2");
        title.className = "d1-secondary-title";
        title.dataset.secondaryTitle = route;
        head.append(title);
        // The Close control sits where DiffReview's does — the trailing end of
        // the view's own head — so one habit closes every centre view.
        const close = document.createElement("button");
        close.type = "button";
        close.className = "review-icon d1-secondary-close";
        close.dataset.secondaryClose = "true";
        close.textContent = "✕";
        close.addEventListener("click", () => closeCenterView());
        head.append(close);
        const body = document.createElement("div");
        body.className = "d1-secondary-body";
        body.dataset.secondaryBody = route;
        shell.append(head, body);
        secondaryShell = shell;
        secondaryBody = body;
        // A new host means a new mount; the memo key belongs to the old node.
        secondaryMountKey = null;
      }
      const title = secondaryShell.querySelector<HTMLElement>("[data-secondary-title]")!;
      title.textContent = translate(locale, SECONDARY_VIEW_LABELS[route], {});
      const close = secondaryShell.querySelector<HTMLButtonElement>("[data-secondary-close]")!;
      const closeLabel = translate(locale, "d1.center.close", {});
      close.title = closeLabel;
      close.setAttribute("aria-label", closeLabel);
      secondaryShell.querySelector("[data-secondary-error]")?.remove();
      if (secondaryError !== null) {
        // Core's own refusal, verbatim, in place of the screen. The pane is
        // never left blank: a blank pane reads as "nothing here", which is a
        // different fact from "Core would not answer".
        const alert = document.createElement("p");
        alert.className = "d1-secondary-error";
        alert.dataset.secondaryError = "true";
        alert.setAttribute("role", "alert");
        alert.textContent = translate(locale, "d1.center.failed", { reason: secondaryError });
        secondaryShell.append(alert);
      }
      workSurface.append(secondaryShell);
    } else {
      // The design's `.tabstrip.lanebar` sits at the top of `ChatView`, inside
      // the centre pane and above the scrolling transcript — not above the
      // whole pane. A centre *view* replaces `ChatView` outright in the
      // flagship's own switch, which is why DiffReview, EvidenceView and the
      // five D-screens draw no strip: the Lane they are scoped to is the
      // selected one, and each of them states its own scope in its head.
      workSurface.dataset.hasLaneTabs = "true";
      workSurface.append(
        renderLaneTabs({
          projection,
          locale,
          selectedLaneId,
          onSelectLane: (laneId) => selectLane(laneId),
          onCreateLane: () => {
            if (options.onCreateLane) options.onCreateLane();
            else openAgentMenu("tabs");
          },
        }),
      );
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
      // C8: Core's own ordered rows for this scope. They are drawn first,
      // because they are the durable conversation and everything after them —
      // the live ACP pair, the live stream — is what has happened *since* the
      // page was read. Rows read for another scope are not drawn at all.
      const orderedRows =
        transcriptRowsProjection.loaded &&
        (transcriptRowsProjection.scopeLaneId ?? null) === selectedLaneId
          ? transcriptRowsProjection.rows
          : [];
      if (orderedRows.length > 0) {
        // The read answered with rows, so the two unavailable placeholders
        // that stood in for them retire: `transcript_user` and
        // `transcript_assistant` were claims about Core, and this is Core
        // answering (GUI-CORE-009).
        acpKinds.add("user");
        acpKinds.add("assistant");
        if (transcriptRowsProjection.older) {
          // Paging backwards is offered as a control as well as on scroll-top,
          // so a keyboard-only operator reaches the same page.
          const older = button(translate(locale, "d1.transcript.loadOlder", {}), "loadOlder");
          older.dataset.transcriptLoadOlder = "true";
          older.disabled = transcriptRowsInFlight;
          older.addEventListener("click", () => loadOlderTranscriptRows());
          transcriptRegion.append(older);
        } else if (transcriptRowsProjection.complete) {
          // Core's own statement, rather than the absence of a button.
          const complete = document.createElement("p");
          complete.className = "d1-empty";
          complete.dataset.transcriptComplete = "true";
          complete.textContent = translate(locale, "d1.transcript.rowsComplete", {});
          transcriptRegion.append(complete);
        }
        appendOrderedTranscriptRows(transcriptRegion, orderedRows, {
          locale,
          agent: focusedAcp ? transcriptAgent(projection, focusedAcp) : undefined,
          onOpenEvidence: options.onNavigate ? () => navigate("evidence") : undefined,
          // `D-AUDIT`'s one-way link: an audit row names a permission object,
          // so the trail is opened by that object and never by an audit id the
          // client would have to construct.
          onOpenAudit: options.onNavigate
            ? (requestId) => navigate("d14", `permission:${requestId}`)
            : undefined,
        });
      } else if (
        transcriptRowsProjection.capabilityAvailable &&
        (transcriptRowsProjection.scopeLaneId ?? null) === selectedLaneId
      ) {
        // The capability exists and this scope produced no rows. Four states,
        // four sentences: Core refused, the read is out, Core answered with
        // nothing, or nothing has been asked for yet.
        const note = document.createElement("p");
        note.className = "d1-empty";
        if (transcriptRowsProjection.outcome.state === "rejected") {
          note.dataset.transcriptRowsState = "rejected";
          note.setAttribute("role", "alert");
          note.textContent = translate(locale, "d1.transcript.rowsRejected", {
            reason: transcriptRowsProjection.outcome.reason ?? "",
          });
        } else if (transcriptRowsProjection.loaded) {
          note.dataset.transcriptRowsState = "empty";
          note.textContent = translate(locale, "d1.transcript.rowsEmpty", {});
          acpKinds.add("user");
          acpKinds.add("assistant");
        } else {
          note.dataset.transcriptRowsState = "pending";
          note.textContent = translate(locale, "d1.transcript.rowsPending", {});
        }
        transcriptRegion.append(note);
      }
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
        appendTypedWorkCards(
          transcriptRegion,
          projection.contextDock.checklist,
          locale,
          projection.permissionDock,
        );
      }
      if (!projectionMatchesSelectedLane) {
        const switching = document.createElement("p");
        switching.dataset.typedEmpty = "transcript-switching";
        switching.textContent = translate(locale, "d1.transcript.switching", {});
        transcriptRegion.append(switching);
      }
      const liveWork = projectionMatchesSelectedLane ? renderLiveWorkBar(projection, locale) : null;
      if (liveWork) transcriptRegion.append(liveWork);
      // C6: a turn Core ended without completing it is a transcript row, in
      // the conversation it belongs to. Scoped to this composer's target, so
      // another Lane's failure is never rendered as this one's; a completed
      // turn clears the fact upstream, so the row never outlives the work.
      const turnFailure = projection.turnFailure ?? null;
      if (
        projectionMatchesSelectedLane &&
        turnFailure &&
        (turnFailure.laneId ?? null) === selectedLaneId
      ) {
        const row = document.createElement("p");
        row.className = "d1-turn-failed";
        row.dataset.turnFailed = turnFailure.outcome;
        // A failure is an alert; a cancellation is the operator's own decision
        // and is reported as status rather than accusing Core of an error.
        row.setAttribute("role", turnFailure.outcome === "failed" ? "alert" : "status");
        row.textContent =
          turnFailure.outcome === "failed" && turnFailure.reason
            ? translate(locale, "d1.transcript.turnFailed", { reason: turnFailure.reason })
            : translate(
                locale,
                turnFailure.outcome === "cancelled"
                  ? "d1.transcript.turnCancelled"
                  : "d1.transcript.turnEndedUnknown",
                {},
              );
        transcriptRegion.append(row);
      }
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
        // Reaching the top is the design's own "read further back" gesture.
        // Guarded on Core having published a cursor, so a scope Core says is
        // complete sends nothing.
        if (transcriptRegion.scrollTop <= 0 && transcriptRowsProjection.older) {
          loadOlderTranscriptRows();
        }
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
        // C7 made the decision a durable audit row, so the id the dock prints
        // now resolves. The route is the cockpit's own, which lands D14 in the
        // centre pane rather than replacing the window.
        options.onNavigate ? (scope) => navigate("d14", `${scope.kind}:${scope.id}`) : undefined,
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
    const right = renderContextDock({
      projection: contextDockProjection,
      locale,
      matchesSelectedLane: projectionMatchesSelectedLane,
      tab: dockTab,
      onSelectTab: (tab) => setDockTab(tab),
      // `C5`'s per-Lane worktree source when Core published one for the
      // selected Lane. The workspace sample is a different tree and never
      // stands in for it, so `null` here means the Local section says it is
      // showing the workspace root.
      laneSource: contextDockProjection.contextDock.laneSource ?? null,
      diff: {
        bound: !!options.workspaceDiff,
        capabilityAvailable: reviewCapability,
        // The same page the review view renders, read once for both.
        projection: reviewProjection,
        selectedPath: dockDiffSelected,
        expanded: dockDiffExpanded,
        onSelect: (path) => {
          dockDiffSelected = path;
          render(false);
        },
        onToggle: (path) => {
          if (dockDiffExpanded.has(path)) dockDiffExpanded.delete(path);
          else dockDiffExpanded.add(path);
          render(false);
        },
        onOpenInReview: (path) => navigate("review", path),
      },
      files: {
        bound: !!options.loadWorkspaceFiles,
        capabilityAvailable: dockFilePages.get("")?.capabilityAvailable ?? null,
        pages: dockFilePages,
        expanded: dockFilesExpanded,
        selectedPath: dockFileSelected,
        onToggleDirectory: (path) => toggleDockDirectory(path),
        onSelect: (path) => {
          dockFileSelected = path;
          render(false);
        },
        // `runtime.workspace_file_reads` (C9). Both halves have to hold: a
        // host bound to send the read, and a Core that publishes the
        // capability. Either missing leaves Open disabled and named, which is
        // what G5 shipped for every Core.
        fileReadsAvailable:
          !!options.workspaceFile && workspaceFileAnswer.capabilityAvailable,
        onOpenFile: (path) => openWorkspaceFile(path),
        content: workspaceFileAnswer,
      },
      commit: {
        // The row routes rather than acts, so it is enabled exactly when the
        // commit bar it routes to can act: Core publishes the capability, Core
        // bound an owner this client may act as, and there is a source to
        // commit against.
        available:
          !!options.operatorGit &&
          operatorGitState.capabilityAvailable &&
          operatorGitState.ownerAvailable &&
          !!contextDockProjection.contextDock.source,
        reason: !options.operatorGit
          ? translate(locale, "d1.dock.unbound", {})
          : !operatorGitState.capabilityAvailable
            ? "runtime.operator_git"
            : !operatorGitState.ownerAvailable
              ? (operatorGitState.ownerUnavailableReason ??
                translate(locale, "d1.topbar.sync.noOwner", {}))
              : !contextDockProjection.contextDock.source
                ? translate(locale, "d1.context.noSource", {})
                : null,
        onCommitOrPush: () => openCommitBar(),
      },
    });
    right.dataset.drawerOpen = String(contextDrawerOpen);
    topbar.contextDrawerToggle.setAttribute("aria-expanded", String(contextDrawerOpen));
    right.tabIndex = -1;

    /**
     * The dock's host.
     *
     * Outside focus it is the body's own third grid track. Under focus it
     * moves into the design's `.edgewrap.r` hot zone — the mirror of the Lane
     * sidebar's — so the dock is one pointer move away rather than gone, which
     * is what `D-SIDEBAR`'s "两侧强制 hover 浮窗" says. The component is the
     * same node in both; only its host changes, exactly as the sidebar's does.
     */
    const dockHost = ((): HTMLElement => {
      if (!focusMode) return right;
      const edge = document.createElement("div");
      edge.className = "edgewrap r d1-dock-edge";
      edge.dataset.dockEdge = "true";
      edge.dataset.peek = String(dockPeek);
      const hint = document.createElement("span");
      hint.className = "edgehint";
      hint.setAttribute("aria-hidden", "true");
      edge.append(hint);
      const panel = document.createElement("div");
      panel.className = "floatpanel d1-dock-float";
      panel.append(right);
      edge.append(panel);
      edge.addEventListener("pointerenter", () => showDockPeek());
      edge.addEventListener("pointerleave", () => scheduleDockPeekClose());
      return edge;
    })();

    const status = renderStatusbar(
      projection.statusbar,
      locale,
      // The gate segment is a destination like any other, so it goes through
      // the cockpit's router and lands on the in-cockpit decision queue rather
      // than replacing the window the operator is working in.
      (route) => navigate(route),
      {
        ambient: statusbarAmbient,
        open: statusbarConfigOpen,
        onToggleOpen: () => {
          statusbarConfigOpen = !statusbarConfigOpen;
          render(false);
        },
        onToggleSegment: (id: StatusbarAmbientSegment) => {
          const next = { ...statusbarAmbient, [id]: !statusbarAmbient[id] };
          if (!options.layout || !layoutRecord.capabilityAvailable) {
            // No record to write to: the set stays session-local, which the
            // popover's own note says out loud.
            statusbarAmbient = next;
            render(false);
            return;
          }
          // Core stores the *hidden* half, and names it does not recognize are
          // carried through untouched: a newer client's hidden segment must
          // not be unhidden by this one rewriting the list from its own
          // vocabulary.
          const known = new Set<string>(STATUSBAR_AMBIENT_SEGMENTS);
          const foreign = layoutRecord.hiddenStatusbarSegments.filter(
            (segment) => !known.has(segment),
          );
          const hidden = STATUSBAR_AMBIENT_SEGMENTS.filter((segment) => !next[segment]);
          void sendLayoutPatch({ hiddenStatusbarSegments: [...hidden, ...foreign] });
        },
        note: layoutNote(),
      },
    );
    const currentFrame = root.querySelector<HTMLElement>('[data-screen="d1-cockpit"]');
    const currentBody = currentFrame?.querySelector<HTMLElement>(":scope > .d1-body");
    const currentActivity = currentBody?.querySelector<HTMLElement>(
      ':scope > [data-shell-landmark="activity-rail"]',
    );
    // The rail may sit directly in the body (pinned) or inside the floating
    // hot zone, so it is resolved by landmark rather than by position.
    const currentLanes = currentBody?.querySelector<HTMLElement>(
      '[data-shell-landmark="lane-rail"]',
    );
    // A mode switch changes the sidebar's *host*, not its contents, so the
    // in-place refresh cannot express it: rebuild the frame instead.
    const sidebarHostUnchanged = currentBody?.dataset.laneSidebarMode === laneSidebarMode;
    const currentTopbar = currentFrame?.querySelector<HTMLElement>(
      ':scope > [data-shell-landmark="topbar"]',
    );
    const currentMain = currentBody?.querySelector<HTMLElement>(
      ':scope > [data-shell-landmark="lane-work-surface"]',
    );
    // Resolved by landmark, because under focus the dock lives inside the
    // right-edge hot zone rather than directly in the body.
    const currentRight = currentBody?.querySelector<HTMLElement>(
      '[data-shell-landmark="context-dock"]',
    );
    // A focus switch changes the dock's *host*, not its contents, so the
    // in-place refresh cannot express it: rebuild the frame instead. Same rule
    // as the sidebar's mode switch above.
    const dockHostUnchanged = currentBody?.dataset.focusMode === String(focusMode);
    const currentStatus = currentFrame?.querySelector<HTMLElement>(
      ':scope > [data-shell-landmark="statusbar"]',
    );
    if (
      !showWelcome &&
      currentFrame &&
      currentBody &&
      currentActivity &&
      currentLanes &&
      sidebarHostUnchanged &&
      dockHostUnchanged &&
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
      currentFrame.dataset.centerView = frame.dataset.centerView ?? "transcript";
      currentBody.className = body.className;
      currentBody.dataset.cockpitLayout = body.dataset.cockpitLayout;
      // The mode itself cannot change on this path (a mode switch rebuilds the
      // frame), but whether the pinned column takes a track can, so the flag
      // the grid reads is written rather than left on the discarded node.
      currentBody.dataset.laneColumn = body.dataset.laneColumn ?? "false";
      currentBody.dataset.focusMode = body.dataset.focusMode ?? "false";
      const currentDockEdge = currentBody.querySelector<HTMLElement>("[data-dock-edge]");
      if (currentDockEdge) currentDockEdge.dataset.peek = String(dockPeek);
      // The hot zone itself persists with its pointer listeners; only the peek
      // state it renders moves, so it is written rather than rebuilt.
      const currentEdge = currentBody.querySelector<HTMLElement>("[data-lane-edge]");
      if (currentEdge) currentEdge.dataset.peek = String(laneRailPeek);
    } else {
      if (showWelcome) body.append(activity, main);
      else body.append(activity, laneHost, main, dockHost);
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
  // A returning draft reopens the popover where the operator left it. This is
  // the D4 Cancel path: the wizard was a detour, and the compact creator is
  // still the surface they were using.
  if (options.newLaneDraft) openAgentMenu("tabs");
  // One no-traffic read at mount, so the review entry points know whether Core
  // publishes structured diff rows before anyone clicks one. It sends no Core
  // command, so it costs nothing on a Core that does not have the capability.
  ensureReviewCapability();
  // The same at-mount, no-traffic read for the evidence archive, so the
  // palette row and the `⌘E` chord know whether Core publishes one before
  // anyone reaches for either.
  ensureEvidenceCapability();
  // The same at-mount, no-traffic read for the action side, so the commit bar
  // and the titlebar sync chip know whether they may act before anyone presses
  // one of them.
  readOperatorGit();
  // The ordered conversation, read once the capability is known. A Core
  // without it keeps the two named unavailable rows rather than an empty
  // transcript, which is what GUI-CORE-009 reported.
  ensureTranscriptRows();
  // The same no-traffic read for single-file reads, so the dock inspector's
  // Open and the Code tab know whether Core answers before anyone clicks.
  ensureWorkspaceFile();
  // The layout record, read before anything is drawn twice: the operator's
  // sidebar mode and hidden statusbar segments are Core's facts, so the first
  // frame that can carry them does.
  void refreshLayoutRecord();
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
