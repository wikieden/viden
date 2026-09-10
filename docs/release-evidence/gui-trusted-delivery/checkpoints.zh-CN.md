# 0.3.3 可信交付 Checkpoints

English version: [checkpoints.md](checkpoints.md)

日期：2026-09-10

本证据描述的**只是一个本地候选**。这里的任何内容都没有被发布、签名、公证、推送、
合并、打 tag，也没有经过 live provider 认证。下文每个版本都是记录在本地集成分支上
的候选；任何需要网络、凭据或打包步骤的 release gate 都没有运行。

## 候选线

| 项 | SHA / 路径 |
| --- | --- |
| 基线 `origin/main` | `7473a73eeec6336c2ed9059780d3ed97082b0c5e` |
| 集成分支 | `claude/int-0.3.3`，位于 `13795abce263bd60f0e4785e4af0f3f852cb3009` |
| E1 分支 / worktree | `claude/e1-release-evidence`，在 `.worktrees/e1-release-evidence` |
| 运行代码类 gate 时的 E1 HEAD | `727b87b5e6454e21e30449af24b4fa8836413103`（Part A 的提交）。其后的提交只改动 Markdown 与截取文件，因此没有任何代码类 gate 过期。按 D1 的先例，本文不指名自己的提交；分支 tip 在 handoff 中报告。 |
| Core `0.3.6` 实现 checkpoint | `1cec82185bbe860d6b8536a63741bc01f1edf2f6`（集成分支上最后一个触及 `crates/**` 的提交） |
| Core `0.3.6` 基线契约 checkpoint | `5bd2b80b0953f4194d082940a7b9164c7231ca2d`（自 `0.3.0` 以来未变） |
| TUI 候选 | `0.3.4`，`min_core_version` 保持 `0.3.4` |
| GUI 候选 | `0.1.0-rc.4`，`[core].minimum_version` 保持 `0.3.5` |
| 前端 schema | `1`，未变 |
| Capabilities | 15 项冻结基线 + 23 项 extension |

三条组件线各自独立推进，这正是 `docs/parallel-development-plan.zh-CN.md` 的要求。
Core 推进是因为 `0.3.3` 契约增量新增了四项能力；两个客户端推进是因为它们都完成了
采纳。

### 这里的「不可变 checkpoint」是什么意思

`crates/core/release-manifest.toml` 现在记录 `component_version = "0.3.6"`，
`status = "immutable_checkpoint"`。这个声明说的是契约，不是分发：它冻结的 payload
是 schema-1 协议、23 项公布的能力，以及下文的 fixture 摘要。它指名的提交位于一条
本地集成分支上。如果该分支在合并前被 rebase，这个 checkpoint 必须对着新的 SHA
重新声明，而不能假定它自动存活。

## 确定性证据

下表每一行都在 `claude/e1-release-evidence` 的上述 HEAD、在该 worktree 内、离线
运行。

