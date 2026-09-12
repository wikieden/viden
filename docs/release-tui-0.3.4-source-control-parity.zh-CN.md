# Viden TUI 0.3.4 源代码管理与冲突对等

English version: [release-tui-0.3.4-source-control-parity.md](release-tui-0.3.4-source-control-parity.md)

TUI 针对 `0.3.3` 三项源代码管理能力的最小对等实现：
`runtime.structured_diff`（GUI-CORE-012）、`runtime.operator_git`
（GUI-CORE-020）与 `runtime.conflict_content`（GUI-CORE-015）。契约见
[前端集成契约](frontend-integration-contract.md) 的
"Source Control And Diff UI Contract"、"Operator Source-Control Actions" 与
"Conflict content" 章节；设计见
[0.3.3 Core 契约增量](release-0.3.3-contract-design.md)。

以下每一条事实都由 Core 拥有。TUI 不解析差异文本、不执行 `git`、不从输出推断结果。

## 审批决策上下文

- 当 `ApprovalRequestView.decision_context` 带有差异时，审批浮层以差异块行替换
  `input_preview` 行：文件行给出变更类型与真实的 `+`/`-` 计数，每个块给出 `@@`
  头，每一行给出 Git 写下的 `old_line`/`new_line` 行号。
- 某一行不存在的一侧保持空白。被删除的行没有新文件行号，TUI 不会在那里打印 `0`。
- `binary`、`omitted` 与 `truncated` 各有独立的陈述行，"未显示"绝不会被渲染成
  "没有变化"。`base_sha256` 渲染为 `computed against <前 8 位>` 提示。
- 路径保留可区分的尾部；内容行在末尾用已注册的 `…` 标记截断。面板随行数增高，
  但受行数上限约束，以保证审批操作始终可见，剩余行数被计数说明。
- 缺少 `runtime.structured_diff` 时，浮层保留预览行并追加一行说明缺失的能力名。

## `/git` 操作者动作

- `/git`（别名 `/source`）打开选择列表优先的面板，与 `/acp` 完全一致。打开面板不
  发送任何命令。
- 行项：暂存全部更改、提交…、推送、抓取。提交会打开单行信息输入；Esc 返回行项并
  丢弃草稿。空信息在本地被拒绝，不会发送任何内容。
- 每一行发送一条 `RunOperatorGitAction { owner, target, action }`。目标是聚焦的
  Lane，否则是工作区；TUI 从不传递路径，`Stage` 不携带路径，由 Core 暂存全部更改。
- 命令的 `owner` 与信封 owner 相同，这是 Core 监督器的要求，该 owner 也是审计记录
  写明的执行者。Lane 目标使用 Core 为该 Lane 发布的运行时 owner。
- 凡是 Core 未发布 owner 的目标，一律在发送之前于本地拒绝；`RunOperatorGitAction`
  绝不发送 `RuntimeOwner::default()`。相关行仍然列出并标注原因，选中该行会以带类型
  的系统条目说明拒绝理由。GUI 在 `D1-OPERATOR-GIT-OWNER` 处拒绝同样的两种情况，因此
  两个客户端指向同一个缺口而不会各行其是：
  - Core 尚未为其发布运行时 owner 的 Lane——条目写明该 Lane；
  - 工作区——Core 尚未发布工作区范围的操作者身份（GUI-CORE-027），条目引用该编号。
    面板的目标行仍然展示 Core 已发布的工作区源状态：被拒绝的是执行者，而不是这棵
    工作树。
- 等待的事件依次为：`CommandAccepted`（回执，绝非结果），若权限门需要则
  `ApprovalRequested`（由常规审批浮层处理，并以差异块行渲染已暂存的变更），随后
  `OperatorGitActionFinished`，其后是 `WorkspaceSourceUpdated`。
- `Completed` 渲染重新采样的分支、领先/落后与是否有未提交更改，并把有界输出折叠为
  行数与首行，必要时附带截断提示。`Failed` 渲染本地化的失败类别、对应的恢复动作与
  Core 的 `detail`。`CommandRejected` 原样渲染 Core 的原因，并说明它发生在任何执行
  之前。
- 同时只允许一个动作在执行中；第二个在本地被拒绝，从而"未发送任何内容"始终为真。
  另有一行可停止跟踪滞留的关联，但不声称 Core 已停止。
- 缺少 `runtime.operator_git` 时，四行仍然列出、置为不可用并标注能力名。TUI 不会
  退化为 shell 调用。

### 失败类别与操作者文案

| `OperatorGitFailureClass` | 提示 | 恢复动作 |
| --- | --- | --- |
| `NothingToCommit` | 没有已暂存的更改可提交 | 请先暂存更改 |
| `NonFastForward` | 远端有本分支缺少的提交 | 先抓取再推送 |
| `AuthenticationRequired` | 远端拒绝了凭据 | 配置远端凭据 |
| `RemoteUnreachable` | 无法连接远端 | 检查网络与远端 |
| `NoUpstream` | 该分支没有上游 | 带上游选项重新推送 |
| `PathOutsideRepository` | 路径超出了目标仓库 | 选择目标内部的路径 |
| `Other` | git 报告了一个失败 | 请阅读下方详情 |
| 无法识别的变体 | git 报告了本版本无法归类的失败 | 请阅读下方详情 |

`Other` 是永久设计而非缺口。无法识别的变体保留 Core 的真实 `detail`，不会被塞进
最接近的类别。

## 冲突内容

- 当 `ConflictBounce.content` 存在时，决策中心的退回行追加
  `n 个文件 · m 个冲突块` 与基线类型（修订版本、已评审证据、未知或无法识别）。没有
  内容的退回保持原来的仅原因行。
- 监督浮层上的查看行会打开只读弹窗，逐文件列出每个被拒绝的冲突块的三个带标签区块：
  按文件自身行号显示的"当前"、来自传入补丁的"传入"、以及补丁原像"原像"。原因标签
  给出 Core 的分类。
