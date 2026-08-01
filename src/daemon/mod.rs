pub mod core;
pub mod http;
pub mod logs;
pub mod rpc;
pub mod state;
pub mod supervisor;

use tokio::net::{UnixListener, UnixStream};

use crate::paths;

/// Run the daemon in the foreground. Spawned detached by the CLI.
pub fn run() -> std::io::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async_main())
}

async fn async_main() -> std::io::Result<()> {
    std::fs::create_dir_all(paths::data_dir())?;
    let sock = paths::socket_path();

    // Singleton: a live daemon accepts connections. A connect failure with a
    // socket file present means a stale socket from a dead daemon.
    if UnixStream::connect(&sock).await.is_ok() {
        eprintln!("rr: daemon already running");
        std::process::exit(1);
    }
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock)?;

    let core = core::Core::new()?;
    println!("rr: daemon listening on {}", sock.display());

    tokio::select! {
        _ = rpc::serve(core.clone(), listener) => {}
        res = http::serve(core.clone()) => {
            if let Err(e) = res {
                eprintln!("rr: http server failed: {e}");
            }
        }
        _ = shutdown_signal() => {
            println!("rr: daemon shutting down");
        }
    }
    let _ = std::fs::remove_file(&sock);
    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    let mut int = signal(SignalKind::interrupt()).expect("install SIGINT handler");
    tokio::select! {
        _ = term.recv() => {}
        _ = int.recv() => {}
    }
}
