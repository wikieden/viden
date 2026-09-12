# Viden 0.3.4 Core 契约增量 - 工作区身份、回合生命周期、持久工作证据、转录行、工作区文件读取

English version: [release-0.3.4-contract-design.md](release-0.3.4-contract-design.md)

本文是 [release-0.3.4-plan.zh-CN.md](release-0.3.4-plan.zh-CN.md) 作为批次 C5–C9 派发的可加
能力的设计。它是设计，不是对已实现行为的描述。这里的字段名与边界是实现的简报；批次只能在
其报告中记录理由后偏离，偏离若通过评审则回写本文。

目标：Core `0.3.7`，schema `1`，`compatibility = additive_capability_gated`。
约束规则：`docs/frontend-integration-contract.md` 的协议演进规则与命令归属表，
`docs/core-0.3-compatibility.md` 的扩展流程，以及 `release-0.3.3-contract-design.zh-CN.md`
的"共同规则"（原样适用：仅可加；每个新事件在同一提交内可知并带 fixture；`command_id` 关联与
`CommandRejected`；效果前先在同一 `ToolSpec` 词汇上过权限；变更前先审计；有界且诚实；显式
目标；能力门控且九个基础 fixture 逐字节不变）。

2026-09-12 依据 `main` @ `25072a0a` 的阅读写成。以下引用的事实均在该提交核实。

## 为什么是这六项

`0.3.3` 的真实任务停在四个不存在的 Core 事实上：

| 缺失事实 | 停在哪里 | 能力 |
| --- | --- | --- |
| 工作区目标的操作者身份 | 无 Lane 时提交与推送被拒（到处都是 `RuntimeOwner::default()`，`crates/types/src/protocol.rs:64`） | 1 `runtime.workspace_owner` |
| 内建回合的终止事实 | 原生路径的 `assistant_stream` 永不 settle（`crates/types/src/runtime.rs:1204`）；会话队列只入不出（`runtime_contract.rs:640`，没有 `InputDequeued` 生产者） | 3 `runtime.turn_lifecycle` |
| 归档看到监督器驱动的工作 | `WorkspaceChangeUpdated` 不持久（`session_lifecycle.rs:1292`）；ACP 补丁证据以 `canonical: None` 构造（`crates/agents/src/glue.rs:1295`、`:1360`）；`RespondToApproval` 从不写审计行（`runtime_supervisor.rs`） | 4 `runtime.durable_work_evidence` |
| owner 作用域的有序转录 | GUI-CORE-009；基础的 `runtime.transcript_page` 无作用域且仅单会话 | 5 `runtime.transcript_rows` |

驾驶舱拉齐还需要两项：一个客户端可见的布局偏好——它不能挂在 `UiPreferences` 上，否则会移动
所有基础 fixture 摘要（`ResolvedUiPreferences` 总是序列化进 `RuntimeViewState`，
`crates/types/src/runtime.rs:1029`）；以及 Files tab 与面板文件行背后的单文件读取。

能力计数：23 → 29（计划写的是 28；布局偏好是第六项，依计划自己的门禁"若任一基础摘要移动，
则改为独立偏好记录"）。`0.3.4` 计划按此计数修订。

## 1. `runtime.workspace_owner`（C5，关闭 GUI-CORE-027）

### 类型

```rust
// crates/types/src/workspace_owner.rs（新建）

/// Core 为作用于工作区根而非某个 Lane 的工作发布的 owner。
/// `lane_id`、`session_id`、`task_id`、`turn_id` 为 `None`；`workspace_id` 与
/// `project_id` 绝不为空。
#[derive(Serialize, Deserialize)]
pub struct WorkspaceRuntimeOwnerBinding {
    pub canonical_root: String,
    pub owner: RuntimeOwner,
    /// `project_id` 的来源：从 `.viden/project.toml` 读出，或本次打开时铸造并写入。
    pub project_id_origin: ProjectIdOrigin,
}

#[non_exhaustive]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectIdOrigin { Existing, Minted }
```

