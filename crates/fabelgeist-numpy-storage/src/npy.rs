//! `.npy` array decoding.

use std::path::Path;

use anyhow::{Context, Result, bail};
use burn::tensor::{Device, Int, Tensor, TensorData};

/// The NumPy element types this crate decodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dtype {
    F32,
    F64,
    I32,
    I64,
    U8,
    Bool,
}

impl Dtype {
    fn parse(descr: &str) -> Result<Self> {
        Ok(match descr.trim_matches(['\'', '"']) {
            "<f4" | "=f4" | "f4" => Dtype::F32,
            "<f8" | "=f8" | "f8" => Dtype::F64,
            "<i4" | "=i4" | "i4" => Dtype::I32,
            "<i8" | "=i8" | "i8" => Dtype::I64,
            "|u1" | "u1" => Dtype::U8,
            "|b1" | "b1" => Dtype::Bool,
            other => bail!("unsupported NumPy dtype {other:?}"),
        })
    }

    pub fn size(self) -> usize {
        match self {
            Dtype::U8 | Dtype::Bool => 1,
            Dtype::F32 | Dtype::I32 => 4,
            Dtype::F64 | Dtype::I64 => 8,
        }
    }
}

/// One decoded array: shape, element type, and the raw little-endian payload.
pub struct NpyArray {
    pub shape: Vec<usize>,
    pub dtype: Dtype,
    pub(crate) bytes: Vec<u8>,
}

impl NpyArray {
    pub fn len(&self) -> usize {
        self.shape.iter().product()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Element bytes, exactly as stored.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Values widened to `f32`, whatever the stored element type.
    pub fn to_f32(&self) -> Vec<f32> {
        match self.dtype {
            Dtype::F32 => self.map_chunks(4, |b| f32::from_le_bytes(b.try_into().unwrap())),
            Dtype::F64 => self.map_chunks(8, |b| f64::from_le_bytes(b.try_into().unwrap()) as f32),
            Dtype::I32 => self.map_chunks(4, |b| i32::from_le_bytes(b.try_into().unwrap()) as f32),
            Dtype::I64 => self.map_chunks(8, |b| i64::from_le_bytes(b.try_into().unwrap()) as f32),
            Dtype::U8 | Dtype::Bool => self.bytes.iter().map(|b| *b as f32).collect(),
        }
    }

    /// Values widened to `i64`, whatever the stored element type.
    pub fn to_i64(&self) -> Vec<i64> {
        match self.dtype {
            Dtype::F32 => self.map_chunks(4, |b| f32::from_le_bytes(b.try_into().unwrap()) as i64),
            Dtype::F64 => self.map_chunks(8, |b| f64::from_le_bytes(b.try_into().unwrap()) as i64),
            Dtype::I32 => self.map_chunks(4, |b| i32::from_le_bytes(b.try_into().unwrap()) as i64),
            Dtype::I64 => self.map_chunks(8, |b| i64::from_le_bytes(b.try_into().unwrap())),
            Dtype::U8 | Dtype::Bool => self.bytes.iter().map(|b| *b as i64).collect(),
        }
    }

    pub fn to_bool(&self) -> Vec<bool> {
        self.bytes.iter().map(|b| *b != 0).collect()
    }

    fn map_chunks<T>(&self, width: usize, convert: impl Fn(&[u8]) -> T) -> Vec<T> {
        self.bytes.chunks_exact(width).map(convert).collect()
    }

    fn dims<const D: usize>(&self) -> Result<[usize; D]> {
        self.shape
            .as_slice()
            .try_into()
            .with_context(|| format!("array has {} dimensions, expected {D}", self.shape.len()))
    }

    /// Uploads the array to a device as a float tensor.
    pub fn to_tensor<const D: usize>(&self, device: &Device) -> Result<Tensor<D>> {
        Ok(Tensor::from_data(
            TensorData::new(self.to_f32(), self.dims::<D>()?),
            device,
        ))
    }

    /// Uploads the array to a device as an integer tensor.
    pub fn to_int_tensor<const D: usize>(&self, device: &Device) -> Result<Tensor<D, Int>> {
        Ok(Tensor::from_data(
            TensorData::new(self.to_i64(), self.dims::<D>()?),
            device,
        ))
    }
}

/// Parses a `.npy` buffer: header dictionary first, then the element payload.
pub fn parse(bytes: &[u8]) -> Result<NpyArray> {
    const MAGIC: &[u8] = b"\x93NUMPY";
    if bytes.len() < 10 || &bytes[..6] != MAGIC {
        bail!("not a .npy array");
    }
    let (header_len, header_start) = if bytes[6] == 1 {
        (
            u16::from_le_bytes(bytes[8..10].try_into().unwrap()) as usize,
            10,
        )
    } else {
        (
            u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize,
            12,
        )
    };
    if header_start + header_len > bytes.len() {
        bail!("truncated .npy header");
    }
    let header = std::str::from_utf8(&bytes[header_start..header_start + header_len])
        .context("non-UTF-8 .npy header")?;

    let dtype =
        Dtype::parse(header_value(header, "'descr'").context("missing 'descr' in .npy header")?)?;
    if header_value(header, "'fortran_order'").is_some_and(|v| v.starts_with("True")) {
        bail!("Fortran-ordered .npy arrays are not supported");
    }

    let shape_text = header
        .split_once("'shape'")
        .and_then(|(_, rest)| rest.split_once('('))
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(inside, _)| inside)
        .context("missing 'shape' in .npy header")?;
    let shape: Vec<usize> = shape_text
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| token.parse::<usize>().context("bad .npy shape"))
        .collect::<Result<_>>()?;

    let count: usize = shape.iter().product();
    let start = header_start + header_len;
    let end = start + count * dtype.size();
    if end > bytes.len() {
        bail!("truncated .npy payload");
    }

    Ok(NpyArray {
        shape,
        dtype,
        bytes: bytes[start..end].to_vec(),
    })
}

