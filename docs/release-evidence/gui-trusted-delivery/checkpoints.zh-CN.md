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
   **已由 C6（Core）于 2026-09-12 修复；客户端在 G7/T2 中接入。**
   `RuntimeCommand::QueueFollowUp` 曾压入 `SessionEngine::queued_runtime_inputs`，
   而没有任何地方移除或执行它；`InputDequeued` 当时的唯一生产者是 Lane worker
   自己的队列。`runtime.turn_lifecycle` 为每一次原生回合发布终结事实，并在一次
   已完成的回合之后按最旧优先排空会话队列：每一条先由 `InputDequeued` 宣告，再
   作为自带起止括号的回合运行。失败或被取消的回合则保留队列。GUI 输入框与 TUI
   的活动工作判定将在 G7 与 T2 中改读 `active_turns`，而不再读显示残留。
2. **TUI 的输入框在一次会话余下的时间里不再提交**（TUI，阻断级）。
   **已由 T1c（2026-09-10）修复，并由 T2（2026-09-12）补全。** T1c 拆分了判定并
   让路由以 owner 为作用域，消除了下述三个成因；但原生路径仍需要一个客户端侧的
   活跃性窗口，因为 Core 当时不为它发布任何终结事实。T2 删除了那个窗口：
   `runtime.turn_lifecycle`（C6）为每个回合加上起止括号，因此
   `state::composer_target_busy` 按本次输入所指的作用域读取
   `RuntimeViewState.active_turns`，本客户端不再持有任何窗口。两个情形都已固定为
   测试，并于 2026-09-12 再次实机走通
   （`docs/release-tui-0.3.4-source-control-parity.zh-CN.md`）。原始发现如下。
   `command_for_composer` 在 `state::runtime_has_active_work` 为真时一律入队，而
   该判定在以下情况为真：一次已完成的内置回合的文本仍留在 `assistant_stream` 中、
   任何 Lane 处于 `Draft`（Core 正是把 starter Lane 留在这个状态）、或
   `queued_inputs` 非空——按缺陷 1，这将永远为真。两种触发方式都实测到了：一次
   fallback 回合之后，以及创建一条 Lane 之后。GUI 自己的输入框判定以 owner 为
   作用域、基于 `turn_id` 与 Agent session 状态，并刻意排除 Lane 生命周期状态，
   因此 GUI 不受这一半影响；两个客户端对「忙」的定义并不一致。
3. **TUI 的 `/git` 选择器只可能到达工作区目标**（TUI）。
   **已由 T1c（2026-09-10）修复，并由 C5 加 T2（2026-09-12）补全，但留有一个
   Core 侧缺口。** T1c 把 `lane_detail_open` 与 `focused_lane` 分离、增加了 `Esc`
   回退级，使 `runtime.operator_git` 对 Lane 可达。T2 采纳
   `runtime.workspace_owner`（C5），因此工作区目标在 Core 发布的 owner 之下发送，
   选择器的 TARGET 行也读取各目标自己的源。缺口是：`apps/cli` 直接引导 engine，
   而不经过 `LocalCoreHost::open_workspace`——它是
   `SessionEngine::bind_workspace_owner` 在生产代码中的唯一调用方——所以 `viden`
   的 TUI 会话仍看到 `workspace_owner` 缺失并显示拒绝；该项记为兼容性跟进项 11，
   E2 需要它。GUI 不受影响，因为它的适配层经由该 host 打开工作区。原始发现如下。
   Lane 的选中态绑定在 lane 详情浮层的焦点上，而为了到达输入框（`/git` 在那里
   键入）离开该浮层就会清空它。结合 GUI-CORE-027，`runtime.operator_git` 在 TUI 上
   实际不可达。
4. **审批上的 audit id 不是一条持久审计记录**（Core）。**已由 C7（Core）于
   2026-09-12 修复；客户端在 G7/T2 中采纳。** 持久时间线此前只由 trust loop 与
   operator git 动作追加，因此一次被批准并已应用的原生工具变更不会留下审计行，
   而屏幕上却显示了一个 audit id。`RespondToApproval` 现在会在 `ApprovalResolved`
   之前追加一条 `AuditRecord`，使用的正是那个预铸 id，actor 为 `Operator`，action
   为 `approval.<allow_once|allow_session|allow_repo|deny>`，objects 指名该审批
   请求、工具，以及该决定释放的作业，因此 `QueryAudit` 能解析客户端早已被展示的
   那个 id。
5. **由 supervisor 驱动的工作在持久证据归档中为空**（Core）。**已由 C7（Core）于
   2026-09-12 修复；客户端在 G7/T2 中采纳。** 记录为 **GUI-CORE-028**，现已在
   Core 侧关闭。一次被应用的原生变更会归档一条带规范 ContextStore 字节的 `patch`
   行，适配器上报的补丁由运行时的摄取完成规范化，监督者把每个终结回合批次交给
   `SessionEngine::absorb_supervised_events`，使这些行进入归档据以重建的
   `runtime_projection`。一个重启测试重放了该行并提供了它校验通过的字节。
6. **EvidenceView 的报告可能在内容答案为 `HashMismatch` 的同时显示「已校验」**
   （GUI，表述问题但会误导）。**已由 H2 于 2026-09-12 修复。** 两个不同的事实，
   界面原先没有一句话说明两者关系。现在报告用一句话把两者联系起来 —— 归档行当初
   记录下的判定，与 Core 刚刚做的那次读取 —— 并给那条记录加上设计中的「不一致」
   处理（`var(--error)` 加删除线，因此状态不只靠颜色表达），同时把它作为事实继续
   留在界面上。`failed` 与 `HashMismatch` 是两个彼此一致的事实，不会产生告警；为
   另一行回显的内容答案也不会与本行冲突。两种到达顺序都有覆盖
   （`apps/gui/tests/evidence_view.spec.ts`），截图为
   `evidence-hash-mismatch-1440x900-dark-en.png`。

### 这对计划目标 4 意味着什么

`docs/release-0.3.3-plan.zh-CN.md` 的目标 4 是「一个真实的 local-first 开发任务
通过 GUI 从接入走到一次已提交的变更，并记录审计与证据」。诚实的表述是：

- 接入、在 Core 审批下创建 Lane、以及在 Core 的类型化决策上下文之上批准并应用一次
  对真实文件的变更——**已达成**，但走的是 TUI 而不是 GUI。
- 已提交的变更——**未达成**：`runtime.operator_git` 在任何命令被发出之前就被拒绝。
- 归档证据（GUI-CORE-028）与该次变更的持久审计记录（缺陷 4）——**当时未达成**。
  两处 Core 侧的问题均已于 2026-09-12 由 C7 修复；目标本身由 E2 重新取证，因为
  两个客户端都尚未采纳。
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
   性的）。**H2 于 2026-09-12 修复了其中可见的一半；进程退出本身未能复现。**
   可见的那一半在外壳接缝处复现了，原因是控制器泄漏而不是 adapter 丢失：
   `renderD1Cockpit` 把 `⌘K`、`⌘L`、`⌘.`、`⌘G`、`⌘E`、`⌘R`、`⌘O` 与 `Escape`
   注册在 `window` 上并一直持有到被 dispose，而 `bootstrapShell` 丢弃了自己的
   控制器，于是水合前的那层外壳在活动座舱下面继续响应这些快捷键，打开第二个命令
   面板，并把它自己的 `connecting` 投影重新渲染进同一个根节点 —— 项目 chip 退回
   `—`，`Core connection pending` 重新出现。最后运行的那个处理器赢得绘制，这正是
   它只在三次启动中出现一次的原因。现在每一次座舱挂载都经过 `claimRoot`
   （`apps/gui/src/main.ts`），因此只有一个控制器持有这些快捷键；
   `apps/gui/tests/adapter_drop.spec.ts` 对已绑定的宿主按下这些快捷键，并断言只有
   一个命令面板、且不出现 `Core connection pending`。

   **进程退出未能复现，代码中也不存在这样的路径。** 已尝试的事项：对 GUI 链接的
   全部 Rust 源码检索 `process::exit` / `process::abort` / `libc::exit`（仅命中
   `apps/cli/src/main.rs` 以及 `crates/plugin-host` 中以字符串嵌入的测试辅助程序，
   两者都不在桌面客户端的可达路径上）；通读 `apps/gui/src-tauri/src/lib.rs` 的命令
   层与 `spawn_core_event_pump`，后者唯一的退出口是被污染的 adapter 锁，并且结束的
   是它自己的线程而不是进程；通读命令面板的关闭路径，它不会重新发起 Core 连接（已
   有断言）；以及在两个接缝上实际驱动复现 —— `apps/gui/tests/reconnect.rs` 在传输
   断开后抽取十六轮事件，断言 adapter 保留 Core 最后发布的视图、把状态分类为
   `Disconnected`、阻断业务成功，并给出契约已建模的重连动作；
   `apps/gui/tests/adapter_drop.spec.ts` 让宿主在绑定之后拒绝每一次读取，断言座舱
   仍挂载在最后的事实上，且绝不把传输层的那句话当成 Core 事实绘制出来。真正会结束
   进程的机制只有 Tauri 自己的：在 macOS 上，最后一个窗口关闭时应用就会退出。窗口
   为何关闭仍然原因不明；本次运行自己记下的「宿主是一台还有其他软件在使用的共享
   桌面」这一点并未被排除。在 `claimRoot` 之后，这一项值得在 E2 中用原生方式重新
   驱动一次。

   以下是原始观察，未作改动。在 `⌘K` 与 `Escape` 之后，座舱把中间栏换成了 `CONNECTING · Establishing
   the versioned Core connection. · Core connection pending`，标题栏的项目 chip 退回
   `—`；约十秒内窗口就从 `CGWindowListCopyWindowInfo` 中消失，进程也不再存在。
   `~/Library/Logs/DiagnosticReports` 中没有对应条目，进程合并后的 stdout/stderr
   日志是空的，所以这是一次退出而不是崩溃。三次启动中只在第二次见到；第三次启动在
   屏幕锁定时仍在运行。前端本身的诚实性在这里是对的 —— 它说的是 Core 连接待定，而
   不是继续显示过期事实 —— 但一个丢失 adapter 之后直接终止的客户端，会让操作者的
   会话在没有任何说明的情况下消失。
