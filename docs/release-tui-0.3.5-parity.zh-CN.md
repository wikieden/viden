# Viden TUI 0.3.5 对齐 —— 计划文档、会话控制、审批范围、starter Lane 的 Agent 绑定

English version: [release-tui-0.3.5-parity.md](release-tui-0.3.5-parity.md)

本文记录 TUI 对 `0.3.5` 四项 Core 能力的最小对齐：`runtime.plan_documents`
（C13）、`runtime.agent_session_control`（C14）、`runtime.approval_scopes`
（C15，GUI-CORE-019）与 `runtime.starter_lane_agent`（C15，GUI-CORE-030）。契约见
[前端集成契约](frontend-integration-contract.md)的
"Plan documents"、"Session Control"、"Approval Scopes" 与
"Starter-Lane Agent Binding" 各节；设计与其落地修订见
[0.3.5 Core 契约增量](release-0.3.5-contract-design.zh-CN.md)。

下述每一条事实都由 Core 权威持有。TUI 不持有计划、不通过本地扣留输入来"暂停"
会话、不记忆任何授权、也不自行启动会话。展示给操作者的每一条拒绝，都是 Core 自己
的理由文本。

## 计划透镜（`runtime.plan_documents`）

`/plan` 打开一个覆盖 `RuntimeViewState.plan_documents` 的浮层。透镜不持有计划：
记录在每一帧、以及每一次按键时都从视图重新读取，因此在渲染与按键之间发生变化的
修订，要么按 Core 当前持有的内容决策，要么被 Core 判为 stale 而拒绝。

展示哪个计划：若已聚焦某个 Lane，且 Core 为该 Lane 发布过计划，则收窄到这些计划；
在候选中 **open** 状态（`Drafting`、`Proposed`、`Accepted`）优先，并按 Core 自己的
`updated_at` 排序（`apps/tui/src/tui/plan_lens.rs:168`）。

### 各行展示的内容

| 行 | Core 事实 |
| --- | --- |
| `<title> · <status> · rev <n>` | `title`、`status`、`revision` |
| `GOAL` | `goal` |
| `AUTHOR` | `proposed_by` —— `assistant · session … · turn …` 或 `operator` |
| `DIGEST` | `content_sha256` |
| 章节标题与正文 | `sections`，每个 `PlanSectionKind` 至多一节 |
| `STEPS <n>` 及每个步骤一行 | `steps` |
| `HANDOFF dag … · audit …` | `handoff`，在 Core 发布之前不存在 |

步骤字形取自已登记的 TUI 状态字形词表
（`docs/viden-design/Viden/docs/DESIGN-REF.md`，"TUI 字形词表"）：`Pending` 用
`◌` wait、`InProgress` 用 `▶` run、`Completed` 用 `✓` done。每个步骤行还带上它的
`role` 与 `dependencies`（`· after step_1`）。

### 各键发送什么

| 键 | 命令 | 仅在以下条件发送 |
| --- | --- | --- |
| `a` | `AcceptPlanDocument { plan_id, revision, content_sha256 }` | `status == Proposed` |
| `r` | 先打开理由行，再发 `RejectPlanDocument { …, reason }` | `status == Proposed` |
| `h` | `HandOffPlanDocument { plan_id, revision, content_sha256 }` | `status == Accepted` |
| `↑` `↓` | 仅滚动 | 始终 |
| `Esc` | 先收起理由行，再关闭透镜 | 始终 |

`revision` 与 `content_sha256` 取自按键那一刻重新读取的记录
（`apps/tui/src/tui/app.rs:1477`、`apps/tui/src/tui/plan_lens.rs:414`），而不是取自
已绘制的那一行。按键映射见 `apps/tui/src/tui/app.rs:1402`，状态闸门见
`apps/tui/src/tui/app.rs:1418`。

### 缺席与禁用状态

- **尚未提出计划。** `plan_documents` 为空时渲染为一行明确说明，绝不从助手叙述中
  解析计划，也绝不把空计划渲染成计划。
- **没有该能力。** 透镜仍会打开，其唯一一行点名 `runtime.plan_documents`，从而把
  "这个 Core 不发布计划"与"尚未提出计划"区分开。
- **状态拒绝的键**会被列出并标注 —— `h Hand off · unavailable in proposed` ——
  且完全不发送：Core 自己的契约已说明它会拒绝，因此不去问。
- **计划模式下的交接**保留按键并标注 `· refused in plan mode`。只有 Core 有权说
  该模式拒绝它，所以命令照常发出，`plan_mode` 拒绝由 Core 的理由渲染。
