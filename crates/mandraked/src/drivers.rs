//! Which driver implementations the daemon runs with, and how driver
//! failures become API problems.

use std::{sync::Arc, time::Duration};

use axum::http::StatusCode;
use mandrake_core::shell::SystemRunner;
use mandrake_zfs::{FailureKind, FakeZfs, Zfs, ZfsCli, ZfsError};

use crate::{auth::ratelimit::LoginLimiter, error::ApiError};

/// How long list reads are cached (ADR-0011).
pub const LIST_TTL: Duration = Duration::from_secs(2);

/// Everything `AppState` needs besides the database.
pub struct Options {
    /// Login rate limiter.
    pub login_limiter: LoginLimiter,
    /// Storage driver.
    pub zfs: Arc<dyn Zfs>,
    /// How often a scan job polls the pool.
    pub scan_poll: Duration,
}

impl Options {
    /// The real drivers, shelling out to illumos tooling.
    pub fn system() -> Self {
        Self {
            login_limiter: LoginLimiter::default_login(),
            zfs: Arc::new(ZfsCli::new(Arc::new(SystemRunner::new()))),
            scan_poll: Duration::from_secs(2),
        }
    }

    /// In-memory fakes seeded with a typical host, for development away
    /// from illumos and for tests.
    pub fn fake() -> Self {
        Self {
            login_limiter: LoginLimiter::default_login(),
            zfs: Arc::new(FakeZfs::typical()),
            scan_poll: Duration::from_millis(20),
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
}

impl From<ZfsError> for ApiError {
    fn from(e: ZfsError) -> Self {
        let detail = match &e {
            ZfsError::Command(c) => c.stderr().to_owned(),
            other => other.to_string(),
        };
        match e.kind() {
            FailureKind::NotFound => {
                ApiError::new(StatusCode::NOT_FOUND, "Not Found").detail(detail)
            }
            FailureKind::Exists => ApiError::conflict(&detail),
            FailureKind::Conflict => {
                ApiError::typed(StatusCode::CONFLICT, "busy", "Conflict").detail(detail)
            }
            FailureKind::Forbidden => ApiError::forbidden(&detail),
            FailureKind::Invalid => ApiError::unprocessable(&detail),
            FailureKind::Other => {
                tracing::error!(error = %e, "storage command failed");
                ApiError::typed(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "command-failed",
                    "Internal Server Error",
                )
                .detail(detail)
            }
        }
    }
}
