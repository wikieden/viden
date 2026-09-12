# 主窗口交互 · 视觉证据

英文版：[EVIDENCE.md](EVIDENCE.md)

覆盖 `claude/gui-main-window-interactions` 上的主窗口交互工作：D6 恢复动作
（重启 / 关闭 Lane、纯展示的事实展开、以及被拒绝时的告警）、Composer 的
工作模式 / 权限级别 / 模型选择器与驾驶舱状态栏、活动栏齿轮后的设置面板、
D11 项目接入，以及 D12 合并闸的批准 / 退回动作栏及其必填理由输入框。

在 `claude/gui-supervision-debts` 上扩展了 D2 评审裁决
（`RuntimeCommand::DecideReview`，关闭 GUI-CORE-011）与 D10 成本不可计量 Lane
标记及 Core 为其发布的有界运行事实。

在 `claude/gui-d14-audit` 上再次扩展，覆盖 D14 的两种模式：基于 Core
`QueryAudit` -> `AuditPageLoaded` 契约（`runtime.audit`）的审计轨迹、按 D12 回滚行
导航所用对象带范围的同一条轨迹，以及缺少 `runtime.audit` 的 Core 留下的原始事件
回放兜底。

## harness 是什么

[`qa.html`](qa.html) 加 [`qa.ts`](qa.ts) 按 `?state=` 每次只渲染一个截图状态。
页面**只调用生产环境导出的渲染函数** —— `renderD1Cockpit`、`renderD6Recovery`
（经由驾驶舱自己的工作区挂载）、`renderD11Intake`、`renderD12IntegrationGate`、
`renderD2Decisions`、`renderD10LaneMonitor`、`renderD14` 以及它们挂载的组件。这里不重新实现任何控件、文案或布局，因此 `src/**` 的回归
会直接反映在截图里，而不会被 harness 自带的 UI 副本掩盖。

渲染逻辑放在 `qa.ts` 而不是内联 `<script>`，这样 `tsc --noEmit` 会按生产渲染
签名对 harness 做类型检查。

### 确定性

- 首次渲染前把 `Date.now` 冻结在 `2026-01-01T00:00:00.000Z`。工作状态条打印
  `now() - startedAt`，时钟不冻结会让同一状态的两次截图不一致。
- `Math.random` 冻结为 `0`。
- 驾驶舱以 `poll: false` 挂载，操作者取景时不会有定时器刷新截图。
- 所有宿主回调都是永不 resolve 的 promise，harness 稳定后状态不再变化。唯一
  例外是 `d6-error`：它的 `sendD6Intent` 会以一句固定文本 reject —— 这次拒绝
  正是要截取的状态。
- 每个状态最后把自身名字写入 `document.documentElement.dataset.captureReady`。
  出现该属性后再截图。

## 投影来自哪里

| 状态组 | 来源 | 类型 |
| --- | --- | --- |
| `d1*`、`settings*`、`d6-*` | [`../../tests/support/d1_projection.ts`](../../tests/support/d1_projection.ts) | vitest 各套件共用的 D1 fixture |
| `d12-actions`、`d12-blocked`、`d12-conflict-none` | [`../gui-screen-restore/projections/d12.json`](../gui-screen-restore/projections/d12.json) | 由 `tests/capture_projections.rs`**生成** |
| `d12-conflict-content` | [`../gui-screen-restore/projections/d12-conflict.json`](../gui-screen-restore/projections/d12-conflict.json) | 由同一测试从规范 fixture `conflict-content.json`**生成** |
| `d12-conflict-omitted` | [`../gui-screen-restore/projections/d12-conflict-omitted.json`](../gui-screen-restore/projections/d12-conflict-omitted.json) | 由同一测试**生成** —— 触及上限的变体 |
| `d2-review-*` | [`../gui-screen-restore/projections/d2-review.json`](../gui-screen-restore/projections/d2-review.json) | 由 `tests/capture_projections.rs`**生成** —— 已选中待处理评审的决策队列 |
| `d2-review-confirmed` | [`../gui-screen-restore/projections/d2-review-decided.json`](../gui-screen-restore/projections/d2-review-decided.json) | 由同一测试**生成** —— `decide_review` 之后 Core 留下的队列 |
| `d10-blind*` | [`../gui-screen-restore/projections/d10.json`](../gui-screen-restore/projections/d10.json) | 由 `tests/capture_projections.rs`**生成** |
| `d14-audit*` | [`../gui-screen-restore/projections/d14-audit.json`](../gui-screen-restore/projections/d14-audit.json) | 由同一测试**生成** —— 生产用的 acceptance-first 关联机从 Core `AuditPageLoaded` 页面产出的投影 |
| `d14-raw-fallback` | [`../gui-screen-restore/projections/d14-raw.json`](../gui-screen-restore/projections/d14-raw.json) | 由同一测试经 `CoreClient::replay`**生成** |
| `d11`、`d11-recent` | 镜像 `tests/d11_intake.spec.ts` 里的 fixture，历史面板另用下文手写的 `RecentWorkResult` | 手写；D11 目前还没有生成的捕获投影 |
| `lane-rail`、`project-picker`、`project-switch-confirm` | 共享 D1 fixture 加一份手写的 `RecentWorkResult` | 手写；`frontend-contract-v1` 目前还没有规范的最近工作捕获投影，因此形状镜像 `tests/recent_work.rs` 与 `tests/project_picker.spec.ts` |

D12 投影从不手写。`tests/capture_projections.rs` 用真实 Rust 投影跑规范
`frontend-contract-v1` 的 `merge-gate.json` fixture 并序列化结果，因此截出来
的像素就是 Core 事实实际产出的。投影有任何改动后重新生成：

```bash
cargo test -p viden-gui --test capture_projections -- --ignored
```

W4 给 D12 闸动作加上了类型化的可用性码；已提交的 `d12.json` 现在在 accept 上
携带 `missing_evidence`、在 reject 上携带 `no_actor`，这正是 `d12-blocked`
要截取的内容。

同一个测试现在还会生成 `d2-review.json`（已选中待处理评审的队列，因此可用的
接受/驳回动作栏与评审意见输入框都来自真实的 Core 评审记录）与
`d2-review-decided.json`（同一队列在该评审被结算之后：状态 `accepted`、Core 新
分配的裁决审计 id、记录在案的评审意见），并在 `multi-lane.json` fixture 的
terminal Lane 上写入有界运行事实，因此 `d10.json` 同时携带一条带运行事实的成本
不可计量 Lane 与一条完全没有这些事实的可计量 Lane。

之所以需要 `d2-review-decided.json`：确认本来就**来自** `ReviewRequestUpdated`，
outcome 变为 confirmed 时评审早已结算，因此「确认回执压在仍为 pending 的行之上」
是生产环境永远到不了的状态。harness 因此把 Core 在该 outcome 下真正会重新发布的
投影交给 intent 结果。

### 在来源之上写的 delta

harness 补充的每个值都是上述来源之上的 delta，`qa.ts` 中每条都有行内注释指明
它镜像的是哪个 fixture。

| 状态 | Delta | 镜像自 |
| --- | --- | --- |
| 全部 `d1*` | `preferences.locale/skin/mode` 跟随 URL 参数 | 驾驶舱从投影而非文档元素读取语言 |
| 全部 `d1*` | 上下文坞的一条 `ContextUsageProjection`，以及共享 fixture 留空的三个状态栏字段（`context`、`diagnosticsCount`、`pendingGateCount`） | `tests/statusbar.spec.ts` 中已填满的状态栏 fixture |
| 全部 `d1*` | `agentAdapters[0].models` | `tests/composer_controls.spec.ts` 中的适配器 fixture |
| `d6-actions`、`d6-error` | 一个已停止的会话，其 `restart` 携带 session id、`close_lane` 携带 lane id | `tests/d6_recovery.spec.ts` 中的 `STOPPED` fixture |
| `d12-actions` | 已记录必需证据、验证方满足，两个动作都可用且 code 为 `null` | `tests/d12_integration_gate.spec.ts` 中的 `DECIDABLE` fixture |
| `d12-conflict-content` | 在 Lane B 的 bounce 上追加第二个原因不同（`already_applied`）的被拒 hunk，以 Core 自己的 `ConflictContent` 线上形式写入，因此截图能显示每次拒绝都带各自的归类与补救说明。原像折叠条被展开，因为折叠的第三侧无法证明该面板所作的声明 | `tests/d12_integration_gate.rs` 中的 `d12_projects_the_hunks_two_sides_and_preimage_core_published_for_a_bounce` |
| `d12-conflict-omitted` | 同一 bounce 的内容替换为：`revision` 基线、一个渲染出的文件、一个 `omitted` 文件，并置 `truncated` | `tests/d12_integration_gate.rs` 中的 `d12_keeps_an_omitted_file_and_a_truncated_payload_distinct_from_an_empty_conflict` |
| `d12-conflict-none` | 无增量；这是未经改动的 `merge-gate.json` 投影，其 bounce 由 Core 发布时就不带任何内容 | `tests/d12_integration_gate.rs` 中的 `d12_leaves_a_bounce_without_content_absent_rather_than_empty` |
| `d11` | 已探测的 `/workspace/demo` rust 项目，提供方处于凭据锁定状态 | `tests/d11_intake.spec.ts` 中的已探测项目 fixture |
| `d11`、`d11-recent` | 交给屏幕最近工作端口的同一份两项目 `RecentWorkResult` | `tests/d11_intake.spec.ts` 中的已加载行 fixture |
| `d14-audit-scoped` | 仅把 `scope` 设为 `revert:revert-1` 对象；行仍是生成页面本身，因为截图不得为过滤器编造记录 | `tests/d14_audit_trail.rs` 的 `a_scoped_query_passes_the_exact_object_through_and_reports_the_scope` |
| `d14-raw-fallback` | 清空 `capabilityAvailable`，行为空且 outcome 为 idle —— 表达「缺失」而非「为空」 | `tests/d14_audit_trail.rs` 的 `audit_mode_is_unavailable_and_sends_nothing_without_the_core_capability` |
| `palette` | 交给 `loadPaletteCrossLane` 的一条跨 Lane 合并闸与一条询问 | `tests/d12_integration_gate.spec.ts` 的闸 fixture，以及 D1 fixture 本就带的那条 `liveWork.approvals` |
| `palette`、`palette-files` | 交给 `loadPaletteFiles` 的六条真实 Viden 路径，按 Core 的字典序排列，字节大小固定 | `tests/command_palette.spec.ts` 的已加载清单 fixture，以及 `tests/workspace_files.rs` 断言的 page 形状 |
| `d10-ticker` | 一页四条、横跨两个交错项目的 audit 记录，经屏幕自己的 `applyEvents` 应用 | 规范 fixture `audit-ordering.json` 与 `crates/core/tests/frontend_contract_v1.rs` 中的 `audit_ordering_fixture_orders_two_projects_as_one_newest_first_timeline` |
| `review*` | 一页 `WorkspaceDiffLoaded`：与规范扩展 fixture 相同的两个条目 —— 一个带真实 hunk 的已暂存 `modified` 文件，以及一个计数仍真实的 `omitted` 新增 —— 加上同样的 `truncated: true` 页标志。`review-rejected` 把 outcome 换成该 fixture 自己的 `CommandRejected` 原因；`review-empty` 保留页并清空 `entries` | 扩展 fixture `structured-diff.json` 与 `tests/workspace_diff.rs` 断言的页形状 |
| `review-commit*`、`review-push-no-upstream`、`review-rejected-action` | 同一页，外加一份 `OperatorGitProjection`：基线是「Core 发布了 `runtime.operator_git`、且本客户端有一个可代行的 owner」，其余都是它的增量。三个答案取自规范 fixture `operator-git.json` 自己的三条：一次批准后完成的 `Commit`（`ahead: 2`、工作区干净），一次以 `NoUpstream` 失败并带 git 原话的 `Push`，以及一次在执行任何效果前就被 `git_add` deny 规则拒绝的 `Stage` | 扩展 fixture `operator-git.json` 与 `tests/operator_git.rs` 断言的投影 |
| `approval-hunks` | 共享的待审批被改指到 `edit_file`，并附上该 fixture 的单文件 `decision_context` 与 `base_sha256`。改工具是刻意的：`edit_file` 正是 Core 会附上下文的三种提议之一，而带 hunk 的 `shell` 审批会是一张 Core 从不发布的东西的截图 | `structured-diff.json` 中的 `edit_file` 审批，以及 `tests/decision_context.rs` 中的 `an_approval_with_a_decision_context_projects_its_rows_and_base_hash` |
| `lane-rail`、`project-picker`、`project-switch-confirm` | 一份两项目的 `RecentWorkResult`，时间戳是相对冻结时钟的偏移，因此渲染出的相对时间稳定。当前打开的根目录被刻意包含在内——选择器必须把它从「最近」中剔除，而不是提供切换到已经打开的项目 | `tests/recent_work.rs` 断言的 `RecentWorkLoaded` 载荷 |

共享 D1 fixture 的 `topbarSource.project` 现在携带 `viden` 而不是 `null`。这是 Core
发布的名称，也正是标题栏选择器、侧栏 `.wsroot` 分组头与选择器「工作区内」一行共同
渲染的内容；回退到路径的分支仍由 `tests/cockpit_topbar.spec.ts` 与
`tests/lane_rail.spec.ts` 覆盖，它们显式把该字段覆写为 `null`。

## 如何运行

在拥有 `apps/gui/**` 的 worktree 里启动 dev server：

```bash
npm --prefix apps/gui run dev -- --port 4173 --strictPort
```

