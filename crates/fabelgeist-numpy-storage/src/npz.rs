//! `.npz` archive reading (a zip of `.npy` members).
//!
//! `np.savez` writes stored members and `np.savez_compressed` writes deflated
//! ones; both are handled, as is zip64, which NumPy emits for large arrays. The
//! archive plumbing itself lives in [`crate::zip`].

use std::path::Path;

use anyhow::{Context, Result};

use crate::npy::{self, NpyArray};
use crate::zip::ZipArchive;

/// A memory-mapped `.npz` archive.
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
        npy::parse(&bytes).with_context(|| format!("reading array {name:?}"))
    }
}

fn matches(member: &str, name: &str) -> bool {
    member == name || member.strip_suffix(".npy") == Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const END_OF_CENTRAL_DIRECTORY: u32 = 0x0605_4b50;
    const CENTRAL_FILE_HEADER: u32 = 0x0201_4b50;
    const LOCAL_FILE_HEADER: u32 = 0x0403_4b50;

    /// Builds a minimal stored-member zip so the reader can be tested without
    /// depending on an external archiver.
    fn zip(members: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut directory = Vec::new();
        for (name, payload) in members {
            let offset = out.len() as u32;
            let crc = 0u32;
            out.extend_from_slice(&LOCAL_FILE_HEADER.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // version, flags, method, time, date
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.write_all(payload).unwrap();

            directory.extend_from_slice(&CENTRAL_FILE_HEADER.to_le_bytes());
            directory.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            directory.extend_from_slice(&crc.to_le_bytes());
            directory.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            directory.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            directory.extend_from_slice(&(name.len() as u16).to_le_bytes());
            directory.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            directory.extend_from_slice(&offset.to_le_bytes());
            directory.extend_from_slice(name.as_bytes());
        }

        let directory_offset = out.len() as u32;
        let directory_size = directory.len() as u32;
        out.extend_from_slice(&directory);
        out.extend_from_slice(&END_OF_CENTRAL_DIRECTORY.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&(members.len() as u16).to_le_bytes());
        out.extend_from_slice(&(members.len() as u16).to_le_bytes());
        out.extend_from_slice(&directory_size.to_le_bytes());
        out.extend_from_slice(&directory_offset.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }

    fn write_temp(bytes: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "fabelgeist-numpy-storage-{}.npz",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn reads_stored_members() {
        let payload: Vec<u8> = [1.0f32, 2.0, 3.0, 4.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let archive = zip(&[
            (
                "weights.npy",
                crate::npy::tests::encode("<f4", "2, 2", &payload),
            ),
            (
                "indices.npy",
                crate::npy::tests::encode(
                    "<i8",
                    "2,",
                    &[7i64, 9]
                        .iter()
                        .flat_map(|v| v.to_le_bytes())
                        .collect::<Vec<_>>(),
                ),
            ),
        ]);

        let path = write_temp(&archive);
        let npz = Npz::open(&path).unwrap();
        assert_eq!(npz.keys().collect::<Vec<_>>(), ["weights", "indices"]);
        assert!(npz.contains("weights"));
        assert!(npz.contains("weights.npy"));
        assert!(!npz.contains("missing"));

        let weights = npz.array("weights").unwrap();
        assert_eq!(weights.shape, [2, 2]);
        assert_eq!(weights.to_f32(), [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(npz.array("indices").unwrap().to_i64(), [7, 9]);

        let memory_npz = Npz::from_bytes(archive).unwrap();
        assert_eq!(
            memory_npz.array("weights").unwrap().to_f32(),
            [1.0, 2.0, 3.0, 4.0]
        );

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn reports_a_missing_member() {
        let archive = zip(&[("a.npy", crate::npy::tests::encode("<f4", "1,", &[0; 4]))]);
        let path = write_temp(&archive);
        let npz = Npz::open(&path).unwrap();
        assert!(npz.array("b").is_err());
        std::fs::remove_file(path).ok();
    }
}
