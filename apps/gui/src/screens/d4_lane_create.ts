import { translate, type Locale } from "../i18n/catalog";
import "./d4_lane_create.css";

export type D4Preset = "coder" | "reviewer" | "tester";

/**
 * The Agent a starter Lane should begin its first session with
 * (`runtime.starter_lane_agent`, GUI-CORE-030).
 *
 * `null` on a seed is the two-step flow Core kept: the Lane is created and a
 * session is started from the cockpit afterwards. It is a request shape, not a
 * missing value, which is why step 2 offers it as its own row.
 */
export interface D4AgentBinding {
  agentId: string;
  model: string | null;
  task: string | null;
}

export interface D4StarterSeed {
  laneId: string;
  preset: D4Preset;
  branch: string | null;
  worktreePath: string | null;
  agent: D4AgentBinding | null;
}

/** `StarterLaneReceipt.session`, as the host projects it. */
export type D4SessionOutcome =
  | { state: "started"; sessionId: string }
  | { state: "refused"; reason: string };

interface D4ResolvedLane {
  id: string;
  role: string;
  route: string;
  gateStrength: string;
  mutationPolicy: string;
  worktree: string | null;
  branch: string | null;
  target: string;
  dataEgress: string;
  status: string;
  budget: {
    tokenLimit: number | null;
    costLimitMicroUsd: number | null;
    wallTimeLimitSecs: number | null;
  };
  summary: string;
}

export interface D4Preview {
  previewId: string;
  contentSha256: string;
  owner: {
    workspaceId: string;
    projectId: string;
    laneId: string | null;
    sessionId: string | null;
    taskId: string | null;
    turnId: string | null;
  };
  lane: D4ResolvedLane;
  branch: string;
  worktreePath: string;
  baseRevision: string;
  diagnostics: string[];
  /**
   * Core's own echo of the binding on a preview, and always `null` on a
   * receipt. The wizard draws this rather than the draft it holds: the echo is
   * what Core resolved the request to, and an unknown adapter comes back
   * echoed with a `starter_lane.agent_unknown` diagnostic beside it.
   */
  agent: D4AgentBinding | null;
}

export interface D4LaneCreateProjection {
  availability: { available: boolean; capability: string; message: string };
  workMode: string;
  canCreate: boolean;
  preview: D4Preview | null;
  receipt: D4Preview | null;
  pendingApproval: { id: string; title: string; risk: string; target: string } | null;
  outcome: { state: string; reason: string | null; requiresRepreview: boolean };
  /**
   * What `CreateStarterLane` made of the binding. `null` beside a receipt is
   * its own answer — Core published no outcome — and never "no agent".
   */
  receiptSession: D4SessionOutcome | null;
  navigationLaneId: string | null;
}

export type D4Intent =
  | { type: "preview"; request: D4StarterSeed }
  | { type: "create"; request: D4StarterSeed }
  | {
      type: "respond_to_approval";
      requestId: string;
      decision: "allow_once" | "deny";
    };

export interface D4IntentResult {
  projection: D4LaneCreateProjection;
  pendingCommandId: string | null;
  pendingIntent: "preview_starter_lane" | "create_starter_lane" | null;
}

/**
 * What the New Lane popover was holding when the operator chose "Full setup…".
 *
 * Both halves now reach Core. `StarterLaneRequest.agent`
 * (`runtime.starter_lane_agent`) carries the adapter the operator picked and
 * the task they typed, so the wizard opens on that pick and sends it; the task
 * travels as the bound Agent's objective rather than only as the Lane's name.
 * Without a pick the seed is still presentation: it names the Lane and branch,
 * and the task becomes the first message once the Lane is open, which is what
 * the popover's own quick path does.
 */
export interface D4Seed {
  agentId: string | null;
  task: string;
}

