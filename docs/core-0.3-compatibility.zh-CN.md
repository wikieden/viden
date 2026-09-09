# Core 0.3 兼容性

English version: [core-0.3-compatibility.md](core-0.3-compatibility.md)

本文是冻结的 Core 0.3.0 `frontend-contract-v1` payload 及其向后兼容 Core 0.3.3
extension candidate 的人类可读兼容清单，记录前端 schema、handshake capabilities、
migration 顺序、确定性 fixture corpus、UI 偏好契约和设计入口层级。

## 冻结状态

```text
component = viden-core
component_version = 0.3.0
supported_schema_versions = [1]
active_schema_version = 1
contract_payload_sha: 5bd2b80b0953f4194d082940a7b9164c7231ca2d
```

这里记录的 40 字符 SHA 标识评审通过的 contract payload commit。本文通过单独的
evidence commit 落盘；该 evidence commit 是 TUI 与 GUI 的共同精确分支基线，并且它的
parent 必须等于这里记录的 payload SHA。本文不授权 tag、push、publish 或 Homebrew
变更。

### ExecutionTarget 冻结

`ExecutionTarget`（`crates/types/src/agent.rs`）在 0.3.x 线冻结为 schema-1 的
lane 事实，仅有两个已声明变体：`local` 与 `ssh { host }`。目前只有 `local` 拥有
执行适配器；`ssh` 是已声明的 P1 目标——lane 可以携带它作为契约数据，但尚无运行时
适配器执行它，客户端必须诚实呈现该状态，不得暗示远程执行可用。新增目标类型属于
加法式 schema 变更，只能经由本文管辖所有冻结面新增的同一契约评审落地；该枚举有意
保持精确（不加 `non_exhaustive`），使新增变体时客户端的穷尽匹配在编译期 fail
closed。

## 冻结 Capability 集合

Core 以 `CORE_CLIENT_CAPABILITIES` 暴露并供 handshake 使用的冻结 capability 常量是
唯一真源。Core 0.3.0 公布以下精确、唯一且按字典序排列的集合：

```text
runtime.agent_dag
runtime.approvals
runtime.commands
runtime.context
runtime.cost
runtime.events
runtime.evidence
runtime.merge_gate
runtime.queued_input
runtime.replay
runtime.snapshot
runtime.transcript_page
runtime.typed_lanes
runtime.typed_tasks
ui.preferences
```

每份冻结 base fixture 的 requirement 都必须存在于该集合；单独登记的 extension fixture
只能要求已公布的 extension capabilities。任何 fixture 要求未知 mandatory capability 都会
在兼容性验证中失败；malformed 或 ambiguous legacy input 必须拒绝，不能猜测。

## Schema-1 冻结后的扩展候选

上面的 Core 0.3.0 冻结 capability 集合与原九份 fixture bytes 保持不变。Core 0.3.3
候选通过 `FRONTEND_V1_EXTENSION_CAPABILITIES` 和
`crates/core/frontend-contract-extensions.toml` 单独公布以下精确、按字典序排列的增量
capability：

```text
core.workspace_host
runtime.agent_adapters
runtime.agent_conversation
runtime.agent_permission_bridge
runtime.agent_session_input
runtime.agent_sessions
runtime.audit
runtime.cockpit_context_v1
runtime.credential_handles
runtime.credential_staging
runtime.lane_lifecycle
runtime.lane_owner_projection
runtime.project_onboarding
runtime.recent_work
runtime.starter_lane_preview
runtime.structured_diff
runtime.trust_loop
runtime.workspace_eligibility
runtime.workspace_files
ui.preference_persistence
```

schema 仍为 `1`。客户端只具备冻结 base 集合也可以连接；extension capability 缺失只
禁用对应功能，不得阻止无关启动。被禁用功能必须明确显示 unavailable，并且 command
零发送。具体而言：TUI stable Settings 要求 `ui.preference_persistence`；GUI D11 recent
work 要求 `core.workspace_host` 与 `runtime.recent_work`；TUI/GUI reviewed D4 创建要求
`runtime.starter_lane_preview`；精确 active-Lane cancel 还要求
`runtime.lane_owner_projection` 且恰好一条权威 binding。

只读的追加式 audit timeline 查询（`QueryAudit` -> `AuditPageLoaded`）要求
`runtime.audit`：不触发 permission prompt，也不被 plan mode 阻断。缺少该 capability
的客户端零发送，并明确显示 timeline 不可用，而不是渲染成空列表。

自 core-0.3.5 起新增两处 schema-1 additive 扩展，用于在并发与过滤场景下保持该查询诚实：

- `AuditPageLoaded.command_id` 指名该 page 所回答的那次 `QueryAudit`。客户端要求精确
  匹配；携带其他读者 id 的 page 一律忽略。该字段是 optional，因此早于该字段的 Core 发出的
  page 会反序列化为 `None`，客户端退回到"以自己被接受的查询做关联"的旧规则。客户端绝不
  为这类 page 伪造 id。
