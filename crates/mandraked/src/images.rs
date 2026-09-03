//! The image catalogue (ADR-0012): sources and their cached indexes,
//! imported images, the refresh, and the import job.
//!
//! Rows live in SQLite; the payloads live in ZFS through the images crate.

use mandrake_core::{
    Id, Timestamp,
    api::ObjectRef,
    image::{CatalogueEntry, Image, ImageSource, ImageState, ImageType},
    storage::PoolHealth,
};
use mandrake_images::{ImportPlan, Progress, index};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;

use crate::{
    app::AppState,
    error::{ApiError, ApiResult},
};

/// The user the daemon runs as; owns the staging and ISO directories.
pub const STORE_OWNER: &str = "mandrake";

// ------------------------------------------------------------ sources

fn source_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ImageSource> {
    let id: String = r.get("id")?;
    let created: String = r.get("created_at")?;
    let refreshed: Option<String> = r.get("last_refreshed_at")?;
    Ok(ImageSource {
        id: id.parse().unwrap_or_default(),
        name: r.get("name")?,
        url: r.get("url")?,
        public_key: r.get("public_key")?,
        enabled: r.get::<_, i64>("enabled")? != 0,
        builtin: r.get::<_, i64>("builtin")? != 0,
        verified: r.get::<_, i64>("verified")? != 0,
        image_count: u32::try_from(r.get::<_, i64>("image_count")?).unwrap_or(u32::MAX),
        last_refreshed_at: refreshed.and_then(|s| s.parse().ok()),
        last_error: r.get("last_error")?,
        created_at: created.parse().unwrap_or_else(|_| Timestamp::now()),
    })
}

const SOURCE_SELECT: &str = "SELECT s.*, \
    (SELECT COUNT(*) FROM image_catalogue c WHERE c.source_id = s.id) AS image_count \
    FROM image_sources s";

/// Every source, built-in first, then by name.
pub fn list_sources(conn: &Connection) -> rusqlite::Result<Vec<ImageSource>> {
    let mut stmt = conn.prepare(&format!("{SOURCE_SELECT} ORDER BY s.builtin DESC, s.name"))?;
    let rows = stmt.query_map([], source_row)?;
    rows.collect()
}

/// One source.
pub fn get_source(conn: &Connection, id: Id) -> rusqlite::Result<Option<ImageSource>> {
    conn.query_row(
        &format!("{SOURCE_SELECT} WHERE s.id = ?1"),
        [id.to_string()],
        source_row,
    )
    .optional()
}

/// Insert a user source.
pub fn insert_source(
    conn: &Connection,
    id: Id,
    name: &str,
    url: &str,
    public_key: Option<&str>,
    enabled: bool,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO image_sources (id, name, url, public_key, enabled, builtin, verified, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, ?6)",
        params![
            id.to_string(),
            name,
            url,
            public_key,
            i64::from(enabled),
            Timestamp::now().to_rfc3339()
        ],
    )?;
    Ok(())
}

/// Fields a source update may change.
#[derive(Debug, Default)]
pub struct SourcePatch {
    /// Name.
    pub name: Option<String>,
    /// URL.
    pub url: Option<String>,
    /// Key: `Some(None)` clears.
    pub public_key: Option<Option<String>>,
    /// Enabled.
    pub enabled: Option<bool>,
}

/// Apply a patch; a cleared or changed key marks the source unverified
/// until the next refresh.
pub fn update_source(conn: &Connection, id: Id, patch: &SourcePatch) -> rusqlite::Result<()> {
    let id = id.to_string();
    if let Some(n) = &patch.name {
        conn.execute(
            "UPDATE image_sources SET name = ?1 WHERE id = ?2",
            params![n, id],
        )?;
    }
    if let Some(u) = &patch.url {
        conn.execute(
            "UPDATE image_sources SET url = ?1, verified = 0 WHERE id = ?2",
            params![u, id],
        )?;
    }
    if let Some(k) = &patch.public_key {
        conn.execute(
            "UPDATE image_sources SET public_key = ?1, verified = 0 WHERE id = ?2",
            params![k, id],
        )?;
    }
    if let Some(e) = patch.enabled {
        conn.execute(
            "UPDATE image_sources SET enabled = ?1 WHERE id = ?2",
            params![i64::from(e), id],
        )?;
    }
    Ok(())
}

