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
    **Closed by C12 07d5afe5** (compatibility follow-up 14).
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
    **Closed by C12 da3dd5d0** (compatibility follow-up 15).
12. **The `NoUpstream` recovery has no TUI control** (TUI). Core's refusal says
    "push again with set upstream", and the contract states the same recovery
    (`crates/types/src/source_control.rs:225`: "`NoUpstream` -> offer
    `set_upstream`"). The `/git` picker offers exactly four rows and its push
    row hard-codes `set_upstream: false`
    (`apps/tui/src/tui/modal.rs:1163-1167`), so the step Core names is
    unreachable from the client that showed the refusal. E2 had to configure
    the tracking ref out of band to evidence an accepted push, which is
    recorded in step 16 rather than hidden.
    **Closed by T3 3a4ce963** (2026-09-13). Once Core has answered
    `Failed { NoUpstream }` for a target, the picker appends a `Push and set
    upstream` row that sends `Push { remote, set_upstream: true }` — armed
    from Core's echoed target and action in
    `apps/tui/src/tui/operator_git.rs:172` and rendered in
    `apps/tui/src/tui/modal.rs:1182`. It is never offered before that answer,
    it carries the remote the refused push carried, and a push Core reports as
    completed for that target removes it. No Core change was required: the
    command field already existed. Step 16's out-of-band qualification stands
    as the record of *that* run.
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
    **Closed by C12 8972b0da** (compatibility follow-up 16).

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

## Defects 14–21 — H4 (2026-09-13, on `claude/gui-native-defects-h4`)

Branch `claude/gui-native-defects-h4` in `.worktrees/gui-native-defects-h4`,
base `claude/int-0.3.5` at `67353a71` (= `main` `75e2a84a` + the `0.3.5`
contract design, H3, C12, DS, C13, T3 and this document's E3a section). The
host screen was unlocked and `CGSessionCopyCurrentDictionary()` was read before
every keystroke and click batch; `CGSSessionScreenIsLocked` was `0` at every
one of them.

This section records the **verdict per defect** and a native re-run of the
steps E3a was blocked on. It is not the `0.3.5` native proof: that is E3b's,
on the release candidate, and the Lane half beyond creation is still owed.

### The application under test

`npm --prefix apps/gui ci --no-audit --no-fund` added 97 packages in 2 s.
`npm --prefix apps/gui run tauri -- build --bundles app` PASSED and produced
`target/release/bundle/macos/Viden.app`:

| Fact | Value |
| --- | --- |
| `CFBundleShortVersionString` | `0.1.0-rc.5` (H4 does not bump the GUI version) |
| `CFBundleExecutable` | `viden-gui` |
| Architecture | `Mach-O 64-bit executable arm64` |
| Executable size | 41,496,368 bytes |
| Bundle size | 40 MB |
| Signature | ad-hoc, `TeamIdentifier=not set`, `Sealed Resources=none` |

Every launch ran with `VIDEN_HOME` pointed at a scratch directory, so this
repository's own `.viden/` was never read or written. The fixture is a fresh
temporary Git repository with one commit `06a79a3` ("Add the fixture README"),
a six-line `README.md`, a `.gitignore` covering `.viden/` and `.worktrees/`,
and a `.viden/config.toml` selecting `provider = "fallback"` /
`model = "test-local"`.

**One deviation, named rather than hidden.** The workspace was bound with
`VIDEN_GUI_WORKSPACE` rather than through `⌘O` and the native folder panel.
E3a already proved that path end to end (its steps 2 and 3); H4's subject is
what happens *after* a bound cockpit, and the launch binding is a documented
entry point (`apps/gui/README.md`, "Run the desktop client locally"). Nothing
below depends on how the workspace was bound.

### Verdicts

| # | Defect | Verdict |
| --- | --- | --- |
| 14 | New Lane can never create a Lane while ACP discovery is outstanding | **Fixed** (`ee2ce04f`). `create.disabled` no longer ORs the discovery state; the gate is the selected adapter's own startability. Proven natively below: a Lane created on the built-in agent on a host with no ACP binary |
| 15 | D4's `Preview with Core` never resolves behind a stuck probe | **Fixed on the client** (`ee2ce04f`) and **reported as a Core finding** (`GUI-CORE-035`). The client abandons an unanswered adapter *read* so its own command slot is free; Core's single command worker is still held by the probe it is running, which only Core can fix |
| 16 | The Decision Center keeps a decided approval and never surfaces the next | **Fixed** (`ee2ce04f`). Two halves: the decision now waits for Core's `ApprovalResolved` through the permission pipeline, and leaving a centre view for DiffReview or EvidenceView drops the pane and its mount memo so returning is a new Core read |
| 17 | The source-control in-flight state never clears after a completed commit | **Fixed** (`ee2ce04f`). Root cause: every drain reconciled only its own pending command, so an `OperatorGitActionFinished` consumed by the cockpit's 250 ms poll was swallowed. All drains now funnel through `reconcile_ambient_pendings` |
| 18 | The application terminates itself mid-session | **Not reproduced** in H4's bounded effort — see below |
| 19 | The Lane tab strip overlaps its own metadata with the dock open | **Fixed** (`284c4229`). Proven natively below |
| 20 | The dock's `Changes` total and `Local` flag disagree with the file list | **Fixed** (`284c4229`). Both now read the sample the file list reads, and `data-changes-sample` states which sample that is |
| 21 | DiffReview's changed-file list is not re-sampled after a commit | **Fixed** (`ee2ce04f`). A `completed` result re-samples with one `QueryWorkspaceDiff`; a refusal or a transport failure still rides the debounced staleness path, because neither moved the tree |

### The native re-run

Every frame was captured twice and the second kept; the Accessibility tree was
read as the state of record for every assertion; every PNG listed was viewed
before it was listed.

| # | Step | Reached | Result / reason | Frames |
| --- | --- | --- | --- | --- |
| 1 | **Cockpit bound, tab strip and dock legible** | yes | Dock Environment `Provider fallback`, `Model test-local`, `Mode build`, `Permission ask`; Local `Showing Workspace root`, `Branch main`, `Working tree Clean`; statusbar `MODE build · PERM ask · EVENTS #7 · LANE —`. The zero-Lane tab strip draws `No Lanes yet`, `＋`, the project path and `Build` each in its own space with the context dock open — defect 19, natively | [`h4-01`](native-0.3.4/h4-01-cockpit-bound.png) |
| 2 | **`Create lane ↵` enabled with every ACP probe unanswered** | yes | The popover opened with the built-in `Viden Agent` selected and the three ACP adapters disabled under `Probe did not answer within 15s.` — the client's own stated bound, with a `Retry ACP check` beside it — and `Create lane ↵` read `enabled=true` once the task was pasted. Earlier in the same run the same popover read `Checking ACP agents… each probe is given up to 15s.` while `Create lane ↵` was already enabled. **Defect 14, natively.** `Checking ACP agents…` never stood open-ended | [`h4-02`](native-0.3.4/h4-02-new-lane-create-enabled.png) |
| 3 | **A Core `lane_create` approval** | yes | `Approve lane_create · lane_create`, `Medium risk`, `branch: viden/lane_1789287265044945000 path: …/.worktrees/lane_1789287265044945000`, `Reason: lane mutation policy requires approval`, `Audit: audit_1789287266911108000`, over `Y · Once` / `Shift+A · Allow repo paths` / `N · Deny`, with `Always` and `Edit` disabled and labelled `GUI-CORE-003`, and `1 awaiting you` on the status bar. **The first `lane_create` approval ever raised through this client** | [`h4-03`](native-0.3.4/h4-03-lane-create-approval.png) |
| 4 | **Lane created under that approval** | yes | `Y · Once` released the effect: the tab strip carries `lane_1789287265044945000 · coder start…`, the statusbar reads `LANE lane_1789287265044945000 — draft`, the titlebar `1 worktree`, and the dock's Local section switched to `Showing Lane worktree` with the Lane's own branch and worktree. Verified out of band: `.worktrees/lane_1789287265044945000/` exists and `git branch` lists `viden/lane_1789287265044945000`. `LANE OUTPUT` carries Core's own `Preparing worktree (new branch 'viden/lane_1789287265044945000')`. **Defects 14 and 15 are closed on the GUI** | [`h4-04`](native-0.3.4/h4-04-lane-created.png) |
| 5 | Composer `tool edit_file …` in that Lane | **no** | The composer is enabled and accepts the text, but the created Lane stands with an active turn (`Cancel active turn` on screen, `composer.busy`), and the submit did not leave: `EVENTS` stayed at `#25` across two attempts (the Send control and `Return`). Not a defect 14/15 residue — the Lane exists and Core owns it — and not diagnosed here. It belongs to E3b, which owns the full Lane half on the candidate | — |
| 6 | D2 two sequential approvals, inline tool diff, check run, EvidenceView patch, D14 `approval.allow_once` for both, queued follow-up | **no** | All six sit behind step 5 | — |

### Defect 18 — not reproduced

Bounded effort, stated exactly. The bundle was run with `RUST_LOG=debug` and
stdout/stderr captured to a file across six launches on 2026-09-13 between
07:34Z and 08:22Z, including one instance driven through bind, Lane creation
and an approval and then left idle under a five-second process sampler. No
instance exited on its own: every launch that ended did so because this batch
killed it, and the last one — the instance that created the Lane above — was sampled
every five seconds for 16 minutes 18 seconds (92 samples) and was still alive
at the end. The captured stderr held nothing but the AppKit TSM/IMK line E3a also
saw, and no macOS crash report was written for `viden-gui` in the window.

What *was* reproduced is one shape E3a described and E1b's defect 7 described
before it: **the window briefly leaves the Accessibility tree**. Three times,
immediately after a click that caused a full cockpit re-render (`Create lane ↵`
and `Y · Once`), `System Events` answered
`Can't get window 1 of process … Invalid index. (-1719)`, and a read three
seconds later found the window back with `count of windows` = 1 and the process
alive. That is consistent with "the window disappeared and the process was
gone" being *two* observations rather than one, and it is the thread the next
attempt should pull. It is recorded as an observation, not as a fix.

No fix is claimed for defect 18. It stays open, with this batch's negative
result and the re-render/AX observation attached to it.

### Honesty notes

- The frames above are from the **native app**, not from the capture harness.
  The four qa states this batch added — `h4-lane-create-builtin`,
  `h4-probe-timed-out`, `h4-d2-next-approval`, `h4-tabstrip-dock-open` — are
  reproducible harness entries for the same surfaces and are not the evidence
  for the verdicts; the native frames are.
- The native run found one defect of its own, fixed in the same commit: a
  discovery walk could stall with `Checking ACP agents…` on screen and *nothing
  outstanding*, because `advanceAgentDiscovery` is re-queued only by a drain
  and a quiet Core produces none. The bound now belongs to the walk, not to one
  probe, and `waitForCommandSlot` has a bound of its own so a creation can
  report instead of waiting silently.
- `startLane` used to overwrite the host's own refusal with
  `Core did not publish a creatable Lane preview.`, which blamed Core for a
  refusal it never issued. The native run hit exactly that and lost the reason;
  the send failure's own sentence now stands.

## Native GUI Run — 2026-09-13 (0.3.5 candidate `da4c8a6d`)

This is E3b: the full real task driven through the native cockpit on the
`0.3.5` release candidate. Branch `claude/evidence-e3b` in
`.worktrees/evidence-e3b`, base the integration branch `claude/int-0.3.5` at
`da4c8a6d0f7152691d2c9a47bb992cb409656e02` — every `0.3.5` batch through G9 and
C16. G10 had not run when this was taken, so the bundle still carries
`0.1.0-rc.5`, and the D4 agent binding is still the "pick does not travel"
state; the Lane below was therefore created with the built-in agent.

The host screen was unlocked and `CGSessionCopyCurrentDictionary()` was read
before every keystroke and click batch; `CGSSessionScreenIsLocked` was `0` at
every one of them. Nothing here is inferred from a fixture replay, and no
defect below was fixed: this is an evidence step.

It is again a **partial** run, and the partition has moved. H4 closed Lane
creation, and this run created Lanes through the cockpit three times, under a
Core `lane_create` approval, and watched that approval write its durable audit
row — the trust gap `0.3.5` goal 2 is about. What it could **not** do is start
a turn inside a created Lane: H4's unexplained step 5 reproduces exactly, and
the diagnosis below finds it is not a stalled turn at all but a command the
client believes Core still owes it an answer for (defect 22). Everything the
agent turn gates — the tool approval, the inline tool diff, the check run, the
archived patch, the queued follow-up, a proposed plan, an Agent session to
pause or kill — stays unreached.

### The application under test

`npm --prefix apps/gui ci --no-audit --no-fund` added 97 packages in 2 s
(09:49:07Z). `npm --prefix apps/gui run tauri -- build --bundles app` PASSED —
`Finished \`release\` profile [optimized] target(s) in 1m 07s` — and produced
`target/release/bundle/macos/Viden.app` at 09:50:25Z:

| Fact | Value |
| --- | --- |
| `CFBundleShortVersionString` | `0.1.0-rc.5` (G10 had not run; it owns the `rc.6` bump) |
| `CFBundleIdentifier` | `dev.viden.gui` |
| `CFBundleExecutable` | `viden-gui` |
| Architecture | `Mach-O 64-bit executable arm64` |
| Executable size | 42,746,560 bytes |
| Bundle size | 41 MB |
| Signature | ad-hoc, linker-signed, `TeamIdentifier=not set`, `Sealed Resources=none` |

It was launched from the bundle, never installed, signed with an identity,
notarized or distributed. Every launch ran with `VIDEN_HOME` pointed at a
scratch directory, so this repository's own `.viden/` was never read or
written. Launched PIDs, in order: 43643, 45136, 67729, 68885, 80397.

### Fixtures

Two fresh temporary Git repositories under the run's scratch directory, each
with one commit `2b593e1` ("Add the fixture README"), a five-line `README.md`,
a `.gitignore` covering `.viden/` and `.worktrees/`, and a `.viden/config.toml`
selecting `provider = "fallback"` / `model = "test-local"`. The second exists
because the diagnosis needed a clean `VIDEN_HOME` and a clean workspace to
separate a stuck client from a stuck Core; the two are named where they matter.
Core minted `.viden/project.toml` `prj_1789293381253952000` and
`prj_1789295636864533000` at the two bootstraps.

### Two deviations from the scripted task, and why

- **The working-tree change was made out of band again**, for the same reason
  as E3a but a different cause: the composer's `tool edit_file …` never leaves
  the client (defect 22), so the mutation was made with `sed` inside the Lane's
  own worktree and the cockpit was driven over the resulting dirty tree.
  Everything downstream — Core's typed decision context, the approvals, the
  outcomes and the audit rows — is real GUI-driven Core behaviour over a real
  change, but the *authorship* of the change is still not the product's.
- **`NoUpstream` was not reached, and neither was an accepted push.** The only
  push entry points remain `Commit & Push ⇡` and the titlebar sync chip. The
  chip needs Core to sample `ahead > 0`, which presupposes an upstream;
  `Commit & Push ⇡` raised its `git_commit` approval correctly and then lost
  the `git_push` half (defect 25). No bare `origin` was added, because there
  was no reachable push to send to it.

### Step by step

Every frame was captured twice and the second kept, and every PNG listed was
viewed before it was listed. One harness change is named in the notes below:
the frames are window-id captures rather than screen-rect captures.

| # | Step | Reached | Result / reason | Frames |
| --- | --- | --- | --- | --- |
| 1 | Welcome | yes | `No project open`, `Open project ⌘ O`, `Recent projects appear once a project is open.` / `Core adapter is not connected`. The activity rail carries eleven slots including the two `0.3.5` additions, **`Plan Studio`** and **`Agent Board`**, all disabled while unbound. Status bar `MODE — · PERM — · CONTEXT — · EVENTS #0 · LANE — · LATENCY — · TOKENS — · DIAG 0× · REQ —` | [`01`](native-0.3.5/01-welcome.png) |
| 2 | **`⌘O` bind, native panel path** | yes | `⌘O` opened the `Open project folder` panel; `⌘⇧G`, the path pasted, `Return`, `Return`, driven by keystroke only as the harness notes require. The go-to sheet resolved to `…/e3b/fixture` and the breadcrumb confirmed it before the accept | [`02`](native-0.3.5/02-open-panel-goto.png) |
| 3 | **Cockpit bound** | yes | Dock Environment `Provider fallback`, `Model test-local`, `Mode build`, `Permission ask`, `Tokens 0`; Changes `Lines +0 −0` / `No changes`; Local `Showing Workspace root`, `Branch main`, the fixture path as `Worktree`, `Ahead 0`, `Behind 0`, `Working tree Clean`; status bar `MODE build · PERM ask · EVENTS #7 · LANE — · DIAG 0× · REQ 0 req / 0 err`. Verified out of band: Core wrote `session_meta` `canonical_root` = the fixture path, `work_mode` `build`, `permission_mode` `default`, `model` `test-local` into the scratch `VIDEN_HOME`, and minted `.viden/project.toml` `prj_1789293381253952000`. **The zero-Lane tab strip draws `No Lanes yet`, `＋`, the project path and `Build` each in its own space with the context dock open — defect 19, natively** | [`03`](native-0.3.5/03-cockpit-bound.png) |
| 4 | **Workspace owner visible with no Lane** | yes | `LANE —`, and the dock's `Commit or push` renders `Open the commit bar` **enabled** (`AXEnabled=true`) while the titlebar chip reads `Fetch from the remote (0 behind). Pull is excluded by contract; this client pushes and fetches only.`, also enabled. Neither falls back to `D1-OPERATOR-GIT-NO-WORKSPACE-OWNER` — GUI-CORE-027 still closed on the GUI | [`03`](native-0.3.5/03-cockpit-bound.png) |
| 5 | `⌘L` New Lane | yes | The popover opened with `Viden Agent built-in` selected and the three ACP adapters disabled under `Probe did not answer within 15s.`, with `Retry ACP check` beside them. `Checking ACP agents…` never stood open-ended — **defect 14's bound, natively, on a host with no ACP binary** | [`04`](native-0.3.5/04-new-lane-popover.png) |
| 6 | **`Create lane ↵` enabled with every ACP probe unanswered** | yes | With the task entered, `Create lane ↵` read `enabled=true` and the derived branch became `⌁ vd/edit-the-fixture-readme-he`. **Defect 14, natively, on the candidate** | [`05`](native-0.3.5/05-new-lane-create-enabled.png) |
| 7 | **A Core `lane_create` approval, with C15's full tier list** | yes | `Approve lane_create · lane_create`, `Medium risk`, `branch: viden/lane_1789293683646574000 path: …/.worktrees/lane_…`, `Reason: lane mutation policy requires approval`, `Audit: audit_1789293687048689000`, over `Y · Once` / `Allow repo paths` / **`Shift+A · Always · scope_not_offered`** / **`E · Edit · not_editable`** / `N · Deny`, with the revision field and its sentence `Every key travels, including the ones you left alone: Core evaluates the whole input, not a patch of it.` Both new tiers are disabled carrying **Core's own refusal names** rather than `GUI-CORE-003` — **register 019 closed on the GUI, live**. Rail badge `1`, status bar `1 awaiting you` | [`06`](native-0.3.5/06-lane-create-approval.png) |
| 8 | **D2 shows it** | yes | `Decision Center`, `1 awaiting you`, `GATE APPROVALS · 1`, the card `Approve lane_create prj_… · lane_… · pending · medium`, the full detail, `View audit trail`, and the same five-tier row | [`07`](native-0.3.5/07-d2-lane-create-pending.png) |
| 9 | **D2 decides it and the queue clears** | yes | `Approve once` released the effect and the card **left the queue**: `0 awaiting you`, `GATE APPROVALS · 0`, `Core is holding no decision for you.` **Defect 16's first half, natively.** Verified out of band: `.worktrees/lane_1789293683646574000/` exists and `git branch` lists `viden/lane_1789293683646574000` | [`08`](native-0.3.5/08-lane-created-d2-cleared.png) |
| 10 | **The Lane, created under that approval** | yes | Tab strip `lane_1789293683646574000 · coder start… · vi…`, status bar `LANE lane_1789293683646574000 — draft —`, titlebar `1 worktree`, dock Local switched to `Showing Lane worktree` with the Lane's own branch and worktree, and `LANE OUTPUT` carrying Core's own `HEAD is now at 2b593e1 Add the fixture README` / `Preparing worktree (new branch 'viden/lane_1789293683646574000')` | [`09`](native-0.3.5/09-lane-created-conversation.png) |
| 11 | **The Lane-path approval writes its durable audit row** | yes | Verified byte for byte in the scratch `workflows/projects/*/audit.jsonl`: `audit_1789293687048689000`, actor `operator`, action `approval.allow_once`, outcome `success`, objects `permission:lane-approval_1789293687048688000`, `tool:lane_create`, `job:gui-d1-6df9db2e-…`, `lane:lane_1789293683646574000`, `args {"scope":"allow_once"}` — **the id the request had already shown on screen**. Reproduced in the second fixture as `audit_1789295715906249000` through the D1 permission dock instead of D2. **Compatibility follow-up 15 is closed on the lane path, live, and E1 defect 11 with it** | — |
| 12 | **Composer `tool edit_file …` in that Lane** | **no** | The composer is editable and accepts the text, and the submit **does not leave**: `EVENTS` pinned at `#25` across ten attempts in two application sessions, no message on screen, and the composer never cleared. See the diagnosis and defect 22 | [`10`](native-0.3.5/10-step5-submit-dropped.png) |
| 13 | D2 two sequential tool approvals, inline tool diff, check run, EvidenceView archived patch, D14 `approval.allow_once` for the runtime path of that edit, queued follow-up drain | **no** | All six sit behind step 12 | — |
| 14 | **Plan Studio (D15), on Core facts** | yes, empty | The rail's `plan` slot opens the centre-pane view with the chrome retained: `PLAN STUDIO`, `BUILD`, `No plan proposed`, `lane_… · no plan record`, `Core holds no plan for this owner. Ask in Plan mode and the planner writes one with update_plan; it appears here the moment the first revision is proposed.`, `Plan mode blocks file, shell, Git, memory and task mutations.`, a `Plan record` rail reading `Core holds no plan record for this owner.` with every field `—` except `owner`, and `STEPS BY ROLE · no role on any step`. **Absence drawn as absence, never an empty plan with an enabled Accept** | [`15`](native-0.3.5/15-plan-studio.png) |
| 15 | Plan mode, a proposed plan, `update_plan` in D15, Accept, Hand off refused in plan mode, Build, Hand off, D13's DAG | **no** | A plan revision comes from the planner's `update_plan` call, which needs a turn. Not reachable behind step 12 | — |
| 16 | **Agent Board (D16), on Core facts** | yes, empty | The rail's `robot` slot opens the centre-pane view with the chrome retained: `AGENT BOARD`, `Agent Board 1 lanes · 0 sessions · 0 awaiting you`, the Lane row `lane_1789295710755489000 ⎇ viden/lane_… built_in` with `no session under this Lane yet` under it, and the detail pane `No session selected / pick a session under a Lane on the left` over `With no Lane there is nothing to supervise — the detail pane stays empty rather than showing a placeholder agent.` **The board's rule 1 — a session is never detached from its Lane — and its empty states are drawn as designed** | [`16`](native-0.3.5/16-agent-board.png) |
| 17 | Pause → Paused with held input → Resume → Kill → Killed → Retry | **no** | An Agent session comes from a started turn. Not reachable behind step 12 | — |
| 18 | **DiffReview over the Lane's dirty tree** | yes | `Re-read the workspace diff` refreshed Core's sample to `1 files · +1 −1`, `README.md`, `@@ -1,4 +1,4 @@`, `-# E3b Fixture` / `+# E3b Edited in the Lane` with per-line numbering. **The dock's `Changes` total, its file list and the `Local` flag now agree — `Lines +1 −1`, `M README.md +1 −1`, `Working tree Dirty` — which is defect 20, natively** | [`11`](native-0.3.5/11-diffreview-lane.png) |
| 19 | **Stage under a Core approval, with the Lane** | yes | `Stage all` → `Stage is waiting for approval in the permission dock.` D2 showed `Approve git_add prj_… · lane_… · pending · low`, `TOOL INPUT PREVIEW all: true`, Core's own `Core exposes an opaque input preview; structured diff rows are unavailable. · GUI-CORE-012`, `EVIDENCE Core published no evidence for this decision.` and `Decision and reason are written to audit audit_1789296485837909000`. `Approve once` staged it: ` M README.md` → `M  README.md` | [`12`](native-0.3.5/12-d2-git-add-lane.png) |
| 20 | **`OperatorGitOutcome::Completed` for a stage, with a Lane** | yes | The commit bar rendered `Stage completed · viden/lane_1789295710755489000 ↑0 ↓0 · uncommitted changes remain`, and `Commit` became live the moment a message was typed. **Defect 17, natively: the in-flight flag cleared on completion** | — |
| 21 | **Commit with the Lane, under a Core approval, with C1's typed decision context** | yes | `Approve git_commit prj_… · lane_… · pending · medium`, `TOOL INPUT PREVIEW message: … path: …`, and **Core's typed hunks rather than the tool input** — `README.md`, `@@ -1,4 +1,4 @@`, `-# E3b Fixture` / `+# E3b Edited in the Lane` with context rows. `Approve once` committed. Verified out of band: `c98308e Edit the fixture README heading in the Lane`, working tree clean. **`OperatorGitOutcome::Completed` for a commit observed live in the GUI with a Lane selected** — E3a had it only with `LANE —` | [`13`](native-0.3.5/13-commit-approval-hunks.png) |
| 22 | Push refused `NoUpstream`, set upstream, push accepted | **no** | A second change was staged and committed the same way (`1be4d0b Add the push-leg line`) through `Commit & Push ⇡`, whose `git_commit` half was approved correctly. The `git_push` half then reached no decidable surface at all: the commit bar stood on `Push is waiting for approval in the permission dock.` with `Commit & Push ⇡` disabled, D2 read `0 awaiting you`, no badge appeared, and no `git_push` row was ever written to the audit store — polled for 40 s. See defect 25 | [`14`](native-0.3.5/14-push-waiting-no-approval.png) |
| 23 | **D14 audit timeline** | yes | `Audit trail 13`, `Outcomes on the loaded page · 13 success 13`, with `source.commit operator success … phase=completed` beside its `phase=authorized tool=git_commit` pair, and `approval.allow_once operator success · permission · approval_… · tool · git_commit · job · gui-git-e152f078-… · scope=allow_once`, every row carrying `lane · lane_1789295710755489000`. The `tool · lane_create` row for the Lane-path approval is on the same loaded page. Verified byte for byte against the scratch `audit.jsonl`. **Follow-up 15's row is visible in D14 as well as durable on disk** | [`18`](native-0.3.5/18-d14-audit.png) |
| 24 | EvidenceView archived patch, canonical, correct path label (follow-up 14) | **no** | Two reasons, and both are named rather than merged: C7 archives a `patch` for a native *tool* mutation and none could be made (step 12); and once the client's command slot is occupied the Evidence rail slot no longer mounts its pane at all — the control answers, the centre does not change. The second is a consequence of defect 22 | — |
| 25 | **Focus mode `⌘.`** | yes | The `Focus mode ⌘.` control hid the context dock and gave the centre the full window width with the titlebar, rail, composer and status bar all retained; the audit rows re-flowed to one line each | [`20`](native-0.3.5/20-focus-mode.png) |
| 26 | **`Esc` return** | yes | `Esc` returned from the Audit Timeline to the Conversation with the chrome retained and focus mode still on. The tab strip draws cleanly at full width | [`21`](native-0.3.5/21-esc-return.png) |

### H4's step 5, diagnosed

H4 left this open: "the created Lane stands with an active turn (`Cancel active
turn` on screen, `composer.busy`), and the submit did not leave: `EVENTS` stayed
at `#25`". Every part of that reproduces. The reading does not.

**There is no turn, and nothing is waiting on an approval.** The Lane's
durable record is `status: draft`, `task_id: null`, `active_session_ids: []`
(scratch `workflows/projects/*/lanes.jsonl`). The session JSONL for the whole
run holds `session_meta` rows and nothing else — no `TurnStarted`, no
`TurnFinished`, no tool call, in either fixture. D2 reads `0 awaiting you` and
the audit store holds no pending decision. So the Lane's initial task — the
popover's TASK field — **never became a turn on the fallback provider**, and
the client is not blocked behind an approval the Decision Center fails to show.

**`composer.busy` was false.** The label above the composer reads `Prompt`
(`d1.composer.prompt`) in every frame, never `Queue follow-up`
(`d1.composer.queue`), which is what `apps/gui/src/screens/d1_cockpit.ts:4872`
prints when `busy` is set. What is on screen is `Cancel active turn`, and that
control is drawn from `composer.canCancel`, computed as `supports_owner &&
selected_lane.is_active() && exact_binding.is_some()`
(`apps/gui/src-tauri/src/projection.rs:2163-2165`) — a predicate that never
reads `busy` or `active_turns`. A `draft` Lane with a worker binding therefore
advertises a turn that does not exist (defect 24).

**What actually happens is that the client's one command slot is never
released.** `sendComposerIntent` reaches `sendAndWait`, whose first statement is
`if (sending || pendingCommandId) return null;`
(`apps/gui/src/screens/d1_cockpit.ts:3179`) — a silent return with no message,
no composer clear and no command. `composerDispatchQueued` is not the culprit:
it would disable the Send control, and `Send message` read `enabled=true`
throughout. `sending` cannot stick, because `sendAndWait` clears it in a
`finally`. By elimination from the rendered accessibility state,
`pendingCommandId` is set and stays set.

Which command: the host's `PendingD1Kind::CreateStarterLane` reached
`accepted == true` — provable from the screen, because routing the Lane-creation
approval into the permission dock is gated on exactly that
(`apps/gui/src-tauri/src/adapter.rs:2023-2032`) and the dock rendered it. The
only thing that settles it afterwards is `StarterLaneCreated` whose receipt
matches the preview's `preview_id` and `content_sha256`
(`apps/gui/src-tauri/src/adapter.rs:610-615`); Core builds that receipt from the
same stored preview it matched (`crates/lanes/src/lane_supervisor.rs:1088-1099`),
so a value mismatch is not possible. The receipt therefore does not arrive.

Four experiments separate the client from Core, and all four point the same way:

- ten submits and one `Cancel active turn`, over more than eighty seconds, all
  dropped, with `EVENTS` pinned — so it is not a race;
- the same result with the approval decided in **D2** and with it decided in the
  **D1 permission dock** — so it is not the decision surface;
- the same result after leaving the cockpit for DiffReview and returning, which
  H4 made a fresh Core read — so a remount does not clear it, because the fresh
  controller re-reads the pending id from the host;
- reproduced in a second application session against a second fixture with a
  clean `VIDEN_HOME` — so it is not accumulated state.

Meanwhile Core is not wedged: in the same process, `QueryWorkspaceDiff`,
`QueryWorkspaceFiles`, `QueryAudit` and all four operator-git commands ran to
completion, with their effects on disk and their audit rows written. It is one
command's receipt that is missing, not Core's worker.

**This is Core-side, so it is numbered and stopped here rather than fixed**, as
this batch's authorization requires. No `crates/**` file was edited.

**Correction, 2026-09-13 (H5).** That last inference was wrong, and it is left
standing above so the reasoning can be audited. The reading — one command's
receipt missing while Core's worker keeps answering — was right; the attribution
was not. Core emits the receipt correctly on this exact path, proven by a
runtime test that goes GREEN on its first run. The receipt is lost *after* Core
publishes it, in the desktop shell's own background event pump, which drains it
without offering it to the client's D1 pending. The corrected root cause, its
RED and its fix are under defect 22 below.

### Defects And Gaps Found By E3b

Numbering continues from the twenty-one above. Each was reproduced, not
inferred. None was fixed here.

22. **A starter Lane created under an approval never settles its creation
    command, so the cockpit's one command slot stays occupied for the life of
    the process** (GUI host, blocking). **Fixed 2026-09-13 by H5**
    (`05764cb2`). E3b numbered this Core-side and stopped; the reproduction
    says otherwise. A runtime test drives the exact native path —
    `PreviewDefaultStarterLane` in a real Git workspace, the real
    `LocalLaneEffectExecutor`, a `lane_create` approval decided allow-once —
    and Core emits `StarterLaneCreated` after `LaneUpdated` with no
    invalidation before it, GREEN on the first run
    (`crates/runtime/src/tests/frontend_services_tests.rs:2331`,
    `default_starter_lane_in_real_git_workspace_settles_its_creation_receipt`).
    The RED is one layer up, at the transport. `reconcile_ambient_pendings`
    (`apps/gui/src-tauri/src/adapter.rs:6252`) — the funnel H4 introduced so
    that whichever drain consumes an envelope reconciles every correlated
    command — offered the event to every pending **except `pending_d1`**, which
    only `poll_d1` observed. In the shipped desktop shell `poll_d1` is almost
    never the drain that sees a D1 receipt: `spawn_core_event_pump`
    (`apps/gui/src-tauri/src/lib.rs:1484`) holds the bounded receive on a
    background thread and the webview reads only after the wake that pump
    emits, so `pump_events` (`adapter.rs:6277`) consumed `StarterLaneCreated`
    and the creation command was never settled. Interposing one pump drain
    between the allow-once and the screen's poll reproduces it exactly:
    `assertion \`left == right\` failed: a receipt drained by the desktop pump
    must still free the client's one command slot / left: Some("pump-create-lane")
    / right: None`
    (`apps/gui/src-tauri/src/adapter.rs`,
    `desktop_event_pump_settles_the_lane_creation_it_drains`). It also explains
    the run's own control group, which E3b recorded without being able to read:
    `QueryWorkspaceDiff`, `QueryWorkspaceFiles`, `QueryAudit` and all four
    operator-git commands kept working in the same process because every one of
    their pendings *is* in the funnel. `observe_pending_d1` now runs inside the
    funnel and records the outcome on the adapter, so the next `poll_d1` reads
    the freed slot and the confirmed or rejected outcome regardless of which
    drain saw the receipt. Plan Studio's `pending_d15` and the Agent Board's
    `pending_d16` were the last two commands still outside that funnel and had
    the identical hole, so both were closed the same way in the same batch and
    now hold their outcome on the adapter for their own next poll to read.
    Tests:
    `apps/gui/tests/h5_native_defects.rs`
    `a_receipt_drained_by_the_desktop_pump_still_frees_the_d1_command_slot`, and
    the live-Core `desktop_event_pump_settles_the_lane_creation_it_drains`
    above.
    **What the popover's TASK field does on this path**, since the run could
    not see it: nothing reaches Core with the creation. `StarterLaneRequest`
    carries no task on this base and D4 sends `agent: None`
    (`apps/gui/src-tauri/src/adapter.rs:4726`), so Core's default preview binds
    no agent and starts no session — the receipt's `session` is `None`. The
    task is held client-side in `pendingLaneStart`
    (`apps/gui/src/screens/d1_cockpit.ts:3356`) and submitted as an ordinary
    composer `SubmitUserInput` by `maybeResumeLaneStart` (`:3373`) once Core
    publishes the created Lane **and** `commandSlotAvailable()` is true. With
    the slot never freed that guard returned early on every wake, which is why
    the Lane's durable record stayed `status: draft`, `task_id: null`,
    `active_session_ids: []` and the session JSONL held no turn fact at all.
    The Lane was never stalled; its first message was never sent.
23. **A composer submit the client cannot send is dropped in silence** (GUI,
    blocking for diagnosis). **Fixed 2026-09-13 by H5** (`0296ce0b`).
    `sendAndWait` returned `null` when the slot was occupied
    (`apps/gui/src/screens/d1_cockpit.ts:3179`). `D1IntentResult` now carries
    `pending_command` (`apps/gui/src-tauri/src/d1.rs:624`) — the occupied
    pending's command id, its stable Core command name, its age since the
    client sent it, and whether the host would allow the wait to be abandoned —
    and the composer renders a `role=alert` `data-d1-slot-busy` naming the kind
    and the age, keeps its text, offers the host's own abandon only for the two
    adapter-discovery reads, and otherwise says plainly that the command
    carries an effect and its wait is never abandoned client-side.
    `PendingD1Kind::is_abandonable` (`adapter.rs:465`) is the same predicate
    `abandon_pending_probe` enforces, kept as one function so the control
    cannot be offered where the host refuses it. Tests:
    `apps/gui/tests/h5_native_defects.rs`
    `an_occupied_command_slot_publishes_its_kind_age_and_whether_it_may_be_abandoned`,
    and `apps/gui/tests/h5_native_defects.spec.ts` "defect 23 — a refused
    submit names what holds the command slot" (five cases; RED before the fix
    with `expected null not to be null` on `[data-d1-slot-busy]`).
24. **`Cancel active turn` is drawn for a Lane with no turn** (GUI, honesty).
    **Fixed 2026-09-13 by H5** (`26eff93d`). `composer.can_cancel`
    (`apps/gui/src-tauri/src/projection.rs:2172`) read
    `lane.is_active() && exact_binding.is_some()`, neither of which is a fact
    about work: an owner binding keeps its `turn_id` after the turn ends. It
    now gates on `busy` — Core's own `active_turns` entry for this Lane, or a
    running, starting or waiting-approval Agent session under its owner. The
    RED was the canonical projection assertion that encoded the old reading:
    `panicked at apps/gui/src-tauri/../tests/d1_cockpit.rs:139:5: assertion
    failed: projection.composer.can_cancel`. Tests:
    `apps/gui/tests/h5_native_defects.rs`
    `cancel_is_not_offered_for_a_bound_lane_core_is_running_no_turn_for` and
    `cancel_is_offered_once_core_reports_an_active_turn_for_the_lane`, plus the
    two render cases in `h5_native_defects.spec.ts`.
25. **The `git_push` half of `Commit & Push ⇡` reaches no decidable surface**
    (GUI, blocking for push). **Still open after H5, and it is not defect 22.**
    H5 probed the pair against live in-process Core with the desktop pump
    interposed (`apps/gui/src-tauri/src/adapter.rs`,
    `the_push_half_of_commit_and_push_reaches_a_decidable_surface`): after the
    `git_commit` half is decided and the commit lands, Core raises its own
    `git_push` approval, `operator_git` reports `awaiting_approval`, the
    permission dock carries the request with `tool_name == "git_push"`, the
    Decision Center's `pending_total` is `1`, and answering it settles the
    action with Core's typed `NoUpstream` outcome. The gate also runs before
    the upstream check (`crates/runtime/src/operator_git.rs:85` versus `:157`),
    so a missing remote cannot suppress the approval. The probe ran with a
    workspace target; the native run had a Lane selected, so the remaining
    suspect for E3c is the Lane-scoped routing —
    `GuiCoreAdapter::operator_git_owner` and
    `RuntimeProjection::permission_dock_for_owner` — not the command slot.
    The surviving shape of defect 16 for the *second* approval of a
    two-approval action stands as E3b recorded it.

    **Resolved by H8 (2026-09-14), in process and natively** — see "Defect
    25 — H8" at the end of this file. Neither suspect above was the cause:
    the push was sent and Core raised its approval every time, but the
    command's own drain consumed the `ApprovalRequested`, so the desktop
    pump never woke the webview and the dock, the Decision Center and the
    rail badge stayed on a projection from before it.
26. **Relaunching against a `VIDEN_HOME` that already holds a created Lane
    leaves the window blank** (GUI, blocking for restart). With
    `VIDEN_GUI_WORKSPACE` set and a home carrying one Lane, the process started,
    drew its window frame, put nothing in the web area — the accessibility tree
    held the window and its three title-bar buttons and no content — and burned
    21–27 % CPU for three minutes before it was killed. The same bundle with the
    same home and no Lane renders normally, and the same home with a Lane
    renders normally when the workspace is bound through `⌘O` afterwards. The
    launch-time bind against restored Lane state is the differentiator.

### The four expected divergences, reproduced or not

`0.3.4` left defects 10 to 13 open; E3a could reach none of them.

- **10 — an archived patch's rendered diff calls the file a rename.** *Not
  reproduced; not reachable.* The archive is empty because C7 archives a
  `patch` for a native tool mutation and no tool mutation could be made. C12
  closed this on the Core side; it stays unevidenced on the GUI.
- **11 — a lane-lifecycle approval decision writes no durable audit row.**
  **Reproduced as fixed.** Twice, on both decision surfaces. See step 11.
- **12 — the `NoUpstream` recovery has no client control.** *Not reproduced;
  the refusal was never put on screen.* The narrower observation E3a offered
  still holds and is unchanged: neither GUI push entry point offers a
  set-upstream option, and the chip is only enabled once Core samples
  `ahead > 0`, which already presupposes an upstream.
- **13 — a session follow-up queued during a turn is never observably
  pending.** *Not reproduced; not reachable.* There was no running turn to
  queue behind.

### Verdicts on defects 10–21, as reached

| # | Verdict on the candidate |
| --- | --- |
| 10 | Not reachable — the archive stayed empty; Core-side close from C12 unevidenced here |
| 11 | **Closed, natively**, on both decision surfaces (step 11) |
| 12 | Not reachable; E3a's narrower observation unchanged |
| 13 | Not reachable — no turn to queue behind |
| 14 | **Fixed, confirmed natively.** `Create lane ↵` enabled with all three ACP probes unanswered, built-in agent selected, on a host with no ACP binary |
| 15 | **Fixed, confirmed natively.** The bounded probe reported `Probe did not answer within 15s.` with a `Retry ACP check` beside it; creation was never gated on it. Core's own worker bound (`GUI-CORE-035`) was not exercised |
| 16 | **Half fixed, confirmed natively.** A decided approval leaves the queue and the totals move (step 9). The *next* approval of a two-approval action still never surfaces — see defect 25 |
| 17 | **Fixed, confirmed natively.** `Stage completed` left `Commit` live in the same session (step 20) |
| 18 | **Not reproduced, and one more datum.** No launched instance exited on its own; all five were killed by this batch. The window-leaves-the-accessibility-tree shape reproduced **six times**, every one of them immediately after a click that caused a full cockpit re-render (centre-view switch, approval decision, Lane selection), each answering `-1719` and each recovering with the process alive 3–10 s later. Defect 26 is a third shape in the same family and is the most tractable of them |
| 19 | **Fixed, confirmed natively.** The zero-Lane strip draws its four parts separately with the dock open (step 3) |
| 20 | **Fixed, confirmed natively.** `Lines`, the file list and `Working tree` agree across a re-sample (step 18) |
| 21 | **Fixed, confirmed natively.** The changed-file list re-sampled to `0 files · +0 −0` / `No changes in the working tree` after each completed commit |

### Observations, not defects

- **`⌘V` does not reach the Tauri webview.** The app installs no Edit menu, so
  the clipboard paste E3a's IME note prescribes works in the native open panel
  and nowhere inside the window. Text entry in this run went through the
  accessibility value setter on the focused field, which WebKit turns into a
  real DOM input event — proven by the client's own derived state reacting
  (the New Lane branch became `⌁ vd/edit-the-fixture-readme-he` the moment the
  task landed). The next native run should use that path, not `keystroke` and
  not `⌘V`.
- **Fronting the process for a capture defocuses the open panel's go-to sheet.**
  The first bind of this run went to a *previous* batch's fixture, because a
  capture taken between the paste and the `Return` moved focus and the panel
  accepted its own remembered path. The rule is narrow and absolute: take no
  frame between typing into that sheet and committing it. This is the second
  time that sheet has silently accepted something other than what was typed —
  E3a saw it truncate a long path — so verify the breadcrumb, then press
  `Return` with nothing in between.
- **The frames are window-id captures.** `screencapture -R` photographs whatever
  owns the rect, and on a multi-display host with other applications running,
  another window came over the Viden rect mid-run and one frame caught it.
  `screencapture -l<window id>` photographs the window even when it is behind
  something else; the `0.3.3` note that `-l` returns a stale frame still holds,
  so the double capture stays. `shotw.sh` in this batch's scratch directory is
  the version that should carry forward.
- **The permission dock's pinned panel overlays the Decision Center's own
  list.** In D2 the dock draws the same request over the `GATE APPROVALS` card
  it duplicates. Nothing false is shown, and both are decidable; it is noted so
  a later fidelity pass can decide whether D2 should suppress the pinned copy.

### What This Means For Plan Goal 4, Per Surface

`docs/release-0.3.3-plan.md` goal 4, carried into `0.3.4` as goal 6 — "one real
local-first development task completes through the GUI, from intake to a
committed change, with audit and evidence recorded":

- **Through the TUI — met, end to end**, unchanged from the 2026-09-12 record.
- **Through the native GUI window on the `0.3.5` candidate — further than
  `0.3.4` reached, and still short of the whole task.** Intake, the
  Core-published workspace owner, Lane creation under a Core `lane_create`
  approval with C15's full tier list and Core's own refusal names on the two
  tiers it does not offer, **the Lane-path approval's durable audit row**, the
  Decision Center clearing a decided approval, a Core-sampled Lane diff, a stage
  and a **commit** inside the Lane under their own approvals with Core's typed
  hunks, the audit timeline reading back every one of those decisions, Plan
  Studio and the Agent Board rendering their honest empty states, focus mode and
  the `Esc` return — all driven through the cockpit.
- **What the cockpit still cannot do is start a turn in a Lane it just
  created**, and with it the tool approval, the inline tool diff, the check run,
  the archived patch, the queued follow-up, a proposed plan, and any Agent
  session to pause, resume or kill. The cause is defect 22 — recorded here as
  Core-side and corrected by H5 to the GUI host's event pump; see the
  correction above and defect 22 below.
- **The honest aggregate:** the source-control half of goal 4 is proven on the
  GUI **with a Lane** as well as without one, and the trust gap `0.3.5` goal 2
  names — an approval decided on the Lane path leaving a durable audit row — is
  **closed and evidenced natively**. The agent half of goal 4 remains proven on
  the TUI and **blocked** on the GUI, by a different and better-located defect
  than the one `0.3.4` was blocked by. Exit criterion 1 is met as to the
  candidate being driven natively; it is not met as to the full task, and this
  document does not pretend otherwise.

## Native GUI Run — 2026-09-14 (0.3.5 candidate `22124903`, E3c)

This is E3c: the Lane half of the real task driven through the native cockpit
on the `0.3.5` candidate after H5, G9b, G10 and DS2. Branch
`claude/evidence-e3c` in `.worktrees/evidence-e3c`, base the integration branch
`claude/int-0.3.5` at `22124903c4e21778ad6046fd6779aecf24837bb1` — every
`0.3.5` batch through DS2, so H5's defect 22/23/24 fixes, G9b's agent binding,
G10's token-mapping fixes and DS2's light-skin token fix are all in the bundle
under test.

The host screen was unlocked and `CGSessionCopyCurrentDictionary()` was read
before every keystroke and click batch; `CGSSessionScreenIsLocked` was `0` at
every one of them, from `2026-09-14T02:06:28Z` to `2026-09-14T04:11:29Z`.

**The batch's headline: the Lane's turn is closed.** H5's fix holds natively.
The popover TASK became a turn on the fallback provider, twice, in two separate
application sessions, and everything the agent turn gates opened behind it — the
tool approval with Core's typed hunks, allow-once, the inline tool diff, the
archived patch in EvidenceView, D14 reading back the Lane-path *and* the
runtime-path `approval.allow_once`, C15's `Always` with its standing rule and
its revoke, and C16's typed `Edit` revision with the `revised_from` link. What
this run could not finish is the source-control half **with a Lane selected**,
because the commit bar refuses against a Lane that is not the selected one
(defect 30, which is E3b defect 25's named suspect), and the Plan Studio and
Agent Board flows, for the reasons named per step.

One blocker found here was a GUI client defect and was fixed in this batch under
the launch authorization, red-first, in its own commit: defect 27.

### The application under test

`npm ci` added 97 packages in 4 s. `npx tauri build --bundles app` PASSED —
`Finished \`release\` profile [optimized] target(s) in 1m 16s` — and produced
`target/release/bundle/macos/Viden.app` at `2026-09-14T02:08:21Z`:

| Fact | Value |
| --- | --- |
| `CFBundleShortVersionString` | `0.1.0-rc.5` |
| `CFBundleIdentifier` | `dev.viden.gui` |
| `CFBundleExecutable` | `viden-gui` |
| Architecture | `Mach-O 64-bit executable arm64` |
| Executable size | 42,858,160 bytes |
| Bundle size | 42 MB |
| Signature | ad-hoc, linker-signed, `TeamIdentifier=not set`, `Sealed Resources=none` |

The bundle still carries `0.1.0-rc.5`. The plan's version table targets
`0.1.0-rc.6` for this milestone and no batch has taken that bump; R1 owns it.
Recorded here because a native evidence run names the version it drove.

The bundle was rebuilt once mid-run, after the defect 27 fix, with the same
result (`Finished \`release\` profile [optimized] target(s) in 1m 03s`,
`2026-09-14T03:19:26Z`). It was launched from the bundle, never installed,
signed with an identity, notarized or distributed. Every launch ran with
`VIDEN_HOME` pointed at a scratch directory, so this repository's own `.viden/`
was never read or written. Launched PIDs, in order: 95394, 23420, 29576. All
three were killed by this batch; none exited on its own.

### Fixture

One fresh temporary Git repository under the run's scratch directory, with one
commit `5a8b132` ("Add the fixture README"), a five-line `README.md`, a
`.gitignore` covering `.viden/` and `.worktrees/`, and a `.viden/config.toml`
selecting `provider = "fallback"` / `model = "test-local"`. Core minted
`.viden/project.toml` `prj_1789351944807166000` at the bootstrap. A bare
`origin` was created in the scratch directory and never used, because no push
became reachable (defect 30).

### One deviation from the scripted task, and why

**The Lane-worktree change for the source-control half was made out of band**,
as in E3a and E3b, and for a reason this run can now state precisely: the
product's own `edit_file` *does* run, under its approval, with the Lane
selected and Core's `turn_owner` naming the Lane — and it writes into the
**workspace root**, not the Lane worktree (defect 28). The Lane worktree
therefore cannot be dirtied by the product on this candidate. Everything
downstream of the dirty tree — Core's sample, the file list, the `Local` flag —
is real GUI-driven Core behaviour over a real change; the authorship of that
one change is not the product's.

### Step by step

Every frame was captured twice by window id and the second kept, and every PNG
listed was viewed before it was listed. Frames live under `native-0.3.5/` with
the `e3c-` prefix.

| # | Step | Reached | Result / reason | Frames |
| --- | --- | --- | --- | --- |
| 1 | Welcome | yes | `No project open`, `Open project ⌘ O`, `Core adapter is not connected`; eleven rail slots including `Plan Studio` and `Agent Board`, all disabled while unbound; `MODE — · PERM — · CONTEXT — · EVENTS #0 · LANE — · LATENCY — · TOKENS — · DIAG 0× · REQ —` | [`01`](native-0.3.5/e3c-01-welcome.png) |
| 2 | **`⌘O` bind, native panel path** | yes | `⌘O`, `⌘⇧G`, the path pasted, `Return`, `Return`. The first attempt bound nothing because the keystrokes reached another application: the harness note below is new and absolute | [`02`](native-0.3.5/e3c-02-open-panel-goto.png) |
| 3 | **Cockpit bound** | yes | Dock `Provider fallback`, `Model test-local`, `Mode build`, `Permission ask`, `Tokens 0`; `Lines +0 −0` / `No changes`; `Showing Workspace root`, `Branch main`, the fixture path as `Worktree`, `Ahead 0`, `Behind 0`, `Working tree Clean`; `MODE build · PERM ask · EVENTS #7 · LANE — · DIAG 0× · REQ 0 req / 0 err`. The zero-Lane strip draws `No Lanes yet`, `＋`, the path and `Build` each in its own space with the dock open — **defect 19, natively, again** | [`03`](native-0.3.5/e3c-03-cockpit-bound.png) |
| 4 | **`⌘L` New Lane, built-in agent, TASK** | yes | `Viden Agent built-in` selected; Codex, Claude Agent and Kiro CLI disabled under `Probe did not answer within 15s.` with `Retry ACP check` beside them; `Checking ACP agents… each probe is given up to 15s.` never stood open-ended. With the TASK entered, `Create lane ↵` read `enabled=true` and the branch became `⌁ vd/edit-the-fixture-readme-he`. **Defects 14 and 15, natively, on a host with no ACP binary** | [`04`](native-0.3.5/e3c-04-new-lane-popover.png), [`05`](native-0.3.5/e3c-05-new-lane-task-set.png) |
| 5 | **A Core `lane_create` approval with C15's full tier list** | yes | `Approve lane_create · lane_create`, `Medium risk`, `branch: viden/lane_1789352091587813000 path: …/.worktrees/lane_…`, `Reason: lane mutation policy requires approval`, `Audit: audit_1789352092704825000`, over `Y · Once` / `Allow repo paths` / **`Shift+A · Always · scope_not_offered`** / **`E · Edit · not_editable`** / `N · Deny`. Both disabled tiers carry **Core's own refusal sentences**, not a `GUI-CORE-*` placeholder — **register 019, closed on the GUI, live** | [`06`](native-0.3.5/e3c-06-lane-create-approval.png) |
| 6 | **D2 shows it and decides it** | yes | `Decision Center`, `1 awaiting you`, `GATE APPROVALS · 1`, the card `Approve lane_create prj_… · lane_… · pending · medium`, `TOOL INPUT PREVIEW`, `EVIDENCE Core published no evidence for this decision.` `Approve once` released the effect and the card **left the queue**: `0 awaiting you`, `GATE APPROVALS · 0`, `Core is holding no decision for you.` **Defect 16's first half, natively** | [`07`](native-0.3.5/e3c-07-d2-lane-create-pending.png), [`08`](native-0.3.5/e3c-08-lane-created-d2-cleared.png) |
| 7 | **The Lane, created under that approval** | yes | Tab strip `lane_1789352091587813000 coder starter…`, sidebar row `lane_… Viden · draft`, `LANE lane_1789352091587813000 — draft —`, titlebar `1 worktree`, dock `Showing Lane worktree` with the Lane's branch and worktree, `LANE OUTPUT` carrying Core's own `HEAD is now at 5a8b132 Add the fixture README` / `Preparing worktree (new branch 'viden/lane_…')`. Verified out of band: the worktree exists and `git branch` lists `viden/lane_1789352091587813000`. **The sidebar row naming `Viden` is G9b's agent binding travelling; `lane-agent-bindings.jsonl` carries `agent_id: "viden"`** | [`09`](native-0.3.5/e3c-09-lane-task-became-a-turn.png) |
| 8 | **The popover TASK becomes a turn — defect 22 closed** | yes | The composer freed (`Prompt`, `Send message enabled`, **no `Cancel active turn`**), `EVENTS` advanced `#19 → #44`, `LATENCY 19ms avg 19ms`, `REQ 1 req / 0 err`. Verified byte for byte in the scratch session JSONL: `turn_owner` set with `lane_id: lane_1789352091587813000` and `turn_id: turn_1789352172234403000`, the user message `Edit the fixture README heading in the Lane`, a cost row `outcome: success` on `fallback` / `test-local`, the assistant reply, `task_updated` `status: done` / `activity: provider turn complete`, then `turn_owner` cleared. **H5's fix holds natively**, and reproduced again in the second application session for the second Lane | [`09`](native-0.3.5/e3c-09-lane-task-became-a-turn.png) |
| 9 | **Composer `tool edit_file path=README.md old=Fixture new=Edited`** | yes | The submit left, Core raised `Approve edit_file · edit_file`, `Medium risk`, `TOOL INPUT PREVIEW new: Edited old: Fixture path: README.md`, **Core's typed C1 hunks** `@@ -1,5 +1,5 @@` with `-# E3c Fixture` / `+# E3c Edited`, over `Y · Once` / `Allow repo paths` / `Shift+A · Always` (**enabled**) / `E · Edit · not_editable` / `N · Deny`. `LIVE WORK APPROVE EDIT_FILE`, `Core is working; no owner-scoped status is available. typed`, `0:40`, `Cancel turn · Esc`, and the composer label switched to `Queue follow-up` | [`10`](native-0.3.5/e3c-10-tool-approval-hunks.png) |
| 10 | **Allow once, and the inline tool diff** | yes | `Y · Once` released it. The transcript drew `▸ Modified README.md +1 −1`, expandable to `README.md`, `@@ -1,5 +1,5 @@`, `-# E3c Fixture` / `+# E3c Edited` with per-line numbering. The composer freed and `Cancel active turn` left with the turn — **defect 24, natively, in both directions** | [`11`](native-0.3.5/e3c-11-after-allow-once.png), [`12`](native-0.3.5/e3c-12-inline-tool-diff.png) |
| 11 | Check run | **no** | Core offered none for this tool call; nothing was inferred | — |
| 12 | **EvidenceView archived patch — follow-up 14** | yes | `± Patch · native lane lane_… edit: README.md (+1/-1)`, `evidence id patch-tool_1789352339666433000`, `kind patch`, `source native`, **`path README.md`**, `owner lane lane_1789352091587813000`, `canonical item ctxi_…`, `bundle ctx-main-session_…`, `source hash f4aa16edd7def41430e7e74bf07acaccd11bda1063cfa0fb38f9501b9d7f9b5c`, `producer native · coder`, `verification verified`, `canonical quality pass`, and `Archive complete — Core published every entry matching this scope.` **The path label is the file's own path, canonical and correct** | [`13`](native-0.3.5/e3c-13-evidence-archived-patch.png) |
| 13 | Defect 10 — the archived patch's rendered diff calling the file a rename | **no** | `Open in review` navigates to the working-tree DiffReview, not to the patch's own rendered diff, and the Lane worktree was clean because the edit landed in the workspace root (defect 28). Nothing was rendered, so nothing can be said about the rename shape | [`14`](native-0.3.5/e3c-14-archived-patch-review.png) |
| 14 | **D14 shows `approval.allow_once` for BOTH paths — follow-up 15** | yes | `Audit trail 2`, `Outcomes on the loaded page · 2 success 2`, `approval.allow_once operator success · permission · approval_1789352339708803000 · tool · edit_file · job · gui-d1-39b14aaf-… · scope=allow_once` beside `approval.allow_once operator success · permission · lane-approval_1789352092704824000 · tool · lane_create · job · gui-d1-dc9c6b60-… · lane · lane_1789352091587813000 · scope=allow_once`, and `Core reports the audit timeline is complete.` Verified byte for byte against the scratch `audit.jsonl`, with the ids the two requests had already shown on screen. **The Lane path and the runtime path, side by side** | [`15`](native-0.3.5/e3c-15-d14-audit-allow-once.png) |
| 15 | **Queue a follow-up while a turn runs, then see it drain — follow-up 16** | **no** | The label reads `Queue follow-up` and the submit is **refused**: `Not sent. This client is still waiting for Core's answer to submit_user_input, asked 0s ago. Your message is kept. submit_user_input carries an effect, so the wait is never abandoned here: only Core's answer records whether it ran.` After the approval was decided the composer was cleared and the kept text was never sent — no such message exists in the session JSONL. See defect 29 | [`16`](native-0.3.5/e3c-16-followup-queued.png) |
| 16 | **Defect 23's visible refusal** | yes | The alert above is that refusal, and a second shape appeared later naming `set_work_mode`. **Defect 23, closed and confirmed natively, twice** | [`16`](native-0.3.5/e3c-16-followup-queued.png), [`26`](native-0.3.5/e3c-26-set-work-mode-slot-held.png) |
| 17 | **A medium-risk request answered `Always` (C15)** | yes | `Shift+A · Always` on a second `edit_file`. The edit applied (`# E3c Requeued`) and the audit store took `audit_1789352746460919000 approval.always … args {"scope":"always"}` | [`17`](native-0.3.5/e3c-17-always-granted.png) |
| 18 | **The repeat auto-allowed** | yes, with the scope named | A *different* input (`old=Requeued new=AutoAllowed`) asked again — Core keys the standing rule on the whole tool input, so `1 awaiting you` and a fresh `audit_1789353002795519000`. The **identical** input (`old=Edited new=Requeued`) was **auto-allowed**: no request on screen, no new audit row, the turn ran to its own typed failure `edit_file could not find the target text`. C15's `Always` works and is input-scoped, which the settings panel states plainly | [`18`](native-0.3.5/e3c-18-always-not-matched-by-a-new-input.png), [`20`](native-0.3.5/e3c-20-identical-input-auto-allowed.png) |
| 19 | **The settings standing list, and the revoke (C15)** | yes for the effect, **no** for the list | `Settings · Core-owned preferences` → `Standing approvals`, `Rules an Always decision installed. They hold across sessions and repositories until revoked.`, the row `edit_file · new=Requeued old=Edited …`, `granted 1789352875 in ws_65ba50067a87fc27`, `Revoke`, `Open the audit trail`. `Revoke` then `Save` wrote `audit_1789353306646568000 approval.revoke … args {"scope":"always"}`, and the identical input **asked again**, so Core revoked it. The panel, reopened, **still lists the revoked rule with its `Revoke` button**. See defect 31 | [`19`](native-0.3.5/e3c-19-standing-approvals.png), [`21`](native-0.3.5/e3c-21-revoke-pressed-rule-still-listed.png), [`22`](native-0.3.5/e3c-22-after-save-rule-still-listed.png) |
| 20 | **An editable request revised with `E`, and the `revised_from` link (C16)** | yes | `tool shell command=echo …` raised `Approve shell · shell`, `Scope: Once, Always, Edit`, with **`E · Edit` enabled** — `shell` is in `EDITABLE_APPROVAL_TOOLS` where `edit_file` is not, and the disabled tier says so in Core's own words. `E` opened a **typed** revision surface: one `AXTextField` named `command`, value `echo`, not a raw blob. Set to `pwd` and `Send the revision`: Core raised a **new** request carrying **`This request revises approval_1789353592354218000; Core re-evaluated the narrowed input and still wants a decision.`**, and the audit store took `audit_1789353592354220000 approval.edit … args {"revised_input":"command=pwd","scope":"edit"}`. **C16's typed input and the `revised_from` link, natively** | [`23`](native-0.3.5/e3c-23-shell-editable-approval.png), [`24`](native-0.3.5/e3c-24-edit-typed-input.png), [`25`](native-0.3.5/e3c-25-revised-from.png) |
| 21 | **Plan mode** | yes, after the defect 27 fix | The work-mode pill's popover is a real popover: the first attempt held the client's one command slot for the life of the process (defect 27, fixed in this batch). On the rebuilt bundle the switch settled, `MODE plan` with **`PERM read_only`** — Core's own work-mode/permission coupling, applied as Core truth and never as a client edit — and a composer submit immediately afterwards left and ran a turn with no slot-busy alert | [`26`](native-0.3.5/e3c-26-set-work-mode-slot-held.png), [`28`](native-0.3.5/e3c-28-plan-mode-slot-free.png), [`29`](native-0.3.5/e3c-29-plan-mode-read-only.png) |
| 22 | Plan Studio: an `update_plan` proposal in D15, Accept, Hand off refused in plan mode, Build, Hand off, D13's DAG | **no** | Two named reasons. `update_plan` reached Core and Core answered with its own typed refusal — `` `plan` is not a JSON array of plan steps `` — and the well-formed JSON could not be entered, because the composer applies macOS automatic quote substitution and turns a `"` into a `”` (defect 32, with a one-line reproduction). Separately, once Core reported execution stopped for the Lane, the rail highlighted `Plan Studio` and the centre kept the recovery surface (defect 33) | [`30`](native-0.3.5/e3c-30-plan-studio-not-rendered.png) |
| 23 | Agent Board: the Lane's session rows, Pause → Paused, Resume, Kill → Killed, Retry | **no** | The built-in `Viden Agent` runs provider turns, not `runtime.agent_session_control` Agent sessions; no ACP adapter is installed on this host, so no session row could exist to pause or kill. Not a defect, and not evidence either | — |
| 24 | **DiffReview over the Lane's dirty tree** | yes | `Re-read the workspace diff` refreshed Core's sample: dock `Lines +2 −0`, `M README.md +2 −0`, `Showing Lane worktree` with the **selected** Lane's branch and worktree, `Working tree Dirty`. **The total, the file list and the `Local` flag agree — defect 20, natively** | [`32`](native-0.3.5/e3c-32-diffreview-lane.png) |
| 25 | DiffReview commit **with the Lane selected**, `Commit & Push ⇡`, the `git_push` half (defect 25), set upstream, push accepted | **no** | `Open the commit bar` is **disabled**, carrying Core's own refusal: `No commit or push can be sent from here: D1-OPERATOR-GIT-OWNER: Lane \`lane_1789352091587813000\` does not have one exact Core operator git owner` — while the status bar's `LANE` reads `lane_1789357518367069000`. The commit bar refuses against a Lane that is **not the selected one**. That is exactly the Lane-scoped routing E3b named as defect 25's remaining suspect, and it is now observed; see defect 30. No push entry point became reachable, so the bare `origin` was never used | [`32`](native-0.3.5/e3c-32-diffreview-lane.png) |
| 26 | **Defect 24 — no `Cancel active turn` on an idle Lane** | yes | Absent on every idle frame (`09`, `11`, `12`, `28`) and present only while Core reported the turn (`10`, `16`, `18`, `23`). **Closed, confirmed natively, in both directions** | [`09`](native-0.3.5/e3c-09-lane-task-became-a-turn.png), [`10`](native-0.3.5/e3c-10-tool-approval-hunks.png) |
| 27 | **Defect 26 — relaunch bound via `VIDEN_GUI_WORKSPACE` against a home holding Lanes** | yes, **not reproduced** | The process started, took about fifteen seconds to enter the Accessibility tree, and then **rendered the full cockpit**: both Lane tabs, the bound dock, `2 worktrees`, `MODE build · PERM ask · EVENTS #9`. CPU settled at 10.3 % over 55 s, not the 21–27 % burn for three minutes E3b recorded. **Defect 26 does not reproduce on this candidate** | [`35`](native-0.3.5/e3c-35-workspace-env-relaunch.png) |
| 28 | **Focus mode `⌘.` and the `Esc` return** | yes | The control hid the context dock and gave the centre the full window width with the titlebar, rail, composer and status bar retained. `Esc` returned the rail to `Conversation` with the chrome retained and focus mode still on, which is what `0.3.4` recorded | [`33`](native-0.3.5/e3c-33-focus-mode.png), [`34`](native-0.3.5/e3c-34-esc-return.png) |

### Defects And Gaps Found By E3c

Numbering continues from the twenty-six above. Each was reproduced, not
inferred. One was fixed here, under the launch authorization; the rest were
numbered and stopped.

27. **A composer control never settles its command, so the cockpit's one
    command slot stays occupied for the life of the process** (GUI, blocking).
    **Fixed 2026-09-14 in this batch** (`fix(gui): settle a composer control
    against the owner Core stamps`). `composer_control_envelope`
    (`apps/gui/src-tauri/src/adapter.rs:5155`) sends `RuntimeOwner::default()`
    on purpose — a work-mode, permission-level or model change mutates
    workspace-level runtime state, not one Lane's execution. Core's
    `stamp_workspace_identity`
    (`crates/runtime/src/runtime_supervisor.rs:5789`) then **completes** that
    bare owner with the `workspace_id` and `project_id` it published, before
    the acceptance is emitted. `PendingD1::acceptance_owner_matches`
    (`adapter.rs:633`) compared the sent owner with the stamped one for
    equality, so the pending was never `accepted`; without acceptance the
    `SnapshotUpdated` that confirms it is ignored
    (`matches_business_fact`, `adapter.rs:700-706`). The mode changed on
    screen — Core ran it — and the client waited forever for a receipt it had
    already drained. Natively: `MODE plan` with
    `Not sent. This client is still waiting for Core's answer to
    set_work_mode, asked 28s ago.` on every subsequent submit, unchanged after
    50 s. The acceptance now also matches the workspace completion Core
    stamps, mirroring Core's rule exactly and nothing wider: only the two ids
    the client left empty may differ, so a Lane-scoped envelope is still
    refused. RED, against live in-process Core:
    `assertion \`left == right\` failed: a composer control whose bare owner
    Core completed must still free the client's one command slot /
    left: Some("control-owner-mode") / right: None`, with the same run
    asserting the mode did reach `plan`. Tests:
    `apps/gui/src-tauri/src/adapter.rs`
    `a_workspace_level_composer_control_settles_against_the_owner_core_stamps`
    and
    `a_bare_pending_accepts_the_stamped_workspace_owner_and_refuses_a_lane_owner`.
    The two adapter-discovery reads send the same bare owner and had the same
    hole; they were masked because `PendingD1Kind::is_abandonable` releases
    them.
28. **A tool called in a Lane-owned turn runs against the workspace root, not
    the Lane worktree** (Core, blocking for the Lane half). With
    `lane_1789352091587813000` selected and Core's own `turn_owner` carrying
    `lane_id: lane_1789352091587813000`, `tool edit_file path=README.md`
    reported `path: …/scratchpad/e3c/fixture/README.md` and left the Lane
    worktree clean; `git status` in the workspace root showed ` M README.md`
    and in the Lane worktree showed nothing. The product says so itself: `tool
    shell command=pwd`, in the same Lane, printed
    `…/scratchpad/e3c/fixture` — the workspace root. The client sent the owner
    correctly (the `turn_owner` meta proves it reached Core), so the
    resolution site is Core's: the turn's tool working directory is the
    session's `canonical_root`
    (`crates/runtime/src/frontend_services.rs`, the session engine's root),
    not the owning Lane's `worktree`
    (`crates/lanes/src/lane_supervisor.rs`, the record that carries it).
    Numbered and stopped here; no `crates/**` file was edited.
    **Fixed 2026-09-14 by Core batch C17** (`fix(runtime): run a Lane-owned
    turn's tools in the Lane worktree`). The resolution site was one step
    further in than E3c's suspect: `frontend_services.rs` reads workspace files
    and diffs and is not the tool working directory. The turn opened its
    attribution window with the right owner, but the tool loop several frames
    below built every `ToolExecutionContext` from `SessionEngine::cwd` —
    `crates/runtime/src/runtime_loop.rs:604` (the approval's
    `decision_context` preview), `:702` (the registry execution) and `:844`
    (the post-edit LSP diagnostics). The native turn context now carries the
    root it was opened with, `NativeTurnContext::tool_root`
    (`crates/runtime/src/work_evidence.rs:101`), resolved once per turn by
    `native_turn_tool_root` (`:435`) from the durable Lane record and read back
    by `active_tool_root` (`:468`); all three sites read that one value, so the
    file an operator approves and the file that changes cannot differ. A Lane
    with no worktree still runs in the workspace root, and containment is
    re-checked at the consuming end because `PermissionEngine` normalizes a
    relative tool path against the root. No event, capability, count, or
    fixture moved. Tests:
    `crates/runtime/src/tests/lane_execution_scope_tests.rs:200` and `:340`.
    Compatibility follow-up 20.
29. **A follow-up cannot be queued during a turn; the label promises a queue,
    the submit is refused, and the kept text is discarded** (GUI). The
    composer label switches to `Queue follow-up` the moment Core reports the
    turn, but `sendComposerIntent` still goes through the one command slot,
    which the first `SubmitUserInput` holds until Core answers it — and Core
    does not answer while the turn waits on an approval. The refusal is
    honest (defect 23's alert, and it says `Your message is kept`), but the
    promise above it is not, and after the approval was decided the composer
    re-rendered empty: `Summarise what you just changed` never reached the
    session JSONL. Core has `RuntimeCommand::QueueFollowUp` and publishes
    `InputQueued`, and `PendingD1Kind::Queue` already matches it
    (`apps/gui/src-tauri/src/adapter.rs:646`), so the contract is there; the
    cockpit does not route the busy-composer submit to it. **Compatibility
    follow-up 16 is therefore unmet on the GUI.**
30. **The operator-git owner does not follow the Lane selection** (GUI,
    blocking for the source-control half with a Lane). With
    `lane_1789357518367069000` selected — the status bar, the titlebar and the
    context dock all name it — the commit bar is disabled carrying
    `D1-OPERATOR-GIT-OWNER: Lane \`lane_1789352091587813000\` does not have one
    exact Core operator git owner`, which is the **other** Lane in the
    workspace. Re-selecting either Lane does not change the name in the
    refusal. This is precisely the suspect E3b left for E3c under defect 25 —
    `GuiCoreAdapter::operator_git_owner` and
    `RuntimeProjection::permission_dock_for_owner` — and it explains why E3b's
    `git_push` half reached no decidable surface with a Lane selected while
    H5's in-process probe, which used a workspace target, found the approval
    arriving correctly. **Defect 25 stays open and this is its located cause
    on the client side**; the `git_push` half itself was not re-driven,
    because no commit-bar entry point was reachable.
31. **The settings standing-approvals list still shows a rule Core has
    revoked** (GUI, honesty). `Revoke` then `Save` wrote
    `audit_1789353306646568000 approval.revoke … {"scope":"always"}` and the
    rule stopped applying — the identical tool input asked for approval again
    — but the panel, closed and reopened, still lists
    `edit_file · new=Requeued old=Edited …`, `granted 1789352875 in
    ws_65ba50067a87fc27`, with its `Revoke` button. The operator is shown a
    standing rule that no longer stands.
32. **The composer applies macOS automatic quote substitution, so a typed
    double quote becomes a curly quote** (GUI, blocking for any tool argument
    or code the operator types). Minimal reproduction: setting the composer to
    `A"b"c` reads back as `A”b"c` (`U+201D` for the first quote). It corrupted
    every `update_plan` payload this run attempted, twice, at the same offset,
    and Core answered each with its own typed parse refusal. Any JSON, any
    quoted shell argument and any code snippet sent through the composer is
    affected. The entry path here was the accessibility value setter, which
    WebKit routes through the same editing pipeline a human keystroke uses, so
    the substitution is the webview's and not the harness's; a confirmation
    with real keystrokes is owed, because this host's input source is a Pinyin
    IME and `keystroke` on it produced `X"有"` rather than `X"y"z`.
33. **With Core reporting execution stopped, the rail selects a centre view
    and the centre keeps the recovery surface** (GUI). After a tool call
    failed, the centre drew `AGENT STOPPED` / `Core reports that execution has
    stopped.` over Core's own fact controls — `Reconnect · GUI-CORE-003`,
    `Inspect facts`, `Restart agent · no_retryable_session`, `Close Lane`,
    `Restore checkpoint · GUI-CORE-003`, every unbacked one disabled and
    labelled, which is right. What is wrong is that pressing `Plan Studio` or
    `Diff review` afterwards highlighted the rail slot and left the centre on
    the recovery surface, so both registered centre views were unreachable for
    the rest of that Lane's life. The `Esc` return worked from that state.
34. **A restored Lane has no execution owner after a relaunch** (GUI or Core,
    blocking for restart). Rebinding the same workspace through `⌘O` against a
    `VIDEN_HOME` holding a created Lane restores the Lane row, its branch and
    its worktree, and then refuses everything: `The selected Lane has no sole
    Core execution owner.`, the composer and its three pills disabled, the
    commit bar disabled. `lane-agent-bindings.jsonl` holds the binding with
    the **previous** process's `session_id`, and the new process mints a new
    session, so the exact-owner predicate finds none. A Lane created before a
    restart is therefore read-only forever. This is the operable half of what
    E3b numbered 26; the blank-window half does not reproduce (step 27).
    **Fixed 2026-09-14 by Core batch C17** (`fix(runtime): re-bind restored
    Lanes to the new session at hydration`). It was Core's, and it had two
    independent causes, both of which had to go: the snapshot prefix
    republishes `LaneUpdated` for every durable Lane and deliberately published
    no owner (`crates/runtime/src/runtime_contract.rs:2103`), so the
    exact-owner predicate found none; and `lane-agent-bindings.jsonl` still
    named the dead session, so even with an owner in hand
    `run_supervised_input`'s reservation
    (`crates/runtime/src/runtime_supervisor.rs:5209`) refused the submit.
    Core now, at hydration, emits `LaneRuntimeOwnerBound` for every restored
    non-terminal Lane naming this process's session
    (`rebind_restored_lanes_to_this_session`,
    `crates/runtime/src/runtime_supervisor.rs:3748`) *and* appends a
    superseding durable binding row for it
    (`WorkflowStore::rebind_lane_agent_session`,
    `crates/workflows/src/stores.rs:287`); the old row stays, because the log
    is append-only. A Lane owned by an external agent is skipped, both halves,
    so a restored ACP continuation keeps its own owner. No capability, count,
    event shape, or fixture moved. Test:
    `crates/runtime/src/tests/lane_execution_scope_tests.rs:414`.
    Compatibility follow-up 21.

### Verdicts on defects 22–26

| # | Verdict on the candidate |
| --- | --- |
| 22 | **Closed, confirmed natively, twice.** The popover TASK becomes a turn on the fallback provider: the composer frees, `EVENTS` advances, the turn settles, and the session JSONL carries the whole turn under the Lane owner |
| 23 | **Closed, confirmed natively, twice.** A refused submit renders a `role=alert` naming the held command (`submit_user_input`, then `set_work_mode`), its age, and that an effect's wait is never abandoned client-side |
| 24 | **Closed, confirmed natively, in both directions.** `Cancel active turn` is absent on an idle Lane and present only while Core reports an active turn |
| 25 | **Still open; its client-side cause is now located.** The commit bar never became reachable with a Lane selected, because the operator-git owner resolves to a different Lane than the one selected (defect 30). The `git_push` half itself was not re-driven |
| 26 | **Not reproduced.** A `VIDEN_GUI_WORKSPACE` launch against a home holding two Lanes rendered the full cockpit. The operable half of the same restart problem is real and is numbered 34 |

### Verdicts on compatibility follow-ups 14–16

| # | Verdict on the candidate |
| --- | --- |
| 14 | **Closed, natively.** The archived patch is canonical and its path label is the file's own path: `kind patch`, `source native`, `path README.md`, `owner lane lane_…`, `verification verified`, `canonical quality pass` |
| 15 | **Closed, natively, for both paths.** D14 shows `approval.allow_once` for `tool · lane_create` and for `tool · edit_file` on the same loaded page, each durable in `audit.jsonl` under the id its request had already shown |
| 16 | **Unmet on the GUI.** The composer advertises `Queue follow-up` during a turn and then refuses the submit, and the kept text is discarded rather than queued or sent. See defect 29 |

### Observations, not defects

- **Front the process before every keystroke batch, and verify it.** The first
  `⌘O` bind of this run went to another application entirely: the keystrokes
  were sent with `Claude` frontmost and the Viden panel never saw them, which
  the breadcrumb caught (`README.md 155 bytes` — a previous batch's fixture —
  rather than `182 bytes`). E3b's rule was about not capturing between typing
  and `Return`; this one is narrower and earlier: read
  `name of first process whose frontmost is true` and assert it is
  `viden-gui` before sending anything.
- **An Accessibility walk dismisses the composer-control popovers.** The
  work-mode, permission and model pills open a real popover; any `axtree`-style
  walk between the opening click and the choice closes it, so the pill looked
  inert through five different attempts. A popover choice is two real clicks
  with no Accessibility read in between, and the frame that proves the popover
  opened must be a window-id capture taken before the walk.
- **Address the window by name, not by index.** While a composer popover is
  open the process owns a second, unnamed `layer=20` window, and
  `first window of process` selects it; every walk then returns the popover's
  own 161-line tree and every control "disappears". The helpers in this
  batch's scratch directory use `first window whose name is "Viden"`.
- **The window leaves the Accessibility tree after a full re-render.** Six
  `-1719` answers across the run, each immediately after a click that caused a
  centre-view switch, an approval decision or a Lane selection, each
  recovering with the process alive 3–10 s later. Unchanged from E3b's
  reading; defect 18 is still not a self-termination in this batch's evidence.
- **The pinned permission dock overlays the Decision Center's own list.**
  Reproduced exactly as E3b recorded it, in D2, with both copies decidable.
- **The bundle is still `0.1.0-rc.5`.** The plan targets `rc.6` for this
  milestone and no batch has taken the bump.

### What This Means For Plan Goal 4, Per Surface

`docs/release-0.3.3-plan.md` goal 4, carried into `0.3.4` as goal 6 — "one real
local-first development task completes through the GUI, from intake to a
committed change, with audit and evidence recorded":

- **Through the TUI — met, end to end**, unchanged from the 2026-09-12 record.
- **Through the native GUI window on the `0.3.5` candidate — the agent half is
  now open, and the Lane's source-control half has closed behind it.** E3b
  could not start a turn in a created Lane; E3c starts one from the popover's
  own TASK field, and behind it reaches the tool approval with Core's typed
  hunks, allow-once, the inline tool diff, the archived patch, D14 reading back
  both approval paths, C15's `Always` and its revoke, and C16's typed `Edit`
  with the `revised_from` link. That is everything the `0.3.5` trust contract
  adds, evidenced natively.
- **What the cockpit still cannot do is finish the task inside the Lane.** The
  agent's edit lands in the workspace root rather than the Lane worktree
  (defect 28), and the commit bar refuses against the wrong Lane (defect 30),
  so no Lane-scoped commit and no push was reachable in this run. E3b reached
  a Lane-scoped stage and commit; it did so before either defect could be
  observed, over a tree dirtied out of band, and the two records should be read
  together rather than as a regression claim.
- **The honest aggregate:** the `0.3.5` trust surfaces — the Lane-path audit
  row, the approval scopes, the editable revision, the archived patch, the
  audit timeline — are **closed and evidenced natively**. Goal 4's own
  end-to-end claim is **still not met on the GUI**, for two newly located and
  differently-shaped reasons rather than for the one that blocked `0.3.4` and
  `0.3.5`'s first attempt. Exit criterion 1 is met as to the candidate being
  driven natively and as to the Lane's turn; it is not met as to the full task,
  and this document does not pretend otherwise.

## Defects 29–33 And The Client Half Of 34 — H6 (2026-09-14, on `claude/gui-native-defects-h6`)

Branch `claude/gui-native-defects-h6` in `.worktrees/gui-native-defects-h6`,
base `claude/int-0.3.5` at `b868f5c8` (the commit that merged E3c's evidence).
No native window was launched and no keystroke was sent in this batch: every
frame below is a headless capture, and **E3d re-proves all of it natively**.
Defect 28 and the Core half of 34 are not touched here; they are Core's, and
batch C17 owns them.

Each fix is red-first, one defect per commit, with the failing text recorded in
the commit it belongs to.

### Verdicts

| # | Verdict | Commit | Where | Test |
| --- | --- | --- | --- | --- |
| 25 | **Client cause fixed (H6); native confirmation owed to E3d.** The commit bar never became reachable with a Lane selected because the operator-git owner resolved to a different Lane (defect 30). With that fixed, the in-process live-Core probe drives the Lane-selected pair end to end and the `git_push` half reaches the permission dock and settles with Core's typed `no_upstream` | `eacdbf68` | `apps/gui/src-tauri/src/adapter.rs` (the probe) | `the_push_half_of_commit_and_push_reaches_a_decidable_surface_with_a_lane_selected` |
| 29 | **Fixed.** A submit while Core reports a turn now carries `RuntimeCommand::QueueFollowUp`, waits in its own `pending_queue` rather than behind the unanswered submit holding the D1 slot, and is confirmed by Core's `InputQueued`. The `Queue follow-up` label reads the same predicate the route does, so it is shown only where the queue can be sent. A follow-up refused by its own occupied queue renders defect 23's alert and **keeps the text**: the refusal now clears the submitted draft, which is what let an unrelated confirmation clear the composer under it. The queued row is unchanged and is Core's own `InputQueued` through `liveWork.queuedInputs`. **Compatibility follow-up 16 is met on the GUI** | `87d20d96` | `apps/gui/src-tauri/src/adapter.rs` (`queue_follow_up_and_wait`, `observe_pending_queue`, `pending_queue`), `apps/gui/src-tauri/src/lib.rs` (`d1_queue_follow_up`), `apps/gui/src/screens/d1_cockpit.ts` (`queueComposerFollowUp`, `queueFollowUpAvailable`, `refuseOccupiedCommandSlot`) | `a_follow_up_queues_while_the_submit_that_started_the_turn_is_still_unanswered`; `tests/h6_native_defects.spec.ts` "defect 29" |
| 30 | **Fixed.** The host was already right — `operator_git_owner` resolves per Lane from the binding Core published. The webview was not: `selectLane` re-read the owner, but the selection also moves when Core's own `selectedLaneId` arrives in a projection, when a creation resumes into its Lane, and on a retried Agent session, and each assigned the field directly. Every assignment goes through one `setSelectedLane` funnel now, the read records which Lane it asked about, an out-of-order answer for a superseded selection is dropped, and the render refuses to show an answer scoped to another Lane at all | `eacdbf68` | `apps/gui/src/screens/d1_cockpit.ts` (`setSelectedLane`, `readOperatorGit`, `operatorGitForSelection`, `operatorGitScope`) | `the_operator_git_owner_is_resolved_against_the_selected_lane_alone`; `tests/h6_native_defects.spec.ts` "defect 30" |
| 31 | **Fixed.** The host already answered a revoke with the `standing_approvals` Core holds afterwards; the shell kept only the verdict and threw the list away, and nothing else refreshed the cockpit projection — the revoke's own drain consumes the `StandingApprovalRevoked`, and a quiet Core produces no later wake. The whole answer now reaches the settings panel and is rendered until a newer projection replaces it. It is Core mirrored, not a client-side removal: a refused revoke comes back with the rule installed and the row stays | `76f0f0b5` | `apps/gui/src/main.ts` (`revokeStandingApproval`), `apps/gui/src/screens/d1_cockpit.ts` (`standingAfterRevoke`) | `the_revoke_answers_with_the_standing_list_core_holds_afterwards`; `tests/h6_native_defects.spec.ts` "defect 31" |
| 32 | **Attributes set; keystroke confirmation owed to E3d.** `verbatimField` turns off `autocorrect`, `autocapitalize`, `spellcheck` and `autocomplete`, and every text field in the client is created through it — the composer, the Lane task, the commit message, the approval Edit field, the plan reject reason, the D2 review note, the D4 Lane id and branch, the D11 path and config draft, the D12 reason, the D16 field, and the palette and evidence search boxes. `spellcheck="false"` is the load-bearing one, because WebKit's substitution passes ride the text-checking pipeline. What E3c measured was the accessibility value setter, which WebKit routes through the same editing pipeline a keystroke uses; if a substituted quote survives these attributes the remaining setting is the webview's and belongs to the shell rather than to the page | `e7fac8d1` | `apps/gui/src/ui/verbatim_field.ts`, and the fourteen call sites it is applied at | `tests/h6_native_defects.spec.ts` "defect 32", including a guard that walks the client source so a field added later fails until it is verbatim |
| 33 | **Fixed.** The recovery surface is what the *conversation* slot shows, not a modal over the cockpit. It outranked every registered centre view, so with Core reporting execution stopped, `Plan Studio` and `Diff review` highlighted their rail slot and changed nothing. It is scoped to `centerView === "transcript"` now: still reachable, with its Core-backed fact controls and their labels unchanged | `94f86dfb` | `apps/gui/src/screens/d1_cockpit.ts` (`recoveryActive` / `showRecovery`) | `tests/h6_native_defects.spec.ts` "defect 33" |
| 34 (client half) | **Client derivation fixed; the defect stays open on Core.** While the Lane has no exact owner the refusal names what is missing in Core's own words, and that is kept. `exact_lane_owner` required exactly one binding and refused otherwise, so a view carrying both a restored Lane's previous-process binding and the one a relaunch mints resolved no owner at all; it reads the **latest** binding for the Lane now, and still refuses a binding whose owner names another Lane. This alone does not close 34: Core's own view reducer keeps a Lane's *first* valid binding and drops a later one (`RuntimeViewState::apply_event`, `crates/types/src/runtime.rs`), so the view the client reads can still carry only the stale binding. For a relaunched Lane to have an owner, the rebinding has to be the one the view ends up holding — **a Core fact, owned by C17** | `83091072` | `apps/gui/src-tauri/src/adapter.rs` (`exact_lane_owner`) | `a_lane_with_two_published_bindings_resolves_its_latest_execution_owner`, `a_binding_whose_owner_names_another_lane_is_still_refused` |

### Evidence

Headless captures at 1440x1180, listed and described in
[`apps/gui/evidence/main-window-interactions/EVIDENCE.md`](../../../apps/gui/evidence/main-window-interactions/EVIDENCE.md)
under "H6 captures": the queued composer in dark/en and light/zh-CN, the
recovery surface in the conversation slot and Plan Studio in the centre from
the identical projection, and the settings panel after an answered revoke.
Defect 30's change is a selection funnel and defect 32's is a set of input
attributes; neither alters what is drawn in any single state, so neither has a
frame, and both are covered by the tests named above.

### What is still owed

- **E3d, natively:** the Lane-selected `Commit & Push` pair, a composer
  follow-up queued and drained during a live turn, a real-keystroke check that
  a typed `"` stays `U+0022`, a centre view opened over a stopped Lane, and the
  settings list after a revoke.
- **C17, on Core:** defect 28's tool working directory, and the half of defect
  34 that decides which binding a relaunched Lane's view carries.

## Native GUI Run — 2026-09-14 (0.3.5 candidate `71d5156a`, E3d)

This is E3d: the remaining Lane steps driven through the native cockpit on the
`0.3.5` candidate after C17 and H6. Branch `claude/evidence-e3d` in
`.worktrees/evidence-e3d`, base the integration branch `claude/int-0.3.5` at
`71d5156ac489a83c041a7e82b98e336932a0a276` — E3c's evidence plus C17 (Lane-turn
tools run in the Lane worktree; restored Lanes re-bound at hydration) and H6
(the operator-git owner follows the selection, queue-behind-turn, the standing
list, verbatim fields, centre views over the recovery surface).

The host screen was unlocked and `CGSessionCopyCurrentDictionary()` was read
before every keystroke and click batch; `CGSSessionScreenIsLocked` was `0` at
every one of them, from `2026-09-14T05:35:58Z` to `2026-09-14T07:07:31Z`.

**The batch's headline: the agent now works inside the Lane, and the Lane's
source-control half is blocked one step earlier than before.** C17's two fixes
hold natively and completely — a tool called in a Lane-owned turn writes into
the **Lane worktree** and the workspace root is untouched (defect 28), and a
Lane restored across a relaunch has an execution owner and takes a turn
(defect 34). H6's selection funnel holds too: with a Lane selected the commit
bar is **reachable**, carries no wrong-Lane refusal, and names only the
selected Lane (defect 30). What this run could not finish is the commit and
the push themselves, because `Stage all` never reaches a decidable surface:
the client stands on `Stage is waiting for approval in the permission dock.`
while the dock holds nothing, no `git_*` audit row is ever written, and the
status bar reads `REQ 0 req / 0 err` (defect 35). The bare `origin` was
therefore created and never used, and **defect 25 stays open**.

No blocker fix was taken in this batch. Defect 35 is not decidable from the
client side alone on this evidence — see its entry — and a speculative fix to
a surface this run could not localize would have been worse than the finding.

### The application under test

`npm ci` added 97 packages, and audited 98 packages in 4 s.
`npx tauri build --bundles app` PASSED —
`Finished \`release\` profile [optimized] target(s) in 1m 08s` — and produced
`target/release/bundle/macos/Viden.app`:

| Fact | Value |
| --- | --- |
| `CFBundleShortVersionString` | `0.1.0-rc.5` |
| `CFBundleIdentifier` | `dev.viden.gui` |
| `CFBundleExecutable` | `viden-gui` |
| Architecture | `Mach-O 64-bit executable arm64` |
| Executable size | 42,891,264 bytes |
| Bundle size | 42 MB |
| Signature | ad-hoc, linker-signed, `TeamIdentifier=not set`, `Sealed Resources=none` |

The bundle still carries `0.1.0-rc.5`; the plan targets `0.1.0-rc.6` and no
batch has taken that bump. R1 owns it. It was launched from the bundle, never
installed, signed with an identity, notarized or distributed. Every launch ran
with `VIDEN_HOME` pointed at a scratch directory, so this repository's own
`.viden/` was never read or written. Launched PIDs, in order: 39630, 40297,
49594, 58246. All four were killed by this batch; none exited on its own.

### Fixture

One fresh temporary Git repository under the run's scratch directory, with one
commit `2b1b3e3` ("Add the fixture README"), a five-line 125-byte `README.md`, a
`.gitignore` covering `.viden/` and `.worktrees/`, and a `.viden/config.toml`
selecting `provider = "fallback"` / `model = "test-local"`. Core minted
`prj_1789364819298232000` in `ws_8f05f137b4ef25a8` at the bootstrap. A bare
`origin` was created in the scratch directory and **never used**, because no
push became reachable (defect 35). Two Lanes were created:
`lane_1789365083535959000` and `lane_1789366692288148000`.

### Two deviations from the scripted task, and why

**The bind was taken at launch through `VIDEN_GUI_WORKSPACE`, not through
`⌘O`.** The `⌘O` panel opened and was driven as far as its `⌘⇧G` "Go to" sheet,
whose path field was set verbatim through the accessibility setter and which
resolved the path to `… › e3d › fixture` on screen — but neither `Return` nor a
click on the resolved row committed it, on either the host's Pinyin input
source or ABC. Keystrokes sent to the process reach the main window (`⌘O` and
`⌘L` both worked) and do **not** reach the panel's child window, which renders
inactive throughout. E3c's breadcrumb rule caught the consequence before any
damage: the panel had restored **E3c's** fixture (`README.md 186 bytes`) and
would have bound that, not this run's (`125 bytes`). The launch-time bind is
the same product path E3c step 27 drove, so the cockpit under test is bound the
way Core binds it; only the operator gesture differs, and it is recorded here
rather than glossed.

**The source-control half was never reached, so nothing downstream of a commit
was driven.** This is not a shortcut: it is defect 35, recorded as a blocker.

### Step by step

Every frame was captured twice by window id and the second kept, and every PNG
listed was viewed before it was listed. Frames live under `native-0.3.5/` with
the `e3d-` prefix.

| # | Step | Reached | Result / reason | Frames |
| --- | --- | --- | --- | --- |
| 1 | Welcome | yes | `Welcome to Viden`, `Your local-first agent workspace`, `Open project ⌘ O`, `No project open`, `Core adapter is not connected`; `MODE — · PERM — · CONTEXT — · EVENTS #0 · LANE — · LATENCY — · TOKENS — · DIAG 0× · REQ —` | [`01`](native-0.3.5/e3d-01-welcome.png) |
| 2 | **Bound cockpit** | yes | Dock `Provider fallback`, `Model test-local`, `Mode build`, `Permission ask`, `Tokens 0`; `Lines +0 −0` / `No changes`; `Showing Workspace root`, `Branch main`, the fixture path as `Worktree`, `Ahead 0`, `Behind 0`, `Working tree Clean`; `MODE build · PERM ask · EVENTS #7 · LANE — · DIAG 0× · REQ 0 req / 0 err`. The bind path is the deviation recorded above | [`02`](native-0.3.5/e3d-02-cockpit-bound.png) |
| 3 | **`⌘L` New Lane, built-in agent, TASK** | yes | `Viden Agent built-in` selected; Codex, Claude Agent and Kiro CLI disabled, first under `Checking ACP agents… each probe is given up to 15s.` and then under `Probe did not answer within 15s.` with `Retry ACP check` beside them — the probe is bounded and says so. With the TASK entered, `Create lane ↵` read `enabled=true` and the branch became `⌁ vd/edit-the-fixture-readme-he`. **Defects 14 and 15, natively, again, on a host with no ACP binary** | [`03`](native-0.3.5/e3d-03-new-lane-popover.png), [`04`](native-0.3.5/e3d-04-new-lane-task-set.png) |
| 4 | **A Core `lane_create` approval with C15's full tier list** | yes | `Approve lane_create · lane_create`, `Medium risk`, `branch: viden/lane_1789365083535959000 path: …/.worktrees/lane_…`, `Reason: lane mutation policy requires approval`, `Default: Deny`, `Audit: audit_1789365099211250000`, over `Y · Once` / `Allow repo paths` / **`Shift+A · Always · scope_not_offered`** / **`E · Edit · not_editable`** / `N · Deny`. Both disabled tiers carry **Core's own refusal words** — **register 019, closed on the GUI, live, again** | [`05`](native-0.3.5/e3d-05-lane-create-approval.png) |
| 5 | **The Lane, and the popover TASK becoming a turn — defect 22** | yes | `Y · Once` released it. Tab strip `lane_1789365083535959000 coder starter…`, titlebar `1 worktree`, dock `Showing Lane worktree`, `LANE OUTPUT` carrying Core's own `HEAD is now at 2b1b3e3 Add the fixture README` / `Preparing worktree (new branch 'viden/lane_…')`. Verified out of band: the worktree exists and `git branch` lists `viden/lane_1789365083535959000`. The composer freed with **no `Cancel active turn`**, `EVENTS #46`, `REQ 1 req / 0 err`. Verified byte for byte in the scratch session JSONL: `turn_owner` with `lane_id: lane_1789365083535959000` and `turn_id: turn_1789365157674412000`, the user message `Edit the fixture README heading in the Lane`, a cost row `outcome: success` on `fallback` / `test-local`, the assistant reply, then `turn_owner` cleared. **H5's fix still holds natively** | [`06`](native-0.3.5/e3d-06-lane-created-turn.png) |
| 6 | **Composer `tool edit_file path=README.md old=Fixture new=Edited`** | yes | Core raised `Approve edit_file · edit_file`, `Medium risk`, `TOOL INPUT PREVIEW new: Edited old: Fixture path: README.md`, **Core's typed C1 hunks** `@@ -1,5 +1,5 @@` with `-# E3d Fixture`, over `Y · Once` / `Allow repo paths` / `Shift+A · Always` (**enabled**) / `E · Edit · not_editable` / `N · Deny`. `Core is working; no owner-scoped status is available. typed`, `Cancel turn · Esc`, and the composer label switched to `Queue follow-up`. The dock had already switched to `Showing Lane worktree` with the Lane's branch and worktree | [`07`](native-0.3.5/e3d-07-edit-file-approval.png) |
| 7 | **Defect 28 — the tool runs in the LANE WORKTREE** | yes, **closed** | `Y · Once`. Core's own tool result: `edit_file completed` / `path: …/scratchpad/e3d/fixture/.worktrees/lane_1789365083535959000/README.md` / `size: 124 B` / `effect: edited 5 lines`. On disk, out of band: the **workspace root** is clean and still `# E3d Fixture` at 125 bytes; the **Lane worktree** is ` M README.md`, `# E3d Edited`, 124 bytes, `@@ -1,4 +1,4 @@` `-# E3d Fixture` / `+# E3d Edited`. And the product says it itself: `tool shell command=pwd`, in the same Lane, printed `…/fixture/.worktrees/lane_1789365083535959000` — the Lane worktree, where E3c's printed the workspace root. **C17's fix holds natively, in both directions** | [`09`](native-0.3.5/e3d-09-lane-worktree-tool-root.png) |
| 8 | **Follow-up 16 / defect 29 — queue a follow-up during a turn** | **no**, and worse than E3c | With Core reporting the turn and the label reading `Queue follow-up`, `Summarise what you just changed` was submitted. It was **not** refused — E3c's honest `Not sent…` alert never appeared — the composer cleared, no queued row was drawn, and `EVENTS` did not advance (`#49` before and after). The turn then ran to completion normally, and the text **never reached Core**: no such message exists in the session JSONL and the string appears nowhere under `VIDEN_HOME`. Driven twice, in two turns. See defect 36 | [`08`](native-0.3.5/e3d-08-followup-queued.png) |
| 9 | **Defect 33 — a centre view over a stopped Lane** | yes, **closed** | With the centre drawing `AGENT STOPPED` / `Core reports that execution has stopped.` over Core's fact controls (`Reconnect · GUI-CORE-003`, `Inspect facts`, `Restart agent · no_retryable_session`, `Close Lane`, `Restore checkpoint · GUI-CORE-003`, every unbacked one disabled and labelled), `Diff review` **switched the centre**: the recovery surface left the Accessibility tree entirely and DiffReview rendered in its place. Returning to `Conversation` brought the recovery surface back, which is exactly H6's scoping — still reachable in the slot it belongs to | [`10`](native-0.3.5/e3d-10-defect33-diffreview-over-stopped.png) |
| 10 | **DiffReview over the Lane's dirty tree, dirtied by the product** | yes | `CHANGES 1 files · +1 −1`, `M README.md +1 −1`, `README.md`, `@@ -1,4 +1,4 @@`, `-# E3d Fixture` / `+# E3d Edited` with per-line numbering; dock `Lines +1 −1`, `M README.md +1 −1`, `Showing Lane worktree`, the selected Lane's branch and worktree, `Working tree Dirty`. The total, the file list and the `Local` flag agree — **defect 20, natively**. Unlike E3a, E3b and E3c, **this change is the product's own**, made by `edit_file` under its approval | [`10`](native-0.3.5/e3d-10-defect33-diffreview-over-stopped.png) |
| 11 | **A second Lane, and D2 deciding its creation** | yes | The New Lane popover from the tab strip's `+ New Lane`; `⌘L` did not open it from a non-transcript centre view. Its `lane_create` approval landed in the Decision Center rather than the transcript, and `Approve once` there released it: `lane_1789366692288148000`, a second worktree, `2 worktrees` in the titlebar. **Defect 16's first half, natively, again** | [`11`](native-0.3.5/e3d-11-two-lanes-lane2-selected.png) |
| 12 | **Defect 30 — the operator-git owner follows the selection** | yes, **closed** | With `lane_1789365083535959000` selected after the relaunch, `Open the commit bar` is **enabled**, the commit bar renders (`Commit message` enabled, `Stage all` enabled, `Commit` and `Commit & Push ⇡` disabled until something is staged), **no `D1-OPERATOR-GIT-OWNER` refusal appears anywhere**, and the only Lane named in the whole view is `viden/lane_1789365083535959000` — the selected one. E3c had `Open the commit bar` disabled, carrying a refusal that named the *other* Lane | [`13`](native-0.3.5/e3d-13-commit-bar-lane-selected.png) |
| 13 | Stage, commit, `Commit & Push ⇡`, the `git_push` half, `NoUpstream`, set upstream, push accepted | **no** | `Stage all` is enabled and presses cleanly, and nothing happens. The centre renders `Stage is waiting for approval in the permission dock.`; the dock holds `0 awaiting you` / `Core is holding no decision for you.`; the Lane worktree stays ` M README.md` (unstaged) through 60 s of watching; no `git_*` row is ever appended to `audit.jsonl`; and the status bar reads `DIAG 0× · REQ 0 req / 0 err` — there is not even a request in flight. Reproduced on a fresh process and a fresh Core connection, with the correct Lane resolved. `Commit` and `Commit & Push ⇡` therefore never enable, no push entry point is reachable, and the bare `origin` was never used. See defect 35 | [`15`](native-0.3.5/e3d-15-defect35-stage-never-decidable.png) |
| 14 | **Defect 34 — a Lane restored across a relaunch** | yes, **closed** | Quit and relaunched against the same workspace and `VIDEN_HOME`. `lane-agent-bindings.jsonl` took C17's superseding rows — `lane-agent-rebound:lane_…:session_1789367410385273000` for **both** Lanes, the append-only previous rows left in place. In the cockpit the restored Lane is selected, the refusal `The selected Lane has no sole Core execution owner.` is **absent**, the composer is enabled, and a submit **ran a turn**: `turn_owner` with `lane_id: lane_1789365083535959000` and `session_id: session_1789367410385273000` — the new session — and the user message in the new session's JSONL. Its commit bar is enabled (step 12). The transcript also reads back the durable audit rows across the restart: `lane-approval_1789365099211245000 · decision allow_once`, `approval_1789365849889072000 · decision allow_once`, `approval_1789366009927556000 · decision allow_once`, each with `Open audit trail` | [`12`](native-0.3.5/e3d-12-relaunch-restored-lane.png), [`17`](native-0.3.5/e3d-17-esc-return.png) |
| 15 | **Defect 32 — a typed `"` with real keystrokes** | yes, **closed** | The input source was switched to `com.apple.keylayout.ABC` for this leg alone and restored afterwards (recorded below). `A"b"c` was typed into the composer as five real HID key events posted at `cghidEventTap` — the same tap a keyboard enters at — and read back byte for byte: `41 22 62 22 63`, identical to the expected string. **Both quotes are `U+0022`.** E3c's `A”b"c` (`U+201D`) does **not** reproduce: H6's `verbatimField` attributes hold under real keystrokes, so the remaining setting E3c reserved judgement on is not needed | [`14`](native-0.3.5/e3d-14-defect32-verbatim-quotes.png) |
| 16 | Plan Studio: an `update_plan` proposal in D15, Accept, Hand off refused in plan mode, Build, Hand off, D13's DAG | **no** | Not driven. Defect 32's blocker on typing JSON is gone (step 15), so the reason E3c could not enter the payload has been removed — but this run's sessions were consumed by the source-control half and by defect 37, and the leg was not started. Owed, and not evidenced either way here | — |
| 17 | Defect 31: `Always`, the auto-allowed repeat, the revoke, the row leaving the settings list | **no** | Not driven, for the same reason as step 16. H6's fix is untested natively. Owed | — |
| 18 | Agent Board controls | **no**, and not a defect | The built-in `Viden Agent` runs provider turns, not `runtime.agent_session_control` Agent sessions, and no ACP adapter is installed on this host — all three ACP agents read `Probe did not answer within 15s.` in the New Lane popover (step 3). No Agent session row can exist to pause, kill or retry. Said once, not faked | [`03`](native-0.3.5/e3d-03-new-lane-popover.png) |
| 19 | **Focus mode `⌘.` and the `Esc` return** | yes | The control hid the context dock and gave the centre the full window width with the titlebar, rail, composer and status bar retained. `Esc` returned the rail to `Conversation` with the chrome retained and focus mode still on, which is what `0.3.4` and E3c recorded. `⌘.` itself did not fire; the titlebar control did. See the harness note | [`16`](native-0.3.5/e3d-16-focus-mode.png), [`17`](native-0.3.5/e3d-17-esc-return.png) |

### Defects And Gaps Found By E3d

Numbering continues from the thirty-four above. Each was reproduced, not
inferred. None was fixed here.

35. **A Lane-scoped `Stage all` never reaches a decidable surface, and the
    commit bar waits on an approval that does not exist** (blocking for the
    whole source-control half; the owning side is not decidable from this
    evidence). With `lane_1789365083535959000` selected, its worktree dirty by
    the product's own `edit_file`, and the commit bar reachable and correctly
    scoped (defect 30 closed), `Stage all` presses cleanly and nothing
    happens. The centre renders Core's own
    `Stage is waiting for approval in the permission dock.`; the Decision
    Center renders `0 awaiting you` and
    `Core is holding no decision for you.`; the Lane worktree stays
    ` M README.md` through 60 s; **no `git_*` row is ever appended to
    `audit.jsonl`** (the only four rows in the whole run are the two
    `lane_create` approvals, the `edit_file` and the `shell`); and the status
    bar reads `REQ 0 req / 0 err` — no request is in flight at all. Reproduced
    on a fresh process against a fresh Core connection. `Commit` and
    `Commit & Push ⇡` never enable, so **no push entry point became reachable
    and the bare `origin` was never used — defect 25 stays open**, now behind a
    different and earlier wall than E3c's.
    Two sites bound this, and this run cannot choose between them. On the
    client, `run_operator_git_action_and_wait`
    (`apps/gui/src-tauri/src/adapter.rs:3146`) sends the command and then
    drains for 250 ms through its own `poll_operator_git` (`:3209`); the commit
    bar afterwards "polls this on the cockpit's ordered Core wake rather than
    holding a timer of its own" (`operator_git_poll`,
    `apps/gui/src-tauri/src/lib.rs:747-757`), so on a quiet Core no later wake
    arrives — which is the *exact* failure shape H6 fixed for defect 31, where
    "the revoke's own drain consumes the `StandingApprovalRevoked`, and a quiet
    Core produces no later wake". On Core, `run_supervised_project_mutation`
    (`crates/runtime/src/runtime_supervisor.rs:3012`) is the path
    `RuntimeCommand::RunOperatorGitAction` takes
    (`crates/runtime/src/runtime_supervisor.rs:2768`), and its first gate emits
    `CommandRejected` when an active job is already held for the owner. That
    branch is ruled out *as a silent one*, because `PendingOperatorGit::observe`
    (`apps/gui/src-tauri/src/adapter.rs:1526`) matches `CommandRejected` on the
    command id and would have rendered the refusal. What is not ruled out is
    that Core accepted the command and published no approval, and that the
    client's own send-drain consumed the request event. **Numbered and stopped;
    no `crates/**` file was edited, and no GUI fix was attempted, because this
    evidence does not localize the fault to the client.**
36. **A composer follow-up submitted during a turn is silently discarded**
    (GUI, blocking for follow-up 16, and a regression in honesty against E3c).
    H6 closed defect 29 by routing a busy-composer submit to
    `RuntimeCommand::QueueFollowUp` and promised it would be "confirmed by
    Core's `InputQueued`". Natively, none of that is observable and the text is
    lost. With Core reporting the turn and the label reading
    `Queue follow-up`, the submit is accepted — **no refusal is rendered**, so
    E3c's honest `Not sent. … Your message is kept.` alert is gone — the
    composer clears, **no queued row is drawn**, and `EVENTS` does not advance
    (`#49` before the submit and `#49` after). The turn then completed
    normally, and the follow-up **never reached Core**: no message with that
    content exists in the session JSONL, and the string
    `Summarise what you just changed` appears nowhere under `VIDEN_HOME`.
    Driven twice, in two separate turns. E3c at least told the operator the
    message was kept; this candidate tells them nothing and drops it.
    **Compatibility follow-up 16 is therefore still unmet on the GUI**, and the
    failure is now silent rather than loud.
37. **The Core connection drops mid-session and never recovers** (GUI or the
    adapter's transport; blocking when it happens). After roughly forty
    minutes of driving in one process, the cockpit fell back to
    `◌ CONNECTING` / `Establishing the versioned Core connection.` /
    `Core connection pending` with `DIAG 1×`, a `LANES` panel reading
    `No Lanes yet`, and the composer inert. The process stayed alive and the
    window stayed in the Accessibility tree; the state did not clear across
    six checks over 60 s, and only a kill and relaunch recovered it. Nothing
    was written to the process's stdout/stderr at `RUST_LOG=info` — the log
    file is zero bytes for that entire session — so this batch cannot say what
    the transport did. Recorded as observed, with the diagnostic counter Core's
    own status bar showed.
38. **A Lane created while another Lane's agent is stopped gets no execution
    owner, and the selection cannot be moved off it** (GUI or Core; observed
    once). Immediately after `lane_1789366692288148000` was created, the
    status bar, the titlebar and the context dock all named it, the composer
    carried `The selected Lane has no sole Core execution owner.`, the
    recovery surface's control read `Close Lane · no_exact_lane`, and the dock
    read `Waiting for the selected Lane context.` Clicking the **other** Lane's
    sidebar row did not move the selection: the status bar, the titlebar and
    the dock all stayed on the new Lane across two attempts and a settled
    re-read. For a window during the creation the context dock also showed the
    **first** Lane's branch, worktree and `+1 −1` while every other surface
    named the second — a stale scope that resolved on its own, so it is
    recorded as staleness rather than as a second instance of defect 30. The
    relaunch in step 14 cleared all of it: C17's rehydration gave **both**
    Lanes owners. Observed once, in the process that later lost its Core
    connection (defect 37), so the two may share a cause.

### Verdicts on defects 25–34

| # | Verdict on the candidate |
| --- | --- |
| 25 | **Still open, and now blocked earlier.** Its two client-side causes are gone — the commit bar is reachable with a Lane selected and scoped to that Lane (defect 30, closed) — but `Stage all` never reaches a decidable surface (defect 35), so `Commit & Push ⇡` never enabled, the `git_push` half was never sent, no `NoUpstream` refusal was seen, and the bare `origin` was never used |
| 26 | **Not reproduced, again.** Three launch-time binds against a home holding Lanes each rendered the full cockpit |
| 27 | **Closed, confirmed natively.** Composer control popovers opened and settled; no submit was ever refused with a held `set_work_mode` slot in this run |
| 28 | **Closed, confirmed natively, in both directions.** C17's fix holds: the tool result names the Lane worktree, the Lane worktree carries the edit, the workspace root is untouched, and the product's own `pwd` in the Lane prints the worktree |
| 29 | **Not closed; the shape changed for the worse.** See defect 36 |
| 30 | **Closed, confirmed natively.** With a Lane selected the commit bar is reachable, carries no `D1-OPERATOR-GIT-OWNER` refusal, and names only the selected Lane |
| 31 | **Not re-driven.** The `Always` / revoke leg was not reached in this run. H6's fix remains unconfirmed natively |
| 32 | **Closed, confirmed natively with real keystrokes.** `A"b"c` typed as HID key events on an ABC source reads back `41 22 62 22 63`; both quotes are `U+0022` |
| 33 | **Closed, confirmed natively.** `Diff review` pressed over a stopped Lane switches the centre, and the recovery surface remains reachable in the conversation slot |
| 34 | **Closed, confirmed natively.** C17's rehydration appends a superseding binding for every restored non-terminal Lane; the restored Lane has an owner, accepts a submit, runs a turn under the new session, and its commit bar is enabled |

### Verdict on compatibility follow-up 16

| # | Verdict on the candidate |
| --- | --- |
| 16 | **Still unmet on the GUI, and now failing silently.** The composer advertises `Queue follow-up` during a turn, accepts the submit with no refusal, clears the draft, publishes no `InputQueued` and does not advance `EVENTS`, and the text never reaches Core. See defect 36 |

### Observations, not defects

- **Keystrokes reach the main window and not its child panel.** `⌘O` and `⌘L`
  both worked against the Viden window, while the `NSOpenPanel` they raise took
  no key event at all — not `⌘⇧G` (until it was opened by a click into the
  panel first), not `Return` on the resolved Go-to row, on either input source.
  Clicks reached it; the panel rendered inactive throughout. The bind was taken
  at launch instead, and E3c's breadcrumb rule caught that the panel had
  restored E3c's fixture before anything was bound to it.
- **The window leaves the Accessibility tree far more often than E3b or E3c
  recorded.** Roughly a dozen `-1719` answers across this run, each immediately
  after a click that caused a centre-view switch, an approval decision or a
  Lane selection, each recovering with the process alive within 3–15 s. It cost
  this run an approval: the first `edit_file` request **expired at its own
  `Expires: 1789365601`** while the press could not be delivered, and Core
  wrote `decision: "deny"`, `reason: "RequiresApproval"`,
  `message: "User denied the permission request"`. Every subsequent decision in
  this batch went through a retry loop that waits for the window to re-enter
  the tree. Unchanged in kind from E3b's reading; defect 18 is still not a
  self-termination in this batch's evidence.
- **An approval has a five-minute life, and a harness must respect it.** The
  `Expires` field the approval card prints is real and enforced. Any leg that
  spends the approval window on something else loses the approval to a
  timeout deny.
- **Some accelerators are context-scoped.** `⌘L` opened the New Lane popover
  from the transcript and did nothing from DiffReview; `⌘.` did not fire at all
  while the `+ New Lane` popup button had been the last thing pressed, though
  the titlebar's own focus-mode control worked immediately. Not investigated,
  and not claimed as a defect.
- **A window-id capture can photograph a transition.** Two frames in this run
  were taken while a centre-view switch was still painting and showed the
  outgoing view; the Accessibility tree read a moment later showed the incoming
  one, and the frame was retaken. A capture disagreeing with the tree is the
  capture's fault, not the product's, and the tree is the state of record.
- **The bundle is still `0.1.0-rc.5`.** The plan targets `rc.6` for this
  milestone and no batch has taken the bump.

### What This Means For Plan Goal 4, Per Surface

`docs/release-0.3.3-plan.md` goal 4, carried into `0.3.4` as goal 6 — "one real
local-first development task completes through the GUI, from intake to a
committed change, with audit and evidence recorded":

- **Through the TUI — met, end to end**, unchanged from the 2026-09-12 record.
- **Through the native GUI window on the `0.3.5` candidate — the agent half is
  now complete inside the Lane.** E3c opened the Lane's turn; E3d closes the
  two things that made that turn's work land in the wrong place or vanish on
  restart. A tool called in a Lane-owned turn now edits the Lane worktree and
  leaves the workspace root alone, the product's own `pwd` says so, and a Lane
  restored across a relaunch owns itself again and takes another turn. With
  those, the cockpit can do real work in a Lane and keep it.
- **What the cockpit still cannot do is commit that work.** The commit bar is
  reachable and correctly scoped for the first time (defect 30 closed), and
  then `Stage all` reaches nothing decidable (defect 35). Nothing downstream —
  commit, `Commit & Push ⇡`, the `git_push` half, `NoUpstream`, the upstream,
  the push — was driven, and this document claims none of it.
- **The honest aggregate:** every `0.3.5` trust surface E3c evidenced still
  stands, and E3d adds the Lane's *working* half — correct tool scope, correct
  restart ownership, verbatim operator input, reachable centre views over a
  stopped Lane, and a commit bar that finally names the Lane the operator
  selected. Goal 4's end-to-end claim is **still not met on the GUI**, now for
  one blocking reason at the staging step rather than the three E3c named.
  Exit criterion 1 is met as to the candidate being driven natively, as to the
  Lane's turn, and as to the Lane's work landing in the Lane; it is not met as
  to the full task, and this document does not pretend otherwise.

## Defects 35–38 — H7 (2026-09-14, on `claude/gui-shell-regressions-h7`)

Branch `claude/gui-shell-regressions-h7` in `.worktrees/gui-shell-regressions-h7`,
base `claude/int-0.3.5` at `1963f8d1` (the commit that merged E3d's evidence).
The native confirmation this batch was narrowly allowed to take **was taken**.
The screen was locked at the first check
(`time=2026-09-14T08:23:46Z CGSSessionScreenIsLocked=1 onConsole=1`), unlocked
before the run began (`time=2026-09-14T08:31:04Z CGSSessionScreenIsLocked=0`),
read `CGSSessionScreenIsLocked=0` at every keystroke and click batch from
`08:31:30Z` to `08:58:20Z`, and locked again at `09:02:25Z`, which is where the
native driving stopped. Launched PIDs, in order: 22821, 29999; both were killed
by this batch and neither exited on its own. Both launches ran with
`VIDEN_HOME` under the run's scratch directory, so this repository's own
`.viden/` was never read or written.

**The batch's headline: H6 did not introduce defect 35, and neither did any
other batch — the desktop pump has always held the adapter mutex against the
operator's own gesture, and E3d is the run that finally lost a command to it.**
H6's tests passed because they exercised the cockpit against a mocked port and
the adapter directly. Neither of those layers has a pump. The shipped path is
webview -> `main.ts` port -> Tauri `invoke` -> the `lib.rs` command -> the
adapter, every command runs on a thread that is not the pump's, and both take
the one `DesktopState` adapter mutex. `spawn_core_event_pump` held that mutex
across a 250 ms blocking receive and re-acquired it the instant it let go;
`std::sync::Mutex` promises no fairness, so a command could queue behind the
drain indefinitely.

### The harness, and what it found before any fix

`apps/gui/tests/h7_shell_path.rs` drives the production command bodies — now
in `apps/gui/src-tauri/src/shell.rs`, one body per `#[tauri::command]`, each
command delegating to it — against a real `LocalCoreHost` over a temporary
session home and a temporary Git fixture, with a real pump thread running
beside it on the same `DesktopState`. Nothing in it reads or writes this
repository's `.viden/`.

Against the base it did not fail; it **hung**. Forty no-traffic `operator_git`
reads, which do nothing but take the adapter lock, did not complete in 120 s
and the harness watchdog aborted the binary (`signal: 6, SIGABRT`). A `sample`
of the stuck process is unambiguous:

| Thread | Where, in samples |
| --- | --- |
| The command thread | `shell::d1_poll` -> `with_adapter_mut` -> `Mutex::lock`, **1359 of 1384** |
| The pump thread | `pump_core_events_once` -> `pump_events` -> `receive_event`, holding that lock, **1382 of 1384** |

Every symptom E3d recorded for 35, 37 and 38 is a read that never answered, and
the process stayed alive through all of them.

### Verdicts

| # | Verdict | Commit | Where | Test |
| --- | --- | --- | --- | --- |
| 35 | **Fixed, and confirmed natively.** Not a Lane-scoping fault and not H6's: the stage never left the client because the `invoke` carrying it never reached the adapter. `pump_core_events_once` now *tries* the lock rather than waiting for it — a command holding it is answering a gesture, and the events this round does not drain stay queued — and does not return until the lock has been free for `PUMP_YIELD`, so a blocked command has an unlocked mutex to take. The drain window drops from 250 ms to `PUMP_DRAIN`, which also cuts the wake latency by an order of magnitude. Through the shell with the pump running, the Lane-scoped stage is on the wire as Core's own `stage` verb, Core's `git_add` approval reaches the dock, and answering it completes the action. Natively: `Stage all` raises `git_add` in the Decision Center, answering it stages the file, and `source.stage` is written to `audit.jsonl` twice over the run | `22eb5af4` | `apps/gui/src-tauri/src/shell.rs` (`pump_core_events_once`, `PUMP_DRAIN`, `PUMP_YIELD`), `apps/gui/src-tauri/src/lib.rs:1447` (`spawn_core_event_pump`) | `the_desktop_pump_leaves_the_adapter_lock_free_for_the_operators_own_command`, `a_lane_scoped_stage_reaches_the_permission_dock_through_the_shell` |
| 25 | **Still open, and now past the staging wall.** With the stage decidable, E3b's steps 19–21 run end to end through the shell with a Lane selected: stage under `git_add`, commit under `git_commit`, and the `git_push` half of `Commit & Push` reaching the dock and settling with Core's own typed `no_upstream`. Natively the stage and both commits go through with Core's own approvals and durable audit rows, and the `git_push` half then raises no approval at all — the same wall E3b step 22 hit. Still open, and not on the client side of the stage path | `22eb5af4` | as above | `the_commit_and_push_pair_runs_through_the_shell_with_a_lane_selected` |
| 36 | **Client half fixed and confirmed natively; the follow-up stays unmet on Core, and is now named.** The client retired a draft it had only *sent*: `queueComposerFollowUp` cleared the composer on any outcome that was not `rejected`, and `queue_follow_up_and_wait` returns `pending` as soon as the command leaves the client, before Core has published anything. The cockpit now marks nothing as submitted on the send, polls the queue's own pending through a new `d1_queue_follow_up_poll` route, retires the text only on a `confirmed` outcome backed by Core's `InputQueued`, and otherwise renders a `role=alert` saying the follow-up was sent and not acknowledged — with the text still in the box. **The other half is Core's:** its accept path for `RuntimeCommand::QueueFollowUp` is guarded `owner.lane_id.is_none()` (`crates/runtime/src/runtime_supervisor.rs:1612`), so it pushes and announces a *session*-scoped follow-up immediately and lets a Lane-scoped one fall through into the single supervisor worker's channel — the worker the unanswered turn is occupying. No `CommandAccepted`, no `InputQueued`, no event at all, which is exactly the `EVENTS #49` before and after that E3d recorded. The GUI sends the Lane's exact submit owner because a follow-up is work for that Lane | `a324ade6` | `apps/gui/src/screens/d1_cockpit.ts:3479` (`queueComposerFollowUp`), `apps/gui/src-tauri/src/shell.rs:182` and `apps/gui/src-tauri/src/lib.rs:877` (`d1_queue_follow_up_poll`), `apps/gui/src/main.ts` (`queueFollowUpPort`) | `a_follow_up_submitted_during_a_turn_is_never_reported_as_queued_unless_core_said_so`; `tests/h7_shell_regressions.spec.ts` "defect 36" |
| 37 | **Logging and retry added; the drop itself is attributed, not proven.** Two things were missing and both are here. The log was never going to appear at `RUST_LOG=info`: this workspace has **no `log` or `tracing` subscriber anywhere**, so a level filter had nothing to enable. `connection_log` writes one `key=value` line per *transition* to stderr unconditionally, never per poll, and `set_connection` is now the only place the state changes. And nothing retried: Core publishes no reconnect command (`GUI-CORE-003`), so the retry is the host re-running the handshake and snapshot it already owns — `maintain_connection`, once per pump round, on a bounded 1/2/4/8/16 s backoff, with the attempt, the countdown, the last transport error verbatim and the give-up state carried on `D6RecoveryProjection.reconnect` and rendered beside `CONNECTING`. **The drop's own cause is not claimed.** The most likely one is the lock starvation above: every symptom E3d recorded — an inert composer, `No Lanes yet`, a state that did not clear across six checks over 60 s, a live process — is a read that never answered. If it recurs, the log says which | `0a5d55ec` | `apps/gui/src-tauri/src/connection_log.rs`, `apps/gui/src-tauri/src/adapter.rs:6737` (`set_connection`) and `:6767` (`maintain_connection`), `apps/gui/src-tauri/src/d6.rs` (`D6ReconnectProjection`), `apps/gui/src/screens/d6_recovery.ts` | `a_severed_core_connection_is_logged_and_retried_with_a_visible_countdown`; `tests/h7_shell_regressions.spec.ts` "defect 37" |
| 38 | **Not reproduced in-process. No fix claimed.** The first Lane's execution is stopped through the recovery surface's own `close_lane` control, a second Lane is created beside it, and both halves of what the native run found inert hold: the new Lane has a Core execution owner, and a read scoped to the *other* Lane resolves to that other Lane with the new one still in the list. E3d observed it once, in the process that later lost its Core connection, and every symptom in it — `The selected Lane has no sole Core execution owner.`, `Close Lane · no_exact_lane`, `Waiting for the selected Lane context.`, a sidebar click that moved nothing — is a read that never answered, which is the shape of the starvation fixed here | `8f194708` | `apps/gui/src-tauri/src/shell.rs` (`d6_send_intent`) | `a_lane_created_beside_a_stopped_lane_keeps_its_owner` |

### The native confirmation

One fresh temporary Git repository under the run's scratch directory, one
commit `895cdd9` ("Add the fixture README"), a `.gitignore` covering `.viden/`
and `.worktrees/`, and a `.viden/config.toml` selecting `provider = "fallback"`
/ `model = "test-local"`. Core minted `prj_1789374856626993000` in
`ws_76287ed4d61b938e`. One Lane, `lane_1789375075442733000`. The bind was taken
at launch through `VIDEN_GUI_WORKSPACE`, the same product path E3c step 27 and
E3d drove. Every frame was captured twice by window id, the second kept, and
every PNG listed was viewed before it was listed.

| # | Step | Reached | Result | Frame |
| --- | --- | --- | --- | --- |
| 1 | `+ New Lane`, built-in agent, TASK, `Create lane ↵`, Core's `lane_create` approval, `Y · Once` | yes | `lane_1789375075442733000`, `⎇ 1 worktree`, Core's own `Preparing worktree (new branch 'viden/lane_1789375075442733000')` | — |
| 2 | Composer `tool edit_file path=README.md old=H7 new=Edited` | yes | `Approve edit_file · edit_file`, `Medium risk`, `new: Edited old: H7 path: README.md`, `Core is working; no owner-scoped status is available. typed`, and the composer label switched to `Queue follow-up` | [`01`](../../../apps/gui/evidence/main-window-interactions/h7-01-followup-sent-not-acknowledged.png) |
| 3 | **Defect 36 — a follow-up submitted while that turn waits on its approval** | yes, **client half closed** | `Summarise what you just changed` was submitted. The composer **kept the text**, `Sent, but Core has not acknowledged the follow-up. Your message is kept.` rendered as a `role=alert` beneath it, no queued row was drawn, and `EVENTS` stayed `#49`. E3d's silence is gone and E3c's honesty is back; **follow-up 16 remains unmet, and the unmet half is Core's** | [`01`](../../../apps/gui/evidence/main-window-interactions/h7-01-followup-sent-not-acknowledged.png) |
| 4 | `Y · Once` on the `edit_file`, then the Lane worktree out of band | yes | ` M README.md`, `# Edited Fixture` — the product's own edit, in the **Lane worktree** | — |
| 5 | **Defect 35 — `Stage all` with the Lane selected** | yes, **closed** | The commit bar renders, `Stage all` presses, and the Decision Center reads **`1 awaiting you`** with `Approve git_add prj_1789374856626993000 · lane_1789375075442733000 · pending · low`, `TOOL INPUT PREVIEW all: true`, Core's own `GUI-CORE-012` note, and `Decision and reason are written to audit audit_…`. E3d had `0 awaiting you` and no `git_*` row at all | [`02`](../../../apps/gui/evidence/main-window-interactions/h7-02-stage-raises-git-add.png) |
| 6 | `Approve once` on the `git_add` | yes | ` M README.md` -> `M  README.md`, and the commit bar renders `Stage completed · viden/lane_1789375075442733000 ↑0 ↓0 · uncommitted changes remain` over Core's own unified hunk `-# H7 Fixture` / `+# Edited Fixture`. Durable in `audit.jsonl`: `source.stage` `phase: authorized` `tool: git_add`, then `source.stage` `phase: completed` | [`03`](../../../apps/gui/evidence/main-window-interactions/h7-03-stage-completed.png) |
| 7 | **Commit under its own approval** | yes | `Approve once` on the `git_commit` committed `ad8b445 Edit the fixture README heading in the Lane`; the Lane worktree went clean. `audit.jsonl`: `source.commit` `phase: authorized` `tool: git_commit`, then `phase: completed` | — |
| 8 | **`Commit & Push ⇡` — the commit half** | yes | A second change was staged and committed the same way through the pair: `d798e8a Add the push-leg line`, with its own `git_add` and `git_commit` approvals and four more durable rows | — |
| 9 | The `git_push` half, `NoUpstream`, set upstream, push accepted | **no** | The commit bar stands on `Push is waiting for approval in the permission dock.` with `Commit & Push ⇡` disabled, the Decision Center reads `0 awaiting you`, and **no `git_push` approval and no `source.push` row ever appeared** — watched for 60 s. The fourteen rows in `audit.jsonl` are four `approval.allow_once`, two `source.stage` pairs and two `source.commit` pairs, and nothing else. **This is defect 25, unchanged and still open** | [`04`](../../../apps/gui/evidence/main-window-interactions/h7-04-push-half-no-approval.png) |

**So the wall moved exactly one step.** E3d could not stage; this run stages,
commits, and does it twice, with Core's own approvals and Core's own durable
audit rows. What it still cannot do is push, and the `git_push` half fails the
way E3b step 22 and E3d recorded it: the action is sent, the bar says it is
waiting, and no approval is ever raised. Defect 35 is closed; defect 25 is not,
and its remaining cause is not on the client side of the stage path.

### Observations, not defects

- **An approval has a five-minute life, and this run lost one to it.** The
  first `git_add` approval was raised correctly and then expired while the run
  was reading state; answering it afterwards returned Core's own
  `approval ... is no longer pending`. The stage after it then stood on
  `Stage sent · waiting for Core.` with no approval at all, and only a relaunch
  cleared it — so **an operator-git action whose approval expires can leave the
  next one unable to raise its own**, which was not driven further here and is
  recorded rather than numbered. Every leg that answered its approval promptly
  went through on the first try.
- **The permission dock does not render a Lane-scoped `git` approval; the
  Decision Center does.** Both `git_add` approvals appeared in D2 and not in
  the D1 dock, which is where E3b found them too. Not investigated, and not
  claimed as a defect.
- **The connection log is live in the shipped bundle.** Both launches wrote
  `viden-gui core-connection state from=disconnected to=connecting` and
  `... to=live` to the process's stderr with no configuration, which is what
  defect 37's entry promised.

### The static half

`apps/gui/tests/shell_port_contract.spec.ts` walks the sources for the three
joints of the shell path that are matched by name at runtime and silent when
they drift: every `invoke` name against `generate_handler!`, every `CoreClient`
member against its Tauri implementation, and every `D1RenderOptions` port
against the object `main.ts` hands `renderD1Cockpit`. None of them is a type
error — the options bag is optional by design — and an unwired port is
`undefined` at a call site guarded with `if (!port) return`, which is a silent
no-op rather than a rejected promise. It found one gap in this batch's own
work, a `d6_recovery` command no port called, and that command is gone
(`9766e279`).

### Verdict on compatibility follow-up 16

| # | Verdict on the candidate |
| --- | --- |
| 16 | **Still unmet on the GUI, and no longer silent.** The composer keeps the operator's text, announces that the follow-up was sent and not acknowledged, and never reports a queue Core did not confirm. Meeting the follow-up needs Core to accept a Lane-scoped `QueueFollowUp` on its accept path the way it already accepts a session-scoped one (`crates/runtime/src/runtime_supervisor.rs:1612`); no GUI change can substitute for it, and none was attempted |

### What is still owed

- **E3e, natively:** the `git_push` half of `Commit & Push`, `NoUpstream`, the
  upstream and the push — defect 25, the only step of the source-control half
  this run could not reach; defect 31's `Always`/revoke leg and the Plan Studio
  leg, both still not driven since E3c; and the expired-approval observation
  above, which was recorded rather than pursued.
- **On Core:** the Lane-scoped `QueueFollowUp` accept path (defect 36's other
  half, compatibility follow-up 16), and whatever keeps the `git_push` half
  from raising an approval (defect 25).

## Core Resolutions

Core-side closures for defects and follow-ups this file recorded against the
GUI. A Core closure is an in-process claim: the behaviour is held by Core's own
tests, and the native confirmation is owed to the next GUI run.

### C18 — 2026-09-14, on `claude/core-lane-queue-c18`

| # | Resolution | Evidence |
| --- | --- | --- |
| 36 (Core half) / follow-up 16 | **Closed in-process.** The accept path's `QueueFollowUp` arm was guarded `owner.lane_id.is_none()`, so a Lane-scoped follow-up did not match it and fell through into the single supervisor worker's channel — the worker the Lane's unanswered turn was occupying — publishing no `CommandAccepted`, no `InputQueued`, no event at all. That is the `EVENTS #49` before and after this file recorded at E3d and named at H7. A Lane's entry now goes onto a per-Lane queue the runtime owns, is announced on the accept path with the Lane's own owner on `QueuedInputView.owner`, and is drained by a `TurnFinished { completed }` owned by that Lane: `InputDequeued` first, then the entry's own Lane-owned turn. A failed or cancelled Lane turn keeps the queue; killing the Lane's Agent session discards it and announces each discarded entry; a queued entry is not durable and does not survive a Core restart | `crates/runtime/src/tests/lane_queue_tests.rs` (four tests, the first asserting the acknowledgement arrives while the Lane turn is still parked on its approval); `runtime_supervisor_kill_discards_the_lanes_follow_up_queue` |

**What the GUI must still do.** Nothing new on the send path: H7 already sends
the Lane's exact submit owner and already retires the composer text only on a
`confirmed` outcome backed by Core's `InputQueued`, which is exactly the fact
Core now publishes. Two things to check when consuming it — read
`QueuedInputView.owner` rather than assuming one queue, since the session's
entries and each Lane's are separate and are released by different turns; and
keep treating `InputDequeued` as "left the queue", because a discard on kill
emits it without a `TurnStarted` naming the entry.

**Still owed:** native confirmation at E3e. Nothing here was driven through the
real GUI.

## Defect 25 — H8 (2026-09-14, on `claude/gui-push-half-h8`)

Branch `claude/gui-push-half-h8` in `.worktrees/gui-push-half-h8`, base
`claude/int-0.3.5` at `b2aacbe1` (H7 merged). No `crates/**` change, no GUI
version bump, nothing under `apps/gui/manifests/` or `docs/viden-design/**`.

**Verdict: defect 25 is closed — in process, and natively on the built
bundle.** The `git_push` half of `Commit & Push ⇡` now reaches the dock and
the Decision Center, Core's typed `no_upstream` refusal renders with its
`Push and set upstream` recovery, that recovery raises its own second
`git_push` approval, and the accepted push puts the Lane's branch in a bare
`origin` with four `source.push` rows in `audit.jsonl`.

### What was ruled out first

The shell path was driven exactly as the client drives it, in
`apps/gui/tests/h7_shell_path.rs`
(`the_push_half_sent_from_the_commits_own_observation_reaches_a_decidable_surface`):
the push is sent from inside the poll that carried the commit's own
`Completed`, with no settle round in between — `advanceCommitPair`'s timing,
which no earlier test used. It was **green on the first run**. Core receives
the `RunOperatorGitAction`, raises `git_push`, the dock holds it, and the
action settles. None of the brief's three suspects — the in-flight slot, the
selection-tagged read, `runOperatorGit` re-entrancy — reproduces, so none of
them is the cause.

### The cause

RED, from `apps/gui/tests/h8_push_half.spec.ts` before the fix:

```text
AssertionError: the bar says `Push is waiting for approval in the
permission dock.` while the permission dock holds nothing to decide —
E3b defect 25, a push that is waiting on an approval the operator cannot
reach: expected '' to contain 'git_push'
```

The test's own assertion that the push was sent passes; only the surface
fails. The desktop host wires `onCoreWake`, so `renderD1Cockpit` sets
`coreWakeActive` and `schedulePoll` returns at once
(`apps/gui/src/screens/d1_cockpit.ts:3328` at the base): the cockpit
projection is re-read only on a pump wake, and the pump wakes only for events
*it* drained. `GuiCoreAdapter::poll_operator_git`
(`apps/gui/src-tauri/src/adapter.rs:3259`) drains inside the command, so the
push's `ApprovalRequested` is consumed there and the pump never sees it — and
Core then says nothing more until that approval is answered. The commit bar
knew (its projection is the command's own return value); the dock, the
Decision Center and the rail badge did not. Stage and commit survived by
timing: between two separate gestures the pump had other traffic to wake on.
The push is sent from inside `advanceCommitPair`, and its approval is the last
thing Core publishes.

### The fix

`refreshCockpitAfterOperatorGit` in `apps/gui/src/screens/d1_cockpit.ts`,
called from `runOperatorGit` and `noteOperatorGitPending`, runs the same
`drainOnce` the wake runs after every operator-git answer, bounded by that
answer's signature so an unchanged poll does not loop. Two honesty rules land
with it: the commit draft is retired only once the push has left the client
(`runOperatorGit` now reports whether it did), and a completed commit whose
push never left renders `d1.review.actionPushNotSent` with a `Push` control
(`onPush`).

| Commit | What |
| --- | --- |
| `6c82f69b` | Shell harness: the client's push timing, the `no_upstream` → set-upstream → bare-`origin` leg, and `settle` waiting on Core's D1 slot instead of eight fixed rounds (it failed 3/3 with eight tests in parallel, and is 3/3 green on the slot wait) |
| `c7ef4bf0` | The fix and the webview tests, with the RED text above |

### The native confirmation

Taken. The host screen read `CGSSessionScreenIsLocked=0` before every
keystroke and click batch, from `time=2026-09-14T09:41:19Z` (the first check)
through `time=2026-09-14T10:09:53Z` (the last input). Bundle
`target/release/bundle/macos/Viden.app` built from `c7ef4bf0`; `VIDEN_HOME`
and a fresh one-commit fixture under the run's scratch directory; bind through
`VIDEN_GUI_WORKSPACE`. Launched PIDs 34026 and 4319, both killed by this
batch. Frames are window-id captures, each viewed before it was listed, in
`apps/gui/evidence/main-window-interactions/` (see its `EVIDENCE.md`).

| # | Step | Reached | Result | Frame |
| --- | --- | --- | --- | --- |
| 1 | `+ New Lane`, built-in agent, `lane_create` approval, `Y · Once` | yes | `lane_1789379401566830000`, `⎇ 1 worktree` | — |
| 2 | Lane worktree `README.md` changed out of band, `Stage all`, `git_add`, `Y · Once` | yes | `Stage completed · viden/lane_… ↑0 ↓0 · uncommitted changes remain` | — |
| 3 | `Commit & Push ⇡` with `Add the push-leg line`, `git_commit`, `Y · Once` | yes | The commit lands; the working tree goes clean | — |
| 4 | **The `git_push` half** | **yes — defect 25 closed** | `Push is waiting for approval in the permission dock.` **with `Approve git_push · git_push` on the dock beneath it**, `remote: origin`, the rail badge at `1`, the status bar at `1 awaiting you` | [`h8-01`](../../../apps/gui/evidence/main-window-interactions/h8-01-push-approval-on-the-dock.png) |
| 5 | `Y · Once` on the push, no remote configured | yes | Core's typed `no_upstream`: `Push failed. This branch has no upstream…` with git's own line and the `Push and set upstream` control | [`h8-02`](../../../apps/gui/evidence/main-window-interactions/h8-02-no-upstream-and-set-upstream.png) |
| 6 | Bare `origin` added out of band, `Push and set upstream` | yes | Its **own** second `git_push` approval, `set_upstream: true`, `1 awaiting you` | [`h8-03`](../../../apps/gui/evidence/main-window-interactions/h8-03-set-upstream-raises-its-own-approval.png) |
| 7 | `Y · Once` | yes | `Push completed · viden/lane_1789379401566830000 ↑0 ↓0 · working tree clean`; the bare repository holds `viden/lane_1789379401566830000` at `4092adc` | [`h8-04`](../../../apps/gui/evidence/main-window-interactions/h8-04-push-completed.png) |

`audit.jsonl` ends with four `source.push` rows: `phase: authorized, tool:
git_push`; `class: no_upstream, phase: failed`; `phase: authorized`;
`phase: completed` — the first `source.push` rows any run has written.

### Observations, not defects

- **The first launch froze after a coordinate click into the commit-message
  box followed by `⌘V`**: the centre went to `CONNECTING / Establishing the
  versioned Core connection.`, the rail went disabled and `REQ 1 req` stood
  still, while the connection log stayed on `to=live`. A relaunch recovered
  the bound Lane with its staged change intact, and the second launch drove
  every step above. Not reproduced and not claimed as a defect; it is the
  shape defect 37 named — a read that never answered — and is left for E3e.
- **Synthetic keystrokes did not reach the webview's text fields on this
  host**: the active input method turned `keystroke` into Chinese candidates
  in the New Lane `TASK` box, and backspace/paste into the commit-message box
  were dropped. The commit message was therefore set through the
  Accessibility value of the field, which fires the field's own `input` event.
  The `TASK` text of the Lane is not what was intended; nothing downstream
  reads it.
- No frame was taken for steps 1–3; they are the stage-and-commit half H7
  already recorded natively.

## Native GUI Run — 2026-10-10 (0.3.5 candidate `3c652fa8`, E3e)

This is E3e: the remaining Lane steps driven through the native cockpit after
H7, C18 and H8. Branch `claude/evidence-e3e` in `.worktrees/evidence-e3e`,
base the integration branch `claude/int-0.3.5` at
`3c652fa8` — E3d's base plus H7 (the desktop pump no longer starves the
operator's command, a follow-up is kept until Core's `InputQueued`, the Core
connection is logged and retried), C18 (Core accepts, queues and drains a
Lane-scoped `QueueFollowUp`) and H8 (the `git_push` half of `Commit & Push ⇡`
reaches the dock).

The host screen was unlocked and `CGSessionCopyCurrentDictionary()` was read
before every keystroke and click batch; `CGSSessionScreenIsLocked` was `0` at
every one of them (131 recorded checks), from `2026-10-10T08:11:26Z` to
`2026-10-10T08:48:57Z`.

**The batch's headline: the real task completes through the native GUI.** A
Lane created under Core's approval took its TASK as a turn, the product's own
`edit_file` changed the Lane worktree and nothing else, a follow-up queued
during that turn was acknowledged by Core and drained into the Lane's next
turn, and the Lane's work was staged, committed and pushed — `git_add`,
`git_commit` and `git_push` each decided on the dock, Core's typed
`no_upstream` answered with `Push and set upstream`, and the bare `origin`
holds the Lane's branch. C15's `Always` and its revoke behave natively, and a
plan went from `update_plan` through Plan Studio's Accept to a hand-off and
D13's DAG.

Two GUI blockers were found on the way and fixed in this batch, red-first,
each in its own commit, under the launch authorization: defect 38 (a Lane
selection never re-read the cockpit on a quiet Core —
`5afb01b0 fix(gui): re-read the cockpit when the operator selects a Lane on a quiet Core`)
and defect 41 (Plan Studio's and the Agent Board's intents were refused by
the host's field names —
`4c92b511 fix(gui): read the Plan Studio and Agent Board intents in the webview's field names`).
One new finding is Core's and is numbered and stopped (42). No `crates/**`
file was edited.

### The application under test

`npm ci` completed in 3 s (the `added N packages` line was not kept by this
run's log tail). `npx tauri build --bundles app` PASSED — `Finished \`release\`
profile [optimized] target(s) in 55.36s` — and produced
`target/release/bundle/macos/Viden.app` at `2026-10-10T08:13:38Z`:

| Fact | Value |
| --- | --- |
| `CFBundleShortVersionString` | `0.1.0-rc.5` |
| `CFBundleIdentifier` | `dev.viden.gui` |
| `CFBundleExecutable` | `viden-gui` |
| Architecture | `Mach-O 64-bit executable arm64` |
| Executable size | 42,875,296 bytes |
| Bundle size | 42 MB |
| Signature | ad-hoc, linker-signed, `TeamIdentifier=not set`, `Sealed Resources=none` |
| Executable SHA-256 at `3c652fa8` | `8968dec5a4cdee3a22ab243e7f9495d591c8558019077808bc24ebf37481aa93` |

The bundle was rebuilt twice, after each blocker fix, with the same toolchain
and the same facts other than the digest: at `5afb01b0`
(`Finished … in 18.23s`, SHA-256 `7c25570f…1d39`) and at `4c92b511`
(SHA-256 `6b686c82…6e1a`). The bundle still carries `0.1.0-rc.5`; R1 owns the
`rc.6` bump. It was launched from the bundle, never installed, signed with an
identity, notarized or distributed. Every launch ran with `VIDEN_HOME` under
the run's scratch directory, so this repository's own `.viden/` was never read
or written. Launched PIDs, in order, with the bundle each ran:
80448 (`3c652fa8`), 32732 and 18414 (`5afb01b0`), 46950 and 56638
(`4c92b511`). All five were killed by this batch; none exited on its own.

### Fixture

One fresh temporary Git repository under the run's scratch directory, with one
commit `497ba79` ("Add the fixture README"), a five-line 97-byte `README.md`, a
`.gitignore` covering `.viden/` and `.worktrees/`, and a `.viden/config.toml`
selecting `provider = "fallback"` / `model = "test-local"`. A bare repository
created by this run was configured as the fixture's `origin` from the start,
with no upstream on any branch, so Core's `no_upstream` refusal and the GUI's
own set-upstream recovery are what the push leg exercises. Core minted
`prj_1791620100850307000` in `ws_0c19e0480d6091d6`. Four Lanes were created:
`lane_1791620201547372000` (Lane 1), `lane_1791620454348932000` (Lane 2),
`lane_1791621924201135000` (Lane 3) and `lane_1791621962889570000` (Lane 4).

### Two deviations from the scripted task, and why

**The bind was taken at launch through `VIDEN_GUI_WORKSPACE`, not through
`⌘O`**, exactly as E3d, H7 and H8 did, for E3d's recorded reason (the open
panel's child window takes no key event on this host). The cockpit is bound
the way Core binds it; only the operator gesture differs.

**The push completed on a second `Commit & Push ⇡` pair, in a relaunched
process.** The first pair (process 32732) reached the `git_push` approval on
the dock — `Approve git_push · High risk`, `Push is waiting for approval in
the permission dock.` — and before it was answered the window moved to another
display, was zoomed, and its rail switched to `Lanes`, which rendered the
pre-hydration `CONNECTING` shell (defect 43). No input of this run's own
appears between the read that showed the approval and the read that showed
`CONNECTING`; the run's log has a capture there and nothing else. The
cockpit did not leave that shell over about 100 s of reads (the `Esc` that
does leave it was found later, in step 21), so the process was killed, and the
pending push died with its in-process Core: no `source.push` row was written
for it. The relaunched process had no push affordance for a committed,
unpushed branch with no upstream (observation below), so a second product
edit was made and the whole pair re-run. Nothing on the second pair was done
out of band: `origin` had been configured with the fixture.

### Step by step

Every frame was captured twice by window id and the second kept, every PNG
listed was viewed before it was listed, and none is under 30 KB. Frames live
under `native-0.3.5/` with the `e3e-` prefix. Two captured frames were
dropped: one taken after the window had already left the state it was named
for, and one byte-identical to `e3e-34` (the `Esc` return, which therefore is
`34`). Where a frame cannot show a fact — Plan Studio's decision row is
occluded at this window size (defect 44) — the Accessibility read is the
record and is quoted.

| # | Step | Reached | Result / reason | Frames |
| --- | --- | --- | --- | --- |
| 1 | **Bound cockpit** | yes | Dock `Provider fallback`, `Model test-local`, `Mode build`, `Permission ask`, `Tokens 0`; `Showing Workspace root`, `Branch main`, `Working tree Clean`; `MODE build · PERM ask · EVENTS #7 · LANE — · DIAG 0✕ · REQ 0 req / 0 err` | [`01`](native-0.3.5/e3e-01-cockpit-bound.png) |
| 2 | **`⌘L` New Lane, built-in agent, TASK** | yes | `Viden Agent built-in` selected; Codex, Claude Agent and Kiro CLI disabled under `Checking ACP agents… each probe is given up to 15s.`; with the TASK set through the field's Accessibility value, `Create lane ↵` enabled and the branch read `⌁ vd/edit-the-fixture-readme-he` | [`02`](native-0.3.5/e3e-02-new-lane-task-set.png) |
| 3 | **Core's `lane_create` approval** | yes | `Approve lane_create · lane_create`, `Medium risk`, the Lane's branch and worktree path, over `Y · Once` / `Allow repo paths` / `Shift+A · Always · scope_not_offered` / `E · Edit · not_editable` / `N · Deny` — register 019 still closed live | [`03`](native-0.3.5/e3e-03-lane-create-approval.png) |
| 4 | **The Lane, and its TASK as a turn — defect 22, re-confirmed** | yes | `Y · Once`: tab `lane_1791620201547372000`, `⎇ 1 worktree`, `Showing Lane worktree`, Core's own `HEAD is now at 497ba79 Add the fixture README` / `Preparing worktree (new branch 'viden/lane_1791620201547372000')`, `EVENTS #45`, `REQ 1 req / 0 err`. Session JSONL: `turn_owner` `lane_id: lane_1791620201547372000`, `turn_id: turn_1791620215932178000`, the user message, a cost row `outcome: success` on `fallback` / `test-local`, the reply, `turn_owner` cleared | [`04`](native-0.3.5/e3e-04-lane-created-turn.png) |
| 5 | **Defect 28 — the Lane's tool runs in the Lane worktree, re-confirmed** | yes, **closed** | `tool edit_file path=README.md old=Fixture new=Edited` raised `Approve edit_file`, `Medium risk`, Core's typed hunk `@@ -1,5 +1,5 @@` `-# E3e Fixture` / `+# E3e Edited`, `Shift+A · Always` enabled, the label `Queue follow-up`. After `Y · Once` Core's result read `edit_file completed` / `path: …/fixture/.worktrees/lane_1791620201547372000/README.md` / `size: 96 B`. Out of band: the workspace root is clean, `# E3e Fixture`, 97 bytes; the Lane worktree is ` M README.md`, `# E3e Edited`, 96 bytes. `tool shell command=pwd` in the Lane printed `…/fixture/.worktrees/lane_1791620201547372000` | [`05`](native-0.3.5/e3e-05-edit-file-approval-hunks.png), [`07`](native-0.3.5/e3e-07-followup-drained-lane-worktree-edit.png) |
| 6 | **Follow-up 16 / defects 29 and 36 — a follow-up during a Lane turn, and C18's drain** | yes, **met** | With that `edit_file` turn parked on its approval, `Summarise what you just changed` was submitted. Core acknowledged it: the live-work strip drew **`1 queued · runs after the current turn`** with the row `Medium risk · Summarise what you just changed` under the Lane, `EVENTS` `#48 → #50`, and the composer was empty after the acknowledgement. (The interval between the send and Core's `InputQueued` was shorter than one Accessibility read, so "kept until acknowledged" is H7's test's claim and was not separately observed here.) `Y · Once` then completed the turn and Core drained the queue into a **new Lane-owned turn**: session JSONL `turn_owner` `lane_id: lane_1791620201547372000`, `turn_id: turn_1791620294134838000`, user message `Summarise what you just changed`. E3c refused it, E3d dropped it silently, H7 kept it unacknowledged; **E3e sees Core take it and run it** | [`06`](native-0.3.5/e3e-06-followup-queued.png), [`07`](native-0.3.5/e3e-07-followup-drained-lane-worktree-edit.png) |
| 7 | Cancel a turn that has a queued follow-up | **no** | `tool shell command=ls` parked on its approval; `Held behind a cancelled turn` queued (`1 queued`, row drawn). `Cancel turn · Esc` in the live-work strip did nothing — no event (`EVENTS #158` throughout), no alert; the `Esc` key did nothing; `Cancel active turn` under the composer was **refused**: `Not sent. This client is still waiting for Core's answer to submit_user_input, asked 1s ago. Your message is kept.` The turn cannot be cancelled while it waits on an approval (defect 39). `N · Deny` then ended the turn, and Core drained the queued follow-up into Lane-owned `turn_1791620434365713000` — the queue survives a denied tool and runs, which is C18's completed-turn rule | [`08`](native-0.3.5/e3e-08-cancel-leg-queued.png), [`09`](native-0.3.5/e3e-09-cancel-refused-slot-held.png) |
| 8 | **A second Lane** | yes | `⌘L`, TASK `Second Lane for the selection check`, `Create lane ↵`; the approval landed in the Decision Center (`GATE APPROVALS · 1`, `Approve lane_create prj_… · lane_1791620454348932000 · pending · medium`), `Approve once` released it, `⎇ 2 worktrees`, and its TASK ran as Lane-owned `turn_1791620478025146000`. While the creation was pending the centre drew `AGENT STOPPED` and the composer `The selected Lane has no sole Core execution owner.` | [`10`](native-0.3.5/e3e-10-lane2-pending-creation.png) |
| 9 | **Defect 38 — reproduced natively, root-caused, fixed in this batch** | yes, **fixed** | With Lane 2 just created, pressing Lane 1's sidebar row — by Accessibility press and by a real HID click — moved nothing: titlebar and status bar stayed on Lane 2, the dock read `Waiting for the selected Lane context.`, the composer `The selected Lane has no sole Core execution owner.`, the recovery surface `Close Lane · no_exact_lane`, and `Diff review` drew **Lane 1's** `README.md` change under Lane 2's name. The connection log read `to=live` with no later transition, and a `sample` of the process showed the pump idle in `receive_event` and **no thread waiting on the adapter mutex** — so this is not H7's starvation. Cause: on the desktop the cockpit projection, which the host scopes to the Lane it is asked about, is re-read only on a pump wake, and a selection is not a Core event; `selectLane` re-read the transcript and the commit bar's owner and never the projection. Fixed in `5afb01b0`; see defect 38 below | [`11`](native-0.3.5/e3e-11-defect38-reproduced.png), [`12`](native-0.3.5/e3e-12-diffreview-stale-scope.png) |
| 10 | **The fix, natively, and defect 34 re-confirmed** | yes | Relaunched on the fixed bundle against the same home: `lane-agent-bindings.jsonl` took `lane-agent-rebound:…:session_1791620855028201000` for **both** Lanes, and the selection now moves both ways — Lane 2's row puts `lane_1791620454348932000` in the titlebar and status bar, Lane 1's row puts `lane_1791620201547372000` back, the composer accepts, no owner refusal | [`13`](native-0.3.5/e3e-13-select-lane2-after-fix.png) |
| 11 | **Defect 30 — the commit bar resolves the selected Lane** | yes, **closed** | Lane 2 selected: the bar reads `0 files · +0 −0`, `No changes in the working tree.`, the dock `viden/lane_1791620454348932000`, `Clean`. Lane 1 selected: `1 files · +1 −1`, `M README.md +1 −1`, Core's hunk, the dock `viden/lane_1791620201547372000`, `Dirty`. Each Lane's own facts, and no `D1-OPERATOR-GIT-OWNER` refusal anywhere. Before the bar is opened, though, the dock keeps the previous Lane's sample (defect 40) | [`14`](native-0.3.5/e3e-14-commit-bar-lane2-clean.png), [`15`](native-0.3.5/e3e-15-commit-bar-lane1-dirty.png) |
| 12 | **Stage and commit with the Lane selected** | yes | `Stage all`: `Stage is waiting for approval in the permission dock.` **with `Approve git_add · git_add`, `Low risk`, `all: true`, `Target: Source control · git_add (lane:lane_1791620201547372000)` on the dock beneath it**; `Y · Once` → ` M` became `M  README.md` and `Stage completed · viden/lane_1791620201547372000 ↑0 ↓0 · uncommitted changes remain`. The message `Edit the fixture README heading in the Lane` set, `Commit & Push ⇡` raised `Approve git_commit`, `Medium risk`; `Y · Once` committed `b19a0da` | [`16`](native-0.3.5/e3e-16-stage-git-add-on-dock.png) |
| 13 | **Defect 25 — the `git_push` half reaches the dock** | yes | Straight after the commit: `Push is waiting for approval in the permission dock.` over `Approve git_push · git_push`, `High risk`, `Target: Source control · git_push (lane:lane_1791620201547372000)`, `⏸ 1 awaiting you` (Accessibility read; no frame, because the state was left before the capture — see the deviation above and defect 43) | — |
| 14 | **The push, end to end — defect 25 closed natively** | yes, **closed** | On the relaunched process: a second product `edit_file` in Lane 1 (into the Lane worktree), `git_add` once, message `Add the push-leg line`, `Commit & Push ⇡`, `git_commit` once, `git_push` once — Core's typed refusal: **`Push failed. This branch has no upstream, so ahead and behind would be unknowable. Push once with set-upstream to create one.`** with git's own `the current branch has no upstream branch; push with set_upstream to create one` and **`Push and set upstream`**. That control raised its **own** second `git_push` approval; `Y · Once` → **`Push completed · viden/lane_1791620201547372000 ↑0 ↓0 · working tree clean`**. Out of band: the bare `origin` holds `viden/lane_1791620201547372000` at `7dfbad5 Add the push-leg line` over `b19a0da` over `497ba79`, and the Lane branch tracks `origin/viden/lane_1791620201547372000`. `audit.jsonl` carries four `source.push` rows: `authorized` `git_push`; `class: no_upstream, phase: failed`; `authorized` `git_push`; `completed` | [`19`](native-0.3.5/e3e-19-no-upstream-refusal.png), [`20`](native-0.3.5/e3e-20-push-completed.png) |
| 15 | **Defect 31 — `Always`, the auto-allowed repeat, the revoke, the row leaving the list** | yes, **closed** | `tool shell command=date`: `Shift+A · Always`; `audit.jsonl` took `approval.always` on `approval_1791621269740437000`. The identical input again: **no request, no new audit row** (25 rows before and after), and the session holds two `shell` completions. Settings → `Standing approvals`: `shell · command=date`, `granted 1791621273 in ws_0c19e0480d6091d6`, `Revoke`, `Open the audit trail`. `Revoke`: the list reads **`No standing rule is installed.`** at once (no `Save` needed), and still does after the panel is closed and reopened; `approval.revoke` is in `audit.jsonl`. The identical input then **asked again** — `Approve shell`, `1 awaiting you` — and was denied | [`21`](native-0.3.5/e3e-21-standing-approval-listed.png), [`22`](native-0.3.5/e3e-22-revoked-row-left-list.png), [`23`](native-0.3.5/e3e-23-identical-input-asks-again.png) |
| 16 | **Plan mode, and an `update_plan` proposal in D15** | yes | The work-mode pill's popover: `Plan` → `MODE plan`, `PERM read_only` (defect 27 re-confirmed: the slot settled). `tool update_plan plan=[{"step":"Retitle-the-README",…},{"step":"Commit-the-retitle",…,"dependencies":["step_1"]}] title=E3e-handoff-plan goal=Prove-the-Plan-to-Build-handoff`, set through the composer's Accessibility value with both quotes read back `U+0022`, was accepted by Core: `Recorded the plan \`E3e-handoff-plan\`: 2 step(s)`. Plan Studio: `PLAN · READ ONLY`, `E3e-handoff-plan`, `plan_1791621401009792000`, owner `lane_1791620201547372000`, `Proposed`, `rev 1 · sha 93aaa047…7b3f`, both steps `PENDING`, `step_2 after step_1` | [`24`](native-0.3.5/e3e-24-plan-studio-proposal.png) |
| 17 | **Accept — refused by the host, defect 41, fixed in this batch** | yes, **fixed** | `Accept ⌘⏎` came back `refused` / **``invalid args `intent` for command `d15_send_intent`: missing field `plan_id` ``** (Accessibility read; the decision row is occluded in the frame, defect 44). The webview sends camelCase `planId` and the host read snake_case. Fixed in `4c92b511`, with `D16Intent` (`sessionId`) fixed alongside | [`25`](native-0.3.5/e3e-25-plan-studio-decision-row-occluded.png) |
| 18 | The proposed plan across a relaunch | **no** | On the fixed bundle, against the same home, Plan Studio read **`No plan proposed`** / `Core holds no plan for this owner.` for Lane 1, while `agents.jsonl` still carries the `plan_document_updated` for `plan_1791621401009792000`. Defect 42, Core's, numbered and stopped | [`26`](native-0.3.5/e3e-26-plan-gone-after-relaunch.png) |
| 19 | **Accept, Hand off refused in Plan mode, Build, Hand off, D13's DAG** | yes | A new `update_plan` in Plan mode → `plan_1791621781419990000`. `Accept` → **`Accepted`**, `decided 2026-10-10 08:43:14 UTC · audit_1791621794526741000`. `Hand off` then disabled with Core's code **`plan_mode`**: `Handing a plan off starts agent work, which Core refuses while plan mode is active. Accept in Plan, switch to Build, then hand it off.` The pill → `Build`: `MODE build`, `PERM ask` — and Plan Studio kept the `plan_mode` refusal until it was closed and reopened (defect 45). Reopened: `Hand off` enabled; pressed: **`Handed off to dag_1791621840994851000 · 2 tasks · audit audit_1791621840994689000`**, status `Handed off`. `Open in Fleet & Workflow ↗` → D13: `E3e-handoff-plan · active`, `Retitle-the-README coder queued`, `Commit-the-retitle coder queued`, `depends on: plan_1791621781419990000-step_1`. **The Plan Studio hand-off is evidenced natively for the first time.** The refusal in Plan mode is the client's disabled control carrying Core's code; Core's own refusal was not provoked | [`27`](native-0.3.5/e3e-27-accepted-handoff-refused-plan-mode.png), [`28`](native-0.3.5/e3e-28-build-mode-handoff-still-refused.png), [`29`](native-0.3.5/e3e-29-handed-off-dag.png), [`30`](native-0.3.5/e3e-30-d13-dag.png) |
| 20 | **Defect 33 — centre views over a stopped Lane, re-confirmed** | yes, **closed** | `tool edit_file path=README.md old=NoSuchText new=Whatever`, allowed, failed; the centre drew `AGENT STOPPED` / `Core reports that execution has stopped.` with `Reconnect · GUI-CORE-003`, `Inspect facts`, `Restart agent · no_retryable_session`, `Close Lane · no_exact_lane`, `Restore checkpoint · GUI-CORE-003`. `Diff review` switched the centre (the recovery surface left the tree, `CHANGES` rendered), `Plan Studio` switched it (`PLAN STUDIO` rendered), and `Conversation` brought the recovery surface back | [`31`](native-0.3.5/e3e-31-agent-stopped.png), [`32`](native-0.3.5/e3e-32-plan-studio-over-stopped.png) |
| 21 | **Defect 38 — a Lane created beside a stopped Lane, twice** | yes, **closed** | With Lane 1 stopped: Lane 3 (`lane_1791621924201135000`) and Lane 4 (`lane_1791621962889570000`), each through `⌘L`, its TASK, and `Approve once` in the Decision Center. Each has an exact owner — the dock's `Lane Agent` names it, the composer is enabled with no owner refusal, its TASK ran as a turn in `session_1791621678660570000`, `lane-agent-bound` rows for both — and the selection moves between them. Both nevertheless drew Lane 1's `AGENT STOPPED` surface in their own conversation slot (defect 46), and the dock's `Local` kept Lane 1's branch (defect 40) | [`33`](native-0.3.5/e3e-33-lane3-beside-stopped-has-owner.png), [`34`](native-0.3.5/e3e-34-lane4-beside-stopped-has-owner.png) |
| 22 | **Defect 37 — the connection log, across the whole run** | yes | All five processes wrote exactly `viden-gui core-connection state from=disconnected to=connecting` and `… from=connecting to=live`, and **nothing after** — no drop, no `reconnect-attempt`, so no D6 countdown was ever due. The one `CONNECTING` this run saw (deviation above) was not a transport state: the log stayed `to=live`, and it is reproducible on demand from the `Lanes` rail (defect 43) | [`18`](native-0.3.5/e3e-18-connecting-while-host-live.png), [`38`](native-0.3.5/e3e-38-lanes-rail-renders-connecting.png) |
| 23 | Agent Board controls | **no**, and not a defect | No ACP binary is usable on this host: `Codex · ACP agent initialize probe failed; inspect local agent logs`, `Claude Agent · Probe did not answer within 15s.`, `Kiro CLI · Probe did not answer within 15s.` No Agent session row can exist to pause, kill or retry. Said once, not faked. (Defect 41's fix covers the board's intent shape at the wire only) | — |
| 24 | **Focus mode `⌘.` and the `Esc` return** | yes | `⌘.` fired this time: `Focus mode ⌘.` `v=1`, the dock hidden, the centre given the window's width with the titlebar, rail, composer and status bar retained. `Esc`: `v=0`, chrome retained, and the window rendered byte-identical to the frame before `⌘.` | [`35`](native-0.3.5/e3e-35-focus-mode.png), [`34`](native-0.3.5/e3e-34-lane4-beside-stopped-has-owner.png) |
| 25 | **Defect 32 — a typed `"` with real keystrokes, re-confirmed** | yes, **closed** | `A"b"c` typed as five HID key events into the composer reads back `41 22 62 22 63`. The input source read `com.apple.keylayout.ABC` before and after with Viden frontmost (recorded below) | [`37`](native-0.3.5/e3e-37-defect32-keystrokes.png) |

### Defects And Gaps Found By E3e

Numbering continues from the thirty-eight above. Each was reproduced, not
inferred. Two were fixed here under the launch authorization (38, 41); the
rest were numbered and stopped.

38. **Reproduced natively, root-caused and fixed** (GUI). E3d observed it once
    and H7 attributed it to lock starvation and could not reproduce it in
    process, because H7's harness has no webview. E3e reproduced it on a live,
    unstarved process (step 9). The desktop sets `coreWakeActive`, so
    `schedulePoll` never runs and the cockpit projection — host-scoped to the
    Lane it is asked about — is re-read only by `drainOnce` on a pump wake. A
    selection is not a Core event; `selectLane`
    (`apps/gui/src/screens/d1_cockpit.ts`) re-read the transcript and the
    commit bar's owner and never the projection, so on a quiet Core the new
    Lane's projection was never read and every symptom E3d listed followed.
    **Fixed 2026-10-10 in this batch** (`5afb01b0 fix(gui): re-read the cockpit
    when the operator selects a Lane on a quiet Core`): a selection owes the
    cockpit one read through the same `drainOnce`, and a drain whose selection
    moved while it was out reads again for the new Lane. RED, against the
    shipped `renderD1Cockpit` with `onCoreWake` wired and no wake fired:
    `no read of the newly selected Lane's projection was sent: on a quiet Core
    the cockpit stays on the previous Lane's projection forever (E3e, defect
    38): expected [] to include 'lane-review'` and `the answer for the
    superseded Lane was the last read: expected 'lane-core' to be
    'lane-review'`. Test: `apps/gui/tests/e3e_lane_selection.spec.ts`.
    Confirmed natively on the rebuilt bundle (steps 10 and 21).
39. **A turn parked on an approval cannot be cancelled from the cockpit**
    (GUI; blocking for the cancel leg of follow-up 16). `Cancel active turn` is
    refused with defect 23's alert — `Not sent. This client is still waiting
    for Core's answer to submit_user_input, asked 1s ago. Your message is
    kept.` — because the cancel goes through the one D1 command slot the
    unanswered submit holds, the same shape H6 removed for the follow-up. The
    live-work strip's `Cancel turn · Esc` and the `Esc` key do nothing at all,
    with no alert. The only way out of the turn is to answer its approval.
    Numbered and stopped; whether Core answers a cancel while the turn waits
    is not established by this evidence, because the cancel never left the
    client.
40. **After a Lane selection the context dock keeps the previous Lane's
    source sample** (GUI, honesty). With the fix for 38 in, the titlebar, the
    status bar, the composer and the commit bar follow the selection, but the
    dock's `Changes` and `Local` sections — and an already-open DiffReview
    centre — keep the previous Lane's branch, worktree and `+1 −1` until
    something re-reads the workspace diff (opening the commit bar does). Seen
    with Lane 2 selected showing Lane 1's `viden/lane_1791620201547372000`
    and `M README.md +1 −1`, and again with Lanes 3 and 4. The same
    no-wake shape as 38, for the diff read instead of the projection.
41. **Plan Studio's and the Agent Board's intents are refused by the host's
    field names** (GUI, blocking for the hand-off). `Accept ⌘⏎` answered
    ``invalid args `intent` for command `d15_send_intent`: missing field
    `plan_id` ``: `models/plan.ts` sends `planId` / `contentSha256`, and
    `D15Intent` (`apps/gui/src-tauri/src/plan.rs`) had `rename_all =
    "snake_case"` without `rename_all_fields`, so Tauri refused every Accept,
    Reject and Hand off before the adapter saw it. The D15 tests build the
    intent in Rust and never crossed the wire. `D16Intent`
    (`apps/gui/src-tauri/src/board.rs`) had the same shape against
    `models/board.ts`'s `sessionId`. **Fixed 2026-10-10 in this batch**
    (`4c92b511 fix(gui): read the Plan Studio and Agent Board intents in the
    webview's field names`). RED: `the webview's accept must deserialize:
    Error("missing field `plan_id`", line: 0, column: 0)` and `the webview's
    pause must deserialize: Error("missing field `session_id`", line: 0,
    column: 0)`. Tests: `apps/gui/tests/d15_plan_studio.rs`
    `the_intent_the_webview_sends_is_the_intent_the_host_reads`,
    `apps/gui/tests/d16_agent_board.rs`
    `the_board_intent_the_webview_sends_is_the_intent_the_host_reads`.
    Confirmed natively for D15 (step 19); D16 is unreachable on this host.
42. **A proposed plan does not survive a Core restart** (Core; numbered and
    stopped). After a relaunch against the same `VIDEN_HOME`, Plan Studio for
    Lane 1 reads `No plan proposed` / `Core holds no plan for this owner.`,
    and the client's D15 projection only filters what Core's
    `RuntimeViewState::plan_documents` carries — while
    `workflows/projects/…/agents.jsonl` still holds the
    `plan_document_updated` event for `plan_1791621401009792000`. The
    contract design calls the plan "a Core-owned durable fact"; on this
    evidence the record is durable on disk and absent from the view a new
    process hydrates. No `crates/**` file was edited.
43. **The `Lanes` rail can render the pre-hydration `CONNECTING` shell over a
    live Core connection** (GUI). The rail's `Lanes` item drew a `LANES`
    sidebar reading `— 0 No Lanes yet` beside `◌ CONNECTING` / `Establishing
    the versioned Core connection.` / `Core connection pending`, with
    `Conversation`, `Plan Studio` (`unavailable: Core publishes no
    runtime.plan_documents`), `Decisions` and `Settings` disabled, while the
    connection log stayed `to=live` and the status bar kept the bound Lane.
    Reproduced on demand as the first `Lanes` press after a relaunch (step 22,
    frame `38`); later presses in the same process did not reproduce it; `Esc`
    leaves it. In process 32732 it appeared with no input of this run's own
    and cost that process its pending `git_push` approval, because the run
    did not know about `Esc` then.
44. **Plan Studio's decision row is occluded by the composer at the default
    window size** (GUI). At 1280×800 `Accept ⌘⏎`, `Reject with reason`,
    `Hand off`, the refusal note and `Handed off to dag_…` sit at the
    composer's height and do not appear in any frame (`24`, `25`, `27`–`29`);
    they were reachable by Accessibility and by `⌘⏎` only. An operator with a
    pointer cannot see what they are deciding with.
45. **Plan Studio keeps the `plan_mode` refusal after the work mode returns to
    Build** (GUI). `MODE build`, `PERM ask` in the status bar and the pill,
    and `Hand off` still disabled with the plan-mode sentence until Plan
    Studio was closed and reopened.
46. **A newly created, healthy Lane draws another Lane's `AGENT STOPPED`
    surface** (GUI or Core; owning side not decided). After Lane 1's tool
    failure, Lanes 3 and 4 — each with an exact owner and a completed TASK
    turn — drew `AGENT STOPPED` / `Core reports that execution has stopped.`
    with `Close Lane · no_exact_lane` in their own conversation slot. The
    same surface also stood over Lane 2's pending creation (step 8).

### Verdicts on defects 25–38

| # | Verdict on the candidate |
| --- | --- |
| 25 | **Closed, confirmed natively, end to end.** The `git_push` half reaches the dock, Core's typed `no_upstream` renders with `Push and set upstream`, that raises its own approval, and the push lands in the bare `origin` with four `source.push` rows |
| 26 | Not re-driven; five launch-time binds against a home holding Lanes each rendered the full cockpit |
| 27 | **Closed, re-confirmed.** The work-mode pill settled twice (Plan, then Build); no submit met a held `set_work_mode` slot |
| 28 | **Closed, re-confirmed in both directions** (step 5) |
| 29 | **Closed by 36's and C18's fixes together.** The busy-composer submit is queued by Core and runs (step 6) |
| 30 | **Closed, re-confirmed.** The commit bar names and acts on the selected Lane only (step 11) |
| 31 | **Closed, confirmed natively.** The revoked row leaves the list at once and stays gone; the identical input asks again (step 15) |
| 32 | **Closed, re-confirmed with real keystrokes** (step 25) |
| 33 | **Closed, re-confirmed** (step 20) |
| 34 | **Closed, re-confirmed** across four relaunches: `lane-agent-bindings.jsonl` took a superseding `lane-agent-rebound` row for every restored Lane each time, Lane 1 took turns in two relaunched processes, and its commit bar staged, committed and pushed in one of them |
| 35 | **Closed, re-confirmed.** `Stage all` raises `git_add` on the dock every time it was pressed (steps 12 and 14) |
| 36 | **Closed.** Core acknowledges the Lane's follow-up and drains it (step 6) |
| 37 | **No drop observed in five processes.** The connection log is live and wrote only the two start-up transitions each time; the one `CONNECTING` seen is defect 43, not a transport state. H7's retry path was therefore never exercised natively |
| 38 | **Fixed in this batch and confirmed natively.** Reproduced, root-caused (selection without a projection read on a quiet Core), fixed in `5afb01b0`, and two Lanes created beside a stopped Lane each have an exact owner |

### Verdict on compatibility follow-up 16

| # | Verdict on the candidate |
| --- | --- |
| 16 | **Met on the GUI, natively.** A follow-up submitted while the Lane's turn waits on its approval is acknowledged by Core (`1 queued · runs after the current turn`), and when the turn completes Core drains it into a new Lane-owned turn carrying the follow-up's text. A turn that ends in a denied tool drains it too. The cancel leg — cancel a turn with a queued follow-up and see nothing run — was **not reachable**, because a parked turn cannot be cancelled from the cockpit (defect 39) |

### Observations, not defects

- **The webview enters the Accessibility tree only while the app is
  frontmost.** Every read and every batch fronted the process first and
  asserted `front=Viden pid=<ours>` before sending anything; other
  applications (`UserNotificationCenter`, `Google Chrome`) took the front
  between batches several times, and the window was moved between this host's
  three displays and zoomed once without an input from this run.
- **A committed, unpushed branch with no upstream has no push control.** After
  the relaunch the commit bar offered `Stage all`, `Commit` and
  `Commit & Push ⇡` (both needing a message) and nothing that pushes the
  existing commit; `↑0 ↓0` is all the sync chip can say without an upstream.
  Whether the chip pushes when `ahead > 0` was not tested.
- **The approval revision editor is drawn on requests that cannot be
  revised.** `Revise the tool input… / Send the revision` renders under
  `lane_create`, `edit_file` and `git_add` requests whose `E · Edit` tier is
  disabled with `not_editable` (frames `03`, `05`, `16`). Not pressed, not
  claimed.
- **A Lane's conversation slot draws Core's own rows, not the turn's text.**
  `LANE OUTPUT`, the inline `Modified README.md` diff and the permission
  decisions render; the user and assistant messages the session JSONL holds
  for each Lane turn do not. Not investigated.
- **Input source.** `com.apple.inputmethod.SCIM.ITABC` (Pinyin) at
  `08:14:56Z` with another application frontmost; `com.apple.keylayout.ABC`
  with Viden frontmost at `08:46:43Z` and before and after the keystroke leg
  at `08:47:08Z`. This run never called the input-source setter, so there was
  nothing to restore; the difference is the host's per-application source.
- **The bundle is still `0.1.0-rc.5`.**

### What This Means For Plan Goal 4, Per Surface

`docs/release-0.3.3-plan.md` goal 4, carried into `0.3.4` as goal 6 — "one real
local-first development task completes through the GUI, from intake to a
committed change, with audit and evidence recorded":

- **Through the TUI — met, end to end**, unchanged from the 2026-09-12 record.
- **Through the native GUI window on the `0.3.5` candidate — met.** A Lane is
  created under Core's approval, its TASK runs as a turn, the agent's
  `edit_file` lands in the Lane worktree and nowhere else, a follow-up queued
  during the turn is run by Core afterwards, and the Lane's change is staged,
  committed and pushed to `origin` with every step decided on the dock and
  written to `audit.jsonl` — `source.stage`, `source.commit`, and the first
  natively driven `source.push` set including Core's `no_upstream` recovery.
  Plan Studio's Plan → Build hand-off reaches D13's DAG.
- **With these qualifications, stated rather than smoothed:** the bind was taken
  at launch, not through `⌘O`; the push completed on a second pair in a
  relaunched process, the first pair's push having been lost when the window
  fell into defect 43; and E3e's bundle includes two GUI fixes taken in this
  batch (38, 41). Open on the candidate: GUI defects 39, 40 and 43–46, and
  Core defect 42.
