# Viden GUI

英文版：[README.md](README.md)

本目录是 Viden 的 GUI 实施线。Alpha 证据门禁已经选择 Tauri，
`0.1.0-rc.3` 使用 Core `0.3.5` 的同状态 fixture 认证规范 D1 驾驶舱。
`0.1.0-beta.1` 建立唯一 production desktop bootstrap。原生启动器会通过
frontend-safe `LocalCoreHost` 打开工作区，并把它的 `CoreClient` 注入
`GuiCoreAdapter`。应用始终先显示 D1 驾驶舱外壳。未绑定工作区时，“打开项目”只会
打开系统文件夹选择器，并通过 `LocalCoreHost::open_workspace` 重绑；它不会进入 D11，
也不会要求选择模型。已打开但没有 Lane 的项目仍显示项目驾驶舱及“新建 Lane”，
并打开 D1 的 New Lane 弹层，用于快速启动原生或 ACP Lane；精确 Core Lane receipt
会把焦点带回 D1。

## 本地运行桌面客户端

进入 `apps/gui`，安装锁定版本的前端依赖并启动原生 Tauri 开发窗口：

```bash
npm ci
npm run tauri -- dev
```

Vite 与 Tauri 固定共用 `http://localhost:1420`；如果端口被占用，启动会直接失败，
不会静默切换到其他端口。只有显式设置 `VIDEN_GUI_WORKSPACE` 时，原生 bootstrap 才会
预先绑定工作区；否则 D1 欢迎页使用系统文件夹选择器。真正的 Core 启动失败才显示明确
D6 断开状态。

macOS 仍保留原生红黄绿窗口按钮，但标题栏使用 overlay，由 HTML 外壳提供深色可拖拽区域；
不再渲染原型窗口边框、圆角内框或白色原生标题条。

构建并打开不依赖 Vite 开发服务的独立 macOS debug App：

```bash
npm run tauri -- build --debug --bundles app
open ../../target/debug/bundle/macos/Viden.app
```

直接运行 binary 时，可以显式绑定项目：

```bash
VIDEN_GUI_WORKSPACE=/absolute/project/path \
  ../../target/debug/bundle/macos/Viden.app/Contents/MacOS/viden-gui
```

桌面宿主会在 Core bootstrap 前，将实际存在的标准用户工具目录（如 `~/.local/bin`、Bun、
Volta、asdf、mise/fnm 与 Homebrew）加入继承的 `PATH` 前部。这样从 Finder 打开 App 时，
Agent 可用性探测与后续 ACP spawn 使用同一命令路径；整个过程不执行 login shell，也不写死
当前机器的绝对路径。

## 冻结输入

| 字段 | 值 |
| --- | --- |
| GUI 组件版本 | `0.1.0-rc.3` |
| 最低 Core 版本 | `0.3.5` |
| 支持 frontend schema | `[1]` |
| 共同分支基线 | `3a7740ea72e58f4a22248a80f9e7324c49bb0f73` |
| Core 最终 checkpoint | `f7fe1b31dfb237e4062209767a7051c2b2c68b93` |
| Core code checkpoint | `17fa2071398d5eaf30045257163d57d22d99177b` |
| 合同 payload | `5bd2b80b0953f4194d082940a7b9164c7231ca2d` |
| 规范 D1 fixture | `d1-main-cockpit.json`，SHA-256 `f96ba30cc6e80aa52cb15a2fd1f03c082487a3cd4779c25f61e42ee1548e1e3b` |
| 必需 Core capabilities | 15 项冻结能力加 additive extension capabilities，包括 `runtime.cockpit_context_v1` |
| 内置 locale | `en`、`zh-CN` |
| 外观系统 | 5 套 skin、8 组有效 skin/mode、3 档 density、3 种 motion |

当前机器可读 manifest 是 [release-manifest.toml](release-manifest.toml)。
不可变 rc.3 快照是
[manifests/0.1.0-rc.3.toml](manifests/0.1.0-rc.3.toml)；此版本 checkpoint
下两者必须逐字节一致。更早的 alpha、beta 与 rc.2 快照继续作为历史证据保留，不会被重写。

## 设计真源顺序

视觉和交互盘点固定从以下层级进入：

1. `docs/viden-design/Viden/index.html`
2. `docs/viden-design/Viden/GUI/Viden - 设计稿索引 (GUI).html`
3. `docs/viden-design/Viden/GUI/Viden - 组件库 (GUI).html`
4. `docs/viden-design/Viden/GUI/Viden - 桌面驾驶舱 (GUI).html`（D1）

D11 项目配置、D4 Lane 创建、D6 运行期恢复都是
`docs/viden-design/Viden/GUI/pages/` 下的从属屏。它们定义操作闭环，但不能替代 D1
作为桌面驾驶舱基线。

可复现的 design revision 还必须覆盖已登记的组件语义和这些屏幕实际消费的本地源：
`docs/DESIGN-REF.md`、`GUI/gui-kit.css`、`GUI/gui-icons.jsx`、
`GUI/gui-titlebar.jsx`、`GUI/gui-statusbar.jsx`、`GUI/gui-inbox.jsx`、
`GUI/gui-settings.jsx`。Manifest 记录精确有序列表；archive 和 mock 源不计入。

## Core 边界

GUI 代码只能依赖 `viden-core` 和 GUI 自有框架/平台代码。允许使用的 Core 入口是：

- `CoreClient`、`CoreTransport`、`StatefulCoreClient`、`LocalCoreTransport`；
- `CoreHandshake`、schema/capability 常量、command/event envelope、snapshot、replay、
  transcript paging、`RuntimeViewState`；
- `viden-core` 重新导出的 frontend-neutral domain records。

GUI 禁止导入 `viden_core::legacy`、`viden-runtime`、`viden-provider`、
`viden-tools`、`viden-permissions`、`viden-session`、`viden-workflows`、
`viden-context` 或 config internals。所有 mutation 都发送 `RuntimeCommand`；
只有收到 `CommandAccepted` 和后续有序 state event 后才能显示成功。

