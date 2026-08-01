//! Process registry with JSON persistence.
//!
//! Only the specs are persisted; runtime state (pid, status, counters) is
//! rebuilt from scratch, so every process loads as `Stopped` after a daemon
//! restart.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::protocol::{ProcessInfo, ProcessStatus};
use crate::util::now_ts;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcSpec {
    pub name: String,
    pub command: String,
    pub cwd: String,
    pub created_at: i64,
}

#[derive(Debug)]
pub struct ProcEntry {
    pub spec: ProcSpec,
    pub status: ProcessStatus,
    pub pid: Option<u32>,
    pub started_at: Option<i64>,
    pub restarts: u64,
    pub last_exit_code: Option<i32>,
    pub last_crash_at: Option<i64>,
}

impl ProcEntry {
    fn new(spec: ProcSpec) -> Self {
        Self {
            spec,
            status: ProcessStatus::Stopped,
            pid: None,
            started_at: None,
            restarts: 0,
            last_exit_code: None,
            last_crash_at: None,
        }
    }

    fn info(&self) -> ProcessInfo {
        ProcessInfo {
            name: self.spec.name.clone(),
            command: self.spec.command.clone(),
            cwd: self.spec.cwd.clone(),
            status: self.status,
            pid: self.pid,
            uptime_secs: match (self.status, self.started_at) {
                (ProcessStatus::Running, Some(at)) => Some((now_ts() - at).max(0) as u64),
                _ => None,
            },
            restarts: self.restarts,
            last_exit_code: self.last_exit_code,
            last_crash_at: self.last_crash_at,
            created_at: self.spec.created_at,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistedState {
    processes: Vec<ProcSpec>,
}

#[derive(Debug)]
pub struct Store {
    path: PathBuf,
    entries: Mutex<HashMap<String, ProcEntry>>,
}

impl Store {
    /// Load the registry from `path`, or start empty if it doesn't exist.
    pub fn load(path: PathBuf) -> std::io::Result<Self> {
        let entries = match std::fs::read(&path) {
            Ok(bytes) => {
                let persisted: PersistedState = serde_json::from_slice(&bytes)
                    .map_err(|e| std::io::Error::other(format!("corrupt state file: {e}")))?;
                persisted
                    .processes
                    .into_iter()
                    .map(|spec| (spec.name.clone(), ProcEntry::new(spec)))
                    .collect()
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(e) => return Err(e),
        };
        Ok(Self {
            path,
            entries: Mutex::new(entries),
        })
    }

    fn save_locked(&self, entries: &HashMap<String, ProcEntry>) -> std::io::Result<()> {
        let mut specs: Vec<ProcSpec> = entries.values().map(|e| e.spec.clone()).collect();
        specs.sort_by_key(|s| (s.created_at, s.name.clone()));
        let state = PersistedState { processes: specs };
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&state)?)?;
        std::fs::rename(&tmp, &self.path)
    }

    /// Register a new process spec. Fails if the name is taken.
    pub fn insert(&self, spec: ProcSpec) -> Result<(), String> {
        let mut entries = self.entries.lock().unwrap();
        if entries.contains_key(&spec.name) {
            return Err(format!("process '{}' already exists", spec.name));
        }
        entries.insert(spec.name.clone(), ProcEntry::new(spec));
        self.save_locked(&entries)
            .map_err(|e| format!("failed to persist state: {e}"))
    }

    /// Remove a process. Fails if it is not currently stopped/crashed.
    pub fn remove(&self, name: &str) -> Result<(), String> {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries
            .get(name)
            .ok_or_else(|| format!("no such process: {name}"))?;
        if matches!(
            entry.status,
            ProcessStatus::Running | ProcessStatus::Backoff
        ) {
            return Err(format!("process '{name}' is {}; stop it first", entry.status));
        }
        entries.remove(name);
        self.save_locked(&entries)
            .map_err(|e| format!("failed to persist state: {e}"))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.lock().unwrap().contains_key(name)
    }

    pub fn info(&self, name: &str) -> Option<ProcessInfo> {
        self.entries.lock().unwrap().get(name).map(|e| e.info())
    }

    /// All processes, oldest first.
    pub fn list(&self) -> Vec<ProcessInfo> {
        let entries = self.entries.lock().unwrap();
        let mut infos: Vec<ProcessInfo> = entries.values().map(|e| e.info()).collect();
        infos.sort_by_key(|i| (i.created_at, i.name.clone()));
        infos
    }

    /// Mutate one entry's runtime state (not persisted).
    pub fn update<F: FnOnce(&mut ProcEntry)>(&self, name: &str, f: F) {
        if let Some(entry) = self.entries.lock().unwrap().get_mut(name) {
            f(entry);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str) -> ProcSpec {
        ProcSpec {
            name: name.into(),
            command: "sleep 60".into(),
            cwd: "/tmp".into(),
            created_at: now_ts(),
        }
    }

    fn temp_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::load(dir.path().join("state.json")).unwrap();
        (dir, store)
    }

    #[test]
    fn insert_and_list() {
        let (_dir, store) = temp_store();
        store.insert(spec("api")).unwrap();
        store.insert(spec("web")).unwrap();
        let names: Vec<String> = store.list().into_iter().map(|i| i.name).collect();
        assert_eq!(names, vec!["api", "web"]);
    }

    #[test]
    fn duplicate_name_rejected() {
        let (_dir, store) = temp_store();
        store.insert(spec("api")).unwrap();
        assert!(store.insert(spec("api")).is_err());
    }

    #[test]
    fn remove_running_rejected() {
        let (_dir, store) = temp_store();
        store.insert(spec("api")).unwrap();
        store.update("api", |e| e.status = ProcessStatus::Running);
        assert!(store.remove("api").is_err());
        store.update("api", |e| e.status = ProcessStatus::Stopped);
        store.remove("api").unwrap();
        assert!(!store.contains("api"));
    }

    #[test]
    fn persistence_roundtrip_loads_as_stopped() {
        let (dir, store) = temp_store();
        store.insert(spec("api")).unwrap();
        store.update("api", |e| {
            e.status = ProcessStatus::Running;
            e.pid = Some(1234);
            e.restarts = 3;
        });
        drop(store);

        let reloaded = Store::load(dir.path().join("state.json")).unwrap();
        let info = reloaded.info("api").unwrap();
        assert_eq!(info.status, ProcessStatus::Stopped);
        assert_eq!(info.pid, None);
        assert_eq!(info.restarts, 0);
        assert_eq!(info.command, "sleep 60");
    }
}