/// Reads a standalone `.npy` file.
pub fn read(path: impl AsRef<Path>) -> Result<NpyArray> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    parse(&bytes).with_context(|| format!("parsing {}", path.display()))
}

/// The scalar value following `key:` in the header dictionary, up to the next
/// comma. Only used for `descr` and `fortran_order`, neither of which is a
/// composite value.
fn header_value<'a>(header: &'a str, key: &str) -> Option<&'a str> {
    let (_, rest) = header.split_once(key)?;
    let rest = rest.trim_start().strip_prefix(':')?;
    Some(rest.split(',').next()?.trim())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn encode(dtype: &str, shape: &str, payload: &[u8]) -> Vec<u8> {
        let header =
            format!("{{'descr': '{dtype}', 'fortran_order': False, 'shape': ({shape}), }}");
        let mut padded = header.into_bytes();
        while (10 + padded.len()) % 64 != 63 {
            padded.push(b' ');
        }
        padded.push(b'\n');
        let mut out = b"\x93NUMPY\x01\x00".to_vec();
        out.extend_from_slice(&(padded.len() as u16).to_le_bytes());
        out.extend_from_slice(&padded);
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn parses_a_float_array() {
        let payload: Vec<u8> = [1.0f32, -2.5, 3.25]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let array = parse(&encode("<f4", "3,", &payload)).unwrap();
        assert_eq!(array.shape, [3]);
        assert_eq!(array.dtype, Dtype::F32);
        assert_eq!(array.to_f32(), [1.0, -2.5, 3.25]);
    }

    #[test]
    fn parses_a_two_dimensional_int_array() {
        let payload: Vec<u8> = [1i64, 2, 3, 4]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let array = parse(&encode("<i8", "2, 2", &payload)).unwrap();
        assert_eq!(array.shape, [2, 2]);
        assert_eq!(array.to_i64(), [1, 2, 3, 4]);
        assert_eq!(array.to_f32(), [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn parses_a_bool_array() {
        let array = parse(&encode("|b1", "4,", &[1, 0, 1, 1])).unwrap();
        assert_eq!(array.to_bool(), [true, false, true, true]);
    }

    #[test]
    fn parses_a_scalar_shape() {
        let array = parse(&encode("<f4", "", &1.5f32.to_le_bytes())).unwrap();
        assert!(array.shape.is_empty());
        assert_eq!(array.len(), 1);
    }

    #[test]
    fn rejects_fortran_order_and_bad_dtypes() {
        let mut header = b"{'descr': '<f4', 'fortran_order': True, 'shape': (1,), }\n".to_vec();
        let mut bytes = b"\x93NUMPY\x01\x00".to_vec();
        bytes.extend_from_slice(&(header.len() as u16).to_le_bytes());
        bytes.append(&mut header);
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        assert!(parse(&bytes).is_err());

        assert!(parse(&encode("<c8", "1,", &[0; 8])).is_err());
    }

    #[test]
    fn rejects_a_truncated_payload() {
        assert!(parse(&encode("<f4", "4,", &[0; 8])).is_err());
    }
}