export interface D4QueueState {
  queue: readonly D4StarterSeed[];
  queueIndex: number;
  completedLaneIds: string[];
  onCancel: () => void;
  onNavigateToD1: (laneId: string) => void;
  /** The New Lane popover's draft, when the wizard was entered from it. */
  seed?: D4Seed;
  /**
   * Opens the Agent Board on the Lane a started session runs in.
   *
   * Absent leaves the started session a fact rather than a dead control: a
   * caller that cannot host D16 must not draw a link that goes nowhere.
   */
  onNavigateToAgentBoard?: (laneId: string) => void;
  /**
   * The agent adapters Core published, for the agent step. Absent while the
   * shell bound none, which states the step as unavailable rather than drawing
   * an empty list that would read as "no agents installed".
   */
  agents?: ReadonlyArray<{ agentId: string; displayName: string; startability: string }>;
}

export interface D4Controller {
  state: { draft: D4StarterSeed; step: number };
  applyResult: (result: D4IntentResult) => Promise<void>;
}

type SendD4Intent = (intent: D4Intent) => Promise<D4IntentResult>;
type PollD4Intent = () => Promise<D4IntentResult>;

const PRESETS: readonly D4Preset[] = ["coder", "reviewer", "tester"];

function action(label: string, marker: string): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "gbtn";
  button.textContent = label;
  button.dataset[marker] = "true";
  return button;
}

function cloneSeed(seed: D4StarterSeed): D4StarterSeed {
  return { ...seed, agent: seed.agent ? { ...seed.agent } : null };
}

function requestKey(seed: D4StarterSeed): string {
  return JSON.stringify(seed);
}

/**
 * The same slug the New Lane popover previews for a task, so the Lane the
 * wizard proposes is the Lane the popover was about to make.
 */
function laneSlug(task: string): string {
  return (
    task
      .toLowerCase()
      .trim()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 26) || "new-lane"
  );
}