- 弹窗明确声明这不是合并结果，且不提供任何解决动作。Core 没有计算合并基。
- `base: None` 渲染为"该冲突块没有原像"；`Some(vec![])` 渲染为"补丁预期为空区域"。
  两者是不同的事实。
- `omitted` 文件与 `truncated` 内容都会被陈述。`content: None` 表示 Core 没有可展示
  的内容，绝不表示冲突为空。
- Lane 冲突在其记录条目上给出同样的摘要，并从 `LaneConflictView.content` 渲染同样的
  弹窗内容。

## 证据检视器

`runtime.evidence_reads`（GUI-CORE-025）。契约见
[前端集成契约](frontend-integration-contract.md) 的 "Evidence archive reads" 章节；
它关闭了 `0.3.2` 监督检查点上推迟的专用证据检视器
（[检查点](release-evidence/tui-supervision/checkpoints.zh-CN.md)）。

- 两个入口。监督决策浮层新增 **查看证据…** 读取行，作用域是记录自身的 `owner`，
  逐字复制自 Core——合并门或评审请求的 owner。新的 `/evidence` 系统命令打开同一个
  检视器，作用域是聚焦 Lane，只设置 Core 发布的那个 lane id，其余 owner 字段全部
  留空，让 Core 的前缀匹配把它们当作通配符；没有聚焦 Lane 时读取整个归档。两行都是
  读取：它们排在所有 Core 决策之后，因此不会改变任何决策的编号，也既不会被在途的
  监督命令阻塞，也不会结算它。
- 行绝不来自 `RuntimeViewState.latest_evidence`。那是一个没有排序规则、没有游标、
  没有内容的近期窗口投影；把它当作归档展示等于宣称 Core 从未声明过的完整性。决策
  浮层仍按原样使用该窗口给出的证据计数。
- 列表按 Core 的顺序分组：**UNDATED** 组在最前——没有时间的行是 Core 能诚实给出的
  最早说法——随后是按 UTC 天升序。每行是 `HH:MM:SS`、种类标签、摘要，以及 owner 指明
  的 Lane；没有时间的行渲染 `--:--:--`，而不是虚构的零点。
- 页脚给出 `LOADED n · more` 或 `LOADED n · archive complete`。加载下一页把
  `next_after` **逐字**回传为 `after`；游标从不被解析、构造或比较。翻页绝不会扩大
  操作者打开时的作用域。
- `f` 在"全部"与 Core 实际返回过的种类之间循环，并从第一页重新查询，而不是过滤
  手上已有的行：Core 是先过滤再切页，因此本地过滤的一页无法说明匹配的行是否位于
  客户端从未加载过的页上。分组只是分组，从不隐藏：Core 交付的每一行都有一行。
- 在某行上按 `Enter` 会发出 `ReadEvidenceContent` 并打开详情面板：头部给出种类与
  摘要，随后是 id 与时间、owner，然后是 `source`、`path`、存在时的规范 item/bundle
  与 `source_hash` 前缀，以及 metadata 的**键**——metadata 的值是自由格式 JSON，
  绝不作为类型化事实渲染。同一时刻只有一个内容读取在途；第二个在本地被拒绝且不发送
  任何命令，Core 已发布的内容从浮层自身的缓存重新渲染而不再读取。
- 内容形态：`Text` 渲染有界可滚动的行，并给出 Core 校验它们所用的 `sha256`，当字节
  上限截断时附带 `truncated` 提示；`Diff` 通过审批浮层使用的同一个差异块生产者渲染，
  因此一份证据补丁与一份工作区差异是同样的行；`Unavailable` 渲染类型化原因。
  `CommandRejected` 逐字渲染 Core 的原因——拒绝绝不是空归档。
- 检视器打开期间到达的 `EvidenceRecorded` 会用一行横幅把已加载的页标记为**过期**；
  按 `r` 从第一页重新加载。不会在操作者背后重新读取，因为后台重查会把他们正在看的
  行移走。
- `Esc` 先收起详情面板，再关闭浮层。关闭会丢弃面板、页与内容缓存，因此重新打开的
  检视器总是重新查询，而不是展示一页年龄不明的数据。
- 缺少 `runtime.evidence_reads` 时不发送任何命令。跳转索引里的 `/evidence` 行仍然
  列出，但呈现为不可用并标注能力名；监督浮层的"查看证据…"行以本地拒绝陈述同一缺口；
  在输入框敲 `/evidence` 则以类型化系统条目陈述它。

### 不可用原因到操作者文案

| `EvidenceUnavailableReason` | 渲染语句 |
| --- | --- |
| `SummaryOnly` | 没有规范字节：这是仅供展示的证据，永远不作为合并证据。 |
| `MissingCanonicalBytes` | 没有规范字节：存储中已不再保存该行所指向的内容。 |
| `HashMismatch` | 规范字节校验失败 — 不予展示 |
| `Binary` | 规范字节校验通过但不是 UTF-8：没有可发布的文本或 diff 形态。 |
| 未识别的变体 | Core 给出的原因本构建无法命名。 |

`HashMismatch` 刻意不做柔化。那正是评审者绝不能被当作规范内容看到的字节，Core 也
从不提供它们，因此该行说的是校验失败，而不是内容缺失。未识别的变体保留自己的语句，
而不是借用某个它并不属于的原因的措辞。

### 证据离线实机检查

在草稿工作区中对 `.viden` 去掉 `cache/` 的副本运行，绝不使用线上目录，tmux 中以
`--provider fallback --model test-local` 启动。

没有聚焦 Lane 时 `/evidence` 读取整个归档，空结果被当作一个答案陈述出来：

```
┌ EVIDENCE INSPECTOR ───────── Esc back · Enter open · f filter · r reload ┐
│ SCOPE whole archive · oldest first                                       │
│ FILTER every kind Core returns                                           │
│ No evidence in this scope.                                               │
│ LOADED 0 · archive complete                                              │
└──────────────────────────────────────────────────────────────────────────┘
```

