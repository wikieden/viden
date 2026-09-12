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
runtime.conflict_content
runtime.credential_handles
runtime.credential_staging
runtime.evidence_reads
runtime.lane_lifecycle
runtime.lane_owner_projection
runtime.operator_git
runtime.project_onboarding
runtime.recent_work
runtime.starter_lane_preview
runtime.structured_diff
runtime.trust_loop
runtime.workspace_eligibility
runtime.workspace_files
runtime.workspace_owner
ui.layout_preferences
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

结构化冲突内容需要 `runtime.conflict_content`（GUI-CORE-015）。没有任何新事件类型：
`ConflictBounce`、`LaneConflictView` 与 `LaneConflictDetected` 各新增一个可选的 `content`
字段，仅在存在时序列化。不带该字段发布的记录编码为它一直以来的字节，本 capability 之前
写下的 payload 反序列化为 `None` 且仍是**已知**事件，冻结 base fixture 保持其字节与摘要身份。

- 内容是两侧加上补丁的原像，仅此而已。`ours` 是对目标文件在该 hunk 声明的旧区间上的只读
  读取，并被钳制到文件范围内；`theirs` 是传入 hunk 的新侧行，位于其新起点；`base` 是该
  hunk 自身的原像，即补丁期望找到的文本。Core 不计算 merge base，也不做任何解决，因此这
  **不是**三方合并，客户端不得把它呈现为三方合并，也不得提供自动解决。
- 基线说明 `ours` 是相对什么读取的。当 merge gate 持有 canonical reviewed evidence 时为
  `Evidence { bindings }`，因为那正是评审者接受的内容，也是本次应用被授权的依据；Lane
  应用路径，以及 bindings 已无法校验的 gate，为 `Revision { sha }`，指名文件被读取时的
  commit；Core 无法指名任何基线时为 `Unknown`。`Unknown` 是一个真实答案，必须渲染为"未知"，
  绝不可悄悄当作 `HEAD`。
- 只列出严格应用真正拒绝掉的 hunk。拒绝之后的 hunk 从未被尝试；解析器无法解析的补丁，以及
  没有 hunk 形状的拒绝，都完全不产生内容；操作者的 `BounceMergeConflict` 携带 `None`，因为
  人的判断背后没有一次失败的应用。`None` 意味着 Core 无内容可展示，绝不是"冲突是空的"。
- `ConflictHunkReason` 由 Core 依据应用所见在一处分类：`ContextMismatch`、文件已包含该 hunk
  新侧时的 `AlreadyApplied`、`FileMissing`、删除补丁的原像不足以清空文件时的 `FileDeleted`，
  以及 `Binary`。本地应用路径不会产生 `Binary`，因为二进制文件没有可供拒绝的行；该变体是为
  能够看到二进制拒绝的应用路径保留的。客户端永不解析消息来决定提供什么操作。
- 边界为已发布行合计 256 KiB。越界文件保留条目，置 `omitted` 且不带 hunk，内容置
  `truncated`，因此"未展示"始终可与"这里没有冲突"区分。

操作者源码控制动作（`RunOperatorGitAction` -> `OperatorGitActionFinished`，其后跟随
`WorkspaceSourceUpdated`）需要 `runtime.operator_git`（GUI-CORE-020）。命令与事件都是新增的，
因此既有形状不变，冻结 base fixture 保持其字节与摘要身份。

- 每个动作恰好映射到一个既有的 agent 工具 spec：`Stage` 到 `git_add`，`Unstage` 到带
  `staged=true worktree=false` 的 `git_restore`，`Commit` 到 `git_commit`，`Push` 到
  `git_push`，`Fetch` 到新增的变更型 `git_fetch`。执行走同一个 tool registry，因此同一套
  `viden.toml` 规则同时约束操作者的提交栏与 agent 的 git 调用，Viden 中始终只有一份 git 实现。
- 门禁在任何进程启动之前先跑。格式错误的动作、越出目标的路径、未知或已归档的 Lane、
  plan mode、deny 规则以及被拒绝的审批，都以 `CommandRejected` 指名该命令返回，并把可操作的
  提示折进 reason。
- 门禁放行**之后**的失败是携带 `Failed` 结果的 `OperatorGitActionFinished`，绝不是拒绝：
  效果已被尝试，且该尝试已被审计。Core 在一处把 git 的 stderr 归类为 `NothingToCommit`、
  `NonFastForward`、`AuthenticationRequired`、`RemoteUnreachable`、`NoUpstream`、
  `PathOutsideRepository` 或 `Other`；客户端按类别渲染本地化文案，绝不解析输出文本。
- 授权审计记录在效果之前追加且 fail-closed，使用 `AuditObjectRef` kind `source`（Lane 目标
  另加 Lane ref），action key 为 `source.<verb>`。日志只追加，因此结果无法修改该记录：结果
  作为一条完成记录出现，通过 `attempt` 指名该授权，而完成事件携带的是授权 id。
- 分支没有 upstream 时，`set_upstream: false` 的 `Push` 会被判为 `Failed { NoUpstream }`
  而不执行。`git push <remote> <branch>` 本会成功并创建一个未被跟踪的远端分支，此后
  ahead/behind 无从得知，源码状态条会永远显示"已同步"。
