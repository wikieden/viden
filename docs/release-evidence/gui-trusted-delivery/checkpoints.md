# 0.3.3 Trusted Delivery Checkpoints

Chinese version: [checkpoints.zh-CN.md](checkpoints.zh-CN.md)

Date: 2026-09-10

This evidence describes a **local candidate only**. Nothing here is published,
signed, notarized, pushed, merged, tagged, or live-provider certified. Every
version below is a candidate recorded on a local integration branch; no release
gate that requires a network, a credential, or a packaging step was run.

## Candidate Line

| Item | SHA / path |
| --- | --- |
| Base `origin/main` | `7473a73eeec6336c2ed9059780d3ed97082b0c5e` |
| Integration branch | `claude/int-0.3.3` at `13795abce263bd60f0e4785e4af0f3f852cb3009` |
| E1 branch / worktree | `claude/e1-release-evidence` in `.worktrees/e1-release-evidence` |
| E1 HEAD at this document's evidence run | `727b87b5e6454e21e30449af24b4fa8836413103` |
| Core `0.3.6` implementation checkpoint | `1cec82185bbe860d6b8536a63741bc01f1edf2f6` (last commit touching `crates/**` on the integration branch) |
| Core `0.3.6` base contract checkpoint | `5bd2b80b0953f4194d082940a7b9164c7231ca2d` (unchanged since `0.3.0`) |
| TUI candidate | `0.3.4`, `min_core_version` unchanged at `0.3.4` |
| GUI candidate | `0.1.0-rc.4`, `[core].minimum_version` unchanged at `0.3.5` |
| Frontend schema | `1`, unchanged |
| Capabilities | 15 frozen base + 23 extensions |

The three component lines move independently, as
`docs/parallel-development-plan.md` requires. Core moves because the `0.3.3`
contract increment added four capabilities; the clients move because both
adopted all four.

### What "immutable checkpoint" means here

`crates/core/release-manifest.toml` now records `component_version = "0.3.6"`
with `status = "immutable_checkpoint"`. That declaration is about the contract,
not about distribution: the payload it freezes is the schema-1 protocol plus the
23 advertised capabilities and the fixture digests below. The commit it names is
on a local integration branch. If that branch is rebased before it merges, this
checkpoint must be re-declared against the new SHA rather than assumed to have
survived.

## Deterministic Evidence

Every row was run on `claude/e1-release-evidence` at the HEAD above, in the
worktree, offline.

