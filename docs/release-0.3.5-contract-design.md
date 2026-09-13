# Viden 0.3.5 Core Contract Increment - Plan Documents, Agent Session Control, Approval Scopes, Starter-Lane Agent Binding

Chinese version: [release-0.3.5-contract-design.zh-CN.md](release-0.3.5-contract-design.zh-CN.md)

This is the design for the additive capabilities that
[release-0.3.5-plan.md](release-0.3.5-plan.md) dispatches as batches C13 to
C15, and the record of the hardening rules batch C12 implements without a
capability. It is a design, not a description of implemented behavior.
Field names and bounds here are the brief for implementation; a batch may
deviate only with the reason recorded in its report and, if the deviation
survives review, in this document's amendments.

Target: Core `0.3.8`, schema `1`, `compatibility = additive_capability_gated`.
Governing rules: the protocol evolution rules and command ownership table in
`docs/frontend-integration-contract.md`, the extension procedure in
`docs/core-0.3-compatibility.md`, and the Shared Rules of
`release-0.3.3-contract-design.md`, which apply unchanged (additive only,
every new event known with a fixture in the same commit, `command_id`
correlation with `CommandRejected`, permission before effect on the same
`ToolSpec` vocabulary, audit before mutation, bounded and honest, explicit
target, capability gating with the nine base fixtures byte-immutable).

Written 2026-09-13 from a read of `main` at `75e2a84a` (code identical to
the `0.3.4` release at `edcc7b3d`). Facts cited below were verified at that
commit. Reference architectures consulted per `AGENTS.md`: Codex's
`update_plan` tool (steps with pending, in-progress, completed status) and
plan-then-execute flow; Claude Code's plan mode with an explicit exit
approval; DeepSeek-TUI's read-only Plan mode built on `update_plan`. Viden
diverges in one way it already chose in `0.3.0`: the plan is a Core-owned
durable fact with an operator decision, never client state.

## Why These Four

| Missing fact | Where it stops | Capability |
| --- | --- | --- |
| a plan the operator can read, accept, and hand off | plan mode denies every mutation (`crates/permissions/src/lib.rs:192`, the frozen `plan-denial` fixture) but the planner's output is transcript text; nothing says what the plan is, whether it was accepted, or what work it became | 1 `runtime.plan_documents` |
| pause and kill for an Agent session | `RuntimeCommand` carries `StartAgentSession`, `SendAgentSessionInput`, `RetryAgentSession`, `CancelAgentSession` and nothing else for a session (`crates/types/src/runtime.rs:35`); D10 renders Pause and Kill disabled and says so (`apps/gui/README.md:1472-1473`) | 2 `runtime.agent_session_control` |
| an Always scope and an Edit decision | `ApprovalScope` is `Once`, `Session`, `RepoAllowlist` (`crates/types/src/approval.rs:23`); every installed rule is `PermissionRuleSource::Session` (`crates/permissions/src/lib.rs:266`); the engine already carries `updated_input` and `user_modified` (`crates/types/src/lib.rs:800-811`) but no decision variant delivers a revision (register 019) | 3 `runtime.approval_scopes` |
| the agent the operator picked in D4 reaching Core | `StarterLaneRequest` is `lane_id`, `preset`, `branch`, `worktree_path` (`crates/types/src/frontend_services.rs:221`); the pick travels nowhere and the wizard says so (register 030) | 4 `runtime.starter_lane_agent` |

Capability count: 29 to 33. Each capability serializes its new view-state
field with `#[serde(default, skip_serializing_if = ...)]` so a view that
carries none of it encodes to the bytes it did before; the nine base
fixtures and the `0.3.4` extension fixtures prove that in every batch.

## 0. Hardening Rules (C12, no capability)

These close compatibility follow-ups 15, 16, 14, 10, and 13 and are
recorded here so the contract states them, not only the batch brief.

1. **Both approval paths audit.** An approval the lane supervisor owns is
   audited exactly like a runtime-owned one: the same pre-minted `audit_id`
   the client saw on the request, the same `approval.<decision>` row, written
   durably before the fact that announces the decision. Audit stays
   runtime-owned policy; `crates/lanes` gains no audit writer.
2. **The queue is acknowledged at accept.** A `QueueFollowUp` received while
   a turn runs enters `queued_inputs` and emits `InputQueued` when the
   command is accepted; `InputDequeued` is emitted when the drain starts
   that turn. The two never share an instant while a turn was running.