- `pull`、`merge`、`rebase`、`commit --amend`、`reset`、force push、删除分支、`switch`、
  `checkout` 与 `stash` 被刻意排除。前三者移动 `HEAD` 并可能产生属于 Lane 冲突机制的冲突；
  随后四者重写历史，在已登记的 DiffReview 中既无入口也无撤销路径；`switch` 与 `checkout`
  会使已绑定 Lane 的、由 Core 拥有的 owner 绑定失效；`stash` 不在设计范围内。
- 完成事件是对单个命令的结论性回答，绝不折叠进 `RuntimeViewState`。其后的
  `WorkspaceSourceUpdated` 携带重新采样的源码事实并照常归约，因此只跟踪状态条的客户端
  仍能看到效果之后的工作树。

证据归档读取（`QueryEvidence` -> `EvidencePageLoaded`，`ReadEvidenceContent` ->
`EvidenceContentLoaded`）需要 `runtime.evidence_reads`（GUI-CORE-025）。两条命令与两个事件
都是新增的，因此既有形状不变，冻结 base fixture 保持其字节与摘要身份。没有该 capability 时，
客户端只能沿用 `RuntimeViewState.latest_evidence` 给它的内容，并且不得声称那就是归档。

- 数据源是持久归档，不是近期窗口。Core 在打开时从追加式 workflow agent 日志重建它——即携带
  `EvidenceRecorded` 的 `runtime_projection` 与 `runtime_projection_batch` 行——并对其分页。
  `latest_evidence` 是客户端对它自己所收到那条事件流的归约，没有排序规则、没有 cursor、也没有
  内容，因此对它分页等于用"你恰好看到了什么"回答"存在哪些证据"。
- 排序按 `(timestamp, id)` 升序，其中 timestamp 是 `EvidenceView.timestamp`。Core 从未标注
  时间的行排在**最前**，位于所有已标注时间的行之前：未标注时间是 Core 能诚实给出的最旧说法，
  因此向前分页的客户端会在开头恰好遇到它一次，而不是看着它出现在已被渲染为更新的行之后。
- Cursor 是不透明的。`EvidencePage.next_after` 是一个字符串，客户端原样作为
  `EvidenceQuery.after` 传回；客户端不得解析、构造或比较它。它是带标签而非按位的，因此
  "未标注时间"这个位置与 `timestamp = 0` 保持不同，且包含分隔符的 id 仍能往返。
- `owner` 是对 `RuntimeOwner` 的前缀作用域匹配，且两侧都 fail closed。指名 task 的查询不会被
  只知道自己 lane 的行满足；没有记录 owner 的行永远不满足带作用域的读取，因为用它来回答会把
  "Core 并不知道"变成"这条 lane 产生了它"。`kinds` 为空表示所有 kind，绝不是"什么都不要"。
- 所有过滤都在切页之前执行，因此 `complete` 与 `next_after` 描述的是**过滤后**的归档。这正是
  过滤属于契约而不是客户端工作的原因：客户端对手上已有的一页做过滤，看不到落在它从未加载的
  那一页上的匹配行，因此它的"该 lane 没有 `patch` 证据"是一个它并无依据的完备性断言。
- 内容只从 `EvidenceView.canonical` 指名的 canonical ContextStore 字节读取，并且只有在这些
  字节与该引用的 `source_hash` 校验通过之后才读取。没有 canonical 引用回答
  `Unavailable { SummaryOnly }`；无法打开的 store、缺失的 blob 或 handle、被拒绝的 scope 回答
  `Unavailable { MissingCanonicalBytes }`；未通过自身哈希的字节回答
  `Unavailable { HashMismatch }`；校验通过但非 UTF-8 的字节回答 `Unavailable { Binary }`；
  kind 为 `patch` 时经由结构化 diff capability 所用的同一个 `parse_diff_document` 回答 `Diff`；
  其余一律回答有界的 `Text`。
- **Core 绝不把未经校验的字节当作 canonical 提供。** 不存在用于"Core 无法校验的内容"的变体；
  `HashMismatch` 与其他所有 store 失败刻意分开，因为它是相反的事实：字节就在那里，而且正是
  评审者绝不该被展示的那些。Core 无法打开 store 时回答 `MissingCanonicalBytes` 而不是不匹配，
  因为什么都没有被比较过，声称不匹配等于凭空造事实。
- 门禁姿态与 `QueryAudit` 一致，刻意不同于 `QueryWorkspaceFiles`：有界且带 owner 作用域，
  绝不由工具门禁把关。证据归档是 Viden 自己的状态而不是操作者的工作树，因此这里没有需要授权的
  工作区读取，也没有任何 `git_*` 或文件工具的 `viden.toml` 规则能描述它；用工具 spec 来把关
  会凭空造出一条毫无意义的权限，并让一条针对文件系统写的规则遮蔽 Core 本就持有的事实。两个
  读取都不产生变更、不请求审批，因此在 Plan mode 下均可回答。
