# Viden 0.3.3 Core 契约增量 - DiffReview 宿主与证据读取

English version: [release-0.3.3-contract-design.md](release-0.3.3-contract-design.md)

本文是 [release-0.3.3-plan.zh-CN.md](release-0.3.3-plan.zh-CN.md) 以批次
C1 至 C4 派发的四个附加 capability 的设计。它是设计，不是对已实现行为的
描述。这里的字段名与边界是实现的 brief；批次只能在其报告里写明原因后偏离，
偏离若经评审存活，则回写到本文。

目标：Core `0.3.6`，schema `1`，`compatibility = additive_capability_gated`。
约束规则：`docs/frontend-integration-contract.md` 的协议演进规则与命令归属
表、`docs/core-0.3-compatibility.md` 的扩展流程、设计包中 `DiffReview` 与
`EvidenceView` 的登记（`SPEC.md` 决策 `D-RAILNAV`，`DESIGN-REF.md`
"D1 次级视图" 一节）。

## 为什么是一次增量

GUI-CORE-020、012、015 是分别延后的，但它们共用一个面——已登记的
`DiffReview` 视图——和一个底料：Core 已经在四处生产或消费的统一 diff 文本。

| 站点 | 现状 | 消费者 |
| --- | --- | --- |
| 工具结果 diff | `WorkspaceChangeView.patch: Option<String>`（64 KiB 上限） | D1 变更文件芯片 |
| 审批预览 | `ApprovalRequestView.input_preview: String` | D1 权限 dock、D2 |
| merge apply | `LocalPatchBackend::prepare` 内部解析文件与 hunk | trust loop |
| lane apply | `LaneEffectRequest::Apply { unified_diff }` | lane runtime |

三条请求分开设计会得到三种 diff 形状和两条 git 动作路径。本次增量加一个
类型化的 `DiffDocument`，挂到上述站点，加一条复用 agent tool spec 的操作者
动作路径，再加一个用同一批行构造的冲突形状。证据读取是第四个 capability，
因为 `EvidenceView` 设计需要分页和行背后的内容，而最近窗口的
`latest_evidence` 投影给不了。

## 共同规则

下面每个 capability 都遵守这些规则；批次报告逐条声明已验证。

- **只做附加。** schema 保持 `1`。新字段带 `#[serde(default)]`，Option 带
  `skip_serializing_if`，没有该字段的记录序列化出的字节与之前相同。新枚举
  `#[non_exhaustive]`。
- **每个新事件都是已知事件。** `is_known_runtime_event_type` 分支、事实进入
  `RuntimeViewState` 处的 reducer 分支、扩展 fixture 在同一提交落地。此前的
  事件有三次漏掉；fixture 测试是守卫。
- **关联，不推断。** 读或动作命令携带客户端自选的 `command_id`；应答事件
  原样回带。效果前的拒绝是 `CommandRejected { command_id, reason }`，可操作
  的提示折进 `reason`，绝不是空页也绝不是裸 `Error`
  （`crates/runtime/src/frontend_services.rs` 中 `QueryWorkspaceFiles` 的先例）。
- **权限先于效果，同一词表。** 操作者动作或 workspace 读取用 agent 回合会
  用的同一个 `ToolSpec` 过 `PermissionEngine::decide`，于是一套 `viden.toml`
  allow/ask/deny 规则同时约束 agent 与操作者。`Ask` 以 `ApprovalRequested`
  进入 owner 作用域的 supervisor 审批队列；`Deny` 与 plan mode 在任何进程
  spawn 之前拒绝。
- **审计先于变更。** 每个变更型动作在效果之前追加审计记录，与 trust loop
  相同，完成事件携带 `audit_id`。
- **有界且诚实。** 每个载荷按字节和行数限界，带 `truncated` 或 `omitted`
  标志。`None` 表示 Core 不知道；绝不是默认值，也绝不是推断值。
- **显式目标。** 动作与读取指明目标：workspace 根或某一个 Lane worktree。
  Core 自行校验 Lane 存在、未归档并解析其 worktree 路径；客户端从不传路径。