前端侧的同一纪律收敛为单一宿主接缝：`src/host/core_client.ts` 定义传输中立的
`GuiCoreClient` 接口（区别于上文 Rust 侧的 `CoreClient`），
`src/host/tauri_core_client.ts` 是前端唯一允许导入 `@tauri-apps/*` 的模块。
屏幕和 shell 只消费注入的接口，因此更换桌面宿主意味着提供另一个
`GuiCoreClient` 实现，而不是修改屏幕代码。

## 对照 Core `0.3.5` 的清单

| GUI 区域 | 设计意图 | Core `0.3.5` 状态 | GUI 处理 |
| --- | --- | --- | --- |
| 打开项目 / D11 接入 | 原生文件夹打开，以及 project probe、provider health、config preview/confirm、credential handles | `LocalCoreHost::open_workspace` 已提供可信文件夹重绑，`runtime.recent_work` 已响应 `QueryRecentWork`；Core 不发布首启接入信号，也没有仓库克隆命令、项目脚手架命令与并发多根托管 | Welcome 直接使用原生选择器和 host rebind，并列出 Core 的最近项目；标题栏项目选择器提供带确认的切换。D11 只作为项目内显式配置流程，不接管打开文件夹。入口为 `?screen=d11` 与 agent 菜单的 `Full setup`；shell 不会自行重定向进入 |
| D4 Lane 创建 | typed role、route、gate strength、mutation policy、target、budget、worktree preview、lane receipt | 已有 `PreviewStarterLane`/`CreateStarterLane`、Core 解析 preview、invalidation、approval、精确 receipt 与 `runtime.starter_lane_preview` 广告 | Task 8 渲染四步复核流程；连接旧版 Core 时仍以可见 unavailable 和零发送 fail closed |
| D1 驾驶舱 | 无项目欢迎中心、零 Lane 项目驾驶舱、activity/lane rails、streaming transcript/tool rows、Environment、Live Work、composer、evidence/context/cost facts | stream/tool/approval/queue/task/lane/owner/evidence/context/cost/preferences/recent-work facts 已有；diff/apply、稳定 audit timeline、可操作 Lane recovery 与并发多工作区托管尚不完整 | 未绑定 host 才显示 Welcome；已绑定空项目仍留在 D1 并提供“新建 Lane”；Lane 侧栏把 Lane 收拢在 Core 托管的那一个项目分组下（`GUI-CORE-023`）；实时工作从 `RuntimeViewState` 渲染 |
| Permission dock | scoped approve/deny、risk、target、expiry、default action、audit id | `ApprovalRequestView` 和 `RespondToApproval` 已有 | 可经 Core 使用；GUI 不得直接执行 tool |
| D2 决策中心 | 跨 Lane 的统一决策队列：闸审批、lane 问询、契约确认共用「上下文 / 证据 / 动作栏」一套卡片骨架 | `pending_approvals` + `RespondToApproval`、`review_requests` + `DecideReview`、`contracts` + `ConfirmContract` 已有；审批的结构化 diff 与待确认契约事实缺失 | 入口 `?screen=d2`；闸、契约与评审决定都发出 Core 命令，评审裁决可携带可选评审意见，且只由 Core 发布的有序 `ReviewRequestUpdated` 确认；Core 会拒绝的评审保持禁用并标注 `D2-REVIEW-SETTLED` 或 `D2-NO-REVIEWER-ACTOR`，审批 diff 以 `GUI-CORE-012` 声明不可用，契约分组以 `GUI-CORE-013` 标注为已决历史 |
| D10 Lane 监视器 | 跨项目每条 Lane 一张卡：门控强度、状态、进度、证据、成本可计量性与「等你」计数 | `lanes`、`lane_runtime_owners`、`tasks`、`agent_sessions`、`latest_evidence`、`AgentLaneRecord.run_stats` 已有；有序历史来自审计时间线而非视图状态 | 入口 `?screen=d10`；只读，门控强度取自 `AgentLaneRecord.gate_strength` 而非 agent 标签，未绑定 Lane 不显示项目，无 Core 任务的 Lane 不显示进度；成本不可计量路由（`AgentRoute::cost_meterability`）会被标记，并展示 Core 记录的有界运行事实而非推断成本——Core 未观测到运行时该组事实缺席而不是补零，退出码缺失时明确标为未知；事件流是 Core 追加式审计时间线（`QueryAudit` -> `AuditPageLoaded`，limit 50，不加作用域以覆盖每个项目）的一页有界 newest-first 记录，逐行渲染稳定 id、原样的点分 action key、所属项目与 Lane、以及时间戳；缺少 `runtime.audit`、读取中、被拒绝、已回答但为空，保持四条不同的文案 |
| D12 集成闸 | 冲突横幅、闸策略、退回原 Lane 的恢复时间线、合入后回滚，且不提供手动 merge | `merge_gates`、`conflict_bounces`、`reverts`、`check_runs`、`AcceptMergeGate`、`RejectMergeGate` 已有；不发布结构化冲突内容 | 入口 `?screen=d12`；`批准并合入` 与 `退回原 Lane` 各自发送对应 Core command，只有满足 `decide_merge_gate` 实际执行的规则时才开放，否则标注阻塞代码；时间线与回滚按选中闸限定，冲突 hunk 以 `GUI-CORE-015` 声明不可用 |
| D14 审计与时间线 | 谁在什么对象上做了什么、结果如何，另加一份用于诊断的原始有序事件日志 | `runtime.audit` 之下的 `RuntimeCommand::QueryAudit` -> `AuditPageLoaded` 已有，`CoreClient::replay` 的 `ReplayRequest`/`ReplayBatch` 与 `EventCursor` 也已有；`AuditPageLoaded` 不携带 command id、`AuditQuery` 没有 actor 与时间过滤（`GUI-CORE-024`），视图状态没有事件日志（`GUI-CORE-014`） | 入口 `?screen=d14`，并可从 D2 决策详情与 D12 回滚行按 Core 实际关联的审计对象带范围进入。两种模式：**审计**（默认）以 newest-first 分页读取 Core 的追加式审计存储，采用 acceptance-first 关联（页面只有在 Core 接受了本次确切 `command_id` 之后才被采纳，本地拒绝第二个并发读取，行只来自确认页），点分 `action` key 原样渲染——它是 Core 稳定且可 diff 的词汇表，本构建无法命名的 actor 或 outcome 标为 `unknown` 而不是借用已知值，每条记录的时间在所有语言下都按固定的 `YYYY-MM-DD HH:MM:SS UTC` 时钟渲染——审计记录是要跨机器比对的证据；**原始事件回放（诊断）** 保留回放 cursor 日志，行标签用 Core 自己的 serde 判别名，无法解码的事件仍占一行，回放失败显式提示而不是给出更短但看起来完整的轨迹。缺少 `runtime.audit` 时 D14 直接以原始模式打开、点名该 capability，并且零发送审计命令。设计稿的过滤 chip、按天分组、详情侧栏、汇总与导出未实现；其中需要 Core 的部分记为 `GUI-CORE-024` |
| D13 Fleet 编排与 Workflow | 每个 workflow DAG 一块看板：声明的依赖边、节点运行状态、阻塞原因与 Lane 交接 | `agent_dags`（含 `AgentDagTaskSpec`）、`tasks`、`dependencies`、`handoffs` 已有 | 入口 `?screen=d13`；只读，依赖边取自任务规格自身的 dependencies，节点只有在 Core 真正跑该任务时才显示状态，阻塞只来自 Core 的 `DependencyState::Blocked` 记录，交接绝不由依赖边推导 |
| D6 恢复 | 连接中、断连、agent stopped、budget exhausted、gate queue clear、reconnect/restart/close actions | Runtime errors、CoreClient snapshot recovery、context budget facts、queue/gate facts、`RetryAgentSession` 与 `StopLane` 已有；检查点完全未被建模 | Task 10 渲染运行期 Core-owned 恢复状态；无项目 `empty` 状态由 D1 Welcome Center 承担；restart 与 close Lane 针对 Core 发布的唯一目标发送对应 Core 命令，inspect 在本地展开既有事实，checkpoint 仍以 `GUI-CORE-003` 明确禁用（`GUI-CORE-018`） |
| Locale 与换肤 | `en`/`zh-CN`、Aurora/Ice/Mono/Amber/Phosphor、明暗约束、density、motion | 已有 `RuntimeSnapshot.ui_preferences`、`SetUiPreferences`、`ResetUiPreferences`、`UiPreferencesUpdated`、持久化和安全回退诊断，并以 `ui.preference_persistence` 公布 | rail 上的设置齿轮编辑未保存 draft 并发送 `SetUiPreferences`/`ResetUiPreferences`；只有有序的 `UiPreferencesUpdated` 才改变渲染状态，缺少该 capability 时面板以只读方式打开 |