在真实的休眠合并门上，"查看证据…"行把读取作用域限定到该记录自身由 Core 发布的
owner。作用域行只显示 Core 实际设置的字段，未设置的字段被省略而不是打印为空：

```
┌ SUPERVISION DECISION ─── Esc back · arrows/number select · Enter confirm ┐
│ ⏸ GATE gate-acp-session-019fb6ec-9f2e-72b1-a563-af76fc5561ca · Proposed  │
│ EVIDENCE 0 · VALIDATOR - · CONFLICT -                                    │
│ > 1 Accept merge gate                                                    │
│   2 Reject merge gate                                                    │
│   3 Evidence…                                                            │
│   4 Audit trail                                                          │
└──────────────────────────────────────────────────────────────────────────┘

┌ EVIDENCE INSPECTOR ───────── Esc back · Enter open · f filter · r reload ┐
│ SCOPE session=agent-session_1785480383543782000 task=acp-session-019fb6… │
│ FILTER every kind Core returns                                           │
│ No evidence in this scope.                                               │
│ LOADED 0 · archive complete                                              │
└──────────────────────────────────────────────────────────────────────────┘
```

详情面板没有实测截图。该工作区中 Core 在打开时重建的归档是空的，而 TUI 能发出的
命令都不会填充它：唯一把 `task_summary` 写入归档的路径是
`RuntimeCommand::StartAgentTask`，TUI 从不发送它。详情渲染改由
`evidence-reads.json` 回放与 `main-evidence-detail` 预览覆盖；参见已知缺口。

## 离线实机检查

在暂存目录中对 `.viden` 的副本运行（绝不使用线上目录），命令为
`--provider fallback --model test-local`，在 tmux 中执行。下方两段结果抓取于本地
owner 拒绝落地**之前**、且目标为工作区；它们仍然展示 `OperatorGitActionFinished`
结算后的渲染，但工作区目标现在已不会到达 Core。面板一段是当前的渲染，由
`scripts/tui-previews.sh` 重新生成到 `main-git-picker.txt`：

```
┌ Source control ──────────────────────────────────────────────────────┐
│   Stage all changes · no workspace owner · GUI-CORE-027              │
│ > Commit… · no workspace owner · GUI-CORE-027                        │
│   Push · no workspace owner · GUI-CORE-027                           │
│   Fetch · no workspace owner · GUI-CORE-027                          │
│ TARGET  workspace · codex/v3-tui-client · ahead 2 behind 0 · dirty   │
└──────────────────────────────────────────────────────────────────────┘
```

```
SYSTEM
  Stage all changes completed · main · ahead 0 behind 0 · dirty ·
  OUTPUT 1 lines · git add --all completed AUDIT audit_...

SYSTEM
  Fetch failed · the remote could not be reached · check the network and
  the remote · fatal: 'origin' does not appear to be a git repository
  fatal: Could not read from remote repository. ... AUDIT audit_...
```

两条结果都来自 `OperatorGitActionFinished`；失败类别由 Core 判定为
`remote_unreachable`，两条 `source.fetch` 审计记录（先 `phase: authorized`，后
`phase: failed`）证明了先审计后执行。

## 对等证据

`crates/types/tests/fixtures/frontend-contract-v1/` 下的共享夹具回放出与 GUI 相同的
业务事实：

| 夹具 | TUI 测试 |
| --- | --- |
| `structured-diff.json` | `tui::modal::tests::structured_diff_fixture_replays_into_approval_hunk_rows` |
| `operator-git.json` | `tui::app::tests::operator_git_fixture_replays_into_typed_outcome_entries` |
| `conflict-content.json` | `tui::modal::tests::conflict_content_fixture_replays_into_summary_rows_and_a_three_sided_detail` |
| `evidence-reads.json` | `tui::evidence_panel::tests::evidence_reads_fixture_replays_into_rows_pages_content_and_a_refusal` |

由 `scripts/tui-previews.sh` 生成、`scripts/tui-regression.sh` 导出的确定性预览状态：

- `main-approval-hunks`：带决策上下文差异块、被省略文件与基线提示的审批浮层；
- `main-git-picker`：带目标与源状态行的 `/git` 面板；
- `main-git-outcome`：一条 `Completed` 与一条 `Failed` 结果条目；
- `main-conflict-detail`：带三侧内容、被省略文件与截断提示的冲突弹窗；
- `main-evidence-list`：UNDATED 组在最前、两个 UTC 天分组、加载下一页行与
  `LOADED 3 · more` 页脚的分页归档；
- `main-evidence-detail`：一条 `patch` 行的规范字节渲染成差异块行，并给出 Core
  校验它们所用的 sha256。

## 输入框路由与 `/git` 目标（T1c，2026-09-10）

E1 的发布证据实机跑通了一个真实任务，中途停住三次。三处在改动任何代码之前都先
以失败测试复现；复现过程与 Core 侧事实见
[Core 0.3 兼容性](core-0.3-compatibility.zh-CN.md)开放跟进项 5-7。

### 输入框只在自己的目标忙时入队

`command_for_composer` 过去基于一个统一的「Core 忙」判定路由，而该判定在以下情况
为真：Core 不带作用域的 `assistant_stream` 仍残留着一次已完成的内置回合的答复、
任何 Lane 处于 `Draft`、以及 `queued_inputs` 非空。Core 从不排空会话队列，所以
最后一项再也回不到假：一次 fallback 回合之后，或创建一条 starter Lane 之后，
后续每条提示都入队且都不执行。

现在判定按各调用方真正要问的问题拆开：

- `state::composer_target_busy` 回答路由问题，并以 owner 为作用域，与 GUI 输入框
  采用同一条规则。它读取活跃工具调用、待决审批、活跃 task，或已发布 owner 属于
  本次输入目标的活跃 Agent session，外加本客户端自己在途的原生回合。owner 缺失
  按会话作用域计：前端契约规定 owner 缺失表示该事实不属于任何 Lane 作用域，而
  内建 provider 发布的每一条事实都是这样，把它们当成别人的工作会开出第二个并发
  回合。
