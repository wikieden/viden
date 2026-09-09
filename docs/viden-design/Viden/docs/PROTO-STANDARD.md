# 平台原型结构规范（PROTO-STANDARD）

> 以 **TUI 定稿结构**为样板,**GUI / 未来平台一律照此做**。配合 `CLAUDE.md`、`docs/DESIGN-REF.md`、`docs/CHECKLIST.md` 使用。
> 一句话:**每个平台 = 一个全屏产品入口 + 一个组件库 + 一个设计稿索引 + pages/ + 规范(kit),真源只一套。**

## 1 · 每个平台的目录布局
```
<PLATFORM>/                          （TUI / GUI）
├── Viden - <旗舰原型> (<P>).html    # ★ 全屏产品入口（打开即用·窗口化）
├── Viden - 组件库 (<P>).html        # 组件库逐件陈列（查阅/复制）
├── Viden - 设计稿索引 (<P>).html     # 设计稿导航（侧栏 + iframe·保活秒切）
├── <p>-kit.css                       # 组件样式真源（TUI=.v* / GUI=.gui-*·D1 同源类）
├── pages/                            # 各设计稿屏（平铺）
│   └── assets/                       # 屏专属 jsx/js
└── _archive/                         # 改造前冻结快照（不扫·可回滚）
```
根目录只放**入口 + 组件库 + 索引 + 规范**;具体屏全进 `pages/`,脚本进 `pages/assets/`。

## 2 · 单一真源地图（铁律·改一处全跟随）
| 东西 | 唯一真源 |
|---|---|
| 数值(色/字/距/圆角/阴影) | 根 `tokens.css`（只此一处可写 `#hex`） |
| 组件样式 | `<p>-kit.css` 的 `.v*` / `.gui-*` |
| 旗舰产品原型 | `Viden - <旗舰>`（一份·绝不复制驾驶舱代码） |
| 共享脚本 | 根 `i18n.js` / `chrome.js` / `tweaks-panel.jsx`（子目录 `../` 引） |
| 组件目录/速查 | `docs/DESIGN-REF.md` |

## 3 · 入口 / 交互约定
- **旗舰原型 = 全屏可用产品**,不带文档外壳(无 kicker/标题/说明);**窗口化**:四边四角拖拽 resize、拖标题栏移动、点绿灯 / 双击标题栏最大化↔还原(复用 TUI `统一原型` 末尾的窗口管理器脚本)。
- **设计稿索引 = 侧栏导航 + iframe**;iframe **保活缓存**(首访加载、再访秒切无闪、**无白屏**——见 §3b);顶栏皮肤切换经 `rc-state` reload 内嵌屏跟随。
- 根 `index.html` 每平台收敛为 **1 张产品入口卡 + 1 张组件库卡**(不再铺一堆散卡)。

## 3b · 无白屏导航引擎（设计稿索引 iframe 切换·别退化成 display:none）
> 借鉴 hirobot `PROTOTYPE-ARCH.md`。索引页切 iframe **绝不**新建+重载、也**绝不**用 `display:none`/`visibility:hidden` 藏旧屏——隐藏会让浏览器丢弃 iframe 渲染层,再显示时整页从头重绘 → 每次切一下闪白(TUI/GUI 设计稿索引 2026-06-29 修掉的正是这反例)。范式四件套:
1. **常驻缓存 iframe**:每个屏按 file 缓存一个 iframe(`frames[file]`),只建一次(`make()`)。
2. **空闲预热**:首屏稳定后 `requestIdleCallback(prewarm)` 把其余屏全部建好,常驻底层 `z-index:0`——之后任意切换皆秒开。
3. **z-index 叠放(全程可见)**:所有 iframe 绝对定位 `inset:0` 重叠,当前页 `z-index:1` 盖住底层 `z:0`。**绝不 display:none**——切换 = 只改 z-index,纯合成零重绘。
4. **冷页就绪门控**:点到尚未预热的冷页时,旧屏(z:1)留显不动、加 `.loading`;待新 iframe `load` 事件且 `want` 仍指向它,才 `raise()` 提到顶层。**不要**提前 raise,否则露白一帧。
> 反例(已修):早期 `frames[k].style.display=block/none` 显隐切换 → 隐藏页丢渲染层,再切回闪白。`换肤=对所有 frame contentWindow.location.reload()`(底层 z:0 静默重载,当前页就地刷新)。