事件、命令、视图状态：

- `RuntimeEventKind::WorkspaceRuntimeOwnerBound { binding: WorkspaceRuntimeOwnerBinding }`
  每次打开发布一次，作为 `runtime_state_events` 中紧随 `SnapshotUpdated` 之后的第一个事实，
  使快照回放携带它；工作区重新绑定时再次发布。
- `RuntimeViewState.workspace_owner: Option<RuntimeOwner>`，带
  `#[serde(default, skip_serializing_if = "Option::is_none")]`。缺失表示 Core 未发布；九个
  基础 fixture 从不携带它，因此字节不动。
- `RuntimeEventKind::LaneSourceUpdated { lane_id, source: WorkspaceSourceView }` 与
  `RuntimeViewState.lane_sources: BTreeMap<AgentLaneId, WorkspaceSourceView>`，带
  `skip_serializing_if = "BTreeMap::is_empty"`。`WorkspaceSourceUpdated` 含义不变（仅工作区根）。

### 语义

- **铸造。** `workspace_id` 为 `ws_` 加规范根路径 SHA-256 的前 16 位十六进制：这台机器上该
  位置的稳定身份，无需存储即可推导，并如实声明（移动仓库会改变它）。`project_id` 从
  `.viden/project.toml`（`[project] id = "prj_<ulid>"`）读取；缺失时 Core 铸造并在打开时写入
  该文件。`.viden/` 已由 Core 拥有且从不提交。`LocalCoreHost::open_workspace`
  （`crates/core/src/host.rs:166`）完成两者，`WorkspaceBinding` 增加这两个 id。
- **owner 唯一来源。** `SessionEngine::workspace_owner()` 返回已绑定的 owner；今天所有为
  **新**绑定从 `RuntimeOwner::default()` 构造 owner 的地方（`crates/lanes/src/lane_supervisor.rs:1114`
  的 Lane 绑定、`runtime_supervisor.rs:584` 的 Agent 会话 owner）改为从它出发，因此 Lane owner
  携带工作区与项目 id。既有的空 owner 事实（`runtime_loop.rs:82` 内建回合的工具事实）由 C7
  重新盖章，不在本项。
- **工作区目标的操作者 git。** 命令的 `owner` 等于已发布的工作区 owner 时，
  `SourceTarget::Workspace` 的 `RunOperatorGitAction` 被接受（`project_runtime.rs:222` 既有的
  信封执行者相等性检查，在客户端发送该 owner 后即完成此事）。审计记录以该 owner 命名。两个
  客户端删除各自的本地拒绝（`D1-OPERATOR-GIT-OWNER`、TUI 的 `/git` 拒绝）。
- **按 Lane 的源状态。** `LaneSourceUpdated` 用既有的 `sample_workspace_source` 对 Lane 工作树
  采样：Lane 工作树创建时、每次目标为该 Lane 的操作者 git 动作之后、以及每个快照前缀对每个有
  工作树的存活 Lane。没有自己工作树的 Lane 没有行（它就是工作区）。

### Fixture `workspace-owner.json`

打开 → `WorkspaceRuntimeOwnerBound`（项目 id 已铸造）→ 创建一个 Lane，其绑定携带相同的工作区
与项目 id → `RunOperatorGitAction { target: Workspace, action: Commit }` 在工作区 owner 下被
接受并审计 → 一次 Lane 目标的 `Stage` 之后该 Lane 的 `LaneSourceUpdated`。摘要测试证明新字段
缺失时九个基础 fixture 逐字节一致。

### 客户端

GUI：`workspace_owner` 存在时工作区目标的提交栏与 sync chip 启用；Lane tab strip 与上下文坞
从 `lane_sources[lane]` 读取 Lane 的分支与领先/落后；没有该能力时两者保留今天的拒绝文案。
TUI：同条件下 `/git` 工作区行启用。

## 2. `ui.layout_preferences`（C5，驾驶舱布局）

### 类型

