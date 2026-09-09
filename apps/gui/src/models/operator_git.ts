/**
 * The DiffReview action side, mirroring the host's `OperatorGitProjection`
 * field for field (`runtime.operator_git`, GUI-CORE-020).
 *
 * Core is the only actor. The client names an action, waits for the ordered
 * answer, and renders it: it never runs `git`, never falls back to a shell
 * command, and never reads a fact out of `output`, which is display text under
 * Core's 8 KiB bound.
 *
 * Three pairs stay distinct here, because each collapses into a lie if the
 * view has to guess it:
 *
 * | fact | meaning |
 * | --- | --- |
 * | `outcome.state === "rejected"` | Core refused **before** anything ran |
 * | `result.kind === "failed"` | the effect **was** attempted and failed |
 * | `capabilityAvailable: false` | Core publishes no operator git at all |
 * | `ownerAvailable: false` | this client has no Core owner to act **as** |
 * | `awaitingApproval` | Core published a `git` ask; the dock owns it now |
 *
 * Rendering a `Failed` as a denial would send an operator to their permission
 * file about a problem in their own index; rendering a refusal as a failure
 * would claim an effect Core never ran.
 */

import type { WorkspaceDiffSourceProjection } from "./diff_review";

/** The frontend-contract-v1 capability that carries operator git actions. */
export const OPERATOR_GIT_CAPABILITY = "runtime.operator_git";

/**
 * Core's bound on a commit message, in **bytes**.
 *
 * `MAX_OPERATOR_COMMIT_MESSAGE_BYTES`. Counted as UTF-8 bytes rather than as
 * characters, because that is what Core measures: a Chinese commit message
 * reaches the bound at roughly a third of the character count, and a
 * character-based check would let the client send something Core refuses.
 */
export const MAX_COMMIT_MESSAGE_BYTES = 4 * 1024;

/** One operator action, exactly as the host deserializes it. */
export type OperatorGitActionRequest =
  /** Empty `paths` means every changed path — Core's own "stage all". */
  | { type: "stage"; paths: string[] }
  | { type: "unstage"; paths: string[] }
  | { type: "commit"; message: string }
  | { type: "push"; remote: string | null; setUpstream: boolean }
  | { type: "fetch"; remote: string | null };

/**
 * Every failure class Core can publish, plus `unknown` for a class this build
 * cannot name.
 *
 * `unknown` is not a gap to close later: `OperatorGitFailureClass` is
 * open-ended, and squeezing an unrecognized failure into the nearest-looking
 * class would offer the operator the wrong recovery.
 */
export type OperatorGitFailureClass =
  | "nothing_to_commit"
  | "non_fast_forward"
  | "authentication_required"
  | "remote_unreachable"
  | "no_upstream"
  | "path_outside_repository"
  | "other"
  | "unknown";

export interface OperatorGitResultProjection {
  /** `completed` or `failed`. Never `rejected`: no effect was attempted then. */
  kind: string;
  /** Core's own audit verb, echoed from the settling event. */
  action: string;
  targetLaneId: string | null;
  /** The authorization record appended before the effect. */
  auditId: string;
  /** `completed` only. Display text, never parsed. */
  output: string | null;
  /** Core's 8 KiB bound cut `output`. */
  truncated: boolean;
  /** `completed` only: the target's facts resampled *after* the effect. */
  source: WorkspaceDiffSourceProjection | null;
  /** `failed` only. */
  failureClass: string | null;
  /** `failed` only: git's own message, verbatim. */
  detail: string | null;
}

export interface OperatorGitProjection {
  /** `idle`, `pending`, `confirmed`, or `rejected` with Core's own reason. */
  outcome: { state: string; reason: string | null };
  pendingCommandId: string | null;
  /** The audit verb of the action in flight. */
  pendingAction: string | null;
  /** Core published a pending `git` approval for the acting owner. */
  awaitingApproval: boolean;
  result: OperatorGitResultProjection | null;
  capabilityAvailable: boolean;
  /** Core published exactly one owner binding this client may act as. */
  ownerAvailable: boolean;
  /** This client's own words for why there is no owner. Never Core's. */
  ownerUnavailableReason: string | null;
}

/** The projection an unbound host renders: nothing available, nothing claimed. */
export const IDLE_OPERATOR_GIT: OperatorGitProjection = {
  outcome: { state: "idle", reason: null },
  pendingCommandId: null,
  pendingAction: null,
  awaitingApproval: false,
  result: null,
  capabilityAvailable: false,
  ownerAvailable: false,
  ownerUnavailableReason: null,
};

/** UTF-8 length, which is the unit Core's 4 KiB commit-message bound uses. */
export function commitMessageBytes(message: string): number {
  return new TextEncoder().encode(message).length;
}
