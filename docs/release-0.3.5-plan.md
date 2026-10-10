# Viden 0.3.5 Plan - Trusted Delivery Hardening, Plan Studio, Agent Board, And The Production Release Gate

Chinese version: [release-0.3.5-plan.zh-CN.md](release-0.3.5-plan.zh-CN.md)

`0.3.5` is the aggregate workspace milestone that follows `0.3.4`, which
merged to `main` at `edcc7b3d` on 2026-09-13. `PLAN.md` and
`docs/parallel-development-plan.md` name it "visual fidelity and production
release gate" and, since 2026-09-12, also the home of Plan Studio and Agent
Board. This document keeps that scope and orders it: the trust gaps the
`0.3.4` release evidence found are closed first, the two designed-but-
unregistered screens are brought into the design package before any client
draws them, and the production gate runs last on a candidate that has
already been proven natively. The Core contract content will live in
`release-0.3.5-contract-design.md`, written after this plan is accepted.

Status: plan written 2026-09-13 from the `0.3.4` completion report, the
compatibility follow-ups it left open, and the register. Nothing in this
document is implemented. Every batch is dispatched separately, reviewed
adversarially in the main session, and merged only on an explicit push.

## Baseline

`main` at `edcc7b3d` on 2026-09-13:

| Line | Version | State |
| --- | --- | --- |
| Core | `0.3.7` immutable checkpoint `39ed155d` | 29 extension capabilities, schema `1`; nine frozen base fixtures unchanged since `0.3.0` |
| TUI | `0.3.5` | parity for every `0.3.4` capability; the whole real task ran end to end here, including a commit and an accepted push with no Lane selected |
| GUI | `0.1.0-rc.5` | cockpit level with the D1 design: rail-as-router, in-cockpit D2/D10/D12/D13/D14, Lane tab strip, tabbed context dock, consumers of all six `0.3.4` capabilities; macOS `.app` builds |

What `0.3.4` left open, verbatim from its report:

- the native GUI capture set does not exist — the host screen was locked for
  the whole release step, so goal 6 is met on the contract and on the TUI and
  unproven on the GUI cockpit;
- compatibility follow-ups 10, 13, 14, 15, 16 and the TUI's missing
  `set_upstream` control;
- the open register: 013, 018, 019, 021, 023, 026, 029 (reserved for the D5
  gallery review), 030, 031, 032, 033;
- the production release gate itself: `scripts/release-gate.sh --phase
  prepublish` refuses without `DEEPSEEK_API_KEY`, and no packaging,
  signing, notarization, GitHub Release, or Homebrew step has run for any
  `0.3.x` line.

## Goals

1. **Prove `0.3.4` natively, first.** One unlocked-screen run of the real
   task through the GUI cockpit against `main` at `edcc7b3d`, recorded as the
   missing E2 section. No `0.3.5` client change lands before this evidence
   exists, so the proof is attributable to what shipped.
2. **Close the trust gaps.** An approval decided on the Lane path leaves a
   durable audit row (follow-up 15, blocking); a session follow-up is
   acknowledged into Core's queue when the command is accepted, not when the
   worker reaches it (16); an archived patch's document names its file and
   change kind (14); an Agent-reported patch is canonicalized durably (10);
   the `interaction-closed-loop` bytes and digest are refreshed (13); the TUI
   offers the `set_upstream` retry Core's own refusal names.
3. **Register Plan Studio and Agent Board in the design package, then build
   them in the cockpit.** Neither has a registered D-screen; the functional
   design (`docs/gui-version-functional-design.md` sections 3 and 4) is the
   only source. The design package gains both screens under its own rules
   first; the GUI then hosts them as centre-pane views with the chrome
   retained, exactly as D2/D10/D12/D13/D14 were hosted in `0.3.4`.
4. **Give the two screens the Core facts they need**, as additive
   capabilities: plan documents with a Plan → Build handoff, and agent
   session control (pause, resume, kill) beside the existing cancel. Two
   register entries that unblock D4's honesty close on the way: 030 (agent
   binding on the starter-Lane request) and 019 (Always and Edit approval
   decisions).
5. **Pass the design-fidelity gate**: screenshot and component parity with
   the design package, CJK, keyboard-only, accessibility, and bounded
   performance records, on both themes, for every cockpit surface.
6. **Run the production release gate** as one release unit: three-platform
   packaging, signing and notarization where credentials exist, the
   live-provider certification, a GitHub Release, and a Homebrew tap
   validated at the same version. Every externally visible step is
   user-authorized at the moment it runs, never assumed from this plan.

