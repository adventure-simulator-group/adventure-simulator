//! Strategic Layer Web Server
//!
//! An SSR, HATEOAS-style web UI for the Adventure Simulator strategic layer.
//! Uses Axum + Maud + Datastar with SpacetimeDB as the backend.

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

    let cache_path = request.uri().path().to_owned();
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
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control(&cache_path)),
    );
    response
}

fn cache_control(path: &str) -> &'static str {
    let immutable_map = path
        .strip_prefix("/tactical/map/map-")
        .and_then(|suffix| suffix.strip_suffix(".json"))
        .is_some_and(valid_content_hash)
        || path
            .strip_prefix("/tactical/map/paper-map-")
            .and_then(|suffix| suffix.strip_suffix(".svg"))
            .is_some_and(valid_content_hash);
    if immutable_map {
        "public, max-age=31536000, immutable"
    } else if path.starts_with("/static/") || path.starts_with("/tactical/wasm/") {
        "public, max-age=3600"
    } else {
        // Pages and APIs may contain session-derived character/party information.
        "private, no-store"
    }
}

fn valid_content_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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

#[cfg(test)]
mod cache_tests {
    use super::cache_control;

    #[test]
    fn only_exact_content_addressed_map_artifacts_are_immutable() {
        let hash = "a".repeat(64);
        assert_eq!(
            cache_control(&format!("/tactical/map/map-{hash}.json")),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(
            cache_control(&format!("/tactical/map/paper-map-{hash}.svg")),
            "public, max-age=31536000, immutable"
        );
        for path in [
            "/tactical/map/manifest.json",
            "/tactical/map/map-short.json",
            "/tactical/map/map-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.json/extra",
            "/locations/settlement/demo/inn",
            "/api/settlements/demo/service-quests",
        ] {
            assert_eq!(cache_control(path), "private, no-store", "{path}");
        }
        assert_eq!(
            cache_control("/static/strategic-renderer.js"),
            "public, max-age=3600"
        );
    }

    #[test]
    fn strategic_loader_matches_build_output_and_preserves_fallback_semantics() {
        let loader = include_str!("../static/strategic-renderer.js");
        let tactical = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../adventuresim-stdb-module/static/tactical.html"
        ));
        let build = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/build_wasm.sh"
        ));
        for source in [loader, tactical] {
            assert!(source.contains("adventuresim-tactical-client.js"));
        }
        assert!(build.contains("adventuresim-tactical-client.wasm"));
        assert!(loader.contains("fetch(config.startup.package_url"));
        assert!(loader.contains("fetch(manifest.package_url"));
        assert!(loader.contains("wasm_set_suspended(document.hidden)"));
        assert!(!loader.contains("querySelector('[data-renderer-fallback]').hidden = true"));
    }
}
