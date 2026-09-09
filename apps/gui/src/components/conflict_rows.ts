import { translate, type Locale } from "../i18n/catalog";
import type {
  ConflictAuditScope,
  ConflictBaselineProjection,
  ConflictContentProjection,
  ConflictFileProjection,
  ConflictHunkProjection,
} from "../models/conflict";
import { CONFLICT_CONTENT_CAPABILITY } from "../models/conflict";
import "./conflict_rows.css";

/**
 * The one structured-conflict renderer in the GUI
 * (`runtime.conflict_content`, GUI-CORE-015).
 *
 * It draws what Core published and nothing else: **two sides plus the patch
 * preimage, never a three-way merge.** OURS is a read-only read of the Lane's
 * current file at the hunk's declared old range, THEIRS is the incoming
 * patch's new side, and BASE is the hunk's own preimage — the text the patch
 * expected to find. Core computes no merge base and resolves nothing, so this
 * component renders no merged text and offers no resolve control. That is the
 * whole reason D12 has no manual-merge escape hatch: a hunk resolved in the
 * gate would be code that never passed the Lane's own gates.
 *
 * Rows reuse the registered `.diffbody > .dl` family (`.ln` gutter, `.tx`
 * text) that `gui-kit.css` carries for DiffReview, so a conflict line and a
 * diff line are the same object on screen. Nothing here parses text: line
 * numbers are counted forward from Core's own `oursStart` / `theirsStart`, and
 * a hunk whose start Core published as `0` — the patch declared none — gets no
 * number rather than a fabricated `1`.
 */

/// Rows Core did not publish, each with its own sentence. None of them means
/// "no conflict here", and none may render as an empty body.
type ConflictNoteKind = "omitted" | "no-hunks" | "empty-side";

function note(host: HTMLElement, kind: ConflictNoteKind, text: string): HTMLElement {
  const row = document.createElement("p");
  row.className = "dl note";
  row.dataset.conflictNote = kind;
  row.textContent = text;
  host.append(row);
  return row;
}

/**
 * One side's lines as registered `.dl` rows.
 *
 * `start` is Core's own 1-based first line; `0` means the patch declared none,
 * and then no number is drawn at all. `side` only tints the rows — it never
 * claims a row was added or removed, because a conflict side is neither.
 */
function renderSide(
  host: HTMLElement,
  side: "ours" | "theirs" | "base",
  start: number,
  lines: string[],
  locale: Locale,
): HTMLElement {
  const body = document.createElement("div");
  body.className = `diffbody cc-body cc-${side}`;
  body.dataset.conflictSide = side;
  if (lines.length === 0) {
    note(body, "empty-side", translate(locale, "d12.conflict.emptySide", {}));
    host.append(body);
    return body;
  }
  lines.forEach((line, index) => {
    const row = document.createElement("div");
    row.className = `dl cc-line cc-${side}`;
    const number = document.createElement("span");
    number.className = "ln";
    number.textContent = start > 0 ? String(start + index) : "";
    const text = document.createElement("span");
    text.className = "tx";
    // Core sends the line as it stands in the file, newline included; the row
    // is already `white-space: pre`, so the trailing break would draw a blank
    // line under every row.
    text.textContent = line.replace(/\r?\n$/, "");
    row.append(number, text);
    body.append(row);
  });
  host.append(body);
  return body;
}

/// The five reasons Core names today. `ConflictHunkReason` is open-ended, so
/// anything else keeps Core's own tag rather than borrowing one of these.
const NAMED_REASONS = new Set([
  "context_mismatch",
  "already_applied",
  "file_missing",
  "file_deleted",
  "binary",
]);

function renderReason(host: HTMLElement, reason: string, locale: Locale): void {
  const row = document.createElement("p");
  row.className = "cc-reason";
  const chip = document.createElement("span");
  chip.className = "cc-rchip";
  chip.dataset.conflictReason = reason;
  const remedy = document.createElement("span");
  remedy.className = "cc-remedy";
  remedy.dataset.conflictRemedy = reason;
  if (NAMED_REASONS.has(reason)) {
    // The keys are the exact Core discriminants, so a new reason fails the
    // catalog parity test rather than silently rendering a known sentence.
    chip.textContent = translate(
      locale,
      `d12.conflict.reason.${reason}` as "d12.conflict.reason.context_mismatch",
      {},
    );
    remedy.textContent = translate(
      locale,
      `d12.conflict.reason.${reason}.remedy` as "d12.conflict.reason.context_mismatch.remedy",
      {},
    );
  } else {
    chip.textContent = translate(locale, "d12.conflict.reason.unnamed", { reason });
    remedy.textContent = translate(locale, "d12.conflict.reason.unnamed.remedy", {});
  }
  row.append(chip, remedy);
  host.append(row);
}

