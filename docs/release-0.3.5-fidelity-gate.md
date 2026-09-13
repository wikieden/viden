# Viden 0.3.5 — GUI design-fidelity gate (G10)

Chinese version: [release-0.3.5-fidelity-gate.zh-CN.md](release-0.3.5-fidelity-gate.zh-CN.md)

This is the GUI-4 gate from `docs/gui-version-functional-design.md`: *token
mapping is checkable; components do not write raw colors, scattered type sizes,
or temporary spacing; differences are recorded as accepted deviations;
unexplained drift blocks release.*

| Field | Value |
| --- | --- |
| Branch | `claude/gui-fidelity-g10` |
| Worktree | `.worktrees/gui-fidelity-g10` |
| Base | `6c06a831bccb80686ef4bbbc5b136afef51d3808` (`claude/int-0.3.5`, after G8, G9, G9b, H4, E3b) |
| GUI version | `0.1.0-rc.5` — **not bumped by this batch** |
| Harness | `apps/gui/evidence/main-window-interactions/qa.html` + `qa.ts`, one production render per `?state=` |
| Capture | headless Chrome `--headless=new --disable-gpu --hide-scrollbars --window-size=1440,1180 --virtual-time-budget=6000` against the vite dev server on `127.0.0.1:4222` |
| Pairs | `aurora/dark` + `en`, `ice/light` + `zh-CN` (the two pairs the harness ships; `aurora` is the default) |
| Frames | `apps/gui/evidence/fidelity-gate/` — 44 frames, both pairs, every one viewed |
| Verdict | **the headless gate passes; four native legs did not run because the host screen was locked for the whole batch** |

## Screen lock

Every native leg in this gate is gated on `…/scratchpad/screenlock`. It
reported locked at launch and never unlocked:

```
time=2026-09-13T11:29:48Z CGSSessionScreenIsLocked=1 onConsole=1   # launch
time=2026-09-13T11:57:07Z CGSSessionScreenIsLocked=1 onConsole=1   # before the native legs
time=2026-09-13T12:04:06Z CGSSessionScreenIsLocked=1 onConsole=1   # at close
```

No keystroke was sent, no native application was launched, and nothing waited
on the screen. The four legs that require it are recorded as **not run** below
with the exact lines above, not as passes and not as failures.

## 1. Surface inventory

Every cockpit surface the 0.3.5 candidate hosts, its design source, and the qa
state that renders it. All 22 already had a qa state; **the gate added none**,
so there is no `test(gui): add the qa states the fidelity gate needs` commit.

| # | Surface | Design source | qa state | Verdict |
| --- | --- | --- | --- | --- |
| 1 | D1 cockpit chrome + transcript | `GUI/Viden - 桌面驾驶舱 (GUI).html` | `d1` | pass |
| 2 | D1 context dock tabs | same | `dock-environment` | pass |
| 3 | D2 Decision Center | `GUI/pages/…D2 决策中心…` | `d2` | pass after fix |
| 4 | D4 Lane creation | `GUI/pages/…D4 Lane创建流程…` | `d4` | pass |
| 5 | D4 agent binding (G9b) | same | `d4-agent-binding-started` | pass |
| 6 | D6 recovery states | `GUI/pages/…D6 空态与错误态…` | `d6-actions` | pass |
| 7 | D10 Lane monitor | `GUI/pages/…D10 Lane监视器…` | `d10-ticker` | pass after fix |
| 8 | D11 project intake | `GUI/pages/…D11 首启与项目接入…` | `d11` | pass after fix |
| 9 | D12 integration gate | `GUI/pages/…D12 集成闸冲突退回…` | `d12-actions` | pass after fix |
| 10 | D13 fleet and workflow | `GUI/pages/…D13 Fleet 编排与 Workflow…` | `d13` | pass after fix |
| 11 | D14 audit timeline | `GUI/pages/…D14 审计与时间线…` | `d14-audit` | pass after fix |
| 12 | D15 Plan Studio | `GUI/pages/…D15 Plan Studio…` | `plan-proposed` | pass |
| 13 | D16 Agent Board | `GUI/pages/…D16 Agent Board…` | `board-lanes` | pass after fix |
| 14 | Permission dock | D1 cockpit + component library | `permission-ask` | pass |
| 15 | Always / Edit dock (G9) | D1 cockpit | `dock-always-edit` | pass |
| 16 | DiffReview | D1 cockpit, `DESIGN-REF` "D1 次级视图" | `review` | pass |
| 17 | EvidenceView | same | `evidence` | pass |
| 18 | Command palette | D1 cockpit | `palette` | pass after fix |
| 19 | Focus mode | D1 cockpit | `centre-focus-mode` | pass |
| 20 | Welcome | D1 cockpit | `welcome-fill` | pass |
| 21 | Settings overlay | `GUI/gui-settings.jsx` | `settings` | pass after fix |
| 22 | Lane rail | D1 cockpit | `lane-rail` | pass |

