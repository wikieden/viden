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
| GUI 组件版本 | `0.1.0-rc.4` |
| 最低 Core 版本 | `0.3.5` |
| 支持 frontend schema | `[1]` |
| 共同分支基线 | `3a7740ea72e58f4a22248a80f9e7324c49bb0f73` |
| Core 最终 checkpoint | `f7fe1b31dfb237e4062209767a7051c2b2c68b93` |
| Core code checkpoint | `1cec82185bbe860d6b8536a63741bc01f1edf2f6` |
| 合同 payload | `5bd2b80b0953f4194d082940a7b9164c7231ca2d` |
| 规范 D1 fixture | `d1-main-cockpit.json`，SHA-256 `05ac25909beaa84942a0468d2ae2d8058e7348bd0bd2b7bc86d6fd344fe69439` |
| 必需 Core capabilities | 15 项冻结能力加 additive extension capabilities，包括 `runtime.cockpit_context_v1` |
| 内置 locale | `en`、`zh-CN` |
| 外观系统 | 5 套 skin、8 组有效 skin/mode、3 档 density、3 种 motion |

当前机器可读 manifest 是 [release-manifest.toml](release-manifest.toml)。
不可变 rc.4 快照是
[manifests/0.1.0-rc.4.toml](manifests/0.1.0-rc.4.toml)；此版本 checkpoint
下两者必须逐字节一致。更早的 alpha、beta、rc.2 与 rc.3 快照继续作为历史证据保留，
不会被重写。`rc.4` 新增 DiffReview 与 EvidenceView 两个界面以及 `0.3.3` 的四项
Core capability；它的 `[evidence].root` 有意仍指向 rc.3 目录，因为本次 checkpoint
没有产出 `0.1.0-rc.4` 验收目录，指名一个不存在的目录比指名一个真实存在的更糟。

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
| 打开项目 / D11 接入 | 原生文件夹打开，以及 project probe、provider health、config preview/confirm、credential handles | `LocalCoreHost::open_workspace` 已提供可信文件夹重绑，`runtime.recent_work` 已响应 `QueryRecentWork`；Core 不发布首启接入信号，也没有仓库克隆命令、项目脚手架命令与并发多根托管 | Welcome 直接使用原生选择器和 host rebind，并列出 Core 的最近项目；标题栏项目选择器提供带确认的切换。D11 只作为项目内显式配置流程，不接管打开文件夹。入口为 `?screen=d11` 与项目选择器的「配置此项目…」行；shell 不会自行重定向进入 |
| D4 Lane 创建 | typed role、route、gate strength、mutation policy、target、budget、worktree preview、lane receipt | 已有 `PreviewStarterLane`/`CreateStarterLane`、Core 解析 preview、invalidation、approval、精确 receipt 与 `runtime.starter_lane_preview` 广告 | Task 8 渲染四步复核流程；连接旧版 Core 时仍以可见 unavailable 和零发送 fail closed |
| D1 驾驶舱 | 无项目欢迎中心、零 Lane 项目驾驶舱、activity/lane rails、streaming transcript/tool rows、Environment、Live Work、composer、evidence/context/cost facts | stream/tool/approval/queue/task/lane/owner/evidence/context/cost/preferences/recent-work facts 已有；diff/apply、稳定 audit timeline、可操作 Lane recovery 与并发多工作区托管尚不完整 | 未绑定 host 才显示 Welcome；已绑定空项目仍留在 D1 并提供“新建 Lane”；Lane 侧栏把 Lane 收拢在 Core 托管的那一个项目分组下（`GUI-CORE-023`）；实时工作从 `RuntimeViewState` 渲染 |
| Permission dock | scoped approve/deny、risk、target、expiry、default action、audit id | `ApprovalRequestView` 和 `RespondToApproval` 已有 | 可经 Core 使用；GUI 不得直接执行 tool |
| D2 决策中心 | 跨 Lane 的统一决策队列：闸审批、lane 问询、契约确认共用「上下文 / 证据 / 动作栏」一套卡片骨架 | `pending_approvals` + `RespondToApproval`、`review_requests` + `DecideReview`、`contracts` + `ConfirmContract` 已有；审批的结构化 diff 与待确认契约事实缺失 | 入口 `?screen=d2`；闸、契约与评审决定都发出 Core 命令，评审裁决可携带可选评审意见，且只由 Core 发布的有序 `ReviewRequestUpdated` 确认；Core 会拒绝的评审保持禁用并标注 `D2-REVIEW-SETTLED` 或 `D2-NO-REVIEWER-ACTOR`，审批 diff 以 `GUI-CORE-012` 声明不可用，契约分组以 `GUI-CORE-013` 标注为已决历史，且两个契约裁决都以同一编码禁用并标注，因为 Core 会拒绝对已记录的契约再做一次裁决 |
| D10 Lane 监视器 | 跨项目每条 Lane 一张卡：门控强度、状态、进度、证据、成本可计量性与「等你」计数 | `lanes`、`lane_runtime_owners`、`tasks`、`agent_sessions`、`latest_evidence`、`AgentLaneRecord.run_stats` 已有；有序历史来自审计时间线而非视图状态 | 入口 `?screen=d10`；只读，门控强度取自 `AgentLaneRecord.gate_strength` 而非 agent 标签，未绑定 Lane 不显示项目，无 Core 任务的 Lane 不显示进度；成本不可计量路由（`AgentRoute::cost_meterability`）会被标记，并展示 Core 记录的有界运行事实而非推断成本——Core 未观测到运行时该组事实缺席而不是补零，退出码缺失时明确标为未知；事件流是 Core 追加式审计时间线（`QueryAudit` -> `AuditPageLoaded`，limit 50，不加作用域以覆盖每个项目）的一页有界 newest-first 记录，逐行渲染稳定 id、原样的点分 action key、所属项目与 Lane、以及时间戳；缺少 `runtime.audit`、读取中、被拒绝、已回答但为空，保持四条不同的文案 |
| D12 集成闸 | 冲突横幅、闸策略、退回原 Lane 的恢复时间线、冲突 hunk、合入后回滚，且不提供手动 merge | `merge_gates`、`conflict_bounces`、`reverts`、`check_runs`、`AcceptMergeGate`、`RejectMergeGate` 已有，且 `runtime.conflict_content` 发布 `ConflictBounce.content` 与 `LaneConflictView.content` | 入口 `?screen=d12`；`批准并合入` 与 `退回原 Lane` 各自发送对应 Core command，只有满足 `decide_merge_gate` 实际执行的规则时才开放，否则标注阻塞代码；时间线与回滚按选中闸限定；每条冲突记录把被拒 hunk 画成 OURS 与 THEIRS 并排、补丁原像作为第三条折叠条 —— 绝不是合并结果（`GUI-CORE-015` 已采纳） |
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

Shell 通过 `?screen=d11` 与项目选择器中位于已打开项目旁的「配置此项目…」行进入 D11。
该入口原本挂在「新建 Lane」弹层的 `Full setup…` 上；`0.3.4` 驾驶舱中央批次把那个动作
移到了它本应归属的 D4 Lane 向导，因为为了创建一条 Lane 而要求操作者配置整个项目是错的
问题。D11 配置的是*项目*——probe、`viden.toml`、凭据、starter Lane——因此入口在项目界面。
`d11_poll` 同时充当入口读取与等待，因此重新进入会
继续等待仍未收到 Core 回执的命令，而不是重新开始；D11 收集的起始 Lane 种子交给 D4，
由 D4 拥有 preview/confirm 回执循环。没有自动跳转进入 D11：Core 不发布首启接入事实，
客户端只能凭空编造一个。

## D4 起始 Lane 复核创建

Task 8 从项目驾驶舱的“新建 Lane”进入 D4，每次只复核一个 seed。创建前 Cancel/Skip
会零 mutation 返回 D1；创建发出后，界面只提供精确 Core
approval 的 allow/deny，不伪造 cancel command。每个完整
`StarterLaneCreated.receipt` 推进一项，最后一个 receipt 发出 typed D1 导航请求，并聚焦
最后创建的 Lane。

### 四个步骤

向导的步骤即设计稿自身的四步（`GUI/pages/Viden - D4 Lane创建流程 (GUI).html` 的
`STEPS`），每一步只渲染自己的字段，而不是把四个标题摞在同一张表单上：

