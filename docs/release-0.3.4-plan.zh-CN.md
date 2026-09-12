# Viden 0.3.4 计划 - 驾驶舱拉齐与可信交付补完

English version: [release-0.3.4-plan.md](release-0.3.4-plan.md)

`0.3.4` 是 `0.3.3` 候选之后的聚合工作区里程碑。`PLAN.md` 与
`docs/parallel-development-plan.md` 此前把它定名为"视觉保真与生产发布门禁"；
本文重新划定范围（见"范围修订"），首先只做一件事：让已交付的 GUI 驾驶舱与已接受的
D1 设计、以及设计包里已经做好的全部交互与跳转拉齐，并补完 `0.3.3` 只做了一半的
真实任务交付。Core 契约内容在
[release-0.3.4-contract-design.zh-CN.md](release-0.3.4-contract-design.zh-CN.md)。

状态：2026-09-12 依据当日的"设计 vs 实现"差距评审写成。本文没有任何内容已实现。
每个批次单独派发、在主会话对抗式评审、仅在明确的 push 指令下合并。

## 基线

2026-09-12 `main` @ `25072a0a`：

| 线 | 版本 | 状态 |
| --- | --- | --- |
| Core | `0.3.6` 不可变检查点 | 23 项扩展能力，schema `1` |
| TUI | `0.3.4` | 四项 `0.3.3` 能力的对等最小集；输入框路由与 `/git` Lane 目标已修（T1c） |
| GUI | `0.1.0-rc.4` | DiffReview 与 EvidenceView 为驾驶舱内视图；D2/D4/D6/D10/D11/D12/D13/D14 为全窗口独立屏 |

`0.3.3` 的真实任务走到了"经审批的类型化编辑已落盘"，在提交前停住：未选中 Lane 时没有
操作者身份（GUI-CORE-027）；监督器驱动的工作从不进入持久证据归档（GUI-CORE-028）；
会话级排队的后续输入从不执行；审批的审计 id 没有持久审计行。登记册开放项为
009、013、018、019、021、023、026、027、028。

## 差距摘要

来自 2026-09-12 评审（设计包 D1 与 D2–D14 对照 `apps/gui`）；本计划即对它的回应。

- **导航。** 设计的 rail 路由到 Conversation、Lane monitor、Diff review、Evidence、
  Pin lanes、Settings（Worktrees 已放弃、Diagnostics 延期、Inbox 为路线图）。实现的
  rail 路由到 Work、Lanes 开关、Integration gate、Decisions、Lane monitor、Fleet、
  Audit、Settings；两个已登记家族 DiffReview 与 EvidenceView 在 rail 上没有入口；
  D2、D10、D12、D13、D14 以裸的全窗口屏渲染，丢掉标题栏、rail、Lane 侧栏、输入框与
  状态栏，且没有返回路径。
- **驾驶舱中央区。** 没有 Lane tab strip、没有 `Ctrl+Tab` 切 Lane；工具块没有内联
  diff、没有有序的 user/assistant 行（GUI-CORE-009）；没有专注模式；命令面板没有
  "新建 Lane"动作、文件行是空操作；"Full setup"去了 D11 而不是 D4 Lane 向导；
  `Cmd+L`、`Cmd+G`、`Cmd+.` 未绑定。
- **上下文坞。** 五段静态键值，而设计是 tab（Environment、Files、Diff、Code、Docs）、
  可点击的 Changes 行、Commit-or-push 行、PR 状态、预算条与检视面板。Files 与 Diff
  只需要已存在的能力。
- **Lane 侧栏。** 仅有开关；设计的固定/浮动双模式、悬停探出与持久化（`D-SIDEBAR`）
  缺失。
- **Core 阻塞项。** 027、会话队列、028 与审计持久化、009、原生回合活性事实、按 Lane
  的源状态视图、open-file 命令。

## 目标

