//! The shipped shell path, driven end to end against live in-process Core.
//!
//! H6's tests passed while the shipped desktop client dropped a Lane-scoped
//! stage and a follow-up on the floor (E3d defects 35 and 36), because they
//! exercised the cockpit against a mocked port and the adapter directly. The
//! shipped path is webview -> `main.ts` port -> Tauri `invoke` -> the `lib.rs`
//! command -> the adapter, and it is woken only by `spawn_core_event_pump`.
//!
//! Every test here drives that path:
//!
//! * the calls go through `viden_gui::shell`, which holds the one body each
//!   `#[tauri::command]` delegates to;
//! * they share the same `DesktopState` mutex the commands lock;
//! * a real pump thread runs beside them, holding that mutex for its bounded
//!   drain exactly as the desktop pump does, so a command contends with it and
//!   an event the pump consumes first is an event the command never sees.
//!
//! Core is the real `LocalCoreHost` over a temporary session home and a
//! temporary Git fixture. Nothing here reads or writes the repository's own
//! `.viden/`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use viden_core::{CoreClientError, RuntimeSnapshot, RuntimeViewState};
use viden_gui::{
    D1CockpitProjection, D1Intent, D6ConnectionState, D6Intent, DesktopState, GuiCoreAdapter,
    OperatorGitIntent, OperatorGitProjection, PermissionChoice, PermissionIntent,
    open_local_workspace_with_session_home, pump_core_events_once, shell,
};

mod support;
use support::TestCoreClient;

/// The frozen contract fixture, used only as a valid snapshot for the severed
/// transport: defect 37 is about the connection, not about what Core published.
const D1_FIXTURE: &str = include_str!(
    "../../../crates/types/tests/fixtures/frontend-contract-v1/d1-vertical-slice.json"
);

/// The pump's own bounded receive, the same one `spawn_core_event_pump` uses.
const PUMP_TIMEOUT: Duration = viden_gui::shell::PUMP_DRAIN;

/// E3d's own follow-up text, kept verbatim so the assertion names what the
/// native run lost.
const FOLLOW_UP: &str = "Summarise what you just changed";

/* ---- the harness ---- */

/// One desktop shell: a temporary Core, the state the commands lock, and the
/// background pump that is the only thing waking the webview.
struct ShellHarness {
    state: Arc<DesktopState>,
    stop: Arc<AtomicBool>,
    pump: Option<JoinHandle<()>>,
    /// Every wake the pump emitted, which is what the webview re-reads on.
    wakes: Arc<Mutex<usize>>,
    session_home: PathBuf,
    project: PathBuf,
}

/// How long the whole harness may wait before it reports rather than hangs.
///
/// A shell-path fault shows up here as a thread that never gets the adapter
/// lock, and a test binary that hangs says less than one that names the wait.
const WATCHDOG: Duration = Duration::from_secs(120);

impl ShellHarness {
    /// Opens a fixture workspace and starts the pump, as a desktop launch does.
    fn launch(label: &str) -> Self {
        spawn_watchdog(label);
        let session_home = temp_dir(&format!("{label}-session-home"));
        let project = temp_dir(&format!("{label}-project"));
        initialize_git_repository(&project);
        let adapter = open_local_workspace_with_session_home(&project, &session_home)
            .expect("open the fixture workspace against a temporary Core");
        let state = Arc::new(DesktopState::new(Some(adapter)));
        let stop = Arc::new(AtomicBool::new(false));
        let wakes = Arc::new(Mutex::new(0_usize));
        let pump = spawn_pump(Arc::clone(&state), Arc::clone(&stop), Arc::clone(&wakes));
        Self {
            state,
            stop,
            pump: Some(pump),
            wakes,
            session_home,
            project,
        }
    }

    fn state(&self) -> &DesktopState {
        &self.state
    }

    fn wake_count(&self) -> usize {
        *self.wakes.lock().expect("read the wake counter")
    }

    /// The cockpit read the webview does on every wake.
    fn cockpit(&self, lane_id: Option<&str>) -> D1CockpitProjection {
        shell::d1_cockpit(self.state(), lane_id).expect("Core publishes a D1 projection")
    }

