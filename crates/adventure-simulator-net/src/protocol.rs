use core::time::Duration;

use lightyear::netcode::PRIVATE_KEY_BYTES;
use serde::{de, Deserialize, Deserializer, Serialize};

pub const SEND_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrivateKey(pub [u8; PRIVATE_KEY_BYTES]);

#[derive(thiserror::Error, Debug)]
pub enum PrivateKeyParsingError {
    #[error("failed to parse number in private key: {0}")]
    ParseNumber(#[from] std::num::ParseIntError),
    #[error("private key must contain exactly {PRIVATE_KEY_BYTES} numbers, but it only has {0}")]
    NotEnoughNumbers(usize),
}

impl std::str::FromStr for PrivateKey {
    type Err = PrivateKeyParsingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let private_key = s
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == ',')
            .collect::<String>()
            .split(',')
            .map(|s| -> Result<u8, _> { s.parse().map_err(PrivateKeyParsingError::ParseNumber) })
            .collect::<Result<Vec<u8>, _>>()?;

        if private_key.len() != PRIVATE_KEY_BYTES {
            return Err(PrivateKeyParsingError::NotEnoughNumbers(private_key.len()));
        }

        let mut bytes = [0u8; PRIVATE_KEY_BYTES];
        bytes.copy_from_slice(&private_key);
        Ok(Self(bytes))
    }
}

impl<'de> Deserialize<'de> for PrivateKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        std::str::FromStr::from_str(&s).map_err(de::Error::custom)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ProtocolSettings {
    /// An id to identify the protocol version
    pub id: u64,
    /// a 32-byte array to authenticate via the Netcode.io protocol
    pub private_key: PrivateKey,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WebTransportCertificateSettings {
    /// Generate a self-signed certificate, with given SANs list to add to the certifictate
    /// eg: ["example.com", "*.gameserver.example.org", "10.1.2.3", "::1"]
    AutoSelfSigned(Vec<String>),
    /// Load certificate pem files from disk
    FromFile {
        /// Path to cert .pem file
        cert: String,
        /// Path to private key .pem file
        key: String,
    },
}

impl Default for WebTransportCertificateSettings {
    fn default() -> Self {
        let sans = vec![
            "localhost".to_string(),
            "127.0.0.1".to_string(),
            "::1".to_string(),
        ];
        WebTransportCertificateSettings::AutoSelfSigned(sans)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_key_parsing() {
        let json = r#"
            "5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5"
        "#;
        let private_key = serde_json::from_str::<PrivateKey>(json);
        assert_eq!(private_key.unwrap(), PrivateKey([5; 32]));

        let json = r#"
            "1000,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0"
        "#;
        let private_key = serde_json::from_str::<PrivateKey>(json);
        assert!(private_key.is_err()); // error: number too large to fit in target type

        let json = r#"
            "0,0,0,0"
        "#;
        let private_key = serde_json::from_str::<PrivateKey>(json);
        assert!(private_key.is_err()); // error: not enough numbers
    }
}