然后在授权的 Browser 运行时里按 1440x900 视口逐个打开下列 URL，等待
`data-capture-ready` 出现后截图。这与 `tools/capture-d1-visual.sh` 一致：只固化
URL 与尺寸，不在该运行时之外调用浏览器自动化。

所有 URL 共用前缀
`http://localhost:4173/evidence/main-window-interactions/qa.html`。

| 状态 | URL | 截图必须体现什么 |
| --- | --- | --- |
| `d1` | `…/qa.html?state=d1` | 完整驾驶舱；标题栏项目选择器带分支与 dirty 标记，旁边是 `↑/↓` 与工作树 chip；九个状态栏分段全部带事实，另加待决闸提示；三个 Composer 选择器胶囊 |
| `d1-mode-menu` | `…/qa.html?state=d1-mode-menu` | 工作模式弹层在 Composer 上方展开，当前模式标记为选中 |
| `d1-model-menu` | `…/qa.html?state=d1-model-menu` | 模型弹层展开，同时显示提供方分组与 Core 发布的适配器分组 |
| `permission-ask` | `…/qa.html?state=permission-ask` | Core 发布的权限坞，尚未有任何裁决：命令、类型化事实行，以及五个动作中 `Always` / `Edit` 在 `GUI-CORE-003` 之下禁用——它是读取重定向状态时的基线 |
| `permission-deny-redirect` | `…/qa.html?state=permission-deny-redirect` | 同一屏在点击「拒绝」之后：编辑器提示语变为「Tell Codex what to do instead…」并持有光标。harness 中的 `sendPermission` 永不 resolve，这正是要点——提示语在拒绝被**派发**时切换，对 Core 的答复不作任何声明 |
| `settings` | `…/qa.html?state=settings` | 设置面板覆盖在驾驶舱上并带未保存草稿，以面板自身顶部取景：Provider 与模型卡列出 Core 发布的 provider 组与 adapter 组并标出当前模型；权限卡列出全部五个 Core 档位——UI 标签、Core CLI 标识符、一行说明——并标出当前档位；以及只读的工作目录行；下方取消与保存均可用 |
| `settings-unavailable` | `…/qa.html?state=settings-unavailable` | 同一面板点名缺失的 `ui.preference_persistence` 能力，保存禁用，语言与外观控件只读——而 Provider 与权限各行仍**可操作**，因为它们走的是 `SelectModel` / `SetPermissionLevel`，不受该 capability 门控 |
| `d6-actions` | `…/qa.html?state=d6-actions` | 恢复界面上「重启智能体」与「关闭 Lane」可用，并展开检查事实 |
| `d6-error` | `…/qa.html?state=d6-error` | 同一界面在重启被拒后，把 Core 的拒绝理由渲染成告警 |
| `d12-actions` | `…/qa.html?state=d12-actions` | 合并闸的批准可用，退回理由输入框已填写且可用 |
| `d12-blocked` | `…/qa.html?state=d12-blocked` | 同一闸的批准不可用并点名 `missing_evidence`，理由输入框禁用 |
| `d12-conflict-content` | `…/qa.html?state=d12-conflict-content` | 该 bounce 的冲突面板：「两侧加上补丁原像 —— 不是合并结果」的声明、点名该闸已评审证据及其绑定 chip 的「读取基线」行，随后每个 hunk 一枚原因 chip 与补救说明、按 Core 自己的起始行编号的 OURS 与 THEIRS 并排、以及展开的 BASE 折叠条。其下是同一 Lane 的 Lane 应用冲突及其 `revision` 基线。全屏没有任何合并后文本，也没有解决控件（`GUI-CORE-015`） |
| `d12-conflict-omitted` | `…/qa.html?state=d12-conflict-omitted` | 触及上限的载荷：文件之上的截断横幅、一个渲染出的文件，以及保留为条目、说明其 hunk 未展示的 `assets/atlas.png` —— 绝不写成「无冲突」 |
| `d12-conflict-none` | `…/qa.html?state=d12-conflict-none` | 一条 Core 未发布内容的 bounce，且 capability 已通告：面板说明 Core 未为该 bounce 发布内容、并说明操作者退回按契约本就不带内容，同时**不**点名 capability —— 那是另一种缺失 |
| `d11` | `…/qa.html?state=d11` | 项目接入屏，显示已探测项目与提供方告警 |
| `d11-recent` | `…/qa.html?state=d11-recent` | 同一接入屏滚动到「最近工作」面板，显示 Core `QueryRecentWork` 行（名称、相对时间、会话数、规范根目录），替代已退役的静态不可用文案 |
| `d2-review-pending` | `…/qa.html?state=d2-review-pending` | 选中待处理评审，「接受评审」「驳回评审」均可用，评审意见已键入，回执说明裁决已发出而 Core 尚未记录 |
| `d2-review-confirmed` | `…/qa.html?state=d2-review-confirmed` | 同一评审在 Core 记录裁决之后，前后完全自洽：队列行显示 `accepted`，命令栏计数降为 1，审计落点显示裁决自身的审计 id，两个裁决动作以 `D2-REVIEW-SETTLED` 禁用，评审意见已清空且禁用，下方是确认回执 |
| `d2-review-rejected` | `…/qa.html?state=d2-review-rejected` | 同一评审在 Core 拒绝之后：原样渲染 Core 自己的拒绝语句作为告警，评审意见保留 |
| `d2-review-blocked` | `…/qa.html?state=d2-review-blocked` | 两个裁决动作均禁用并点名 `D2-NO-REVIEWER-ACTOR`，动作栏下方完整说明原因，评审意见输入框禁用——既不「可点却无效」，也不隐藏 |
| `d2` | `…/qa.html?state=d2` | D2 打开时的队列：三个分组及其计数、选中第一条闸审批、契约分组以 `GUI-CORE-013` 标注为已决历史，该审批自身的 diff 行以 `GUI-CORE-012` 声明不可用 |
| `d2-contract` | `…/qa.html?state=d2-contract` | 选中契约记录：确认与驳回**可见且禁用**，各自标注 `GUI-CORE-013`，理由在动作栏下方完整写出一次。Core 发布的每条契约都已裁决，且会拒绝对已记录 id 的再次裁决，因此此处的可点裁决只可能得到一次拒绝 |
| `d4` | `…/qa.html?state=d4` | 已复核 starter Lane 向导停在复核步：route、gate strength、执行目标、预算、worktree、base revision 全部原样来自 Core 的 preview，其上没有任何选择器或编辑器 |
| `d13` | `…/qa.html?state=d13` | 舰队看板：一个 Core 工作流 DAG，其中被阻塞节点点名 Core 写入的依赖记录作为阻塞原因；handoff 条说明 Core 未记录任何 handoff |
| `d10-blind` | `…/qa.html?state=d10-blind` | 成本不可计量的 terminal Lane 带「成本不可计量路由」标记与四项有界运行事实（累计耗时、运行次数、已应用 diff、最近退出码），旁边是完全不带这些事实的可计量 ACP Lane |
| `d10-blind-unobserved` | `…/qa.html?state=d10-blind-unobserved` | 同一盲路由 Lane 在 Core 观测到任何运行之前：只有标记与「尚未观测到运行」的说明，没有任何补零的事实 |
| `d14-audit` | `…/qa.html?state=d14-audit` | 模式切换里「审计轨迹」按下，旁边是「原始事件回放（诊断）」；三条 newest-first 审计行，每行显示 Core 原始的点分 `action` key、actor（agent 行带 `codex-acp`）、outcome（`denied` 与 `success` 明显区分）、关联对象 chip、有界参数 chip，以及明确标出时区的可读时间 `YYYY-MM-DD HH:MM:SS UTC`；Core 页面未完，因此显示加载更早控件 |
| `d14-audit-scoped` | `…/qa.html?state=d14-audit-scoped` | D12 回滚行打开的同一条轨迹：头部带可移除的 `Scoped to revert · revert-1` chip，移除后重新发起无范围查询 |
| `d14-raw-fallback` | `…/qa.html?state=d14-raw-fallback` | 缺少 `runtime.audit` 的 Core：原始模式按下、审计按钮禁用、说明点名该 capability，下方是回放行，其中无法解码的行被保留并高亮 |
| `evidence` | `…/qa.html?state=evidence` | 从命令面板的 `Open evidence` 行打开的 EvidenceView：类型 chip 中 `task_summary` 排在最后而非被隐藏，搜索框自陈其范围，`无日期` 组排在**最前**、其后是按 Core 自身升序排列的两个本地日期组，选中的 `patch` 行的规范字节经共享 diff 行渲染（`GUI-CORE-025`） |
| `evidence-text` | `…/qa.html?state=evidence-text` | 同一列表中选中一条 `test_result` 行，详情侧栏滚动到其内容：有界文本、说明 Core 的 256 KiB 上限把它截断且这并不表示证据很短的句子、`已按 <sha256> 校验` 一行、关联 chip，以及因该行不是 `patch` 而禁用的「在评审中打开」 |
| `evidence-summary-only` | `…/qa.html?state=evidence-summary-only` | 仅供展示的 `task_summary` 行：报告没有任何 canonical 字段并说明该条目未指向规范字节，`Unavailable { SummaryOnly }` 渲染为「仅供展示的证据——Core 未持有其规范字节」 |
| `evidence-unavailable` | `…/qa.html?state=evidence-unavailable` | `patch` 行上的相反缺席：`Unavailable { HashMismatch }` 渲染为「规范字节校验失败——不予展示」，使用错误色，整屏没有任何正文 |
| `evidence-empty` | `…/qa.html?state=evidence-empty` | 「此范围内没有证据。」——唯一可以这样画的状态，且画在 Core 确实答复过的一页之上——并把 `complete` 明说为「档案已完整」，而不是靠 `加载更早` 按钮的缺席来表示 |
| `evidence-rejected` | `…/qa.html?state=evidence-rejected` | Core 对越界 `kinds` 的拒绝原文放进 `role=alert`，其 `hint:` 行保持独立成行，什么都未加载，没有翻页脚，也没有空档案的句子 |
| `nav-d2-in-cockpit` | `…/qa.html?state=nav-d2-in-cockpit` | 决策队列作为**中央面板视图**：四周驾驶舱 chrome 原封不动——带 source 块的标题栏、`决策` 槽位标记 `aria-current` 并带上 Core 自己的待处理计数徽标的活动 rail、上下文坞、仍然对准选中 Lane 的 composer、带待审闸芯片的状态栏——外加该视图自己的头部与 Close 控件（位置与 DiffReview 的一致） |
| `nav-d14-in-cockpit` | `…/qa.html?state=nav-d14-in-cockpit` | 同一外壳中的审计轨迹，也就是 `D-AUDIT` 从证据行或 D12 基线芯片单向链接如今落到的地方：rail 的 `审计时间线` 槽位标记为当前，视图头部之下是模式切换与三条审计行，而对话只差一个 `Esc` |
| `nav-sidebar-floating-peek` | `…/qa.html?state=nav-sidebar-floating-peek` | `D-SIDEBAR` **浮动**模式——该决策的默认值——经键盘路径（Lane rail 槽位）peek 打开：侧栏是**覆盖在整宽转录之上的浮层**而非布局列，12px 热区连同 `.edgehint` 提示条贴在活动 rail 右缘，rail 上（设置齿轮之上）的 pin 读作未固定 |
| `nav-statusbar-config` | `…/qa.html?state=nav-statusbar-config` | `D-STATUSBAR` 配置齿轮打开：`.sbcfg` 弹层列出六个环境段及其勾选框，页脚写明身份项与可操作项始终固定，而 `MODE`、`PERM`、`LANE` 与待审闸芯片就在其后的状态栏上、却不在列表中 |
| `d10-actions` | `…/qa.html?state=d10-actions` | 驾驶舱中的 Lane 监视器卡片动作行：**两张卡片上都有 接管 / 停止 / 暂停 / 终止**，在 Core 发布了 Agent 会话的那条 Lane 上「停止」可用、在未发布会话的那条上禁用，两张卡片的「暂停」「终止」都禁用并注明 `RuntimeCommand` 并不携带的命令（G6） |
| `d13-drill` | `…/qa.html?state=d13-drill` | 同一列里同时给出两种 Lane 绑定答案：Core 绑定了 Lane 的节点带着可见的 `Open Lane lane_core ↗` 一行与 `role="button"`；同一个生成节点去掉绑定后的副本说明 Core 未绑定任何 Lane，也不提供点击（G6） |
| `d14-filtered` | `…/qa.html?state=d14-filtered` | 启用了 `agent` 执行者筛选的审计轨迹：执行者标签是**本页**携带的值（`全部执行者`、`operator`、`agent`——没有 `system`，因为本页没有这类行），四个时间标签中 `全部时间` 处于按下态，筛选条下的说明点明切分范围是已加载页并注明 `GUI-CORE-024`，结果分布条读作 `已加载页（已筛选）的结果分布 · 1 of 3 · denied 1`，剩下一行记录，`导出` 禁用（G6） |
| `d2-rail-badge` | `…/qa.html?state=d2-rail-badge` | 同一个 Core 数字出现在打印它的几处：侧栏 D2 气泡显示 `7`、状态栏 `⏸ 7 gate waiting` 段落，以及在中央面板打开的决策队列。该计数刻意不取共享 fixture 的 `2`，以便截图证明气泡读的是投影而不是常量（G6） |
| `palette` | `…/qa.html?state=palette` | 从标题栏按钮打开、覆盖在驾驶舱之上的 ⌘K 命令面板，四个分区全部可见——动作、跳转到（跨 Lane 的闸与询问，加上本 Lane）、设置，以及列出 Core 已发布工作区清单的「文件」分区 |
| `palette-files` | `…/qa.html?state=palette-files` | 同一个面板但预先限定到 `~`，单独框出 Core 发布的清单：六条路径按 Core 的字典序排列，每条带 Core 报告的条目类型，没有任何一条是客户端自行发现的（`GUI-CORE-022`） |
| `d10-ticker` | `…/qa.html?state=d10-ticker` | Lane 卡片下方的 D10 事件走马灯：Core 审计时间线的一页有界 newest-first 记录，两个项目交错出现，因此该条展示的是跨项目的同一个顺序而不是按项目分组的列表；每行携带 Core 的稳定 id、原样的点分 action key、owner 与时间戳（`GUI-CORE-014`） |
| `review` | `…/qa.html?state=review` | 由标题栏变更标记打开的 DiffReview：文件树带 `M`/`A` 字形、承载 Core 自身 `staged` 事实的逐行暂存开关、逐文件计数与表头合计；统一视图带 Git 自己的 `@@` 头与逐侧行号；页级截断横幅；`分栏` 可见且禁用；提交栏已可用，但因尚未输入信息，`提交` 与 `提交并推送` 处于禁用 |
| `review-commit` | `…/qa.html?state=review-commit` | 提交栏在动作中：通过生产环境的 input 监听器输入的提交信息、三个动作全部可用、标题栏同步芯片是一个可按的 `Push` |
| `review-commit-pending-approval` | `…/qa.html?state=review-commit-pending-approval` | `Ask` 路径 —— Core 为该 owner 发布了一条 `git` 审批，因此提交栏声明决策归权限坞所有，包括同步芯片在内的每个控件都不可用 |
| `review-commit-completed` | `…/qa.html?state=review-commit-completed` | 成功行由 Core *重新采样* 的 source 构成（`↑2 ↓0`、工作区干净），git 的输出收折在 `Git 输出` 之后 |
| `review-push-no-upstream` | `…/qa.html?state=review-push-no-upstream` | 一次 `Failed { NoUpstream }` 结果：该类别的本地化句子、其下 git 自己的原话，以及契约指名的那一个恢复动作 `推送并设置 upstream` —— 绝不是 `Pull` |
| `review-rejected-action` | `…/qa.html?state=review-rejected-action` | 同一条提交栏上的效果前 `CommandRejected`：Core 的原话未经编辑，置于 `role=alert` 中，既没有失败行也没有恢复动作 |
| `review-omitted` | `…/qa.html?state=review-omitted` | 同一视图选中第二个条目，于是「未显示 diff 行」的说明与依然真实的计数并列（`omitted`） |
| `review-rejected` | `…/qa.html?state=review-rejected` | Core 的拒绝在 `role=alert` 中原样呈现，表头没有文件计数，也没有空树句子 |
| `review-empty` | `…/qa.html?state=review-empty` | 「工作区没有变更」—— 唯一可以这样渲染的状态，且建立在 Core 确实回答过的页之上 |
| `approval-hunks` | `…/qa.html?state=approval-hunks` | D1 权限坞把 `decision_context` 渲染成 hunk 行，上方是「预览基于 … 计算」，`input_preview` 在其上、决策行固定在其下（`GUI-CORE-012`） |
| `lane-rail` | `…/qa.html?state=lane-rail` | `D-SIDEBAR` **pinned** 模式：侧栏作为真正的布局列（设计默认 218px），把工作面推开而不是覆盖它，活动 rail 的 pin 标记为已按下。画面显示 Core 监督的那一个 `.wsroot` 项目分组 `viden`，其 `▾` 折叠、Lane 计数、分组内 `＋`、嵌套其下的 Lane，以及 `＋ 添加项目…` 页脚——没有第二个分组，也没有「Global」小节 |
| `project-picker` | `…/qa.html?state=project-picker` | 选择器在标题栏 `▾` 之下展开，三列同时可见：可用的 `添加目录…` 与两行点名 `GUI-CORE-023` 的禁用行、当前打开项目的唯一「工作区内」行及其 lane 计数，以及一行带相对时间的「最近」 |
| `project-switch-confirm` | `…/qa.html?state=project-switch-confirm` | 同一选择器在点击最近项目后进入内联确认：目标根目录、点名 `GUI-CORE-023` 的替换说明、正在运行的工作计数，以及「取消」与「切换工作区」两个按钮 |

