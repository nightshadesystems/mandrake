//! Where payloads go: ZFS datasets, zvols, and files under `<pool>/images`
//! (ADR-0012). Decompression is a pipeline of illumos tools, no shell.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use mandrake_core::shell::{Command, Runner};

use crate::{
    BoxFuture,
    types::{Compression, ImageError, Result},
};

/// The image store.
pub trait Store: Send + Sync {
    /// Make sure `<pool>/images`, `images/iso`, and `images/staging` exist
    /// and the daemon can write the last two.
    fn prepare<'a>(&'a self, pool: &'a str) -> BoxFuture<'a, Result<()>>;
    /// Where a payload is downloaded before import.
    fn staging_path(&self, pool: &str, id: &str) -> PathBuf {
        PathBuf::from(format!("/{pool}/images/staging/{id}.part"))
    }
    /// Receive a ZFS stream file into `dataset`.
    fn receive<'a>(
        &'a self,
        file: &'a Path,
        compression: Compression,
        dataset: &'a str,
    ) -> BoxFuture<'a, Result<()>>;
    /// Create `zvol` of `size` bytes and write the raw image file into it.
    fn write_volume<'a>(
        &'a self,
        file: &'a Path,
        compression: Compression,
        zvol: &'a str,
        size: u64,
    ) -> BoxFuture<'a, Result<()>>;
    /// Move a file into its final place.
    fn keep_file<'a>(&'a self, file: &'a Path, dest: &'a Path) -> BoxFuture<'a, Result<()>>;
    /// `zfs snapshot dataset@image`.
    fn snapshot<'a>(&'a self, dataset: &'a str) -> BoxFuture<'a, Result<()>>;
    /// `zfs destroy -r dataset`, for cleanup and delete.
    fn destroy<'a>(&'a self, dataset: &'a str) -> BoxFuture<'a, Result<()>>;
    /// Remove a file; missing is fine.
    fn remove_file<'a>(&'a self, path: &'a Path) -> BoxFuture<'a, Result<()>>;
}

/// The real store over a [`Runner`].
#[derive(Clone)]
pub struct ZfsStore {
    runner: Arc<dyn Runner>,
    owner: String,
}

impl ZfsStore {
    /// A store whose staging and ISO directories belong to `owner`, the
    /// user the daemon runs as.
    pub fn new(runner: Arc<dyn Runner>, owner: impl Into<String>) -> Self {
        Self {
            runner,
            owner: owner.into(),
        }
    }

    async fn run(&self, cmd: Command) -> Result<()> {
        self.runner.run(&cmd).await?;
        Ok(())
    }

    async fn pipeline(&self, cmds: &[Command]) -> Result<()> {
        self.runner.pipeline(cmds).await?;
        Ok(())
    }

    /// `[gzip -dc file] | sink`, or `sink < file` when plain.
    async fn feed(&self, file: &Path, compression: Compression, sink: Command) -> Result<()> {
        let path = file.to_string_lossy().into_owned();
        match compression.decompressor() {
            Some(dec) => self.pipeline(&[dec.arg(path), sink]).await,
            None => self.pipeline(&[Command::new("cat").arg(path), sink]).await,
        }
    }
}

impl Store for ZfsStore {
    fn prepare<'a>(&'a self, pool: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            for sub in ["images", "images/iso", "images/staging"] {
                self.run(
                    Command::new("zfs")
                        .args(["create", "-p", &format!("{pool}/{sub}")])
                        .privileged(),
                )
                .await?;
            }
            self.run(
                Command::new("chown")
                    .arg(&self.owner)
                    .arg(format!("/{pool}/images/iso"))
                    .arg(format!("/{pool}/images/staging"))
                    .privileged(),
            )
            .await
        })
    }

    fn receive<'a>(
        &'a self,
        file: &'a Path,
        compression: Compression,
        dataset: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let sink = Command::new("zfs")
                .args(["receive", "-u", dataset])
                .privileged();
            self.feed(file, compression, sink).await
        })
    }

    fn write_volume<'a>(
        &'a self,
        file: &'a Path,
        compression: Compression,
        zvol: &'a str,
        size: u64,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.run(
                Command::new("zfs")
                    .args(["create", "-V", &size.to_string(), zvol])
                    .privileged(),
            )
            .await?;
            let sink = Command::new("dd")
                .arg(format!("of=/dev/zvol/rdsk/{zvol}"))
                .arg("bs=1M")
                .privileged();
            self.feed(file, compression, sink).await
        })
    }

    fn keep_file<'a>(&'a self, file: &'a Path, dest: &'a Path) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            tokio::fs::rename(file, dest).await.map_err(|e| {
                ImageError::Io(format!("{} -> {}: {e}", file.display(), dest.display()))
            })
        })
    }

    fn snapshot<'a>(&'a self, dataset: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.run(
                Command::new("zfs")
                    .args(["snapshot", &format!("{dataset}@{}", crate::IMAGE_SNAPSHOT)])
                    .privileged(),
            )
            .await
        })
    }

    fn destroy<'a>(&'a self, dataset: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.run(
                Command::new("zfs")
                    .args(["destroy", "-r", dataset])
                    .privileged(),
            )
            .await
        })
    }

    fn remove_file<'a>(&'a self, path: &'a Path) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            match tokio::fs::remove_file(path).await {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(ImageError::Io(format!("{}: {e}", path.display()))),
            }
        })
    }
}

