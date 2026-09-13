//! Append-only typed lane lifecycle state.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use viden_types::{AgentLaneRecord, LaneRunStats, LaneStatus};

pub const LEGACY_LANES_MIGRATION_ID: &str = "legacy_lanes_tsv_v0";
pub const LEGACY_LANES_SCHEMA: &str = "viden.lanes.tsv.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaneEvent {
    pub event_id: String,
    pub lane_id: String,
    pub timestamp: u64,
    pub origin_session_id: Option<String>,
    pub kind: LaneEventKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LaneEventKind {
    Created {
        lane: AgentLaneRecord,
    },
    Replaced {
        lane: AgentLaneRecord,
    },
    StatusChanged {
        status: LaneStatus,
        summary: String,
    },
    Archived {
        summary: String,
    },
    LegacyImported {
        source: String,
        schema: String,
        lanes: Vec<AgentLaneRecord>,
    },
    /// One bounded run measurement. These are accounting facts, not lifecycle
    /// transitions: they never change lane status and are reduced leniently.
    RunObserved {
        observation: LaneRunObservation,
    },
}

/// A single directly observed run fact for a lane.
///
/// The three phases are exactly what Core can see for a cost-blind terminal or
/// tmux route: a process started, a process finished (with whatever exit code
/// the platform still had to offer), and bytes of a unified diff that actually
/// applied. Nothing here is derived from a provider or a price table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "phase")]
pub enum LaneRunObservation {
    /// A lane runtime was started; opens a run for wall-time accounting.
    Started,
    /// A lane runtime stopped. `exit_code` is `None` whenever the platform gave
    /// no status (signal kill, tmux `kill-session`, still-unknown result).
    Stopped { exit_code: Option<i32> },
    /// A unified diff of `diff_bytes` bytes applied successfully.
    Applied { diff_bytes: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaneMigrationAudit {
    pub migration_id: String,
    pub source: String,
    pub schema: String,
    pub imported_lane_ids: Vec<String>,
    pub event_id: String,
    pub timestamp: u64,
    pub origin_session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LaneState {
    lanes: BTreeMap<String, AgentLaneRecord>,
    migrations: BTreeMap<String, LaneMigrationAudit>,
    seen_event_ids: BTreeSet<String>,
    // Run accounting is reducer-owned. A `Created`, `Replaced`, or
    // `LegacyImported` payload never authors `run_stats`: the reducer projects
    // the accumulator onto the record it publishes, so no writer can assert a
    // measurement it did not actually observe.
    run_stats: BTreeMap<String, LaneRunStats>,
    // Start timestamp of the currently open run per lane, when one is open.
    open_runs: BTreeMap<String, u64>,
}

impl LaneState {
    pub fn lane(&self, lane_id: &str) -> Option<&AgentLaneRecord> {
        self.lanes.get(lane_id)
    }

    pub fn lanes(&self) -> &BTreeMap<String, AgentLaneRecord> {
        &self.lanes
    }

    pub fn migrations(&self) -> &BTreeMap<String, LaneMigrationAudit> {
        &self.migrations
    }

    pub fn migration(&self, migration_id: &str) -> Option<&LaneMigrationAudit> {
        self.migrations.get(migration_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegacyLaneImportOutcome {
    pub imported: bool,
    pub lane_count: usize,
}

impl LaneEvent {
    pub fn created(
        event_id: impl Into<String>,
        lane: AgentLaneRecord,
        timestamp: u64,
        origin_session_id: Option<String>,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            lane_id: lane.id.clone(),
            timestamp,
            origin_session_id,
            kind: LaneEventKind::Created { lane },
        }
    }

    pub fn status_changed(
        event_id: impl Into<String>,
        lane_id: impl Into<String>,
        status: LaneStatus,
        summary: impl Into<String>,
        timestamp: u64,
        origin_session_id: Option<String>,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            lane_id: lane_id.into(),
            timestamp,
            origin_session_id,
            kind: LaneEventKind::StatusChanged {
                status,
                summary: summary.into(),
            },
        }
    }

    pub fn run_observed(
        event_id: impl Into<String>,
        lane_id: impl Into<String>,
        observation: LaneRunObservation,
        timestamp: u64,
        origin_session_id: Option<String>,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            lane_id: lane_id.into(),
            timestamp,
            origin_session_id,
            kind: LaneEventKind::RunObserved { observation },
        }
    }

    pub fn legacy_imported(
        event_id: impl Into<String>,
        source: impl Into<String>,
        lanes: Vec<AgentLaneRecord>,
        timestamp: u64,
        origin_session_id: Option<String>,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            lane_id: LEGACY_LANES_MIGRATION_ID.to_string(),
            timestamp,
            origin_session_id,
            kind: LaneEventKind::LegacyImported {
                source: source.into(),
                schema: LEGACY_LANES_SCHEMA.to_string(),
                lanes,
            },
        }
    }
}

pub fn reduce_lane_events(events: &[LaneEvent]) -> Result<LaneState, String> {
    let mut state = LaneState::default();
    for event in events {
        if !state.seen_event_ids.insert(event.event_id.clone()) {
            return Err(format!("duplicate lane event id `{}`", event.event_id));
        }
        apply_event(&mut state, event)?;
    }
    Ok(state)
}

fn apply_event(state: &mut LaneState, event: &LaneEvent) -> Result<(), String> {
    match &event.kind {
        LaneEventKind::Created { lane } => {
            require_matching_lane_id(event, lane)?;
            if state.lanes.insert(lane.id.clone(), lane.clone()).is_some() {
                return Err(format!("lane `{}` already exists", lane.id));
            }
            project_run_stats(state, &lane.id);
        }
        LaneEventKind::Replaced { lane } => {
            require_matching_lane_id(event, lane)?;
            if !state.lanes.contains_key(&lane.id) {
                return Err(format!("lane `{}` does not exist", lane.id));
            }
            state.lanes.insert(lane.id.clone(), lane.clone());
            // A replacement payload describes lane configuration, never past
            // measurements, so accumulated run facts survive it.
            project_run_stats(state, &lane.id);
        }
        LaneEventKind::StatusChanged { status, summary } => {
            let lane = state
                .lanes
                .get_mut(&event.lane_id)
                .ok_or_else(|| format!("lane `{}` does not exist", event.lane_id))?;
            lane.status = *status;
            lane.summary = summary.clone();
        }
        LaneEventKind::Archived { summary } => {
            let lane = state
                .lanes
                .get_mut(&event.lane_id)
                .ok_or_else(|| format!("lane `{}` does not exist", event.lane_id))?;
            lane.status = LaneStatus::Archived;
            lane.summary = summary.clone();
        }
        LaneEventKind::LegacyImported {
            source,
            schema,
            lanes,
        } => {
            if event.lane_id != LEGACY_LANES_MIGRATION_ID {
                return Err("legacy lane import uses the wrong migration identity".to_string());
            }
            if schema != LEGACY_LANES_SCHEMA {
                return Err(format!("unsupported legacy lane schema `{schema}`"));
            }
            if state.migrations.contains_key(LEGACY_LANES_MIGRATION_ID) {
                return Err("legacy lane migration already exists".to_string());
            }
            let mut imported_lane_ids = Vec::with_capacity(lanes.len());
            for lane in lanes {
                if state.lanes.contains_key(&lane.id) {
                    return Err(format!("legacy lane `{}` already exists", lane.id));
                }
                if imported_lane_ids.contains(&lane.id) {
                    return Err(format!("legacy lane `{}` is duplicated", lane.id));
                }
                imported_lane_ids.push(lane.id.clone());
            }
            // Validate the complete batch before publishing any lane so a bad
            // migration event cannot leave a partially reduced state.
            for lane in lanes {
                state.lanes.insert(lane.id.clone(), lane.clone());
                project_run_stats(state, &lane.id);
            }
            state.migrations.insert(
                LEGACY_LANES_MIGRATION_ID.to_string(),
                LaneMigrationAudit {
                    migration_id: LEGACY_LANES_MIGRATION_ID.to_string(),
                    source: source.clone(),
                    schema: schema.clone(),
                    imported_lane_ids,
                    event_id: event.event_id.clone(),
                    timestamp: event.timestamp,
                    origin_session_id: event.origin_session_id.clone(),
                },
            );
        }
        LaneEventKind::RunObserved { observation } => {
            // An observation still requires a known lane, exactly like
            // `StatusChanged`: a measurement for a lane that was never created
            // is a corrupt log, not a tolerable gap.
            if !state.lanes.contains_key(&event.lane_id) {
                return Err(format!("lane `{}` does not exist", event.lane_id));
            }
            let stats = state.run_stats.entry(event.lane_id.clone()).or_default();
            match observation {
                LaneRunObservation::Started => {
                    stats.run_count = stats.run_count.saturating_add(1);
                    // A second `Started` without an intervening `Stopped` means
                    // the previous run's end was never observed; the newer start
                    // wins rather than fabricating a close for the older one.
                    state
                        .open_runs
                        .insert(event.lane_id.clone(), event.timestamp);
                }
                LaneRunObservation::Stopped { exit_code } => {
                    // Deliberately lenient where the lifecycle events are strict:
                    // these are stats events, not state-machine events. A crash
                    // between `Started` and `Stopped` leaves an orphan stop, and
                    // that must not brick the whole lanes log. So an orphan stop
                    // is not an error and accumulates no wall time; only the exit
                    // code, which was genuinely observed, is recorded.
                    if let Some(started_at) = state.open_runs.remove(&event.lane_id) {
                        // `LaneEvent::timestamp` is a Unix time in SECONDS (see
                        // `viden_types::now_timestamp`), so the elapsed value is
                        // scaled to the millisecond unit the field promises.
                        // Resolution is therefore one second, never finer; a
                        // sub-second run legitimately accumulates 0 ms.
                        stats.wall_time_ms = stats.wall_time_ms.saturating_add(
                            event
                                .timestamp
                                .saturating_sub(started_at)
                                .saturating_mul(1_000),
                        );
                    }
                    stats.last_exit_code = *exit_code;
                }
                LaneRunObservation::Applied { diff_bytes } => {
                    stats.diff_bytes = stats.diff_bytes.saturating_add(*diff_bytes);
                }
            }
            project_run_stats(state, &event.lane_id);
        }
    }
    Ok(())
}

/// Publish the reducer-owned accumulator onto the lane record clients read.
fn project_run_stats(state: &mut LaneState, lane_id: &str) {
    let stats = state.run_stats.get(lane_id).copied();
    if let Some(lane) = state.lanes.get_mut(lane_id) {
        // `None` means "never observed", which stays distinguishable from
        // `Some(LaneRunStats::default())` ("ran and measured zero").
        lane.run_stats = stats;
    }
}

fn require_matching_lane_id(event: &LaneEvent, lane: &AgentLaneRecord) -> Result<(), String> {
    if event.lane_id == lane.id {
        Ok(())
    } else {
        Err(format!(
            "lane event id `{}` does not match payload `{}`",
            event.lane_id, lane.id
        ))
    }
}

pub fn parse_legacy_lanes_tsv(raw: &str) -> Result<Vec<AgentLaneRecord>, String> {
    raw.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            let parts = line.split('\t').collect::<Vec<_>>();
            if !matches!(parts.len(), 5 | 7 | 8) {
                return Err(format!(
                    "legacy lane line {} must contain five, seven, or eight tab-separated columns",
                    index + 1
                ));
            }
            if parts[0].trim().is_empty() {
                return Err(format!("legacy lane line {} has an empty id", index + 1));
            }
            if let Some(raw_progress) = parts.get(5) {
                let progress = raw_progress
                    .parse::<u8>()
                    .map_err(|_| format!("legacy lane line {} has invalid progress", index + 1))?;
                if progress > 100 {
                    return Err(format!(
                        "legacy lane line {} has progress above 100",
                        index + 1
                    ));
                }
            }
            let stable_id = parts[0].trim_start_matches("L-").replace('-', "_");
            let summary = parts
                .get(6)
                .filter(|summary| !summary.trim().is_empty())
                .copied()
                .unwrap_or(parts[2]);
            serde_json::from_value(serde_json::json!({
                "id": parts[0],
                "task_id": format!("task_{stable_id}"),
                "agent": parts[1],
                "screen": parts[2],
                "transport": parts[4],
                "status": parts[3],
                "summary": summary,
                "evidence": [format!("evidence_{stable_id}")],
            }))
            .map_err(|error| format!("legacy lane line {}: {error}", index + 1))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::{Duration, Instant};

    use viden_types::{AgentLaneRecord, LaneStatus, fresh_id};

    use super::*;
    use crate::stores::WorkflowStore;

    const LEGACY_LANES: &str =
        include_str!("../../types/tests/fixtures/frontend-contract-v1/legacy-lanes.tsv");

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("viden_workflows_lane_{name}_{}", fresh_id("tmp")));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn legacy_lane_import_is_atomic_and_idempotent() {
        let home = temp_dir("migration_home");
        let cwd = temp_dir("migration_cwd");
        let legacy_path = cwd.join(".viden").join("lanes.tsv");
        fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
        fs::write(&legacy_path, LEGACY_LANES).unwrap();
        let store = WorkflowStore::new(&home, &cwd).unwrap();

        let first = store
            .import_legacy_lanes_tsv_once(&legacy_path, 10, Some("session_import".into()))
            .unwrap();
        let second = store
            .import_legacy_lanes_tsv_once(&legacy_path, 20, Some("session_repeat".into()))
            .unwrap();

        assert!(first.imported);
        assert_eq!(first.lane_count, 4);
        assert!(!second.imported);
        let events = store.load_lane_events().unwrap();
        assert_eq!(events.len(), 1, "the import is one append-only transaction");
        let state = store.load_lane_state().unwrap();
        assert_eq!(state.lanes().len(), 4);
        assert_eq!(state.migrations().len(), 1);
        assert_eq!(
            state.lane("L-conflict").unwrap().status,
            LaneStatus::Blocked
        );
        assert_eq!(
            state.lane("L-detached").unwrap().status,
            LaneStatus::Detached
        );
    }

    #[test]
    fn concurrent_legacy_lane_import_publishes_one_valid_event() {
        let home = temp_dir("concurrent_migration_home");
        let cwd = temp_dir("concurrent_migration_cwd");
        let legacy_path = cwd.join(".viden").join("lanes.tsv");
        fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
        fs::write(&legacy_path, LEGACY_LANES).unwrap();
        let store = WorkflowStore::new(&home, &cwd).unwrap();
        let barrier = Arc::new(Barrier::new(8));

        let handles = (0..8)
            .map(|index| {
                let store = store.clone();
                let barrier = Arc::clone(&barrier);
                let legacy_path = legacy_path.clone();
                thread::spawn(move || {
                    barrier.wait();
                    store.import_legacy_lanes_tsv_once(
                        &legacy_path,
                        10 + index,
                        Some(format!("session_{index}")),
                    )
                })
            })
            .collect::<Vec<_>>();
        let outcomes = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(
            outcomes.iter().filter(|outcome| outcome.imported).count(),
            1
        );
        assert_eq!(store.load_lane_events().unwrap().len(), 1);
        assert_eq!(store.load_lane_state().unwrap().lanes().len(), 4);
    }

    #[test]
    fn cross_process_legacy_import_publishes_one_valid_event() {
        let home = temp_dir("process_migration_home");
        let cwd = temp_dir("process_migration_cwd");
        let legacy_path = cwd.join(".viden").join("lanes.tsv");
        fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
        fs::write(&legacy_path, LEGACY_LANES).unwrap();
        let start_gate = cwd.join("import.start");
        let results = (0..4)
            .map(|index| cwd.join(format!("import-{index}.result")))
            .collect::<Vec<_>>();
        let mut children = results
            .iter()
            .enumerate()
            .map(|(index, result)| {
                spawn_lane_store_helper(LaneStoreHelperRequest {
                    case: "import",
                    home: &home,
                    cwd: &cwd,
                    legacy_path: Some(&legacy_path),
                    lane_id: None,
                    result_path: result,
                    start_gate: &start_gate,
                    index,
                })
            })
            .collect::<Vec<_>>();

        release_lane_helpers_when_ready(&mut children, &results, &start_gate);
        wait_for_lane_helpers(children);

        let outcomes = results
            .iter()
            .map(|path| fs::read_to_string(path).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes.iter().filter(|value| *value == "imported").count(),
            1
        );
        assert_eq!(
            outcomes.iter().filter(|value| *value == "existing").count(),
            3
        );
        let store = WorkflowStore::new(&home, &cwd).unwrap();
        assert_eq!(store.load_lane_events().unwrap().len(), 1);
        assert_eq!(store.load_lane_state().unwrap().lanes().len(), 4);
    }

    #[test]
    fn cross_process_checked_create_rejects_duplicate_lane_atomically() {
        let home = temp_dir("process_duplicate_home");
        let cwd = temp_dir("process_duplicate_cwd");
        let results = [
            cwd.join("duplicate-a.result"),
            cwd.join("duplicate-b.result"),
        ];
        let start_gate = cwd.join("duplicate.start");
        let mut children = results
            .iter()
            .enumerate()
            .map(|(index, result)| {
                spawn_lane_store_helper(LaneStoreHelperRequest {
                    case: "create",
                    home: &home,
                    cwd: &cwd,
                    legacy_path: None,
                    lane_id: Some("lane_duplicate"),
                    result_path: result,
                    start_gate: &start_gate,
                    index,
                })
            })
            .collect::<Vec<_>>();

        release_lane_helpers_when_ready(&mut children, &results, &start_gate);
        wait_for_lane_helpers(children);

        let outcomes = results
            .iter()
            .map(|path| fs::read_to_string(path).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes.iter().filter(|value| *value == "created").count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|value| value.starts_with("rejected:"))
                .count(),
            1
        );
        let store = WorkflowStore::new(&home, &cwd).unwrap();
        assert_eq!(store.load_lane_events().unwrap().len(), 1);
        assert_eq!(store.load_lane_state().unwrap().lanes().len(), 1);
    }

