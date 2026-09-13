/* ============================================================
   Viden · 屏幕状态【唯一真源】（machine-readable）
   ------------------------------------------------------------
   这是所有 Core / TUI / GUI 设计稿「画到哪了」的唯一真源。
   门户 index.html 运行时直读它渲染卡片 + 进度 —— 加 / 删 / 改屏
   只改这里（见 CLAUDE.md DoD「加/删/改屏」一行）。

   · state 枚举：
       PLANNED   仅规划（未起稿）
       WIP       在画
       BUILT     已建可用
       REVIEWED  已过评审
       ARCHIVED  存档（旧版/早期方案，仍可查；不计入进度分母）
       DROPPED   废弃（ID 保留不复用，不计入进度分母）
   · kind 枚举（构建载体，门户卡片右上角徽标）：
       kit       已接组件套件（tui-kit / gui-kit · 单一真源）
       inline    仍内联自造（迁移目标）
       doc       文档/展示页（无 kit 概念）
       arch      存档载体
   · cursor：当前里程碑（下一步）的 screen id；空 = 无下一步（打磨/评审期）。
   · file：可点进的 HTML 路径（相对项目根）。
   · code：门户卡片左上角短码 / 字形（沿用历史展示码）。
   · nm/meta = 中文；nmEn/metaEn = 英文（门户随 i18n 切换）。
   · note：AI / 开发详注（可空）。
   ★ 改这里后跑 tools/check-status.js（防悬空 file / 重复 id / 非法枚举）。
   ============================================================ */
