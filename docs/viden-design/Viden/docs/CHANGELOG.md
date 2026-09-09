# Viden 更新日志（Changelog）

> 按**天**记录更新,每条标注涉及的**模块**便于筛选。**最新日期段在最上面**(newest-first)。
> 格式:`- [模块] 变更描述`。一天内可有多条,按模块归类。
> **定档即写**:仅把已定稿的工作写入;草稿 / 试验不记录。模块索引随新模块扩充。
>
> **卫生三条**(由 `tools/check-changelog.js` 机检):
> 1. **同日合并**——写前先 `grep '^## <今天>'`,命中就 append,绝不开第二个同日 `## YYYY-MM-DD` 段。
> 2. **深度上限**——一条 = 1 行标题 + 最多 3 子 bullet;根因 / 踩坑一句话带过,深内容分流到对应 doc。
> 3. **滚动归档**——主文件只留最近约 2 个会话日;超 ~200 行把窗口外**最旧整段(文件底部)**移到 `_archive/CHANGELOG-YYYY-MM.md`,底部留链接。

## 模块索引
- **设计规范** — tokens / 组件 / 展示文档
- **文档** — CLAUDE.md / DESIGN-REF / 本文件
- **TUI** — 终端版屏与组件
- **GUI** — 桌面(Tauri)版屏与组件
- **品牌** — logo / favicon / OG / 图标
- **入口** — index.html 门户 / 启动卡 / screens-status
- **工具** — tools/ guard 脚本（check-tokens / changelog / status）

---

## 2026-09-09
- [GUI][文档] **DiffReview / EvidenceView 升进 gui-kit 的尝试受阻,先记录实测再裁决**（D-SOT · D-RAILNAV ①②;本次**未动任何 CSS**,纯文档）：
  - **D1 不能反向 link `gui-kit.css`**：两边同名规则已有意分叉,D1 只声明部分属性、kit 多出的属性会泄漏（`.envrow .cv` 多 font-family/font-size）→ 实测 chat 视图 **2584 px** 变化;结论写进 PROTO-STANDARD §5,升进只能做成逐字镜像。
  - **`.review` 族与 D2 决策中心自带的 `.dl`/`.diffbody` 撞名**（两个不同组件、只是重名）→ 加进 kit 后实测 D2 **1698 px** 变化,阻塞;裁决项与 EvidenceView 可单独升进（D14/组件库/D1 实测 AE=0）都记进 DESIGN-REF「D1 次级视图」。
  - 顺带更正文档笔误：DESIGN-REF 迁移状态与 CHECKLIST §5 原列「D1 已接 gui-kit」,实物从未 link,两处改为「有意不接」。

## 2026-09-08
- [GUI][文档] **D1 次级面裁决定档：活动 rail = 路由器 + 四族登记**（SPEC 新增 `D-RAILNAV`）：
  - rail 按钮导航到独立 D 屏（D12/D2/D14/D10/D13）= 已接受的实现模型；旗舰 D1 的页内视图切换保留作设计探索。
  - DESIGN-REF 新增「D1 次级视图」节登记四族：DiffReview（0.3.3）· EvidenceView（0.3.3）· DiagnosticsView（deferred · 0.3.4 候选）· DockSD（roadmap · 0.3.4+/V2）；WorktreeBoard `.wtwrap` 与单 lane 内嵌 subagent 树**有意不登记**（不作实现目标 / 延后 fleet 家族）。
  - 收口 D-POPOUT 治理不一致（SPEC 指名 DockSD 为召唤坞实现真源、DESIGN-REF 从未登记），D-POPOUT 附 2026-09-08 修订注；本次纯文档/登记改动，零页面视觉、零 tokens.css。

---

## 2026-07-20
- [GUI][文档] **`0.1.0-alpha.1` framework gate 定档 Tauri 为唯一 production baseline**:
  - Tauri/GPUI 均通过同一 D1 fixture 功能 parity、10k event 顺序、共享 50k paging 与本机 macOS build/launch smoke;fixture 身份/摘要绑定到实际 test command，launch 须完整存活 5 秒，机读 JSON 保留精确命令、exit code 与 host/tool version。
  - GPUI 因 p95、真实 IME/可访问性、Linux/Windows、framework rendering、soak/CPU、视觉与交付恢复证据缺失触发 hard rule no-go;缺失项未用估算伪造。
  - GPUI 仅保留 comparison spike;Task 5 只启动 Tauri production shell,Tauri 同类缺口继续作为后续 release blocker。

## 2026-07-06
- [GUI][文档] **对齐 robocode 最新 runtime 契约(0.2.2 收口 + frontend-integration-contract + 2026-07-04 编排评审)——GUI 稿 6 项调整**:
  - **gate_strength 门控硬度**(D-GATESTR·新):lane 契约一等事实常显 `●full/◐coop/○containment`——D1 `.viab .gs`(route chip 内前缀)、D10 `.lc .gsb`、D13 fleet GNode(glyph+FULL/COOP/CTN);gui-kit `.wslane .viab .gs` 登记。
  - **MergeGate 证据清单**(D-MERGEGATE·新):持久状态机 proposed→collecting_evidence→(blocked|needs_changes)→accepted→(merged|reverted) + 五类 required evidence(patch/test_result/review/doc_update/release_artifact);新 gui-kit `.mgate` 组件,补进 D12;status 由 reducer 归约非前端勾选。
  - **预算盲区**(D-BUDGET-BLIND·新):外部 CLI(containment)token 不可见 → D1 `.envctx` blind 分支(`.blindtag/.blindwhy/.blindprox` 代理指标),禁估值冒充精确费用。
  - **角色收敛**(D-ROLES·新):7 内置角色(planner/coder/reviewer/tester/doc-writer/researcher/release-operator);D13 `editor→coder`、D1 subagent `writer→doc-writer`;orchestrator/context-builder/lane-supervisor = runtime 组件非角色。
  - **mutation_policy 正交字段**(D-MUTPOLICY·新):autonomous/propose-only/read-only 加进 D4 步骤4;manual review 从「第五条 route」降级为 propose-only,可与任何 route 组合。
  - **task 状态枚举映射**登记进 DESIGN-REF「内置角色 & task 状态词表」(waiting_approval/needs_input/blocked=一等「需要人」);现有 run/done/gate/idle/wait 视觉码定为其显示分组,不另立枚举。
  - 同步:SPEC 新增 6 条 @DECISION;DESIGN-REF 登记 `.viab .gs`/`.mgate`/`.envctx` blind + 角色/状态词表。全部 5 屏 babel 编译零错。

