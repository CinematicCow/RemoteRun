//! HTTP API for the dashboard: a JSON-RPC endpoint mirroring the unix socket
//! protocol, an SSE stream for live logs, and the embedded dashboard assets.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response as HttpResponse};
use axum::routing::{get, post};
use rust_embed::RustEmbed;
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

use super::core::Core;
use crate::protocol::{Request, Response};

pub const DEFAULT_PORT: u16 = 7070;
const BIND: std::net::Ipv4Addr = std::net::Ipv4Addr::LOCALHOST;

pub fn port() -> u16 {
    std::env::var("RR_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

#[derive(RustEmbed)]
#[folder = "dashboard/dist"]
struct Assets;

/// Bind the dashboard port on loopback. The dashboard has no authentication,
/// so it must never listen on a public or LAN address. Remote access belongs
/// to the network layer (Tailscale, SSH, Cloudflare Access).
pub async fn bind() -> std::io::Result<tokio::net::TcpListener> {
    let addr = std::net::SocketAddr::from((BIND, port()));
    tokio::net::TcpListener::bind(addr).await
}

pub async fn serve(core: Arc<Core>, listener: tokio::net::TcpListener) -> std::io::Result<()> {
    let app = axum::Router::new()
        .route("/api/rpc", post(rpc))
        .route("/api/logs/{name}/stream", get(logs_stream))
        .fallback(static_asset)
        .with_state(core);

    println!("rr: dashboard on http://{}", listener.local_addr()?);
    axum::serve(listener, app).await
}

/// Requests that change process state. The HTTP dashboard may only send these
/// when `RR_DASHBOARD_CONTROL` is set; the value is built into `Core` at
/// startup.
const fn requires_control(req: &Request) -> bool {
    matches!(
        req,
        Request::Start { .. }
            | Request::Stop { .. }
            | Request::Restart { .. }
            | Request::Remove { .. }
    )
}

/// Single entry point mirroring the unix socket protocol: the body is a
/// `Request`, the reply a `Response`. Transport-level failures use a non-2xx
/// status with an `error` field; RPC-level errors ride inside the `Response`.
async fn rpc(State(core): State<Arc<Core>>, Json(req): Json<Request>) -> HttpResponse {
    if requires_control(&req) && !core.dashboard_control {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "dashboard control is disabled (set RR_DASHBOARD_CONTROL=1)"
            })),
        )
            .into_response();
    }
    let resp = match req {
        Request::Logs { name, lines, .. } => match core.log_history(&name, lines) {
            Ok(lines) => Response::LogHistory { lines },
            Err(message) => {
                return (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response();
            }
        },
        other => core.handle(other).await,
    };
    Json(resp).into_response()
}

async fn logs_stream(
    State(core): State<Arc<Core>>,
    Path(name): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let rx = core.logs.subscribe();
    let live = BroadcastStream::new(rx).filter_map(move |item| match item {
        Ok(line) if line.name == name => {
            let data = serde_json::to_string(&line).ok()?;
            Some(Ok(Event::default().data(data)))
        }
        _ => None, // other processes' lines, or lagged receiver
    });
    // Emit an initial comment so the first bytes flush immediately. Without
    // it, buffering proxies hold the response until the first log line or
    // keep-alive tick, and EventSource sits in "connecting" for seconds.
    let stream = tokio_stream::iter([Ok::<Event, std::convert::Infallible>(
        Event::default().comment("connected"),
    )])
    .chain(live);
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn static_asset(uri: Uri) -> HttpResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    // SPA fallback: unknown paths serve the app shell.
    let asset = Assets::get(path).or_else(|| Assets::get("index.html"));
    match asset {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_text_plain();
            // Embedded (release) assets are 'static — serve them zero-copy.
            let body = match content.data {
                std::borrow::Cow::Borrowed(b) => axum::body::Bytes::from_static(b),
                std::borrow::Cow::Owned(v) => axum::body::Bytes::from(v),
            };
            ([(header::CONTENT_TYPE, mime.as_ref().to_string())], body).into_response()
        }
        None => (StatusCode::NOT_FOUND, "dashboard not built").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::core::Core;
    use crate::daemon::logs::LogManager;
    use crate::daemon::state::Store;
    use crate::daemon::supervisor::Supervisor;
    use axum::extract::State;
    use std::sync::Arc;

    fn test_core(control: bool) -> (tempfile::TempDir, Arc<Core>) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::load(dir.path().join("state.json")).unwrap());
        let logs = Arc::new(LogManager::new(dir.path().join("logs")).unwrap());
        let supervisor = Supervisor::new(store.clone(), logs.clone());
        (
            dir,
            Arc::new(Core {
                store,
                logs,
                supervisor,
                dashboard_control: control,
            }),
        )
    }

    fn mutations() -> [Request; 4] {
        [
            Request::Start {
                name: "x".into(),
                command: "true".into(),
                cwd: "/".into(),
            },
            Request::Stop { name: "x".into() },
            Request::Restart { name: "x".into() },
            Request::Remove { name: "x".into() },
        ]
    }

    #[test]
    fn control_predicate_covers_every_mutation() {
        for req in mutations() {
            assert!(requires_control(&req));
        }
        assert!(!requires_control(&Request::Ps));
        assert!(!requires_control(&Request::Logs {
            name: "x".into(),
            lines: 1,
            follow: false,
        }));
    }

    #[tokio::test]
    async fn read_only_blocks_every_mutation() {
        let (_dir, core) = test_core(false);
        for req in mutations() {
            let response = rpc(State(core.clone()), Json(req)).await;
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
    }

    #[tokio::test]
    async fn read_only_allows_reads() {
        let (_dir, core) = test_core(false);
        let response = rpc(State(core), Json(Request::Ps)).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn control_passes_mutations_through() {
        let (_dir, core) = test_core(true);
        let response = rpc(State(core), Json(Request::Stop { name: "x".into() })).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["kind"], "error");
    }
}