1. **rail 即路由器，做完整。** 每个已登记目的地都可从 rail 到达并在驾驶舱 chrome 内
   渲染，且有返回路径，与 `D-RAILNAV` 和 D1 旗舰页一致。
2. **中央区对等。** Lane tab strip、切 Lane、带内联 diff 的工具块、面板创建 Lane、
   设计登记的快捷键、专注模式。
3. **上下文坞对等。** 先用 Core 已发布的事实支撑 tab 与分节，再接入新的文件读取事实。
4. **无 Lane 也能交付。** 工作区级操作者身份，让提交与推送按设计从驾驶舱直接可用。
5. **真实的队列、证据与审计。** 排队的后续输入会执行；原生或 ACP 的变更带规范字节进入
   持久证据归档；审批决定是持久审计行。
6. **真实任务完成。** 在 GUI 中原生重跑 `0.3.3` 任务，从接入到推送被接受，归档与审计非空。

## 对控制计划的范围修订

日期 2026-09-12；作为注记写入 `docs/parallel-development-plan.md`。用户可否决任意一条。

- **生产发布门禁移至 `0.3.5`。** 驾驶舱与设计拉齐之前不启动打包、公证、Homebrew 与
  实盘供应商认证；认证现在的驾驶舱等于认证上述差距。
- **Plan Studio 与 Agent Board 移至 `0.3.5`**，理由与它们移出 `0.3.3` 时相同：没有
  登记的 D 屏、没有契约。
- **二级屏改为驾驶舱内视图。** D2、D10、D12、D13、D14 在中央区渲染并保留全部 chrome，
  与 DiffReview、EvidenceView 相同；`?screen=` 深链接保留为入口，打开时驾驶舱直接停在
  该视图。这是对差距评审遗留问题的裁定，取 D1 旗舰页自身的行为。
- **D5 画廊评审不纳入。** 设计标为 active，但 Core 没有画廊或变体模型；作为
  GUI-CORE-029 开给 `0.3.5`。
- **沿用 `D-RAILNAV`：** WorktreeBoard 放弃；内嵌子代理树、DiagnosticsView、DockSD、
  弹出窗口、D7、D8、D9 与 Pip 留在本里程碑之外。

## 组件版本目标

| 线 | 现在 | `0.3.4` 目标 |
| --- | --- | --- |
| Core | `0.3.6` | `0.3.7` 不可变检查点，schema `1`，六项可加能力（23 → 29；2026-09-12 自 28 修订，见契约设计） |
| TUI | `0.3.4` | `0.3.5`，新事实的对等最小集 |
| GUI | `0.1.0-rc.4` | `0.1.0-rc.5`，驾驶舱与 D1 拉齐 |

## 批次

每个批次一个 `.worktrees/` 下的工作树，测试先行，合并前按简报评审。Core 批次先于消费
它们的客户端批次落地；契约设计给出精确形状。G3–G6 不依赖 Core 变更，与 Core 并行立即
开始。

