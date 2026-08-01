//! rr — remote run: a tiny process manager for long-running commands.
//!
//! A single binary with two roles: the CLI is a thin client speaking
//! newline-delimited JSON over a unix socket, and a detached daemon
//! ([`daemon`]) supervises processes, captures logs and serves the web
//! dashboard.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(unsafe_code)]
#![deny(
    unused,
    trivial_casts,
    trivial_numeric_casts,
    unused_qualifications,
    unused_import_braces
)]

mod cli;
mod client;
mod daemon;
mod paths;
mod protocol;
mod util;

use clap::Parser;
use cli::{Cli, Cmd};
use protocol::Request;

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Cmd::Daemon) => {
            if let Err(e) = daemon::run() {
                eprintln!("rr: daemon failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        Some(Cmd::Ps) => client::run(&Request::Ps),
        Some(Cmd::Stop { name }) => client::run(&Request::Stop { name }),
        Some(Cmd::Restart { name }) => client::run(&Request::Restart { name }),
        Some(Cmd::Rm { name }) => client::run(&Request::Remove { name }),
        Some(Cmd::Logs {
            name,
            lines,
            no_follow,
        }) => client::run(&Request::Logs {
            name,
            lines,
            follow: !no_follow,
        }),
        None => start_command(cli),
    };

    if let Err(message) = result {
        eprintln!("rr: {message}");
        std::process::exit(1);
    }
}

fn start_command(cli: Cli) -> Result<(), String> {
    if cli.run.is_empty() {
        return Err("nothing to run (see `rr --help`)".into());
    }
    let Some(name) = cli.name else {
        return Err("--name is required when starting a process".into());
    };
    let cwd = std::env::current_dir()
        .map_err(|e| format!("cannot determine working directory: {e}"))?
        .to_string_lossy()
        .into_owned();
    client::run(&Request::Start {
        name,
        command: cli.run.join(" "),
        cwd,
    })
}
