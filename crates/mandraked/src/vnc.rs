//! `GET /vms/{id}/vnc`: the guest's VNC server relayed over a WebSocket
//! (ADR-0013). bhyve listens on a UNIX socket under the zone root; the
//! daemon runs `pfexec nc -U <socket>` and shuttles bytes both ways as
//! binary frames, so noVNC in the console speaks RFB to it directly. One
//! session per VM at a time; the VNC server is never exposed otherwise.

use std::process::Stdio;

use axum::{
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::Response,
};
use mandrake_bhyve::vnc_socket_path;
use mandrake_core::{Id, Role, api::ObjectRef, zone::ZoneState};
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStdin, ChildStdout, Command},
};

use crate::{
    app::AppState,
    auth::Auth,
    error::{ApiError, ApiResult},
    vms::{self, FAMILY},
};

fn busy(detail: &str) -> ApiError {
    ApiError::typed(StatusCode::CONFLICT, "busy", "Conflict").detail(detail)
}

/// `GET /vms/{id}/vnc`.
pub async fn attach(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Id>,
    ws: WebSocketUpgrade,
) -> ApiResult<Response> {
    auth.require(Role::Operator)?;
    let (id, info) = vms::find_vm(&state, id).await?;
    let name = info.zone.summary.name.clone();
    if info.zone.summary.state != ZoneState::Running {
        return Err(busy("the VM is not running"));
    }
    if !info.config.vnc {
        return Err(ApiError::unprocessable("VNC is off for this VM"));
    }
    let socket_path = vnc_socket_path(&info.zone.summary.zonepath);
    if !state.vnc_sessions.claim(&name) {
        return Err(busy("a VNC session is already attached to this VM"));
    }
    let actor = auth.actor.clone();
    Ok(ws.on_upgrade(move |socket| async move {
        state
            .emit(
                "vm.vnc",
                ObjectRef::new(FAMILY, id, &name),
                Some(actor),
                Some(json!({ "attached": true })),
            )
            .await;
        run(socket, &name, &socket_path).await;
        state.vnc_sessions.release(&name);
        state
            .emit(
                "vm.vnc",
                ObjectRef::new(FAMILY, id, &name),
                None,
                Some(json!({ "attached": false })),
            )
            .await;
    }))
}

/// `nc -U` with both pipes in hand.
struct Relay {
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
}

fn spawn_relay(socket_path: &str) -> Result<Relay, String> {
    let mut child = Command::new(mandrake_core::shell::PFEXEC)
        .args(["nc", "-U", socket_path])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("cannot start nc: {e}"))?;
    let stdin = child.stdin.take().ok_or("nc has no stdin")?;
    let stdout = child.stdout.take().ok_or("nc has no stdout")?;
    Ok(Relay {
        child,
        stdin,
        stdout,
    })
}

async fn run(mut socket: WebSocket, vm: &str, socket_path: &str) {
    let mut relay = match spawn_relay(socket_path) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(vm, error = %e, "vnc relay unavailable");
            let _ = socket
                .send(Message::Text(format!("[vnc unavailable: {e}]").into()))
                .await;
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    };
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        tokio::select! {
            read = relay.stdout.read(&mut buf) => match read {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if socket.send(Message::Binary(buf[..n].to_vec().into())).await.is_err() {
                        break;
                    }
                }
            },
            frame = socket.recv() => match frame {
                Some(Ok(Message::Binary(bytes))) => {
                    if relay.stdin.write_all(&bytes).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Text(text))) => {
                    if relay.stdin.write_all(text.as_bytes()).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
            }
        }
    }
    let _ = relay.child.start_kill();
    let _ = relay.child.wait().await;
    let _ = socket.send(Message::Close(None)).await;
}