- **capability 门控。** 每个 capability 追加到
  `FRONTEND_V1_EXTENSION_CAPABILITIES` 与
  `crates/core/frontend-contract-extensions.toml`，带其 fixture 与 view 摘要。
  九个冻结基础 fixture 字节不可变，`scripts/tui-regression.sh` 的 capability
  计数门从 19 改为 23。

## 1. `runtime.structured_diff`（C1，关闭 GUI-CORE-012）

底料；最先落地。

### 类型

新模块 `crates/types/src/diff.rs`：

```rust
pub struct DiffDocument {
    pub files: Vec<DiffFile>,
    pub truncated: bool,
    pub byte_limit: u32,
}

pub struct DiffFile {
    pub path: String,
    pub old_path: Option<String>,
    pub kind: WorkspaceChangeKind,
    pub binary: bool,
    /// 文件属于变更集，但其 hunk 被字节上限丢弃。`additions` 与
    /// `deletions` 仍是真实值。
    pub omitted: bool,
    pub additions: u32,
    pub deletions: u32,
    pub hunks: Vec<DiffHunk>,
}

pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub header: Option<String>,
    pub lines: Vec<DiffLine>,
}

pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}

#[non_exhaustive]
pub enum DiffLineKind { Context, Added, Removed }
```

`WorkspaceChangeKind` 是 `crates/types/src/frontend_services.rs` 中已有的
封闭枚举。Core 是唯一生产者：`crates/tools/src/patch.rs` 里已有的统一 diff
解析器（文件头、hunk 头、旧行与新行）提升为产出 `DiffDocument`，客户端从不
把文本解析成行。

### 挂载站点

1. **审批决策上下文。** `ApprovalRequestView` 新增
   `decision_context: Option<DecisionContext>`：

   ```rust
   pub struct DecisionContext {
       pub diff: Option<DiffDocument>,
       /// diff 所依据的文件内容的 SHA-256，仅当 diff 来自一次拟议的
       /// 单文件变更、且 Core 能读到该内容时存在
       /// （2026-09-09 修订，见"实现修订"）。
       pub base_sha256: Option<String>,
   }
   ```

   对 `edit_file` 与 `write_file` 审批：只读读取当前文件，在内存中套用拟议
   替换，然后 diff；审批时刻不发生任何变更。对 `MergeAgentPatch` 的 trust
   loop 审批：从规范补丁字节生成，这就是 GUI-CORE-012 要求的多文件情形。
   明示的局限：预览在审批时刻计算，执行运行的是拟议的工具入参；期间被改动
   的文件会产生不同结果，`base_sha256` 让客户端或审计读者事后可察觉。执行
   前复核不在 `0.3.3` 范围。

2. **workspace 变更事实。** `WorkspaceChangeView` 在现有 `patch` 字符串旁
   新增 `diff: Option<DiffDocument>`，受同一 `MAX_COCKPIT_PATCH_BYTES` 上限。
   `patch` 为基础客户端保留。

3. **操作者 diff 读取。**

   ```rust
   RuntimeCommand::QueryWorkspaceDiff { command_id: String, query: WorkspaceDiffQuery }

   pub struct WorkspaceDiffQuery {
       pub target: SourceTarget,
       pub scope: WorkspaceDiffScope,
       /// 空表示全部已变更路径。
       pub paths: Vec<String>,
       /// 夹到 1..=1 MiB；默认 256 KiB。
       pub byte_limit: Option<u32>,
   }

   #[non_exhaustive]
   pub enum SourceTarget { Workspace, Lane { lane_id: AgentLaneId } }

   #[non_exhaustive]
   pub enum WorkspaceDiffScope { Worktree, Index, Both }
   // `Both` 以带 `staged` 标记的 worktree 行应答，而非每个路径两行
   // （2026-09-09 修订，见"实现修订"）。

   RuntimeEventKind::WorkspaceDiffLoaded { command_id: String, page: WorkspaceDiffPage }

   pub struct WorkspaceDiffPage {
       pub target: SourceTarget,
       pub source: WorkspaceSourceView,
       pub entries: Vec<WorkspaceDiffEntry>,
       pub truncated: bool,
   }

   pub struct WorkspaceDiffEntry {
       pub path: String,
       pub index: Option<WorkspaceChangeKind>,
       pub worktree: Option<WorkspaceChangeKind>,
       pub staged: bool,
       pub diff: Option<DiffFile>,
   }
   ```

   门：以目标根为入参，对现有非变更型 `git_diff` spec 过
   `PermissionEngine::decide`；拒绝即 `CommandRejected`。来源：Core 在目标
   根运行 `git status --porcelain=v2` 取条目状态，再按 scope 运行 `git diff`
   与 `git diff --cached`。未跟踪条目沿用 `QueryWorkspaceFiles` 的排除表。
   该页是与 `WorkspaceFilesLoaded` 同类的查询应答，不存入
   `RuntimeViewState`，因此不移动任何快照摘要。