## 2. Token mapping — the gate's headline finding

GUI-4 says token mapping must be *checkable*. Nothing checked it, and the
client had drifted badly. `apps/gui/tests/visual/token_registration.spec.ts`
now asserts two properties over every GUI-owned stylesheet, and both failed on
the base commit:

```
apps/gui/src/components/command_palette.css:34: box-shadow: var(--shadow-overlay, 0 2.5rem 6.25rem -1.875rem rgb(0 0 0 / 90%));
```

plus **36 `var()` references naming 13 custom properties that no `tokens.css`
registers and nothing else declares**.

This is not cosmetic. A `var(--not-a-token)` with no fallback is invalid at
computed-value time, so the *whole declaration* is dropped with no error. Two
before/after frames are committed under
`apps/gui/evidence/fidelity-gate/drift-before/`:

- **D2** — `Decision Center` rendered in 16px Times, and the `cargo test` tool
  input preview rendered in a **proportional serif** rather than the mono face,
  because `--lh-code` voided its `font:` shorthand. The command bar, the gate
  bar and the `<pre>` had no panel surface at all.
- **D14** — `Audit trail` and all seven filter chips in Times; the head, mode
  and filter bars flat against `--bg-base`; and the `denied 1` rollup segment
  drawn in ordinary primary ink because `--warn` is not a token.

### Drift fixed (each its own commit)

| Commit | Drift | Sites |
| --- | --- | --- |
| `0789eab6` | `test(gui)` — the check itself, **intentionally red**, listing all 36 offences | — |
| `d81d28d5` | six unregistered names that dropped the whole declaration | 25 |
| `63e5d0bb` | the command palette's raw-literal fallbacks, incl. the only raw color | 9 |
| `f152ac1f` | D14's two unregistered names, one of which hid a failure | 2 |
| `1f284ead` | the user-agent `ButtonFace` fill behind the kit's controls | 10 |
| `c564879c` | the missing `color-scheme`, and the UA `buttontext` half it exposed | global |

The mapping used, and why each replacement is the package's own registered role
rather than a guess:

| Unregistered | Registered | Basis |
| --- | --- | --- |
| `--bg-raised` | `--bg-panel` | every site is a header bar, card, code block or gate bar — a panel surface. The design pages use `--bg-panel` for these (3–5 uses each); `tokens.css` documents `--bg-elev` as the popover/modal step. |
| `--bg-selected` | `--bg-sel` | all three sites are `[aria-pressed="true"]` chips; `--bg-sel` is the registered selected-row surface. |
| `--fg-default` | `--fg-primary` | the package's default foreground. |
| `--fs-md` | `--fs-base` | D15's `.d15-title` already writes `font: var(--fs-base)/var(--lh-ui) var(--font-sans)` — the identical declaration with the registered token. |
| `--lh-code` | `--lh-body` | the registered ladder is `--lh-tight/ui/body`; a `<pre>` body is `--lh-body`. |
| `--sp-7` | `--sp-6` | see accepted deviations. |
| `--warn` | `--warning` | plain typo; the fallback was rendering a failure as an ordinary count. |
| `--fg-tertiary` | *(name dropped)* | the ladder has four steps and no tertiary; `--fg-faint` is documented as large-text/UI-stroke only at ≥3:1, which an 11px chip label is not. The chip keeps the `--fg-secondary` its fallback was already producing — no visual change. |
| `--radius-lg, 0.8125rem` | `--r-lg` | the registered card radius. |
| `--shadow-overlay, …rgb(0 0 0 / 90%)` | `--shadow-pop-gui` | the package's registered role is literally "GUI 下拉 / 菜单 / 弹层浮投影". |
| `--fs-2, 0.9375rem` | `--fs-base` | 15px, exact. |
| `--fs-1, 0.8125rem` | `--fs-sm` | 13px, exact. |
| `--fs-0, 0.65625rem` / `0.625rem` | `--fs-micro` | see accepted deviations. |

### Raw-color grep (required, must be empty)