export function renderD4LaneCreate(
  root: HTMLElement,
  initial: D4IntentResult,
  send: SendD4Intent,
  poll: PollD4Intent,
  locale: Locale,
  queueState: D4QueueState,
): D4Controller {
  let projection = initial.projection;
  let pendingCommandId = initial.pendingCommandId;
  let pendingIntent = initial.pendingIntent;
  let queueIndex = queueState.queueIndex;
  const completedLaneIds = [...queueState.completedLaneIds];
  // A task the operator already typed names the Lane and its branch, the same
  // `vd/<slug>` the popover previews. Without a seed the wizard keeps its own
  // starter defaults.
  const seedSlug = queueState.seed ? laneSlug(queueState.seed.task) : null;
  // The popover's pick opens the wizard already bound, and its task becomes the
  // bound Agent's objective. Both travel on `StarterLaneRequest.agent`.
  const seedBinding: D4AgentBinding | null = queueState.seed?.agentId
    ? { agentId: queueState.seed.agentId, model: null, task: queueState.seed.task }
    : null;
  const queued = queueState.queue[queueIndex];
  const firstSeed: D4StarterSeed = queued
    ? // A queued seed names the Lane; the popover's pick is still the pick, so
      // it fills a binding the queue does not carry rather than being dropped.
      { ...queued, agent: queued.agent ?? seedBinding }
    : (seedSlug
      ? {
          laneId: seedSlug,
          preset: "coder" as const,
          branch: `vd/${seedSlug}`,
          worktreePath: null,
          agent: seedBinding,
        }
      : {
          laneId: "starter-coder",
          preset: "coder" as const,
          branch: null,
          worktreePath: null,
          agent: seedBinding,
        });
  const state = { draft: cloneSeed(firstSeed), step: 0 };
  let reviewedRequestKey = projection.preview ? requestKey(state.draft) : null;
  let sending = false;
  let pollTimer: number | null = null;

  /**
   * Hands the receipt on: records the Lane, then either opens the next queued
   * seed or leaves for the cockpit.
   */
  const completeReceipt = (): boolean => {
    const laneId = projection.navigationLaneId;
    if (!laneId || completedLaneIds.includes(laneId)) return false;
    completedLaneIds.push(laneId);
    queueIndex += 1;
    if (queueIndex >= queueState.queue.length) {
      queueState.onNavigateToD1(laneId);
      return true;
    }
    state.draft = cloneSeed(queueState.queue[queueIndex]!);
    state.step = 0;
    reviewedRequestKey = null;
    projection = {
      ...projection,
      canCreate: false,
      preview: null,
      receipt: null,
      pendingApproval: null,
      navigationLaneId: null,
      outcome: { state: "idle", reason: null, requiresRepreview: false },
    };
    pendingCommandId = null;
    pendingIntent = null;
    return false;
  };

  /**
   * A bound creation stops on step 4 instead of leaving for the cockpit.
   *
   * Core answers a binding with a session outcome, and navigating past the
   * receipt would swallow it — a refusal the operator never saw is a Lane they
   * believe has an Agent in it. An unbound creation is the two-step flow that
   * existed before the binding and still navigates straight through.
   */
  const advanceReceipt = (): boolean => {
    if (!projection.navigationLaneId) return false;
    if (state.draft.agent && !completedLaneIds.includes(projection.navigationLaneId)) {
      state.step = 3;
      return false;
    }
    return completeReceipt();
  };

  const controller: D4Controller = {
    state,
    applyResult: async (result) => {
      projection = result.projection;
      pendingCommandId = result.pendingCommandId;
      pendingIntent = result.pendingIntent;
      if (projection.preview && pendingIntent !== "preview_starter_lane") {
        reviewedRequestKey = requestKey(state.draft);
      }
      const navigated = advanceReceipt();
      if (!navigated) render();
    },
  };

  const submit = (intent: D4Intent): void => {
    if (sending) return;
    sending = true;
    render();
    void send(intent)
      .then(controller.applyResult)
      .catch((error: unknown) => {
        sending = false;
        projection = {
          ...projection,
          outcome: {
            state: "rejected",
            reason: String(error),
            requiresRepreview: intent.type !== "preview",
          },
        };
        render();
      })
      .finally(() => {
        sending = false;
      });
  };

  const schedulePoll = (): void => {
    if (!pendingCommandId || pollTimer !== null) return;
    pollTimer = window.setTimeout(() => {
      pollTimer = null;
      void poll()
        .then(controller.applyResult)
        .catch(() => schedulePoll());
    }, 250);
  };

  const render = (): void => {
    const requestChanged =
      reviewedRequestKey !== null && reviewedRequestKey !== requestKey(state.draft);
    const requiresRepreview = projection.outcome.requiresRepreview || requestChanged;
    const creating = pendingIntent === "create_starter_lane";

    const frame = document.createElement("section");
    frame.className = "frame d4-frame";
    frame.dataset.screen = "d4-lane-create";

    const titlebar = document.createElement("header");
    titlebar.className = "vbar d4-titlebar";
    titlebar.dataset.tauriDragRegion = "true";
    const title = document.createElement("h1");
    title.textContent = translate(locale, "d4.title", {});
    title.dataset.tauriDragRegion = "true";
    titlebar.append(title);

    const layout = document.createElement("div");
    layout.className = "d4-layout";
    const navigation = document.createElement("nav");
    navigation.className = "d4-steps";
    navigation.setAttribute("aria-label", translate(locale, "d4.title", {}));
    const stepList = document.createElement("ol");
    // The design's own four (`GUI/pages/Viden - D4 Lane创建流程 (GUI).html`
    // `STEPS`): role & workstation, agent, skill pack, gates & target.
    const stepKeys = [
      "d4.step.role",
      "d4.step.agent",
      "d4.step.skills",
      "d4.step.gates",
    ] as const;
    stepKeys.forEach((key, index) => {
      const item = document.createElement("li");
      const button = action(`${index + 1}. ${translate(locale, key, {})}`, "d4Step");
      button.dataset.d4Step = String(index);
      if (state.step === index) button.setAttribute("aria-current", "step");
      button.addEventListener("click", () => {
        state.step = index;
        render();
        root.querySelector<HTMLElement>("[data-step-heading]")?.focus();
      });
      item.append(button);
      stepList.append(item);
    });
    navigation.append(stepList);

    const form = document.createElement("section");
    form.className = "d4-form";
    form.tabIndex = -1;
    const heading = document.createElement("h2");
    heading.tabIndex = -1;
    heading.dataset.stepHeading = "true";
    heading.textContent = translate(locale, stepKeys[state.step]!, {});
    form.append(heading);

    const laneLabel = document.createElement("label");
    laneLabel.textContent = translate(locale, "d4.laneId", {});
    const laneId = document.createElement("input");
    laneId.dataset.laneId = "true";
    laneId.value = state.draft.laneId;
    laneId.autocomplete = "off";
    laneId.addEventListener("input", () => {
      state.draft.laneId = laneId.value;
      render();
    });
    laneLabel.append(laneId);

    const branchLabel = document.createElement("label");
    branchLabel.textContent = translate(locale, "d4.branch", {});
    const branch = document.createElement("input");
    branch.dataset.branch = "true";
    branch.value = state.draft.branch ?? "";
    branch.autocomplete = "off";
    branch.addEventListener("input", () => {
      state.draft.branch = branch.value || null;
      render();
    });
    branchLabel.append(branch);

    const roleGroup = document.createElement("div");
    roleGroup.className = "d4-role-group";
    roleGroup.setAttribute("role", "radiogroup");
    roleGroup.setAttribute("aria-label", translate(locale, "d4.role", {}));
    PRESETS.forEach((preset, index) => {
      const role = action(translate(locale, `d4.preset.${preset}`, {}), "preset");
      role.setAttribute("role", "radio");
      role.dataset.preset = preset;
      role.setAttribute("aria-checked", String(state.draft.preset === preset));
      role.tabIndex = state.draft.preset === preset ? 0 : -1;
      const choose = () => {
        state.draft.preset = preset;
        if (state.draft.laneId.startsWith("starter-")) {
          state.draft.laneId = `starter-${preset}`;
        }
        render();
      };
      role.addEventListener("click", choose);
      role.addEventListener("keydown", (event) => {
        if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) return;
        event.preventDefault();
        const offset = event.key === "ArrowRight" || event.key === "ArrowDown" ? 1 : -1;
        state.draft.preset = PRESETS[(index + offset + PRESETS.length) % PRESETS.length]!;
        if (state.draft.laneId.startsWith("starter-")) {
          state.draft.laneId = `starter-${state.draft.preset}`;
        }
        render();
        root.querySelector<HTMLElement>(`[data-preset="${state.draft.preset}"]`)?.focus();
      });
      roleGroup.append(role);
    });

    /// One step's own section, with the design's `.shead` lead sentence.
    const stepSection = (marker: string, lead: string): HTMLElement => {
      const section = document.createElement("section");
      section.className = "d4-step-body";
      section.dataset.d4StepBody = marker;
      const description = document.createElement("p");
      description.className = "d4-step-lead";
      description.textContent = lead;
      section.append(description);
      return section;
    };

    /// A fact Core resolved that this wizard does not edit.
    const readOnlyRow = (
      host: HTMLElement,
      label: string,
      value: string,
      marker: string,
    ): void => {
      const row = document.createElement("p");
      row.className = "d4-readonly";
      const term = document.createElement("span");
      term.className = "d4-readonly-key";
      term.textContent = label;
      const detail = document.createElement("span");
      detail.className = "d4-readonly-value";
      detail.dataset[marker] = "true";
      detail.textContent = value;
      row.append(term, detail);
      host.append(row);
    };

    const EM_DASH = "—";
    // Core's resolution of the reviewed request. The preview slot is cleared
    // the moment the receipt lands, and the receipt resolves the same request,
    // so a wizard holding on step 4 reads Core's facts rather than em-dashes
    // about a Lane that exists.
    const resolvedFacts = projection.preview ?? projection.receipt;

    if (state.step === 0) {
      // Step 1 — Role & workstation. The preset is the only editable role
      // model Core has: `StarterLanePreset` is coder / reviewer / tester, and
      // the design's seven-role domain pack (`D-ROLES`) is not on this
      // contract, so the three Core names are the three offered.
      const section = stepSection("role", translate(locale, "d4.step.role.lead", {}));
      section.append(roleGroup, laneLabel, branchLabel);
      if (queueState.seed) {
        // The task is shown, not sent: `StarterLaneRequest` has no field for
        // it. It travels as the operator's first message once the Lane opens,
        // which is exactly what the popover's own Create does.
        readOnlyRow(
          section,
          translate(locale, "d4.seed.task", {}),
          queueState.seed.task,
          "seedTask",
        );
        const note = document.createElement("p");
        note.className = "d4-step-note";
        note.dataset.seedNote = "true";
        note.textContent = translate(locale, "d4.seed.note", {});
        section.append(note);
      }
      readOnlyRow(
        section,
        translate(locale, "d4.worktree", {}),
        resolvedFacts?.worktreePath ?? translate(locale, "d4.auto", {}),
        "autoWorktree",
      );
      readOnlyRow(
        section,
        translate(locale, "d4.base", {}),
        resolvedFacts?.baseRevision ?? translate(locale, "d4.auto", {}),
        "autoBase",
      );
      form.append(section);
    } else if (state.step === 1) {
      // Step 2 — Choose agent. The list is Core's published adapters and the
      // pick is a real binding: it travels on `StarterLaneRequest.agent`
      // (`runtime.starter_lane_agent`) and Core starts the session after the
      // Lane exists. "No agent" is a row of its own rather than the absence of
      // a pick, because an unbound request is the two-step flow Core kept.
      const section = stepSection("agent", translate(locale, "d4.step.agent.lead", {}));
      const agents = queueState.agents ?? [];
      const bindTo = (binding: D4AgentBinding | null): void => {
        state.draft.agent = binding;
        render();
        root
          .querySelector<HTMLElement>(
            binding ? `[data-d4-agent="${binding.agentId}"]` : "[data-d4-agent-none]",
          )
          ?.focus();
      };
      const choiceGroup = document.createElement("div");
      choiceGroup.className = "d4-agent-list";
      choiceGroup.setAttribute("role", "radiogroup");
      choiceGroup.setAttribute("aria-label", translate(locale, "d4.step.agent", {}));
      if (agents.length === 0) {
        const empty = document.createElement("p");
        empty.dataset.d4Unavailable = "agent";
        empty.setAttribute("role", "status");
        empty.textContent = translate(locale, "d4.agent.none", {});
        section.append(empty);
      } else {
        for (const adapter of agents) {
          const item = document.createElement("button");
          item.type = "button";
          item.className = "gbtn d4-agent";
          item.setAttribute("role", "radio");
          item.dataset.d4Agent = adapter.agentId;
          const selected = state.draft.agent?.agentId === adapter.agentId;
          item.setAttribute("aria-checked", String(selected));
          item.tabIndex = selected ? 0 : -1;
          // The popover's own pick stays marked even after the operator moves
          // the binding elsewhere: "what I chose in New Lane" and "what this
          // request will carry" are two different facts.
          if (queueState.seed?.agentId === adapter.agentId) {
            item.dataset.d4AgentChosen = "true";
          }
          const name = document.createElement("strong");
          name.textContent = adapter.displayName;
          const status = document.createElement("small");
          status.textContent = adapter.startability.replaceAll("_", " ");
          item.append(name, status);
          if (queueState.seed?.agentId === adapter.agentId) {
            const pick = document.createElement("em");
            pick.textContent = translate(locale, "d4.agent.chosen", {});
            item.append(pick);
          }
          item.addEventListener("click", () =>
            bindTo({
              agentId: adapter.agentId,
              // Core resolves the adapter's own model; the wizard has no model
              // picker and must not invent one.
              model: null,
              task: queueState.seed?.task ?? null,
            }),
          );
          choiceGroup.append(item);
        }
        const none = document.createElement("button");
        none.type = "button";
        none.className = "gbtn d4-agent";
        none.setAttribute("role", "radio");
        none.dataset.d4AgentNone = "true";
        none.setAttribute("aria-checked", String(state.draft.agent === null));
        none.tabIndex = state.draft.agent === null ? 0 : -1;
        none.textContent = translate(locale, "d4.agent.unbound", {});
        none.addEventListener("click", () => bindTo(null));
        choiceGroup.append(none);
        section.append(choiceGroup);
      }
      // Core's own echo, never the draft: an adapter Core does not know is
      // echoed back with a `starter_lane.agent_unknown` diagnostic and the
      // preview stays creatable, so the two rows have to be read together.
      readOnlyRow(
        section,
        translate(locale, "d4.agent.echo", {}),
        projection.preview?.agent?.agentId ?? EM_DASH,
        "d4AgentEcho",
      );
      for (const diagnostic of projection.preview?.diagnostics ?? []) {
        if (!diagnostic.startsWith("starter_lane.agent")) continue;
        const row = document.createElement("p");
        row.className = "d4-step-note";
        row.dataset.d4AgentDiagnostic = "true";
        row.setAttribute("role", "status");
        row.textContent = diagnostic;
        section.append(row);
      }
      readOnlyRow(
        section,
        translate(locale, "d4.route", {}),
        resolvedFacts?.lane.route ?? EM_DASH,
        "agentRoute",
      );
      const note = document.createElement("p");
      note.className = "d4-step-note";
      note.dataset.d4AgentNote = "true";
      note.textContent = translate(locale, "d4.agent.note", {});
      section.append(note);
      form.append(section);
    } else if (state.step === 2) {
      // Step 3 — Skill pack. Core publishes no skill, pack, or context
      // injection fact anywhere on `frontend-contract-v1`, and
      // `StarterLaneRequest` has no field for one. The step is drawn and
      // stated as unavailable rather than filled with a list this client would
      // have invented.
      const section = stepSection("skills", translate(locale, "d4.step.skills.lead", {}));
      const unavailable = document.createElement("p");
      unavailable.dataset.d4Unavailable = "skills";
      unavailable.setAttribute("role", "status");
      unavailable.textContent = translate(locale, "d4.skills.unavailable", {});
      section.append(unavailable);
      form.append(section);
    } else {
      // Step 4 — Gates & target. Every value is Core's own resolution of the
      // reviewed request; the wizard edits none of them, which is what
      // "policy-as-code" means on this screen.
      const section = stepSection("gates", translate(locale, "d4.step.gates.lead", {}));
      form.append(section);
    }

    /**
     * What `CreateStarterLane` made of the binding, once a receipt exists.
     *
     * Three answers, kept apart. A started session names the id Core minted
     * and links to the board it runs on; a refusal prints Core's own reason
     * without softening it; and no outcome at all says exactly that — the Lane
     * was created and Core published nothing about a session — rather than
     * "no agent", which would be this client answering for Core.
     */
    const appendSessionOutcome = (host: HTMLElement): void => {
      const outcome = projection.receiptSession;
      const block = document.createElement("section");
      block.className = "d4-session";
      block.dataset.d4Session = outcome?.state ?? "absent";
      block.setAttribute("role", "status");
      const heading = document.createElement("h3");
      heading.textContent = translate(locale, "d4.session.title", {});
      block.append(heading);
      const body = document.createElement("p");
      if (outcome?.state === "started") {
        body.textContent = translate(locale, "d4.session.started", {
          session: outcome.sessionId,
        });
      } else if (outcome?.state === "refused") {
        body.textContent = translate(locale, "d4.session.refused", {});
        const reason = document.createElement("p");
        reason.className = "d4-session-reason";
        reason.dataset.d4SessionReason = "true";
        reason.textContent = outcome.reason;
        block.append(body, reason);
      } else {
        body.textContent = translate(locale, "d4.session.absent", {});
      }
      if (!block.contains(body)) block.append(body);
      const laneId = projection.navigationLaneId;
      if (outcome?.state === "started" && laneId && queueState.onNavigateToAgentBoard) {
        const open = action(translate(locale, "d4.session.open", {}), "d4OpenAgentBoard");
        open.addEventListener("click", () => queueState.onNavigateToAgentBoard!(laneId));
        block.append(open);
      }
      if (laneId) {
        const carry = action(translate(locale, "d4.receipt.continue", {}), "d4ReceiptContinue");
        carry.classList.add("primary");
        carry.addEventListener("click", () => {
          if (!completeReceipt()) render();
        });
        block.append(carry);
      }
      host.append(block);
    };

    const resolved = document.createElement("dl");
    resolved.className = "d4-resolved";
    const addResolved = (key: string, value: string, marker: string) => {
      const term = document.createElement("dt");
      term.textContent = key;
      const detail = document.createElement("dd");
      detail.textContent = value;
      detail.dataset[marker] = "true";
      resolved.append(term, detail);
    };
    addResolved(translate(locale, "d4.route", {}), resolvedFacts?.lane.route ?? EM_DASH, "resolvedRoute");
    addResolved(translate(locale, "d4.gate", {}), resolvedFacts?.lane.gateStrength ?? EM_DASH, "resolvedGate");
    addResolved(translate(locale, "d4.target", {}), resolvedFacts?.lane.target ?? EM_DASH, "resolvedTarget");
    addResolved(translate(locale, "d4.budget.default", {}), translate(locale, "d4.budget.default", {}), "resolvedBudget");
    addResolved(translate(locale, "d4.worktree", {}), resolvedFacts?.worktreePath ?? EM_DASH, "resolvedWorktree");
    addResolved(translate(locale, "d4.base", {}), resolvedFacts?.baseRevision ?? EM_DASH, "resolvedBase");
    addResolved(
      translate(locale, "d4.mutationPolicy", {}),
      resolvedFacts?.lane.mutationPolicy ?? EM_DASH,
      "resolvedMutationPolicy",
    );
    if (state.step === 3) {
      const gates = form.querySelector<HTMLElement>('[data-d4-step-body="gates"]')!;
      gates.append(resolved);
      // `ExecutionTarget` has an `Ssh` arm in Core's types, but
      // `StarterLaneRequest` carries no target and the preview always resolves
      // `local`, so the step states the one target it can honestly offer and
      // names where the remote one is designed (D9) rather than drawing a
      // picker that cannot reach Core.
      const target = document.createElement("p");
      target.className = "d4-step-note";
      target.dataset.d4TargetNote = "true";
      target.textContent = translate(locale, "d4.target.localOnly", {});
      gates.append(target);
      if (projection.receipt) appendSessionOutcome(gates);
    }

    const rail = document.createElement("aside");
    rail.className = "d4-summary";
    const summaryTitle = document.createElement("h2");
    summaryTitle.textContent = state.draft.laneId;
    const summary = document.createElement("p");
    summary.textContent = resolvedFacts?.lane.summary ?? translate(locale, "d4.preview", {});
    rail.append(summaryTitle, summary);

    if (!projection.availability.available) {
      const unavailable = document.createElement("p");
      unavailable.setAttribute("role", "alert");
      unavailable.textContent = `${projection.availability.capability} · ${projection.availability.message}`;
      rail.append(unavailable);
    }
    if (requiresRepreview) {
      const warning = document.createElement("p");
      warning.dataset.repreviewRequired = "true";
      warning.setAttribute("role", "alert");
      warning.textContent = projection.outcome.reason
        ? `${translate(locale, "d4.repreview", {})} ${projection.outcome.reason}`
        : translate(locale, "d4.repreview", {});
      rail.append(warning);
    }
    if (projection.pendingApproval) {
      const approval = document.createElement("section");
      approval.className = "d4-approval";
      approval.setAttribute("role", "status");
      approval.textContent = projection.pendingApproval.title;
      const allow = action(translate(locale, "d4.approval.allow", {}), "allowStarterApproval");
      allow.addEventListener("click", () => submit({
        type: "respond_to_approval",
        requestId: projection.pendingApproval!.id,
        decision: "allow_once",
      }));
      const deny = action(translate(locale, "d4.approval.deny", {}), "denyStarterApproval");
      deny.addEventListener("click", () => submit({
        type: "respond_to_approval",
        requestId: projection.pendingApproval!.id,
        decision: "deny",
      }));
      approval.append(allow, deny);
      rail.append(approval);
    }
    if (creating) {
      const waiting = document.createElement("p");
      waiting.dataset.createWaiting = "true";
      waiting.setAttribute("role", "status");
      waiting.setAttribute("aria-live", "polite");
      waiting.textContent = translate(locale, "d4.waiting", {});
      rail.append(waiting);
    }

    layout.append(navigation, form, rail);

    const footer = document.createElement("footer");
    footer.className = "wfoot d4-footer";
    // The design's `.prog` readout, so "how far in am I" is answerable without
    // counting the rail.
    const progress = document.createElement("p");
    progress.className = "prog d4-progress";
    progress.dataset.d4Progress = "true";
    progress.textContent = translate(locale, "d4.progress", {
      step: String(state.step + 1),
      total: String(stepKeys.length),
      name: translate(locale, stepKeys[state.step]!, {}),
    });
    footer.append(progress);
    if (!creating) {
      const cancel = action(translate(locale, "d4.cancel", {}), "cancelD4");
      cancel.addEventListener("click", queueState.onCancel);
      const skip = action(translate(locale, "d4.skip", {}), "skipD4");
      skip.addEventListener("click", queueState.onCancel);
      footer.append(cancel, skip);
    }
    if (state.step > 0) {
      const previous = action(translate(locale, "d4.back", {}), "previousD4Step");
      previous.addEventListener("click", () => {
        state.step -= 1;
        render();
        root.querySelector<HTMLElement>("[data-step-heading]")?.focus();
      });
      footer.append(previous);
    }
    if (state.step < 3) {
      const next = action(
        translate(locale, stepKeys[state.step + 1]!, {}),
        "nextD4Step",
      );
      next.addEventListener("click", () => {
        state.step += 1;
        render();
        root.querySelector<HTMLElement>("[data-step-heading]")?.focus();
      });
      footer.append(next);
    }
    const preview = action(translate(locale, "d4.preview", {}), "previewStarterLane");
    preview.disabled =
      !projection.availability.available || !state.draft.laneId.trim() || sending || creating;
    preview.addEventListener("click", () => {
      if (!preview.disabled) submit({ type: "preview", request: cloneSeed(state.draft) });
    });
    const create = action(translate(locale, "d4.create", {}), "createStarterLane");
    create.classList.add("primary");
    create.disabled =
      !projection.canCreate || requiresRepreview || sending || pendingCommandId !== null;
    create.addEventListener("click", () => {
      if (!create.disabled) submit({ type: "create", request: cloneSeed(state.draft) });
    });
    footer.append(preview, create);
    if (projection.workMode === "plan") {
      const reason = document.createElement("p");
      reason.dataset.createDisabledReason = "true";
      reason.textContent = translate(locale, "d4.create.planDisabled", {});
      footer.append(reason);
    }

    frame.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && !creating) {
        event.preventDefault();
        queueState.onCancel();
        return;
      }
      if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        if (!create.disabled) create.click();
        return;
      }
      if (
        event.key === "Enter" &&
        state.step < 3 &&
        !(event.target instanceof HTMLButtonElement)
      ) {
        event.preventDefault();
        state.step += 1;
        render();
        root.querySelector<HTMLElement>("[data-step-heading]")?.focus();
      }
    });

    frame.append(titlebar, layout, footer);
    root.replaceChildren(frame);
    if (document.activeElement === document.body || !root.contains(document.activeElement)) {
      // The workstation identity field exists only on step 1; on the other
      // steps the form itself takes the caret so the keyboard never lands on
      // a node this render left detached.
      if (laneId.isConnected) laneId.focus();
      else form.focus();
    }
    schedulePoll();
  };

  const alreadyNavigated = advanceReceipt();
  if (!alreadyNavigated) render();
  return controller;
}
