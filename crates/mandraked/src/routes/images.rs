//! `/images/*`: the catalogue, sources, and imports (ADR-0012).

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use mandrake_core::{
    Id, Role,
    api::{Job, Metadata, ObjectRef, Page},
    image::{
        CatalogueEntry, Image, ImageImport, ImageSource, ImageSourceCreate, ImageSourceUpdate,
        ImageState, ImageType,
    },
};
use mandrake_images::{ImportPlan, index, types::valid_sha256};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::Ctx;
use crate::{
    app::AppState,
    audit::Record,
    auth::Auth,
    cursor::{self, Pagination},
    error::{ApiError, ApiResult},
    images::{self, NewImage, SourcePatch},
    metadata,
};

/// `{ "items": [...] }`.
#[derive(Debug, Serialize)]
pub struct Items<T> {
    /// The items.
    pub items: Vec<T>,
}

/// The image routes, mounted under `/api/v1`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/images", get(list_images))
        .route("/images/import", post(import_image))
        .route("/images/available", get(list_available))
        .route("/images/sources", get(list_sources).post(create_source))
        .route(
            "/images/sources/{id}",
            get(get_source).patch(update_source).delete(delete_source),
        )
        .route("/images/sources/{id}/refresh", post(refresh_source))
        .route(
            "/images/{id}",
            get(get_image).patch(update_image).delete(delete_image),
        )
}

fn protected_error(what: &str) -> ApiError {
    ApiError::typed(StatusCode::FORBIDDEN, "protected", "Forbidden")
        .detail(format!("{what} is built in and cannot be changed that way"))
}

fn valid_url(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && url.len() > 8
        && !url.contains(' ')
}

// ------------------------------------------------------------ images

/// Query for `GET /images`.
#[derive(Debug, Default, Deserialize)]
pub struct ImageFilter {
    /// Type.
    #[serde(rename = "type")]
    pub type_: Option<ImageType>,
    /// State.
    pub state: Option<ImageState>,
    /// Paging.
    #[serde(flatten)]
    pub paging: Pagination,
}

async fn metadata_for(state: &AppState, ids: &[Id]) -> ApiResult<HashMap<Id, Metadata>> {
    let ids = ids.to_vec();
    state
        .db
        .call(move |conn| metadata::get_many(conn, &ids))
        .await
}

/// Clone counts per image dataset, from the `@image` snapshots.
async fn clone_counts(state: &AppState, images: &[Image]) -> HashMap<String, u32> {
    if !images.iter().any(|i| i.dataset.is_some()) {
        return HashMap::new();
    }
    let zfs = state.zfs.clone();
    let snapshots = state
        .snapshots_cache
        .get_or(|| async move { zfs.list_snapshots(None, false).await })
        .await
        .unwrap_or_default();
    let mut counts = HashMap::new();
    for i in images {
        if let Some(ds) = &i.dataset {
            let snap = mandrake_images::import::clone_source(ds);
            if let Some(s) = snapshots.iter().find(|s| s.name == snap) {
                counts.insert(
                    ds.clone(),
                    u32::try_from(s.clones.len()).unwrap_or(u32::MAX),
                );
            }
        }
    }
    counts
}

async fn decorate(state: &AppState, mut images: Vec<Image>) -> ApiResult<Vec<Image>> {
    let ids: Vec<Id> = images.iter().map(|i| i.id).collect();
    let mut meta = metadata_for(state, &ids).await?;
    let clones = clone_counts(state, &images).await;
    for i in &mut images {
        i.metadata = meta.remove(&i.id);
        i.in_use_by = i.dataset.as_ref().and_then(|d| clones.get(d).copied());
    }
    Ok(images)
}

async fn find_image(state: &AppState, id: Id) -> ApiResult<Image> {
    let image = state
        .db
        .call(move |conn| images::get_image(conn, id))
        .await?
        .ok_or_else(|| ApiError::not_found("image"))?;
    decorate(state, vec![image])
        .await?
        .pop()
        .ok_or_else(|| ApiError::not_found("image"))
}

