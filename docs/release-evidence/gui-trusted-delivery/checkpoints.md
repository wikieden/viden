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
| E1 HEAD when the code gates ran | `727b87b5e6454e21e30449af24b4fa8836413103` (the Part A commit). The commits after it change only Markdown and capture files, so no code gate is stale. Following the D1 precedent, this document does not name its own commit; report the branch tip in the handoff. |
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
   **Fixed by C6 (Core), 2026-09-12; clients adopt in G7/T2.**
   `RuntimeCommand::QueueFollowUp` pushed onto
   `SessionEngine::queued_runtime_inputs` and nothing ever removed or ran it;
   the only `InputDequeued` producer was the Lane worker's own queue.
   `runtime.turn_lifecycle` publishes a terminal fact for every native turn and
   drains the session queue behind a completed one, oldest first, each entry
   announced by `InputDequeued` and run as its own bracketed turn. A failed or
   cancelled turn keeps the queue. The GUI composer and the TUI's active-work
   predicate read `active_turns` instead of display residue in G7 and T2.
2. **The TUI composer stops submitting for the rest of a session** (TUI,
   blocking). **Fixed by T1c (2026-09-10) and completed by T2 (2026-09-12).**
   T1c split the predicate and made routing owner-scoped, which removed all
   three causes below; it still needed a client-side liveness window for the
   native path, because Core published no terminal fact for one. T2 deletes that
   window: `runtime.turn_lifecycle` (C6) brackets every turn, so
   `state::composer_target_busy` reads `RuntimeViewState.active_turns` for the
   scope the input addresses and this client holds no window at all. Both
   scenarios are pinned by tests and were walked again live on 2026-09-12
   (`docs/release-tui-0.3.4-source-control-parity.md`). The original finding
   follows. `command_for_composer` queues whenever
   `state::runtime_has_active_work` is true, and that is true when a completed
   built-in turn's text still sits in `assistant_stream`, when any Lane is in
   `Draft` (which is where Core leaves a starter Lane), or when `queued_inputs`
   is non-empty — which, by defect 1, is forever. Observed both ways: after one
   fallback turn, and after creating one Lane. The GUI's own composer predicate
   is owner-scoped on `turn_id` and Agent-session status and deliberately
   excludes Lane lifecycle state, so the GUI is not affected by this half; the
   two clients disagree about what "busy" means.
3. **The TUI `/git` picker can only ever reach the workspace target** (TUI).
   **Fixed by T1c (2026-09-10) and completed by C5 plus T2 (2026-09-12), with
   one Core-side gap open.** T1c separated `lane_detail_open` from
   `focused_lane`, added the `Esc` rung, and made `runtime.operator_git`
   reachable for a Lane. T2 adopts `runtime.workspace_owner` (C5) so the
   workspace target is sent under the owner Core publishes, and the picker's
   TARGET row reads each target's own source. The gap: `apps/cli` bootstraps the
   engine directly rather than through `LocalCoreHost::open_workspace`, the only
   production caller of `SessionEngine::bind_workspace_owner`, so a `viden` TUI
   session still sees `workspace_owner` absent and shows the refusal — recorded
   as compatibility follow-up 11 and needed by E2. The GUI is unaffected,
   because its adapter opens through the host. The original finding follows. The
   Lane selection is bound to the lane-detail overlay focus, and leaving that
   overlay to reach the composer — where `/git` is typed — clears it. Combined
   with GUI-CORE-027, `runtime.operator_git` is unreachable from the TUI in
   practice.
4. **An approval's audit id is not a durable audit record** (Core).
   **Fixed by C7 (Core), 2026-09-12; clients adopt in G7/T2.** The durable
   timeline was appended only by trust-loop and operator-git actions, so an
   approved and applied native tool mutation left no audit row while showing an
   audit id on screen. `RespondToApproval` now appends one `AuditRecord` before
   `ApprovalResolved`, under that exact pre-minted id, as actor `Operator` with
   action `approval.<allow_once|allow_session|allow_repo|deny>` and objects
   naming the approval request, the tool, and the job the decision released, so
   `QueryAudit` resolves the id a client was already being shown.
5. **The durable evidence archive is empty for supervisor-driven work** (Core).
   **Fixed by C7 (Core), 2026-09-12; clients adopt in G7/T2.** Recorded as
   **GUI-CORE-028**, now closed on the Core side. An applied native mutation
   archives a `patch` row with canonical ContextStore bytes, an adapter-reported
   patch is canonicalized by the runtime's ingestion, and the supervisor hands
   every terminal turn batch to `SessionEngine::absorb_supervised_events`, so
   the rows reach the `runtime_projection` the archive is rebuilt from. A
   restart test replays the row and serves its verified bytes.
6. **The EvidenceView report can state `verified` beside a content answer of
   `HashMismatch`** (GUI, cosmetic but misleading).
   **Fixed by H2, 2026-09-12.** Two different facts, no sentence relating them.
   The report now relates them in one sentence — the verdict the archive row
   recorded, and the read Core just did — and the recorded claim carries the
   design's mismatch treatment (`var(--error)` plus a strike-through, so the
   state is not colour alone) while staying on screen as the data it is.
   `failed` beside `HashMismatch` is two facts that agree and produces no
   alert; an answer echoed for another row contradicts nothing. Both arrival
   orders are covered (`apps/gui/tests/evidence_view.spec.ts`), and the capture
   is `evidence-hash-mismatch-1440x900-dark-en.png`.

### What this means for plan goal 4

`docs/release-0.3.3-plan.md` goal 4 is "one real local-first development task
completes through the GUI, from intake to a committed change, with audit and
evidence recorded". The honest statement is:

- **Met** for intake, Lane creation under a Core approval, and a mutation
  approved against Core's typed decision context and applied to a real file —
  through the TUI rather than the GUI.
- **Not met** for the committed change: `runtime.operator_git` was refused
  before any command was sent.
- **Not met at the time** for archived evidence (GUI-CORE-028) or for a durable
  audit record of the mutation (defect 4). Both Core halves were fixed by C7 on
  2026-09-12; the goal itself is re-evidenced by E2, because neither client has
  adopted them yet.
- **Not attempted** through the native GUI window, because the host's screen was
  locked.

## Native GUI Run — 2026-09-10

E1 could not drive the native window because the host's screen was locked. This
section records the rerun that could: branch `claude/e1b-native-gui` in
`.worktrees/e1b-native-gui`, HEAD `cd1d28f4a2702f17c80ba818ec963a7066d742d2` —
the same complete `0.3.3` candidate as the `claude/int-0.3.3` tip, so the
candidate line above is unchanged. The screen was verified unlocked before every
keystroke batch with `CGSessionCopyCurrentDictionary()`. It locked again during
the run, and the run stops exactly where it stopped. Nothing below is inferred
from a fixture replay.

### The application under test

`npm --prefix apps/gui run tauri -- build --bundles app` PASSED in 1 m 48 s and
produced `target/release/bundle/macos/Viden.app`:
`CFBundleShortVersionString` `0.1.0-rc.4`, `CFBundleIdentifier` `dev.viden.gui`,
`arm64`, ad-hoc linker-signed with no TeamIdentifier, executable 38,946,048
bytes. It was launched from the bundle with `VIDEN_HOME` pointed at a scratch
directory, so the repository's own `.viden/` was never read or written.

### Fixture

A fresh temporary Git repository under the run's scratch directory: one commit
`fc39694d4239b3fbad22c0968139dde55e21221b` ("Add the fixture README"), a
`README.md`, and a `.viden/config.toml` selecting `provider = "fallback"` /
`model = "test-local"`. The bare `origin` was deliberately not created, because
it is only added after the first refused push — a step this run did not reach.

### How a native Tauri window is driven, and what got in the way

These are facts about the harness, recorded because the next run needs them:

- The window's `CGWindowListCopyWindowInfo` owner name is `Viden`, but the
  Accessibility process name is **`viden-gui`**. `System Events` addressed as
  `process "Viden"` raises `-1719`; every command here targets `viden-gui`.
- `screencapture -x -o -l <window id>` frequently returns the frame from
  *before* the last state change. The command palette was open in the
  Accessibility tree while two consecutive captures still showed the previous
  screen. Every capture below was therefore taken twice and the second frame
  read; the Accessibility tree, not the screenshot, is the reliable state read.
- The `Open project folder` panel is hosted by
  `com.apple.appkit.xpc.openAndSavePanelService`, not by `viden-gui`, so
  `System Events` reports no window for it and its `Open` button cannot be
  pressed through Accessibility. It is reachable by keystroke only
  (`⌘⇧G`, the path, `Return`, `Return`).
- Synthetic mouse clicks posted with `CGEvent(... .leftMouseDown ...)` from this
  shell had no effect on the window, so pointer input was limited to
  `System Events`' own `click at`, which resolves through Accessibility.

### Step by step

