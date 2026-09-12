/**
 * The cockpit layout record, mirroring the host's
 * `LayoutPreferencesProjection` field for field (`ui.layout_preferences`, C5).
 *
 * G3 left two seams in memory — the Lane sidebar's `D-SIDEBAR` mode and the
 * statusbar's `D-STATUSBAR` ambient set — because the frontend contract makes
 * Core the single preference authority and the design prototype's
 * `localStorage` keys (`vd-leftmode`, `vd-leftw`) would have been the second
 * one it forbids. This is the record that closes them.
 *
 * Three facts stay distinct, because each collapses into a lie if the view has
 * to guess it:
 *
 * | fact | meaning |
 * | --- | --- |
 * | `capabilityAvailable: false` | this Core publishes no layout record at all |
 * | `laneSidebarMode: null` | the capability exists and Core published no record yet |
 * | `persisted: false` | Core applied the record and could not write the file |
 *
 * The cockpit holds no copy beyond the view it is rendering: every wake
 * re-reads this projection, so a change made in another window lands here
 * rather than being overwritten by a remembered value.
 */

import type { PreferenceDiagnostic } from "../ui/theme";

/** The frontend-contract-v1 capability that carries the layout record. */
export const LAYOUT_PREFERENCES_CAPABILITY = "ui.layout_preferences";

/** The patch the webview sends. An omitted axis is left as it is. */
export interface LayoutPreferencePatch {
  laneSidebarMode?: string;
  hiddenStatusbarSegments?: string[];
}

export interface LayoutPreferencesProjection {
  outcome: { state: string; reason: string | null };
  pendingCommandId: string | null;
  capabilityAvailable: boolean;
  /**
   * `pinned`, `floating`, `unknown` for a mode this build cannot draw, or
   * `null` when Core published no record.
   */
  laneSidebarMode: string | null;
  hiddenStatusbarSegments: string[];
  /** `null` before any layout command answered in this session. */
  persisted: boolean | null;
  diagnostics: PreferenceDiagnostic[];
}

/** The projection an unbound host renders: nothing published, nothing claimed. */
export const IDLE_LAYOUT_PREFERENCES: LayoutPreferencesProjection = {
  outcome: { state: "idle", reason: null },
  pendingCommandId: null,
  capabilityAvailable: false,
  laneSidebarMode: null,
  hiddenStatusbarSegments: [],
  persisted: null,
  diagnostics: [],
};
