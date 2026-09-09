# GUI Core 契约请求

英文版：[contract-requests.md](contract-requests.md)

规划注记 2026-09-09（`docs/release-0.3.3-plan.zh-CN.md`）：012、015、020
经 `docs/release-0.3.3-contract-design.zh-CN.md` 中的 Core 契约增量排入
`0.3.3`，该增量同时为 EvidenceView 面开立 GUI-CORE-025（证据读取）。013 是
`0.3.3` 的 stretch，与创建流打包。009、018、019 移到 `0.3.4`；021 与 023
保持 `0.3.4+`。排期不等于关闭：每条只在其声明的条件于 `main` 上满足时才
关闭。

## GUI-CORE-008：所选 Lane 的上下文作用域 — 已关闭

历史：Core `0.3.5` 已暴露 `RuntimeViewState.context_budgets`，但 frontend-neutral
的 `viden-core` facade 尚未重新导出 `ContextBudgetRecord` 与 `ContextScope`。因此
GUI 无法在不重建私有序列化 schema 的前提下证明某个 budget 属于所选 Lane 的 task，
D1 只能把 `contextDock.context` 投影为 `null`；它从未任意选择 budget、反序列化猜测
的 scope 形状，也从未从展示文本推断用量。

Core 状态：已交付。`viden-core` 现在重新导出 `ContextScope` 与
`ContextBudgetRecord`，并由 facade 测试断言。schema-1 扩展 fixture
`context-budgets.json` 发布两条并发 Lane，各自携带精确绑定的 runtime owner 与互不
相交的 task-scoped budget，一条处于软压力、一条越过硬上限；其重放测试证明每条 Lane
的 task scope 恰好解析出一条 budget，且永远不是另一条 Lane 的。

GUI 状态：已在 `claude/core-contract-closures` 接线。D1 通过 Core 为所选 Lane 绑定的
精确 runtime owner 所指名的 typed `ContextScope::Task` 解析 `contextDock.context`，
并取该 scope 内最新的 budget；新旧比较绝不跨 scope。若没有精确 owner、没有 task id，
或该 scope 内没有 budget，仍然投影 `null`，而不是借用一条仅仅被发布过的 budget。
statusbar 的 context 段未改动，仍是其类型所记录的工作区级「最新 budget」粗粒度指示，
不是按 Lane 的数字。

## GUI-CORE-009：按 Owner 范围限定的类型化转录行

前端契约仅将 Lane 输出暴露为未类型化流，并暴露全局 assistant 流；它没有提供
按 Owner 范围限定且有序的 user/assistant 转录序列。因此 D1 仅为选中的精确
Owner 渲染类型化 Lane 输出，并将 user/assistant 行明确标为不可用；不得从展示
文本推断角色。

当 Core 发布包含稳定行 id、完整 `RuntimeOwner`、类型化 `user`/`assistant` 角色、
内容或不可变内容引用以及 replay/分页 cursor 的有序转录行时，关闭此请求。规范
D1 fixture 必须证明两个 Lane 的行不会跨 Owner 泄漏。

## GUI-CORE-010：按 Owner 范围限定的实时工作事实 — 已关闭

历史：在 frontend-contract-v1 中，`AgentTaskRecord`、活动工具调用、排队输入和证据
视图都不携带 `RuntimeOwner`。D1 因此省略这些全局事实并以该编码声明缺口；从不依据
时序或标签把它们归属给选中的 Lane。

Core 状态：已交付。`AgentTaskRecord`、`ToolCallView`、`QueuedInputView` 与
`EvidenceView` 各自新增一个 additive、optional 的完整 `RuntimeOwner`；
`RuntimeEventKind::ToolCallStarted` 也带上同一字段，因为 reducer 是从 event 而不是
envelope 折出该 view 的。字段缺省时不写入 wire，因此无已知 owner 的记录编码字节与
该字段存在之前完全一致，九份冻结基线 fixture 未变。

Core 只在发出点确实持有真实 owner 身份时才填充它：Lane worker 自身的绑定（Lane
排队输入）、Core 发布该 Agent session 时使用的 owner（该 session 的 tool call 与
evidence）、merge gate 自身的 owner（gate 绑定的 evidence）、以及随 durable agent
job 持久化的 owner（其 task 记录）。其余位置一律发布 `None`，含义是"Core 在发出时
并不知道 owner"——绝不是 default owner，也绝不依据时序、顺序或标签推断。特别地，
内建引擎自身的 turn 保持无 owner：它不绑定 Lane 或 Agent session，且 Core 不会自行
铸造 workspace/project 身份（见 GUI-CORE-023）。

schema-1 扩展 fixture `owner-scoped-live-work.json` 将其固化为规范：两个同时存活的
Lane 在各自精确绑定的 owner 下交错发布 task、tool call、排队输入与 evidence 事实，
另有同样四类事实完全没有 owner。其 replay 测试证明按选中 owner 的投影只解析出该
Lane 的四条事实，既不含另一个 Lane 的，也不含无 owner 的。

GUI 状态：已在 `claude/core-owner-facts` 接入。D1 通过与选中 Lane 的精确
`LaneRuntimeOwnerBinding` 做完整 `RuntimeOwner` 相等匹配来限定 `liveWork.tasks`、
`tools`、`queuedInputs` 与 `evidence` 的范围——与 context dock 对 workspace change、
permission dock 对 approval 采用的匹配纪律完全一致。owner 缺省或不一致的事实仍被
排除在 Lane 范围之外；没有精确 Core owner 的 Lane 完全不投影实时工作。
`live_work_scope` 不可用条目已移除。

TUI 接入不在本次关闭范围内。同样的字段对它同样可用，`apps/tui` 后续可以在不需要
Core 变更的情况下接入按 owner 限定的实时工作。

残留项，且 D1 工作状态条（`apps/gui/src/components/work_status.ts`）仍以该编码标注：
Core 未发布按 owner 限定的 turn**开始时间戳**，因此该条的计时以客户端观察到工作开始
的时刻为锚点并明示这一点；当 Core 未为选中 Lane 限定任何 Agent session 时，它仍退回
到未限定范围的状态标签。二者都不是实时工作事实，因此都不在本请求的关闭标准内；owner
字段并不提供它们，该条必须继续如实声明，而不是虚构开始时间或借用另一个 Lane 的状态。

## GUI-CORE-011：评审决定命令 — 已关闭

