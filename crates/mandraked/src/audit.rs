//! Audit log and event emission (ADR-0007).
//!
//! Every mutating call records one audit row and one event row, then
//! publishes the event on the bus. Secrets never reach either table.

use mandrake_core::{
    Actor, Timestamp,
    api::{AuditResult, Event, ObjectRef},
};
use serde_json::Value;

use crate::{app::AppState, error::ApiResult};

/// What to record about a call.
#[derive(Debug, Clone)]
pub struct Record {
    /// `<kind>.<verb>`, for example `user.create`.
    pub action: String,
    /// What was acted on.
    pub object: ObjectRef,
    /// Summary before.
    pub before: Option<Value>,
    /// Summary after.
    pub after: Option<Value>,
    /// Outcome.
    pub result: AuditResult,
    /// Free text.
    pub detail: Option<String>,
}

impl Record {
    /// A successful action on `object`.
    pub fn ok(action: &str, object: ObjectRef) -> Self {
        Self {
            action: action.to_owned(),
            object,
            before: None,
            after: None,
            result: AuditResult::Ok,
            detail: None,
        }
    }

    /// A denied action on `object`.
    pub fn denied(action: &str, object: ObjectRef, why: &str) -> Self {
        Self {
            result: AuditResult::Denied,
            detail: Some(why.to_owned()),
            ..Self::ok(action, object)
        }
    }

    /// Attach the before summary.
    #[must_use]
    pub fn before(mut self, v: Value) -> Self {
        self.before = Some(v);
        self
    }

    /// Attach the after summary.
    #[must_use]
    pub fn after(mut self, v: Value) -> Self {
        self.after = Some(v);
        self
    }
}

/// Per-request context an audit row needs.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Client address or `socket`.
    pub source: Option<String>,
    /// Request id from the `X-Request-Id` header.
    pub request_id: Option<String>,
}

impl AppState {
    /// Write the audit row and the event, then publish the event.
    pub async fn record(&self, actor: &Actor, ctx: &Context, rec: Record) -> ApiResult<()> {
        let at = Timestamp::now();
        let actor = actor.clone();
        let ctx = ctx.clone();
        let event_actor = actor.clone();
        let event_kind = rec.action.clone();
        let event_object = rec.object.clone();
        let event_data = rec.after.clone();
        let is_ok = rec.result == AuditResult::Ok;
        // Copies for the blocking closure; the originals build the bus event.
        let (db_actor, db_kind, db_object, db_data) = (
            event_actor.clone(),
            event_kind.clone(),
            event_object.clone(),
            event_data.clone(),
        );

        let event_id: i64 = self
            .db
            .call(move |conn| {
                let tx = conn.transaction()?;
                tx.execute(
                    "INSERT INTO audit (at, actor_id, actor_username, actor_role, actor_via, \
                     actor_token_id, action, object_kind, object_id, object_name, before, after, \
                     result, detail, request_id, source) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                    rusqlite::params![
                        at.to_rfc3339(),
                        actor.id.map(|i| i.to_string()),
                        actor.username,
                        actor.role.as_str(),
                        serde_json::to_value(actor.via)
                            .ok()
                            .and_then(|v| v.as_str().map(str::to_owned))
                            .unwrap_or_default(),
                        actor.token_id.map(|i| i.to_string()),
                        rec.action,
                        rec.object.kind,
                        rec.object.id.map(|i| i.to_string()),
                        rec.object.name,
                        rec.before.map(|v| v.to_string()),
                        rec.after.map(|v| v.to_string()),
                        rec.result.as_str(),
                        rec.detail,
                        ctx.request_id,
                        ctx.source,
                    ],
                )?;
                let mut event_id = 0;
                if is_ok {
                    tx.execute(
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
                    event_id = tx.last_insert_rowid();
                }
                tx.commit()?;
                Ok(event_id)
            })
            .await?;

        if is_ok {
            self.events.publish(Event {
                id: event_id.to_string(),
                at,
                kind: event_kind,
                object: Some(event_object),
                actor: Some(event_actor),
                data: event_data,
            });
        }
        Ok(())
    }
}