8. **Welcome 界面没有填满窗口**（GUI，观感问题）。**已由 H2 于 2026-09-12 修复。**
   先复现：在 qa 工装里强制出打包后的实际层叠顺序 —— `gui-kit.css` 的 `.frame`
   flex 列在 `display` 上压过 `.d1-frame` 的 grid，且 `.d1-body` 处于初始的
   `flex: 0 1 auto` —— 在 768 px 高的窗口里内容与状态栏停在 540 px，正是报告中的
   形状。外壳层那一半（`.d1-body { flex: 1 1 auto }`）已随导航外壳落地；H2 把
   Welcome 所在的中间栏改成「填充轨道」而不是「百分比」：`.d1-main-welcome` 移到
   `.d1-main` 之后，从而真正赢得它原先默默输掉的同权重之争，工作面本身成为单轨
   grid，Welcome 作为 grid 项被拉伸填满。全程没有任何像素高度。
   `apps/gui/tests/welcome_fill.spec.ts` 按打包顺序加载真实样式表，并在 640、800、
   900 三个高度上断言计算后的整条链路；它的十六个用例中有九个在 `25072a0a` 的 CSS
   上失败。截图：`welcome-fill-1440x900-dark-en.png` 与
   `welcome-fill-1440x640-dark-en.png`。

   以下是原始观察，未作改动。它的内容与状态栏大约停在 525 px，
   窗口其余部分留空：在 800 px、900 px 与 640 px 三种窗口高度下都是如此，其中窗口
   尺寸没有改变布局的两张截图逐字节相同。已绑定的座舱能正确填满窗口，所以这是
   Welcome 的布局问题，不是外壳的问题。
9. **Welcome 把最近列表为空的原因写成 `Core adapter is not connected`**（GUI，措辞）。
   **已由 H2 于 2026-09-12 修复。** Welcome 只在没有绑定工作区时渲染，因此最近工作
   读取失败时现在以 D1 自己的「尚未打开项目」作为首句，并说明下一步；宿主原本那句话
   作为诊断保留在下方 —— 两个事实都不隐藏。若 Core 未发布 `runtime.recent_work`
   能力，则仍用它自己的措辞，因为那无论是否打开项目都是关于 Core 的事实。
   `Core adapter is not connected` 只在已绑定工作区的界面上继续作为首句，那里由 D6
   连接状态负责说明。双语均已更新；截图为 `welcome-fill-1440x900-dark-en.png`。

   以下是原始观察，未作改动。在那一刻 adapter 未连接，是因为还没有绑定项目，这是正常的首次启动状态，因此这句话
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

# 0.3.4 可信交付收尾（E2）

日期：2026-09-12

上面的全部内容是 `0.3.3` 的记录，不做改写。本部分是 `0.3.4` 的发布步骤。与上半部分
一样，它描述的只是一个**本地候选**：本文中没有任何内容被发布、签名、公证、推送、
合并、打 tag，或对着 live provider 做过认证。

## 候选线

| 项 | SHA / 路径 |
| --- | --- |
| 基线 `origin/main` | `25072a0acee7a9959bfd8060721378cb3d4d5397` |
| 集成分支 | `claude/int-0.3.4`，位于 `39ed155dcc1847b915965f49626e6779b8b538d7`，领先 `main` 68 个提交 |
| E2 分支 / worktree | `claude/e2-release-evidence`，位于 `.worktrees/e2-release-evidence`，从该 SHA 创建 |
| Core `0.3.7` 契约 checkpoint | `39ed155dcc1847b915965f49626e6779b8b538d7` —— 所有 `0.3.4` 批次都已落地其上的集成分支 tip，也是 `crates/core/release-manifest.toml` 现在记录的 `contract_implementation_checkpoint` |
| Core `0.3.7` 基础契约 checkpoint | `5bd2b80b0953f4194d082940a7b9164c7231ca2d`，自 `0.3.0` 起未变 |
| TUI 候选 | `0.3.5`；`min_core_version` 有意保持在 `0.3.4` |
| GUI 候选 | `0.1.0-rc.5`；`[core].minimum_version` 有意保持在 `0.3.5` |
| 前端 schema | `1`，未变 |
| 能力 | 15 个冻结基础能力 + 29 个扩展能力 |

三条线各自独立推进。Core 前进是因为 `0.3.4` 增量在 C5 到 C9 中新增了六个能力
（23 到 29）；两个客户端前进是因为它们都完成了适配（G7、T2）。

为什么 checkpoint 在这里声明而不是逐批声明：每个 Core 批次只添加自己的 fixture 行，
而不动 `component_version` 和 checkpoint —— 因为在契约还在移动时命名一个
checkpoint，命名的是一个并不存在的契约。如果 `claude/int-0.3.4` 在合并前被 rebase，
这个 checkpoint 必须针对新的 SHA 重新声明，而不能假定它自动存续。

### 冻结基础 fixture 的字节

九个冻结的 `frontend-contract-v1` 基础 fixture 做了三方比对：本 worktree 磁盘上的
字节、`git show 25072a0a:<path>` 的字节，以及 `apps/tui/release-manifest.toml` 中
固定的摘要。九个全部三方一致；在整个 `0.3.4` 增量中没有一个移动过。

| Fixture | sha256（worktree = `25072a0a` = 固定值） |
| --- | --- |
| `approval-allow-deny.json` | `a31d8c64…8700248e` |
| `context-pressure-cost-blind.json` | `dbae6f87…e270d697` |
| `d1-vertical-slice.json` | `d8dc7a14…248a71df5e` |
| `dag-blocker.json` | `98e2ad2b…bc37c85e5c7` |
| `merge-gate.json` | `d71807ab…d50365becf11` |
| `multi-lane.json` | `1a20e8ec…90788331fc5cd` |
| `plan-denial.json` | `6a04b5ef…d55d783b307` |
| `queued-follow-up.json` | `83c31272…7c6892299031` |
| `stream-tool.json` | `a097f17d…9951c9fd548a` |

### 确定性证据

本候选的完整 gate 表 —— 把 `viden-plugin-host` 与 `viden-agents` 串行重跑的工作区
套件、fmt、clippy、依赖边界、TUI 回归与两个 smoke、GUI 的 vitest 与构建、投影截取、
Tauri 应用包，以及文档配对/链接检查，并说明哪些被刻意不运行及原因 —— 在
[release-0.3.4-report.zh-CN.md](../../release-0.3.4-report.zh-CN.md) 的「Gate」一节。
它只保存在一处而不在此复制，因为一张 gate 表的两份副本会漂移。

## 原生 GUI 运行 —— 2026-09-12（0.3.4）

**未尝试：宿主屏幕在 14:37:21Z 处于锁定状态**，早于本批次任何工作开始；到 14:45:30Z
应用包构建完成时仍然锁定。两次检查中 `CGSessionCopyCurrentDictionary()` 都返回
`CGSSessionScreenIsLocked = 1` 且 `kCGSSessionOnConsoleKey = 1`。没有向窗口发送任何
按键或点击，也没有为驱动而启动应用，因为锁定的会话会把输入路由到登录窗口 —— 这与
E1 和 E1b 遵循的是同一条规则，也是 E1b 停在原处的原因。解锁这台 Mac 需要输入用户
密码，本次运行不得输入。

因此本节记录的是在没有显示器的情况下能够确立的事实，并明确说明不能确立的部分。
下文没有任何内容是从 fixture 回放推断出来的，也没有对任何未被驱动的界面作出断言。

### 被测应用

`npm --prefix apps/gui run tauri -- build --bundles app` 在 1 分 10 秒内通过
（14:43:54Z 到 14:45:04Z），产出
`target/release/bundle/macos/Viden.app`：

| 事实 | 值 |
| --- | --- |
| `CFBundleShortVersionString` | `0.1.0-rc.5` |
| `CFBundleVersion` | `0.1.0-rc.5` |
| `CFBundleIdentifier` | `dev.viden.gui` |
| `CFBundleExecutable` | `viden-gui` |
| 架构 | `Mach-O 64-bit executable arm64` |
| 可执行文件大小 | 40,655,440 字节 |
| 签名 | ad-hoc，linker-signed，`TeamIdentifier=not set`，`Sealed Resources=none` |