    #[test]
    fn cross_process_first_checked_appends_preserve_both_events() {
        let home = temp_dir("process_first_append_home");
        let cwd = temp_dir("process_first_append_cwd");
        let results = [cwd.join("first-a.result"), cwd.join("first-b.result")];
        let lane_ids = ["lane_first_a", "lane_first_b"];
        let start_gate = cwd.join("first-append.start");
        let mut children = results
            .iter()
            .zip(lane_ids)
            .enumerate()
            .map(|(index, (result, lane_id))| {
                spawn_lane_store_helper(LaneStoreHelperRequest {
                    case: "create",
                    home: &home,
                    cwd: &cwd,
                    legacy_path: None,
                    lane_id: Some(lane_id),
                    result_path: result,
                    start_gate: &start_gate,
                    index,
                })
            })
            .collect::<Vec<_>>();

        release_lane_helpers_when_ready(&mut children, &results, &start_gate);
        wait_for_lane_helpers(children);

        assert!(
            results
                .iter()
                .all(|path| fs::read_to_string(path).unwrap() == "created")
        );
        let store = WorkflowStore::new(&home, &cwd).unwrap();
        assert_eq!(store.load_lane_events().unwrap().len(), 2);
        assert_eq!(store.load_lane_state().unwrap().lanes().len(), 2);
    }