/// Remove a source and its catalogue.
pub fn delete_source(conn: &Connection, id: Id) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM image_sources WHERE id = ?1", [id.to_string()])
}

/// Replace a source's cached catalogue and record the refresh.
pub fn store_catalogue(
    conn: &mut Connection,
    source_id: Id,
    entries: &[(index::IndexEntry, String)],
    verified: bool,
) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    let sid = source_id.to_string();
    tx.execute("DELETE FROM image_catalogue WHERE source_id = ?1", [&sid])?;
    for (e, url) in entries {
        tx.execute(
            "INSERT OR REPLACE INTO image_catalogue \
             (source_id, name, version, type, url, sha256, size, description, os, published_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                sid,
                e.name,
                e.version,
                e.type_.as_str(),
                url,
                e.sha256.to_ascii_lowercase(),
                i64::try_from(e.size).unwrap_or(i64::MAX),
                e.description,
                e.os,
                e.published_at.map(Timestamp::to_rfc3339),
            ],
        )?;
    }
    tx.execute(
        "UPDATE image_sources SET verified = ?1, last_refreshed_at = ?2, last_error = NULL WHERE id = ?3",
        params![i64::from(verified), Timestamp::now().to_rfc3339(), sid],
    )?;
    tx.commit()
}

/// Record a failed refresh; the old catalogue stays.
pub fn record_refresh_error(conn: &Connection, source_id: Id, error: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE image_sources SET last_error = ?1, verified = 0 WHERE id = ?2",
        params![error, source_id.to_string()],
    )?;
    Ok(())
}

fn image_type(s: &str) -> ImageType {
    match s {
        "zone-native" => ImageType::ZoneNative,
        "zone-lx" => ImageType::ZoneLx,
        "vm-raw" => ImageType::VmRaw,
        _ => ImageType::VmIso,
    }
}

fn catalogue_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<CatalogueEntry> {
    let sid: String = r.get("source_id")?;
    let type_: String = r.get("type")?;
    let published: Option<String> = r.get("published_at")?;
    let image_id: Option<String> = r.get("image_id")?;
    Ok(CatalogueEntry {
        source_id: sid.parse().unwrap_or_default(),
        source_name: r.get("source_name")?,
        name: r.get("name")?,
        version: r.get("version")?,
        type_: image_type(&type_),
        url: r.get("url")?,
        sha256: r.get("sha256")?,
        size_bytes: u64::try_from(r.get::<_, i64>("size")?).unwrap_or(0),
        description: r.get("description")?,
        os: r.get("os")?,
        published_at: published.and_then(|s| s.parse().ok()),
        imported: image_id.is_some(),
        image_id: image_id.and_then(|s| s.parse().ok()),
    })
}

const CATALOGUE_SELECT: &str = "SELECT c.*, s.name AS source_name, \
    (SELECT i.id FROM images i WHERE i.sha256 = c.sha256 AND i.state != 'failed' LIMIT 1) AS image_id \
    FROM image_catalogue c JOIN image_sources s ON s.id = c.source_id";

/// Catalogue entries of enabled sources, optionally one source or type.
pub fn catalogue(
    conn: &Connection,
    source_id: Option<Id>,
    type_: Option<ImageType>,
) -> rusqlite::Result<Vec<CatalogueEntry>> {
    let mut stmt = conn.prepare(&format!(
        "{CATALOGUE_SELECT} WHERE s.enabled = 1 \
         AND (?1 IS NULL OR c.source_id = ?1) AND (?2 IS NULL OR c.type = ?2) \
         ORDER BY c.name, c.version DESC"
    ))?;
    let rows = stmt.query_map(
        params![
            source_id.map(|i| i.to_string()),
            type_.map(ImageType::as_str)
        ],
        catalogue_row,
    )?;
    rows.collect()
}

