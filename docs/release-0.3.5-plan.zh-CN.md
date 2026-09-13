# Viden 0.3.5 计划 —— 可信交付加固、Plan Studio、Agent Board 与生产发布门

English version: [release-0.3.5-plan.md](release-0.3.5-plan.md)

`0.3.5` 是 `0.3.4` 之后的聚合工作区里程碑；`0.3.4` 已于 2026-09-13 合入 `main`
（`edcc7b3d`）。`PLAN.md` 与 `docs/parallel-development-plan.md` 把它命名为「视觉保真
与生产发布门」，并自 2026-09-12 起把 Plan Studio 与 Agent Board 也归入其中。本文档保留
该范围并给出顺序：先关闭 `0.3.4` 发布证据发现的信任缺口；把两个已设计但未登记的屏幕先
纳入设计包，再由客户端绘制；最后在一个已经原生验证过的候选上跑生产门。Core 契约内容
将在本计划被接受后写入 `release-0.3.5-contract-design.md`。

状态：计划写于 2026-09-13，依据 `0.3.4` 完成报告、它遗留的兼容性后续项与注册表。本文档
中没有任何内容已实现。每个批次单独派发、在主会话中做对抗式评审、仅在明确 push 时合入。

## 基线

2026-09-13 的 `main` @ `edcc7b3d`：

| 线 | 版本 | 状态 |
| --- | --- | --- |
| Core | `0.3.7` 不可变检查点 `39ed155d` | 29 项扩展能力，schema `1`；九个冻结基线 fixture 自 `0.3.0` 以来未变 |
| TUI | `0.3.5` | 对 `0.3.4` 全部能力的对等；整个真实任务在此端到端跑通，包括未选 Lane 时的 commit 与被接受的 push |
| GUI | `0.1.0-rc.5` | 驾驶舱已与 D1 设计拉齐：rail 即路由、驾驶舱内 D2/D10/D12/D13/D14、Lane 标签条、分页上下文坞、六项 `0.3.4` 能力的消费者；macOS `.app` 可构建 |

`0.3.4` 报告原样列出的未完成项：

- 原生 GUI 截图集不存在——发布步骤全程主机屏幕锁定，因此目标 6 在契约与 TUI 上达成，
  在 GUI 驾驶舱上未证明；
- 兼容性后续项 10、13、14、15、16，以及 TUI 缺失的 `set_upstream` 控件；
- 开放注册表：013、018、019、021、023、026、029（为 D5 画廊评审保留）、030、031、032、033；
- 生产发布门本身：`scripts/release-gate.sh --phase prepublish` 在没有 `DEEPSEEK_API_KEY`
  时拒绝启动；任何 `0.3.x` 线都未跑过打包、签名、公证、GitHub Release 或 Homebrew 步骤。

## 目标

1. **先原生证明 `0.3.4`。** 在屏幕解锁时，对 `main` @ `edcc7b3d` 用 GUI 驾驶舱跑一次真实
   任务，作为缺失的 E2 原生段记录。在该证据存在之前，不落任何 `0.3.5` 客户端改动，
   以便证明可归属于已发布之物。
2. **关闭信任缺口。** Lane 路径上的审批决定留下持久审计行（后续项 15，阻断级）；会话
   跟进在命令被接受时即被确认进入 Core 队列，而不是等 worker 取到时（16）；归档补丁的
   文档指名其文件与改动类型（14）；Agent 上报的补丁持久规范化（10）；刷新
   `interaction-closed-loop` 的字节与摘要（13）；TUI 提供 Core 自己的拒绝所指名的
   `set_upstream` 重试。
3. **先在设计包登记 Plan Studio 与 Agent Board，再在驾驶舱内构建。** 两者都没有登记的
   D 屏；功能设计（`docs/gui-version-functional-design.md` 第 3、4 节）是唯一来源。设计包
   先按其自身规则获得这两个屏幕；GUI 再把它们作为保留 chrome 的中央面板视图承载，与
   `0.3.4` 承载 D2/D10/D12/D13/D14 的方式完全一致。