开放请求记录在 [contract-requests.md](contract-requests.md) 和
[contract-requests.zh-CN.md](contract-requests.zh-CN.md)。GUI 不得用私有 reducer 或直接访问
runtime 来绕开这些缺口。

剩余开放请求只阻塞各自点名的生产屏，不阻塞 framework-neutral、fixture-only 的
Task 2-3 及其证据；spike 结果不能授权生产 mutation 或 persistence。

## D11 项目接入

Task 7 在固定 Core `0.3.2` integration checkpoint 之后实现 D11 项目内显式配置流程。
`GuiCoreAdapter` 会按 advertised extension capabilities 分别 gate project onboarding 与
credential-handle intents。Probe、preview 与 confirm 保持为不同 Core command；D11 的
starter 选择只形成有序本地复核队列，绝不发送旧 `CreateLane`。只读 `d11_poll` command
会在首次有界等待之后继续接收迟到事实；adapter 会串行化
pending intake command，并在清除 pending 前匹配 preview hash、confirm id/hash、Lane id
和 active approval request id。Project-config approval 必须先绑定精确 Core metadata
token `sha256=<64 lowercase hex>`，GUI 才接受它的 request id；如果存在有边界的
`preview_id=` token，也必须等于 pending preview id。hash substring、非 lowercase
hash、free text 和非 `sha256` 字段都不能 retarget 或清除当前 pending command。
Allow 决策后继续等待 matching business fact；deny/expiry 决策会
清除 pending，并继续 drain 后续 Core error projection。瞬时 poll 失败会保留 local draft
与 pending identity，然后按有界 backoff 重试。等待期间的 Core 中间投影变化仍会显示。
Cancel 只清除内存导航状态，不发生 Core mutation，并返回 D1。Welcome 不进入该流程：
文件夹选择和 host rebind 会先独立完成。

独立 host 会先消费有序 command events，再刷新权威 snapshot，因此 acceptance 不会吞掉
后续 probe、preview 或 confirmation 事实。新项目 draft 默认包含必填的 `name` 与 `pack`
字段。若确认需要 Core 批准，D11 会嵌入与 D1 相同的 typed Permission Dock；
`Allow once` 或 `Deny` 仍是显式 Core command，不会成为 GUI 侧绕过。

Shell 通过 `?screen=d11` 与 agent 菜单的 `Full setup` 进入 D11：该动作打开完整接入
流程，而不是单 Lane 的 D4 表单。`d11_poll` 同时充当入口读取与等待，因此重新进入会
继续等待仍未收到 Core 回执的命令，而不是重新开始；D11 收集的起始 Lane 种子交给 D4，
由 D4 拥有 preview/confirm 回执循环。没有自动跳转进入 D11：Core 不发布首启接入事实，
客户端只能凭空编造一个。

## D4 起始 Lane 复核创建

