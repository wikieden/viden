import {
  PERMISSION_LEVELS,
  permissionLabel,
  type ModelGroup,
} from "./composer_controls";
import { translate, type Locale, type MessageKey } from "../i18n/catalog";
import type { ComposerControlIntent } from "../models/composer";
import type {
  PreferenceDraft,
  PreferenceIntentOutcome,
  PreferenceState,
} from "../preferences";
import { DENSITIES, MOTIONS, SKINS } from "../ui/theme";
import "./settings_panel.css";

/**
 * The Settings overlay: provider/model, permissions, language, and appearance.
 *
 * Two different Core contracts meet here, and they are deliberately not merged.
 *
 * The language and appearance controls edit a GUI-local draft only. Nothing
 * there writes a preference, re-resolves precedence, or validates the
 * skin/mode pair — Core owns all three, so an invalid pair stays selectable and
 * comes back as Core's own rejection rather than as a client rule the operator
 * cannot see. Rendered authority changes only when a Core result is confirmed.
 *
 * The permission-level and model controls are a *second surface* onto the two
 * commands the composer pills already own (`SetPermissionLevel`,
 * `SelectModel`). They dispatch immediately rather than drafting, they read
 * their current value from the same Core facts the pills read, and they
 * enumerate from the same shared sources (`PERMISSION_LEVELS`, `modelGroups`).
 * One wire path, two surfaces — never a settings-private model.
 *
 * Visual vocabulary: the registered design component
 * `docs/viden-design/Viden/GUI/gui-settings.jsx`. Popover behaviour: the
 * agent-menu conventions (Escape closes and returns focus, an outside click
 * closes, arrow keys move within a group).
 */

export type SettingsField = "locale" | "skin" | "mode" | "density" | "motion";

/** Requestable locales. `system` is a Core value Core resolves per host. */
export const LOCALE_OPTIONS = ["system", "en", "zh-CN"] as const;
/** Requestable modes. Core resolves `system` into dark or light. */
export const MODE_OPTIONS = ["system", "dark", "light"] as const;
/** Skins Core only accepts in dark mode; shown as guidance, never enforced here. */
const DARK_ONLY_SKINS = new Set(["amber", "phosphor"]);

const OPTION_KEYS: Record<string, MessageKey> = {
  "locale:system": "settings.locale.system",
  "locale:en": "settings.locale.en",
  "locale:zh-CN": "settings.locale.zhCN",
  "skin:aurora": "settings.skin.aurora",
  "skin:ice": "settings.skin.ice",
  "skin:mono": "settings.skin.mono",
  "skin:amber": "settings.skin.amber",
  "skin:phosphor": "settings.skin.phosphor",
  "mode:system": "settings.mode.system",
  "mode:dark": "settings.mode.dark",
  "mode:light": "settings.mode.light",
  "density:compact": "settings.density.compact",
  "density:regular": "settings.density.regular",
  "density:comfy": "settings.density.comfy",
  "motion:system": "settings.motion.system",
  "motion:reduced": "settings.motion.reduced",
  "motion:full": "settings.motion.full",
};

/** Localizes one option; an unknown value stays visible as itself. */
export function settingsOptionLabel(
  locale: Locale,
  field: SettingsField,
  value: string,
): string {
  const key = OPTION_KEYS[`${field}:${value}`];
  return key ? translate(locale, key, {}) : value;
}

/**
 * The value a field shows: the operator's unsaved choice when there is one,
 * otherwise the value Core resolved. A draft is never mistaken for authority —
 * it is only what the pending Save would ask for.
 */
export function settingsFieldValue(
  state: PreferenceState,
  field: SettingsField,
): string {
  return state.draft?.[field] ?? state.resolved[field];
}

/**
 * The Core-command half of the panel, mirroring the composer pills.
 *
 * `null` when the cockpit bound no control dispatcher at all (the no-project
 * shell). The sections still render in that case — an absent capability is
 * stated by disabling the controls, never by removing the rows, so the
 * operator can see what Core would own.
 */
export interface SettingsControlsModel {
  /** The level Core published, read from the same statusbar fact the pill reads. */
  permissionLevel: string;
  providerId: string;
  model: string;
  /** Exactly what `modelGroups()` returned for the composer's model pill. */
  groups: ModelGroup[];
  /** The workspace root Core opened. Display only; Core has no scope command. */
  cwd: string;
  /** False while the composer is not editable or no workspace is open. */
  enabled: boolean;
  /** True while any command holds Core's one-command-at-a-time D1 slot. */
  busy: boolean;
}