- **空的或超界的拒绝理由**在本地被拒绝且不发送任何内容：Core 会存储该理由并拒绝空
  理由，客户端选择拒绝而不是截断（`apps/tui/src/tui/app.rs:1444`）。
- **`Superseded` 记录**会就地说明。此构建无法命名的状态渲染为未知，绝不渲染成
  `Proposed`，也绝不渲染成已决策。

## 会话控制（`runtime.agent_session_control`）

Lane 视图的 `/acp` 会话行获得五个控制键。每个键都在输入归约器之前被拦截，与该选择器
存在以来 retry 键的做法完全一致（`apps/tui/src/tui/app.rs:500`）。

| 键 | 命令 | 可从以下状态接受 |
| --- | --- | --- |
| `p` | `PauseAgentSession` | `Starting`、`Running`、`WaitingApproval` |
| `u` | `ResumeAgentSession` | 仅 `Paused` |
| `k` | `KillAgentSession` | 任何非终止状态，含 `Paused` |
| `s` | `CancelAgentSession`（不变） | 任何非终止状态 |
| `r` | `RetryAgentSession`（不变） | `Failed`、`Cancelled`、`Killed` |

`r` 仍是 Retry —— 自该选择器存在以来一直如此 —— 所以 Resume 取 `u`：把操作者已经
形成肌肉记忆的键悄悄指向另一个命令，正是"恢复"变成"重启"的方式。每条命令都在会话
自身 Core 发布的 owner 下发送，与 cancel 早已遵循的规则一致
（`apps/tui/src/tui/app.rs:538`）。状态闸门见
`apps/tui/src/tui/session_control.rs:94`，载荷见
`apps/tui/src/tui/session_control.rs:117`。

### 各行展示的内容

`⏸ codex-acp · tidy the config loader · Paused · held 2`

- 已登记的状态字形（`apps/tui/src/tui/session_control.rs:163`）：`Starting` 用
  `◌` wait、`Running` 用 `▶` run、`WaitingApproval` 用 `◆` your-turn、`Completed`
  用 `✓` done、**`Paused` 用 `⏸` gate**、**`Failed` 与 `Killed` 用 `✗` fail**、
  `Cancelled` 用 `⚠` warning。`⏸` 就是词表自己的暂停标记，`✗` 正是被操作者终止的
  会话；两个字形都不是新造的。
- `· held <n>` 来自 `control.held_input_count` —— "已键入但未发送"唯一诚实的来源；
- `· ended by kill` / `· ended by cancel` 来自 `control.terminated_by` —— 在两者都
  settled 之后，唯一能区分终止与取消的事实。

当 Core 未发布 `control` 时两者都不显示：缺席就是缺席，绝不是零
（`apps/tui/src/tui/session_control.rs:281`）。

### 缺席与禁用状态

- 会话当前状态拒绝的控制会标注 `· unavailable in <status>` 且不发送任何内容。
- 没有该扩展时，Pause、Resume、Kill 标注缺失的能力，而 Stop 与 Retry 仍然可用，从而
  把"这个 Core 不能暂停"与"这个会话不能停止"区分开。
- `session_status_<current>` 拒绝以 Core 自己的理由进入 transcript
  （`apps/tui/src/tui/app.rs:588`），并在与监督、`/git` 相互独立的槽位上按本客户端
  自己的 command id 关联。
- **切换模型**是被陈述而非被提供的：第二行惰性行写着 `Switch model = k Kill + new
  session: model, load_session_id`。Core 未发布切换命令，契约将其组合为两步。

## 审批范围（`runtime.approval_scopes`，GUI-CORE-019）

`Always → AllowOnce` 的投影被退役（`apps/tui/src/tui/projection.rs`）：在 Core 提供
一条跨越所有会话与所有工作区的规则时，却告诉操作者他在"允许一次"，正是知情决策的
反面。

| 键 | 决策 | 仅在以下条件发送 |
| --- | --- | --- |
| `1` / `y` | `Allow { Once }` | `allowed_scopes` 含 `once` |
| `2` | `Allow { Session }` | `allowed_scopes` 含 `session` |
| `3` | `Allow { RepoAllowlist }` | `allowed_scopes` 含 `repo_allowlist` |
| `a` | `Allow { Always }` | `allowed_scopes` 含 `always` |
| `4` / `n` | `Deny` | 始终 |
| `e` | 打开修订字段，随后发 `Edit { revised_input }` | `editable` 为真 |