| Command | Result |
| --- | --- |
| `cargo build --workspace --tests` | PASS |
| `cargo test -p viden-types` | PASS, 147 (140 + 7 + 0) |
| `cargo test -p viden-core` | PASS, 53 across 6 suites, 15 ignored |
| `cargo test -p viden-core --test frontend_contract_v1` | PASS, 19 passed, 15 ignored |
| `cargo test -p viden-tui` | PASS, 410 (409 + 1) |
| `cargo test -p viden-gui` | PASS, 274, 1 ignored |
| `cargo test -p viden-gui --test architecture_boundary` | PASS, 7 |
| `cargo test -p viden-gui --test capture_projections -- --ignored` | PASS, 1 |
| `cargo test --workspace --quiet` | PASS, **1942 passed, 0 failed**, 78 result lines, first run, no flake reruns needed |
| `npm --prefix apps/gui test -- --run` | PASS, 42 files / 656 tests |
| `npm --prefix apps/gui run build` | PASS (`tsc --noEmit && vite build`) |
| `npm --prefix apps/gui run tauri -- build --bundles app` | PASS |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets` | PASS, exit 0, warnings only (2 in `viden-runtime`, 4 in `viden-gui`), all pre-existing and outside this batch's diff |
| `scripts/check-dependency-boundaries.sh` | PASS, exit 0 |
| `scripts/tui-regression.sh` | PASS, exit 0; evidence at `target/tui-regression/0.3.4/` |
| `scripts/tui-previews.sh` | PASS (run inside `tui-regression.sh`) |
| `scripts/native-acp-fixture-parity.sh` | PASS, exit 0 |
| `scripts/tui-turn-controller-smoke.sh` | PASS, exit 0 |
| `scripts/rc-tui-stability-smoke.sh` | PASS, exit 0 |
| `scripts/check-doc-pairs.sh` (every changed Markdown pair) | PASS |
| `scripts/check-doc-links.sh` (every changed Markdown pair) | PASS |
| `git diff --check` | PASS |
| `scripts/release-gate.sh --phase prepublish` | **NOT RUN.** It refuses to start without `DEEPSEEK_API_KEY` and its `release-smoke.sh --deepseek` leg is a live-provider certification, which this batch is not authorized to run. Its offline legs — dependency boundaries, the doc pair/link checks — were run individually and are in the rows above. |

### Fixture digests

The nine frozen `frontend-contract-v1` base fixtures were verified byte-equal to
the digests pinned in `apps/tui/release-manifest.toml` with `shasum -a 256`. All
nine match; none moved.

The four `0.3.3` extension fixtures are recorded in
`crates/core/release-manifest.toml` for the first time. `payload_sha256` is
sha256 over the fixture file's exact bytes; `view_sha256` is the fixture's own
`expected_view_sha256`, which the replay test recomputes from the reduced view;
`final_cursor` is the sequence the replay ends at.

| Fixture | payload sha256 | view sha256 | final cursor |
| --- | --- | --- | --- |
| `structured-diff.json` | `e24915b3…0de21cd0` | `3f5f4caf…e080676b8a` | 6 |
| `operator-git.json` | `f29aa213…0ba683936` | `07eadb0e…5a86ac14e1` | 10 |
| `conflict-content.json` | `830afb77…22c96ce1f` | `d73ea2a1…9dc1c4d01bf` | 4 |
| `evidence-reads.json` | `4d335131…6f6393e6fc4caa0` | `b15cb2fd…9f51edf8ef9` | 14 |

### Two stale digests found and repinned

Both were literals, and a literal pin proves only that somebody typed it once.

- `crates/core/release-manifest.toml` and
  `crates/core/frontend-contract-extensions.toml` pinned the interaction fixture
  at `78e8993f…` / `46db05ab…`. `interaction-closed-loop.json` was edited twice
  after that pin was written (commits `77b540a0` and `2d88628e`) and is now
  `a6f1c436…` with view `c43d9fe3…`. Both files, and the corpus tables in
  `docs/core-0.3-compatibility{,.zh-CN}.md` (which carried a third, different
  pair), are repinned to the bytes on disk.
- `apps/gui/release-manifest.toml` pinned `extension_fixture_sha256` at
  `f96ba30c…`, which matches no file in the corpus today;
  `d1-main-cockpit.json` is `05ac2590…`.

Both guards now **derive** the digest rather than compare a literal:
`crates/core/tests/frontend_contract_v1.rs` recomputes the interaction fixture's
bytes and replayed view and the four extension fixtures' three values, and
`apps/gui/tests/architecture_boundary.rs` recomputes the digest of the canonical
fixture the manifest itself names and asserts every `required_ids` entry exists
in the corpus. A moved fixture now fails the release record.

## The Real Task

### What was attempted, and through which client

The brief called for the task to be driven through the **native GUI window**.
That was not possible in this environment, and the reason is not a tooling gap:

- The Tauri app built and launched. `target/release/bundle/macos/Viden.app`,
  `CFBundleShortVersionString` `0.1.0-rc.4`, `CFBundleIdentifier`
  `dev.viden.gui`, arm64, ad-hoc linker-signed with no TeamIdentifier, executable
  38,946,048 bytes. Launched with `VIDEN_HOME` pointed at a scratch directory, it
  ran and registered a real window: `CGWindowListCopyWindowInfo` reported window
  `25172`, owner `Viden`, 1254×784 at (237, 112).
- The host Mac's screen was **locked**. `CGSessionCopyCurrentDictionary`
  returned `CGSSessionScreenIsLocked = 1`. `screencapture -x -l 25172` and
  `screencapture -x -R 237,112,1254,784` both answered "could not create image";
  a full-screen `screencapture -x` produced an all-black frame.
  `CGPreflightScreenCaptureAccess()` returned `true`, so the Screen Recording
  permission is granted and the black frame is the lock, not the permission.
- Unlocking the Mac requires entering the user's password, which is not
  something this run may do. Keystrokes were deliberately **not** sent while the
  screen was locked, because a locked session routes them to the login window.

So the task was driven through the **TUI**, which is the other client of the same
Core contract and needs no display. The GUI half of the flow — DiffReview,
EvidenceView, the D2 dock — is therefore covered by this batch's deterministic
headless captures under `apps/gui/evidence/`, not by a live window.

### Fixture

A fresh temporary Git repository under the run's scratch directory, one commit
(`Add the fixture README`), a `README.md`, and a `.viden/config.toml` selecting
`provider = "fallback"` / `model = "test-local"` — which is where a project
selects its provider (`crates/config/src/lib.rs`, `config_paths`); `viden.toml`
is repository policy for gates and permissions, not provider selection. The
repository's own `.viden/` was never touched.

The `fallback` provider turns a user message of the form
`tool <name> key=value …` into a real tool call
(`crates/provider/src/fallback.rs`, `parse_explicit_tool_call`), which is how a
mutation happens with no live model.

### Step by step

| # | Step | Result | Capture |
| --- | --- | --- | --- |
| 1 | Intake | The TUI opened the fixture repository as the workspace and resolved `fallback` / `test-local` from the project config. `0 lanes`, version `v0.3.4`. | [`01-welcome.txt`](tui/01-welcome.txt) |
| 2 | New Lane | `/lanes` published a Core `lane_create` approval, risk **Medium**, target the workspace root, input naming the branch and worktree path, with `1 Allow once` / `2 Allow for session · unavailable` / `3 Add repo allowlist` / `4 Deny` and an `auto-deny @… · default Deny` expiry. | [`02-lane-create-approval.txt`](tui/02-lane-create-approval.txt) |
| 3 | Lane created | `Allow once` created the Lane, its branch `viden/lane_…`, and its worktree under `.worktrees/`. Verified out of band: `git worktree list` shows both, `git branch` shows the Lane branch. Route `main→side-1`, state `Draft`. | [`03-lane-created.txt`](tui/03-lane-created.txt) |
| 4 | Composer edit | `tool edit_file path=README.md old=Fixture new=Edited` produced a Core `edit_file` approval, risk **Medium**, pinned in the transcript with its audit id. | [`04-edit-approval-pinned.txt`](tui/04-edit-approval-pinned.txt) |
| 5 | **C1 decision context** | The approval detail rendered Core's typed hunks, not the tool input: `README.md  Modified  +1 -1`, `@@ -1,5 +1,5 @@`, per-line old/new numbering with `- # E1 Fixture` / `+ # E1 Edited`, unchanged context rows, and the base note `computed against 65f130d7`. This is the whole point of GUI-CORE-012: the operator approved a **diff**, not a string. | [`05-edit-approval-hunks.txt`](tui/05-edit-approval-hunks.txt) |
| 6 | Edit applied | `Allow once` applied it. Verified out of band: `README.md` line 1 is now `# E1 Edited` and `git status --short` reports ` M README.md`. | — |
| 7 | Check run | **Not run.** See the defects below: after one completed turn the composer stops submitting, so no second tool call could be issued in the same session, and the GUI's own check-run trigger was unreachable behind the locked screen. |  — |
| 8 | Stage / commit / push | **Refused before anything was sent.** `/git` over the dirty workspace rendered `TARGET workspace · main · ahead 0 behind 0 · dirty` with all four rows disabled and labelled `no workspace owner · GUI-CORE-027`. Activating a row sent no command, which is exactly what T1a specified. | [`06-git-picker-refused.txt`](tui/06-git-picker-refused.txt) |
| 9 | Push refused / accepted | **Not reached.** The refusal at step 8 is upstream of both. No `OperatorGitActionFinished` was produced, so `Completed`, `Failed { NoUpstream }`, and `Failed { RemoteUnreachable }` were **not** observed live in this run; they exist only as the canonical `operator-git.json` replay. The bare `origin` remote was consequently never added. | — |
| 10 | EvidenceView | `/evidence` answered honestly over a real empty archive: `SCOPE whole archive · oldest first`, `FILTER every kind Core returns`, `No evidence in this scope.`, `LOADED 0 · archive complete`. This is the expected GUI-CORE-028 state: the session's transcript is full of evidence and the durable archive holds none. | [`07-evidence-empty.txt`](tui/07-evidence-empty.txt) |
| 11 | Audit | The audit timeline answered `SCOPE project timeline · newest first`, `Core published no audit record for this scope.`, `LOADED 0 · nothing older matches` — after an approval that displayed an audit id on screen. No audit JSONL exists in the fixture's runtime state. | [`08-audit-empty.txt`](tui/08-audit-empty.txt) |