| # | 步骤 | 渲染什么 | Core 未建模的部分 |
| --- | --- | --- | --- |
| 1 | 角色与工位 | 三个 `StarterLanePreset` 角色、Lane 名称与分支，以及 Core 解析出的 worktree 与基线修订（只读） | 设计稿的七角色领域包（`D-ROLES`）不在本契约上，因此只提供 Core 的三个名字 |
| 2 | 选择 agent | Core 发布的适配器（只读），标出「新建 Lane」弹层中的选择，并附解析出的 route | `StarterLaneRequest` **不携带 agent 绑定**，因此创建 Lane 不会启动 Agent 会话；该步直说这一点，而不是提供一个到不了 Core 的选择——本批次起草但未编号的契约请求，因为 `0.3.4` 的登记表是冻结的 |
| 3 | Skill 包 | 渲染该步，并说明其不可用与原因 | `frontend-contract-v1` 任何地方都不发布 skill、包或上下文注入事实，请求也没有对应字段——同一份起草请求 |
| 4 | 闸与执行目标 | Core 自己的解析：route、gate strength、target、budget、worktree、基线修订与 mutation policy | 请求不携带执行目标，预览恒解析为 `local`；远程目标在 D9 中设计，不在本契约内——该步说明这一点，而不是画一个选择器 |

弹层带来的任务显示在第 1 步，并注明它**不属于**创建命令：`StarterLaneRequest` 只携带
Lane id、预设、分支与 worktree，因此任务会在 Lane 打开后作为操作者的第一条消息发出——
这正是紧凑版「新建 Lane」的做法（`create_starter_lane`，然后 `submit` /
`start_agent_session`）。

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
`StoreCredentialHandle` 路径以 `GUI-CORE-026` 明确禁用。D11 的历史面板现与下文的
Welcome 中心和项目选择器渲染同一份 Core `QueryRecentWork` 清单，以
`runtime.recent_work` capability 为门（旧的 `GUI-CORE-007` 临时文案已退役）；
这些界面都不扫描 local storage、JSONL 或 SQLite。
project switching 现通过 Core-owned `LocalCoreHost::open_workspace` 完成；
安全 raw credential staging 仍是 `GUI-CORE-026` 的未完成部分。

## D1 流式驾驶舱