/// Remembers what it was asked to do; datasets are names in a list and
/// files live in a temporary directory.
#[derive(Clone)]
pub struct FakeStore {
    root: Arc<PathBuf>,
    datasets: Arc<Mutex<Vec<String>>>,
    snapshots: Arc<Mutex<Vec<String>>>,
    kept: Arc<Mutex<Vec<PathBuf>>>,
}

impl FakeStore {
    /// A store over a fresh directory under the system temp directory.
    pub fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("mandrake-fake-store-{}", mandrake_core::Id::new()));
        let _ = std::fs::create_dir_all(&root);
        Self {
            root: Arc::new(root),
            datasets: Arc::new(Mutex::new(Vec::new())),
            snapshots: Arc::new(Mutex::new(Vec::new())),
            kept: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Datasets and zvols created, in order.
    pub fn datasets(&self) -> Vec<String> {
        self.datasets.lock().map(|d| d.clone()).unwrap_or_default()
    }

    /// Snapshots taken.
    pub fn snapshots(&self) -> Vec<String> {
        self.snapshots.lock().map(|d| d.clone()).unwrap_or_default()
    }

    /// Files kept.
    pub fn kept(&self) -> Vec<PathBuf> {
        self.kept.lock().map(|d| d.clone()).unwrap_or_default()
    }

    /// The temporary root, for locating files in tests.
    pub fn root(&self) -> &Path {
        self.root.as_path()
    }

    fn under_root(&self, pool: &str, rest: &str) -> PathBuf {
        self.root.as_path().join(pool).join(rest)
    }
}

impl Default for FakeStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Store for FakeStore {
    fn prepare<'a>(&'a self, pool: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            for sub in ["images/iso", "images/staging"] {
                tokio::fs::create_dir_all(self.under_root(pool, sub))
                    .await
                    .map_err(|e| ImageError::Io(e.to_string()))?;
            }
            Ok(())
        })
    }

    fn staging_path(&self, pool: &str, id: &str) -> PathBuf {
        self.under_root(pool, &format!("images/staging/{id}.part"))
    }

    fn receive<'a>(
        &'a self,
        file: &'a Path,
        _compression: Compression,
        dataset: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            if !file.exists() {
                return Err(ImageError::Io(format!("{}: missing", file.display())));
            }
            if let Ok(mut d) = self.datasets.lock() {
                d.push(dataset.to_owned());
            }
            Ok(())
        })
    }

    fn write_volume<'a>(
        &'a self,
        file: &'a Path,
        _compression: Compression,
        zvol: &'a str,
        _size: u64,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            if !file.exists() {
                return Err(ImageError::Io(format!("{}: missing", file.display())));
            }
            if let Ok(mut d) = self.datasets.lock() {
                d.push(zvol.to_owned());
            }
            Ok(())
        })
    }

    fn keep_file<'a>(&'a self, file: &'a Path, dest: &'a Path) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            // Keep ISO files under the temporary root regardless of the
            // absolute path the plan names.
            let target = self
                .root
                .as_path()
                .join(dest.to_string_lossy().trim_start_matches(['/', '\\']));
            if let Some(parent) = target.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| ImageError::Io(e.to_string()))?;
            }
            tokio::fs::rename(file, &target)
                .await
                .map_err(|e| ImageError::Io(e.to_string()))?;
            if let Ok(mut k) = self.kept.lock() {
                k.push(target);
            }
            Ok(())
        })
    }

    fn snapshot<'a>(&'a self, dataset: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            if let Ok(mut s) = self.snapshots.lock() {
                s.push(format!("{dataset}@{}", crate::IMAGE_SNAPSHOT));
            }
            Ok(())
        })
    }

    fn destroy<'a>(&'a self, dataset: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            if let Ok(mut d) = self.datasets.lock() {
                d.retain(|x| x != dataset);
            }
            Ok(())
        })
    }

    fn remove_file<'a>(&'a self, path: &'a Path) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            match tokio::fs::remove_file(path).await {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(ImageError::Io(e.to_string())),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use mandrake_core::shell::ScriptedRunner;

    use super::*;

    #[tokio::test]
    async fn real_store_commands() {
        let r = Arc::new(ScriptedRunner::new());
        r.ok("zfs", "").ok("chown", "").ok("gzip", "").ok("cat", "");
        let s = ZfsStore::new(Arc::clone(&r) as Arc<dyn Runner>, "mandrake");
        s.prepare("tank").await.unwrap();
        s.receive(
            Path::new("/tank/images/staging/x.part"),
            Compression::Gzip,
            "tank/images/x",
        )
        .await
        .unwrap();
        s.write_volume(
            Path::new("/tank/images/staging/y.part"),
            Compression::None,
            "tank/images/y",
            1024,
        )
        .await
        .unwrap();
        s.snapshot("tank/images/x").await.unwrap();
        s.destroy("tank/images/x").await.unwrap();
        assert_eq!(
            r.lines(),
            vec![
                "pfexec zfs create -p tank/images",
                "pfexec zfs create -p tank/images/iso",
                "pfexec zfs create -p tank/images/staging",
                "pfexec chown mandrake /tank/images/iso /tank/images/staging",
                "pfexec zfs create -V 1024 tank/images/y",
                "pfexec zfs snapshot tank/images/x@image",
                "pfexec zfs destroy -r tank/images/x",
            ]
        );
        assert_eq!(
            r.pipeline_lines(),
            vec![
                "gzip -dc /tank/images/staging/x.part | pfexec zfs receive -u tank/images/x",
                "cat /tank/images/staging/y.part | pfexec dd of=/dev/zvol/rdsk/tank/images/y bs=1M",
            ]
        );
    }
}
