//! `GET /zones/{id}/console` and `GET /vms/{id}/serial`: the zone console over a WebSocket, backed by
//! `pfexec zlogin -C` under a pseudo-terminal (ADR-0012).
//!
//! Server frames are terminal output. Client text or binary frames are
//! input, except a text frame that parses as `{"resize": {"cols", "rows"}}`,
//! which resizes the terminal. One session per zone at a time.

use std::{
    collections::HashSet,
    io::{Read, Write},
    sync::Mutex,
};

use axum::{
    extract::{
        Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::Response,
};
use mandrake_core::{Id, Role, api::ObjectRef};
use serde::Deserialize;
use serde_json::json;

use crate::{
    app::AppState,
    auth::Auth,
    error::{ApiError, ApiResult},
    vms, zones,
};

/// The escape character handed to `zlogin -e`: Ctrl-], so `~.` typed
/// inside the zone does not end the session.
const ESCAPE: &str = "\u{1d}";

/// Which zones have a console attached.
#[derive(Debug, Default)]
pub struct ConsoleSessions {
    attached: Mutex<HashSet<String>>,
}

impl ConsoleSessions {
    /// Whether `zone` has a session.
    pub fn contains(&self, zone: &str) -> bool {
        self.attached.lock().is_ok_and(|s| s.contains(zone))
    }

    /// Take the session for `zone`; `false` when one is attached already.
    pub fn claim(&self, zone: &str) -> bool {
        self.attached
            .lock()
            .is_ok_and(|mut s| s.insert(zone.to_owned()))
    }

    /// Give the session back.
    pub fn release(&self, zone: &str) {
        if let Ok(mut s) = self.attached.lock() {
            s.remove(zone);
        }
    }
}

/// `GET /zones/{id}/console` query.
#[derive(Debug, Default, Deserialize)]
pub struct ConsoleQuery {
    /// Columns.
    pub cols: Option<u16>,
    /// Rows.
    pub rows: Option<u16>,
}

/// A client-to-server control frame.
#[derive(Debug, Deserialize)]
struct Control {
    resize: Option<Size>,
}

/// Terminal size.
#[derive(Debug, Clone, Copy, Deserialize)]
struct Size {
    cols: u16,
    rows: u16,
}

/// Parse a text frame as a resize request, if it is one.
fn resize_request(text: &str) -> Option<Size> {
    if !text.trim_start().starts_with('{') {
        return None;
    }
    serde_json::from_str::<Control>(text).ok()?.resize
}

/// `GET /zones/{id}/console`.
pub async fn attach(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Id>,
    Query(q): Query<ConsoleQuery>,
    ws: WebSocketUpgrade,
) -> ApiResult<Response> {
    auth.require(Role::Operator)?;
    let (id, info) = zones::find_zone(&state, id).await?;
    let name = info.summary.name.clone();
    attach_named(state, &auth, ws, &q, "zone.console", id, name)
}

/// `GET /vms/{id}/serial`: the same console for a bhyve zone, where
/// `zlogin -C` is the guest's serial port.
pub async fn attach_vm(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Id>,
    Query(q): Query<ConsoleQuery>,
    ws: WebSocketUpgrade,
) -> ApiResult<Response> {
    auth.require(Role::Operator)?;
    let (id, info) = vms::find_vm(&state, id).await?;
    let name = info.zone.summary.name.clone();
    attach_named(state, &auth, ws, &q, "vm.serial", id, name)
}

/// Claim the session for `name` and upgrade; `event` is emitted with
/// `attached` true and false around the session.
fn attach_named(
    state: AppState,
    auth: &Auth,
    ws: WebSocketUpgrade,
    q: &ConsoleQuery,
    event: &'static str,
    id: Id,
    name: String,
) -> ApiResult<Response> {
    let family = event.split('.').next().unwrap_or("zone");
    if !state.console_sessions.claim(&name) {
        return Err(ApiError::typed(StatusCode::CONFLICT, "busy", "Conflict")
            .detail("a console session is already attached"));
    }
    let size = Size {
        cols: q.cols.unwrap_or(80).clamp(20, 500),
        rows: q.rows.unwrap_or(24).clamp(5, 200),
    };
    let actor = auth.actor.clone();
    Ok(ws.on_upgrade(move |socket| async move {
        state
            .emit(
                event,
                ObjectRef::new(family, id, &name),
                Some(actor),
                Some(json!({ "attached": true })),
            )
            .await;
        run(socket, &name, size).await;
        state.console_sessions.release(&name);
        state
            .emit(
                event,
                ObjectRef::new(family, id, &name),
                None,
                Some(json!({ "attached": false })),
            )
            .await;
    }))
}

/// What the WebSocket side sends to the terminal thread.
enum Input {
    Bytes(Vec<u8>),
    Resize(Size),
    Close,
}

/// A running `zlogin -C` under a pty, with threads bridging it.
struct Terminal {
    output: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    input: std::sync::mpsc::Sender<Input>,
}

fn pty_size(size: Size) -> portable_pty::PtySize {
    portable_pty::PtySize {
        rows: size.rows,
        cols: size.cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn spawn_terminal(zone: &str, size: Size) -> Result<Terminal, String> {
    let system = portable_pty::native_pty_system();
    let pair = system
        .openpty(pty_size(size))
        .map_err(|e| format!("cannot open a pty: {e}"))?;
    let mut cmd = portable_pty::CommandBuilder::new(mandrake_core::shell::PFEXEC);
    cmd.args(["zlogin", "-C", "-e", ESCAPE, zone]);
    cmd.env("TERM", "xterm-256color");
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("cannot start zlogin: {e}"))?;
    drop(pair.slave);
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("cannot read the pty: {e}"))?;
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("cannot write the pty: {e}"))?;
    let master = pair.master;

    let (out_tx, output) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if out_tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let (input, in_rx) = std::sync::mpsc::channel::<Input>();
    std::thread::spawn(move || {
        for msg in in_rx {
            match msg {
                Input::Bytes(b) => {
                    if writer.write_all(&b).and_then(|()| writer.flush()).is_err() {
                        break;
                    }
                }
                Input::Resize(size) => {
                    let _ = master.resize(pty_size(size));
                }
                Input::Close => break,
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        drop(master);
    });

    Ok(Terminal { output, input })
}

async fn run(mut socket: WebSocket, zone: &str, size: Size) {
    let mut terminal = match spawn_terminal(zone, size) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(zone, error = %e, "zone console unavailable");
            let _ = socket
                .send(Message::Text(
                    format!("\r\n[console unavailable: {e}]\r\n").into(),
                ))
                .await;
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    };
    loop {
        tokio::select! {
            out = terminal.output.recv() => {
                if let Some(bytes) = out {
                    if socket.send(Message::Binary(bytes.into())).await.is_err() {
                        break;
                    }
                } else {
                    let _ = socket
                        .send(Message::Text("\r\n[console closed]\r\n".into()))
                        .await;
                    break;
                }
            }
            frame = socket.recv() => {
                match frame {
                    Some(Ok(Message::Text(text))) => {
                        if let Some(size) = resize_request(&text) {
                            let _ = terminal.input.send(Input::Resize(size));
                        } else if terminal.input.send(Input::Bytes(text.as_bytes().to_vec())).is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Binary(bytes))) => {
                        if terminal.input.send(Input::Bytes(bytes.to_vec())).is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                }
            }
        }
    }
    let _ = terminal.input.send(Input::Close);
    let _ = socket.send(Message::Close(None)).await;
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn resize_frames_are_recognised() {
        let s = resize_request(r#"{"resize":{"cols":120,"rows":40}}"#).expect("resize");
        assert_eq!((s.cols, s.rows), (120, 40));
        assert!(resize_request("ls -l\r").is_none());
        assert!(resize_request("{ not json").is_none());
        assert!(resize_request(r#"{"other":1}"#).is_none());
    }

    #[test]
    fn sessions_are_exclusive() {
        let s = ConsoleSessions::default();
        assert!(s.claim("web"));
        assert!(!s.claim("web"));
        assert!(s.contains("web"));
        s.release("web");
        assert!(!s.contains("web"));
        assert!(s.claim("web"));
    }
}
