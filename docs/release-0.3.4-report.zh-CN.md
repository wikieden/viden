# Viden 0.3.4 完成报告

English version: [release-0.3.4-report.md](release-0.3.4-report.md)

日期：2026-09-12。由 [release-0.3.4-plan.zh-CN.md](release-0.3.4-plan.zh-CN.md)
的发布证据步骤（批次 E2）撰写。

**本报告中的任何内容都没有被推送。** 整个 `0.3.4` 候选都位于本地集成分支
`claude/int-0.3.4` 上；`main` 仍在 `25072a0a`。没有 tag、没有发布、没有 Homebrew
改动、没有公证、没有 live provider 认证。

## 候选

| 线 | 版本 | 位置 |
| --- | --- | --- |
| 工作区候选 | `0.3.4` | `claude/int-0.3.4` 的 `39ed155dcc1847b915965f49626e6779b8b538d7`，领先 `main` 的 `25072a0acee7a9959bfd8060721378cb3d4d5397` 68 个提交 |
| Core | `0.3.7` 不可变契约 checkpoint | checkpoint SHA `39ed155dcc1847b915965f49626e6779b8b538d7`；schema `1`；15 个冻结基础 + **29** 个扩展能力；基础契约 checkpoint `5bd2b80b…` 自 `0.3.0` 起未变 |
| TUI | `0.3.5` | `min_core_version` 有意保持在 `0.3.4` —— 新能力都是独立协商的特性门，因此没有一个会阻止基础 schema-1 客户端启动 |
| GUI | `0.1.0-rc.5` | `[core].minimum_version` 出于同样理由有意保持在 `0.3.5`；macOS `.app` 以该版本构建 |

Core `0.3.7` checkpoint 在此一次性声明而不是逐批声明，因为在契约还在移动时命名一个
checkpoint，命名的是一个并不存在的契约。它指名的是一个本地分支上的提交：**如果
`claude/int-0.3.4` 在合并前被 rebase，该 checkpoint 必须针对新的 SHA 重新声明**，
而不能假定它自动存续。

## 交付了什么

六个增量式 Core 能力，把对外通告的扩展集合由 23 推到 29，且九个冻结基础 fixture 的
字节未变：

| 批次 | 能力 | 它使什么成为可能 |
| --- | --- | --- |
| C5 | `runtime.workspace_owner`、`ui.layout_preferences` | 作用域在工作区根目录的工作拥有了操作者身份，因此提交与 push 不再需要 Lane；Lane worktree 不再覆盖工作区来源 chip；驾驶舱布局得以持久化。关闭 GUI-CORE-027 |
| C6 | `runtime.turn_lifecycle` | 原生轮次有了终结事实，因此客户端的忙碌判据是 Core 事实而不是显示残留，且会话队列在一个已完成轮次之后排空 |
| C7 | `runtime.durable_work_evidence` | 一次已应用的原生变更会归档出一条带规范字节的 `patch` 行，而一次审批决策会成为一条持久审计行。关闭 GUI-CORE-028 |
| C8 | `runtime.transcript_rows` | owner 作用域的有序类型化行。关闭 GUI-CORE-009 |
| C9 | `runtime.workspace_file_reads` | 在 `read_file` 权限门之后读取一个文件的字节 |
| C10、C11 | —— | 工作区 owner 的绑定移入共享 bootstrap，使每个前端入口都能拿到该身份；持久化的转录字符串按 UTF-8 回放 |

五个 GUI 批次把驾驶舱推到与已接受的 D1 设计齐平 —— 带舱内次级视图的 rail-as-router
导航外壳（G3）、Lane 标签条、内联工具 diff、palette 创建 Lane 与聚焦模式（G4）、
带页签的上下文 dock（G5）、D10/D13/D14 视图工作（G6），以及全部六个新能力的消费方
（G7）。T2 把 TUI 带到对等。H2 关闭了 E1 的卫生类缺陷。

## 取到了什么证据，在哪个界面上