- `state::has_active_work` 回答呈现问题——状态行的 `ACTIVE`、活跃工作条、退出
  确认、`Ctrl-C`——且不以 owner 为作用域，因此一条正在跑自己回合的 Lane 仍然
  显示为「有事情在发生」。

H1 的单一判定规则以蕴含而非等价的形式在这次拆分中保留下来，并且是构造上成立的：
呈现判定由路由判定计算而来，所以只要输入框入队，状态行就说 `ACTIVE`。两者可以在
描述两个不同的回合，但绝不会对同一个回合给出两种说法。

Lane 生命周期状态已从两个判定中移除。`LaneStatus::is_active` 是一个生命周期判定，
它把 `Draft`、`Attached` 与 `Detached` 都算作活跃；真正「在跑」的子集是 `Queued`、
`Starting`、`Running`、`WaitingApproval` 与 `NeedsInput`。`queued_inputs` 也从两个
判定中移除，Core 的队列缺口仍然开放并有记录——本客户端只是不再把一个 Core 不会
执行的队列当作回合的证据。

### 原生回合的残留窗口，写得确切

**已于 2026-09-12 由 T2 关闭。** `runtime.turn_lifecycle`（C6）为每一种原生回合
出口都发布 `TurnStarted`/`TurnFinished`，因此 `apps/tui/src/tui/native_turn.rs`
已删除，本客户端不再持有任何活跃性窗口。下文保留该窗口的原样与它接受的漏判，因为
只有对照它才能看清替代方案；现在实际交付的内容见「回合活跃性是 Core 事实」。

Core 不为内建 provider 发布任何回合活跃性事实：没有 Agent session、没有 task，
`AssistantDelta` 也不带 session id。它还只在收到终结性的 Agent session 事实时才
结算那条不带作用域的流，因此一次已完成的答复会在视图里留到会话结束。
`apps/tui/src/tui/native_turn.rs` 改为持有这个窗口，并且不持有任何权威事实：

- 它在输入框派发 `SubmitUserInput` 时**打开**；
- 它在本命令的 `CommandRejected`、派发之后的第一条 `SnapshotUpdated`（原生回合
  返回时 `runtime_events_for_streaming_output` 发出的终结批次的首事件，而中途的
  审批边界批次不带这个前缀），或一次丢弃了相关流的快照替换时**关闭**。

`SnapshotUpdated` 不是回合活跃性事实。有三条操作者命令会自行发布一条
`SnapshotUpdated`——`SetWorkMode`、`SetPermissionLevel`、`SelectModel`——因此在
原生回合流式输出**期间**更改工作模式、权限级别或模型会提前关闭该窗口。这一次误判
是有界的，并且由 Core 而不是由猜测来回答：下一条提示会被提交，而 supervisor 以
`active runtime job … is already running` 拒绝为同一 owner 开出第二个作业，转录会
渲染该答复。它取代的那种失败——一个永远不会关闭的窗口——在会话内部根本无法恢复。
要真正正确地关闭它，需要开放跟进项 3 里那条 Core 回合活跃性事实。

### 被选中的 Lane 在离开详情面板后仍然存在

`/git` 是在输入框里键入的，而 Lane 的选中态过去会随操作者为了到达输入框而关闭的
lane 详情面板一起消失，于是从输入框打开的每个 `/git` 都以工作区为目标并被
GUI-CORE-027 拒绝。第一个成因背后还有第二个：环境性的 lane 详情面板在渲染顺序上
压过交互面板，因此即便选中态得以保留，看到的仍会是 lane 检视面板，而不是选择器的
行。第三个成因只有在前两个修好、能够走通实机流程之后才暴露出来：一条被选中且没有
session 的 Lane 会把客户端拉到 board 视图，而 board 与 `Setup`、`Decisions`、
`Gallery` 一样根本不渲染输入框（`render::render_frame`）。如果目标所服务的那个界面
不在屏幕上，保住目标本身毫无意义。

- `TuiUiState.lane_detail_open` 与 `TuiUiState.focused_lane` 分离。前者表示屏幕上
  显示着什么，后者表示下一条命令点名哪条 Lane。
- `Esc` 的回退链多了一级：浮层 -> lane 详情 -> Lane 目标 -> 插入模式。第一次
  `Esc` 收起面板并说明该 Lane 仍是目标；第二次清除它并同样给出说明，因此清除目标
  的方式是有据可查而非隐藏手势。状态行的 `L:<lane>` 全程表示目标。
- 交互面板在环境性 lane 详情之前渲染。操作者主动打开的选择器不会被没打开的面板
  遮住。
- board 视图由收起面板的同一级回退一并撤销，且 `reconcile_ui_state_with_runtime`
  在该面板关闭之后不再把 Session 视图拉回 board。两处都以 `lane_detail_open` 为
  条件，因此操作者正在查看该 Lane 时 board 仍然优先，而按名字主动进入的视图
  ——`Setup`、`Decisions`、`Gallery`——不会被这一级动到。
- 工作区目标的 027 拒绝行为未变。

选择器的 TARGET 行现在会点名该 Lane，并对 Lane 目标声明源状态未知。
`RuntimeViewState.workspace_source` 是单一的工作区作用域视图，Core 不为每条 Lane
发布任何源状态，因此把工作区的分支与领先/落后印在 Lane 的名字旁边，等于把一棵树
的状态安到另一棵树上。

### 离线实机检查，2026-09-10

在 tmux（120x40）中，针对 `.viden` 的一份临时副本运行——包含 `agents/`、
`context-engine/`、`projects/`、`workflows/`、`index.sqlite3` 与 `lanes.tsv`，
排除 `cache/`——工作区是一个临时 Git 仓库，参数为 `--provider fallback --model
test-local`。仓库自身的 `.viden/` 未被打开。序列即 E1 的序列，并越过它停住的那个
点继续：一次 fallback 回合、`New Lane`、批准、`Esc` 退出详情面板、第二次输入框
提交、`/git`，再按一次 `Esc`。