`a` 见 `apps/tui/src/tui/input.rs:130`；`e` 见 `apps/tui/src/tui/input.rs:135` 与
`apps/tui/src/tui/input.rs:165`。修订使用契约自己的 `ToolInput` 编码解码 ——
`viden_types::decode_tool_input`，`key=value`、以制表符分隔 ——
（`apps/tui/src/tui/input.rs:219`），绝不从渲染后的 `input_preview` 解析。

`/approvals` 列出 `RuntimeViewState.standing_approvals`，每条规则带上工具、内容、
授予工作区与审计行；`Enter` 通过 `RevokeStandingApproval` 撤销选中的 `rule_id`
（`apps/tui/src/tui/approval_scopes.rs:33`、`apps/tui/src/tui/app.rs:1267`）。该列表
读的是 Core 的字段，而不是本客户端对所见授权的记忆。

### 缺席与禁用状态

- Core 未提供的范围显示为带 ` · unavailable` 的禁用行，其按键不发送任何内容。高风险
  请求或计划模式下不提供 `Always`，操作者看到的正是那一行禁用状态。
- Core 标记为不可修改的请求显示 `e Edit input · Core 标记该输入不可修改`，按键无效。
- 解码为空的修订在本地被拒绝：`修订需为 key=value 键值对。未发送任何内容。`
- 由一次编辑产生的第二个请求带 `修订自 <request_id>`（来自 `revised_from`），使操作
  者替换他正在看的那一行，而不是看到两个互不相干的提示。
- `standing_approvals` 为空时渲染为缺席；没有该扩展时列表点名缺失的能力。

### 已知缺口 —— 修订字段无法预填

`ApprovalRequestView` 不携带结构化 `ToolInput`。`input_preview` 是为人类读者构造的
`key: value` 渲染文本（`crates/permissions/src/lib.rs:435`），而同一模块中的
`candidate_paths` 之所以存在，正是为了让审批界面不去刮取展示文本。因此客户端既无法
展示"可编辑的输入字段"，也无法在不刮取的前提下预填修订。

落地方案：字段以空白打开，其提示行陈述该缺口 —— `修订 · key=value，以制表符分隔 ·
Enter 发送 · Esc 放弃 · Core 未发布可预填的结构化输入`。修订由操作者键入，其下游的
一切 —— 重新判定、第二个请求、`revision_unchanged` 拒绝 —— 仍完全属于 Core，未作
改动。已作为 Core 契约请求登记，见下文"Core 契约请求"。

## Starter Lane 的 Agent 绑定（`runtime.starter_lane_agent`，GUI-CORE-030）

Lane 创建分为两步。任务步交接给一个由 `RuntimeViewState.agent_adapters` 构建的
Agent 选择；在确认选择之前不发送任何内容，且 `Esc` 将选择回退到任务步，而不是关闭
面板。

| 行 | 发送的绑定 |
| --- | --- |
| `不绑定 Agent · 先创建 Lane，之后再启动会话`（恒为第一行） | `agent: None` —— 两步流程，与该字段存在之前本客户端所发字节完全一致 |
| `<agent_id> · 适配器默认模型 · <startability>` | `agent: { agent_id, model: None, task: None }` |
| `<agent_id> · <model> · <startability>` | `agent: { agent_id, model: Some(model), task: None }` |

每个适配器一行，外加该适配器发布的每个模型各一行
（`apps/tui/src/tui/starter_lane_agent.rs:76`），因此选择模型绝不意味着写下一个本
客户端无法核实其存在的模型名。`task` 留空：Core 会将其默认为该 preset 的 Lane 摘要，
而 composer 中的文本是第一个回合，由创建流程单独提交
（`apps/tui/src/tui/starter_lane_agent.rs:55`）。

### 回显与渲染的内容

- **preview 的** `agent` 由 Core 的回显渲染，而不是由本客户端对所选项的记忆渲染，并
  连同每一条 preview 诊断一起展示 —— 因此 Core 不认识的 `agent_id` 会在 receipt 到达
  之前就以 `starter_lane.agent_unknown:<id>` 出现。
- **receipt 的** `session` 结果会被陈述：`Started` 点名会话 id；`Refused` 在一个确实
  存在的 Lane 旁点名 Core 的理由，因为 Lane 创建绝不会因为会话未启动而失败。
- 没有该扩展时，选择列表只有两步流程一行，并点名缺失的能力：一个会被这个 Core 丢弃的
  绑定比没有绑定更糟，因为操作者会以为会话正在启动。

