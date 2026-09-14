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

## 缺陷 14–21 —— H4（2026-09-13，在 `claude/gui-native-defects-h4` 上）

分支 `claude/gui-native-defects-h4`，工作树 `.worktrees/gui-native-defects-h4`，
基线 `claude/int-0.3.5` 的 `67353a71`（= `main` `75e2a84a` + `0.3.5` 契约设计、
H3、C12、DS、C13、T3 以及本文档的 E3a 小节）。主机屏幕处于解锁状态，每一批按键与点击
之前都读取了 `CGSessionCopyCurrentDictionary()`，每一次 `CGSSessionScreenIsLocked`
都是 `0`。

本节记录**每个缺陷的裁决**，以及对 E3a 被阻塞步骤的一次原生复跑。它不是 `0.3.5` 的
原生证明 —— 那属于 E3b，在发布候选上进行，而 Lane 半边在创建之后仍然欠着。

### 被测应用

`npm --prefix apps/gui ci --no-audit --no-fund` 用 2 秒新增 97 个包。
`npm --prefix apps/gui run tauri -- build --bundles app` 通过，产出
`target/release/bundle/macos/Viden.app`：

| 事实 | 值 |
| --- | --- |
| `CFBundleShortVersionString` | `0.1.0-rc.5`（H4 不提升 GUI 版本） |
| `CFBundleExecutable` | `viden-gui` |
| 架构 | `Mach-O 64-bit executable arm64` |
| 可执行文件大小 | 41,496,368 字节 |
| 包体积 | 40 MB |
| 签名 | ad-hoc，`TeamIdentifier=not set`，`Sealed Resources=none` |

每次启动都把 `VIDEN_HOME` 指向一个 scratch 目录，因此本仓库自己的 `.viden/` 从未被
读取或写入。夹具是一个全新的临时 Git 仓库：一次提交 `06a79a3`（"Add the fixture
README"）、一个六行的 `README.md`、一个覆盖 `.viden/` 与 `.worktrees/` 的
`.gitignore`，以及一个选择 `provider = "fallback"` / `model = "test-local"` 的
`.viden/config.toml`。

**一处偏差，写明而不是藏起来。** 工作区是用 `VIDEN_GUI_WORKSPACE` 绑定的，而不是
经由 `⌘O` 与原生文件夹面板。E3a 已经端到端证明了那条路径（它的步骤 2 与 3）；H4 的
主题是绑定之后发生的事，而启动绑定是一个有文档的入口
（`apps/gui/README.md`，「Run the desktop client locally」）。下文没有任何一条依赖
工作区是如何绑定的。

### 裁决

| # | 缺陷 | 裁决 |
| --- | --- | --- |
| 14 | ACP 发现未完成时新建 Lane 永远无法创建 | **已修复**（`ee2ce04f`）。`create.disabled` 不再与发现状态取或；闸门是所选适配器自身的可启动性。下文原生证明：在没有任何 ACP 可执行文件的主机上，用内置 Agent 创建了一个 Lane |
| 15 | 探测卡死时 D4 的 `Preview with Core` 永不落定 | **客户端侧已修复**（`ee2ce04f`），并**作为 Core 发现上报**（`GUI-CORE-035`）。客户端放弃仍在外的适配器**读取**以释放自己的命令槽位；Core 唯一的命令工作线程仍被它正在执行的探测占用，这只有 Core 能修 |
| 16 | 决策中心保留已决策的审批且从不呈现下一条 | **已修复**（`ee2ce04f`）。两半：决策现在通过权限管线等待 Core 的 `ApprovalResolved`；离开中心视图去 DiffReview 或 EvidenceView 会丢弃该面板及其挂载备忘，因此返回是一次新的 Core 读取 |
| 17 | 提交完成后源码管理的进行中状态从不清除 | **已修复**（`ee2ce04f`）。根因：每个排空只调和自己的待决命令，因此被驾驶舱 250 毫秒轮询消费掉的 `OperatorGitActionFinished` 被吞掉了。所有排空现在都经过 `reconcile_ambient_pendings` |
| 18 | 应用在会话中途自行终止 | 在 H4 的有界努力内**未能复现** —— 见下文 |
| 19 | 上下文坞打开时 Lane 页签条与自身元信息重叠 | **已修复**（`284c4229`）。下文原生证明 |
| 20 | 坞的 `Changes` 合计与 `Local` 标志与旁边的文件列表互相矛盾 | **已修复**（`284c4229`）。两者现在读取文件列表所读的同一份采样，并由 `data-changes-sample` 说明是哪一份 |
| 21 | 提交之后 DiffReview 的变更文件列表未被重新采样 | **已修复**（`ee2ce04f`）。`completed` 结果用一次 `QueryWorkspaceDiff` 重新采样；拒绝或传输失败仍走去抖的陈旧路径，因为它们都没有移动工作树 |

### 原生复跑

每帧都拍两次并保留第二张；每一条断言都以 Accessibility 树为准据；列出的每张 PNG
在被列出之前都已被查看。

| # | 步骤 | 是否到达 | 结果／原因 | 帧 |
| --- | --- | --- | --- | --- |
| 1 | **驾驶舱已绑定，页签条与坞可读** | 是 | 坞 Environment `Provider fallback`、`Model test-local`、`Mode build`、`Permission ask`；Local `Showing Workspace root`、`Branch main`、`Working tree Clean`；状态栏 `MODE build · PERM ask · EVENTS #7 · LANE —`。零 Lane 的页签条在上下文坞打开的情况下把 `No Lanes yet`、`＋`、项目路径与 `Build` 各自画在自己的空间里 —— 缺陷 19，原生 | [`h4-01`](native-0.3.4/h4-01-cockpit-bound.png) |
| 2 | **在所有 ACP 探测都未应答时 `Create lane ↵` 可用** | 是 | 弹层打开时内置 `Viden Agent` 被选中，三个 ACP 适配器在 `Probe did not answer within 15s.` 下被禁用 —— 这是客户端自己陈述的时限，旁边带 `Retry ACP check` —— 而粘贴任务后 `Create lane ↵` 读到 `enabled=true`。同一次运行较早时，同一个弹层显示 `Checking ACP agents… each probe is given up to 15s.` 而 `Create lane ↵` 已经可用。**缺陷 14，原生。** `Checking ACP agents…` 从未无限期停留 | [`h4-02`](native-0.3.4/h4-02-new-lane-create-enabled.png) |
| 3 | **一次 Core `lane_create` 审批** | 是 | `Approve lane_create · lane_create`、`Medium risk`、`branch: viden/lane_1789287265044945000 path: …/.worktrees/lane_1789287265044945000`、`Reason: lane mutation policy requires approval`、`Audit: audit_1789287266911108000`，在 `Y · Once` / `Shift+A · Allow repo paths` / `N · Deny` 之上，`Always` 与 `Edit` 被禁用并标注 `GUI-CORE-003`，状态栏为 `1 awaiting you`。**这是本客户端历史上第一次抬起的 `lane_create` 审批** | [`h4-03`](native-0.3.4/h4-03-lane-create-approval.png) |
| 4 | **在该审批下创建了 Lane** | 是 | `Y · Once` 放行了效果：页签条上出现 `lane_1789287265044945000 · coder start…`，状态栏读 `LANE lane_1789287265044945000 — draft`，标题栏读 `1 worktree`，坞的 Local 小节切换为 `Showing Lane worktree` 并显示该 Lane 自己的分支与工作树。带外核对：`.worktrees/lane_1789287265044945000/` 存在，`git branch` 列出 `viden/lane_1789287265044945000`。`LANE OUTPUT` 承载 Core 自己的 `Preparing worktree (new branch 'viden/lane_1789287265044945000')`。**缺陷 14 与 15 在 GUI 上关闭** | [`h4-04`](native-0.3.4/h4-04-lane-created.png) |
| 5 | 在该 Lane 中使用编写器 `tool edit_file …` | **否** | 编写器可用并接受文本，但新建的 Lane 带着一个进行中的轮次（屏幕上有 `Cancel active turn`，`composer.busy`），提交没有发出：两次尝试（Send 控件与 `Return`）中 `EVENTS` 始终停在 `#25`。这不是缺陷 14/15 的残留 —— Lane 存在且由 Core 拥有 —— 本批次也未对其诊断。它属于 E3b，由后者在候选上承担完整的 Lane 半边 | — |
| 6 | D2 两次连续审批、内联工具 diff、检查运行、EvidenceView 补丁、两者的 D14 `approval.allow_once`、排队后续消息 | **否** | 六项都排在步骤 5 之后 | — |

### 缺陷 18 —— 未复现

有界努力，如实陈述。该包在 2026-09-13 的 07:34Z 到 08:22Z 之间以 `RUST_LOG=debug`
运行了六次，stdout/stderr 均被捕获到文件，其中一次实例被驱动完成绑定、Lane 创建与一次
审批，随后在五秒一次的进程采样下闲置。没有任何一个实例自行退出：每一次结束的启动都是
被本批次杀掉的，而最后一个 —— 就是上面创建了 Lane 的那个实例 —— 以五秒一次的频率被采样了 16 分 18 秒
（92 次采样），到最后仍然存活。捕获到的 stderr 里只有 E3a
也看到的那行 AppKit TSM/IMK，窗口期内没有为 `viden-gui` 写入任何 macOS 崩溃报告。

**确实**复现的是 E3a 描述过、在它之前 E1b 缺陷 7 也描述过的一种形态：**窗口会短暂地
离开 Accessibility 树**。三次，都紧接在导致驾驶舱整屏重绘的点击之后（`Create lane ↵`
与 `Y · Once`），`System Events` 回答
`Can't get window 1 of process … Invalid index. (-1719)`，而三秒后的读取发现窗口回来
了，`count of windows` = 1，进程存活。这与「窗口消失且进程不见了」实际上是**两个**观察
而不是一个相吻合，也是下一次尝试应当顺着拉的线头。它被记为观察，而不是修复。

缺陷 18 未声称任何修复。它保持开放，并附上本批次的阴性结果与重绘／AX 观察。

### 诚实说明

- 上述帧来自**原生应用**，而不是采集 harness。本批次新增的四个 qa 状态 ——
  `h4-lane-create-builtin`、`h4-probe-timed-out`、`h4-d2-next-approval`、
  `h4-tabstrip-dock-open` —— 是同一批界面的可复现 harness 条目，它们不是这些裁决的
  证据；原生帧才是。
- 原生运行发现了它自己的一个缺陷，已在同一次提交中修复：发现流程可能在屏幕上留着
  `Checking ACP agents…` 而**没有任何命令在外**地停住，因为 `advanceAgentDiscovery`
  只由排空重新入队，而安静的 Core 不产生排空。时限现在属于整个发现流程而不是单次探测，
  并且 `waitForCommandSlot` 也有自己的时限，好让一次创建能够报告而不是无声等待。
- `startLane` 过去会用 `Core did not publish a creatable Lane preview.` 覆盖宿主自己的
  拒绝原因，把 Core 从未发出的拒绝算在它头上。原生运行正好撞上这一点并丢掉了原因；现在
  发送失败自己的那句话会保留下来。

## 原生 GUI 运行 —— 2026-09-13（0.3.5 候选版 `da4c8a6d`）

这是 E3b：在 `0.3.5` 发布候选版上，通过原生驾驶舱驱动完整真实任务。分支
`claude/evidence-e3b`，位于 `.worktrees/evidence-e3b`，基线为集成分支
`claude/int-0.3.5` 的 `da4c8a6d0f7152691d2c9a47bb992cb409656e02` —— 包含直到
G9 与 C16 的全部 `0.3.5` 批次。取证时 G10 尚未运行，因此包版本仍为
`0.1.0-rc.5`，D4 的 Agent 绑定也仍处于"选择不会传递"的状态；下文的 Lane 因此
使用内置 Agent 创建。

主机屏幕处于解锁状态，且在每一批按键与点击之前都读取了
`CGSessionCopyCurrentDictionary()`；每一次 `CGSSessionScreenIsLocked` 均为
`0`。本节没有任何内容来自固定装置回放，下文的缺陷一个也没有被修复：这是取证
步骤。

这仍然是一次**部分**运行，但分界线已经移动。H4 关闭了 Lane 创建，本次运行通过
驾驶舱在 Core `lane_create` 审批之下创建了三次 Lane，并看到该审批写下了它那条
持久审计行 —— 正是 `0.3.5` 目标 2 所针对的信任缺口。它**无法**做到的是在已创建
的 Lane 中启动一个回合：H4 那个未解释的第 5 步精确复现，而下文的诊断发现它根本
不是一个停滞的回合，而是客户端认为 Core 仍欠它一个应答的命令（缺陷 22）。凡是
由 Agent 回合把守的一切 —— 工具审批、内联工具差异、检查运行、归档补丁、排队跟进、
已提出的计划、可暂停或终止的 Agent 会话 —— 均未触及。

### 被测应用

`npm --prefix apps/gui ci --no-audit --no-fund` 在 2 秒内新增 97 个包
（09:49:07Z）。`npm --prefix apps/gui run tauri -- build --bundles app` 通过 ——
`Finished \`release\` profile [optimized] target(s) in 1m 07s` —— 并在 09:50:25Z
产出 `target/release/bundle/macos/Viden.app`：

| 事实 | 值 |
| --- | --- |
| `CFBundleShortVersionString` | `0.1.0-rc.5`（G10 尚未运行，`rc.6` 的版本提升归它） |
| `CFBundleIdentifier` | `dev.viden.gui` |
| `CFBundleExecutable` | `viden-gui` |
| 架构 | `Mach-O 64-bit executable arm64` |
| 可执行文件大小 | 42,746,560 字节 |
| 包体积 | 41 MB |
| 签名 | ad-hoc、linker-signed、`TeamIdentifier=not set`、`Sealed Resources=none` |

它从包中启动，从未被安装、用身份签名、公证或分发。每一次启动都将 `VIDEN_HOME`
指向暂存目录，因此本仓库自身的 `.viden/` 从未被读取或写入。启动的 PID 依次为：
43643、45136、67729、68885、80397。

### 固定装置

暂存目录下两个全新的临时 Git 仓库，各有一个提交 `2b593e1`（"Add the fixture
README"）、一个五行的 `README.md`、一个覆盖 `.viden/` 与 `.worktrees/` 的
`.gitignore`，以及一个选择 `provider = "fallback"` / `model = "test-local"` 的
`.viden/config.toml`。第二个之所以存在，是因为诊断需要干净的 `VIDEN_HOME` 和
干净的工作区，才能把"卡住的客户端"与"卡住的 Core"区分开；两者在相关处均具名。
Core 在两次引导时分别铸出 `.viden/project.toml` `prj_1789293381253952000` 与
`prj_1789295636864533000`。

### 与脚本任务的两处偏离及其原因

- **工作树改动再次在带外完成**，原因与 E3a 相同但成因不同：composer 的
  `tool edit_file …` 从未离开客户端（缺陷 22），因此改动是用 `sed` 在 Lane 自己的
  工作树内做出的，驾驶舱随后在由此产生的脏树上被驱动。改动下游的一切 —— Core
  的类型化决策上下文、各项审批、结果与审计行 —— 都是真实改动之上真实的、由 GUI
  驱动的 Core 行为，但改动的*作者身份*仍不属于产品。
- **`NoUpstream` 未触及，被接受的推送也未触及。** 推送入口仍只有
  `Commit & Push ⇡` 与标题栏同步芯片。芯片需要 Core 采样到 `ahead > 0`，而这已
  预设存在上游；`Commit & Push ⇡` 正确地提出了它的 `git_commit` 审批，随后丢失了
  `git_push` 那一半（缺陷 25）。没有添加裸 `origin`，因为没有可达的推送可以送往
  它。

### 逐步记录

每一帧都拍摄两次并保留第二帧，列出的每个 PNG 在列出之前都已查看。下文注记中具名
了一处工具链改动：这些帧是按窗口 id 拍摄的，而非按屏幕矩形。