`mode=dark|light` 与 `locale=en|zh-CN` 每个状态都接受，并统一走共享的
`resolveTheme`，harness 不携带第二套配色。`mode=light` 同时选用 `ice` 皮肤，
与设计的搭配一致。建议的语言与皮肤验证：
`…/qa.html?state=d1&mode=light&locale=zh-CN`。

## 已捕获截图

2026-08-21 以 headless Chrome
(`--headless --window-size=1440,900 --virtual-time-budget=6000`)对 4173 端口
的 vite 开发服务器采集,随后人工目检(评审抽样 6/11;构建时 11 个状态均已做
DOM 级验证)。

| 文件 | 状态 | 视口 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [d1-1440x900-dark-en.png](d1-1440x900-dark-en.png) | d1 | 1440x900 | dark | en |
| [d1-mode-menu-1440x900-dark-en.png](d1-mode-menu-1440x900-dark-en.png) | d1-mode-menu | 1440x900 | dark | en |
| [d1-model-menu-1440x900-dark-en.png](d1-model-menu-1440x900-dark-en.png) | d1-model-menu | 1440x900 | dark | en |
| [settings-1440x900-dark-en.png](settings-1440x900-dark-en.png) | settings | 1440x900 | dark | en |
| [settings-unavailable-1440x900-dark-en.png](settings-unavailable-1440x900-dark-en.png) | settings-unavailable | 1440x900 | dark | en |
| [d6-actions-1440x900-dark-en.png](d6-actions-1440x900-dark-en.png) | d6-actions | 1440x900 | dark | en |
| [d6-error-1440x900-dark-en.png](d6-error-1440x900-dark-en.png) | d6-error | 1440x900 | dark | en |
| [d12-actions-1440x900-dark-en.png](d12-actions-1440x900-dark-en.png) | d12-actions | 1440x900 | dark | en |
| [d12-blocked-1440x900-dark-en.png](d12-blocked-1440x900-dark-en.png) | d12-blocked | 1440x900 | dark | en |
| [d11-1440x900-dark-en.png](d11-1440x900-dark-en.png) | d11 | 1440x900 | dark | en |
| [d11-recent-1440x900-dark-en.png](d11-recent-1440x900-dark-en.png) | d11-recent | 1440x900 | dark | en |
| [d1-1440x900-light-zh-CN.png](d1-1440x900-light-zh-CN.png) | d1 | 1440x900 | light | zh-CN |

八张带标题栏的截图（`d1*`、`settings*`、`d6-*`）已于 2026-08-21 在标题栏 git 块
落地后重新采集，显示带分支与脏标记点的项目选择器及两个 `.gitops` chip
（`↑1 ↓0`、`⎇ 1 个工作树`）；独立屏 `d12*` 不含驾驶舱标题栏，原截图仍然有效。
`d11-recent` 于 2026-08-29 首次采集——历史面板自此渲染 Core 的最近工作行，
替代已退役的 `GUI-CORE-007` 文案；同日重采的 `d11` 与原图逐字节一致，
因为该面板位于此视口折叠线之下。

命令面板在 `.tbtools` 中新增了一个 `.tbtbtn` 按钮;八张带标题栏的截图与新的
`palette` 状态已于 2026-08-21 在 1440x900 下重新采集并人工目检(palette 截图
显示前缀图例、全部四个分组、kbd 提示与禁用的 Files 行)。

| 文件 | 状态 | 视口 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [palette-1440x900-dark-en.png](palette-1440x900-dark-en.png) | palette | 1440x900 | dark | en |

## 项目选择器与分组侧栏截图

九张带标题栏的截图(标题栏选择器获得设计稿的 `▾` 与按钮外观;侧栏获得 `.wsroot`
分组头与 `＋ 添加项目…` 页脚)连同三个新状态已于 2026-08-21 在 1440x900 下重新
采集并人工目检(选择器三列含两行 `GUI-CORE-023` 禁用行、当前项目与最近项目行、
切换确认对话框明示替换语义与影响计数)。

| 文件 | 状态 | 视口 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [project-picker-1440x900-dark-en.png](project-picker-1440x900-dark-en.png) | project-picker | 1440x900 | dark | en |
| [lane-rail-1440x900-dark-en.png](lane-rail-1440x900-dark-en.png) | lane-rail | 1440x900 | dark | en |
| [project-switch-confirm-1440x900-dark-en.png](project-switch-confirm-1440x900-dark-en.png) | project-switch-confirm | 1440x900 | dark | en |

## 监管欠账截图

2026-08-30 以 headless Chrome
(`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`)对 4173 端口的 vite 开发服务器采集，随后人工目检
（八张全部人工检查）。

| 文件 | 状态 | 视口 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [d2-review-pending-1440x900-dark-en.png](d2-review-pending-1440x900-dark-en.png) | d2-review-pending | 1440x900 | dark | en |
| [d2-review-confirmed-1440x900-dark-en.png](d2-review-confirmed-1440x900-dark-en.png) | d2-review-confirmed | 1440x900 | dark | en |
| [d2-review-rejected-1440x900-dark-en.png](d2-review-rejected-1440x900-dark-en.png) | d2-review-rejected | 1440x900 | dark | en |
| [d2-review-blocked-1440x900-dark-en.png](d2-review-blocked-1440x900-dark-en.png) | d2-review-blocked | 1440x900 | dark | en |
| [d10-blind-1440x900-dark-en.png](d10-blind-1440x900-dark-en.png) | d10-blind | 1440x900 | dark | en |
| [d10-blind-unobserved-1440x900-dark-en.png](d10-blind-unobserved-1440x900-dark-en.png) | d10-blind-unobserved | 1440x900 | dark | en |
| [d2-review-confirmed-1440x900-light-zh-CN.png](d2-review-confirmed-1440x900-light-zh-CN.png) | d2-review-confirmed | 1440x900 | light | zh-CN |
| [d10-blind-1440x900-light-zh-CN.png](d10-blind-1440x900-light-zh-CN.png) | d10-blind | 1440x900 | light | zh-CN |

两张 `d2-review-confirmed` 截图已于 2026-08-30 在 harness 改为按该 outcome 提供
生成的已裁决投影后重采；此前那一对把确认回执压在仍为 pending 的行之上，那不是
生产环境可以到达的状态。

两张 light/`zh-CN` 截图是这两块屏的语言与皮肤验证：新增的每条文案——评审意见
标题、三条 outcome 语句、成本不可计量标记与四项运行事实名称——都已翻译，两块屏
也都没有自带第二套配色。

## 已知限制

授权的 Browser 运行时能渲染并核对这些页面，但不能写 PNG 文件，因此在操作者实际
截图之前，本目录保存的是可复现的 harness 而不是已提交的图片。
`tools/capture-d1-visual.sh` 也刻意停在同一条边界上。上文列出的图片由操作者
直接运行 headless Chrome 采集，所用参数记录在各自表格旁。

## D14 审计截图

2026-08-31 使用 headless Chrome
（`--headless --disable-gpu --window-size=1440,900 --virtual-time-budget=6000`）
对 4177 端口上的 vite dev server 采集，并逐张目视复核（四张全部复核）。

| 文件 | 状态 | 视口 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [d14-audit-1440x900-dark-en.png](d14-audit-1440x900-dark-en.png) | d14-audit | 1440x900 | dark | en |
| [d14-audit-scoped-1440x900-dark-en.png](d14-audit-scoped-1440x900-dark-en.png) | d14-audit-scoped | 1440x900 | dark | en |
| [d14-raw-fallback-1440x900-dark-en.png](d14-raw-fallback-1440x900-dark-en.png) | d14-raw-fallback | 1440x900 | dark | en |
| [d14-audit-1440x900-light-zh-CN.png](d14-audit-1440x900-light-zh-CN.png) | d14-audit | 1440x900 | light | zh-CN |

light/`zh-CN` 截图是语言与皮肤验证：模式切换、屏幕标题、加载更早控件与范围 chip 文案
都已翻译，而行内每个 Core 值——点分 `action` key、对象 kind 与 id、参数 key 与 value
——都保持 Core 发布时的原样。这正是刻意的划分：把动作词汇表本地化会毁掉两份审计
时间线可互相 diff 这一性质。

行内时间戳落在这条划分的同一侧：它渲染为可读形式（`2023-11-14 22:28:20 UTC`）而不是
原始 epoch 秒，但格式固定、两种语言下时区都是 UTC——审计记录是要跨机器比对的证据，
随语言漂移的时钟会让两位读者对同一事实产生分歧。ISO 值保留在 `<time datetime>` 属性上
供机器读取。原始回放模式刻意保留 epoch 读数：本次改动后其 `d14-raw-fallback` 截图
逐字节不变，这正是诊断模式呈现未被改动的证明。

## 工作区清单与事件走马灯截图

2026-09-02 以 headless Chrome
（`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`）对 4188 端口上的 vite dev server 采集，并逐张目视复核
（四张全部复核）。