    /// Creates one starter Lane the way the New Lane popover does, answering
    /// the `lane_create` approval the dock raises.
    fn create_starter_lane(&self, label: &str) -> String {
        let preview = shell::d1_send_intent(
            self.state(),
            &format!("{label}-preview"),
            D1Intent::PreviewDefaultLane {
                preset: "coder".to_string(),
            },
        )
        .expect("preview a default Lane")
        .projection
        .starter_lane_previews
        .into_iter()
        .next()
        .expect("Core publishes a starter Lane preview");

        shell::d1_send_intent(
            self.state(),
            &format!("{label}-create"),
            D1Intent::CreateStarterLane {
                lane_id: preview.lane_id.clone(),
                preset: "coder".to_string(),
                branch: preview.branch.clone(),
                preview_id: preview.preview_id.clone(),
                content_sha256: preview.content_sha256.clone(),
            },
        )
        .expect("request starter Lane creation");
        self.decide_dock_approval(&format!("{label}-create-allow"), Some(&preview.lane_id));
        self.settle(Some(&preview.lane_id));
        preview.lane_id
    }

    /// Answers whatever approval the permission dock is holding, the way the
    /// dock's own control does: read the projection, send `Respond`.
    ///
    /// Polls rather than reading once, because the pump may not have drained
    /// the `ApprovalRequested` yet — which is precisely the ordering the
    /// shipped shell lives with.
    fn decide_dock_approval(&self, command_id: &str, lane_id: Option<&str>) -> String {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if let Some(request) = self.cockpit(lane_id).permission_dock.request {
                let tool_name = request.tool_name.clone();
                shell::permission_send_intent(
                    self.state(),
                    command_id,
                    PermissionIntent::Respond {
                        request_id: request.id,
                        choice: PermissionChoice::Once,
                        feedback: None,
                        revised_input: None,
                    },
                )
                .expect("answer the approval the dock is holding");
                return tool_name;
            }
            thread::sleep(Duration::from_millis(25));
        }
        panic!("no approval reached the permission dock within 20s");
    }

    /// Waits for Core to raise one named approval, without answering it.
    ///
    /// This is the state E3d submitted its follow-up in: a turn stopped on an
    /// approval, with Core's one worker not answering commands.
    fn await_dock_request(&self, tool_name: &str) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if self
                .cockpit(None)
                .permission_dock
                .request
                .is_some_and(|request| request.tool_name == tool_name)
            {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        panic!("Core never raised a `{tool_name}` approval within 30s");
    }

    /// Lets the pump advance and re-reads, the way the webview does on a wake.
    fn settle(&self, lane_id: Option<&str>) {
        for _ in 0..8 {
            thread::sleep(Duration::from_millis(30));
            let _ = shell::d1_poll(self.state(), lane_id, false);
        }
    }

    /// Core's own record of where a Lane's tree lives.
    fn lane_worktree(&self, lane_id: &str) -> PathBuf {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if let Some(source) = self
                .cockpit(Some(lane_id))
                .lanes
                .iter()
                .find(|lane| lane.id == lane_id)
                .and_then(|lane| lane.source.as_ref())
                .and_then(|source| source.worktree.clone())
            {
                return PathBuf::from(source);
            }
            thread::sleep(Duration::from_millis(25));
        }
        panic!("Core never published a worktree for {lane_id}");
    }

    /// Drains through the shell until the operator action settles, or reports
    /// the state it is stuck in.
    fn settle_operator_git(&self, lane_id: Option<&str>) -> OperatorGitProjection {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let state = shell::operator_git_poll(self.state(), lane_id)
                .expect("poll the operator action through the shell");
            if state.outcome.state != "pending" {
                return state;
            }
            if Instant::now() >= deadline {
                return state;
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}

impl Drop for ShellHarness {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(pump) = self.pump.take() {
            let _ = pump.join();
        }
        let _ = std::fs::remove_dir_all(&self.session_home);
        let _ = std::fs::remove_dir_all(&self.project);
    }
}

/// The desktop pump, as `spawn_core_event_pump` runs it.
fn spawn_pump(
    state: Arc<DesktopState>,
    stop: Arc<AtomicBool>,
    wakes: Arc<Mutex<usize>>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            match pump_core_events_once(&state, PUMP_TIMEOUT) {
                Ok(Some(true)) => {
                    *wakes.lock().expect("count a wake") += 1;
                }
                Ok(Some(false)) => {}
                Ok(None) => thread::sleep(PUMP_TIMEOUT),
                Err(_) => break,
            }
        }
    })
}

