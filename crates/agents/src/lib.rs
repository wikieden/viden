//! External agent adapters: every way Viden drives an agent that runs in
//! another process.
//!
//! # Boundary
//!
//! This crate sits below `viden-runtime` and knows nothing about sessions,
//! lanes, providers, frontends, or the runtime's trust loop. It owns one
//! strategy per external CLI — the generic ACP JSON-RPC client, the Codex
//! app-server and CLI client, and the spawn/probe infrastructure both share —
//! and it reaches the operating system only through `viden-tools`
//! capabilities (filesystem, process, terminal). It never opens a process, a
//! file, or a socket itself.
//!
//! Runtime policy arrives as parameters rather than imports, which is what
//! keeps the dependency edge one-directional:
//!
//! - a `PermissionContext` and a `PermissionEngine` describe what the agent's
//!   reverse-RPC filesystem and terminal requests are allowed to do;
//! - an approver closure carries an `Ask` out to whichever operator surface
//!   the embedding runtime owns;
//! - a [`RuntimeEventSink`] receives the runtime events an agent turn
//!   produces, so this crate never decides where a fact is persisted.
//!
//! Permission checks still happen before effects: every reverse-RPC mutation
//! resolves through `viden_permissions::resolve_permission`, the shared
//! decide -> ask -> apply_approval gate, before the capability is called.
//!
//! Module ownership, in dependency order:
//!
//! - [`infra`]: process spawning, shell command planning, output capture, and
//!   the static adapter descriptor table. Knows nothing about a protocol.
//! - [`acp`]: the ACP wire protocol — framing, handshake, prompt turns, update
//!   and patch parsing, and the reverse-RPC filesystem/terminal callbacks the
//!   agent makes back into us.
//! - [`codex`]: the Codex band — CLI command handlers, job records and their
//!   lifecycle, diagnostics, and the app-server JSON-RPC client.
//! - [`glue`]: protocol-independent session glue — the resident ACP session
//!   cache, the typed-session entrypoints, job/task/adapter-view projections,
//!   and runtime-event persistence.
//! - [`render`]: presentation helpers for the `/agent` command surface.
//!
//! The `SessionEngine` command surface that dispatches to these bands stays in
//! `viden-runtime` (`agent_dispatch.rs`); it calls into the bands rather than
//! reaching into their internals.

use std::sync::Arc;

use viden_types::RuntimeEvent;

mod acp;
mod codex;
mod glue;
mod infra;
mod render;

#[cfg(test)]
mod streaming_tests;
#[cfg(test)]
mod tests;

/// Where an agent turn publishes the runtime events it produces.
///
/// The embedding runtime supplies the sink, so this crate decides what
/// happened without deciding where the fact is recorded.
pub type RuntimeEventSink = Arc<dyn Fn(Vec<RuntimeEvent>) + Send + Sync + 'static>;

pub use acp::{
    AcpRunArgs, handle_acp_agent_run_command, handle_agent_auth_command,
    handle_agent_probe_command, parse_acp_run_args, run_acp_smoke_gate,
};
pub use codex::{
    CodexJobKind, ParsedCodexRunArgs, cancel_codex_job, codex_command, codex_run_command_args,
    ensure_codex_target, handle_codex_challenge_command, handle_codex_review_command,
    parse_codex_run_args, render_codex_job_result, render_codex_job_status,
    start_codex_app_server_job, start_codex_job,
};
pub use glue::{
    AgentSessionApprover, append_agent_turn_runtime_events, cancel_typed_agent_session,
    mark_typed_agent_session_status, probe_typed_agent_adapter, resume_typed_agent_session,
    retry_typed_agent_session, shutdown_resident_acp_sessions, start_typed_agent_session,
    tracked_agent_job_runtime_events, tracked_agent_job_sessions, tracked_agent_job_tasks,
    typed_agent_adapter_views, typed_agent_session_request_from_compat_input,
    validate_typed_agent_session_request,
};
pub use render::{render_agent_doctor, render_agent_list, render_agent_logs_help};
