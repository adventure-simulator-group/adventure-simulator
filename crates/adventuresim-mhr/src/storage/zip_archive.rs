use std::borrow::Cow;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};
use flate2::read::DeflateDecoder;
use memmap2::Mmap;

enum ArchiveData {
    Mapped(Mmap),
    Owned(Vec<u8>),
}

impl AsRef<[u8]> for ArchiveData {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Mapped(data) => data,
            Self::Owned(data) => data,
        }
    }
}

const END_OF_CENTRAL_DIRECTORY: u32 = 0x0605_4b50;
const CENTRAL_FILE_HEADER: u32 = 0x0201_4b50;
const LOCAL_FILE_HEADER: u32 = 0x0403_4b50;
const ZIP64_END_OF_CENTRAL_DIRECTORY: u32 = 0x0606_4b50;
const ZIP64_LOCATOR: u32 = 0x0706_4b50;

pub(crate) struct Entry {
    pub(crate) name: String,
    pub(crate) compression: u16,
    pub(crate) compressed_size: u64,
    pub(crate) uncompressed_size: u64,
    pub(crate) local_header_offset: u64,
}

/// A read-only zip archive reader (memory-mapped or owned).
pub struct ZipArchive {
    data: ArchiveData,
    entries: Vec<Entry>,
}

fn u16_at(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap())
}

fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

fn u64_at(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap())
}

/// Replaces saturated 32-bit sizes/offsets with their zip64 values.
fn apply_zip64_extra(extra: &[u8], entry: &mut Entry) {
    let mut offset = 0;
    while offset + 4 <= extra.len() {
        let id = u16_at(extra, offset);
        let size = u16_at(extra, offset + 2) as usize;
        let body = offset + 4;
        if body + size > extra.len() {
            return;
        }
        if id == 0x0001 {
            let mut cursor = body;
            for slot in [
                &mut entry.uncompressed_size,
                &mut entry.compressed_size,
                &mut entry.local_header_offset,
            ] {
                if *slot == u32::MAX as u64 && cursor + 8 <= body + size {
                    *slot = u64_at(extra, cursor);
                    cursor += 8;
                }
            }
            return;
        }
        offset = body + size;
    }
}

/// Locates the central directory, following the zip64 locator when present.
fn find_central_directory(data: &[u8]) -> Result<(u64, u64)> {
    let earliest = data.len().saturating_sub(66_000);
    let mut eocd = None;
    for offset in (earliest..data.len().saturating_sub(21)).rev() {
        if u32_at(data, offset) == END_OF_CENTRAL_DIRECTORY {
            eocd = Some(offset);
            break;
        }
    }
    let eocd = eocd.context("not a zip archive: no end-of-central-directory record")?;
    let mut count = u16_at(data, eocd + 10) as u64;
    let mut offset = u32_at(data, eocd + 16) as u64;

    if (count == u16::MAX as u64 || offset == u32::MAX as u64) && eocd >= 20 {
        let locator = eocd - 20;
        if u32_at(data, locator) == ZIP64_LOCATOR {
            let record = u64_at(data, locator + 8) as usize;
            if record + 56 <= data.len() && u32_at(data, record) == ZIP64_END_OF_CENTRAL_DIRECTORY {
                count = u64_at(data, record + 32);
                offset = u64_at(data, record + 48);
            }
        }
    }
    Ok((count, offset))
}

impl ZipArchive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file =
            std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
        let data = unsafe { Mmap::map(&file) }
            .with_context(|| format!("memory-mapping {}", path.display()))?;
        Self::from_data(ArchiveData::Mapped(data))
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        Self::from_data(ArchiveData::Owned(data))
    }

    fn from_data(data: ArchiveData) -> Result<Self> {
        let (count, mut offset) = find_central_directory(data.as_ref())?;
        let mut entries = Vec::with_capacity(count.min(4096) as usize);
        for _ in 0..count {
            let base = offset as usize;
            if base + 46 > data.as_ref().len() || u32_at(data.as_ref(), base) != CENTRAL_FILE_HEADER {
                break;
            }
            let name_len = u16_at(data.as_ref(), base + 28) as usize;
            let extra_len = u16_at(data.as_ref(), base + 30) as usize;
            let comment_len = u16_at(data.as_ref(), base + 32) as usize;
            let mut entry = Entry {
                name: String::from_utf8_lossy(&data.as_ref()[base + 46..base + 46 + name_len]).into_owned(),
                compression: u16_at(data.as_ref(), base + 10),
                compressed_size: u32_at(data.as_ref(), base + 20) as u64,
                uncompressed_size: u32_at(data.as_ref(), base + 24) as u64,
                local_header_offset: u32_at(data.as_ref(), base + 42) as u64,
            };
            apply_zip64_extra(
                &data.as_ref()[base + 46 + name_len..base + 46 + name_len + extra_len],
                &mut entry,
            );
            entries.push(entry);
            offset += (46 + name_len + extra_len + comment_len) as u64;
        }

        Ok(Self { data, entries })
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|entry| entry.name.as_str())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.iter().any(|entry| entry.name == name)
    }

    pub fn uncompressed_size(&self, name: &str) -> Option<u64> {
        self.entries
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.uncompressed_size)
    }

    pub(crate) fn find(&self, predicate: impl Fn(&str) -> bool) -> Option<&Entry> {
        self.entries.iter().find(|entry| predicate(&entry.name))
    }

    pub fn bytes(&self, name: &str) -> Result<Cow<'_, [u8]>> {
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.name == name)
            .with_context(|| format!("archive has no member {name:?}"))?;
        self.entry_bytes(entry)
    }

    pub(crate) fn entry_bytes(&self, entry: &Entry) -> Result<Cow<'_, [u8]>> {
        let data = self.data.as_ref();
        let base = entry.local_header_offset as usize;
        if base + 30 > data.len() || u32_at(data, base) != LOCAL_FILE_HEADER {
            bail!("corrupt zip entry {}", entry.name);
        }
        let name_len = u16_at(data, base + 26) as usize;
        let extra_len = u16_at(data, base + 28) as usize;
        let start = base + 30 + name_len + extra_len;
        let end = start + entry.compressed_size as usize;
        if end > data.len() {
            bail!("zip entry {} runs past the end of the archive", entry.name);
        }
        let raw = &data[start..end];

        match entry.compression {
            0 => Ok(Cow::Borrowed(raw)),
            8 => {
                let mut out = Vec::with_capacity(entry.uncompressed_size as usize);
                DeflateDecoder::new(raw)
                    .read_to_end(&mut out)
                    .with_context(|| format!("inflating zip entry {}", entry.name))?;
                Ok(Cow::Owned(out))
            }
            other => bail!("unsupported zip compression method {other}"),
        }
    }
}
