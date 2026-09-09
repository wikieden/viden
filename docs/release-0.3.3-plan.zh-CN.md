# Viden 0.3.3 计划 - 可信交付与可操作 GUI beta

English version: [release-0.3.3-plan.md](release-0.3.3-plan.md)

`0.3.3` 是 `PLAN.md`（"可操作 GUI beta 与兼容性加固"）和
`docs/parallel-development-plan.md`（"可信交付与可操作 GUI beta"）命名的
workspace 级里程碑。本文把这条一行条目展开为批次、门禁和完成判据。其中的
Core 契约内容见
[release-0.3.3-contract-design.zh-CN.md](release-0.3.3-contract-design.zh-CN.md)。

状态：计划于 2026-09-09 批准派发。本文中的任何内容都尚未实现。下面每个批次
单独派发、在主会话做对抗式评审，且只在明确的 push 指令下合并。

## 基线

2026-09-09，`main` 位于 `216c1e4f`：

| 线 | 版本 | 状态 |
| --- | --- | --- |
| Core | `0.3.5` 不可变检查点 | 19 个扩展 capability，schema `1` |
| TUI | `0.3.3`（组件线） | native/ACP 交互已对 Core `0.3.4` 认证 |
| GUI | `0.1.0-rc.3` | D11/D4/D1/D2/D6/D10/D12/D13/D14 外壳跑在 CoreClient 接缝上 |

自 `0.3.2` 于 2026-08-30 收口以来，`main` 新增：GUI 契约请求积压的裁决
（11 条关闭、5 条延后）、`viden-lanes` 与 `viden-agents` 两次 crate 切分、
transcript 双重持久化修复与 R1 评审批次、QW1 D1 快赢、设计裁决 `D-RAILNAV`。
`apps/gui/contract-requests.md` 中未关闭的条目为 009、012、013、015、018、
019、020、021、023。

## 目标

1. **可信交付面。** 把已登记的 `DiffReview` in-cockpit 视图建成操作者 git
   动作（GUI-CORE-020）、结构化审批 diff（GUI-CORE-012）、结构化冲突内容
   （GUI-CORE-015）的唯一宿主，落在一次 Core 契约增量上，而不是三个点修。
2. **证据面。** 建成已登记的 `EvidenceView` 读侧：分页证据查询与规范字节
   内容读取。
3. **兼容性加固。** 两个客户端对每个新扩展 fixture 回放出相同业务事实；
   差距评审期间记录的延后代码跟进项与 D1 卫生项全部关闭或重新标日期。
4. **可审计的真实任务。** 一个真实的 local-first 开发任务经由 GUI 从 intake
   跑到提交变更，审计与证据记录在 `docs/release-evidence/` 下。

## 对控制计划的范围修订

日期 2026-09-09；以注记形式记入 `docs/parallel-development-plan.md`。

- **Plan Studio 与 Agent Board 移到 `0.3.4`。** 二者都没有登记的 D 屏，也
  没有 Core 侧的 plan handoff 或 board 契约，而 `0.3.3` 已经新增两个驾驶舱
  面。没有契约就再加第四、第五个面，等于重演 `D-RAILNAV` 刚解决的未登记
  视图问题。
- **"GUI 完成 D2/D10/D12/D14" 的解读为：** D2 获得审批的类型化决策上下文
  （hunk）；D12 获得结构化冲突内容；D10 与 D14 只做卫生改动。二者已在
  `0.3.2` 的 lane 债务清理中迁移到 audit 与 live-work 契约。
- **依 `D-RAILNAV`：** WorktreeBoard 已砍；内嵌 subagent 树、DiagnosticsView、
  DockSD 召唤坞留在 `0.3.3` 之外。

## 组件版本目标