构建日志中出现 `Compiling viden-gui v0.1.0-rc.5`，所以这个包带的是本批次的版本号，
而不是它替换掉的 rc.4。它是一个本地构建产物：没有被安装、分发、用身份签名、公证，
也没有被启动。

### 哪些步骤未走到，以及为什么

| 步骤 | 状态 |
| --- | --- |
| Welcome、`⌘O` 绑定、驾驶舱绑定、Lane 标签条、`⌘L` 新建 Lane、Core 审批下创建 Lane、composer 变更、D2 hunk、内联工具 diff、check-run 块、EvidenceView 归档行、D14 审计行、排队追加提示的排空、带 Lane 与不带 Lane 的 DiffReview 提交、push 被拒后被接受、dock Files 页签、聚焦模式 `⌘.`、`Esc` 返回 | **未尝试。** 宿主屏幕锁定；见上文。本次未拍摄任何原生截图，所以本节不列任何 PNG。 |

因此 GUI 自身对这些界面的证据，是 G3 到 G7 批次在 `apps/gui/evidence/` 下拍摄的
确定性 harness 截图及其 `EVIDENCE.md` 行，加上 vitest 与 projection 套件 —— 而不是
一个活的窗口。这对**集成**来说是更弱的证据，本文不作相反的暗示：「驾驶舱原生地承载
了整个任务」这句话**仍然未被证明**，这已经是连续第三个发布步骤如此，且每次都是
环境原因。

### 原封不动沿用的 harness 事实

再次记录，因为下一次运行需要它们，而本次运行既无法重新验证也无法否证它们：

- 窗口的 `CGWindowListCopyWindowInfo` owner 名是 `Viden`，但 Accessibility 进程名是
  `viden-gui`；用 `process "Viden"` 寻址 `System Events` 会抛出 `-1719`；
- `screencapture -x -o -l <window id>` 经常返回**上一次**状态变化之前的帧，所以每次
  截图必须拍两次并读第二帧，并以 Accessibility 树作为可靠的状态读取源；
- `Open project folder` 面板由
  `com.apple.appkit.xpc.openAndSavePanelService` 托管，所以它的 `Open` 按钮无法通过
  Accessibility 触达，该面板只能用按键驱动（`⌘⇧G`、路径、`Return`、`Return`）；
- 用 `CGEvent` 合成的鼠标点击无效；指针输入必须走 `System Events` 自己的
  `click at`。

## TUI 交叉核对 —— 2026-09-12

这是能够发生的那次运行：离线，在 140×40 的 `tmux` 中，使用
`cargo run -p viden-cli -- --provider fallback --model test-local`（构建出的二进制
`target/debug/viden`，版本横幅 `v0.3.5`），对着一个临时 Git 仓库运行，`VIDEN_HOME`
指向一个临时目录。本仓库自己的 `.viden/` 从未被读取或写入。

它是交叉核对，不是替代：TUI 与 GUI 是同一份契约的两个客户端，所以本次运行观察到的
Core 事实就是 Core 事实；但本次运行没有触碰的 GUI **界面**仍然没有证据。

### Fixture

运行临时目录下的一个全新 Git 仓库：一个提交
`8b5cf4990d4818bd6089a7fa751f647aeec9fabd`（"Add the fixture README"）、一个
`README.md`、一个覆盖 `.viden/` 与 `.worktrees/` 的 `.gitignore`，以及一个选择
`provider = "fallback"` / `model = "test-local"` 的 `.viden/config.toml`。裸仓库
`origin` 有意在第一次 push 被拒**之后**才创建。`fallback` provider 会把形如
`tool <name> key=value …` 的用户消息转成一次真实的工具调用
（`crates/provider/src/fallback.rs`，`parse_explicit_tool_call`），这就是在没有
live 模型的情况下发生真实变更的方式。

### 逐步记录

下面每一帧都是运行中 TUI 的 `tmux capture-pane`，在所描述的那一刻拍摄，并且每一帧
都在被列出之前被读过。截图宿主的临时目录路径被替换成等宽占位符，以保持帧的列对齐；
除此之外没有任何编辑。