## Scope Decisions

- **The native proof runs against `main`, before any `0.3.5` change.** If
  the screen stays locked, the milestone proceeds without it and the proof
  moves to the candidate; the report says which SHA it was taken on.
- **Design before client.** Plan Studio and Agent Board are drawn in
  `docs/viden-design/Viden/` first (zh-primary, package guards, status and
  changelog per its `AGENTS.md`). A GUI batch that finds the design missing
  stops and reports; it does not invent a screen.
- **Agent Board shows the Lane/session hierarchy only.** Sub-agent trees
  stay deferred (`D-RAILNAV` ④). Pause and kill are new Core commands, not
  aliases of cancel; a control Core does not publish renders disabled and
  labelled, as in `0.3.4`.
- **Plan Studio is Core-fact-driven.** Plan mode already denies mutations
  and publishes plan denials; the new capability adds the plan document
  (steps, status, the accepted handoff) as facts. The client never keeps a
  plan Core does not know.
- **The release gate is gated by the user, step by step.** Building bundles
  and running offline legs need no authorization. Live-provider
  certification (`DEEPSEEK_API_KEY`), code signing, notarization, dispatching
  `release.yml`, publishing a GitHub Release, and touching the Homebrew tap
  each need an explicit "go" for that step, and the batch stops and reports
  where authorization is missing.
- **GUI version**: the candidate is `0.1.0-rc.6`. It is promoted to `0.1.0`
  only if the production gate passes in full; otherwise it ships as `rc.6`
  and the gate's failing rows are the `0.3.6` backlog.
- **Out for `0.3.5`**, adjudicated in the hygiene batch and re-dated rather
  than silently carried: 013 (pending contract-confirmation fact), 018
  (checkpoint capture and restore), 021 (forge
  and PR status), 023 (multi-workspace supervision), 026 (platform
  credential intake), 029 (D5 gallery), 031 (skill packs), 032 (terminal
  PTY facts, DockSD roadmap), 033 (workspace document facts); DockSD,
  Diagnostics, D7/D8/D9, pop-out windows, and Pip remain roadmap or deferred
  per the design package.

## Version Targets

| Line | From | To |
| --- | --- | --- |
| Core | `0.3.7` (29 capabilities) | `0.3.8`, four additive capabilities (target 33), immutable checkpoint declared once in the release step |
| TUI | `0.3.5` | `0.3.6`, parity minimum for the new facts plus the `set_upstream` control |
| GUI | `0.1.0-rc.5` | `0.1.0-rc.6` candidate; `0.1.0` if the production gate passes in full |

## Batches

Ownership follows `0.3.4`: Core batches serialize on `protocol.rs`,
`runtime.rs`, the manifests, and the count constants; GUI batches serialize
on `d1_cockpit.ts` and `main.ts`; the design batch owns
`docs/viden-design/**` alone. Briefs live in the dispatcher's scratchpad;
every batch returns a raw report with verbatim check tails.

