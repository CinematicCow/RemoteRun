//! HTTP API for the dashboard: REST endpoints mirroring the RPC, an SSE
//! stream for live logs, and the embedded dashboard static files.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response as HttpResponse};
use axum::routing::{delete, get, post};
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

use super::core::Core;
use crate::protocol::{Request, Response};

pub const DEFAULT_PORT: u16 = 7070;
const DEFAULT_HOST: std::net::Ipv4Addr = std::net::Ipv4Addr::LOCALHOST;

pub fn port() -> u16 {
    std::env::var("RR_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

/// Bind address for the dashboard. Defaults to loopback-only, since the
/// dashboard has no auth — set `RR_HOST` (e.g. to a LAN IP) to expose it
/// beyond this machine, understanding that anyone who can reach that address
/// gets unauthenticated control over every managed process.
fn host() -> std::net::Ipv4Addr {
    std::env::var("RR_HOST")
        .ok()
        .and_then(|h| h.parse().ok())
        .unwrap_or(DEFAULT_HOST)
}

/// Whether the dashboard may start processes. Off by default (safe): starting
/// arbitrary commands is the riskiest thing the unauthenticated dashboard can
/// do, so it must be explicitly enabled with `RR_DASHBOARD_START=1`. The CLI
/// can always start processes over the unix socket.
fn dashboard_start_enabled() -> bool {
    std::env::var("RR_DASHBOARD_START")
        .ok()
        .is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "on" | "yes"))
}

#[derive(RustEmbed)]
#[folder = "dashboard/dist"]
struct Assets;

/// Bind the dashboard port. Done before anything else at daemon startup so a
/// port conflict fails loudly instead of killing an already-serving daemon.
pub async fn bind() -> std::io::Result<tokio::net::TcpListener> {
    let addr = std::net::SocketAddr::from((host(), port()));
    tokio::net::TcpListener::bind(addr).await
}

pub async fn serve(core: Arc<Core>, listener: tokio::net::TcpListener) -> std::io::Result<()> {
    let app = axum::Router::new()
        .route("/api/ps", get(ps))
        .route("/api/start", post(start))
        .route("/api/processes/{name}/stop", post(stop))
        .route("/api/processes/{name}/restart", post(restart))
        .route("/api/processes/{name}", delete(remove))
        .route("/api/logs/{name}", get(logs_history))
        .route("/api/logs/{name}/stream", get(logs_stream))
        .fallback(static_asset)
        .with_state(core);

    println!("rr: dashboard on http://{}", listener.local_addr()?);
    axum::serve(listener, app).await
}

fn to_http(resp: Response) -> HttpResponse {
    match resp {
        Response::Ok | Response::Pong => Json(json!({ "ok": true })).into_response(),
        Response::Error { message } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": message })),
        )
            .into_response(),
        Response::Process { process } => Json(json!({ "process": process })).into_response(),
        Response::Processes { processes } => {
            Json(json!({ "processes": processes })).into_response()
        }
        Response::LogLine { .. } | Response::LogHistoryEnd => {
            (StatusCode::INTERNAL_SERVER_ERROR, "unexpected response").into_response()
        }
    }
}

async fn ps(State(core): State<Arc<Core>>) -> HttpResponse {
    match core.handle(Request::Ps).await {
        Response::Processes { processes } => Json(json!({
            "processes": processes,
            "startEnabled": dashboard_start_enabled(),
        }))
        .into_response(),
        other => to_http(other),
    }
}

#[derive(Deserialize)]
struct StartBody {
    name: String,
    command: String,
    cwd: Option<String>,
}

async fn start(State(core): State<Arc<Core>>, Json(body): Json<StartBody>) -> HttpResponse {
    if !dashboard_start_enabled() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "starting processes from the dashboard is disabled (set RR_DASHBOARD_START=1)"
            })),
        )
            .into_response();
    }
    let cwd = body.cwd.filter(|c| !c.trim().is_empty()).unwrap_or_else(|| {
        dirs::home_dir().map_or_else(|| "/".into(), |h| h.to_string_lossy().into_owned())
    });
    to_http(
        core.handle(Request::Start {
            name: body.name,
            command: body.command,
            cwd,
        })
        .await,
    )
}

async fn stop(State(core): State<Arc<Core>>, Path(name): Path<String>) -> HttpResponse {
    to_http(core.handle(Request::Stop { name }).await)
}

async fn restart(State(core): State<Arc<Core>>, Path(name): Path<String>) -> HttpResponse {
    to_http(core.handle(Request::Restart { name }).await)
}

async fn remove(State(core): State<Arc<Core>>, Path(name): Path<String>) -> HttpResponse {
    to_http(core.handle(Request::Remove { name }).await)
}

#[derive(Deserialize)]
struct HistoryQuery {
    #[serde(default = "default_lines")]
    lines: usize,
}

const fn default_lines() -> usize {
    100
}

async fn logs_history(
    State(core): State<Arc<Core>>,
    Path(name): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> HttpResponse {
    if !core.store.contains(&name) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("no such process: {name}") })),
        )
            .into_response();
    }
    Json(json!({ "lines": core.logs.history(&name, q.lines) })).into_response()
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
            (
                [(header::CONTENT_TYPE, mime.as_ref().to_string())],
                body,
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "dashboard not built").into_response(),
    }
}