| 命令 | 结果 |
| --- | --- |
| `cargo build --workspace --tests` | PASS |
| `cargo test -p viden-types` | PASS，147（140 + 7 + 0） |
| `cargo test -p viden-core` | PASS，6 个 suite 共 53，15 个 ignored |
| `cargo test -p viden-core --test frontend_contract_v1` | PASS，19 通过，15 ignored |
| `cargo test -p viden-tui` | PASS，410（409 + 1） |
| `cargo test -p viden-gui` | PASS，274，1 个 ignored |
| `cargo test -p viden-gui --test architecture_boundary` | PASS，7 |
| `cargo test -p viden-gui --test capture_projections -- --ignored` | PASS，1 |
| `cargo test --workspace --quiet` | PASS，**1942 通过，0 失败**，78 条结果行，首次运行即通过，无需重跑任何抖动测试 |
| `npm --prefix apps/gui test -- --run` | PASS，42 个文件 / 656 个测试 |
| `npm --prefix apps/gui run build` | PASS（`tsc --noEmit && vite build`） |
| `npm --prefix apps/gui run tauri -- build --bundles app` | PASS |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets` | PASS，退出码 0，只有 warning（`viden-runtime` 2 条、`viden-gui` 4 条），全部是既有问题且不在本批次 diff 内 |
| `scripts/check-dependency-boundaries.sh` | PASS，退出码 0 |
| `scripts/tui-regression.sh` | PASS，退出码 0；证据在 `target/tui-regression/0.3.4/` |
| `scripts/tui-previews.sh` | PASS（在 `tui-regression.sh` 内部运行） |
| `scripts/native-acp-fixture-parity.sh` | PASS，退出码 0 |
| `scripts/tui-turn-controller-smoke.sh` | PASS，退出码 0 |
| `scripts/rc-tui-stability-smoke.sh` | PASS，退出码 0 |
| `scripts/check-doc-pairs.sh`（每一对改动的 Markdown） | PASS |
| `scripts/check-doc-links.sh`（每一对改动的 Markdown） | PASS |
| `git diff --check` | PASS |
| `scripts/release-gate.sh --phase prepublish` | **未运行。** 没有 `DEEPSEEK_API_KEY` 它直接拒绝启动，而它的 `release-smoke.sh --deepseek` 一段属于 live provider 认证，本批次未获授权运行。它的离线部分——依赖边界、文档配对/链接检查——已单独运行，见上文各行。 |

### Fixture 摘要

九个冻结的 `frontend-contract-v1` 基线 fixture 已用 `shasum -a 256` 校验，与
`apps/tui/release-manifest.toml` 中固定的摘要逐字节一致。九个全部匹配，无一移动。

四个 `0.3.3` extension fixture 首次记入 `crates/core/release-manifest.toml`。
`payload_sha256` 是对 fixture 文件确切字节的 sha256；`view_sha256` 是 fixture
自带的 `expected_view_sha256`，重放测试会从归约后的视图重新算出它；
`final_cursor` 是重放结束时的序号。

| Fixture | payload sha256 | view sha256 | 终止 cursor |
| --- | --- | --- | --- |
| `structured-diff.json` | `e24915b3…0de21cd0` | `3f5f4caf…e080676b8a` | 6 |
| `operator-git.json` | `f29aa213…0ba683936` | `07eadb0e…5a86ac14e1` | 10 |
| `conflict-content.json` | `830afb77…22c96ce1f` | `d73ea2a1…9dc1c4d01bf` | 4 |
| `evidence-reads.json` | `4d335131…6f6393e6fc4caa0` | `b15cb2fd…9f51edf8ef9` | 14 |

### 发现并重新固定的两处陈旧摘要

两处都是字面量，而字面量固定只能证明「有人曾经把它敲进去过一次」。

- `crates/core/release-manifest.toml` 与
  `crates/core/frontend-contract-extensions.toml` 把 interaction fixture 固定在
  `78e8993f…` / `46db05ab…`。`interaction-closed-loop.json` 在那次固定之后被改过
  两次（提交 `77b540a0` 与 `2d88628e`），现在是 `a6f1c436…`，view 为
  `c43d9fe3…`。这两个文件，以及
  `docs/core-0.3-compatibility{,.zh-CN}.md` 中的 corpus 表（那里还携带着第三组
  不同的值），都已按磁盘上的字节重新固定。
- `apps/gui/release-manifest.toml` 把 `extension_fixture_sha256` 固定在
  `f96ba30c…`，而今天的 corpus 里没有任何文件与之匹配；
  `d1-main-cockpit.json` 是 `05ac2590…`。

两处守卫现在都改为**推导**摘要而不是比对字面量：
`crates/core/tests/frontend_contract_v1.rs` 重新计算 interaction fixture 的字节与
重放视图，以及四个 extension fixture 的三个值；
`apps/gui/tests/architecture_boundary.rs` 重新计算 manifest 自己指名的规范
fixture 的摘要，并断言 `required_ids` 中每一项在 corpus 中确实存在。一个移动过的
fixture 现在会让发布记录失败。

## 真实任务

### 尝试了什么，通过哪个客户端

简报要求这次任务通过**原生 GUI 窗口**驱动。在本环境中做不到，原因不是工具缺口：

- Tauri 应用构建并启动成功。`target/release/bundle/macos/Viden.app`，
  `CFBundleShortVersionString` `0.1.0-rc.4`，`CFBundleIdentifier`
  `dev.viden.gui`，arm64，ad-hoc linker-signed，无 TeamIdentifier，可执行文件
  38,946,048 字节。以指向临时目录的 `VIDEN_HOME` 启动后，它运行起来并注册了一个
  真实窗口：`CGWindowListCopyWindowInfo` 报告窗口 `25172`，owner `Viden`，
  1254×784，位于 (237, 112)。
- 宿主 Mac 的屏幕处于**锁定**状态。`CGSessionCopyCurrentDictionary` 返回
  `CGSSessionScreenIsLocked = 1`。`screencapture -x -l 25172` 与
  `screencapture -x -R 237,112,1254,784` 都回答「could not create image」；全屏
  `screencapture -x` 得到一张全黑的图。`CGPreflightScreenCaptureAccess()` 返回
  `true`，因此录屏权限是有的，全黑源于锁屏而不是权限。
- 解锁这台 Mac 需要输入用户的密码，这不是本次运行可以做的事。屏幕锁定期间**刻意
  没有**发送任何按键，因为锁定会话会把按键送到登录窗口。

于是任务改为通过 **TUI** 驱动——它是同一套 Core 契约的另一个客户端，且不需要显示
器。流程中属于 GUI 的那一半（DiffReview、EvidenceView、D2 dock），因此由本批次
`apps/gui/evidence/` 下的确定性无头截图覆盖，而不是由一个活着的窗口覆盖。

### Fixture

本次运行临时目录下的一个全新 Git 仓库，一个提交（`Add the fixture README`）、一个
`README.md`，以及一个选择 `provider = "fallback"` / `model = "test-local"` 的
`.viden/config.toml`——项目正是在这里选择 provider（`crates/config/src/lib.rs`，
`config_paths`）；`viden.toml` 是关于 gate 与权限的仓库策略，不是 provider 选择。
本仓库自身的 `.viden/` 全程未被触碰。

`fallback` provider 会把形如 `tool <name> key=value …` 的用户消息变成一次真实的
工具调用（`crates/provider/src/fallback.rs` 的 `parse_explicit_tool_call`），这就是
在没有 live model 的情况下产生变更的方式。

### 逐步结果

| # | 步骤 | 结果 | 截取 |
| --- | --- | --- | --- |
| 1 | 接入 | TUI 把 fixture 仓库作为工作区打开，并从项目配置解析出 `fallback` / `test-local`。`0 lanes`，版本 `v0.3.4`。 | [`01-welcome.txt`](tui/01-welcome.txt) |
| 2 | 新建 Lane | `/lanes` 发布了一次 Core `lane_create` 审批，风险 **Medium**，目标为工作区根，input 指名分支与 worktree 路径，选项为 `1 Allow once` / `2 Allow for session · unavailable` / `3 Add repo allowlist` / `4 Deny`，并带有 `auto-deny @… · default Deny` 的过期策略。 | [`02-lane-create-approval.txt`](tui/02-lane-create-approval.txt) |
| 3 | Lane 已创建 | `Allow once` 创建了 Lane、其分支 `viden/lane_…` 以及 `.worktrees/` 下的 worktree。带外验证：`git worktree list` 显示两个 worktree，`git branch` 显示该 Lane 分支。路由 `main→side-1`，状态 `Draft`。 | [`03-lane-created.txt`](tui/03-lane-created.txt) |
| 4 | 输入框编辑 | `tool edit_file path=README.md old=Fixture new=Edited` 产生了一次 Core `edit_file` 审批，风险 **Medium**，固定在转录中并带有它的 audit id。 | [`04-edit-approval-pinned.txt`](tui/04-edit-approval-pinned.txt) |
| 5 | **C1 决策上下文** | 审批详情渲染的是 Core 的类型化 hunk，而不是工具输入：`README.md  Modified  +1 -1`、`@@ -1,5 +1,5 @@`、带新旧行号的逐行 `- # E1 Fixture` / `+ # E1 Edited`、未变的上下文行，以及基线说明 `computed against 65f130d7`。这正是 GUI-CORE-012 的全部意义：操作者批准的是一份 **diff**，不是一个字符串。 | [`05-edit-approval-hunks.txt`](tui/05-edit-approval-hunks.txt) |
| 6 | 编辑已应用 | `Allow once` 把它应用了。带外验证：`README.md` 第一行现在是 `# E1 Edited`，`git status --short` 报告 ` M README.md`。 | — |
| 7 | 检查运行 | **未运行。** 见下文缺陷：一次回合完成之后输入框就不再提交，因此同一会话内无法再发出第二次工具调用；而 GUI 自己的检查运行入口在锁屏之后不可达。 | — |
| 8 | 暂存 / 提交 / 推送 | **在发送任何东西之前就被拒绝。** 在有变更的工作区上打开 `/git`，渲染出 `TARGET workspace · main · ahead 0 behind 0 · dirty`，四行全部禁用并标注 `no workspace owner · GUI-CORE-027`。激活其中一行没有发出任何命令，这正是 T1a 所规定的行为。 | [`06-git-picker-refused.txt`](tui/06-git-picker-refused.txt) |
| 9 | 推送被拒 / 被接受 | **未触及。** 第 8 步的拒绝发生在两者之前。没有产生任何 `OperatorGitActionFinished`，因此 `Completed`、`Failed { NoUpstream }` 与 `Failed { RemoteUnreachable }` 在本次运行中**没有**被实地观察到，它们只存在于规范的 `operator-git.json` 重放中。裸 `origin` 远端因此从未被添加。 | — |
| 10 | EvidenceView | `/evidence` 在一个真实为空的归档上如实作答：`SCOPE whole archive · oldest first`、`FILTER every kind Core returns`、`No evidence in this scope.`、`LOADED 0 · archive complete`。这正是预期中的 GUI-CORE-028 状态：会话的转录里满是证据，而持久归档一条也没有。 | [`07-evidence-empty.txt`](tui/07-evidence-empty.txt) |
| 11 | 审计 | 审计时间线回答 `SCOPE project timeline · newest first`、`Core published no audit record for this scope.`、`LOADED 0 · nothing older matches`——而此前那次审批在屏幕上显示过一个 audit id。fixture 的运行时状态中不存在任何审计 JSONL。 | [`08-audit-empty.txt`](tui/08-audit-empty.txt) |

