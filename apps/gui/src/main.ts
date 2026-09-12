import type { CoreClient } from "./host/core_client";
import { createTauriCoreClient } from "./host/tauri_core_client";
import { translate } from "./i18n/catalog";
import type { ComposerControlIntent } from "./models/composer";
import type { PermissionIntent, PermissionIntentResult } from "./components/permission_dock";
import type { D6Intent, D6RecoveryProjection } from "./models/workspace";
import type { LayoutPreferencePatch } from "./models/layout_preferences";
import type { OperatorGitActionRequest } from "./models/operator_git";
import type { RecentWorkResult } from "./models/recent_work";
import {
  requestPreferenceRestore,
  requestPreferenceSave,
  type PreferenceIntentOutcome,
  type PreferenceState,
  type ResolvedPreferences,
} from "./preferences";
import {
  renderD1Cockpit,
  type D1Controller,
  type D1CockpitProjection,
  type D1Intent,
  type D1IntentResult,
  type NewLaneDraft,
} from "./screens/d1_cockpit";
import {
  renderD10LaneMonitor,
  type D10LaneMonitorProjection,
} from "./screens/d10_lane_monitor";
import {
  draftFromD11Projection,
  pendingFromD11Result,
  renderD11Intake,
  type D11Intent,
} from "./screens/d11_intake";
import {
  renderD12IntegrationGate,
  type D12IntegrationGateProjection,
  type D12Intent,
} from "./screens/d12_integration_gate";
import {
  renderD13FleetWorkflow,
  type D13FleetWorkflowProjection,
} from "./screens/d13_fleet_workflow";
import {
  renderD14,
  type D14AuditProjection,
  type D14AuditScope,
} from "./screens/d14_audit_timeline";
import {
  renderD2Decisions,
  type D2DecisionsProjection,
  type D2Intent,
  type D2IntentResult,
} from "./screens/d2_decisions";
import {
  renderD4LaneCreate,
  type D4Intent,
  type D4IntentResult,
  type D4StarterSeed,
} from "./screens/d4_lane_create";
import { applyResolvedTheme, resolveTheme } from "./ui/theme";
import "./ui/theme.css";
import "./ui/tokens.css";
import "./ui/window_chrome.css";

type ShellState = "connecting" | "disconnected" | "empty";

/**
 * The `?screen=` values that open the cockpit on a centre view.
 *
 * `d4` and `d11` are deliberately not here: they are full-window flows that
 * replace the cockpit rather than render inside it.
 */
const CENTER_VIEW_SCREENS: readonly string[] = ["d2", "d10", "d12", "d13", "d14"];

function shellProjection(
  preferences: ResolvedPreferences | undefined,
  shellState: ShellState,
): D1CockpitProjection {
  const locale = preferences?.locale ?? "en";
  const connectionState = shellState === "empty" ? "live" : shellState;
  return {
    preferences: preferences ?? {
      locale,
      skin: "aurora",
      mode: "dark",
      density: "regular",
      motion: "system",
      diagnostics: [],
    },
    selectedLaneId: null,
    // The shell has no confirmed view, so it has no workspace source either:
    // the titlebar's git block stays absent rather than showing a clean tree.
    topbarSource: null,
    contextDock: {
      source: null,
      context: null,
      laneAgent: null,
      provider: null,
      services: [],
      checklist: [],
    },
    lanes: [],
    environment: {
      cwd: "—",
      providerId: "—",
      model: "—",
      workMode: "—",
      permissionLevel: "—",
      tokenTotal: 0,
      costMicroUsd: null,
    },
    liveWork: {
      tasks: [],
      tools: [],
      approvals: [],
      queuedInputs: [],
      evidence: [],
    },
    transcript: [],
    workspaceEligibility: null,
    starterLanePreviews: [],
    agentAdapters: [],
    agentSessions: [],
    composer: {
      editable: false,
      busy: false,
      canCancel: false,
      canSubmitImmediately: false,
    },
    // Presentation-safe placeholders only: before Core is live there is no
    // fact to show, so every segment renders its explicit absent state.
    statusbar: {
      workMode: "—",
      permissionLevel: "—",
      context: null,
      eventStreamPosition: 0,
      lane: null,
      latency: null,
      tokens: null,
      diagnosticsCount: 0,
      requests: null,
      // No Core count yet. `0` would read as an empty decision queue.
      pendingDecisionCount: null,
    },
    permissionDock: { workMode: "—", permissionLevel: "—", request: null },
    recovery: {
      connection: connectionState,
      state: shellState,
      detail:
        shellState === "empty"
          ? null
          : translate(
              locale,
              shellState === "connecting" ? "connection.pending" : "connection.unavailable",
              {},
            ),
      hint: null,
      recoverable: false,
      businessSuccessBlocked: shellState !== "empty",
      usedTokens: null,
      hardTokenLimit: null,
      missingCapabilities: [],
      actions: [],
    },
    unavailableFeatures: [],
  };
}