## 回归帧

`scripts/tui-previews.sh` 与 `scripts/tui-regression.sh` 新增四帧，每一帧断言的都是
Core 事实而非版式：

| 帧 | 断言内容 |
| --- | --- |
| `main-plan-lens` | `rev 2`、`a Accept`、`step_1`、`Hand off · unavailable in proposed` |
| `main-session-control` | `held 2`、`ended by kill`、`p Pause`、`u Resume`、`k Kill`、`load_session_id` |
| `main-approval-scopes` | `a Allow always`、`e Edit input`、`REVISES` |
| `main-lane-agent` | `No Agent`、`claude-sonnet` |

### T4 重新锚定的基线及其原因

| 基线 | 原因 |
| --- | --- |
| `jump::tests::index_groups_typed_runtime_facts_in_stable_order_and_keeps_file_unavailable` | 命令行数 16 → 18：`/plan` 与 `/approvals` 加入注册表 |
| `modal::tests::global_jump_windows_rows_to_keep_selected_item_visible` | 选中行 14 → 16，原因同上 |
| `modal::tests::global_jump_window_keeps_default_disabled_tail_selected` | 禁用的 FILES 尾行 16 → 18，原因同上 |
| `preview::tests::previews_use_stable_demo_workspace_snapshot` | 命令面板八行窗口现在止于 `/decisions`，故改为断言 `/approvals` 与 `/plan`，而 `/evidence` 已在窗口外一行 |
| `modal::tests::acp_picker_lists_lane_sessions_before_truthful_adapter_rows` | 适配器行由索引 1 移到 3；两行惰性控制行追加在**会话之后**，因此任何会话行的索引都不移动 |
| `app::tests::native_lane_task_waits_for_preview_and_receipt_before_submitting` 与 `…_preserves_direct_workspace_preview_without_fake_branch` | Lane 创建新增 Agent 步；两者现在都断言在确认选择之前不发送任何内容 |
| 审批浮层高度 14 → 18（`modal.rs`） | 否则新增的常驻范围行与修订行会把 `Deny` 与 `Edit` 挤出面板底部 |
| `apps/tui/release-manifest.toml` 的 `catalog_en` / `catalog_zh_cn` | 两份词表新增了计划、会话控制、审批范围与 starter Lane 的英文与简体中文键 |

`scripts/tui-regression.sh` 在完成分支上通过：24 个文本帧、对应的 `.ansi` 渲染与 24 个
`.svg` 产物，外加 22 项认证测试与边界扫描。

## 离线实机检查，2026-09-13

在 tmux（140x40）中以 `--provider fallback --model test-local` 运行，针对本次会话
scratchpad 下为此次运行新建的 Git 仓库执行，`VIDEN_HOME` 指向其旁的 scratch 目录。
引擎 home 解析为该 scratch 仓库自身的 `.viden/`（`SessionStore::new` 优先使用
`<cwd>/.viden`），因此常驻审批文件是 `<scratch 仓库>/.viden/settings.toml`。本仓库的
`.viden/` 与 `~/.viden/` 既未读取也未写入，两者均未被修改。fixture 为 `main` 上的
一次提交（`4f78a0c Add the fixture README`）。

计划是经由真实工具路径驱动的：fallback provider 会把 `tool <name> key=value …`
作为模型工具调用转发，因此 `update_plan` 由注册表执行、并由 runtime 折叠为计划，
与规划模型自己的调用路径完全相同。