| # | Step | Result | Capture |
| --- | --- | --- | --- |
| 1 | Welcome | The window opened on Welcome: `No project open`, one `Open project` action with `⌘O`, and `Recent project history is unavailable · Core adapter is not connected`. Status bar `MODE — · PERM — · CONTEXT — · EVENTS #0 · LANE — · DIAG 0× · REQ —`. | [`01-welcome.png`](native/01-welcome.png) |
| 2 | Open Project | `⌘O` opened the native `Open project folder` panel; `⌘⇧G` and the fixture path drove it to the fixture. | [`02-open-panel-goto.png`](native/02-open-panel-goto.png) |
| 3 | Panel at the fixture | The panel listed the fixture's `README.md` with `Open` enabled. | [`03-open-panel-at-fixture.png`](native/03-open-panel-at-fixture.png) |
| 4 | **Intake** | `Open` bound the workspace and Core built a supervisor for it. The cockpit shows Provider `fallback`, Model `test-local`, Mode `build`, Permission `ask`; Changes/Source `Branch override main`, the fixture path as `Worktree`, `Ahead 0`, `Behind 0`, `Dirty Clean`; the titlebar `⎇ 0 worktrees`; the status bar `MODE build · PERM ask · EVENTS #7 · LANE — · REQ 0 req / 0 err`. Verified out of band: Core wrote `session_meta` `canonical_root` = the fixture path, `work_mode` `build`, `permission_mode` `default`, `model` `test-local` into the scratch `VIDEN_HOME`. | [`04-cockpit-bound.png`](native/04-cockpit-bound.png) |
| 5 | Command palette | `⌘K` opened it. With a bound project and no Lane its `ACTIONS` group held exactly one row, `Focus the composer`; `JUMP TO` said `Cross-Lane gates and decisions are…` unavailable and `FILES` said `Files unavailable · Core publishes no workspace file inventory`. **The palette offers no Lane-creation entry**, so New Lane has to come from the activity rail. | [`05-command-palette.png`](native/05-command-palette.png) |
| 6 | New Lane | **Not reached.** Dismissing the palette was followed by the cockpit replacing its centre pane with `CONNECTING · Establishing the versioned Core connection. · Core connection pending`, and seconds later the window disappeared and the process was gone. See defect 7. | [`06-core-connection-pending.png`](native/06-core-connection-pending.png) |
| 7 | Lane selected, composer, D2 hunks, DiffReview, Stage, Commit, Push, EvidenceView, D14 audit | **Not reached.** A third launch repeated steps 1–4 successfully and stopped there: the host's screen locked (`CGSSessionScreenIsLocked = 1`) before the Lane could be created, and no keystroke may be sent to a locked session. | — |

Every capture listed was read before it was listed. The activity rail's Lane
entry point was located in the Accessibility tree during the third launch —
`AXButton` `Lanes` at (230, 248), between `Integration gate` and `Decisions` —
but it was never pressed, so nothing is claimed about what it opens.

### Exact Core outcome variants observed in the GUI

- Workspace intake: a Core supervisor built for the chosen root, with the
  durable `session_meta` facts above — observed live.
- `RuntimeViewState` with `workspace_source` `Ready`, branch `main`, ahead 0,
  behind 0, clean, and a resolved `fallback` / `test-local` environment —
  observed live.
- `ApprovalRequestView`, `ApprovalDecision`, `OperatorGitActionFinished`
  (`Completed` / `Failed { NoUpstream }` / `Failed { RemoteUnreachable }`),
  `WorkspaceDiffPage`, `EvidencePage`, and `AuditPage` — **not observed** in the
  GUI. They remain covered by the TUI run above (approval and decision only) and
  by fixture replay.

### Defects and observations from this run

Numbering continues from the list above. None was fixed here.

7. **The GUI window can disappear and the process exit while a project is
   bound** (GUI, blocking for this run).
   **Partly fixed by H2, 2026-09-12; the exit itself was not reproduced.** The
   visible half reproduces at the shell seam and the cause is a leaked
   controller, not a lost adapter: `renderD1Cockpit` registers `⌘K`, `⌘L`,
   `⌘.`, `⌘G`, `⌘E`, `⌘R`, `⌘O` and `Escape` on `window` and holds them until
   it is disposed, and `bootstrapShell` discarded its controller, so the
   pre-hydration shell kept answering those chords under the live cockpit,
   opened a second command palette, and re-rendered its own `connecting`
   projection into the same root — the project chip back at `—` and `Core
   connection pending` returning. Whichever handler ran last won the paint,
   which is why it was seen once in three launches. Every cockpit mount now
   goes through `claimRoot` (`apps/gui/src/main.ts`), so exactly one controller
   holds the chords; `apps/gui/tests/adapter_drop.spec.ts` presses the chords
   against a bound host and asserts one palette and no `Core connection
   pending`.

   The **process exit** was not reproduced, and no code path for it exists.
   What was tried: a grep of every Rust source the GUI links for
   `process::exit` / `process::abort` / `libc::exit` (the only hits are
   `apps/cli/src/main.rs` and a test-fixture program embedded as a string in
   `crates/plugin-host`, neither reachable from the desktop client); a read of
   `apps/gui/src-tauri/src/lib.rs`'s command layer and `spawn_core_event_pump`,
   whose only escape is a poisoned adapter lock and which ends its own thread
   rather than the process; a read of the palette's close path, which issues no
   Core connect (asserted); and a driven reproduction at both seams —
   `apps/gui/tests/reconnect.rs` pumps sixteen drains after a transport drop
   and asserts the adapter keeps the last view Core published, classifies
   `Disconnected`, blocks business success and offers the modelled reconnect,
   while `apps/gui/tests/adapter_drop.spec.ts` has the host refuse every read
   after the bind and asserts the cockpit stays mounted on the last facts and
   never paints the transport sentence as a Core fact. The one mechanism that
   does end the process is Tauri's own: on macOS the app exits when its last
   window closes. Why the window closed remains unexplained; the run's own
   note that the host was a shared desktop with other software running is not
   ruled out. After `claimRoot` this is worth re-driving natively in E2.

   The original observation follows, unchanged. After `⌘K` and `Escape`, the cockpit
   swapped its centre pane for `CONNECTING · Establishing the versioned Core
   connection. · Core connection pending` and the titlebar project chip fell
   back to `—`; within about ten seconds the window was gone from
   `CGWindowListCopyWindowInfo` and the process no longer existed. There is no
   entry in `~/Library/Logs/DiagnosticReports`, and the process's merged
   stdout/stderr log is empty, so this was an exit rather than a crash. Seen
   once, in the second of three launches; the third launch was still running
   when the screen locked. The frontend's own honesty is correct here — it said
   the Core connection was pending rather than showing stale facts — but a
   client that loses its adapter and then terminates loses the operator's
   session with no message.
8. **The Welcome screen does not fill the window** (GUI, cosmetic).
   **Fixed by H2, 2026-09-12.** Reproduced first: forcing the bundle's own
   cascade in the qa harness — `gui-kit.css`'s `.frame` flex column winning
   `display` over `.d1-frame`'s grid, with `.d1-body` at its initial
   `flex: 0 1 auto` — puts the content and the status bar at 540 px in a 768 px
   window, which is the reported shape. The frame-level half (`.d1-body
   { flex: 1 1 auto }`) landed with the navigation shell; H2 makes the welcome
   centre a fill chain rather than a percentage one: `.d1-main-welcome` moves
   below `.d1-main`, where it can win the tie it was silently losing, and the
   work surface becomes a one-track grid Welcome stretches into. No pixel
   height anywhere. `apps/gui/tests/welcome_fill.spec.ts` loads the real
   stylesheets in the bundle's order and asserts the computed chain at 640, 800
   and 900; nine of its sixteen cases fail against the CSS as of `25072a0a`.
   Captures: `welcome-fill-1440x900-dark-en.png` and
   `welcome-fill-1440x640-dark-en.png`.

   The original observation follows, unchanged. Its content
   and status bar stop at roughly 525 px, leaving the rest of the window empty:
   the same at an 800 px, a 900 px, and a 640 px window height, and the three
   captures are byte-identical where the window size did not change the layout.
   The bound cockpit fills the window correctly, so this is Welcome's layout,
   not the shell's.