| # | 步骤 | 结果 | 截图 |
| --- | --- | --- | --- |
| 1 | 接入 | TUI 把 fixture 作为工作区打开，并从项目配置解析出 `fallback` / `test-local`。`0 lanes`，版本 `v0.3.5`。Core 在 bootstrap 时铸造了 `.viden/project.toml` `[project] id = prj_1789224423279254000` —— 这就是 C10 的绑定点，在磁盘上可见。 | [`01-welcome.txt`](tui-0.3.4/01-welcome.txt) |
| 2 | **未选中 Lane 时的 `/git`** | 四行全部可选，`TARGET workspace · main · ahead 0 behind 0 · clean`。E1 看到这几行是禁用的并标注 `no workspace owner · GUI-CORE-027`，而 T2 自己的实地检查也仍然无法启用它们，因为 `apps/cli` 没有绑定 owner。C5 发布了这个身份，C10 把绑定移进了共享 bootstrap；这一帧是两者的第一份实地 TUI 证据。 | [`02-git-picker-workspace-enabled.txt`](tui-0.3.4/02-git-picker-workspace-enabled.txt) |
| 3 | 新建 Lane | `n` 打开了 `NEW NATIVE LANE`；第一条任务描述发布了一个 Core `lane_create` 审批，风险 **Medium**，`TARGET` 是工作区根目录，`INPUT` 指明分支与 worktree 路径，`AUDIT audit_1789224503187620000`，带 `1 Allow once` / `2 Allow for session · unavailable` / `3 Add repo allowlist` / `4 Deny` 以及 `auto-deny @1789224803 · default Deny` 过期项。 | [`03-new-lane-overlay.txt`](tui-0.3.4/03-new-lane-overlay.txt)、[`04-lane-create-approval.txt`](tui-0.3.4/04-lane-create-approval.txt)、[`06-lane-create-approval-detail.txt`](tui-0.3.4/06-lane-create-approval-detail.txt) |
| 4 | Lane 已创建 | `Allow once` 创建了 Lane，路由 `main→side-1`，状态 `Draft`。带外验证：`git worktree list` 显示 `.worktrees/lane_1789224502859401000`，`git branch` 显示 `viden/lane_1789224502859401000`。 | [`07-lane-created.txt`](tui-0.3.4/07-lane-created.txt) |
| 5 | 放弃 Lane 目标 | 两级 `Esc` 分别关闭详情面板并清除目标，各自以自己的 system 行宣告（`Cleared the Lane target … /git now names the workspace.`），`L:` 回到 `-`。本次运行余下部分有意是工作区作用域的：这正是本里程碑关心的「没有 Lane 也能交付」。 | [`08-edit-approval-pinned.txt`](tui-0.3.4/08-edit-approval-pinned.txt) |
| 6 | composer 变更 | `tool edit_file path=README.md old=Fixture new=Edited` 产生了一个 Core `edit_file` 审批，风险 **Medium**，以 `AUDIT audit_1789224570047389000` 常驻。常驻面板按设计显示工具输入。 | [`08-edit-approval-pinned.txt`](tui-0.3.4/08-edit-approval-pinned.txt) |
| 7 | **C1 决策上下文** | 审批详情渲染的是 Core 的类型化 hunk，而不是工具输入：`README.md  Modified  +1 -1`、`@@ -1,6 +1,6 @@`、带逐行旧/新行号的 `- # E2 Fixture` / `+ # E2 Edited`、四行未变上下文，以及基线说明 `computed against ed2e9faf`。操作者批准的是一份 diff，不是一个字符串。 | [`09-edit-approval-hunks.txt`](tui-0.3.4/09-edit-approval-hunks.txt) |
| 8 | 变更已应用 | `Allow once` 应用了它。带外验证：`README.md` 第一行是 `# E2 Edited`，`git status --short` 报告 ` M README.md`。 | [`10-edit-applied.txt`](tui-0.3.4/10-edit-applied.txt) |
| 9 | **EvidenceView：一条归档 patch** | `/evidence` 回答 `LOADED 1 · archive complete`，内容为 `14:49:58 [patch] native session edit: README.md (+1/-1) · no lane`。这正是 E1 无法产生、T2 也无法产生的那一行；GUI-CORE-028 与 E1 缺陷 5 现在是实地证据，而不是 fixture 回放。 | [`11-evidence-archive.txt`](tui-0.3.4/11-evidence-archive.txt) |
| 10 | **规范字节** | 打开该行后显示 `ID patch-tool_1789224570013118000`、`OWNER workspace=ws_d449023423e1a290 project=prj_1789224423279254000`、`SOURCE native`、`PATH README.md`、一个 `CANONICAL` item/bundle 引用、`HASH d46176df`、`PRODUCER native · coder · task turn_1789224569891144000`、`APPROVAL audit audit_1789224570047389000`、`RECORD Core verified the canonical reference: verified` 与 `DIFF verified against d46176df`，其下是 Core 自己解析出的行。带外验证：ContextStore blob 为 132 字节，其 sha256 是 `d46176df6e9abd082f0d16ec270ff0d20e158c42ef67fc450b2b163c86fd95cb` —— 屏幕上的哈希就是磁盘字节的哈希。此处有一个标注缺陷，见缺陷 10。 | [`12-evidence-detail-canonical.txt`](tui-0.3.4/12-evidence-detail-canonical.txt) |
| 11 | **D14：审批成为持久审计行** | 审计时间线回答 `SCOPE project timeline · newest first`、`14:49:58 approval.allow_once ✓ permission:approval_1789224570047386000`、`LOADED 1 · nothing older matches`。在 `.viden/workflows/projects/*/audit.jsonl` 中带外验证：actor `operator`、action `approval.allow_once`、objects `permission:approval_…`、`tool:edit_file`、`job:tui-7`、outcome `success`，记在 `audit_1789224570047389000` 之下 —— 正是审批此前已经显示过的那个 id。E1 缺陷 4 在这条路径上已闭环；还剩一个缺口，见缺陷 11。 | [`13-audit-approval-rows.txt`](tui-0.3.4/13-audit-approval-rows.txt) |
| 12 | Stage | 在 dirty 工作区上 `/git`，`Stage all changes` → 一个 Core `git_add` 审批，风险 **Low**，`TARGET git_add (workspace)`。`Allow once` 完成暂存；`git status --short` 报告 `M  README.md`。 | [`14-git-picker-dirty.txt`](tui-0.3.4/14-git-picker-dirty.txt)、[`15-stage-approval.txt`](tui-0.3.4/15-stage-approval.txt)、[`16-staged.txt`](tui-0.3.4/16-staged.txt) |
| 13 | **Commit** | `Commit…` 打开消息输入；提交消息产生了一个 Core `git_commit` 审批，风险 **Medium**，`TARGET git_commit (workspace)`。`Allow once` 完成提交：`Commit completed · main · ahead 0 behind 0 · clean · OUTPUT 2 lines · [main b9f393e] Edit the fixture README heading AUDIT audit_1789224929400408000`。带外验证：`b9f393e Edit the fixture README heading`，工作树干净。**`OperatorGitOutcome::Completed` 首次被实地观察到**，且未选中任何 Lane，运行在 Core 发布的工作区 owner 之下。 | [`17-commit-message-prompt.txt`](tui-0.3.4/17-commit-message-prompt.txt)、[`18-commit-approval.txt`](tui-0.3.4/18-commit-approval.txt)、[`19-committed.txt`](tui-0.3.4/19-committed.txt) |
| 14 | **Push 被拒** | `Push` → 一个 Core `git_push` 审批，风险 **High**。`Allow once` 得到 `Push failed · this branch has no upstream · push again with set upstream · the current branch has no upstream branch; push with set_upstream to create one AUDIT audit_1789224970080956000`。**`OperatorGitOutcome::Failed { NoUpstream }` 首次被实地观察到。** 此时还不存在任何 remote。 | [`20-push-refused-noupstream.txt`](tui-0.3.4/20-push-refused-noupstream.txt)、[`21-push-noupstream-outcome.txt`](tui-0.3.4/21-push-noupstream-outcome.txt) |
| 15 | 加入裸 `origin` | 在临时目录下 `git init --bare` 并 `git remote add origin`，带外执行，在被拒**之后**而非之前。 | —— |
| 16 | **Push 被接受** | `Push completed · main · ahead 0 behind 0 · clean · OUTPUT 2 lines · To …/e2-origin.git AUDIT audit_1789225169647993000`。带外验证：裸 `origin` 现在持有 `b9f393e`，本地分支读作 `## main...origin/main` 且无分歧。**push 的 `OperatorGitOutcome::Completed` 被实地观察到。** 一个诚实的限定：分支的 upstream tracking ref 必须带外配置，因为 TUI 选择器不提供 set-upstream 控件 —— 见缺陷 12。push 本身是 Core 在一次操作者审批之下执行的，并且把一个真实提交送进了一个真实 remote。 | [`22-git-picker-after-remote.txt`](tui-0.3.4/22-git-picker-after-remote.txt)、[`23-push-approval.txt`](tui-0.3.4/23-push-approval.txt)、[`24-push-accepted.txt`](tui-0.3.4/24-push-accepted.txt) |
| 17 | **排队的追加提示被排空** | 第二次 `tool edit_file` 让该轮次停在它的审批上：composer 切换为 `[^J Queue]`，状态行切换为 `ACTIVE`，两者都来自 Core 的 `active_turns` 而不是显示残留。在那里提交的追加提示被接受，并且只在第一轮结束后才运行。在会话 JSONL 中带外验证：`turn_owner` 在 `1789225244` 被清空（第一轮的 `TurnFinished`），同一秒内 `turn_owner` 再次被设置，随后是用户消息 `the queued follow-up for E2` 及其 assistant 回复，然后 `turn_owner` 再次清空。该提示等待了约 39 秒，并作为自己的成对轮次在一个已完成轮次之后运行 —— 这是 C6 的会话队列排空，也是 T2 自己的实地检查无法展示的部分。此处有一个接缝观察，见缺陷 13。 | [`25-followup-queued.txt`](tui-0.3.4/25-followup-queued.txt)、[`26-followup-drained.txt`](tui-0.3.4/26-followup-drained.txt) |
| 18 | 运行结束后的归档与时间线 | `LOADED 2 · archive complete`（第二条 patch 是 `+0/-0`，因为 fallback 的工具输入解析器按空格切分，第二次变更解析成了一次空操作）。审计时间线持有十二行：四条 `approval.allow_once`，以及 `source.stage`、`source.commit` 和两次 push 各自的 `authorized` 与 `completed`/`failed` 配对，其中 `14:56:10 source.push ✗ … attempt=audit_1789224970080956` 与它的授权行并列。 | [`27-evidence-two-patches.txt`](tui-0.3.4/27-evidence-two-patches.txt)、[`28-audit-timeline-full.txt`](tui-0.3.4/28-audit-timeline-full.txt)、[`29-audit-timeline-oldest.txt`](tui-0.3.4/29-audit-timeline-oldest.txt) |

### 实地观察到的 Core 结果变体

- `apps/cli` 路径上的 `WorkspaceRuntimeOwnerBound`，带铸造出的
  `.viden/project.toml` id —— 实地观察到（C5 + C10）。
- 在该 owner 之下、未选中任何 Lane 时被授权的
  `RunOperatorGitAction { target: Workspace }` —— 实地观察到（C5）。
- `stage`、`commit` 与 `push` 的 `OperatorGitOutcome::Completed` —— 实地观察到。
  `OperatorGitOutcome::Failed { NoUpstream }` —— 实地观察到。在本次运行之前，这三者
  只存在于 `operator-git.json` 回放中。
- 携带 `DiffDocument` 的 `ApprovalRequestView.decision_context`（一个文件、一个
  hunk、`+1 -1`、`base_sha256` 存在）—— 实地观察到。
- 五次审批上的 allow-once `ApprovalDecision`，每一次之后都跟着它的效果 ——
  实地观察到。
- **非空**的 `EvidencePage`，其中一条 `patch` 行的规范引用被 Core 验证过，且其字节
  哈希等于屏幕上的值 —— 实地观察到（C7）。
- **非空**的 `AuditPage`，其中有释放了该变更的那次决策对应的 `approval.*` 行 ——
  实地观察到（C7）。
- 包裹一次原生轮次的 `TurnStarted` / `TurnFinished`，以及在一个已完成轮次之后被排空
  的会话队列 —— 通过它们持久的 `turn_owner` 事实实地观察到（C6）。
- `RemoteUnreachable` —— **未观察到**。裸 `origin` 是一个本地路径，所以不存在
  remote 不可达的情形；它仍然只有 fixture 回放。
- 所有 GUI 界面 —— **未观察到**。宿主屏幕锁定。

## E2 发现的缺陷与缺口

编号接续上面的九条。每一条都是复现出来的，不是推断出来的，并且都定位到了源码。E2
没有修复其中任何一条：这是一个发布证据步骤，在这里修 Core 行为会让它刚刚声明的
checkpoint 失效。

10. **一条归档 patch 渲染出的 diff 把文件称为重命名**（Core，观感问题但会误导）。
    证据行自己的 `PATH` 写着 `README.md`，而它下面的行写着
    `after  Renamed  +1 -1` / `renamed from before`。`render_diff`
    （`crates/tools/src/files.rs:148`）写入占位的 `--- before` / `+++ after` 头部
    且不写 `@@` 行 —— 这是有意的，其文档注释也这么说。该输出的另外两个读取方都会把
    自己真正解析出的路径盖到占位符之上：
    `crates/runtime/src/decision_context.rs:110-116`，其注释陈述了这条规则
    （「工具输入才是两者的权威来源；渲染出的头部不是」），以及
    `crates/runtime/src/frontend_services.rs:1224-1227`。
    `crates/runtime/src/evidence_reads.rs:147` 没有这么做，所以 Core 发布的归档文档
    带着占位路径，而每个客户端都会渲染出一次从未发生的重命名。对着磁盘字节确认过：
    132 字节的 ContextStore blob 以 `--- before` / `+++ after` 开头。字节、哈希、
    验证结论与 `PATH` 行全都正确；只有派生出的单文件标注是错的。闭环方式是在
    `evidence_reads.rs` 中把条目的路径盖到文档上，就像审批路径已经做的那样。
    **已由 C12 07d5afe5 关闭**（兼容性后续项 14）。
