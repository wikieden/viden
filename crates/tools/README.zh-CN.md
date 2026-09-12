# viden-tools

## 目的

`viden-tools` 负责内置本地工具和执行适配器。

## 不负责

- 权限决策。
- 模型规划。
- Transcript 或 workflow state。
- Merge gate orchestration 或 workflow persistence。

## 公共接口

- `BuiltinTool`
- `ToolRegistry`
- shell、files、glob、grep、web、Git 内置工具。
- 用于 Git worktree、本地进程组、类型化 tmux/PTY terminal backend 和
  checked patch apply 的 Lane effect adapters。

## 不变量

- Mutating tools 必须在 `ToolSpec` 中标记为 mutating。
- 输出必须变成可序列化的 `ToolResult`。
- Shell 保持平台适配：Unix 用 POSIX，Windows 用 PowerShell。
- 本地 Lane 进程不会把 stdout 或 stderr 留在无人读取的 pipe 中：调用方可
  指定持久化合并日志，否则输出会被明确丢弃。
- `TerminalBackend` 将类型化 tmux/PTY 的启动、输入和停止语义与普通
  `ProcessBackend` 子进程 effect 分开。
- Patch adapters 必须先准备全部创建、写入和删除，再触碰文件系统；因此标准
  `/dev/null` 新建/删除 diff 也进入同一套 runtime transaction 安全回滚。
- 严格 apply 写不了的文件必须是明示结果，绝不是静默的缺失。二进制片段 —— Git 报告
  为 `Binary files … differ` 或 `GIT binary patch` 的那些 —— 会由 `check` 与 `apply`
  在 `PatchApplyOutcome.conflicts` 中点名，同一补丁中的文本文件仍照常应用；
  `conflict_content` 把它发布为一条不带行、`reason` 为 `ConflictHunkReason::Binary`
  且没有原像的 hunk。
- Git worktree tools 必须委托给 Core Lane orchestration 使用的同一套 Lane
  worktree adapter。

## `.ref` 对齐

用 Rust traits 和本地 adapters 对齐 `.ref` 的 `Tool.ts` 和 tool registry 行为。

## 测试

```bash
cargo test -p viden-tools
```
