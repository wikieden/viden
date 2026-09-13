# Viden Core 分支线 Agent 指南

英文版：[AGENTS.md](AGENTS.md)

## 作用范围

本文件适用于 `crates/**`。在 V3 并行开发中，这里是 Core 的归属区域。
在这里改代码之前，先读仓库根目录的 `AGENTS.md`、`PLAN.md` 和
`docs/parallel-development-plan.md`。

## 分支与 Worktree

- 在 `.worktrees/v3-core-runtime` 中使用分支 `codex/v3-core-runtime`。
- 从同步后的 `origin/main` 创建它。
- 保留无关工作，不要编辑其他负责人的 worktree。
- 根 workspace manifests、frontend contracts 和共享 fixtures 都是共享界面；
  在交接中说明它们受到的影响。

## C0 Contract Freeze

在正式 TUI 或 GUI 实现开始之前，为 `core-v0.3.0` 交付一个不可变的
`frontend-contract-v1` checkpoint，内容包含：

- 版本化的 `RuntimeCommand -> RuntimeEvent -> RuntimeViewState` 协议；
- `schema_version`、capability discovery、event cursors、snapshot/replay 和
  sequence-gap recovery；
- typed lane、task、gate、route、gate-strength 和 mutation-policy 记录；
- 传输中立的 Core client contract；
- 覆盖语言、locale、skin、mode、density、font scale、terminal color capability
  和 accessibility flags 的共享展示偏好 contract；
- 对历史持久化记录和 fixtures 的 migration 覆盖；
- Core、TUI 和 GUI 共享的 parity fixtures。

报告确切的 checkpoint SHA；在 TUI 或 GUI 分支已经从它开出之后，
不要再重写那个 checkpoint。

## 归属与边界

- Core 独占权威 runtime facts、持久化、permissions、
  provider/tool 执行、lane 副作用和 workflow reduction。
- `crates/lanes` 和 `crates/agents` 是 runtime 之下的叶子 crate，
  带有枚举式依赖 allow-list。Runtime 拥有的策略——permission
  context、approver、event sink、持久化——只作为参数注入给它们，
  它们不会反向导入；
  `scripts/check-dependency-boundaries.sh` 同时强制 allow-list 和
  它们带走的模块不得复现。给这两个 crate 中任何一个新增 `viden-*`
  依赖是一次架构决策，而不是图方便。
- 保持 Core 独立于 `apps/tui` 和 `apps/gui`。
- 不要在这里实现前端布局或前端本地交互状态。
- Contract freeze 之后，尽可能做向后兼容扩展。破坏性变更需要
  schema version、migration、fixtures，以及显式的跨分支线通知。
- 所有 mutation 仍然要先经过 permission checks，然后才产生副作用。
- 保持 append-only JSONL facts 和可重建的 SQLite 索引。
- Plan mode 必须在文件、shell、Git、workflow、memory 或 task 状态发生变化
  之前拒绝 mutation。
- Core 拥有用户展示偏好的持久化和事件发布。TUI 和
  GUI 可以用不同方式渲染这些偏好，但由 Core 定义可接受的取值，
  并通过 frontend contract 发出生效的偏好状态。

## 开发与验证

- 行为变更使用 TDD。
- 等待时间的测试不得断言墙钟时间上界。把被测对象挂在测试自己掌握的信号上 —— 测试释放的
  channel、子进程等待的文件、被杀死的子进程永远写不出的 marker —— 剩下的时长只作为宽松的
  存活上限。完整工作区套件并行运行且不排除任何 crate，因此与代码自身 deadline 赛跑的上界
  是 flake 而不是检查。编译或派生辅助程序的测试基础设施必须每个测试进程做一次，而不是每个
  测试做一次。
- 为协议边界和不明显的 invariants 补充简洁注释。
- 同步更新中英文面向用户的文档。
- 开发过程中运行聚焦的 crate 测试，然后运行：

```bash
cargo test -p viden-types
cargo test -p viden-session
cargo test -p viden-workflows
cargo test -p viden-lanes
cargo test -p viden-agents
cargo test -p viden-runtime
cargo test -p viden-core
scripts/check-dependency-boundaries.sh
cargo test --workspace --quiet
```

交接必须包含分支、worktree、HEAD、contract/schema 变更、
migrations、fixture 覆盖、测试证据、前端可消费的接口、
未决 contract requests 和 blockers。没有用户明确指示，
不要合并或推送 `main`。