11. **Lane 生命周期审批的决策不写持久审计行**（Core，对审计完整性而言是阻塞级）。
    这正是 E1 缺陷 4 的原形，在两条审批路径中的另一条上越过了 C7。
    `RespondToApproval` 先检查 `lane_supervisor.pending_approval_owner`
    （`crates/runtime/src/runtime_supervisor.rs:1138-1160`），对于由 lane supervisor
    持有的审批，它转发为 `SupervisorMessage::LaneApprovalResponse` 并**在**
    `:1253` 的 `ApprovalAuditLog::record_decision` **之前返回**。
    `LaneApprovalResponse` 分支（`:1853-1894`）只发出 `CommandAccepted`，不写审计
    记录；而 `crates/lanes` 根本没有审计写入方 —— 这是正确的，因为审计是 runtime
    拥有、注入进去的策略。复现：`lane_create` 审批在屏幕上显示了
    `AUDIT audit_1789224503187620000`，本次会话共批准了五次审批，而持久时间线持有
    四条 `approval.allow_once` 行 —— 缺的那条就是 `lane_create`。跟着那张收据去查的
    操作者什么也找不到。
    **已由 C12 da3dd5d0 关闭**（兼容性后续项 15）。
12. **`NoUpstream` 的恢复路径在 TUI 中没有控件**（TUI）。Core 的拒绝语说
    「push again with set upstream」，契约也陈述了同一条恢复路径
    （`crates/types/src/source_control.rs:225`：「`NoUpstream` -> offer
    `set_upstream`」）。`/git` 选择器只提供四行，而它的 push 行把
    `set_upstream: false` 写死（`apps/tui/src/tui/modal.rs:1163-1167`），所以 Core
    指出的那一步，在显示了这条拒绝的客户端上无法触达。E2 必须带外配置 tracking ref
    才能拿到一次被接受的 push 的证据，这一点记在第 16 步里而不是被隐藏。
    **已由 T3 3a4ce963 关闭**（2026-09-13）。当 Core 对某个目标回答过
    `Failed { NoUpstream }` 之后，选择器会追加一行「推送并设置上游」，发送
    `Push { remote, set_upstream: true }` —— 在
    `apps/tui/src/tui/operator_git.rs:172` 依据 Core 回显的目标与动作装填，在
    `apps/tui/src/tui/modal.rs:1182` 渲染。在那条回答之前绝不提供；它携带那次被拒
    push 所携带的 remote；Core 对该目标回报完成的 push 会移除它。无需 Core 改动：
    命令字段本就存在。第 16 步的带外说明仍然是*那一次*运行的如实记录。
13. **在一次轮次进行中排队的会话追加提示，其「待排队」状态永远不可观察**
    （Core/TUI 接缝，产品缺口 —— 屏幕上没有任何虚假陈述）。composer 提供了
    `[^J Queue]`，提示确实进了队列，并在 39 秒后在已完成的轮次之后运行。在整段等待
    期间 `RuntimeViewState.queued_inputs` 一直是空的，composer 渲染的是
    `composer.active`（「Type next prompt while Viden works…」）而不是
    `composer.queued`，所以操作者完全看不到自己的提示正在排队。成因在它被造成的地方
    就有文档说明：supervisor 是单个 worker，所以在一次轮次进行中发出的
    `QueueFollowUp` 是在 supervisor 的 channel 里等待，而不是在 Core 的队列里
    （`crates/runtime/src/runtime_supervisor.rs:1596-1605`），只有 worker 空闲后才到
    达 `runtime_contract.rs:793`，并在同一瞬间发出 `InputQueued` 与 `InputDequeued`。
    所以 Core 是诚实的 —— 它还没有接受一个队列条目；客户端对 Core 发布的内容也是
    诚实的；但两个客户端基于 C6 建起来的 `queued_inputs` 界面，在原生路径的会话作用
    域上是不可达的。这被报告为产品缺口，而不是契约破裂。
    **已由 C12 8972b0da 关闭**（兼容性后续项 16）。

两条关于驱动器而非客户端的 harness 说明。三条 `USER` 行读作 `i/lanes`、`i/evidence`
和 `iconfirm the push outcome`，因为驱动器在 composer 已处于 Insert 模式时又发了一个
`i` —— 与 T2 实地检查记录的是同一个现象。另外，在没有 Lane 的工作区上 `/lanes` 打开
的是一个空看板而不是创建流程；Normal 模式下的 `n` 才是入口，第 3 步用的就是它。

## 这对计划目标 4 意味着什么

`docs/release-0.3.3-plan.md` 的目标 4 —— 「一个真实的本地优先开发任务通过 GUI 从
接入走到一次已提交的变更，并记录审计与证据」—— 在 `0.3.4` 中作为目标 6 延续。按界面
分别说明：

- **通过 TUI —— 首次端到端达成。** 接入、在 Core 审批下创建 Lane、一次对着 Core
  类型化决策上下文批准并应用到真实文件的变更、一条带有 Core 验证过的规范字节的归档
  `patch` 行、一条持久的 `approval.allow_once` 审计行、一次暂存、一次**提交**、一次
  被判为 `NoUpstream` 的 push，以及一次被接受并进入真实 remote 的 push —— 其中所有
  源码管理环节都在未选中 Lane 的情况下、运行在 Core 发布的工作区 owner 之下完成。
  E1 的每一条「未达成」现在在这个界面上都已达成，而 `0.3.3` 结构性缺失的两块
  （GUI-CORE-027、GUI-CORE-028）现在是实地证据而不是回放。
- **通过原生 GUI 窗口 —— 未尝试。** 宿主屏幕在本批次开始前就已锁定，并且从未解锁。
  E1 无法驱动窗口；E1b 走到接入就停了；E2 没有开始。驾驶舱自己的界面由 G3 到 G7 的
  确定性 harness 截图覆盖，那是关于渲染的证据，不是关于集成的证据。
- **诚实的总体结论：** 该任务现在已在契约的一个客户端上端到端证明，并且它所需要的
  契约能力都已被实地证明。仍未证明的是 **GUI 驾驶舱**能承载它。因此目标 4
  **在契约上与 TUI 上已达成，在 GUI 上未被证明**，而剩下的缺口是环境性的，不是能力
  缺口。它唯一需要的，是一次屏幕未锁定的运行。

## 边界声明

这是一个本地候选。候选版本 Core `0.3.7`、TUI `0.3.5`、GUI `0.1.0-rc.5` 只存在于
本地分支 `claude/e2-release-evidence` 上，该分支基于本地集成分支
`claude/int-0.3.4`。本部分中的任何内容都没有被发布、代码签名、公证、推送、合并、
打 tag、为任何平台打包、同步到 Homebrew tap，也没有对着 live provider 做过认证。
上文提到的 macOS `.app` 是本地构建产物，既未安装也未分发。release gate 的
prepublish 阶段没有运行；打包、公证、Homebrew 与 live provider 认证按 `0.3.4` 计划
自己的范围修订属于 `0.3.5` 的范围。

## 原生 GUI 运行 —— 2026-09-13（0.3.4，取自 main 75e2a84a）

这是 `0.3.4` 发布步骤未能做成的那次运行。分支 `claude/evidence-e3a`，位于
`.worktrees/evidence-e3a`，基线为 `main` 的
`75e2a84ade9f2363732aa10332cc666ff366d029` —— 与 `0.3.4` 发布提交 `edcc7b3d`
代码完全相同，两者之间只有计划文档不同，所以下文证明的就是 `0.3.4` 实际交付的东西。
宿主屏幕处于解锁状态，并且在每一批按键与点击之前都读取了
`CGSessionCopyCurrentDictionary()`；每一行都记录在本批次报告里，且每一次
`CGSSessionScreenIsLocked` 都是 `0`。下文没有任何内容是从 fixture 回放推断出来的，
也没有修复其中任何缺陷：这是证据步骤。

这是一次**部分**运行，而且这个切分很重要。真实任务的整个源码管理半边 —— 暂存、提交
与推送，各自在自己的 Core 审批之下、在未选中 Lane 的情况下 —— 首次通过驾驶舱端到端
跑通。Lane 那半边完全没有跑，因为**这个构建里无法创建 Lane**（缺陷 14 与 15），
它把编辑器输入框、工具审批、内联工具 diff、检查运行块、归档补丁与排队后续消息都挡在
了后面。

### 被测应用

`npm --prefix apps/gui ci --no-audit --no-fund` 在 9 秒内新增 97 个包；
`npm --prefix apps/gui run tauri -- build --bundles app` 在 1 分 12 秒内通过
（05:19:10Z 至 05:20:2xZ），产出 `target/release/bundle/macos/Viden.app`：

