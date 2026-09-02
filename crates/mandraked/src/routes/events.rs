//! `GET /events`: the WebSocket event stream.

use axum::{
    extract::{
        Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::Response,
};
use mandrake_core::{
    Role,
    api::{Event, ObjectRef},
};
use serde::Deserialize;
use tokio::sync::broadcast::{Receiver, error::RecvError};

use crate::{app::AppState, auth::Auth, db, error::ApiResult};

/// Most events replayed for `since`.
const REPLAY_LIMIT: i64 = 1000;

/// `GET /events` query.
#[derive(Debug, Default, Deserialize)]
pub struct StreamQuery {
    /// Replay events with an id greater than this first.
    pub since: Option<String>,
}

/// `GET /events`.
pub async fn stream(
    State(state): State<AppState>,
    auth: Auth,
    Query(q): Query<StreamQuery>,
    ws: WebSocketUpgrade,
) -> ApiResult<Response> {
    auth.require(Role::Viewer)?;
    let since = q.since.and_then(|s| s.parse::<i64>().ok());
    // Subscribe before replaying so nothing published in between is lost.
    let rx = state.events.subscribe();
    Ok(ws.on_upgrade(move |socket| run(socket, state, rx, since)))
}

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    let id: i64 = r.get("id")?;
    let object_kind: Option<String> = r.get("object_kind")?;
    let actor: Option<String> = r.get("actor")?;
    Ok(Event {
        id: id.to_string(),
        at: db::get_ts(r, "at")?,
        kind: r.get("kind")?,
        object: object_kind
            .map(|kind| -> rusqlite::Result<ObjectRef> {
                Ok(ObjectRef {
                    kind,
                    id: db::get_opt_id(r, "object_id")?,
                    name: r.get("object_name")?,
                })
            })
            .transpose()?,
        actor: actor.and_then(|a| serde_json::from_str(&a).ok()),
        data: db::get_opt_json(r, "data")?,
    })
}

async fn send(socket: &mut WebSocket, event: &Event) -> bool {
    let Ok(text) = serde_json::to_string(event) else {
        return true;
    };
    socket.send(Message::Text(text.into())).await.is_ok()
}

async fn run(mut socket: WebSocket, state: AppState, mut rx: Receiver<Event>, since: Option<i64>) {
    let mut last_sent: Option<i64> = since;
    if let Some(since) = since {
        let replay = state
            .db
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, at, kind, object_kind, object_id, object_name, actor, data \
                     FROM events WHERE id > ?1 ORDER BY id LIMIT ?2",
                )?;
                let rows = stmt.query_map([since, REPLAY_LIMIT], from_row)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })
            .await
            .unwrap_or_default();
        for event in replay {
            if !send(&mut socket, &event).await {
                return;
            }
            last_sent = event.id.parse().ok();
        }
    }

    loop {
        tokio::select! {
            received = rx.recv() => match received {
                Ok(event) => {
                    let id = event.id.parse::<i64>().ok();
                    if last_sent.is_some_and(|l| id.is_some_and(|i| i <= l)) {
                        continue;
                    }
                    if !send(&mut socket, &event).await {
                        return;
                    }
                    last_sent = id;
                }
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_)) | Err(_)) | None => return,
                Some(Ok(_)) => {}
            },
        }
    }
}