历史：`frontend-contract-v1` 曾发布带 `Pending` 状态的 `ReviewRequestRecord` 与
`RuntimeCommand::RequestReview`，但没有任何命令用于记录评审决定。因此 D2 会列出
待处理评审及其 Core 证据，并把接受/驳回动作以该编码置为禁用；从未用
`AcceptLaneOutput` 或审批响应冒充评审结论。

Core 状态：已在 `core-v0.3.2`（`a04260af`）交付
`RuntimeCommand::DecideReview { review_id, verdict, feedback, actor }`。只有独立
评审 Lane 可以做出决定；该结论结算评审事实，接受时为 gate validator 打上
`validated_at`；已结算的接受结论不会被后续 gate 决定覆盖；评审被驳回后
`AcceptMergeGate` 失败关闭。`ReviewRequestRecord` 新增可选且向后兼容的
`feedback` 字段。schema-1 扩展 fixture `review-decision.json` 证明
`Pending -> Accepted` 迁移。

GUI 状态：已在 `claude/gui-supervision-debts` 完成接线。D2 发送 `DecideReview`
时携带 `validate_review_decider` 接受的 actor，推导规则与 TUI 一致：优先回放 Core
在请求评审时写入 gate validator 的 owner（须指向该评审的评审方 Lane），否则复现
Core 自身的 `reviewer_owner_from_requester` 形状——评审 owner 改指评审方 Lane，
并清空 session 与 turn 身份。评审意见为可选，会先去除首尾空白，超过 Core 的 500
字符上限时本地拒绝而非截断。裁决只由携带本评审 id 且状态与命令一致的有序
`ReviewRequestUpdated` 确认；仅有 `CommandAccepted` 绝不构成确认，其后为 validator
打戳的 `MergeGateUpdated` 会被容忍。Core 会拒绝的评审保持禁用，并给出本地原因编码
而非本请求编码：已裁决为 `D2-REVIEW-SETTLED`，无法推导出可用评审方身份为
`D2-NO-REVIEWER-ACTOR`。

## GUI-CORE-012：审批的结构化决策上下文 — 已关闭（Core 侧，2026-09-09）

历史：`ApprovalRequestView` 只携带 `input_preview` 这一不透明展示字符串。D2 设计稿
要求按行渲染待执行变更的 diff。D2 原样渲染该预览并声明 diff 不可用，而不是把展示
文本解析成 diff 行。

Core 状态：由类型化结构化 diff（capability `runtime.structured_diff`）交付。
`ApprovalRequestView.decision_context` 携带 `DiffDocument`——带文件路径、两侧行号与
变更类型的有序 hunk，正是本请求要求的形状。Core 是其唯一生产者：**应用**补丁的那个
unified diff 解析器（`crates/tools/src/patch.rs`）被提升为发布该文档，因此应用路径与
所有客户端对"什么是一个 hunk"的理解一致，前端不再解析展示文本。对 `edit_file` 与
`write_file`，上下文由只读读取目标文件、在内存中计算拟议内容得到——审批时不写入任何
字节，这正是让"拒绝"仍然有意义的前提；对 trust loop 的 `MergeAgentPatch`，则来自 Core
已持有的规范补丁字节。后者就是本请求点名的多文件情形，schema-1 扩展 fixture
`structured-diff.json` 与单文件 `edit_file` 预览一起将其规范化。

两条限制被记录而非隐藏。`base_sha256` 命名单文件预览所基于的字节，因为执行时才会把
拟议的工具输入作用到当时的文件内容上；客户端或审计读者可以据此发现文件在期间发生了
变化，但 Core 在 `0.3.3` 中不会在执行前重新校验。多文件补丁完全不携带 `base_sha256`：
一个哈希无法描述多个文件，给出其中之一只会诱导客户端去校验错误的对象。

GUI 状态：2026-09-09（G1a）已接入读侧。DiffReview 视图在 D1 中央区以登记族
`.review > .filetree + .diffpane` 渲染 `QueryWorkspaceDiff` ->
`WorkspaceDiffLoaded`；D1 权限坞、D2 决策详情、D1 变更文件卡片共用同一个行渲染器
渲染 `decision_context` 与 `WorkspaceChangeView.diff`。当 Core 广告
`runtime.structured_diff` 时 D1 的 `diff` 不可用行被移除，否则保留；D2 的标记按单条
决策移除 —— 只对 Core 确实附了上下文的审批移除，因为 `shell` 与 `git_*` 族本就不带
上下文，那里的预览就是全部上下文。`base_sha256` 渲染为「预览基于 <8 位> 计算」，
陈述的是预览所依据的原像，而非「文件此后没有变过」的保证。

DiffReview 族画出的提交栏自 2026-09-09 起已经可用；动作侧以及客户端刻意不做的事见
GUI-CORE-020。统一/分栏切换中的「分栏」出于诚实仍然可见且禁用 —— 本版本只实现了统一
视图。冲突内容（GUI-CORE-015）属于另一个批次。

## GUI-CORE-013：待确认契约事实

`ContractRecord.decision` 只有 `Confirmed` 与 `Rejected`，因此发布出来的契约都已
决定。D2 设计稿展示的是等待人确认的契约队列。D2 把契约记录列为已决历史并给该分组
打上此编码；不得把已决记录当成待办积压。

当 Core 发布携带提议方、目标契约版本、订阅方与审计 id 的待确认契约事实，且规范
fixture 证明待确认契约经 `ConfirmContract` 转为已决时，关闭此请求。

GUI 状态，在 `claude/hygiene-h1` 上修正：契约**分组**已声明该缺口，但契约**详情**
仍对 Core 已裁决的记录渲染可点击的 Confirm 与 Reject 按钮。`RuntimeSupervisor` 的
`confirm_contract` 会拒绝已记录过的 id，因此这两个控件只可能得到一次拒绝。现在两个
裁决都投影为 `available: false` 并带上本编码，渲染为「禁用且标注」——保持可见，让操作者
看到裁决存在；保持不可点，让客户端不发出注定被拒的命令。当 Core 发布本请求要求的待确认
事实后，它们将改为依据记录自身状态判断，而不再是无条件不可用。

## GUI-CORE-014：视图状态中的有序事件日志 — 已关闭

历史：`RuntimeViewState` 只发布当前事实，没有有序事件日志。D10 设计稿展示跨项目的
书记官汇总事件流，D14 也需要同一份有序历史。D10 不渲染任何 ticker，以该编码声明
缺口；从未通过比对相邻快照重建时间线。

