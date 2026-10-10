# Viden 0.3.5 Completion Report

Chinese version: [release-0.3.5-report.zh-CN.md](release-0.3.5-report.zh-CN.md)

Date: 2026-10-10. Written by the production release gate (batch R1) of
[release-0.3.5-plan.md](release-0.3.5-plan.md).

**Nothing in this report is pushed, and nothing is released.** The `0.3.5`
candidate is the local integration branch `claude/int-0.3.5` plus R1's three
release-step commits on `claude/release-gate-r1`. `main` was fast-forwarded to
`0dcfaa66` on 2026-10-10 as a mid-plan snapshot with no version bump; the
candidate goes to `main` only on the user's explicit "push". No external
release step was authorized for R1, so none ran: no live-provider
certification, no signing or notarization, no workflow dispatch, no tag, no
GitHub Release, no Homebrew change.

## The candidate

| Line | Version | Where |
| --- | --- | --- |
| Workspace candidate | `0.3.5` | `claude/int-0.3.5` at `d522e4f38f7643d0e1dd37d70af5e84c43f59765` (every batch through C19 and E3e), plus R1's commits on `claude/release-gate-r1`, awaiting push |
| Core | `0.3.8` immutable contract checkpoint | checkpoint SHA `d522e4f38f7643d0e1dd37d70af5e84c43f59765`; schema `1`; 15 frozen base + **33** extension capabilities; base contract checkpoint `5bd2b80b…` unchanged since `0.3.0` |
| TUI | `0.3.6` | `min_core_version` deliberately unchanged at `0.3.4`: the four new capabilities are independently negotiated feature gates, so none blocks a base schema-1 client from starting |
| GUI | `0.1.0-rc.6` | `[core].minimum_version` unchanged at `0.3.5` for the same reason; macOS arm64 `.app` rebuilt at this version. **Not promoted to `0.1.0`**: the plan promotes only if the production gate passes in full, and it did not (exit criteria 6 and 7, and every external step) |
| Workspace / CLI | `0.1.30` | unchanged; a `0.3.5` GitHub Release needs a version decision first (see "Release identity") |

The checkpoint is declared once, here, against the integration tip every batch
landed on, and it names a commit on a local branch: **if `claude/int-0.3.5` is
rebased before it merges, the checkpoint must be re-declared** rather than
assumed to have survived. The declaration is in
[core-0.3-compatibility.md](core-0.3-compatibility.md), "Core 0.3.8 Contract
Checkpoint".

## What shipped

Four additive Core capabilities, taking the advertised extension set from 29
to 33 with the nine frozen base fixtures byte-unchanged:

| Batch | Capability | Clients |
| --- | --- | --- |
| C13 | `runtime.plan_documents` — plan documents as Core facts, an operator decision, and a Plan → Build handoff into the DAG Core already owns | GUI G8 (Plan Studio, D15), TUI T4 |
| C14 | `runtime.agent_session_control` — pause, resume, and kill beside cancel | GUI G9 (Agent Board, D16), TUI T4 |
| C15 | `runtime.approval_scopes` — `Always` and `Edit` decisions with a revocable standing rule (register 019) | GUI G9, TUI T4 |
| C15 | `runtime.starter_lane_agent` — an agent binding on the starter-Lane request (register 030) | GUI G9b, TUI T4 |

Around them: C12 closed compatibility follow-ups 10, 13, 14, 15 and 16's
session half; C16, C17, C18 and C19 closed follow-ups 19, 20, 21, 16's Lane
half and 18's row-cap half; T3 added the TUI's `set_upstream` retry; DS
registered Plan Studio and Agent Board as D15 and D16 in the design package and
DS2 fixed the light-skin `--accent-bright` contrast at the token; G10 ran the
design-fidelity gate; H3 adjudicated the register and made the timing tests
deterministic; H4–H8 and E3e fixed the native GUI defects the E3a–E3e runs
found, except those listed below; P1 and P2 built the packaging path and the
GUI release workflow.

R1 itself changed no behaviour. It declared the checkpoint and moved the three
versions, and found and fixed one bookkeeping gap: Core's release manifest
advertised 30 of the 33 capabilities and pinned no row for C15's two fixtures,
and no test noticed. The contract test now derives both from the source of
truth (see the compatibility document).