- 拒绝发生在回答之前并且指名。越界的 `kinds` 列表、本构建从未签发过的 cursor、以及从未记录过的
  evidence id，都以 `CommandRejected` 携带调用者自己的 command id 返回，绝不是一个会被客户端
  渲染成空归档的空 page。`limit` 像 `AuditQuery` 一样钳制到 `1..=200` 而不是拒绝，因为请求了
  过多行的客户端仍然表达了一个可回答的意思。
- 边界：默认每页 50 行、最多 200 行，最多 32 个 `kinds` 过滤项，内容 256 KiB——文本路径用
  `truncated`，diff 路径用文档自身的 `truncated`。
- 两个回答都是查询结果，都不折叠进 `RuntimeViewState`。这里的理由比审计 page 更尖锐：视图本就
  携带 `latest_evidence`，把归档 page 折进去会让一次对旧行的分页读取覆盖实时投影，而客户端将
  无法区分二者。

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
| `interaction-closed-loop` | 文件夹绑定但不隐式配置、reviewed Lane 创建、built-in 与 ACP adapter/session、共享审批、evidence/gate、apply conflict、typed recovery、重连 replay 与完成态 | `c43d9fe304c28a50349e441c643837a9899cccab79566f9c193838248df2da1d` | `a6f1c436a15f7c77a5410c3563d8c3f67c5a5a3864692de61db61623f93ed891` |
| `review-decision` | 独立评审结论：`ReviewRequestStatus` 由 `Pending` 迁移到 `Accepted`，携带 reviewer feedback 与被打戳的 gate validator，同时 gate 决定仍然独立 | `38f81bbc1966fbf5742b0087bdd9e871eb11d58cdee747628ed3f4ca1323713c` | `b8e0b5389c3f21be4b4f28cfeba8d902917a304c6b9252cf9911dcccb6146a2b` |
| `context-budgets` | 两条并发 Lane 各自携带精确绑定的 owner 与互不相同的 task-scoped budget，一条处于软压力、一条越过硬上限 | `1b251b312b05ef950cdfc8190347e848a38d92bdaf26fe7d196e1ba053fc667b` | `7fcbde9edc5aa1a40a5cd41b0a8442403c6424903cc754cbe64d45980389029f` |
| `streamed-turn` | 同一 session 与 message id 下的有序 `AssistantDelta` chunk 恰好重建最终回复，作为终止标记的完成事实不会重复追加 | `2567d9709e6ec96d621fa281acc205ba5a8fe0b8a08f5868b70ca386f70e3a7d` | `3b1129fb57860aa337c571a9f70be2eacca432c4419bfbb1f4c2943dd371b8e2` |
| `message-parts` | ACP turn 在文本之外返回 image part：typed part 只挂到自己的消息上，reference 是 parts 目录的不可变 digest 路径，未建模的 part kind 无损往返 | `0162e39121f8f9f4543970fdc8098580bc5e01e43786de56d50cc520baed32fe` | `2e7f430cf1694baedd4615c0b60faf9b5614475e52b3c1b365b69a22d0f3d0c2` |
| `audit-reads` | 两次并发 audit 读取以相反顺序被回答，每个 page 指名自己的 `command_id`；另有一次过滤读取，其 `complete` 描述的是过滤后的 timeline，而未过滤 page 上仍存在更旧的记录 | `389739e9f28cfaf1e1cc9632316760e60fc43495f3702a21d2944874027bb28e` | `a1bdc24b45fc015b9601cf30ae7916dedd5ee0d5bcd2bbc1b5792e2964ef07d2` |
| `owner-scoped-live-work` | 两个并发 Lane 在各自精确绑定的 owner 下交错发布 task、tool call、排队输入与 evidence 事实，另有同样四类事实在没有 owner 的情况下发布 | `6972686f93d9d2653fa3510a0f74c50d4b7905426ac0554362a07945ac2541d4` | `87dc66790932f819f84903b3efd457dca1c85e3992c862a44919d0fe5bdeefc2` |
| `audit-ordering` | 一页 newest-first 的 audit 记录横跨两个交错的项目，且有一对跨项目、时间戳相同的记录靠降序 audit id 定序 | `4da28fdd43503046033cf65b5362c2cdd482c42ade083bb32f45a014b942c842` | `6c7de7344afb54cf58793c5878672c435da848aea426e5e0a89253ee98d9c3e4` |
| `workspace-files` | 同一项目上的两次并发清单读取以相反顺序被回答，每个 page 都携带必填的 `command_id`；带 prefix 的读取对其子树返回 `complete`，而未加 prefix 的读取仍未完；另有第二个已挂载项目只发布 lane 事实、完全没有清单读取 | `9f1c95e59ff5c4a172791d8c0c862f6286326311853b228cc5b83674e4775c37` | `f907b793d2817372fc71c95122e4e33152755682fff5fe46aa20150704bcb949` |
| `structured-diff` | 一个 `edit_file` 审批，其决策上下文含一个文件、一个 hunk，以及其所基于字节的 `base_sha256`；一个 `MergeAgentPatch` 审批，携带两文件变更且不含基线哈希；一页 diff，其中一条为已暂存条目，另一条被字节边界省略但计数保持真实；以及第二次读取被 `CommandRejected` 以其自身 command id 拒绝 | `3f5f4caf39cee2c46162999a35618599c0197aae15a3abfdd3a098e080676b8a` | `e24915b31f4192d85349be99da4c0ea81b6fb0b126b2076de17775d70de21cd0` |
| `operator-git` | 一个 `Stage` 被策略拒绝，并以 `CommandRejected` 指名映射后的 `git_add` spec 返回；一个 `Commit` 走 ask 路径、携带已暂存行被审批并完成，其后跟随重新采样的源码事实，其中 `ahead` 前进且工作树干净；一个 `Push` 结论为 `Failed { NoUpstream }` 而不是被拒绝，因为门禁放行了它且该尝试已被审计 | `07eadb0e93c8e151ba5e3dd0f069035ff5e97ca20156b6f1f0c0e85a86ac14e1` | `f29aa213870cfd2e511553453b077db7f193dac553068e932c787ae0ba683936` |
| `conflict-content` | 两条 Lane 触及同一个文件：Lane A 的补丁合入，Lane B 的 `MergeAgentPatch` 被拒绝，bounce 携带一个 hunk，含 `ours`、`theirs` 与补丁原像，基线为 `Evidence`；另有一条 `LaneConflictDetected` 以相同形状携带内容，基线为 `Revision` | `d73ea2a144cd2682f3c5121f124c952dfad158e4befdd1a60ec9b9dc1c4d01bf` | `830afb77c04cf807926d0010309b07c3f1802e580daf48bc0715d4722c96ce1f` |
| `evidence-reads` | 两页以第一页发布的那个不透明 cursor 原样拼接同一份三行归档；一页按 kind 过滤、对其过滤而言 `complete`，而未过滤的归档并非如此；三次内容读取分别回答有界文本、`patch` 行的已解析 diff 行、以及仅供展示证据的 `Unavailable { SummaryOnly }`；另有一次越界 `kinds` 查询以 `CommandRejected` 回答且完全不发布 page | `b15cb2fd024f60a1abb3a5a39b5c5736fca8bec443ae1a99a69e99f51edf8ef9` | `4d33513151bda26aa8e11242a9963d7fde25cc332980a40306f6393e6fc4caa0` |
| `workspace-owner` | 铸造出的工作区身份作为快照之后的第一条事实发布；随后创建的 Lane，其绑定携带同样的两个 id；一次 Workspace 目标的 `Commit` 在该 owner 下结算并被审计；一次 Lane 目标的 `Stage` 以该 Lane 自己的 source 行作答，而工作区芯片仍描述工作区 | `0866370b2f4a9c85b1a577688e7cce42f51243b711440c7f3a033a5e75515a87` | `b8ac58db0fc8d3780d514e75531b21d98d7592e7e44b8aebd7bab69094777286` |
| `ui-layout-preferences` | 快照前缀的副本不带 command id；一条 Core 已应用但写入失败的记录；一次越界的隐藏段列表在写入任何内容之前按 command id 被拒；一次 reset 落回 pinned 默认值 —— 其中一个无法识别的段名被原样保留 | `4bd474eb181ac8bf8acb002627074fa80920a233be47d57dff0a1991d3a7c0e1` | `3dbde7428fba6a00be4fc771b12eb9b7efe01ec2c97cb016b36049ece8612908` |

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

