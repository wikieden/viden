/**
 * `runtime.approval_scopes` (C15, GUI-CORE-019) client constants.
 *
 * The projection types live beside the permission dock's in `workspace.ts`,
 * because they travel on the same Core read; this module holds only what both
 * the dock and the settings panel name, so neither imports the other.
 */

/** The capability that carries the standing `Always` scope and `Edit`. */
export const APPROVAL_SCOPES_CAPABILITY = "runtime.approval_scopes";

/**
 * Core's own refusal when a client sends a scope the request did not offer.
 *
 * Rendered as Core's word rather than softened: the operator has to be able to
 * match it against the audit row Core wrote.
 */
export const SCOPE_NOT_OFFERED = "scope_not_offered";

/**
 * Core's refusal when a revision is byte-identical to the original.
 *
 * The dock does not pre-empt it: Core compares the canonical tool input and the
 * client only holds the preview it parsed, so a local comparison could refuse a
 * revision Core would have accepted.
 */
export const REVISION_UNCHANGED = "revision_unchanged";

export type { StandingApproval, StandingApprovalsProjection, StandingApprovalIntentResult } from "./workspace";