```
$ grep -rnE "#[0-9a-fA-F]{3,8}\b|rgba?\(" apps/gui/src --include="*.css"
$ echo $?
1
```

Empty. The single hit on the base commit was the command palette's shadow
fallback, removed by `63e5d0bb`.

## 3. Component parity

Method: every selector block in `apps/gui/src/**/*.css` was parsed and its
class names matched against the 196 class names registered in
`docs/viden-design/Viden/GUI/gui-kit.css`.

- **Registered classes the GUI forked: none.** 56 GUI rules do name a
  registered class, but every one of them is scoped under a registered kit
  container the GUI legitimately extends — `.review`, `.evwrap`, `.diffbody`,
  `.cc`, `.ppch`, or `#app >`. That is the documented pattern: the kit is
  `@import`ed by nine GUI stylesheets rather than copied, and the extensions sit
  inside it. No GUI rule restyles a registered class at the bare, unscoped
  selector.
- **GUI class names with no kit registration: 585**, concentrated in the
  screen-local families (`d1-` 104, `d15-` 54, `gset` 39, `d2-` 35, `d10-` 33,
  `d14-` 33, `d12-` 24). These are GUI-owned screen scaffolding and are not
  drift on their own. The kit-promotion debt the README already records —
  the `.envp` family, `lane_tabs`, `tool_row` — is unchanged by this batch and
  remains the design package's to take.

## 4. Accepted deviations

| Deviation | Reason |
| --- | --- |
| D11 step badge `.stp .nn` is `--sp-6` (24px) where the D11 design page draws 26px | `tokens.css` defines `--sp-1…6, 8, 10, 12, 16` and skips 7, so no registered step lands on 26 or 28px. `--sp-6` is the nearest. Before the fix the declaration was dropped entirely and the badge collapsed to its text, so 24px is a large improvement over what shipped. Filed for the package. |
| Command palette radius is `--r-lg` (11px) where the raw literal was 13px | the point of a scale is that the client stops inventing steps between its rungs. |
| Command palette `--fs-micro` (11px) where the raw literals were 10.5px and 10px | the same name carried two different literals, which is the proof it was never a token. |
| `.d14-fchip` stays on `--fg-secondary` rather than moving to `--fg-faint` | the package reserves `--fg-faint` for large, non-body text at ≥3:1. An 11px chip label is body text. No visual change from what shipped. |
| G8's record rail sits inside the centre pane | pre-existing, recorded by G8; unchanged here. |

## 5. Drift filed, not fixed

These are design-package defects. This branch does not edit
`docs/viden-design/**`, so each is recorded here for the package to take.

| # | Finding | Evidence |
| --- | --- | --- |
| F1 | `gui-kit.css` styles `.tbtbtn` and `.gitchip` with size, radius, colour and a transparent border but **never a `background`**. The design pages draw them as non-interactive elements; a client that renders them as real `<button>`s — as it must, for keyboard access — gets the UA `ButtonFace` fill. Measured `rgb(107,107,107)` behind the three titlebar tool icons on every cockpit-bearing surface. Worked around GUI-side in `1f284ead`; the kit should declare the background. | `apps/gui/src/ui/theme.css`, commit `1f284ead` |
| F2 | **~~`--accent`~~ `--accent-bright` failed its own contrast contract in every light skin — fixed by DS2 at the token level; see §12.** The token the failing ink resolves from is `--accent-bright`, not `--accent`: `gui-kit.css` paints every `.on` state's ink with it (`.tbtbtn` :57, `.actbtn` :68, `.lseg .sg` :77, `.wslane .nm` :102, `.gperm-opt` :202/:204, `.evbar .fchip` :317), and all three light skins defined it *lighter* than `--accent` (`ice/light` `#5a8af0`: **2.57:1** on `--bg-sel`, **3.08:1** on `--bg-base`). DS2 darkened it in aurora/ice/mono light (`#076f85` / `#1558e7` / `#076d82`); the ink now reads **4.54:1 to 5.86:1** on all four surfaces and 17 failing surfaces became 3. | `accessibility.json`, re-measured; DS2 §12 |
| F3 | `--sp-7` is documented in `DESIGN-REF.md` as part of a `--sp-1…--sp-16` 4px scale but is not defined in `tokens.css`; the D11 design page hard-codes 26px for the step badge. Either register the step or move the design source onto `--sp-6`. | `d11_intake.css:49–50` |
| F4 | Raw spacing and type sizes remain in GUI-owned CSS outside this gate's scope — `.d14-fbar`/`.d14-fchip`/`.d14-rollup` carry `gap: 6px`, `padding: 8px 14px`, `font-size: 11px`/`10.5px`, `border-radius: 999px`, `margin: 0 4px`. GUI-4's "no temporary spacing" clause is not yet met. Not fixed here: it is a broad sweep, not parity drift this gate's frames expose. | `d14_audit_timeline.css:229–295` |
| F5 | 17 distinct interactive controls are under the package's ≥28px target rule in at least one dimension, in both pairs. The largest offenders are `.sb-gear` 15×11, `.d1-topbar-dirty` 15×12, `.d14-fchip` ×7 at 19px high, `.gset-close` 19×17, `.d1-mpill` ×3 at 24px, `.review-icon` 23×23, `.dtab` 26px, `.gitchip` 27px. Raising them changes the cockpit's density baseline, which is a design-package decision. | `accessibility.json` |
| F6 | `.review-stage` reads 3.48:1 in `aurora/dark` and 3.37:1 in `ice/light` at 11px, against 4.5:1. The only dark-pair contrast failure in the batch. | `accessibility.json` |