2026-09-07 延后的评审跟进项，三项已于 2026-09-09 在 `claude/hygiene-h1`（`0.3.3`
批次 H1）全部关闭。在审计上述修复时确认了三处不一致，并有意留给后续独立改动
处理：三者都是客户端内部清理，不影响契约。记录于此，以免被当作新发现重新提出：

1. **2026-09-09 关闭。** `CockpitProjection.assistant_stream`
   （`apps/tui/src/tui/projection.rs:26`，构建于 `:77`）是死字段：无人读取。TUI 直接
   从 `RuntimeViewState` 渲染该 stream，因此该字段是一份会与原件各自独立结算的副本。
   该字段及其构建已删除；本地 supervision fixture 矩阵改为点名该回合产出的 typed
   evidence，而不再引用投影已不再携带的回复文本。
2. **2026-09-09 关闭。** TUI 中存在两套「是否有活跃工作」的定义——
   `apps/tui/src/tui/app.rs` 用于命令路由，`apps/tui/src/tui/state.rs` 用于状态文本。
   路由判据读取 `agent_sessions` 而状态文本不读，因此一个尚未发布其他事实的 Agent
   回合在 composer 看来是忙、在状态行看来是闲。二者现在都调用同一个
   `state::runtime_has_active_work`，读取同一组事实。`assistant_stream` 刻意保留在
   该集合中：内建 provider 回合不发布 Agent session 也不发布 task，而 supervisor 在
   worker 线程上流式发出它的 delta，此时 composer 是活的，因此它是 Core 为该路径发布
   的唯一活跃性事实。该回合结束后残留的文本正是上文记录的限制；要消除它需要为内建路径
   提供回合活跃性事实，而不是让客户端去猜回合已结束。