| 文件 | 状态 | 视口 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [palette-files-1440x900-dark-en.png](palette-files-1440x900-dark-en.png) | palette-files | 1440x900 | dark | en |
| [palette-files-1440x900-light-zh-CN.png](palette-files-1440x900-light-zh-CN.png) | palette-files | 1440x900 | light | zh-CN |
| [d10-ticker-1440x900-dark-en.png](d10-ticker-1440x900-dark-en.png) | d10-ticker | 1440x900 | dark | en |
| [d10-ticker-1440x900-light-zh-CN.png](d10-ticker-1440x900-light-zh-CN.png) | d10-ticker | 1440x900 | light | zh-CN |

这四张在视觉上关闭 `GUI-CORE-022` 与 `GUI-CORE-014`。`palette-files` 截图把原先永久
禁用的「文件」行换成了 Core 发布的清单，按 Core 自己的字典序排列，每条路径旁是 Core
报告的条目类型；两张图里没有任何一条路径是客户端自行发现的。`d10-ticker` 截图把
`d10.events.noOrderedLog` 那条不可用提示换成了审计时间线，其中两个交错的项目正是
`audit-ordering.json` 所证明性质的可视形态。

light/`zh-CN` 截图是这两个界面的语言划分，且与 D14 已记录的划分一致：分区标题、条目
类型列与走马灯标题会翻译，而每个 Core 值都保持 Core 发布时的原样——工作区相对路径，
以及走马灯的稳定 audit id 与点分 `action` key。把动作词汇表本地化会毁掉两份时间线
可互相 diff 这一性质，把路径本地化则会让它无法被打开。

## 设置的 Core 命令分区截图

2026-09-07 以 headless Chrome
（`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`）对 4199 端口上的 vite dev server 采集，并逐张目视复核
（三张全部复核）。

| 文件 | 状态 | 视口 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [settings-1440x900-dark-en.png](settings-1440x900-dark-en.png) | settings | 1440x900 | dark | en |
| [settings-unavailable-1440x900-dark-en.png](settings-unavailable-1440x900-dark-en.png) | settings-unavailable | 1440x900 | dark | en |
| [settings-1440x900-light-zh-CN.png](settings-1440x900-light-zh-CN.png) | settings | 1440x900 | light | zh-CN |

两张 dark 截图是在面板新增 Provider 与模型、权限两个分区之后重拍的。与之一同变化、
并且在图中可见的还有两处。其一，harness 现在把 `settings` 截图锚定在面板自身的滚动
顶部：草稿之后的重挂会聚焦被草拟的选项，否则两张新卡会被滚出画面。其二，面板的
`max-height` 现在会减去它自己的底部偏移量：齿轮把浮层锚在 rail 底部附近，面板一旦长
到这个高度，按整屏高度计算就会越过视口顶部，把标题与关闭按钮顶出可及范围。

`settings-unavailable` 是能力划分的证据。在点名缺失的 `ui.preference_persistence`
之下，语言与外观控件只读，而同一张图里 Provider 与权限的每一行仍可操作——它们走的是
`SelectModel` 与 `SetPermissionLevel`，不在该 capability 的门控之内。

light/`zh-CN` 截图是新增文案的语言与皮肤证据，其划分与审计时间线已记录的一致：分区
标题、档位标签、说明文字与工作目录标题会翻译，而每个 Core 值都保持 Core 发布时的
原样——五个 CLI 标识符（`ask`、`auto_edit`、`auto`、`read_only`、`full_access`）、
模型名、provider 组标签与工作区根目录。把 CLI 标识符本地化会毁掉这个标记唯一的存在
理由：拿这个面板去对照 Core 的日志。
\n
## 权限拒绝重定向截图

2026-09-07 以 headless Chrome
（`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`）对 4199 端口上的 vite dev server 采集，并逐张目视复核
（三张全部复核）。

| 文件 | 状态 | 视口 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [permission-ask-1440x900-dark-en.png](permission-ask-1440x900-dark-en.png) | permission-ask | 1440x900 | dark | en |
| [permission-deny-redirect-1440x900-dark-en.png](permission-deny-redirect-1440x900-dark-en.png) | permission-deny-redirect | 1440x900 | dark | en |
| [permission-deny-redirect-1440x900-light-zh-CN.png](permission-deny-redirect-1440x900-light-zh-CN.png) | permission-deny-redirect | 1440x900 | light | zh-CN |

这几张是设计中「拒绝之后的编辑器」（`Viden - 桌面驾驶舱 (GUI).html`，权限坞旁边的
编辑器）。这一对应当对照着看：同一屏在一次点击前后，只有提示语与光标不同。其余一切
都没有动，因为其余一切都不许动——重定向只是呈现，`feedback` 仍为 `null`
（GUI-CORE-019），而权限坞仍显示该请求，因为 harness 的 `sendPermission` 永不答复。

Agent 名取自 Core 发布的 adapter `displayName`，这也是 harness 的待审投影为何带一条
ACP 会话、且其 session id 正是 Core `laneAgent` 事实已经点名的那一个：没有 ACP 会话
时，提示语会退回通用措辞，而不是从 agent id 猜一个名字。两条路径都在
`tests/permission_deny_redirect.spec.ts` 中有覆盖。

light/`zh-CN` 截图是语言与皮肤证据：提示语翻译为「告诉 Codex 改做什么…」，而
`Codex` 本身保持 Core 发布时的原样。把 adapter 的显示名翻译掉，会让重定向点名一个
操作者在驾驶舱其他任何地方都找不到的 agent。

## 活动 rail 目的地重拍

Rail 的五个路由槽位已按其实际打开的屏幕重新命名，并从已登记的
`GUI/gui-icons.jsx` 集合重新取字形，因此每一张显示驾驶舱外壳的截图里 rail 都不一样。
全部十八张带驾驶舱的图已于 2026-09-07 以 headless Chrome
（`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`）对 4199 端口上的 vite dev server 重拍，并做目视复核
（十八张中抽查六张；十八张全部由 `tests/activity_rail_destinations.spec.ts`
在构建期做 DOM 校验）。

| 文件 | 状态 | 视口 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [d1-1440x900-dark-en.png](d1-1440x900-dark-en.png) | d1 | 1440x900 | dark | en |
| [d1-1440x900-light-zh-CN.png](d1-1440x900-light-zh-CN.png) | d1 | 1440x900 | light | zh-CN |
| [d1-mode-menu-1440x900-dark-en.png](d1-mode-menu-1440x900-dark-en.png) | d1-mode-menu | 1440x900 | dark | en |
| [d1-model-menu-1440x900-dark-en.png](d1-model-menu-1440x900-dark-en.png) | d1-model-menu | 1440x900 | dark | en |
| [palette-1440x900-dark-en.png](palette-1440x900-dark-en.png) | palette | 1440x900 | dark | en |
| [palette-files-1440x900-dark-en.png](palette-files-1440x900-dark-en.png) | palette-files | 1440x900 | dark | en |
| [palette-files-1440x900-light-zh-CN.png](palette-files-1440x900-light-zh-CN.png) | palette-files | 1440x900 | light | zh-CN |
| [lane-rail-1440x900-dark-en.png](lane-rail-1440x900-dark-en.png) | lane-rail | 1440x900 | dark | en |
| [project-picker-1440x900-dark-en.png](project-picker-1440x900-dark-en.png) | project-picker | 1440x900 | dark | en |
| [project-switch-confirm-1440x900-dark-en.png](project-switch-confirm-1440x900-dark-en.png) | project-switch-confirm | 1440x900 | dark | en |
| [settings-1440x900-dark-en.png](settings-1440x900-dark-en.png) | settings | 1440x900 | dark | en |
| [settings-1440x900-light-zh-CN.png](settings-1440x900-light-zh-CN.png) | settings | 1440x900 | light | zh-CN |
| [settings-unavailable-1440x900-dark-en.png](settings-unavailable-1440x900-dark-en.png) | settings-unavailable | 1440x900 | dark | en |
| [d6-actions-1440x900-dark-en.png](d6-actions-1440x900-dark-en.png) | d6-actions | 1440x900 | dark | en |
| [d6-error-1440x900-dark-en.png](d6-error-1440x900-dark-en.png) | d6-error | 1440x900 | dark | en |
| [permission-ask-1440x900-dark-en.png](permission-ask-1440x900-dark-en.png) | permission-ask | 1440x900 | dark | en |
| [permission-deny-redirect-1440x900-dark-en.png](permission-deny-redirect-1440x900-dark-en.png) | permission-deny-redirect | 1440x900 | dark | en |
| [permission-deny-redirect-1440x900-light-zh-CN.png](permission-deny-redirect-1440x900-light-zh-CN.png) | permission-deny-redirect | 1440x900 | light | zh-CN |

每张图里都有两个字形换了形状：第四个槽位现在是决策队列的 `decide` 对勾，而不是
`review` 书本；第七个是舰队看板的 `fleet` 节点图，而不是 `inbox` 托盘。标签本身是
tooltip 与可访问名称文本，在静态截图里看不到——每个槽位的路由与名称之间的一致性改由
`tests/activity_rail_destinations.spec.ts` 断言，那才是它该待的地方。

## `0.3.3` H1 卫生批次截图

`0.3.3` 设计缺口复核发现四个 harness 能渲染但从未截过的状态，以及两张已经过期的
截图。2026-09-09 使用 headless Chrome（`--headless --disable-gpu
--hide-scrollbars --window-size=1440,900 --virtual-time-budget=6000`）对 4173
端口上的 vite dev server 采集，随后逐张目视复核（八张全部复核）。

| 文件 | 状态 | 视口 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [d2-1440x900-dark-en.png](d2-1440x900-dark-en.png) | d2 | 1440x900 | dark | en |
| [d2-contract-1440x900-dark-en.png](d2-contract-1440x900-dark-en.png) | d2-contract | 1440x900 | dark | en |
| [d2-contract-1440x900-light-zh-CN.png](d2-contract-1440x900-light-zh-CN.png) | d2-contract | 1440x900 | light | zh-CN |
| [d4-1440x900-dark-en.png](d4-1440x900-dark-en.png) | d4 | 1440x900 | dark | en |
| [d13-1440x900-dark-en.png](d13-1440x900-dark-en.png) | d13 | 1440x900 | dark | en |
| [d10-blind-1440x900-dark-en.png](d10-blind-1440x900-dark-en.png) | d10-blind | 1440x900 | dark | en |
| [d10-blind-1440x900-light-zh-CN.png](d10-blind-1440x900-light-zh-CN.png) | d10-blind | 1440x900 | light | zh-CN |
| [d10-blind-unobserved-1440x900-dark-en.png](d10-blind-unobserved-1440x900-dark-en.png) | d10-blind-unobserved | 1440x900 | dark | en |

`d2`、`d2-contract`、`d4`、`d13` 是新增的 harness 状态。`d2` 与 `d13` 原样渲染生成
投影（`d2.json`、`d13.json`）；`d2-contract` 是 `d2.json` 上的一处 delta——选中契约
记录，并使用 `tests/d2_decisions.rs` 断言的可用性；`d4` 手写自
`tests/d4_lane_create.spec.ts`，因为 D4 的已复核 preview 存放在 adapter 自己的槽位
而不在 `RuntimeViewState` 中，`tests/capture_projections.rs` 无从投影它。

两张 `d10-blind*` 是**重采**，重采的理由本身就是重点：已提交的这一对底部仍写着
「Core publishes no ordered event log in the view state, so the event stream is
unavailable · GUI-CORE-014」。该请求已随 Core 发布审计时间线而关闭，`d10.json`
现在携带 `unavailable: []`，事件条读作「Core publishes no audit timeline, so the
event stream is unavailable」——因为这两个状态没有应用任何审计页，而 `d10-ticker`
应用了。旧图是一块已不存在的屏幕的证据。

`d2-contract` 这一对是同一处改动的双语佐证：英文截图读作 "Core already recorded
this contract's decision, and publishes no pending contract to confirm."，中文读作
「Core 已记录该契约的裁决，且不发布任何待确认契约。」，而编码 `GUI-CORE-013` 在两者
中都不翻译，因为原因编码是标识符而不是散文。

## DiffReview 与审批 hunk 截图

2026-09-09 以无头 Chrome 采集
（`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`），针对 4211 端口上的 vite 开发服务器，随后人工复核
（六张全部抽检）。

五张 `review*` 已于 2026-09-09 针对 4173 端口重新采集：提交栏已经可用 —— 信息框是
真正的 `<input>`、每个文件行带暂存/取消暂存开关，且禁用的 `提交` 不再保留实心绿底，
因为 55% 不透明度的实心主按钮看上去仍像「可以按」。

| 文件 | 状态 | 视口 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [review-1440x900-dark-en.png](review-1440x900-dark-en.png) | review | 1440x900 | dark | en |
| [review-1440x900-light-zh-CN.png](review-1440x900-light-zh-CN.png) | review | 1440x900 | light | zh-CN |
| [review-omitted-1440x900-dark-en.png](review-omitted-1440x900-dark-en.png) | review-omitted | 1440x900 | dark | en |
| [review-rejected-1440x900-dark-en.png](review-rejected-1440x900-dark-en.png) | review-rejected | 1440x900 | dark | en |
| [review-empty-1440x900-dark-en.png](review-empty-1440x900-dark-en.png) | review-empty | 1440x900 | dark | en |
| [approval-hunks-1440x900-dark-en.png](approval-hunks-1440x900-dark-en.png) | approval-hunks | 1440x900 | dark | en |

这些是登记的 DiffReview 族（`docs/DESIGN-REF.md`「D1 次级视图」·`D-RAILNAV ①`），
以及 D1 权限坞渲染 Core 决策上下文的画面。五个评审状态都按操作者的方式打开 ——
经标题栏的变更标记 —— 而不是直接挂载屏幕，因此每张图都同时验证了入口。

