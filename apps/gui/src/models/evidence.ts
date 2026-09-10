/**
 * The EvidenceView read side, mirroring the host's evidence projections field
 * for field (`runtime.evidence_reads`, GUI-CORE-025).
 *
 * Core is the only producer. The client never orders the archive, never
 * decides where a page ends, never opens the ContextStore, and never reads a
 * row's content out of its `summary`.
 *
 * Four list states stay distinct rather than collapsing into one empty list,
 * because each needs a different sentence on screen:
 *
 * | fact | meaning |
 * | --- | --- |
 * | `capabilityAvailable: false` | Core publishes no evidence archive at all |
 * | `outcome.state === "pending"` | the read is out, no page yet |
 * | `outcome.state === "rejected"` | Core refused, with its own words |
 * | `loaded && rows.length === 0` | Core answered: no evidence in this scope |
 */

/** The frontend-contract-v1 capability that carries the evidence archive. */
export const EVIDENCE_READS_CAPABILITY = "runtime.evidence_reads";

/**
 * The evidence kinds the contract calls first-class checklist groups.
 *
 * The filter bar shows these plus every other kind Core actually returned, in
 * that order. A kind outside this list is *grouped last, never hidden*: the
 * contract says clients may display other runtime-provided kinds, and a chip
 * bar that dropped one would hide rows the archive holds.
 */
export const FIRST_CLASS_EVIDENCE_KINDS = [
  "patch",
  "test_result",
  "review",
  "doc_update",
  "release_artifact",
] as const;

/** One top-level `metadata` key, rendered as a fact and never interpreted. */
export interface EvidenceMetadataProjection {
  key: string;
  value: string;
}

/**
 * The canonical ContextStore reference a row names.
 *
 * Null is exactly the row `ReadEvidenceContent` answers with
 * `Unavailable { summary_only }`.
 */
export interface EvidenceCanonicalProjection {
  itemId: string;
  bundleId: string;
  /** The hash Core verifies served bytes against. */
  sourceHash: string;
  producerIdentity: string;
  producerRole: string;
  producerTaskId: string;
}

export interface EvidenceRowProjection {
  id: string;
  kind: string;
  /** Display text. Nothing infers content, outcome, or verification from it. */
  summary: string;
  path: string | null;
  source: string | null;
  /**
   * Seconds. Null is a real position in Core's order — the row Core never
   * dated, which sorts *first* — and is not the same as `0`.
   */
  timestamp: number | null;
  /** Null means Core did not attribute the row; never filled from `source`. */
  ownerLaneId: string | null;
  ownerTaskId: string | null;
  canonical: EvidenceCanonicalProjection | null;
  metadata: EvidenceMetadataProjection[];
}

export interface EvidenceArchiveProjection {
  outcome: { state: string; reason: string | null };
  /** Every page confirmed so far, in Core's order. Never re-sorted. */
  rows: EvidenceRowProjection[];
  /** Core's opaque cursor, carried verbatim. Never parsed or constructed. */
  nextAfter: string | null;
  /** No newer entry matches the query — for the *filtered* archive. */
  complete: boolean;
  /** Whether a page has actually arrived. */
  loaded: boolean;
  pendingCommandId: string | null;
  capabilityAvailable: boolean;
  /**
   * Core recorded evidence after the loaded pages were read.
   *
   * A banner and a Refresh, never an auto-reload: a paged list that reloaded
   * under the operator would move rows they were reading.
   */
  stale: boolean;
  scopeLaneId: string | null;
  kinds: string[];
}

export interface EvidenceContentProjection {
  outcome: { state: string; reason: string | null };
  evidenceId: string | null;
  /** `absent`, `text`, `diff`, `unavailable`, or `unknown`. */
  kind: string;
  text: string | null;
  truncated: boolean;
  sha256: string | null;
  document: import("./diff_review").DiffDocumentProjection | null;
  /**
   * `summary_only`, `missing_canonical_bytes`, `hash_mismatch`, `binary`, or
   * `unknown` for a reason this build cannot name. Clients switch on this
   * rather than parsing a message.
   */
  reason: string | null;
  pendingCommandId: string | null;
}

/** Nothing read yet. Distinct from every answered state. */
export const ABSENT_EVIDENCE_CONTENT: EvidenceContentProjection = {
  outcome: { state: "idle", reason: null },
  evidenceId: null,
  kind: "absent",
  text: null,
  truncated: false,
  sha256: null,
  document: null,
  reason: null,
  pendingCommandId: null,
};

/** The list before any read has answered. */
export const PENDING_EVIDENCE_ARCHIVE: EvidenceArchiveProjection = {
  outcome: { state: "pending", reason: null },
  rows: [],
  nextAfter: null,
  complete: false,
  loaded: false,
  pendingCommandId: null,
  capabilityAvailable: true,
  stale: false,
  scopeLaneId: null,
  kinds: [],
};

/**
 * The chip vocabulary for one loaded archive: the first-class kinds followed
 * by every other kind Core actually returned, deduplicated and in that order.
 *
 * Built from the rows on screen rather than from a fixed list, so a kind this
 * build has never heard of still gets a chip. It is grouped after the
 * first-class set, never dropped.
 */
export function evidenceKindChips(rows: EvidenceRowProjection[]): string[] {
  const seen = new Set(rows.map((row) => row.kind));
  const extra = [...seen]
    .filter((kind) => !FIRST_CLASS_EVIDENCE_KINDS.includes(kind as never))
    .sort();
  return [...FIRST_CLASS_EVIDENCE_KINDS, ...extra];
}

/**
 * Groups rows into local days, preserving Core's order inside and between
 * groups.
 *
 * Undated rows form one group and it comes **first**, which is where Core's
 * ordering already puts them: undated is the oldest thing Core can honestly
 * say about a row. Grouping is presentation over Core's order, never a second
 * ordering rule — nothing here sorts.
 */
export interface EvidenceDayGroup {
  /** `null` for the undated group. */
  day: string | null;
  rows: EvidenceRowProjection[];
}

export function groupEvidenceByDay(rows: EvidenceRowProjection[]): EvidenceDayGroup[] {
  const groups: EvidenceDayGroup[] = [];
  for (const row of rows) {
    const day = row.timestamp === null ? null : localDayKey(row.timestamp);
    const last = groups.at(-1);
    if (last && last.day === day) last.rows.push(row);
    else groups.push({ day, rows: [row] });
  }
  return groups;
}

/** `YYYY-MM-DD` in the viewer's own zone, which is the day they read it in. */
export function localDayKey(timestampSeconds: number): string {
  const date = new Date(timestampSeconds * 1000);
  const month = `${date.getMonth() + 1}`.padStart(2, "0");
  const day = `${date.getDate()}`.padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

/** `HH:MM` in the viewer's own zone. */
export function localTimeLabel(timestampSeconds: number): string {
  const date = new Date(timestampSeconds * 1000);
  const hours = `${date.getHours()}`.padStart(2, "0");
  const minutes = `${date.getMinutes()}`.padStart(2, "0");
  return `${hours}:${minutes}`;
}

/**
 * Whether a row matches the operator's search box.
 *
 * Substring, case-insensitive, over the fields already on screen. It is
 * deliberately *not* a query: Core exposes no evidence search, so this filters
 * the rows this client has loaded and nothing else. The box says so.
 */
export function evidenceRowMatches(row: EvidenceRowProjection, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (!needle) return true;
  return [row.summary, row.kind, row.id, row.path ?? "", row.source ?? ""]
    .join("\n")
    .toLowerCase()
    .includes(needle);
}
