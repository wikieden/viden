//! Workflow event-log persistence.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use viden_session::project_key_for_path;
use viden_types::{AuditPage, AuditQuery, AuditRecord};

use crate::audit::{audit_page, revalidate_audit_record};
use crate::lanes::{
    LEGACY_LANES_MIGRATION_ID, LaneEvent, LaneState, LegacyLaneImportOutcome,
    parse_legacy_lanes_tsv, reduce_lane_events,
};
use crate::memory::{MemoryEvent, MemoryState, reduce_memory_events};
use crate::tasks::{TaskEvent, TaskState, reduce_task_events};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowPaths {
    pub home_dir: PathBuf,
    pub projects_dir: PathBuf,
    pub project_dir: PathBuf,
    pub tasks_log: PathBuf,
    pub memory_log: PathBuf,
    pub agent_log: PathBuf,
    pub lanes_log: PathBuf,
    pub lanes_lock: PathBuf,
    /// Canonical append-only audit timeline. SQLite is not involved: see
    /// `crate::audit`.
    pub audit_log: PathBuf,
    pub index_db_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct WorkflowStore {
    paths: WorkflowPaths,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowTaskEvent {
    pub event_id: String,
    pub task_id: String,
    pub event_type: String,
    pub timestamp: u64,
    pub origin_session_id: Option<String>,
    pub payload: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowMemoryEvent {
    pub event_id: String,
    pub memory_id: String,
    pub event_type: String,
    pub timestamp: u64,
    pub origin_session_id: Option<String>,
    pub payload: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowAgentEvent {
    pub event_id: String,
    pub dag_id: String,
    pub task_id: Option<String>,
    pub event_type: String,
    pub timestamp: u64,
    pub origin_session_id: Option<String>,
    pub payload: BTreeMap<String, String>,
}

/// Durable execution identity for one Lane, stored outside the public Lane
/// lifecycle event schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaneAgentBinding {
    pub lane_id: String,
    pub agent_id: String,
    pub session_id: String,
    pub event_id: String,
    pub timestamp: u64,
}

impl WorkflowStore {
    pub fn new(home_dir: impl Into<PathBuf>, cwd: impl AsRef<Path>) -> Result<Self, String> {
        let home_dir = home_dir.into();
        let projects_dir = home_dir.join("workflows").join("projects");
        let project_dir = projects_dir.join(project_key_for_path(cwd.as_ref()));
        let paths = WorkflowPaths {
            tasks_log: project_dir.join("tasks.jsonl"),
            memory_log: project_dir.join("memory.jsonl"),
            agent_log: project_dir.join("agents.jsonl"),
            lanes_log: project_dir.join("lanes.jsonl"),
            lanes_lock: project_dir.join("lanes.lock"),
            audit_log: project_dir.join("audit.jsonl"),
            index_db_path: project_dir.join("workflow.sqlite3"),
            home_dir,
            projects_dir,
            project_dir,
        };
        fs::create_dir_all(&paths.project_dir).map_err(|err| err.to_string())?;
        Ok(Self { paths })
    }

    pub fn paths(&self) -> &WorkflowPaths {
        &self.paths
    }

    pub fn append_task_event(&self, event: &WorkflowTaskEvent) -> Result<(), String> {
        append_json_line(&self.paths.tasks_log, event)
    }

    pub fn append_memory_event(&self, event: &WorkflowMemoryEvent) -> Result<(), String> {
        append_json_line(&self.paths.memory_log, event)
    }

    pub fn append_agent_event(&self, event: &WorkflowAgentEvent) -> Result<(), String> {
        append_json_line(&self.paths.agent_log, event)
    }

    /// Appends one audit fact to the canonical append-only timeline.
    ///
    /// The record is re-validated against the sanitization bounds first: a
    /// record that did not come from `AuditRecord::sanitized` must never reach
    /// the durable log. The append itself is unconditional and lock-free, the
    /// same as the task log, because the timeline is never reduced or
    /// rewritten — only appended and scanned.
    pub fn append_audit_record(&self, record: &AuditRecord) -> Result<(), String> {
        revalidate_audit_record(record)?;
        append_json_line(&self.paths.audit_log, record)
    }

    /// Returns one newest-first page of the audit timeline.
    ///
    /// Read-only JSONL scan. A malformed line is a hard error rather than a
    /// skipped line: an audit answer that silently omits records would be read
    /// as evidence that nothing happened. For the same reason a query that
    /// cannot mean what it says (an inverted time range) is rejected rather
    /// than answered with an empty page.
    pub fn query_audit(&self, query: &AuditQuery) -> Result<AuditPage, String> {
        query.validate()?;
        let records = self.load_audit_records()?;
        Ok(audit_page(records, query))
    }

    /// Loads every audit record in file order.
    pub fn load_audit_records(&self) -> Result<Vec<AuditRecord>, String> {
        let path = &self.paths.audit_log;
        if !path.exists() {
            return Ok(Vec::new());
        }
        let contents = fs::read_to_string(path).map_err(|err| err.to_string())?;
        let mut records = Vec::new();
        for (index, line) in contents.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let record: AuditRecord = serde_json::from_str(line).map_err(|err| {
                format!(
                    "malformed audit record on line {} of {}: {err}",
                    index + 1,
                    path.display()
                )
            })?;
            records.push(record);
        }
        Ok(records)
    }

    pub fn load_task_events(&self) -> Result<Vec<WorkflowTaskEvent>, String> {
        load_json_lines(&self.paths.tasks_log)
    }

    pub fn load_memory_events(&self) -> Result<Vec<WorkflowMemoryEvent>, String> {
        load_json_lines(&self.paths.memory_log)
    }

    pub fn load_agent_events(&self) -> Result<Vec<WorkflowAgentEvent>, String> {
        load_json_lines(&self.paths.agent_log)
    }

    pub fn append_task_domain_event(&self, event: &TaskEvent) -> Result<(), String> {
        append_json_line(&self.paths.tasks_log, event)
    }

    pub fn append_task_domain_event_checked(&self, event: &TaskEvent) -> Result<(), String> {
        let mut events = self.load_task_domain_events()?;
        events.push(event.clone());
        reduce_task_events(&events)?;
        self.append_task_domain_event(event)
    }

    pub fn load_task_domain_events(&self) -> Result<Vec<TaskEvent>, String> {
        load_json_lines(&self.paths.tasks_log)
    }

    pub fn load_task_state(&self) -> Result<TaskState, String> {
        reduce_task_events(&self.load_task_domain_events()?)
    }

    pub fn append_memory_domain_event(&self, event: &MemoryEvent) -> Result<(), String> {
        append_json_line(&self.paths.memory_log, event)
    }

    pub fn append_memory_domain_event_checked(&self, event: &MemoryEvent) -> Result<(), String> {
        let mut events = self.load_memory_domain_events()?;
        events.push(event.clone());
        reduce_memory_events(&events)?;
        self.append_memory_domain_event(event)
    }

    pub fn load_memory_domain_events(&self) -> Result<Vec<MemoryEvent>, String> {
        load_json_lines(&self.paths.memory_log)
    }

    pub fn load_memory_state(&self) -> Result<MemoryState, String> {
        reduce_memory_events(&self.load_memory_domain_events()?)
    }

    pub fn append_lane_event(&self, event: &LaneEvent) -> Result<(), String> {
        let _lock = self.lock_lanes_exclusive()?;
        self.append_lane_event_unlocked(event)
    }

    pub fn append_lane_event_checked(&self, event: &LaneEvent) -> Result<(), String> {
        let _lock = self.lock_lanes_exclusive()?;
        let mut events = self.load_lane_events_unlocked()?;
        events.push(event.clone());
        reduce_lane_events(&events)?;
        self.append_lane_event_unlocked(event)
    }

    pub fn load_lane_events(&self) -> Result<Vec<LaneEvent>, String> {
        let _lock = self.lock_lanes_shared()?;
        self.load_lane_events_unlocked()
    }

    pub fn load_lane_state(&self) -> Result<LaneState, String> {
        reduce_lane_events(&self.load_lane_events()?)
    }

    pub fn bind_lane_agent_once(
        &self,
        lane_id: &str,
        agent_id: &str,
        session_id: &str,
        timestamp: u64,
    ) -> Result<LaneAgentBinding, String> {
        let _lock = self.lock_lane_agent_bindings_exclusive()?;
        let bindings = self.load_lane_agent_binding_records_unlocked()?;
        let state = reduce_lane_agent_bindings(&bindings)?;
        if let Some(existing) = state.get(lane_id) {
            if existing.agent_id == agent_id && existing.session_id == session_id {
                return Ok(existing.clone());
            }
            return Err(format!(
                "lane `{lane_id}` is already bound to agent `{}` session `{}`; cannot bind agent `{agent_id}` session `{session_id}`",
                existing.agent_id, existing.session_id
            ));
        }
        let binding = LaneAgentBinding {
            event_id: format!("lane-agent-bound:{lane_id}"),
            lane_id: lane_id.to_string(),
            agent_id: agent_id.to_string(),
            session_id: session_id.to_string(),
            timestamp,
        };
        append_json_line(&self.lane_agent_bindings_log(), &binding)?;
        Ok(binding)
    }

    /// Re-stamps a Lane's durable execution identity onto a new session of the
    /// *same* agent, by appending a superseding row (E3c defect 34).
    ///
    /// `bind_lane_agent_once` is the right rule while a process lives: a Lane
    /// has one execution identity and a second claimant is refused. Across a
    /// restart it is the wrong rule, because the session it names is dead and
    /// the new process mints a different one — which is what left a restored
    /// Lane read-only forever. This is the one sanctioned way past it, and it
    /// is deliberately narrow: the agent may not change, only the session, so
    /// a Lane bound to an external agent can never be re-bound to the built-in
    /// runtime (or the reverse) by calling this.
    ///
    /// The earlier row stays on disk. The log is append-only; the reducer
    /// reads the last row for a lane as its current session.
    pub fn rebind_lane_agent_session(
        &self,
        lane_id: &str,
        agent_id: &str,
        session_id: &str,
        timestamp: u64,
    ) -> Result<LaneAgentBinding, String> {
        let _lock = self.lock_lane_agent_bindings_exclusive()?;
        let bindings = self.load_lane_agent_binding_records_unlocked()?;
        let state = reduce_lane_agent_bindings(&bindings)?;
        if let Some(existing) = state.get(lane_id) {
            if existing.agent_id != agent_id {
                return Err(format!(
                    "lane `{lane_id}` is bound to agent `{}`; cannot rebind it to agent `{agent_id}`",
                    existing.agent_id
                ));
            }
            if existing.session_id == session_id {
                return Ok(existing.clone());
            }
        }
        let binding = LaneAgentBinding {
            // Distinct from the first bind's id so the two rows are separable
            // in the log a reviewer reads.
            event_id: format!("lane-agent-rebound:{lane_id}:{session_id}"),
            lane_id: lane_id.to_string(),
            agent_id: agent_id.to_string(),
            session_id: session_id.to_string(),
            timestamp,
        };
        append_json_line(&self.lane_agent_bindings_log(), &binding)?;
        Ok(binding)
    }

    pub fn load_lane_agent_bindings(&self) -> Result<BTreeMap<String, LaneAgentBinding>, String> {
        let _lock = self.lock_lane_agent_bindings_shared()?;
        reduce_lane_agent_bindings(&self.load_lane_agent_binding_records_unlocked()?)
    }

    pub fn import_legacy_lanes_tsv_once(
        &self,
        legacy_path: impl AsRef<Path>,
        timestamp: u64,
        origin_session_id: Option<String>,
    ) -> Result<LegacyLaneImportOutcome, String> {
        // Import check, parse validation, reducer validation, and append share
        // one cross-process critical section so concurrent session startup can
        // never publish duplicate migration events.
        let _lock = self.lock_lanes_exclusive()?;
        let mut events = self.load_lane_events_unlocked()?;
        let state = reduce_lane_events(&events)?;
        if let Some(audit) = state.migration(LEGACY_LANES_MIGRATION_ID) {
            return Ok(LegacyLaneImportOutcome {
                imported: false,
                lane_count: audit.imported_lane_ids.len(),
            });
        }

        let raw = fs::read_to_string(legacy_path.as_ref()).map_err(|error| {
            format!(
                "failed to read legacy lanes {}: {error}",
                legacy_path.as_ref().display()
            )
        })?;
        let lanes = parse_legacy_lanes_tsv(&raw)?;
        let lane_count = lanes.len();
        // One event contains the entire validated import so the JSONL boundary
        // never publishes only a prefix of the legacy lane set.
        let event = LaneEvent::legacy_imported(
            "evt_legacy_lanes_tsv_v0",
            "project:.viden/lanes.tsv",
            lanes,
            timestamp,
            origin_session_id,
        );
        events.push(event.clone());
        reduce_lane_events(&events)?;
        self.append_lane_event_unlocked(&event)?;
        Ok(LegacyLaneImportOutcome {
            imported: true,
            lane_count,
        })
    }

    fn append_lane_event_unlocked(&self, event: &LaneEvent) -> Result<(), String> {
        append_json_line(&self.paths.lanes_log, event)
    }

    fn load_lane_events_unlocked(&self) -> Result<Vec<LaneEvent>, String> {
        load_json_lines(&self.paths.lanes_log)
    }

    fn lock_lanes_exclusive(&self) -> Result<fs::File, String> {
        let lock = open_lock_file(&self.paths.lanes_lock)?;
        lock.lock_exclusive().map_err(|error| error.to_string())?;
        Ok(lock)
    }

    fn lock_lanes_shared(&self) -> Result<fs::File, String> {
        let lock = open_lock_file(&self.paths.lanes_lock)?;
        lock.lock_shared().map_err(|error| error.to_string())?;
        Ok(lock)
    }

    fn lane_agent_bindings_log(&self) -> PathBuf {
        self.paths.project_dir.join("lane-agent-bindings.jsonl")
    }

    fn lane_agent_bindings_lock(&self) -> PathBuf {
        self.paths.project_dir.join("lane-agent-bindings.lock")
    }

    fn load_lane_agent_binding_records_unlocked(&self) -> Result<Vec<LaneAgentBinding>, String> {
        load_json_lines(&self.lane_agent_bindings_log())
    }

    fn lock_lane_agent_bindings_exclusive(&self) -> Result<fs::File, String> {
        let lock = open_lock_file(&self.lane_agent_bindings_lock())?;
        lock.lock_exclusive().map_err(|error| error.to_string())?;
        Ok(lock)
    }

    fn lock_lane_agent_bindings_shared(&self) -> Result<fs::File, String> {
        let lock = open_lock_file(&self.lane_agent_bindings_lock())?;
        lock.lock_shared().map_err(|error| error.to_string())?;
        Ok(lock)
    }

    pub fn rebuild_index(&self) -> Result<(), String> {
        if sqlite_available() {
            let sql = "CREATE TABLE IF NOT EXISTS workflow_events (
                event_id TEXT PRIMARY KEY,
                entity_kind TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                timestamp INTEGER NOT NULL
            );";
            run_sql(&self.paths.index_db_path, sql)?;
        } else if !self.paths.index_db_path.exists() {
            fs::write(&self.paths.index_db_path, []).map_err(|err| err.to_string())?;
        }
        Ok(())
    }
}

/// Reduces the append-only binding log to one identity per Lane.
///
/// Two different *agents* claiming one Lane is still a conflict and still
/// fails the whole read: that is the invariant the log exists to protect, and
/// a silent last-writer-wins there would let one agent take over another's
/// Lane. A later row for the same agent is a session re-stamp
/// (`rebind_lane_agent_session`, E3c defect 34) and supersedes the earlier
/// one, because the earlier session belonged to a process that is gone.
fn reduce_lane_agent_bindings(
    bindings: &[LaneAgentBinding],
) -> Result<BTreeMap<String, LaneAgentBinding>, String> {
    let mut state: BTreeMap<String, LaneAgentBinding> = BTreeMap::new();
    for binding in bindings {
        if let Some(existing) = state.get(&binding.lane_id)
            && existing.agent_id != binding.agent_id
        {
            return Err(format!(
                "lane `{}` has conflicting durable agent identities: agent `{}` session `{}` versus agent `{}` session `{}`",
                binding.lane_id,
                existing.agent_id,
                existing.session_id,
                binding.agent_id,
                binding.session_id
            ));
        }
        state.insert(binding.lane_id.clone(), binding.clone());
    }
    Ok(state)
}

fn append_json_line<T>(path: &Path, value: &T) -> Result<(), String>
where
    T: Serialize,
{
    let mut payload = serde_json::to_string(value).map_err(|err| err.to_string())?;
    payload.push('\n');
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| err.to_string())?;
    file.write_all(payload.as_bytes())
        .map_err(|err| err.to_string())
}