4. **给这两个屏幕提供所需的 Core 事实**，以附加式能力交付：带 Plan → Build 移交的计划
   文档；在现有 cancel 之外的 Agent 会话控制（暂停、恢复、终止）。顺路关闭两条解除 D4
   诚实性阻塞的注册项：030（starter-Lane 请求上的 agent 绑定）与 019（Always 与 Edit
   审批决定）。
5. **通过设计保真门**：每个驾驶舱表面在双主题下与设计包的截图与组件对等、CJK、纯键盘、
   无障碍、有界性能记录。
6. **把生产发布门作为一个发布单元跑通**：三平台打包、在有凭据处签名与公证、真实
   provider 认证、GitHub Release、同版本验证的 Homebrew tap。每个对外可见的步骤都在执行
   当下由用户授权，绝不从本计划推定。

## 范围决定

- **原生证明在任何 `0.3.5` 改动之前对 `main` 运行。** 若屏幕持续锁定，里程碑不因此
  阻塞，证明改在候选上进行；报告写明是在哪个 SHA 上取得的。
- **设计先于客户端。** Plan Studio 与 Agent Board 先在 `docs/viden-design/Viden/` 绘制
  （zh 为主、包守卫、按其 `AGENTS.md` 更新状态与变更日志）。GUI 批次若发现设计缺失即停止
  并报告，不臆造屏幕。
- **Agent Board 只展示 Lane/会话层级。** 子代理树仍然延期（`D-RAILNAV` ④）。暂停与终止
  是新的 Core 命令，不是 cancel 的别名；Core 未发布的控件按 `0.3.4` 惯例置灰并标注。
- **Plan Studio 由 Core 事实驱动。** Plan 模式已经拒绝变更并发布计划拒绝事实；新能力把
  计划文档（步骤、状态、已接受的移交）作为事实加入。客户端绝不保存 Core 不知道的计划。
- **发布门由用户逐步把关。** 构建 bundle 与跑离线段不需授权。真实 provider 认证
  （`DEEPSEEK_API_KEY`）、代码签名、公证、派发 `release.yml`、发布 GitHub Release、触碰
  Homebrew tap，每一步都需要针对该步骤的明确「go」；批次在缺少授权处停止并报告。
- **GUI 版本**：候选为 `0.1.0-rc.6`。仅当生产门完整通过才提升为 `0.1.0`；否则以 `rc.6`
  交付，门的失败行成为 `0.3.6` 的待办。
- **`0.3.5` 不做**，在卫生批次中裁定并重新标注日期而非静默顺延：018（检查点捕获与恢复）、
  021（forge 与 PR 状态）、023（多工作区监督）、026（平台凭据接入）、029（D5 画廊）、
  031（skill pack）、032（终端 PTY 事实，DockSD roadmap）、033（工作区文档事实）；
  DockSD、Diagnostics、D7/D8/D9、弹出窗口与 Pip 依设计包保持 roadmap 或延期。

## 版本目标

| 线 | 从 | 到 |
| --- | --- | --- |
| Core | `0.3.7`（29 项能力） | `0.3.8`，四项附加能力（目标 33），不可变检查点在发布步骤中一次性声明 |
| TUI | `0.3.5` | `0.3.6`，新事实的对等下限加 `set_upstream` 控件 |
| GUI | `0.1.0-rc.5` | 候选 `0.1.0-rc.6`；生产门完整通过则为 `0.1.0` |

## 批次

所有权沿用 `0.3.4`：Core 批次在 `protocol.rs`、`runtime.rs`、清单与计数常量上串行；
GUI 批次在 `d1_cockpit.ts` 与 `main.ts` 上串行；设计批次独占 `docs/viden-design/**`。
brief 位于派发者的 scratchpad；每个批次返回带逐字检查尾部的原始报告。