本里程碑的要点是目标 6：把 `0.3.3` 留下一半的真实任务做完。它**首次端到端完成**，
是通过 TUI：接入、在 Core 审批下创建 Lane、一次对着 Core 类型化 hunk 批准并应用到
真实文件的变更、一条规范哈希等于磁盘字节的归档 `patch` 行、一条持久的
`approval.allow_once` 审计行、一次暂存、一次**提交**、一次被判为 `NoUpstream` 的
push、一次被**接受**并进入真实 remote 的 push，以及一条在已完成轮次之后被排空的
排队追加提示 —— 其中每一个源码管理步骤都是在**未选中任何 Lane** 的情况下、运行在
Core 发布的工作区 owner 之下完成的。

三类 Core 结果变体首次被实地观察到（`OperatorGitOutcome::Completed`、
`Failed { NoUpstream }`，以及非空的 `EvidencePage` 与 `AuditPage`）；在本次运行之前
它们只存在于 fixture 回放中。

## 哪一部分只完成了一半

**只有一件事，而且这已是连续第三个发布步骤的同一件事：原生 GUI 窗口没有被驱动。**
宿主屏幕在 14:37:21Z 处于锁定状态（`CGSSessionScreenIsLocked = 1`），早于本批次工作
开始；到 14:45:30Z `0.1.0-rc.5` 应用包就绪时仍然锁定。没有发送任何按键，因为锁定的
会话会把输入路由到登录窗口，而解锁需要用户密码。因此：

- `.app` 应用包已**构建**，其事实已记录（`0.1.0-rc.5`、`dev.viden.gui`、arm64、
  ad-hoc linker-signed、40,655,440 字节可执行文件），但没有为驱动而启动；
- **不存在任何原生截图**，因此退出标准 7 未达成；
- GUI 自己的界面由 G3–G7 在 `apps/gui/evidence/` 下的确定性 harness 截图以及 864 个
  vitest 测试覆盖 —— 那是关于渲染的证据，不是关于集成的证据。

因此目标 6 **在契约上与 TUI 上已达成，在 GUI 驾驶舱上未被证明**。剩下的缺口是环境性
的，不是能力缺口：一次屏幕未锁定的运行即可闭环。

按 `0.3.4` 自己退出标准所用的含义（「在 `main` 上」），它同样未完成 —— 候选未推送、
未合并。那是用户的决定，不是缺口。

## E2 新开的跟进项

四个缺陷，每一个都已复现并定位到源码，且在此都未修复 —— 在声明 checkpoint 的那一步
修改 Core 行为，会让该 checkpoint 失效。

| # | 发现 | 归属 | 记录位置 |
| --- | --- | --- | --- |
| 1 | 一条归档 patch 渲染出的 diff 把一个原地修改的文件称为重命名：`evidence_reads.rs:147` 发布了 `render_diff` 的占位 `before`/`after` 头部而没有盖上条目自己的路径，而该输出的另外两个读取方都会这么做 | Core | 兼容性跟进项 14 |
| 2 | Lane 生命周期审批的决策不写持久审计行：`RespondToApproval` 在 lane 分支上于 `record_decision` 之前返回，因此 `lane_create` 显示的审计 id 查不到任何东西 | Core，对审计完整性而言是阻塞级 | 兼容性跟进项 15 |
| 3 | 在一次轮次进行中排队的会话追加提示，其「待排队」状态永远不可观察：`InputQueued` 与 `InputDequeued` 在同一瞬间到达，因此 `queued_inputs` 保持为空，操作者看不到任何队列 | Core/TUI 接缝，产品缺口 | 兼容性跟进项 16 |
| 4 | `NoUpstream` 的恢复路径在 TUI 中没有控件：`/git` 选择器把 `set_upstream: false` 写死，因此 Core 自己的拒绝语所指名的那一步无法触达 | TUI | `release-tui-0.3.4-source-control-parity.zh-CN.md` 的「已知缺口」 |

带入 `0.3.5` 的：兼容性跟进项 10（ACP 产物在实时路径上被规范化但未持久化）与 13
（`interaction-closed-loop` 生成器漂移），以及登记表的十条未决条目 —— 013、018、
019、021、023、026 与 G7 的四条 030–033，029 为 D5 画廊评审预留。

## Gate

