//! Process registry with JSON persistence.
//!
//! Only the spec fields (name, command, cwd, `created_at`) are persisted;
//! runtime state (pid, status, counters) is rebuilt from scratch, so every
//! process loads as `Stopped` after a daemon restart.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::protocol::{ProcessInfo, ProcessStatus};
use crate::util::now_ts;

/// A managed process: its persisted spec plus its live runtime state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcEntry {
    pub name: String,
    pub command: String,
    pub cwd: String,
    pub created_at: i64,
    #[serde(skip)]
    pub status: ProcessStatus,
    #[serde(skip)]
    pub pid: Option<u32>,
    #[serde(skip)]
    pub started_at: Option<i64>,
    #[serde(skip)]
    pub restarts: u64,
    #[serde(skip)]
    pub last_exit_code: Option<i32>,
    #[serde(skip)]
    pub last_crash_at: Option<i64>,
}

impl ProcEntry {
    pub const fn new(name: String, command: String, cwd: String, created_at: i64) -> Self {
        Self {
            name,
            command,
            cwd,
            created_at,
            status: ProcessStatus::Stopped,
            pid: None,
            started_at: None,
            restarts: 0,
            last_exit_code: None,
            last_crash_at: None,
        }
    }

    fn info(&self) -> ProcessInfo {
        let uptime_secs = match (self.status, self.started_at) {
            (ProcessStatus::Running, Some(at)) => Some(u64::try_from(now_ts() - at).unwrap_or(0)),
            _ => None,
        };
        ProcessInfo {
            name: self.name.clone(),
            command: self.command.clone(),
            cwd: self.cwd.clone(),
            status: self.status,
            pid: self.pid,
            uptime_secs,
            restarts: self.restarts,
            last_exit_code: self.last_exit_code,
            last_crash_at: self.last_crash_at,
            created_at: self.created_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedState {
    processes: Vec<ProcEntry>,
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
                    .map(|entry| (entry.name.clone(), entry))
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
        let mut processes: Vec<ProcEntry> = entries.values().cloned().collect();
        processes.sort_by(|a, b| (a.created_at, &a.name).cmp(&(b.created_at, &b.name)));
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(&PersistedState { processes })?,
        )?;
        std::fs::rename(&tmp, &self.path)
    }

    /// A poisoned lock means another thread panicked mid-update; that is a
    /// bug, not a recoverable condition, so `expect` is appropriate.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, ProcEntry>> {
        self.entries.lock().expect("state lock poisoned")
    }

    /// Register a new process. Fails if the name is taken.
    // The guard is held across `save_locked` on purpose: the on-disk snapshot
    // must be the same map we just mutated, and serializing concurrent
    // insert/remove calls through the lock prevents a lost update on disk.
    #[allow(clippy::significant_drop_tightening)]
    pub fn insert(&self, entry: ProcEntry) -> Result<(), String> {
        let mut entries = self.lock();
        if entries.contains_key(&entry.name) {
            return Err(format!("process '{}' already exists", entry.name));
        }
        entries.insert(entry.name.clone(), entry);
        self.save_locked(&entries)
            .map_err(|e| format!("failed to persist state: {e}"))
    }

    /// Remove a process. Fails if it is not currently stopped/crashed.
    // See `insert` for why the lock intentionally spans the persistence write.
    #[allow(clippy::significant_drop_tightening)]
    pub fn remove(&self, name: &str) -> Result<(), String> {
        let mut entries = self.lock();
        let entry = entries
            .get(name)
            .ok_or_else(|| format!("no such process: {name}"))?;
        if matches!(
            entry.status,
            ProcessStatus::Running | ProcessStatus::Backoff
        ) {
            return Err(format!(
                "process '{name}' is {}; stop it first",
                entry.status
            ));
        }
        entries.remove(name);
        self.save_locked(&entries)
            .map_err(|e| format!("failed to persist state: {e}"))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.lock().contains_key(name)
    }

    pub fn info(&self, name: &str) -> Option<ProcessInfo> {
        self.lock().get(name).map(ProcEntry::info)
    }

    /// All processes, oldest first.
    pub fn list(&self) -> Vec<ProcessInfo> {
        let mut infos: Vec<ProcessInfo> = self.lock().values().map(ProcEntry::info).collect();
        infos.sort_by(|a, b| (a.created_at, &a.name).cmp(&(b.created_at, &b.name)));
        infos
    }

    /// Mutate one entry's runtime state (not persisted).
    pub fn update<F: FnOnce(&mut ProcEntry)>(&self, name: &str, f: F) {
        if let Some(entry) = self.lock().get_mut(name) {
            f(entry);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str) -> ProcEntry {
        ProcEntry::new(name.into(), "sleep 60".into(), "/tmp".into(), now_ts())
    }

    fn temp_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::load(dir.path().join("state.json")).unwrap();
        (dir, store)
    }

    #[test]
    fn insert_and_list() {
        let (_dir, store) = temp_store();
        store.insert(entry("api")).unwrap();
        store.insert(entry("web")).unwrap();
        let names: Vec<String> = store.list().into_iter().map(|i| i.name).collect();
        assert_eq!(names, vec!["api", "web"]);
    }

    #[test]
    fn duplicate_name_rejected() {
        let (_dir, store) = temp_store();
        store.insert(entry("api")).unwrap();
        assert!(store.insert(entry("api")).is_err());
    }

    #[test]
    fn remove_running_rejected() {
        let (_dir, store) = temp_store();
        store.insert(entry("api")).unwrap();
        store.update("api", |e| e.status = ProcessStatus::Running);
        assert!(store.remove("api").is_err());
        store.update("api", |e| e.status = ProcessStatus::Stopped);
        store.remove("api").unwrap();
        assert!(!store.contains("api"));
    }

    #[test]
    fn persistence_roundtrip_loads_as_stopped() {
        let (dir, store) = temp_store();
        store.insert(entry("api")).unwrap();
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
