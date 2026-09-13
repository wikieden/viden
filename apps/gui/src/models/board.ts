/**
 * D16 Agent Board projection types (`runtime.agent_session_control`, C14).
 *
 * Mirrors `apps/gui/src-tauri/src/board.rs` one-to-one. Nothing here derives a
 * session fact: the client renders the Lane/session hierarchy Core published,
 * reads liveness from each session's own `status`, and sends the controls the
 * projection said were available.
 */

/** The capability every control on the screen rides. */
export const AGENT_SESSION_CONTROL_CAPABILITY = "runtime.agent_session_control";

/** Codes a blocked control carries, defined by the Rust projection. */
export const D16_CAPABILITY_ABSENT_CODE = "D16-NO-CAPABILITY";
export const D16_NO_LANE_CODE = "D16-NO-LANE";
export const D16_NO_SESSION_CODE = "D16-NO-SESSION";
export const D16_NO_CORE_FACT_CODE = "D16-NO-CORE-FACT";
export const D16_SESSION_ENDED_CODE = "D16-SESSION-ENDED";
export const D16_SESSION_COMPLETED_CODE = "D16-SESSION-COMPLETED";
export const D16_NO_MODEL_CODE = "D16-NO-MODEL";
/** Prefix of the refusal family Core answers a badly-timed control with. */
export const D16_STATUS_BLOCK_PREFIX = "session_status_";

export interface D16Control {
  pausedAt: number | null;
  resumedAt: number | null;
  /** `cancelled` | `killed`, or `null` while never terminated by a command. */
  terminatedBy: string | null;
  heldInputCount: number;
}

export interface D16Action {
  /**
   * `pause` | `resume` | `kill` | `stop` | `retry` | `switch_model` |
   * `downgrade_manual` | `ask_user`.
   */
  kind: string;
  available: boolean;
  /** Why not, in Core's own vocabulary where Core has one. */
  block: string | null;
  /** True for the two controls with no runtime fact at all. */
  request: boolean;
}

export interface D16Session {
  sessionId: string;
  laneId: string;
  agentId: string;
  model: string | null;
  task: string;
  status: string;
  diagnostic: string | null;
  control: D16Control | null;
  terminal: boolean;
  awaitsHuman: boolean;
  actions: D16Action[];
}

export interface D16Lane {
  laneId: string;
  /** False when Core published sessions for a Lane it published no record for. */
  published: boolean;
  summary: string | null;
  role: string | null;
  route: string | null;
  gateStrength: string | null;
  status: string | null;
  branch: string | null;
  worktree: string | null;
  sessions: D16Session[];
  awaiting: number;
}

export interface D16Evidence {
  id: string;
  kind: string;
  summary: string;
}

export interface D16ConversationMessage {
  messageId: string;
  role: string;
  content: string;
}

export interface D16SwitchModel {
  loadSessionId: string;
  currentModel: string | null;
  agentId: string;
}

export interface D16Detail {
  sessionId: string;
  laneId: string;
  conversation: D16ConversationMessage[];
  evidence: D16Evidence[];
  switchModel: D16SwitchModel | null;
  switchModelBlock: string | null;
}

export interface D16AgentBoardProjection {
  capabilityAvailable: boolean;
  lanes: D16Lane[];
  totalLanes: number;
  totalSessions: number;
  awaitingTotal: number;
  selectedSessionId: string | null;
  detail: D16Detail | null;
}

/**
 * One control the board sends.
 *
 * There is deliberately no `switch_model` intent: the switch is Kill (or Stop)
 * then Start, two operator decisions that stay two commands and two audit rows.
 */
export type D16Intent =
  | { type: "pause"; sessionId: string }
  | { type: "resume"; sessionId: string }
  | { type: "kill"; sessionId: string }
  | { type: "stop"; sessionId: string }
  | { type: "retry"; sessionId: string }
  | { type: "start"; sessionId: string; model: string | null };

export interface D16IntentResult {
  projection: D16AgentBoardProjection;
  pendingCommandId: string | null;
  outcome: {
    state: "idle" | "pending" | "confirmed" | "rejected";
    reason: string | null;
  };
}

/**
 * The pre-Core frame: a capability nobody has answered for yet.
 *
 * `capabilityAvailable` is deliberately `false` rather than `true`: a client
 * that assumed it before the handshake would offer Pause over a Core that
 * cannot hold a session.
 */
export const IDLE_AGENT_BOARD: D16AgentBoardProjection = {
  capabilityAvailable: false,
  lanes: [],
  totalLanes: 0,
  totalSessions: 0,
  awaitingTotal: 0,
  selectedSessionId: null,
  detail: null,
};