fn open_lock_file(path: &Path) -> Result<fs::File, String> {
    fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| error.to_string())
}

fn load_json_lines<T>(path: &Path) -> Result<Vec<T>, String>
where
    T: for<'de> Deserialize<'de>,
{
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = fs::read_to_string(path).map_err(|err| err.to_string())?;
    contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(|err| err.to_string()))
        .collect()
}

fn sqlite_available() -> bool {
    Command::new("sqlite3")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn run_sql(db_path: &Path, sql: &str) -> Result<String, String> {
    let output = Command::new("sqlite3")
        .arg(db_path)
        .arg(sql)
        .output()
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;

    use viden_types::fresh_id;

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("viden_workflows_{name}_{}", fresh_id("tmp")));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn workflow_store_paths_are_project_scoped() {
        let home = temp_dir("paths_home");
        let cwd = temp_dir("paths_cwd");
        let store = WorkflowStore::new(&home, &cwd).unwrap();

        assert_eq!(store.paths().home_dir, home);
        assert!(
            store
                .paths()
                .project_dir
                .starts_with(store.paths().projects_dir.clone())
        );
        assert_eq!(store.paths().tasks_log.file_name().unwrap(), "tasks.jsonl");
        assert_eq!(
            store.paths().memory_log.file_name().unwrap(),
            "memory.jsonl"
        );
        assert_eq!(store.paths().agent_log.file_name().unwrap(), "agents.jsonl");
        assert_eq!(store.paths().lanes_log.file_name().unwrap(), "lanes.jsonl");
        assert_eq!(store.paths().lanes_lock.file_name().unwrap(), "lanes.lock");
        assert_eq!(
            store.paths().index_db_path.file_name().unwrap(),
            "workflow.sqlite3"
        );
    }

    #[test]
    fn task_and_memory_events_roundtrip_through_jsonl() {
        let home = temp_dir("roundtrip_home");
        let cwd = temp_dir("roundtrip_cwd");
        let store = WorkflowStore::new(&home, &cwd).unwrap();

        let mut task_payload = BTreeMap::new();
        task_payload.insert("title".to_string(), "Plan workflow store".to_string());
        let task_event = WorkflowTaskEvent {
            event_id: "evt_task_1".to_string(),
            task_id: "task_1".to_string(),
            event_type: "task_created".to_string(),
            timestamp: 10,
            origin_session_id: Some("session_1".to_string()),
            payload: task_payload,
        };

        let mut memory_payload = BTreeMap::new();
        memory_payload.insert("content".to_string(), "Use append-only logs".to_string());
        let memory_event = WorkflowMemoryEvent {
            event_id: "evt_memory_1".to_string(),
            memory_id: "mem_1".to_string(),
            event_type: "memory_added".to_string(),
            timestamp: 20,
            origin_session_id: Some("session_1".to_string()),
            payload: memory_payload,
        };

        store.append_task_event(&task_event).unwrap();
        store.append_memory_event(&memory_event).unwrap();

        assert_eq!(store.load_task_events().unwrap(), vec![task_event]);
        assert_eq!(store.load_memory_events().unwrap(), vec![memory_event]);
    }

    #[test]
    fn workflow_index_rebuilds_from_event_logs() {
        let home = temp_dir("index_home");
        let cwd = temp_dir("index_cwd");
        let store = WorkflowStore::new(&home, &cwd).unwrap();
        store
            .append_task_event(&WorkflowTaskEvent {
                event_id: "evt_task_index".to_string(),
                task_id: "task_index".to_string(),
                event_type: "task_created".to_string(),
                timestamp: 30,
                origin_session_id: None,
                payload: BTreeMap::new(),
            })
            .unwrap();

        store.rebuild_index().unwrap();

        assert!(store.paths().index_db_path.exists());
    }

    #[test]
    fn agent_events_roundtrip_through_separate_jsonl() {
        let home = temp_dir("agent_home");
        let cwd = temp_dir("agent_cwd");
        let store = WorkflowStore::new(&home, &cwd).unwrap();

        let mut payload = BTreeMap::new();
        payload.insert("role".to_string(), "planner".to_string());
        let event = WorkflowAgentEvent {
            event_id: "evt_agent_1".to_string(),
            dag_id: "dag_1".to_string(),
            task_id: Some("agent_task_1".to_string()),
            event_type: "agent_task_queued".to_string(),
            timestamp: 40,
            origin_session_id: Some("session_1".to_string()),
            payload,
        };

        store.append_agent_event(&event).unwrap();

        assert_eq!(store.load_agent_events().unwrap(), vec![event]);
        assert!(store.load_task_domain_events().unwrap().is_empty());
    }
}
