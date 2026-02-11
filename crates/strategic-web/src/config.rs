//! Configuration for strategic-web server

use clap::Parser;

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
    #[arg(long, env = "SPACETIMEDB_DATABASE", default_value = "strategic-db")]
    pub spacetimedb_database: String,

    /// SpacetimeDB auth token (optional)
    #[arg(long, env = "SPACETIMEDB_TOKEN")]
    pub spacetimedb_token: Option<String>,

    /// Address to bind the web server to
    #[arg(long, env = "BIND_ADDRESS", default_value = "0.0.0.0:8080")]
    pub bind_address: String,

    /// Path to static files directory
    #[arg(long, env = "STATIC_DIR", default_value = "static")]
    pub static_dir: String,

    /// Edgegap API base URL
    #[arg(
        long,
        env = "EDGEGAP_API_URL",
        default_value = "https://api.edgegap.com"
    )]
    pub edgegap_api_url: String,

    /// Edgegap API token (enables production deployment API path)
    #[arg(long, env = "EDGEGAP_API_TOKEN")]
    pub edgegap_api_token: Option<String>,

    /// Edgegap application name for tactical server deployments
    #[arg(
        long,
        env = "EDGEGAP_APPLICATION_NAME",
        default_value = "tactical-server"
    )]
    pub edgegap_application_name: String,

    /// Edgegap application version name
    #[arg(long, env = "EDGEGAP_VERSION_NAME", default_value = "latest")]
    pub edgegap_version_name: String,
}