### fixture `structured-diff.json`

一个 `edit_file` 审批，其 `decision_context.diff` 含一个文件一个 hunk；一个
带两文件 diff 的 `MergeAgentPatch` 审批；一页 `WorkspaceDiffLoaded` 含两个
条目，一个已暂存、一个因上限 `omitted`；一个被拒的 `QueryWorkspaceDiff`
以 `CommandRejected` 应答。

### 客户端

GUI：D2 与 D1 权限 dock 从 `decision_context` 渲染 hunk 行并撤下
GUI-CORE-012 的不可用标记；DiffReview 的文件树与 diff 面渲染
`WorkspaceDiffLoaded`；D1 变更文件芯片使用 `WorkspaceChangeView.diff`。
TUI：审批浮层在上下文存在时渲染 hunk 行，否则渲染预览文本。

## 2. `runtime.operator_git`（C2，关闭 GUI-CORE-020）

### 类型

```rust
RuntimeCommand::RunOperatorGitAction {
    command_id: String,
    target: SourceTarget,
    action: OperatorGitAction,
}

#[non_exhaustive]
pub enum OperatorGitAction {
    /// paths 为空表示全部已变更路径。
    Stage { paths: Vec<String> },
    Unstage { paths: Vec<String> },
    Commit { message: String },
    Push { remote: Option<String>, set_upstream: bool },
    Fetch { remote: Option<String> },
}

RuntimeEventKind::OperatorGitActionFinished {
    command_id: String,
    target: SourceTarget,
    action: OperatorGitAction,
    outcome: OperatorGitOutcome,
    audit_id: String,
}

#[non_exhaustive]
pub enum OperatorGitOutcome {
    /// `output` 限 8 KiB；`source` 在效果之后重采样。
    Completed { output: String, source: WorkspaceSourceView },
    Failed { class: OperatorGitFailureClass, detail: String },
}

#[non_exhaustive]
pub enum OperatorGitFailureClass {
    NothingToCommit,
    NonFastForward,
    AuthenticationRequired,
    RemoteUnreachable,
    NoUpstream,
    PathOutsideRepository,
    Other,
}
```

### 有意排除

- `pull`、`merge`、`rebase`：它们移动 `HEAD` 且可能制造冲突，冲突属于 Lane
  冲突机制。`pull` 在 `0.3.4` 冲突内容能渲染其结果后再议。
- `commit --amend`、`reset`、force push、删分支：改写历史，已登记的 DiffReview
  没有这些操作位，也没有撤销故事。
- `switch`、`checkout`：Lane 身份由 Core 经 starter Lane 路径持有；在已绑定
  的 Lane 之下切换 workspace 分支会使其 owner 绑定失效。
- `stash`：不在设计中。

### 映射与流程

每个动作精确解析为 `crates/tools/src/git/` 中的一个既有 tool spec 与入参：
`Stage` 到 `git_add`，`Unstage` 到 `staged=true` 的 `git_restore`，`Commit`
到 `git_commit`，`Push` 到 `git_push`，`Fetch` 到新增的 `git_fetch` spec
（`is_mutating: true`，它写 refs）。动作经同一个 `BuiltinTool::run` 执行，
操作者与 agent 产生完全相同的效果、受完全相同的规则约束。风险分级：`Push`
为 `High`，`Commit` 为 `Medium`，其余 `Low`。