3. **2026-09-09 关闭。** ACP merge gate 被两种标识键控：`crates/agents/src/acp.rs`
   以协议会话句柄发出开场的 `Proposed` 事实，而 `crates/agents/src/glue.rs` 用已发布的
   Agent session 为同一个 gate 的后续每次更新划定范围，于是一个受监督回合产出了两条
   gate 记录。源头现在用同一个 scoped id 构建开场事实。Gate id 对客户端不透明——没有
   生产代码解析它，也没有任何持久化交叉引用以它为键——已用协议句柄写入的 gate 在回放时
   保留原 id，并仍由 `tracked_agent_job_runtime_events` 中的 owner 回填绑定，该路径有
   一条基于旧日志的测试覆盖。

`structured-diff` fixture 是 `runtime.structured_diff`（GUI-CORE-012）的生成式证据。
它刻意不是顺利路径：一次读取被回答、一次被拒绝，被回答的 page 中一条带行数据，另一条
被字节边界剥掉了行。若客户端把空 page、被边界省略的条目与拒绝渲染成同一种样子，就会
三次说出"没有变更"，而这正是本 capability 要防止的失败。两个审批都出现，是因为单文件与
多文件生产者能够诚实主张的内容不同：`edit_file` 预览给出其读取字节的 `base_sha256`，
多文件补丁则不给出，因为一个哈希无法描述多个文件。这些行的唯一生产者是 Core——应用补丁
的那个 unified diff 解析器被提升为发布它们——因此客户端永不把 diff 文本解析成行。

`operator-git` fixture 是 `runtime.operator_git`（GUI-CORE-020）的生成式证据，它的存在
就是为了把三种回答彼此分开。一个 `Stage` 在任何东西运行之前被策略拒绝，以
`CommandRejected` 指名 `git_add`——即操作者自己的规则所约束的那个映射后 agent spec——返回，
并把提示折进 reason。一个 `Commit` 携带它将要变成提交的已暂存行到达审批坞，被解决、完成，
其后跟随重新采样的源码事实。一个 `Push` 以 `Failed { NoUpstream }` 结束，因为门禁放行了它，
且该尝试已被审计。若客户端把拒绝与失败渲染成同一种样子，就会让操作者去改权限规则来修一个
跟踪问题；若客户端从沉默推断成功，就会把一次从未离开本机的推送报告为已完成。

`conflict-content` fixture 是 `runtime.conflict_content`（GUI-CORE-015）的生成式证据。
两条 Lane 触及同一个文件，因为冲突只有相对于已经落地的东西才有意义：Lane A 的补丁合入，
Lane B 的补丁是针对 Lane A 替换掉的那段文本写的，因此被拒绝，bounce 于是携带文件现在的
内容、Lane B 携带的内容，以及 Lane B 期望找到的原像。Lane 应用路径在它旁边以相同形状回答。
两个基线刻意是不同种类：gate 的基线是它的 canonical reviewed evidence，Lane 的基线是其文件
被读取时的 revision，单一的基线字符串对其中之一必然是谎言。payload 中没有任何合并后的结果，
因此若客户端把它渲染成可解决的三方合并，就会提供一个 Core 从未产生过的自动解决。

`evidence-reads` fixture 是 `runtime.evidence_reads`（GUI-CORE-025）的生成式证据。它的四种情形
在朴素客户端里渲染出来完全一样——都是"没有证据"——而每一种都是不同的事实，这正是它们共用一个
fixture 的全部理由。被切断的一页在 cursor 之后还有行，并且指名了那个 cursor；第二页从那个确切
字符串恢复，而不是从一个重建出来的字符串，这使 cursor 的不透明性变得可测。按 kind 过滤的一页
对其过滤而言是 `complete`，而未过滤的归档并非如此，因此把 `complete` 读成关于归档的断言的客户端
会提前停止分页。一条 `task_summary` 行确实存在且没有 canonical 字节，因此是
`Unavailable { SummaryOnly }` 而不是空正文。而越界的 `kinds` 查询被拒绝且完全不发布 page，
因为空 page 与空归档无法区分。回放断言同时证明两个回答都停在 `RuntimeViewState` 之外：归约每一个
page 与每一个内容事件之后 `latest_evidence` 仍为空，这正是归档 page 不会覆盖近期窗口的保证。

0.3.3 契约增量已于 2026-09-10 落到 `claude/int-0.3.3`。上文的四项能力 ——
`runtime.structured_diff`、`runtime.operator_git`、`runtime.conflict_content`
与 `runtime.evidence_reads` —— 把对外通告的扩展集合从 19 项推到 23 项，并新增
语料表中列出的四个 fixture；九个冻结基线 fixture 的字节未变，
`scripts/tui-regression.sh` 中的能力计数门由 19 移到 23。两个客户端都已采纳
全部四项（GUI 批次 G1a/G1b/G2a/G2b，TUI 批次 T1a/T1b）。这只使 Core 成为
`0.3.6` **候选**。它不是不可变 checkpoint：checkpoint 的声明与
`crates/core/release-manifest.toml` 中 `component_version` 的提升属于 `0.3.3`
的发布步骤（E1），且在集成分支合并之前，此处内容尚未进入 `main`。