| F7 | **`--accent` itself reads 4.16:1 as 11–13px ink on `--bg-sel`** in `ice/light` (4.05:1 aurora/light, 4.07:1 mono/light), against the same ≥4.5 guardrail. It clears every other surface (4.64:1 panel, 5.00:1 base, 5.38:1 elev), so only selected rows are affected: `.gbtn` on the D4 wizard's current step and DiffReview's `.on` view segment — the two remaining light-pair contrast failures after DS2. DS2 deliberately did not touch `--accent`: the adjudicated scope was `--accent-bright`, and moving the brand accent is a separate design-package decision. | `accessibility.json`, 2 surfaces |

## 6. Accessibility record

Machine-readable: `apps/gui/evidence/fidelity-gate/accessibility.json` — 22
surfaces × 2 pairs, with roles, accessible-name resolution, target sizes and
WCAG 2.1 contrast ratios per element.

| Property | Result |
| --- | --- |
| Accessible name on every focusable element | **pass** — zero missing across 22 surfaces × 2 pairs |
| Every tabbable element accepts focus | **pass** — 22 surfaces × 2 pairs |
| Contrast, `aurora/dark` | **pass except F6** (`.review-stage` 3.48:1) — unchanged by DS2, which did not touch the dark skins |
| Contrast, `ice/light` | **was fail — F2** (`--accent-bright`, 1–5 failing pairs on 17 of 22 surfaces). After DS2: **3 surfaces**, all of them F7 (`--accent` on `--bg-sel`, 4.16:1) or F6 |
| Target size ≥ 28px | **fail — F5**, 17 distinct controls |
| `color-scheme` follows the skin | **pass after `c564879c`** — reported `dark` under `ice/light` before the fix |

Method and its limits are recorded in the artifact. Two caveats worth naming
here: the contrast walk resolves the nearest opaque ancestor background and
does not model layered translucency; and no platform accessibility-tree audit
(NSAccessibility / AXAPI) ran, so these are the DOM's roles and names, not what
VoiceOver would announce.

## 7. Keyboard-only traversal

Headless, over 17 surfaces. Real key-driven traversal in the native window is a
native leg and did not run.

| Check | Result |
| --- | --- |
| Focus order | the rail-first order the design implies: git chip → titlebar tools → activity rail → centre → dock → statusbar. 43–72 tabbables per cockpit surface. |
| Dead ends | **none found.** Every tabbable element accepted focus on every surface. |
| Pointer-only controls | none. Two candidates were checked and cleared: the disabled dock tabs (`Terminal`, `Code`, `Diff`, `Docs`) are disabled-and-labelled, which is the honesty doctrine working, not a dead end; and `div.palrow` is correctly non-tabbable because the palette is a real combobox. |
| Palette semantics | `input[role=combobox]` + `aria-controls` → `ul[role=listbox]` with `li[role=option]` and `aria-selected`; `ArrowDown` moved `aria-activedescendant` from `gpal-row-0` to `gpal-row-1`. |
| Esc chains | Esc closed the palette, the Settings overlay, DiffReview, EvidenceView and the project picker — five for five. |

## 8. Performance

Machine-readable: `apps/gui/evidence/fidelity-gate/performance.json`. Three runs
each, median reported. Bounds are this gate's own, stated before the runs; Core
publishes no event-to-visible budget for the GUI, so no bound is quoted from a
contract.

