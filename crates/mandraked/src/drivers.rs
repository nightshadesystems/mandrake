//! Which driver implementations the daemon runs with, and how driver
//! failures become API problems.

use std::{net::IpAddr, sync::Arc, time::Duration};

use axum::http::StatusCode;
use mandrake_core::shell::{FailureKind, Runner, ScriptedRunner, SystemRunner};
use mandrake_images::{FakeStore, FakeTransport, HttpTransport, ImageError, Importer, ZfsStore};
use mandrake_net::{FakeNet, Net, NetCli, NetError};
use mandrake_zfs::{BeadmCli, BootEnvs, FakeBeadm, FakeZfs, Zfs, ZfsCli, ZfsError};
use mandrake_zones::{FakeZones, ZoneError, Zones, ZonesCli};

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
    /// Zone driver.
    pub zones: Arc<dyn Zones>,
    /// Boot environments.
    pub beadm: Arc<dyn BootEnvs>,
    /// Package updates.
    pub pkg: Arc<dyn crate::pkg::Pkg>,
    /// Image transport and store.
    pub importer: Importer,
    /// How often a scan job polls the pool.
    pub scan_poll: Duration,
    /// The address the HTTPS listener is bound to, when it is a specific
    /// one; part of what makes the management path protected.
    pub listen: Option<IpAddr>,
    /// Runs host commands the daemon issues itself (reboot).
    pub runner: Arc<dyn Runner>,
    /// How long a reboot job waits before `shutdown` (ADR-0015).
    pub reboot_grace: Duration,
}

impl Options {
    /// The real drivers, shelling out to illumos tooling.
    pub fn system() -> Result<Self, ApiError> {
        let runner = Arc::new(SystemRunner::new());
        let transport = HttpTransport::new()?;
        Ok(Self {
            login_limiter: LoginLimiter::default_login(),
            zfs: Arc::new(ZfsCli::new(runner.clone())),
            net: Arc::new(NetCli::new(runner.clone())),
            zones: Arc::new(ZonesCli::new(runner.clone())),
            beadm: Arc::new(BeadmCli::new(runner.clone())),
            pkg: Arc::new(crate::pkg::PkgCli::new(runner.clone())),
            importer: Importer::new(
                Arc::new(transport),
                Arc::new(ZfsStore::new(runner.clone(), crate::images::STORE_OWNER)),
            ),
            scan_poll: Duration::from_secs(2),
            listen: None,
            runner,
            reboot_grace: crate::routes::updates::REBOOT_GRACE,
        })
    }

    /// In-memory fakes seeded with a typical host, for development away
    /// from illumos and for tests.
    pub fn fake() -> Self {
        Self {
            login_limiter: LoginLimiter::default_login(),
            zfs: Arc::new(FakeZfs::typical()),
            net: Arc::new(FakeNet::typical()),
            zones: Arc::new(FakeZones::typical()),
            beadm: Arc::new(FakeBeadm::typical()),
            pkg: Arc::new(crate::pkg::FakePkg::new()),
            importer: Importer::new(Arc::new(FakeTransport::new()), Arc::new(FakeStore::new())),
            scan_poll: Duration::from_millis(20),
            listen: None,
            runner: {
                let r = ScriptedRunner::new();
                r.ok("shutdown", "");
                Arc::new(r)
            },
            reboot_grace: Duration::from_millis(20),
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

    /// Replace the zone driver.
    #[must_use]
    pub fn with_zones(mut self, zones: Arc<dyn Zones>) -> Self {
        self.zones = zones;
        self
    }

    /// Replace the boot-environment driver.
    #[must_use]
    pub fn with_beadm(mut self, beadm: Arc<dyn BootEnvs>) -> Self {
        self.beadm = beadm;
        self
    }

    /// Replace the package driver.
    #[must_use]
    pub fn with_pkg(mut self, pkg: Arc<dyn crate::pkg::Pkg>) -> Self {
        self.pkg = pkg;
        self
    }

    /// Replace the host command runner.
    #[must_use]
    pub fn with_runner(mut self, runner: Arc<dyn Runner>) -> Self {
        self.runner = runner;
        self
    }

    /// Replace the image importer.
    #[must_use]
    pub fn with_importer(mut self, importer: Importer) -> Self {
        self.importer = importer;
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

impl From<ImageError> for ApiError {
    fn from(e: ImageError) -> Self {
        let detail = match &e {
            ImageError::Command(c) => c.stderr().to_owned(),
            other => other.to_string(),
        };
        driver_error(e.kind(), detail, "image", &e)
    }
}

impl From<crate::pkg::PkgError> for ApiError {
    fn from(e: crate::pkg::PkgError) -> Self {
        let detail = match &e {
            crate::pkg::PkgError::Command(c) => c.stderr().to_owned(),
            crate::pkg::PkgError::Parse(_) => e.to_string(),
        };
        driver_error(FailureKind::Other, detail, "pkg", &e)
    }
}

impl From<ZoneError> for ApiError {
    fn from(e: ZoneError) -> Self {
        let detail = match &e {
            ZoneError::Command(c) => c.stderr().to_owned(),
            other => other.to_string(),
        };
        driver_error(e.kind(), detail, "zone", &e)
    }
}