上表每一份截取都是运行中 TUI 的 `tmux capture-pane` 画面，在所述时刻取得，且在
列入之前都被逐一阅读过。采集主机的临时目录路径被替换成等宽占位符，以保持画面的
列对齐；除此之外没有任何编辑。

### 实地观察到的 Core 结果变体

- 携带 `DiffDocument` 的 `ApprovalRequestView.decision_context`：一个文件、
  一个 hunk、`+1 -1`、`base_sha256` 存在——实地观察到。
- 两次审批上的 allow-once `ApprovalDecision` 及其后的效果——实地观察到。
- 空且 `complete` 为真的 `EvidencePage`——实地观察到。
- 空且没有更早内容的 `AuditPage`——实地观察到。
- 客户端本地拒绝 `no workspace owner · GUI-CORE-027`，且未派发任何命令——实地
  观察到。
- `OperatorGitOutcome::{Completed, Failed}`——**未实地观察到**，仅有 fixture
  重放。

## 缺陷与缺口

以下没有一条在 E1 中被修复。每一条都是复现出来的，不是推断出来的，并且都已作为
带编号的开放后续项写入 `docs/core-0.3-compatibility.zh-CN.md`。

1. **会话级排队的后续输入永远不会被执行**（Core，阻断级）。
   `RuntimeCommand::QueueFollowUp` 压入 `SessionEngine::queued_runtime_inputs`，
   而没有任何地方移除或执行它；`InputDequeued` 的唯一生产者是 Lane worker 自己的
   队列。
