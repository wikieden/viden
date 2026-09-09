/**
 * The DiffReview read side, mirroring the host's `WorkspaceDiffProjection`
 * field for field (`runtime.structured_diff`, GUI-CORE-012).
 *
 * Core is the only producer of diff rows. The client never parses diff text
 * into rows, never counts line numbers itself, and never re-sorts what Core
 * ordered: a row exists here only because a `WorkspaceDiffLoaded` page carried
 * it.
 *
 * Four states stay distinct rather than collapsing into one empty list, because
 * each needs a different sentence on screen:
 *
 * | fact | meaning |
 * | --- | --- |
 * | `capabilityAvailable: false` | Core publishes no structured diff at all |
 * | `outcome.state === "pending"` | the read is out, no page yet |
 * | `outcome.state === "rejected"` | Core refused, with its own words |
 * | `loaded && entries.length === 0` | Core answered: nothing has changed |
 */

/** The frontend-contract-v1 capability that carries structured diff rows. */
export const STRUCTURED_DIFF_CAPABILITY = "runtime.structured_diff";

export interface DiffLineProjection {
  /**
   * `context`, `added`, `removed`, or `unknown` for a row kind this build
   * cannot name. `unknown` is drawn as its own row rather than as context,
   * because `DiffLineKind` is open-ended and mislabelling an unmodeled row as
   * unchanged code would be worse than saying it is unnamed.
   */
  kind: string;
  /** The line text without its `+`/`-`/space marker, exactly as Core sent it. */
  content: string;
  /** Absent for an added line. Never `0`, which would read as a real line. */
  oldLine: number | null;
  /** Absent for a removed line. */
  newLine: number | null;
}

export interface DiffHunkProjection {
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  /** Git's own section heading, or null when the `@@` header carried none. */
  header: string | null;
  lines: DiffLineProjection[];
}

export interface DiffFileProjection {
  path: string;
  /** The pre-rename path, present only for a rename. */
  oldPath: string | null;
  kind: string;
  /** Git reported binary content, so there are no rows to render. */
  binary: boolean;
  /**
   * The byte bound dropped this file's rows while `additions`/`deletions`
   * stayed real. Rendered as "rows not shown", never as "no changes".
   */
  omitted: boolean;
  additions: number;
  deletions: number;
  hunks: DiffHunkProjection[];
}

export interface WorkspaceDiffEntryProjection {
  path: string;
  /** `HEAD` vs index. Null means the index matches `HEAD` there. */
  index: string | null;
  /** Index vs working tree. */
  worktree: string | null;
  /** Core's own derivation, not the client's. */
  staged: boolean;
  /** Null means Core produced no diff for this path — never "no change". */
  diff: DiffFileProjection | null;
}

/** The target's source-control facts, resampled by Core for this read. */
export interface WorkspaceDiffSourceProjection {
  status: string;
  branch: string | null;
  worktree: string | null;
  ahead: number;
  behind: number;
  added: number;
  deleted: number;
  dirty: boolean;
}

export interface WorkspaceDiffProjection {
  outcome: { state: string; reason: string | null };
  /** The Lane Core's answer describes, or null for the workspace root. */
  targetLaneId: string | null;
  source: WorkspaceDiffSourceProjection | null;
  /** Lexicographic by path, exactly as Core delivered. */
  entries: WorkspaceDiffEntryProjection[];
  /** At least one entry's rows were dropped by the byte bound. */
  truncated: boolean;
  /** Whether a page has actually arrived. */
  loaded: boolean;
  pendingCommandId: string | null;
  capabilityAvailable: boolean;
  /**
   * Core published a workspace source or change fact after this page was read,
   * so the rows on screen describe an older tree.
   *
   * The host counts those two facts; the view compares the count against the
   * one its page was read at. It says "re-read", never "this file changed".
   */
  stale: boolean;
}
