//! Runtime: listeners, background tasks, and shutdown.

use std::{net::SocketAddr, time::Duration};

use axum_server::{Handle, tls_rustls::RustlsConfig};

use crate::{
    app::{self, AppState},
    auth::session,
    config::Config,
    db::{Db, DbError},
    error::ApiError,
    host, tls,
};

/// Fatal startup errors.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// Database.
    #[error("database: {0}")]
    Db(#[from] DbError),
    /// TLS material.
    #[error("tls: {0}")]
    Tls(#[from] tls::TlsError),
    /// Listener or socket.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// State initialisation.
    #[error("startup: {0}")]
    State(#[from] ApiError),
}

/// Run the daemon until a shutdown signal.
pub async fn run(cfg: Config) -> Result<(), RunError> {
    // The ring provider, not aws-lc-rs (ADR-0009 discussion). Installing
    // twice is harmless.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let db = Db::open(&cfg.db)?;
    tracing::info!(path = %cfg.db.display(), "database open");
    let options = if cfg.fake_drivers {
        tracing::warn!("running with fake drivers; storage and network are simulated");
        crate::drivers::Options::fake().with_listen(cfg.listen.ip())
    } else {
        crate::drivers::Options::system().with_listen(cfg.listen.ip())
    };
    let state = AppState::with_options(db, options).await?;
    match state.db.call(|conn| crate::jobs::recover(conn)).await {
        Ok(n) if n > 0 => tracing::warn!(
            jobs = n,
            "marked jobs interrupted by the previous daemon as failed"
        ),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "job recovery failed"),
    }

    let hostname = match cfg.hostname.clone() {
        Some(h) => h,
        None => host::facts().await.hostname,
    };
    let material = tls::load_or_generate(&cfg.tls_dir, &hostname)?;
    if material.generated {
        tracing::warn!(dir = %cfg.tls_dir.display(), "generated a self-signed TLS certificate");
    }
    let rustls =
        RustlsConfig::from_pem(material.cert_pem.clone(), material.key_pem.clone()).await?;

    let router = app::router(state.clone());

    let sweeper_state = state.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(3600));
        loop {
            tick.tick().await;
            match sweeper_state.db.call(|conn| session::sweep(conn)).await {
                Ok(n) if n > 0 => {
                    tracing::debug!(rows = n, "swept expired sessions and idempotency records");
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "sweep failed"),
            }
        }
    });

    #[cfg(unix)]
    if !cfg.no_socket {
        let socket_router = router.clone();
        let path = cfg.socket.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::socket::serve(&path, socket_router).await {
                tracing::error!(error = %e, path = %path.display(), "unix socket listener failed");
            }
        });
    }
    #[cfg(not(unix))]
    if !cfg.no_socket {
        tracing::warn!("unix socket is not available on this platform; skipping");
    }

    let handle = Handle::new();
    tokio::spawn(shutdown_signal(handle.clone()));

    let port = cfg.listen.port();
    tracing::info!(listen = %cfg.listen, "serving HTTPS");
    println!("Mandrake console: https://{hostname}:{port}/");
    println!(
        "TLS certificate SHA-256 fingerprint: {}",
        material.fingerprint
    );

    axum_server::bind_rustls(cfg.listen, rustls)
        .handle(handle)
        .serve(router.into_make_service_with_connect_info::<SocketAddr>())
        .await?;
    tracing::info!("stopped");
    Ok(())
}

async fn shutdown_signal(handle: Handle<SocketAddr>) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut term = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "cannot listen for SIGTERM");
                let _ = tokio::signal::ctrl_c().await;
                handle.graceful_shutdown(Some(Duration::from_secs(10)));
                return;
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
    tracing::info!("shutdown requested");
    handle.graceful_shutdown(Some(Duration::from_secs(10)));
}