2. **TUI 的输入框在一次会话余下的时间里不再提交**（TUI，阻断级）。
   `command_for_composer` 在 `state::runtime_has_active_work` 为真时一律入队，而
   该判定在以下情况为真：一次已完成的内置回合的文本仍留在 `assistant_stream` 中、
   任何 Lane 处于 `Draft`（Core 正是把 starter Lane 留在这个状态）、或
   `queued_inputs` 非空——按缺陷 1，这将永远为真。两种触发方式都实测到了：一次
   fallback 回合之后，以及创建一条 Lane 之后。GUI 自己的输入框判定以 owner 为
   作用域、基于 `turn_id` 与 Agent session 状态，并刻意排除 Lane 生命周期状态，
   因此 GUI 不受这一半影响；两个客户端对「忙」的定义并不一致。
3. **TUI 的 `/git` 选择器只可能到达工作区目标**（TUI）。Lane 的选中态绑定在 lane
   详情浮层的焦点上，而为了到达输入框（`/git` 在那里键入）离开该浮层就会清空它。
   结合 GUI-CORE-027，`runtime.operator_git` 在 TUI 上实际不可达。
4. **审批上的 audit id 不是一条持久审计记录**（Core）。持久时间线只由 trust loop
   与 operator git 动作追加，因此一次被批准并已应用的原生工具变更不会留下审计行，
   而屏幕上却显示了一个 audit id。