| # | 步骤 | 是否触及 | 结果／原因 | 帧 |
| --- | --- | --- | --- | --- |
| 1 | 欢迎页 | 是 | `No project open`、`Open project ⌘ O`、`Recent projects appear once a project is open.` / `Core adapter is not connected`。活动导轨带有十一个槽位，其中包括 `0.3.5` 新增的两个 —— **`Plan Studio`** 与 **`Agent Board`** —— 在未绑定时全部禁用。状态栏 `MODE — · PERM — · CONTEXT — · EVENTS #0 · LANE — · LATENCY — · TOKENS — · DIAG 0× · REQ —` | [`01`](native-0.3.5/01-welcome.png) |
| 2 | **`⌘O` 绑定，原生面板路径** | 是 | `⌘O` 打开 `Open project folder` 面板；按工具链注记要求仅用按键驱动（`⌘⇧G`、粘贴路径、`Return`、`Return`）。跳转浮层解析到 `…/e3b/fixture`，面包屑在接受之前予以确认 | [`02`](native-0.3.5/02-open-panel-goto.png) |
| 3 | **驾驶舱已绑定** | 是 | 停靠栏 Environment `Provider fallback`、`Model test-local`、`Mode build`、`Permission ask`、`Tokens 0`；Changes `Lines +0 −0` / `No changes`；Local `Showing Workspace root`、`Branch main`、固定装置路径作为 `Worktree`、`Ahead 0`、`Behind 0`、`Working tree Clean`；状态栏 `MODE build · PERM ask · EVENTS #7 · LANE — · DIAG 0× · REQ 0 req / 0 err`。带外核验：Core 向暂存 `VIDEN_HOME` 写入了 `session_meta` `canonical_root` = 固定装置路径、`work_mode` `build`、`permission_mode` `default`、`model` `test-local`，并铸出 `.viden/project.toml` `prj_1789293381253952000`。**零 Lane 标签条在上下文停靠栏打开时把 `No Lanes yet`、`＋`、项目路径与 `Build` 各自绘制在自己的空间中 —— 这是缺陷 19，原生验证** | [`03`](native-0.3.5/03-cockpit-bound.png) |
| 4 | **无 Lane 时工作区所有者可见** | 是 | `LANE —`，停靠栏 `Commit or push` 绘制的 `Open the commit bar` **启用**（`AXEnabled=true`），标题栏芯片读作 `Fetch from the remote (0 behind). Pull is excluded by contract; this client pushes and fetches only.`，同样启用。两者都没有退回 `D1-OPERATOR-GIT-NO-WORKSPACE-OWNER` —— GUI-CORE-027 在 GUI 上依然关闭 | [`03`](native-0.3.5/03-cockpit-bound.png) |
| 5 | `⌘L` 新建 Lane | 是 | 浮层打开时选中 `Viden Agent built-in`，三个 ACP 适配器在 `Probe did not answer within 15s.` 之下禁用，旁边是 `Retry ACP check`。`Checking ACP agents…` 从未无限期停留 —— **在没有任何 ACP 二进制的主机上，原生验证了缺陷 14 的界限** | [`04`](native-0.3.5/04-new-lane-popover.png) |
| 6 | **所有 ACP 探测未应答时 `Create lane ↵` 仍可用** | 是 | 输入任务后，`Create lane ↵` 读作 `enabled=true`，派生分支变为 `⌁ vd/edit-the-fixture-readme-he`。**缺陷 14，在候选版上原生验证** | [`05`](native-0.3.5/05-new-lane-create-enabled.png) |
| 7 | **一次 Core `lane_create` 审批，带 C15 完整层级列表** | 是 | `Approve lane_create · lane_create`、`Medium risk`、`branch: viden/lane_1789293683646574000 path: …/.worktrees/lane_…`、`Reason: lane mutation policy requires approval`、`Audit: audit_1789293687048689000`，操作为 `Y · Once` / `Allow repo paths` / **`Shift+A · Always · scope_not_offered`** / **`E · Edit · not_editable`** / `N · Deny`，并带修订字段及其说明 `Every key travels, including the ones you left alone: Core evaluates the whole input, not a patch of it.` 两个新层级禁用时携带的是 **Core 自己的拒绝名称**，而非 `GUI-CORE-003` —— **登记项 019 在 GUI 上实时关闭**。导轨徽标 `1`，状态栏 `1 awaiting you` | [`06`](native-0.3.5/06-lane-create-approval.png) |
| 8 | **D2 展示它** | 是 | `Decision Center`、`1 awaiting you`、`GATE APPROVALS · 1`、卡片 `Approve lane_create prj_… · lane_… · pending · medium`、完整详情、`View audit trail`，以及同一组五层操作行 | [`07`](native-0.3.5/07-d2-lane-create-pending.png) |
| 9 | **D2 作出决定且队列清空** | 是 | `Approve once` 释放了效果，卡片**离开队列**：`0 awaiting you`、`GATE APPROVALS · 0`、`Core is holding no decision for you.` **缺陷 16 的前一半，原生验证。** 带外核验：`.worktrees/lane_1789293683646574000/` 存在，`git branch` 列出 `viden/lane_1789293683646574000` | [`08`](native-0.3.5/08-lane-created-d2-cleared.png) |
| 10 | **在该审批之下创建的 Lane** | 是 | 标签条 `lane_1789293683646574000 · coder start… · vi…`，状态栏 `LANE lane_1789293683646574000 — draft —`，标题栏 `1 worktree`，停靠栏 Local 切换为 `Showing Lane worktree` 并显示 Lane 自己的分支与工作树，`LANE OUTPUT` 携带 Core 自己的 `HEAD is now at 2b593e1 Add the fixture README` / `Preparing worktree (new branch 'viden/lane_1789293683646574000')` | [`09`](native-0.3.5/09-lane-created-conversation.png) |
| 11 | **Lane 路径审批写下持久审计行** | 是 | 在暂存 `workflows/projects/*/audit.jsonl` 中逐字节核验：`audit_1789293687048689000`，actor `operator`，action `approval.allow_once`，outcome `success`，objects `permission:lane-approval_1789293687048688000`、`tool:lane_create`、`job:gui-d1-6df9db2e-…`、`lane:lane_1789293683646574000`，`args {"scope":"allow_once"}` —— **正是请求早已在屏幕上显示过的那个 id**。在第二个固定装置中通过 D1 权限停靠栏（而非 D2）复现为 `audit_1789295715906249000`。**兼容性跟进项 15 在 Lane 路径上实时关闭，E1 缺陷 11 随之关闭** | — |
| 12 | **在该 Lane 中使用 composer `tool edit_file …`** | **否** | composer 可编辑并接受文本，而提交**没有离开**：在两个应用会话中十次尝试，`EVENTS` 固定在 `#25`，屏幕上没有任何消息，composer 也从未清空。见诊断与缺陷 22 | [`10`](native-0.3.5/10-step5-submit-dropped.png) |
| 13 | D2 两个连续工具审批、内联工具差异、检查运行、EvidenceView 归档补丁、该编辑运行时路径的 D14 `approval.allow_once`、排队跟进排空 | **否** | 六项全部位于第 12 步之后 | — |
| 14 | **Plan Studio（D15），基于 Core 事实** | 是（为空） | 导轨 `plan` 槽位打开中央窗格视图并保留外框：`PLAN STUDIO`、`BUILD`、`No plan proposed`、`lane_… · no plan record`、`Core holds no plan for this owner. Ask in Plan mode and the planner writes one with update_plan; it appears here the moment the first revision is proposed.`、`Plan mode blocks file, shell, Git, memory and task mutations.`，一条读作 `Core holds no plan record for this owner.` 的 `Plan record` 记录轨（除 `owner` 外每个字段均为 `—`），以及 `STEPS BY ROLE · no role on any step`。**缺席被绘制为缺席，绝不是一个带可用 Accept 的空计划** | [`15`](native-0.3.5/15-plan-studio.png) |
| 15 | Plan 模式、提出的计划、D15 中的 `update_plan`、Accept、Plan 模式下 Hand off 被拒、Build、Hand off、D13 的 DAG | **否** | 计划修订来自规划器的 `update_plan` 调用，而它需要一个回合。位于第 12 步之后，不可达 | — |
| 16 | **Agent Board（D16），基于 Core 事实** | 是（为空） | 导轨 `robot` 槽位打开中央窗格视图并保留外框：`AGENT BOARD`、`Agent Board 1 lanes · 0 sessions · 0 awaiting you`，Lane 行 `lane_1789295710755489000 ⎇ viden/lane_… built_in` 下方是 `no session under this Lane yet`，详情窗格为 `No session selected / pick a session under a Lane on the left`，其下是 `With no Lane there is nothing to supervise — the detail pane stays empty rather than showing a placeholder agent.` **看板的规则 1 —— 会话绝不与其 Lane 分离 —— 及其空状态按设计绘制** | [`16`](native-0.3.5/16-agent-board.png) |
| 17 | Pause → Paused 并保留输入 → Resume → Kill → Killed → Retry | **否** | Agent 会话来自已启动的回合。位于第 12 步之后，不可达 | — |
| 18 | **在 Lane 脏树之上的 DiffReview** | 是 | `Re-read the workspace diff` 将 Core 的采样刷新为 `1 files · +1 −1`、`README.md`、`@@ -1,4 +1,4 @@`、`-# E3b Fixture` / `+# E3b Edited in the Lane`，并带逐行编号。**停靠栏的 `Changes` 总数、其文件列表与 `Local` 标志现已一致 —— `Lines +1 −1`、`M README.md +1 −1`、`Working tree Dirty` —— 这是缺陷 20，原生验证** | [`11`](native-0.3.5/11-diffreview-lane.png) |
| 19 | **带 Lane 在 Core 审批之下暂存** | 是 | `Stage all` → `Stage is waiting for approval in the permission dock.` D2 显示 `Approve git_add prj_… · lane_… · pending · low`、`TOOL INPUT PREVIEW all: true`、Core 自己的 `Core exposes an opaque input preview; structured diff rows are unavailable. · GUI-CORE-012`、`EVIDENCE Core published no evidence for this decision.` 以及 `Decision and reason are written to audit audit_1789296485837909000`。`Approve once` 完成暂存：` M README.md` → `M  README.md` | [`12`](native-0.3.5/12-d2-git-add-lane.png) |
| 20 | **带 Lane 的暂存 `OperatorGitOutcome::Completed`** | 是 | 提交栏渲染 `Stage completed · viden/lane_1789295710755489000 ↑0 ↓0 · uncommitted changes remain`，且一旦键入消息 `Commit` 即刻可用。**缺陷 17，原生验证：在途标志在完成时被清除** | — |
| 21 | **带 Lane、在 Core 审批之下、带 C1 类型化决策上下文的提交** | 是 | `Approve git_commit prj_… · lane_… · pending · medium`、`TOOL INPUT PREVIEW message: … path: …`，以及 **Core 的类型化 hunk 而非工具输入** —— `README.md`、`@@ -1,4 +1,4 @@`、`-# E3b Fixture` / `+# E3b Edited in the Lane` 并带上下文行。`Approve once` 完成提交。带外核验：`c98308e Edit the fixture README heading in the Lane`，工作树干净。**在 GUI 中于选中 Lane 的情况下实时观察到提交的 `OperatorGitOutcome::Completed`** —— E3a 只在 `LANE —` 下观察到过 | [`13`](native-0.3.5/13-commit-approval-hunks.png) |
| 22 | 推送被 `NoUpstream` 拒绝、设置上游、推送被接受 | **否** | 第二处改动以同样方式经 `Commit & Push ⇡` 暂存并提交（`1be4d0b Add the push-leg line`），其 `git_commit` 一半被正确审批。随后 `git_push` 一半没有到达任何可决定的界面：提交栏停在 `Push is waiting for approval in the permission dock.` 且 `Commit & Push ⇡` 禁用，D2 读作 `0 awaiting you`，没有出现徽标，审计存储中也从未写入任何 `git_push` 行 —— 轮询 40 秒。见缺陷 25 | [`14`](native-0.3.5/14-push-waiting-no-approval.png) |
| 23 | **D14 审计时间线** | 是 | `Audit trail 13`、`Outcomes on the loaded page · 13 success 13`，`source.commit operator success … phase=completed` 与其 `phase=authorized tool=git_commit` 成对出现，以及 `approval.allow_once operator success · permission · approval_… · tool · git_commit · job · gui-git-e152f078-… · scope=allow_once`，每一行都携带 `lane · lane_1789295710755489000`。Lane 路径审批的 `tool · lane_create` 行位于同一已加载页。与暂存 `audit.jsonl` 逐字节核验。**跟进项 15 的那一行既在 D14 中可见，也持久落盘** | [`18`](native-0.3.5/18-d14-audit.png) |
| 24 | EvidenceView 归档补丁、规范化、正确的路径标签（跟进项 14） | **否** | 两个原因，都具名而不合并：C7 为原生*工具*变更归档 `patch`，而没有工具变更可做（第 12 步）；并且一旦客户端命令槽位被占用，Evidence 导轨槽位就不再挂载其窗格 —— 控件有响应，中央区域不变。后者是缺陷 22 的后果 | — |
| 25 | **专注模式 `⌘.`** | 是 | `Focus mode ⌘.` 控件隐藏了上下文停靠栏并把整窗宽度交给中央区域，标题栏、导轨、composer 与状态栏全部保留；审计行重排为每行一条 | [`20`](native-0.3.5/20-focus-mode.png) |
| 26 | **`Esc` 返回** | 是 | `Esc` 从审计时间线返回对话视图，外框保留且专注模式仍然开启。标签条在全宽下绘制整洁 | [`21`](native-0.3.5/21-esc-return.png) |

### H4 的第 5 步，已诊断

H4 留下的原文是："创建出的 Lane 带着一个活动回合（屏幕上有 `Cancel active
turn`、`composer.busy`），提交没有离开：`EVENTS` 停在 `#25`"。其中每一项都复现
了。但那个解读不成立。

**不存在回合，也没有任何东西在等待审批。** Lane 的持久记录是 `status: draft`、
`task_id: null`、`active_session_ids: []`（暂存
`workflows/projects/*/lanes.jsonl`）。整个运行的 session JSONL 只有
`session_meta` 行，别无其他 —— 两个固定装置中都没有 `TurnStarted`、没有
`TurnFinished`、没有工具调用。D2 读作 `0 awaiting you`，审计存储中也没有待决
决定。因此 Lane 的初始任务 —— 浮层的 TASK 字段 —— **从未在 fallback 供应商上
变成一个回合**，客户端也没有被卡在某个决策中心未能显示的审批之后。

**`composer.busy` 为假。** composer 上方的标签在每一帧中都读作 `Prompt`
（`d1.composer.prompt`），从未读作 `Queue follow-up`（`d1.composer.queue`），
而后者正是 `apps/gui/src/screens/d1_cockpit.ts:4872` 在 `busy` 置位时打印的。
屏幕上的是 `Cancel active turn`，而该控件由 `composer.canCancel` 绘制，其计算为
`supports_owner && selected_lane.is_active() && exact_binding.is_some()`
（`apps/gui/src-tauri/src/projection.rs:2163-2165`）—— 一个从不读取 `busy` 或
`active_turns` 的谓词。因此一个带有 worker 绑定的 `draft` Lane 会宣告一个并不
存在的回合（缺陷 24）。

**真正发生的是客户端唯一的命令槽位从未被释放。** `sendComposerIntent` 到达
`sendAndWait`，而后者的第一条语句是
`if (sending || pendingCommandId) return null;`
（`apps/gui/src/screens/d1_cockpit.ts:3179`）—— 一次静默返回，没有消息、不清空
composer、也不发出命令。祸首不是 `composerDispatchQueued`：它会禁用发送控件，而
`Send message` 全程读作 `enabled=true`。`sending` 不可能卡住，因为
`sendAndWait` 在 `finally` 中清除它。从渲染出的可访问性状态排除法可知，
`pendingCommandId` 已置位且保持置位。

是哪个命令：宿主的 `PendingD1Kind::CreateStarterLane` 到达了 `accepted == true`
—— 这可从屏幕证明，因为把 Lane 创建审批路由进权限停靠栏正是以此为门槛
（`apps/gui/src-tauri/src/adapter.rs:2023-2032`），而停靠栏确实渲染了它。此后
唯一能了结它的，是其收据与预览的 `preview_id` 及 `content_sha256` 相匹配的
`StarterLaneCreated`（`apps/gui/src-tauri/src/adapter.rs:610-615`）；而 Core 正是
用它匹配过的那份已存预览来构造该收据
（`crates/lanes/src/lane_supervisor.rs:1088-1099`），因此数值不匹配是不可能的。
所以该收据并未到达。

四项实验把客户端与 Core 区分开，四项指向同一结论：

- 十次提交与一次 `Cancel active turn`，历时八十余秒，全部被丢弃且 `EVENTS`
  固定不动 —— 因此不是竞态；
- 在 **D2** 中决定审批与在 **D1 权限停靠栏**中决定审批，结果相同 —— 因此不是
  决策界面的问题；
- 离开驾驶舱进入 DiffReview 再返回（H4 使之成为一次全新的 Core 读取）之后结果
  相同 —— 因此重新挂载并不能清除它，因为新的控制器会从宿主重新读取待决 id；
- 在第二个应用会话中、针对第二个固定装置、使用干净的 `VIDEN_HOME` 复现 ——
  因此不是累积状态。

与此同时 Core 并未卡死：在同一个进程中，`QueryWorkspaceDiff`、
`QueryWorkspaceFiles`、`QueryAudit` 以及全部四个 operator-git 命令都运行至完成，
其效果落盘、审计行写入。缺失的是一个命令的收据，而不是 Core 的 worker。

**这属于 Core 侧，因此按本批次的授权在此编号并停止，而不是修复。** 没有编辑任何
`crates/**` 文件。

**更正，2026-09-13（H5）。** 上面最后那条推断是错的，它被原样保留，以便推理过程可
被审计。读数 —— 某一条命令的收据缺失，而 Core 的 worker 仍在继续作答 —— 是对的；
归属不对。Core 在这条完全相同的路径上正确地发出了收据，由一个首次运行即 GREEN 的
runtime 测试证明。收据是在 Core 发布*之后*丢失的，丢在桌面外壳自己的后台事件 pump
里：它排空了该事件，却没有把它交给客户端的 D1 pending。更正后的根因、它的 RED 与修
复记录在下面的缺陷 22 中。

### E3b 发现的缺陷与缺口

编号接续上文二十一项。每一项都是复现出来的，不是推断出来的。此处没有修复任何
一项。

22. **在审批之下创建的 starter Lane 从未了结其创建命令，因而驾驶舱唯一的命令
    槽位在进程整个生命周期内保持被占用**（GUI 宿主，阻塞）。**2026-09-13 由 H5
    修复**（`05764cb2`）。E3b 把它编号为 Core 侧并就此停手；复现说明并非如此。
    一个 runtime 测试驱动了完全相同的原生路径 —— 真实 Git 工作区中的
    `PreviewDefaultStarterLane`、真实的 `LocalLaneEffectExecutor`、以 allow-once
    决定的 `lane_create` 审批 —— Core 在 `LaneUpdated` 之后发出
    `StarterLaneCreated`，此前没有任何作废事件，首次运行即为 GREEN
    （`crates/runtime/src/tests/frontend_services_tests.rs:2331`,
    `default_starter_lane_in_real_git_workspace_settles_its_creation_receipt`）。
    RED 在上一层，在传输层。`reconcile_ambient_pendings`
    （`apps/gui/src-tauri/src/adapter.rs:6252`）—— H4 为"无论哪一次排空消费了信
    封，都要调和每一个相关命令"而引入的漏斗 —— 把事件交给了每一个 pending，
    **唯独漏掉 `pending_d1`**，后者只有 `poll_d1` 观察。而在已发布的桌面外壳
    里，看到 D1 回执的排空几乎从来不是 `poll_d1`：`spawn_core_event_pump`
    （`apps/gui/src-tauri/src/lib.rs:1484`）在后台线程上持有有界接收，webview 只
    在该 pump 发出的唤醒之后才读取，于是 `pump_events`（`adapter.rs:6277`）消费
    了 `StarterLaneCreated`，而创建命令从未被结算。在 allow-once 与屏幕自身的轮
    询之间插入一次 pump 排空即可精确复现：
    `assertion \`left == right\` failed: a receipt drained by the desktop pump
    must still free the client's one command slot / left: Some("pump-create-lane")
    / right: None`（`apps/gui/src-tauri/src/adapter.rs`,
    `desktop_event_pump_settles_the_lane_creation_it_drains`）。这也解释了 E3b
    记录下却无法解读的对照组：`QueryWorkspaceDiff`、`QueryWorkspaceFiles`、
    `QueryAudit` 以及四个 operator-git 命令在同一进程内照常工作，正因为它们的
    pending 全都*在*漏斗里。`observe_pending_d1` 现在在漏斗内部运行并把结果记在
    adapter 上，因此无论哪一次排空看到回执，下一次 `poll_d1` 都能读到已释放的槽
    位与 confirmed 或 rejected 的结果。规划工作台的 `pending_d15` 与 Agent 看板
    的 `pending_d16` 是最后两个仍留在该漏斗之外的命令，且有着一模一样的洞，因此
    在同一批次中以同样方式关闭，并把结果保存在 adapter 上供各自的下一次轮询读
    回。测试：`apps/gui/tests/h5_native_defects.rs`
    `a_receipt_drained_by_the_desktop_pump_still_frees_the_d1_command_slot`，以及
    上面那个真实 Core 的 `desktop_event_pump_settles_the_lane_creation_it_drains`。
    **弹出框 TASK 字段在这条路径上做了什么**（那次运行无法看到）：随创建一同抵
    达 Core 的什么也没有。在本基线上 `StarterLaneRequest` 不携带 task，D4 发送
    `agent: None`（`apps/gui/src-tauri/src/adapter.rs:4726`），因此 Core 的默认预
    览不绑定任何 agent、也不启动任何会话 —— 收据的 `session` 为 `None`。该任务
    被客户端保存在 `pendingLaneStart`
    （`apps/gui/src/screens/d1_cockpit.ts:3356`），并在 Core 发布已创建的 Lane
    **且** `commandSlotAvailable()` 为真之后，由 `maybeResumeLaneStart`（`:3373`）
    作为一次普通的 composer `SubmitUserInput` 提交。命令槽从未释放，该守卫在每
    一次唤醒时都提前返回，这正是 Lane 的持久记录停留在 `status: draft`、
    `task_id: null`、`active_session_ids: []`，而 session JSONL 中完全没有回合事
    实的原因。Lane 从未停滞；是它的第一条消息从未发出。