function renderHunk(
  host: HTMLElement,
  hunk: ConflictHunkProjection,
  index: number,
  locale: Locale,
): void {
  const block = document.createElement("div");
  block.className = "cc-hunk";
  block.dataset.conflictHunk = String(index);
  renderReason(block, hunk.reason, locale);

  const sides = document.createElement("div");
  sides.className = "cc-sides";
  for (const side of ["ours", "theirs"] as const) {
    const column = document.createElement("div");
    column.className = "cc-col";
    const heading = document.createElement("p");
    heading.className = "cc-shead";
    const start = side === "ours" ? hunk.oursStart : hunk.theirsStart;
    heading.textContent =
      start > 0
        ? translate(locale, side === "ours" ? "d12.conflict.ours" : "d12.conflict.theirs", {
            line: String(start),
          })
        : translate(
            locale,
            side === "ours" ? "d12.conflict.oursUnnumbered" : "d12.conflict.theirsUnnumbered",
            {},
          );
    column.append(heading);
    renderSide(column, side, start, side === "ours" ? hunk.ours : hunk.theirs, locale);
    sides.append(column);
  }
  block.append(sides);

  // The preimage is the third side, collapsed by default: it is what the patch
  // expected, not what either side holds. `null` and `[]` are different facts —
  // "no preimage at all" versus "it expected an empty region" — and get
  // different sentences rather than one shared empty strip.
  if (hunk.base === null) {
    const absent = document.createElement("p");
    absent.className = "dl note cc-baseline-note";
    absent.dataset.conflictBase = "absent";
    absent.textContent = translate(locale, "d12.conflict.base.absent", {});
    block.append(absent);
  } else if (hunk.base.length === 0) {
    const empty = document.createElement("p");
    empty.className = "dl note cc-baseline-note";
    empty.dataset.conflictBase = "empty";
    empty.textContent = translate(locale, "d12.conflict.base.empty", {});
    block.append(empty);
  } else {
    const strip = document.createElement("details");
    strip.className = "cc-base";
    strip.dataset.conflictBase = "present";
    const summary = document.createElement("summary");
    summary.textContent = translate(locale, "d12.conflict.base", {});
    strip.append(summary);
    // The preimage carries the hunk's old-side numbering, the same range OURS
    // was read at, because that is the region the patch expected to find.
    renderSide(strip, "base", hunk.oursStart, hunk.base, locale);
    block.append(strip);
  }

  host.append(block);
}

function renderFile(host: HTMLElement, file: ConflictFileProjection, locale: Locale): void {
  const block = document.createElement("div");
  block.className = "cc-file";
  block.dataset.conflictFile = file.path;
  const heading = document.createElement("p");
  heading.className = "cc-fhead";
  heading.title = file.path;
  heading.textContent = file.path;
  block.append(heading);

  if (file.omitted) {
    // The entry survives on purpose: a reviewer must be able to tell "not
    // shown" from "this file was fine".
    note(block, "omitted", translate(locale, "d12.conflict.omitted", {}));
  } else if (file.hunks.length === 0) {
    // Core listed the file with neither hunks nor the flag. That is not "no
    // conflict" either — it is a file this build cannot account for.
    note(block, "no-hunks", translate(locale, "d12.conflict.noHunks", {}));
  }
  file.hunks.forEach((hunk, index) => renderHunk(block, hunk, index, locale));
  host.append(block);
}

