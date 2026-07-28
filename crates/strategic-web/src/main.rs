//! Strategic Layer Web Server
//!
//! An SSR, HATEOAS-style web UI for the Adventure Simulator strategic layer.
//! Uses Axum + Maud + Datastar with SpacetimeDB as the backend.

mod config;
mod live;
mod medical;
mod routes;
mod session;
mod spacetimedb;
mod strategic_map;
mod templates;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use axum::{
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{HeaderName, HeaderValue, Method, StatusCode, Uri, header},
    middleware::Next,
    response::Response,
};
use clap::Parser;
use tower::ServiceExt;
use tower_http::{compression::CompressionLayer, services::ServeDir};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::Config;
use live::LiveState;
use routes::{AppState, build_router};
use spacetimedb::SpacetimeClient;

fn sats_option_string(value: Option<&str>) -> serde_json::Value {
    match value {
        Some(value) => serde_json::json!({ "some": value }),
        None => serde_json::json!({ "none": [] }),
    }
}

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
    let measurement_baseline = db.query_metrics();
    let measurement_delta = measurement_baseline.delta(measurement_baseline);
    tracing::debug!(
        ?measurement_baseline,
        ?measurement_delta,
        "strategic SQL measurement baseline"
    );
    if config
        .spacetimedb_token
        .as_deref()
        .is_none_or(|token| token.trim().is_empty())
    {
        anyhow::bail!(
            "SPACETIMEDB_TOKEN is required: strategic travel reducers accept only the registered gateway identity"
        );
    }
    let live = LiveState::connect(
        &config.spacetimedb_host,
        &config.spacetimedb_database,
        config.spacetimedb_token.clone(),
    )?;

    // Create app state
    let assets = (|| -> anyhow::Result<_> {
        let map = strategic_map::StrategicMap::load(&config.strategic_map_bundle_dir)?;
        let pack = adventuresim_terrain::TerrainPack::load(
            &config
                .strategic_map_bundle_dir
                .join("terrain-routing-v3.json"),
            &config
                .strategic_map_bundle_dir
                .join("terrain-routing-v3.pack"),
        )?;
        map.validate_terrain_identity(&pack)?;
        let digest = pack.digest().to_string();
        Ok((
            std::sync::Arc::new(map),
            std::sync::Arc::new(routes::travel::TerrainPlanner::new(std::sync::Arc::new(
                pack,
            ))),
            digest,
        ))
    })();
    let (strategic_map, terrain) = match assets {
        Ok((map, terrain, digest)) => {
            tracing::info!(bundle=%config.strategic_map_bundle_dir.display(),%digest,"loaded coherent final strategic map and terrain bundle");
            (Some(map), Some(terrain))
        }
        Err(error) => {
            tracing::warn!(bundle=%config.strategic_map_bundle_dir.display(),%error,"strategic map and terrain bundle unavailable or incoherent; disabling both");
            (None, None)
        }
    };
    db.call(
        "register_strategic_gateway",
        &[
            sats_option_string(terrain.as_ref().map(|planner| planner.digest())),
            serde_json::json!(if terrain.is_some() { 3_u32 } else { 0_u32 }),
        ],
    )
    .await
    .map_err(|error| anyhow::anyhow!("could not register strategic gateway: {error}"))?;
    let state = AppState {
        db,
        live,
        strategic_map,
        terrain,
    };

    // Keep a middleware-free clone for rendering authoritative POST
    // destinations without issuing a second browser request.
    let navigation = StrategicNavigationMiddleware {
        renderer: build_router(state.clone()),
    };
    let app = build_router(state);

    // Add static file serving
    let static_path = PathBuf::from(&config.static_dir);
    let app = app.nest_service("/static", ServeDir::new(static_path));
    let tactical_static_path = PathBuf::from(&config.tactical_static_dir);
    let app = app.nest_service("/tactical", ServeDir::new(tactical_static_path));

    // Add health check before the outer request tracing layer so it is logged too.
    let app = app.route("/health", axum::routing::get(health_check));

    let app = app
        .layer(axum::middleware::from_fn_with_state(
            navigation,
            strategic_navigation_metadata,
        ))
        .layer(CompressionLayer::new())
        .layer(axum::middleware::from_fn(log_http_request));

    // Parse bind address
    let addr: SocketAddr = config
        .validated_bind_address()
        .map_err(anyhow::Error::msg)?;

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> &'static str {
    "OK"
}

#[derive(Clone)]
struct StrategicNavigationMiddleware {
    renderer: axum::Router,
}

