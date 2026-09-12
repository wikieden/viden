/**
 * Ordered owner-scoped transcript rows, mirroring the host's
 * `TranscriptRowsProjection` field for field (`runtime.transcript_rows`, C8,
 * closes GUI-CORE-009).
 *
 * Four facts stay distinct here, because each collapses into a lie if the view
 * has to guess it:
 *
 * | fact | meaning |
 * | --- | --- |
 * | `capabilityAvailable: false` | this Core publishes no ordered rows at all |
 * | `loaded: false` | the read is out, or has not been made |
 * | `rows: []` with `loaded: true` | this owner's conversation really is empty |
 * | `outcome.state === "rejected"` | Core refused, in its own words |
 *
 * The cursor is Core's. `older` travels back verbatim as the next read's
 * `before`; the client never parses, constructs, or compares one, and
 * `complete` is stated rather than left to the absence of a control.
 */

/** The frontend-contract-v1 capability that carries ordered transcript rows. */
export const TRANSCRIPT_ROWS_CAPABILITY = "runtime.transcript_rows";

export interface TranscriptRowProjection {
  id: string;
  /**
   * `user`, `assistant`, `tool_call`, `tool_result`, `check_run`,
   * `permission`, or `unknown` for a content shape this build cannot draw.
   */
  kind: string;
  laneId: string | null;
  /** Core's ordering position. An ordering key, never a count. */
  sequence: number;
  timestamp: number | null;
  text: string | null;
  /** Core's 8 KiB row bound cut `text`. */
  truncated: boolean;
  /** The canonical evidence row holding this body or this result's work. */
  evidenceId: string | null;
  toolCallId: string | null;
  toolName: string | null;
  inputPreview: string | null;
  success: boolean | null;
  summary: string | null;
  checkId: string | null;
  label: string | null;
  command: string | null;
  status: string | null;
  failingLocation: string | null;
  requestId: string | null;
  /** `null` is Core saying it does not know, never "allow once". */
  decision: string | null;
  auditId: string | null;
}

export interface TranscriptRowsProjection {
  outcome: { state: string; reason: string | null };
  pendingCommandId: string | null;
  capabilityAvailable: boolean;
  loaded: boolean;
  /** Oldest first, newest last: the order a transcript renders in. */
  rows: TranscriptRowProjection[];
  older: string | null;
  complete: boolean;
  scopeLaneId: string | null;
}

/** The projection an unbound host renders: nothing read, nothing claimed. */
export const IDLE_TRANSCRIPT_ROWS: TranscriptRowsProjection = {
  outcome: { state: "idle", reason: null },
  pendingCommandId: null,
  capabilityAvailable: false,
  loaded: false,
  rows: [],
  older: null,
  complete: false,
  scopeLaneId: null,
};
