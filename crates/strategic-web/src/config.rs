//! Configuration for strategic-web server

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(name = "strategic-web")]
#[command(about = "Strategic layer web server for Adventure Simulator")]
pub struct Config {
    /// SpacetimeDB host URL
    #[arg(
        long,
        env = "SPACETIMEDB_HOST",
        default_value = "http://localhost:3000"
    )]
    pub spacetimedb_host: String,

    /// SpacetimeDB database name
    #[arg(
        long,
        env = "SPACETIMEDB_DATABASE",
        default_value = "adventuresim-stdb-module"
    )]
    pub spacetimedb_database: String,

    /// SpacetimeDB auth token (optional)
    #[arg(long, env = "SPACETIMEDB_TOKEN")]
    pub spacetimedb_token: Option<String>,

    /// Address to bind the web server to. Loopback is the safe default because
    /// character selection is not user authentication.
    #[arg(long, env = "BIND_ADDRESS", default_value = "127.0.0.1:8080")]
    pub bind_address: String,

    /// Permit the anonymous single-user UI to listen on a non-loopback address.
    /// This is intentionally named as an insecure development escape hatch.
    #[arg(
        long,
        env = "ALLOW_INSECURE_NON_LOOPBACK_BIND",
        default_value_t = false
    )]
    pub allow_insecure_non_loopback_bind: bool,

    /// Path to static files directory
    #[arg(
        long,
        env = "STATIC_DIR",
        default_value = "crates/strategic-web/static"
    )]
    pub static_dir: String,

    /// Path to tactical client static files directory
    #[arg(
        long,
        env = "TACTICAL_STATIC_DIR",
        default_value = "crates/adventuresim-stdb-module/static"
    )]
    pub tactical_static_dir: String,

    /// Optional runtime strategic-map bundle directory. Absence does not prevent startup.
    #[arg(
        long,
        env = "STRATEGIC_MAP_BUNDLE_DIR",
        default_value = "target/strategic-map"
    )]
    pub strategic_map_bundle_dir: PathBuf,
}

impl Config {
    pub fn validated_bind_address(&self) -> Result<std::net::SocketAddr, String> {
        let address: std::net::SocketAddr = self
            .bind_address
            .parse()
            .map_err(|error| format!("invalid bind address {}: {error}", self.bind_address))?;
        if !address.ip().is_loopback() && !self.allow_insecure_non_loopback_bind {
            return Err(format!(
                "refusing non-loopback bind {address}: character selection is not authentication; pass --allow-insecure-non-loopback-bind only for an isolated development network"
            ));
        }
        Ok(address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(bind_address: &str, allow: bool) -> Config {
        Config {
            spacetimedb_host: String::new(),
            spacetimedb_database: String::new(),
            spacetimedb_token: None,
            bind_address: bind_address.into(),
            allow_insecure_non_loopback_bind: allow,
            static_dir: String::new(),
            tactical_static_dir: String::new(),
            strategic_map_bundle_dir: PathBuf::new(),
        }
    }

    #[test]
    fn loopback_is_allowed_without_an_escape_hatch() {
        assert!(
            config("127.0.0.1:8080", false)
                .validated_bind_address()
                .is_ok()
        );
        assert!(config("[::1]:8080", false).validated_bind_address().is_ok());
    }

    #[test]
    fn public_bind_requires_explicit_insecure_development_opt_in() {
        assert!(
            config("0.0.0.0:8080", false)
                .validated_bind_address()
                .is_err()
        );
        assert!(
            config("0.0.0.0:8080", true)
                .validated_bind_address()
                .is_ok()
        );
    }
}