5. **由 supervisor 驱动的工作在持久证据归档中为空**（Core）。已决策并作为
   **GUI-CORE-028** 推迟到 `0.3.4`。
6. **EvidenceView 的报告可能在内容答案为 `HashMismatch` 的同时显示「已校验」**
   （GUI，表述问题但会误导）。两个不同的事实，界面没有一句话说明两者关系。

### 这对计划目标 4 意味着什么

`docs/release-0.3.3-plan.zh-CN.md` 的目标 4 是「一个真实的 local-first 开发任务
通过 GUI 从接入走到一次已提交的变更，并记录审计与证据」。诚实的表述是：

- 接入、在 Core 审批下创建 Lane、以及在 Core 的类型化决策上下文之上批准并应用一次
  对真实文件的变更——**已达成**，但走的是 TUI 而不是 GUI。
- 已提交的变更——**未达成**：`runtime.operator_git` 在任何命令被发出之前就被拒绝。
- 归档证据（GUI-CORE-028）与该次变更的持久审计记录（缺陷 4）——**未达成**。
- 通过原生 GUI 窗口——**未尝试**，因为宿主屏幕处于锁定状态。

## 原生 GUI 运行 —— 2026-09-10

E1 无法驱动原生窗口，因为宿主屏幕处于锁定状态。本节记录一次能够驱动它的重跑：分支
`claude/e1b-native-gui`，位于 `.worktrees/e1b-native-gui`，HEAD 为
`cd1d28f4a2702f17c80ba818ec963a7066d742d2` —— 与 `claude/int-0.3.3` 的 tip 是同一个
完整的 `0.3.3` 候选，因此上文的候选线没有变化。每一批按键之前都用
`CGSessionCopyCurrentDictionary()` 确认过屏幕未锁定。运行途中屏幕再次锁定，运行就
停在它停下的地方。下文没有任何一条是从 fixture 回放推断出来的。

### 被测应用

`npm --prefix apps/gui run tauri -- build --bundles app` 用时 1 分 48 秒 PASS，产出
`target/release/bundle/macos/Viden.app`：`CFBundleShortVersionString` 为
`0.1.0-rc.4`，`CFBundleIdentifier` 为 `dev.viden.gui`，`arm64`，ad-hoc
linker-signed 且没有 TeamIdentifier，可执行文件 38,946,048 字节。它从该 bundle 启动，
`VIDEN_HOME` 指向一个 scratch 目录，因此本仓库自己的 `.viden/` 从未被读取或写入。

### Fixture

运行 scratch 目录下一个全新的临时 Git 仓库：一个提交
`fc39694d4239b3fbad22c0968139dde55e21221b`（「Add the fixture README」）、一个
`README.md`，以及一个选择 `provider = "fallback"` / `model = "test-local"` 的
`.viden/config.toml`。裸仓库 `origin` 被刻意没有创建，因为它只在第一次 push 被拒绝
之后才添加——而本次运行没有走到那一步。

### 原生 Tauri 窗口如何驱动，以及遇到了什么

以下是关于驱动手段的事实，记录下来是因为下一次运行需要它们：

- 窗口在 `CGWindowListCopyWindowInfo` 里的 owner 名是 `Viden`，但辅助功能
  （Accessibility）进程名是 **`viden-gui`**。以 `process "Viden"` 寻址
  `System Events` 会报 `-1719`；这里每条命令都指向 `viden-gui`。
- `screencapture -x -o -l <window id>` 经常返回**上一次**状态变化之前的那一帧。命令
  面板已经出现在辅助功能树里时，连续两次截图仍显示的是上一个界面。因此下文每张截图
  都截了两次并读取第二帧；可靠的状态读取来自辅助功能树，而不是截图。
- `Open project folder` 面板由 `com.apple.appkit.xpc.openAndSavePanelService` 托管，
  而不是 `viden-gui`，所以 `System Events` 看不到它的窗口，它的 `Open` 按钮也无法通过
  辅助功能按下。它只能用按键驱动（`⌘⇧G`、路径、`Return`、`Return`）。