async fn strategic_navigation_metadata(
    State(navigation): State<StrategicNavigationMiddleware>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let negotiated = request
        .headers()
        .get("x-strategic-navigation")
        .is_some_and(|value| value == "true");
    let canonical = request.uri().to_string();
    let current_url = request
        .headers()
        .get("x-strategic-current-url")
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.starts_with('/') && !value.starts_with("//") && !value.contains('\\'))
        .map(str::to_owned);
    let cookie = request.headers().get(header::COOKIE).cloned();
    let response = apply_strategic_navigation_metadata(
        &method,
        &canonical,
        negotiated,
        next.run(request).await,
    );
    if negotiated && method == Method::GET {
        strategic_root_fragment(response).await
    } else if negotiated && method == Method::POST && response.status().is_success() {
        negotiated_post_root(navigation, response, current_url.as_deref(), cookie).await
    } else {
        response
    }
}

fn strategic_hard_boundary(path: &str) -> bool {
    path.starts_with("/characters")
        || path.starts_with("/missions")
        || path.starts_with("/tactical")
        || path == "/map/data-license"
}

fn local_redirect_path(value: &str) -> Option<String> {
    (value.starts_with('/') && !value.starts_with("//") && !value.contains('\\'))
        .then(|| value.to_owned())
}

fn valid_hard_navigation_target(target: &str) -> bool {
    local_redirect_path(target).is_some()
        || target
            .parse::<reqwest::Url>()
            .is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

fn hard_navigation_response(target: &str, set_cookies: &[HeaderValue]) -> Response {
    if !valid_hard_navigation_target(target) {
        let mut response = Response::new(Body::from("Unsafe navigation target."));
        *response.status_mut() = StatusCode::BAD_REQUEST;
        append_set_cookies(&mut response, set_cookies);
        return response;
    }
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::NO_CONTENT;
    if let Ok(value) = HeaderValue::from_str(target) {
        response.headers_mut().insert(
            HeaderName::from_static("x-strategic-hard-navigation"),
            value,
        );
    }
    append_set_cookies(&mut response, set_cookies);
    response
}

fn response_set_cookies(response: &Response) -> Vec<HeaderValue> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .cloned()
        .collect()
}

fn append_set_cookies(response: &mut Response, cookies: &[HeaderValue]) {
    for cookie in cookies {
        response
            .headers_mut()
            .append(header::SET_COOKIE, cookie.clone());
    }
}

fn update_cookie_header(
    cookie: Option<HeaderValue>,
    set_cookies: &[HeaderValue],
) -> Option<HeaderValue> {
    let mut values = BTreeMap::<String, String>::new();
    if let Some(cookie) = cookie.and_then(|value| value.to_str().ok().map(str::to_owned)) {
        for pair in cookie.split(';') {
            if let Some((name, value)) = pair.trim().split_once('=') {
                values.insert(name.trim().to_owned(), value.trim().to_owned());
            }
        }
    }
    for set_cookie in set_cookies {
        let Ok(set_cookie) = set_cookie.to_str() else {
            continue;
        };
        let (pair, attributes) = set_cookie.split_once(';').unwrap_or((set_cookie, ""));
        let Some((name, value)) = pair.trim().split_once('=') else {
            continue;
        };
        let clears = value.is_empty()
            || attributes
                .to_ascii_lowercase()
                .split(';')
                .any(|attribute| attribute.trim() == "max-age=0");
        if clears {
            values.remove(name.trim());
        } else {
            values.insert(name.trim().to_owned(), value.trim().to_owned());
        }
    }
    HeaderValue::from_str(
        &values
            .into_iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; "),
    )
    .ok()
    .filter(|value| !value.is_empty())
}