/// One catalogue entry by source, name, and version.
pub fn catalogue_entry(
    conn: &Connection,
    source_id: Id,
    name: &str,
    version: &str,
) -> rusqlite::Result<Option<CatalogueEntry>> {
    conn.query_row(
        &format!("{CATALOGUE_SELECT} WHERE c.source_id = ?1 AND c.name = ?2 AND c.version = ?3"),
        params![source_id.to_string(), name, version],
        catalogue_row,
    )
    .optional()
}

// ------------------------------------------------------------ images

fn image_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Image> {
    let id: String = r.get("id")?;
    let type_: String = r.get("type")?;
    let state: String = r.get("state")?;
    let source_id: Option<String> = r.get("source_id")?;
    let created: String = r.get("created_at")?;
    let imported: Option<String> = r.get("imported_at")?;
    Ok(Image {
        id: id.parse().unwrap_or_default(),
        name: r.get("name")?,
        version: r.get("version")?,
        type_: image_type(&type_),
        state: ImageState::parse(&state).unwrap_or(ImageState::Failed),
        sha256: r.get("sha256")?,
        size_bytes: u64::try_from(r.get::<_, i64>("size")?).unwrap_or(0),
        pool: r.get("pool")?,
        dataset: r.get("dataset")?,
        path: r.get("path")?,
        source_id: source_id.and_then(|s| s.parse().ok()),
        source_name: r.get("source_name")?,
        url: r.get("url")?,
        description: r.get("description")?,
        os: r.get("os")?,
        progress: r.get("progress")?,
        error: r.get("error")?,
        in_use_by: None,
        created_at: created.parse().unwrap_or_else(|_| Timestamp::now()),
        imported_at: imported.and_then(|s| s.parse().ok()),
        metadata: None,
    })
}

/// Every image, name then version.
pub fn list_images(
    conn: &Connection,
    type_: Option<ImageType>,
    state: Option<ImageState>,
) -> rusqlite::Result<Vec<Image>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM images WHERE (?1 IS NULL OR type = ?1) AND (?2 IS NULL OR state = ?2) \
         ORDER BY name, version, seq",
    )?;
    let rows = stmt.query_map(
        params![type_.map(ImageType::as_str), state.map(ImageState::as_str)],
        image_row,
    )?;
    rows.collect()
}

/// One image.
pub fn get_image(conn: &Connection, id: Id) -> rusqlite::Result<Option<Image>> {
    conn.query_row(
        "SELECT * FROM images WHERE id = ?1",
        [id.to_string()],
        image_row,
    )
    .optional()
}

/// An image with this payload hash that is not failed, if any.
pub fn image_by_sha256(conn: &Connection, sha256: &str) -> rusqlite::Result<Option<Image>> {
    conn.query_row(
        "SELECT * FROM images WHERE sha256 = ?1 AND state != 'failed' ORDER BY seq LIMIT 1",
        [sha256.to_ascii_lowercase()],
        image_row,
    )
    .optional()
}

/// What an image row is created from.
#[derive(Debug, Clone)]
pub struct NewImage {
    /// Id.
    pub id: Id,
    /// Name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Type.
    pub type_: ImageType,
    /// Hash.
    pub sha256: String,
    /// Size.
    pub size: u64,
    /// Pool.
    pub pool: String,
    /// Source.
    pub source: Option<(Id, String)>,
    /// URL.
    pub url: String,
    /// Description.
    pub description: Option<String>,
    /// OS.
    pub os: Option<String>,
}

/// Insert a pending image.
pub fn insert_image(conn: &Connection, image: &NewImage) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO images (id, name, version, type, state, sha256, size, pool, source_id, \
         source_name, url, description, os, progress, created_at) \
         VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 0.0, ?13)",
        params![
            image.id.to_string(),
            image.name,
            image.version,
            image.type_.as_str(),
            image.sha256.to_ascii_lowercase(),
            i64::try_from(image.size).unwrap_or(i64::MAX),
            image.pool,
            image.source.as_ref().map(|(id, _)| id.to_string()),
            image.source.as_ref().map(|(_, n)| n.clone()),
            image.url,
            image.description,
            image.os,
            Timestamp::now().to_rfc3339(),
        ],
    )?;
    Ok(())
}

