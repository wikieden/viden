import { translate, type Locale } from "../i18n/catalog";
import {
  D15_ALREADY_DECIDED_CODE,
  D15_ALREADY_HANDED_OFF_CODE,
  D15_AMBIGUOUS_OWNER_CODE,
  D15_CAPABILITY_ABSENT_CODE,
  D15_NO_PLAN_CODE,
  D15_NO_STEPS_CODE,
  D15_NOT_ACCEPTED_CODE,
  D15_PLAN_MODE_CODE,
  D15_REVISION_STALE_CODE,
  PLAN_DOCUMENTS_CAPABILITY,
  rejectReasonBytes,
  type D15Intent,
  type D15IntentResult,
  type D15Plan,
  type D15PlanStudioProjection,
} from "../models/plan";
import "./d15_plan_studio.css";
import { verbatimField } from "../ui/verbatim_field";

/**
 * D15 Plan Studio — the cockpit's centre-pane view over `runtime.plan_documents`.
 *
 * Every control maps to one Core fact or one Core command, and every control
 * that cannot act says which rule holds it. Three honest states are the point
 * of the screen:
 *
 * - **no plan proposed** is absence, never an empty plan. A Core that holds no
 *   plan for this owner gets a sentence about the owner, not a blank form.
 * - **`plan_revision_stale`** is Core's refusal, rendered in Core's own words.
 *   The view then re-reads the record Core holds rather than deciding again on
 *   what it was already holding, because the whole point of the pin is that the
 *   operator decides on bytes they read.
 * - **`plan_mode`** is the one mutation Core refuses while plan mode is active.
 *   Hand off renders disabled and labelled, never hidden and never enabled with
 *   a refusal waiting behind it.
 *
 * The decision row sends the revision and `content_sha256` **this render drew**.
 * That is what makes a decision pinned, and it is why the intent carries them
 * rather than letting the host resolve them fresh at send time.
 */

export type { D15PlanStudioProjection } from "../models/plan";

export interface D15Controller {
  applyProjection: (projection: D15PlanStudioProjection) => void;
  dispose: () => void;
}

export interface D15Options {
  /** Sends one decision and resolves with Core's own answer. */
  send?: (intent: D15Intent) => Promise<D15IntentResult>;
  /** Re-reads the record Core holds — the stale refusal's only remedy. */
  reload?: () => Promise<D15PlanStudioProjection>;
  /** Opens the DAG a handed-off plan became, in D13. */
  onOpenDag?: (dagId: string) => void;
  /** Opens the audit trail scoped to one Core audit object. */
  onOpenAuditTrail?: (scope: { kind: string; id: string }) => void;
}

type Outcome = D15IntentResult["outcome"];

const IDLE_OUTCOME: Outcome = { state: "idle", reason: null };

/** The audit object kind Core files a plan decision under. */
const PLAN_AUDIT_KIND = "plan";

/** Section kinds in `PlanSectionKind`'s own declaration order, for labelling. */
const SECTION_LABELS = {
  requirements: "d15.section.requirements",
  assumptions: "d15.section.assumptions",
  constraints: "d15.section.constraints",
  architecture_decisions: "d15.section.architectureDecisions",
  risks: "d15.section.risks",
  test_strategy: "d15.section.testStrategy",
  acceptance_criteria: "d15.section.acceptanceCriteria",
} as const;

const STEP_STATUS_LABELS = {
  pending: "d15.step.pending",
  in_progress: "d15.step.inProgress",
  completed: "d15.step.completed",
} as const;

/** The design's step glyphs, from the registered `.psstep .sg` vocabulary. */
const STEP_GLYPHS: Record<string, string> = {
  pending: "◌",
  in_progress: "▶",
  completed: "✓",
};

const PLAN_STATUS_LABELS = {
  drafting: "d15.status.drafting",
  proposed: "d15.status.proposed",
  accepted: "d15.status.accepted",
  rejected: "d15.status.rejected",
  handed_off: "d15.status.handedOff",
  superseded: "d15.status.superseded",
} as const;