Every capture above is a `tmux capture-pane` frame of the running TUI, taken at
the moment described, and every one was read before it was listed. The capture
host's scratch path is replaced by a same-width placeholder so the frames stay
column-aligned; nothing else in them is edited.

### Exact Core outcome variants observed

- `ApprovalRequestView` with `decision_context` carrying a `DiffDocument`:
  one file, one hunk, `+1 -1`, `base_sha256` present — observed live.
- `ApprovalDecision` allow-once on both approvals, followed by the effect —
  observed live.
- `EvidencePage` empty with `complete` true — observed live.
- `AuditPage` empty with nothing older — observed live.
- Client-local refusal `no workspace owner · GUI-CORE-027`, with no command
  dispatched — observed live.
- `OperatorGitOutcome::{Completed, Failed}` — **not observed live.** Fixture
  replay only.

## Defects And Gaps

None of these were fixed in E1. Each is reproduced, not inferred, and each is
written into `docs/core-0.3-compatibility.md` as a numbered open follow-up.

1. **A session-level queued follow-up is never executed** (Core, blocking).
   `RuntimeCommand::QueueFollowUp` pushes onto
   `SessionEngine::queued_runtime_inputs` and nothing ever removes or runs it;
   the only `InputDequeued` producer is the Lane worker's own queue.
