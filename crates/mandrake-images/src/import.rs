//! One import from start to finish: download, verify, land in ZFS or on
//! disk, snapshot. The daemon runs this inside a job and turns
//! [`Progress`] into events.

use std::sync::Arc;

use mandrake_core::{
    Id,
    image::{ImageState, ImageType},
};

use crate::{
    IMAGE_SNAPSHOT, Store, Transport, dataset_for, iso_path_for,
    types::{Compression, ImageError, Result},
};

/// What to import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPlan {
    /// The image id, which names the dataset or file.
    pub id: Id,
    /// Type.
    pub image_type: ImageType,
    /// Payload URL.
    pub url: String,
    /// Expected hex sha256.
    pub sha256: String,
    /// Expected size, for progress and zvol sizing.
    pub size: u64,
    /// Pool.
    pub pool: String,
}

/// Where an import is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    /// Phase.
    pub state: ImageState,
    /// 0 to 1 within the phase.
    pub fraction: f64,
}

/// What an import produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Dataset or zvol, with an `@image` snapshot.
    pub dataset: Option<String>,
    /// File path (ISO).
    pub path: Option<String>,
}

/// Progress callback.
pub type ProgressFn<'a> = &'a (dyn Fn(Progress) + Send + Sync);

/// Runs imports.
#[derive(Clone)]
pub struct Importer {
    transport: Arc<dyn Transport>,
    store: Arc<dyn Store>,
}

impl Importer {
    /// An importer over a transport and a store.
    pub fn new(transport: Arc<dyn Transport>, store: Arc<dyn Store>) -> Self {
        Self { transport, store }
    }

    /// The store, for deletes and cleanup outside an import.
    pub fn store(&self) -> &Arc<dyn Store> {
        &self.store
    }

    /// The transport, for index fetches.
    pub fn transport(&self) -> &Arc<dyn Transport> {
        &self.transport
    }

    /// Run `plan`. On failure nothing of the image is left behind except
    /// the error.
    pub async fn run(&self, plan: &ImportPlan, progress: ProgressFn<'_>) -> Result<Outcome> {
        self.store.prepare(&plan.pool).await?;
        let staging = self.store.staging_path(&plan.pool, &plan.id.to_string());
        let result = self.download_and_land(plan, &staging, progress).await;
        let _ = self.store.remove_file(&staging).await;
        result
    }

    async fn download_and_land(
        &self,
        plan: &ImportPlan,
        staging: &std::path::Path,
        progress: ProgressFn<'_>,
    ) -> Result<Outcome> {
        let size = plan.size.max(1);
        progress(Progress {
            state: ImageState::Downloading,
            fraction: 0.0,
        });
        #[allow(clippy::cast_precision_loss)] // progress only
        let report = |bytes: u64| {
            progress(Progress {
                state: ImageState::Downloading,
                fraction: (bytes as f64 / size as f64).min(1.0),
            });
        };
        let downloaded = self.transport.download(&plan.url, staging, &report).await?;
        progress(Progress {
            state: ImageState::Verifying,
            fraction: 1.0,
        });
        if !downloaded.sha256.eq_ignore_ascii_case(&plan.sha256) {
            return Err(ImageError::Verify {
                expected: plan.sha256.to_ascii_lowercase(),
                actual: downloaded.sha256,
            });
        }
        progress(Progress {
            state: ImageState::Importing,
            fraction: 0.0,
        });
        let compression = Compression::from_url(&plan.url);
        let outcome = self.land(plan, staging, compression).await;
        if outcome.is_err() && plan.image_type.is_cloneable() {
            let _ = self.store.destroy(&dataset_for(&plan.pool, plan.id)).await;
        }
        outcome
    }

    async fn land(
        &self,
        plan: &ImportPlan,
        staging: &std::path::Path,
        compression: Compression,
    ) -> Result<Outcome> {
        match plan.image_type {
            ImageType::ZoneNative | ImageType::ZoneLx => {
                let dataset = dataset_for(&plan.pool, plan.id);
                self.store.receive(staging, compression, &dataset).await?;
                self.store.snapshot(&dataset).await?;
                Ok(Outcome {
                    dataset: Some(dataset),
                    path: None,
                })
            }
            ImageType::VmRaw => {
                let zvol = dataset_for(&plan.pool, plan.id);
                self.store
                    .write_volume(staging, compression, &zvol, raw_size(plan))
                    .await?;
                self.store.snapshot(&zvol).await?;
                Ok(Outcome {
                    dataset: Some(zvol),
                    path: None,
                })
            }
            ImageType::VmIso => {
                let dest = iso_path_for(&plan.pool, plan.id);
                self.store
                    .keep_file(staging, std::path::Path::new(&dest))
                    .await?;
                Ok(Outcome {
                    dataset: None,
                    path: Some(dest),
                })
            }
        }
    }

    /// Remove what an image occupies.
    pub async fn remove(&self, image_type: ImageType, pool: &str, id: Id) -> Result<()> {
        if image_type.is_cloneable() {
            self.store.destroy(&dataset_for(pool, id)).await
        } else {
            self.store
                .remove_file(std::path::Path::new(&iso_path_for(pool, id)))
                .await
        }
    }
}