流程：校验目标与动作；plan mode 拒绝；对映射的 spec 过
`PermissionEngine::decide`；`Allow` 继续，`Ask` 发布 `target.kind = "git"`
的 `ApprovalRequested`（`Commit` 时附带持有已暂存 diff 的
`decision_context`），`Deny` 拒绝。随后追加审计记录（`AuditObjectRef` 种类
`source`，Lane 目标再加 Lane 引用），运行工具，发出
`OperatorGitActionFinished`，再以重采样的 source 发出
`WorkspaceSourceUpdated`。权限已授予之后的失败是带 `Failed` 结果的事件，
因为效果已被尝试且审计记录承载它；`CommandRejected` 只用于效果前的拒绝。
Core 在唯一一处从 git 的 stderr 分类失败，客户端从不解析输出文本。

Lane 目标校验 Lane 存在且未归档。Lane 的 agent 变更策略约束的是 agent 而非
操作者；操作者动作仍经权限门并对该 Lane 审计。

### fixture `operator-git.json`

一个被策略拒绝的 `Stage` 以 `CommandRejected` 应答；一个走 `Ask` 路径的
`Commit`，带 `ApprovalRequested`、`ApprovalResolved`、
`OperatorGitActionFinished { Completed }` 与显示 `ahead` 递增、`dirty` 为
false 的 `WorkspaceSourceUpdated`；一个以 `Failed { NoUpstream }` 结束的
`Push`。输出为固定字符串，不含机器路径。

### 客户端

GUI DiffReview 提交栏：Stage all、Commit、Commit and Push，最后一个是两条
顺序命令，第二条只在第一条报告 `Completed` 后发出。标题栏 sync 芯片变为
控件：`ahead > 0` 时 push，否则 fetch；capability 缺席时禁用并标注而不是
隐藏。TUI：`/git` 系统命令带 stage、commit、push、fetch 选择行，发送同一
命令；结果以带本地化失败类别的类型化系统条目渲染。两个客户端都不从输出
文本推断成功。

## 3. `runtime.conflict_content`（C3，关闭 GUI-CORE-015）

### 类型

```rust
pub struct ConflictContent {
    pub baseline: ConflictBaseline,
    pub files: Vec<ConflictFile>,
    pub truncated: bool,
}

#[non_exhaustive]
pub enum ConflictBaseline {
    Revision { sha: String },
    Evidence { bindings: Vec<ReviewedEvidenceBinding> },
    Unknown,
}

pub struct ConflictFile {
    pub path: String,
    pub hunks: Vec<ConflictHunk>,
    pub omitted: bool,
}

pub struct ConflictHunk {
    /// 当前文件在该 hunk 旧区间处的区段。
    pub ours_start: u32,
    pub ours: Vec<String>,
    /// 来自补丁的传入 hunk 行。
    pub theirs_start: u32,
    pub theirs: Vec<String>,
    /// 补丁前像：该 hunk 期望找到的内容。
    pub base: Option<Vec<String>>,
    pub reason: ConflictHunkReason,
}

#[non_exhaustive]
pub enum ConflictHunkReason {
    ContextMismatch,
    AlreadyApplied,
    FileMissing,
    FileDeleted,
    Binary,
}
```

挂为 merge 失败引发的弹回的 `ConflictBounce.content: Option<ConflictContent>`，
以及 Lane apply 路径的 `LaneConflictView.content` 与
`LaneConflictDetected.content`。

### 生产者

当 `LocalPatchBackend::prepare` 或 `LaneEffectRequest::Apply` 拒绝某个 hunk
时，Core 从传入 hunk 取 `theirs`，只读读取当前文件在该 hunk 旧区间处取
`ours`，从该 hunk 自身的前像行取 `base`。该前像正是冲突所依据的基线，
所以没有任何编造。`ConflictBaseline` 在 gate 持有规范基线绑定时为
`Evidence`（已在 `ConflictBounce.baseline_evidence` 上），无绑定时取 apply
目标中的 `git rev-parse HEAD` 作为 `Revision`，否则为 `Unknown`
（2026-09-09 修订，见"实现修订"：`AgentLaneRecord` 并无 `base_revision`）。操作者经
`BounceMergeConflict` 带理由发起的弹回背后没有 apply 失败，因此没有内容。
上限：256 KiB，按文件 `omitted`。

