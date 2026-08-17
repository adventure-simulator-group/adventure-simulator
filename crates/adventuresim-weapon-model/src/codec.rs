use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{GENERATOR_VERSION, SCHEMA_VERSION, ValidationError, WeaponDesign, validate};

#[derive(Serialize, Deserialize)]
struct Envelope {
    schema_version: u16,
    generator_version: u16,
    design: WeaponDesign,
}

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("weapon design transport is malformed: {0}")]
    Postcard(#[from] postcard::Error),
    #[error("unsupported weapon schema version {found}; expected {expected}")]
    SchemaVersion { found: u16, expected: u16 },
    #[error("unsupported weapon generator version {found}; expected {expected}")]
    GeneratorVersion { found: u16, expected: u16 },
    #[error("weapon design failed validation")]
    InvalidDesign(Vec<ValidationError>),
}

pub fn encode(design: &WeaponDesign) -> Result<Vec<u8>, CodecError> {
    validate(design).map_err(CodecError::InvalidDesign)?;
    Ok(postcard::to_allocvec(&Envelope {
        schema_version: SCHEMA_VERSION,
        generator_version: GENERATOR_VERSION,
        design: design.clone(),
    })?)
}

pub fn decode(bytes: &[u8]) -> Result<WeaponDesign, CodecError> {
    let envelope: Envelope = postcard::from_bytes(bytes)?;
    if envelope.schema_version != SCHEMA_VERSION {
        return Err(CodecError::SchemaVersion {
            found: envelope.schema_version,
            expected: SCHEMA_VERSION,
        });
    }
    if envelope.generator_version != GENERATOR_VERSION {
        return Err(CodecError::GeneratorVersion {
            found: envelope.generator_version,
            expected: GENERATOR_VERSION,
        });
    }
    validate(&envelope.design).map_err(CodecError::InvalidDesign)?;
    Ok(envelope.design)
}
