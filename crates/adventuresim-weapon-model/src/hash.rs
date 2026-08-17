use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{GENERATOR_VERSION, SCHEMA_VERSION, WeaponDesign};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct DesignHash(pub [u8; 32]);

impl DesignHash {
    pub fn to_hex(self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

pub fn design_hash(design: &WeaponDesign) -> DesignHash {
    let mut hash = Sha256::new();
    hash.update(b"fabelgeist.weapon-design\0");
    hash.update(SCHEMA_VERSION.to_le_bytes());
    hash.update(GENERATOR_VERSION.to_le_bytes());
    hash.update(postcard::to_allocvec(design).expect("WeaponDesign is postcard-serializable"));
    DesignHash(hash.finalize().into())
}
