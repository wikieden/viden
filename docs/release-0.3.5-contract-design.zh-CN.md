# Viden 0.3.5 Core 契约增量 —— 计划文档、Agent 会话控制、审批范围、Starter-Lane Agent 绑定

English version: [release-0.3.5-contract-design.md](release-0.3.5-contract-design.md)

本文是 [release-0.3.5-plan.zh-CN.md](release-0.3.5-plan.zh-CN.md) 以批次 C13 至 C15 派发的
附加能力的设计，也是批次 C12 在不引入能力的前提下实现的加固规则的记录。它是设计，不是
已实现行为的描述。本文的字段名与上界是实现的 brief；批次只能在报告中记录理由后偏离，
偏离通过评审后再记入本文的修订段。

目标：Core `0.3.8`，schema `1`，`compatibility = additive_capability_gated`。
约束规则：`docs/frontend-integration-contract.md` 的协议演进规则与命令所有权表、
`docs/core-0.3-compatibility.md` 的扩展流程，以及 `release-0.3.3-contract-design.md` 的
共享规则，均原样适用（仅附加；每个新事件在同一提交中带 fixture；`command_id` 与
`CommandRejected` 关联；在同一 `ToolSpec` 词表上先权限后效果；先审计后变更；有界且诚实；
显式目标；能力门控且九个基线 fixture 字节不可变）。

写于 2026-09-13，基于对 `main` @ `75e2a84a` 的阅读（代码与 `0.3.4` 发布 `edcc7b3d` 完全
相同）。下文引用的事实均在该提交上核实。按 `AGENTS.md` 参考的架构：Codex 的
`update_plan` 工具（带 pending / in-progress / completed 状态的步骤）与先计划后执行的流程；
Claude Code 带显式退出审批的 plan mode；DeepSeek-TUI 建立在 `update_plan` 上的只读 Plan
模式。Viden 在 `0.3.0` 就已选定的一处分歧：计划是 Core 拥有、由操作者决定的持久事实，
绝不是客户端状态。

## 为什么是这四项

| 缺失的事实 | 停在哪里 | 能力 |
| --- | --- | --- |
| 操作者可读、可接受、可移交的计划 | plan 模式拒绝一切变更（`crates/permissions/src/lib.rs:192`，冻结的 `plan-denial` fixture），但规划者的输出只是记录文本；没有事实说明计划是什么、是否被接受、变成了什么工作 | 1 `runtime.plan_documents` |
| Agent 会话的暂停与终止 | `RuntimeCommand` 对会话只有 `StartAgentSession`、`SendAgentSessionInput`、`RetryAgentSession`、`CancelAgentSession`（`crates/types/src/runtime.rs:35`）；D10 把 Pause 与 Kill 置灰并如实标注（`apps/gui/README.md:1472-1473`） | 2 `runtime.agent_session_control` |
| Always 范围与 Edit 决定 | `ApprovalScope` 只有 `Once`、`Session`、`RepoAllowlist`（`crates/types/src/approval.rs:23`）；安装的规则全是 `PermissionRuleSource::Session`（`crates/permissions/src/lib.rs:266`）；引擎已有 `updated_input` 与 `user_modified`（`crates/types/src/lib.rs:800-811`）但没有决定变体承载修订（注册项 019） | 3 `runtime.approval_scopes` |
| D4 中操作者选的 agent 到达 Core | `StarterLaneRequest` 只有 `lane_id`、`preset`、`branch`、`worktree_path`（`crates/types/src/frontend_services.rs:221`）；选择无处可去，向导如实说明（注册项 030） | 4 `runtime.starter_lane_agent` |

能力计数：29 → 33。每项能力的新视图状态字段都以 `#[serde(default, skip_serializing_if = ...)]`
序列化，因此不含这些字段的视图编码为原来的字节；每个批次都以九个基线 fixture 与 `0.3.4`
扩展 fixture 证明这一点。

## 0. 加固规则（C12，无能力）

这些规则关闭兼容性后续项 15、16、14、10、13，记录在此以便契约本身而非仅批次 brief
陈述它们。

1. **两条审批路径都审计。** lane supervisor 拥有的审批与 runtime 拥有的审批审计方式完全
   一致：客户端在请求上看到的同一个预铸 `audit_id`、同样的 `approval.<decision>` 行、在
   宣告决定的事实之前持久写入。审计仍是 runtime 拥有的策略；`crates/lanes` 不新增审计
   写入者。