| Batch | Owner | Delivers | Depends on |
| --- | --- | --- | --- |
| E3a native proof of `0.3.4` | evidence | the real task through the native GUI window against `main` `edcc7b3d`, every step captured twice and read from the Accessibility tree, appended to the `0.3.4` checkpoints document as the E2 native section; stops honestly on a locked screen | an unlocked host screen |
| CD2 contract design | main session | `release-0.3.5-contract-design{,.zh-CN}.md`: `runtime.plan_documents`, `runtime.agent_session_control`, `runtime.approval_scopes` (019), `runtime.starter_lane_agent` (030), with types, semantics, fixtures, client consumers, bookkeeping, decided defaults, and the hardening items' exact rules | this plan accepted |
| C12 trusted-delivery hardening | Core | follow-ups 15, 16, 14, 10, 13 closed with red-first tests; no new capability; the `0.3.4` fixtures and the nine frozen base fixtures byte-unchanged | CD2 |
| T3 TUI hardening | TUI | `set_upstream` retry control in the `/git` picker; composer queue copy reads the acknowledged queue (16); regression baselines with causes | C12 |
| DS design package | design | Plan Studio and Agent Board registered as D-screens in `docs/viden-design/Viden/` (pages, DESIGN-REF registrations, SPEC decisions, status and changelog, package guards green); every control maps to a Core fact or is marked as a request | CD2 |
| C13 `runtime.plan_documents` | Core | plan document facts (steps, status, accepted handoff), Plan → Build handoff command, fixture; closes the Plan View's Core side | C12 |
| C14 `runtime.agent_session_control` | Core | pause, resume, kill for Agent sessions with the terminal facts they publish; fixture; the G6 drafts adjudicated | C13 |
| C15 `runtime.approval_scopes` + `runtime.starter_lane_agent` | Core | Always and Edit approval decisions (019); `agent_id` on `StarterLaneRequest` echoed on preview and creation (030); fixtures | C14 |
| G8 Plan Studio | GUI | in-cockpit Plan View with the Plan → Build handoff, on C13 facts; qa states, PNGs both themes, EVIDENCE | DS, C13 |
| G9 Agent Board | GUI | in-cockpit Agent Board on the Lane/session hierarchy with pause/resume/kill/switch-model controls on C14 facts; D4 step 2 sends the agent binding (C15); Always/Edit in the permission dock (C15) | DS, C14, C15, G8 |
| T4 TUI parity | TUI | plan document lens, session control keys, Always/Edit decisions, agent binding on `/lanes`; regression baselines with causes | C13 to C15 |
| G10 design-fidelity gate | GUI | screenshot and component parity against the design package for every cockpit surface, CJK IME, keyboard-only traversal, machine-readable accessibility, bounded performance records, both themes; the fidelity report | G8, G9 |
| H4 GUI native defects | GUI | defects 14–21 from the E3a native run closed with red-first tests: Lane creation no longer gated on ACP discovery for the built-in agent, probes bounded and labelled, the starter-Lane preview not queued behind a probe, the Decision Center surfacing the next approval after a decision, source-control in-flight state cleared on completion, the silent self-termination diagnosed and fixed, the three layout/state nits; a native re-run of the blocked steps as proof | E3a |
| E3b native proof on the candidate | evidence | the full real task, Lane half included, through the native GUI on the release candidate after H4, G8, G9; appended to the evidence document; feeds R1 | H4, G10 |
| H3 hygiene | Core, GUI | register adjudication (018, 021, 023, 026, 029, 031, 032, 033 re-dated to `0.3.6` or closed with reasons); the `viden-plugin-host` and `viden-agents` timing tests made deterministic so the serial-rerun rule can be retired; clippy's pre-existing warning set cleared | none |
| P1 packaging | release | reproducible bundles for macOS (arm64, x86_64), Windows, and Linux through `release.yml` dispatched with `upload_to_release=false`; checksums; signing and notarization only where the user supplies credentials; the bundle matrix recorded | G10 |
| P2 GUI release workflow | release | `release.yml` gains GUI bundle jobs (macOS arm64 and x86_64 `.app`/`.dmg`, Windows `.msi`/NSIS, Linux `.deb`/`.AppImage`) with checksums and artifact upload, validated by YAML parse and a dry job review; P1 found the workflow builds the CLI only, so the GUI half of the three-platform gate had no CI path at all; dispatch stays user-authorized | P1 |
| R1 production release gate | release | `scripts/release-gate.sh --phase prepublish` with the live-provider leg (user-supplied key), migration and full-workspace legs, GitHub Release and Homebrew tap validation at one version, `--phase postpublish`; Core `0.3.8` checkpoint and the three version bumps; the `0.3.5` completion report | everything above, and a "go" per external step |

## Gates

Unchanged from `0.3.4` unless stated:

- every Core batch: crate suites, the full workspace suite with no crate
  excluded and no `--test-threads` override — H3 retired the
  `viden-plugin-host` / `viden-agents` serial-rerun rule on 2026-09-13 —
  `cargo fmt`, `cargo
  clippy` with zero new warnings, dependency boundaries, `tui-regression`,
  doc pairs and links, `git diff --check`; the nine frozen base fixtures and
  every `0.3.4` extension fixture byte-identical (`shasum` against
  `git show edcc7b3d:<path>`); count constants moved only by what a batch
  adds, with tracking comments;
- every GUI batch: vitest, `tsc` build, `cargo test -p viden-gui`,
  `capture_projections`, PNGs viewed by the author and by the reviewer, no
  frame under 30 KB (the error-page signature), one i18n tail block per
  catalog, honesty doctrine (absence is not zero, grouping never hides,
  disabled is labelled), no local persistence of Core-owned state;
- the design batch: the package's own guards, status and changelog rules,
  zh-primary, no `.ref/` edits;
- E3a and R1: `CGSSessionScreenIsLocked` checked before every keystroke
  batch; the repository's live `.viden/` never touched; no live provider,
  publish, release, signing, or Homebrew step without an explicit "go".

