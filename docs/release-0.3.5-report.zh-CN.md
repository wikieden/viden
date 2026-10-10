# Viden 0.3.5 完成报告

English version: [release-0.3.5-report.md](release-0.3.5-report.md)

日期：2026-10-10。由 [release-0.3.5-plan.zh-CN.md](release-0.3.5-plan.zh-CN.md) 的生产
发布门禁（批次 R1）撰写。

**本报告中的任何内容都没有被推送，也没有任何内容被发布。** `0.3.5` 候选是本地集成分支
`claude/int-0.3.5`，加上 R1 在 `claude/release-gate-r1` 上的三个发布步骤提交。`main`
已于 2026-10-10 作为计划中途的快照快进到 `0dcfaa66`，没有版本提升；候选只有在用户明确
说「push」时才会进入 `main`。R1 没有获得任何对外发布步骤的授权，因此一个都没有运行：
没有 live provider 认证、没有签名或公证、没有工作流分发、没有 tag、没有 GitHub Release、
没有 Homebrew 改动。

## 候选

| 产品线 | 版本 | 位置 |
| --- | --- | --- |
| 工作区候选 | `0.3.5` | `claude/int-0.3.5` 的 `d522e4f38f7643d0e1dd37d70af5e84c43f59765`（截至 C19 与 E3e 的每个批次），加上 `claude/release-gate-r1` 上 R1 的提交，等待推送 |
| Core | `0.3.8` 不可变契约 checkpoint | checkpoint SHA `d522e4f38f7643d0e1dd37d70af5e84c43f59765`；schema `1`；15 个冻结基础 + **33** 个扩展能力；基础契约 checkpoint `5bd2b80b…` 自 `0.3.0` 起未变 |
| TUI | `0.3.6` | `min_core_version` 有意保持 `0.3.4`：四个新能力都是独立协商的功能门，没有一个会阻止只具备基础 schema-1 的客户端启动 |
| GUI | `0.1.0-rc.6` | `[core].minimum_version` 出于同样原因保持 `0.3.5`；已按此版本重建 macOS arm64 `.app`。**没有提升到 `0.1.0`**：计划规定只有生产门禁完整通过才提升，而它没有（退出标准 6 与 7，以及每一个对外步骤） |
| Workspace / CLI | `0.1.30` | 未变；`0.3.5` 的 GitHub Release 需要先做版本决定（见「发布身份」） |

checkpoint 在这里一次性声明，针对的是每个批次落地其上的集成分支末端，而它指名的是本地
分支上的一个提交：**如果 `claude/int-0.3.5` 在合并前被 rebase，checkpoint 必须重新
声明**，而不能假定它自动存续。声明见
[core-0.3-compatibility.zh-CN.md](core-0.3-compatibility.zh-CN.md) 的「Core 0.3.8 契约
checkpoint」。

## 交付了什么

四个增量式 Core 能力，把对外通告的扩展集合从 29 推到 33，九个冻结基础 fixture 字节未变：

| 批次 | 能力 | 客户端 |
| --- | --- | --- |
| C13 | `runtime.plan_documents` —— 计划文档作为 Core 事实，带操作者决定，以及移交到 Core 既有 DAG 的 Plan → Build | GUI G8（规划工作台，D15）、TUI T4 |
| C14 | `runtime.agent_session_control` —— 与取消并列的暂停、恢复与终止 | GUI G9（Agent 看板，D16）、TUI T4 |
| C15 | `runtime.approval_scopes` —— 带可撤销常设规则的「始终」与「编辑」决定（登记 019） | GUI G9、TUI T4 |
| C15 | `runtime.starter_lane_agent` —— 起始 Lane 请求上的 agent 绑定（登记 030） | GUI G9b、TUI T4 |