23. **客户端无法发送的 composer 提交被静默丢弃**（GUI，对诊断构成阻塞）。
    **2026-09-13 由 H5 修复**（`0296ce0b`）。槽位被占用时 `sendAndWait` 返回
    `null`（`apps/gui/src/screens/d1_cockpit.ts:3179`）。`D1IntentResult` 现在携
    带 `pending_command`（`apps/gui/src-tauri/src/d1.rs:624`）—— 被占用 pending
    的命令 id、它稳定的 Core 命令名、自客户端发出以来的时长，以及宿主是否允许放
    弃该等待 —— composer 渲染一个 `role=alert` 的 `data-d1-slot-busy`，写明种类
    与时长，保留已输入文本，仅对两个 adapter 发现读操作提供宿主自己的放弃入口，
    其余情况则明说该命令带有副作用、其等待绝不会由客户端放弃。
    `PendingD1Kind::is_abandonable`（`adapter.rs:465`）与 `abandon_pending_probe`
    执行同一个判定，保持为一个函数，使该控件不可能出现在宿主会拒绝的地方。测
    试：`apps/gui/tests/h5_native_defects.rs`
    `an_occupied_command_slot_publishes_its_kind_age_and_whether_it_may_be_abandoned`，
    以及 `apps/gui/tests/h5_native_defects.spec.ts` 的"defect 23 — a refused
    submit names what holds the command slot"（五个用例；修复前为 RED，在
    `[data-d1-slot-busy]` 上报 `expected null not to be null`）。
24. **`Cancel active turn` 为一个没有回合的 Lane 绘制**（GUI，诚实性）。
    **2026-09-13 由 H5 修复**（`26eff93d`）。`composer.can_cancel`
    （`apps/gui/src-tauri/src/projection.rs:2172`）读的是
    `lane.is_active() && exact_binding.is_some()`，两者都不是关于工作的事实：
    owner 绑定在回合结束后仍保留其 `turn_id`。它现在依据 `busy` —— Core 自己
    针对该 Lane 的 `active_turns` 条目，或其 owner 下处于 running / starting /
    waiting-approval 的 Agent 会话。RED 是那条把旧读法固化下来的规范投影断言：
    `panicked at apps/gui/src-tauri/../tests/d1_cockpit.rs:139:5: assertion
    failed: projection.composer.can_cancel`。测试：
    `apps/gui/tests/h5_native_defects.rs`
    `cancel_is_not_offered_for_a_bound_lane_core_is_running_no_turn_for` 与
    `cancel_is_offered_once_core_reports_an_active_turn_for_the_lane`，外加
    `h5_native_defects.spec.ts` 中的两个渲染用例。
25. **`Commit & Push ⇡` 的 `git_push` 一半没有到达任何可决定的界面**（GUI，对
    推送构成阻塞）。**H5 之后仍然开放，并且它不是缺陷 22。** H5 在插入了桌面
    pump 的进程内真实 Core 上探查了这一对命令
    （`apps/gui/src-tauri/src/adapter.rs`,
    `the_push_half_of_commit_and_push_reaches_a_decidable_surface`）：在
    `git_commit` 一半被决定、提交落地之后，Core 提出它自己的 `git_push` 审批，
    `operator_git` 报告 `awaiting_approval`，权限坞承载 `tool_name == "git_push"`
    的请求，决策中心的 `pending_total` 为 `1`，答复之后该操作以 Core 的类型化
    `NoUpstream` 结果结算。权限门也运行在上游检查之前
    （`crates/runtime/src/operator_git.rs:85` 对 `:157`），因此缺少远端无法压制
    该审批。该探针以工作区为目标运行；原生运行则选中了 Lane，所以留给 E3c 的嫌
    疑落在 Lane 作用域的路由上 —— `GuiCoreAdapter::operator_git_owner` 与
    `RuntimeProjection::permission_dock_for_owner` —— 而不是命令槽。缺陷 16 在两
    审批动作的*第二个*审批上残存的形状，仍如 E3b 所记录。
26. **针对已持有已创建 Lane 的 `VIDEN_HOME` 重新启动会留下空白窗口**（GUI，对
    重启构成阻塞）。在设置了 `VIDEN_GUI_WORKSPACE` 且 home 中带有一个 Lane 时，
    进程启动、绘制出窗口框架、在 web 区域中不放任何内容 —— 可访问性树中只有窗口
    及其三个标题栏按钮，没有内容 —— 并以 21–27 % CPU 空转三分钟，直至被终止。
    同一个包、同一个 home 但没有 Lane 时渲染正常；同一个带 Lane 的 home 在之后
    通过 `⌘O` 绑定工作区时也渲染正常。区分因素是启动时针对已恢复 Lane 状态的
    绑定。

### 四项预期分歧，复现与否

`0.3.4` 留下缺陷 10 至 13；E3a 一项也未触及。

- **10 —— 归档补丁渲染出的差异把文件称作重命名。** *未复现；不可达。* 归档为空，
  因为 C7 为原生工具变更归档 `patch`，而没有工具变更可做。C12 在 Core 侧关闭了
  它；在 GUI 上仍未取证。
- **11 —— lane 生命周期审批决定不写持久审计行。** **复现为已修复。** 两次，在两
  个决策界面上。见第 11 步。
- **12 —— `NoUpstream` 恢复没有客户端控件。** *未复现；该拒绝从未出现在屏幕上。*
  E3a 给出的那个更窄的观察依然成立且未改变：两个 GUI 推送入口都不提供设置上游的
  选项，而芯片只有在 Core 采样到 `ahead > 0` 时才启用，这已预设存在上游。
- **13 —— 回合进行中排队的会话跟进从不可观察地处于待决。** *未复现；不可达。*
  没有正在运行的回合可供排队。

### 对缺陷 10–21 的裁定（就触及范围而言）

| # | 在候选版上的裁定 |
| --- | --- |
| 10 | 不可达 —— 归档保持为空；C12 的 Core 侧关闭在此未取证 |
| 11 | **已关闭，原生验证**，在两个决策界面上（第 11 步） |
| 12 | 不可达；E3a 那个更窄的观察未改变 |
| 13 | 不可达 —— 没有回合可供排队 |
| 14 | **已修复，原生确认。** 在没有任何 ACP 二进制的主机上，三个 ACP 探测全部未应答、选中内置 Agent 时 `Create lane ↵` 可用 |
| 15 | **已修复，原生确认。** 有界探测报告 `Probe did not answer within 15s.` 并在旁边给出 `Retry ACP check`；创建从未以它为门槛。Core 自身的 worker 界限（`GUI-CORE-035`）未被触及 |
| 16 | **修复一半，原生确认。** 已决定的审批离开队列且总数变动（第 9 步）。两审批动作的*下一个*审批仍然从不浮现 —— 见缺陷 25 |
| 17 | **已修复，原生确认。** `Stage completed` 在同一会话中使 `Commit` 保持可用（第 20 步） |
| 18 | **未复现，并多出一项数据。** 没有任何已启动实例自行退出；五个全部由本批次终止。窗口离开可访问性树的形状**复现了六次**，每一次都紧随一次引发驾驶舱整体重绘的点击（中央视图切换、审批决定、Lane 选择），每次都应答 `-1719`，并在 3–10 秒后恢复且进程存活。缺陷 26 是同一族中的第三种形状，也是其中最易处理的一种 |
| 19 | **已修复，原生确认。** 零 Lane 标签条在停靠栏打开时分开绘制四个部分（第 3 步） |
| 20 | **已修复，原生确认。** 跨一次重采样后 `Lines`、文件列表与 `Working tree` 一致（第 18 步） |
| 21 | **已修复，原生确认。** 每次提交完成后，变更文件列表重采样为 `0 files · +0 −0` / `No changes in the working tree` |

### 观察，而非缺陷

- **`⌘V` 到不了 Tauri webview。** 应用没有安装 Edit 菜单，因此 E3a 的输入法注记
  所规定的剪贴板粘贴在原生打开面板中有效，在窗口内部则处处无效。本次运行的文本
  录入走的是聚焦字段上的可访问性值设置器，WebKit 会把它转成真实的 DOM input
  事件 —— 由客户端自身派生状态的反应所证明（任务落下的那一刻，新建 Lane 的分支
  变成了 `⌁ vd/edit-the-fixture-readme-he`）。下一次原生运行应使用该路径，而不是
  `keystroke`，也不是 `⌘V`。
- **为拍摄而把进程置前会使打开面板的跳转浮层失焦。** 本次运行的第一次绑定落到了
  *上一个*批次的固定装置上，因为在粘贴与 `Return` 之间拍了一帧，焦点被移走，面板
  接受了它自己记住的路径。规则狭窄而绝对：在向该浮层键入与提交之间不要拍任何帧。
  这已是该浮层第二次静默接受了与所键入内容不同的东西 —— E3a 见过它截断长路径 ——
  因此应核验面包屑，然后中间不插入任何动作地按下 `Return`。
- **这些帧是按窗口 id 拍摄的。** `screencapture -R` 拍到的是占据该矩形的任何窗口，
  而在运行着其他应用的多显示器主机上，另一个窗口在运行中途盖到了 Viden 的矩形上，
  有一帧拍到了它。`screencapture -l<window id>` 即使窗口被遮挡也能拍到它；`0.3.3`
  关于 `-l` 会返回陈旧帧的注记依然成立，因此双次拍摄保留。本批次暂存目录中的
  `shotw.sh` 是应当沿用的版本。
- **权限停靠栏的固定面板覆盖了决策中心自己的列表。** 在 D2 中，停靠栏把同一个请求
  画在它所复制的 `GATE APPROVALS` 卡片之上。没有显示任何虚假内容，两者都可决定；
  记录于此，以便后续保真度检查决定 D2 是否应抑制那份固定副本。

### 这对计划目标 4 意味着什么，按界面

`docs/release-0.3.3-plan.md` 目标 4，在 `0.3.4` 中作为目标 6 延续 —— "一个真实的
本地优先开发任务通过 GUI 完成，从接入到一次已提交的改动，并记录审计与证据"：

- **通过 TUI —— 端到端达成**，与 2026-09-12 的记录相同。
- **通过 `0.3.5` 候选版上的原生 GUI 窗口 —— 比 `0.3.4` 走得更远，仍未走完整个
  任务。** 接入、Core 发布的工作区所有者、在 Core `lane_create` 审批之下的 Lane
  创建（带 C15 完整层级列表，并在两个未提供的层级上给出 Core 自己的拒绝名称）、
  **Lane 路径审批的持久审计行**、决策中心清空一个已决定的审批、一次 Core 采样的
  Lane 差异、在 Lane 内于各自审批之下带 Core 类型化 hunk 的一次暂存与一次**提交**、
  把上述每一个决定都读回来的审计时间线、渲染诚实空状态的 Plan Studio 与 Agent
  Board、专注模式与 `Esc` 返回路径 —— 全部通过驾驶舱驱动完成。
- **驾驶舱仍然做不到的，是在它刚刚创建的 Lane 中启动一个回合**，随之而来的还有
  工具审批、内联工具差异、检查运行、归档补丁、排队跟进、已提出的计划，以及任何
  可暂停、恢复或终止的 Agent 会话。成因是缺陷 22 —— 此处记录为 Core 侧，已由 H5
  更正为 GUI 宿主的事件 pump；参见上面的更正与下面的缺陷 22。
- **诚实的总结：** 目标 4 的源码控制半区在 GUI 上**带 Lane** 与不带 Lane 都已
  得到证明，而 `0.3.5` 目标 2 所指的信任缺口 —— 在 Lane 路径上决定的审批留下一条
  持久审计行 —— **已关闭并原生取证**。目标 4 的 Agent 半区仍然在 TUI 上得到证明，
  在 GUI 上**被阻塞**，阻塞它的是一个与 `0.3.4` 所受阻塞不同、且定位更清楚的缺陷。
  退出标准 1 就"候选版已被原生驱动"而言已达成；就"完整任务"而言尚未达成，本文档
  不作任何相反的粉饰。

## 原生 GUI 运行 —— 2026-09-14（`0.3.5` 候选版 `22124903`，E3c）

这是 E3c：在 H5、G9b、G10 与 DS2 之后，于 `0.3.5` 候选版上通过原生驾驶舱驱动真实
任务的 Lane 半区。分支 `claude/evidence-e3c`，位于 `.worktrees/evidence-e3c`，
基线为集成分支 `claude/int-0.3.5` 的
`22124903c4e21778ad6046fd6779aecf24837bb1` —— 直到 DS2 的每一个 `0.3.5` 批次，
因此 H5 对缺陷 22/23/24 的修复、G9b 的 agent 绑定、G10 的令牌映射修复与 DS2 的
浅色皮肤令牌修复都在被测包中。

宿主屏幕处于解锁状态，并且在每一批按键与点击之前都读取了
`CGSessionCopyCurrentDictionary()`；每一次
`CGSSessionScreenIsLocked` 都是 `0`，从 `2026-09-14T02:06:28Z` 到
`2026-09-14T04:11:29Z`。

**本批次的要点：Lane 的回合已经打通。** H5 的修复在原生环境成立。弹出层的 TASK
字段在 fallback provider 上变成了一个回合，在两个独立的应用会话中各一次，并且
agent 回合所把守的一切都在其后打开了 —— 带 Core 类型化 hunk 的工具审批、允许一次、
内联工具差异、EvidenceView 中的归档补丁、D14 同时读回 Lane 路径**与**运行时路径的
`approval.allow_once`、C15 的 `Always` 及其常驻规则与撤销、以及 C16 带
`revised_from` 链接的类型化 `Edit` 修订。本次运行未能完成的，是**在选中 Lane 的
前提下**的源码控制半区 —— 因为提交栏是针对一个并非当前选中的 Lane 作出拒绝的
（缺陷 30，也就是 E3b 缺陷 25 所点名的嫌疑），以及 Plan Studio 与 Agent Board 的
流程，原因逐步在下表中写明。

此处发现的一个阻塞项属于 GUI 客户端缺陷，已依据启动授权在本批次中以红先方式、
用独立提交修复：缺陷 27。

### 被测应用

`npm ci` 用 4 秒添加了 97 个包。`npx tauri build --bundles app` 通过 ——
`Finished \`release\` profile [optimized] target(s) in 1m 16s` —— 并在
`2026-09-14T02:08:21Z` 产出 `target/release/bundle/macos/Viden.app`：

| 事实 | 值 |
| --- | --- |
| `CFBundleShortVersionString` | `0.1.0-rc.5` |
| `CFBundleIdentifier` | `dev.viden.gui` |
| `CFBundleExecutable` | `viden-gui` |
| 架构 | `Mach-O 64-bit executable arm64` |
| 可执行文件大小 | 42,858,160 字节 |
| 包大小 | 42 MB |
| 签名 | ad-hoc、linker-signed、`TeamIdentifier=not set`、`Sealed Resources=none` |

包仍然带着 `0.1.0-rc.5`。计划的版本表为本里程碑设定的目标是 `0.1.0-rc.6`，而尚无
批次执行该版本号提升；这由 R1 负责。此处记录，是因为一次原生取证运行必须说明它
驱动的是哪个版本。

运行中途在缺陷 27 修复之后重新构建过一次，结果相同
（`Finished \`release\` profile [optimized] target(s) in 1m 03s`，
`2026-09-14T03:19:26Z`）。它从包直接启动，从未安装、以身份签名、公证或分发。每次
启动都把 `VIDEN_HOME` 指向一个临时目录，因此本仓库自己的 `.viden/` 从未被读写。
启动的 PID 依次为：95394、23420、29576。三个都由本批次杀死，没有一个自行退出。

### 夹具

运行临时目录下的一个全新临时 Git 仓库，含一次提交 `5a8b132`（"Add the fixture
README"）、一个五行的 `README.md`、一个覆盖 `.viden/` 与 `.worktrees/` 的
`.gitignore`，以及一个选择 `provider = "fallback"` / `model = "test-local"` 的
`.viden/config.toml`。Core 在引导时铸造了 `.viden/project.toml`
`prj_1789351944807166000`。临时目录中创建了一个裸 `origin`，但从未被使用，因为
没有任何推送路径可达（缺陷 30）。

### 与脚本任务的一处偏离，以及原因

**源码控制半区所需的 Lane 工作树改动是在带外做出的**，与 E3a、E3b 相同，而本次
运行现在可以精确说明原因：产品自己的 `edit_file` **确实**运行了，在它自己的审批
之下、在选中 Lane 且 Core 的 `turn_owner` 指名该 Lane 的情况下 —— 而它写入的是
**工作区根目录**，不是 Lane 工作树（缺陷 28）。因此在本候选版上，产品无法把 Lane
工作树弄脏。脏树下游的一切 —— Core 的采样、文件清单、`Local` 标记 —— 都是真实的
GUI 驱动的 Core 行为，作用在一次真实改动上；只有这一次改动的作者身份不属于产品。

### 逐步记录

每一帧都按窗口 id 拍摄两次并保留第二帧，列出的每一个 PNG 在列出之前都已查看。
帧位于 `native-0.3.5/` 下，前缀为 `e3c-`。