| 线 | 现在 | `0.3.3` 目标 |
| --- | --- | --- |
| Core | `0.3.5` | `0.3.6` 不可变检查点，schema `1`，四个附加 capability |
| TUI | `0.3.3` | `0.3.4`，四个 capability 的最小 parity |
| GUI | `0.1.0-rc.3` | `0.1.0-rc.4`，带 DiffReview 与 EvidenceView 的 beta 候选 |

workspace 聚合版本保持未发布。打包、公证、Homebrew 与实时 provider 认证
属于 `0.3.4` 发版门禁。

## 批次

每个批次是 `.worktrees/` 下的一个 worktree，测试先行，合并前按其 brief
评审。Core 批次先于消费它们的客户端批次落地；精确形状见契约设计。

| 批次 | 归属 | 内容 | 依赖 |
| --- | --- | --- | --- |
| C1 `runtime.structured_diff` | Core | `DiffDocument` 类型、解析器提升、`ApprovalRequestView.decision_context`、`WorkspaceChangeView.diff`、`QueryWorkspaceDiff` / `WorkspaceDiffLoaded`、fixture `structured-diff.json`、文档 | 无 |
| C2 `runtime.operator_git` | Core | `RunOperatorGitAction` / `OperatorGitActionFinished`、复用 tool spec 做权限门、审计先于效果、source 重采样、fixture `operator-git.json`、文档 | C1 |
| C3 `runtime.conflict_content` | Core | `ConflictBounce` 与 `LaneConflictView` 上的 `ConflictContent`、两个 apply 站点的生产者、fixture `conflict-content.json`、文档 | C1（与 C2 并行） |
| C4 `runtime.evidence_reads` | Core | `QueryEvidence` / `EvidencePageLoaded`、`ReadEvidenceContent` / `EvidenceContentLoaded`、fixture `evidence-reads.json`、文档；开立并关闭 GUI-CORE-025 | C1 |
| D0 gui-kit 提升 | 设计包 | 按 `D-SOT` 把 `.review` 与 `.evwrap` 两族从 D1 内联 CSS 提升进 `gui-kit.css`；包守卫；变更日志 | 无（并行） |
| G1 DiffReview | GUI | in-cockpit 视图（文件树、已暂存标记、统一 diff、提交栏）、标题栏 sync 芯片变为控件、D2 hunk 行、qa 状态、PNG 证据、关闭 012 与 020 | C1、C2、D0 |
| G2 冲突 + 证据 | GUI | D12 ours/theirs/base hunk、EvidenceView（按天分组列表、详情、关联芯片、在 review 中打开）、关闭 015 与 025 | C3、C4、D0 |
| T1 TUI parity | TUI | 审批浮层 hunk 行、`/git` 系统命令、决策浮层中的冲突详情、证据检视器、带原因的回归基线 | C1 至 C4 |
| H1 卫生 | GUI、TUI | 过期引用（`d1.rs` 的 004/006、D11 的 001、`GUI-CORE-D1-OWNER-CARDINALITY` 形似码）、不可达的 `component_gallery.ts`、缺失的 D4/D13/D2 基础态/D10 多 lane PNG、D2 契约详情对已决记录仍显示 Confirm/Reject、死代码 `CockpitProjection.assistant_stream`、active-work 定义分歧、gate id 键控不一致 | 无 |
| S1 stretch | Core、TUI | GUI-CORE-013 待确认契约事实，加上 handoff/contract/dependency 创建流（其 intent builder 已存在于 `apps/tui/src/tui/supervision.rs`） | C1 至 C4、G1、G2 先合并；否则滚到 `0.3.4` |
| E1 发版证据 | 全部 | 一个真实任务：intake、Lane、带 hunk 的编辑审批、check run、DiffReview 提交、push 先拒后准、证据与审计复核；双语 checkpoints 放在 `docs/release-evidence/gui-trusted-delivery/` | G1、G2、T1 |

派发顺序：C1；然后 C2 与 C3 并行，同时跑 D0；然后 C4；然后 G1、G2、T1，
H1 在评审空档穿插。S1 只在此前全部内容进入 `main` 后启动。