Task 8 从项目驾驶舱的“新建 Lane”进入 D4，每次只复核一个 seed。创建前 Cancel/Skip
会零 mutation 返回 D1；创建发出后，界面只提供精确 Core
approval 的 allow/deny，不伪造 cancel command。每个完整
`StarterLaneCreated.receipt` 推进一项，最后一个 receipt 发出 typed D1 导航请求，并聚焦
最后创建的 Lane。

Adapter 发送 `PreviewStarterLane`、保留原始请求，并且只接受同 owner 的
`StarterLanePreviewed` 事实。branch、worktree、base revision、route、gate、target、
mutation policy 与 budget 都是只读 Core 事实。Build 模式只能创建未变更的已复核请求；
Plan 模式可预览但不可创建。`CommandAccepted` 与 `LaneUpdated` 都只是中间事实，绝不触发
导航。请求变更、rejection、approval deny 和 typed preview invalidation 会保留 webview
draft，并要求重新预览。只有 owner/id/hash/Lane/branch/worktree/base/config 全量匹配的
`StarterLaneCreated` 才能推进队列。

Core `0.3.5` production handshake 已包含精确 additive capabilities
`runtime.starter_lane_preview` 与 `runtime.cockpit_context_v1`，因此 D4 与 D1
Context Dock 可使用已复核 typed flow。连接旧版或不完整 Core 时仍明确展示门禁并保持零发送；
`runtime.lane_lifecycle` 不能作为替代。

rc.3 的确定性浏览器证据位于 `evidence/0.1.0-rc.3/`。完整 8 组主题矩阵、3 档
density、两套 catalog 和 reduced-motion 行为仍由自动化测试覆盖。

配置 rail 只渲染 Core 返回的 `viden.toml` 精确复核内容，confirm 的 preview id 与 SHA
也从当前 Core projection 复制。Credential 行只显示 masked handle。由于尚无
frontend-safe 平台 credential staging channel，raw credential 输入与 webview
`StoreCredentialHandle` 路径以 `GUI-CORE-001` 明确禁用。D11 的历史面板现与下文的
Welcome 中心和项目选择器渲染同一份 Core `QueryRecentWork` 清单，以
`runtime.recent_work` capability 为门（旧的 `GUI-CORE-007` 临时文案已退役）；
这些界面都不扫描 local storage、JSONL 或 SQLite。
project switching 现通过 Core-owned `LocalCoreHost::open_workspace` 完成；
安全 raw credential staging 仍是 `GUI-CORE-001` 的未完成部分。

## D1 流式驾驶舱

Task 9 让 D1 成为常驻应用外壳和唯一主工作面。它先显示 D6 连接中/断开状态，或
host-owned 无项目欢迎页。已绑定但没有 Lane 的项目仍是 D1；D4 只从项目内创建入口进入，
D11 只用于显式配置。Activity rail、Lane rail、Environment、Live Work、
transcript/tool rows、排队状态、evidence 与 composer 都是 Core 最新
`RuntimeViewState` 的 transport-safe 投影。Webview 只持有焦点、draft、布局、有界行窗口
与滚动锚点，不解析显示字符串，不持久化第二套 workspace 模型，也不会把 command
acceptance 当作业务成功。有序 Core 刷新会原位更新 activity 与 Lane rail，让它们的
hover 根节点在易变的 Lane/Agent 状态变化期间保持挂载；这样既不会隐藏最新 Core 事实，
也不会让浮动侧栏反复闪烁。Activity rail 中每一个可用槽位背后都有真实动作：路由槽位打开
其对应的已恢复屏幕，Lane 槽位切换 Lane rail，而标记为 `aria-current`（因为它正是当前屏幕）
的 `Work` 槽位把焦点交还给 composer。没有可用动作的槽位一律禁用，而不是既可点击又无响应。

“新建 Lane”会打开一个紧凑的锚定弹层，默认选中内置 Viden Agent，并包含已发现的 ACP
Agents、品牌身份、任务 draft、Core 投影的 eligibility/probe 诊断，以及只作呈现的
isolation 提示。“完整设置…”进入既有 D4 兼容流程，不在快速创建器里继续堆叠选项。
Git 工作区预览由任务文本派生的 branch/worktree；非 Git 目录明确提示 Lane
直接在已打开工作区运行，不创建二者。
选择 Agent 不会关闭弹层，任务 textarea 会获得焦点，任务非空前“创建 Lane”保持禁用。
该 textarea 持有 IME 组合态时，会推迟 Core 或 Agent 探测引发的界面重绘，避免 macOS
候选输入过程中节点被移除。
创建时沿用现有有序路径：`preview_default_lane`、`create_starter_lane`，并且只有在精确
Core Lane 已投影后才发送原生 `submit` 或 ACP `start_agent_session`。Transport 或 Core
拒绝会保留 draft，并使用 D1 已有 typed rejection surface。ACP 发现只在当前驾驶舱生命
周期内自动执行一次；重新打开弹层会复用结果。发现失败会结束忙碌态，在弹层中显示精确
诊断，并且只有用户点击“重试 ACP 检测”才再次执行。ACP 启动被拒绝时，其 Lane 创建后仍
会在 D1 typed rejection surface 上显示原因。