3. **An archived diff names its file.** `EvidenceContent::Diff` built from
   canonical patch bytes carries the entry's own path and
   `WorkspaceChangeKind`, exactly as the decision context does; a file
   modified in place is never published as a rename.
4. **An Agent-reported patch is durably canonical.** The artifact on disk
   carries the same `canonical` the live `EvidenceCanonicalized` published.
5. **`interaction-closed-loop` is refreshed** from its generator with the
   corpus digest updated in one bookkeeping commit; no other fixture moves.

## 1. `runtime.plan_documents` (C13)

### Types

```rust
// crates/types/src/plan.rs (new)

pub struct PlanDocumentId(pub String); // "plan_<ulid>"

#[serde(rename_all = "snake_case")]
pub enum PlanDocumentStatus {
    /// The planner has started writing; no revision proposed yet.
    Drafting,
    /// A revision awaits the operator's decision.
    Proposed,
    Accepted,
    Rejected,
    /// Accepted and turned into work; `handoff` names the DAG.
    HandedOff,
    /// A newer revision replaced this decision.
    Superseded,
}

#[serde(rename_all = "snake_case")]
pub enum PlanStepStatus { Pending, InProgress, Completed }

pub struct PlanStep {
    pub id: String,                  // stable within the plan, "step_<n>"
    pub title: String,               // <= 256 bytes
    pub status: PlanStepStatus,
    pub role: Option<AgentRole>,     // defaults to Coder at handoff
    pub file_scope: Vec<String>,     // workspace-relative, <= 64 entries
    pub dependencies: Vec<String>,   // step ids inside this plan
}

#[serde(rename_all = "snake_case")]
pub enum PlanSectionKind {
    Requirements, Assumptions, Constraints, ArchitectureDecisions,
    Risks, TestStrategy, AcceptanceCriteria,
}

pub struct PlanSection { pub kind: PlanSectionKind, pub body: String } // body <= 16 KiB

#[serde(rename_all = "snake_case")]
pub enum PlanAuthor {
    Assistant { session_id: SessionId, turn_id: String },
    Operator,
}

pub struct PlanHandoff { pub dag_id: AgentDagId, pub handed_off_at: u64, pub audit_id: String }

pub struct PlanDocumentRecord {
    pub plan_id: PlanDocumentId,
    pub owner: RuntimeOwner,         // session or Lane scope, never default
    pub title: String,
    pub goal: String,
    pub status: PlanDocumentStatus,
    pub revision: u32,               // 1-based; every proposal increments
    pub content_sha256: String,      // over title, goal, sections, steps
    pub sections: Vec<PlanSection>,  // at most one per kind
    pub steps: Vec<PlanStep>,        // <= 64
    pub proposed_by: PlanAuthor,
    pub created_at: u64,
    pub updated_at: u64,
    pub decided_at: Option<u64>,
    pub decision_audit_id: Option<String>,
    pub handoff: Option<PlanHandoff>,
}

// RuntimeViewState
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub plan_documents: Vec<PlanDocumentRecord>,
```

Commands (`RuntimeCommand`, all owner-scoped):

```rust
/// Operator-authored or client-forwarded proposal. The model never sends
/// this; it calls the built-in `update_plan` tool instead.
ProposePlanDocument { draft: PlanDocumentDraft },      // title, goal, sections, steps
AcceptPlanDocument  { plan_id, revision, content_sha256 },
RejectPlanDocument  { plan_id, revision, content_sha256, reason: String },
/// Turns an Accepted plan into the DAG Core already owns.
HandOffPlanDocument { plan_id, revision, content_sha256 },
```

Events (`RuntimeEventKind`):

```rust
PlanDocumentUpdated  { plan: PlanDocumentRecord },              // every revision and status change
PlanDocumentDecided  { plan_id, revision, status, audit_id, owner },
PlanDocumentHandedOff{ plan_id, revision, dag_id, audit_id, owner },
```

### The `update_plan` tool

A built-in, non-mutating tool (`ToolSpec { name: "update_plan", is_mutating:
false }`) with the Codex shape: `explanation: Option<String>`, `plan:
[{ step, status }]`, extended by optional `title`, `goal`, `sections`,
`role`, `file_scope`, `dependencies`. A call creates the owner's plan if
none is open, otherwise proposes the next revision. It is available in every
work mode, and it is the one tool whose effect plan mode allows, because its
effect is a Core fact about the plan and not a workspace, memory, or task
mutation. The permission engine sees it as a safe read; nothing about it
bypasses the gate for any other tool.

