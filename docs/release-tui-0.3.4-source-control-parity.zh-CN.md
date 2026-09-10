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

## 已知缺口

- 冲突弹窗由监督浮层的查看行打开，该行面向合并门退回。Lane 冲突在其记录条目上给出
  计数，但 TUI 目前没有可选中的 Lane 冲突行来打开弹窗，这一项与其余 Lane 决策界面
  一同推迟。
- 常驻的审批固定面板仍保留 `input_preview` 行。差异块行位于审批浮层中——审批是在
  那里做出的；固定面板是定高摘要，增高会把固定操作挤出较矮的终端。
- 证据检视器的详情面板没有实测截图。草稿工作区中 Core 在打开时重建的归档是空的，
  而 TUI 发出的命令都不会填充它：把 `task_summary` 写入归档的唯一路径是
  `RuntimeCommand::StartAgentTask`，TUI 从不发送它。同一会话中 `latest_evidence`
  达到 1，而 `QueryEvidence` 返回的是空且完整的归档——近期窗口与归档是两个不同的
  投影，这正是本客户端绝不从窗口推导归档行的原因。Core 是否也应把那条已记录的
  事实并入归档，是 Core 侧的问题，而不是客户端改动。
- 检视器一次只提供一个种类过滤。Core 的 `kinds` 是 OR 列表，但浮层只有一个循环
  控件，发送多个就等于宣称操作者做过并不存在的选择。所开作用域之下的 owner 过滤
  与时间范围同样没有控件。
- 检视器内没有逐行动作：没有进入差异界面的"在评审中打开"，没有复制证据 id，也没有
  跳转到需要它的合并门。该浮层只负责浏览与读取，不做任何决策。
- `QueryWorkspaceDiff` / `WorkspaceDiffLoaded` 尚无 TUI 读取方：TUI 渲染 Core 附加在
  审批上的差异，而不是操作者差异面板。
- Core 尚未发布工作区范围的操作者身份（GUI-CORE-027），因此 `/git` 行项只有在 Core
  已为该 Lane 发布运行时 owner 时才可操作。其余目标一律在本地拒绝并说明原因。TUI 不
  用默认 owner 顶替，否则这类动作写下的审计记录将无人归属；GUI 在
  `D1-OPERATOR-GIT-OWNER` 处作出完全相同的拒绝。解除该限制需要 Core 提供事实，而不是
  客户端改动。
