<!--
  SPEC · Viden 设计决策/护栏机读真源
  受众：AI agent（动设计前必读）。人类叙事版 = CLAUDE.md + Core/「产品方案 v2」。
  约定：决策 ID 永不复用；用 ID 引用；改决策改这里不改副本。
  与邻居的分工：
    · CLAUDE.md          = 基调/纪律叙事（自动加载）
    · docs/DESIGN-REF.md = token 全表 + 组件目录（造件前查）
    · docs/screens-status.js = 屏幕状态机读真源（画到哪了）
    · Core/「设计审查看板」 = 开放问题/矛盾的活看板（@OPEN 的详情在那）
    · 本文件             = 已定稿决策护栏 + 开放问题索引（grep 锚点）
  grep 锚点：@DECISION @TRACK @OPEN @GREP @FLUTTER(N/A·本品 Rust)
-->

# SPEC — Viden 设计决策护栏

```yaml
doc: SPEC
role: machine-source-of-truth   # 决策护栏；叙事版 = CLAUDE.md
read_before: [design_screen, change_token, change_decision, cross-track_change]
on_change: [sync_self, sync_CHANGELOG, sync_DESIGN-REF(若动 token/组件)]
status: living
updated: 2026-09-08
product: Viden                  # AI agent 编排开发工具：lane/会话编排多智能体协作写代码，带工具门控(gate)+人审
tracks: [Core, TUI, GUI]        # Core=Rust 视觉真源&文档 / TUI=终端驾驶舱 / GUI=Rust+Tauri 桌面
impl: { core: Rust, gui: "Rust + Tauri" }   # UI 设计阶段以 HTML 高保真稿沉淀规范
```

## @DECISION 已定稿护栏（引用用 ID·改这里不改副本）

