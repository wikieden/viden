# Viden 设计自查清单（CHECKLIST · Definition of Done）

> 给每个 Claude / 协作者的**收尾自查**。配合 `CLAUDE.md` 的「收尾同步表」与 `DESIGN-REF.md` 的组件目录使用。
> 标 🤖 的由 `tools/` guard 机检(`read_file` 脚本全文 → 粘进 `run_script` 跑,看末行 `RESULT: PASS|FAIL`)。
> 核心信条:**任何影响产物的改动都带一个同步义务,漏一项 = 漂移。**

---

## 0 · 单一真源地图（改哪里 = 改这一处，全站跟随）
| 要改的东西 | 唯一真源 | 谁引用它 |
|---|---|---|
| 颜色 / 字号 / 间距 / 圆角 / 阴影 / 密度 | **`tokens.css`**(根) | 所有页 `<link href="(../)tokens.css">`,组件只用 `var(--*)` |
| 5 皮肤 × 明暗换肤 | `tokens.css` 的 `[data-skin][data-mode]` 段 | 组件不动,换 `data-skin`/`data-mode` 即换肤(amber/phosphor 仅 dark) |
| 终端组件(终端框/状态栏/lane/闸/后端 chip) | **`TUI/tui-kit.css`** 的 `.v*` 类 | TUI 页 `<link href="tui-kit.css">` |
| 桌面组件(窗口壳/标题栏/活动 rail/lane/Environment) | **`GUI/gui-kit.css`** + `GUI/gui-statusbar.jsx` | GUI 页引用 |
| EN/中 切换 + `window.t` / `window.tk` | **`i18n.js`**(根) | 子目录页 `../i18n.js`;根页 `i18n.js` |
| 跨页复用文案(chrome/通用术语) | **`i18n-dict.js`**(根) | `tk('key')` / `<… data-i18n-key="key">`;页面独有长句仍用内联 `t(en,zh)` |
| 皮肤+密度切换 + `window.RC` | **`chrome.js`**(根) | 同上 `../chrome.js` |
| Tweaks 面板 | **`tweaks-panel.jsx`**(根) | 同上 `../tweaks-panel.jsx` |
| 屏目录 / 状态 / 进度 | **`docs/screens-status.js`** | 门户 `index.html` 运行时直读渲染卡片 + 进度 |
| 设计决策护栏 | **`docs/SPEC.md`**(@DECISION) | 动设计前 grep;开放问题索引 → Core 审查看板 |
| GUI 主窗口列宽 | `tokens.css` `--rail-act/-left/-right` | D1 同源,各 GUI 页引用 token,勿写死 px |

> **铁律**:以上任一真源**绝不复制副本**。看到 `Core/i18n.js` 之类的子目录副本 = 漂移源,删掉并改回 `../`。

---

## 1 · 开工前（造任何 UI 元素之前）
- [ ] **先 grep 再写**:`grep` 一下 `docs/DESIGN-REF.md` + 现有 `.v*`/`.gui-*` 类。命中就**抄类名直接用**,别重造已沉淀的组件。
- [ ] 确认目标平台壳:TUI 用 `tui-kit.css`、GUI 用 `gui-kit.css`、展示页用 `--page-*` 系。
- [ ] 需要的颜色/字号/间距**已在 `tokens.css` 有 token** 吗?没有就先去 tokens.css 加,再用 `var(--*)`——**不要在组件里写裸值**。

## 2 · 写代码时
- [ ] 只用 `var(--*)`,**禁裸 `#hex` / `rgba()` / 假 fallback** `var(--x,#fff)`。🤖 `check-tokens.js`
- [ ] 新页 `<head>` 接入:`tokens.css` + 字体 + `../i18n.js`(+ `../chrome.js`,React 页加 `../tweaks-panel.jsx`)。
- [ ] 复用组件 = 抄 DESIGN-REF 的类名 + 最小 HTML;**不在页面里内联自造**已登记的组件。
- [ ] 屏 / 高层级区域加 `[data-screen-label]`(便于评论定位);改结构时保留 `data-comment-anchor`。
- [ ] 文案中英双语用 `window.t('EN','中')`;CJK 正文行高 1.55–1.7。
- [ ] 用 flex/grid + `gap` 排版,**不靠裸 inline + 空白节点**(直接编辑更稳)。
- [ ] 固定尺寸内容(屏/卡)letterbox scale-to-fit;不要 `height:100%+overflow:auto` 撑内层。

## 3 · `done` 前（DoD · 逐行过）
> 与 `CLAUDE.md` 收尾同步表一致 —— 任一改动触发对应同步义务:

