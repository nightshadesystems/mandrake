//! Application state and router assembly.

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use axum::{
    Router,
    extract::{ConnectInfo, Request},
    http::{HeaderValue, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post, put},
};
use mandrake_core::Id;
use mandrake_images::Importer;
use mandrake_net::{AddressInfo, LinkInfo, Net, RouteInfo};
use mandrake_zfs::{DatasetInfo, PoolInfo, SnapshotInfo, Zfs};
use mandrake_zones::Zones;
use rusqlite::OptionalExtension;
use tower::ServiceBuilder;
use tower_http::{
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

use crate::{
    auth::{Source, ratelimit::LoginLimiter},
    cache::TtlCache,
    console,
    db::Db,
    drivers::{LIST_TTL, Options},
    error::ApiError,
    events::EventBus,
    idempotency, routes,
};

/// Largest request body accepted.
const BODY_LIMIT: usize = 1 << 20;

/// Shared state behind every handler. Cheap to clone.
#[derive(Clone)]
pub struct AppState(Arc<Inner>);

/// The state proper.
pub struct Inner {
    /// Metadata store.
    pub db: Db,
    /// Event bus.
    pub events: EventBus,
    /// Login attempt limiter.
    pub login_limiter: LoginLimiter,
    /// Host id, generated once and kept in the database.
    pub host_id: Id,
    /// Daemon version.
    pub version: &'static str,
    /// Storage driver.
    pub zfs: Arc<dyn Zfs>,
    /// How often scan jobs poll the pool.
    pub scan_poll: Duration,
    /// Cached `zpool` listing.
    pub pools_cache: TtlCache<Vec<PoolInfo>>,
    /// Cached `zfs list` of filesystems and volumes.
    pub datasets_cache: TtlCache<Vec<DatasetInfo>>,
    /// Cached `zfs list -t snapshot` of everything.
    pub snapshots_cache: TtlCache<Vec<SnapshotInfo>>,
    /// Network driver.
    pub net: Arc<dyn Net>,
    /// The specific address the HTTPS listener is bound to, if any.
    pub listen: Option<IpAddr>,
    /// Cached datalinks.
    pub links_cache: TtlCache<Vec<LinkInfo>>,
    /// Cached address objects.
    pub addresses_cache: TtlCache<Vec<AddressInfo>>,
    /// Cached routing table.
    pub routes_cache: TtlCache<Vec<RouteInfo>>,
    /// Image transport and store.
    pub importer: Importer,
    /// Zone driver.
    pub zones: Arc<dyn Zones>,
    /// Cached zone listing with configurations.
    pub zones_cache: TtlCache<Vec<crate::zones::ZoneInfo>>,
    /// Which zones have a console attached.
    pub console_sessions: crate::zone_console::ConsoleSessions,
}

impl std::ops::Deref for AppState {
    type Target = Inner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AppState {
    /// Build state over an opened database, creating the host id if needed.
    pub async fn new(db: Db) -> Result<Self, ApiError> {
        Self::with_options(db, Options::system()?).await
    }

    /// Fake drivers and a custom login limiter (tests).
    pub async fn with_limiter(db: Db, login_limiter: LoginLimiter) -> Result<Self, ApiError> {
        Self::with_options(db, Options::fake().with_limiter(login_limiter)).await
    }

    /// Build state over an opened database with the given drivers.
    pub async fn with_options(db: Db, options: Options) -> Result<Self, ApiError> {
        let Options {
            login_limiter,
            zfs,
            net,
            zones,
            importer,
            scan_poll,
            listen,
        } = options;
        let host_id = db
            .call(|conn| {
                let existing: Option<String> = conn
                    .query_row("SELECT value FROM host WHERE key = 'id'", [], |r| r.get(0))
                    .optional()?;
                if let Some(id) = existing.and_then(|s| s.parse::<Id>().ok()) {
                    return Ok(id);
                }
                let id = Id::new();
                conn.execute(
                    "INSERT INTO host (key, value) VALUES ('id', ?1)",
                    [id.to_string()],
                )?;
                Ok(id)
            })
            .await?;
        Ok(Self(Arc::new(Inner {
            db,
            events: EventBus::new(),
            login_limiter,
            host_id,
            version: env!("CARGO_PKG_VERSION"),
            zfs,
            scan_poll,
            pools_cache: TtlCache::new(LIST_TTL),
            datasets_cache: TtlCache::new(LIST_TTL),
            snapshots_cache: TtlCache::new(LIST_TTL),
            net,
            listen,
            links_cache: TtlCache::new(LIST_TTL),
            addresses_cache: TtlCache::new(LIST_TTL),
            routes_cache: TtlCache::new(LIST_TTL),
            importer,
            zones,
            zones_cache: TtlCache::new(LIST_TTL),
            console_sessions: crate::zone_console::ConsoleSessions::default(),
        })))
    }
}

#[derive(Clone, Copy)]
struct MakeUuidRequestId;

impl MakeRequestId for MakeUuidRequestId {
    fn make_request_id<B>(&mut self, _: &axum::http::Request<B>) -> Option<RequestId> {
        HeaderValue::from_str(&Id::new().to_string())
            .ok()
            .map(RequestId::new)
    }
}

async fn source_layer(mut req: Request, next: Next) -> Response {
    if req.extensions().get::<Source>().is_none() {
        let source = req
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map_or_else(|| "unknown".to_owned(), |c| c.0.ip().to_string());
        req.extensions_mut().insert(Source(source));
    }
    next.run(req).await
}

/// The API router, mounted at `/api/v1`.
pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(routes::health::get))
        .route("/auth/login", post(routes::auth::login))
        .route("/auth/logout", post(routes::auth::logout))
        .route("/auth/session", get(routes::auth::session))
        .route("/system", get(routes::system::info))
        .route("/system/resources", get(routes::system::resources))
        .route(
            "/users",
            get(routes::users::list).post(routes::users::create),
        )
        .route(
            "/users/{id}",
            get(routes::users::get_one)
                .patch(routes::users::update)
                .delete(routes::users::delete),
        )
        .route("/users/{id}/password", put(routes::users::set_password))
        .route(
            "/tokens",
            get(routes::tokens::list).post(routes::tokens::create),
        )
        .route(
            "/tokens/{id}",
            get(routes::tokens::get_one).delete(routes::tokens::delete),
        )
        .route("/audit", get(routes::audit::list))
        .route("/jobs", get(routes::jobs::list))
        .route("/jobs/{id}", get(routes::jobs::get_one))
        .route("/events", get(routes::events::stream))
        .merge(routes::storage::router())
        .merge(routes::network::router())
        .merge(routes::images::router())
        .merge(routes::zones::router())
        .merge(routes::vms::router())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            idempotency::layer,
        ))
        .with_state(state)
}

/// The full router: API plus embedded console, with the common layers.
pub fn router(state: AppState) -> Router {
    Router::new()
        .nest("/api/v1", api_router(state))
        .fallback(console::serve)
        .layer(
            ServiceBuilder::new()
                .layer(SetRequestIdLayer::x_request_id(MakeUuidRequestId))
                .layer(PropagateRequestIdLayer::x_request_id())
                .layer(TraceLayer::new_for_http())
                .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(30)))
                // Before the body limit: from_fn wants the plain axum body.
                .layer(middleware::from_fn(source_layer))
                .layer(RequestBodyLimitLayer::new(BODY_LIMIT)),
        )
}