该内容展示两侧与前像。它不是三方合并，也不得呈现为三方合并。

### fixture `conflict-content.json`

两个 Lane 一个文件：Lane A 的补丁合并（`MergeGateUpdated` 到 `merged`）；
Lane B 的 `MergeAgentPatch` 在同一文件失败，`MergeConflictBounced` 携带含
一个 hunk（ours、theirs、base）的内容；再加一个 Lane apply 路径的带内容
`LaneConflictDetected`。view 摘要包含该弹回。

### 客户端

GUI D12 并排渲染 ours 与 theirs，附 base 行与原因；撤下 GUI-CORE-015 的
不可用标记。TUI：决策浮层显示 `n files · m hunks`，详情模态仅用已登记字形
列出 hunk。

## 4. `runtime.evidence_reads`（C4，开立并关闭 GUI-CORE-025）

`RuntimeViewState.latest_evidence` 是最近窗口投影。`EvidenceView` 设计需要
按天分组、可分页的归档以及行背后的内容，所以本 capability 加两个读取。
GUI 在关闭它的同一批次里开立 GUI-CORE-025。

### 类型

```rust
RuntimeCommand::QueryEvidence { command_id: String, query: EvidenceQuery }

pub struct EvidenceQuery {
    /// owner 前缀作用域匹配，同 `AuditQuery.lane_id`。
    pub owner: Option<RuntimeOwner>,
    /// 空表示全部种类。上限 32 条；超限的过滤器被拒绝，而非收窄
    /// （2026-09-09 修订，见"实现修订"）。
    pub kinds: Vec<String>,
    /// 夹到 1..=200。是夹取而非拒绝：请求了过多行的客户端，其意图仍是
    /// 可以回答的。
    pub limit: u16,
    pub after: Option<String>,
}

RuntimeEventKind::EvidencePageLoaded { command_id: String, page: EvidencePage }

pub struct EvidencePage {
    pub entries: Vec<EvidenceView>,
    pub complete: bool,
    pub next_after: Option<String>,
}

RuntimeCommand::ReadEvidenceContent { command_id: String, evidence_id: EvidenceId }

RuntimeEventKind::EvidenceContentLoaded {
    command_id: String,
    evidence_id: EvidenceId,
    content: EvidenceContent,
}

#[non_exhaustive]
pub enum EvidenceContent {
    /// 限 256 KiB。
    Text { text: String, truncated: bool, sha256: String },
    Diff { document: DiffDocument, sha256: String },
    Unavailable { reason: EvidenceUnavailableReason },
}

#[non_exhaustive]
pub enum EvidenceUnavailableReason {
    SummaryOnly,
    MissingCanonicalBytes,
    HashMismatch,
    Binary,
}
```

### 语义

按 `(timestamp, id)` 稳定排序，游标不透明。内容只从 `EvidenceView.canonical`
指名的规范 ContextStore 字节读取。`task_summary` 这类仅供展示的证据应答
`Unavailable { SummaryOnly }`；哈希不再匹配的字节应答
`Unavailable { HashMismatch }`，绝不作为规范内容提供。`patch` 种类的证据经
capability 1 的同一解析器应答 `Diff`。门的姿态与 `QueryAudit` 一致：有界、
owner 作用域、不经 tool 门，因为证据存储是 Viden 自身状态而非 workspace。

### fixture `evidence-reads.json`

镜像 `audit-reads.json`：两页、一个种类过滤、一次文本内容读取、一个仅摘要
的 `Unavailable`、一个 `kinds` 过滤器超出 32 条上限、以 `CommandRejected`
应答的查询。

### 客户端