| 批次 | 归属 | 内容 | 依赖 |
| --- | --- | --- | --- |
| CD 契约设计 | 主会话 | `release-0.3.4-contract-design{,.zh-CN}.md`：C5–C9 的精确形状 | 本计划被接受 |
| C5 `runtime.workspace_owner` + `ui.layout_preferences` | Core | Core 在打开时铸造 `workspace_id`/`project_id` 并发布 `WorkspaceRuntimeOwnerBound`；`SourceTarget::Workspace` 的 `RunOperatorGitAction` 在该身份下授权与审计；按 Lane 工作树的 `LaneSourceUpdated`；独立的 `UiLayoutPreferences` 记录（`lane_sidebar_mode`、隐藏的状态栏段），因为 `UiPreferences` 加字段会移动全部基础摘要（2026-09-12 修订）；两个 fixture；关闭 GUI-CORE-027 | CD |
| C6 `runtime.turn_lifecycle` | Core | 原生与 ACP 回合带 owner 的 `TurnStarted`/`TurnFinished`；`TurnFinished` 时 settle `assistant_stream`；`TurnFinished` 时排空会话级队列并发布 `InputDequeued`；fixture；关闭 E1 缺陷 1 与兼容性后续项 3 | CD |
| C7 `runtime.durable_work_evidence` | Core | 已应用的原生变更与 ACP 补丁各自产生带规范 ContextStore 字节与 `source_hash` 的归档 `patch` 行，经归档重建所依赖的 `runtime_projection` 行持久化；审批决定写为持久审计行；fixture；关闭 GUI-CORE-028 与 E1 缺陷 4 | C6 |
| C8 `runtime.transcript_rows` | Core | 在既有转录页之上的 owner 作用域有序类型化行（user、assistant、tool call、tool result、check run）；fixture；关闭 GUI-CORE-009 | C6 |
| C9 `runtime.workspace_file_reads` | Core | `ReadWorkspaceFile { path, byte_limit }` → `WorkspaceFileLoaded { content: Text \| Binary \| Unavailable }`，以 `read_file` 工具规格门控；fixture | C5 |
| G3 导航壳 | GUI | 按设计的 rail 集合，D2/D10/D12/D13/D14 作为保留 chrome 的中央区视图并有 Close/`Esc` 返回；DiffReview 与 EvidenceView 的 rail 入口；`Cmd+G` 到决策中心；Lane 侧栏固定/浮动与悬停探出，C5 落地后经 `lane_sidebar_mode` 持久化、之前仅内存保持；状态栏配置齿轮 | 无 |
| G4 中央区 | GUI | Lane tab strip；`Ctrl+Tab`/`Ctrl+Shift+Tab`；面板"新建 Lane…"动作与 `Cmd+L`；"Full setup…"到 D4，并把 D4 向导按设计四步对齐到 Core 已建模的范围（角色、agent、技能包、变更策略；执行目标仅本地）；由 `WorkspaceChangeView.diff` 生成的带内联 diff 工具块与检查运行块；专注模式 `Cmd+.` | 无 |
| G5 上下文坞 | GUI | Environment/Files/Diff 三个 tab；Environment 分节：Changes（行点开 DiffReview 对应文件）、Commit or push（路由到 DiffReview 与 sync chip）、预算条、Sources；Diff tab 来自 `QueryWorkspaceDiff`；Files tab 来自 `QueryWorkspaceFiles`（C9 前仅树）；MCP 状态只以"不可用 + 原因"呈现 | 无 |
| G6 二级视图 | GUI | D10 的 Attach（选中 Lane）与 Stop；D13 节点下钻到 Lane；D14 客户端的执行者/时间筛选与汇总；D2 在 rail 徽标上显示队列数 | G3 |
| G7 消费者 | GUI | 无 Lane 的提交栏与 sync chip（C5）；输入框队列真相与回合活性（C6）；EvidenceView 归档行与 D14 审批行（C7）；有序转录行（C8）；Files tab 内容、面板文件行、Code tab（C9）；登记册关闭 009、027、028 | C5–C9，G3–G5 |
| T2 TUI 对等 | TUI | 在 Core 发布的身份下的 `/git` 工作区目标；原生回合活性替换客户端窗口；证据检视器显示归档补丁；带原因的回归基线 | C5–C7 |
| H2 卫生 | GUI、Core | E1 缺陷 6（HashMismatch 旁的 verified）、7（面板 Escape 后进程退出，先复现）、8 与 9（欢迎页布局与措辞）；D10 `eventsUnavailable` 文案；严格应用静默丢弃二进制 | 无 |
| E2 发布证据 | 全部 | 屏幕解锁后在 GUI 中原生重跑 `0.3.3` 真实任务：接入、Lane、带 hunk 的审批编辑、检查运行、有 Lane 与无 Lane 的 DiffReview 提交、推送先拒绝再接受、非空归档与审计；双语检查点；记录 Core `0.3.7` 检查点与客户端版本 | 以上全部 |