当 assistant stream、tool、task、approval 或 queued input 仍活跃时，composer 仍可编辑。
此时 Enter 发送 `QueueFollowUp`，空闲时发送 `SubmitUserInput`；Shift+Enter 保留多行输入，
CJK IME 组合阶段绝不提前提交；streaming Core 重绘同样会等到 `compositionend` 后再替换
输入节点。两种命令都使用 Core 发布的精确 Lane owner，来源只能是
live owner binding 或 D4 receipt。Cancel 更严格：只有选中 Lane 仍 active、Core 宣告
`runtime.lane_owner_projection`，并且 `lane_runtime_owners` 中恰好有一个完整匹配 owner 时，
控件才可见且允许传输。owner 缺失、错配或歧义都 fail closed，保持零发送。
若另一个有序 Core command 或 Agent probe 仍占用客户端命令槽，一条 composer 输入会以
`Queue follow-up` 可见等待，同时禁用重复“发送”；槽位释放后自动投递，绝不静默丢弃输入。
桌面端重启后，Core 恢复出的唯一 terminal ACP Session 即使已经没有进程内 Lane binding，
仍可使用其精确、持久的 Session owner 接收后续输入。Continuation 启动前，Core 会为同一
持久 owner 发布新的 `LaneRuntimeOwnerBound` 事实，因此 ACP 响应运行期间 Lane 会持续显示
busy，而不会短暂掉入 Agent Stopped；重复 Session、owner 错配以及非 ACP 恢复仍然 fail closed。
在同一次 App/Core 生命周期内，已完成 ACP turn 会保留健康的进程与远端 Session；下一次
“发送”因此直接进入 `session/prompt`。常驻连接已退出或不兼容时，Core 会在发送 prompt 前
回退到持久化 `session/load` 路径。GUI 不持有这份缓存，仍只渲染 Core 的有序事实。即时
Starting/busy 反馈衡量 Core dispatch；首个 assistant 内容的等待时间还包含冷启动时的 agent
启动、上下文处理和模型推理。
Core 将常驻池限制为八个 Session，并回收空闲 15 分钟的连接；之后的“发送”会透明走持久化
reload 路径。Core 关闭时也会回收该工作区的全部常驻连接。
取消内置 Agent 的活跃模型推理时，现在会先终止该 turn，再考虑 Lane 生命周期取消，因此
Lane 与其精确 owner 仍可继续路由。对于已经没有 owner 的旧 terminal 原生 Lane，composer
和“发送”会显示为禁用，而不是保留一个点击后无反应的控件。

Composer meta 行提供三个弹层选择器：工作模式（规划、构建、评审、探索）、权限级别
（询问、自动编辑、自动、只读、完全访问）与模型（按活跃 provider 与 Core 发布的各
adapter 模型列表分组；当前组合高亮，Core 未发布选项时绝不凭空生成）。选择某项会经由
host 命令 `set_work_mode`、`set_permission_level`、`select_model` 发送
`SetWorkMode`、`SetPermissionLevel` 或 `SelectModel`，它们共用有序的 D1 pending
管线并以刷新后的投影返回。选择器从不在本地套用 Core 的模式/权限联动规则：两个
pill 都只按 Core 重新发布的 snapshot 重绘，因此选择“规划”时权限 pill 恰在 Core
宣告的那一刻变为“只读”。控制命令在途时整行 `aria-busy` 且 pill 禁用；composer 不可
编辑或未打开工作区时同样禁用。Core 拒绝或传输失败都渲染在 D1 已有 typed rejection
surface（`role=alert`）上。弹层遵循 agent-menu 约定：Escape 关闭并把焦点交还 pill，
外部点击关闭，方向键移动选项焦点。

活动 rail 在 spacer 之后以原型的设置齿轮收尾。它打开 provider/模型、权限、语言、
皮肤、明暗、密度与动效的设置浮层，取自注册设计组件 `GUI/gui-settings.jsx`，
只使用共享 token。

浮层的前两节是**编辑器 pill 的第二个界面，而不是第二套模型**。权限节使用与 pill
相同的 `PERMISSION_LEVELS` 枚举渲染权限档位——每行给出 UI 标签、逐字照抄的 Core
CLI 标识符，以及一行说明——从同一个 `statusbar.permissionLevel` 事实读取当前档位，
并派发同一个 `SetPermissionLevel` intent。Provider 与模型节列出的正是 pill 所用
`modelGroups()` 的返回值（当前 provider 组，加上 Core 发布的每个 adapter 组），
标出 Core 的当前选择，并派发同一个 `SelectModel` intent。二者在编辑器不可编辑、
未打开工作区，或有命令占用 Core 的单命令 D1 槽位时被禁用——绝不隐藏；该门控与
`ui.preference_persistence` 无关，后者只管下方的 draft/保存那一半。一个只读的
工作目录行写明 Core 打开的工作区根目录。

设计在这两节里画出、而此处刻意留白（而非伪造）的元素有三处：新增 provider 动作与
各 provider 的 API key 标记（凭据仍属 GUI-CORE-001 残留范围）、Requests 卡片
（`request_timeout_secs`、`max_retries`、`provider_plugin_dirs` 在
`frontend-contract-v1` 中没有对应的 Core 命令），以及权限规则预览框与可编辑的
“额外工作目录”字段（Core 既不发布规则表，也没有范围命令）。每处留白都在代码注释
里点名对应的设计元素。没画出来不是谎言；画一个够不到 Core 的控件才是。

语言与外观控件维持既有契约。它们都只编辑未保存的 GUI 本地 draft：未改动任何轴时
“保存”保持禁用，并且只有操作
者真正选择过的轴才进入 patch，未触碰的轴继续沿用 Core 的解析结果。保存经由 host
命令 `preferences_save`、`preferences_restore`、`preferences_poll` 发送
`SetUiPreferences`，“恢复默认”发送 `ResetUiPreferences`。

确认的唯一依据是有序的 `UiPreferencesUpdated`：重新发布的 snapshot 不是持久化回执；
保存只有在持久化的 `[ui]` 表带回 patch 请求的每个值时才算确认；恢复则以该表消失、
而 resolved 回退仍可渲染为确认。确认后面板采用 Core 的解析结果——包括点击并未请求、
由 Core 联动改动的轴——并应用到实时主题与文档语言。客户端不预先校验皮肤/明暗组合，
因此 Amber 配浅色这类组合仍可选中，并以 Core 自己的拒绝理由出现在 `role=alert`
行中，与 Core 的 diagnostic 并列；规划/评审/探索模式的拒绝走同一条路径，配置文件
字节保持不变。命令在途时面板 `aria-busy` 且所有控件禁用。当 Core 握手未公布
`ui.preference_persistence` 时，齿轮仍会打开只读面板并写明该 capability——既不隐藏
入口，也不给出可点击但无响应的控件。浮层遵循 agent-menu 约定：Escape 关闭并把焦点
交还齿轮，外部点击关闭，方向键移动选项焦点。

