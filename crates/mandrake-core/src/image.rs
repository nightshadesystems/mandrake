//! Image wire types mirroring the `images` family in `api/openapi.yaml`.

// `Option<Option<T>>` is the contract's three-state PATCH field.
#![allow(clippy::option_option)]

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Id, Timestamp, api::Metadata};

/// What an image is used for; fixes its on-disk form (ADR-0012).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImageType {
    /// Native zone root as a ZFS stream.
    ZoneNative,
    /// lx zone root as a ZFS stream.
    ZoneLx,
    /// Raw VM disk image, written to a zvol.
    VmRaw,
    /// Installer ISO, kept as a file.
    VmIso,
}

impl ImageType {
    /// As the API spells it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ZoneNative => "zone-native",
            Self::ZoneLx => "zone-lx",
            Self::VmRaw => "vm-raw",
            Self::VmIso => "vm-iso",
        }
    }

    /// Whether the image is a dataset or zvol that zones and VMs clone.
    pub const fn is_cloneable(self) -> bool {
        !matches!(self, Self::VmIso)
    }
}

impl fmt::Display for ImageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where an import is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageState {
    /// Queued.
    Pending,
    /// Fetching the payload.
    Downloading,
    /// Checking the hash.
    Verifying,
    /// Receiving into ZFS or writing the zvol.
    Importing,
    /// Usable.
    Ready,
    /// Import failed; `error` says why.
    Failed,
}

impl ImageState {
    /// As the API spells it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Downloading => "downloading",
            Self::Verifying => "verifying",
            Self::Importing => "importing",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }

    /// From the stored form.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "downloading" => Some(Self::Downloading),
            "verifying" => Some(Self::Verifying),
            "importing" => Some(Self::Importing),
            "ready" => Some(Self::Ready),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

impl fmt::Display for ImageState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An image on this host, in any state of import.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Image {
    /// Id.
    pub id: Id,
    /// Name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Type.
    #[serde(rename = "type")]
    pub type_: ImageType,
    /// State.
    pub state: ImageState,
    /// Published hash.
    pub sha256: String,
    /// Published size.
    pub size_bytes: u64,
    /// Pool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    /// Dataset or zvol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset: Option<String>,
    /// File path (ISO).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<Id>,
    /// Source name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,
    /// Payload URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// OS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    /// Progress while not ready.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    /// Failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Clones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_use_by: Option<u32>,
    /// Created.
    pub created_at: Timestamp,
    /// Imported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_at: Option<Timestamp>,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// `POST /images/import`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageImport {
    /// Source to pick from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<Id>,
    /// Name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Direct URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Direct hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Direct type.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<ImageType>,
    /// Pool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    /// Metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// One image a source offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogueEntry {
    /// Source.
    pub source_id: Id,
    /// Source name.
    pub source_name: String,
    /// Name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Type.
    #[serde(rename = "type")]
    pub type_: ImageType,
    /// Payload URL, resolved.
    pub url: String,
    /// Hash.
    pub sha256: String,
    /// Size.
    pub size_bytes: u64,
    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// OS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    /// Published.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<Timestamp>,
    /// Already on the host.
    pub imported: bool,
    /// The imported image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<Id>,
}

/// An image source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageSource {
    /// Id.
    pub id: Id,
    /// Name.
    pub name: String,
    /// Index URL.
    pub url: String,
    /// Base64 Ed25519 public key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    /// Enabled.
    pub enabled: bool,
    /// Shipped with Mandrake.
    pub builtin: bool,
    /// A key is set and the last index verified.
    pub verified: bool,
    /// Cached entries.
    pub image_count: u32,
    /// Last refresh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refreshed_at: Option<Timestamp>,
    /// Last failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Created.
    pub created_at: Timestamp,
}

/// `POST /images/sources`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageSourceCreate {
    /// Name.
    pub name: String,
    /// Index URL.
    pub url: String,
    /// Public key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    /// Enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// `PATCH /images/sources/{id}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageSourceUpdate {
    /// Name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Public key; explicit null makes the source unverified.
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub public_key: Option<Option<String>>,
    /// Enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

fn double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}