2. **队列在接受时确认。** 轮次进行中收到的 `QueueFollowUp` 在命令被接受时进入
   `queued_inputs` 并发出 `InputQueued`；`InputDequeued` 在 drain 真正启动该轮次时发出。
   轮次进行中两者绝不共享同一瞬间。
3. **归档 diff 指名其文件。** 由规范补丁字节构建的 `EvidenceContent::Diff` 带有条目自身的
   路径与 `WorkspaceChangeKind`，与决策上下文完全一致；原地修改的文件绝不发布为重命名。
4. **Agent 上报的补丁持久规范化。** 磁盘上的 artifact 带有与实时 `EvidenceCanonicalized`
   相同的 `canonical`。
5. **刷新 `interaction-closed-loop`**：从生成器刷新并在一个簿记提交中更新语料摘要；其他
   fixture 不动。

## 1. `runtime.plan_documents`（C13）

### 类型

```rust
// crates/types/src/plan.rs（新）

pub struct PlanDocumentId(pub String); // "plan_<ulid>"

#[serde(rename_all = "snake_case")]
pub enum PlanDocumentStatus {
    /// 规划者已开始撰写；尚未提出修订。
    Drafting,
    /// 一个修订等待操作者决定。
    Proposed,
    Accepted,
    Rejected,
    /// 已接受并转为工作；`handoff` 指名 DAG。
    HandedOff,
    /// 更新的修订替代了这个决定。
    Superseded,
}

#[serde(rename_all = "snake_case")]
pub enum PlanStepStatus { Pending, InProgress, Completed }

pub struct PlanStep {
    pub id: String,                  // 计划内稳定，"step_<n>"
    pub title: String,               // <= 256 字节
    pub status: PlanStepStatus,
    pub role: Option<AgentRole>,     // 移交时默认 Coder
    pub file_scope: Vec<String>,     // 工作区相对路径，<= 64 项
    pub dependencies: Vec<String>,   // 本计划内的步骤 id
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
    pub owner: RuntimeOwner,         // 会话或 Lane 范围，绝不是默认值
    pub title: String,
    pub goal: String,
    pub status: PlanDocumentStatus,
    pub revision: u32,               // 从 1 起；每次提议递增
    pub content_sha256: String,      // 覆盖 title、goal、sections、steps
    pub sections: Vec<PlanSection>,  // 每种至多一个
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

命令（`RuntimeCommand`，均按 owner 作用域）：

```rust
/// 操作者撰写或客户端转发的提议。模型绝不发送它；模型调用内置 `update_plan` 工具。
ProposePlanDocument { draft: PlanDocumentDraft },      // title、goal、sections、steps
AcceptPlanDocument  { plan_id, revision, content_sha256 },
RejectPlanDocument  { plan_id, revision, content_sha256, reason: String },
/// 把已接受的计划转为 Core 已拥有的 DAG。
HandOffPlanDocument { plan_id, revision, content_sha256 },
```

事件（`RuntimeEventKind`）：

```rust
PlanDocumentUpdated  { plan: PlanDocumentRecord },              // 每次修订与状态变化
PlanDocumentDecided  { plan_id, revision, status, audit_id, owner },
PlanDocumentHandedOff{ plan_id, revision, dag_id, audit_id, owner },
```

### `update_plan` 工具

内置的非变更工具（`ToolSpec { name: "update_plan", is_mutating: false }`），形状同 Codex：
`explanation: Option<String>`、`plan: [{ step, status }]`，并可选扩展 `title`、`goal`、
`sections`、`role`、`file_scope`、`dependencies`。一次调用在 owner 没有打开的计划时创建
计划，否则提议下一个修订。它在所有工作模式下可用，也是 plan 模式唯一允许其效果的工具，
因为其效果是关于计划的 Core 事实，而不是工作区、记忆或任务的变更。权限引擎把它视为安全
读取；它不为任何其他工具绕过门控。

### 语义

- 每个 owner 一个打开的计划：状态为 `Drafting`、`Proposed` 或 `Accepted` 时计划处于打开
  状态。在 `Proposed` 或 `Accepted` 时的新提议使 `revision` 递增、状态置为 `Proposed`，并
  通过在同一记录的历史中把先前修订发布为 `Superseded` 使先前决定失效（记录承载最新
  修订；JSONL 保留每个修订）。
- 决定是钉住的：`Accept` 与 `Reject` 携带操作者看到的 `revision` 与 `content_sha256`；不
  匹配则 `CommandRejected`，原因 `plan_revision_stale`。决定在 `PlanDocumentDecided` 之前
  写入审计行（`plan.accepted`、`plan.rejected`，actor Operator，object 为计划）。
- 移交是唯一的变更，且在 plan 模式下被拒绝：`PermissionMode::Plan` 下的
  `HandOffPlanDocument` 为 `CommandRejected`，原因 `plan_mode`，与 starter-Lane 预览使用同
  一原因族。已接受的计划等待操作者切换到 Build。成功时 Core 由步骤构建
  `AgentDagTaskSpec`（每步一个，`role` 默认 `Coder`，`objective` = 步骤标题，`dependencies`
  映射，`file_scope` 复制，`permission_policy` = Lane 的变更策略名），并在该命令 id 下走
  现有 `start_agent_dag` 路径，客户端依次看到 `AgentDagUpdated`（状态 `Active`）与
  `PlanDocumentHandedOff`，计划的 `handoff` 指名该 DAG。移交非 `Accepted` 的计划为
  `CommandRejected`。
- 规划者运行时输入的跟进照旧走 `QueueFollowUp`；C12 规则 2 落地后无需新增。
- 上界：64 步、七个 16 KiB 的段、256 字节标题、4 KiB goal。超界输入对 `update_plan` 是
  模型可见的工具错误，不是被截断的计划。
- 持久性：每个 `PlanDocumentUpdated` 都是 JSONL 事实；回放与恢复精确重建
  `plan_documents`。

### Fixture `plan-documents.json`

开启 plan 模式；`update_plan` 提议修订 1；第二次调用提议修订 2（事件中可见 `Superseded`
历史）；以修订 1 `Accept` 被拒 `plan_revision_stale`；`Accept` 修订 2 → 审计行 →
`PlanDocumentDecided`；plan 模式下 `HandOff` 被拒 `plan_mode`；`SetWorkMode Build`；
`HandOff` → `AgentDagUpdated`（Active，两个任务及映射的依赖）→ `PlanDocumentHandedOff`；
最终视图：一个 `HandedOff` 的计划带 `handoff.dag_id`，`agent_dags` 含该 DAG。

### 客户端

GUI G8 Plan View：段落、带状态的步骤、决定行（Accept、带理由的 Reject、Hand off）、修订
与过期决定的拒绝文案、进入 D13 的移交链接。TUI T4：带同样三个动作的计划透镜。两者都把
「未提议计划」显示为缺席，绝不显示空计划。

## 2. `runtime.agent_session_control`（C14）

### 类型

```rust
// crates/types/src/agent.rs，附加变体
pub enum AgentSessionStatus { Starting, Running, WaitingApproval, Completed, Failed, Cancelled,
    /// 存活，无在途 prompt，恢复前不转发输入。
    Paused,
    /// 无取消宽限地终止；终态，可像 Failed 一样重试。
    Killed,
}

