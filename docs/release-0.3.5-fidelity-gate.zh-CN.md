# Viden 0.3.5 — GUI 设计保真闸（G10）

English version: [release-0.3.5-fidelity-gate.md](release-0.3.5-fidelity-gate.md)

本文是 `docs/gui-version-functional-design.md` 里的 GUI-4 闸：*token 映射可校验；
组件不写裸色、零散字号或临时间距；差异记为已接受偏差；无法解释的漂移阻塞发布。*

| 字段 | 值 |
| --- | --- |
| 分支 | `claude/gui-fidelity-g10` |
| 工作树 | `.worktrees/gui-fidelity-g10` |
| 基线 | `6c06a831bccb80686ef4bbbc5b136afef51d3808`（`claude/int-0.3.5`，含 G8、G9、G9b、H4、E3b） |
| GUI 版本 | `0.1.0-rc.5` — **本批次不升版** |
| Harness | `apps/gui/evidence/main-window-interactions/qa.html` + `qa.ts`，每个 `?state=` 渲染一个生产状态 |
| 采集 | headless Chrome `--headless=new --disable-gpu --hide-scrollbars --window-size=1440,1180 --virtual-time-budget=6000`，对 `127.0.0.1:4222` 上的 vite dev server |
| 皮肤/明暗对 | `aurora/dark` + `en`、`ice/light` + `zh-CN`（harness 出货的两对；`aurora` 为默认） |
| 帧 | `apps/gui/evidence/fidelity-gate/` — 44 帧，覆盖两对，逐张已看 |
| 结论 | **headless 闸通过；四条原生腿因整批次屏幕锁定未执行** |

## 屏幕锁定

本闸的每条原生腿都以 `…/scratchpad/screenlock` 为前置。它在启动时即为锁定，
且全程未解锁：

```
time=2026-09-13T11:29:48Z CGSSessionScreenIsLocked=1 onConsole=1   # 启动
time=2026-09-13T11:57:07Z CGSSessionScreenIsLocked=1 onConsole=1   # 原生腿之前
time=2026-09-13T12:04:06Z CGSSessionScreenIsLocked=1 onConsole=1   # 收尾
```

没有发送任何按键，没有启动原生应用，也没有等待屏幕。需要屏幕的四条腿在下文记为
**未执行**，并附上上述原文行；既不记为通过，也不记为失败。

## 1. 界面清单

0.3.5 候选承载的每个驾驶舱界面、它的设计真源，以及渲染它的 qa 状态。22 个界面
**全部已有 qa 状态，本闸未新增**，因此没有
`test(gui): add the qa states the fidelity gate needs` 这一提交。

| # | 界面 | 设计真源 | qa 状态 | 结论 |
| --- | --- | --- | --- | --- |
| 1 | D1 驾驶舱外壳 + 转录 | `GUI/Viden - 桌面驾驶舱 (GUI).html` | `d1` | 通过 |
| 2 | D1 上下文坞页签 | 同上 | `dock-environment` | 通过 |
| 3 | D2 决策中心 | `GUI/pages/…D2 决策中心…` | `d2` | 修复后通过 |
| 4 | D4 Lane 创建 | `GUI/pages/…D4 Lane创建流程…` | `d4` | 通过 |
| 5 | D4 agent 绑定（G9b） | 同上 | `d4-agent-binding-started` | 通过 |
| 6 | D6 恢复态 | `GUI/pages/…D6 空态与错误态…` | `d6-actions` | 通过 |
| 7 | D10 Lane 监视器 | `GUI/pages/…D10 Lane监视器…` | `d10-ticker` | 修复后通过 |
| 8 | D11 项目接入 | `GUI/pages/…D11 首启与项目接入…` | `d11` | 修复后通过 |
| 9 | D12 集成闸 | `GUI/pages/…D12 集成闸冲突退回…` | `d12-actions` | 修复后通过 |
| 10 | D13 Fleet 与 Workflow | `GUI/pages/…D13 Fleet 编排与 Workflow…` | `d13` | 修复后通过 |
| 11 | D14 审计时间线 | `GUI/pages/…D14 审计与时间线…` | `d14-audit` | 修复后通过 |
| 12 | D15 Plan Studio | `GUI/pages/…D15 Plan Studio…` | `plan-proposed` | 通过 |
| 13 | D16 Agent Board | `GUI/pages/…D16 Agent Board…` | `board-lanes` | 修复后通过 |
| 14 | 权限坞 | D1 驾驶舱 + 组件库 | `permission-ask` | 通过 |
| 15 | 始终 / 编辑坞（G9） | D1 驾驶舱 | `dock-always-edit` | 通过 |
| 16 | DiffReview | D1 驾驶舱、`DESIGN-REF`「D1 次级视图」 | `review` | 通过 |
| 17 | EvidenceView | 同上 | `evidence` | 通过 |
| 18 | 命令面板 | D1 驾驶舱 | `palette` | 修复后通过 |
| 19 | 专注模式 | D1 驾驶舱 | `centre-focus-mode` | 通过 |
| 20 | 欢迎页 | D1 驾驶舱 | `welcome-fill` | 通过 |
| 21 | 设置浮层 | `GUI/gui-settings.jsx` | `settings` | 修复后通过 |
| 22 | Lane rail | D1 驾驶舱 | `lane-rail` | 通过 |