2. **The TUI composer stops submitting for the rest of a session** (TUI,
   blocking). `command_for_composer` queues whenever
   `state::runtime_has_active_work` is true, and that is true when a completed
   built-in turn's text still sits in `assistant_stream`, when any Lane is in
   `Draft` (which is where Core leaves a starter Lane), or when `queued_inputs`
   is non-empty — which, by defect 1, is forever. Observed both ways: after one
   fallback turn, and after creating one Lane. The GUI's own composer predicate
   is owner-scoped on `turn_id` and Agent-session status and deliberately
   excludes Lane lifecycle state, so the GUI is not affected by this half; the
   two clients disagree about what "busy" means.
3. **The TUI `/git` picker can only ever reach the workspace target** (TUI). The
   Lane selection is bound to the lane-detail overlay focus, and leaving that
   overlay to reach the composer — where `/git` is typed — clears it. Combined
   with GUI-CORE-027, `runtime.operator_git` is unreachable from the TUI in
   practice.
4. **An approval's audit id is not a durable audit record** (Core). The durable
   timeline is appended only by trust-loop and operator-git actions, so an
   approved and applied native tool mutation leaves no audit row while showing
   an audit id on screen.
5. **The durable evidence archive is empty for supervisor-driven work** (Core).
   Decided and deferred to `0.3.4` as **GUI-CORE-028**.
6. **The EvidenceView report can state `verified` beside a content answer of
   `HashMismatch`** (GUI, cosmetic but misleading). Two different facts, no
   sentence relating them.

### What this means for plan goal 4

`docs/release-0.3.3-plan.md` goal 4 is "one real local-first development task
completes through the GUI, from intake to a committed change, with audit and
evidence recorded". The honest statement is:

- **Met** for intake, Lane creation under a Core approval, and a mutation
  approved against Core's typed decision context and applied to a real file —
  through the TUI rather than the GUI.
- **Not met** for the committed change: `runtime.operator_git` was refused
  before any command was sent.
- **Not met** for archived evidence (GUI-CORE-028) or for a durable audit record
  of the mutation (defect 4).
- **Not attempted** through the native GUI window, because the host's screen was
  locked.

## Boundary Statement

This is a local candidate. The candidate versions Core `0.3.6`, TUI `0.3.4`, and
GUI `0.1.0-rc.4` exist only on the local branch `claude/e1-release-evidence`,
which is based on the local integration branch `claude/int-0.3.3`. Nothing in
this document has been published, code-signed, notarized, pushed, merged,
tagged, packaged for any platform, synchronized to the Homebrew tap, or
certified against a live provider. The macOS `.app` referenced above is a local
build artifact and was not installed or distributed. The release gate's
prepublish phase was not run.