/// Aborts the test binary with a named diagnosis rather than hanging forever.
fn spawn_watchdog(label: &str) {
    let label = label.to_string();
    let deadline = Instant::now() + WATCHDOG;
    thread::spawn(move || {
        while Instant::now() < deadline {
            thread::sleep(Duration::from_millis(500));
        }
        eprintln!(
            "h7 shell-path harness `{label}` made no progress in {WATCHDOG:?}. The usual cause is \
             the desktop pump holding the adapter lock across its drain, which is what starves the \
             command the operator pressed."
        );
        std::process::abort();
    });
}

fn temp_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after Unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("viden-gui-h7-{label}-{unique}"));
    std::fs::create_dir_all(&path).expect("create the temporary harness directory");
    path
}

fn initialize_git_repository(project: &Path) {
    std::fs::write(project.join("README.md"), "# H7 shell-path fixture\n")
        .expect("write the fixture readme");
    for args in [
        vec!["init"],
        vec!["add", "README.md"],
        vec![
            "-c",
            "user.name=Viden GUI Test",
            "-c",
            "user.email=viden-gui-test@localhost",
            "commit",
            "-m",
            "Initialize fixture",
        ],
    ] {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(project)
            .status()
            .expect("run the git fixture command");
        assert!(status.success(), "git fixture command must succeed");
    }
}

/* ---- defect 35: the lock the operator's own command has to take ---- */

/// E3d defect 35's shell-path cause, measured rather than argued.
///
/// Every Tauri command runs on a thread that is not the pump's, and both take
/// the one `DesktopState` adapter mutex. The pump used to hold that mutex
/// across its whole bounded receive and re-acquire it the instant it let go;
/// `std::sync::Mutex` promises no fairness, so the operator's own gesture
/// could queue behind a background drain indefinitely. `Stage all` pressing
/// cleanly and nothing happening is what that looks like from the outside.
///
/// The bound is deliberately generous: this is not a latency budget, it is the
/// difference between "the client is slow" and "the client never sends".
#[test]
fn the_desktop_pump_leaves_the_adapter_lock_free_for_the_operators_own_command() {
    let harness = ShellHarness::launch("lock-fairness");

    // The cheapest command in the shell: a no-traffic projection read that
    // does nothing but take the lock. Anything it waits for is the pump.
    let mut worst = Duration::ZERO;
    for _ in 0..40 {
        let started = Instant::now();
        shell::operator_git(harness.state(), None)
            .expect("the no-traffic operator-git read answers through the shell");
        worst = worst.max(started.elapsed());
        thread::sleep(Duration::from_millis(5));
    }

    assert!(
        worst < Duration::from_millis(500),
        "a command the operator pressed waited {worst:?} for the adapter lock the pump holds; a \
         gesture that cannot reach the adapter is a gesture that sends nothing"
    );
}

/* ---- defect 35: a Lane-scoped stage must reach a decidable surface ---- */

/// E3d defect 35, through the shipped shell rather than the adapter.
///
/// With two Lanes and one selected, `Stage all` must put a real
/// `RunOperatorGitAction` on the wire and Core's `git_add` approval must reach
/// the dock the operator is looking at. E3b steps 19-21 had exactly this
/// natively before H6; E3d found the client standing on
/// "Stage is waiting for approval in the permission dock." beside an empty
/// dock, with nothing staged and no `git_*` audit row.
#[test]
fn a_lane_scoped_stage_reaches_the_permission_dock_through_the_shell() {
    let harness = ShellHarness::launch("stage-dock");
    let lane_a = harness.create_starter_lane("lane-a");
    let lane_b = harness.create_starter_lane("lane-b");
    assert_ne!(lane_a, lane_b, "the fixture needs two distinct Lanes");

    let worktree = harness.lane_worktree(&lane_a);
    std::fs::write(worktree.join("README.md"), "# H7 edited in the Lane\n")
        .expect("dirty the Lane worktree");

    let sent = shell::run_operator_git_action(
        harness.state(),
        "gui-git-stage",
        Some(&lane_a),
        OperatorGitIntent::Stage { paths: Vec::new() },
    )
    .expect("the shell sends the Lane-scoped stage");

    assert!(
        sent.capability_available && sent.owner_available,
        "the selected Lane has a Core owner to act as: {:?}",
        sent.owner_unavailable_reason
    );
    assert_eq!(
        sent.pending_action,
        Some("stage"),
        "the stage is on the wire as Core's own audit verb, not merely rendered"
    );

    let tool = harness.decide_dock_approval("gui-git-stage-allow", Some(&lane_a));
    assert_eq!(
        tool, "git_add",
        "the approval the dock holds is the stage's own git tool"
    );

    let settled = harness.settle_operator_git(Some(&lane_a));
    assert_eq!(
        settled.outcome.state, "confirmed",
        "the stage settles once its approval is answered: {:?}",
        settled.outcome
    );
    let result = settled.result.as_ref().expect("Core answered the stage");
    assert_eq!(result.kind, "completed");
    assert_eq!(result.action, "stage");

    // And the pump did its own job throughout: the wake is the only thing that
    // tells the webview to re-read, so a fairness rule that quieted the pump
    // would trade one silence for another.
    assert!(
        harness.wake_count() > 0,
        "the pump drained and woke while the operator's commands went through it"
    );
}

