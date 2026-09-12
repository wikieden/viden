/**
 * One workspace file's content, mirroring the host's
 * `WorkspaceFileProjection` field for field (`runtime.workspace_file_reads`,
 * C9).
 *
 * The inventory (`PaletteWorkspaceFiles`, GUI-CORE-022) says a path exists;
 * this says what is in it. Both are Core-owned for the same reason: a client
 * opening a workspace path itself would be outside the client boundary and
 * past the permission gate that governs every other path read.
 *
 * Four facts stay distinct, because each collapses into a lie if the view has
 * to guess it:
 *
 * | fact | meaning |
 * | --- | --- |
 * | `capabilityAvailable: false` | this Core publishes no file reads at all |
 * | `file === null` with a pending outcome | the read is out |
 * | `file.body === "unavailable"` | Core answered: there is nothing to read, and why |
 * | `outcome.state === "rejected"` | Core refused, in its own words |
 *
 * There is no `loaded` flag: every answer Core publishes carries a body, so
 * `file === null` is exactly "nothing answered yet".
 */

/** The frontend-contract-v1 capability that carries a single file's content. */
export const WORKSPACE_FILE_READS_CAPABILITY = "runtime.workspace_file_reads";

export interface WorkspaceFileFacts {
  /** The normalized target-relative path Core resolved, echoed from the answer. */
  path: string;
  /**
   * `text`, `binary`, `unavailable`, or `unknown` for a body shape this build
   * cannot draw. The four are not collapsible: text Core will publish, bytes
   * it will not, nothing to read, and a shape this build predates each get
   * their own sentence.
   */
  body: string;
  /** The published prefix, for a `text` body only. */
  text: string | null;
  /** Core's byte bound cut the body — never that the file was short. */
  truncated: boolean;
  /** Byte length of the whole file. `null` when there was none to measure. */
  size: number | null;
  /** SHA-256 of the whole file, never of the published prefix. */
  sha256: string | null;
  /**
   * `not_found`, `directory`, `unreadable`, or `unknown` for an `unavailable`
   * body whose reason this build cannot name. `null` for every other body.
   */
  reason: string | null;
}

export interface WorkspaceFileProjection {
  outcome: { state: string; reason: string | null };
  pendingCommandId: string | null;
  capabilityAvailable: boolean;
  /** The path this client asked for, so a refusal can name its read. */
  requestedPath: string | null;
  /** The Lane whose worktree was read, or `null` for the workspace root. */
  targetLaneId: string | null;
  /** Core's answer, or `null` while nothing has been answered. */
  file: WorkspaceFileFacts | null;
}

/** The projection a client renders before anything has been read. */
export const IDLE_WORKSPACE_FILE: WorkspaceFileProjection = {
  outcome: { state: "idle", reason: null },
  pendingCommandId: null,
  capabilityAvailable: false,
  requestedPath: null,
  targetLaneId: null,
  file: null,
};