/// `GET /images`.
pub async fn list_images(
    State(state): State<AppState>,
    auth: Auth,
    Query(filter): Query<ImageFilter>,
) -> ApiResult<Json<Page<Image>>> {
    auth.require(Role::Viewer)?;
    let (type_, image_state) = (filter.type_, filter.state);
    let rows = state
        .db
        .call(move |conn| images::list_images(conn, type_, image_state))
        .await?;
    let items = decorate(&state, rows).await?;
    let key = |i: &Image| format!("{}\u{1}{}\u{1}{}", i.name, i.version, i.id);
    let after = filter.paging.after().unwrap_or_default();
    let limit = filter.paging.limit();
    let rows: Vec<Image> = items
        .into_iter()
        .filter(|i| key(i) > after)
        .take(usize::try_from(limit).unwrap_or(usize::MAX) + 1)
        .collect();
    let (items, next_cursor) = cursor::page(rows, limit, key);
    Ok(Json(Page { items, next_cursor }))
}

/// `GET /images/{id}`.
pub async fn get_image(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Id>,
) -> ApiResult<Json<Image>> {
    auth.require(Role::Viewer)?;
    Ok(Json(find_image(&state, id).await?))
}

/// `PATCH /images/{id}`.
pub async fn update_image(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    Json(patch): Json<Metadata>,
) -> ApiResult<Json<Image>> {
    auth.require(Role::Operator)?;
    let image = find_image(&state, id).await?;
    let m = patch.clone();
    state
        .db
        .call(move |conn| metadata::merge(conn, id, &m))
        .await?;
    let updated = find_image(&state, id).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("image.update", ObjectRef::new("image", id, &image.name))
                .after(serde_json::to_value(&updated.metadata).unwrap_or_default()),
        )
        .await?;
    Ok(Json(updated))
}