function renderBaseline(
  host: HTMLElement,
  baseline: ConflictBaselineProjection,
  locale: Locale,
  onOpenEvidence?: (scope: ConflictAuditScope) => void,
): void {
  const row = document.createElement("p");
  row.className = "cc-baseline";
  row.dataset.conflictBaseline = baseline.kind;
  const label = document.createElement("span");
  label.className = "cc-blabel";
  label.textContent = translate(locale, "d12.conflict.baseline", {});
  const value = document.createElement("span");
  value.className = "cc-bvalue";
  if (baseline.kind === "revision") {
    value.textContent = translate(locale, "d12.conflict.baseline.revision", {
      sha: baseline.shortSha ?? baseline.sha ?? "",
    });
    if (baseline.sha) value.title = baseline.sha;
  } else if (baseline.kind === "evidence") {
    value.textContent = translate(locale, "d12.conflict.baseline.evidence", {});
  } else if (baseline.kind === "unknown") {
    // A real Core answer: it held no baseline it could name. Never `HEAD`.
    value.textContent = translate(locale, "d12.conflict.baseline.unknown", {});
  } else {
    // `ConflictBaseline` is open-ended; an unmodelled kind reaches the screen
    // as itself rather than collapsing into `unknown`.
    value.textContent = translate(locale, "d12.conflict.baseline.unnamed", {
      kind: baseline.kind,
    });
  }
  row.append(label, value);

  for (const binding of baseline.bindings) {
    // The gate's baseline is its evidence, so each binding is a cross-link.
    // `AuditQuery` filters by object, so the chip carries the audit object
    // Core linked — the same route D12's revert rows already take.
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = "cc-evchip";
    chip.dataset.conflictEvidence = binding.evidenceId;
    chip.textContent = `${binding.evidenceId} · ${binding.shortHash}`;
    chip.title = `${binding.evidenceId} · ${binding.sourceHash}`;
    chip.setAttribute(
      "aria-label",
      translate(locale, "d12.conflict.openEvidence", { evidence: binding.evidenceId }),
    );
    chip.disabled = onOpenEvidence === undefined;
    if (onOpenEvidence) {
      chip.addEventListener("click", () => onOpenEvidence(binding.auditScope));
    }
    row.append(chip);
  }
  host.append(row);
}

/**
 * Renders one `ConflictContent` into `host`.
 *
 * The "not a merge result" statement is drawn with every pane rather than once
 * per screen, because the pane is what an operator reads and the claim it must
 * not make is exactly the one the layout invites.
 */
export function renderConflictContent(
  host: HTMLElement,
  content: ConflictContentProjection,
  locale: Locale,
  onOpenEvidence?: (scope: ConflictAuditScope) => void,
): HTMLElement {
  const pane = document.createElement("div");
  pane.className = "cc";
  pane.dataset.conflictContent = "true";

  const claim = document.createElement("p");
  claim.className = "cc-notmerge";
  claim.dataset.conflictNotMerge = "true";
  claim.textContent = translate(locale, "d12.conflict.notMerge", {});
  pane.append(claim);

  renderBaseline(pane, content.baseline, locale, onOpenEvidence);

  if (content.truncated) {
    const banner = document.createElement("p");
    banner.className = "cc-trunc";
    banner.dataset.conflictTruncated = "true";
    banner.setAttribute("role", "status");
    banner.textContent = translate(locale, "d12.conflict.truncated", {});
    pane.append(banner);
  }

  if (content.files.length === 0) {
    // Core published content naming no file. That is not "no conflict"; it is
    // content this build cannot account for.
    note(pane, "no-hunks", translate(locale, "d12.conflict.noFiles", {}));
  }
  for (const file of content.files) {
    renderFile(pane, file, locale);
  }

  host.append(pane);
  return pane;
}

/**
 * The two ways conflict lines can be absent, which are different facts.
 *
 * `capabilityAvailable: false` is a Core that publishes no structured content
 * at all — the reason text is everything there is. With the capability
 * present, an absent content means Core published none for *this* record: an
 * operator `BounceMergeConflict` is a human judgement with no failed apply
 * behind it and carries none by contract. Neither ever means the conflict was
 * empty.
 */
export function renderConflictUnavailable(
  host: HTMLElement,
  locale: Locale,
  capabilityAvailable: boolean,
): HTMLElement {
  const row = document.createElement("p");
  row.className = "cc-absent";
  if (capabilityAvailable) {
    row.dataset.conflictAbsent = "record";
    row.textContent = translate(locale, "d12.conflict.absent.record", {});
  } else {
    row.dataset.conflictAbsent = "capability";
    row.textContent = translate(locale, "d12.conflict.absent.capability", {
      capability: CONFLICT_CONTENT_CAPABILITY,
    });
  }
  host.append(row);
  return row;
}