驾驶舱标题栏承载工作区的源码管理事实：host 依据 Core 的 `workspace_source` 采样
计算，投影为 `topbarSource`。项目选择器显示 Core 发布的项目名——Core 未命名时显示
它确实发布的工作区路径，绝不从路径推导名称——其后是 `⎇ <分支>`，采样报告有未提交
更改时再加一个 dirty 标记。旁边的 `.gitops` 块含两个 chip：`↑ahead ↓behind` 是
`role=status` 元素而非按钮，因为 frontend-contract-v1 没有发布任何面向操作者的 git
命令（契约请求 `GUI-CORE-020`）；`⎇ N 个工作树` 是该块唯一的控件，点击进入 D10 Lane
监视墙，未注入导航回调时禁用。`N` 统计项目活跃 Lane 的去重工作树：Core 没有发布 git
worktree 清单，且两个 Lane 共用一个工作树只算一个。Core 未发布工作区源或报告其
unavailable 时，整块省略，而不是渲染会被读成「干净且已同步」的零值；truncated 采样
保留已发布计数，但加上截断标记。设计稿的 `▾` 项目选择器在多项目栏出现前刻意缺席
——GUI 不发布可点击但无响应的控件。

驾驶舱状态栏按终端词表渲染 host 计算的 statusbar 投影分段：`MODE`、`PERM`、
`CONTEXT`（最近的工作区预算）、`EVENTS`（重放游标流位置；frontend-contract-v1 没有
事件计数器，因此以位置标注）、`LANE`（选中 Lane、其唯一绑定 agent、状态与任务进度）、
`LATENCY`、`TOKENS`（输入↑输出↓）、`DIAG`（运行时错误数）与 `REQ`（provider 请求/错误
计数）。Core 事实缺失的分段渲染明确的破折号，而不是编造数字。存在待审批或开启的
merge gate 时，右侧显示待审闸分段——状态栏唯一的交互元素——点击进入 D2 决策队列。

Transcript 最多保留 240 行。离开最新输出边缘后会设为 `follow_latest=false`、保留当前锚点，
并递增可见的新输出计数，而不会强制滚动。Rust 与 webview 测试覆盖 10,000-event burst、
50,000 行、resize/idle 读取、CJK composition、多行 paste/undo、键盘遍历、ARIA region 与
可见焦点。`evidence/0.1.0-rc.3/` 下的 Browser-controlled 证据包含 Aurora
dark/regular 英文、Ice light/regular 英文、Aurora dark/regular 中文、compact density
和 responsive drawer 状态，并包含一个由 `d1-main-cockpit.json` 填充的独立同状态设计
reference。它还包含一个补充 Context Dock bottom-state capture，用于证明下方事实可通过内部
滚动到达。Diff、apply、audit 与未类型化 recovery actions 始终是明确 unavailable facts；D1
不会伪造成功占位。

## 项目、最近工作与分组侧栏

Core 一次只托管**一个**工作区。`LocalCoreHost::open_workspace` 每次调用都会新建一个
`RuntimeSupervisor`，桌面宿主只是替换它唯一的 adapter 槽位，因此一次成功的打开会
**替换**当前工作区：旧 supervisor 被 drop 时会 join 其工作线程并关闭所有常驻 ACP
会话。下面的一切都由这条事实推导而来。

**最近工作**是一次 Core 读取，而不是客户端扫描。`queryRecentWork` 发送
`QueryRecentWork`，并且只把有序的 `RecentWorkLoaded` 事实当作答复——先行的
`CommandAccepted` 按命令 id **与**命令变体双重匹配，重新发布的快照永远不构成确认，
在本命令被接受之前到达的清单答复属于其他读取方。Core 拥有共享会话主目录的扫描、
`1..=100` 的钳制、白名单 DTO 与排序；GUI 原样重新序列化并且不重新排序，Core 的诊断
逐字渲染。四种状态保持区分，不会塌缩成同一个空列表：缺少 `runtime.recent_work`
能力、Core 拒绝、Core 已接受但尚未答复的读取，以及确实为空的清单。

**Welcome** 用相对时间与来自同一条有界事实的会话计数列出这些项目。Welcome 只在没有
绑定工作区时渲染，因此点击最近项目会直接打开——没有需要替换的对象。

**项目选择器**从标题栏 `.projsel`（只有在选择器确实能打开时，它才获得设计稿的 `▾`
与按钮语义）与侧栏的 `＋ 添加项目…` 页脚打开，绘制设计稿的三列：

| 列 | 内容 |
| --- | --- |
| 添加 | `添加目录…` 调起原生选择器，随后进入切换确认。`克隆仓库…` 与 `新建空项目` 可见且**禁用**，并点名 `GUI-CORE-023` |
| 工作区内 | 恰好一行——当前打开的项目，标记为当前项且不可点击，附带 Lane 计数 |
| 最近 | Core 的最近项目，排除已打开的根；点击进入切换确认 |

**每一次切换都要内联确认**，绝不使用浏览器 `confirm()`。确认步骤点名目标根目录，
说明 Viden 一次只托管一个工作区因而这会替换当前工作区（`GUI-CORE-023`），并计出将被
拆除的正在运行的 Lane 与 Agent 会话数量。空闲工作区同样需要确认，只是文案更温和：
没有工作被中断，但会话仍会被关闭并重建。确认中按 Escape 退回三列而不会同时关闭浮层；
在三列状态按 Escape 关闭浮层并把焦点交还给打开它的锚点，且解析的是**当前存活的**锚点
——因为一次 Core 刷新会重建标题栏与侧栏。

