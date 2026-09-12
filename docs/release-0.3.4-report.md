# Viden 0.3.4 Completion Report

Chinese version: [release-0.3.4-report.zh-CN.md](release-0.3.4-report.zh-CN.md)

Date: 2026-09-12. Written by the release-evidence step (batch E2) of
[release-0.3.4-plan.md](release-0.3.4-plan.md).

**Nothing in this report is pushed.** The whole `0.3.4` candidate sits on the
local integration branch `claude/int-0.3.4`; `main` is still at `25072a0a`. No
tag, no publish, no Homebrew change, no notarization, no live-provider
certification.

## The candidate

| Line | Version | Where |
| --- | --- | --- |
| Workspace candidate | `0.3.4` | `claude/int-0.3.4` at `39ed155dcc1847b915965f49626e6779b8b538d7`, 68 commits ahead of `main` at `25072a0acee7a9959bfd8060721378cb3d4d5397` |
| Core | `0.3.7` immutable contract checkpoint | checkpoint SHA `39ed155dcc1847b915965f49626e6779b8b538d7`; schema `1`; 15 frozen base + **29** extension capabilities; base contract checkpoint `5bd2b80b…` unchanged since `0.3.0` |
| TUI | `0.3.5` | `min_core_version` deliberately unchanged at `0.3.4` — the new capabilities are independently negotiated feature gates, so none of them blocks a base schema-1 client from starting |
| GUI | `0.1.0-rc.5` | `[core].minimum_version` deliberately unchanged at `0.3.5`, for the same reason; macOS `.app` built at this version |

The Core `0.3.7` checkpoint is declared once here rather than per batch,
because a checkpoint named while the contract is still moving names a contract
that does not exist. It names a commit on a local branch: **if
`claude/int-0.3.4` is rebased before it merges, the checkpoint must be
re-declared against the new SHA** rather than assumed to have survived.

## What shipped

Six additive Core capabilities, taking the advertised extension set from 23 to
29 with the nine frozen base fixtures byte-unchanged:

| Batch | Capability | What it makes possible |
| --- | --- | --- |
| C5 | `runtime.workspace_owner`, `ui.layout_preferences` | work scoped to the workspace root has an operator identity, so a commit and a push need no Lane; Lane worktrees stop overwriting the workspace source chip; the cockpit layout persists. Closed GUI-CORE-027 |
| C6 | `runtime.turn_lifecycle` | a native turn has a terminal fact, so a client's busy predicate is a Core fact rather than display residue, and the session queue drains behind a completed turn |
| C7 | `runtime.durable_work_evidence` | an applied native mutation archives a `patch` row with canonical bytes, and an approval decision becomes a durable audit row. Closed GUI-CORE-028 |
| C8 | `runtime.transcript_rows` | owner-scoped ordered typed rows. Closed GUI-CORE-009 |
| C9 | `runtime.workspace_file_reads` | one file's bytes behind the `read_file` permission gate |
| C10, C11 | — | the workspace-owner binding moved into the shared bootstrap so every frontend entrypoint gets the identity; persisted transcript strings replay as UTF-8 |

Five GUI batches brought the cockpit level with the accepted D1 design — the
rail-as-router navigation shell with in-cockpit secondary views (G3), the Lane
tab strip, inline tool diffs, palette Lane creation and focus mode (G4), the
tabbed context dock (G5), the D10/D13/D14 view work (G6), and the consumers for
all six new capabilities (G7). T2 brought the TUI to parity. H2 closed the E1
hygiene defects.

## What was evidenced, and on which surface

The milestone's point was goal 6: the real task `0.3.3` left partial,
completed. It **completed end to end for the first time**, through the TUI:
intake, Lane creation under a Core approval, a mutation approved against Core's
typed hunks and applied to a real file, an archived `patch` row whose canonical
hash equals the bytes on disk, a durable `approval.allow_once` audit row, a
stage, a **commit**, a push refused as `NoUpstream`, a push **accepted** into a
real remote, and a queued follow-up that drained behind a completed turn — with
every source-control step taken with **no Lane selected**, under the workspace
owner Core published.

Three Core outcome variants were observed live for the first time
(`OperatorGitOutcome::Completed`, `Failed { NoUpstream }`, and a non-empty
`EvidencePage` and `AuditPage`); before this run they existed only as fixture
replay.

## What is partial

**One thing, and it is the same thing for the third release step running: the
native GUI window was not driven.** The host screen was locked
(`CGSSessionScreenIsLocked = 1`) at 14:37:21Z, before this batch's work began,
and still locked at 14:45:30Z when the `0.1.0-rc.5` bundle was ready. No
keystroke was sent, because a locked session routes input to the login window,
and unlocking needs the user's password. So:

- the `.app` bundle was **built** and its facts recorded (`0.1.0-rc.5`,
  `dev.viden.gui`, arm64, ad-hoc linker-signed, 40,655,440-byte executable),
  but not launched to be driven;
- **no native capture exists**, so exit criterion 7 fails;
- the GUI's own surfaces are covered by the G3–G7 deterministic harness
  captures under `apps/gui/evidence/` and by 864 vitest tests — evidence about
  rendering, not about the integration.

Goal 6 is therefore **met on the contract and on the TUI, and unproven on the
GUI cockpit**. The remaining gap is environmental, not a capability gap: one
run with an unlocked screen closes it.

`0.3.4` is also not complete in the sense its own exit criteria use, which is
"on `main`" — the candidate is unpushed and unmerged. That is a decision for
the user, not a gap.