/// E3b steps 19-21 in the harness: stage, commit, `Commit & Push`, and the
/// `git_push` half arriving at the dock and settling with `no_upstream`.
#[test]
fn the_commit_and_push_pair_runs_through_the_shell_with_a_lane_selected() {
    let harness = ShellHarness::launch("commit-pair");
    let lane = harness.create_starter_lane("pair-lane");
    let worktree = harness.lane_worktree(&lane);
    std::fs::write(worktree.join("README.md"), "# H7 pair\n").expect("dirty the Lane worktree");

    shell::run_operator_git_action(
        harness.state(),
        "gui-git-stage",
        Some(&lane),
        OperatorGitIntent::Stage { paths: Vec::new() },
    )
    .expect("the shell sends the stage");
    harness.decide_dock_approval("gui-git-stage-allow", Some(&lane));
    let staged = harness.settle_operator_git(Some(&lane));
    assert_eq!(staged.outcome.state, "confirmed", "{:?}", staged.outcome);

    shell::run_operator_git_action(
        harness.state(),
        "gui-git-commit",
        Some(&lane),
        OperatorGitIntent::Commit {
            message: "Edit the Lane README".to_string(),
        },
    )
    .expect("the shell sends the commit");
    harness.decide_dock_approval("gui-git-commit-allow", Some(&lane));
    let committed = harness.settle_operator_git(Some(&lane));
    assert_eq!(
        committed.outcome.state, "confirmed",
        "{:?}",
        committed.outcome
    );
    assert_eq!(
        committed.result.as_ref().map(|result| result.kind),
        Some("completed"),
        "the commit ran"
    );

    // The second half, exactly as `advanceCommitPair` sends it.
    shell::run_operator_git_action(
        harness.state(),
        "gui-git-push",
        Some(&lane),
        OperatorGitIntent::Push {
            remote: None,
            set_upstream: false,
        },
    )
    .expect("the shell sends the push");
    let tool = harness.decide_dock_approval("gui-git-push-allow", Some(&lane));
    assert_eq!(tool, "git_push", "the push raises its own approval");
    let pushed = harness.settle_operator_git(Some(&lane));
    assert_eq!(pushed.outcome.state, "confirmed", "{:?}", pushed.outcome);
    let result = pushed.result.as_ref().expect("Core answered the push");
    assert_eq!(result.action, "push");
    assert_eq!(
        result.failure_class,
        Some("no_upstream"),
        "the fixture has no remote, so Core's typed refusal is NoUpstream"
    );
}

/* ---- defect 36: a follow-up during a turn ---- */