| 事实 | 值 |
| --- | --- |
| `CFBundleShortVersionString` | `0.1.0-rc.5` |
| `CFBundleIdentifier` | `dev.viden.gui` |
| `CFBundleExecutable` | `viden-gui` |
| 架构 | `Mach-O 64-bit executable arm64` |
| 可执行文件大小 | 40,655,440 字节 |
| Bundle 大小 | 39 MB |
| 签名 | ad-hoc、linker-signed、`TeamIdentifier=not set`、`Sealed Resources=none` |

它是从 bundle 启动的，没有被安装、用身份签名、公证或分发。每次启动都把
`VIDEN_HOME` 指向一个 scratch 目录，因此本仓库自己的 `.viden/` 从未被读或写。
启动过的 PID 依次为：93235、9284、59134（那次长运行）、86319、3918、47443。

### Fixture

运行 scratch 目录下的一个全新临时 Git 仓库：一个提交
`c593aefa2f9fed571e2caef713e9f0b39e6b7f2e`（"Add the fixture README"）、一个七行的
`README.md`、一个覆盖 `.viden/` 与 `.worktrees/` 的 `.gitignore`，以及一个选择
`provider = "fallback"` / `model = "test-local"` 的 `.viden/config.toml`。
Core 在 bootstrap 时铸出了 `.viden/project.toml`
`[project] id = "prj_1789277024425237000"` —— 这就是 C10 的绑定点，在磁盘上可见，
也是下文每个审批携带的 id。

### 与脚本任务的两处偏离，以及原因

两处都是本次运行发现的缺陷带来的后果，这里点名说明，而不是藏进步骤行里。

- **工作区改动是在带外做的。** 任务的变更本应是 Lane 内的编辑器输入框
  `tool edit_file …`。GUI 的输入框是 Lane 作用域的（`Message the selected Lane`
  处于禁用状态，下方写着 `Create or select a Lane to start.`），而 Lane 无法创建
  （缺陷 14、15），所以这次改动是用 `sed` 在 fixture 上做的，然后驱动驾驶舱处理这棵
  变脏的工作树。编辑之后的一切 —— Core 的类型化决策上下文、审批、结果与审计行 ——
  都是 GUI 驱动、作用在真实改动上的真实 Core 行为，但这次改动的**作者**不是产品。
- **裸 `origin` 是在任何被拒推送之前加上的。** TUI 那次是靠在没有 remote 的情况下
  推送拿到 `OperatorGitOutcome::Failed { NoUpstream }` 的。在驾驶舱里，推送的入口
  只有 DiffReview 提交条上的 `Commit & Push ⇡` 与标题栏同步 chip；chip 只在 Core
  采样到 `ahead > 0` 时才提供推送，而这需要 upstream，而 `Commit & Push` 会先发起
  `git_commit` 审批再发起 `git_push` 审批 —— 一个应用会话里两次审批，这正是缺陷 16
  让人做不到的事。所以 `NoUpstream` **没有在原生环境下走到**，裸 `origin` 与跟踪
  引用是在带外创建的，而下文那次被接受的推送，是 Core 把一个真实提交推进一个真实
  remote 的真实推送。

### 逐步记录

每一帧都拍了两次（沿用 `0.3.3` harness 记录里的 `screencapture` 陈旧问题），保留第二
帧；每一条断言都以 Accessibility 树为准，且列出的每一张 PNG 都在列出前被看过。

| # | 步骤 | 是否走到 | 结果 / 原因 | 帧 |
| --- | --- | --- | --- | --- |
| 1 | Welcome | 是 | `No project open`，一个带 `⌘ O` 的 `Open project` 动作，`Recent projects appear once a project is open.` / `Core adapter is not connected`。状态栏 `MODE — · PERM — · CONTEXT — · EVENTS #0 · LANE — · DIAG 0× · REQ —`。 | [`01`](native-0.3.4/01-welcome.png) |
| 2 | `⌘O` 绑定 | 是 | `⌘O` 打开 `Open project folder` 面板，按 harness 记录要求仅用按键驱动（`⌘⇧G`、粘贴路径、`Return`、`Return`）。面板列出 fixture 的 `README.md`，118 字节，`Open` 可用。 | [`02`](native-0.3.4/02-open-panel-at-fixture.png) |
| 3 | **驾驶舱已绑定** | 是 | 驾驶舱在 fixture 上起来了：dock Environment `Provider fallback`、`Model test-local`、`Mode build`、`Permission ask`；Local `Branch main`、fixture 路径作为 `Worktree`、`Ahead 0`、`Behind 0`、`Working tree Clean`；状态栏 `MODE build · PERM ask · EVENTS #7 · LANE — · REQ 0 req / 0 err`。带外核对：Core 把 `session_meta` 的 `canonical_root`＝fixture 路径、`work_mode` `build`、`permission_mode` `default`、`model` `test-local` 写进了 scratch `VIDEN_HOME`。 | [`03`](native-0.3.4/03-cockpit-bound.png) |
| 4 | **未选中 Lane 时可见的工作区 owner** | 是 | `LANE —`，而 dock 的 `Commit or push` 区把 `Open the commit bar` 渲染为**可用**（`AXEnabled=true`），同时标题栏同步 chip 可用，帮助文本为 `0 ahead, 0 behind the upstream, as Core last sampled it.` 两者都没有退回 `D1-OPERATOR-GIT-NO-WORKSPACE-OWNER`，因此 C5 的 `runtime.workspace_owner` 已发布并被消费 —— GUI-CORE-027 在 GUI 上实地关闭。 | [`04`](native-0.3.4/04-workspace-owner-commit-bar.png) |
| 5 | `⌘L` 新建 Lane | 部分 | `⌘L` 打开了 New Lane 浮层，`Viden Agent built-in` 被选中，三个 ACP adapter 在 `Checking ACP agents…` 下变灰，任务已粘贴，推导出的分支为 `⌁ vd/edit-the-fixture-readme-he`，隔离方式 `local`。 | [`05`](native-0.3.4/05-new-lane-popover.png) |
| 6 | **在 Core 审批下创建 Lane** | **否** | `Create lane ↵` 始终没有变为可用。`Checking ACP agents…` 在两个独立的应用会话里各自持续超过六分钟，期间 Accessibility 树一直读到 `enabled=false`，而且选中的是内建 agent。`⌘⏎` 无效。从未发起过 `lane_create` 审批。见缺陷 14。 | [`06`](native-0.3.4/06-new-lane-create-blocked.png) |
| 7 | 作为第二条创建路径的 D4 `Full setup…` | **否** | 向导以整窗形式打开在第 1 步，带着 Core 的只读解析（`Coder`/`Reviewer`/`Tester`、Lane ID、`Branch override`、被带过来的任务并注明它不属于创建命令、`Worktree` 与 `Base revision` 显示 `Core resolves this on preview`）。按下 `Preview with Core`；80 秒内没有 `StarterLanePreviewed` 事实到达，`Create reviewed Lane` 保持禁用。见缺陷 15。 | [`07`](native-0.3.4/07-d4-wizard-step1.png)、[`08`](native-0.3.4/08-d4-preview-unresolved.png) |
| 8 | 输入框 `tool edit_file …`、该编辑的 D2 hunks、内联工具 diff、检查运行块 | **否** | 这四项在本客户端都是 Lane 作用域的；没有 Lane 时输入框禁用并明说。被第 6、7 步挡住，走不到。 | — |
| 9 | 运行前的 EvidenceView | 是 | 在驾驶舱 chrome 内打开，带设计自己的筛选 chip（`All`/`Patch`/`Test result`/`Review`/`Doc update`/`Release artifact`）、搜索框、`No evidence in this scope.` 与 `Archive complete — Core published every entry matching this scope.` 这是一个诚实的空归档，不是一个沉默的空归档。 | [`09`](native-0.3.4/09-evidenceview-empty-before.png) |
| 10 | 变脏工作树上的 DiffReview | 是 | 带外做完改动后，视图自身的重读控件刷新了 Core 的采样：`1 files · +1 −1`、`README.md`、`@@ -1,4 +1,4 @@`、`-# E3a Fixture` / `+# E3a Edited`，dock 的 `Changes` 区列出 `M README.md +1 −1`。 | [`10`](native-0.3.4/10-diffreview-dirty.png) |
| 11 | **在 Core 审批下暂存** | 是 | `Stage all` → `Stage is waiting for approval in the permission dock.` D2 显示 `Approve git_add prj_1789277024425237000 · pending · low`、`TOOL INPUT PREVIEW all: true`、`EVIDENCE Core published no evidence for this decision.`，以及 `Decision and reason are written to audit audit_1789279167519670000`，下面是 `Approve once` / `Deny`。`Approve once` 完成暂存：`git status --short` 从 ` M README.md` 变成 `M  README.md`。 | [`11`](native-0.3.4/11-stage-waiting-approval.png)、[`12`](native-0.3.4/12-stage-approval-d2.png)、[`13`](native-0.3.4/13-stage-applied.png) |
| 12 | **暂存的 `OperatorGitOutcome::Completed`** | 是 | 提交条渲染出 `Stage completed · main ↑0 ↓0 · uncommitted changes remain`，带 `▶ Git output` 折叠项，`LANE —`。 | [`14`](native-0.3.4/14-stage-completed.png) |
| 13 | 决策之后的 D2 | **缺陷** | 已决策的 `git_add` 卡片仍停在 `pending · low`，仍是 `1 awaiting you` 与 `Sent. Waiting for Core to record the verdict.`，而随后发起的 `git_commit` 审批从未出现。见缺陷 16。 | [`15`](native-0.3.4/15-d2-stale-after-decision.png) |
| 14 | **D14 显示 `approval.allow_once` 审计行** | 是 | 审计时间线渲染出 `approval.allow_once operator success · permission · approval_1789279167519659000 · tool · git_add · job · gui-git-2d695ebc-… · scope=allow_once`，旁边是它的 `source.stage` authorized/completed 对，下方是 `Core reports the audit timeline is complete.` 带外核对 scratch `workflows/projects/*/audit.jsonl`：同样三行、同样的 id。 | [`16`](native-0.3.4/16-d14-audit-stage.png) |
| 15 | **dock Files 标签打开文件** | 是 | `Files` 标签列出 Core 的工作区清单（`.gitignore` 20、`README.md` 117），inspector 的 `Open file` 读到了它：`workspace root · 117 bytes on disk · sha256 f1d3490374e3…`，下方是文件正文。带外核对：文件是 117 字节，sha256 为 `f1d3490374e339e9f46c273d44757462cf8c0818c5cd54269b6488646bd96bdf`。C9 的 `runtime.workspace_file_reads` 被实地消费。 | [`17`](native-0.3.4/17-dock-files-open.png) |
| 16 | **专注模式 `⌘.` 与 `Esc`** | 部分 | `Focus mode ⌘.` 控件会隐藏上下文 dock 并把整窗宽度给中栏；`Esc` 从审计时间线回到会话视图，chrome 保留、专注模式仍开着，且记录区带着一条持久的 `PERMISSION approval_… · decision allow_once` 行与 `Open audit trail`。当焦点落在 dock 的文件列表里时 `⌘.` 组合键本身没有触发；控件触发了。这记为下文的观察而非缺陷，因为从外部无法证明一次合成按键的焦点落点。 | [`18`](native-0.3.4/18-focus-mode.png)、[`19`](native-0.3.4/19-esc-return.png) |
| 17 | **未选中 Lane、在 Core 审批下、带 C1 类型化决策上下文的提交** | 是 | 在一个全新的应用会话里，`git_commit` 审批正确出现了：`Approve git_commit prj_1789277024425237000 · pending · medium`、`TOOL INPUT PREVIEW message: … path: …`，以及**Core 的类型化 hunks 而不是工具输入** —— `README.md`、`@@ -1,4 +1,4 @@`、`-# E3a Fixture` / `+# E3a Edited`，带逐行行号。`Approve once` 完成提交。带外核对：`a5d3d53 Edit the fixture README heading`，工作树干净。**GUI 中实地观察到提交的 `OperatorGitOutcome::Completed`，且 `LANE —`。** | [`20`](native-0.3.4/20-commit-approval-hunks.png)、[`21`](native-0.3.4/21-commit-completed.png) |
| 18 | 推送被 `NoUpstream` 拒绝 | **否** | 走不到；见上文第二处偏离。第二个提交 `2d0a30d` 是用同样方式通过 `Commit & Push ⇡` 做的，其 `git_push` 审批随后被缺陷 16 吞掉 —— 提交条显示 `Push is waiting for approval in the permission dock.`，而 D2 仍显示已决策的 `git_commit`。 | — |
| 19 | **推送被接受** | 是 | 在裸 `origin` 与跟踪引用就位、本地领先一个提交的情况下，标题栏同步 chip 显示 `Push 1 local commits. Pull is excluded by contract; this client pushes and fetches only.` 且可用。它发起了 `Approve git_push prj_1789277024425237000 · pending · high`、`remote: origin`、审计 `audit_1789281559244376000`，下面是 `Approve once` / `Approve for repo paths` / `Deny`。`Approve once` 完成推送：提交条渲染出 `Push completed · main ↑0 ↓0 · working tree clean`。带外核对：裸 `origin` 里有 `70bda55`，本地分支读作 `## main...origin/main` 且没有分叉。**GUI 中实地观察到推送的 `OperatorGitOutcome::Completed`，且 `LANE —`。** | [`22`](native-0.3.4/22-sync-chip-ahead.png)、[`23`](native-0.3.4/23-push-approval-d2.png)、[`24`](native-0.3.4/24-push-completed.png) |
| 20 | **运行之后的 D14** | 是 | `Audit trail 15`、`Outcomes on the loaded page · 15 success 15`：五条 `approval.allow_once`（两条 `git_add`、两条 `git_commit`、一条 `git_push`），每条都带自己的 `gui-git-…` job id，以及两次暂存、两次提交与一次推送各自的 `authorized`/`completed` 对。与 scratch `audit.jsonl` 逐字核对一致。 | [`25`](native-0.3.4/25-d14-audit-full.png) |
| 21 | 运行之后的 EvidenceView | 是 | 仍是 `No evidence in this scope.` 与 `Archive complete`，搜索框自己的措辞是 `0 entries`。这是正确的：C7 为**原生工具**变更归档 `patch`，而本次没有工具变更可做。 | [`26`](native-0.3.4/26-evidenceview-empty-after.png) |
| 22 | 排队的后续消息在一轮进行中被排空 | **否** | 本客户端里一轮需要 Lane。被第 6、7 步挡住，走不到。 | — |