```
D-TOKEN   token 单一真源 = 根 tokens.css。一律 var(--*)；禁裸 #hex / 裸 rgba() / 假 fallback
          (var(--x,#fff) 非法；var(--x,var(--y)) 合法)。🤖 check-tokens.js 守。
          基准：间距 4px(--sp-*) / 圆角 --r-* / 阴影 --shadow-* / 字阶 --fs-*。
D-SKIN    两轴换肤：data-skin（性格/强调色）× data-mode（明暗：dark|light）。
          5 皮肤：aurora(青·默认) · ice(蓝) · mono(灰) · amber(琥珀) · phosphor(绿)。
          @DECISION aurora/ice/mono = 产品皮肤 → dark+light 成对；amber/phosphor = 复古终端族 → dark-only
          (魂在「磷光打黑屏」,选择器侧 light 禁用;CSS 侧 [data-skin="amber"] 不挂 mode → 强制仪深)。
          有效组合 8 个。皮肤注册表单一真源 = chrome.js 的 window.RC.SCHEMES([id,en,zh,modes[]]);加/删只改这里。
          所有颜色 token 随 [data-skin][data-mode] 重定义 → 组件只用语义 var(--*)、零改动。
          持久化 localStorage：rc-skin / rc-mode / rc-density(旧 rc-scheme 一次性迁移)。
          密度挂 data-density：compact(默认·桌面高密度,= :root 基线) | regular | comfy。
D-COLOR   青(robot logo 同源)= 品牌 + 唯一交互焦点(边框激活/标题/选中行/focus)。
          金 = viden 字标 + 工作模式 + 「需要人」(门控/待审批)。语义四色 success/warning/error/progress。
          填充芯片上的文字/图标 = `--on-accent`(随 mode 翻黑/白)；禁再借 `--bg-void` 当墨色。
          ⚠ 颜色 = 状态语义(T4)；图表/装饰不得硬编码色、须随 theme(已落实:决策中心图表走 `var(--fg-muted)`前/`--accent`后·O-B8 done)。
D-BACKEND lane 后端三分(TUI/GUI 同契约·后端 chip 铁律·颜色跨轨固定):
          ACP=--accent 青(桥接外部 CLI·codex/claude-code/gemini) · built-in=--builtin 紫(直调模型·可路由)
          · tmux=--gold 金(附着已有 tmux pane/session)。TUI chip = .vbe.acp/.builtin/.tmux(格式 <类型>:<agent>)；
          GUI lane = .viab(色 = VIA_COLOR[via])。⚠ GUI 曾误把 built-in 映成金·已改回紫、tmux 归金(D1 VIA_COLOR + D4 AGENTS 补齐)。O-A3 收口。2026-07-01。
D-A11Y    对比护栏(6 套皮肤逐套满足 WCAG):fg-primary≥7 · fg-secondary≥4.5 · fg-muted≥4.5(正文)
          · fg-faint≥3(仅大字/UI描边、非正文) · accent/gold≥4.5 · on-accent 压所有填充色≥4.5。
D-TYPE    UI/正文 = Inter + Noto Sans SC；代码/终端 = JetBrains Mono；终端态全程等宽对齐。
          CJK 正文行高 1.55–1.7；等宽态中文宽字符占两格。
D-PLAT    桌面 · 高密度 cockpit(多栏)。TUI = 方硬(圆角 ~3px)/无阴影/box-drawing 边框/对齐等宽单元格；
          GUI = 圆润(--r-md/lg)+ --shadow-lg、鼠标命中 ≥28px。
D-I18N    界面中英双语。跨页复用文案 → i18n-dict.js(集中词典) 用 tk()/data-i18n-key；
          页面独有长句 → 内联 t(en,zh)。每页双语为目标(渐进铺开·index.html 为范式)。
D-STATUS  屏幕状态唯一真源 = docs/screens-status.js(机读)；门户 index.html 运行时直读。
          加/删/改屏只改它。🤖 check-status.js 守。
D-SOT     单一真源铁律：数值只在 tokens.css；组件样式在 <p>-kit.css；旗舰原型一份(不复制驾驶舱代码)；
          共享脚本(i18n/chrome/tweaks)根目录各一份。冲突以真源为准。
D-PROTO   平台结构 = 全屏产品入口 + 组件库 + 设计稿索引 + pages/ + kit(见 PROTO-STANDARD)。
          索引页 iframe 走「无白屏导航引擎」(PROTO-STANDARD §3b)，不退化成 display:none。
D-COMP    新组件准入：DESIGN-REF 有条目(类名+最小 HTML)才算「可复用」；没登记 = 临时草稿。
          先 grep DESIGN-REF / 现有 class 再写，命中就抄，别重造已沉淀组件。
D-ICON    GUI 图标单一真源 = GUI/gui-icons.jsx(window.ICONS 线性图标 + AgentLogo 品牌徽标 + GuiIcon 取用)。
          rail / 视图 / 标题栏工具一律取 {ICONS[key]}；换 class/尺寸用 <GuiIcon name= className= style=>。
          @DECISION 同一概念一套画法：worktree/lanes/review 等以 D1 驾驶舱(视觉真源)画法为准，各页不得各画各的
          (lanes = swimlanes，非 hamburger)。AgentLogo 用品牌固定色(codex 绿/claude 橙/kiro 紫/viden 青金)，刻意不随主题换肤。
          GUI 禁 emoji——要图标走 gui-icons 线性 SVG(lock/robot/game/ml 已替 🔒🤖🎮📈)。
          新图标先在 DESIGN-REF「GUI 图标目录」登记 key + 语义再用；没登记 = 临时草稿。gui-icons.jsx 须先于 gui-titlebar / 页面主脚本加载。
          🤖 check-icons.js 守(扫 GUI/：页内联 `<svg class="ic">` / `VRAIL_ICONS` 注册表 / emoji 即 FAIL；baseline grandfather 组件库 showcase)。
D-GLYPH   TUI 图标 = 等宽栅格内联 Unicode 几何字形（不抽代码模块——终端真源 + ANSI-256 降级）。
          单一真源 = DESIGN-REF「TUI 字形词表」+ T4 §08（色=状态、全局固定）。
          ◆待你 / ▶run / ✓done / ▣skill / ◌wait / ⏸gate / ✗fail——色承载语义，donor emoji 1:1 换几何字形。
          TUI 禁 emoji（各终端/字体渲染不一、撑破等宽栅格）。新字形先登记词表再用。🤖 check-tui-glyphs.js 守。
D-PERM    权限 ≠ 闸（两件事·都落审计流）。模型真源 = 本条；渲染样板 TUI=T3、GUI=gui-kit `.gperm`。
          permission：执行前拦「能不能做这个动作」，阻塞此会话、就地弹，不进决策中心。
          gate：产出后拦「批不批合入」，异步攒进决策中心（D2 / 画廊 / 契约 / 集成）。危险动作两者都走。
          四类 permission：① shell 命令(高危·含递归删除/导出) ② 写受管路径(中·改后还触发契约闸)
            ③ 网络 / MCP 工具(中·出网记审计) ④ 远程 target(高·实机·e-stop 前置)。
          范围分级：y 仅此次 · a 本会话此类 · ⇧A 始终(写 viden.toml allowlist，策略变更本身走契约闸)
            · e 编辑命令后允许 · n/esc 拒绝。
          策略内免打扰：读文件 / 写自己 worktree / allowlist 命令由 viden.toml 预批，只有越界才弹。
          远程·实机升级：危险动作强制 e-stop 前置，field 级需双人批(co-sign)，禁「始终允许」。
          无人值守(nightly/daemon)：越权动作默认拒绝并入队(不自动放行)，回来在收件箱处理。
          色：permission = 金「需要人」(D-COLOR)；风险 hi/md/lo = error/gold/success；填充墨 = on-accent。
D-NAME    产品名定档 = Viden(2026-07-02)；代码侧(RoboCode v0.1.30)改名跟设计。映射唯一真源 =
          docs/NAMING-MAP.md(字标/配置路径/env 前缀/分支前缀 vd·含门控类型↔UI 标签 看板#21)。
          品牌字符串单一真源 = i18n-dict.js 的 window.VIDEN_BRAND；新稿禁再散写字面量。
          PermissionMode 枚举 cli 名(default/acceptEdits/bypassPermissions/dontAsk/plan)↔UI 标签
          (Ask/Auto Edit/Full Access/Don't Ask/Read Only)以 NAMING-MAP §2 为准。
D-SETTINGS 设置/偏好 = D1 驾驶舱第 8 视图(view==='settings'·沿 view-state 路由·不新开窗口)。
          单一真源 = GUI/gui-settings.jsx(<SettingsView/>·样式自注入 .gset-)。7 节：Provider&Models /
          Permissions(引擎枚举) / Appearance(真接 RC) / Keyboard / Notifications(含 #13 离桌兜底) /
          Privacy&Telemetry / Workspace。非引擎项界面标「GUI 层」；即改即存无 Save。
D-AUDIT   审计 ≠ 证据：审计 = 决策链(谁何时以何理由授权了什么·permission 批拒/闸裁决/合入/模式与
          策略变更/lane 生命周期)，工作区级只追加、锚 git、可导出 —— 载体 = D14；
          证据 = lane 工作产物(测试/diff/截屏) —— 载体 = D1 Evidence 视图。审计行链接证据，不反向。
          策略自动动作(RuleAllow 等)折叠汇总进审计，不逐条刷屏。
D-ROLLBACK 合入后回滚(看板#12) = 走同一条闸链的正向新动作： git revert -m 1 反向 commit +
          回放基线同步回退 + revert 本身过集成闸；禁改写历史。原 lane 带 revert 上下文重开向前修。
          渲染样板 = D12「合入后回滚」节。
D-NOTIFY  离桌拉回(看板#13)：桌面内 = Pip/收件箱/状态栏金徽标；离桌兜底 = email + webhook，
          只升级「需要人」四类(gate 待审/高危 permission/lane 连败停机/集成闸退回)且超时才发；
          quiet hours 只留 webhook。IM 直连 = V2 roadmap。载体 = 设置屏 Notifications 节。
D-POPOUT  popout 独立窗口顶栏单一真源 = gui-titlebar.jsx <GuiPopoutBar>(.winbar·度量同 gui-kit)；
          各页禁再手抄 dots/字标。召唤坞实现真源 = D1 内置 DockSD；D2h/D3 降为概念稿(只探版式·不再扩展)。
          ⚠ DockSD 此前只在本条被指名、DESIGN-REF 从未登记 = 治理不一致(SPEC 说它是实现真源,
          准入护栏 D-COMP 却判它临时草稿)；已补登记并带 roadmap 标(D-RAILNAV ⑥)。2026-09-08。
D-SIDEBAR 左侧 lanes 侧栏双模式(D1 驾驶舱)：float(默认·hover 峰显,水平空间让给转录) | pinned(占布局列·
          可拖宽 176–360,默认 218)。同一 <ThreadSidebar/> 组件,只换宿主——两态内容/结构零分叉。
          float 触发 = 活动栏右缘 12px 热区(.edgewrap.l+.edgehint 青提示条),移入滑出、移出延时 ~700ms 收起。
          切换入口单处：活动栏底部 pin 按钮(设置⚙上方)。(原「侧栏顶部 .pinbtn」第二入口从未渲染,
          死 CSS 已删——编码侧核查 2026-07-02。)focus 专注模式覆盖此偏好 →
          两侧强制 hover 浮窗(退出恢复)。持久化 localStorage：vd-leftmode(float|pinned) + vd-leftw(宽度)。
D-ROADMAP screens-status 的 roadmap:true = 路线图屏(引擎无后端·非 v1 交付)：D7/D8/D9。
          BUILT 只说设计建成，不承诺可开工；索引页渲染虚线 ROADMAP 徽标。
D-COPY    高保真原型零描述性文案(2026-07-02)：产品入口(D1 驾驶舱 / TUI 统一原型及其组件)只写
          产品微文案；设计说明/规范注(看板编号·映射表·「为什么这样设计」)只允许出现在演示稿
          (pages/ 设计稿的 pkicker/ph1/plead)与代码注释。单一真源：屏级说明 = screens-status.js
          meta/note；机制说明 = SPEC/DESIGN-REF —— 删时不另拷副本。产品微文案一律 t(en,zh) 双语
          (D-I18N)，禁中英同屏混排；语言 = 设置项(viden-lang · 设置屏 Appearance · Language)。
          豁免(有意 EN-only·不算混排)：HUD/状态栏缩写(MODE/PERM/CTX…)、枚举与术语名
          (Build/Ask/Once 档位名、cli_name、类名)、键位字母(Y/A/E/N)、代码与命令内容 —— 句子级文案不豁免。
D-STATUSBAR 状态栏 = 单行 HUD,三区,钉/滚由「可操作性」定(不是紧急程度)。跨轨同契约。
          左固定(身份)：project · lane · backend · mode —— 永不轮播。
          右固定(待处理)：gate ⏸N(可点→决策中心) · error ✗(钉到清除) · 权限/问询 —— 永不轮播。
          中段(环境)：留空(`.vsp`) 或 ticker 轮播长尾(运行数·ctx/预算·训练进度·上条事件·memo·日程);
            悬停/聚焦暂停,点某段跳去处理。**可操作项绝不进 ticker**(滚走就点不到 → O-B6 根因)。
          宽度阀：窄屏 ticker 是溢出阀 —— 先丢最不重要的环境项,钉住的不动。
          reduced-motion：不自动滚 → 显精简汇总 / 按键步进;信息靠文字+色仍完整。
          组件：TUI=tui-kit `.vstatus`(`.vseg.proj/.lane/.r` 固定 + `.vgate-badge` 钉闸 + `.vticker` 环境);
            GUI=gui-kit `.statusbar`(`.sb-right` 钉闸 + `.sb-tickwrap` 环境) / `<GuiStatusBar>`。颜色语义沿用 D-COLOR/T4。
D-GATESTR gate_strength = lane 契约一等事实(随 route 派生但独立字段·跨轨常显)：让人对「这条 lane 的输出可以多信」有直觉。
          full=native/built-in 逐工具调用拦截 · cooperative=ACP(session/request_permission 建议性·须 worktree 兜底)
          · containment=terminal/tmux(无法逐调用拦截·worktree 围栏 + 退出 diff 兜底)。
          渲染：填充级字形 ●full/◐coop/○containment(实心→空心=门越来越软)；色随 route(D-BACKEND)、义随填充。
          GUI 载体：D1 `.viab .gs`(route chip 内前缀)、D10 `.lc .gsb`、D13 GNode `.gsb`(glyph+FULL/COOP/CTN)。fleet/监视视图常显。
          来源 = robocode agent-orchestration-review 2026-07-04 §1.4.3 / §3.1.4。
D-MERGEGATE MergeGate 持久状态机(唯一真源·对齐 robocode frontend-integration-contract)：
          proposed → collecting_evidence → (blocked | needs_changes) → accepted → (merged | reverted)。
          五类一等 required evidence：patch · test_result · review · doc_update · release_artifact。
          status 由已记录 evidence 的 kind 归约(core reducer·非前端本地勾选/id 后缀)：缺必备=collecting_evidence；
          全满足→自动 accepted；evidence 被 reject→needs_changes 并移除该 id。AcceptAgentArtifact 只接受已记录 id。
          渲染样板 = gui-kit `.mgate`(required vs collected 清单)；载体 D12/D2。verifier 须独立 session、默认拒绝、verdict 附可复核 evidence。
D-MUTPOLICY mutation_policy = 与 route(D-BACKEND)正交的每-lane 字段：autonomous / propose-only / read-only。
          propose-only = 原「manual review」语义(逐条提议·未经人批不落地)，可与任何 route 组合(含 ACP/terminal)。
          read-only = 无任何 mutation；Plan 模式强制 read-only。route 四选一 + mutation_policy 三选一，不再把 manual review 当第五条 route。
          载体 = D4 步骤4(Gates & target)。来源 = review §1.1 / §3.3。
D-BUDGET-BLIND 外部 CLI 成本盲区：terminal/tmux(containment)lane 跑 Claude Code/Codex 等外部 CLI 时，
          Viden 看不到 provider token 消耗 → 费用面板须显式标「计量盲区」，禁用估值冒充精确费用；
          用代理指标兜底(wall-clock runtime · 运行次数 · diff 大小 · exit code)。ACP lane 有 usage_update → 不盲。
          渲染 = D1 `.envctx` blind 分支(`.blindtag/.blindwhy/.blindprox`)。来源 = review §1.4.5。
D-ROLES 7 个内置角色(runtime 唯一真源)：planner · coder · reviewer · tester · doc-writer · researcher · release-operator。
          曾用名映射 editor→coder、writer→doc-writer(GUI 稿已改)。orchestrator/context-builder/lane-supervisor = runtime 组件、非角色(不进清单·可作 fleet 节点)。
          task 状态枚举(AgentTaskRecord.status)UI 只映射不猜：进行中/等待(waiting_approval·needs_input·blocked=一等「需要人」)/完成/失败——
          现有视觉码 run/done/gate/idle/wait 是其显示分组，勿另立枚举。词表详见 DESIGN-REF「内置角色 & task 状态词表」。来源 = review §1.2.3。
D-RAILNAV 活动 rail = 路由器(2026-09-08 定档)：rail 按钮导航到独立 D 屏(D12/D2/D14/D10/D13)，
          此为已接受的实现模型；D1 旗舰保留页内视图切换作**设计探索**，实现目标 = 独立 D 屏 +
          下面登记的面。旗舰五个次级面逐面裁决(「登记」= 进 DESIGN-REF 组件目录·准入护栏 D-COMP)：
          ① DiffReview **登记**(`.review/.filetree/.diffpane/.commitbar`)：文件树 + 暂存 + 统一 diff +
            提交栏；定为操作者 git 动作 / 结构化 diff / 冲突内容的计划宿主
            (GUI-CORE-020/012/015)。实现目标 0.3.3。
          ② EvidenceView **登记**(`.evwrap/.evmain/.evdet`)：按天分组证据时间线 + 详情 + 关联芯片；
            读侧先行，配 D-AUDIT「审计行链接证据、不反向」的单向链。实现目标 0.3.3(排在 DiffReview 之后)。
          ③ WorktreeBoard **不登记**、并从实现目标中拿掉：lane↔worktree 二元同一 → 与 lane 监视(D10)
            重复；其独有动作归 DiffReview 与 D12。原型 markup(`.wtwrap`)留在旗舰作探索，不是实现目标。
          ④ 单 lane 内嵌 subagent 树 **延后**到 fleet 家族(0.3.4+)，契约依赖同 D13；现不登记。
          ⑤ DiagnosticsView **登记但带 deferred 标**(`.dgwrap/.dgmain/.dgdet`)：实现目标 0.3.4 候选，
            待 lsp→契约接线勘查。
          ⑥ DockSD 召唤坞 **登记但带 roadmap 标**(`.sdock/.sdtabs/.sdsummon`·0.3.4+/V2)：收口
            D-POPOUT 指名它为召唤坞实现真源、DESIGN-REF 却从未登记的治理不一致，两处自此一致。
          ⚠ 登记只说「组件可复用」(D-COMP)，不承诺可开工；deferred/roadmap 标沿用 D-ROADMAP 语义
            —— 实现目标版本是排期，不是交付承诺。
```