| Record | Bound | Median | Result |
| --- | --- | --- | --- |
| Cold start to bound — D1 | 400 ms | 40.1 ms | pass |
| Cold start to bound — D14 page load | 400 ms | 41.3 ms | pass |
| Cold start to bound — DiffReview | 400 ms | 47.1 ms | pass |
| Cold start to bound — D16 | 400 ms | 38.5 ms | pass |
| Cold start to bound — D15 | 400 ms | 39.0 ms | pass |
| Dock file read (Files tab bound) | 400 ms | 49.1 ms | pass |
| Route switch, rail Conversation → Lane monitor | 150 ms | 8.9 ms | pass |
| Dock tab switch, Environment → Files | 150 ms | 6.4 ms | pass |
| Transcript page, 500 rows (2,500 DOM nodes) | 250 ms | 19.8 ms | pass |
| Diff render, 5,000-line patch (15,004 DOM nodes) | 500 ms | 98.9 ms | pass |

**These are dev-server numbers, not release-bundle numbers.** The brief asks for
the release bundle; the screen was locked, so the native application was never
launched. The figures bound the client's own render cost and nothing more. The
diff record is the one to watch: DiffReview does not virtualize, so the 5k-line
render is linear in the byte bound.

## 9. Legs not run — screen locked

| Leg | Status |
| --- | --- |
| CJK IME (Pinyin into the composer, Lane task field, reject reason and the typed Edit field; composition, candidate commit, no chord mid-composition; capture twice; restore the input source) | **not run — screen locked at `time=2026-09-13T11:57:07Z CGSSessionScreenIsLocked=1 onConsole=1`.** No keystroke was sent and the input source was never changed, so there is nothing to restore. |
| Keyboard-only traversal in the native window | **not run — screen locked at the same line.** The headless traversal in §7 is the delivered substitute and is labelled as such. |
| Release-bundle performance (`tauri build --bundles app`, three runs, median) | **not run — screen locked at the same line.** No bundle was built and no native timing was taken. |
| Native capture of any surface | **not run — screen locked at the same line.** Every frame in `evidence/fidelity-gate/` is a headless Chrome capture of the production render functions. |

## 10. Checks

```
npm --prefix apps/gui test -- --run          Test Files 64 passed (64) · Tests 943 passed (943)   VITEST_EXIT=0
npm --prefix apps/gui run build              tsc --noEmit && vite build, 93 modules              BUILD_EXIT=0
cargo test -p viden-gui                      all suites ok, 0 failed                             CARGO_EXIT=0
cargo test -p viden-gui --test capture_projections -- --ignored   1 passed                       CAPPROJ_EXIT=0
grep -rnE "#[0-9a-fA-F]{3,8}\b|rgba?\(" apps/gui/src --include="*.css"   (empty)                 GREP_EXIT=1
git diff --check                             (empty)                                             DIFFCHECK_EXIT=0
scripts/check-doc-pairs.sh / check-doc-links.sh   see the batch report
PNG floor                                    44 frames, smallest 39,240 B, none under 30 KB
```

## 11. Verdict

The headless gate **passes**. Every surface matches its design source in both
shipped pairs after five drift fixes, no registered component is forked, the
raw-color grep is empty, every token reference now resolves, no keyboard dead
end was found, and all ten performance records are inside their bounds.

Two things kept this from being the whole gate. Four legs need an unlocked
screen and are recorded above as not run. And **F2 was a genuine release
risk** — the `.on`-state ink failed WCAG AA everywhere in the light skins,
against the package's own written contract. It was filed rather than fixed
because the fix belongs in `tokens.css`, which this branch may not touch;
**DS2 has since made it there** (§12), and the re-measured record now shows
three failing light surfaces instead of seventeen. What remains is F6 and the
newly separated F7, both still filed for the design package.

## 12. DS2 — F2 re-measured after the token fix

Batch DS2 (branch `claude/design-ds2`, base `47915901` = this gate merged onto
`claude/int-0.3.5`) fixed F2 where it belongs: in `docs/viden-design/Viden/tokens.css`.
No GUI source changed, `--accent` was not touched, and the dark skins were not
touched.

**What was wrong.** `--accent-bright` is *ink* in the kit — six `.on`-state
rules paint text with it, and the cockpit page repeats that about 37 times — but
all three light skins defined it as a *lighter* variant of `--accent`, which is
the wrong direction on a light ground. G10 attributed the failure to `--accent`;
the resolved value it measured (`#5a8af0`) is `--accent-bright`.