| # | 步骤 | 是否到达 | 结果 / 原因 | 帧 |
| --- | --- | --- | --- | --- |
| 1 | 欢迎页 | 是 | `No project open`、`Open project ⌘ O`、`Core adapter is not connected`；十一个活动栏槽位，含 `Plan Studio` 与 `Agent Board`，未绑定时全部禁用；`MODE — · PERM — · CONTEXT — · EVENTS #0 · LANE — · LATENCY — · TOKENS — · DIAG 0× · REQ —` | [`01`](native-0.3.5/e3c-01-welcome.png) |
| 2 | **`⌘O` 绑定，原生面板路径** | 是 | `⌘O`、`⌘⇧G`、粘贴路径、`Return`、`Return`。第一次尝试什么也没绑定，因为按键送到了另一个应用；下面的工装注记是新的，且是绝对要求 | [`02`](native-0.3.5/e3c-02-open-panel-goto.png) |
| 3 | **驾驶舱已绑定** | 是 | 停靠区 `Provider fallback`、`Model test-local`、`Mode build`、`Permission ask`、`Tokens 0`；`Lines +0 −0` / `No changes`；`Showing Workspace root`、`Branch main`、夹具路径作为 `Worktree`、`Ahead 0`、`Behind 0`、`Working tree Clean`；`MODE build · PERM ask · EVENTS #7 · LANE — · DIAG 0× · REQ 0 req / 0 err`。零 Lane 标签条在停靠区打开时把 `No Lanes yet`、`＋`、路径与 `Build` 各自画在自己的空间里 —— **缺陷 19，原生环境，再次确认** | [`03`](native-0.3.5/e3c-03-cockpit-bound.png) |
| 4 | **`⌘L` 新建 Lane，内置 agent，TASK** | 是 | 选中 `Viden Agent built-in`；Codex、Claude Agent 与 Kiro CLI 在 `Probe did not answer within 15s.` 之下禁用，旁边是 `Retry ACP check`；`Checking ACP agents… each probe is given up to 15s.` 从未无期限地悬着。填入 TASK 后 `Create lane ↵` 读作 `enabled=true`，分支变为 `⌁ vd/edit-the-fixture-readme-he`。**缺陷 14 与 15，原生环境，在一台没有任何 ACP 二进制的宿主上** | [`04`](native-0.3.5/e3c-04-new-lane-popover.png)、[`05`](native-0.3.5/e3c-05-new-lane-task-set.png) |
| 5 | **一次带 C15 完整层级列表的 Core `lane_create` 审批** | 是 | `Approve lane_create · lane_create`、`Medium risk`、`branch: viden/lane_1789352091587813000 path: …/.worktrees/lane_…`、`Reason: lane mutation policy requires approval`、`Audit: audit_1789352092704825000`，五个层级为 `Y · Once` / `Allow repo paths` / **`Shift+A · Always · scope_not_offered`** / **`E · Edit · not_editable`** / `N · Deny`。两个禁用层级带的是 **Core 自己的拒绝语句**，而不是 `GUI-CORE-*` 占位 —— **登记项 019 在 GUI 侧实时关闭** | [`06`](native-0.3.5/e3c-06-lane-create-approval.png) |
| 6 | **D2 显示并决定它** | 是 | `Decision Center`、`1 awaiting you`、`GATE APPROVALS · 1`、卡片 `Approve lane_create prj_… · lane_… · pending · medium`、`TOOL INPUT PREVIEW`、`EVIDENCE Core published no evidence for this decision.`。`Approve once` 放行了效果，卡片**离开了队列**：`0 awaiting you`、`GATE APPROVALS · 0`、`Core is holding no decision for you.` **缺陷 16 的前半，原生环境** | [`07`](native-0.3.5/e3c-07-d2-lane-create-pending.png)、[`08`](native-0.3.5/e3c-08-lane-created-d2-cleared.png) |
| 7 | **在该审批之下创建出的 Lane** | 是 | 标签条 `lane_1789352091587813000 coder starter…`、侧栏行 `lane_… Viden · draft`、`LANE lane_1789352091587813000 — draft —`、标题栏 `1 worktree`、停靠区 `Showing Lane worktree` 带该 Lane 自己的分支与工作树、`LANE OUTPUT` 承载 Core 自己的 `HEAD is now at 5a8b132 Add the fixture README` / `Preparing worktree (new branch 'viden/lane_…')`。带外核验：工作树存在，`git branch` 列出 `viden/lane_1789352091587813000`。**侧栏行上的 `Viden` 就是 G9b 的 agent 绑定在传递；`lane-agent-bindings.jsonl` 带 `agent_id: "viden"`** | [`09`](native-0.3.5/e3c-09-lane-task-became-a-turn.png) |
| 8 | **弹出层的 TASK 变成一个回合 —— 缺陷 22 关闭** | 是 | 编辑器释放（`Prompt`、`Send message enabled`、**没有 `Cancel active turn`**），`EVENTS` 从 `#19` 推进到 `#44`，`LATENCY 19ms avg 19ms`，`REQ 1 req / 0 err`。在临时会话 JSONL 中逐字节核验：`turn_owner` 带 `lane_id: lane_1789352091587813000` 与 `turn_id: turn_1789352172234403000`、用户消息 `Edit the fixture README heading in the Lane`、一条 `outcome: success` 的成本行（`fallback` / `test-local`）、助手回复、`task_updated` `status: done` / `activity: provider turn complete`，随后 `turn_owner` 被清空。**H5 的修复在原生环境成立**，并在第二个应用会话中为第二个 Lane 再次复现 | [`09`](native-0.3.5/e3c-09-lane-task-became-a-turn.png) |
| 9 | **编辑器 `tool edit_file path=README.md old=Fixture new=Edited`** | 是 | 提交离开了客户端，Core 抬起 `Approve edit_file · edit_file`、`Medium risk`、`TOOL INPUT PREVIEW new: Edited old: Fixture path: README.md`，以及 **Core 的类型化 C1 hunk** `@@ -1,5 +1,5 @@`，含 `-# E3c Fixture` / `+# E3c Edited`，层级为 `Y · Once` / `Allow repo paths` / `Shift+A · Always`（**已启用**）/ `E · Edit · not_editable` / `N · Deny`。`LIVE WORK APPROVE EDIT_FILE`、`Core is working; no owner-scoped status is available. typed`、`0:40`、`Cancel turn · Esc`，编辑器标签切换为 `Queue follow-up` | [`10`](native-0.3.5/e3c-10-tool-approval-hunks.png) |
| 10 | **允许一次，以及内联工具差异** | 是 | `Y · Once` 放行。转录区画出 `▸ Modified README.md +1 −1`，展开为 `README.md`、`@@ -1,5 +1,5 @@`、`-# E3c Fixture` / `+# E3c Edited`，带逐行行号。编辑器释放，`Cancel active turn` 随回合一起消失 —— **缺陷 24，原生环境，双向确认** | [`11`](native-0.3.5/e3c-11-after-allow-once.png)、[`12`](native-0.3.5/e3c-12-inline-tool-diff.png) |
| 11 | 检查运行 | **否** | Core 未为该次工具调用提供任何检查；未作任何推断 | — |
| 12 | **EvidenceView 归档补丁 —— 后续项 14** | 是 | `± Patch · native lane lane_… edit: README.md (+1/-1)`、`evidence id patch-tool_1789352339666433000`、`kind patch`、`source native`、**`path README.md`**、`owner lane lane_1789352091587813000`、`canonical item ctxi_…`、`bundle ctx-main-session_…`、`source hash f4aa16edd7def41430e7e74bf07acaccd11bda1063cfa0fb38f9501b9d7f9b5c`、`producer native · coder`、`verification verified`、`canonical quality pass`，以及 `Archive complete — Core published every entry matching this scope.` **路径标签就是文件自己的路径，规范且正确** | [`13`](native-0.3.5/e3c-13-evidence-archived-patch.png) |
| 13 | 缺陷 10 —— 归档补丁的渲染差异把文件叫作重命名 | **否** | `Open in review` 导航到工作树的 DiffReview，而不是补丁自己的渲染差异，而 Lane 工作树是干净的，因为那次编辑落在了工作区根目录（缺陷 28）。什么也没有渲染，因此对重命名形态无从判断 | [`14`](native-0.3.5/e3c-14-archived-patch-review.png) |
| 14 | **D14 为两条路径都显示 `approval.allow_once` —— 后续项 15** | 是 | `Audit trail 2`、`Outcomes on the loaded page · 2 success 2`、`approval.allow_once operator success · permission · approval_1789352339708803000 · tool · edit_file · job · gui-d1-39b14aaf-… · scope=allow_once`，与 `approval.allow_once operator success · permission · lane-approval_1789352092704824000 · tool · lane_create · job · gui-d1-dc9c6b60-… · lane · lane_1789352091587813000 · scope=allow_once` 并列，以及 `Core reports the audit timeline is complete.`。与临时 `audit.jsonl` 逐字节核验，id 正是两次请求早已在屏幕上显示过的那两个。**Lane 路径与运行时路径，并排** | [`15`](native-0.3.5/e3c-15-d14-audit-allow-once.png) |
| 15 | **在回合进行中排队一条跟进，然后看它被排干 —— 后续项 16** | **否** | 标签读作 `Queue follow-up`，而提交被**拒绝**：`Not sent. This client is still waiting for Core's answer to submit_user_input, asked 0s ago. Your message is kept. submit_user_input carries an effect, so the wait is never abandoned here: only Core's answer records whether it ran.` 审批决定之后编辑器被清空，被保留的文本从未发出 —— 会话 JSONL 中不存在这条消息。参见缺陷 29 | [`16`](native-0.3.5/e3c-16-followup-queued.png) |
| 16 | **缺陷 23 的可见拒绝** | 是 | 上面那条提示就是该拒绝，稍后还出现了第二种形态，指名 `set_work_mode`。**缺陷 23 关闭，原生环境确认两次** | [`16`](native-0.3.5/e3c-16-followup-queued.png)、[`26`](native-0.3.5/e3c-26-set-work-mode-slot-held.png) |
| 17 | **以 `Always` 回答一个中风险请求（C15）** | 是 | 对第二次 `edit_file` 按下 `Shift+A · Always`。编辑生效（`# E3c Requeued`），审计库收下 `audit_1789352746460919000 approval.always … args {"scope":"always"}` | [`17`](native-0.3.5/e3c-17-always-granted.png) |
| 18 | **重复请求被自动允许** | 是，并写明作用域 | 一个**不同**的输入（`old=Requeued new=AutoAllowed`）再次询问 —— Core 把常驻规则以整个工具输入为键，于是 `1 awaiting you` 与一条新的 `audit_1789353002795519000`。**完全相同**的输入（`old=Edited new=Requeued`）则**被自动允许**：屏幕上没有请求、没有新的审计行，回合一直跑到它自己的类型化失败 `edit_file could not find the target text`。C15 的 `Always` 生效，且以输入为作用域，这一点设置面板也直白说明了 | [`18`](native-0.3.5/e3c-18-always-not-matched-by-a-new-input.png)、[`20`](native-0.3.5/e3c-20-identical-input-auto-allowed.png) |
| 19 | **设置中的常驻列表与撤销（C15）** | 效果为是，列表为**否** | `Settings · Core-owned preferences` → `Standing approvals`、`Rules an Always decision installed. They hold across sessions and repositories until revoked.`、行 `edit_file · new=Requeued old=Edited …`、`granted 1789352875 in ws_65ba50067a87fc27`、`Revoke`、`Open the audit trail`。`Revoke` 之后 `Save` 写下了 `audit_1789353306646568000 approval.revoke … args {"scope":"always"}`，且完全相同的输入**再次询问**，因此 Core 确实撤销了。而该面板重新打开后，**仍然列着这条已被撤销的规则以及它的 `Revoke` 按钮**。参见缺陷 31 | [`19`](native-0.3.5/e3c-19-standing-approvals.png)、[`21`](native-0.3.5/e3c-21-revoke-pressed-rule-still-listed.png)、[`22`](native-0.3.5/e3c-22-after-save-rule-still-listed.png) |
| 20 | **用 `E` 修订一个可编辑请求，并显示 `revised_from` 链接（C16）** | 是 | `tool shell command=echo …` 抬起 `Approve shell · shell`、`Scope: Once, Always, Edit`，其中 **`E · Edit` 已启用** —— `shell` 在 `EDITABLE_APPROVAL_TOOLS` 中而 `edit_file` 不在，被禁用的层级用 Core 自己的话说明了这一点。`E` 打开了一个**类型化**修订面：一个名为 `command`、值为 `echo` 的 `AXTextField`，而不是一团原始文本。改为 `pwd` 并 `Send the revision`：Core 抬起了一个**新的**请求，带着 **`This request revises approval_1789353592354218000; Core re-evaluated the narrowed input and still wants a decision.`**，审计库收下 `audit_1789353592354220000 approval.edit … args {"revised_input":"command=pwd","scope":"edit"}`。**C16 的类型化输入与 `revised_from` 链接，原生环境** | [`23`](native-0.3.5/e3c-23-shell-editable-approval.png)、[`24`](native-0.3.5/e3c-24-edit-typed-input.png)、[`25`](native-0.3.5/e3c-25-revised-from.png) |
| 21 | **Plan 模式** | 是，在缺陷 27 修复之后 | 工作模式药丸的弹出层是一个真实的 popover：第一次尝试把客户端唯一的命令槽占据了整个进程生命周期（缺陷 27，已在本批次修复）。在重建后的包上切换得以落定，`MODE plan` 并带 **`PERM read_only`** —— Core 自己的工作模式/权限耦合规则，作为 Core 真相应用，而不是客户端的乐观编辑 —— 紧接着的一次编辑器提交顺利发出并跑完一个回合，没有任何槽位占用提示 | [`26`](native-0.3.5/e3c-26-set-work-mode-slot-held.png)、[`28`](native-0.3.5/e3c-28-plan-mode-slot-free.png)、[`29`](native-0.3.5/e3c-29-plan-mode-read-only.png) |
| 22 | Plan Studio：D15 中的 `update_plan` 提案、Accept、plan 模式下 Hand off 被拒、Build 模式、Hand off、D13 的 DAG | **否** | 两个写明的原因。`update_plan` 到达了 Core，Core 用自己的类型化拒绝作答 —— `` `plan` is not a JSON array of plan steps `` —— 而格式正确的 JSON 无法输入，因为编辑器应用了 macOS 的自动引号替换，把 `"` 变成了 `”`（缺陷 32，附一行最小复现）。另外，一旦 Core 报告该 Lane 执行已停止，活动栏高亮了 `Plan Studio`，而中央面板仍停在恢复态界面（缺陷 33） | [`30`](native-0.3.5/e3c-30-plan-studio-not-rendered.png) |
| 23 | Agent Board：该 Lane 的会话行、Pause → Paused、Resume、Kill → Killed、Retry | **否** | 内置的 `Viden Agent` 跑的是 provider 回合，而不是 `runtime.agent_session_control` 的 Agent 会话；这台宿主上没有安装任何 ACP 适配器，因此不可能存在可暂停或终止的会话行。这不是缺陷，也不构成证据 | — |
| 24 | **对 Lane 脏树的 DiffReview** | 是 | `Re-read the workspace diff` 刷新了 Core 的采样：停靠区 `Lines +2 −0`、`M README.md +2 −0`、`Showing Lane worktree` 带**当前选中**的 Lane 的分支与工作树、`Working tree Dirty`。**总量、文件清单与 `Local` 标记一致 —— 缺陷 20，原生环境** | [`32`](native-0.3.5/e3c-32-diffreview-lane.png) |
| 25 | **在选中 Lane 的前提下** 在 DiffReview 中提交、`Commit & Push ⇡`、`git_push` 半区（缺陷 25）、设置上游、推送被接受 | **否** | `Open the commit bar` 是**禁用**的，带着 Core 自己的拒绝：`No commit or push can be sent from here: D1-OPERATOR-GIT-OWNER: Lane \`lane_1789352091587813000\` does not have one exact Core operator git owner` —— 而状态栏的 `LANE` 读作 `lane_1789357518367069000`。提交栏是针对一个**并非当前选中**的 Lane 作出拒绝的。这正是 E3b 点名为缺陷 25 剩余嫌疑的 Lane 作用域路由，现在已被观测到；参见缺陷 30。没有任何推送入口可达，因此裸 `origin` 从未被使用 | [`32`](native-0.3.5/e3c-32-diffreview-lane.png) |
| 26 | **缺陷 24 —— 空闲 Lane 上没有 `Cancel active turn`** | 是 | 在每一个空闲帧（`09`、`11`、`12`、`28`）上都不存在，只有在 Core 报告回合时才出现（`10`、`16`、`18`、`23`）。**关闭，原生环境双向确认** | [`09`](native-0.3.5/e3c-09-lane-task-became-a-turn.png)、[`10`](native-0.3.5/e3c-10-tool-approval-hunks.png) |
| 27 | **缺陷 26 —— 用 `VIDEN_GUI_WORKSPACE` 对着已含 Lane 的 home 重新启动绑定** | 是，**未复现** | 进程启动，约十五秒后进入 Accessibility 树，然后**渲染出完整驾驶舱**：两个 Lane 标签、已绑定的停靠区、`2 worktrees`、`MODE build · PERM ask · EVENTS #9`。CPU 在 55 秒内稳定在 10.3%，而不是 E3b 记录的连续三分钟 21–27%。**缺陷 26 在本候选版上不复现** | [`35`](native-0.3.5/e3c-35-workspace-env-relaunch.png) |
| 28 | **专注模式 `⌘.` 与 `Esc` 返回** | 是 | 该控件隐藏了上下文停靠区，把整窗宽度交给中央面板，同时保留标题栏、活动栏、编辑器与状态栏。`Esc` 把活动栏带回 `Conversation`，外框保留，专注模式仍然开着，这与 `0.3.4` 的记录一致 | [`33`](native-0.3.5/e3c-33-focus-mode.png)、[`34`](native-0.3.5/e3c-34-esc-return.png) |

### E3c 发现的缺陷与缺口

编号接续上面的二十六条。每一条都是复现的，不是推断的。其中一条依据启动授权在此
修复；其余均编号后停止。

