//! `mandrake-image-index`: build and sign an image source index so anyone
//! can host a source (ADR-0012).
//!
//! ```text
//! mandrake-image-index keygen --out source.key        # prints the public key
//! mandrake-image-index build manifest.json --key source.key [--out DIR]
//! mandrake-image-index sign index.json --key source.key
//! mandrake-image-index verify index.json --public-key BASE64
//! ```
//!
//! The manifest lists the files to publish:
//!
//! ```json
//! { "name": "my-images",
//!   "images": [ { "name": "debian-12", "version": "20260901", "type": "zone-lx",
//!                 "file": "debian-12-20260901.zfs.gz", "os": "debian-12",
//!                 "description": "Debian 12 lx root" } ] }
//! ```
//!
//! `build` hashes each file, writes `index.json` next to the manifest (or
//! under `--out`), and signs it into `index.json.sig`. Entry URLs are the
//! file names, so the files go beside the index on the web server.

#![allow(clippy::missing_errors_doc)]

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Parser, Subcommand};
use mandrake_core::{Timestamp, image::ImageType};
use mandrake_images::{Index, IndexEntry, hex, index};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Build and sign Mandrake image source indexes.
#[derive(Debug, Parser)]
#[command(name = "mandrake-image-index", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate a signing key; the public key is printed.
    Keygen {
        /// Where to write the secret key (base64, mode 0600).
        #[arg(long, default_value = "source.key")]
        out: PathBuf,
    },
    /// Hash the files in a manifest, write index.json and index.json.sig.
    Build {
        /// The manifest.
        manifest: PathBuf,
        /// Secret key file from keygen.
        #[arg(long)]
        key: PathBuf,
        /// Output directory; default: the manifest's.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Sign an existing index.json into index.json.sig.
    Sign {
        /// The index.
        index: PathBuf,
        /// Secret key file from keygen.
        #[arg(long)]
        key: PathBuf,
    },
    /// Check index.json against index.json.sig and a public key.
    Verify {
        /// The index.
        index: PathBuf,
        /// Base64 public key.
        #[arg(long)]
        public_key: String,
    },
}

#[derive(Debug, Deserialize)]
struct Manifest {
    name: String,
    images: Vec<ManifestEntry>,
}

#[derive(Debug, Deserialize)]
struct ManifestEntry {
    name: String,
    version: String,
    #[serde(rename = "type")]
    type_: ImageType,
    file: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    os: Option<String>,
}

fn read_key(path: &Path) -> Result<String, String> {
    fs::read_to_string(path)
        .map(|s| s.trim().to_owned())
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn write_secret(path: &Path, secret: &str) -> Result<(), String> {
    fs::write(path, format!("{secret}\n")).map_err(|e| format!("{}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<(String, u64), String> {
    let mut file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut size = 0u64;
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    Ok((hex(&hasher.finalize()), size))
}

fn build(manifest_path: &Path, key: &Path, out: Option<PathBuf>) -> Result<(), String> {
    let text = fs::read_to_string(manifest_path)
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let manifest: Manifest =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let dir = manifest_path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let out = out.unwrap_or_else(|| dir.clone());
    let mut images = Vec::new();
    for e in manifest.images {
        let path = dir.join(&e.file);
        let (sha256, size) = hash_file(&path)?;
        eprintln!("{}@{}: {} bytes, {sha256}", e.name, e.version, size);
        images.push(IndexEntry {
            name: e.name,
            version: e.version,
            type_: e.type_,
            url: e.file,
            sha256,
            size,
            description: e.description,
            os: e.os,
            published_at: Some(Timestamp::now()),
        });
    }
    let index = Index {
        name: manifest.name,
        generated_at: Some(Timestamp::now()),
        images,
    };
    let bytes = serde_json::to_vec_pretty(&index).map_err(|e| e.to_string())?;
    let index_path = out.join("index.json");
    fs::write(&index_path, &bytes).map_err(|e| format!("{}: {e}", index_path.display()))?;
    sign(&index_path, key)
}

fn sign(index_path: &Path, key: &Path) -> Result<(), String> {
    let bytes = fs::read(index_path).map_err(|e| format!("{}: {e}", index_path.display()))?;
    index::parse(&bytes).map_err(|e| e.to_string())?;
    let secret = read_key(key)?;
    let sig = index::sign(&bytes, &secret).map_err(|e| e.to_string())?;
    let sig_path = PathBuf::from(format!("{}.sig", index_path.display()));
    fs::write(&sig_path, format!("{sig}\n")).map_err(|e| format!("{}: {e}", sig_path.display()))?;
    let public = index::public_key_of(&secret).map_err(|e| e.to_string())?;
    println!("wrote {} and {}", index_path.display(), sig_path.display());
    println!("public key: {public}");
    Ok(())
}

fn verify(index_path: &Path, public_key: &str) -> Result<(), String> {
    let bytes = fs::read(index_path).map_err(|e| format!("{}: {e}", index_path.display()))?;
    let sig_path = PathBuf::from(format!("{}.sig", index_path.display()));
    let sig = fs::read_to_string(&sig_path).map_err(|e| format!("{}: {e}", sig_path.display()))?;
    let parsed = index::parse(&bytes).map_err(|e| e.to_string())?;
    index::verify(&bytes, sig.trim(), public_key).map_err(|e| e.to_string())?;
    println!("ok: {} with {} images", parsed.name, parsed.images.len());
    Ok(())
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Keygen { out } => {
            let (secret, public) = index::keypair();
            write_secret(&out, &secret)?;
            println!("secret key written to {}", out.display());
            println!("public key: {public}");
            Ok(())
        }
        Command::Build { manifest, key, out } => build(&manifest, &key, out),
        Command::Sign { index, key } => sign(&index, &key),
        Command::Verify { index, public_key } => verify(&index, &public_key),
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mandrake-image-index: {e}");
            ExitCode::FAILURE
        }
    }
}