## Exit criteria

| # | Criterion | State | Evidence |
| --- | --- | --- | --- |
| 1 | The `0.3.4` real task evidenced natively through the GUI cockpit | **Met, natively, with three qualifications** | E3e on `claude/int-0.3.5` at `3c652fa8`: Lane created under Core's approval, its TASK run as a turn, `edit_file` confined to the Lane worktree, a follow-up acknowledged and drained (follow-up 16 met on the GUI), stage, commit and push each decided on the dock with Core's `no_upstream` answered by `Push and set upstream` and four `source.push` rows, `Always`/revoke, and Plan Studio's Accept → Hand off → D13. The qualifications, from the plan's own amendment: the bind was taken at launch through `VIDEN_GUI_WORKSPACE`, not `⌘O`; the push completed on a second `Commit & Push ⇡` pair in a relaunched process after defect 43 cost the first pair's pending push; and E3e's bundle carries two GUI blocker fixes taken red-first in that batch (`5afb01b0`, `4c92b511`). [checkpoints.md, E3e](release-evidence/gui-trusted-delivery/checkpoints.md#native-gui-run--2026-10-10-035-candidate-3c652fa8-e3e) |
| 2 | Follow-ups 10, 13, 14, 15, 16 closed with dates and file:line; the Lane-path approval writes its audit row before the fact | **Met** | All five closed 2026-09-13 by C12 with file:line; 16's Lane half closed by C18 and met natively at E3e. Residual: follow-up 17 (a Lane-path row can say "allow" when the worker's re-check refuses), open for `0.3.6`. [core-0.3-compatibility.md](core-0.3-compatibility.md) |
| 3 | Plan Studio and Agent Board as registered D-screens and as centre-pane views on Core facts, unbacked controls disabled and labelled | **Met; Agent Board unevidenced natively** | D15/D16 registered by DS; G8/G9 in the cockpit; Plan Studio driven natively at E3e. The Agent Board's controls were never driven natively: no ACP binary answers on this host, so no Agent session exists to pause or kill |
| 4 | Core `0.3.8` declares 33 capabilities with fixtures; the nine frozen base fixtures and every `0.3.4` fixture byte-identical to `edcc7b3d` | **Met, with the plan's own exception** | Nine base fixtures identical; every `0.3.4` extension fixture identical except `interaction-closed-loop`, whose bytes C12 refreshed under follow-up 13 (goal 2 of this plan required it) with its view digest unchanged. Proof table below |
| 5 | Register 019 and 030 closed on both sides; 013, 018, 021, 023, 026, 029, 031, 032, 033 closed or re-dated with reasons | **Partly met** | 019 and 030 closed on both sides (G9b). Seven re-dated to `0.3.6` by H3 with a reason, 029 written down as a reservation re-dated to `0.3.6`. **032 stays open and unscheduled**: H3 deferred "is an integrated terminal out of scope?" to the contract design, and CD2 did not take that decision. 036 opened and stays open as a TUI adoption item. [contract-requests.md](../apps/gui/contract-requests.md) |
| 6 | Design-fidelity report passes for every surface, both themes, with CJK, keyboard-only, accessibility and performance records | **Partly met** | The headless gate passes for 22 surfaces × 2 pairs. Four legs did not run because the host screen was locked: CJK IME, native keyboard-only traversal, release-bundle performance, native capture. F2 was closed by DS2 at the token; F5, F6 and F7 are filed for the design package. [release-0.3.5-fidelity-gate.md](release-0.3.5-fidelity-gate.md) |
| 7 | Bundles for three platforms with checksums; each external step done under authorization or recorded as not run | **Partly met** | Only macOS arm64 exists: the `.app` rebuilt at `rc.6` (binary SHA-256 `efff1259…c53a`). macOS x86_64, Windows and Linux need an authorized `release.yml` dispatch on a pushed ref. Every external step is recorded below as not run, with the missing authorization named. [release-0.3.5-packaging.md](release-0.3.5-packaging.md) |
| 8 | The completion report in both languages, stating what is pushed and naming the GUI version earned | **Met** | This document. Nothing is pushed; the GUI earned `0.1.0-rc.6` |