- `AuditQuery.actor`、`AuditQuery.from` 与 `AuditQuery.until` 按 actor（`operator`、
  `system`、`any_agent` 或具名 agent）与半开区间 `[from, until)`（unix 秒）过滤。Core 在
  分页之前应用它们，因此 `complete` 与 `next_before` 描述的是**过滤后**的 timeline。区间
  倒置会被拒绝，而不是返回空 page，因为空 page 会被读成"该时间窗内什么都没发生"。本次构建
  无法归类的 filter 变体或 actor 变体一律不匹配，因此 filter 绝不会声称拥有它无法指名的
  记录。三个字段默认缺省，因此旧客户端写出的查询语义完全不变。

工作区文件清单查询（`QueryWorkspaceFiles` -> `WorkspaceFilesLoaded`）要求
`runtime.workspace_files`（GUI-CORE-022）。与 audit 读取不同，它要过权限门禁，因为它读的是
操作者的工作树：

- Core 在读取任何一个目录项之前先咨询权限引擎，工具名为非变更的
  `workspace_file_inventory`，输入路径为工作区根——与其他工作区读取完全同一道门禁。deny
  会以 `CommandRejected` 返回，指名这次确切的读取并携带拒绝原因；未解决的 ask 同样如此：
  这次读取回应的是一次按键而不是一次交互回合，因此它以非交互方式判定，而不是把客户端卡在
  审批弹窗后面。两种情况都绝不发布空 page，因为"你无权读取"和"这个工作区没有文件"是两个
  不同的事实；也都不是不带 command id 的裸 `Error`——那会让有读取在途的客户端把无关的 lane
  或 provider 失败误认成自己这次读取的拒绝。
- 该工具不产生变更，因此 plan mode 仍会通过权限引擎的 safe-read 分支给出答案，同时所有变更
  依旧被阻断。
- 遍历遵循 gitignore，即便不在 Git 仓库内也遵循 `.gitignore`，且不读取全局或父目录的 ignore
  文件，因此同一个工作区在两台机器上枚举结果一致。`.git/`、`.viden/`、`.omx/`、
  `.worktrees/`、`.ref/` 被无条件排除：它们承载的是运行时与 agent 状态，而不是工作区内容。
- 条目先按字典序排序，**然后**才应用 prefix 过滤、排他的 `after` 游标与 `1..=500` 的 limit
  钳制，因此 `complete` 与 `next_after` 描述的是过滤后的有序清单。越出工作区的 prefix 会被
  拒绝，而不是被钳制或以空 page 回答。
- `WorkspaceFilesLoaded.command_id` 是**必填**而非 optional。audit page 的关联 id 是后续新增
  的，因此永远带着一个 `None` 分支；本事件是全新的，所以客户端始终能拿到该 page 所回答的确切
  读取，绝不需要退回到"关联自己被接受的查询"。
- 缺少该 capability 的客户端零发送，并明确显示清单不可用，而不是自行遍历文件系统——那超出了
  客户端边界，也会绕过这道门禁。

结构化 diff（`ApprovalRequestView.decision_context`、`WorkspaceChangeView.diff`，以及
`QueryWorkspaceDiff` -> `WorkspaceDiffLoaded`）需要 `runtime.structured_diff`
（GUI-CORE-012）。它在严格意义上是叠加式的：两个新字段都是带 `skip_serializing_if` 的
`Option`，因此不携带它们发布的审批或变更编码出的字节与此前完全一致，九个冻结 base
fixture 保持其字节与摘要身份。

- Core 是 diff 行的唯一生产者。**应用**补丁的那个 unified diff 解析器被提升为发布
  `DiffDocument`，因此应用路径与所有客户端对"什么是一个 hunk"的理解一致，前端不再把
  展示文本解析成行。
- 决策上下文以只读方式计算。`edit_file` 或 `write_file` 预览读取目标文件并在内存中应用
  拟议替换；审批时不写入任何字节，这正是让"拒绝"仍然有意义的前提。trust loop 的
  `MergeAgentPatch` 上下文来自 Core 已持有的规范补丁字节，即多文件情形。
- `base_sha256` 命名单文件预览所基于的字节。已记录的限制是：执行时才把拟议的工具输入
  作用到当时的文件内容上，因此期间被修改的文件会产生不同结果；该哈希让客户端或审计
  读者事后能够发现这一点，而 `0.3.3` 不做执行前重新校验。多文件补丁完全不携带哈希，
  因为一个哈希无法描述多个文件。
- 操作者读取与文件清单一样过权限门禁，但使用**既有的**非变更工具 `git_diff`，输入路径
  为解析后的目标根，因此同一套规则同时约束操作者的评审面板与 agent 的 `git_diff` 调用。
  deny、未解决的 ask、越出目标的路径、未知或已归档的 Lane，都以 `CommandRejected` 指名
  该次读取返回——绝不发布空 page，因为空 page 会被读成"没有变更"。该工具不产生变更，
  因此 plan mode 仍可回答。