Core 状态：由追加式 audit timeline（`QueryAudit` -> `AuditPageLoaded`，capability
`runtime.audit`）交付。一条 `AuditRecord` 恰好携带本请求要求的事实：稳定的
`audit_id`、绝不本地化的稳定点分 `action` key、完整 `RuntimeOwner`、以及以秒计的
`timestamp`。分页按 `AuditRecord::cursor()`（即 `(timestamp, audit_id)`）newest-first
进行，因此该顺序是全序而不是按项目分组。schema-1 扩展 fixture `audit-ordering.json`
将其规范化：一页记录横跨两个交错的项目，其中一对记录**跨越项目边界**共享时间戳，因此
`audit_id` 的定序恰好在"按项目分组"或"平局时回退到到达顺序"的客户端会渲染出可见差异
的位置被检验。它是独立 fixture，而不是去改 `audit-reads.json`——后者三条记录同属一个
项目，且其字节已被登记。

GUI 状态：已在 `claude/core-workspace-files` 接通。D10 的 ticker 是该 timeline 的一页
有界 newest-first 记录（`D10_EVENT_TICKER_LIMIT` = 50），不加作用域以覆盖每个项目，
经由 D14 审计模式使用的同一个 adapter 槽位读取——两个屏幕是同一条 Core timeline 的两个
视图，且同一时刻只有一个被挂载。每行渲染 Core 自己的稳定 id、点分 action key、所属项目
与 Lane、以及时间戳；点分 key 绝不本地化，因为本地化后的时间线无法跨语言比对。行不携带
任何动作：ticker 是环境信息，可操作队列仍归决策中心所有。缺少 `runtime.audit`、读取尚未
回答、被拒绝、以及已回答但为空，四者保持四条不同的文案，因此空条不会被读成"从未发生过
任何事"。`d10.events.noOrderedLog` 那条 unavailable 行已移除。

## GUI-CORE-015：结构化合并冲突内容 — 已关闭（Core 侧，2026-09-09）

历史：`MergeGateRecord` 与 `ConflictBounce` 给出闸、原 Lane 与理由，但不携带冲突内容，
`LaneConflictView` 也只带一个 summary。D12 设计稿要求并排展示两条 Lane 的 hunk，因此
D12 只渲染 Core 的理由文本并声明 hunk 不可用，而不去读取 worktree、也不把理由字符串
解析成 diff 行。

Core 状态：已交付为结构化冲突内容（capability `runtime.conflict_content`）。
`ConflictBounce.content`、`LaneConflictView.content` 与 `LaneConflictDetected` payload
各携带一个可选的 `ConflictContent`：按文件给出严格应用拒绝掉的 hunk，按 hunk 给出位于
`ours_start` 的 `ours`、位于 `theirs_start` 的 `theirs`、作为 `base` 的补丁原像，以及
类型化的 `ConflictHunkReason`。没有任何新事件类型——两个挂载点都是失败应用本就会发布的
事件——因此既有形状不变，九个冻结 base fixture 保持其字节。

有两处答案比请求的措辞更窄，值得点明。请求要的是"ours/theirs hunk"；Core 发布的是两侧
**加上补丁原像**，并且明确不是三方合并。Core 不计算 merge base，也不做任何解决，因此 D12
必须渲染这三侧，不得呈现合并后的结果，也不得提供自动解决。基线是类型化的而不是字符串：
gate 持有 canonical reviewed evidence 时为 `Evidence { bindings }`，因为 gate 的基线是它的
bindings 而不是一个裸 commit；Lane 应用路径为 `Revision { sha }`；Core 无法指名任何基线时
为 `Unknown`，D12 必须把它渲染为"未知"，而不是悄悄当作 `HEAD`。

内容只会由一次真实的应用失败生成。操作者的 `BounceMergeConflict` 是带理由的人为判断，
背后没有失败的应用，因此其内容为 `None`；拒绝之后的 hunk 从未被尝试，因此不会被列出。
`None` 意味着 Core 无内容可展示，绝不是"冲突是空的"，因此内容缺失时应保留设计稿的
"不可用"标记，而不是渲染一个空冲突。

规范 fixture 是 `conflict-content.json`，而不是扩展后的 `merge-gate.json`，因为冻结的 base
fixture 必须保持字节不变。它正是请求所要的单文件两 Lane 冲突：Lane A 的补丁合入，Lane B 的
`MergeAgentPatch` 被拒绝、bounce 携带一个基线为 `Evidence` 的 hunk，旁边的
`LaneConflictDetected` 以相同形状携带基线为 `Revision` 的内容。

GUI 状态：已采纳，2026-09-09（G2a）。D12 把每条冲突记录被拒的 hunk 按 Core 自己的起始行
画成 OURS 与 THEIRS 并排，补丁原像作为可折叠的第三条，原因 chip 同时给出 Core 的归类与
操作者可以做什么。面板随每个冲突声明这是两侧加上原像而不是合并结果，整屏不提供合并后的
文本，也不提供解决控件。基线按 Core 的类型渲染 —— 证据绑定渲染为可打开该证据对象自身审计
轨迹的 chip，revision 渲染短 sha 并把完整值放进行 title，`Unknown` 渲染为未知 ——
`LaneConflictView.content` 使用同一面板，按选中闸涉及的 Lane 限定。四种缺失保持四句话：
capability 缺失、Core 未为该记录发布内容（点明操作者退回的契约）、`omitted` 文件、
`truncated` 载荷。旧的不可用标记不再引用本请求（它已关闭），改为点名
`runtime.conflict_content`，即仍可能缺失的那个东西。证据：
`apps/gui/evidence/main-window-interactions/` 下的 `d12-conflict-content`、
`d12-conflict-omitted` 与 `d12-conflict-none`。

有一处遗留，属于工程学而非契约：`viden-core` 再导出了 `ConflictBounce` 与
`LaneConflictView`，却没有再导出它们携带的 `ConflictContent`、`ConflictBaseline`、
`ConflictFile`、`ConflictHunk` 与 `ConflictHunkReason`，而 GUI 不得持有第二个 `viden-*`
依赖（`apps/gui/tests/architecture_boundary.rs`）。因此投影通过 Core 自己的规范 serde
编码读取该值，而不是指名这些类型。没有任何猜测，也不存在第二个解析器，但客户端无法像契约
设想的那样对 `ConflictHunkReason` 做穷尽匹配。把这五个名字加进 facade 的再导出列表即可
关闭它。

## GUI-CORE-016：Agent 消息的流式分片 — 已关闭

