use std::{
    fs::{self, File},
    io::{BufReader, Read},
    path::Path,
};

use sha2::{Digest, Sha256};

use crate::{Error, Result};

pub(super) fn sha256_file(path: &Path) -> Result<String> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(super) fn sha256_json(path: &Path) -> Result<String> {
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path)?).map_err(|error| {
        Error::Validation(format!(
            "invalid source manifest {}: {error}",
            path.display()
        ))
    })?;
    let canonical = serde_json::to_vec(&value)?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}