27. **一个编辑器控件从不结清自己的命令，于是驾驶舱唯一的命令槽被占据整个进程
    生命周期**（GUI，阻塞）。**已于 2026-09-14 在本批次修复**（`fix(gui): settle
    a composer control against the owner Core stamps`）。
    `composer_control_envelope`（`apps/gui/src-tauri/src/adapter.rs:5155`）
    有意发送 `RuntimeOwner::default()` —— 工作模式、权限级别或模型的变更改变的是
    工作区级别的运行时状态，而不是某一个 Lane 的执行。随后 Core 的
    `stamp_workspace_identity`（`crates/runtime/src/runtime_supervisor.rs:5789`）
    会在发出受理事件之前，用它发布的 `workspace_id` 与 `project_id` **补全**这个
    空白 owner。`PendingD1::acceptance_owner_matches`（`adapter.rs:633`）把发出的
    owner 与被补全的 owner 作相等比较，于是 pending 永远不会进入 `accepted`；没有
    受理，确认它的 `SnapshotUpdated` 就会被忽略（`matches_business_fact`，
    `adapter.rs:700-706`）。模式在屏幕上确实变了 —— Core 执行了它 —— 而客户端
    永远在等一个它早已排干的回执。原生表现：`MODE plan`，而其后每一次提交都是
    `Not sent. This client is still waiting for Core's answer to set_work_mode,
    asked 28s ago.`，50 秒后依然如此。现在受理也会匹配 Core 所补全的工作区 owner，
    严格镜像 Core 的规则而不放宽：只有客户端留空的那两个 id 允许不同，因此一个
    Lane 作用域的信封依然会被拒绝。对着进程内真实 Core 的 RED：
    `assertion \`left == right\` failed: a composer control whose bare owner
    Core completed must still free the client's one command slot /
    left: Some("control-owner-mode") / right: None`，同一次运行还断言了模式确实
    到达了 `plan`。测试：`apps/gui/src-tauri/src/adapter.rs` 的
    `a_workspace_level_composer_control_settles_against_the_owner_core_stamps`
    与
    `a_bare_pending_accepts_the_stamped_workspace_owner_and_refuses_a_lane_owner`。
    两个适配器发现读取命令发送同样的空白 owner，也有同样的洞；它们被掩盖了，因为
    `PendingD1Kind::is_abandonable` 会释放它们。
28. **在 Lane 所属回合中调用的工具运行在工作区根目录，而不是 Lane
    工作树**（Core，阻塞 Lane 半区）。在选中 `lane_1789352091587813000`、Core 自己的
    `turn_owner` 带 `lane_id: lane_1789352091587813000` 的情况下，
    `tool edit_file path=README.md` 报告 `path: …/scratchpad/e3c/fixture/README.md`
    并让 Lane 工作树保持干净；工作区根目录的 `git status` 显示 ` M README.md`，
    Lane 工作树的则什么也没有。产品自己也这么说：同一个 Lane 中的
    `tool shell command=pwd` 打印出 `…/scratchpad/e3c/fixture` —— 工作区根目录。
    客户端正确发送了 owner（`turn_owner` 元数据证明它到达了 Core），因此解析点在
    Core 侧：回合的工具工作目录是会话的 `canonical_root`
    （`crates/runtime/src/frontend_services.rs`，会话引擎的根），而不是所属 Lane 的
    `worktree`（`crates/lanes/src/lane_supervisor.rs`，承载它的那条记录）。此处
    编号并停止；未编辑任何 `crates/**` 文件。
    **已于 2026-09-14 由 Core 批次 C17 修复**（`fix(runtime): run a Lane-owned
    turn's tools in the Lane worktree`）。真正的解析点比 E3c 点名的疑犯再往里一层：
    `frontend_services.rs` 读取工作区文件与 diff，并不是工具的工作目录。回合用正确的
    owner 打开了归属窗口，但下方数层的工具循环用 `SessionEngine::cwd` 构造每一个
    `ToolExecutionContext` —— `crates/runtime/src/runtime_loop.rs:604`（审批的
    `decision_context` 预览）、`:702`（注册表执行）与 `:844`（编辑后的 LSP 诊断）。
    原生回合上下文现在携带它被打开时所用的根目录 `NativeTurnContext::tool_root`
    （`crates/runtime/src/work_evidence.rs:101`），由 `native_turn_tool_root`
    （`:435`）从持久 Lane 记录每轮解析一次，并由 `active_tool_root`（`:468`）读回；
    三个站点读同一个值，因此操作者批准的文件与实际改动的文件不可能不同。没有 worktree
    的 Lane 仍在工作区根目录中运行，并且在消费端再次检查包含关系，因为
    `PermissionEngine` 是相对根目录规范化工具的相对路径。没有事件、capability、计数或
    fixture 发生移动。测试：
    `crates/runtime/src/tests/lane_execution_scope_tests.rs:200` 与 `:340`。
    兼容性后续项 20。
29. **回合进行中无法排队跟进；标签承诺了一个队列，提交被拒绝，被保留的文本被
    丢弃**（GUI）。Core 一报告回合，编辑器标签就切换为 `Queue follow-up`，但
    `sendComposerIntent` 依然走那唯一的命令槽，而第一条 `SubmitUserInput` 在 Core
    回答之前一直占着它 —— 而回合在等审批时 Core 不会回答。拒绝本身是诚实的
    （缺陷 23 的提示，且它说 `Your message is kept`），但它上方的承诺不是；并且在
    审批被决定之后编辑器重绘为空：`Summarise what you just changed` 从未进入会话
    JSONL。Core 有 `RuntimeCommand::QueueFollowUp` 并发布 `InputQueued`，
    `PendingD1Kind::Queue` 也已经匹配它（`apps/gui/src-tauri/src/adapter.rs:646`），
    所以契约是在的；是驾驶舱没有把忙碌编辑器的提交路由过去。**因此兼容性后续项 16
    在 GUI 上未达成。**
30. **operator-git owner 不跟随 Lane 选择**（GUI，阻塞带 Lane 的源码控制半区）。在
    选中 `lane_1789357518367069000` 时 —— 状态栏、标题栏与上下文停靠区都指名它 ——
    提交栏被禁用，带着
    `D1-OPERATOR-GIT-OWNER: Lane \`lane_1789352091587813000\` does not have one
    exact Core operator git owner`，而那是工作区里的**另一个** Lane。重新选择任一
    Lane 都不会改变拒绝语句中的名字。这正是 E3b 在缺陷 25 之下留给 E3c 的嫌疑 ——
    `GuiCoreAdapter::operator_git_owner` 与
    `RuntimeProjection::permission_dock_for_owner` —— 它也解释了为什么 E3b 在选中
    Lane 时 `git_push` 半区到不了任何可决断面，而 H5 用工作区目标做的进程内探针却
    发现审批正常抬起。**缺陷 25 保持开放，这是它在客户端一侧被定位到的成因**；
    `git_push` 半区本身没有被重新驱动，因为没有任何提交栏入口可达。
31. **设置中的常驻审批列表仍然显示一条 Core 已撤销的规则**（GUI，诚实性）。
    `Revoke` 之后 `Save` 写下了
    `audit_1789353306646568000 approval.revoke … {"scope":"always"}`，规则也确实
    停止生效 —— 完全相同的工具输入再次请求审批 —— 但该面板关闭再打开后，仍然列着
    `edit_file · new=Requeued old=Edited …`、`granted 1789352875 in
    ws_65ba50067a87fc27` 及其 `Revoke` 按钮。操作者看到的是一条已经不成立的常驻
    规则。
32. **编辑器应用了 macOS 的自动引号替换，于是键入的双引号变成了弯引号**（GUI，
    阻塞操作者键入的任何工具参数或代码）。最小复现：把编辑器设为 `A"b"c`，读回来
    是 `A”b"c`（第一个引号成了 `U+201D`）。它在本次运行中两次破坏了 `update_plan`
    的载荷，位置相同，Core 每次都用自己的类型化解析拒绝作答。任何经编辑器发送的
    JSON、任何带引号的 shell 参数、任何代码片段都受影响。此处的输入路径是
    Accessibility 值设置器，WebKit 把它走在与人类按键相同的编辑管线上，因此替换是
    webview 的而不是工装的；仍欠一次用真实按键的确认，因为这台宿主的输入源是拼音
    输入法，`keystroke` 在它上面产出的是 `X"有"` 而不是 `X"y"z`。
33. **在 Core 报告执行已停止时，活动栏选中了一个中央视图而中央面板仍停在恢复态
    界面**（GUI）。一次工具调用失败之后，中央面板画出 `AGENT STOPPED` /
    `Core reports that execution has stopped.`，其下是 Core 自己的事实控件 ——
    `Reconnect · GUI-CORE-003`、`Inspect facts`、`Restart agent ·
    no_retryable_session`、`Close Lane`、`Restore checkpoint · GUI-CORE-003`，每一个
    没有事实支撑的都被禁用并加了标注，这是对的。不对的是：此后按下 `Plan Studio`
    或 `Diff review` 会高亮活动栏槽位，而中央面板留在恢复态界面，于是这两个注册的
    中央视图在该 Lane 的余生里都不可达。`Esc` 返回在该状态下仍然可用。
34. **重新启动之后被恢复的 Lane 没有执行所有者**（GUI 或 Core，阻塞重启）。对着
    一个已含创建过 Lane 的 `VIDEN_HOME`，通过 `⌘O` 重新绑定同一个工作区，会恢复
    Lane 行、它的分支与工作树，然后拒绝一切：`The selected Lane has no sole Core
    execution owner.`，编辑器与它的三个药丸被禁用，提交栏被禁用。
    `lane-agent-bindings.jsonl` 中的绑定带着**上一个**进程的 `session_id`，而新进程
    铸造了新的会话，于是"唯一所有者"谓词找不到任何一个。因此重启之前创建的 Lane
    从此只读。这是 E3b 编号为 26 的问题中"可操作性"的那一半；"空白窗口"的那一半
    不复现（步骤 27）。
    **已于 2026-09-14 由 Core 批次 C17 修复**（`fix(runtime): re-bind restored
    Lanes to the new session at hydration`）。它属于 Core，并且有两个彼此独立、都必须
    消除的成因：快照前缀为每条持久 Lane 重新发布 `LaneUpdated`，且刻意不发布 owner
    （`crates/runtime/src/runtime_contract.rs:2103`），于是"唯一所有者"谓词找不到任何
    一个；而 `lane-agent-bindings.jsonl` 仍指名已死的 session，因此即便手里有 owner，
    `run_supervised_input` 的预留（`crates/runtime/src/runtime_supervisor.rs:5209`）
    也会拒绝该提交。Core 现在在 hydration 时为每一条被恢复的非终态 Lane 发出指名本进程
    session 的 `LaneRuntimeOwnerBound`（`rebind_restored_lanes_to_this_session`，
    `crates/runtime/src/runtime_supervisor.rs:3748`），*并且*为它追加一条取代性的持久
    绑定行（`WorkflowStore::rebind_lane_agent_session`，
    `crates/workflows/src/stores.rs:287`）；旧行保留原地，因为该日志是只追加的。由外部
    agent 拥有的 Lane 两半都跳过，因此被恢复的 ACP 续接保留它自己的 owner。没有
    capability、计数、事件形状或 fixture 发生移动。测试：
    `crates/runtime/src/tests/lane_execution_scope_tests.rs:414`。兼容性后续项 21。

### 对缺陷 22–26 的裁定

| # | 在候选版上的裁定 |
| --- | --- |
| 22 | **关闭，原生环境确认两次。** 弹出层的 TASK 在 fallback provider 上变成一个回合：编辑器释放、`EVENTS` 推进、回合落定，会话 JSONL 在该 Lane owner 之下承载整个回合 |
| 23 | **关闭，原生环境确认两次。** 被拒绝的提交渲染一条 `role=alert`，指名占槽的命令（先是 `submit_user_input`，后是 `set_work_mode`）、它的年龄，以及带效果的等待绝不在客户端侧放弃 |
| 24 | **关闭，原生环境双向确认。** `Cancel active turn` 在空闲 Lane 上不存在，只有 Core 报告活跃回合时才出现 |
| 25 | **仍然开放；其客户端侧成因现已定位。** 在选中 Lane 的情况下提交栏从未可达，因为 operator-git owner 解析到的是与选中 Lane 不同的另一个 Lane（缺陷 30）。`git_push` 半区本身未被重新驱动 |
| 26 | **未复现。** 对着已含两个 Lane 的 home 的 `VIDEN_GUI_WORKSPACE` 启动渲染出了完整驾驶舱。同一重启问题中可操作的那一半是真实的，编号为 34 |

### 对兼容性后续项 14–16 的裁定

| # | 在候选版上的裁定 |
| --- | --- |
| 14 | **关闭，原生环境。** 归档补丁是规范的，路径标签就是文件自己的路径：`kind patch`、`source native`、`path README.md`、`owner lane lane_…`、`verification verified`、`canonical quality pass` |
| 15 | **关闭，原生环境，两条路径都是。** D14 在同一加载页上同时显示 `tool · lane_create` 与 `tool · edit_file` 的 `approval.allow_once`，各自在 `audit.jsonl` 中以其请求早已显示过的 id 持久化 |
| 16 | **在 GUI 上未达成。** 编辑器在回合中宣称 `Queue follow-up`，随后拒绝提交，被保留的文本被丢弃而不是排队或发送。参见缺陷 29 |

### 观察，而非缺陷

- **每一批按键之前先把进程置前，并加以核验。** 本次运行第一次 `⌘O` 绑定完全送到
  了另一个应用：按键是在 `Claude` 处于最前时发出的，Viden 面板从未看见它们，是
  面包屑抓住了这一点（`README.md 155 bytes` —— 上一批次的夹具 —— 而不是
  `182 bytes`）。E3b 的规则是"不要在键入与 `Return` 之间截图"；这一条更窄也更早：
  在发送任何东西之前，读取
  `name of first process whose frontmost is true` 并断言它是 `viden-gui`。
- **一次 Accessibility 遍历会关掉编辑器控件的弹出层。** 工作模式、权限与模型三个
  药丸打开的是真实 popover；在打开点击与选项点击之间的任何 `axtree` 式遍历都会
  关掉它，于是那个药丸在五种不同尝试下看起来都毫无反应。一次弹出层选择是两次真实
  点击、中间不做任何 Accessibility 读取；而证明弹出层打开过的那一帧必须是遍历之前
  拍下的窗口 id 截图。
- **按名字而不是按序号定位窗口。** 编辑器弹出层打开时，进程还拥有第二个未命名的
  `layer=20` 窗口，而 `first window of process` 选中的是它；此后每次遍历返回的都是
  弹出层自己的 161 行树，每一个控件都"消失"。本批次临时目录中的助手脚本使用
  `first window whose name is "Viden"`。
- **窗口在一次完整重渲染之后会离开 Accessibility 树。** 整个运行中出现六次
  `-1719`，每一次都紧跟在一次引发中央视图切换、审批决定或 Lane 选择的点击之后，
  每一次都在 3–10 秒后随进程存活而恢复。与 E3b 的读法相同；在本批次的证据里，
  缺陷 18 仍然不是一次自我终止。
- **固定的权限停靠面板覆盖在决策中心自己的列表之上。** 在 D2 中与 E3b 记录的完全
  一致，两份拷贝都可决断。
- **包仍然是 `0.1.0-rc.5`。** 计划为本里程碑设定的目标是 `rc.6`，而尚无批次执行该
  版本号提升。

### 这对计划目标 4 意味着什么（按界面）

`docs/release-0.3.3-plan.md` 目标 4，在 `0.3.4` 中作为目标 6 延续 —— "一个真实的
本地优先开发任务通过 GUI 完成，从接入到一次已提交的改动，并记录审计与证据"：

- **通过 TUI —— 端到端达成**，与 2026-09-12 的记录相同。
- **通过 `0.3.5` 候选版上的原生 GUI 窗口 —— agent 半区现在打通了，而 Lane 的源码
  控制半区在它之后合上了。** E3b 无法在创建出的 Lane 中启动回合；E3c 从弹出层
  自己的 TASK 字段启动了一个，并在其后到达了带 Core 类型化 hunk 的工具审批、允许
  一次、内联工具差异、归档补丁、D14 读回两条审批路径、C15 的 `Always` 及其撤销，
  以及 C16 带 `revised_from` 链接的类型化 `Edit`。这就是 `0.3.5` 信任契约新增的
  全部内容，已原生取证。
- **驾驶舱仍然做不到的，是在 Lane 内把任务做完。** agent 的编辑落在工作区根目录
  而不是 Lane 工作树（缺陷 28），提交栏又对着错误的 Lane 作出拒绝（缺陷 30），
  因此本次运行没有任何 Lane 作用域的提交或推送可达。E3b 到达过一次 Lane 作用域的
  暂存与提交；它是在这两个缺陷能够被观察到之前、在一棵带外弄脏的树上做到的，两份
  记录应当合起来读，而不应被读作一次回退主张。
- **诚实的总结：** `0.3.5` 的各个信任界面 —— Lane 路径的审计行、审批作用域、可编辑
  的修订、归档补丁、审计时间线 —— **已关闭并原生取证**。目标 4 自身的端到端主张
  在 GUI 上**仍未达成**，其原因是两个新定位、形态各异的缺陷，而不是阻塞 `0.3.4`
  以及 `0.3.5` 首次尝试的那一个。退出标准 1 就"候选版已被原生驱动"以及"Lane 的
  回合"而言已达成；就"完整任务"而言尚未达成，本文档不作任何相反的粉饰。

## 缺陷 29–33 与缺陷 34 的客户端一半 —— H6（2026-09-14，在 `claude/gui-native-defects-h6` 上）

分支 `claude/gui-native-defects-h6`，位于 `.worktrees/gui-native-defects-h6`，
基底为 `claude/int-0.3.5` 的 `b868f5c8`（合入 E3c 证据的那次提交）。本批次没有启
动任何原生窗口，也没有发送任何按键：下面每一帧都是无头抓取，**E3d 会在原生环境里
重新验证全部内容**。缺陷 28 与缺陷 34 的 Core 一半不在这里改动；它们属于 Core，由
批次 C17 负责。

每处修复都先写失败测试，一个缺陷一次提交，失败文本记录在它所属的提交里。

### 判定