9. **Welcome states the reason for an empty recent list as `Core adapter is not
   connected`** (GUI, wording).
   **Fixed by H2, 2026-09-12.** Welcome renders only while no workspace is
   bound, so a failed recent-work read now leads with D1's own `No project
   open` and names the next step, and keeps the host's sentence underneath as
   the diagnostic it is — neither fact hidden. An absent `runtime.recent_work`
   capability keeps its own wording, because that is a statement about Core
   either way. `Core adapter is not connected` stays the headline only on the
   bound-workspace surfaces, where the D6 connection state says so. Both
   languages; the capture is `welcome-fill-1440x900-dark-en.png`.

   The original observation follows, unchanged. At that moment the adapter is not connected
   because no project is bound, which is the normal first-run state, so the
   sentence reads as a fault where D1's own vocabulary would call it "no project
   open yet".

One observation is recorded without being called a defect, because it could not
be reproduced. The **first** launch was seen bound to
`/Users/wiki/Documents/GitHub/viden-test` — a directory this run never chose —
with a `session_meta` `canonical_root` naming it in the run's own scratch
`VIDEN_HOME`. `VIDEN_GUI_WORKSPACE` was unset, and two further launches with a
fresh `VIDEN_HOME` stayed on Welcome and never bound anything until `⌘O` was
driven. The host is a shared desktop that other software was using during the
run, so this is reported as unexplained rather than attributed to the GUI.

### What this means for plan goal 4, per surface

`docs/release-0.3.3-plan.md` goal 4 is "one real local-first development task
completes through the GUI, from intake to a committed change, with audit and
evidence recorded". Updating the statement above with this run:

- **Intake through the native GUI window — met.** The window opened, the native
  folder panel bound a real project, and Core built a supervisor whose durable
  session facts name that project.
- **Lane creation, the approved mutation, DiffReview, Stage, Commit, Push,
  EvidenceView, and the D14 audit trail through the native GUI window — not
  reached**, for the two environmental reasons in the table (an unexplained
  process exit, then a locked host screen), not for a contract or capability
  reason. E1's line "not attempted through the native GUI window, because the
  host's screen was locked" is therefore replaced by: attempted, intake reached,
  the rest not reached.
- The TUI statements above are unchanged: Lane creation under a Core approval
  and a mutation approved against Core's typed decision context are met there;
  the committed change, archived evidence, and a durable audit record are not.
- **No surface has yet carried the whole task end to end.** Goal 4 remains
  unmet, and the parts of it that are met are met on the TUI.

## Boundary Statement

This is a local candidate. The candidate versions Core `0.3.6`, TUI `0.3.4`, and
GUI `0.1.0-rc.4` exist only on the local branch `claude/e1-release-evidence`,
which is based on the local integration branch `claude/int-0.3.3`. Nothing in
this document has been published, code-signed, notarized, pushed, merged,
tagged, packaged for any platform, synchronized to the Homebrew tap, or
certified against a live provider. The macOS `.app` referenced above is a local
build artifact and was not installed or distributed. The release gate's
prepublish phase was not run.

# 0.3.4 Trusted Delivery Completion (E2)

Date: 2026-09-12

Everything above is the `0.3.3` record and is not rewritten. This part is the
`0.3.4` release step. Like the part above it describes a **local candidate
only**: nothing here is published, signed, notarized, pushed, merged, tagged,
or live-provider certified.

## Candidate Line

| Item | SHA / path |
| --- | --- |
| Base `origin/main` | `25072a0acee7a9959bfd8060721378cb3d4d5397` |
| Integration branch | `claude/int-0.3.4` at `39ed155dcc1847b915965f49626e6779b8b538d7`, 68 commits ahead of `main` |
| E2 branch / worktree | `claude/e2-release-evidence` in `.worktrees/e2-release-evidence`, created at that SHA |
| Core `0.3.7` contract checkpoint | `39ed155dcc1847b915965f49626e6779b8b538d7` — the integration tip every `0.3.4` batch had landed on, and the SHA `crates/core/release-manifest.toml` now records as `contract_implementation_checkpoint` |
| Core `0.3.7` base contract checkpoint | `5bd2b80b0953f4194d082940a7b9164c7231ca2d`, unchanged since `0.3.0` |
| TUI candidate | `0.3.5`; `min_core_version` deliberately unchanged at `0.3.4` |
| GUI candidate | `0.1.0-rc.5`; `[core].minimum_version` deliberately unchanged at `0.3.5` |
| Frontend schema | `1`, unchanged |
| Capabilities | 15 frozen base + 29 extensions |

The three lines move independently. Core moves because the `0.3.4` increment
added six capabilities (23 to 29) across C5 to C9; the clients move because
both adopted them (G7, T2).

Why the checkpoint is declared here rather than per batch: each Core batch
added its own fixture rows and left `component_version` and the checkpoint
alone, because a checkpoint named while the contract is still moving names a
contract that does not exist. If `claude/int-0.3.4` is rebased before it
merges, this checkpoint must be re-declared against the new SHA rather than
assumed to have survived.

### Frozen base fixture bytes

The nine frozen `frontend-contract-v1` base fixtures were compared three ways:
the bytes on disk in this worktree, the bytes at `git show 25072a0a:<path>`,
and the digest pinned in `apps/tui/release-manifest.toml`. All three agree for
all nine; none moved across the whole `0.3.4` increment.

| Fixture | sha256 (worktree = `25072a0a` = pinned) |
| --- | --- |
| `approval-allow-deny.json` | `a31d8c64…8700248e` |
| `context-pressure-cost-blind.json` | `dbae6f87…e270d697` |
| `d1-vertical-slice.json` | `d8dc7a14…248a71df5e` |
| `dag-blocker.json` | `98e2ad2b…bc37c85e5c7` |
| `merge-gate.json` | `d71807ab…d50365becf11` |
| `multi-lane.json` | `1a20e8ec…90788331fc5cd` |
| `plan-denial.json` | `6a04b5ef…d55d783b307` |
| `queued-follow-up.json` | `83c31272…7c6892299031` |
| `stream-tool.json` | `a097f17d…9951c9fd548a` |

### Deterministic evidence

The full gate table for this candidate — the workspace suite with
`viden-plugin-host` and `viden-agents` rerun serially, fmt, clippy, the
dependency boundary, the TUI regression and both smokes, the GUI vitest and
build, the projection capture, the Tauri bundle, and the doc pair/link checks,
with what was deliberately not run and why — is in
[release-0.3.4-report.md](../../release-0.3.4-report.md), "Gates". It is kept in
one place rather than copied here, because two copies of a gate table drift.

## Native GUI Run — 2026-09-12 (0.3.4)

**Not attempted: the host screen was locked at 14:37:21Z**, before any of this
batch's work began, and it was still locked at 14:45:30Z when the bundle was
ready. `CGSessionCopyCurrentDictionary()` returned
`CGSSessionScreenIsLocked = 1` with `kCGSSessionOnConsoleKey = 1` at both
checks. No keystroke and no click was sent to the window, and the application
was not launched to be driven, because a locked session routes input to the
login window — the same rule E1 and E1b followed, and the reason E1b stopped
where it did. Unlocking the Mac needs the user's password, which this run may
not enter.

So this section records what could be established without a display, and says
plainly what could not. Nothing below is inferred from a fixture replay, and
nothing is claimed about a surface that was not driven.

### The application under test

`npm --prefix apps/gui run tauri -- build --bundles app` PASSED in 1 m 10 s
(14:43:54Z to 14:45:04Z) and produced
`target/release/bundle/macos/Viden.app`:

| Fact | Value |
| --- | --- |
| `CFBundleShortVersionString` | `0.1.0-rc.5` |
| `CFBundleVersion` | `0.1.0-rc.5` |
| `CFBundleIdentifier` | `dev.viden.gui` |
| `CFBundleExecutable` | `viden-gui` |
| Architecture | `Mach-O 64-bit executable arm64` |
| Executable size | 40,655,440 bytes |
| Signature | ad-hoc, linker-signed, `TeamIdentifier=not set`, `Sealed Resources=none` |

The build log shows `Compiling viden-gui v0.1.0-rc.5`, so the bundle carries
this batch's version bump rather than the rc.4 one it replaced. It is a local
build artifact: it was not installed, distributed, signed with an identity,
notarized, or launched.

### What was not reached, and why

| Step | State |
| --- | --- |
| Welcome, `⌘O` bind, cockpit bound, Lane tab strip, `⌘L` New Lane, Lane under a Core approval, composer edit, D2 hunks, inline tool diff, check-run block, EvidenceView archive row, D14 audit row, queued follow-up drain, DiffReview commit with and without a Lane, push refused then accepted, dock Files tab, focus mode `⌘.`, `Esc` return | **Not attempted.** Host screen locked; see above. No native capture was taken, so this section lists no PNG. |

The GUI's own evidence for these surfaces is therefore the deterministic
harness captures the G3 to G7 batches took under `apps/gui/evidence/` and their
`EVIDENCE.md` rows, plus the vitest and projection suites — not a live window.
That is weaker evidence for the *integration*, and this document does not
pretend otherwise: the statement "the cockpit carries the whole task natively"
is **still unproven**, for the third release step running, and for an
environmental reason each time.

### The harness facts, carried forward unchanged

Recorded again because the next run needs them and nothing in this run could
re-verify or refute them:

- the window's `CGWindowListCopyWindowInfo` owner name is `Viden`, but the
  Accessibility process name is `viden-gui`; `System Events` addressed as
  `process "Viden"` raises `-1719`;
- `screencapture -x -o -l <window id>` frequently returns the frame from
  *before* the last state change, so every capture must be taken twice and the
  second frame read, with the Accessibility tree as the reliable state read;
- the `Open project folder` panel is hosted by
  `com.apple.appkit.xpc.openAndSavePanelService`, so its `Open` button is not
  reachable through Accessibility and the panel is driven by keystroke only
  (`⌘⇧G`, the path, `Return`, `Return`);
- synthetic `CGEvent` mouse clicks had no effect; pointer input must go through
  `System Events`' own `click at`.

## TUI Cross-Check — 2026-09-12

This is the run that could happen: offline, in `tmux` at 140×40, with
`cargo run -p viden-cli -- --provider fallback --model test-local` (built
binary `target/debug/viden`, version banner `v0.3.5`) against a scratch Git
repository, `VIDEN_HOME` pointed at a scratch directory. This repository's own
`.viden/` was never read or written.

It is a cross-check, not a substitute: the TUI and the GUI are two clients of
one contract, so a Core fact this run observed is a Core fact, but a GUI
*surface* this run did not touch stays unevidenced.

### Fixture

A fresh temporary Git repository under the run's scratch directory: one commit
`8b5cf4990d4818bd6089a7fa751f647aeec9fabd` ("Add the fixture README"), a
`README.md`, a `.gitignore` covering `.viden/` and `.worktrees/`, and a
`.viden/config.toml` selecting `provider = "fallback"` / `model = "test-local"`.
The bare `origin` was deliberately **not** created until after the first
refused push. The `fallback` provider turns a user message of the form
`tool <name> key=value …` into a real tool call
(`crates/provider/src/fallback.rs`, `parse_explicit_tool_call`), which is how a
mutation happens with no live model.

### Step by step

Every frame below is a `tmux capture-pane` of the running TUI, taken at the
moment described, and every one was read before it was listed. The capture
host's scratch path is replaced by a same-width placeholder so the frames stay
column-aligned; nothing else in them is edited.

| # | Step | Result | Capture |
| --- | --- | --- | --- |
| 1 | Intake | The TUI opened the fixture as the workspace and resolved `fallback` / `test-local` from the project config. `0 lanes`, version `v0.3.5`. Core minted `.viden/project.toml` `[project] id = prj_1789224423279254000` at bootstrap — the C10 binding site, visible on disk. | [`01-welcome.txt`](tui-0.3.4/01-welcome.txt) |
| 2 | **`/git` with no Lane selected** | All four rows pickable, `TARGET workspace · main · ahead 0 behind 0 · clean`. This is the row E1 saw disabled as `no workspace owner · GUI-CORE-027` and the row T2's own live check still could not enable, because `apps/cli` did not bind the owner. C5 published the identity and C10 moved the binding into the shared bootstrap; this frame is the first live TUI evidence of both. | [`02-git-picker-workspace-enabled.txt`](tui-0.3.4/02-git-picker-workspace-enabled.txt) |
| 3 | New Lane | `n` opened `NEW NATIVE LANE`; the first task description published a Core `lane_create` approval, risk **Medium**, `TARGET` the workspace root, `INPUT` naming the branch and worktree path, `AUDIT audit_1789224503187620000`, with `1 Allow once` / `2 Allow for session · unavailable` / `3 Add repo allowlist` / `4 Deny` and an `auto-deny @1789224803 · default Deny` expiry. | [`03-new-lane-overlay.txt`](tui-0.3.4/03-new-lane-overlay.txt), [`04-lane-create-approval.txt`](tui-0.3.4/04-lane-create-approval.txt), [`06-lane-create-approval-detail.txt`](tui-0.3.4/06-lane-create-approval-detail.txt) |
| 4 | Lane created | `Allow once` created the Lane, route `main→side-1`, state `Draft`. Verified out of band: `git worktree list` shows `.worktrees/lane_1789224502859401000` and `git branch` shows `viden/lane_1789224502859401000`. | [`07-lane-created.txt`](tui-0.3.4/07-lane-created.txt) |
| 5 | Lane target dropped | Two `Esc` rungs closed the detail and cleared the target, each announced as its own system row (`Cleared the Lane target … /git now names the workspace.`), and `L:` returned to `-`. The rest of the run is workspace-scoped on purpose: this is the "deliverable without a Lane" the milestone is about. | [`08-edit-approval-pinned.txt`](tui-0.3.4/08-edit-approval-pinned.txt) |
| 6 | Composer edit | `tool edit_file path=README.md old=Fixture new=Edited` produced a Core `edit_file` approval, risk **Medium**, pinned with `AUDIT audit_1789224570047389000`. The pinned panel shows the tool input, as designed. | [`08-edit-approval-pinned.txt`](tui-0.3.4/08-edit-approval-pinned.txt) |
| 7 | **C1 decision context** | The approval detail rendered Core's typed hunks rather than the tool input: `README.md  Modified  +1 -1`, `@@ -1,6 +1,6 @@`, per-line old/new numbering with `- # E2 Fixture` / `+ # E2 Edited`, four unchanged context rows, and the base note `computed against ed2e9faf`. The operator approved a diff, not a string. | [`09-edit-approval-hunks.txt`](tui-0.3.4/09-edit-approval-hunks.txt) |
| 8 | Edit applied | `Allow once` applied it. Verified out of band: `README.md` line 1 is `# E2 Edited` and `git status --short` reports ` M README.md`. | [`10-edit-applied.txt`](tui-0.3.4/10-edit-applied.txt) |
| 9 | **EvidenceView: an archived patch** | `/evidence` answered `LOADED 1 · archive complete` with `14:49:58 [patch] native session edit: README.md (+1/-1) · no lane`. This is the row E1 could not produce and T2 could not either; GUI-CORE-028 and E1 defect 5 are now evidenced live, not by fixture replay. | [`11-evidence-archive.txt`](tui-0.3.4/11-evidence-archive.txt) |
| 10 | **Canonical bytes** | Opening the row showed `ID patch-tool_1789224570013118000`, `OWNER workspace=ws_d449023423e1a290 project=prj_1789224423279254000`, `SOURCE native`, `PATH README.md`, a `CANONICAL` item/bundle reference, `HASH d46176df`, `PRODUCER native · coder · task turn_1789224569891144000`, `APPROVAL audit audit_1789224570047389000`, `RECORD Core verified the canonical reference: verified` and `DIFF verified against d46176df` above Core's parsed rows. Verified out of band: the ContextStore blob is 132 bytes and its sha256 is `d46176df6e9abd082f0d16ec270ff0d20e158c42ef67fc450b2b163c86fd95cb` — the hash on screen is the hash of the bytes on disk. One labelling defect here; see defect 10. | [`12-evidence-detail-canonical.txt`](tui-0.3.4/12-evidence-detail-canonical.txt) |
| 11 | **D14: the approval as a durable audit row** | The audit timeline answered `SCOPE project timeline · newest first`, `14:49:58 approval.allow_once ✓ permission:approval_1789224570047386000`, `LOADED 1 · nothing older matches`. Verified out of band in `.viden/workflows/projects/*/audit.jsonl`: actor `operator`, action `approval.allow_once`, objects `permission:approval_…`, `tool:edit_file`, `job:tui-7`, outcome `success`, under `audit_1789224570047389000` — the id the approval had already shown. E1 defect 4 is closed for this path; one gap remains, defect 11. | [`13-audit-approval-rows.txt`](tui-0.3.4/13-audit-approval-rows.txt) |
| 12 | Stage | `/git` over the dirty workspace, `Stage all changes` → a Core `git_add` approval, risk **Low**, `TARGET git_add (workspace)`. `Allow once` staged it; `git status --short` reports `M  README.md`. | [`14-git-picker-dirty.txt`](tui-0.3.4/14-git-picker-dirty.txt), [`15-stage-approval.txt`](tui-0.3.4/15-stage-approval.txt), [`16-staged.txt`](tui-0.3.4/16-staged.txt) |
| 13 | **Commit** | `Commit…` opened the message prompt; the message produced a Core `git_commit` approval, risk **Medium**, `TARGET git_commit (workspace)`. `Allow once` committed: `Commit completed · main · ahead 0 behind 0 · clean · OUTPUT 2 lines · [main b9f393e] Edit the fixture README heading AUDIT audit_1789224929400408000`. Verified out of band: `b9f393e Edit the fixture README heading`, working tree clean. **`OperatorGitOutcome::Completed` observed live for the first time**, with no Lane selected, under the Core-published workspace owner. | [`17-commit-message-prompt.txt`](tui-0.3.4/17-commit-message-prompt.txt), [`18-commit-approval.txt`](tui-0.3.4/18-commit-approval.txt), [`19-committed.txt`](tui-0.3.4/19-committed.txt) |
| 14 | **Push refused** | `Push` → a Core `git_push` approval, risk **High**. `Allow once` produced `Push failed · this branch has no upstream · push again with set upstream · the current branch has no upstream branch; push with set_upstream to create one AUDIT audit_1789224970080956000`. **`OperatorGitOutcome::Failed { NoUpstream }` observed live for the first time.** No remote existed at this point. | [`20-push-refused-noupstream.txt`](tui-0.3.4/20-push-refused-noupstream.txt), [`21-push-noupstream-outcome.txt`](tui-0.3.4/21-push-noupstream-outcome.txt) |
| 15 | Bare `origin` added | `git init --bare` under the scratch directory and `git remote add origin`, out of band, after the refusal and not before. | — |
| 16 | **Push accepted** | `Push completed · main · ahead 0 behind 0 · clean · OUTPUT 2 lines · To …/e2-origin.git AUDIT audit_1789225169647993000`. Verified out of band: the bare `origin` now holds `b9f393e`, and the local branch reads `## main...origin/main` with no divergence. **`OperatorGitOutcome::Completed` for a push observed live.** One honest qualification: the branch's upstream tracking ref had to be configured out of band, because the TUI picker offers no set-upstream control — see defect 12. The push itself was performed by Core under an operator approval, and it moved a real commit into a real remote. | [`22-git-picker-after-remote.txt`](tui-0.3.4/22-git-picker-after-remote.txt), [`23-push-approval.txt`](tui-0.3.4/23-push-approval.txt), [`24-push-accepted.txt`](tui-0.3.4/24-push-accepted.txt) |
| 17 | **A queued follow-up drains** | A second `tool edit_file` left the turn open on its approval: the composer switched to `[^J Queue]` and the status row to `ACTIVE`, both from Core's `active_turns` rather than from display residue. A follow-up submitted there was accepted, and it ran only when the first turn ended. Verified out of band in the session JSONL: `turn_owner` cleared at `1789225244` (the first turn's `TurnFinished`), `turn_owner` set again in the same second, then the user message `the queued follow-up for E2` and its assistant reply, then `turn_owner` cleared again. The prompt waited about 39 seconds and ran as its own bracketed turn behind a completed one — C6's session-queue drain, which T2's own live check could not show. One seam observation, defect 13. | [`25-followup-queued.txt`](tui-0.3.4/25-followup-queued.txt), [`26-followup-drained.txt`](tui-0.3.4/26-followup-drained.txt) |
| 18 | Archive and timeline after the run | `LOADED 2 · archive complete` (the second patch is `+0/-0`, because the fallback tool-input parser splits on spaces and the second edit resolved to a no-op). The audit timeline holds twelve rows: four `approval.allow_once`, and an `authorized`/`completed`-or-`failed` pair for each of `source.stage`, `source.commit` and the two pushes, with `14:56:10 source.push ✗ … attempt=audit_1789224970080956` beside its authorization. | [`27-evidence-two-patches.txt`](tui-0.3.4/27-evidence-two-patches.txt), [`28-audit-timeline-full.txt`](tui-0.3.4/28-audit-timeline-full.txt), [`29-audit-timeline-oldest.txt`](tui-0.3.4/29-audit-timeline-oldest.txt) |