| 批次 | 所有者 | 交付 | 依赖 |
| --- | --- | --- | --- |
| E3a `0.3.4` 原生证明 | 证据 | 对 `main` `edcc7b3d` 用原生 GUI 窗口跑真实任务，每步截两次并以无障碍树为准读取，追加为 `0.3.4` 检查点文档的 E2 原生段；屏幕锁定时诚实停止 | 主机屏幕解锁 |
| CD2 契约设计 | 主会话 | `release-0.3.5-contract-design{,.zh-CN}.md`：`runtime.plan_documents`、`runtime.agent_session_control`、`runtime.approval_scopes`（019）、`runtime.starter_lane_agent`（030），含类型、语义、fixture、客户端消费者、簿记、已定默认值，以及加固项的精确规则 | 本计划被接受 |
| C12 可信交付加固 | Core | 后续项 15、16、14、10、13 以红先测试关闭；无新能力；`0.3.4` fixture 与九个冻结基线 fixture 字节不变 | CD2 |
| T3 TUI 加固 | TUI | `/git` 选择器的 `set_upstream` 重试控件；输入框队列文案读取已确认队列（16）；带原因的回归基线 | C12 |
| DS 设计包 | 设计 | Plan Studio 与 Agent Board 在 `docs/viden-design/Viden/` 登记为 D 屏（页面、DESIGN-REF 登记、SPEC 决定、状态与变更日志、包守卫通过）；每个控件映射到 Core 事实或标为请求 | CD2 |
| C13 `runtime.plan_documents` | Core | 计划文档事实（步骤、状态、已接受的移交）、Plan → Build 移交命令、fixture；关闭 Plan View 的 Core 侧 | C12 |
| C14 `runtime.agent_session_control` | Core | Agent 会话的暂停、恢复、终止及其发布的终结事实；fixture；裁定 G6 的草案 | C13 |
| C15 `runtime.approval_scopes` + `runtime.starter_lane_agent` | Core | Always 与 Edit 审批决定（019）；`StarterLaneRequest` 上的 `agent_id` 并在预览与创建时回显（030）；fixture | C14 |
| G8 Plan Studio | GUI | 基于 C13 事实的驾驶舱内 Plan View 与 Plan → Build 移交；qa 状态、双主题 PNG、EVIDENCE | DS、C13 |
| G9 Agent Board | GUI | 基于 Lane/会话层级的驾驶舱内 Agent Board，含基于 C14 事实的暂停/恢复/终止/切换模型控件；D4 第 2 步发送 agent 绑定（C15）；权限坞中的 Always/Edit（C15） | DS、C14、C15、G8 |
| T4 TUI 对等 | TUI | 计划文档透镜、会话控制按键、Always/Edit 决定、`/lanes` 上的 agent 绑定；带原因的回归基线 | C13 至 C15 |
| G10 设计保真门 | GUI | 每个驾驶舱表面对设计包的截图与组件对等、CJK IME、纯键盘遍历、机器可读无障碍、有界性能记录、双主题；保真报告 | G8、G9 |
| H3 卫生 | Core、GUI | 注册表裁定（018、021、023、026、029、031、032、033 重标为 `0.3.6` 或带理由关闭）；把 `viden-plugin-host` 与 `viden-agents` 的计时测试改为确定性以退役串行重跑规则；清掉 clippy 既有告警集 | 无 |
| P1 打包 | 发布 | 通过以 `upload_to_release=false` 派发的 `release.yml` 产出 macOS（arm64、x86_64）、Windows、Linux 的可复现 bundle；校验和；仅在用户提供凭据处签名与公证；记录 bundle 矩阵 | G10 |
| R1 生产发布门 | 发布 | 带真实 provider 段（用户提供密钥）、迁移与全工作区段的 `scripts/release-gate.sh --phase prepublish`，同版本的 GitHub Release 与 Homebrew tap 验证，`--phase postpublish`；Core `0.3.8` 检查点与三条版本提升；`0.3.5` 完成报告 | 以上全部，且每个对外步骤各需一次「go」 |

## 门禁

除特别说明外与 `0.3.4` 相同：

