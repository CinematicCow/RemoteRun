mod cli;
mod daemon;
mod paths;
mod protocol;
mod util;

use clap::Parser;
use cli::{Cli, Cmd};

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Cmd::Daemon) => todo!("daemon"),
        Some(Cmd::Ps) => todo!("ps"),
        Some(Cmd::Stop { .. }) => todo!("stop"),
        Some(Cmd::Restart { .. }) => todo!("restart"),
        Some(Cmd::Rm { .. }) => todo!("rm"),
        Some(Cmd::Logs { .. }) => todo!("logs"),
        None => {
            if cli.run.is_empty() {
                eprintln!("usage: rr --name <name> \"<command>\"  (see rr --help)");
                std::process::exit(2);
            }
            todo!("start")
        }
    }
}