历史：ACP 适配器已经收到 `agent_message_chunk` 更新，但
`crates/runtime/src/agent_commands.rs` 只是把它们累加进一个局部字符串，在轮次结束
时发布单条 `AgentConversationMessageView`。没有任何有序事件承载部分消息，因此 GUI
无法边生成边渲染，只能显示已完成的整段。

D1 因此按整条消息渲染，并用工作状态条表达「仍在进行」。不得对一条已完成的消息伪造
打字机效果。

当 Core 发布携带会话 id、所属消息 id、追加文本与终止标记的有序分片事件，且规范
fixture 证明重放分片可精确重建最终消息时，关闭此请求。

Core 状态：已交付。`AssistantDelta` 携带可选会话 id，ACP 适配器在整个提示轮次内保持
同一个消息 id，reducer 因此增长单条归属明确的消息。schema-1 扩展 fixture
`streamed-turn.json` 将其固化为规范：其重放测试证明有序分片恰好重建最终消息、作为终止
标记的完成事实只结算该轮次而不再追加一份副本，以及未归属会话的 `assistant_stream`
保有完全相同的回复，供尚未支持 owner-scoped 对话的客户端使用。

2026-09-07 修订（评审发现 4）：上述句子原本承诺未归属会话的 `assistant_stream` 在
完成**之后**仍保有完全相同的回复。该承诺被收窄，而非撤销。该 stream 在回合**进行中**
保有完全相同的回复；终态 fact 现在会清空它，因为一个永不清空的 unscoped stream 会在
重放时把每个历史会话的回复串接成一整块无归属文本。结算之后，回复由完成 fact 的
`session.output` 和 owner-scoped `agent_conversation` 承载，二者都是每个 schema-1
客户端已经收到的。`streamed-turn.json` 重放测试同时断言两半：在最后一个终态前事件处
整条回复位于 stream 中，而应用完成 fact 之后 stream 为空。

## GUI-CORE-017：Agent 消息的非文本内容 — 已关闭

历史：`AgentConversationMessageView.content` 是单个 `String`，且
`acp_message_chunk_text` 只提取 `content.type == "text"`。因此当 ACP agent 返回图像块
时，到达客户端的只是一段声称「图已画好」的文字，背后没有任何图像事实——这正是操作者
看到的「agent 说画了，但什么都没有」。D1 只渲染 Core 发布的文本，从不合成附件。

Core 状态：已交付。会话消息携带类型化内容分部，`AgentMessagePart` 把分部挂到它所属的
消息上；内联字节按内容摘要写入 `.viden/agents/parts/`，因此引用不可变，字节同时作为
证据发布。Core 未建模的分部类型无损往返，而不是被丢弃。桌面外壳通过 `agent_content`
命令解析工作区引用——webview 无法直接打开工作区路径——并拒绝 parts 目录之外的任何引用。

schema-1 扩展 fixture `message-parts.json` 将其固化为规范：一次 ACP 轮次在文本之外
返回图像分部；两个分部都只挂到其事件指名的那条消息上，而同一会话中的第二条消息保持
无分部；图像引用是 parts 目录的摘要路径而非内联字节；未建模的分部类型重新编码后与
Core 发布的对象完全一致。

编写该 fixture 同时暴露并修复了一个真实的 wire 缺口：`agent_message_part` 此前不在
schema-1 已知 event type 列表内，因此每个分部都会退化为被隔离的未知事件，并在
snapshot 与 replay 中被丢弃。现在它已是已知 event type，并有 types 层往返测试覆盖。

## GUI-CORE-018：检查点的捕获与恢复

D6 渲染了「恢复检查点」这一恢复动作，但 schema 1 完全没有建模检查点：没有任何
`RuntimeCommand` 能捕获或恢复检查点，`RuntimeViewState` 中没有检查点记录，也没有
事件报告恢复结果。其余 D6 动作现在都已接到真实 Core 命令——`restart` 发送
`RetryAgentSession`，`close_lane` 发送 `StopLane`——因此 `checkpoint` 成为唯一背后
无物的动作。

GUI 将其投影为 `available: false`、code 为 `GUI-CORE-003`，且不挂载任何处理器。它不
得用重放模拟恢复、不得把会话回退到更早的游标，也不得把重新读取快照伪装成检查点
恢复。

当 Core 发布带稳定 id 与归属 owner 的类型化检查点记录、提供恢复命令、发出携带恢复
结果的事件，且规范 `frontend-contract-v1` fixture 覆盖一次「捕获后恢复」时，关闭此
请求。

## GUI-CORE-019：Always 审批作用域与 Edit 决定

`ApprovalScope` 只建模了 `Once`、`Session` 与 `RepoAllowlist`。权限坞的设计还提供
「Always」与「Edit」：Always 是跨会话、跨仓库的长期决定，Edit 则返回一条经修改的
命令重新走审批，而不是接受或拒绝原提案。二者在 schema 1 中都不存在，因此都渲染为
fail-closed 的 `GUI-CORE-003` 占位，且 `PermissionChoice::Always` 与
`PermissionChoice::Edit` 在构造任何命令之前就被拒绝。

这也让一处键盘分歧保持开启。设计把 `Shift+A` 指派给「Always」；GUI 仍将 `Shift+A`
绑定在 `repo_allowlist`——Core 实际接受的最宽作用域——而不是把可用快捷键绑到一个失效
动作上。

当 Core 建模持久的 Always 作用域及使其可安全授予的撤销路径、建模能让修改后的命令
重新通过同一审批门禁的 Edit 决定，且规范 fixture 覆盖两者时，关闭此请求。届时 GUI
将恢复设计规定的 `Shift+A` 绑定。

## GUI-CORE-020：面向操作者的 git 动作 — 已关闭（Core 侧，2026-09-09）

历史：驾驶舱标题栏可以显示工作区源码管理的样子——分支、ahead/behind、dirty——但操作者
无法对它做任何事。`RuntimeCommand` 没有建模 commit、push、fetch、stage 或 unstage；
`crates/tools` 中面向模型的 Git 工具是 `pub(crate)`：只有走权限门禁的 agent 轮次能触达
它们。因此 sync chip 以 `role=status` 元素而非按钮发布，「提交或推送」入口完全没做。

Core 状态：由类型化的操作者源码控制动作交付（capability `runtime.operator_git`）。
`RunOperatorGitAction { owner, target, action }` 覆盖针对工作区或某个 Lane worktree 的
`Stage`、`Unstage`、`Commit`、`Push` 与 `Fetch`，`OperatorGitActionFinished` 以类型化
结果回答它，其后跟随重新采样的 `WorkspaceSourceUpdated`。