```rust
// crates/types/src/ui_preferences.rs（新增）

#[derive(Serialize, Deserialize, Default)]
pub struct UiLayoutPreferences {
    #[serde(default)]
    pub lane_sidebar_mode: LaneSidebarMode,
    /// 操作者隐藏的环境态状态栏段；身份段与可操作段绝不列在这里。
    #[serde(default)]
    pub hidden_statusbar_segments: Vec<String>, // 有界 MAX_HIDDEN_STATUSBAR_SEGMENTS = 16
}

#[non_exhaustive]
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LaneSidebarMode { #[default] Pinned, Floating } // 2026-09-12 修订，见「实现修订」

pub struct UiLayoutPreferencePatch {
    pub lane_sidebar_mode: Option<LaneSidebarMode>,
    pub hidden_statusbar_segments: Option<Vec<String>>,
}
```

- 命令：`SetUiLayoutPreferences { command_id, patch }` 与 `ResetUiLayoutPreferences { command_id }`。
  事件：`UiLayoutPreferencesUpdated { command_id: Option<String>, preferences, persisted: bool }`
  （快照前缀上 `command_id` 缺失）。
- 视图状态：`RuntimeViewState.layout_preferences: Option<UiLayoutPreferences>`，带
  `skip_serializing_if = "Option::is_none"`。这正是独立记录的意义：`ResolvedUiPreferences`
  不动，基础摘要不动。
- 持久化：同一配置文件中 `[ui]` 旁的 `[ui.layout]` 表，走既有的 `apply_patch_to_table` 模式
  （`crates/config/src/ui_preferences.rs:186`）；文件写不了时 `persisted: false` 并在诊断中给
  原因，与 `ui.preference_persistence` 一致。

### Fixture `ui-layout-preferences.json`

设为浮动 → 已更新且已持久化 → 重置 → 默认固定（2026-09-12 修订，见「实现修订」：默认值是
floating，因此实际落地的序列是「floating 默认 → 设为 pinned → 重置回 floating」）；
`hidden_statusbar_segments` 中未知的段名原样保留（Core 不知道客户端的段词汇），但列表有界，
超界即拒绝。

## 3. `runtime.turn_lifecycle`（C6）

### 类型

```rust
// crates/types/src/turn_lifecycle.rs（新建）

#[derive(Serialize, Deserialize)]
pub struct TurnView {
    pub turn_id: String,
    pub owner: RuntimeOwner,          // 会话级输入框的 lane_id 为 None
    pub source: TurnSource,
    pub started_at: u64,
}

#[non_exhaustive]
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnSource {
    UserInput,
    QueuedInput { input_id: String },
    AgentSession { session_id: SessionId },
}

#[non_exhaustive]
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnOutcome {
    Completed,
    Failed { reason: String },      // 已清洗，有界 500
    Cancelled,
}
```

事件：`TurnStarted { turn: TurnView }`、`TurnFinished { turn_id, owner, outcome: TurnOutcome, finished_at }`。
视图状态：`RuntimeViewState.active_turns: Vec<TurnView>`，带 `skip_serializing_if = "Vec::is_empty"`；
`TurnStarted` 按 `turn_id` upsert，`TurnFinished` 移除。

### 语义

- **原生路径。** `SubmitUserInput`（`runtime_contract.rs:606`）在 `CommandAccepted` 之后立刻
  发布 `TurnStarted`，`owner` 取 C5 的工作区 owner（C5 缺失时为空 owner）并配新的 `turn_id`；
  在每条退出路径上以 `TurnFinished` 作为回合的最后一个事实：成功为 `Completed`，引擎错误路径上
  在既有 `Error` 事件旁给 `Failed { reason }`，`CancelActiveTurn` 停止它时为 `Cancelled`。
  尾部的 `SnapshotUpdated` 保留；它不再是客户端用来判断结束的信号。