下面每个块都是那次运行的 `tmux capture-pane` 帧，裁到所述的行，并缩短了临时路径。
lane id 是 Core 在那次运行中生成的。

**1. 一次 fallback 回合完成之后。** 输入框给出 `Send`，状态行是 `IDLE`。在 T1c
之前，这里两者都会永久变成 `Queue` / `ACTIVE`，因为 Core 不带作用域的流里仍留着
那条答复。

```
│ MODE [Build]  PERM [Ask]      ACTIONS: [^J Send] [^K Clr] [^R Regenerate] [^N New Task] [? Help] │
 P:t1c-live L:- M:Build INSERT · IDLE · EVENTS 8 · TOKENS 0 · PROVIDER healthy
```

**2. `New Lane` 与 `Allow once` 之后。** 这条 Lane 是真的——Core 创建了分支
`viden/lane_1789024399248187000` 及其工作树，两者都用 `git branch` 与
`git worktree list` 在带外确认过。它处于 `Draft`，状态行是 `IDLE` 而不是
`ACTIVE`：创建出来的 Lane 不是正在跑的回合。

```
┌ LANE DETAIL ─────────────────────────────────── lane_1789024399248187000 ┐
│ lane_1789024399248187000 coder                                           │
│ ROUTE main→side-1                                                        │
│ STATE  Draft                                                             │
 P:t1c-live L:lane_178902439 M:Build NORMAL · IDLE · EVENTS 10 · TOKENS 0 · PROVIDER healthy
```

**3. `Esc` 退出 lane 详情。** 面板收起，cockpit 与它的输入框回来了，
`L:lane_178902439` 保留，转录说明了发生了什么以及如何撤销。

```
 VIDEN / COCKPIT  fallback  test-local  healthy
 /private/tmp/…  lane lane_1789024399248187000  session -  approvals 0
...
│ MODE [Build]  PERM [Ask]      ACTIONS: [^J Send] [^K Clr] [^R Regenerate] [^N New Task] [? Help] │
 P:t1c-live L:lane_178902439 M:Build NORMAL · IDLE · EVENTS 10 · TOKENS 0 · PROVIDER healthy
```

```
SYSTEM
  Closed the lane detail. The Lane stays the target for /git; Esc again clears it.
```

**4. Lane 存在的情况下再做一次输入框提交。** 被接受并得到答复——一条 `USER` 行
与一条回复，整个会话里没有任何 `queued …` 行。在旧判定下，这就是 E1 的第二次
停住。

```
USER
  where does the config loader live

USER
  add a config loader note

SYSTEM
  Closed the lane detail. The Lane stays the target for /git; Esc again clears it.

USER
  third prompt after the Lane exists
```

**5. 在输入框中键入 `/git`。** 选择器点名该 Lane，四行全部可选：Core 为这条 Lane
发布了 runtime owner，因此 `runtime.operator_git` 从本客户端可达。没有真的执行
提交；这张截图要证明的是目标与被启用的行。

```
┌ Source control ──────────────────────────────────────────────────────┐
│ > Stage all changes                                                  │
│   Commit…                                                            │
│   Push                                                               │
│   Fetch                                                              │
│ TARGET  lane_1789024399248187000 · source unknown                    │
└──────────────────────────────────────────────────────────────────────┘
```

**6. 有据可查的清除方式。** 再按一次 `Esc` 清除目标并给出说明；`L:` 回到 `-`。

```
SYSTEM
  Cleared the Lane target lane_1789024399248187000. /git now names the workspace.

 P:t1c-live L:- M:Build INSERT · IDLE · EVENTS 13 · TOKENS 0 · PROVIDER healthy
```

**7. 工作区目标未变。** 没有选中 Lane 时的 `/git` 仍然把四行渲染成受
GUI-CORE-027 拒绝，旁边是工作区自己的源状态事实——这些事实它确实有。

```
┌ Source control ──────────────────────────────────────────────────────┐
│ > Stage all changes · no workspace owner · GUI-CORE-027              │
│   Commit… · no workspace owner · GUI-CORE-027                        │
│   Push · no workspace owner · GUI-CORE-027                           │
│   Fetch · no workspace owner · GUI-CORE-027                          │
│ TARGET  workspace · main · ahead 0 behind 0 · dirty                  │
└──────────────────────────────────────────────────────────────────────┘
```

关于同一次运行的三点如实说明。Core 的 `assistant_stream` 明显把每条答复连在一起，
因为 Core 对内建路径仍然从不结算它——那是开放跟进项 3，此处未变，而它现在只是一个
显示层的瑕疵，不再是路由输入。临时 `.viden` 副本带来了八个来自被复制会话的
`Proposed` 门，监督条会全程列出它们；它们属于被复制的状态，而不属于这次运行。
另外有一条 `USER` 行内容是 `i/git`，因为驱动脚本在输入框已处于插入模式时又发了
一个 `i`；那是脚本操作者的问题，不是客户端的问题。

## 面向 0.3.4 Core 增量的 TUI 对等（T2，2026-09-12）

[0.3.4 计划](release-0.3.4-plan.zh-CN.md)的批次 T2 采纳 Core 增量 C5 到 C7，并
记录 C9 的对等最小集。下文的形态是实际交付的形态，与
[契约设计](release-0.3.4-contract-design.zh-CN.md)正文的差异由该文档自己的
"Amendments From Implementation" 一节记录；`workspace-owner.json`、
`turn-lifecycle.json` 与 `durable-work-evidence.json` 这三个 fixture 才是本客户端
所依据的线格式事实，下文其中三个测试直接回放它们，而不是手工构造行。