接缝不是本请求提议的那一处，这个差别值得点明：每个动作不是走 lane executor 上新增的
类型化 effect，而是解析到执行它的**既有 agent 工具 spec**——`git_add`、`git_restore`、
`git_commit`、`git_push`，以及新增的 `git_fetch`——并经由 agent 调用所走的同一个 tool
registry 执行。这既满足了请求所要的（同一道权限门禁、同一份 append-only 事实），也带来
了它没有要求的一点：同一套 `viden.toml` 规则同时约束操作者的提交栏与 agent 的提交，
Viden 中始终只有一份 git 实现。

请求中的「含拒绝与冲突」由契约现在明确作出的一个区分来回答。在任何东西运行**之前**发生
的拒绝——格式错误的动作、越出目标的路径、未知 Lane、plan mode、deny 规则、被拒绝的
审批——是指名该命令的 `CommandRejected`。门禁放行**之后**的失败是携带 `Failed` 的
`OperatorGitActionFinished`，因为效果已被尝试且该尝试已被审计；Core 把 git 的 stderr
归类为 `NothingToCommit`、`NonFastForward`、`AuthenticationRequired`、
`RemoteUnreachable`、`NoUpstream`、`PathOutsideRepository` 或 `Other`，因此客户端按类别
渲染本地化文案，永不解析输出。schema-1 extension fixture `operator-git.json` 把三者都
固化为规范：一次被拒的 `Stage`、一次被审批并完成的 `Commit`，以及一次失败的 `Push`。

两项行为被记录而非留作隐含。分支没有 upstream 时，`set_upstream: false` 的 `Push` 判为
`Failed { NoUpstream }` 而不执行，因为 `git push <remote> <branch>` 会创建一个未被跟踪的
远端分支，此后 ahead/behind 无从得知，sync chip 会永远显示「已同步」。以及 `pull`、
`merge`、`rebase`、`commit --amend`、`reset`、force push、删除分支、`switch`、`checkout`
与 `stash` 被刻意排除，每一项的理由都记录在前端集成契约中；`pull` 待 `0.3.4` 冲突内容
能够呈现其结果后再议。

GUI 状态：已采纳，2026-09-09（G1b）。DiffReview 提交栏与标题栏同步芯片都会动作。每个
控件通过 CoreClient 缝隙发送一条带客户端自选 `command_id` 的 `RunOperatorGitAction`；
只有点名该 id 的有序事件才能结算它；同一时刻只有一个动作在途。客户端渲染本请求的答案
所区分的三件事实，并且从不把它们合并：`CommandRejected` 是 Core 效果前的原话，逐字放进
alert；`Failed` 结果是该类别的本地化句子加上 git 的 `detail` 原文；`Completed` 的成功行
由结果中**重新采样**的 `source` 构成，而不是由 git 的输出得出 —— 输出收折在折叠区之后，
并带自己的截断说明。提供的恢复动作就是本契约为该类别指名的那一个 —— `NonFastForward`
给 fetch、`NoUpstream` 给「推送并设置 upstream」—— 而 `AuthenticationRequired` 不给，
因为那里的重试按钮只会变成针对本客户端无法提供的凭据的重试循环。`Ask` 以普通的、按
owner 限定并带 `target.kind = "git"` 的 `ApprovalRequested` 抵达既有权限坞；提交栏说明
它在等哪个动作，并在坞给出结论之前保持不可用。

按本契约，客户端**不做**的事：不做 pull、merge、rebase、`commit --amend`、reset、强制
推送、删除分支、switch、checkout 或 stash —— 界面上任何位置都不提供，且同步芯片的提示在
两种语言里都写明 pull 被排除，而不是让操作者去猜为什么一个「同步」控件只会 fetch。它不
自己驱动 `git`，也不退化成 shell 命令。它不从 `output` 推断成功；在提交回报 `Completed`
之前绝不发送「提交并推送」的后一半 —— 提交失败或被拒绝会中止这一对并如实说明。

有一条客户端本地的限制值得记录，而不是留给以后重新发现。`RunOperatorGitAction` 要求
`owner` 与信封 owner 一致，而 Core 按 Lane 发布 owner 绑定，因此本客户端只以 Core 为
驾驶舱当前选中 Lane 绑定的那个精确 `RuntimeOwner` 代行。没有精确绑定 Lane 的驾驶舱就
没有可指名的执行身份：此时提交栏与芯片禁用并标注客户端本地编码
`D1-OPERATOR-GIT-OWNER`，而不是发送 `RuntimeOwner::default()` —— 那会把一次已授权的
变更记成「不属于任何人」。这不是重新打开一条 Core 请求 —— 该能力完全按规范工作 —— 但
一个工作区级的操作者身份会消除这条限制，未来的请求也会是这个形状。

D1 的 `apply` 不可用行不再是无条件的：只有在 Core 构建确实没有发布
`runtime.operator_git` 时它才保留，并且直接指名那项能力，而不是暗示本条目仍然开启。

## GUI-CORE-021：Pull request 与 forge 状态

设计稿的标题栏与 Lane 界面带有一行「Pull request status」：分支是否有开启的 PR、
其评审状态与检查结果。schema 1 完全没有建模 forge——没有 remote、没有 pull request
记录，也没有来自工作区之外的评审或检查状态。`CheckRunView` 是本地检查运行器，不是
forge 的 CI；`MergeGateView` 是 Viden 自己的门禁，不是远端的合并状态。

因此 GUI 不渲染 PR 行，也不渲染 forge 徽标。它不得从分支名推导 PR、不得用
`WorkspaceSourceView` 的 ahead/behind 计数推断 remote，也不得直接调用 forge API：
访问网络服务的前端已在客户端边界之外，而 forge 凭据属于 Core 已经拥有的那条凭据
路径。

当 Core 发布类型化的 forge 状态记录——Lane 分支与远端 pull request 的关联、其评审
状态与检查结果——连同使拉取安全的凭据与数据外发策略，且规范
`frontend-contract-v1` fixture 覆盖「有 PR 的分支」与「无 PR 的分支」时，关闭此请求。

## GUI-CORE-022：工作区文件清单 — 已关闭

