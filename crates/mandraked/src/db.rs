//! SQLite metadata store (ADR-0009).
//!
//! One connection behind a mutex; every query runs on the blocking pool via
//! [`Db::call`]. Migrations are embedded SQL applied in order and tracked
//! with `PRAGMA user_version`.

use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use mandrake_core::{Id, Timestamp};
use rusqlite::{Connection, OpenFlags, Row, types::Type};

use crate::error::ApiError;

const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_images.sql"),
];

/// Errors opening or migrating the database.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    /// SQLite reported an error.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The file was written by a newer daemon.
    #[error("database schema version {found} is newer than this daemon supports ({supported})")]
    SchemaTooNew {
        /// Version in the file.
        found: u32,
        /// Highest version this binary knows.
        supported: u32,
    },
    /// The connection mutex was poisoned by a panic elsewhere.
    #[error("database lock poisoned")]
    Poisoned,
    /// Filesystem error creating the parent directory.
    #[error("creating {path}: {source}")]
    Dir {
        /// Directory that could not be created.
        path: String,
        /// Underlying error.
        source: std::io::Error,
    },
}

impl From<DbError> for ApiError {
    fn from(e: DbError) -> Self {
        Self::internal(e)
    }
}

/// Handle to the database, cheap to clone.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    /// Open (creating if needed) the database at `path` and migrate it.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| DbError::Dir {
                path: parent.display().to_string(),
                source,
            })?;
        }
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(path, flags)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::finish_open(conn)
    }

    /// A private in-memory database, for tests.
    pub fn open_in_memory() -> Result<Self, DbError> {
        Self::finish_open(Connection::open_in_memory()?)
    }

    fn finish_open(conn: Connection) -> Result<Self, DbError> {
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrate(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Run `f` with the connection on the blocking pool.
    pub async fn call<T, F>(&self, f: F) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        let result = tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock().map_err(|_| DbError::Poisoned)?;
            f(&mut guard).map_err(DbError::from)
        })
        .await?;
        result.map_err(ApiError::from)
    }
}

fn migrate(conn: &Connection) -> Result<(), DbError> {
    let found: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let supported = u32::try_from(MIGRATIONS.len()).unwrap_or(u32::MAX);
    if found > supported {
        return Err(DbError::SchemaTooNew { found, supported });
    }
    for (index, sql) in MIGRATIONS.iter().enumerate() {
        let version = u32::try_from(index + 1).unwrap_or(u32::MAX);
        if version <= found {
            continue;
        }
        tracing::info!(version, "applying database migration");
        conn.execute_batch("BEGIN")?;
        if let Err(e) = conn.execute_batch(sql) {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(e.into());
        }
        conn.pragma_update(None, "user_version", version)?;
        conn.execute_batch("COMMIT")?;
    }
    Ok(())
}

fn conversion_error(
    column: usize,
    e: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, Type::Text, Box::new(e))
}

/// Read an [`Id`] column.
pub fn get_id(row: &Row<'_>, column: &str) -> rusqlite::Result<Id> {
    let text: String = row.get(column)?;
    text.parse().map_err(|e| conversion_error(0, e))
}

/// Read a nullable [`Id`] column.
pub fn get_opt_id(row: &Row<'_>, column: &str) -> rusqlite::Result<Option<Id>> {
    let text: Option<String> = row.get(column)?;
    text.map(|t| t.parse().map_err(|e| conversion_error(0, e)))
        .transpose()
}

/// Read a [`Timestamp`] column.
pub fn get_ts(row: &Row<'_>, column: &str) -> rusqlite::Result<Timestamp> {
    let text: String = row.get(column)?;
    text.parse().map_err(|e| conversion_error(0, e))
}

/// Read a nullable [`Timestamp`] column.
pub fn get_opt_ts(row: &Row<'_>, column: &str) -> rusqlite::Result<Option<Timestamp>> {
    let text: Option<String> = row.get(column)?;
    text.map(|t| t.parse().map_err(|e| conversion_error(0, e)))
        .transpose()
}

/// Read a nullable JSON column.
pub fn get_opt_json(row: &Row<'_>, column: &str) -> rusqlite::Result<Option<serde_json::Value>> {
    let text: Option<String> = row.get(column)?;
    text.map(|t| serde_json::from_str(&t).map_err(|e| conversion_error(0, e)))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_a_fresh_database() {
        let db = Db::open_in_memory();
        assert!(db.is_ok());
    }

    #[test]
    fn refuses_a_newer_schema() {
        let conn = Connection::open_in_memory().ok();
        let Some(conn) = conn else {
            return;
        };
        let _ = conn.pragma_update(None, "user_version", 99);
        let err = migrate(&conn).err();
        assert!(matches!(err, Some(DbError::SchemaTooNew { found: 99, .. })));
    }
}