## Native proof, E3a to E3e

Read from [checkpoints.md](release-evidence/gui-trusted-delivery/checkpoints.md):

| Run | Candidate | What it reached |
| --- | --- | --- |
| E3a (2026-09-13) | `main` `75e2a84a` (= `0.3.4`) | the workspace half — bind, stage, commit, push, audit, file read; no Lane could be created (defects 14–16) |
| E3b (2026-09-13) | `da4c8a6d` | Lane creation and its audit row, a Lane-scoped stage and commit, Plan Studio, the Agent Board; the Lane's turn blocked (defect 22) |
| E3c (2026-09-14) | `22124903` | defect 22 closed natively; tool approval with typed hunks, archived patch, `Always` and `Edit`; blocked by 28, 29, 30 |
| E3d (2026-09-14) | `71d5156a` | 28, 30, 32, 33, 34 closed natively; a Lane-scoped stage never reached a decidable surface (defect 35) |
| H7, H8 (2026-09-14) | int | 35 and 36's client half fixed and confirmed natively; 25 (the `git_push` half) closed natively by H8 |
| **E3e (2026-10-10)** | **`3c652fa8`** | **the whole real task, natively**, with the three qualifications under exit criterion 1 |

## Open on the candidate

**GUI defects** (numbered in the checkpoints document, all GUI unless stated):
39 (a turn parked on an approval cannot be cancelled), 40 (the context dock
keeps the previous Lane's sample after a selection), 43 (the `Lanes` rail can
render the pre-hydration `CONNECTING` shell over a live connection), 44 (Plan
Studio's decision row is hidden behind the composer at 1280×800), 45 (Plan
Studio keeps the `plan_mode` refusal after switching to Build until reopened),
46 (a new healthy Lane shows another Lane's `AGENT STOPPED` surface; owner
undecided, GUI or Core).

**Core defect 42**: a proposed plan does not survive a Core restart although
`plan_document_updated` is in `agents.jsonl`. Recorded as compatibility
follow-up **22**, not fixed in R1 — fixing a Core behaviour in the step that
declares the checkpoint would invalidate it.

**Compatibility follow-ups open**: 17 (above), 18 (its row-cap half closed by
C19 — the test now tells a deadline kill from a row cap; its second test, the
repeated-timeout case, was left unchanged because its resample loop already
absorbs a stalled first call, and it passed in every C19 run and in R1's
parallel suite), and 22 (above). There is no follow-up 20 open:
defect 22 of the evidence log was not Core's, and follow-up 20 (a Lane's tool
root) is closed.

**Open register**: 013, 018, 021, 023, 026, 031, 032, 033, 036 — nine,
`open_count = 9` in the rc.6 manifest; 029 is a reservation, not counted.

**Environment.** On this macOS Darwin 27 host the first exec of a freshly
written script stalls about 3 s under full-suite load (syspolicyd inferred, not
proven); the product's real `git` is unaffected. C19 made the one test that
raced it tell a deadline kill from a row cap, and the full workspace suite ran
in parallel with no crate excluded and passed (below). `rustup` is absent, so
no local x86_64 bundle exists. The host screen was unlocked for E3e and locked
for G10.

## Frozen fixture proof

`shasum -a 256` of each file against `git show edcc7b3d:<path>`, run on the R1
branch:

| Set | Result |
| --- | --- |
| Nine frozen base fixtures (`stream-tool`, `approval-allow-deny`, `queued-follow-up`, `dag-blocker`, `multi-lane`, `merge-gate`, `context-pressure-cost-blind`, `plan-denial`, `d1-vertical-slice`) | **all identical**, and equal to their `apps/tui/release-manifest.toml` digests |
| Every other fixture present at `edcc7b3d` (22 files, `legacy-lanes.tsv` included) | identical, except `interaction-closed-loop.json`: `a6f1c436…` → `c2ea4647…` (C12, follow-up 13; view digest `c43d9fe3…` unchanged) |
| New in `0.3.5` | `plan-documents`, `agent-session-control`, `approval-scopes` (C16 refresh), `starter-lane-agent` — each pinned in `crates/core/release-manifest.toml` and derived by `frontend_contract_v1.rs` |

