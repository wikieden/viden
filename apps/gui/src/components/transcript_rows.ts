import { translate, type Locale } from "../i18n/catalog";
import { appendTranscriptRows, type TranscriptAgent } from "./transcript";
import { checkRow, localizedStatus, toolBlock } from "./tool_row";
import type { TranscriptRowProjection } from "../models/transcript_rows";

/**
 * The D1 transcript's ordered rows (`runtime.transcript_rows`, C8, closes
 * GUI-CORE-009).
 *
 * Until C8 the cockpit had no ordered user/assistant history to render: the
 * view state carries a Lane's ACP conversation and an unscoped
 * `assistant_stream`, and the two `transcript_user` / `transcript_assistant`
 * unavailable rows were the honest rendering of that gap. This renderer draws
 * Core's own page instead, and it reuses the blocks the cockpit already has
 * rather than inventing a second vocabulary for the same facts:
 *
 * - a `user` or `assistant` row is the same `.d1-row` the live stream draws,
 *   through `appendTranscriptRows`, so one conversation has one row shape;
 * - a `tool_call`, `tool_result` or `check_run` row is G4's `.tool` block,
 *   header over body, the same one a live workspace change and check use;
 * - a `permission` row is the decision joined to its durable audit row, which
 *   is the only place a scoped allow's payload does *not* exist — Core says it
 *   does not know rather than reporting `allow_once` for it.
 *
 * Three honesty rules, each with its own rendering:
 *
 * 1. **`truncated` is stated.** Core's 8 KiB row bound cut the body; the row
 *    says so, and offers the canonical evidence when Core named one. Reading
 *    half an answer as the whole one is what the flag exists to prevent.
 * 2. **An unmodelled row still occupies a row.** `TranscriptRowContent` is
 *    `#[non_exhaustive]`; dropping a kind this build cannot draw would
 *    silently shorten a conversation.
 * 3. **Nothing is inferred from text.** Every chip and every number below is a
 *    field Core published.
 */

export interface TranscriptRowsOptions {
  locale: Locale;
  /** The agent that produced the assistant rows, when Core bound one. */
  agent?: TranscriptAgent;
  /** Opens EvidenceView on the canonical row a body or a result names. */
  onOpenEvidence?: (evidenceId: string) => void;
  /** Opens D14 scoped to the audit row a permission decision was read from. */
  onOpenAudit?: (requestId: string) => void;
}