- 边界降级的是行，绝不是条目。`byte_limit` 钳制到 `1..=1 MiB`，默认 256 KiB；越界文件以
  `omitted` 发布并保留真实计数，page 置 `truncated`，因此"未展示"始终可与"未变更"区分。
  该 page 与 `WorkspaceFilesLoaded` 一样是查询结果，绝不折叠进 `RuntimeViewState`，因此
  不会移动任何快照摘要。

自 core-0.3.5 起还新增一处 additive schema-1 扩展，使实时工作可归属（GUI-CORE-010）。
`AgentTaskRecord`、`ToolCallView`、`QueuedInputView` 与 `EvidenceView` 各自新增一个
optional 的完整 `RuntimeOwner`；`RuntimeEventKind::ToolCallStarted` 也带上同一字段，因为
reducer 是从 event 而不是 envelope 折出该 view 的。Core 只在发出点确实持有真实 owner 身份
时才填充它：Lane worker 自身的绑定（Lane 排队输入）、Core 发布该 Agent session 时使用的
owner（该 session 的 tool call 与 evidence）、merge gate 自身的 owner（gate 绑定的
evidence）、以及随 durable agent job 持久化的 owner（其 task 记录）。其余位置一律留空，
含义是"Core 在发出时并不知道 owner"——绝不是 default owner，客户端也绝不可依据时序、顺序
或展示标签推断。字段缺省时不写入 wire，因此无已知 owner 的记录编码字节与该字段存在之前完全
一致，冻结语料未变。

Agent 选择要求 `runtime.agent_adapters`；启动和取消 typed external session 要求
`runtime.agent_sessions`；ACP permission request 只有在协商到
`runtime.agent_permission_bridge` 后才可交互。Adapter view 只包含安全的
availability/auth facts，不包含 raw command、环境引用或 agent-native credential。
前台与异步 ACP session 共用 Core-owned approval queue。重启后，Core 必须先终止被中断
的 external process，再发布可恢复 failed session，因此 replay 不会虚构仍存活的进程。

基于 Core 0.3.0 编写的客户端仍然只要求冻结集合，并把不支持的 schema-1 事件保留为
`RuntimeWireEvent::Unknown`。新客户端只有在协商到 `runtime.lane_lifecycle` 后，才启用
13 个 Lane 生命周期命令以及 `LaneUpdated`、`LaneCommandAccepted`、
`LaneOutputAppended`、`LaneConflictDetected`、`LaneRecoveryRequired` 投影。Lane 命令回执
使用扩展专属的顶层事件，因此 0.3.0 客户端会把整个 payload 保留为 unknown，而不会因
内嵌的新命令变体导致解码失败。扩展投影为空时不会参与序列化，
因此重放冻结的 0.3.0 corpus 仍保持已记录的 canonical bytes 与 digest。

Terminal 与 tmux Lane 被显式标记为成本盲区：`AgentRoute::cost_meterability` 对它们
返回 `blind`，对 built-in 与 ACP route 返回 `metered`；Core 绝不为 blind route 发布
推断出的 token 或金额。它们的全部成本面就是直接观测到的有界 `LaneRunStats`——累计
wall time、run count、已应用 diff 字节数，以及最近一次完成运行的 exit code。exit code
是尽力而为的：平台没有提供时即缺失，tmux 恒为此情形，因为 `kill-session` 会在读取任何
状态之前销毁 pane。这些事实由新增的 append-only `RunObserved` Lane event 归约，包含
`started`、`stopped`、`applied` 三个 phase。所有拆除活跃 Lane 运行时的路径——stop、
cancel、archive 与 cleanup——都会关闭当前开启的 run，因此操作者终止失控 terminal Lane
时仍会保留其 wall time 与 exit code；从未运行过的 Lane 被拆除时不记录任何观测。
与生命周期事件不同，运行观测采用宽松归约：
没有对应 start 的 stop 只记录 exit code、不累加 wall time，因此运行中途崩溃不会导致
Lane 日志无法重放；但针对未知 Lane 的观测仍会被拒绝。`AgentLaneRecord.run_stats` 是
可选的附加字段：Lane 从未被观测到运行时不参与序列化，因此已记录的 schema-1 fixture
bytes 与 digest 保持不变，且"缺失"与"测量为零"仍然可区分。