- **ACP 路径。** 一个回合即一次 Agent 会话运行：会话为某个提示进入 `Running` 时 `TurnStarted`，
  在终止的 `AgentSessionCompleted`/`Failed`/`Updated{Cancelled}`（`glue.rs:807`）旁 `TurnFinished`，
  owner 为会话 owner。Agent 会话的其他一切不变。
- **流的 settle。** reducer 在 owner `lane_id: None` 的 `TurnFinished` 上清空 `assistant_stream`；
  Agent 会话事实上既有的 `settles_turn` 清空（`runtime.rs:1225`）保留。
- **队列排空。** 会话级 owner 的 `TurnFinished { outcome: Completed }` 时，Core 弹出最旧的
  `queued_runtime_inputs`，发布 `InputDequeued { input_id }`，并以
  `TurnSource::QueuedInput { input_id }` 作为新回合运行，重复直到队列为空或某个回合未完成。
  `Failed` 或 `Cancelled` 时队列保留并由快照前缀重新列出；失败的回合之后什么都不会跑。
  Lane worker 自己的队列（`lane_worker.rs:959`）不变。
- **崩溃安全。** 回合绝不跨重启恢复：快照前缀只重发 Core 此刻正在运行的回合，因此重连的客户端
  看到空的 `active_turns` 与一个可检视的队列。

### Fixture `turn-lifecycle.json`

提交 → `TurnStarted { UserInput }` → 增量 → `TurnFinished { Completed }` 且随后
`assistant_stream` 为空 → 第二个回合运行时两次 `QueueFollowUp` → `TurnFinished` → 第一条的
`InputDequeued` + `TurnStarted { QueuedInput }`，然后第二条 → 被取消的第三个回合让之后排队的
输入仍然排队。

### 客户端

GUI：输入框忙 = 其 owner 在 `active_turns` 中有条目（替换 `turn_id` 启发式），仅当
`queued_inputs` 非空且有回合活跃时显示"已排队"文案，失败后显示"已排队 · 等待下一个完成的回合"。
TUI：`native_turn` 窗口由 `active_turns` 替换；`docs/release-tui-0.3.4-source-control-parity.zh-CN.md`
中的残留陈述关闭。

## 4. `runtime.durable_work_evidence`（C7，关闭 GUI-CORE-028 与 E1 缺陷 4）

### 形状

无新类型。变化在于哪些事实进入归档与审计日志，以及对既有事件的一处新用法：

- 已应用的 `write_file`/`edit_file`（`record_completed_tool`，`runtime_loop.rs:82`）在实时的
  `WorkspaceChangeUpdated` 旁产生 `EvidenceRecorded { evidence }`：`kind: "patch"`、
  `id: "patch-<tool_call_id>"`、`owner` 为回合 owner、`timestamp`、`path`、`summary`，以及
  `canonical: Some(CanonicalEvidenceReference)`——其字节是经 `ContextEngine::store` 存入的统一
  diff（`ContextPutRequest { scope: owner 的证据作用域, kind: Patch, content, evidence_id }`），
  `source_hash` 为这些字节的 SHA-256，`producer { identity: "native", role: "coder",
  task_id: <turn_id> }`，`permission_snapshot_id` 在变更经审批时为审批的 audit id，
  `verification: Verified`，`quality` 按存储报告。超过 64 KiB 驾驶舱上限的 diff 仍完整存储至
  `MAX_EVIDENCE_CONTENT_BYTES`；再超则以 `canonical: None` 记录并在摘要中说明。
- ACP 补丁事实（`glue.rs:1295`、`:1360`）构造时保持 `canonical: None`（agents crate 不拥有
  存储），运行时在摄取它时存入 `metadata` 中携带的字节，并发布既有的
  `EvidenceCanonicalized { evidence_id, canonical }`。
- 持久化：监督器把每个终止的回合批次经一个新入口 `SessionEngine::absorb_supervised_events`
  交给引擎，后者应用 `is_durable_runtime_domain_event`、记住、并经既有的
  `persist_workflow_runtime_projection_batch` 持久化。这是从未存在过的接线；
  `WorkspaceChangeUpdated` 与 `CheckRunUpdated` 仍仅实时，新的 `EvidenceRecorded` 不是。