| # | 判定 | 提交 | 位置 | 测试 |
| --- | --- | --- | --- | --- |
| 25 | **客户端成因已修（H6）；原生确认仍欠 E3d。** 选中 Lane 时提交栏从来无法抵达，是因为 operator-git owner 解析到了另一条 Lane（缺陷 30）。修好之后，进程内实时 Core 探针把「选中 Lane」的这一对命令端到端跑通，`git_push` 那一半抵达权限坞并以 Core 的类型化 `no_upstream` 落定 | `eacdbf68` | `apps/gui/src-tauri/src/adapter.rs`（探针） | `the_push_half_of_commit_and_push_reaches_a_decidable_surface_with_a_lane_selected` |
| 29 | **已修。** Core 报告回合时的提交现在携带 `RuntimeCommand::QueueFollowUp`，在自己的 `pending_queue` 里等待，而不是排在占着 D1 槽位、尚未被应答的提交后面，并由 Core 的 `InputQueued` 确认。`Queue follow-up` 标签读的是路由读的同一个判据，因此只在排队真能发出时才显示。被自己已占用的排队槽拒绝的后续输入会渲染缺陷 23 的告警并**保留文本**：拒绝现在会清掉已提交草稿，正是它让一条无关的确认把输入区在文本下面清空了。排队行未变，仍是 Core 自己的 `InputQueued` 经 `liveWork.queuedInputs` 而来。**兼容性后续项 16 在 GUI 上已满足** | `87d20d96` | `apps/gui/src-tauri/src/adapter.rs`（`queue_follow_up_and_wait`、`observe_pending_queue`、`pending_queue`）、`apps/gui/src-tauri/src/lib.rs`（`d1_queue_follow_up`）、`apps/gui/src/screens/d1_cockpit.ts`（`queueComposerFollowUp`、`queueFollowUpAvailable`、`refuseOccupiedCommandSlot`） | `a_follow_up_queues_while_the_submit_that_started_the_turn_is_still_unanswered`；`tests/h6_native_defects.spec.ts` 的「defect 29」 |
| 30 | **已修。** 宿主本来就是对的——`operator_git_owner` 按 Lane 从 Core 发布的绑定解析。Webview 不是：`selectLane` 会重读 owner，但选中项还会在 Core 自己的 `selectedLaneId` 随投影到达时、在创建流程恢复进入其 Lane 时、以及在重试 Agent 会话时移动，而每一处都直接给字段赋值。现在所有赋值都走同一个 `setSelectedLane` 漏斗，读取会记下它问的是哪条 Lane，被更新选中项取代的乱序答案会被丢弃，渲染也根本不再展示限定在另一条 Lane 上的答案 | `eacdbf68` | `apps/gui/src/screens/d1_cockpit.ts`（`setSelectedLane`、`readOperatorGit`、`operatorGitForSelection`、`operatorGitScope`） | `the_operator_git_owner_is_resolved_against_the_selected_lane_alone`；`tests/h6_native_defects.spec.ts` 的「defect 30」 |
| 31 | **已修。** 宿主本来就用撤销之后 Core 持有的 `standing_approvals` 作答；外壳只留下结论、把列表丢了，而又没有别的东西刷新驾驶舱投影——撤销自己的排空消费掉了 `StandingApprovalRevoked`，安静的 Core 不会产生后续唤醒。现在整个回答都会到达设置面板，并一直渲染到更新的投影取代它为止。这是镜像 Core，而不是客户端删行：被拒绝的撤销回来时规则仍在，行也就留着 | `76f0f0b5` | `apps/gui/src/main.ts`（`revokeStandingApproval`）、`apps/gui/src/screens/d1_cockpit.ts`（`standingAfterRevoke`） | `the_revoke_answers_with_the_standing_list_core_holds_afterwards`；`tests/h6_native_defects.spec.ts` 的「defect 31」 |
| 32 | **属性已设置；按键确认仍欠 E3d。** `verbatimField` 关掉 `autocorrect`、`autocapitalize`、`spellcheck` 与 `autocomplete`，客户端里每一个文本字段都经由它创建——输入区、Lane 任务、提交信息、审批 Edit 字段、计划拒绝理由、D2 评审备注、D4 的 Lane id 与分支、D11 的路径与配置草稿、D12 的理由、D16 的字段，以及命令面板与证据的搜索框。`spellcheck="false"` 是起作用的那一个，因为 WebKit 的替换流程骑在文本检查流水线上。E3c 测到的是无障碍取值通道，而 WebKit 把它和真实按键送进同一条编辑流水线；若替换后的引号仍然挺过这些属性，剩下的设置就是 webview 的，属于外壳而不属于页面 | `e7fac8d1` | `apps/gui/src/ui/verbatim_field.ts`，以及它被应用的十四处调用点 | `tests/h6_native_defects.spec.ts` 的「defect 32」，其中包含一条走查客户端源码的守卫，后来新增的字段在变成逐字之前都会失败 |
| 33 | **已修。** 恢复界面是**对话**槽位展示的东西，不是盖在驾驶舱上的模态。它压过了每一个已注册的中间视图，于是在 Core 报告执行已停止时，`Plan Studio` 与 `Diff review` 只会点亮导航槽而什么都不改。现在它被限定在 `centerView === "transcript"`：依然可达，其基于 Core 的事实控件与标签不变 | `94f86dfb` | `apps/gui/src/screens/d1_cockpit.ts`（`recoveryActive` / `showRecovery`） | `tests/h6_native_defects.spec.ts` 的「defect 33」 |
| 34（客户端一半） | **客户端推导已修；该缺陷在 Core 一侧仍未关闭。** 在 Lane 没有精确 owner 时，拒绝用 Core 自己的措辞说明缺什么，这一点保留。`exact_lane_owner` 要求恰好一条绑定，否则拒绝，于是同时带着恢复 Lane 的前一进程绑定与重启新铸绑定的视图根本解析不出 owner；现在它读该 Lane 的**最新**绑定，并且仍然拒绝 owner 指向另一条 Lane 的绑定。仅此并不关闭 34：Core 自己的视图归约器保留一条 Lane 的**第一条**有效绑定并丢弃后来的（`RuntimeViewState::apply_event`，`crates/types/src/runtime.rs`），因此客户端读到的视图仍可能只带着那条过期绑定。重启后的 Lane 要有 owner，重新绑定必须是视图最终持有的那一条——**这是 Core 的事实，由 C17 负责** | `83091072` | `apps/gui/src-tauri/src/adapter.rs`（`exact_lane_owner`） | `a_lane_with_two_published_bindings_resolves_its_latest_execution_owner`、`a_binding_whose_owner_names_another_lane_is_still_refused` |

### 证据

1440x1180 的无头抓取，清单与说明在
[`apps/gui/evidence/main-window-interactions/EVIDENCE.zh-CN.md`](../../../apps/gui/evidence/main-window-interactions/EVIDENCE.zh-CN.md)
的「H6 截图」一节：深色/en 与浅色/zh-CN 的排队输入区、同一份投影下对话槽位里的
恢复界面与中间区里的 Plan Studio，以及撤销得到应答之后的设置面板。缺陷 30 改的
是选中漏斗，缺陷 32 改的是一组输入属性；两者都不改变任何单一状态下画出来的东
西，因此都没有帧，两者都由上表点名的测试覆盖。

### 仍然欠着什么

- **E3d，在原生环境里：** 选中 Lane 的 `Commit & Push` 这一对、在活动回合中排队
  并被排空的后续输入、用真实按键确认键入的 `"` 仍是 `U+0022`、在已停止的 Lane 上
  打开中间视图，以及撤销之后的设置列表。
- **C17，在 Core 上：** 缺陷 28 的工具工作目录，以及缺陷 34 里决定重启后的 Lane
  视图携带哪条绑定的那一半。

## 原生 GUI 运行 —— 2026-09-14（`0.3.5` 候选版 `71d5156a`，E3d）

这是 E3d：在 C17 与 H6 之后，于 `0.3.5` 候选版上通过原生驾驶舱驱动剩余的 Lane
步骤。分支 `claude/evidence-e3d`，位于 `.worktrees/evidence-e3d`，基线为集成分支
`claude/int-0.3.5` 的 `71d5156ac489a83c041a7e82b98e336932a0a276` —— 即 E3c 的证据
加上 C17（Lane 拥有的回合中的工具在 Lane 工作树里运行；恢复的 Lane 在水合时重新
绑定）与 H6（operator-git 属主跟随选中项、回合期间排队、常驻规则列表、逐字输入
字段、在恢复界面之上打开中间视图）。

主机屏幕处于解锁状态，并在**每一批**按键或点击之前读取
`CGSessionCopyCurrentDictionary()`；从 `2026-09-14T05:35:58Z` 到
`2026-09-14T07:07:31Z`，每一次的 `CGSSessionScreenIsLocked` 都是 `0`。

**本批次的头条：智能体现在能在 Lane 内部真正干活，而 Lane 的源码控制这一半被挡在
比以前更早的一步上。** C17 的两处修复在原生环境下完整成立 —— Lane 拥有的回合中
调用的工具写入的是 **Lane 工作树**，工作区根目录毫发无损（缺陷 28）；跨重启恢复的
Lane 拥有执行属主并能跑起一个回合（缺陷 34）。H6 的选中项漏斗同样成立：选中一个
Lane 时，提交栏是**可达的**，不再携带指向另一个 Lane 的拒绝语，并且只指名被选中的
那个 Lane（缺陷 30）。这次运行没能完成的是提交与推送本身，因为 `Stage all` 从未
抵达任何可决断的界面：客户端停在
`Stage is waiting for approval in the permission dock.`，而权限坞里什么都没有，
`audit.jsonl` 中从未写入任何 `git_*` 行，状态栏读作 `REQ 0 req / 0 err`（缺陷
35）。因此裸 `origin` 被创建但从未使用，**缺陷 25 仍然开着**。

本批次没有采取任何阻断性修复。就现有证据而言，缺陷 35 无法仅从客户端一侧判定归属
（见其条目），而对一个本次运行无法定位的界面做投机性修复，会比这条发现本身更糟。

### 被测应用

`npm ci` 添加了 97 个包，并在 4 秒内审计了 98 个包。
`npx tauri build --bundles app` 通过 ——
`Finished \`release\` profile [optimized] target(s) in 1m 08s` —— 并产出
`target/release/bundle/macos/Viden.app`：

| 事实 | 取值 |
| --- | --- |
| `CFBundleShortVersionString` | `0.1.0-rc.5` |
| `CFBundleIdentifier` | `dev.viden.gui` |
| `CFBundleExecutable` | `viden-gui` |
| 架构 | `Mach-O 64-bit executable arm64` |
| 可执行文件大小 | 42,891,264 字节 |
| 包体大小 | 42 MB |
| 签名 | ad-hoc、linker-signed、`TeamIdentifier=not set`、`Sealed Resources=none` |

包体仍带着 `0.1.0-rc.5`；计划的目标是 `0.1.0-rc.6`，尚无批次执行该提升，归 R1
所有。应用是从包体直接启动的，从未安装、从未用身份签名、从未公证、从未分发。每次
启动都把 `VIDEN_HOME` 指向暂存目录，因此本仓库自身的 `.viden/` 从未被读写。按顺序
启动的 PID：39630、40297、49594、58246。四个都由本批次结束；没有一个自行退出。

### 夹具

运行暂存目录下一个全新的临时 Git 仓库，含一个提交 `2b1b3e3`（"Add the fixture
README"）、一个五行 125 字节的 `README.md`、一个覆盖 `.viden/` 与 `.worktrees/`
的 `.gitignore`，以及一个选择 `provider = "fallback"` / `model = "test-local"`
的 `.viden/config.toml`。Core 在引导时于 `ws_8f05f137b4ef25a8` 中铸出
`prj_1789364819298232000`。暂存目录里创建了一个裸 `origin`，并且**从未使用**，因为
没有任何推送入口变得可达（缺陷 35）。创建了两个 Lane：
`lane_1789365083535959000` 与 `lane_1789366692288148000`。

### 与脚本任务的两处偏离，以及原因

**绑定是在启动时通过 `VIDEN_GUI_WORKSPACE` 完成的，而不是通过 `⌘O`。** `⌘O` 面板
确实打开了，并被驱动到它的 `⌘⇧G`"前往"子表单：路径字段通过辅助功能设值器逐字设置，
屏幕上也把路径解析成了 `… › e3d › fixture` —— 但无论在主机的拼音输入源还是 ABC 上，
`Return` 与点击已解析行都无法提交它。发给该进程的按键能抵达主窗口（`⌘O` 与 `⌘L`
都奏效），却**抵达不了**面板的子窗口，而该面板全程渲染为非活动态。E3c 的面包屑规则
在造成任何损害之前抓住了后果：面板恢复的是 **E3c** 的夹具（`README.md 186 bytes`），
若继续下去绑定的会是它，而不是本次运行的（`125 bytes`）。启动时绑定与 E3c 第 27 步
驱动的是同一条产品路径，因此被测驾驶舱是按 Core 自己的方式绑定的；不同的只是操作者
手势，这里如实记录而非一笔带过。

**源码控制这一半从未抵达，因此提交之下游的一切都没有被驱动。** 这不是走捷径：这就是
缺陷 35，作为阻断项记录在案。

### 逐步记录

每一帧都按窗口 id 拍摄两次并保留第二次，且列出的每个 PNG 在列出之前都已被查看。帧
位于 `native-0.3.5/` 下，前缀为 `e3d-`。