历史：命令面板移植了 TUI jump 索引的 `~` 选择器，而 TUI 本身也有同样的缺口：
`RuntimeViewState` 承载 Lane、会话、合并闸与审批，却没有工作区文件的清单。既没有
类型化的路径列表，没有搜索索引，也没有任何一次读取能让前端在不自己遍历文件系统的
前提下枚举目录树——而自行遍历已在客户端边界之外，还会绕过管辖其余所有路径读取的
权限门禁。两个客户端当时都把「文件」分区渲染成恰好一行、点名本请求的永久禁用行；
从未遍历工作区、从未 shell out 调用文件列举工具，也从未从证据记录或工具入参预览里
偶然出现的路径拼出一棵树。

Core 状态：以 `RuntimeCommand::QueryWorkspaceFiles` -> `RuntimeEventKind::WorkspaceFilesLoaded`
交付，扩展 capability 为 `runtime.workspace_files`。它在源头即受权限门禁管辖：Core 在读取
任何一个目录项之前先咨询权限引擎，工具名为非变更的 `workspace_file_inventory`，输入路径为
工作区根——与其他所有工作区读取同一道门禁。deny 会以 `CommandRejected` 返回，指名这次确切的
读取并携带拒绝原因；未解决的 ask 同样如此，因为这次读取回应的是一次按键而不是一次交互回合，
不得把客户端卡在审批弹窗后面。两者都绝不发布空 page："你无权读取"与"这个工作区没有文件"是
两个不同的事实；两者也都不是不带 command id 的裸 `Error`——那会让有读取在途的客户端把无关的
lane 或 provider 失败归给自己这次读取，渲染出 Core 从未发出的拒绝。该工具不
产生变更，因此 plan mode 仍会通过权限引擎的 safe-read 分支给出答案。

遍历遵循 gitignore（即便不在 Git 仓库内也遵循，且不读取全局或父目录的 ignore 文件，因此同一
工作区在两台机器上枚举结果一致），并无条件排除 `.git/`、`.viden/`、`.omx/`、`.worktrees/`、
`.ref/`——它们承载运行时与 agent 状态，而不是工作区内容。条目先按字典序排序，**然后**才应用
prefix 过滤、排他的 `after` 游标与 `1..=500` 的 limit 钳制，因此 `complete` 与 `next_after`
描述的是过滤后的有序清单；越出工作区的 prefix 会被拒绝，而不是被钳制或以空 page 回答。
`WorkspaceFilesLoaded.command_id` 为必填而非 optional，因此不像 audit page 那样存在无法关联
的情形，也没有"以自己被接受的查询做关联"的退路。schema-1 扩展 fixture
`workspace-files.json` 覆盖本请求要求的两半：一个有清单的项目（两次并发读取以相反顺序被回答，
只有必填的 command id 能正确归属），以及第二个只发布 lane 事实、完全没有清单读取的已挂载项目。

GUI 状态：已在 `claude/core-workspace-files` 接通。面板的 `~` 作用域按 Core 自己的字典序列出
Core 发布的路径，在面板打开时经由一个仿照 `PendingAuditPage` 的单槽位待决读取读取一次。TUI 的
jump 索引通过 `apps/tui/src/tui/workspace_files.rs` 做同样的事，每棵已加载的树只读一次。两个
客户端都不遍历任何东西。缺失、读取中、被拒绝与为空，在两个客户端里保持四条不同的行——缺少
capability 时保留点名本请求的诚实禁用行且零命令发送，被拒绝时逐字显示 Core 自己的句子，已回答
但工作区为空时明说工作区为空，而不是借用"不可用"那句话。

## GUI-CORE-023：并发多工作区托管

Core 一次只托管一个工作区。`LocalCoreHost::open_workspace`
（`crates/core/src/host.rs:166-234`）每次调用都会新建一个 `RuntimeSupervisor`，
而桌面宿主只是替换它那唯一的 `Mutex<Option<GuiCoreAdapter>>` 槽位
（`apps/gui/src-tauri/src/lib.rs:79-94`）。因此打开一个项目会**替换**当前打开的
项目：旧 supervisor 被 drop 时（`crates/runtime/src/runtime_supervisor.rs:1373-1404`）
会 join 其工作线程并关闭所有常驻 ACP 会话，旧工作区里正在运行的每条 Lane 与
Agent 都会停止。

这与设计稿画的不一致。`WorkspacePanel` 渲染了多个 `.wsroot` 项目分组以及一个跨项目
的「Global」lane 分区，`ProjectPicker` 的「工作区内」一列带项目计数和每项目 lane
计数，D13 展示的是跨项目的舰队视图。在单根 supervisor 之上，这些都无法表达。

在 Core 发布多根托管之前，GUI 守住以下边界：

- 侧栏只渲染一个分组——当前打开的项目——不伪造同级项目，也没有「Global」分区；
- 选择器的「工作区内」一列只有一行，标记为当前项且不可点击，因为工作区内部没有可
  切换的目标；
- 其他任何项目都是一次**切换**，必须先经过内联确认，确认文案点名将被拆除的正在运行
  的 Lane 与 Agent 会话数量；
- `克隆仓库…` 与 `新建空项目` 渲染为禁用并点名本请求：`frontend-contract-v1` 没有
  发布仓库克隆命令，也没有项目脚手架命令，而 GUI 不得自行 shell out 调用
  `git clone` 或写出项目骨架——这两者都是绕过 Core 权限门禁的变更。

当 Core 发布以下内容时，关闭此请求：

1. **N 根托管**——同时托管多个工作区，`open_workspace` 变为叠加语义（或获得显式的
   替换标志），而不是静默替换；
2. **项目注册表**——把已挂载的根集合作为 `RuntimeViewState` 中类型化、有序的事实
   发布，使侧栏分组来自 Core 而不是来自单一的 `environment.cwd`；
3. **跨项目 Lane 枚举**——可跨已挂载根读取 Lane、会话、闸门与审批事实，这正是设计稿
   的「Global」分区与 D13 舰队看板真正展示的内容；
4. **`RuntimeOwner.project_id` 推导规则**——从规范根到 `RuntimeOwner` 已携带的
   `project_id` 的稳定且有文档的映射，使客户端无需按路径做字符串匹配即可把
   owner 绑定事实归属到项目；
5. **项目初始化命令**——把「克隆进工作区」与「脚手架新建项目」作为 Core 命令发布，
   并与其他所有变更走同一道权限门禁，这才是解锁两行禁用选项的前提；
6. **规范 `frontend-contract-v1` fixture**，覆盖单根与双根两种情况，让分组侧栏与跨
   项目舰队拥有生成的证据而非手写投影。

最近工作清单（`runtime.recent_work`）**不是**这个缺口：它已经能让客户端列出可以打开
的项目。缺的是同时打开多个项目的能力。