GUI EvidenceView：按天分组列表、带键值报告与输出尾的详情、来自 `metadata`
与 `canonical` 的关联芯片、`patch` 种类的 "在 review 中打开" 进入
DiffReview。TUI：`0.3.2` 监督 checkpoints 中延后的证据检视器，从决策浮层
打开。

## 每个 capability 的簿记

- `viden-types`：类型、进入 view 的事实的 reducer 分支（审批上下文、
  workspace 变更 diff、弹回与 Lane 冲突内容）、已知事件分支。
- `viden-runtime`：生产者、门、审计记录、失败分类。
- `viden-core`：capability 常量、带刷新测试的扩展 fixture、清单摘要。
- 双语文档：`docs/core-0.3-compatibility.md` 的 capability 列表与 fixture
  语料；`docs/frontend-integration-contract.md` 的命令归属行与新的
  "Source Control And Diff UI Contract" 一节；`apps/gui/contract-requests.md`
  带日期的关闭条目；`scripts/tui-regression.sh` 的 capability 计数。
- 落地顺序：1，然后 2 与 3 并行，然后 4。各自独立分支与评审。

## 已定默认值

以下是本设计做出的判断；在依赖它的批次派发之前，操作者可以推翻其中任一条。

- `Fetch` 是变更型 spec，因为它写 refs。
- "Commit and Push" 是两条命令，绝不是一个复合效果。
- 对 Lane worktree 的操作者动作不受该 Lane 的 agent 变更策略限制，但始终经
  权限门并对该 Lane 审计。
- diff 与冲突载荷上限：workspace 变更事实 64 KiB，diff 读取默认 256 KiB、
  最大 1 MiB，冲突内容与证据文本 256 KiB。

## 实现修订（2026-09-09）

前言说明：批次可以偏离本设计，前提是在其报告中记录理由；若该偏离通过评审
后仍然成立，则记入本文档。本节即该记录。以下每一条都已在对应批次的批准
记录中被接受；上文正文只在与其矛盾处做最小订正，原始措辞的历史保留不改写。

### C1 `runtime.structured_diff`

- **Core 读不到的文件，`base_sha256` 缺省**，而不是空内容的 SHA-256。空内容
  的哈希是读者可以拿去比对并因此出错的值；缺省则表示 Core 不知道。
- **多文件补丁不带 `base_sha256`。** 它命名的是单个文件的前像，而
  `MergeAgentPatch` 有多个文件；在那里给一个哈希会宣称超出其覆盖范围的事实。
- **`render_diff` 的输出折叠为一个隐式的整文件 hunk**，使既有渲染器的字节
  不变，同时在其旁边给出类型化的行。
- **审批预览的 `DiffFile.path` 与 `DiffFile.kind` 取自工具入参**：拟议变更
  已经命名了两者，而内存中对替换内容做的 diff 不带文件头。
- **`Both` 作用域以 worktree 行加 `staged` 标记应答**，而非每个路径两行。
  一个文件两行会被读成两处变更。
- **直接工作区 Lane 解析到工作区根目录。** 没有自有 worktree 的 Lane 不是
  错误，它的目标就是工作区本身。

### C2 `runtime.operator_git`

- **命令携带 `owner`**，并与信封 actor 做相等校验。被审计的变更必须指明
  是谁发起的。
- **`Completed` 在 `output` 旁携带 `truncated`**，使被截断的 8 KiB 尾部是
  一个明示事实，而不是一条看起来很短的命令输出。
- **每个动作写两条审计记录**而非一条：效果之前写 `authorized`，之后写
  `completed` 或 `failed`，以 `attempt` 关联。只在事后写一条记录，无法在
  效果执行中途崩溃时留下痕迹。
- **不带 `set_upstream` 的 `Push` 先做 `NoUpstream` 预检**，使未跟踪分支以
  可操作的失败类别被拒绝，而不是抛出 git 的 stderr。
- **未知远端归类为 `RemoteUnreachable`。** 该类别的补救动作——指定一个存在
  的远端——正好匹配。
- **类型位于 `crates/types/src/source_control.rs`**，与其重采样的
  workspace source view 放在一起。

### C3 `runtime.conflict_content`