这一组的选取原则是：源码控制契约陈述的每条诚实规则，都恰好有一张图可以证伪它。

| 图 | 它所证明的规则 |
| --- | --- |
| `review` | `truncated` 的页带有横幅；文件树显示 `M`/`A` 字形、Core 自己的暂存 `✓` 与逐文件计数；提交栏整体禁用并标注 `GUI-CORE-020`；`分栏` 可见且禁用 |
| `review-omitted` | 选中第二个条目，于是「未显示 diff 行（1284 行新增、0 行删除）—— 超出字节上限」与依然真实的计数并列成像。这是第一个文件的截图无法展示的那条规则 |
| `review-rejected` | Core 的拒绝文本连同 hint 原样呈现，表头**没有**文件计数 —— 「0 files · +0 −0」会是 Core 从未给出的数字，且会读作工作区干净 |
| `review-empty` | 唯一可以渲染成「工作区没有变更」的状态：Core 回答过、条目为零的页 |
| `approval-hunks` | 权限坞把 `decision_context` 渲染成 hunk 行，上方是「预览基于 3f79bb7b 计算」，`input_preview` 仍在其上，决策行固定在其下 |

`review-rejected` 与 `review-empty` 值得成对阅读：两张图的差别正是契约要求的差别。
一张是 Core 自己的话且没有计数，另一张是零计数加上空树句子。把两者合一的客户端会
产出两张一样的图。

light/`zh-CN` 那张是新增文案的语言与皮肤佐证：标题、分段控件、截断横幅、提交栏说明与
计数都翻译，而 `crates/types/src/diff.rs`、`@@` 头、diff 内容、分支名与编码
`GUI-CORE-020` 完全不变。翻译 `@@` 头或原因编码会破坏 diff 面板存在的唯一意义 ——
与评审者在终端里看到的内容对得上。

有两件图里能看见、但值得写明而不是留给眼睛的事。坞的动作行被固定、上下文自身滚动，
这是本批次做的改动：在 Core 发布决策上下文之前坞总能装进它的宿主，而让无界的预览滚动
整个坞会把「允许」「拒绝」挤到它们所要回答的那些行后面（`permission-ask` 未变，是
改动前的对照图）。另外，长于 236px 文件树的文件名仍会省略；拆分只保证目录一半先消失，
完整路径保留在该行的 title 与面板表头里。

## DiffReview 动作侧截图

2026-09-09 以无头 Chrome 采集
（`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`），针对 4173 端口上的 vite 开发服务器，随后人工复核
（六张全部抽检）。

| 文件 | 状态 | 视口 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [review-commit-1440x900-dark-en.png](review-commit-1440x900-dark-en.png) | review-commit | 1440x900 | dark | en |
| [review-commit-pending-approval-1440x900-dark-en.png](review-commit-pending-approval-1440x900-dark-en.png) | review-commit-pending-approval | 1440x900 | dark | en |
| [review-commit-completed-1440x900-dark-en.png](review-commit-completed-1440x900-dark-en.png) | review-commit-completed | 1440x900 | dark | en |
| [review-push-no-upstream-1440x900-dark-en.png](review-push-no-upstream-1440x900-dark-en.png) | review-push-no-upstream | 1440x900 | dark | en |
| [review-rejected-action-1440x900-dark-en.png](review-rejected-action-1440x900-dark-en.png) | review-rejected-action | 1440x900 | dark | en |
| [review-commit-completed-1440x900-light-zh-CN.png](review-commit-completed-1440x900-light-zh-CN.png) | review-commit-completed | 1440x900 | light | zh-CN |

这是同一个登记族的动作一半（`runtime.operator_git`、GUI-CORE-020）。每张图都挑成
「恰好只有一条契约规则能被它证伪」：

| 图 | 它证明的规则 |
| --- | --- |
| `review-commit` | 提交栏真的会动作：输入信息后 `提交` 与 `提交并推送` 可用，`全部暂存` 不需要信息，每个文件行都带暂存开关，标题栏芯片是一个可按的 `Push` |
| `review-commit-pending-approval` | `Ask` 属于权限坞。提交栏说明它在等哪个动作，包括同步芯片在内的每个控件都是禁用而不是隐藏 |
| `review-commit-completed` | 成功行由结果里重新采样的 `source` 构成（`↑2 ↓0`、工作区干净），**不是**由 git 的输出文本得出 —— 后者收折在 `Git 输出` 之后 |
| `review-push-no-upstream` | 通过权限门之后的失败是「结果」而不是「拒绝」：Core `NoUpstream` 类别的本地化句子、git 自己的原话，以及契约指名的那一个恢复动作 |
| `review-rejected-action` | 权限门之前的拒绝是 Core 的原话，逐字放在 `role=alert` 里 —— 旁边既没有失败句子，也没有恢复动作 |

`review-push-no-upstream` 与 `review-rejected-action` 值得成对阅读，这也是两张都要
存在的原因。两张都是红的，但它们不是同一件事：一张说 git 跑了并拒绝了这次推送，并给出
下一条 git 命令；另一张说什么都没跑，并指名是哪条 `viden.toml` 规则挡下的。把两者画成
一样的客户端，会因为操作者自己分支上的问题把他们指向权限文件。

有两处「缺席」是刻意的，而且看得见。没有任何一张图提供 `Pull`：契约在 `0.3.3` 排除了
它，因为它会移动 `HEAD` 并可能制造属于 Lane 冲突机制的冲突，两种语言的同步提示都把这
一点写了出来。而 `review-push-no-upstream` 提供的是「推送并设置 upstream」而不是原样
重试，因为 Core 会直接拒绝一次不带跟踪的推送 —— 一旦 `git push <remote> <branch>`
成功，领先/落后就无从得知，同步芯片会永远显示「已同步」。

light/`zh-CN` 那张是动作文案的语言与皮肤佐证：完成句子、工作区状态、`Git 输出` 折叠标题
与三个按钮标签都翻译，而分支名、重新采样的计数以及 git 自己的输出与 Core 发布的完全一致。
## D12 结构化冲突内容

2026-09-09 以同一套 headless Chrome 流程
（`--headless --window-size=1440,900 --virtual-time-budget=6000`）针对 4173 端口
的 vite 开发服务器采集，并逐张目视复核。这是客户端中 `runtime.conflict_content`
（GUI-CORE-015）的首批图像。

| 文件 | 状态 | 视口 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [d12-conflict-content-1440x900-dark-en.png](d12-conflict-content-1440x900-dark-en.png) | d12-conflict-content | 1440x900 | dark | en |
| [d12-conflict-omitted-1440x900-dark-en.png](d12-conflict-omitted-1440x900-dark-en.png) | d12-conflict-omitted | 1440x900 | dark | en |
| [d12-conflict-none-1440x900-dark-en.png](d12-conflict-none-1440x900-dark-en.png) | d12-conflict-none | 1440x900 | dark | en |
| [d12-conflict-content-1440x900-light-zh-CN.png](d12-conflict-content-1440x900-light-zh-CN.png) | d12-conflict-content | 1440x900 | light | zh-CN |

每张图都用于让一条诚实性规则可被证伪：

| 图像 | 它证明的规则 |
| --- | --- |
| `d12-conflict-content` | 面板画的是**两侧加上补丁原像，绝不是合并结果**：OURS 与 THEIRS 按 Core 自己的 `ours_start` / `theirs_start` 并排，原像作为单独标注的第三侧，声明写在行的上方，且整屏没有合并后文本、没有解决控件。两个 hunk 各带自己的原因 chip 与补救说明，因此读者能看出归类是按 hunk 而不是按文件。基线是该闸已评审的证据及其绑定 chip，这正是合并路径的答案 —— 而不是一个裸 commit |
| `d12-conflict-omitted` | `omitted` 与 `truncated` 保持可见：横幅位于文件列表之上，`assets/atlas.png` 保留条目并说明其 hunk 未展示。评审者必须始终能区分「未展示」与「该文件没问题」；同一张图还带 `revision` 基线，因此两种基线类型都在这组图里出现 |
| `d12-conflict-none` | 两种缺失是两句不同的话。这里 capability **是**通告的，只是该记录不带内容，因此面板说明 Core 未为该 bounce 发布内容并点明操作者退回的契约 —— 它不点名 `runtime.conflict_content`，那是 capability 本身缺失时该屏「不可用」行要说的 |

light/`zh-CN` 那张是新增文案的语言佐证：段落标题、非合并声明、基线行、原因 chip 及其
补救说明、OURS/THEIRS/BASE 标签都翻译；文件路径、冲突源码行、证据 id、源哈希，以及
Core 自己的 Lane 与闸 id 完全不变。翻译源码行会破坏冲突面板存在的唯一意义。

原因词表来自 Core 的 `ConflictHunkReason`，按精确判别式渲染，因此本构建未命名的原因
会原样到达屏幕，而不会借用一个已知原因：

| `ConflictHunkReason` | 标签 | 操作者可以做什么 |
| --- | --- | --- |
| `context_mismatch` | 上下文不匹配 | 原 Lane 必须基于当前内容重新生成补丁；在这里无法应用任何内容 |
| `already_applied` | 已经应用 | 文件在该区间已经是该 hunk 的新侧，没有可应用的内容 |
| `file_missing` | 文件缺失 | 补丁修改的文件不在目标树中 —— 这是文件级决定 |
| `file_deleted` | 删除后文件仍会残留 | 删除的原像未覆盖整个文件，文件会被留下 —— 这是文件级决定 |
| `binary` | 二进制 | Core 报告为非文本内容，没有可匹配的行 |
| 其他 | `原因 <tag>` | 按 Core 发布的原样展示 |

## EvidenceView 证据档案截图

2026-09-10 以同一套无头 Chrome 流程
（`--headless --window-size=1440,900 --virtual-time-budget=6000`）对着 4173 端口上的
vite 开发服务器采集，随后逐张目视复核。这是 `runtime.evidence_reads`（GUI-CORE-025）
在客户端中的首批图像。

行与内容答案是 `qa.ts` 中的内联 fixture，而不是生成的 projection：该能力的投影是查询答案
而不是运行时事实，因此 `RuntimeProjection` 从不持有它，`tests/capture_projections.rs`
也没有可为其发出的东西。这些行按 Core 自身的 `(timestamp, id)` 升序书写、无日期行在最前，
也就是 Core 本来会交付它们的顺序；客户端不做任何排序。

| 文件 | 状态 | 视口 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [evidence-1440x900-dark-en.png](evidence-1440x900-dark-en.png) | evidence | 1440x900 | dark | en |
| [evidence-text-1440x900-dark-en.png](evidence-text-1440x900-dark-en.png) | evidence-text | 1440x900 | dark | en |
| [evidence-summary-only-1440x900-dark-en.png](evidence-summary-only-1440x900-dark-en.png) | evidence-summary-only | 1440x900 | dark | en |
| [evidence-unavailable-1440x900-dark-en.png](evidence-unavailable-1440x900-dark-en.png) | evidence-unavailable | 1440x900 | dark | en |
| [evidence-empty-1440x900-dark-en.png](evidence-empty-1440x900-dark-en.png) | evidence-empty | 1440x900 | dark | en |
| [evidence-rejected-1440x900-dark-en.png](evidence-rejected-1440x900-dark-en.png) | evidence-rejected | 1440x900 | dark | en |
| [evidence-1440x900-light-zh-CN.png](evidence-1440x900-light-zh-CN.png) | evidence | 1440x900 | light | zh-CN |

每张图都为了让某一条诚实规则可被证伪：

| 图像 | 它证明的规则 |
| --- | --- |
| `evidence` | 这是**归档**，不是 `latest_evidence`。`无日期` 组领先，因为 Core 的排序就是把它放在那里；两个日期组按升序跟随；`task_summary` chip 排在五个一等类型之后而不是被丢弃——隐藏一种类型的 chip 条就是在隐藏归档里确实存在的行。因为 Core 的这一页并不完整，所以提供「加载更早」；搜索框自身的标签写明它只过滤已加载的行，因为 Core 不发布证据搜索 |
| `evidence-text` | 上限被明说，而不是被暗示：截断句写清是 Core 的 256 KiB 上限截断了内容，且这**不**表示证据本身很短；`已按 <sha256> 校验` 一行让读者把屏幕上的内容与该行的 canonical 引用对上。非 `patch` 行上的「在评审中打开」是可见且禁用，而不是隐藏 |
| `evidence-summary-only` | 「没有 canonical 引用」本身就是一条事实。报告不含 canonical 条目、捆绑包、哈希与生产者，并说明该条目未指向规范字节；内容区把 `Unavailable { SummaryOnly }` 渲染成一句话而不是空正文 |
| `evidence-unavailable` | `HashMismatch` 与 `SummaryOnly` 是**相反**的事实，这一对截图证明二者没有被折叠成一个。此处 Core 持有字节却拒绝提供，于是面板说校验失败并且完全不显示正文——评审者绝不该被展示的，恰恰是未通过自身哈希的内容 |
| `evidence-empty` | 四种缺席保持四句话。这是唯一可以画成「此范围内没有证据」的一种，而且画在 Core 确实答复过的一页之上；`complete` 用文字明说，而不是留给一个缺席的按钮 |
| `evidence-rejected` | 拒绝绝不是空页。Core 自己的理由原样放进 `role=alert`，其 `hint:` 行保持独立成行，列表保持未加载——没有行、没有翻页脚，也没有空档案的句子 |