## @OPEN 开放问题（详情/进度在 Core「设计审查看板」·此处只做 grep 索引）

```
分类：A=三轨矛盾 · B=单轨矛盾 · C=设计漏洞 · D=产品忽略点 · E=一致性收尾
O-A1 done  语言策略定档(2026-07-02)：**交互稿 = 双语**(t()/tk · 已铺 TUI/GUI 全部交互页,末尾 D3 本轮补齐)；
           **Core 文档型页 = zh-primary 有意**(产品方案/机制图/品牌页是中文叙事文档,不再列债)。
           品牌字符串走 VIDEN_BRAND(D-NAME)。自此 O-A1 关闭。
O-A2 done  Core「Logo & TUI 品牌」定性 = **品牌概念展示页**(2026-07-02)：不跟 TUI 产品 chrome、
           不追统一原型的 2 行 tab 布局；页内已加显式标注,旧 mock 作品牌气质参考保留。
           TUI 产品 chrome 真源仍 = 统一原型。自此 O-A2 关闭。
O-A3 done  后端模型不一致(GUI 只 ACP/built-in、无 tmux)：已立 @DECISION D-BACKEND。GUI 补齐三后端——
           D1 VIA_COLOR 改回 ACP=青/built-in=紫/tmux=金 + L2 改 tmux lane + 新建 tmux agent 组 + lane-proto 注；
           D4 AGENTS 加 tmux bridge 选项、步骤2 文案改「ACP / built-in / tmux 三后端」。2026-07-01。
O-A4 done  权限提示 permission(执行前)≠gate(产出后)：模型已升 @DECISION D-PERM；GUI 此前只有 gate、
           无 permission UI，现用 gui-kit `.gperm` 补齐(D1 旗舰把含糊的 APPROVAL 模态改为纯 permission
           提示·动作+理由+风险+范围分级；diff/评审归 gate)。2026-06-30 收口。
O-A5 done  决策中心命名已收敛：活跃页统一「决策中心 / Decision Center」(D2/D10/D9/T4/opencode/
           screens-status)；“routing foyer” 活跃文件零命中；“行为 Diff 评审” 仅存 ARCHIVED 存档屏标签 +
           产品方案 v2 一句血缘注 —— 皆有意历史、非活跃漂移。2026-06-30。
O-B6 done  状态栏自相矛盾(gate 滚进 marquee)：已立 @DECISION D-STATUSBAR(可操作项恒钉·绝不进 ticker)。
           核查 T1/T1b/T1d/T2/统一原型 + D1 + 两组件库:gate 一律 `.vgate-badge`/`.sb-right` 钉右、ticker 仅环境
           —— 全轨实已合(T1d 现钉 gate·非滚),仅缺护栏;本次补 D-STATUSBAR 收口。2026-06-30。
O-B7 done  两套状态栏实为一套:T1 中段留空(`.vsp`)、T1d 中段轮播(`.vticker`),两者都钉 proj/lane/backend + gate。
           收进 D-STATUSBAR(中段 = 空 或 ambient-only ticker·可操作恒钉)。2026-06-30。
O-B8 done  D2 图表硬编码色：活跃「决策中心」图表已全 token(`var(--fg-muted)` 改动前 / `--accent` 改动后 / `--border-soft`
           网格，随 theme 切换·0 裸色)。唯一硬编码图表色(15 hex)在 **ARCHIVED** 「D2 行为Diff评审 存档」
           —— 冻结快照·baseline 接受·不进现用,为有意例外。2026-06-30。
> 全表 23 条(含 C/D/E)见 Core/「Viden - 设计审查看板 (Core).html」；定了的决策回写本文件 @DECISION。
```

## @GREP 锚点速查（动哪类东西先 grep）
```
改 token / 颜色      → tokens.css(真源) + DESIGN-REF「Token 速查」+ grep 'var(--' 命中复用
造 / 改组件          → DESIGN-REF「组件目录」+ grep 类名(.v* / .gui-*)；登记后才算可复用
加 / 删 / 改屏        → docs/screens-status.js(真源) → 跑 check-status.js
跨页复用文案         → i18n-dict.js + grep 'tk(' / 'data-i18n-key'
平台结构 / 索引导航   → docs/PROTO-STANDARD.md(§3b 无白屏引擎)
某元素「为何长这样」  → grep '<类名>' docs/CHANGELOG.md(查病史·勿顺手「统一」掉有意特例)
```