/// The zvol size for a raw image: the published size rounded up to a
/// mebibyte, since a compressed payload's size is the compressed one and
/// a raw image is at least that large.
fn raw_size(plan: &ImportPlan) -> u64 {
    const MIB: u64 = 1 << 20;
    plan.size.div_ceil(MIB).max(1) * MIB
}

/// The snapshot name every clone starts from.
pub fn clone_source(dataset: &str) -> String {
    format!("{dataset}@{IMAGE_SNAPSHOT}")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::sync::Mutex;

    use sha2::{Digest, Sha256};

    use super::*;
    use crate::{FakeStore, FakeTransport, hex};

    fn plan(image_type: ImageType, url: &str, body: &[u8]) -> ImportPlan {
        ImportPlan {
            id: Id::new(),
            image_type,
            url: url.to_owned(),
            sha256: hex(&Sha256::digest(body)),
            size: body.len() as u64,
            pool: "tank".to_owned(),
        }
    }

    #[tokio::test]
    async fn zone_image_is_received_and_snapshotted() {
        let transport = FakeTransport::new();
        let body = b"not really a zfs stream";
        transport.add("https://x/img.zfs.gz", body.to_vec());
        let store = FakeStore::new();
        let importer = Importer::new(Arc::new(transport), Arc::new(store.clone()));
        let p = plan(ImageType::ZoneLx, "https://x/img.zfs.gz", body);
        let seen = Mutex::new(Vec::new());
        let outcome = importer
            .run(&p, &|pr| {
                if let Ok(mut s) = seen.lock() {
                    s.push(pr.state);
                }
            })
            .await
            .unwrap();
        let dataset = format!("tank/images/{}", p.id);
        assert_eq!(outcome.dataset.as_deref(), Some(dataset.as_str()));
        assert_eq!(store.datasets(), vec![dataset.clone()]);
        assert_eq!(store.snapshots(), vec![format!("{dataset}@image")]);
        let states = seen.lock().unwrap().clone();
        assert_eq!(states.first(), Some(&ImageState::Downloading));
        assert!(states.contains(&ImageState::Verifying));
        assert_eq!(states.last(), Some(&ImageState::Importing));
        assert!(!store.staging_path("tank", &p.id.to_string()).exists());
    }

    #[tokio::test]
    async fn iso_is_kept_and_raw_gets_a_zvol() {
        let transport = FakeTransport::new();
        transport
            .add("https://x/a.iso", b"iso".to_vec())
            .add("https://x/b.raw.xz", b"raw".to_vec());
        let store = FakeStore::new();
        let importer = Importer::new(Arc::new(transport), Arc::new(store.clone()));
        let iso = plan(ImageType::VmIso, "https://x/a.iso", b"iso");
        let out = importer.run(&iso, &|_| {}).await.unwrap();
        assert_eq!(
            out.path.as_deref(),
            Some(iso_path_for("tank", iso.id).as_str())
        );
        assert_eq!(store.kept().len(), 1);
        assert!(store.kept()[0].exists());
        let raw = plan(ImageType::VmRaw, "https://x/b.raw.xz", b"raw");
        let out = importer.run(&raw, &|_| {}).await.unwrap();
        assert_eq!(
            out.dataset.as_deref(),
            Some(dataset_for("tank", raw.id).as_str())
        );
        assert_eq!(raw_size(&raw), 1 << 20);
        importer
            .remove(ImageType::VmRaw, "tank", raw.id)
            .await
            .unwrap();
        assert!(!store.datasets().contains(&dataset_for("tank", raw.id)));
    }

    #[tokio::test]
    async fn hash_mismatch_fails_and_cleans_up() {
        let transport = FakeTransport::new();
        transport.add("https://x/img.zfs.gz", b"payload".to_vec());
        let store = FakeStore::new();
        let importer = Importer::new(Arc::new(transport), Arc::new(store.clone()));
        let mut p = plan(ImageType::ZoneLx, "https://x/img.zfs.gz", b"payload");
        p.sha256 = "0".repeat(64);
        let err = importer.run(&p, &|_| {}).await.err().unwrap();
        assert!(matches!(err, ImageError::Verify { .. }));
        assert!(store.datasets().is_empty());
        assert!(!store.staging_path("tank", &p.id.to_string()).exists());
        let missing = plan(ImageType::ZoneLx, "https://x/nope", b"");
        let err = importer.run(&missing, &|_| {}).await.err().unwrap();
        assert!(matches!(err, ImageError::Transport(_)));
    }

    #[test]
    fn compression_from_url() {
        assert_eq!(
            Compression::from_url("https://x/a.zfs.gz?x=1"),
            Compression::Gzip
        );
        assert_eq!(Compression::from_url("https://x/a.raw.xz"), Compression::Xz);
        assert_eq!(Compression::from_url("https://x/a.iso"), Compression::None);
        assert_eq!(clone_source("tank/images/x"), "tank/images/x@image");
    }
}
