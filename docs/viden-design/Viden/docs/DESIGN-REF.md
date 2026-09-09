# Viden 设计规范 · AI 速查手册（DESIGN-REF）

> 本文件是**给 AI / 开发快速复用的索引**,不是展示文档(展示见 `Core/Viden - Aurora 主题 (Core).html`)。
> **复用任何组件前先读本文件**:直接抄类名与最小 HTML,不必重读 CSS。
> 黄金规则:**只引用 `var(--*)`,绝不写死颜色 / 字号 / 间距**;改完若定档,按 `CHANGELOG.md` 规矩记录。

## 文件结构
```
项目根/
├── CLAUDE.md                 # 项目说明（根目录，自动加载）
├── index.html                # ★ 原型总览入口：分组导航全部 Core/TUI/GUI 屏 + 单一真源面板
├── tokens.css                # 所有设计变量（唯一真源）← 页面 <link> 它
├── i18n.js                   # ★ 共享：EN/中 切换 + window.t + window.tk（单一真源 · 子目录页用 ../i18n.js）
├── i18n-dict.js              # ★ 共享：集中 i18n 词典 window.VIDEN_DICT（跨页复用文案唯一真源 · 在 i18n.js 之前引）
├── chrome.js                 # ★ 共享：皮肤+密度切换 + window.RC（同上 · ../chrome.js）
├── tweaks-panel.jsx          # ★ 共享：Tweaks 面板（同上 · ../tweaks-panel.jsx）
├── docs/
│   ├── DESIGN-REF.md         # 本文件 · AI 速查
│   ├── NAMING-MAP.md         # ★ 产品名映射唯一真源(Viden ⇄ RoboCode · 门控类型↔UI 标签 · SPEC D-NAME)
│   ├── SPEC.md              # ★ 决策护栏机读真源（@DECISION / @OPEN · 动设计前读）
│   ├── screens-status.js     # ★ 屏幕状态唯一真源（机读）：screens[] 门户卡+进度 · docs[] 两个设计稿索引页的 NAV 真源
│   ├── CHECKLIST.md          # ★ 收尾自查清单（开工前 / 写码时 / done 前 DoD）
│   └── CHANGELOG.md          # 更新日志
├── tools/{check-tokens.js, check-changelog.js, check-status.js}
├── brand-assets/             # 图标 / favicon / OG（见下「品牌资产」）
├── Core/                     # 视觉真源：Aurora 主题、品牌、协作机制
│   └── tui-kit 无关的展示页
├── TUI/                      # 终端版(规范基准)
│   ├── Viden - 统一原型 (TUI).html    # ★ 原型入口(全屏驾驶舱,拖拽/最大化)
│   ├── Viden - 组件库 (TUI).html      # 组件库陈列(.v* 逐件)
│   ├── Viden - 设计稿索引 (TUI).html  # 设计稿索引(侧栏导航 pages/，从原型 ▤ 进)
│   ├── tui-kit.css            # 规范:组件库样式(.v*)
│   ├── pages/                # 各设计稿屏(T0–T5 / 会话页 / 借鉴…)
│   │   └── assets/  tui-screens.jsx · tui-gate-timeline.jsx
│   └── _archive/             # 冻结备份
└── GUI/  + gui-kit.css · gui-icons.jsx · gui-statusbar.jsx · gui-titlebar.jsx · gui-inbox.jsx · gui-settings.jsx   # 桌面(Tauri)版
    ├── Viden - 桌面驾驶舱 (GUI).html      # ★ 旗舰原型(全屏窗口化·打开即用·窗口管理器)
    ├── Viden - 组件库 (GUI).html         # gui-kit `.act/.wslane/.envp/...` 逐件活体陈列
    ├── Viden - 设计稿索引 (GUI).html     # 侧栏导航 pages/ + iframe 保活秒切
    ├── pages/                            # D2–D13 各设计稿屏(D1 旗舰除外)
    └── _archive/                         # 迁移前冻结快照
```
> **共享运行时单一真源（防漂移）**：`i18n.js` / `chrome.js` / `tweaks-panel.jsx` 各**只在根目录留一份**;子目录页面一律 `../` 引用,**绝不再在子目录复制副本**(历史副本已清除)。改这一份 = 全站跟随。新页面照抄此引用方式。

## 接入方式
页面 `<head>` 引 token 真源,根元素声明皮肤与密度:
```html
<link rel="stylesheet" href="tokens.css">      <!-- 子目录用 ../tokens.css -->
<html data-mode="dark" data-skin="aurora" data-density="compact"> <!-- skin: aurora|ice|mono|amber|phosphor · mode: dark|light(amber/phosphor 仅 dark) -->
```

## 页面交互 chrome（每页统一）
> 两个 drop-in 脚本给每页同一套通用交互,放在 app 脚本之前。**单一真源在根目录,子目录页用 `../` 引用**(勿再复制副本):
```html
<script src="../i18n-dict.js"></script> <!-- 可选：集中词典 window.VIDEN_DICT（要用 tk() 则必须在 i18n.js 之前） -->
<script src="../i18n.js"></script>    <!-- 浮动 EN/中 切换 + window.t(en,zh) + window.tk(key) -->
<script src="../chrome.js"></script>  <!-- 浮动「皮肤+密度」切换器 + window.RC -->
<!-- React 页另加：<script type="text/babel" src="../tweaks-panel.jsx"></script> -->
<!-- 根目录页面（如 index.html）直接 bare 引用：src="i18n.js" -->
```
- `chrome.js`:右下角浮动控件,SKIN(5 皮肤圆点,各显其 accent)+ MODE(☾/☀,amber/phosphor 置灰)+ DENS(紧/中/松)。写 `data-skin`/`data-mode`/`data-density` 到 `<html>`,持久化到 `localStorage` 键 **`rc-skin`/`rc-mode`/`rc-density`**(旧 `rc-scheme` 一次性迁移)
  - **皮肤注册表单一真源 = `window.RC.SCHEMES`**(chrome.js 内 `[id, en, zh, modes[]]` 四元组;modes 决定该皮肤支持哪些 mode)。加/删皮肤**只改 chrome.js 这一处**;索引页读 `window.RC.SCHEMES ‖ 回退列表`,勿再各抄。 → **全站任一页切换,其余页自动跟随**。

### i18n 取词（两种，都随浮动 EN/中 切换）
- **切换机制（i18n.js v2 · 热切换）**：`window.vSetLang(v)` 单一入口（悬浮 EN/中 + 设置屏 Language 同走）。页面 `<html data-i18n-live>` → 零刷新（hook React 根重渲 + 重绑 key + `v-lang` 事件）；未标页 reload。**live 页纪律：t() 只在组件 render 内调用，含 t() 的数据表写成函数**（模块级 const 会把语言冻在首载）。已开 live：D1 驾驶舱。
- **品牌字符串**（产品名/字标拆字/配置路径/分支前缀）→ `window.VIDEN_BRAND`（i18n-dict.js 内定义 · 改名 = 改一处 · 映射详情 `docs/NAMING-MAP.md`）。
- **页面独有长句** → `window.t('English','中文')` 内联对（向后兼容，不进词典）。
- **跨页复用的 chrome / 通用术语** → 只在 `i18n-dict.js` 定义一次，取词两法：
  - HTML：`<span data-i18n-key="open"></span>` —— i18n.js 挂载时自动填入（动态插内容后可 `window.vBindI18nKeys()` 重绑）。
  - JS：`window.tk('open')` 返回当前语言字符串（缺词典/缺 key 安全回退到 key）。
- **纪律**：同一个词在多页出现 → 进 `i18n-dict.js`（防「每页各翻一遍」）；页面独有文案不塞词典。`index.html` = 范式（状态枚举/载体/图例/动作词均走 `tk()`）。
- `window.RC`:`setSkin(v)`/`setMode(v)`/`setDensity(v)`/`cycleSkin()`/`toggleMode()`/`supports(skin,mode)`;派发 `rc-state` 事件供页面联动。复用已存在的 `window.RC` 不覆盖。
- 例外:Aurora 展示页 + 统一原型 用自带控制器(与 chrome.js 同两轴 API),不挂 chrome.js;Aurora 共用 rc-* 键,统一原型 用自己的 viden-skin/mode/density 键。