/** Appends one Core page, oldest first, in the order Core returned it. */
export function appendOrderedTranscriptRows(
  root: HTMLElement,
  rows: readonly TranscriptRowProjection[],
  options: TranscriptRowsOptions,
): void {
  const { locale } = options;
  for (const row of rows) {
    switch (row.kind) {
      case "user":
      case "assistant": {
        appendTranscriptRows(
          root,
          [{ id: row.id, kind: row.kind, content: row.text ?? "" }],
          row.kind === "assistant" ? options.agent : undefined,
        );
        const article = root.lastElementChild as HTMLElement | null;
        if (!article) break;
        article.dataset.transcriptRowSource = "core";
        article.dataset.transcriptSequence = String(row.sequence);
        if (row.truncated) {
          const note = document.createElement("p");
          note.className = "d1-row-truncated";
          note.dataset.rowTruncated = "true";
          note.textContent = translate(locale, "d1.transcript.rowTruncated", {});
          article.append(note);
        }
        if (row.evidenceId) {
          // The full body lives in the canonical evidence row Core named. The
          // control is offered whether or not the bound cut this one: the
          // archive is where the verified bytes are either way.
          appendEvidenceLink(article, row.evidenceId, options);
        }
        break;
      }

      case "tool_call": {
        const { block } = toolBlock(locale, {
          name: row.toolName ?? translate(locale, "d1.transcript.toolUnnamed", {}),
          detail: row.inputPreview ?? "",
          gate: null,
          stats: null,
          // A call has no body of its own — the result is its own row — so
          // there is nothing to disclose.
          collapsible: false,
        });
        block.dataset.transcriptRow = "tool_call";
        block.dataset.transcriptRowId = row.id;
        if (row.toolCallId) block.dataset.toolCallId = row.toolCallId;
        root.append(block);
        break;
      }

      case "tool_result": {
        const succeeded = row.success === true;
        const { block, body } = toolBlock(locale, {
          name: translate(locale, "d1.transcript.toolResult", {}),
          detail: row.toolCallId ?? "",
          gate: {
            text: localizedStatus(locale, succeeded ? "passed" : "failed"),
            tone: succeeded ? "passed" : "failed",
          },
          stats: null,
          collapsible: false,
        });
        block.dataset.transcriptRow = "tool_result";
        block.dataset.transcriptRowId = row.id;
        block.dataset.toolResult = succeeded ? "success" : "failure";
        // Core's bounded summary, and its absence is as absent as an empty
        // one: a blank line would read as "nothing to report".
        const summary = checkRow(
          body,
          "summary",
          translate(locale, "d1.checkRun.result", {}),
          row.summary ? row.summary : translate(locale, "d1.checkRun.emptyResult", {}),
        );
        if (!row.summary) {
          summary.lastElementChild!.setAttribute("data-typed-empty", "tool-result-summary");
        }
        if (row.evidenceId) appendEvidenceLink(body, row.evidenceId, options);
        root.append(block);
        break;
      }

      case "check_run": {
        const failed = row.status === "failed";
        const { block, body } = toolBlock(locale, {
          name: row.label ?? translate(locale, "d1.checkRun", {}),
          detail: row.command ?? translate(locale, "d1.checkRun.emptyCommand", {}),
          gate: {
            text: localizedStatus(locale, row.status ?? "queued"),
            tone: failed ? "failed" : row.status === "passed" ? "passed" : "neutral",
          },
          stats: null,
          // The same rule the live check block follows: its body is three
          // lines and the failing one is why the block is on screen.
          collapsible: false,
        });
        block.dataset.transcriptRow = "check_run";
        block.dataset.transcriptRowId = row.id;
        if (row.checkId) block.dataset.checkRun = row.checkId;
        checkRow(
          body,
          "status",
          translate(locale, "d1.checkRun.status", {}),
          localizedStatus(locale, row.status ?? "queued"),
        );
        if (row.failingLocation) {
          checkRow(
            body,
            "failing",
            translate(locale, "d1.checkRun.failing", {}),
            row.failingLocation,
          );
        }
        const result = checkRow(
          body,
          "result",
          translate(locale, "d1.checkRun.result", {}),
          row.summary ? row.summary : translate(locale, "d1.checkRun.emptyResult", {}),
        );
        if (!row.summary) {
          result.lastElementChild!.setAttribute("data-typed-empty", "check-run-result");
        }
        root.append(block);
        break;
      }

      case "permission": {
        const article = document.createElement("article");
        article.className = "d1-row d1-transcript-permission";
        article.dataset.transcriptRow = "permission";
        article.dataset.transcriptRowId = row.id;
        const kind = document.createElement("span");
        kind.className = "d1-row-kind";
        kind.textContent = translate(locale, "d1.transcript.permission", {});
        const detail = document.createElement("pre");
        // A scoped allow's payload is not in the durable audit row, so Core
        // publishes no decision for it. Saying "Core recorded the scope only"
        // is the honest rendering; printing `allow_once` would misreport what
        // the operator chose.
        detail.textContent = [
          row.requestId ?? "",
          row.decision
            ? translate(locale, "d1.transcript.permissionDecision", { decision: row.decision })
            : translate(locale, "d1.transcript.permissionScopeOnly", {}),
        ]
          .filter((part) => part.length > 0)
          .join(" · ");
        article.append(kind, detail);
        if (row.requestId && options.onOpenAudit) {
          const audit = document.createElement("button");
          audit.type = "button";
          audit.className = "d1-action";
          audit.dataset.transcriptAudit = row.requestId;
          audit.textContent = translate(locale, "d1.transcript.openAudit", {});
          const requestId = row.requestId;
          audit.addEventListener("click", () => options.onOpenAudit?.(requestId));
          article.append(audit);
        }
        root.append(article);
        break;
      }

      default: {
        // Never dropped: a conversation that silently loses a row is worse
        // than one that says a row exists it cannot draw.
        const unknown = document.createElement("p");
        unknown.className = "d1-empty";
        unknown.dataset.transcriptRow = "unknown";
        unknown.dataset.transcriptRowId = row.id;
        unknown.textContent = translate(locale, "d1.transcript.rowUnknown", { kind: row.kind });
        root.append(unknown);
      }
    }
  }
}

/// The canonical evidence a row names, offered as a control when the caller
/// can host EvidenceView and as a stated fact when it cannot.
function appendEvidenceLink(
  host: HTMLElement,
  evidenceId: string,
  options: TranscriptRowsOptions,
): void {
  if (!options.onOpenEvidence) {
    const note = document.createElement("p");
    note.className = "d1-empty";
    note.dataset.rowEvidence = evidenceId;
    note.textContent = translate(options.locale, "d1.transcript.evidenceNamed", {
      evidence: evidenceId,
    });
    host.append(note);
    return;
  }
  const open = document.createElement("button");
  open.type = "button";
  open.className = "d1-action";
  open.dataset.rowEvidence = evidenceId;
  open.textContent = translate(options.locale, "d1.transcript.openEvidence", {});
  open.addEventListener("click", () => options.onOpenEvidence?.(evidenceId));
  host.append(open);
}