### Exact Core outcome variants observed live

- `WorkspaceRuntimeOwnerBound` on the `apps/cli` path, with the minted
  `.viden/project.toml` id — observed live (C5 + C10).
- `RunOperatorGitAction { target: Workspace }` authorized under that owner with
  no Lane selected — observed live (C5).
- `OperatorGitOutcome::Completed` for `stage`, `commit` and `push` — observed
  live. `OperatorGitOutcome::Failed { NoUpstream }` — observed live. Before
  this run all three existed only as `operator-git.json` replay.
- `ApprovalRequestView` with `decision_context` carrying a `DiffDocument`
  (one file, one hunk, `+1 -1`, `base_sha256` present) — observed live.
- `ApprovalDecision` allow-once on five approvals, each followed by its
  effect — observed live.
- `EvidencePage` **non-empty**, with a `patch` row whose canonical reference
  Core verified and whose bytes hash to the value on screen — observed live
  (C7).
- `AuditPage` **non-empty**, with the `approval.*` row for the decision that
  released the mutation — observed live (C7).
- `TurnStarted` / `TurnFinished` bracketing a native turn, and the session
  queue drained behind a completed one — observed live through their durable
  `turn_owner` facts (C6).
- `RemoteUnreachable` — **not observed.** The bare `origin` is a local path, so
  no unreachable-remote case arose; it remains fixture replay only.