在 `claude/e2-release-evidence` 的 worktree 中离线运行。完整尾部输出与 fixture 证明见
[release-evidence/gui-trusted-delivery/checkpoints.zh-CN.md](release-evidence/gui-trusted-delivery/checkpoints.zh-CN.md)。

| 命令 | 结果 |
| --- | --- |
| `cargo test --workspace --quiet --exclude viden-plugin-host --exclude viden-agents` | PASS，exit 0，**2014 passed，0 failed**，78 条结果行 |
| `cargo test -p viden-plugin-host -- --test-threads=1` | PASS，27 passed |
| `cargo test -p viden-agents -- --test-threads=1` | PASS，80 passed |
| `cargo test -p viden-core` | PASS，6 个套件共 61，21 ignored |
| `cargo test -p viden-core --test frontend_contract_v1` | PASS，25 passed，21 ignored |
| `cargo test -p viden-tui` | PASS，427 |
| `cargo test -p viden-types` | PASS，191 |
| `cargo test -p viden-gui` | PASS，311，1 ignored |
| `cargo test -p viden-gui --test architecture_boundary` | PASS，7 |
| `cargo test -p viden-gui --test capture_projections -- --ignored` | PASS，1 |
| `cargo fmt --all -- --check` | PASS，exit 0 |
| `cargo clippy --workspace --all-targets` | PASS，exit 0，仅有警告：共 9 条，每一条都在本批次 diff 未触碰的文件中（`crates/runtime/src/tests/frontend_services_tests.rs`、`crates/runtime/src/runtime_contract.rs`、`crates/workflows/src/lanes.rs`、`crates/types/src/agent.rs`、`apps/gui/src-tauri/src/projection.rs` ×3、`apps/gui/src-tauri/src/adapter.rs` ×2）。新增为零 |
| `scripts/check-dependency-boundaries.sh` | PASS，exit 0 |
| `scripts/tui-regression.sh` | PASS，exit 0；证据在 `target/tui-regression/0.3.5/` |
| `scripts/tui-previews.sh` | PASS（在 `tui-regression.sh` 内运行） |
| `scripts/tui-turn-controller-smoke.sh` | PASS，exit 0 |
| `scripts/rc-tui-stability-smoke.sh` | PASS，exit 0 |
| `npm --prefix apps/gui test -- --run` | PASS，59 个文件 / **864 个测试** |
| `npm --prefix apps/gui run build` | PASS（`tsc --noEmit && vite build`） |
| `npm --prefix apps/gui run tauri -- build --bundles app` | PASS，1 分 10 秒，`0.1.0-rc.5` |
| 九个冻结基础 fixture | PASS —— 与 `git show 25072a0a:<path>` **以及** `apps/tui/release-manifest.toml` 中固定的摘要逐字节一致 |
| `scripts/check-doc-pairs.sh`、`scripts/check-doc-links.sh`（每一对改动的 Markdown） | PASS |
| `git diff --check` | PASS，exit 0 |
| `scripts/release-gate.sh --phase prepublish` | **未运行。** 没有 `DEEPSEEK_API_KEY` 它会拒绝启动，而它的 smoke 环节是一次本批次未被授权运行的 live provider 认证。它的离线环节 —— 依赖边界与文档检查 —— 已在上面各行中 |

`viden-plugin-host` 与 `viden-agents` 串行运行并从并行的工作区运行中排除：两者都有在
本宿主并行负载下抖动的计时测试，`0.3.4` 计划把这一点记为规则而不是通过条件。

## 下一个安全动作

`0.3.4` 中没有任何部分还需要写代码。下一个动作是决定而不是任务，而且有两个：

1. **在宿主屏幕解锁时做那次原生 GUI 运行** —— 这是目标 6 仅剩的一块，也是唯一未达成
   的退出标准。下一次运行所需的 harness 事实已记录在证据文档中。
2. **推送并合并 `claude/int-0.3.4`**，这需要用户明确授权，而授权尚未给出。在那之前，
   本报告中的每一个版本都是本地候选。

`0.3.5` 负责生产发布门 —— 打包、公证、Homebrew、live provider 认证 —— 以及
Plan Studio、Agent Board、D5 画廊评审和上文列出的跟进项。