围绕它们：C12 关闭了兼容性后续项 10、13、14、15 与 16 的会话一半；C16、C17、C18 与 C19
关闭了后续项 19、20、21、16 的 Lane 一半以及 18 的行上限一半；T3 为 TUI 加上了
`set_upstream` 重试；DS 把规划工作台与 Agent 看板作为 D15 与 D16 登记进设计包，DS2 在
token 层修复了浅色皮肤的 `--accent-bright` 对比度；G10 跑了设计保真门；H3 裁定了登记并让
计时测试变得确定；H4–H8 与 E3e 修复了 E3a–E3e 运行发现的原生 GUI 缺陷（下文列出的除外）；
P1 与 P2 建立了打包路径与 GUI 发布工作流。

R1 本身没有改变任何行为。它声明了 checkpoint、移动了三个版本，并发现且修复了一个记账
缺口：Core 的发布清单只通告了 33 个能力中的 30 个，也没有为 C15 的两个 fixture 钉住任何
行，而没有测试察觉。契约测试现在从真源推导这两者（见兼容性文档）。

## 退出标准

| # | 标准 | 状态 | 证据 |
| --- | --- | --- | --- |
| 1 | `0.3.4` 的真实任务通过 GUI 驾驶舱原生取证 | **已达成，原生，附三项限定** | E3e，`claude/int-0.3.5` 的 `3c652fa8`：在 Core 审批下创建 Lane，其 TASK 作为一个轮次运行，`edit_file` 限定在 Lane worktree 内，一条后续输入被确认并被排空（后续项 16 在 GUI 上达成），暂存、提交与推送均在 dock 上决定，Core 的 `no_upstream` 由 `Push and set upstream` 回应并写下四条 `source.push` 行，「始终」/撤销，以及规划工作台的接受 → 移交 → D13。限定来自计划自身的修订：绑定是在启动时通过 `VIDEN_GUI_WORKSPACE` 完成的，而不是 `⌘O`；推送是在重启后的进程里由第二组 `Commit & Push ⇡` 完成的，因为缺陷 43 让第一组的待决推送丢失；E3e 的产物带有在该批次内先红后绿修复的两个 GUI 阻塞（`5afb01b0`、`4c92b511`）。[checkpoints.md，E3e](release-evidence/gui-trusted-delivery/checkpoints.md#native-gui-run--2026-10-10-035-candidate-3c652fa8-e3e) |
| 2 | 后续项 10、13、14、15、16 带日期与 file:line 关闭；Lane 路径的审批先写审计行再发布宣告它的事实 | **已达成** | 五项都于 2026-09-13 由 C12 带 file:line 关闭；16 的 Lane 一半由 C18 关闭，并在 E3e 原生达成。残留：后续项 17（当 worker 的复检拒绝时，Lane 路径的行可能写着「allow」），留待 `0.3.6`。[core-0.3-compatibility.zh-CN.md](core-0.3-compatibility.zh-CN.md) |
| 3 | 规划工作台与 Agent 看板作为已登记的 D 屏，并作为基于 Core 事实的中心窗格视图，无支撑的控件禁用并加标签 | **已达成；Agent 看板未原生取证** | DS 登记了 D15/D16；G8/G9 进入驾驶舱；规划工作台在 E3e 中被原生驱动。Agent 看板的控件从未被原生驱动：本宿主机上没有任何会应答的 ACP 二进制，因此不存在可以暂停或终止的 Agent 会话 |
| 4 | Core `0.3.8` 声明 33 个带 fixture 的能力；九个冻结基础 fixture 与每个 `0.3.4` fixture 与 `edcc7b3d` 逐字节一致 | **已达成，附计划自身的例外** | 九个基础 fixture 一致；每个 `0.3.4` 扩展 fixture 一致，唯一例外是 `interaction-closed-loop` —— C12 依后续项 13 刷新了它的字节（本计划目标 2 要求如此），其视图摘要不变。证明表见下文 |
| 5 | 登记 019 与 030 两侧关闭；013、018、021、023、026、029、031、032、033 关闭或带理由改期 | **部分达成** | 019 与 030 两侧关闭（G9b）。七项由 H3 带理由改期至 `0.3.6`，029 作为保留号写下并改期至 `0.3.6`。**032 保持开放且未排期**：H3 把「集成终端是否在契约范围之外」留给契约设计，而 CD2 没有做这个决定。036 已开启，并作为 TUI 采纳项保持开放。[contract-requests.zh-CN.md](../apps/gui/contract-requests.zh-CN.md) |
| 6 | 设计保真报告对每个界面、两种主题通过，附 CJK、仅键盘、无障碍与性能记录 | **部分达成** | 无头门禁对 22 个界面 × 2 组通过。四个环节因宿主屏幕锁定而未运行：CJK 输入法、原生仅键盘遍历、发布包性能、原生截图。F2 由 DS2 在 token 层关闭；F5、F6、F7 已提交给设计包。[release-0.3.5-fidelity-gate.zh-CN.md](release-0.3.5-fidelity-gate.zh-CN.md) |
| 7 | 三个平台的产物带校验和；每个对外步骤要么在授权下完成，要么记录为未运行 | **部分达成** | 只有 macOS arm64 存在：按 `rc.6` 重建的 `.app`（二进制 SHA-256 `efff1259…c53a`）。macOS x86_64、Windows 与 Linux 需要在已推送的 ref 上获得授权的 `release.yml` 分发。每个对外步骤都在下文记录为未运行，并写明缺失的授权。[release-0.3.5-packaging.zh-CN.md](release-0.3.5-packaging.zh-CN.md) |
| 8 | 双语完成报告，说明推送状态并指名 GUI 获得的版本 | **已达成** | 本文档。没有任何推送；GUI 获得的是 `0.1.0-rc.6` |

## 原生证明，E3a 到 E3e

读自 [checkpoints.md](release-evidence/gui-trusted-delivery/checkpoints.md)：

| 运行 | 候选 | 到达了什么 |
| --- | --- | --- |
| E3a（2026-09-13） | `main` `75e2a84a`（= `0.3.4`） | 工作区一半 —— 绑定、暂存、提交、推送、审计、文件读取；无法创建任何 Lane（缺陷 14–16） |
| E3b（2026-09-13） | `da4c8a6d` | Lane 创建及其审计行、Lane 作用域的暂存与提交、规划工作台、Agent 看板；Lane 的轮次被阻塞（缺陷 22） |
| E3c（2026-09-14） | `22124903` | 缺陷 22 原生关闭；带类型化 hunk 的工具审批、归档补丁、「始终」与「编辑」；被 28、29、30 阻塞 |
| E3d（2026-09-14） | `71d5156a` | 28、30、32、33、34 原生关闭；Lane 作用域的暂存始终到不了可决定的界面（缺陷 35） |
| H7、H8（2026-09-14） | int | 35 与 36 的客户端一半修复并原生确认；25（`git_push` 一半）由 H8 原生关闭 |
| **E3e（2026-10-10）** | **`3c652fa8`** | **完整的真实任务，原生完成**，附退出标准 1 下的三项限定 |

## 候选上仍未关闭的项

**GUI 缺陷**（编号见 checkpoints 文档，除另有说明外均属 GUI）：39（停在审批上的轮次无法
取消）、40（选择 Lane 后上下文 dock 保留上一条 Lane 的样本）、43（`Lanes` rail 可能在
活跃连接之上渲染 hydrate 之前的 `CONNECTING` 外壳）、44（在 1280×800 下规划工作台的决定
行被输入框遮挡）、45（切回 Build 后规划工作台保留 `plan_mode` 拒绝，直到重新打开）、
46（新建的健康 Lane 显示另一条 Lane 的 `AGENT STOPPED` 界面；归属未定，GUI 或 Core）。

**Core 缺陷 42**：已提出的计划在 Core 重启后不复存在，尽管 `plan_document_updated` 仍在
`agents.jsonl` 中。记为兼容性后续项 **22**，R1 中不修复 —— 在声明 checkpoint 的步骤中
修复 Core 行为会让该 checkpoint 失效。

**仍开放的兼容性后续项**：17（见上）、18（其行上限一半已由 C19 关闭 —— 该测试现在能区分
期限终止与行上限；它的第二个测试，即重复超时用例，没有改动，因为其重采样循环本就吸收了
首次调用的停顿，并且它在 C19 的每次运行以及 R1 的并行套件中都通过了），以及 22（见上）。
没有开放的后续项 20：证据日志中的缺陷 22 不属于 Core，而后续项 20（Lane 的工具根目录）
已关闭。

**开放登记**：013、018、021、023、026、031、032、033、036 —— 九条，rc.6 清单中
`open_count = 9`；029 是保留号，不计入。

**环境。** 在这台 macOS Darwin 27 宿主机上，全套件负载下首次执行一个刚写出的脚本会停顿
约 3 秒（推断为 syspolicyd，未证实）；产品真正使用的 `git` 不受影响。C19 让唯一与之竞争
的测试区分期限终止与行上限，完整工作区套件以并行方式、不排除任何 crate 运行并通过（见下）。
`rustup` 缺席，因此本地不存在 x86_64 产物。宿主屏幕在 E3e 期间解锁，在 G10 期间锁定。

## 冻结 fixture 证明

在 R1 分支上，对每个文件运行 `shasum -a 256` 并与 `git show edcc7b3d:<path>` 比较：

| 集合 | 结果 |
| --- | --- |
| 九个冻结基础 fixture（`stream-tool`、`approval-allow-deny`、`queued-follow-up`、`dag-blocker`、`multi-lane`、`merge-gate`、`context-pressure-cost-blind`、`plan-denial`、`d1-vertical-slice`） | **全部一致**，且等于 `apps/tui/release-manifest.toml` 中的摘要 |
| `edcc7b3d` 时已存在的其他每个 fixture（22 个文件，含 `legacy-lanes.tsv`） | 一致，唯一例外是 `interaction-closed-loop.json`：`a6f1c436…` → `c2ea4647…`（C12，后续项 13；视图摘要 `c43d9fe3…` 不变） |
| `0.3.5` 新增 | `plan-documents`、`agent-session-control`、`approval-scopes`（C16 刷新）、`starter-lane-agent` —— 每个都钉在 `crates/core/release-manifest.toml` 中，并由 `frontend_contract_v1.rs` 推导 |

## Gate

在 `claude/release-gate-r1` 上声明 checkpoint 的那个提交处离线运行。

| 命令 | 结果 |
| --- | --- |
| `cargo test --workspace --quiet`（并行，不排除，无 `--test-threads`） | 通过，退出码 0，**2343 通过，0 失败**，30 忽略，89 行结果，192 秒 |
| `cargo fmt --all -- --check` | 通过，退出码 0 |
| `cargo clippy --workspace --all-targets` | 通过，退出码 0，**零警告** |
| `scripts/check-dependency-boundaries.sh` | 通过，退出码 0 |
| `scripts/tui-regression.sh` | 通过，退出码 0；证据位于 `target/tui-regression/0.3.6/` |
| `scripts/tui-turn-controller-smoke.sh` | 通过，`TUI TurnController smoke passed` |
| `scripts/rc-tui-stability-smoke.sh` | 通过，退出码 0 |
| `scripts/tui-previews.sh` | 通过，退出码 0（其默认目录仍是 `target/tui-previews/0.3.4/`，这是该脚本自 `0.3.4` 起带着的过期标签） |
| `npm --prefix apps/gui test -- --run` | 通过，**70 个文件 / 982 个测试** |
| `npm --prefix apps/gui run build` | 通过（`tsc --noEmit && vite build`） |
| `cargo test -p viden-gui` | 通过，46 个套件，**392 通过**，1 忽略 |
| `cargo test -p viden-gui --test capture_projections -- --ignored` | 通过，1 |
| 迁移（`frontend_contract_v1_migrations_are_idempotent_before_fixture_replay`） | 通过，1 |
| `npm --prefix apps/gui run tauri -- build --bundles app` | 56 秒内通过，`0.1.0-rc.6` |
| 对每对改动的 Markdown 运行 `scripts/check-doc-pairs.sh`、`scripts/check-doc-links.sh`；`git diff --check` | 通过 |

R1 自身的先红证据：先改测试时，`frontend_contract_v1` 因版本失败（3 个测试），
`architecture_boundary` 因 rc.6 版本、缺失的 rc.6 快照以及被改过的 rc.5 快照
（`e3873be0…` 对比发布时的 `b2177792…`）失败，TUI 清单测试因 `0.3.5` 失败；只移动 Core
版本时，契约测试在新增的能力行断言处失败。改动之后全部转绿。

### Prepublish 门禁

`scripts/release-gate.sh --version 0.1.30 --phase prepublish` **拒绝运行**，退出码 2，
`DEEPSEEK_API_KEY is required for release prepublish gate` —— 密钥不存在，也没有收到
「go」。随后按其顺序逐个运行了它的各个环节：

| 环节，按 `release-gate.sh` 顺序 | 是否运行 | 结果 |
| --- | --- | --- |
| `scripts/check-task10-guards-test.sh` | 是 | 通过，`Task 10 guard fixture tests passed.` |
| `scripts/check-dependency-boundaries.sh` | 是 | 通过 |
| `scripts/check-doc-pairs.sh`，覆盖门禁指定的十二个文件 | 是 | 通过 |
| `scripts/check-doc-links.sh`，覆盖门禁指定的四个文件 | 是 | 通过 |
| `scripts/context-engine-benchmark.sh` 确定性，3 次运行 | 是 | 通过，`Result: passed`，输入 token 中位数 off `1000` / on `750`（减少 `25.00%`） |
| `scripts/release-smoke.sh --version 0.1.30 --target aarch64-apple-darwin`，不带 `--deepseek` | 是，393 秒 | **17 通过，5 跳过**：`cargo-fmt`、`tdd-testing-contract-smoke`、`tui-turn-controller-smoke`、`rc-tui-stability-contract-smoke`、`final-zero-bug-contract-smoke`、`cargo-clippy`（`-D warnings`）、`viden-terminal-entry-tests`、`workspace-tests`（串行）、`tui-regression`、`fallback-cli-smoke`、`plan-mode-smoke`、`daily-loop-smoke`、`codex-app-server-protocol-fixture`、`codex-app-server-write-guard`、`lane-operator-loop-smoke`、`rc-tui-stability-smoke`、`package-smoke`（`viden-v0.1.30-aarch64-apple-darwin.tar.gz`，10,268,799 B，SHA-256 `ef9e3fa32c41fa781e162c80240a542c9fd103f6cb21e24e7b8665e6e13e3182`，二进制报告 `viden 0.1.30`） |
| ↳ `deepseek-dev-scenario-smoke`（门禁会传 `--deepseek`） | **拒绝** | 未运行 —— 缺少授权：没有 `DEEPSEEK_API_KEY`，没有「go」 |
| ↳ `provider-live-smoke`、`github-actions-release-validation`、`github-release-assets-validation`、`homebrew-validation` | 按参数跳过 | 不属于本门禁的 prepublish 环节；后三项是未获授权的对外步骤 |
| `scripts/final-zero-bug-smoke.sh`（因版本为 `0.1.30` 而必需） | 是 | **失败，退出码 2**：`VIDEN_TUI_MANUAL_EVIDENCE_DIR is required for the final zero-bug gate`。该环节要求至少一张由人拍摄的 macOS Terminal 与一张 iTerm2 的 TUI 截图；本候选没有，而 agent 无法在锁定的屏幕上拍摄。这是证据缺口，不是授权缺口 |
| `scripts/context-engine-benchmark.sh --provider deepseek`（live A/B） | **拒绝** | 未运行 —— 缺少授权：没有 `DEEPSEEK_API_KEY`，没有「go」 |
| 迁移（契约的先迁移后重放测试） | 是 | 通过，1 个测试，且包含在两次工作区运行中 |
| 完整工作区 | 是 | 两次通过：并行（2343 通过）与 `release-smoke` 内的串行 |

因此 prepublish 门禁**没有通过**：两个环节因缺少授权被拒绝，一个环节（`final-zero-bug`）
因缺少人工拍摄的 Terminal 与 iTerm2 证据而失败。其余每个环节都通过。

## 发布身份

2026-10-10 的只读 `gh` 调用发现：最新的 GitHub Release 是 `v0.1.30`，标题为
"RoboCode v0.1.30"，资产为 `robocode-v0.1.30-*`；`wikieden/homebrew-tap` 只有
`Formula/robocode.rb`。发布门禁接收的是 workspace/CLI 版本（`0.1.30`），其 postpublish
环节期望 `viden-v*` / `Viden*` 资产与 `wikieden/tap/viden` formula，因此 **在 `0.1.30`
上运行 postpublish 两个环节都会失败**。`0.3.5` 发布首先需要一个版本决定（新的 workspace
版本，或为三条产品线采用另一种 tag 方案），以及一次新增或重命名 formula 的 tap 改动。
详见 [release-0.3.5-packaging.zh-CN.md](release-0.3.5-packaging.zh-CN.md) 的「R1」。

## 对外步骤

每一行都是 **未运行 —— 缺少授权**。每一项都需要在运行之时由调度方转达的、属于它自己的
「go」。

| 步骤 | 「go」之外的前提 | 状态 |
| --- | --- | --- |
| live provider 认证（`release-smoke --deepseek`、DeepSeek A/B 上下文基准） | 环境中有 `DEEPSEEK_API_KEY` | 未运行 —— 缺少授权；密钥不存在 |
| 推送候选（合入 R1 后的 `claude/int-0.3.5`，然后 `main`） | 用户明确的「push」 | 未运行 —— 缺少授权 |
| 以 `upload_to_release=false` 分发 `release.yml`（macOS x86_64、Windows、Linux 的 CLI 与 GUI 产物） | 已推送的 ref；`gh` 现已认证 | 未运行 —— 缺少授权 |
| 代码签名（Developer ID） | 宿主机或 Actions secrets 中的身份，`tauri.conf.json` 中的 `signingIdentity` 与 entitlements | 未运行 —— 缺少授权；不存在任何身份 |
| 公证 | Apple ID / Team ID 凭据 | 未运行 —— 缺少授权；没有凭据 |
| 发布单元的版本决定 | 见「发布身份」 | 未做出 —— 属于用户决定 |
| Git tag | 版本决定 | 未运行 —— 缺少授权 |
| 带 P1/R1 及分发产物的 GitHub Release | tag 与分发产出的产物 | 未运行 —— 缺少授权 |
| 同一版本的 Homebrew tap（`wikieden/homebrew-tap`） | `viden` formula 与已发布的资产 | 未运行 —— 缺少授权 |
| `scripts/release-gate.sh --phase postpublish` | 发布与 tap | 未运行 —— 缺少授权；按上述原因今天会失败 |

## 状态

**候选位于 `claude/int-0.3.5` 的 `d522e4f3`，R1 的发布步骤提交位于
`claude/release-gate-r1`，等待推送。** 不在 `main` 上（`main` 位于计划中途的快照
`0dcfaa66`），未合并，未打 tag，未发布。下一步安全动作都是决定，依次为：在用户发话后把
R1 合入 `claude/int-0.3.5` 并推送；选定发布版本；然后逐一授予上述对外步骤。