async fn negotiated_post_root(
    navigation: StrategicNavigationMiddleware,
    response: Response,
    current_url: Option<&str>,
    cookie: Option<HeaderValue>,
) -> Response {
    let mut set_cookies = response_set_cookies(&response);
    let mut cookie = update_cookie_header(cookie, &set_cookies);
    let redirected = response
        .headers()
        .get("x-strategic-redirected")
        .is_some_and(|value| value == "true");
    let target = if redirected {
        let raw = response
            .headers()
            .get("x-strategic-canonical-url")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        match raw {
            Some(raw) => match local_redirect_path(&raw) {
                Some(local) => Some(local),
                None => return hard_navigation_response(&raw, &set_cookies),
            },
            None => None,
        }
    } else {
        current_url.and_then(local_redirect_path)
    };
    let Some(mut target) = target else {
        return response;
    };

    for _ in 0..5 {
        if strategic_hard_boundary(target.split(['?', '#']).next().unwrap_or(target.as_str())) {
            return hard_navigation_response(&target, &set_cookies);
        }
        let request_target = target.split('#').next().unwrap_or(target.as_str());
        let Ok(uri) = request_target.parse::<Uri>() else {
            return hard_navigation_response(&target, &set_cookies);
        };
        let mut request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("x-strategic-navigation-internal", "true");
        if let Some(cookie) = cookie.as_ref() {
            request = request.header(header::COOKIE, cookie.clone());
        }
        let rendered = navigation
            .renderer
            .clone()
            .oneshot(request.body(Body::empty()).expect("valid internal request"))
            .await
            .expect("router service is infallible");
        let next_set_cookies = response_set_cookies(&rendered);
        cookie = update_cookie_header(cookie, &next_set_cookies);
        set_cookies.extend(next_set_cookies);
        if rendered.status().is_redirection() {
            let Some(next) = rendered
                .headers()
                .get(header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .and_then(local_redirect_path)
            else {
                return hard_navigation_response(&target, &set_cookies);
            };
            target = next;
            continue;
        }
        let mut rendered = strategic_root_fragment(rendered).await;
        if !rendered.status().is_success()
            || !rendered
                .headers()
                .get("x-strategic-response")
                .is_some_and(|value| value == "root")
        {
            let mut error = Response::new(Body::from(
                "The updated strategic page could not be rendered.",
            ));
            *error.status_mut() = StatusCode::BAD_GATEWAY;
            append_set_cookies(&mut error, &set_cookies);
            return error;
        }
        *rendered.status_mut() = StatusCode::OK;
        rendered.headers_mut().insert(
            HeaderName::from_static("x-strategic-canonical-url"),
            HeaderValue::from_str(&target).expect("validated local target is a header"),
        );
        rendered.headers_mut().insert(
            HeaderName::from_static("x-strategic-response"),
            HeaderValue::from_static("root"),
        );
        if redirected {
            rendered.headers_mut().insert(
                HeaderName::from_static("x-strategic-redirected"),
                HeaderValue::from_static("true"),
            );
        }
        rendered.headers_mut().remove(header::SET_COOKIE);
        append_set_cookies(&mut rendered, &set_cookies);
        return rendered;
    }
    hard_navigation_response(&target, &set_cookies)
}

fn apply_strategic_navigation_metadata(
    method: &Method,
    canonical: &str,
    negotiated: bool,
    mut response: Response,
) -> Response {
    if negotiated {
        let mut response_kind =
            (*method == Method::POST && response.status().is_success()).then_some("mutation");
        if *method == Method::POST && response.status().is_redirection() {
            let location = response
                .headers()
                .get(header::LOCATION)
                .cloned()
                .unwrap_or_else(|| {
                    HeaderValue::from_str(&canonical).expect("request URI is valid")
                });
            *response.status_mut() = StatusCode::NO_CONTENT;
            response.headers_mut().remove(header::LOCATION);
            response.headers_mut().insert(
                HeaderName::from_static("x-strategic-canonical-url"),
                location,
            );
            response.headers_mut().insert(
                HeaderName::from_static("x-strategic-redirected"),
                HeaderValue::from_static("true"),
            );
            response_kind = Some("mutation");
        } else {
            response.headers_mut().insert(
                HeaderName::from_static("x-strategic-canonical-url"),
                HeaderValue::from_str(&canonical).expect("request URI is a valid header value"),
            );
        }
        if let Some(response_kind) = response_kind {
            response.headers_mut().insert(
                HeaderName::from_static("x-strategic-response"),
                HeaderValue::from_static(response_kind),
            );
        }
    }
    response
}

async fn strategic_root_fragment(response: Response) -> Response {
    const START: &str = "<!-- strategic-page-start -->";
    const END: &str = "<!-- strategic-page-end -->";
    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = to_bytes(body, 16 * 1024 * 1024).await else {
        return Response::from_parts(parts, Body::empty());
    };
    let Ok(document) = String::from_utf8(bytes.to_vec()) else {
        return Response::from_parts(parts, Body::from(bytes));
    };
    let Some(start) = document.find(START).map(|index| index + START.len()) else {
        return Response::from_parts(parts, Body::from(document));
    };
    let Some(end) = document[start..].find(END).map(|index| start + index) else {
        return Response::from_parts(parts, Body::from(document));
    };
    let fragment = &document[start..end];
    let profile = ["strategic", "live", "entry"]
        .into_iter()
        .find(|profile| fragment.contains(&format!("data-script-profile=\"{profile}\"")));
    parts.headers.remove(header::CONTENT_LENGTH);
    parts.headers.insert(
        HeaderName::from_static("x-strategic-response"),
        HeaderValue::from_static("root"),
    );
    if let Some(profile) = profile {
        parts.headers.insert(
            HeaderName::from_static("x-strategic-script-profile"),
            HeaderValue::from_static(profile),
        );
    }
    Response::from_parts(parts, Body::from(fragment.to_owned()))
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

#[cfg(test)]
mod strategic_navigation_contract_tests {
    use axum::{
        Router,
        body::Body,
        http::{HeaderMap, HeaderValue, Method, Response, StatusCode, header},
        response::Html,
        routing::get,
    };

    use super::{
        StrategicNavigationMiddleware, apply_strategic_navigation_metadata, local_redirect_path,
        negotiated_post_root, strategic_hard_boundary, strategic_root_fragment,
        update_cookie_header, valid_hard_navigation_target,
    };

    #[test]
    fn negotiated_posts_are_terminal_instead_of_redirecting_to_a_get() {
        let redirect = Response::builder()
            .status(StatusCode::SEE_OTHER)
            .header(header::LOCATION, "/camp?from=travel#party")
            .body(Body::empty())
            .unwrap();
        let response =
            apply_strategic_navigation_metadata(&Method::POST, "/camp/continue", true, redirect);
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(!response.headers().contains_key(header::LOCATION));
        assert_eq!(
            response.headers()["x-strategic-canonical-url"],
            "/camp?from=travel#party"
        );
        assert_eq!(response.headers()["x-strategic-response"], "mutation");
        assert_eq!(response.headers()["x-strategic-redirected"], "true");
    }

    #[test]
    fn ordinary_gets_are_not_marked_as_negotiated_fragments() {
        let response = Response::new(Body::empty());
        let response = apply_strategic_navigation_metadata(
            &Method::GET,
            "/locations/settlement/lubeck",
            false,
            response,
        );
        assert!(!response.headers().contains_key("x-strategic-response"));
        assert!(!response.headers().contains_key("x-strategic-canonical-url"));
    }

    #[test]
    fn negotiated_gets_include_canonical_metadata_before_body_rendering() {
        let response = apply_strategic_navigation_metadata(
            &Method::GET,
            "/locations/settlement/lubeck?building=inn",
            true,
            Response::new(Body::empty()),
        );
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()["x-strategic-canonical-url"],
            "/locations/settlement/lubeck?building=inn"
        );
        assert!(
            !response
                .headers()
                .contains_key("x-strategic-script-profile")
        );
        assert!(!response.headers().contains_key("x-strategic-response"));
    }

    #[tokio::test]
    async fn negotiated_get_body_contains_only_the_stable_root() {
        let document = concat!(
            "<!doctype html><head><title>Camp</title></head><body>",
            "<div id=\"strategic-live-stream\"></div>",
            "<!-- strategic-page-start --><div id=\"strategic-page\" ",
            "data-page-title=\"Camp\" data-script-profile=\"strategic\">Camp</div>",
            "<!-- strategic-page-end --></body>",
        );
        let response = strategic_root_fragment(Response::new(Body::from(document))).await;
        assert_eq!(response.headers()["x-strategic-response"], "root");
        assert_eq!(
            response.headers()["x-strategic-script-profile"],
            "strategic"
        );
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let body = std::str::from_utf8(&body).unwrap();
        assert!(body.starts_with("<div id=\"strategic-page\""));
        assert!(!body.contains("<!doctype"));
        assert!(!body.contains("strategic-live-stream"));
    }

    #[test]
    fn redirected_entry_and_tactical_targets_are_hard_boundaries() {
        for path in [
            "/characters",
            "/characters/new",
            "/missions/mission-1",
            "/tactical/tactical.html",
            "/map/data-license",
        ] {
            assert!(strategic_hard_boundary(path), "{path}");
        }
        assert!(!strategic_hard_boundary("/locations/settlement/lubeck"));
        assert_eq!(
            local_redirect_path("/camp?from=travel"),
            Some("/camp?from=travel".into())
        );
        assert_eq!(local_redirect_path("//example.test/steal"), None);
        assert_eq!(local_redirect_path("https://example.test/steal"), None);
    }

    #[tokio::test]
    async fn negotiated_post_renders_redirect_destination_in_the_same_response() {
        let renderer = Router::new().route(
            "/camp",
            get(|| async {
                Html(concat!(
                    "<!doctype html><body><!-- strategic-page-start -->",
                    "<div id=\"strategic-page\" data-page-title=\"Camp\" ",
                    "data-script-profile=\"strategic\">Fresh camp</div>",
                    "<!-- strategic-page-end --></body>",
                ))
            }),
        );
        let redirect = Response::builder()
            .status(StatusCode::SEE_OTHER)
            .header(header::LOCATION, "/camp")
            .body(Body::empty())
            .unwrap();
        let negotiated =
            apply_strategic_navigation_metadata(&Method::POST, "/camp/continue", true, redirect);
        let response = negotiated_post_root(
            StrategicNavigationMiddleware { renderer },
            negotiated,
            Some("/camp"),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["x-strategic-response"], "root");
        assert_eq!(
            response.headers()["x-strategic-script-profile"],
            "strategic"
        );
        assert_eq!(response.headers()["x-strategic-canonical-url"], "/camp");
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        assert!(std::str::from_utf8(&body).unwrap().contains("Fresh camp"));
    }

    #[tokio::test]
    async fn negotiated_post_keeps_entry_redirect_as_a_hard_boundary() {
        let redirect = Response::builder()
            .status(StatusCode::SEE_OTHER)
            .header(header::LOCATION, "/characters")
            .body(Body::empty())
            .unwrap();
        let negotiated =
            apply_strategic_navigation_metadata(&Method::POST, "/quests/accept", true, redirect);
        let response = negotiated_post_root(
            StrategicNavigationMiddleware {
                renderer: Router::new(),
            },
            negotiated,
            Some("/quests"),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            response.headers()["x-strategic-hard-navigation"],
            "/characters"
        );
    }

    #[tokio::test]
    async fn stale_selected_character_clear_reaches_internal_render_and_browser() {
        let renderer = Router::new().route(
            "/camp",
            get(|headers: HeaderMap| async move {
                let cookie = headers
                    .get(header::COOKIE)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("");
                assert!(!cookie.contains("character_id="));
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::SET_COOKIE, "session=fresh; Path=/; HttpOnly")
                    .body(Body::from(concat!(
                        "<!doctype html><body><!-- strategic-page-start -->",
                        "<div id=\"strategic-page\" data-page-title=\"Camp\" ",
                        "data-script-profile=\"strategic\">Fresh camp</div>",
                        "<!-- strategic-page-end --></body>",
                    )))
                    .unwrap()
            }),
        );
        let redirect = Response::builder()
            .status(StatusCode::SEE_OTHER)
            .header(header::LOCATION, "/camp")
            .header(
                header::SET_COOKIE,
                "character_id=; Max-Age=0; Path=/; HttpOnly",
            )
            .body(Body::empty())
            .unwrap();
        let negotiated =
            apply_strategic_navigation_metadata(&Method::POST, "/camp/continue", true, redirect);
        let response = negotiated_post_root(
            StrategicNavigationMiddleware { renderer },
            negotiated,
            Some("/camp"),
            Some(HeaderValue::from_static("character_id=stale; session=old")),
        )
        .await;
        let set_cookies = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .collect::<Vec<_>>();
        assert_eq!(set_cookies.len(), 2);
        assert!(
            set_cookies
                .iter()
                .any(|value| value.starts_with("character_id=;"))
        );
        assert!(
            set_cookies
                .iter()
                .any(|value| value.starts_with("session=fresh;"))
        );
    }

    #[test]
    fn cookie_replacements_and_hard_navigation_schemes_fail_closed() {
        let cookie = update_cookie_header(
            Some(HeaderValue::from_static("character_id=stale; session=old")),
            &[
                HeaderValue::from_static("character_id=; Max-Age=0; Path=/"),
                HeaderValue::from_static("session=fresh; Path=/"),
            ],
        )
        .unwrap();
        assert_eq!(cookie, "session=fresh");
        for target in [
            "/characters",
            "https://example.test/",
            "http://example.test/",
        ] {
            assert!(valid_hard_navigation_target(target), "{target}");
        }
        for target in [
            "javascript:alert(1)",
            "data:text/html,boom",
            "file:///tmp/secret",
            "//example.test/scheme-relative",
        ] {
            assert!(!valid_hard_navigation_target(target), "{target}");
        }
    }
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
mod tests {
    use super::sats_option_string;

    #[test]
    fn gateway_registration_uses_spacetimedb_option_sum_json() {
        assert_eq!(
            sats_option_string(Some("digest")),
            serde_json::json!({ "some": "digest" })
        );
        assert_eq!(sats_option_string(None), serde_json::json!({ "none": [] }));
    }
}