- 从本 shell 用 `CGEvent(... .leftMouseDown ...)` 投递的合成鼠标点击对窗口没有任何
  作用，因此指针输入只能用 `System Events` 自己的 `click at`，它是走辅助功能解析的。

### 逐步记录

| # | 步骤 | 结果 | 截图 |
| --- | --- | --- | --- |
| 1 | Welcome | 窗口在 Welcome 打开：`No project open`、一个带 `⌘O` 的 `Open project` 动作，以及 `Recent project history is unavailable · Core adapter is not connected`。状态栏 `MODE — · PERM — · CONTEXT — · EVENTS #0 · LANE — · DIAG 0× · REQ —`。 | [`01-welcome.png`](native/01-welcome.png) |
| 2 | Open Project | `⌘O` 打开原生的 `Open project folder` 面板；`⌘⇧G` 加 fixture 路径把它带到 fixture。 | [`02-open-panel-goto.png`](native/02-open-panel-goto.png) |
| 3 | 面板停在 fixture | 面板列出 fixture 的 `README.md`，`Open` 可用。 | [`03-open-panel-at-fixture.png`](native/03-open-panel-at-fixture.png) |
| 4 | **接入** | `Open` 绑定了工作区，Core 为它建立了 supervisor。座舱显示 Provider `fallback`、Model `test-local`、Mode `build`、Permission `ask`；Changes/Source 为 `Branch override main`、`Worktree` 是 fixture 路径、`Ahead 0`、`Behind 0`、`Dirty Clean`；标题栏 `⎇ 0 worktrees`；状态栏 `MODE build · PERM ask · EVENTS #7 · LANE — · REQ 0 req / 0 err`。带外验证：Core 在 scratch `VIDEN_HOME` 中写入了 `session_meta` 的 `canonical_root` = fixture 路径、`work_mode` `build`、`permission_mode` `default`、`model` `test-local`。 | [`04-cockpit-bound.png`](native/04-cockpit-bound.png) |
| 5 | 命令面板 | `⌘K` 打开了它。在已绑定项目且没有 Lane 的状态下，`ACTIONS` 组里只有一行 `Focus the composer`；`JUMP TO` 显示 `Cross-Lane gates and decisions are…` 不可用，`FILES` 显示 `Files unavailable · Core publishes no workspace file inventory`。**命令面板没有提供创建 Lane 的入口**，因此 New Lane 只能从活动栏进入。 | [`05-command-palette.png`](native/05-command-palette.png) |
| 6 | New Lane | **未走到。** 关闭命令面板之后，座舱把中间栏换成了 `CONNECTING · Establishing the versioned Core connection. · Core connection pending`，几秒后窗口消失，进程也不存在了。见缺陷 7。 | [`06-core-connection-pending.png`](native/06-core-connection-pending.png) |
| 7 | 选中 Lane、输入框、D2 hunks、DiffReview、Stage、Commit、Push、EvidenceView、D14 审计 | **未走到。** 第三次启动成功重复了第 1–4 步并停在那里：宿主屏幕在 Lane 被创建之前锁定（`CGSSessionScreenIsLocked = 1`），而向锁定会话发送按键是不允许的。 | — |

上表列出的每一张截图都在列出之前被读过。活动栏的 Lane 入口在第三次启动时已在辅助
功能树中定位到 —— `AXButton` `Lanes`，位于 (230, 248)，在 `Integration gate` 与
`Decisions` 之间 —— 但它从未被按下，因此这里不对它打开什么下任何结论。

### 在 GUI 中实际观察到的 Core 结果变体

- 工作区接入：Core 为所选根目录建立 supervisor，并写下上述持久 `session_meta`
  事实 —— 实机观察到。
- `RuntimeViewState` 的 `workspace_source` 为 `Ready`、分支 `main`、ahead 0、
  behind 0、clean，环境解析为 `fallback` / `test-local` —— 实机观察到。
- `ApprovalRequestView`、`ApprovalDecision`、`OperatorGitActionFinished`
  （`Completed` / `Failed { NoUpstream }` / `Failed { RemoteUnreachable }`）、
  `WorkspaceDiffPage`、`EvidencePage`、`AuditPage` —— 在 GUI 中**未观察到**。
  它们仍然只由上文的 TUI 运行（仅审批与决策）以及 fixture 回放覆盖。

### 本次运行的缺陷与观察

编号接续上文的列表。这里没有修复其中任何一项。