/// `DELETE /images/{id}`.
pub async fn delete_image(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
) -> ApiResult<StatusCode> {
    auth.require(Role::Operator)?;
    let image = find_image(&state, id).await?;
    if image.in_use_by.is_some_and(|n| n > 0) {
        return Err(
            ApiError::typed(StatusCode::CONFLICT, "busy", "Conflict").detail(format!(
                "{} zone(s) or VM(s) are cloned from this image",
                image.in_use_by.unwrap_or(0)
            )),
        );
    }
    if image.state == ImageState::Ready
        && let Some(pool) = &image.pool
    {
        state.importer.remove(image.type_, pool, id).await?;
        state.snapshots_cache.clear();
        state.datasets_cache.clear();
    }
    // In-flight imports notice the missing row when they finish.
    state
        .db
        .call(move |conn| images::delete_image(conn, id))
        .await?;
    let _ = state.db.call(move |conn| metadata::remove(conn, id)).await;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("image.delete", ObjectRef::new("image", id, &image.name)).before(json!({
                "name": image.name,
                "version": image.version,
                "type": image.type_,
                "state": image.state,
            })),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------ import

/// What an import request resolved to.
struct Resolved {
    new: NewImage,
}

async fn resolve_import(state: &AppState, body: &ImageImport) -> ApiResult<Resolved> {
    let id = Id::new();
    let pool = match &body.pool {
        Some(p) => {
            if !images::pool_exists(state, p).await? {
                return Err(ApiError::not_found(&format!("pool `{p}`")));
            }
            p.clone()
        }
        None => images::default_pool(state).await?,
    };
    if let Some(source_id) = body.source_id {
        let source = state
            .db
            .call(move |conn| images::get_source(conn, source_id))
            .await?
            .ok_or_else(|| ApiError::not_found("image source"))?;
        if !source.verified {
            return Err(ApiError::typed(
                StatusCode::UNPROCESSABLE_ENTITY,
                "unverified-source",
                "Unprocessable Entity",
            )
            .detail(format!(
                "source `{}` has no verified index; set its public key and refresh, or import by URL with a sha256",
                source.name
            )));
        }
        let (name, version) = (body.name.clone(), body.version.clone());
        let entry: CatalogueEntry = state
            .db
            .call(move |conn| images::catalogue_entry(conn, source_id, &name, &version))
            .await?
            .ok_or_else(|| {
                ApiError::not_found(&format!(
                    "image `{}@{}` in source `{}`",
                    body.name, body.version, source.name
                ))
            })?;
        return Ok(Resolved {
            new: NewImage {
                id,
                name: entry.name,
                version: entry.version,
                type_: entry.type_,
                sha256: entry.sha256,
                size: entry.size_bytes,
                pool,
                source: Some((source.id, source.name)),
                url: entry.url,
                description: entry.description,
                os: entry.os,
            },
        });
    }
    let (Some(url), Some(sha256), Some(type_)) = (&body.url, &body.sha256, body.type_) else {
        return Err(ApiError::unprocessable(
            "a direct import needs url, sha256, and type; or give source_id",
        ));
    };
    if !valid_url(url) {
        return Err(ApiError::unprocessable("url must be http:// or https://"));
    }
    if !valid_sha256(sha256) {
        return Err(ApiError::unprocessable("sha256 must be 64 hex characters"));
    }
    Ok(Resolved {
        new: NewImage {
            id,
            name: body.name.clone(),
            version: body.version.clone(),
            type_,
            sha256: sha256.to_ascii_lowercase(),
            size: 0,
            pool,
            source: None,
            url: url.clone(),
            description: None,
            os: None,
        },
    })
}

/// `POST /images/import`.
pub async fn import_image(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Json(body): Json<ImageImport>,
) -> ApiResult<(StatusCode, Json<Job>)> {
    auth.require(Role::Operator)?;
    if body.name.trim().is_empty() || body.version.trim().is_empty() {
        return Err(ApiError::unprocessable("name and version are required"));
    }
    let Resolved { new } = resolve_import(&state, &body).await?;
    let sha = new.sha256.clone();
    if let Some(existing) = state
        .db
        .call(move |conn| images::image_by_sha256(conn, &sha))
        .await?
    {
        return Err(ApiError::conflict(&format!(
            "image {}@{} ({}) already has this payload",
            existing.name, existing.version, existing.id
        )));
    }
    let row = new.clone();
    state
        .db
        .call(move |conn| images::insert_image(conn, &row))
        .await?;
    if let Some(m) = &body.metadata
        && !m.is_empty()
    {
        let m = m.clone();
        let id = new.id;
        state
            .db
            .call(move |conn| metadata::merge(conn, id, &m))
            .await?;
    }
    let plan = ImportPlan {
        id: new.id,
        image_type: new.type_,
        url: new.url.clone(),
        sha256: new.sha256.clone(),
        size: new.size,
        pool: new.pool.clone(),
    };
    let job = images::start_import(&state, plan, &new.name, &auth.actor).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("image.import", ObjectRef::new("image", new.id, &new.name)).after(json!({
                "version": new.version,
                "type": new.type_,
                "url": new.url,
                "pool": new.pool,
                "source": new.source.as_ref().map(|(_, n)| n.clone()),
                "job": job.id,
            })),
        )
        .await?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

/// Query for `GET /images/available`.
#[derive(Debug, Default, Deserialize)]
pub struct AvailableFilter {
    /// Source.
    pub source_id: Option<Id>,
    /// Type.
    #[serde(rename = "type")]
    pub type_: Option<ImageType>,
}

/// `GET /images/available`.
pub async fn list_available(
    State(state): State<AppState>,
    auth: Auth,
    Query(filter): Query<AvailableFilter>,
) -> ApiResult<Json<Items<CatalogueEntry>>> {
    auth.require(Role::Viewer)?;
    let items = state
        .db
        .call(move |conn| images::catalogue(conn, filter.source_id, filter.type_))
        .await?;
    Ok(Json(Items { items }))
}

// ------------------------------------------------------------ sources

async fn find_source(state: &AppState, id: Id) -> ApiResult<ImageSource> {
    state
        .db
        .call(move |conn| images::get_source(conn, id))
        .await?
        .ok_or_else(|| ApiError::not_found("image source"))
}

/// `GET /images/sources`.
pub async fn list_sources(
    State(state): State<AppState>,
    auth: Auth,
) -> ApiResult<Json<Items<ImageSource>>> {
    auth.require(Role::Viewer)?;
    let items = state.db.call(|conn| images::list_sources(conn)).await?;
    Ok(Json(Items { items }))
}

/// `GET /images/sources/{id}`.
pub async fn get_source(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Id>,
) -> ApiResult<Json<ImageSource>> {
    auth.require(Role::Viewer)?;
    Ok(Json(find_source(&state, id).await?))
}

fn check_key(key: Option<&str>) -> ApiResult<()> {
    if key.is_some_and(|k| !index::valid_public_key(k)) {
        return Err(ApiError::unprocessable(
            "public_key must be a base64 Ed25519 public key (32 bytes)",
        ));
    }
    Ok(())
}

/// `POST /images/sources`.
pub async fn create_source(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Json(body): Json<ImageSourceCreate>,
) -> ApiResult<(StatusCode, Json<ImageSource>)> {
    auth.require(Role::Operator)?;
    let name = body.name.trim().to_owned();
    if name.is_empty() || name.len() > 64 {
        return Err(ApiError::unprocessable("name must be 1 to 64 characters"));
    }
    if !valid_url(&body.url) {
        return Err(ApiError::unprocessable("url must be http:// or https://"));
    }
    check_key(body.public_key.as_deref())?;
    let id = Id::new();
    let (row_name, url, key, enabled) = (
        name.clone(),
        body.url.clone(),
        body.public_key.clone(),
        body.enabled,
    );
    state
        .db
        .call(move |conn| images::insert_source(conn, id, &row_name, &url, key.as_deref(), enabled))
        .await
        .map_err(|e| {
            if e.0.status == 500 {
                ApiError::conflict("a source with this name exists")
            } else {
                e
            }
        })?;
    let source = find_source(&state, id).await?;
    let source = images::refresh_source(&state, &source).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok(
                "image-source.create",
                ObjectRef::new("image-source", id, &source.name),
            )
            .after(json!({ "url": source.url, "verified": source.verified, "images": source.image_count })),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(source)))
}