### Semantics

- One open plan per owner: a plan is open while its status is `Drafting`,
  `Proposed`, or `Accepted`. A new proposal while one is `Proposed` or
  `Accepted` increments `revision`, sets `Proposed`, and voids the earlier
  decision by publishing the earlier revision as `Superseded` inside the
  same record's history (the record carries the latest; the JSONL keeps
  every revision).
- Decisions are pinned: `Accept` and `Reject` carry the `revision` and
  `content_sha256` the operator saw; a mismatch is `CommandRejected` with
  reason `plan_revision_stale`. A decision writes its audit row
  (`plan.accepted`, `plan.rejected`, actor Operator, object the plan) before
  `PlanDocumentDecided`.
- Handoff is the only mutation and is refused in plan mode:
  `HandOffPlanDocument` under `PermissionMode::Plan` is `CommandRejected`
  with reason `plan_mode`, the same reason family the starter-Lane preview
  uses. Accepted plans wait until the operator switches to Build. On
  success Core builds `AgentDagTaskSpec`s from the steps (one per step,
  `role` default `Coder`, `objective` = step title, `dependencies` mapped,
  `file_scope` copied, `permission_policy` = the Lane's mutation policy
  name) and runs the existing `start_agent_dag` path under the command's
  id, so the client sees `AgentDagUpdated` (status `Active`) and
  `PlanDocumentHandedOff` in that order, and the plan's `handoff` names the
  DAG. Handoff of a plan that is not `Accepted` is `CommandRejected`.
- Follow-ups typed while the planner runs go through `QueueFollowUp` as
  today; nothing new is needed once C12 rule 2 lands.
- Bounds: 64 steps, seven sections of 16 KiB, 256-byte titles, 4 KiB goal.
  Over-bound input to `update_plan` is a tool error the model sees, not a
  truncated plan.
- Durability: every `PlanDocumentUpdated` is a JSONL fact; replay and resume
  rebuild `plan_documents` exactly.

### Fixture `plan-documents.json`

Plan mode on; `update_plan` proposes revision 1; a second call proposes
revision 2 (`Superseded` history visible in the events); `Accept` with
revision 1 rejected `plan_revision_stale`; `Accept` revision 2 → audit row
→ `PlanDocumentDecided`; `HandOff` in plan mode rejected `plan_mode`;
`SetWorkMode Build`; `HandOff` → `AgentDagUpdated` (Active, two tasks with
the mapped dependency) → `PlanDocumentHandedOff`; final view: one plan
`HandedOff` with `handoff.dag_id`, `agent_dags` holds the DAG.

### Clients

GUI G8 Plan View: sections, steps with status, the decision row (Accept,
Reject with reason, Hand off), revision and stale-decision refusal copy, the
handoff link into D13. TUI T4: a plan lens with the same three actions.
Both show "no plan proposed" as absence, never an empty plan.

## 2. `runtime.agent_session_control` (C14)

### Types

```rust
// crates/types/src/agent.rs, additive variants
pub enum AgentSessionStatus { Starting, Running, WaitingApproval, Completed, Failed, Cancelled,
    /// Alive, no prompt in flight, no input forwarded until resumed.
    Paused,
    /// Terminated without a cancel grace; terminal, retryable like Failed.
    Killed,
}

#[serde(rename_all = "snake_case")]
pub enum AgentSessionTermination { Cancelled, Killed }

pub struct AgentSessionControlView {
    pub paused_at: Option<u64>,
    pub resumed_at: Option<u64>,
    pub terminated_by: Option<AgentSessionTermination>,
    /// Inputs held while paused; forwarded in order on resume.
    pub held_input_count: u32,
}

// AgentSessionView
#[serde(default, skip_serializing_if = "Option::is_none")]
pub control: Option<AgentSessionControlView>,
```

Commands: `PauseAgentSession { session_id }`, `ResumeAgentSession {
session_id }`, `KillAgentSession { session_id }`. Events:
`AgentSessionUpdated` carries `Paused` and the resumed status;
`AgentSessionKilled { session }` is the new terminal event, and the reducer's
terminal arm (`crates/types/src/runtime.rs:1371`) treats it like
`Completed`/`Failed`.