派发顺序：CD 先行（主会话）。然后 C5 与 G3、G4 并行；C6 与 G5 并行；C7、C9 与 G6
并行；C8；随后 G7、T2，H2 在评审者空闲时穿插；E2 最后。本地集成分支
`claude/int-0.3.4`；`main` 仅在明确 push 时移动。

## 门禁

每批次先跑最小相关检查，再跑 `docs/parallel-development-plan.md` 的分支门禁。命令与
`0.3.3` 计划相同；新增：

- 九个冻结的 `frontend-contract-v1` 基础 fixture 逐字节不变；`UiPreferences` 字段为
  可加字段，若任一基础摘要移动，则改为独立偏好记录发布；
- 能力计数在 `scripts/tui-regression.sh` 与 `apps/tui/src/tui/client.rs` 中从 23 移到
  29，附跟踪注释；
- 每个驾驶舱内视图都有 qa harness 状态、双主题 PNG 与 `EVIDENCE.md` 行；每个 rail 目的地
  都有测试证明切换后 chrome 仍在且 `Esc` 返回；
- plugin-host crate 的 `context_reducer_process_*` 测试与 agents crate 的 ACP/Codex 作业
  计时测试串行运行（`--test-threads=1`）；两者在本机任何并行运行下都会抖动（C8 证明
  agents 的抖动在集成基线上就已存在）。

## 完成判据

只有以下各项全部在 `main` 上成立，`0.3.4` 才算完成：

- Core `0.3.7` 记录为不可变检查点，含 29 项能力、六个新扩展 fixture，基础 fixture 字节不变。
- 设计中未放弃、未延期、非路线图的每个 rail 目的地都在驾驶舱 chrome 内渲染并有返回路径，
  两个 `0.3.3` 家族有 rail 入口。
- 中央区有 Lane tab strip、切 Lane、面板创建 Lane、`Cmd+L`/`Cmd+G`/`Cmd+.` 与内联工具
  diff；上下文坞有由 Core 事实支撑的 Environment、Files、Diff 三个 tab。
- 未选中 Lane 时，在 Core 发布的工作区身份下从驾驶舱完成一次提交与一次推送。
- 排队的后续输入会执行；已应用的原生编辑在 EvidenceView 中以内容已校验的 `patch` 行出现；
  允许它的审批出现在 D14。
- 登记册带日期关闭 009、027、028；013、018、019、021、023、026、029 带 `0.3.5` 注记。
- E2 证据文档双语存在，含每一步的原生 GUI 截图；兼容性后续项关闭或改期。

明确不属于完成范围：打包、公证、Homebrew、实盘供应商认证、Plan Studio、Agent Board、
D5、D7、D8、D9、DockSD、Diagnostics、弹出窗口、多工作区监督（023）、forge 状态（021）。

## 风险

- **保留 chrome 的视图会牵动 D 屏测试。** D2/D10/D12/D13/D14 各有为全窗口渲染写的投影
  测试与 vitest。G3 必须把既有屏渲染器托管进中央区而不是重写，以保住这些测试。
- **铸造工作区身份触及所有非 Lane 命令。** C5 改变了 `RuntimeOwner::default()` 在线上的
  含义；契约设计必须证明九个携带空 id 的基础 fixture 仍逐字节回放一致。
- **原生路径的回合生命周期是新的 Core 状态。** C6 是监督器首次为内建回合发布终止事实；
  设计必须说明崩溃或取消的回合如何处理，队列绝不能排空进一个已死的会话。
- **持久证据给原生回合加了实时事件。** C7 在原生变更上新增的 `EvidenceRecorded` 必须对照
  冻结 fixture 检查；它们是静态 JSON，只有重生成才会移动，检查的是"不需要重生成"。
- **原生 GUI 运行需要解锁的屏幕。** 主机会话锁定时，E2 与任何 G 批次的实机检查停下而不是
  发送按键。
- **plugin-host 抖动。** 已知且无关；串行重跑是规则，不是通过。