## Exit Criteria

| # | Criterion |
| --- | --- |
| 1 | The `0.3.4` real task is evidenced natively through the GUI cockpit, or the report states the screen stayed locked and names the candidate SHA the proof was taken on instead. Amended 2026-09-13 after E3a: the run reached the workspace half (bind, stage, commit, push, audit, file read) and was blocked on the Lane half by GUI defects 14–16; H4 closes them and E3b takes the full proof on the candidate. Amended again 2026-09-13 after E3b, on `claude/int-0.3.5` at `da4c8a6d`: Lane creation under a Core approval, its durable Lane-path audit row, a Lane-scoped stage and commit, Plan Studio and the Agent Board are evidenced natively; the Lane's **turn** is not, because a starter Lane never settles its creation command and the cockpit's command slot stays occupied (defect 22, Core-side). The criterion is met as to the candidate being driven natively and unmet as to the full task; closing defect 22 and re-running the Lane half is owed before R1 can call goal 1 done. Amended a third time 2026-09-14 after E3c, on `claude/int-0.3.5` at `22124903`: defect 22 is **closed and confirmed natively, twice** — the popover's TASK becomes a turn on the fallback provider — and everything that turn gates is evidenced with it: the tool approval with Core's typed hunks, allow-once, the inline tool diff, the archived patch with a correct path label (follow-up 14), D14 showing `approval.allow_once` for the Lane path and the runtime path side by side (follow-up 15), C15's `Always` with its standing rule and its revoke, and C16's typed `Edit` with the `revised_from` link. Defects 23 and 24 are closed natively; defect 26 does not reproduce. **The criterion remains unmet as to the full task**, now for three different reasons: a tool called in a Lane-owned turn edits the workspace root rather than the Lane worktree (defect 28, Core-side, numbered and stopped), the operator-git owner does not follow the Lane selection so no commit or push was reachable with a Lane selected (defect 30, which locates defect 25's client-side cause), and a follow-up cannot be queued during a turn (defect 29, so follow-up 16 is unmet on the GUI). One blocker found in E3c was a GUI client defect and was fixed in that batch red-first (defect 27, `fix(gui): settle a composer control against the owner Core stamps`). Closing defects 28 and 30 and re-running the Lane's source-control half is owed before R1 can call goal 1 done. Amended a fourth time 2026-09-14 after E3d, on `claude/int-0.3.5` at `71d5156a`: **defect 28 is closed and confirmed natively in both directions** — a tool called in a Lane-owned turn edits the Lane worktree, the workspace root is untouched, and the product's own `tool shell command=pwd` in that Lane prints the worktree — and **defect 34 is closed**, because C17's hydration re-binds every restored non-terminal Lane, so a relaunched Lane has an exact owner, accepts a composer submit, runs a turn under the new session, and has an enabled commit bar. **Defect 30 is closed**: with a Lane selected the commit bar is reachable, carries no `D1-OPERATOR-GIT-OWNER` refusal, and names only the selected Lane. **Defect 33 is closed** (a centre view opens over a stopped Lane and the recovery surface stays reachable in the conversation slot) and **defect 32 is closed with real keystrokes** (`A"b"c` typed as HID key events on an ABC source reads back `41 22 62 22 63`; both quotes are `U+0022`). **The criterion remains unmet as to the full task**, now for one blocking reason instead of three: a Lane-scoped `Stage all` never reaches a decidable surface — the client stands on `Stage is waiting for approval in the permission dock.` while the dock holds `0 awaiting you`, no `git_*` audit row is ever written, and the status bar reads `REQ 0 req / 0 err` (defect 35, owning side not decidable from that evidence, numbered with file:line on both candidate sites and stopped). So `Commit & Push ⇡` never enabled, the `git_push` half was never sent, no `NoUpstream` refusal was seen, the bare `origin` was never used, and **defect 25 stays open behind an earlier wall**. Follow-up 16 is still unmet and now fails silently (defect 36): the composer accepts a follow-up during a turn with no refusal, clears the draft, publishes no `InputQueued`, and the text never reaches Core. E3d also recorded a Core connection that drops mid-session and never recovers (defect 37) and a Lane created beside a stopped Lane that gets no owner (defect 38). No blocker fix was taken in E3d, because the evidence does not localize defect 35 to the client. Defect 31's `Always`/revoke leg and the Plan Studio leg were not driven and remain owed. Closing defect 35 and driving the commit, the `git_push` half and the push is owed before R1 can call goal 1 done Amended a fifth and final time 2026-10-10 after E3e, on `claude/int-0.3.5` at `3c652fa8` (after H7, C18 and H8): **the criterion is met, natively, as to the full task**. A Lane created under Core's approval took its TASK as a turn; the product's `edit_file` changed the Lane worktree and nothing else; a follow-up submitted while that turn waited on its approval was acknowledged by Core and drained into the Lane's next turn (compatibility follow-up 16 met on the GUI); and the Lane's change was staged, committed and pushed with `git_add`, `git_commit` and `git_push` each decided on the dock, Core's typed `no_upstream` answered by the GUI's own `Push and set upstream`, the branch landing in the bare `origin`, and four `source.push` rows in `audit.jsonl` — **defect 25 is closed natively**, as are 35 and 36 behind it. Defect 31's `Always` / revoke leg is closed natively (the revoked row leaves the list and the identical input asks again), and the Plan Studio leg reached Accept, a `plan_mode`-held Hand off, Build, Hand off and D13's DAG. Three qualifications are part of the claim: the bind is taken at launch through `VIDEN_GUI_WORKSPACE`, not through `⌘O`; the push completed on a second `Commit & Push ⇡` pair in a relaunched process, the first pair's pending push having been lost when the `Lanes` rail fell into a pre-hydration `CONNECTING` shell (defect 43); and E3e's bundle carries two GUI blocker fixes taken red-first in that batch — defect 38 (`5afb01b0`, a Lane selection never re-read the cockpit on a quiet Core) and defect 41 (`4c92b511`, Plan Studio's and the Agent Board's intents were refused by the host's field names). Still open on the candidate and handed to the next batches rather than to this criterion: GUI defects 39 (a turn parked on an approval cannot be cancelled), 40 (the dock keeps the previous Lane's sample after a selection), 43, 44 (Plan Studio's decision row is occluded at 1280×800), 45 and 46, and Core defect 42 (a proposed plan does not survive a Core restart). The Agent Board flows stay unevidenced on a host with no usable ACP binary |
| 2 | Compatibility follow-ups 10, 13, 14, 15, 16 are closed with dates and file:line; the Lane-path approval writes its audit row before the fact that announces it |
| 3 | Plan Studio and Agent Board exist in the design package as registered D-screens and in the cockpit as centre-pane views with the chrome retained, on Core facts, with every unbacked control disabled and labelled |
| 4 | Core `0.3.8` declares 33 capabilities with fixtures; the nine frozen base fixtures and every `0.3.4` fixture are byte-identical to `edcc7b3d` |
| 5 | Register 019 and 030 close on both sides; 013, 018, 021, 023, 026, 029, 031, 032, 033 are closed or re-dated with reasons |
| 6 | The design-fidelity report passes for every cockpit surface on both themes, with CJK, keyboard-only, accessibility, and performance records attached |
| 7 | Bundles exist for the three platforms with checksums; each external release step is recorded as done under authorization, or as not run with the missing authorization named |
| 8 | The `0.3.5` completion report exists in both languages, states what is pushed and what is not, and names the GUI version the gate earned |