| 能力 | T2 中的 TUI 对等 |
| --- | --- |
| `runtime.turn_lifecycle` | 已采纳：忙判定与队列文案 |
| `runtime.workspace_owner` | 已采纳：`/git` 工作区目标、按 Lane 的源代码行 |
| `runtime.durable_work_evidence` | 已采纳：归档的 `patch` 行与审批审计行 |
| `ui.layout_preferences` | 无对等最小集；TUI 不渲染 lane 侧栏 |
| `runtime.workspace_file_reads` | 无对等最小集；TUI 未注册文件查看器 |

### 回合活跃性是 Core 事实

按输入框作用域给出的实际判定：

- `state::composer_target(state)`（`apps/tui/src/tui/state.rs:311`）解析下一次
  提交所指的 owner 作用域。聚焦中的 Agent 会话指向该会话所属的 Lane；其余一切都
  是会话作用域，即 `lane_id: None`，也就是驱动自身的信封 owner。只有一处解析器，
  路由与发送路径因此不可能对「在问谁的回合」产生分歧。
- `state::turn_is_active_for`（`apps/tui/src/tui/state.rs:340`）按该作用域匹配
  `RuntimeViewState.active_turns`，而绝不按 `turn_id` 匹配：`turn_id` 是每回合新
  生成的值，其存在是为了让审计行有可关联的键，契约要求客户端匹配作用域。
- `state::composer_target_busy`（`apps/tui/src/tui/state.rs:396`）回答路由问题
  ——入队还是提交，由 `app::command_for_composer`（`apps/tui/src/tui/app.rs:2632`）
  读取。对**会话作用域**，它是一个 Core 已发布的会话回合、活跃工具调用、待决审批、
  活跃 task，或已发布 owner 属于该作用域的活跃 Agent session。对**某条 Lane 的
  会话**，它是 Core 为该 Lane 发布的回合，或该 Lane 自己的 Agent session 正在运行。
- `state::has_active_work`（`apps/tui/src/tui/state.rs:440`）回答展示问题——状态行
  的 `ACTIVE`、实时工作条、退出确认、`Ctrl-C`——且不按 owner 取作用域：它从路由
  答案出发，再加上任何回合、工具调用、审批、task、运行中的 Lane 或活跃 Agent
  session。因此 H1 的单一判定规则仍以蕴含关系成立，并且是结构性成立。

两个判定都不读取的四项事实，各有各的理由：

- `assistant_stream`，Core 那条不带作用域的流。Core 现在会在会话作用域的
  `TurnFinished` 上清空它，而本客户端无论如何都不读它。
- 本客户端自己派发的 command id。那是 T1c 的窗口，它在派发后的第一个
  `SnapshotUpdated` 上关闭——那并不是回合活跃性事实，因为 `SetWorkMode`、
  `SetPermissionLevel` 与 `SelectModel` 各自也会发布一个。
  `apps/tui/src/tui/native_turn.rs` 已删除。
- `queued_inputs`。队列是 Core **尚未**开始的工作；排空由 `InputDequeued` 宣告，
  它启动的回合由 `TurnStarted` 宣告。
- Lane 生命周期状态，包括 `Draft`。与 T1c 相同，未变。

`active_turns` 为空是一个真实答案，重启之后也一样：Core 不跨重启恢复回合，也不
持久化这两个事实。没有该能力时该列表恒为空，于是本客户端直接提交，由 Core 用它
自己的 `CommandRejected` 作答——这是 Core 的答案而非客户端的猜测，正是契约要求的
方向。

队列文案说明 Core 正在作出的是哪一种承诺
（`apps/tui/src/tui/composer.rs:93`）。Core 只在**已完成**的会话作用域回合之后
排空会话队列，因此在这样的回合运行期间输入框显示 `N queued; runs after the
current turn`；而当没有会话回合在运行时——失败之后、取消之后，或运行的是一条 Lane
的回合（它从不装填排空）——显示 `N queued; waits for the next completed turn`。
在后一种情形说「当前回合结束后运行」，等于承诺 Core 已经决定不做的执行。

E1 缺陷 2 的两个情形都已固定为测试。判定层：
`state::tests::the_composer_is_idle_after_one_completed_fallback_turn` 与
`state::tests::a_draft_lane_and_a_session_queue_are_not_active_work`。路由层，
`app::tests::core_turn_brackets_decide_whether_the_next_prompt_queues_or_submits`
把 `TurnStarted` 再把 `TurnFinished` 推过真正的 reducer，因此断言的是「由这对括号
决定」，而不是由某个标志位决定。

### `/git` 工作区目标在 Core 已发布的 owner 之下

`operator_git::operator_git_owner`（`apps/tui/src/tui/operator_git.rs:235`）是
选择器行与发送路径共同读取的唯一解析器，因此一行绝不会先被当作可选中、等操作者
选了之后再被拒绝。

- **工作区目标。** Core 在 `RuntimeViewState.workspace_owner` 中发布的 owner，
  原样拷贝进 Core 会比较的两个位置：命令自身的 `owner` 字段与信封的 owner。
  客户端不会从根路径重算 `ws_` 摘要，也不会用 `RuntimeOwner::default()` 顶替——
  Core 按**它自己**发布的 workspace 与 project id 授权该动作，而本客户端推导出的
  id 不属于其中。
- **缺失。** `workspace_owner` 缺失表示这个 Core 没有发布过。四行仍然列出、置灰，
  并标注 `Core published no workspace owner`，且什么都不发送。该标注不再引用
  GUI-CORE-027：该登记项已在 Core 侧关闭，继续引用它等于客户端报告一个已不存在的
  Core 缺口。
- **Lane 目标。** 与 T1c 相同：Core 为该条确切 Lane 发布的运行时 owner。
- **未建模的目标。** `SourceTarget` 是 `#[non_exhaustive]`，因此更新版 Core 命名的
  目标会被拒绝并称为未知，而不是解析成看起来最接近的那个。