/// Record the job driving an image's import.
pub fn set_image_job(conn: &Connection, id: Id, job: Id) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE images SET job_id = ?1 WHERE id = ?2",
        params![job.to_string(), id.to_string()],
    )?;
    Ok(())
}

/// Move an in-flight image along. Returns whether the row still exists.
pub fn set_image_progress(
    conn: &Connection,
    id: Id,
    state: ImageState,
    progress: f64,
) -> rusqlite::Result<bool> {
    let n = conn.execute(
        "UPDATE images SET state = ?1, progress = ?2 WHERE id = ?3 AND state != 'failed'",
        params![state.as_str(), progress, id.to_string()],
    )?;
    Ok(n > 0)
}

/// Mark an image ready. Returns whether the row still exists.
pub fn finish_image(
    conn: &Connection,
    id: Id,
    dataset: Option<&str>,
    path: Option<&str>,
) -> rusqlite::Result<bool> {
    let n = conn.execute(
        "UPDATE images SET state = 'ready', progress = 1.0, error = NULL, dataset = ?1, path = ?2, \
         imported_at = ?3 WHERE id = ?4 AND state != 'failed'",
        params![dataset, path, Timestamp::now().to_rfc3339(), id.to_string()],
    )?;
    Ok(n > 0)
}

/// Mark an image failed.
pub fn fail_image(conn: &Connection, id: Id, error: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE images SET state = 'failed', error = ?1 WHERE id = ?2",
        params![error, id.to_string()],
    )?;
    Ok(())
}

/// Remove an image row.
pub fn delete_image(conn: &Connection, id: Id) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM images WHERE id = ?1", [id.to_string()])
}

/// Images left mid-import by a previous daemon run become failed; their
/// staging files are removed by the store's next `prepare`.
pub fn recover(conn: &Connection) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE images SET state = 'failed', error = 'interrupted by a daemon restart' \
         WHERE state IN ('pending', 'downloading', 'verifying', 'importing')",
        [],
    )
}

// ------------------------------------------------------------ refresh

/// Fetch, verify, and cache a source's index. Returns the source as it
/// stands afterwards; failures are recorded in `last_error`, not returned.
pub async fn refresh_source(state: &AppState, source: &ImageSource) -> ApiResult<ImageSource> {
    let id = source.id;
    let outcome = fetch_index(state, source).await;
    match outcome {
        Ok((entries, verified)) => {
            state
                .db
                .call(move |conn| store_catalogue(conn, id, &entries, verified))
                .await?;
        }
        Err(e) => {
            let message = e.to_string();
            tracing::warn!(source = %source.name, error = %message, "image source refresh failed");
            state
                .db
                .call(move |conn| record_refresh_error(conn, id, &message))
                .await?;
        }
    }
    state
        .db
        .call(move |conn| get_source(conn, id))
        .await?
        .ok_or_else(|| ApiError::not_found("image source"))
}

type Fetched = (Vec<(index::IndexEntry, String)>, bool);

async fn fetch_index(
    state: &AppState,
    source: &ImageSource,
) -> Result<Fetched, mandrake_images::ImageError> {
    let transport = state.importer.transport();
    let bytes = transport.get(&source.url).await?;
    let verified = match &source.public_key {
        Some(key) => {
            let sig = transport.get(&index::signature_url(&source.url)).await?;
            let sig = String::from_utf8_lossy(&sig).trim().to_owned();
            index::verify(&bytes, &sig, key)?;
            true
        }
        None => false,
    };
    let parsed = index::parse(&bytes)?;
    let entries = parsed
        .images
        .into_iter()
        .map(|e| {
            let url = index::resolve_url(&source.url, &e.url);
            (e, url)
        })
        .collect();
    Ok((entries, verified))
}

// ------------------------------------------------------------ pool choice

