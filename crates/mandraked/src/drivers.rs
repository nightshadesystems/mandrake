//! Which driver implementations the daemon runs with, and how driver
//! failures become API problems.

use std::{net::IpAddr, sync::Arc, time::Duration};

use axum::http::StatusCode;
use mandrake_core::shell::{FailureKind, SystemRunner};
use mandrake_net::{FakeNet, Net, NetCli, NetError};
use mandrake_zfs::{FakeZfs, Zfs, ZfsCli, ZfsError};

use crate::{auth::ratelimit::LoginLimiter, error::ApiError};

/// How long list reads are cached (ADR-0011).
pub const LIST_TTL: Duration = Duration::from_secs(2);

/// Everything `AppState` needs besides the database.
pub struct Options {
    /// Login rate limiter.
    pub login_limiter: LoginLimiter,
    /// Storage driver.
    pub zfs: Arc<dyn Zfs>,
    /// Network driver.
    pub net: Arc<dyn Net>,
    /// How often a scan job polls the pool.
    pub scan_poll: Duration,
    /// The address the HTTPS listener is bound to, when it is a specific
    /// one; part of what makes the management path protected.
    pub listen: Option<IpAddr>,
}

impl Options {
    /// The real drivers, shelling out to illumos tooling.
    pub fn system() -> Self {
        let runner = Arc::new(SystemRunner::new());
        Self {
            login_limiter: LoginLimiter::default_login(),
            zfs: Arc::new(ZfsCli::new(runner.clone())),
            net: Arc::new(NetCli::new(runner)),
            scan_poll: Duration::from_secs(2),
            listen: None,
        }
    }

    /// In-memory fakes seeded with a typical host, for development away
    /// from illumos and for tests.
    pub fn fake() -> Self {
        Self {
            login_limiter: LoginLimiter::default_login(),
            zfs: Arc::new(FakeZfs::typical()),
            net: Arc::new(FakeNet::typical()),
            scan_poll: Duration::from_millis(20),
            listen: None,
        }
    }

    /// Replace the limiter.
    #[must_use]
    pub fn with_limiter(mut self, limiter: LoginLimiter) -> Self {
        self.login_limiter = limiter;
        self
    }

    /// Replace the storage driver.
    #[must_use]
    pub fn with_zfs(mut self, zfs: Arc<dyn Zfs>) -> Self {
        self.zfs = zfs;
        self
    }

    /// Replace the network driver.
    #[must_use]
    pub fn with_net(mut self, net: Arc<dyn Net>) -> Self {
        self.net = net;
        self
    }

    /// Record the listener address; wildcard addresses count as none.
    #[must_use]
    pub fn with_listen(mut self, listen: IpAddr) -> Self {
        self.listen = (!listen.is_unspecified()).then_some(listen);
        self
    }
}

/// A driver failure as an API problem.
fn driver_error(
    kind: FailureKind,
    detail: String,
    what: &str,
    error: &dyn std::fmt::Display,
) -> ApiError {
    match kind {
        FailureKind::NotFound => ApiError::new(StatusCode::NOT_FOUND, "Not Found").detail(detail),
        FailureKind::Exists => ApiError::conflict(&detail),
        FailureKind::Conflict => {
            ApiError::typed(StatusCode::CONFLICT, "busy", "Conflict").detail(detail)
        }
        FailureKind::Forbidden => ApiError::forbidden(&detail),
        FailureKind::Invalid => ApiError::unprocessable(&detail),
        FailureKind::Other => {
            tracing::error!(error = %error, "{what} command failed");
            ApiError::typed(
                StatusCode::INTERNAL_SERVER_ERROR,
                "command-failed",
                "Internal Server Error",
            )
            .detail(detail)
        }
    }
}

impl From<ZfsError> for ApiError {
    fn from(e: ZfsError) -> Self {
        let detail = match &e {
            ZfsError::Command(c) => c.stderr().to_owned(),
            other => other.to_string(),
        };
        driver_error(e.kind(), detail, "storage", &e)
    }
}

impl From<NetError> for ApiError {
    fn from(e: NetError) -> Self {
        let detail = match &e {
            NetError::Command(c) => c.stderr().to_owned(),
            other => other.to_string(),
        };
        driver_error(e.kind(), detail, "network", &e)
    }
}
