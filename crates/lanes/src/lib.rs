//! Lane orchestration: the durable lifecycle of an agent lane and every local
//! side effect it is allowed to cause.
//!
//! # Boundary
//!
//! This crate sits below `viden-runtime` and knows nothing about sessions,
//! providers, ACP, or any frontend. It owns the lane state machine
//! ([`LaneSupervisor`]), the per-lane worker thread that serializes commands
//! against one lane, and the effect executor that reaches the operating system
//! only through `viden-tools` backends (worktree, process, terminal, patch).
//! It never opens a process, a file, or a repository itself.
//!
//! Policies that belong to the embedding runtime are injected rather than
//! imported, which is what keeps the dependency edge one-directional:
//!
//! - [`LanePersistence`] — where lane facts are appended and rehydrated from;
//! - [`LaneEffectExecutor`] — how a lane effect actually reaches the OS;
//! - [`LaneEventSink`] — where lane events are published;
//! - [`LaneApprovalResolver`] — how a queued approval is re-validated against
//!   the runtime's shared permission gate;
//! - [`LaneCommandRedactor`] — what an announced command may reveal.
//!
//! Permission checks still happen before effects: the lane worker holds a
//! lane-scoped `PermissionEngine` and resolves every mutation through it (and,
//! for a queued approval, through the injected resolver) before the effect
//! executor is called.

mod lane_runtime;
mod lane_supervisor;
mod lane_worker;

pub use lane_runtime::{
    LaneEffectExecutor, LaneEffectRequest, LaneEffectResult, LocalLaneEffectExecutor,
    resolve_lane_output_log,
};
pub use lane_supervisor::{
    LaneCommandRedactor, LanePersistence, LaneSupervisor, WorkflowLanePersistence,
    workspace_eligibility,
};
pub use lane_worker::{
    LaneApprovalIntake, LaneApprovalResolver, LaneEventSink, LanePendingApproval,
};