- `RespondToApproval` 在 `ApprovalResolved` 之前追加一条 `AuditRecord`：`audit_id` 为请求已
  展示的预铸 id，`actor: Operator`，`action: "approval.<allow_once|allow_session|allow_repo|deny>"`，
  `objects: [审批请求 id, 工具调用 id]`，`outcome: Applied`，`args` 为作用域。监督器为此获得
  一个窄的审计追加句柄。
- 合并闸：原生 Lane 的归档 `patch` 行在 `producer.task_id` 与闸的任务匹配时满足要求 `patch`
  的闸；会话级回合的行永远不满足（它不命名任务），闸会说明是哪种情况。

### Fixture `durable-work-evidence.json`

审批一个 `edit_file` → `ApprovalResolved` → 审计行可经 `QueryAudit` 读到 →
`EvidenceRecorded { patch, canonical }` → `QueryEvidence` 返回它 → `ReadEvidenceContent`
以已校验字节回答 `Diff` → 一个 ACP 补丁事实随后跟着 `EvidenceCanonicalized`。

### 客户端

GUI：EvidenceView 显示该行；D14 显示该审批；D2/权限坞的"审计"链接可解析。TUI：证据检视器
详情现在有实时内容。

## 5. `runtime.transcript_rows`（C8，关闭 GUI-CORE-009）

### 类型

```rust
// crates/types/src/transcript_rows.rs（新建）

pub const MAX_TRANSCRIPT_ROWS_PAGE: u16 = 200;
pub const DEFAULT_TRANSCRIPT_ROWS_PAGE: u16 = 50;
pub const MAX_TRANSCRIPT_ROW_TEXT_BYTES: u32 = 8 * 1024;

pub struct TranscriptRowsQuery {
    pub owner: RuntimeOwner,          // 前缀作用域，与 EvidenceQuery 同一匹配器
    pub before: Option<String>,       // 上一页给出的不透明游标
    pub limit: Option<u16>,           // 钳制
}

pub struct OwnedTranscriptRow {
    pub id: String,
    pub owner: RuntimeOwner,          // 完整 owner，绝不放宽
    pub sequence: u64,
    pub timestamp: Option<u64>,
    pub content: TranscriptRowContent,
}

#[non_exhaustive]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TranscriptRowContent {
    User { text: String, truncated: bool },
    Assistant { text: String, truncated: bool },
    ToolCall { call: ToolCallView },
    ToolResult { tool_call_id: String, success: bool, summary: String, evidence_id: Option<String> },
    CheckRun { check: CheckRunView },
    Permission { request_id: String, decision: Option<ApprovalDecision>, audit_id: String },
}

pub struct TranscriptRowsPage { pub rows: Vec<OwnedTranscriptRow>, pub older: Option<String>, pub complete: bool }
```

命令与事件：`QueryTranscriptRows { command_id, query }` → `TranscriptRowsLoaded { command_id, page }`；
拒绝为 `CommandRejected`。不归约进 `RuntimeViewState`。

### 语义

- 行由 Core 已保存的持久事实推导：会话级 owner 用原生会话转录（`SessionStore`），Lane owner 用
  Agent 会话的对话与工具事实；按记录序号排序，页内最新在后，用 `before` 向前翻页。超界文本在
  字符边界截断并标 `truncated`；正文存在规范证据行时 `evidence_id` 命名它。
- owner 作用域双向 fail-closed，与 `EvidenceQuery.matches` 完全一致：owner 未知的行绝不回答
  作用域查询。基础的 `runtime.transcript_page` 不动。

### Fixture `transcript-rows.json`

两个 Lane 的交错回合与一个会话级回合；三次查询（每个 Lane、会话）证明没有行跨 owner；一处
翻页边界；一条命名其证据的被截断助手行。

### 客户端