- Every GUI surface — **not observed.** Host screen locked.

## Defects And Gaps Found By E2

Numbering continues from the nine above. Each was reproduced, not inferred, and
each is located in the source. None was fixed in E2: this is a release-evidence
step, and fixing a Core behaviour here would invalidate the checkpoint it just
declared.

10. **An archived patch's rendered diff calls the file a rename** (Core,
    cosmetic but misleading). The evidence row's own `PATH` says `README.md`,
    and the rows under it say `after  Renamed  +1 -1` / `renamed from before`.
    `render_diff` (`crates/tools/src/files.rs:148`) writes placeholder
    `--- before` / `+++ after` headers and no `@@` line — deliberately, and its
    doc comment says so. Both other readers of that output stamp the path they
    actually resolved over the placeholder:
    `crates/runtime/src/decision_context.rs:110-116`, whose comment states the
    rule ("the tool input is the authoritative source for both; the rendered
    header is not"), and `crates/runtime/src/frontend_services.rs:1224-1227`.
    `crates/runtime/src/evidence_reads.rs:147` does not, so Core publishes the
    archived document with the placeholder paths and every client renders a
    rename that never happened. Confirmed against the bytes on disk: the
    132-byte ContextStore blob begins `--- before` / `+++ after`. The bytes,
    the hash, the verification verdict and the `PATH` row are all correct; only
    the derived per-file label is wrong. Closing it means stamping the entry's
    path onto the document in `evidence_reads.rs`, the way the approval path
    already does.
11. **A lane-lifecycle approval decision writes no durable audit row** (Core,
    blocking for audit completeness). This is E1 defect 4's exact shape,
    surviving C7 on the other of the two approval paths. `RespondToApproval`
    checks `lane_supervisor.pending_approval_owner` first
    (`crates/runtime/src/runtime_supervisor.rs:1138-1160`) and, for an approval
    the lane supervisor owns, forwards it as
    `SupervisorMessage::LaneApprovalResponse` and **returns before**
    `ApprovalAuditLog::record_decision` at `:1253`. The
    `LaneApprovalResponse` arm (`:1853-1894`) emits `CommandAccepted` and no
    audit record, and `crates/lanes` has no audit writer at all — correctly,
    since audit is runtime-owned policy that is injected. Reproduced: the
    `lane_create` approval displayed `AUDIT audit_1789224503187620000` on
    screen, five approvals were allowed in this session, and the durable
    timeline holds four `approval.allow_once` rows — the missing one is
    `lane_create`. An operator who follows that receipt finds nothing.
12. **The `NoUpstream` recovery has no TUI control** (TUI). Core's refusal says
    "push again with set upstream", and the contract states the same recovery
    (`crates/types/src/source_control.rs:225`: "`NoUpstream` -> offer
    `set_upstream`"). The `/git` picker offers exactly four rows and its push
    row hard-codes `set_upstream: false`
    (`apps/tui/src/tui/modal.rs:1163-1167`), so the step Core names is
    unreachable from the client that showed the refusal. E2 had to configure
    the tracking ref out of band to evidence an accepted push, which is
    recorded in step 16 rather than hidden.
13. **A session follow-up queued during a turn is never observably pending**
    (Core/TUI seam, product gap — nothing on screen is false). The composer
    offered `[^J Queue]`, the prompt was genuinely queued, and it ran 39
    seconds later behind the completed turn. Throughout that wait
    `RuntimeViewState.queued_inputs` stayed empty and the composer rendered
    `composer.active` ("Type next prompt while Viden works…") rather than
    `composer.queued`, so the operator got no indication their prompt was in
    line. The mechanism is documented where it is caused: the supervisor is one
    worker, so a `QueueFollowUp` sent while a turn runs waits in the
    supervisor's channel rather than in Core's queue
    (`crates/runtime/src/runtime_supervisor.rs:1596-1605`), reaches
    `runtime_contract.rs:793` only once the worker is free, and emits
    `InputQueued` and `InputDequeued` in the same instant. So Core is honest —
    it has not accepted a queue entry yet — and the client is honest about what
    Core published, but the `queued_inputs` surface both clients built on C6 is
    unreachable for the session scope on the native path. This is reported as a
    product gap, not as a contract break.

Two harness notes, which are about the driver and not about the client. Three
`USER` rows read `i/lanes`, `i/evidence` and `iconfirm the push outcome`,
because the driver sent an `i` while the composer was already in Insert mode —
the same artifact T2's live check recorded. And `/lanes` on a workspace with no
Lanes opens an empty board rather than a creation flow; `n` in Normal mode is
the entry point, which is what step 3 used.

## What This Means For Plan Goal 4

`docs/release-0.3.3-plan.md` goal 4 — "one real local-first development task
completes through the GUI, from intake to a committed change, with audit and
evidence recorded" — carried into `0.3.4` as goal 6. Per surface:

- **Through the TUI — met, end to end, for the first time.** Intake, Lane
  creation under a Core approval, a mutation approved against Core's typed
  decision context and applied to a real file, an archived `patch` row with
  canonical bytes Core verified, a durable `approval.allow_once` audit row, a
  staged change, a **commit**, a push refused as `NoUpstream`, and a push
  accepted into a real remote — all of the source-control half with no Lane
  selected, under the workspace owner Core published. Every E1 "not met" is now
  met on this surface, and the two pieces `0.3.3` was missing structurally
  (GUI-CORE-027, GUI-CORE-028) are evidenced live rather than by replay.
- **Through the native GUI window — not attempted.** The host screen was locked
  before this batch began and never unlocked. E1 could not drive the window;
  E1b reached intake and stopped; E2 did not start. The cockpit's own surfaces
  are covered by deterministic harness captures from G3 to G7, which is
  evidence about rendering and not about the integration.
- **The honest aggregate:** the task is now proven end to end on one client of
  the contract, and the contract capabilities it needs are proven live. What is
  still unproven is that the **GUI cockpit** carries it. Goal 4 is therefore
  **met on the contract and on the TUI, and unproven on the GUI**, and the
  remaining gap is environmental rather than a capability gap. A run with an
  unlocked screen is the only thing it needs.

## Boundary Statement

This is a local candidate. The candidate versions Core `0.3.7`, TUI `0.3.5`,
and GUI `0.1.0-rc.5` exist only on the local branch
`claude/e2-release-evidence`, based on the local integration branch
`claude/int-0.3.4`. Nothing in this part has been published, code-signed,
notarized, pushed, merged, tagged, packaged for any platform, synchronized to
the Homebrew tap, or certified against a live provider. The macOS `.app`
referenced above is a local build artifact and was not installed or
distributed. The release gate's prepublish phase was not run; packaging,
notarization, Homebrew and live-provider certification are `0.3.5` scope by the
`0.3.4` plan's own scope amendment.

## Native GUI Run — 2026-09-13 (0.3.4, taken on main 75e2a84a)

This is the run the `0.3.4` release step could not take. Branch
`claude/evidence-e3a` in `.worktrees/evidence-e3a`, base `main` at
`75e2a84ade9f2363732aa10332cc666ff366d029` — code-identical to the `0.3.4`
release commit `edcc7b3d`; only plan documents differ between them, so what is
proven below is proven about what `0.3.4` shipped. The host screen was
unlocked and `CGSessionCopyCurrentDictionary()` was read before every keystroke
and click batch; every line is recorded in the batch report and
`CGSSessionScreenIsLocked` was `0` at every one of them. Nothing here is
inferred from a fixture replay, and no defect below was fixed: this is an
evidence step.

It is a **partial** run, and the partition matters. The whole source-control
half of the real task — stage, commit and push, each under its own Core
approval, with no Lane selected — ran end to end through the cockpit for the
first time. The Lane half did not run at all, because **no Lane can be created
in this build** (defects 14 and 15), which closes the composer, the tool
approval, the inline tool diff, the check-run block, the archived patch and the
queued follow-up behind it.

### The application under test

`npm --prefix apps/gui ci --no-audit --no-fund` added 97 packages in 9 s;
`npm --prefix apps/gui run tauri -- build --bundles app` PASSED in 1 m 12 s
(05:19:10Z to 05:20:2xZ) and produced
`target/release/bundle/macos/Viden.app`:

| Fact | Value |
| --- | --- |
| `CFBundleShortVersionString` | `0.1.0-rc.5` |
| `CFBundleIdentifier` | `dev.viden.gui` |
| `CFBundleExecutable` | `viden-gui` |
| Architecture | `Mach-O 64-bit executable arm64` |
| Executable size | 40,655,440 bytes |
| Bundle size | 39 MB |
| Signature | ad-hoc, linker-signed, `TeamIdentifier=not set`, `Sealed Resources=none` |

It was launched from the bundle, never installed, signed with an identity,
notarized or distributed. Every launch ran with `VIDEN_HOME` pointed at a
scratch directory, so this repository's own `.viden/` was never read or
written. Launched PIDs, in order: 93235, 9284, 59134 (the long run), 86319,
3918, 47443.

### Fixture

A fresh temporary Git repository under the run's scratch directory: one commit
`c593aefa2f9fed571e2caef713e9f0b39e6b7f2e` ("Add the fixture README"), a
seven-line `README.md`, a `.gitignore` covering `.viden/` and `.worktrees/`,
and a `.viden/config.toml` selecting `provider = "fallback"` /
`model = "test-local"`. Core minted `.viden/project.toml`
`[project] id = "prj_1789277024425237000"` at bootstrap — the C10 binding site,
visible on disk, and the id every approval below carries.

### Two deviations from the scripted task, and why

Both are consequences of defects found in this run, and both are named here
rather than hidden in a step row.

- **The working-tree change was made out of band.** The task's mutation was to
  be a composer `tool edit_file …` inside a Lane. The GUI composer is
  Lane-scoped (`Message the selected Lane` is disabled, with
  `Create or select a Lane to start.` under it) and no Lane can be created
  (defects 14, 15), so the edit was made with `sed` in the fixture and the
  cockpit was driven over the resulting dirty tree. Everything downstream of
  the edit — Core's typed decision context, the approvals, the outcomes and
  the audit rows — is therefore real GUI-driven Core behaviour over a real
  change, but the *authorship* of the change is not the product's.
- **The bare `origin` was added before any refused push.** The TUI run reached
  `OperatorGitOutcome::Failed { NoUpstream }` by pushing with no remote. In the
  cockpit the only push entry points are `Commit & Push ⇡` in the DiffReview
  commit bar and the titlebar sync chip; the chip offers a push only when Core
  samples `ahead > 0`, which needs an upstream, and `Commit & Push` raises a
  `git_commit` approval before the `git_push` one — two approvals in one
  application session, which defect 16 makes impossible. So `NoUpstream` was
  **not reached natively**, the bare `origin` and the tracking ref were created
  out of band, and the accepted push below is a real Core push of a real
  commit into a real remote.

### Step by step

Every frame was captured twice (the `screencapture` staleness carried forward
from the `0.3.3` harness notes) and the second frame kept; the Accessibility
tree was read as the state of record for every assertion, and every PNG listed
was viewed before it was listed.

| # | Step | Reached | Result / reason | Frames |
| --- | --- | --- | --- | --- |
| 1 | Welcome | yes | `No project open`, one `Open project` action with `⌘ O`, `Recent projects appear once a project is open.` / `Core adapter is not connected`. Status bar `MODE — · PERM — · CONTEXT — · EVENTS #0 · LANE — · DIAG 0× · REQ —`. | [`01`](native-0.3.4/01-welcome.png) |
| 2 | `⌘O` bind | yes | `⌘O` opened the `Open project folder` panel, driven by keystroke only as the harness notes require (`⌘⇧G`, the path pasted, `Return`, `Return`). The panel listed the fixture's `README.md`, 118 bytes, with `Open` enabled. | [`02`](native-0.3.4/02-open-panel-at-fixture.png) |
| 3 | **Cockpit bound** | yes | The cockpit came up on the fixture: dock Environment `Provider fallback`, `Model test-local`, `Mode build`, `Permission ask`; Local `Branch main`, the fixture path as `Worktree`, `Ahead 0`, `Behind 0`, `Working tree Clean`; status bar `MODE build · PERM ask · EVENTS #7 · LANE — · REQ 0 req / 0 err`. Verified out of band: Core wrote `session_meta` `canonical_root` = the fixture path, `work_mode` `build`, `permission_mode` `default`, `model` `test-local` into the scratch `VIDEN_HOME`. | [`03`](native-0.3.4/03-cockpit-bound.png) |
| 4 | **Workspace owner visible with no Lane** | yes | `LANE —`, and the dock's `Commit or push` section renders `Open the commit bar` **enabled** (`AXEnabled=true`) while the titlebar sync chip is enabled with the help string `0 ahead, 0 behind the upstream, as Core last sampled it.` Neither falls back to `D1-OPERATOR-GIT-NO-WORKSPACE-OWNER`, so C5's `runtime.workspace_owner` is published and consumed — GUI-CORE-027 closed on the GUI, live. | [`04`](native-0.3.4/04-workspace-owner-commit-bar.png) |
| 5 | `⌘L` New Lane | partly | `⌘L` opened the New Lane popover with `Viden Agent built-in` selected, the three ACP adapters greyed under `Checking ACP agents…`, the task pasted, and the derived branch `⌁ vd/edit-the-fixture-readme-he`, isolation `local`. | [`05`](native-0.3.4/05-new-lane-popover.png) |
| 6 | **Lane created under a Core approval** | **no** | `Create lane ↵` never became enabled. `Checking ACP agents…` stood for more than six minutes in two separate application sessions, and the Accessibility tree read `enabled=false` throughout, with the built-in agent selected. `⌘⏎` did nothing. No `lane_create` approval was ever raised. See defect 14. | [`06`](native-0.3.4/06-new-lane-create-blocked.png) |
| 7 | D4 `Full setup…` as the second creation path | **no** | The wizard opened full-window on step 1 with Core's read-only resolution (`Coder`/`Reviewer`/`Tester`, Lane ID, `Branch override`, the carried task with the note that it is not part of the create command, `Worktree` and `Base revision` as `Core resolves this on preview`). `Preview with Core` was pressed; no `StarterLanePreviewed` fact arrived in 80 s and `Create reviewed Lane` stayed disabled. See defect 15. | [`07`](native-0.3.4/07-d4-wizard-step1.png), [`08`](native-0.3.4/08-d4-preview-unresolved.png) |
| 8 | Composer `tool edit_file …`, D2 hunks for that edit, inline tool diff, check-run block | **no** | All four are Lane-scoped in this client; the composer is disabled without a Lane and says so. Not reachable behind steps 6 and 7. | — |
| 9 | EvidenceView before the run | yes | Opened inside the cockpit chrome with the design's filter chips (`All`/`Patch`/`Test result`/`Review`/`Doc update`/`Release artifact`), search, `No evidence in this scope.` and `Archive complete — Core published every entry matching this scope.` An honest empty archive, not a silent one. | [`09`](native-0.3.4/09-evidenceview-empty-before.png) |
| 10 | DiffReview over the dirty tree | yes | After the change was made out of band, the view's own re-read control refreshed Core's sample: `1 files · +1 −1`, `README.md`, `@@ -1,4 +1,4 @@`, `-# E3a Fixture` / `+# E3a Edited`, and the dock's `Changes` section listing `M README.md +1 −1`. | [`10`](native-0.3.4/10-diffreview-dirty.png) |
| 11 | **Stage under a Core approval** | yes | `Stage all` → `Stage is waiting for approval in the permission dock.` D2 showed `Approve git_add prj_1789277024425237000 · pending · low`, `TOOL INPUT PREVIEW all: true`, `EVIDENCE Core published no evidence for this decision.`, and `Decision and reason are written to audit audit_1789279167519670000` over `Approve once` / `Deny`. `Approve once` staged it: `git status --short` moved from ` M README.md` to `M  README.md`. | [`11`](native-0.3.4/11-stage-waiting-approval.png), [`12`](native-0.3.4/12-stage-approval-d2.png), [`13`](native-0.3.4/13-stage-applied.png) |
| 12 | **`OperatorGitOutcome::Completed` for a stage** | yes | The commit bar rendered `Stage completed · main ↑0 ↓0 · uncommitted changes remain` with a `▶ Git output` disclosure, `LANE —`. | [`14`](native-0.3.4/14-stage-completed.png) |
| 13 | D2 after the decision | **defect** | The decided `git_add` card stayed `pending · low` with `1 awaiting you` and `Sent. Waiting for Core to record the verdict.`, and the `git_commit` approval raised next never appeared. See defect 16. | [`15`](native-0.3.4/15-d2-stale-after-decision.png) |
| 14 | **D14 shows the `approval.allow_once` audit row** | yes | The Audit Timeline rendered `approval.allow_once operator success · permission · approval_1789279167519659000 · tool · git_add · job · gui-git-2d695ebc-… · scope=allow_once` beside its `source.stage` authorized/completed pair, under `Core reports the audit timeline is complete.` Verified out of band in the scratch `workflows/projects/*/audit.jsonl`: the same three rows, same ids. | [`16`](native-0.3.4/16-d14-audit-stage.png) |
| 15 | **Dock Files tab opens a file** | yes | The `Files` tab listed Core's workspace inventory (`.gitignore` 20, `README.md` 117) and the inspector's `Open file` read it: `workspace root · 117 bytes on disk · sha256 f1d3490374e3…` over the file's text. Verified out of band: the file is 117 bytes and its sha256 is `f1d3490374e339e9f46c273d44757462cf8c0818c5cd54269b6488646bd96bdf`. C9's `runtime.workspace_file_reads` consumed live. | [`17`](native-0.3.4/17-dock-files-open.png) |
| 16 | **Focus mode `⌘.` and `Esc`** | partly | The `Focus mode ⌘.` control hides the context dock and gives the centre the full width; `Esc` returned from the Audit Timeline to the Conversation view with the chrome retained and focus mode still on, and the transcript carried a durable `PERMISSION approval_… · decision allow_once` row with `Open audit trail`. The `⌘.` chord itself did not fire while focus sat in the dock's file list; the control did. Recorded as an observation below rather than as a defect, because a synthetic keystroke's focus target cannot be proven from here. | [`18`](native-0.3.4/18-focus-mode.png), [`19`](native-0.3.4/19-esc-return.png) |
| 17 | **Commit with no Lane, under a Core approval, with C1's typed decision context** | yes | In a fresh application session the `git_commit` approval appeared correctly: `Approve git_commit prj_1789277024425237000 · pending · medium`, `TOOL INPUT PREVIEW message: … path: …`, and **Core's typed hunks rather than the tool input** — `README.md`, `@@ -1,4 +1,4 @@`, `-# E3a Fixture` / `+# E3a Edited` with per-line numbering. `Approve once` committed. Verified out of band: `a5d3d53 Edit the fixture README heading`, working tree clean. **`OperatorGitOutcome::Completed` for a commit observed live in the GUI, with `LANE —`.** | [`20`](native-0.3.4/20-commit-approval-hunks.png), [`21`](native-0.3.4/21-commit-completed.png) |
| 18 | Push refused as `NoUpstream` | **no** | Not reachable; see the second deviation above. A second commit, `2d0a30d`, was made the same way through `Commit & Push ⇡`, and its `git_push` approval was then lost to defect 16 — the commit bar read `Push is waiting for approval in the permission dock.` while D2 still showed the decided `git_commit`. | — |
| 19 | **Push accepted** | yes | With the bare `origin` and the tracking ref in place and one local commit ahead, the titlebar sync chip read `Push 1 local commits. Pull is excluded by contract; this client pushes and fetches only.` and was enabled. It raised `Approve git_push prj_1789277024425237000 · pending · high`, `remote: origin`, audit `audit_1789281559244376000`, over `Approve once` / `Approve for repo paths` / `Deny`. `Approve once` pushed: the commit bar rendered `Push completed · main ↑0 ↓0 · working tree clean`. Verified out of band: the bare `origin` holds `70bda55` and the local branch reads `## main...origin/main` with no divergence. **`OperatorGitOutcome::Completed` for a push observed live in the GUI, with `LANE —`.** | [`22`](native-0.3.4/22-sync-chip-ahead.png), [`23`](native-0.3.4/23-push-approval-d2.png), [`24`](native-0.3.4/24-push-completed.png) |
| 20 | **D14 after the run** | yes | `Audit trail 15`, `Outcomes on the loaded page · 15 success 15`: five `approval.allow_once` rows (two `git_add`, two `git_commit`, one `git_push`), each carrying its own `gui-git-…` job id, and an `authorized`/`completed` pair for each of the two stages, the two commits and the push. Verified byte for byte against the scratch `audit.jsonl`. | [`25`](native-0.3.4/25-d14-audit-full.png) |
| 21 | EvidenceView after the run | yes | Still `No evidence in this scope.` over `Archive complete`, and `0 entries` in the search field's own words. Correct: C7 archives a `patch` for a *native tool* mutation, and no tool mutation could be made. | [`26`](native-0.3.4/26-evidenceview-empty-after.png) |
| 22 | Queued follow-up drains while a turn runs | **no** | A turn needs a Lane in this client. Not reachable behind steps 6 and 7. | — |

### Exact Core outcome variants observed live in the GUI

- `WorkspaceRuntimeOwnerBound` on the Tauri host path, with the minted
  `.viden/project.toml` id — observed live (C5 + C10).
- `RunOperatorGitAction { target: Workspace }` authorized under that owner with
  **no Lane selected** — observed live, three times (stage, commit, push).
- `OperatorGitOutcome::Completed` for `stage`, `commit` **and** `push` —
  observed live in the GUI. `Failed { NoUpstream }` and `RemoteUnreachable` —
  **not observed**; see the second deviation.
- `ApprovalRequestView` with `decision_context` carrying a `DiffDocument` (one
  file, one hunk, `+1 −1`) on the `git_commit` approval — observed live. The
  `git_add` and `git_push` approvals carried the opaque input preview and said
  so in Core's own words with `GUI-CORE-012`.
- `ApprovalDecision` allow-once on five approvals, each followed by its effect
  on disk — observed live.
- `AuditPage` **non-empty**, with the `approval.*` row for every decision that
  released a mutation and its `source.*` pair — observed live (C7).
- `WorkspaceFileRead` with a verified digest — observed live (C9).
- `EvidencePage` — observed live and **empty**, which is the truthful answer
  for this run rather than evidence of C7's archive.
- `StarterLanePreviewed`, `StarterLaneCreated`, `TurnStarted`/`TurnFinished`,
  `InputQueued`/`InputDequeued`, `ToolCallStarted`, `EvidenceRecorded` — **not
  observed.** No Lane could be created.

### Defects And Gaps Found By E3a

Numbering continues from the thirteen above. Each was reproduced, not inferred.
None was fixed here.

14. **The New Lane popover can never create a Lane while ACP adapter discovery
    is outstanding** (GUI, blocking). `create.disabled` ORs in `model.probing`
    (`apps/gui/src/components/agent_menu.ts:277-282`), and `model.probing` is
    `discoveryIsProbing()` = `agentDiscoveryStarted && !agentDiscoveryComplete`
    (`apps/gui/src/screens/d1_cockpit.ts:3046-3048`). Discovery walks every
    adapter whose `startability` is `probe_required`
    (`advanceAgentDiscovery`, `:3644-3673`) and each probe waits for Core's
    `AgentAdapterProbed` business fact
    (`apps/gui/src-tauri/src/adapter.rs:588-591`). On a host with none of
    Codex, Claude Agent or Kiro CLI installed, no `AgentAdapterProbed` arrived:
    the diagnostic stayed `Checking ACP agents…` for over six minutes in two
    separate application sessions and `Create lane ↵` read `enabled=false`
    throughout — **including with the built-in `Viden Agent` selected, which
    needs no probe at all.** The gate is an ACP concern applied to every
    creation. This is the single defect that makes the Lane half of the real
    task unreachable in the shipped `0.3.4` GUI.
15. **The D4 starter-Lane wizard's `Preview with Core` never resolves behind
    that stuck probe** (GUI, blocking). `Full setup…` reaches D4 with the task
    carried over, but `Preview with Core` produced no `StarterLanePreviewed`
    fact in 80 s and `Create reviewed Lane` stayed disabled, so the second
    creation path is closed by the first one's failure. The mechanism is one
    process-wide command slot: `PendingD1` holds the unresolved probe
    (`apps/gui/src-tauri/src/adapter.rs:507-509`) and the webview's own
    dispatchers bail while `pendingCommandId` is set
    (`apps/gui/src/screens/d1_cockpit.ts:3599-3612`, `:3644-3656`). A stuck
    probe therefore does not degrade one menu, it stops Lane creation
    altogether. Closing 14 without also bounding this pending command would
    leave the same shape behind any other unanswered command.
16. **The Decision Center keeps a decided approval as the only pending one and
    never surfaces the next** (GUI, blocking for any two-decision task). After
    `Approve once` on the `git_add` approval Core applied the effect — the file
    staged on disk, the audit rows written, the commit bar rendering
    `Stage completed` — while D2 kept the same card at `pending · low`, kept
    `1 awaiting you`, and kept the footer `Sent. Waiting for Core to record the
    verdict.` The `git_commit` approval raised next never appeared, although
    DiffReview said `Commit is waiting for approval in the permission dock.`
    Navigating away to DiffReview and back, and toggling the `Gate approvals`
    filter, did not refresh it; only restarting the application did, and in a
    fresh session whose first decision had not yet been made the next approval
    rendered correctly. Reproduced twice (`git_add` then `git_commit`;
    `git_commit` then `git_push`). The practical effect is one approval per
    application session, which is why steps 17 and 19 each needed their own
    launch.
17. **The source-control in-flight state never clears after a completed
    commit** (GUI). With `Commit completed · main ↑0 ↓0 · working tree clean`
    on screen, `Commit & Push ⇡` stayed disabled with the tooltip
    `Waiting for the permission decision.` and the titlebar sync chip stayed
    disabled with `A source-control action is already running.`, unchanged
    30 s later. Nothing on screen is false about the commit; the client's own
    busy flag is stale, and it makes push unreachable in that session.
18. **The application terminates itself mid-session** (GUI, blocking for any
    long native run). Five times in this batch the process exited with status
    `0`, no stderr beyond the two AppKit TSM/IMK lines, and no macOS crash
    report: 05:32:09Z, 05:34:06Z, 06:10:50Z, 06:27:03Z, 06:36:29Z. It happened
    both while a Core command was pending and roughly seventy seconds after a
    clean bind with nothing pending, and once between two Accessibility reads
    that both succeeded. Each exit discarded the session's Core state and cost
    a rebind. This is the same *shape* as E1b's defect 7 — the window
    disappearing and the process being gone — now with a clean exit status
    rather than a visible `CONNECTING` state, so it is recorded separately
    rather than folded into it.
19. **The Lane tab strip overlaps its own metadata when the context dock is
    open** (GUI, cosmetic). With no Lanes the strip draws `No Lanes yet`, the
    project path and `Build` on top of one another. In focus mode, where the
    centre pane has the full window width, the same strip renders cleanly, so
    it is a width-budget defect in the strip's meta slot rather than a missing
    truncation rule.
20. **The dock's `Changes` total and `Local` working-tree flag disagree with
    the file list beside them** (GUI, honesty). Immediately after Core
    re-sampled the workspace diff, the dock listed `M README.md +1 −1` under a
    `Lines` total still reading `+0 −0`, with `Working tree Clean` below it.
    The three come from different Core samples and the client renders them
    without reconciling or dating them, so the panel states two contradictory
    facts at once.
21. **DiffReview's changed-file list is not re-sampled after a commit** (GUI,
    cosmetic). With `Commit completed · main ↑0 ↓0 · working tree clean` on
    screen, `CHANGES` still read `1 files · +1 −1` and still drew the hunk that
    had just been committed.

### The four expected divergences, reproduced or not

`0.3.4` left defects 10 to 13 open and E3a was to confirm each natively.

- **10 — an archived patch's rendered diff calls the file a rename.** *Not
  reproduced; not reachable.* The archive is empty in this run because C7
  archives a `patch` for a native *tool* mutation and no tool mutation could be
  made (defects 14, 15). Nothing here contradicts it either; it remains open on
  the TUI evidence.
- **11 — a lane-lifecycle approval decision writes no durable audit row.**
  *Not reproduced; not reachable.* No `lane_create` approval could be raised,
  so the lane-supervisor approval path was never exercised. What this run does
  show is the other path behaving: all five runtime-path approvals
  (`git_add`, `git_commit`, `git_push`) wrote their `approval.allow_once` row,
  which is consistent with defect 11 being specific to the lane path.
- **12 — the `NoUpstream` recovery has no client control.** *Not reproduced on
  the GUI; the refusal was never put on screen* (second deviation). The
  observation the run can support is narrower and is offered as such: neither
  GUI push entry point — `Commit & Push ⇡` and the titlebar sync chip — offers
  a set-upstream option, and the chip is only enabled once Core samples
  `ahead > 0`, which already presupposes an upstream. Whether Core's
  `push again with set upstream` sentence is rendered with a control behind it
  is untested here.
- **13 — a session follow-up queued during a turn is never observably
  pending.** *Not reproduced; not reachable.* A turn needs a Lane in this
  client, so there was no running turn to queue behind.

### Observations, not defects

- The `⌘.` chord did not toggle focus mode while keyboard focus sat in the
  dock's file list, and `⌘L` opened the New Lane popover in one session and not
  in another; the `Focus mode ⌘.` control and the `＋ New Lane` control both
  worked every time. A synthetic keystroke's focus target cannot be established
  from outside the webview, so these are recorded rather than numbered.
- The host's active input source is a Pinyin IME, which mangles synthetic
  `keystroke` text (`Edit the fixture README heading` arrived as
  `的Ei天他和fixture READMEheading`). Text entry in this run was done by
  clipboard paste, which bypasses the IME; the input source was switched to
  `ABC` for one stretch and restored afterwards. The next native run should
  paste rather than type.
- The `Open project folder` panel's go-to field silently truncated a long
  pasted path once and completed it correctly on the retry; the panel remains
  keyboard-only through Accessibility, as recorded in the `0.3.3` harness
  notes, and those notes otherwise held unchanged.
- The Accessibility process name is still `viden-gui` while the window owner is
  `Viden`, `screencapture -l` still returns stale frames, and the main window
  must be selected by window *name* when the open panel is up — three windows
  can be on screen at once and the first `layer=0` hit is the sheet.

### What This Means For Plan Goal 4, Per Surface

`docs/release-0.3.3-plan.md` goal 4, carried into `0.3.4` as goal 6 — "one real
local-first development task completes through the GUI, from intake to a
committed change, with audit and evidence recorded":

- **Through the TUI — met, end to end**, unchanged from the 2026-09-12 record
  above.
- **Through the native GUI window — partly met, and the unmet half is a
  capability gap rather than an environmental one.** Intake, the Core-published
  workspace owner with no Lane, Core's typed decision context on a real
  approval, three approvals decided and applied, `OperatorGitOutcome::Completed`
  for a stage, a **commit** and an accepted **push**, the durable
  `approval.allow_once` rows in D14, a verified workspace file read in the dock,
  focus mode and the `Esc` return path — all driven through the cockpit for the
  first time. What the cockpit could **not** do is create a Lane, and with it
  the agent turn, the tool approval, the inline tool diff, the archived patch
  and the queued follow-up.
- **The honest aggregate:** the source-control half of goal 4 is now proven on
  the GUI as well as the TUI; the Lane half is proven on the TUI and
  **blocked** on the GUI by defects 14 and 15. This is no longer "unproven for
  an environmental reason" — the screen was unlocked, the window was driven,
  and the client stopped. Goal 4 is therefore **met on the contract and on the
  TUI, and partly met on the GUI with a named, reproducible blocker**, which is
  a better-understood position than `0.3.4` left it in and a worse one than
  `0.3.4` hoped for.