### Semantics

Neither ACP nor the Codex app-server has a pause verb; ACP has
`session/cancel` (`crates/agents/src/acp.rs:1991`) and Core's cancel is that
plus a bounded process-termination fallback (`crates/agents/src/codex.rs:566-657`).
Pause is therefore a Core-side state over the session's input path:

- **Pause** (from `Starting`, `Running`, `WaitingApproval`): Core sends the
  adapter's cancel for the in-flight prompt with today's grace, keeps the
  resident session and its `load_session_id` alive, and forwards no further
  `SendAgentSessionInput`; inputs sent while paused are accepted
  (`AgentSessionInputAccepted`) and held, counted in `held_input_count`. An
  approval the session was waiting on is resolved by Core as `Deny` with
  reason `session_paused` and audited, because the prompt it belonged to is
  gone. Status `Paused`, `paused_at` set. Audit row `agent_session.pause`.
- **Resume** (from `Paused` only): if inputs are held, the first is
  forwarded and status is `Running`; otherwise status is `Completed`, which
  in this contract already means "no turn in flight, session alive and
  accepting input". `resumed_at` set. Audit row `agent_session.resume`.
- **Kill** (from any non-terminal status, including `Paused`): immediate
  process termination with no cancel grace, resident session retired, held
  inputs discarded and named in the diagnostic, pending approvals denied
  with reason `session_killed` and audited; status `Killed`,
  `terminated_by: Killed`. `RetryAgentSession` accepts `Killed` as it
  accepts `Failed`. Audit row `agent_session.kill`.
- **Cancel** is unchanged and now stamps `terminated_by: Cancelled`.
- **Switch model** is by design a client composition of Kill (or Cancel)
  and `StartAgentSession` with `model` and `load_session_id`, which the
  contract already supports; no new command. G9 renders it as that two-step
  flow and says so.
- Commands against a status that does not allow them are `CommandRejected`
  with reason `session_status_<current>`; a session the owner does not hold
  is rejected as today.
- A Lane's `active_session_ids` keeps a paused session; a killed one leaves
  it as a cancelled one does.

### Fixture `agent-session-control.json`

Start → `Running` → an approval pending → `Pause` → approval denied
`session_paused` with an audit row → `Paused`, `held_input_count: 0` →
`SendAgentSessionInput` while paused → accepted, `held_input_count: 1` →
`Resume` → `Running`, the held input forwarded → `Kill` → `AgentSessionKilled`,
`terminated_by: Killed` → `Retry` → new `Starting` under the same logical
session. `Resume` on a running session → `CommandRejected
session_status_running`.

### Clients

GUI G9 Agent Board and the D10 action row: Pause, Resume, Kill live; Stop
stays Cancel; switch model as the two-step flow. TUI T4: session control
keys in the Lane view.

## 3. `runtime.approval_scopes` (C15, closes GUI-CORE-019)

### Types

```rust
// crates/types/src/approval.rs, additive variants
pub enum ApprovalScope { Once, Session { .. }, RepoAllowlist { .. },
    /// A standing rule across sessions and repositories, revocable.
    Always,
}
pub enum ApprovalDecision { Allow { scope }, Deny,
    /// The operator revised the input; Core re-evaluates the revision.
    Edit { revised_input: ToolInput },
}

pub struct StandingApprovalView {
    pub rule_id: String,            // "rule_<ulid>"
    pub tool_name: String,
    pub rule_content: String,       // the same encoding Session rules use
    pub granted_at: u64,
    pub granted_in_workspace: String, // workspace_id
    pub audit_id: String,
}

// ApprovalRequestView, additive
#[serde(default, skip_serializing_if = "Option::is_none")]
pub revised_from: Option<String>,   // the request this revision replaces
#[serde(default)]
pub editable: bool,

// RuntimeViewState
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub standing_approvals: Vec<StandingApprovalView>,
```

Commands: `RevokeStandingApproval { rule_id }`. Events:
`StandingApprovalGranted { rule: StandingApprovalView }`,
`StandingApprovalRevoked { rule_id, audit_id, owner }`. `ApprovalResolved`
carries the new decision variants unchanged in shape.

### Semantics