## Risks

- **The screen lock is environmental and recurring.** E3a is scheduled
  first and re-attempted opportunistically; the milestone does not block on
  it, and the report never claims a native proof that was not taken.
- **Plan Studio and Agent Board are new surfaces, not alignments.** The
  design batch may find the functional design under-specified; it records
  decisions in the package's SPEC rather than in a client brief, and the
  GUI batches follow the package.
- **Pause and kill change Agent-session semantics.** What a paused ACP
  session does with in-flight tool calls and queued input is a Core
  decision CD2 must settle before C14; a wrong default corrupts live
  sessions.
- **The release gate needs credentials this repository must never hold.**
  The batch reads them from the environment at run time only, records
  nothing secret, and stops where a key is absent.
- **Three-platform packaging has never run.** Windows and Linux bundles may
  fail on toolchain or Tauri configuration; P1 records the matrix honestly
  and R1 ships what built.
- **Timing-test determinism** (H3) touches test infrastructure two crates
  depend on; it runs alone and is gated by the full workspace suite three
  times in a row.

## Next Action

Accept or amend this plan; then the main session writes
`release-0.3.5-contract-design{,.zh-CN}.md` (CD2) and dispatches E3a the
moment the host screen is unlocked, C12 and H3 as soon as CD2 lands.