## Token 速查
> 数值唯一真源在 `tokens.css`;下表只做语义索引。改了 tokens.css 必须同步本表。
> 全部颜色 token 随 `[data-skin][data-mode]` 重定义 —— 组件只用语义名,换 skin/mode 即换肤。

### 背景（6 档纵深 · 随 skin×mode）
| token | 语义 |
|---|---|
| `--bg-void` | 最沉底(终端 chrome / 窗口外圈 / 输入区底) |
| `--bg-base` | 主背景(画布 / 终端屏) |
| `--bg-panel` | 面板底(侧栏 / 卡片) |
| `--bg-topbar` | 顶栏底 |
| `--bg-elev` | 抬升面(弹层 / 模态) |
| `--bg-sel` | **选中/高亮行底**(载重元素,配青色左竖条) |

### 前景（4 档 · 靠明度分级）
| token | 语义 |
|---|---|
| `--fg-primary` | 主文(承载内容) |
| `--fg-secondary` | 次文(标签 / 结构) |
| `--fg-muted` | 弱文(元数据)· **正文级,≥4.5:1 vs bg-base/panel/elev** |
| `--fg-faint` | 极弱(提示 / 引导线 / 占位)· **≥3:1,仅限大字·非正文·UI 描边,勿当正文** |

### 强调 & 语义
| token | 语义 |
|---|---|
| `--accent` / `--accent-bright` / `--accent-dim` | 品牌青 = 唯一交互焦点:边框激活 / 标题 / 选中 / focus |
| `--on-accent` | **反相墨色** = 填充强调/语义芯片上的文字/图标(accent/gold/语义四色/builtin 填充面通用)·随 mode 翻黑/白·**填充芯片的文字一律用它,勿再借 `--bg-void`** |
| `--gold` / `--gold-bright` | 次强调金 = viden 字标 / 工作模式 / **「需要人」**(门控·待审批) |
| `--success` / `--warning` / `--error` / `--progress` | 成功绿 / 警告金 / 失败红 / 进行中蓝(刻意与品牌青拉开) |
| `--builtin` | 内置工具紫(GUI 第四类强调) |
| `--border` / `--border-soft` / `--border-active` | 主边框 / 弱分隔 / 激活边框 |
| `--page-bg` `--page-card` `--page-line` `--page-ink` `--page-ink-dim` `--page-accent` | 展示页 chrome(非应用界面用) |

### 字体 / 字阶 / 间距 / 圆角 / 阴影 / 密度
| 类别 | token | 说明 |
|---|---|---|
| 字体 | `--font-sans`(UI/正文 Inter+Noto SC) / `--font-mono`(JetBrains Mono) | 旧别名 `--sans` `--mono` 仍可用 |
| 字阶 | `--fs-kicker` `--fs-xs` `--fs-sm` `--fs-base` `--fs-lead` `--fs-h3/h2/h1` | 行高 `--lh-tight/ui/body`;字距 `--ls-kicker/label` |
| 间距 | `--sp-1`…`--sp-16` | 4px 基准 |
| 圆角 | `--r-xs`(3 TUI) `--r-sm` `--r-md` `--r-lg` `--r-xl`(14 GUI) `--r-full` | TUI 方硬 → GUI 圆润 |
| 骨架列宽 | `--rail-act`(52 活动 rail) `--rail-left`(218 左 lanes) `--rail-right`(300 右 context rail) | **GUI 主窗口骨架单一真源**(D1 驾驶舱同源);各 GUI 横屏页对应列一律引用,勿写死 px |
| 阴影 | `--shadow-sm/md/lg` / `--shadow-win`(GUI 桌面窗口浮空) / `--shadow-pop-gui`(GUI 下拉/菜单/弹层) / `--shadow-toast`(GUI toast) / `--shadow-pop`(TUI 硬投影) | |
| 密度 | 骨架级 `--c-gap` `--panel-pad-x/y` `--rail-mb` `--topbar-mb` · 行/区块级 `--row-pad-y` `--row-pad-y-sm` `--card-pad-y` `--list-gap` `--sec-pad-y` `--input-pad-y` `--msg-mb` | 随 `data-density`=compact/regular/comfy;行/区块级由 gui-kit.css + D1 消费(regular = GUI 现值基线) |
| 终端框 | `--term-screen` `--term-chrome` `--term-bar` `--term-edge` / `--win-edge` | 仿真实终端/桌面窗口外框 · **theme-independent 固定深色**,刻意区别于 Aurora 应用面 |
| 动效 | `--motion-fast/base` `--ease` | |

## 组件目录
> 每个可复用组件一条:类名 + 一句用途 + 最小 HTML。**没登记的视为临时草稿。**
> 颜色文字助手(TUI/展示通用):`.k-accent .k-bright .k-gold .k-goldb .k-success .k-warn .k-error .k-prog .k-pri .k-sec .k-mut .k-faint` + `.b`(加粗)。

### 展示页 chrome

**`.topnav` / `.brand` / `.navlinks` / `.toggle`** — 展示页顶栏(品牌 + 锚点 + 主题切换)。
```html
<nav class="topnav">
  <div class="brand"><span class="mono"><span class="c">[</span>◉<span class="c">]</span></span>
    <span class="mono"><span class="r">v</span><span class="c">iden</span></span></div>
  <div class="navlinks"><a href="#tokens">Color tokens</a></div>
  <div class="toggle"><span class="dot"></span><span>Aurora</span></div>
</nav>
```

**`.kicker` / `h1 .ac` / `.lead`** — 区块小标签(mono 大写+前导横线)/ 大标题(青字 highlight)/ 引言。
```html
<div class="kicker">Deliverable A · TUI theme skin</div>
<h1>Aurora — the <span class="ac">visual theme</span></h1>
<p class="lead">Skin only, no layout change.</p>
```

**`.rcard`(推理卡) · `.gcard`(状态画廊) · `.callout`(说明块) · `.toktable`(token 表) · `.ladder`/`.lrow`(层级阶梯)** — 展示页内容块,均 `var(--page-*)` 系。最小 HTML:
```html
<!-- 推理卡（放 .reasoning 网格内）-->
<div class="rcard"><h3>My call</h3><p>把青色收敛为「品牌 + 交互焦点」一种角色…</p></div>

<!-- 说明块（强调一句结论）-->
<div class="callout"><b>ANSI 256 degradation rule:</b><span>色相优先于明度还原…</span></div>

<!-- token 表（5 列:Token / Dark hex / ANSI 256 / Light hex / 用途，tbody 由脚本填充）-->
<table class="toktable"><thead><tr><th style="width:30%">Token</th><th>Dark hex</th><th>ANSI 256</th><th>Light hex</th><th>Use</th></tr></thead><tbody id="tb-bg"></tbody></table>

<!-- 视觉层级阶梯（.lvl 编号着语义色 + 右侧说明）-->
<div class="ladder"><div class="lrow"><div class="lvl" style="color:var(--warning)">1</div><div><b>需要人</b> · 门控/待审批…</div></div></div>

<!-- 状态画廊卡（放 .gallery 网格内，.glabel 标题 + .demo 放活体示例）-->
<div class="gallery"><div class="gcard"><div class="glabel">Selector · three row states</div><div class="demo"><!-- 活体组件 --></div></div></div>
```