## 2. Token 映射 —— 本闸的首要发现

GUI-4 要求 token 映射「可校验」。此前无人校验，客户端已严重漂移。
`apps/gui/tests/visual/token_registration.spec.ts` 现在对每个 GUI 自有样式表断言
两条性质，两条在基线提交上都失败：

```
apps/gui/src/components/command_palette.css:34: box-shadow: var(--shadow-overlay, 0 2.5rem 6.25rem -1.875rem rgb(0 0 0 / 90%));
```

以及 **36 处 `var()` 引用，指向 13 个 `tokens.css` 未登记、别处也未声明的自定义属性**。

这不是化妆问题。无 fallback 的 `var(--不是-token)` 在计算值阶段即为非法，
**整条声明**被丢弃且不报错。两张前后对比帧已提交在
`apps/gui/evidence/fidelity-gate/drift-before/`：

- **D2** —— `Decision Center` 以 16px Times 渲染，`cargo test` 工具入参预览以
  **比例衬线**而非等宽渲染，因为 `--lh-code` 让它的 `font:` 简写整条失效。
  命令栏、闸栏与 `<pre>` 完全没有面板底。
- **D14** —— `Audit trail` 与全部七个筛选 chip 是 Times；头栏、模式栏、筛选栏
  贴在 `--bg-base` 上；`denied 1` 汇总段以普通主文墨色绘制，因为 `--warn` 不是 token。

### 已修漂移（每项一个提交）

| 提交 | 漂移 | 位点 |
| --- | --- | --- |
| `0789eab6` | `test(gui)` —— 校验本身，**刻意为红**，列出全部 36 处 | — |
| `d81d28d5` | 六个让整条声明被丢弃的未登记名 | 25 |
| `63e5d0bb` | 命令面板的裸字面 fallback，含唯一一处裸色 | 9 |
| `f152ac1f` | D14 的两个未登记名，其中一个掩盖了失败 | 2 |
| `1f284ead` | 套件控件背后透出的 UA `ButtonFace` 填充 | 10 |
| `c564879c` | 缺失的 `color-scheme`，以及它暴露出的 UA `buttontext` 那一半 | 全局 |

所用映射，以及每个替换为何是套件自己登记的语义而非猜测：

| 未登记 | 已登记 | 依据 |
| --- | --- | --- |
| `--bg-raised` | `--bg-panel` | 每个位点都是头栏、卡片、代码块或闸栏 —— 面板面。设计稿本身对这些用 `--bg-panel`（各 3–5 处）；`tokens.css` 把 `--bg-elev` 记为弹层/模态档。 |
| `--bg-selected` | `--bg-sel` | 三处全是 `[aria-pressed="true"]` chip；`--bg-sel` 即登记的选中行底。 |
| `--fg-default` | `--fg-primary` | 套件的默认前景。 |
| `--fs-md` | `--fs-base` | D15 的 `.d15-title` 已经写着 `font: var(--fs-base)/var(--lh-ui) var(--font-sans)` —— 同一条声明，用的是已登记 token。 |
| `--lh-code` | `--lh-body` | 已登记梯级是 `--lh-tight/ui/body`；`<pre>` 正文取 `--lh-body`。 |
| `--sp-7` | `--sp-6` | 见已接受偏差。 |
| `--warn` | `--warning` | 纯拼写错误；fallback 让失败渲染成普通计数。 |
| `--fg-tertiary` | *（去名）* | 前景梯级只有四档且无 tertiary；`--fg-faint` 被明确限定为「仅限大字·非正文·UI 描边」且只保 ≥3:1，11px chip 标签不属此列。chip 保留 fallback 本就产出的 `--fg-secondary` —— 无视觉变化。 |
| `--radius-lg, 0.8125rem` | `--r-lg` | 已登记的卡片圆角。 |
| `--shadow-overlay, …rgb(0 0 0 / 90%)` | `--shadow-pop-gui` | 套件登记语义原文即「GUI 下拉 / 菜单 / 弹层浮投影」。 |
| `--fs-2, 0.9375rem` | `--fs-base` | 15px，完全一致。 |
| `--fs-1, 0.8125rem` | `--fs-sm` | 13px，完全一致。 |
| `--fs-0, 0.65625rem` / `0.625rem` | `--fs-micro` | 见已接受偏差。 |