export function renderD15PlanStudio(
  root: HTMLElement,
  initial: D15PlanStudioProjection,
  locale: Locale,
  options: D15Options = {},
): D15Controller {
  let projection = initial;
  let outcome: Outcome = IDLE_OUTCOME;
  let pendingCommandId: string | null = null;
  /** The reason draft, open only while the operator is writing one. */
  let rejectDraft: string | null = null;
  let disposed = false;

  const t = <K extends Parameters<typeof translate>[1]>(
    key: K,
    args: Parameters<typeof translate<K>>[2],
  ) => translate(locale, key, args);

  /** A muted sentence: absence stated, never an error. */
  const note = (text: string, marker?: string): HTMLElement => {
    const line = document.createElement("p");
    line.className = "d15-muted";
    if (marker) line.dataset.d15Note = marker;
    line.textContent = text;
    return line;
  };

  const keyValue = (key: string, value: string, marker: string, dim = false): HTMLElement => {
    const row = document.createElement("div");
    row.className = "d15-kv";
    row.dataset.d15Record = marker;
    const k = document.createElement("span");
    k.className = "d15-k";
    k.textContent = key;
    const v = document.createElement("span");
    v.className = dim ? "d15-v d15-dim" : "d15-v";
    v.textContent = value;
    row.append(k, v);
    return row;
  };

  /** Core's timestamps are seconds; a record compared across machines is UTC. */
  const clock = (seconds: number | null): string => {
    if (seconds === null) return "—";
    const date = new Date(seconds * 1000);
    if (Number.isNaN(date.getTime())) return "—";
    return `${date.toISOString().slice(0, 19).replace("T", " ")} UTC`;
  };

  const shortSha = (sha: string): string =>
    sha.length > 16 ? `${sha.slice(0, 8)}…${sha.slice(-4)}` : sha;

  /**
   * The short phrase a blocked control prints inside itself.
   *
   * The full sentence goes to the accessible name, where a screen reader and a
   * tooltip both reach it; the button carries the phrase, because a paragraph
   * inside a control makes the decision row taller than the plan it decides.
   * Both come from the same Core code, so they can never disagree.
   */
  const blockPhrase = (code: string | null): string | null => {
    switch (code) {
      case null:
        return null;
      case D15_PLAN_MODE_CODE:
        return t("d15.why.planMode", {});
      case D15_NO_PLAN_CODE:
        return t("d15.why.noPlan", {});
      case D15_CAPABILITY_ABSENT_CODE:
        return t("d15.why.capability", {});
      case D15_ALREADY_DECIDED_CODE:
        return t("d15.why.decided", {});
      case D15_NOT_ACCEPTED_CODE:
        return t("d15.why.notAccepted", {});
      case D15_ALREADY_HANDED_OFF_CODE:
        return t("d15.why.handedOff", {});
      case D15_AMBIGUOUS_OWNER_CODE:
        return t("d15.why.ambiguous", {});
      case D15_NO_STEPS_CODE:
        return t("d15.why.noSteps", {});
      default:
        return t("d15.why.unknown", { code });
    }
  };

  /** The sentence a blocked control carries, by Core's own code. */
  const blockSentence = (code: string | null): string | null => {
    switch (code) {
      case null:
        return null;
      case D15_PLAN_MODE_CODE:
        return t("d15.block.planMode", {});
      case D15_NO_PLAN_CODE:
        return t("d15.block.noPlan", {});
      case D15_CAPABILITY_ABSENT_CODE:
        return t("d15.unavailable.body", { capability: PLAN_DOCUMENTS_CAPABILITY });
      case D15_ALREADY_DECIDED_CODE:
        return t("d15.block.decided", {});
      case D15_NOT_ACCEPTED_CODE:
        return t("d15.block.notAccepted", {});
      case D15_ALREADY_HANDED_OFF_CODE:
        return t("d15.block.handedOff", {});
      case D15_AMBIGUOUS_OWNER_CODE:
        return t("d15.block.ambiguous", {});
      case D15_NO_STEPS_CODE:
        return t("d15.block.noSteps", {});
      default:
        // A code this build cannot name is printed as itself rather than
        // nearest-matched to one it can: the reason belongs to Core.
        return t("d15.block.unknown", { code });
    }
  };

  const send = (intent: D15Intent): void => {
    if (!options.send) return;
    outcome = { state: "pending", reason: null };
    render();
    void options.send(intent).then(
      (result) => {
        if (disposed) return;
        projection = result.projection;
        outcome = result.outcome;
        pendingCommandId = result.pendingCommandId;
        // A confirmed rejection has been recorded; the draft is spent.
        if (result.outcome.state === "confirmed") rejectDraft = null;
        render();
      },
      (error: unknown) => {
        if (disposed) return;
        // A host refusal is an outcome too: it is why nothing was sent, and it
        // belongs on screen rather than in a console.
        outcome = {
          state: "rejected",
          reason: error instanceof Error ? error.message : String(error),
        };
        render();
      },
    );
  };

  const reload = (): void => {
    if (!options.reload) return;
    void options.reload().then(
      (next) => {
        if (disposed) return;
        projection = next;
        outcome = IDLE_OUTCOME;
        pendingCommandId = null;
        render();
      },
      (error: unknown) => {
        if (disposed) return;
        outcome = {
          state: "rejected",
          reason: error instanceof Error ? error.message : String(error),
        };
        render();
      },
    );
  };

  /** The plan body: goal, the seven sections, and the steps. */
  const body = (plan: D15Plan): HTMLElement => {
    const pane = document.createElement("div");
    pane.className = "d15-body";
    pane.dataset.d15Body = plan.planId;

    const goal = document.createElement("p");
    goal.className = "d15-goal";
    goal.dataset.d15Goal = "true";
    const goalLabel = document.createElement("b");
    goalLabel.textContent = `${t("d15.goal", {})} · `;
    goal.append(goalLabel, document.createTextNode(plan.goal));
    pane.append(goal);

    const sections = document.createElement("div");
    sections.className = "d15-sections";
    if (plan.sections.length === 0) {
      // A plan with no prose is a plan the author wrote without it, which is a
      // fact about the plan rather than a gap in the screen.
      sections.append(note(t("d15.sections.none", {}), "sections-none"));
    }
    for (const section of plan.sections) {
      const card = document.createElement("section");
      card.className = "d15-section";
      card.dataset.d15Section = section.kind;
      const heading = document.createElement("h4");
      const key = SECTION_LABELS[section.kind as keyof typeof SECTION_LABELS];
      // A kind this build cannot name prints Core's own word: the vocabulary is
      // the contract's and a newer Core will publish one.
      heading.textContent = key ? t(key, {}) : section.kind;
      const text = document.createElement("p");
      text.textContent = section.body;
      card.append(heading, text);
      sections.append(card);
    }
    pane.append(sections);

    const stepsHead = document.createElement("div");
    stepsHead.className = "d15-subhead";
    stepsHead.dataset.d15StepsHead = "true";
    stepsHead.textContent = t("d15.steps", {});
    const count = document.createElement("span");
    count.className = "d15-count";
    count.textContent = t("d15.steps.completed", {
      completed: String(plan.completedSteps),
      total: String(plan.steps.length),
    });
    stepsHead.append(count);
    pane.append(stepsHead);

    if (plan.steps.length === 0) {
      pane.append(note(t("d15.steps.none", {}), "steps-none"));
    }
    for (const step of plan.steps) {
      const row = document.createElement("article");
      row.className = "d15-step";
      row.dataset.d15Step = step.id;
      row.dataset.d15StepStatus = step.status;
      // The edge is queryable on the row itself, so an evidence capture and a
      // spec read the dependency Core declared rather than parsing display text.
      if (step.dependencies.length > 0) {
        row.dataset.d15Dependencies = step.dependencies.join(",");
      }

      const glyph = document.createElement("span");
      glyph.className = "d15-glyph";
      glyph.ariaHidden = "true";
      glyph.textContent = STEP_GLYPHS[step.status] ?? "·";
      const title = document.createElement("span");
      title.className = "d15-step-title";
      title.textContent = step.title;
      const status = document.createElement("span");
      status.className = "d15-step-status";
      const statusKey = STEP_STATUS_LABELS[step.status as keyof typeof STEP_STATUS_LABELS];
      status.textContent = statusKey ? t(statusKey, {}) : step.status;
      const meta = document.createElement("span");
      meta.className = "d15-step-meta";

      const role = document.createElement("span");
      role.className = step.role ? "d15-role" : "d15-role d15-dim";
      // An unset role is said to be unset. Core defaults it to `coder` at
      // handoff; printing that default here would report a role the author
      // never wrote.
      role.textContent = step.role ?? t("d15.step.roleUnset", {});
      meta.append(role);

      const id = document.createElement("span");
      id.textContent = step.id;
      meta.append(id);

      if (step.fileScope.length > 0) {
        const scope = document.createElement("span");
        scope.dataset.d15Scope = "true";
        scope.textContent = step.fileScope.join(" · ");
        meta.append(scope);
      }
      if (step.dependencies.length > 0) {
        const deps = document.createElement("span");
        deps.className = "d15-dep";
        deps.textContent = t("d15.step.after", { steps: step.dependencies.join(" · ") });
        meta.append(deps);
      }
      row.append(glyph, title, status, meta);
      pane.append(row);
    }
    return pane;
  };

  /** The record rail: what Core holds, field by field. */
  const record = (plan: D15Plan | null): HTMLElement => {
    const rail = document.createElement("aside");
    rail.className = "d15-rail";
    rail.dataset.d15Rail = "true";
    rail.setAttribute("aria-label", t("d15.record", {}));

    const head = document.createElement("div");
    head.className = "d15-rail-head";
    const title = document.createElement("h3");
    title.textContent = t("d15.record", {});
    const sub = document.createElement("p");
    sub.className = "d15-muted";
    sub.textContent = plan ? t("d15.record.rule", {}) : t("d15.record.none", {});
    head.append(title, sub);
    rail.append(head);

    const section = document.createElement("div");
    section.className = "d15-rail-sec";
    section.textContent = t("d15.record.section", {});
    rail.append(section);

    const owner = plan
      ? (plan.ownerLaneId ?? plan.ownerSessionId ?? t("d15.owner.unnamed", {}))
      : (projection.ownerLaneId ?? t("d15.owner.session", {}));
    rail.append(keyValue("plan", plan?.planId ?? "—", "plan", !plan));
    rail.append(keyValue("owner", owner, "owner", !plan));
    const statusKey = plan
      ? PLAN_STATUS_LABELS[plan.status as keyof typeof PLAN_STATUS_LABELS]
      : undefined;
    rail.append(
      keyValue("status", plan ? (statusKey ? t(statusKey, {}) : plan.status) : "—", "status", !plan),
    );
    rail.append(keyValue("revision", plan ? String(plan.revision) : "—", "revision", !plan));
    rail.append(
      keyValue("content sha", plan ? shortSha(plan.contentSha256) : "—", "contentSha", !plan),
    );
    rail.append(
      keyValue(
        "proposed by",
        plan
          ? [plan.proposedBy.kind, plan.proposedBy.turnId].filter(Boolean).join(" · ")
          : "—",
        "proposedBy",
        !plan,
      ),
    );
    rail.append(keyValue("created", clock(plan?.createdAt ?? null), "createdAt", !plan));
    rail.append(keyValue("updated", clock(plan?.updatedAt ?? null), "updatedAt", !plan));
    rail.append(
      keyValue(
        "decided",
        plan?.decidedAt
          ? `${clock(plan.decidedAt)}${plan.decisionAuditId ? ` · ${plan.decisionAuditId}` : ""}`
          : "—",
        "decidedAt",
        !plan?.decidedAt,
      ),
    );
    rail.append(
      keyValue(
        "handoff",
        plan?.handoff ? `${plan.handoff.dagId} · ${plan.handoff.auditId}` : "—",
        "handoff",
        !plan?.handoff,
      ),
    );

    if (plan && plan.decisionAuditId && options.onOpenAuditTrail) {
      const audit = document.createElement("button");
      audit.type = "button";
      audit.className = "d15-link";
      audit.dataset.d15OpenAudit = plan.planId;
      audit.textContent = t("d15.record.openAudit", {});
      audit.addEventListener("click", () =>
        options.onOpenAuditTrail?.({ kind: PLAN_AUDIT_KIND, id: plan.planId }),
      );
      rail.append(audit);
    }

    const rolesHead = document.createElement("div");
    rolesHead.className = "d15-rail-sec";
    rolesHead.textContent = t("d15.record.roles", {});
    rail.append(rolesHead);
    const roles = document.createElement("div");
    roles.className = "d15-roles";
    roles.dataset.d15Roles = "true";
    if (!plan || plan.stepsByRole.length === 0) {
      const none = document.createElement("span");
      none.className = "d15-chip d15-dim";
      none.textContent = t("d15.record.rolesNone", {});
      roles.append(none);
    }
    for (const entry of plan?.stepsByRole ?? []) {
      const chip = document.createElement("span");
      chip.className = "d15-chip";
      chip.dataset.d15Role = entry.role;
      chip.textContent = `${entry.role} ×${entry.steps}`;
      roles.append(chip);
    }
    rail.append(roles);

    if (plan) {
      const revisionsHead = document.createElement("div");
      revisionsHead.className = "d15-rail-sec";
      revisionsHead.textContent = t("d15.record.revisions", {});
      rail.append(revisionsHead);
      // The record carries the latest revision only; the reducer upserts by
      // plan id, so a revision list would be a history this client invented.
      // What is honest is the one relation the record itself states.
      rail.append(
        keyValue(`rev ${plan.revision}`, t("d15.record.revisionCurrent", {}), "revisionCurrent"),
      );
      if (plan.revision > 1) {
        rail.append(
          keyValue(
            `rev ${plan.revision - 1}`,
            t("d15.record.revisionSuperseded", {}),
            "revisionSuperseded",
            true,
          ),
        );
        rail.append(note(t("d15.record.revisionNote", {}), "revision-note"));
      }
    }

    const foot = document.createElement("p");
    foot.className = "d15-rail-foot";
    foot.textContent = t("d15.record.foot", {});
    rail.append(foot);
    return rail;
  };

  /** The decision row: refusals, the handoff link, the reason draft, actions. */
  const decisionRow = (plan: D15Plan | null): HTMLElement => {
    const bar = document.createElement("div");
    bar.className = "d15-decide";
    bar.dataset.d15Decide = "true";

    // Core's own refusal, verbatim. `plan_revision_stale` gets the reload the
    // refusal itself prescribes; nothing else is inferred from the text.
    if (outcome.state === "rejected" && outcome.reason) {
      const stale = outcome.reason.includes(D15_REVISION_STALE_CODE);
      const refuse = document.createElement("div");
      refuse.className = stale ? "d15-refuse" : "d15-refuse d15-refuse-soft";
      refuse.dataset.d15Refusal = stale ? D15_REVISION_STALE_CODE : "core";
      refuse.setAttribute("role", "alert");
      const code = document.createElement("span");
      code.className = "d15-refuse-code";
      code.textContent = stale ? D15_REVISION_STALE_CODE : t("d15.refused", {});
      const text = document.createElement("span");
      text.textContent = outcome.reason;
      refuse.append(code, text);
      if (stale && options.reload) {
        const again = document.createElement("button");
        again.type = "button";
        again.className = "d15-link";
        again.dataset.d15Reload = "true";
        again.textContent = t("d15.reload", {});
        again.addEventListener("click", () => reload());
        refuse.append(again);
      }
      bar.append(refuse);
    }

    // The plan-mode refusal is shown before it happens, because Core publishes
    // the mode: the operator learns the handoff is blocked from the control
    // rather than from a rejection after the click.
    if (projection.decision.handOffBlock === D15_PLAN_MODE_CODE) {
      const soft = document.createElement("div");
      soft.className = "d15-refuse d15-refuse-soft";
      soft.dataset.d15Refusal = D15_PLAN_MODE_CODE;
      const code = document.createElement("span");
      code.className = "d15-refuse-code";
      code.textContent = D15_PLAN_MODE_CODE;
      const text = document.createElement("span");
      text.textContent = t("d15.block.planMode", {});
      soft.append(code, text);
      bar.append(soft);
    }

    if (plan?.handoff) {
      const line = document.createElement("div");
      line.className = "d15-handoff";
      line.dataset.d15Handoff = plan.handoff.dagId;
      const text = document.createElement("span");
      text.textContent = t("d15.handoff.done", {
        dag: plan.handoff.dagId,
        tasks: String(plan.steps.length),
        audit: plan.handoff.auditId,
      });
      line.append(text);
      if (options.onOpenDag) {
        const open = document.createElement("button");
        open.type = "button";
        open.className = "d15-link";
        open.dataset.d15OpenDag = plan.handoff.dagId;
        open.textContent = `${t("d15.handoff.open", {})} ↗`;
        const dagId = plan.handoff.dagId;
        open.addEventListener("click", () => options.onOpenDag?.(dagId));
        line.append(open);
      } else {
        line.append(note(t("d15.handoff.openUnavailable", {}), "handoff-open"));
      }
      bar.append(line);
    }

    if (rejectDraft !== null && plan) {
      const row = document.createElement("div");
      row.className = "d15-reason";
      row.dataset.d15Reason = "true";
      const label = document.createElement("label");
      label.className = "d15-reason-label";
      label.htmlFor = "d15-reason-field";
      label.textContent = t("d15.reject.reason", {});
      const field = verbatimField(document.createElement("textarea"));
      field.id = "d15-reason-field";
      field.className = "d15-reason-field";
      field.dataset.d15ReasonField = "true";
      field.rows = 2;
      field.value = rejectDraft;
      field.placeholder = t("d15.reject.placeholder", {});
      // No `maxlength`: a bound that silently truncates is a rejection the
      // operator did not write. Over the bound the send is refused and says so.
      field.addEventListener("input", () => {
        rejectDraft = field.value;
        const over = rejectReasonBytes(field.value) > projection.decision.rejectReasonMaxBytes;
        counter.dataset.d15ReasonOver = String(over);
        counter.textContent = t("d15.reject.bytes", {
          bytes: String(rejectReasonBytes(field.value)),
          max: String(projection.decision.rejectReasonMaxBytes),
        });
        confirm.disabled = over || field.value.trim().length === 0;
      });
      const counter = document.createElement("span");
      counter.className = "d15-reason-count";
      counter.dataset.d15ReasonCount = "true";
      counter.dataset.d15ReasonOver = String(
        rejectReasonBytes(rejectDraft) > projection.decision.rejectReasonMaxBytes,
      );
      counter.textContent = t("d15.reject.bytes", {
        bytes: String(rejectReasonBytes(rejectDraft)),
        max: String(projection.decision.rejectReasonMaxBytes),
      });
      const confirm = document.createElement("button");
      confirm.type = "button";
      confirm.className = "d15-btn d15-btn-reject";
      confirm.dataset.d15RejectConfirm = "true";
      confirm.textContent = t("d15.reject.confirm", {});
      confirm.disabled =
        rejectDraft.trim().length === 0 ||
        rejectReasonBytes(rejectDraft) > projection.decision.rejectReasonMaxBytes;
      confirm.addEventListener("click", () => {
        if (rejectDraft === null) return;
        send({
          type: "reject",
          planId: plan.planId,
          revision: plan.revision,
          contentSha256: plan.contentSha256,
          reason: rejectDraft,
        });
      });
      const cancel = document.createElement("button");
      cancel.type = "button";
      cancel.className = "d15-btn";
      cancel.dataset.d15RejectCancel = "true";
      cancel.textContent = t("d15.reject.cancel", {});
      cancel.addEventListener("click", () => {
        rejectDraft = null;
        render();
      });
      row.append(label, field, counter, confirm, cancel);
      bar.append(row);
    }

    const actions = document.createElement("div");
    actions.className = "d15-actions";

    const pin = document.createElement("p");
    pin.className = "d15-pin";
    pin.dataset.d15Pin = plan ? `${plan.revision}` : "none";
    pin.textContent = plan
      ? t("d15.pin", { revision: String(plan.revision), sha: shortSha(plan.contentSha256) })
      : t("d15.pin.none", {});
    actions.append(pin);

    const spacer = document.createElement("span");
    spacer.className = "d15-spacer";
    actions.append(spacer);

    const button = (
      marker: string,
      label: string,
      enabled: boolean,
      blockedBy: string | null,
      onClick: () => void,
      variant?: string,
    ): HTMLButtonElement => {
      const node = document.createElement("button");
      node.type = "button";
      node.className = variant ? `d15-btn ${variant}` : "d15-btn";
      node.dataset.d15Action = marker;
      node.textContent = label;
      const sentence = blockSentence(blockedBy);
      // Disabled is labelled, and the label is part of the accessible name —
      // not a tooltip a pointer would have to find.
      node.disabled = !enabled;
      if (!enabled && sentence) {
        node.dataset.d15Block = blockedBy ?? "";
        node.title = `${label} — ${sentence}`;
        node.setAttribute("aria-label", `${label} — ${sentence}`);
        const why = document.createElement("span");
        why.className = "d15-why";
        why.textContent = blockPhrase(blockedBy) ?? sentence;
        node.append(why);
      } else {
        node.title = label;
      }
      if (enabled) node.addEventListener("click", onClick);
      return node;
    };

    const sendable = !!options.send && outcome.state !== "pending";
    const hostBlock = options.send ? null : D15_NO_PLAN_CODE;

    actions.append(
      button(
        "reject",
        t("d15.reject", {}),
        sendable && projection.decision.canDecide && plan !== null && rejectDraft === null,
        projection.decision.canDecide ? hostBlock : projection.decision.decideBlock,
        () => {
          rejectDraft = "";
          render();
        },
        "d15-btn-reject",
      ),
    );
    actions.append(
      button(
        "handoff",
        t("d15.handoff", {}),
        sendable && projection.decision.canHandOff && plan !== null,
        projection.decision.canHandOff ? hostBlock : projection.decision.handOffBlock,
        () => {
          if (!plan) return;
          send({
            type: "hand_off",
            planId: plan.planId,
            revision: plan.revision,
            contentSha256: plan.contentSha256,
          });
        },
      ),
    );
    const accept = button(
      "accept",
      t("d15.accept", {}),
      sendable && projection.decision.canDecide && plan !== null,
      projection.decision.canDecide ? hostBlock : projection.decision.decideBlock,
      () => {
        if (!plan) return;
        send({
          type: "accept",
          planId: plan.planId,
          revision: plan.revision,
          contentSha256: plan.contentSha256,
        });
      },
      "d15-btn-go",
    );
    if (!accept.disabled) {
      const chord = document.createElement("kbd");
      chord.textContent = "⌘⏎";
      accept.append(chord);
    }
    actions.append(accept);
    bar.append(actions);

    if (outcome.state === "pending") {
      bar.append(
        note(
          pendingCommandId
            ? t("d15.outcome.pendingNamed", { command: pendingCommandId })
            : t("d15.outcome.pending", {}),
          "outcome-pending",
        ),
      );
    } else if (outcome.state === "confirmed") {
      bar.append(note(t("d15.outcome.confirmed", {}), "outcome-confirmed"));
    }
    return bar;
  };

  /**
   * `⌘⏎` accepts, scoped to this view.
   *
   * Bound on the stage rather than on `window`: the cockpit's composer owns
   * Enter and lives outside this pane, and a window-level chord here would
   * decide a plan from a text field the operator was typing in.
   */
  const handleAcceptChord = (event: KeyboardEvent): void => {
    if (event.key !== "Enter" || !(event.metaKey || event.ctrlKey)) return;
    const plan = projection.plan;
    if (!plan || !options.send || !projection.decision.canDecide) return;
    if (outcome.state === "pending") return;
    event.preventDefault();
    send({
      type: "accept",
      planId: plan.planId,
      revision: plan.revision,
      contentSha256: plan.contentSha256,
    });
  };

  const render = (): void => {
    const stage = document.createElement("section");
    stage.className = "d15-stage";
    stage.dataset.route = "d15";
    stage.dataset.d15Status = projection.plan?.status ?? "none";
    stage.dataset.d15PlanMode = String(projection.planModeActive);
    stage.addEventListener("keydown", handleAcceptChord);

    const head = document.createElement("div");
    head.className = "d15-head";
    const mode = document.createElement("span");
    mode.className = projection.planModeActive ? "d15-mode d15-mode-plan" : "d15-mode";
    mode.dataset.d15Mode = projection.workMode;
    mode.textContent = projection.planModeActive
      ? t("d15.mode.plan", {})
      : t("d15.mode.build", { mode: projection.workMode });
    head.append(mode);

    const titles = document.createElement("div");
    titles.className = "d15-titles";
    const title = document.createElement("h2");
    title.className = "d15-title";
    title.dataset.d15Title = "true";
    title.textContent = projection.plan?.title ?? t("d15.empty.head", {});
    const scope = document.createElement("p");
    scope.className = "d15-scope";
    scope.dataset.d15Scope = "true";
    const ownerLabel = projection.ownerLaneId ?? t("d15.owner.session", {});
    scope.textContent = projection.plan
      ? t("d15.scope", {
          owner: ownerLabel,
          revision: String(projection.plan.revision),
          sha: shortSha(projection.plan.contentSha256),
        })
      : t("d15.scope.none", { owner: ownerLabel });
    titles.append(title, scope);
    head.append(titles);
    stage.append(head);

    const split = document.createElement("div");
    split.className = projection.capabilityAvailable ? "d15-split" : "d15-split d15-split-solo";

    const centre = document.createElement("div");
    centre.className = "d15-centre";

    if (!projection.capabilityAvailable) {
      // The capability gate. Nothing was read and nothing can be sent, and the
      // screen says which capability rather than showing an empty plan.
      const empty = document.createElement("div");
      empty.className = "d15-empty";
      empty.dataset.d15Empty = "capability";
      const heading = document.createElement("p");
      heading.className = "d15-empty-head";
      heading.textContent = t("d15.unavailable.head", {});
      empty.append(
        heading,
        note(t("d15.unavailable.body", { capability: PLAN_DOCUMENTS_CAPABILITY })),
      );
      centre.append(empty);
    } else if (projection.ambiguousPlanIds.length > 0) {
      const empty = document.createElement("div");
      empty.className = "d15-empty";
      empty.dataset.d15Empty = "ambiguous";
      const heading = document.createElement("p");
      heading.className = "d15-empty-head";
      heading.textContent = t("d15.ambiguous.head", {});
      empty.append(
        heading,
        note(t("d15.block.ambiguous", {})),
        note(projection.ambiguousPlanIds.join(", "), "ambiguous-ids"),
      );
      centre.append(empty);
    } else if (!projection.plan) {
      const empty = document.createElement("div");
      empty.className = "d15-empty";
      empty.dataset.d15Empty = "no-plan";
      const heading = document.createElement("p");
      heading.className = "d15-empty-head";
      heading.textContent = t("d15.empty.head", {});
      const hint = document.createElement("p");
      hint.className = "d15-empty-hint";
      hint.textContent = t("d15.empty.planMode", {});
      empty.append(heading, note(t("d15.empty.body", {})), hint);
      centre.append(empty);
    } else {
      centre.append(body(projection.plan));
    }

    centre.append(decisionRow(projection.plan));
    // The record rail is the plan's own fields; with no capability there is no
    // record to print, and an empty one beside the unavailable sentence would
    // read as a plan Core is withholding rather than one it never published.
    if (projection.capabilityAvailable) split.append(centre, record(projection.plan));
    else split.append(centre);
    stage.append(split);
    root.replaceChildren(stage);

    // A reopened reason draft keeps the caret at its end, so a re-render under
    // an ordered Core wake does not send the operator back to the first word.
    const field = root.querySelector<HTMLTextAreaElement>("[data-d15-reason-field]");
    if (field && document.activeElement !== field && rejectDraft !== null && rejectDraft !== "") {
      field.setSelectionRange(field.value.length, field.value.length);
    }
  };

  render();
  return {
    applyProjection: (next) => {
      projection = next;
      render();
    },
    dispose: () => {
      disposed = true;
    },
  };
}
