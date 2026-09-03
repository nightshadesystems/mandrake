//! In-process event bus feeding the `/events` WebSocket.
//!
//! Events are also persisted (see `audit.rs`) so a client can resume with
//! `since`; the bus only carries live delivery.

use mandrake_core::api::Event;
use tokio::sync::broadcast;

/// Buffered events per subscriber before the slowest one starts losing.
const CAPACITY: usize = 1024;

/// Broadcast bus of [`Event`]s.
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Event>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    /// A new bus with no subscribers.
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(CAPACITY);
        Self { tx }
    }

    /// Deliver to every current subscriber. Nobody listening is fine.
    pub fn publish(&self, event: Event) {
        let _ = self.tx.send(event);
    }

    /// Start receiving events published from now on.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}

impl crate::app::AppState {
    /// Persist and broadcast an event about any object.
    pub async fn emit(
        &self,
        kind: &str,
        object: mandrake_core::api::ObjectRef,
        actor: Option<mandrake_core::Actor>,
        data: Option<serde_json::Value>,
    ) {
        let at = mandrake_core::Timestamp::now();
        let db_kind = kind.to_owned();
        let db_object = object.clone();
        let db_actor = actor.clone();
        let db_data = data.clone();
        let event_id = self
            .db
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO events (at, kind, object_kind, object_id, object_name, actor, data) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        at.to_rfc3339(),
                        db_kind,
                        db_object.kind,
                        db_object.id.map(|i| i.to_string()),
                        db_object.name,
                        serde_json::to_string(&db_actor).ok(),
                        db_data.as_ref().map(ToString::to_string),
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await
            .unwrap_or(0);
        self.events.publish(Event {
            id: event_id.to_string(),
            at,
            kind: kind.to_owned(),
            object: Some(object),
            actor,
            data,
        });
    }
}
