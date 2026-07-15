//! Strategic Layer Web Server
//!
//! An SSR, HATEOAS-style web UI for the Adventure Simulator strategic layer.
//! Uses Axum + Maud + Datastar with SpacetimeDB as the backend.

mod auth;
mod config;
mod live;
mod routes;
mod session;
mod spacetimedb;
mod templates;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};
use clap::Parser;
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::Config;
use live::LiveState;
use routes::{AppState, build_router};
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
    let live = LiveState::connect(
        &config.spacetimedb_host,
        &config.spacetimedb_database,
        config.spacetimedb_token.clone(),
    )?;

    // Create app state
    let state = AppState { db, live };

    // Build router
    let app = build_router(state);

    // Add static file serving
    let static_path = PathBuf::from(&config.static_dir);
    let app = app.nest_service("/static", ServeDir::new(static_path));
    let tactical_static_path = PathBuf::from(&config.tactical_static_dir);
    let app = app.nest_service("/tactical", ServeDir::new(tactical_static_path));

    // Add health check before the outer request tracing layer so it is logged too.
    let app = app.route("/health", axum::routing::get(health_check));

    let app = app.layer(axum::middleware::from_fn(log_http_request));

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

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

async fn log_http_request(request: Request, next: Next) -> Response {
    let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let mut log = HttpRequestLog {
        request_id,
        method: request.method().to_string(),
        uri: request.uri().to_string(),
        started: Instant::now(),
        finished: false,
    };
    tracing::info!(request_id, method = %log.method, uri = %log.uri, "http request started");

    let mut response = next.run(request).await;
    tracing::info!(
        request_id,
        method = %log.method,
        uri = %log.uri,
        status = response.status().as_u16(),
        elapsed_ms = log.started.elapsed().as_millis() as u64,
        "http request finished"
    );
    log.finished = true;
    response.headers_mut().insert(
        HeaderName::from_static("x-request-id"),
        HeaderValue::from_str(&request_id.to_string()).expect("numeric request id is a header"),
    );
    response
}

struct HttpRequestLog {
    request_id: u64,
    method: String,
    uri: String,
    started: Instant,
    finished: bool,
}

impl Drop for HttpRequestLog {
    fn drop(&mut self) {
        if !self.finished {
            tracing::warn!(
                request_id = self.request_id,
                method = %self.method,
                uri = %self.uri,
                elapsed_ms = self.started.elapsed().as_millis() as u64,
                "http request canceled before a response was produced"
            );
        }
    }
}