light/`zh-CN` 截图是新增文案的语言证明。类型 chip、`无日期` 标签、报告字段名、元数据说明、
内容句子与页脚两个动作都会翻译；证据 id、Core 发布的摘要、路径、来源哈希、生产者、
Core 没有本地化名称的原始 `task_summary` 类型，以及 `YYYY-MM-DD` 日期键，都保持 Core 发布的原样。

本族有一条特有的确定性说明：按天分组与行时间是**本地**的，这是设计使然——操作者是在自己所处的
那一天读档案，这与审计记录不同，后者固定为 UTC，因为它要跨机器比对。这些截图冻结了 `Date.now`
但没有冻结时区，因此图中的日期标题与时间是采集主机所在时区的结果。fixture 的两个时间点是
`2023-11-14 22:15:00 UTC` 以及其后 24 小时；在另一个时区采集会为同样的行显示不同的日期键，
那是分组规则在正常工作，而不是漂移。

不可用词表来自 Core 的 `EvidenceUnavailableReason`，按判别式分支，因此本构建未建模的原因
会原样到达屏幕，而不会借用一个已知原因：

| `EvidenceUnavailableReason` | 详情侧栏说什么 |
| --- | --- |
| `summary_only` | 仅供展示的证据——Core 未持有其规范字节 |
| `missing_canonical_bytes` | Core 指向的规范字节已不在存储中 |
| `hash_mismatch` | 规范字节校验失败——不予展示 |
| `binary` | 规范字节校验通过但不是文本，因此没有可展示的正文 |
| 其他 | Core 给出了本版本未建模的原因（`<reason>`） |

### 2026-09-10 重新采集（E1）

上述七张图在 2026-09-10 以完全相同的流程重新采集（`--headless --disable-gpu
--hide-scrollbars --window-size=1440,900 --virtual-time-budget=6000`，对着
4173 端口上的 vite 开发服务器），并再次逐张目视复核。原因：F1 批次在 `viden-core`
门面上重新导出了 `EvidenceVerificationState` 与 `EvidenceQualityStatus`，详情侧栏
的报告因此新增两行，于是 F1 之前采集的截图比它们所记录的界面落后一个修订。

七张中四张变化、三张未变，这件事本身值得记录：

| 文件 | 是否变化 | 原因 |
| --- | --- | --- |
| `evidence-1440x900-dark-en.png` | 是 | 选中的 `patch` 行同时带有两个判定，报告中因此出现 `verification` 与 `canonical quality` 两行 |
| `evidence-text-1440x900-dark-en.png` | 是 | 同上，发生在选中的 `test_result` 行 |
| `evidence-unavailable-1440x900-dark-en.png` | 是 | 与 `evidence` 是同一行，只是内容答案不同 |
| `evidence-1440x900-light-zh-CN.png` | 是 | 这两行标签会翻译，因此语言证明必须跟着更新 |
| `evidence-summary-only-1440x900-dark-en.png` | 否 | `task_summary` 行两个判定都没有，因此没有新增行，报告逐字节未变 |
| `evidence-empty-1440x900-dark-en.png` | 否 | 没有选中任何行，也就没有报告可加 |
| `evidence-rejected-1440x900-dark-en.png` | 否 | 查询被拒绝，什么都没加载，也没有选中项 |

`evidence-unavailable` 这张图让一件事变得可见，但界面上还没有用文字解释：报告可以显示
`校验状态 已校验` / `规范字节质量 通过`，而下方内容区却写着"规范字节校验失败——不予展示"。
这是两个不同的事实——Core 对**证据记录**记下的判定，与**内容读取**自身对 `source_hash`
的哈希校验——而屏幕目前没有说明两者的关系。已作为后续项记入
`docs/core-0.3-compatibility.md`，本批次不做修复。

## 导航外壳截图（G3）

2026-09-12 以同一套 headless Chrome 流程拍摄
（`--headless --window-size=1440,900 --virtual-time-budget=6000`），针对 4173
端口上的 vite dev server，随后逐张目视复核。这是 rail-as-router 外壳的第一批图像：
`D-RAILNAV` 下五个次级 D 屏渲染在驾驶舱内部、`D-SIDEBAR` 的浮动模式，以及
`D-STATUSBAR` 的配置齿轮。

D2 与 D14 的投影就是独立屏截图已经用过的那份**生成**投影
（`../gui-screen-restore/projections/d2.json` 与 `d14-audit.json` / `d14-raw.json`），
经驾驶舱的 `secondaryViews` 接缝交给同一批生产渲染函数。这些截图里屏幕本身没有任何改动
——只有承载它们的容器变了——而这正是这些图像要展示的性质。

| 文件 | 状态 | 视口 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [nav-d2-in-cockpit-1440x900-dark-en.png](nav-d2-in-cockpit-1440x900-dark-en.png) | nav-d2-in-cockpit | 1440x900 | dark | en |
| [nav-d2-in-cockpit-1440x900-light-zh-CN.png](nav-d2-in-cockpit-1440x900-light-zh-CN.png) | nav-d2-in-cockpit | 1440x900 | light | zh-CN |
| [nav-d14-in-cockpit-1440x900-dark-en.png](nav-d14-in-cockpit-1440x900-dark-en.png) | nav-d14-in-cockpit | 1440x900 | dark | en |
| [nav-sidebar-floating-peek-1440x900-dark-en.png](nav-sidebar-floating-peek-1440x900-dark-en.png) | nav-sidebar-floating-peek | 1440x900 | dark | en |
| [nav-statusbar-config-1440x900-dark-en.png](nav-statusbar-config-1440x900-dark-en.png) | nav-statusbar-config | 1440x900 | dark | en |

每张图都让一个主张可被证伪：

| 图像 | 它证明的主张 |
| --- | --- |
| `nav-d2-in-cockpit` | chrome 在切换中**存活**。G3 之前这块屏幕会替换整个窗口；这里标题栏、活动 rail、上下文坞、composer 与状态栏都仍在它旁边，composer 也仍然指名选中的 Lane。Rail 把 `决策` 标记为当前并带 `2`——来自 Core 自己的 `pendingGateCount`，是 rail 唯一被允许展示的已发布数字 |
| `nav-d2-in-cockpit`（light/zh-CN） | 新增文案的语言证明：视图头部、Close 控件的可访问名称与 rail 新槽位名都会翻译，而 Core 的 id、原始动作键与能力名保持 Core 发布时的原样 |
| `nav-d14-in-cockpit` | 返回路径有地方可回。`D-AUDIT` 的链接是单向的——审计行链接证据，不反向——因此从证据行或 D12 芯片跟过去的操作者过去会丢掉对话；这里轨迹渲染在对话之上，`Esc` 或 Close 控件把转录带回来 |
| `nav-sidebar-floating-peek` | `D-SIDEBAR` 的浮动模式是浮层，不是第二套布局。Peek 打开的侧栏之后转录保持完整宽度（网格仍是三条轨道），12px 热区连同 `.edgehint` 贴在活动 rail 右缘，齿轮之上的 rail pin 读作未固定——同一个组件，不同的宿主。请与 `lane-rail` 对读，那是同一决策的 pinned 另一半 |
| `nav-statusbar-config` | `D-STATUSBAR` 按**可操作性**而不是紧急程度切分状态栏。弹层恰好提供六个环境段；`MODE`、`PERM`、`LANE` 与待审闸芯片就在其后的状态栏上，且不在列表中，因为可以被关掉的控件就是操作者在需要时够不到的控件（`O-B6`） |

G3 之前拍摄的带驾驶舱截图已在同一批次内重拍，见下文重拍小节。

## 导航外壳之后的驾驶舱重拍（2026-09-12）

本目录中每一张带驾驶舱的图像都在 **2026-09-12** 经 qa 采集脚本
（`evidence/main-window-interactions/qa.ts`）针对 4173 端口上的 vite dev server
重拍，流程与既有的 headless Chrome 一致
（`--headless --window-size=1440,900 --virtual-time-budget=6000`，等待
`data-capture-ready`），且每一张在提交前都被打开逐一看过。

原因不是外观微调。G3 改动了两块出现在**每一帧**里的 chrome：

- **活动 rail** 成为 `D-RAILNAV` 的路由器。它在既有的 Review/Evidence 一对与
  spacer 之间新增了五个目的地槽位（`决策`、`Lane 监视`、`集成闸`、`舰队看板`、
  `审计时间线`），并在设置齿轮正上方新增了 Lane 侧栏的 **pin**；
- **状态栏**在起始边新增了 `D-STATUSBAR` 的配置齿轮。

因此本批次之前拍摄的每一张图，展示的都是一个已经不存在的 rail 与状态栏。42 个文件
全部发生变化，没有一张字节相同。它们背后的生成投影没有改动——改的只是外面的外壳
——这正是它属于重拍而不是新主张的原因。

`evidence/0.1.0-rc.3/` 是历史发布包，刻意未动。

| 文件 | 重拍日期 | 图中变了什么 |
| --- | --- | --- |
| `approval-hunks-1440x900-dark-en.png` | 2026-09-12 | rail 槽位 + pin，状态栏齿轮 |
| `d1-1440x900-dark-en.png` | 2026-09-12 | rail 槽位 + pin，状态栏齿轮；这是新 rail 顺序的基准帧 |
| `d1-1440x900-light-zh-CN.png` | 2026-09-12 | 同上，浅色皮肤与 zh-CN，新槽位名与 pin 的标题在此翻译 |
| `d1-mode-menu-1440x900-dark-en.png` | 2026-09-12 | rail 槽位 + pin，状态栏齿轮；模式菜单本身未变 |
| `d1-model-menu-1440x900-dark-en.png` | 2026-09-12 | rail 槽位 + pin，状态栏齿轮；模型菜单本身未变 |
| `d6-actions-1440x900-dark-en.png` | 2026-09-12 | rail 槽位 + pin，状态栏齿轮；agent 停止界面未变 |
| `d6-error-1440x900-dark-en.png` | 2026-09-12 | 同上，Core 的重启拒绝仍在动作下方 |
| `evidence-1440x900-dark-en.png` | 2026-09-12 | rail 槽位 + pin，状态栏齿轮；`Evidence` 现在是路由目的地并被标记为当前 |
| `evidence-1440x900-light-zh-CN.png` | 2026-09-12 | 同上，light/zh-CN |
| `evidence-empty-1440x900-dark-en.png` | 2026-09-12 | 同上；空档案的回答未变 |
| `evidence-rejected-1440x900-dark-en.png` | 2026-09-12 | 同上；Core 的拒绝文案未变 |
| `evidence-summary-only-1440x900-dark-en.png` | 2026-09-12 | 同上；仅展示型报告未变 |
| `evidence-text-1440x900-dark-en.png` | 2026-09-12 | 同上；内容被截断的提示未变 |
| `evidence-unavailable-1440x900-dark-en.png` | 2026-09-12 | 同上；校验失败的内容回答未变 |
| `lane-rail-1440x900-dark-en.png` | 2026-09-12 | **是行为改变，不只是 chrome**：该状态现在以 `laneSidebarMode: "pinned"` 挂载，Lane 侧栏成为真正的第四条网格轨道（`52px 218px …`）并把工作区向右推开，而不再是过去的浮层。齿轮之上的 rail pin 读作已按下 |
| `nav-d14-in-cockpit-1440x900-dark-en.png` | 2026-09-12 | 在 D-SIDEBAR 修正之后重拍，使 rail 与本批次其余图像一致 |
| `nav-d2-in-cockpit-1440x900-dark-en.png` | 2026-09-12 | 同上 |
| `nav-d2-in-cockpit-1440x900-light-zh-CN.png` | 2026-09-12 | 同上 |
| `nav-sidebar-floating-peek-1440x900-dark-en.png` | 2026-09-12 | **按修正后的默认值重拍**：该状态不再传入任何模式，因此它证明的是"调用方不表态时驾驶舱就是 `floating`" |
| `nav-statusbar-config-1440x900-dark-en.png` | 2026-09-12 | **替换了一个损坏的文件**——见下方说明 |
| `palette-1440x900-dark-en.png` | 2026-09-12 | 遮罩之后的 rail 槽位 + pin 与状态栏齿轮；命令面板自身的动作列表本就列出 D2/D4/D10/D11/D12/D13/D14，未变 |
| `palette-files-1440x900-dark-en.png` | 2026-09-12 | 同上 |
| `palette-files-1440x900-light-zh-CN.png` | 2026-09-12 | 同上，light/zh-CN |
| `permission-ask-1440x900-dark-en.png` | 2026-09-12 | rail 槽位 + pin，状态栏齿轮；权限坞未变 |
| `permission-deny-redirect-1440x900-dark-en.png` | 2026-09-12 | 同上；改写后的 composer 占位文案未变 |
| `permission-deny-redirect-1440x900-light-zh-CN.png` | 2026-09-12 | 同上，light/zh-CN |
| `project-picker-1440x900-dark-en.png` | 2026-09-12 | rail 槽位 + pin，状态栏齿轮；项目选择弹层未变 |
| `project-switch-confirm-1440x900-dark-en.png` | 2026-09-12 | 同上；切换确认未变 |
| `review-1440x900-dark-en.png` | 2026-09-12 | rail 槽位 + pin，状态栏齿轮；`Review` 现在是路由目的地并被标记为当前 |
| `review-1440x900-light-zh-CN.png` | 2026-09-12 | 同上，light/zh-CN |
| `review-commit-1440x900-dark-en.png` | 2026-09-12 | 同上；可用的"提交 / 提交并推送"一对未变 |
| `review-commit-completed-1440x900-dark-en.png` | 2026-09-12 | 同上；完成行与 Git 输出折叠未变 |
| `review-commit-completed-1440x900-light-zh-CN.png` | 2026-09-12 | 同上，light/zh-CN |
| `review-commit-pending-approval-1440x900-dark-en.png` | 2026-09-12 | 同上；等待批准行未变 |
| `review-empty-1440x900-dark-en.png` | 2026-09-12 | 同上；干净工作区的回答未变 |
| `review-omitted-1440x900-dark-en.png` | 2026-09-12 | 同上；超出字节上限的行提示未变 |
| `review-push-no-upstream-1440x900-dark-en.png` | 2026-09-12 | 同上；无 upstream 的解释与其唯一恢复动作未变 |
| `review-rejected-1440x900-dark-en.png` | 2026-09-12 | 同上；Core 对 `git_diff` 的拒绝未变 |
| `review-rejected-action-1440x900-dark-en.png` | 2026-09-12 | 同上；Core 对 `git_add` 的拒绝未变 |
| `settings-1440x900-dark-en.png` | 2026-09-12 | 对话框之后的 rail 槽位 + pin 与状态栏齿轮；设置对话框未变 |
| `settings-1440x900-light-zh-CN.png` | 2026-09-12 | 同上，light/zh-CN |
| `settings-unavailable-1440x900-dark-en.png` | 2026-09-12 | 同上；`ui.preference_persistence` 提示未变 |