### TUI canonical kit（`tui-kit.css` · 单一类名词汇 · 防漂移)
> **新 TUI 屏一律用 `tui-kit.css` 的 `.v*` 类,别再各页自造终端框/状态栏/红绿灯/后端 chip/lane 行/闸。**
> **TUI 对齐基准 = `Viden - 统一原型 (TUI).html`**(单一组件库的活体真源 · 已接 tui-kit;新屏与存量迁移一律对齐它的结构与交互)。规范源 = `T4 交互规则`。
> 迁移现状(2026-06-29):**T0–T5 + T1b·c·d 全部产品屏已接 tui-kit `.v*`**(终端框/状态栏/后端 chip/命令面板/闸均改用 kit,各页内联 fork 已删);连同 **统一原型** + **会话页 opencode** 共 11 屏对齐。**仅 `Charm 与 HUD 借鉴` / `opencode 与 hermes 借鉴` 两个探索页有意保留内联**(借鉴稿,非产品屏)。原产品屏迁移前快照在 `TUI/_archive/`。
>
> **TUI 入口(单一·像软件)** = `Viden - TUI 原型 (TUI).html` —— app 外壳:左侧分组导航 + 右侧 iframe,从一个壳切换全部屏,记忆上次位置,皮肤切换同步到内嵌屏。新增 TUI 屏只需在该壳的 `NAV` 数组加一项。
> **组件库陈列** = `Viden - 组件库 (TUI).html` —— 把 `.v*` 逐件拆开(活体 demo + 可抄最小 HTML),与 `统一原型`(集成)、本文件(文字目录)三者同源 `tui-kit.css`。
>
> **迁移速查(旧自造类 → tui-kit `.v*`)**:
> - 终端框:`.term`/`.tui` → `.vterm`;`.term-chrome` → `.vterm-chrome`;`.tlt`/`.tlights` → `.vlights`(`i.a/.b/.c`);`.tnm`/`.tlabel` → `.vtitle`;屏体 → `.vterm-body`。
> - 底部状态栏:`.statline`/`.c-statusbar` → `.vstatus`(`.vseg.proj/.lane/.r[.hl]` + 中段 `.vticker`>`.vtk-track`>`.vtk-grp`>`.vtk-item`(`.lbl`/`.v`/`b`/`.g/.ok/.bad/.pr`)+`.vtk-sep` + `.vgate-badge` + `.vled` + `.vgear`/`.vhelp`)。
> - lane 行:→ `.vlane.s-{gate|run|wait|done|block}`(`.on` 选中)+ `.vled-st` + `.vlane-tx`。
> - 后端 chip:→ `.vbe.acp/.builtin/.tmux`(`.k` 类型 + `.ag` agent)或边框型 `.vbe-pill.*`。
> - 迁移法:页头加 `<link href="tui-kit.css">` → HTML 改类名 → 删被取代的内联 CSS → 保留页面专属(doc chrome / zone 标注等)→ 截图比对统一原型。
> 接入:`<link href="../tokens.css"><link href="tui-kit.css">`。
- **终端框**:`.vterm` + `.vterm-chrome`(`.vlights`>`i.a/.b/.c`=红/黄/绿 + `.vtitle`) + `.vterm-body`。
- **状态栏**:`.vstatus` > 左固定 `.vseg`(`.proj` 青底 / `.lane` / 后端段)+ **`.vticker`(中段自动滚动跑大量指标 `.vtk-item`>`.lbl`+`.v`,`.vtk-sep` 分隔,悬停暂停;内容双份无缝循环)** + 右固定 `.vseg.r` · `.vgate-badge`(金 ⏸N 点跳决策) · `.vled`(连接) · `.vhelp`。**钉/滚契约见 SPEC `D-STATUSBAR`**:身份(左)+ 可操作(右 gate/error)恒钉、绝不进 ticker;中段 = 空 `.vsp` 或 ambient-only `.vticker`。
- **后端 chip(铁律 ACP=`--accent` · built-in=`--builtin` · tmux=`--gold`)**:`.vbe.acp/.builtin/.tmux`,格式 `<类型>:<agent>`(如 `ACP:codex`);边框变体 `.vbe-pill.*`。
```html
<span class="vbe acp"><span class="k">ACP</span>:<span class="ag">codex</span></span>
```
- **lane 行**:`.vlane.s-{gate|run|wait|done|block}`(状态色 gate金/run蓝/wait灰/done绿/block红)+ `.on`(选中)·`.vled-st`·`.vlane-id/-role/-meta`·`.vlane-gate`(金 ⏸ 闸标)。
- **4 档审批闸**:`.vgate` > `.vgate-head`(`.badge`+`.risk`) · `.vgate-cmd` · `.vgate-opts`>`.vgate-opt`(`.n` 1–4,`.deny`,`.on`) · `.vgate-foot`(`.cd` 倒计时自动拒)。键:1–4 直达 / ↑↓ / ⏎。
- **快捷键提示**:`.vhints`>`.h b`(键金色)。**overlay**(命令面板/决策/lane 切换):`.vscrim`>`.voverlay`>`.voverlay-in`+`.voverlay-list`>`.vorow.on`+`.vogrp`+`.voverlay-foot`。
- 文字色助手:`.vk-accent/-bright/-gold/-success/-warn/-error/-prog/-builtin/-pri/-sec/-mut/-faint` + `.vb`。

### TUI 字形词表（单一真源 · 防漂移）
> TUI 图标 = 等宽栅格内联 Unicode 几何字形,**不是 SVG**(终端真源:所见即所打 + 带 ANSI-256 降级)——不抽代码模块。
> **状态字形真源 = T4 §08「颜色即状态」**(色=语义、全局固定);donor emoji / ASCII 一律 1:1 换成下表几何字形,颜色承载含义(256 色终端也读得对)。
> **TUI 禁 emoji**(各终端/字体渲染不一、撑破等宽栅格)。新字形先登记此表再用;没登记 = 临时草稿。🤖 `check-tui-glyphs.js` 守。

**状态字形（色 = 状态 · 锚 T4 §08）**
| 字形 | 颜色 | 语义 |
|---|---|---|
| `◆` | 金 gold | clarify / 待你 your turn |
| `▶` | 蓝 blue | run / 流式 streaming |
| `✓` | 绿 green | read·edit / 完成 done |
| `▣` | 青 cyan | skill / 目录 catalog |
| `◌` | 灰 grey | preparing / 待命 wait |
| `⏸` | 金 gold | gate / 待你决策(`⏸ N` 状态栏徽标 = 待审批数) |
| `✗` | 红 red | fail / deny / 错误 error |
| `⚠` | 金 warning | 危险命令审批 / 不可逆动作 |

**结构 / 装饰字形**
- 品牌字标:`[◉]`(括号青 + `◉` 亮青瞳)。
- 框线:box-drawing `╭─╮ │ ╰─╯ ├ ┆`;LIVE WORK `╭─ •`;树形管线 `│ ├`。
- 指针 / 展开:`▸`(规则 / 跳转)·`▾`(折叠)·`›`(prompt / 选择器标记 / 光标)。
- 状态点:`●` 实(活跃 / 占位)·`○` 空(空闲)·`◌` 待命;lane 行状态点走 `.vled-st`(CSS 色,gate金/run蓝/wait灰/done绿/block红)。
- 旋转器:braille `⠋⠙⠹…`(spinner 帧)。`⌕` 搜索 · `↩` 拒绝兜底 · `⊟/⊞` 折叠 · `◧` 色深降级 · `§ ¶ ✉` 文档 / 契约 / 备忘。
- **禁**:emoji(✅ ❌ ⚡ ⭐ 😀 📖 …)与 emoji-presentation 字符(⚡ U+26A1 等会被终端渲成彩色 → 用几何字形替;`✓ ✗ ⏸ ◆` 这类 BMP text-presentation 几何符 OK)。

### TUI 旧类名（已废弃 · 全部迁到 tui-kit `.v*`)
> 11 个 TUI 展示页的终端框/红绿灯/标题已全部迁到上面的 **TUI canonical kit**(`.vterm`/`.vterm-chrome`/`.vlights`/`.vtitle`)。
> 以下旧类名 **已不再使用,勿新写**:`.term` `.emu` `.term-chrome` `.tbar` / `.tlt` `.lt` `.tnm` `.emunm` `.nm` `.ti` `.dot`。
> 仍存留的页面局部类(非框架、不算 fork):`.tbody`(正文)、各页 `.statusbar`(单行状态条,待后续并入 `.vstatus`)、`.emu` 仅作降透明度修饰符(`.vterm-chrome emu{opacity:.55}`)。

**`.c-topbar` + `.chip`** — 顶部状态条(单下边线,不成框)。`.wm` 字标 / `.ver` 版本金 / `.chip .lbl` 标签青。
```html
<div class="c-topbar">
  <span class="wm"><span class="k-accent">[</span><span class="k-bright">◉</span><span class="k-accent">]</span>
    <span class="k-gold">v</span><span class="k-accent">iden</span></span>
  <span class="ver">v0.1.30</span>
  <span class="chip"><span class="lbl">MODEL</span> <b>deep~flash</b></span>
</div>
```