GUI：D1 转录渲染有序的 `User`/`Assistant` 行并撤掉 `transcript_user`/`transcript_assistant`
不可用占位；工具块保留来自 `WorkspaceChangeView.diff` 的内联 diff。TUI：转录镜头对聚焦 Lane
读取同样的行。

## 6. `runtime.workspace_file_reads`（C9）

### 类型

```rust
// crates/types/src/workspace_files.rs（新增）

pub const MAX_WORKSPACE_FILE_BYTES: u32 = 1024 * 1024;
pub const DEFAULT_WORKSPACE_FILE_BYTES: u32 = 256 * 1024;

pub struct WorkspaceFileReadQuery {
    pub target: SourceTarget,
    pub path: String,                 // 项目相对路径；绝对、`..`、控制字符 → 拒绝而非钳制
    pub byte_limit: Option<u32>,      // 钳制 1..=MAX
}

pub struct WorkspaceFileContent {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub content: WorkspaceFileBody,
}

#[non_exhaustive]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceFileBody {
    Text { text: String, truncated: bool },
    Binary,
    Unavailable { reason: WorkspaceFileUnavailableReason },
}

#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceFileUnavailableReason { NotFound, Directory, Unreadable }
```

命令与事件：`ReadWorkspaceFile { command_id, query }` → `WorkspaceFileLoaded { command_id, file }`。
不归约进 `RuntimeViewState`。

### 语义

- 以注册表中真实的 `read_file` `ToolSpec` 门控（`self.tools.spec("read_file")`，操作者 git 的
  先例，`operator_git.rs:366`），输入 `path` 为解析后的绝对路径，经 `PermissionEngine::decide`
  非交互地判定：`Allow` 读取，`Ask` 与 `Deny` 为点名规则的 `CommandRejected`（`QueryWorkspaceDiff`
  的先例）。`read_file` 自身的 `resolve_path` 不做任何限定，因此上面的路径规则与
  `resolve_source_target_root` 才是把读取限制在目标内的东西。
- 前 8 KiB 含 NUL 字节或字节不是合法 UTF-8 时为 `Binary`；`Text` 在字符边界截断并标
  `truncated`；`sha256` 覆盖整个文件。解析到目标之外的符号链接为 `Unreadable`。

### Fixture `workspace-file-reads.json`

一次文本读取、一次截断读取、一个二进制、一个缺失路径、一个目录、一次 `..` 拒绝、一次拒绝规则的拒绝。

### 客户端

GUI：Files tab 内容与 Code tab，面板 `~` 行在 Code tab 中打开文件。TUI：无对等最小集（TUI 没有
登记的文件查看器）；如实记录。

## 逐能力簿记

- `viden-types`：类型、承载视图的事实的 reducer 分支（工作区 owner、Lane 源状态、布局偏好、
  活跃回合）、已知事件分支。
- `viden-runtime`：生产者、门控、审计记录、监督器到引擎的吸收路径（C7）、队列排空（C6）。
- `viden-core`：能力常量、`WorkspaceBinding` 的 id、带刷新测试的扩展 fixture、清单摘要。
- `viden-config`：`[ui.layout]`。
- 双语文档：`docs/core-0.3-compatibility.md` 能力列表与语料；`docs/frontend-integration-contract.md`
  归属行与新的"工作区身份、回合生命周期与持久证据"一节；`apps/gui/contract-requests.md` 关闭
  009、027、028；`scripts/tui-regression.sh` 与 `apps/tui/src/tui/client.rs` 的计数 23 → 29。
- 落地顺序：C5（1 与 2 一起），然后 C6，然后 C7 与 C9 并行，然后 C8。各自独立分支与评审。

## 已决默认

在依赖它的批次派发之前，操作者可覆盖任意一条。