- **Lane 基线取 apply 目标中的 `git rev-parse HEAD`。** `AgentLaneRecord`
  并无 `base_revision` 字段，设计中的措辞命名了 Core 并不持有的事实；实际
  被 apply 所依据的修订版本是诚实的替代。
- **合并路径在 gate 无规范绑定时回退到工作区 `HEAD`。**
- **生产者位于 `crates/tools/src/patch.rs`**，即唯一的严格 apply，而不在
  runtime 中。两个生产者就是"被拒绝的 hunk"的两种定义。
- **本地 apply 从不产生 `Binary`**：它在进入 hunk 匹配之前就拒绝二进制
  补丁。该变体为后续 apply 路径保留，并记录为当前不可达。
- **写入失败、后像失败与重命名拒绝的 `content` 为 `None`。** 它们不是 hunk
  冲突，把它们装扮成冲突，等于向评审者展示从未冲突过的行。
- **fixture 为 `conflict-content.json`。** `merge-gate.json` 不含弹回，新的
  内容在那里无处附着。

### C4 `runtime.evidence_reads`

- **`limit` 夹取；`kinds` 上限 32 条，超限则拒绝。** 这解决了设计自身的
  不一致：一处说夹取，另一处说拒绝。页大小是 Core 可以代为收窄的偏好；
  过滤器列表不是——收窄它等于回答了另一个问题。
- **owner 作用域匹配器为本 capability 新写。** `AuditQuery` 没有可复用的
  通用匹配器。
- **`EvidenceCursor` 公开，无日期行排在最前。** Core 从未标注日期的行在
  顺序中有真实位置，而那个位置就是开头。
- **上限是具名常量**（`MAX_EVIDENCE_PAGE_SIZE`、
  `DEFAULT_EVIDENCE_PAGE_SIZE`、`MAX_EVIDENCE_QUERY_KINDS`、
  `MAX_EVIDENCE_CONTENT_BYTES`），而不是字面量，客户端因此可以断言它们。

### 客户端

- **G1a（GUI DiffReview 读取侧）。** 入口是标题栏的 dirty 标记而非同步
  芯片，另加 Actions 分组下的命令面板条目与 `⌘R`。芯片改为在 G1b 中成为
  push/fetch 控件。
- **G1b（GUI DiffReview 动作侧）。** "Commit and Push" 是两条命令，第二条
  仅在第一条报告 `Completed` 之后发出。按文件的暂存/取消暂存是字形开关。
  apply 行以 capability 为条件，沿用 GUI-CORE-012 的先例。
- **G2a（GUI D12 冲突）。** ours 与 theirs 并排渲染，而非按设计标记的上下
  堆叠，因为这处碰撞读起来就是一次对照。操作者发起的弹回与"产生不了内容
  的 apply 弹回"无法区分——`ConflictBounce.reason` 不是可选字段——因此二者
  由一句诚实的说明共同覆盖，而不是断言某个成因。对冲突类型的 serde 编码
  读取已由 F1 的 facade 再导出终结。
- **G2b（GUI EvidenceView）。** D1 没有证据芯片或状态栏计数，因此未接线。
  `EvidenceVerificationState` 与 `EvidenceQualityStatus` 当时不在
  `viden-core` facade 上；F1 已再导出，详情栏现在陈述这两项判定。
- **T1a（TUI 对等）。** 冲突详情仅对 gate 弹回可达，因为 Core 只在那里
  附上 TUI 能取到的内容。固定审批面板保留 `input_preview`，hunk 行是新增。
  两个客户端在目标没有 Core 发布的 owner 时都拒绝操作者 git 动作（GUI 为
  `D1-OPERATOR-GIT-OWNER`，TUI 为禁用并标注的选择行），并援引
  GUI-CORE-027。
- **T1b（TUI 证据检视器）。** `JumpIndex` 与 `CommandDefinition` 增加了
  capability 门控，使入口诚实地消失而不是在使用时失败。没有实时详情截图：
  离线原生会话中持久归档为空——见 `docs/core-0.3-compatibility.md` 中记录
  的未决 Core 缺口。
