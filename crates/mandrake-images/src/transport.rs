//! Fetching: small documents whole, payloads streamed to a file and hashed
//! on the way.

use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
};

use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::{
    BoxFuture, MAX_DOCUMENT_BYTES,
    types::{ImageError, Result, hex},
};

/// Progress callback: bytes received so far.
pub type ProgressFn<'a> = &'a (dyn Fn(u64) + Send + Sync);

/// What a download produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Downloaded {
    /// Bytes written.
    pub bytes: u64,
    /// Hex sha256 of them.
    pub sha256: String,
}

/// Network access. Implemented over HTTP and by a fake serving canned bytes.
pub trait Transport: Send + Sync {
    /// Fetch a small document whole (at most [`MAX_DOCUMENT_BYTES`]).
    fn get<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<Vec<u8>>>;
    /// Stream `url` into `dest`, hashing as it goes.
    fn download<'a>(
        &'a self,
        url: &'a str,
        dest: &'a Path,
        progress: ProgressFn<'a>,
    ) -> BoxFuture<'a, Result<Downloaded>>;
}

/// Over `reqwest`. The daemon installs the rustls provider before use.
#[derive(Clone)]
pub struct HttpTransport {
    client: reqwest::Client,
}

impl HttpTransport {
    /// A transport with a 30 s connect timeout and no total timeout, since
    /// payloads can be large.
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .user_agent(concat!("mandrake/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| ImageError::Transport(e.to_string()))?;
        Ok(Self { client })
    }

    async fn response(&self, url: &str) -> Result<reqwest::Response> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| ImageError::Transport(format!("{url}: {e}")))?;
        if !response.status().is_success() {
            return Err(ImageError::Transport(format!(
                "{url}: HTTP {}",
                response.status()
            )));
        }
        Ok(response)
    }
}

impl Transport for HttpTransport {
    fn get<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<Vec<u8>>> {
        Box::pin(async move {
            let mut response = self.response(url).await?;
            let mut body = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|e| ImageError::Transport(format!("{url}: {e}")))?
            {
                if body.len() + chunk.len() > MAX_DOCUMENT_BYTES {
                    return Err(ImageError::Transport(format!(
                        "{url}: document larger than {MAX_DOCUMENT_BYTES} bytes"
                    )));
                }
                body.extend_from_slice(&chunk);
            }
            Ok(body)
        })
    }

    fn download<'a>(
        &'a self,
        url: &'a str,
        dest: &'a Path,
        progress: ProgressFn<'a>,
    ) -> BoxFuture<'a, Result<Downloaded>> {
        Box::pin(async move {
            let mut response = self.response(url).await?;
            let mut file = tokio::fs::File::create(dest)
                .await
                .map_err(|e| ImageError::Io(format!("{}: {e}", dest.display())))?;
            let mut hasher = Sha256::new();
            let mut bytes: u64 = 0;
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|e| ImageError::Transport(format!("{url}: {e}")))?
            {
                hasher.update(&chunk);
                file.write_all(&chunk)
                    .await
                    .map_err(|e| ImageError::Io(format!("{}: {e}", dest.display())))?;
                bytes += chunk.len() as u64;
                progress(bytes);
            }
            file.flush()
                .await
                .map_err(|e| ImageError::Io(format!("{}: {e}", dest.display())))?;
            Ok(Downloaded {
                bytes,
                sha256: hex(&hasher.finalize()),
            })
        })
    }
}

/// Serves canned bytes by URL; anything else is a transport error.
#[derive(Clone, Default)]
pub struct FakeTransport {
    docs: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl FakeTransport {
    /// Nothing served.
    pub fn new() -> Self {
        Self::default()
    }

    /// Serve `bytes` at `url`.
    pub fn add(&self, url: &str, bytes: impl Into<Vec<u8>>) -> &Self {
        if let Ok(mut d) = self.docs.lock() {
            d.insert(url.to_owned(), bytes.into());
        }
        self
    }

    fn lookup(&self, url: &str) -> Result<Vec<u8>> {
        self.docs
            .lock()
            .ok()
            .and_then(|d| d.get(url).cloned())
            .ok_or_else(|| ImageError::Transport(format!("{url}: HTTP 404")))
    }
}

impl Transport for FakeTransport {
    fn get<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<Vec<u8>>> {
        Box::pin(async move { self.lookup(url) })
    }

    fn download<'a>(
        &'a self,
        url: &'a str,
        dest: &'a Path,
        progress: ProgressFn<'a>,
    ) -> BoxFuture<'a, Result<Downloaded>> {
        Box::pin(async move {
            let body = self.lookup(url)?;
            tokio::fs::write(dest, &body)
                .await
                .map_err(|e| ImageError::Io(format!("{}: {e}", dest.display())))?;
            let bytes = body.len() as u64;
            progress(bytes);
            Ok(Downloaded {
                bytes,
                sha256: hex(&Sha256::digest(&body)),
            })
        })
    }
}