/// E3d defect 36, through the shipped shell, in the shape the native run hit.
///
/// The composer advertises `Queue follow-up` the moment Core reports a turn,
/// and the turn E3d was in was **waiting on an approval** — which is exactly
/// when Core's one supervisor worker is answering nothing. What this asserts
/// is the only thing the client can be held to: whatever Core does or does not
/// publish, the client must never report the follow-up as queued when Core has
/// not said so, because the composer retires the operator's text on that word.
///
/// The Core half is named rather than worked around. Core's accept path for
/// `RuntimeCommand::QueueFollowUp` is guarded `owner.lane_id.is_none()`
/// (`crates/runtime/src/runtime_supervisor.rs:1612`), so it acknowledges a
/// *session*-scoped follow-up and lets a Lane-scoped one fall through into the
/// busy worker's channel with no `CommandAccepted` and no `InputQueued`. The
/// GUI sends the Lane's exact submit owner because a follow-up is work for
/// that Lane. Compatibility follow-up 16 is therefore still unmet for a Lane,
/// and it is Core's to meet; if Core starts acknowledging it, the queued-input
/// branch below is the one that passes.
#[test]
fn a_follow_up_submitted_during_a_turn_is_never_reported_as_queued_unless_core_said_so() {
    let harness = ShellHarness::launch("queue-follow-up");
    let lane = harness.create_starter_lane("queue-lane");

    // A turn that stops on Core's own approval, which is where E3d was: the
    // `SubmitUserInput` that started it is unanswered and holds the D1 slot.
    shell::d1_send_intent(
        harness.state(),
        "gui-d1-submit",
        D1Intent::Submit {
            lane_id: lane.clone(),
            content: "tool edit_file path=README.md old=H7 new=Edited".to_string(),
        },
    )
    .expect("the shell sends the submit that starts the turn");
    harness.await_dock_request("edit_file");

    let Ok(sent) = shell::d1_queue_follow_up(harness.state(), "gui-d1-queue", &lane, FOLLOW_UP)
    else {
        // A refusal is an acceptable answer — it is what E3c rendered — and
        // the composer keeps the text on that path.
        return;
    };
    assert_ne!(
        sent.outcome.state, "idle",
        "an accepted follow-up is never idle: it is pending Core's answer or settled"
    );

    // The queue's own pending, polled the way the composer polls it. Whatever
    // it settles to, `confirmed` may never arrive without the queued input
    // Core's `InputQueued` publishes — that pair is the lie H6 would have told.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut latest = sent;
    while Instant::now() < deadline && latest.outcome.state == "pending" {
        thread::sleep(Duration::from_millis(50));
        latest = shell::d1_queue_follow_up_poll(harness.state(), &lane)
            .expect("poll the queue's own pending through the shell");
    }

    let queued_here = latest
        .projection
        .live_work
        .queued_inputs
        .iter()
        .any(|input| input.content_preview.contains(FOLLOW_UP));
    match latest.outcome.state {
        "confirmed" => assert!(
            queued_here,
            "a confirmed queue must be one the operator can see: Core published no queued input \
             for `{FOLLOW_UP}`"
        ),
        "pending" => assert!(
            latest.pending_command.is_some(),
            "an unacknowledged follow-up stays a named pending the composer can render, so the \
             text is kept rather than retired"
        ),
        "rejected" => {}
        other => panic!("unexpected queue outcome `{other}`"),
    }
}

/* ---- defect 37: a dropped connection is retried, and says so ---- */

/// The recovery surface exactly as the webview sees it: the cockpit read the
/// shell already ships, whose `recovery` block the adapter fills from the
/// connection state and its own reconnect schedule.
fn cockpit_recovery(state: &DesktopState) -> viden_gui::D6RecoveryProjection {
    shell::d1_cockpit(state, None)
        .expect("the cockpit reads back even while the transport is severed")
        .recovery
}

