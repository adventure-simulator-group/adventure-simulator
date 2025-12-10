use bevy::tasks::IoTaskPool;
use lightyear::prelude::*;

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

impl From<&WebTransportCertificateSettings> for Identity {
    fn from(wt: &WebTransportCertificateSettings) -> Identity {
        match wt {
            WebTransportCertificateSettings::AutoSelfSigned(sans) => {
                // In addition to and Subject Alternate Names (SAN) added via the config,
                // we add the public ip and domain for edgegap, if detected, and also
                // any extra values specified via the SELF_SIGNED_SANS environment variable.
                let mut sans = sans.clone();
                // Are we running on edgegap?
                // TODO: remove `std::env::var`
                if let Ok(public_ip) = std::env::var("ARBITRIUM_PUBLIC_IP") {
                    println!("🔐 SAN += ARBITRIUM_PUBLIC_IP: {public_ip}");
                    sans.push(public_ip);
                    sans.push("*.pr.edgegap.net".to_string());
                }
                // generic env to add domains and ips to SAN list:
                // SELF_SIGNED_SANS="example.org,example.com,127.1.1.1"
                // TODO: remove `std::env::var`
                if let Ok(san) = std::env::var("SELF_SIGNED_SANS") {
                    println!("🔐 SAN += SELF_SIGNED_SANS: {san}");
                    sans.extend(san.split(',').map(|s| s.to_string()));
                }
                println!("🔐 Generating self-signed certificate with SANs: {sans:?}");
                let identity = Identity::self_signed(sans).unwrap();
                let digest = identity.certificate_chain().as_slice()[0].hash();
                println!("🔐 Certificate digest: {digest}");
                identity
            }
            WebTransportCertificateSettings::FromFile {
                cert: cert_pem_path,
                key: private_key_pem_path,
            } => {
                println!(
                    "Reading certificate PEM files:\n * cert: {cert_pem_path}\n * key: {private_key_pem_path}",
                );
                // this is async because we need to load the certificate from io
                // we need async_compat because wtransport expects a tokio reactor
                let identity = IoTaskPool::get()
                    .scope(|s| {
                        s.spawn(async_compat::Compat::new(async {
                            Identity::load_pemfiles(cert_pem_path, private_key_pem_path)
                                .await
                                .unwrap()
                        }));
                    })
                    .pop()
                    .unwrap();
                let digest = identity.certificate_chain().as_slice()[0].hash();
                println!("🔐 Certificate digest: {digest}");
                identity
            }
        }
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