Task 9 让 D1 成为常驻应用外壳和唯一主工作面。它先显示 D6 连接中/断开状态，或
host-owned 无项目欢迎页。已绑定但没有 Lane 的项目仍是 D1；D4 只从项目内创建入口进入，
D11 只用于显式配置。Activity rail、Lane rail、分标签的上下文坞、Live Work、
transcript/tool rows、排队状态、evidence 与 composer 都是 Core 最新
`RuntimeViewState` 的 transport-safe 投影。Webview 只持有焦点、draft、布局、有界行窗口
与滚动锚点，不解析显示字符串，不持久化第二套 workspace 模型，也不会把 command
acceptance 当作业务成功。有序 Core 刷新会原位更新 activity 与 Lane rail，让它们的
hover 根节点在易变的 Lane/Agent 状态变化期间保持挂载；这样既不会隐藏最新 Core 事实，
也不会让浮动侧栏反复闪烁。Activity rail 中每一个可用槽位背后都有真实动作；没有可用动作
的槽位一律禁用，而不是既可点击又无响应。Rail 本身就是驾驶舱的路由器，路由表、中央视图、
返回路径与 Lane 侧栏双模式见下文[导航外壳](#导航外壳)。

### 中央面板

中央面板即设计稿的 `.center` 列：`.tabstrip.lanebar` Lane 标签条位于滚动转录之上。
标签条为**驾驶舱投影列出的每条 Lane 渲染一个标签**——状态点、Lane id、Lane 名称，以及
该 Lane 自己记录的分支——再加上 Core 为其绑定的 agent、末尾用于打开「新建 Lane」弹层的
`＋`，以及设计稿的 `.tabmeta` 槽（项目、Core 发布的上下文预算、已解析的工作模式）。

三处“缺席”是有意的。未记录分支的 Lane 就不显示分支：工作区的分支是另一件事实，绝不
顶替 Lane 的（C5 的 `lane_sources` 正是承载每-Lane worktree 来源的接缝）。预算与模式
放在末尾的 meta 而不是每个标签上，因为驾驶舱投影按 Lane 限定——`contextDock.context`
与 `statusbar.workMode` 描述的是该次读取所针对的那条 Lane，把它们印在别的 Lane 上就是
挂错名字的数字。没有 Agent 会话的 Lane 按内置 runtime 命名，与 Lane rail 的做法一致。

完全没有 Lane 时标签条仍在，显示 rail 自己的「No Lanes yet」文案与 `＋`。中央**视图**
（DiffReview、EvidenceView 或五个 D 屏之一）会替换整列——这是旗舰稿自己的切换方式——
因此这些视图不画标签条，而是在各自头部说明作用域。

点击标签经 `selectLane` 选中 Lane，与 Lane rail、命令面板同一条路径。`⌃⇥` / `⌃⇧⇥`
在已投影的 Lane 之间循环，这正是设计稿键位表绑定给「下一条 / 上一条 lane」的组合键；
当 IME 组合态或任一浮层占有键盘时它们让位，因为把对话从操作者正被要求作出的决定下面
挪走，是这个组合键绝不能做的事。

**工具块。** 工作区改动与检查运行都渲染为设计稿的 `.tool` 块——`.th` 头部加 `.tb` 主体。
改动的主体是共享 hunk 渲染器（`diff_rows`），与 DiffReview、permission dock、D2 用的是
同一套行，并且**默认折叠**：转录是自上而下读的，中间夹一段长 hunk 会把对话埋掉。检查运行
不折叠，其主体是设计稿的三条 `.testrow`——状态、Core 报告的失败位置（若有）、结果——
而失败那行正是这个块出现的原因。

头部只说 Core 说过的话。Core 不会为工作区改动发布产生它的工具，因此名称槽显示改动类别——
除非当前 approval 自己的 `decisionContext` diff 恰好覆盖这个路径，那是设计稿的
`write_file` / `edit_file` 成为 Core 事实而非猜测的唯一情形（有序 typed tool-call 行仍是
`GUI-CORE-009`）。闸状态芯片只依据同一份证据出现：Core 未附上下文的 approval 在这里不闸
任何东西。没有行的改动保持它一直以来的 patch-或-不可用主体。

**专注模式（`⌘.` / `⌃.`）。** `D-SIDEBAR` 的覆盖条款：「focus 专注模式覆盖此偏好 →
两侧强制 hover 浮窗(退出恢复)」。两侧面板各自移到已登记的 `.edgewrap` 热区之后——左侧是
Lane 侧栏，右侧是 Context Dock，两者用同样的约 700ms peek 延时——转录拿走宽度。
Activity rail 留下，因为它是驾驶舱的路由器，而消失的路由器就是死路。

该标志**遮蔽**而非写入 `laneSidebarMode`，这正是「退出恢复」无需保存副本的原因：退出时
操作者选择的 pinned 列自然回来。它与侧栏模式、状态栏环境项一样是内存态，但与那两者不同，
它不是偏好——它是操作者维持几分钟的姿态——因此**不**属于 C5 的 `UiLayoutPreferences`。
标题栏为它提供设计稿的 `IFocus` 控件，位置就是设计稿 `.tbtools` 中的位置，
`aria-pressed` 报告当前状态。

“新建 Lane”会打开一个紧凑的锚定弹层，默认选中内置 Viden Agent，并包含已发现的 ACP
Agents、品牌身份、任务 draft、Core 投影的 eligibility/probe 诊断，以及只作呈现的
isolation 提示。“完整设置…”在弹层自己的 draft 上打开 D4 Lane 向导——任务用弹层预览的同
一个 `vd/<slug>` 命名 Lane 与分支，所选 agent 在向导的 agent 步骤上被标出。取消向导会
带着 draft 回到驾驶舱并重新打开弹层，因此绕道完整表单一次也不会让操作者丢掉已输入的内容。
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
各 provider 的 API key 标记（凭据仍属 GUI-CORE-026 范围）、Requests 卡片
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
更改时再加一个 dirty 标记。旁边的 `.gitops` 块含两个 chip：`↑ahead ↓behind` 自
`runtime.operator_git` 起是一个操作者控件（push/fetch 规则见 DiffReview 一节；在没有
动作端口的宿主上它仍退回 `role=status` 只读芯片）；`⎇ N 个工作树` 点击进入 D10 Lane
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
merge gate 时，右侧显示待审闸分段——状态栏唯一的可操作元素——点击打开驾驶舱内的 D2
决策队列。状态栏起始端的齿轮打开 `D-STATUSBAR` 配置弹层，覆盖六个环境段，
见[导航外壳](#导航外壳)。

Transcript 最多保留 240 行。离开最新输出边缘后会设为 `follow_latest=false`、保留当前锚点，
并递增可见的新输出计数，而不会强制滚动。Rust 与 webview 测试覆盖 10,000-event burst、
50,000 行、resize/idle 读取、CJK composition、多行 paste/undo、键盘遍历、ARIA region 与
可见焦点。`evidence/0.1.0-rc.3/` 下的 Browser-controlled 证据包含 Aurora
dark/regular 英文、Ice light/regular 英文、Aurora dark/regular 中文、compact density
和 responsive drawer 状态，并包含一个由 `d1-main-cockpit.json` 填充的独立同状态设计
reference。它还包含一个补充 Context Dock bottom-state capture，用于证明下方事实可通过内部
滚动到达。checkpoint 捕获与恢复（渲染为 `GUI-CORE-003`，契约请求 `GUI-CORE-018`）
仍是明确的 unavailable fact；D1 不会伪造成功占位。`audit`、`diff` 与 `apply` 三行已
移除：Core 的审计时间线、`runtime.structured_diff` 与 `runtime.operator_git` 已关闭
这些缺口，再保留只会是关于 Core 的过期陈述而不是事实。面对真正没有这两项能力的 Core
构建，`diff` 与 `apply` 仍会出现，并直接指名那项能力。

### 上下文坞

右侧面板即设计稿的 `.rail.dock`：`.docktabs` 标签条位于单个 `.dockbody` 面板之上。
标签条按设计稿 `ALLTABS` 的顺序渲染**全部六个**标签——环境、文件、终端、源码、对比、
文档。其中三个可用，另外三个禁用并直接写出打不开的确切原因：终端属于设计稿的召唤坞，
登记时带 roadmap 标（`D-RAILNAV` ⑥），且 Core 没有 PTY 事实；源码需要
`runtime.workspace_file_reads`，本版本尚未消费；文档则完全没有 Core 工作区文档事实。
三者都不隐藏——消失的标签会让坞看起来已经完工——也都不是「可点击却无响应」。

当前打开的标签只存在内存中，且有意不持久化：它是操作者看一眼时采取的姿态，不是偏好，
因此既不写 `localStorage`（前端契约规定 Core 是唯一偏好权威），也不属于 C5 的
`UiLayoutPreferences`。专注模式仍会把整个坞移出网格、收到右缘热区之后，见
[导航外壳](#导航外壳)。

**组合键。** `⌥⌘E`、`⌘P`、`⌘D` 分别打开环境、文件、对比，正是设计稿给这三个标签的
绑定。设计稿六个键中有三个未绑定，每一处都是冲突而非遗漏：`⌘J`（终端）与 `⌘/`（文档）
会承诺打不开的面板；`⌘O`（源码）已经是 Welcome 的文件夹选择器，先前的绑定保留该键。
`⌘P` 只绑 meta 键，因为 `⌃P` 是命令面板的作用域键并予保留。坞组合键同时会**显示**它
刚切换的坞——专注模式下峰显热区，窄窗口下打开抽屉——因为改变屏幕外的东西等于什么都
没改。

**环境面板。** 按设计稿顺序排列的可折叠 `.envsec` 节，每一节背后都有一条具名事实：

| 节 | 背后的事实 | 事实缺失时 |
| --- | --- | --- |
| 环境 | `environment`（提供方、模型、模式、权限、token、成本） | 与此前一致，逐项显示破折号 |
| 变更 | `+n −m` 取 `contextDock.source.added`/`.deleted`；文件行取 `QueryWorkspaceDiff` 条目，逐文件 `+a −b` 仅在结构化 diff 给出时显示 | 四句互不相同的话：未绑定 host、Core 未发布 `runtime.structured_diff`、读取进行中、Core 回答为空（「没有变更」），或 Core 自己的拒绝原文 |
| 本地 | Core 发布了 C5 `lane_sources` 行时取所选 Lane 自己的行，否则取工作区采样——并**说明当前显示的是哪一个** | 「没有可用的来源事实。」 |
| 提交或推送 | `runtime.operator_git` 加上 Core 发布的 owner 与一份 source；该行路由到 DiffReview 的提交栏并聚焦其消息框，提交栏未绘制时聚焦顶栏 sync 芯片 | 禁用，并把原因**显示在屏幕上**，不只放在 tooltip 里 |
| PR 状态 | 没有——`frontend-contract-v1` 不发布 forge 状态 | 始终显示说明该缺失的句子 |
| 上下文 | `contextDock.context`，按设计稿 `.envctx` 条渲染 Core 的已用/上限与占硬上限的百分比 | 「没有可用的类型化上下文预算。」；成本为空时保留 `D-BUDGET-BLIND` 的具名计量盲区，而不是给出估值 |
| 子代理 | 没有——内嵌树已延后到 fleet 家族（`D-RAILNAV` ④） | 一行说明该延后；绝不编造一棵树 |
| 来源 | `contextDock.source` 计数，与此前一致 | 「没有可用的来源事实。」 |
| Lane 执行身份 | 精确的 Core owner 绑定，与此前一致 | 「没有 Agent owner」 |
| MCP | Core 自己的 MCP 行——本契约上它**只可能**是 `Unavailable` 加一个 `detail_key`；坞把该键本地化，绝不显示「已连接」 | 说明 Core 未发布 MCP 事实的句子 |
| LSP | `contextDock.services` 中 kind 为 `lsp` 的行 | 「没有可用的服务。」 |
| Todo | `contextDock.checklist` | 「没有可用的任务清单。」 |

点击变更行会在 DiffReview 中打开**该文件**。这是一条路由，不是进入该视图的第二个入口：
`navigate("review", path)` 先写入驾驶舱自己的 `reviewSelectedPath`，DiffReview 再把它
与 Core 回答的 page 对照解析——下一页不包含的路径就是未被选中而已。评审已打开时点击另
一行会移动选择，而不是关闭视图。

**文件面板。** 文件树来自 `QueryWorkspaceFiles`，操作者每展开一个目录读一页——`prefix`
是以 `/` 结尾的目录，根目录则完全不带 prefix。一次读完整棵树只会得到一页有界结果，且
无法越过该上限；任何一页 `complete: false` 都会在发生处渲染 Core 自己的截断说明。文件行
只做选中：检视面板写出文件名，并让 Open 保持禁用、写明 `runtime.workspace_file_reads`
——G7 会把它变成真正的读取。`WorkspaceFilesQuery` 不带 target，因此该面板始终是工作区根
并如实说明；按 Lane 限定的清单是一条 Core 契约请求，不是客户端可以从路径合成的东西。

**对比面板。** 对同一 target 的 `QueryWorkspaceDiff`，每个变更文件一个条目，**默认折叠**：
该面板回答「是哪些文件」，而 300px 列里的一墙行什么也回答不了。展开后绘制共享的
`diff_rows` 主体——与 DiffReview、Permission Dock、D2 相同的行——并在自己的容器内横向滚动。
选中文件后检视面板显示文件、差异与「在评审中打开」；暂存与还原渲染为禁用并具名，因为
Core 未发布逐文件的暂存或还原命令。

**一次 diff 读取，三个读者。** 变更节、对比面板与 DiffReview 都渲染同一 target 的同一份
`QueryWorkspaceDiff` page。由于坞在未被打开时就要列出变更文件，驾驶舱在挂载时发出这一次
读取——Core 以非交互方式裁决它，返回拒绝而不是让客户端停在审批提示前——打开评审时复用
该 page，而不是再跑一次 git。过期重查规则未变，仍然只在评审占有中央面板时生效。


## 导航外壳

Activity rail 是驾驶舱的路由器（`D-RAILNAV`），它指名的每一个目的地都渲染在驾驶舱**内部**。
`0.3.4` 计划按 D1 旗舰自身的行为裁定了 `D-RAILNAV` 留下的那个问题：D2、D10、D12、D13、D14
过去是整窗屏幕，会丢掉标题栏、两条 rail、上下文坞、composer 与状态栏，而且除了浏览器式
路由之外没有返回路径；它们现在是覆盖在转录之上的视图，与 DiffReview、EvidenceView 一致。

**一张路由表。** `src/components/activity_rail.ts` 导出 `D1_RAIL_ROUTES`——一个既是渲染
顺序、又是路由映射的有序数组。它取代了「槽位清单 + 以文案 id 为键的独立路由表」这一对会
各自漂移的结构；一张表让「每个已登记目的地都能从 rail 抵达」成为可检查的事实而非断言。

| # | 槽位 | 打开 | 字形 | 可用条件 |
| --- | --- | --- | --- | --- |
| 1 | 对话 | 转录；并把焦点交还 composer | `chat` | composer 可聚焦 |
| 2 | Lane 列表 | 切换 Lane 侧栏（见下） | `lanes` | 已绑定工作区 |
| 3 | Diff 评审 | 中央视图 `review` | `review` | `runtime.structured_diff` |
| 4 | 证据 | 中央视图 `evidence` | `evidence` | `runtime.evidence_reads` |
| 5 | 决策 | 中央视图 `d2` | `decide` | 已绑定宿主 |
| 6 | Lane 监视墙 | 中央视图 `d10` | `diagnostics` | 已绑定宿主 |
| 7 | 集成闸 | 中央视图 `d12` | `worktree` | 已绑定宿主 |
| 8 | 舰队看板 | 中央视图 `d13` | `fleet` | 已绑定宿主 |
| 9 | 审计时间线 | 中央视图 `d14` | `brief` | 已绑定宿主 |
| — | 设置 | 设置浮层，位于 spacer 之下 | `settings` | 已绑定宿主 |

每个字形都来自已登记的 `GUI/gui-icons.jsx`，没有任何槽位自行画图。任意时刻恰有一个槽位
被标记 `aria-current="page"`，而且由这张表决定：只有当没有任何目的地是当前视图时，「对话」
槽位才是当前——因此 rail 不可能同时标记两个。

**缺失要被指名，而不是被隐藏。** 缺少 Core 能力的目的地既禁用**又**在可访问名称里（而不是
只在 tooltip 里）写出所缺能力——`Diff 评审 — 不可用：Core 未发布 runtime.structured_diff`。
审计时间线是唯一永不被能力拦住的目的地：没有 `runtime.audit` 时 D14 以原始事件回放打开，
那是同一个问题的另一种视图，因此槽位提前说明，而不是让操作者点完才发现。唯一的徽标是
「决策」槽位上的待处理计数，取自 `statusbar.pendingGateCount`——Core 已经发布的数字。其他
槽位都不带徽标，因为它们没有 Core 发布的计数，而 0 会被读成「没有待办」，真相却是「没人计数」。

**中央视图。** `centerView` 为 `transcript | review | evidence | d2 | d10 | d12 | d13 | d14`，
驾驶舱把它写在自己 frame 的 `data-center-view` 上；`root.dataset.route` 仍是 `d1`，因为
中央视图是关于驾驶舱的事实而不是一条路由。五个 D 屏通过唯一接缝
`D1RenderOptions.secondaryViews` 挂载：外壳负责**装什么**（每个屏幕都是属于客户端边界的
Core 读取），驾驶舱负责**装在哪**——中央面板、四周 chrome、Close 控件与 `Esc`。屏幕渲染
函数本身未被改写；它们接收一个容器并填充它，只是容器从窗口根移进了 D1 的中央面板。正因如此
它们既有的 Rust 投影、`onNavigate` 语义与 vitest 覆盖全部继续通过。宿主节点跨有序 Core 刷新
保留，因此已挂载的屏幕保住自己的选中项、筛选与模式，而不会在每次唤醒时被重建并重新读取
Core。被拒绝的读取会以 `role=alert` 原样呈现 Core 自己的措辞来代替屏幕；面板绝不留空，
因为留空读起来是「这里没有东西」，而不是「Core 不肯回答」。

**深链接。** `?screen=d2|d10|d12|d13|d14` 打开驾驶舱并切换其中央面板，因此链接产生的
chrome、选中 Lane 与返回路径都与从 rail 进入完全一致。`?screen=d4` 与 `?screen=d11` 仍是
整窗流程：它们确实会替换驾驶舱。未绑定工作区时仍然是 Welcome 优先——深链接只在 Core 已
给出驾驶舱投影之后才被读取。

**所有跨视图链接都落在驾驶舱内。** 命令面板的 `#` 行（合并闸到 D12、问询到 D2）、状态栏的
待审闸段、标题栏的 worktrees 芯片、D2 与 D12 的审计轨迹链接、EvidenceView 页脚的「打开审计
轨迹」，以及 D10 卡片通往决策中心的动作，都走同一个函数——驾驶舱的 `navigate`：能承载的
目的地切换中央视图，不能承载的则回落到外壳自己的窗口路由。各自携带的作用域都被保留：
D14 的 `kind:id` 审计作用域仍经 `parseAuditScope`，D12 的闸 id 与 D2 的决策 id 仍是 Core
自己的 id，并且由屏幕在渲染前重新读取。

**返回路径。** 每个非转录视图都在 DiffReview 放置 Close 的同一位置——视图头部的尾端——带一个
Close 控件；再次按下同一个 rail 槽位同样返回转录。`⌘G` / `⌃G` 打开决策并以同样方式切换，
与 `⌘R`（Diff 评审）、`⌘E`（证据）并列。`Esc` 只在一个地方处理，并带显式优先级，因为这个
顺序**就是**契约：

1. IME 组合态完全占有该键；
2. 打开的浮层占有它——设置面板、命令面板、composer 控件弹层、新建 Lane 弹层、项目选择器或
   permission dock。正在请求操作者作出的决定高于任何导航；
3. 浮动 Lane 侧栏的 peek，屏幕上最短暂的东西，也是 `D-SIDEBAR` 把 `Esc` 绑上去的那个；
4. composer 的取消当前回合绑定，它停止的是真实工作而不是移动一个视图。它的可视入口——Live
   Work 条上的取消——位于转录内部，因此二者实际上从不冲突；
5. 中央视图的返回路径；
6. 最后才是专注模式——它既不遮蔽决定也不中断工作，因此以上每个界面都先拿到该键。

### 组合键

驾驶舱绑定的全部窗口级组合键，集中于此。非 macOS 上 `⌘` 即 `⌃`。每一个在 IME 组合态
占有该键时让位；除 `⌘K` / `⌃P` 外，其余在模态浮层持有焦点时也让位。

| 组合键 | 作用 | 何时让位 |
| --- | --- | --- |
| `⌘K` | 命令面板（切换） | 设置面板、新建 Lane 弹层或控件弹层持有焦点 |
| `⌃P` | 预置 `>` 作用域的命令面板 | 同上 |
| `⌘L` | 新建 Lane 弹层 | Core 判定该工作区无法承载 Lane——与命令面板行相同的失败关闭条件，并带 Core 自己的说明 |
| `⌘R` | DiffReview（切换） | 未绑定 host，或 Core 未发布 `runtime.structured_diff` |
| `⌘E` | EvidenceView（切换） | 未绑定 host，或 Core 未发布 `runtime.evidence_reads` |
| `⌘F` | 聚焦证据搜索框 | EvidenceView 未占有中央面板 |
| `⌘G` | 决策队列（切换） | 未绑定路由与次级宿主 |
| `⌥⌘E` | 坞的环境面板 | 浮层持有焦点 |
| `⌘P` | 坞的文件面板（只绑 meta 键；`⌃P` 仍属命令面板） | 浮层持有焦点 |
| `⌘D` | 坞的对比面板 | 浮层持有焦点 |
| `⌘.` | 专注模式（切换） | 浮层持有焦点 |
| `⌃⇥` / `⌃⇧⇥` | 下一条 / 上一条 Lane | 浮层持有焦点，或没有可切换目标 |
| `⌘O` | 文件夹选择器 | 仅在无项目 Welcome 上绑定——它保留设计稿给坞「源码」标签的那个键，而该标签打不开 |
| `Esc` | 上文优先级列表 | — |

**Lane 侧栏双模式（`D-SIDEBAR`）。** `floating` 是该决策的默认值，也是驾驶舱的默认值：侧栏
把横向空间让给转录，藏在贴着 activity rail 右缘、带 `.edgehint` 提示条的 12px 热区之后。
指针移入即滑出，移出后按决策的约 700ms 延时收起。选中 Lane 会立即收起——它已经完成了
任务——`Esc` 亦然。键盘路径是 Lane rail 槽位，它切换 peek，因为 12px 的窄条并非人人都能
命中的指针目标。

`pinned` 是**真正的布局列**，而不是同一个浮层加一个标志位：驾驶舱主体增加第四条
网格轨道，宽度取设计默认值 `--rail-left`（218px），侧栏不再绝对定位。在该模式下 Lane 槽位
切换的是这一列本身——这是不离开该模式又把宽度还回去的唯一办法——而 `Esc` 不动它：`Esc`
属于那个短暂的 peek。宽度不大于 1100px 时外壳本就塌缩为两条轨道，因此 pinned 在那里回退为浮层，
而不是去占转录没有的宽度。

唯一的切换入口是 activity rail 底部、设置齿轮正上方的 pin 按钮，正是 `D-SIDEBAR` 指定的
位置；其 `aria-pressed` 报告的是当前模式而不是它将执行的动作。这里刻意没有第二个入口：
设计稿曾画过的侧栏顶部 `.pinbtn` 从未被渲染，其 CSS 已于 2026-07-02 删除。两种模式下
侧栏组件是同一个节点，只有宿主不同，这正是该决策自身的规则。

Token 中记录的 176–360px 拖宽**未**实现；本批次 pinned 列固定为 218px。

**状态栏配置齿轮（`D-STATUSBAR`）。** 状态栏起始端的齿轮打开设计的 `.sbcfg` 弹层，列出六个
**环境**段——`CONTEXT`、`EVENTS`、`LATENCY`、`TOKENS`、`DIAG`、`REQ`——各带一个勾选框。被钉住
的那一半不在列表中、也无法关闭：`MODE`、`PERM`、`LANE` 与待审闸芯片是身份与动作，而
`D-STATUSBAR` 按「可操作性」而非紧急程度切分状态栏。未注入配置端口挂载的状态栏根本不渲染
齿轮，而不是渲染一个点了没反应的齿轮。

**两处内存接缝。** Lane 侧栏模式与状态栏环境段可见性是本批次保存在内存中的呈现状态，并且
**刻意不做持久化**。前端契约规定 Core 是唯一的偏好权威，因此设计原型的 `localStorage` 键
（`vd-leftmode`、`vd-leftw`）就是契约禁止的第二套偏好模型。Core 批次 `C5` 会加入
`UiLayoutPreferences.lane_sidebar_mode`；届时 G7 从已解析的偏好投影读取它并写回，
`D1RenderOptions.laneSidebarMode` 也从调用方默认值变为 Core 的取值。pinned 列的宽度——
token 记录的 176–360 拖宽——是同一条记录的第二个字段。两处接缝在代码里都带有该注释。

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

**Lane 侧栏**有两种 `D-SIDEBAR` 模式（见[导航外壳](#导航外壳)），并且在两种模式下都是
设计稿的工作区浏览器：一个 `.wsroot` 分组头，承载 Core 发布的项目
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

Actions 区带有设计稿自己的 Lane 创建行「Delegate task to new worktree… ⌘L」——创建一条
Lane *就是*把任务委派给一棵新的 worktree，这正是设计稿如此命名的原因。它打开的是 rail
与标签条的 `＋` 所打开的同一个「新建 Lane」弹层，因此创建界面只有一个而不是两个。当 Core
判定该工作区无法承载 Lane 时，该行禁用并带上 Core 自己的诊断；若 Core 根本没有发布工作区
可用性，则给出属于它自己的说法——那不是拒绝，也不能被写成拒绝。

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

「拒绝」还会重定向 composer，遵循设计中拒绝之后的那一状态：提示语变为
「Tell <agent> what to do instead…」并接管光标，让操作者就在原本注视的位置纠正 agent。
当聚焦会话是 ACP 会话时，agent 名取自 Core 发布的 adapter `displayName`，否则使用通用
措辞——绝不从 agent id 猜一个名字出来。这只是呈现，仅此而已。它在拒绝被**派发**时应用，
而不是等 Core 答复，因为它对答复不作任何声明；`feedback` 仍为 `null`：schema 1 没有
feedback 字段（`GUI-CORE-019`），因此纠正内容以操作者的下一条普通消息发出，而不是伪造
一个字段。重定向在下一次提交时、或 Core 发布**另一个** request id 时退场；仍携带刚被
拒绝的那个请求的刷新不会动它，因为那正是命令与 Core 答复之间的常态。

D6 是 D1 中央工作面的从属状态，不建立第二套 cockpit shell。Empty、connection、provider、
agent stopped、context overflow、capability、incompatible schema、queue clear 与 event gap
只来自 Core projection 或 CoreClient error。Event gap 的 reconnect 走 CoreClient snapshot
路径，并在已验证 live snapshot 发布前保持 busy。Restart 针对 Core 报告为 failed/cancelled
的那一个 Lane 绑定 ACP session 发送 `RetryAgentSession`，close Lane 针对 Core 发布的那一个
活跃 Lane 发送 `StopLane`；由于 D6 不携带 Lane 选择，目标不唯一时两者都 fail closed。
Inspect 只是对投影中既有事实的本地展开，不触达任何 Core 命令。Checkpoint 控件仍可见但以
`GUI-CORE-003` 禁用（契约请求 `GUI-CORE-018`）；GUI 不伪造 recovery receipt。

## DiffReview 变更评审

`⌘R`（非 macOS 为 `⌃R`）、标题栏的变更标记、活动 rail 的 `Diff 评审` 槽位与命令面板的
`打开评审` 四个入口都在 D1 中央区打开登记族 `.review > .filetree + .diffpane`。它是
**驾驶舱内的视图，不是路由**：DiffReview 是 `D-RAILNAV` 登记的次级面之一，关闭它回到
对话流而不是导航离开。G3 之后 rail 像对待任何目的地一样路由到它——而且 rail 指名的每个
目的地都是中央视图，因此「rail 是通往独立 D 屏的路由器」已不再是当初那个区分，
见[导航外壳](#导航外壳)。CSS 取自 `GUI/gui-kit.css` —— `D0`
升进正是为这个第二消费者做的镜像；按设计包自己的规则，手抄 D1 内联样式即漂移。

**数据通路。** 经 CoreClient 缝合层发一次 `QueryWorkspaceDiff` ->
`WorkspaceDiffLoaded`，关联方式与 `QueryWorkspaceFiles` 相同：同时只有一次读在飞行中，
页与 `CommandRejected` 都携带精确的 `command_id`，且不存在「以受理兜底」的猜测，因为页
上的 id 是必填字段。默认查询覆盖整个目标、`scope: Both`、不带路径过滤、使用 Core 自己
的字节上限。目标跟随驾驶舱的 Lane 选择 —— 选中某 Lane 即评审该 Lane 的工作树 —— 并由
Core 依 Lane id 解析工作树，因为客户端从不传路径。权限门（非变更的 `git_diff` 工具，
因此该读取在 Plan 模式下仍可回答）、`git status`/`git diff` 的执行、上限与排序都归
Core。客户端不执行 git，也不解析 diff 文本。

**重读规则。** 刷新按钮按需重读。除此之外，视图打开期间每次有序 Core 唤醒都会读一次
宿主的零流量投影；适配器在唯一的事件接收漏斗里统计使两个使 diff 失效的事实 ——
`WorkspaceSourceUpdated` 与 `WorkspaceChangeUpdated` —— 因此即使该事实被别的屏幕的轮询
消费掉，打开着的评审仍能得知。页在读取之后被失效时会立刻显示横幅，并在 400 ms 去抖后
重读，于是一串 agent 写入只花一次有界查询而不是每个事件一次。期间行始终留在屏幕上：
「该重读了」绝不能把操作者眼前唯一的事实抹掉。关闭的评审不做任何检查。修订号在命令发出
时记录而非在页到达时记录，于是失败方向是多读一次，而不是让评审面默默过期。

**诚实规则**，每条都有自己的句子和自己的测试：

| Core 事实 | 视图渲染 |
| --- | --- |
| `DiffFile.omitted` | 「未显示 diff 行（n 行新增、m 行删除）—— 超出字节上限」，并保留真实计数 |
| `DiffFile.binary` | 「二进制文件，无 diff 行」 |
| `WorkspaceDiffEntry.diff: None` | 「Core 没有为该文件产出 diff。这不代表它没有变化。」 |
| `WorkspaceDiffPage.truncated` | 页级横幅，说明上限丢弃了至少一个文件的行 |
| 已加载但零条目的页 | 「工作区没有变更」—— 唯一可以这样渲染的状态 |
| 尚未得到回答的读取 | 「正在从 Core 读取工作区 diff…」 |
| `CommandRejected` | 在 `role=alert` 中原样呈现 Core 的拒绝文本 |
| 无 `runtime.structured_diff` | 点名该能力，并明确说明这不代表工作区干净 |

表头合计只累加 Core 给出了行的条目；只要有条目没有 diff，合计就被标为不完整，而不是
悄悄少报；页到达之前完全不显示合计。文件行携带 `WorkspaceChangeKind` 字形（M/A/D/R/?）、
Core 自己的 `staged` 标记与逐文件计数。长路径保住文件名：目录一半让位的速度快一百倍，
完整路径挂在该行的 title 上。

**动作侧**（`runtime.operator_git`、GUI-CORE-020）。提交栏会动作，标题栏同步芯片也会。
每个按钮通过 CoreClient 缝隙发送一条带客户端自选 `command_id` 的
`RunOperatorGitAction`，并且只有点名该 id 的有序事件才能结算它。客户端不跑 git、不退化
成 shell 命令，也绝不从 `output` 里读取任何事实。

- **提交栏。** 一个以 Core 的 4 KiB 为界的提交信息框 —— 按 UTF-8 字节计，也就是 Core
  自己度量的单位，这样中文信息不会先越过一个 Core 随后拒绝的上限 —— 外加「全部暂存」
  （`Stage { paths: [] }`）、「提交」与「提交并推送」。每个文件行带一个暂存/取消暂存
  开关，方向由 Core 自身的 `staged` 决定；客户端绝不从 Core 已据以推导的 index 分类中
  重新推导它。
- **「提交并推送」是两条命令。** 只有在提交回报 `Completed`（Core 的类型化答案，而不是
  对 git 输出的解读）之后才发送推送。提交失败或被拒绝会中止这一对，并且提交栏会说明推送
  没有发出 —— 悄悄丢掉后一半，会让操作者以为分支已经发布。
- **一次只有一个动作**，严格按 `command_id` 关联。提交栏是一个只有一个信息框的界面，
  第二个在途动作只能和第一个抢同一个 index。
- **执行身份来自 Core。** `RunOperatorGitAction` 要求 owner 与信封 owner 一致，审计记录
  也需要一个真实的 owner，因此动作只会带着 Core 为该 Lane 绑定的那个精确 `RuntimeOwner`
  发出。没有它时 host 以 `D1-OPERATOR-GIT-OWNER` 在本地拒绝，而不是发一个默认 owner ——
  那会把一次已授权的变更记成「不属于任何人」。`target` 就是当前评审正在读取的那个
  `SourceTarget`。
- **重新读取，而不是打补丁。** 动作结算后，Core 随之发布的 `WorkspaceSourceUpdated` 会
  使该页失效，评审沿着 Core 侧写入所走的同一条去抖失效路径重新读取。

**动作侧的诚实规则**，每条都有自己的句子和自己的测试：

| Core 事实 | 视图渲染 |
| --- | --- |
| `CommandRejected` | Core 的原因逐字放进 `role=alert`。拒绝发生在任何东西运行**之前** |
| `Failed { class, detail }` | 该类别的本地化句子，加上 git 的 `detail` 原文。效果**确实**被尝试并已审计；绝不画成「拒绝」 |
| `Completed` | 由结果中重新采样的 `source` 构成的一行 —— 分支、领先、落后、干净或有变更 —— 绝不来自输出文本 |
| `Completed.output` | 收折在「Git 输出」之后，`truncated` 时另配一条说明 |
| `target.kind = "git"` 的 `ApprovalRequested` | 决策归权限坞所有；提交栏说明它在等哪个动作并保持不可用 |
| 没有 `runtime.operator_git` | 每个控件可见、禁用并指名那项能力 —— 而不是已经关闭的 `GUI-CORE-020` |
| 有在途动作，或 `workspace_source.status != Ready` | 同步芯片禁用并标注原因，同时仍说明它本来会做什么 |

提供的恢复动作只有契约为该类别指名的那一个：`NonFastForward` 给 fetch、`NoUpstream`
给「推送并设置 upstream」，而 `AuthenticationRequired` **什么都不给** —— 那里的重试按钮
只会变成针对本客户端无法提供的凭据的重试循环。无法识别的类别保留自己的句子和 Core 的
`detail`，而不是借用最接近的那一个，后者会给出错误的下一步。

**标题栏同步芯片**是控件：Core 重新采样的计数说明有本地工作时是 `Push`，其余状态
（落后，或干净）都是 `Fetch`。它绝不提供 `Pull`，提示里也写明了原因：契约在 `0.3.3`
排除了 pull、merge、rebase、`commit --amend`、reset、强制推送、删除分支、switch 与
stash，每一项的理由都记录在前端集成契约里。「分栏」仍然可见且禁用，因为只实现了统一
视图。冲突内容属于 `GUI-CORE-015`，是另一个批次。

**审批决策上下文。** 同一个行渲染器在 D1 权限坞与 D2 决策详情中渲染
`ApprovalRequestView.decision_context`，在 D1 变更文件卡片中渲染
`WorkspaceChangeView.diff`。`base_sha256` 渲染为「预览基于 <8 位> 计算」—— 那是预览所
依据的原像，而不是「文件没有变过」的保证，因为执行时才会把拟议的工具输入作用到当时的
文件内容上，且 Core 不会在执行前重新校验。不可用标记按「关于 Core 的断言」对待：Core
广告该能力时 D1 的 `diff` 行消失，D2 的标记按单条决策消失、且只在 Core 确实附了上下文
处消失。`shell` 与 `git_*` 族本就不带上下文，那里的 `input_preview` 保持原有措辞不变。
坞的动作行被固定，上下文自身滚动，因此再长的预览也不会把「允许」「拒绝」挤到它们所要
回答的那些行后面。

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

## D12 冲突内容

`runtime.conflict_content`（Core `0.3.6`，GUI-CORE-015）为 `ConflictBounce` 与
`LaneConflictView` 挂上可选的 `ConflictContent`。D12 把它渲染在携带它的记录之下：
恢复时间线里每条 bounce 画自己的面板；Core 为该闸涉及的 Lane（闸自身的 Lane 与每条
bounce 指名的原 Lane）记录的 Lane 应用冲突单独列出，因为那是另一种由另一个生产者写下
的 Core 记录，而不是该闸恢复过程中的一步。

面板画什么，以及绝不能画什么：

- **两侧加上补丁原像，不是三方合并。** OURS 是 Core 在该 hunk 声明的旧区间上对 Lane
  当前文件所作的只读读取，按 `ours_start` 编号；THEIRS 是传入补丁的新侧，按
  `theirs_start` 编号；BASE 是该 hunk 自己的原像，放在可折叠的第三条里。Core 不计算
  merge base，也不做任何解决，因此客户端不显示合并后的文本、不提供解决控件 —— 该声明
  随每个面板打印，因为双栏布局恰恰会诱发错误理解。这与两个变更动作所依赖的规则是同一
  条：在闸里解掉的 hunk 会是一段从未走过该 Lane 自身闸的代码。
- **基线被指名，而不是被假定。** `Evidence { bindings }` 是合并路径的答案，每个绑定渲染
  成一枚 chip，打开该证据对象自己的审计轨迹，正是 D12 回滚行已有的路由；
  `Revision { sha }` 渲染短 sha，完整值放在该行 title 里；`Unknown` 渲染为未知，绝不
  悄悄当作 `HEAD`。本构建未命名的基线类型按原样渲染。
- **每次拒绝都带归类与补救。** 原因 chip 来自 Core 的 `ConflictHunkReason`，是按判别式
  分支而不是解析消息得来的：上下文不匹配说明原 Lane 必须重新生成补丁，已经应用说明没有
  可应用的内容，文件缺失或删除后残留说明这是文件级决定。未建模的原因原样展示，而不会被
  折进某个已知原因。
- **四种缺失是四句话。** 缺少 `runtime.conflict_content` 时由该屏的不可用行点名该
  capability；Core 未为某条记录发布内容时如实说明，并说明操作者的 `BounceMergeConflict`
  是背后没有失败应用的人为判断、按契约本就不带内容；`omitted` 文件保留条目并说明其 hunk
  超出 Core 的字节上限；`truncated` 载荷带横幅。它们都不得被读成「无冲突」。

行使用 `gui-kit.css` 中已登记的 `.diffbody > .dl` 族，因此冲突行与 diff 行在屏幕上是
同一种对象。ours/theirs 的着色遵循 D12 设计稿，并有意不复用 `.dl.add` / `.dl.del`：
冲突的一侧既不是新增也不是删除，借用这两个类会宣称一个 Core 从未做出的归类。

有一处实现说明值得写明。`viden-core` 再导出了 `ConflictBounce` 与 `LaneConflictView`，
但没有再导出它们携带的 `ConflictContent` 家族，而 GUI 不得持有第二个 `viden-*` 依赖，
因此 `RuntimeProjection` 通过 Core 自己的规范 serde 编码读取该值，而不是引入第二个
解析器。Core 侧的再导出可以去掉这一跳；此事记在 GUI-CORE-015 下。

## EvidenceView 证据档案

`⌘E`（macOS 之外为 `⌃E`）、活动 rail 的 `证据` 槽位与命令面板的 `Open evidence` 在 D1
中央区打开已登记的 `.evwrap > .evmain(.evbar + .evscroll) + .evdet` 族。与 DiffReview
一样，它是**驾驶舱内的一个视图，而不是一条路由**：`D-RAILNAV` 把次级界面登记为覆盖在
会话之上的视图，因此关闭它是回到会话而不是导航。
CSS 来自 `GUI/gui-kit.css`——`D0` 的提升正是为了这第二个消费者而镜像了该族。

`E` 与 `⌘F` 在本外壳中此前都未被占用（驾驶舱仅有 `⌘K`/`⌃P`、`⌘O` 与 `⌘R`），
因此 `⌘E` 切换视图，`⌘F` 在该视图占据中央区时聚焦搜索框。D1 的 Live Work 条**不是**
入口：它是把任务、工具、审批、排队输入与 `latest_evidence` 合并成的一条 `role=status`
行，而把这个近窗投影变成通往档案的门，恰恰是本视图要终结的混淆。

**这是档案，不是 `latest_evidence`。** `RuntimeViewState.latest_evidence` 是近窗投影
——当前事件流发布过什么就按 id 覆盖成一张列表，没有排序规则、没有 cursor、也没有内容。
两者从不合并，也从不出现在同一张列表里。以下全部来自 `runtime.evidence_reads`
（Core `0.3.6`，GUI-CORE-025）。

**数据通路。** 经 CoreClient 缝隙的一次 `QueryEvidence` -> `EvidencePageLoaded`，
关联方式与 `QueryWorkspaceDiff` 相同：同时只有一个读取在途，page 与 `CommandRejected`
上都带确切的 `command_id`，且没有以接受为条件的回退——因为 page 的 id 是必填字段。
首页 `limit: 50`；`owner` 作用域是 Core 为所选 Lane 绑定的确切 `RuntimeOwner`，
收窄到 workspace/project/Lane，并刻意**不**携带 Core 的 turn 绑定——那会把"该 Lane 的
证据"这个问题答成"该轮次的证据"。若选中了 Lane 而 Core 没有确切 owner，宿主在
`D1-EVIDENCE-NO-OWNER` 下本地拒绝，而不是改为无作用域读取：以某个 Lane 的名义给出
无作用域的答案，等于把每条 Lane 的证据都说成那条 Lane 的。未选中 Lane 则读取整个档案。
门禁姿态取自 `QueryAudit` 而非 `QueryWorkspaceFiles`：有界、带 owner 作用域、绝不由
工具门禁把关，因此两个读取在 Plan mode 下均可回答。

**翻页。** "加载更早"再发一次 `QueryEvidence`，把 Core 自己的 `next_after` 原样作为
`after` 带回。客户端从不解析、构造或比较 cursor；Core 没有发布 cursor 时该控件禁用，
而不是让客户端自行拼一个。`complete` 会被明说——"档案已完整"——而不是靠按钮的缺席来
表示；又因为 Core 在切页之前过滤，`complete` 描述的是**过滤后**的档案。

**过滤与搜索是两回事，视图把这点说出来。** 类型 chip 就是 `EvidenceQuery.kinds`，
因此改动 chip 是一次新的 Core 查询：客户端若只收窄手头这一页，无法知道匹配项是否落在
它从未加载的页上。chip 词汇是五个一等类型（`patch`、`test_result`、`review`、
`doc_update`、`release_artifact`），其后跟上已加载行实际携带的其他每一种类型——排在最后，
但绝不隐藏。搜索框正相反：Core 未提供证据搜索，因此它只是对已加载行的大小写不敏感子串
匹配，其 tooltip 与 `aria-label` 会点明这个范围与已加载行数。把所有行都过滤掉的搜索
有自己的句子，与"没有证据"区分开。

**分组。** 行按 `(timestamp, id)` 升序到达，并按到达顺序追加；客户端不做任何排序。
按天分组是覆盖在 Core 顺序之上的呈现——同一本地日期上连续的行归为一组——因此无日期组
排在最前，这正是 Core 的排序已经把它放的位置。无日期行的时间列是一个短横，绝不虚构时钟。

**重查规则：标记陈旧，绝不重载。** 适配器在其唯一的接收漏斗里计数 `EvidenceRecorded`，
因此由无关屏幕的轮询排空的一条记录也能到达打开着的视图；该计数与已加载页读取时的修订号
比较。陈旧的列表保留其行并出现带"刷新"的横幅。与 DiffReview 不同，它绝不自行重读：
diff 面板只持有一棵树的一页，而证据列表持有操作者一页页手动翻出来的若干页，重载会把它
丢弃，并挪动他们正在读的行。

**内容。** 选中一行发送一次 `ReadEvidenceContent`——同时只有一个在途，答案在视图生命期内
按 id 缓存，因此重新选中同一行不再产生第二次读取。视图关闭时缓存被丢弃：活得比视图更久的
正文，可能会被渲染在一份此后已经变化的档案之上。答案按 **Core 回显的** `evidence_id` 归档，
而不是客户端发问时用的 id；当选中的行不是某个答案所属的行时，该区块显示"正在读取"，
而不是显示另一行的字节。Core 只读取该行自身引用指名的 canonical ContextStore 字节，
并先按其 `source_hash` 校验，因此客户端不打开存储，也不渲染未经校验的字节。

**内容诚实规则**，每条都有自己的句子和自己的测试：

| Core 事实 | 详情侧栏渲染什么 |
| --- | --- |
| `Text { truncated: true }` | 正文加上"超过 Core 的 256 KiB 上限：内容被截断"，并明说这不表示证据本身很短 |
| `Text` / `Diff` 的 `sha256` | "已按 \<hash\> 校验"，读者可据此把屏幕上的内容与该行的 canonical 引用对上 |
| `Diff` | DiffReview 与审批界面所用的同一套共享 `diff_rows` 渲染器，因为 Core 用同一个生产者解析了该补丁 |
| `Unavailable { SummaryOnly }` | "仅供展示的证据——Core 未持有其规范字节" |
| `Unavailable { MissingCanonicalBytes }` | "Core 指向的规范字节已不在存储中" |
| `Unavailable { HashMismatch }` | "规范字节校验失败——不予展示" |
| `Unavailable { Binary }` | "规范字节校验通过但不是文本，因此没有可展示的正文" |
| 未建模的原因或内容形态 | 明说本版本无法命名，绝不折叠进某个已知项——两个枚举都是 `#[non_exhaustive]` |
| `CommandRejected` | Core 的拒绝原文原样放进 `role=alert` |

**列表诚实规则。** 尚无答案的读取显示"正在读取"；Core 已答复但零条目，是唯一可以画成
"此范围内没有证据"的状态；拒绝渲染 Core 的原话且不加载任何东西；缺失
`runtime.evidence_reads` 时点名该能力、声明这不等于空档案，并让两个入口保持可见、禁用、
并被贴上该能力的标签——绝不静默隐藏。命令面板行与 `⌘E` 快捷键在挂载时读取一次无流量投影
来得到这个答案，不发送任何命令。

**详情侧栏。** 头部携带类型字形、本地化的类型名、Core 的摘要、归属 Lane、时间戳与证据 id。
报告陈述 `source`、`path`、`归属 lane`，以及在该行指名时的 canonical 条目、捆绑包、
`来源哈希` 与生产者；Core 未记录的字段会明说，而不是留白。`metadata` 只摊平一层并渲染成
事实，其上有一条说明"此处不做任何解读"：Core 的键是自由形态 JSON，客户端若替某个键决定
它**意味着**什么，就是在自造一套词汇。任何东西都不会从 `summary` 推断该行的内容、结果或
校验状态。关联 chip 指名该行携带的 Core 对象——canonical 条目、路径、归属任务——它们是事实
而不是导航。

**页脚。** "在评审中打开"只对 `patch` 行提供，通过 `⌘R` 所用的同一个 `centerView` 开关把
中央区切到 DiffReview；它会说明 DiffReview 读的是**当前工作区**而不是这条已记录的补丁，
并对非 `patch` 行、未绑定宿主、以及不发布 `runtime.structured_diff` 的 Core 各以自己的句子
禁用。"打开审计轨迹"打开限定到 `evidence` 审计对象的 D14——因为 `D-AUDIT` 的链接是单向的，
审计行链接证据而非反向，所以绝不给该行安上一个它并不具备的审计 id。

一条实现说明。`viden-core` 重导出了 `EvidenceView`，但没有重导出
`EvidenceVerificationState` 与 `EvidenceQualityStatus`，因此报告陈述该行的 canonical 引用、
生产者与哈希，而不命名 Core 对它的校验状态或质量状态。这一点记在 GUI-CORE-025 名下。

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

Task 11 曾新增 framework-neutral 组件画廊
（`src/screens/component_gallery.ts`），以及面向 D1、D11、D4、D6 和 gallery 的
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

画廊模块本身已在 `claude/hygiene-h1` 上删除：从来没有任何地方 import 它，没有任何
qa state 或脚本渲染它，它的样式表也从未进入过 bundle，因此它既没有产出 capture，也
不可能发生回归。于是 `release-manifest.toml` 以及 `0.1.0-rc.2`/`0.1.0-rc.3` 快照中的
`[evidence] component_gallery` 键曾指向一个不再存在的路径。rc.2 与 rc.3 两份快照被
`rc_release_manifest_is_an_immutable_byte_equivalent_snapshot` 逐字节冻结，无法就地
修正；该键已在本次 release 步骤编写的 active manifest 及其 `0.1.0-rc.4` 快照中移除，
被冻结的 rc.2 与 rc.3 快照保留它作为那两个版本自身声明的记录，模块本身仍可从 Git
历史中恢复。

## Lane 监视器动作

D10 的卡片带上了设计稿的动作行。其中四个控件里有两个在
`frontend-contract-v1` 上真实存在、两个不存在，动作行的呈现方式让读者无需
点击就能分辨。

| 控件 | 它是什么 |
| --- | --- |
| **接管（Attach）** | 不是 Core 命令。它调用驾驶舱自身的 `selectLane`——Lane 侧栏、Lane 标签条与 `⌃⇥` 共用的同一条路径——随后走路由的 `conversation` 路线，于是输入区开始面向该 Lane，中央面板回到对话。 |
| **停止（Stop）** | 针对该 Lane 自己的 Agent 会话发送 `RuntimeCommand::CancelAgentSession`，命令 id 形如 `gui-d10-stop-…`。展示的结果来自应答事件：被接受，或 Core 自己的拒绝原因。 |
| **暂停（Pause）** | 契约上没有对应命令。渲染为禁用并说明这一点。 |
| **终止（Kill）** | 契约上没有对应命令。渲染为禁用并说明这一点，同时说明「停止」是通过 `CancelAgentSession` 取消，而不是暗示两者是同一个动作。 |

停止的目标按 fail-closed 的方式从该 Lane 已发布的会话中解析，与驾驶舱解析
owner 绑定的方式一致：恰好一个会话才是目标，零个表示没有可停止的对象，多于
一个则说明客户端不会替你选择。未绑定宿主时，整行控件全部禁用并说明原因，而
不是隐藏，也不是保留一个点了没反应的按钮。

`RuntimeCommand` 并未为 Agent 会话提供 pause 或 kill
（`crates/types/src/runtime.rs`）；那两个禁用控件就是这一事实的诚实呈现，而不
是占位。事件流自身的 `eventsUnavailable` 文案在此未改动——它属于 hygiene 批次。

## Fleet 节点下钻

D13 的节点会打开其任务所绑定的 Lane。Core 并未发布「节点→Lane」这条边，因此
GUI 投影在 Core 确实发布的绑定上做联接——`AgentLaneRecord::task_id`——并把结果
按节点携带为 `laneIds`，且刻意用列表而不是 `Option`：

| `laneIds` | 节点的行为 |
| --- | --- |
| 恰好一个 | `role="button"`、可聚焦，点击或 `Enter` 经驾驶舱 `selectLane` 交接；一行可见的 `Open Lane <id> ↗` 在点击前就说明目的地 |
| 空 | 说明 Core 未为该任务绑定任何 Lane；不可聚焦、不可点击 |
| 多于一个 | 说明 Core 绑定了多个并列出它们；客户端不做选择 |

缺失不是错误，也不是失效控件：无法打开 Lane 的节点绝不会渲染成可以打开的样
子。未绑定宿主时每个节点都如实说明。`Space` 刻意不绑定——在非 button 元素上它
是 webview 自己的滚动，夺走它的代价高于该快捷键的价值。

## 审计筛选

D14 在**客户端已持有的这一页**上提供设计稿的执行者与时间筛选，并且筛选条上的
每个标签都说明了这一点。`AuditQuery` 携带 `project_id`、`lane_id`、`object` 与
`before` 游标——没有执行者筛选，也没有时间区间（GUI-CORE-024）——因此服务端筛选
并不存在，而一个对自身范围保持沉默的客户端筛选，会在匹配记录位于从未加载过的
页面时，错误地报告「该执行者没有任何记录」。

- **执行者标签**是已加载页实际携带的 `actorKind` 值，外加一个中性的
  `全部执行者`。契约上合法但本页不存在的类型不会被提供：一个只可能筛出空结果的
  标签，会错误描述已加载的内容。这些值按 Core 自己的用词原样渲染、不做本地化，
  与 `action` 键同理。
- **时间标签**为 `全部时间`、`今天（UTC）`、`近 24 小时` 与 `近 7 天`，按行自身
  的 Core 时间戳切分。`今天`指 UTC 日历日而不是滚动 24 小时，因为行打印的是 UTC
  时钟，两个比对证据的人必须指同一天。
- **结果分布条**统计其下方实际绘制的行，标题始终点明已加载页——
  `已加载页的结果分布 · N`，或 `已加载页（已筛选）的结果分布 · N of M`。它绝不
  是总数，因为审计库比任何一页都大。
- **筛到空**时显示「已加载页中没有符合这些筛选条件的记录」。这与「Core 未为该视
  图记录任何审计条目」是两个不同事实，两者绝不共用一句话。
- **导出**在设计稿筛选条中已登记，但背后没有命令，因此渲染为禁用并注明
  GUI-CORE-024。

筛选状态只在本次挂载期间保留，绝不持久化：Core 是唯一的偏好权威，而被记住的筛
选也正是让操作者回到一条悄悄隐藏了一半内容的轨迹的方式。原始回放模式未被触
碰——它是诊断事件日志，其价值在于 Core 流位置，而不是执行者或挂钟时间。

## 决策队列计数气泡

活动侧栏的 D2 槽位携带 Core 发布的 `D1StatusbarProjection.pendingGateCount`，
与状态栏 `⏸` 段落打印的是同一个数。该字段现在是 `number | null`，三种取值对应
三种不同呈现：

| 取值 | 侧栏 | 状态栏 |
| --- | --- | --- |
| 大于零 | 设计稿的 `.badge`，显示 Core 的数字 | `⏸` 段落 |
| `0` | 无气泡 | 无段落 |
| `null` | 无气泡，且槽位名称说明 Core 未发布决策计数 | 无段落 |

`null` 是外壳在连接建立前的占位投影所携带的值。它此前是 `0`，于是在任何计数发
生之前就把决策队列渲染成空队列；缺失与零是不同事实，现在也画得不同。

一处如实记录而非隐藏的保留项：D2 视图自身的 `pendingTotal` 是**另一个**和
——审批加待处理复查，而气泡是审批加未休眠的开放合并闸——因此气泡与视图标题可能
合理地不一致。二者的对齐属于拥有这些计数的批次的投影问题；在侧栏里再数一遍，
只会让侧栏同时与状态栏也不一致。
