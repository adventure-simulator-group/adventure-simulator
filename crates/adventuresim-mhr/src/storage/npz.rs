use std::path::Path;

use anyhow::{Context, Result};

use crate::storage::npy_array::{self, NpyArray};
use crate::storage::zip_archive::ZipArchive;

/// A memory-mapped or in-memory `.npz` archive.
pub struct Npz {
    archive: ZipArchive,
}

impl Npz {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            archive: ZipArchive::open(path)?,
        })
    }

    /// Opens an archive already loaded in memory, for browser and streamed assets.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        Ok(Self {
            archive: ZipArchive::from_bytes(bytes)?,
        })
    }

    /// Member names, as stored (NumPy keeps the `.npy` suffix).
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.archive.names()
    }

    /// Array names, with the `.npy` suffix stripped.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.names()
            .map(|name| name.strip_suffix(".npy").unwrap_or(name))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.archive.find(|member| matches(member, name)).is_some()
    }

    /// Size of a member once decoded, without decoding it.
    pub fn uncompressed_size(&self, name: &str) -> Option<u64> {
        self.archive
            .find(|member| matches(member, name))
            .and_then(|entry| self.archive.uncompressed_size(&entry.name))
    }

    /// Decodes one member. The `.npy` suffix is optional.
    pub fn array(&self, name: &str) -> Result<NpyArray> {
        let entry = self
            .archive
            .find(|member| matches(member, name))
            .with_context(|| format!("archive has no member {name:?}"))?;
        let bytes = self.archive.entry_bytes(entry)?;
        npy_array::parse(&bytes).with_context(|| format!("reading array {name:?}"))
    }
}

fn matches(member: &str, name: &str) -> bool {
    member == name || member.strip_suffix(".npy") == Some(name)
}