`runtime.workspace_owner`（GUI-CORE-027）为作用于工作区根目录的工作提供了一个
操作者身份。它由 `LocalCoreHost::open_workspace` 铸造：`workspace_id` 是 `ws_`
加上规范根路径 SHA-256 的前 16 位小写十六进制字符；`project_id` 从
`.viden/project.toml` 的 `[project] id` 读取，若不存在则在首次 open 时以
`prj_<token>` 铸造并写入该文件。两半刻意以不同方式获得。派生出的 workspace id
不需要任何存储，因此同一目录永远铸造出同一个 id，客户端也可以自行重算 —— 而移动
仓库会改变它，因为这个 id 指名的是一个**位置**，这一点被明说而不是被掩盖。
project id 是持久的那一半，是审计轨迹据以连接的那一半；文件中已有的 id 永不
改写：第二次铸造会把同一个项目的历史劈成两个彼此无法关联的身份。
`WorkspaceRuntimeOwnerBound` 同时携带这两者以及取值为 `existing` 或 `minted`
的 `project_id_origin`，于是操作者在审计轨迹里遇到一个新的 project id 时，能
分辨发生的是哪一种。

工作区 owner 是一个作用域，不是兜底值：它只携带这两个 id，其余一概没有；同时
指名 Lane、session、task 或 turn 的绑定会被生产者拒绝、被归约器忽略，而不是被
裁剪。该绑定是 `SnapshotUpdated` 之后的第一条事实，因此快照重放会带上它；而
`RuntimeViewState.workspace_owner` 在 Core 发布之前是缺席的：缺席意味着这个
Core 没有发布工作区身份，客户端据此对工作区目标的提交栏做能力门禁，而不是拿
`RuntimeOwner::default()` 顶替 —— 那个 owner 谁也不是。新的绑定从该身份出发 ——
supervisor 把两个 id 一次性折进那些两者皆空的信封 owner，因此 open 之后创建的
Lane 会携带它们 —— 而客户端已经指名的 owner 永不改写，自带 actor 的命令则完全
不动。`SourceTarget::Workspace` 的 `RunOperatorGitAction`，在其 owner 指名了已
发布的工作区时被接受；在它谁也没指名、或指名了另一个工作区时，在任何进程启动
之前被拒，拒绝理由中援引 GUI-CORE-027。同一能力还新增 `LaneSourceUpdated` 与
`RuntimeViewState.lane_sources`，使 Lane worktree 的分支与 ahead/behind 不再
覆盖工作区芯片：`WorkspaceSourceUpdated` 保持其原义 —— 仅指工作区根目录。没有
自己 worktree 的 Lane 就是工作区本身，因此没有行；空 map 意味着 Core 没有采样
任何 Lane，而绝不意味着每个 Lane 都是干净的。

`ui.layout_preferences` 持久化驾驶舱布局 —— Lane 侧栏模式，以及操作者隐藏的
环境类状态栏段 —— 写入与 `[ui]` 同一个用户配置文件中的 `[ui.layout]` 表。它是
一条独立记录、一项独立能力，而不是 `UiPreferences` 上的一个字段，原因只有一个：
`ResolvedUiPreferences` 会序列化进每一个 `RuntimeSnapshot`，在那里多加一个字段
会移动全部九个冻结基线 fixture 的记录摘要，而一个缺席即跳过的可选视图字段一个
也不会移动。两条记录对应两条命令，因此重置外观档案刻意把 `[ui.layout]` 原样
带过 —— 操作者重置主题时不该发现驾驶舱被重排。Core 已应用但写入失败的记录以
`persisted: false` 加上作为诊断的原因发布，与 `ui.preference_persistence` 一致；
隐藏段列表上界为 16 个名字、每个至多 64 字节，越界时拒绝而非截断，因为被截断的
列表会继续显示操作者要求隐藏的段，且没有任何事实说明这一点。Core 不认识的名字
原样保留：状态栏词汇属于客户端。这条路径没有权限提示 —— 该记录不授予任何权限，
只是安排某个客户端自己的窗口 —— 因此每一次拒绝都发生在写入之前，并以指名调用方
命令的 `CommandRejected` 作答。

0.3.4 契约增量的 C5 批次已于 2026-09-12 落到 `claude/int-0.3.4`。
`runtime.workspace_owner` 与 `ui.layout_preferences` 把对外通告的扩展集合从 23
项推到 25 项，并新增语料表中列出的两个 fixture；九个冻结基线 fixture 的字节未变，
`scripts/tui-regression.sh` 中的能力计数门由 23 移到 25。里程碑目标是 29 项；
每个批次按其新增量移动该计数。两个客户端都尚未采纳这两项 —— GUI 的提交栏与同步
芯片（G7）、TUI 的 `/git` 工作区行（T2）在 `0.3.4` 的后续批次 ——
`crates/core/release-manifest.toml` 保持其 `0.3.6` 的 `component_version` 与已
记录的 checkpoint，因为 Core `0.3.7` 的 checkpoint 由发布步骤（E2）一次性声明，
而不是逐批次声明。

2026-09-10 记录的未决跟进项。每一条都是在 `0.3.3` 各批次中确认、并被刻意留在
批次之外的，因此它们不会日后被当作新发现重新提出：

