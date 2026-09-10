use clap::{Parser, Subcommand};

/// rr — remote run: a tiny process manager.
#[derive(Debug, Parser)]
#[command(name = "rr", version, args_conflicts_with_subcommands = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Cmd>,

    /// Name for the process (required when starting one).
    #[arg(long, short)]
    pub name: Option<String>,

    /// Command to run as a managed daemon, e.g. `rr -n api "bun start"`.
    #[arg(trailing_var_arg = true)]
    pub run: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// List managed processes.
    Ps,
    /// Interactive terminal UI.
    Tui,
    /// Stop a process (SIGTERM, then SIGKILL).
    Stop { name: String },
    /// Restart a process.
    Restart { name: String },
    /// Remove a stopped process from the registry.
    Rm { name: String },
    /// Tail logs of a process.
    Logs {
        name: String,
        /// Number of history lines to print first.
        #[arg(long, short = 'l', default_value_t = 50)]
        lines: usize,
        /// Print history and exit instead of following.
        #[arg(long)]
        no_follow: bool,
    },
    /// Run the daemon in the foreground (spawned internally).
    #[command(name = "__daemon", hide = true)]
    Daemon,
}
