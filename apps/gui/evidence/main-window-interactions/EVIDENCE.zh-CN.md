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
| `d12-*` | [`../gui-screen-restore/projections/d12.json`](../gui-screen-restore/projections/d12.json) | 由 `tests/capture_projections.rs`**生成** |
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
| `d11` | 已探测的 `/workspace/demo` rust 项目，提供方处于凭据锁定状态 | `tests/d11_intake.spec.ts` 中的已探测项目 fixture |
| `d11`、`d11-recent` | 交给屏幕最近工作端口的同一份两项目 `RecentWorkResult` | `tests/d11_intake.spec.ts` 中的已加载行 fixture |
| `d14-audit-scoped` | 仅把 `scope` 设为 `revert:revert-1` 对象；行仍是生成页面本身，因为截图不得为过滤器编造记录 | `tests/d14_audit_trail.rs` 的 `a_scoped_query_passes_the_exact_object_through_and_reports_the_scope` |
| `d14-raw-fallback` | 清空 `capabilityAvailable`，行为空且 outcome 为 idle —— 表达「缺失」而非「为空」 | `tests/d14_audit_trail.rs` 的 `audit_mode_is_unavailable_and_sends_nothing_without_the_core_capability` |
| `palette` | 交给 `loadPaletteCrossLane` 的一条跨 Lane 合并闸与一条询问 | `tests/d12_integration_gate.spec.ts` 的闸 fixture，以及 D1 fixture 本就带的那条 `liveWork.approvals` |
| `palette`、`palette-files` | 交给 `loadPaletteFiles` 的六条真实 Viden 路径，按 Core 的字典序排列，字节大小固定 | `tests/command_palette.spec.ts` 的已加载清单 fixture，以及 `tests/workspace_files.rs` 断言的 page 形状 |
| `d10-ticker` | 一页四条、横跨两个交错项目的 audit 记录，经屏幕自己的 `applyEvents` 应用 | 规范 fixture `audit-ordering.json` 与 `crates/core/tests/frontend_contract_v1.rs` 中的 `audit_ordering_fixture_orders_two_projects_as_one_newest_first_timeline` |
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
| `palette` | `…/qa.html?state=palette` | 从标题栏按钮打开、覆盖在驾驶舱之上的 ⌘K 命令面板，四个分区全部可见——动作、跳转到（跨 Lane 的闸与询问，加上本 Lane）、设置，以及列出 Core 已发布工作区清单的「文件」分区 |
| `palette-files` | `…/qa.html?state=palette-files` | 同一个面板但预先限定到 `~`，单独框出 Core 发布的清单：六条路径按 Core 的字典序排列，每条带 Core 报告的条目类型，没有任何一条是客户端自行发现的（`GUI-CORE-022`） |
| `d10-ticker` | `…/qa.html?state=d10-ticker` | Lane 卡片下方的 D10 事件走马灯：Core 审计时间线的一页有界 newest-first 记录，两个项目交错出现，因此该条展示的是跨项目的同一个顺序而不是按项目分组的列表；每行携带 Core 的稳定 id、原样的点分 action key、owner 与时间戳（`GUI-CORE-014`） |
| `lane-rail` | `…/qa.html?state=lane-rail` | 侧栏被固定展开（它默认自动隐藏），显示名为 `viden` 的唯一 `.wsroot` 项目分组、`▾` 折叠控件、Lane 计数、分组内 `＋`、嵌套其下的 Lane，以及 `＋ 添加项目…` 页脚；没有第二个分组，也没有「Global」分区 |
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