**`.panel` / `.panel.rail` + `.ptitle` + `.pcount`** — 精简面板:线分隔而非整框;`.railwrap` 给右栏加左竖线。
```html
<div class="panel rail"><span class="ptitle">ACTIVE TASKS</span><span class="pcount k-accent">4</span>…</div>
```

**`.sel-list` / `.sel-row` / `.sel-row.on`** — ★选择器高亮行(整个界面视觉锚点):选中行 `--bg-sel` + 青色 `inset 3px` 左竖条。
```html
<div class="sel-list">
  <div class="sel-row on"><span class="mk">›</span><span class="cmd">/model</span><span class="desc">switch model</span></div>
  <div class="sel-row"><span class="mk"> </span><span class="cmd">/lane</span><span class="desc">new lane</span></div>
</div>
```

**`.livework`** — LIVE WORK 进行中条(最亮层级,青边 + 青底 7%)。
```html
<div class="livework"><div class="lw-title">╭─ LIVE WORK •</div>
  <div class="lw-main"><span class="pulse">◉</span> Supervising 1 agent</div>
  <div class="lw-meta"><b>phase</b> testing · <span class="k-prog">64%</span></div></div>
```

**`.composer` + `.modepill` + `.mode-pop`** — 输入区(顶边线)+ 模式 chip + 模式选择弹层(金框 + 硬投影 + reverse-video)。`.cur` 闪烁光标。
```html
<div class="composer"><div class="cinput"><span class="cprompt">›</span>
  <span class="ctext">Add tests…<span class="cur"></span></span>
  <span class="modepill">MODE Build ▾<span class="mode-pop">…</span></span></div></div>
```

**`.c-statusbar` + `.sb-ticker` / `.sb-track` + `.sb-item`** — 合并式滚动状态 ticker(悬停暂停);左侧 `.sb-gear` 配置入口,右侧 `.sb-help`。

**`.approval` + `.ap-head .badge` + `.diffbox`(`.dl-add/.dl-del/.dl-ctx`) + `.ap-acts` / `.tact` / `.tact.focus`** — 审批模态(金/警告框 + box-drawing + 键盘优先动作菜单,reverse-video 焦点)。
```html
<div class="approval"><div class="ap-head"><span class="badge">GATE</span><span class="title">write_file</span></div>
  <div class="ap-body"><div class="diffbox"><span class="dl-add">+ added</span></div>
    <div class="ap-acts"><span class="tact focus"><span class="ky">a</span>Approve</span>
      <span class="tact deny"><span class="ky">d</span>Deny</span></div></div></div>
```

**`.welcome` + `.robot`(`.eye`/`.base`)** — 启动欢迎屏(ASCII 机器人 + 青色辉光)。
**Lane 监视模式 `.lm-banner` / `.lm-grid` / `.lm-item.on` / `.tbar`** — 全屏 lane 监控(同样 `.on` 高亮规则)。

### GUI（桌面 / Tauri）

> **GUI canonical kit（`gui-kit.css` · 对标 tui-kit.css · 防漂移)**
> **新 GUI 桌面屏一律用 `gui-kit.css` 的类**(窗口壳/标题栏/活动 rail/lane 行/Environment/状态栏/输入区/边缘浮出),别再各页内联自造。视觉真源 = D1 驾驶舱;数值真源仍是 tokens.css(本套件只引用 `var(--*)`)。
> 接入:`<link href="../tokens.css"><link href="gui-kit.css">`。
> 存量页迁移状态(2026-06-29 · **2026-09-09 更正 D1**):**D2决策 / D4 / D5 / D6 / D7 / D8 / D9 / D10 / D11 / D12 / D13 / D14 + 组件库 已接 gui-kit**(窗口壳/标题栏/活动 rail `.act`·`.actbtn`·`.badge`/winbar dots 用套件,各页内联 `.vbar`/`.vrail`/`.vbtn` fork 已删,裸色收 token);**D1 旗舰有意不接** —— 它是视觉真源、kit 是它的镜像,反向 link 会让 kit 里已分叉的同名规则属性泄漏(2026-09-09 实测 chat 视图 2584 px 变化;原文列 D1 为「已接」是笔误,实物从未 link);D2存档 UI 语义色已收(图表数据序列色基线保留);**召唤坞 D2横/D3竖 与 宠物 Pip 为独立浮层/装饰概念,有意保持自包含**(token-clean·不接套件,避免全局 `.composer`/`.side` 泄漏)。新页直接用套件。下列条目即套件登记内容。

**图标目录（`gui-icons.jsx` · GUI 图标单一真源 · 防重绘）** — 所有 GUI 线性图标 + agent 品牌徽标一份收口,视觉母版 = D1 驾驶舱。**别再各页内联 `VRAIL_ICONS`/`I*` 自画**(同一 worktree/lanes/review 此前被各画各的,已统一)。接入:`<script type="text/babel" src="../gui-icons.jsx"></script>`(gui-titlebar / 主脚本**之前**;根目录页用 `gui-icons.jsx`)。导出 `window.ICONS`(元素表)/ `GuiIcon`(换 class/尺寸)/ `AgentLogo`。
```jsx
{ICONS.chat}                                  /* rail / 标题栏按钮内直接取元素 */
<GuiIcon name="lock" className="ic sm"/>        /* 小号 15px */
<GuiIcon name="lock" style={{width:13,height:13}}/>  /* 任意尺寸(URL 栏/徽标位) */
<AgentLogo agent="codex"/>                      /* codex/claude/kiro/viden·品牌固定色不换肤 */
```
> **画法规范**:线性 · viewBox 24 · class `ic`(gui-kit `svg.ic` 给 stroke currentColor / 19px / 圆端)·`ic sm`=15px。颜色一律 `currentColor`;唯 `AgentLogo` 用品牌色(codex 绿/claude 橙/kiro 紫/viden 青+金)。
> **key 目录**(语义):rail/视图 = `chat worktree lanes(swimlanes) review decide fleet evidence diagnostics inbox brief gallery remote rocket` · 标题栏工具 = `palette focus popout term panel slot pin settings` · 替 emoji = `lock(🔒) robot(🤖) game(🎮) ml(📈)`。
> **铁律**:GUI 禁 emoji——要图标走这里的线性 SVG。新图标先在此加 key + 登记语义再页面引用;没登记 = 临时草稿。护栏见 SPEC `D-ICON`。

**窗口标题栏 chrome（D1 同源 · 全 GUI 横屏页统一规范）** — 三套等价实现,**度量必须一致**:`.titlebar`(D1/召唤坞,含 `.tbtitle`)· `.vbar`(流程/内容页,共享壳)· `.winbar`(D10 监视墙,用 `.dots` 代 `.tl`)。规范:**栏高 46px · gap 13px · padding 0 14px · 交通灯点 12px(gap 8,红 `--error`/黄 `--warning`/绿 `--success`)· 字标 mono 700 13.5px**。字标 = `[◉]`(括号 `--accent` + `◉` `--accent-bright`)+ `v`(`.r`=`--gold`)+ `iden`(`.c`=`--accent`)。
```html
<div class="titlebar"><div class="tl"><i class="a"></i><i class="b"></i><i class="c"></i></div>
  <span class="wm"><span style="color:var(--accent)">[</span><span style="color:var(--accent-bright)">◉</span><span style="color:var(--accent)">]</span> <span class="r">v</span><span class="c">iden</span></span></div>
```

**`<GuiTitleBar>`(`gui-titlebar.jsx` · GUI 标题栏 canonical · 单一真源)** — 把上面 chrome(灯+字标+`projsel`+`gitops` sync/worktrees+`tbtools` 工具组)抽成组件,视觉母版 = D1。**横屏 D 页一律用它,别再各页内联自造 `.vbar`。** 样式仍走 `gui-kit.css`(组件不注样式)。接入:`<script type="text/babel" src="../gui-titlebar.jsx"></script>`(主脚本前)。
```html
<GuiTitleBar/>                                  <!-- viden/main · 默认工具组 -->
<GuiTitleBar project="Robocode 工作区" branch="18 项目" git={false}/>
<GuiTitleBar project="未接入项目" branch="" git={false} dim tools={['index','settings']}/>
```
> props 均可省:`project`/`branch`(projsel 文字)· `git`(显示 gitops·默认 true)· `sync`{up,down}/`worktrees`(数·null 隐)· `tools`(按钮 key 数组:palette focus popout term panel index settings `|`分隔)· `active`/`onTool` · `indexHref`(▤ 设计稿索引)· `dim`(projsel 半透,如无项目)。已接:D2决策/D4/D5/D7/D9/D11/D12/D13/D14。D1 旗舰保留其交互版 `.titlebar`(窗口管理器拖拽/最大化依赖之),作视觉真源不替换。

