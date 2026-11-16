//! WebSocket /ws/events — real-time push from EventBus.
//!
//! Bounded on three axes so a slow or hostile client cannot exhaust
//! resources:
//!   - concurrent connections capped at `ctx.max_sockets`
//!   - per-message size capped at 1 MiB (event JSON is already small;
//!     any larger frame is a protocol misuse)
//!   - idle timeout 60s per received frame: half-open clients are
//!     closed even if the peer keeps the TCP socket open

use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use tokio::sync::Semaphore;
use tracing::{debug, warn};

use crate::telemetry::ApiState;

/// 1 MiB is generous: the biggest event payload is a Wasm/rhai emit
/// already bounded elsewhere. A larger frame means either a broken
/// client or a deliberate abuse attempt.
const WS_MAX_MESSAGE_BYTES: usize = 1024 * 1024;

/// Client must send *something* (including ping/pong) at least this
/// often, otherwise the connection is closed. 60s matches most
/// browser WS keepalive conventions.
const WS_IDLE_TIMEOUT_SECS: u64 = 60;

/// Module-level connection cap. Initialized lazily from
/// `ctx.max_sockets` on the first upgrade; the orchestrator is a
/// single-process app so the singleton is safe.
fn ws_semaphore_sync(max: usize) -> &'static Arc<Semaphore> {
    use std::sync::OnceLock;
    static SEM: OnceLock<Arc<Semaphore>> = OnceLock::new();
    SEM.get_or_init(|| Arc::new(Semaphore::new(max)))
}

pub fn router() -> Router<ApiState> {
    Router::new().route("/ws/events", get(ws_handler))
}

async fn ws_handler(ws: WebSocketUpgrade, State(s): State<ApiState>) -> impl IntoResponse {
    let max = s.ctx.config.max_sockets;
    let sem = ws_semaphore_sync(max);

    let permit = match sem.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            warn!(max, "ws: connection cap reached, rejecting");
            return axum::response::Response::builder()
                .status(axum::http::StatusCode::SERVICE_UNAVAILABLE)
                .body(axum::body::Body::from("ws connection cap reached"))
                .unwrap_or_else(|_| axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response());
        }
    };

    let ws = ws.max_message_size(WS_MAX_MESSAGE_BYTES);
    ws.on_upgrade(move |socket| handle_socket(socket, s, permit))
}

async fn handle_socket(socket: WebSocket, s: ApiState, _permit: tokio::sync::OwnedSemaphorePermit) {
    debug!("ws: client connected");
    let mut rx = s.ctx.event_bus.subscribe();
    let (mut sender, mut receiver) = socket.split();

    let mut send_task = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let json = match serde_json::to_string(&event) {
                        Ok(j) => j,
                        Err(e) => {
                            warn!(err = %e, "ws: serialize failed");
                            continue;
                        }
                    };
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(missed = n, "ws: client lagged");
                }
            }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        loop {
            let next = tokio::time::timeout(
                std::time::Duration::from_secs(WS_IDLE_TIMEOUT_SECS),
                receiver.next(),
            )
            .await;
            match next {
                Err(_) => {
                    debug!("ws: idle timeout, closing");
                    break;
                }
                Ok(None) => break,
                Ok(Some(Err(_))) => break,
                Ok(Some(Ok(Message::Close(_)))) => break,
                Ok(Some(Ok(_))) => {}
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    }
    debug!("ws: client disconnected");
}