- `workspace_id` 由规范根推导；`project_id` 为铸造的 ULID，持久化在 `.viden/project.toml`。
- 布局偏好是独立记录与独立能力，而不是 `UiPreferences` 的字段，因为后者会移动全部九个基础摘要。
- 会话队列只在 `Completed` 的回合之后排空；失败与取消的回合让它保持排队且可见。
- 原生补丁证据以回合作为其生产者任务；会话级回合的证据永远不满足 Lane 的合并闸。
- 文件读取以真实的 `read_file` 规格非交互地门控。
- 边界：每条转录行文本 8 KiB，每次文件读取默认 256 KiB、最大 1 MiB，隐藏状态栏段 16 个。

## 实现修订（2026-09-12）

由 C5 批次记录，遵循本契约设计自身的规则：批次只能在其报告中给出理由的前提下偏离；
若该偏离通过评审，则记录在此。上文正文保持原样，被修订的行已就地标注；本节是实际落地的内容。

- **`LaneSidebarMode` 的默认值是 `Floating`，而不是 `Pinned`。** 正文的
  `#[default] Pinned` 与 `docs/viden-design/Viden/docs/SPEC.md` 中的设计决策
  `D-SIDEBAR` 相抵触 —— 后者规定 `float` 为默认模式（hover 峰显，把水平空间让给转录），
  `pinned` 为备选。错的是这份设计，不是设计包；GUI 批次同样按 `D-SIDEBAR` 对齐。
  fixture 现在从 floating 默认值开始、设为 pinned、再重置回 floating，因此设置与重置
  落在不同模式上，且两者都不是 fixture 自身的默认值。
- **`SetUiLayoutPreferences { patch }` 与 `ResetUiLayoutPreferences` 不带
  `command_id` 字段。** 没有任何 `RuntimeCommand` 变体带它：id 位于
  `RuntimeCommandEnvelope` 上，由作答事件回显，这与 `QueryWorkspaceDiff` →
  `WorkspaceDiffLoaded` 的既有做法一致。设计所要求的关联关系没有改变。
- **`UiLayoutPreferencesUpdated` 增加了 `diagnostics: Vec<UiPreferenceDiagnostic>`。**
  设计说 `persisted: false` 要「在诊断中给原因」，却没有给该事件承载诊断的字段。
  该字段可选，为空即跳过。
- **新增 `MAX_HIDDEN_STATUSBAR_SEGMENT_BYTES = 64`。** 设计只约束列表长度、不约束单个
  名字，这会让「有界」列表在字节上无界。空名字与控制字符基于同一理由被拒绝。
- **`project_id` 是 `prj_<纳秒 token>`，不是 ULID。** 工作区没有 ULID 依赖，本批次也未
  获授权新增；`fresh_id` 是既有辅助函数。前缀与形状与设计一致。
- **铸造逻辑位于 `viden-runtime`，由 `LocalCoreHost::open_workspace` 调用。**
  workspace id 的摘要需要 `sha2` —— 它是 `viden-runtime` 的正式依赖，而对 `viden-core`
  只是 dev-dependency。host 仍在 open 时完成这两步。
- **重置外观档案会保留 `[ui.layout]`。** 设计把布局表放在 `[ui]` 之下，而外观重置原本
  整表删除；两条记录对应两条不同的命令，操作者重置主题时不该发现驾驶舱被重排。
- **Lane 目标的操作者 git 动作发布 `LaneSourceUpdated`，而不是 `WorkspaceSourceUpdated`。**
  这是设计自身「`WorkspaceSourceUpdated` 保持其原义（仅指工作区根目录）」所要求的；
  原先的行为把一棵树的分支与 ahead/behind 放进了另一棵树的芯片。
- **工作区目标的授权比较的是 workspace 与 project 两个 id，而不是整个 owner。**
  Lane 作用域的操作者对工作区根目录动手是真实场景，其审计记录应当指名它来自哪个 Lane。
  被拒绝的是**谁也没指名**工作区的 owner —— 即 GUI-CORE-027 所针对的
  `RuntimeOwner::default()` 情形 —— 或指名了另一个工作区的 owner。