export interface SettingsPanelModel {
  /** UI language for the panel's own copy. */
  locale: Locale;
  state: PreferenceState;
  /** False when Core's handshake has no `ui.preference_persistence`. */
  available: boolean;
  /** True while a preference command is in flight. */
  saving: boolean;
  /** The last Core outcome, or null before any command in this session. */
  outcome: PreferenceIntentOutcome | null;
  controls: SettingsControlsModel | null;
}

export interface SettingsPanelHandlers {
  onDraft: (update: PreferenceDraft) => void;
  onSave: () => void;
  onCancel: () => void;
  onRestore: () => void;
  onClose: () => void;
  /** Dispatches through the cockpit's shared composer-control path. */
  onControl: (intent: ComposerControlIntent) => void;
}

export interface SettingsPanelController {
  root: HTMLElement;
  close: () => void;
}

interface FieldSpec {
  field: SettingsField;
  titleKey: MessageKey;
  detailKey: MessageKey | null;
  options: readonly string[];
  /** Skins render as the design's chip row rather than a segmented control. */
  chips?: boolean;
}

/**
 * One line per Core permission level.
 *
 * Keyed by the Core CLI name so the enumeration stays owned by
 * `PERMISSION_LEVELS`: a level Core adds renders with its own identifier and no
 * description rather than being silently dropped from the list.
 */
const PERMISSION_DETAIL_KEYS: Record<string, MessageKey> = {
  ask: "settings.permission.ask.detail",
  auto_edit: "settings.permission.auto_edit.detail",
  auto: "settings.permission.auto.detail",
  read_only: "settings.permission.read_only.detail",
  full_access: "settings.permission.full_access.detail",
};

const APPEARANCE_FIELDS: FieldSpec[] = [
  {
    field: "skin",
    titleKey: "settings.skin",
    detailKey: "settings.skin.detail",
    options: SKINS,
    chips: true,
  },
  {
    field: "mode",
    titleKey: "settings.mode",
    detailKey: "settings.mode.detail",
    options: MODE_OPTIONS,
  },
  { field: "density", titleKey: "settings.density", detailKey: null, options: DENSITIES },
  {
    field: "motion",
    titleKey: "settings.motion",
    detailKey: "settings.motion.detail",
    options: MOTIONS,
  },
];

