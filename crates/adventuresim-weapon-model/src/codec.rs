use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    GENERATOR_VERSION, HOLDER_GENERATOR_VERSION, HOLDER_SCHEMA_VERSION, SCHEMA_VERSION,
    ValidationError, WeaponDesign, WeaponHolderDesign, validate, validate_holder,
};

#[derive(Serialize, Deserialize)]
struct Envelope {
    schema_version: u16,
    generator_version: u16,
    design: WeaponDesign,
}

#[derive(Serialize, Deserialize)]
struct HolderEnvelope {
    schema_version: u16,
    generator_version: u16,
    design: WeaponHolderDesign,
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

pub fn encode_holder(design: &WeaponHolderDesign) -> Result<Vec<u8>, CodecError> {
    validate_holder(design).map_err(CodecError::InvalidDesign)?;
    Ok(postcard::to_allocvec(&HolderEnvelope {
        schema_version: HOLDER_SCHEMA_VERSION,
        generator_version: HOLDER_GENERATOR_VERSION,
        design: design.clone(),
    })?)
}

pub fn decode_holder(bytes: &[u8]) -> Result<WeaponHolderDesign, CodecError> {
    let envelope: HolderEnvelope = postcard::from_bytes(bytes)?;
    if envelope.schema_version != HOLDER_SCHEMA_VERSION {
        return Err(CodecError::SchemaVersion {
            found: envelope.schema_version,
            expected: HOLDER_SCHEMA_VERSION,
        });
    }
    if envelope.generator_version != HOLDER_GENERATOR_VERSION {
        return Err(CodecError::GeneratorVersion {
            found: envelope.generator_version,
            expected: HOLDER_GENERATOR_VERSION,
        });
    }
    validate_holder(&envelope.design).map_err(CodecError::InvalidDesign)?;
    Ok(envelope.design)
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
