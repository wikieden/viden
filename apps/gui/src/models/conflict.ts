/**
 * Structured conflict content, mirroring the host's D12 conflict projection
 * field for field (`runtime.conflict_content`, GUI-CORE-015).
 *
 * Core is the only producer. The client never reads a worktree, never parses a
 * reason sentence into rows, and — the invariant that shapes this whole
 * module — never presents a merged result:
 *
 * > **Two sides plus the patch preimage. Not a three-way merge.**
 *
 * `ours` is a read-only read of the target file where the hunk said its region
 * was, `theirs` is the incoming hunk's new side, and `base` is that hunk's own
 * preimage — the text the patch expected to find. Core computes no merge base
 * and resolves nothing, so there is no merged text to render and no resolve
 * action to offer. The screen has neither.
 *
 * Four absences stay four different sentences, because each is a different
 * fact and none of them means "no conflict":
 *
 * | fact | meaning |
 * | --- | --- |
 * | `conflictContentAvailable: false` | Core publishes no structured content at all |
 * | `content: null` on a bounce | Core published none for this record — an operator bounce carries none by contract |
 * | `file.omitted` | this file conflicted; its lines were over the byte bound |
 * | `content.truncated` | at least one file lost its lines to that bound |
 */

/** The `frontend-contract-v1` capability that carries conflict lines. */
export const CONFLICT_CONTENT_CAPABILITY = "runtime.conflict_content";

/** The audit object a cross-link opens, `kind:id`, as Core linked it. */
export interface ConflictAuditScope {
  kind: string;
  id: string;
}

/** One reviewed-evidence binding a conflict baseline names. */
export interface ConflictEvidenceProjection {
  evidenceId: string;
  sourceHash: string;
  /** Display form of `sourceHash`; the full value stays available. */
  shortHash: string;
  /**
   * Where the chip routes. `AuditQuery` filters by object, so the chip carries
   * the evidence object Core links rather than the bare id — the same route
   * D12's revert rows already take.
   */
  auditScope: ConflictAuditScope;
}

/**
 * What the `ours` side was read against.
 *
 * `kind` is Core's own tag. `ConflictBaseline` is open-ended, so an unmodelled
 * kind is rendered as itself; `unknown` is a real Core answer — it held no
 * baseline it could name — and is never rendered silently as `HEAD`.
 */
export interface ConflictBaselineProjection {
  kind: string;
  /** Present only for `revision`. */
  sha: string | null;
  shortSha: string | null;
  /** Present only for `evidence`. */
  bindings: ConflictEvidenceProjection[];
}

/** One hunk the strict apply refused. */
export interface ConflictHunkProjection {
  /** 1-based first line of `ours` in the current file. `0` when the patch declared none. */
  oursStart: number;
  ours: string[];
  /** 1-based first line of `theirs` in the patched file. */
  theirsStart: number;
  theirs: string[];
  /**
   * `null` means the hunk had no preimage at all (a binary file); an empty
   * array means it expected an empty region, which is what a creation hunk
   * expects. The two are different facts and get different sentences.
   */
  base: string[] | null;
  /** Core's own `ConflictHunkReason` tag; open-ended, so an unnamed one renders raw. */
  reason: string;
}

export interface ConflictFileProjection {
  path: string;
  hunks: ConflictHunkProjection[];
  /** The file conflicted; its lines were dropped by Core's byte bound. */
  omitted: boolean;
}

export interface ConflictContentProjection {
  baseline: ConflictBaselineProjection;
  files: ConflictFileProjection[];
  /** At least one file's hunks were dropped by Core's byte bound. */
  truncated: boolean;
}