---

## 2026-07-02
- [GUI][文档] **验证器复查:D1 env dock 补双语 + 分支泄漏修正**:Environment/Changes/Local/Commit or push/PR 状态句/Context/used·spent/Subagents/Sources/MCP connected·offline/LSP ready 全接 t();env 分支行 `codex/config-loader`(agent 名拼接残留)→ `vd/config-loader`(直用 lane.branch),Sources 行同。
- [文档] **CHECKLIST DoD 新增两条(用户提议)**:改 tools/guard 必附实际运行证据(仿真受限须如实标注);改 SPEC 条目须逐句核对实物存在——「声明先于验证」定为事故模式(node wrapper / .pinbtn 两例)。
- [工具][GUI] **第 3 轮编码侧核查 3 项收口**:
  - `run-checks.node.js` 修双 bug:RESULT 解析改 includes+截尾(兼容前导 `\n`),isSandboxBlocked stub 兼容箭头函数形态(check-tui-glyphs);沙箱内仿真验证 stub 命中全部 5 脚本 + changelog/status 端到端 PASS(tokens/icons/tui-glyphs 全量需真 node,编码侧 CI 首跑请确认 exit 0)。
  - D-SIDEBAR 定单入口:删 D1 `.pinbtn` 死 CSS(从未渲染),SPEC 改「切换入口单处 = 活动栏底部 pin」。
  - 收尾:D8 `.mrow`/D13 `.arow` 接密度轴(--card-pad-y/--row-pad-y/--list-gap);D1 commitbar+evfoot 按钮补 t();D2h TOOLDEF 描述函数化补双语;产品方案 v1 存档移入 `Core/_archive/`(screens-status 同步);T5 "same Rust stack as RoboCode"→Viden 引擎,NAMING-MAP §4 立「渲染文案禁提引擎旧名」纪律。
- [文档][GUI] **编码侧复审 punch list 5 项收口(2026-07-02 回执)**:
  - NAMING-MAP 修正:`dontAsk` UI 标签改 **Full Access**(引擎 from_legacy_mode 5→4 折叠,无独立 label;靠 cli_name 区分档位),§1 补 CLI bin `robocode-cli`/macOS 配置路径/briefs+steering 带名路径,§3 provider 注册表补 groq/mistral/together/kimi/qwen/zhipu/deepseek-anthropic;设置屏 dontAsk 行同步。
  - mock 泄漏清尾:组件库 projsel `robocode→viden`、D9 `RoboCode 0.4→viden 0.1.30`;D1 补双语(rail 视图名/workspace 面板/NewLanePopover/ProjectPicker/AGENTS.md 标签)。
  - D2h 补概念稿标注 + 全页 t() 双语(此前 0 t());删 D1 死代码 AGENT_PRESETS_F(真源 NL_AGENTS);gui-settings 头注更新(vSetLang);check-tokens baseline 重固化(清 7 条陈旧 D1 条目,214 条);新增 `tools/run-checks.node.js` 裸 node wrapper(CI 复现 guard,check 脚本零改动)。
- [设计规范][GUI] **i18n.js v2:语言热切换(用户反馈「切换要刷新」)**:t()/tk() 改为动态读 `window.vLang`;新 API `window.vSetLang(v)`——页面标 `<html data-i18n-live>` 时切语言零刷新(hook React 根重渲 + 重绑 data-i18n-key + 派发 `v-lang`),未标页照旧 reload(模块级冻结安全)。
  - D1 驾驶舱开启 live:含 t() 的模块级数据表函数化(TOOLDEF_SD_F/AGENT_PRESETS_F),gui-inbox 四张数据表收进 `inboxData()`;设置屏 Language 项改走 vSetLang——切换保留窗口位置与视图状态。
  - live 页纪律入 i18n.js 头注:t() 只在组件 render 内调用,数据表写成函数;重渲用 React.cloneElement 换新引用(React 18 同引用 bail-out)。