/// E3d defect 37, in-process: the transport is severed mid-session and the
/// host must both *say* so and *try again*.
///
/// The native run fell back to `◌ CONNECTING` with `DIAG 1x`, an inert
/// composer and a `LANES` panel reading `No Lanes yet`, stayed there across six
/// checks over 60 s, and only a kill and relaunch recovered it — while the
/// process wrote **zero bytes** with `RUST_LOG=info` set. Two things were
/// missing and both are here: a transition log that does not depend on a level
/// filter this workspace has no subscriber for, and a bounded retry whose
/// state the recovery surface can render instead of a spinner.
#[test]
fn a_severed_core_connection_is_logged_and_retried_with_a_visible_countdown() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let fixture: serde_json::Value = serde_json::from_str(D1_FIXTURE).expect("parse D1 fixture");
    let snapshot = serde_json::from_value::<RuntimeSnapshot>(fixture["initial_snapshot"].clone())
        .expect("read the fixture snapshot");
    let client = TestCoreClient::new(RuntimeViewState::new(snapshot), sent);
    let severed = client.severed_handle();
    let mut adapter = GuiCoreAdapter::new(Box::new(client));
    adapter.connect().expect("the fixture transport connects");
    assert_eq!(adapter.d6_recovery().connection, D6ConnectionState::Live);

    let state = DesktopState::new(Some(adapter));

    // Cut the wire the way a mid-session drop cuts it: nothing is torn down,
    // the next read simply fails.
    *severed.lock().expect("sever the transport") =
        Some(CoreClientError::Transport("core pipe closed".into()));

    // The pump notices on the round whose drain fails, and retries on the next
    // one. Two rounds, then — and the retry is the pump's own, with no
    // operator behind it, which is the whole point.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        pump_core_events_once(&state, PUMP_TIMEOUT).expect("the pump round runs");
        let attempted = cockpit_recovery(&state).reconnect.is_some();
        if attempted {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the pump never attempted a reconnect against a severed transport"
        );
    }

    let recovery = cockpit_recovery(&state);
    assert_ne!(
        recovery.connection,
        D6ConnectionState::Live,
        "a severed transport is not a live connection"
    );
    let retry = recovery
        .reconnect
        .as_ref()
        .expect("the host says what it is doing about the drop");
    assert_eq!(retry.attempts, 1, "the pump attempted the reconnect itself");
    assert!(!retry.gave_up, "one failure is not giving up");
    assert!(
        retry.next_attempt_in_ms.is_some(),
        "an operator is told when the next attempt is, not left on a spinner"
    );
    assert_eq!(
        retry
            .last_error
            .as_deref()
            .map(|error| error.contains("core pipe closed")),
        Some(true),
        "the host's own words reach the surface: {:?}",
        retry.last_error
    );

    // And it recovers on its own once the transport comes back, without the
    // kill and relaunch E3d needed.
    *severed.lock().expect("restore the transport") = None;
    let recovered_by = Instant::now() + Duration::from_secs(30);
    loop {
        pump_core_events_once(&state, PUMP_TIMEOUT).expect("the pump round runs");
        let recovery = cockpit_recovery(&state);
        if recovery.connection == D6ConnectionState::Live {
            assert!(
                recovery.reconnect.is_none(),
                "a live connection has no countdown left to render"
            );
            return;
        }
        assert!(
            Instant::now() < recovered_by,
            "the host never reconnected to a transport that came back: {:?}",
            recovery.reconnect
        );
    }
}

/* ---- defect 38: a Lane created beside a stopped Lane ---- */

/// E3d defect 38, through the shipped shell: a Lane created while another
/// Lane's execution is stopped must have an execution owner, and the selection
/// must be able to move off it.
///
/// The native run found the new Lane carrying
/// `The selected Lane has no sole Core execution owner.`, its recovery control
/// reading `Close Lane · no_exact_lane`, the dock reading
/// `Waiting for the selected Lane context.`, and clicking the *other* Lane's
/// row moving nothing across two attempts. It was observed once, in the same
/// process that later lost its Core connection (defect 37), and every symptom
/// in it is a read that never answered.
#[test]
fn a_lane_created_beside_a_stopped_lane_keeps_its_owner() {
    let harness = ShellHarness::launch("second-lane");
    let first = harness.create_starter_lane("first-lane");

    // Stop the first Lane's execution, which is the state E3d created the
    // second Lane in. `close_lane` is the recovery surface's own control.
    shell::d6_send_intent(
        harness.state(),
        "gui-d6-close-first",
        D6Intent::CloseLane {
            lane_id: first.clone(),
        },
    )
    .expect("stop the first Lane through the shell");
    harness.settle(Some(&first));

    let second = harness.create_starter_lane("second-lane");
    let owner_of_second = shell::operator_git(harness.state(), Some(&second))
        .expect("read the second Lane's owner through the shell");
    assert!(
        owner_of_second.owner_available,
        "the Lane Core just created has an execution owner: {:?}",
        owner_of_second.owner_unavailable_reason
    );

    // The selection moves back to the first Lane, which is the gesture E3d
    // found inert.
    let back = harness.cockpit(Some(&first));
    assert_eq!(
        back.selected_lane_id.as_deref(),
        Some(first.as_str()),
        "the cockpit read for the first Lane resolves to the first Lane"
    );
    assert!(
        back.lanes.iter().any(|lane| lane.id == second),
        "and the second Lane is still in the list it was read beside"
    );
}