## 门禁

每个批次先跑最小相关检查，再跑 `docs/parallel-development-plan.md` 的分支
门禁。

Core 批次：

```bash
cargo test -p viden-types
cargo test -p viden-runtime
cargo test -p viden-core
scripts/check-dependency-boundaries.sh
cargo test --workspace --quiet
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
```

另加：九个冻结的 `frontend-contract-v1` 基础 fixture 保持字节与 view 摘要
不变；每个新事件在同一提交里带 `is_known_runtime_event_type` 分支与
fixture；`scripts/tui-regression.sh` 的 capability 计数从 19 改为 23 并带
跟踪注释；每个新命令的拒绝都是带调用方 command id 的 `CommandRejected`。

TUI 批次：

```bash
cargo test -p viden-tui
scripts/tui-turn-controller-smoke.sh
scripts/rc-tui-stability-smoke.sh
scripts/tui-regression.sh
```

GUI 批次：

```bash
npm --prefix apps/gui test -- --run
npm --prefix apps/gui run build
cargo test -p viden-gui
cargo test -p viden-gui --test capture_projections -- --ignored
```

另加批次报告中列出的 qa harness 状态与 headless PNG 重采，以及双语更新的
`apps/gui/EVIDENCE.md`。

文档与设计包：

```bash
scripts/check-doc-pairs.sh <changed-markdown> [...]
scripts/check-doc-links.sh <changed-markdown> [...]
git diff --check
node docs/viden-design/Viden/tools/run-checks.node.js
```

已知对负载敏感的测试，先隔离重跑一次，仍红则串行：
`acp_async_job_can_be_cancelled_by_pid`、`context_reducer_process_*` 族、
`inline_image_bytes_become_referenced_evidence`。

## 完成判据

只有当以下各项在 `main` 上全部成立时，`0.3.3` 才算完成：

- Core `0.3.6` 以不可变检查点记入 `crates/core/release-manifest.toml`，带
  23 个 capability、四个新扩展 fixture、基础 fixture 字节不变。
- GUI 以已登记的组件族交付 DiffReview 与 EvidenceView，D2 渲染决策上下文
  hunk，D12 渲染冲突内容，每个缺失事实都显示为不可用而不是某个值。
- TUI 在四个 capability 上达到最小 parity，每条移动的回归基线都有明确原因。
- 两个客户端对四个新 fixture 回放出相同业务事实。
- 登记册带日期关闭 012、015、020、025；009、018、019 带 `0.3.4` 注记；
  021 与 023 保持 `0.3.4+`。
- E1 证据文档双语存在，`docs/core-0.3-compatibility.md` 中的延后跟进项已
  关闭或重新标日期。

明确不属于完成范围：三平台打包、公证、Homebrew 同步、DeepSeek 实时认证、
Plan Studio、Agent Board、forge 状态（021）、多 workspace 监督（023）。

## 风险

- **冻结摘要移动。** 给 `ApprovalRequestView` 与 `WorkspaceChangeView` 加
  可选字段不得改变任何基础 fixture view 的规范字节；字段缺席时不序列化。
  C1 的冻结 fixture 测试是证明，基础摘要一动即停批。
- **共享权限词表。** 操作者 git 动作复用 agent 的 tool spec，使一套
  `viden.toml` 规则同时约束两者。新 spec（`git_fetch`）必须在 G1 暴露 fetch
  之前与其他 spec 一并写入文档。
- **真实仓库中的边界。** diff 与冲突载荷按字节和行数限界；两个客户端必须
  把 `truncated` 与 `omitted` 当事实渲染，绝不渲染成空变更集。
- **冲突内容语义。** 内容展示的是传入 hunk、当前文件区段和补丁前像。它不是
  三方合并，也不得被标成三方合并。
- **stretch 蔓延。** S1 是 stretch；C1 至 C4 若延期，S1 滚到 `0.3.4`，不重谈
  完成判据。
