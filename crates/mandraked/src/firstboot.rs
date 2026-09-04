//! First boot: the console admin from `/etc/mandrake/firstboot.json`
//! (ADR-0014). The installer writes the file into the new boot
//! environment; the daemon applies it once, when no user exists yet,
//! and destroys it whatever happened, so the password never outlives the
//! first start.

use std::path::Path;

use mandrake_core::{Actor, Id, Role, Timestamp, actor::Via, api::ObjectRef};
use rusqlite::params;
use serde::Deserialize;

use crate::{
    app::AppState,
    audit::{Context, Record},
    auth::password,
    error::ApiError,
    routes::users::{self, validate_username},
};

/// The file's shape.
#[derive(Debug, Deserialize)]
struct FirstBoot {
    admin: Admin,
}

#[derive(Debug, Deserialize)]
struct Admin {
    username: String,
    password: String,
}

/// What happened to the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// No file.
    Absent,
    /// The admin was created.
    Created(String),
    /// Users existed already; the file was destroyed unapplied.
    Skipped,
    /// The file was unusable and has been destroyed.
    Invalid(String),
}

/// The synthetic actor audit rows name.
fn installer() -> Actor {
    Actor {
        id: None,
        username: "installer".to_owned(),
        role: Role::Admin,
        via: Via::Socket,
        token_id: None,
    }
}

/// Overwrite the file's bytes, then unlink it. Best effort: the unlink is
/// what matters, and the overwrite only shortens how long the password is
/// recoverable from the pool.
fn destroy(path: &Path) {
    if let Ok(meta) = std::fs::metadata(path) {
        let len = usize::try_from(meta.len()).unwrap_or(0);
        let _ = std::fs::write(path, vec![0u8; len]);
    }
    match std::fs::remove_file(path) {
        Ok(()) => tracing::info!(path = %path.display(), "first-boot file destroyed"),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "cannot remove the first-boot file");
        }
    }
}

/// Apply the first-boot file at `path`, if there is one. Database errors
/// leave the file in place so a restart can retry; everything else
/// destroys it.
pub async fn apply(state: &AppState, path: &Path) -> Result<Outcome, ApiError> {
    let raw = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Outcome::Absent),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "cannot read the first-boot file");
            return Ok(Outcome::Absent);
        }
    };
    let count: i64 = state
        .db
        .call(|conn| conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0)))
        .await?;
    if count > 0 {
        tracing::warn!(
            path = %path.display(),
            users = count,
            "first-boot file found but users exist; destroying it unapplied"
        );
        destroy(path);
        return Ok(Outcome::Skipped);
    }
    let parsed: Result<FirstBoot, String> = serde_json::from_slice::<FirstBoot>(&raw)
        .map_err(|e| format!("not the expected JSON: {e}"))
        .and_then(|f| {
            validate_username(&f.admin.username).map_err(|e| e.to_string())?;
            password::check_policy(&f.admin.password).map_err(str::to_owned)?;
            Ok(f)
        });
    let first = match parsed {
        Ok(f) => f,
        Err(why) => {
            tracing::error!(path = %path.display(), error = %why, "first-boot file rejected");
            destroy(path);
            return Ok(Outcome::Invalid(why));
        }
    };
    let hash = password::hash(&first.admin.password).map_err(ApiError::internal)?;
    let id = Id::new();
    let now = Timestamp::now().to_rfc3339();
    let username = first.admin.username.clone();
    let created = state
        .db
        .call(move |conn| {
            conn.execute(
                "INSERT INTO users (id, username, display_name, role, password_hash, disabled, created_at, updated_at) \
                 VALUES (?1, ?2, NULL, 'admin', ?3, 0, ?4, ?4)",
                params![id.to_string(), username, hash, now],
            )?;
            users::find(conn, id)
        })
        .await?;
    let Some(user) = created else {
        destroy(path);
        return Err(ApiError::internal("the first-boot admin was not stored"));
    };
    let ctx = Context {
        source: Some("firstboot".to_owned()),
        request_id: None,
    };
    state
        .record(
            &installer(),
            &ctx,
            Record::ok(
                "user.create",
                ObjectRef::new("user", user.id, &user.username),
            )
            .after(users::summary(&user)),
        )
        .await?;
    tracing::info!(username = %user.username, "first-boot admin created");
    destroy(path);
    Ok(Outcome::Created(user.username))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;
    use crate::{db::Db, drivers::Options};

    async fn state() -> AppState {
        let db = Db::open_in_memory().expect("db");
        AppState::with_options(db, Options::fake())
            .await
            .expect("state")
    }

    async fn user_count(state: &AppState) -> i64 {
        state
            .db
            .call(|conn| conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0)))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn creates_the_admin_once_and_destroys_the_file() {
        let state = state().await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("firstboot.json");
        std::fs::write(
            &path,
            r#"{ "admin": { "username": "cody", "password": "correct horse battery" } }"#,
        )
        .unwrap();

        let out = apply(&state, &path).await.unwrap();
        assert_eq!(out, Outcome::Created("cody".to_owned()));
        assert!(!path.exists());
        assert_eq!(user_count(&state).await, 1);
        let (role, hash): (String, String) = state
            .db
            .call(|conn| {
                conn.query_row(
                    "SELECT role, password_hash FROM users WHERE username = 'cody'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
            })
            .await
            .unwrap();
        assert_eq!(role, "admin");
        assert!(password::verify("correct horse battery", &hash));
        let actor: String = state
            .db
            .call(|conn| {
                conn.query_row(
                    "SELECT actor_username FROM audit WHERE action = 'user.create'",
                    [],
                    |r| r.get(0),
                )
            })
            .await
            .unwrap();
        assert_eq!(actor, "installer");

        // A second file with users present is destroyed unapplied.
        std::fs::write(
            &path,
            r#"{ "admin": { "username": "mallory", "password": "eight chars!" } }"#,
        )
        .unwrap();
        assert_eq!(apply(&state, &path).await.unwrap(), Outcome::Skipped);
        assert!(!path.exists());
        assert_eq!(user_count(&state).await, 1);
    }

    #[tokio::test]
    async fn absent_and_invalid_files() {
        let state = state().await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("firstboot.json");
        assert_eq!(apply(&state, &path).await.unwrap(), Outcome::Absent);

        std::fs::write(&path, "not json").unwrap();
        assert!(matches!(
            apply(&state, &path).await.unwrap(),
            Outcome::Invalid(_)
        ));
        assert!(!path.exists());

        std::fs::write(
            &path,
            r#"{ "admin": { "username": "Root", "password": "short" } }"#,
        )
        .unwrap();
        assert!(matches!(
            apply(&state, &path).await.unwrap(),
            Outcome::Invalid(_)
        ));
        assert!(!path.exists());
        assert_eq!(user_count(&state).await, 0);
    }
}