### 裸色 grep（必需，且必须为空）

```
$ grep -rnE "#[0-9a-fA-F]{3,8}\b|rgba?\(" apps/gui/src --include="*.css"
$ echo $?
1
```

为空。基线提交上唯一一处命中是命令面板的投影 fallback，已由 `63e5d0bb` 移除。

## 3. 组件对齐

方法：解析 `apps/gui/src/**/*.css` 中的每个选择器块，把其类名与
`docs/viden-design/Viden/GUI/gui-kit.css` 登记的 196 个类名比对。

- **被 GUI 分叉的已登记类：无。** 确有 56 条 GUI 规则点到了已登记类名，但每一条
  都作用在 GUI 合法扩展的已登记套件容器之内 —— `.review`、`.evwrap`、`.diffbody`、
  `.cc`、`.ppch` 或 `#app >`。这正是被记录的做法：套件由九个 GUI 样式表 `@import`
  而非复制，扩展写在其内部。没有任何 GUI 规则在裸的、未限定的选择器上重绘已登记类。
- **无套件登记的 GUI 类名：585 个**，集中在各屏自有族（`d1-` 104、`d15-` 54、
  `gset` 39、`d2-` 35、`d10-` 33、`d14-` 33、`d12-` 24）。它们是 GUI 自有的屏内骨架，
  本身不构成漂移。README 已记录的套件晋升欠账 —— `.envp` 族、`lane_tabs`、`tool_row`
  —— 本批次未改动，仍归设计包处置。

## 4. 已接受偏差

| 偏差 | 理由 |
| --- | --- |
| D11 步骤徽标 `.stp .nn` 用 `--sp-6`（24px），而 D11 设计稿画的是 26px | `tokens.css` 定义了 `--sp-1…6, 8, 10, 12, 16` 并跳过 7，没有任何已登记档位落在 26 或 28px。`--sp-6` 最近。修复前该声明被整条丢弃、徽标塌缩到文字尺寸，因此 24px 相对出货状态是大幅改善。已为套件立案。 |
| 命令面板圆角取 `--r-lg`（11px），而裸字面是 13px | 字阶的意义就是客户端不再在档位之间自造。 |
| 命令面板取 `--fs-micro`（11px），而裸字面是 10.5px 与 10px | 同一个名字带着两个不同字面，本身就证明它从来不是 token。 |
| `.d14-fchip` 保留 `--fg-secondary` 而不改用 `--fg-faint` | 套件把 `--fg-faint` 限定给大字、非正文、≥3:1。11px chip 标签属正文。相对出货状态无视觉变化。 |
| G8 的 record rail 位于中栏之内 | 既有，G8 已记录；本批次未改。 |

## 5. 已立案未修的漂移

以下为设计包缺陷。本分支不编辑 `docs/viden-design/**`，故逐条记录，交由套件处置。

