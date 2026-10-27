//! Filesystem layout for the rr data directory.
//!
//! Everything lives under `~/.rr` (override with `RR_DIR`, used by tests).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("RR_DIR") {
        return PathBuf::from(dir);
    }
    dirs::home_dir()
        .expect("cannot determine home directory")
        .join(".rr")
}

/// Create the data directory and make it private to the current user. Logs,
/// process state, and the control socket live here.
pub fn ensure_data_dir() -> std::io::Result<()> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    restrict(&dir, 0o700)
}

/// Set a file or directory mode. Used for logs, state, and the unix socket.
pub fn restrict(path: &Path, mode: u32) -> std::io::Result<()> {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

pub fn socket_path() -> PathBuf {
    data_dir().join("rr.sock")
}

pub fn state_path() -> PathBuf {
    data_dir().join("state.json")
}

pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

pub fn daemon_log_path() -> PathBuf {
    data_dir().join("daemon.log")
}

#[cfg(test)]
mod tests {
    use super::restrict;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn restrict_sets_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file");
        std::fs::write(&path, b"x").unwrap();
        restrict(&path, 0o600).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