- 每个 Core 批次：crate 套件、工作区套件（`viden-plugin-host` 与 `viden-agents` 串行，
  直到 H3 退役该规则）、`cargo fmt`、零新增告警的 `cargo clippy`、依赖边界、
  `tui-regression`、文档配对与链接、`git diff --check`；九个冻结基线 fixture 与每个
  `0.3.4` 扩展 fixture 字节一致（`shasum` 对比 `git show edcc7b3d:<path>`）；计数常量
  只按批次所加移动并带跟踪注释；
- 每个 GUI 批次：vitest、`tsc` 构建、`cargo test -p viden-gui`、`capture_projections`、
  作者与评审者都看过的 PNG、无小于 30 KB 的帧（错误页特征）、每个目录一个 i18n 尾块、
  诚实原则（缺席不是零、分组不隐藏、置灰必标注）、不在本地持久化 Core 拥有的状态；
- 设计批次：包自身守卫、状态与变更日志规则、zh 为主、不改 `.ref/`；
- E3a 与 R1：每批按键前检查 `CGSSessionScreenIsLocked`；绝不触碰仓库的实时 `.viden/`；
  没有明确「go」不做任何真实 provider、发布、release、签名或 Homebrew 步骤。

## 退出准则

| # | 准则 |
| --- | --- |
| 1 | `0.3.4` 真实任务已通过 GUI 驾驶舱原生证明，或报告写明屏幕持续锁定并指名改为在哪个候选 SHA 上取证 |
| 2 | 兼容性后续项 10、13、14、15、16 带日期与 file:line 关闭；Lane 路径审批在宣告事实之前写入审计行 |
| 3 | Plan Studio 与 Agent Board 在设计包中是登记的 D 屏，在驾驶舱中是保留 chrome 的中央面板视图，基于 Core 事实，每个无后端控件置灰并标注 |
| 4 | Core `0.3.8` 声明 33 项能力并附 fixture；九个冻结基线 fixture 与每个 `0.3.4` fixture 与 `edcc7b3d` 字节一致 |
| 5 | 注册项 019 与 030 双侧关闭；013、018、021、023、026、029、031、032、033 已关闭或带理由重标日期 |
| 6 | 设计保真报告对每个驾驶舱表面双主题通过，附 CJK、纯键盘、无障碍与性能记录 |
| 7 | 三平台 bundle 存在并附校验和；每个对外发布步骤记录为已授权完成，或记录为未运行并指名缺失的授权 |
| 8 | `0.3.5` 完成报告双语存在，说明已 push 与未 push 之物，并指名门所赢得的 GUI 版本 |

## 风险

- **屏幕锁定是环境性且反复发生的。** E3a 排在最前并伺机重试；里程碑不因此阻塞，报告
  绝不声称未曾取得的原生证明。
- **Plan Studio 与 Agent Board 是新表面，不是拉齐。** 设计批次可能发现功能设计规格不足；
  它把决定记入包的 SPEC 而非客户端 brief，GUI 批次以包为准。
- **暂停与终止改变 Agent 会话语义。** 暂停中的 ACP 会话如何处理在途工具调用与排队输入
  是 CD2 必须在 C14 之前定下的 Core 决定；错误的默认值会破坏活动会话。
- **发布门需要本仓库绝不能持有的凭据。** 批次只在运行时从环境读取、不记录任何秘密、
  在缺少密钥处停止。
- **三平台打包从未跑过。** Windows 与 Linux bundle 可能因工具链或 Tauri 配置失败；P1
  诚实记录矩阵，R1 交付已构建成功的部分。
- **计时测试确定性**（H3）触及两个 crate 依赖的测试基础设施；单独运行并以连续三次全工
  作区套件为门。

## 下一步

接受或修订本计划；随后主会话撰写 `release-0.3.5-contract-design{,.zh-CN}.md`（CD2），
在主机屏幕解锁的那一刻派发 E3a，在 CD2 落地后立即派发 C12 与 H3。