`runtime.trust_loop` 新增 typed handoff、review request、contract、dependency、
merge-gate policy/validator/decision、conflict bounce 与 revert facts。八个新增跨 Lane
command（含显式 `RevalidateMergeConflict` 与 `DecideReview`）及其 events 均经过权限门禁并由共享
reducer 重放。Schema 仍为 `1`：新增
record fields 提供默认值，未知字段可忽略；扩展前的 string merge decision 会读取为只读
`legacy` decision，新写入则始终序列化 typed decision。只有绑定真实 ContextStore bytes
与 Core 签发 permission receipt 的 evidence 才能产生 canonical acceptance；展示摘要不能
替代 evidence。指定 validator 必须绑定精确 id/hash 集，且所有 trust 纯 preflight 在
approval 前完成；`RequestReview` 本身由发起请求的 gate owner 授权。`DecideReview` 只
授权给独立评审 Lane，要求被评审的 evidence binding 未发生漂移，接受结论会为 gate
validator 打戳，驳回结论则阻断 `AcceptMergeGate`；已结算的评审不会被后续 gate 决定
覆盖。Dependency id 是稳定
edge id，不能重绑到不同端点。Merge 在改动文件前持久化私有 content-addressed recovery
snapshot 与 workflow precommit；重复 preimage blob 会复用，私有 recovery lock 拒绝
symlink traversal，从而在不把 raw preimage 写入 event log 的前提下支持重启后 audited revert。

Core 负责 Lane 权限判定，并在每个 Lane 命令前从当前 runtime mode 刷新权限状态。
所有有副作用的命令都使用 permission check 与 effect executor 共享的 canonical worktree
或 repository target 判定。已有 symlink 只能解析到仓库内；缺失 target 通过最近的真实父目录
解析，拒绝 symlink parent 与 `..`，并在本地 effect 前再次校验。审批预览会红写 command、
arguments、environment、input 和 diff payload。重启时，处于 starting、running 或等待审批
状态的 Lane 会恢复为 blocked recovery fact，并继续绑定其持久化 session owner。

项目接入对当前目录执行只读探测。`PreviewProjectConfig` 校验仓库根
`viden.toml` policy，并返回可审阅的精确 UTF-8 内容及 SHA-256，不写文件；
该 D11 parser 只接受已登记的 `project`、`gates`、`runner`、`budget`、`targets`
schema，拒绝未知 nested field；候选包含 secret field 或 credential-shaped value 时，
不会返回 exact contents。
`ConfirmProjectConfig` 只接受已缓存的 preview id/hash，重新核对目标文件的 base hash，
并在 Build 模式权限批准后写入同一组精确字节。Credential command 只携带 provider、
backend 与一次性 ingress 标识；这些标识采用有长度上限的 ASCII opaque-id grammar，
并拒绝 secret-like marker 与 path syntax。secret bytes 始终留在注入的 backend 中，replay/audit
只记录 `CredentialHandle` 安全元数据。

普通 tool 与 Lane 的审批响应都会按 supervisor 中 permission/mode 变更的命令顺序判定。
但两者刻意采用不同的 generation 语义：普通 tool 读取已提交的 permission control
reservation，因此 permission 或 work-mode 命令一经入队，即使 worker 尚未应用，也会立即
且永久地使阻塞中的审批失效。即使控制命令的 SessionMeta batch 随后持久化失败，已提交的
generation 也不会递减或复用；旧普通审批以 `Deny` 终结，不能恢复，用户必须重新触发 tool
以取得新审批。失败的 reservation 仍会从应用状态投影队列移除，避免其 policy 泄漏到后续
控制命令。Lane 请求则原子冻结 worker 已应用的 generation 及其所描述的 permission
engine；只有队列中的控制命令成功应用后，这一 generation 才会推进，因此 Lane 审批可以
在一次控制命令失败后继续有效。permission 与 work-mode 控制命令会先原子持久化完整的
session metadata batch，再发布新的 live snapshot 与 permission engine；batch 失败时，
engine、snapshot、Lane 配对和已应用 generation 都保持不变。审批等待期间只要已应用的
permission 或 work mode 代际发生过变化，即使可见 flags 随后恢复原值，
旧 Lane 审批也会失效。Lane 响应被接受后，supervisor
必须等待终态 `ApprovalResolved` 以及 effect/persistence 完成，才能处理或发布后续 permission
snapshot。审批产生的 session/repository allow rule 只保存在所属 Lane worker 内，因此会在
该 Lane 的常规权威权限刷新后保留，但不会授权另一条 Lane 或 owner；
Plan/ReadOnly 刷新会立即丢弃这些 rule。Create 与 Lane 状态迁移和其他持久化 effect 使用
同一 permission 与 mutation-policy gate。终态 worker 通过 completion reaper 自动注销并
join，不需要等待下一条 Lane 命令。

## Client 边界

前端 client 只能使用 `CoreClient` 和 `viden-core` 重导出的 protocol/view contracts。
Transport interface 仅包含 discovery、command send、event receive、snapshot、replay 和
transcript paging。前端禁止导入或调用 runtime、provider、tool、permission、session 或
workflow 内部模块。

`StatefulCoreClient` 在提交状态前验证 handshake 与 schema。它忽略 duplicate/older
cursor，只应用连续的 next event；gap replay 必须完整且验证通过后才提交；stream
mismatch 或 snapshot-required recovery 通过验证后的 snapshot 恢复。前端永远不能自行
合成 effect 成功状态。