## 驾驶舱中央截图（G4）

2026-09-12 以同一套 headless Chrome 流程拍摄
（`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`），针对 **4175** 端口上的 vite dev server（4173 属于
另一个 worktree），随后逐张打开阅读。这是驾驶舱中央与设计对齐后的第一批图像：
`.tabstrip.lanebar` Lane 标签条、`.tool` 转录块、专注模式，以及从「新建 Lane」弹层
进入的 D4 向导。

支撑它们的是两份新投影，都是共享 D1 fixture 的 delta，并在 `qa.ts` 中与其镜像的
fixture 并列注明：

- `d1TwoLanes()` 增加一条带**自己**记录分支的 Lane 以及绑定其上的 ACP 会话，于是
  标签条能同时展示内置 Lane 与 agent 绑定的 Lane。没有第二条 Lane，标签条什么也证明不了。
- `d1ToolBlocks()` 用一条工作区改动（携带与评审状态相同的单文件单 hunk 页
  `REVIEW_PAGE.entries[0].diff`）与一次失败检查运行（携带 canonical
  `d1-main-cockpit.json` 记录的 `file:line` 位置）替换清单。

| 文件 | 状态 | 视口 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [centre-lane-tabs-1440x900-dark-en.png](centre-lane-tabs-1440x900-dark-en.png) | centre-lane-tabs | 1440x900 | dark | en |
| [centre-lane-tabs-1440x900-light-zh-CN.png](centre-lane-tabs-1440x900-light-zh-CN.png) | centre-lane-tabs | 1440x900 | light | zh-CN |
| [centre-tool-diff-1440x900-dark-en.png](centre-tool-diff-1440x900-dark-en.png) | centre-tool-diff | 1440x900 | dark | en |
| [centre-focus-mode-1440x900-dark-en.png](centre-focus-mode-1440x900-dark-en.png) | centre-focus-mode | 1440x900 | dark | en |
| [d4-from-popover-1440x900-dark-en.png](d4-from-popover-1440x900-dark-en.png) | d4-from-popover | 1440x900 | dark | en |

每张图都让一个主张可被证伪：

| 图像 | 它证明的主张 |
| --- | --- |
| `centre-lane-tabs` | 标签条**按 Lane、按 Core 事实**渲染。两个标签，选中的那个带设计稿的强调线，各自携带该 Lane 自己记录的分支——`codex/lane-core` 与 `vd/retry-policy`，两者不同，因此都不可能是工作区的——以及 Core 为其绑定的 agent（内置 Lane 为 `Viden Agent`，ACP Lane 为 `codex-acp`）。末尾的 `.tabmeta` 只带**一次**项目、`42.1k`（Core 发布的预算）与 `Build`（Core 解析的模式），而不是每个标签一份，因为投影按 Lane 限定 |
| `centre-lane-tabs`（light/zh-CN） | 新增文案的语言证明：标签条的可访问名称、创建入口的标签与模式词都会翻译，而 Lane id、分支名与 agent id 保持 Core 发布时的原样 |
| `centre-tool-diff` | 转录内联承载证据。改动块展示设计稿的头部——展开箭头、Core 发布的改动类别、路径、`+1 −1`——之上是共享 hunk 渲染器，带 Git 自己的 `@@` 头与逐侧行号；检查块展示命令、`Failed` 芯片与三条 `.testrow`，含 Core 报告的失败位置。此处特意展开了 diff 块：折叠才是它的默认值，而截图必须展示展开后的内容 |
| `centre-focus-mode` | `D-SIDEBAR` 的覆盖条款确实是**两侧强制 hover 浮窗**。主体只剩两条轨道——活动 rail 与转录——该状态起始时的 pinned Lane 列已让出，上下文坞离开网格、退到右缘热区之后，标题栏的 `IFocus` 控件读作已按下。请与 `lane-rail` 对读，那是同一驾驶舱保留该列的样子 |
| `d4-from-popover` | 「完整设置…」保留 draft 并说明差距。Lane 依弹层携带的任务命名为 `refactor-the-config-loader`，agent 步骤列出 Core 的适配器并把 `Codex` 标为弹层中的选择，该步还用自己的话说明 `StarterLaneRequest` 不携带 agent 绑定——因此向导绝不暗示它会启动会话 |

本目录中每一张带驾驶舱的截图都在同一批次内重拍，见下文重拍小节。

### 中央批次之后的驾驶舱重拍（2026-09-12，G4）

Lane 标签条出现在每一帧带转录的画面里，`.tool` 块出现在每一帧带清单的画面里，因此
G3 重拍过的那 **42** 张带驾驶舱图像再次全部重拍，另加 `d4-1440x900-dark-en.png`，
因为向导的步骤发生了变化——共 43 个文件。全部针对同一台运行中的服务器拍摄，且每一张
在提交前都被打开阅读过。

有一处布局修复随之提交，值得点名，因为它是被截图而不是被测试发现的。`gui-kit.css`
的 `.frame` 是 flex 列，与 `.d1-frame` 的 `display: grid` 特异性相同却位于层叠更后
的位置，因此外壳一直是按 flex 布局的；主体之所以能抵到状态栏，只是因为上下文坞的自然
高度恰好够到。专注模式把坞移出文档流，于是第一张 `centre-focus-mode` 截图显示外壳在
状态栏上方 170px 处就停住、后面露出裸页面。`.d1-body` 现在显式声明占据剩余空间
（`flex: 1 1 auto`），这正是框架行高一直想表达的意思，并且在每一个非专注状态下测得的
高度完全一致。

重拍的 43 个文件，列在一处以便核对：

- `approval-hunks-1440x900-dark-en.png`
- `d1-1440x900-dark-en.png`
- `d1-1440x900-light-zh-CN.png`
- `d1-mode-menu-1440x900-dark-en.png`
- `d1-model-menu-1440x900-dark-en.png`
- `d4-1440x900-dark-en.png`
- `d6-actions-1440x900-dark-en.png`
- `d6-error-1440x900-dark-en.png`
- `evidence-1440x900-dark-en.png`
- `evidence-1440x900-light-zh-CN.png`
- `evidence-empty-1440x900-dark-en.png`
- `evidence-rejected-1440x900-dark-en.png`
- `evidence-summary-only-1440x900-dark-en.png`
- `evidence-text-1440x900-dark-en.png`
- `evidence-unavailable-1440x900-dark-en.png`
- `lane-rail-1440x900-dark-en.png`
- `nav-d14-in-cockpit-1440x900-dark-en.png`
- `nav-d2-in-cockpit-1440x900-dark-en.png`
- `nav-d2-in-cockpit-1440x900-light-zh-CN.png`
- `nav-sidebar-floating-peek-1440x900-dark-en.png`
- `nav-statusbar-config-1440x900-dark-en.png`
- `palette-1440x900-dark-en.png`
- `palette-files-1440x900-dark-en.png`
- `palette-files-1440x900-light-zh-CN.png`
- `permission-ask-1440x900-dark-en.png`
- `permission-deny-redirect-1440x900-dark-en.png`
- `permission-deny-redirect-1440x900-light-zh-CN.png`
- `project-picker-1440x900-dark-en.png`
- `project-switch-confirm-1440x900-dark-en.png`
- `review-1440x900-dark-en.png`
- `review-1440x900-light-zh-CN.png`
- `review-commit-1440x900-dark-en.png`
- `review-commit-completed-1440x900-dark-en.png`
- `review-commit-completed-1440x900-light-zh-CN.png`
- `review-commit-pending-approval-1440x900-dark-en.png`
- `review-empty-1440x900-dark-en.png`
- `review-omitted-1440x900-dark-en.png`
- `review-push-no-upstream-1440x900-dark-en.png`
- `review-rejected-1440x900-dark-en.png`
- `review-rejected-action-1440x900-dark-en.png`
- `settings-1440x900-dark-en.png`
- `settings-1440x900-light-zh-CN.png`
- `settings-unavailable-1440x900-dark-en.png`

五个新文件列在上表中。

## 上下文坞截图（G5）

2026-09-12 以同一套 headless Chrome 流程拍摄
（`--headless --disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`），针对 **4176** 端口上的 vite dev server（4173 与
4175 属于其他 worktree），随后逐张打开阅读。这是上下文坞与设计对齐后的第一批图像：
六标签 `.docktabs` 条、环境面板的 `.envsec` 节、文件树，以及带共享 hunk 行的对比面板。

支撑它们的是两份 fixture，都在 `qa.ts` 中与其镜像的对象并列：

- `PALETTE_FILES_APPS` 是针对 `apps/` prefix 的第二份 `QueryWorkspaceFiles` page，
  只有展开该目录时才会作答。没有第二份 page，文件标签就可能只是对第一份的客户端过滤，
  而那正是客户端边界所禁止的。
- 对比相关状态复用 `REVIEW_PAGE`——评审截图已经在用的那一页——因为坞与 DiffReview
  确实按 target 渲染同一页。

| 文件 | 状态 | 视口 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [dock-environment-1440x900-dark-en.png](dock-environment-1440x900-dark-en.png) | dock-environment | 1440x900 | dark | en |
| [dock-environment-1440x900-light-zh-CN.png](dock-environment-1440x900-light-zh-CN.png) | dock-environment | 1440x900 | light | zh-CN |
| [dock-files-1440x900-dark-en.png](dock-files-1440x900-dark-en.png) | dock-files | 1440x900 | dark | en |
| [dock-diff-1440x900-dark-en.png](dock-diff-1440x900-dark-en.png) | dock-diff | 1440x900 | dark | en |
| [dock-unavailable-1440x900-dark-en.png](dock-unavailable-1440x900-dark-en.png) | dock-unavailable | 1440x900 | dark | en |

每张图都让一个主张可被证伪：

| 图像 | 它证明的主张 |
| --- | --- |
| `dock-environment` | 坞**已分标签，并且对全部六个标签都如实说明**。环境 / 文件 / 对比可用；终端 / 源码 / 文档带删除线，原因写在 title 里。其下变更节在两条真实文件行（含逐文件计数）之上显示 Core 的 `+3 -1`，随后是 Core 的字节上限说明、点名当前显示工作区根的本地节、提交或推送路由、PR 状态缺失，以及 `42.1k / 128k` 的 `.envctx` 条带「已用 33」与 Core 发布的花费 |
| `dock-environment`（light/zh-CN） | 本批次新增的每一条文案的语言证明，标签名也在内：`环境 / 文件 / 终端 / 源码 / 对比 / 文档`、`变更`、`本地`、`提交或推送`、`PR 状态`、`上下文`。路径、分支名与 token 计数保持 Core 发布时的原样 |
| `dock-files` | 文件树是**每个目录一份 Core page**。`apps/` 已展开并显示 `cli`、`gui`、`tui`——这些行之所以存在，只因为请求了该 prefix——而 Core 的 `complete: false` 渲染为「该页在这棵树结束之前就截止了」。选中的文件填充检视面板，其 Open 处于禁用并把 `runtime.workspace_file_reads` 写在屏幕上，而不是藏在 tooltip 里 |
| `dock-diff` | 该面板**先是一份文件列表**。两个条目，第二个仍折叠并显示真实的 `+1284 -0`，第一个展开在共享 `diff_rows` 主体之上（带 Git 自己的 `@@` 头与逐侧行号），检视面板的文件 / 差异 / 「在评审中打开」旁是禁用的暂存与还原——因为 Core 未为二者发布任何逐文件命令 |
| `dock-unavailable` | 四种缺失，四句话。变更节说明未绑定 host；提交或推送禁用**并**在屏幕上说明原因；PR 状态保留自己的缺失句；本地节仍然渲染，因为工作区 source 是 Core 确实发布过的事实 |

有一个状态故意读起来别扭，值得点名：`review-empty` 的坞在「没有变更」之上显示
`行数 +3 -1`。两者都来自 Core，只是出自两次不同的读取——工作区 source 采样与结构化
diff page——该节分别标注它们，而不去调和。客户端若为了与 page 一致而悄悄把计数清零，
那就是在编造 Core 从未发布过的一致性。

### 上下文坞之后的驾驶舱重拍（2026-09-12，G5）

