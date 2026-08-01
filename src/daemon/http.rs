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

pub fn port() -> u16 {
    std::env::var("RR_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

#[derive(RustEmbed)]
#[folder = "dashboard/dist"]
struct Assets;

pub async fn serve(core: Arc<Core>) -> std::io::Result<()> {
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

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port()));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("rr: dashboard on http://{addr}");
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
    to_http(core.handle(Request::Ps).await)
}

#[derive(Deserialize)]
struct StartBody {
    name: String,
    command: String,
    cwd: Option<String>,
}

async fn start(State(core): State<Arc<Core>>, Json(body): Json<StartBody>) -> HttpResponse {
    let cwd = body.cwd.filter(|c| !c.trim().is_empty()).unwrap_or_else(|| {
        dirs::home_dir()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/".into())
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

fn default_lines() -> usize {
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
    let stream = BroadcastStream::new(rx).filter_map(move |item| match item {
        Ok(line) if line.name == name => {
            let data = serde_json::to_string(&line).ok()?;
            Some(Ok(Event::default().data(data)))
        }
        _ => None, // other processes' lines, or lagged receiver
    });
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
            (
                [(header::CONTENT_TYPE, mime.as_ref().to_string())],
                content.data.into_owned(),
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "dashboard not built").into_response(),
    }
}