7. **在已绑定项目的情况下，GUI 窗口可能消失且进程退出**（GUI，对本次运行是阻塞
   性的）。在 `⌘K` 与 `Escape` 之后，座舱把中间栏换成了 `CONNECTING · Establishing
   the versioned Core connection. · Core connection pending`，标题栏的项目 chip 退回
   `—`；约十秒内窗口就从 `CGWindowListCopyWindowInfo` 中消失，进程也不再存在。
   `~/Library/Logs/DiagnosticReports` 中没有对应条目，进程合并后的 stdout/stderr
   日志是空的，所以这是一次退出而不是崩溃。三次启动中只在第二次见到；第三次启动在
   屏幕锁定时仍在运行。前端本身的诚实性在这里是对的 —— 它说的是 Core 连接待定，而
   不是继续显示过期事实 —— 但一个丢失 adapter 之后直接终止的客户端，会让操作者的
   会话在没有任何说明的情况下消失。
8. **Welcome 界面没有填满窗口**（GUI，观感问题）。它的内容与状态栏大约停在 525 px，
   窗口其余部分留空：在 800 px、900 px 与 640 px 三种窗口高度下都是如此，其中窗口
   尺寸没有改变布局的两张截图逐字节相同。已绑定的座舱能正确填满窗口，所以这是
   Welcome 的布局问题，不是外壳的问题。
9. **Welcome 把最近列表为空的原因写成 `Core adapter is not connected`**（GUI，措辞）。
   在那一刻 adapter 未连接，是因为还没有绑定项目，这是正常的首次启动状态，因此这句话
   读起来像故障，而 D1 自己的词汇会把它称作「还没有打开项目」。

有一条观察被记录下来但没有被称作缺陷，因为它无法复现。**第一次**启动时看到它绑定了
`/Users/wiki/Documents/GitHub/viden-test` —— 一个本次运行从未选择过的目录 —— 并且在
本次运行自己的 scratch `VIDEN_HOME` 里写下了指向它的 `session_meta` `canonical_root`。
`VIDEN_GUI_WORKSPACE` 并未设置，之后两次用全新 `VIDEN_HOME` 的启动都停在 Welcome，
直到被 `⌘O` 驱动才绑定任何东西。宿主是一台运行期间还有其他软件在使用的共享桌面，
因此这里按「原因不明」报告，而不归因于 GUI。

### 这对计划目标 4 意味着什么（按界面分别说明）

`docs/release-0.3.3-plan.zh-CN.md` 的目标 4 是「一个真实的 local-first 开发任务通过
GUI 从接入走到一次已提交的变更，并记录审计与证据」。结合本次运行更新上文的表述：

- **通过原生 GUI 窗口接入 —— 已达成。** 窗口打开了，原生文件夹面板绑定了一个真实
  项目，Core 为它建立的 supervisor 写下的持久会话事实指名了那个项目。
- **通过原生 GUI 窗口完成 Lane 创建、被批准的变更、DiffReview、Stage、Commit、
  Push、EvidenceView 与 D14 审计时间线 —— 未走到**，原因是上表中两个环境性原因
  （一次原因不明的进程退出，然后是宿主屏幕锁定），而不是契约或能力上的原因。因此
  E1 的那句「未通过原生 GUI 窗口尝试，因为宿主屏幕处于锁定状态」被替换为：已尝试，
  接入已达成，其余未走到。
- 上文关于 TUI 的表述不变：在 Core 审批下创建 Lane、以及在 Core 的类型化决策上下文
  之上批准一次变更，在那里是已达成的；已提交的变更、归档证据与持久审计记录不是。
- **目前还没有任何一个界面把整个任务端到端走通。** 目标 4 仍未达成，其中已达成的
  部分是在 TUI 上达成的。

## 边界声明

这是一个本地候选。候选版本 Core `0.3.6`、TUI `0.3.4`、GUI `0.1.0-rc.4` 只存在于
本地分支 `claude/e1-release-evidence` 上，而该分支基于本地集成分支
`claude/int-0.3.3`。本文中的任何内容都没有被发布、代码签名、公证、推送、合并、
打 tag、为任何平台打包、同步到 Homebrew tap，也没有对着 live provider 做过认证。
上文提到的 macOS `.app` 是本地构建产物，既未安装也未分发。release gate 的
prepublish 阶段没有运行。