**Lane 侧栏**就是设计稿的工作区浏览器：一个 `.wsroot` 分组头，承载 Core 发布的项目
名（Core 未发布时用工作区路径）、一个 `▸`/`▾` 折叠控件（状态属于 GUI 本地状态，能跨
有序 Core 刷新保留）、分组内的 `＋`（仍是同一个创建 Lane 动作，只是在设计稿的裸字形
背后补上了可访问名称），以及嵌套其下的 Lane 行。设计稿画了多个项目分组和一个跨项目的
「Global」分区，两者都是 mock 数据，因此侧栏只渲染**一个**分组，不伪造同级项目。这就
是 `GUI-CORE-023` 可见的那一半。

## 命令面板

标题栏的面板按钮与 **⌘K**（macOS 之外为 ⌃K）打开 `Viden - 桌面驾驶舱 (GUI).html`
里绘制的驾驶舱命令面板（`scrim top` / `palette` / `palin` / `palsec` / `palrow`）。
**⌃P** 打开同一层浮层并预置 `>` 作用域，与设计稿 Composer 说明
（`⌘K palette · ⌃P commands`）一致。

查询语法与模糊打分是对 TUI jump 索引（`apps/tui/src/tui/jump.rs`）的刻意移植，
因此跨前端只有一套选择器语言：

| 前缀 | 作用域 |
| --- | --- |
| `:` | Lane |
| `@` | Agent 会话 |
| `#` | 合并闸与询问 |
| `>` | 命令（动作与设置两个分区） |
| `~` | 文件 |
| _无_ | 所有类别 |

不带前缀的查询按子序列匹配每一行的标题、上下文与关键词，采用与 TUI 相同的
「位置分 + 相邻加成」算法。行不会在光标下被重新排序：设计稿的分区顺序（动作、
跳转到、设置、文件）保持不变，正如 TUI 保持其分组顺序。

选中 Lane 或 Agent 会话会走与 Lane rail 完全相同的路径**在驾驶舱内**完成选择，
随后聚焦 Composer；对当前屏已经拥有的东西，面板绝不跳转离开。合并闸打开 D12，
询问打开 D2，各自携带确切的 Core id，而目标屏在渲染前仍会自行重读它自己的 Core
投影。

跨 Lane 的闸与询问不在按 Lane 作用域的 D1 投影里，因此打开面板时由外壳读取
`d2_decisions` 与 `d12_integration_gate`——这两个投影本就存在。读取是预取的，
所以 `#` 一敲下去就有答案；也是 fail-soft 的：读取被拒时只把那一个分区降级为一条
携带 Core 原话的提示，Lane、会话与动作照常可用。「文件」分区列出 Core 发布的
工作区清单（`runtime.workspace_files` 之下的 `QueryWorkspaceFiles` ->
`WorkspaceFilesLoaded`，GUI-CORE-022），以同样预取且 fail-soft 的方式读取，并按 Core
自己的字典序渲染。客户端不遍历任何东西。缺少该 capability 的 Core 会保留点名该请求的
永久禁用行且零命令发送；读取中、携带 Core 原话的拒绝、以及已回答但工作区为空，各自
占一行，因此空列表绝不会顶替其中任何一种。TUI 的 jump 索引对这四种情况一一对应。

### 与 TUI 的按键分歧

GUI 在这里遵循它自己的设计，而它与终端客户端正好相反：

| 组合键 | GUI | TUI（`apps/tui/src/tui/keymap.rs`） |
| --- | --- | --- |
| ⌘K / Ctrl+K | 打开面板 | 命令面板 |
| Ctrl+P | 打开面板并限定 `>` | jump 索引 |

这是刻意为之，不是漂移。⌘K 是驾驶舱设计在标题栏 tooltip 与 Composer 说明中明确
承诺的桌面惯例；GUI 面板是一个单一界面，本就同时包含 TUI 拆到两个组合键上的两半，
因此把 ⌃P 绑到这一个界面的命令作用域，既让设计稿的说明文字诚实，也保留了
「⌃P 意味着命令」的肌肉记忆。两个客户端的**查询**语法完全一致——操作者在两者之间
切换时，真正重要的正是这份一致。

面板内的 Escape 只负责关闭它自己。驾驶舱在 window 级把 Escape 绑到「取消进行中的
轮次」，因此浮层会吞掉自己的关闭键，而不是在退出时顺手取消 Core 的工作。浮层是
`role="dialog"` / `aria-modal`，输入框是带标签、拥有一个 `listbox` 的 `combobox`，
高亮行是它的 `aria-activedescendant`，焦点在关闭时交回标题栏按钮——并在关闭那一刻
重新解析，因为面板打开期间 Core 刷新可能已经重建过标题栏。

## Permission Dock 与 D6 恢复

Task 10 把规范 `.gperm.dock` 紧贴放在 D1 composer 上方。它精确展示 Core approval 的
risk、target、allowed scopes、reason、input preview、expiry、default action 与 audit id。
Once、Session、仓库 allowlist 与 Deny 只映射到 `RespondToApproval`；Always 与 Edit 以
`GUI-CORE-003` 保持禁用（契约请求 `GUI-CORE-019`），因此设计中的 `Shift+A` 组合键仍绑定
在 `repo_allowlist`，而不是绑到一个失效动作上。Plan 模式下的 mutation response 在 transport 前 fail closed。
Command acceptance 不代表成功：只有 owner/request/audit 全部匹配的有序
`ApprovalResolved` 事实才能清除 pending。

D6 是 D1 中央工作面的从属状态，不建立第二套 cockpit shell。Empty、connection、provider、
agent stopped、context overflow、capability、incompatible schema、queue clear 与 event gap
只来自 Core projection 或 CoreClient error。Event gap 的 reconnect 走 CoreClient snapshot
路径，并在已验证 live snapshot 发布前保持 busy。Restart 针对 Core 报告为 failed/cancelled
的那一个 Lane 绑定 ACP session 发送 `RetryAgentSession`，close Lane 针对 Core 发布的那一个
活跃 Lane 发送 `StopLane`；由于 D6 不携带 Lane 选择，目标不唯一时两者都 fail closed。
Inspect 只是对投影中既有事实的本地展开，不触达任何 Core 命令。Checkpoint 控件仍可见但以
`GUI-CORE-003` 禁用（契约请求 `GUI-CORE-018`）；GUI 不伪造 recovery receipt。

