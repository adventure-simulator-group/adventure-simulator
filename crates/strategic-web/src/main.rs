//! Strategic Layer Web Server
//!
//! An SSR, HATEOAS-style web UI for the Adventure Simulator strategic layer.
//! Uses Axum + Maud + Datastar with SpacetimeDB as the backend.

mod auth;
mod config;
mod routes;
mod session;
mod spacetimedb;
mod templates;

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::Router;
use clap::Parser;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::Config;
use routes::{build_router, AppState};
use spacetimedb::SpacetimeClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "strategic_web=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse config
    let config = Config::parse();

    tracing::info!(
        "Starting strategic-web server on {}",
        config.bind_address
    );
    tracing::info!(
        "Connecting to SpacetimeDB at {} (database: {})",
        config.spacetimedb_host,
        config.spacetimedb_database
    );

    // Create SpacetimeDB client
    let db = SpacetimeClient::new(&config.spacetimedb_host, &config.spacetimedb_database)
        .with_token(config.spacetimedb_token.clone());

    // Create app state
    let state = AppState { db };

    // Build router
    let app = build_router(state);

    // Add static file serving
    let static_path = PathBuf::from(&config.static_dir);
    let app = app.nest_service("/static", ServeDir::new(static_path));

    // Add tracing layer
    let app = app.layer(TraceLayer::new_for_http());

    // Add health check
    let app = app.route("/health", axum::routing::get(health_check));

    // Parse bind address
    let addr: SocketAddr = config.bind_address.parse()?;

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> &'static str {
    "OK"
}