**`<GuiPopoutBar>`(gui-titlebar.jsx 内 · popout 独立窗口顶栏 canonical · SPEC D-POPOUT)** — 监视墙/竖条等 popout 窗口的轻量顶栏：`.winbar`(度量同源 gui-kit) + dots + 字标 + `.tt` 题字 + 右侧插槽(`.winbar .pin`/`.pop` 已进 gui-kit)。**各页禁再手抄 dots/字标。**已接：D10 监视墙 + 竖条。
```html
<GuiPopoutBar title={<b>Lane Monitor</b>}>
  <span className="pin on">… pin top</span><span className="pop">↗ pop out</span>
</GuiPopoutBar>
```

**`.frame`(≡ `.win` / 配 `.winbar`)** — 桌面窗口外壳。**统一规范(D1 同源,全 GUI 横屏页一致)**:边 `1px solid var(--win-edge)` + 圆角 `var(--r-xl)` + 浮空投影 `var(--shadow-win)`;`height`/`min-width`/布局(flex|grid)按各屏自定。**禁** `--border` 边 / 裸 px 半径 / 裸 `rgba()` 投影。

> **主窗口骨架列宽(D1 同源,单一真源)**:活动 rail `var(--rail-act)`=52 · 左 lanes `var(--rail-left)`=218 · 中央 `1fr` · 右 context rail `var(--rail-right)`=300。各页 `.inner`/`.vbody`/`.frame` 的对应列**一律引用 token,勿写死 px**。例外:D9 远程开发右栏 420px(内含 4 列 GPU 监视器,有意加宽);D4/D11 向导步骤栏 236px(非 lanes 栏,独立组件)。

**`<GuiStatusBar>`(`gui-statusbar.jsx` · GUI 窗口级底栏 canonical · 单一真源)** — 所有 GUI 桌面窗口共用的 28px 等宽底栏(左 ⚙ + 中段滚动 ticker + 右状态段),视觉母版 = D1 `.statusbar`。**新 GUI 桌面屏一律用它,别再各页自造底栏。** 放在 `.frame`/`.win` 的**最后一个子节点**(frame 固定高 + overflow:hidden,自动贴底)。接入:`<script type="text/babel" src="gui-statusbar.jsx"></script>`(在主脚本之前,同 tweaks-panel 机制),class 前缀 `gui-sb`(与 D1 `.statusbar` 不冲突)。
```html
<div class="frame"> … <GuiStatusBar right="⏸ 1 gate waiting" /> </div>
```
> props 均可省:`items`(中段指标 `[label,value]` 数组,默认 Aurora 占位 provider/model/ctx/cost/mode/perm/branch)· `right`(右侧固定状态段,默认隐藏)。**钉/滚契约见 SPEC `D-STATUSBAR`**:`right`(`.sb-right`)钉可操作项(gate/error,绝不进 ticker),`.sb-tickwrap` 仅跑 ambient。D1 驾驶舱保留其更丰富的交互版 `.statusbar`(含配置弹层),不替换。已铺:D2决策/D2存档/D4/D5/D7/D9/D11/D12/D13。未铺(有意):D2横/D3 召唤坞 · D10 双独立窗口 showcase(监视墙+竖条,无单一 cockpit 窗) · Pip 装饰页。
```html
<div class="frame">…</div>  <!-- 或 .win，二者度量必须一致 -->
```

**`<InboxView>`(`gui-inbox.jsx` · 团队收件箱/简报/通告 canonical · 单一真源)** — 「团队 · 人+agent 通信频道」：闸收件箱(按 viden.toml ownership 路由) + 团队 roster + 移交/自动驾驶 + 团队简报 + 变更通告(通道 A 送到人 / 通道 B 送到 agent memo + ack 追踪)。视觉母版 = D7;**旗舰 D1 驾驶舱(活动 rail `IInbox` 按钮)与 D7 设计稿共用同一份**,改一处全改。样式自注入(`.gi-root` 作用域隔离,不污染宿主页)。接入:`<script type="text/babel" src="gui-inbox.jsx"></script>`(主脚本前) → `<InboxView/>`。
```html
<InboxView defaultTab="notice"/>   <!-- 自带 收件箱/简报/通告 三标签 + 团队 rail -->
```
> props 均可省:`defaultTab` `'inbox'|'briefing'|'notice'`(默认 inbox)· `onToast(msg)`(认领回调,内部也有浮层)。D1 用 `<InboxView/>`;D7 用 `<InboxView key={tw.defaultTab} defaultTab={tw.defaultTab}/>`(Tweak 切默认标签经 key 重挂)。

