//! Strategic Layer Web Server
//!
//! An SSR, HATEOAS-style web UI for the Adventure Simulator strategic layer.
//! Uses Axum + Maud + Datastar with SpacetimeDB as the backend.

mod auth;
mod config;
mod edgegap;
mod routes;
mod session;
mod spacetimedb;
mod templates;

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::Config;
use edgegap::EdgegapClient;
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

    tracing::info!("Starting strategic-web server on {}", config.bind_address);
    tracing::info!(
        "Connecting to SpacetimeDB at {} (database: {})",
        config.spacetimedb_host,
        config.spacetimedb_database
    );

    // Create SpacetimeDB client
    let db = SpacetimeClient::new(&config.spacetimedb_host, &config.spacetimedb_database)
        .with_token(config.spacetimedb_token.clone());

    // Create Edgegap client if configured
    let edgegap = if let Some(token) = &config.edgegap_api_token {
        tracing::info!(
            "Edgegap deployment API enabled: {} (app={}, version={})",
            config.edgegap_api_url,
            config.edgegap_application_name,
            config.edgegap_version_name
        );
        Some(EdgegapClient::new(
            &config.edgegap_api_url,
            token,
            &config.edgegap_application_name,
            &config.edgegap_version_name,
        ))
    } else {
        tracing::info!("Edgegap API token not configured, using local tactical-spawner flow");
        None
    };

    // Create app state
    let state = AppState {
        db,
        edgegap,
        spacetimedb_host: config.spacetimedb_host.clone(),
        spacetimedb_database: config.spacetimedb_database.clone(),
    };

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

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}

async fn health_check() -> &'static str {
    "OK"
}