### GUI 中实地观察到的 Core 结果变体

- `WorkspaceRuntimeOwnerBound` 走 Tauri 宿主路径，带着铸出的 `.viden/project.toml`
  id —— 实地观察到（C5 + C10）。
- `RunOperatorGitAction { target: Workspace }` 在该 owner 之下、**未选中 Lane**
  的情况下被授权 —— 实地观察到三次（暂存、提交、推送）。
- `stage`、`commit` **与** `push` 的 `OperatorGitOutcome::Completed` ——
  在 GUI 中实地观察到。`Failed { NoUpstream }` 与 `RemoteUnreachable` ——
  **未观察到**；见第二处偏离。
- `git_commit` 审批上带 `DiffDocument` 的 `ApprovalRequestView.decision_context`
  （一个文件、一个 hunk、`+1 −1`）—— 实地观察到。`git_add` 与 `git_push` 审批带的是
  不透明输入预览，并用 Core 自己的措辞加 `GUI-CORE-012` 说明了这一点。
- 五个审批上的 `ApprovalDecision` allow-once，每一个后面都跟着它在磁盘上的效果 ——
  实地观察到。
- `AuditPage` **非空**，每一个释放变更的决策都有 `approval.*` 行及其 `source.*`
  配对 —— 实地观察到（C7）。
- 带已验证摘要的 `WorkspaceFileRead` —— 实地观察到（C9）。
- `EvidencePage` —— 实地观察到且为**空**，这是本次运行的真实答案，而不是 C7 归档的
  证据。
- `StarterLanePreviewed`、`StarterLaneCreated`、`TurnStarted`/`TurnFinished`、
  `InputQueued`/`InputDequeued`、`ToolCallStarted`、`EvidenceRecorded` ——
  **未观察到**。无法创建 Lane。

### E3a 发现的缺陷与缺口

编号接着上文十三条继续。每一条都是复现出来的，不是推断出来的。这里都没有修复。

14. **只要 ACP adapter 探测还没结束，New Lane 浮层就永远创建不了 Lane**
    （GUI，阻塞）。`create.disabled` 里或上了 `model.probing`
    （`apps/gui/src/components/agent_menu.ts:277-282`），而 `model.probing` 就是
    `discoveryIsProbing()` = `agentDiscoveryStarted && !agentDiscoveryComplete`
    （`apps/gui/src/screens/d1_cockpit.ts:3046-3048`）。探测会遍历每一个
    `startability` 为 `probe_required` 的 adapter（`advanceAgentDiscovery`,
    `:3644-3673`），而每次探测都在等 Core 的 `AgentAdapterProbed` 业务事实
    （`apps/gui/src-tauri/src/adapter.rs:588-591`）。在一台没有安装 Codex、
    Claude Agent 或 Kiro CLI 的宿主上，没有任何 `AgentAdapterProbed` 到达：
    诊断行在两个独立应用会话里各自停在 `Checking ACP agents…` 超过六分钟，
    `Create lane ↵` 全程读到 `enabled=false` —— **包括在选中根本不需要探测的内建
    `Viden Agent` 时**。这个门是把一个 ACP 的关切套在了每一次创建上。正是这一条缺陷
    让真实任务的 Lane 半边在已交付的 `0.3.4` GUI 里走不通。