选择器的 TARGET 行（`apps/tui/src/tui/modal.rs:1083`）读取各目标自己的源：Lane 读
`lane_sources[lane]`，工作区读 `workspace_source`。这正是 `LaneSourceUpdated` 的
用途——在它之前，一次 Lane 目标动作会把一棵树的分支与 ahead/behind 放进工作区的
chip 里。没有对应行的 Lane 仍显示 `source unknown`，因为没有行的 Lane 要么是直接
使用工作区的 Lane，要么是 Core 尚未采样的 Lane，而把工作区的数字印在 Lane 名字
旁边会把一棵树的状态归到另一棵树上。

### 证据检视器展示归档的 patch

`runtime.durable_work_evidence` 改变的是哪些事实进入归档，而不是读取契约，因此
检视器的翻页、游标处理与内容状态都未变。新增的是详情面板的规范内容部分
（`apps/tui/src/tui/evidence_panel.rs:693`），拆成四行分别陈述，因为归档 patch 的
阅读者有四个各自独立的问题：

- `CANONICAL item … · bundle …`——字节在哪里；
- `PRODUCER <identity> · <role> · task <task>`——`producer.task_id` 正是合并门要
  检查的东西，所以这一行是向「看得见证据存在」的人解释 `MissingProducer` 拒绝的
  依据；
- `APPROVAL audit <id>`，或 `no operator approval receipt; a rule or an adapter
  allowed this`——`permission_snapshot_id` 缺失表示没有操作者审批放行过那次调用，
  而在那里什么都不渲染会被读成「已获批准」；
- `RECORD Core verified the canonical reference: <state>`——Core 对**记录**给出的
  结论，这与内容读取自身对 `source_hash` 的哈希校验是不同的事实。把两者分开，才
  不会让一条 `verified` 记录与一个未加解释的 `HashMismatch` 并排出现，而后者正是
  E1 缺陷 9 在 GUI 侧记录的形态。

`patch` 行上的 `canonical: None` 渲染为 `CANONICAL none`，原因由该行自己的摘要
承载——差异超过 `MAX_EVIDENCE_CONTENT_BYTES`，或存储写入失败。那里留空会被读成
仅供展示的证据，而合并门对后者的处理是不同的。

内容状态未变且已完整：`Diff` 通过审批浮层使用的同一个 hunk 生成器渲染，`Text`
说明已校验的哈希以及字节上限是否截断了它，`SummaryOnly`、
`MissingCanonicalBytes`、`HashMismatch` 与 `Binary` 各自保留自己的句子
（`evidence_panel::unavailable_reason_key`）。

审计视图不需要新代码：带点号的 `action` 键是 Core 的稳定词汇表，原样渲染，这也是
`approval.allow_once` 不需要为每种决定加一条文案条目就可读的原因。此处的改动是
证明——`audit_panel::tests::the_durable_work_evidence_fixture_replays_the_approval_
decision_into_the_audit_lens` 回放 fixture 的审计页，并断言该行写明了它解决的审批
请求与该决定放行的工具。在 C7 之前，那个 id 是一个写在任何地方都没有的实时关联
id，这就是 E1 缺陷 4。

### 两个没有 TUI 对等最小集的能力

- `ui.layout_preferences` 持久化 lane 侧栏模式与被操作者隐藏的常驻状态栏段。TUI
  **没有 lane 侧栏**：它的 Lane 界面是侧屏、lane 详情面板与 `/lane` 选择器，没有
  哪一个是可固定/可浮动的侧栏。`apps/tui/**` 中没有任何地方读取、持久化或镜像
  `lane_sidebar_mode`，也没有任何地方发送 `SetUiLayoutPreferences` 或
  `ResetUiLayoutPreferences`；状态栏的段在这里也不是可由操作者隐藏的。若 TUI 将来
  长出一个可隐藏的常驻段，它会消费既有记录，而不是再加一套偏好模型。
- `runtime.workspace_file_reads` 回答某个工作区文件里有什么。TUI 未注册文件查看器
  也不发送 `ReadWorkspaceFile`，因此没有渲染它的地方；文件**清单**读取方
  （`apps/tui/src/tui/workspace_files.rs`）未变。

两条都已如此记录在
[Core 0.3 兼容性](core-0.3-compatibility.zh-CN.md)中。

### T2 改变的回归帧及其原因

`scripts/tui-regression.sh` 先在本批次自己的基线（`claude/int-0.3.4` 的
`e262fbc7`）上运行一次，再在完成后的分支上运行一次，然后逐文件比较生成的帧。恰好
有两个发生变化，各自连同它的 `.ansi` 与 `.svg` 渲染；其余每个帧、行数、宽度与
chip 配平都逐字节一致。

| 帧 | 原因 |
| --- | --- |
| `main-git-picker` | 预览现在发布 `RuntimeViewState.workspace_owner`，即 `runtime.workspace_owner`（C5）新增的内容，因此四行渲染为可选中，而不再带 `· no workspace owner · GUI-CORE-027` 后缀。 |
| `main-evidence-detail` | 预览的 `patch` 行现在携带自 `runtime.durable_work_evidence`（C7）起归档原生 patch 真正具有的 `CanonicalEvidenceReference`，因此详情多出五行——`CANONICAL`、`HASH`、`PRODUCER`、`APPROVAL`、`RECORD`——取代差异下方原来的五行空白。差异块本身未变，仍能放进面板。 |

`scripts/tui-previews.sh` 是回归脚本的第一步，所以两个帧都由同一次运行重新生成；
没有任何预览是手工重新生成的。

### 离线实机检查，2026-09-12

在 tmux（140x40）中以 `--provider fallback --model test-local` 运行，针对本次会话
scratchpad 下为该运行新建的临时 Git 仓库，而不是针对本仓库：该工作区用
`git init` 创建，含两个文件与一次提交，Core 在其中创建了自己的 `.viden/` 与 Lane
worktree。本仓库的 `.viden/` 既未被打开也未被修改。

驱动脚本能够展示的：

