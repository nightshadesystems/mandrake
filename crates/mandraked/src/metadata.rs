//! Per-object metadata: display name, description, tags, notes (ADR-0002).
//! Keyed by the object's Mandrake id; the object itself lives in illumos.

use std::collections::HashMap;

use mandrake_core::{Id, Timestamp, api::Metadata};
use rusqlite::{Connection, OptionalExtension, params};

/// Read metadata for one object.
pub fn get(conn: &Connection, id: Id) -> rusqlite::Result<Option<Metadata>> {
    conn.query_row(
        "SELECT display_name, description, tags, notes FROM metadata WHERE object_id = ?1",
        [id.to_string()],
        row,
    )
    .optional()
}

/// Read metadata for many objects at once.
pub fn get_many(conn: &Connection, ids: &[Id]) -> rusqlite::Result<HashMap<Id, Metadata>> {
    let mut out = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT display_name, description, tags, notes FROM metadata WHERE object_id = ?1",
    )?;
    for id in ids {
        if let Some(m) = stmt.query_row([id.to_string()], row).optional()? {
            out.insert(*id, m);
        }
    }
    Ok(out)
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Metadata> {
    let tags: Option<String> = r.get("tags")?;
    Ok(Metadata {
        display_name: r.get("display_name")?,
        description: r.get("description")?,
        tags: tags.and_then(|t| serde_json::from_str(&t).ok()),
        notes: r.get("notes")?,
    })
}

/// Merge `patch` into the stored metadata; fields present in the patch
/// replace, absent ones stay. Returns the result.
pub fn merge(conn: &Connection, id: Id, patch: &Metadata) -> rusqlite::Result<Metadata> {
    let current = get(conn, id)?.unwrap_or_default();
    let merged = Metadata {
        display_name: patch.display_name.clone().or(current.display_name),
        description: patch.description.clone().or(current.description),
        tags: patch.tags.clone().or(current.tags),
        notes: patch.notes.clone().or(current.notes),
    };
    conn.execute(
        "INSERT INTO metadata (object_id, display_name, description, tags, notes, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(object_id) DO UPDATE SET display_name = excluded.display_name, \
         description = excluded.description, tags = excluded.tags, notes = excluded.notes, \
         updated_at = excluded.updated_at",
        params![
            id.to_string(),
            merged.display_name,
            merged.description,
            merged
                .tags
                .as_ref()
                .map(|t| serde_json::to_string(t).unwrap_or_default()),
            merged.notes,
            Timestamp::now().to_rfc3339(),
        ],
    )?;
    Ok(merged)
}

/// Drop metadata for an object that no longer exists.
pub fn remove(conn: &Connection, id: Id) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM metadata WHERE object_id = ?1",
        [id.to_string()],
    )?;
    Ok(())
}

/// Sweep rows whose object is not in `live`.
pub fn sweep(conn: &Connection, live: &[Id]) -> rusqlite::Result<usize> {
    let keep: std::collections::HashSet<String> = live.iter().map(ToString::to_string).collect();
    let mut stmt = conn.prepare("SELECT object_id FROM metadata")?;
    let all: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut removed = 0;
    for id in all.into_iter().filter(|id| !keep.contains(id)) {
        removed += conn.execute("DELETE FROM metadata WHERE object_id = ?1", [id])?;
    }
    Ok(removed)
}