`viden_core::legacy` 已 deprecated。它只为 pre-v3 TUI bootstrap 临时保留，新的 TUI、
GUI、CLI、API 或 plugin client 禁止使用。

## Schema-1 Fixture Corpus

Fixture 文件位于 `crates/types/tests/fixtures/frontend-contract-v1/`。下表 digest 单元是
经过测试的 fixture 值，必须与对应 fixture state 一起变更。

| Fixture id | 冻结场景 | Expected final view SHA-256 |
| --- | --- | --- |
| `stream-tool` | Assistant stream、tool start、成功 tool finish | `8478c7c0ce6f0adc3efdd3aa11497462e96b3aba50cf66e81b0ad9ddcd992eef` |
| `approval-allow-deny` | Structured scoped allow/deny，前端不拥有 effect | `7788f2f4b34ce54893ab8ed41beb6e37958ff5fda95642d045ef2d1dedbf7b39` |
| `queued-follow-up` | Active work 保持可见时 queue/dequeue follow-up | `eb1bc1a00185d5642f9a95a2cffde7a81f2bd4ac4417385c5c1b6e2aefa8354a` |
| `dag-blocker` | Typed DAG/task dependency blocker 与 recovery action | `a496d331e42f730d41565afe58a3308bf38a7b7e3b92e0279d198e9c407e7719` |
| `multi-lane` | 多条 typed lane，覆盖不同 role、route、gate、owner、target、budget、session facts | `e491d3bc547601b3c54eae05dc1b1259c9cc8ccac948908be8519d432b62fe38` |
| `merge-gate` | Typed evidence 与 Core-owned MergeGate reduction | `41f4d842a12356586a461b173d072d7e7efedb4d7707471c99eb77dd37533321` |
| `context-pressure-cost-blind` | Context pressure/omission 与明确 unknown/unmetered terminal cost | `2e39ec2e32fac56ae6279e8f681bcf4357701de51a6772bad14caee0ddb4ba5e` |
| `plan-denial` | Plan mode 拒绝 mutation，且没有成功 mutation fact | `fa1fa859af8f056686c06b30b789706539d9ed19e02756519757993d5ee31b2d` |
| `d1-vertical-slice` | D1 可见 transcript/tool、lane/task、decision、evidence/gate、context/cost、recovery、UI preferences | `7dd8faf04cca9f3013198e25823894eae91c2869e27087aa1eb0a34890cdf804` |

上表九行是冻结 base corpus。单独登记的 schema-1 extension fixture 为：