## GUI-CORE-024：AuditQuery 过滤器与 AuditPageLoaded 的 command id — 已关闭

D14 在 `runtime.audit` capability 之下，通过
`RuntimeCommand::QueryAudit` -> `RuntimeEventKind::AuditPageLoaded` 读取 Core 的
审计时间线。该契约有两处缺口，直接决定客户端能诚实实现到什么程度。

**1. `AuditPageLoaded` 不携带 command id。** 客户端只能以 acceptance-first 方式关联：
只有在 Core 接受了*本次* `command_id` 之后到达的页面才被采纳，并且本地拒绝第二个并发
读取，使在途读取始终至多一个。这排除了「在我们被接受之前就到达的页面」，但*另一个*
客户端并发查询同一个 Core 时，其页面仍可能落在我们的 acceptance 与 Core 答复之间，从而
被归属到本次读取。该页面依然是真实的 Core 页面，屏幕重新查询时即被丢弃；而替代方案
（按记录内容匹配，或从 cursor 猜测）会凭空制造契约并不提供的确定性，因此不予实现。
TUI 在 `apps/tui/src/tui/audit_panel.rs` 记录了同一限制。

**2. `AuditQuery` 没有 actor 与时间过滤。** 其过滤项只有 `project_id`、`lane_id`、
`object` 与 `before` cursor。D14 设计稿展示了 actor 与时间范围过滤 chip 以及按天分组；
客户端只能对已加载的页面做过滤，而当匹配记录位于尚未加载的页面时，这会静默地误报
「该 actor 没有记录」。因此 D14 两者都不提供，而不是提供一个会谎报完整性的过滤器。

当 Core 发布以下内容时，关闭此请求：

1. **`AuditPageLoaded` 上的 command id**，使页面可归属到发起它的确切读取，并使并发
   读取者不再构成关联风险；
2. **服务端 `AuditQuery` 过滤器**，覆盖 actor（operator / system / 指定 agent）与时间
   范围，且在分页之前应用，使 `complete` 与 `next_before` 描述的是过滤后的时间线；
3. **规范 `frontend-contract-v1` fixture**，覆盖两个并发审计读取与一个过滤页面，让
   关联与过滤拥有生成的证据而非手写投影。

无需 Core 改动、因而**不属于**本请求的客户端后续项：对已加载页面做按天分组、选中记录
的详情侧栏、已加载页面的汇总。导出需要的是宿主文件写入路径，而不是 Core 契约新增。

Core 状态：三条关闭条件全部交付。

1. `AuditPageLoaded.command_id` 就是该页面所回答的那次 `QueryAudit` 的 command id，
   由命令处理器直接透传，任何其他路径都不会铸造它。该字段是 additive optional，因此
   早于该字段的 Core 发出的页面会反序列化为 `None`。
2. `AuditQuery` 新增 `actor`（`AuditActorFilter`：operator / system / any_agent /
   指定 agent，按完整 id 精确匹配）与半开区间 `[from, until)`（unix 秒）。Core 在分页
   之前应用它们，因此 `complete` 与 `next_before` 描述的是过滤后的时间线。区间倒置会被
   拒绝而不是返回空页面；本次构建无法归类的 actor 或 filter 变体一律不匹配。
3. schema-1 扩展 fixture `audit-reads.json` 在回答任一读取之前先接受两次读取，并以相反
   顺序返回它们的页面——因此按到达顺序归属会归错，只有已发布的 id 能归对——另有一次过滤
   读取，其 `complete` 描述的是 agent 时间线，而未过滤页面上仍能看到严格更旧的 operator
   与 system 记录。

关闭本请求时还修复了与 GUI-CORE-017 同形的真实线上缺口：`audit_page_loaded` 此前不在
schema-1 已知 event type 集合中，因此在任何序列化 snapshot/replay 路径上，每个审计页面都
会退化为被隔离的未知事件。它现已是 known event type，并有 types 级往返测试。

客户端状态：两个客户端均已采用精确关联。GUI 宿主
（`apps/gui/src-tauri/src/adapter.rs`）与 TUI 面板
（`apps/tui/src/tui/audit_panel.rs`）都要求已发布的 `command_id` 等于自己在途的读取，
忽略指名其他读取的页面，并仅对不带 id 的页面保留 acceptance-gated 回退。两者都不伪造 id。

剩余客户端后续项（不被 Core 阻塞）：基于新 `AuditQuery` 字段的 D14 actor 与时间范围过滤
chip。目前没有客户端发送 actor 或时间过滤，因为还没有让操作者做出选择的控件。

## GUI-CORE-025：证据读取 — 已关闭（Core 侧，2026-09-09）

请求：已注册的 `EvidenceView` 界面是一份按天分组的归档，外加详情侧栏——键值报告、输出尾部、
由 `metadata` 与 `canonical` 链出的 chip，以及 `patch` 行的"在评审中打开"。前端今天唯一能拿到的
证据是 `RuntimeViewState.latest_evidence`，它无法承载这个界面，原因有三条，且客户端都无法绕过：

1. 它是**近期窗口投影**——客户端对它自己恰好收到的那条事件流的归约。中途连接的客户端，或从序列
   缺口恢复的客户端，持有的是严格子集，因此"这是最近 N 条证据"是一个它并无依据的关于归档的断言。
2. 它**没有排序规则、也没有 cursor**。它是按到达顺序 upsert-by-id 的列表，因此按天分组意味着
   GUI 自行发明一套排序，分页意味着 GUI 自行决定一页在哪里结束——归档于是有两个定义，其中一个
   在 Core 之外。
3. 它**不携带内容**。`EvidenceView` 指名了 canonical 引用但从不给出字节，要渲染报告或输出尾部，
   唯一的办法是 GUI 自己打开 ContextStore——那在客户端边界之外，并且绕过了哈希校验，而哈希校验
   正是 canonical evidence 之所以有意义的全部原因。

Core 状态：已交付为 `runtime.evidence_reads`。`QueryEvidence` -> `EvidencePageLoaded` 对 Core 在
打开时从追加式 workflow agent 日志重建的持久归档分页，`ReadEvidenceContent` ->
`EvidenceContentLoaded` 回答单条行背后的字节。两条命令与两个事件都是新增的，因此既有形状不变，
九个冻结 base fixture 保持其字节。