window.VIDEN_STATUS = {
  updated: "2026-09-13",
  cursor: "",   /* Core/TUI/GUI 三轨主屏均已建 · 进入跨轨收敛与评审打磨期（见 设计审查看板） */

  /* 轨道元信息（门户分组头直读，不再硬编码） */
  tracks: [
    { id: "Core", nm: "核心 · 视觉真源 & 文档", nmEn: "Core · visual source & docs", meta: "visual source of truth" },
    { id: "TUI",  nm: "终端版 · cockpit 等宽栅格", nmEn: "TUI · terminal cockpit",      meta: "monospace cockpit · 单一入口" },
    { id: "GUI",  nm: "桌面版 · Rust + Tauri 多栏", nmEn: "GUI · desktop (Tauri)",       meta: "desktop · Tauri" },
  ],

  screens: [
    /* ── Core ───────────────────────────────────────────── */
    { id: "C-THEME", track: "Core", code: "THEME", state: "REVIEWED", kind: "doc",
      file: "Core/Viden - Aurora 主题 (Core).html",
      nm: "Aurora 主题", nmEn: "Aurora Theme",
      meta: "配色/层级/组件展示页 · 6 套皮肤实时切换",
      metaEn: "Color/hierarchy/component showcase · 6 live skins",
      note: "视觉真源展示页；6 皮肤经自带 nav + Tweaks 切换（共用 rc-* localStorage 键）" },
    { id: "C-MECH", track: "Core", code: "MECH", state: "BUILT", kind: "doc",
      file: "Core/Viden - Lane 协作机制图 (Core).html",
      nm: "Lane 协作机制", nmEn: "Lane Collaboration",
      meta: "多 agent lane 编排与门控机制图",
      metaEn: "Multi-agent lane orchestration & gating diagram", note: "" },
    { id: "C-PLAN", track: "Core", code: "PLAN", state: "BUILT", kind: "doc",
      file: "Core/Viden - 产品方案 v2 (Core).html",
      nm: "产品方案 v2", nmEn: "Product Plan v2",
      meta: "产品定位、功能与场景方案",
      metaEn: "Product positioning, features & scenarios", note: "" },
    { id: "C-LOGO", track: "Core", code: "LOGO", state: "BUILT", kind: "doc",
      file: "Core/Viden - Logo & TUI 品牌 (Core).html",
      nm: "Logo & TUI 品牌", nmEn: "Logo & TUI Brand",
      meta: "字标 / 机器人吉祥物 / 终端品牌",
      metaEn: "Wordmark / robot mascot / terminal brand",
      note: "定性 = 品牌概念展示页(SPEC O-A2 done 2026-07-02)：不跟 TUI 产品 chrome；旧 mock 作品牌气质参考保留，页内已标注" },
    { id: "C-BRAND", track: "Core", code: "BRAND", state: "BUILT", kind: "doc",
      file: "Core/Viden - 品牌资产 (Core).html",
      nm: "品牌资产", nmEn: "Brand Assets",
      meta: "图标 / favicon / OG banner 一览",
      metaEn: "Icons / favicon / OG banner", note: "" },
    { id: "C-REVIEW", track: "Core", code: "REVIEW", state: "BUILT", kind: "doc",
      file: "Core/Viden - 设计审查看板 (Core).html",
      nm: "设计审查看板", nmEn: "Design Review Board",
      meta: "三轨终审看板：矛盾 / 漏洞 / 忽略点 / 收尾",
      metaEn: "Cross-track final-review kanban",
      note: "23 条跨轨 / 单轨矛盾 + 设计漏洞 + 产品忽略点 + 一致性收尾" },
    { id: "C-ARCH", track: "Core", code: "ARCH", state: "ARCHIVED", kind: "arch",
      file: "Core/_archive/Viden - 产品方案 v1 存档 (Core).html",
      nm: "产品方案 v1", nmEn: "Product Plan v1",
      meta: "早期方案存档", metaEn: "Early plan, archived", note: "已被 v2 取代；ID 保留不复用" },

    /* ── TUI ────────────────────────────────────────────── */
    { id: "T-COCKPIT", track: "TUI", code: "★", state: "BUILT", kind: "kit",
      file: "TUI/Viden - 统一原型 (TUI).html",
      nm: "TUI 原型 · 驾驶舱(打开即用)", nmEn: "TUI prototype · live cockpit",
      meta: "全屏交互驾驶舱：欢迎屏→lane→闸→命令面板，⌃L/⌃P/⌃G 可操作 —— 像真在用 Viden TUI",
      metaEn: "Full-screen interactive cockpit — like actually using Viden TUI",
      note: "唯一 TUI 原型入口；接 tui-kit `.v*`；左下角 ▤ 通往设计稿索引" },
    { id: "T-KIT", track: "TUI", code: "◳", state: "BUILT", kind: "kit",
      file: "TUI/Viden - 组件库 (TUI).html",
      nm: "组件库", nmEn: "Component kit",
      meta: "tui-kit 组件逐件陈列 · 查阅/复制(打开即用)",
      metaEn: "Component kit showcase — browse & copy", note: "" },
    { id: "T-INDEX", track: "TUI", code: "▤", state: "BUILT", kind: "kit",
      file: "TUI/Viden - 设计稿索引 (TUI).html",
      nm: "设计稿索引", nmEn: "Design docs index",
      meta: "侧栏导航 + iframe 保活秒切：T0–T5 / 会话 / 借鉴等设计稿",
      metaEn: "Sidebar nav + iframe: all TUI design screens",
      note: "从原型左下角 ▤ design docs 进；iframe 保活缓存秒切" },

    /* ── GUI ────────────────────────────────────────────── */
    { id: "G-COCKPIT", track: "GUI", code: "★", state: "BUILT", kind: "kit",
      file: "GUI/Viden - 桌面驾驶舱 (GUI).html",
      nm: "驾驶舱 · 打开即用", nmEn: "Cockpit · the product",
      meta: "全屏窗口化桌面驾驶舱：拖拽/四边缩放/最大化 · 活动 rail 切视图 · 6 皮肤换肤 —— 打开即用",
      metaEn: "Full-screen windowed desktop cockpit — open & use",
      note: "GUI 视觉真源 = D1；gui-kit 是它的镜像，改 D1 chrome 须同步 gui-kit" },
    { id: "G-KIT", track: "GUI", code: "◳", state: "BUILT", kind: "kit",
      file: "GUI/Viden - 组件库 (GUI).html",
      nm: "组件库", nmEn: "Component kit",
      meta: "gui-kit 组件逐件陈列 · 查阅/复制(打开即用)",
      metaEn: "GUI canonical kit showcase — browse & copy", note: "" },
    { id: "G-INDEX", track: "GUI", code: "▤", state: "BUILT", kind: "kit",
      file: "GUI/Viden - 设计稿索引 (GUI).html",
      nm: "设计稿索引", nmEn: "Design docs index",
      meta: "侧栏导航 + iframe 秒切：D1–D16 全部 GUI 设计稿",
      metaEn: "Sidebar nav + iframe: all GUI design screens", note: "" },
  ],

  /* ── 设计稿（doc 级）─────────────────────────────────────
     各平台「设计稿索引」侧栏 NAV 的唯一真源（此前硬编码在两个索引页里·已收敛）。
     索引页运行时按 track 过滤、按 group 分组渲染；门户 index.html 不渲染这些。
     file = 相对项目根的完整路径（索引页自行 strip 掉 <track>/ 前缀作 iframe src）。
     nav = NAV 短码；kind = 徽标(kit/ref/star)。改/加设计稿改这里。
     roadmap = true → 路线图屏(引擎无后端·非 v1 交付)：设计完整保留,但 BUILT ≠ 可开工；
       索引页渲染虚线 ROADMAP 徽标(编码侧误读拦截 · 评审反馈 2026-07-02 第3项)。
     ★ 旗舰 D1/组件库同时是 portal 卡(screens[])又是索引导航项 = 有意的导航快捷，
       file 在两处都出现属预期（check-status.js 只在同数组内查 file 重复）。 */
  docs: [
    /* —— TUI —— */
    { track:"TUI", nav:"T0",  group:"总览",          groupEn:"Overview",         state:"BUILT", kind:"kit", nm:"设计总览",        nmEn:"Design overview",    file:"TUI/pages/Viden - T0 设计总览 (TUI).html" },
    { track:"TUI", nav:"T1",  group:"会话",          groupEn:"Session",          state:"BUILT", kind:"kit", nm:"会话主屏 · 多 lane", nmEn:"Session · multi-lane", file:"TUI/pages/Viden - T1 会话主屏与多lane (TUI).html" },
    { track:"TUI", nav:"OPC", group:"会话",          groupEn:"Session",          state:"BUILT", kind:"kit", nm:"会话页 · opencode", nmEn:"Session · opencode", file:"TUI/pages/Viden - 会话页 opencode 借鉴 (TUI).html" },
    { track:"TUI", nav:"T1b", group:"会话",          groupEn:"Session",          state:"BUILT", kind:"kit", nm:"侧栏探索",        nmEn:"Sidebar",            file:"TUI/pages/Viden - T1b 侧栏探索 (TUI).html" },
    { track:"TUI", nav:"T1c", group:"会话",          groupEn:"Session",          state:"BUILT", kind:"kit", nm:"输入与模型切换",   nmEn:"Input & model",      file:"TUI/pages/Viden - T1c 输入与模型切换 (TUI).html" },
    { track:"TUI", nav:"T1d", group:"会话",          groupEn:"Session",          state:"BUILT", kind:"kit", nm:"状态栏滚动",      nmEn:"Status ticker",      file:"TUI/pages/Viden - T1d 状态栏滚动 (TUI).html" },
    { track:"TUI", nav:"T2",  group:"交互",          groupEn:"Interaction",      state:"BUILT", kind:"kit", nm:"全局跳转",        nmEn:"Global jump",        file:"TUI/pages/Viden - T2 全局跳转 (TUI).html" },
    { track:"TUI", nav:"T3",  group:"交互",          groupEn:"Interaction",      state:"BUILT", kind:"kit", nm:"对话工作状态",     nmEn:"Work states",        file:"TUI/pages/Viden - T3 对话工作状态 (TUI).html" },
    { track:"TUI", nav:"T4",  group:"交互",          groupEn:"Interaction",      state:"BUILT", kind:"kit", nm:"交互规则",        nmEn:"Interaction rules",  file:"TUI/pages/Viden - T4 交互规则 (TUI).html" },
    { track:"TUI", nav:"T5",  group:"保真 & 组件",    groupEn:"Fidelity & Kit",   state:"BUILT", kind:"kit", nm:"真终端保真对比",   nmEn:"Fidelity check",     file:"TUI/pages/Viden - T5 真终端保真对比 (TUI).html" },
    { track:"TUI", nav:"REF", group:"借鉴",          groupEn:"Reference",        state:"BUILT", kind:"ref", nm:"Charm · HUD",     nmEn:"Charm · HUD",        file:"TUI/pages/Viden - Charm 与 HUD 借鉴 (TUI).html" },
    { track:"TUI", nav:"REF", group:"借鉴",          groupEn:"Reference",        state:"BUILT", kind:"ref", nm:"opencode · hermes", nmEn:"opencode · hermes", file:"TUI/pages/Viden - opencode 与 hermes 借鉴 (TUI).html" },

    /* —— GUI（旗舰在 GUI/ 根，其余在 GUI/pages/）—— */
    { track:"GUI", nav:"D1",  group:"旗舰",          groupEn:"Flagship",         state:"BUILT", kind:"star", nm:"驾驶舱 · 打开即用", nmEn:"Cockpit · the product", file:"GUI/Viden - 桌面驾驶舱 (GUI).html" },
    { track:"GUI", nav:"KIT", group:"旗舰",          groupEn:"Flagship",         state:"BUILT", kind:"star", nm:"组件库",          nmEn:"Component library",  file:"GUI/Viden - 组件库 (GUI).html" },
    { track:"GUI", nav:"D15", group:"规划",          groupEn:"Plan",             state:"BUILT", kind:"kit", nm:"Plan Studio",     nmEn:"Plan Studio",        file:"GUI/pages/Viden - D15 Plan Studio (GUI).html" },
    { track:"GUI", nav:"D2",  group:"决策",          groupEn:"Decision",         state:"BUILT", kind:"kit", nm:"决策中心",        nmEn:"Decision Center",    file:"GUI/pages/Viden - D2 决策中心 (GUI).html" },
    { track:"GUI", nav:"D2a", group:"决策",          groupEn:"Decision",         state:"ARCHIVED", kind:"ref", nm:"行为 Diff 评审 · 存档", nmEn:"Behavior diff · archive", file:"GUI/pages/Viden - D2 行为Diff评审 存档 (GUI).html" },
    { track:"GUI", nav:"D11", group:"接入与创建",     groupEn:"Onboarding",       state:"BUILT", kind:"kit", nm:"首启与项目接入",   nmEn:"First-run onboarding", file:"GUI/pages/Viden - D11 首启与项目接入 (GUI).html" },
    { track:"GUI", nav:"D4",  group:"接入与创建",     groupEn:"Onboarding",       state:"BUILT", kind:"kit", nm:"Lane 创建流程",   nmEn:"New Lane flow",      file:"GUI/pages/Viden - D4 Lane创建流程 (GUI).html" },
    { track:"GUI", nav:"D5",  group:"评审与协作",     groupEn:"Review",           state:"BUILT", kind:"kit", nm:"画廊评审",        nmEn:"Gallery review",     file:"GUI/pages/Viden - D5 画廊评审 (GUI).html" },
    { track:"GUI", nav:"D7",  group:"评审与协作",     groupEn:"Review",           state:"BUILT", kind:"kit", roadmap:true, nm:"团队收件箱与简报", nmEn:"Team inbox & briefing", file:"GUI/pages/Viden - D7 团队收件箱与简报 (GUI).html" },
    { track:"GUI", nav:"D8",  group:"评审与协作",     groupEn:"Review",           state:"BUILT", kind:"kit", roadmap:true, nm:"团队身份与权限",   nmEn:"Team & permissions", file:"GUI/pages/Viden - D8 团队身份与权限 (GUI).html" },
    { track:"GUI", nav:"D12", group:"评审与协作",     groupEn:"Review",           state:"BUILT", kind:"kit", nm:"集成闸冲突退回",   nmEn:"Integration bounce", file:"GUI/pages/Viden - D12 集成闸冲突退回 (GUI).html" },
    { track:"GUI", nav:"D14", group:"评审与协作",     groupEn:"Review",           state:"BUILT", kind:"kit", nm:"审计与时间线",     nmEn:"Audit & timeline",   file:"GUI/pages/Viden - D14 审计与时间线 (GUI).html" },
    { track:"GUI", nav:"D10", group:"监视与编排",     groupEn:"Monitor",          state:"BUILT", kind:"kit", nm:"Lane 监视器",     nmEn:"Lane monitor",       file:"GUI/pages/Viden - D10 Lane监视器 (GUI).html" },
    { track:"GUI", nav:"D13", group:"监视与编排",     groupEn:"Monitor",          state:"BUILT", kind:"kit", nm:"Fleet 编排与 Workflow", nmEn:"Fleet & Workflow", file:"GUI/pages/Viden - D13 Fleet 编排与 Workflow (GUI).html" },
    { track:"GUI", nav:"D16", group:"监视与编排",     groupEn:"Monitor",          state:"BUILT", kind:"kit", nm:"Agent 看板",      nmEn:"Agent Board",        file:"GUI/pages/Viden - D16 Agent Board (GUI).html" },
    { track:"GUI", nav:"D6",  group:"系统态",         groupEn:"States",           state:"BUILT", kind:"kit", nm:"空态与错误态",     nmEn:"Empty & error states", file:"GUI/pages/Viden - D6 空态与错误态 (GUI).html" },
    { track:"GUI", nav:"D9",  group:"远程",          groupEn:"Remote",           state:"BUILT", kind:"kit", roadmap:true, nm:"远程目标开发",     nmEn:"Remote target dev",  file:"GUI/pages/Viden - D9 远程目标开发 (GUI).html" },
    { track:"GUI", nav:"D2h", group:"召唤坞 (概念稿)", groupEn:"Summon (concept · source = D1 dock)", state:"BUILT", kind:"ref", nm:"横屏召唤坞",   nmEn:"Summon dock · H",    file:"GUI/pages/Viden - D2 横屏召唤坞 (GUI).html" },
    { track:"GUI", nav:"D3",  group:"召唤坞 (概念稿)", groupEn:"Summon (concept · source = D1 dock)", state:"BUILT", kind:"ref", nm:"竖屏召唤坞",   nmEn:"Summon dock · V",    file:"GUI/pages/Viden - D3 竖屏召唤坞 (GUI).html" },
    { track:"GUI", nav:"PIP", group:"装饰",          groupEn:"Extra",            state:"BUILT", kind:"ref", nm:"宠物 Pip",        nmEn:"Desktop pet · Pip",  file:"GUI/pages/Viden - 宠物 Pip (GUI).html" },
  ],
};