## Gates

Run on `claude/release-gate-r1` at the commit that declares the checkpoint,
offline.

| Command | Result |
| --- | --- |
| `cargo test --workspace --quiet` (parallel, no exclude, no `--test-threads`) | PASS, exit 0, **2343 passed, 0 failed**, 30 ignored, 89 result lines, 192 s |
| `cargo fmt --all -- --check` | PASS, exit 0 |
| `cargo clippy --workspace --all-targets` | PASS, exit 0, **zero warnings** |
| `scripts/check-dependency-boundaries.sh` | PASS, exit 0 |
| `scripts/tui-regression.sh` | PASS, exit 0; evidence at `target/tui-regression/0.3.6/` |
| `scripts/tui-turn-controller-smoke.sh` | PASS, `TUI TurnController smoke passed` |
| `scripts/rc-tui-stability-smoke.sh` | PASS, exit 0 |
| `scripts/tui-previews.sh` | PASS, exit 0 (its default directory is still `target/tui-previews/0.3.4/`, a stale label the script has carried since `0.3.4`) |
| `npm --prefix apps/gui test -- --run` | PASS, **70 files / 982 tests** |
| `npm --prefix apps/gui run build` | PASS (`tsc --noEmit && vite build`) |
| `cargo test -p viden-gui` | PASS, 46 suites, **392 passed**, 1 ignored |
| `cargo test -p viden-gui --test capture_projections -- --ignored` | PASS, 1 |
| Migration (`frontend_contract_v1_migrations_are_idempotent_before_fixture_replay`) | PASS, 1 |
| `npm --prefix apps/gui run tauri -- build --bundles app` | PASS in 56 s, `0.1.0-rc.6` |
| `scripts/check-doc-pairs.sh`, `scripts/check-doc-links.sh` on every changed Markdown pair; `git diff --check` | PASS |

R1's own red-first evidence: with the tests moved first, `frontend_contract_v1`
failed on the version (3 tests), `architecture_boundary` on the rc.6 version,
the missing rc.6 snapshot and the edited rc.5 snapshot (`e3873be0…` against the
shipped `b2177792…`), and the TUI manifest test on `0.3.5`; with only the Core
version moved, the contract test failed at the new capability-line assertion.
All green after the change.

### The prepublish gate

`scripts/release-gate.sh --version 0.1.30 --phase prepublish` **refused**,
exit 2, `DEEPSEEK_API_KEY is required for release prepublish gate` — the key is
absent and no "go" was given. Its legs were then run one by one, in its order:

| Leg, in `release-gate.sh` order | Ran? | Result |
| --- | --- | --- |
| `scripts/check-task10-guards-test.sh` | yes | PASS, `Task 10 guard fixture tests passed.` |
| `scripts/check-dependency-boundaries.sh` | yes | PASS |
| `scripts/check-doc-pairs.sh` over the gate's twelve files | yes | PASS |
| `scripts/check-doc-links.sh` over the gate's four files | yes | PASS |
| `scripts/context-engine-benchmark.sh` deterministic, 3 runs | yes | PASS, `Result: passed`, median input tokens off `1000` / on `750` (`25.00%` reduction) |
| `scripts/release-smoke.sh --version 0.1.30 --target aarch64-apple-darwin`, without `--deepseek` | yes, 393 s | **17 PASS, 5 SKIP**: `cargo-fmt`, `tdd-testing-contract-smoke`, `tui-turn-controller-smoke`, `rc-tui-stability-contract-smoke`, `final-zero-bug-contract-smoke`, `cargo-clippy` (`-D warnings`), `viden-terminal-entry-tests`, `workspace-tests` (serial), `tui-regression`, `fallback-cli-smoke`, `plan-mode-smoke`, `daily-loop-smoke`, `codex-app-server-protocol-fixture`, `codex-app-server-write-guard`, `lane-operator-loop-smoke`, `rc-tui-stability-smoke`, `package-smoke` (`viden-v0.1.30-aarch64-apple-darwin.tar.gz`, 10,268,799 B, SHA-256 `ef9e3fa32c41fa781e162c80240a542c9fd103f6cb21e24e7b8665e6e13e3182`, binary reports `viden 0.1.30`) |
| ↳ `deepseek-dev-scenario-smoke` (the gate passes `--deepseek`) | **refused** | not run — authorization missing: no `DEEPSEEK_API_KEY`, no "go" |
| ↳ `provider-live-smoke`, `github-actions-release-validation`, `github-release-assets-validation`, `homebrew-validation` | skipped by flag | not prepublish legs of this gate; the last three are external and unauthorized |
| `scripts/final-zero-bug-smoke.sh` (required because the version is `0.1.30`) | yes | **FAIL, exit 2**: `VIDEN_TUI_MANUAL_EVIDENCE_DIR is required for the final zero-bug gate`. The leg wants at least one human-taken macOS Terminal and one iTerm2 screenshot of the TUI; none exists for this candidate and an agent cannot take one on a locked screen. An evidence gap, not an authorization gap |
| `scripts/context-engine-benchmark.sh --provider deepseek` (live A/B) | **refused** | not run — authorization missing: no `DEEPSEEK_API_KEY`, no "go" |
| Migration (the contract's migration-before-replay test) | yes | PASS, 1 test, and inside both workspace runs |
| Full workspace | yes | PASS twice: parallel (2343 passed) and serial inside `release-smoke` |

So the prepublish gate is **not passed**: two legs are refused for missing
authorization and one (`final-zero-bug`) fails for missing manual Terminal and
iTerm2 evidence. Every other leg passes.

## Release identity

Read-only `gh` calls on 2026-10-10 found that the latest GitHub Release is
`v0.1.30`, titled "RoboCode v0.1.30", with `robocode-v0.1.30-*` assets, and that
`wikieden/homebrew-tap` holds only `Formula/robocode.rb`. The release gate takes
the workspace/CLI version (`0.1.30`) and its postpublish legs expect `viden-v*` /
`Viden*` assets and a `wikieden/tap/viden` formula, so **postpublish at
`0.1.30` would fail both legs**. A `0.3.5` release needs, first, a version
decision (a new workspace version or another tag scheme for the three lines)
and a tap change that adds or renames the formula. Details in
[release-0.3.5-packaging.md](release-0.3.5-packaging.md), "R1".

## External steps

Every row is **not run — authorization missing**. Each needs its own "go",
relayed by the dispatcher at the moment it runs.

| Step | Precondition besides the "go" | State |
| --- | --- | --- |
| Live-provider certification (`release-smoke --deepseek`, the DeepSeek A/B context benchmark) | `DEEPSEEK_API_KEY` in the environment | not run — authorization missing; the key is absent |
| Push the candidate (`claude/int-0.3.5` with R1 merged, then `main`) | the user's explicit "push" | not run — authorization missing |
| Dispatch `release.yml` with `upload_to_release=false` (macOS x86_64, Windows, Linux CLI and GUI bundles) | a pushed ref; `gh` is now authenticated | not run — authorization missing |
| Code signing (Developer ID) | an identity on the host or in Actions secrets, `signingIdentity` and entitlements in `tauri.conf.json` | not run — authorization missing; no identity exists |
| Notarization | Apple ID / Team ID credentials | not run — authorization missing; no credentials |
| Version decision for the release unit | see "Release identity" | not taken — user decision |
| Git tag | the version decision | not run — authorization missing |
| GitHub Release with the P1/R1 and dispatched assets | the tag and the dispatched artifacts | not run — authorization missing |
| Homebrew tap (`wikieden/homebrew-tap`) at the same version | a `viden` formula and the published assets | not run — authorization missing |
| `scripts/release-gate.sh --phase postpublish` | the release and the tap | not run — authorization missing; would fail today for the reasons above |

## Status

**Candidate on `claude/int-0.3.5` at `d522e4f3` with R1's release-step commits
on `claude/release-gate-r1`, awaiting push.** Not on `main` (which is at the
mid-plan snapshot `0dcfaa66`), not merged, not tagged, not released. The next
safe actions are decisions, in order: merge R1 into `claude/int-0.3.5` and
push on the user's word; choose the release version; then grant the external
steps above one at a time.
