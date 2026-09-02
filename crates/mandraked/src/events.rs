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