#[serde(rename_all = "snake_case")]
pub enum AgentSessionTermination { Cancelled, Killed }

pub struct AgentSessionControlView {
    pub paused_at: Option<u64>,
    pub resumed_at: Option<u64>,
    pub terminated_by: Option<AgentSessionTermination>,
    /// 暂停期间持有的输入；恢复时按序转发。
    pub held_input_count: u32,
}

// AgentSessionView
#[serde(default, skip_serializing_if = "Option::is_none")]
pub control: Option<AgentSessionControlView>,
```

命令：`PauseAgentSession { session_id }`、`ResumeAgentSession { session_id }`、
`KillAgentSession { session_id }`。事件：`AgentSessionUpdated` 承载 `Paused` 与恢复后的
状态；`AgentSessionKilled { session }` 是新的终态事件，reducer 的终态分支
（`crates/types/src/runtime.rs:1371`）把它当作 `Completed`/`Failed` 处理。

### 语义

ACP 与 Codex app-server 都没有暂停动词；ACP 有 `session/cancel`
（`crates/agents/src/acp.rs:1991`），Core 的取消是它加上有界的进程终止兜底
（`crates/agents/src/codex.rs:566-657`）。因此暂停是 Core 侧作用于会话输入路径的状态：

- **Pause**（从 `Starting`、`Running`、`WaitingApproval`）：Core 以今天的宽限对在途 prompt
  发送适配器的取消，保留驻留会话及其 `load_session_id`，不再转发 `SendAgentSessionInput`；
  暂停期间发送的输入被接受（`AgentSessionInputAccepted`）并持有，计入
  `held_input_count`。会话正在等待的审批由 Core 以 `Deny` 解决，原因 `session_paused`，并
  审计，因为它所属的 prompt 已不存在。状态 `Paused`，设置 `paused_at`。审计行
  `agent_session.pause`。
- **Resume**（仅从 `Paused`）：若有持有的输入，转发第一个且状态为 `Running`；否则状态为
  `Completed`——在本契约中它已经意味着「无在途轮次、会话存活并接受输入」。设置
  `resumed_at`。审计行 `agent_session.resume`。
- **Kill**（从任何非终态，包括 `Paused`）：无取消宽限地立即终止进程，退役驻留会话，丢弃
  持有的输入并在 diagnostic 中指名，待决审批以原因 `session_killed` 拒绝并审计；状态
  `Killed`，`terminated_by: Killed`。`RetryAgentSession` 像接受 `Failed` 一样接受 `Killed`。
  审计行 `agent_session.kill`。
- **Cancel** 不变，并新增标记 `terminated_by: Cancelled`。
- **切换模型** 按设计是客户端对 Kill（或 Cancel）与带 `model` 及 `load_session_id` 的
  `StartAgentSession` 的组合，契约已支持；不新增命令。G9 以该两步流程呈现并如实说明。
- 针对不允许的状态发出的命令为 `CommandRejected`，原因 `session_status_<current>`；owner
  不持有的会话照旧拒绝。
- Lane 的 `active_session_ids` 保留暂停的会话；被终止的会话像被取消的一样离开。

### Fixture `agent-session-control.json`

Start → `Running` → 一个审批待决 → `Pause` → 审批以 `session_paused` 拒绝并有审计行 →
`Paused`，`held_input_count: 0` → 暂停中 `SendAgentSessionInput` → 接受，
`held_input_count: 1` → `Resume` → `Running`，持有的输入被转发 → `Kill` →
`AgentSessionKilled`，`terminated_by: Killed` → `Retry` → 同一逻辑会话下新的 `Starting`。
对运行中会话 `Resume` → `CommandRejected session_status_running`。

### 客户端

GUI G9 Agent Board 与 D10 动作行：Pause、Resume、Kill 生效；Stop 仍为 Cancel；切换模型为
两步流程。TUI T4：Lane 视图中的会话控制按键。

## 3. `runtime.approval_scopes`（C15，关闭 GUI-CORE-019）

### 类型

```rust
// crates/types/src/approval.rs，附加变体
pub enum ApprovalScope { Once, Session { .. }, RepoAllowlist { .. },
    /// 跨会话与仓库的常设规则，可撤销。
    Always,
}
pub enum ApprovalDecision { Allow { scope }, Deny,
    /// 操作者修订了输入；Core 重新评估修订。
    Edit { revised_input: ToolInput },
}