| # | 步骤 | Pane 文本 |
| --- | --- | --- |
| 1 | `tool update_plan title=Split-the-supervisor plan=[…3 步…]` | `EVIDENCE  Recorded the plan `Split-the-supervisor`: 3 step(s) (1 pending, 1 in progress, 1 completed) and 0 section(s). The operator decides it; it is not work until they hand it off.` |
| 2 | `/plan` | `Split-the-supervisor · proposed · rev 1` · `DIGEST d71340819cc71fa0…` · `✓ step_1  Extract-the-lane-seam` · `▶ step_2  Move-the-supervisor-arms` · `◌ step_3  Re-run-the-frozen-digests` · `a Accept` · `r Reject…` · `h Hand off · unavailable in proposed` |
| 3 | `a` | `Split-the-supervisor · accepted · rev 1` · `a Accept · unavailable in accepted` · `r Reject… · unavailable in accepted` · `h Hand off` —— 决策行随 Core 自己的状态变更翻转 |
| 4 | `tool shell command=pwd` | `APPROVAL approval_1789288934582889000 · Active · AUDIT audit_1789288934582893000` |
| 5 | `Ctrl-G`、`Enter` | `Approve shell · Medium` · `1 Allow once` · `2 Allow for session · unavailable` · `3 Add repo allowlist · unavailable` · `a Allow always` · `4 Deny` · `e Edit input` |
| 6 | `a` | 请求被解决。`<scratch 仓库>/.viden/settings.toml` 出现 `[[permissions.standing]] … rule_content = "command=pwd" … tool_name = "shell"`，审计日志出现 `audit_1789288934582893000 approval.always`。 |
| 7 | 重启客户端后再次 `tool shell command=pwd` | **完全没有审批**即运行：`EVIDENCE  /…/t4-smoke/repo`。规则在新进程中于 bootstrap 时载入 —— 这正是它与 `session`、`repo_allowlist` 的根本差别。 |
| 8 | `/approvals` | `> shell  command=pwd · granted in ws_dd36031d32c89aa8 · audit audit_…` |
| 9 | `Enter`（撤销） | `No standing approvals. Core holds no rule that outlives this session.`；文件读作 `[permissions]` / `standing = []`；审计行 `approval.revoke`。 |
| 10 | 再次 `tool shell command=pwd` | `APPROVAL approval_1789289107439888000 · Active` —— 请求回来了。 |
| 11 | `Ctrl-G`、`Enter`、`e`、`command=id` | `Revision · key=value, tab-separated · Enter sends · Esc discards · Core …` · `› command=id` |
| 12 | `Enter` | Core 重新判定了该修订并发布了**第二个**请求：`TARGET command: id`，新 id `approval_1789289132946687000`，新的 `AUDIT audit_1789289132946687000`，以及 `REVISES approval_1789289107439888000`。 |
| 13 | `1` | 修订后的输入运行；审计行 `approval.allow_once`。 |

本次运行写入的审计行，从 scratch 目录带外读取：

```
audit_1789288863870522000 plan.accepted
audit_1789288934582893000 approval.always
audit_1789289094067974000 approval.revoke
audit_1789289107439893000 approval.edit
audit_1789289132946687000 approval.allow_once
```

一条属于驱动脚本而非客户端的说明：有一行 `USER` 读作 `itool read_file
path=README.md`，因为脚本在 composer 已处于 Insert 模式时又发了一个 `i`。这与 T2、
T3 与 E2 记录的是同一种现象。

## 实机运行未覆盖的部分

- **暂停、恢复与终止未在实机中执行。** 它们需要一个运行中的 ACP 或 Codex
  app-server 会话，而离线 fallback provider 无法启动这类会话；fallback 路径下根本
  没有可控制的 Agent 会话。命令、状态闸门、字形与 `session_status_<current>` 拒绝
  由 crate 测试覆盖（`session_control.rs` 与 `app.rs`），依据共享 fixture
  `agent-session-control.json` 自身的状态；`main-session-control` 帧同时渲染了一个
  paused 与一个 killed 会话。
- **starter Lane 绑定同样未在实机中执行**，原因相同：离线状态下没有可启动的适配器，
  因此被绑定的会话会因环境原因而非契约原因被拒绝。请求上的绑定、preview 回显与两种
  receipt 结果均在 `app.rs` 中依据共享 fixture 覆盖。
- **交接**未在实机发送：已接受计划的交接会派生 agent DAG，那是工作而非证据；且本次
  运行未处于计划模式，因此 `plan_mode` 拒绝没有可拒绝的对象。

## Core 契约请求

1. **`editable` 请求的 `ApprovalRequestView` 需要结构化 `ToolInput`。** Core 已经
   持有它 —— C15 为 `revision_unchanged` 的逐字节比较新增的 `PermissionPrompt.input`
   —— 但未发布。线上唯一可用的是渲染后的 `input_preview`，而契约自身的规则是客户端
   不刮取展示文本。在它被发布之前，任何客户端都无法渲染"可编辑的输入字段"，也无法
   预填修订；TUI 打开一个空字段并如实说明。
2. **`viden_core` 未重导出 `StandingApprovalView`、`StarterLaneAgentBinding` 与
   `StarterLaneSessionOutcome`。** 这四项能力中的其他类型都由 facade 重导出。TUI 通过
   它对 `viden-types` 的直接依赖取用它们 —— 可行，但意味着三个 `0.3.5` 类型缺席于
   前端本应消费的那个 crate。

两项都不阻塞本批次。
