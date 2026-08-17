use std::io::Read;

use anyhow::{Context, Result, anyhow, bail};
use flate2::read::ZlibDecoder;

use crate::{Node, Prop};

pub(crate) struct Reader<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) pos: usize,
    pub(crate) version: u32,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(data: &'a [u8], pos: usize, version: u32) -> Self {
        Self { data, pos, version }
    }

    pub(crate) fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| anyhow!("FBX read overflow"))?;
        if end > self.data.len() {
            bail!(
                "truncated FBX file: wanted {len} bytes at offset {}",
                self.pos
            );
        }
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    pub(crate) fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub(crate) fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    /// Node headers went 32-bit -> 64-bit in FBX 7500.
    pub(crate) fn header_word(&mut self) -> Result<u64> {
        if self.version >= 7500 {
            self.u64()
        } else {
            Ok(self.u32()? as u64)
        }
    }

    pub(crate) fn header_size(&self) -> usize {
        // 3 header words + the 1-byte name length.
        if self.version >= 7500 { 25 } else { 13 }
    }
}

pub(crate) fn decode_array<T: Copy>(
    raw: &[u8],
    count: usize,
    encoding: u32,
    parse: impl Fn(&[u8]) -> T,
    width: usize,
) -> Result<Vec<T>> {
    let bytes = if encoding == 0 {
        raw.to_vec()
    } else {
        let mut out = Vec::with_capacity(count * width);
        ZlibDecoder::new(raw)
            .read_to_end(&mut out)
            .context("inflating FBX array property")?;
        out
    };
    if bytes.len() < count * width {
        bail!(
            "FBX array property is short: {} bytes for {count} x {width}",
            bytes.len()
        );
    }
    Ok((0..count)
        .map(|i| parse(&bytes[i * width..(i + 1) * width]))
        .collect())
}

pub(crate) fn read_props(reader: &mut Reader<'_>, count: usize) -> Result<Vec<Prop>> {
    let mut props = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = reader.u8()?;
        let prop = match kind {
            b'Y' => Prop::I16(i16::from_le_bytes(reader.take(2)?.try_into().unwrap())),
            b'C' => Prop::Bool(reader.u8()? != 0),
            b'I' => Prop::I32(i32::from_le_bytes(reader.take(4)?.try_into().unwrap())),
            b'F' => Prop::F32(f32::from_le_bytes(reader.take(4)?.try_into().unwrap())),
            b'D' => Prop::F64(f64::from_le_bytes(reader.take(8)?.try_into().unwrap())),
            b'L' => Prop::I64(i64::from_le_bytes(reader.take(8)?.try_into().unwrap())),
            b'f' | b'd' | b'l' | b'i' | b'b' => {
                let count = reader.u32()? as usize;
                let encoding = reader.u32()?;
                let compressed_len = reader.u32()? as usize;
                let raw = reader.take(compressed_len)?;
                match kind {
                    b'f' => Prop::ArrF32(decode_array(
                        raw,
                        count,
                        encoding,
                        |b| f32::from_le_bytes(b.try_into().unwrap()),
                        4,
                    )?),
                    b'd' => Prop::ArrF64(decode_array(
                        raw,
                        count,
                        encoding,
                        |b| f64::from_le_bytes(b.try_into().unwrap()),
                        8,
                    )?),
                    b'i' => Prop::ArrI32(decode_array(
                        raw,
                        count,
                        encoding,
                        |b| i32::from_le_bytes(b.try_into().unwrap()),
                        4,
                    )?),
                    b'l' => Prop::ArrI64(decode_array(
                        raw,
                        count,
                        encoding,
                        |b| i64::from_le_bytes(b.try_into().unwrap()),
                        8,
                    )?),
                    _ => Prop::ArrBool(decode_array(raw, count, encoding, |b| b[0], 1)?),
                }
            }
            b'S' => {
                let len = reader.u32()? as usize;
                Prop::Str(reader.take(len)?.to_vec())
            }
            b'R' => {
                let len = reader.u32()? as usize;
                Prop::Raw(reader.take(len)?.to_vec())
            }
            other => bail!("unknown FBX property type {:?}", other as char),
        };
        props.push(prop);
    }
    Ok(props)
}

/// Reads one node record. Returns `None` for the null record that terminates a list.
pub(crate) fn read_node(reader: &mut Reader<'_>) -> Result<Option<Node>> {
    let end_offset = reader.header_word()? as usize;
    let num_props = reader.header_word()? as usize;
    let _prop_list_len = reader.header_word()?;
    let name_len = reader.u8()? as usize;
    let name = String::from_utf8_lossy(reader.take(name_len)?).into_owned();

    if end_offset == 0 {
        return Ok(None);
    }

    let props = read_props(reader, num_props)?;

    let mut children = Vec::new();
    let sentinel = reader.header_size();
    while reader.pos + sentinel <= end_offset {
        match read_node(reader)? {
            Some(child) => children.push(child),
            None => break,
        }
    }
    reader.pos = end_offset;

    Ok(Some(Node {
        name,
        props,
        children,
    }))
}

/// Parses the top-level node list of a binary FBX file.
pub fn parse(data: &[u8]) -> Result<Vec<Node>> {
    const MAGIC: &[u8] = b"Kaydara FBX Binary  \x00";
    if data.len() < 27 || &data[..MAGIC.len()] != MAGIC {
        // ASCII FBX is a different format sharing the extension. Saying so is
        // more use than "not a binary FBX file", because the fix is a re-export.
        if data.starts_with(b"; FBX") || data.starts_with(b"\xef\xbb\xbf; FBX") {
            bail!("this is an ASCII FBX file; re-export it as binary FBX");
        }
        bail!("not a binary FBX file");
    }
    let version = u32::from_le_bytes(data[23..27].try_into().unwrap());
    let mut reader = Reader::new(data, 27, version);

    let mut roots = Vec::new();
    while reader.pos + reader.header_size() <= data.len() {
        match read_node(&mut reader)? {
            Some(node) => roots.push(node),
            None => break,
        }
    }
    Ok(roots)
}