- **Always** installs a `PermissionRuleSource::UserSettings` allow rule with
  the same tool-plus-content key a `Session` rule uses, persisted in the
  user-level settings file `viden-config` owns (a new `[[permissions.standing]]`
  table; the batch names the file). It loads at bootstrap for every
  workspace, so `decide` answers `RuleAllow` without a prompt. It is offered
  (`allowed_scopes` contains `Always`) only when the request's `risk` is not
  `High` and the mode is not `Plan`; a client sending it when not offered is
  `CommandRejected` `scope_not_offered`. Granting writes `approval.always`
  and `StandingApprovalGranted`; revoking writes `approval.revoke` and
  removes the rule from the file and the engine. The revocation path exists
  in the same batch as the grant, never later.
- **Edit** is offered only when `editable` is true: Core marks editable the
  built-in tools whose input is a flat text map an operator can safely
  revise (shell commands and path-bearing inputs), never patch-bearing
  inputs. On `Edit`, Core runs `decide(tool, revised_input)`: `Allow` →
  the tool runs with `updated_input = revised`, `user_modified: true`, the
  original request resolves with decision `Edit`, audit row
  `approval.edit` with both inputs' bounded previews; `Ask` → the original
  resolves `Edit` and a new `ApprovalRequested` follows whose view has
  `revised_from` and a fresh pre-minted `audit_id`; `Deny` (rule or plan
  mode) → the original resolves `Deny` with the engine's reason. A revision
  that is byte-identical to the original is `CommandRejected`
  `revision_unchanged`.
- Both paths audit on both approval branches (C12 rule 1).

### Fixture `approval-scopes.json`

A medium-risk shell request with `allowed_scopes` including `Always` → `Allow
Always` → `StandingApprovalGranted`, audit row; the same tool and input in a
second session → tool runs on `RuleAllow` with no `ApprovalRequested`;
`RevokeStandingApproval` → revoked, audit row; the request returns on a
third try. An editable request → `Edit` to a narrower path → new request
`revised_from` → `Allow Once` → tool ran with the revised input
(`user_modified: true` visible in the tool-call view); `Edit` under plan
mode → `Deny plan_mode`; a high-risk request's `allowed_scopes` lacks
`Always` and sending it is rejected `scope_not_offered`.

### Clients

GUI: the permission dock's Always and Edit go live, `Shift+A` returns to
Always, the standing approvals list gets a revoke control in settings and a
D14 link. TUI: the decision modal gains `a` (Always) and `e` (Edit with an
inline field).

## 4. `runtime.starter_lane_agent` (C15, closes GUI-CORE-030)

### Types

```rust
pub struct StarterLaneAgentBinding {
    pub agent_id: String,
    pub model: Option<String>,
    pub task: Option<String>,        // defaults to the preset's summary
}

// StarterLaneRequest, additive
#[serde(default, skip_serializing_if = "Option::is_none")]
pub agent: Option<StarterLaneAgentBinding>,

// StarterLanePreview, additive
#[serde(default, skip_serializing_if = "Option::is_none")]
pub agent: Option<StarterLaneAgentBinding>,   // echoed; diagnostics name an unknown adapter

#[serde(rename_all = "snake_case")]
pub enum StarterLaneSessionOutcome {
    Started { session_id: SessionId },
    Refused { reason: String },
}

// StarterLaneReceipt, additive
#[serde(default, skip_serializing_if = "Option::is_none")]
pub session: Option<StarterLaneSessionOutcome>,
```

### Semantics

- The preview's `content_sha256` covers the binding, so changing the pick
  invalidates the preview as `RequestChanged`, like any other field. A
  binding naming an adapter absent from `agent_adapters` is echoed with a
  diagnostic; the preview is still creatable and the receipt will say the
  session was refused.
- `CreateStarterLane` with a binding: the Lane is created and its receipt
  written exactly as today; then Core starts the session through the same
  path `StartAgentSession` takes, under the same owner and lane-binding
  checks (`crates/runtime/src/runtime_supervisor.rs:974-1040`), and stamps
  the outcome on the receipt. Lane creation never fails because the session
  did not start; the receipt's `session: Refused { reason }` is the honest
  answer, and the client may still send `StartAgentSession` itself.
- The two-step flow remains valid; a request without `agent` behaves as
  before, byte for byte.

### Fixture `starter-lane-agent.json`

Preview with a binding → echoed; create → receipt `session: Started` →
`AgentSessionUpdated Starting` under the Lane owner; a second preview with
an unknown `agent_id` → diagnostic → create → receipt `session: Refused`
and the Lane exists.