针对上面三点的回答，依次是：排序按 `(timestamp, id)` 升序，Core 从未标注时间的行排在**最前**
——未标注时间是 Core 能诚实给出的最旧说法，因此向前分页的客户端在开头恰好遇到它一次，而不是在
已被渲染为更新的行之后。Cursor 不透明——`next_after` 原样作为 `after` 传回，GUI 不得解析、构造或
比较它——并且所有过滤都在切页之前执行，因此 `complete` 描述的是过滤后的归档而不是原始归档。
内容只从 canonical ContextStore 字节读取，并且只有在它们与该行自身的 `source_hash` 校验通过之后。

有两处答案值得点名，因为它们比"把内容给我们"更窄。`EvidenceContent` 没有用于"Core 无法校验的
字节"的变体：没有 canonical 引用的行为 `SummaryOnly`，字节已消失的为 `MissingCanonicalBytes`，
字节在场却未通过自身哈希的为 `HashMismatch`，校验通过但没有文本或 diff 形状的为 `Binary`。
其中 `HashMismatch` 永不作为内容提供——那些正是评审者绝不该被展示的字节——因此详情侧栏在那里
渲染的是一个类型化的原因而不是正文。门禁姿态与 `QueryAudit` 一致而非 `QueryWorkspaceFiles`：
有界且带 owner 作用域、绝不由工具门禁把关，因为归档是 Viden 自己的状态而不是操作者的工作树。
两个读取在 Plan mode 下均可回答。

规范 fixture 是 `evidence-reads.json`：两页以第一页发布的那个确切 cursor 拼接同一份三行归档；
一页按 kind 过滤、对其过滤而言 `complete` 而未过滤的归档并非如此；三次内容读取分别回答文本、
已解析的 diff 行与 `Unavailable { SummaryOnly }`；以及一次越界 `kinds` 查询以 `CommandRejected`
回答且完全不发布 page。这四种情形在朴素客户端里渲染出来完全一样——都是"没有证据"——这正是它们
共用一个 fixture 的理由。

GUI 状态：尚未采纳。EvidenceView 仍渲染 `latest_evidence` 给它的内容；按天分组的归档、详情侧栏、
链出的 chip，以及进入 DiffReview 的"在评审中打开"，将随 EvidenceView 批次（G2）落地。
TUI 状态：`0.3.2` 监督检查点上推迟的证据检视器，从决策浮层打开，随 T1 落地。

## GUI-CORE-026：平台凭据录入

`RuntimeCommand::StoreCredentialHandle` 已存在，并要求一个
`credential_request_id`，但没有任何路径会发布它。`CredentialRequestId` 被文档化
为「不透明、一次性」，只在受信本地宿主边界内铸造，而 schema 1 没有暴露任何暂存
密钥并返回该 id 的 command、event 或宿主接口。GUI 若要填上该字段，只能在客户端
接收原始密钥并自行铸造 id——这会把密钥字节放进前端，并伪造一个 Core 从未签发的
身份。

因此 D11 将 `credentialIngress` 投影为 `available: false` 并标注本编码，且在构造
任何命令之前就拒绝 `D11Intent::StoreCredentialHandle`
（`apps/gui/src-tauri/src/adapter.rs`）。基于同一理由，Settings 不绘制各 provider
的 API key 标记，也不绘制 add-provider 动作。

当 Core 发布以下内容时，关闭此请求：一条不让密钥穿越前端边界即可返回
`CredentialRequestId` 的暂存路径、报告其结果的事件，以及一个覆盖「暂存凭据成为
`CredentialHandle`」与「暂存被拒绝」两种情形的规范 `frontend-contract-v1`
fixture。

## 已退役的前登记编码

`GUI-CORE-001` 到 `GUI-CORE-007` 早于本登记册，本册从 008 开始。它们从来不是这里
的条目，因此投影引用其中之一时，读者无从查证。它们的去向：

| 退役编码 | 原含义 | 今天 |
| --- | --- | --- |
| `GUI-CORE-001` | typed project intake、config preview/confirm、masked credential handles | 项目接入已交付；凭据部分为 GUI-CORE-026 |
| `GUI-CORE-002` | lane lifecycle commands 与 starter-lane 创建 | 已交付；D4 与 starter-lane 路径发送 Core 命令 |
| `GUI-CORE-003` | 结构化连接与 lane 恢复事实 | 大部分已交付；保留为 schema 1 未建模的 D6 动作所**渲染**的 fail-closed 编码，其开放请求是 GUI-CORE-018（checkpoint）与 GUI-CORE-019（Always / Edit） |
| `GUI-CORE-004` | 稳定的 append-only 审计时间线 | 已交付；以 GUI-CORE-014 与 GUI-CORE-024 关闭 |
| `GUI-CORE-005` | 偏好设置的变更与持久化 | 已作为 `ui.preference_persistence` capability 交付 |
| `GUI-CORE-006` | 结构化 diff / test / apply / conflict / retry 事实 | 已拆分：diff 为 GUI-CORE-012，冲突内容为 GUI-CORE-015，操作者 apply 与 commit 为 GUI-CORE-020；check run 已交付 |
| `GUI-CORE-007` | 分页的 recent project / session 历史 | 已作为 `runtime.recent_work` capability 交付 |

`GUI-CORE-003` 是唯一仍由生产投影发出的编码。这是刻意的，并记录在 GUI-CORE-018 与
GUI-CORE-019 中：这两条条目点名它为「动作背后的能力缺失期间 GUI 渲染的编码」。其余
投影现在都引用上表之后的条目。

## 客户端本地原因编码

并非每个不可用控件都是 Core 缺口。客户端因自身理由拒绝提供的控件，携带屏幕作用域的
编码且不带 `GUI-CORE-` 前缀，以便读者一眼分辨契约请求与本地规则。

| 编码 | 位置 | 含义 |
| --- | --- | --- |
| `D1-OWNER-CARDINALITY` | `apps/gui/src-tauri/src/d1.rs`，在 D1 驾驶舱投影中使用 | 选中 Lane 携带了多于一个精确 runtime owner binding。Core 的 reducer 对每个 Lane 最多保留一个，因此这是客户端拒绝渲染的视图——它转入 snapshot/replay 恢复而不是挑一个 owner——而不是 Core 尚欠的事实。 |
| `D2-REVIEW-SETTLED` | `apps/gui/src-tauri/src/d2.rs` | Core 已记录该评审的裁决；`DecideReview` 只对 `Pending` 评审裁决一次。 |
| `D2-NO-REVIEWER-ACTOR` | `apps/gui/src-tauri/src/d2.rs` | 从该评审的已发布事实中，推导不出 `validate_review_decider` 会接受的 actor。 |