/**
 * The cockpit controller that currently owns the root, or `null`.
 *
 * `renderD1Cockpit` registers its chords — `⌘K`, `⌘L`, `⌘.`, `⌘G`, `⌘E`,
 * `⌘R`, `⌘O`, `Escape` — on `window`, and holds them until it is disposed.
 * Mounting a second cockpit over the first therefore leaves *two* sets of
 * handlers listening: the pre-hydration shell answers `⌘K` alongside the live
 * cockpit, opens a second command palette, and re-renders its own
 * `connecting` projection into the same root — which is E1 defect 7's visible
 * half, the bound cockpit falling back to `Core connection pending` with the
 * titlebar project chip at `—`. Whichever handler runs last wins the paint, so
 * the symptom is a race and was seen once in three launches.
 *
 * Every mount of a cockpit into the root goes through `claimRoot`, so exactly
 * one controller ever holds the chords.
 */
let rootOwner: D1Controller | null = null;

/// Disposes whatever cockpit owns the root, then records the new owner.
function claimRoot(owner: D1Controller | null): D1Controller | null {
  if (rootOwner && rootOwner !== owner) rootOwner.dispose();
  rootOwner = owner;
  return owner;
}

export function bootstrapShell(
  root: HTMLElement,
  preferences?: ResolvedPreferences,
  connectionState: "connecting" | "disconnected" = "connecting",
): D1Controller {
  const locale = preferences?.locale ?? "en";
  if (preferences) {
    document.documentElement.lang = locale;
    applyResolvedTheme(document.documentElement, resolveTheme(preferences));
  }

  // The cockpit is the stable application shell. Before Core is live, only
  // presentation-safe placeholders and the typed D6 connection state render.
  const projection = shellProjection(preferences, connectionState);
  root.dataset.clientState = connectionState;
  root.dataset.route = "d1";
  claimRoot(null);
  const controller = renderD1Cockpit(
    root,
    projection,
    async () => {
      throw new Error(translate(locale, "connection.pending", {}));
    },
    async () => ({
      projection,
      pendingCommandId: null,
      outcome: { state: "idle", reason: null },
    }),
    undefined,
    undefined,
    { poll: false },
  );
  claimRoot(controller);
  return controller;
}