| # | 步骤 | 是否抵达 | 结果 / 原因 | 帧 |
| --- | --- | --- | --- | --- |
| 1 | 欢迎页 | 是 | `Welcome to Viden`、`Your local-first agent workspace`、`Open project ⌘ O`、`No project open`、`Core adapter is not connected`；`MODE — · PERM — · CONTEXT — · EVENTS #0 · LANE — · LATENCY — · TOKENS — · DIAG 0× · REQ —` | [`01`](native-0.3.5/e3d-01-welcome.png) |
| 2 | **已绑定的驾驶舱** | 是 | 上下文坞 `Provider fallback`、`Model test-local`、`Mode build`、`Permission ask`、`Tokens 0`；`Lines +0 −0` / `No changes`；`Showing Workspace root`、`Branch main`、夹具路径作为 `Worktree`、`Ahead 0`、`Behind 0`、`Working tree Clean`；`MODE build · PERM ask · EVENTS #7 · LANE — · DIAG 0× · REQ 0 req / 0 err`。绑定路径即上文记录的偏离 | [`02`](native-0.3.5/e3d-02-cockpit-bound.png) |
| 3 | **`⌘L` 新建 Lane、内置智能体、TASK** | 是 | 选中 `Viden Agent built-in`；Codex、Claude Agent 与 Kiro CLI 禁用，先是在 `Checking ACP agents… each probe is given up to 15s.` 之下，随后是在 `Probe did not answer within 15s.` 之下，旁边配有 `Retry ACP check` —— 探测有界限并且自己说明了这一点。填入 TASK 后，`Create lane ↵` 读作 `enabled=true`，分支变为 `⌁ vd/edit-the-fixture-readme-he`。**缺陷 14 与 15，在没有 ACP 二进制的主机上，再次于原生环境成立** | [`03`](native-0.3.5/e3d-03-new-lane-popover.png)、[`04`](native-0.3.5/e3d-04-new-lane-task-set.png) |
| 4 | **一个带 C15 完整档位列表的 Core `lane_create` 审批** | 是 | `Approve lane_create · lane_create`、`Medium risk`、`branch: viden/lane_1789365083535959000 path: …/.worktrees/lane_…`、`Reason: lane mutation policy requires approval`、`Default: Deny`、`Audit: audit_1789365099211250000`，其上是 `Y · Once` / `Allow repo paths` / **`Shift+A · Always · scope_not_offered`** / **`E · Edit · not_editable`** / `N · Deny`。两个被禁用的档位都携带 **Core 自己的拒绝措辞** —— **登记项 019，在 GUI 侧再次实时闭合** | [`05`](native-0.3.5/e3d-05-lane-create-approval.png) |
| 5 | **Lane，以及弹出框 TASK 变成一个回合 —— 缺陷 22** | 是 | `Y · Once` 放行。标签条 `lane_1789365083535959000 coder starter…`，标题栏 `1 worktree`，上下文坞 `Showing Lane worktree`，`LANE OUTPUT` 携带 Core 自己的 `HEAD is now at 2b1b3e3 Add the fixture README` / `Preparing worktree (new branch 'viden/lane_…')`。带外核验：工作树存在，`git branch` 列出 `viden/lane_1789365083535959000`。输入框释放且**没有 `Cancel active turn`**，`EVENTS #46`，`REQ 1 req / 0 err`。在暂存会话 JSONL 中逐字节核验：`turn_owner` 带 `lane_id: lane_1789365083535959000` 与 `turn_id: turn_1789365157674412000`、用户消息 `Edit the fixture README heading in the Lane`、一条 `outcome: success` 的成本行（`fallback` / `test-local`）、助手回复，随后 `turn_owner` 被清空。**H5 的修复在原生环境下依然成立** | [`06`](native-0.3.5/e3d-06-lane-created-turn.png) |
| 6 | **输入框 `tool edit_file path=README.md old=Fixture new=Edited`** | 是 | Core 抬起 `Approve edit_file · edit_file`、`Medium risk`、`TOOL INPUT PREVIEW new: Edited old: Fixture path: README.md`、**Core 的定型 C1 差异块** `@@ -1,5 +1,5 @@` 含 `-# E3d Fixture`，其上是 `Y · Once` / `Allow repo paths` / `Shift+A · Always`（**启用**）/ `E · Edit · not_editable` / `N · Deny`。`Core is working; no owner-scoped status is available. typed`、`Cancel turn · Esc`，输入框标签切换为 `Queue follow-up`。上下文坞此时已切到 `Showing Lane worktree`，带着该 Lane 的分支与工作树 | [`07`](native-0.3.5/e3d-07-edit-file-approval.png) |
| 7 | **缺陷 28 —— 工具在 LANE 工作树中运行** | 是，**已闭合** | `Y · Once`。Core 自己的工具结果：`edit_file completed` / `path: …/scratchpad/e3d/fixture/.worktrees/lane_1789365083535959000/README.md` / `size: 124 B` / `effect: edited 5 lines`。磁盘上带外核验：**工作区根目录**干净，仍是 125 字节的 `# E3d Fixture`；**Lane 工作树**为 ` M README.md`、`# E3d Edited`、124 字节、`@@ -1,4 +1,4 @@` `-# E3d Fixture` / `+# E3d Edited`。而且产品自己就这么说：同一个 Lane 中的 `tool shell command=pwd` 打印出 `…/fixture/.worktrees/lane_1789365083535959000` —— 即 Lane 工作树，而 E3c 打印的是工作区根目录。**C17 的修复在原生环境下双向成立** | [`09`](native-0.3.5/e3d-09-lane-worktree-tool-root.png) |
| 8 | **后续项 16 / 缺陷 29 —— 回合期间排队一条后续输入** | **否**，且比 E3c 更糟 | 在 Core 报告回合、标签读作 `Queue follow-up` 的情况下提交了 `Summarise what you just changed`。它**没有**被拒绝 —— E3c 那条诚实的 `Not sent…` 提示根本没有出现 —— 输入框被清空，没有画出任何排队行，`EVENTS` 也没有前进（提交前后都是 `#49`）。随后回合正常跑完，而这段文字**从未抵达 Core**：会话 JSONL 中不存在这样的消息，该字符串在 `VIDEN_HOME` 下任何位置都不出现。驱动了两次，分属两个回合。见缺陷 36 | [`08`](native-0.3.5/e3d-08-followup-queued.png) |
| 9 | **缺陷 33 —— 在已停止的 Lane 上打开中间视图** | 是，**已闭合** | 在中间区绘制 `AGENT STOPPED` / `Core reports that execution has stopped.` 并覆盖 Core 的事实控件（`Reconnect · GUI-CORE-003`、`Inspect facts`、`Restart agent · no_retryable_session`、`Close Lane`、`Restore checkpoint · GUI-CORE-003`，每个无支撑的都被禁用并标注）的情况下，`Diff review` **切换了中间区**：恢复界面完全离开辅助功能树，DiffReview 在其位置渲染。回到 `Conversation` 又把恢复界面带回来，这正是 H6 的作用域划分 —— 它在自己该待的槽位里依然可达 | [`10`](native-0.3.5/e3d-10-defect33-diffreview-over-stopped.png) |
| 10 | **在 Lane 的脏树上做 DiffReview，而且是被产品弄脏的** | 是 | `CHANGES 1 files · +1 −1`、`M README.md +1 −1`、`README.md`、`@@ -1,4 +1,4 @@`、`-# E3d Fixture` / `+# E3d Edited` 带逐行编号；上下文坞 `Lines +1 −1`、`M README.md +1 −1`、`Showing Lane worktree`、被选中 Lane 的分支与工作树、`Working tree Dirty`。总数、文件列表与 `Local` 标记彼此一致 —— **缺陷 20，在原生环境下成立**。与 E3a、E3b、E3c 不同，**这处改动是产品自己做的**，由 `edit_file` 在其审批之下完成 | [`10`](native-0.3.5/e3d-10-defect33-diffreview-over-stopped.png) |
| 11 | **第二个 Lane，以及 D2 决断它的创建** | 是 | 新建 Lane 弹出框来自标签条的 `+ New Lane`；在非转录中间视图下 `⌘L` 打不开它。它的 `lane_create` 审批落在决策中心而非转录区，在那里 `Approve once` 放行了它：`lane_1789366692288148000`、第二个工作树、标题栏 `2 worktrees`。**缺陷 16 的前一半，再次于原生环境成立** | [`11`](native-0.3.5/e3d-11-two-lanes-lane2-selected.png) |
| 12 | **缺陷 30 —— operator-git 属主跟随选中项** | 是，**已闭合** | 重启后选中 `lane_1789365083535959000` 时，`Open the commit bar` 是**启用**的，提交栏渲染出来（`Commit message` 启用、`Stage all` 启用、`Commit` 与 `Commit & Push ⇡` 在暂存之前禁用），**整个视图里任何位置都没有出现 `D1-OPERATOR-GIT-OWNER` 拒绝语**，且全视图指名的唯一 Lane 就是 `viden/lane_1789365083535959000` —— 被选中的那个。E3c 当时 `Open the commit bar` 是禁用的，携带的拒绝语指名的是*另一个* Lane | [`13`](native-0.3.5/e3d-13-commit-bar-lane-selected.png) |
| 13 | 暂存、提交、`Commit & Push ⇡`、`git_push` 那一半、`NoUpstream`、设置上游、推送被接受 | **否** | `Stage all` 是启用的，按下也很干净，然后什么都没发生。中间区渲染 `Stage is waiting for approval in the permission dock.`；权限坞里是 `0 awaiting you` / `Core is holding no decision for you.`；在 60 秒的持续观察中 Lane 工作树一直是 ` M README.md`（未暂存）；`audit.jsonl` 中从未追加任何 `git_*` 行；状态栏读作 `DIAG 0× · REQ 0 req / 0 err` —— 连一个在途请求都没有。在全新进程与全新 Core 连接上复现，且解析出的 Lane 正确。因此 `Commit` 与 `Commit & Push ⇡` 从未启用，没有任何推送入口可达，裸 `origin` 从未被使用。见缺陷 35 | [`15`](native-0.3.5/e3d-15-defect35-stage-never-decidable.png) |
| 14 | **缺陷 34 —— 跨重启恢复的 Lane** | 是，**已闭合** | 退出并针对同一工作区与 `VIDEN_HOME` 重启。`lane-agent-bindings.jsonl` 接收了 C17 的接替行 —— **两个** Lane 各一条 `lane-agent-rebound:lane_…:session_1789367410385273000`，仅追加的旧行原样保留。驾驶舱中恢复的 Lane 被选中，拒绝语 `The selected Lane has no sole Core execution owner.` **不存在**，输入框启用，并且一次提交**跑起了一个回合**：`turn_owner` 带 `lane_id: lane_1789365083535959000` 与 `session_id: session_1789367410385273000` —— 新会话 —— 且用户消息进入新会话的 JSONL。它的提交栏是启用的（第 12 步）。转录区还跨重启读回了持久审计行：`lane-approval_1789365099211245000 · decision allow_once`、`approval_1789365849889072000 · decision allow_once`、`approval_1789366009927556000 · decision allow_once`，每条都带 `Open audit trail` | [`12`](native-0.3.5/e3d-12-relaunch-restored-lane.png)、[`17`](native-0.3.5/e3d-17-esc-return.png) |
| 15 | **缺陷 32 —— 用真实按键键入的 `"`** | 是，**已闭合** | 仅为这一段把输入源切到 `com.apple.keylayout.ABC`，之后恢复（记录见下）。`A"b"c` 作为五个真实 HID 按键事件投递到 `cghidEventTap` —— 与键盘进入的是同一个事件 tap —— 并逐字节读回：`41 22 62 22 63`，与期望字符串完全相同。**两个引号都是 `U+0022`。** E3c 的 `A”b"c`（`U+201D`）**不复现**：H6 的 `verbatimField` 属性在真实按键下成立，因此 E3c 保留判断的那项剩余设置并不需要 | [`14`](native-0.3.5/e3d-14-defect32-verbatim-quotes.png) |
| 16 | Plan Studio：D15 中的 `update_plan` 提案、接受、计划模式下拒绝移交、切到构建模式、移交、D13 的 DAG | **否** | 未驱动。缺陷 32 对键入 JSON 的阻断已经消失（第 15 步），因此 E3c 无法录入载荷的那个原因已被移除 —— 但本次运行的会话被源码控制这一半与缺陷 37 消耗殆尽，这一段没有开始。仍然欠着，此处也不对其作任何方向的结论 | — |
| 17 | 缺陷 31：`Always`、被自动放行的重复、撤销、以及该行离开设置列表 | **否** | 未驱动，原因同第 16 步。H6 的修复在原生环境下未经检验。仍然欠着 | — |
| 18 | Agent Board 控件 | **否**，且不是缺陷 | 内置 `Viden Agent` 跑的是供应商回合，不是 `runtime.agent_session_control` 的 Agent 会话，而本主机没有安装任何 ACP 适配器 —— 新建 Lane 弹出框中三个 ACP 智能体都读作 `Probe did not answer within 15s.`（第 3 步）。不可能存在可暂停、终止或重试的 Agent 会话行。只说一次，不作伪 | [`03`](native-0.3.5/e3d-03-new-lane-popover.png) |
| 19 | **专注模式 `⌘.` 与 `Esc` 返回** | 是 | 该控件隐藏了上下文坞，把整个窗口宽度交给中间区，同时保留标题栏、导轨、输入框与状态栏。`Esc` 把导轨退回 `Conversation`，保留其余外框且专注模式仍然开着，这与 `0.3.4` 和 E3c 的记录一致。`⌘.` 本身没有触发；标题栏控件触发了。见下方工装说明 | [`16`](native-0.3.5/e3d-16-focus-mode.png)、[`17`](native-0.3.5/e3d-17-esc-return.png) |

### E3d 发现的缺陷与缺口

编号接上文三十四条继续。每一条都是复现出来的，而非推断出来的。本批次没有修复任何
一条。

35. **Lane 作用域的 `Stage all` 从未抵达可决断界面，提交栏在等待一个并不存在的
    审批**（对整个源码控制这一半构成阻断；就本证据而言无法判定归属哪一侧）。在选中
    `lane_1789365083535959000`、其工作树已被产品自己的 `edit_file` 弄脏、且提交栏
    可达并作用域正确（缺陷 30 已闭合）的情况下，`Stage all` 按下很干净，然后什么都
    没发生。中间区渲染 Core 自己的
    `Stage is waiting for approval in the permission dock.`；决策中心渲染
    `0 awaiting you` 与 `Core is holding no decision for you.`；Lane 工作树在
    60 秒内一直是 ` M README.md`；**`audit.jsonl` 中从未追加任何 `git_*` 行**
    （整次运行仅有的四行是两个 `lane_create` 审批、一个 `edit_file` 和一个
    `shell`）；状态栏读作 `REQ 0 req / 0 err` —— 根本没有在途请求。在全新进程、
    全新 Core 连接上复现。`Commit` 与 `Commit & Push ⇡` 从未启用，因此**没有任何
    推送入口变得可达，裸 `origin` 从未被使用 —— 缺陷 25 仍然开着**，只是如今挡在
    一堵不同且更早的墙后面。
    有两个站点与此有关，本次运行无法在它们之间做出选择。客户端一侧，
    `run_operator_git_action_and_wait`
    （`apps/gui/src-tauri/src/adapter.rs:3146`）发出命令后通过自己的
    `poll_operator_git`（`:3209`）排空 250 毫秒；此后提交栏"在驾驶舱的有序 Core
    唤醒上轮询它，而不是自己持一个定时器"（`operator_git_poll`,
    `apps/gui/src-tauri/src/lib.rs:747-757`），因此在安静的 Core 上不会再有后续
    唤醒到来 —— 这与 H6 为缺陷 31 修复的失败形状*完全一致*，那里是"撤销自己的排空
    消费掉了 `StandingApprovalRevoked`，而安静的 Core 不会产生更晚的唤醒"。Core
    一侧，`run_supervised_project_mutation`
    （`crates/runtime/src/runtime_supervisor.rs:3012`）是
    `RuntimeCommand::RunOperatorGitAction` 走的路径
    （`crates/runtime/src/runtime_supervisor.rs:2768`），其第一道闸门会在该属主已
    持有活动作业时发出 `CommandRejected`。该分支*作为一个静默分支*已被排除，因为
    `PendingOperatorGit::observe`
    （`apps/gui/src-tauri/src/adapter.rs:1526`）会按命令 id 匹配 `CommandRejected`
    并渲染出拒绝语。没有被排除的是：Core 接受了命令却没有发布审批，而客户端自己的
    发送排空消费掉了请求事件。**已编号并就此打住；没有编辑任何 `crates/**` 文件，
    也没有尝试任何 GUI 修复，因为本证据无法把故障定位到客户端。**
36. **回合期间提交的输入框后续消息被静默丢弃**（GUI，对后续项 16 构成阻断，并且在
    诚实性上相对 E3c 是倒退）。H6 通过把繁忙输入框的提交路由到
    `RuntimeCommand::QueueFollowUp` 闭合了缺陷 29，并承诺它会"由 Core 自己的
    `InputQueued` 确认"。在原生环境下，这些都观察不到，而且文字丢失了。在 Core
    报告回合、标签读作 `Queue follow-up` 的情况下，提交被接受 —— **没有渲染任何
    拒绝语**，因此 E3c 那条诚实的 `Not sent. … Your message is kept.` 提示消失了
    —— 输入框被清空，**没有画出排队行**，`EVENTS` 也没有前进（提交前 `#49`，提交后
    `#49`）。随后回合正常完成，而这条后续消息**从未抵达 Core**：会话 JSONL 中不存在
    带该内容的消息，字符串 `Summarise what you just changed` 在 `VIDEN_HOME` 下
    任何位置都不出现。驱动了两次，分属两个独立回合。E3c 至少还告诉操作者消息被保留
    了；这个候选版什么都不告诉他们，并且把它丢掉。**因此兼容性后续项 16 在 GUI 上
    仍未满足**，而且失败方式从响亮变成了静默。
37. **Core 连接在会话中途断开且再不恢复**（GUI 或适配器的传输层；发生时构成阻断）。
    在同一进程中驱动约四十分钟后，驾驶舱回落到 `◌ CONNECTING` /
    `Establishing the versioned Core connection.` / `Core connection pending`，
    伴随 `DIAG 1×`、一个读作 `No Lanes yet` 的 `LANES` 面板，以及无响应的输入框。
    进程仍然存活，窗口也仍在辅助功能树中；该状态在 60 秒内的六次检查中都没有清除，
    只有终止并重启才恢复。在 `RUST_LOG=info` 下该进程的标准输出/标准错误没有写入
    任何内容 —— 整个会话的日志文件是零字节 —— 因此本批次无法说明传输层做了什么。
    按观察如实记录，并附上 Core 自己状态栏显示的诊断计数。
38. **在另一个 Lane 的智能体已停止时创建的 Lane 得不到执行属主，且选中项无法从它
    移开**（GUI 或 Core；观察到一次）。`lane_1789366692288148000` 刚创建之后，状态
    栏、标题栏与上下文坞都指名它，输入框携带
    `The selected Lane has no sole Core execution owner.`，恢复界面的控件读作
    `Close Lane · no_exact_lane`，上下文坞读作
    `Waiting for the selected Lane context.`。点击**另一个** Lane 的侧栏行并没有
    移动选中项：在两次尝试与一次稳定后的重读中，状态栏、标题栏与上下文坞都停留在新
    Lane 上。在创建过程中的一小段时间里，上下文坞还显示着**第一个** Lane 的分支、
    工作树与 `+1 −1`，而其他每个界面都指名第二个 —— 这是一种自行解决的陈旧作用域，
    因此记为陈旧而非缺陷 30 的第二个实例。第 14 步的重启把这一切都清除了：C17 的
    水合给**两个** Lane 都赋予了属主。仅观察到一次，且发生在后来丢失 Core 连接的
    那个进程里（缺陷 37），两者可能同源。

### 对缺陷 25–34 的裁定

| # | 对该候选版的裁定 |
| --- | --- |
| 25 | **仍然开着，而且如今被挡得更早。** 它在客户端一侧的两个成因都已消失 —— 选中一个 Lane 时提交栏可达且作用域限定于该 Lane（缺陷 30，已闭合）—— 但 `Stage all` 从未抵达可决断界面（缺陷 35），因此 `Commit & Push ⇡` 从未启用、`git_push` 那一半从未发出、没有看到 `NoUpstream` 拒绝、裸 `origin` 也从未被使用 |
| 26 | **再次未复现。** 三次针对持有 Lane 的 home 的启动时绑定，每次都渲染出完整驾驶舱 |
| 27 | **已闭合，原生环境确认。** 输入框控件弹出框能打开并结算；本次运行中从未出现因 `set_work_mode` 槽位被占而被拒绝的提交 |
| 28 | **已闭合，原生环境双向确认。** C17 的修复成立：工具结果指名 Lane 工作树、Lane 工作树携带该改动、工作区根目录毫发无损，且产品自己在 Lane 中的 `pwd` 打印的就是该工作树 |
| 29 | **未闭合；形状变得更糟。** 见缺陷 36 |
| 30 | **已闭合，原生环境确认。** 选中一个 Lane 时提交栏可达、不携带 `D1-OPERATOR-GIT-OWNER` 拒绝语，且只指名被选中的 Lane |
| 31 | **未重新驱动。** 本次运行没有抵达 `Always` / 撤销这一段。H6 的修复在原生环境下仍未确认 |
| 32 | **已闭合，用真实按键在原生环境确认。** 在 ABC 输入源上以 HID 按键事件键入的 `A"b"c` 读回 `41 22 62 22 63`；两个引号都是 `U+0022` |
| 33 | **已闭合，原生环境确认。** 在已停止的 Lane 上按下 `Diff review` 会切换中间区，而恢复界面在对话槽位中依然可达 |
| 34 | **已闭合，原生环境确认。** C17 的水合为每个恢复的非终态 Lane 追加一条接替绑定；恢复的 Lane 拥有属主、接受提交、在新会话下跑起回合，且其提交栏是启用的 |

### 对兼容性后续项 16 的裁定

| # | 对该候选版的裁定 |
| --- | --- |
| 16 | **在 GUI 上仍未满足，而且如今是静默失败。** 输入框在回合期间宣称 `Queue follow-up`，接受提交却不给拒绝语，清空草稿，不发布 `InputQueued` 也不推进 `EVENTS`，而文字从未抵达 Core。见缺陷 36 |

### 观察，而非缺陷

- **按键能抵达主窗口，却抵达不了它的子面板。** `⌘O` 与 `⌘L` 对 Viden 窗口都奏效，
  而它们抬起的 `NSOpenPanel` 根本收不到任何按键事件 —— 既不是 `⌘⇧G`（除非先用点击
  进入面板把它打开），也不是在已解析的"前往"行上按 `Return`，两种输入源下都如此。
  点击能抵达它；该面板全程渲染为非活动态。绑定改在启动时完成，而 E3c 的面包屑规则
  在任何东西被绑定之前就抓到了：面板恢复的是 E3c 的夹具。
- **窗口离开辅助功能树的频率远高于 E3b 或 E3c 的记录。** 本次运行中约十余次
  `-1719` 应答，每次都紧跟在引发中间视图切换、审批决断或 Lane 选择的点击之后，每次
  都在进程存活的情况下于 3–15 秒内恢复。它让本次运行付出了一个审批的代价：第一个
  `edit_file` 请求在按下无法送达期间**按它自己的 `Expires: 1789365601` 过期**，
  Core 写下了 `decision: "deny"`、`reason: "RequiresApproval"`、
  `message: "User denied the permission request"`。此后本批次的每一次决断都走一个
  等待窗口重新进入树的重试循环。性质上与 E3b 的读数无异；就本批次的证据而言，缺陷
  18 仍然不是一次自我终止。
- **审批有五分钟寿命，工装必须尊重它。** 审批卡片打印的 `Expires` 字段是真实且被
  强制执行的。任何把审批窗口花在别处的步骤，都会因超时拒绝而失去该审批。
- **某些快捷键受上下文约束。** `⌘L` 从转录区能打开新建 Lane 弹出框，从 DiffReview
  则毫无反应；在 `+ New Lane` 弹出按钮刚被按过之后 `⌘.` 完全不触发，而标题栏自己的
  专注模式控件立刻奏效。未作调查，也不主张其为缺陷。