15. **D4 起始 Lane 向导的 `Preview with Core` 卡在那次未完成的探测后面**
    （GUI，阻塞）。`Full setup…` 能带着任务进入 D4，但 `Preview with Core` 在
    80 秒内没有产出 `StarterLanePreviewed` 事实，`Create reviewed Lane` 保持禁用，
    所以第二条创建路径被第一条的失败关掉了。机制是一个进程级的命令槽：`PendingD1`
    占着那个未完成的探测（`apps/gui/src-tauri/src/adapter.rs:507-509`），而 webview
    自己的派发器在 `pendingCommandId` 置位时会直接返回
    （`apps/gui/src/screens/d1_cockpit.ts:3599-3612`、`:3644-3656`）。因此一次卡住的
    探测不是让某一个菜单降级，而是让 Lane 创建整体停摆。只关掉 14 而不同时给这个
    pending 命令加上界，会把同样的形状留在任何其他无人应答的命令后面。
16. **决策中心把一个已决策的审批一直当作唯一待办，并且从不呈现下一个**
    （GUI，对任何需要两次决策的任务都是阻塞）。在 `git_add` 审批上按下 `Approve once`
    之后，Core 已经把效果落地 —— 文件在磁盘上进了暂存区、审计行已写、提交条渲染出
    `Stage completed` —— 而 D2 把同一张卡片留在 `pending · low`，留着
    `1 awaiting you`，也留着页脚 `Sent. Waiting for Core to record the verdict.`
    随后发起的 `git_commit` 审批从未出现，尽管 DiffReview 写着
    `Commit is waiting for approval in the permission dock.` 导航到 DiffReview 再
    回来、以及切换 `Gate approvals` 筛选，都没有刷新它；只有重启应用才行，而在一个
    还没做出第一次决策的新会话里，下一个审批渲染正常。复现两次（`git_add` 后
    `git_commit`；`git_commit` 后 `git_push`）。实际效果是一个应用会话只能服务一次
    审批，这也是第 17 与第 19 步各需要一次自己的启动的原因。
17. **提交完成之后源码管理的在途状态从不清除**（GUI）。在
    `Commit completed · main ↑0 ↓0 · working tree clean` 已经显示在屏幕上的情况下，
    `Commit & Push ⇡` 仍然禁用，提示为 `Waiting for the permission decision.`，
    标题栏同步 chip 也仍然禁用，提示为
    `A source-control action is already running.`，30 秒后仍无变化。屏幕上关于这次
    提交的说法没有一句是假的；假的是客户端自己的忙碌标志，而它让推送在那个会话里
    不可达。
18. **应用会在会话中途自行终止**（GUI，对任何长时间原生运行都是阻塞）。本批次中有
    五次进程以状态 `0` 退出，除了两行 AppKit TSM/IMK 之外没有 stderr，也没有 macOS
    崩溃报告：05:32:09Z、05:34:06Z、06:10:50Z、06:27:03Z、06:36:29Z。它既发生在有
    Core 命令挂起时，也发生在一次干净绑定之后约七十秒、什么都没挂起时，还有一次发生
    在两次都成功的 Accessibility 读取之间。每次退出都丢掉该会话的 Core 状态并要重新
    绑定。这与 E1b 的缺陷 7 是同一*形状* —— 窗口消失、进程没了 —— 只是现在是干净的
    退出码而不是可见的 `CONNECTING` 状态，所以单独记录而不是并入那一条。
19. **上下文 dock 打开时 Lane 标签条与自己的元信息重叠**（GUI，外观）。在没有 Lane
    时，标签条把 `No Lanes yet`、项目路径与 `Build` 画在了一起。在专注模式下，中栏
    拿到整窗宽度，同一条标签条渲染正常，所以这是标签条 meta 槽位的宽度预算缺陷，而
    不是缺少截断规则。
20. **dock 的 `Changes` 合计与 `Local` 工作树标志与紧挨着它们的文件列表自相矛盾**
    （GUI，诚实性）。在 Core 刚刚重新采样工作区 diff 之后，dock 在一个仍读作
    `+0 −0` 的 `Lines` 合计下列出了 `M README.md +1 −1`，下面还写着
    `Working tree Clean`。这三者来自不同的 Core 采样，而客户端把它们渲染出来时既没有
    对账也没有标注时间，于是这块面板同时陈述了两个互相矛盾的事实。
21. **DiffReview 的变更文件列表在提交之后没有重新采样**（GUI，外观）。在
    `Commit completed · main ↑0 ↓0 · working tree clean` 已显示在屏幕上时，
    `CHANGES` 仍读作 `1 files · +1 −1`，并仍然画着刚刚被提交掉的那个 hunk。

### 四条预期分歧：复现与否

`0.3.4` 留下了缺陷 10 到 13，E3a 要做的是在原生环境下逐条确认。

- **10 —— 归档补丁渲染出的 diff 把文件叫成 rename。** *未复现；走不到。* 本次运行
  归档是空的，因为 C7 是为原生**工具**变更归档 `patch` 的，而本次做不出工具变更
  （缺陷 14、15）。这里也没有任何东西与它矛盾；它继续按 TUI 证据保持开启。
- **11 —— Lane 生命周期审批决策不写持久审计行。** *未复现；走不到。* 无法发起
  `lane_create` 审批，因此 lane supervisor 的审批路径从未被触发。本次运行能显示的是
  另一条路径工作正常：五次运行时路径审批（`git_add`、`git_commit`、`git_push`）
  都写出了自己的 `approval.allow_once` 行，这与缺陷 11 只针对 lane 路径的说法一致。
- **12 —— `NoUpstream` 的恢复动作没有客户端控件。** *在 GUI 上未复现；那次拒绝
  从未出现在屏幕上*（第二处偏离）。本次运行能支撑的观察更窄，也就按更窄的说：GUI
  的两个推送入口 —— `Commit & Push ⇡` 与标题栏同步 chip —— 都没有提供 set-upstream
  选项，而 chip 只有在 Core 采样到 `ahead > 0` 时才可用，这本身就已经预设了
  upstream。Core 那句 `push again with set upstream` 背后是否配了控件，这里没有测到。
- **13 —— 一轮进行中排队的会话后续消息从不可见地处于 pending。** *未复现；走不到。*
  本客户端里一轮需要 Lane，因此没有正在跑的一轮可供排队。

### 观察，不是缺陷

- 当键盘焦点落在 dock 的文件列表里时，`⌘.` 组合键没有切换专注模式；`⌘L` 在一个会话
  里打开了 New Lane 浮层，在另一个会话里没有；而 `Focus mode ⌘.` 控件与
  `＋ New Lane` 控件每次都有效。从 webview 外部无法确定一次合成按键的焦点落点，
  所以这些只记录、不编号。
- 宿主当前的输入源是拼音输入法，它会把合成的 `keystroke` 文本搅乱
  （`Edit the fixture README heading` 变成了 `的Ei天他和fixture READMEheading`）。
  本次运行的文本输入都改用剪贴板粘贴，这会绕开输入法；其中有一段把输入源切到了
  `ABC` 并在之后恢复。下一次原生运行应当粘贴而不是打字。
- `Open project folder` 面板的 go-to 输入框有一次把很长的粘贴路径静默截断了，重试
  一次就正确补全；该面板通过 Accessibility 仍然只能用键盘驱动，与 `0.3.3` 的
  harness 记录一致，而那些记录的其余部分也原样成立。
- Accessibility 进程名仍是 `viden-gui` 而窗口 owner 是 `Viden`，
  `screencapture -l` 仍会返回陈旧帧，而且当打开面板弹出时必须按窗口*名字*来选主窗口
  —— 屏幕上可以同时有三个窗口，第一个 `layer=0` 命中的是那张 sheet。

### 这对计划目标 4 意味着什么（分界面）

`docs/release-0.3.3-plan.md` 的目标 4，在 `0.3.4` 里承接为目标 6 ——「一个真实的
本地优先开发任务通过 GUI 完成，从接入到一次已提交的变更，并记录审计与证据」：

- **通过 TUI —— 已达成，端到端**，与上文 2026-09-12 的记录一致，未变。
- **通过原生 GUI 窗口 —— 部分达成，而未达成的那半边是能力缺口，不是环境原因。**
  接入、未选中 Lane 时 Core 发布的工作区 owner、真实审批上 Core 的类型化决策上下文、
  三次被决策并落地的审批、暂存/**提交**/被接受的**推送**三者的
  `OperatorGitOutcome::Completed`、D14 中持久的 `approval.allow_once` 行、dock 里
  一次经过校验的工作区文件读取、专注模式与 `Esc` 返回路径 —— 全部首次通过驾驶舱
  驱动完成。驾驶舱**做不到**的是创建 Lane，随之做不到的还有 agent 轮次、工具审批、
  内联工具 diff、归档补丁与排队后续消息。
- **诚实的总体结论：** 目标 4 的源码管理半边现在在 GUI 上与在 TUI 上一样已被证明；
  Lane 半边在 TUI 上已证明，而在 GUI 上被缺陷 14 与 15 **阻塞**。这已经不再是
  「因环境原因未被证明」—— 屏幕是解锁的，窗口被驱动了，是客户端停住了。因此目标 4
  **在契约上与 TUI 上已达成，在 GUI 上部分达成并带有一个有名有姓、可复现的阻塞项**，
  这比 `0.3.4` 留下的状态理解得更清楚，也比 `0.3.4` 所期望的更糟。
