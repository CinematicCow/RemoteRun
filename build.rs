//! Builds the web dashboard into `dashboard/dist` so rust-embed has assets
//! to embed. Reruns only when dashboard sources change.
//!
//! Escape hatches: set `RR_SKIP_DASHBOARD_BUILD=1` to embed a placeholder
//! page instead (no bun required); a machine without bun but with an
//! existing `dist/` reuses it as-is with a warning.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // Inputs only — never dist/, which this script writes.
    println!("cargo::rerun-if-changed=dashboard/src");
    println!("cargo::rerun-if-changed=dashboard/index.html");
    println!("cargo::rerun-if-changed=dashboard/package.json");
    println!("cargo::rerun-if-changed=dashboard/bun.lock");
    println!("cargo::rerun-if-changed=dashboard/vite.config.ts");
    println!("cargo::rerun-if-changed=dashboard/tsconfig.json");
    println!("cargo::rerun-if-env-changed=RR_SKIP_DASHBOARD_BUILD");

    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let dashboard = root.join("dashboard");
    let dist = dashboard.join("dist");

    if std::env::var_os("RR_SKIP_DASHBOARD_BUILD").is_some() {
        ensure_placeholder(&dist);
        return;
    }

    if !bun_available() {
        if dist.join("index.html").is_file() {
            println!("cargo::warning=bun not found; embedding existing dashboard/dist as-is");
            return;
        }
        die("bun is required to build the dashboard (https://bun.sh); \
             or set RR_SKIP_DASHBOARD_BUILD=1 to embed a placeholder page");
    }

    if !dashboard.join("node_modules").is_dir() {
        run_bun(&dashboard, &["install"]);
    }
    run_bun(&dashboard, &["run", "build"]);
}

fn bun_available() -> bool {
    Command::new("bun")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn run_bun(dir: &Path, args: &[&str]) {
    match Command::new("bun").args(args).current_dir(dir).status() {
        Ok(status) if status.success() => {}
        Ok(status) => die(&format!("`bun {}` failed: {status}", args.join(" "))),
        Err(e) => die(&format!("failed to run bun: {e}")),
    }
}

/// Minimal stand-in so rust-embed still compiles when the build is skipped.
fn ensure_placeholder(dist: &Path) {
    let index = dist.join("index.html");
    if index.is_file() {
        return;
    }
    std::fs::create_dir_all(dist).expect("create dashboard/dist");
    std::fs::write(
        index,
        "<!doctype html><html><head><meta charset=\"utf-8\"/><title>rr</title></head>\
         <body><p>Dashboard build was skipped (RR_SKIP_DASHBOARD_BUILD). \
         Rebuild rr without it to embed the real dashboard.</p></body></html>\n",
    )
    .expect("write placeholder index.html");
}

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1);
}