**`<SettingsView>`(`gui-settings.jsx` · 设置/偏好 canonical · 单一真源 · SPEC D-SETTINGS)** — D1 驾驶舱第 8 视图(活动 rail 底部 ⚙ · `view==='settings'` · 不新开窗口)。7 节:Provider&Models(引擎 registry) / Permissions(permission_mode 5 档·规则预览) / Appearance(真接 window.RC) / Keyboard / Notifications(#13 离桌兜底·escalation/quiet hours) / Privacy&Telemetry / Workspace(配置链/分支前缀)。样式自注入 `.gset-` 作用域隔离。引擎对齐见 `docs/NAMING-MAP.md`。
```html
<script type="text/babel" src="gui-settings.jsx"></script>   <!-- 主脚本前 -->
<SettingsView/>            <!-- 可选 defaultSection='provider|permission|appearance|keyboard|notify|privacy|workspace' -->
```

**`.cmdbar` + `.lg`(logo) + `.path` + `.cbtn` + `.dentry`(决策入口带 `.bdg` 红角标)** — 顶部命令栏。

**`.lanebar` + `.lrow` / `.lrow.on` + `.led` + `.gate` + `.tgtchip`** — lane 侧栏(选中 `--bg-sel` + 青左竖条;`.gate` 金角标待审;`.tgtchip` 远程目标)。
```html
<div class="lrow on"><span class="led"></span><div class="tx"><div class="t1"><span class="lid">L1</span> codex</div>
  <div class="t2">s1/pty/01</div></div></div>
```

**`.sesstabs` + `.stab` / `.stab.on`** — 会话标签页(底部青色 active 线)。
**`.targetbar`(`.ssh` 变体)** — 远程目标状态条(蓝色系)。
**`.work` 内转录:`.umsg`(用户) · `.amsg`/`.ahdr`/`.atext`(助手) · `.tool`(`.add/.del/.ok/.wn`) · `.gatemsg`+`.gobtn`(门控提示)** — 桌面版转录消息族。
**`.composer` + `.cline` + `.pm`(模式 pill)** — GUI 输入行(圆角框 + 真 input)。

**`.gperm`(权限提示 · gui-kit · 对标 TUI `.perm` · 模型见 SPEC `D-PERM`)** — **执行前**拦截、**阻塞此会话**的权限提示(金=「需要人」);**permission ≠ gate**:permission 拦「能不能执行」(就地·阻塞),gate 拦「批不批合入」(决策中心·异步·diff 评审归它)。卡片与放置解耦,可内联会话底或包进 `.scrim` 居中弹。`.gperm-hd`(`⏸ ic`+标题+`.risk.hi/.md/.lo`=error/gold/success)· `.gperm-what`(`.cmd` mono 动作,`.tgt` 红远程前缀 · `.why` 理由)· `.gperm-opts`(**横向紧凑 chip 行** · GUI 停靠带高度有限;TUI `.vgate` 那边才纵向键盘列表)>`.gperm-opt`(`.k` 键码 chip · `.on` 安全项默认高亮(青=焦点)· `.deny` 红键;命中≥30px · 范围分级 Y·A·⇧A·E·N · `.sub` scope 注默认隐·横向省高)· `.gperm-foot`(permission/gate 区分注)。`.dock`(满幅停靠 composer 上沿·无浮投影·只留顶边 = D1 旗舰内联用法);卡片本身扁平无 shadow(读作会话内元素·非弹窗)。`.pulse` = 金边脉冲(reduced-motion 关·**仅后台 lane 用·前台已聚焦的提示不脉冲**)。Deny → 转向(D1 把 composer 占位切「Tell agent what to do instead」)。四类活体样本 + 页内 tweak 见「组件库 (GUI)」。
```html
<div class="gperm dock"><div class="gperm-hd"><span class="ic">⏸</span>Permission · shell command<span class="risk hi">HIGH</span></div>
  <div class="gperm-what"><div class="cmd">rm -rf target/ &amp;&amp; cargo build --release</div><div class="why"><b>Reason:</b> recursive delete · out of allowlist</div></div>
  <div class="gperm-opts"><div class="gperm-opt on"><span class="k">Y</span> Once</div><div class="gperm-opt"><span class="k">A</span> Session</div><div class="gperm-opt deny"><span class="k">N</span> Deny</div></div>
  <div class="gperm-foot"><b>Permission</b> guards execution; the change is reviewed later at a <span class="c">gate</span>.</div></div>
```

#### Cockpit 交互件（D1 驾驶舱同源 · 横屏 cockpit 与竖屏 4K 共用）

**`.act` + `.actbtn`(`.on`) + `.actspacer` + `.badge`** — 52px 活动 rail(视图切换 + 底部 pin/Settings)。`.actbtn` 38×38 圆角;`.on`=`--bg-sel`+`--accent-dim` 边+`--accent-bright`;`.badge` 角标(`--accent`/`--progress`/`--gold`/`--error`)。**底部 pin 键**=`.actbtn`,`leftMode==='pinned'?'on':''`,点击在 lanes 边栏 float↔pinned 间切换。
```html
<div class="act">
  <div class="actbtn on" title="Conversation"><svg class="ic">…</svg></div>
  <div class="actbtn" title="Worktrees"><svg class="ic">…</svg><span class="badge" style="background:var(--accent)">4</span></div>
  <div class="actspacer"></div>
  <div class="actbtn on" title="Lanes 边栏 · 常驻↔自动隐藏"><svg class="ic sm">…</svg></div>
  <div class="actbtn" title="Settings"><svg class="ic">…</svg></div>
</div>
```

**`.wslane.s-{work|done|need|stop}`(`.on`) + `.aico` + `.lbody`>`.r1`(`.lid`/`.nm`)+`.r2`(`.viab`/`.cli`/`.br`)** — lane 行(载重组件)。状态边框:`s-work`蓝/`s-done`绿/`s-need`金脉冲(需要人)/`s-stop`红;`.on`=`--bg-sel`+青 `inset 3px` 左竖条;`.aico`放 `AgentLogo`。容器:`.side`(栏)→`.wsghd`(分组头)/`.wslanes`(组)/`.wsseclabel`(分段标题)/`.lseg`+`.sg`(Lanes/Workspace 段切换)。
> **`.viab` 后端 chip 铁律(D-BACKEND · 跨轨固定)**:色 = `VIA_COLOR[via]` —— `ACP`=`--accent` 青(桥接外部 CLI) · `built-in`=`--builtin` 紫(直调模型) · `tmux`=`--gold` 金(附着已有 tmux pane)。**勿把 built-in 映射成金**(金归 tmux)。
```html
<div class="wslane on s-need" title="…">
  <span class="aico"><!-- AgentLogo --></span>
  <div class="lbody">
    <div class="r1"><span class="lid">L1</span><span class="nm">config loader refactor</span></div>
    <div class="r2"><span class="viab"><span class="gs">●</span>ACP</span><span class="cli">codex · gpt-5</span><span class="br">⎇ config-loader</span></div>
  </div>
</div>
```
> **`.viab .gs` gate_strength 门控硬度徽标(D-GATESTR · fleet 常显)**:route 芯片内前缀一个填充级字形 —— `●`full(native/built-in 逐调用拦截)·`◐`cooperative(ACP 建议性·worktree 兜底)·`○`containment(terminal/tmux 只能围栏·退出 diff)。实心→空心 = 门越来越软；色随 route，义随填充。D10 卡片/D13 fleet 用同一套字形 + `FULL/COOP/CTN` 缩写。gate_strength 是 lane 契约一等字段(随 route 派生但独立)。

**`AgentLogo`(agent: `codex`/`claude`/`kiro`/`viden`)** — agent 品牌徽标 SVG(17×17,填品牌色,**非主题 token**:codex `#10a37f` / claude 射线 `#d97757` / kiro `#8b5cf6` / viden 括号眼 `--accent`+`--gold`)。未命中的 agent 回落 viden 母版。

**`.envp` > `.envsec` > `.envhd`(`.t`+`.caret`/`.gear`) + 行族** — Environment 复合面板(右 context rail 或竖屏浮层)。可折叠分段:Environment / Context / Subagents / Sources / MCP / LSP / Todo。行:`.envrow`(`.ei`/`.el`/`.cv`/`.stat .ad/.rm`)· `.envctx`(`.big` token + `.bar` + `.sub2` cache/cost)· `.todorow`(`.ck`,`.done`/`.active`)· `.mcprow` · `.lspnote` · `.envfoot`。
> **`.envctx` 预算盲区(D-BUDGET-BLIND)**:外部 CLI(terminal/tmux · containment)自计 token、Viden 无法计量 → `.big` 显 `—` + `.blindtag`(warning 边框「计量盲区」)、`.blindwhy` 一句因、`.blindprox`(`.pk` 代理指标:runtime/runs/diff/exit)。禁用估值冒充精确费用。

**`.mgate` > `.mgate-hd`(`.st .collect/.accepted/.needs` + `.txt` + `.prog`) + `.mgate-list` > `.mgate-ev.ok/.wait/.na`(`.ck`/`.kind`/`.det`)** — MergeGate 证据清单(D2/D12 · required vs collected · SPEC D-MERGEGATE)。五类一等 required evidence:`patch`·`test_result`·`review`·`doc_update`·`release_artifact`。status 由已记 evidence 的 kind 归约(非前端本地勾选):缺必备=collecting_evidence、全满足→accepted、被拒→needs_changes。`✓`已收(success)·`◌`待收(gold)·`–`此闸不需要(faint)。契约见 frontend-integration-contract。
```html
<div class="mgate"><div class="mgate-hd"><span class="st collect">collecting evidence</span><span class="txt">gate accepts only when every required kind is recorded</span><span class="prog">1 / 3</span></div>
  <div class="mgate-list"><div class="mgate-ev ok"><span class="ck">✓</span><span class="kind">patch</span><span class="det">+12−3</span></div>
    <div class="mgate-ev wait"><span class="ck">◌</span><span class="kind">review</span><span class="det">awaiting your approve</span></div></div></div>
```
```html
<div class="envsec"><div class="envhd"><span class="t"><span class="caret">▾</span>Context</span></div>
  <div class="envctx"><div class="big">42.1k <span>/128k</span></div><div class="bar"><i style="width:33%"></i></div>
    <div class="sub2"><span>cache 18.4k</span><span>·</span><span>cost $0.78</span></div></div></div>
```

**`.edgewrap.l/.r` + `.edgehint` + `.floatpanel`** — 边栏自动隐藏 + 边缘悬停浮出(`leftMode==='float'`)。热区贴活动 rail 右缘(`left:52px`);`.edgehint` 青提示条;`.floatpanel` 锚定 rail 右侧,入场 `translateX(-14px)→0`+淡入(**勿用 -115% 大位移横扫**)。`cols` 按实际子元素数生成轨道:pinned 4 轨 / float 3 轨 / focus 2 轨(否则 centerwrap 落进 0 宽轨)。

#### D1 次级视图（活动 rail 的路由目的地 · SPEC `D-RAILNAV`）

> **活动 rail = 路由器**(D-RAILNAV·2026-09-08 定档)：rail 按钮导航到独立 D 屏(D12/D2/D14/D10/D13);
> 旗舰 D1 的页内视图切换保留作**设计探索**。下面四族是该次裁决**判定登记**的次级面 —— 登记后即「可复用」(D-COMP)。
> **样式现状(如实登记)**:**DiffReview / EvidenceView 两族的 CSS 已按 D-SOT 镜像进 `gui-kit.css`**
> (2026-09-09 · kit 尾部「D1 次级视图」块);第二个消费页 `<link href="gui-kit.css">` 直接取用,别再从 D1
> 手抄一份 —— 手抄即漂移。**DiagnosticsView / DockSD 两族仍只在 D1 内联**(见下「未升进的两族」)。
> ⚠ **2026-09-09 裁决与实测(照此复用,别重跑一遍踩同样的坑)**:
> ① **D1 不能反向 link `gui-kit.css`** —— 两边同名 chrome 规则已有意分叉,D1 只声明部分属性、kit 那条
>   多出的属性会泄漏进来(`.envrow .cv` 多了 `font-family`/`font-size`,直接改掉 Environment 行字形);
>   无头 Chrome 逐视图像素比对:chat **2584 px**、review 175 px、evidence/diagnostics 各 189 px 变化。
>   → 升进只能做成**镜像**(kit 与 D1 内联逐字一致、改一侧必须同步另一侧 · PROTO-STANDARD §5),不是「搬走」。
>   已落地:kit 尾块 = DiffReview 43 条 + EvidenceView 35 条,与 D1 内联**逐条 byte-identical**(脚本核对)。
> ② **裸族名撞名照 PROTO-STANDARD §5 的既定解法办:改页面专属类名、保留登记族名**(先例 D12 `.tl` → `.rline`)。
>   D2 决策中心私有的 `.diffbody`/`.dl`(3 列 grid + `.sg` 符号列 + 删除行删除线,与 DiffReview 的 2 列 flex
>   是**两个不同组件、只是重名**;未改名时实测 kit 泄漏使 D2 **1698 px** 变化)→ **`.d2diff` / `.d2dl`**;
>   D14 私有的 `.evchip`(原先靠逐属性覆盖才与登记名共存,脆)→ **`.d14chip`**。
>   登记族名自此归 kit 所有:新页要这套画法就接 kit,**别再拿裸族名当页面私有类**。
> ③ 镜像 + 改名后逐页像素回归(无头 Chrome 1440×900 · 动画冻结):D1 八视图 · 组件库 · 12 张 kit 消费屏
>   **全部 AE=0**。
> **未升进的两族**:DiagnosticsView(`.dgwrap`)与 DockSD(`.sdock`)是 deferred / roadmap,没有第二消费者,
> 暂不动;它们的详情壳复用 EvidenceView 的 `.evdethead`/`.evsec`/`.evfoot`,将来一起升进。
> **家族边界(已照此切)**:`.fchip`/`.search` 归 EvidenceView(镜像为 `.evbar .fchip`/`.evbar .search`);DiagnosticsView 的
> `.dgbar .fchip` 是另一份、仍在 D1 内联;`.mpill`(`.evfoot` 用)、`.envrow .stat`、`.todorow .ck` 是别族共用件,不随族走。
> **未登记(有意)**:`WorktreeBoard`(`.wtwrap`)与旗舰单 lane 内嵌 subagent 树 —— D-RAILNAV ③④ 判为
> 不作实现目标 / 延后到 fleet 家族;原型 markup 留在旗舰作探索,**按 D-COMP 视为临时草稿,勿在新页复用**。
> **登记 ≠ 已实现**:实现目标版本(0.3.3 / 0.3.4)是排期不是交付承诺,语义同 `D-ROADMAP`。

**`.review` > `.filetree`(`.fthead`/`.ftlist`>`.ftrow.on`) + `.diffpane`(`.dphead`+`.diffbody`>`.dl.add/.del/.hunk`+`.commitbar`)** — **DiffReview**:in-cockpit 逐文件 diff 评审(文件树 + 暂存 + 统一/分栏 diff + 提交栏)。`.ftrow .stat.m/.a/.d`=改/增/删(金/绿/红)·`.on`=`--bg-sel`+青 `inset 2px` 左竖条·`.ck` 已暂存 ✓;`.dl .ln` 行号 + `.tx`(`.ctx` 上下文行);`.cbtn` 提交按钮(`.commit` 绿实心 / `.push` 青描边)。**实现目标 0.3.3** = 操作者 git 动作 / 结构化 diff / 冲突内容的计划宿主(GUI-CORE-020/012/015)。
```html
<div class="review">
  <div class="filetree">
    <div class="fthead"><h3>Changes</h3><span class="ct">4 files · +54 −56</span></div>
    <div class="ftlist">
      <div class="ftrow on"><span class="stat m">M</span><span class="fn">src/config.rs</span><span class="pm">+24 −6</span><span class="ck">✓</span></div>
      <div class="ftrow"><span class="stat a">A</span><span class="fn">src/errors.rs</span><span class="pm">+12</span></div>
    </div>
  </div>
  <div class="diffpane">
    <div class="dphead"><span class="fn">src/config.rs</span>
      <div class="seg"><button class="on">Unified</button><button>Split</button></div></div>
    <div class="diffbody">
      <div class="dl hunk"><span class="ln"></span><span class="tx">@@ -38,9 +38,12 @@ impl ConfigLoader</span></div>
      <div class="dl del"><span class="ln">41</span><span class="tx">-       let raw = fs::read_to_string(path).unwrap();</span></div>
      <div class="dl add"><span class="ln">41</span><span class="tx">+       let raw = fs::read_to_string(path)?;</span></div>
      <div class="dl ctx"><span class="ln">45</span><span class="tx ctx">        toml::from_str(&amp;raw).map_err(Into::into)</span></div>
    </div>
    <div class="commitbar">
      <div class="msg-in">fix(config): use ? + guard empty file <span class="ph">· conventional commit suggested by viden</span></div>
      <button class="cbtn">Stage all</button><button class="cbtn commit">Commit</button><button class="cbtn push">Commit &amp; Push ⇡</button>
    </div>
  </div>
</div>
```

**`.evwrap` > `.evmain`(`.evbar`>`.fchip`+`.search` · `.evscroll`>`.evday`+`.evrow2.on`) + `.evdet`(`.evdethead`+`.evsec`+`.evfoot`)** — **EvidenceView**:in-cockpit 证据视图(按天分组的证据时间线 + 右侧详情 + 关联芯片)。行 `.evrow2`>`.tm2` 时间·`.ic2` 类型字形(色 = 语义)·`.tt2` 标题·`.lane-b` 归属 lane;详情 `.evkv`(`.k`/`.v` 键值)·`.evterm`(`.gp`/`.rp`/`.dim` 输出尾)·`.evchip`(关联 patch/resume/approval)·`.evfoot`>`.mpill.go` 主动作。读侧先行;配 **D-AUDIT**「审计行链接证据、不反向」的单向链。**实现目标 0.3.3**(排在 DiffReview 之后)。
```html
<div class="evwrap">
  <div class="evmain">
    <div class="evbar"><span class="fchip on">All</span><span class="fchip">Tests</span><span class="fchip">Diffs</span>
      <span class="search">⌕ search evidence… ⌘F</span></div>
    <div class="evscroll">
      <div class="evday">Today</div>
      <div class="evrow2 on"><span class="tm2">14:32</span><span class="ic2">✕</span><span class="tt2">test failed · config_tests</span><span class="lane-b">L1</span></div>
    </div>
  </div>
  <div class="evdet">
    <div class="evdethead"><div class="t1">✕ test failed · config_tests</div>
      <div class="t2">L1 codex · ⎇ vd/config-loader · 14:32 · evidence #e-0142</div></div>
    <div class="evsec"><h5>Report</h5>
      <div class="evkv"><span class="k">command</span><span class="v">cargo test -p viden-cli config_tests</span></div>
      <div class="evkv"><span class="k">exit</span><span class="v">101</span></div></div>
    <div class="evsec"><h5>Output tail</h5>
      <div class="evterm"><div class="gp">test config::loads_valid ... ok</div><div class="rp">test config::rejects_empty ... FAILED</div>
        <div class="dim">assertion at config.rs:42 — expected Err(Empty), got Ok</div></div></div>
    <div class="evsec"><h5>Linked</h5><span class="evchip">+24−6 patch · config.rs</span><span class="evchip">⤴ resume 4f2b…7e</span></div>
    <div class="evfoot"><span class="mpill go">Open in review</span><span class="mpill">Re-run test</span></div>
  </div>
</div>
```

**`.dgwrap` > `.dgmain`(`.dgbar`>`.fchip`+`.sum` · `.dgfile` + `.dgrow.on`) + `.dgdet`(`.evdethead`+`.dgcode`+`.evsec`+`.evfoot`)** — **DiagnosticsView**(⏳ **deferred**):in-cockpit 诊断视图(按文件分组的 error/warning 列表 + 右侧代码定位)。行 `.dgrow`>`.sev` 严重度字形(✕ error / ⚠ warning)·`.msg3` 消息·`.loc` 行列·`.src` 来源(test/clippy/LSP);详情 `.dgcode`>`.lnum`+`.squig`(波浪下划线)。详情壳复用 EvidenceView 的 `.evdethead`/`.evsec`/`.evfoot`。**实现目标 0.3.4 候选**,待 lsp→契约接线勘查 —— 登记只保证组件可复用,不承诺可开工(D-ROADMAP 语义)。
```html
<div class="dgwrap">
  <div class="dgmain">
    <div class="dgbar"><span class="fchip on">All</span><span class="fchip">Errors</span><span class="fchip">clippy</span>
      <span class="sum">1 error · 3 warnings</span></div>
    <div class="dgfile"><span class="fn">src/config.rs</span><span>· ⎇ vd/config-loader</span></div>
    <div class="dgrow on"><span class="sev">✕</span><span class="msg3">mismatched types: expected Err(Empty), got Ok</span>
      <span class="loc">42:15</span><span class="src">test</span></div>
    <div class="dgrow"><span class="sev">⚠</span><span class="msg3">unused import: std::io::Read</span>
      <span class="loc">3:5</span><span class="src">clippy</span></div>
  </div>
  <div class="dgdet">
    <div class="evdethead"><div class="t1">✕ mismatched types</div>
      <div class="t2">src/config.rs:42:15 · test · ⎇ vd/config-loader · L1 codex</div></div>
    <div class="dgcode"><div><span class="lnum">42</span><span>assert!(matches!(cfg, Err(Empty)));</span></div>
      <div><span class="lnum">  </span><span class="squig">              ^^^^^^^^^^^^^^^ got Ok(_)</span></div></div>
    <div class="evsec"><h5>Context</h5><div class="evkv"><span class="k">lane</span><span class="v">L1 codex · fixing now (attempt 2/3)</span></div></div>
    <div class="evfoot"><span class="mpill go">Delegate fix → lane</span><span class="mpill">Re-run check</span></div>
  </div>
</div>
```

**`.sdock`(`.collapsed`) > `.sdgrip` + `.sdtabs`(`.sdtab.host/.on` + `.sdaddwrap`>`.sdadd`+`.sdveil`+`.sdsummon` + `.sdright`>`.sdmini`) + `.sdbody`(`.sdempty`>`.sdquick`>`.sdqbtn`)** — **DockSD 召唤坞**(🗺 **roadmap**):D1 底部可拖高的工具坞 —— 标签页(host 终端 + 已召唤工具)、`+` 召唤菜单、坞体。`.sdsummon` 菜单行 `.ar`(`.open`=已开)>`.ti` 图标 + `.tt2`(`.n` 名 / `.s` 描述) + `.ak` 键位,五个工具:terminal / files / review / browser / sidechat。**召唤坞实现真源 = 本组件**(SPEC `D-POPOUT`),D2h/D3 是只探版式的概念稿。**roadmap 0.3.4+/V2** —— 本次登记只为让 SPEC 与 DESIGN-REF 一致(D-RAILNAV ⑥),不改变排期。
```html
<div class="sdock">
  <div class="sdgrip"></div>
  <div class="sdtabs">
    <div class="sdtab host"><span class="hdot"></span>wiki@viden</div>
    <div class="sdtab on"><span class="tic"><!-- ICONS.term --></span>Terminal<span class="x">✕</span></div>
    <div class="sdaddwrap">
      <div class="sdadd open">+</div>
      <div class="sdveil"></div>
      <div class="sdsummon">
        <div class="amh">Summon a tool</div>
        <div class="ar"><span class="ti"><!-- ICONS.review --></span>
          <span class="tt2"><span class="n">Review</span><span class="s">Per-file diff review of this round</span></span>
          <kbd class="ak">⌘R</kbd></div>
      </div>
    </div>
    <div class="sdright"><span class="sdmini">▾</span></div>
  </div>
  <div class="sdbody">
    <div class="sdempty"><div class="eh">Dock is empty · summon a tool to start</div>
      <div class="sdquick"><div class="sdqbtn"><span class="qi"><!-- ICONS.term --></span>Terminal<span class="qk">⌘J</span></div></div></div>
  </div>
</div>
```

**竖屏 cockpit**:`Viden - 竖屏 4K 驾驶舱 (GUI).html` — 上述全部 chrome 的竖向编排(活动 rail + lanes 常驻 · 对话↑ → 监控+终端 dock↓ · Environment 浮层),固定 1080×1920 letterbox scale-to-fit,纯静态 HTML。新竖屏/异形屏从它 fork。

## 内置角色 & task 状态词表（跨轨 · 契约对齐 robocode/frontend-integration-contract）
> 动 lane/agent/fleet 相关 UI 前先对齐；名字以 robocode runtime 契约为准，别自造角色。

- **7 个内置角色(runtime 规范·唯一真源)**:`planner` · `coder` · `reviewer` · `tester` · `doc-writer` · `researcher` · `release-operator`。
  - 曾用名映射:`editor`→**coder**(D13 已改)、`writer`→**doc-writer**(D1 已改)、`integrator`=集成阶段实例(挂 `release-operator` 语义)。
  - **runtime 组件≠角色**:`orchestrator`(fleet 主脑/lane root)、`context-builder`、`lane-supervisor` 是 runtime 组件,不进 7 角色清单——它们可作 fleet 编排节点显示,但不当 persona 角色卡登记。
- **task 状态枚举(`AgentTaskRecord.status` · UI 只映射不猜)**:
  - 进行中 `queued/thinking/streaming/editing/running_tool/testing/reviewing/running/attached` → 活动动画 + 可 cancel + composer 保持可编辑。
  - 等待 `waiting_approval/needs_input/blocked` → 一等「需要人」状态灯(金脉冲·D1 `.wslane.s-need`/D10 `.attn`)。
  - 完成 `done/applied/discarded/archived` → 结果 + evidence + next action。
  - 失败/取消 `failed/cancelled` → recovery hint + retry/cancel。
  - 现有视觉码(run/done/gate/idle/wait)是上述枚举的显示分组,非新增状态；映射关系固定,勿另立枚举。
- **mutation_policy(与 route 正交·D-MUTPOLICY)**:`autonomous` / `propose-only`(原 manual review) / `read-only`(Plan 模式强制)。任何 route 都可组合。载体 = D4 步骤4。
- **gate_strength(lane 一等事实·D-GATESTR)**:`full`(native/built-in) / `cooperative`(ACP) / `containment`(terminal/tmux)。fleet/监视常显 `●/◐/○`。

## 品牌资产（`brand-assets/`）
- `app-icon-1024.png`(深色,Tauri 主图标)/ `app-icon-light-1024.png`(浅色变体)。
- `favicon-{512,192,64,32}.png`(网站/PWA 多档)。
- `og-banner-1200x630.png`(README / og:image)。
- `icon.svg`(92×92 矢量母版,任意尺寸无损导出/改色)。瞳孔金色是小尺寸识别点,不要去掉。
- 字标:`[◉]` glyph + `v`(金)`iden`(青);吉祥物机器人头青色线稿。

## 图标与贡献约定
- **GUI 图标**:一律走 `GUI/gui-icons.jsx`(`window.ICONS` / `GuiIcon` / `AgentLogo`,见上「图标目录」);**GUI 禁 emoji**,要图标用线性 SVG。
- **TUI 图标**:终端态用 box-drawing / Unicode 几何符(◉ ● ◆ ⚒ ♙ ▸ ╭─);避免 emoji。
- 新增组件 / 图标先在本目录登记(类名或 key + 最小 HTML),再写进 CHANGELOG。**没登记 = 临时草稿。**