- **「在 Lane worktree 创建时采样」由 Lane 事件汇实现，每个 Lane 一次**，发生在第一条
  宣告已存在 worktree 的 `LaneUpdated` 上。Lane 创建被派发给异步 Lane worker，派发时
  worktree 尚不存在；而在每条 `LaneUpdated` 上采样会把若干次有界 `git` 调用放到该
  worker 的热路径上。
- **「每次快照前缀为每个活跃 Lane 采样」由状态生命周期采样实现** —— 连接时、每次快照
  请求时、每条已完成的受监督命令之后，也就是工作区 source 本来就被采样的位置 —— 而不是
  在 `runtime_state_events` 中。后者对每一条命令都会重建，在那里为 N 个 Lane 采样会让
  每条命令派生 5N 个 `git` 进程。快照信封仍然携带这些行，因为采样就发生在信封构建之前。

### C6 `runtime.turn_lifecycle`（评审通过，2026-09-12）

- **turn 的 owner 是命令自身的作用域加一个新 `turn_id`，而不是裸的工作区 owner。**
  受监督输入路径同样服务 Lane 的原生 turn（`lane_id: Some`）；在那里强制使用工作区
  owner 会让 Lane 自己的工作从它自己的 composer 中消失。会话作用域的 composer 得到的
  正是设计所述的 owner：已绑定时为 `workspace_owner()`，未绑定时为空 owner，绝不臆造。
- **会话队列的排空由「已完成的 turn」*武装*，而不是只在完成瞬间触发。** 监督者是单一
  worker，turn 运行期间发来的 `QueueFollowUp` 在 turn 结束时仍在其通道里；字面意义的
  「在 `TurnFinished { completed }` 时排空」会让最常见的情形永远排队。已完成的会话作用域
  turn 武装排空，武装期间到达的跟进立即运行，任何未完成的 turn 解除武装。失败或取消的
  turn 之后不会有任何东西运行。Lane 的原生 turn 既不武装也不解除。
- **两条 turn 事实都不持久化。** 进程在 `TurnFinished` 之前死亡时，持久化的 `TurnStarted`
  会在重放中变成 Core 无法取消的幽灵运行 turn。两者仅进实时汇，且被排除在
  `is_durable_runtime_domain_event` 之外；重启后 `active_turns` 为空。会话队列本身在内存中，
  能跨重连而不能跨重启（既有行为，未改）。
- **ACP 的 `TurnStarted` 与 `AgentSessionStarted` 一同发出。** ACP 启动路径上没有独立的
  「为 prompt 进入 `Running`」转换；`turn_id` 即 runner 既有的 artifact id。
- **被排空的 turn 触及上下文硬上限时以 `Error` 回答，而不是 `CommandRejected`。**
  它没有在途命令；拒绝原始命令 id 会把客户端已看到被接受的请求当作已结算。

### C9 `runtime.workspace_file_reads`（评审通过，2026-09-12）

- **`ReadWorkspaceFile { query }` 不携带 `command_id` 字段。** 与 C5 相同的约定：id 位于
  信封上，`WorkspaceFileLoaded { command_id, file }` 复述它。
- **`size` 与 `sha256` 为 `Option`，在 `Unavailable` 时省略。** 缺失的文件没有长度也没有
  摘要；`0` 与 `""` 会是臆造值，共享规则禁止。
- **权限门收到的是词法拼接的绝对路径，而非规范化路径。** 权限引擎的作用域检查是相对工作
  目录的词法比较，规范化路径会让任何自身路径经过符号链接的工作区被拒绝。包含性检查在
  权限门之后、读取任何字节之前，单独对规范化根目录进行。
- **查询路径会被规范化（去掉 `./` 与空段），应答回显规范化后的拼写。** `..` 在规范化之前
  就被拒绝，因此这绝不会把穿越路径合法化。
- **已知代价：** `sha256` 覆盖整个文件，以 64 KiB 分块流式计算，超大文件在内存上有界、在
  时间上无界。首次出现在 8 KiB 嗅探窗口之后的 NUL 不会被嗅探捕获。
