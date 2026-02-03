//! Configuration for strategic-web server

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "strategic-web")]
#[command(about = "Strategic layer web server for Adventure Simulator")]
pub struct Config {
    /// SpacetimeDB host URL
    #[arg(long, env = "SPACETIMEDB_HOST", default_value = "http://localhost:3000")]
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
}
