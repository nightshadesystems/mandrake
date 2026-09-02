//! The root Unix socket (ADR-0007, spec §9).
//!
//! The same router is served over a Unix socket. Each connection's peer
//! uid is attached to every request as [`SocketPeer`]; the `Auth`
//! extractor turns uid 0 into the root actor and rejects anyone else.

use std::path::Path;

use axum::Router;
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder,
    service::TowerToHyperService,
};
use tokio::net::UnixListener;
use tower::ServiceBuilder;

use crate::auth::{SocketPeer, Source};

/// Bind `path` (replacing any stale socket file) and serve `app` forever.
pub async fn serve(path: &Path, app: Router) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    tracing::info!(path = %path.display(), "listening on unix socket");

    loop {
        let (stream, _) = listener.accept().await?;
        let uid = stream.peer_cred().ok().map(|c| c.uid());
        let app = app.clone();
        tokio::spawn(async move {
            let svc = ServiceBuilder::new()
                .map_request(move |mut req: hyper::Request<hyper::body::Incoming>| {
                    req.extensions_mut().insert(SocketPeer(uid));
                    req.extensions_mut().insert(Source("socket".to_owned()));
                    req
                })
                .service(app);
            let result = Builder::new(TokioExecutor::new())
                .serve_connection_with_upgrades(TokioIo::new(stream), TowerToHyperService::new(svc))
                .await;
            if let Err(e) = result {
                tracing::debug!(error = %e, "unix socket connection ended");
            }
        });
    }
}