- [ ] 改了 **`tokens.css`** → 同步 `DESIGN-REF.md` Token 速查表。
- [ ] **新增/改/删可复用组件** → 在 `DESIGN-REF.md` 组件目录登记/更新(类名 + 最小 HTML)。**没登记 = 临时草稿,不算可复用。**
- [ ] **新增一个屏** → 在 `docs/screens-status.js` 加一条(id/track/state/kind/file/中英) → 跑 `check-status.js`;门户 `index.html` 运行时直读自动出卡 + 更新进度。🤖 `check-status.js`
- [ ] **跨页复用文案** → 同一词多页出现就进 `i18n-dict.js`,用 `tk()`/`data-i18n-key` 取;页面独有长句才内联 `t(en,zh)`。
- [ ] **新平台 / 大改造** → 遵 `docs/PROTO-STANDARD.md`:全屏产品入口 + 组件库 + 设计稿索引 + `pages/(assets/)` + kit + `_archive/`;入口窗口化、索引 iframe 保活秒切;根 index.html 收敛为产品卡 + 组件库卡。
- [ ] 改了**颜色相关**值 → 自查禁裸 `#hex`/`rgba()`/假 fallback。🤖 `check-tokens.js`
- [ ] **任意定档** → 写当天 `CHANGELOG.md`(先 `grep '^## <今天>'`,命中即 append;最新日期段置顶;一条 = 1 行 + ≤3 子 bullet)。🤖 `check-changelog.js`
- [ ] 改了 **`tools/` 脚本或 guard 机制** → 定档前**必附本次改动后的实际运行证据**(真实环境跑通;环境不具备就仿真到能仿真的最大范围并在 CHANGELOG **如实标注未验证部分**)。声明先于验证 = 事故模式(2026-07-02 两例:node wrapper 发布即坏、.pinbtn spec 写双入口实物单入口)。
- [ ] 改了 **SPEC 决策条目** → 逐句核对条目描述的每个实物(入口/类名/行为)在代码里真实存在——**spec 写了的必须渲染得出来,渲染不出来的不写进 spec**。
- [ ] 加的脚本/样式是**单一真源**吗?没在子目录留副本吧?(见 §0)
- [ ] 页面载入无 console 报错;6 套皮肤 + 3 档密度切换不破版。

---

## 4 · 跑 guard（三条机检闸）
> 纯只读脚本:`read_file` 脚本全文 → 整段粘进 `run_script` 执行,看末行 `RESULT`。

| guard | 守什么 | FAIL 怎么办 |
|---|---|---|
| `tools/check-tokens.js` | 代码里新增的裸 `#hex` / `rgba()` / 假 `var(--x,#fff)` fallback | 收编进 `tokens.css` 或改用 `var(--*)`;确属基线再更新 baseline |
| `tools/check-changelog.js` | CHANGELOG 同日重复段 / 超长 / 单条过深 | 合并同日段、精简、深内容分流到对应 doc |
| `tools/check-status.js` | `screens-status.js` 重复 id / 非法 state·kind / track 越界 / **file 悬空(点了 404)** | 修真源对应字段;补建缺失文件或改正路径 |

---

## 5 · 迁移欠债（组件共享 · 持续收口）
> 「改一处别处不跟」的根因是**页面没接 kit、各自内联**。新页直接用 kit;存量页逐屏迁移。状态见 `index.html` 卡片的 **KIT / INLINE** 徽章。
- **TUI(已定稿 · 样板)**:全部产品屏已接 `tui-kit`;结构按 `PROTO-STANDARD`(入口 `统一原型` / `组件库` / `设计稿索引` + `pages/`)。新平台照此。
- **GUI**:视觉真源 = `D1 驾驶舱`。`gui-kit.css` / `gui-statusbar.jsx` / `gui-titlebar.jsx` / `gui-icons.jsx` 已就位;**D2/D4/D5/D6/D7/D8/D9/D10/D11/D12/D13/D14 + 组件库 已接 gui-kit**(窗口壳/标题栏/活动 rail/lane/状态栏走套件);**D1 旗舰有意不接**——它是视觉真源、kit 是它的镜像(2026-09-09 更正:原文列 D1 为「已接」是笔误,实物从未 link;反接会属性泄漏,实测见 PROTO-STANDARD §5)。有意保持自包含:D2横/D3 召唤坞 · Pip 装饰。新页直接用套件。
- 迁移法:页头加 `<link href="(tui|gui)-kit.css">` → HTML 改类名 → 删被取代的内联 CSS → 截图比对基准屏 → 更新 index.html 徽章为 KIT。