| # | 发现 | 证据 |
| --- | --- | --- |
| F1 | `gui-kit.css` 给 `.tbtbtn`、`.gitchip` 定了尺寸、圆角、颜色与透明边框，**却从不给 `background`**。设计稿把它们画成非交互元素；客户端为了键盘可达必须渲染成真 `<button>`，于是透出 UA 的 `ButtonFace` 填充。在每个承载驾驶舱的界面上，三个标题栏工具图标背后实测 `rgb(107,107,107)`。已在 `1f284ead` 于 GUI 侧绕过；套件应声明该背景。 | `apps/gui/src/ui/theme.css`，提交 `1f284ead` |
| F2 | **`--accent` 在 `ice/light` 下违反套件自己的对比度契约。** `tokens.css` 第 113 行写着「accent/gold≥4.5」，但 `ice/light` 把 `--accent` 解析为 `#5a8af0`，作为小字在 `--bg-base` `#f3f7fc` 上只有 **2.57:1**，在坞页签面上 **3.08:1**。所有 accent 色标签均不达标：`.actbtn`（活动 rail 当前项）、`.wslane`（选中 lane）、`.dtab`、`.fchip`、`.gset-option`。GUI 无法在不自带第二套调色板的前提下修复，而 `AGENTS.md` 明令禁止。 | `accessibility.json`，9 个界面 |
| F3 | `--sp-7` 在 `DESIGN-REF.md` 里被记为 `--sp-1…--sp-16` 4px 基准的一员，但 `tokens.css` 未定义；D11 设计稿对步骤徽标硬写 26px。要么登记该档位，要么把设计真源改到 `--sp-6`。 | `d11_intake.css:49–50` |
| F4 | GUI 自有 CSS 中仍有本闸范围之外的裸间距与裸字号 —— `.d14-fbar`/`.d14-fchip`/`.d14-rollup` 带着 `gap: 6px`、`padding: 8px 14px`、`font-size: 11px`/`10.5px`、`border-radius: 999px`、`margin: 0 4px`。GUI-4 的「无临时间距」一条尚未达成。此处不修：那是一次普查，不是本闸的帧暴露出的对齐漂移。 | `d14_audit_timeline.css:229–295` |
| F5 | 有 17 个互不相同的交互控件在至少一个方向上低于套件 ≥28px 的点击目标规则，两对皮肤下均如此。最突出的是 `.sb-gear` 15×11、`.d1-topbar-dirty` 15×12、`.d14-fchip` ×7 高 19px、`.gset-close` 19×17、`.d1-mpill` ×3 高 24px、`.review-icon` 23×23、`.dtab` 26px、`.gitchip` 27px。抬高它们会改变驾驶舱的密度基线，属设计包决策。 | `accessibility.json` |
| F6 | `.review-stage` 在 `aurora/dark` 下 3.48:1、`ice/light` 下 3.37:1，字号 11px，低于 4.5:1。这是本批次深色对中唯一的对比度失败。 | `accessibility.json` |

## 6. 可访问性记录

机器可读：`apps/gui/evidence/fidelity-gate/accessibility.json` —— 22 个界面 × 2 对，
逐元素记录角色、可访问名解析、目标尺寸与 WCAG 2.1 对比度。

| 性质 | 结果 |
| --- | --- |
| 每个可聚焦元素都有可访问名 | **通过** —— 22 界面 × 2 对，零缺失 |
| 每个可 Tab 元素都能接受焦点 | **通过** —— 22 界面 × 2 对 |
| 对比度，`aurora/dark` | **除 F6 外通过**（`.review-stage` 3.48:1） |
| 对比度，`ice/light` | **失败 —— F2**，`--accent` token；每界面 1–5 对不达标 |
| 点击目标 ≥ 28px | **失败 —— F5**，17 个控件 |
| `color-scheme` 跟随皮肤 | **`c564879c` 后通过** —— 修复前在 `ice/light` 下报告为 `dark` |

方法及其边界记在产物里。两点值得在此点名：对比度遍历取最近的不透明祖先背景，
不建模分层半透明；且未执行平台可访问性树审计（NSAccessibility / AXAPI），
因此这些是 DOM 自身的角色与名字，不是 VoiceOver 会朗读的内容。

## 7. 纯键盘遍历

headless，覆盖 17 个界面。原生窗口内真实按键驱动的遍历属原生腿，未执行。

| 检查 | 结果 |
| --- | --- |
| 焦点顺序 | 与设计隐含的 rail 优先顺序一致：git chip → 标题栏工具 → 活动 rail → 中栏 → 坞 → 状态栏。每个驾驶舱界面 43–72 个可 Tab 元素。 |
| 死角 | **未发现。** 每个界面上每个可 Tab 元素都接受了焦点。 |
| 仅指针可用的控件 | 无。两个疑似项经核查排除：被禁用的坞页签（`Terminal`、`Code`、`Diff`、`Docs`）是「禁用且标注」，正是诚实原则在生效，不是死角；`div.palrow` 不可 Tab 是正确的，因为面板是真正的 combobox。 |
| 面板语义 | `input[role=combobox]` + `aria-controls` → `ul[role=listbox]`，其中 `li[role=option]` 带 `aria-selected`；`ArrowDown` 把 `aria-activedescendant` 从 `gpal-row-0` 移到 `gpal-row-1`。 |
| Esc 链 | Esc 关闭了命令面板、设置浮层、DiffReview、EvidenceView 与项目选择器 —— 五中五。 |

