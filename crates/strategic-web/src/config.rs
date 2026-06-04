//! Configuration for strategic-web server

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "strategic-web")]
#[command(about = "Strategic layer web server for Adventure Simulator")]
pub struct Config {
    /// SQLite database URL.
    #[arg(long, env = "DATABASE_URL", default_value = "sqlite://adventuresim.db")]
    pub database_url: String,

    /// Address to bind the web server to
    #[arg(long, env = "BIND_ADDRESS", default_value = "0.0.0.0:8080")]
    pub bind_address: String,

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
        default_value = "crates/strategic-web/static/tactical"
    )]
    pub tactical_static_dir: String,

    /// Path to the tactical server binary.
    #[arg(
        long,
        env = "TACTICAL_SERVER_BIN",
        default_value = "target/debug/adventuresim-tactical-server"
    )]
    pub tactical_server_bin: String,

    /// Host/IP tactical server processes bind to.
    #[arg(long, env = "TACTICAL_BIND_HOST", default_value = "127.0.0.1")]
    pub tactical_bind_host: String,

    /// Host/IP browser clients use to connect to spawned tactical servers.
    #[arg(long, env = "TACTICAL_PUBLIC_HOST", default_value = "127.0.0.1")]
    pub tactical_public_host: String,

    /// Base URL tactical server processes use for internal callbacks.
    #[arg(
        long,
        env = "STRATEGIC_INTERNAL_URL",
        default_value = "http://127.0.0.1:8080"
    )]
    pub strategic_internal_url: String,
}