export async function hydrateShellFromCore(
  root: HTMLElement,
  core: CoreClient = createTauriCoreClient(),
): Promise<void> {
  let resolvedPreferences: ResolvedPreferences | undefined;
  try {
    const preferences = await core.resolvedPreferences();
    resolvedPreferences = preferences ?? undefined;
    if (preferences) {
      bootstrapShell(root, preferences);
    }
    const locale = preferences?.locale ?? "en";
      const sendD4 = async (intent: D4Intent) =>
        await core.d4SendIntent(`gui-d4-${crypto.randomUUID()}`, intent);
      const pollD4 = async () => await core.d4Poll();
      const sendD11 = async (intent: D11Intent) =>
        await core.d11SendIntent(`gui-d11-${crypto.randomUUID()}`, intent);
      const pollD11 = async () => await core.d11Poll();
      const pollD1 = async (laneId?: string, waitForEvent = false) =>
        await core.d1Poll(laneId ?? null, waitForEvent);
      const sendD1 = async (intent: D1Intent) =>
        await core.d1SendIntent(`gui-d1-${crypto.randomUUID()}`, intent);
      const sendPermission = async (intent: PermissionIntent) =>
        await core.permissionSendIntent(`gui-permission-${crypto.randomUUID()}`, intent);
      const recoverD6 = async () => await core.d6Recover();
      const sendD6Intent = async (intent: D6Intent) =>
        await core.d6SendIntent(`gui-d6-${crypto.randomUUID()}`, intent);
      const sendComposerControl = async (
        intent: ComposerControlIntent,
        selectedLaneId: string | null,
      ) => {
        const commandId = `gui-d1-${crypto.randomUUID()}`;
        if (intent.type === "set_work_mode") {
          return await core.setWorkMode(commandId, intent.mode, selectedLaneId);
        }
        if (intent.type === "set_permission_level") {
          return await core.setPermissionLevel(commandId, intent.level, selectedLaneId);
        }
        return await core.selectModel(commandId, intent.providerId, intent.model, selectedLaneId);
      };
      // The Core-owned preference loop. Only a confirmed Core result reaches
      // the live document: the theme and the `lang` attribute follow the
      // resolution Core published, never the operator's unsaved draft.
      const applyConfirmedPreferences = (
        outcome: PreferenceIntentOutcome,
      ): PreferenceIntentOutcome => {
        if (outcome.status === "confirmed") {
          document.documentElement.lang = outcome.resolved.locale;
          applyResolvedTheme(document.documentElement, resolveTheme(outcome.resolved));
        }
        return outcome;
      };
      const preferencePort = {
        isAvailable: async () => await core.preferencesAvailable(),
        save: async (state: PreferenceState) =>
          applyConfirmedPreferences(
            await requestPreferenceSave(state, core, `gui-pref-${crypto.randomUUID()}`),
          ),
        restore: async () =>
          applyConfirmedPreferences(
            await requestPreferenceRestore(core, `gui-pref-${crypto.randomUUID()}`),
          ),
      };
      let activeD1: D1Controller | null = null;
      /**
       * The agent adapters the last cockpit projection carried, for D4's agent
       * step. Kept here rather than re-read: the wizard is entered from a
       * cockpit that already holds Core's answer, and a second
       * `query_agent_adapters` on the way into a form would be traffic for a
       * fact this shell just had.
       */
      let lastAgentAdapters: Array<{
        agentId: string;
        displayName: string;
        startability: string;
      }> = [];
      const onCoreWake = core.onCoreWake;

      // Content Core persisted lives in the workspace, which the webview
      // cannot open. The host reads it and returns an inline data URL.
      const resolveContent = async (reference: string) => await core.agentContent(reference);

      const showD1 = async (
        laneId?: string,
        newLaneDraft?: NewLaneDraft,
      ): Promise<D1Controller | null> => {
        const projection = await core.d1Cockpit(laneId ?? null);
        if (!projection || (laneId && projection.selectedLaneId !== laneId)) {
          throw new Error(
            laneId
              ? `Core did not confirm D1 Lane ${laneId}`
              : "Core did not provide the D1 cockpit projection",
          );
        }
        activeD1?.dispose();
        // Includes the pre-hydration shell: its window chords would otherwise
        // keep answering alongside this cockpit's (E1 defect 7).
        claimRoot(null);
        root.dataset.route = "d1";
        root.dataset.clientState = "connected";
        document.documentElement.lang = projection.preferences.locale;
        const currentPreferences = await core.resolvedPreferences();
        if (currentPreferences) {
          applyResolvedTheme(document.documentElement, resolveTheme(currentPreferences));
        }
        if (projection.selectedLaneId) {
          root.dataset.focusLaneId = projection.selectedLaneId;
        } else {
          delete root.dataset.focusLaneId;
        }
        lastAgentAdapters = projection.agentAdapters.map((adapter) => ({
          agentId: adapter.agentId,
          displayName: adapter.displayName,
          startability: adapter.startability,
        }));
        activeD1 = renderD1Cockpit(
          root,
          projection,
          sendD1,
          pollD1,
          sendPermission,
          recoverD6,
          {
            // "Full setup…" is the *Lane* wizard, which is what the popover it
            // sits in is creating. D11 is project intake — probe, config
            // confirmation, credentials — and reaching it from a Lane creator
            // asked the operator to configure the project to make one Lane.
            // It stays reachable from the project surfaces: the project
            // picker's `Configure this project…` row and `?screen=d11`.
            onFullSetup: (draft) => void showD4([], draft),
            // A draft the D4 wizard is handing back on Cancel reopens the
            // popover where the operator left it.
            newLaneDraft,
            onCoreWake,
            resolveContent,
            sendD6Intent,
            sendComposerControl,
            preferences: preferencePort,
            loadRecentWork,
            onPickProjectFolder: pickProjectFolder,
            onOpenWorkspace: openWorkspace,
            loadPaletteCrossLane,
            loadWorkspaceFiles,
            workspaceDiff: workspaceDiffPort,
            operatorGit: operatorGitPort,
            layout: layoutPort,
            transcriptRows: transcriptRowsPort,
            evidence: evidencePort,
            // EvidenceView's footer opens the audit trail scoped to the
            // evidence object, the same one-way `D-AUDIT` link D12's baseline
            // chips use. The route is D14's own; nothing new is invented here.
            onOpenAuditTrail: openAuditTrail,
            // D2, D10, D12, D13, and D14 are centre-pane views of this same
            // cockpit; the shell supplies the Core read and the screen, the
            // cockpit supplies the chrome, the Close control, and `Esc`.
            secondaryViews: { mount: mountSecondaryView },
            onNavigate: (route: string) => {
              // What is left here is the two flows that really do replace the
              // window: D11 project intake and the D4 Lane wizard. Every other
              // route the palette or a screen names is handled in-cockpit
              // above and never reaches this callback.
              if (route === "d4") void showD4();
              else if (route === "d11") void showD11();
            },
          },
        );
        claimRoot(activeD1);
        return activeD1;
      };

      // D11 is the full project intake and first-run setup flow. It is entered
      // from the agent menu's full-setup action and from `?screen=d11`; the
      // Welcome "Open project" path stays the compact native folder flow.
      //
      // Core publishes no first-run signal (the D11 projection carries only
      // probe/preview/confirmed-config facts), so the shell never redirects
      // into D11 on its own.
      const showD11 = async () => {
        activeD1?.dispose();
        claimRoot(null);
        activeD1 = null;
        // `d11_poll` is the entry read as well as the wait: it drains the
        // ordered events already queued and reports any command still awaiting
        // its Core receipt, so the screen resumes instead of restarting.
        const result = await pollD11();
        root.dataset.route = "d11";
        // One bounded Core read per D11 entry: intent-driven rerenders reuse
        // the same inventory instead of reissuing `QueryRecentWork` each time.
        let recentWorkRead: Promise<RecentWorkResult> | null = null;
        renderD11Intake(
          root,
          result.projection,
          sendD11,
          locale,
          pollD11,
          {
            draft: draftFromD11Projection(result.projection),
            pending: pendingFromD11Result(result),
          },
          // Starter Lanes are created by D4, which owns the preview/confirm
          // receipt loop; D11 only hands over the seeds the operator picked.
          (queue) => void showD4([...queue]),
          undefined,
          () => void showD1(),
          { load: () => (recentWorkRead ??= loadRecentWork()) },
        );
      };

      const showD4 = async (queue: D4StarterSeed[] = [], seed?: NewLaneDraft) => {
        activeD1?.dispose();
        claimRoot(null);
        activeD1 = null;
        const d4Result = await pollD4();
        root.dataset.route = "d4";
        renderD4LaneCreate(root, d4Result, sendD4, pollD4, locale, {
          queue,
          queueIndex: 0,
          completedLaneIds: [],
          seed,
          agents: lastAgentAdapters,
          // Cancel returns to the cockpit carrying the draft the wizard was
          // entered with, so a detour through the full form never costs the
          // operator what they had already typed.
          onCancel: () => void showD1(undefined, seed),
          onNavigateToD1: (laneId) => {
            // D4 supplies only the exact Core receipt Lane id; D1 then
            // re-reads the canonical view before it renders or sends.
            void showD1(laneId);
          },
        });
      };

      /**
       * The command palette's cross-Lane index.
       *
       * The D1 cockpit projection is Lane-scoped, so gates and asks belonging
       * to other Lanes are read from the projections that already own them —
       * `d2_decisions` and `d12_integration_gate`. No new Core capability is
       * involved, and nothing here derives a decision the projections did not
       * publish. A rejection propagates: the palette states it in place of the
       * section rather than showing an empty one.
       */
      const loadPaletteCrossLane = async () => {
        const [decisions, gate] = await Promise.all([
          core.d2Decisions(null),
          core.d12IntegrationGate(null),
        ]);
        return {
          // The projection already orders active gates ahead of dormant ones;
          // this preserves that order and carries the flag the palette tags.
          gates: (gate?.gates ?? []).map((entry) => ({
            gateId: entry.gateId,
            taskId: entry.taskId,
            status: entry.status,
            dormant: entry.dormant,
          })),
          asks: (decisions?.groups ?? []).flatMap((group) =>
            group.items.map((item) => ({
              id: item.id,
              title: item.title,
              kind: item.kind,
              laneId: item.laneId,
            })),
          ),
        };
      };

      // D2 is the cross-Lane decision queue. It reads the same Core facts as
      // the D1 permission dock, so it never keeps a second decision model.
      //
      // `selectedId` is Core's own selection input: the projection comes back
      // carrying that decision's detail, so a palette jump lands on the record
      // it named rather than on the queue's default head.
      const showD2 = async (container: HTMLElement, selectedId: string | null) => {
        const projection = await core.d2Decisions(selectedId);
        if (!projection) {
          throw new Error("Core did not provide the D2 decision projection");
        }
        renderD2Decisions(
          container,
          projection,
          async (intent: D2Intent) =>
            await core.d2SendIntent(`gui-d2-${crypto.randomUUID()}`, intent),
          locale,
          openAuditTrail,
        );
      };

      // D10 watches every Lane across projects. It is read-only: every
      // actionable decision routes back into D2.
      const showD10 = async (container: HTMLElement) => {
        const projection = await core.d10LaneMonitor();
        if (!projection) {
          throw new Error("Core did not provide the D10 lane monitor projection");
        }
        const controller = renderD10LaneMonitor(
          container,
          projection,
          locale,
          () => activeD1?.openCenterView("d2"),
          null,
          {
            // Attach is the cockpit's own Lane hand-off, not a Core command:
            // it selects the Lane through the shared selection path and
            // returns the centre pane to the conversation.
            attach: (laneId) => activeD1?.selectLane(laneId),
            // Stop is Core's `CancelAgentSession`, correlated by this command
            // id and reported from the answering event — never from the click.
            stop: async (laneId, sessionId) => {
              let result = await core.d1SendIntent(`gui-d10-stop-${crypto.randomUUID()}`, {
                type: "cancel_agent_session",
                laneId,
                sessionId,
              });
              for (
                let attempt = 0;
                attempt < 4 && result.outcome.state === "pending";
                attempt += 1
              ) {
                result = await core.d1Poll(laneId, true);
              }
              return result.outcome;
            },
          },
        );
        // The event ticker is a Core audit read, so it resolves after the
        // cards mount rather than blocking them. A refusal degrades the strip
        // alone and states Core's own words in place of it (GUI-CORE-014).
        void (async () => {
          try {
            let page = await core.d10Events(`gui-d10-events-${crypto.randomUUID()}`);
            for (let attempt = 0; attempt < 4 && page.outcome.state === "pending"; attempt += 1) {
              page = await core.d10EventsPoll();
            }
            controller.applyEvents({
              rows: page.rows,
              loaded: page.loaded,
              capabilityAvailable: page.capabilityAvailable,
              outcome: page.outcome,
            });
          } catch (error: unknown) {
            controller.applyEvents({
              rows: [],
              loaded: false,
              capabilityAvailable: true,
              outcome: {
                state: "rejected",
                reason: error instanceof Error ? error.message : String(error),
              },
            });
          }
        })();
      };

      // D12 is the integration-gate failure path. Accept opens only when Core
      // says every required evidence id is present.
      const showD12 = async (container: HTMLElement, gateId: string | null) => {
        const projection = await core.d12IntegrationGate(gateId);
        if (!projection) {
          throw new Error("Core did not provide the D12 integration gate projection");
        }
        renderD12IntegrationGate(
          container,
          projection,
          locale,
          // Selecting another gate is the same view scoped to another Core id,
          // so it re-enters through the cockpit's router rather than rendering
          // a second screen over this one.
          (next) => activeD1?.openCenterView("d12", next),
          async (intent: D12Intent) =>
            await core.d12SendIntent(`gui-d12-${crypto.randomUUID()}`, intent),
          openAuditTrail,
        );
      };

      // D14 is the audit trail. Audit mode reads Core's append-only audit
      // store; raw replay mode stays available as the diagnostic event log and
      // is the only mode when Core published no `runtime.audit`. Neither mode
      // reconstructs history from the current view state.
      //
      // `scope` is the route argument, `kind:id`, exactly the way the palette
      // hands D2 and D12 the one Core id to preselect.
      const showD14 = async (container: HTMLElement, scope: string | null) => {
        const parsed = parseAuditScope(scope ?? undefined);
        const audit = await queryAudit(parsed);
        // Raw mode is pre-loaded only when it is the mode D14 will open in, so
        // an available audit trail costs no replay traffic.
        const raw = audit.capabilityAvailable ? null : await core.d14AuditTimeline(null, 200);
        renderD14(
          container,
          audit,
          locale,
          {
            queryAudit: (next) => queryAudit(next),
            loadOlderAudit: async () =>
              await core.d14AuditLoadOlder(`gui-audit-${crypto.randomUUID()}`),
            loadRaw: async (after) => await core.d14AuditTimeline(after, 200),
          },
          raw,
        );
      };

      /// One bounded audit read. The send call waits briefly; a slower Core is
      /// drained here rather than reported as an empty timeline.
      const queryAudit = async (scope: D14AuditScope | null): Promise<D14AuditProjection> => {
        let result = await core.d14AuditQuery(`gui-audit-${crypto.randomUUID()}`, scope);
        for (let attempt = 0; attempt < 4 && result.outcome.state === "pending"; attempt += 1) {
          result = await core.d14AuditPoll();
        }
        return result;
      };

      /// Parses the `kind:id` route argument. The kind never contains `:`
      /// (`AuditObjectRef` restricts it to lowercase, digits, and `_`), while an
      /// audit object id legally may, so the first colon is the separator.
      const parseAuditScope = (raw?: string): D14AuditScope | null => {
        if (!raw) return null;
        const separator = raw.indexOf(":");
        if (separator <= 0 || separator === raw.length - 1) return null;
        return { kind: raw.slice(0, separator), id: raw.slice(separator + 1) };
      };

      /**
       * `D-AUDIT`'s one-way link: an evidence row, a D2 decision, or a D12
       * baseline chip opens the audit trail scoped to that object. It is now a
       * centre-pane switch rather than a window route, so the operator returns
       * to what they were reading with `Esc` instead of navigating back.
       */
      const openAuditTrail = (scope: D14AuditScope) => {
        activeD1?.openCenterView("d14", `${scope.kind}:${scope.id}`);
      };

      // D13 is the fleet board. It is read-only: workflow mutations stay with
      // the Core commands that own the DAG.
      const showD13 = async (container: HTMLElement) => {
        const projection = await core.d13FleetWorkflow();
        if (!projection) {
          throw new Error("Core did not provide the D13 fleet projection");
        }
        // A node's drill is the same Lane hand-off D10's Attach performs, so a
        // node and a card leave the cockpit in exactly one state.
        renderD13FleetWorkflow(container, projection, locale, (laneId) =>
          activeD1?.selectLane(laneId),
        );
      };

      /**
       * The cockpit's secondary-view host.
       *
       * Each of the five screens is a Core read this boundary owns, so the
       * shell answers "what goes in the centre pane" and the cockpit answers
       * "where, with what chrome, and how it closes". The renderers themselves
       * are untouched: they take a container and fill it, which is what they
       * always did — only the container moved from the window root into D1's
       * centre pane, so the titlebar, rails, dock, composer, and statusbar
       * survive the switch and the composer keeps addressing the selected
       * Lane. A rejection propagates: the cockpit renders Core's own words in
       * place of the screen.
       */
      const mountSecondaryView = async (
        route: "d2" | "d10" | "d12" | "d13" | "d14",
        container: HTMLElement,
        arg: string | null,
      ): Promise<void> => {
        if (route === "d2") await showD2(container, arg);
        else if (route === "d10") await showD10(container);
        else if (route === "d12") await showD12(container, arg);
        else if (route === "d13") await showD13(container);
        else await showD14(container, arg);
      };

      const pickProjectFolder = async () =>
        await core.pickProjectFolder(translate(locale, "d1.welcome.openFolderTitle", {}));

      const openProject = async () => {
        const selected = await pickProjectFolder();
        if (selected === null) return;
        await openWorkspace(selected);
      };

      // Opening a workspace is a replacement: Core builds a new supervisor and
      // the host swaps its single adapter slot, so the cockpit must be rebuilt
      // against the new projection rather than kept over stale Lane state.
      const openWorkspace = async (root: string) => {
        await core.openWorkspace(root);
        await showD1();
      };

      // One bounded read of Core's cross-project inventory. The send call waits
      // briefly; a slower Core is drained here rather than reported as an empty
      // history. Nothing falls back to scanning the session home.
      const loadRecentWork = async () => {
        let result = await core.queryRecentWork(`gui-recent-${crypto.randomUUID()}`, 20);
        for (let attempt = 0; attempt < 4 && result.outcome.state === "pending"; attempt += 1) {
          result = await core.recentWorkPoll();
        }
        return result;
      };

      /**
       * Reads the Core workspace file inventory for the palette's `~` scope.
       *
       * Core owns the permission gate, the walk, the exclusions, the ordering,
       * and the page bound; the shell only names the read and waits for the
       * ordered answer. A rejection propagates so the palette states Core's own
       * refusal in place of the section rather than showing an empty inventory
       * (GUI-CORE-022).
       */
      /**
       * The ordered transcript port (`runtime.transcript_rows`, C8,
       * GUI-CORE-009).
       *
       * `read` is the no-traffic projection; `query` reads the newest page for
       * one scope and `loadOlder` pages backwards through Core's own cursor.
       * Core owns the ordering, the owner scope, the row bound and the cursor
       * — the shell only names the scope and waits for the ordered answer.
       */
      const transcriptRowsPort = {
        read: async () => await core.transcriptRows(),
        query: async (laneId: string | null) => {
          let result = await core.queryTranscriptRows(
            `gui-rows-${crypto.randomUUID()}`,
            laneId,
          );
          for (let attempt = 0; attempt < 4 && result.outcome.state === "pending"; attempt += 1) {
            result = await core.transcriptRowsPoll();
          }
          return result;
        },
        loadOlder: async (laneId: string | null) => {
          let result = await core.transcriptRowsLoadOlder(
            `gui-rows-${crypto.randomUUID()}`,
            laneId,
          );
          for (let attempt = 0; attempt < 4 && result.outcome.state === "pending"; attempt += 1) {
            result = await core.transcriptRowsPoll();
          }
          return result;
        },
      };

      /**
       * The cockpit layout port (`ui.layout_preferences`, C5).
       *
       * `read` is the no-traffic projection the cockpit re-reads on every
       * ordered wake, so the Lane sidebar mode and the hidden statusbar
       * segments are Core's record rather than a value this webview
       * remembered. `set` sends one `SetUiLayoutPreferences` and drains until
       * Core answers, exactly as the appearance preferences do; Core owns the
       * bound, the `[ui.layout]` table, and the `persisted` verdict.
       */
      const layoutPort = {
        read: async () => await core.layoutPreferences(),
        set: async (patch: LayoutPreferencePatch) => {
          let result = await core.layoutPreferencesSet(
            `gui-layout-${crypto.randomUUID()}`,
            patch,
          );
          for (let attempt = 0; attempt < 4 && result.outcome.state === "pending"; attempt += 1) {
            result = await core.layoutPreferencesPoll();
          }
          return result;
        },
      };

      /**
       * The DiffReview read pair (GUI-CORE-012).
       *
       * `read` is the no-traffic projection read the cockpit uses to learn the
       * capability and to check staleness on an ordered Core wake; `query`
       * sends the actual `QueryWorkspaceDiff` and drains until Core answers.
       * Core owns the permission gate, the git invocations, the byte bound,
       * and the ordering — the shell only names the target and waits.
       */
      const workspaceDiffPort = {
        read: async () => await core.workspaceDiff(),
        query: async (laneId: string | null) => {
          let result = await core.queryWorkspaceDiff(`gui-diff-${crypto.randomUUID()}`, laneId);
          for (let attempt = 0; attempt < 4 && result.outcome.state === "pending"; attempt += 1) {
            result = await core.workspaceDiffPoll();
          }
          return result;
        },
      };

      /**
       * The operator source-control port (GUI-CORE-020).
       *
       * `read` is the no-traffic projection read; `run` sends one
       * `RunOperatorGitAction` and drains until Core answers; `poll` keeps
       * draining while an action is out, which is how an approval-gated action
       * settles once the operator answers the dock. Core owns the permission
       * gate on the mapped agent tool spec, the audit record, the effect, and
       * the classification of git's stderr — the shell only names the action.
       */
      const operatorGitPort = {
        read: async (laneId: string | null) => await core.operatorGit(laneId),
        run: async (laneId: string | null, action: OperatorGitActionRequest) =>
          await core.runOperatorGitAction(`gui-git-${crypto.randomUUID()}`, laneId, action),
        poll: async (laneId: string | null) => await core.operatorGitPoll(laneId),
      };

      /**
       * The Core-owned evidence archive port (GUI-CORE-025).
       *
       * `read` is the no-traffic projection read the entry points and the open
       * view use for the capability and the staleness signal; `query` sends one
       * `QueryEvidence` and drains until Core answers; `loadOlder` sends the
       * next page through Core's own opaque cursor; `content` sends one
       * `ReadEvidenceContent`. Core owns the archive, the order, the cursor,
       * the bounds, and the hash verification — the shell only names the scope.
       */
      const evidencePort = {
        read: async () => await core.evidenceArchive(),
        query: async (laneId: string | null, kinds: string[]) => {
          let result = await core.queryEvidence(
            `gui-evidence-${crypto.randomUUID()}`,
            laneId,
            kinds,
          );
          for (let attempt = 0; attempt < 4 && result.outcome.state === "pending"; attempt += 1) {
            result = await core.evidencePoll();
          }
          return result;
        },
        loadOlder: async () => {
          let result = await core.evidenceLoadOlder(`gui-evidence-${crypto.randomUUID()}`);
          for (let attempt = 0; attempt < 4 && result.outcome.state === "pending"; attempt += 1) {
            result = await core.evidencePoll();
          }
          return result;
        },
        content: async (evidenceId: string) => {
          let result = await core.readEvidenceContent(
            `gui-evidence-content-${crypto.randomUUID()}`,
            evidenceId,
          );
          for (let attempt = 0; attempt < 4 && result.outcome.state === "pending"; attempt += 1) {
            result = await core.evidenceContentPoll();
          }
          return result;
        },
      };

      const loadWorkspaceFiles = async (prefix: string | null) => {
        let result = await core.queryWorkspaceFiles(`gui-files-${crypto.randomUUID()}`, prefix);
        for (let attempt = 0; attempt < 4 && result.outcome.state === "pending"; attempt += 1) {
          result = await core.workspaceFilesPoll();
        }
        return result;
      };

      const initialProjection = await core.d1Cockpit(null);
      if (initialProjection) {
        // D4 remains an explicit compatibility surface; normal project entry
        // and the D1 `+` action use the compact native/ACP menu.
        const screen = new URLSearchParams(window.location.search).get("screen");
        if (screen === "d4") {
          await showD4();
        } else if (screen === "d11") {
          await showD11();
        } else if (screen && CENTER_VIEW_SCREENS.includes(screen)) {
          // A `?screen=` deep link is an *entry point*, not a second shell:
          // it opens the cockpit and then switches its centre pane, so the
          // chrome, the selected Lane, and the return path are the same ones
          // the rail produces.
          const cockpit = await showD1();
          cockpit?.openCenterView(screen as "d2" | "d10" | "d12" | "d13" | "d14");
        } else {
          await showD1();
        }
      } else {
        root.dataset.clientState = "empty";
        root.dataset.route = "d1";
        const projection = shellProjection(resolvedPreferences, "empty");
        claimRoot(null);
        activeD1 = renderD1Cockpit(
          root,
          projection,
          async () => {
            throw new Error("No project is open");
          },
          async () => ({
            projection,
            pendingCommandId: null,
            outcome: { state: "idle", reason: null },
          }),
          undefined,
          undefined,
          {
            onOpenProject: openProject,
            // Welcome has no workspace to replace, so a recent row opens
            // directly; the guarded switch belongs to the picker, which always
            // has one.
            loadRecentWork,
            onOpenWorkspace: openWorkspace,
            showWelcome: true,
            poll: false,
            // A host is bound even without a project, so the gear opens the
            // panel and states what Core has not published rather than
            // disappearing.
            preferences: preferencePort,
          },
        );
        claimRoot(activeD1);
      }
  } catch {
    // Keep the D1 shell visible and make the failed Core bootstrap explicit.
    bootstrapShell(root, resolvedPreferences, "disconnected");
  }
}

const root = document.querySelector<HTMLElement>("#app");
if (root) {
  bootstrapShell(root);
  void hydrateShellFromCore(root);
}