- **按窗口 id 的截图可能拍到过渡态。** 本次运行中有两帧是在中间视图切换尚在绘制时
  拍下的，显示的是将要离开的视图；片刻之后读取的辅助功能树显示的是进来的那个，该帧
  随即重拍。截图与树不一致，是截图的过错而非产品的，且树才是记录在案的状态。
- **包体仍是 `0.1.0-rc.5`。** 计划在本里程碑的目标是 `rc.6`，尚无批次执行该提升。

### 这对计划目标 4 意味着什么（按界面）

`docs/release-0.3.3-plan.md` 的目标 4，在 `0.3.4` 中作为目标 6 延续 —— "一个真实
的本地优先开发任务通过 GUI 完成，从接入到一次已提交的改动，并记录审计与证据"：

- **通过 TUI —— 端到端满足**，与 2026-09-12 的记录一致。
- **通过 `0.3.5` 候选版的原生 GUI 窗口 —— 智能体这一半在 Lane 内部已经完整。**
  E3c 打开了 Lane 的回合；E3d 闭合了会让那个回合的工作落错地方、或在重启后消失的
  两件事。Lane 拥有的回合中调用的工具如今编辑的是 Lane 工作树并且不碰工作区根目录，
  产品自己的 `pwd` 就这么说，而跨重启恢复的 Lane 重新拥有自己并能再跑一个回合。有
  了这些，驾驶舱就能在 Lane 里做真实工作并把它留住。
- **驾驶舱仍然做不到的是提交这份工作。** 提交栏第一次变得可达且作用域正确（缺陷
  30 已闭合），紧接着 `Stage all` 抵达不了任何可决断的东西（缺陷 35）。其下游的一切
  —— 提交、`Commit & Push ⇡`、`git_push` 那一半、`NoUpstream`、上游、推送 —— 都没有
  被驱动，本文也不对其中任何一项作出主张。
- **诚实的总账：** E3c 所证的每一个 `0.3.5` 信任界面依然成立，而 E3d 补上了 Lane
  的*工作*这一半 —— 正确的工具作用域、正确的重启归属、逐字的操作者输入、在已停止的
  Lane 上可达的中间视图，以及一条终于指名操作者所选 Lane 的提交栏。目标 4 的端到端
  主张在 GUI 上**仍未满足**，如今是因为暂存这一步上的一个阻断原因，而不是 E3c 指出
  的那三个。退出标准 1 就"候选版已被原生驱动"、"Lane 的回合"、以及"Lane 的工作落在
  Lane 里"而言是满足的；就完整任务而言并不满足，本文也不假装满足。

## 缺陷 35–38 —— H7（2026-09-14，位于 `claude/gui-shell-regressions-h7`）

分支 `claude/gui-shell-regressions-h7`，工作树 `.worktrees/gui-shell-regressions-h7`，
基线为 `claude/int-0.3.5` 的 `1963f8d1`（合入 E3d 证据的那次提交）。

本批次获准进行的那次范围狭窄的原生确认**已经进行**。第一次检查时屏幕是锁着的
（`time=2026-09-14T08:23:46Z CGSSessionScreenIsLocked=1 onConsole=1`），运行开始
前已解锁（`time=2026-09-14T08:31:04Z CGSSessionScreenIsLocked=0`），从
`08:31:30Z` 到 `08:58:20Z` 的每一次按键与点击批次前读到的都是
`CGSSessionScreenIsLocked=0`，`09:02:25Z` 再次锁定，原生驱动即在那里停止。按顺序
启动的 PID：22821、29999；两个都由本批次杀掉，没有一个自行退出。两次启动的
`VIDEN_HOME` 都指向本次运行的暂存目录，因此本仓库自己的 `.viden/` 从未被读写。

**本批次的要点：H6 并没有引入缺陷 35，其他任何批次也没有——桌面事件泵一直攥着
适配器互斥锁、挡在操作者自己的手势前面，而 E3d 是终于因此丢掉一条命令的那次运
行。** H6 的测试之所以通过，是因为它们拿模拟端口驱动驾驶舱、并直接调用适配器。
这两层都没有事件泵。出厂路径是 webview -> `main.ts` 端口 -> Tauri `invoke` ->
`lib.rs` 命令 -> 适配器；每条命令都跑在并非事件泵的线程上；两者要拿同一把
`DesktopState` 适配器互斥锁。`spawn_core_event_pump` 在 250 毫秒的阻塞接收期间
一直持有那把锁，放开的一瞬又立刻拿回，而 `std::sync::Mutex` 不承诺公平性——于是
一条命令可以无限期地排在后台排空之后。

### 测试装置，以及它在任何修复之前发现的东西

`apps/gui/tests/h7_shell_path.rs` 驱动的是出厂命令函数体——现在位于
`apps/gui/src-tauri/src/shell.rs`，每个 `#[tauri::command]` 一个函数体、每条命令
都委托给它——对着真实的 `LocalCoreHost`、临时会话主目录与临时 Git 夹具，旁边在同
一个 `DesktopState` 上跑着一条真实的事件泵线程。其中没有任何东西读写本仓库的
`.viden/`。

在基线上它不是失败，而是**挂住**。四十次只拿适配器锁、不产生任何流量的
`operator_git` 读取，在 120 秒内没有完成，看门狗中止了二进制（`signal: 6,
SIGABRT`）。对卡住进程的 `sample` 毫不含糊：

| 线程 | 位置，按采样数 |
| --- | --- |
| 命令线程 | `shell::d1_poll` -> `with_adapter_mut` -> `Mutex::lock`，**1384 中的 1359** |
| 事件泵线程 | `pump_core_events_once` -> `pump_events` -> `receive_event`，持有那把锁，**1384 中的 1382** |

E3d 为 35、37、38 记录的每一个症状，都是一次从未得到回答的读取，而进程在其中始终
存活。

### 判定

| # | 判定 | 提交 | 位置 | 测试 |
| --- | --- | --- | --- | --- |
| 35 | **已修复，并已原生确认。** 既不是 Lane 作用域的问题，也不是 H6 的问题：暂存之所以没有离开客户端，是因为承载它的 `invoke` 从未到达适配器。`pump_core_events_once` 现在**尝试**拿锁而不是等待——持有锁的命令正在回答一个手势，这一轮没有排空的事件仍在队列里——并且在锁被空出 `PUMP_YIELD` 之前不返回。排空窗口从 250 毫秒降到 `PUMP_DRAIN`。在事件泵运行的外壳路径上，Lane 作用域的暂存以 Core 自己的 `stage` 动词上线，Core 的 `git_add` 审批抵达坐席，批准即完成。原生：`Stage all` 在决策中心引发 `git_add`，批准后文件被暂存，整轮运行两次写入 `source.stage` | `22eb5af4` | `apps/gui/src-tauri/src/shell.rs`、`apps/gui/src-tauri/src/lib.rs:1447` | `the_desktop_pump_leaves_the_adapter_lock_free_for_the_operators_own_command`、`a_lane_scoped_stage_reaches_the_permission_dock_through_the_shell` |
| 25 | **仍然开放，但已越过暂存这堵墙。** 在进程内，E3b 第 19–21 步端到端跑通。原生：暂存与两次提交都带着 Core 自己的审批与持久审计行通过，而 `git_push` 那一半随后完全没有引发审批——与 E3b 第 22 步撞上的是同一堵墙。仍然开放，且不在暂存路径的客户端一侧 | `22eb5af4` | 同上 | `the_commit_and_push_pair_runs_through_the_shell_with_a_lane_selected` |
| 36 | **客户端一半已修复并已原生确认；这条后续项在 Core 上仍未满足，且现已指名。** 客户端把一条它只是*发送*出去的草稿退了场。驾驶舱现在在发送时不标记任何提交，通过新的 `d1_queue_follow_up_poll` 路由轮询队列自己的挂起项，只在由 Core 的 `InputQueued` 支撑的 `confirmed` 结果上让文本退场，否则渲染一条 `role=alert` 说明这条追加消息已发出但未被确认——文本仍在框里。**另一半属于 Core**：它对 `RuntimeCommand::QueueFollowUp` 的受理路径带有 `owner.lane_id.is_none()` 守卫（`crates/runtime/src/runtime_supervisor.rs:1612`） | `a324ade6` | `apps/gui/src/screens/d1_cockpit.ts:3479`、`apps/gui/src-tauri/src/shell.rs:182`、`apps/gui/src-tauri/src/lib.rs:877` | `a_follow_up_submitted_during_a_turn_is_never_reported_as_queued_unless_core_said_so`；`tests/h7_shell_regressions.spec.ts` "defect 36" |
| 37 | **已加入日志与重试；掉线本身是归因，不是证明。** `RUST_LOG=info` 之所以产出零字节，是因为这个工作区里没有任何 `log` 或 `tracing` 订阅者。`connection_log` 对每一次**状态迁移**无条件向 stderr 写一行，绝不按轮询写。Core 不发布重连命令（`GUI-CORE-003`），所以重试是主机重跑它本就拥有的握手与快照——`maintain_connection`，每个事件泵轮次一次，退避有界为 1/2/4/8/16 秒，尝试次数、倒计时、逐字的传输错误与放弃状态都由 `D6RecoveryProjection.reconnect` 承载并渲染在 `CONNECTING` 旁边。**掉线本身的成因不作断言** | `0a5d55ec` | `apps/gui/src-tauri/src/connection_log.rs`、`apps/gui/src-tauri/src/adapter.rs:6737` 与 `:6767`、`apps/gui/src-tauri/src/d6.rs`、`apps/gui/src/screens/d6_recovery.ts` | `a_severed_core_connection_is_logged_and_retried_with_a_visible_countdown`；`tests/h7_shell_regressions.spec.ts` "defect 37" |
| 38 | **在进程内未复现。不宣称修复。** 第一个 Lane 的执行通过恢复面板自身的 `close_lane` 控件停止，第二个 Lane 在它旁边创建，原生运行发现失灵的两半在这里都成立 | `8f194708` | `apps/gui/src-tauri/src/shell.rs`（`d6_send_intent`） | `a_lane_created_beside_a_stopped_lane_keeps_its_owner` |

### 原生确认

一个位于本次运行暂存目录下的全新临时 Git 仓库，一次提交 `895cdd9`，一个
`.viden/config.toml` 选定 `provider = "fallback"` / `model = "test-local"`。Core
铸出 `prj_1789374856626993000`。一个 Lane：`lane_1789375075442733000`。绑定在启动
时通过 `VIDEN_GUI_WORKSPACE` 完成。每一帧都按窗口 id 拍两次、保留第二次，并且列出
之前都已查看。

| # | 步骤 | 是否到达 | 结果 | 帧 |
| --- | --- | --- | --- | --- |
| 1 | `+ New Lane`、内置智能体、TASK、`Create lane ↵`、Core 的 `lane_create` 审批、`Y · Once` | 是 | `lane_1789375075442733000`，`⎇ 1 worktree` | — |
| 2 | 输入区 `tool edit_file path=README.md old=H7 new=Edited` | 是 | `Approve edit_file · edit_file`，`Medium risk`，输入区标签切换为 `Queue follow-up` | [`01`](../../../apps/gui/evidence/main-window-interactions/h7-01-followup-sent-not-acknowledged.png) |
| 3 | **缺陷 36 —— 在该轮次等待审批期间提交一条追加消息** | 是，**客户端一半已闭合** | 输入区**保住了文本**，下方渲染 `Sent, but Core has not acknowledged the follow-up. Your message is kept.`，没有画出排队行，`EVENTS` 停在 `#49`。E3d 的沉默没有了，E3c 的诚实回来了 | [`01`](../../../apps/gui/evidence/main-window-interactions/h7-01-followup-sent-not-acknowledged.png) |
| 4 | 对 `edit_file` 按 `Y · Once` | 是 | ` M README.md`、`# Edited Fixture` —— 产品自己的编辑，落在 **Lane 工作树** | — |
| 5 | **缺陷 35 —— 选中 Lane 时的 `Stage all`** | 是，**已闭合** | 决策中心读到 **`1 awaiting you`**，卡片为 `Approve git_add prj_… · lane_… · pending · low`。E3d 读到的是 `0 awaiting you` 且没有任何 `git_*` 行 | [`02`](../../../apps/gui/evidence/main-window-interactions/h7-02-stage-raises-git-add.png) |
| 6 | 对 `git_add` 按 `Approve once` | 是 | ` M README.md` -> `M  README.md`，提交栏渲染 `Stage completed · viden/lane_… ↑0 ↓0 · uncommitted changes remain`。`audit.jsonl` 中持久留下 `source.stage` 的 `authorized` 与 `completed` | [`03`](../../../apps/gui/evidence/main-window-interactions/h7-03-stage-completed.png) |
| 7 | **在自己的审批下提交** | 是 | `ad8b445 Edit the fixture README heading in the Lane`，工作树转为干净。`audit.jsonl`：`source.commit` 的 `authorized` 与 `completed` | — |
| 8 | **`Commit & Push ⇡` 的提交一半** | 是 | 第二处改动走同一对命令：`d798e8a Add the push-leg line`，另有四条持久行 | — |
| 9 | `git_push` 那一半、`NoUpstream`、设置上游、推送被接受 | **否** | 提交栏停在 `Push is waiting for approval in the permission dock.`，决策中心读到 `0 awaiting you`，**从未出现 `git_push` 审批，也从未出现 `source.push` 行** —— 观察 60 秒。**这就是缺陷 25，原封不动，仍然开放** | [`04`](../../../apps/gui/evidence/main-window-interactions/h7-04-push-half-no-approval.png) |

**所以墙正好挪了一步。** E3d 无法暂存；这次运行完成了暂存与提交，而且做了两遍，
带着 Core 自己的审批与 Core 自己的持久审计行。它仍然做不到的是推送。缺陷 35 已闭
合；缺陷 25 没有。

### 观察，不是缺陷

- **一次审批只有五分钟的寿命，这次运行为此丢了一次。** 第一次 `git_add` 审批被正
  确引发，随后在本次运行读取状态期间过期；其后的那次暂存停在
  `Stage sent · waiting for Core.` 且完全没有审批，只有重启才清除——因此**一次操作
  者 Git 动作的审批过期后，可能让下一次无法引发自己的审批**，此处记录而不编号。
- **权限坐席不渲染 Lane 作用域的 `git` 审批；决策中心渲染。** 两次 `git_add` 审批
  都出现在 D2 而不在 D1 坐席里，E3b 当时也是在那里找到它们的。未作调查，也不宣称
  为缺陷。
- **连接日志在出厂包里是活的。** 两次启动都在无任何配置的情况下向进程 stderr 写出
  了 `viden-gui core-connection state ...`。

### 静态那一半

`apps/gui/tests/shell_port_contract.spec.ts` 走查源码，检查出厂路径上三个在运行期
按名字匹配、一旦漂移就沉默的接缝。它在本批次自己的工作里发现了一处缺口——一条没有
任何端口调用的 `d6_recovery` 命令——那条命令已被删除（`9766e279`）。

### 对兼容性后续项 16 的判定

| # | 对候选版的判定 |
| --- | --- |
| 16 | **在 GUI 上仍未满足，但不再沉默。** 输入区保住操作者的文本，说明这条追加消息已发出而未被确认，绝不报告一个 Core 没有确认过的排队。要满足这条后续项，需要 Core 在受理路径上像接受会话作用域那样接受 Lane 作用域的 `QueueFollowUp`；没有任何 GUI 改动可以替代，本批次也没有尝试替代 |

### 仍然欠下的

- **E3e，原生：** `Commit & Push` 的 `git_push` 那一半、`NoUpstream`、上游与推送
  —— 缺陷 25，本次运行唯一没能到达的一步；缺陷 31 的 `Always`/撤销这条腿与规划工
  作台这条腿，自 E3c 起仍未驱动；以及上面那条审批过期的观察。
- **在 Core 上：** Lane 作用域的 `QueueFollowUp` 受理路径（缺陷 36 的另一半，兼容
  性后续项 16），以及让 `git_push` 那一半无法引发审批的原因（缺陷 25）。

## Core 侧的解决

针对本文件记录在 GUI 名下的缺陷与后续项，Core 侧给出的关闭。Core 的关闭是一项
进程内声明：该行为由 Core 自己的测试把住，原生确认欠到下一次 GUI 运行。

### C18 —— 2026-09-14，在 `claude/core-lane-queue-c18` 上

| # | 解决 | 证据 |
| --- | --- | --- |
| 36（Core 那一半）/ 后续项 16 | **进程内关闭。** 受理路径的 `QueueFollowUp` 分支带有 `owner.lane_id.is_none()` 守卫，因此 Lane 作用域的追加提示根本不匹配它，而是落入唯一 supervisor worker 的通道 —— 也就是该 Lane 那条尚未被回答的回合正占用着的 worker —— 不发布 `CommandAccepted`，不发布 `InputQueued`，不发布任何事件。这正是本文件在 E3d 记录、在 H7 指名的「`EVENTS #49` 前后不变」。Lane 的条目现在进入由 runtime 拥有的按 Lane 分开的队列，在受理路径上发布，并把该 Lane 自己的 owner 放在 `QueuedInputView.owner` 上；排空由该 Lane 自己 owner 的 `TurnFinished { completed }` 触发：先 `InputDequeued`，再为该条目启动它自己的、归属该 Lane 的回合。回合失败或被取消的 Lane 保留队列；杀死该 Lane 的 Agent 会话会丢弃队列并逐条公告；队列条目不是持久事实，不跨越 Core 重启 | `crates/runtime/src/tests/lane_queue_tests.rs`（四条测试，第一条断言确认在该 Lane 的回合仍停在审批上时就已到达）；`runtime_supervisor_kill_discards_the_lanes_follow_up_queue` |

**GUI 仍需要做什么。** 发送路径上没有新增：H7 已经发送该 Lane 精确的提交 owner，也
已经只在由 Core 的 `InputQueued` 背书的 `confirmed` 结果下才清掉输入区文本，而这正
是 Core 现在发布的事实。消费时有两点要注意 —— 读 `QueuedInputView.owner`，不要假定
只有一个队列，因为会话的条目与每个 Lane 的条目彼此独立、由不同的回合释放；以及继续
把 `InputDequeued` 读作「已离开队列」，因为杀死会话导致的丢弃会发布它，却不会有指名
该条目的 `TurnStarted`。

**仍然欠下的：** E3e 的原生确认。这里没有任何一项经过真实 GUI 驱动。
