use bevy::prelude::*;
use lightyear::netcode::{CONNECT_TOKEN_BYTES, PRIVATE_KEY_BYTES};
use serde::{de, Deserialize, Deserializer, Serialize};

/// [`ConnectToken`] represented by hex numbers.
pub type HexConnectToken = HexToken<CONNECT_TOKEN_BYTES>;
/// Private [`Key`] represented by hex numbers.
pub type HexPrivateKey = HexToken<PRIVATE_KEY_BYTES>;

#[derive(thiserror::Error, Debug)]
pub enum HexTokenParsingError {
    #[error("failed to parse hex token: {0}")]
    HexError(#[from] hex::FromHexError),
    #[error("hex token must contain exactly {0} numbers, but it only has {1}")]
    InvalidLength(usize, usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deref, DerefMut)]
pub struct HexToken<const N: usize>([u8; N]);

impl<const N: usize> HexToken<N> {
    fn from_slice(bytes: &[u8]) -> Self {
        let mut token = [0; N];
        token.copy_from_slice(bytes);
        Self(token)
    }

    pub fn new(bytes: [u8; N]) -> Self {
        Self(bytes)
    }
}

impl<const N: usize> std::str::FromStr for HexToken<N> {
    type Err = HexTokenParsingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = hex::decode(s)?;
        if bytes.len() != N {
            return Err(HexTokenParsingError::InvalidLength(N, bytes.len()));
        }
        Ok(Self::from_slice(&bytes))
    }
}

impl<'de, const N: usize> Deserialize<'de> for HexToken<N> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        std::str::FromStr::from_str(&s).map_err(de::Error::custom)
    }
}

impl<const N: usize> Serialize for HexToken<N> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<const N: usize> ToString for HexToken<N> {
    fn to_string(&self) -> String {
        hex::encode(&self.0)
    }
}

impl<const N: usize> Default for HexToken<N> {
    fn default() -> Self {
        Self([0; N])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_key_parsing() {
        let json = r#"
            "0505050505050505050505050505050505050505050505050505050505050505"
        "#;
        let private_key = serde_json::from_str::<HexPrivateKey>(json);
        assert_eq!(private_key.unwrap(), HexPrivateKey::new([5; 32]));

        let json = r#"
            "xx595"
        "#;
        let private_key = serde_json::from_str::<HexPrivateKey>(json);
        assert!(private_key.is_err()); // error: not a valid hex number

        let json = r#"
            "deadbeef"
        "#;
        let private_key = serde_json::from_str::<HexPrivateKey>(json);
        assert!(private_key.is_err()); // error: not enough numbers
    }
}