pub struct StandingApprovalView {
    pub rule_id: String,            // "rule_<ulid>"
    pub tool_name: String,
    pub rule_content: String,       // 与 Session 规则相同的编码
    pub granted_at: u64,
    pub granted_in_workspace: String, // workspace_id
    pub audit_id: String,
}

// ApprovalRequestView，附加
#[serde(default, skip_serializing_if = "Option::is_none")]
pub revised_from: Option<String>,   // 本修订所替代的请求
#[serde(default)]
pub editable: bool,

// RuntimeViewState
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub standing_approvals: Vec<StandingApprovalView>,
```

命令：`RevokeStandingApproval { rule_id }`。事件：
`StandingApprovalGranted { rule: StandingApprovalView }`、
`StandingApprovalRevoked { rule_id, audit_id, owner }`。`ApprovalResolved` 以不变的形状承载
新的决定变体。

### 语义

- **Always** 安装一条 `PermissionRuleSource::UserSettings` 允许规则，键与 `Session` 规则相同
  （工具加内容），持久化在 `viden-config` 拥有的用户级设置文件中（新的
  `[[permissions.standing]]` 表；批次指名文件）。它在每个工作区的引导时加载，因此
  `decide` 无需提示即回答 `RuleAllow`。仅当请求的 `risk` 不为 `High` 且模式不为 `Plan` 时
  提供（`allowed_scopes` 含 `Always`）；未提供时客户端发送它为 `CommandRejected`
  `scope_not_offered`。授予写 `approval.always` 与 `StandingApprovalGranted`；撤销写
  `approval.revoke` 并从文件与引擎移除规则。撤销路径与授予在同一批次，绝不延后。
- **Edit** 仅当 `editable` 为 true 时提供：Core 把输入是操作者可安全修订的扁平文本映射的
  内置工具（shell 命令与含路径输入）标为可编辑，含补丁的输入绝不可编辑。收到 `Edit` 时
  Core 运行 `decide(tool, revised_input)`：`Allow` → 工具以 `updated_input = revised`、
  `user_modified: true` 运行，原请求以决定 `Edit` 解决，审计行 `approval.edit` 含两个输入
  的有界预览；`Ask` → 原请求以 `Edit` 解决，随后新的 `ApprovalRequested`，其视图带
  `revised_from` 与新的预铸 `audit_id`；`Deny`（规则或 plan 模式）→ 原请求以引擎的原因
  `Deny` 解决。与原输入逐字节相同的修订为 `CommandRejected` `revision_unchanged`。
- 两条路径在两条审批分支上都审计（C12 规则 1）。

### Fixture `approval-scopes.json`

一个中风险 shell 请求，`allowed_scopes` 含 `Always` → `Allow Always` →
`StandingApprovalGranted`，审计行；第二个会话中相同工具与输入 → 以 `RuleAllow` 运行且无
`ApprovalRequested`；`RevokeStandingApproval` → 撤销，审计行；第三次请求重新出现。一个可
编辑请求 → `Edit` 为更窄的路径 → 带 `revised_from` 的新请求 → `Allow Once` → 工具以修订
输入运行（工具调用视图中可见 `user_modified: true`）；plan 模式下 `Edit` →
`Deny plan_mode`；高风险请求的 `allowed_scopes` 不含 `Always`，发送它被拒
`scope_not_offered`。

### 客户端

GUI：权限坞的 Always 与 Edit 生效，`Shift+A` 回到 Always，常设审批列表在设置中获得撤销
控件与 D14 链接。TUI：决定弹窗新增 `a`（Always）与 `e`（带内联字段的 Edit）。

## 4. `runtime.starter_lane_agent`（C15，关闭 GUI-CORE-030）

### 类型

```rust
pub struct StarterLaneAgentBinding {
    pub agent_id: String,
    pub model: Option<String>,
    pub task: Option<String>,        // 默认为预设的摘要
}