| Fixture id | 扩展场景 | 最终 view SHA-256 | Canonical fixture bytes SHA-256 |
| --- | --- | --- | --- |
| `frontend-host-services` | UI 偏好持久化、安全 recent work、reviewed starter-Lane preview/create/invalidation、精确 live Lane owner，以及一个可容忍的未来 optional event | `b118534bb0a568a6a1e781171cecf0512c7d987736c06e4f84d51b5835022a0e` | `96dd5fde9f1241eb50f9d8978cf478d0ac5d3327448dc6ccde9d0e5018ce1580` |
| `interaction-closed-loop` | 文件夹绑定但不隐式配置、reviewed Lane 创建、built-in 与 ACP adapter/session、共享审批、evidence/gate、apply conflict、typed recovery、重连 replay 与完成态 | `31b71bf154d42c8c7923fe9c64763a5245f785a2cd953913124f30a981589b51` | `596e82efa03d21b1f9645f40cf500ca8c4c1b86b2aa78be85a6bea0184822bff` |
| `review-decision` | 独立评审结论：`ReviewRequestStatus` 由 `Pending` 迁移到 `Accepted`，携带 reviewer feedback 与被打戳的 gate validator，同时 gate 决定仍然独立 | `38f81bbc1966fbf5742b0087bdd9e871eb11d58cdee747628ed3f4ca1323713c` | `b8e0b5389c3f21be4b4f28cfeba8d902917a304c6b9252cf9911dcccb6146a2b` |
| `context-budgets` | 两条并发 Lane 各自携带精确绑定的 owner 与互不相同的 task-scoped budget，一条处于软压力、一条越过硬上限 | `1b251b312b05ef950cdfc8190347e848a38d92bdaf26fe7d196e1ba053fc667b` | `7fcbde9edc5aa1a40a5cd41b0a8442403c6424903cc754cbe64d45980389029f` |
| `streamed-turn` | 同一 session 与 message id 下的有序 `AssistantDelta` chunk 恰好重建最终回复，作为终止标记的完成事实不会重复追加 | `2567d9709e6ec96d621fa281acc205ba5a8fe0b8a08f5868b70ca386f70e3a7d` | `3b1129fb57860aa337c571a9f70be2eacca432c4419bfbb1f4c2943dd371b8e2` |
| `message-parts` | ACP turn 在文本之外返回 image part：typed part 只挂到自己的消息上，reference 是 parts 目录的不可变 digest 路径，未建模的 part kind 无损往返 | `0162e39121f8f9f4543970fdc8098580bc5e01e43786de56d50cc520baed32fe` | `2e7f430cf1694baedd4615c0b60faf9b5614475e52b3c1b365b69a22d0f3d0c2` |
| `audit-reads` | 两次并发 audit 读取以相反顺序被回答，每个 page 指名自己的 `command_id`；另有一次过滤读取，其 `complete` 描述的是过滤后的 timeline，而未过滤 page 上仍存在更旧的记录 | `389739e9f28cfaf1e1cc9632316760e60fc43495f3702a21d2944874027bb28e` | `a1bdc24b45fc015b9601cf30ae7916dedd5ee0d5bcd2bbc1b5792e2964ef07d2` |
| `owner-scoped-live-work` | 两个并发 Lane 在各自精确绑定的 owner 下交错发布 task、tool call、排队输入与 evidence 事实，另有同样四类事实在没有 owner 的情况下发布 | `6972686f93d9d2653fa3510a0f74c50d4b7905426ac0554362a07945ac2541d4` | `87dc66790932f819f84903b3efd457dca1c85e3992c862a44919d0fe5bdeefc2` |
| `audit-ordering` | 一页 newest-first 的 audit 记录横跨两个交错的项目，且有一对跨项目、时间戳相同的记录靠降序 audit id 定序 | `4da28fdd43503046033cf65b5362c2cdd482c42ade083bb32f45a014b942c842` | `6c7de7344afb54cf58793c5878672c435da848aea426e5e0a89253ee98d9c3e4` |
| `workspace-files` | 同一项目上的两次并发清单读取以相反顺序被回答，每个 page 都携带必填的 `command_id`；带 prefix 的读取对其子树返回 `complete`，而未加 prefix 的读取仍未完；另有第二个已挂载项目只发布 lane 事实、完全没有清单读取 | `9f1c95e59ff5c4a172791d8c0c862f6286326311853b228cc5b83674e4775c37` | `f907b793d2817372fc71c95122e4e33152755682fff5fe46aa20150704bcb949` |
| `structured-diff` | 一个 `edit_file` 审批，其决策上下文含一个文件、一个 hunk，以及其所基于字节的 `base_sha256`；一个 `MergeAgentPatch` 审批，携带两文件变更且不含基线哈希；一页 diff，其中一条为已暂存条目，另一条被字节边界省略但计数保持真实；以及第二次读取被 `CommandRejected` 以其自身 command id 拒绝 | `3f5f4caf39cee2c46162999a35618599c0197aae15a3abfdd3a098e080676b8a` | `e24915b31f4192d85349be99da4c0ea81b6fb0b126b2076de17775d70de21cd0` |

2026-09-07 语义修正（评审发现 4）：`RuntimeViewState.assistant_stream` 此前没有生命
周期——它在整个 view 生命期内只追加，因此启动重放会把每个历史会话的回复串接成一整块
无归属文本。终态 agent-session fact（`AgentSessionCompleted`、`AgentSessionFailed`，
或携带终态 status 的 `AgentSessionUpdated`）现在会清空它。该 stream 在回合进行中仍
保有完整回复；结算之后，回复由终态 fact 的 `session.output` 与 owner-scoped
`agent_conversation` 承载。这移动了两个 extension fixture 记录的最终 view digest ——
即事件序列中终态 fact 位于 delta 之后的 `streamed-turn` 与 `message-parts` —— 以及
它们的 canonical bytes，因为每个 fixture 自身记录了其期望 view digest。没有任何冻结
base fixture 发生移动：九个 base fixture 都没有把终态 agent-session fact 放在
`AssistantDelta` 之后。没有 agent session 的回合——内置本地 provider 完全不发出
agent-session facts——没有终态事件，其文本照旧留在 stream 中。这是本次修复已记录的
限制，而非疏漏：没有为本地路径发明新事件。

2026-09-07 延后的评审跟进项。在审计上述修复时确认了三处不一致，并有意留给后续独立改动
处理：三者都是客户端内部清理，不影响契约。记录于此，以免被当作新发现重新提出：

1. `CockpitProjection.assistant_stream`（`apps/tui/src/tui/projection.rs:26`，
   构建于 `:77`）是死字段：无人读取。TUI 直接从 `RuntimeViewState` 渲染该 stream，
   因此该字段是一份会与原件各自独立结算的副本。
2. TUI 中存在两套"是否有活跃工作"的定义——`apps/tui/src/tui/app.rs:1878` 用于
   命令路由，`apps/tui/src/tui/state.rs:234` 用于状态文本。二者读取的 fact 集合
   重叠但不相等，因此 composer 与状态行可能对"回合是否在进行"给出不同判断。