## 4 · 新平台改造流程（GUI 直接套用）
0. **快照**:整目录 → `<P>/_archive/`（GUI 已做过）。
1. 页头接 `<p>-kit.css`（GUI 现状:`gui-kit.css` 已就位,但 D1–D13 都没 link)。
2. **逐屏迁移**:内联自造 chrome(窗口壳 `.frame`/标题栏/活动 rail/`.wslane`/Environment/状态栏/输入区)→ 换成 gui-kit 已登记类;删被取代的内联 CSS;裸 `#hex`/`rgba` 收进 tokens 或 `var(--*)`。
3. **对齐基准 = D1 驾驶舱**(GUI 视觉真源);同内容的屏向它收敛结构。讲解型布局(卡片/对比/表)保留。
4. **旗舰原型**:把 D1 做成全屏窗口化产品入口(套用窗口管理器);它是 GUI 的"打开即用"那一个。
5. 建 `设计稿索引 (GUI)` + `组件库 (GUI)`;屏移进 `GUI/pages/` + `assets/`,同步相对路径。
6. 根 `index.html` GUI 区收敛为产品卡 + 组件库卡。
7. **收尾**:DESIGN-REF 文件结构/迁移状态、index 徽标、CHANGELOG;跑 `check-tokens` / `check-changelog`。
> 逐页迁移后**截图比对存档原样**,逐屏确认再继续(D1–D13 是复杂驾驶舱,分多轮)。

## 5 · 本轮沉淀（经验 / 坑）
- **中文文件名**:`run_script` 的 `readFile/saveFile` 不接受中文路径 → 中文名 HTML 一律用 `str_replace_edit` / `copy_files`,不要走 run_script。
- **移动屏的路径代价**:同深度移动(`screens/`↔`pages/`)多数 `../../` 不变;跨深度(根↔子目录)才需改 `../`↔`../../`。移动前先 grep 出每页引用差异(chrome/tweaks/kit/jsx/外链各页有无不同)。
- **原子批量编辑**:`str_replace_edit` 的 edits[] 是原子的——一处 `old_str` 不匹配会整批回滚;不确定某页有没有某引用时**单独发**,别和关键改动绑一批。
- **验证**:截图工具不抓 iframe 内容、预览偶发 flaky → 以"无 console 报错 + `eval_js` 探关键结构"为准,别死磕截图。
- **有意 baseline(非漂移)**:`.depth256`(256 色 ANSI 模拟)、`tui-screens.jsx` 的地形小地图色 = 数据/模拟用色,保留。
- **讲解页 vs 产品**:T0–T5 这类是设计依据(降为后排,从产品 `▤ design docs` 进);只有旗舰原型是"产品"。

### GUI 改造轮沉淀（2026-06-29）
- **gui-kit 全局类名碰撞**:页面专属类若与 gui-kit 全局类同名(D12 时间线 `.tl` vs 交通灯 `.tl`)→ 接 kit 前先把页面专属类改名(`.rline`),否则被全局规则覆盖错乱。**2026-09-09 再用一次(反向:新族进 kit 时撞已接 kit 的页面)**——DiffReview 族的 `.dl`/`.diffbody` 与 D2 决策中心自带的同名私有类是两个不同组件,改的是**页面专属类**、保留登记族名:D2 `.diffbody`/`.dl` → `.d2diff`/`.d2dl`、D14 `.evchip` → `.d14chip`;登记族名归 kit,新页要那套画法就接 kit。
- **同名定制组件别强接 kit**:召唤坞/装饰页的 `.composer`/`.side`/`.cbox` 是有意不同的变体——全量 link gui-kit 会让全局同名规则**属性泄漏**破坏布局。保持自包含(token-clean 即可)或改名再接,别硬接。
- **旗舰窗口化配方**:App 只渲染 cockpit `<Win>` · `#root` 全屏 stage · 复用窗口管理器(GUI 适配 `.titlebar` 拖拽 + `.tl i.c` 绿灯最大化)· 换肤走根 `../chrome.js`;去文档外壳但驾驶舱代码仍只一份。
- **镜像纪律(D1 ↔ gui-kit · 防漂移关键)**:D1 = GUI 视觉真源,gui-kit 是它的镜像;**改 D1 的 chrome 必须同步改 gui-kit**(否则消费 kit 的页面与 D1 漂移)。**不建议让 D1 反向消费 gui-kit**——同名定制组件会泄漏、且 D1 有意更丰富(`.statusbar` 配置弹层等),收益 < 风险。
  - **2026-09-09 实测坐实(不再是「不建议」,是不行)**:给 D1 加 `<link href="gui-kit.css">`(放内联 `<style>` 之前,让同名规则仍以 D1 为准)后无头 Chrome 逐视图像素比对 —— chat **2584 px 变化**、review 175 px、evidence/diagnostics 各 189 px。根因不是特异性,是**同名规则的部分属性泄漏**:D1 的 `.envrow .cv` 只声明 `color`/`flex`,kit 那条还带 `font-family`/`font-size` → 直接改掉 Environment 行的字形。同类分叉 47 处 + kit 独有选择器 43 处。**新家族要跨页复用就镜像进 kit(逐字一致),别让 D1 反接。**
- **baseline 随文件移动失真**:屏移动 / 脚本 root 化后 `check-tokens` baseline 路径会整批 "removed" → 复查确认无新增真违规后 `args=['--write-baseline']` 重固化,并在 CHANGELOG 注明原因。
