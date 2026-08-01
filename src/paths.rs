//! Filesystem layout for the rr data directory.
//!
//! Everything lives under `~/.rr` (override with `RR_DIR`, used by tests).

use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("RR_DIR") {
        return PathBuf::from(dir);
    }
    dirs::home_dir()
        .expect("cannot determine home directory")
        .join(".rr")
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