## Follow-ups E2 opened

Four defects, each reproduced and located in the source, none fixed here —
fixing a Core behaviour in the step that declares the checkpoint would
invalidate the checkpoint.

| # | Finding | Owner | Recorded as |
| --- | --- | --- | --- |
| 1 | An archived patch's rendered diff calls a modified file a rename: `evidence_reads.rs:147` publishes `render_diff`'s placeholder `before`/`after` headers without stamping the entry's own path, which both other readers of that output do | Core | compatibility follow-up 14 |
| 2 | A lane-lifecycle approval decision writes no durable audit row: `RespondToApproval` returns on the lane branch before `record_decision`, so `lane_create` shows an audit id that resolves to nothing | Core, blocking for audit completeness | compatibility follow-up 15 |
| 3 | A session follow-up queued during a turn is never observably pending: `InputQueued` and `InputDequeued` arrive in the same instant, so `queued_inputs` stays empty and the operator sees no queue | Core/TUI seam, product gap | compatibility follow-up 16 |
| 4 | The `NoUpstream` recovery has no TUI control: the `/git` picker hard-codes `set_upstream: false`, so the step Core's own refusal names is unreachable | TUI | `release-tui-0.3.4-source-control-parity.md`, Known Gaps |

Carried into `0.3.5`: compatibility follow-ups 10 (ACP artifact canonicalized
live but not durably) and 13 (the `interaction-closed-loop` generator drift),
plus the register's ten open entries — 013, 018, 019, 021, 023, 026 and the
four G7 entries 030–033, with 029 reserved for the D5 gallery review.

## Gates

Run on `claude/e2-release-evidence` in its worktree, offline. Full tails and
the fixture proof are in
[release-evidence/gui-trusted-delivery/checkpoints.md](release-evidence/gui-trusted-delivery/checkpoints.md).

| Command | Result |
| --- | --- |
| `cargo test --workspace --quiet --exclude viden-plugin-host --exclude viden-agents` | PASS, exit 0, **2014 passed, 0 failed**, 78 result lines |
| `cargo test -p viden-plugin-host -- --test-threads=1` | PASS, 27 passed |
| `cargo test -p viden-agents -- --test-threads=1` | PASS, 80 passed |
| `cargo test -p viden-core` | PASS, 61 across 6 suites, 21 ignored |
| `cargo test -p viden-core --test frontend_contract_v1` | PASS, 25 passed, 21 ignored |
| `cargo test -p viden-tui` | PASS, 427 |
| `cargo test -p viden-types` | PASS, 191 |
| `cargo test -p viden-gui` | PASS, 311, 1 ignored |
| `cargo test -p viden-gui --test architecture_boundary` | PASS, 7 |
| `cargo test -p viden-gui --test capture_projections -- --ignored` | PASS, 1 |
| `cargo fmt --all -- --check` | PASS, exit 0 |
| `cargo clippy --workspace --all-targets` | PASS, exit 0, warnings only: 9, every one in a file this batch's diff does not touch (`crates/runtime/src/tests/frontend_services_tests.rs`, `crates/runtime/src/runtime_contract.rs`, `crates/workflows/src/lanes.rs`, `crates/types/src/agent.rs`, `apps/gui/src-tauri/src/projection.rs` ×3, `apps/gui/src-tauri/src/adapter.rs` ×2). Zero new |
| `scripts/check-dependency-boundaries.sh` | PASS, exit 0 |
| `scripts/tui-regression.sh` | PASS, exit 0; evidence at `target/tui-regression/0.3.5/` |
| `scripts/tui-previews.sh` | PASS (run inside `tui-regression.sh`) |
| `scripts/tui-turn-controller-smoke.sh` | PASS, exit 0 |
| `scripts/rc-tui-stability-smoke.sh` | PASS, exit 0 |
| `npm --prefix apps/gui test -- --run` | PASS, 59 files / **864 tests** |
| `npm --prefix apps/gui run build` | PASS (`tsc --noEmit && vite build`) |
| `npm --prefix apps/gui run tauri -- build --bundles app` | PASS in 1 m 10 s, `0.1.0-rc.5` |
| Nine frozen base fixtures | PASS — byte-identical to `git show 25072a0a:<path>` **and** to their pinned digests in `apps/tui/release-manifest.toml` |
| `scripts/check-doc-pairs.sh`, `scripts/check-doc-links.sh` (every changed Markdown pair) | PASS |
| `git diff --check` | PASS, exit 0 |
| `scripts/release-gate.sh --phase prepublish` | **NOT RUN.** It refuses to start without `DEEPSEEK_API_KEY` and its smoke leg is a live-provider certification this batch is not authorized to run. Its offline legs — dependency boundaries, the doc checks — are in the rows above |

`viden-plugin-host` and `viden-agents` are run serially and excluded from the
parallel workspace run: both have timing tests that flake under parallel load
on this host, which the `0.3.4` plan records as the rule rather than as a pass.

## The next safe action

Nothing in `0.3.4` needs more code. The next action is a decision, not a task,
and there are two:

1. **Take the native GUI run** when the host screen is unlocked — the only
   remaining piece of goal 6, and the only failing exit criterion. The harness
   facts the next run needs are recorded in the evidence document.
2. **Push and merge `claude/int-0.3.4`**, which requires explicit user
   authorization and has not been given. Until then every version in this
   report is a local candidate.

`0.3.5` owns the production release gate — packaging, notarization, Homebrew,
live-provider certification — plus Plan Studio, Agent Board, the D5 gallery
review, and the follow-ups listed above.