- **一次已完成的 fallback 回合之后输入框仍可提交。** 在欢迎提示、已批准的
  `lane_create` 与 `Esc` 退出 lane 详情之后，输入框提供 `[^J Send]`，状态行显示
  `IDLE`。
- **已创建的 Lane 不是正在运行的回合。** 该 Lane 处于 `Draft`，状态行显示
  `L:lane_1789212279910316000` 并同时显示 `IDLE`。
- **随后两条输入框提示都执行了。** 两者都作为 `USER` 行出现，整个会话中没有任何
  `queued …` 行，这就是 E1 缺陷 2 两个情形的完整走通。
- **Lane 目标陈述它自己的源。** 选中 Lane 后 `/git` 显示四行可选中，且
  `TARGET lane_1789212279910316000 · viden/lane_1789212279910316000 · …`，读自
  `lane_sources`。在 C5 之前这一行对每条 Lane 都显示 `source unknown`，也就是上文
  T1c 一节记录的那条诚实性后果；它现在是一个已发布的事实。
- **放弃目标的既有方式仍然有效。** 再按一次 `Esc` 清除目标并说明这一点，`L:`
  回到 `-`。
- **证据检视器回答的是空且完整的归档**，这对本次运行是正确的：fallback provider
  没有应用任何文件改动，而 `runtime.durable_work_evidence` 归档的是**已应用**改动
  的 `patch` 行。此处的空归档不是 E1 缺陷 5 那种空。

驱动脚本无法展示的：

- **工作区 `/git` 行处于可用状态。** 未选中 Lane 时 `/git` 仍把四行渲染为置灰并
  标注 `Core published no workspace owner`，因为在 `viden` 二进制实际走的那条路径
  上 `RuntimeViewState.workspace_owner` 是缺失的。这是 Core 侧的缺口而不是客户端
  的缺口，已如此记录在下文与
  [Core 0.3 兼容性](core-0.3-compatibility.zh-CN.md)中：`SessionEngine::bind_workspace_owner`
  在生产代码中的唯一调用方是 `LocalCoreHost::open_workspace`，GUI 走它而
  `apps/cli` 不走——CLI 直接引导 engine 与 supervisor。客户端那一半改由
  `app::tests::a_published_workspace_owner_enables_the_rows_and_is_sent_verbatim`
  与重新生成的 `main-git-picker` 帧来证明。
- **TUI 会话中的归档 patch。** 填充归档需要一次已应用的
  `write_file`/`edit_file`，fallback provider 不会产生它；E1 也只是通过真实
  provider 驱动的已批准工具调用才拿到一次。此处详情面板的证据是 fixture 回放加
  预览帧。
- **被排空的会话队列。** 本次运行没有任何内容入队，因为没有任何工作忙到足以让后续
  提示排在后面；队列文案的两种状态由
  `composer::tests::composer_invites_next_prompt_during_active_turn` 覆盖。

关于驱动脚本而非客户端的两点实话。有两条 `USER` 行内容是 `try point live` 与
`i/evidence`，因为脚本在输入框已处于插入模式时又发了一个 `i`，吞掉了该行开头的
几个字符——那是脚本操作者的问题，不是客户端的问题。另外本次运行的第一条提示走的是
欢迎流程，它会创建一条 starter Lane 并请求 `lane_create`；该审批是在 Decisions
界面上批准的，那里才是常驻审批按键所在之处。

## 已知缺口

- 冲突弹窗由监督浮层的查看行打开，该行面向合并门退回。Lane 冲突在其记录条目上给出
  计数，但 TUI 目前没有可选中的 Lane 冲突行来打开弹窗，这一项与其余 Lane 决策界面
  一同推迟。
- 常驻的审批固定面板仍保留 `input_preview` 行。差异块行位于审批浮层中——审批是在
  那里做出的；固定面板是定高摘要，增高会把固定操作挤出较矮的终端。
- 证据检视器的详情面板仍然没有**实测**截图。缺口的位置变了：
  `runtime.durable_work_evidence`（C7）让一次已应用的原生改动归档出一条 `patch`
  行，所以对 TUI 会话而言归档已不再是结构性为空，但拍到一次的任务属于 E2。T2 为
  该面板提供的证据是生成的预览帧加上文点名的两次 fixture 回放。近期窗口与归档仍是
  两个不同的投影，这也是本客户端仍然绝不从 `latest_evidence` 推导归档行的原因。
- 检视器一次只提供一个种类过滤。Core 的 `kinds` 是 OR 列表，但浮层只有一个循环
  控件，发送多个就等于宣称操作者做过并不存在的选择。所开作用域之下的 owner 过滤
  与时间范围同样没有控件。
- 检视器内没有逐行动作：没有进入差异界面的"在评审中打开"，没有复制证据 id，也没有
  跳转到需要它的合并门。该浮层只负责浏览与读取，不做任何决策。
- `QueryWorkspaceDiff` / `WorkspaceDiffLoaded` 尚无 TUI 读取方：TUI 渲染 Core 附加在
  审批上的差异，而不是操作者差异面板。
- **已于 2026-09-12 由 C5 与 T2 关闭，但留有一个 Core 侧缺口。** Core 会铸造一个
  工作区范围的操作者身份，本客户端也在该身份之下发送工作区目标，因此 `/git` 行项
  对工作区与「Core 已发布运行时 owner 的 Lane」都可操作。没有已发布 owner 的目标
  仍在本地被拒绝，只是现在写明缺失的事实而不再引用 GUI-CORE-027。缺口是：
  `apps/cli` 直接引导 engine 与 supervisor，而不经过
  `LocalCoreHost::open_workspace`——后者是 `SessionEngine::bind_workspace_owner`
  在生产代码中的唯一调用方——所以 `viden` 的 TUI 会话仍看到 `workspace_owner`
  缺失并显示拒绝。GUI 不受影响，因为它的适配层经由该 host 打开工作区。关闭它是
  Core 或 CLI 的改动，不是客户端改动。