## D12 集成闸决策

`批准并合入` 与 `退回原 Lane` 是 D12 仅有的两个变更动作；没有手动 merge 后门，客户端
也不自行解决冲突。两者分别作为 `AcceptMergeGate` 与 `RejectMergeGate` 发出，且都由
`RuntimeContract::decide_merge_gate` 与 `validate_reject_actor` 实际执行的规则推导：

- 批准要求每个必需证据类别均已校验、闸策略要求时存在独立验证方、不存在仍待原 Lane
  复验的冲突退回、actor 与验证方 Lane 匹配（Core 未记录验证方时则等于闸 owner），
  且 reviewed-evidence bindings 与 Core 记录一致；
- 驳回直接拒绝默认 owner，其余只接受验证方 Lane 或闸 owner，并要求非空理由；Core
  将该理由存为闸决策，原 Lane 的 agent 据此工作；
- 携带所请求状态的 `MergeGateUpdated` 才是确认该决策的业务事实。command acceptance
  不等于决策本身。

可用性按 fail-closed 推导：以上每个条件都是 Core 接受该命令的**必要**条件，而非充分
条件。Core 保有 `frontend-contract-v1` 不承载的事实——canonical context item、
permission snapshot、证据质量——因此本投影允许的命令仍可能被拒绝；拒绝理由以
`role=alert` 原样呈现，而不是由 GUI 私有闸模型提前判定。被关闭的控件会标注阻塞代码
（`missing_evidence`、`evidence_not_canonical`、`validator_required`、
`conflict_pending`、`review_not_pending`、`no_actor`、`gate_closed`），而不是直接变灰。

命令离开 host 之前会针对当前 Core view 重新解析该闸，并从 Core 自身记录中重放 actor
与证据 bindings，因此渲染与点击之间消失或已关闭的闸会在本地失败，任何 runtime 身份
或证据哈希都不会从展示文本重建。

## Production bootstrap

`src-tauri` 是 root Rust workspace 中唯一 GUI member，并显式声明独立
`0.1.0-rc.3` 版本。`GuiCoreAdapter`、其 D4 adapter extension 与
`RuntimeProjection` 是 production source 中仅有的 Core contract 边界模块；
`GuiPreferences`、`WorkspaceSelection`、
`ComposerDraft` 与 `TranscriptViewport` 只保存 presentation state。关闭窗口只会丢弃
注入的 client，不会发送 mutation。

## Locale 与外观

Production webview 读取 `RuntimeViewState.snapshot.ui_preferences` 的 transport-safe
投影。只有这份 Core resolved state 能设置 document language、skin、effective mode、
density 和 motion 属性。内置 `en`、`zh-CN` catalog 会检查 key 与 placeholder parity；
shortcut、路径和代码保持原样，不进入翻译流程。

有效外观矩阵固定为 8 组：Aurora、Ice、Mono 各自支持 dark/light，Amber 与
Phosphor 仅支持 dark。非法或损坏值采用确定性的安全回退，同时保留 diagnostic。
Tauri CSS adapter 直接 import `docs/viden-design/Viden/tokens.css`。运行
`tools/check-generated-tokens.sh` 会检查 SHA-256、semantic roles、theme/density
矩阵、adapter import 和 generated metadata；production GUI source 不手抄 token 值。

Preference 控件保留未保存的内存 draft。Save/restore 使用
`SetUiPreferences` 或 `ResetUiPreferences`：GUI 不写 browser storage、文件、config，
也不建立私有 preference authority；只有 `UiPreferencesUpdated` 提供新的 resolved
projection 后才能改变渲染状态。可用性来自握手 capability
`ui.preference_persistence`（`preferences_available`）；客户端不自定义更细粒度的
preference capability。

默认原生 binary 在未显式设置 `VIDEN_GUI_WORKSPACE` 时不预绑工作区。D1 Welcome 打开
系统文件夹选择器，`LocalCoreHost` 在注入的 frontend-safe `CoreClient` 背后构造并持有
runtime。真正的 host/bootstrap 失败显示 D6 disconnected，不进入 D11。D11 adapter 仍
需要注入 frontend-safe `CoreClient`。GUI 不直接导入 runtime ownership，也不自行构造
`SessionEngine`/`RuntimeSupervisor` 或增加私有 reducer。
Task 6 现已负责 resolved locale/appearance projection 与未保存 draft contract；Task 7
负责 D11，Task 9 负责 D1。

## rc.3 视觉、元数据与 bundle 门禁

Task 11 新增 framework-neutral 组件画廊，以及面向 D1、D11、D4、D6 和 gallery 的
deterministic pairwise case inventory/DOM contract。它枚举中英文、全部有效 skin/mode
组合、3 档 density、system/reduced motion，以及桌面、窄屏和放大字体要求。当前已复核
视觉证据包含代表性 desktop gate 和精确尺寸的 D1 同状态 QA；gallery、窄屏与放大字体
capture 仍明确标为 partial。D1 的 pass/fail 视觉 QA 使用独立 canonical-state design
reference 与 production canonical capture 对比；旧 accepted desktop cockpit 截图仅作为历史视觉
lineage 保留。

机器可读的可访问性、有界本地性能记录、Browser-controlled 同状态截图、side-by-side QA、
精确方法和明确的原生审计/profile skip 都在
[evidence/0.1.0-rc.3](evidence/0.1.0-rc.3/README.md)。active manifest 与
immutable rc.3 snapshot 记录相同证据路径并保持逐字节一致。macOS `.app` bundle
只是本地构建产物；未安装、签名、公证、发布、打 tag 或 release。