export function renderSettingsPanel(
  anchor: HTMLButtonElement,
  model: SettingsPanelModel,
  handlers: SettingsPanelHandlers,
): SettingsPanelController {
  const { locale } = model;
  const disabled = !model.available || model.saving;

  const panel = document.createElement("div");
  panel.className = "gset-panel";
  panel.dataset.settingsPanel = "true";
  panel.dataset.settingsAvailable = String(model.available);
  panel.setAttribute("role", "dialog");
  panel.setAttribute("aria-modal", "false");
  panel.setAttribute("aria-label", translate(locale, "settings.title", {}));
  panel.setAttribute("aria-busy", String(model.saving));
  panel.tabIndex = -1;

  const header = document.createElement("header");
  header.className = "gset-header";
  const heading = document.createElement("h2");
  heading.className = "gset-heading";
  heading.textContent = translate(locale, "settings.title", {});
  const source = document.createElement("span");
  source.className = "gset-source";
  source.dataset.settingsSource = "true";
  source.textContent = translate(locale, "settings.source", {});
  const close = document.createElement("button");
  close.type = "button";
  close.className = "gset-close";
  close.dataset.settingsClose = "true";
  close.setAttribute("aria-label", translate(locale, "settings.close", {}));
  close.textContent = "×";
  close.addEventListener("click", () => controller.close());
  header.append(heading, source, close);
  panel.append(header);

  if (!model.available) {
    // An absent capability is stated, not hidden: the controls below stay
    // visible and read-only so the operator can see exactly what Core would
    // own once it publishes `ui.preference_persistence`.
    const notice = document.createElement("p");
    notice.className = "gset-unavailable";
    notice.dataset.settingsUnavailable = "true";
    notice.setAttribute("role", "status");
    notice.textContent = translate(locale, "preferences.unavailable", {
      capability: "ui.preference_persistence",
    });
    panel.append(notice);
  }

  const optionGroups: HTMLElement[] = [];

  const renderField = (card: HTMLElement, spec: FieldSpec): void => {
    const row = document.createElement("div");
    row.className = "gset-row";
    const labels = document.createElement("div");
    labels.className = "gset-row-labels";
    const title = document.createElement("div");
    title.className = "gset-row-title";
    title.id = `gset-label-${spec.field}`;
    title.textContent = translate(locale, spec.titleKey, {});
    labels.append(title);
    if (spec.detailKey) {
      const detail = document.createElement("div");
      detail.className = "gset-row-detail";
      detail.textContent = translate(locale, spec.detailKey, {});
      labels.append(detail);
    }

    const group = document.createElement("div");
    group.className = spec.chips ? "gset-seg gset-skins" : "gset-seg";
    group.dataset.settingsField = spec.field;
    group.setAttribute("role", "radiogroup");
    group.setAttribute("aria-labelledby", title.id);
    const current = settingsFieldValue(model.state, spec.field);
    for (const value of spec.options) {
      const option = document.createElement("button");
      option.type = "button";
      option.className = "gset-option";
      option.dataset.settingsOption = `${spec.field}:${value}`;
      option.setAttribute("role", "radio");
      const selected = value === current;
      option.setAttribute("aria-checked", String(selected));
      option.tabIndex = selected ? 0 : -1;
      option.disabled = disabled;
      if (spec.chips) {
        const dot = document.createElement("span");
        dot.className = "gset-skin-dot";
        dot.setAttribute("aria-hidden", "true");
        option.append(dot);
      }
      const label = document.createElement("span");
      label.textContent = settingsOptionLabel(locale, spec.field, value);
      option.append(label);
      if (spec.chips && DARK_ONLY_SKINS.has(value)) {
        // Guidance only. Core is the authority on the pair, so the option
        // stays selectable and an invalid pair returns Core's own rejection.
        const note = document.createElement("small");
        note.className = "gset-skin-note";
        note.textContent = translate(locale, "settings.skin.darkOnly", {});
        option.append(note);
      }
      option.addEventListener("click", () => {
        if (disabled) return;
        handlers.onDraft({ [spec.field]: value } as PreferenceDraft);
      });
      group.append(option);
    }
    optionGroups.push(group);
    row.append(labels, group);
    card.append(row);
  };

  const card = (headingKey: MessageKey, specs: FieldSpec[]): void => {
    const element = document.createElement("section");
    element.className = "gset-card";
    const head = document.createElement("div");
    head.className = "gset-card-head";
    head.textContent = translate(locale, headingKey, {});
    element.append(head);
    for (const spec of specs) renderField(element, spec);
    panel.append(element);
  };

  // The Core-command sections are gated by whether Core can accept a control
  // command right now — never by `ui.preference_persistence`, which governs
  // only the draft-and-save half below. Conflating the two would grey out a
  // working `SetPermissionLevel` because an unrelated capability is absent.
  const controls = model.controls;
  const controlsDisabled = !controls || !controls.enabled || controls.busy;

  /// Builds one immediate-dispatch radio row group inside its own card.
  const controlCard = (
    headingKey: MessageKey,
    detailKey: MessageKey,
    field: "permission" | "model",
    groupLabelKey: MessageKey,
    rows: Array<{ key: string; build: (row: HTMLButtonElement) => void; selected: boolean; intent: ComposerControlIntent }>,
  ): HTMLElement => {
    const element = document.createElement("section");
    element.className = "gset-card";
    const head = document.createElement("div");
    head.className = "gset-card-head";
    head.textContent = translate(locale, headingKey, {});
    const detail = document.createElement("p");
    detail.className = "gset-card-detail";
    detail.textContent = translate(locale, detailKey, {});
    element.append(head, detail);

    const group = document.createElement("div");
    group.className = "gset-rows";
    group.dataset.settingsField = field;
    group.setAttribute("role", "radiogroup");
    group.setAttribute("aria-label", translate(locale, groupLabelKey, {}));
    for (const row of rows) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = field === "permission" ? "gset-pmrow" : "gset-provrow";
      button.dataset.settingsOption = row.key;
      button.setAttribute("role", "radio");
      button.setAttribute("aria-checked", String(row.selected));
      button.tabIndex = row.selected ? 0 : -1;
      button.disabled = controlsDisabled;
      row.build(button);
      button.addEventListener("click", () => {
        if (controlsDisabled) return;
        handlers.onControl(row.intent);
      });
      group.append(button);
    }
    optionGroups.push(group);
    element.append(group);
    panel.append(element);
    return element;
  };

  const providerCard = controlCard(
    "settings.provider",
    "settings.provider.detail",
    "model",
    "d1.controls.model",
    (controls?.groups ?? []).flatMap((group) =>
      group.models.map((candidate) => ({
        key: `model:${group.providerId}:${candidate}`,
        selected:
          group.providerId === controls?.providerId && candidate === controls?.model,
        intent: {
          type: "select_model" as const,
          providerId: group.providerId,
          model: candidate,
        },
        build: (row: HTMLButtonElement) => {
          const name = document.createElement("span");
          name.className = "gset-prov-model";
          name.textContent = candidate;
          const label = document.createElement("span");
          label.className = "gset-prov-group";
          label.textContent = group.label;
          row.append(name, label);
        },
      })),
    ),
  );
  // The design's section 1 also draws an add-provider action, per-provider API
  // key chips, and a Requests card (`request_timeout_secs`, `max_retries`,
  // `provider_plugin_dirs`). None are drawn here: credentials stay
  // GUI-CORE-026 territory, and the request knobs have no Core
  // command in `frontend-contract-v1`. Absence is honest; a dead control is not.
  if ((controls?.groups.length ?? 0) === 0) {
    const empty = document.createElement("p");
    empty.className = "gset-empty";
    empty.dataset.settingsNoModels = "true";
    empty.textContent = translate(locale, "d1.controls.noOptions", {});
    providerCard.append(empty);
  }

  const permissionCard = controlCard(
    "settings.permissions",
    "settings.permissions.detail",
    "permission",
    "d1.controls.permission",
    PERMISSION_LEVELS.map((level) => ({
      key: `permission:${level}`,
      selected: level === controls?.permissionLevel,
      intent: { type: "set_permission_level" as const, level },
      build: (row: HTMLButtonElement) => {
        const name = document.createElement("span");
        name.className = "gset-pm-name";
        name.textContent = permissionLabel(locale, level);
        // The Core CLI name, verbatim. An operator matching this panel against
        // a Core log needs the identifier, not a second reworded label.
        const cli = document.createElement("code");
        cli.className = "gset-pm-cli";
        cli.textContent = level;
        row.append(name, cli);
        const detailKey = PERMISSION_DETAIL_KEYS[level];
        if (detailKey) {
          const description = document.createElement("small");
          description.className = "gset-pm-detail";
          description.textContent = translate(locale, detailKey, {});
          row.append(description);
        }
      },
    })),
  );
  // The design's section 2 additionally draws a "Rules preview" box
  // (allow/ask/deny rule lines) and an editable additional-working-directories
  // field. Core publishes neither the rule table nor a scope command, so
  // neither is drawn and neither gets a fail-closed placeholder row: a drawn
  // element that shows nothing real would be the lie, an absent one is not.
  const cwdRow = document.createElement("div");
  cwdRow.className = "gset-row";
  cwdRow.dataset.settingsCwd = "true";
  const cwdLabels = document.createElement("div");
  cwdLabels.className = "gset-row-labels";
  const cwdTitle = document.createElement("div");
  cwdTitle.className = "gset-row-title";
  cwdTitle.textContent = translate(locale, "settings.workingDirectory", {});
  const cwdDetail = document.createElement("div");
  cwdDetail.className = "gset-row-detail";
  cwdDetail.textContent = translate(locale, "settings.workingDirectory.detail", {});
  cwdLabels.append(cwdTitle, cwdDetail);
  const cwdValue = document.createElement("code");
  cwdValue.className = "gset-value";
  cwdValue.textContent = controls?.cwd ?? translate(locale, "d1.permission.unavailable", {});
  cwdRow.append(cwdLabels, cwdValue);
  permissionCard.append(cwdRow);

  card("settings.language", [
    {
      field: "locale",
      titleKey: "settings.locale",
      detailKey: "settings.locale.detail",
      options: LOCALE_OPTIONS,
    },
  ]);
  card("settings.appearance", APPEARANCE_FIELDS);

  // Diagnostics are Core's message about what it had to correct. They are
  // rendered verbatim by code rather than reworded into a client guess.
  const diagnostics = [
    ...(model.outcome?.status === "confirmed" || model.outcome?.status === "rejected"
      ? model.outcome.diagnostics
      : []),
    ...model.state.resolved.diagnostics,
  ];
  if (diagnostics.length > 0) {
    const list = document.createElement("ul");
    list.className = "gset-diagnostics";
    list.dataset.settingsDiagnostics = "true";
    for (const diagnostic of diagnostics) {
      const item = document.createElement("li");
      item.className = "gset-diagnostic";
      item.dataset.settingsDiagnostic = diagnostic.code;
      item.textContent = diagnostic.rejectedValue
        ? `${diagnostic.code} · ${diagnostic.field ?? ""} ${diagnostic.rejectedValue}`.trim()
        : diagnostic.code;
      list.append(item);
    }
    panel.append(list);
  }

  if (model.outcome?.status === "rejected") {
    const alert = document.createElement("p");
    alert.className = "gset-alert";
    alert.dataset.settingsAlert = "true";
    alert.setAttribute("role", "alert");
    alert.textContent = model.outcome.reason;
    panel.append(alert);
  } else if (model.outcome?.status === "confirmed") {
    const status = document.createElement("p");
    status.className = "gset-status";
    status.dataset.settingsStatus = model.outcome.persisted ? "saved" : "restored";
    status.setAttribute("role", "status");
    status.textContent = translate(
      locale,
      model.outcome.persisted ? "settings.saved" : "settings.restored",
      {},
    );
    panel.append(status);
  }

  const actions = document.createElement("div");
  actions.className = "gset-actions";
  const restore = document.createElement("button");
  restore.type = "button";
  restore.className = "gset-restore";
  restore.dataset.settingsRestore = "true";
  restore.textContent = translate(locale, "settings.restore", {});
  restore.disabled = disabled;
  restore.addEventListener("click", () => handlers.onRestore());
  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.className = "gset-cancel";
  cancel.dataset.settingsCancel = "true";
  cancel.textContent = translate(locale, "settings.cancel", {});
  cancel.disabled = model.saving || !model.state.dirty;
  cancel.addEventListener("click", () => handlers.onCancel());
  const save = document.createElement("button");
  save.type = "button";
  save.className = "gset-save";
  save.dataset.settingsSave = "true";
  save.textContent = translate(
    locale,
    model.saving ? "settings.saving" : "settings.save",
    {},
  );
  // Nothing to persist until the operator actually changed an axis.
  save.disabled = disabled || !model.state.dirty;
  save.addEventListener("click", () => handlers.onSave());
  actions.append(restore, cancel, save);
  panel.append(actions);

  const anchorRect = anchor.getBoundingClientRect();
  panel.style.setProperty("--gset-anchor-inline", `${anchorRect.right}px`);
  panel.style.setProperty("--gset-anchor-block", `${anchorRect.bottom}px`);
  anchor.setAttribute("aria-expanded", "true");
  // Portalled out of the rail so the overlay is not clipped by the
  // auto-hiding sidebar, matching the New Lane popover.
  (anchor.closest(".d1-frame")?.parentElement ?? document.body).append(panel);

  const focusAt = (group: HTMLElement, index: number): void => {
    const options = Array.from(
      group.querySelectorAll<HTMLButtonElement>("[data-settings-option]"),
    ).filter((option) => !option.disabled);
    if (options.length === 0) return;
    const target = options[((index % options.length) + options.length) % options.length]!;
    options.forEach((option) => (option.tabIndex = -1));
    target.tabIndex = 0;
    target.focus();
  };

  panel.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      controller.close();
      return;
    }
    const target = event.target as HTMLElement | null;
    if (!(target instanceof HTMLButtonElement) || !target.dataset.settingsOption) return;
    const group = optionGroups.find((candidate) => candidate.contains(target));
    if (!group) return;
    const options = Array.from(
      group.querySelectorAll<HTMLButtonElement>("[data-settings-option]"),
    ).filter((option) => !option.disabled);
    const current = options.indexOf(target);
    if (event.key === "ArrowRight" || event.key === "ArrowDown") {
      event.preventDefault();
      focusAt(group, current + 1);
    } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
      event.preventDefault();
      focusAt(group, current - 1);
    } else if (event.key === "Home") {
      event.preventDefault();
      focusAt(group, 0);
    } else if (event.key === "End") {
      event.preventDefault();
      focusAt(group, options.length - 1);
    }
  });

  // The cockpit rebuilds the rail on every Core refresh, so the gear that is
  // mounted now may not be the node this panel was anchored to. Both the
  // outside-click guard and the focus hand-back resolve the live gear instead
  // of a detached one — otherwise the gear could no longer close its own
  // panel, and Escape would drop focus to the document body.
  const liveAnchor = (): HTMLButtonElement =>
    anchor.isConnected
      ? anchor
      : (document.querySelector<HTMLButtonElement>("[data-settings-toggle]") ?? anchor);

  let closed = false;
  const outside = (event: MouseEvent): void => {
    const target = event.target;
    if (panel.contains(target as Node)) return;
    if (target instanceof Element && target.closest("[data-settings-toggle]")) return;
    controller.close();
  };
  const controller: SettingsPanelController = {
    root: panel,
    close: () => {
      if (closed) return;
      closed = true;
      document.removeEventListener("mousedown", outside);
      panel.remove();
      const gear = liveAnchor();
      gear.setAttribute("aria-expanded", "false");
      gear.focus();
      handlers.onClose();
    },
  };
  document.addEventListener("mousedown", outside);
  return controller;
}