- [GUI] **i18n 全站打通收尾**:普查全部活跃屏 —— 唯一漏挂的 D2 横屏召唤坞补上 `i18n.js`;至此所有 Core/TUI/GUI 活跃页均有悬浮 EN/中 切换(`viden-lang` 全站持久),D1 另有 设置→Appearance→Language 产品级入口。
- [GUI][设计规范] **立 `@DECISION D-COPY`:高保真原型零描述性文案 + 全量双语化(用户评审)**:D1 驾驶舱与 TUI 统一原型清空设计说明(死 intro CSS/states·hints 注释块/隐藏 plead + TryRow)、长解释 tooltip 收为产品微文案;屏级说明只留 screens-status meta 与设计稿索引(单一真源,不拷副本)。
  - D1 挂 `i18n-dict.js + i18n.js`,全部界面文案(tooltip/菜单/权限描述/坞工具描述/side chat mock)改 `t(en,zh)` —— 消中英同屏混排。
  - `gui-settings.jsx` 文案全 t() 双语,删规范注(「引擎无对应」/#13/NAMING-MAP 标签移入代码注释);Appearance 新增 **Language 设置项**(viden-lang · reload 生效)。
- [文档][设计规范] **产品名定档 Viden + 新增 `docs/NAMING-MAP.md` 映射真源(评审反馈 🔴2 / 看板 #21)**:字标/CLI/配置路径/env 前缀/分支前缀 `vd/`/门控类型↔UI 标签 全量映射 RoboCode v0.1.30 引擎现名;SPEC 立 `D-NAME`。
  - i18n-dict.js 新增 `window.VIDEN_BRAND` 品牌字符串真源(改名=改一处);新稿禁散写字面量。
  - 修 mock 泄漏:D1/D6/D8/D13/T0/产品方案 v2 的 "robocode" project scope → `viden`(自举叙事)。
- [GUI] **新增设置/偏好屏 = D1 第 8 视图(评审反馈 🔴1)**:新组件 `gui-settings.jsx` `<SettingsView/>`(.gset- 自注入),活动 rail ⚙ 走 view-state 路由。
  - 7 节:Provider&Models / Permissions / Appearance(真接 RC) / Keyboard / Notifications / Privacy / Workspace;权限模式/配置键对齐引擎真实枚举(NAMING-MAP §2/§3)。
  - Notifications 节承接 **#13 离桌通知兜底**(email/webhook escalation + quiet hours);SPEC 立 `D-SETTINGS`/`D-NOTIFY`。
- [GUI][入口] **新增 D14 审计与时间线(看板 #11 · 合规卖点收口)**:工作区级只追加决策链时间线(permission 批拒/闸裁决/合入/策略变更/lane 生命周期),锚 git、可导出 JSONL/CSV/git-notes;审计≠证据分界立 SPEC `D-AUDIT`;登记 screens-status。
- [GUI] **D12 补合入后回滚流(看板 #12)**:合入态新增「Merged but broken?」节 + 「↩ Revert this merge」——revert 走同一条闸链(反向 commit+回放基线回退+集成闸),禁改历史;SPEC 立 `D-ROLLBACK`。
- [入口][文档] **D7/D8/D9 打 roadmap 标(评审反馈 🟡3)**:screens-status 新增 `roadmap:true` 字段 + GUI 设计稿索引渲染虚线 ROADMAP 徽标(BUILT≠可开工);SPEC 立 `D-ROADMAP`。
- [GUI][设计规范] **popout chrome 去重(评审反馈 🟢5)**:gui-titlebar.jsx 新增 `<GuiPopoutBar>`(winbar 度量同源 gui-kit,`.tt/.sp/.pin/.pop` 收进 gui-kit),D10 双窗口接入、删页内手抄字标;召唤坞定实现真源 = D1 DockSD,D2h/D3 降为概念稿并标注;SPEC 立 `D-POPOUT`。
- [文档][Core] **O-A1/O-A2 收口(评审反馈 🟢6)**:O-A1 语言策略定档(交互稿双语·Core 文档 zh-primary 有意),D3 竖屏召唤坞本轮补齐 t() 双语;O-A2 定性 Logo 页 = 品牌概念展示页(页内已标注,不跟产品 chrome);看板 #1/#2/#11/#12/#13/#21 默认态 open→done。
- [GUI][设计规范] **立 `@DECISION D-SIDEBAR`:左侧 lanes 侧栏 float/pinned 双模式定档**:默认 float(hover 峰显·宽度让给转录),pin 切换入口两处(活动栏底部 + 侧栏顶部);focus 专注模式覆盖强制浮窗。
  - D1 补偏好持久化 `vd-leftmode` + `vd-leftw`(刷新/重开不再重置回 float/218)。
- [设计规范][GUI] **密度轴接通(此前 DENS 切换在 GUI 无效)**:tokens.css 密度段新增行/区块级 token(`--row-pad-y/-sm` `--card-pad-y` `--list-gap` `--sec-pad-y` `--input-pad-y` `--msg-mb`),regular = GUI 现值基线。
  - gui-kit.css + D1 驾驶舱关键间距接入:lane 行/env 行/todo 行/thread 卡/分节标题/composer/转录消息间距。
  - 全 GUI 覆盖:D2 决策(qitem/msg)/D2·D3 召唤坞(msg/composer)/D7+gui-inbox(irow/bitem)/D9(msg)/D14(arow)/gui-settings(setrow) 用 calc 偏移接入,regular 与现值逐像素一致;讲解型卡片/表格布局刻意不随密度。

## 2026-07-01
- [GUI][设计规范] **立 `@DECISION D-BACKEND` + 补齐 GUI 三后端(收审查看板 #3 / O-A3)**:GUI 此前只有 ACP/built-in 两类、且把 built-in 误映射成金——现对齐 TUI 铁律 **ACP=青 / built-in=紫 / tmux=金**。
  - D1:`VIA_COLOR` 改三色 · L2 改 tmux lane · agent 选择器 + New-lane 弹层加 tmux 组 · lane-proto 注补 tmux 行。
  - D4:`AGENTS` 加 tmux bridge 选项,步骤2 文案「ACP / built-in / tmux 三后端」;SPEC 加 D-BACKEND,DESIGN-REF `.viab` 登记三色铁律。
- [GUI][入口] **新增两屏补设计漏洞:D6 空态与错误态 + D8 团队身份与权限(收审查看板 #9 / #10)**:均接 gui-kit(GuiTitleBar/GuiStatusBar/活动 rail)+ 双语 + Tweaks。
  - D6:空驾驶舱 / 连接中 / 断开 / agent 停止 / 预算耗尽 / 空闸队列 六态一壳切换,每态「原因 + 下一步动作」。
  - D8:成员名册(人 + agent 身份)· 角色×能力矩阵(谁批实机闸 / 谁改 viden.toml)· 邀请/加入流程;登记进 `screens-status.js`。
- [文档] **审查看板对账 + CHECKLIST §5 GUI 迁移现状校准**:看板默认态把 SPEC 已 done 的 #3–#8 + 本次补的 #9/#10 从 open→done(恢复进度可信);§5 GUI 行改为「D1 + 11 屏已接 gui-kit」(原「仍多为内联」已过时)。

## 2026-06-30
- [文档] **CLAUDE.md 补「快速检索（冷启动地图）」+「翻译成 app（防漂移）」两节 + 新增根 `AGENTS.md` 指针**:为外部/冷启动 agent 补入口——检索表按 token/组件/图标/屏 定位真源再 grep;app 翻译分三层(①直接共享 CSS·②脚本派生原生色值·③原型脚手架原生重写)。
  - `AGENTS.md` 只做跨工具(Codex/Cursor)指针,零内容复制 → 路由回 CLAUDE.md / DESIGN-REF,防漂移。
- [TUI][工具] **TUI 字形收敛登记 + 新增 `check-tui-glyphs.js` emoji 闸 + 清 3 处真 emoji**:TUI 字形集本已收敛(◆▶✓▣◌⏸✗·pinned T4 §08「色=状态」),但真源埋在借鉴探索页未登记——现进 DESIGN-REF「TUI 字形词表」(状态字形 + 结构字形 + 禁用项)。
  - 清:会话页/统一原型 todo 勾 ✅→✓;组件库说明 📖→§;T2 卡片 ⚡→↻(emoji-presentation 撑破等宽栅格)。借鉴页 😀 反面样例有意留(自带「与字形集冲突」说明)。
  - guard 扫 `TUI/` 禁 emoji,baseline grandfather 😀 样例;CLAUDE.md DoD + SPEC `D-GLYPH` 挂 🤖 check-tui-glyphs。
- [工具][GUI] **新增 `tools/check-icons.js` 机检防图标漂移 + 补清 3 处遗漏 emoji + pin 尺寸对齐**:对标 check-tokens——扫 `GUI/`(跳 `_archive`/`gui-icons.jsx`),命中页内联 `<svg class="ic">` / `VRAIL_ICONS` 注册表 / emoji(astral+VS16)即 FAIL;baseline grandfather 组件库 showcase 的内联陈列。
  - D10 置顶 📌×2 → 线性 pin SVG;D2存档 stagegate 🔒 → lock SVG(此前 emoji 清理漏网,被 guard 抓出)。
  - D1 活动 rail 底部 pin 由 `.ic sm`(15px)改 `.ic`(19px),与齿轮/其余 rail 图标对齐;CLAUDE.md DoD + SPEC `D-ICON` 挂 🤖 check-icons。
- [GUI][设计规范] **GUI 图标收口单一真源 `GUI/gui-icons.jsx`(window.ICONS + AgentLogo + GuiIcon)+ 统一重绘分歧 + 清 emoji**:此前 D1 一套 `I*`、D2/D5/D7/D12/D13 各自 `VRAIL_ICONS`、标题栏一套——同一 worktree/lanes/review 各画各的;现全部取 `ICONS[key]`,画法以 D1(视觉真源)为准。
  - lanes 统一为 D1 swimlanes(替 D2/D5/D7/D12 的 hamburger 三横线);worktree/review 收敛到 D1 画法;`AgentLogo` 从 D1 抽出共享(品牌固定色不换肤)。
  - GUI 禁 emoji:🔒→`lock` / 🤖🎮📈→`robot`/`game`/`ml` 线性 SVG(D1 sdburl · D2决策/D9 stagegate · D2横/D3竖召唤坞 · D11 模式卡)。
  - SPEC 加 `D-ICON` 护栏;DESIGN-REF 登记「GUI 图标目录」;`gui-titlebar` 改吃 `window.ICONS`(8 个 GUI 页加 `gui-icons.jsx` 引用,须先于 titlebar/主脚本)。
- [GUI][设计规范] **权限提示去"浮窗割裂感" + 紧凑化(参考 agent 客户端通用惯例·不复刻专有 UI)**:① 去 `--shadow-pop-gui` 浮投影 + 新 `.dock` 满幅停靠 composer 上沿(扁平·只留顶边分隔)→ 读作"输入区被门控"而非贴上去的弹窗;② 选项用**横向紧凑 chip 行**(纵向键盘列表试过但 433px > 对话列高·弃),取惯例中可落地的:安全项 `Y Once` 默认 `.on` 高亮(青=焦点)+ `.deny` 红键 + `Deny` 点击**转向**(composer 占位切「Tell codex what to do instead」)+ 前台去 `.pulse`(已聚焦+金框不必招摇);③ 砍脚注 / 短理由 / 收内边距,**高度 433→165px**(转录从 32→105px 恢复)。gui-kit + D1 内联镜像 + DESIGN-REF 同步。
- [文档] **@OPEN A 类对账(承 B 类全销):A5 销 · A1/A2 留并校准描述**:逐条核查活跃代码——O-A5 决策中心命名已收敛(活跃页统一「决策中心 / Decision Center」·"routing foyer" 零命中·"行为 Diff 评审" 只剩存档屏标签 + 产品方案 v2 血缘注)→ 标 done;O-A1(双语铺设)、O-A2(Logo 页中文 only + 旧单行 chrome)经核为**真·留开**,但把过时描述校准到现状(原说「T3/T4/T5 全中文」实已 t() 双语)。至此 @OPEN high/mid:A4/B6/B7/B8/A5 五条销、A1/A2 二条留 —— **开放清单恢复可信(不再有"已修未销")**。
- [GUI][设计规范] **收 O-B8(D2 图表硬编码色·已无活跃漂移)+ D-COLOR 落实回写**:核查活跃「决策中心」4 图(Jump/Loss/Step/PID)——全走 `var(--fg-muted)`(改动前)/`--accent`(改动后)/`--border-soft`(网格),**0 裸色·随 theme**;15 处硬编码图表色只在 ARCHIVED「D2 行为Diff评审 存档」(冻结快照·baseline 接受·有意例外)。@OPEN O-B8 标 done,D-COLOR 的 O-8 指针改为「已落实」。
- [设计规范][TUI][GUI] **状态栏钉/滚立 `D-STATUSBAR` 护栏 + 收 O-B6/O-B7(已无代码漂移·补机读真源)**:核查全部活跃状态栏(T1/T1b/T1d/T2/统一原型 + D1 + 两组件库)——gate 一律 `.vgate-badge`(TUI)/`.sb-right`(GUI)钉右、ticker 仅跑 ambient,**早已全轨合规**;O-B6 描述的「T1d 把 gate 滚进 marquee」在现版不成立(T1d 现钉 gate)。
  - 缺的是护栏没进 `@DECISION` → 仍挂 @OPEN 像没解;补 `SPEC D-STATUSBAR`(三区·身份左钉/可操作右钉绝不进 ticker/中段=空或 ambient ticker·宽度阀·reduced-motion·跨 TUI`.vstatus`+GUI`.statusbar`),@OPEN O-B6/O-B7 标 done。
  - DESIGN-REF `.vstatus` / `<GuiStatusBar>` 两条目补「钉/滚契约」指向 D-STATUSBAR。
- [GUI] **组件库 (GUI) 加 `.gperm` 四类活体样本 + 页内 tweak**:permission 四类(shell·高 / 写受管路径·中 / 网络·MCP·中 / 远程 target·高)各陈列一张活体卡(风险三档 hi/md/lo·远程带 `.tgt` 红前缀+禁 ⇧A·需双人);页内 vanilla tweak 条切 Pulse / Footnote / Scope tiers(full↔minimal),仅驱动本区展示、不改组件。
- [GUI][设计规范] **补齐 GUI 权限 UI(O-A4 收口)+ 立 `D-PERM` 护栏 + gui-kit `.gperm`**:此前 permission(执行前)只在 TUI(T3),GUI 只有 gate、无权限 UI;D1 旗舰那个含糊的 `APPROVAL` 模态实为漂移本体——把 permission(Allow/Ask/Deny tool)与 gate(diff 预览 + Open in review)揉成一框,且没接进产品流程(无触发·是死的)。
  - permission 模型从 T3 提升为产品级护栏 `SPEC @DECISION D-PERM`(permission≠gate · 四类 · 范围分级 y/a/⇧A/e/n · 远程 e-stop/双人 · 无人值守入队);@OPEN O-A4 标 done。
  - 新增 canonical `.gperm`(gui-kit + D1 内联镜像·token-only·金=「需要人」·风险三档·命中≥28px·reduced-motion 关 pulse);D1 把权限提示改为**内联会话底·阻塞 composer**(忠实 T3「就地弹·阻塞此会话」·默认可见=可达),删被取代的死模态 `Approval` + `.appmodal/.permseg/.amdiff/.abtn` 内联 CSS(顺带消一处裸 hex `#04140a`),diff/评审归 gate。
  - DESIGN-REF 登记 `.gperm`(类名+最小 HTML);死代码 hints.approval 文案同步改正(原写「allow/ask/deny + diff」错模型)。
- [GUI] **旗舰文件改名 `D1 驾驶舱`→`桌面驾驶舱`(去看不懂的编号)**:`GUI/Viden - D1 驾驶舱 (GUI).html` → `GUI/Viden - 桌面驾驶舱 (GUI).html`(与同目录 组件库/设计稿索引 无编号对齐·"桌面"区分 TUI 驾驶舱);同步 4 处引用:screens-status.js×2 / GUI.html 启动链接 / DESIGN-REF 目录树 / check-tokens.baseline.json 键。
- [TUI] **2 个 TUI 页终端面随皮肤(同 D1·补浅色可读)**:`opencode 借鉴`(`.term`/`.statusbar`/gate 条)、`T1b 侧栏探索`(`.dock`)此前用固定 `--term-*` 深底 + 主题 `--fg-*` 文字 → 浅色下深字深底看不清;body 级别名 `--term-*`→`--bg-void/topbar/panel/border-soft`,深色不变、浅色转浅底深字。至此活跃页固定-term-深底类色债清零。
- [GUI] **D1 终端坞随皮肤(修浅色下深块不协调 + 深字深底看不清)**:`.sdock` 把固定 `--term-screen/chrome/bar/edge` 别名到主题 `--bg-void/topbar/panel/border-soft`(对齐 TUI 统一原型先例·消漂移)→ 终端随 skin×mode,浅色下转浅底深字、可读且融入;顺带审批 badge 文字 `--term-screen`→`--on-accent`。
- [GUI] **D1 旗舰清理为「最终产品形态」(去评审/草稿痕迹)**:删标题栏跳「设计稿索引」的外链(唯一离页入口)+ 「TBD」占位钮;`<title>` 去「概念稿」;删左栏 float 折叠态的 `.peekcap` 提示文字——它被塞进 12px 窄边逐字折行成竖排乱码,改为只留 `.edgehint` 细高亮条作悬停提示。
- [GUI] **补齐 GUI 子页换肤能力 + 索引页加 Theme 直达**:GUI/pages/* 13 屏此前未载 chrome.js → 无切换器,且索引切肤 reload iframe 后读不到 rc-skin/mode 不跟随(卡 aurora-dark);统一注入 `../../chrome.js`(对齐 TUI/pages 既有做法,零 token 漂移——全页仍只链一份 tokens.css)。两索引页工具栏加「◉ Theme 主题换肤」链接直达 Core/Aurora 展示页。
- [设计规范] **债4:换肤重构为两轴 `data-skin` × `data-mode`(彻底去 `data-theme`,无兼容垫层)**:旧单轴 6 值(5 暗1亮·明暗与性格混挂)→ 两根正交轴。`data-skin`=aurora/ice/mono/amber/phosphor,`data-mode`=dark/light。tokens.css 颜色层重写为 8 个 `[data-skin][data-mode]` 块。
  - aurora/ice/mono **成对 dark+light**(新作 ice-light/mono-light,逐套校 WCAG:fg-muted≥4.5·accent/gold≥4.5·on-accent 压填充≥4.5);amber/phosphor = 复古终端族 **dark-only**(CSS `[data-skin]` 不挂 mode 强制深;选择器侧 light 禁用)。
  - 6 个切换器全改两轴:chrome.js(SKIN 圆点 + MODE ☾/☀ + DENS,amber/phosphor 灰 light)、两索引页、Aurora 自带 RC+Tweaks、统一原型;legacy `rc-scheme`/`viden-theme` 一次性迁移到 `rc-skin`+`rc-mode`。38 页 + 3 根 `<html>` sweep 为两轴(28 个裸根补显式 axes);D1 删本地死代码 scheme 块(消 11 hex + data-theme 冲突)。
- [设计规范] **修守卫失明:check-tokens 纳入全部 38 个设计页 + ASCII 暂存流程**:run_script 的 readFile 拒收 CJK/括号路径(所有设计页),旧版静默跳过=守卫对正经页全盲;升级 check-tokens.js——这类文件显式报「覆盖缺口→RESULT: BLOCKED」并吐出 ready-to-paste 的 copy_files 暂存清单 + `_scan/_manifest.json`,重跑时读 ASCII 副本按真实路径记账。
  - [设计规范] 经暂存揪出 216 处此前不可见的裸色,由守卫本体重建 baseline(226 条,strip 一致);分类:~87 复制 token 值的漂移(跨主题 swatch 数组,债4 收)/~117 全新 hex(展示页/品牌/T5 终端保真,意图性)/12 rgba(D1 阴影)。守卫从此抓各页未来新增漂移。
- [设计规范] **债3:皮肤清单收单一真源 `window.RC.SCHEMES`**:此前 5 处硬编码皮肤列表(chrome.js / 两索引页 / Aurora / D1),加一套要改 5 处;chrome.js 的 `SCHEMES` 升为 `[id,en,zh]` 三元组并暴露到 `RC.SCHEMES`,两个索引页改读它(带回退);Aurora 展示页 + D1 mock 的内联副本标记为两轴重构时折叠。
  - [设计规范] 密度默认对齐:`:root` 基线改 compact(=全站实际默认·桌面高密度),消「:root 写 regular 但 chrome 默认 compact」的矛盾;DESIGN-REF/SPEC 同步登记注册表位置 + 密度默认。
- [设计规范] **还债:立 `--on-accent` 墨色 token,消 `--bg-void` 一token两用**:填充强调/语义芯片上的文字此前全借 `--bg-void`(背景token)当墨,耦合脆弱;新增语义 token `--on-accent`(随 mode 翻黑/白),全库 38 处(18 活跃文件:D1/D2–D13/Aurora/T0/T5…+ gui-kit/tui-kit/chrome.js/gui-inbox)统一迁移;light 的 success/warning/progress 微调暗一档让白墨达标(单 ink 压全部填充色 ≥4.5)。
  - [文档] DESIGN-REF 登记 `--on-accent`、SPEC 加 D-A11Y 对比护栏 + D-COLOR 注明填充墨色规则;hex 真源仍只在 tokens.css。`_archive/` 保留旧式不迁。
- [GUI] **D1 角标 + Send 按钮文字色对齐正版 kit(修 light 下糊)**:两处本地 fork 写死深色文字——角标 `--term-screen`(固定近黑)、Send `#04141a`(裸 hex)——dark 没事但 light 下近黑压深色底=糊;改用 `--bg-void`(随皮肤翻转,dark 近黑/light 近白),light 对比从 3.8–3.9 提到 4.0–4.7,顺带消一处裸 hex。
- [设计规范] **6 套皮肤逐套校 WCAG 对比度(token 单一真源)**:`--fg-muted` 全皮肤 3.3–4.4 → **≥4.5:1**(正文级,vs bg-base/panel/elev);`--fg-faint` ~1.8–2.3 → **≥3:1**(仅大字/UI 描边,明确非正文);light 最弱的 `--accent` 3.93→4.54、`--gold` 3.5→4.51(连带 border-active/page-accent 同步),「需要人」告警在 light 下恢复份量。
  - [文档] DESIGN-REF fg-muted/faint 行写入对比度护栏(≥4.5 正文 / ≥3 非正文);hex 真源仍只在 tokens.css。
- [GUI] **抽出 `<GuiTitleBar>`(gui-titlebar.jsx)canonical 标题栏 + 8 屏对齐 D1 真源**:各页此前 `.vbar` 各搭各的工具组(D7 仅 palette / D5 palette+popout / D2 三按钮 / D11 仅 settings…),现统一为组件(灯+字标+projsel+gitops sync·worktrees+工具组 palette/focus/popout/▤索引/settings);D2决策/D4/D5/D7/D9/D11/D12/D13 全部改用,删各页内联 chrome fork(顺带修 D9 残留 `.vbtn` popout)。
  - [设计规范] gui-kit `.tbtitle`/`.projsel`/`.gitchip` 补 `flex:none`+`white-space:nowrap`(防窄宽字标换行);召唤坞 D2横/D3竖 `.projsel .br` 金→青对齐 D1。
- [GUI] **团队·人+agent 通信频道整合进旗舰 D1 + 抽 `<InboxView>`(gui-inbox.jsx)单一真源**:D1 活动 rail 加 `IInbox` 入口(金 badge),开闸收件箱/团队 roster/移交/简报/变更通告(通道 A 送人 + 通道 B 送 agent memo + ack 追踪);组件 `.gi-root` 作用域自注样式,D1 与 D7 共用同一份,D7 去重(-18KB,删本地 Inbox/Briefing/Notice + 数据)。
  - [文档] DESIGN-REF 登记 `<GuiTitleBar>`/`<InboxView>`(接入+props),文件树加两文件;CHANGELOG 同步。


- [TUI][GUI] **设计稿索引 NAV 收进 screens-status.js(消最后一处屏清单重复)**:两索引页此前各硬编码一份 NAV 数组(TUI 12 / GUI 15 屏)——并存于状态真源之外的第二份清单;现 `screens-status.js` 加 `docs[]`(27 项·track/group/nav/file/state/kind),两页运行时按 track 过滤+group 分组渲染,srcOf strip `<track>/` 前缀;状态真源成完整屏清单。
  - [工具] check-status.js 扩展:kind 加 ref/star、新增 docs[] 校验(track/kind/file 悬空);RESULT PASS(screens 12/12 · docs 26 已建 + 1 存档)。
- [TUI][GUI] **借鉴 hirobot:设计稿索引修无白屏导航引擎**:两索引页 iframe 切换从 `display:none` 显隐(隐藏丢渲染层→切一下闪白)改为 **z-index 叠放全程可见** + 空闲预热(requestIdleCallback)+ 冷页就绪门控(load 后才 raise·旧屏留显不露白)。
  - [文档] PROTO-STANDARD 新增 §3b「无白屏导航引擎」四件套 + display:none 反例;§3 索引约定指向它。
- [文档] **CHECKLIST 同步新真源 + 第三条 guard**:§0 真源地图加 screens-status.js / i18n-dict.js / SPEC.md，§3 DoD「新增屏」改指 screens-status.js，§4 guard 表加 check-status.js(两条→三条)。
- [文档] **借鉴 hirobot:新增 `docs/SPEC.md` 决策护栏机读真源**:把散在 CLAUDE.md/审查看板的护栏收敛成 grep 锚点(@DECISION 9 条 D-* + @OPEN 索引审查看板 7 条 high/mid + @GREP 速查);CLAUDE.md 交付物 + DESIGN-REF 文件树指向它。:新增该真源(13 屏 × Core/TUI/GUI 三轨·id/state/kind/file/中英),index.html 删硬编码 `data[]` 改运行时直读 + 渲染进度(12/12 已建·按轨小计)与 state 徽标;卡片/轨道元信息均从真源派生,加/删/改屏只改一处。
  - [工具] 新增 `tools/check-status.js` guard(run_script 风格):机检重复 id / 非法 state·kind / track 越界 / **file 悬空(点了 404)**;当前 RESULT: PASS。CLAUDE.md DoD 加「加/删/改屏」一行(🤖 守)。
- [文档] **借鉴 hirobot:集中 i18n 词典 `i18n-dict.js`(跨页复用文案唯一真源)**:i18n.js 扩展 `window.tk(key)` + `[data-i18n-key]` 挂载自动绑定(向后兼容 `t(en,zh)`);index.html 作范式(状态枚举/载体/图例/动作词走 tk),收敛审查看板 A-1「语言不统一」的机制缺口。DESIGN-REF + CLAUDE.md DoD 同步。:两个 token-clean 启动卡(品牌壳 + i18n)0.7s 自动 `location.replace` 旗舰驾驶舱,并留旗舰/设计稿索引/组件库/index 手动链接;删除根目录无引用空壳 `Canvas.dc.html` + 其运行时 `support.js`(遗留 design-canvas scaffold)。
- [GUI] **QA 抽查 + D1 出口闭环 + 零裸色收尾**:逐屏复核 D1/D5/D7/D9/D10/D11/D13 + 组件库渲染无碰撞无报错;D1 标题栏新增 `▤` → 设计稿索引(纯产品回文档闭环,复用一个 TBD 槽);D5 缩略图按钮 `rgba(5,9,15,.85)`→`color-mix(bg-void)`、Pip 三处装饰阴影→`--shadow-md/lg/toast`。召唤坞已 token-clean;D2存档图表为归档数据序列色(基线保留)。
- [GUI] **补全 PROTO-STANDARD §1 结构:新增 `设计稿索引 (GUI)` + `组件库 (GUI)`,index GUI 区收敛**:索引 = 侧栏分组导航 + iframe 保活秒切(D1 旗舰在根 / D2–D13 在 pages/,共享 `../chrome.js` 换肤经 rc-state reload 跟随);组件库 = gui-kit 逐件活体陈列(链 `gui-kit.css`,改一处全跟随)。根 index.html GUI 区从 13 张散卡收敛为 **产品入口 + 组件库 + 设计稿索引** 三卡(D1 徽标 inline→kit,修掉 D2存档 失效根链接)。DESIGN-REF 文件树同步。
- [GUI] **目录按 PROTO-STANDARD 重整 + 旗舰 D1 全屏窗口化产品入口**:13 屏全部移入 `GUI/pages/`(资源引用 `../`→`../../`、kit/statusbar→`../`),D1 / gui-kit / gui-statusbar 留根目录;入站链接 index.html(12)/ TUI-T0(3)/ Core-产品方案v2(7)同步加 `pages/`。
  - [GUI] **D1 改为打开即用纯净产品**:去文档外壳(kicker/标题/switcher/scheme/注解),App 只渲染 cockpit `<Win>`;`#root` 全屏 stage + 窗口管理器(四边四角 resize / 拖标题栏移动 / 绿灯·双击最大化↔还原);换肤改走根 `../chrome.js`(6 皮肤+密度·单一真源)。驾驶舱代码仍只此一份。
- [GUI] **10 屏接 gui-kit、删内联 chrome fork、裸色收 token**:D2决策/D4/D5/D7/D9/D11/D12/D13/D10 + D1 旗舰统一用 gui-kit 窗口壳/标题栏/活动 rail(`.act`/`.actbtn`/`.badge`)/dots;删被取代的 `.vbar`/`.vrail`/`.vbtn` fork;`#06222b`/`#1a1206`(accent/gold 上深字)→`var(--bg-void)`、toast 阴影→`--shadow-toast`、D9 图表色→accent/gold/border-soft。
  - [GUI] D12 时间线 `.tl` 改名 `.rline` 避开 gui-kit 全局 `.tl`(交通灯)冲突;D2存档 UI 语义色收口(diff add/del→success/error、approve→bg-void),图表数据序列色按基线保留;召唤坞 D2横/D3竖 与 Pip 为独立浮层/装饰概念,保持自包含(token-clean)。
- [设计规范] **tokens 加 GUI 浮层阴影 + gui-kit 补 winbar dots**:`--shadow-pop-gui`(下拉/菜单)/`--shadow-toast`(toast);gui-kit `.winbar .dots i:nth-child` 上红/黄/绿(此前缺色)。DESIGN-REF token 表 + GUI 迁移状态同步。
- [文档] **沉淀 `docs/PROTO-STANDARD.md` 平台原型结构规范**:以 TUI 定稿为样板(全屏产品入口 + 组件库 + 设计稿索引 + pages/assets + kit + _archive·单一真源·窗口化·保活秒切),含 GUI 改造 7 步流程 + 本轮经验坑。CLAUDE.md/CHECKLIST 指向它。
- [TUI] **讲解/借鉴页裸色收口**:T0/T1/T1b/T1c/T2/T5/opencode-hermes 里页面专属 CSS 的裸 `rgba()`/`#hex`(composer/statline/keybar/browser/player/toast/阴影等)全部改 `var(--*)`/`color-mix`;`.depth256` 256 色模拟与小地图数据色作 baseline 保留。
- [TUI] **目录参照 hirobot 重整**:`screens/`→`pages/`(各设计屏平铺)、组件 jsx 进 `pages/assets/`、`组件库` 提到 `TUI/` 根(展示页);`TUI/` 根=原型入口(统一原型)+ 组件库 + 设计稿索引 + 规范(tui-kit.css)。设计稿索引 iframe 前缀 `screens/`→`pages/` 并移除组件库项;index.html TUI 区加组件库卡。各屏相对路径相应修正(pages 同深度多数不变;T0/T1 jsx 引用→`assets/`;组件库→根)。
- [TUI] **原型入口与索引分离正名**:`统一原型` 移到 `TUI/` 根作为唯一原型入口(全屏驾驶舱);原 `Viden - TUI 原型` 改名 `Viden - 设计稿索引`。
- [TUI] **`统一原型` 改为全屏产品入口(唯一原型·一套真源)**:去掉文档外壳(kicker/标题/说明/try),驾驶舱 `#cockpit` 铺满视口、无边框圆角,`body` overflow:hidden —— 打开即"在用一个 TUI"(欢迎屏→lane→闸→命令面板,⌃L/⌃P/⌃G 可操作)。左下角 `▤ design docs` 角标通往设计稿外壳;根 `index.html` TUI 入口直指它。驾驶舱代码仍只此一份(不复制)。
- [TUI] **TUI 目录整理(规范级)**:14 个屏全部收进 `TUI/screens/`,根目录只留入口 `Viden - TUI 原型 (TUI)` + 规范 `tui-kit.css` + 组件 jsx + `_archive/`;各屏相对路径同步(`../`→`../../`、`tui-kit.css`→`../tui-kit.css`),外壳 iframe 加 `screens/` 前缀。根 index.html 指向外壳不变。GUI 待 TUI 定稿后参照此结构。
- [TUI] **新增 TUI app 外壳 `Viden - TUI 原型 (TUI)` —— 单一入口·像软件**:左侧分组导航(总览/会话/交互/保真&组件/借鉴)+ 右侧 iframe,一个壳切换全部 12 屏;记忆上次位置(localStorage),顶栏皮肤切换经 `rc-state` reload 内嵌屏跟随。根 `index.html` TUI 区收敛为单一"驾驶舱入口"卡直达它(去掉 13 张散卡)。
  - [TUI] 新增 `Viden - 组件库 (TUI)`:`.v*` 逐件陈列(活体 demo + 最小 HTML,可切皮肤),作为 tui-kit 的查阅/复制库(区别于 `统一原型` 的集成、DESIGN-REF 的文字目录)。DESIGN-REF TUI 段登记两个新入口。
- [TUI] **9 个产品屏全部迁到 tui-kit `.v*`(组件共享落地)**:T0/T1/T1b/T1c/T1d/T2/T3/T4/T5 改用 `.vterm`/`.vstatus`/`.vbe`/`.vticker`/`.vscrim`/`.voverlay`/`.vorow`/`.vgate`,删除各页内联自造的终端框/状态栏/后端 chip/命令面板/闸 fork(顺带消除其中裸 hex/rgba);讲解型布局(anatomy/卡片/box-drawing/对比)保留。开工前整目录快照到 `TUI/_archive/`(冻结可回滚)。
  - [TUI] 共享 `tui-gate-timeline.jsx` 的 CommandGate 迁到 `.vgate`;`index.html` TUI 卡徽标 INLINE→KIT;仅 Charm·HUD / opencode·hermes 两借鉴探索页有意保留内联。
- [设计规范] **共享运行时脚本收口为单一真源(防漂移)**:`i18n.js` / `chrome.js` / `tweaks-panel.jsx` 此前在 Core/TUI/GUI 各留副本(`i18n.js` 根版已与子目录版漂移),现各只在**根目录**留一份 token 化正确版,31 个子目录页面引用统一改为 `../`,删除 8 个副本。改一处全站跟随。
- [文档] **新增 `index.html` 原型总览入口**:cockpit 风格启动页,顶部「单一真源」面板 + 按 Core/TUI/GUI 分组卡片直达全部屏;每卡标 KIT/INLINE/ARCHIVED 徽章,把组件共享/迁移状态暴露在入口;中英双语 + chrome.js 皮肤/密度切换,token-only。
- [文档] **新增 `docs/CHECKLIST.md` 收尾自查清单**:单一真源地图 + 开工前/写码时/done 前 DoD + 两条 guard + 迁移欠债;DESIGN-REF 文件结构与接入段同步(脚本改 `../` 单一真源、加 index.html / CHECKLIST.md)。

---

> 更早条目（2026-06-15 → 2026-06-28）见 `docs/_archive/CHANGELOG-2026-06.md`。