坞出现在每一帧带驾驶舱的画面里，因此全部 **46** 张都针对同一台运行中的服务器重拍，
且每一张在提交前都被打开阅读过。独立 D 屏家族（`d2-*`、`d4-*`、`d10-*`、
`d11-*`、`d12-*`、`d13-*`、`d14-*`）是不带坞的全窗口渲染器，本批次不可能影响它们，
故未改动。每个输出都按文件大小筛查过 Chrome 错误页的特征（约 28 KB，而真实帧为
110-210 KB）——没有可疑对象。

重拍的 46 个文件，列在一处以便核对：

- `approval-hunks-1440x900-dark-en.png`
- `centre-focus-mode-1440x900-dark-en.png`
- `centre-lane-tabs-1440x900-dark-en.png`
- `centre-lane-tabs-1440x900-light-zh-CN.png`
- `centre-tool-diff-1440x900-dark-en.png`
- `d1-1440x900-dark-en.png`
- `d1-1440x900-light-zh-CN.png`
- `d1-mode-menu-1440x900-dark-en.png`
- `d1-model-menu-1440x900-dark-en.png`
- `d6-actions-1440x900-dark-en.png`
- `d6-error-1440x900-dark-en.png`
- `evidence-1440x900-dark-en.png`
- `evidence-1440x900-light-zh-CN.png`
- `evidence-empty-1440x900-dark-en.png`
- `evidence-rejected-1440x900-dark-en.png`
- `evidence-summary-only-1440x900-dark-en.png`
- `evidence-text-1440x900-dark-en.png`
- `evidence-unavailable-1440x900-dark-en.png`
- `lane-rail-1440x900-dark-en.png`
- `nav-d14-in-cockpit-1440x900-dark-en.png`
- `nav-d2-in-cockpit-1440x900-dark-en.png`
- `nav-d2-in-cockpit-1440x900-light-zh-CN.png`
- `nav-sidebar-floating-peek-1440x900-dark-en.png`
- `nav-statusbar-config-1440x900-dark-en.png`
- `palette-1440x900-dark-en.png`
- `palette-files-1440x900-dark-en.png`
- `palette-files-1440x900-light-zh-CN.png`
- `permission-ask-1440x900-dark-en.png`
- `permission-deny-redirect-1440x900-dark-en.png`
- `permission-deny-redirect-1440x900-light-zh-CN.png`
- `project-picker-1440x900-dark-en.png`
- `project-switch-confirm-1440x900-dark-en.png`
- `review-1440x900-dark-en.png`
- `review-1440x900-light-zh-CN.png`
- `review-commit-1440x900-dark-en.png`
- `review-commit-completed-1440x900-dark-en.png`
- `review-commit-completed-1440x900-light-zh-CN.png`
- `review-commit-pending-approval-1440x900-dark-en.png`
- `review-empty-1440x900-dark-en.png`
- `review-omitted-1440x900-dark-en.png`
- `review-push-no-upstream-1440x900-dark-en.png`
- `review-rejected-1440x900-dark-en.png`
- `review-rejected-action-1440x900-dark-en.png`
- `settings-1440x900-dark-en.png`
- `settings-1440x900-light-zh-CN.png`
- `settings-unavailable-1440x900-dark-en.png`

五个新文件列在上方表格中。

### 关于 `nav-statusbar-config` 这个文件，直说

本批次的第一个提交里，`nav-statusbar-config-1440x900-dark-en.png` 是看过且正确的；
但随后工作区收到了同一路径的第二次、**损坏的**写入：一次重拍在 dev server 已被停掉
之后才启动，于是 Chrome 渲染了它自己的"This site can't be reached /
ERR_CONNECTION_REFUSED"页面，采集脚本把那个页面存成了证据。它落盘的时间晚于上一轮
最后一次工作区检查，因此从未被看到。本次提交中的文件是在 vite 运行状态下拍摄、并在
提交前读过的；它展示的是六个环境段之上的 `.sbcfg` 弹层，也就是上表所声称的内容。
2026-09-12 这一轮对重演做了防护：开跑之前先确认 dev server 处于运行状态，跑完之后
再按文件大小筛查每一个输出是否带有错误页的特征（约 28 KB，而真实帧是 100–200 KB）
——没有可疑项。该防护存在于一次性的采集脚本里，并不在仓库中；真正长期有效的保护仍然是
那条规则：报告里列出的每一张 PNG 都必须被打开看过。

本目录中的 27 张独立屏截图（`d2-*`、`d4-*`、`d10-*`、`d11-*`、`d12-*`、`d13-*`、
`d14-*`）**不**带驾驶舱——它们是没有 rail、没有状态栏的整窗渲染——G3 不可能改动它们，
因此未动。

## 二级视图截图（G6）

2026-09-12 使用 headless Chrome 拍摄
（`--disable-gpu --hide-scrollbars --window-size=1440,900
--virtual-time-budget=6000`），对应端口 4177 上的 vite dev server——4173、4175
与 4176 先用 `lsof` 检查过——随后逐张视觉复核（五张全部打开看过）。

五张中的两张（`d10-actions` 与 dark/en 的 `d14-filtered`）使用旧的
`--headless` 拍摄，其余三张使用 `--headless=new`：在本机上有第二个 Chrome 同时
采集时，旧模式开始出现无限挂起，而挂起的采集驱动正是让损坏文件在没人看的情况下
被提交的原因。其余参数完全一致、渲染的是同一个页面；此处记录模式，只是为了让方法
与实际执行一致。

| 文件 | 状态 | 视口 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [d10-actions-1440x900-dark-en.png](d10-actions-1440x900-dark-en.png) | d10-actions | 1440x900 | dark | en |
| [d13-drill-1440x900-dark-en.png](d13-drill-1440x900-dark-en.png) | d13-drill | 1440x900 | dark | en |
| [d14-filtered-1440x900-dark-en.png](d14-filtered-1440x900-dark-en.png) | d14-filtered | 1440x900 | dark | en |
| [d14-filtered-1440x900-light-zh-CN.png](d14-filtered-1440x900-light-zh-CN.png) | d14-filtered | 1440x900 | light | zh-CN |
| [d2-rail-badge-1440x900-dark-en.png](d2-rail-badge-1440x900-dark-en.png) | d2-rail-badge | 1440x900 | dark | en |

| 图 | 它证明的主张 |
| --- | --- |
| `d10-actions` | 动作行在**每张卡片上都是四个控件**，其中无法执行的两个会如实说明。`lane_core`（terminal 路由、无已发布会话）显示「接管」可用、「停止」变暗；`lane_review`（ACP 路由、一个已发布会话）两者都可用。两张卡片的「暂停」「终止」都变暗——它们仍在设计稿给出的位置上，并带着说明 `RuntimeCommand` 缺少哪个命令的句子。可与 `d10-blind` 对照，那是动作行存在之前的同两张卡片 |
| `d13-drill` | 两种 Lane 绑定答案出现在同一帧里。上面的节点带 `Open Lane lane_core ↗`，它是控件；下面那个——同一个生成节点去掉绑定后的副本——带着「Core 未为该任务绑定任何 Lane，没有可打开的对象。」，且完全没有可点击暗示。差异在画面里而不是在 hover 状态里，这正是可见提示的意义 |
| `d14-filtered` | 已加载页这一整套主张同时成立：执行者标签是 `全部执行者 / operator / agent`——没有 `system` 标签，因为本页没有这类行——四个时间标签停在 `全部时间` 上，筛选条下的说明点明切分范围是已加载页并注明 `GUI-CORE-024`，结果分布条读作 `已加载页（已筛选）的结果分布 · 1 of 3` 与 `denied 1`，筛选后恰好剩一行，`导出` 在筛选条末端变暗 |
| `d14-filtered`（light/zh-CN） | 新增文案的语言与皮肤验证：标签文字、已加载页说明、结果分布标题与导出控件都会翻译，而存留行里的每个 Core 值——点分键 `evidence.rejected`、执行者词 `agent`、agent id `codex-acp`、结果 `denied`、对象与参数芯片——都与 Core 发布时完全一致，时间戳在两种语言下都保持固定的 `UTC` 格式 |
| `d2-rail-badge` | 一个数字、一个来源、两处显示：侧栏 D2 槽位显示 `7`，状态栏显示 `⏸ 7 gate waiting`。`7` 不是共享 fixture 的 `2`，因此可以证明气泡读的确实是 `statusbar.pendingGateCount` 而不是常量。**同一帧里队列自身的 `2 awaiting you` 标题应读作已记录的差异，而不是 bug：** `D2DecisionsProjection.pendingTotal` 与 `pendingGateCount` 是两个不同的和（前者为审批加待处理复查，后者为审批加未休眠的开放合并闸），README 已写明这一点，其对齐属于拥有这些计数的批次。本张截图正是「二者今天并不是同一个数」的可见证据 |

本批次有两个事实没有截图、改由 vitest 覆盖，这里如实写出而不是含糊带过：

- **缺失的决策计数。** `pendingGateCount: null` 渲染为*没有*气泡、也没有状态栏
  段落，原因写在槽位的可访问名称里。一张「缺失」的截图与一张「零」的截图无法
  区分，因此该区别由
  [`../../tests/rail_decision_badge.spec.ts`](../../tests/rail_decision_badge.spec.ts)
  固定，而不是由 PNG。
- **已落定的停止结果。** harness 的宿主回调刻意永不 resolve，因此
  `d10-actions` 呈现的是命令发出之前的动作行。等待中的提示、Core 的接受与
  Core 的原文拒绝由
  [`../../tests/d10_lane_actions.spec.ts`](../../tests/d10_lane_actions.spec.ts)
  固定，command id 关联由
  [`../../tests/lane_handoff.spec.ts`](../../tests/lane_handoff.spec.ts) 固定。

`d14-filtered` 的行使用生成页自身 2023 年的时间戳，因此三个较窄的时间区间都会
把它筛空。于是截图呈现的是时间标签存在、`全部时间` 处于按下态，而不是启用某个
区间；没有为了画面更好看而编造更新的时间戳。区间规则本身由
[`../../tests/d14_audit_filters.spec.ts`](../../tests/d14_audit_filters.spec.ts)
中的 `filterAuditRows` 针对冻结时钟覆盖 `today`、`24h` 与 `7d`。

`d13-drill` 与 `d10-actions` 两个状态是带驾驶舱的：自 G3 起这两个屏都是中央面板
视图，截图中标题栏、活动侧栏、Lane 侧栏、上下文坞、输入区与状态栏在其四周保持
不变，视图自身的 Close 控件也在 DiffReview 放置自己那一个的位置上。

## H2 卫生批次截图

由 H2 批次（2026-09-12）追加，对应本批卫生修复新增的三个状态。每一张都通过 `qa.ts`
调用生产渲染函数拍摄，除行内另有说明外均为 `1440x900` 深色/英文，并且都在提交之前
被打开看过。

| 状态 | URL | 必须展示什么 |
| --- | --- | --- |
| `evidence-hash-mismatch` | `…/qa.html?state=evidence-hash-mismatch` | E1 缺陷 6。该 `patch` 行的归档记录写着 `verified`，而内容答案是 `Unavailable { HashMismatch }`。报告中的 `verification` 行保留「verified」这个当初记录下来的词，并带上不一致处理 —— 错误色加删除线，因此状态不只靠颜色表达 —— 其下那句话把两个事实联系起来：归档当初记录了什么，以及 Core 刚刚读出字节时得到了什么。下方的内容说明仍然写校验失败，且不展示任何正文。画面按报告区取景，使两者同处一帧。 |
| `welcome-fill` | `…/qa.html?state=welcome-fill` | E1 缺陷 8 与 9 合在一帧。未绑定项目的首次启动中间栏：活动栏、欢迎列与状态栏都抵达窗口底部，最近区域的提示读作「尚未打开项目 / 打开一个项目后，最近项目会出现在这里。」，宿主原本那句 `Error: Core adapter is not connected` 作为弱化的诊断保留在下方。 |
| `welcome-fill`（`1440x640`） | 同上 | E1 运行实测的第二个窗口高度下的同一布局；旧链路无论窗口多高都大约停在同一个 525 px。这里没有任何截断，状态栏仍在底部。 |
| `d10-events-not-read` | `…/qa.html?state=d10-events-not-read` | 兼容性待办 2。Lane 监视器作为座舱内视图 —— 完整外壳、带关闭控件 —— 事件流写着「The Core audit timeline has not been read yet.」。该状态不发出任何审计读取，这句话也不对 Core 是否提供时间线作任何声称；那种声称属于「能力缺失」状态，后者会点名 `runtime.audit`。 |

| 文件 | 状态 | 尺寸 | 模式 | 语言 |
| --- | --- | --- | --- | --- |
| [evidence-hash-mismatch-1440x900-dark-en.png](evidence-hash-mismatch-1440x900-dark-en.png) | evidence-hash-mismatch | 1440x900 | dark | en |
| [welcome-fill-1440x900-dark-en.png](welcome-fill-1440x900-dark-en.png) | welcome-fill | 1440x900 | dark | en |
| [welcome-fill-1440x640-dark-en.png](welcome-fill-1440x640-dark-en.png) | welcome-fill | 1440x640 | dark | en |
| [d10-events-not-read-1440x900-dark-en.png](d10-events-not-read-1440x900-dark-en.png) | d10-events-not-read | 1440x900 | dark | en |

H2 没有重拍任何既有截图。上文的 `evidence-unavailable` 与 `evidence-hash-mismatch`
是**同一个** Core 答案，刻意保留：那一张取景在内容说明，这一张取景在报告区，两张
合起来才说明两个事实现在是被联系起来的，而不只是同时存在。