1. **严格 apply 会静默丢弃补丁中的二进制文件**（既有问题，C3 期间发现）。
   `crates/tools/src/patch.rs` 在 hunk 匹配之前就拒绝二进制文件，并且不为其
   报告任何内容，因此混合补丁会应用其中的文本文件，而对二进制文件只字不提。
   这也是本 apply 路径从不产生 `ConflictHunkReason::Binary` 的原因。修复方式
   是给出按文件的明示结果，而不是静默跳过。
2. **D10 的 `eventsUnavailable` 文案混淆了两种状态**（既有问题，H1 期间发现）。
   该字串说 Core 未发布审计时间线，但它必须以「能力确实缺失」为条件，而不是
   以「页面尚未加载」为条件。「未读取」与「未提供」是两种不同的事实，而当前
   文案对两者都读作后者。
3. **原生内建轮次没有轮次存活事实**（H1 期间发现）。内建本地 provider 不发布
   Agent session 也不发布任务，因此 TUI 的活动判定在该路径上依赖
   `assistant_stream` 的残留。关闭它需要 Core 提供轮次存活事实，而不是客户端
   的猜测。完整推理见上文延后跟进项的第 2 条。
4. **离线原生会话中的持久证据归档为空**（T1b 期间发现）。**已于 2026-09-10
   决策，作为 GUI-CORE-028 推迟到 `0.3.4`**；E1 发布证据回合在真实仓库上复现了
   它。任何经 `RuntimeSupervisor` 驱动的工作 —— 原生 Lane 回合、ACP 会话 ——
   都到不了 engine 归约中的 `EvidenceRecorded` 分支
   （`crates/runtime/src/session_lifecycle.rs:860`），而那是该归档唯一的写入点；
   因此一次原生工具变更只记录转录事实、一条实时 `WorkspaceChangeUpdated` 和一条
   瞬态 `tool_result` 行，完全没有归档的 `patch` 证据。于是对于一个记录里满是
   证据的会话，`runtime.evidence_reads` 会正确地回答一个空归档，两个客户端也
   如实渲染。决策是：这是 Core 的持久化缺口，而不是契约措辞或 `latest_evidence`
   作用域问题；跨 `runtime_loop` / `runtime_supervisor` / `session_lifecycle` 的
   接线超出 `0.3.3` 的风险预算。完整陈述、逐个生产者的引用与关闭条件见
   `apps/gui/contract-requests.zh-CN.md` 的 GUI-CORE-028。`0.3.3` 中未作任何
   改动。

2026-09-10 由 E1 发布证据回合新增的开放后续项。该回合在一个临时 Git 仓库上、
使用 `fallback` provider，通过 TUI 跑了一次真实任务。每一条都是复现出来的，不是
推断出来的；E1 中一条也没有修复。

5. **会话级排队的后续输入永远不会被执行**（Core，阻断级）。
   `RuntimeCommand::QueueFollowUp` 把内容压入
   `SessionEngine::queued_runtime_inputs`（`crates/runtime/src/runtime_contract.rs:643`）
   并重放进视图（`:1762`）。没有任何地方移除它，也没有任何地方执行它：
   `InputDequeued` 的唯一生产者是 Lane worker 自己的队列
   （`crates/lanes/src/lane_worker.rs:979`）。因此对内置路径而言，
   `RuntimeViewState.queued_inputs` 只增不减，排队的提示是 Core 发布出来却从不
   处理的事实。
6. **TUI 的输入框在一次会话余下的时间里不再提交**（TUI，阻断级，是第 5 条与上文
   第 3 条的后果）。**已在 TUI 修复（T1c，2026-09-10）。**
   `command_for_composer` 在 `state::runtime_has_active_work` 为真时一律路由到
   `QueueFollowUp`；而该判定在以下情况为真：`assistant_stream` 仍残留着一次已完成
   的内置回合的文本、任何 Lane 处于 `Draft`
   （`LaneStatus::is_active`，`crates/types/src/agent.rs:403`，而 Core 正是把
   starter Lane 留在这个状态）、或 `queued_inputs` 非空——按第 5 条，一旦有东西
   入队它就永远非空。实测：一次 fallback 回合完成之后，或创建一条 starter Lane
   之后，后续每条提示都入队且都不执行。GUI 从来不受这一半影响：它的输入框 `busy`
   以 owner 为作用域、基于 `turn_id` 与 Agent session 状态，并刻意排除 Lane
   生命周期状态（`apps/gui/src-tauri/src/projection.rs:1281`），这才是正确的判定。

   修复把 TUI 的判定按各调用方真正要问的问题拆开。`state::composer_target_busy`
   回答路由问题，并与 GUI 一样以 owner 为作用域：活跃工具调用、待决审批、活跃
   task，或已发布 owner 属于本次输入目标的活跃 Agent session，外加本客户端自己
   在途的原生回合。owner 缺失按会话作用域计——前端契约正是这样定义 owner 缺失，
   而内建 provider 发布的每一条事实都不带 owner。`state::has_active_work` 回答
   呈现问题——状态行的 `ACTIVE`、退出确认、`Ctrl-C`——且不以 owner 为作用域，
   因此一条正在跑自己回合的 Lane 仍然显示为「有事情在发生」。H1 的单一判定规则
   以蕴含关系的形式保留，并且是构造上成立的：呈现判定由路由判定计算而来，所以
   只要输入框入队，状态行就必然同意。两个判定都不再读取 Lane 生命周期状态、
   `queued_inputs` 或 `assistant_stream`。第 5 条仍然开放，本客户端只是不再把一个
   Core 不会执行的队列当作回合的证据。

   原生回合自身的活跃性窗口在 `apps/tui/src/tui/native_turn.rs`。它在输入框派发
   `SubmitUserInput` 时打开，并在本命令的 `CommandRejected`、派发之后的第一条
   `SnapshotUpdated`（`runtime_events_for_streaming_output` 为原生回合发出的终结
   批次的首事件），或一次快照替换时关闭。`SnapshotUpdated` 不是回合活跃性事实
   ——那正是第 3 条，仍然开放——因此在原生回合流式输出**期间**更改工作模式、
   权限级别或模型会提前关闭该窗口；此时下一条提示会被提交，Core 以
   `active runtime job … is already running` 作答，转录会渲染该答复。该模块把这个
   窗口写得很确切。第 3 条仍是真正的关闭条件。
