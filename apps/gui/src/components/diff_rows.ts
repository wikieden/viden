import { translate, type Locale } from "../i18n/catalog";
import type { DiffFileProjection, DiffHunkProjection } from "../models/diff_review";
import "./diff_rows.css";

/**
 * The one hunk-row renderer in the GUI.
 *
 * DiffReview's diff pane, the D1 permission dock, and the D2 decision detail
 * all draw the same rows from the same Core facts, so they share this
 * component rather than each growing a private copy that could drift into a
 * different idea of what a hunk is. The registered `.diffbody > .dl` markup is
 * the DiffReview family's; the approval surfaces reuse it as a component,
 * which is exactly what the design's D-COMP registration means.
 *
 * Nothing here parses text. Line numbers come from `DiffLine.old_line` /
 * `DiffLine.new_line`, which Core read out of Git's `@@` header, and a row
 * renders the number for the side it exists on: a removed row has no new-file
 * position and an added row has no old-file position. Both numbers stay
 * readable through `data-old-line` / `data-new-line` and the row title, which
 * is how the unified body carries per-side numbering without adding a second
 * gutter the registered markup does not draw.
 */

/// Rows Core did not publish, each with its own sentence.
///
/// The three are genuinely different facts and the operator has to be able to
/// tell them apart: a file whose rows the byte bound dropped, a file Git
/// reported as binary, and a file Core produced no diff for at all. None of
/// them means "unchanged", and none of them may render as an empty body.
type DiffNoteKind = "omitted" | "binary" | "no-diff";

function note(host: HTMLElement, kind: DiffNoteKind, text: string): void {
  const row = document.createElement("p");
  row.className = "dl note";
  row.dataset.diffNote = kind;
  row.textContent = text;
  host.append(row);
}

/// The literal `@@` header, rebuilt from Core's own starts and lengths.
///
/// Rebuilt rather than translated: it is Git's syntax, the same string a
/// reviewer sees in a terminal, and the section heading after it is the
/// author's own code.
function hunkHeaderText(hunk: DiffHunkProjection): string {
  const header = `@@ -${hunk.oldStart},${hunk.oldLines} +${hunk.newStart},${hunk.newLines} @@`;
  return hunk.header ? `${header} ${hunk.header}` : header;
}

const ROW_CLASS: Record<string, string> = {
  context: "ctx",
  added: "add",
  removed: "del",
};

/**
 * Renders one file's rows into `host` as the registered `.diffbody`.
 *
 * `file` being `null` is Core having produced no diff for the path, which gets
 * its own sentence rather than an empty body.
 */
export function renderDiffBody(
  host: HTMLElement,
  file: DiffFileProjection | null,
  locale: Locale,
): HTMLElement {
  const body = document.createElement("div");
  body.className = "diffbody";
  body.dataset.diffBody = "true";
  if (!file) {
    note(body, "no-diff", translate(locale, "d1.review.noDiff", {}));
    host.append(body);
    return body;
  }
  if (file.binary) {
    note(body, "binary", translate(locale, "d1.review.binary", {}));
  } else if (file.omitted) {
    // The counts stay real, which is the whole point of the flag: a reviewer
    // must always be able to tell "not shown" from "unchanged".
    note(
      body,
      "omitted",
      translate(locale, "d1.review.omitted", {
        additions: String(file.additions),
        deletions: String(file.deletions),
      }),
    );
  } else if (file.hunks.length === 0) {
    // Core published a diff with no rows and neither flag set. That is not
    // "unchanged" either — it is a diff this build cannot account for.
    note(body, "no-diff", translate(locale, "d1.review.noDiff", {}));
  }

  for (const hunk of file.hunks) {
    const header = document.createElement("div");
    header.className = "dl hunk";
    const headerNumber = document.createElement("span");
    headerNumber.className = "ln";
    headerNumber.ariaHidden = "true";
    const headerText = document.createElement("span");
    headerText.className = "tx";
    headerText.textContent = hunkHeaderText(hunk);
    header.append(headerNumber, headerText);
    body.append(header);

    for (const line of hunk.lines) {
      const row = document.createElement("div");
      const variant = ROW_CLASS[line.kind];
      // An unnamed row kind keeps its own class rather than borrowing `ctx`:
      // drawing an unmodeled row as unchanged code would be a lie about Core.
      row.className = `dl ${variant ?? "unknown"}`;
      row.dataset.diffLineKind = line.kind;
      if (line.oldLine !== null) row.dataset.oldLine = String(line.oldLine);
      if (line.newLine !== null) row.dataset.newLine = String(line.newLine);
      const number = document.createElement("span");
      number.className = "ln";
      // The number for the side this row exists on. `0` is never printed,
      // because Core publishes absence rather than zero for the missing side.
      number.textContent =
        line.newLine !== null
          ? String(line.newLine)
          : line.oldLine !== null
            ? String(line.oldLine)
            : "";
      row.title = translate(locale, "d1.review.lineNumbers", {
        old: line.oldLine === null ? "—" : String(line.oldLine),
        new: line.newLine === null ? "—" : String(line.newLine),
      });
      const text = document.createElement("span");
      text.className = line.kind === "context" ? "tx ctx" : "tx";
      // The marker belongs to the row kind, so it is drawn rather than parsed
      // back out of the content Core stripped it from.
      const marker = line.kind === "added" ? "+" : line.kind === "removed" ? "-" : " ";
      text.textContent = `${marker}${line.content}`;
      row.append(number, text);
      body.append(row);
    }
  }
  host.append(body);
  return body;
}

/**
 * Renders a whole `DiffDocument`'s files, each under its own path heading.
 *
 * This is the approval surfaces' entry point: an `edit_file` context carries
 * one file and a `MergeAgentPatch` context carries several, and both must be
 * readable in the same dock.
 */
export function renderDiffFiles(
  host: HTMLElement,
  files: DiffFileProjection[],
  locale: Locale,
): void {
  for (const file of files) {
    const heading = document.createElement("p");
    heading.className = "diff-file-head";
    heading.dataset.diffFile = file.path;
    heading.title = file.path;
    heading.textContent = file.oldPath
      ? translate(locale, "d1.review.renamed", { from: file.oldPath, to: file.path })
      : file.path;
    host.append(heading);
    renderDiffBody(host, file, locale);
  }
}