    #[test]
    fn lane_store_process_helper() {
        let Ok(case) = env::var("VIDEN_LANE_STORE_HELPER") else {
            return;
        };
        let home = PathBuf::from(env::var_os("VIDEN_LANE_HOME").unwrap());
        let cwd = PathBuf::from(env::var_os("VIDEN_LANE_CWD").unwrap());
        let result_path = PathBuf::from(env::var_os("VIDEN_LANE_RESULT").unwrap());
        let ready_path = PathBuf::from(env::var_os("VIDEN_LANE_READY").unwrap());
        let start_gate = PathBuf::from(env::var_os("VIDEN_LANE_START_GATE").unwrap());
        let index = env::var("VIDEN_LANE_INDEX")
            .unwrap()
            .parse::<u64>()
            .unwrap();

        // Publish readiness before opening the store so every contender reaches the
        // same pre-lock boundary and the parent can release them together.
        fs::write(&ready_path, b"ready").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !start_gate.exists() {
            assert!(
                Instant::now() < deadline,
                "lane store helper timed out waiting for start gate"
            );
            thread::sleep(Duration::from_millis(5));
        }

        let store = WorkflowStore::new(home, &cwd).unwrap();
        let result = match case.as_str() {
            "import" => {
                let legacy_path = PathBuf::from(env::var_os("VIDEN_LANE_LEGACY").unwrap());
                match store.import_legacy_lanes_tsv_once(
                    legacy_path,
                    100 + index,
                    Some(format!("session_process_{index}")),
                ) {
                    Ok(outcome) if outcome.imported => "imported".to_string(),
                    Ok(_) => "existing".to_string(),
                    Err(error) => format!("error:{error}"),
                }
            }
            "create" => {
                let lane_id = env::var("VIDEN_LANE_ID").unwrap();
                let mut lane = parse_legacy_lanes_tsv(LEGACY_LANES).unwrap()[0].clone();
                lane.id = lane_id.clone();
                lane.task_id = Some(format!("task_{lane_id}"));
                let event = LaneEvent::created(
                    format!("event_{lane_id}_{index}"),
                    lane,
                    100 + index,
                    Some(format!("session_process_{index}")),
                );
                match store.append_lane_event_checked(&event) {
                    Ok(()) => "created".to_string(),
                    Err(error) => format!("rejected:{error}"),
                }
            }
            other => panic!("unknown lane store helper case {other}"),
        };
        fs::write(result_path, result).unwrap();
    }