7. **TUI 的 `/git` 选择器只可能看到工作区目标**（TUI）。
   **已在 TUI 修复（T1c，2026-09-10）。** `runtime.operator_git` 需要 Core 发布的
   Lane owner，而 TUI 的 Lane 选中态随 lane 详情面板一同消失：为了到达输入框
   （`/git` 是在那里键入的）而关闭该面板，选中态就被清空（状态栏上表现为
   `L:-`）。因此从输入框打开的 `/git` 一律以工作区为目标，四行全部禁用并标注
   `no workspace owner · GUI-CORE-027`。第一个成因背后还有第二个：环境性的 lane
   详情面板在渲染顺序上压过交互面板，因此即便选中态得以保留，看到的仍会是 lane
   检视面板，而不是选择器的行。第三个成因只有在前两个修好、能够走通实机流程之后
   才暴露出来：一条被选中且没有 session 的 Lane 会把客户端拉到 board 视图，而
   board 根本不渲染输入框（`apps/tui/src/tui/render.rs` 的 `render_frame`），
   于是保住的目标没有任何可供键入的界面。

   三个成因都已关闭。`TuiUiState.lane_detail_open` 现在与
   `TuiUiState.focused_lane` 分离，`Esc` 的回退链多了一级：浮层 -> lane 详情 ->
   Lane 目标 -> 插入模式。第一次 `Esc` 收起面板并说明该 Lane 仍是目标；第二次
   清除它并说明，这就是有据可查的清除方式。状态行的 `L:<lane>` 全程表示目标。
   交互面板在环境性 lane 详情之前渲染，因此操作者主动打开的选择器不会被没打开的
   面板遮住。board 视图由收起面板的同一级回退一并撤销，且
   `reconcile_ui_state_with_runtime` 在面板关闭之后不再把 Session 视图拉回
   board——两处都以 `lane_detail_open` 为条件，因此操作者正在查看该 Lane 时
   board 仍然优先，而按名字主动进入的视图不会被这一级动到。工作区目标的 027
   拒绝行为未变。

   由此带来一条诚实性后果：`RuntimeViewState.workspace_source` 是单一的工作区
   作用域视图，Core 不为每条 Lane 发布任何源状态，因此一个可达的 Lane 目标没有
   分支、领先/落后或脏状态事实。选择器的 TARGET 行会点名该 Lane 并声明源状态未知，
   而不是把工作区的数字印在 Lane 的名字旁边。按 Lane 的源视图是这个界面一旦存在
   就会使用的 Core 事实；此处不提出该请求，因为 `0.3.3` 没有任何东西被它阻塞。
8. **审批上的 audit id 不是一条持久审计记录**（Core）。`edit_file` 权限提示的
   固定审批面板会显示 `AUDIT audit_<id>`，但 `QueryAudit` 读取的持久时间线只由
   trust loop 与 operator git 动作追加
   （`crates/runtime/src/trust_loop.rs:1328`、
   `crates/runtime/src/operator_git.rs:122` 与 `:257`）。在一次被批准并已应用的
   原生工具变更之后，审计时间线正确地回答「Core 未为此作用域发布任何审计记录」，
   而屏幕上的 id 是一个实时关联 id，并不承诺有任何东西被写下。这与
   GUI-CORE-028 是同一种形状，只是高了一层：实时事实存在，持久事实不存在。
9. **EvidenceView 的报告可能在内容答案为 `HashMismatch` 的同时显示「已校验」**
   （GUI，表述问题但会误导）。F1 新增的两行携带的是 Core 对**证据记录**记下的
   判定；下方内容区携带的是**内容读取**自身对 `source_hash` 的哈希校验。这是
   两个不同的事实，而界面没有说明两者的关系。可见于
   `apps/gui/evidence/main-window-interactions/evidence-unavailable-1440x900-dark-en.png`。

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