// StarterLaneRequest，附加
#[serde(default, skip_serializing_if = "Option::is_none")]
pub agent: Option<StarterLaneAgentBinding>,

// StarterLanePreview，附加
#[serde(default, skip_serializing_if = "Option::is_none")]
pub agent: Option<StarterLaneAgentBinding>,   // 回显；diagnostics 指名未知适配器

#[serde(rename_all = "snake_case")]
pub enum StarterLaneSessionOutcome {
    Started { session_id: SessionId },
    Refused { reason: String },
}

// StarterLaneReceipt，附加
#[serde(default, skip_serializing_if = "Option::is_none")]
pub session: Option<StarterLaneSessionOutcome>,
```

### 语义

- 预览的 `content_sha256` 覆盖绑定，因此更改选择像其他字段一样以 `RequestChanged` 使预览
  失效。指名 `agent_adapters` 中不存在的适配器的绑定被回显并附 diagnostic；预览仍可创建，
  回执会说明会话被拒绝。
- 带绑定的 `CreateStarterLane`：Lane 的创建与回执写入与今天完全一致；随后 Core 通过
  `StartAgentSession` 所走的同一路径、在同样的 owner 与 lane 绑定检查下
  （`crates/runtime/src/runtime_supervisor.rs:974-1040`）启动会话，并把结果盖在回执上。
  Lane 创建绝不因会话未启动而失败；回执的 `session: Refused { reason }` 是诚实的回答，
  客户端仍可自行发送 `StartAgentSession`。
- 两步流程仍然有效；不带 `agent` 的请求逐字节保持原行为。

### Fixture `starter-lane-agent.json`

带绑定的预览 → 回显；创建 → 回执 `session: Started` → Lane owner 下的
`AgentSessionUpdated Starting`；第二个带未知 `agent_id` 的预览 → diagnostic → 创建 → 回执
`session: Refused` 且 Lane 存在。

### 客户端

GUI G9：D4 第 2 步发送绑定，第 4 步显示回执结果。TUI T4：`/lanes` 创建接受可选 agent。

## 设计包输入（供批次 DS）

Plan Studio 与 Agent Board 都没有登记的 D 屏。设计批次按包自身规则登记两者；下表是每个
控件映射的事实，以便没有任何控件在无事实或无明确请求的情况下被绘制。

Plan Studio（中央面板视图，保留 chrome，从 rail 到达）：

| 元素 | 事实或命令 | 缺席状态 |
| --- | --- | --- |
| 模式徽标 | `WorkMode`、`PermissionMode::Plan` | 从不缺席 |
| 段落 | `PlanDocumentRecord.sections` | 「未提议计划」 |
| 带状态的步骤 | `PlanDocumentRecord.steps` | 同上 |
| 修订与作者 | `revision`、`proposed_by` | 同上 |
| Accept、带理由的 Reject | `AcceptPlanDocument`、`RejectPlanDocument` | 非 `Proposed` 时置灰 |
| Hand off | `HandOffPlanDocument` | 非 `Accepted` 时置灰；模式为 Plan 时标注「plan 模式下拒绝」 |
| 移交链接 | `handoff.dag_id` → D13 | 移交前缺席 |
| 输入框与队列 | 现有输入框、`queued_inputs` | 现有 |

Agent Board（覆盖 Lane/会话层级的中央面板视图）：

| 元素 | 事实或命令 | 缺席状态 |
| --- | --- | --- |
| Lane 行及其下的会话 | `lanes`、按 `lane_id` 的 `agent_sessions` | 「无 Lane」 |
| 角色、任务、状态、diagnostic | `AgentSessionView` | 按字段 |
| 事件时间线 | 会话范围的 `runtime.audit` 行、`agent_conversation` | 「无行」 |
| 证据 | 按 owner 的 `latest_evidence` | 「未发布」 |
| Pause、Resume、Kill | C14 命令 | 置灰并附状态原因 |
| Stop | `CancelAgentSession` | 现有 |
| Retry | `RetryAgentSession` | 非终态时置灰 |
| 切换模型 | Kill 后 `StartAgentSession { model, load_session_id }` | 以两步流程呈现 |
| 降级为手动、询问用户 | 无 Core 事实 | 请求条目，置灰并标注 |
| 子代理树 | 延期（`D-RAILNAV` ④） | 不绘制 |

## 各能力的簿记

- `viden-types`：类型、reducer 分支（`plan_documents`、`standing_approvals`、
  `AgentSessionKilled`）、已知事件分支、每个穷举匹配中的新状态。
- `viden-tools`：内置 `update_plan`。
- `viden-permissions`：`Always` 的安装与撤销、`Edit` 的重新评估、`editable` 列表。
- `viden-config`：常设审批表及其文件。
- `viden-runtime`：生产者、门控、审计行、进入 `start_agent_dag` 的移交、agent dispatch 接缝
  上的 pause/resume/kill、starter-Lane 的会话启动。
- `viden-agents`：无宽限的 kill；暂停时保留驻留会话。
- `viden-core`：能力常量、带刷新测试的扩展 fixture、清单摘要、计数 29 → 33。
- 双语文档：`docs/core-0.3-compatibility.md` 能力列表与语料；
  `docs/frontend-integration-contract.md` 所有权行与「计划文档、会话控制与审批范围」一节；
  `apps/gui/contract-requests.md` 对 019 与 030 的 Core 侧关闭；`scripts/tui-regression.sh`
  与 `apps/tui/src/tui/client.rs` 的计数。
- 落地顺序：C13，然后 C14，然后 C15（3 与 4 合并）。每个都是独立分支与评审；C12 在三者
  之前落地。

## 已定默认值

操作者可在依赖它的批次派发前覆盖其中任何一项。

- 计划工具命名为 `update_plan` 且非变更；移交是唯一的计划变更，plan 模式拒绝它。
- 移交产出 DAG 而非 Lane；DAG 的任务通过 D13 已有的路径成为 Lane。
- 暂停取消在途 prompt 并拒绝其待决审批；不试图在工具调用中途冻结适配器，因为没有适配器
  提供这种能力。
- Kill 无宽限且被审计；Cancel 保留宽限。
- Always 绝不对高风险请求或在 plan 模式下提供，并随撤销一起交付。
- Edit 仅对扁平文本输入提供，绝不对补丁正文提供。
- 即使会话启动被拒，starter Lane 仍被创建。
- 上界：64 个计划步骤、每段 16 KiB、4 KiB goal；暂停期间持有的输入受现有输入队列上界
  约束。