**The rule now written into the guardrail.** `--accent-bright` is ink, so it
meets the same ≥4.5:1 as `--accent`; in dark modes "brighter than accent"
achieves that, in light modes it must be *darker* than `--accent` and clear
≥4.5:1 on all four surfaces an `.on` state can sit on (`--bg-base`,
`--bg-panel`, `--bg-sel`, `--bg-elev`; the `.on` states sit on `--bg-sel`).
The values were picked by searching the token's own hue and saturation for the
*lightest* value meeting that, with `--on-accent` still ≥4.5:1 against it —
the kit does fill with `--accent-bright` on primary-button hover.

| skin/light | token | value | `--bg-base` | `--bg-panel` | `--bg-sel` | `--bg-elev` | `--on-accent` on it |
| --- | --- | --- | --- | --- | --- | --- | --- |
| aurora | `--accent` (unchanged) | `#0b7791` | 4.85:1 | 4.54:1 | 4.05:1 | 5.17:1 | 4.84:1 |
| aurora | `--accent-bright` old | `#0aa6c6` | 2.71:1 | 2.53:1 | 2.26:1 | 2.88:1 | 2.70:1 |
| aurora | `--accent-bright` **new** | `#076f85` | **5.44:1** | **5.09:1** | **4.54:1** | **5.80:1** | **5.43:1** |
| ice | `--accent` (unchanged) | `#2d65d2` | 5.00:1 | 4.64:1 | 4.16:1 | 5.38:1 | 5.00:1 |
| ice | `--accent-bright` old | `#5a8af0` | 3.08:1 | 2.86:1 | 2.57:1 | 3.32:1 | 3.08:1 |
| ice | `--accent-bright` **new** | `#1558e7` | **5.45:1** | **5.05:1** | **4.54:1** | **5.86:1** | **5.45:1** |
| mono | `--accent` (unchanged) | `#0b748d` | 5.02:1 | 4.60:1 | 4.07:1 | 5.39:1 | 5.02:1 |
| mono | `--accent-bright` old | `#0a96b4` | 3.24:1 | 2.97:1 | 2.63:1 | 3.48:1 | 3.24:1 |
| mono | `--accent-bright` **new** | `#076d82` | **5.56:1** | **5.09:1** | **4.50:1** | **5.97:1** | **5.56:1** |

Hue and saturation are held: aurora 190.2°→190.5° at 90%, ice 220.8°→220.9° at
83%, mono 190.6°→190.2° at 90%. Only lightness moves (41%→28%, 65%→49%,
37%→27%). The visible consequence is that in the light skins `--accent-bright`
now sits *close to* `--accent` rather than above it — that is what the ≥4.5:1
guardrail costs on a light ground, and it is recorded in the package's
`CHANGELOG`.

**Re-measured.** G10's probe was re-run over the same 22 surfaces × 2 pairs.
On `aurora/dark` every re-measured field is identical to what G10 filed,
`contrast_failures` included, which is the check that the re-run is faithful.
On `ice/light`, **17 surfaces with failures became 3**: every `2.57:1` and
`3.08:1` entry (the `--accent-bright` ink) is gone, leaving `.gbtn` on `d4`
and `d4-agent-binding-started`, and `.review-stage` + `.on` on `review`. F5
and F6 stand as filed; `target_under_28px` was not rewritten, because the
re-run renders each surface in the top-level 1440×1180 viewport rather than
G10's nested iframe and the ~15px the iframe's scrollbar took off the layout
width shifts a few of those measurements by 1–11px.

**Frames.** 16 of the 22 `*-1440x1180-light-zh-CN.png` frames changed and were
re-captured and viewed; the other 6 came back byte-identical, which is the
second faithfulness check on the capture path. The dark set is untouched. The
design package's own two light previews under
`docs/viden-design/Viden/docs/previews/design/0.3.5/` were re-captured as well.

**Token digest.** `tokens.css` moved from
`826826ee6ddab845897472701add67ee9f55aff25af539651e6089553b7e6398` to
`2d80baa92bc7b83a188257a08dec27cb1fc143fc27237a02dfdbb61eee8b941a`; every live
pin was regenerated (`apps/gui/src/ui/tokens.ts`, `apps/gui/release-manifest.toml`,
its `manifests/0.1.0-rc.5.toml` snapshot, `apps/tui/release-manifest.toml` and
the literal its manifest test asserts). The historical rc.2–rc.4 manifests and
the rc.3 evidence README keep the digest they shipped with.
