use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

use axum::{
    body::Body,
    http::{Response, StatusCode, header::CONTENT_TYPE},
};
use maud::{Markup, html};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::spacetimedb::Settlement;

const WIDTH: f64 = 1200.0;
const HEIGHT: f64 = 800.0;
const VIEW_ASPECT_RATIO: f64 = WIDTH / HEIGHT;
const DEFAULT_VIEW_WIDTH: f64 = 390.0;
const DEFAULT_VIEW_HEIGHT: f64 = DEFAULT_VIEW_WIDTH / VIEW_ASPECT_RATIO;
const ROUTE_MIN_VIEW_WIDTH: f64 = 90.0;
const ROUTE_MIN_VIEW_HEIGHT: f64 = ROUTE_MIN_VIEW_WIDTH / VIEW_ASPECT_RATIO;
const PACKAGE_JSON: &str = include_str!("../static/map/strategic-map-v1.json");
const TILE_PACK_BYTES: &[u8] = include_bytes!("../static/map/strategic-map-tiles-v1.pack");
pub(crate) const TILE_PATH_PREFIX: &str = "/map/tiles/";

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Package {
    schema: u32,
    year: i32,
    bounds: [f64; 4],
    source: Source,
    roads: Vec<Line>,
    water: Vec<Vec<[f64; 2]>>,
    elevation: ElevationLayer,
    forest: ForestLayer,
    tiles: TilePyramid,
    package_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TilePyramid {
    format: String,
    tile_size: u32,
    gutter: u8,
    max_zoom: u8,
    content_sha256: String,
    entries: Vec<TileEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TileEntry {
    theme: String,
    zoom: u8,
    x: u16,
    y: u16,
    offset: u64,
    length: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Source {
    name: String,
    version: String,
    url: String,
    license: String,
    files_sha256: std::collections::BTreeMap<String, String>,
    verification_status: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Line {
    kind: String,
    points: Vec<[f64; 2]>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LayerSource {
    name: String,
    version: String,
    url: String,
    license: String,
    file_count: usize,
    files_sha256: BTreeMap<String, String>,
    verification_status: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ElevationLayer {
    source: LayerSource,
    cells: Vec<ElevationCell>,
    contours: Vec<ElevationContour>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ElevationCell {
    bounds: [f64; 4],
    band_m: i16,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ElevationContour {
    elevation_m: i16,
    points: Vec<[f64; 2]>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ForestLayer {
    source: LayerSource,
    coverage: Vec<[f64; 4]>,
    regions: Vec<ForestRegion>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ForestRegion {
    bounds: [f64; 4],
    density: u8,
    kind: String,
}

static PACKAGE: OnceLock<Package> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq)]
struct ViewBox {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

fn centered_view_box(center: (f64, f64), width: f64, height: f64) -> ViewBox {
    ViewBox {
        x: (center.0 - width / 2.0).clamp(0.0, WIDTH - width),
        y: (center.1 - height / 2.0).clamp(0.0, HEIGHT - height),
        width,
        height,
    }
}

fn initial_view_box(origin: (f64, f64), destination: Option<(f64, f64)>) -> ViewBox {
    let Some(destination) = destination else {
        return centered_view_box(origin, DEFAULT_VIEW_WIDTH, DEFAULT_VIEW_HEIGHT);
    };

    let span_x = (destination.0 - origin.0).abs();
    let span_y = (destination.1 - origin.1).abs();
    let required_width = (span_x + 2.0 * (span_x * 0.12).max(18.0)).max(ROUTE_MIN_VIEW_WIDTH);
    let required_height = (span_y + 2.0 * (span_y * 0.12).max(12.0)).max(ROUTE_MIN_VIEW_HEIGHT);
    let width = required_width
        .max(required_height * VIEW_ASPECT_RATIO)
        .min(WIDTH);
    let height = width / VIEW_ASPECT_RATIO;
    centered_view_box(
        (
            (origin.0 + destination.0) / 2.0,
            (origin.1 + destination.1) / 2.0,
        ),
        width,
        height,
    )
}

fn package() -> &'static Package {
    PACKAGE.get_or_init(|| {
        let package: Package = serde_json::from_str(PACKAGE_JSON)
            .expect("generated strategic map package must be valid JSON");
        assert_eq!(
            package.schema, 2,
            "unsupported strategic map package schema"
        );
        let actual = package_json_digest(PACKAGE_JSON, &package.package_sha256);
        assert_eq!(
            actual, package.package_sha256,
            "strategic map package digest mismatch"
        );
        assert_eq!(package.tiles.format, "avif", "unsupported map tile format");
        assert!(package.tiles.tile_size.is_power_of_two());
        assert!(u32::from(package.tiles.gutter) < package.tiles.tile_size / 4);
        assert!(package.tiles.max_zoom <= 8);
        assert_eq!(
            format!("{:x}", Sha256::digest(TILE_PACK_BYTES)),
            package.tiles.content_sha256,
            "strategic map tile-pack digest mismatch"
        );
        assert!(package.tiles.entries.iter().all(|entry| {
            usize::try_from(entry.offset)
                .ok()
                .and_then(|start| start.checked_add(entry.length as usize))
                .is_some_and(|end| end <= TILE_PACK_BYTES.len())
        }));
        package
    })
}

pub(crate) fn is_current_world_tile(path: &str, query: Option<&str>) -> bool {
    tile_coordinates(path).is_some()
        && query.and_then(|value| value.strip_prefix("v="))
            == Some(package().tiles.content_sha256.as_str())
}

pub(crate) async fn world_tile(
    axum::extract::Path((theme, zoom, x, tile)): axum::extract::Path<(String, u8, u16, String)>,
) -> Response<Body> {
    let Some(y) = tile
        .strip_suffix(".avif")
        .and_then(|value| value.parse::<u16>().ok())
    else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .expect("static tile response");
    };
    let package = package();
    let Some(entry) =
        package.tiles.entries.iter().find(|entry| {
            entry.theme == theme && entry.zoom == zoom && entry.x == x && entry.y == y
        })
    else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .expect("static tile response");
    };
    let start = usize::try_from(entry.offset).ok();
    let end = start.and_then(|start| start.checked_add(entry.length as usize));
    let Some(bytes) = start
        .zip(end)
        .and_then(|(start, end)| TILE_PACK_BYTES.get(start..end))
    else {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty())
            .expect("static tile error response");
    };
    Response::builder()
        .header(CONTENT_TYPE, "image/avif")
        .body(Body::from(bytes))
        .expect("static tile response")
}

fn tile_coordinates(path: &str) -> Option<(&str, u8, u16, u16)> {
    let value = path.strip_prefix(TILE_PATH_PREFIX)?.strip_suffix(".avif")?;
    let mut parts = value.split('/');
    let theme = parts.next()?;
    let zoom = parts.next()?.parse().ok()?;
    let x = parts.next()?.parse().ok()?;
    let y = parts.next()?.parse().ok()?;
    if parts.next().is_some() || theme != "paper" {
        return None;
    }
    Some((theme, zoom, x, y))
}

fn tile_url(theme: &str, zoom: u8, x: u16, y: u16, version: &str) -> String {
    format!("{TILE_PATH_PREFIX}{theme}/{zoom}/{x}/{y}.avif?v={version}")
}

fn initial_tile_zoom(view_width: f64, max_zoom: u8) -> u8 {
    (768.0 / view_width)
        .log2()
        .ceil()
        .clamp(0.0, f64::from(max_zoom)) as u8
}

fn visible_tiles(view: ViewBox, tile_size: u32, zoom: u8) -> Vec<(u16, u16, f64, f64)> {
    let span = f64::from(tile_size) / f64::from(1_u32 << zoom);
    let max_x = (WIDTH / span).ceil() as i32 - 1;
    let max_y = (HEIGHT / span).ceil() as i32 - 1;
    let start_x = (view.x / span).floor() as i32;
    let end_x = ((view.x + view.width) / span).ceil() as i32 - 1;
    let start_y = (view.y / span).floor() as i32;
    let end_y = ((view.y + view.height) / span).ceil() as i32 - 1;
    let mut tiles = Vec::new();
    for y in start_y.max(0)..=end_y.min(max_y) {
        for x in start_x.max(0)..=end_x.min(max_x) {
            tiles.push((x as u16, y as u16, f64::from(x) * span, f64::from(y) * span));
        }
    }
    tiles
}

pub(crate) fn has_geographic_source(settlement: &Settlement) -> bool {
    settlement.source_node_id.is_some()
        && settlement.coord_x.is_finite()
        && settlement.coord_y.is_finite()
}

pub fn strategic_map(
    settlements: &[Settlement],
    current_id: &str,
    connected_ids: &BTreeSet<&str>,
    selected_id: Option<&str>,
    map_path: &str,
) -> Markup {
    let package = package();
    let current = settlements
        .iter()
        .find(|settlement| settlement.id == current_id);
    let (origin_x, origin_y) = current.map_or((WIDTH / 2.0, HEIGHT / 2.0), |settlement| {
        project(settlement.coord_x, settlement.coord_y, package.bounds)
    });
    let destination = selected_id
        .and_then(|selected_id| {
            settlements.iter().find(|settlement| {
                settlement.id == selected_id && has_geographic_source(settlement)
            })
        })
        .map(|settlement| project(settlement.coord_x, settlement.coord_y, package.bounds));
    let initial_view = initial_view_box((origin_x, origin_y), destination);
    let view_box = format!(
        "{:.2} {:.2} {:.2} {:.2}",
        initial_view.x, initial_view.y, initial_view.width, initial_view.height
    );
    let initial_pin_scale = initial_view.width / DEFAULT_VIEW_WIDTH;
    let initial_tile_zoom = initial_tile_zoom(initial_view.width, package.tiles.max_zoom);
    let initial_tiles = visible_tiles(initial_view, package.tiles.tile_size, initial_tile_zoom);
    let tile_span = f64::from(package.tiles.tile_size) / f64::from(1_u32 << initial_tile_zoom);
    let tile_gutter = f64::from(package.tiles.gutter) / f64::from(1_u32 << initial_tile_zoom);

    html! {
        section class="strategic-map" data-strategic-map data-map-theme="paper"
            data-origin-x=(format!("{origin_x:.3}")) data-origin-y=(format!("{origin_y:.3}"))
            data-map-tile-size=(package.tiles.tile_size) data-map-max-tile-zoom=(package.tiles.max_zoom)
            data-map-tile-gutter=(package.tiles.gutter)
            data-map-tile-version=(&package.tiles.content_sha256) data-map-tile-root=(TILE_PATH_PREFIX)
            aria-label=(format!("Map around {}", current.map_or("the current settlement", |item| item.name.as_str()))) {
            div class="strategic-map-toolbar" role="toolbar" aria-label="Map controls" {
                div class="strategic-map-zoom" {
                    button type="button" data-map-zoom="in" aria-label="Zoom map in" { "+" }
                    button type="button" data-map-zoom="out" aria-label="Zoom map out" { "−" }
                    button type="button" data-map-reset aria-label="Reset map view" { "Reset" }
                }
            }
            svg class="strategic-map-svg" data-map-svg viewBox=(view_box)
                aria-labelledby="strategic-map-title strategic-map-description" tabindex="0" {
                title id="strategic-map-title" { "Settlement road map" }
                desc id="strategic-map-description" { "Use arrow keys or drag to pan. Use plus and minus controls or the mouse wheel to zoom. Activate a settlement pin to inspect it." }
                g data-map-viewport {
                    g class="map-tile-layer" data-map-tile-layer aria-hidden="true" {
                        @for (x, y, left, top) in initial_tiles {
                            image x=(format!("{:.3}", left - tile_gutter)) y=(format!("{:.3}", top - tile_gutter))
                                width=(format!("{:.3}", tile_span + 2.0 * tile_gutter)) height=(format!("{:.3}", tile_span + 2.0 * tile_gutter))
                                preserveAspectRatio="none"
                                href=(tile_url("paper", initial_tile_zoom, x, y, &package.tiles.content_sha256)) {}
                        }
                    }
                    g class="map-overlay-layer" {
                        @for settlement in settlements.iter().filter(|settlement| has_geographic_source(settlement)) {
                            @let (x, y) = project(settlement.coord_x, settlement.coord_y, package.bounds);
                            @let is_current = settlement.id == current_id;
                            @let is_connected = connected_ids.contains(settlement.id.as_str());
                            @let is_selected = selected_id == Some(settlement.id.as_str());
                            @let label = if is_current { format!("{}, current settlement", settlement.name) } else if is_connected { format!("{}, direct route available", settlement.name) } else { format!("{}, no direct route", settlement.name) };
                            a href=(format!("{map_path}?destination={}", settlement.id))
                                class="map-pin-link" aria-label=(label) aria-current=[is_selected.then_some("true")]
                                data-map-pin data-settlement-id=(&settlement.id) data-connected=(is_connected) {
                                g class=(format!("map-pin{}{}{}", if is_current { " current" } else { "" }, if is_connected { " connected" } else { "" }, if is_selected { " selected" } else { "" }))
                                    transform=(format!("translate({x:.3} {y:.3})")) {
                                    g data-map-pin-symbol transform=(format!("scale({initial_pin_scale:.5})")) {
                                        @if is_selected { circle class="map-pin-selection" r="10" {} }
                                        path class="map-pin-shape" d="M0,-7 C4,-7 7,-4 7,0 C7,5 0,11 0,11 C0,11 -7,5 -7,0 C-7,-4 -4,-7 0,-7 Z" {}
                                        circle class="map-pin-center" r="2.3" {}
                                        @if is_current { path class="map-pin-current-mark" d="M-3,0 H3 M0,-3 V3" {} }
                                        @if is_selected { path class="map-pin-selected-mark" d="M-4,14 H4" {} }
                                        title { (&settlement.name) }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            p class="strategic-map-legend" {
                span class="map-legend-elevation" aria-hidden="true" {} "Higher ground"
                span class="map-legend-forest" aria-hidden="true" {} "Forest (partial)"
                span class="map-legend-pin connected" aria-hidden="true" {} "Direct route"
                span class="map-legend-pin" aria-hidden="true" {} "Other settlement"
                span class="map-legend-selected" aria-hidden="true" {} "Selected"
            }
            p class="strategic-map-attribution small-copy" {
                "Map data: " a href=(&package.source.url) { (&package.source.name) }
                " (" (&package.source.license) "), adapted for " (package.year) ". Package "
                code { (&package.package_sha256[..12]) }
                @if package.source.verification_status != "verified" { ". Source status: legacy sidecar without byte sizes (release-blocked)." }
                " Elevation: " a href=(&package.elevation.source.url) { (&package.elevation.source.name) }
                " (" (package.elevation.source.file_count) " tiles, generalized)."
                " Forest: " a href=(&package.forest.source.url) { (&package.forest.source.name) }
                " (" (package.forest.coverage.len()) " partial-coverage tiles)."
            }
        }
    }
}

pub fn strategic_map_unavailable(settlement_name: &str) -> Markup {
    html! {
        section class="strategic-map strategic-map-unavailable" role="status" aria-labelledby="map-unavailable-title" {
            h2 id="map-unavailable-title" { "Map data not initialized" }
            p { (settlement_name) " does not have imported geographic source data. Initialize and load the historical world before using settlement map pins or travel actions." }
        }
    }
}

#[cfg(test)]
fn package_digest(package: &Package) -> String {
    let mut unsigned = package.clone();
    unsigned.package_sha256 = "0".repeat(64);
    let bytes =
        serde_json::to_vec(&unsigned).expect("strategic map package identity is serializable");
    format!("{:x}", Sha256::digest(bytes))
}

fn package_json_digest(json: &str, expected: &str) -> String {
    assert_eq!(
        expected.len(),
        64,
        "strategic map digest must be lowercase SHA-256"
    );
    let marker = format!("\"package_sha256\":\"{expected}\"");
    assert_eq!(
        json.matches(&marker).count(),
        1,
        "strategic map digest field must occur exactly once"
    );
    let unsigned = json.trim_end_matches(['\r', '\n']).replace(
        &marker,
        &format!("\"package_sha256\":\"{}\"", "0".repeat(64)),
    );
    let bytes = unsigned.as_bytes();
    format!("{:x}", Sha256::digest(bytes))
}

fn project(longitude: f64, latitude: f64, [west, south, east, north]: [f64; 4]) -> (f64, f64) {
    let x = ((longitude - west) / (east - west) * WIDTH).clamp(0.0, WIDTH);
    let y = ((north - latitude) / (north - south) * HEIGHT).clamp(0.0, HEIGHT);
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settlement(id: &str, name: &str, longitude: f64, latitude: f64) -> Settlement {
        Settlement {
            id: id.into(),
            name: name.into(),
            coord_x: longitude,
            coord_y: latitude,
            population_level: 4,
            population_estimate: 1_000,
            category: crate::spacetimedb::SettlementCategory::Town,
            industries: adventuresim_world_schema::InferredIndustryProfile::new(vec![
                adventuresim_world_schema::IndustryEvidence::Fallback(
                    adventuresim_world_schema::FallbackIndustry::CroplandGrain,
                ),
            ])
            .unwrap(),
            scene_key: "hills".into(),
            religion_id: "western_church".into(),
            currency_id: "coin".into(),
            source_node_id: Some(1),
        }
    }

    #[test]
    fn projection_maps_bounds_and_keeps_north_at_top() {
        let bounds = [-10.0, 40.0, 30.0, 70.0];
        assert_eq!(project(-10.0, 70.0, bounds), (0.0, 0.0));
        assert_eq!(project(30.0, 40.0, bounds), (WIDTH, HEIGHT));
        assert!(project(10.0, 60.0, bounds).1 < project(10.0, 50.0, bounds).1);
    }

    #[test]
    fn selected_destination_uses_a_close_fitted_view() {
        let view = initial_view_box((600.0, 400.0), Some((606.0, 404.0)));

        assert_eq!(view.width, ROUTE_MIN_VIEW_WIDTH);
        assert_eq!(view.height, ROUTE_MIN_VIEW_HEIGHT);
        assert!(view.x <= 600.0 && view.x + view.width >= 606.0);
        assert!(view.y <= 400.0 && view.y + view.height >= 404.0);
    }

    #[test]
    fn selected_destination_frames_distant_endpoints_and_world_edges() {
        let view = initial_view_box((0.0, 0.0), Some((900.0, 600.0)));

        assert_eq!(view.x, 0.0);
        assert_eq!(view.y, 0.0);
        assert!(view.x + view.width >= 900.0);
        assert!(view.y + view.height >= 600.0);
        assert!((view.width / view.height - VIEW_ASPECT_RATIO).abs() < f64::EPSILON);
        assert!(view.width <= WIDTH && view.height <= HEIGHT);
    }

    #[test]
    fn map_without_a_destination_keeps_the_regional_default() {
        let view = initial_view_box((600.0, 400.0), None);

        assert_eq!(view.width, DEFAULT_VIEW_WIDTH);
        assert_eq!(view.height, DEFAULT_VIEW_HEIGHT);
        assert_eq!(view.x, 405.0);
        assert_eq!(view.y, 270.0);
    }

    #[test]
    fn tiled_map_has_accessible_controls_and_canonical_pin_links() {
        let mut source_less = settlement("demo", "Demo", 0.0, 0.0);
        source_less.source_node_id = None;
        let settlements = [
            settlement("origin", "Origin", 10.0, 53.0),
            settlement("near", "Nearby", 11.0, 53.2),
            settlement("far", "Far away", 20.0, 60.0),
            source_less,
        ];
        let connected = BTreeSet::from(["near"]);
        let markup = strategic_map(
            &settlements,
            "origin",
            &connected,
            Some("near"),
            "/locations/settlement/origin/map",
        )
        .into_string();

        assert!(markup.contains("data-map-theme=\"paper\""));
        assert!(!markup.contains("data-map-theme-choice"));
        assert!(markup.contains("aria-label=\"Zoom map in\""));
        assert!(markup.contains("tabindex=\"0\""));
        assert!(markup.contains("?destination=near"));
        assert!(markup.contains("Nearby, direct route available"));
        assert!(markup.contains("Far away, no direct route"));
        assert!(!markup.contains("?destination=demo"));
        assert!(!markup.contains("role=\"img\""));
        assert!(markup.contains("aria-current=\"true\""));
        assert!(markup.contains("map-pin-selection"));
        assert!(markup.contains("data-map-pin-symbol"));
        assert!(markup.contains("map-tile-layer"));
        assert!(markup.contains("map-overlay-layer"));
        assert!(markup.contains(TILE_PATH_PREFIX));
        assert!(markup.contains(&package().tiles.content_sha256));
        assert!(markup.contains("image"));
        assert!(!markup.contains("class=\"map-elevation"));
        assert!(
            markup.len() < 50_000,
            "dynamic map markup grew to {} bytes",
            markup.len()
        );
        assert!(markup.contains("Higher ground"));
        assert!(markup.contains("Forest (partial)"));
    }

    #[test]
    fn source_less_origin_has_explicit_unavailable_state() {
        let mut origin = settlement("demo", "Demo settlement", 0.0, 0.0);
        origin.source_node_id = None;
        assert!(!has_geographic_source(&origin));
        let markup = strategic_map_unavailable(&origin.name).into_string();
        assert!(markup.contains("Map data not initialized"));
        assert!(markup.contains("role=\"status\""));
        assert!(!markup.contains("data-map-pin"));
        assert!(!markup.contains("Begin journey"));
    }

    #[test]
    fn package_identity_changes_with_every_presentation_field() {
        let package = package();
        let original = package_digest(package);
        let mut changed = package.clone();
        changed.bounds[0] += 0.01;
        assert_ne!(original, package_digest(&changed));
        changed = package.clone();
        changed.source.license.push_str("-tampered");
        assert_ne!(original, package_digest(&changed));
        changed = package.clone();
        changed.elevation.cells[0].band_m += 50;
        assert_ne!(original, package_digest(&changed));
        changed = package.clone();
        changed.forest.regions[0].density = if changed.forest.regions[0].density == 1 {
            2
        } else {
            1
        };
        assert_ne!(original, package_digest(&changed));
    }

    #[test]
    fn base_map_tile_urls_are_content_versioned() {
        let markup = strategic_map(
            &[settlement("origin", "Origin", 10.0, 53.0)],
            "origin",
            &BTreeSet::new(),
            None,
            "/locations/settlement/origin/map",
        )
        .into_string();
        let package = package();
        let entry = package.tiles.entries.first().expect("generated tile index");
        let path = format!(
            "{TILE_PATH_PREFIX}{}/{}/{}/{}.avif",
            entry.theme, entry.zoom, entry.x, entry.y
        );
        assert!(markup.contains("data-map-tile-version"));
        assert!(markup.contains("data-map-tile-gutter=\"4\""));
        assert!(is_current_world_tile(
            &path,
            Some(&format!("v={}", package.tiles.content_sha256))
        ));
        assert!(!is_current_world_tile(&path, None));
        assert!(!is_current_world_tile(&path, Some("v=stale")));
        assert!(!is_current_world_tile("/map/tiles/paper/0/0/0.png", None));
        let start = usize::try_from(entry.offset).unwrap();
        let end = start + entry.length as usize;
        assert!(
            TILE_PACK_BYTES[start..end]
                .windows(8)
                .any(|bytes| bytes == b"ftypavif")
        );
    }

    #[test]
    fn initial_tiles_cover_the_view_at_a_bounded_zoom() {
        let view = ViewBox {
            x: 590.0,
            y: 390.0,
            width: 90.0,
            height: 60.0,
        };
        let zoom = initial_tile_zoom(view.width, 4);
        let tiles = visible_tiles(view, 512, zoom);

        assert_eq!(zoom, 4);
        assert!(!tiles.is_empty());
        assert!(tiles.len() <= 12);
        assert!(tiles.iter().all(|(_, _, x, y)| *x < WIDTH && *y < HEIGHT));
    }
}
