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

use viden_gui::{
    D1CockpitProjection, D1Intent, DesktopState, OperatorGitIntent, OperatorGitProjection,
    PermissionChoice, PermissionIntent, open_local_workspace_with_session_home,
    pump_core_events_once, shell,
};

/// The pump's own bounded receive, the same one `spawn_core_event_pump` uses.
const PUMP_TIMEOUT: Duration = Duration::from_millis(250);

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
                Err(()) => break,
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
        "a command the operator pressed waited {worst:?} for the adapter lock the pump holds;          a gesture that cannot reach the adapter is a gesture that sends nothing"
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
        result.failure_class.as_deref(),
        Some("no_upstream"),
        "the fixture has no remote, so Core's typed refusal is NoUpstream"
    );
}

/* ---- defect 36: a follow-up during a turn ---- */

/// E3d defect 36, through the shipped shell, in the shape the native run hit.
///
/// The composer advertises `Queue follow-up` the moment Core reports a turn,
/// and the turn E3d was in was **waiting on an approval** — which is exactly
/// when Core's single supervisor worker is not answering commands. The submit
/// must then either put a `QueueFollowUp` on the wire that Core answers with
/// its own `InputQueued`, or be refused with the text kept. Accepted, cleared
/// and lost is the one outcome that is not allowed, and it is what E3d found
/// twice.
#[test]
fn a_follow_up_submitted_during_a_turn_is_queued_or_refused_through_the_shell() {
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

    let queued = shell::d1_queue_follow_up(harness.state(), "gui-d1-queue", &lane, FOLLOW_UP);

    let Ok(result) = queued else {
        // A refusal is an acceptable answer — it is what E3c rendered — so
        // long as it is a refusal and not a silent accept. The composer keeps
        // the text on this path.
        let refusal = queued.expect_err("checked above");
        assert!(
            !refusal.is_empty(),
            "a refused follow-up must say why, so the composer can render it"
        );
        return;
    };

    assert_ne!(
        result.outcome.state, "idle",
        "an accepted follow-up is never idle: it is pending Core's answer or settled"
    );

    // Core's own `InputQueued`, read back through the projection, is the only
    // thing that makes the queue real. A `confirmed` outcome with no queued
    // row is the E3d shape: the client believed it, the operator saw nothing.
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut latest = result;
    while Instant::now() < deadline {
        if latest.outcome.state == "rejected" {
            return;
        }
        if latest
            .projection
            .live_work
            .queued_inputs
            .iter()
            .any(|input| input.content_preview.contains(FOLLOW_UP))
        {
            return;
        }
        thread::sleep(Duration::from_millis(50));
        latest = shell::d1_poll(harness.state(), Some(&lane), false)
            .expect("re-read the cockpit through the shell");
    }
    panic!(
        "the follow-up was accepted with no refusal and never became a queued input the \
         operator could see: outcome={:?} queued={:?} activeTurns={} wakes={}",
        latest.outcome,
        latest.projection.live_work.queued_inputs,
        latest.projection.active_turns.len(),
        harness.wake_count()
    );
}

/* ---- defect 38: a Lane created beside a stopped Lane ---- */

/// E3d defect 38, through the shipped shell: a Lane created while another
/// Lane's agent is stopped must have an execution owner, and the selection
/// must be able to move off it.
#[test]
fn a_lane_created_beside_a_stopped_lane_keeps_its_owner() {
    let harness = ShellHarness::launch("second-lane");
    let first = harness.create_starter_lane("first-lane");
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
    let owner_of_first = shell::operator_git(harness.state(), Some(&first))
        .expect("read the first Lane's owner through the shell");
    assert!(
        owner_of_first.owner_available,
        "the Lane the selection moved back to still has an execution owner: {:?}",
        owner_of_first.owner_unavailable_reason
    );
}