### Clients

GUI G9: D4 step 2 sends the binding and step 4 shows the receipt's outcome.
TUI T4: `/lanes` creation takes an optional agent.

## Design Package Inputs (for batch DS)

Neither Plan Studio nor Agent Board has a registered D-screen. The design
batch registers both under the package's own rules; the tables below are
the facts each control maps to, so no control is drawn without a fact or a
stated request.

Plan Studio (a centre-pane view, chrome retained, reached from the rail):

| Element | Fact or command | Absent state |
| --- | --- | --- |
| mode badge | `WorkMode`, `PermissionMode::Plan` | never absent |
| sections | `PlanDocumentRecord.sections` | "no plan proposed" |
| steps with status | `PlanDocumentRecord.steps` | same |
| revision and author | `revision`, `proposed_by` | same |
| Accept, Reject with reason | `AcceptPlanDocument`, `RejectPlanDocument` | disabled while not `Proposed` |
| Hand off | `HandOffPlanDocument` | disabled while not `Accepted`; labelled "refused in plan mode" when the mode is Plan |
| handoff link | `handoff.dag_id` → D13 | absent until handed off |
| composer and queue | existing composer, `queued_inputs` | existing |

Agent Board (a centre-pane view over the Lane/session hierarchy):

| Element | Fact or command | Absent state |
| --- | --- | --- |
| Lane rows, sessions under them | `lanes`, `agent_sessions` by `lane_id` | "no Lane" |
| role, task, status, diagnostic | `AgentSessionView` | per field |
| event timeline | `runtime.audit` rows scoped to the session, `agent_conversation` | "no rows" |
| evidence | `latest_evidence` by owner | "none published" |
| Pause, Resume, Kill | C14 commands | disabled with the status reason |
| Stop | `CancelAgentSession` | existing |
| Retry | `RetryAgentSession` | disabled unless terminal |
| Switch model | Kill then `StartAgentSession { model, load_session_id }` | shown as the two-step flow |
| Downgrade to manual, Ask user | no Core fact | request entries, drawn disabled and labelled |
| sub-agent tree | deferred (`D-RAILNAV` ④) | not drawn |

## Bookkeeping Per Capability

- `viden-types`: types, reducer arms (`plan_documents`, `standing_approvals`,
  `AgentSessionKilled`), known-event arms, the new statuses in every
  exhaustive match.
- `viden-tools`: the `update_plan` built-in.
- `viden-permissions`: `Always` install and revoke, `Edit` re-evaluation,
  the `editable` list.
- `viden-config`: the standing-approval table and its file.
- `viden-runtime`: producers, gates, audit rows, the handoff into
  `start_agent_dag`, pause/resume/kill over the agent dispatch seam, the
  starter-Lane session start.
- `viden-agents`: kill without grace; resident session kept on pause.
- `viden-core`: capability constants, extension fixtures with refresh tests,
  manifest digests, count 29 → 33.
- Docs in both languages: `docs/core-0.3-compatibility.md` capability list
  and corpus; `docs/frontend-integration-contract.md` ownership rows and a
  "Plan Documents, Session Control, And Approval Scopes" section;
  `apps/gui/contract-requests.md` Core-side closes for 019 and 030;
  `scripts/tui-regression.sh` and `apps/tui/src/tui/client.rs` counts.
- Landing order: C13, then C14, then C15 (3 and 4 together). Each is its
  own branch and review; C12 lands before all three.

## Decided Defaults

The operator may override any of these before the batch that depends on it
is dispatched.

- The plan tool is named `update_plan` and is non-mutating; handoff is the
  only plan mutation and plan mode refuses it.
- Handoff produces a DAG, not Lanes; the DAG's tasks become Lanes through
  the paths D13 already has.
- Pause cancels the in-flight prompt and denies its pending approval; it
  does not try to freeze an adapter mid-tool-call, because no adapter
  offers that.
- Kill has no grace and is audited; Cancel keeps its grace.
- Always is never offered for high-risk requests or in plan mode, and ships
  with its revocation.
- Edit is offered only for flat text inputs, never patch bodies.
- A starter Lane is created even when its session start is refused.
- Bounds: 64 plan steps, 16 KiB per section, 4 KiB goal; held inputs while
  paused are bounded by the existing input queue bound.