/// `PATCH /images/sources/{id}`.
pub async fn update_source(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
    Json(body): Json<ImageSourceUpdate>,
) -> ApiResult<Json<ImageSource>> {
    auth.require(Role::Operator)?;
    let source = find_source(&state, id).await?;
    if source.builtin && (body.name.is_some() || body.url.is_some()) {
        return Err(protected_error(&format!("source `{}`", source.name)));
    }
    if let Some(u) = &body.url
        && !valid_url(u)
    {
        return Err(ApiError::unprocessable("url must be http:// or https://"));
    }
    if let Some(n) = &body.name
        && (n.trim().is_empty() || n.len() > 64)
    {
        return Err(ApiError::unprocessable("name must be 1 to 64 characters"));
    }
    check_key(body.public_key.as_ref().and_then(|k| k.as_deref()))?;
    let patch = SourcePatch {
        name: body.name.clone().map(|n| n.trim().to_owned()),
        url: body.url.clone(),
        public_key: body.public_key.clone(),
        enabled: body.enabled,
    };
    let refetch = patch.url.is_some() || patch.public_key.is_some();
    state
        .db
        .call(move |conn| images::update_source(conn, id, &patch))
        .await?;
    let mut updated = find_source(&state, id).await?;
    if refetch && updated.enabled {
        updated = images::refresh_source(&state, &updated).await?;
    }
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok(
                "image-source.update",
                ObjectRef::new("image-source", id, &updated.name),
            )
            .after(json!({ "url": updated.url, "enabled": updated.enabled, "verified": updated.verified })),
        )
        .await?;
    Ok(Json(updated))
}

/// `DELETE /images/sources/{id}`.
pub async fn delete_source(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
) -> ApiResult<StatusCode> {
    auth.require(Role::Operator)?;
    let source = find_source(&state, id).await?;
    if source.builtin {
        return Err(protected_error(&format!("source `{}`", source.name)));
    }
    state
        .db
        .call(move |conn| images::delete_source(conn, id))
        .await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok(
                "image-source.delete",
                ObjectRef::new("image-source", id, &source.name),
            )
            .before(json!({ "url": source.url })),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /images/sources/{id}/refresh`.
pub async fn refresh_source(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
) -> ApiResult<Json<ImageSource>> {
    auth.require(Role::Operator)?;
    let source = find_source(&state, id).await?;
    let refreshed = images::refresh_source(&state, &source).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok(
                "image-source.refresh",
                ObjectRef::new("image-source", id, &refreshed.name),
            )
            .after(json!({ "verified": refreshed.verified, "images": refreshed.image_count, "error": refreshed.last_error })),
        )
        .await?;
    Ok(Json(refreshed))
}