3. ACP merge gate 被两种标识键控：`crates/agents/src/acp.rs:1126` 发出协议会话句柄，
   而 `crates/agents/src/glue.rs` 用已发布的 Agent session 为同一个 gate 划定范围。
   Gate 自身的 id 为保持连续性仍沿用协议句柄，2026-09-07 加入的 owner 绑定才使其可
   join；这两个键仍须一并读取。

`structured-diff` fixture 是 `runtime.structured_diff`（GUI-CORE-012）的生成式证据。
它刻意不是顺利路径：一次读取被回答、一次被拒绝，被回答的 page 中一条带行数据，另一条
被字节边界剥掉了行。若客户端把空 page、被边界省略的条目与拒绝渲染成同一种样子，就会
三次说出"没有变更"，而这正是本 capability 要防止的失败。两个审批都出现，是因为单文件与
多文件生产者能够诚实主张的内容不同：`edit_file` 预览给出其读取字节的 `base_sha256`，
多文件补丁则不给出，因为一个哈希无法描述多个文件。这些行的唯一生产者是 Core——应用补丁
的那个 unified diff 解析器被提升为发布它们——因此客户端永不把 diff 文本解析成行。

`context-budgets` fixture 为 `ContextScope` 与 `ContextBudgetRecord` 的 frontend-neutral
facade 导出提供依据。Budget 只能通过该 Lane 精确绑定的 runtime owner 所指名的 typed task
scope 归属到 Lane；"取最近一条 budget" 永远不是有效归属，fixture 中两个 scope 刻意互不相交。
`ContextBudgetExceeded` 同时承载软压力（`exceeded: false`）与越过硬上限两种事实。

`streamed-turn` 与 `message-parts` fixture 把已实现的流式与 typed content part 行为固化为
规范。`agent_message_part` 是 schema-1 的已知 event type，因此 part 会被归约而不是作为未知
事件隔离；part 只挂到其事件指名的那条消息上；Core 未建模的 part kind 保留其发布时的原始对象。

`audit-reads` fixture 是 audit 关联与服务端过滤的生成式证据。两次读取在任一被回答之前都已
被接受，且 page 以相反顺序返回，因此按到达顺序归属会归错，只有已发布的 `command_id` 能归
对。第三次读取过滤到 agent actor，返回 `complete`，而未过滤 page 上仍能看到严格更旧的
operator 与 system 记录——这正是客户端侧过滤永远无法确立的完备性事实。与
`agent_message_part` 一样，`audit_page_loaded` 现已进入 schema-1 已知集合，因此 audit page
会被归约而不是作为未知事件隔离；该 fixture 同时证明 page 绝不折叠进 `RuntimeViewState`。

`owner-scoped-live-work` fixture 是 owner 范围实时工作的生成式证据。两个 Lane 同时存活且
事实交错发布，因此顺序与新近度都不能代替归属：只有已发布的 owner 才能把一条事实归到某个
Lane。第四组事实没有 owner，因此不属于任何 Lane 范围，同时仍在 workspace 层可见——诚实的
客户端渲染的正是这一点，而不是把它归给当前选中的 Lane。

`audit-ordering` fixture 是"audit timeline 跨项目是同一个顺序、而不是按项目分组"的生成式
证据（GUI-CORE-014）。D10 事件走马灯是横跨工作区内每个项目的一页 newest-first 记录，因此它
渲染的顺序必须是全序。`audit-reads` 无法证明这一点——它的三条记录 `project_id` 相同——所以
这里新增独立 fixture，而不是去改动一个数字已被登记的既有 fixture。两个项目交错出现，且有一对
时间戳相同的记录**跨越**项目边界，因此 `audit_id` 的定序恰好在"按项目分组"或"平局时回退到到达
顺序"的客户端会产生可见差异的位置被检验。排序键是 `AuditRecord::cursor()`，即分页使用的同一组
`(timestamp, audit_id)`。

`workspace-files` fixture 是清单读取的生成式证据。同一项目上的两次读取在任一被回答之前都已
被接受，且 page 以相反顺序返回，因此按到达顺序归属会归错，只有必填的 `command_id` 能归对；
带 prefix 的读取对其子树返回 `complete`，而未加 prefix 的读取仍未完——这正是客户端对已持有
page 做过滤永远无法确立的完备性事实。第二个已挂载项目只发布普通 lane 事实、完全没有清单
读取，这就是该请求所要的"没有清单的项目"那一半：范围在该项目的客户端得到的是"没有列表"，
而不是一个可被渲染成"该项目没有文件"的空列表。与 `agent_message_part`、`audit_page_loaded`
一样，`workspace_files_loaded` 已进入 schema-1 已知集合，因此清单 page 会被归约而不是作为
未知事件隔离；该 fixture 同时证明 page 绝不折叠进 `RuntimeViewState`。

