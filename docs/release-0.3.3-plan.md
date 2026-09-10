# Viden 0.3.3 Plan - Trusted Delivery And Operable GUI Beta

Chinese version: [release-0.3.3-plan.zh-CN.md](release-0.3.3-plan.zh-CN.md)

`0.3.3` is the aggregate workspace milestone named in `PLAN.md` ("operable
GUI beta and compatibility hardening") and in
`docs/parallel-development-plan.md` ("trusted delivery and operable GUI
beta"). This document turns that one-line entry into batches, gates, and exit
criteria. Its Core contract content lives in
[release-0.3.3-contract-design.md](release-0.3.3-contract-design.md).

Status: plan approved for dispatch on 2026-09-09. Nothing in this document is
implemented. Every batch below is dispatched separately, reviewed
adversarially in the main session, and merged only on an explicit push.

## Baseline

`main` at `216c1e4f` on 2026-09-09:

| Line | Version | State |
| --- | --- | --- |
| Core | `0.3.5` immutable checkpoint | 19 extension capabilities, schema `1` |
| TUI | `0.3.3` (component line) | native/ACP interaction certified against Core `0.3.4` |
| GUI | `0.1.0-rc.3` | D11/D4/D1/D2/D6/D10/D12/D13/D14 shells on the CoreClient seam |

Since `0.3.2` closed on 2026-08-30, `main` gained: the GUI contract-request
backlog adjudication (eleven requests closed, five deferred), the
`viden-lanes` and `viden-agents` crate cuts, the transcript double-persistence
fix and the R1 review batch, the QW1 D1 quick wins, and the design
adjudication `D-RAILNAV`. The open register in `apps/gui/contract-requests.md`
is 009, 012, 013, 015, 018, 019, 020, 021, and 023.

## Goals

1. **Trusted delivery surface.** Build the registered `DiffReview` in-cockpit
   view as the single host for operator git actions (GUI-CORE-020),
   structured approval diff (GUI-CORE-012), and structured conflict content
   (GUI-CORE-015), on one Core contract increment rather than three point
   fixes.
2. **Evidence surface.** Build the registered `EvidenceView` read side: a
   paged evidence query and a canonical-bytes content read.
3. **Compatibility hardening.** Both clients replay every new extension
   fixture to the same business facts; the deferred review follow-ups and the
   D1 hygiene items recorded during the gap review are closed or re-dated.
4. **Auditable real task.** One real local-first development task completes
   through the GUI, from intake to a committed change, with audit and
   evidence recorded under `docs/release-evidence/`.

## Scope Amendments To The Controlling Plan

Dated 2026-09-09; recorded as notes in `docs/parallel-development-plan.md`.

- **Plan Studio and Agent Board move to `0.3.4`.** Neither has a registered
  D-screen, neither has a Core plan-handoff or board contract, and `0.3.3`
  already adds two new cockpit surfaces. Adding a fourth and fifth without
  contracts would repeat the unregistered-view situation `D-RAILNAV` just
  resolved.
- **"GUI completes D2/D10/D12/D14" is read as:** D2 gains typed decision
  context (hunks) for approvals; D12 gains structured conflict content;
  D10 and D14 change only through hygiene. Both were migrated to the audit
  and live-work contracts during the `0.3.2` lane-debt clearance.
- **Per `D-RAILNAV`:** WorktreeBoard is dropped; the embedded subagent tree,
  DiagnosticsView, and the DockSD summon dock stay outside `0.3.3`.

## Component Version Targets

| Line | Now | `0.3.3` target |
| --- | --- | --- |
| Core | `0.3.5` | `0.3.6` immutable checkpoint, schema `1`, four additive capabilities |
| TUI | `0.3.3` | `0.3.4`, parity minimum for the four capabilities |
| GUI | `0.1.0-rc.3` | `0.1.0-rc.4`, beta candidate with DiffReview and EvidenceView |

The aggregate workspace stays unreleased. Packaging, notarization, Homebrew,
and live-provider certification belong to the `0.3.4` release gate.

## Batches

Each batch is one worktree under `.worktrees/`, developed test-first, and
reviewed against its brief before merge. Core batches land before the client
batches that consume them; the contract design gives the exact shapes.

| Batch | Owner | Content | Depends on |
| --- | --- | --- | --- |
| C1 `runtime.structured_diff` | Core | `DiffDocument` types, parser promotion, `ApprovalRequestView.decision_context`, `WorkspaceChangeView.diff`, `QueryWorkspaceDiff` / `WorkspaceDiffLoaded`, fixture `structured-diff.json`, docs | none |
| C2 `runtime.operator_git` | Core | `RunOperatorGitAction` / `OperatorGitActionFinished`, tool-spec reuse for the permission gate, audit-before-effect, source resample, fixture `operator-git.json`, docs | C1 |
| C3 `runtime.conflict_content` | Core | `ConflictContent` on `ConflictBounce` and `LaneConflictView`, producer at the two apply sites, fixture `conflict-content.json`, docs | C1 (parallel with C2) |
| C4 `runtime.evidence_reads` | Core | `QueryEvidence` / `EvidencePageLoaded`, `ReadEvidenceContent` / `EvidenceContentLoaded`, fixture `evidence-reads.json`, docs; opens and closes GUI-CORE-025 | C1 |
| D0 gui-kit promotion | Design package | promote the `.review` and `.evwrap` families from D1-inline CSS into `gui-kit.css` per `D-SOT`; package guards; changelog | none (parallel) |
| G1 DiffReview | GUI | in-cockpit view (file tree, staged marks, unified diff, commit bar), titlebar sync chip becomes a control, D2 hunk rows, qa states, PNG evidence, register closes for 012 and 020 | C1, C2, D0 |
| G2 conflict + evidence | GUI | D12 ours/theirs/base hunks, EvidenceView (day-grouped list, detail, linked chips, open-in-review), register closes for 015 and 025 | C3, C4, D0 |
| T1 TUI parity | TUI | approval overlay hunk rows, `/git` system command, conflict detail in the decisions overlay, evidence inspector, regression baselines with causes | C1 to C4 |
| H1 hygiene | GUI, TUI | stale citations (004/006 in `d1.rs`, 001 in D11, the `GUI-CORE-D1-OWNER-CARDINALITY` lookalike), unreachable `component_gallery.ts`, missing PNGs for D4/D13/D2 base/D10 multi-lane, D2 contract detail showing live Confirm/Reject against decided records, dead `CockpitProjection.assistant_stream`, divergent active-work definitions, gate-id keying inconsistency | none |
| S1 stretch | Core, TUI | GUI-CORE-013 pending-contract fact plus the handoff/contract/dependency creation flows whose intent builders already exist in `apps/tui/src/tui/supervision.rs` | C1 to C4, G1, G2 merged first; otherwise rolls to `0.3.4` |
| E1 release evidence | all | one real task: intake, Lane, edit approved with hunks, check run, DiffReview commit, push refused then accepted, evidence and audit reviewed; bilingual checkpoints under `docs/release-evidence/gui-trusted-delivery/` | G1, G2, T1 |

Dispatch order: C1, then C2 and C3 in parallel with D0, then C4, then G1,
G2, and T1, with H1 interleaved wherever a reviewer is idle. S1 starts only
when everything before it is on `main`.

## Gates

Per batch, the smallest relevant check first, then the branch gates from
`docs/parallel-development-plan.md`.

Core batches:

```bash
cargo test -p viden-types
cargo test -p viden-runtime
cargo test -p viden-core
scripts/check-dependency-boundaries.sh
cargo test --workspace --quiet
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
```

Plus: the nine frozen `frontend-contract-v1` base fixtures keep their exact
bytes and pinned view digests; every new event has an
`is_known_runtime_event_type` arm and a fixture in the same commit; the
capability count in `scripts/tui-regression.sh` moves from 19 to 23 with a
tracking comment; every new command refusal is `CommandRejected` with the
caller's command id.

TUI batches:

```bash
cargo test -p viden-tui
scripts/tui-turn-controller-smoke.sh
scripts/rc-tui-stability-smoke.sh
scripts/tui-regression.sh
```

GUI batches:

```bash
npm --prefix apps/gui test -- --run
npm --prefix apps/gui run build
cargo test -p viden-gui
cargo test -p viden-gui --test capture_projections -- --ignored
```

Plus qa harness states and headless PNG recaptures listed in the batch
report, and `apps/gui/EVIDENCE.md` updated in both languages.

Documentation and design package:

```bash
scripts/check-doc-pairs.sh <changed-markdown> [...]
scripts/check-doc-links.sh <changed-markdown> [...]
git diff --check
node docs/viden-design/Viden/tools/run-checks.node.js
```

Known load-sensitive tests, rerun isolated once and serially if still red:
`acp_async_job_can_be_cancelled_by_pid`, the `context_reducer_process_*`
family, and `inline_image_bytes_become_referenced_evidence`.

## Exit Criteria

`0.3.3` is complete only when all of the following hold on `main`:

- Core `0.3.6` is recorded as an immutable checkpoint in
  `crates/core/release-manifest.toml` with the 23-capability set, four new
  extension fixtures, and unchanged base fixture bytes.
- The GUI ships DiffReview and EvidenceView as the registered families,
  D2 renders decision-context hunks, D12 renders conflict content, and every
  absent fact renders as unavailable rather than as a value.
- The TUI reaches parity minimum on all four capabilities and every moved
  regression baseline has a stated cause.
- Both clients replay the four new fixtures to identical business facts.
- The register closes 012, 015, 020, and 025 with dates; 009, 018, and 019
  carry `0.3.4` notes; 021 and 023 stay `0.3.4+`.
- The E1 evidence document exists in both languages, and the deferred
  follow-ups in `docs/core-0.3-compatibility.md` are closed or re-dated.

Explicitly not part of completion: three-platform packaging, notarization,
Homebrew sync, live DeepSeek certification, Plan Studio, Agent Board, forge
status (021), and multi-workspace supervision (023).

## Risks

- **Frozen digest movement.** Adding optional fields to `ApprovalRequestView`
  and `WorkspaceChangeView` must not change the canonical bytes of any base
  fixture view; the fields are omitted when absent. C1's frozen-fixture test
  is the proof, and a moved base digest stops the batch.
- **Shared permission vocabulary.** Operator git actions reuse the agent tool
  specs so one `viden.toml` rule set governs both. A new spec (`git_fetch`)
  must be documented with the others before G1 exposes fetch.
- **Bounds in real repositories.** Diff and conflict payloads are bounded by
  bytes and rows; both clients must render `truncated` and `omitted` as
  facts, never as an empty change set.
- **Conflict content semantics.** The content shows the incoming hunk, the
  current file region, and the patch preimage. It is not a three-way merge
  and must not be labelled as one.
- **Stretch creep.** S1 is a stretch; if C1 to C4 slip, S1 rolls to `0.3.4`
  without renegotiating the exit criteria.

## Status

Dated 2026-09-10. All work below is on the local integration branch
`claude/int-0.3.3`, not on `main`. A batch listed as landed is approved and
cherry-picked onto that branch; none of it is pushed, merged, or released.

| Batch | State | Approved |
| --- | --- | --- |
| C1 `runtime.structured_diff` | landed | 2026-09-09 |
| C2 `runtime.operator_git` | landed | 2026-09-09 |
| C3 `runtime.conflict_content` | landed | 2026-09-09 |
| C4 `runtime.evidence_reads` | landed | 2026-09-09 |
| D0 gui-kit promotion | landed as D0 (`9fc61225`) + D0b/D0c mirror (`36b416bc`) | D0 2026-09-09; the mirror round carries no separate approval record |
| H1 hygiene | landed | 2026-09-09 |
| G1a DiffReview read side | landed | 2026-09-09 |
| G1b DiffReview action side | landed | 2026-09-09 |
| G2a D12 conflict content | landed | 2026-09-09 |
| G2b EvidenceView | landed | 2026-09-10 |
| T1a TUI parity (approvals, `/git`, conflict detail) | landed | 2026-09-09 |
| T1b TUI evidence inspector | landed | 2026-09-10 |
| F1 facade re-exports and docs write-back | landed | 2026-09-10 |

G1 and G2 were each split in two once C2 and C4 landed, and T1 likewise; the
split is recorded here because the batch table above still names the
undivided units. D0 needed two follow-on rounds after the first proved the
promotion as briefed would have changed pixels.

S1 (GUI-CORE-013 plus the supervision creation flows) has not started. The
plan makes it conditional on everything before it being on `main`, which has
not happened, so it rolls to `0.3.4` unless the integration branch merges
first.

E1 (release evidence: one real task from intake to a committed change) is
pending and is the remaining gate for the exit criteria. It also owns the
Core `0.3.6` immutable-checkpoint declaration and the `component_version` bump
in `crates/core/release-manifest.toml`, neither of which any batch so far was
authorized to make.

Two contract facts changed since the baseline above: GUI-CORE-012, 015, 020,
and 025 are fully closed (Core published, both clients adopted), and
GUI-CORE-027 (workspace-scoped operator identity) is opened and scheduled for
`0.3.4`. The open register is 009, 013, 018, 019, 021, 023, 026, and 027.
