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
- 命令的 `owner` 与信封 owner 相同，这是 Core 监督器的要求。Lane 目标使用 Core
  为该 Lane 发布的运行时 owner，其余情况使用本客户端的默认信封 owner。
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

## 离线实机检查

在暂存目录中对 `.viden` 的副本运行（绝不使用线上目录），命令为
`--provider fallback --model test-local`，在 tmux 中执行。抓取结果：

```
┌ Source control ──────────────────────────────────────────────────────┐
│ > Stage all changes                                                  │
│   Commit…                                                            │
│   Push                                                               │
│   Fetch                                                              │
│ TARGET  workspace · main · ahead 0 behind 0 · dirty                  │
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

由 `scripts/tui-previews.sh` 生成、`scripts/tui-regression.sh` 导出的确定性预览状态：

- `main-approval-hunks`：带决策上下文差异块、被省略文件与基线提示的审批浮层；
- `main-git-picker`：带目标与源状态行的 `/git` 面板；
- `main-git-outcome`：一条 `Completed` 与一条 `Failed` 结果条目；
- `main-conflict-detail`：带三侧内容、被省略文件与截断提示的冲突弹窗。

## 已知缺口

- 冲突弹窗由监督浮层的查看行打开，该行面向合并门退回。Lane 冲突在其记录条目上给出
  计数，但 TUI 目前没有可选中的 Lane 冲突行来打开弹窗，这一项与其余 Lane 决策界面
  一同推迟。
- 常驻的审批固定面板仍保留 `input_preview` 行。差异块行位于审批浮层中——审批是在
  那里做出的；固定面板是定高摘要，增高会把固定操作挤出较矮的终端。
- `runtime.evidence_reads` 的证据检视面板属于独立批次。
- `QueryWorkspaceDiff` / `WorkspaceDiffLoaded` 尚无 TUI 读取方：TUI 渲染 Core 附加在
  审批上的差异，而不是操作者差异面板。