扩展 fixture 使用六个正式 known event：`UiPreferencesUpdated`、`RecentWorkLoaded`、
`StarterLanePreviewed`、`StarterLaneCreated`、`StarterLanePreviewInvalidated`、
`LaneRuntimeOwnerBound`；禁止用 transient error 或展示 placeholder 代替。正常内存
journal、snapshot 与 replay 路径必须把这些 facts 归约为相同 `RuntimeViewState`。未来
optional event 只推进 cursor，不修改该 state。

交互闭环 fixture 包含 18 个有序 event，并只使用 locale-neutral fact key。重连测试会
刻意观察一次 cursor gap，重放缺失的连续 batch，并证明 normalized final
`RuntimeViewState`、cursor 与 digest 和不中断 replay 完全一致。
`crates/core/release-manifest.toml` 记录两份 fixture payload 与 Core 0.3.3 contract
implementation checkpoint，但不授权创建 tag。

每个 JSON fixture envelope 包含：

- `fixture_id`；
- `schema_version: 1`；
- 唯一且按字典序排列的 `required_capabilities`；
- `initial_snapshot`；
- 非空、cursor 连续的 `RuntimeEventEnvelope`；
- `expected_final_cursor`；
- `expected_view_sha256`。

每个已知 event 的 event sequence 必须等于 cursor sequence。从同一 initial snapshot
把解析后的 fixture replay 两次，必须得到 byte-identical canonical state、相同 cursor 和
相同 digest。Fixture 值必须确定，且不能包含机器专属绝对路径或 secret。

## Canonical Digest

Final-view digest 是对 `RuntimeViewState` 递归排序所有 object key 后的 compact JSON 计算
SHA-256。Array 顺序保留，因为它具有语义。测试会把生成的 64 字符小写十六进制 digest
与 fixture 和本清单比较。

## Migration Gate 与顺序

Migration 必须在 schema-1 fixture replay 前运行，并且保持幂等：

1. 通过支持的 v0 lane input boundary 解析 `legacy-lanes.tsv`，得到 typed
   `AgentLaneRecord`。
2. 与 `typed-lanes.json` 比较，序列化 normalized typed values 后再次解析，并要求相等。
3. 把支持的 legacy flat cost shape 解析为 structured `CostUsageRecord`；unknown actual
   cost 必须保持 `None`，序列化 normalized record 后再次解析，并要求相等。
4. 把支持的 legacy approval boolean 解析为 structured `ApprovalResponse`，序列化时不再
   输出 legacy boolean，再次解析并要求相等。
5. 所有 migration 通过后，才将每个 schema-1 fixture replay 两次，并验证 identity、
   capabilities、cursor continuity、final state 和 digest。

未知 lane role/route/status、ambiguous cost shape、malformed approval record 和未知
mandatory fixture capability 都必须让 gate 失败。Migration 不得静默强转。

## UI 偏好兼容性

Schema-1 preference surface 与前端框架无关：

前端消费的 effective fact 是
`RuntimeSnapshot.ui_preferences: ResolvedUiPreferences`。Client 只渲染已解析值，不能在
本地重新执行优先级或 fallback policy。

| 维度 | 支持值 |
| --- | --- |
| 内置 locale | `en`、`zh-CN`（`system` 解析为其中之一） |
| Skin | `aurora`、`ice`、`mono`、`amber`、`phosphor` |
| Effective mode | `dark`、`light` |
| Density | `compact`、`regular`、`comfy` |
| Motion | `system`、`reduced`、`full` |

八组有效 effective skin/mode 是：

```text
aurora/dark
aurora/light
ice/dark
ice/light
mono/dark
mono/light
amber/dark
phosphor/dark
```

`amber` 与 `phosphor` 仅支持 dark。偏好优先级是 CLI、user、project、client default。
无效 effective pair 使用安全的 `aurora/dark` + regular density 回退，并记录
`ui.invalid_skin_mode_pair` diagnostic；locale 与 motion 保持解析结果。

## 设计入口层级

视觉验证遵循一条路径：

1. 全局索引：`docs/viden-design/Viden/index.html`；
2. client 索引：`TUI/Viden - 设计稿索引 (TUI).html` 或
   `GUI/Viden - 设计稿索引 (GUI).html`；
3. 组件库：`TUI/Viden - 组件库 (TUI).html` 或
   `GUI/Viden - 组件库 (GUI).html`；
4. canonical 产品入口：`TUI/Viden - 统一原型 (TUI).html` 或
   `GUI/Viden - 桌面驾驶舱 (GUI).html`（D1）。

GUI `pages/Viden - D11 首启与项目接入 (GUI).html` 是下级 onboarding，不是驾驶舱，
也不能替代 D1 成为 GUI baseline。以上所有相对路径均从
`docs/viden-design/Viden/` 起算。旧截图和 generated previews 只属于历史证据，不能
覆盖该层级。

## 历史兼容性

v0 lane TSV input、legacy flat cost shape、legacy approval boolean 和
`viden_core::legacy` bridge 都是 migration surface，不是新的 client API。Client 迁移到
schema `1` 与 CoreClient-only 边界时，保留历史 release evidence 不变。