## 8. 性能

机器可读：`apps/gui/evidence/fidelity-gate/performance.json`。每项三次取中位数。
界限是本闸自定的，且在跑之前先行声明；Core 未为 GUI 发布 event-to-visible 预算，
因此没有任何界限引自契约。

| 记录 | 界限 | 中位数 | 结果 |
| --- | --- | --- | --- |
| 冷启动到绑定 —— D1 | 400 ms | 40.1 ms | 通过 |
| 冷启动到绑定 —— D14 页加载 | 400 ms | 41.3 ms | 通过 |
| 冷启动到绑定 —— DiffReview | 400 ms | 47.1 ms | 通过 |
| 冷启动到绑定 —— D16 | 400 ms | 38.5 ms | 通过 |
| 冷启动到绑定 —— D15 | 400 ms | 39.0 ms | 通过 |
| 坞内文件读取（Files 页签绑定） | 400 ms | 49.1 ms | 通过 |
| 路由切换，rail 会话 → Lane 监视器 | 150 ms | 8.9 ms | 通过 |
| 坞页签切换，环境 → 文件 | 150 ms | 6.4 ms | 通过 |
| 转录页 500 行（2,500 DOM 节点） | 250 ms | 19.8 ms | 通过 |
| Diff 渲染 5,000 行补丁（15,004 DOM 节点） | 500 ms | 98.9 ms | 通过 |

**这些是 dev server 的数，不是发布包的数。** 任务书要求在发布包上测；屏幕锁定，
原生应用从未启动。这些数字只界定客户端自身的渲染开销，别无其他。Diff 那条最该盯：
DiffReview 不做虚拟化，5k 行渲染对字节上限是线性的。

## 9. 未执行的腿 —— 屏幕锁定

| 腿 | 状态 |
| --- | --- |
| CJK 输入法（拼音输入到编辑器、Lane 任务字段、驳回理由与带类型的 Edit 字段；组合、候选提交、组合中不触发和弦；采集两次；恢复输入源） | **未执行 —— 屏幕锁定于 `time=2026-09-13T11:57:07Z CGSSessionScreenIsLocked=1 onConsole=1`。** 未发送任何按键，输入源从未变更，因此无需恢复。 |
| 原生窗口内的纯键盘遍历 | **未执行 —— 锁定行同上。** §7 的 headless 遍历是交付的替代，并已如此标注。 |
| 发布包性能（`tauri build --bundles app`，三次取中位数） | **未执行 —— 锁定行同上。** 未构建任何 bundle，未取任何原生计时。 |
| 任何界面的原生采集 | **未执行 —— 锁定行同上。** `evidence/fidelity-gate/` 中的每一帧都是对生产渲染函数的 headless Chrome 采集。 |

## 10. 检查

```
npm --prefix apps/gui test -- --run          Test Files 64 passed (64) · Tests 943 passed (943)   VITEST_EXIT=0
npm --prefix apps/gui run build              tsc --noEmit && vite build，93 modules              BUILD_EXIT=0
cargo test -p viden-gui                      全部套件 ok，0 failed                                CARGO_EXIT=0
cargo test -p viden-gui --test capture_projections -- --ignored   1 passed                       CAPPROJ_EXIT=0
grep -rnE "#[0-9a-fA-F]{3,8}\b|rgba?\(" apps/gui/src --include="*.css"   （空）                  GREP_EXIT=1
git diff --check                             （空）                                              DIFFCHECK_EXIT=0
scripts/check-doc-pairs.sh / check-doc-links.sh   见批次报告
PNG 下限                                     44 帧，最小 39,240 B，无一低于 30 KB
```

## 11. 结论

headless 闸**通过**。经五项漂移修复后，每个界面在两对出货皮肤下都与其设计真源一致，
没有已登记组件被分叉，裸色 grep 为空，每个 token 引用都能解析，未发现键盘死角，
十条性能记录全部落在界限之内。

有两点使它还不是完整的闸。四条腿需要未锁定的屏幕，已在上文记为未执行。以及
**F2 是真实的发布风险**：`--accent` 作为小字在浅色皮肤下处处不满足 WCAG AA，
且违背套件自己写下的契约。之所以立案而非修复，是因为修复属于 `tokens.css`，
而本分支不得触碰它。