    struct LaneStoreHelperRequest<'a> {
        case: &'a str,
        home: &'a Path,
        cwd: &'a Path,
        legacy_path: Option<&'a Path>,
        lane_id: Option<&'a str>,
        result_path: &'a Path,
        start_gate: &'a Path,
        index: usize,
    }

    fn spawn_lane_store_helper(request: LaneStoreHelperRequest<'_>) -> Child {
        let ready_path = request.result_path.with_extension("ready");
        let mut command = Command::new(env::current_exe().unwrap());
        command
            .arg("--exact")
            .arg("lanes::tests::lane_store_process_helper")
            .arg("--nocapture")
            .env("VIDEN_LANE_STORE_HELPER", request.case)
            .env("VIDEN_LANE_HOME", request.home)
            .env("VIDEN_LANE_CWD", request.cwd)
            .env("VIDEN_LANE_RESULT", request.result_path)
            .env("VIDEN_LANE_READY", ready_path)
            .env("VIDEN_LANE_START_GATE", request.start_gate)
            .env("VIDEN_LANE_INDEX", request.index.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(path) = request.legacy_path {
            command.env("VIDEN_LANE_LEGACY", path);
        }
        if let Some(lane_id) = request.lane_id {
            command.env("VIDEN_LANE_ID", lane_id);
        }
        command.spawn().unwrap()
    }

    fn release_lane_helpers_when_ready(
        children: &mut [Child],
        result_paths: &[PathBuf],
        start_gate: &PathBuf,
    ) {
        let ready_paths = result_paths
            .iter()
            .map(|path| path.with_extension("ready"))
            .collect::<Vec<_>>();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ready_paths.iter().all(|path| path.exists()) {
            for child in children.iter_mut() {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!("lane store helper exited before ready: {status}");
                }
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for lane store helpers to become ready"
            );
            thread::sleep(Duration::from_millis(5));
        }

        assert!(
            result_paths.iter().all(|path| !path.exists()),
            "lane store helper ran before the shared start gate opened"
        );
        fs::write(start_gate, b"start").unwrap();
    }

    fn wait_for_lane_helpers(children: Vec<Child>) {
        for mut child in children {
            assert!(child.wait().unwrap().success());
        }
    }

    #[test]
    fn legacy_lane_parser_matches_the_frozen_typed_fixture() {
        let parsed = parse_legacy_lanes_tsv(LEGACY_LANES).unwrap();
        let frozen: Vec<AgentLaneRecord> = serde_json::from_str(include_str!(
            "../../types/tests/fixtures/frontend-contract-v1/typed-lanes.json"
        ))
        .unwrap();

        assert_eq!(parsed, frozen);
    }

    #[test]
    fn lane_lifecycle_replay_matches_live_reduction() {
        let imported = parse_legacy_lanes_tsv(LEGACY_LANES).unwrap();
        let mut lane = imported[0].clone();
        lane.status = LaneStatus::Running;
        lane.summary = "lane running".into();
        let events = vec![
            LaneEvent::created("evt_create", lane.clone(), 10, Some("session_1".into())),
            LaneEvent::status_changed(
                "evt_block",
                lane.id.clone(),
                LaneStatus::Blocked,
                "dependency failed",
                20,
                Some("session_1".into()),
            ),
        ];

        let live = reduce_lane_events(&events).unwrap();
        assert_eq!(live.lane(&lane.id).unwrap().status, LaneStatus::Blocked);
        assert_eq!(live.lane(&lane.id).unwrap().summary, "dependency failed");

        let home = temp_dir("replay_home");
        let cwd = temp_dir("replay_cwd");
        let store = WorkflowStore::new(&home, &cwd).unwrap();
        for event in &events {
            store.append_lane_event_checked(event).unwrap();
        }
        assert_eq!(store.load_lane_state().unwrap(), live);
    }

    #[test]
    fn malformed_legacy_import_writes_no_partial_event() {
        let home = temp_dir("invalid_home");
        let cwd = temp_dir("invalid_cwd");
        let legacy_path = cwd.join("lanes.tsv");
        fs::write(&legacy_path, "L-bad\tcodex\tmissing-columns\n").unwrap();
        let store = WorkflowStore::new(&home, &cwd).unwrap();

        let error = store
            .import_legacy_lanes_tsv_once(&legacy_path, 10, None)
            .unwrap_err();

        assert!(error.contains("five, seven, or eight tab-separated columns"));
        assert!(store.load_lane_events().unwrap().is_empty());
        assert!(!store.paths().lanes_log.exists());
    }

    #[test]
    fn lifecycle_rejects_unknown_lane_without_appending() {
        let home = temp_dir("unknown_home");
        let cwd = temp_dir("unknown_cwd");
        let store = WorkflowStore::new(&home, &cwd).unwrap();
        let event = LaneEvent::status_changed(
            "evt_unknown",
            "lane_missing",
            LaneStatus::Running,
            "must not exist",
            10,
            None,
        );

        assert!(store.append_lane_event_checked(&event).is_err());
        assert!(store.load_lane_events().unwrap().is_empty());
    }

    #[test]
    fn lane_agent_binding_replays_once_and_rejects_conflicting_identity() {
        let home = temp_dir("lane_agent_binding_replay_home");
        let cwd = temp_dir("lane_agent_binding_replay_cwd");
        let store = WorkflowStore::new(&home, &cwd).unwrap();
        store
            .bind_lane_agent_once("lane-cockpit", "viden", "session-native", 10)
            .unwrap();

        let restarted = WorkflowStore::new(&home, &cwd).unwrap();
        let bindings = restarted.load_lane_agent_bindings().unwrap();
        let binding = bindings.get("lane-cockpit").unwrap();
        assert_eq!(binding.agent_id, "viden");
        assert_eq!(binding.session_id, "session-native");

        let error = restarted
            .bind_lane_agent_once("lane-cockpit", "codex-acp", "session-acp", 11)
            .unwrap_err();
        assert!(error.contains("already bound"));
        assert!(error.contains("viden"));
        assert!(error.contains("codex-acp"));
    }

    #[test]
    fn lane_agent_binding_preserves_the_public_lane_event_kind_surface() {
        fn legacy_lane_event_name(kind: LaneEventKind) -> &'static str {
            match kind {
                LaneEventKind::Created { .. } => "created",
                LaneEventKind::Replaced { .. } => "replaced",
                LaneEventKind::StatusChanged { .. } => "status_changed",
                LaneEventKind::Archived { .. } => "archived",
                LaneEventKind::LegacyImported { .. } => "legacy_imported",
                LaneEventKind::RunObserved { .. } => "run_observed",
            }
        }

        assert_eq!(
            legacy_lane_event_name(LaneEventKind::Archived {
                summary: "done".to_string(),
            }),
            "archived"
        );
    }

    #[test]
    fn lane_agent_binding_store_is_idempotent_and_conflict_atomic() {
        let home = temp_dir("lane_agent_binding_home");
        let cwd = temp_dir("lane_agent_binding_cwd");
        let store = WorkflowStore::new(&home, &cwd).unwrap();

        let first = store
            .bind_lane_agent_once("lane-cockpit", "viden", "session-native", 10)
            .unwrap();
        let repeated = store
            .bind_lane_agent_once("lane-cockpit", "viden", "session-native", 11)
            .unwrap();
        let error = store
            .bind_lane_agent_once("lane-cockpit", "codex-acp", "session-acp", 12)
            .unwrap_err();

        assert_eq!(repeated, first);
        assert!(error.contains("already bound"));
        assert!(store.load_lane_events().unwrap().is_empty());
        assert_eq!(
            store
                .load_lane_agent_bindings()
                .unwrap()
                .get("lane-cockpit")
                .unwrap()
                .agent_id,
            "viden"
        );
    }

    #[test]
    fn concurrent_lane_agent_binding_revalidation_publishes_one_identity() {
        let home = temp_dir("lane_agent_binding_race_home");
        let cwd = temp_dir("lane_agent_binding_race_cwd");
        let store = WorkflowStore::new(&home, &cwd).unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let attempts = [("viden", "session-native"), ("codex-acp", "session-acp")]
            .into_iter()
            .map(|(agent_id, session_id)| {
                let store = store.clone();
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    store.bind_lane_agent_once("lane-race", agent_id, session_id, 10)
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let results = attempts
            .into_iter()
            .map(|attempt| attempt.join().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
        assert!(store.load_lane_events().unwrap().is_empty());
        assert!(
            store
                .load_lane_agent_bindings()
                .unwrap()
                .contains_key("lane-race")
        );
    }

    fn observed_lane(lane_id: &str) -> AgentLaneRecord {
        serde_json::from_value(serde_json::json!({
            "id": lane_id,
            "task_id": null,
            "role": "coder",
            "route": "tmux",
            "gate_strength": "containment",
            "mutation_policy": "propose_only",
            "worktree": null,
            "branch": null,
            "target": "local",
            "data_egress": "deny",
            "status": "draft",
            "budget": {},
            "active_session_ids": [],
            "summary": "blind lane",
            "evidence": []
        }))
        .unwrap()
    }

    fn created(lane_id: &str) -> LaneEvent {
        LaneEvent::created("evt_created", observed_lane(lane_id), 1, None)
    }

    fn observation(
        event_id: &str,
        lane_id: &str,
        timestamp: u64,
        observation: LaneRunObservation,
    ) -> LaneEvent {
        LaneEvent::run_observed(event_id, lane_id, observation, timestamp, None)
    }

    #[test]
    fn run_observations_accumulate_bounded_lane_stats() {
        let events = vec![
            created("lane_blind"),
            observation("evt_a", "lane_blind", 1_000, LaneRunObservation::Started),
            observation(
                "evt_b",
                "lane_blind",
                1_007,
                LaneRunObservation::Stopped {
                    exit_code: Some(17),
                },
            ),
            observation("evt_c", "lane_blind", 1_010, LaneRunObservation::Started),
            observation(
                "evt_d",
                "lane_blind",
                1_013,
                LaneRunObservation::Stopped { exit_code: None },
            ),
            observation(
                "evt_e",
                "lane_blind",
                4_300,
                LaneRunObservation::Applied { diff_bytes: 640 },
            ),
            observation(
                "evt_f",
                "lane_blind",
                4_400,
                LaneRunObservation::Applied { diff_bytes: 60 },
            ),
        ];

        let state = reduce_lane_events(&events).unwrap();
        let stats = state.lane("lane_blind").unwrap().run_stats.unwrap();
        assert_eq!(stats.run_count, 2);
        // Seven plus three seconds of observed run time, expressed in ms.
        assert_eq!(stats.wall_time_ms, 10_000);
        assert_eq!(stats.diff_bytes, 700);
        assert_eq!(stats.last_exit_code, None);
    }

    #[test]
    fn a_lane_without_observations_has_absent_rather_than_zero_run_stats() {
        let state = reduce_lane_events(&[created("lane_quiet")]).unwrap();
        assert_eq!(state.lane("lane_quiet").unwrap().run_stats, None);
    }

    #[test]
    fn orphan_stop_records_the_exit_code_without_inventing_wall_time() {
        let events = vec![
            created("lane_blind"),
            observation(
                "evt_orphan",
                "lane_blind",
                9_000,
                LaneRunObservation::Stopped { exit_code: Some(2) },
            ),
        ];

        let state = reduce_lane_events(&events).unwrap();
        let stats = state.lane("lane_blind").unwrap().run_stats.unwrap();
        assert_eq!(stats.wall_time_ms, 0);
        assert_eq!(stats.run_count, 0);
        assert_eq!(stats.last_exit_code, Some(2));
    }

    #[test]
    fn a_stop_observed_before_its_start_timestamp_never_underflows_wall_time() {
        let events = vec![
            created("lane_blind"),
            observation("evt_a", "lane_blind", 5_000, LaneRunObservation::Started),
            observation(
                "evt_b",
                "lane_blind",
                1_000,
                LaneRunObservation::Stopped { exit_code: Some(0) },
            ),
        ];

        let state = reduce_lane_events(&events).unwrap();
        let stats = state.lane("lane_blind").unwrap().run_stats.unwrap();
        assert_eq!(stats.wall_time_ms, 0);
        assert_eq!(stats.run_count, 1);
        assert_eq!(stats.last_exit_code, Some(0));
    }

    #[test]
    fn run_observation_accumulation_saturates_instead_of_overflowing() {
        let events = vec![
            created("lane_blind"),
            observation(
                "evt_a",
                "lane_blind",
                1,
                LaneRunObservation::Applied {
                    diff_bytes: u64::MAX,
                },
            ),
            observation(
                "evt_b",
                "lane_blind",
                2,
                LaneRunObservation::Applied { diff_bytes: 8 },
            ),
            observation("evt_c", "lane_blind", 0, LaneRunObservation::Started),
            observation(
                "evt_d",
                "lane_blind",
                u64::MAX,
                LaneRunObservation::Stopped { exit_code: Some(0) },
            ),
            observation("evt_e", "lane_blind", 0, LaneRunObservation::Started),
            observation(
                "evt_f",
                "lane_blind",
                u64::MAX,
                LaneRunObservation::Stopped { exit_code: Some(0) },
            ),
        ];

        let state = reduce_lane_events(&events).unwrap();
        let stats = state.lane("lane_blind").unwrap().run_stats.unwrap();
        assert_eq!(stats.diff_bytes, u64::MAX);
        assert_eq!(stats.wall_time_ms, u64::MAX);
    }

    #[test]
    fn run_observation_on_an_unknown_lane_is_rejected_like_a_status_change() {
        let error = reduce_lane_events(&[observation(
            "evt_unknown",
            "lane_missing",
            10,
            LaneRunObservation::Started,
        )])
        .unwrap_err();
        assert!(
            error.contains("lane `lane_missing` does not exist"),
            "{error}"
        );
    }

    #[test]
    fn duplicate_run_observation_event_ids_are_rejected() {
        let events = vec![
            created("lane_blind"),
            observation("evt_same", "lane_blind", 1, LaneRunObservation::Started),
            observation("evt_same", "lane_blind", 2, LaneRunObservation::Started),
        ];
        let error = reduce_lane_events(&events).unwrap_err();
        assert!(
            error.contains("duplicate lane event id `evt_same`"),
            "{error}"
        );
    }

    #[test]
    fn run_observations_use_stable_snake_case_wire_names() {
        let event = observation(
            "evt_wire",
            "lane_blind",
            7,
            LaneRunObservation::Stopped { exit_code: Some(3) },
        );
        let encoded = serde_json::to_value(&event).unwrap();
        assert_eq!(encoded["kind"]["type"], "run_observed");
        assert_eq!(
            encoded["kind"]["observation"],
            serde_json::json!({"phase": "stopped", "exit_code": 3})
        );
        assert_eq!(
            serde_json::to_value(LaneRunObservation::Started).unwrap(),
            serde_json::json!({"phase": "started"})
        );
        assert_eq!(
            serde_json::to_value(LaneRunObservation::Applied { diff_bytes: 5 }).unwrap(),
            serde_json::json!({"phase": "applied", "diff_bytes": 5})
        );
        assert_eq!(serde_json::from_value::<LaneEvent>(encoded).unwrap(), event);
    }
}