/// The pool an image or zone goes to when none is named: the data pool
/// with the most free space, `rpool` only when it is the only one
/// (ADR-0012).
pub async fn default_pool(state: &AppState) -> ApiResult<String> {
    let zfs = state.zfs.clone();
    let pools = state
        .pools_cache
        .get_or(|| async move { zfs.list_pools().await })
        .await?;
    let best = pools
        .iter()
        .filter(|p| p.name != "rpool" && p.health != PoolHealth::Unavail)
        .max_by_key(|p| p.free)
        .or_else(|| pools.iter().find(|p| p.name == "rpool"))
        .ok_or_else(|| ApiError::unprocessable("no pool to import into"))?;
    Ok(best.name.clone())
}

/// Whether `pool` exists.
pub async fn pool_exists(state: &AppState, pool: &str) -> ApiResult<bool> {
    let zfs = state.zfs.clone();
    let pools = state
        .pools_cache
        .get_or(|| async move { zfs.list_pools().await })
        .await?;
    Ok(pools.iter().any(|p| p.name == pool))
}

// ------------------------------------------------------------ import job

/// Start the job that imports `plan` for the image row `plan.id`.
pub async fn start_import(
    state: &AppState,
    plan: ImportPlan,
    name: &str,
    actor: &mandrake_core::Actor,
) -> ApiResult<mandrake_core::api::Job> {
    let image_id = plan.id;
    let target = ObjectRef::new("image", image_id, name);
    let job_state = state.clone();
    let job = state
        .start_job(
            "image.import",
            Some(target),
            Some(actor),
            move |job| async move { run_import(job_state, job, plan).await },
        )
        .await?;
    let job_id = job.id;
    state
        .db
        .call(move |conn| set_image_job(conn, image_id, job_id))
        .await?;
    Ok(job)
}

async fn run_import(
    state: AppState,
    job: crate::jobs::JobContext,
    plan: ImportPlan,
) -> ApiResult<String> {
    let image_id = plan.id;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Progress>();
    let progress_state = state.clone();
    let progress_job = job.clone();
    let relay = tokio::spawn(async move {
        let mut last: Option<(ImageState, u32)> = None;
        while let Some(p) = rx.recv().await {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let percent = (p.fraction * 100.0).round() as u32;
            if last == Some((p.state, percent)) {
                continue;
            }
            last = Some((p.state, percent));
            let overall = match p.state {
                ImageState::Downloading => p.fraction * 0.8,
                ImageState::Verifying => 0.85,
                ImageState::Importing => 0.9 + p.fraction * 0.1,
                ImageState::Pending => 0.0,
                ImageState::Ready | ImageState::Failed => 1.0,
            };
            let _ = progress_state
                .db
                .call(move |conn| set_image_progress(conn, image_id, p.state, overall))
                .await;
            progress_job.progress(overall, format!("{}", p.state)).await;
        }
    });
    let importer = state.importer.clone();
    let result = importer
        .run(&plan, &move |p| {
            let _ = tx.send(p);
        })
        .await;
    let _ = relay.await;
    match result {
        Ok(outcome) => {
            let dataset = outcome.dataset.clone();
            let path = outcome.path.clone();
            let kept = state
                .db
                .call(move |conn| finish_image(conn, image_id, dataset.as_deref(), path.as_deref()))
                .await?;
            if !kept {
                // Deleted while importing: take the payload away again.
                let _ = importer.remove(plan.image_type, &plan.pool, image_id).await;
                return Err(ApiError::typed(
                    axum::http::StatusCode::CONFLICT,
                    "canceled",
                    "Conflict",
                )
                .detail("image deleted during import"));
            }
            state.snapshots_cache.clear();
            state.datasets_cache.clear();
            state
                .emit(
                    "image.ready",
                    ObjectRef::new("image", image_id, &plan.url),
                    None,
                    Some(json!({ "dataset": outcome.dataset, "path": outcome.path })),
                )
                .await;
            Ok("image ready".to_owned())
        }
        Err(e) => {
            let message = e.to_string();
            let db_message = message.clone();
            let _ = state
                .db
                .call(move |conn| fail_image(conn, image_id, &db_message))
                .await;
            state
                .emit(
                    "image.failed",
                    ObjectRef::new("image", image_id, &plan.url),
                    None,
                    Some(json!({ "error": message })),
                )
                .await;
            Err(ApiError::from(e))
        }
    }
}
